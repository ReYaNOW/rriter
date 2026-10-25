//! Parser-side host of `ronsole_term::TermSink`: queues replies for the PTY writer thread without
//! blocking, forwards titles and host events, and publishes the mode 2026 hold deadline and the
//! DECSET 1004 state so the UI thread can read them without the grid lock.
//!
//! The PTY writer queue lives here too: one FIFO carries input, focus reports and replies.

use crate::app::terminal_host_events::HostEventsSink;
use crate::app::terminal_process::{TERMINAL_REPLY_QUEUE_CAPACITY, TerminalTitleCache};
use ronsole_term::{Term, TermSink};
use std::io::{self, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

/// One write for the PTY writer thread.
enum PtyWrite {
    /// Keyboard, mouse, paste, automation and focus bytes. Never dropped.
    Input(Vec<u8>),
    /// Parser replies. Capped at `TERMINAL_REPLY_QUEUE_CAPACITY` pending messages.
    Reply(Vec<u8>),
}

/// Producer side of the PTY writer queue. Input and replies share one FIFO, so the child reads
/// them in the order they were queued. Cloning shares the same queue and reply count.
#[derive(Clone)]
pub(crate) struct PtyWriteQueue {
    tx: Sender<PtyWrite>,
    pending_replies: Arc<AtomicUsize>,
}

/// Consumer side of the PTY writer queue, owned by the writer thread.
pub(crate) struct PtyWriteReceiver {
    rx: Receiver<PtyWrite>,
    pending_replies: Arc<AtomicUsize>,
}

pub(crate) fn pty_write_queue() -> (PtyWriteQueue, PtyWriteReceiver) {
    let (tx, rx) = std::sync::mpsc::channel();
    let pending_replies = Arc::new(AtomicUsize::new(0));
    (
        PtyWriteQueue {
            tx,
            pending_replies: pending_replies.clone(),
        },
        PtyWriteReceiver {
            rx,
            pending_replies,
        },
    )
}

impl PtyWriteQueue {
    /// Queues input without blocking and without dropping it. Fails only when the writer thread
    /// has stopped.
    pub(crate) fn input(&self, bytes: &[u8]) -> io::Result<()> {
        self.tx
            .send(PtyWrite::Input(bytes.to_vec()))
            .map_err(|_| io::Error::other("terminal writer has stopped"))
    }

    /// Queues a parser reply without blocking. Dropped when `TERMINAL_REPLY_QUEUE_CAPACITY`
    /// replies are already pending or the writer has stopped. Returns whether it was queued.
    pub(crate) fn reply(&self, bytes: &[u8]) -> bool {
        // Reserve the slot before sending, so concurrent producers never exceed the capacity.
        if self.pending_replies.fetch_add(1, Ordering::Relaxed) >= TERMINAL_REPLY_QUEUE_CAPACITY {
            self.pending_replies.fetch_sub(1, Ordering::Relaxed);
            return false;
        }
        let queued = self.tx.send(PtyWrite::Reply(bytes.to_vec())).is_ok();
        if !queued {
            self.pending_replies.fetch_sub(1, Ordering::Relaxed);
        }
        queued
    }
}

impl PtyWriteReceiver {
    /// Writes queued messages in order until every producer is gone or a write fails.
    pub(crate) fn run(self, mut writer: Box<dyn Write + Send>) {
        while let Ok(message) = self.rx.recv() {
            let (bytes, is_reply) = match message {
                PtyWrite::Input(bytes) => (bytes, false),
                PtyWrite::Reply(bytes) => (bytes, true),
            };
            let written = writer.write_all(&bytes).and_then(|()| writer.flush());
            if is_reply {
                // Released after the write, so a reply counts against the cap until the child has it.
                self.pending_replies.fetch_sub(1, Ordering::Relaxed);
            }
            if written.is_err() {
                break;
            }
        }
    }

    /// Bytes of the next queued message, without writing it.
    #[cfg(test)]
    pub(crate) fn try_next(&self) -> Option<Vec<u8>> {
        match self.rx.try_recv().ok()? {
            PtyWrite::Input(bytes) => Some(bytes),
            PtyWrite::Reply(bytes) => {
                self.pending_replies.fetch_sub(1, Ordering::Relaxed);
                Some(bytes)
            }
        }
    }
}

/// Host side of `ronsole_term::TermSink`: forwards replies to the PTY writer, programmed
/// titles to the title cache, and clipboard and working-directory events to the tab's
/// `HostEventsSink` for the UI thread.
pub(crate) struct RriterTermSink {
    title_cache: Option<TerminalTitleCache>,
    reply_queue: Option<PtyWriteQueue>,
    host_events: Option<HostEventsSink>,
}

impl RriterTermSink {
    pub(crate) fn new(
        title_cache: Option<TerminalTitleCache>,
        reply_queue: Option<PtyWriteQueue>,
    ) -> Self {
        Self {
            title_cache,
            reply_queue,
            host_events: None,
        }
    }

    pub(crate) fn with_host_events(mut self, host_events: HostEventsSink) -> Self {
        self.host_events = Some(host_events);
        self
    }

    /// Publishes the grid's mode 2026 hold deadline, DECSET 1004 state and content generation for
    /// the UI thread. Call with the grid lock held, after every change to the grid, so the published
    /// values match it.
    pub(crate) fn publish_sync_deadline(&self, grid: &Term) {
        if let Some(host_events) = self.host_events.as_ref() {
            host_events.set_sync_deadline(grid.sync_deadline());
            host_events.set_focus_reporting(grid.focus_reporting());
            host_events.set_content_generation(grid.content_generation());
        }
    }
}

impl TermSink for RriterTermSink {
    fn reply(&mut self, bytes: &[u8]) {
        if let Some(queue) = self.reply_queue.as_ref() {
            // A full queue drops the reply: blocking here would stall the parser under the grid lock.
            queue.reply(bytes);
        }
    }

    fn title(&mut self, title: &str) {
        if let Some(cache) = self.title_cache.as_ref() {
            crate::platform::lock_recover(cache).set_programmed(title.to_string());
        }
    }

    fn clipboard_set(&mut self, text: String) {
        if let Some(host_events) = self.host_events.as_ref() {
            host_events.set_clipboard(text);
        }
    }

    fn working_directory(&mut self, host_name: &str, path: &[u8]) {
        if let Some(host_events) = self.host_events.as_ref() {
            host_events.set_cwd(host_name, path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::terminal::test_term;
    use std::time::{Duration, Instant};

    /// Writer that holds each write until the test sends on `gate`, then reports the bytes.
    struct GatedWriter {
        gate: Receiver<()>,
        written: Sender<Vec<u8>>,
    }

    impl Write for GatedWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let _ = self.gate.recv();
            let _ = self.written.send(buf.to_vec());
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn input_returns_while_writer_is_blocked_and_keeps_order() {
        let (gate_tx, gate_rx) = std::sync::mpsc::channel::<()>();
        let (written_tx, written_rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let (queue, receiver) = pty_write_queue();
        std::thread::spawn(move || {
            receiver.run(Box::new(GatedWriter {
                gate: gate_rx,
                written: written_tx,
            }))
        });

        // The writer is stuck in its first write, so queueing must return at once.
        let started = Instant::now();
        // A focus report is queued before the key it precedes.
        queue.input(b"\x1b[I").expect("focus report queued");
        queue.input(b"a").expect("key queued");
        assert!(started.elapsed() < Duration::from_secs(1), "input waited for the writer");

        gate_tx.send(()).expect("release first write");
        gate_tx.send(()).expect("release second write");
        let timeout = Duration::from_secs(2);
        assert_eq!(written_rx.recv_timeout(timeout).ok(), Some(b"\x1b[I".to_vec()));
        assert_eq!(written_rx.recv_timeout(timeout).ok(), Some(b"a".to_vec()));
    }

    #[test]
    fn reply_queue_caps_pending_replies_and_never_drops_input() {
        let (queue, rx) = pty_write_queue();
        for _ in 0..TERMINAL_REPLY_QUEUE_CAPACITY {
            assert!(queue.reply(b"reply"));
        }
        // Replies past the cap are dropped; input is not limited by it.
        assert!(!queue.reply(b"dropped"));
        queue.input(b"\x1b[I").expect("input queued");

        // Taking a reply frees its slot once the writer has taken it.
        assert_eq!(rx.try_next(), Some(b"reply".to_vec()));
        assert!(queue.reply(b"again"));

        let mut queued = Vec::new();
        while let Some(bytes) = rx.try_next() {
            queued.push(bytes);
        }
        // 31 replies, the input, then the reply queued after the slot was freed.
        assert_eq!(queued.len(), TERMINAL_REPLY_QUEUE_CAPACITY + 1);
        assert_eq!(queued[TERMINAL_REPLY_QUEUE_CAPACITY - 1], b"\x1b[I".to_vec());
        assert_eq!(queued.last(), Some(&b"again".to_vec()));
    }

    #[test]
    fn closed_writer_refuses_input_and_replies_without_panicking() {
        let (queue, receiver) = pty_write_queue();
        drop(receiver);
        assert!(queue.input(b"\x1b[O").is_err());
        assert!(!queue.reply(b"reply"));
    }

    #[test]
    fn terminal_replies_through_reply_queue() {
        let mut grid = test_term(8, 2);
        let (queue, rx) = pty_write_queue();
        let mut sink = RriterTermSink::new(None, Some(queue));

        // Primary device attributes replies `ESC [ ? 62 c` (ronsole-term perform.rs).
        grid.advance(b"\x1b[c", Instant::now(), &mut sink);
        assert_eq!(rx.try_next(), Some(b"\x1b[?62c".to_vec()));

        // With nobody draining the queue, replies beyond its capacity are dropped, not blocked on.
        for _ in 0..TERMINAL_REPLY_QUEUE_CAPACITY * 2 {
            grid.advance(b"\x1b[c", Instant::now(), &mut sink);
        }
        let mut queued = 0;
        while rx.try_next().is_some() {
            queued += 1;
        }
        assert_eq!(queued, TERMINAL_REPLY_QUEUE_CAPACITY);
    }
}
