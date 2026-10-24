use crate::app::terminal_sync_output::{SYNC_UI_FALLBACK_GRACE, sync_ui_fallback_due};
use ronsole_term::{Term, TermConfig, TermIdentity, TermPalette};
use std::io;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

/// Scrollback depth of every RRiter terminal, in lines.
const TERMINAL_SCROLLBACK_LINES: usize = 10_000;

/// A poisoned terminal mutex must not cascade into a UI-thread panic: the terminal is
/// still structurally valid and can be redrawn, cleared, or shut down safely.
#[inline]
fn recover_terminal_lock<T>(result: std::sync::LockResult<T>) -> T {
    result.unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[inline]
pub(crate) fn lock_terminal_grid(grid: &Mutex<Term>) -> MutexGuard<'_, Term> {
    recover_terminal_lock(grid.lock())
}

/// Resizes the grid when its size changes and drops the selection: reflow moves rows, so
/// selection indices would point at the wrong text. Returns whether a resize happened.
#[inline]
pub(crate) fn resize_terminal_grid(grid: &mut Term, cols: usize, rows: usize) -> bool {
    if grid.cols() == cols && grid.rows() == rows {
        return false;
    }
    grid.resize(cols, rows);
    grid.set_selection(None);
    true
}

/// Replays the held mode 2026 output on `grid` with the same sink the parser thread uses, so
/// replies, titles and host events produced by the replay are not dropped.
fn flush_expired_sync_on_grid(
    grid: &Mutex<Term>,
    title_cache: &crate::app::terminal_process::TerminalTitleCache,
    reply_tx: Option<std::sync::mpsc::SyncSender<Vec<u8>>>,
    host_events: &crate::app::terminal_host_events::HostEventsSink,
    now: Instant,
) -> bool {
    let mut sink =
        crate::app::terminal_process::RriterTermSink::new(Some(title_cache.clone()), reply_tx)
            .with_host_events(host_events.clone());
    let mut grid = lock_terminal_grid(grid);
    crate::app::terminal_sync_output::flush_expired_terminal_output(&mut grid, &mut sink, now)
}

/// Builds a `Term` with the production config (scrollback, identity) and a fixed
/// palette, for tests that need a grid without spawning a process.
#[cfg(test)]
pub(crate) fn test_term(cols: usize, rows: usize) -> Term {
    let black = ronsole_term::Rgb { r: 0, g: 0, b: 0 };
    let white = ronsole_term::Rgb {
        r: 255,
        g: 255,
        b: 255,
    };
    Term::new(TermConfig {
        cols,
        rows,
        scrollback_lines: TERMINAL_SCROLLBACK_LINES,
        identity: TermIdentity {
            name: "rriter",
            version: env!("CARGO_PKG_VERSION"),
        },
        palette: TermPalette {
            foreground: white,
            background: black,
            cursor: white,
            ansi: [black; 16],
        },
    })
}

/// Feeds raw output bytes into a grid the way the PTY reader does, for tests that set DECSET modes.
#[cfg(test)]
pub(crate) fn advance_test_output(grid: &mut Term, bytes: &[u8]) {
    let mut sink = crate::app::terminal_process::RriterTermSink::new(None, None);
    grid.advance(bytes, Instant::now(), &mut sink);
}

/// Feeds raw output bytes into tab `term` through the parser's batch path, so the hold deadline
/// is published for the UI thread the way the parser thread publishes it.
#[cfg(test)]
pub(crate) fn feed_test_terminal_output(term: &Terminal, bytes: &[u8]) {
    let mut grid = lock_terminal_grid(&term.grid);
    let mut sink = crate::app::terminal_process::RriterTermSink::new(None, None)
        .with_host_events(term.host_events.clone());
    crate::app::terminal_sync_output::advance_terminal_output_batch(&mut grid, &mut sink, &[bytes.to_vec()]);
}

