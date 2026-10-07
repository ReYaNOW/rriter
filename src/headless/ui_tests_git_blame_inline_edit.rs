//! Git blame inline: editing, hunk popup, wrap and deadline-wake scenarios.

use super::ui_tests_git_blame_inline::{open_and_wait, TEST_HEIGHT, TEST_SCALE, TEST_WIDTH};
use crate::headless::tests_support::{dump, git, git_blame_fixture, run_script, wait_until, workspace_with_explorer};
use crate::headless::HeadlessSession;
use std::path::Path;

fn open_blame_file(root: &Path, file: &Path) -> HeadlessSession {
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, root);
    session.app.git_blame_inline = true;
    session.app.git_blame_delay_ms = 100;
    open_and_wait(&mut session, file, 3);
    session
}

fn run_ok(session: &mut HeadlessSession, script: &str) -> Vec<String> {
    let lines = run_script(session, script.as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    lines
}

fn wait_for_annotation(session: &mut HeadlessSession, author: &str) -> String {
    wait_until(session, 5000, "inline Git blame annotation", |session| {
        dump(session)["blame_inline"]["text"]
            .as_str()
            .is_some_and(|text| text.contains(author))
    });
    dump(session)["blame_inline"]["text"].as_str().unwrap().to_owned()
}

fn move_cursor_to_line(session: &mut HeadlessSession, line: usize) {
    session.app.editor.cursor = session.app.editor.line_offsets[line];
    run_ok(session, "mouse_move 0 0\n");
}

#[test]
fn headless_git_blame_inline_deadline_wakes_without_idle_frames() {
    let (root, file) = git_blame_fixture("ui-git-blame-inline-deadline");
    let mut session = open_blame_file(&root, &file);
    move_cursor_to_line(&mut session, 1);

    let wakes = run_ok(&mut session, "wake 1000\nwake 1000\nwake 1000\ndump\n");
    assert!(wakes.iter().any(|line| line.contains("cause=deadline") && line.contains("frame=true")), "{wakes:?}");
    let state = serde_json::from_str::<serde_json::Value>(wakes.last().unwrap().strip_prefix("ok ").unwrap()).unwrap();
    assert!(state["blame_inline"]["text"].as_str().is_some_and(|text| text.contains("Grace")), "{state}");

    let idle = run_ok(&mut session, "idle 100\ndump\n");
    assert!(idle[0].contains("frames=0") && idle[0].contains("deadlines=0"), "{idle:?}");
    let state = serde_json::from_str::<serde_json::Value>(idle[1].strip_prefix("ok ").unwrap()).unwrap();
    assert_eq!(state["event_loop"]["control_flow"], "wait");
    assert!(state["event_loop"]["deadline_ms"].is_null());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn headless_git_blame_inline_typing_hides_and_skips_the_modified_line() {
    let (root, file) = git_blame_fixture("ui-git-blame-inline-typing");
    let mut session = open_blame_file(&root, &file);
    move_cursor_to_line(&mut session, 1);
    assert!(wait_for_annotation(&mut session, "Grace").contains("second"));

    run_ok(&mut session, "type local edit\n");
    assert!(dump(&mut session)["blame_inline"].is_null());
    run_ok(&mut session, "idle 250\n");
    assert!(dump(&mut session)["blame_inline"].is_null());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn headless_git_blame_inline_attributes_a_line_below_inserted_block() {
    let (root, file) = git_blame_fixture("ui-git-blame-inline-insert-block");
    let mut session = open_blame_file(&root, &file);
    session.app.editor.cursor = session.app.editor.line_offsets[1];
    run_ok(&mut session, "type inserted one\\ninserted two\\n");
    let line = session.app.editor.get_full_text().find("second updated").unwrap();
    session.app.editor.cursor = line;
    run_ok(&mut session, "mouse_move 0 0\n");

    let annotation = wait_for_annotation(&mut session, "Grace");
    assert!(annotation.contains("second"), "{annotation}");
    assert_eq!(dump(&mut session)["blame_inline"]["text"], annotation);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn headless_git_blame_inline_hunk_popup_suppresses_annotation() {
    let (root, file) = git_blame_fixture("ui-git-blame-inline-popup");
    std::fs::write(&file, "working first\nsecond updated\nthird\n").unwrap();
    let mut session = open_blame_file(&root, &file);
    wait_until(&mut session, 5000, "inline Git hunk", |session| !session.app.editor.git_hunks.is_empty());
    move_cursor_to_line(&mut session, 1);
    wait_for_annotation(&mut session, "Grace");

    session.app.show_inline_git_hunk_popup(0, 2);
    run_ok(&mut session, "mouse_move 0 0\n");
    assert!(dump(&mut session)["overlays"]["inline_git_popup"].as_bool().unwrap());
    assert!(dump(&mut session)["blame_inline"].is_null());

    run_ok(&mut session, "key escape\n");
    wait_for_annotation(&mut session, "Grace");
    assert!(!dump(&mut session)["overlays"]["inline_git_popup"].as_bool().unwrap());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn headless_git_blame_inline_skips_annotation_on_a_long_unwrapped_line() {
    let (root, file) = git_blame_fixture("ui-git-blame-inline-wrap");
    let wrapped = format!("{} tail\nsecond updated\nthird\n", "word ".repeat(300));
    std::fs::write(&file, wrapped).unwrap();
    git(&root, &["add", "blame.txt"]);
    git(&root, &["-c", "user.name=Wrap Author", "-c", "user.email=wrap@example.invalid", "commit", "-qm", "long wrapped line"]);
    let mut session = open_blame_file(&root, &file);
    move_cursor_to_line(&mut session, 0);
    run_ok(&mut session, "idle 250\n");
    assert!(dump(&mut session)["blame_inline"].is_null());
    let _ = std::fs::remove_dir_all(root);
}
