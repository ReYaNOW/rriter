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
                    rel_path: "src/lib.rs".to_string(),
                    old_rel_path: None,
                    display_path: "src/lib.rs".to_string(),
                    depth: 1,
                    staged: true,
                    status: crate::app::git_panel::GitFileStatus::Modified,
                },
                crate::app::git_panel::GitFileEntry {
                    workspace_idx: 0,
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

fn open_api_mock_query_test_route(app: &mut App) -> crate::app::api_client::ApiSpecId {
    let spec_id = open_api_mock_test_route(app);
    let model = app.ide_panel.api.models.get_mut(&spec_id).unwrap();
    model.routes[0].query_params = vec![crate::app::api_client::ApiParam {
        name: "name".to_string(),
        location: crate::app::api_client::ApiParamLocation::Query,
        required: true,
        primitive_type: crate::app::api_client::ApiPrimitiveType::String,
        item_type: None,
        enum_values: Vec::new(),
        default_value: None,
        example: None,
        examples: Vec::new(),
        description: String::new(),
        constraints: crate::app::api_mock::types::ApiMockFieldConstraints::default(),
    }];
    spec_id
}

fn add_second_api_mock_test_route(app: &mut App, spec_id: crate::app::api_client::ApiSpecId) {
    let model = app.ide_panel.api.models.get_mut(&spec_id).unwrap();
    let mut route = api_mock_test_route();
    route.path = "/orders".to_string();
    model.routes.push(route);
}

fn api_client_tab_route_idx(tab: &crate::app::EditorTab) -> Option<usize> {
    match &tab.kind {
        crate::app::EditorTabKind::ApiClient(_, state) => state.route_idx,
        _ => None,
    }
}

#[test]
fn manual_mock_schema_focus_uses_visible_schema_text_for_selection() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.ide_panel
        .api
        .mock
        .manual_routes
        .push(crate::app::api_mock::types::ApiManualRoute {
            stable_id: "manual-users".to_string(),
            method: crate::app::api_client::ApiMethod::Get,
            path: "/users/{user_id}".to_string(),
            enabled: true,
            response: crate::app::api_mock::types::ApiMockResponse::Generated,
            python: Some(crate::app::api_mock::types::default_api_mock_python_script()),
            input_fields: Vec::new(),
            output_fields: Vec::new(),
        });

    app.open_api_manual_route(0);
    app.focus_api_input(crate::app::api_client::ApiFocus::InputSchema {
        spec_id: crate::app::api_client::API_MANUAL_MOCK_SPEC_ID,
        route_idx: 0,
    });

    let text = app.ide_panel.api.input_editor.get_full_text();
    assert!(text.contains("user_id"));
    app.ide_panel.api.input_editor.select_all();
    assert_eq!(
        app.ide_panel.api.input_editor.get_selection().as_deref(),
        Some(text.as_str())
    );
}

#[test]
fn api_mock_contract_focus_switch_keeps_python_highlight_cache() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);
    app.toggle_api_route_python(0);
    app.focus_api_input(crate::app::api_client::ApiFocus::MockContract { route_idx: 0 });
    app.ide_panel.api.input_editor.cursor = 0;
    app.ide_panel.api.input_editor.selection_anchor = None;
    let version = app.ide_panel.api.input_editor.version;
    let cache_key = (
        0,
        crate::app::api_mock::ty_check::ApiMockSourcePart::Contract,
    );
    let target = Some((cache_key.0, cache_key.1, version));
    app.ide_panel.api.mock_highlight_cache.insert(
        cache_key,
        vec![crate::highlighter::ColorSpan {
            start: 0,
            end: 5,
            color: crate::highlighter::DRACULA_CYAN,
        }],
    );
    app.ide_panel.api.mock_highlight_target = target;

    app.focus_api_input(crate::app::api_client::ApiFocus::MockPrelude { route_idx: 0 });

    assert!(matches!(
        app.ide_panel.api.focused,
        Some(crate::app::api_client::ApiFocus::MockPrelude { route_idx: 0 })
    ));
    assert_eq!(app.ide_panel.api.mock_highlight_target, target);
    assert!(
        app.ide_panel
            .api
            .mock_highlight_cache
            .contains_key(&cache_key)
    );
}

