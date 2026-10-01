//! Markdown heading table-of-contents popup UI coverage.

use crate::headless::HeadlessSession;
use crate::headless::tests_support::{
    click_ui, dump, has_ui, run_script, scratch_dir, wait_until, workspace_with_explorer,
};
use serde_json::Value;
use std::path::{Path, PathBuf};

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;
const WAIT_MS: u64 = 15_000;

fn fixture(name: &str, markdown: &str) -> (PathBuf, PathBuf) {
    let dir = scratch_dir(name);
    let path = dir.join("preview.md");
    std::fs::write(&path, markdown).expect("write Markdown fixture");
    (dir, path)
}

fn open_markdown(dir: &Path, path: &Path) -> HeadlessSession {
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, dir);
    let lines = run_script(
        &mut session,
        format!("open {}\nsettle 1000\n", path.display()).as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(&mut session, 5000, "Markdown mode button", |session| {
        has_ui(&dump(session), "MarkdownModeToggle")
    });
    session
}

fn open_toc_with_shortcut(session: &mut HeadlessSession) -> Value {
    let lines = run_script(session, b"key ctrl+shift+o\ndump\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    state_reply(&lines)
}

fn state_reply(lines: &[String]) -> Value {
    let payload = lines.last().and_then(|line| line.strip_prefix("ok "))
        .unwrap_or_else(|| panic!("expected dump reply: {lines:?}"));
    serde_json::from_str(payload).expect("dump JSON")
}

fn many_lines(count: usize) -> String {
    let mut text = String::new();
    for index in 0..count {
        text.push_str(&format!("Paragraph {index} keeps the heading away from the document end.\n\n"));
    }
    text
}

#[test]
fn toc_read_navigation_edit_refresh_and_shortcut_routing() {
    let mut markdown = String::from("# First\n\n");
    markdown.push_str(&many_lines(12));
    markdown.push_str("## Second\n\nTarget body.\n\n");
    markdown.push_str(&many_lines(24));
    let (dir, path) = fixture("markdown-toc", &markdown);
    std::fs::write(dir.join("plain.rs"), "fn main() {}\n").expect("write Rust tab");
    let mut session = open_markdown(&dir, &path);

    click_ui(&mut session, "MarkdownModeToggle");
    wait_until(&mut session, WAIT_MS, "Markdown Reader layout", |session| {
        session.app.markdown.read_layout.content_height() > 0.0
    });
    let lines = run_script(&mut session, b"key ctrl+o\ndump\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert_eq!(state_reply(&lines)["external_request"]["kind"], "pick_file");
    let state = open_toc_with_shortcut(&mut session);
    assert_eq!(state["markdown_toc"]["open"], true);
    assert_eq!(state["markdown_toc"]["items"].as_array().map(Vec::len), Some(2));
    let second_target = {
        let headings = session
            .app
            .markdown
            .read_document(session.app.editor.version)
            .expect("Markdown document")
            .headings(&session.app.markdown.read_source);
        session
            .app
            .markdown
            .read_layout
            .source_target_y(&headings[1].source_range)
            .expect("second heading position")
    };
    let lines = run_script(&mut session, b"key down\nkey enter\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(&mut session, WAIT_MS, "Reader scroll reaches second heading", |session| {
        !session.app.markdown_toc.open && (session.app.scroll_y.current - second_target).abs() < 1.0
    });

    click_ui(&mut session, "MarkdownModeToggle");
    let replacement = format!("# Edited\n\n## Second\n\n{}", many_lines(30));
    let escaped_replacement = replacement.replace('\n', "\\n");
    let type_command = format!("key ctrl+a\ntype {escaped_replacement}\nsettle 500\n");
    let lines = run_script(&mut session, type_command.as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let state = open_toc_with_shortcut(&mut session);
    assert_eq!(state["markdown_toc"]["open"], true);
    assert_eq!(state["markdown_toc"]["items"][0], "Edited");
    let lines = run_script(&mut session, b"key enter\ndump\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let state = state_reply(&lines);
    assert_eq!(state["markdown_toc"]["open"], false);
    assert_eq!(state["tabs"][0]["cursor"]["line"], 1);
    let lines = run_script(&mut session, b"key ctrl+shift+o\nkey escape\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert_eq!(dump(&mut session)["markdown_toc"]["open"], false);

    let lines = run_script(
        &mut session,
        format!("open {}\nsettle 500\n", dir.join("plain.rs").display()).as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    // The prebuilt release probe on 2026-10-01 showed Ctrl+Shift+O in src/main.rs requests pick_file.
    let lines = run_script(&mut session, b"key ctrl+shift+o\ndump\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert_eq!(state_reply(&lines)["external_request"]["kind"], "pick_file");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn toc_button_and_empty_document_keep_an_open_empty_popup() {
    let (dir, path) = fixture("markdown-toc-empty", "");
    let mut session = open_markdown(&dir, &path);
    click_ui(&mut session, "MarkdownTocToggle");
    let state = dump(&mut session);
    assert_eq!(state["markdown_toc"]["open"], true);
    assert_eq!(state["markdown_toc"]["items"].as_array().map(Vec::len), Some(0));
    assert_eq!(state["markdown_toc"]["selected"], Value::Null);
    assert!(has_ui(&state, "MarkdownTocToggle"));
    let lines = run_script(&mut session, b"key down\nkey enter\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert_eq!(dump(&mut session)["markdown_toc"]["open"], true);
    let lines = run_script(&mut session, b"key escape\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert_eq!(dump(&mut session)["markdown_toc"]["open"], false);
    let lines = run_script(&mut session, b"key ctrl+shift+o\nmouse_move 20 20\nclick left\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert_eq!(dump(&mut session)["markdown_toc"]["open"], false);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn toc_arrow_selection_clamps_at_the_first_and_last_heading() {
    let (dir, path) = fixture(
        "markdown-toc-edges",
        "# First\n\n## Second\n\n### Third\n\n#### Fourth\n",
    );
    let mut session = open_markdown(&dir, &path);
    let _ = open_toc_with_shortcut(&mut session);
    let lines = run_script(&mut session, b"key down\ndump\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let state = state_reply(&lines);
    assert_eq!(state["markdown_toc"]["selected"], 1);
    assert_eq!(state["markdown_toc"]["open"], true);
    assert_eq!(state["markdown_toc"]["items"].as_array().map(Vec::len), Some(4));
    assert_eq!(state["editor"]["cursor"], 0);
    let lines = run_script(&mut session, b"key up\nkey up\nkey up\nkey up\ndump\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert_eq!(state_reply(&lines)["markdown_toc"]["selected"], 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn toc_ime_commit_does_not_change_editor_text_while_open() {
    let (dir, path) = fixture("markdown-toc-ime", "# First\n\n## Second\n");
    let mut session = open_markdown(&dir, &path);
    let before = session.app.editor.get_full_text();
    let _ = open_toc_with_shortcut(&mut session);
    session.app.handle_main_ime_commit("committed text");
    assert!(session.app.editor.text_equals(&before));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn toc_click_activates_the_selected_heading() {
    let mut markdown = String::from("# First\n\n");
    markdown.push_str(&many_lines(16));
    markdown.push_str("## Second\n\n");
    markdown.push_str(&many_lines(20));
    let (dir, path) = fixture("markdown-toc-click", &markdown);
    let mut session = open_markdown(&dir, &path);
    click_ui(&mut session, "MarkdownModeToggle");
    wait_until(&mut session, WAIT_MS, "Markdown Reader layout", |session| {
        session.app.markdown.read_layout.content_height() > 0.0
    });
    let heading_target = {
        let headings = session
            .app
            .markdown
            .read_document(session.app.editor.version)
            .expect("Markdown document")
            .headings(&session.app.markdown.read_source);
        session
            .app
            .markdown
            .read_layout
            .source_target_y(&headings[1].source_range)
            .expect("second heading position")
    };
    let _ = open_toc_with_shortcut(&mut session);
    wait_until(&mut session, 5000, "second TOC row", |session| {
        has_ui(&dump(session), "MarkdownTocItem(1)")
    });
    click_ui(&mut session, "MarkdownTocItem(1)");
    wait_until(&mut session, WAIT_MS, "clicked TOC heading scroll", |session| {
        !session.app.markdown_toc.open && (session.app.scroll_y.current - heading_target).abs() < 1.0
    });
    let _ = std::fs::remove_dir_all(dir);
}
