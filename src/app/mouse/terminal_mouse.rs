//! Terminal mouse and focus reporting: encodes host pointer events and window
//! focus changes for the PTY according to the application's DECSET modes.
//! Byte encoding is delegated to `ronsole_term`; this module only routes.

use crate::app::PanelId;
use crate::app::App;
use crate::app::terminal::{Terminal, lock_terminal_grid};
use crate::ui_system::UiId;
use ronsole_term::{MouseButton, MouseEventKind, Term, mouse_report};
use winit::event::ElementState;

/// Per-tab mouse state: the buttons whose press was forwarded to the PTY and whose release is still owed, and the last cell reported.
#[derive(Default)]
pub(crate) struct TerminalMouseState {
    /// One bit per button (see `button_bit`).
    held_buttons: u8,
    last_cell: Option<(usize, usize)>,
}

impl TerminalMouseState {
    /// Records a forwarded press or release of `btn`; other buttons keep their bits.
    fn update_held(&mut self, pressed: bool, btn: MouseButton) {
        if pressed {
            self.held_buttons |= button_bit(btn);
        } else {
            self.held_buttons &= !button_bit(btn);
        }
    }

    /// Whether the press of `btn` was forwarded and its release is still owed.
    fn holds(&self, btn: MouseButton) -> bool {
        self.held_buttons & button_bit(btn) != 0
    }

    /// Button reported with motion in button-motion modes: the first held one in `Left, Middle, Right` order.
    fn motion_button(&self) -> Option<MouseButton> {
        [MouseButton::Left, MouseButton::Middle, MouseButton::Right]
            .into_iter()
            .find(|&btn| self.holds(btn))
    }

    /// Forgets every held button when the host drops the gesture (focus loss, tab switch, cancelled drag).
    pub(crate) fn clear_held(&mut self) {
        self.held_buttons = 0;
    }
}

fn button_bit(btn: MouseButton) -> u8 {
    match btn {
        MouseButton::Left => 1,
        MouseButton::Middle => 2,
        MouseButton::Right => 4,
    }
}

fn terminal_mouse_button(button: winit::event::MouseButton) -> Option<MouseButton> {
    match button {
        winit::event::MouseButton::Left => Some(MouseButton::Left),
        winit::event::MouseButton::Middle => Some(MouseButton::Middle),
        winit::event::MouseButton::Right => Some(MouseButton::Right),
        _ => None,
    }
}

