#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseQueryCompletionResult {
    pub connection_id: DatabaseConnectionId,
    pub database_name: String,
    pub console_id: super::SqlConsoleId,
    pub metadata: DatabaseQueryCompletionMetadata,
}

pub fn format_database_sql(sql: &str) -> Result<String, String> {
    format_sql_conservative(sql)
}

pub fn analyze_database_query_sql(
    metadata: &DatabaseQueryCompletionMetadata,
    sql: &str,
) -> SqlAnalysis {
    let mut analysis = analyze_sql(sql);
    if !metadata.tables.is_empty() || !metadata.columns.is_empty() {
        analysis
            .diagnostics
            .extend(database_query_semantic_diagnostics(metadata, &analysis));
    }
    analysis.diagnostics.sort_by(|left, right| {
        left.range
            .start
            .cmp(&right.range.start)
            .then(left.range.end.cmp(&right.range.end))
            .then(left.code.cmp(right.code))
    });
    analysis.diagnostics.dedup_by(|left, right| {
        left.range == right.range && left.code == right.code && left.message == right.message
    });
    analysis
}

pub fn database_query_editor_diagnostics(
    analysis: &SqlAnalysis,
    backend: Option<&DatabaseQueryDiagnostic>,
    text: &str,
    line_offsets: &[usize],
) -> Vec<crate::lsp::Diagnostic> {
    let mut diagnostics = Vec::with_capacity(
        analysis
            .diagnostics
            .len()
            .saturating_add(usize::from(backend.is_some())),
    );
    for diagnostic in &analysis.diagnostics {
        diagnostics.push(editor_diagnostic(
            diagnostic.range.start,
            diagnostic.range.end,
            match diagnostic.severity {
                SqlDiagnosticSeverity::Error => crate::lsp::DiagSeverity::Error,
                SqlDiagnosticSeverity::Warning => crate::lsp::DiagSeverity::Warning,
            },
            Some(diagnostic.code),
            "RRiter SQL",
            diagnostic.message.clone(),
            text,
            line_offsets,
        ));
    }
    if let Some(diagnostic) = backend {
        let mut message = diagnostic.message.clone();
        if let Some(detail) = diagnostic.detail.as_deref() {
            message.push_str("\n\n");
            message.push_str(detail);
        }
        if let Some(hint) = diagnostic.hint.as_deref() {
            message.push_str("\n\nПодсказка: ");
            message.push_str(hint);
        }
        diagnostics.push(editor_diagnostic(
            diagnostic.start_byte,
            diagnostic.end_byte,
            crate::lsp::DiagSeverity::Error,
            diagnostic.sqlstate.as_deref(),
            "PostgreSQL",
            message,
            text,
            line_offsets,
        ));
    }
    diagnostics.sort_by(|left, right| {
        left.start_line
            .cmp(&right.start_line)
            .then(left.start_col.cmp(&right.start_col))
            .then_with(|| {
                diagnostic_severity_rank(left.severity)
                    .cmp(&diagnostic_severity_rank(right.severity))
            })
            .then_with(|| left.source.cmp(&right.source))
            .then_with(|| left.code.cmp(&right.code))
    });
    diagnostics
}

pub fn next_database_query_diagnostic_offset(
    diagnostics: &[crate::lsp::Diagnostic],
    text: &str,
    cursor: usize,
) -> Option<usize> {
    let first = diagnostics.first().map(|diagnostic| {
        crate::lsp::lsp_pos_to_offset(text, diagnostic.start_line, diagnostic.start_col)
    })?;
    diagnostics
        .iter()
        .map(|diagnostic| {
            crate::lsp::lsp_pos_to_offset(text, diagnostic.start_line, diagnostic.start_col)
        })
        .find(|&offset| offset > cursor)
        .or(Some(first))
}

