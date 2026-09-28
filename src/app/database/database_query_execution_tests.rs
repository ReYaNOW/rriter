    #[test]
    fn execution_target_prefers_selection_then_statement() {
        let text = "select 1;\nselect 2;";
        assert_eq!(
            query_execution_target(text, Some((10, 18)), 0),
            Some(("select 2".to_string(), 10))
        );
        assert_eq!(
            query_execution_target(text, None, 3),
            Some(("select 1;".to_string(), 0))
        );
    }

    #[test]
    fn a4_b004_invalid_utf8_selection_falls_back_to_current_statement() {
        let text = "SELECT Ж;\nSELECT 2;";
        assert_eq!(
            query_execution_target(text, Some((7, 8)), text.len()),
            Some(("SELECT 2;".to_string(), "SELECT Ж;\n".len()))
        );
    }

    #[test]
    fn postgres_character_offsets_map_unicode_to_bytes() {
        assert_eq!(postgres_character_to_byte("Жx", 0), 0);
        assert_eq!(postgres_character_to_byte("Жx", 1), 2);
        assert_eq!(postgres_character_to_byte("Жx", 2), 3);
    }

    #[test]
    fn execution_effects_keep_returned_and_changed_rows_separate() {
        let mut effects = SqlExecutionEffects {
            returned_rows: 100,
            ..SqlExecutionEffects::default()
        };
        effects.record_command(crate::languages::sql::SqlStatementKind::Query, 100);
        assert_eq!(effects.returned_rows, 100);
        assert_eq!(effects.changed_rows, 0);
        assert!(!effects.requires_review());

        effects.record_command(crate::languages::sql::SqlStatementKind::Mutation, 0);
        assert!(!effects.requires_review());
        effects.record_command(crate::languages::sql::SqlStatementKind::Mutation, 1);
        assert_eq!(effects.changed_rows, 1);
        assert!(effects.requires_review());
    }

    #[test]
    fn definition_requires_review_without_changed_rows() {
        let mut effects = SqlExecutionEffects::default();
        effects.record_command(crate::languages::sql::SqlStatementKind::Definition, 0);
        assert_eq!(effects.changed_rows, 0);
        assert!(effects.requires_review());
    }

    #[test]
    fn a4_b007_explain_analyze_mutation_marks_effects_for_review() {
        let statements =
            crate::languages::sql::validate_managed_user_sql("UPDATE items SET value = 2").unwrap();
        let mut effects = SqlExecutionEffects::default();
        mark_explain_analyze_side_effects(
            DatabaseQueryMode::ExplainAnalyze,
            &statements,
            &mut effects,
        );
        assert!(effects.has_other_effect);
        assert!(effects.requires_review());
    }

    #[test]
    fn postgres_diagnostics_keep_source_code_and_details() {
        let text = "SELECT Ж";
        let backend = DatabaseQueryDiagnostic {
            start_byte: "SELECT ".len(),
            end_byte: "SELECT Ж".len(),
            message: "syntax error".to_string(),
            detail: Some("detail".to_string()),
            hint: Some("hint".to_string()),
            sqlstate: Some("42601".to_string()),
        };
        let diagnostics =
            database_query_editor_diagnostics(&SqlAnalysis::default(), Some(&backend), text, &[0]);

        assert_eq!(diagnostics.len(), 1);
        let diagnostic = &diagnostics[0];
        assert_eq!(diagnostic.code.as_deref(), Some("42601"));
        assert_eq!(diagnostic.source.as_deref(), Some("PostgreSQL"));
        assert!(diagnostic.message.contains("detail"));
        assert!(diagnostic.message.contains("Подсказка: hint"));
    }

    #[test]
    fn explain_rejects_multiple_statements() {
        let statements = validate_managed_user_sql("select 1; select 2").unwrap();
        assert!(
            explain_sql(
                "select 1; select 2",
                &statements,
                DatabaseQueryMode::Explain
            )
            .is_err()
        );
    }

    #[test]
    fn execution_target_uses_whole_document_when_cursor_has_no_statement() {
        let text = "  SELECT 1;
SELECT 2;  ";
        assert_eq!(
            query_execution_target(text, None, text.len()),
            Some(("SELECT 2;".to_string(), 12))
        );
        assert_eq!(query_execution_target("   ", None, 0), None);
    }

    #[test]
    fn result_set_limit_is_enforced_before_growth() {
        let mut results = vec![DatabaseQueryResultSet::default(); MAX_RESULT_SETS];
        let error = push_result(&mut results, DatabaseQueryResultSet::default()).unwrap_err();
        assert!(matches!(error, DatabaseBackendError::LimitExceeded(_)));
        assert_eq!(results.len(), MAX_RESULT_SETS);
    }

    #[test]
    fn explain_modes_preserve_statement_and_analyze_flag() {
        let statements = validate_managed_user_sql("SELECT * FROM public.users").unwrap();
        let explain = explain_sql(
            "SELECT * FROM public.users",
            &statements,
            DatabaseQueryMode::Explain,
        )
        .unwrap();
        assert!(explain.text.starts_with("EXPLAIN (VERBOSE, FORMAT TEXT)"));
        assert_eq!(
            explain.prefix_characters,
            "EXPLAIN (VERBOSE, FORMAT TEXT) ".chars().count()
        );
        let analyze = explain_sql(
            "SELECT * FROM public.users",
            &statements,
            DatabaseQueryMode::ExplainAnalyze,
        )
        .unwrap();
        assert!(analyze.text.contains("ANALYZE"));
        assert!(analyze.text.ends_with("SELECT * FROM public.users"));
    }

    #[test]
    fn result_memory_estimate_includes_cells_columns_and_tags() {
        let result = DatabaseQueryResultSet {
            title: "Result 1".to_string(),
            columns: vec!["value".to_string()],
            rows: vec![vec![DatabaseQueryCell {
                value: Some("hello".to_string()),
            }]],
            command_kind: "SELECT".to_string(),
            ..DatabaseQueryResultSet::default()
        };
        assert!(result.estimated_bytes() >= "Result 1valuehelloSELECT".len());
    }

    #[test]
    fn optional_postgres_review_transaction_rolls_back() {
        let Ok(url) = std::env::var("RRITER_TEST_POSTGRES_URL") else {
            return;
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls)
                .await
                .expect("RRITER_TEST_POSTGRES_URL must accept a direct non-TLS test connection");
            let driver = tokio::spawn(async move { connection.await });
            client.batch_execute(
                "BEGIN; CREATE TEMP TABLE rriter_query_review_test(value integer);",
            ).await.unwrap();
            let messages = client.simple_query(
                "INSERT INTO rriter_query_review_test VALUES (1) RETURNING value; SELECT value FROM rriter_query_review_test;",
            ).await.unwrap();
            let rows = messages.iter().filter(|message| {
                matches!(message, tokio_postgres::SimpleQueryMessage::Row(_))
            }).count();
            assert_eq!(rows, 2);
            client.batch_execute("ROLLBACK").await.unwrap();
            let exists: bool = client.query_one(
                "SELECT to_regclass('pg_temp.rriter_query_review_test') IS NOT NULL",
                &[],
            ).await.unwrap().get(0);
            assert!(!exists);
            drop(client);
            driver.await.unwrap().unwrap();
        });
    }

    #[test]
    fn command_kind_discards_row_counts_and_keeps_sql_command() {
        let sql = "SELECT * FROM items; UPDATE items SET value = 1;";
        let statements = crate::languages::sql::validate_managed_user_sql(sql).unwrap();
        assert_eq!(command_kind(sql, statements.first()), "SELECT");
        assert_eq!(command_kind(sql, statements.get(1)), "UPDATE");
    }
