use super::*;

fn test_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("rriter_{name}_{}", std::process::id()))
}

#[test]
fn file_tree_ignore_patterns_cover_exact_prefix_and_suffix() {
    let patterns = ["node_modules", "*.pyc", "target*"];

    assert!(matches_ignore_pattern("node_modules", &patterns));
    assert!(matches_ignore_pattern("main.pyc", &patterns));
    assert!(matches_ignore_pattern("target-debug", &patterns));
    assert!(!matches_ignore_pattern("src", &patterns));
}

#[test]
fn file_tree_name_input_stays_single_line_and_bounded() {
    let mut editor = Editor::new(16);
    insert_file_tree_name_text(&mut editor, "alpha\nbeta\r");

    assert_eq!(editor.get_full_text(), "alphabeta");

    editor.select_all();
    insert_file_tree_name_text(
        &mut editor,
        &"x".repeat(FILE_TREE_NAME_INPUT_MAX_BYTES + 20),
    );

    assert_eq!(editor.get_full_text().len(), FILE_TREE_NAME_INPUT_MAX_BYTES);
}

#[test]
fn file_tree_name_input_edit_keys_cover_undo_redo_and_copy() {
    let mut editor = Editor::new(16);
    handle_file_tree_name_editor_input(
        &mut editor,
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyA),
        Some("a"),
        false,
        false,
        false,
        true,
        None,
    );
    handle_file_tree_name_editor_input(
        &mut editor,
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyB),
        Some("b"),
        false,
        false,
        false,
        true,
        None,
    );
    assert_eq!(editor.get_full_text(), "ab");

    handle_file_tree_name_editor_input(
        &mut editor,
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyZ),
        None,
        true,
        false,
        false,
        false,
        None,
    );
    assert_eq!(editor.get_full_text(), "");

    handle_file_tree_name_editor_input(
        &mut editor,
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyY),
        None,
        true,
        false,
        false,
        false,
        None,
    );
    assert_eq!(editor.get_full_text(), "ab");

    handle_file_tree_name_editor_input(
        &mut editor,
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyZ),
        None,
        true,
        false,
        false,
        false,
        None,
    );
    handle_file_tree_name_editor_input(
        &mut editor,
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyZ),
        None,
        true,
        false,
        true,
        false,
        None,
    );
    assert_eq!(editor.get_full_text(), "ab");

    handle_file_tree_name_editor_input(
        &mut editor,
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyA),
        None,
        true,
        false,
        false,
        false,
        None,
    );
    let copied = handle_file_tree_name_editor_input(
        &mut editor,
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyC),
        None,
        true,
        false,
        false,
        false,
        None,
    );
    assert_eq!(copied.as_deref(), Some("ab"));
}

#[test]
fn file_tree_name_input_hit_testing_accounts_for_scroll() {
    let text = "abcdef";
    let scroll_x = file_tree_name_input_scroll_x(text, text.len(), 30.0, |_| 10.0);

    assert_eq!(scroll_x, 30.0);
    assert_eq!(
        file_tree_name_input_hit_index(text, 5.0 + scroll_x, |_| 10.0),
        3
    );
    assert_eq!(
        file_tree_name_input_hit_index(text, 500.0, |_| 10.0),
        text.len()
    );
}

