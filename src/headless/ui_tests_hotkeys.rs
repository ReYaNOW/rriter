use crate::app::PanelId;
use crate::headless::tests_support::{
    click_ui, dump, git, git_fixture, keyboard_session, run_script, scratch_dir,
    session_for_test, terminal_session, wait_until,
};
use crate::headless::HeadlessSession;
use crate::keymap::KeymapOverrides;
use serde_json::Value;
use std::path::Path;

fn run_ok(session: &mut HeadlessSession, script: &str) {
    let lines = run_script(session, script.as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
}

fn install_config_keymap(session: &mut HeadlessSession, config_path: &Path, keymap: Value) {
    let content = serde_json::json!({"keymap": keymap}).to_string();
    std::fs::write(config_path, &content)
        .unwrap_or_else(|error| panic!("write config fixture {}: {error}", config_path.display()));
    let loaded = std::fs::read_to_string(config_path)
        .unwrap_or_else(|error| panic!("read config fixture {}: {error}", config_path.display()));
    let config: Value = serde_json::from_str(&loaded)
        .unwrap_or_else(|error| panic!("parse config fixture {}: {error}", config_path.display()));
    let overrides = KeymapOverrides::from_value(config["keymap"].clone());
    session.app.set_keymap_overrides(overrides);
}

#[test]
fn headless_hotkeys_config_reassigns_chord() {
    let (dir, mut session) = keyboard_session("ui-hotkeys-config-reassign");
    install_config_keymap(
        &mut session,
        &dir.join("config.json"),
        serde_json::json!({"view.toggle_fps": ["ctrl+shift+f8"]}),
    );

    let fps_before = session.app.show_fps;
    run_ok(&mut session, "key f8\n");
    assert_eq!(session.app.show_fps, fps_before, "old F8 binding still toggled FPS");
    run_ok(&mut session, "key ctrl+shift+f8\n");
    assert_ne!(session.app.show_fps, fps_before, "new Ctrl+Shift+F8 binding did not toggle FPS");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_empty_config_binding_unbinds_command() {
    let (dir, mut session) = keyboard_session("ui-hotkeys-config-unbind");
    install_config_keymap(
        &mut session,
        &dir.join("config.json"),
        serde_json::json!({"settings.toggle": []}),
    );

    run_ok(&mut session, "key f1\n");
    assert!(!dump(&mut session)["overlays"]["settings"].as_bool().unwrap_or(false),
        "F1 opened Settings despite the empty binding: {}", dump(&mut session));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_unavailable_command_shows_reason() {
    let mut session = session_for_test(1280, 720);
    let config_dir = scratch_dir("ui-hotkeys-unavailable-config");
    install_config_keymap(
        &mut session,
        &config_dir.join("config.json"),
        serde_json::json!({"git.stage_all": ["ctrl+shift+g"]}),
    );

    run_ok(&mut session, "key ctrl+shift+g\n");
    let state = dump(&mut session);
    assert_eq!(session.app.readonly_notice_text, "Нет активного репозитория",
        "unexpected unavailable-command reason: {}", session.app.readonly_notice_text);
    assert!(state["overlays"]["readonly_notice"].as_bool().unwrap_or(false),
        "unavailable reason was not shown in notice overlay: {state}");
    let _ = std::fs::remove_dir_all(config_dir);
}

#[test]
fn headless_hotkeys_git_stage_all_command_stages_fixture_changes() {
    let dir = scratch_dir("ui-hotkeys-stage-all");
    git_fixture(&dir);
    let mut session = session_for_test(1280, 720);
    run_ok(&mut session, &format!("workspace {}\n", dir.display()));
    click_ui(&mut session, "SidebarSlot(Git)");
    wait_until(&mut session, 8000, "Git fixture changes", |session| {
        dump(session)["ui"].as_array().is_some_and(|elements| {
            elements.iter().any(|element| element["id"].as_str().unwrap_or("").starts_with("GitFile"))
        })
    });
    let config_dir = scratch_dir("ui-hotkeys-stage-all-config");
    install_config_keymap(
        &mut session,
        &config_dir.join("config.json"),
        serde_json::json!({"git.stage_all": ["ctrl+shift+g"]}),
    );

    run_ok(&mut session, "key ctrl+shift+g\n");
    wait_until(&mut session, 8000, "Git staged fixture changes", |_| {
        !git(&dir, &["diff", "--cached", "--name-only"]).trim().is_empty()
    });
    let staged = git(&dir, &["diff", "--cached", "--name-only"]);
    assert!(staged.contains("changed.txt"), "Git stage-all did not stage changed.txt: {staged}");
    assert!(staged.contains("deleted.txt"), "Git stage-all did not stage deleted.txt: {staged}");
    assert!(staged.contains("untracked.txt"), "Git stage-all did not stage untracked.txt: {staged}");
    let _ = std::fs::remove_dir_all(config_dir);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_terminal_keeps_intercepted_global_chord() {
    let (dir, mut session) = terminal_session("ui-hotkeys-terminal-intercept");
    click_ui(&mut session, "TerminalAdd");
    wait_until(&mut session, 8000, "second terminal tab", |session| {
        session.app.ide_panel.terminals.len() == 2
    });
    let config_dir = scratch_dir("ui-hotkeys-terminal-intercept-config");
    install_config_keymap(
        &mut session,
        &config_dir.join("config.json"),
        serde_json::json!({"view.toggle_fps": ["ctrl+4"]}),
    );
    let fps_before = session.app.show_fps;

    run_ok(&mut session, "key ctrl+4\n");
    wait_until(&mut session, 5000, "terminal Ctrl+4 tab close", |session| {
        session.app.ide_panel.terminals.len() == 1
    });
    assert_eq!(session.app.show_fps, fps_before,
        "terminal Ctrl+4 reached the Global FPS command");
    assert!(session.app.ide_panel.is_open(PanelId::Terminal),
        "closing the active terminal tab closed the whole panel");
    let _ = std::fs::remove_dir_all(config_dir);
    let _ = std::fs::remove_dir_all(dir);
}
