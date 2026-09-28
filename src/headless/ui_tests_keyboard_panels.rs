//! Keyboard shortcuts that open, toggle and close IDE panels and move focus
//! between the editor, the terminal and Project Search.

use crate::headless::tests_support::{
    dump, has_ui, keyboard_session, open_terminal_with_alt_q, panel_open, run_script,
    shell_failed, terminal_has_line, wait_until,
};
use crate::headless::HeadlessSession;

const EDITOR_TEXT: &str = "alpha\n";

fn run_ok(session: &mut HeadlessSession, script: &str) {
    let lines = run_script(session, script.as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
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

#[test]
fn headless_keyboard_ctrl4_with_terminal_focus_closes_terminal_tab_only() {
    let (dir, mut session) = keyboard_session("ui-keyboard-routing-ctrl4");
    open_terminal_with_alt_q(&mut session);
    assert!(!shell_failed(&session, 0), "test shell must be available");
    assert!(session.app.ide_panel.terminal_focused);

    let lines = run_script(&mut session, b"key ctrl+4\nsettle 500\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let state = dump(&mut session);
    assert_eq!(state["tabs"].as_array().map_or(0, Vec::len), 1, "{state}");
    assert!(has_ui(&state, "EditorTab(0)"), "{state}");
    assert!(!has_ui(&state, "TerminalTab(0)"), "{state}");
    assert!(!session.app.ide_panel.terminal_focused);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_keyboard_escape_from_project_search_returns_input_to_terminal() {
    let (dir, mut session) = keyboard_session("ui-keyboard-routing-search-escape");
    open_terminal_with_alt_q(&mut session);
    assert!(!shell_failed(&session, 0), "test shell must be available");
    assert!(session.app.ide_panel.terminal_focused);

    let lines = run_script(
        &mut session,
        b"key ctrl+shift+f\ntype query_marker\nkey escape\ntype echo escape_route_marker\nkey enter\n",
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert!(session.app.ide_panel.project_search.query_editor.get_full_text().contains("query_marker"));
    assert!(session.app.ide_panel.project_search.focused.is_none());
    assert!(session.app.ide_panel.terminal_focused);
    assert_eq!(session.app.editor.get_full_text(), EDITOR_TEXT);
    wait_until(&mut session, 5000, "command typed into the terminal after Escape", |session| {
        terminal_has_line(session, 0, "escape_route_marker")
    });

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_keyboard_f1_opens_settings_only_when_terminal_is_not_focused() {
    let (dir, mut session) = keyboard_session("ui-keyboard-routing-f1");
    open_terminal_with_alt_q(&mut session);
    assert!(!shell_failed(&session, 0), "test shell must be available");
    assert!(session.app.ide_panel.terminal_focused);

    let lines = run_script(&mut session, b"key f1\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert_eq!(dump(&mut session)["overlays"]["settings"], false);
    assert!(session.app.ide_panel.terminal_focused);

    let lines = run_script(&mut session, b"key alt+q\nkey f1\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert!(!session.app.ide_panel.terminal_focused);
    assert_eq!(dump(&mut session)["overlays"]["settings"], true);
    assert_eq!(session.app.editor.get_full_text(), EDITOR_TEXT);

    let _ = std::fs::remove_dir_all(dir);
}
