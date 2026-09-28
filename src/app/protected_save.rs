//! Background protected (elevated) saves.
//!
//! State machine, one slot per `PathKey`:
//! * no slot: nothing in flight for the path; `enqueue` starts a write -> `Running`;
//! * `Running`: one elevated write of an immutable snapshot is in flight; a new
//!   save of the same path is stored as the single `queued` snapshot (latest
//!   wins, a superseded queued id is redirected to its successor), so writes to
//!   one path never race;
//! * write finished (`poll`): the completion is reported with the snapshot that
//!   was written, which is the saved baseline (`Editor::mark_saved_as`), never
//!   the current text. On success the queued snapshot starts; on failure it is
//!   reported failed too, without a second elevation prompt.
//!
//! A "save then run the confirmed action" flow waits for one id
//! (`await_for_action`); `App` resumes or cancels the flow when it completes.
//! Shutdown is bounded: a short wait for writes to finish, then cancellation
//! (the Linux pkexec tree gets a graceful stop, then is killed), then a second
//! bounded wait.

use crate::platform::{PathKey, TextFileFormat};
use crate::ui_waker::{OneShot, OneShotState, UiWaker};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub(crate) type ProtectedSaveId = u64;

/// Blocking elevated writer run on the worker thread; the last argument is the
/// cancel flag. Production: `platform::write_text_file_elevated`; tests inject a fake.
pub(crate) type ProtectedWriter =
    Arc<dyn Fn(&Path, &str, TextFileFormat, &AtomicBool) -> io::Result<()> + Send + Sync>;

/// Shutdown: how long running writes may finish on their own before cancellation.
const SHUTDOWN_FINISH_WAIT: Duration = Duration::from_millis(1000);
/// Shutdown: how long cancelled writes may take to stop (covers the pkexec grace period).
const SHUTDOWN_CANCEL_WAIT: Duration = Duration::from_millis(2000);
const SHUTDOWN_POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Result of saving the active document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SaveOutcome {
    Saved,
    /// A protected save runs in the background; the buffer stays dirty until it completes.
    Pending(ProtectedSaveId),
    Failed,
}

struct Snapshot {
    id: ProtectedSaveId,
    path: PathBuf,
    text: Arc<str>,
    format: TextFileFormat,
}

impl Snapshot {
    fn same_content(&self, text: &str, format: TextFileFormat) -> bool {
        self.format == format && &*self.text == text
    }
}

struct Running {
    snapshot: Snapshot,
    job: OneShot<io::Result<()>>,
    cancel: Arc<AtomicBool>,
}

struct PathSlot {
    key: PathKey,
    running: Running,
    queued: Option<Snapshot>,
}

/// One finished (or abandoned) save. `text` is exactly what was written.
pub(crate) struct ProtectedSaveCompletion {
    pub id: ProtectedSaveId,
    pub key: PathKey,
    pub path: PathBuf,
    pub text: Arc<str>,
    pub result: Result<(), String>,
}

pub(crate) struct ProtectedSaves {
    slots: Vec<PathSlot>,
    next_id: ProtectedSaveId,
    writer: ProtectedWriter,
    writer_injected: bool,
    /// A headless `App` never elevates for real, even in cargo tests, where the
    /// process-wide headless policy is not installed.
    headless: bool,
    awaiting_action: Option<ProtectedSaveId>,
}

impl Default for ProtectedSaves {
    fn default() -> Self {
        Self::new(false)
    }
}

impl ProtectedSaves {
    pub(crate) fn new(headless: bool) -> Self {
        let mut saves = Self::with_writer(Arc::new(|path, text, format, cancel| {
            crate::platform::write_text_file_elevated(path, text, format, cancel)
        }));
        saves.writer_injected = false;
        saves.headless = headless;
        saves
    }

    pub(crate) fn with_writer(writer: ProtectedWriter) -> Self {
        Self {
            slots: Vec::new(),
            next_id: 1,
            writer,
            writer_injected: true,
            headless: false,
            awaiting_action: None,
        }
    }

    /// An injected writer is a fake, so it may run even where real elevation may not.
    pub(crate) fn elevation_allowed(&self) -> bool {
        self.writer_injected
            || (!self.headless
                && crate::platform::elevation_allowed(crate::platform::headless_policy()))
    }

    pub(crate) fn write_synchronously(
        &self,
        path: &Path,
        text: &str,
        format: TextFileFormat,
        cancel: &AtomicBool,
    ) -> io::Result<()> {
        (self.writer)(path, text, format, cancel)
    }

