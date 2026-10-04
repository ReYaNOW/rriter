//! Headless coverage for recording edge cases and remapped UI labels.

use super::ui_tests_hotkeys_settings::begin_recording;
use crate::headless::tests_support::{
    click_ui, keyboard_session, open_settings_tab, run_script, session_for_test,
};
use crate::headless::HeadlessSession;
use crate::app::keymap_settings::RowWarning;
use crate::keymap::{Chord, Command, KeymapOverrides};
use serde_json::json;

fn run_ok(session: &mut HeadlessSession, script: &str) {
    let lines = run_script(session, script.as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
}

fn chord(text: &str) -> Chord {
    Chord::parse(crate::platform::CURRENT_PLATFORM, text)
        .unwrap_or_else(|error| panic!("invalid test chord {text}: {error:?}"))
}

fn primary_o() -> &'static str {
    if crate::platform::CURRENT_PLATFORM == crate::platform::PlatformKind::Macos {
        "super+o"
    } else {
        "ctrl+o"
    }
}

#[test]
fn headless_hotkeys_recording_existing_chord_ends_unchanged() {
    let (dir, mut session) = keyboard_session("ui-hotkeys-recording-existing");
    begin_recording(&mut session, Command::FileOpen, "open");
    let before = session.app.keymap.chords(Command::FileOpen).to_vec();
    let overrides_before = session.app.keymap_overrides.to_value();

    run_ok(&mut session, &format!("key {}\n", primary_o()));

    assert!(session.app.keymap_settings.recording.is_none());
    assert_eq!(session.app.keymap.chords(Command::FileOpen), before);
    assert_eq!(session.app.keymap_overrides.to_value(), overrides_before);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_recording_click_outside_chip_cancels() {
    let (dir, mut session) = keyboard_session("ui-hotkeys-recording-outside-click");
    begin_recording(&mut session, Command::GitRefresh, "refresh");

    click_ui(&mut session, "SettingsKeymapFilter");

    assert!(session.app.keymap_settings.recording.is_none());
    assert!(session.app.keymap.chords(Command::GitRefresh).is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_recording_bare_key_keeps_recording_with_hint() {
    let (dir, mut session) = keyboard_session("ui-hotkeys-recording-needs-modifier");
    begin_recording(&mut session, Command::GitRefresh, "refresh");

    run_ok(&mut session, "key e\n");
    assert!(session.app.keymap_settings.recording.is_some());
    assert_eq!(session.app.keymap_settings.hint, Some("нужен Ctrl/Alt/⌘"));
    assert!(session.app.keymap.chords(Command::GitRefresh).is_empty());

    run_ok(&mut session, "key ctrl+e\n");
    assert!(session.app.keymap_settings.recording.is_none());
    assert!(session.app.keymap.chords(Command::GitRefresh).contains(&chord("ctrl+e")));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_handwritten_conflict_marks_rows_and_first_binding_fires() {
    let (dir, mut session) = keyboard_session("ui-hotkeys-handwritten-conflict");
    let overrides = KeymapOverrides::from_value(json!({
        "editor.undo": ["ctrl+e"],
        "editor.redo": ["ctrl+e"]
    }));
    session.app.set_keymap_overrides(overrides);
    run_ok(&mut session, "type x\nkey ctrl+e\n");
    assert_eq!(session.app.editor.get_full_text(), "alpha\n", "EditorUndo should win before EditorRedo");

    open_settings_tab(&mut session, 6);
    let row_warnings = &session.app.keymap_settings.rows;
    assert_eq!(row_warnings.iter().find(|row| row.command == Command::EditorUndo).map(|row| row.warning), Some(RowWarning::Conflict));
    assert_eq!(row_warnings.iter().find(|row| row.command == Command::EditorRedo).map(|row| row.warning), Some(RowWarning::Conflict));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_remap_refreshes_empty_state_and_faq_labels() {
    let mut session = session_for_test(1280, 720);
    let mut overrides = KeymapOverrides::default();
    overrides.add_chord(crate::platform::CURRENT_PLATFORM, Command::FileOpen, chord("ctrl+shift+o"));
    session.app.set_keymap_overrides(overrides);
    let label = session.app.keymap.label(Command::FileOpen);

    assert!(session.app.empty_ide_open_label.contains(label));
    assert!(session.app.empty_ide_open_label.ends_with("— открыть файл"));
    assert!(session.app.faq_editor.get_full_text().contains(&format!("{label}\tОткрыть файл")));
    let lines = run_script(&mut session, b"mouse_move 0 0\n");
    assert!(lines.iter().all(|line| line == "ok"), "render remapped labels: {lines:?}");
}
