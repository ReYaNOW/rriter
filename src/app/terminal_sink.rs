//! Parser-side host of `ronsole_term::TermSink`: queues replies for the PTY writer thread without
//! blocking, forwards titles and host events, and publishes the mode 2026 hold deadline so the UI
//! thread can read it without the grid lock.

use crate::app::terminal_host_events::HostEventsSink;
use crate::app::terminal_process::TerminalTitleCache;
use ronsole_term::{Term, TermSink};
use std::sync::mpsc::SyncSender;

/// Queues `bytes` for the PTY writer thread without blocking. Returns `false` when the message
/// was dropped because the queue is full or its writer is gone.
pub(crate) fn try_queue_report(tx: &SyncSender<Vec<u8>>, bytes: &[u8]) -> bool {
    tx.try_send(bytes.to_vec()).is_ok()
}

/// Host side of `ronsole_term::TermSink`: forwards replies to the PTY writer, programmed
/// titles to the title cache, and clipboard and working-directory events to the tab's
/// `HostEventsSink` for the UI thread.
pub(crate) struct RriterTermSink {
    title_cache: Option<TerminalTitleCache>,
    reply_tx: Option<SyncSender<Vec<u8>>>,
    host_events: Option<HostEventsSink>,
}

impl RriterTermSink {
    pub(crate) fn new(
        title_cache: Option<TerminalTitleCache>,
        reply_tx: Option<SyncSender<Vec<u8>>>,
    ) -> Self {
        Self {
            title_cache,
            reply_tx,
            host_events: None,
        }
    }

    pub(crate) fn with_host_events(mut self, host_events: HostEventsSink) -> Self {
        self.host_events = Some(host_events);
        self
    }

    /// Publishes the grid's mode 2026 hold deadline for the UI thread. Call with the grid lock
    /// held, after every change to the grid, so the published value matches the grid.
    pub(crate) fn publish_sync_deadline(&self, grid: &Term) {
        if let Some(host_events) = self.host_events.as_ref() {
            host_events.set_sync_deadline(grid.sync_deadline());
        }
    }
}

impl TermSink for RriterTermSink {
    fn reply(&mut self, bytes: &[u8]) {
        if let Some(tx) = self.reply_tx.as_ref() {
            // A full queue drops the reply: blocking here would stall the parser under the grid lock.
            try_queue_report(tx, bytes);
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
    use crate::app::terminal_process::TERMINAL_REPLY_QUEUE_CAPACITY;
    use std::time::{Duration, Instant};

    #[test]
    fn full_report_queue_drops_report_without_blocking() {
        let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(1);
        assert!(try_queue_report(&tx, b"\x1b[I"));

        // Nobody drains the queue: a focus report must be dropped, not waited for.
        let (done_tx, done_rx) = std::sync::mpsc::channel::<bool>();
        let sender = tx.clone();
        std::thread::spawn(move || {
            let _ = done_tx.send(try_queue_report(&sender, b"\x1b[O"));
        });
        let queued = done_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("report blocked on a full queue");
        assert!(!queued);
        assert_eq!(rx.try_recv().ok(), Some(b"\x1b[I".to_vec()));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn reports_keep_queue_order_and_closed_writer_is_not_fatal() {
        let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(4);
        assert!(try_queue_report(&tx, b"reply"));
        // Focus reports share the queue with parser replies, so their order is kept.
        assert!(try_queue_report(&tx, b"\x1b[I"));
        assert_eq!(rx.try_recv().ok(), Some(b"reply".to_vec()));
        assert_eq!(rx.try_recv().ok(), Some(b"\x1b[I".to_vec()));

        drop(rx);
        assert!(!try_queue_report(&tx, b"\x1b[O"));
    }

    #[test]
    fn terminal_replies_through_bounded_channel() {
        let mut grid = test_term(8, 2);
        let (reply_tx, reply_rx) =
            std::sync::mpsc::sync_channel::<Vec<u8>>(TERMINAL_REPLY_QUEUE_CAPACITY);
        let mut sink = RriterTermSink::new(None, Some(reply_tx));

        // Primary device attributes replies `ESC [ ? 62 c` (ronsole-term perform.rs).
        grid.advance(b"\x1b[c", Instant::now(), &mut sink);
        assert_eq!(reply_rx.try_recv().ok(), Some(b"\x1b[?62c".to_vec()));

        // With nobody draining the queue, replies beyond its capacity are dropped, not blocked on.
        for _ in 0..TERMINAL_REPLY_QUEUE_CAPACITY * 2 {
            grid.advance(b"\x1b[c", Instant::now(), &mut sink);
        }
        let mut queued = 0;
        while reply_rx.try_recv().is_ok() {
            queued += 1;
        }
        assert_eq!(queued, TERMINAL_REPLY_QUEUE_CAPACITY);
    }
}
