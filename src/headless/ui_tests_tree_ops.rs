//! File Tree create/rename/move/delete and context-menu regressions.

use crate::headless::HeadlessSession;
use crate::headless::tests_support::{
    assert_ui_rect_inside_window, click_ui, dump, has_ui, run_script, scratch_dir, ui_center,
    ui_rect, workspace_with_explorer,
};
use serde_json::Value;
use std::path::PathBuf;

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;

fn tree_fixture(name: &str) -> PathBuf {
    let dir = scratch_dir(name);
    let nested = dir.join("alpha-folder").join("inside");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(nested.join("nested.txt"), "nested fixture\n").unwrap();
    std::fs::write(dir.join("zeta.txt"), "tree fixture\n").unwrap();
    dir
}

pub(crate) fn tree_node_count(state: &Value) -> usize {
    state["ui"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|element| element["id"].as_str().unwrap_or("").starts_with("FileTreeNode("))
        .count()
}

fn assert_context_menu(state: &Value, item_count: usize) {
    assert_eq!(state["overlays"]["context_menu"], true, "{state}");
    let actual_count = state["ui"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|element| element["id"].as_str().unwrap_or("").starts_with("FileTreeMenuItem("))
        .count();
    assert_eq!(actual_count, item_count, "{state}");
    for index in 0..item_count {
        let id = format!("FileTreeMenuItem({index})");
        assert!(has_ui(state, &id), "{id} missing from {state}");
        assert_ui_rect_inside_window(state, &id);
    }
}

