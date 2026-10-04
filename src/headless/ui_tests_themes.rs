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
    wait_until(&mut session, 5000, "inactive Python highlight", |session| {
        session.app.tabs.len() == 2 && session.app.tabs[1].is_highlight_complete
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
    let switched = run_script(&mut session, b"key ctrl+tab\n");
    assert!(switched.iter().all(|line| line == "ok"), "{switched:?}");
    assert_eq!(session.app.active_tab, 0);
    assert!(session.app.tabs[0]
        .spans
        .iter()
        .any(|span| span.role == crate::theme::SyntaxRole::KeywordControl));

    session.app.pdf_dark_pages = true;
    let changed_theme_gen = session.app.renderer.as_ref().expect("renderer").theme_gen;
    session.app.apply_themes(ThemeId::Sepia, ThemeId::Sepia);
    assert!(session.app.pdf_dark_pages);
    assert_eq!(session.app.renderer.as_ref().expect("renderer").theme_gen, changed_theme_gen);

    drop(session);
    let session = session_for_test(1280, 800);
    assert_eq!(session.app.editor_theme_id, ThemeId::Sepia);
    assert_eq!(session.app.ui_theme_id, ThemeId::Sepia);
}