#[test]
fn file_tree_scan_sorts_expands_and_skips_ignored_nodes_end_to_end() {
    let root = test_root("file_tree_scan");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("dir10")).unwrap();
    std::fs::create_dir_all(root.join("dir2")).unwrap();
    std::fs::create_dir_all(root.join("__pycache__")).unwrap();
    std::fs::write(root.join("b.txt"), "b").unwrap();
    std::fs::write(root.join("a.py"), "a").unwrap();
    std::fs::write(root.join("z.pyc"), "ignored").unwrap();

    let mut expanded = FxHashSet::default();
    expanded.insert(root.clone());
    let gitignore = ignore::gitignore::Gitignore::empty();
    let nodes = scan_dir_parallel(
        root.clone(),
        "workspace".to_string(),
        0,
        &expanded,
        true,
        2,
        &gitignore,
        DEFAULT_IGNORE_PATTERNS,
    );
    let names: Vec<_> = nodes.iter().map(|node| node.name.as_str()).collect();

    assert_eq!(names, vec!["workspace", "dir2", "dir10", "a.py", "b.txt"]);
    assert!(nodes[0].is_expanded);
    assert_eq!(nodes[1].depth, 1);
    assert!(nodes.iter().all(|node| node.name != "__pycache__"));
    assert!(nodes.iter().all(|node| node.name != "z.pyc"));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn file_tree_scan_covers_collapsed_depth_limit_and_gitignore_marks() {
    let root = test_root("file_tree_depth_gitignore");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("alpha").join("nested")).unwrap();
    std::fs::create_dir_all(root.join("ignored_dir")).unwrap();
    std::fs::write(root.join("alpha").join("nested").join("deep.py"), "deep").unwrap();
    std::fs::write(root.join("ignored.txt"), "ignored").unwrap();
    std::fs::write(root.join(".gitignore"), "ignored.txt\nignored_dir/\n").unwrap();

    let mut builder = ignore::gitignore::GitignoreBuilder::new(&root);
    let _ = builder.add(root.join(".gitignore"));
    let gitignore = builder.build().unwrap();

    let collapsed = FxHashSet::default();
    let collapsed_nodes = scan_dir_parallel(
        root.clone(),
        "workspace".to_string(),
        0,
        &collapsed,
        true,
        10,
        &gitignore,
        DEFAULT_IGNORE_PATTERNS,
    );
    assert_eq!(collapsed_nodes.len(), 1);
    assert_eq!(collapsed_nodes[0].name, "workspace");
    assert!(!collapsed_nodes[0].is_expanded);
    assert!(!collapsed_nodes[0].is_ignored);

    let mut expanded = FxHashSet::default();
    expanded.insert(root.clone());
    expanded.insert(root.join("alpha"));
    let nodes = scan_dir_parallel(
        root.clone(),
        "workspace".to_string(),
        0,
        &expanded,
        true,
        1,
        &gitignore,
        DEFAULT_IGNORE_PATTERNS,
    );
    let names: Vec<_> = nodes.iter().map(|node| node.name.as_str()).collect();

    assert!(names.contains(&"alpha"));
    assert!(names.contains(&"ignored_dir"));
    assert!(names.contains(&"ignored.txt"));
    assert!(names.contains(&".gitignore"));
    assert!(!names.contains(&"deep.py"));

    let alpha = nodes.iter().find(|node| node.name == "alpha").unwrap();
    assert!(alpha.is_dir);
    assert!(alpha.is_expanded);
    assert_eq!(alpha.depth, 1);

    let ignored_file = nodes
        .iter()
        .find(|node| node.name == "ignored.txt")
        .unwrap();
    assert!(!ignored_file.is_dir);
    assert!(ignored_file.is_ignored);

    let ignored_dir = nodes
        .iter()
        .find(|node| node.name == "ignored_dir")
        .unwrap();
    assert!(ignored_dir.is_dir);
    assert!(ignored_dir.is_ignored);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn spawn_scan_skips_missing_roots_applies_user_patterns_and_sends_final_tree() {
    let root = test_root("spawn_scan");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("keep_dir")).unwrap();
    std::fs::create_dir_all(root.join("skip_dir")).unwrap();
    std::fs::write(root.join("keep.rs"), "keep").unwrap();
    std::fs::write(root.join("skip.py"), "skip").unwrap();

    let mut expanded = FxHashSet::default();
    expanded.insert(root.clone());
    let rx = spawn_scan(
        vec![root.join("missing"), root.clone()],
        expanded,
        vec!["skip*".to_string()],
    );

    let first = match rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap() {
        FileTreeScanMessage::Nodes(nodes) => nodes,
        FileTreeScanMessage::IconsReady => panic!("scan must send nodes before icon signal"),
    };
    let second = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();

    let names: Vec<_> = first.iter().map(|node| node.name.as_str()).collect();

    assert!(matches!(second, FileTreeScanMessage::IconsReady));
    assert!(names.contains(&"keep_dir"));
    assert!(names.contains(&"keep.rs"));
    assert!(!names.contains(&"missing"));
    assert!(!names.contains(&"skip_dir"));
    assert!(!names.contains(&"skip.py"));
    assert!(first.iter().all(|node| node.path.exists()));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn file_tree_new_child_validation_blocks_empty_paths_and_traversal() {
    assert!(validate_child_name("main.rs").is_ok());
    assert!(validate_child_name("").is_err());
    assert!(validate_child_name("../x").is_err());
    assert!(validate_child_name("a/b").is_err());
    assert!(validate_child_name("..").is_err());
}

