use crate::headless::HeadlessSession;
use crate::headless::tests_support::{dump, run_script, scratch_dir, session_for_test, wait_until};
use std::path::{Path, PathBuf};

const WAIT_MS: u64 = 8000;

fn png(path: &Path, width: u32, height: u32) {
    let pixels = image::RgbaImage::from_pixel(width, height, image::Rgba([90, 140, 210, 255]));
    pixels.save_with_format(path, image::ImageFormat::Png)
        .unwrap_or_else(|error| panic!("write image fixture: {error}"));
}

fn open_image(name: &str, ext: &str, bytes: Option<&[u8]>) -> (PathBuf, PathBuf, HeadlessSession) {
    let dir = scratch_dir(name);
    let path = dir.join(format!("sample.{ext}"));
    if let Some(bytes) = bytes {
        std::fs::write(&path, bytes).unwrap_or_else(|error| panic!("write fixture: {error}"));
    } else {
        png(&path, 1600, 1000);
    }
    let mut session = session_for_test(1280, 720);
    let lines = run_script(
        &mut session,
        format!("workspace {}\nopen {}\n", dir.display(), path.display()).as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    (dir, path, session)
}

fn wait_phase(session: &mut HeadlessSession, phase: &str) {
    wait_until(session, WAIT_MS, "image tab phase", |session| {
        dump(session)["tabs"].as_array().and_then(|tabs| tabs.last()).is_some_and(|tab| tab["kind"] == "image" && tab["image"]["phase"] == phase)
    });
}

#[test]
fn png_opens_in_a_ready_image_tab_without_marking_text_modified() {
    let (_dir, path, mut session) = open_image("ui-image-open", "png", None);
    wait_phase(&mut session, "ready");
    let state = dump(&mut session);
    let tab = state["tabs"].as_array().and_then(|tabs| tabs.last()).expect("image tab");
    assert_eq!(tab["kind"], "image");
    assert_eq!(tab["image"]["natural_w"], 1600.0);
    assert_eq!(tab["image"]["natural_h"], 1000.0);
    assert_eq!(tab["modified"], false);
    assert_eq!(tab["path"], path.display().to_string());
    assert!(tab["image"]["texture"].as_bool().unwrap_or(false));
}

#[test]
fn truncated_png_bytes_show_a_failed_image_phase() {
    let (_dir, _path, mut session) = open_image("ui-image-corrupt", "png", Some(b"\x89PNG\r\n\x1a\ncut"));
    wait_phase(&mut session, "failed");
    assert_eq!(dump(&mut session)["tabs"].as_array().unwrap().last().unwrap()["image"]["phase"], "failed");
}

#[test]
fn non_image_contents_with_png_extension_fail_instead_of_becoming_text() {
    let (_dir, _path, mut session) = open_image("ui-image-wrong-type", "png", Some(b"this is not an image"));
    wait_phase(&mut session, "failed");
    assert_eq!(dump(&mut session)["tabs"].as_array().unwrap().last().unwrap()["kind"], "image");
}

#[test]
fn deleting_an_open_image_reloads_to_failed() {
    let (_dir, path, mut session) = open_image("ui-image-deleted", "png", None);
    wait_phase(&mut session, "ready");
    std::fs::remove_file(&path).unwrap_or_else(|error| panic!("delete fixture: {error}"));
    session.app.revalidate_image_tabs();
    wait_phase(&mut session, "failed");
}

#[test]
fn svg_uses_the_shared_media_decoder() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="120" height="60"><rect width="120" height="60" fill="#2878c8"/></svg>"##;
    let (_dir, _path, mut session) = open_image("ui-image-svg", "svg", Some(svg));
    wait_phase(&mut session, "ready");
    let image = &dump(&mut session)["tabs"][0]["image"];
    assert_eq!(image["natural_w"], 120.0);
    assert_eq!(image["natural_h"], 60.0);
}

#[test]
fn left_shift_wheel_zooms_but_right_shift_does_not() {
    let (_dir, _path, mut session) = open_image("ui-image-wheel", "png", None);
    wait_phase(&mut session, "ready");
    let before = dump(&mut session)["tabs"][0]["image"]["zoom"].as_f64().unwrap_or(0.0);
    session.app.left_shift_down = true;
    let lines = run_script(&mut session, b"wheel 0 -4\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let zoomed = dump(&mut session)["tabs"][0]["image"]["zoom"].as_f64().unwrap_or(0.0);
    assert!(zoomed > before, "left shift should zoom: {before} -> {zoomed}");
    session.app.left_shift_down = false;
    session.app.modifiers = winit::keyboard::ModifiersState::SHIFT;
    run_script(&mut session, b"wheel 0 -4\n");
    let after_right_shift = dump(&mut session)["tabs"][0]["image"]["zoom"].as_f64().unwrap_or(0.0);
    assert_eq!(after_right_shift, zoomed, "aggregate Shift must not zoom image");
    run_script(&mut session, b"key 0\n");
    let fit = dump(&mut session)["tabs"][0]["image"]["zoom"].as_f64().unwrap_or(0.0);
    assert!(fit > 0.0 && fit <= 1.0, "0 should restore fit-to-window: {fit}");
}

#[test]
fn overwriting_an_open_image_refreshes_its_natural_dimensions() {
    let (_dir, path, mut session) = open_image("ui-image-reload", "png", None);
    wait_phase(&mut session, "ready");
    png(&path, 320, 180);
    session.app.revalidate_image_tabs();
    wait_until(&mut session, WAIT_MS, "updated image dimensions", |session| {
        let tab = &dump(session)["tabs"][0]["image"];
        tab["phase"] == "ready" && tab["natural_w"] == 320.0 && tab["natural_h"] == 180.0
    });
}