    pub(crate) fn has_pending(&self) -> bool {
        !self.slots.is_empty()
    }

    pub(crate) fn is_pending(&self, key: &PathKey) -> bool {
        self.slots.iter().any(|slot| slot.key == *key)
    }

    fn allocate_id(&mut self) -> ProtectedSaveId {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        id
    }

    /// Starts an elevated write of `text`, or queues it behind the write already
    /// running for the same path. Returns the id whose completion confirms this
    /// content; `Err` only when the worker could not be started.
    pub(crate) fn enqueue(
        &mut self,
        waker: &UiWaker,
        path: PathBuf,
        text: String,
        format: TextFileFormat,
    ) -> Result<ProtectedSaveId, String> {
        let key = PathKey::new(&path);
        if let Some(index) = self.slots.iter().position(|slot| slot.key == key) {
            let running_id = self.slots[index].running.snapshot.id;
            if self.slots[index].running.snapshot.same_content(&text, format) {
                // The disk ends at this content once the running write lands.
                if let Some(dropped) = self.slots[index].queued.take() {
                    self.redirect_awaiting(dropped.id, running_id);
                }
                return Ok(running_id);
            }
            if let Some(queued) = &self.slots[index].queued
                && queued.same_content(&text, format)
            {
                return Ok(queued.id);
            }
            let id = self.allocate_id();
            let snapshot = Snapshot {
                id,
                path,
                text: Arc::from(text),
                format,
            };
            if let Some(superseded) = self.slots[index].queued.replace(snapshot) {
                self.redirect_awaiting(superseded.id, id);
            }
            return Ok(id);
        }
        let id = self.allocate_id();
        let snapshot = Snapshot {
            id,
            path,
            text: Arc::from(text),
            format,
        };
        match start(&self.writer, waker, snapshot) {
            Ok(running) => {
                self.slots.push(PathSlot {
                    key,
                    running,
                    queued: None,
                });
                Ok(id)
            }
            Err((_, error)) => Err(spawn_error_message(&error)),
        }
    }

    /// Collects finished writes in completion order and starts queued successors.
    pub(crate) fn poll(&mut self, waker: &UiWaker) -> Vec<ProtectedSaveCompletion> {
        let mut done = Vec::new();
        let mut index = 0;
        while index < self.slots.len() {
            let result = match self.slots[index].running.job.poll() {
                OneShotState::Pending => {
                    index += 1;
                    continue;
                }
                OneShotState::Ready(result) => result.map_err(|error| error.to_string()),
                OneShotState::Closed => {
                    Err("фоновое сохранение завершилось без результата".to_string())
                }
            };
            let key = self.slots[index].key.clone();
            let queued = self.slots[index].queued.take();
            let failure = result.as_ref().err().cloned();
            done.push(completion(&key, &self.slots[index].running.snapshot, result));
            match (queued, failure) {
                (Some(next), None) => match start(&self.writer, waker, next) {
                    Ok(running) => {
                        self.slots[index].running = running;
                        index += 1;
                    }
                    Err((next, error)) => {
                        done.push(completion(&key, &next, Err(spawn_error_message(&error))));
                        self.slots.remove(index);
                    }
                },
                (Some(next), Some(message)) => {
                    let message = format!("предыдущее сохранение не удалось: {message}");
                    done.push(completion(&key, &next, Err(message)));
                    self.slots.remove(index);
                }
                (None, _) => {
                    self.slots.remove(index);
                }
            }
        }
        done
    }

    /// The confirmed action (close, quit) runs only after `id` completes.
    pub(crate) fn await_for_action(&mut self, id: ProtectedSaveId) {
        self.awaiting_action = Some(id);
    }

    /// The confirmation flow was answered again, cancelled or superseded.
    pub(crate) fn clear_awaiting_action(&mut self) {
        self.awaiting_action = None;
    }

    /// `true` once, when `id` is the save the confirmed action waits for.
    pub(crate) fn take_awaited(&mut self, id: ProtectedSaveId) -> bool {
        if self.awaiting_action == Some(id) {
            self.awaiting_action = None;
            return true;
        }
        false
    }

    fn redirect_awaiting(&mut self, from: ProtectedSaveId, to: ProtectedSaveId) {
        if self.awaiting_action == Some(from) {
            self.awaiting_action = Some(to);
        }
    }

