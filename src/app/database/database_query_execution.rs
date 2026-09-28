#[derive(Clone, Debug, PartialEq, Eq)]
struct DatabaseExecutionSql {
    text: String,
    prefix_characters: usize,
}
#[derive(Debug)]
pub struct DatabasePreparedQueryTransaction {
    pub result_sets: Vec<DatabaseQueryResultSet>,
    pub messages: Vec<DatabaseQueryMessage>,
    pub effects: SqlExecutionEffects,
    pub mode: DatabaseQueryMode,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SqlExecutionEffects {
    pub returned_rows: u64,
    pub changed_rows: u64,
    pub has_definition: bool,
    pub has_other_effect: bool,
}

impl SqlExecutionEffects {
    pub fn requires_review(self) -> bool {
        self.changed_rows > 0 || self.has_definition || self.has_other_effect
    }

    fn record_command(&mut self, kind: crate::languages::sql::SqlStatementKind, rows: u64) {
        match kind {
            crate::languages::sql::SqlStatementKind::Query
            | crate::languages::sql::SqlStatementKind::Explain => {}
            crate::languages::sql::SqlStatementKind::Mutation => {
                self.changed_rows = self.changed_rows.saturating_add(rows);
            }
            crate::languages::sql::SqlStatementKind::Definition => {
                self.has_definition = true;
            }
            crate::languages::sql::SqlStatementKind::Other => {
                self.has_other_effect = true;
            }
        }
    }
}

#[derive(Debug)]
pub struct DatabaseQueryExecutionError {
    pub error: DatabaseBackendError,
    pub diagnostic: Option<DatabaseQueryDiagnostic>,
}

impl std::fmt::Display for DatabaseQueryExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}

impl std::error::Error for DatabaseQueryExecutionError {}

pub fn query_execution_target(
    text: &str,
    selection: Option<(usize, usize)>,
    cursor: usize,
) -> Option<(String, usize)> {
    if let Some((start, end)) = selection {
        let start = start.min(text.len());
        let end = end.min(text.len());
        if start < end
            && let Some(raw) = text.get(start..end)
        {
            let sql = raw.trim();
            if !sql.is_empty() {
                let leading = raw.len() - raw.trim_start().len();
                return Some((sql.to_string(), start.saturating_add(leading)));
            }
        }
    }
    if let Some(range) = statement_range_at(text, cursor) {
        let raw = text.get(range.clone())?;
        let sql = raw.trim();
        if !sql.is_empty() {
            let leading = raw.len() - raw.trim_start().len();
            return Some((sql.to_string(), range.start.saturating_add(leading)));
        }
    }
    let sql = text.trim();
    if sql.is_empty() {
        None
    } else {
        let leading = text.len() - text.trim_start().len();
        Some((sql.to_string(), leading))
    }
}

pub async fn begin_user_query_transaction(
    connection: &DatabaseConnectionConfig,
    secrets: &DatabaseSecretBundle,
    database_name: &str,
    sql: &str,
    source_offset: usize,
    mode: DatabaseQueryMode,
    settings: &DatabaseSettings,
    ssh_options: &SshConnectOptions,
) -> Result<(PostgresSession, DatabasePreparedQueryTransaction), DatabaseQueryExecutionError> {
    let statements = validate_managed_user_sql(sql).map_err(|error| {
        let message = error.to_string();
        let diagnostic = error.range.as_ref().map(|range| DatabaseQueryDiagnostic {
            start_byte: source_offset.saturating_add(range.start),
            end_byte: source_offset.saturating_add(range.end.max(range.start + 1)),
            message: message.clone(),
            ..DatabaseQueryDiagnostic::default()
        });
        DatabaseQueryExecutionError {
            error: DatabaseBackendError::InvalidConfiguration(message),
            diagnostic,
        }
    })?;
    let execution =
        explain_sql(sql, &statements, mode).map_err(|message| DatabaseQueryExecutionError {
            error: DatabaseBackendError::InvalidConfiguration(message),
            diagnostic: None,
        })?;
    let session = connect_postgres(connection, secrets, database_name, settings, ssh_options)
        .await
        .map_err(|error| DatabaseQueryExecutionError {
            error,
            diagnostic: None,
        })?;
    let statement_timeout_ms = settings.statement_timeout_seconds.saturating_mul(1_000);
    let lock_timeout_ms = settings.lock_timeout_seconds.saturating_mul(1_000);
    let idle_timeout_ms = settings
        .transaction_review_timeout_seconds
        .saturating_add(30)
        .saturating_mul(1_000);
    let begin = format!(
        "BEGIN; SET LOCAL statement_timeout = {statement_timeout_ms}; SET LOCAL lock_timeout = {lock_timeout_ms}; SET LOCAL idle_in_transaction_session_timeout = {idle_timeout_ms};"
    );
    session
        .client
        .batch_execute(&begin)
        .await
        .map_err(|error| DatabaseQueryExecutionError {
            diagnostic: diagnostic_from_error(&error, sql, source_offset),
            error: DatabaseBackendError::Postgres(error),
        })?;
    let result = execute_simple_query(&session, &execution.text, &statements, settings).await;
    match result {
        Ok((result_sets, mut effects)) => {
            mark_explain_analyze_side_effects(mode, &statements, &mut effects);
            let messages = session
                .drain_server_notices()
                .into_iter()
                .map(DatabaseQueryMessage::from)
                .collect();
            Ok((
                session,
                DatabasePreparedQueryTransaction {
                    result_sets,
                    messages,
                    effects,
                    mode,
                },
            ))
        }
        Err(error) => {
            let diagnostic = match &error {
                DatabaseBackendError::Postgres(postgres) => diagnostic_from_execution_error(
                    postgres,
                    sql,
                    source_offset,
                    execution.prefix_characters,
                ),
                _ => None,
            };
            let error = rollback_postgres_transaction_after_error(&session, error).await;
            Err(DatabaseQueryExecutionError { error, diagnostic })
        }
    }
}