fn diagnostic_severity_rank(severity: crate::lsp::DiagSeverity) -> u8 {
    match severity {
        crate::lsp::DiagSeverity::Error => 0,
        crate::lsp::DiagSeverity::Warning => 1,
        crate::lsp::DiagSeverity::Info => 2,
        crate::lsp::DiagSeverity::Hint => 3,
    }
}

#[allow(clippy::too_many_arguments)]
fn editor_diagnostic(
    start: usize,
    end: usize,
    severity: crate::lsp::DiagSeverity,
    code: Option<&str>,
    source: &str,
    message: String,
    text: &str,
    line_offsets: &[usize],
) -> crate::lsp::Diagnostic {
    let (start, end) = normalized_diagnostic_range(text, start, end);
    let (start_line, start_col) = crate::lsp::offset_to_lsp_pos(text, start, line_offsets);
    let (end_line, end_col) = crate::lsp::offset_to_lsp_pos(text, end, line_offsets);
    crate::lsp::Diagnostic {
        start_line,
        start_col,
        end_line,
        end_col,
        severity,
        code: code.map(std::sync::Arc::<str>::from),
        code_href: None,
        message: std::sync::Arc::<str>::from(message),
        source: Some(std::sync::Arc::<str>::from(source)),
        tags: crate::lsp::DiagTags::NONE,
        extra: None,
    }
}

fn normalized_diagnostic_range(text: &str, start: usize, end: usize) -> (usize, usize) {
    let mut start = start.min(text.len());
    while start > 0 && !text.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = end.min(text.len()).max(start);
    while end < text.len() && !text.is_char_boundary(end) {
        end += 1;
    }
    (start, end.max(start))
}

pub fn database_query_completion_context(sql: &str, cursor: usize) -> SqlCompletionContext {
    completion_context(sql, cursor)
}

pub fn completion_recovery_analysis(
    metadata: &DatabaseQueryCompletionMetadata,
    sql: &str,
    cursor: usize,
    context: &SqlCompletionContext,
    analysis: &SqlAnalysis,
) -> Option<SqlAnalysis> {
    if context.kind != SqlCompletionKind::QualifiedColumn {
        return None;
    }
    let qualifier = context.qualifier.as_deref()?;
    if relation_for_qualifier(analysis, cursor, qualifier).is_some() {
        return None;
    }
    let range = context.replace_range.clone();
    if range.start > range.end || range.end > sql.len() {
        return None;
    }
    let mut repaired = String::with_capacity(sql.len() + 24);
    repaired.push_str(sql.get(..range.start)?);
    repaired.push_str("__rriter_completion");
    repaired.push_str(sql.get(range.end..)?);
    Some(analyze_database_query_sql(metadata, &repaired))
}

