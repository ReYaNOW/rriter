//! Headless regressions for Git diff tabs and hunks.

use crate::headless::HeadlessSession;
use crate::headless::tests_support::{
    assert_ui_rect_inside_window, click_ui, dump, has_ui, run_script, scratch_dir, ui_center,
    workspace_with_explorer,
};
use serde_json::Value;
use std::path::{Path, PathBuf};

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;

fn run_git(dir: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("git fixture command");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_diff_fixture(name: &str) -> (PathBuf, String) {
    let dir = scratch_dir(name);
    let path = dir.join("hunks.txt");
    let mut original = String::new();
    let mut changed = String::new();
    for line in 1..=30 {
        original.push_str(&format!("line {line:02}\n"));
        let text = match line {
            3 => "changed first hunk".to_string(),
            15 => "changed second hunk".to_string(),
            27 => "changed third hunk".to_string(),
            _ => format!("line {line:02}"),
        };
        changed.push_str(&format!("{text}\n"));
    }
    std::fs::write(&path, &original).unwrap();
    run_git(&dir, &["init", "-q"]);
    run_git(&dir, &["config", "user.name", "Headless Test"]);
    run_git(&dir, &["config", "user.email", "headless@example.invalid"]);
    run_git(&dir, &["add", "."]);
    run_git(&dir, &["commit", "-qm", "fixture"]);
    std::fs::write(path, changed).unwrap();
    (dir, original)
}

fn writable_workspace(dir: &Path) -> HeadlessSession {
    let options = crate::headless::profile::HeadlessOptions {
        size: (TEST_WIDTH, TEST_HEIGHT),
        allow_writes: true,
        ..Default::default()
    };
    let root = crate::headless::tests_support::ensure_test_profile_root();
    let mut session = match HeadlessSession::new(&options, root) {
        Ok(session) => session,
        Err((code, message)) => panic!("headless session (code {code}): {message}"),
    };
    session.hz_probe = || None;
    let lines = run_script(
        &mut session,
        format!("scale {TEST_SCALE}\nworkspace {}\nsettle 2000\n", dir.display()).as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    session
}

fn double_click_ui(session: &mut HeadlessSession, id: &str) {
    let (x, y) = ui_center(&dump(session), id);
    let lines = run_script(
        session,
        format!("mouse_move {x} {y}\ndblclick\nwait 1500\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
}

fn open_git_diff(session: &mut HeadlessSession) -> Value {
    let state = dump(session);
    if state["ide_panel"]["active"] != "git" {
        click_ui(session, "SidebarSlot(Git)");
    }
    let lines = run_script(session, b"wait 8000\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let files = dump(session);
    assert!(has_ui(&files, "GitFileDiff(0, 0)"), "changed Git file missing: {files}");
    double_click_ui(session, "GitFileDiff(0, 0)");
    let opened = dump(session);
    assert_eq!(opened["tabs"][0]["title"], "Diff: hunks.txt", "{opened}");
    opened
}

#[test]
fn headless_git_diff_opens_from_changed_file_list() {
    let (dir, _) = git_diff_fixture("ui-git-diff-open");
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    let opened = open_git_diff(&mut session);
    assert!(has_ui(&opened, "GitDiffPrevHunk"), "previous-hunk control missing: {opened}");
    assert!(has_ui(&opened, "GitDiffNextHunk"), "next-hunk control missing: {opened}");
    assert_eq!(opened["tabs"].as_array().unwrap().len(), 1, "{opened}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_git_diff_navigation_moves_between_hunks() {
    let (dir, _) = git_diff_fixture("ui-git-diff-navigation");
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    let opened = open_git_diff(&mut session);
    let initial_scroll = opened["tabs"][0]["scroll_y"].as_f64().unwrap();

    click_ui(&mut session, "GitDiffNextHunk");
    run_script(&mut session, b"settle 1000\n");
    let second = dump(&mut session);
    let second_scroll = second["tabs"][0]["scroll_y"].as_f64().unwrap();
    assert!(second_scroll > initial_scroll, "next hunk did not advance: {second}");
    assert!(has_ui(&second, "GitDiffRollbackHunk(0, 1)"), "second hunk missing: {second}");

    click_ui(&mut session, "GitDiffNextHunk");
    run_script(&mut session, b"settle 1000\n");
    let third = dump(&mut session);
    let third_scroll = third["tabs"][0]["scroll_y"].as_f64().unwrap();
    assert!(third_scroll > second_scroll, "next hunk did not advance: {third}");
    assert!(has_ui(&third, "GitDiffRollbackHunk(0, 2)"), "third hunk missing: {third}");

    click_ui(&mut session, "GitDiffPrevHunk");
    run_script(&mut session, b"settle 1000\n");
    let previous = dump(&mut session);
    let previous_scroll = previous["tabs"][0]["scroll_y"].as_f64().unwrap();
    assert!(previous_scroll < third_scroll, "previous hunk did not move back: {previous}");
    assert!(has_ui(&previous, "GitDiffRollbackHunk(0, 1)"), "second hunk missing: {previous}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_git_diff_rollback_saves_only_selected_hunk() {
    let (dir, original) = git_diff_fixture("ui-git-diff-rollback");
    let path = dir.join("hunks.txt");
    let mut session = writable_workspace(&dir);
    assert_eq!(dump(&mut session)["writes_allowed"], true);
    let opened = open_git_diff(&mut session);
    assert!(has_ui(&opened, "GitDiffRollbackHunk(0, 0)"), "first hunk rollback missing: {opened}");

    click_ui(&mut session, "GitDiffRollbackHunk(0, 0)");
    let lines = run_script(&mut session, b"key ctrl+s\nwait 1000\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let expected = original
        .replace("line 15\n", "changed second hunk\n")
        .replace("line 27\n", "changed third hunk\n");
    assert_eq!(std::fs::read_to_string(path).unwrap(), expected);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_git_diff_tab_closes_from_tab_close_button() {
    let (dir, _) = git_diff_fixture("ui-git-diff-close");
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    let opened = open_git_diff(&mut session);
    assert!(has_ui(&opened, "EditorTabClose(0)"), "tab close control missing: {opened}");

    click_ui(&mut session, "EditorTabClose(0)");
    let closed = dump(&mut session);
    assert!(closed["tabs"].as_array().unwrap().is_empty(), "diff tab remained open: {closed}");
    assert!(!has_ui(&closed, "EditorTabClose(0)"), "tab close control remained: {closed}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_bug_git_diff_hunk_rollback_hitbox_stays_inside_window() {
    let (dir, _) = git_diff_fixture("ui-bug-git-diff-hitbox");
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    open_git_diff(&mut session);
    click_ui(&mut session, "GitDiffNextHunk");
    run_script(&mut session, b"settle 1000\n");
    let state = dump(&mut session);
    let mut visible = 0;
    for element in state["ui"].as_array().unwrap() {
        let id = element["id"].as_str().unwrap_or("");
        if id.starts_with("GitDiffRollbackHunk(") {
            assert_ui_rect_inside_window(&state, id);
            visible += 1;
        }
    }
    // The hunk navigated to must keep a clickable rollback button.
    assert!(visible > 0, "no rollback hitbox after hunk navigation: {state}");
    let _ = std::fs::remove_dir_all(dir);
}
