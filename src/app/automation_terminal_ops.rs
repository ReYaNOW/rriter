//! `terminal_ops` PGO group: opens the terminal panel, adds a second terminal, floods it with
//! 20 000 lines of `seq` output, searches the output, switches between the tabs and closes the
//! second one. The shell runs through the managed terminal path; the command touches neither
//! the network nor `HOME`.

use std::path::Path;

use crate::app::automation::{AutomationButton, AutomationStep, AutomationTarget};
use crate::app::{App, PanelId};
use crate::ui_system::UiId;

/// Last line printed by `seq 1 20000`.
const LAST_LINE: &str = "20000";
/// Rows inspected at the end of the scrollback besides the visible screen: the prompt line
/// that follows the output can push the last number a row or two up.
const SCROLLBACK_TAIL_ROWS: usize = 4;

/// The shell and `seq` are the unix ones; Windows has no equivalent workload here.
pub(super) fn requires(_app: &App) -> Result<(), String> {
    requires_with(cfg!(unix))
}

fn requires_with(unix: bool) -> Result<(), String> {
    if unix { Ok(()) } else { Err("terminal workload is unix-only".to_string()) }
}

fn click(id: UiId) -> AutomationStep {
    AutomationStep::Click {
        at: AutomationTarget::Ui(id),
        button: AutomationButton::Left,
        mods: "",
        clicks: 1,
    }
}

fn ui_visible(app: &App, id: UiId) -> bool {
    app.ui_registry.element_hits().any(|(hit_id, _, rect, _)| hit_id == id && rect.is_some())
}

fn row_is(row: &[crate::app::terminal::Cell], expected: &str) -> bool {
    let mut cells = row.iter().map(|cell| cell.c).skip_while(|c| c.is_whitespace());
    let mut expected = expected.chars();
    loop {
        match (cells.next(), expected.next()) {
            (Some(actual), Some(want)) if actual == want => {}
            (None, None) => return true,
            (Some(rest), None) => {
                return rest.is_whitespace() && cells.all(char::is_whitespace);
            }
            _ => return false,
        }
    }
}

/// `seq` finished: a row holding only the last number is at the end of the active terminal.
/// The typed command line contains the number too, but never as a whole row.
fn output_done(app: &App) -> bool {
    let Some(terminal) = app.ide_panel.terminals.get(app.ide_panel.active_terminal) else {
        return false;
    };
    let grid = crate::app::terminal::lock_terminal_grid(&terminal.grid);
    grid.lines
        .iter()
        .chain(grid.scrollback.iter().rev().take(SCROLLBACK_TAIL_ROWS))
        .any(|row| row_is(row, LAST_LINE))
}

pub(super) fn steps(_workspace: &Path) -> Vec<AutomationStep> {
    use AutomationStep as S;
    vec![
        S::OpenPanel(PanelId::Terminal),
        S::WaitTerminal,
        S::WaitUntil {
            what: "terminal add button",
            check: |app| ui_visible(app, UiId::TerminalAdd),
            timeout_ms: 10_000,
        },
        click(UiId::TerminalAdd),
        S::WaitUntil {
            what: "second terminal tab",
            check: |app| {
                app.ide_panel.terminals.len() == 2 && ui_visible(app, UiId::TerminalTab(1))
            },
            timeout_ms: 10_000,
        },
        click(UiId::TerminalTab(1)),
        S::WaitUntil {
            what: "second terminal active",
            check: |app| app.ide_panel.active_terminal == 1 && ui_visible(app, UiId::TerminalBody),
            timeout_ms: 5_000,
        },
        click(UiId::TerminalBody),
        S::WaitUntil {
            what: "terminal focused",
            check: |app| app.ide_panel.terminal_focused,
            timeout_ms: 5_000,
        },
        S::TypeText("seq 1 20000"),
        S::Key("enter"),
        S::WaitUntil { what: "output done", check: output_done, timeout_ms: 30_000 },
        S::Key("ctrl+f"),
        S::TypeText("1999"),
        S::WaitUntil {
            what: "terminal search shown",
            check: |app| app.ide_panel.term_show_search,
            timeout_ms: 5_000,
        },
        S::WaitFrames(2),
        S::Key("escape"),
        S::WaitFrames(2),
        click(UiId::TerminalTab(0)),
        S::WaitUntil {
            what: "first terminal active",
            check: |app| app.ide_panel.active_terminal == 0,
            timeout_ms: 5_000,
        },
        click(UiId::TerminalTab(1)),
        S::WaitUntil {
            what: "second terminal active again",
            check: |app| app.ide_panel.active_terminal == 1,
            timeout_ms: 5_000,
        },
        click(UiId::TerminalTabClose(1)),
        S::WaitUntil {
            what: "second terminal closed",
            check: |app| app.ide_panel.terminals.len() == 1,
            timeout_ms: 5_000,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::terminal::Cell;

    fn row(text: &str) -> Vec<Cell> {
        text.chars()
            .map(|c| Cell { c, fg: 0, bg: 0, presentation: 0 })
            .collect()
    }

    #[test]
    fn non_unix_skips_the_group_with_a_reason() {
        assert_eq!(requires_with(false), Err("terminal workload is unix-only".to_string()));
        assert_eq!(requires_with(true), Ok(()));
    }

    #[test]
    fn the_last_number_matches_only_as_a_whole_row() {
        assert!(row_is(&row("20000"), LAST_LINE));
        assert!(row_is(&row("20000   "), LAST_LINE));
        assert!(!row_is(&row("seq 1 20000"), LAST_LINE));
        assert!(!row_is(&row("200001"), LAST_LINE));
        assert!(!row_is(&row("2000"), LAST_LINE));
    }
}
