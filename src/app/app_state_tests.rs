    #[test]
    fn panel_state_toggle_and_group_queries_end_to_end() {
        let mut panels = IdePanelState::default();

        assert!(!panels.any_top_open());
        assert!(!panels.any_bottom_open());
        assert!(!panels.is_open(PanelId::Explorer));

        panels.toggle(PanelId::Explorer);
        panels.toggle(PanelId::Terminal);

        assert!(panels.is_open(PanelId::Explorer));
        assert!(panels.is_open(PanelId::Terminal));
        assert!(panels.terminal_focused);
        assert!(panels.any_top_open());
        assert!(panels.any_bottom_open());

        panels.toggle(PanelId::Explorer);
        assert!(!panels.is_open(PanelId::Explorer));
        assert!(panels.any_bottom_open());
    }

    #[test]
    fn panel_state_keeps_one_open_panel_per_group() {
        let mut panels = IdePanelState::default();
        panels.toggle(PanelId::Terminal);
        panels.toggle(PanelId::Problems);
        assert!(!panels.is_open(PanelId::Terminal));
        assert!(panels.is_open(PanelId::Problems));
        assert!(!panels.terminal_focused);

        panels.toggle(PanelId::Explorer);
        panels.toggle(PanelId::Database);
        assert!(!panels.is_open(PanelId::Explorer));
        assert!(panels.is_open(PanelId::Database));

        panels.toggle(PanelId::LspServers);
        assert!(!panels.is_open(PanelId::Database));
        assert!(panels.is_open(PanelId::LspServers));

        panels
            .slots
            .iter_mut()
            .find(|slot| slot.id == PanelId::Terminal)
            .unwrap()
            .open = true;
        panels
            .slots
            .iter_mut()
            .find(|slot| slot.id == PanelId::Problems)
            .unwrap()
            .open = true;
        panels.enforce_single_open_per_group();
        assert!(panels.is_open(PanelId::Terminal));
        assert!(!panels.is_open(PanelId::Problems));
    }

    #[test]
    fn moving_an_open_panel_keeps_the_destination_group_exclusive() {
        let mut panels = IdePanelState::default();
        panels.open(PanelId::Explorer);
        panels.open(PanelId::Terminal);

        let explorer = panels
            .slots
            .iter_mut()
            .find(|slot| slot.id == PanelId::Explorer)
            .expect("explorer slot");
        explorer.group = PanelGroup::Bottom;
        panels.reconcile_moved_panel(PanelId::Explorer);

        assert!(panels.is_open(PanelId::Explorer));
        assert!(!panels.is_open(PanelId::Terminal));
        assert!(!panels.terminal_focused);
        assert_eq!(
            panels
                .slots
                .iter()
                .filter(|slot| slot.group == PanelGroup::Bottom && slot.open)
                .count(),
            1
        );
    }

    #[test]
    fn opening_a_peer_panel_clears_focus_owned_by_hidden_panels() {
        let mut panels = IdePanelState::default();
        panels.open(PanelId::Search);
        panels.project_search.focused = Some(crate::app::project_search::ProjectSearchField::Query);
        panels.open(PanelId::LspServers);
        assert!(panels.project_search.focused.is_none());

        panels.lsp_log_filter_focused = true;
        panels.lsp_logs_focused = Some("rust-analyzer".to_string());
        panels.open(PanelId::Explorer);
        assert!(!panels.lsp_log_filter_focused);
        assert!(panels.lsp_logs_focused.is_none());

        panels.file_tree_focused = true;
        panels.open(PanelId::Git);
        assert!(!panels.file_tree_focused);
    }

    #[test]
    fn closing_git_panel_revokes_vcs_copy_owner_before_reopen() {
        let mut panels = IdePanelState::default();
        panels.open(PanelId::Git);
        panels.git.toggle_logs_pane();
        panels.git.claim_git_logs_copy_owner();
        assert!(panels.git.owns_git_logs_copy());

        panels.toggle(PanelId::Git);
        assert!(!panels.is_open(PanelId::Git));
        assert!(!panels.git.owns_git_logs_copy());

        panels.open(PanelId::Git);
        assert!(panels.is_open(PanelId::Git));
        assert!(!panels.git.owns_git_logs_copy());
    }

    #[test]
    fn hidden_top_group_has_no_visible_left_width() {
        let mut panels = IdePanelState::default();
        panels.left_width = 240.0;
        assert_eq!(panels.visible_left_width(2.0), 0.0);
        panels.open(PanelId::Explorer);
        assert_eq!(panels.visible_left_width(2.0), 480.0);
    }

    #[test]
    fn open_terminal_exclusive_closes_bottom_peers_and_focuses_terminal() {
        let mut panels = IdePanelState::default();
        panels.toggle(PanelId::Problems);
        panels.term_search_focused = true;
        panels.git.message_focused = true;

        panels.open_terminal_exclusive();

        assert!(panels.is_open(PanelId::Terminal));
        assert!(!panels.is_open(PanelId::Problems));
        assert!(panels.terminal_focused);
        assert!(!panels.term_search_focused);
        assert!(!panels.git.message_focused);
    }

    #[test]
    fn open_panel_opens_without_toggling_closed_and_keeps_group_exclusive() {
        let mut panels = IdePanelState::default();
        panels.open(PanelId::Problems);
        panels.open(PanelId::Problems);

        assert!(panels.is_open(PanelId::Problems));
        assert!(!panels.terminal_focused);

        panels.open(PanelId::Terminal);
        assert!(panels.is_open(PanelId::Terminal));
        assert!(!panels.is_open(PanelId::Problems));
        assert!(panels.terminal_focused);

        panels.open(PanelId::Problems);
        assert!(panels.is_open(PanelId::Problems));
        assert!(!panels.is_open(PanelId::Terminal));
        assert!(!panels.terminal_focused);
    }

    #[test]
    fn bottom_panel_blocks_editor_hover_except_unfocused_terminal() {
        let mut panels = IdePanelState::default();
        assert!(!panels.bottom_panel_blocks_editor_hover());

        panels.toggle(PanelId::Problems);
        assert!(panels.bottom_panel_blocks_editor_hover());
        assert_eq!(panels.editor_reserved_bottom_height(1.0), 0.0);

        panels.toggle(PanelId::Terminal);
        assert!(panels.bottom_panel_blocks_editor_hover());
        assert_eq!(panels.editor_reserved_bottom_height(1.0), 0.0);

        panels.terminal_focused = false;
        assert!(panels.bottom_terminal_is_transparent());
        assert!(!panels.bottom_panel_blocks_editor_hover());
        assert_eq!(panels.editor_reserved_bottom_height(1.0), 0.0);

        if let Some(slot) = panels
            .slots
            .iter_mut()
            .find(|slot| slot.id == PanelId::LspServers)
        {
            slot.group = PanelGroup::Bottom;
            slot.open = true;
        }
        if let Some(slot) = panels
            .slots
            .iter_mut()
            .find(|slot| slot.id == PanelId::Terminal)
        {
            slot.open = false;
        }
        panels.bottom_height = 180.0;
        assert_eq!(panels.open_bottom_panel_id(), Some(PanelId::LspServers));
        assert!(!panels.bottom_terminal_is_transparent());
        assert_eq!(panels.editor_reserved_bottom_height(2.0), 360.0);
    }

    #[test]
    fn python_inlay_hints_keep_positionals_and_skip_named_args() {
        let text = "build(1, name=2, other = 3, value)\n";
        let hints = vec![
            crate::lsp::LspInlayHint {
                line: 0,
                col: 6,
                label: "id=".to_string(),
            },
            crate::lsp::LspInlayHint {
                line: 0,
                col: 9,
                label: "name=".to_string(),
            },
            crate::lsp::LspInlayHint {
                line: 0,
                col: 17,
                label: "other=".to_string(),
            },
            crate::lsp::LspInlayHint {
                line: 0,
                col: 28,
                label: "payload:".to_string(),
            },
            crate::lsp::LspInlayHint {
                line: 0,
                col: 34,
                label: ": Unknown".to_string(),
            },
        ];

        let out = python_positional_inlay_hints_from_lsp(text, &hints);

        assert_eq!(
            out,
            vec![
                PythonInlayHint {
                    byte_offset: 6,
                    label: Arc::<str>::from("id: "),
                },
                PythonInlayHint {
                    byte_offset: 28,
                    label: Arc::<str>::from("payload: "),
                },
            ]
        );
    }

    #[test]
    fn python_inlay_hints_shift_with_local_edits() {
        let mut hints = vec![
            PythonInlayHint {
                byte_offset: 10,
                label: Arc::<str>::from("first: "),
            },
            PythonInlayHint {
                byte_offset: 30,
                label: Arc::<str>::from("second: "),
            },
        ];

        shift_python_inlay_hints_for_edits(
            &mut hints,
            &[
                SyncEdit::Insert {
                    offset: 0,
                    text: "\n".to_string(),
                },
                SyncEdit::Delete { offset: 12, len: 2 },
            ],
        );

        assert_eq!(hints[0].byte_offset, 11);
        assert_eq!(hints[1].byte_offset, 29);

        shift_python_inlay_hints_for_edits(&mut hints, &[SyncEdit::Delete { offset: 10, len: 3 }]);

        assert_eq!(
            hints,
            vec![PythonInlayHint {
                byte_offset: 26,
                label: Arc::<str>::from("second: "),
            }]
        );
    }

    #[test]
    fn fuzzy_match_preserves_target_indices_case_insensitively() {
        assert_eq!(fuzzy_match("rtr", "RRiter"), Some(vec![0, 3, 5]));
        assert_eq!(fuzzy_match("IDE", "IntegratedDevEnv"), Some(vec![0, 9, 11]));
        assert_eq!(fuzzy_match("xyz", "RRiter"), None);
    }

    #[test]
    fn lsp_log_filter_matches_text_direction_and_case_modes() {
        let send = crate::lsp::LogEntry {
            text: "[LSP SEND]\n{\"method\":\"textDocument/didChange\"}".to_string(),
            spans: Vec::new(),
            folds: Vec::new(),
            created_at: std::time::Instant::now(),
        };
        let recv = crate::lsp::LogEntry {
            text: "[LSP RECV]\n{\"method\":\"window/logMessage\"}".to_string(),
            spans: Vec::new(),
            folds: Vec::new(),
            created_at: std::time::Instant::now(),
        };
        let other = crate::lsp::LogEntry {
            text: "ruff stderr warning".to_string(),
            spans: Vec::new(),
            folds: Vec::new(),
            created_at: std::time::Instant::now(),
        };
        let filter = LspLogFilter {
            query: "change".to_string(),
            case_sensitive: false,
            show_send: true,
            show_recv: false,
            show_other: false,
        };

        assert!(filter.matches(&send));
        assert!(!filter.matches(&recv));
        assert!(!filter.matches(&other));

        let case_filter = LspLogFilter {
            query: "CHANGE".to_string(),
            case_sensitive: true,
            show_send: true,
            show_recv: true,
            show_other: true,
        };
        assert!(!case_filter.matches(&send));
    }

    #[test]
    fn panel_metadata_maps_to_labels_and_icons() {
        assert_eq!(PanelId::Explorer.label(), "Проводник");
        assert_eq!(PanelId::Search.label(), "Поиск");
        assert_eq!(PanelId::Terminal.label(), "Терминал");
        assert!(PanelId::Search.icon() == crate::widgets::IconType::Search);
        assert!(PanelId::Problems.icon() == crate::widgets::IconType::Problems);
        assert!(PanelId::LspServers.icon() == crate::widgets::IconType::LspServers);
        assert!(PanelId::ApiClient.icon() == crate::widgets::IconType::Api);
        assert_eq!(IdePanelState::default().bottom_height, 180.0);
    }

    fn problem_test_diagnostic(message: &str) -> crate::lsp::Diagnostic {
        crate::lsp::Diagnostic {
            start_line: 0,
            start_col: 0,
            end_line: 0,
            end_col: 1,
            severity: crate::lsp::DiagSeverity::Error,
            code: None,
            code_href: None,
            message: std::sync::Arc::<str>::from(message),
            source: None,
            quickfixes: Box::new([]),
            tags: Box::new([]),
        }
    }

    #[test]
    fn bug_34_stale_problem_rows_do_not_consume_layout_slots() {
        let path = std::path::PathBuf::from("query.sql");
        let mut panel = IdePanelState::default();
        panel.query_problem_path = Some(path.clone());
        panel.query_problem_diagnostics = vec![problem_test_diagnostic("valid")];
        panel.flat_diags = vec![(path.clone(), 0), (path.clone(), 99), (path.clone(), 0)];

        let visible = panel
            .flat_diags
            .iter()
            .filter(|(path, index)| panel.problem_row_visible(None, path, *index))
            .count();
        assert_eq!(visible, 2);
        assert!(!panel.problem_row_visible(None, &path, 99));
    }

    #[test]
    fn bug_38_problem_scroll_height_counts_only_renderable_rows() {
        let path = std::path::PathBuf::from("query.sql");
        let mut panel = IdePanelState::default();
        panel.query_problem_path = Some(path.clone());
        panel.query_problem_diagnostics = vec![problem_test_diagnostic("valid")];
        panel.flat_diags = vec![(path.clone(), usize::MAX), (path.clone(), 0), (path, 42)];
        let rows = panel.visible_problem_row_count(None);
        assert_eq!(rows, 2);
        assert_eq!(problems_scroll_content_height(rows, 24.0), 48.0);
        assert_eq!(problems_scroll_content_height(rows, f32::NAN), 0.0);
    }
