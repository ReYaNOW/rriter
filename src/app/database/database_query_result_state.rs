use super::{
    DatabaseConnectionId, DatabasePanelState, DatabaseQueryHistoryEntry, DatabaseQueryTabState,
    DATABASE_GRID_MAX_COLUMN_WIDTH, DATABASE_GRID_MIN_COLUMN_WIDTH, MAX_SQL_HISTORY_BYTES,
    MAX_SQL_HISTORY_ENTRIES, analysis_error_ranges, analyze_database_query_sql,
    database_query_editor_diagnostics, query_execution_target, sanitize_history_sql,
    set_database_column_width, trim_database_query_history,
};
use crate::languages::sql_analysis::SqlDiagnosticSeverity;

pub(crate) struct DatabaseQueryExecutionPlan {
    pub(crate) sql: String,
    pub(crate) source_offset: usize,
    pub(crate) error_ranges: Vec<(usize, usize)>,
    pub(crate) has_errors: bool,
}

impl DatabaseQueryTabState {
    pub(crate) fn database_query_prepare_execution(
        &mut self,
        text: &str,
        selection: Option<(usize, usize)>,
        cursor: usize,
        editor_version: u64,
        line_offsets: &[usize],
    ) -> Option<DatabaseQueryExecutionPlan> {
        let (sql, source_offset) = query_execution_target(text, selection, cursor)?;
        let mut analysis = analyze_database_query_sql(&self.completion, &sql);
        for diagnostic in &mut analysis.diagnostics {
            diagnostic.range.start = diagnostic.range.start.saturating_add(source_offset);
            diagnostic.range.end = diagnostic.range.end.saturating_add(source_offset);
        }
        let error_ranges = analysis_error_ranges(&analysis);
        let editor_diagnostics = database_query_editor_diagnostics(
            &analysis,
            None,
            text,
            line_offsets,
        );
        let has_errors = analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == SqlDiagnosticSeverity::Error);
        self.analysis = analysis;
        self.analysis_editor_version = Some(editor_version);
        self.editor_diagnostics = editor_diagnostics;
        if has_errors {
            self.running = false;
            self.error = Some(
                "SQL-анализатор обнаружил ошибки. Исправьте их перед выполнением.".to_string(),
            );
            self.messages.clear();
            self.result_view.invalidate_review_message_layout();
            self.result_view.active_result = self.results.len();
            self.result_view.reset_scroll();
        }
        Some(DatabaseQueryExecutionPlan {
            sql,
            source_offset,
            error_ranges,
            has_errors,
        })
    }

    pub(crate) fn database_query_toggle_history(&mut self) {
        self.history_open = !self.history_open;
        self.history_selected = 0;
        self.result_view.reset_scroll();
    }

    pub(crate) fn database_query_select_result(&mut self, index: usize) {
        self.history_open = false;
        self.result_view.active_result = index.min(self.results.len());
        self.result_view.reset_scroll();
    }

    pub(crate) fn database_query_move_history_selection(&mut self, delta: i32, len: usize) {
        if len == 0 {
            return;
        }
        self.history_selected = if delta < 0 {
            self.history_selected
                .saturating_sub(delta.unsigned_abs() as usize)
        } else {
            self.history_selected
                .saturating_add(delta as usize)
                .min(len - 1)
        };
    }

    pub(crate) fn database_query_set_history_selection(&mut self, last: bool, len: usize) {
        self.history_selected = if last { len.saturating_sub(1) } else { 0 };
    }

    pub(crate) fn database_query_start_result_resize(&mut self) {
        self.result_view.is_resizing_height = true;
    }

    pub(crate) fn database_query_set_result_height(&mut self, height: f32) {
        self.result_view.preferred_height = height;
    }

    pub(crate) fn database_query_auto_size_column(&mut self, column_index: usize) -> bool {
        let Some(result) = self.results.get(self.result_view.active_result) else {
            return false;
        };
        let Some(column_name) = result.columns.get(column_index) else {
            return false;
        };
        let mut max_chars = column_name.chars().count().saturating_add(3);
        for row in result.rows.iter().take(100) {
            if let Some(cell) = row.get(column_index) {
                max_chars = max_chars.max(cell.display_text().chars().count().min(160));
            }
        }
        let width = (max_chars as f32 * 8.0 + 24.0)
            .clamp(DATABASE_GRID_MIN_COLUMN_WIDTH, DATABASE_GRID_MAX_COLUMN_WIDTH);
        set_database_column_width(
            &mut self.result_view.column_widths,
            column_name,
            width,
        );
        true
    }

    pub(crate) fn database_query_start_column_resize(
        &mut self,
        column_index: usize,
        mouse_x: f32,
    ) -> bool {
        let Some(result) = self.results.get(self.result_view.active_result) else {
            return false;
        };
        let Some(column_name) = result.columns.get(column_index) else {
            return false;
        };
        let width = super::database_column_width(&self.result_view.column_widths, column_name);
        self.result_view.column_resize = Some((column_index, mouse_x, width));
        true
    }

    pub(crate) fn database_query_update_column_resize(&mut self, mouse_x: f32) -> bool {
        let Some((column_index, start_x, start_width)) = self.result_view.column_resize else {
            return false;
        };
        let Some(column_name) = self
            .results
            .get(self.result_view.active_result)
            .and_then(|result| result.columns.get(column_index))
            .cloned()
        else {
            return false;
        };
        set_database_column_width(
            &mut self.result_view.column_widths,
            &column_name,
            start_width + mouse_x - start_x,
        );
        true
    }

    pub(crate) fn database_query_finish_scroll_drag(&mut self) {
        self.result_view.scroll_x.end_drag();
        self.result_view.scroll_y.end_drag();
        self.result_view.review_message_scroll_y.end_drag();
        self.result_view.is_resizing_height = false;
        self.result_view.column_resize = None;
    }
}

