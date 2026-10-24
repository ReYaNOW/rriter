//! Headless coverage for terminal hyperlinks: Ctrl+hover cursor and hover state, Ctrl+click
//! routing (never to the PTY, even under mouse tracking) and the URI scheme matrix. The
//! expected requests come from the link text and the task matrix, not from the app code.

use crate::app::terminal::lock_terminal_grid;
use crate::headless::HeadlessSession;
use crate::headless::tests_support::{
    dump, feed, run_script, shell_failed, terminal_session, ui_rect, written,
};
use crate::platform::ExternalRequest;
use crate::render_view::terminal_ui::TERMINAL_TEXT_SCALE;
use winit::keyboard::ModifiersState;

fn run(session: &mut HeadlessSession, script: &str) {
    let lines = run_script(session, script.as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
}

/// Clears the visible grid and writes `label` as an OSC 8 hyperlink to `uri`, so the URI is exact.
fn link_bytes(uri: &str) -> Vec<u8> {
    format!("\x1b[H\x1b[2J\x1b]8;;{uri}\x1b\\link\x1b]8;;\x1b\\").into_bytes()
}

fn char_metrics(session: &HeadlessSession) -> (f32, f32) {
    let renderer = session.app.renderer.as_ref().unwrap_or_else(|| panic!("headless renderer"));
    let char_w = renderer.ascii_advances['A' as usize].max(1.0) * TERMINAL_TEXT_SCALE;
    (renderer.scale_factor, char_w)
}

/// Pointer position at the centre of visible cell `(col, row)` on a grid that is not scrolled back.
/// The position is found through the app's own cell mapping, so the test checks the same geometry.
fn point_for_visible_cell(session: &mut HeadlessSession, col: usize, row: usize) -> (f64, f64) {
    let body = ui_rect(&dump(session), "TerminalBody");
    let (scale, char_w) = char_metrics(session);
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

fn open_shell_terminal(name: &str) -> Option<(std::path::PathBuf, HeadlessSession)> {
    let (dir, session) = terminal_session(name);
    if shell_failed(&session, 0) {
        let _ = std::fs::remove_dir_all(dir);
        return None;
    }
    Some((dir, session))
}

/// Ctrl+click on the link at visible cell (2, 0) with `uri` written there; returns the one recorded request.
fn ctrl_click_uri(session: &mut HeadlessSession, uri: &str) -> Option<ExternalRequest> {
    feed(session, 0, &link_bytes(uri));
    let (x, y) = point_for_visible_cell(session, 2, 0);
    session.app.external_requests.take();
    session.app.modifiers = ModifiersState::CONTROL;
    run(session, &format!("mouse_move {x} {y}\nclick left down\nclick left up\n"));
    session.app.modifiers = ModifiersState::empty();
    session.app.external_requests.take()
}

#[test]
fn ctrl_click_with_tracking_not_sent_to_pty() {
    let Some((dir, mut session)) = open_shell_terminal("ui-terminal-links-tracking") else {
        return;
    };
    feed(&session, 0, b"\x1b[?1000h\x1b[?1006h");
    feed(&session, 0, &link_bytes("https://a"));
    let (x, y) = point_for_visible_cell(&mut session, 2, 0);
    written(&session, 0);
    session.app.external_requests.take();
    session.app.modifiers = ModifiersState::CONTROL;
    run(&mut session, &format!("mouse_move {x} {y}\nclick left down\nclick left up\n"));
    session.app.modifiers = ModifiersState::empty();
    assert_eq!(written(&session, 0), b"", "Ctrl+click on a link must not reach the PTY");
    assert_eq!(
        session.app.external_requests.take(),
        Some(ExternalRequest::OpenUrl("https://a".to_string()))
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn ctrl_right_click_over_link_reaches_pty_under_tracking() {
    let Some((dir, mut session)) = open_shell_terminal("ui-terminal-links-right") else {
        return;
    };
    feed(&session, 0, b"\x1b[?1000h\x1b[?1006h");
    feed(&session, 0, &link_bytes("https://a"));
    let (x, y) = point_for_visible_cell(&mut session, 2, 0);
    written(&session, 0);
    session.app.external_requests.take();
    session.app.modifiers = ModifiersState::CONTROL;
    run(&mut session, &format!("mouse_move {x} {y}\nclick right down\nclick right up\n"));
    session.app.modifiers = ModifiersState::empty();
    // SGR button code 2 is the right button; only a left press on a link is opened.
    assert_eq!(written(&session, 0), b"\x1b[<2;3;1M\x1b[<2;3;1m");
    assert_eq!(session.app.external_requests.take(), None);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn ctrl_over_link_keeps_pty_drag_going() {
    let Some((dir, mut session)) = open_shell_terminal("ui-terminal-links-drag") else {
        return;
    };
    feed(&session, 0, b"\x1b[?1002h\x1b[?1006h");
    feed(&session, 0, &link_bytes("https://a"));
    let (x0, y0) = point_for_visible_cell(&mut session, 0, 1);
    let (x1, y1) = point_for_visible_cell(&mut session, 2, 0);
    written(&session, 0);
    session.app.external_requests.take();
    run(&mut session, &format!("mouse_move {x0} {y0}\nclick left down\n"));
    session.app.modifiers = ModifiersState::CONTROL;
    run(&mut session, &format!("mouse_move {x1} {y1}\nclick left up\n"));
    session.app.modifiers = ModifiersState::empty();
    // SGR: left press at column 1 row 2; motion with left held (32 + 0) at column 3 row 1; left release.
    assert_eq!(written(&session, 0), b"\x1b[<0;1;2M\x1b[<32;3;1M\x1b[<0;3;1m");
    assert_eq!(session.app.external_requests.take(), None);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn ctrl_click_link_schemes() {
    let Some((dir, mut session)) = open_shell_terminal("ui-terminal-links-schemes") else {
        return;
    };
    let work = std::env::temp_dir().join(format!("rriter-terminal-links-{}-schemes", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).expect("create test directory");
    let canonical = std::fs::canonicalize(&work).expect("canonical test directory");
    std::fs::write(work.join("a b"), b"text").expect("write file with space");
    std::fs::write(work.join("run.sh"), b"#!/bin/sh\n").expect("write script");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(work.join("run.sh"), std::fs::Permissions::from_mode(0o755))
            .expect("make script executable");
    }
    std::fs::write(work.join("app.desktop"), b"[Desktop Entry]\n").expect("write desktop entry");
    let dir_uri = canonical.display().to_string();

    assert_eq!(ctrl_click_uri(&mut session, "https://a"), Some(ExternalRequest::OpenUrl("https://a".to_string())));
    assert_eq!(
        ctrl_click_uri(&mut session, "file:///tmp"),
        Some(ExternalRequest::OpenPath(std::path::PathBuf::from("/tmp")))
    );
    assert_eq!(
        ctrl_click_uri(&mut session, &format!("file://{dir_uri}/a%20b")),
        Some(ExternalRequest::OpenPath(canonical.join("a b")))
    );
    assert_eq!(ctrl_click_uri(&mut session, "file:///tmp/a%zz"), None);
    assert_eq!(ctrl_click_uri(&mut session, "file:///tmp/a%00"), None);
    assert_eq!(
        ctrl_click_uri(&mut session, "file://LOCALHOST/tmp"),
        Some(ExternalRequest::OpenPath(std::path::PathBuf::from("/tmp")))
    );
    assert_eq!(ctrl_click_uri(&mut session, "file://other/tmp"), None);
    assert_eq!(ctrl_click_uri(&mut session, "mailto:a@b"), None);
    assert_eq!(
        ctrl_click_uri(&mut session, &format!("file://{dir_uri}/run.sh")),
        Some(ExternalRequest::RevealPath(canonical.join("run.sh")))
    );
    assert_eq!(
        ctrl_click_uri(&mut session, &format!("file://{dir_uri}/app.desktop")),
        Some(ExternalRequest::RevealPath(canonical.join("app.desktop")))
    );
    assert_eq!(ctrl_click_uri(&mut session, &format!("file://{dir_uri}/missing")), None);

    let _ = std::fs::remove_dir_all(&work);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn ctrl_hover_sets_pointer() {
    let Some((dir, mut session)) = open_shell_terminal("ui-terminal-links-hover") else {
        return;
    };
    feed(&session, 0, &link_bytes("https://a"));
    let (x, y) = point_for_visible_cell(&mut session, 2, 0);
    let (blank_x, blank_y) = point_for_visible_cell(&mut session, 10, 0);

    session.app.modifiers = ModifiersState::CONTROL;
    run(&mut session, &format!("mouse_move {x} {y}\n"));
    assert!(session.app.terminal_link_cursor_active(), "Ctrl over a link must show the link cursor");
    assert!(session.app.ide_panel.terminals[0].hovered_link.is_some());

    run(&mut session, &format!("mouse_move {blank_x} {blank_y}\n"));
    assert!(!session.app.terminal_link_cursor_active(), "a blank cell has no link");

    run(&mut session, &format!("mouse_move {x} {y}\n"));
    session.app.modifiers = ModifiersState::empty();
    session.app.refresh_terminal_link_hover_at_pointer();
    assert!(!session.app.terminal_link_cursor_active(), "releasing Ctrl clears the link cursor");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn ctrl_press_over_link_requests_redraw() {
    let Some((dir, mut session)) = open_shell_terminal("ui-terminal-links-redraw") else {
        return;
    };
    feed(&session, 0, &link_bytes("https://a"));
    let (x, y) = point_for_visible_cell(&mut session, 2, 0);
    run(&mut session, &format!("mouse_move {x} {y}\n"));

    // Ctrl changes without a pointer motion: the hover must change and ask for a frame.
    session.app.modifiers = ModifiersState::CONTROL;
    assert!(session.app.refresh_terminal_link_hover_at_pointer(), "Ctrl over a link changes the hover");
    assert!(!session.app.refresh_terminal_link_hover_at_pointer(), "same state does not ask for a frame");

    session.app.modifiers = ModifiersState::empty();
    assert!(session.app.refresh_terminal_link_hover_at_pointer(), "releasing Ctrl changes the hover");
    let _ = std::fs::remove_dir_all(dir);
}
