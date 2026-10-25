//! Synchronized output (DEC mode 2026) on the parser side: a held batch must be replayed when
//! its deadline passes, even if the PTY stays silent and no frame is being drawn.

use crate::app::terminal_sink::RriterTermSink;
use ronsole_term::Term;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

/// How long the UI thread waits after a hold deadline before replaying the held bytes itself.
/// The parser thread replays on time; the UI replay is only a fallback for a stalled parser.
pub(crate) const SYNC_UI_FALLBACK_GRACE: Duration = Duration::from_millis(50);

/// Whether the UI thread may replay a hold with `deadline` at `now`.
pub(crate) fn sync_ui_fallback_due(deadline: Instant, now: Instant) -> bool {
    deadline
        .checked_add(SYNC_UI_FALLBACK_GRACE)
        .is_some_and(|at| now >= at)
}

/// Replays the held mode 2026 output when its deadline has passed. Returns whether the grid changed.
/// Publishes the grid's hold deadline afterwards, so the UI thread sees the replay.
pub(crate) fn flush_expired_terminal_output(
    grid: &mut Term,
    sink: &mut RriterTermSink,
    now: Instant,
) -> bool {
    let changed = grid.flush_expired(now, sink).grid_changed;
    if changed {
        grid.mark_changed();
    }
    sink.publish_sync_deadline(grid);
    changed
}

/// Next PTY chunk for the parser thread. While mode 2026 holds output the wait ends at the
/// hold deadline, so the held bytes are replayed on time. `redraw` is called when a replay
/// changed the grid. Returns `None` when the reader is gone or the grid lock is poisoned.
pub(crate) fn receive_terminal_chunk(
    rx: &Receiver<Vec<u8>>,
    grid: &Mutex<Term>,
    sink: &mut RriterTermSink,
    redraw: &dyn Fn(),
) -> Option<Vec<u8>> {
    loop {
        let deadline = grid.lock().ok()?.sync_deadline();
        let Some(deadline) = deadline else {
            return rx.recv().ok();
        };
        // A chunk that is already queued must not skip the deadline: while output keeps
        // arriving, `recv_timeout` returns at once and the hold would never end.
        if deadline > Instant::now() {
            match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(chunk) => return Some(chunk),
                Err(RecvTimeoutError::Disconnected) => return None,
                Err(RecvTimeoutError::Timeout) => {}
            }
        }
        let changed = {
            let mut guard = grid.lock().ok()?;
            flush_expired_terminal_output(&mut guard, sink, Instant::now())
        };
        if changed {
            redraw();
        }
    }
}

/// Reader closed while mode 2026 holds output: waits out the deadline and replays the held
/// bytes, so the exit message that follows is not stranded in the hold buffer.
/// The replay clears the hold, so the loop ends after one pass.
pub(crate) fn wait_out_sync_deadline(
    grid: &Mutex<Term>,
    sink: &mut RriterTermSink,
    redraw: &dyn Fn(),
) {
    loop {
        let Some(deadline) = grid
            .lock()
            .ok()
            .and_then(|guard| guard.sync_deadline())
        else {
            return;
        };
        std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
        let changed = match grid.lock() {
            Ok(mut guard) => flush_expired_terminal_output(&mut guard, sink, Instant::now()),
            Err(_) => return,
        };
        if changed {
            redraw();
        }
    }
}

/// What one parsed batch did to the grid.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct BatchOutcome {
    /// The batch started a mode 2026 hold, so the UI thread must re-arm its wait.
    pub(crate) new_deadline: bool,
    /// The batch changed the visible grid; a batch held entirely by mode 2026 does not.
    pub(crate) grid_changed: bool,
}

