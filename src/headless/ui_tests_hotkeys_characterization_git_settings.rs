//! Characterization of Git, Settings, Welcome, and modal keyboard ownership.

use crate::app::git_panel::GitLogTextPoint;
use crate::app::{PanelId, PendingAction};
use crate::headless::tests_support::{
    click_ui, dump, has_ui, open_settings_tab, run_script, session_for_test, terminal_session,
    wait_until,
};
use crate::headless::HeadlessSession;
use crate::platform::ToolKind;

pub(super) const EXCLUDED_HOTKEY_ROWS: &[(&str, &str)] = &[
    (
        "Git graph tooltip drag selection",
        "the prebuilt binary rendered GitGraphCommit rows, but dump exposes neither tooltip text nor its text-row coordinates, so drag selection could not be established",
    ),
];

fn run_ok(session: &mut HeadlessSession, script: &str) {
    let lines = run_script(session, script.as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
}

fn seed_git_log_selection(session: &mut HeadlessSession) {
    session.app.ide_panel.open(PanelId::Git);
    session.app.ide_panel.git.toggle_logs_pane();
    session.app.ide_panel.git.seed_git_log_for_test("selected git log text");
    let line = session
        .app
        .ide_panel
        .git
        .git_logs
        .display_line_at(0)
        .expect("seeded Git log line");
    assert!(session.app.ide_panel.git.git_logs.set_selection(
        GitLogTextPoint {
            line: line.id(),
            byte: 0,
        },
        GitLogTextPoint {
            line: line.id(),
            byte: line.byte_len(),
        },
    ));
    session.app.ide_panel.git.claim_git_logs_copy_owner();
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

#[test]
fn headless_hotkeys_git_selection_copy_respects_terminal_focus() {
    let (dir, mut session) = crate::headless::tests_support::keyboard_session("ui-hotkeys-git-copy");
    seed_git_log_selection(&mut session);
    run_ok(&mut session, "key ctrl+c\n");
    assert_eq!(
        dump(&mut session)["clipboard"]["text"].as_str(),
        Some("selected git log text")
    );
    drop(session);
    let _ = std::fs::remove_dir_all(dir);

    let (dir, mut session) = terminal_session("ui-hotkeys-git-copy-terminal");
    click_ui(&mut session, "TerminalBody");
    assert!(session.app.ide_panel.terminal_focused);
    seed_git_log_selection(&mut session);
    run_ok(&mut session, "key ctrl+c\n");
    assert_eq!(dump(&mut session)["clipboard"]["text"], serde_json::Value::Null);
    drop(session);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_settings_installer_log_ctrl_c_copies_the_log() {
    const PICKER_BUSY_ERROR: &str = "Окно выбора инструмента уже открыто";

    let mut session = session_for_test(1280, 720);
    open_settings_tab(&mut session, 1);
    let picker_id = format!("SettingsToolPick({})", ToolKind::Ruff.index());
    wait_until(&mut session, 2000, "General settings tools", |session| {
        has_ui(&dump(session), &picker_id)
    });

    let (_picker_tx, picker_rx) = std::sync::mpsc::channel();
    session.app.settings_tool_picker_rx = Some(picker_rx);
    click_ui(
        &mut session,
        &format!("SettingsToolPick({})", ToolKind::Ruff.index()),
    );
    assert!(session.app.tool_installer.is_log_open());
    assert!(has_ui(
        &dump(&mut session),
        "SettingsCopyToolInstallLog"
    ));

    run_ok(&mut session, "key ctrl+c\n");
    let copied = dump(&mut session)["clipboard"]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(copied.contains(&format!("[error] {PICKER_BUSY_ERROR}")), "{copied:?}");
    assert!(session.app.tool_installer.is_log_open());
}

#[test]
fn headless_hotkeys_welcome_ctrl_q_exits_the_session() {
    let mut session = session_for_test(1280, 720);
    assert!(session.app.show_welcome);

    run_ok(&mut session, "key ctrl+q\n");
    assert!(session.loop_state.exit_requested.load(std::sync::atomic::Ordering::Relaxed));
}

#[test]
fn headless_hotkeys_dialog_over_terminal_owns_the_input_matrix() {
    let (dir, mut session) = terminal_session("ui-hotkeys-dialog-terminal-owner");
    click_ui(&mut session, "TerminalBody");
    assert!(session.app.ide_panel.terminal_focused);
    run_ok(&mut session, "type matrix-input\n");
    wait_until(&mut session, 3000, "terminal prompt input", |session| {
        terminal_grid_text(session).contains("matrix-input")
    });
    let terminal = &session.app.ide_panel.terminals[session.app.ide_panel.active_terminal];
    let cursor_before = crate::app::terminal::lock_terminal_grid(&terminal.grid).cur_x;
    session.app.confirm_dialog = crate::app::ConfirmDialog::armed_with(PendingAction::CloseAllTabs);
    assert!(session.app.modal_dialog_open());
    session.app.set_clipboard_text("dialog clipboard sentinel");

    run_ok(
        &mut session,
        "key ctrl+c\nkey ctrl+v\nkey ctrl+shift+f\nkey ctrl+a\n",
    );
    run_ok(&mut session, "wait 200\n");

    assert!(session.app.modal_dialog_open());
    assert!(session.app.ide_panel.terminal_focused);
    assert!(!session.app.ide_panel.is_open(PanelId::Search));
    assert!(!session.app.show_settings);
    assert_eq!(
        dump(&mut session)["clipboard"]["text"].as_str(),
        Some("dialog clipboard sentinel")
    );
    assert!(terminal_grid_text(&session).contains("matrix-input"));
    assert!(!terminal_grid_text(&session).contains("dialog clipboard sentinel"));
    let terminal = &session.app.ide_panel.terminals[session.app.ide_panel.active_terminal];
    assert_eq!(crate::app::terminal::lock_terminal_grid(&terminal.grid).cur_x, cursor_before);
    drop(session);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_excluded_rows_have_reasons() {
    for (row, reason) in EXCLUDED_HOTKEY_ROWS {
        assert!(!row.is_empty() && !reason.is_empty());
    }
}
