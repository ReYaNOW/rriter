//! Terminal mouse and focus reporting: encodes host pointer events and window
//! focus changes for the PTY according to the application's DECSET modes.
//! Byte encoding is delegated to `ronsole_term`; this module only routes.

use crate::app::PanelId;
use crate::app::App;
use crate::app::terminal::lock_terminal_grid;
use crate::ui_system::UiId;
use ronsole_term::{MouseButton, MouseEventKind, Term, mouse_report};
use winit::event::ElementState;

/// Per-tab mouse state: the button held over the body and the last cell reported.
#[derive(Default)]
pub(crate) struct TerminalMouseState {
    held: Option<MouseButton>,
    last_cell: Option<(usize, usize)>,
}

fn terminal_mouse_button(button: winit::event::MouseButton) -> Option<MouseButton> {
    match button {
        winit::event::MouseButton::Left => Some(MouseButton::Left),
        winit::event::MouseButton::Middle => Some(MouseButton::Middle),
        winit::event::MouseButton::Right => Some(MouseButton::Right),
        _ => None,
    }
}

/// Desired DECSET 1004 state of one tab: window, terminal panel and tab must all be focused.
pub(crate) fn terminal_focus_desired(
    window_focused: bool,
    panel_focused: bool,
    panel_open: bool,
    is_active: bool,
) -> bool {
    window_focused && panel_focused && panel_open && is_active
}

/// Number of arrow keys sent for one wheel event without mouse tracking.
pub(crate) fn terminal_wheel_steps(dy: f32) -> usize {
    ((dy.abs() / 20.0).max(1.0) as usize).min(3)
}

/// Arrow-key bytes for a wheel event on the alternate screen without mouse tracking.
pub(crate) fn terminal_wheel_arrow_bytes(up: bool, app_cursor: bool, steps: usize) -> Vec<u8> {
    let sequence: &[u8] = match (up, app_cursor) {
        (true, true) => b"\x1bOA",
        (true, false) => b"\x1b[A",
        (false, true) => b"\x1bOB",
        (false, false) => b"\x1b[B",
    };
    sequence.repeat(steps)
}

/// Encodes one pointer event for the grid's tracking mode; `None` when the application did not request it.
/// `cell` is `(column, row from the top of the visible area)`, as returned by `App::terminal_report_cell`.
pub(crate) fn terminal_mouse_report(
    grid: &Term,
    kind: MouseEventKind,
    cell: (usize, usize),
) -> Option<Vec<u8>> {
    mouse_report(grid.mouse_tracking(), grid.mouse_sgr(), kind, cell.0, cell.1)
}

impl App {
    /// Active terminal index when the pointer is over its body, the panel is focused,
    /// and the application enabled mouse tracking. Tracking takes every pointer event (no modifier bypass).
    fn terminal_tracking_target(&self, mx: f32, my: f32) -> Option<usize> {
        if !self.is_ide_mode
            || !self.ide_panel.is_open(PanelId::Terminal)
            || !self.ide_panel.terminal_focused
            || self.ui_registry.find_at(mx, my) != Some(UiId::TerminalBody)
        {
            return None;
        }
        let active = self.ide_panel.active_terminal;
        let term = self.ide_panel.terminals.get(active)?;
        lock_terminal_grid(&term.grid)
            .mouse_tracking()
            .enabled()
            .then_some(active)
    }

    /// Terminal that takes a release: the tracking terminal under the pointer, or the active
    /// one when it still holds this button's press, so a drag that left the body ends in the PTY.
    fn terminal_release_target(
        &self,
        button: winit::event::MouseButton,
        mx: f32,
        my: f32,
    ) -> Option<usize> {
        if let Some(active) = self.terminal_tracking_target(mx, my) {
            return Some(active);
        }
        let btn = terminal_mouse_button(button)?;
        let active = self.ide_panel.active_terminal;
        let term = self.ide_panel.terminals.get(active)?;
        (term.mouse.held == Some(btn)
            && lock_terminal_grid(&term.grid).mouse_tracking().enabled())
        .then_some(active)
    }

    /// Forwards a press or release over a tracking terminal to the PTY. Returns `true`
    /// when the event belongs to the terminal and must not start a local selection.
    pub(super) fn forward_terminal_mouse_button(
        &mut self,
        state: ElementState,
        button: winit::event::MouseButton,
        mx: f32,
        my: f32,
    ) -> bool {
        let pressed = state == ElementState::Pressed;
        let target = if pressed {
            self.terminal_tracking_target(mx, my)
        } else {
            self.terminal_release_target(button, mx, my)
        };
        let Some(active) = target else {
            return false;
        };
        let Some(cell) = self.terminal_report_cell(mx, my) else {
            return true;
        };
        let Some(btn) = terminal_mouse_button(button) else {
            return true;
        };
        let Some(term) = self.ide_panel.terminals.get_mut(active) else {
            return true;
        };
        let kind = if pressed {
            MouseEventKind::Press(btn)
        } else {
            MouseEventKind::Release(btn)
        };
        let report = terminal_mouse_report(&lock_terminal_grid(&term.grid), kind, cell);
        term.mouse.held = pressed.then_some(btn);
        term.mouse.last_cell = Some(cell);
        if let Some(bytes) = report {
            let _ = term.write_input(&bytes);
        }
        true
    }

