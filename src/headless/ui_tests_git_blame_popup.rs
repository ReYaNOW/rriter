use crate::headless::HeadlessSession;
use crate::headless::tests_support::{
    click_ui, dump, git, git_blame_fixture, run_script, wait_until,
    workspace_with_explorer,
};
use std::time::Instant;

const TEST_WIDTH: u32 = 1920;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;

fn open_popup_fixture() -> (std::path::PathBuf, std::path::PathBuf, HeadlessSession) {
    let (root, file) = git_blame_fixture(&format!("ui-git-blame-popup-{}", std::process::id()));
    std::fs::write(&file, "first updated\nsecond updated\nthird\nfourth\n")
        .unwrap_or_else(|error| panic!("write blame fixture: {error}"));
    git(&root, &["add", "blame.txt"]);
    git(
        &root,
        &[
            "-c", "user.name=Popup Author", "-c", "user.email=popup@example.invalid",
            "commit", "-m", "popup subject", "-m", "first body line", "-m", "second body line",
            "--date=2004-01-01T00:00:00Z",
        ],
    );
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &root);
    session.app.git_blame_inline = true;
    session.app.git_blame_delay_ms = 0;
    let lines = run_script(&mut session, format!("open {}\n", file.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(&mut session, 8000, "blame popup fixture", |session| {
        session.app.editor.git_blame.blame.is_some()
    });
    session.app.editor.cursor = session.app.editor.line_offsets[3];
    let now = Instant::now();
    session.app.tick_git_blame_inline(now);
    session.app.tick_git_blame_inline(now);
    (root, file, session)
}

#[test]
fn headless_git_blame_commit_popup_shows_full_details_copies_hash_and_dismisses() {
    let (root, _file, mut session) = open_popup_fixture();
    let _ = run_script(&mut session, b"mouse_move 0 0\ndump\n");
    let inline = dump(&mut session)["blame_inline"].clone();
    assert!(!inline.is_null(), "annotation must fit in the wide test window");
    let x = inline["x"].as_f64().unwrap_or_default() + inline["w"].as_f64().unwrap_or_default() * 0.5;
    let y = inline["y"].as_f64().unwrap_or_default() + inline["h"].as_f64().unwrap_or_default() * 0.5;
    let lines = run_script(&mut session, format!("mouse_move {x} {y}\n").as_bytes());
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    wait_until(&mut session, 5000, "full popup commit message", |session| {
        session.app.renderer.as_ref().is_some_and(|renderer| {
            renderer.git_blame_popup_details.as_ref().is_some_and(|details| details.message.is_some())
        })
    });

    let renderer = session.app.renderer.as_ref().unwrap_or_else(|| panic!("renderer missing"));
    let details = renderer.git_blame_popup_details.as_ref().unwrap_or_else(|| panic!("popup details missing"));
    let expected_oid = git(&root, &["rev-parse", "HEAD"]).trim().to_string();
    assert_eq!(details.oid, expected_oid);
    assert_eq!(details.author, "Popup Author");
    assert_eq!(details.author_mail.as_deref(), Some("popup@example.invalid"));
    assert_eq!(details.summary, "popup subject");
    assert_eq!(details.message.as_deref(), Some("popup subject\n\nfirst body line\n\nsecond body line\n"));
    assert!(renderer.git_blame_popup_hover.is_some());

    let popup = renderer.git_blame_popup_hover.unwrap_or_else(|| panic!("popup hover missing")).popup;
    assert!(popup.3 > 280.0, "full multi-line message should expand popup height: {popup:?}");
    let lines = run_script(&mut session, format!("mouse_move {} {}\n", popup.0 + popup.2 * 0.5, popup.1 + popup.3 * 0.5).as_bytes());
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    assert!(session.app.renderer.as_ref().is_some_and(|renderer| renderer.git_blame_popup_hover.is_some()));

    click_ui(&mut session, "GitBlameCopyHash");
    assert_eq!(dump(&mut session)["clipboard"]["text"].as_str(), Some(expected_oid.as_str()));
    assert!(session.app.renderer.as_ref().is_some_and(|renderer| renderer.git_blame_popup_copied.is_some()));

    let lines = run_script(&mut session, b"mouse_move 0 0\n");
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    let _ = dump(&mut session);
    assert!(session.app.renderer.as_ref().is_some_and(|renderer| renderer.git_blame_popup_hover.is_none()));
    drop(session);
    let _ = std::fs::remove_dir_all(root);
}
