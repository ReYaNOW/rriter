//! Characterization of FileTree shortcuts before keymap changes.

use crate::app::file_tree::FileTreeClipboardMode;
use crate::headless::tests_support::{
    click_ui, run_script, scratch_dir, wait_until, workspace_with_explorer,
};
use crate::headless::ui_tests_hotkeys_characterization::assert_chord_effect;

#[test]
fn headless_hotkeys_characterize_file_tree_shortcuts() {
    let dir = scratch_dir("ui-hotkeys-file-tree");
    let file = dir.join("entry.txt");
    let destination = dir.join("destination");
    std::fs::create_dir(&destination).expect("create FileTree destination");
    std::fs::write(&file, "entry\n").expect("write FileTree fixture");
    let mut session = writable_tree_session(&dir);
    wait_until(&mut session, 5000, "FileTree fixture entry", |session| {
        session.app.ide_panel.file_tree_nodes.iter().any(|node| node.path == file)
    });

    let node_index = session
        .app
        .ide_panel
        .file_tree_nodes
        .iter()
        .position(|node| node.path == file)
        .expect("FileTree fixture entry exists");
    click_ui(&mut session, &format!("FileTreeNode({node_index})"));
    assert!(session.app.ide_panel.file_tree_focused);
    assert_eq!(session.app.ide_panel.file_tree_selection.len(), 1);

    assert_chord_effect(
        &mut session,
        "ctrl+c",
        |_| {},
        |session| assert_eq!(
            session.app.ide_panel.file_tree_clipboard.as_ref().map(|clipboard| clipboard.mode),
            Some(FileTreeClipboardMode::Copy),
        ),
    );
    assert_chord_effect(
        &mut session,
        "ctrl+x",
        |_| {},
        |session| assert_eq!(
            session.app.ide_panel.file_tree_clipboard.as_ref().map(|clipboard| clipboard.mode),
            Some(FileTreeClipboardMode::Cut),
        ),
    );
    let destination_index = session
        .app
        .ide_panel
        .file_tree_nodes
        .iter()
        .position(|node| node.path == destination)
        .expect("FileTree destination exists");
    click_ui(&mut session, &format!("FileTreeNode({destination_index})"));
    run_ok(&mut session, "key ctrl+v\n");
    assert!(!file.exists());
    assert!(destination.join("entry.txt").exists());
    assert_eq!(session.app.ide_panel.file_tree_undo_stack.len(), 1);
    run_ok(&mut session, "key ctrl+z\n");
    assert!(file.exists());
    assert!(!destination.join("entry.txt").exists());
    assert!(session.app.ide_panel.file_tree_undo_stack.is_empty());

    run_ok(&mut session, "key delete\n");
    assert!(session.app.ide_panel.file_tree_delete_dialog.is_some());
    run_ok(&mut session, "key escape\n");
    assert!(session.app.ide_panel.file_tree_delete_dialog.is_none());
    run_ok(&mut session, "key f2\n");
    assert!(session.app.ide_panel.file_tree_rename_dialog.is_some());
    run_ok(&mut session, "key escape\n");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_file_tree_clipboard_stays_active_with_terminal_focused() {
    let dir = scratch_dir("ui-hotkeys-file-tree-terminal");
    let file = dir.join("entry.txt");
    let destination = dir.join("destination");
    std::fs::create_dir(&destination).expect("create FileTree destination");
    std::fs::write(&file, "entry\n").expect("write FileTree fixture");
    let mut session = workspace_with_explorer(1280, 720, 4.0 / 3.0, &dir);
    wait_until(&mut session, 5000, "FileTree fixture entry", |session| {
        session.app.ide_panel.file_tree_nodes.iter().any(|node| node.path == file)
    });
    let node_index = session
        .app
        .ide_panel
        .file_tree_nodes
        .iter()
        .position(|node| node.path == file)
        .expect("FileTree fixture entry exists");
    click_ui(&mut session, &format!("FileTreeNode({node_index})"));
    run_ok(&mut session, "key alt+q\n");
    wait_until(&mut session, 8000, "terminal focus", |session| {
        session.app.ide_panel.terminal_focused
    });
    assert!(session.app.ide_panel.terminal_focused);
    assert!(session.app.ide_panel.file_tree_focused);

    assert_chord_effect(
        &mut session,
        "ctrl+c",
        |_| {},
        |session| assert_eq!(
            session.app.ide_panel.file_tree_clipboard.as_ref().map(|clipboard| clipboard.mode),
            Some(FileTreeClipboardMode::Copy),
        ),
    );
    assert_chord_effect(
        &mut session,
        "ctrl+x",
        |_| {},
        |session| assert_eq!(
            session.app.ide_panel.file_tree_clipboard.as_ref().map(|clipboard| clipboard.mode),
            Some(FileTreeClipboardMode::Cut),
        ),
    );
    session.app.ide_panel.file_tree_selection.clear();
    session.app.ide_panel.file_tree_selection.insert(destination.clone());
    assert!(session.app.ide_panel.terminal_focused);
    assert!(session.app.ide_panel.file_tree_focused);
    run_ok(&mut session, "key ctrl+v\n");
    assert!(!file.exists());
    assert!(destination.join("entry.txt").exists());
    assert_eq!(session.app.ide_panel.file_tree_undo_stack.len(), 1);
    run_ok(&mut session, "key ctrl+z\n");
    assert!(file.exists());
    assert!(!destination.join("entry.txt").exists());
    assert!(session.app.ide_panel.file_tree_undo_stack.is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_file_tree_shortcuts_require_selection_and_closed_settings() {
    let dir = scratch_dir("ui-hotkeys-file-tree-guards");
    let file = dir.join("entry.txt");
    std::fs::write(&file, "entry\n").expect("write FileTree fixture");
    let mut session = writable_tree_session(&dir);
    session.app.ide_panel.file_tree_focused = true;

    run_ok(&mut session, "key ctrl+x\n");
    assert!(session.app.ide_panel.file_tree_clipboard.is_none());
    assert!(file.exists());

    session.app.ide_panel.file_tree_selection.insert(file.clone());
    run_ok(&mut session, "key f1\n");
    assert!(session.app.show_settings);
    run_ok(&mut session, "key ctrl+x\n");
    assert!(session.app.ide_panel.file_tree_clipboard.is_none());
    assert!(file.exists());
    let _ = std::fs::remove_dir_all(dir);
}

fn run_ok(session: &mut crate::headless::HeadlessSession, script: &str) {
    let lines = run_script(session, script.as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
}

fn writable_tree_session(dir: &std::path::Path) -> crate::headless::HeadlessSession {
    let options = crate::headless::profile::HeadlessOptions {
        size: (1280, 720),
        allow_writes: true,
        ..Default::default()
    };
    let root = crate::headless::tests_support::ensure_test_profile_root();
    let mut session = match crate::headless::HeadlessSession::new(&options, root) {
        Ok(session) => session,
        Err((code, message)) => panic!("headless session (code {code}): {message}"),
    };
    session.hz_probe = || None;
    run_ok(&mut session, &format!("scale 1.3333334\nworkspace {}\n", dir.display()));
    if !crate::headless::tests_support::dump(&mut session)["ide_panel"]["open"]
        .as_array()
        .is_some_and(|panels| panels.iter().any(|panel| panel == "explorer"))
    {
        click_ui(&mut session, "SidebarSlot(Explorer)");
    }
    run_ok(&mut session, "settle 2000\n");
    session
}
