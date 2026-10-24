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
                result.push(cell.ch());
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
    title_cache: crate::app::terminal_process::TerminalTitleCache,
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
        let result = crate::app::terminal_process::TerminalProcess::spawn(
            grid.clone(),
            title_cache.clone(),
            window,
            cwd,
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
            title_cache,
        }
    }

    pub(crate) fn write_display_title(&self, output: &mut String) {
        crate::platform::lock_recover(&self.title_cache).write_resolved(output);
    }

    pub fn write_input(&self, bytes: &[u8]) -> io::Result<()> {
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
    fn selection_text_in_alt_screen() {
        let mut term = test_term(20, 3);
        let mut sink = RriterTermSink::new(None, None);
        term.advance(b"\x1b[?1049hhello world", Instant::now(), &mut sink);
        term.set_selection(Some((0, 0, 4, 0)));
        assert_eq!(terminal_selection_text(&term).as_deref(), Some("hello"));
    }
}
