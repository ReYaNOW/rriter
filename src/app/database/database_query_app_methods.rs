use crate::app::database::{
    DatabaseQueryHistoryEntry, DatabaseQueryMode, SqlConsoleId, completion_words,
    format_database_sql, history_started_now, query_execution_target, sanitize_history_sql,
};

impl App {
    pub(crate) fn clear_stale_active_database_query_diagnostic(&mut self) -> bool {
        let editor_version = self.editor.version;
        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return false;
        };
        let EditorTabKind::DatabaseQuery(_, state) = &mut tab.kind else {
            return false;
        };
        if !database_query_diagnostic_is_stale(
            state.diagnostic_editor_version,
            editor_version,
        ) {
            return false;
        }
        state.diagnostic = None;
        state.diagnostic_editor_version = None;
        state.error = None;
        tab.syntax_errors.clear();
        self.highlighter.syntax_errors.clear();
        true
    }

    fn active_database_query_meta_state(
        &self,
    ) -> Option<(&DatabaseQueryTabMeta, &DatabaseQueryTabState)> {
        self.tabs.get(self.active_tab).and_then(|tab| match &tab.kind {
            EditorTabKind::DatabaseQuery(meta, state) => Some((meta, state)),
            _ => None,
        })
    }

    fn database_query_tab_index(
        &self,
        connection_id: DatabaseConnectionId,
        console_id: SqlConsoleId,
    ) -> Option<usize> {
        self.tabs.iter().position(|tab| match &tab.kind {
            EditorTabKind::DatabaseQuery(meta, _) => {
                meta.connection_id == connection_id && meta.console_id == console_id
            }
            _ => false,
        })
    }

    pub(crate) fn request_active_database_query_completion(&mut self) {
        let Some((meta, state)) = self.active_database_query_meta_state() else {
            return;
        };
        if state.completion_loaded || self.ide_panel.database.pending_job.is_some() {
            return;
        }
        let meta = meta.clone();
        let Some(connection) = self
            .ide_panel
            .database
            .connection(meta.connection_id)
            .map(|node| node.config.clone())
        else {
            return;
        };
        let job_id = self.ide_panel.database.allocate_job_id();
        let pending = DatabasePendingJob {
            id: job_id,
            kind: DatabasePendingJobKind::LoadQueryCompletion,
            connection_id: meta.connection_id,
            database_name: Some(meta.database_name.clone()),
            table_name: None,
        };
        let settings = self.ide_panel.database.settings().clone();
        let secrets = self.connection_job_secrets(meta.connection_id);
        self.send_database_command(
            DatabaseCommand::LoadQueryCompletion {
                job_id,
                connection,
                database_name: meta.database_name,
                console_id: meta.console_id,
                secrets,
                settings,
                ssh_options: crate::app::database::host_key_options(
                    crate::app::database::SshHostKeyPolicy::Strict,
                ),
            },
            pending,
        );
    }

    pub fn run_active_database_query(&mut self, mode: DatabaseQueryMode) {
        if self.ide_panel.database.pending_job.is_some() {
            self.ide_panel.database.global_error = Some(
                "Сейчас уже выполняется запрос к базе данных. Отмените его или дождитесь завершения."
                    .to_string(),
            );
            return;
        }
        let Some((meta, state)) = self.active_database_query_meta_state() else {
            return;
        };
        if state.review.is_some() {
            self.ide_panel.database.global_error = Some(
                "Сначала примените или отмените текущую транзакцию SQL-консоли".to_string(),
            );
            return;
        }
        let meta = meta.clone();
        let text = self.editor.get_full_text();
        let selection = self
            .editor
            .selection_anchor
            .filter(|anchor| *anchor != self.editor.cursor)
            .map(|anchor| {
                let start = anchor.min(self.editor.cursor);
                let end = anchor.max(self.editor.cursor);
                (start, end)
            });
        let Some((sql, source_offset)) =
            query_execution_target(&text, selection, self.editor.cursor)
        else {
            self.ide_panel.database.global_error = Some("SQL-запрос пуст".to_string());
            return;
        };
        let Some(connection) = self
            .ide_panel
            .database
            .connection(meta.connection_id)
            .map(|node| node.config.clone())
        else {
            return;
        };
        let job_id = self.ide_panel.database.allocate_job_id();
        let pending = DatabasePendingJob {
            id: job_id,
            kind: DatabasePendingJobKind::RunUserSql,
            connection_id: meta.connection_id,
            database_name: Some(meta.database_name.clone()),
            table_name: None,
        };
        let settings = self.ide_panel.database.settings().clone();
        let secrets = self.connection_job_secrets(meta.connection_id);
        if let Some(index) = self.database_query_tab_index(meta.connection_id, meta.console_id)
            && let EditorTabKind::DatabaseQuery(_, state) = &mut self.tabs[index].kind
        {
            state.mark_running(sql.clone(), history_started_now());
        }
        self.save_active_database_query();
        self.ide_panel.database.pending_query_mode = Some(mode);
        self.send_database_command(
            DatabaseCommand::RunUserSql {
                job_id,
                connection,
                database_name: meta.database_name,
                console_id: meta.console_id,
                sql,
                source_offset,
                mode,
                secrets,
                settings,
                ssh_options: crate::app::database::host_key_options(
                    crate::app::database::SshHostKeyPolicy::Strict,
                ),
            },
            pending,
        );
    }

    pub(crate) fn request_database_query_close(&mut self, index: usize) -> bool {
        let Some(EditorTabKind::DatabaseQuery(_, state)) =
            self.tabs.get(index).map(|tab| &tab.kind)
        else {
            return false;
        };
        if state.review.is_some() {
            self.ide_panel.database.global_error = Some(
                "Сначала примените или отмените неподтверждённую транзакцию SQL-консоли"
                    .to_string(),
            );
            return true;
        }
        if state.running {
            self.ide_panel.database.global_error = Some(
                "Сначала отмените выполняющийся запрос SQL-консоли".to_string(),
            );
            return true;
        }
        false
    }

    pub fn cancel_active_database_query(&mut self) {
        let Some((_, state)) = self.active_database_query_meta_state() else {
            return;
        };
        if state.review.is_some() {
            self.rollback_active_database_query();
        } else {
            self.cancel_database_job();
        }
    }

    pub fn commit_active_database_query(&mut self) {
        self.finish_active_database_query(true);
    }

    pub fn rollback_active_database_query(&mut self) {
        self.finish_active_database_query(false);
    }

    fn finish_active_database_query(&mut self, commit: bool) {
        let Some((meta, state)) = self.active_database_query_meta_state() else {
            return;
        };
        let Some(review) = state.review.as_ref() else {
            return;
        };
        let meta = meta.clone();
        let transaction_id = review.transaction_id;
        let job_id = self.ide_panel.database.allocate_job_id();
        let kind = if commit {
            DatabasePendingJobKind::CommitTransaction
        } else {
            DatabasePendingJobKind::RollbackTransaction
        };
        let pending = DatabasePendingJob {
            id: job_id,
            kind,
            connection_id: meta.connection_id,
            database_name: Some(meta.database_name),
            table_name: None,
        };
        let command = if commit {
            DatabaseCommand::CommitTransaction { job_id, transaction_id }
        } else {
            DatabaseCommand::RollbackTransaction { job_id, transaction_id }
        };
        self.send_database_command(command, pending);
    }

    pub fn format_active_database_query(&mut self) {
        if !self.active_tab_is_database_query() {
            return;
        }
        let text = self.editor.get_full_text();
        match format_database_sql(&text) {
            Ok(formatted) if formatted != text => {
                let len = self.editor.len();
                self.editor.replace_range(0, len, &formatted);
                self.reset_highlighter_with_text(self.editor.get_full_text(), false);
                self.save_active_database_query();
            }
            Ok(_) => {}
            Err(error) => self.ide_panel.database.global_error = Some(error),
        }
    }

    pub fn toggle_active_database_query_history(&mut self) {
        let Some((meta, _)) = self.active_database_query_meta_state() else {
            return;
        };
        let key = (meta.connection_id, meta.console_id);
        if let Some(index) = self.database_query_tab_index(key.0, key.1)
            && let EditorTabKind::DatabaseQuery(_, state) = &mut self.tabs[index].kind
        {
            state.history_open = !state.history_open;
            state.history_selected = 0;
            state.result_view.scroll_x = 0;
            state.result_view.scroll_y = 0;
        }
    }

    pub fn select_active_database_query_result(&mut self, index: usize) {
        let Some((meta, _)) = self.active_database_query_meta_state() else {
            return;
        };
        if let Some(tab_index) = self.database_query_tab_index(meta.connection_id, meta.console_id)
            && let EditorTabKind::DatabaseQuery(_, state) = &mut self.tabs[tab_index].kind
        {
            state.result_view.active_result = index.min(state.results.len());
            state.result_view.scroll_x = 0;
            state.result_view.scroll_y = 0;
        }
    }

    pub fn load_database_query_history_entry(&mut self, visible_index: usize) {
        let Some((meta, _)) = self.active_database_query_meta_state() else {
            return;
        };
        let connection_id = meta.connection_id;
        let console_id = meta.console_id;
        let database_name = meta.database_name.clone();
        let entry = self
            .ide_panel
            .database
            .persisted
            .query_history
            .iter()
            .rev()
            .filter(|entry| {
                entry.connection_id == connection_id && entry.database_name == database_name
            })
            .nth(visible_index)
            .cloned();
        let Some(entry) = entry else { return; };
        let len = self.editor.len();
        self.editor.replace_range(0, len, &entry.sql);
        let full_text = self.editor.get_full_text();
        self.reset_highlighter_with_text(full_text, false);
        if let Some(tab_index) = self.database_query_tab_index(connection_id, console_id)
            && let EditorTabKind::DatabaseQuery(_, state) = &mut self.tabs[tab_index].kind
        {
            state.history_open = false;
        }
        self.save_active_database_query();
    }

    pub fn show_active_database_query_completion(&mut self) {
        let Some((_, state)) = self.active_database_query_meta_state() else {
            return;
        };
        if !state.completion_loaded {
            self.request_active_database_query_completion();
            return;
        }
        let words = completion_words(
            &state.completion,
            &self.editor.get_full_text(),
            self.editor.cursor,
        );
        self.autocomplete_options = words
            .into_iter()
            .map(|(word, detail)| {
                let kind = match detail.as_str() {
                    "table" => SymbolKind::Class,
                    "function" => SymbolKind::Function,
                    "operator" | "PostgreSQL" => SymbolKind::Keyword,
                    "enum" => SymbolKind::Builtin,
                    _ => SymbolKind::Property,
                };
                (
                    AutocompleteItem {
                        word,
                        kind,
                        scope_start: 0,
                        scope_end: usize::MAX,
                        module: None,
                        module_path: None,
                        detail: Some(detail),
                        insert_text: None,
                        text_edit: None,
                        additional_text_edits: Vec::new(),
                    },
                    Vec::new(),
                )
            })
            .collect();
        self.autocomplete_selected_idx = 0;
        self.autocomplete_mode = AutocompleteMode::Sql;
        self.autocomplete_active = !self.autocomplete_options.is_empty();
    }

    pub(crate) fn record_database_query_history(
        &mut self,
        mut entry: DatabaseQueryHistoryEntry,
    ) {
        entry.sql = sanitize_history_sql(&entry.sql);
        if let Some(error) = entry.error_summary.as_mut() {
            *error = sanitize_history_sql(error);
        }
        entry.normalize();
        let limit = self
            .ide_panel
            .database
            .settings()
            .sql_history_limit
            .min(crate::app::database::MAX_SQL_HISTORY_ENTRIES);
        let history = &mut self.ide_panel.database.persisted.query_history;
        history.push(entry);
        if history.len() > limit {
            let remove = history.len() - limit;
            history.drain(0..remove);
        }
        while history
            .iter()
            .map(|entry| {
                entry.sql.len()
                    + entry.database_name.len()
                    + entry.error_summary.as_ref().map_or(0, String::len)
                    + 64
            })
            .sum::<usize>()
            > crate::app::database::MAX_SQL_HISTORY_BYTES
        {
            if history.is_empty() {
                break;
            }
            history.remove(0);
        }
        self.save_database_panel_state();
    }

    pub(crate) fn adjust_database_setting(&mut self, setting: usize, delta: i32) {
        let settings = &mut self.ide_panel.database.persisted.settings;
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
                        crate::app::database::DatabaseConnectionColor::Blue => crate::app::database::DatabaseConnectionColor::Green,
                        crate::app::database::DatabaseConnectionColor::Green => crate::app::database::DatabaseConnectionColor::Yellow,
                        crate::app::database::DatabaseConnectionColor::Yellow => crate::app::database::DatabaseConnectionColor::Orange,
                        crate::app::database::DatabaseConnectionColor::Orange => crate::app::database::DatabaseConnectionColor::Red,
                        crate::app::database::DatabaseConnectionColor::Red => crate::app::database::DatabaseConnectionColor::Purple,
                        crate::app::database::DatabaseConnectionColor::Purple => crate::app::database::DatabaseConnectionColor::Cyan,
                        crate::app::database::DatabaseConnectionColor::Cyan => crate::app::database::DatabaseConnectionColor::Gray,
                        crate::app::database::DatabaseConnectionColor::Gray => crate::app::database::DatabaseConnectionColor::Blue,
                    }
                } else {
                    match settings.default_connection_color {
                        crate::app::database::DatabaseConnectionColor::Blue => crate::app::database::DatabaseConnectionColor::Gray,
                        crate::app::database::DatabaseConnectionColor::Green => crate::app::database::DatabaseConnectionColor::Blue,
                        crate::app::database::DatabaseConnectionColor::Yellow => crate::app::database::DatabaseConnectionColor::Green,
                        crate::app::database::DatabaseConnectionColor::Orange => crate::app::database::DatabaseConnectionColor::Yellow,
                        crate::app::database::DatabaseConnectionColor::Red => crate::app::database::DatabaseConnectionColor::Orange,
                        crate::app::database::DatabaseConnectionColor::Purple => crate::app::database::DatabaseConnectionColor::Red,
                        crate::app::database::DatabaseConnectionColor::Cyan => crate::app::database::DatabaseConnectionColor::Purple,
                        crate::app::database::DatabaseConnectionColor::Gray => crate::app::database::DatabaseConnectionColor::Cyan,
                    }
                };
            }
            _ => return,
        }
        settings.normalize();
        self.save_database_panel_state();
    }
}

fn database_query_diagnostic_is_stale(
    diagnostic_version: Option<u64>,
    editor_version: u64,
) -> bool {
    diagnostic_version.is_some_and(|version| version != editor_version)
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
mod database_query_app_method_tests {
    use super::*;

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

    #[test]
    fn server_diagnostic_becomes_stale_after_document_edit() {
        assert!(!database_query_diagnostic_is_stale(None, 2));
        assert!(!database_query_diagnostic_is_stale(Some(2), 2));
        assert!(database_query_diagnostic_is_stale(Some(2), 3));
    }
}
