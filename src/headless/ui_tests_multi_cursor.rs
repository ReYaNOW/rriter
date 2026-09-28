use crate::headless::tests_support::{
    dump, open_file_session, run_script, scratch_dir, ui_rect, wait_until,
};
use crate::headless::HeadlessSession;
use serde_json::Value;
use std::path::PathBuf;

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;

fn multi_cursor_session(name: &str, text: &str) -> (PathBuf, HeadlessSession) {
    let dir = scratch_dir(name);
    let path = dir.join("multi.rs");
    if let Err(error) = std::fs::write(&path, text) {
        panic!("write multi-cursor fixture: {error}");
    }
    (dir, open_file_session(WIDTH, HEIGHT, 1.0, &path))
}

fn click_offset(session: &mut HeadlessSession, offset: usize, alt: bool) {
    let state = dump(session);
    let [body_x, body_y, _, _] = ui_rect(&state, "EditorTextBody");
    let (x, y) = {
        let app = &mut session.app;
        let line = app
            .editor
            .line_offsets
            .partition_point(|&line_start| line_start <= offset)
            .saturating_sub(1);
        let Some(&line_start) = app.editor.line_offsets.get(line) else {
            panic!("no editor line for byte offset {offset}");
        };
        let Some(renderer) = app.renderer.as_mut() else {
            panic!("headless renderer missing");
        };
        let text_x = renderer.visual_x_for_byte_offset(&app.editor, line_start, offset, true);
        let y = body_y
            + renderer.line_height as f64 * (line as f64 + 0.5)
            - app.scroll_y.current as f64;
        (
            (body_x + text_x as f64 - app.scroll_x.current as f64).round(),
            y.round(),
        )
    };
    let command = if alt { "click alt" } else { "click" };
    let lines = run_script(
        session,
        format!("mouse_move {x} {y}\n{command}\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
}

fn click_offsets(session: &mut HeadlessSession, offsets: &[usize]) {
    for &offset in offsets {
        click_offset(session, offset, true);
    }
}

fn extra_offsets(state: &Value) -> Vec<u64> {
    let Some(cursors) = state["editor"]["extra_cursors"].as_array() else {
        panic!("dump is missing extra_cursors: {state}");
    };
    cursors.iter().filter_map(Value::as_u64).collect()
}

#[test]
fn headless_multi_cursor_alt_click_adds_and_toggles_extra_caret() {
    let (dir, mut session) = multi_cursor_session("ui-multi-cursor-toggle", "one\ntwo\nthree");
    click_offset(&mut session, 4, true);
    let added = dump(&mut session);
    assert_eq!(added["editor"]["cursor"], 0, "primary caret moved: {added}");
    assert_eq!(extra_offsets(&added), vec![4], "{added}");

    click_offset(&mut session, 4, true);
    let removed = dump(&mut session);
    assert!(extra_offsets(&removed).is_empty(), "{removed}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_multi_cursor_types_at_three_carets_and_updates_highlights() {
    let (dir, mut session) =
        multi_cursor_session("ui-multi-cursor-type", "name1\nname2\nname3");
    click_offsets(&mut session, &[6, 12]);
    let added = dump(&mut session);
    assert_eq!(extra_offsets(&added), vec![6, 12], "{added}");

    let lines = run_script(&mut session, b"type fn \n");
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    let text = session.app.editor.get_full_text();
    assert_eq!(text, "fn name1\nfn name2\nfn name3");
    wait_until(&mut session, 5000, "multi-cursor highlight update", |session| {
        let state = dump(session);
        state["editor"]["highlight_version"].as_u64()
            .is_some_and(|version| version >= session.app.editor.version)
    });
    let state = dump(&mut session);
    let Some(spans) = state["editor"]["highlight_spans"].as_array() else {
        panic!("dump is missing highlight spans: {state}");
    };
    for start in [0_u64, 9, 18] {
        assert!(
            spans.iter().any(|span| {
                span[0].as_u64().is_some_and(|span_start| span_start <= start)
                    && span[1].as_u64().is_some_and(|span_end| span_end > start)
            }),
            "no syntax span covers `fn` at byte {start}: {state}"
        );
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_multi_cursor_backspace_edits_each_line_and_escape_clears() {
    let (dir, mut session) = multi_cursor_session("ui-multi-cursor-backspace", "one\ntwo\nthree");
    click_offsets(&mut session, &[5, 9]);
    click_offset(&mut session, 1, true);
    assert_eq!(extra_offsets(&dump(&mut session)), vec![5, 9]);

    let lines = run_script(&mut session, b"key backspace\n");
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    assert_eq!(session.app.editor.get_full_text(), "ne\nwo\nhree");

    let before_escape = dump(&mut session);
    let lines = run_script(&mut session, b"key escape\n");
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    let after_escape = dump(&mut session);
    assert!(extra_offsets(&after_escape).is_empty(), "{after_escape}");
    assert_eq!(after_escape["editor"]["cursor"], before_escape["editor"]["cursor"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_multi_cursor_plain_click_clears_extra_carets() {
    let (dir, mut session) = multi_cursor_session("ui-multi-cursor-plain-click", "one\ntwo\nthree");
    click_offsets(&mut session, &[4, 8]);
    assert_eq!(extra_offsets(&dump(&mut session)), vec![4, 8]);
    click_offset(&mut session, 0, false);
    assert!(extra_offsets(&dump(&mut session)).is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_multi_cursor_undo_restores_all_carets_and_redo_reapplies() {
    let (dir, mut session) = multi_cursor_session("ui-multi-cursor-history", "one\ntwo\nthree");
    click_offsets(&mut session, &[4, 8]);
    let lines = run_script(&mut session, b"type X\nkey ctrl+z\n");
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    let undone = dump(&mut session);
    assert_eq!(session.app.editor.get_full_text(), "one\ntwo\nthree");
    assert_eq!(undone["editor"]["cursor"], 0, "{undone}");
    assert_eq!(extra_offsets(&undone), vec![4, 8], "{undone}");

    let lines = run_script(&mut session, b"key ctrl+y\n");
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    assert_eq!(session.app.editor.get_full_text(), "Xone\nXtwo\nXthree");
    assert_eq!(extra_offsets(&dump(&mut session)), vec![5, 10]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_multi_cursor_paste_distributes_three_lines() {
    let (dir, mut session) = multi_cursor_session("ui-multi-cursor-paste", "one\ntwo\nthree");
    click_offsets(&mut session, &[4, 8]);
    session.app.set_clipboard_text("1\n2\n3");
    let lines = run_script(&mut session, b"key ctrl+v\n");
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    assert_eq!(session.app.editor.get_full_text(), "1one\n2two\n3three");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_multi_cursor_shift_movement_collapses_to_primary() {
    let (dir, mut session) = multi_cursor_session("ui-multi-cursor-shift", "one\ntwo\nthree");
    click_offsets(&mut session, &[4, 8]);
    let lines = run_script(&mut session, b"key shift+right\n");
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    let state = dump(&mut session);
    assert!(extra_offsets(&state).is_empty(), "{state}");
    assert_ne!(state["editor"]["selection"], Value::Null, "{state}");
    let _ = std::fs::remove_dir_all(dir);
}
