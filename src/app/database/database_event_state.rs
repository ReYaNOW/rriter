use super::{
    DatabaseConnectionId, DatabaseEvent, DatabaseQueryCompletionMetadata,
    DatabaseQueryDiagnostic, DatabaseQueryHistoryEntry, DatabaseQueryReviewState,
    DatabasePendingJobKind, DatabaseQueryTabState, DatabaseTableTabState,
    DatabaseTransactionId, SqlConsoleId,
};

impl DatabaseQueryTabState {
    pub(crate) fn apply_query_completion_event(
        &mut self,
        metadata: DatabaseQueryCompletionMetadata,
    ) {
        self.completion = metadata;
        self.completion_loaded = true;
        self.analysis_editor_version = None;
        self.error = None;
    }

    pub(crate) fn apply_query_prepared_event(
        &mut self,
        event: DatabaseEvent,
        editor_text: &str,
        line_offsets: &[usize],
    ) -> Option<DatabaseQueryHistoryEntry> {
        let (
            connection_id,
            transaction_id,
            database_name,
            console_id,
            sql,
            source_offset,
            started_unix_ms,
            result_sets,
            messages,
            deadline_unix_ms,
            duration_ms,
            returned_rows,
            changed_rows,
            requires_review,
            mode,
        ) = match event {
            DatabaseEvent::QueryTransactionPrepared {
                connection_id,
                transaction_id,
                database_name,
                console_id,
                sql,
                source_offset,
                started_unix_ms,
                result_sets,
                messages,
                deadline_unix_ms,
                duration_ms,
                returned_rows,
                changed_rows,
                requires_review,
                mode,
                ..
            } => (
                connection_id,
                transaction_id,
                database_name,
                console_id,
                sql,
                source_offset,
                started_unix_ms,
                result_sets,
                messages,
                deadline_unix_ms,
                duration_ms,
                returned_rows,
                changed_rows,
                requires_review,
                mode,
            ),
            _ => return None,
        };
        self.running = false;
        self.running_sql = None;
        self.running_started_unix_ms = 0;
        self.error = None;
        self.diagnostic = None;
        self.diagnostic_editor_version = None;
        self.editor_diagnostics =
            crate::app::database::database_query_editor_diagnostics(
                &self.analysis,
                None,
                editor_text,
                line_offsets,
            );
        self.results = result_sets;
        self.messages = messages;
        self.result_view.invalidate_review_message_layout();
        self.result_view.active_result = 0;
        self.result_view.reset_scroll();
        self.last_duration_ms = duration_ms;
        self.last_returned_rows = returned_rows;
        self.last_changed_rows = changed_rows;
        if requires_review {
            self.review = Some(DatabaseQueryReviewState {
                transaction_id,
                sql,
                source_offset,
                started_unix_ms,
                deadline_unix_ms,
                duration_ms,
                returned_rows,
                changed_rows,
                mode,
                finishing: false,
            });
            None
        } else {
            self.review = None;
            Some(DatabaseQueryHistoryEntry {
                connection_id,
                database_name,
                console_id,
                sql,
                started_unix_ms,
                duration_ms,
                succeeded: true,
                returned_rows,
                affected_rows: changed_rows,
                error_summary: None,
            })
        }
    }

    pub(crate) fn apply_query_committed_event(
        &mut self,
        connection_id: DatabaseConnectionId,
        database_name: String,
        console_id: SqlConsoleId,
    ) -> (Option<DatabaseQueryHistoryEntry>, bool) {
        let history = self.review.take().map(|review| {
            let refresh_metadata = crate::languages::sql::scan_statements(&review.sql)
                .iter()
                .any(|statement| {
                    matches!(
                        statement.kind,
                        crate::languages::sql::SqlStatementKind::Definition
                    )
                });
            if refresh_metadata {
                self.completion_loaded = false;
            }
            (refresh_metadata, DatabaseQueryHistoryEntry {
                connection_id,
                database_name,
                console_id,
                sql: review.sql,
                started_unix_ms: review.started_unix_ms,
                duration_ms: review.duration_ms,
                succeeded: true,
                returned_rows: review.returned_rows,
                affected_rows: review.changed_rows,
                error_summary: None,
            })
        });
        self.running = false;
        match history {
            Some((refresh_metadata, history)) => (Some(history), refresh_metadata),
            None => (None, false),
        }
    }

