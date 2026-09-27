//! Markdown preview / Read mode UI coverage.

use crate::headless::tests_support::{
    click_ui, dump, has_ui, run_script, scratch_dir, ui_rect, wait_until, workspace_with_explorer,
};
use crate::headless::HeadlessSession;
use crate::languages::markdown::{MarkdownBlockKind, MarkdownListKind};
use serde_json::Value;
use std::path::{Path, PathBuf};

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;

fn markdown_fixture(name: &str) -> (PathBuf, PathBuf, String) {
    let dir = scratch_dir(name);
    let path = dir.join("preview.md");
    let mut source = String::from(
        "# Markdown Preview Probe\n\nIntro paragraph.\n\n## Lists\n\n- alpha\n- beta\n\n1. first\n2. second\n\n## Code\n\n```rust\nfn copied_probe() {\n    println!(\"clipboard-probe\");\n}\n```\n",
    );
    for index in 1..36 {
        source.push_str(&format!(
            "\n## Section {index}\n\nParagraph {index} for scrolling. More content in this preview.\n"
        ));
    }
    std::fs::write(&path, &source).expect("write Markdown fixture");
    (dir, path, source)
}

fn open_markdown_read(dir: &Path, path: &Path) -> HeadlessSession {
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, dir);
    let lines = run_script(&mut session, format!("open {}\n", path.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(
        &mut session,
        5000,
        "Markdown tab and mode toggle",
        |session| {
            let state = dump(session);
            state["tabs"].as_array().is_some_and(|tabs| {
                tabs.iter().any(|tab| tab["path"] == path.display().to_string())
            }) && has_ui(&state, "MarkdownModeToggle")
        },
    );

    click_ui(&mut session, "MarkdownModeToggle");
    wait_until(
        &mut session,
        5000,
        "Markdown read layout",
        |session| {
            let lines = run_script(session, b"settle 2000\n");
            lines.iter().any(|line| line.ends_with("settled=true"))
                && session.app.markdown.read_layout.content_height() > 0.0
        },
    );
    session
}

fn markdown_code_copy_id(state: &Value) -> Option<String> {
    state["ui"].as_array()?.iter().find_map(|element| {
        let id = element["id"].as_str()?;
        id.starts_with("MarkdownCodeCopy(").then(|| id.to_owned())
    })
}

fn hover_markdown_code_copy(session: &mut HeadlessSession, block_id: usize, ui_id: &str) {
    let state = dump(session);
    let rect = ui_rect(&state, "MarkdownReadBody");
    let frame = (rect[0] as f32, rect[1] as f32, rect[2] as f32, rect[3] as f32);
    let point = {
        let app = &session.app;
        let renderer = app.renderer.as_ref().expect("headless renderer");
        let mut found = None;
        'scan: for y in (rect[1] as i32 + 8..(rect[1] + rect[3] - 8.0) as i32).step_by(8) {
            for x in (rect[0] as i32 + 8..(rect[0] + rect[2] - 8.0) as i32).step_by(16) {
                if renderer.markdown_read_code_block_at(
                    &app.markdown,
                    app.editor.version,
                    frame,
                    app.scroll_y.current,
                    x as f32,
                    y as f32,
                ) == Some(block_id)
                {
                    found = Some((x, y));
                    break 'scan;
                }
            }
        }
        found.unwrap_or_else(|| panic!("Markdown code block {block_id} has no visible hover area: {state}"))
    };
    let lines = run_script(session, format!("mouse_move {} {}\n", point.0, point.1).as_bytes());
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    wait_until(session, 5000, "hovered Markdown code copy button", |session| {
        has_ui(&dump(session), ui_id)
    });
}

