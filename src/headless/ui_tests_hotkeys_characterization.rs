//! Characterization of global and main-dispatch keyboard shortcuts before keymap changes.

use crate::headless::tests_support::{
    click_ui, dump, has_ui, keyboard_session, open_settings_tab, open_terminal_with_alt_q,
    panel_open, run_script, scratch_dir, terminal_session, wait_until,
};
use crate::headless::HeadlessSession;

pub(super) const EXCLUDED_SECTION_1_ROWS: &[(&str, &str)] = &[];

pub(super) const EXCLUDED_SECTION_10_ROWS: &[(&str, &str)] = &[
    ("search Enter direction", "the active search result is not represented in dump state"),
];

pub(super) const EXCLUDED_SECTION_12_CELLS: &[(&str, &str)] = &[];

#[test]
fn headless_hotkeys_excluded_rows_have_reasons() {
    for (row, reason) in EXCLUDED_SECTION_1_ROWS
        .iter()
        .chain(EXCLUDED_SECTION_10_ROWS)
        .chain(EXCLUDED_SECTION_12_CELLS)
    {
        assert!(!row.is_empty() && !reason.is_empty());
    }
}

fn run_ok(session: &mut HeadlessSession, script: &str) {
    let lines = run_script(session, script.as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
}

fn terminal_grid_text(session: &HeadlessSession) -> String {
    let terminal = &session.app.ide_panel.terminals[session.app.ide_panel.active_terminal];
    let grid = crate::app::terminal::lock_terminal_grid(&terminal.grid);
    grid.scrollback
        .iter()
        .chain(grid.lines.iter())
        .map(|row| row.iter().map(|cell| cell.c).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn assert_chord_effect(
    session: &mut HeadlessSession,
    chord: &str,
    setup: impl FnOnce(&mut HeadlessSession),
    check: impl FnOnce(&mut HeadlessSession),
) {
    setup(session);
    run_ok(session, &format!("key {chord}\n"));
    check(session);
}

#[test]
fn headless_hotkeys_characterize_global_panels_and_search() {
    let (dir, mut session) = keyboard_session("ui-hotkeys-global-panels");

    assert_chord_effect(
        &mut session,
        "f1",
        |_| {},
        |session| assert_eq!(dump(session)["overlays"]["settings"], true),
    );
    run_ok(&mut session, "key f1\n");

    let fps_before = session.app.show_fps;
    assert_chord_effect(
        &mut session,
        "f8",
        |_| {},
        |session| assert_ne!(session.app.show_fps, fps_before),
    );
    assert_chord_effect(
        &mut session,
        "alt+w",
        |_| {},
        |session| assert!(panel_open(&dump(session), "problems")),
    );
    assert_chord_effect(
        &mut session,
        "ctrl+shift+f",
        |_| {},
        |session| assert!(panel_open(&dump(session), "search")),
    );
    assert!(has_ui(&dump(&mut session), "ProjectSearchQueryInput"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_characterize_terminal_toggle_and_close() {
    let (dir, mut session) = keyboard_session("ui-hotkeys-terminal-global");
    assert_chord_effect(
        &mut session,
        "alt+q",
        |_| {},
        |session| {
            wait_until(session, 8000, "Alt+Q terminal panel", |session| {
                panel_open(&dump(session), "terminal") && session.app.ide_panel.terminal_focused
            });
        },
    );
    assert_chord_effect(
        &mut session,
        "alt+q",
        |_| {},
        |session| assert!(!session.app.ide_panel.terminal_focused),
    );
    assert_chord_effect(
        &mut session,
        "shift+alt+q",
        |_| {},
        |session| assert!(!panel_open(&dump(session), "terminal")),
    );
    assert_chord_effect(
        &mut session,
        "shift+alt+q",
        |_| {},
        |session| {
            wait_until(session, 8000, "Shift+Alt+Q terminal open", |session| {
                panel_open(&dump(session), "terminal") && !session.app.ide_panel.terminals.is_empty()
            });
        },
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_characterize_terminal_tab_close_and_tab_switch() {
    let (dir, mut session) = terminal_session("ui-hotkeys-terminal-tab-close");
    open_terminal_with_alt_q(&mut session);
    crate::headless::tests_support::click_ui(&mut session, "TerminalAdd");
    wait_until(&mut session, 8000, "second terminal tab", |session| {
        session.app.ide_panel.terminals.len() == 2
    });
    run_ok(&mut session, "key ctrl+4\n");
    wait_until(&mut session, 5000, "Ctrl+4 terminal tab close", |session| {
        session.app.ide_panel.terminals.len() == 1
    });
    let _ = std::fs::remove_dir_all(dir);

    let (dir, mut session) = keyboard_session("ui-hotkeys-tab-switch");
    let second = dir.join("b.txt");
    std::fs::write(&second, "beta\n").expect("write second tab fixture");
    run_ok(&mut session, &format!("open {}\n", second.display()));
    wait_until(&mut session, 5000, "second editor tab", |session| {
        dump(session)["tabs"].as_array().is_some_and(|tabs| tabs.len() == 2)
    });
    assert_chord_effect(
        &mut session,
        "ctrl+pagedown",
        |_| {},
        |session| assert_eq!(dump(session)["tabs"][0]["active"], true),
    );
    assert_chord_effect(
        &mut session,
        "ctrl+pageup",
        |_| {},
        |session| assert_eq!(dump(session)["tabs"][1]["active"], true),
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_characterize_repeat_and_keyboard_owners() {
    let (dir, mut session) = keyboard_session("ui-hotkeys-repeat-undo");
    run_ok(&mut session, "type a\ntype b\nkey --repeat 1 ctrl+z\n");
    assert_eq!(session.app.editor.get_full_text(), "alpha\n", "Ctrl+Z applies both the initial press and repeated press");
    let _ = std::fs::remove_dir_all(dir);

    let dir = scratch_dir("ui-hotkeys-repeat-markdown");
    let file = dir.join("note.md");
    std::fs::write(&file, "# title\n").expect("write Markdown fixture");
    let mut session = crate::headless::tests_support::session_for_test(1280, 720);
    run_ok(&mut session, &format!("workspace {}\nopen {}\nkey --repeat 1 ctrl+shift+v\n", dir.display(), file.display()));
    assert_eq!(dump(&mut session)["tabs"][0]["markdown"], true, "the repeat is consumed after the initial Markdown toggle");
    let _ = std::fs::remove_dir_all(dir);

    let (dir, mut session) = keyboard_session("ui-hotkeys-terminal-matrix");
    open_terminal_with_alt_q(&mut session);
    assert!(session.app.ide_panel.terminal_focused);
    run_ok(&mut session, "key ctrl+shift+f\n");
    assert!(panel_open(&dump(&mut session), "search"), "project search wins over terminal focus");
    assert!(session.app.ide_panel.terminal_focused);
    assert!(has_ui(&dump(&mut session), "ProjectSearchQueryInput"));

    run_ok(&mut session, "key f1\n");
    assert_eq!(dump(&mut session)["overlays"]["settings"], false, "F1 stays with the focused terminal");
    run_ok(&mut session, "key ctrl+a\n");
    assert!(session.app.ide_panel.project_search.query_editor.selection_anchor.is_some(), "Ctrl+A belongs to the focused project search field");
    assert!(!session.app.ide_panel.term_search_focused);
    assert!(session.app.ide_panel.is_open(crate::app::PanelId::Terminal));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_characterize_slash_comment_and_ide_close_all() {
    let dir = scratch_dir("ui-hotkeys-slash-comment");
    let file = dir.join("main.py");
    std::fs::write(&file, "alpha\n").expect("write Python fixture");
    let mut session = crate::headless::tests_support::session_for_test(1280, 720);
    run_ok(&mut session, &format!("workspace {}\nopen {}\nkey ctrl+/\n", dir.display(), file.display()));
    assert_eq!(session.app.editor.get_full_text(), "#alpha\n");
    let _ = std::fs::remove_dir_all(dir);

    let (dir, mut session) = keyboard_session("ui-hotkeys-ctrl-q-ide");
    run_ok(&mut session, "key ctrl+q\n");
    wait_until(&mut session, 5000, "Ctrl+Q close active IDE tab", |session| {
        dump(session)["tabs"].as_array().is_some_and(|tabs| tabs.is_empty())
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_characterize_terminal_search_and_settings_owners() {
    let (dir, mut session) = keyboard_session("ui-hotkeys-settings-terminal-owner");
    open_terminal_with_alt_q(&mut session);
    assert!(session.app.ide_panel.terminal_focused);
    run_ok(&mut session, "key f1\n");
    assert_eq!(dump(&mut session)["overlays"]["settings"], false);
    assert!(session.app.ide_panel.terminal_focused);
    run_ok(&mut session, "key ctrl+f\n");
    assert!(session.app.ide_panel.term_search_focused);
    run_ok(&mut session, "type search-owner\nkey ctrl+a\n");
    assert!(session.app.ide_panel.term_search_editor.selection_anchor.is_some());
    run_ok(&mut session, "key ctrl+c\n");
    assert_eq!(dump(&mut session)["clipboard"]["text"], "search-owner");
    session.app.set_clipboard_text("terminal-search-paste");
    run_ok(&mut session, "key ctrl+v\n");
    assert_eq!(
        session.app.ide_panel.term_search_editor.get_full_text(),
        "terminal-search-paste"
    );
    run_ok(&mut session, "key ctrl+shift+f\n");
    assert!(panel_open(&dump(&mut session), "search"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_characterize_terminal_focused_control_bytes() {
    let (dir, mut session) = terminal_session("ui-hotkeys-terminal-focused-control-bytes");
    click_ui(&mut session, "TerminalBody");
    run_ok(&mut session, "type cat -v\nkey enter\n");
    wait_until(&mut session, 3000, "cat -v terminal command", |session| {
        terminal_grid_text(session).contains("cat -v")
    });

    session.app.set_clipboard_text("terminal-paste-marker");
    run_ok(&mut session, "key ctrl+a\nkey ctrl+v\nkey enter\n");
    wait_until(&mut session, 3000, "terminal control-byte input", |session| {
        let grid = terminal_grid_text(session);
        grid.contains("^A") && grid.contains("terminal-paste-marker")
    });
    run_ok(&mut session, "key ctrl+c\n");
    wait_until(&mut session, 3000, "Ctrl+C reaches PTY", |session| {
        terminal_grid_text(session).contains("^C")
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_characterize_search_and_settings_over_terminal() {
    let (dir, mut session) = terminal_session("ui-hotkeys-search-field-over-terminal");
    click_ui(&mut session, "TerminalBody");
    run_ok(&mut session, "key ctrl+shift+f\ntype field-marker\nkey ctrl+a\nkey ctrl+c\n");
    assert_eq!(dump(&mut session)["clipboard"]["text"], "field-marker");
    session.app.set_clipboard_text("field-paste-marker");
    run_ok(&mut session, "key ctrl+v\n");
    assert_eq!(
        session.app.ide_panel.project_search.query_editor.get_full_text(),
        "field-paste-marker"
    );
    assert!(session.app.ide_panel.terminal_focused);
    run_ok(&mut session, "key ctrl+shift+f\n");
    assert!(panel_open(&dump(&mut session), "search"));
    assert!(session.app.ide_panel.terminal_focused);
    drop(session);
    let _ = std::fs::remove_dir_all(dir);

    let (dir, mut session) = terminal_session("ui-hotkeys-settings-over-terminal");
    click_ui(&mut session, "TerminalBody");
    run_ok(&mut session, "key alt+q\n");
    assert!(!session.app.ide_panel.terminal_focused);
    open_settings_tab(&mut session, 0);
    session.app.set_clipboard_text("settings-clipboard-sentinel");
    run_ok(&mut session, "key ctrl+c\nkey ctrl+v\nkey ctrl+shift+f\nkey ctrl+a\n");
    let state = dump(&mut session);
    assert_eq!(state["overlays"]["settings"], true);
    assert_eq!(state["overlays"]["search"], false);
    assert_eq!(state["clipboard"]["text"], "settings-clipboard-sentinel");
    assert!(!terminal_grid_text(&session).contains("settings-clipboard-sentinel"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_characterize_project_search_ctrl_enter() {
    let (dir, mut session) = keyboard_session("ui-hotkeys-project-search-run");
    run_ok(&mut session, "key ctrl+shift+f\ntype alpha\nkey ctrl+enter\n");
    wait_until(&mut session, 5000, "Ctrl+Enter project search results", |session| {
        has_ui(&dump(session), "ProjectSearchMatchJump(0, 0)")
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_characterize_startup_keyboard_gate() {
    let (dir, mut session) = keyboard_session("ui-hotkeys-startup-key-gate");
    session.app.startup_editor_pending = Some(
        std::time::Instant::now() + std::time::Duration::from_secs(30),
    );
    run_ok(&mut session, "key ctrl+z\n");
    assert_eq!(
        session.app.editor.get_full_text(),
        "alpha\n",
        "the startup gate holds editor shortcuts"
    );
    run_ok(&mut session, "key f1\n");
    assert_eq!(
        dump(&mut session)["overlays"]["settings"],
        true,
        "global settings remains available during the startup gate"
    );
    session.app.startup_editor_pending = None;
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_characterize_tab_switching_with_three_tabs() {
    let (dir, mut session) = keyboard_session("ui-hotkeys-three-tabs");
    for (index, (name, contents)) in [("b.txt", "beta\n"), ("c.txt", "gamma\n")]
        .into_iter()
        .enumerate()
    {
        let path = dir.join(name);
        std::fs::write(&path, contents).expect("write tab fixture");
        run_ok(&mut session, &format!("open {}\n", path.display()));
        let expected_tabs = index + 2;
        wait_until(&mut session, 5000, "editor tab", |session| {
            dump(session)["tabs"]
                .as_array()
                .is_some_and(|tabs| tabs.len() == expected_tabs)
        });
    }
    let tabs = dump(&mut session)["tabs"].as_array().unwrap().len();
    assert_eq!(tabs, 3);
    assert_chord_effect(
        &mut session,
        "ctrl+pageup",
        |_| {},
        |session| assert_eq!(dump(session)["tabs"][1]["active"], true),
    );
    assert_chord_effect(
        &mut session,
        "ctrl+pagedown",
        |_| {},
        |session| assert_eq!(dump(session)["tabs"][2]["active"], true),
    );
    let _ = std::fs::remove_dir_all(dir);
}
