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
    assert!(app.ide_panel.lsp_scroll_y.is_dragging);
    assert!(app.ide_panel.lsp_scroll_x.is_dragging);

    app.handle_ui_click(crate::ui_system::UiId::EditorScrollbarX);
    assert!(app.scroll_x.is_dragging);

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
            .is_some_and(|scroll| scroll.is_dragging)
    );
    assert!(
        app.ide_panel
            .lsp_logs_scroll_x
            .get("ruff")
            .is_some_and(|scroll| scroll.is_dragging)
    );

    app.ide_panel
        .flat_diags
        .push((PathBuf::from("/tmp/main.py"), 0));
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
            files: Vec::new(),
            tree: vec![crate::app::git_panel::GitTreeRow {
                name: "src".to_string(),
                path: "src".to_string(),
                depth: 0,
                file_idx: None,
                icon_key: "src",
            }],
            ahead: 0,
            error: None,
        }],
    };

    app.handle_ui_click(crate::ui_system::UiId::GitCommitMenuToggle);
    assert!(app.ide_panel.git.commit_menu_open);

    app.handle_ui_click(crate::ui_system::UiId::GitFolder(0, 0));
    assert!(!app.ide_panel.git.commit_menu_open);
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
    app.ide_panel.git.message_focused = true;
    app.handle_ui_click(crate::ui_system::UiId::GitCommitMenuItem(1));
    assert_eq!(app.ide_panel.git.notice.as_deref(), Some("No staged files"));
    assert!(!app.ide_panel.git.message_focused);
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
                    repo_root: PathBuf::from("/workspace"),
                    rel_path: "src/lib.rs".to_string(),
                    old_rel_path: None,
                    display_path: "src/lib.rs".to_string(),
                    depth: 1,
                    staged: true,
                    status: crate::app::git_panel::GitFileStatus::Modified,
                },
                crate::app::git_panel::GitFileEntry {
                    workspace_idx: 0,
                    repo_root: PathBuf::from("/workspace"),
                    rel_path: "src/main.rs".to_string(),
                    old_rel_path: None,
                    display_path: "src/main.rs".to_string(),
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
                repo_root: PathBuf::from("/workspace"),
                rel_path: "tests/test_api.py".to_string(),
                old_rel_path: None,
                display_path: "tests/test_api.py".to_string(),
                depth: 1,
                staged: false,
                status: crate::app::git_panel::GitFileStatus::Modified,
            }],
            tree: vec![
                crate::app::git_panel::GitTreeRow {
                    name: "tests".to_string(),
                    path: "tests".to_string(),
                    depth: 0,
                    file_idx: None,
                    icon_key: "tests",
                },
                crate::app::git_panel::GitTreeRow {
                    name: "test_api.py".to_string(),
                    path: "tests/test_api.py".to_string(),
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
fn git_file_row_checkbox_only_toggles_stage() {
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
                repo_root: PathBuf::from("/workspace"),
                rel_path: "src/main.rs".to_string(),
                old_rel_path: None,
                display_path: "src/main.rs".to_string(),
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
    assert!(app.tabs.is_empty());
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
                repo_root: PathBuf::from("/workspace"),
                rel_path: "src/main.rs".to_string(),
                old_rel_path: None,
                display_path: "src/main.rs".to_string(),
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
                repo_root: PathBuf::from("/workspace"),
                rel_path: "src/main.rs".to_string(),
                old_rel_path: None,
                display_path: "src/main.rs".to_string(),
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
        base_title: "Diff: main.rs".to_string(),
        file_extension: "rs".to_string(),
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

fn api_mock_test_route() -> crate::app::api_client::ApiRouteRow {
    crate::app::api_client::ApiRouteRow {
        tag: String::new(),
        method: crate::app::api_client::ApiMethod::Get,
        path: "/users".to_string(),
        summary: String::new(),
        operation_id: String::new(),
        security: None,
        path_params: Vec::new(),
        query_params: Vec::new(),
        request_body: None,
        responses: Vec::new(),
    }
}

fn open_api_mock_test_route(app: &mut App) -> crate::app::api_client::ApiSpecId {
    let spec_id = crate::app::api_client::ApiSpecId(777);
    let entry = crate::app::api_client::ApiSpecEntry {
        id: spec_id,
        title: "Mock API".to_string(),
        version: "1".to_string(),
        openapi_version: "3.1.0".to_string(),
        source: crate::app::api_client::ApiSpecSource::Url(
            "https://example.test/openapi.json".to_string(),
        ),
        last_loaded: None,
        last_fetch_secs: None,
        last_parse_secs: None,
        last_url_status: None,
        selected: true,
        error: None,
    };
    let model = crate::app::api_client::ApiSpecModel {
        id: spec_id,
        title: "Mock API".to_string(),
        version: "1".to_string(),
        openapi_version: "3.1.0".to_string(),
        servers: Vec::new(),
        routes: vec![api_mock_test_route()],
        security_schemes: Vec::new(),
        root_security: Vec::new(),
        schema_arena: Vec::new(),
    };
    app.ide_panel.api.specs.push(entry);
    app.ide_panel.api.models.insert(spec_id, model);
    app.open_api_route(spec_id, 0);
    spec_id
}

#[test]
fn api_mock_python_toggle_preserves_code_and_does_not_enable_openapi_mock() {
    let Some(mut app) = test_app() else {
        return;
    };
    let spec_id = open_api_mock_test_route(&mut app);

    app.toggle_api_route_python(0);
    let override_route = app.ide_panel.api.mock.route_overrides.first().unwrap();
    assert!(!override_route.enabled);
    assert!(
        override_route
            .python
            .as_ref()
            .is_some_and(|script| script.enabled)
    );

    let entry = app.ide_panel.api.specs.first().unwrap();
    let model = app.ide_panel.api.models.get(&spec_id).unwrap();
    let routes = crate::app::api_mock::merge::build_api_mock_routes(
        [(entry, model)],
        &app.ide_panel.api.mock,
    );
    assert!(!routes.first().unwrap().enabled);

    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    let _ = app
        .ide_panel
        .api
        .input_editor
        .insert_str("\n    value = 42");
    app.toggle_api_route_python(0);
    assert!(
        !app.ide_panel.api.mock.route_overrides[0]
            .python
            .as_ref()
            .unwrap()
            .enabled
    );
    app.toggle_api_route_python(0);
    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    assert!(
        app.ide_panel
            .api
            .input_editor
            .get_full_text()
            .contains("value = 42")
    );
}

#[test]
fn api_mock_python_editors_keep_independent_undo_and_reset_parts() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);
    app.toggle_api_route_python(0);

    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    let _ = app
        .ide_panel
        .api
        .input_editor
        .insert_str("\n    body_marker = 1");
    assert!(
        app.ide_panel
            .api
            .input_editor
            .get_full_text()
            .contains("body_marker")
    );

    app.focus_api_input(crate::app::api_client::ApiFocus::MockPrelude { route_idx: 0 });
    let _ = app.ide_panel.api.input_editor.insert_str("import os\n");
    assert!(
        app.ide_panel
            .api
            .input_editor
            .get_full_text()
            .contains("import os")
    );

    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    let _ = app.ide_panel.api.input_editor.undo();
    assert!(
        !app.ide_panel
            .api
            .input_editor
            .get_full_text()
            .contains("body_marker")
    );

    app.focus_api_input(crate::app::api_client::ApiFocus::MockPrelude { route_idx: 0 });
    assert!(
        app.ide_panel
            .api
            .input_editor
            .get_full_text()
            .contains("import os")
    );

    app.reset_api_route_python_part(
        0,
        crate::app::api_mock::ty_check::ApiMockSourcePart::Prelude,
    );
    assert_eq!(app.ide_panel.api.input_editor.get_full_text(), "");

    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    let _ = app
        .ide_panel
        .api
        .input_editor
        .insert_str("\n    reset_me = 1");
    app.reset_api_route_python_part(0, crate::app::api_mock::ty_check::ApiMockSourcePart::Body);
    let body = app.ide_panel.api.input_editor.get_full_text();
    assert_eq!(body, "    return Response(ok=True)");
    assert!(!body.contains("reset_me"));
    assert!(
        app.ide_panel
            .api
            .mock_highlight_target
            .is_some_and(|(route_idx, part, _)| {
                route_idx == 0 && part == crate::app::api_mock::ty_check::ApiMockSourcePart::Body
            })
    );
    assert!(
        app.ide_panel
            .api
            .mock_highlight_cache
            .contains_key(&(0, crate::app::api_mock::ty_check::ApiMockSourcePart::Body))
    );
    assert!(app.ide_panel.api.mock_ty_diagnostics.is_empty());
}

#[test]
fn api_mock_signature_parameters_feed_autocomplete_like_python_editor() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);
    app.toggle_api_route_python(0);
    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    app.ide_panel
        .api
        .input_editor
        .set_text_clean("    result = make_item(");
    app.ide_panel.api.input_editor.cursor = app.ide_panel.api.input_editor.len();
    app.autocomplete_mode = crate::app::AutocompleteMode::TyContext;

    app.update_api_mock_ty_signature_help_autocomplete(vec!["value".to_string()]);

    assert!(app.autocomplete_active);
    assert_eq!(app.autocomplete_options[0].0.word, "value");
    assert_eq!(
        app.autocomplete_options[0].0.insert_text.as_deref(),
        Some("value=")
    );
    assert!(app.apply_api_mock_autocomplete());
    assert_eq!(
        app.ide_panel.api.input_editor.get_full_text(),
        "    result = make_item(value="
    );
}

