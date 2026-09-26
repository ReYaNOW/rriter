use crate::headless::tests_support::{
    assert_rect_inside_window, run_script, scratch_dir, session_for_test, wait_until,
};
use crate::headless::HeadlessSession;
use std::path::PathBuf;

const COMPLETION_SOURCE: &str = "fn primary() {}\nfn print_other() {}\nfn example() {\n    pri";
const COMPLETION_SCALE: f32 = 4.0 / 3.0;

fn completion_session(name: &str, source: &str) -> (HeadlessSession, PathBuf) {
    let dir = scratch_dir(name);
    let file = dir.join("completion.rs");
    std::fs::write(&file, source).expect("write completion fixture");
    let mut session = session_for_test(1280, 720);
    let lines = run_script(
        &mut session,
        format!(
            "scale {COMPLETION_SCALE}\nopen {}\nsettle 2000\nkey ctrl+end\n",
            file.display()
        )
        .as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    (session, dir)
}

fn trigger_completion(session: &mut HeadlessSession) {
    let lines = run_script(session, b"key ctrl+space\n");
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    wait_until(session, 8000, "Tree-sitter completion popup", |session| {
        session.app.autocomplete_active && !session.app.autocomplete_options.is_empty()
    });
}

fn completion_words(session: &HeadlessSession) -> Vec<&str> {
    session
        .app
        .autocomplete_options
        .iter()
        .map(|(item, _)| item.word.as_str())
        .collect()
}

// The popup height grows with the open animation; rows below the animated height are
// outside `autocomplete_rect`, and a click there closes completion instead of hitting a row.
fn wait_for_completion_rect(session: &mut HeadlessSession) {
    wait_until(session, 8000, "fully opened completion popup rectangle", |session| {
        let scale = session.app.renderer.as_ref().unwrap().scale_factor;
        let rows = session.app.autocomplete_options.len().clamp(1, 7) as f32;
        session
            .app
            .autocomplete_rect
            .is_some_and(|(_, _, width, height)| width > 0.0 && height >= rows * 36.0 * scale - 0.5)
    });
}

fn click_completion_row(session: &mut HeadlessSession, row: usize) {
    let (x, y) = {
        let (rx, ry, _, _) = session.app.autocomplete_rect.unwrap();
        let scale = session.app.renderer.as_ref().unwrap().scale_factor;
        (rx + 24.0 * scale, ry + (row as f32 + 0.5) * 36.0 * scale)
    };
    let lines = run_script(
        session,
        format!("mouse_move {x} {y}\nclick\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
}

#[test]
fn headless_editor_completion_filters_tree_sitter_items_and_escape_preserves_text() {
    let (mut session, dir) = completion_session("ui-editor-completion-filter", COMPLETION_SOURCE);
    trigger_completion(&mut session);

    let initial = completion_words(&session);
    assert!(initial.contains(&"print!"), "missing print! in {initial:?}");
    assert!(initial.contains(&"println!"), "missing println! in {initial:?}");

    // Filtering is fuzzy, so `print` keeps every initial item it matches (`println!` too)
    // and drops the rest; the list must equal the initial one filtered by the same matcher.
    let mut expected: Vec<String> = initial
        .iter()
        .filter(|word| crate::app::autocomplete_match_candidate("print", word).is_some())
        .map(|word| word.to_string())
        .collect();
    expected.sort();
    assert!(expected.iter().any(|word| word == "print!"), "{expected:?}");
    assert!(expected.iter().any(|word| word == "println!"), "{expected:?}");
    let lines = run_script(&mut session, b"type nt\n");
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    wait_until(&mut session, 8000, "completion filtering for print", |session| {
        let mut words = completion_words(session);
        words.sort_unstable();
        session.app.autocomplete_active && words == expected
    });
    let typed_text = session.app.editor.get_full_text();
    assert!(typed_text.ends_with("print"), "typed prefix changed: {typed_text:?}");

    let lines = run_script(&mut session, b"key escape\n");
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    assert!(!session.app.autocomplete_active, "Escape should close completion");
    assert_eq!(session.app.editor.get_full_text(), typed_text);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_completion_enter_and_tab_apply_selection_and_place_cursor_after_text() {
    for (name, key) in [("enter", "enter"), ("tab", "tab")] {
        let (mut session, dir) = completion_session(
            &format!("ui-editor-completion-{name}"),
            COMPLETION_SOURCE,
        );
        trigger_completion(&mut session);
        let lines = run_script(&mut session, b"key down\n");
        assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
        assert_eq!(session.app.autocomplete_selected_idx, 1);
        let lines = run_script(&mut session, b"key up\n");
        assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
        assert_eq!(session.app.autocomplete_selected_idx, 0);
        let lines = run_script(&mut session, b"key down\n");
        assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
        assert_eq!(session.app.autocomplete_selected_idx, 1);

        let lines = run_script(&mut session, format!("key {key}\n").as_bytes());
        assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
        let expected = format!("{}println!", COMPLETION_SOURCE.strip_suffix("pri").unwrap());
        assert_eq!(session.app.editor.get_full_text(), expected);
        assert_eq!(session.app.editor.cursor, expected.len());
        assert!(!session.app.autocomplete_active);
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn headless_editor_completion_mouse_click_applies_selected_item_at_fractional_scale() {
    let (mut session, dir) = completion_session("ui-editor-completion-mouse-selected", COMPLETION_SOURCE);
    trigger_completion(&mut session);
    wait_for_completion_rect(&mut session);
    click_completion_row(&mut session, 0);
    let expected = format!("{}print!", COMPLETION_SOURCE.strip_suffix("pri").unwrap());
    assert_eq!(session.app.editor.get_full_text(), expected);
    assert_eq!(session.app.editor.cursor, expected.len());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_completion_mouse_first_click_selects_second_click_applies_item() {
    let (mut session, dir) = completion_session("ui-editor-completion-mouse", COMPLETION_SOURCE);
    trigger_completion(&mut session);
    wait_for_completion_rect(&mut session);
    let original = session.app.editor.get_full_text();
    click_completion_row(&mut session, 1);
    assert_eq!(session.app.editor.get_full_text(), original);
    assert!(session.app.autocomplete_active, "first click should keep completion open");
    assert_eq!(session.app.autocomplete_selected_idx, 1);
    assert_eq!(completion_words(&session)[1], "println!");
    click_completion_row(&mut session, 1);
    let expected = format!("{}println!", COMPLETION_SOURCE.strip_suffix("pri").unwrap());
    assert_eq!(session.app.editor.get_full_text(), expected);
    assert_eq!(session.app.editor.cursor, expected.len());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_completion_popup_stays_inside_right_and_lower_window_edges() {
    let mut session = session_for_test(1280, 720);
    let lines = run_script(
        &mut session,
        format!("scale {COMPLETION_SCALE}\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    let (target_row, target_column) = {
        let renderer = session.app.renderer.as_ref().expect("headless renderer");
        let char_advance = renderer.ascii_advances['0' as usize].max(1.0);
        (
            (((renderer.height - 100.0) / renderer.line_height).floor() as usize)
                .saturating_sub(1)
                .max(3),
            ((renderer.width - 200.0) / char_advance).max(1.0) as usize,
        )
    };
    {
        let renderer = session.app.renderer.as_ref().expect("headless renderer");
        let char_advance = renderer.ascii_advances['0' as usize].max(1.0);
        assert!(target_row as f32 * renderer.line_height > renderer.height * 0.75);
        assert!(target_column as f32 * char_advance > renderer.width * 0.75);
    }
    let dir = scratch_dir("ui-editor-completion-edges");
    let file = dir.join("completion.rs");
    let mut source = String::from("fn primary() {}\nfn print_other() {}\nfn example() {\n");
    for row in 3..target_row {
        source.push_str(&format!("    let filler_{row} = 0;\n"));
    }
    source.push_str(&" ".repeat(target_column.saturating_sub(3)));
    source.push_str("pri");
    std::fs::write(&file, source).expect("write edge completion fixture");
    let lines = run_script(
        &mut session,
        format!("open {}\nsettle 2000\nkey ctrl+end\n", file.display()).as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    trigger_completion(&mut session);
    wait_for_completion_rect(&mut session);

    let (x, y, width, height) = session.app.autocomplete_rect.unwrap();
    let renderer = session.app.renderer.as_ref().expect("headless renderer");
    assert_rect_inside_window(
        [x as f64, y as f64, width as f64, height as f64],
        renderer.width as f64,
        renderer.height as f64,
        0.0,
        0.5,
        "completion popup",
    );
    let _ = std::fs::remove_dir_all(dir);
}