    /// Reports pointer motion over a tracking terminal, once per cell. Returns `true`
    /// when the terminal consumed the move.
    pub(crate) fn forward_terminal_mouse_motion(&mut self, mx: f32, my: f32) -> bool {
        let Some(active) = self.terminal_tracking_target(mx, my) else {
            return false;
        };
        let Some(cell) = self.terminal_report_cell(mx, my) else {
            return true;
        };
        let Some(term) = self.ide_panel.terminals.get_mut(active) else {
            return true;
        };
        if term.mouse.last_cell == Some(cell) {
            return true;
        }
        term.mouse.last_cell = Some(cell);
        let kind = MouseEventKind::Motion {
            held: term.mouse.held,
        };
        if let Some(bytes) = terminal_mouse_report(&lock_terminal_grid(&term.grid), kind, cell) {
            let _ = term.write_input(&bytes);
        }
        true
    }

    /// Single point that reconciles every tab's DECSET 1004 state with the host focus.
    /// Sends `CSI I` / `CSI O` only on transitions, and only when the application enabled reporting.
    pub(crate) fn sync_terminal_focus_reports(&mut self) {
        let window_focused = self.is_focused;
        let panel_focused = self.ide_panel.terminal_focused;
        let panel_open = self.ide_panel.is_open(PanelId::Terminal);
        let active = self.ide_panel.active_terminal;
        for (index, term) in self.ide_panel.terminals.iter_mut().enumerate() {
            let desired = terminal_focus_desired(
                window_focused,
                panel_focused,
                panel_open,
                index == active,
            );
            if desired == term.focus_sent {
                continue;
            }
            term.focus_sent = desired;
            let report = lock_terminal_grid(&term.grid)
                .focus_changed(desired)
                .map(<[u8]>::to_vec);
            if let Some(bytes) = report {
                let _ = term.write_input(&bytes);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::terminal::test_term;
    use crate::app::terminal_process::RriterTermSink;
    use std::time::Instant;

    fn term_with_modes(modes: &[u8]) -> Term {
        let mut term = test_term(10, 5);
        term.advance(modes, Instant::now(), &mut RriterTermSink::new(None, None));
        term
    }

    #[test]
    fn press_and_release_report_real_cell_in_sgr() {
        let term = term_with_modes(b"\x1b[?1000h\x1b[?1006h");
        let press = terminal_mouse_report(&term, MouseEventKind::Press(MouseButton::Left), (4, 2));
        let release =
            terminal_mouse_report(&term, MouseEventKind::Release(MouseButton::Left), (4, 2));
        assert_eq!(press.as_deref(), Some(b"\x1b[<0;5;3M".as_slice()));
        assert_eq!(release.as_deref(), Some(b"\x1b[<0;5;3m".as_slice()));
    }

    #[test]
    fn motion_follows_tracking_mode_and_held_button() {
        // Expected bytes from the protocol reference: 1002 reports motion only with a held button,
        // 1003 also without one (code 35 = 3 + 32); a held button adds 32 to its code.
        let motion_1002 = term_with_modes(b"\x1b[?1002h\x1b[?1006h");
        let motion_1003 = term_with_modes(b"\x1b[?1003h\x1b[?1006h");
        let none = MouseEventKind::Motion { held: None };
        let left = MouseEventKind::Motion {
            held: Some(MouseButton::Left),
        };
        assert_eq!(terminal_mouse_report(&motion_1002, none, (4, 2)), None);
        assert_eq!(
            terminal_mouse_report(&motion_1003, none, (4, 2)).as_deref(),
            Some(b"\x1b[<35;5;3M".as_slice())
        );
        assert_eq!(
            terminal_mouse_report(&motion_1002, left, (4, 2)).as_deref(),
            Some(b"\x1b[<32;5;3M".as_slice())
        );
        assert_eq!(
            terminal_mouse_report(&motion_1003, left, (4, 2)).as_deref(),
            Some(b"\x1b[<32;5;3M".as_slice())
        );
    }

    #[test]
    fn no_report_without_tracking() {
        let term = term_with_modes(b"");
        assert_eq!(
            terminal_mouse_report(&term, MouseEventKind::Press(MouseButton::Left), (0, 0)),
            None
        );
    }

    #[test]
    fn wheel_arrows_respect_cursor_mode_and_step_cap() {
        assert_eq!(terminal_wheel_arrow_bytes(true, false, 3), b"\x1b[A\x1b[A\x1b[A");
        assert_eq!(terminal_wheel_arrow_bytes(true, true, 1), b"\x1bOA");
        assert_eq!(terminal_wheel_arrow_bytes(false, false, 1), b"\x1b[B");
        assert_eq!(terminal_wheel_arrow_bytes(false, true, 1), b"\x1bOB");
        assert_eq!(terminal_wheel_steps(500.0), 3);
        assert_eq!(terminal_wheel_steps(5.0), 1);
    }

    #[test]
    fn focus_desired_requires_window_panel_open_and_active_tab() {
        assert!(terminal_focus_desired(true, true, true, true));
        assert!(!terminal_focus_desired(false, true, true, true));
        assert!(!terminal_focus_desired(true, false, true, true));
        assert!(!terminal_focus_desired(true, true, false, true));
        assert!(!terminal_focus_desired(true, true, true, false));
    }

    #[test]
    fn focus_transitions_report_only_when_mode_1004_is_on() {
        let mut on = term_with_modes(b"\x1b[?1004h");
        assert_eq!(on.focus_changed(false), Some(b"\x1b[O".as_slice()));
        assert_eq!(on.focus_changed(true), Some(b"\x1b[I".as_slice()));
        let mut off = term_with_modes(b"");
        assert_eq!(off.focus_changed(true), None);
    }
}
