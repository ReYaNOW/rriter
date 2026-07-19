use super::database_postgres::{
    DatabaseBackendError, DatabaseServerNotice, PostgresSession, connect_postgres,
    rollback_postgres_transaction_after_error,
};
use super::database_ssh::SshConnectOptions;
use super::DatabaseQueryTabMeta;
use super::{
    DatabaseConnectionConfig, DatabaseConnectionId, DatabaseSecretBundle, DatabaseSettings,
    DatabaseTransactionId, MAX_COLUMNS_PER_RESULT, MAX_RESULT_BYTES, MAX_RESULT_ROWS,
    MAX_RESULT_SETS,
};
use crate::languages::sql::{
    SqlStatement, format_sql_conservative, statement_range_at, validate_managed_user_sql,
};
use crate::languages::sql_analysis::{
    SqlAnalysis, SqlAnalysisDiagnostic, SqlCompletionContext, SqlCompletionKind,
    SqlDiagnosticSeverity, analyze_sql, completion_context, output_aliases_at,
    relation_for_qualifier,
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
    pub command_kind: String,
    pub returned_rows: u64,
    pub affected_rows: u64,
    pub truncated: bool,
}

impl DatabaseQueryResultSet {
    pub fn estimated_bytes(&self) -> usize {
        self.title
            .len()
            .saturating_add(self.command_kind.len())
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
    pub returned_rows: u64,
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
            returned_rows: 0,
            affected_rows: 0,
            error_summary: None,
        }
    }
}

