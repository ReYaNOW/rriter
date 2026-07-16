use super::database_postgres::{
    DatabaseBackendError, DatabaseServerNotice, PostgresSession, connect_postgres,
};
use super::database_ssh::SshConnectOptions;
use super::{
    DatabaseConnectionConfig, DatabaseConnectionId, DatabaseSecretBundle, DatabaseSettings,
    DatabaseTransactionId, MAX_COLUMNS_PER_RESULT, MAX_RESULT_BYTES, MAX_RESULT_ROWS,
    MAX_RESULT_SETS,
};
use crate::languages::sql::{
    SqlStatement, format_sql_conservative, statement_range_at, validate_managed_user_sql,
};
use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio_postgres::SimpleQueryMessage;

const QUERY_COMPLETION_COLUMNS_SQL: &str = "SELECT c.relname, a.attname, pg_catalog.format_type(a.atttypid, a.atttypmod)\n\
FROM pg_class c\n\
JOIN pg_namespace n ON n.oid = c.relnamespace\n\
JOIN pg_attribute a ON a.attrelid = c.oid\n\
WHERE n.nspname = 'public' AND c.relkind IN ('r','p') AND a.attnum > 0 AND NOT a.attisdropped\n\
ORDER BY c.relname, a.attnum";
const QUERY_COMPLETION_ENUMS_SQL: &str = "SELECT DISTINCT e.enumlabel\n\
FROM pg_type t JOIN pg_enum e ON e.enumtypid = t.oid\n\
JOIN pg_namespace n ON n.oid = t.typnamespace\n\
WHERE n.nspname = 'public' ORDER BY e.enumlabel LIMIT 4096";
const QUERY_COMPLETION_FUNCTIONS_SQL: &str = "SELECT DISTINCT p.proname\n\
FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace\n\
WHERE n.nspname IN ('pg_catalog','public') ORDER BY p.proname LIMIT 4096";
const QUERY_COMPLETION_OPERATORS_SQL: &str =
    "SELECT DISTINCT oprname FROM pg_operator ORDER BY oprname LIMIT 1024";

