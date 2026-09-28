mod tests {
    use super::*;

    fn diagnostics_with_code(sql: &str, code: &str) -> Vec<SqlAnalysisDiagnostic> {
        analyze_sql(sql)
            .diagnostics
            .into_iter()
            .filter(|item| item.code == code)
            .collect()
    }

    #[test]
    fn resolves_alias_from_tree_sitter_relation() {
        let sql = "SELECT b.car_wash_id FROM booking AS b WHERE b.id = 1";
        let analysis = analyze_sql(sql);
        let relation = relation_for_qualifier(&analysis, 12, "b").unwrap();
        assert_eq!(relation.table_name, "booking");
        assert_eq!(relation.alias, "b");
        assert!(
            analysis
                .qualified_references
                .iter()
                .any(|reference| reference.qualifier == "b" && reference.name == "car_wash_id")
        );
    }

    #[test]
    fn completion_after_alias_dot_is_qualified_column_context() {
        let sql = "SELECT b.ca FROM booking b";
        let context = completion_context(sql, "SELECT b.ca".len());
        assert_eq!(context.kind, SqlCompletionKind::QualifiedColumn);
        assert_eq!(context.qualifier.as_deref(), Some("b"));
        assert_eq!(context.prefix, "ca");
        assert_eq!(&sql[context.replace_range], "ca");
    }

    #[test]
    fn completion_after_comparison_does_not_offer_columns_automatically() {
        let sql = "SELECT * FROM booking WHERE id = ";
        let context = completion_context(sql, sql.len());
        assert_eq!(context.kind, SqlCompletionKind::Value);
        assert!(!context.automatic);
    }

    #[test]
    fn completion_after_column_offers_operators_without_reopening_on_value() {
        let sql = "SELECT * FROM booking WHERE id ";
        let context = completion_context(sql, sql.len());
        assert_eq!(context.kind, SqlCompletionKind::Operator);
        assert!(context.automatic);

        let with_operator = "SELECT * FROM booking WHERE id =";
        let value = completion_context(with_operator, with_operator.len());
        assert_eq!(value.kind, SqlCompletionKind::Value);
        assert!(!value.automatic);
    }

    #[test]
    fn dangerous_mutations_are_reported_from_ast() {
        let delete = analyze_sql("DELETE FROM booking");
        assert!(
            delete
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "SQL101")
        );
        let update = analyze_sql("UPDATE booking SET status = 'done'");
        assert!(
            update
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "SQL102")
        );
        let safe = analyze_sql("DELETE FROM booking WHERE id = 1");
        assert!(
            !safe
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "SQL101")
        );
    }

    #[test]
    fn nested_where_does_not_make_outer_delete_safe() {
        let sql =
            "DELETE FROM booking USING (SELECT id FROM old_booking WHERE archived = TRUE) old";
        let analysis = analyze_sql(sql);
        assert!(
            analysis
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "SQL101")
        );
    }

    #[test]
    fn syntax_errors_have_stable_ranges() {
        let sql = "SELECT FROM booking";
        let analysis = analyze_sql(sql);
        assert!(analysis.diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == SqlDiagnosticSeverity::Error
                && diagnostic.range.start < diagnostic.range.end
        }));
    }

    #[test]
    fn trailing_comma_before_from_is_one_precise_error() {
        let sql = "SELECT id, FROM car__body_type";
        let diagnostics = diagnostics_with_code(sql, "SQL004");
        assert_eq!(diagnostics.len(), 1);
        let diagnostic = &diagnostics[0];
        assert_eq!(diagnostic.severity, SqlDiagnosticSeverity::Error);
        assert_eq!(sql.get(diagnostic.range.clone()), Some(","));
        for outside in [sql.find("id").unwrap(), sql.find("car__body_type").unwrap()] {
            assert!(!diagnostic.range.contains(&outside));
        }
        assert!(diagnostic.message.contains("запятая перед FROM"));
    }

    #[test]
    fn whitespace_comments_and_unicode_keep_comma_range_exact() {
        for sql in [
            "SELECT id,\nFROM car__body_type",
            "SELECT id, -- comment\nFROM car__body_type",
            "SELECT имя, FROM car__body_type",
            "SELECT \"Имя поля\", FROM car__body_type",
        ] {
            let diagnostics = diagnostics_with_code(sql, "SQL004");
            assert_eq!(diagnostics.len(), 1, "{sql}");
            let range = diagnostics[0].range.clone();
            assert_eq!(sql.get(range.clone()), Some(","));
            assert!(sql.is_char_boundary(range.start) && sql.is_char_boundary(range.end));
        }
    }

    #[test]
    fn nested_cte_and_subquery_select_lists_are_checked() {
        for sql in [
            "WITH x AS (SELECT id, FROM car__body_type) SELECT * FROM x",
            "SELECT * FROM (SELECT id, FROM car__body_type) nested",
        ] {
            let diagnostics = diagnostics_with_code(sql, "SQL004");
            assert_eq!(diagnostics.len(), 1, "{sql}");
            assert_eq!(sql.get(diagnostics[0].range.clone()), Some(","));
        }
    }

    #[test]
    fn invalid_select_lists_in_multiple_statements_are_independent() {
        let sql = "SELECT id, FROM first_table; SELECT name, FROM second_table";
        let diagnostics = diagnostics_with_code(sql, "SQL004");
        assert_eq!(diagnostics.len(), 2);
        assert_eq!(
            diagnostics
                .iter()
                .map(|item| item.range.clone())
                .collect::<Vec<_>>(),
            sql.match_indices(',')
                .map(|(start, _)| start..start + 1)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn malformed_select_list_has_no_overlapping_parser_error_duplicate() {
        let sql = "SELECT id, FROM car__body_type";
        let analysis = analyze_sql(sql);
        let comma = sql.find(',').unwrap();
        let errors_at_comma = analysis
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == SqlDiagnosticSeverity::Error)
            .filter(|diagnostic| diagnostic.range.contains(&comma))
            .collect::<Vec<_>>();
        assert_eq!(errors_at_comma.len(), 1, "{:?}", analysis.diagnostics);
        assert_eq!(errors_at_comma[0].code, "SQL004");
    }

    #[test]
    fn valid_and_incomplete_sql_only_reports_unambiguous_trailing_comma() {
        for (sql, expected) in [
            ("SELECT id FROM car__body_type", 0),
            ("SELECT id, name FROM car__body_type", 0),
            ("SELECT func(id, name) FROM car__body_type", 0),
            ("SELECT id, from_value FROM car__body_type", 0),
            ("SELECT id, \"FROM\" FROM car__body_type", 0),
            ("SELECT id,", 0),
            ("SELECT id, F", 0),
            ("SELECT id, FR", 0),
            ("SELECT id, FROM", 1),
            ("INSERT INTO items (id, name) VALUES (1, 'one')", 0),
            ("CREATE TABLE items (id integer, name text)", 0),
            ("SELECT ARRAY[1, 2, 3] FROM items", 0),
            ("SELECT COALESCE(name, 'unknown') FROM items", 0),
        ] {
            assert_eq!(
                diagnostics_with_code(sql, "SQL004").len(),
                expected,
                "{sql}"
            );
        }
    }

    #[test]
    fn completion_ignores_strings_and_comments() {
        let string_sql = "SELECT 'b.ca' FROM booking b";
        let string_cursor = string_sql.find("ca").unwrap() + 2;
        assert_eq!(
            completion_context(string_sql, string_cursor).kind,
            SqlCompletionKind::None
        );

        let comment_sql = "SELECT 1 -- b.ca\nFROM booking b";
        let comment_cursor = comment_sql.find("ca").unwrap() + 2;
        assert_eq!(
            completion_context(comment_sql, comment_cursor).kind,
            SqlCompletionKind::None
        );
    }

    #[test]
    fn cte_and_join_aliases_are_collected_from_ast_scopes() {
        let sql = "WITH recent AS (SELECT id FROM booking) SELECT r.id, cw.name FROM recent r JOIN car_wash cw ON cw.id = r.car_wash_id";
        let analysis = analyze_sql(sql);
        assert!(analysis.ctes.iter().any(|(_, name)| name == "recent"));
        assert!(analysis.relations.iter().any(|relation| {
            relation.alias == "r" && relation.table_name == "recent" && relation.is_cte
        }));
        assert!(
            analysis
                .relations
                .iter()
                .any(|relation| { relation.alias == "cw" && relation.table_name == "car_wash" })
        );
    }

    #[test]
    fn quoted_and_schema_qualified_relations_preserve_semantic_names() {
        let sql = "SELECT b.\"Car Wash ID\" FROM public.\"Booking Entry\" AS b";
        let analysis = analyze_sql(sql);
        let relation = relation_for_qualifier(&analysis, sql.find("b.").unwrap(), "b").unwrap();
        assert_eq!(relation.schema.as_deref(), Some("public"));
        assert_eq!(relation.table_name, "Booking Entry");
        assert!(
            analysis
                .qualified_references
                .iter()
                .any(|reference| { reference.qualifier == "b" && reference.name == "Car Wash ID" })
        );
    }

    #[test]
    fn ast_lints_cover_null_comparison_and_destructive_patterns() {
        let sql = "SELECT * FROM booking WHERE deleted_at = NULL ORDER BY 1 LIMIT 10; TRUNCATE booking; DROP TABLE old_booking CASCADE";
        let analysis = analyze_sql(sql);
        for code in ["SQL103", "SQL104", "SQL113", "SQL119", "SQL120", "SQL122"] {
            assert!(
                analysis
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code == code),
                "missing {code}: {:?}",
                analysis.diagnostics
            );
        }
        let unstable_limit = analyze_sql("SELECT id FROM booking LIMIT 10");
        assert!(
            unstable_limit
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "SQL117")
        );
    }

    #[test]
    fn sql117_and_sql119_have_precise_non_overlapping_ranges() {
        let sql = "SELECT *\nFROM \"public\".\"car__model\"\nLIMIT 100;";
        let analysis = analyze_sql(sql);
        let sql117 = analysis
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "SQL117")
            .collect::<Vec<_>>();
        let sql119 = analysis
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "SQL119")
            .collect::<Vec<_>>();

        assert_eq!(
            sql117.len(),
            1,
            "unexpected diagnostics: {:?}",
            analysis.diagnostics
        );
        assert_eq!(
            sql119.len(),
            1,
            "unexpected diagnostics: {:?}",
            analysis.diagnostics
        );
        assert_eq!(sql.get(sql119[0].range.clone()), Some("*"));
        assert_eq!(sql.get(sql117[0].range.clone()), Some("LIMIT 100"));
        assert!(
            sql119[0].range.end <= sql117[0].range.start
                || sql117[0].range.end <= sql119[0].range.start
        );
    }

    #[test]
    fn select_star_range_stays_byte_exact_after_unicode() {
        let sql = "SELECT 'Ж', *\nFROM \"public\".\"car__model\"\nLIMIT 100;";
        let analysis = analyze_sql(sql);
        let sql119 = analysis
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "SQL119")
            .expect("SQL119");

        assert_eq!(sql.get(sql119.range.clone()), Some("*"));
        assert!(sql.is_char_boundary(sql119.range.start));
        assert!(sql.is_char_boundary(sql119.range.end));
    }

    #[test]
    fn sql_warning_order_and_deduplication_are_deterministic() {
        let sql = "SELECT *\nFROM \"public\".\"car__model\"\nLIMIT 100;";
        let first = analyze_sql(sql);
        let second = analyze_sql(sql);

        assert_eq!(first.diagnostics, second.diagnostics);
        for code in ["SQL117", "SQL119"] {
            assert_eq!(
                first
                    .diagnostics
                    .iter()
                    .filter(|diagnostic| diagnostic.code == code)
                    .count(),
                1
            );
        }
    }

    #[test]
    fn duplicate_alias_is_an_error_with_a_real_range() {
        let sql = "SELECT * FROM booking b JOIN car_wash b ON b.id = b.car_wash_id";
        let analysis = analyze_sql(sql);
        assert!(analysis.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "SQL201"
                && diagnostic.severity == SqlDiagnosticSeverity::Error
                && diagnostic.range.start < diagnostic.range.end
        }));
    }

    #[test]
    fn a4_b010_completion_clamps_mid_utf8_cursor_without_panicking() {
        let context = completion_context("SELECT Ж FROM items", "SELECT ".len() + 1);
        assert!(context.replace_range.start <= context.replace_range.end);
        assert!(context.replace_range.end <= "SELECT Ж FROM items".len());
    }
}
