//! Sticky-line navigation and geometry regressions.

use crate::headless::tests_support::{
    assert_ui_rect_inside_window, assert_ui_y_integral, click_ui, dump, has_ui, run_script,
    scratch_dir, session_for_test, ui_rect, wait_until,
};
use crate::headless::HeadlessSession;
use serde_json::Value;
use std::path::{Path, PathBuf};

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;

fn nested_rust_fixture(name: &str) -> (PathBuf, PathBuf, usize, usize) {
    let dir = scratch_dir(name);
    let path = dir.join("nested.rs");
    let mut source = String::new();
    source.push_str("fn outer() {\n");
    for i in 0..30 {
        source.push_str(&format!("    let outer_{i} = {i};\n"));
    }

    let inner_one_start = source.len();
    source.push_str("    fn inner_one() {\n");
    for i in 0..80 {
        source.push_str(&format!("        let inner_one_{i} = {i};\n"));
    }
    source.push_str("    }\n");

    let inner_two_start = source.len();
    source.push_str("    fn inner_two() {\n");
    for i in 0..80 {
        source.push_str(&format!("        let inner_two_{i} = {i};\n"));
    }
    source.push_str("    }\n}\nfn after() {\n");
    for i in 0..50 {
        source.push_str(&format!("    let after_{i} = {i};\n"));
    }
    source.push_str("}\n");

    std::fs::write(&path, source).unwrap();
    (dir, path, inner_one_start, inner_two_start)
}

fn open_sticky_file(path: &Path) -> HeadlessSession {
    let mut session = session_for_test(TEST_WIDTH, TEST_HEIGHT);
    let script = format!("scale {TEST_SCALE}\nopen {}\n", path.display());
    let lines = run_script(&mut session, script.as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(&mut session, 5000, "Rust sticky fold ranges", |session| {
        session
            .app
            .editor
            .foldable_ranges_bytes
            .iter()
            .any(|(_, _, is_sticky)| *is_sticky)
    });
    session
}

fn scroll_wheels(session: &mut HeadlessSession, count: usize) {
    for _ in 0..count {
        let lines = run_script(session, b"wheel 0 -5\n");
        assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    }
}

fn sticky_ids(state: &Value) -> Vec<String> {
    state["ui"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|element| element["id"].as_str())
        .filter(|id| id.starts_with("StickyLine("))
        .map(str::to_owned)
        .collect()
}

fn assert_sticky_geometry(state: &Value) {
    for id in sticky_ids(state) {
        assert_ui_rect_inside_window(state, &id);
        let [_, y, _, _] = ui_rect(state, &id);
        assert_ui_y_integral(y, 0.0, &id);
    }
}

#[test]
fn headless_editor_sticky_is_absent_at_file_top_and_shows_outer_block() {
    let (dir, path, inner_one_start, _) = nested_rust_fixture("ui-editor-sticky-outer");
    let mut session = open_sticky_file(&path);
    let top = dump(&mut session);
    assert_eq!(top["tabs"][0]["scroll_y"].as_f64(), Some(0.0));
    assert!(sticky_ids(&top).is_empty(), "sticky rows at file top: {top}");

    scroll_wheels(&mut session, 1);
    let outer_id = "StickyLine(0, 0)".to_string();
    wait_until(&mut session, 5000, "outer sticky row", |session| {
        has_ui(&dump(session), &outer_id)
    });
    let outer = dump(&mut session);
    assert!(has_ui(&outer, &outer_id), "{outer}");
    assert!(!has_ui(&outer, &format!("StickyLine({inner_one_start}, 1)")), "{outer}");
    assert_sticky_geometry(&outer);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_sticky_switches_between_nested_siblings() {
    let (dir, path, inner_one_start, inner_two_start) =
        nested_rust_fixture("ui-editor-sticky-siblings");
    let mut session = open_sticky_file(&path);
    let outer_id = "StickyLine(0, 0)".to_string();
    let inner_one_id = format!("StickyLine({inner_one_start}, 1)");
    let inner_two_id = format!("StickyLine({inner_two_start}, 1)");

    scroll_wheels(&mut session, 2);
    wait_until(&mut session, 5000, "first nested sticky row", |session| {
        has_ui(&dump(session), &inner_one_id)
    });
    let first = dump(&mut session);
    assert!(has_ui(&first, &outer_id), "{first}");
    assert!(has_ui(&first, &inner_one_id), "{first}");
    assert!(!has_ui(&first, &inner_two_id), "{first}");
    assert_sticky_geometry(&first);

    scroll_wheels(&mut session, 4);
    wait_until(&mut session, 5000, "second nested sticky row", |session| {
        let state = dump(session);
        has_ui(&state, &inner_two_id) && !has_ui(&state, &inner_one_id)
    });
    let second = dump(&mut session);
    assert!(has_ui(&second, &outer_id), "{second}");
    assert!(has_ui(&second, &inner_two_id), "{second}");
    assert!(!has_ui(&second, &inner_one_id), "{second}");
    assert_sticky_geometry(&second);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_sticky_click_moves_cursor_to_nested_header() {
    let (dir, path, _, inner_two_start) = nested_rust_fixture("ui-editor-sticky-click");
    let mut session = open_sticky_file(&path);
    scroll_wheels(&mut session, 6);

    let outer_id = "StickyLine(0, 0)".to_string();
    let inner_two_id = format!("StickyLine({inner_two_start}, 1)");
    wait_until(&mut session, 5000, "clickable nested sticky row", |session| {
        let state = dump(session);
        has_ui(&state, &outer_id) && has_ui(&state, &inner_two_id)
    });
    let sticky = dump(&mut session);
    assert_sticky_geometry(&sticky);
    click_ui(&mut session, &inner_two_id);

    let expected_line = session
        .app
        .editor
        .line_offsets
        .partition_point(|&offset| offset <= inner_two_start);
    wait_until(&mut session, 5000, "sticky header navigation", |session| {
        dump(session)["tabs"][0]["cursor"]["line"].as_u64() == Some(expected_line as u64)
    });
    let navigated = dump(&mut session);
    assert_eq!(
        navigated["tabs"][0]["cursor"]["line"].as_u64(),
        Some(expected_line as u64),
        "{navigated}"
    );
    let _ = std::fs::remove_dir_all(dir);
}