    pub(crate) fn apply_query_rolled_back_event(
        &mut self,
        connection_id: DatabaseConnectionId,
        database_name: String,
        console_id: SqlConsoleId,
    ) -> Option<DatabaseQueryHistoryEntry> {
        let history = self.review.take().map(|review| DatabaseQueryHistoryEntry {
            connection_id,
            database_name,
            console_id,
            sql: review.sql,
            started_unix_ms: review.started_unix_ms,
            duration_ms: review.duration_ms,
            succeeded: false,
            returned_rows: review.returned_rows,
            affected_rows: review.changed_rows,
            error_summary: Some("Транзакция отменена пользователем".to_string()),
        });
        self.running = false;
        history
    }

    pub(crate) fn apply_query_expired_event(
        &mut self,
        transaction_id: DatabaseTransactionId,
        connection_id: DatabaseConnectionId,
        database_name: String,
        console_id: SqlConsoleId,
    ) -> Option<DatabaseQueryHistoryEntry> {
        let matches = self.review.as_ref().is_some_and(|review| {
            review.transaction_id == transaction_id
        });
        let history = if matches {
            self.review.take().map(|review| DatabaseQueryHistoryEntry {
                connection_id,
                database_name,
                console_id,
                sql: review.sql,
                started_unix_ms: review.started_unix_ms,
                duration_ms: review.duration_ms,
                succeeded: false,
                returned_rows: review.returned_rows,
                affected_rows: review.changed_rows,
                error_summary: Some("Транзакция автоматически отменена по таймауту".to_string()),
            })
        } else {
            None
        };
        if history.is_some() {
            self.running = false;
            self.error = Some("Транзакция SQL-консоли автоматически отменена по таймауту".to_string());
        }
        history
    }

    pub(crate) fn apply_query_expiry_failed_event(
        &mut self,
        transaction_id: DatabaseTransactionId,
        connection_id: DatabaseConnectionId,
        database_name: String,
        console_id: SqlConsoleId,
        message: String,
    ) -> Option<DatabaseQueryHistoryEntry> {
        let matches = self.review.as_ref().is_some_and(|review| {
            review.transaction_id == transaction_id
        });
        if matches {
            self.review.take().map(|review| {
                let error = format!("Не удалось автоматически отменить транзакцию: {message}");
                self.running = false;
                self.error = Some(error.clone());
                DatabaseQueryHistoryEntry {
                    connection_id,
                    database_name,
                    console_id,
                    sql: review.sql,
                    started_unix_ms: review.started_unix_ms,
                    duration_ms: review.duration_ms,
                    succeeded: false,
                    returned_rows: review.returned_rows,
                    affected_rows: review.changed_rows,
                    error_summary: Some(error),
                }
            })
        } else {
            None
        }
    }

    pub(crate) fn apply_query_failed_event(
        &mut self,
        message: &str,
        diagnostic: Option<DatabaseQueryDiagnostic>,
        diagnostic_editor_version: Option<u64>,
        editor_text: &str,
        line_offsets: &[usize],
    ) -> Vec<(usize, usize)> {
        let syntax_errors = diagnostic
            .as_ref()
            .map(|diagnostic| vec![(diagnostic.start_byte, diagnostic.end_byte)])
            .unwrap_or_default();
        self.running = false;
        self.running_sql = None;
        self.running_started_unix_ms = 0;
        self.error = Some(message.to_string());
        self.messages.clear();
        self.result_view.invalidate_review_message_layout();
        self.diagnostic = diagnostic;
        self.diagnostic_editor_version = diagnostic_editor_version;
        self.editor_diagnostics = crate::app::database::database_query_editor_diagnostics(
            &self.analysis,
            self.diagnostic.as_ref(),
            editor_text,
            line_offsets,
        );
        self.review = None;
        self.result_view.active_result = self.results.len();
        self.result_view.reset_scroll();
        syntax_errors
    }

    pub(crate) fn apply_query_job_failed_event(&mut self, message: &str) {
        self.running = false;
        self.error = Some(message.to_string());
    }
}