pub fn completion_words_for_context(
    metadata: &DatabaseQueryCompletionMetadata,
    analysis: &SqlAnalysis,
    context: &SqlCompletionContext,
    cursor: usize,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let visible_relations = analysis
        .relations
        .iter()
        .filter(|relation| relation.scope.start <= cursor && cursor <= relation.scope.end)
        .collect::<Vec<_>>();

    match context.kind {
        SqlCompletionKind::None => {}
        SqlCompletionKind::Table => {
            for table in &metadata.tables {
                out.push((quote_completion_identifier(table), "table".to_string()));
            }
            for (scope, cte) in &analysis.ctes {
                if scope.start <= cursor && cursor <= scope.end {
                    out.push((quote_completion_identifier(cte), "CTE".to_string()));
                }
            }
        }
        SqlCompletionKind::QualifiedColumn => {
            if let Some(qualifier) = context.qualifier.as_deref()
                && let Some(relation) = relation_for_qualifier(analysis, cursor, qualifier)
            {
                for column in metadata
                    .columns
                    .iter()
                    .filter(|column| column.table_name.eq_ignore_ascii_case(&relation.table_name))
                {
                    out.push((
                        quote_completion_identifier(&column.column_name),
                        format!("{} · {}", relation.alias, column.data_type),
                    ));
                }
                out.push(("*".to_string(), format!("{} · все столбцы", relation.alias)));
            }
        }
        SqlCompletionKind::Column => {
            let visible_tables = visible_relations
                .iter()
                .map(|relation| relation.table_name.to_ascii_lowercase())
                .collect::<std::collections::BTreeSet<_>>();
            for column in &metadata.columns {
                if visible_tables.is_empty()
                    || visible_tables.contains(&column.table_name.to_ascii_lowercase())
                {
                    out.push((
                        quote_completion_identifier(&column.column_name),
                        format!("{} · {}", column.table_name, column.data_type),
                    ));
                }
            }
            for alias in output_aliases_at(analysis, cursor) {
                out.push((
                    quote_completion_identifier(alias),
                    "alias SELECT".to_string(),
                ));
            }
            for function in &metadata.functions {
                out.push((function.clone(), "function".to_string()));
            }
            if !context.prefix.is_empty() {
                for keyword in crate::languages::sql::SQL_KEYWORDS {
                    out.push(((*keyword).to_string(), "SQL".to_string()));
                }
            }
        }
        SqlCompletionKind::Operator => {
            for operator in &metadata.operators {
                out.push((operator.clone(), "operator".to_string()));
            }
            for operator in [
                "=",
                "<>",
                "!=",
                "<",
                ">",
                "<=",
                ">=",
                "LIKE",
                "ILIKE",
                "IN",
                "BETWEEN",
                "IS NULL",
                "IS NOT NULL",
            ] {
                out.push((operator.to_string(), "operator".to_string()));
            }
        }
        SqlCompletionKind::Value => {
            for value in &metadata.enum_values {
                out.push((
                    format!("'{}'", value.replace('\'', "''")),
                    "enum".to_string(),
                ));
            }
            for value in ["TRUE", "FALSE", "NULL", "CURRENT_DATE", "CURRENT_TIMESTAMP"] {
                out.push((value.to_string(), "value".to_string()));
            }
        }
        SqlCompletionKind::Direction => {
            for value in ["ASC", "DESC"] {
                out.push((value.to_string(), "ORDER BY".to_string()));
            }
        }
        SqlCompletionKind::NullOrdering => {
            for value in ["NULLS FIRST", "NULLS LAST"] {
                out.push((value.to_string(), "ORDER BY".to_string()));
            }
        }
        SqlCompletionKind::Keyword => {
            for keyword in crate::languages::sql::SQL_KEYWORDS {
                out.push(((*keyword).to_string(), "SQL".to_string()));
            }
        }
    }

    let prefix = context.prefix.to_ascii_lowercase();
    if !prefix.is_empty() {
        out.retain(|(word, detail)| {
            let candidate = word
                .trim_matches('"')
                .trim_matches('\'')
                .to_ascii_lowercase();
            if detail == "alias SELECT" && candidate == prefix {
                return false;
            }
            crate::app::autocomplete_match_candidate(&prefix, &candidate).is_some()
        });
    }
    out.sort_unstable_by(|left, right| {
        completion_rank(&left.0, &left.1, &prefix)
            .cmp(&completion_rank(&right.0, &right.1, &prefix))
            .then_with(|| {
                left.0
                    .to_ascii_lowercase()
                    .cmp(&right.0.to_ascii_lowercase())
            })
            .then_with(|| left.1.cmp(&right.1))
    });
    out.dedup_by(|left, right| left.0 == right.0 && left.1 == right.1);
    out
}

