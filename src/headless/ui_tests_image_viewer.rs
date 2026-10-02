use crate::headless::HeadlessSession;
use crate::app::{PendingAction, events::host_loop::HostLoop};
use crate::headless::tests_support::{click_ui, dump, run_script, scratch_dir, session_for_test, shell_failed, terminal_has_line, terminal_session, ui_rect, wait_until, workspace_with_explorer};
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
    let body = ui_rect(&dump(&mut session), "PdfBody");
    let before = session.app.tabs[0].image.as_ref().unwrap().fit_scale(body[2] as f32, body[3] as f32);
    session.app.left_shift_down = true;
    let lines = run_script(
        &mut session,
        format!("mouse_move {} {}\nwheel 0 4\n", body[0] + body[2] / 2.0, body[1] + body[3] / 2.0).as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let zoomed = dump(&mut session)["tabs"][0]["image"]["zoom"].as_f64().unwrap_or(0.0);
    assert!(zoomed > f64::from(before), "left shift should zoom: {before} -> {zoomed}");
    session.app.left_shift_down = false;
    session.app.modifiers = winit::keyboard::ModifiersState::SHIFT;
    run_script(
        &mut session,
        format!("mouse_move {} {}\nwheel 0 4\n", body[0] + body[2] / 2.0, body[1] + body[3] / 2.0).as_bytes(),
    );
    let after_right_shift = dump(&mut session)["tabs"][0]["image"]["zoom"].as_f64().unwrap_or(0.0);
    assert_eq!(after_right_shift, zoomed, "aggregate Shift must not zoom image");
    run_script(&mut session, b"key 0\n");
    let fit = dump(&mut session)["tabs"][0]["image"]["zoom"].as_f64().unwrap_or(0.0);
    assert!((fit - f64::from(before)).abs() <= f64::EPSILON, "0 should restore first-open fit {before}: {fit}");
}

#[test]
fn image_zoom_anchor_and_vertical_scroll_reach_both_edges() {
    let (_dir, _path, mut session) = open_image("ui-image-geometry", "png", None);
    wait_phase(&mut session, "ready");
    let [bx, by, bw, bh] = ui_rect(&dump(&mut session), "PdfBody").map(|value| value as f32);
    let image = session.app.tabs[0].image.as_deref_mut().expect("image state");
    image.body = (bx, by, bw, bh);
    let old_zoom = image.fit_scale(bw, bh);
    let natural = image.natural;
    let cursor = (bx + bw * 0.37, by + bh * 0.62);
    let old_origin = crate::app::image_tab::image_origin((bw, bh), (natural.0 * old_zoom, natural.1 * old_zoom));
    let image_point = (
        (cursor.0 - bx - old_origin.0 - image.offset.0) / old_zoom,
        (cursor.1 - by - old_origin.1 - image.offset.1) / old_zoom,
    );
    image.zoom_at(2.0, cursor.0, cursor.1);
    let new_origin = crate::app::image_tab::image_origin((bw, bh), (natural.0 * image.zoom, natural.1 * image.zoom));
    let anchored = (
        bx + new_origin.0 + image.offset.0 + image_point.0 * image.zoom,
        by + new_origin.1 + image.offset.1 + image_point.1 * image.zoom,
    );
    assert!(
        (anchored.0 - cursor.0).abs() <= 1.0 && (anchored.1 - cursor.1).abs() <= 1.0,
        "anchor drift: cursor={cursor:?}, anchored={anchored:?}, body={:?}, natural={natural:?}, old_origin={old_origin:?}, new_origin={new_origin:?}, offset={:?}, old_zoom={old_zoom}, zoom={}",
        image.body,
        image.offset,
        image.zoom,
    );

    image.scroll(-100_000.0);
    assert!((image.offset.1 - (bh - 1000.0 * image.zoom)).abs() <= 1.0);
    image.scroll(100_000.0);
    assert!(image.offset.1.abs() <= f32::EPSILON);
}

