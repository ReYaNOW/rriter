use super::DatabaseQueryTabMeta;
use super::database_postgres::{
    DatabaseBackendError, DatabaseServerNotice, PostgresSession, connect_postgres,
    rollback_postgres_transaction_after_error,
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
    let available_height =
        (window_height - bottom_panel_height - (DATABASE_QUERY_EDITOR_MIN_HEIGHT * scale).round())
            .max(0.0);
    let desired = (preferred_height.max(DATABASE_QUERY_RESULTS_MIN_HEIGHT) * scale).round();
    if available_height < min_height {
        available_height
    } else {
        desired.clamp(min_height, available_height)
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct DatabaseQueryHistoryLayoutEntry {
    pub(crate) history_index: usize,
    pub(crate) offset_y: f32,
    pub(crate) height: f32,
    pub(crate) preview_lines: usize,
    pub(crate) truncated: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct DatabaseQueryHistoryLayoutCache {
    valid: bool,
    revision: u64,
    history_len: usize,
    connection_id: Option<DatabaseConnectionId>,
    database_name: String,
    scale_bits: u32,
    content_height: f32,
    entries: Vec<DatabaseQueryHistoryLayoutEntry>,
    #[cfg(test)]
    rebuild_count: usize,
}

impl DatabaseQueryHistoryLayoutCache {
    fn matches(
        &self,
        revision: u64,
        history_len: usize,
        meta: &DatabaseQueryTabMeta,
        scale: f32,
    ) -> bool {
        self.valid
            && self.revision == revision
            && self.history_len == history_len
            && self.connection_id == Some(meta.connection_id)
            && self.database_name == meta.database_name
            && self.scale_bits == scale.max(0.0).to_bits()
    }

    fn rebuild(
        &mut self,
        revision: u64,
        history: &[DatabaseQueryHistoryEntry],
        meta: &DatabaseQueryTabMeta,
        scale: f32,
    ) {
        let scale = scale.max(0.0);
        self.entries.clear();
        let mut offset_y = 0.0;
        for (history_index, entry) in history.iter().enumerate().rev() {
            if entry.connection_id != meta.connection_id
                || entry.database_name != meta.database_name
            {
                continue;
            }
            let (preview_lines, truncated) = database_query_history_preview_metrics(&entry.sql);
            let height =
                database_query_history_entry_height_from_metrics(preview_lines, truncated, scale);
            self.entries.push(DatabaseQueryHistoryLayoutEntry {
                history_index,
                offset_y,
                height,
                preview_lines,
                truncated,
            });
            offset_y += height;
        }
        self.valid = true;
        self.revision = revision;
        self.history_len = history.len();
        self.connection_id = Some(meta.connection_id);
        self.database_name.clear();
        self.database_name.push_str(&meta.database_name);
        self.scale_bits = scale.to_bits();
        self.content_height = offset_y;
        #[cfg(test)]
        {
            self.rebuild_count = self.rebuild_count.saturating_add(1);
        }
    }

    pub(crate) fn content_height(&self) -> f32 {
        self.content_height
    }

    pub(crate) fn entries(&self) -> &[DatabaseQueryHistoryLayoutEntry] {
        &self.entries
    }

    pub(crate) fn visible_range(
        &self,
        scroll_y: f32,
        viewport_height: f32,
    ) -> std::ops::Range<usize> {
        let top = scroll_y.max(0.0);
        let bottom = top + viewport_height.max(0.0);
        let start = self
            .entries
            .partition_point(|entry| entry.offset_y + entry.height <= top);
        let end = start + self.entries[start..].partition_point(|entry| entry.offset_y < bottom);
        start..end
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct DatabaseQueryReviewMessageLayoutItem {
    pub(crate) text: String,
    pub(crate) color: [f32; 4],
    pub(crate) ranges: Vec<(usize, usize)>,
    pub(crate) offset_y: f32,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct DatabaseQueryReviewMessageLayoutCache {
    valid: bool,
    revision: u64,
    message_count: usize,
    max_text_width_bits: u32,
    scale_bits: u32,
    /// UI palette the item colours were resolved against; a theme switch changes
    /// it and so rebuilds the items.
    palette_roles: Option<[[f32; 4]; crate::theme::UiRole::COUNT]>,
    line_height: f32,
    item_gap: f32,
    total_height: f32,
    items: Vec<DatabaseQueryReviewMessageLayoutItem>,
}

impl DatabaseQueryReviewMessageLayoutCache {
    pub(crate) fn matches(
        &self,
        revision: u64,
        message_count: usize,
        max_text_width: f32,
        scale: f32,
        palette_roles: &[[f32; 4]; crate::theme::UiRole::COUNT],
    ) -> bool {
        self.valid
            && self.palette_roles.as_ref() == Some(palette_roles)
            && self.revision == revision
            && self.message_count == message_count
            && self.max_text_width_bits == max_text_width.max(0.0).to_bits()
            && self.scale_bits == scale.max(0.0).to_bits()
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn replace(
        &mut self,
        revision: u64,
        message_count: usize,
        max_text_width: f32,
        scale: f32,
        palette_roles: [[f32; 4]; crate::theme::UiRole::COUNT],
        line_height: f32,
        item_gap: f32,
        total_height: f32,
        items: Vec<DatabaseQueryReviewMessageLayoutItem>,
    ) {
        self.valid = true;
        self.palette_roles = Some(palette_roles);
        self.revision = revision;
        self.message_count = message_count;
        self.max_text_width_bits = max_text_width.max(0.0).to_bits();
        self.scale_bits = scale.max(0.0).to_bits();
        self.line_height = line_height;
        self.item_gap = item_gap;
        self.total_height = total_height;
        self.items = items;
    }

    pub(crate) fn line_height(&self) -> f32 {
        self.line_height
    }
    pub(crate) fn item_gap(&self) -> f32 {
        self.item_gap
    }
    pub(crate) fn total_height(&self) -> f32 {
        self.total_height
    }
    pub(crate) fn items(&self) -> &[DatabaseQueryReviewMessageLayoutItem] {
        &self.items
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
    history_layout_revision: u64,
    history_layout_cache: std::cell::RefCell<DatabaseQueryHistoryLayoutCache>,
    review_message_layout_revision: u64,
    pub(crate) review_message_layout_cache:
        std::cell::RefCell<DatabaseQueryReviewMessageLayoutCache>,
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
            history_layout_revision: 0,
            history_layout_cache: std::cell::RefCell::new(
                DatabaseQueryHistoryLayoutCache::default(),
            ),
            review_message_layout_revision: 0,
            review_message_layout_cache: std::cell::RefCell::new(
                DatabaseQueryReviewMessageLayoutCache::default(),
            ),
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

    pub(crate) fn invalidate_history_layout(&mut self) {
        self.history_layout_revision = self.history_layout_revision.wrapping_add(1);
    }

    pub(crate) fn invalidate_review_message_layout(&mut self) {
        self.review_message_layout_revision = self.review_message_layout_revision.wrapping_add(1);
        self.review_message_max_scroll.set(0.0);
    }

    pub(crate) fn review_message_layout_revision(&self) -> u64 {
        self.review_message_layout_revision
    }

    pub(crate) fn history_layout<'a>(
        &'a self,
        meta: &DatabaseQueryTabMeta,
        history: &[DatabaseQueryHistoryEntry],
        scale: f32,
    ) -> std::cell::Ref<'a, DatabaseQueryHistoryLayoutCache> {
        let needs_rebuild = {
            let cache = self.history_layout_cache.borrow();
            !cache.matches(self.history_layout_revision, history.len(), meta, scale)
        };
        if needs_rebuild {
            self.history_layout_cache.borrow_mut().rebuild(
                self.history_layout_revision,
                history,
                meta,
                scale,
            );
        }
        self.history_layout_cache.borrow()
    }
}

fn database_query_history_preview_metrics(sql: &str) -> (usize, bool) {
    let mut lines = sql.lines();
    let preview_lines = lines.by_ref().take(20).count().max(1);
    (preview_lines, lines.next().is_some())
}

fn database_query_history_entry_height_from_metrics(
    preview_lines: usize,
    truncated: bool,
    scale: f32,
) -> f32 {
    let height = 30.0 + preview_lines as f32 * 20.0 + if truncated { 18.0 } else { 0.0 };
    (height * scale.max(0.0)).round()
}

#[cfg(test)]
pub fn database_query_history_preview_lines(sql: &str) -> usize {
    database_query_history_preview_metrics(sql).0
}

#[cfg(test)]
pub fn database_query_history_is_truncated(sql: &str) -> bool {
    database_query_history_preview_metrics(sql).1
}

#[cfg(test)]
pub fn database_query_history_entry_height(sql: &str) -> f32 {
    let (preview_lines, truncated) = database_query_history_preview_metrics(sql);
    30.0 + preview_lines as f32 * 20.0 + if truncated { 18.0 } else { 0.0 }
}

#[cfg(test)]
pub fn database_query_history_entry_height_px(sql: &str, scale: f32) -> f32 {
    let (preview_lines, truncated) = database_query_history_preview_metrics(sql);
    database_query_history_entry_height_from_metrics(preview_lines, truncated, scale)
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

#[cfg(test)]
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
        let content_height = state
            .result_view
            .history_layout(meta, history, scale)
            .content_height();
        return (0.0, (content_height - viewport_height).max(0.0));
    }
    if let Some(result) = state.results.get(state.result_view.active_result) {
        let content_width = super::database_columns_content_width(
            &state.result_view.column_widths,
            result.columns.iter().map(String::as_str),
        ) * scale;
        let row_height = super::database_grid_row_height_px(scale);
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
        let Some(relative) = lower.find("password") else {
            break;
        };
        let token_start = cursor + relative;
        let after = token_start + "password".len();
        let boundary_before =
            token_start == 0 || !output.as_bytes()[token_start - 1].is_ascii_alphanumeric();
        let boundary_after =
            after >= output.len() || !output.as_bytes()[after].is_ascii_alphanumeric();
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
        let Some(relative) = lower.find("password=") else {
            break;
        };
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

include!("database_query_analysis_completion.rs");
include!("database_query_execution.rs");

#[cfg(test)]
mod tests {
    use super::*;
    include!("database_query_analysis_completion_tests.rs");
    include!("database_query_execution_tests.rs");
    include!("database_query_history_tests.rs");
}