#[test]
fn api_route_open_reuses_last_api_tab_without_ctrl() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.is_ide_mode = true;
    let spec_id = open_api_mock_test_route(&mut app);
    add_second_api_mock_test_route(&mut app, spec_id);

    app.open_api_route_with_new_tab(spec_id, 0, true);
    assert_eq!(app.tabs.len(), 2);
    app.switch_to_tab(0);

    app.open_api_route(spec_id, 1);

    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.active_tab, 1);
    assert_eq!(api_client_tab_route_idx(&app.tabs[0]), Some(0));
    assert_eq!(api_client_tab_route_idx(&app.tabs[1]), Some(1));
}

#[test]
fn api_route_open_with_ctrl_adds_same_api_tab() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.is_ide_mode = true;
    let spec_id = open_api_mock_test_route(&mut app);
    add_second_api_mock_test_route(&mut app, spec_id);

    app.open_api_route_with_new_tab(spec_id, 1, true);

    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.active_tab, 1);
    assert_eq!(api_client_tab_route_idx(&app.tabs[0]), Some(0));
    assert_eq!(api_client_tab_route_idx(&app.tabs[1]), Some(1));
}

#[test]
fn api_client_stays_visible_when_last_remaining_tab() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.is_ide_mode = true;
    open_api_mock_test_route(&mut app);
    app.open_new_tab();
    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.active_tab, 1);

    app.close_tab_at(1);

    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.active_tab, 0);
    assert!(app.active_tab_is_api_client());
    assert!(!app.show_welcome);
}

fn text_change_for_source_span(
    source: &str,
    start: usize,
    end: usize,
    new_text: &str,
) -> crate::lsp::TextChange {
    let mut line_offsets = vec![0usize];
    for (idx, b) in source.bytes().enumerate() {
        if b == b'\n' {
            line_offsets.push(idx + 1);
        }
    }
    let (start_line, start_col) = crate::lsp::offset_to_lsp_pos(source, start, &line_offsets);
    let (end_line, end_col) = crate::lsp::offset_to_lsp_pos(source, end, &line_offsets);
    crate::lsp::TextChange {
        start_line,
        start_col,
        end_line,
        end_col,
        new_text: new_text.to_string(),
    }
}

fn api_lsp_item(
    label: &str,
    kind: crate::highlighter::SymbolKind,
    module: Option<&str>,
    detail: Option<&str>,
) -> crate::lsp::LspCompletionItem {
    crate::lsp::LspCompletionItem {
        label: label.to_string(),
        kind,
        module: module.map(str::to_string),
        detail: detail.map(str::to_string),
        insert_text: None,
        text_edit: None,
        additional_text_edits: Vec::new(),
    }
}

#[test]
fn api_mock_python_toggle_preserves_code_and_enables_openapi_mock() {
    let Some(mut app) = test_app() else {
        return;
    };
    let spec_id = open_api_mock_test_route(&mut app);

    app.toggle_api_route_python(0);
    let override_route = app.ide_panel.api.mock.route_overrides.first().unwrap();
    assert!(override_route.enabled);
    assert!(!override_route.proxy_when_disabled);
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
    assert!(routes.first().unwrap().enabled);

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
fn api_mock_disable_turns_off_python() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);

    app.toggle_api_route_python(0);
    app.toggle_api_route_mock(0);

    let override_route = app.ide_panel.api.mock.route_overrides.first().unwrap();
    assert!(!override_route.enabled);
    assert!(override_route.proxy_when_disabled);
    assert!(
        !override_route
            .python
            .as_ref()
            .is_some_and(|script| script.enabled)
    );
}

#[test]
fn api_mock_route_reset_removes_override_and_cached_python_editors() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);

    app.toggle_api_route_mock(0);
    app.toggle_api_route_python(0);
    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    let _ = app
        .ide_panel
        .api
        .input_editor
        .insert_str("\n    reset_marker = 1");

    app.handle_ui_click(crate::ui_system::UiId::ApiMockRouteReset(0));

    assert!(app.ide_panel.api.mock_route_reset_dialog.is_some());
    assert_eq!(app.ide_panel.api.mock.route_overrides.len(), 1);
    app.handle_ui_click(crate::ui_system::UiId::ApiMockRouteResetConfirm);

    assert!(app.ide_panel.api.mock.route_overrides.is_empty());
    assert!(app.ide_panel.api.focused.is_none());
    assert!(
        app.ide_panel
            .api
            .mock_python_editors
            .keys()
            .all(|(route_idx, _)| *route_idx != 0)
    );
    assert!(app.ide_panel.api.mock_ty_diagnostics.is_empty());
}

