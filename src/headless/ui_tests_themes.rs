//! Theme selection, persistence, and render generation regressions.

use crate::headless::tests_support::{
    api_client_session, click_ui, dump, open_settings_tab, run_script, scratch_dir,
    serve_api_spec, serve_http_responses, session_for_test, send_request, wait_until,
    wheel_until_visible,
};
use crate::theme::ThemeId;
use std::time::Duration;

#[test]
fn appearance_theme_pick_updates_dump_pdf_and_persists_between_sessions() {
    let root = crate::headless::tests_support::ensure_test_profile_root();
    let config_path = crate::platform::app_paths_for_root(&root).config.join("config.json");
    std::fs::create_dir_all(config_path.parent().expect("config parent"))
        .expect("create test config directory");
    std::fs::write(&config_path, "{}\n").expect("seed test config");

    let dir = scratch_dir("themes");
    let file = dir.join("main.py");
    let inactive_file = dir.join("inactive.py");
    std::fs::write(&file, "if True:\n    print('hello')\n").expect("write Python fixture");
    std::fs::write(&inactive_file, "if False:\n    print('inactive')\n")
        .expect("write inactive Python fixture");
    let mut session = session_for_test(1280, 800);
    let workspace = run_script(
        &mut session,
        format!("workspace {}\n", dir.display()).as_bytes(),
    );
    assert!(workspace.iter().all(|line| line.starts_with("ok")), "{workspace:?}");
    let opened = run_script(
        &mut session,
        format!("open {}\nsettle 2000\n", file.display()).as_bytes(),
    );
    assert!(opened.iter().all(|line| line.starts_with("ok")), "{opened:?}");
    wait_until(&mut session, 5000, "Python highlight", |session| {
        session.app.highlighter.is_complete
    });
    let opened = run_script(
        &mut session,
        format!("open {}\nsettle 2000\n", inactive_file.display()).as_bytes(),
    );
    assert!(opened.iter().all(|line| line.starts_with("ok")), "{opened:?}");
    wait_until(&mut session, 5000, "second Python tab highlight", |session| {
        session.app.tabs.len() == 2
            && session.app.active_tab == 1
            && session.app.is_highlight_complete
    });
    let highlight_version = session.app.highlighter.current_version;
    let initial_theme_gen = session.app.renderer.as_ref().expect("renderer").theme_gen;

    open_settings_tab(&mut session, 3);
    click_ui(&mut session, "SettingsThemePick(Both, Sepia)");
    let state = dump(&mut session);
    assert_eq!(state["themes"]["editor"], "sepia");
    assert_eq!(state["themes"]["ui"], "sepia");
    assert!(state["themes"]["linked"].as_bool().unwrap_or(false));
    assert!(!session.app.pdf_dark_pages);
    assert_eq!(session.app.highlighter.current_version, highlight_version);
    assert_eq!(
        session.app.renderer.as_ref().expect("renderer").theme_gen,
        initial_theme_gen + 1,
    );
    let closed_settings = run_script(&mut session, b"key escape\n");
    assert!(closed_settings.iter().all(|line| line == "ok"), "{closed_settings:?}");
    // The slide-out keeps dimming the editor until the animation reaches 0.
    wait_until(&mut session, 5000, "Settings overlay closed", |session| {
        dump(session)["overlays"]["settings"] == false && session.app.settings_anim_progress <= 0.0
    });
    // No default binding for ctrl+tab (tabs switch on mod+pagedown), so click the tab itself.
    switch_to_tab(&mut session, 0);
    assert!(session.app.highlighter.spans
        .iter()
        .any(|span| span.role == crate::theme::SyntaxRole::KeywordControl));
    assert_keyword_pixel(
        &mut session,
        &theme_shot_path("active-theme.png"),
        crate::theme::SyntaxPalette::for_id(ThemeId::Sepia)
            .color(crate::theme::SyntaxRole::KeywordControl),
    );

    switch_to_tab(&mut session, 1);
    assert!(session.app.highlighter.spans
        .iter()
        .any(|span| span.role == crate::theme::SyntaxRole::KeywordControl));
    assert_keyword_pixel(
        &mut session,
        &theme_shot_path("inactive-theme.png"),
        crate::theme::SyntaxPalette::for_id(ThemeId::Sepia)
            .color(crate::theme::SyntaxRole::KeywordControl),
    );

    let changed_theme_gen = session.app.renderer.as_ref().expect("renderer").theme_gen;
    session.app.apply_themes(ThemeId::Sepia, ThemeId::Sepia);
    assert_eq!(session.app.renderer.as_ref().expect("renderer").theme_gen, changed_theme_gen);

    session.app.pdf_dark_pages = true;
    session.app.set_theme_linked(false);
    session.app.apply_themes(ThemeId::Sepia, ThemeId::OneDark);
    assert!(session.app.pdf_dark_pages);
    assert_eq!(session.app.renderer.as_ref().expect("renderer").theme_gen, changed_theme_gen + 1);

    drop(session);
    let session = session_for_test(1280, 800);
    assert_eq!(session.app.editor_theme_id, ThemeId::Sepia);
    assert_eq!(session.app.ui_theme_id, ThemeId::OneDark);
}

