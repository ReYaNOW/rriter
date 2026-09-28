//! Tabs of deleted files (VS Code behaviour): tree delete closes clean tabs and
//! marks dirty ones; an external `rm` marks the tab; save or the file coming
//! back clears the mark.

use crate::headless::tests_support::{click_ui, dump, run_script, wait_until};
use crate::headless::ui_tests_tree_trash::{
    cleanup, confirm_delete, open_delete_dialog, open_workspace, tab_with_path, tree_fixture,
};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Workspace with the Explorer open and `zeta.txt` (tree row 2) in a tab. With
/// `edit` the tab gets unsaved text and `zz-other.txt` is opened after it: a
/// click into the tree autosaves the active tab, so only an inactive tab can
/// stay dirty through a tree delete.
fn workspace_with_open_file(name: &str, edit: bool) -> (PathBuf, PathBuf, crate::headless::HeadlessSession) {
    let (dir, _, file) = tree_fixture(name);
    let other = dir.join("zz-other.txt");
    std::fs::write(&other, "other\n").unwrap_or_else(|err| panic!("write {}: {err}", other.display()));
    let mut session = open_workspace(&dir);
    // The Explorer holds keyboard focus: click into the editor before typing.
    let edit = if edit {
        format!("mouse_move 900 400\nclick\ntype edited\nsettle 300\nopen {}\nsettle 1000\n", other.display())
    } else {
        String::new()
    };
    let script = format!("open {}\nsettle 1000\n{edit}", file.display());
    let lines = run_script(&mut session, script.as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let state = dump(&mut session);
    assert_eq!(tab_with_path(&state, &file)["modified"], !edit.is_empty(), "{state}");
    (dir, file, session)
}

fn wait_deleted(session: &mut crate::headless::HeadlessSession, file: &Path, deleted: bool) {
    wait_until(session, 5000, &format!("tab deleted={deleted}"), |session| {
        tab_with_path(&dump(session), file)["deleted"] == deleted
    });
}

#[test]
fn headless_tree_delete_closes_clean_tab() {
    let (dir, file, mut session) = workspace_with_open_file("ui-deleted-tab-clean", false);

    open_delete_dialog(&mut session, "FileTreeNode(2)");
    confirm_delete(&mut session);

    let state = dump(&mut session);
    assert!(!file.exists());
    assert_eq!(tab_with_path(&state, &file), Value::Null, "{state}");
    cleanup(&dir);
}

#[test]
fn headless_tree_delete_keeps_dirty_tab_marked_until_undo() {
    let (dir, file, mut session) = workspace_with_open_file("ui-deleted-tab-dirty", true);

    open_delete_dialog(&mut session, "FileTreeNode(2)");
    confirm_delete(&mut session);
    let state = dump(&mut session);
    let tab = tab_with_path(&state, &file);
    assert_eq!(tab["deleted"], true, "{state}");
    assert_eq!(tab["modified"], true, "{state}");

    // Undo puts the file back at its own path: the mark clears, edits stay.
    click_ui(&mut session, "FileTreeNode(0)");
    let lines = run_script(&mut session, b"key ctrl+z\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_deleted(&mut session, &file, false);
    assert!(file.is_file());
    assert_eq!(tab_with_path(&dump(&mut session), &file)["modified"], true);
    cleanup(&dir);
}

#[test]
fn headless_external_rm_marks_clean_tab() {
    let (dir, file, mut session) = workspace_with_open_file("ui-deleted-tab-rm", false);

    std::fs::remove_file(&file).unwrap_or_else(|err| panic!("rm {}: {err}", file.display()));
    wait_deleted(&mut session, &file, true);
    let state = dump(&mut session);
    let tab = tab_with_path(&state, &file);
    assert_eq!(tab["modified"], false, "{state}");
    assert_eq!(tab["active"], true, "{state}");
    cleanup(&dir);
}

#[test]
fn headless_save_recreates_deleted_file_and_clears_mark() {
    let (dir, file, mut session) = workspace_with_open_file("ui-deleted-tab-save", false);
    std::fs::remove_file(&file).unwrap_or_else(|err| panic!("rm {}: {err}", file.display()));
    wait_deleted(&mut session, &file, true);

    let lines = run_script(&mut session, b"type saved-\nkey ctrl+s\nsettle 300\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");

    let state = dump(&mut session);
    let tab = tab_with_path(&state, &file);
    assert_eq!(tab["deleted"], false, "{state}");
    assert_eq!(tab["modified"], false, "{state}");
    let text = std::fs::read_to_string(&file).unwrap_or_else(|err| panic!("read {}: {err}", file.display()));
    assert!(text.contains("saved-"), "{text}");
    cleanup(&dir);
}

#[test]
fn headless_focus_loss_autosave_skips_deleted_dirty_tab() {
    let (dir, file, mut session) = workspace_with_open_file("ui-deleted-tab-autosave", false);
    let lines = run_script(&mut session, b"mouse_move 900 400\nclick\ntype unsaved-\nsettle 300\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    std::fs::remove_file(&file).unwrap_or_else(|err| panic!("rm {}: {err}", file.display()));
    wait_deleted(&mut session, &file, true);

    // A click into the tree takes focus from the editor: that autosave must
    // not write the deleted file back.
    click_ui(&mut session, "FileTreeNode(0)");
    let lines = run_script(&mut session, b"settle 300\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");

    assert!(!file.exists(), "autosave recreated {}", file.display());
    let state = dump(&mut session);
    let tab = tab_with_path(&state, &file);
    assert_eq!(tab["deleted"], true, "{state}");
    assert_eq!(tab["modified"], true, "{state}");
    cleanup(&dir);
}

#[test]
fn headless_external_recreate_clears_mark_and_reloads_clean_tab() {
    let (dir, file, mut session) = workspace_with_open_file("ui-deleted-tab-recreate", false);
    std::fs::remove_file(&file).unwrap_or_else(|err| panic!("rm {}: {err}", file.display()));
    wait_deleted(&mut session, &file, true);

    std::fs::write(&file, "recreated outside\n")
        .unwrap_or_else(|err| panic!("write {}: {err}", file.display()));
    wait_deleted(&mut session, &file, false);
    wait_until(&mut session, 5000, "clean tab reloaded from disk", |session| {
        session.app.editor.get_full_text() == "recreated outside\n"
    });
    assert_eq!(tab_with_path(&dump(&mut session), &file)["modified"], false);
    cleanup(&dir);
}
