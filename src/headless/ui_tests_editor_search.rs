use crate::headless::tests_support::{
    click_ui, dump, has_ui, open_file_session, run_script, scratch_dir,
};

fn open_search_session(name: &str, contents: &str) -> (std::path::PathBuf, crate::headless::HeadlessSession) {
    let dir = scratch_dir(name);
    let file = dir.join("search.txt");
    std::fs::write(&file, contents).unwrap();
    let mut session = open_file_session(1280, 720, 4.0 / 3.0, &file);
    let lines = run_script(&mut session, b"key ctrl+f\ntype needle\nsettle\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    (dir, session)
}

#[test]
fn headless_editor_search_prev_next_buttons_and_shift_enter_wrap() {
    let (dir, mut session) = open_search_session(
        "ui-editor-search-navigation",
        "needle first\nother\nNEEDLE upper\nneedle last\n",
    );
    assert_eq!(session.app.search_results.len(), 3);
    assert_eq!(session.app.search_current_idx, Some(0));
    let initial = dump(&mut session);
    assert!(has_ui(&initial, "SearchPrev"), "{initial}");
    assert!(has_ui(&initial, "SearchNext"), "{initial}");

    click_ui(&mut session, "SearchNext");
    assert_eq!(session.app.search_current_idx, Some(1));
    assert_eq!(dump(&mut session)["tabs"][0]["cursor"]["line"], 3);
    click_ui(&mut session, "SearchNext");
    assert_eq!(session.app.search_current_idx, Some(2));
    assert_eq!(dump(&mut session)["tabs"][0]["cursor"]["line"], 4);
    click_ui(&mut session, "SearchNext");
    assert_eq!(session.app.search_current_idx, Some(0));
    assert_eq!(dump(&mut session)["tabs"][0]["cursor"]["line"], 1);
    click_ui(&mut session, "SearchPrev");
    assert_eq!(session.app.search_current_idx, Some(2));
    assert_eq!(dump(&mut session)["tabs"][0]["cursor"]["line"], 4);

    run_script(&mut session, b"key shift+enter\n");
    assert_eq!(session.app.search_current_idx, Some(1));
    assert_eq!(dump(&mut session)["tabs"][0]["cursor"]["line"], 3);
    run_script(&mut session, b"key shift+enter\n");
    assert_eq!(session.app.search_current_idx, Some(0));
    assert_eq!(dump(&mut session)["tabs"][0]["cursor"]["line"], 1);
    run_script(&mut session, b"key shift+enter\n");
    assert_eq!(session.app.search_current_idx, Some(2));
    assert_eq!(dump(&mut session)["tabs"][0]["cursor"]["line"], 4);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_search_case_toggle_changes_matches_and_position() {
    let (dir, mut session) = open_search_session(
        "ui-editor-search-case",
        "needle first\nother\nNEEDLE upper\nneedle last\n",
    );
    assert_eq!(session.app.search_results.len(), 3);

    click_ui(&mut session, "SearchNext");
    assert_eq!(dump(&mut session)["tabs"][0]["cursor"]["line"], 3);
    click_ui(&mut session, "SearchCaseToggle");
    assert!(session.app.search_case_sensitive);
    assert_eq!(session.app.search_results.len(), 2);
    assert_eq!(session.app.search_current_idx, Some(1));
    assert_eq!(dump(&mut session)["tabs"][0]["cursor"]["line"], 4);

    click_ui(&mut session, "SearchNext");
    assert_eq!(session.app.search_current_idx, Some(0));
    assert_eq!(dump(&mut session)["tabs"][0]["cursor"]["line"], 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_search_empty_and_no_match_leave_cursor_unchanged() {
    let (dir, mut session) = open_search_session(
        "ui-editor-search-empty",
        "needle first\nother\n",
    );
    assert_eq!(session.app.search_results.len(), 1);
    let original_cursor = dump(&mut session)["tabs"][0]["cursor"].clone();

    run_script(&mut session, b"key ctrl+a\ntype missing\n");
    assert!(session.app.search_results.is_empty());
    assert_eq!(session.app.search_current_idx, None);
    click_ui(&mut session, "SearchPrev");
    click_ui(&mut session, "SearchNext");
    assert_eq!(dump(&mut session)["tabs"][0]["cursor"], original_cursor);

    run_script(&mut session, b"key ctrl+a\nkey backspace\n");
    assert!(session.app.search_editor.get_full_text().is_empty());
    assert!(session.app.search_results.is_empty());
    assert_eq!(session.app.search_current_idx, None);
    assert_eq!(dump(&mut session)["tabs"][0]["cursor"], original_cursor);
    let _ = std::fs::remove_dir_all(dir);
}