#[test]
fn file_tree_relative_path_uses_first_matching_workspace() {
    let root = PathBuf::from("/tmp/rriter_ws");
    let path = root.join("src/main.rs");

    assert_eq!(
        relative_path_for_workspace(&path, &[root]),
        PathBuf::from("src/main.rs")
    );
}

#[test]
fn file_tree_move_dialog_message_names_source_and_target() {
    assert_eq!(
        file_tree_move_dialog_message(
            &[PathBuf::from("/tmp/ws/.env.test")],
            Path::new("/tmp/ws/tests"),
        ),
        "Переместить '.env.test' в 'tests'?"
    );
    assert_eq!(
        file_tree_move_dialog_message(
            &[PathBuf::from("/tmp/ws/a.py"), PathBuf::from("/tmp/ws/b.py")],
            Path::new("/tmp/ws/tests"),
        ),
        "Переместить 2 элементов в 'tests'?"
    );
}

#[test]
fn file_tree_overlay_state_covers_menu_dialogs_and_overlay_ids() {
    let root = PathBuf::from("/tmp/ws");
    let mut panel = crate::app::IdePanelState::default();

    assert!(!file_tree_overlay_active_for_panel(&panel));
    assert!(!file_tree_modal_overlay_active_for_panel(&panel));

    panel.file_tree_context_menu = Some(FileTreeContextMenu {
        x: 1.0,
        y: 2.0,
        target_path: Some(root.join("main.rs")),
        target_is_dir: false,
        target_dir: Some(root.clone()),
        entries: vec![FileTreeMenuAction::Copy],
        opened_at: Instant::now(),
    });
    assert!(file_tree_overlay_active_for_panel(&panel));
    assert!(!file_tree_modal_overlay_active_for_panel(&panel));
    panel.file_tree_context_menu = None;

    panel.file_tree_create_dialog = Some(FileTreeCreateDialog {
        kind: FileTreeCreateKind::File,
        parent_dir: root.clone(),
        editor: Editor::new(64),
        error: None,
    });
    assert!(file_tree_overlay_active_for_panel(&panel));
    assert!(file_tree_modal_overlay_active_for_panel(&panel));
    panel.file_tree_create_dialog = None;

    panel.file_tree_rename_dialog = Some(FileTreeRenameDialog {
        path: root.join("old.rs"),
        editor: Editor::new(64),
        input_scroll_x: crate::scroll::ScrollState::new(7.0),
        error: None,
    });
    assert!(file_tree_overlay_active_for_panel(&panel));
    panel.file_tree_rename_dialog = None;

    panel.file_tree_move_dialog = Some(FileTreeMoveDialog {
        sources: vec![root.join("old.rs")],
        target_dir: root.join("src"),
        error: None,
    });
    assert!(file_tree_overlay_active_for_panel(&panel));
    panel.file_tree_move_dialog = None;

    panel.file_tree_delete_dialog = Some(FileTreeDeleteDialog {
        paths: vec![root.join("old.rs")],
        error: None,
    });
    assert!(file_tree_overlay_active_for_panel(&panel));
    panel.file_tree_delete_dialog = None;

    panel.api.mock_contract_field_delete_dialog =
        Some(crate::app::api_client::ApiMockContractFieldDeleteDialog {
            route_idx: 0,
            group: crate::ui_system::ApiMockContractFieldGroup::Query,
            field_idx: 0,
            field_label: "q".to_string(),
        });
    assert!(file_tree_overlay_active_for_panel(&panel));

    assert!(crate::app::App::ui_id_is_file_tree_overlay(
        crate::ui_system::UiId::FileTreeRenameInput
    ));
    assert!(crate::app::App::ui_id_is_file_tree_overlay(
        crate::ui_system::UiId::FileTreeDeleteConfirm
    ));
    assert!(crate::app::App::ui_id_is_file_tree_overlay(
        crate::ui_system::UiId::ApiMockContractFieldRemoveConfirm
    ));
    assert!(!crate::app::App::ui_id_is_file_tree_overlay(
        crate::ui_system::UiId::EditorTextBody
    ));
}

