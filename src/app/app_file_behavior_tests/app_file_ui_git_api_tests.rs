#[test]
fn lsp_actions_noqa_workspace_edit_and_panel_log_sizes_headless() {
    let Some(mut app) = test_app() else {
        return;
    };

    let path = PathBuf::from("/tmp/main.py");
    app.file_path = Some(path.clone());
    app.file_extension = "py".to_string();
    app.base_title = "main.py".to_string();
    app.editor = editor_with("x = 1\nvalue = 2  # noqa: E501\n");

    app.insert_noqa_comment(0, &["F401".to_string(), "E501".to_string()]);
    assert!(
        app.editor
            .get_full_text()
            .starts_with("x = 1  # noqa: F401, E501\n")
    );

    app.insert_noqa_comment(1, &["F821".to_string(), "E501".to_string()]);
    assert!(
        app.editor
            .get_full_text()
            .contains("value = 2  # noqa: E501, F821")
    );

    app.insert_noqa_comment(1, &[]);
    assert!(app.editor.get_full_text().contains("value = 2  # noqa\n"));

    app.editor = editor_with("abc\ndef\nghi\n");
    app.editor.cursor = 5;
    app.editor.selection_anchor = Some(1);

    let mut changes = std::collections::HashMap::new();
    changes.insert(
        path.clone(),
        vec![
            crate::lsp::TextChange {
                start_line: 1,
                start_col: 0,
                end_line: 1,
                end_col: 3,
                new_text: "DEF".to_string(),
            },
            crate::lsp::TextChange {
                start_line: 0,
                start_col: 1,
                end_line: 0,
                end_col: 2,
                new_text: "B".to_string(),
            },
        ],
    );
    app.apply_workspace_edit(&crate::lsp::WorkspaceEdit { changes }, true);
    assert_eq!(app.editor.get_full_text(), "aBc\nDEF\nghi\n");
    assert_eq!(app.editor.cursor, 5);
    assert_eq!(app.editor.selection_anchor, Some(1));

    let mut action_changes = std::collections::HashMap::new();
    action_changes.insert(
        path.clone(),
        vec![crate::lsp::TextChange {
            start_line: 2,
            start_col: 0,
            end_line: 2,
            end_col: 3,
            new_text: "GHI".to_string(),
        }],
    );
    app.lsp_actions_menu = Some(LspActionsMenu {
        cursor_line: 0,
        items: vec![LspActionItem::CodeAction(crate::lsp::CodeAction {
            title: "Upper".to_string(),
            kind: Some("quickfix".to_string()),
            edit: Some(crate::lsp::WorkspaceEdit {
                changes: action_changes,
            }),
            code: Some("T001".to_string()),
        })],
        selected: 0,
        menu_x: 0.0,
        menu_y: 0.0,
        pending_request_id: None,
    });
    app.apply_selected_lsp_action();
    assert_eq!(app.editor.get_full_text(), "aBc\nDEF\nGHI\n");

    app.lsp_actions_menu = Some(LspActionsMenu {
        cursor_line: 0,
        items: vec![LspActionItem::AddNoqa {
            codes: vec!["T002".to_string()],
        }],
        selected: 0,
        menu_x: 0.0,
        menu_y: 0.0,
        pending_request_id: None,
    });
    app.apply_selected_lsp_action();
    assert!(
        app.editor
            .get_full_text()
            .starts_with("aBc  # noqa: T002\n")
    );

    assert!(app.lsp_panel_bounds().is_none());

    let info = crate::lsp::LspServerInfo {
        name: "ruff",
        status: crate::lsp::LspServerStatus::Running,
        logs: Vec::new(),
    };
    app.ide_panel.lsp_servers = vec![info.clone()];
    assert_eq!(app.lsp_server_logs_h(&info, 1.0), 0.0);

    app.ide_panel.lsp_logs_expanded.insert("ruff".to_string());
    let mut log_editor = editor_with("header\n  detail\nlast line\n");
    log_editor.foldable_lines.insert(0, 1);
    log_editor.folded_lines.insert(0);
    app.ide_panel
        .lsp_log_editors
        .insert("ruff".to_string(), log_editor);

    let (inner_h, inner_w) = app.lsp_server_inner_size(&info, 1.0);
    assert!(inner_h >= 32.0);
    assert!(inner_w > 0.0);
    assert!(app.lsp_server_logs_h(&info, 1.0) >= 50.0);
    assert!(app.lsp_panel_total_h(1.0) >= 210.0);
}

