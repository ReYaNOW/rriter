use crate::headless::tests_support::{dump, open_file_session, run_script, scratch_dir, ui_rect};
use crate::headless::HeadlessSession;
use serde_json::Value;
use std::path::PathBuf;

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;
const SELECTION_TEXT: &str = "alpha bravo charlie\nsecond line";

fn selection_session(name: &str) -> (PathBuf, HeadlessSession) {
    let dir = scratch_dir(name);
    let file = dir.join("selection.txt");
    std::fs::write(&file, SELECTION_TEXT).expect("write selection fixture");
    let session = open_file_session(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &file);
    (dir, session)
}

fn selection(state: &Value) -> &Value {
    &state["editor"]["selection"]
}

#[test]
fn headless_editor_double_click_selects_word() {
    let (dir, mut session) = selection_session("ui-editor-double-click-word");
    let [body_x, body_y, _, _] = ui_rect(&dump(&mut session), "EditorTextBody");
    // Aim at the middle of "bravo" (bytes 6..11 of line 1) from the renderer's glyph
    // metrics, so the click column does not depend on font width.
    let (x, y) = {
        let app = &mut session.app;
        let scroll_x = app.scroll_x.current.round();
        let scroll_y = app.scroll_y.current.round();
        let renderer = app.renderer.as_mut().expect("headless renderer");
        let text_x = renderer.visual_x_for_byte_offset(&app.editor, 0, 8, true) as f64;
        (
            (body_x + text_x + renderer.ascii_advances['a' as usize] as f64 * 0.5
                - scroll_x as f64)
                .round(),
            (body_y + renderer.line_height as f64 * 0.5 - scroll_y as f64).round(),
        )
    };
    let lines = run_script(&mut session, format!("mouse_move {x} {y}\ndblclick\n").as_bytes());
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    let state = dump(&mut session);
    assert_eq!(selection(&state), &serde_json::json!({"start": [1, 7], "end": [1, 12]}), "{state}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_shift_home_and_end_expand_selection() {
    let (dir, mut session) = selection_session("ui-editor-shift-home-end");
    run_script(&mut session, b"key ctrl+home\nkey shift+end\n");
    let to_end = dump(&mut session);
    assert_eq!(selection(&to_end), &serde_json::json!({"start": [1, 1], "end": [1, 20]}), "{to_end}");

    run_script(&mut session, b"key ctrl+end\nkey shift+home\n");
    let to_start = dump(&mut session);
    assert_eq!(selection(&to_start), &serde_json::json!({"start": [2, 1], "end": [2, 12]}), "{to_start}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_ctrl_a_selects_entire_document() {
    let (dir, mut session) = selection_session("ui-editor-ctrl-a");
    run_script(&mut session, b"key ctrl+a\n");
    let state = dump(&mut session);
    assert_eq!(selection(&state), &serde_json::json!({"start": [1, 1], "end": [2, 12]}), "{state}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_ctrl_shift_arrows_select_by_words() {
    let (dir, mut session) = selection_session("ui-editor-word-selection");
    run_script(&mut session, b"key ctrl+home\nkey ctrl+shift+right\n");
    let right = dump(&mut session);
    assert_eq!(selection(&right)["start"], serde_json::json!([1, 1]), "{right}");
    assert!(selection(&right)["end"][1].as_u64().unwrap() > 1, "{right}");

    run_script(&mut session, b"key ctrl+end\nkey ctrl+shift+left\n");
    let left = dump(&mut session);
    assert_eq!(selection(&left)["end"], serde_json::json!([2, 12]), "{left}");
    assert!(selection(&left)["start"][1].as_u64().unwrap() < 12, "{left}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_typing_replaces_selection() {
    let (dir, mut session) = selection_session("ui-editor-replace-selection");
    run_script(&mut session, b"key ctrl+home\nkey shift+end\ntype replacement\n");
    let state = dump(&mut session);
    assert_eq!(state["editor"]["lines"], 2, "{state}");
    assert_eq!(state["tabs"][0]["modified"], true, "{state}");
    assert_eq!(selection(&state), &Value::Null, "{state}");
    assert_eq!(state["tabs"][0]["cursor"], serde_json::json!({"line": 1, "col": 12}), "{state}");
    let _ = std::fs::remove_dir_all(dir);
}
