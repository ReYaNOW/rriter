//! Git commit, stage, options, and push flow regressions.

use crate::headless::HeadlessSession;
use crate::headless::tests_support::{
    click_ui, dump, git, git_fixture, has_ui, run_script, scratch_dir, wait_until,
    workspace_with_explorer,
};
use std::path::{Path, PathBuf};

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;

fn open_git_panel(dir: &Path) -> HeadlessSession {
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, dir);
    click_ui(&mut session, "SidebarSlot(Git)");
    wait_until(&mut session, 8000, "Git panel activation", |session| {
        dump(session)["ide_panel"]["active"] == "git"
    });
    session
}

fn wait_git(session: &mut HeadlessSession, dir: &Path, expected_cached: &str) {
    wait_until(session, 2000, "staged Git file list", |_| {
        git(dir, &["diff", "--cached", "--name-only"]) == expected_cached
    });
}

fn git_fixture_with_bare_remote(dir: &Path) -> (PathBuf, String) {
    git_fixture(dir);
    let name = dir.file_name().and_then(|name| name.to_str()).expect("scratch dir name");
    let bare = dir.with_file_name(format!("{name}-bare"));
    let _ = std::fs::remove_dir_all(&bare);
    let bare_path = bare.to_str().expect("bare repo path is utf-8");
    git(dir, &["init", "-q", "--bare", bare_path]);
    git(dir, &["remote", "add", "origin", bare_path]);
    let branch = git(dir, &["branch", "--show-current"]).trim().to_string();
    git(dir, &["push", "--quiet", "--set-upstream", "origin", &branch]);
    (bare, branch)
}

#[test]
fn headless_git_file_can_be_staged_and_unstaged() {
    let dir = scratch_dir("ui-git-stage-unstage");
    git_fixture(&dir);
    let mut session = open_git_panel(&dir);

    assert_eq!(git(&dir, &["diff", "--cached", "--name-only"]), "");
    click_ui(&mut session, "GitFile(0, 0)");
    wait_git(&mut session, &dir, "changed.txt\n");
    assert_eq!(git(&dir, &["diff", "--cached", "--name-only"]), "changed.txt\n");

    click_ui(&mut session, "GitFile(0, 0)");
    wait_git(&mut session, &dir, "");
    assert_eq!(git(&dir, &["diff", "--cached", "--name-only"]), "");
    assert!(git(&dir, &["status", "--porcelain"]).contains(" M changed.txt"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_git_commit_uses_message_and_commits_only_staged_file() {
    let dir = scratch_dir("ui-git-commit-message");
    git_fixture(&dir);
    let mut session = open_git_panel(&dir);
    click_ui(&mut session, "GitFile(0, 0)");
    wait_git(&mut session, &dir, "changed.txt\n");
    click_ui(&mut session, "GitMessageInput");
    let lines = run_script(&mut session, b"type headless: staged-file commit\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    click_ui(&mut session, "GitCommit");
    wait_until(&mut session, 8000, "Git commit", |session| {
        git(&dir, &["log", "-1", "--format=%s"]).trim() == "headless: staged-file commit"
    });

    assert_eq!(git(&dir, &["log", "-1", "--format=%s"]).trim(), "headless: staged-file commit");
    assert_eq!(git(&dir, &["show", "--pretty=format:", "--name-only", "HEAD"]).trim(), "changed.txt");
    assert!(git(&dir, &["status", "--porcelain"]).contains("?? untracked.txt"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_git_commit_menu_and_options_are_actionable() {
    let dir = scratch_dir("ui-git-commit-options");
    git_fixture(&dir);
    let before = git(&dir, &["rev-parse", "HEAD"]);
    let mut session = open_git_panel(&dir);
    click_ui(&mut session, "GitFile(0, 0)");
    wait_git(&mut session, &dir, "changed.txt\n");

    click_ui(&mut session, "GitCommitMenuToggle");
    let lines = run_script(&mut session, b"wait 400\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let menu = dump(&mut session);
    for index in 0..3 {
        assert!(has_ui(&menu, &format!("GitCommitMenuItem({index})")), "{menu}");
    }
    click_ui(&mut session, "GitCommitMenuItem(0)");
    // Negative check: give an async commit time to land before asserting HEAD is unchanged.
    let lines = run_script(&mut session, b"wait 2000\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert_eq!(git(&dir, &["rev-parse", "HEAD"]), before);

    click_ui(&mut session, "GitCommitOptionsToggle");
    let lines = run_script(&mut session, b"wait 400\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let options = dump(&mut session);
    assert!(has_ui(&options, "GitCommitOptionsItem(0)"), "{options}");
    click_ui(&mut session, "GitCommitOptionsItem(0)");
    assert!(session.app.ide_panel.git.commit_options.skip_hooks);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_git_rejects_empty_message_and_hides_commit_without_staged_files() {
    let dir = scratch_dir("ui-git-empty-commit");
    git_fixture(&dir);
    let before = git(&dir, &["rev-parse", "HEAD"]);
    let mut session = open_git_panel(&dir);
    let empty = dump(&mut session);
    assert!(!has_ui(&empty, "GitCommit"), "commit enabled without staged files: {empty}");
    assert!(!has_ui(&empty, "GitCommitMenuToggle"), "commit menu enabled without staged files: {empty}");

    click_ui(&mut session, "GitFile(0, 0)");
    wait_git(&mut session, &dir, "changed.txt\n");
    click_ui(&mut session, "GitCommit");
    // Negative check: give an async commit time to land before asserting HEAD is unchanged.
    let lines = run_script(&mut session, b"wait 2000\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert_eq!(git(&dir, &["rev-parse", "HEAD"]), before);
    assert_eq!(git(&dir, &["diff", "--cached", "--name-only"]), "changed.txt\n");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_git_push_updates_local_bare_remote() {
    let dir = scratch_dir("ui-git-push-bare");
    let (bare, branch) = git_fixture_with_bare_remote(&dir);
    let mut session = open_git_panel(&dir);
    click_ui(&mut session, "GitFile(0, 0)");
    wait_git(&mut session, &dir, "changed.txt\n");
    click_ui(&mut session, "GitMessageInput");
    let lines = run_script(&mut session, b"type headless: push flow\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    click_ui(&mut session, "GitCommit");
    wait_until(&mut session, 8000, "Git commit before push", |session| {
        has_ui(&dump(session), "GitPush(0)")
    });
    assert!(has_ui(&dump(&mut session), "GitPush(0)"));

    click_ui(&mut session, "GitPush(0)");
    wait_until(&mut session, 8000, "push to local bare remote", |_| {
        let local_head = git(&dir, &["rev-parse", "HEAD"]);
        git(&bare, &["rev-parse", &format!("refs/heads/{branch}")]) == local_head
    });
    let local_head = git(&dir, &["rev-parse", "HEAD"]);
    let remote_head = git(&bare, &["rev-parse", &format!("refs/heads/{branch}")]);
    assert_eq!(remote_head, local_head);

    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(bare);
}