#[test]
fn image_tab_passes_keys_to_terminal_and_escape_closes_dialog() {
    let (dir, mut session) = terminal_session("ui-image-key-routing");
    let path = dir.join("sample.png");
    png(&path, 80, 60);
    run_script(&mut session, format!("open {}\n", path.display()).as_bytes());
    wait_phase(&mut session, "ready");
    session.app.show_action_dialog(&HostLoop::headless(&session.loop_state), PendingAction::Quit);
    assert_eq!(dump(&mut session)["dialog"]["action"], "Quit");
    run_script(&mut session, b"key escape\n");
    assert_eq!(dump(&mut session)["dialog"], serde_json::Value::Null);

    if shell_failed(&session, 0) {
        let _ = std::fs::remove_dir_all(dir);
        return;
    }
    click_ui(&mut session, "TerminalBody");
    assert!(session.app.ide_panel.terminal_focused);
    let lines = run_script(&mut session, b"type echo image-key-route\nkey enter\n");
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    wait_until(&mut session, WAIT_MS, "terminal command from image tab", |session| {
        terminal_has_line(session, 0, "image-key-route")
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn wheel_over_file_tree_does_not_zoom_or_scroll_image() {
    let dir = scratch_dir("ui-image-tree-wheel");
    for index in 0..80 {
        std::fs::write(dir.join(format!("item-{index:02}.txt")), "tree row\n")
            .unwrap_or_else(|error| panic!("write tree fixture: {error}"));
    }
    let path = dir.join("sample.png");
    png(&path, 1600, 1000);
    let mut session = workspace_with_explorer(1280, 720, 1.0, &dir);
    run_script(&mut session, format!("open {}\n", path.display()).as_bytes());
    wait_phase(&mut session, "ready");
    let tree_bar = ui_rect(&dump(&mut session), "FileTreeScrollY");
    let tree_point = (tree_bar[0] - 30.0, tree_bar[1] + tree_bar[3] * 0.5);
    let image_before = dump(&mut session)["tabs"][0]["image"].clone();
    let lines = run_script(
        &mut session,
        format!("mouse_move {} {}\nwheel 0 -100\n", tree_point.0, tree_point.1).as_bytes(),
    );
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    assert!(session.app.ide_panel.explorer_scroll.target > 0.0, "tree did not scroll");
    let image_after = dump(&mut session)["tabs"][0]["image"].clone();
    assert_eq!(image_after["zoom"], image_before["zoom"]);
    assert_eq!(session.app.tabs[0].image.as_ref().unwrap().offset, (0.0, 0.0));
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

#[test]
fn image_texture_tracks_the_active_tab_and_is_released_for_text_tabs() {
    let (_dir, image_path, mut session) = open_image("ui-image-tab-lifecycle", "png", None);
    wait_phase(&mut session, "ready");
    let text_path = image_path.with_extension("txt");
    std::fs::write(&text_path, "neighbour\n").expect("write neighbour");
    run_script(&mut session, format!("open {}\n", text_path.display()).as_bytes());
    wait_until(&mut session, WAIT_MS, "image texture released", |session| {
        dump(session)["tabs"].as_array().is_some_and(|tabs| tabs[0]["image"]["texture"] == false)
    });
    session.app.switch_to_tab(0);
    session.app.close_tab_at_unchecked(1);
    wait_until(&mut session, WAIT_MS, "active image texture restored", |session| {
        dump(session)["tabs"].as_array().is_some_and(|tabs| tabs[0]["image"]["phase"] == "ready" && tabs[0]["image"]["texture"] == true)
    });
}

#[test]
fn repeated_watcher_events_while_image_loads_do_not_replace_the_in_flight_load() {
    let (_dir, _path, mut session) = open_image("ui-image-repeated-events", "png", None);
    for _ in 0..8 {
        session.app.revalidate_image_tabs();
    }
    wait_phase(&mut session, "ready");
    wait_until(&mut session, WAIT_MS, "image texture ready after revalidation", |session| {
        dump(session)["tabs"].as_array().is_some_and(|tabs| tabs[0]["image"]["texture"] == true)
    });
}

#[test]
fn moving_without_a_pressed_button_does_not_pan_the_image() {
    let (_dir, _path, mut session) = open_image("ui-image-hover-pan", "png", None);
    wait_phase(&mut session, "ready");
    let [x, y, w, h] = ui_rect(&dump(&mut session), "PdfBody");
    session.app.tabs[0].image.as_deref_mut().expect("image state").zoom_at(2.0, (x + w / 2.0) as f32, (y + h / 2.0) as f32);
    let before = session.app.tabs[0].image.as_ref().expect("image state").offset;
    run_script(&mut session, format!("mouse_move {} {}\nmouse_move {} {}\n", x + w / 2.0, y + h / 2.0, x + w / 2.0 + 40.0, y + h / 2.0 + 40.0).as_bytes());
    assert_eq!(session.app.tabs[0].image.as_ref().expect("image state").offset, before);
}

#[test]
fn settings_and_bottom_panel_wheels_do_not_zoom_the_image() {
    let (_dir, _path, mut session) = open_image("ui-image-wheel-overlays", "png", None);
    wait_phase(&mut session, "ready");
    let body = ui_rect(&dump(&mut session), "PdfBody");
    let point = (body[0] + body[2] / 2.0, body[1] + body[3] / 2.0);
    let zoom = dump(&mut session)["tabs"][0]["image"]["zoom"].clone();
    session.app.show_settings = true;
    session.app.settings_tab = 1;
    session.app.settings_anim_progress = 1.0;
    run_script(&mut session, format!("mouse_move {} {}\nwheel 0 4\n", point.0, point.1).as_bytes());
    assert_eq!(dump(&mut session)["tabs"][0]["image"]["zoom"], zoom);
    session.app.show_settings = false;
    session.app.ide_panel.toggle(crate::app::PanelId::Problems);
    let bottom_y = body[1] + body[3] - 4.0;
    run_script(&mut session, format!("mouse_move {} {}\nwheel 0 4\n", point.0, bottom_y).as_bytes());
    assert_eq!(dump(&mut session)["tabs"][0]["image"]["zoom"], zoom);
}
