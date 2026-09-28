fn init_git_reconcile_test_repo(
    name: &str,
) -> (PathBuf, git2::Repository, PathBuf) {
    init_git_reconcile_test_repo_with_text(name, "A\n")
}

fn init_git_reconcile_test_repo_with_text(
    name: &str,
    initial_text: &str,
) -> (PathBuf, git2::Repository, PathBuf) {
    let unique = format!(
        "rriter-git-reconcile-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let root = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&root).unwrap();
    let repo = git2::Repository::init(&root).unwrap();
    let path = root.join("tracked.txt");
    std::fs::write(&path, initial_text).unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("tracked.txt")).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = git2::Signature::now("RRiter Test", "rriter@example.invalid").unwrap();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        "initial",
        &tree,
        &[],
    )
    .unwrap();
    drop(tree);
    drop(index);
    (root, repo, path)
}

fn configure_git_reconcile_test_app(
    app: &mut App,
    root: &std::path::Path,
    path: &std::path::Path,
    editor_text: &str,
    staged: bool,
) {
    app.is_ide_mode = true;
    app.show_welcome = false;
    app.file_path = Some(path.to_path_buf());
    app.file_key = Some(crate::platform::PathKey::new(path));
    app.file_extension = "txt".to_string();
    app.text_file_format = crate::platform::TextFileFormat {
        encoding: crate::platform::TextEncoding::Utf8,
        line_ending: crate::platform::LineEnding::Lf,
    };
    app.ide_workspaces = vec![root.to_path_buf()];
    app.editor = editor_with(editor_text);
    app.ide_panel.git.snapshot = crate::app::git_panel::GitStatusSnapshot {
        workspaces: vec![crate::app::git_panel::GitWorkspaceStatus {
            workspace_idx: 0,
            root: root.to_path_buf(),
            repo_root: Some(root.to_path_buf()),
            branch_name: None,
            files: vec![crate::app::git_panel::GitFileEntry {
                workspace_idx: 0,
                rel_path: "tracked.txt".into(),
                old_rel_path: None,
                display_path: "tracked.txt".into(),
                depth: 0,
                staged,
                status: crate::app::git_panel::GitFileStatus::Modified,
            }],
            tree: Vec::new(),
            ahead: 0,
            error: None,
        }],
    };
}

