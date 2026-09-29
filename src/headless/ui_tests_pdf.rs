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

fn pdf_page_center_pixel(session: &mut HeadlessSession, path: &Path) -> [u8; 4] {
    let lines = run_script(session, format!("screenshot {}\n", path.display()).as_bytes());
    assert!(lines[0].starts_with("ok "), "{lines:?}");
    let (x, y, w, h) = session.app.ui_registry.rect_for(crate::ui_system::UiId::PdfPage(0)).expect("visible first PDF page");
    image::open(path).expect("decode PDF screenshot").to_rgba8()
        .get_pixel((x + w * 0.5).round() as u32, (y + h * 0.5).round() as u32).0
}

#[test]
fn pdf_fixture_opens_and_draws_a_rasterized_page() {
    let (dir, path, mut session) = open_fixture("ui-pdf-open");
    wait_ready(&mut session);
    wait_until(&mut session, 5000, "first PDF texture", |session| dump(session)["tabs"][0]["pdf"]["textures"].as_u64().unwrap_or(0) >= 1);
    let state = dump(&mut session);
    assert_eq!(state["tabs"][0]["kind"], "pdf");
    assert_eq!(state["tabs"][0]["pdf"]["page_count"], 3);
    assert_eq!(state["tabs"][0]["pdf"]["current_page"], 0);
    assert!(has_ui(&state, "PdfPage(0)"), "{state}");
    let shot = run_script(&mut session, format!("screenshot {}/pdf.png\n", dir.display()).as_bytes());
    assert!(shot[0].starts_with("ok "), "{shot:?}");
    let (page_x, page_y, page_w, page_h) = session.app.ui_registry.rect_for(crate::ui_system::UiId::PdfPage(0)).expect("visible PDF page");
    let center_x = (page_x + page_w * 0.5).round() as u32;
    let center_y = (page_y + page_h * 0.5).round() as u32;
    let center = image::open(dir.join("pdf.png")).expect("decode PDF screenshot").to_rgba8().get_pixel(center_x, center_y).0;
    assert!(center[..3].iter().all(|channel| *channel < 80), "default dark PDF page center was {center:?}");
    let background = image::open(dir.join("pdf.png")).expect("decode PDF screenshot").to_rgba8().get_pixel(center_x.saturating_sub((page_w * 0.5) as u32 + 4), center_y).0;
    assert_ne!(&center[..3], &background[..3], "PDF page center matches the surrounding viewport");
    assert_eq!(state["tabs"][0]["path"], path.display().to_string());
}