pub(crate) fn terminal_selection_text(term: &Term) -> Option<String> {
    let (sx, sy, ex, ey) = term.selection()?;
    let (start_x, start_y, end_x, end_y) = ronsole_term::normalized_selection_bounds(sx, sy, ex, ey);
    let last_col = term.cols().saturating_sub(1);
    let mut result = String::new();

    // Rows come from `Term::row`, which already accounts for the alternate screen.
    for y in start_y..=end_y {
        let Some(row) = term.row(y) else {
            continue;
        };
        let line_start = if y == start_y { start_x } else { 0 };
        let line_end = if y == end_y { end_x } else { last_col };
        for x in line_start..=line_end {
            if let Some(cell) = row.get(x) {
                cell.append_text_to(&mut result);
            }
        }
        if y != end_y {
            result.push('\n');
        }
    }

    Some(result.trim_end().to_string())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TerminalPresentationIntent {
    None,
    ActivateWhenReady,
    OpenPanelWhenReady,
}

pub struct Terminal {
    pub grid: Arc<Mutex<Term>>,
    process: Option<crate::app::terminal_process::TerminalProcess>,
    pub scroll_y: crate::scroll::ScrollState,
    pub(crate) presentation_intent: TerminalPresentationIntent,
    pub(crate) reveal_right_tail_when_presented: bool,
    pub(crate) mouse: crate::app::mouse::TerminalMouseState,
    /// Focus state last reported to the PTY (DECSET 1004); reconciled by `sync_terminal_focus_reports`.
    pub(crate) focus_sent: bool,
    /// Bytes passed to `write_input`, kept so headless tests can check the PTY stream.
    #[cfg(test)]
    pub(crate) written_input: Mutex<Vec<u8>>,
    /// Hovered link and the grid generation it was resolved against; the underline is drawn only
    /// while the generation matches. Always `None` until hover tracking fills it.
    pub(crate) hovered_link: Option<(ronsole_term::LinkTarget, u64)>,
    /// Cell and grid generation of the last hover lookup that found no link; a repeated lookup
    /// on the same cell and generation returns without walking the grid.
    pub(crate) hovered_link_miss: Option<((usize, usize), u64)>,
    /// Number of `resolve_link` calls, so tests can observe the miss cache.
    #[cfg(test)]
    pub(crate) resolve_calls: std::sync::atomic::AtomicUsize,
    title_cache: crate::app::terminal_process::TerminalTitleCache,
    /// Working directory last reported by OSC 7 for a local host; a new tab starts there.
    pub(crate) osc7_cwd: Option<std::path::PathBuf>,
    /// Latest unapplied OSC 52 and OSC 7 events from the parser thread.
    host_events: crate::app::terminal_host_events::HostEventsSink,
}

fn write_terminal_spawn_error(term: &mut Term, error: &io::Error) {
    let message = format!("RRiter terminal error: {error}\r\n");
    let mut sink = crate::app::terminal_process::RriterTermSink::new(None, None);
    term.advance(message.as_bytes(), Instant::now(), &mut sink);
    term.mark_presentation_ready();
}

impl Terminal {
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn spawn(
        window: Option<std::sync::Arc<crate::platform::WindowHost>>,
        cwd: Option<&std::path::Path>,
        display_number: u64,
        palette: TermPalette,
        ui_waker: &crate::ui_waker::UiWaker,
    ) -> Self {
        let title_cache = Arc::new(Mutex::new(
            crate::app::terminal_process::TerminalTitleState::new_numbered(
                "terminal".to_string(),
                display_number,
            ),
        ));
        let term = Term::new(TermConfig {
            cols: 200,
            rows: 60,
            scrollback_lines: TERMINAL_SCROLLBACK_LINES,
            identity: TermIdentity {
                name: "rriter",
                version: env!("CARGO_PKG_VERSION"),
            },
            palette,
        });
        let grid = Arc::new(Mutex::new(term));
        let host_events = crate::app::terminal_host_events::HostEventsSink::new(ui_waker.clone());
        let result = crate::app::terminal_process::TerminalProcess::spawn(
            grid.clone(),
            title_cache.clone(),
            window,
            cwd,
            host_events.clone(),
        );
        let process = match result {
            Ok((process, _shell)) => Some(process),
            Err(error) => {
                crate::platform::lock_recover(&title_cache)
                    .set_fallback("terminal error".to_string());
                let mut term = crate::platform::lock_recover(&grid);
                write_terminal_spawn_error(&mut term, &error);
                None
            }
        };

        Self {
            grid,
            process,
            scroll_y: crate::scroll::ScrollState::new(7.0),
            presentation_intent: TerminalPresentationIntent::None,
            reveal_right_tail_when_presented: false,
            mouse: crate::app::mouse::TerminalMouseState::default(),
            focus_sent: false,
            #[cfg(test)]
            written_input: Mutex::new(Vec::new()),
            hovered_link: None,
            hovered_link_miss: None,
            #[cfg(test)]
            resolve_calls: std::sync::atomic::AtomicUsize::new(0),
            title_cache,
            osc7_cwd: None,
            host_events,
        }
    }

    /// Takes the OSC 7 directory and returns the OSC 52 clipboard text stored since the last call.
    /// The caller sets the clipboard, because the App owns it.
    pub(crate) fn apply_host_events(&mut self) -> Option<String> {
        let events = self.host_events.take();
        if let Some(cwd) = events.cwd {
            let local_host = crate::platform::local_host_name();
            if let Some(path) = crate::platform::osc7_path(&cwd.host, &cwd.path, &local_host) {
                self.osc7_cwd = Some(path);
            }
        }
        events.clipboard
    }

    /// Hold deadline of a mode 2026 batch that has not been replayed yet. Read from the value the
    /// parser published, so it never waits for the grid lock.
    pub(crate) fn sync_deadline(&self) -> Option<Instant> {
        self.host_events.sync_deadline()
    }

    /// When the UI thread should wake to replay a hold the parser has not replayed: the deadline
    /// plus `SYNC_UI_FALLBACK_GRACE`.
    pub(crate) fn sync_wake_at(&self) -> Option<Instant> {
        self.sync_deadline()
            .and_then(|deadline| deadline.checked_add(SYNC_UI_FALLBACK_GRACE))
    }

    /// UI-thread fallback: replays a mode 2026 batch the parser has held past `SYNC_UI_FALLBACK_GRACE`
    /// after its deadline. Returns whether the grid changed.
    /// Replies go to the same PTY queue as the parser thread, so the replay keeps their order.
    pub(crate) fn flush_expired_sync(&self, now: Instant) -> bool {
        let Some(deadline) = self.sync_deadline() else {
            return false;
        };
        if !sync_ui_fallback_due(deadline, now) {
            return false;
        }
        let reply_tx = self.process.as_ref().map(|process| process.reply_sender());
        flush_expired_sync_on_grid(&self.grid, &self.title_cache, reply_tx, &self.host_events, now)
    }

    /// Queues a terminal report (focus) without blocking. A full queue or a missing process drops it.
    pub(crate) fn queue_report(&self, bytes: &[u8]) {
        // Tests read the reports the way they read `write_input`, which records before queueing.
        #[cfg(test)]
        crate::platform::lock_recover(&self.written_input).extend_from_slice(bytes);
        if let Some(process) = self.process.as_ref() {
            process.queue_report(bytes);
        }
    }

    /// A terminal without a PTY process: `write_input` only records its bytes in `written_input`.
    #[cfg(test)]
    pub(crate) fn test_without_process(cols: usize, rows: usize) -> Self {
        Self {
            grid: Arc::new(Mutex::new(test_term(cols, rows))),
            process: None,
            scroll_y: crate::scroll::ScrollState::new(7.0),
            presentation_intent: TerminalPresentationIntent::None,
            reveal_right_tail_when_presented: false,
            mouse: crate::app::mouse::TerminalMouseState::default(),
            focus_sent: false,
            written_input: Mutex::new(Vec::new()),
            hovered_link: None,
            hovered_link_miss: None,
            #[cfg(test)]
            resolve_calls: std::sync::atomic::AtomicUsize::new(0),
            title_cache: Arc::new(Mutex::new(
                crate::app::terminal_process::TerminalTitleState::new_numbered("terminal".to_string(), 1),
            )),
            osc7_cwd: None,
            host_events: crate::app::terminal_host_events::HostEventsSink::new(
                crate::ui_waker::UiWaker::counting(),
            ),
        }
    }

    pub(crate) fn write_display_title(&self, output: &mut String) {
        crate::platform::lock_recover(&self.title_cache).write_resolved(output);
    }

    pub fn write_input(&self, bytes: &[u8]) -> io::Result<()> {
        #[cfg(test)]
        crate::platform::lock_recover(&self.written_input).extend_from_slice(bytes);
        self.process
            .as_ref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "terminal is not running"))?
            .write_input(bytes)
    }

    pub fn is_closed(&mut self) -> bool {
        self.process
            .as_mut()
            .is_some_and(|process| process.try_wait().unwrap_or(true))
    }

    pub fn shutdown(&mut self) {
        if let Some(process) = self.process.as_mut() {
            process.shutdown();
        }
        self.process = None;
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn resize_pty(&self, cols: u16, rows: u16) {
        if let Some(process) = self.process.as_ref() {
            let _ = process.resize(cols, rows);
        }
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::terminal_process::RriterTermSink;

    #[test]
    fn osc_10_11_reply_uses_theme_palette() {
        let mut term = test_term(20, 3);
        let foreground = ronsole_term::Rgb {
            r: 0xab,
            g: 0xcd,
            b: 0xef,
        };
        let background = ronsole_term::Rgb {
            r: 0x12,
            g: 0x34,
            b: 0x56,
        };
        term.set_palette(TermPalette {
            foreground,
            background,
            cursor: foreground,
            ansi: [background; 16],
        });
        let (reply_tx, reply_rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(8);
        let mut sink = RriterTermSink::new(None, Some(reply_tx));
        term.advance(b"\x1b]10;?\x07\x1b]11;?\x07", Instant::now(), &mut sink);

        // One batch of replies is sent as a single message, both queries in order.
        let reply = String::from_utf8_lossy(&reply_rx.try_recv().unwrap_or_default()).into_owned();
        // xterm `rgb:` spec doubles each 8-bit channel (ronsole-term osc.rs `rgb_spec`).
        assert!(reply.contains("rgb:abab/cdcd/efef"), "{reply}");
        assert!(reply.contains("rgb:1212/3434/5656"), "{reply}");
        assert!(!reply.contains("ffff"), "{reply}");
    }

    #[test]
    fn resize_terminal_grid_drops_selection_only_on_size_change() {
        let mut term = test_term(8, 2);
        term.set_selection(Some((0, 0, 2, 0)));

        assert!(!resize_terminal_grid(&mut term, 8, 2));
        assert_eq!(term.selection(), Some((0, 0, 2, 0)));

        assert!(resize_terminal_grid(&mut term, 4, 2));
        assert_eq!(term.selection(), None);
        assert_eq!((term.cols(), term.rows()), (4, 2));

        term.set_selection(Some((0, 0, 1, 0)));
        assert!(resize_terminal_grid(&mut term, 4, 3));
        assert_eq!(term.selection(), None);
        assert_eq!(term.rows(), 3);
    }

    #[test]
    fn selection_text_in_alt_screen() {
        let mut term = test_term(20, 3);
        let mut sink = RriterTermSink::new(None, None);
        // Five newlines on three rows push lines into scrollback before the alt screen.
        term.advance(b"a\r\nb\r\nc\r\nd\r\ne\r\n", Instant::now(), &mut sink);
        assert!(term.scrollback_len() > 0);
        term.advance(b"\x1b[?1049hhello world", Instant::now(), &mut sink);
        assert!(term.is_alt_screen());
        term.set_selection(Some((0, 0, 4, 0)));
        assert_eq!(terminal_selection_text(&term).as_deref(), Some("hello"));
    }

    #[test]
    fn selection_text_skips_wide_spacers_and_keeps_zero_width() {
        let mut term = test_term(20, 3);
        let mut sink = RriterTermSink::new(None, None);
        // Wide CJK glyphs occupy two cells each; the spacer cell must not add a space.
        term.advance("日本e\u{301}".as_bytes(), Instant::now(), &mut sink);
        term.set_selection(Some((0, 0, 4, 0)));
        assert_eq!(terminal_selection_text(&term).as_deref(), Some("日本e\u{301}"));
    }

    #[test]
    fn reply_from_flush_reaches_writer() {
        let grid = Mutex::new(test_term(20, 2));
        let title_cache = Arc::new(Mutex::new(
            crate::app::terminal_process::TerminalTitleState::new("shell".to_string()),
        ));
        let host_events = crate::app::terminal_host_events::HostEventsSink::new(
            crate::ui_waker::UiWaker::counting(),
        );
        let (reply_tx, reply_rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(8);
        let now = Instant::now();
        // The parser thread holds this batch without a reply queue; the CPR is produced only by the replay.
        let mut parser_sink = RriterTermSink::new(None, None);
        lock_terminal_grid(&grid).advance(b"\x1b[?2026h\x1b[6n", now, &mut parser_sink);
        let deadline = lock_terminal_grid(&grid)
            .sync_deadline()
            .expect("mode 2026 holds the output");
        assert!(reply_rx.try_recv().is_err());

        assert!(flush_expired_sync_on_grid(
            &grid,
            &title_cache,
            Some(reply_tx),
            &host_events,
            deadline
        ));
        // Cursor at home: CPR reply is CSI row;col R with 1-based coordinates (xterm ctlseqs).
        assert_eq!(reply_rx.try_recv().ok(), Some(b"\x1b[1;1R".to_vec()));
    }

    use std::time::Duration;

    /// Holds one mode 2026 batch in `term` through a sink that publishes to its host events.
    fn hold_batch(term: &Terminal) -> Instant {
        let mut sink = RriterTermSink::new(None, None).with_host_events(term.host_events.clone());
        let mut grid = lock_terminal_grid(&term.grid);
        crate::app::terminal_sync_output::advance_terminal_output_batch(
            &mut grid,
            &mut sink,
            &[b"\x1b[?2026hheld".to_vec()],
        );
        drop(grid);
        term.sync_deadline().expect("mode 2026 holds the output")
    }

    #[test]
    fn ui_fallback_replays_only_after_grace_past_deadline() {
        let term = Terminal::test_without_process(20, 2);
        let deadline = hold_batch(&term);

        let inside_grace = deadline + SYNC_UI_FALLBACK_GRACE - Duration::from_millis(1);
        assert!(!term.flush_expired_sync(inside_grace));
        assert_eq!(term.sync_deadline(), Some(deadline), "UI replayed inside the grace");

        assert!(term.flush_expired_sync(deadline + SYNC_UI_FALLBACK_GRACE));
        assert_eq!(term.sync_deadline(), None);
    }

    #[test]
    fn sync_wake_at_reads_published_deadline_without_grid_lock() {
        let term = Terminal::test_without_process(20, 2);
        let deadline = hold_batch(&term);

        let grid = term.grid.clone();
        let (locked_tx, locked_rx) = std::sync::mpsc::channel::<()>();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let holder = std::thread::spawn(move || {
            let _guard = lock_terminal_grid(&grid);
            let _ = locked_tx.send(());
            let _ = release_rx.recv_timeout(Duration::from_secs(3));
        });
        locked_rx.recv().expect("holder thread took the grid lock");

        let started = Instant::now();
        let wake_at = term.sync_wake_at();
        // A read that takes the grid lock would wait for the holder to release it.
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "sync_wake_at waited for the grid lock"
        );
        assert_eq!(wake_at, Some(deadline + SYNC_UI_FALLBACK_GRACE));

        let _ = release_tx.send(());
        holder.join().expect("holder thread");
    }

    #[test]
    fn queue_report_without_process_is_a_no_op() {
        let term = Terminal::test_without_process(20, 2);
        // Focus reports must never block the UI thread, even with no PTY behind the tab.
        term.queue_report(b"\x1b[O");
    }
}
