    fn test_line_offsets(text: &str) -> Vec<usize> {
        std::iter::once(0)
            .chain(
                text.char_indices()
                    .filter_map(|(index, ch)| (ch == '\n').then_some(index + 1)),
            )
            .collect()
    }

    fn completion_for(
        metadata: &DatabaseQueryCompletionMetadata,
        sql: &str,
        cursor: usize,
    ) -> Vec<(String, String)> {
        let context = database_query_completion_context(sql, cursor);
        let base = analyze_database_query_sql(metadata, sql);
        let analysis =
            completion_recovery_analysis(metadata, sql, cursor, &context, &base).unwrap_or(base);
        completion_words_for_context(metadata, &analysis, &context, cursor)
    }

    #[test]
    fn completion_resolves_alias_columns() {
        let metadata = DatabaseQueryCompletionMetadata {
            tables: vec!["users".to_string()],
            columns: vec![DatabaseQueryCompletionColumn {
                table_name: "users".to_string(),
                column_name: "email".to_string(),
                data_type: "text".to_string(),
            }],
            ..DatabaseQueryCompletionMetadata::default()
        };
        let words = completion_for(&metadata, "select u. from users as u", 9);
        assert!(words.iter().any(|(word, _)| word == "email"));
    }

    #[test]
    fn completion_filters_alias_columns_by_prefix_and_table_metadata() {
        let metadata = DatabaseQueryCompletionMetadata {
            tables: vec!["booking".to_string(), "car_wash".to_string()],
            columns: vec![
                DatabaseQueryCompletionColumn {
                    table_name: "booking".to_string(),
                    column_name: "car_wash_id".to_string(),
                    data_type: "bigint".to_string(),
                },
                DatabaseQueryCompletionColumn {
                    table_name: "booking".to_string(),
                    column_name: "customer_id".to_string(),
                    data_type: "bigint".to_string(),
                },
                DatabaseQueryCompletionColumn {
                    table_name: "car_wash".to_string(),
                    column_name: "capacity".to_string(),
                    data_type: "integer".to_string(),
                },
            ],
            ..DatabaseQueryCompletionMetadata::default()
        };
        let sql = "SELECT b.ca FROM booking AS b";
        let words = completion_for(&metadata, sql, "SELECT b.ca".len());
        assert!(words.iter().any(|(word, _)| word == "car_wash_id"));
        assert!(
            !words
                .iter()
                .any(|(word, detail)| { word == "capacity" || detail.starts_with("car_wash ·") })
        );
    }

    #[test]
    fn completion_after_select_prefix_keeps_from_ahead_of_identifiers() {
        let metadata = DatabaseQueryCompletionMetadata {
            columns: vec![
                DatabaseQueryCompletionColumn {
                    table_name: "items".to_string(),
                    column_name: "frame".to_string(),
                    data_type: "text".to_string(),
                },
                DatabaseQueryCompletionColumn {
                    table_name: "items".to_string(),
                    column_name: "far_value".to_string(),
                    data_type: "text".to_string(),
                },
            ],
            functions: vec!["format".to_string()],
            ..DatabaseQueryCompletionMetadata::default()
        };

        for prefix in ["F", "FR", "FRO", "fr"] {
            let sql = format!("SELECT *\n{prefix}");
            let words = completion_for(&metadata, &sql, sql.len());
            let from_index = words
                .iter()
                .position(|(word, detail)| word == "FROM" && detail == "SQL")
                .expect("FROM completion");
            assert!(from_index <= 1, "prefix={prefix:?}, words={words:?}");
            assert!(!words.iter().any(|(word, detail)| {
                detail == "alias SELECT" && word.trim_matches('"').eq_ignore_ascii_case(prefix)
            }));
            assert_eq!(
                words
                    .iter()
                    .filter(|(word, _)| word.eq_ignore_ascii_case("FROM"))
                    .count(),
                1
            );
        }
    }

    #[test]
    fn sql_completion_filter_uses_shared_exact_prefix_and_fuzzy_matching() {
        assert_eq!(
            crate::app::autocomplete_match_candidate("from", "FROM").map(|(kind, _)| kind),
            Some(crate::app::AutocompleteMatchKind::Exact)
        );
        assert_eq!(
            crate::app::autocomplete_match_candidate("fr", "FROM").map(|(kind, _)| kind),
            Some(crate::app::AutocompleteMatchKind::Prefix)
        );
        assert_eq!(
            crate::app::autocomplete_match_candidate("fm", "format").map(|(kind, _)| kind),
            Some(crate::app::AutocompleteMatchKind::Fuzzy)
        );
    }

    #[test]
    fn sql_analysis_diagnostics_use_standard_editor_diagnostic_shape() {
        let text = "SELECT * FROM";
        let analysis = SqlAnalysis {
            diagnostics: vec![SqlAnalysisDiagnostic {
                range: text.len()..text.len(),
                severity: SqlDiagnosticSeverity::Error,
                code: "SQL001",
                message: "Ожидалось имя таблицы".to_string(),
            }],
            ..SqlAnalysis::default()
        };
        let diagnostics = database_query_editor_diagnostics(&analysis, None, text, &[0]);

        assert_eq!(diagnostics.len(), 1);
        let diagnostic = &diagnostics[0];
        assert_eq!(diagnostic.code.as_deref(), Some("SQL001"));
        assert_eq!(diagnostic.source.as_deref(), Some("RRiter SQL"));
        assert_eq!(diagnostic.severity, crate::lsp::DiagSeverity::Error);
        assert_eq!(diagnostic.start_line, diagnostic.end_line);
        assert_eq!(diagnostic.start_col, diagnostic.end_col);
    }

    #[test]
    fn sql_warning_ranges_round_trip_through_lsp_positions() {
        let text = "SELECT *\nFROM \"public\".\"car__model\"\nLIMIT 100;";
        let analysis = analyze_sql(text);
        let diagnostics =
            database_query_editor_diagnostics(&analysis, None, text, &test_line_offsets(text));

        for code in ["SQL117", "SQL119"] {
            let source = analysis
                .diagnostics
                .iter()
                .find(|diagnostic| diagnostic.code == code)
                .expect("source diagnostic");
            let editor = diagnostics
                .iter()
                .find(|diagnostic| diagnostic.code.as_deref() == Some(code))
                .expect("editor diagnostic");
            let start = crate::lsp::lsp_pos_to_offset(text, editor.start_line, editor.start_col);
            let end = crate::lsp::lsp_pos_to_offset(text, editor.end_line, editor.end_col);

            assert_eq!(start..end, source.range);
        }
    }

    #[test]
    fn sql_warning_lsp_round_trip_is_utf16_safe_before_star() {
        let text = "SELECT 'Ж', *\nFROM \"public\".\"car__model\"\nLIMIT 100;";
        let analysis = analyze_sql(text);
        let diagnostics =
            database_query_editor_diagnostics(&analysis, None, text, &test_line_offsets(text));
        let source = analysis
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "SQL119")
            .expect("SQL119 source");
        let editor = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code.as_deref() == Some("SQL119"))
            .expect("SQL119 editor");

        let start = crate::lsp::lsp_pos_to_offset(text, editor.start_line, editor.start_col);
        let end = crate::lsp::lsp_pos_to_offset(text, editor.end_line, editor.end_col);
        assert_eq!(start..end, source.range);
        assert_eq!(text.get(start..end), Some("*"));
    }

    #[test]
    fn sql004_round_trips_to_hover_diagnostic_with_utf16_safe_comma_range() {
        let text = "SELECT Ж, FROM car__body_type";
        let analysis = analyze_sql(text);
        let source = analysis
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "SQL004")
            .expect("SQL004 source");
        let diagnostics =
            database_query_editor_diagnostics(&analysis, None, text, &test_line_offsets(text));
        let editor = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code.as_deref() == Some("SQL004"))
            .expect("SQL004 editor");
        let start = crate::lsp::lsp_pos_to_offset(text, editor.start_line, editor.start_col);
        let end = crate::lsp::lsp_pos_to_offset(text, editor.end_line, editor.end_col);

        assert_eq!(editor.severity, crate::lsp::DiagSeverity::Error);
        assert!(editor.message.contains("запятая перед FROM"));
        assert_eq!(start..end, source.range);
        assert_eq!(text.get(start..end), Some(","));
    }

    #[test]
    fn next_sql_diagnostic_wraps_after_last_range() {
        let text = "SELECT one;\nSELECT two;";
        let analysis = SqlAnalysis {
            diagnostics: vec![
                SqlAnalysisDiagnostic {
                    range: 7..10,
                    severity: SqlDiagnosticSeverity::Error,
                    code: "SQL001",
                    message: "one".to_string(),
                },
                SqlAnalysisDiagnostic {
                    range: 19..22,
                    severity: SqlDiagnosticSeverity::Warning,
                    code: "SQL002",
                    message: "two".to_string(),
                },
            ],
            ..SqlAnalysis::default()
        };
        let diagnostics = database_query_editor_diagnostics(&analysis, None, text, &[0, 12]);

        assert_eq!(
            next_database_query_diagnostic_offset(&diagnostics, text, 0),
            Some(7)
        );
        assert_eq!(
            next_database_query_diagnostic_offset(&diagnostics, text, 7),
            Some(19)
        );
        assert_eq!(
            next_database_query_diagnostic_offset(&diagnostics, text, 22),
            Some(7)
        );
    }

    #[test]
    fn semantic_analysis_reports_unknown_and_ambiguous_columns() {
        let metadata = DatabaseQueryCompletionMetadata {
            tables: vec!["booking".to_string(), "car_wash".to_string()],
            columns: vec![
                DatabaseQueryCompletionColumn {
                    table_name: "booking".to_string(),
                    column_name: "id".to_string(),
                    data_type: "bigint".to_string(),
                },
                DatabaseQueryCompletionColumn {
                    table_name: "car_wash".to_string(),
                    column_name: "id".to_string(),
                    data_type: "bigint".to_string(),
                },
            ],
            ..DatabaseQueryCompletionMetadata::default()
        };
        let analysis = analyze_database_query_sql(
            &metadata,
            "SELECT id, b.missing, x.id FROM booking b JOIN car_wash cw ON cw.id = b.id",
        );
        for code in ["SQL204", "SQL203", "SQL206"] {
            assert!(
                analysis
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code == code),
                "missing {code}: {:?}",
                analysis.diagnostics
            );
        }
    }

    #[test]
    fn semantic_analysis_reports_unknown_public_table_but_allows_cte() {
        let metadata = DatabaseQueryCompletionMetadata {
            tables: vec!["booking".to_string()],
            columns: vec![DatabaseQueryCompletionColumn {
                table_name: "booking".to_string(),
                column_name: "id".to_string(),
                data_type: "bigint".to_string(),
            }],
            ..DatabaseQueryCompletionMetadata::default()
        };
        let unknown = analyze_database_query_sql(&metadata, "SELECT x.id FROM missing x");
        assert!(
            unknown
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "SQL202")
        );

        let cte = analyze_database_query_sql(
            &metadata,
            "WITH recent AS (SELECT id FROM booking) SELECT r.id FROM recent r",
        );
        assert!(
            !cte.diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "SQL202")
        );
    }

    #[test]
    fn completion_uses_postgresql_metadata_for_the_current_ast_context() {
        let metadata = DatabaseQueryCompletionMetadata {
            enum_values: vec!["active".to_string()],
            functions: vec!["jsonb_set".to_string()],
            operators: vec!["->>".to_string()],
            ..DatabaseQueryCompletionMetadata::default()
        };
        let analysis = SqlAnalysis::default();
        let base = SqlCompletionContext {
            replace_range: 0..0,
            scope: 0..0,
            ..SqlCompletionContext::default()
        };
        let columns = completion_words_for_context(
            &metadata,
            &analysis,
            &SqlCompletionContext {
                kind: SqlCompletionKind::Column,
                ..base.clone()
            },
            0,
        );
        assert!(columns.iter().any(|(word, _)| word == "jsonb_set"));
        let operators = completion_words_for_context(
            &metadata,
            &analysis,
            &SqlCompletionContext {
                kind: SqlCompletionKind::Operator,
                ..base.clone()
            },
            0,
        );
        assert!(operators.iter().any(|(word, _)| word == "->>"));
        let values = completion_words_for_context(
            &metadata,
            &analysis,
            &SqlCompletionContext {
                kind: SqlCompletionKind::Value,
                ..base
            },
            0,
        );
        assert!(values.iter().any(|(word, _)| word == "'active'"));
    }

    #[test]
    fn completion_catalog_queries_have_one_from_clause_and_one_statement() {
        for sql in [
            QUERY_COMPLETION_COLUMNS_SQL,
            QUERY_COMPLETION_ENUMS_SQL,
            QUERY_COMPLETION_FUNCTIONS_SQL,
            QUERY_COMPLETION_OPERATORS_SQL,
        ] {
            assert_eq!(
                crate::languages::sql::scan_statements(sql).len(),
                1,
                "{sql}"
            );
        }
        assert_eq!(
            QUERY_COMPLETION_FUNCTIONS_SQL
                .to_ascii_uppercase()
                .matches("FROM PG_PROC")
                .count(),
            1
        );
    }