impl DatabaseTableTabState {
    pub(crate) fn apply_metadata_loaded_event(
        &mut self,
        metadata: super::DatabaseTableMetadata,
    ) {
        self.metadata = Some(metadata);
        self.loading = false;
        self.error = None;
        self.clear_unavailable_selection();
    }

    pub(crate) fn apply_table_transaction_expired_event(&mut self) {
        self.grid.pending_close_after_save = false;
    }

    pub(crate) fn apply_table_transaction_expiry_failed_event(&mut self, message: &str) {
        self.grid.pending_close_after_save = false;
        self.error = Some(format!(
            "Не удалось автоматически отменить транзакцию: {message}"
        ));
    }

    pub(crate) fn apply_table_job_failed_event(
        &mut self,
        kind: DatabasePendingJobKind,
        message: &str,
    ) -> (bool, bool) {
        match kind {
            DatabasePendingJobKind::CountRows => {
                let filter_target = self.grid.pending_filter_error_target(false);
                self.grid.loading_count = false;
                self.grid.finish_refresh();
                self.grid.count_error = Some(message.to_string());
                if self.grid.post_commit_refresh_pending {
                    self.error = Some(format!(
                        "Изменения успешно применены, но обновить данные не удалось: {message}"
                    ));
                    self.grid.post_commit_refresh_pending = false;
                } else if let Some(target) = filter_target {
                    self.grid.filter_error = Some((target, message.to_string()));
                    self.error = None;
                } else {
                    self.error = Some(message.to_string());
                }
                self.grid.abort_pending_view();
                (true, false)
            }
            DatabasePendingJobKind::LoadChunk => {
                let filter_target = self.grid.pending_filter_error_target(true);
                self.grid.loading_chunk = false;
                self.grid.finish_refresh();
                self.grid.in_flight_chunk = None;
                self.grid.desired_chunk = None;
                if self.grid.post_commit_refresh_pending {
                    self.error = Some(format!(
                        "Изменения успешно применены, но обновить данные не удалось: {message}"
                    ));
                    self.grid.post_commit_refresh_pending = false;
                } else if let Some(target) = filter_target {
                    self.grid.filter_error = Some((target, message.to_string()));
                    self.error = None;
                } else {
                    self.error = Some(message.to_string());
                }
                self.grid.abort_pending_view();
                (true, false)
            }
            DatabasePendingJobKind::BeginTableSave => {
                self.grid.pending_close_after_save = false;
                self.error = Some(message.to_string());
                (true, true)
            }
            DatabasePendingJobKind::CommitTransaction
            | DatabasePendingJobKind::RollbackTransaction => {
                self.grid.pending_close_after_save = false;
                self.error = Some(message.to_string());
                (true, false)
            }
            DatabasePendingJobKind::LoadMetadata => {
                self.loading = false;
                self.error = Some(message.to_string());
                self.set_unavailable_text(message);
                (true, false)
            }
            _ => (false, false),
        }
    }

    pub(crate) fn apply_table_job_cancelled_event(
        &mut self,
        kind: DatabasePendingJobKind,
    ) -> bool {
        match kind {
            DatabasePendingJobKind::CountRows => {
                self.grid.loading_count = false;
                self.grid.finish_refresh();
                self.grid.abort_pending_view();
            }
            DatabasePendingJobKind::LoadChunk => {
                self.grid.loading_chunk = false;
                self.grid.finish_refresh();
                self.grid.in_flight_chunk = None;
                self.grid.abort_pending_view();
            }
            DatabasePendingJobKind::BeginTableSave => {
                self.grid.pending_close_after_save = false;
                return true;
            }
            DatabasePendingJobKind::CommitTransaction
            | DatabasePendingJobKind::RollbackTransaction => {
                self.grid.pending_close_after_save = false;
            }
            _ => {}
        }
        false
    }

    pub(crate) fn apply_table_busy_event(&mut self, message: &str) {
        self.grid.loading_count = false;
        self.grid.loading_chunk = false;
        self.grid.in_flight_chunk = None;
        self.grid.desired_chunk = None;
        self.grid.finish_refresh();
        self.grid.abort_pending_view();
        self.error = None;
        self.show_timed_notice(message.to_string());
    }
}