impl DatabaseQueryHistoryEntry {
    pub fn normalize(&mut self) {
        super::truncate_utf8(&mut self.database_name, 128);
        super::truncate_utf8(&mut self.sql, super::MAX_SQL_CONSOLE_BYTES);
        if let Some(error) = &mut self.error_summary {
            super::truncate_utf8(error, 4_096);
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

pub const DATABASE_QUERY_RESULTS_DEFAULT_HEIGHT: f32 = 260.0;
pub const DATABASE_QUERY_RESULTS_MIN_HEIGHT: f32 = 140.0;
pub const DATABASE_QUERY_EDITOR_MIN_HEIGHT: f32 = 220.0;

pub fn database_query_results_visible(state: &DatabaseQueryTabState) -> bool {
    state.history_open
        || !state.results.is_empty()
        || !state.messages.is_empty()
        || state.review.is_some()
        || state.error.is_some()
}

pub fn database_query_results_height(
    preferred_height: f32,
    window_height: f32,
    bottom_panel_height: f32,
    scale: f32,
) -> f32 {
    let min_height = (DATABASE_QUERY_RESULTS_MIN_HEIGHT * scale).round();
    let available_height = (window_height
        - bottom_panel_height
        - (DATABASE_QUERY_EDITOR_MIN_HEIGHT * scale).round())
    .max(0.0);
    let desired = (preferred_height.max(DATABASE_QUERY_RESULTS_MIN_HEIGHT) * scale).round();
    if available_height < min_height {
        available_height
    } else {
        desired.clamp(min_height, available_height)
    }
}

#[derive(Clone, Debug)]
pub struct DatabaseQueryResultViewState {
    pub active_result: usize,
    pub preferred_height: f32,
    pub is_resizing_height: bool,
    pub scroll_x: crate::scroll::ScrollState,
    pub scroll_y: crate::scroll::ScrollState,
    pub review_message_scroll_y: crate::scroll::ScrollState,
    pub review_message_max_scroll: std::cell::Cell<f32>,
    pub column_widths: Vec<super::DatabaseColumnWidth>,
    pub column_resize: Option<(usize, f32, f32)>,
    pub selected_row: Option<usize>,
    pub selected_column: Option<usize>,
}

impl Default for DatabaseQueryResultViewState {
    fn default() -> Self {
        Self {
            active_result: 0,
            preferred_height: DATABASE_QUERY_RESULTS_DEFAULT_HEIGHT,
            is_resizing_height: false,
            scroll_x: crate::scroll::ScrollState::new(15.0),
            scroll_y: crate::scroll::ScrollState::new(15.0),
            review_message_scroll_y: crate::scroll::ScrollState::new(15.0),
            review_message_max_scroll: std::cell::Cell::new(0.0),
            column_widths: Vec::new(),
            column_resize: None,
            selected_row: None,
            selected_column: None,
        }
    }
}

impl PartialEq for DatabaseQueryResultViewState {
    fn eq(&self, other: &Self) -> bool {
        self.active_result == other.active_result
            && self.preferred_height.to_bits() == other.preferred_height.to_bits()
            && self.is_resizing_height == other.is_resizing_height
            && self.scroll_x.current.to_bits() == other.scroll_x.current.to_bits()
            && self.scroll_x.target.to_bits() == other.scroll_x.target.to_bits()
            && self.scroll_y.current.to_bits() == other.scroll_y.current.to_bits()
            && self.scroll_y.target.to_bits() == other.scroll_y.target.to_bits()
            && self.review_message_scroll_y.current.to_bits()
                == other.review_message_scroll_y.current.to_bits()
            && self.review_message_scroll_y.target.to_bits()
                == other.review_message_scroll_y.target.to_bits()
            && self.review_message_max_scroll.get().to_bits()
                == other.review_message_max_scroll.get().to_bits()
            && self.column_widths == other.column_widths
            && self.column_resize == other.column_resize
            && self.selected_row == other.selected_row
            && self.selected_column == other.selected_column
    }
}

impl Eq for DatabaseQueryResultViewState {}

impl DatabaseQueryResultViewState {
    pub fn reset_scroll(&mut self) {
        self.scroll_x.reset();
        self.scroll_y.reset();
        self.review_message_scroll_y.reset();
        self.review_message_max_scroll.set(0.0);
    }
}

pub fn database_query_history_preview_lines(sql: &str) -> usize {
    sql.lines().take(20).count().max(1)
}

pub fn database_query_history_is_truncated(sql: &str) -> bool {
    sql.lines().nth(20).is_some()
}

pub fn database_query_history_entry_height(sql: &str) -> f32 {
    let lines = database_query_history_preview_lines(sql) as f32;
    30.0 + lines * 20.0 + if database_query_history_is_truncated(sql) { 18.0 } else { 0.0 }
}

pub fn database_query_history_entry_height_px(sql: &str, scale: f32) -> f32 {
    (database_query_history_entry_height(sql) * scale.max(0.0)).round()
}

pub fn database_query_history_entry_bytes(entry: &DatabaseQueryHistoryEntry) -> usize {
    entry
        .sql
        .len()
        .saturating_add(entry.database_name.len())
        .saturating_add(entry.error_summary.as_ref().map_or(0, String::len))
        .saturating_add(64)
}

pub fn trim_database_query_history(
    history: &mut Vec<DatabaseQueryHistoryEntry>,
    entry_limit: usize,
    byte_limit: usize,
) {
    let remove_for_count = history.len().saturating_sub(entry_limit);
    let total_bytes = history
        .iter()
        .map(database_query_history_entry_bytes)
        .sum::<usize>();
    let mut remove_for_bytes = 0usize;
    let mut remaining_bytes = total_bytes;
    while remove_for_bytes < history.len() && remaining_bytes > byte_limit {
        remaining_bytes = remaining_bytes.saturating_sub(database_query_history_entry_bytes(
            &history[remove_for_bytes],
        ));
        remove_for_bytes += 1;
    }
    let remove = remove_for_count.max(remove_for_bytes);
    if remove > 0 {
        history.drain(0..remove);
    }
}

pub fn database_query_history_content_height<'a>(
    entries: impl Iterator<Item = &'a DatabaseQueryHistoryEntry>,
    scale: f32,
) -> f32 {
    entries
        .map(|entry| database_query_history_entry_height_px(&entry.sql, scale))
        .sum()
}

/// Returns horizontal and vertical scroll limits for the shared query result viewport.
pub fn database_query_scroll_limits(
    meta: &DatabaseQueryTabMeta,
    state: &DatabaseQueryTabState,
    history: &[DatabaseQueryHistoryEntry],
    viewport_width: f32,
    viewport_height: f32,
    scale: f32,
) -> (f32, f32) {
    if state.history_open {
        let content_height = database_query_history_content_height(
            history.iter().filter(|entry| {
                entry.connection_id == meta.connection_id
                    && entry.database_name == meta.database_name
            }),
            scale,
        );
        return (0.0, (content_height - viewport_height).max(0.0));
    }
    if let Some(result) = state.results.get(state.result_view.active_result) {
        let content_width = super::database_columns_content_width(
            &state.result_view.column_widths,
            result.columns.iter().map(String::as_str),
        ) * scale;
        let row_height = super::DATABASE_GRID_ROW_HEIGHT * scale;
        return (
            (content_width - viewport_width).max(0.0),
            super::database_grid_max_scroll(result.rows.len(), row_height, viewport_height),
        );
    }
    (0.0, 0.0)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseQueryReviewState {
    pub transaction_id: DatabaseTransactionId,
    pub sql: String,
    pub source_offset: usize,
    pub started_unix_ms: u128,
    pub deadline_unix_ms: u128,
    pub duration_ms: u64,
    pub returned_rows: u64,
    pub changed_rows: u64,
    pub mode: DatabaseQueryMode,
    pub finishing: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DatabaseQueryTabState {
    pub running: bool,
    pub running_sql: Option<String>,
    pub running_started_unix_ms: u128,
    pub error: Option<String>,
    pub diagnostic: Option<DatabaseQueryDiagnostic>,
    pub diagnostic_editor_version: Option<u64>,
    pub editor_diagnostics: Vec<crate::lsp::Diagnostic>,
    pub analysis: SqlAnalysis,
    pub analysis_editor_version: Option<u64>,
    pub results: Vec<DatabaseQueryResultSet>,
    pub messages: Vec<DatabaseQueryMessage>,
    pub result_view: DatabaseQueryResultViewState,
    pub review: Option<DatabaseQueryReviewState>,
    pub completion: DatabaseQueryCompletionMetadata,
    pub completion_loaded: bool,
    pub history_open: bool,
    pub history_selected: usize,
    pub last_duration_ms: u64,
    pub last_returned_rows: u64,
    pub last_changed_rows: u64,
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
            returned_rows: 0,
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
        if start < end && let Some(raw) = text.get(start..end) {
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
            .then_with(|| diagnostic_severity_rank(left.severity).cmp(&diagnostic_severity_rank(right.severity)))
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
        quickfixes: Box::new([]),
        tags: Box::new([]),
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
                out.push((quote_completion_identifier(alias), "alias SELECT".to_string()));
            }
            for function in &metadata.functions {
                out.push((function.clone(), "function".to_string()));
            }
        }
        SqlCompletionKind::Operator => {
            for operator in &metadata.operators {
                out.push((operator.clone(), "operator".to_string()));
            }
            for operator in ["=", "<>", "!=", "<", ">", "<=", ">=", "LIKE", "ILIKE", "IN", "BETWEEN", "IS NULL", "IS NOT NULL"] {
                out.push((operator.to_string(), "operator".to_string()));
            }
        }
        SqlCompletionKind::Value => {
            for value in &metadata.enum_values {
                out.push((format!("'{}'", value.replace('\'', "''")), "enum".to_string()));
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
        out.retain(|(word, _)| {
            let candidate = word.trim_matches('"').trim_matches('\'').to_ascii_lowercase();
            candidate.contains(&prefix)
        });
    }
    out.sort_unstable_by(|left, right| {
        completion_rank(&left.0, &prefix)
            .cmp(&completion_rank(&right.0, &prefix))
            .then_with(|| left.0.to_ascii_lowercase().cmp(&right.0.to_ascii_lowercase()))
            .then_with(|| left.1.cmp(&right.1))
    });
    out.dedup_by(|left, right| left.0 == right.0 && left.1 == right.1);
    out
}

fn completion_rank(candidate: &str, prefix: &str) -> u8 {
    if prefix.is_empty() {
        return 0;
    }
    let candidate = candidate.trim_matches('"').trim_matches('\'').to_ascii_lowercase();
    if candidate == prefix {
        0
    } else if candidate.starts_with(prefix) {
        1
    } else {
        2
    }
}

fn quote_completion_identifier(identifier: &str) -> String {
    let mut chars = identifier.chars();
    let simple = chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_lowercase())
        && chars.all(|ch| ch == '_' || ch.is_ascii_lowercase() || ch.is_ascii_digit());
    if simple && !crate::languages::sql::SQL_KEYWORDS
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
        if relation.is_cte || relation.schema.as_deref().is_some_and(|schema| !schema.eq_ignore_ascii_case("public")) {
            continue;
        }
        if !known_tables.contains(&relation.table_name.to_ascii_lowercase()) {
            diagnostics.push(SqlAnalysisDiagnostic {
                range: relation.source_range.clone(),
                severity: SqlDiagnosticSeverity::Error,
                code: "SQL202",
                message: format!("Таблица «{}» не найдена в public schema", relation.table_name),
            });
        }
    }
    for reference in &analysis.qualified_references {
        let Some(relation) = relation_for_qualifier(analysis, reference.range.start, &reference.qualifier) else {
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
                    && relation.schema.as_deref().is_none_or(|schema| schema.eq_ignore_ascii_case("public"))
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
                message: format!("Столбец «{}» не найден в таблицах текущего SQL-блока", reference.name),
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
    let result = execute_simple_query(&session, &execution.text, &statements, settings).await;
    match result {
        Ok((result_sets, mut effects)) => {
            mark_explain_analyze_side_effects(mode, &statements, &mut effects);
            let messages = session
                .drain_server_notices()
                .into_iter()
                .map(DatabaseQueryMessage::from)
                .collect();
            Ok((session, DatabasePreparedQueryTransaction {
                result_sets,
                messages,
                effects,
                mode,
            }))
        }
        Err(error) => {
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
                    return Err(DatabaseBackendError::LimitExceeded("result has more than 512 columns"));
                }
                current = Some(DatabaseQueryResultSet {
                    title: format!("Результат {}", result_sets.len() + 1),
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
                    title: format!("Результат {}", result_sets.len() + 1),
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
                    effects.returned_rows = effects
                        .returned_rows
                        .saturating_add(result.returned_rows);
                    result.affected_rows = if kind
                        == crate::languages::sql::SqlStatementKind::Mutation
                    {
                        affected
                    } else {
                        0
                    };
                    result.command_kind = command_kind.clone();
                    push_result(&mut result_sets, result)?;
                } else {
                    let title = format!("Результат {}", result_sets.len() + 1);
                    push_result(&mut result_sets, DatabaseQueryResultSet {
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
                    })?;
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
            if keyword == "WITH" { "SELECT".to_string() } else { keyword }
        }
        crate::languages::sql::SqlStatementKind::Mutation
        | crate::languages::sql::SqlStatementKind::Definition
        | crate::languages::sql::SqlStatementKind::Explain => keyword,
        crate::languages::sql::SqlStatementKind::Other => {
            if keyword.is_empty() { "COMMAND".to_string() } else { keyword }
        }
    }
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

    super::truncate_utf8(&mut output, super::MAX_SQL_CONSOLE_BYTES);
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

    fn completion_for(
        metadata: &DatabaseQueryCompletionMetadata,
        sql: &str,
        cursor: usize,
    ) -> Vec<(String, String)> {
        let context = database_query_completion_context(sql, cursor);
        let base = analyze_database_query_sql(metadata, sql);
        let analysis = completion_recovery_analysis(
            metadata,
            sql,
            cursor,
            &context,
            &base,
        )
        .unwrap_or(base);
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
        assert!(!words.iter().any(|(word, detail)| {
            word == "capacity" || detail.starts_with("car_wash ·")
        }));
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
        let statements = crate::languages::sql::validate_managed_user_sql(
            "UPDATE items SET value = 2",
        )
        .unwrap();
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
        let diagnostics = database_query_editor_diagnostics(
            &analysis,
            None,
            text,
            &[0],
        );

        assert_eq!(diagnostics.len(), 1);
        let diagnostic = &diagnostics[0];
        assert_eq!(diagnostic.code.as_deref(), Some("SQL001"));
        assert_eq!(diagnostic.source.as_deref(), Some("RRiter SQL"));
        assert_eq!(diagnostic.severity, crate::lsp::DiagSeverity::Error);
        assert_eq!(diagnostic.start_line, diagnostic.end_line);
        assert_eq!(diagnostic.start_col, diagnostic.end_col);
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
        let diagnostics = database_query_editor_diagnostics(
            &SqlAnalysis::default(),
            Some(&backend),
            text,
            &[0],
        );

        assert_eq!(diagnostics.len(), 1);
        let diagnostic = &diagnostics[0];
        assert_eq!(diagnostic.code.as_deref(), Some("42601"));
        assert_eq!(diagnostic.source.as_deref(), Some("PostgreSQL"));
        assert!(diagnostic.message.contains("detail"));
        assert!(diagnostic.message.contains("Подсказка: hint"));
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
        let diagnostics = database_query_editor_diagnostics(
            &analysis,
            None,
            text,
            &[0, 12],
        );

        assert_eq!(next_database_query_diagnostic_offset(&diagnostics, text, 0), Some(7));
        assert_eq!(next_database_query_diagnostic_offset(&diagnostics, text, 7), Some(19));
        assert_eq!(next_database_query_diagnostic_offset(&diagnostics, text, 22), Some(7));
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
                analysis.diagnostics.iter().any(|diagnostic| diagnostic.code == code),
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
        assert!(unknown.diagnostics.iter().any(|diagnostic| diagnostic.code == "SQL202"));

        let cte = analyze_database_query_sql(
            &metadata,
            "WITH recent AS (SELECT id FROM booking) SELECT r.id FROM recent r",
        );
        assert!(!cte.diagnostics.iter().any(|diagnostic| diagnostic.code == "SQL202"));
    }

    #[test]
    fn query_result_height_clamps_to_editor_and_panel_space() {
        assert_eq!(database_query_results_height(260.0, 900.0, 0.0, 1.0), 260.0);
        assert_eq!(database_query_results_height(80.0, 900.0, 0.0, 1.0), 140.0);
        assert_eq!(database_query_results_height(900.0, 900.0, 180.0, 1.0), 500.0);
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
            rows: vec![vec![DatabaseQueryCell::default(), DatabaseQueryCell::default()]],
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
        assert!(entry.database_name.is_char_boundary(entry.database_name.len()));
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

}