#[test]
fn file_tree_path_input_layout_clips_parent_and_preserves_input_width() {
    let parent = PathBuf::from("/tmp/workspace/src/features/bookings");
    let (prefix, input_x, input_w) =
        file_tree_path_input_layout(100.0, 460.0, 1.0, &parent, |text| text.len() as f32 * 8.0);

    assert!(prefix.starts_with("..."));
    assert!(prefix.ends_with(&format!("bookings{}", std::path::MAIN_SEPARATOR)));
    assert!(input_x > 100.0 + FILE_TREE_DIALOG_SIDE_PAD);
    assert!(input_w >= FILE_TREE_PATH_INPUT_MIN_W);

    let short = PathBuf::from("/tmp/ws");
    let (prefix, _, _) =
        file_tree_path_input_layout(0.0, 460.0, 1.0, &short, |text| text.len() as f32 * 4.0);
    assert_eq!(prefix, file_tree_parent_path_prefix(&short));
}

#[test]
fn file_tree_trash_single_path_and_restore_roundtrip() {
    let root = test_root("file_tree_trash");
    let _ = std::fs::remove_dir_all(&root);
    let workspace = root.join("workspace");
    let files_dir = root.join("trash").join("files");
    let info_dir = root.join("trash").join("info");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(&files_dir).unwrap();
    std::fs::create_dir_all(&info_dir).unwrap();
    let path = workspace.join("booking.py");
    std::fs::write(&path, "box\n").unwrap();

    let entry = trash_single_path(&path, &files_dir, &info_dir).unwrap();
    assert!(!path.exists());
    assert!(entry.trash_path.exists());
    let info = std::fs::read_to_string(&entry.info_path).unwrap();
    if cfg!(target_os = "linux") {
        assert!(info.contains("[Trash Info]"));
        assert!(info.contains("Path=/"));
    } else {
        assert!(info.contains("[RRiter Trash]"));
        assert!(info.contains("Path=rriter-path-v1:"));
    }

    let restored = restore_trash_entries(&[entry]).unwrap();
    assert_eq!(restored, vec![path.clone()]);
    assert!(path.exists());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn file_tree_managed_trash_metadata_restores_paths_without_utf8_assumptions() {
    let root = test_root("file_tree_managed_trash");
    let _ = std::fs::remove_dir_all(&root);
    let workspace = root.join("workspace");
    let files_dir = root.join("trash").join("files");
    let info_dir = root.join("trash").join("info");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(&files_dir).unwrap();
    std::fs::create_dir_all(&info_dir).unwrap();
    let path = workspace.join("booking.py");
    std::fs::write(&path, "box\n").unwrap();

    let entry = trash_single_path_with_layout(&path, &files_dir, &info_dir, false).unwrap();
    let info = std::fs::read_to_string(&entry.info_path).unwrap();
    assert!(info.contains("[RRiter Trash]"));
    assert!(info.contains("Path=rriter-path-v1:"));
    assert!(!path.exists());

    assert_eq!(restore_trash_entries(&[entry]).unwrap(), vec![path.clone()]);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "box\n");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn file_tree_context_menu_labels_and_anim_progress_are_stable() {
    assert_eq!(
        FileTreeMenuAction::CopyRelativePath.label(),
        "Скопировать относительный путь"
    );
    assert_eq!(
        FileTreeMenuAction::CopyTargetAbsolutePath.label(),
        "Скопировать абсолютный путь"
    );
    assert_eq!(
        FileTreeMenuAction::CopyTargetRelativePath.label(),
        "Скопировать относительный путь"
    );
    assert_eq!(
        FileTreeMenuAction::ShowInExplorer.label(),
        "Показать в проводнике"
    );

    let start = Instant::now();
    assert_eq!(crate::app::context_menu::context_menu_anim_progress(start, start), 0.0);
    assert_eq!(
        crate::app::context_menu::context_menu_anim_progress(
            start,
            start + std::time::Duration::from_secs(1),
        ),
        1.0
    );
    assert_eq!(file_tree_context_menu_anchor(100.0, 80.0, 1.0), (110.0, 90.0));
    assert_eq!(
        file_tree_context_menu_anchor(100.0, 80.0, 1.5),
        (115.0, 95.0)
    );

    assert_eq!(
        file_tree_context_menu_cursor(None),
        winit::window::CursorIcon::Default
    );
    assert_eq!(
        file_tree_context_menu_cursor(Some(crate::ui_system::UiId::EditorTab(0))),
        winit::window::CursorIcon::Default
    );
    assert_eq!(
        file_tree_context_menu_cursor(Some(crate::ui_system::UiId::FileTreeMenuItem(0))),
        winit::window::CursorIcon::Pointer
    );
    assert_eq!(
        file_tree_context_menu_cursor(Some(crate::ui_system::UiId::DatabaseContextItem(0))),
        winit::window::CursorIcon::Pointer
    );
}

#[test]
fn file_tree_copy_move_delete_paths_end_to_end() {
    let root = test_root("file_tree_ops");
    let _ = std::fs::remove_dir_all(&root);
    let src_dir = root.join("src");
    let target_dir = root.join("target_dir");
    std::fs::create_dir_all(src_dir.join("nested")).unwrap();
    std::fs::create_dir_all(&target_dir).unwrap();
    std::fs::write(src_dir.join("main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(src_dir.join("nested").join("mod.rs"), "mod x;\n").unwrap();

    let copied = copy_paths_to_dir(
        &[src_dir.join("main.rs"), src_dir.join("nested")],
        &target_dir,
    )
    .unwrap();
    assert_eq!(copied.len(), 2);
    assert!(target_dir.join("main.rs").exists());
    assert!(target_dir.join("nested").join("mod.rs").exists());

    let (_old, moved) = move_path_to_dir(&target_dir.join("main.rs"), &src_dir).unwrap();
    assert!(moved.exists());
    assert!(
        moved
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains("copy")
    );
    assert!(!target_dir.join("main.rs").exists());

    assert!(delete_paths(&[target_dir.join("nested")], &[root.clone()]).is_ok());
    assert!(!target_dir.join("nested").exists());
    assert!(delete_paths(&[root.clone()], &[root.clone()]).is_err());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn file_tree_prunes_duplicate_and_nested_paths_before_batch_operations() {
    let root = PathBuf::from("/tmp/ws");
    let paths = vec![
        root.join("src/main.rs"),
        root.join("src"),
        root.join("src/main.rs"),
        root.join("tests"),
    ];

    assert_eq!(
        prune_nested_paths(&paths),
        vec![root.join("src"), root.join("tests")]
    );
}

#[cfg(unix)]
#[test]
fn file_tree_copy_preserves_symlinks_without_following_cycles() {
    use std::os::unix::fs::symlink;

    let root = test_root("file_tree_symlink_copy");
    let _ = std::fs::remove_dir_all(&root);
    let source = root.join("source");
    let destination = root.join("destination");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("value.txt"), "value").unwrap();
    symlink(".", source.join("cycle")).unwrap();

    copy_path_recursive(&source, &destination).unwrap();

    assert_eq!(
        std::fs::read_to_string(destination.join("value.txt")).unwrap(),
        "value"
    );
    assert!(std::fs::symlink_metadata(destination.join("cycle"))
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        std::fs::read_link(destination.join("cycle")).unwrap(),
        PathBuf::from(".")
    );
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn file_tree_copy_supports_non_utf8_file_names() {
    use std::os::unix::ffi::OsStringExt;

    let root = test_root("file_tree_non_utf8_copy");
    let _ = std::fs::remove_dir_all(&root);
    let source = root.join("source");
    let destination = root.join("destination");
    std::fs::create_dir_all(&source).unwrap();
    let name = std::ffi::OsString::from_vec(vec![b'n', b'a', b'm', b'e', 0xff]);
    std::fs::write(source.join(&name), "value").unwrap();

    copy_path_recursive(&source, &destination).unwrap();

    assert_eq!(
        std::fs::read_to_string(destination.join(&name)).unwrap(),
        "value"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn file_tree_rename_path_updates_file_and_rejects_workspace_root() {
    let root = test_root("file_tree_rename");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let old = root.join("old.env");
    std::fs::write(&old, "x=1\n").unwrap();

    let new = rename_path(&old, "new.env", &[root.clone()]).unwrap();
    assert_eq!(new, root.join("new.env"));
    assert!(!old.exists());
    assert!(new.exists());
    assert!(rename_path(&root, "renamed-root", &[root.clone()]).is_err());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn file_tree_path_after_rename_updates_nested_open_paths() {
    let old_root = PathBuf::from("/tmp/ws/package");
    let new_root = PathBuf::from("/tmp/ws/package2");
    assert_eq!(
        path_after_rename(&old_root.join("src/main.rs"), &old_root, &new_root),
        Some(new_root.join("src/main.rs"))
    );
    assert_eq!(
        path_after_rename(Path::new("/tmp/ws/other.rs"), &old_root, &new_root),
        None
    );
}

#[test]
fn file_tree_selected_paths_preserve_visible_tree_order() {
    let root = PathBuf::from("/tmp/ws");
    let a = FileNode {
        path: root.join("a.rs"),
        name: "a.rs".to_string(),
        depth: 1,
        is_dir: false,
        is_expanded: false,
        icon_key: "default_file",
        is_ignored: false,
    };
    let b = FileNode {
        path: root.join("b.rs"),
        name: "b.rs".to_string(),
        depth: 1,
        is_dir: false,
        is_expanded: false,
        icon_key: "default_file",
        is_ignored: false,
    };
    let mut selection = FxHashSet::default();
    selection.insert(b.path.clone());
    selection.insert(a.path.clone());

    assert_eq!(
        selected_paths(&[a.clone(), b.clone()], &selection, &root),
        vec![a.path, b.path]
    );
    assert_eq!(
        selected_paths(&[], &FxHashSet::default(), &root),
        vec![root]
    );
}

#[test]
fn file_tree_cross_volume_move_removes_staging_on_success() {
    let root = test_root("file_tree_cross_volume_move_success");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("source.txt");
    let destination = root.join("destination.txt");
    std::fs::write(&source, "value").unwrap();

    cross_volume_move(&source, &destination).unwrap();

    assert!(!source.exists());
    assert_eq!(std::fs::read_to_string(&destination).unwrap(), "value");
    assert!(!std::fs::read_dir(&root)
        .unwrap()
        .any(|entry| entry.unwrap().file_name().to_string_lossy().starts_with(".rriter-move-")));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn file_tree_cross_volume_move_restores_source_after_copy_failure() {
    let root = test_root("file_tree_cross_volume_move_rollback");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("source.txt");
    let destination = root.join("missing").join("destination.txt");
    std::fs::write(&source, "value").unwrap();

    let error = cross_volume_move(&source, &destination).unwrap_err();

    assert!(!error.is_empty());
    assert_eq!(std::fs::read_to_string(&source).unwrap(), "value");
    assert!(!destination.exists());
    assert!(!std::fs::read_dir(&root)
        .unwrap()
        .any(|entry| entry.unwrap().file_name().to_string_lossy().starts_with(".rriter-move-")));
    let _ = std::fs::remove_dir_all(root);
}
