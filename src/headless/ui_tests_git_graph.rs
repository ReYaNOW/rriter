//! Git graph rendering, commit details, and long-history scrolling regressions.

use crate::headless::HeadlessSession;
use crate::headless::tests_support::{
    assert_ui_rect_inside_window, click_ui, dump, git, git_fixture, has_ui, run_script,
    scratch_dir, ui_center, workspace_with_explorer,
};
use serde_json::Value;
use std::path::PathBuf;

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;
const GIT_WAIT_MS: u32 = 8000;

fn graph_fixture(name: &str) -> PathBuf {
    let dir = scratch_dir(name);
    git_fixture(&dir);
    git(&dir, &["add", "--all"]);
    git(&dir, &["commit", "-qm", "graph-root"]);
    git(&dir, &["branch", "-M", "main"]);

    for i in 1..=10 {
        let message = format!("main-{i:02}");
        git(&dir, &["commit", "--allow-empty", "-qm", &message]);
    }
    git(&dir, &["switch", "-c", "feature"]);
    for i in 1..=3 {
        let message = format!("feature-{i}");
        git(&dir, &["commit", "--allow-empty", "-qm", &message]);
    }
    git(&dir, &["switch", "main"]);
    for i in 11..=30 {
        let message = format!("main-{i:02}");
        git(&dir, &["commit", "--allow-empty", "-qm", &message]);
    }
    git(&dir, &["merge", "--no-ff", "-qm", "graph-merge", "feature"]);
    dir
}

fn open_graph(name: &str) -> (PathBuf, HeadlessSession) {
    let dir = graph_fixture(name);
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    click_ui(&mut session, "SidebarSlot(Git)");
    let lines = run_script(&mut session, format!("wait {GIT_WAIT_MS}\n").as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    click_ui(&mut session, "GitGraphToggle");
    let lines = run_script(&mut session, format!("wait {GIT_WAIT_MS}\n").as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    (dir, session)
}

fn visible_graph_commit_indices(state: &Value) -> Vec<usize> {
    state["ui"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|element| {
            element["id"]
                .as_str()?
                .strip_prefix("GitGraphCommit(0, ")?
                .strip_suffix(')')?
                .parse()
                .ok()
        })
        .collect()
}

#[test]
fn headless_git_graph_renders_merge_history() {
    let (dir, mut session) = open_graph("ui-git-graph-render");
    let state = dump(&mut session);

    assert!(has_ui(&state, "GitGraphWorkspace(0)"), "{state}");
    assert!(has_ui(&state, "GitGraphResize"), "{state}");
    assert!(has_ui(&state, "GitGraphScroll"), "{state}");
    let commits = visible_graph_commit_indices(&state);
    assert!(commits.len() >= 3, "visible commits: {commits:?}; {state}");
    assert_eq!(commits[0], 0, "{state}");
    for index in commits {
        assert_ui_rect_inside_window(&state, &format!("GitGraphCommit(0, {index})"));
    }

    drop(session);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_git_graph_commit_click_shows_details_and_copies_hash() {
    let (dir, mut session) = open_graph("ui-git-graph-commit-details");
    let row = ui_center(&dump(&mut session), "GitGraphCommit(0, 0)");
    let lines = run_script(
        &mut session,
        format!("mouse_move {} {}\nwait 1200\nclick\n", row.0, row.1).as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");

    let details = dump(&mut session);
    assert!(has_ui(&details, "GitGraphCopyCommit(0, 0)"), "{details}");
    assert!(has_ui(&details, "GitGraphOpenCommit(0, 0)"), "{details}");
    click_ui(&mut session, "GitGraphCopyCommit(0, 0)");
    let copied = dump(&mut session);
    let expected = git(&dir, &["rev-parse", "HEAD"]).trim().to_string();
    assert_eq!(copied["clipboard"]["text"].as_str(), Some(expected.as_str()));

    drop(session);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_git_graph_scrolls_long_history() {
    let (dir, mut session) = open_graph("ui-git-graph-scroll");
    let before = visible_graph_commit_indices(&dump(&mut session));
    assert!(before.len() >= 3, "visible commits before scroll: {before:?}");

    let lines = run_script(&mut session, b"mouse_move 200 600\nwheel 0 -8\nwait 800\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let after_state = dump(&mut session);
    let after = visible_graph_commit_indices(&after_state);
    assert!(!after.is_empty(), "no visible commits after scroll: {after_state}");
    assert!(after[0] > before[0], "before {before:?}, after {after:?}");
    assert!(after.iter().any(|index| *index >= 20), "after {after:?}");

    drop(session);
    let _ = std::fs::remove_dir_all(dir);
}