#[test]
fn api_mock_tree_sitter_completions_show_before_ty_merge() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);
    app.toggle_api_route_python(0);
    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    app.ide_panel.api.input_editor.set_text_clean("    r");
    app.ide_panel.api.input_editor.cursor = app.ide_panel.api.input_editor.len();
    app.ide_panel.api.mock_highlighter.completions = vec![crate::highlighter::CompletionItem {
        word: "Response".to_string(),
        kind: crate::highlighter::SymbolKind::Class,
        scope_start: 0,
        scope_end: usize::MAX,
    }];

    app.update_api_mock_tree_sitter_autocomplete();

    assert!(app.autocomplete_active);
    assert_eq!(app.autocomplete_mode, crate::app::AutocompleteMode::TreeSitter);
    assert_eq!(app.autocomplete_options[0].0.word, "Response");
}

#[test]
fn api_mock_python_hover_uses_virtual_source_fallback() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);
    app.toggle_api_route_python(0);
    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });

    let body = app.ide_panel.api.input_editor.get_full_text();
    let edit_byte = body.find("Response").unwrap();
    let target = crate::app::api_client::ApiMockHoverTarget {
        route_idx: 0,
        part: crate::app::api_mock::ty_check::ApiMockSourcePart::Body,
        edit_byte,
        version: app.ide_panel.api.input_editor.version,
    };
    let source = "class Response:\n    ok: bool = True\n\nreturn Response(ok=True)".to_string();
    let source_cursor = source.rfind("Response").unwrap();
    app.ide_panel.api.mock_hover_target = Some(target.clone());
    app.ide_panel.api.mock_hover_request = Some(crate::app::api_client::ApiMockHoverRequest {
        request_id: 77,
        target,
        source,
        source_cursor,
        anchor: (0.0, 0.0),
    });
    crate::app::mouse::HOVER_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.byte_offset = Some(edit_byte);
        state.request_id = Some(77);
        state.popup = None;
        state.pending_popup = None;
    });

    assert!(app.apply_api_mock_hover_response(77, Some("None".to_string())));
    crate::app::mouse::HOVER_STATE.with(|state| {
        let mut state = state.borrow_mut();
        let popup_text = state.popup.as_ref().map(|popup| popup.text.as_str());
        assert!(popup_text.is_some_and(|text| text.contains("class Response")));
        state.popup = None;
        state.byte_offset = None;
    });
}