impl DatabasePanelState {
    pub(crate) fn database_query_history_len(
        &self,
        connection_id: DatabaseConnectionId,
        database_name: &str,
    ) -> usize {
        self.persisted
            .query_history
            .iter()
            .filter(|entry| {
                entry.connection_id == connection_id && entry.database_name == database_name
            })
            .count()
    }

    pub(crate) fn database_query_history_entry(
        &self,
        connection_id: DatabaseConnectionId,
        database_name: &str,
        visible_index: usize,
    ) -> Option<DatabaseQueryHistoryEntry> {
        self.persisted
            .query_history
            .iter()
            .rev()
            .filter(|entry| {
                entry.connection_id == connection_id && entry.database_name == database_name
            })
            .nth(visible_index)
            .cloned()
    }

    pub(crate) fn database_query_record_history(
        &mut self,
        mut entry: DatabaseQueryHistoryEntry,
    ) -> bool {
        entry.sql = sanitize_history_sql(&entry.sql);
        if let Some(error) = entry.error_summary.as_mut() {
            *error = sanitize_history_sql(error);
        }
        entry.normalize();
        let limit = self
            .settings()
            .sql_history_limit
            .min(MAX_SQL_HISTORY_ENTRIES);
        let history = &mut self.persisted.query_history;
        history.push(entry);
        trim_database_query_history(history, limit, MAX_SQL_HISTORY_BYTES);
        true
    }

    pub(crate) fn database_query_adjust_setting(&mut self, setting: usize, delta: i32) -> bool {
        let settings = &mut self.persisted.settings;
        match setting {
            0 => adjust_u64(&mut settings.transaction_review_timeout_seconds, delta, 30),
            1 => adjust_u64(&mut settings.statement_timeout_seconds, delta, 1),
            2 => adjust_u64(&mut settings.lock_timeout_seconds, delta, 1),
            3 => adjust_u64(&mut settings.connect_timeout_seconds, delta, 1),
            4 => adjust_u64(&mut settings.ssh_startup_timeout_seconds, delta, 1),
            5 => adjust_usize(&mut settings.default_table_limit, delta, 10),
            6 => adjust_usize(&mut settings.result_row_limit, delta, 1_000),
            7 => adjust_usize(&mut settings.result_memory_limit_bytes, delta, 1024 * 1024),
            8 => adjust_usize(&mut settings.sql_history_limit, delta, 10),
            9 => {
                settings.default_connection_color = if delta >= 0 {
                    match settings.default_connection_color {
                        super::DatabaseConnectionColor::Blue => super::DatabaseConnectionColor::Green,
                        super::DatabaseConnectionColor::Green => super::DatabaseConnectionColor::Yellow,
                        super::DatabaseConnectionColor::Yellow => super::DatabaseConnectionColor::Orange,
                        super::DatabaseConnectionColor::Orange => super::DatabaseConnectionColor::Red,
                        super::DatabaseConnectionColor::Red => super::DatabaseConnectionColor::Purple,
                        super::DatabaseConnectionColor::Purple => super::DatabaseConnectionColor::Cyan,
                        super::DatabaseConnectionColor::Cyan => super::DatabaseConnectionColor::Gray,
                        super::DatabaseConnectionColor::Gray => super::DatabaseConnectionColor::Blue,
                    }
                } else {
                    match settings.default_connection_color {
                        super::DatabaseConnectionColor::Blue => super::DatabaseConnectionColor::Gray,
                        super::DatabaseConnectionColor::Green => super::DatabaseConnectionColor::Blue,
                        super::DatabaseConnectionColor::Yellow => super::DatabaseConnectionColor::Green,
                        super::DatabaseConnectionColor::Orange => super::DatabaseConnectionColor::Yellow,
                        super::DatabaseConnectionColor::Red => super::DatabaseConnectionColor::Orange,
                        super::DatabaseConnectionColor::Purple => super::DatabaseConnectionColor::Red,
                        super::DatabaseConnectionColor::Cyan => super::DatabaseConnectionColor::Purple,
                        super::DatabaseConnectionColor::Gray => super::DatabaseConnectionColor::Cyan,
                    }
                };
            }
            _ => return false,
        }
        settings.normalize();
        true
    }
}

fn adjust_u64(value: &mut u64, delta: i32, step: u64) {
    if delta >= 0 {
        *value = value.saturating_add(step.saturating_mul(delta as u64));
    } else {
        *value = value.saturating_sub(step.saturating_mul(delta.unsigned_abs() as u64));
    }
}

fn adjust_usize(value: &mut usize, delta: i32, step: usize) {
    if delta >= 0 {
        *value = value.saturating_add(step.saturating_mul(delta as usize));
    } else {
        *value = value.saturating_sub(step.saturating_mul(delta.unsigned_abs() as usize));
    }
}

#[cfg(test)]
mod tests {
    use super::{adjust_u64, adjust_usize, sanitize_history_sql};

    #[test]
    fn setting_adjusters_saturate() {
        let mut value = 1u64;
        adjust_u64(&mut value, -2, 10);
        assert_eq!(value, 0);
        let mut value = 2usize;
        adjust_usize(&mut value, 3, 4);
        assert_eq!(value, 14);
    }

    #[test]
    fn query_history_entry_is_sanitized_before_persistence() {
        let clean = sanitize_history_sql("ALTER ROLE x PASSWORD 'secret'");
        assert!(!clean.contains("secret"));
    }
}
