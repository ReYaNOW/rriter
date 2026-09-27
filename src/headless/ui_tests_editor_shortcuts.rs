use crate::headless::tests_support::{dump, open_file_session, run_script, scratch_dir};

fn assert_editor_state(
    session: &mut crate::headless::HeadlessSession,
    text: &str,
    line: u64,
    col: u64,
) {
    assert_eq!(session.app.editor.get_full_text(), text);
    let state = dump(session);
    assert_eq!(state["tabs"][0]["cursor"], serde_json::json!({"line": line, "col": col}));
}

#[test]
fn headless_editor_shortcuts_line_and_word_navigation() {
    let dir = scratch_dir("ui-editor-shortcuts-navigation");
    let file = dir.join("navigation.txt");
    let original = "alpha beta\n\nomega";
    std::fs::write(&file, original).unwrap();
    let mut session = open_file_session(1280, 720, 4.0 / 3.0, &file);

    run_script(&mut session, b"key end\n");
    assert_editor_state(&mut session, original, 1, 11);
    run_script(&mut session, b"key home\n");
    assert_editor_state(&mut session, original, 1, 1);
    run_script(&mut session, b"key ctrl+right\n");
    assert_editor_state(&mut session, original, 1, 6);
    run_script(&mut session, b"key ctrl+end\nkey end\n");
    assert_editor_state(&mut session, original, 3, 6);
    run_script(&mut session, b"key ctrl+right\n");
    assert_editor_state(&mut session, original, 3, 6);
    run_script(&mut session, b"key ctrl+home\nkey ctrl+left\n");
    assert_editor_state(&mut session, original, 1, 1);
    run_script(&mut session, b"key down\nkey end\n");
    assert_editor_state(&mut session, original, 2, 1);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_shortcuts_delete_words_and_respect_file_edges() {
    let dir = scratch_dir("ui-editor-shortcuts-word-delete");
    let file = dir.join("words.txt");
    std::fs::write(&file, "alpha beta").unwrap();
    let mut session = open_file_session(1280, 720, 4.0 / 3.0, &file);

    run_script(&mut session, b"key ctrl+backspace\n");
    assert_editor_state(&mut session, "alpha beta", 1, 1);
    run_script(&mut session, b"key ctrl+delete\n");
    assert_editor_state(&mut session, " beta", 1, 1);
    run_script(&mut session, b"key ctrl+end\nkey ctrl+backspace\n");
    assert_editor_state(&mut session, " ", 1, 2);
    run_script(&mut session, b"key ctrl+backspace\n");
    assert_editor_state(&mut session, "", 1, 1);
    run_script(&mut session, b"key ctrl+backspace\nkey ctrl+delete\n");
    assert_editor_state(&mut session, "", 1, 1);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_shortcuts_enter_keeps_auto_indent_and_handles_empty_line() {
    let dir = scratch_dir("ui-editor-shortcuts-enter-indent");
    let file = dir.join("indent.rs");
    std::fs::write(&file, "fn main() {\n}").unwrap();
    let mut session = open_file_session(1280, 720, 4.0 / 3.0, &file);

    run_script(&mut session, b"key end\nkey enter\n");
    assert_editor_state(&mut session, "fn main() {\n    \n}", 2, 5);
    run_script(&mut session, b"key enter\n");
    assert_editor_state(&mut session, "fn main() {\n    \n    \n}", 3, 5);

    run_script(&mut session, b"key ctrl+a\nkey enter\n");
    assert_editor_state(&mut session, "\n", 2, 1);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_shortcuts_tab_inserts_four_spaces_on_empty_line() {
    let dir = scratch_dir("ui-editor-shortcuts-tab-indent");
    let file = dir.join("empty.txt");
    std::fs::write(&file, "").unwrap();
    let mut session = open_file_session(1280, 720, 4.0 / 3.0, &file);

    run_script(&mut session, b"key tab\n");
    assert_editor_state(&mut session, "    ", 1, 5);
    run_script(&mut session, b"key ctrl+end\nkey tab\n");
    assert_editor_state(&mut session, "        ", 1, 9);

    let _ = std::fs::remove_dir_all(dir);
}