#[test]
fn api_mock_hover_clears_old_popup_when_crossing_mock_editors() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);
    app.toggle_api_route_python(0);
    app.ui_registry.register_text_input(
        crate::ui_system::UiId::ApiMockPreludeInput(0),
        10.0,
        10.0,
        220.0,
        80.0,
        20.0,
        20.0,
    );

    let old_target = crate::app::api_client::ApiMockHoverTarget {
        route_idx: 0,
        part: crate::app::api_mock::ty_check::ApiMockSourcePart::Body,
        edit_byte: 3,
        version: 1,
    };
    app.ide_panel.api.mock_hover_target = Some(old_target);
    crate::app::mouse::HOVER_STATE.with(|state| {
        let mut state = state.borrow_mut();
        *state = crate::app::mouse::HoverState::default();
        state.byte_offset = Some(3);
        state.request_id = Some(44);
        state.rect = Some((40.0, 40.0, 180.0, 90.0));
        state.popup = Some(crate::app::mouse::HoverPopup {
            text: "old".to_string(),
            spans: Vec::new(),
            line_kinds: Vec::new(),
            inline_code_ranges: Vec::new(),
            byte_offset: 3,
            anchor_x: 40.0,
            anchor_y: 40.0,
            offset_x: Some(6.0),
            offset_y: Some(8.0),
            anim_progress: 1.0,
            scroll: crate::scroll::ScrollState::new(15.0),
            layout_cache: None,
        });
    });

    assert!(app.update_api_mock_hover_from_cursor(20.0, 20.0, false, false));

    assert!(app.ide_panel.api.mock_hover_target.is_none());
    crate::app::mouse::HOVER_STATE.with(|state| {
        let mut state = state.borrow_mut();
        assert!(state.popup.is_none());
        assert!(state.byte_offset.is_none());
        assert!(state.request_id.is_none());
        assert!(state.rect.is_none());
        *state = crate::app::mouse::HoverState::default();
    });
}