#[test]
fn ui_handlers_state_only_branches_work_without_window() {
    let Some(mut app) = test_app() else {
        return;
    };

    app.handle_ui_click(crate::ui_system::UiId::HoverPopupScroll);
    app.handle_ui_click(crate::ui_system::UiId::BottomPanelBody);
    app.handle_ui_click(crate::ui_system::UiId::StatusDiagnostics);
    assert!(app.ide_panel.is_open(crate::app::PanelId::Problems));
    app.handle_ui_click(crate::ui_system::UiId::StatusDiagnostics);
    assert!(!app.ide_panel.is_open(crate::app::PanelId::Problems));

    app.ide_panel.terminal_focused = true;
    app.handle_ui_click(crate::ui_system::UiId::ResizeLeft);
    app.handle_ui_click(crate::ui_system::UiId::ResizeBottom);
    assert!(!app.ide_panel.is_resizing_left);
    assert!(!app.ide_panel.is_resizing_bottom);

    app.handle_ui_click(crate::ui_system::UiId::LspScrollY);
    app.handle_ui_click(crate::ui_system::UiId::LspScrollX);
    assert!(!app.ide_panel.lsp_scroll_y.is_dragging);
    assert!(app.ide_panel.lsp_scroll_x.is_dragging);

    app.handle_ui_click(crate::ui_system::UiId::EditorScrollbarX);
    assert!(!app.scroll_x.is_dragging);

    app.ide_panel.lsp_servers = vec![crate::lsp::LspServerInfo {
        name: "ruff",
        status: crate::lsp::LspServerStatus::Running,
        logs: Vec::new(),
    }];
    app.handle_ui_click(crate::ui_system::UiId::LspLogScrollY(0));
    app.handle_ui_click(crate::ui_system::UiId::LspLogScrollX(0));
    assert!(
        app.ide_panel
            .lsp_logs_scroll_y
            .get("ruff")
            .is_some_and(|scroll| !scroll.is_dragging)
    );
    assert!(
        app.ide_panel
            .lsp_logs_scroll_x
            .get("ruff")
            .is_some_and(|scroll| !scroll.is_dragging)
    );

    app.ide_panel
        .flat_diags
        .push(crate::app::ProblemRow::group_header(
            std::path::Path::new("/tmp/main.py").into(),
        ));
    app.handle_ui_click(crate::ui_system::UiId::ProblemFileToggle(0));
    assert!(app.ide_panel.problems_collapsed.is_empty());
}

#[test]
fn git_panel_ui_handlers_cover_menu_commit_and_folder_state_headless() {
    let Some(mut app) = test_app() else {
        return;
    };
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
                staged: true,
                status: crate::app::git_panel::GitFileStatus::Modified,
            }],
            tree: vec![crate::app::git_panel::GitTreeRow {
                name: "src".into(),
                path: "src".into(),
                depth: 0,
                file_idx: None,
                icon_key: "src",
            }],
            ahead: 0,
            error: None,
        }],
    };

    app.handle_ui_click(crate::ui_system::UiId::GitCommitMenuToggle);
    assert!(app.ide_panel.git.commit_menu_open());
    assert!(app.ide_panel.git.commit_menu_opened_at.is_some());

    app.handle_ui_click(crate::ui_system::UiId::GitCommitOptionsToggle);
    assert!(!app.ide_panel.git.commit_menu_open());
    assert!(app.ide_panel.git.commit_options_menu_open());
    app.handle_ui_click(crate::ui_system::UiId::GitCommitOptionsItem(0));
    assert!(app.ide_panel.git.commit_options.skip_hooks);
    assert!(app.ide_panel.git.commit_options.any_enabled());
    app.handle_ui_click(crate::ui_system::UiId::GitCommitOptionsItem(0));
    assert!(!app.ide_panel.git.commit_options.skip_hooks);

    app.handle_ui_click(crate::ui_system::UiId::GitCommitMenuToggle);
    assert!(app.ide_panel.git.commit_menu_open());

    app.handle_ui_click(crate::ui_system::UiId::GitFolder(0, 0));
    assert!(!app.ide_panel.git.commit_menu_open());
    assert!(!app.ide_panel.git.commit_options_menu_open());
    assert!(
        app.ide_panel
            .git
            .collapsed_dirs
            .get(&0)
            .is_some_and(|dirs| dirs.contains("src"))
    );

    app.handle_ui_click(crate::ui_system::UiId::GitFolder(0, 0));
    assert!(
        !app.ide_panel
            .git
            .collapsed_dirs
            .get(&0)
            .is_some_and(|dirs| dirs.contains("src"))
    );

    app.ide_panel.git.message_focused = true;
    app.handle_ui_click(crate::ui_system::UiId::GitCommit);
    assert_eq!(
        app.ide_panel.git.notice.as_deref(),
        Some("Commit message empty")
    );
    assert!(!app.ide_panel.git.message_focused);

    let _ = app.ide_panel.git.message_editor.insert_str("ready");
    app.ide_panel.git.snapshot.workspaces[0].files[0].staged = false;
    app.ide_panel.git.notice = None;
    app.ide_panel.git.message_focused = true;
    app.handle_ui_click(crate::ui_system::UiId::GitCommitMenuItem(1));
    assert_eq!(app.ide_panel.git.notice.as_deref(), None);
    assert!(app.ide_panel.git.message_focused);
    assert!(!app.ide_panel.git.pending);

    app.handle_ui_click(crate::ui_system::UiId::GitGraphToggle);
    assert_eq!(
        app.ide_panel.git.bottom_pane,
        crate::app::git_panel::GitBottomPane::Graph
    );
    app.handle_ui_click(crate::ui_system::UiId::GitLogsToggle);
    assert_eq!(
        app.ide_panel.git.bottom_pane,
        crate::app::git_panel::GitBottomPane::Logs
    );
    app.handle_ui_click(crate::ui_system::UiId::GitLogsToggle);
    assert_eq!(
        app.ide_panel.git.bottom_pane,
        crate::app::git_panel::GitBottomPane::Closed
    );
    app.ide_panel.git.seed_git_log_for_test("entry");
    assert!(!app.ide_panel.git.git_logs.is_empty());
    app.handle_ui_click(crate::ui_system::UiId::GitLogsClear);
    assert!(app.ide_panel.git.git_logs.is_empty());
}