fn completion_rank(candidate: &str, detail: &str, prefix: &str) -> (u8, usize) {
    if prefix.is_empty() {
        return (0, candidate.len());
    }
    let candidate = candidate
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase();
    let match_kind = crate::app::autocomplete_match_candidate(prefix, &candidate)
        .map(|(kind, _)| kind)
        .unwrap_or(crate::app::AutocompleteMatchKind::Fuzzy);
    let keyword = matches!(detail, "SQL" | "operator" | "ORDER BY");
    let priority = match (match_kind, keyword) {
        (crate::app::AutocompleteMatchKind::Exact, _) => 0,
        (crate::app::AutocompleteMatchKind::Prefix, true) => 1,
        (crate::app::AutocompleteMatchKind::Prefix, false) => 2,
        (crate::app::AutocompleteMatchKind::Fuzzy, true) => 3,
        (crate::app::AutocompleteMatchKind::Fuzzy, false) => 4,
    };
    (priority, candidate.len())
}

fn quote_completion_identifier(identifier: &str) -> String {
    let mut chars = identifier.chars();
    let simple = chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_lowercase())
        && chars.all(|ch| ch == '_' || ch.is_ascii_lowercase() || ch.is_ascii_digit());
    if simple
        && !crate::languages::sql::SQL_KEYWORDS
            .iter()
            .any(|keyword| keyword.eq_ignore_ascii_case(identifier))
    {
        identifier.to_string()
    } else {
        format!("\"{}\"", identifier.replace('"', "\"\""))
    }
}

fn database_query_semantic_diagnostics(
    metadata: &DatabaseQueryCompletionMetadata,
    analysis: &SqlAnalysis,
) -> Vec<SqlAnalysisDiagnostic> {
    let known_tables = metadata
        .tables
        .iter()
        .map(|table| table.to_ascii_lowercase())
        .collect::<std::collections::BTreeSet<_>>();
    let mut diagnostics = Vec::new();
    for relation in &analysis.relations {
        if relation.is_cte
            || relation
                .schema
                .as_deref()
                .is_some_and(|schema| !schema.eq_ignore_ascii_case("public"))
        {
            continue;
        }
        if !known_tables.contains(&relation.table_name.to_ascii_lowercase()) {
            diagnostics.push(SqlAnalysisDiagnostic {
                range: relation.source_range.clone(),
                severity: SqlDiagnosticSeverity::Error,
                code: "SQL202",
                message: format!(
                    "Таблица «{}» не найдена в public schema",
                    relation.table_name
                ),
            });
        }
    }
    for reference in &analysis.qualified_references {
        let Some(relation) =
            relation_for_qualifier(analysis, reference.range.start, &reference.qualifier)
        else {
            diagnostics.push(SqlAnalysisDiagnostic {
                range: reference.range.clone(),
                severity: SqlDiagnosticSeverity::Error,
                code: "SQL203",
                message: format!("Неизвестный псевдоним таблицы «{}»", reference.qualifier),
            });
            continue;
        };
        if relation.is_cte || reference.name == "*" {
            continue;
        }
        if !metadata.columns.iter().any(|column| {
            column.table_name.eq_ignore_ascii_case(&relation.table_name)
                && column.column_name.eq_ignore_ascii_case(&reference.name)
        }) {
            diagnostics.push(SqlAnalysisDiagnostic {
                range: reference.range.clone(),
                severity: SqlDiagnosticSeverity::Error,
                code: "SQL204",
                message: format!(
                    "Столбец «{}.{}» не найден в таблице «{}»",
                    reference.qualifier, reference.name, relation.table_name
                ),
            });
        }
    }
    for reference in &analysis.unqualified_references {
        if output_aliases_at(analysis, reference.range.start)
            .any(|alias| alias.eq_ignore_ascii_case(&reference.name))
        {
            continue;
        }
        let visible_relations = analysis
            .relations
            .iter()
            .filter(|relation| {
                relation.scope == reference.scope
                    && !relation.is_cte
                    && relation
                        .schema
                        .as_deref()
                        .is_none_or(|schema| schema.eq_ignore_ascii_case("public"))
            })
            .collect::<Vec<_>>();
        if visible_relations.is_empty() {
            continue;
        }
        let matching_tables = visible_relations
            .iter()
            .filter(|relation| {
                metadata.columns.iter().any(|column| {
                    column.table_name.eq_ignore_ascii_case(&relation.table_name)
                        && column.column_name.eq_ignore_ascii_case(&reference.name)
                })
            })
            .map(|relation| relation.table_name.to_ascii_lowercase())
            .collect::<std::collections::BTreeSet<_>>();
        if matching_tables.is_empty() {
            diagnostics.push(SqlAnalysisDiagnostic {
                range: reference.range.clone(),
                severity: SqlDiagnosticSeverity::Error,
                code: "SQL205",
                message: format!(
                    "Столбец «{}» не найден в таблицах текущего SQL-блока",
                    reference.name
                ),
            });
        } else if matching_tables.len() > 1 {
            diagnostics.push(SqlAnalysisDiagnostic {
                range: reference.range.clone(),
                severity: SqlDiagnosticSeverity::Error,
                code: "SQL206",
                message: format!(
                    "Столбец «{}» неоднозначен; укажите псевдоним таблицы",
                    reference.name
                ),
            });
        }
    }
    diagnostics
}