#[derive(Clone, Debug, PartialEq, Eq)]
struct DatabaseExecutionSql {
    text: String,
    prefix_characters: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DatabaseQueryMode {
    #[default]
    Run,
    Explain,
    ExplainAnalyze,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DatabaseQueryCell {
    pub value: Option<String>,
}

impl DatabaseQueryCell {
    pub fn display_text(&self) -> &str {
        self.value.as_deref().unwrap_or("<NULL>")
    }

    pub fn estimated_bytes(&self) -> usize {
        self.value.as_ref().map_or(8, String::len)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DatabaseQueryResultSet {
    pub title: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<DatabaseQueryCell>>,
    pub command_tag: String,
    pub affected_rows: u64,
    pub truncated: bool,
}

impl DatabaseQueryResultSet {
    pub fn estimated_bytes(&self) -> usize {
        self.title
            .len()
            .saturating_add(self.command_tag.len())
            .saturating_add(self.columns.iter().map(String::len).sum::<usize>())
            .saturating_add(
                self.rows
                    .iter()
                    .flat_map(|row| row.iter())
                    .map(DatabaseQueryCell::estimated_bytes)
                    .sum::<usize>(),
            )
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DatabaseQueryMessage {
    pub severity: String,
    pub message: String,
    pub detail: Option<String>,
    pub hint: Option<String>,
}

impl From<DatabaseServerNotice> for DatabaseQueryMessage {
    fn from(value: DatabaseServerNotice) -> Self {
        Self {
            severity: value.severity,
            message: value.message,
            detail: value.detail,
            hint: value.hint,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DatabaseQueryDiagnostic {
    pub start_byte: usize,
    pub end_byte: usize,
    pub message: String,
    pub detail: Option<String>,
    pub hint: Option<String>,
    pub sqlstate: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseQueryHistoryEntry {
    pub connection_id: DatabaseConnectionId,
    pub database_name: String,
    pub console_id: super::SqlConsoleId,
    pub sql: String,
    pub started_unix_ms: u128,
    pub duration_ms: u64,
    pub succeeded: bool,
    pub affected_rows: u64,
    pub error_summary: Option<String>,
}

impl Default for DatabaseQueryHistoryEntry {
    fn default() -> Self {
        Self {
            connection_id: DatabaseConnectionId(0),
            database_name: String::new(),
            console_id: super::SqlConsoleId(0),
            sql: String::new(),
            started_unix_ms: 0,
            duration_ms: 0,
            succeeded: false,
            affected_rows: 0,
            error_summary: None,
        }
    }
}

impl DatabaseQueryHistoryEntry {
    pub fn normalize(&mut self) {
        self.database_name.truncate(128);
        self.sql.truncate(super::MAX_SQL_CONSOLE_BYTES);
        if let Some(error) = &mut self.error_summary {
            error.truncate(4_096);
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DatabaseQueryCompletionColumn {
    pub table_name: String,
    pub column_name: String,
    pub data_type: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DatabaseQueryCompletionMetadata {
    pub tables: Vec<String>,
    pub columns: Vec<DatabaseQueryCompletionColumn>,
    pub enum_values: Vec<String>,
    pub functions: Vec<String>,
    pub operators: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DatabaseQueryResultViewState {
    pub active_result: usize,
    pub scroll_x: i32,
    pub scroll_y: i32,
    pub selected_row: Option<usize>,
    pub selected_column: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseQueryReviewState {
    pub transaction_id: DatabaseTransactionId,
    pub sql: String,
    pub source_offset: usize,
    pub started_unix_ms: u128,
    pub result_sets: Vec<DatabaseQueryResultSet>,
    pub messages: Vec<DatabaseQueryMessage>,
    pub deadline_unix_ms: u128,
    pub duration_ms: u64,
    pub affected_rows: u64,
    pub mode: DatabaseQueryMode,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DatabaseQueryTabState {
    pub running: bool,
    pub running_sql: Option<String>,
    pub running_started_unix_ms: u128,
    pub error: Option<String>,
    pub diagnostic: Option<DatabaseQueryDiagnostic>,
    pub diagnostic_editor_version: Option<u64>,
    pub results: Vec<DatabaseQueryResultSet>,
    pub messages: Vec<DatabaseQueryMessage>,
    pub result_view: DatabaseQueryResultViewState,
    pub review: Option<DatabaseQueryReviewState>,
    pub completion: DatabaseQueryCompletionMetadata,
    pub completion_loaded: bool,
    pub history_open: bool,
    pub history_selected: usize,
    pub last_duration_ms: u64,
    pub last_affected_rows: u64,
}

impl DatabaseQueryTabState {
    pub fn mark_running(&mut self, sql: String, started_unix_ms: u128) {
        self.running = true;
        self.running_sql = Some(sql);
        self.running_started_unix_ms = started_unix_ms;
        self.error = None;
        self.diagnostic = None;
        self.diagnostic_editor_version = None;
    }

    pub fn take_cancelled_history(
        &mut self,
        connection_id: DatabaseConnectionId,
        database_name: &str,
        console_id: super::SqlConsoleId,
    ) -> Option<DatabaseQueryHistoryEntry> {
        let sql = self.running_sql.take()?;
        let entry = DatabaseQueryHistoryEntry {
            connection_id,
            database_name: database_name.to_string(),
            console_id,
            sql,
            started_unix_ms: self.running_started_unix_ms,
            duration_ms: history_started_now()
                .saturating_sub(self.running_started_unix_ms)
                .min(u64::MAX as u128) as u64,
            succeeded: false,
            affected_rows: 0,
            error_summary: Some("Запрос отменён пользователем".to_string()),
        };
        self.running = false;
        self.running_started_unix_ms = 0;
        Some(entry)
    }
}

#[derive(Debug)]
pub struct DatabasePreparedQueryTransaction {
    pub result_sets: Vec<DatabaseQueryResultSet>,
    pub messages: Vec<DatabaseQueryMessage>,
    pub affected_rows: u64,
    pub mode: DatabaseQueryMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseQueryCompletionResult {
    pub connection_id: DatabaseConnectionId,
    pub database_name: String,
    pub console_id: super::SqlConsoleId,
    pub metadata: DatabaseQueryCompletionMetadata,
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
        if start < end {
            let sql = text.get(start..end)?.trim();
            if !sql.is_empty() {
                let leading = text.get(start..end)?.len() - text.get(start..end)?.trim_start().len();
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

pub fn format_database_sql(sql: &str) -> Result<String, String> {
    format_sql_conservative(sql)
}

pub fn completion_words(
    metadata: &DatabaseQueryCompletionMetadata,
    sql: &str,
    cursor: usize,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for table in &metadata.tables {
        out.push((table.clone(), "table".to_string()));
    }
    let aliases = sql_aliases(sql);
    let prefix = dotted_prefix(sql, cursor);
    for column in &metadata.columns {
        let include = match prefix.as_deref() {
            Some(owner) => owner == column.table_name || aliases.get(owner) == Some(&column.table_name),
            None => true,
        };
        if include {
            out.push((column.column_name.clone(), format!("{} · {}", column.table_name, column.data_type)));
        }
    }
    for value in &metadata.enum_values {
        out.push((value.clone(), "enum".to_string()));
    }
    for function in &metadata.functions {
        out.push((function.clone(), "function".to_string()));
    }
    for operator in &metadata.operators {
        out.push((operator.clone(), "operator".to_string()));
    }
    for keyword in ["RETURNING", "ON CONFLICT", "DO UPDATE", "DO NOTHING", "ARRAY", "ANY", "ALL"] {
        out.push((keyword.to_string(), "PostgreSQL".to_string()));
    }
    out.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    out.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    out
}

fn sql_aliases(sql: &str) -> std::collections::BTreeMap<String, String> {
    let words = sql
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'))
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    let mut aliases = std::collections::BTreeMap::new();
    let mut idx = 0usize;
    while idx + 2 < words.len() {
        if words[idx].eq_ignore_ascii_case("from") || words[idx].eq_ignore_ascii_case("join") {
            let table = words[idx + 1].rsplit('.').next().unwrap_or(words[idx + 1]);
            let mut alias_idx = idx + 2;
            if words[alias_idx].eq_ignore_ascii_case("as") && alias_idx + 1 < words.len() {
                alias_idx += 1;
            }
            let alias = words[alias_idx];
            if !is_sql_clause(alias) {
                aliases.insert(alias.to_string(), table.to_string());
            }
        }
        idx += 1;
    }
    aliases
}

fn is_sql_clause(word: &str) -> bool {
    matches!(word.to_ascii_uppercase().as_str(), "WHERE" | "JOIN" | "LEFT" | "RIGHT" | "FULL" | "INNER" | "ORDER" | "GROUP" | "LIMIT" | "OFFSET" | "RETURNING" | "ON")
}

fn dotted_prefix(sql: &str, cursor: usize) -> Option<String> {
    let bytes = sql.as_bytes();
    let mut end = cursor.min(bytes.len());
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    if end == 0 || bytes[end - 1] != b'.' {
        return None;
    }
    let mut start = end - 1;
    while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
        start -= 1;
    }
    sql.get(start..end - 1).filter(|value| !value.is_empty()).map(str::to_string)
}

pub async fn load_query_completion_metadata(
    connection: &DatabaseConnectionConfig,
    secrets: &DatabaseSecretBundle,
    database_name: &str,
    console_id: super::SqlConsoleId,
    settings: &DatabaseSettings,
    ssh_options: &SshConnectOptions,
) -> Result<DatabaseQueryCompletionResult, DatabaseBackendError> {
    let session = connect_postgres(connection, secrets, database_name, settings, ssh_options).await?;
    let timeout = Duration::from_secs(settings.statement_timeout_seconds);
    let rows = tokio::time::timeout(
        timeout,
        session.client.query(QUERY_COMPLETION_COLUMNS_SQL, &[]),
    )
        .await
        .map_err(|_| DatabaseBackendError::Timeout("PostgreSQL completion metadata"))??;
    if rows.len() > super::MAX_PUBLIC_TABLES_PER_DATABASE.saturating_mul(MAX_COLUMNS_PER_RESULT) {
        return Err(DatabaseBackendError::LimitExceeded("completion metadata exceeds supported size"));
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
    let execution = explain_sql(sql, &statements, mode).map_err(|message| DatabaseQueryExecutionError {
        error: DatabaseBackendError::InvalidConfiguration(message),
        diagnostic: None,
    })?;
    let session = connect_postgres(connection, secrets, database_name, settings, ssh_options)
        .await
        .map_err(|error| DatabaseQueryExecutionError { error, diagnostic: None })?;
    let statement_timeout_ms = settings.statement_timeout_seconds.saturating_mul(1_000);
    let lock_timeout_ms = settings.lock_timeout_seconds.saturating_mul(1_000);
    let idle_timeout_ms = settings
        .transaction_review_timeout_seconds
        .saturating_add(30)
        .saturating_mul(1_000);
    let begin = format!(
        "BEGIN; SET LOCAL statement_timeout = {statement_timeout_ms}; SET LOCAL lock_timeout = {lock_timeout_ms}; SET LOCAL idle_in_transaction_session_timeout = {idle_timeout_ms};"
    );
    session.client.batch_execute(&begin).await.map_err(|error| DatabaseQueryExecutionError {
        diagnostic: diagnostic_from_error(&error, sql, source_offset),
        error: DatabaseBackendError::Postgres(error),
    })?;
    let result = execute_simple_query(&session, &execution.text, settings).await;
    match result {
        Ok((result_sets, affected_rows)) => {
            let messages = session
                .drain_server_notices()
                .into_iter()
                .map(DatabaseQueryMessage::from)
                .collect();
            Ok((session, DatabasePreparedQueryTransaction {
                result_sets,
                messages,
                affected_rows,
                mode,
            }))
        }
        Err(error) => {
            let _ = session.client.batch_execute("ROLLBACK").await;
            let diagnostic = match &error {
                DatabaseBackendError::Postgres(postgres) => {
                    diagnostic_from_execution_error(
                        postgres,
                        sql,
                        source_offset,
                        execution.prefix_characters,
                    )
                }
                _ => None,
            };
            Err(DatabaseQueryExecutionError { error, diagnostic })
        }
    }
}

pub async fn finish_user_query_transaction(
    session: &PostgresSession,
    commit: bool,
) -> Result<(), DatabaseBackendError> {
    session
        .client
        .batch_execute(if commit { "COMMIT" } else { "ROLLBACK" })
        .await?;
    Ok(())
}

async fn execute_simple_query(
    session: &PostgresSession,
    sql: &str,
    settings: &DatabaseSettings,
) -> Result<(Vec<DatabaseQueryResultSet>, u64), DatabaseBackendError> {
    let stream = session.client.simple_query_raw(sql).await?;
    tokio::pin!(stream);
    let mut result_sets = Vec::new();
    let mut current: Option<DatabaseQueryResultSet> = None;
    let mut total_rows = 0usize;
    let mut total_bytes = 0usize;
    let mut total_affected = 0u64;
    while let Some(message) = stream.as_mut().try_next().await? {
        match message {
            SimpleQueryMessage::RowDescription(columns) => {
                if let Some(result) = current.take() {
                    push_result(&mut result_sets, result)?;
                }
                if columns.len() > MAX_COLUMNS_PER_RESULT {
                    return Err(DatabaseBackendError::LimitExceeded("result has more than 512 columns"));
                }
                current = Some(DatabaseQueryResultSet {
                    title: format!("Result {}", result_sets.len() + 1),
                    columns: columns.iter().map(|column| column.name().to_string()).collect(),
                    ..DatabaseQueryResultSet::default()
                });
            }
            SimpleQueryMessage::Row(row) => {
                total_rows = total_rows.saturating_add(1);
                if total_rows > settings.result_row_limit.min(MAX_RESULT_ROWS) {
                    return Err(DatabaseBackendError::LimitExceeded("query result exceeds configured row limit"));
                }
                let result = current.get_or_insert_with(|| DatabaseQueryResultSet {
                    title: format!("Result {}", result_sets.len() + 1),
                    columns: row.columns().iter().map(|column| column.name().to_string()).collect(),
                    ..DatabaseQueryResultSet::default()
                });
                let mut values = Vec::with_capacity(row.len());
                for index in 0..row.len() {
                    let value = row.try_get(index)?.map(str::to_string);
                    total_bytes = total_bytes.saturating_add(value.as_ref().map_or(8, String::len));
                    if total_bytes > settings.result_memory_limit_bytes.min(MAX_RESULT_BYTES) {
                        return Err(DatabaseBackendError::LimitExceeded("query result exceeds configured memory limit"));
                    }
                    values.push(DatabaseQueryCell { value });
                }
                result.rows.push(values);
            }
            SimpleQueryMessage::CommandComplete(affected) => {
                total_affected = total_affected.saturating_add(affected);
                if let Some(mut result) = current.take() {
                    result.affected_rows = affected;
                    result.command_tag = format!("{} rows", affected);
                    push_result(&mut result_sets, result)?;
                } else {
                    let title = format!("Result {}", result_sets.len() + 1);
                    push_result(&mut result_sets, DatabaseQueryResultSet {
                        title,
                        command_tag: format!("Command complete · {} rows", affected),
                        affected_rows: affected,
                        ..DatabaseQueryResultSet::default()
                    })?;
                }
            }
            _ => {}
        }
    }
    if let Some(result) = current.take() {
        push_result(&mut result_sets, result)?;
    }
    Ok((result_sets, total_affected))
}

fn push_result(
    results: &mut Vec<DatabaseQueryResultSet>,
    result: DatabaseQueryResultSet,
) -> Result<(), DatabaseBackendError> {
    if results.len() >= MAX_RESULT_SETS {
        return Err(DatabaseBackendError::LimitExceeded("query script produced more than 32 result sets"));
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
        end_byte: start.saturating_add(sql.get(relative..).and_then(|tail| tail.chars().next()).map_or(1, char::len_utf8)),
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


pub fn sanitize_history_sql(sql: &str) -> String {
    let mut output = sql.to_string();
    for scheme in ["postgres://", "postgresql://", "jdbc:postgresql://"] {
        let mut search = 0usize;
        while let Some(relative) = output[search..].to_ascii_lowercase().find(scheme) {
            let start = search + relative + scheme.len();
            let authority_end = output[start..]
                .find(|ch: char| matches!(ch, '/' | '?' | '#') || ch.is_whitespace())
                .map_or(output.len(), |offset| start + offset);
            if let Some(at_offset) = output[start..authority_end].rfind('@') {
                let at = start + at_offset;
                if let Some(colon_offset) = output[start..at].find(':') {
                    let secret_start = start + colon_offset + 1;
                    output.replace_range(secret_start..at, "<redacted>");
                    search = secret_start + "<redacted>".len();
                    continue;
                }
            }
            search = authority_end;
        }
    }

    let mut cursor = 0usize;
    loop {
        let lower = output[cursor..].to_ascii_lowercase();
        let Some(relative) = lower.find("password") else { break; };
        let token_start = cursor + relative;
        let after = token_start + "password".len();
        let boundary_before = token_start == 0
            || !output.as_bytes()[token_start - 1].is_ascii_alphanumeric();
        let boundary_after = after >= output.len()
            || !output.as_bytes()[after].is_ascii_alphanumeric();
        if !boundary_before || !boundary_after {
            cursor = after;
            continue;
        }
        let Some(quote_relative) = output[after..].find('\'') else {
            cursor = after;
            continue;
        };
        let quote_start = after + quote_relative;
        let mut index = quote_start + 1;
        while index < output.len() {
            if output.as_bytes()[index] == b'\'' {
                if output.as_bytes().get(index + 1) == Some(&b'\'') {
                    index += 2;
                    continue;
                }
                output.replace_range(quote_start + 1..index, "<redacted>");
                cursor = quote_start + 1 + "<redacted>".len() + 1;
                break;
            }
            index += 1;
        }
        if index >= output.len() {
            break;
        }
    }
    let mut query_cursor = 0usize;
    loop {
        let lower = output[query_cursor..].to_ascii_lowercase();
        let Some(relative) = lower.find("password=") else { break; };
        let value_start = query_cursor + relative + "password=".len();
        let value_end = output[value_start..]
            .find(|ch: char| matches!(ch, '&' | '#' | '\'' | '"') || ch.is_whitespace())
            .map_or(output.len(), |offset| value_start + offset);
        output.replace_range(value_start..value_end, "<redacted>");
        query_cursor = value_start + "<redacted>".len();
    }

    output.truncate(super::MAX_SQL_CONSOLE_BYTES);
    output
}

pub fn history_started_now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_target_prefers_selection_then_statement() {
        let text = "select 1;\nselect 2;";
        assert_eq!(query_execution_target(text, Some((10, 18)), 0), Some(("select 2".to_string(), 10)));
        assert_eq!(query_execution_target(text, None, 3), Some(("select 1;".to_string(), 0)));
    }

    #[test]
    fn postgres_character_offsets_map_unicode_to_bytes() {
        assert_eq!(postgres_character_to_byte("Жx", 0), 0);
        assert_eq!(postgres_character_to_byte("Жx", 1), 2);
        assert_eq!(postgres_character_to_byte("Жx", 2), 3);
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
        let words = completion_words(&metadata, "select u. from users as u", 9);
        assert!(words.iter().any(|(word, _)| word == "email"));
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
        state.mark_running(
            "ALTER ROLE demo PASSWORD 'secret'".to_string(),
            123,
        );
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
        assert_eq!(entry.error_summary.as_deref(), Some("Запрос отменён пользователем"));
        assert!(!state.running);
        assert!(state.running_sql.is_none());
        assert_eq!(state.running_started_unix_ms, 0);
    }

    #[test]
    fn explain_rejects_multiple_statements() {
        let statements = validate_managed_user_sql("select 1; select 2").unwrap();
        assert!(explain_sql("select 1; select 2", &statements, DatabaseQueryMode::Explain).is_err());
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
    fn completion_contains_postgresql_specific_constructs_and_operators() {
        let metadata = DatabaseQueryCompletionMetadata {
            enum_values: vec!["active".to_string()],
            functions: vec!["jsonb_set".to_string()],
            operators: vec!["->>".to_string()],
            ..DatabaseQueryCompletionMetadata::default()
        };
        let words = completion_words(&metadata, "INSERT INTO jobs ", 17);
        for expected in ["RETURNING", "ON CONFLICT", "ARRAY", "jsonb_set", "->>"] {
            assert!(words.iter().any(|(word, _)| word == expected), "missing {expected}");
        }
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
    fn completion_catalog_queries_have_one_from_clause_and_one_statement() {
        for sql in [
            QUERY_COMPLETION_COLUMNS_SQL,
            QUERY_COMPLETION_ENUMS_SQL,
            QUERY_COMPLETION_FUNCTIONS_SQL,
            QUERY_COMPLETION_OPERATORS_SQL,
        ] {
            assert_eq!(crate::languages::sql::scan_statements(sql).len(), 1, "{sql}");
        }
        assert_eq!(
            QUERY_COMPLETION_FUNCTIONS_SQL
                .to_ascii_uppercase()
                .matches("FROM PG_PROC")
                .count(),
            1
        );
    }

    #[test]
    fn result_memory_estimate_includes_cells_columns_and_tags() {
        let result = DatabaseQueryResultSet {
            title: "Result 1".to_string(),
            columns: vec!["value".to_string()],
            rows: vec![vec![DatabaseQueryCell { value: Some("hello".to_string()) }]],
            command_tag: "SELECT 1".to_string(),
            ..DatabaseQueryResultSet::default()
        };
        assert!(result.estimated_bytes() >= "Result 1valuehelloSELECT 1".len());
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
}
