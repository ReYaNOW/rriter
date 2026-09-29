use crate::headless::tests_support::{
    click_ui, dump, has_ui, run_script, scratch_dir, session_for_test, wait_until,
};
use std::path::{Path, PathBuf};
use crate::headless::HeadlessSession;

fn open_fixture(name: &str) -> (PathBuf, PathBuf, HeadlessSession) {
    let dir = scratch_dir(name);
    let path = crate::pdf::fixture::write_fixture_pdf(&dir);
    let mut session = session_for_test(1280, 720);
    let lines = run_script(
        &mut session,
        format!("workspace {}\nopen {}\n", dir.display(), path.display()).as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    (dir, path, session)
}

fn wait_ready(session: &mut HeadlessSession) {
    wait_until(session, 5000, "PDF opened", |session| {
        dump(session)["tabs"][0]["pdf"]["phase"] == "ready"
    });
}

#[test]
fn pdf_fixture_opens_and_draws_visible_page_placeholders() {
    let (dir, path, mut session) = open_fixture("ui-pdf-open");
    wait_ready(&mut session);
    let state = dump(&mut session);
    assert_eq!(state["tabs"][0]["kind"], "pdf");
    assert_eq!(state["tabs"][0]["pdf"]["page_count"], 3);
    assert_eq!(state["tabs"][0]["pdf"]["current_page"], 0);
    assert!(has_ui(&state, "PdfPage(0)"), "{state}");
    let shot = run_script(&mut session, format!("screenshot {}/pdf.png\n", dir.display()).as_bytes());
    assert!(shot[0].starts_with("ok "), "{shot:?}");
    assert_eq!(state["tabs"][0]["path"], path.display().to_string());
}

#[test]
fn invalid_pdf_files_show_errors_and_keep_the_session_alive() {
    for (name, make_file) in [
        ("ui-pdf-garbage", crate::pdf::fixture::write_garbage as fn(&Path) -> PathBuf),
        ("ui-pdf-empty", crate::pdf::fixture::write_empty as fn(&Path) -> PathBuf),
    ] {
        let dir = scratch_dir(name);
        let path = make_file(&dir);
        let mut session = session_for_test(1280, 720);
        let lines = run_script(&mut session, format!("workspace {}\nopen {}\n", dir.display(), path.display()).as_bytes());
        assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
        wait_until(&mut session, 5000, "invalid PDF error", |session| dump(session)["tabs"][0]["pdf"]["phase"] == "error");
        let lines = run_script(&mut session, b"mouse_move 0 0\n");
        assert_eq!(lines, ["ok"]);
    }
}

#[test]
fn opening_the_same_pdf_twice_reuses_its_tab() {
    let (dir, path, mut session) = open_fixture("ui-pdf-dedupe");
    wait_ready(&mut session);
    let lines = run_script(&mut session, format!("open {}\n", path.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert_eq!(dump(&mut session)["tabs"].as_array().map(Vec::len), Some(1));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn switching_between_pdf_tabs_keeps_the_active_path() {
    let (dir, path_a, mut session) = open_fixture("ui-pdf-switch");
    let path_b = dir.join("copy.pdf");
    std::fs::copy(&path_a, &path_b).expect("copy fixture PDF");
    let lines = run_script(&mut session, format!("open {}\n", path_b.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(&mut session, 5000, "second PDF opened", |session| dump(session)["tabs"][1]["pdf"]["phase"] == "ready");
    click_ui(&mut session, "EditorTab(0)");
    assert_eq!(dump(&mut session)["tabs"][0]["active"], true);
    assert_eq!(dump(&mut session)["tabs"][0]["path"], path_a.display().to_string());
}

#[test]
fn closing_a_pdf_before_opened_leaves_the_session_usable() {
    let (dir, path, mut session) = open_fixture("ui-pdf-close-race");
    click_ui(&mut session, "EditorTabClose(0)");
    let state = dump(&mut session);
    assert!(state["tabs"].as_array().is_none_or(|tabs| !tabs.iter().any(|tab| tab["path"] == path.display().to_string())), "{state}");
    assert_eq!(run_script(&mut session, b"mouse_move 0 0\n"), ["ok"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn unavailable_pdfium_shows_engine_message_without_install_button() {
    unsafe { std::env::set_var("RRITER_PDFIUM_PATH", "/nonexistent/libpdfium.so"); }
    let dir = scratch_dir("ui-pdf-engine-missing");
    let path = crate::pdf::fixture::write_fixture_pdf(&dir);
    let mut session = session_for_test(1280, 720);
    let lines = run_script(&mut session, format!("workspace {}\nopen {}\n", dir.display(), path.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let state = dump(&mut session);
    assert_eq!(state["tabs"][0]["pdf"]["phase"], "engine_missing");
    assert_eq!(state["tabs"][0]["pdf"]["engine"], "missing");
    assert!(state["tabs"][0]["pdf"]["engine_message"].as_str().is_some_and(|message| message.contains("/nonexistent/libpdfium.so")));
    assert!(!has_ui(&state, "PdfEngineInstall"), "{state}");
}