    /// Bounded shutdown: queued writes are dropped, running ones get
    /// `SHUTDOWN_FINISH_WAIT` to finish, are then cancelled and get
    /// `SHUTDOWN_CANCEL_WAIT` to stop; whatever still runs is abandoned.
    pub(crate) fn shutdown(&mut self) {
        self.awaiting_action = None;
        for slot in &mut self.slots {
            slot.queued = None;
        }
        self.wait_until_idle(SHUTDOWN_FINISH_WAIT);
        for slot in &self.slots {
            slot.running.cancel.store(true, Ordering::Release);
        }
        self.wait_until_idle(SHUTDOWN_CANCEL_WAIT);
        self.slots.clear();
    }

    fn wait_until_idle(&mut self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        loop {
            self.slots
                .retain_mut(|slot| matches!(slot.running.job.poll(), OneShotState::Pending));
            if self.slots.is_empty() || Instant::now() >= deadline {
                return;
            }
            std::thread::sleep(SHUTDOWN_POLL_INTERVAL);
        }
    }
}

impl Drop for ProtectedSaves {
    fn drop(&mut self) {
        if self.has_pending() {
            self.shutdown();
        }
    }
}

fn start(
    writer: &ProtectedWriter,
    waker: &UiWaker,
    snapshot: Snapshot,
) -> Result<Running, (Snapshot, io::Error)> {
    let cancel = Arc::new(AtomicBool::new(false));
    let job = {
        let writer = Arc::clone(writer);
        let cancel = Arc::clone(&cancel);
        let path = snapshot.path.clone();
        let text = Arc::clone(&snapshot.text);
        let format = snapshot.format;
        waker.spawn_one_shot("rriter-protected-save", move || {
            writer(&path, &text, format, &cancel)
        })
    };
    match job {
        Ok(job) => Ok(Running {
            snapshot,
            job,
            cancel,
        }),
        Err(error) => Err((snapshot, error)),
    }
}

fn completion(
    key: &PathKey,
    snapshot: &Snapshot,
    result: Result<(), String>,
) -> ProtectedSaveCompletion {
    ProtectedSaveCompletion {
        id: snapshot.id,
        key: key.clone(),
        path: snapshot.path.clone(),
        text: Arc::clone(&snapshot.text),
        result,
    }
}

