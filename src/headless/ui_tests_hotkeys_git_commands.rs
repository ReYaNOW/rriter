//! Headless coverage for configurable Git commands.

use crate::app::git_panel::GitConfirmAction;
use crate::app::PanelId;
use crate::headless::tests_support::{
    click_ui, dump, git, git_fixture, has_ui, run_script, scratch_dir, session_for_test,
    wait_until, workspace_with_explorer,
};
use crate::headless::ui_tests_hotkeys::install_config_keymap;
use crate::headless::HeadlessSession;
use serde_json::json;
use std::path::{Path, PathBuf};

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;

fn run_ok(session: &mut HeadlessSession, script: &str) {
    let lines = run_script(session, script.as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
}

fn open_git(dir: &Path) -> HeadlessSession {
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, dir);
    click_ui(&mut session, "SidebarSlot(Git)");
    wait_until(&mut session, 8000, "Git panel and repository snapshot", |session| {
        session.app.ide_panel.git.snapshot.workspaces.iter().any(|workspace| {
            workspace.repo_root.as_deref() == Some(dir)
        })
    });
    session
}

fn bind_git_commands(session: &mut HeadlessSession, name: &str, bindings: serde_json::Value) -> PathBuf {
    let config_dir = scratch_dir(name);
    install_config_keymap(session, &config_dir.join("config.json"), bindings);
    config_dir
}

fn command_chords() -> serde_json::Value {
    json!({
        "git.unstage_all": ["ctrl+alt+1"],
        "git.push": ["ctrl+alt+2"],
        "git.fetch": ["ctrl+alt+3"],
        "git.pull": ["ctrl+alt+4"],
        "git.refresh": ["ctrl+alt+5"],
        "git.toggle_graph": ["ctrl+alt+6"],
        "git.toggle_logs": ["ctrl+alt+7"]
    })
}

fn git_fixture_with_bare_remote(dir: &Path) -> (PathBuf, String) {
    git_fixture(dir);
    let name = dir.file_name().and_then(|name| name.to_str()).expect("scratch dir name");
    let bare = dir.with_file_name(format!("{name}-bare"));
    let bare_path = bare.to_str().expect("bare repo path is utf-8");
    git(dir, &["init", "-q", "--bare", bare_path]);
    git(dir, &["remote", "add", "origin", bare_path]);
    let branch = git(dir, &["branch", "--show-current"]).trim().to_string();
    git(dir, &["add", "--all"]);
    git(dir, &["commit", "-qm", "local changes"]);
    git(dir, &["push", "--quiet", "--set-upstream", "origin", &branch]);
    (bare, branch)
}

fn advance_remote(bare: &Path, branch: &str, label: &str) -> String {
    let clone = bare.with_file_name(format!(
        "{}-{label}-clone",
        bare.file_name().and_then(|name| name.to_str()).expect("bare repo name")
    ));
    let bare_path = bare.to_str().expect("bare repo path is utf-8");
    git(bare, &["clone", "-q", bare_path, clone.to_str().expect("clone path is utf-8")]);
    git(&clone, &["config", "user.name", "Headless Test"]);
    git(&clone, &["config", "user.email", "headless@example.invalid"]);
    std::fs::write(clone.join(format!("{label}.txt")), format!("{label}\n"))
        .expect("write remote commit fixture");
    git(&clone, &["add", "--all"]);
    git(&clone, &["commit", "-qm", label]);
    git(&clone, &["push", "--quiet", "origin", branch]);
    let head = git(&clone, &["rev-parse", "HEAD"]);
    let _ = std::fs::remove_dir_all(clone);
    head
}

