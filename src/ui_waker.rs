//! Wakes the UI thread when a background task has delivered a result.
//!
//! Protocol (coalesced wake):
//! * a sender puts its result into its channel first, then calls [`UiWaker::wake`];
//! * `wake` flips the shared `pending` flag and delivers one event only on the
//!   `false -> true` transition, so a burst of sends costs one event loop wake-up;
//! * the UI calls [`UiWaker::begin_drain`] BEFORE it walks the channels, never after:
//!   a send that lands during the walk then sees `pending == false` and wakes again;
//! * a UI walk that stops early (a limit, a skipped section) with messages left calls
//!   `wake` itself, so the rest is not stranded until an unrelated OS event.
//!
//! The UI applies the drained results and only then requests a redraw; the wake itself
//! never draws. A closed native event loop is ignored silently.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SendError, Sender, SyncSender, TryRecvError, TrySendError};

/// Payload-free user event of the native event loop: "some background result is waiting".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AppWake;

/// Cheap clonable handle; every clone shares one `pending` flag and one event sink.
#[derive(Clone)]
pub(crate) struct UiWaker {
    shared: Arc<WakerShared>,
}

struct WakerShared {
    pending: AtomicBool,
    sink: WakeSink,
}

enum WakeSink {
    Native(winit::event_loop::EventLoopProxy<AppWake>),
    /// Headless driver and tests: counts delivered events instead of posting them.
    Counter(AtomicU64),
}

impl UiWaker {
    pub(crate) fn native(proxy: winit::event_loop::EventLoopProxy<AppWake>) -> Self {
        Self::with_sink(WakeSink::Native(proxy))
    }

    /// Headless / test waker: events are counted, the driver reads them with `take_events`.
    /// Also used where no UI thread listens (synchronous helpers, probes).
    pub(crate) fn counting() -> Self {
        Self::with_sink(WakeSink::Counter(AtomicU64::new(0)))
    }

    fn with_sink(sink: WakeSink) -> Self {
        Self {
            shared: Arc::new(WakerShared {
                pending: AtomicBool::new(false),
                sink,
            }),
        }
    }

    /// Called by a background task after it has sent its result.
    pub(crate) fn wake(&self) {
        // AcqRel: the UI's `begin_drain` swap reads this write and so sees the send before it.
        if !self.shared.pending.swap(true, Ordering::AcqRel) {
            self.deliver();
        }
    }

    /// UI thread, right before walking the background channels.
    pub(crate) fn begin_drain(&self) {
        self.shared.pending.swap(false, Ordering::AcqRel);
    }

    /// UI thread, on an `about_to_wait` path that returns before draining: a wake that is
    /// still pending was consumed by this pass without its results being read, so post it again.
    pub(crate) fn redeliver_pending(&self) {
        if self.shared.pending.load(Ordering::Acquire) {
            self.deliver();
        }
    }

    fn deliver(&self) {
        match &self.shared.sink {
            // Err means the event loop is gone (shutdown): nobody is left to wake.
            WakeSink::Native(proxy) => {
                let _ = proxy.send_event(AppWake);
            }
            WakeSink::Counter(events) => {
                events.fetch_add(1, Ordering::AcqRel);
            }
        }
    }

    /// Counting waker: events delivered since the previous call. Always 0 for a native waker.
    pub(crate) fn take_events(&self) -> u64 {
        match &self.shared.sink {
            WakeSink::Native(_) => 0,
            WakeSink::Counter(events) => events.swap(0, Ordering::AcqRel),
        }
    }

    /// Headless `dump`: `take_events` without consuming.
    pub(crate) fn queued_events(&self) -> u64 {
        match &self.shared.sink {
            WakeSink::Native(_) => 0,
            WakeSink::Counter(events) => events.load(Ordering::Acquire),
        }
    }

    /// Headless `dump`: a wake was delivered and the UI has not begun draining it yet.
    pub(crate) fn is_pending(&self) -> bool {
        self.shared.pending.load(Ordering::Acquire)
    }

    pub(crate) fn channel<T>(&self) -> (WakeSender<T>, Receiver<T>) {
        let (tx, rx) = mpsc::channel();
        (self.sender(tx), rx)
    }

    pub(crate) fn sync_channel<T>(&self, bound: usize) -> (WakeSyncSender<T>, Receiver<T>) {
        let (tx, rx) = mpsc::sync_channel(bound);
        (
            WakeSyncSender {
                tx,
                waker: self.clone(),
            },
            rx,
        )
    }

    pub(crate) fn sender<T>(&self, tx: Sender<T>) -> WakeSender<T> {
        WakeSender {
            tx,
            waker: self.clone(),
        }
    }

