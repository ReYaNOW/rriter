    #[test]
    fn query_result_height_clamps_to_editor_and_panel_space() {
        assert_eq!(database_query_results_height(260.0, 900.0, 0.0, 1.0), 260.0);
        assert_eq!(database_query_results_height(80.0, 900.0, 0.0, 1.0), 140.0);
        assert_eq!(
            database_query_results_height(900.0, 900.0, 180.0, 1.0),
            500.0
        );
        assert_eq!(database_query_results_height(260.0, 600.0, 0.0, 1.5), 270.0);
    }

    #[test]
    fn history_preview_is_bounded_to_twenty_lines_and_marks_truncation() {
        let sql = (1..=21)
            .map(|line| format!("SELECT {line};"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(database_query_history_preview_lines(&sql), 20);
        assert!(database_query_history_is_truncated(&sql));
        assert_eq!(sql.lines().take(20).count(), 20);
    }

    #[test]
    fn history_normalization_bounds_sensitive_growth() {
        let mut entry = DatabaseQueryHistoryEntry {
            sql: "x".repeat(super::super::MAX_SQL_CONSOLE_BYTES + 10),
            error_summary: Some("e".repeat(5000)),
            ..DatabaseQueryHistoryEntry::default()
        };
        entry.normalize();
        assert_eq!(entry.sql.len(), super::super::MAX_SQL_CONSOLE_BYTES);
        assert_eq!(entry.error_summary.as_ref().unwrap().len(), 4096);
    }

    #[test]
    fn cancelled_running_query_becomes_a_sanitizable_history_entry() {
        let mut state = DatabaseQueryTabState::default();
        state.mark_running("ALTER ROLE demo PASSWORD 'secret'".to_string(), 123);
        let mut entry = state
            .take_cancelled_history(
                DatabaseConnectionId(7),
                "postgres",
                super::super::SqlConsoleId(9),
            )
            .unwrap();
        entry.sql = sanitize_history_sql(&entry.sql);
        assert!(!entry.sql.contains("secret"));
        assert_eq!(entry.started_unix_ms, 123);
        assert_eq!(
            entry.error_summary.as_deref(),
            Some("Запрос отменён пользователем")
        );
        assert!(!state.running);
        assert!(state.running_sql.is_none());
        assert_eq!(state.running_started_unix_ms, 0);
    }

    #[test]
    fn history_sanitizer_redacts_passwords_and_connection_uris() {
        let sql = "ALTER ROLE demo PASSWORD 'top secret'; SELECT 'postgres://u:p@host/db?password=hidden', 'jdbc:postgresql://u:q@host/db';";
        let clean = sanitize_history_sql(sql);
        assert!(!clean.contains("top secret"));
        assert!(!clean.contains(":p@"));
        assert!(!clean.contains(":q@"));
        assert!(!clean.contains("hidden"));
        assert!(clean.contains("<redacted>"));
    }

    #[test]
    fn query_scroll_limits_use_shared_resized_column_widths() {
        let meta = DatabaseQueryTabMeta {
            connection_id: DatabaseConnectionId(1),
            database_name: "postgres".to_string(),
            console_id: super::super::SqlConsoleId(1),
            title: "SQL".to_string(),
        };
        let mut state = DatabaseQueryTabState::default();
        state.results.push(DatabaseQueryResultSet {
            columns: vec!["id".to_string(), "description".to_string()],
            rows: vec![vec![
                DatabaseQueryCell::default(),
                DatabaseQueryCell::default(),
            ]],
            ..DatabaseQueryResultSet::default()
        });
        crate::app::database::set_database_column_width(
            &mut state.result_view.column_widths,
            "id",
            80.0,
        );
        crate::app::database::set_database_column_width(
            &mut state.result_view.column_widths,
            "description",
            420.0,
        );
        let (max_x, _) = database_query_scroll_limits(&meta, &state, &[], 300.0, 200.0, 1.0);
        assert_eq!(max_x, 200.0);
    }

    #[test]
    fn bug_10_history_content_height_uses_same_rounded_step_as_renderer() {
        let entries = vec![
            DatabaseQueryHistoryEntry {
                sql: "select 1".to_string(),
                ..DatabaseQueryHistoryEntry::default()
            };
            200
        ];
        let scale = 1.25;
        let content = database_query_history_content_height(entries.iter(), scale);
        let laid_out = entries
            .iter()
            .map(|entry| database_query_history_entry_height_px(&entry.sql, scale))
            .sum::<f32>();
        assert_eq!(content, laid_out);
        assert_ne!(
            content,
            entries
                .iter()
                .map(|entry| database_query_history_entry_height(&entry.sql) * scale)
                .sum::<f32>()
        );
    }

    #[test]
    fn history_layout_cache_reuses_scroll_only_work_and_invalidates_sources() {
        let meta = DatabaseQueryTabMeta {
            connection_id: DatabaseConnectionId(7),
            database_name: "postgres".to_string(),
            console_id: super::super::SqlConsoleId(1),
            title: "SQL".to_string(),
        };
        let mut state = DatabaseQueryTabState {
            history_open: true,
            ..DatabaseQueryTabState::default()
        };
        let mut history = vec![
            DatabaseQueryHistoryEntry {
                connection_id: meta.connection_id,
                database_name: meta.database_name.clone(),
                sql: "select 1".to_string(),
                ..DatabaseQueryHistoryEntry::default()
            },
            DatabaseQueryHistoryEntry {
                connection_id: meta.connection_id,
                database_name: meta.database_name.clone(),
                sql: "select 2\nfrom t".to_string(),
                ..DatabaseQueryHistoryEntry::default()
            },
        ];

        let first = database_query_scroll_limits(&meta, &state, &history, 300.0, 20.0, 1.0);
        let second = database_query_scroll_limits(&meta, &state, &history, 300.0, 20.0, 1.0);
        assert_eq!(first, second);
        assert_eq!(
            state
                .result_view
                .history_layout_cache
                .borrow()
                .rebuild_count,
            1
        );

        history.push(DatabaseQueryHistoryEntry {
            connection_id: meta.connection_id,
            database_name: meta.database_name.clone(),
            sql: "select 3".to_string(),
            ..DatabaseQueryHistoryEntry::default()
        });
        state.result_view.invalidate_history_layout();
        let _ = database_query_scroll_limits(&meta, &state, &history, 300.0, 20.0, 1.0);
        assert_eq!(
            state
                .result_view
                .history_layout_cache
                .borrow()
                .rebuild_count,
            2
        );

        history.remove(0);
        state.result_view.invalidate_history_layout();
        let _ = database_query_scroll_limits(&meta, &state, &history, 300.0, 20.0, 1.0);
        assert_eq!(
            state
                .result_view
                .history_layout_cache
                .borrow()
                .rebuild_count,
            3
        );

        let _ = database_query_scroll_limits(&meta, &state, &history, 300.0, 20.0, 1.25);
        assert_eq!(
            state
                .result_view
                .history_layout_cache
                .borrow()
                .rebuild_count,
            4
        );
        let other_meta = DatabaseQueryTabMeta {
            database_name: "template1".to_string(),
            ..meta.clone()
        };
        let _ = database_query_scroll_limits(&other_meta, &state, &history, 300.0, 20.0, 1.25);
        assert_eq!(
            state
                .result_view
                .history_layout_cache
                .borrow()
                .rebuild_count,
            5
        );
    }

    #[test]
    fn history_layout_cache_seeks_visible_entries_and_preserves_selection_mapping() {
        let meta = DatabaseQueryTabMeta {
            connection_id: DatabaseConnectionId(9),
            database_name: "app".to_string(),
            console_id: super::super::SqlConsoleId(2),
            title: "SQL".to_string(),
        };
        let mut history = (0..5)
            .map(|index| DatabaseQueryHistoryEntry {
                connection_id: meta.connection_id,
                database_name: meta.database_name.clone(),
                sql: format!("select {index}"),
                started_unix_ms: index,
                ..DatabaseQueryHistoryEntry::default()
            })
            .collect::<Vec<_>>();
        history.insert(
            2,
            DatabaseQueryHistoryEntry {
                connection_id: meta.connection_id,
                database_name: "other".to_string(),
                sql: "select 'other'".to_string(),
                started_unix_ms: 99,
                ..DatabaseQueryHistoryEntry::default()
            },
        );
        let state = DatabaseQueryTabState::default();
        let layout = state.result_view.history_layout(&meta, &history, 1.0);
        assert_eq!(layout.entries().len(), 5);
        assert_eq!(layout.content_height(), 250.0);
        assert_eq!(layout.visible_range(0.0, 49.0), 0..1);
        assert_eq!(layout.visible_range(50.0, 50.0), 1..2);
        assert_eq!(layout.visible_range(75.0, 1.0), 1..2);
        assert_eq!(layout.visible_range(200.0, 50.0), 4..5);
        assert_eq!(
            history[layout.entries()[0].history_index].started_unix_ms,
            4
        );
        assert_eq!(
            history[layout.entries()[1].history_index].started_unix_ms,
            3
        );
    }

    #[test]
    fn bug_16_history_trim_removes_one_prefix_in_linear_pass() {
        let mut history = (0..10)
            .map(|index| DatabaseQueryHistoryEntry {
                sql: format!("select {index} -- {}", "x".repeat(64)),
                started_unix_ms: index,
                ..DatabaseQueryHistoryEntry::default()
            })
            .collect::<Vec<_>>();
        let keep_bytes = database_query_history_entry_bytes(&history[8])
            + database_query_history_entry_bytes(&history[9]);
        trim_database_query_history(&mut history, 8, keep_bytes);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].started_unix_ms, 8);
        assert_eq!(history[1].started_unix_ms, 9);
    }

    #[test]
    fn history_normalization_truncates_unicode_on_character_boundaries() {
        let mut entry = DatabaseQueryHistoryEntry {
            database_name: "я".repeat(100),
            sql: "🙂".repeat(super::super::MAX_SQL_CONSOLE_BYTES / 4 + 2),
            error_summary: Some("Ж".repeat(2_049)),
            ..DatabaseQueryHistoryEntry::default()
        };
        entry.normalize();
        assert!(entry.database_name.len() <= 128);
        assert!(entry.sql.len() <= super::super::MAX_SQL_CONSOLE_BYTES);
        assert!(entry.error_summary.as_ref().unwrap().len() <= 4_096);
        assert!(
            entry
                .database_name
                .is_char_boundary(entry.database_name.len())
        );
        assert!(entry.sql.is_char_boundary(entry.sql.len()));
    }

    #[test]
    fn history_sanitizer_safely_bounds_long_unicode_sql() {
        let sql = "🙂".repeat(super::super::MAX_SQL_CONSOLE_BYTES / 4 + 2);
        let clean = sanitize_history_sql(&sql);
        assert!(clean.len() <= super::super::MAX_SQL_CONSOLE_BYTES);
        assert!(clean.is_char_boundary(clean.len()));
    }

    #[test]
    fn query_result_height_never_exceeds_tiny_available_space() {
        assert_eq!(database_query_results_height(260.0, 250.0, 0.0, 1.0), 30.0);
        assert_eq!(database_query_results_height(260.0, 180.0, 0.0, 1.0), 0.0);
    }

    #[test]
    fn resetting_query_result_scroll_ends_all_active_drags() {
        let mut state = DatabaseQueryResultViewState::default();
        for scroll in [
            &mut state.scroll_x,
            &mut state.scroll_y,
            &mut state.review_message_scroll_y,
        ] {
            scroll.current = 10.0;
            scroll.target = 20.0;
            scroll.velocity = 3.0;
            scroll.is_dragging = true;
            scroll.drag_offset = 4.0;
        }
        state.reset_scroll();
        assert!(state.scroll_x.is_settled());
        assert!(state.scroll_y.is_settled());
        assert!(state.review_message_scroll_y.is_settled());
        assert_eq!(state.scroll_x.drag_offset, 0.0);
        assert_eq!(state.scroll_y.drag_offset, 0.0);
        assert_eq!(state.review_message_scroll_y.drag_offset, 0.0);
    }

    #[test]
    fn old_history_json_defaults_returned_rows_to_zero() {
        let json = r#"{
            "connection_id": 1,
            "database_name": "db",
            "console_id": 2,
            "sql": "select 1",
            "started_unix_ms": 3,
            "duration_ms": 4,
            "succeeded": true,
            "affected_rows": 0,
            "error_summary": null
        }"#;
        let entry: DatabaseQueryHistoryEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.returned_rows, 0);
    }