#[test]
fn git_panel_workspace_confirm_dialogs_use_staged_files_headless() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.ide_panel.git.snapshot = crate::app::git_panel::GitStatusSnapshot {
        workspaces: vec![crate::app::git_panel::GitWorkspaceStatus {
            workspace_idx: 0,
            root: PathBuf::from("/workspace"),
            repo_root: Some(PathBuf::from("/workspace")),
            branch_name: None,
            files: vec![
                crate::app::git_panel::GitFileEntry {
                    workspace_idx: 0,
                    rel_path: "src/lib.rs".into(),
                    old_rel_path: None,
                    display_path: "src/lib.rs".into(),
                    depth: 1,
                    staged: true,
                    status: crate::app::git_panel::GitFileStatus::Modified,
                },
                crate::app::git_panel::GitFileEntry {
                    workspace_idx: 0,
                    rel_path: "src/main.rs".into(),
                    old_rel_path: None,
                    display_path: "src/main.rs".into(),
                    depth: 1,
                    staged: false,
                    status: crate::app::git_panel::GitFileStatus::Modified,
                },
            ],
            tree: Vec::new(),
            ahead: 0,
            error: None,
        }],
    };

    app.handle_ui_click(crate::ui_system::UiId::GitRollbackStaged(0));
    let dialog = app.ide_panel.git.confirm_dialog.as_ref().unwrap();
    assert_eq!(
        dialog.action,
        crate::app::git_panel::GitConfirmAction::RollbackStaged
    );
    assert_eq!(dialog.files.len(), 1);
    assert_eq!(dialog.files[0].display_path, "src/lib.rs");

    app.handle_ui_click(crate::ui_system::UiId::GitConfirmCancel);
    assert!(app.ide_panel.git.confirm_dialog.is_none());

    app.handle_ui_click(crate::ui_system::UiId::GitUnstageAll(0));
    assert!(app.ide_panel.git.confirm_dialog.is_none());
    assert!(!app.ide_panel.git.snapshot.workspaces[0].files[0].staged);
    assert_eq!(app.ide_panel.git.stage_pending_workspace_idx, Some(0));
}

#[test]
fn git_panel_stage_clicks_are_locked_while_pending_headless() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.ide_panel.git.snapshot = crate::app::git_panel::GitStatusSnapshot {
        workspaces: vec![crate::app::git_panel::GitWorkspaceStatus {
            workspace_idx: 0,
            root: PathBuf::from("/workspace"),
            repo_root: Some(PathBuf::from("/workspace")),
            branch_name: None,
            files: vec![crate::app::git_panel::GitFileEntry {
                workspace_idx: 0,
                rel_path: "tests/test_api.py".into(),
                old_rel_path: None,
                display_path: "tests/test_api.py".into(),
                depth: 1,
                staged: false,
                status: crate::app::git_panel::GitFileStatus::Modified,
            }],
            tree: vec![
                crate::app::git_panel::GitTreeRow {
                    name: "tests".into(),
                    path: "tests".into(),
                    depth: 0,
                    file_idx: None,
                    icon_key: "tests",
                },
                crate::app::git_panel::GitTreeRow {
                    name: "test_api.py".into(),
                    path: "tests/test_api.py".into(),
                    depth: 1,
                    file_idx: Some(0),
                    icon_key: "python",
                },
            ],
            ahead: 0,
            error: None,
        }],
    };
    app.ide_panel.git.pending = true;

    app.handle_ui_click(crate::ui_system::UiId::GitFolderStage(0, 0));
    assert!(!app.ide_panel.git.snapshot.workspaces[0].files[0].staged);
    assert_eq!(app.ide_panel.git.stage_pending_workspace_idx, None);

    app.handle_ui_click(crate::ui_system::UiId::GitFile(0, 0));
    assert!(!app.ide_panel.git.snapshot.workspaces[0].files[0].staged);
}

#[test]
fn git_file_row_checkbox_only_queues_stage() {
    let Some(mut app) = test_app() else {
        return;
    };
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

    app.handle_ui_click(crate::ui_system::UiId::GitFile(0, 0));
    assert!(app.ide_panel.git.snapshot.workspaces[0].files[0].staged);
    assert_eq!(app.ide_panel.git.stage_pending_workspace_idx, Some(0));
    assert!(app.tabs.is_empty());
}

include!("app_file_git_reconcile_tests.rs");
include!("app_file_api_mock_tests.rs");
include!("app_file_api_mock_completion_tests.rs");