#[test]
fn api_mock_contract_code_edit_updates_field_list_after_focus_leaves() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_query_test_route(&mut app);
    app.toggle_api_route_python(0);
    app.focus_api_input(crate::app::api_client::ApiFocus::MockContract { route_idx: 0 });

    let text = app.ide_panel.api.input_editor.get_full_text();
    let insert_at = text
        .find("class Query:\n")
        .map(|idx| idx + "class Query:\n".len())
        .expect("query contract class exists");
    app.ide_panel.api.input_editor.cursor = insert_at;
    let _ = app
        .ide_panel
        .api
        .input_editor
        .insert_str("    live_value: int\n");

    let script = app.ide_panel.api.mock.route_overrides[0]
        .python
        .as_ref()
        .expect("python mock script");
    assert!(
        !script
            .contract
            .query
            .fields
            .iter()
            .any(|field| field.name == "live_value" && field.enabled)
    );

    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    let script = app.ide_panel.api.mock.route_overrides[0]
        .python
        .as_ref()
        .expect("python mock script");
    assert!(
        script
            .contract
            .query
            .fields
            .iter()
            .any(|field| field.name == "live_value" && field.enabled)
    );
    assert!(
        app.api_mock_contract_source_for_route(0)
            .is_some_and(|source| source.contains("live_value: int"))
    );
}

#[test]
fn api_mock_contract_field_remove_deletes_variable_from_contract() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_query_test_route(&mut app);
    app.toggle_api_route_python(0);

    use crate::ui_system::ApiMockContractFieldGroup;

    let removed_name = app.ide_panel.api.mock.route_overrides[0]
        .python
        .as_ref()
        .expect("python mock script")
        .contract
        .query
        .fields
        .first()
        .expect("query field")
        .name
        .clone();
    app.handle_ui_click(crate::ui_system::UiId::ApiMockContractFieldRemove(
        0,
        ApiMockContractFieldGroup::Query,
        0,
    ));
    assert!(
        app.ide_panel
            .api
            .mock_contract_field_delete_dialog
            .is_some()
    );
    let script = app.ide_panel.api.mock.route_overrides[0]
        .python
        .as_ref()
        .expect("python mock script");
    assert!(
        script
            .contract
            .query
            .fields
            .iter()
            .any(|field| field.name == removed_name)
    );

    app.handle_ui_click(crate::ui_system::UiId::ApiMockContractFieldRemoveConfirm);
    let script = app.ide_panel.api.mock.route_overrides[0]
        .python
        .as_ref()
        .expect("python mock script");
    assert!(
        !script
            .contract
            .query
            .fields
            .iter()
            .any(|field| field.name == removed_name)
    );
    assert!(!script.contract_source.contains(&format!("{removed_name}:")));
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
    assert_eq!(body, "\n    return Response(ok=True)");
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
fn api_mock_autocomplete_applies_lsp_text_edit_and_imports_to_prelude() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);
    app.toggle_api_route_python(0);
    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    app.ide_panel.api.input_editor.set_text_clean("    Res");
    app.ide_panel.api.input_editor.cursor = app.ide_panel.api.input_editor.len();

    let (method, path, route, model) = app.api_mock_route_context(0).unwrap();
    let script = app.api_mock_script_for_tools(0).unwrap();
    let virtual_source = crate::app::api_mock::ty_check::build_api_mock_virtual_source(
        method, &path, &route, &model, &script,
    );
    let start = virtual_source.source.rfind("Res").unwrap();
    let edit = text_change_for_source_span(
        &virtual_source.source,
        start,
        start + "Res".len(),
        "Response",
    );
    app.autocomplete_active = true;
    app.autocomplete_mode = crate::app::AutocompleteMode::TyContext;
    app.autocomplete_options = vec![(
        crate::app::AutocompleteItem {
            word: "Response".to_string(),
            kind: crate::highlighter::SymbolKind::Class,
            scope_start: 0,
            scope_end: usize::MAX,
            module: None,
            module_path: None,
            detail: None,
            insert_text: None,
            text_edit: Some(edit),
            additional_text_edits: vec![crate::lsp::TextChange {
                start_line: 0,
                start_col: 0,
                end_line: 0,
                end_col: 0,
                new_text: "from typing import Any\n".to_string(),
            }],
        },
        Vec::new(),
    )];

    assert!(app.apply_api_mock_autocomplete());

    let script = app.api_mock_script_for_tools(0).unwrap();
    assert_eq!(script.body, "    Response");
    assert!(script.prelude.contains("from typing import Any"));
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
    assert_eq!(
        app.autocomplete_mode,
        crate::app::AutocompleteMode::TreeSitter
    );
    assert_eq!(app.autocomplete_options[0].0.word, "Response");
}

