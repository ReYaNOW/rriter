//! Keyboard shortcuts that open, toggle and close IDE panels and move focus
//! between the editor, the terminal and Project Search.

use crate::headless::tests_support::{
    dump, has_ui, run_script, scratch_dir, session_for_test, shell_failed, terminal_has_line,
    wait_until,
};
use crate::headless::HeadlessSession;
use std::path::PathBuf;

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;
const EDITOR_TEXT: &str = "alpha\n";

fn run_ok(session: &mut HeadlessSession, script: &str) {
    let lines = run_script(session, script.as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
}

/// Workspace with one open file. Under `cfg(test)` panel state is neither loaded
/// nor saved (`load_panel_state`/`save_panel_state`), so every session starts with
/// all panels closed and the first Alt+Q takes the "open" branch of
/// `apply_terminal_alt_q_shortcut`.
fn keyboard_session(name: &str) -> (PathBuf, HeadlessSession) {
    let dir = scratch_dir(name);
    let file = dir.join("a.txt");
    std::fs::write(&file, EDITOR_TEXT).unwrap();
    let mut session = session_for_test(TEST_WIDTH, TEST_HEIGHT);
    run_ok(
        &mut session,
        &format!(
            "scale {TEST_SCALE}\nworkspace {}\nopen {}\nsettle 2000\n",
            dir.display(),
            file.display()
        ),
    );
    assert!(!session.app.ide_panel.is_open(crate::app::PanelId::Terminal));
    assert_eq!(session.app.editor.get_full_text(), EDITOR_TEXT);
    (dir, session)
}

fn panel_open(state: &serde_json::Value, panel: &str) -> bool {
    state["ide_panel"]["open"]
        .as_array()
        .is_some_and(|panels| panels.iter().any(|open| open == panel))
}

fn open_terminal_with_alt_q(session: &mut HeadlessSession) {
    run_ok(session, "key alt+q\n");
    wait_until(session, 8000, "terminal panel after Alt+Q", |session| {
        let state = dump(session);
        panel_open(&state, "terminal")
            && has_ui(&state, "TerminalBody")
            && !session.app.ide_panel.terminals.is_empty()
    });
}

#[test]
fn headless_keyboard_alt_q_opens_focused_terminal_and_toggles_focus_with_editor() {
    let (dir, mut session) = keyboard_session("ui-keyboard-alt-q-focus");
    open_terminal_with_alt_q(&mut session);
    assert!(session.app.ide_panel.terminal_focused);
    if shell_failed(&session, 0) {
        let _ = std::fs::remove_dir_all(dir);
        return;
    }

    // Focused terminal takes typed text; the editor buffer stays untouched.
    run_ok(&mut session, "type echo kbmark\nkey enter\n");
    wait_until(&mut session, 5000, "echo output in terminal", |session| {
        terminal_has_line(session, 0, "kbmark")
    });
    assert_eq!(session.app.editor.get_full_text(), EDITOR_TEXT);

    // Second Alt+Q keeps the panel but hands input back to the editor.
    run_ok(&mut session, "key alt+q\ntype zz\n");
    assert!(!session.app.ide_panel.terminal_focused);
    assert!(panel_open(&dump(&mut session), "terminal"));
    assert_eq!(session.app.editor.get_full_text(), "zzalpha\n");

    // Third Alt+Q moves input back into the terminal.
    run_ok(&mut session, "key alt+q\ntype qq\n");
    assert!(session.app.ide_panel.terminal_focused);
    assert_eq!(session.app.editor.get_full_text(), "zzalpha\n");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_keyboard_shift_alt_q_closes_terminal_and_alt_q_reopens_it() {
    let (dir, mut session) = keyboard_session("ui-keyboard-shift-alt-q");
    open_terminal_with_alt_q(&mut session);

    run_ok(&mut session, "key shift+alt+q\nwait 300\n");
    let state = dump(&mut session);
    assert!(!panel_open(&state, "terminal"), "{}", state["ide_panel"]);
    assert!(!has_ui(&state, "TerminalBody"));
    assert!(!session.app.ide_panel.terminal_focused);

    // Closed panel: input goes to the editor again.
    run_ok(&mut session, "type zz\n");
    assert_eq!(session.app.editor.get_full_text(), "zzalpha\n");

    open_terminal_with_alt_q(&mut session);
    assert!(session.app.ide_panel.terminal_focused);
    assert!(has_ui(&dump(&mut session), "TerminalTab(0)"));

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_keyboard_f1_stays_in_focused_terminal_and_opens_settings_otherwise() {
    let (dir, mut session) = keyboard_session("ui-keyboard-f1-terminal");
    open_terminal_with_alt_q(&mut session);
    assert!(session.app.ide_panel.terminal_focused);

    run_ok(&mut session, "key f1\nwait 300\n");
    assert_eq!(dump(&mut session)["overlays"]["settings"], false);
    assert!(session.app.ide_panel.terminal_focused);

    run_ok(&mut session, "key alt+q\nkey f1\nwait 300\n");
    assert!(!session.app.ide_panel.terminal_focused);
    assert_eq!(dump(&mut session)["overlays"]["settings"], true);

    run_ok(&mut session, "key f1\nwait 300\n");
    assert_eq!(dump(&mut session)["overlays"]["settings"], false);
    assert_eq!(session.app.editor.get_full_text(), EDITOR_TEXT);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_keyboard_alt_w_toggles_problems_panel() {
    let (dir, mut session) = keyboard_session("ui-keyboard-alt-w-problems");
    assert!(!panel_open(&dump(&mut session), "problems"));

    run_ok(&mut session, "key alt+w\nwait 300\n");
    let state = dump(&mut session);
    assert!(panel_open(&state, "problems"), "{}", state["ide_panel"]);
    assert!(has_ui(&state, "ProblemsTab(0)"));

    run_ok(&mut session, "key alt+w\nwait 300\n");
    let state = dump(&mut session);
    assert!(!panel_open(&state, "problems"), "{}", state["ide_panel"]);
    assert!(!has_ui(&state, "ProblemsTab(0)"));
    assert_eq!(session.app.editor.get_full_text(), EDITOR_TEXT);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_keyboard_ctrl_shift_f_from_terminal_focuses_query_and_escape_returns_to_terminal() {
    let (dir, mut session) = keyboard_session("ui-keyboard-ctrl-shift-f");
    open_terminal_with_alt_q(&mut session);
    assert!(session.app.ide_panel.terminal_focused);

    run_ok(&mut session, "key ctrl+shift+f\nwait 300\ntype needle\n");
    let state = dump(&mut session);
    assert!(panel_open(&state, "search"), "{}", state["ide_panel"]);
    assert!(panel_open(&state, "terminal"), "{}", state["ide_panel"]);
    assert!(has_ui(&state, "ProjectSearchQueryInput"));
    let query = session.app.ide_panel.project_search.query_editor.get_full_text();
    assert!(query.contains("needle"), "{query:?}");
    assert_eq!(session.app.editor.get_full_text(), EDITOR_TEXT);

    // Escape leaves the query field; the terminal still owns keyboard focus,
    // so the next keystroke reaches neither the query nor the editor.
    run_ok(&mut session, "key escape\ntype x\n");
    assert!(session.app.ide_panel.project_search.focused.is_none());
    assert!(session.app.ide_panel.terminal_focused);
    assert_eq!(session.app.ide_panel.project_search.query_editor.get_full_text(), query);
    assert_eq!(session.app.editor.get_full_text(), EDITOR_TEXT);
    assert!(panel_open(&dump(&mut session), "search"));

    let _ = std::fs::remove_dir_all(dir);
}
