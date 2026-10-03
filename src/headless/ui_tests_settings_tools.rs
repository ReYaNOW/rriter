//! UI coverage for the external tools and Dart blocks of the General settings tab.

use crate::app::tool_installer::DartToolStatus;
use crate::headless::tests_support::{
    click_ui, dump, has_ui, open_settings_tab, run_script, scratch_dir, session_for_test, ui_rect,
    wait_until, workspace_with_explorer,
};
use crate::headless::HeadlessSession;
use crate::platform::ToolKind;

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const PICKER_BUSY_ERROR: &str = "Окно выбора инструмента уже открыто";

fn open_general_settings(session: &mut HeadlessSession) {
    let lines = run_script(session, b"scale 1\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    open_settings_tab(session, 1);
    wait_until(session, 2000, "General settings content", |session| {
        has_ui(&dump(session), "SettingsRefreshTools")
    });
}

fn frame(session: &mut HeadlessSession) {
    let lines = run_script(session, b"mouse_move 0 0\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
}

/// A picker that is still open makes the next "Выбрать" click report an error,
/// which opens the install log modal without starting a real installation.
fn open_log_via_busy_picker(session: &mut HeadlessSession) {
    click_ui(session, &format!("SettingsToolPick({})", ToolKind::Ruff.index()));
    assert!(session.app.tool_installer.is_log_open());
    let state = dump(session);
    assert!(has_ui(&state, "SettingsToolInstallLogBackdrop"), "{state}");
    assert!(has_ui(&state, "SettingsCopyToolInstallLog"), "{state}");
    assert!(has_ui(&state, "SettingsCloseToolInstallLog"), "{state}");
    assert!(
        !has_ui(&state, "SettingsCancelToolInstall"),
        "cancel is only offered while an installation runs: {state}"
    );
}

#[test]
fn headless_settings_tools_refresh_rechecks_dart_sdk() {
    let mut session = session_for_test(TEST_WIDTH, TEST_HEIGHT);
    open_general_settings(&mut session);

    // Let a probe started at startup finish, so `Checking` below comes from the click.
    wait_until(&mut session, 10000, "startup Dart SDK probe", |session| {
        session.app.dart_tool_state.status() != DartToolStatus::Checking
    });
    click_ui(&mut session, "SettingsRefreshTools");
    // `DartToolState::refresh` resolves the path synchronously and, when an SDK is found,
    // starts an async `dart --version` probe with status `Checking` and the version cleared.
    // Without an SDK the refresh is synchronous and leaves nothing observable to tell it
    // from the startup state, so only the found-SDK path proves the re-check.
    let state = &session.app.dart_tool_state;
    if state.path().is_some() {
        assert_eq!(state.status(), DartToolStatus::Checking, "refresh did not restart the Dart probe");
        assert!(state.version().is_none(), "refresh kept the old Dart version");
    }
    wait_until(&mut session, 10000, "Dart SDK probe", |session| {
        session.app.dart_tool_state.status() != DartToolStatus::Checking
    });

    let state = &session.app.dart_tool_state;
    match state.status() {
        DartToolStatus::Ready => {
            assert!(state.path().is_some());
            assert!(state.version().is_some_and(|version| !version.is_empty()));
        }
        DartToolStatus::NotFound => assert!(state.path().is_none()),
        DartToolStatus::Error => assert!(state.error().is_some()),
        other => panic!("unexpected Dart status after refresh: {other:?}"),
    }
    assert!(session.app.show_settings);
    assert!(has_ui(&dump(&mut session), "SettingsRefreshTools"));
}

#[test]
fn headless_settings_tools_clear_override_restores_autodetection() {
    let mut session = session_for_test(TEST_WIDTH, TEST_HEIGHT);
    open_general_settings(&mut session);
    let clear_id = format!("SettingsToolClear({})", ToolKind::Uv.index());
    assert!(!has_ui(&dump(&mut session), &clear_id));

    // Tool paths are process-wide: restore the defaults however the test ends.
    struct ToolPathsReset;
    impl Drop for ToolPathsReset {
        fn drop(&mut self) {
            crate::platform::configure_tool_paths(crate::platform::ToolPaths::default());
        }
    }
    let _reset = ToolPathsReset;

    // Seed an explicit uv path the way a finished picker would store it.
    let exe = std::env::current_exe().expect("test binary path");
    session.app.tool_paths.set(ToolKind::Uv, Some(exe));
    crate::platform::configure_tool_paths(session.app.tool_paths.clone());
    frame(&mut session);
    assert!(has_ui(&dump(&mut session), &clear_id));

    click_ui(&mut session, &clear_id);
    assert!(session.app.tool_paths.get(ToolKind::Uv).is_none());
    assert!(crate::platform::configured_tool_path(ToolKind::Uv).is_none());
    let resolution = crate::platform::resolve_tool_kind(ToolKind::Uv);
    assert!(resolution.configured_path.is_none(), "{resolution:?}");
    assert_ne!(resolution.source_label(ToolKind::Uv), Some("настройки"));
    let state = dump(&mut session);
    assert!(!has_ui(&state, &clear_id), "{state}");
    assert!(has_ui(&state, &format!("SettingsToolPick({})", ToolKind::Uv.index())));
}

#[test]
fn headless_settings_tools_install_log_copies_and_closes() {
    let mut session = session_for_test(TEST_WIDTH, TEST_HEIGHT);
    open_general_settings(&mut session);
    assert!(!session.app.tool_installer.is_log_open());

    // Keep the sender alive so the seeded picker stays pending for the whole test.
    let (_picker_tx, picker_rx) = std::sync::mpsc::channel();
    session.app.settings_tool_picker_rx = Some(picker_rx);

    open_log_via_busy_picker(&mut session);
    click_ui(&mut session, "SettingsCopyToolInstallLog");
    let copied = dump(&mut session)["clipboard"]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(copied.contains(&format!("[error] {PICKER_BUSY_ERROR}")), "{copied:?}");
    assert!(session.app.tool_installer.is_log_open());
    click_ui(&mut session, "SettingsCloseToolInstallLog");
    assert!(!session.app.tool_installer.is_log_open());
    assert!(!has_ui(&dump(&mut session), "SettingsToolInstallLogBackdrop"));

    open_log_via_busy_picker(&mut session);
    let lines = run_script(&mut session, b"key escape\n");
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    assert!(!session.app.tool_installer.is_log_open());
    assert!(session.app.show_settings, "Escape must close only the log modal");

    open_log_via_busy_picker(&mut session);
    let lines = run_script(&mut session, b"mouse_move 5 5\nclick\n");
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    assert!(!session.app.tool_installer.is_log_open());
    assert!(session.app.show_settings, "backdrop click must close only the log modal");
    assert!(has_ui(&dump(&mut session), "SettingsRefreshTools"));
}

#[test]
fn headless_settings_tools_dart_restart_and_open_log_in_ide() {
    let dir = scratch_dir("settings-tools-dart");
    std::fs::write(dir.join("main.dart"), "void main() {}\n").expect("write main.dart");
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, 1.0, &dir);
    open_general_settings(&mut session);
    if !session.app.dart_settings.enabled {
        click_ui(&mut session, "SettingsDartToggleSupport");
    }
    assert!(session.app.dart_settings.enabled);

    click_ui(&mut session, "SettingsDartRestart");
    assert!(session.app.show_settings);
    assert!(
        session.app.ide_panel.lsp_servers.iter().any(|info| info.name == "dart"),
        "restart must keep the dart server registered"
    );
    frame(&mut session);
    assert!(has_ui(&dump(&mut session), "SettingsDartOpenLog"));

    click_ui(&mut session, "SettingsDartOpenLog");
    assert!(!session.app.show_settings);
    assert!(session.app.ide_panel.lsp_logs_expanded.contains("dart"));
    let dart_idx = session
        .app
        .ide_panel
        .lsp_servers
        .iter()
        .position(|info| info.name == "dart")
        .expect("dart server row");
    frame(&mut session);
    let state = dump(&mut session);
    assert_eq!(state["overlays"]["settings"], false);
    assert_eq!(state["ide_panel"]["active"], "lsp");
    assert!(has_ui(&state, &format!("LspServerLogs({dart_idx})")), "{state}");
    // `LspServerClearLogs` needs non-empty logs, which a just-restarted server may not have yet.
    assert!(
        has_ui(&state, &format!("LspLogArea({dart_idx})")),
        "dart logs must be expanded: {state}"
    );
}

#[test]
fn headless_settings_tools_dart_open_log_outside_ide_shows_lsp_panel() {
    let mut session = session_for_test(TEST_WIDTH, TEST_HEIGHT);
    open_general_settings(&mut session);

    click_ui(&mut session, "SettingsDartOpenLog");
    frame(&mut session);
    let state = dump(&mut session);
    assert_eq!(state["overlays"]["settings"], false, "{state}");
    assert_eq!(state["ide_panel"]["active"], "lsp", "{state}");
    let dart_idx = session
        .app
        .ide_panel
        .lsp_servers
        .iter()
        .position(|info| info.name == "dart")
        .expect("dart server row");
    assert!(has_ui(&state, &format!("LspServerLogs({dart_idx})")), "{state}");
}

#[test]
fn headless_settings_tools_row_buttons_leave_room_for_tool_status() {
    let mut session = session_for_test(TEST_WIDTH, TEST_HEIGHT);
    open_general_settings(&mut session);
    let state = dump(&mut session);

    // The Rust Analyzer row makes the directory buttons scroll below the initial viewport.
    // The General tab's right edge is 40 px left of the tool content column.
    let [tab_x, _, tab_w, _] = ui_rect(&state, "SettingsTab(1)");
    let content_x = tab_x + tab_w + 40.0;
    let [refresh_x, _, refresh_w, _] = ui_rect(&state, "SettingsRefreshTools");
    let content_w = refresh_x + refresh_w - content_x;
    let [pick_x, ..] = ui_rect(&state, &format!("SettingsToolPick({})", ToolKind::Git.index()));
    assert!(
        pick_x >= content_x + content_w * 0.3,
        "Git \"Выбрать\" starts at {pick_x}, over the label column at {content_x}: {state}"
    );
}
