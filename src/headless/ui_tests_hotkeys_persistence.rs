//! Headless tests for configurable hotkey config loading and persistence.

use crate::headless::tests_support::{click_ui, open_settings_tab, run_script, session_for_test};
use crate::headless::HeadlessSession;
use crate::keymap::{Chord, Command};
use serde_json::{json, Value};
use std::path::PathBuf;

fn config_path() -> PathBuf {
    crate::headless::tests_support::ensure_test_profile_root();
    crate::platform::config_dir().join("config.json")
}

fn prepare_config(value: Value) -> PathBuf {
    let path = config_path();
    let parent = path.parent().expect("config path parent");
    std::fs::create_dir_all(parent).expect("create test config directory");
    std::fs::write(&path, value.to_string())
        .unwrap_or_else(|error| panic!("write config fixture {}: {error}", path.display()));
    path
}

fn read_config(path: &PathBuf) -> Value {
    let content = std::fs::read(path)
        .unwrap_or_else(|error| panic!("read config {}: {error}", path.display()));
    serde_json::from_slice(&content)
        .unwrap_or_else(|error| panic!("parse config {}: {error}", path.display()))
}

fn chord(text: &str) -> Chord {
    Chord::parse(crate::platform::CURRENT_PLATFORM, text)
        .unwrap_or_else(|error| panic!("invalid test chord {text}: {error:?}"))
}

fn run_ok(session: &mut HeadlessSession, script: &str) {
    let lines = run_script(session, script.as_bytes());
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
}

fn record_file_save_chord(session: &mut HeadlessSession, key: &str) {
    open_settings_tab(session, 6);
    click_ui(session, "SettingsKeymapFilter");
    run_ok(session, "type file.save\n");
    click_ui(session, &format!("SettingsKeymapAdd({})", Command::FileSave as usize));
    run_ok(session, &format!("key {key}\n"));
    assert!(session.app.keymap.chords(Command::FileSave).contains(&chord(key)));
}

#[test]
fn headless_hotkeys_config_file_is_loaded_at_startup() {
    prepare_config(json!({"keymap": {"view.toggle_fps": ["ctrl+shift+f8"]}}));

    let mut session = session_for_test(1280, 720);
    let fps_before = session.app.show_fps;
    run_ok(&mut session, "key f8\n");
    assert_eq!(session.app.show_fps, fps_before, "default F8 binding still toggled FPS");
    run_ok(&mut session, "key ctrl+shift+f8\n");
    assert_ne!(session.app.show_fps, fps_before, "config binding did not toggle FPS");
}

#[test]
fn headless_hotkeys_settings_persists_diff_remove_reset_and_skipped_values() {
    let unknown = json!([1, true]);
    let skipped = "not-a-chord";
    let path = prepare_config(json!({
        "keymap": {
            "file.save": ["mod+s", skipped],
            "future.command": unknown
        }
    }));
    let mut session = session_for_test(1280, 720);
    record_file_save_chord(&mut session, "ctrl+alt+s");

    let saved = read_config(&path);
    assert_eq!(saved["keymap"]["file.save"], json!(["mod+s", skipped, "mod+alt+s"]));
    assert_eq!(saved["keymap"]["future.command"], unknown);
    assert_eq!(saved["keymap"].as_object().unwrap().len(), 2, "config should contain only explicit diffs and preserved unknown ids");

    click_ui(&mut session, &format!("SettingsKeymapRemove({},1)", Command::FileSave as usize));
    let saved = read_config(&path);
    assert_eq!(saved["keymap"]["file.save"], json!(["mod+s", skipped]));
    assert_eq!(saved["keymap"]["future.command"], unknown);

    click_ui(&mut session, &format!("SettingsKeymapReset({})", Command::FileSave as usize));
    let saved = read_config(&path);
    assert!(saved["keymap"].get("file.save").is_none(), "↺ did not remove the command key: {saved}");
    assert_eq!(saved["keymap"]["future.command"], unknown);
}

#[test]
fn headless_hotkeys_saved_keymap_loads_in_a_new_session() {
    let path = prepare_config(json!({}));
    let mut first = session_for_test(1280, 720);
    record_file_save_chord(&mut first, "ctrl+alt+s");
    assert_eq!(read_config(&path)["keymap"]["file.save"], json!(["mod+s", "mod+alt+s"]));
    drop(first);

    let mut restarted = session_for_test(1280, 720);
    assert!(restarted.app.keymap.chords(Command::FileSave).contains(&chord("ctrl+alt+s")));
}
