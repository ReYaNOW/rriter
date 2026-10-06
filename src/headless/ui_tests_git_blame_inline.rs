use crate::headless::tests_support::{
    git, git_blame_fixture, git_init, run_script, scratch_dir, wait_until,
    workspace_with_explorer,
};
use std::path::Path;

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;

fn open_and_wait(session: &mut crate::headless::HeadlessSession, file: &Path, expected_lines: usize) {
    let lines = run_script(session, format!("open {}\n", file.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(session, 8000, "Git blame result", |session| {
        session.app.editor.git_blame.blame.is_some() || session.app.editor.git_blame.failed_key.is_some()
    });
    let blame = session.app.editor.git_blame.blame.as_ref().unwrap_or_else(|| {
        panic!("Git blame failed for {}; key={:?}", file.display(), session.app.editor.git_blame.key)
    });
    assert_eq!(blame.line_commit.len(), expected_lines);
}

#[test]
fn headless_git_blame_loads_committed_files_with_git_line_counts() {
    let (root, file) = git_blame_fixture("ui-git-blame-inline");
    let crlf = root.join("crlf.txt");
    let no_final_newline = root.join("no-final-newline.txt");
    let empty = root.join("empty.txt");
    let bom = root.join("bom.txt");
    std::fs::write(&crlf, "one\r\ntwo\r\n").unwrap();
    std::fs::write(&no_final_newline, "single line").unwrap();
    std::fs::write(&empty, "").unwrap();
    std::fs::write(&bom, "\u{feff}bom\nsecond\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "line endings"]);
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &root);
    session.app.git_blame_inline = true;

    open_and_wait(&mut session, &file, 3);
    open_and_wait(&mut session, &crlf, 2);
    open_and_wait(&mut session, &no_final_newline, 1);
    open_and_wait(&mut session, &empty, 0);
    open_and_wait(&mut session, &bom, 2);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn headless_git_blame_skips_untracked_unborn_and_outside_files() {
    let (root, _) = git_blame_fixture("ui-git-blame-unavailable");
    let untracked = root.join("untracked.txt");
    std::fs::write(&untracked, "not committed\n").unwrap();
    let outside_root = scratch_dir("ui-git-blame-outside");
    let outside = outside_root.join("outside.txt");
    std::fs::write(&outside, "outside repo\n").unwrap();
    let unborn_root = scratch_dir("ui-git-blame-unborn");
    git_init(&unborn_root);
    let unborn = unborn_root.join("unborn.txt");
    std::fs::write(&unborn, "no commits\n").unwrap();
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &root);
    session.app.git_blame_inline = true;
    let notice_before = session.app.readonly_notice_text.clone();
    let notice_until_before = session.app.readonly_notice_until;

    for (file, workspace) in [
        (untracked.clone(), root.clone()),
        (outside.clone(), root.clone()),
        (unborn.clone(), unborn_root.clone()),
    ] {
        let lines = run_script(
            &mut session,
            format!("workspace {}\nopen {}\n", workspace.display(), file.display()).as_bytes(),
        );
        assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
        assert!(!session.app.editor.git_blame.pending);
        assert!(session.app.editor.git_blame.blame.is_none());
        assert!(session.app.git_blame_rx.is_empty());
        assert_eq!(session.app.readonly_notice_text, notice_before);
        assert_eq!(session.app.readonly_notice_until, notice_until_before);
    }

    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(outside_root);
    let _ = std::fs::remove_dir_all(unborn_root);
}

#[test]
fn headless_git_blame_keeps_tabs_separate_and_loads_full_messages() {
    let (root, _) = git_blame_fixture("ui-git-blame-tabs");
    let large = root.join("large.txt");
    let second = root.join("second.txt");
    let large_text = "line\n".repeat(20_000);
    std::fs::write(&large, large_text).unwrap();
    std::fs::write(&second, "second file\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-m", "Multi-line subject\n\nfull body"]);
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &root);
    session.app.git_blame_inline = true;

    let lines = run_script(&mut session, format!("open {}\n", large.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let first_tab = session.app.active_tab;
    let lines = run_script(&mut session, format!("open {}\n", second.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let second_tab = session.app.active_tab;
    assert_ne!(first_tab, second_tab);
    session.app.switch_to_tab(first_tab);
    session.app.switch_to_tab(second_tab);
    wait_until(&mut session, 12_000, "both tab blame results", |session| {
        session.app.tabs[first_tab].editor.git_blame.blame.is_some()
            && session.app.editor.git_blame.blame.is_some()
    });
    assert_eq!(session.app.tabs[first_tab].editor.git_blame.blame.as_ref().unwrap().line_commit.len(), 20_000);
    assert_eq!(session.app.editor.git_blame.blame.as_ref().unwrap().line_commit.len(), 1);

    let oid = session.app.editor.git_blame.blame.as_ref().unwrap().commits[0].oid;
    session.app.request_commit_message(oid);
    session.app.request_commit_message(oid);
    assert_eq!(session.app.git_blame_message_rx.len(), 1, "duplicate requests should share the in-flight lookup");
    wait_until(&mut session, 5000, "full blame commit message", |session| {
        session.app.editor.git_blame.messages.iter().any(|(cached, _)| *cached == oid)
    });
    let message = session.app.editor.git_blame.messages.iter().find(|(cached, _)| *cached == oid).unwrap();
    assert_eq!(message.1, "Multi-line subject\n\nfull body\n");

    let missing_oid = git2::Oid::from_str("0000000000000000000000000000000000000000").unwrap();
    session.app.request_commit_message(missing_oid);
    wait_until(&mut session, 5000, "failed blame message lookup", |session| {
        session.app.git_blame_message_rx.iter().any(|receiver| receiver.failed)
    });
    session.app.request_commit_message(missing_oid);
    assert_eq!(session.app.git_blame_message_rx.len(), 1, "failed lookups should not be retried for the same oid");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn headless_git_blame_failed_key_is_attempted_once() {
    let (root, file) = git_blame_fixture("ui-git-blame-failure");
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &root);
    let lines = run_script(&mut session, format!("open {}\n", file.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(&mut session, 5000, "HEAD snapshot", |session| {
        session.app.editor.git_head.is_some()
    });
    let head_oid = session.app.editor.git_head.as_ref().unwrap().head_oid.unwrap();
    let object_path = root.join(".git/objects").join(&head_oid.to_string()[..2]).join(&head_oid.to_string()[2..]);
    std::fs::remove_file(object_path).unwrap();
    session.app.git_blame_inline = true;
    session.app.ensure_blame_for_active();
    assert_eq!(session.app.git_blame_rx.len(), 1);
    wait_until(&mut session, 5000, "forced blame failure", |session| {
        session.app.editor.git_blame.failed_key.is_some()
    });
    assert!(session.app.editor.git_blame.blame.is_none());
    session.app.ensure_blame_for_active();
    assert!(session.app.git_blame_rx.is_empty(), "failed key must not spawn a retry");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn headless_git_blame_inline_dwell_formats_once_and_tracks_the_active_file() {
    let (root, file) = git_blame_fixture("ui-git-blame-inline-dwell");
    let second = root.join("second.txt");
    std::fs::write(&second, "other file\n").unwrap();
    git(&root, &["add", "second.txt"]);
    git(&root, &["-c", "user.name=Tab Author", "-c", "user.email=tab@example.invalid", "commit", "-qm", "other file"]);
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &root);
    session.app.git_blame_inline = true;
    session.app.git_blame_delay_ms = 400;
    open_and_wait(&mut session, &file, 3);

    session.app.editor.git_blame.inline_key = None;
    session.app.editor.git_blame.inline_since = None;
    let started = std::time::Instant::now();
    assert!(session.app.tick_git_blame_inline(started));
    assert!(session.app.editor.git_blame.inline_text.is_empty());
    assert!(!session.app.tick_git_blame_inline(started + std::time::Duration::from_millis(399)));
    assert!(session.app.editor.git_blame.inline_text.is_empty());
    assert!(session.app.tick_git_blame_inline(started + std::time::Duration::from_millis(400)));
    assert!(session.app.editor.git_blame.inline_text.contains("Linus"));
    assert!(session.app.editor.git_blame.inline_text.contains("third"));

    let frame = run_script(&mut session, b"mouse_move 0 0\ndump\n");
    let dump: serde_json::Value = serde_json::from_str(frame.last().unwrap().strip_prefix("ok ").unwrap()).unwrap();
    assert_eq!(dump["blame_inline"]["text"], session.app.editor.git_blame.inline_text);
    assert!(dump["blame_inline"]["w"].as_f64().unwrap() > 0.0);
    let idle = run_script(&mut session, b"idle 50\n");
    assert!(idle.last().unwrap().contains("frames=0") && idle.last().unwrap().contains("wakes=1"), "{idle:?}");

    session.app.editor.cursor = session.app.editor.line_offsets[1];
    session.app.editor.git_blame.inline_key = None;
    session.app.editor.git_blame.inline_since = None;
    let moved = std::time::Instant::now();
    assert!(session.app.tick_git_blame_inline(moved));
    assert!(session.app.editor.git_blame.inline_text.is_empty());
    assert!(session.app.tick_git_blame_inline(moved + std::time::Duration::from_millis(400)));
    assert!(session.app.editor.git_blame.inline_text.contains("Grace"));
    assert!(session.app.editor.git_blame.inline_text.contains("second"));

    let lines = run_script(&mut session, format!("open {}\n", second.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(&mut session, 8000, "second file blame result", |session| {
        session.app.editor.git_blame.blame.is_some()
    });
    session.app.editor.git_blame.inline_key = None;
    session.app.editor.git_blame.inline_since = None;
    let switched = std::time::Instant::now();
    assert!(session.app.tick_git_blame_inline(switched));
    assert!(session.app.editor.git_blame.inline_text.is_empty());
    assert!(session.app.tick_git_blame_inline(switched + std::time::Duration::from_millis(400)));
    assert!(session.app.editor.git_blame.inline_text.contains("Tab Author"));
    assert!(session.app.editor.git_blame.inline_text.contains("other file"));
    assert_eq!(session.app.git_blame_inline_wake_at(), None);

    let long = root.join("long.txt");
    std::fs::write(&long, "x".repeat(4000)).unwrap();
    git(&root, &["add", "long.txt"]);
    git(&root, &["-c", "user.name=Long Author", "-c", "user.email=long@example.invalid", "commit", "-qm", "long line"]);
    let lines = run_script(&mut session, format!("open {}\n", long.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(&mut session, 8000, "long file blame result", |session| {
        session.app.editor.git_blame.blame.is_some()
    });
    session.app.editor.git_blame.inline_key = None;
    session.app.editor.git_blame.inline_since = None;
    let long_dwell = std::time::Instant::now();
    assert!(session.app.tick_git_blame_inline(long_dwell));
    assert!(session.app.tick_git_blame_inline(long_dwell + std::time::Duration::from_millis(400)));
    let frame = run_script(&mut session, b"mouse_move 0 0\ndump\n");
    let dump: serde_json::Value = serde_json::from_str(frame.last().unwrap().strip_prefix("ok ").unwrap()).unwrap();
    assert!(dump["blame_inline"].is_null());

    std::fs::write(&long, "external head line\n").unwrap();
    git(&root, &["add", "long.txt"]);
    git(&root, &["-c", "user.name=Focus Author", "-c", "user.email=focus@example.invalid", "commit", "-qm", "external head"]);
    let new_head = git(&root, &["rev-parse", "HEAD"]).trim().to_owned();
    session.app.on_window_focus_gained();
    wait_until(&mut session, 8000, "external HEAD snapshot", |session| {
        session.app.editor.git_head.as_ref().and_then(|head| head.head_oid).is_some_and(|oid| oid.to_string() == new_head)
    });
    wait_until(&mut session, 8000, "external HEAD blame", |session| {
        session.app.editor.git_blame.blame.as_ref().is_some_and(|blame| {
            blame.commits.iter().any(|commit| commit.author == "Focus Author")
        })
    });
    session.app.editor.git_blame.inline_key = None;
    session.app.editor.git_blame.inline_since = None;
    let focused = std::time::Instant::now();
    assert!(session.app.tick_git_blame_inline(focused));
    assert!(session.app.tick_git_blame_inline(focused + std::time::Duration::from_millis(400)));
    assert!(session.app.editor.git_blame.inline_text.contains("Focus Author"));

    let _ = std::fs::remove_dir_all(root);
}