/// Feeds a batch to the grid and reports what changed. Publishes the hold deadline before returning.
pub(crate) fn advance_terminal_output_batch(
    grid: &mut Term,
    sink: &mut RriterTermSink,
    chunks: &[Vec<u8>],
) -> BatchOutcome {
    let now = Instant::now();
    let mut grid_changed = false;
    let mut new_deadline = false;
    for chunk in chunks {
        // A long batch can pass the hold deadline; replay before the chunk joins the held bytes.
        grid_changed |= grid.flush_expired(now, sink).grid_changed;
        let outcome = grid.advance(chunk, now, sink);
        grid_changed |= outcome.grid_changed;
        new_deadline |= outcome.new_deadline;
    }
    if grid_changed {
        grid.mark_changed();
    }
    sink.publish_sync_deadline(grid);
    BatchOutcome {
        new_deadline,
        grid_changed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::terminal::test_term;
    use crate::app::terminal_host_events::HostEventsSink;
    use crate::app::terminal_process::{TerminalTitleState, install_terminal_io_threads};
    use crate::ui_waker::UiWaker;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    fn screen_text(grid: &Term) -> String {
        (0..grid.total_rows())
            .filter_map(|y| grid.row(y))
            .flat_map(|line| line.iter().map(|cell| cell.ch()))
            .collect()
    }

    #[test]
    fn sync_deadline_flushed_at_deadline_redraws() {
        let mut grid = test_term(20, 2);
        let mut sink = RriterTermSink::new(None, None);
        // Mode 2026 holds the batch until its reset or the hold deadline (ronsole-term sync.rs).
        grid.advance(b"\x1b[?2026hheld", Instant::now(), &mut sink);
        let deadline = grid.sync_deadline().expect("mode 2026 holds the output");

        assert!(!flush_expired_terminal_output(
            &mut grid,
            &mut sink,
            deadline - Duration::from_millis(1)
        ));
        assert!(!screen_text(&grid).contains("held"));

        assert!(flush_expired_terminal_output(&mut grid, &mut sink, deadline));
        assert!(screen_text(&grid).contains("held"));
        assert_eq!(grid.sync_deadline(), None);
    }

    #[test]
    fn batch_held_entirely_by_mode_2026_requests_no_frame() {
        let mut grid = test_term(20, 2);
        let mut sink = RriterTermSink::new(None, None);
        let outcome = advance_terminal_output_batch(
            &mut grid,
            &mut sink,
            &[b"\x1b[?2026hheld".to_vec()],
        );
        // Mode 2026 holds the bytes, so the screen is unchanged; the new deadline still wakes the UI.
        // Expected values: ronsole-term tests/sync.rs (held set reports grid_changed false, new_deadline true).
        assert_eq!(
            outcome,
            BatchOutcome {
                new_deadline: true,
                grid_changed: false,
            }
        );
        assert!(grid.sync_deadline().is_some());
    }

    #[test]
    fn published_sync_deadline_follows_the_grid() {
        let mut grid = test_term(20, 2);
        let host_events = HostEventsSink::new(UiWaker::counting());
        let mut sink = RriterTermSink::new(None, None).with_host_events(host_events.clone());
        advance_terminal_output_batch(&mut grid, &mut sink, &[b"\x1b[?2026hheld".to_vec()]);
        let deadline = grid.sync_deadline().expect("mode 2026 holds the output");
        assert_eq!(host_events.sync_deadline(), Some(deadline));

        assert!(flush_expired_terminal_output(&mut grid, &mut sink, deadline));
        assert_eq!(host_events.sync_deadline(), None);
    }

    /// Yields one held batch, then output that never pauses until `stop` is set.
    struct ContinuousOutput {
        first: Option<Vec<u8>>,
        stop: Arc<AtomicBool>,
    }

    impl std::io::Read for ContinuousOutput {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if let Some(first) = self.first.take() {
                let len = first.len().min(buf.len());
                buf[..len].copy_from_slice(&first[..len]);
                return Ok(len);
            }
            if self.stop.load(Ordering::Acquire) {
                return Ok(0);
            }
            std::thread::sleep(Duration::from_millis(1));
            let line = b"tick\r\n";
            buf[..line.len()].copy_from_slice(line);
            Ok(line.len())
        }
    }

    #[test]
    fn sync_hold_flushed_during_continuous_output() {
        let grid = Arc::new(Mutex::new(test_term(40, 4)));
        let writer: Box<dyn std::io::Write + Send> = Box::new(std::io::sink());
        let title_cache = Arc::new(Mutex::new(TerminalTitleState::new("shell".to_string())));
        let write_channel = crate::app::terminal_sink::pty_write_queue();
        let stop = Arc::new(AtomicBool::new(false));
        // The hold never ends by reset, and the stream never gaps: only the deadline can replay it.
        install_terminal_io_threads(
            &grid,
            Box::new(ContinuousOutput {
                first: Some(b"\x1b[?2026hheld-marker\r\n".to_vec()),
                stop: stop.clone(),
            }),
            writer,
            None,
            title_cache,
            HostEventsSink::new(UiWaker::counting()),
            write_channel,
        )
        .expect("terminal io threads spawn");

        let until = Instant::now() + Duration::from_secs(5);
        loop {
            let text = screen_text(&grid.lock().expect("grid lock"));
            if text.contains("held-marker") {
                break;
            }
            assert!(Instant::now() < until, "held output never replayed: {text:?}");
            std::thread::sleep(Duration::from_millis(10));
        }
        stop.store(true, Ordering::Release);
    }

    #[test]
    fn held_output_is_shown_after_stream_ends() {
        let grid = Arc::new(Mutex::new(test_term(20, 2)));
        let writer: Box<dyn std::io::Write + Send> = Box::new(std::io::sink());
        let title_cache = Arc::new(Mutex::new(TerminalTitleState::new("shell".to_string())));
        let write_channel = crate::app::terminal_sink::pty_write_queue();
        // The reader yields a held batch and then EOF: the replay must happen at the deadline.
        install_terminal_io_threads(
            &grid,
            Box::new(std::io::Cursor::new(b"\x1b[?2026hheld".to_vec())),
            writer,
            None,
            title_cache,
            HostEventsSink::new(UiWaker::counting()),
            write_channel,
        )
        .expect("terminal io threads spawn");

        let until = Instant::now() + Duration::from_secs(5);
        loop {
            let text = screen_text(&grid.lock().expect("grid lock"));
            if text.contains("held") {
                break;
            }
            assert!(Instant::now() < until, "held output never replayed: {text:?}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
