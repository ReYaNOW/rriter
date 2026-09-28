//! Headless UI coverage for the Settings Help tab.

use crate::headless::tests_support::{
    assert_ui_rect_inside_window, click_ui, dump, has_ui, open_settings_tab, run_script,
    sample_file, scratch_dir, session_for_test, ui_center, ui_rect, wait_until,
};
use crate::headless::HeadlessSession;
use std::path::{Path, PathBuf};

const TEST_SCALE: f32 = 4.0 / 3.0;

/// RGBA bytes of the Help text area left of the FAQ scrollbar, bottom 200 px of the track.
fn help_text_bottom_pixels(session: &mut HeadlessSession, path: &Path) -> Vec<u8> {
    let lines = run_script(
        session,
        format!("mouse_move 0 0\nscreenshot {}\n", path.display()).as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let image = image::open(path).expect("Help screenshot").to_rgba8();
    let [track_x, track_y, _, track_height] = ui_rect(&dump(session), "SettingsFaqScrollY");
    let bottom = ((track_y + track_height).floor() as u32).min(image.height());
    let top = bottom.saturating_sub(200).max(track_y.ceil() as u32);
    let left = (track_x - 800.0).max(0.0) as u32;
    let right = (track_x.max(0.0) as u32).min(image.width());
    let mut pixels = Vec::with_capacity(((right - left) * (bottom - top) * 4) as usize);
    for y in top..bottom {
        for x in left..right {
            pixels.extend_from_slice(&image.get_pixel(x, y).0);
        }
    }
    pixels
}

fn settings_help_session(name: &str, width: u32, height: u32) -> (PathBuf, HeadlessSession) {
    let dir = scratch_dir(name);
    let file = sample_file(&dir);
    let mut session = session_for_test(width, height);
    let lines = run_script(
        &mut session,
        format!("scale {TEST_SCALE}\nopen {}\n", file.display()).as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(&mut session, 5000, "sample file tab", |session| {
        session.app.file_path.as_deref() == Some(file.as_path())
    });

    open_settings_tab(&mut session, 4);
    assert_ui_rect_inside_window(&dump(&mut session), "SettingsTab(4)");
    wait_until(&mut session, 5000, "Help content and scrollbar", |session| {
        let state = dump(session);
        session.app.settings_tab == 4 && has_ui(&state, "SettingsFaqScrollY")
    });
    assert_ui_rect_inside_window(&dump(&mut session), "SettingsFaqScrollY");
    (dir, session)
}

fn settings_help_max_scroll(session: &mut HeadlessSession) -> f32 {
    let state = dump(session);
    let viewport_height = ui_rect(&state, "SettingsFaqScrollY")[3] as f32;
    let faq_editor = &session.app.faq_editor;
    session
        .app
        .renderer
        .as_mut()
        .expect("headless renderer")
        .get_faq_max_scroll(faq_editor, viewport_height)
}

fn wait_for_help_scroll(session: &mut HeadlessSession, expected: f32) {
    wait_until(session, 5000, "Help scroll animation", |session| {
        let scroll = &session.app.settings_scroll;
        scroll.is_settled() && (scroll.current - expected).abs() < 0.5
    });
}

#[test]
fn headless_settings_help_wheel_clamps_at_both_ends() {
    let (dir, mut session) = settings_help_session("ui-settings-help-wheel", 1280, 720);
    let max_scroll = settings_help_max_scroll(&mut session);
    assert!(max_scroll > 0.0, "Help content should scroll");
    assert_eq!(session.app.settings_scroll.current, 0.0);
    let top_pixels = help_text_bottom_pixels(&mut session, &dir.join("help-top.png"));

    let (x, y) = ui_center(&dump(&mut session), "SettingsFaqScrollY");
    let lines = run_script(
        &mut session,
        format!("mouse_move {x} {y}\nwheel 0 -100\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    wait_for_help_scroll(&mut session, max_scroll);
    assert!(session.app.settings_scroll.current >= 0.0);
    assert!(session.app.settings_scroll.current <= max_scroll);
    assert!((session.app.settings_scroll.current - max_scroll).abs() < 0.5);
    // The lower part of the Help area must show other rows at the bottom limit than at
    // the top, i.e. the scroll really brought the end of the FAQ into view.
    let bottom_pixels = help_text_bottom_pixels(&mut session, &dir.join("help-bottom.png"));
    assert_eq!(top_pixels.len(), bottom_pixels.len());
    let changed = top_pixels
        .chunks_exact(4)
        .zip(bottom_pixels.chunks_exact(4))
        .filter(|(top, bottom)| top != bottom)
        .count();
    assert!(
        changed * 50 > top_pixels.len() / 4,
        "Help rows at the bottom limit should differ from the top: {changed} changed pixels"
    );

    let (x, y) = ui_center(&dump(&mut session), "SettingsFaqScrollY");
    let lines = run_script(
        &mut session,
        format!("mouse_move {x} {y}\nwheel 0 100\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    wait_for_help_scroll(&mut session, 0.0);
    assert!(session.app.settings_scroll.current >= 0.0);
    assert_eq!(session.app.settings_scroll.current, 0.0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_settings_help_drag_scrollbar_clamps_at_bottom() {
    let (dir, mut session) = settings_help_session("ui-settings-help-drag", 1280, 720);
    let max_scroll = settings_help_max_scroll(&mut session);
    let [track_x, track_y, track_width, track_height] =
        ui_rect(&dump(&mut session), "SettingsFaqScrollY");
    let x = track_x + track_width / 2.0;
    let start_y = track_y + 10.0;
    let bottom_y = track_y + track_height + 30.0;
    let lines = run_script(
        &mut session,
        format!(
            "mouse_move {x} {start_y}\nclick down\nmouse_move {x} {bottom_y}\nclick up\n"
        )
        .as_bytes(),
    );
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    wait_for_help_scroll(&mut session, max_scroll);
    assert!(session.app.settings_scroll.current > 0.0);
    assert!(session.app.settings_scroll.current <= max_scroll);
    assert!((session.app.settings_scroll.current - max_scroll).abs() < 0.5);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_settings_help_scroll_survives_tab_switch() {
    let (dir, mut session) = settings_help_session("ui-settings-help-tabs", 1280, 720);
    let max_scroll = settings_help_max_scroll(&mut session);
    let (x, y) = ui_center(&dump(&mut session), "SettingsFaqScrollY");
    let lines = run_script(
        &mut session,
        format!("mouse_move {x} {y}\nwheel 0 -100\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    wait_for_help_scroll(&mut session, max_scroll);
    let before_switch = session.app.settings_scroll.current;

    click_ui(&mut session, "SettingsTab(1)");
    wait_until(&mut session, 5000, "General settings tab", |session| {
        let state = dump(session);
        session.app.settings_tab == 1 && has_ui(&state, "SettingsRefreshTools")
    });
    // Sub-pixel rounding of the scroll position on the tab switch is allowed.
    assert!(
        (session.app.settings_scroll.current - before_switch).abs() < 0.5,
        "scroll {} changed from {before_switch}",
        session.app.settings_scroll.current
    );

    click_ui(&mut session, "SettingsTab(4)");
    wait_until(&mut session, 5000, "Help settings tab restored", |session| {
        let state = dump(session);
        session.app.settings_tab == 4 && has_ui(&state, "SettingsFaqScrollY")
    });
    assert_ui_rect_inside_window(&dump(&mut session), "SettingsFaqScrollY");
    assert!((session.app.settings_scroll.current - before_switch).abs() < 0.5);
    assert!((session.app.settings_scroll.target - before_switch).abs() < 0.5);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_settings_help_escape_closes_overlay() {
    let (dir, mut session) = settings_help_session("ui-settings-help-escape", 1280, 720);
    let lines = run_script(&mut session, b"key escape\n");
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    assert_eq!(dump(&mut session)["overlays"]["settings"], false);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_settings_help_large_window_content_stays_inside_panel() {
    const WIDTH: u32 = 2560;
    const HEIGHT: u32 = 1440;
    let (dir, mut session) = settings_help_session("ui-settings-help-large", WIDTH, HEIGHT);
    let state = dump(&mut session);
    let layout = crate::render_view::settings_ui::settings_modal_layout(
        WIDTH as f32,
        HEIGHT as f32,
        TEST_SCALE,
    );
    let [scroll_x, scroll_y, scroll_width, scroll_height] =
        ui_rect(&state, "SettingsFaqScrollY");
    assert!(scroll_x >= layout.inner.x as f64);
    assert!(scroll_y >= layout.inner.y as f64);
    assert!(scroll_x + scroll_width <= (layout.inner.x + layout.inner.w) as f64);
    assert!(scroll_y + scroll_height <= (layout.inner.y + layout.inner.h) as f64);

    let screenshot = dir.join("settings-help-large.png");
    let lines = run_script(
        &mut session,
        format!("screenshot {}\n", screenshot.display()).as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let image = image::open(&screenshot).expect("large Settings screenshot").to_rgba8();
    let outer = layout.outer;
    let left = outer.x.floor().max(0.0) as u32;
    let top = outer.y.floor().max(0.0) as u32;
    let right = (outer.x + outer.w).ceil().min(WIDTH as f32) as u32;
    let bottom = (outer.y + outer.h).ceil().min(HEIGHT as f32) as u32;
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            if x >= left && x < right && y >= top && y < bottom {
                continue;
            }
            let pixel = image.get_pixel(x, y);
            assert!(
                pixel[0] <= 150 || pixel[1] <= 150 || pixel[2] <= 150,
                "bright Help content outside the Settings panel at ({x}, {y}): {pixel:?}"
            );
        }
    }
    let _ = std::fs::remove_dir_all(dir);
}
