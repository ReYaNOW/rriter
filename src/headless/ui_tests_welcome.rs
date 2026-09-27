//! Welcome screen action regressions.

use crate::headless::tests_support::{click_ui, dump, run_script, scratch_dir, session_for_test};
use crate::headless::HeadlessSession;

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;

fn welcome_session() -> HeadlessSession {
    let mut session = session_for_test(TEST_WIDTH, TEST_HEIGHT);
    let setup = format!("scale {TEST_SCALE}\nsettle 2000\n");
    let lines = run_script(&mut session, setup.as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert!(lines.last().is_some_and(|line| line.ends_with("settled=true")), "{lines:?}");
    session
}

#[test]
fn headless_welcome_new_file_creates_untitled_tab() {
    let mut session = welcome_session();
    click_ui(&mut session, "WelcomeNewFile");

    let state = dump(&mut session);
    assert_eq!(state["mode"], "editor", "{state}");
    assert!(!state["overlays"]["welcome"].as_bool().unwrap());
    assert_eq!(state["tabs"].as_array().unwrap().len(), 1, "{state}");
    assert_eq!(state["tabs"][0]["path"], serde_json::Value::Null);
    assert_eq!(state["tabs"][0]["title"], "Безымянный");
}

#[test]
fn headless_welcome_ide_mode_enters_ide_layout() {
    let mut session = welcome_session();
    click_ui(&mut session, "WelcomeIdeMode");

    let state = dump(&mut session);
    assert_eq!(state["mode"], "ide", "{state}");
    assert!(!state["overlays"]["welcome"].as_bool().unwrap());
    assert!(state["ui"].as_array().unwrap().iter().any(|element| {
        element["id"].as_str().is_some_and(|id| id.starts_with("SidebarSlot("))
    }), "{state}");
}

#[test]
fn headless_welcome_recent_file_opens_seeded_file() {
    let dir = scratch_dir("ui-welcome-recent");
    let path = dir.join("recent.txt");
    std::fs::write(&path, "recent welcome fixture\n").unwrap();

    let mut session = welcome_session();
    session.app.recent_files = vec![path.clone()];
    let lines = run_script(&mut session, b"mouse_move 0 0\n");
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    click_ui(&mut session, "WelcomeRecentFile(0)");

    let state = dump(&mut session);
    assert_eq!(state["mode"], "editor", "{state}");
    assert_eq!(state["tabs"].as_array().unwrap().len(), 1, "{state}");
    assert_eq!(state["tabs"][0]["path"], path.display().to_string());
    assert_eq!(session.app.editor.get_full_text(), "recent welcome fixture\n");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_welcome_deleted_recent_file_is_removed_without_crash() {
    let dir = scratch_dir("ui-welcome-deleted-recent");
    let path = dir.join("deleted.txt");
    std::fs::write(&path, "temporary\n").unwrap();
    std::fs::remove_file(&path).unwrap();

    let mut session = welcome_session();
    session.app.recent_files = vec![path.clone()];
    let lines = run_script(&mut session, b"mouse_move 0 0\n");
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    click_ui(&mut session, "WelcomeRecentFile(0)");

    let state = dump(&mut session);
    assert!(session.app.recent_files.is_empty(), "stale entry retained: {state}");
    assert_eq!(state["mode"], "editor", "{state}");
    assert_eq!(state["tabs"].as_array().unwrap().len(), 1, "{state}");
    assert_eq!(state["tabs"][0]["path"], serde_json::Value::Null, "{state}");
    assert_eq!(state["tabs"][0]["title"], "Добро пожаловать", "{state}");

    let _ = std::fs::remove_dir_all(dir);
}