fn mark_explain_analyze_side_effects(
    mode: DatabaseQueryMode,
    statements: &[SqlStatement],
    effects: &mut SqlExecutionEffects,
) {
    if mode == DatabaseQueryMode::ExplainAnalyze
        && statements.iter().any(|statement| {
            !matches!(
                statement.kind,
                crate::languages::sql::SqlStatementKind::Query
                    | crate::languages::sql::SqlStatementKind::Explain
            )
        })
    {
        effects.has_other_effect = true;
    }
}

async fn execute_simple_query(
    session: &PostgresSession,
    sql: &str,
    statements: &[SqlStatement],
    settings: &DatabaseSettings,
) -> Result<(Vec<DatabaseQueryResultSet>, SqlExecutionEffects), DatabaseBackendError> {
    let stream = session.client.simple_query_raw(sql).await?;
    tokio::pin!(stream);
    let mut result_sets = Vec::new();
    let mut current: Option<DatabaseQueryResultSet> = None;
    let mut total_rows = 0usize;
    let mut total_bytes = 0usize;
    let mut effects = SqlExecutionEffects::default();
    let mut statement_index = 0usize;
    while let Some(message) = stream.as_mut().try_next().await? {
        match message {
            SimpleQueryMessage::RowDescription(columns) => {
                if let Some(result) = current.take() {
                    push_result(&mut result_sets, result)?;
                }
                if columns.len() > MAX_COLUMNS_PER_RESULT {
                    return Err(DatabaseBackendError::LimitExceeded(
                        "result has more than 512 columns",
                    ));
                }
                current = Some(DatabaseQueryResultSet {
                    title: format!("Результат {}", result_sets.len() + 1),
                    columns: columns
                        .iter()
                        .map(|column| column.name().to_string())
                        .collect(),
                    ..DatabaseQueryResultSet::default()
                });
            }
            SimpleQueryMessage::Row(row) => {
                total_rows = total_rows.saturating_add(1);
                if total_rows > settings.result_row_limit.min(MAX_RESULT_ROWS) {
                    return Err(DatabaseBackendError::LimitExceeded(
                        "query result exceeds configured row limit",
                    ));
                }
                let result = current.get_or_insert_with(|| DatabaseQueryResultSet {
                    title: format!("Результат {}", result_sets.len() + 1),
                    columns: row
                        .columns()
                        .iter()
                        .map(|column| column.name().to_string())
                        .collect(),
                    ..DatabaseQueryResultSet::default()
                });
                let mut values = Vec::with_capacity(row.len());
                for index in 0..row.len() {
                    let value = row.try_get(index)?.map(str::to_string);
                    total_bytes = total_bytes.saturating_add(value.as_ref().map_or(8, String::len));
                    if total_bytes > settings.result_memory_limit_bytes.min(MAX_RESULT_BYTES) {
                        return Err(DatabaseBackendError::LimitExceeded(
                            "query result exceeds configured memory limit",
                        ));
                    }
                    values.push(DatabaseQueryCell { value });
                }
                result.rows.push(values);
            }
            SimpleQueryMessage::CommandComplete(affected) => {
                let statement = statements.get(statement_index);
                let kind = statement.map_or(
                    crate::languages::sql::SqlStatementKind::Other,
                    |statement| statement.kind,
                );
                let command_kind = command_kind(sql, statement);
                statement_index = statement_index.saturating_add(1);
                effects.record_command(kind, affected);
                if let Some(mut result) = current.take() {
                    result.returned_rows = result.rows.len() as u64;
                    effects.returned_rows =
                        effects.returned_rows.saturating_add(result.returned_rows);
                    result.affected_rows =
                        if kind == crate::languages::sql::SqlStatementKind::Mutation {
                            affected
                        } else {
                            0
                        };
                    result.command_kind = command_kind.clone();
                    push_result(&mut result_sets, result)?;
                } else {
                    let title = format!("Результат {}", result_sets.len() + 1);
                    push_result(
                        &mut result_sets,
                        DatabaseQueryResultSet {
                            title,
                            command_kind,
                            affected_rows: if kind
                                == crate::languages::sql::SqlStatementKind::Mutation
                            {
                                affected
                            } else {
                                0
                            },
                            ..DatabaseQueryResultSet::default()
                        },
                    )?;
                }
            }
            _ => {}
        }
    }
    if let Some(result) = current.take() {
        effects.returned_rows = effects
            .returned_rows
            .saturating_add(result.rows.len() as u64);
        push_result(&mut result_sets, result)?;
    }
    Ok((result_sets, effects))
}

