//! Git blame toolbar, status indicator, and delay setting UI tests.

use super::ui_tests_git_blame_inline::{TEST_HEIGHT, TEST_SCALE, TEST_WIDTH};
use crate::headless::tests_support::{
    click_ui, close_settings, dump, git_blame_fixture, has_ui, open_settings_tab, run_script,
    session_for_test, wait_until, workspace_with_explorer,
};
use crate::headless::HeadlessSession;

fn seed_blame_config() {
    let root = crate::headless::tests_support::ensure_test_profile_root();
    let config_path = crate::platform::app_paths_for_root(&root).config.join("config.json");
    std::fs::create_dir_all(config_path.parent().expect("config parent")).unwrap();
    std::fs::write(
        config_path,
        r#"{"git_blame_inline":false,"git_blame_delay_ms":400}"#,
    )
    .unwrap();
}

fn settings_session() -> HeadlessSession {
    seed_blame_config();
    let mut session = session_for_test(TEST_WIDTH, TEST_HEIGHT);
    let lines = run_script(&mut session, format!("scale {TEST_SCALE}\n").as_bytes());
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    open_settings_tab(&mut session, 2);
    session
}

#[test]
fn headless_git_blame_toolbar_and_status_toggle_persist() {
    seed_blame_config();
    let (root, file) = git_blame_fixture("ui-git-blame-toggle");
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &root);
    let lines = run_script(&mut session, format!("open {}\n", file.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(&mut session, 5000, "opened blame fixture", |session| {
        session.app.file_path.as_ref() == Some(&file)
    });
    click_ui(&mut session, "SidebarSlot(Git)");
    wait_until(&mut session, 5000, "Git blame toolbar toggle", |session| {
        has_ui(&dump(session), "GitBlameToggle")
    });

    click_ui(&mut session, "GitBlameToggle");
    wait_until(&mut session, 8000, "Git blame data after enabling", |session| {
        session.app.editor.git_blame.blame.is_some()
            || session.app.editor.git_blame.failed_key.is_some()
    });
    let enabled = dump(&mut session);
    assert!(session.app.git_blame_inline);
    assert!(enabled["blame_status_visible"].as_bool().unwrap());
    assert!(crate::load_config().git_blame_inline);

    click_ui(&mut session, "StatusGitBlame");
    assert!(!session.app.git_blame_inline);
    assert!(!dump(&mut session)["blame_status_visible"].as_bool().unwrap());
    assert!(!crate::load_config().git_blame_inline);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn headless_git_blame_delay_setting_steps_clamps_and_persists() {
    let mut session = settings_session();
    assert!(has_ui(&dump(&mut session), "SettingsEditorBlameDelayAdjust(-1)"));
    click_ui(&mut session, "SettingsEditorBlameDelayAdjust(-1)");
    assert_eq!(session.app.git_blame_delay_ms, 300);
    assert_eq!(crate::load_config().git_blame_delay_ms, 300);
    click_ui(&mut session, "SettingsEditorBlameDelayAdjust(1)");
    assert_eq!(session.app.git_blame_delay_ms, 400);
    assert_eq!(crate::load_config().git_blame_delay_ms, 400);

    session.app.git_blame_delay_ms = 0;
    click_ui(&mut session, "SettingsEditorBlameDelayAdjust(-1)");
    assert_eq!(session.app.git_blame_delay_ms, 0);
    session.app.git_blame_delay_ms = 2000;
    click_ui(&mut session, "SettingsEditorBlameDelayAdjust(1)");
    assert_eq!(session.app.git_blame_delay_ms, 2000);
}

#[test]
fn headless_git_blame_zero_delay_shows_annotation_without_dwell() {
    seed_blame_config();
    let (root, file) = git_blame_fixture("ui-git-blame-zero-delay");
    let mut session = workspace_with_explorer(1920, TEST_HEIGHT, TEST_SCALE, &root);
    let lines = run_script(&mut session, format!("open {}\n", file.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    click_ui(&mut session, "SidebarSlot(Git)");
    wait_until(&mut session, 5000, "Git blame toolbar toggle", |session| {
        has_ui(&dump(session), "GitBlameToggle")
    });
    click_ui(&mut session, "GitBlameToggle");
    wait_until(&mut session, 8000, "Git blame result", |session| {
        session.app.editor.git_blame.blame.is_some()
    });
    open_settings_tab(&mut session, 2);
    for _ in 0..4 {
        click_ui(&mut session, "SettingsEditorBlameDelayAdjust(-1)");
    }
    assert_eq!(session.app.git_blame_delay_ms, 0);
    close_settings(&mut session);
    session.app.editor.cursor = session.app.editor.line_offsets[1];
    let lines = run_script(&mut session, b"mouse_move 0 0\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let lines = run_script(&mut session, b"mouse_move 0 0\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let annotation = dump(&mut session)["blame_inline"]["text"].as_str().map(str::to_owned);
    assert!(annotation.is_some_and(|text| !text.is_empty()), "{}", dump(&mut session));
    let _ = std::fs::remove_dir_all(root);
}
