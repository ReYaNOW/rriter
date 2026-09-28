//! Headless UI coverage for IDE settings workspace and ignore controls.

use crate::headless::tests_support::{
    click_ui, dump, has_ui, open_settings_tab, run_script, scratch_dir, session_for_test,
    wait_until,
};
use crate::headless::HeadlessSession;

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;

fn settings_ide_session() -> HeadlessSession {
    let mut session = session_for_test(TEST_WIDTH, TEST_HEIGHT);
    let lines = run_script(&mut session, format!("scale {TEST_SCALE}\n").as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    session.app.ide_workspaces.clear();
    session.app.ide_ignore_patterns.clear();
    session.app.settings_ide_scroll.current = 0.0;
    session.app.settings_ide_scroll.target = 0.0;
    session.app.settings_ide_scroll.velocity = 0.0;
    session
}

fn open_ide_settings(session: &mut HeadlessSession) {
    open_settings_tab(session, 0);
    wait_until(session, 3000, "IDE ignore input", |session| {
        has_ui(&dump(session), "SettingsIdeIgnoreInput")
    });
}

fn type_ignore_input(session: &mut HeadlessSession, text: &str) {
    click_ui(session, "SettingsIdeIgnoreInput");
    let lines = run_script(session, format!("type {text}\n").as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
}

#[test]
fn headless_settings_ide_add_ignore_updates_state_and_shows_chip() {
    let mut session = settings_ide_session();
    open_ide_settings(&mut session);
    let pattern = "headless-added-ignore-marker";

    type_ignore_input(&mut session, pattern);
    wait_until(&mut session, 1500, "Add ignore control", |session| {
        has_ui(&dump(session), "SettingsIdeAddIgnore")
    });
    click_ui(&mut session, "SettingsIdeAddIgnore");

    wait_until(&mut session, 1500, "ignore pattern added", |session| {
        session.app.ide_ignore_patterns.iter().any(|item| item == pattern)
    });
    assert_eq!(session.app.ide_ignore_patterns, vec![pattern.to_string()]);
    assert!(has_ui(&dump(&mut session), "SettingsIdeRemoveIgnore(0)"));
}

#[test]
fn headless_settings_ide_remove_ignore_removes_selected_pattern() {
    let mut session = settings_ide_session();
    session.app.ide_ignore_patterns = vec![
        "remove-this-marker".into(),
        "keep-this-marker".into(),
    ];
    open_ide_settings(&mut session);
    assert!(has_ui(&dump(&mut session), "SettingsIdeRemoveIgnore(0)"));

    click_ui(&mut session, "SettingsIdeRemoveIgnore(0)");
    wait_until(&mut session, 1500, "first ignore pattern removed", |session| {
        session.app.ide_ignore_patterns.len() == 1
            && session.app.ide_ignore_patterns[0] == "keep-this-marker"
    });
    assert_eq!(session.app.ide_ignore_patterns, vec!["keep-this-marker".to_string()]);
}

#[test]
fn headless_settings_ide_remove_workspace_updates_state() {
    let dir = scratch_dir("settings-ide-remove-workspace");
    let mut session = settings_ide_session();
    session.app.ide_workspaces = vec![dir.clone()];
    open_ide_settings(&mut session);
    assert!(has_ui(&dump(&mut session), "SettingsIdeRemoveWorkspace(0)"));

    click_ui(&mut session, "SettingsIdeRemoveWorkspace(0)");
    wait_until(&mut session, 1500, "workspace removed", |session| {
        session.app.ide_workspaces.is_empty()
    });
    drop(session);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_settings_ide_rejects_empty_and_duplicate_ignore_input() {
    let mut session = settings_ide_session();
    open_ide_settings(&mut session);

    type_ignore_input(&mut session, "   ");
    assert!(!has_ui(&dump(&mut session), "SettingsIdeAddIgnore"));
    let lines = run_script(&mut session, b"key ctrl+a\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");

    let pattern = "headless-duplicate-ignore-marker";
    let lines = run_script(&mut session, format!("type {pattern}\n").as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    click_ui(&mut session, "SettingsIdeAddIgnore");
    assert_eq!(session.app.ide_ignore_patterns, vec![pattern.to_string()]);

    type_ignore_input(&mut session, pattern);
    click_ui(&mut session, "SettingsIdeAddIgnore");
    assert_eq!(session.app.ide_ignore_patterns, vec![pattern.to_string()]);
    assert_eq!(session.app.settings_ignore_editor.get_full_text(), pattern);
}

#[test]
fn headless_settings_ide_scrolls_when_ignore_list_overflows() {
    let mut session = settings_ide_session();
    session.app.ide_ignore_patterns = (0..48)
        .map(|index| format!("headless-overflow-pattern-{index:02}"))
        .collect();
    open_ide_settings(&mut session);
    assert!(has_ui(&dump(&mut session), "SettingsIdeScrollY"));

    let lines = run_script(
        &mut session,
        b"mouse_move 800 500\nwheel 0 -10\nsettle 600\n",
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(&mut session, 1500, "IDE settings scroll", |session| {
        session.app.settings_ide_scroll.current > 0.0
    });
    assert!(has_ui(&dump(&mut session), "SettingsIdeScrollY"));
}