    /// Runs `job` on a named thread; its result wakes the UI. Err = thread spawn failed (caller must show it).
    pub(crate) fn spawn_one_shot<T: Send + 'static>(
        &self,
        thread_name: &str,
        job: impl FnOnce() -> T + Send + 'static,
    ) -> std::io::Result<OneShot<T>> {
        let (tx, rx) = self.one_shot_channel();
        std::thread::Builder::new()
            .name(thread_name.to_string())
            .spawn(move || {
                let result = job();
                let _ = tx.send(result);
            })?;
        Ok(OneShot { rx: Some(rx) })
    }

    pub(crate) fn one_shot_channel<T>(&self) -> (WakeSender<T>, OneShot<T>) {
        let (tx, rx) = self.channel();
        (tx, OneShot { rx: Some(rx) })
    }
}

pub(crate) enum OneShotState<T> {
    Pending,
    Ready(T),
    Closed,
}

pub(crate) struct OneShot<T> {
    rx: Option<Receiver<T>>,
}

impl<T> std::fmt::Debug for OneShot<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OneShot")
            .field("pending", &self.is_pending())
            .finish()
    }
}

impl<T> OneShot<T> {
    /// Ready exactly once; Closed if the worker died without a result, and on every poll after Ready/Closed.
    pub(crate) fn poll(&mut self) -> OneShotState<T> {
        let Some(rx) = self.rx.take() else {
            return OneShotState::Closed;
        };
        match rx.try_recv() {
            Ok(value) => OneShotState::Ready(value),
            Err(TryRecvError::Empty) => {
                self.rx = Some(rx);
                OneShotState::Pending
            }
            Err(TryRecvError::Disconnected) => OneShotState::Closed,
        }
    }

    pub(crate) fn is_pending(&self) -> bool {
        self.rx.is_some()
    }
}

/// `mpsc::Sender` that wakes the UI after every successful send and when the last
/// handle of a worker goes away (so a worker that died without an answer is noticed
/// as `Disconnected` without waiting for an unrelated event).
pub(crate) struct WakeSender<T> {
    tx: Sender<T>,
    waker: UiWaker,
}

impl<T> WakeSender<T> {
    pub(crate) fn send(&self, value: T) -> Result<(), SendError<T>> {
        self.tx.send(value)?;
        self.waker.wake();
        Ok(())
    }
}

impl<T> Clone for WakeSender<T> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            waker: self.waker.clone(),
        }
    }
}

impl<T> Drop for WakeSender<T> {
    fn drop(&mut self) {
        self.waker.wake();
    }
}

/// Channel end shared protocol code reports through: the running worker holds a
/// [`WakeSender`], unit tests may pass a plain `mpsc::Sender`.
pub(crate) trait EventSink<T> {
    fn send(&self, value: T) -> Result<(), SendError<T>>;
}

impl<T> EventSink<T> for Sender<T> {
    fn send(&self, value: T) -> Result<(), SendError<T>> {
        Sender::send(self, value)
    }
}

impl<T> EventSink<T> for WakeSender<T> {
    fn send(&self, value: T) -> Result<(), SendError<T>> {
        WakeSender::send(self, value)
    }
}

/// Bounded counterpart of [`WakeSender`].
pub(crate) struct WakeSyncSender<T> {
    tx: SyncSender<T>,
    waker: UiWaker,
}

impl<T> WakeSyncSender<T> {
    pub(crate) fn send(&self, value: T) -> Result<(), SendError<T>> {
        self.tx.send(value)?;
        self.waker.wake();
        Ok(())
    }

    pub(crate) fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        self.tx.try_send(value)?;
        self.waker.wake();
        Ok(())
    }
}

impl<T> Clone for WakeSyncSender<T> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            waker: self.waker.clone(),
        }
    }
}

impl<T> Drop for WakeSyncSender<T> {
    fn drop(&mut self) {
        self.waker.wake();
    }
}

#[cfg(test)]
mod tests {
    use super::{OneShotState, UiWaker};
    use std::sync::{Arc, Barrier};

    #[test]
    fn ui_waker_burst_of_sends_delivers_one_event() {
        const SENDERS: usize = 8;
        let waker = UiWaker::counting();
        let (tx, rx) = waker.channel::<usize>();
        let start = Arc::new(Barrier::new(SENDERS));
        let workers = (0..SENDERS)
            .map(|i| {
                let tx = tx.clone();
                let start = Arc::clone(&start);
                std::thread::spawn(move || {
                    start.wait();
                    tx.send(i).unwrap();
                    // Keep the handle alive: this test counts send wakes, not drop wakes.
                    std::mem::forget(tx);
                })
            })
            .collect::<Vec<_>>();
        for worker in workers {
            worker.join().unwrap();
        }
        std::mem::forget(tx);

        assert_eq!(waker.take_events(), 1);
        assert_eq!(rx.try_iter().count(), SENDERS);
    }

