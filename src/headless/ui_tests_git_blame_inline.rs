use crate::headless::tests_support::{
    git, git_blame_fixture, run_script, wait_until, workspace_with_explorer,
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
