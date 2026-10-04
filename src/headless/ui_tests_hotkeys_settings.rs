use crate::headless::tests_support::{
    click_ui, dump, has_ui, keyboard_session, open_settings_tab, run_script, scratch_dir,
    terminal_session, wait_until,
};
use crate::headless::HeadlessSession;
use crate::keymap::{Chord, Command, KeymapOverrides};

fn run_ok(session: &mut HeadlessSession, script: &str) {
    let lines = run_script(session, script.as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
}

fn chord(text: &str) -> Chord {
    Chord::parse(crate::platform::CURRENT_PLATFORM, text)
        .unwrap_or_else(|error| panic!("invalid test chord {text}: {error:?}"))
}

fn primary_s() -> &'static str {
    if crate::platform::CURRENT_PLATFORM == crate::platform::PlatformKind::Macos {
        "super+s"
    } else {
        "ctrl+s"
    }
}

pub(super) fn begin_recording(session: &mut HeadlessSession, command: Command, filter: &str) {
    open_settings_tab(session, 6);
    click_ui(session, "SettingsKeymapFilter");
    let keys = filter
        .chars()
        .map(|key| format!("key {key}\n"))
        .collect::<String>();
    run_ok(session, &keys);
    let add_id = format!("SettingsKeymapAdd({})", command as usize);
    assert!(has_ui(&dump(session), &add_id), "record button {add_id} missing after filter {filter}");
    click_ui(session, &add_id);
    assert_eq!(session.app.keymap_settings.recording.map(|recording| recording.command), Some(command));
}

