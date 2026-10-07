//! HEAD snapshot refresh regressions for open Git worktree tabs.

use crate::headless::tests_support::{
    click_ui, dump, git, git_init, has_ui, run_script, scratch_dir, wait_until,
    workspace_with_explorer,
};
use std::path::PathBuf;

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;

fn head_fixture(name: &str) -> (PathBuf, PathBuf) {
    let root = scratch_dir(name);
    let file = root.join("tracked.txt");
    std::fs::write(&file, "first\nsecond\n").unwrap_or_else(|error| panic!("write HEAD fixture: {error}"));
    git_init(&root);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "initial"]);
    (root, file)
}

fn ide_session(root: &std::path::Path, file: &std::path::Path) -> crate::headless::HeadlessSession {
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, root);
    let lines = run_script(
        &mut session,
        format!("open {}\nsettle 2000\n", file.display()).as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    session
}

fn head_oid(session: &crate::headless::HeadlessSession) -> Option<git2::Oid> {
    session
        .app
        .editor
        .git_head
        .as_ref()
        .and_then(|snapshot| snapshot.head_oid)
}

#[test]
fn headless_git_head_refreshes_after_focus_returns() {
    let (root, file) = head_fixture("ui-git-blame-head-focus");
    std::fs::write(&file, "working change\nsecond\n").unwrap();
    let mut session = ide_session(&root, &file);
    let before = git(&root, &["rev-parse", "HEAD"]);
    assert_eq!(head_oid(&session).unwrap().to_string(), before.trim());
    assert!(!session.app.editor.git_hunks.is_empty());

    std::fs::write(&file, "committed change\nsecond\n").unwrap();
    git(&root, &["add", "tracked.txt"]);
    git(&root, &["commit", "-qm", "external commit"]);
    let after = git(&root, &["rev-parse", "HEAD"]);
    session.app.on_window_focus_gained();
    assert!(head_oid(&session).is_some_and(|oid| oid.to_string() == after.trim()));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn headless_git_panel_commit_refreshes_head_snapshot_and_hunks() {
    let (root, file) = head_fixture("ui-git-blame-head-panel-commit");
    std::fs::write(&file, "panel commit\nsecond\n").unwrap();
    let mut session = ide_session(&root, &file);
    let before = git(&root, &["rev-parse", "HEAD"]);
    assert_eq!(head_oid(&session).unwrap().to_string(), before.trim());

    click_ui(&mut session, "SidebarSlot(Git)");
    wait_until(&mut session, 8000, "changed Git file", |session| {
        has_ui(&dump(session), "GitFile(0, 0)")
    });
    click_ui(&mut session, "GitFile(0, 0)");
    wait_until(&mut session, 2000, "staged file", |_| {
        git(&root, &["diff", "--cached", "--name-only"]) == "tracked.txt\n"
    });
    click_ui(&mut session, "GitMessageInput");
    run_script(&mut session, b"type snapshot refresh commit\n");
    click_ui(&mut session, "GitCommit");
    wait_until(&mut session, 8000, "Git panel commit", |_| {
        git(&root, &["rev-parse", "HEAD"]) != before
    });
    let after = git(&root, &["rev-parse", "HEAD"]);
    wait_until(&mut session, 3000, "snapshot after Git panel commit", |session| {
        head_oid(session).is_some_and(|oid| oid.to_string() == after.trim())
            && session.app.editor.git_hunks.is_empty()
    });
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn headless_git_head_refreshes_from_git_metadata_watcher() {
    let (root, file) = head_fixture("ui-git-blame-head-watcher");
    let mut session = ide_session(&root, &file);
    let before = head_oid(&session).unwrap();

    std::fs::write(&file, "watcher commit\nsecond\n").unwrap();
    git(&root, &["add", "tracked.txt"]);
    git(&root, &["commit", "-qm", "watcher commit"]);
    let after = git(&root, &["rev-parse", "HEAD"]);
    assert_ne!(before.to_string(), after.trim());
    wait_until(&mut session, 5000, "HEAD snapshot from .git watcher", |session| {
        head_oid(session).is_some_and(|oid| oid.to_string() == after.trim())
    });
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn headless_external_file_reload_restores_head_snapshot() {
    let (root, file) = head_fixture("ui-git-blame-head-file-reload");
    let mut session = ide_session(&root, &file);
    assert!(head_oid(&session).is_some());
    std::fs::write(&file, "external disk change\nsecond\n").unwrap();
    session.app.check_external_changes();
    assert_eq!(session.app.editor.get_full_text(), "external disk change\nsecond\n");
    assert!(head_oid(&session).is_some());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn headless_file_outside_repository_has_no_git_head() {
    let workspace = scratch_dir("ui-git-blame-head-outside-workspace");
    let outside = scratch_dir("ui-git-blame-head-outside-repo");
    let file = outside.join("outside.txt");
    std::fs::write(&file, "outside repository\n").unwrap();
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &workspace);
    let lines = run_script(
        &mut session,
        format!("open {}\n", file.display()).as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert!(session.app.editor.git_head.is_none());
    let _ = std::fs::remove_dir_all(workspace);
    let _ = std::fs::remove_dir_all(outside);
}