#[test]
fn appearance_theme_link_can_split_and_rejoin_editor_and_ui_palettes() {
    let mut session = session_for_test(1280, 800);
    open_settings_tab(&mut session, 3);
    click_ui(&mut session, "SettingsThemeLinked");
    click_ui(&mut session, "SettingsThemePick(Ui, OneLight)");
    let state = dump(&mut session);
    assert_eq!(state["themes"]["editor"], "dracula");
    assert_eq!(state["themes"]["ui"], "one_light");
    assert_eq!(state["themes"]["linked"], false);
    let renderer = session.app.renderer.as_ref().expect("renderer");
    assert_eq!(renderer.theme.syntax, crate::theme::SyntaxPalette::for_id(ThemeId::Dracula));
    assert_eq!(renderer.ui.syntax, crate::theme::SyntaxPalette::for_id(ThemeId::OneLight));

    click_ui(&mut session, "SettingsThemeLinked");
    let state = dump(&mut session);
    assert_eq!(state["themes"]["editor"], "dracula");
    assert_eq!(state["themes"]["ui"], "dracula");
    assert_eq!(state["themes"]["linked"], true);
}

#[test]
fn appearance_api_response_uses_dracula_ui_palette_with_one_light_editor() {
    let (dir, mut session) = api_response_session(
        "themes-api-dracula-ui",
        ThemeId::OneLight,
        ThemeId::Dracula,
    );
    assert_response_surface_colors(&mut session, ThemeId::Dracula, "dracula-ui");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn appearance_api_response_uses_one_light_ui_palette_with_dracula_editor() {
    let (dir, mut session) = api_response_session(
        "themes-api-one-light-ui",
        ThemeId::Dracula,
        ThemeId::OneLight,
    );
    assert_response_surface_colors(&mut session, ThemeId::OneLight, "one-light-ui");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn appearance_api_response_repaints_after_ui_theme_change() {
    let (dir, mut session) = api_response_session(
        "themes-api-ui-change",
        ThemeId::OneLight,
        ThemeId::Dracula,
    );
    assert_response_surface_colors(&mut session, ThemeId::Dracula, "before-ui-change");

    session.app.apply_themes(ThemeId::OneLight, ThemeId::OneLight);
    assert_response_surface_colors(&mut session, ThemeId::OneLight, "after-ui-change");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn appearance_light_dark_ui_switch_starts_icon_prewarm_only_when_variant_changes() {
    let dir = scratch_dir("themes-icon-prewarm");
    let mut session = session_for_test(1280, 800);
    session.app.ide_workspaces = vec![dir.clone()];
    session.app.set_theme_linked(false);
    session.app.apply_themes(ThemeId::Dracula, ThemeId::Dracula);
    session.app.file_tree_rx = None;

    // Dark to dark: the icon variant is unchanged, no rescan.
    session.app.apply_themes(ThemeId::Dracula, ThemeId::OneDark);
    assert!(session.app.file_tree_rx.is_none());

    // Dark to light: the new variant is prewarmed in the background.
    session.app.apply_themes(ThemeId::Dracula, ThemeId::OneLight);
    assert!(session.app.file_tree_rx.is_some());
    let _ = std::fs::remove_dir_all(dir);
}

fn api_response_session(
    name: &str,
    editor_theme: ThemeId,
    ui_theme: ThemeId,
) -> (std::path::PathBuf, crate::headless::HeadlessSession) {
    let (server, _) = serve_http_responses(
        "/get",
        vec![(200, "OK", "", r#"{"active":false}"#.to_string(), Duration::ZERO)],
    );
    let spec = serve_api_spec(
        &server,
        serde_json::json!({
            "/get": {"get": {"responses": {"200": {"description": "ok"}}}}
        }),
    );
    let (dir, mut session) = api_client_session(name, &spec);
    assert!(wheel_until_visible(&mut session, (200.0, 500.0), "ApiRouteRow(0)", 24.0, 20));
    click_ui(&mut session, "ApiRouteRow(0)");
    wait_until(&mut session, 5000, "API response endpoint tab", |session| {
        session.app.active_api_tab().is_some_and(|(_, state)| state.route_idx == Some(0))
    });
    set_split_themes(&mut session, editor_theme, ui_theme);
    assert!(wheel_until_visible(&mut session, (800.0, 400.0), "ApiTryRequest", 24.0, 30));
    assert_api_button_theme_pixel(&mut session, name);
    send_request(&mut session, 0);
    wait_until(&mut session, 5000, "API response body", |session| {
        session.app.active_api_tab().is_some_and(|(_, state)| {
            state.response.as_ref().is_some_and(|response| response.body.contains("false"))
        })
    });
    assert!(wheel_until_visible(&mut session, (800.0, 400.0), "ApiResponseBody(0)", 24.0, 30));
    (dir, session)
}

fn set_split_themes(
    session: &mut crate::headless::HeadlessSession,
    editor: ThemeId,
    ui: ThemeId,
) {
    open_settings_tab(session, 3);
    click_ui(
        session,
        &format!("SettingsThemePick(Both, {})", ui.label().replace(' ', "")),
    );
    click_ui(session, "SettingsThemeLinked");
    if editor != ui {
        click_ui(
            session,
            &format!("SettingsThemePick(Editor, {})", editor.label().replace(' ', "")),
        );
    }
    let themes = dump(session)["themes"].clone();
    assert_eq!(themes["editor"], editor.key());
    assert_eq!(themes["ui"], ui.key());
    assert_eq!(themes["linked"], false);
    let lines = run_script(session, b"key escape\n");
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    wait_until(session, 5000, "Settings overlay closed", |session| {
        dump(session)["overlays"]["settings"] == false && session.app.settings_anim_progress <= 0.0
    });
}

fn assert_response_surface_colors(
    session: &mut crate::headless::HeadlessSession,
    ui_theme: ThemeId,
    name: &str,
) {
    let screenshot_path = theme_shot_path(name);
    let response_rect = crate::headless::tests_support::ui_rect(&dump(session), "ApiResponseBody(0)");
    let response = run_script(
        session,
        format!("screenshot {}\n", screenshot_path.display()).as_bytes(),
    );
    assert!(response.iter().all(|line| line.starts_with("ok")), "{response:?}");
    let image = image::open(&screenshot_path)
        .unwrap_or_else(|error| panic!("read API response screenshot: {error}"))
        .to_rgba8();
    let expected_token = crate::theme::SyntaxPalette::for_id(ui_theme)
        .color(crate::theme::SyntaxRole::KeywordControl);
    let ui_palette = crate::theme::UiPalette::for_id(ui_theme);
    let expected_background = ui_palette.roles[crate::theme::UiRole::BgInput as usize];
    // `false` is the JSON KeywordControl token; choose its closest framebuffer pixel within
    // the response widget, then require a nearby pixel to match that UI theme's input surface.
    let token = closest_pixel(&image, response_rect, expected_token);
    assert_color_near(token.0, expected_token, "KeywordControl token");
    let background = closest_neighbor(&image, token.1, expected_background);
    assert_color_near(background.0, expected_background, "response surface background");

    let panel_toggle = run_script(session, b"key alt+w\n");
    assert!(panel_toggle.iter().all(|line| line == "ok"), "{panel_toggle:?}");
    let panel_rect = crate::headless::tests_support::ui_rect(&dump(session), "BottomPanelBody");
    let panel_shot_path = theme_shot_path(&format!("{name}-problems-panel"));
    let response = run_script(
        session,
        format!("screenshot {}\n", panel_shot_path.display()).as_bytes(),
    );
    assert!(response.iter().all(|line| line.starts_with("ok")), "{response:?}");
    let panel_image = image::open(&panel_shot_path)
        .unwrap_or_else(|error| panic!("read Problems panel screenshot: {error}"))
        .to_rgba8();
    let ui_theme = session.app.ui_theme_id;
    let editor_theme = crate::renderer::Theme::for_id(
        session.app.editor_theme_id,
        session.app.system_selection,
    );
    let ui_theme_values = crate::renderer::Theme::for_id(ui_theme, session.app.system_selection);
    let ui_palette = crate::theme::UiPalette::for_id(ui_theme);
    let panel_alpha = 0.8;
    let expected_tab_bar = ui_palette.pick(
        crate::theme::UiRole::BgPanel,
        [
            (ui_theme_values.bg[0] + 0.07).min(1.0),
            (ui_theme_values.bg[1] + 0.07).min(1.0),
            (ui_theme_values.bg[2] + 0.08).min(1.0),
            panel_alpha,
        ],
    );
    let legacy_tab_bar = [
        (editor_theme.bg[0] + 0.07).min(1.0),
        (editor_theme.bg[1] + 0.07).min(1.0),
        (editor_theme.bg[2] + 0.08).min(1.0),
        panel_alpha,
    ];
    let bottom_panel_bg = alpha_over(ui_theme_values.terminal_bg, editor_theme.surface_bg);
    let expected_tab_bar_pixel = alpha_over(expected_tab_bar, bottom_panel_bg);
    assert_rgb_differs(legacy_tab_bar, expected_tab_bar_pixel, "editor theme tab bar fallback");
    let sample = (
        (panel_rect[0] + panel_rect[2] - 12.0).clamp(0.0, panel_image.width() as f64 - 1.0) as u32,
        (panel_rect[1] + 12.0).clamp(0.0, panel_image.height() as f64 - 1.0) as u32,
    );
    let actual_tab_bar = pixel_rgb(&panel_image, sample);
    assert_color_near_with_tolerance(
        actual_tab_bar,
        expected_tab_bar_pixel,
        12,
        "Problems bottom tab bar",
    );
    let panel_toggle = run_script(session, b"key alt+w\n");
    assert!(panel_toggle.iter().all(|line| line == "ok"), "{panel_toggle:?}");
    eprintln!(
        "{name}: KeywordControl RGB={:?}, nearby background RGB={:?}, Problems tab bar RGB={actual_tab_bar:?}, expected RGB={:?}, editor fallback RGB={:?}",
        token.0,
        background.0,
        rgb8(expected_tab_bar_pixel),
        rgb8(legacy_tab_bar),
    );
}

fn assert_api_button_theme_pixel(session: &mut crate::headless::HeadlessSession, name: &str) {
    let button_rect = crate::headless::tests_support::ui_rect(&dump(session), "ApiTryRequest");
    let screenshot_path = theme_shot_path(&format!("{name}-api-button"));
    let response = run_script(
        session,
        format!("screenshot {}\n", screenshot_path.display()).as_bytes(),
    );
    assert!(response.iter().all(|line| line.starts_with("ok")), "{response:?}");
    let image = image::open(&screenshot_path)
        .unwrap_or_else(|error| panic!("read API button screenshot: {error}"))
        .to_rgba8();
    let ui_theme_values = crate::renderer::Theme::for_id(
        session.app.ui_theme_id,
        session.app.system_selection,
    );
    let editor_theme = crate::renderer::Theme::for_id(
        session.app.editor_theme_id,
        session.app.system_selection,
    );
    let ui_palette = crate::theme::UiPalette::for_id(session.app.ui_theme_id);
    let expected = ui_palette.pick(crate::theme::UiRole::TextPrimary, ui_theme_values.fg);
    assert_rgb_differs(editor_theme.fg, expected, "editor theme button text fallback");
    let (actual, _) = closest_pixel(&image, button_rect, expected);
    assert_color_near(actual, expected, "API request button icon and label");
    eprintln!(
        "{name}: API request button RGB={actual:?}, expected RGB={:?}, editor theme fallback RGB={:?}",
        rgb8(expected),
        rgb8(editor_theme.fg),
    );
}

fn alpha_over(foreground: [f32; 4], background: [f32; 4]) -> [f32; 4] {
    let alpha = foreground[3];
    [
        foreground[0] * alpha + background[0] * (1.0 - alpha),
        foreground[1] * alpha + background[1] * (1.0 - alpha),
        foreground[2] * alpha + background[2] * (1.0 - alpha),
        1.0,
    ]
}

fn pixel_rgb(image: &image::RgbaImage, (x, y): (u32, u32)) -> [u8; 3] {
    let pixel = image.get_pixel(x, y).0;
    [pixel[0], pixel[1], pixel[2]]
}

fn assert_rgb_differs(actual: [f32; 4], expected: [f32; 4], label: &str) {
    let actual = rgb8(actual);
    let expected = rgb8(expected);
    let distance: u16 = actual
        .iter()
        .zip(expected)
        .map(|(&actual, expected)| u16::from(actual.abs_diff(expected)))
        .sum();
    assert!(distance > 2, "{label} RGB {actual:?} was not distinguishable from {expected:?}");
}

fn closest_pixel(
    image: &image::RgbaImage,
    rect: [f64; 4],
    expected: [f32; 4],
) -> ([u8; 3], (u32, u32)) {
    let [x, y, width, height] = rect;
    let x0 = x.max(0.0) as u32;
    let y0 = y.max(0.0) as u32;
    let x1 = (x + width).min(f64::from(image.width())) as u32;
    let y1 = (y + height).min(f64::from(image.height())) as u32;
    let expected = rgb8(expected);
    let mut closest = ([0; 3], (x0, y0), u16::MAX);
    for py in y0..y1 {
        for px in x0..x1 {
            let pixel = image.get_pixel(px, py).0;
            let rgb = [pixel[0], pixel[1], pixel[2]];
            let distance = rgb
                .iter()
                .zip(expected)
                .map(|(&actual, expected)| u16::from(actual.abs_diff(expected)))
                .sum();
            if distance < closest.2 {
                closest = (rgb, (px, py), distance);
            }
        }
    }
    (closest.0, closest.1)
}

fn closest_neighbor(
    image: &image::RgbaImage,
    center: (u32, u32),
    expected: [f32; 4],
) -> ([u8; 3], (u32, u32)) {
    let expected = rgb8(expected);
    let mut closest = ([0; 3], center, u16::MAX);
    let (cx, cy) = center;
    for y in cy.saturating_sub(12)..=(cy + 12).min(image.height() - 1) {
        for x in cx.saturating_sub(12)..=(cx + 12).min(image.width() - 1) {
            let pixel = image.get_pixel(x, y).0;
            let rgb = [pixel[0], pixel[1], pixel[2]];
            let distance = rgb
                .iter()
                .zip(expected)
                .map(|(&actual, expected)| u16::from(actual.abs_diff(expected)))
                .sum();
            if distance < closest.2 {
                closest = (rgb, (x, y), distance);
            }
        }
    }
    (closest.0, closest.1)
}

fn rgb8(color: [f32; 4]) -> [u8; 3] {
    [color[0], color[1], color[2]].map(|channel| (channel * 255.0).round() as u8)
}

fn assert_color_near(actual: [u8; 3], expected: [f32; 4], label: &str) {
    assert_color_near_with_tolerance(actual, expected, 2, label);
}

fn assert_color_near_with_tolerance(
    actual: [u8; 3],
    expected: [f32; 4],
    tolerance: u8,
    label: &str,
) {
    let expected = rgb8(expected);
    assert!(
        actual.iter().zip(expected).all(|(&actual, expected)| actual.abs_diff(expected) <= tolerance),
        "{label} RGB {actual:?} did not match expected {expected:?} within {tolerance}/255 per channel",
    );
}

fn switch_to_tab(session: &mut crate::headless::HeadlessSession, tab: usize) {
    click_ui(session, &format!("EditorTab({tab})"));
    wait_until(session, 5000, "switched tab highlight", |session| {
        session.app.active_tab == tab && session.app.is_highlight_complete
    });
}

fn theme_shot_path(name: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new("/tmp/rriter-themes");
    std::fs::create_dir_all(dir).expect("create theme screenshot directory");
    let name = if name.ends_with(".png") { name.to_string() } else { format!("{name}.png") };
    dir.join(format!("{}-{name}", std::process::id()))
}

fn assert_keyword_pixel(
    session: &mut crate::headless::HeadlessSession,
    screenshot_path: &std::path::Path,
    expected: [f32; 4],
) {
    let response = run_script(
        session,
        format!("screenshot {}\n", screenshot_path.display()).as_bytes(),
    );
    assert!(response.iter().all(|line| line.starts_with("ok")), "{response:?}");
    let screenshot = image::open(screenshot_path)
        .unwrap_or_else(|error| panic!("read theme screenshot: {error}"))
        .to_rgba8();
    let expected = expected.map(|channel| (channel * 255.0).round() as i16);
    assert!(
        screenshot.pixels().any(|pixel| {
            (0..3).all(|channel| (i16::from(pixel.0[channel]) - expected[channel]).abs() <= 8)
        }),
        "keyword color {expected:?} missing from {}",
        screenshot_path.display(),
    );
}
