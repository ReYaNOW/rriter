//! File Tree deletion and trash undo regressions.

use crate::headless::tests_support::{
    click_ui, dump, has_ui, run_script, scratch_dir, wait_until, workspace_with_explorer,
};
use crate::headless::ui_tests_tree_ops::{open_context_menu_on_node, tree_node_count};
use serde_json::Value;
use std::path::{Path, PathBuf};

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;

fn open_delete_dialog(session: &mut crate::headless::HeadlessSession, node_id: &str) {
    open_context_menu_on_node(session, node_id);
    click_ui(session, "FileTreeMenuItem(2)");
    let state = dump(session);
    assert_eq!(state["overlays"]["file_tree_dialog"], "delete", "{state}");
    assert!(has_ui(&state, "FileTreeDeleteConfirm"), "{state}");
}

fn confirm_delete(session: &mut crate::headless::HeadlessSession) {
    click_ui(session, "FileTreeDeleteConfirm");
    assert_eq!(dump(session)["overlays"]["file_tree_dialog"], Value::Null);
}

fn assert_trashed(path: &Path) {
    let layout = crate::platform::trash_layout();
    let Some(name) = path.file_name() else {
        panic!("no file name: {}", path.display());
    };
    let trashed_path = layout.files_dir.join(name);
    assert!(trashed_path.exists(), "missing {}", trashed_path.display());
    let info_path = layout.info_dir.join(format!("{}.trashinfo", name.to_string_lossy()));
    assert!(info_path.is_file(), "missing {}", info_path.display());
    let info = std::fs::read_to_string(&info_path)
        .unwrap_or_else(|err| panic!("read {}: {err}", info_path.display()));
    assert!(info.contains(&path.display().to_string()), "{info}");
}

fn tree_fixture(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let dir = scratch_dir(name);
    let folder = dir.join("alpha-folder");
    let nested = folder.join("inside");
    let file = dir.join("zeta.txt");
    std::fs::create_dir_all(&nested)
        .and_then(|()| std::fs::write(nested.join("nested.txt"), "nested fixture\n"))
        .and_then(|()| std::fs::write(&file, "tree fixture\n"))
        .unwrap_or_else(|err| panic!("tree fixture: {err}"));
    (dir, folder, file)
}

/// Removes the fixture and this process's test Trash (`trash_layout` is per-PID under cfg(test)).
fn cleanup(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
    if let Some(root) = crate::platform::trash_layout().files_dir.parent() {
        let _ = std::fs::remove_dir_all(root);
    }
}

fn open_workspace(dir: &Path) -> crate::headless::HeadlessSession {
    workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, dir)
}

#[test]
fn headless_tree_confirmed_delete_trashes_file_and_removes_tree_row() {
    let (dir, _, file) = tree_fixture("ui-tree-trash-file");
    let mut session = open_workspace(&dir);

    open_delete_dialog(&mut session, "FileTreeNode(2)");
    confirm_delete(&mut session);

    wait_until(&mut session, 5000, "deleted file tree row", |session| {
        !has_ui(&dump(session), "FileTreeNode(2)")
    });
    assert!(!file.exists());
    assert_trashed(&file);
    cleanup(&dir);
}

#[test]
fn headless_tree_confirmed_delete_trashes_non_empty_folder_and_removes_tree_row() {
    let (dir, folder, _) = tree_fixture("ui-tree-trash-folder");
    let mut session = open_workspace(&dir);

    open_delete_dialog(&mut session, "FileTreeNode(1)");
    confirm_delete(&mut session);

    wait_until(&mut session, 5000, "deleted folder tree row", |session| {
        tree_node_count(&dump(session)) == 2
    });
    assert!(!folder.exists());
    assert_trashed(&folder);
    cleanup(&dir);
}

#[test]
fn headless_tree_undo_delete_restores_file_and_tree_row() {
    let (dir, _, file) = tree_fixture("ui-tree-trash-undo");
    let mut session = open_workspace(&dir);

    open_delete_dialog(&mut session, "FileTreeNode(2)");
    confirm_delete(&mut session);
    wait_until(&mut session, 5000, "file removed from tree before undo", |session| {
        !has_ui(&dump(session), "FileTreeNode(2)")
    });
    assert!(!file.exists());

    click_ui(&mut session, "FileTreeNode(0)");
    let lines = run_script(&mut session, b"key ctrl+z\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(&mut session, 5000, "restored file tree row", |session| {
        has_ui(&dump(session), "FileTreeNode(2)")
    });
    assert!(file.is_file());
    cleanup(&dir);
}

#[test]
fn headless_tree_confirmed_delete_keeps_open_file_tab() {
    let (dir, _, file) = tree_fixture("ui-tree-trash-open-tab");
    let mut session = open_workspace(&dir);
    let lines = run_script(
        &mut session,
        format!("open {}\nsettle 1000\n", file.display()).as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");

    open_delete_dialog(&mut session, "FileTreeNode(2)");
    confirm_delete(&mut session);
    wait_until(&mut session, 5000, "deleted open file tree row", |session| {
        !has_ui(&dump(session), "FileTreeNode(2)")
    });
    let state = dump(&mut session);
    assert!(!file.exists());
    let tabs = state["tabs"].as_array().map(Vec::as_slice).unwrap_or_default();
    assert!(
        tabs.iter().any(|tab| tab["path"] == file.display().to_string()),
        "{state}"
    );
    cleanup(&dir);
}