pub fn analysis_error_ranges(analysis: &SqlAnalysis) -> Vec<(usize, usize)> {
    analysis
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == SqlDiagnosticSeverity::Error)
        .map(|diagnostic| (diagnostic.range.start, diagnostic.range.end))
        .collect()
}

pub async fn load_query_completion_metadata(
    connection: &DatabaseConnectionConfig,
    secrets: &DatabaseSecretBundle,
    database_name: &str,
    console_id: super::SqlConsoleId,
    settings: &DatabaseSettings,
    ssh_options: &SshConnectOptions,
) -> Result<DatabaseQueryCompletionResult, DatabaseBackendError> {
    let session =
        connect_postgres(connection, secrets, database_name, settings, ssh_options).await?;
    let timeout = Duration::from_secs(settings.statement_timeout_seconds);
    let rows = tokio::time::timeout(
        timeout,
        session.client.query(QUERY_COMPLETION_COLUMNS_SQL, &[]),
    )
    .await
    .map_err(|_| DatabaseBackendError::Timeout("PostgreSQL completion metadata"))??;
    if rows.len() > super::MAX_PUBLIC_TABLES_PER_DATABASE.saturating_mul(MAX_COLUMNS_PER_RESULT) {
        return Err(DatabaseBackendError::LimitExceeded(
            "completion metadata exceeds supported size",
        ));
    }
    let mut metadata = DatabaseQueryCompletionMetadata::default();
    for row in rows {
        let table_name: String = row.get(0);
        if metadata.tables.last() != Some(&table_name) {
            metadata.tables.push(table_name.clone());
        }
        metadata.columns.push(DatabaseQueryCompletionColumn {
            table_name,
            column_name: row.get(1),
            data_type: row.get(2),
        });
    }
    for row in tokio::time::timeout(
        timeout,
        session.client.query(QUERY_COMPLETION_ENUMS_SQL, &[]),
    )
    .await
    .map_err(|_| DatabaseBackendError::Timeout("PostgreSQL enum metadata"))??
    {
        metadata.enum_values.push(row.get(0));
    }
    for row in tokio::time::timeout(
        timeout,
        session.client.query(QUERY_COMPLETION_FUNCTIONS_SQL, &[]),
    )
    .await
    .map_err(|_| DatabaseBackendError::Timeout("PostgreSQL function metadata"))??
    {
        metadata.functions.push(row.get(0));
    }
    for row in tokio::time::timeout(
        timeout,
        session.client.query(QUERY_COMPLETION_OPERATORS_SQL, &[]),
    )
    .await
    .map_err(|_| DatabaseBackendError::Timeout("PostgreSQL operator metadata"))??
    {
        metadata.operators.push(row.get(0));
    }
    Ok(DatabaseQueryCompletionResult {
        connection_id: connection.id,
        database_name: database_name.to_string(),
        console_id,
        metadata,
    })
}