fn open_context_menu_at(session: &mut HeadlessSession, x: f64, y: f64) {
    let lines = run_script(
        session,
        format!("mouse_move {x} {y}\nclick right\nwait 350\nmouse_move {x} {y}\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
}

pub(crate) fn open_context_menu_on_node(session: &mut HeadlessSession, id: &str) {
    let (x, y) = ui_center(&dump(session), id);
    open_context_menu_at(session, x, y);
}

fn assert_file_tree_dialog(state: &Value, kind: &str) {
    assert_eq!(state["overlays"]["file_tree_dialog"], kind, "{state}");
}

fn open_create_file_dialog(session: &mut HeadlessSession) {
    open_context_menu_on_node(session, "FileTreeNode(0)");
    assert_context_menu(&dump(session), 9);
    click_ui(session, "FileTreeMenuItem(0)");
    assert_file_tree_dialog(&dump(session), "create");
}

fn drag_nodes(session: &mut HeadlessSession, source_id: &str, target_id: &str) {
    let state = dump(session);
    let source = ui_rect(&state, source_id);
    let target = ui_rect(&state, target_id);
    let (start_x, start_y) = (source[0] + source[2] / 2.0, source[1] + source[3] / 2.0);
    let (target_x, target_y) = (target[0] + target[2] / 2.0, target[1] + target[3] / 2.0);
    let lines = run_script(
        session,
        format!(
            "mouse_move {start_x} {start_y}\nclick left down\nmouse_move {target_x} {target_y}\nclick left up\n"
        )
        .as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
}

fn drag_node_across_its_row(session: &mut HeadlessSession, id: &str) {
    let [x, y, width, height] = ui_rect(&dump(session), id);
    let start_x = x + 8.0;
    let end_x = x + width - 8.0;
    let row_y = y + height / 2.0;
    let lines = run_script(
        session,
        format!(
            "mouse_move {start_x} {row_y}\nclick left down\nmouse_move {end_x} {row_y}\nclick left up\n"
        )
        .as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
}

#[test]
fn headless_tree_context_menus_cover_file_folder_and_empty_space() {
    let dir = tree_fixture("ui-tree-ops-context");
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);

    open_context_menu_on_node(&mut session, "FileTreeNode(2)");
    assert_context_menu(&dump(&mut session), 9);
    run_script(&mut session, b"key escape\n");
    let closed = dump(&mut session);
    assert_eq!(closed["overlays"]["context_menu"], false);
    assert!(!has_ui(&closed, "FileTreeMenuItem(0)"));

    open_context_menu_on_node(&mut session, "FileTreeNode(1)");
    assert_context_menu(&dump(&mut session), 9);
    run_script(&mut session, b"mouse_move 900 680\nclick\n");
    let closed = dump(&mut session);
    assert_eq!(closed["overlays"]["context_menu"], false);
    assert!(!has_ui(&closed, "FileTreeMenuItem(0)"));

    open_context_menu_at(&mut session, 120.0, 600.0);
    let empty_menu = dump(&mut session);
    assert_context_menu(&empty_menu, 2);
    assert!(has_ui(&empty_menu, "FileTreeMenuItem(0)"));
    assert!(has_ui(&empty_menu, "FileTreeMenuItem(1)"));
    assert!(!has_ui(&empty_menu, "FileTreeMenuItem(2)"));
    run_script(&mut session, b"key escape\n");
    let closed = dump(&mut session);
    assert_eq!(closed["overlays"]["context_menu"], false);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_tree_create_file_updates_disk_and_tree() {
    let dir = tree_fixture("ui-tree-ops-create-file");
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    let before = dump(&mut session);
    let before_nodes = tree_node_count(&before);

    open_context_menu_on_node(&mut session, "FileTreeNode(2)");
    assert_context_menu(&dump(&mut session), 9);
    click_ui(&mut session, "FileTreeMenuItem(0)");
    assert_file_tree_dialog(&dump(&mut session), "create");
    run_script(&mut session, b"type new-file.txt\n");
    click_ui(&mut session, "FileTreeCreateConfirm");
    run_script(&mut session, b"settle 2000\n");

    let created = dir.join("new-file.txt");
    let after = dump(&mut session);
    assert!(created.is_file());
    assert_eq!(tree_node_count(&after), before_nodes + 1, "{after}");
    assert_eq!(after["overlays"]["file_tree_dialog"], Value::Null);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_tree_create_folder_updates_disk_and_tree() {
    let dir = tree_fixture("ui-tree-ops-create-folder");
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    let before_nodes = tree_node_count(&dump(&mut session));

    open_context_menu_on_node(&mut session, "FileTreeNode(0)");
    assert_context_menu(&dump(&mut session), 9);
    click_ui(&mut session, "FileTreeMenuItem(1)");
    assert_file_tree_dialog(&dump(&mut session), "create");
    run_script(&mut session, b"type new-folder\n");
    click_ui(&mut session, "FileTreeCreateConfirm");
    run_script(&mut session, b"settle 2000\n");

    let created = dir.join("new-folder");
    let after = dump(&mut session);
    assert!(created.is_dir());
    assert_eq!(tree_node_count(&after), before_nodes + 1, "{after}");
    assert_eq!(after["overlays"]["file_tree_dialog"], Value::Null);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_tree_create_rejects_empty_existing_and_separator_names() {
    let dir = tree_fixture("ui-tree-ops-create-invalid");
    let existing = dir.join("zeta.txt");
    let original = std::fs::read(&existing).unwrap();
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    let before_nodes = tree_node_count(&dump(&mut session));

    open_create_file_dialog(&mut session);
    click_ui(&mut session, "FileTreeCreateConfirm");
    let empty_name = dump(&mut session);
    assert_file_tree_dialog(&empty_name, "create");
    click_ui(&mut session, "FileTreeCreateCancel");
    assert_eq!(tree_node_count(&dump(&mut session)), before_nodes);

    open_create_file_dialog(&mut session);
    run_script(&mut session, b"type zeta.txt\n");
    click_ui(&mut session, "FileTreeCreateConfirm");
    let duplicate_name = dump(&mut session);
    assert_file_tree_dialog(&duplicate_name, "create");
    assert_eq!(std::fs::read(&existing).unwrap(), original);
    click_ui(&mut session, "FileTreeCreateCancel");
    assert_eq!(tree_node_count(&dump(&mut session)), before_nodes);

    open_create_file_dialog(&mut session);
    run_script(&mut session, b"type bad/name\n");
    click_ui(&mut session, "FileTreeCreateConfirm");
    let separator_name = dump(&mut session);
    assert_file_tree_dialog(&separator_name, "create");
    assert!(!dir.join("bad").exists());
    click_ui(&mut session, "FileTreeCreateCancel");
    assert_eq!(tree_node_count(&dump(&mut session)), before_nodes);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_tree_rename_updates_disk_tree_and_open_tab() {
    let dir = tree_fixture("ui-tree-ops-rename");
    let old_path = dir.join("zeta.txt");
    let new_path = dir.join("renamed.txt");
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    let lines = run_script(
        &mut session,
        format!("open {}\nsettle 1000\n", old_path.display()).as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let before = dump(&mut session);
    let before_nodes = tree_node_count(&before);

    open_context_menu_on_node(&mut session, "FileTreeNode(2)");
    assert_context_menu(&dump(&mut session), 9);
    click_ui(&mut session, "FileTreeMenuItem(5)");
    assert_file_tree_dialog(&dump(&mut session), "rename");
    run_script(&mut session, b"type renamed.txt\n");
    click_ui(&mut session, "FileTreeRenameConfirm");
    run_script(&mut session, b"settle 2000\n");

    let after = dump(&mut session);
    assert!(!old_path.exists());
    assert!(new_path.is_file());
    assert_eq!(tree_node_count(&after), before_nodes, "{after}");
    assert!(has_ui(&after, "FileTreeNode(2)"), "{after}");
    assert!(after["tabs"].as_array().unwrap().iter().any(|tab| {
        tab["path"] == new_path.display().to_string()
    }), "{after}");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_tree_move_drag_confirms_and_updates_open_tab() {
    let dir = scratch_dir("ui-tree-ops-move");
    let folder = dir.join("alpha-folder");
    std::fs::create_dir_all(&folder).unwrap();
    let old_path = dir.join("zeta.txt");
    std::fs::write(&old_path, "move fixture\n").unwrap();
    let new_path = folder.join("zeta.txt");
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    let lines = run_script(
        &mut session,
        format!("open {}\nsettle 1000\n", old_path.display()).as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");

    drag_node_across_its_row(&mut session, "FileTreeNode(2)");
    let self_drop = dump(&mut session);
    assert_eq!(self_drop["overlays"]["file_tree_dialog"], Value::Null);
    assert!(old_path.is_file());

    drag_nodes(&mut session, "FileTreeNode(2)", "FileTreeNode(1)");
    assert_file_tree_dialog(&dump(&mut session), "move");
    click_ui(&mut session, "FileTreeMoveConfirm");
    run_script(&mut session, b"settle 2000\n");

    let after = dump(&mut session);
    assert!(!old_path.exists());
    assert!(new_path.is_file());
    assert_eq!(tree_node_count(&after), 3, "{after}");
    assert!(has_ui(&after, "FileTreeNode(2)"), "{after}");
    assert!(after["tabs"].as_array().unwrap().iter().any(|tab| {
        tab["path"] == new_path.display().to_string()
    }), "{after}");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_tree_move_rejects_folder_self_and_child_drop() {
    let dir = tree_fixture("ui-tree-ops-move-reject");
    let folder = dir.join("alpha-folder");
    let child = folder.join("inside");
    let nested = child.join("nested.txt");
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    click_ui(&mut session, "FileTreeArrow(1)");
    run_script(&mut session, b"settle 1000\n");

    drag_node_across_its_row(&mut session, "FileTreeNode(1)");
    let self_drop = dump(&mut session);
    assert_eq!(self_drop["overlays"]["file_tree_dialog"], Value::Null);
    assert!(folder.is_dir());
    assert!(nested.is_file());

    drag_nodes(&mut session, "FileTreeNode(1)", "FileTreeNode(2)");
    let child_drop = dump(&mut session);
    assert_eq!(child_drop["overlays"]["file_tree_dialog"], Value::Null);
    assert!(folder.is_dir());
    assert!(child.is_dir());
    assert!(nested.is_file());

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_tree_delete_cancel_preserves_file_tree_and_open_tab() {
    let dir = tree_fixture("ui-tree-ops-delete-cancel");
    let file = dir.join("zeta.txt");
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    let lines = run_script(
        &mut session,
        format!("open {}\nsettle 1000\n", file.display()).as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let before = dump(&mut session);
    let before_nodes = tree_node_count(&before);

    open_context_menu_on_node(&mut session, "FileTreeNode(2)");
    assert_context_menu(&dump(&mut session), 9);
    click_ui(&mut session, "FileTreeMenuItem(2)");
    let delete_dialog = dump(&mut session);
    assert_file_tree_dialog(&delete_dialog, "delete");
    assert!(has_ui(&delete_dialog, "FileTreeDeleteConfirm"));
    assert!(has_ui(&delete_dialog, "FileTreeDeleteCancel"));
    assert_ui_rect_inside_window(&delete_dialog, "FileTreeDeleteConfirm");
    assert_ui_rect_inside_window(&delete_dialog, "FileTreeDeleteCancel");
    click_ui(&mut session, "FileTreeDeleteCancel");

    let after = dump(&mut session);
    assert_eq!(after["overlays"]["file_tree_dialog"], Value::Null);
    assert!(file.is_file());
    assert_eq!(tree_node_count(&after), before_nodes, "{after}");
    assert!(has_ui(&after, "FileTreeNode(2)"), "{after}");
    assert!(after["tabs"].as_array().unwrap().iter().any(|tab| {
        tab["path"] == file.display().to_string()
    }), "{after}");

    let _ = std::fs::remove_dir_all(dir);
}
