//! Git blame inline: HEAD changes, external reloads and on-disk edits.

use super::ui_tests_git_blame_inline::{open_and_wait, TEST_HEIGHT, TEST_SCALE};
use crate::headless::tests_support::{
    click_ui, dump, git, git_blame_fixture, run_script, wait_until, workspace_with_explorer,
};

const SYNC_WIDTH: u32 = 1920;

fn open_blame_file(root: &std::path::Path, file: &std::path::Path) -> crate::headless::HeadlessSession {
    let mut session = workspace_with_explorer(SYNC_WIDTH, TEST_HEIGHT, TEST_SCALE, root);
    session.app.git_blame_inline = true;
    session.app.git_blame_delay_ms = 100;
    open_and_wait(&mut session, file, 3);
    session
}

fn wait_for_annotation(session: &mut crate::headless::HeadlessSession, author: &str) -> String {
    wait_until(session, 8000, "inline Git blame annotation", |session| {
        dump(session)["blame_inline"]["text"]
            .as_str()
            .is_some_and(|text| text.contains(author))
    });
    dump(session)["blame_inline"]["text"].as_str().unwrap().to_owned()
}

fn wait_for_disk_text(session: &mut crate::headless::HeadlessSession, file: &std::path::Path, text: &str) {
    wait_until(session, 5000, "saved editor contents", |_| {
        std::fs::read_to_string(file).is_ok_and(|contents| contents == text)
    });
}

#[test]
fn headless_git_blame_inline_tracks_external_head_after_saved_edit_and_focus() {
    let (root, file) = git_blame_fixture("ui-git-blame-inline-head-change");
    let mut session = open_blame_file(&root, &file);
    session.app.editor.cursor = session.app.editor.line_offsets[0];
    let lines = run_script(&mut session, b"type app edit\nkey ctrl+s\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_for_disk_text(&mut session, &file, "app editfirst updated\nsecond updated\nthird\n");
    let lines = run_script(&mut session, b"wait 150\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert!(dump(&mut session)["blame_inline"].is_null());

    git(&root, &["add", "blame.txt"]);
    git(&root, &[
        "-c", "user.name=External Head Author", "-c", "user.email=external@example.invalid",
        "commit", "-qm", "external line update",
    ]);
    session.app.on_window_focus_gained();
    let annotation = wait_for_annotation(&mut session, "External Head Author");
    assert!(!annotation.contains("Linus"), "stale annotation survived focus refresh: {annotation}");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn headless_git_blame_inline_tracks_git_panel_commit() {
    let (root, file) = git_blame_fixture("ui-git-blame-inline-panel-commit");
    let mut session = open_blame_file(&root, &file);
    session.app.editor.cursor = session.app.editor.line_offsets[0];
    let lines = run_script(&mut session, b"type panel edit\nkey ctrl+s\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_for_disk_text(&mut session, &file, "panel editfirst updated\nsecond updated\nthird\n");

    click_ui(&mut session, "SidebarSlot(Git)");
    wait_until(&mut session, 8000, "changed Git file", |session| {
        crate::headless::tests_support::has_ui(&dump(session), "GitFile(0, 0)")
    });
    click_ui(&mut session, "GitFile(0, 0)");
    wait_until(&mut session, 2000, "staged blame file", |_| {
        git(&root, &["diff", "--cached", "--name-only"]) == "blame.txt\n"
    });
    click_ui(&mut session, "GitMessageInput");
    let lines = run_script(&mut session, b"type inline blame panel commit\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    click_ui(&mut session, "GitCommit");
    wait_until(&mut session, 8000, "Git panel blame commit", |_| {
        git(&root, &["log", "-1", "--format=%s"]).trim() == "inline blame panel commit"
    });
    wait_until(&mut session, 8000, "panel commit blame annotation", |session| {
        dump(session)["blame_inline"]["text"]
            .as_str()
            .is_some_and(|text| text.contains("Headless Test"))
    });
    assert!(dump(&mut session)["blame_inline"]["text"].as_str().unwrap().contains("Headless Test"));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn headless_git_blame_inline_returns_after_external_file_reload() {
    let (root, file) = git_blame_fixture("ui-git-blame-inline-reload");
    let mut session = open_blame_file(&root, &file);
    session.app.editor.cursor = session.app.editor.line_offsets[0];
    let before = wait_for_annotation(&mut session, "Linus");
    assert!(before.contains("Linus"));

    std::fs::write(&file, "first updated\nexternal working tree edit\nthird\n").unwrap();
    wait_until(&mut session, 8000, "external file reload", |session| {
        session.app.editor.text_equals("first updated\nexternal working tree edit\nthird\n")
    });
    let after = wait_for_annotation(&mut session, "Linus");
    assert!(after.contains("Linus"));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn headless_git_blame_inline_skips_working_tree_edits_before_open() {
    let (root, file) = git_blame_fixture("ui-git-blame-inline-disk-edit");
    std::fs::write(&file, "uncommitted edit\nsecond updated\nthird\n").unwrap();
    let mut session = open_blame_file(&root, &file);
    let lines = run_script(&mut session, b"wait 150\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert!(dump(&mut session)["blame_inline"].is_null());

    session.app.editor.cursor = session.app.editor.line_offsets[1];
    let annotation = wait_for_annotation(&mut session, "Grace");
    assert!(annotation.contains("Grace"));

    let _ = std::fs::remove_dir_all(root);
}