#[test]
fn api_mock_contract_completion_suggests_all_constraint_markers() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);
    app.toggle_api_route_python(0);
    app.focus_api_input(crate::app::api_client::ApiFocus::MockContract { route_idx: 0 });
    app.ide_panel
        .api
        .input_editor
        .set_text_clean("class Query:\n    name: Annotated[str, ");
    app.ide_panel.api.input_editor.cursor = app.ide_panel.api.input_editor.len();

    app.update_api_mock_tree_sitter_autocomplete();

    assert!(app.autocomplete_active);
    for marker in [
        "MinLen", "MaxLen", "Pattern", "Ge", "Gt", "Le", "Lt", "MinItems", "MaxItems",
    ] {
        assert!(
            app.autocomplete_options
                .iter()
                .any(|(item, _)| item.word == marker),
            "{marker} missing"
        );
    }
    let max_len = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "MaxLen")
        .map(|(item, _)| item)
        .unwrap();
    assert_eq!(max_len.insert_text.as_deref(), Some("MaxLen(255)"));
    assert!(
        max_len
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("max string length"))
    );
}

#[test]
fn api_mock_contract_field_editor_commits_constraints_and_flags() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_query_test_route(&mut app);
    app.toggle_api_route_python(0);

    use crate::app::api_client::ApiFocus;
    use crate::ui_system::{ApiMockContractFieldGroup, ApiMockContractFieldProp};

    app.handle_ui_click(crate::ui_system::UiId::ApiMockContractFieldAddConstraint(
        0,
        ApiMockContractFieldGroup::Query,
        0,
    ));
    assert_eq!(
        app.ide_panel.api.mock_contract_constraint_menu,
        Some(crate::app::api_client::ApiMockContractConstraintMenu {
            route_idx: 0,
            group: ApiMockContractFieldGroup::Query,
            field_idx: 0,
        })
    );
    app.handle_ui_click(
        crate::ui_system::UiId::ApiMockContractFieldAddConstraintOption(
            0,
            ApiMockContractFieldGroup::Query,
            0,
            ApiMockContractFieldProp::Default,
        ),
    );
    assert!(matches!(
        app.ide_panel.api.focused,
        Some(ApiFocus::MockContractField {
            prop: ApiMockContractFieldProp::Default,
            ..
        })
    ));

    app.toggle_api_mock_contract_field_required(0, ApiMockContractFieldGroup::Query, 0);
    app.toggle_api_mock_contract_field_nullable(0, ApiMockContractFieldGroup::Query, 0);

    for (prop, text) in [
        (ApiMockContractFieldProp::Default, "guest"),
        (ApiMockContractFieldProp::Enum, "guest, admin"),
        (ApiMockContractFieldProp::MinLength, "2"),
        (ApiMockContractFieldProp::MaxLength, "16"),
        (ApiMockContractFieldProp::Pattern, "^[a-z]+$"),
        (ApiMockContractFieldProp::Minimum, "1"),
        (ApiMockContractFieldProp::Maximum, "99"),
        (ApiMockContractFieldProp::MinItems, "1"),
        (ApiMockContractFieldProp::MaxItems, "3"),
    ] {
        app.focus_api_input(ApiFocus::MockContractField {
            route_idx: 0,
            group: ApiMockContractFieldGroup::Query,
            field_idx: 0,
            prop,
        });
        app.ide_panel.api.input_editor.set_text_clean(text);
        app.commit_api_focus();
    }

    let script = app.api_mock_script_for_tools(0).unwrap();
    let field = &script.contract.query.fields[0];
    assert!(!field.required);
    assert!(field.nullable);
    assert_eq!(field.default_value.as_deref(), Some("guest"));
    assert_eq!(field.enum_values, ["guest", "admin"]);
    assert_eq!(field.constraints.min_length, Some(2));
    assert_eq!(field.constraints.max_length, Some(16));
    assert_eq!(field.constraints.pattern.as_deref(), Some("^[a-z]+$"));
    assert_eq!(field.constraints.minimum.as_deref(), Some("1"));
    assert_eq!(field.constraints.maximum.as_deref(), Some("99"));
    assert_eq!(field.constraints.min_items, Some(1));
    assert_eq!(field.constraints.max_items, Some(3));
    assert!(field.constraints.nullable);
    assert!(script.contract_source.contains("class NameEnum(StrEnum):"));
    assert!(script.contract_source.contains("name: Annotated[NameEnum"));
    assert!(script.contract_source.contains("MaxLen(16)"));
    assert!(script.contract_source.contains("Pattern("));
}

