//! Events the terminal parser thread receives from `ronsole_term` that the UI thread applies:
//! OSC 52 clipboard writes and OSC 7 working directories.
//!
//! The parser thread only stores the latest value per tab and wakes the UI through
//! `UiWaker`; the UI thread takes the stored values on its tick, without the grid lock.
//! It also publishes the mode 2026 hold deadline, which the UI thread reads the same way.

use crate::ui_waker::UiWaker;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Working directory reported by OSC 7, kept as the raw host and percent-decoded path bytes.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct PendingCwd {
    pub(crate) host: String,
    pub(crate) path: Vec<u8>,
}

/// Latest unapplied host events of one terminal tab; a newer event replaces an older one.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct PendingHostEvents {
    pub(crate) clipboard: Option<String>,
    pub(crate) cwd: Option<PendingCwd>,
}

/// Parser-side handle: stores events for the UI thread and wakes it.
#[derive(Clone)]
pub(crate) struct HostEventsSink {
    pending: Arc<Mutex<PendingHostEvents>>,
    published_sync_deadline: Arc<Mutex<Option<Instant>>>,
    published_focus_reporting: Arc<AtomicBool>,
    waker: UiWaker,
}

impl HostEventsSink {
    pub(crate) fn new(waker: UiWaker) -> Self {
        Self {
            pending: Arc::new(Mutex::new(PendingHostEvents::default())),
            published_sync_deadline: Arc::new(Mutex::new(None)),
            published_focus_reporting: Arc::new(AtomicBool::new(false)),
            waker,
        }
    }

    /// Parser side: publishes the mode 2026 hold deadline of the grid it just changed.
    pub(crate) fn set_sync_deadline(&self, deadline: Option<Instant>) {
        *crate::platform::lock_recover(&self.published_sync_deadline) = deadline;
    }

    /// Any thread: the last published hold deadline. It reads no grid state, so it never waits
    /// for the grid lock.
    pub(crate) fn sync_deadline(&self) -> Option<Instant> {
        *crate::platform::lock_recover(&self.published_sync_deadline)
    }

    /// Parser side: publishes whether the application enabled focus reporting (DECSET 1004).
    pub(crate) fn set_focus_reporting(&self, enabled: bool) {
        self.published_focus_reporting.store(enabled, Ordering::Release);
    }

    /// Any thread: the last published DECSET 1004 state, read without the grid lock.
    pub(crate) fn focus_reporting(&self) -> bool {
        self.published_focus_reporting.load(Ordering::Acquire)
    }

    pub(crate) fn set_clipboard(&self, text: String) {
        self.update(|events| events.clipboard = Some(text));
    }

    pub(crate) fn set_cwd(&self, host: &str, path: &[u8]) {
        self.update(|events| {
            events.cwd = Some(PendingCwd {
                host: host.to_string(),
                path: path.to_vec(),
            });
        });
    }

    /// Wakes the UI thread without storing anything, e.g. when a mode 2026 hold starts.
    pub(crate) fn wake(&self) {
        self.waker.wake();
    }

    /// UI thread: takes the events stored since the previous call.
    pub(crate) fn take(&self) -> PendingHostEvents {
        std::mem::take(&mut *crate::platform::lock_recover(&self.pending))
    }

    fn update(&self, change: impl FnOnce(&mut PendingHostEvents)) {
        change(&mut crate::platform::lock_recover(&self.pending));
        self.waker.wake();
    }
}

/// Working directory for a new terminal tab: the OSC 7 directory when it still exists,
/// otherwise the fallback the tab would have used anyway.
pub(crate) fn new_terminal_start_dir(
    osc7_cwd: Option<&Path>,
    fallback: Option<PathBuf>,
) -> Option<PathBuf> {
    osc7_cwd
        .filter(|path| path.is_dir())
        .map(Path::to_path_buf)
        .or(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::terminal::test_term;
    use crate::app::terminal_process::RriterTermSink;
    use std::time::Instant;

    fn host_events() -> HostEventsSink {
        HostEventsSink::new(UiWaker::counting())
    }

    #[test]
    fn new_tab_starts_in_osc7_dir() {
        let dir = std::env::temp_dir().join(format!("rriter-t6-start-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let fallback = PathBuf::from("/fallback");

        // An existing OSC 7 directory wins; a missing one or no report falls back as before.
        assert_eq!(
            new_terminal_start_dir(Some(&dir), Some(fallback.clone())),
            Some(dir.clone())
        );
        let missing = dir.join("missing");
        assert_eq!(
            new_terminal_start_dir(Some(&missing), Some(fallback.clone())),
            Some(fallback.clone())
        );
        assert_eq!(
            new_terminal_start_dir(None, Some(fallback.clone())),
            Some(fallback)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn osc52_sets_app_clipboard() {
        let host = host_events();
        let mut grid = test_term(20, 2);
        let mut sink = RriterTermSink::new(None, None).with_host_events(host.clone());
        // Base64 of "hello" is "aGVsbG8=" (RFC 4648, section 10 examples).
        grid.advance(b"\x1b]52;c;aGVsbG8=\x07", Instant::now(), &mut sink);
        assert_eq!(host.take().clipboard.as_deref(), Some("hello"));
    }

    #[test]
    fn osc52_garbage_and_non_utf8_do_not_reach_clipboard() {
        let host = host_events();
        let mut grid = test_term(20, 2);
        let mut sink = RriterTermSink::new(None, None).with_host_events(host.clone());
        // "!!!" is not Base64; "//4=" is Base64 of bytes FF FE, which are not UTF-8.
        for payload in [&b"!!!"[..], &b"//4="[..]] {
            let mut bytes = b"\x1b]52;c;".to_vec();
            bytes.extend_from_slice(payload);
            bytes.push(0x07);
            grid.advance(&bytes, Instant::now(), &mut sink);
            assert_eq!(host.take().clipboard, None, "payload {payload:?}");
        }
    }

    #[test]
    fn osc7_non_utf8_path_bytes_are_kept_raw() {
        let host = host_events();
        let mut grid = test_term(20, 2);
        let mut sink = RriterTermSink::new(None, None).with_host_events(host.clone());
        // %FF is one raw path byte; the sink must not replace it with U+FFFD.
        grid.advance(b"\x1b]7;file://box/%FF\x07", Instant::now(), &mut sink);
        let cwd = host.take().cwd.expect("OSC 7 event");
        assert_eq!(cwd.host, "box");
        // The URI path is "/%FF": the leading slash stays, the escape decodes to one raw byte.
        assert_eq!(cwd.path, b"/\xff".to_vec());
    }

    #[test]
    fn osc7_garbage_stores_nothing() {
        let host = host_events();
        let mut grid = test_term(20, 2);
        let mut sink = RriterTermSink::new(None, None).with_host_events(host.clone());
        grid.advance(b"\x1b]7;not a uri\x07", Instant::now(), &mut sink);
        assert_eq!(host.take().cwd, None);
    }

    #[test]
    fn latest_host_event_replaces_pending_one() {
        let host = host_events();
        host.set_cwd("box", b"/a");
        host.set_cwd("box", b"/b");
        host.set_clipboard("one".to_string());
        host.set_clipboard("two".to_string());
        let taken = host.take();
        assert_eq!(taken.clipboard.as_deref(), Some("two"));
        assert_eq!(taken.cwd.map(|cwd| cwd.path), Some(b"/b".to_vec()));
        assert_eq!(host.take(), PendingHostEvents::default());
    }
}
