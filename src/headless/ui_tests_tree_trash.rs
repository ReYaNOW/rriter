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

pub(crate) fn open_delete_dialog(session: &mut crate::headless::HeadlessSession, node_id: &str) {
    open_context_menu_on_node(session, node_id);
    click_ui(session, "FileTreeMenuItem(2)");
    let state = dump(session);
    assert_eq!(state["overlays"]["file_tree_dialog"], "delete", "{state}");
    assert!(has_ui(&state, "FileTreeDeleteConfirm"), "{state}");
}

pub(crate) fn confirm_delete(session: &mut crate::headless::HeadlessSession) {
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

pub(crate) fn tree_fixture(name: &str) -> (PathBuf, PathBuf, PathBuf) {
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
pub(crate) fn cleanup(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
    if let Some(root) = crate::platform::trash_layout().files_dir.parent() {
        let _ = std::fs::remove_dir_all(root);
    }
}

pub(crate) fn open_workspace(dir: &Path) -> crate::headless::HeadlessSession {
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

/// The dump entry of the tab bound to `path`, or `Null`.
pub(crate) fn tab_with_path(state: &Value, path: &Path) -> Value {
    let path = path.display().to_string();
    state["tabs"]
        .as_array()
        .and_then(|tabs| tabs.iter().find(|tab| tab["path"] == path.as_str()))
        .cloned()
        .unwrap_or(Value::Null)
}

#[test]
fn headless_tree_undo_delete_to_taken_path_rebinds_dirty_tab() {
    let (dir, _, file) = tree_fixture("ui-tree-trash-open-tab");
    let other = dir.join("zz-other.txt");
    std::fs::write(&other, "other\n").unwrap_or_else(|err| panic!("write {}: {err}", other.display()));
    let mut session = open_workspace(&dir);
    // The Explorer holds keyboard focus: click into the editor before typing.
    // Then another tab becomes active: a click into the tree autosaves the
    // active tab, so only an inactive one stays dirty through a tree delete.
    let lines = run_script(
        &mut session,
        format!(
            "open {}\nsettle 1000\nmouse_move 900 400\nclick\ntype edited\nsettle 300\nopen {}\nsettle 1000\n",
            file.display(),
            other.display()
        )
        .as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert_eq!(tab_with_path(&dump(&mut session), &file)["modified"], true);

    open_delete_dialog(&mut session, "FileTreeNode(2)");
    confirm_delete(&mut session);
    let state = dump(&mut session);
    assert!(!file.exists());
    let tab = tab_with_path(&state, &file);
    assert_eq!(tab["deleted"], true, "{state}");
    assert_eq!(tab["modified"], true, "{state}");

    // A folder takes the file's name, so undo has to restore under a new one.
    wait_until(&mut session, 5000, "deleted file tree row", |session| {
        tree_node_count(&dump(session)) == 3
    });
    std::fs::create_dir(&file).unwrap_or_else(|err| panic!("mkdir {}: {err}", file.display()));
    wait_until(&mut session, 5000, "folder row in the file's place", |session| {
        tree_node_count(&dump(session)) == 4
    });
    assert_eq!(tab_with_path(&dump(&mut session), &file)["deleted"], true);

    click_ui(&mut session, "FileTreeNode(0)");
    let lines = run_script(&mut session, b"key ctrl+z\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let restored = dir.join("zeta copy.txt");
    wait_until(&mut session, 5000, "tab rebound to the restored copy", |session| {
        tab_with_path(&dump(session), &restored)["deleted"] == false
    });
    assert!(restored.is_file());
    let state = dump(&mut session);
    assert_eq!(tab_with_path(&state, &restored)["modified"], true, "{state}");
    assert_eq!(tab_with_path(&state, &file), Value::Null, "{state}");
    cleanup(&dir);
}
