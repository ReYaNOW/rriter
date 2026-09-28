    #[test]
    fn git_workspace_collapse_button_only_when_rows_exist() {
        assert!(!git_workspace(Vec::new(), None).has_collapsible_rows());
        assert!(
            git_workspace(Vec::new(), Some("git status failed".to_string())).has_collapsible_rows()
        );
        assert!(
            git_workspace(
                vec![git_file("src/main.rs", false, GitFileStatus::Modified)],
                None
            )
            .has_collapsible_rows()
        );
    }

    #[test]
    fn staged_repo_roots_use_active_workspace_and_dedupe_roots() {
        let repo_a = PathBuf::from("/repo/a");
        let repo_b = PathBuf::from("/repo/b");
        let snapshot = GitStatusSnapshot {
            workspaces: vec![
                GitWorkspaceStatus {
                    workspace_idx: 0,
                    root: PathBuf::from("/ws/a"),
                    repo_root: Some(repo_a.clone()),
                    branch_name: None,
                    files: vec![
                        GitFileEntry {
                            staged: true,
                            ..git_file("src/main.rs", true, GitFileStatus::Added)
                        },
                        GitFileEntry {
                            staged: true,
                            ..git_file("src/lib.rs", true, GitFileStatus::Modified)
                        },
                    ],
                    tree: Vec::new(),
                    ahead: 0,
                    error: None,
                },
                GitWorkspaceStatus {
                    workspace_idx: 1,
                    root: PathBuf::from("/ws/b"),
                    repo_root: Some(repo_b.clone()),
                    branch_name: None,
                    files: vec![GitFileEntry {
                        workspace_idx: 1,
                        rel_path: "other.rs".into(),
                        old_rel_path: None,
                        display_path: "other.rs".into(),
                        depth: 0,
                        staged: true,
                        status: GitFileStatus::Added,
                    }],
                    tree: Vec::new(),
                    ahead: 0,
                    error: None,
                },
            ],
        };

        assert_eq!(snapshot.active_staged_workspace_idx(), Some(0));
        assert_eq!(snapshot.staged_repo_roots(), vec![repo_a]);
    }

    #[test]
    fn staged_workspace_lock_keeps_pending_workspace_stable() {
        let mut state = GitPanelState::default();
        state.stage_pending_workspace_idx = Some(0);
        state.snapshot = GitStatusSnapshot {
            workspaces: vec![GitWorkspaceStatus {
                workspace_idx: 1,
                root: PathBuf::from("/ws/b"),
                repo_root: Some(PathBuf::from("/repo/b")),
                branch_name: None,
                files: vec![GitFileEntry {
                    workspace_idx: 1,
                    rel_path: "other.rs".into(),
                    old_rel_path: None,
                    display_path: "other.rs".into(),
                    depth: 0,
                    staged: true,
                    status: GitFileStatus::Added,
                }],
                tree: Vec::new(),
                ahead: 0,
                error: None,
            }],
        };

        assert_eq!(state.staged_workspace_lock(), Some(0));

        state.snapshot.workspaces[0].files[0].staged = false;
        assert_eq!(state.staged_workspace_lock(), Some(0));
    }

    #[test]
    fn commit_event_clears_message_editor() {
        let mut state = GitPanelState::default();
        let _ = state.message_editor.insert_str("ready");
        state.message_focused = true;

        state.apply_event(GitPanelEvent {
            request_id: 3,
            snapshot: GitStatusSnapshot::default(),
            notice: Some("Committed 1 repo(s)".to_string()),
            preserve_snapshot_on_empty: false,
            clear_message: true,
            refresh_graph: true,
            transaction_failed: false,
        });

        assert_eq!(state.message_editor.get_full_text(), "");
        assert!(!state.message_focused);
    }

    #[test]
    fn git_stage_click_locked_blocks_pending_and_other_workspace() {
        let mut state = GitPanelState::default();
        state.pending = true;
        state.stage_pending_workspace_idx = Some(0);

        assert!(git_stage_click_locked(&state, 0));
        assert!(git_stage_click_locked(&state, 1));

        state.pending = false;
        assert!(!git_stage_click_locked(&state, 0));
        assert!(git_stage_click_locked(&state, 1));
    }

    #[test]
    fn git_commit_visual_stays_enabled_while_stage_task_pending() {
        let mut state = GitPanelState::default();
        state.stage_pending_workspace_idx = Some(0);
        state.snapshot = GitStatusSnapshot {
            workspaces: vec![git_workspace(vec![git_file(
                "src/main.rs",
                true,
                GitFileStatus::Modified,
            )], None)],
        };

        assert!(state.commit_enabled());
        state.stage_pending_workspace_idx = None;
        assert!(state.commit_enabled());
    }

    #[test]
    fn git_status_refresh_state_coalesces_dirty_rerun() {
        let mut state = GitPanelState::default();

        assert!(state.begin_status_refresh());
        assert!(state.status_refresh_pending);
        assert!(!state.status_refresh_dirty);

        assert!(!state.begin_status_refresh());
        assert!(state.status_refresh_pending);
        assert!(state.status_refresh_dirty);

        assert!(state.finish_status_refresh());
        assert!(!state.status_refresh_pending);
        assert!(!state.status_refresh_dirty);

        assert!(state.begin_status_refresh());
        assert!(state.status_refresh_pending);
        assert!(!state.status_refresh_dirty);
    }

    #[test]
    fn git_status_pathspec_limits_workspace_subdir_results() {
        let root = temp_git_root("pathspec");
        let workspace = root.join("sub");
        std::fs::create_dir_all(&workspace).unwrap();
        let _repo = git2::Repository::init(&root).unwrap();
        std::fs::write(root.join("outside.txt"), "outside\n").unwrap();
        std::fs::write(workspace.join("inside.txt"), "inside\n").unwrap();

        let status = collect_workspace_status(7, &workspace);

        assert_eq!(status.workspace_idx, 7);
        assert_eq!(status.files.len(), 1);
        assert_eq!(status.files[0].display_path.as_ref(), "inside.txt");
        assert_eq!(status.files[0].rel_path.as_ref(), "sub/inside.txt");

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn git_branch_ahead_cache_keys_by_head_and_upstream_oid() {
        let root = temp_git_root("ahead_cache");
        std::fs::create_dir_all(&root).unwrap();
        let repo = git2::Repository::init(&root).unwrap();
        std::fs::write(root.join("a.txt"), "a\n").unwrap();
        toggle_stage(&root, "a.txt", None, false).unwrap();
        commit_repo(&root, "initial", false).unwrap();

        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let branch_name = repo.head().unwrap().shorthand().unwrap().to_string();
        repo.remote("origin", root.to_str().unwrap()).unwrap();
        repo.reference(
            &format!("refs/remotes/origin/{branch_name}"),
            head.id(),
            true,
            "test remote ref",
        )
        .unwrap();
        let mut config = repo.config().unwrap();
        config
            .set_str(&format!("branch.{branch_name}.remote"), "origin")
            .unwrap();
        config
            .set_str(
                &format!("branch.{branch_name}.merge"),
                &format!("refs/heads/{branch_name}"),
            )
            .unwrap();

        let mut cache = BranchAheadCache::default();
        assert_eq!(branch_ahead_cached(&repo, &root, &mut cache).unwrap(), 0);
        assert_eq!(cache.len(), 1);
        assert_eq!(branch_ahead_cached(&repo, &root, &mut cache).unwrap(), 0);
        assert_eq!(cache.len(), 1);

        std::fs::write(root.join("b.txt"), "b\n").unwrap();
        toggle_stage(&root, "b.txt", None, false).unwrap();
        commit_repo(&root, "second", false).unwrap();

        assert_eq!(branch_ahead_cached(&repo, &root, &mut cache).unwrap(), 1);
        assert_eq!(cache.len(), 2);
        assert_eq!(branch_ahead_cached(&repo, &root, &mut cache).unwrap(), 1);
        assert_eq!(cache.len(), 2);

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn stage_event_preserves_visible_topology_and_merges_existing_files() {
        let mut state = GitPanelState::default();
        state.snapshot = GitStatusSnapshot {
            workspaces: vec![GitWorkspaceStatus {
                workspace_idx: 0,
                root: PathBuf::from("/workspace"),
                repo_root: Some(PathBuf::from("/workspace")),
                branch_name: None,
                files: vec![git_file("tests/test_api.py", true, GitFileStatus::Modified)],
                tree: Vec::new(),
                ahead: 0,
                error: None,
            }],
        };

        state.apply_event(GitPanelEvent {
            request_id: 7,
            snapshot: GitStatusSnapshot {
                workspaces: vec![GitWorkspaceStatus {
                    workspace_idx: 0,
                    root: PathBuf::from("/workspace"),
                    repo_root: Some(PathBuf::from("/workspace")),
                    branch_name: None,
                    files: vec![
                        git_file("tests/test_api.py", false, GitFileStatus::Modified),
                        git_file(".dockerignore", true, GitFileStatus::Renamed),
                    ],
                    tree: Vec::new(),
                    ahead: 0,
                    error: None,
                }],
            },
            notice: None,
            preserve_snapshot_on_empty: true,
            clear_message: false,
            refresh_graph: false,
            transaction_failed: false,
        });

        assert_eq!(state.latest_request_id, 7);
        assert_eq!(state.snapshot.workspaces[0].files.len(), 1);
        assert!(!state.snapshot.workspaces[0].files[0].staged);
        assert_eq!(
            state.snapshot.workspaces[0].files[0].display_path.as_ref(),
            "tests/test_api.py"
        );

        state.apply_event(GitPanelEvent {
            request_id: 8,
            snapshot: GitStatusSnapshot {
                workspaces: vec![GitWorkspaceStatus {
                    workspace_idx: 0,
                    root: PathBuf::from("/workspace"),
                    repo_root: Some(PathBuf::from("/workspace")),
                    branch_name: None,
                    files: Vec::new(),
                    tree: Vec::new(),
                    ahead: 0,
                    error: None,
                }],
            },
            notice: None,
            preserve_snapshot_on_empty: false,
            clear_message: false,
            refresh_graph: false,
            transaction_failed: false,
        });

        assert!(state.snapshot.workspaces[0].files.is_empty());
    }

    #[test]
    fn stage_event_removes_clean_files_and_clears_pending_workspace() {
        let mut state = GitPanelState::default();
        state.stage_pending_workspace_idx = Some(0);
        state.snapshot = GitStatusSnapshot {
            workspaces: vec![GitWorkspaceStatus {
                workspace_idx: 0,
                root: PathBuf::from("/workspace"),
                repo_root: Some(PathBuf::from("/workspace")),
                branch_name: None,
                files: vec![git_file("tests/test_api.py", true, GitFileStatus::Modified)],
                tree: build_git_tree(&[git_file(
                    "tests/test_api.py",
                    true,
                    GitFileStatus::Modified,
                )]),
                ahead: 0,
                error: None,
            }],
        };

        state.apply_event(GitPanelEvent {
            request_id: 10,
            snapshot: GitStatusSnapshot {
                workspaces: vec![GitWorkspaceStatus {
                    workspace_idx: 0,
                    root: PathBuf::from("/workspace"),
                    repo_root: Some(PathBuf::from("/workspace")),
                    branch_name: Some("main".to_string()),
                    files: Vec::new(),
                    tree: Vec::new(),
                    ahead: 0,
                    error: None,
                }],
            },
            notice: None,
            preserve_snapshot_on_empty: true,
            clear_message: false,
            refresh_graph: false,
            transaction_failed: false,
        });

        assert!(state.snapshot.workspaces[0].files.is_empty());
        assert!(state.snapshot.workspaces[0].tree.is_empty());
        assert_eq!(
            state.snapshot.workspaces[0].branch_name.as_deref(),
            Some("main")
        );
        assert_eq!(state.stage_pending_workspace_idx, None);
    }

    #[test]
    fn stage_workspace_lock_preserves_topology_for_refresh_events_too() {
        let mut state = GitPanelState::default();
        state.stage_pending_workspace_idx = Some(0);
        state.snapshot = GitStatusSnapshot {
            workspaces: vec![GitWorkspaceStatus {
                workspace_idx: 0,
                root: PathBuf::from("/workspace"),
                repo_root: Some(PathBuf::from("/workspace")),
                branch_name: None,
                files: vec![git_file("tests/test_api.py", true, GitFileStatus::Modified)],
                tree: Vec::new(),
                ahead: 0,
                error: None,
            }],
        };

        state.apply_event(GitPanelEvent {
            request_id: 9,
            snapshot: GitStatusSnapshot {
                workspaces: vec![GitWorkspaceStatus {
                    workspace_idx: 0,
                    root: PathBuf::from("/workspace"),
                    repo_root: Some(PathBuf::from("/workspace")),
                    branch_name: None,
                    files: vec![
                        git_file("tests/test_api.py", false, GitFileStatus::Modified),
                        git_file(".dockerignore", true, GitFileStatus::Renamed),
                    ],
                    tree: Vec::new(),
                    ahead: 0,
                    error: None,
                }],
            },
            notice: None,
            preserve_snapshot_on_empty: false,
            clear_message: false,
            refresh_graph: false,
            transaction_failed: false,
        });

        assert_eq!(state.snapshot.workspaces[0].files.len(), 1);
        assert_eq!(
            state.snapshot.workspaces[0].files[0].display_path.as_ref(),
            "tests/test_api.py"
        );
        assert!(state.snapshot.workspaces[0].files[0].staged);
    }

    #[test]
    fn git_tree_builds_folder_rows_icons_and_collapse_counts() {
        let files = vec![
            git_file("README.md", false, GitFileStatus::Modified),
            git_file(".dockerignore", false, GitFileStatus::Modified),
            git_file("src/lib.rs", false, GitFileStatus::Modified),
            git_file("src/main.rs", true, GitFileStatus::Added),
        ];

        let rows = build_git_tree(&files);

        assert_eq!(
            rows.iter()
                .map(|row| (
                    row.name.as_ref(),
                    row.path.as_ref(),
                    row.depth,
                    row.file_idx
                ))
                .collect::<Vec<_>>(),
            vec![
                ("src", "src", 0, None),
                ("lib.rs", "src/lib.rs", 1, Some(2)),
                ("main.rs", "src/main.rs", 1, Some(3)),
                (".dockerignore", ".dockerignore", 0, Some(1)),
                ("README.md", "README.md", 0, Some(0)),
            ]
        );
        assert_ne!(rows[0].icon_key, "default");
        assert_ne!(rows[3].icon_key, "default_file");

        let mut collapsed = FxHashMap::default();
        assert_eq!(git_visible_tree_row_count(0, &rows, &collapsed), 5);
        collapsed.insert(0, FxHashSet::from_iter(["src".to_string()]));
        assert_eq!(git_visible_tree_row_count(0, &rows, &collapsed), 3);
        assert_eq!(git_visible_tree_row_count(1, &rows, &collapsed), 5);
    }

    #[test]
    fn git_folder_stage_state_uses_descendant_files_only() {
        let files = vec![
            git_file("src/lib.rs", false, GitFileStatus::Modified),
            git_file("src/main.rs", true, GitFileStatus::Added),
            git_file("src-extra/mod.rs", true, GitFileStatus::Added),
        ];
        let workspace = GitWorkspaceStatus {
            workspace_idx: 0,
            root: PathBuf::from("/workspace"),
            repo_root: Some(PathBuf::from("/workspace")),
            branch_name: None,
            tree: build_git_tree(&files),
            files,
            ahead: 0,
            error: None,
        };

        let src_idx = workspace
            .tree
            .iter()
            .position(|row| row.path.as_ref() == "src" && row.file_idx.is_none())
            .unwrap();
        let src_extra_idx = workspace
            .tree
            .iter()
            .position(|row| row.path.as_ref() == "src-extra" && row.file_idx.is_none())
            .unwrap();

        assert_eq!(
            git_folder_file_indices(&workspace, src_idx),
            vec![0usize, 1usize]
        );
        assert_eq!(
            git_folder_stage_state(&workspace, src_idx),
            Some(GitFolderStageState::Partial)
        );
        assert_eq!(
            git_folder_stage_state(&workspace, src_extra_idx),
            Some(GitFolderStageState::All)
        );
    }

    #[test]
    fn git_status_stage_and_commit_round_trip_uses_git_cli_commit() {
        let root = temp_git_root("round_trip");
        std::fs::create_dir_all(root.join("src")).unwrap();
        let repo = git2::Repository::init(&root).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();

        let workspace = collect_workspace_status(7, &root);
        assert_eq!(workspace.workspace_idx, 7);
        assert_eq!(workspace.files.len(), 1);
        assert_eq!(workspace.files[0].display_path.as_ref(), "src/main.rs");
        assert!(!workspace.files[0].staged);
        assert_eq!(workspace.files[0].status, GitFileStatus::Untracked);
        assert_eq!(
            workspace
                .tree
                .iter()
                .map(|row| row.name.as_ref())
                .collect::<Vec<_>>(),
            vec!["src", "main.rs"]
        );

        toggle_stage(&root, "src/main.rs", None, false).unwrap();
        let workspace = collect_workspace_status(7, &root);
        assert!(workspace.files[0].staged);
        assert_eq!(workspace.files[0].status, GitFileStatus::Added);

        commit_repo(&root, "initial commit", false).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.message().unwrap().trim_end_matches('\n'), "initial commit");
        assert!(collect_workspace_status(7, &root).files.is_empty());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn conditional_reconcile_unstages_rriter_owned_modified_when_worktree_matches_head() {
        let root = temp_git_root("reconcile_head");
        std::fs::create_dir_all(&root).unwrap();
        let _repo = git2::Repository::init(&root).unwrap();
        std::fs::write(root.join("tracked.txt"), b"A\n").unwrap();
        toggle_stage(&root, "tracked.txt", None, false).unwrap();
        commit_repo(&root, "initial", false).unwrap();

        std::fs::write(root.join("tracked.txt"), b"B\n").unwrap();
        let mut owned_stage_entries = FxHashMap::default();
        assert!(run_git_stage_operation(
            &stage_operation(&root, "tracked.txt", false),
            &mut owned_stage_entries,
        )
        .is_none());
        let key = GitStageOwnershipKey::new(&root, "tracked.txt");
        assert!(owned_stage_entries.contains_key(&key));

        std::fs::write(root.join("tracked.txt"), b"A\n").unwrap();
        assert!(run_git_stage_operation(
            &reconcile_operation(&root, "tracked.txt"),
            &mut owned_stage_entries,
        )
        .is_none());
        assert!(!owned_stage_entries.contains_key(&key));
        assert!(collect_workspace_status(0, &root).files.is_empty());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn conditional_reconcile_preserves_owned_stage_when_worktree_differs_from_head() {
        let root = temp_git_root("reconcile_preserve");
        std::fs::create_dir_all(&root).unwrap();
        let _repo = git2::Repository::init(&root).unwrap();
        std::fs::write(root.join("tracked.txt"), b"A\n").unwrap();
        toggle_stage(&root, "tracked.txt", None, false).unwrap();
        commit_repo(&root, "initial", false).unwrap();

        std::fs::write(root.join("tracked.txt"), b"B\n").unwrap();
        let mut owned_stage_entries = FxHashMap::default();
        assert!(run_git_stage_operation(
            &stage_operation(&root, "tracked.txt", false),
            &mut owned_stage_entries,
        )
        .is_none());
        std::fs::write(root.join("tracked.txt"), b"C\n").unwrap();

        assert!(run_git_stage_operation(
            &reconcile_operation(&root, "tracked.txt"),
            &mut owned_stage_entries,
        )
        .is_none());
        assert!(owned_stage_entries.is_empty());
        let status = collect_workspace_status(0, &root);
        assert_eq!(status.files.len(), 1);
        assert!(status.files[0].staged);
        assert_eq!(status.files[0].status, GitFileStatus::Modified);

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn conditional_reconcile_only_unstages_owned_target_file() {
        let root = temp_git_root("reconcile_target_only");
        std::fs::create_dir_all(&root).unwrap();
        let _repo = git2::Repository::init(&root).unwrap();
        std::fs::write(root.join("one.txt"), b"A1\n").unwrap();
        std::fs::write(root.join("two.txt"), b"A2\n").unwrap();
        toggle_stage(&root, "one.txt", None, false).unwrap();
        toggle_stage(&root, "two.txt", None, false).unwrap();
        commit_repo(&root, "initial", false).unwrap();

        std::fs::write(root.join("one.txt"), b"B1\n").unwrap();
        std::fs::write(root.join("two.txt"), b"B2\n").unwrap();
        let mut owned_stage_entries = FxHashMap::default();
        let stage_both = GitStageOperation::ToggleMany(vec![
            GitStageFileCommand {
                repo_root: root.clone(),
                rel_path: "one.txt".to_string(),
                old_rel_path: None,
                staged: false,
            },
            GitStageFileCommand {
                repo_root: root.clone(),
                rel_path: "two.txt".to_string(),
                old_rel_path: None,
                staged: false,
            },
        ]);
        assert!(run_git_stage_operation(&stage_both, &mut owned_stage_entries).is_none());
        std::fs::write(root.join("one.txt"), b"A1\n").unwrap();

        assert!(run_git_stage_operation(
            &reconcile_operation(&root, "one.txt"),
            &mut owned_stage_entries,
        )
        .is_none());
        assert!(!owned_stage_entries.contains_key(&GitStageOwnershipKey::new(&root, "one.txt")));
        assert!(owned_stage_entries.contains_key(&GitStageOwnershipKey::new(&root, "two.txt")));
        let status = collect_workspace_status(0, &root);
        assert_eq!(status.files.len(), 1);
        assert_eq!(status.files[0].rel_path.as_ref(), "two.txt");
        assert!(status.files[0].staged);
        assert_eq!(status.files[0].status, GitFileStatus::Modified);

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn conditional_reconcile_never_owns_non_modified_index_statuses() {
        let root = temp_git_root("reconcile_status_guard");
        std::fs::create_dir_all(&root).unwrap();
        let _repo = git2::Repository::init(&root).unwrap();
        for (path, content) in [
            ("deleted.txt", b"deleted\n".as_slice()),
            ("unstaged.txt", b"unstaged\n".as_slice()),
            ("rename-old.txt", b"rename\n".as_slice()),
        ] {
            std::fs::write(root.join(path), content).unwrap();
            toggle_stage(&root, path, None, false).unwrap();
        }
        commit_repo(&root, "initial", false).unwrap();

        std::fs::write(root.join("added.txt"), b"new\n").unwrap();
        std::fs::write(root.join("untracked.txt"), b"loose\n").unwrap();
        std::fs::write(root.join("unstaged.txt"), b"changed\n").unwrap();
        std::fs::remove_file(root.join("deleted.txt")).unwrap();
        crate::platform::rename_path(
            &root.join("rename-old.txt"),
            &root.join("rename-new.txt"),
        )
        .unwrap();
        let renamed = collect_workspace_status(0, &root)
            .files
            .into_iter()
            .find(|file| file.status == GitFileStatus::Renamed)
            .expect("unstaged rename");
        assert!(!renamed.staged);

        let mut owned_stage_entries = FxHashMap::default();
        for operation in [
            stage_operation(&root, "added.txt", false),
            stage_operation(&root, "deleted.txt", false),
            GitStageOperation::ToggleMany(vec![GitStageFileCommand {
                repo_root: root.clone(),
                rel_path: renamed.rel_path.to_string(),
                old_rel_path: renamed.old_rel_path.as_ref().map(ToString::to_string),
                staged: false,
            }]),
        ] {
            assert!(run_git_stage_operation(&operation, &mut owned_stage_entries).is_none());
        }
        assert!(owned_stage_entries.is_empty());

        for path in [
            "added.txt",
            "untracked.txt",
            "unstaged.txt",
            "deleted.txt",
            "rename-new.txt",
        ] {
            assert!(run_git_stage_operation(
                &reconcile_operation(&root, path),
                &mut owned_stage_entries,
            )
            .is_none());
        }
        let status = collect_workspace_status(0, &root);
        assert!(status.files.iter().any(|file| {
            file.rel_path.as_ref() == "added.txt"
                && file.staged
                && file.status == GitFileStatus::Added
        }));
        assert!(status.files.iter().any(|file| {
            file.rel_path.as_ref() == "untracked.txt"
                && !file.staged
                && file.status == GitFileStatus::Untracked
        }));
        assert!(status.files.iter().any(|file| {
            file.rel_path.as_ref() == "unstaged.txt"
                && !file.staged
                && file.status == GitFileStatus::Modified
        }));
        assert!(status.files.iter().any(|file| {
            file.rel_path.as_ref() == "deleted.txt"
                && file.staged
                && file.status == GitFileStatus::Deleted
        }));
        assert!(status.files.iter().any(|file| {
            file.rel_path.as_ref() == "rename-new.txt"
                && file.staged
                && file.status == GitFileStatus::Renamed
        }));

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn conditional_reconcile_rejects_external_index_replacement() {
        let root = temp_git_root("reconcile_external_replace");
        std::fs::create_dir_all(&root).unwrap();
        let repo = git2::Repository::init(&root).unwrap();
        std::fs::write(root.join("tracked.txt"), b"A\n").unwrap();
        toggle_stage(&root, "tracked.txt", None, false).unwrap();
        commit_repo(&root, "initial", false).unwrap();

        std::fs::write(root.join("tracked.txt"), b"B\n").unwrap();
        let mut owned_stage_entries = FxHashMap::default();
        assert!(run_git_stage_operation(
            &stage_operation(&root, "tracked.txt", false),
            &mut owned_stage_entries,
        )
        .is_none());
        let key = GitStageOwnershipKey::new(&root, "tracked.txt");
        let owned_identity = owned_stage_entries.get(&key).copied().unwrap();

        std::fs::write(root.join("tracked.txt"), b"C\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("tracked.txt")).unwrap();
        index.write().unwrap();
        let external_identity = index
            .get_path(Path::new("tracked.txt"), 0)
            .map(git_index_entry_identity)
            .unwrap();
        assert_ne!(external_identity, owned_identity);
        drop(index);
        std::fs::write(root.join("tracked.txt"), b"A\n").unwrap();

        assert!(run_git_stage_operation(
            &reconcile_operation(&root, "tracked.txt"),
            &mut owned_stage_entries,
        )
        .is_none());
        assert!(owned_stage_entries.is_empty());
        let current_identity = repo
            .index()
            .unwrap()
            .get_path(Path::new("tracked.txt"), 0)
            .map(git_index_entry_identity)
            .unwrap();
        assert_eq!(current_identity, external_identity);
        let status = collect_workspace_status(0, &root);
        assert_eq!(status.files.len(), 1);
        assert!(status.files[0].staged);
        assert_eq!(status.files[0].status, GitFileStatus::Modified);

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn failed_stage_does_not_arm_future_reconcile() {
        let root = temp_git_root("reconcile_failed_stage");
        std::fs::create_dir_all(&root).unwrap();
        let repo = git2::Repository::init(&root).unwrap();
        std::fs::write(root.join("tracked.txt"), b"A\n").unwrap();
        toggle_stage(&root, "tracked.txt", None, false).unwrap();
        commit_repo(&root, "initial", false).unwrap();

        std::fs::write(root.join("tracked.txt"), b"B\n").unwrap();
        std::fs::write(repo.path().join("index.lock"), b"locked").unwrap();
        let mut owned_stage_entries = FxHashMap::default();
        let notice = run_git_stage_operation(
            &stage_operation(&root, "tracked.txt", false),
            &mut owned_stage_entries,
        );
        assert!(notice.is_some());
        assert!(owned_stage_entries.is_empty());
        std::fs::remove_file(repo.path().join("index.lock")).unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(Path::new("tracked.txt")).unwrap();
        index.write().unwrap();
        let staged_identity = index
            .get_path(Path::new("tracked.txt"), 0)
            .map(git_index_entry_identity)
            .unwrap();
        drop(index);
        std::fs::write(root.join("tracked.txt"), b"A\n").unwrap();

        assert!(run_git_stage_operation(
            &reconcile_operation(&root, "tracked.txt"),
            &mut owned_stage_entries,
        )
        .is_none());
        let current_identity = repo
            .index()
            .unwrap()
            .get_path(Path::new("tracked.txt"), 0)
            .map(git_index_entry_identity)
            .unwrap();
        assert_eq!(current_identity, staged_identity);

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn explicit_unstage_clears_owned_stage_provenance() {
        let root = temp_git_root("reconcile_explicit_unstage");
        std::fs::create_dir_all(&root).unwrap();
        let repo = git2::Repository::init(&root).unwrap();
        std::fs::write(root.join("tracked.txt"), b"A\n").unwrap();
        toggle_stage(&root, "tracked.txt", None, false).unwrap();
        commit_repo(&root, "initial", false).unwrap();

        std::fs::write(root.join("tracked.txt"), b"B\n").unwrap();
        let mut owned_stage_entries = FxHashMap::default();
        assert!(run_git_stage_operation(
            &stage_operation(&root, "tracked.txt", false),
            &mut owned_stage_entries,
        )
        .is_none());
        assert!(!owned_stage_entries.is_empty());
        assert!(run_git_stage_operation(
            &stage_operation(&root, "tracked.txt", true),
            &mut owned_stage_entries,
        )
        .is_none());
        assert!(owned_stage_entries.is_empty());

        std::fs::write(root.join("tracked.txt"), b"C\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("tracked.txt")).unwrap();
        index.write().unwrap();
        let external_identity = index
            .get_path(Path::new("tracked.txt"), 0)
            .map(git_index_entry_identity)
            .unwrap();
        drop(index);
        std::fs::write(root.join("tracked.txt"), b"A\n").unwrap();

        assert!(run_git_stage_operation(
            &reconcile_operation(&root, "tracked.txt"),
            &mut owned_stage_entries,
        )
        .is_none());
        let current_identity = repo
            .index()
            .unwrap()
            .get_path(Path::new("tracked.txt"), 0)
            .map(git_index_entry_identity)
            .unwrap();
        assert_eq!(current_identity, external_identity);

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn conditional_reconcile_uses_git_normalization_for_autocrlf() {
        let root = temp_git_root("reconcile_autocrlf");
        std::fs::create_dir_all(&root).unwrap();
        let repo = git2::Repository::init(&root).unwrap();
        repo.config().unwrap().set_bool("core.autocrlf", true).unwrap();
        std::fs::write(root.join("windows.txt"), b"A\r\nline\r\n").unwrap();
        toggle_stage(&root, "windows.txt", None, false).unwrap();
        commit_repo(&root, "initial", false).unwrap();

        std::fs::write(root.join("windows.txt"), b"B\r\nline\r\n").unwrap();
        let mut owned_stage_entries = FxHashMap::default();
        assert!(run_git_stage_operation(
            &stage_operation(&root, "windows.txt", false),
            &mut owned_stage_entries,
        )
        .is_none());
        assert!(!owned_stage_entries.is_empty());
        std::fs::write(root.join("windows.txt"), b"A\r\nline\r\n").unwrap();

        assert!(run_git_stage_operation(
            &reconcile_operation(&root, "windows.txt"),
            &mut owned_stage_entries,
        )
        .is_none());
        assert!(collect_workspace_status(0, &root).files.is_empty());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn stage_reconcile_candidate_is_session_owned_and_invalidated_by_status() {
        let repo_root = PathBuf::from("/workspace");
        let mut state = GitPanelState::default();
        state.snapshot = GitStatusSnapshot {
            workspaces: vec![git_workspace(
                vec![git_file("tracked.txt", true, GitFileStatus::Modified)],
                None,
            )],
        };
        let stage = GitStageFileCommand {
            repo_root: repo_root.clone(),
            rel_path: "tracked.txt".to_string(),
            old_rel_path: None,
            staged: false,
        };
        state.update_stage_reconcile_candidates(std::slice::from_ref(&stage));
        assert!(state.has_stage_reconcile_candidate_for_test(&repo_root, "tracked.txt"));

        let explicit_unstage = GitStageFileCommand {
            staged: true,
            ..stage.clone()
        };
        state.update_stage_reconcile_candidates(std::slice::from_ref(&explicit_unstage));
        assert!(!state.has_stage_reconcile_candidate_for_test(&repo_root, "tracked.txt"));

        state.update_stage_reconcile_candidates(std::slice::from_ref(&stage));
        state.apply_event(GitPanelEvent {
            request_id: 1,
            snapshot: GitStatusSnapshot {
                workspaces: vec![git_workspace(
                    vec![git_file("tracked.txt", false, GitFileStatus::Modified)],
                    None,
                )],
            },
            notice: None,
            preserve_snapshot_on_empty: false,
            clear_message: false,
            refresh_graph: false,
            transaction_failed: false,
        });
        assert!(!state.has_stage_reconcile_candidate_for_test(&repo_root, "tracked.txt"));
    }

    #[test]
    fn git_autocrlf_normalizes_index_without_dirtying_the_worktree() {
        let root = temp_git_root("autocrlf");
        std::fs::create_dir_all(&root).unwrap();
        let repo = git2::Repository::init(&root).unwrap();
        repo.config().unwrap().set_bool("core.autocrlf", true).unwrap();
        std::fs::write(root.join("windows.txt"), b"first\r\nsecond\r\n").unwrap();

        toggle_stage(&root, "windows.txt", None, false).unwrap();
        commit_repo(&root, "crlf", false).unwrap();

        let commit = repo.head().unwrap().peel_to_commit().unwrap();
        let tree = commit.tree().unwrap();
        let entry = tree.get_path(Path::new("windows.txt")).unwrap();
        let blob = repo.find_blob(entry.id()).unwrap();
        assert_eq!(blob.content(), b"first\nsecond\n");
        assert!(collect_workspace_status(0, &root).files.is_empty());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn git_core_filemode_false_ignores_executable_bit_changes() {
        use std::os::unix::fs::PermissionsExt;

        let root = temp_git_root("filemode");
        std::fs::create_dir_all(&root).unwrap();
        let repo = git2::Repository::init(&root).unwrap();
        repo.config().unwrap().set_bool("core.filemode", false).unwrap();
        let path = root.join("script.sh");
        std::fs::write(&path, b"#!/bin/sh\n").unwrap();
        toggle_stage(&root, "script.sh", None, false).unwrap();
        commit_repo(&root, "script", false).unwrap();

        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).unwrap();

        assert!(collect_workspace_status(0, &root).files.is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn git_case_only_rename_stages_old_and_new_paths_together() {
        let root = temp_git_root("case_rename");
        std::fs::create_dir_all(&root).unwrap();
        let repo = git2::Repository::init(&root).unwrap();
        std::fs::write(root.join("Readme.txt"), b"hello\n").unwrap();
        toggle_stage(&root, "Readme.txt", None, false).unwrap();
        commit_repo(&root, "initial", false).unwrap();

        crate::platform::rename_path(
            &root.join("Readme.txt"),
            &root.join("README.txt"),
        )
        .unwrap();
        let status = collect_workspace_status(0, &root);
        let renamed = status
            .files
            .iter()
            .find(|file| file.status == GitFileStatus::Renamed)
            .expect("case-only rename");
        assert_eq!(renamed.rel_path.as_ref(), "README.txt");
        assert_eq!(renamed.old_rel_path.as_deref(), Some("Readme.txt"));

        toggle_stage(
            &root,
            renamed.rel_path.as_ref(),
            renamed.old_rel_path.as_deref(),
            false,
        )
        .unwrap();
        commit_repo(&root, "rename", false).unwrap();
        let tree = repo.head().unwrap().peel_to_commit().unwrap().tree().unwrap();
        assert!(tree.get_path(Path::new("README.txt")).is_ok());
        assert!(tree.get_path(Path::new("Readme.txt")).is_err());
        assert!(collect_workspace_status(0, &root).files.is_empty());

        std::fs::remove_dir_all(&root).unwrap();
    }