fn command_kind(sql: &str, statement: Option<&SqlStatement>) -> String {
    let Some(statement) = statement else {
        return "COMMAND".to_string();
    };
    let keyword = sql
        .get(statement.range.clone())
        .and_then(|statement_sql| {
            statement_sql
                .split(|ch: char| !ch.is_ascii_alphabetic())
                .find(|part| !part.is_empty())
        })
        .unwrap_or_default()
        .to_ascii_uppercase();
    match statement.kind {
        crate::languages::sql::SqlStatementKind::Query => {
            if keyword == "WITH" {
                "SELECT".to_string()
            } else {
                keyword
            }
        }
        crate::languages::sql::SqlStatementKind::Mutation
        | crate::languages::sql::SqlStatementKind::Definition
        | crate::languages::sql::SqlStatementKind::Explain => keyword,
        crate::languages::sql::SqlStatementKind::Other => {
            if keyword.is_empty() {
                "COMMAND".to_string()
            } else {
                keyword
            }
        }
    }
}

fn push_result(
    results: &mut Vec<DatabaseQueryResultSet>,
    result: DatabaseQueryResultSet,
) -> Result<(), DatabaseBackendError> {
    if results.len() >= MAX_RESULT_SETS {
        return Err(DatabaseBackendError::LimitExceeded(
            "query script produced more than 32 result sets",
        ));
    }
    results.push(result);
    Ok(())
}

fn explain_sql(
    sql: &str,
    statements: &[SqlStatement],
    mode: DatabaseQueryMode,
) -> Result<DatabaseExecutionSql, String> {
    match mode {
        DatabaseQueryMode::Run => Ok(DatabaseExecutionSql {
            text: sql.to_string(),
            prefix_characters: 0,
        }),
        DatabaseQueryMode::Explain | DatabaseQueryMode::ExplainAnalyze => {
            if statements.len() != 1 {
                return Err("Explain supports exactly one SQL statement".to_string());
            }
            let body = sql.trim().trim_end_matches(';').trim_end();
            let options = if mode == DatabaseQueryMode::ExplainAnalyze {
                "ANALYZE, VERBOSE, BUFFERS, FORMAT TEXT"
            } else {
                "VERBOSE, FORMAT TEXT"
            };
            let prefix = format!("EXPLAIN ({options}) ");
            Ok(DatabaseExecutionSql {
                prefix_characters: prefix.chars().count(),
                text: format!("{prefix}{body}"),
            })
        }
    }
}

fn diagnostic_from_execution_error(
    error: &tokio_postgres::Error,
    original_sql: &str,
    source_offset: usize,
    prefix_characters: usize,
) -> Option<DatabaseQueryDiagnostic> {
    let db = error.as_db_error()?;
    let execution_character = match db.position()? {
        tokio_postgres::error::ErrorPosition::Original(position) => *position as usize,
        tokio_postgres::error::ErrorPosition::Internal { position, .. } => *position as usize,
    }
    .saturating_sub(1);
    let original_character = execution_character.saturating_sub(prefix_characters);
    let relative = postgres_character_to_byte(original_sql, original_character);
    let start = source_offset.saturating_add(relative);
    Some(DatabaseQueryDiagnostic {
        start_byte: start,
        end_byte: start.saturating_add(
            original_sql
                .get(relative..)
                .and_then(|tail| tail.chars().next())
                .map_or(1, char::len_utf8),
        ),
        message: db.message().to_string(),
        detail: db.detail().map(str::to_string),
        hint: db.hint().map(str::to_string),
        sqlstate: Some(db.code().code().to_string()),
    })
}

pub fn diagnostic_from_error(
    error: &tokio_postgres::Error,
    sql: &str,
    source_offset: usize,
) -> Option<DatabaseQueryDiagnostic> {
    let db = error.as_db_error()?;
    let character_position = match db.position()? {
        tokio_postgres::error::ErrorPosition::Original(position) => *position as usize,
        tokio_postgres::error::ErrorPosition::Internal { position, .. } => *position as usize,
    };
    let relative = postgres_character_to_byte(sql, character_position.saturating_sub(1));
    let start = source_offset.saturating_add(relative);
    Some(DatabaseQueryDiagnostic {
        start_byte: start,
        end_byte: start.saturating_add(
            sql.get(relative..)
                .and_then(|tail| tail.chars().next())
                .map_or(1, char::len_utf8),
        ),
        message: db.message().to_string(),
        detail: db.detail().map(str::to_string),
        hint: db.hint().map(str::to_string),
        sqlstate: Some(db.code().code().to_string()),
    })
}

pub fn postgres_character_to_byte(text: &str, character_offset: usize) -> usize {
    text.char_indices()
        .nth(character_offset)
        .map_or(text.len(), |(byte, _)| byte)
}
