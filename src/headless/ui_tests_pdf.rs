use crate::headless::tests_support::{
    active_tab_index, click_ui, dump, has_ui, run_script, scratch_dir, session_for_test, ui_center, wait_until,
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
    let (x, y, w, h) = session.app.ui_registry.rect_for(crate::ui_system::UiId::PdfPage(0)).unwrap_or_else(|| panic!("visible first PDF page"));
    image::open(path).unwrap_or_else(|error| panic!("decode PDF screenshot: {error}")).to_rgba8()
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
    let dir = scratch_dir("ui-pdf-engine-missing");
    let path = crate::pdf::fixture::write_fixture_pdf(&dir);
    let mut session = session_for_test(1280, 720);
    session.app.pdf_library_source = crate::app::pdf_tab::PdfLibrarySource::EnvValue(Some("/nonexistent/libpdfium.so".into()));
    let lines = run_script(&mut session, format!("workspace {}\nopen {}\n", dir.display(), path.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let state = dump(&mut session);
    assert_eq!(state["tabs"][0]["pdf"]["phase"], "engine_missing");
    assert_eq!(state["tabs"][0]["pdf"]["engine"], "missing");
    assert!(state["tabs"][0]["pdf"]["engine_message"].as_str().is_some_and(|message| message.contains("/nonexistent/libpdfium.so")));
    assert!(!has_ui(&state, "PdfEngineInstall"), "{state}");
}

/// Scenario 7. The test build never reaches the network: `install_pdfium` fails at once, so this
/// covers the button -> Installing -> Missing(retry) state machine, not the download itself.
#[test]
fn missing_pdfium_offers_download_and_returns_to_retry_when_install_fails() {
    // The library is in neither the test binary's directory nor the per-process test profile.
    let dir = scratch_dir("ui-pdf-engine-install");
    let path = crate::pdf::fixture::write_fixture_pdf(&dir);
    let mut session = session_for_test(1280, 720);
    session.app.pdf_library_source = crate::app::pdf_tab::PdfLibrarySource::EnvValue(None);
    let lines = run_script(&mut session, format!("workspace {}\nopen {}\n", dir.display(), path.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let state = dump(&mut session);
    assert_eq!(state["tabs"][0]["pdf"]["phase"], "engine_missing");
    assert_eq!(state["tabs"][0]["pdf"]["engine"], "missing");
    assert!(has_ui(&state, "PdfEngineInstall"), "{state}");

    click_ui(&mut session, "PdfEngineInstall");
    wait_until(&mut session, 5000, "failed engine install", |session| {
        dump(session)["tabs"][0]["pdf"]["engine_message"].as_str().is_some_and(|message| message.contains("отключена в тестах"))
    });
    let state = dump(&mut session);
    assert_eq!(state["tabs"][0]["pdf"]["phase"], "engine_missing");
    assert_eq!(state["tabs"][0]["pdf"]["engine"], "missing");
    assert!(has_ui(&state, "PdfEngineInstall"), "retry button expected: {state}");
    assert!(!has_ui(&state, "PdfEngineCancel"), "{state}");
    let _ = std::fs::remove_dir_all(dir);
}

fn wait_page_text(session: &mut HeadlessSession, page: usize) {
    wait_until(session, 5000, "PDF page text", |session| {
        session.app.active_pdf_tab().is_some_and(|pdf| pdf.text.get(page).is_some_and(Option::is_some))
    });
}

/// Window point of a page coordinate (points, Y down) on page 0 at scroll 0.
fn page_point(session: &HeadlessSession, x_pt: f32, y_pt: f32) -> (f32, f32) {
    page_point_on(session, 0, x_pt, y_pt)
}

/// Window point of a page coordinate on a page that is currently visible.
fn page_point_on(session: &HeadlessSession, page: usize, x_pt: f32, y_pt: f32) -> (f32, f32) {
    let (x, y, w, _) = session.app.ui_registry.rect_for(crate::ui_system::UiId::PdfPage(page)).unwrap_or_else(|| panic!("visible PDF page {page}"));
    let k = w / 612.0;
    (x + x_pt * k, y + y_pt * k)
}

/// Registry name of the first-page link whose target satisfies `want` (index comes from the tab, not from the fixture order).
fn link_id(session: &HeadlessSession, want: impl Fn(&crate::pdf::LinkTarget) -> bool) -> String {
    let pdf = session.app.active_pdf_tab().unwrap_or_else(|| panic!("PDF tab"));
    let idx = pdf.links[0].iter().position(|link| want(&link.target)).unwrap_or_else(|| panic!("fixture link on page 0"));
    format!("PdfLink(0, {idx})")
}

fn hovered_ui(session: &mut HeadlessSession) -> serde_json::Value {
    dump(session)["hover"]["ui"].clone()
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
    // A query with one match: Enter / Shift+Enter cycle onto the same match without a panic or a jump.
    run_script(&mut session, b"type Go\n");
    wait_until(&mut session, 5000, "one match for Go", |session| {
        let pdf = &dump(session)["tabs"][0]["pdf"];
        pdf["search_matches"] == 1 && pdf["search_done"] == true
    });
    for key in ["enter", "shift+enter", "enter"] {
        let lines = run_script(&mut session, format!("key {key}\n").as_bytes());
        assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
        let state = dump(&mut session);
        assert_eq!(state["tabs"][0]["pdf"]["search_current"], 0, "{key}: {state}");
        assert_eq!(state["tabs"][0]["pdf"]["current_page"], 0, "{key}: {state}");
    }
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
    // The save sink itself refuses a PDF tab, whatever key path (Ctrl+Enter, autosave) reached it.
    assert_eq!(session.app.save_current_file_outcome(), crate::app::SaveOutcome::Failed);
    assert_eq!(std::fs::read(&path).expect("fixture PDF"), before, "the save sink must not overwrite the PDF");
    // A Save As picker that returns while the PDF tab is active must not write the empty hidden editor.
    let save_as_target = dir.join("picked-late.txt");
    let (pdf_path_before, tab_path_before) = (session.app.active_pdf_tab().map(|pdf| pdf.path.clone()), session.app.file_path.clone());
    assert!(!session.app.save_current_file_as(save_as_target.clone()));
    assert!(!save_as_target.exists(), "Save As must not create a file for a PDF tab");
    assert_eq!(session.app.active_pdf_tab().map(|pdf| pdf.path.clone()), pdf_path_before);
    assert_eq!(session.app.file_path, tab_path_before, "Save As must not retarget the PDF tab");
    // Escape drops the selection first; the open search panel closes only on the next Escape.
    run_script(&mut session, b"key ctrl+f\n");
    assert_eq!(dump(&mut session)["overlays"]["search"], true);
    run_script(&mut session, script.as_bytes());
    assert!(dump(&mut session)["tabs"][0]["pdf"]["selection_chars"].as_u64().unwrap_or(0) >= 5);
    run_script(&mut session, b"key escape\n");
    let state = dump(&mut session);
    assert_eq!(state["tabs"][0]["pdf"]["selection_chars"], 0, "{state}");
    assert_eq!(state["overlays"]["search"], true, "{state}");
    run_script(&mut session, b"key escape\n");
    assert_eq!(dump(&mut session)["overlays"]["search"], false);
    // Autoscroll: holding a drag at the bottom edge scrolls the document; the release ends it.
    let bottom = session.app.active_pdf_tab().map(|pdf| pdf.body.1 + pdf.body.3 - 4.0).expect("PDF body");
    let script = format!("mouse_move {x0} {y0}\nclick down\nmouse_move {x1} {bottom}\n");
    let lines = run_script(&mut session, script.as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(&mut session, 5000, "edge autoscroll", |session| dump(session)["tabs"][0]["pdf"]["scroll"].as_f64().unwrap_or(0.0) > 50.0);
    run_script(&mut session, b"click up\n");
    assert!(session.app.active_pdf_tab().is_some_and(|pdf| pdf.press.is_none()));
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
    let (px, py, pw, ph) = session.app.ui_registry.rect_for(crate::ui_system::UiId::PdfPage(0)).expect("visible PDF page");
    run_script(&mut session, format!("mouse_move {} {}\nclick\n", px + pw * 0.9, py + ph * 0.5).as_bytes());
    assert_eq!(session.app.external_requests.take(), None);
    let tabs_before = dump(&mut session)["tabs"].as_array().map(Vec::len);
    let https = link_id(&session, |target| matches!(target, crate::pdf::LinkTarget::Uri(uri) if uri.starts_with("https://")));
    let goto = link_id(&session, |target| matches!(target, crate::pdf::LinkTarget::Page { page: 1, .. }));
    let fit = link_id(&session, |target| matches!(target, crate::pdf::LinkTarget::Page { page: 2, .. }));
    // Every link is a registry element named PdfLink(page, idx).
    let state = dump(&mut session);
    assert!(has_ui(&state, &https) && has_ui(&state, &goto) && has_ui(&state, &fit), "{state}");
    // 4h: hovering a link names it; blank page area and the margin name the page / body instead.
    let (x, y) = ui_center(&state, &https);
    run_script(&mut session, format!("mouse_move {x} {y}\n").as_bytes());
    assert_eq!(hovered_ui(&mut session), https.as_str());
    // The registered page rect is clipped to the body, so its middle is always on screen.
    let (px, py, pw, ph) = session.app.ui_registry.rect_for(crate::ui_system::UiId::PdfPage(0)).expect("visible PDF page");
    run_script(&mut session, format!("mouse_move {} {}\n", px + pw * 0.9, py + ph * 0.5).as_bytes());
    assert_eq!(hovered_ui(&mut session), "PdfPage(0)");
    run_script(&mut session, format!("mouse_move {} {}\n", px - 3.0, py + ph * 0.5).as_bytes());
    assert_eq!(hovered_ui(&mut session), "PdfBody");
    // 4: a click on the web link opens it through the external opener; the tab and page stay put.
    click_ui(&mut session, &https);
    assert_eq!(
        session.app.external_requests.take(),
        Some(crate::platform::ExternalRequest::OpenUrl("https://example.com/".to_owned()))
    );
    let state = dump(&mut session);
    assert_eq!(state["tabs"].as_array().map(Vec::len), tabs_before, "{state}");
    assert_eq!(state["tabs"][0]["pdf"]["current_page"], 0, "{state}");
    // 3: a GoTo link jumps to its page, then Home returns.
    click_ui(&mut session, &goto);
    wait_until(&mut session, 5000, "GoTo link scrolls to page 2", |session| dump(session)["tabs"][0]["pdf"]["current_page"] == 1);
    run_script(&mut session, b"key home\n");
    wait_until(&mut session, 5000, "Home returns to page 1", |session| dump(session)["tabs"][0]["pdf"]["current_page"] == 0);
    // 3 (second part): the /Fit destination lands on page 3, then Home returns again.
    click_ui(&mut session, &fit);
    wait_until(&mut session, 5000, "Fit link scrolls to page 3", |session| dump(session)["tabs"][0]["pdf"]["current_page"] == 2);
    run_script(&mut session, b"key home\n");
    wait_until(&mut session, 5000, "Home returns to page 1 again", |session| dump(session)["tabs"][0]["pdf"]["current_page"] == 0);
    assert_eq!(session.app.external_requests.take(), None);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn pdf_scan_page_has_no_selectable_text_and_search_finds_matches_with_page_three_open() {
    let (dir, _path, mut session) = open_fixture("ui-pdf-scan");
    wait_ready(&mut session);
    run_script(&mut session, b"key end\n");
    wait_page_text(&mut session, 2);
    wait_until(&mut session, 5000, "scroll settles on page 3", |session| {
        session.app.active_pdf_tab().is_some_and(|pdf| (pdf.scroll.current - pdf.scroll.target).abs() < 0.5)
    });
    // S3: a drag over a page with no text layer selects nothing and Ctrl+C leaves the clipboard alone.
    let (x0, y0) = page_point_on(&session, 2, 100.0, 300.0);
    let (x1, y1) = page_point_on(&session, 2, 400.0, 350.0);
    let script = format!("mouse_move {x0} {y0}\nclick down\nmouse_move {x1} {y1}\nclick up\nkey ctrl+c\nwait 300\n");
    let lines = run_script(&mut session, script.as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let state = dump(&mut session);
    assert_eq!(state["tabs"][0]["pdf"]["selection_chars"], 0, "{state}");
    assert_eq!(state["clipboard"]["text"], serde_json::Value::Null, "{state}");
    // Search from page 3: both "second" matches (page 1 and 2) are found and the counter finishes.
    run_script(&mut session, b"key ctrl+f\ntype second\n");
    wait_until(&mut session, 5000, "search finished", |session| {
        let state = dump(session);
        state["tabs"][0]["pdf"]["search_done"] == true && state["tabs"][0]["pdf"]["search_matches"] == 2
    });
    let _ = std::fs::remove_dir_all(dir);
}

/// Track thumb colour of the status-bar progress indicator, blended over the bar background.
fn is_progress_track_purple(pixel: [u8; 4]) -> bool {
    (125..=155).contains(&pixel[0]) && (70..=100).contains(&pixel[1]) && (180..=215).contains(&pixel[2])
}

#[test]
fn ready_pdf_tab_shows_page_label_without_a_progress_track() {
    let (dir, _path, mut session) = open_fixture("ui-pdf-status-no-track");
    wait_ready(&mut session);
    run_script(&mut session, b"mouse_move 0 0\n");
    assert_eq!(session.app.active_pdf_tab().and_then(|pdf| pdf.status_page()), Some((1, 3)));
    let shot = dir.join("status.png");
    let lines = run_script(&mut session, format!("screenshot {}\n", shot.display()).as_bytes());
    assert!(lines[0].starts_with("ok "), "{lines:?}");
    let image = image::open(&shot).unwrap_or_else(|error| panic!("decode screenshot: {error}")).to_rgba8();
    let bar_top = image.height().saturating_sub(40);
    let purple = (bar_top..image.height())
        .flat_map(|y| (0..image.width()).map(move |x| (x, y)))
        .filter(|&(x, y)| is_progress_track_purple(image.get_pixel(x, y).0))
        .count();
    assert_eq!(purple, 0, "a ready PDF tab must not draw the progress thumb in the status bar");
    let _ = std::fs::remove_dir_all(dir);
}

/// Bright (text-coloured) pixels of the status bar within `x0..x1`.
fn bright_status_pixels(image: &image::RgbaImage, x0: u32, x1: u32) -> usize {
    let bar_top = image.height().saturating_sub(40);
    (bar_top..image.height())
        .flat_map(|y| (x0..x1.min(image.width())).map(move |x| (x, y)))
        .filter(|&(x, y)| image.get_pixel(x, y).0[..3].iter().all(|&c| c > 140))
        .count()
}

fn status_bar_shot(session: &mut HeadlessSession, path: &Path) -> image::RgbaImage {
    let lines = run_script(session, format!("mouse_move 0 0\nscreenshot {}\n", path.display()).as_bytes());
    assert!(lines[1].starts_with("ok "), "{lines:?}");
    image::open(path).unwrap_or_else(|error| panic!("decode screenshot: {error}")).to_rgba8()
}

#[test]
fn pdf_tab_status_bar_shows_page_label_and_pdf_language_instead_of_cursor_items() {
    let (dir, _path, mut session) = open_fixture("ui-pdf-status-right");
    wait_ready(&mut session);
    let image = status_bar_shot(&mut session, &dir.join("status.png"));
    let state = dump(&mut session);
    let toggle = crate::headless::tests_support::ui_rect(&state, "PdfDarkToggle");
    let renderer = session.app.renderer.as_mut().expect("renderer");
    let label = renderer.pdf_page_label_layout(1, 3);
    let language_w = f64::from(renderer.measure_ui_width("PDF", 0.95).round());
    let language_x = 1280.0 - 10.0 - language_w;
    let page_right = language_x - 22.0;
    let label_x = page_right - f64::from(label.width);
    // Right to left: "PDF" language label, page label, dark toggle one gap left of the page label.
    let toggle_right = toggle[0] + toggle[2];
    assert!((label_x - 14.0 - toggle_right).abs() <= 2.0, "toggle {toggle:?}, label_x {label_x}");
    // Nothing (no "Стр"/"Сим" items) between the toggle and the label; the "PDF" slot is drawn.
    assert_eq!(bright_status_pixels(&image, (toggle_right + 2.0) as u32, (label_x - 2.0) as u32), 0);
    assert!(bright_status_pixels(&image, label_x as u32, (page_right + 1.0) as u32) > 0, "page label must be drawn");
    assert_eq!(bright_status_pixels(&image, (page_right + 2.0) as u32, (language_x - 2.0) as u32), 0);
    assert!(bright_status_pixels(&image, language_x as u32, 1270) > 0, "PDF language label must be drawn");
    // Nothing between the diagnostics group and the toggle either.
    assert_eq!(bright_status_pixels(&image, 300, (toggle[0] - 2.0) as u32), 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn regular_tab_status_bar_still_shows_cursor_and_language_items() {
    let dir = scratch_dir("ui-status-regular");
    let path = dir.join("a.txt");
    std::fs::write(&path, "hello\n").expect("write text file");
    let mut session = session_for_test(1280, 720);
    let lines = run_script(&mut session, format!("workspace {}\nopen {}\n", dir.display(), path.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let image = status_bar_shot(&mut session, &dir.join("status.png"));
    assert!(bright_status_pixels(&image, 300, 1160) > 0, "line/col items expected");
    assert!(bright_status_pixels(&image, 1160, 1270) > 0, "language label expected");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn pdf_page_label_follows_scroll_and_its_layout_ignores_the_current_page() {
    let (dir, _path, mut session) = open_fixture("ui-pdf-status-label");
    wait_ready(&mut session);
    run_script(&mut session, b"key end\nmouse_move 0 0\n");
    wait_until(&mut session, 5000, "status label shows the last page", |session| {
        run_script(session, b"mouse_move 0 0\n");
        session.app.active_pdf_tab().and_then(|pdf| pdf.status_page()) == Some((3, 3))
    });
    let renderer = session.app.renderer.as_mut().expect("renderer");
    let first = renderer.pdf_page_label_layout(1, 7);
    let last = renderer.pdf_page_label_layout(7, 7);
    assert_eq!(first, last, "single-digit page numbers must not move any part of the label");
    let nine = renderer.pdf_page_label_layout(9, 12);
    let ten = renderer.pdf_page_label_layout(10, 12);
    assert_eq!((nine.separator_dx, nine.count_dx, nine.width), (ten.separator_dx, ten.count_dx, ten.width), "9 -> 10 must not shift the separator, the count or the width");
    assert!(nine.number_dx > ten.number_dx, "the shorter number is right-aligned inside the same field");
    let _ = std::fs::remove_dir_all(dir);
}

/// WCAG relative luminance of a screenshot pixel.
fn pixel_luminance(pixel: [u8; 4]) -> f32 {
    let linear = |value: u8| {
        let value = f32::from(value) / 255.0;
        if value <= 0.04045 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * linear(pixel[0]) + 0.7152 * linear(pixel[1]) + 0.0722 * linear(pixel[2])
}

#[test]
fn dark_pages_keep_highlighted_text_readable_on_a_darkened_highlight() {
    let dir = scratch_dir("ui-pdf-dark-highlight");
    let path = crate::pdf::fixture::write_fixture_pdf_highlight(&dir);
    let mut session = session_for_test(1280, 720);
    let lines = run_script(&mut session, format!("workspace {}\nopen {}\n", dir.display(), path.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_ready(&mut session);
    wait_until(&mut session, 5000, "highlight page texture", |session| dump(session)["tabs"][0]["pdf"]["textures"].as_u64().unwrap_or(0) >= 1);
    // Dark pages are the default (see `pdf_fixture_opens_and_draws_a_rasterized_page`).
    let shot_path = dir.join("pdf-dark-highlight.png");
    let (bg_x, bg_y) = page_point(&session, 100.0, 150.0);
    wait_until(&mut session, 5000, "highlight rasterized", |session| {
        let lines = run_script(session, format!("screenshot {}\n", shot_path.display()).as_bytes());
        lines[0].starts_with("ok ") && image::open(&shot_path).is_ok_and(|shot| {
            let pixel = shot.to_rgba8().get_pixel(bg_x.round() as u32, bg_y.round() as u32).0;
            pixel[0] > pixel[2].saturating_add(40)
        })
    });
    let shot = image::open(&shot_path).unwrap_or_else(|error| panic!("decode PDF screenshot: {error}")).to_rgba8();
    let highlight = shot.get_pixel(bg_x.round() as u32, bg_y.round() as u32).0;
    // Brightest pixel over the top of the "HH" glyphs (x 150..367 pt, Y-down 234..342 pt).
    let (x0, y0) = page_point(&session, 160.0, 245.0);
    let (x1, y1) = page_point(&session, 360.0, 300.0);
    let text = (x0.round() as u32..(x1.round() as u32).min(shot.width()))
        .flat_map(|x| (y0.round() as u32..(y1.round() as u32).min(shot.height())).map(move |y| (x, y)))
        .map(|(x, y)| shot.get_pixel(x, y).0)
        .max_by(|a, b| pixel_luminance(*a).total_cmp(&pixel_luminance(*b)))
        .unwrap_or_else(|| panic!("text area on screen"));
    assert!(pixel_luminance(text) > 0.7, "inverted black text should be light, got {text:?} (highlight {highlight:?})");
    assert!(
        highlight[0] > highlight[2].saturating_add(40) && highlight[0].abs_diff(highlight[1]) < 30,
        "highlight should stay yellow, got {highlight:?}"
    );
    let contrast = (pixel_luminance(text) + 0.05) / (pixel_luminance(highlight) + 0.05);
    assert!(contrast >= 4.5, "text {text:?} on highlight {highlight:?}: contrast {contrast:.2} < 4.5");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn pdf_tab_allow_list_keeps_editing_keys_and_ime_out_of_the_hidden_editor() {
    let (dir, path, mut session) = open_fixture("ui-pdf-allow-list");
    wait_ready(&mut session);
    wait_page_text(&mut session, 0);
    let before = std::fs::read(&path).expect("fixture PDF");
    // Seeded directly (no UI path fills the hidden editor): a non-empty text and a cursor
    // in its middle make any edit or cursor move visible. A frame is rendered afterwards.
    session.app.editor = crate::app::reviewer_stage2_editor_with("one\ntwo\nthree");
    session.app.editor.cursor = 5;
    run_script(&mut session, b"mouse_move 0 0\n");
    // Editing shortcuts, cursor keys with modifiers and IME text never reach the hidden editor.
    let script = "key alt+up\nkey alt+down\nkey ctrl+d\nkey ctrl+shift+d\nkey alt+enter\nkey tab\nkey enter\nkey space\nkey backspace\nkey delete\ntype \u{436}\u{436}\ntype abc\n";
    let lines = run_script(&mut session, script.as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert_eq!(session.app.editor.get_full_text(), "one\ntwo\nthree");
    assert_eq!(session.app.editor.cursor, 5, "no key or IME text may move the hidden cursor");
    let state = dump(&mut session);
    assert_eq!(state["tabs"][0]["modified"], false, "{state}");
    assert_eq!(std::fs::read(&path).expect("fixture PDF"), before);
    // The PDF keys still work: page navigation ...
    run_script(&mut session, b"key end\n");
    assert_eq!(dump(&mut session)["tabs"][0]["pdf"]["current_page"], 2);
    run_script(&mut session, b"key home\n");
    assert_eq!(dump(&mut session)["tabs"][0]["pdf"]["current_page"], 0);
    // ... and copy of a selection.
    let (x0, y0) = page_point(&session, 73.0, 85.0);
    let (x1, y1) = page_point(&session, 200.0, 85.0);
    let script = format!("mouse_move {x0} {y0}\nclick down\nmouse_move {x1} {y1}\nclick up\nkey ctrl+c\n");
    run_script(&mut session, script.as_bytes());
    let copied = dump(&mut session)["clipboard"]["text"].as_str().unwrap_or_default().to_owned();
    assert!(copied.starts_with("Hello"), "copied {copied:?}");
    // The search field is an application shortcut plus a PDF text field: it takes the IME text.
    run_script(&mut session, b"key ctrl+f\ntype second\n");
    wait_until(&mut session, 5000, "search finished", |session| {
        let state = dump(session);
        state["tabs"][0]["pdf"]["search_done"] == true && state["tabs"][0]["pdf"]["search_matches"] == 2
    });
    assert_eq!(dump(&mut session)["overlays"]["search"], true);
    assert_eq!(session.app.editor.get_full_text(), "one\ntwo\nthree", "search typing must not leak into the hidden editor");
    // Application shortcuts still work on a PDF tab: close the search, open a second tab, then
    // switch tabs with Ctrl+PageDown / Ctrl+PageUp, toggle settings with F1 and close the tab with Ctrl+4.
    run_script(&mut session, b"key escape\n");
    assert_eq!(dump(&mut session)["overlays"]["search"], false);
    let second = dir.join("second.txt");
    std::fs::write(&second, "second tab\n").expect("write second file");
    let lines = run_script(&mut session, format!("open {}\n", second.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert_eq!(active_tab_index(&session), 1, "the opened file is active");
    run_script(&mut session, b"key ctrl+pageup\n");
    assert_eq!(active_tab_index(&session), 0);
    assert_eq!(dump(&mut session)["tabs"][0]["kind"], "pdf");
    run_script(&mut session, b"key ctrl+pagedown\n");
    assert_eq!(active_tab_index(&session), 1, "Ctrl+PageDown on the PDF tab switches tabs");
    run_script(&mut session, b"key ctrl+pagedown\n");
    assert_eq!(active_tab_index(&session), 0, "Ctrl+PageDown wraps back to the PDF tab");
    run_script(&mut session, b"key ctrl+pageup\n");
    assert_eq!(active_tab_index(&session), 1, "Ctrl+PageUp on the PDF tab switches tabs");
    run_script(&mut session, b"key ctrl+pageup\n");
    assert_eq!(active_tab_index(&session), 0);
    run_script(&mut session, b"key f1\n");
    assert_eq!(dump(&mut session)["overlays"]["settings"], true, "F1 opens settings over a PDF tab");
    run_script(&mut session, b"key f1\n");
    assert_eq!(dump(&mut session)["overlays"]["settings"], false);
    let fps = |session: &mut HeadlessSession| session.app.show_fps;
    let fps_before = fps(&mut session);
    run_script(&mut session, b"key f8\n");
    assert_ne!(fps(&mut session), fps_before, "F8 toggles the FPS overlay over a PDF tab");
    run_script(&mut session, b"key ctrl+4\nsettle 500\n");
    let state = dump(&mut session);
    assert_eq!(state["tabs"].as_array().unwrap().len(), 1, "Ctrl+4 closes the PDF tab: {state}");
    assert_eq!(state["tabs"][0]["kind"], "normal");
    assert_eq!(std::fs::read(&path).expect("fixture PDF"), before);
    let _ = std::fs::remove_dir_all(dir);
}