/// Desired DECSET 1004 state of one tab: window, IDE mode, terminal panel and tab must all be focused.
pub(crate) fn terminal_focus_desired(
    window_focused: bool,
    is_ide_mode: bool,
    panel_focused: bool,
    panel_open: bool,
    is_active: bool,
) -> bool {
    window_focused && is_ide_mode && panel_focused && panel_open && is_active
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
    /// and the application enabled mouse tracking. Tracking takes every pointer event, except
    /// the link modifier over a link: that stays local (see `terminal_link_at_pointer`).
    fn terminal_tracking_target(&mut self, mx: f32, my: f32) -> Option<usize> {
        if !self.is_ide_mode
            || !self.ide_panel.is_open(PanelId::Terminal)
            || !self.ide_panel.terminal_focused
            || self.ui_registry.find_at(mx, my) != Some(UiId::TerminalBody)
        {
            return None;
        }
        let active = self.ide_panel.active_terminal;
        let term = self.ide_panel.terminals.get(active)?;
        let tracking = lock_terminal_grid(&term.grid).mouse_tracking().enabled();
        if !tracking || self.terminal_link_at_pointer(mx, my).is_some() {
            return None;
        }
        Some(active)
    }

    /// Whether the link modifier (Ctrl, or Command on macOS) is held.
    fn terminal_link_modifier_held(&self) -> bool {
        crate::platform::primary_modifier_for_platform(
            crate::platform::CURRENT_PLATFORM,
            self.modifiers.control_key(),
            self.modifiers.super_key(),
        )
    }

    /// URI of the link under the pointer on the active terminal body; `None` without the link modifier.
    fn terminal_link_at_pointer(&mut self, mx: f32, my: f32) -> Option<String> {
        if !self.terminal_link_modifier_held()
            || self.ui_registry.find_at(mx, my) != Some(UiId::TerminalBody)
        {
            return None;
        }
        let cell = self.terminal_mouse_cell(mx, my)?;
        let term = self.ide_panel.terminals.get(self.ide_panel.active_terminal)?;
        term.resolve_link(cell).map(|(link, _)| link.uri.to_string())
    }

    /// Re-resolves the hovered link of the active terminal for the pointer. The link is cleared
    /// without the link modifier or off the body. Returns `true` when the stored link changed.
    pub(crate) fn refresh_terminal_link_hover(&mut self, mx: f32, my: f32) -> bool {
        let cell = if self.terminal_link_modifier_held()
            && self.ui_registry.find_at(mx, my) == Some(UiId::TerminalBody)
        {
            self.terminal_mouse_cell(mx, my)
        } else {
            None
        };
        let active = self.ide_panel.active_terminal;
        self.ide_panel
            .terminals
            .get_mut(active)
            .is_some_and(|term| term.update_hovered_link(cell))
    }

    /// Refreshes the hover from the last known pointer position, once per frame, so that a
    /// Ctrl change is shown without waiting for a pointer motion. Returns `true` when the link changed.
    pub(crate) fn refresh_terminal_link_hover_at_pointer(&mut self) -> bool {
        let (mx, my) = self.renderer.as_ref().map_or((-1.0, -1.0), |renderer| {
            (renderer.last_mouse_x, renderer.last_mouse_y)
        });
        self.refresh_terminal_link_hover(mx, my)
    }

    /// Whether the pointer shows the link cursor: the link modifier is held over a hovered link of the active terminal.
    pub(crate) fn terminal_link_cursor_active(&self) -> bool {
        self.terminal_link_modifier_held()
            && self
                .ide_panel
                .terminals
                .get(self.ide_panel.active_terminal)
                .is_some_and(|term| term.hovered_link.is_some())
    }

    /// Whether the active terminal has mouse tracking on; a pointer drag then belongs to the PTY, not to a local selection.
    pub(crate) fn active_terminal_tracks_mouse(&self) -> bool {
        self.ide_panel
            .terminals
            .get(self.ide_panel.active_terminal)
            .is_some_and(|term| lock_terminal_grid(&term.grid).mouse_tracking().enabled())
    }

    /// Terminal that takes a release: the active one, only when it holds this button's press.
    /// A release whose press never reached the PTY is not forwarded.
    fn terminal_release_target(&self, button: winit::event::MouseButton) -> Option<usize> {
        let btn = terminal_mouse_button(button)?;
        let active = self.ide_panel.active_terminal;
        let term = self.ide_panel.terminals.get(active)?;
        term.mouse.holds(btn).then_some(active)
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
        // Ctrl+click on a link opens it and is never forwarded, even under tracking; no selection starts.
        let link = if pressed && button == winit::event::MouseButton::Left {
            self.terminal_link_at_pointer(mx, my)
        } else {
            None
        };
        if let Some(uri) = link {
            crate::platform::open_terminal_uri(self.external_requests.sink(), &uri);
            return true;
        }
        let target = if pressed {
            self.terminal_tracking_target(mx, my)
        } else {
            self.terminal_release_target(button)
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
        term.mouse.update_held(pressed, btn);
        term.mouse.last_cell = Some(cell);
        let forwarded = report.is_some();
        if let Some(bytes) = report {
            let _ = term.write_input(&bytes);
        }
        // A release that tracking no longer encodes (mode turned off mid-drag) falls through
        // to the panel drag cleanup instead of being swallowed.
        pressed || forwarded
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
            held: term.mouse.motion_button(),
        };
        // Encode under the grid lock, but write after it is released: a blocking PTY write
        // must not hold the grid while the parser thread waits for it.
        let report = terminal_mouse_report(&lock_terminal_grid(&term.grid), kind, cell);
        if let Some(bytes) = report {
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
        let is_ide_mode = self.is_ide_mode;
        for (index, term) in self.ide_panel.terminals.iter_mut().enumerate() {
            let desired = terminal_focus_desired(
                window_focused,
                is_ide_mode,
                panel_focused,
                panel_open,
                index == active,
            );
            if !desired {
                term.mouse.clear_held();
            }
            set_focus_report(term, desired);
        }
    }

    /// Sends focus-out to tab `index` before the tab is removed, so an application that asked for DECSET 1004 sees it leave.
    pub(crate) fn send_terminal_focus_out(&mut self, index: usize) {
        if let Some(term) = self.ide_panel.terminals.get_mut(index) {
            set_focus_report(term, false);
        }
    }
}

/// Writes `CSI I` / `CSI O` to one tab when its desired DECSET 1004 state changes and the application enabled reporting.
fn set_focus_report(term: &mut Terminal, desired: bool) {
    if desired == term.focus_sent {
        return;
    }
    term.focus_sent = desired;
    let report = lock_terminal_grid(&term.grid)
        .focus_changed(desired)
        .map(<[u8]>::to_vec);
    if let Some(bytes) = report {
        let _ = term.write_input(&bytes);
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
        assert!(terminal_focus_desired(true, true, true, true, true));
        assert!(!terminal_focus_desired(false, true, true, true, true));
        assert!(!terminal_focus_desired(true, false, true, true, true));
        assert!(!terminal_focus_desired(true, true, false, true, true));
        assert!(!terminal_focus_desired(true, true, true, false, true));
        assert!(!terminal_focus_desired(true, true, true, true, false));
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
