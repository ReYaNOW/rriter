//! Headless tests for editing keymap overrides in Settings.

use crate::headless::tests_support::{
    click_ui, dump, open_settings_tab, run_script, scratch_dir, session_for_test, wait_until,
};
use crate::headless::HeadlessSession;
use crate::keymap::{Command, KeymapOverrides};
use serde_json::{json, Value};

fn open_hotkeys_settings(session: &mut HeadlessSession) {
    open_settings_tab(session, 6);
}

fn add_save_chord(session: &mut HeadlessSession) {
    click_ui(session, "SettingsKeymapFilter");
    assert_eq!(run_script(session, b"type file.save\n"), ["ok"]);
    let state = dump(session);
    let add_id = state["ui"]
        .as_array()
        .unwrap_or_else(|| panic!("UI elements missing from settings dump: {state}"))
        .iter()
        .filter_map(|element| element["id"].as_str())
        .find(|id| {
            id.strip_prefix("SettingsKeymapAdd(")
                .and_then(|index| index.strip_suffix(')'))
                .and_then(|index| index.parse::<usize>().ok())
                .and_then(|index| crate::keymap::COMMANDS.get(index))
                .is_some_and(|info| info.id == "file.save")
        })
        .unwrap_or_else(|| panic!("file.save add row missing after filter: {state}"))
        .to_owned();
    click_ui(session, &add_id);
    let lines = run_script(session, b"key ctrl+alt+s\n");
    assert!(lines.iter().all(|line| line == "ok"), "add save chord: {lines:?}");
    assert!(session.app.keymap_overrides.has(Command::FileSave), "file.save override missing after recording");
    click_ui(session, "SettingsKeymapFilter");
    let clear_filter = "key backspace\n".repeat(session.app.keymap_settings.filter.chars().count());
    let lines = run_script(session, clear_filter.as_bytes());
    assert!(lines.iter().all(|line| line == "ok"), "clear filter: {lines:?}");
}

fn visible_ids(session: &mut HeadlessSession) -> Vec<String> {
    let state = dump(session);
    state["ui"]
        .as_array()
        .unwrap_or_else(|| panic!("UI elements missing from settings dump: {state}"))
        .iter()
        .filter_map(|element| element["id"].as_str().map(str::to_owned))
        .collect()
}

fn command_ui_id(session: &mut HeadlessSession, prefix: &str, command: Command) -> String {
    let state = dump(session);
    state["ui"]
        .as_array()
        .unwrap_or_else(|| panic!("UI elements missing from settings dump: {state}"))
        .iter()
        .filter_map(|element| element["id"].as_str())
        .find(|id| {
            id.strip_prefix(prefix)
                .and_then(|tail| tail.split([',', ')']).next())
                .and_then(|index| index.parse::<usize>().ok())
                .and_then(|index| crate::keymap::COMMANDS.get(index))
                .is_some_and(|info| info.command == command)
        })
        .unwrap_or_else(|| panic!("{prefix} row for {command:?} missing: {state}"))
        .to_owned()
}

fn has_command_ui_id(ids: &[String], prefix: &str, command: Command) -> bool {
    ids.iter().any(|id| {
        id.strip_prefix(prefix)
            .and_then(|tail| tail.split([',', ')']).next())
            .and_then(|index| index.parse::<usize>().ok())
            .and_then(|index| crate::keymap::COMMANDS.get(index))
            .is_some_and(|info| info.command == command)
    })
}

#[test]
fn headless_hotkeys_settings_remove_chord_and_restore_command_defaults() {
    let mut session = session_for_test(1280, 720);
    open_hotkeys_settings(&mut session);
    add_save_chord(&mut session);

    assert_eq!(run_script(&mut session, b"type file.save\n"), ["ok"]);
    assert_eq!(session.app.keymap.chords(Command::FileSave).len(), 2, "recorded chords: {:?}", session.app.keymap.chords(Command::FileSave));
    let remove_id = command_ui_id(&mut session, "SettingsKeymapRemove(", Command::FileSave);
    click_ui(&mut session, &remove_id);
    assert_eq!(session.app.keymap.chords(Command::FileSave).len(), 1, "chords after ×: {:?}", session.app.keymap.chords(Command::FileSave));
    assert!(session.app.keymap_overrides.has(Command::FileSave), "removing one chord dropped the remaining override");

    let reset_id = command_ui_id(&mut session, "SettingsKeymapReset(", Command::FileSave);
    click_ui(&mut session, &reset_id);
    assert_eq!(session.app.keymap.chords(Command::FileSave), crate::keymap::Keymap::build(&KeymapOverrides::default()).chords(Command::FileSave), "defaults after ↺: {:?}", session.app.keymap.chords(Command::FileSave));
    assert!(!session.app.keymap_overrides.has(Command::FileSave), "↺ kept the file.save override: {}", session.app.keymap_overrides.to_value());
}

