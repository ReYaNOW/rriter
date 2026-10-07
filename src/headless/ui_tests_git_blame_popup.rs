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
fn headless_git_blame_long_message_popup_keeps_actions_on_screen() {
    let (root, file) = git_blame_fixture(&format!("ui-git-blame-long-popup-{}", std::process::id()));
    let message = (0..60).map(|line| format!("long body line {line}")).collect::<Vec<_>>().join("\n");
    std::fs::write(&file, "updated line one\nupdated line two\nupdated line three\n")
        .unwrap_or_else(|error| panic!("write long-message blame fixture: {error}"));
    git(&root, &["add", "blame.txt"]);
    git(&root, &["commit", "-qm", &message]);
    let mut session = workspace_with_explorer(1920, 1080, 1.0, &root);
    session.app.git_blame_inline = true;
    session.app.git_blame_delay_ms = 0;
    let lines = run_script(&mut session, format!("open {}\n", file.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(&mut session, 8000, "long-message blame result", |session| {
        session.app.editor.git_blame.blame.is_some()
    });
    session.app.editor.cursor = session.app.editor.line_offsets[0];
    let now = Instant::now();
    session.app.tick_git_blame_inline(now);
    session.app.tick_git_blame_inline(now);
    let _ = run_script(&mut session, b"mouse_move 0 0\ndump\n");
    let inline = dump(&mut session)["blame_inline"].clone();
    let x = inline["x"].as_f64().unwrap_or_default() + inline["w"].as_f64().unwrap_or_default() * 0.5;
    let y = inline["y"].as_f64().unwrap_or_default() + inline["h"].as_f64().unwrap_or_default() * 0.5;
    let _ = run_script(&mut session, format!("mouse_move {x} {y}\n").as_bytes());
    wait_until(&mut session, 5000, "long popup full message", |session| {
        session.app.renderer.as_ref().is_some_and(|renderer| {
            renderer.git_blame_popup_details.as_ref().is_some_and(|details| details.message.is_some())
        })
    });
    let ui = dump(&mut session);
    let popup = session.app.renderer.as_ref().and_then(|renderer| renderer.git_blame_popup_hover).unwrap().popup;
    let buttons = ui["ui"].as_array().unwrap();
    for id in ["GitBlameCopyHash", "GitBlameShowInGraph"] {
        let rect = buttons.iter().find(|element| element["id"] == id).unwrap()["rect"].as_array().unwrap();
        let bottom = rect[1].as_f64().unwrap() + rect[3].as_f64().unwrap();
        assert!(bottom <= 1080.0, "{id} off-screen: popup={popup:?}, rect={rect:?}");
    }
    assert!(popup.1 + popup.3 <= 1080.0, "popup off-screen: {popup:?}");
    drop(session);
    let _ = std::fs::remove_dir_all(root);
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
    let line_h = 19.0 * TEST_SCALE;
    let expected_height = (105.0 * TEST_SCALE) + 5.0 * line_h;
    assert!((popup.3 - expected_height).abs() < 0.01, "popup should contain exactly five message rows: {popup:?}");
    let start_x = x as f32;
    let start_y = y as f32;
    let end_x = popup.0 + popup.2 * 0.5;
    let end_y = popup.1 + popup.3 * 0.5;
    for step in 1..=8 {
        let progress = step as f32 / 8.0;
        let mouse_x = start_x + (end_x - start_x) * progress;
        let mouse_y = start_y + (end_y - start_y) * progress;
        let lines = run_script(&mut session, format!("mouse_move {mouse_x} {mouse_y}\n").as_bytes());
        assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
        assert!(session.app.renderer.as_ref().is_some_and(|renderer| renderer.git_blame_popup_hover.is_some()), "popup closed at step {step}");
    }

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

#[test]
fn headless_git_blame_commit_popup_closes_when_dwell_key_changes() {
    let (root, _file, mut session) = open_popup_fixture();
    let _ = run_script(&mut session, b"mouse_move 0 0\ndump\n");
    let inline = dump(&mut session)["blame_inline"].clone();
    let x = inline["x"].as_f64().unwrap_or_default() + inline["w"].as_f64().unwrap_or_default() * 0.5;
    let y = inline["y"].as_f64().unwrap_or_default() + inline["h"].as_f64().unwrap_or_default() * 0.5;
    let _ = run_script(&mut session, format!("mouse_move {x} {y}\n").as_bytes());
    assert!(session.app.renderer.as_ref().is_some_and(|renderer| renderer.git_blame_popup_hover.is_some()));

    session.app.editor.cursor = session.app.editor.line_offsets[2];
    session.app.tick_git_blame_inline(Instant::now());
    let _ = run_script(&mut session, b"settle 1000\n");
    assert!(session.app.renderer.as_ref().is_some_and(|renderer| renderer.git_blame_popup_hover.is_none()));
    drop(session);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn headless_git_blame_commit_popup_closes_on_tab_switch() {
    let (root, _file, mut session) = open_popup_fixture();
    let _ = run_script(&mut session, b"mouse_move 0 0\ndump\n");
    let inline = dump(&mut session)["blame_inline"].clone();
    let x = inline["x"].as_f64().unwrap_or_default() + inline["w"].as_f64().unwrap_or_default() * 0.5;
    let y = inline["y"].as_f64().unwrap_or_default() + inline["h"].as_f64().unwrap_or_default() * 0.5;
    let _ = run_script(&mut session, format!("mouse_move {x} {y}\n").as_bytes());
    assert!(session.app.renderer.as_ref().is_some_and(|renderer| renderer.git_blame_popup_hover.is_some()));

    let other_file = root.join("other.txt");
    std::fs::write(&other_file, "other tab\n").unwrap_or_else(|error| panic!("write second tab: {error}"));
    let lines = run_script(&mut session, format!("open {}\n", other_file.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert!(session.app.active_tab > 0);
    assert!(session.app.renderer.as_ref().is_some_and(|renderer| renderer.git_blame_popup_hover.is_none()));
    drop(session);
    let _ = std::fs::remove_dir_all(root);
}