    #[test]
    fn ui_waker_send_after_drain_started_delivers_a_new_event() {
        let waker = UiWaker::counting();
        let (tx, rx) = waker.channel::<u32>();
        tx.send(1).unwrap();
        assert_eq!(waker.take_events(), 1);

        let reset_done = Arc::new(Barrier::new(2));
        let sent = Arc::new(Barrier::new(2));
        let worker = {
            let tx = tx.clone();
            let (reset_done, sent) = (Arc::clone(&reset_done), Arc::clone(&sent));
            std::thread::spawn(move || {
                reset_done.wait();
                tx.send(2).unwrap();
                std::mem::forget(tx);
                sent.wait();
            })
        };

        waker.begin_drain();
        reset_done.wait();
        // The worker sends while the UI is mid-walk, after the reset.
        sent.wait();
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), vec![1, 2]);
        worker.join().unwrap();
        assert_eq!(waker.take_events(), 1, "the send after the reset must wake again");
        std::mem::forget(tx);
    }

    #[test]
    fn ui_waker_wake_while_pending_is_coalesced_until_drain() {
        let waker = UiWaker::counting();
        let (tx, _rx) = waker.channel::<u32>();
        tx.send(1).unwrap();
        tx.send(2).unwrap();
        assert_eq!(waker.take_events(), 1);
        waker.begin_drain();
        tx.send(3).unwrap();
        assert_eq!(waker.take_events(), 1);
        std::mem::forget(tx);
    }

    #[test]
    fn ui_waker_bounded_drain_with_leftovers_wakes_again() {
        const LIMIT: usize = 2;
        let waker = UiWaker::counting();
        let (tx, rx) = waker.channel::<u32>();
        for value in 0..5 {
            tx.send(value).unwrap();
        }
        assert_eq!(waker.take_events(), 1);

        // One bounded UI pass: stops after LIMIT messages without seeing the channel empty.
        waker.begin_drain();
        let mut read = 0;
        let mut drained = false;
        while read < LIMIT {
            match rx.try_recv() {
                Ok(_) => read += 1,
                Err(_) => {
                    drained = true;
                    break;
                }
            }
        }
        if !drained {
            waker.wake();
        }
        assert_eq!(read, LIMIT);
        assert_eq!(waker.take_events(), 1, "leftovers after the limit must re-wake");
        assert_eq!(rx.try_iter().count(), 3);
        std::mem::forget(tx);
    }

    #[test]
    fn ui_waker_worker_dropping_its_sender_wakes_for_disconnect() {
        let waker = UiWaker::counting();
        let (tx, rx) = waker.channel::<u32>();
        std::thread::spawn(move || drop(tx)).join().unwrap();
        assert_eq!(waker.take_events(), 1);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn ui_waker_redeliver_posts_only_while_pending() {
        let waker = UiWaker::counting();
        waker.redeliver_pending();
        assert_eq!(waker.take_events(), 0);
        waker.wake();
        assert_eq!(waker.take_events(), 1);
        waker.redeliver_pending();
        assert_eq!(waker.take_events(), 1);
        waker.begin_drain();
        waker.redeliver_pending();
        assert_eq!(waker.take_events(), 0);
    }

    #[test]
    fn one_shot_returns_ready_once_then_closed() {
        let mut job = UiWaker::counting()
            .spawn_one_shot("rriter-test-one-shot-ready", || 42)
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        loop {
            match job.poll() {
                OneShotState::Ready(value) => {
                    assert_eq!(value, 42);
                    break;
                }
                OneShotState::Pending if std::time::Instant::now() < deadline => {
                    std::thread::yield_now();
                }
                state => panic!("unexpected one-shot state: {}", match state {
                    OneShotState::Pending => "pending timeout",
                    OneShotState::Ready(_) => "ready",
                    OneShotState::Closed => "closed",
                }),
            }
        }
        assert!(matches!(job.poll(), OneShotState::Closed));
        assert!(matches!(job.poll(), OneShotState::Closed));
    }

    #[test]
    fn one_shot_panicking_job_closes() {
        let mut job = UiWaker::counting()
            .spawn_one_shot("rriter-test-one-shot-panic", || -> () { panic!("worker panic") })
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        loop {
            match job.poll() {
                OneShotState::Closed => break,
                OneShotState::Pending if std::time::Instant::now() < deadline => {
                    std::thread::yield_now();
                }
                _ => panic!("panicking one-shot did not close"),
            }
        }
        assert!(matches!(job.poll(), OneShotState::Closed));
        assert!(matches!(job.poll(), OneShotState::Closed));
    }

    #[test]
    fn one_shot_is_pending_transitions_after_completion() {
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let mut job = UiWaker::counting()
            .spawn_one_shot("rriter-test-one-shot-pending", move || {
                let _ = release_rx.recv();
                7
            })
            .unwrap();
        assert!(job.is_pending());
        assert!(matches!(job.poll(), OneShotState::Pending));
        release_tx.send(()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        loop {
            match job.poll() {
                OneShotState::Ready(7) => break,
                OneShotState::Pending if std::time::Instant::now() < deadline => {
                    std::thread::yield_now();
                }
                _ => panic!("one-shot did not become ready"),
            }
        }
        assert!(!job.is_pending());
    }
}
