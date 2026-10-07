use crate::headless::tests_support::{
    click_ui, dump, git, git_blame_fixture, run_script, wait_until, workspace_with_explorer,
};
use super::ui_tests_git_blame_inline::{open_and_wait, TEST_HEIGHT, TEST_SCALE, TEST_WIDTH};

#[test]
fn headless_git_blame_gutter_menu_closes_on_escape() {
    let (root, file) = git_blame_fixture(&format!("ui-git-blame-escape-{}", std::process::id()));
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &root);
    let lines = run_script(&mut session, format!("open {}\n", file.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    session.app.open_git_blame_column_context_menu(40.0, 40.0);
    assert!(session.app.editor.git_blame.context_menu.is_some());
    let lines = run_script(&mut session, b"key escape\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert!(session.app.editor.git_blame.context_menu.is_none());
    drop(session);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn headless_git_blame_column_refreshes_after_closing_active_tab() {
    let (root, first_file) = git_blame_fixture(&format!("ui-git-blame-close-refresh-{}", std::process::id()));
    let second_file = root.join("second.txt");
    std::fs::write(&second_file, "old committed line\n").unwrap();
    git(&root, &["add", "second.txt"]);
    git(&root, &["commit", "-qm", "second file"]);
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &root);
    session.app.git_blame_inline = false;
    for file in [&second_file, &first_file] {
        let lines = run_script(&mut session, format!("open {}\n", file.display()).as_bytes());
        assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    }
    // Opening two files in a fresh workspace yields exactly [second.txt, blame.txt].
    assert_eq!(session.app.tabs.len(), 2);
    session.app.switch_to_tab(0);
    assert_eq!(session.app.file_path.as_deref(), Some(second_file.as_path()));
    session.app.editor.git_blame.column_open = true;
    session.app.ensure_blame_for_active();
    wait_until(&mut session, 8000, "initial second-file blame", |session| {
        session.app.editor.git_blame.current_for(session.app.editor.git_head.as_ref()).is_some()
    });
    session.app.switch_to_tab(1);
    assert_eq!(session.app.file_path.as_deref(), Some(first_file.as_path()));

    std::fs::write(&second_file, "new committed line\n").unwrap();
    git(&root, &["add", "second.txt"]);
    git(&root, &["commit", "-qm", "update second file"]);
    let expected_head = git(&root, &["rev-parse", "HEAD"]).trim().to_owned();
    session.app.close_tab_at(1);
    assert_eq!(session.app.active_tab, 0);
    assert_eq!(session.app.file_path.as_deref(), Some(second_file.as_path()));
    assert!(session.app.editor.git_blame.current_for(session.app.editor.git_head.as_ref()).is_none());
    let interim = dump(&mut session);
    assert!(interim["blame_column"]["visible_labels"].as_array().is_some_and(Vec::is_empty));
    wait_until(&mut session, 8000, "second-file blame for new HEAD", |session| {
        session.app.editor.git_head.as_ref().and_then(|head| head.head_oid).is_some_and(|oid| oid.to_string() == expected_head)
            && session.app.editor.git_blame.current_for(session.app.editor.git_head.as_ref()).is_some()
    });
    drop(session);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn headless_git_blame_snapshot_loss_hides_labels_for_dirty_buffer() {
    let (root, file) = git_blame_fixture(&format!("ui-git-blame-snapshot-loss-{}", std::process::id()));
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &root);
    session.app.git_blame_inline = false;
    open_column_and_wait(&mut session, &file);
    let lines = run_script(&mut session, b"key end\ntype dirty\nsettle 1000\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert!(session.app.editor.is_dirty());
    session.app.editor.set_git_head_snapshot(None, None);
    session.app.ensure_blame_for_active();
    assert!(session.app.editor.git_blame.blame.is_none());
    let _ = run_script(&mut session, b"mouse_move 0 0\n");
    let state = dump(&mut session);
    assert!(state["blame_column"]["visible_labels"].as_array().is_some_and(Vec::is_empty));
    drop(session);
    let _ = std::fs::remove_dir_all(root);
}

fn block_fixture(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let (root, file) = git_blame_fixture(name);
    std::fs::write(&file, "block alpha unique\nblock beta unique\nblock gamma unique\nblock delta unique\n")
        .unwrap_or_else(|error| panic!("write blame block fixture: {error}"));
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "single block"]);
    (root, file)
}

fn open_column_and_wait(
    session: &mut crate::headless::HeadlessSession,
    file: &std::path::Path,
) {
    let lines = run_script(session, format!("open {}\n", file.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    session.app.editor.git_blame.column_open = true;
    session.app.ensure_blame_for_active();
    wait_until(session, 8000, "column-only Git blame result", |session| {
        session.app.editor.git_blame.blame.is_some()
            || session.app.editor.git_blame.failed_key.is_some()
    });
    assert!(!session.app.git_blame_inline);
    assert!(session.app.editor.git_blame.blame.is_some());
}

#[test]
fn headless_git_blame_column_labels_blocks_modified_lines_and_reveals_commit() {
    let (root, file) = block_fixture(&format!("ui-git-blame-column-{}", std::process::id()));
    std::fs::write(&file, "local alpha\nblock beta unique\nblock gamma unique\nblock delta unique\nlocal line\n")
        .unwrap_or_else(|error| panic!("write blame column fixture: {error}"));
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &root);
    session.app.git_blame_inline = false;
    open_column_and_wait(&mut session, &file);
    session.app.editor.git_blame.column_open = false;
    let _ = run_script(&mut session, b"mouse_move 0 0\n");
    let closed_padding = session.app.renderer.as_ref().map_or(0.0, |renderer| renderer.left_padding);
    session.app.editor.git_blame.column_open = true;
    let _ = run_script(&mut session, b"mouse_move 0 0\n");

    let state = dump(&mut session);
    assert!(state["blame_column"]["open"].as_bool().unwrap_or(false));
    let labels = state["blame_column"]["visible_labels"].as_array().unwrap();
    assert_eq!(labels.len(), 1, "the modified line stays blank and the following multi-line block gets one label: {labels:?}");
    assert_eq!(labels[0]["line"], 1);
    let committed_rows = state["ui"].as_array().unwrap().iter().filter_map(|element| {
        let id = element["id"].as_str()?;
        id.strip_prefix("EditorBlameColumnRow(")?.strip_suffix(')')?.parse::<usize>().ok()
    }).collect::<Vec<_>>();
    assert_eq!(committed_rows, [1, 2, 3]);
    let column_rect = state["blame_column"]["rect"].as_array().unwrap();
    let column_right = column_rect[0].as_f64().unwrap() + column_rect[2].as_f64().unwrap();
    let digits_left = state["blame_column"]["line_number_digits_left"].as_f64().unwrap();
    assert!(column_right <= digits_left, "blame column must end before line-number digits: {column_right} > {digits_left}");
    let shot = run_script(&mut session, b"screenshot /tmp/rriter-t8f1-blame-column.png\n");
    assert!(shot.iter().all(|line| line.starts_with("ok")), "{shot:?}");
    let open_padding = session.app.renderer.as_ref().map_or(0.0, |renderer| renderer.left_padding);
    assert!(open_padding > closed_padding, "open blame column must widen the gutter");
    let cached_column_width = session.app.editor.git_blame.column_width;
    session.app.editor.git_blame.blame = None;
    session.app.ensure_blame_for_active();
    let _ = run_script(&mut session, b"mouse_move 0 0\n");
    assert_eq!(session.app.editor.git_blame.column_width, cached_column_width);
    assert_eq!(session.app.renderer.as_ref().map_or(0.0, |renderer| renderer.left_padding), open_padding);
    wait_until(&mut session, 8000, "blame result after column refresh", |session| {
        session.app.editor.git_blame.blame.is_some()
    });

    let row_id = "EditorBlameColumnRow(1)";
    let (x, y) = crate::headless::tests_support::ui_center(&state, row_id);
    let _ = run_script(&mut session, format!("mouse_move {x} {y}\n").as_bytes());
    wait_until(&mut session, 5000, "blame column popup full message", |session| {
        session.app.renderer.as_ref().is_some_and(|renderer| {
            renderer.git_blame_popup_details.as_ref().is_some_and(|details| details.message.is_some())
        })
    });
    let expected_oid = git(&root, &["blame", "-L", "2,2", "--line-porcelain", "blame.txt"])
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
fn headless_git_blame_column_does_not_repeat_label_after_deletion_in_block() {
    let (root, file) = block_fixture(&format!("ui-git-blame-column-delete-{}", std::process::id()));
    std::fs::write(&file, "block alpha unique\nblock gamma unique\nblock delta unique\n")
        .unwrap_or_else(|error| panic!("delete line in blame fixture: {error}"));
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &root);
    session.app.git_blame_inline = false;
    open_column_and_wait(&mut session, &file);
    let _ = run_script(&mut session, b"mouse_move 0 0\n");
    let labels = dump(&mut session)["blame_column"]["visible_labels"].as_array().unwrap().clone();
    assert_eq!(labels.iter().map(|row| row["line"].as_u64().unwrap()).collect::<Vec<_>>(), [0]);
    drop(session);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn headless_git_blame_column_truncates_long_non_ascii_author() {
    let (root, file) = block_fixture(&format!("ui-git-blame-column-author-{}", std::process::id()));
    std::fs::write(&file, "block alpha unique\nblock beta unique\nblock gamma unique\nblock delta unique\nlong author unique\n")
        .unwrap_or_else(|error| panic!("write long-author blame fixture: {error}"));
    git(&root, &["add", "."]);
    git(&root, &["-c", "user.name=非常に長い作者の名前を切り詰めます", "-c", "user.email=long@example.invalid", "commit", "-qm", "long author"]);
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &root);
    session.app.git_blame_inline = false;
    open_column_and_wait(&mut session, &file);
    let _ = run_script(&mut session, b"mouse_move 0 0\n");
    let state = dump(&mut session);
    let labels = state["blame_column"]["visible_labels"].as_array().unwrap();
    let label = labels.iter().find(|row| row["line"] == 4).unwrap()["label"].as_str().unwrap();
    assert!(label.chars().count() <= 24, "column label exceeded 24 chars: {label}");
    assert!(label.contains('非'), "expected the non-ASCII author in the label: {label}");
    let rect = state["blame_column"]["rect"].as_array().unwrap();
    let column_width = rect[2].as_f64().unwrap() as f32;
    let label_width = session.app.renderer.as_mut().map_or(0.0, |renderer| {
        renderer.measure_ui_width_at_pixel_size(label, 18.0 * TEST_SCALE * 0.82)
    });
    assert!(label_width <= column_width - 12.0 * TEST_SCALE + 1.0);
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

#[test]
fn headless_git_blame_gutter_menu_toggles_column_and_ignores_editor_text() {
    let (root, file) = block_fixture(&format!("ui-git-blame-gutter-menu-{}", std::process::id()));
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &root);
    session.app.git_blame_inline = false;
    let lines = run_script(&mut session, format!("open {}\n", file.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let _ = run_script(&mut session, b"mouse_move 0 0\n");
    let closed_padding = session.app.renderer.as_ref().map_or(0.0, |renderer| renderer.left_padding);

    let state = dump(&mut session);
    let (x, y) = crate::headless::tests_support::ui_center(&state, "EditorLineNumbers");
    let lines = run_script(&mut session, format!("mouse_move {x} {y}\nclick right\n").as_bytes());
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    assert!(session.app.editor.git_blame.context_menu.is_some());
    assert_eq!(session.app.editor.git_blame.column_context_menu_label(), "Показать Git blame");
    crate::headless::tests_support::click_ui(&mut session, "GitBlameColumnMenuItem");
    assert!(session.app.editor.git_blame.column_open);
    wait_until(&mut session, 8000, "gutter-menu blame result", |session| {
        session.app.editor.git_blame.blame.is_some()
            || session.app.editor.git_blame.failed_key.is_some()
    });
    assert!(session.app.editor.git_blame.blame.is_some());
    let _ = run_script(&mut session, b"mouse_move 0 0\n");
    let open_padding = session.app.renderer.as_ref().map_or(0.0, |renderer| renderer.left_padding);
    assert!(open_padding > closed_padding);

    let state = dump(&mut session);
    let (x, y) = crate::headless::tests_support::ui_center(&state, "EditorLineNumbers");
    let lines = run_script(&mut session, format!("mouse_move {x} {y}\nclick right\n").as_bytes());
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    assert_eq!(session.app.editor.git_blame.column_context_menu_label(), "Скрыть Git blame");
    crate::headless::tests_support::click_ui(&mut session, "GitBlameColumnMenuItem");
    assert!(!session.app.editor.git_blame.column_open);
    let _ = run_script(&mut session, b"mouse_move 0 0\n");
    assert_eq!(session.app.renderer.as_ref().map_or(0.0, |renderer| renderer.left_padding), closed_padding);

    let state = dump(&mut session);
    let (x, y) = crate::headless::tests_support::ui_center(&state, "EditorTextBody");
    let lines = run_script(&mut session, format!("mouse_move {x} {y}\nclick right\n").as_bytes());
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    assert!(session.app.editor.git_blame.context_menu.is_none());
    drop(session);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn headless_git_blame_gutter_menu_notifies_for_untracked_file() {
    let (root, _) = git_blame_fixture(&format!("ui-git-blame-gutter-untracked-{}", std::process::id()));
    let file = root.join("untracked.txt");
    std::fs::write(&file, "untracked line\n")
        .unwrap_or_else(|error| panic!("write untracked blame fixture: {error}"));
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &root);
    session.app.git_blame_inline = false;
    let lines = run_script(&mut session, format!("open {}\n", file.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let state = dump(&mut session);
    let (x, y) = crate::headless::tests_support::ui_center(&state, "EditorLineNumbers");
    let lines = run_script(&mut session, format!("mouse_move {x} {y}\nclick right\n").as_bytes());
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    crate::headless::tests_support::click_ui(&mut session, "GitBlameColumnMenuItem");
    assert!(session.app.editor.git_blame.column_open);
    assert_eq!(session.app.readonly_notice_text, "Git blame недоступен: файла нет в HEAD");
    drop(session);
    let _ = std::fs::remove_dir_all(root);
}