fn spawn_error_message(error: &io::Error) -> String {
    format!("не удалось запустить сохранение с повышенными правами: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::{Mutex, mpsc};

    /// Fake elevated writer: records every call and blocks until the test releases it.
    struct Gate {
        calls: Arc<Mutex<Vec<String>>>,
        release: mpsc::Sender<io::Result<()>>,
        max_active: Arc<AtomicUsize>,
    }

    fn gated_saves() -> (ProtectedSaves, Gate) {
        let (release, release_rx) = mpsc::channel::<io::Result<()>>();
        let release_rx = Arc::new(Mutex::new(release_rx));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let writer: ProtectedWriter = {
            let calls = Arc::clone(&calls);
            let max_active = Arc::clone(&max_active);
            Arc::new(
                move |_path: &Path, text: &str, _format: TextFileFormat, _cancel: &AtomicBool| {
                    let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                    max_active.fetch_max(now, Ordering::SeqCst);
                    calls.lock().unwrap().push(text.to_string());
                    let outcome = release_rx
                        .lock()
                        .unwrap()
                        .recv()
                        .unwrap_or_else(|_| Err(io::Error::other("test gate dropped")));
                    active.fetch_sub(1, Ordering::SeqCst);
                    outcome
                },
            )
        };
        let gate = Gate {
            calls,
            release,
            max_active,
        };
        (ProtectedSaves::with_writer(writer), gate)
    }

    fn wait_for_calls(gate: &Gate, count: usize) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while gate.calls.lock().unwrap().len() < count {
            assert!(Instant::now() < deadline, "writer was not called {count} times");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn poll_until(
        saves: &mut ProtectedSaves,
        waker: &UiWaker,
        count: usize,
    ) -> Vec<ProtectedSaveCompletion> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut done = Vec::new();
        while done.len() < count {
            assert!(Instant::now() < deadline, "only {} of {count} saves completed", done.len());
            done.extend(saves.poll(waker));
            std::thread::sleep(Duration::from_millis(2));
        }
        done
    }

    fn target() -> PathBuf {
        PathBuf::from("/protected/rriter-test.txt")
    }

    #[test]
    fn protected_save_edit_during_write_keeps_buffer_dirty() {
        let waker = UiWaker::counting();
        let (mut saves, gate) = gated_saves();
        let mut editor = crate::editor::Editor::new(64);
        editor.set_clean_text("old\n");
        editor.cursor = editor.len();
        let _ = editor.insert_str("new\n");

        let id = saves
            .enqueue(&waker, target(), editor.get_full_text(), TextFileFormat::default())
            .unwrap();
        wait_for_calls(&gate, 1);
        let _ = editor.insert_str("typed during save\n");
        gate.release.send(Ok(())).unwrap();

        let done = poll_until(&mut saves, &waker, 1);
        assert_eq!(done[0].id, id);
        assert_eq!(done[0].result, Ok(()));
        assert_eq!(&*done[0].text, "old\nnew\n");
        editor.mark_saved_as(&done[0].text);
        assert!(editor.is_dirty(), "the edit made during the save must stay unsaved");
        assert!(!saves.has_pending());
    }

    #[test]
    fn protected_saves_to_one_path_are_serialized_latest_snapshot_wins() {
        let waker = UiWaker::counting();
        let (mut saves, gate) = gated_saves();
        let format = TextFileFormat::default();
        let first = saves.enqueue(&waker, target(), "v1".into(), format).unwrap();
        wait_for_calls(&gate, 1);
        let second = saves.enqueue(&waker, target(), "v2".into(), format).unwrap();
        saves.await_for_action(second);
        let third = saves.enqueue(&waker, target(), "v3".into(), format).unwrap();
        assert_ne!(second, third);
        assert_eq!(saves.enqueue(&waker, target(), "v3".into(), format).unwrap(), third);
        assert!(saves.poll(&waker).is_empty());
        assert_eq!(gate.calls.lock().unwrap().len(), 1, "queued save must wait");

        gate.release.send(Ok(())).unwrap();
        let done = poll_until(&mut saves, &waker, 1);
        assert_eq!((done[0].id, &*done[0].text), (first, "v1"));
        assert!(saves.is_pending(&PathKey::new(&target())));

        wait_for_calls(&gate, 2);
        gate.release.send(Ok(())).unwrap();
        let done = poll_until(&mut saves, &waker, 1);
        assert_eq!((done[0].id, &*done[0].text), (third, "v3"));
        assert!(saves.take_awaited(third), "the superseded id is redirected to its successor");
        assert_eq!(*gate.calls.lock().unwrap(), vec!["v1".to_string(), "v3".to_string()]);
        assert_eq!(gate.max_active.load(Ordering::SeqCst), 1);
        assert!(!saves.has_pending());
    }

    #[test]
    fn protected_save_failure_fails_the_queued_snapshot_without_a_second_prompt() {
        let waker = UiWaker::counting();
        let (mut saves, gate) = gated_saves();
        let format = TextFileFormat::default();
        let first = saves.enqueue(&waker, target(), "v1".into(), format).unwrap();
        wait_for_calls(&gate, 1);
        let queued = saves.enqueue(&waker, target(), "v2".into(), format).unwrap();
        gate.release
            .send(Err(io::Error::new(io::ErrorKind::PermissionDenied, "rejected")))
            .unwrap();

        let done = poll_until(&mut saves, &waker, 2);
        assert_eq!(done[0].id, first);
        assert_eq!(done[1].id, queued);
        assert!(done.iter().all(|completion| completion.result.is_err()));
        assert_eq!(gate.calls.lock().unwrap().len(), 1);
        assert!(!saves.has_pending());
    }

    #[test]
    fn protected_save_reverting_to_the_running_snapshot_drops_the_queued_one() {
        let waker = UiWaker::counting();
        let (mut saves, gate) = gated_saves();
        let format = TextFileFormat::default();
        let first = saves.enqueue(&waker, target(), "v1".into(), format).unwrap();
        wait_for_calls(&gate, 1);
        let _ = saves.enqueue(&waker, target(), "v2".into(), format).unwrap();
        assert_eq!(saves.enqueue(&waker, target(), "v1".into(), format).unwrap(), first);
        gate.release.send(Ok(())).unwrap();

        let done = poll_until(&mut saves, &waker, 1);
        assert_eq!(done[0].id, first);
        assert!(!saves.has_pending());
        assert_eq!(gate.calls.lock().unwrap().len(), 1);
    }
}