#[test]
fn headless_hotkeys_settings_adds_recorded_chord() {
    let (dir, mut session) = keyboard_session("ui-hotkeys-settings-add");
    begin_recording(&mut session, Command::GitRefresh, "refresh");

    run_ok(&mut session, "key ctrl+e\n");

    let recorded = session.app.keymap.chords(Command::GitRefresh);
    assert!(recorded.contains(&chord("ctrl+e")), "GitRefresh chords after recording Ctrl+E: {recorded:?}");
    assert!(session.app.keymap_settings.recording.is_none(), "recording after adding Ctrl+E: {:?}", session.app.keymap_settings.recording);
    drop(session);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_settings_conflict_yes_reassigns_all_owners() {
    let (dir, mut session) = keyboard_session("ui-hotkeys-settings-conflict-yes");
    let shared = chord("ctrl+e");
    let mut overrides = KeymapOverrides::default();
    overrides.add_chord(crate::platform::CURRENT_PLATFORM, Command::EditorUndo, shared);
    overrides.add_chord(crate::platform::CURRENT_PLATFORM, Command::FileSave, shared);
    session.app.set_keymap_overrides(overrides);
    begin_recording(&mut session, Command::FileOpen, "open");

    run_ok(&mut session, "key ctrl+e\n");
    let owners = session.app.keymap_settings.pending_conflict.as_ref()
        .map(|conflict| conflict.owners.clone())
        .unwrap_or_else(|| panic!("Ctrl+E did not open a conflict: {:?}", session.app.keymap_settings.hint));
    assert_eq!(owners.len(), 2, "expected two Ctrl+E owners, observed {owners:?}");
    assert!(owners.contains(&Command::EditorUndo) && owners.contains(&Command::FileSave), "unexpected Ctrl+E owners: {owners:?}");
    click_ui(&mut session, "SettingsKeymapConflictAccept");

    let file_open = session.app.keymap.chords(Command::FileOpen);
    let editor_undo = session.app.keymap.chords(Command::EditorUndo);
    let file_save = session.app.keymap.chords(Command::FileSave);
    assert!(file_open.contains(&shared), "FileOpen did not gain Ctrl+E: {file_open:?}");
    assert!(!editor_undo.contains(&shared), "EditorUndo retained reassigned Ctrl+E: {editor_undo:?}");
    assert!(!file_save.contains(&shared), "FileSave retained reassigned Ctrl+E: {file_save:?}");
    assert!(editor_undo.contains(&chord("mod+z")), "EditorUndo chords after reassignment: {editor_undo:?}");
    assert!(file_save.contains(&chord("mod+s")), "FileSave chords after reassignment: {file_save:?}");
    drop(session);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_settings_conflict_no_keeps_both_owners() {
    let (dir, mut session) = keyboard_session("ui-hotkeys-settings-conflict-no");
    begin_recording(&mut session, Command::FileOpen, "open");

    run_ok(&mut session, &format!("key {}\n", primary_s()));
    assert!(session.app.keymap_settings.pending_conflict.is_some(), "{} did not open a conflict", primary_s());
    click_ui(&mut session, "SettingsKeymapConflictCancel");

    let offered = chord("mod+s");
    let file_open = session.app.keymap.chords(Command::FileOpen);
    let file_save = session.app.keymap.chords(Command::FileSave);
    assert!(!file_open.contains(&offered), "FileOpen gained {} after Нет: {file_open:?}", primary_s());
    assert!(file_save.contains(&offered), "FileSave lost {} after Нет: {file_save:?}", primary_s());
    assert!(session.app.keymap_settings.pending_conflict.is_none(), "conflict remained open after Нет");
    drop(session);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_settings_escape_cancels_recording() {
    let (dir, mut session) = keyboard_session("ui-hotkeys-settings-escape");
    begin_recording(&mut session, Command::GitRefresh, "refresh");

    run_ok(&mut session, "key escape\n");

    assert!(session.app.keymap_settings.recording.is_none(), "recording state after Escape: {:?}", session.app.keymap_settings.recording);
    assert!(!session.app.keymap.chords(Command::GitRefresh).contains(&chord("ctrl+e")), "Escape added Ctrl+E: {:?}", session.app.keymap.chords(Command::GitRefresh));
    drop(session);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_settings_refuses_reserved_chord_and_keeps_recording() {
    let (dir, mut session) = keyboard_session("ui-hotkeys-settings-reserved");
    begin_recording(&mut session, Command::GitRefresh, "refresh");

    run_ok(&mut session, "key enter\n");

    assert!(session.app.keymap_settings.recording.is_some(), "recording state after reserved Enter: {:?}", session.app.keymap_settings.recording);
    assert_eq!(session.app.keymap_settings.hint, Some("зарезервировано"), "reserved Enter hint: {:?}", session.app.keymap_settings.hint);
    assert!(session.app.keymap.chords(Command::GitRefresh).is_empty(), "reserved Enter was assigned: {:?}", session.app.keymap.chords(Command::GitRefresh));
    drop(session);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_settings_recording_owns_terminal_chord_before_pty() {
    let (dir, mut session) = terminal_session("ui-hotkeys-settings-terminal-recording");
    click_ui(&mut session, "TerminalBody");
    run_ok(&mut session, "type cat -v\nkey enter\n");
    wait_until(&mut session, 3000, "cat -v waiting for terminal input", |session| {
        let terminal = &session.app.ide_panel.terminals[session.app.ide_panel.active_terminal];
        let grid = crate::app::terminal::lock_terminal_grid(&terminal.grid);
        grid.scrollback.iter().chain(grid.lines.iter())
            .any(|row| row.iter().map(|cell| cell.c).collect::<String>().contains("cat -v"))
    });
    assert!(session.app.ide_panel.terminal_focused, "terminal did not retain focus before opening Settings");
    session.app.ide_panel.terminal_focused = false;
    begin_recording(&mut session, Command::GitRefresh, "refresh");
    session.app.ide_panel.terminal_focused = true;

    run_ok(&mut session, "key ctrl+e\nwait 150\n");

    let recorded = session.app.keymap.chords(Command::GitRefresh);
    assert!(recorded.contains(&chord("ctrl+e")), "recording lost Ctrl+E to another input owner: {recorded:?}");
    let echoed = {
        let terminal = &session.app.ide_panel.terminals[session.app.ide_panel.active_terminal];
        let grid = crate::app::terminal::lock_terminal_grid(&terminal.grid);
        grid.scrollback.iter().chain(grid.lines.iter())
            .any(|row| row.iter().map(|cell| cell.c).collect::<String>().contains("^E"))
    };
    assert!(!echoed, "Ctrl+E reached cat -v in the PTY; echoed={echoed}");
    drop(session);
    let _ = std::fs::remove_dir_all(dir);
}
