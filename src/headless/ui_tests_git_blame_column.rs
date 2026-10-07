use crate::headless::tests_support::{
    click_ui, dump, git, git_blame_fixture, run_script, wait_until, workspace_with_explorer,
};
use super::ui_tests_git_blame_inline::{open_and_wait, TEST_HEIGHT, TEST_SCALE, TEST_WIDTH};

#[test]
fn headless_git_blame_column_labels_blocks_modified_lines_and_reveals_commit() {
    let (root, file) = git_blame_fixture(&format!("ui-git-blame-column-{}", std::process::id()));
    std::fs::write(&file, "first updated\nsecond updated\nthird\nlocal line\n")
        .unwrap_or_else(|error| panic!("write blame column fixture: {error}"));
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &root);
    session.app.git_blame_inline = true;
    open_and_wait(&mut session, &file, 3);
    let closed_padding = session.app.renderer.as_ref().map_or(0.0, |renderer| renderer.left_padding);
    session.app.editor.git_blame.column_open = true;
    session.app.ensure_blame_for_active();
    let _ = run_script(&mut session, b"mouse_move 0 0\n");

    let state = dump(&mut session);
    assert!(state["blame_column"]["open"].as_bool().unwrap_or(false));
    let labels = state["blame_column"]["visible_labels"].as_array().unwrap();
    assert_eq!(labels.len(), 3, "each committed block gets a label, inserted line stays blank: {labels:?}");
    assert_eq!(labels.iter().map(|row| row["line"].as_u64().unwrap()).collect::<Vec<_>>(), [0, 1, 2]);
    let open_padding = session.app.renderer.as_ref().map_or(0.0, |renderer| renderer.left_padding);
    assert!(open_padding > closed_padding, "open blame column must widen the gutter");

    let row_id = "EditorBlameColumnRow(0)";
    let (x, y) = crate::headless::tests_support::ui_center(&state, row_id);
    let _ = run_script(&mut session, format!("mouse_move {x} {y}\n").as_bytes());
    wait_until(&mut session, 5000, "blame column popup full message", |session| {
        session.app.renderer.as_ref().is_some_and(|renderer| {
            renderer.git_blame_popup_details.as_ref().is_some_and(|details| details.message.is_some())
        })
    });
    let expected_oid = git(&root, &["blame", "--line-porcelain", "blame.txt"])
        .lines().next().and_then(|line| line.split_whitespace().next()).unwrap_or_default().to_owned();
    click_ui(&mut session, row_id);
    wait_until(&mut session, 30000, "blame column graph reveal", |session| {
        session.app.ide_panel.git.graph_reveal.highlight_oid() == Some(expected_oid.as_str())
    });

    session.app.editor.git_blame.column_open = false;
    let _ = run_script(&mut session, b"mouse_move 0 0\n");
    let closed = dump(&mut session);
    assert_eq!(closed["blame_column"]["open"], false);
    assert_eq!(session.app.renderer.as_ref().map_or(0.0, |renderer| renderer.left_padding), closed_padding);
    drop(session);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn headless_git_blame_column_state_is_per_tab_and_survives_file_reload() {
    let (root, file) = git_blame_fixture(&format!("ui-git-blame-column-tabs-{}", std::process::id()));
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &root);
    session.app.git_blame_inline = true;
    open_and_wait(&mut session, &file, 3);
    let original_tab = session.app.active_tab;
    session.app.editor.git_blame.column_open = true;

    let other = root.join("other.txt");
    std::fs::write(&other, "other tab\n").unwrap_or_else(|error| panic!("write other tab: {error}"));
    let lines = run_script(&mut session, format!("open {}\n", other.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert!(!session.app.editor.git_blame.column_open);
    session.app.switch_to_tab(original_tab);
    assert!(session.app.editor.git_blame.column_open);

    std::fs::write(&file, "first updated\nexternal reload\nthird\n")
        .unwrap_or_else(|error| panic!("write reload fixture: {error}"));
    wait_until(&mut session, 8000, "blame column external reload", |session| {
        session.app.editor.text_equals("first updated\nexternal reload\nthird\n")
    });
    assert!(session.app.editor.git_blame.column_open);
    drop(session);
    let _ = std::fs::remove_dir_all(root);
}
