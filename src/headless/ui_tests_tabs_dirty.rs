use crate::app::PendingAction;
use crate::app::events::host_loop::HostLoop;
use crate::headless::HeadlessSession;
use crate::headless::tests_support::{dirty_ide_tab, dump, run_script, wait_until};
use std::sync::atomic::Ordering;

/// Ctrl+4 on the dirty tab opens the unsaved-changes dialog for that tab.
fn open_close_dialog(session: &mut HeadlessSession) {
    run_script(session, b"key ctrl+4\nsettle 500\n");
    assert_eq!(dump(session)["dialog"]["action"], "CloseTab");
}

/// Whole editor text through the in-memory clipboard.
fn editor_text(session: &mut HeadlessSession) -> String {
    run_script(session, b"key ctrl+a\nkey ctrl+c\n");
    dump(session)["clipboard"]["text"].as_str().unwrap_or_default().to_string()
}

fn tab_count(session: &mut HeadlessSession) -> usize {
    dump(session)["tabs"].as_array().map_or(0, Vec::len)
}

#[test]
fn headless_tabs_dirty_close_dialog_save_closes_tab() {
    // The written file is checked by `headless_dialog_save_writes_file`; this covers the close.
    let (dir, _file, mut session) = dirty_ide_tab("ui-tabs-dirty-save", b"original\n");
    open_close_dialog(&mut session);
    assert_eq!(run_script(&mut session, b"dialog save\n"), ["ok"]);
    // The tab closes only after the async write lands.
    wait_until(&mut session, 5000, "close of the saved dirty tab", |s| tab_count(s) == 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_tabs_dirty_close_dialog_save_keeps_crlf() {
    let (dir, file, mut session) = dirty_ide_tab("ui-tabs-dirty-crlf", b"alpha\r\nbeta\r\n");
    open_close_dialog(&mut session);
    assert_eq!(run_script(&mut session, b"dialog save\n"), ["ok"]);
    wait_until(&mut session, 5000, "CRLF save and close of the dirty tab", |s| {
        tab_count(s) == 0 && std::fs::read(&file).unwrap() == b"changedalpha\r\nbeta\r\n"
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_tabs_dirty_close_dialog_cancel_keeps_text_and_dirty_marker() {
    let (dir, file, mut session) = dirty_ide_tab("ui-tabs-dirty-cancel", b"original\n");
    open_close_dialog(&mut session);
    assert_eq!(run_script(&mut session, b"dialog cancel\n"), ["ok"]);
    run_script(&mut session, b"settle 500\n");
    let state = dump(&mut session);
    assert_eq!(state["dialog"], serde_json::Value::Null, "{state}");
    assert_eq!(state["tabs"].as_array().unwrap().len(), 1);
    assert_eq!(state["tabs"][0]["modified"], true);
    assert_eq!(editor_text(&mut session), "changedoriginal\n");
    assert_eq!(std::fs::read(&file).unwrap(), b"original\n");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_tabs_dirty_close_dialog_ignores_click_outside() {
    // The dialog is modal: a main-window click only refocuses it and must not reach the editor.
    let (dir, file, mut session) = dirty_ide_tab("ui-tabs-dirty-outside", b"original\n");
    open_close_dialog(&mut session);
    let before = dump(&mut session);
    run_script(&mut session, b"mouse_move 300 300\nclick\nsettle 1000\n");
    let state = dump(&mut session);
    assert_eq!(state["dialog"]["action"], "CloseTab", "{state}");
    // A click reaching the editor would move the caret or change the selection.
    assert_eq!(state["editor"]["selection"], before["editor"]["selection"], "{state}");
    assert_eq!(state["tabs"][0]["cursor"], before["tabs"][0]["cursor"], "{state}");
    assert_eq!(state["tabs"].as_array().unwrap().len(), 1);
    assert_eq!(state["tabs"][0]["modified"], true);
    assert_eq!(run_script(&mut session, b"dialog cancel\n"), ["ok"]);
    assert_eq!(editor_text(&mut session), "changedoriginal\n");
    assert_eq!(std::fs::read(&file).unwrap(), b"original\n");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_tabs_dirty_close_dialog_escape_cancels() {
    // Escape should act like closing the dialog window, which cancels the pending action.
    let (dir, file, mut session) = dirty_ide_tab("ui-tabs-dirty-escape", b"original\n");
    open_close_dialog(&mut session);
    run_script(&mut session, b"key escape\nsettle 500\n");
    let state = dump(&mut session);
    assert_eq!(state["dialog"], serde_json::Value::Null, "{state}");
    assert_eq!(state["tabs"].as_array().unwrap().len(), 1);
    assert_eq!(state["tabs"][0]["modified"], true);
    assert_eq!(editor_text(&mut session), "changedoriginal\n");
    assert_eq!(std::fs::read(&file).unwrap(), b"original\n");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_tabs_close_after_ctrl_s_skips_dialog() {
    let (dir, file, mut session) = dirty_ide_tab("ui-tabs-dirty-ctrl-s", b"original\n");
    run_script(&mut session, b"key ctrl+s\n");
    wait_until(&mut session, 5000, "Ctrl+S save of the dirty tab", |s| {
        dump(s)["tabs"][0]["modified"] == false && std::fs::read(&file).unwrap() == b"changedoriginal\n"
    });
    run_script(&mut session, b"key ctrl+4\nsettle 1000\n");
    let state = dump(&mut session);
    assert_eq!(state["dialog"], serde_json::Value::Null, "{state}");
    assert!(state["tabs"].as_array().is_none_or(Vec::is_empty), "{state}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_tabs_dirty_close_dialog_keeps_its_action_over_a_quit_request() {
    let (dir, file, mut session) = dirty_ide_tab("ui-tabs-dirty-quit-over", b"original\n");
    open_close_dialog(&mut session);
    // Seeded: headless has no main-window close button; its `CloseRequested` handler asks this.
    session.app.show_action_dialog(&HostLoop::headless(&session.loop_state), PendingAction::Quit);
    assert_eq!(dump(&mut session)["dialog"]["action"], "CloseTab");
    // Discard answers the question on screen: the tab closes, the editor keeps running.
    assert_eq!(run_script(&mut session, b"dialog discard\n"), ["ok"]);
    wait_until(&mut session, 5000, "close of the discarded dirty tab", |s| tab_count(s) == 0);
    assert!(!session.loop_state.exit_requested.load(Ordering::Relaxed));
    assert_eq!(std::fs::read(&file).unwrap(), b"original\n");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_tabs_dirty_vanished_save_picker_disarms_the_close() {
    let (dir, file, mut session) = dirty_ide_tab("ui-tabs-dirty-picker-gone", b"original\n");
    open_close_dialog(&mut session);
    // Seeded: the Save-As picker is native and has no headless path. Move the flow to its
    // Save-As phase and hand it a picker channel whose thread already died.
    session.app.confirm_dialog.begin_save_as(vec![0]);
    let (tx, rx) = std::sync::mpsc::channel();
    drop(tx);
    session.app.save_file_rx = Some(rx);
    run_script(&mut session, b"mouse_move 0 0\nsettle 500\n");
    assert!(session.app.save_file_rx.is_none(), "the dead picker channel was polled");
    assert_eq!(session.app.confirm_dialog.action(), PendingAction::None);
    let state = dump(&mut session);
    assert_eq!(state["dialog"], serde_json::Value::Null, "{state}");
    assert_eq!(state["tabs"].as_array().unwrap().len(), 1);
    assert_eq!(state["tabs"][0]["modified"], true);
    assert_eq!(std::fs::read(&file).unwrap(), b"original\n");
    let _ = std::fs::remove_dir_all(dir);
}
