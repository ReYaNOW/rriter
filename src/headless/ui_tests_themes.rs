//! Theme selection, persistence, and render generation regressions.

use crate::headless::tests_support::{
    click_ui, dump, open_settings_tab, run_script, scratch_dir, session_for_test, wait_until,
};
use crate::theme::ThemeId;

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

fn switch_to_tab(session: &mut crate::headless::HeadlessSession, tab: usize) {
    click_ui(session, &format!("EditorTab({tab})"));
    wait_until(session, 5000, "switched tab highlight", |session| {
        session.app.active_tab == tab && session.app.is_highlight_complete
    });
}

fn theme_shot_path(name: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new("/tmp/rriter-themes");
    std::fs::create_dir_all(dir).expect("create theme screenshot directory");
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