#[test]
fn api_mock_detail_popup_uses_api_editor_cursor_context() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);
    app.toggle_api_route_python(0);
    app.editor = editor_with("main_cursor_should_not_win");
    app.editor.cursor = 0;
    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    app.ide_panel
        .api
        .input_editor
        .set_text_clean("    Response");
    app.ide_panel.api.input_editor.cursor = app.ide_panel.api.input_editor.len();
    app.autocomplete_active = true;
    app.autocomplete_selected_idx = 0;
    app.autocomplete_options = vec![(
        crate::app::AutocompleteItem {
            word: "Response".to_string(),
            kind: crate::highlighter::SymbolKind::Class,
            scope_start: 0,
            scope_end: usize::MAX,
            module: None,
            module_path: None,
            detail: Some("class Response".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        Vec::new(),
    )];

    app.refresh_autocomplete_detail_popup();

    let popup = app.autocomplete_detail_popup.as_ref().unwrap();
    assert_eq!(popup.byte_offset, app.ide_panel.api.input_editor.cursor);
    assert_ne!(popup.byte_offset, app.editor.cursor);
}

#[test]
fn api_mock_popup_keys_move_selection_and_refresh_detail_like_editor() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);
    app.toggle_api_route_python(0);
    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    app.autocomplete_active = true;
    app.autocomplete_mode = crate::app::AutocompleteMode::TreeSitter;
    app.autocomplete_options = vec![
        (
            crate::app::AutocompleteItem {
                word: "alpha".to_string(),
                kind: crate::highlighter::SymbolKind::Function,
                scope_start: 0,
                scope_end: usize::MAX,
                module: None,
                module_path: None,
                detail: Some("alpha detail".to_string()),
                insert_text: None,
                text_edit: None,
                additional_text_edits: Vec::new(),
            },
            Vec::new(),
        ),
        (
            crate::app::AutocompleteItem {
                word: "beta".to_string(),
                kind: crate::highlighter::SymbolKind::Function,
                scope_start: 0,
                scope_end: usize::MAX,
                module: None,
                module_path: None,
                detail: Some("beta detail".to_string()),
                insert_text: None,
                text_edit: None,
                additional_text_edits: Vec::new(),
            },
            Vec::new(),
        ),
    ];

    let result = app.handle_active_autocomplete_key(
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowDown),
        false,
    );

    assert_eq!(result, crate::app::AutocompletePopupKeyResult::Consumed);
    assert_eq!(app.autocomplete_selected_idx, 1);
    assert!(
        app.autocomplete_detail_popup
            .as_ref()
            .is_some_and(|popup| popup.text.contains("beta detail"))
    );
}

#[test]
fn api_mock_enter_completion_keeps_body_focus() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);
    app.toggle_api_route_python(0);
    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    app.ide_panel.api.input_editor.set_text_clean("    Res");
    app.ide_panel.api.input_editor.cursor = app.ide_panel.api.input_editor.len();
    app.autocomplete_active = true;
    app.autocomplete_options = vec![(
        api_lsp_item(
            "Response",
            crate::highlighter::SymbolKind::Class,
            None,
            None,
        )
        .into(),
        Vec::new(),
    )];

    let result = app.handle_active_autocomplete_key(
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Enter),
        false,
    );

    assert_eq!(result, crate::app::AutocompletePopupKeyResult::Consumed);
    assert_eq!(
        app.ide_panel.api.focused,
        Some(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 })
    );
}