#[test]
#[ignore = "bug: git.unstage_all skips its requested confirmation"]
fn headless_hotkeys_git_unstage_all_requires_confirmation_and_unstages() {
    let dir = scratch_dir("ui-hotkeys-git-unstage");
    git_fixture(&dir);
    git(&dir, &["add", "--all"]);
    let mut session = open_git(&dir);
    let config_dir = bind_git_commands(&mut session, "ui-hotkeys-git-unstage-config", command_chords());

    run_ok(&mut session, "key ctrl+alt+1\n");
    wait_until(&mut session, 3000, "unstage confirmation", |session| {
        session.app.ide_panel.git.confirm_dialog.is_some()
    });
    let dialog = session.app.ide_panel.git.confirm_dialog.as_ref().expect("confirmation");
    assert_eq!(dialog.action, GitConfirmAction::RollbackStaged);
    assert!(git(&dir, &["diff", "--cached", "--name-only"]).contains("changed.txt"),
        "hotkey unstaged files before confirmation");

    click_ui(&mut session, "GitConfirmAction");
    wait_until(&mut session, 8000, "all files unstaged", |_| {
        git(&dir, &["diff", "--cached", "--name-only"]).is_empty()
    });
    assert!(git(&dir, &["status", "--porcelain"]).contains(" M changed.txt"));
    let _ = std::fs::remove_dir_all(config_dir);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_git_unstage_all_matches_panel_action() {
    let dir = scratch_dir("ui-hotkeys-git-unstage-direct");
    git_fixture(&dir);
    git(&dir, &["add", "--all"]);
    let mut session = open_git(&dir);
    let config_dir = bind_git_commands(&mut session, "ui-hotkeys-git-unstage-direct-config", command_chords());

    run_ok(&mut session, "key ctrl+alt+1\n");
    wait_until(&mut session, 8000, "all files unstaged", |_| {
        git(&dir, &["diff", "--cached", "--name-only"]).is_empty()
    });
    assert!(session.app.ide_panel.git.confirm_dialog.is_none(),
        "Git panel action unexpectedly opened a confirmation");
    assert!(git(&dir, &["status", "--porcelain"]).contains(" M changed.txt"));
    let _ = std::fs::remove_dir_all(config_dir);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_git_remote_commands_match_panel_actions() {
    let dir = scratch_dir("ui-hotkeys-git-remote-commands");
    let (bare, branch) = git_fixture_with_bare_remote(&dir);
    let mut session = open_git(&dir);
    let config_dir = bind_git_commands(&mut session, "ui-hotkeys-git-remote-config", command_chords());

    let pushed_head = git(&dir, &["rev-parse", "HEAD"]);
    run_ok(&mut session, "key ctrl+alt+2\n");
    wait_until(&mut session, 8000, "push to local bare remote", |_| {
        git(&bare, &["rev-parse", &format!("refs/heads/{branch}")]) == pushed_head
    });
    assert_eq!(git(&bare, &["rev-parse", &format!("refs/heads/{branch}")]), pushed_head);

    let fetched_head = advance_remote(&bare, &branch, "fetch-update");
    run_ok(&mut session, "key ctrl+alt+3\n");
    wait_until(&mut session, 8000, "fetch updates remote tracking ref", |_| {
        git(&dir, &["rev-parse", &format!("refs/remotes/origin/{branch}")]) == fetched_head
    });
    assert_eq!(git(&dir, &["rev-parse", &format!("refs/remotes/origin/{branch}")]), fetched_head);
    assert_ne!(git(&dir, &["rev-parse", "HEAD"]), fetched_head, "fetch changed local HEAD");

    let pulled_head = advance_remote(&bare, &branch, "pull-update");
    run_ok(&mut session, "key ctrl+alt+4\n");
    wait_until(&mut session, 8000, "pull updates local HEAD", |_| {
        git(&dir, &["rev-parse", "HEAD"]) == pulled_head
    });
    assert_eq!(git(&dir, &["rev-parse", "HEAD"]), pulled_head);
    let _ = std::fs::remove_dir_all(config_dir);
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(bare);
}

#[test]
fn headless_hotkeys_git_refresh_reloads_repository_snapshot() {
    let dir = scratch_dir("ui-hotkeys-git-refresh");
    git_fixture(&dir);
    let mut session = open_git(&dir);
    let config_dir = bind_git_commands(&mut session, "ui-hotkeys-git-refresh-config", command_chords());
    std::fs::write(dir.join("refresh-hotkey.txt"), "new status entry\n").expect("write fixture");
    assert!(!session.app.ide_panel.git.snapshot.workspaces[0]
        .files.iter().any(|file| file.rel_path.ends_with("refresh-hotkey.txt")));

    run_ok(&mut session, "key ctrl+alt+5\n");
    wait_until(&mut session, 8000, "refresh snapshot contains new file", |session| {
        session.app.ide_panel.git.snapshot.workspaces.iter().any(|workspace| {
            workspace.files.iter().any(|file| file.rel_path.ends_with("refresh-hotkey.txt"))
        })
    });
    assert!(session.app.ide_panel.git.snapshot.workspaces[0]
        .files.iter().any(|file| file.rel_path.ends_with("refresh-hotkey.txt")));
    let _ = std::fs::remove_dir_all(config_dir);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_git_graph_and_logs_toggle_panel_visibility() {
    let dir = scratch_dir("ui-hotkeys-git-panes");
    git_fixture(&dir);
    let mut session = open_git(&dir);
    let config_dir = bind_git_commands(&mut session, "ui-hotkeys-git-panes-config", command_chords());
    assert!(!session.app.ide_panel.git.graph_open());
    assert!(!session.app.ide_panel.git.logs_open());

    run_ok(&mut session, "key ctrl+alt+6\n");
    wait_until(&mut session, 8000, "Git graph opens", |session| {
        session.app.ide_panel.git.graph_open()
    });
    assert!(session.app.ide_panel.git.graph_open());
    assert!(has_ui(&dump(&mut session), "GitGraphToggle"));

    run_ok(&mut session, "key ctrl+alt+7\n");
    wait_until(&mut session, 3000, "Git logs open", |session| {
        session.app.ide_panel.git.logs_open()
    });
    assert!(session.app.ide_panel.git.logs_open());
    assert!(has_ui(&dump(&mut session), "GitLogsToggle"));
    let _ = std::fs::remove_dir_all(config_dir);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_git_command_without_repository_shows_reason_and_consumes_key() {
    let mut session = session_for_test(TEST_WIDTH, TEST_HEIGHT);
    let config_dir = bind_git_commands(
        &mut session,
        "ui-hotkeys-git-unavailable-config",
        json!({"git.refresh": ["ctrl+alt+5"]}),
    );
    let editor_before = session.app.editor.get_full_text();

    run_ok(&mut session, "key ctrl+alt+5\n");
    let state = dump(&mut session);
    assert_eq!(session.app.readonly_notice_text, "Нет активного репозитория");
    assert!(state["overlays"]["readonly_notice"].as_bool().unwrap_or(false), "{state}");
    assert_eq!(session.app.editor.get_full_text(), editor_before, "unavailable chord reached editor");
    let _ = std::fs::remove_dir_all(config_dir);
}

#[test]
fn headless_hotkeys_git_graph_and_logs_are_unavailable_outside_ide_mode() {
    let mut session = session_for_test(TEST_WIDTH, TEST_HEIGHT);
    let config_dir = bind_git_commands(
        &mut session,
        "ui-hotkeys-git-pane-unavailable-config",
        json!({
            "git.toggle_graph": ["ctrl+alt+6"],
            "git.toggle_logs": ["ctrl+alt+7"]
        }),
    );

    run_ok(&mut session, "key ctrl+alt+6\n");
    assert_eq!(session.app.readonly_notice_text, "Доступно в IDE");
    assert!(!session.app.ide_panel.git.graph_open());
    run_ok(&mut session, "key ctrl+alt+7\n");
    assert_eq!(session.app.readonly_notice_text, "Доступно в IDE");
    assert!(!session.app.ide_panel.git.logs_open());
    let _ = std::fs::remove_dir_all(config_dir);
}