fn git_reconcile_test_wait_for_clean(app: &mut App, repo: &git2::Repository) -> bool {
    for _ in 0..200 {
        app.poll_git_panel();
        let repo_clean = repo.statuses(None).unwrap().is_empty();
        let panel_clean = app
            .ide_panel
            .git
            .snapshot
            .workspaces
            .first()
            .is_some_and(|workspace| workspace.files.is_empty());
        if repo_clean && panel_clean {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    false
}

fn git_reconcile_test_wait_for_staged(app: &mut App, repo: &git2::Repository) -> bool {
    for _ in 0..200 {
        app.poll_git_panel();
        if repo
            .status_file(std::path::Path::new("tracked.txt"))
            .is_ok_and(|status| status.contains(git2::Status::INDEX_MODIFIED))
            && !app.ide_panel.git.pending
        {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    false
}

fn git_reconcile_test_wait_for_settled_clean(
    app: &mut App,
    repo: &git2::Repository,
) -> bool {
    for _ in 0..200 {
        app.poll_git_panel();
        let repo_clean = repo.statuses(None).unwrap().is_empty();
        let panel_clean = app
            .ide_panel
            .git
            .snapshot
            .workspaces
            .first()
            .is_some_and(|workspace| workspace.files.is_empty());
        if repo_clean
            && panel_clean
            && !app.ide_panel.git.pending
            && !app.ide_panel.git.status_loading()
            && app.ide_panel.git.applied_status_request_id_for_test()
                == app.ide_panel.git.latest_request_id
        {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    false
}

#[test]
fn successful_save_preserves_preexisting_intentional_staged_modified() {
    let Some(mut app) = test_app() else {
        return;
    };
    let (root, repo, path) = init_git_reconcile_test_repo("preexisting");

    std::fs::write(&path, b"B\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("tracked.txt")).unwrap();
    index.write().unwrap();
    let staged_id = index
        .get_path(std::path::Path::new("tracked.txt"), 0)
        .unwrap()
        .id;
    drop(index);
    std::fs::write(&path, b"A\n").unwrap();

    configure_git_reconcile_test_app(&mut app, &root, &path, "A\n", true);
    app.editor.set_git_base_text(Some("A\n".to_string()));
    assert!(app.editor.git_hunks.is_empty());
    assert!(!app
        .ide_panel
        .git
        .has_stage_reconcile_candidate_for_test(&root, "tracked.txt"));
    let request_id = app.ide_panel.git.latest_request_id;

    assert!(app.save_current_file());
    assert_eq!(app.ide_panel.git.latest_request_id, request_id);
    for _ in 0..20 {
        app.poll_git_panel();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }

    let current_id = repo
        .index()
        .unwrap()
        .get_path(std::path::Path::new("tracked.txt"), 0)
        .unwrap()
        .id;
    assert_eq!(current_id, staged_id);
    let status = repo.status_file(std::path::Path::new("tracked.txt")).unwrap();
    assert!(status.contains(git2::Status::INDEX_MODIFIED));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "A\n");

    drop(app);
    drop(repo);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn successful_save_preserves_preexisting_partial_stage() {
    let Some(mut app) = test_app() else {
        return;
    };
    let (root, repo, path) =
        init_git_reconcile_test_repo_with_text("preexisting-partial", "one\ntwo\n");

    // Simulate an externally prepared partial index: the original worktree had
    // two changed lines, but only the first line was staged.
    std::fs::write(&path, b"ONE\nTWO\n").unwrap();
    std::fs::write(&path, b"ONE\ntwo\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("tracked.txt")).unwrap();
    index.write().unwrap();
    let staged_id = index
        .get_path(std::path::Path::new("tracked.txt"), 0)
        .unwrap()
        .id;
    drop(index);
    std::fs::write(&path, b"one\ntwo\n").unwrap();

    configure_git_reconcile_test_app(&mut app, &root, &path, "one\ntwo\n", true);
    app.editor
        .set_git_base_text(Some("one\ntwo\n".to_string()));
    assert!(app.editor.git_hunks.is_empty());
    assert!(!app
        .ide_panel
        .git
        .has_stage_reconcile_candidate_for_test(&root, "tracked.txt"));
    let request_id = app.ide_panel.git.latest_request_id;

    assert!(app.save_current_file());
    assert_eq!(app.ide_panel.git.latest_request_id, request_id);
    for _ in 0..20 {
        app.poll_git_panel();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }

    let current_id = repo
        .index()
        .unwrap()
        .get_path(std::path::Path::new("tracked.txt"), 0)
        .unwrap()
        .id;
    assert_eq!(current_id, staged_id);
    assert_eq!(
        repo.status_file(std::path::Path::new("tracked.txt")).unwrap()
            & git2::Status::INDEX_MODIFIED,
        git2::Status::INDEX_MODIFIED
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "one\ntwo\n");

    drop(app);
    drop(repo);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn rriter_owned_stage_reconciles_after_successful_save_back_to_head() {
    let Some(mut app) = test_app() else {
        return;
    };
    let (root, repo, path) = init_git_reconcile_test_repo("owned");
    std::fs::write(&path, b"B\n").unwrap();
    configure_git_reconcile_test_app(&mut app, &root, &path, "B\n", false);
    app.editor.set_git_base_text(Some("A\n".to_string()));
    assert_eq!(app.editor.git_hunks.len(), 1);

    app.toggle_git_file_stage(0, 0);
    assert!(app
        .ide_panel
        .git
        .has_stage_reconcile_candidate_for_test(&root, "tracked.txt"));

    let mut staged = false;
    for _ in 0..200 {
        app.poll_git_panel();
        if repo
            .status_file(std::path::Path::new("tracked.txt"))
            .is_ok_and(|status| status.contains(git2::Status::INDEX_MODIFIED))
            && !app.ide_panel.git.pending
        {
            staged = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(staged);
    assert!(app
        .ide_panel
        .git
        .has_stage_reconcile_candidate_for_test(&root, "tracked.txt"));

    app.editor = editor_with("A\n");
    app.editor.set_git_base_text(Some("A\n".to_string()));
    assert!(app.editor.git_hunks.is_empty());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "B\n");
    assert!(app.save_current_file());
    assert!(!app
        .ide_panel
        .git
        .has_stage_reconcile_candidate_for_test(&root, "tracked.txt"));
    assert!(git_reconcile_test_wait_for_clean(&mut app, &repo));

    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let tree = head.tree().unwrap();
    let head_id = tree
        .get_path(std::path::Path::new("tracked.txt"))
        .unwrap()
        .id();
    let index_id = repo
        .index()
        .unwrap()
        .get_path(std::path::Path::new("tracked.txt"), 0)
        .unwrap()
        .id;
    assert_eq!(index_id, head_id);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "A\n");

    drop(tree);
    drop(head);
    drop(app);
    drop(repo);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn reconcile_is_blocking_until_index_mutation_finishes() {
    let Some(mut app) = test_app() else {
        return;
    };
    let (root, repo, path) = init_git_reconcile_test_repo("blocking");
    std::fs::write(&path, b"B\n").unwrap();
    configure_git_reconcile_test_app(&mut app, &root, &path, "B\n", false);
    app.editor.set_git_base_text(Some("A\n".to_string()));

    app.toggle_git_file_stage(0, 0);
    assert!(git_reconcile_test_wait_for_staged(&mut app, &repo));

    app.editor = editor_with("A\n");
    app.editor.set_git_base_text(Some("A\n".to_string()));
    assert!(app.save_current_file());

    assert!(
        app.ide_panel.git.pending,
        "reconcile mutates the index and must keep the existing Git pending contract active"
    );
    let reconcile_request_id = app.ide_panel.git.latest_request_id;

    let _ = app.ide_panel.git.message_editor.insert_str("ready");
    assert!(app.ide_panel.git.commit_enabled());
    app.commit_git_panel();
    assert_eq!(app.ide_panel.git.latest_request_id, reconcile_request_id);
    app.toggle_git_file_stage(0, 0);
    assert_eq!(app.ide_panel.git.latest_request_id, reconcile_request_id);
    assert!(app.ide_panel.git.snapshot.workspaces[0].files[0].staged);

    assert!(git_reconcile_test_wait_for_settled_clean(&mut app, &repo));
    assert!(!app.ide_panel.git.pending);

    drop(app);
    drop(repo);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn manual_refresh_does_not_supersede_outstanding_reconcile() {
    let Some(mut app) = test_app() else {
        return;
    };
    let (root, repo, path) = init_git_reconcile_test_repo("manual-refresh-ordering");
    std::fs::write(&path, b"B\n").unwrap();
    configure_git_reconcile_test_app(&mut app, &root, &path, "B\n", false);
    app.editor.set_git_base_text(Some("A\n".to_string()));

    app.toggle_git_file_stage(0, 0);
    assert!(git_reconcile_test_wait_for_staged(&mut app, &repo));

    app.editor = editor_with("A\n");
    app.editor.set_git_base_text(Some("A\n".to_string()));
    assert!(app.save_current_file());
    let reconcile_request_id = app.ide_panel.git.latest_request_id;
    assert!(app.ide_panel.git.pending);
    assert!(app.ide_panel.git.status_mutation_in_flight_for_test());

    app.refresh_git_panel_window();
    assert_eq!(
        app.ide_panel.git.latest_request_id,
        reconcile_request_id,
        "manual refresh must not supersede an outstanding reconcile mutation"
    );
    assert!(
        app.ide_panel.git.status_mutation_in_flight_for_test(),
        "manual refresh must not discard the outstanding reconcile receiver"
    );
    assert!(app.ide_panel.git.pending);

    assert!(git_reconcile_test_wait_for_settled_clean(&mut app, &repo));
    assert!(repo.statuses(None).unwrap().is_empty());
    assert!(app.ide_panel.git.snapshot.workspaces[0].files.is_empty());

    drop(app);
    drop(repo);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn manual_refresh_preserves_reconcile_blocking_guard() {
    let Some(mut app) = test_app() else {
        return;
    };
    let (root, repo, path) = init_git_reconcile_test_repo("manual-refresh-blocking");
    std::fs::write(&path, b"B\n").unwrap();
    configure_git_reconcile_test_app(&mut app, &root, &path, "B\n", false);
    app.editor.set_git_base_text(Some("A\n".to_string()));

    app.toggle_git_file_stage(0, 0);
    assert!(git_reconcile_test_wait_for_staged(&mut app, &repo));

    app.editor = editor_with("A\n");
    app.editor.set_git_base_text(Some("A\n".to_string()));
    assert!(app.save_current_file());
    let reconcile_request_id = app.ide_panel.git.latest_request_id;
    assert!(app.ide_panel.git.pending);
    assert!(app.ide_panel.git.status_mutation_in_flight_for_test());

    let _ = app.ide_panel.git.message_editor.insert_str("ready");
    assert!(
        app.ide_panel.git.commit_enabled(),
        "the staged snapshot must otherwise allow Commit so this test exercises the pending guard"
    );
    app.refresh_git_panel_window();
    assert!(app.ide_panel.git.pending);
    assert!(app.ide_panel.git.status_mutation_in_flight_for_test());

    app.commit_git_panel();
    assert_eq!(
        app.ide_panel.git.latest_request_id,
        reconcile_request_id,
        "manual refresh must not remove the pending guard and allow Commit to start"
    );

    assert!(git_reconcile_test_wait_for_settled_clean(&mut app, &repo));
    assert!(!app.ide_panel.git.pending);

    drop(app);
    drop(repo);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn refresh_during_reconcile_is_deferred_and_coalesced_without_resurrecting_staged_row() {
    let Some(mut app) = test_app() else {
        return;
    };
    let (root, repo, path) = init_git_reconcile_test_repo("refresh-ordering");
    std::fs::write(&path, b"B\n").unwrap();
    configure_git_reconcile_test_app(&mut app, &root, &path, "B\n", false);
    app.editor.set_git_base_text(Some("A\n".to_string()));

    app.toggle_git_file_stage(0, 0);
    assert!(git_reconcile_test_wait_for_staged(&mut app, &repo));

    app.editor = editor_with("A\n");
    app.editor.set_git_base_text(Some("A\n".to_string()));
    assert!(app.save_current_file());
    let reconcile_request_id = app.ide_panel.git.latest_request_id;
    assert!(app.ide_panel.git.pending);

    app.refresh_git_panel();
    app.refresh_git_panel();
    assert_eq!(
        app.ide_panel.git.latest_request_id,
        reconcile_request_id,
        "refresh during an outstanding index mutation must be deferred without superseding it"
    );

    assert!(git_reconcile_test_wait_for_settled_clean(&mut app, &repo));
    assert_eq!(
        app.ide_panel.git.latest_request_id,
        reconcile_request_id + 1,
        "multiple deferred refresh requests must coalesce into one authoritative refresh"
    );
    assert_eq!(
        app.ide_panel.git.applied_status_request_id_for_test(),
        app.ide_panel.git.latest_request_id
    );
    assert!(app.ide_panel.git.snapshot.workspaces[0].files.is_empty());

    drop(app);
    drop(repo);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn file_watcher_like_refresh_after_reconcile_save_settles_repo_and_panel_clean() {
    let Some(mut app) = test_app() else {
        return;
    };
    let (root, repo, path) = init_git_reconcile_test_repo("watcher-refresh");
    std::fs::write(&path, b"B\n").unwrap();
    configure_git_reconcile_test_app(&mut app, &root, &path, "B\n", false);
    app.editor.set_git_base_text(Some("A\n".to_string()));

    app.toggle_git_file_stage(0, 0);
    assert!(git_reconcile_test_wait_for_staged(&mut app, &repo));

    app.editor = editor_with("A\n");
    app.editor.set_git_base_text(Some("A\n".to_string()));
    assert!(app.save_current_file());
    let reconcile_request_id = app.ide_panel.git.latest_request_id;

    app.refresh_git_panel();
    assert_eq!(app.ide_panel.git.latest_request_id, reconcile_request_id);
    assert!(git_reconcile_test_wait_for_settled_clean(&mut app, &repo));

    assert!(repo.statuses(None).unwrap().is_empty());
    assert!(app.ide_panel.git.snapshot.workspaces[0].files.is_empty());
    assert_eq!(
        app.ide_panel.git.applied_status_request_id_for_test(),
        app.ide_panel.git.latest_request_id
    );

    drop(app);
    drop(repo);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn staged_undo_back_to_head_reconciles_after_focus_autosave_through_stage_queue() {
    let Some(mut app) = test_app() else {
        return;
    };
    let (root, repo, path) = init_git_reconcile_test_repo("fast-race");

    std::fs::write(&path, b"B\n").unwrap();
    configure_git_reconcile_test_app(&mut app, &root, &path, "A\n", false);
    app.editor.selection_anchor = Some(0);
    app.editor.cursor = app.editor.len();
    let _ = app.editor.insert_str("B\n");
    app.editor.mark_saved();
    app.editor.set_git_base_text(Some("A\n".to_string()));
    assert!(!app.editor.is_dirty());
    assert_eq!(app.editor.git_hunks.len(), 1);

    app.toggle_git_file_stage(0, 0);
    assert!(app.ide_panel.git.snapshot.workspaces[0].files[0].staged);
    assert!(app
        .ide_panel
        .git
        .has_stage_reconcile_candidate_for_test(&root, "tracked.txt"));
    let stage_request_id = app.ide_panel.git.latest_request_id;

    assert!(app.editor.undo().is_some());
    assert_eq!(app.editor.get_full_text(), "A\n");
    assert!(app.editor.is_dirty());
    assert!(app.editor.git_hunks.is_empty());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "B\n");
    assert_eq!(app.ide_panel.git.latest_request_id, stage_request_id);

    app.ide_panel.open(crate::app::PanelId::Git);
    app.ide_panel.git.message_focused = true;
    assert!(app.autosave_after_editor_focus_change(true));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "A\n");
    assert!(app.ide_panel.git.latest_request_id > stage_request_id);
    assert!(!app
        .ide_panel
        .git
        .has_stage_reconcile_candidate_for_test(&root, "tracked.txt"));
    assert!(git_reconcile_test_wait_for_clean(&mut app, &repo));

    drop(app);
    drop(repo);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn git_file_label_double_click_opens_diff_not_stage() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.is_ide_mode = true;
    app.ide_panel.git.snapshot = crate::app::git_panel::GitStatusSnapshot {
        workspaces: vec![crate::app::git_panel::GitWorkspaceStatus {
            workspace_idx: 0,
            root: PathBuf::from("/workspace"),
            repo_root: Some(PathBuf::from("/workspace")),
            branch_name: None,
            files: vec![crate::app::git_panel::GitFileEntry {
                workspace_idx: 0,
                rel_path: "src/main.rs".into(),
                old_rel_path: None,
                display_path: "src/main.rs".into(),
                depth: 1,
                staged: false,
                status: crate::app::git_panel::GitFileStatus::Modified,
            }],
            tree: Vec::new(),
            ahead: 0,
            error: None,
        }],
    };

    app.handle_ui_click(crate::ui_system::UiId::GitFileDiff(0, 0));
    assert!(!app.ide_panel.git.snapshot.workspaces[0].files[0].staged);
    assert_eq!(app.ide_panel.git.selected_file, Some((0, 0)));
    assert!(app.tabs.is_empty());

    app.handle_ui_click(crate::ui_system::UiId::GitFileDiff(0, 0));
    assert!(!app.ide_panel.git.snapshot.workspaces[0].files[0].staged);
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.base_title, "Diff: main.rs");
    assert!(matches!(
        &app.tabs[0].kind,
        crate::app::EditorTabKind::GitDiff(_, _)
    ));
}

#[test]
fn diff_tab_dedup_by_repo_path() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.is_ide_mode = true;
    app.ide_panel.git.snapshot = crate::app::git_panel::GitStatusSnapshot {
        workspaces: vec![crate::app::git_panel::GitWorkspaceStatus {
            workspace_idx: 0,
            root: PathBuf::from("/workspace"),
            repo_root: Some(PathBuf::from("/workspace")),
            branch_name: None,
            files: vec![crate::app::git_panel::GitFileEntry {
                workspace_idx: 0,
                rel_path: "src/main.rs".into(),
                old_rel_path: None,
                display_path: "src/main.rs".into(),
                depth: 1,
                staged: false,
                status: crate::app::git_panel::GitFileStatus::Modified,
            }],
            tree: Vec::new(),
            ahead: 0,
            error: None,
        }],
    };

    app.open_git_diff_tab(0, 0);
    app.open_git_diff_tab(0, 0);
    assert_eq!(app.tabs.len(), 1);
}

#[test]
fn undo_redo_rebuilds_diff_decorations() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.is_ide_mode = true;
    let mut state =
        crate::app::git_diff::build_diff_view("a\nold\n".to_string(), "a\nnew\n".to_string());
    state.version = 1;
    let text = state.displayed_text.clone();
    app.editor = editor_with(&text);
    app.editor.set_original_text();
    app.base_title = "Diff: main.rs".to_string();
    app.tabs.push(EditorTab {
        editor: editor_with(&text),
        file_path: None,
        file_key: None,
        text_file_format: crate::platform::TextFileFormat::default(),
        base_title: "Diff: main.rs".to_string(),
        file_extension: "rs".to_string(),
        markdown: Default::default(),
        scroll_y: crate::scroll::ScrollState::new(15.0),
        scroll_x: crate::scroll::ScrollState::new(15.0),
        spans: Vec::new(),
        completions: Vec::new(),
        foldable_ranges: Vec::new(),
        last_sent_version: 0,
        search_results: Vec::new(),
        search_current_idx: None,
        is_highlighted_once: true,
        is_highlight_complete: true,
        icon_key: "default_file",
        syntax_errors: Vec::new(),
        closing_hints: Default::default(),
        deleted: false,
        kind: crate::app::EditorTabKind::GitDiff(
            crate::app::git_diff::GitDiffTabMeta {
                repo_root: PathBuf::from("/workspace"),
                rel_path: "src/main.rs".to_string(),
                old_rel_path: None,
                status: crate::app::git_panel::GitFileStatus::Modified,
                workspace_idx: 0,
            },
            state,
        ),
    });

    app.rollback_active_git_diff_hunk(0);
    assert!(app.active_git_diff_state().unwrap().hunks.is_empty());
    app.editor.undo();
    app.rebuild_active_git_diff_from_editor_after_history(true);
    assert_eq!(app.active_git_diff_state().unwrap().hunks.len(), 1);
    app.editor.redo();
    app.rebuild_active_git_diff_from_editor_after_history(false);
    assert!(app.active_git_diff_state().unwrap().hunks.is_empty());
}

#[test]
fn inline_git_rollback_undo_restores_cursor_to_changed_block_end() {
    let Some(mut app) = test_app() else {
        return;
    };
    let base = "a\nold\nz\n";
    let changed = "a\nnew\nz\n";
    app.editor = editor_with(changed);
    app.editor.cursor = 0;
    app.editor.set_git_base_text(Some(base.to_string()));
    assert_eq!(app.editor.git_hunks.len(), 1);
    app.highlighter.spans = vec![crate::highlighter::ColorSpan {
        start: 0,
        end: 1,
        color: crate::highlighter::DRACULA_FG,
    }];
    app.inline_git_popup = Some(crate::app::InlineGitPopup {
        hunk_idx: 0,
        anchor_line: 1,
        lines: Vec::new(),
        spans: Vec::new(),
        diff_state: crate::app::git_diff::build_diff_view(base.to_string(), changed.to_string()),
    });

    app.rollback_inline_git_hunk();

    assert_eq!(app.editor.get_full_text(), base);
    assert!(!app.highlighter.spans.is_empty());
    app.editor.undo();
    assert_eq!(app.editor.get_full_text(), changed);
    assert_eq!(app.editor.cursor, "a\nnew\n".len());
}

#[test]
fn ui_handlers_search_problem_log_and_diagnostic_actions_are_headless_safe() {
    let Some(mut app) = test_app() else {
        return;
    };

    app.editor = editor_with("alpha beta alpha");
    app.editor.cursor = 0;
    app.search_editor = editor_with("alpha");
    app.show_search = true;
    app.search_focused = true;
    app.update_search();
    assert_eq!(app.search_current_idx, Some(0));

    app.handle_ui_click(crate::ui_system::UiId::SearchNext);
    assert_eq!(app.search_current_idx, Some(1));
    assert_eq!(app.editor.selection_anchor, Some(11));
    assert_eq!(app.editor.cursor, 16);

    app.handle_ui_click(crate::ui_system::UiId::SearchPrev);
    assert_eq!(app.search_current_idx, Some(0));
    assert_eq!(app.editor.selection_anchor, Some(0));
    assert_eq!(app.editor.cursor, 5);

    app.handle_ui_click(crate::ui_system::UiId::SearchCaseToggle);
    assert!(app.search_case_sensitive);
    app.handle_ui_click(crate::ui_system::UiId::SearchClose);
    assert!(!app.show_search);
    assert!(!app.search_focused);
    assert!(app.search_results.is_empty());
    assert_eq!(app.search_current_idx, None);

    let path = PathBuf::from("/tmp/main.py");
    app.ide_panel.flat_diags.push((path.clone(), usize::MAX));
    app.handle_ui_click(crate::ui_system::UiId::ProblemFileToggle(0));
    assert!(app.ide_panel.problems_collapsed.contains(&path));
    app.handle_ui_click(crate::ui_system::UiId::ProblemFileToggle(0));
    assert!(!app.ide_panel.problems_collapsed.contains(&path));

    app.handle_ui_click(crate::ui_system::UiId::ProblemsTab(2));
    assert_eq!(app.ide_panel.problems_tab, 2);

    app.ide_panel.lsp_servers = vec![crate::lsp::LspServerInfo {
        name: "ruff",
        status: crate::lsp::LspServerStatus::Running,
        logs: Vec::new(),
    }];
    let mut log_editor = editor_with("line one\nline two\n");
    log_editor.selection_anchor = Some(0);
    app.ide_panel
        .lsp_log_editors
        .insert("ruff".to_string(), log_editor);

    app.handle_ui_click(crate::ui_system::UiId::LspLogArea(0));
    assert_eq!(app.ide_panel.lsp_logs_focused.as_deref(), Some("ruff"));
    assert!(app.is_dragging_lsp_log);
    assert_eq!(
        app.ide_panel
            .lsp_log_editors
            .get("ruff")
            .and_then(|ed| ed.selection_anchor),
        None
    );
}