#[test]
fn headless_hotkeys_settings_reset_all_confirmation_and_cancel() {
    let mut session = session_for_test(1280, 720);
    open_hotkeys_settings(&mut session);
    add_save_chord(&mut session);

    click_ui(&mut session, "SettingsKeymapResetAll");
    assert_eq!(dump(&mut session)["dialog"]["action"], "ResetKeymap", "reset confirmation: {}", dump(&mut session));
    assert_eq!(run_script(&mut session, b"dialog cancel\n"), ["ok"]);
    assert!(session.app.keymap_overrides.has(Command::FileSave), "cancel removed file.save override: {}", session.app.keymap_overrides.to_value());

    click_ui(&mut session, "SettingsKeymapResetAll");
    assert_eq!(dump(&mut session)["dialog"]["action"], "ResetKeymap", "second reset confirmation: {}", dump(&mut session));
    assert_eq!(run_script(&mut session, b"dialog save\n"), ["ok"]);
    wait_until(&mut session, 3000, "all keymap overrides reset", |session| {
        !session.app.keymap_overrides.has(Command::FileSave)
    });
    assert_eq!(session.app.keymap.chords(Command::FileSave).len(), 1, "file.save defaults after reset all: {:?}", session.app.keymap.chords(Command::FileSave));
}

#[test]
fn headless_hotkeys_settings_filter_by_label_and_chord() {
    let mut session = session_for_test(1280, 720);
    open_hotkeys_settings(&mut session);
    let filter_ids = visible_ids(&mut session);
    assert!(filter_ids.contains(&"SettingsKeymapFilter".to_owned()), "filter control missing: {filter_ids:?}");
    click_ui(&mut session, "SettingsKeymapFilter");
    assert_eq!(run_script(&mut session, b"type file.save\n"), ["ok"]);
    let label_ids = visible_ids(&mut session);
    let save = command_ui_id(&mut session, "SettingsKeymapRemove(", Command::FileSave);
    assert!(label_ids.contains(&save), "label filter omitted Save: {label_ids:?}");
    assert!(!has_command_ui_id(&label_ids, "SettingsKeymapRemove(", Command::FileOpen), "label filter kept FileOpen: {label_ids:?}");

    let mut session = session_for_test(1280, 720);
    open_hotkeys_settings(&mut session);
    add_save_chord(&mut session);
    click_ui(&mut session, "SettingsKeymapFilter");
    assert_eq!(run_script(&mut session, b"type Alt+S\n"), ["ok"]);
    let chord_ids = visible_ids(&mut session);
    let chord_id = command_ui_id(&mut session, "SettingsKeymapRemove(", Command::FileSave);
    assert!(chord_ids.contains(&chord_id), "chord filter omitted Ctrl+Alt+S: {chord_ids:?}");
    assert!(!has_command_ui_id(&chord_ids, "SettingsKeymapRemove(", Command::FileOpen), "chord filter kept FileOpen: {chord_ids:?}");
}

#[test]
fn headless_hotkeys_settings_shows_skipped_config_entry() {
    let dir = scratch_dir("ui-hotkeys-settings-invalid-config");
    let config_path = dir.join("config.json");
    std::fs::write(&config_path, json!({ "keymap": { "file.save": ["not-a-chord"] } }).to_string())
        .unwrap_or_else(|error| panic!("write {}: {error}", config_path.display()));
    let config: Value = serde_json::from_slice(
        &std::fs::read(&config_path).unwrap_or_else(|error| panic!("read {}: {error}", config_path.display())),
    )
    .unwrap_or_else(|error| panic!("parse {}: {error}", config_path.display()));
    assert!(dir.file_name().unwrap().to_string_lossy().contains(&std::process::id().to_string()), "fixture dir lacks process id: {}", dir.display());

    let mut session = session_for_test(1280, 720);
    session.app.set_keymap_overrides(KeymapOverrides::from_value(config["keymap"].clone()));
    open_hotkeys_settings(&mut session);
    assert_eq!(session.app.keymap_settings.skipped_label.as_deref(), Some("Пропущено 1 записей keymap"), "skipped entry label: {:?}", session.app.keymap_settings.skipped_label);
    assert_eq!(session.app.keymap.skipped().len(), 1, "skipped config entries: {:?}", session.app.keymap.skipped());
    let state = dump(&mut session);
    let filter = state["ui"].as_array().unwrap().iter().find(|element| element["id"] == "SettingsKeymapFilter").unwrap_or_else(|| panic!("hotkey settings controls missing after invalid config: {state}"));
    let filter_rect = filter["rect"].as_array().unwrap();
    let row_top = (filter_rect[1].as_f64().unwrap() + 45.0).round() as u32;
    let row_bottom = (filter_rect[1].as_f64().unwrap() + 62.0).round() as u32;
    let screenshot_path = dir.join("settings.png");
    assert_eq!(run_script(&mut session, format!("screenshot {}\n", screenshot_path.display()).as_bytes()), [format!("ok {} 1280x720", screenshot_path.display())]);
    let screenshot = image::open(&screenshot_path).unwrap().to_rgba8();
    let red_label_pixel = (row_top..row_bottom).any(|y| {
        (filter_rect[0].as_f64().unwrap().round() as u32..(filter_rect[0].as_f64().unwrap() + filter_rect[2].as_f64().unwrap() * 0.66).round() as u32)
            .any(|x| {
                let [red, green, blue, _] = screenshot.get_pixel(x, y).0;
                red > 150 && red > green.saturating_mul(5) / 4 && red > blue.saturating_mul(5) / 4
            })
    });
    assert!(red_label_pixel, "skipped config label has no red pixels in y={row_top}..{row_bottom}: {state}");

    let _ = std::fs::remove_dir_all(dir);
}