#[test]
fn pdf_resize_navigation_wheel_and_dark_page_toggle_work() {
    let (dir, _path, mut session) = open_fixture("ui-pdf-input-render");
    wait_ready(&mut session);
    wait_until(&mut session, 5000, "initial PDF texture", |session| dump(session)["tabs"][0]["pdf"]["textures"].as_u64().unwrap_or(0) >= 1);
    let lines = run_script(&mut session, b"key end\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert_eq!(dump(&mut session)["tabs"][0]["pdf"]["current_page"], 2);
    let lines = run_script(&mut session, b"key home\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert_eq!(dump(&mut session)["tabs"][0]["pdf"]["current_page"], 0);
    let lines = run_script(&mut session, b"key pagedown\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(&mut session, 1000, "page-down scroll", |session| dump(session)["tabs"][0]["pdf"]["scroll"].as_f64().unwrap_or(0.0) > 0.0);
    let before = dump(&mut session)["tabs"][0]["pdf"]["scroll"].as_f64().unwrap_or(0.0);
    let lines = run_script(&mut session, b"wheel 0 -30\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(&mut session, 1000, "wheel scroll", |session| dump(session)["tabs"][0]["pdf"]["scroll"].as_f64().unwrap_or(0.0) > before);
    let page_before_resize = dump(&mut session)["tabs"][0]["pdf"]["current_page"].as_u64();
    let lines = run_script(&mut session, b"key a\nresize 800x600\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(&mut session, 5000, "resized PDF texture", |session| dump(session)["tabs"][0]["pdf"]["textures"].as_u64().unwrap_or(0) >= 1 && dump(session)["tabs"][0]["pdf"]["phase"] == "ready");
    let shot_path = dir.join("pdf-resized.png");
    let shot = run_script(&mut session, format!("screenshot {}\n", shot_path.display()).as_bytes());
    assert!(shot[0].starts_with("ok "), "{shot:?}");
    assert_eq!(image::image_dimensions(&shot_path).expect("resized PDF screenshot"), (800, 600));
    let state = dump(&mut session);
    assert_eq!(state["tabs"][0]["pdf"]["current_page"].as_u64(), page_before_resize);
    assert_eq!(state["tabs"][0]["modified"], false);
    click_ui(&mut session, "PdfDarkToggle");
    let shot_path = dir.join("pdf-light.png");
    run_script(&mut session, b"key home\n");
    wait_until(&mut session, 5000, "light PDF raster", |session| {
        dump(session)["tabs"][0]["pdf"]["scroll"].as_f64().unwrap_or(1.0) == 0.0
            && pdf_page_center_pixel(session, &shot_path)[..3].iter().all(|channel| *channel > 200)
    });
    let px = pdf_page_center_pixel(&mut session, &shot_path);
    assert!(px[..3].iter().all(|channel| *channel > 200), "light PDF page center was {px:?}");
    click_ui(&mut session, "PdfDarkToggle");
    let dark_path = dir.join("pdf-dark-again.png");
    wait_until(&mut session, 5000, "dark PDF raster", |session| pdf_page_center_pixel(session, &dark_path)[..3].iter().all(|channel| *channel < 80));
    let dark = pdf_page_center_pixel(&mut session, &dark_path);
    assert!(dark[..3].iter().all(|channel| *channel < 80), "dark PDF page center was {dark:?}");
}

#[test]
fn garbage_pdf_shows_error_and_keeps_the_session_alive() {
    assert_invalid_pdf_shows_error("ui-pdf-garbage", crate::pdf::fixture::write_garbage);
}

#[test]
fn empty_pdf_shows_error_and_keeps_the_session_alive() {
    assert_invalid_pdf_shows_error("ui-pdf-empty", crate::pdf::fixture::write_empty);
}

// One session per test: pdfium binds once per process and the engine never restarts (spec §4.3).
fn assert_invalid_pdf_shows_error(name: &str, make_file: fn(&Path) -> PathBuf) {
    let dir = scratch_dir(name);
    let path = make_file(&dir);
    let mut session = session_for_test(1280, 720);
    let lines = run_script(&mut session, format!("workspace {}\nopen {}\n", dir.display(), path.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(&mut session, 5000, "invalid PDF error", |session| dump(session)["tabs"][0]["pdf"]["phase"] == "error");
    let lines = run_script(&mut session, b"mouse_move 0 0\n");
    assert_eq!(lines, ["ok"]);
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

fn wait_page_text(session: &mut HeadlessSession, page: usize) {
    wait_until(session, 5000, "PDF page text", |session| {
        session.app.active_pdf_tab().is_some_and(|pdf| pdf.text.get(page).is_some_and(Option::is_some))
    });
}

/// Window point of a page coordinate (points, Y down) on page 0 at scroll 0.
fn page_point(session: &HeadlessSession, x_pt: f32, y_pt: f32) -> (f32, f32) {
    let (x, y, w, _) = session.app.ui_registry.rect_for(crate::ui_system::UiId::PdfPage(0)).expect("visible first PDF page");
    let k = w / 612.0;
    (x + x_pt * k, y + y_pt * k)
}

#[test]
fn pdf_search_matches_jump_between_pages_and_handle_empty_and_missing_queries() {
    let (dir, _path, mut session) = open_fixture("ui-pdf-search");
    wait_ready(&mut session);
    let lines = run_script(&mut session, b"key ctrl+f\ntype second\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(&mut session, 5000, "two search matches", |session| {
        let pdf = &dump(session)["tabs"][0]["pdf"];
        pdf["search_matches"] == 2 && pdf["search_done"] == true
    });
    assert_eq!(dump(&mut session)["tabs"][0]["pdf"]["search_current"], 0);
    assert_eq!(dump(&mut session)["tabs"][0]["pdf"]["current_page"], 0);
    let lines = run_script(&mut session, b"key enter\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(&mut session, 5000, "jump to the match on page 2", |session| {
        let pdf = &dump(session)["tabs"][0]["pdf"];
        pdf["search_current"] == 1 && pdf["current_page"] == 1
    });
    run_script(&mut session, b"key shift+enter\n");
    wait_until(&mut session, 5000, "jump back to the first match", |session| {
        let pdf = &dump(session)["tabs"][0]["pdf"];
        pdf["search_current"] == 0 && pdf["current_page"] == 0
    });
    // Negative: a query with no matches and an empty query both finish with zero matches.
    run_script(&mut session, b"key ctrl+a\ntype zzzzqqq\n");
    wait_until(&mut session, 5000, "no matches", |session| {
        let pdf = &dump(session)["tabs"][0]["pdf"];
        pdf["search_matches"] == 0 && pdf["search_done"] == true
    });
    run_script(&mut session, b"key ctrl+a\ntype second\n");
    wait_until(&mut session, 5000, "matches again", |session| dump(session)["tabs"][0]["pdf"]["search_matches"] == 2);
    run_script(&mut session, b"key ctrl+a\nkey backspace\n");
    wait_until(&mut session, 5000, "empty query", |session| {
        let pdf = &dump(session)["tabs"][0]["pdf"];
        pdf["search_matches"] == 0 && pdf["search_done"] == true
    });
    run_script(&mut session, b"key escape\n");
    assert_eq!(dump(&mut session)["overlays"]["search"], false);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn pdf_selection_copies_text_and_ctrl_keys_never_reach_the_hidden_editor() {
    let (dir, path, mut session) = open_fixture("ui-pdf-selection");
    wait_ready(&mut session);
    wait_page_text(&mut session, 0);
    let before = std::fs::read(&path).expect("fixture PDF");
    // Negative: nothing is selected, so Ctrl+C leaves the clipboard alone.
    run_script(&mut session, b"key ctrl+c\n");
    assert_eq!(dump(&mut session)["clipboard"]["text"], serde_json::Value::Null);
    // Ctrl+S / Ctrl+V / Ctrl+X / Ctrl+A stay inside the PDF tab: the file is not rewritten.
    run_script(&mut session, b"key ctrl+s\nkey ctrl+v\nkey ctrl+x\nkey ctrl+a\nkey ctrl+z\n");
    let state = dump(&mut session);
    assert_eq!(state["tabs"][0]["modified"], false, "{state}");
    assert_eq!(std::fs::read(&path).expect("fixture PDF"), before, "Ctrl+S must not overwrite the PDF");
    let (x0, y0) = page_point(&session, 73.0, 85.0);
    let (x1, y1) = page_point(&session, 200.0, 85.0);
    let script = format!("mouse_move {x0} {y0}\nclick down\nmouse_move {x1} {y1}\nclick up\n");
    let lines = run_script(&mut session, script.as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert!(dump(&mut session)["tabs"][0]["pdf"]["selection_chars"].as_u64().unwrap_or(0) >= 5);
    run_script(&mut session, b"key ctrl+c\n");
    let copied = dump(&mut session)["clipboard"]["text"].as_str().unwrap_or_default().to_owned();
    assert!(copied.starts_with("Hello"), "copied {copied:?}");
    run_script(&mut session, b"key escape\n");
    assert_eq!(dump(&mut session)["tabs"][0]["pdf"]["selection_chars"], 0);
    // A plain click (no drag) also leaves nothing selected.
    run_script(&mut session, format!("mouse_move {x0} {y0}\nclick\n").as_bytes());
    assert_eq!(dump(&mut session)["tabs"][0]["pdf"]["selection_chars"], 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn pdf_links_open_web_addresses_scroll_to_pages_and_ignore_other_schemes() {
    let (dir, _path, mut session) = open_fixture("ui-pdf-links");
    wait_ready(&mut session);
    wait_page_text(&mut session, 0);
    // Negative: a mailto: link is never handed to the OS opener.
    let (x, y) = page_point(&session, 100.0, 180.0);
    run_script(&mut session, format!("mouse_move {x} {y}\nclick\n").as_bytes());
    assert_eq!(session.app.external_requests.take(), None);
    // Negative: a click on blank page area does nothing either.
    let (x, y) = page_point(&session, 500.0, 400.0);
    run_script(&mut session, format!("mouse_move {x} {y}\nclick\n").as_bytes());
    assert_eq!(session.app.external_requests.take(), None);
    let (x, y) = page_point(&session, 100.0, 80.0);
    run_script(&mut session, format!("mouse_move {x} {y}\nclick\n").as_bytes());
    assert_eq!(
        session.app.external_requests.take(),
        Some(crate::platform::ExternalRequest::OpenUrl("https://example.com/".to_owned()))
    );
    let (x, y) = page_point(&session, 100.0, 120.0);
    run_script(&mut session, format!("mouse_move {x} {y}\nclick\n").as_bytes());
    wait_until(&mut session, 5000, "GoTo link scrolls to page 2", |session| dump(session)["tabs"][0]["pdf"]["current_page"] == 1);
    let _ = std::fs::remove_dir_all(dir);
}