#[test]
fn headless_markdown_read_lays_out_preview_blocks() {
    let (dir, path, source) = markdown_fixture("ui-markdown-layout");
    let mut session = open_markdown_read(&dir, &path);
    let document = session
        .app
        .markdown
        .read_document(session.app.editor.version)
        .expect("parsed Markdown document");
    assert!(document
        .blocks
        .iter()
        .any(|block| matches!(&block.kind, MarkdownBlockKind::Heading { .. })));
    assert!(document
        .blocks
        .iter()
        .any(|block| matches!(&block.kind, MarkdownBlockKind::List(list) if list.kind == MarkdownListKind::Unordered)));
    assert!(document
        .blocks
        .iter()
        .any(|block| matches!(&block.kind, MarkdownBlockKind::List(list) if list.kind == MarkdownListKind::Ordered)));
    assert!(document
        .blocks
        .iter()
        .any(|block| matches!(&block.kind, MarkdownBlockKind::Code(code) if code.fenced)));
    let state = dump(&mut session);
    assert_eq!(state["tabs"][0]["markdown"], true);
    assert!(has_ui(&state, "MarkdownReadBody"), "{state}");
    assert!(has_ui(&state, "MarkdownReadScrollbar"), "{state}");

    let code_start = source.find("```rust").expect("code fence");
    let code_copy_id = format!("MarkdownCodeCopy({code_start})");
    hover_markdown_code_copy(&mut session, code_start, &code_copy_id);
    assert_eq!(
        markdown_code_copy_id(&dump(&mut session)).as_deref(),
        Some(code_copy_id.as_str())
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_markdown_read_wheel_clamps_at_both_ends() {
    let (dir, path, _) = markdown_fixture("ui-markdown-wheel");
    let mut session = open_markdown_read(&dir, &path);
    let max_scroll = session
        .app
        .markdown
        .read_scroll_bounds()
        .expect("reader scroll bounds");
    assert!(max_scroll > 0.0);

    run_script(&mut session, b"mouse_move 600 350\nwheel 0 -1000\n");
    assert_eq!(session.app.scroll_y.target, max_scroll);
    run_script(&mut session, b"wheel 0 -1000\n");
    assert_eq!(session.app.scroll_y.target, max_scroll);

    run_script(&mut session, b"wheel 0 1000\n");
    assert_eq!(session.app.scroll_y.target, 0.0);
    run_script(&mut session, b"wheel 0 1000\n");
    assert_eq!(session.app.scroll_y.target, 0.0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_markdown_read_scrollbar_is_present_and_draggable() {
    let (dir, path, _) = markdown_fixture("ui-markdown-scrollbar");
    let mut session = open_markdown_read(&dir, &path);
    wait_until(&mut session, 5000, "settled Markdown scrollbar geometry", |session| {
        let lines = run_script(session, b"settle 2000\n");
        lines.iter().any(|line| line.ends_with("settled=true"))
    });
    let state = dump(&mut session);
    assert!(has_ui(&state, "MarkdownReadScrollbar"), "{state}");
    let track = ui_rect(&state, "MarkdownReadScrollbar");
    let thumb = crate::render_view::markdown_read::markdown_read_scrollbar_thumb(
        track[1] as f32,
        track[3] as f32,
        session.app.markdown.read_layout.content_height(),
        session.app.scroll_y.current.round(),
        TEST_SCALE,
    )
    .expect("reader scrollbar thumb");
    let x = track[0] + track[2] * 0.5;
    let press_y = thumb.start + thumb.len * 0.5;
    let drag_y = (press_y + 120.0).min(track[1] as f32 + track[3] as f32 - 1.0);
    let before = session.app.scroll_y.target;

    let lines = run_script(
        &mut session,
        format!(
            "mouse_move {x} {press_y}\nclick left down\nmouse_move {x} {drag_y}\n"
        )
        .as_bytes(),
    );
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    wait_until(&mut session, 5000, "Markdown scrollbar drag", |session| {
        session.app.scroll_y.is_dragging && session.app.scroll_y.target > before
    });
    assert!(session.app.scroll_y.is_dragging);
    assert!(session.app.scroll_y.target > before);
    let lines = run_script(
        &mut session,
        format!("mouse_move {x} {drag_y}\nclick left up\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    wait_until(&mut session, 5000, "settled Markdown scrollbar release", |session| {
        !session.app.scroll_y.is_dragging && session.app.scroll_y.is_settled()
    });
    assert!(!session.app.scroll_y.is_dragging);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_markdown_code_copy_and_edit_toggle_preserve_source() {
    let (dir, path, source) = markdown_fixture("ui-markdown-copy-toggle");
    let mut session = open_markdown_read(&dir, &path);
    let code_start = source.find("```rust").expect("code fence");
    let code_copy_id = format!("MarkdownCodeCopy({code_start})");
    hover_markdown_code_copy(&mut session, code_start, &code_copy_id);
    click_ui(&mut session, &code_copy_id);
    assert_eq!(
        dump(&mut session)["clipboard"]["text"],
        "fn copied_probe() {\n    println!(\"clipboard-probe\");\n}\n"
    );

    click_ui(&mut session, "MarkdownModeToggle");
    let state = dump(&mut session);
    assert_eq!(state["tabs"][0]["markdown"], false);
    assert_eq!(session.app.editor.get_full_text(), source);
    let _ = std::fs::remove_dir_all(dir);
}