#[test]
fn api_mock_pending_enter_and_tab_share_main_autocomplete_gate() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);
    app.toggle_api_route_python(0);
    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    app.ide_panel
        .api
        .input_editor
        .set_text_clean("    response.");
    app.ide_panel.api.input_editor.cursor = app.ide_panel.api.input_editor.len();
    app.autocomplete_mode = crate::app::AutocompleteMode::TyContext;
    app.autocomplete_pending_request_id = Some(77);

    assert!(
        app.mark_pending_autocomplete_apply_for_key(winit::keyboard::PhysicalKey::Code(
            winit::keyboard::KeyCode::Enter
        ))
    );
    assert!(app.autocomplete_apply_pending_response);

    app.autocomplete_apply_pending_response = false;
    app.autocomplete_pending_request_id = Some(78);
    assert!(
        app.mark_pending_autocomplete_apply_for_key(winit::keyboard::PhysicalKey::Code(
            winit::keyboard::KeyCode::Tab
        ))
    );
    assert!(app.autocomplete_apply_pending_response);
}

#[test]
fn api_mock_ty_exact_single_match_closes_like_main_editor() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);
    app.toggle_api_route_python(0);
    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    app.ide_panel
        .api
        .input_editor
        .set_text_clean("    model_dump");
    app.ide_panel.api.input_editor.cursor = app.ide_panel.api.input_editor.len();
    app.autocomplete_active = true;
    app.autocomplete_mode = crate::app::AutocompleteMode::TyContext;

    app.update_api_mock_ty_autocomplete(vec![api_lsp_item(
        "model_dump",
        crate::highlighter::SymbolKind::Function,
        Some("Response"),
        Some("def Response.model_dump(self) -> dict"),
    )]);

    assert!(!app.autocomplete_active);
    assert!(app.autocomplete_options.is_empty());
}

#[test]
fn api_mock_ty_member_order_reuses_python_owner_and_private_ranking() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);
    app.toggle_api_route_python(0);
    app.ide_panel
        .api
        .mock
        .route_overrides
        .iter_mut()
        .find_map(|override_route| override_route.python.as_mut())
        .unwrap()
        .prelude =
        "class GrandBase:\n    grand_public: int\n\nclass BoxReadPublic(GrandBase):\n    base_public: int\n    _base_hidden: int\n\nclass BoxRead(BoxReadPublic):\n    current_public: int\n    _current_hidden: int\n"
            .to_string();
    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    app.ide_panel.api.input_editor.set_text_clean("    box.");
    app.ide_panel.api.input_editor.cursor = app.ide_panel.api.input_editor.len();
    app.autocomplete_mode = crate::app::AutocompleteMode::TyContext;

    app.update_api_mock_ty_autocomplete(vec![
        api_lsp_item(
            "base_public",
            crate::highlighter::SymbolKind::Class,
            Some("int"),
            Some("int"),
        ),
        api_lsp_item(
            "_base_hidden",
            crate::highlighter::SymbolKind::Class,
            Some("int"),
            Some("int"),
        ),
        api_lsp_item(
            "grand_public",
            crate::highlighter::SymbolKind::Class,
            Some("int"),
            Some("int"),
        ),
        api_lsp_item(
            "current_public",
            crate::highlighter::SymbolKind::Class,
            Some("int"),
            Some("int"),
        ),
        api_lsp_item(
            "_current_hidden",
            crate::highlighter::SymbolKind::Class,
            Some("int"),
            Some("int"),
        ),
        api_lsp_item(
            "model_dump",
            crate::highlighter::SymbolKind::Function,
            Some("BoxRead"),
            Some("def BoxRead.model_dump(self) -> dict"),
        ),
        api_lsp_item(
            "base_method",
            crate::highlighter::SymbolKind::Function,
            Some("BoxReadPublic"),
            Some("def BoxReadPublic.base_method(self) -> dict"),
        ),
        api_lsp_item(
            "mro",
            crate::highlighter::SymbolKind::Function,
            Some("BoxRead"),
            Some("def BoxRead.mro(self) -> list[type]"),
        ),
    ]);

    let words = app
        .autocomplete_options
        .iter()
        .map(|(item, _)| item.word.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        words,
        vec![
            "current_public",
            "model_dump",
            "_current_hidden",
            "base_public",
            "base_method",
            "_base_hidden",
            "grand_public",
            "mro",
        ]
    );
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
        assert!(popup_text.is_some_and(|text| {
            text.starts_with("[[MODULE]] api_mock.mock_api.get_users\n")
                && text.contains("class Response")
        }));
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
