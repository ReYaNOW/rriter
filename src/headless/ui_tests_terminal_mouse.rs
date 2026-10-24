//! Headless coverage for terminal mouse reports and DECSET 1004 focus reports.
//! Expected bytes come from the protocol spec (SGR `ESC[<b;x;yM/m`, motion code +32,
//! focus `ESC[I`/`ESC[O`), not from the encoder; the test reads what the app writes to the PTY.

use crate::app::terminal::lock_terminal_grid;
use crate::headless::HeadlessSession;
use crate::headless::tests_support::{
    click_ui, dump, feed, run_script, shell_failed, terminal_session, ui_rect, wait_until, written,
};
use crate::render_view::terminal_ui::{TERMINAL_TEXT_SCALE, terminal_text_padding};
use winit::keyboard::ModifiersState;

fn run(session: &mut HeadlessSession, script: &str) {
    let lines = run_script(session, script.as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
}

/// Wheel up into scrollback (positive dy, as in the terminal scrollbar test), then wait until the scroll settles.
fn scroll_back_into_history(session: &mut HeadlessSession, x: f64, y: f64) {
    run(session, &format!("mouse_move {x} {y}\nwheel 0 3\n"));
    wait_until(session, 5000, "terminal scrolled into history", |session| {
        let scroll = &session.app.ide_panel.terminals[0].scroll_y;
        scroll.is_settled() && scroll.current > 0.0
    });
}

/// Clicks tab `index` and waits until it is the active terminal, so tracking set on it is the one the pointer reaches.
fn activate_terminal_tab(session: &mut HeadlessSession, index: usize) {
    click_ui(session, &format!("TerminalTab({index})"));
    wait_until(session, 5000, "terminal tab activated", |session| session.app.ide_panel.active_terminal == index);
    assert_eq!(session.app.ide_panel.active_terminal, index);
}

fn char_metrics(session: &HeadlessSession) -> (f32, f32, f32) {
    let renderer = session.app.renderer.as_ref().unwrap_or_else(|| panic!("headless renderer"));
    let char_w = renderer.ascii_advances['A' as usize].max(1.0) * TERMINAL_TEXT_SCALE;
    let char_h = renderer.line_height * TERMINAL_TEXT_SCALE;
    (renderer.scale_factor, char_w, char_h)
}

/// Pointer position at the centre of visible cell `(col, row)` on a grid that is not scrolled back.
/// The position is found through the app's own cell mapping, so the test checks the same geometry.
fn point_for_visible_cell(session: &mut HeadlessSession, col: usize, row: usize) -> (f64, f64) {
    let body = ui_rect(&dump(session), "TerminalBody");
    let (scale, char_w, _) = char_metrics(session);
    let x = body[0] as f32 + 10.0 * scale + (col as f32 + 0.5) * char_w;
    let index = session.app.ide_panel.active_terminal;
    let scrollback = lock_terminal_grid(&session.app.ide_panel.terminals[index].grid).scrollback_len();
    let target = Some((col, scrollback + row));
    let mut y = body[1] as f32;
    let bottom = (body[1] + body[3]) as f32;
    while y < bottom {
        if session.app.terminal_mouse_cell(x, y) == target {
            return (x as f64, y as f64);
        }
        y += 0.5;
    }
    panic!("no pointer position maps to visible cell {col},{row}");
}

/// Pointer position at the centre of the bottom visible row, whatever the scroll offset.
fn point_for_bottom_row(session: &mut HeadlessSession, col: usize) -> (f64, f64) {
    let body = ui_rect(&dump(session), "TerminalBody");
    let (scale, char_w, char_h) = char_metrics(session);
    let (_, bottom_pad) = terminal_text_padding(scale);
    let x = body[0] as f32 + 10.0 * scale + (col as f32 + 0.5) * char_w;
    let y = (body[1] + body[3]) as f32 - bottom_pad - char_h * 0.5;
    (x as f64, y as f64)
}

fn open_shell_terminal(name: &str) -> Option<(std::path::PathBuf, HeadlessSession)> {
    let (dir, session) = terminal_session(name);
    if shell_failed(&session, 0) {
        let _ = std::fs::remove_dir_all(dir);
        return None;
    }
    Some((dir, session))
}

#[test]
fn mouse_press_reports_real_cell_sgr() {
    let Some((dir, mut session)) = open_shell_terminal("ui-terminal-mouse-press") else {
        return;
    };
    feed(&session, 0, b"\x1b[?1000h\x1b[?1006h");
    let (x, y) = point_for_visible_cell(&mut session, 4, 2);
    written(&session, 0);
    run(&mut session, &format!("mouse_move {x} {y}\nclick left down\nclick left up\n"));
    assert_eq!(written(&session, 0), b"\x1b[<0;5;3M\x1b[<0;5;3m");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn shift_press_with_tracking_goes_to_pty() {
    let Some((dir, mut session)) = open_shell_terminal("ui-terminal-mouse-shift") else {
        return;
    };
    feed(&session, 0, b"\x1b[?1000h\x1b[?1006h");
    let (x, y) = point_for_visible_cell(&mut session, 4, 2);
    written(&session, 0);
    session.app.modifiers = ModifiersState::SHIFT;
    run(&mut session, &format!("mouse_move {x} {y}\nclick left down\nclick left up\n"));
    session.app.modifiers = ModifiersState::empty();
    assert_eq!(written(&session, 0), b"\x1b[<0;5;3M\x1b[<0;5;3m");
    let grid = lock_terminal_grid(&session.app.ide_panel.terminals[0].grid);
    assert!(grid.selection().is_none(), "tracking must not start a local selection");
    drop(grid);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn wheel_without_tracking_main_screen_scrolls() {
    let Some((dir, mut session)) = open_shell_terminal("ui-terminal-wheel-main") else {
        return;
    };
    let lines: Vec<u8> = b"line\r\n".repeat(120);
    feed(&session, 0, &lines);
    let (x, y) = point_for_visible_cell(&mut session, 4, 2);
    written(&session, 0);
    scroll_back_into_history(&mut session, x, y);
    assert!(written(&session, 0).is_empty(), "no PTY input without tracking on the main screen");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn focus_report_on_window_and_panel_focus() {
    let Some((dir, mut session)) = open_shell_terminal("ui-terminal-focus-panel") else {
        return;
    };
    feed(&session, 0, b"\x1b[?1004h");
    written(&session, 0);
    session.app.is_focused = false;
    run(&mut session, "wait 10\n");
    session.app.is_focused = true;
    run(&mut session, "wait 10\n");
    // The editor is not laid out in this session, so panel focus moves through the field;
    // the frame step after each change runs the focus reconciliation.
    session.app.ide_panel.terminal_focused = false;
    run(&mut session, "wait 10\n");
    session.app.ide_panel.terminal_focused = true;
    run(&mut session, "wait 10\n");
    assert_eq!(written(&session, 0), b"\x1b[O\x1b[I\x1b[O\x1b[I");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn focus_report_on_tab_switch_and_close() {
    let Some((dir, mut session)) = open_shell_terminal("ui-terminal-focus-tabs") else {
        return;
    };
    click_ui(&mut session, "TerminalAdd");
    wait_until(&mut session, 5000, "second terminal tab", |session| {
        session.app.ide_panel.terminals.len() == 2 && crate::headless::tests_support::has_ui(&dump(session), "TerminalTab(1)")
    });
    if shell_failed(&session, 1) {
        let _ = std::fs::remove_dir_all(dir);
        return;
    }
    feed(&session, 0, b"\x1b[?1004h");
    feed(&session, 1, b"\x1b[?1004h");
    run(&mut session, "wait 10\n");
    written(&session, 0);
    written(&session, 1);

    click_ui(&mut session, "TerminalTab(0)");
    run(&mut session, "wait 10\n");
    assert_eq!(session.app.ide_panel.active_terminal, 0);
    assert_eq!(written(&session, 1), b"\x1b[O", "old tab gets focus-out");
    assert_eq!(written(&session, 0), b"\x1b[I", "new tab gets focus-in");

    click_ui(&mut session, "TerminalTab(1)");
    run(&mut session, "wait 10\n");
    written(&session, 0);
    written(&session, 1);

    click_ui(&mut session, "TerminalTabClose(1)");
    wait_until(&mut session, 5000, "terminal tab close", |session| session.app.ide_panel.terminals.len() == 1);
    run(&mut session, "wait 10\n");
    assert_eq!(session.app.ide_panel.active_terminal, 0);
    assert_eq!(written(&session, 0), b"\x1b[I", "tab that becomes active gets focus-in");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn drag_with_tracking_1002_reports_release() {
    let Some((dir, mut session)) = open_shell_terminal("ui-terminal-mouse-drag") else {
        return;
    };
    feed(&session, 0, b"\x1b[?1002h\x1b[?1006h");
    let (x1, y1) = point_for_visible_cell(&mut session, 4, 2);
    let (x2, y2) = point_for_visible_cell(&mut session, 6, 2);
    written(&session, 0);
    run(
        &mut session,
        &format!("mouse_move {x1} {y1}\nclick left down\nmouse_move {x2} {y2}\nclick left up\n"),
    );
    assert_eq!(
        written(&session, 0),
        b"\x1b[<0;5;3M\x1b[<32;7;3M\x1b[<0;7;3m"
    );
    let grid = lock_terminal_grid(&session.app.ide_panel.terminals[0].grid);
    assert!(grid.selection().is_none(), "tracking must not start a local selection");
    drop(grid);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn release_after_drag_out_of_body_reaches_pty_clamped() {
    let Some((dir, mut session)) = open_shell_terminal("ui-terminal-mouse-release-out") else {
        return;
    };
    feed(&session, 0, b"\x1b[?1002h\x1b[?1006h");
    let (x1, y1) = point_for_visible_cell(&mut session, 4, 2);
    let (x2, _) = point_for_visible_cell(&mut session, 6, 2);
    let body = ui_rect(&dump(&mut session), "TerminalBody");
    let y_above = body[1] - 40.0;
    written(&session, 0);
    run(
        &mut session,
        &format!("mouse_move {x1} {y1}\nclick left down\nmouse_move {x2} {y_above}\nclick left up\n"),
    );
    // Release is clamped into the visible area: column 7, top row 1 (SGR is 1-based).
    let bytes = written(&session, 0);
    assert!(bytes.starts_with(b"\x1b[<0;5;3M"), "{bytes:?}");
    assert!(bytes.ends_with(b"\x1b[<0;7;1m"), "{bytes:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn first_click_on_tracking_terminal_without_focus_sends_no_release() {
    let Some((dir, mut session)) = open_shell_terminal("ui-terminal-mouse-first-click") else {
        return;
    };
    feed(&session, 0, b"\x1b[?1002h\x1b[?1006h");
    let (x, y) = point_for_visible_cell(&mut session, 4, 2);
    session.app.ide_panel.terminal_focused = false;
    written(&session, 0);
    run(&mut session, &format!("mouse_move {x} {y}\nclick left down\nclick left up\n"));
    // A release without its press is not a valid report; the focusing click must not write to the PTY.
    assert!(written(&session, 0).is_empty(), "release without a press reached the PTY");
    assert!(!session.app.ide_panel.is_dragging_terminal, "release over the body must end the body drag");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn release_of_second_button_reaches_pty_while_first_is_held() {
    let Some((dir, mut session)) = open_shell_terminal("ui-terminal-mouse-two-buttons") else {
        return;
    };
    feed(&session, 0, b"\x1b[?1002h\x1b[?1006h");
    let (x, y) = point_for_visible_cell(&mut session, 4, 2);
    run(&mut session, &format!("mouse_move {x} {y}\n"));
    written(&session, 0);
    run(&mut session, "click left down\nclick right down\nclick right up\n");
    // SGR button codes from the protocol: left 0, right 2. The right release is owed while left is still held.
    assert_eq!(written(&session, 0), b"\x1b[<0;5;3M\x1b[<2;5;3M\x1b[<2;5;3m");
    run(&mut session, "click left up\n");
    assert_eq!(written(&session, 0), b"\x1b[<0;5;3m");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn cancelled_pointer_gesture_forgets_held_button() {
    let Some((dir, mut session)) = open_shell_terminal("ui-terminal-mouse-cancel-held") else {
        return;
    };
    feed(&session, 0, b"\x1b[?1002h\x1b[?1006h");
    let (x, y) = point_for_visible_cell(&mut session, 4, 2);
    run(&mut session, &format!("mouse_move {x} {y}\nclick left down\n"));
    written(&session, 0);
    session.app.cancel_pointer_interactions();
    run(&mut session, "click left up\n");
    assert!(written(&session, 0).is_empty(), "release after a cancelled press must not reach the PTY");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn focus_loss_forgets_held_button_before_release() {
    let Some((dir, mut session)) = open_shell_terminal("ui-terminal-mouse-focus-held") else {
        return;
    };
    feed(&session, 0, b"\x1b[?1002h\x1b[?1006h");
    let (x, y) = point_for_visible_cell(&mut session, 4, 2);
    run(&mut session, &format!("mouse_move {x} {y}\nclick left down\n"));
    written(&session, 0);
    session.app.ide_panel.terminal_focused = false;
    session.app.sync_terminal_focus_reports();
    run(&mut session, "click left up\n");
    assert!(written(&session, 0).is_empty(), "release after focus loss must not reach the PTY");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn tab_drag_over_tracking_body_keeps_moving_tab() {
    let Some((dir, mut session)) = open_shell_terminal("ui-terminal-mouse-tab-drag") else {
        return;
    };
    click_ui(&mut session, "TerminalAdd");
    wait_until(&mut session, 5000, "second terminal tab", |session| {
        session.app.ide_panel.terminals.len() == 2 && crate::headless::tests_support::has_ui(&dump(session), "TerminalTab(1)")
    });
    if shell_failed(&session, 1) {
        let _ = std::fs::remove_dir_all(dir);
        return;
    }
    activate_terminal_tab(&mut session, 0);
    feed(&session, 0, b"\x1b[?1002h\x1b[?1006h");
    let (x, y) = point_for_visible_cell(&mut session, 4, 2);
    let tab = ui_rect(&dump(&mut session), "TerminalTab(0)");
    let tab_x = tab[0] + tab[2] / 2.0;
    let tab_y = tab[1] + tab[3] / 2.0;
    run(&mut session, &format!("mouse_move {tab_x} {tab_y}\nclick left down\n"));
    assert!(session.app.ide_panel.terminal_tab_drag.is_some(), "tab press must start a tab drag");
    session.app.ide_panel.terminal_focused = true;
    run(&mut session, &format!("mouse_move {x} {y}\n"));
    let drag = session.app.ide_panel.terminal_tab_drag.as_ref().expect("tab drag still active");
    assert!((drag.current_x - x as f32).abs() < 0.5, "tab drag must follow the pointer over the body: {}", drag.current_x);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn tab_drag_released_over_tracking_body_ends_tab_drag() {
    let Some((dir, mut session)) = open_shell_terminal("ui-terminal-mouse-tab-release") else {
        return;
    };
    click_ui(&mut session, "TerminalAdd");
    wait_until(&mut session, 5000, "second terminal tab", |session| {
        session.app.ide_panel.terminals.len() == 2 && crate::headless::tests_support::has_ui(&dump(session), "TerminalTab(1)")
    });
    if shell_failed(&session, 1) {
        let _ = std::fs::remove_dir_all(dir);
        return;
    }
    activate_terminal_tab(&mut session, 0);
    feed(&session, 0, b"\x1b[?1002h\x1b[?1006h");
    let (x, y) = point_for_visible_cell(&mut session, 4, 2);
    let tab = ui_rect(&dump(&mut session), "TerminalTab(0)");
    let tab_x = tab[0] + tab[2] / 2.0;
    let tab_y = tab[1] + tab[3] / 2.0;
    written(&session, 0);
    run(&mut session, &format!("mouse_move {tab_x} {tab_y}\nclick left down\n"));
    session.app.ide_panel.terminal_focused = true;
    run(&mut session, &format!("mouse_move {x} {y}\nclick left up\n"));
    assert!(session.app.ide_panel.terminal_tab_drag.is_none(), "release over the body must end the tab drag");
    assert!(written(&session, 0).is_empty(), "release of a press that never reached the PTY must not be written");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn wheel_alt_screen_without_tracking_sends_capped_arrows() {
    let Some((dir, mut session)) = open_shell_terminal("ui-terminal-wheel-alt") else {
        return;
    };
    // DECCKM off: normal cursor keys; wheel down is 100 px, i.e. 5 steps, capped at 3 arrows.
    feed(&session, 0, b"\x1b[?1049h");
    let (x, y) = point_for_visible_cell(&mut session, 4, 2);
    written(&session, 0);
    run(&mut session, &format!("mouse_move {x} {y}\nwheel 0 -100 px\n"));
    assert_eq!(written(&session, 0), b"\x1b[B\x1b[B\x1b[B");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn wheel_alt_screen_with_application_cursor_keys_sends_ss3_arrows() {
    let Some((dir, mut session)) = open_shell_terminal("ui-terminal-wheel-alt-app") else {
        return;
    };
    // DECCKM on (`?1h`): application cursor keys, so wheel down is SS3 B.
    feed(&session, 0, b"\x1b[?1049h\x1b[?1h");
    let (x, y) = point_for_visible_cell(&mut session, 4, 2);
    written(&session, 0);
    run(&mut session, &format!("mouse_move {x} {y}\nwheel 0 -100 px\n"));
    assert_eq!(written(&session, 0), b"\x1bOB\x1bOB\x1bOB");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn wheel_with_tracking_reports_real_cell() {
    let Some((dir, mut session)) = open_shell_terminal("ui-terminal-wheel-tracking") else {
        return;
    };
    // Wheel down (SGR button 65) at visible cell (4, 2) is column 5, row 3; three capped steps.
    feed(&session, 0, b"\x1b[?1000h\x1b[?1006h");
    let (x, y) = point_for_visible_cell(&mut session, 4, 2);
    written(&session, 0);
    run(&mut session, &format!("mouse_move {x} {y}\nwheel 0 -100 px\n"));
    assert_eq!(written(&session, 0), b"\x1b[<65;5;3M\x1b[<65;5;3M\x1b[<65;5;3M");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn motion_1003_reports_without_button_and_with_button_once_per_cell() {
    let Some((dir, mut session)) = open_shell_terminal("ui-terminal-mouse-1003") else {
        return;
    };
    feed(&session, 0, b"\x1b[?1003h\x1b[?1006h");
    let (x1, y1) = point_for_visible_cell(&mut session, 4, 2);
    let (x2, y2) = point_for_visible_cell(&mut session, 6, 2);
    let (x3, y3) = point_for_visible_cell(&mut session, 8, 2);
    written(&session, 0);
    // Motion without a button: code 35 (3 + 32).
    run(&mut session, &format!("mouse_move {x1} {y1}\n"));
    assert_eq!(written(&session, 0), b"\x1b[<35;5;3M");
    // A second event inside the same cell is deduplicated.
    run(&mut session, &format!("mouse_move {} {y1}\n", x1 + 1.0));
    assert_eq!(written(&session, 0), b"", "motion within one cell must not be reported twice");
    // Motion with the left button held: code 32 + 0.
    run(&mut session, &format!("mouse_move {x2} {y2}\nclick left down\nmouse_move {x3} {y3}\n"));
    assert_eq!(written(&session, 0), b"\x1b[<35;7;3M\x1b[<0;7;3M\x1b[<32;9;3M");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn tracking_reports_visible_row_while_scrolled_back() {
    let Some((dir, mut session)) = open_shell_terminal("ui-terminal-mouse-scrollback") else {
        return;
    };
    let lines: Vec<u8> = b"line\r\n".repeat(120);
    feed(&session, 0, &lines);
    let (x, y) = point_for_bottom_row(&mut session, 4);
    scroll_back_into_history(&mut session, x, y);
    feed(&session, 0, b"\x1b[?1000h\x1b[?1006h");
    let (x, y) = point_for_bottom_row(&mut session, 4);
    let rows = lock_terminal_grid(&session.app.ide_panel.terminals[0].grid).rows();
    written(&session, 0);
    run(&mut session, &format!("mouse_move {x} {y}\nclick left down\nclick left up\n"));
    let expected = format!("\x1b[<0;5;{rows}M\x1b[<0;5;{rows}m");
    assert_eq!(written(&session, 0), expected.as_bytes());
    let _ = std::fs::remove_dir_all(dir);
}
