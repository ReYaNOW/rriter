use crate::headless::tests_support::{dump, open_file_session, run_script, scratch_dir, ui_rect};
use crate::headless::HeadlessSession;

fn open_nested_search_session(
    name: &str,
    width: u32,
    height: u32,
) -> (std::path::PathBuf, HeadlessSession) {
    let dir = scratch_dir(name);
    let file = dir.join("search_center.rs");
    let mut source = String::from("fn outer() {\n");
    for i in 0..80 {
        source.push_str(&format!("    let outer_padding_{i} = {i};\n"));
    }
    source.push_str("    fn nested() {\n");
    for i in 0..30 {
        source.push_str(&format!("        let nested_padding_{i} = {i};\n"));
    }
    source.push_str("        let needle_first = 1;\n");
    for i in 0..20 {
        source.push_str(&format!("        let nested_tail_{i} = {i};\n"));
    }
    source.push_str("        let needle_next = 2;\n    }\n}\n");
    std::fs::write(&file, source).expect("write nested search fixture");
    let session = open_file_session(width, height, 4.0 / 3.0, &file);
    (dir, session)
}

fn assert_match_in_center_band(session: &mut HeadlessSession) {
    let state = dump(session);
    let body = ui_rect(&state, "EditorTextBody");
    // The dump reports 1-based lines; the row index is one less.
    let match_line = state["tabs"][0]["cursor"]["line"].as_u64().unwrap() as f64;
    let scroll_y = state["tabs"][0]["scroll_y"].as_f64().unwrap();
    let line_height = session.app.renderer.as_ref().unwrap().line_height as f64;
    // Top of the match line; search_anchor_central_band_target snaps this to
    // the nearest edge of the 35-65% band.
    let match_y = body[1] + (match_line - 1.0) * line_height - scroll_y;
    let sticky_bottom = state["ui"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|element| {
            element["id"]
                .as_str()
                .is_some_and(|id| id.starts_with("StickyLine("))
        })
        .map(|element| {
            let rect = element["rect"].as_array().unwrap();
            rect[1].as_f64().unwrap() + rect[3].as_f64().unwrap()
        })
        .max_by(f64::total_cmp)
        .expect("nested scopes should show sticky lines");
    let relative_y = (match_y - body[1]) / body[3];
    // The scroll target is rounded to whole pixels.
    let pixel = 1.0 / body[3];

    assert!(
        (0.35 - pixel..=0.65 + pixel).contains(&relative_y),
        "match line {match_line} at y={match_y} ({:.1}%) outside center band; body={body:?}, scroll_y={scroll_y}, line_height={line_height}",
        relative_y * 100.0
    );
    assert!(
        match_y >= sticky_bottom,
        "match at y={match_y} is under sticky lines ending at {sticky_bottom}"
    );
}

fn search_first_and_next_match(width: u32, height: u32, name: &str) {
    let (dir, mut session) = open_nested_search_session(name, width, height);
    let lines = run_script(&mut session, b"key ctrl+f\ntype needle\nsettle 3000\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert_match_in_center_band(&mut session);

    let lines = run_script(&mut session, b"key enter\nsettle 3000\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert_match_in_center_band(&mut session);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_search_match_stays_centered_at_2560x1440() {
    search_first_and_next_match(2560, 1440, "ui-editor-search-center-large");
}

#[test]
fn headless_editor_search_match_stays_centered_at_1280x720() {
    search_first_and_next_match(1280, 720, "ui-editor-search-center-small");
}
