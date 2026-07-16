#[cfg_attr(coverage_nightly, coverage(off))]
impl App {
    pub fn cancel_database_job(&mut self) {
        let Some(pending) = self.ide_panel.database.pending_job.as_ref() else {
            return;
        };
        if let Some(runtime) = self.database_runtime.as_ref() {
            let _ = runtime.send(DatabaseCommand::CancelJob { job_id: pending.id });
        }
    }

    pub fn resolve_database_host_key(&mut self, policy: SshHostKeyPolicy) {
        let Some(prompt) = self.ide_panel.database.host_key_prompt.take() else {
            return;
        };
        let Some(pending) = self.ide_panel.database.pending_job.clone() else {
            return;
        };
        if prompt.job_id != pending.id {
            return;
        }
        self.ide_panel.database.pending_job = None;
        match pending.kind {
            DatabasePendingJobKind::TestConnection => self.test_database_dialog_connection_with_policy(policy),
            DatabasePendingJobKind::LoadDatabases => self.load_connection_databases(pending.connection_id, policy),
            DatabasePendingJobKind::LoadTables => {
                if let Some(database_name) = pending.database_name {
                    self.load_public_database_tables(pending.connection_id, &database_name, policy);
                }
            }
            DatabasePendingJobKind::LoadMetadata => {
                if let (Some(database_name), Some(table_name)) = (pending.database_name, pending.table_name) {
                    let meta = DatabaseTableTabMeta {
                        tab_id: crate::app::database::DatabaseTabId(0),
                        connection_id: pending.connection_id,
                        database_name,
                        table_name,
                    };
                    self.load_database_table_metadata(&meta, policy);
                }
            }
            DatabasePendingJobKind::LoadDdl => {
                if let (Some(database_name), Some(table_name)) = (pending.database_name, pending.table_name) {
                    self.load_database_ddl(pending.connection_id, &database_name, &table_name, policy);
                }
            }
            DatabasePendingJobKind::CountRows
            | DatabasePendingJobKind::LoadChunk
            | DatabasePendingJobKind::BeginTableSave => {
                self.ide_panel.database.host_key_policy_override = Some(policy);
                let tab_id = self.tabs.iter().find_map(|tab| match &tab.kind {
                    EditorTabKind::DatabaseTable(meta, _)
                        if meta.connection_id == pending.connection_id
                            && pending.database_name.as_deref() == Some(meta.database_name.as_str())
                            && pending.table_name.as_deref() == Some(meta.table_name.as_str()) => Some(meta.tab_id),
                    _ => None,
                });
                if let Some(tab_id) = tab_id {
                    if pending.kind == DatabasePendingJobKind::BeginTableSave {
                        let close_after = self.database_table_meta_state(tab_id)
                            .is_some_and(|(_, state)| state.grid.pending_close_after_save);
                        self.save_database_table_changes(tab_id, close_after);
                    } else {
                        self.queue_database_table_initial_load(tab_id);
                    }
                }
            }
            DatabasePendingJobKind::LoadQueryCompletion => {
                self.ide_panel.database.host_key_policy_override = Some(policy);
                self.request_active_database_query_completion();
            }
            DatabasePendingJobKind::RunUserSql => {
                let mode = self.ide_panel.database.pending_query_mode.take().unwrap_or_default();
                self.ide_panel.database.host_key_policy_override = Some(policy);
                self.run_active_database_query(mode);
            }
            DatabasePendingJobKind::CommitTransaction
            | DatabasePendingJobKind::RollbackTransaction
            | DatabasePendingJobKind::SaveConnection
            | DatabasePendingJobKind::DeleteConnection => {}
        }
    }

    fn test_database_dialog_connection_with_policy(&mut self, policy: SshHostKeyPolicy) {
        let (connection, secrets) = {
            let Some(dialog) = self.ide_panel.database.dialog.as_ref() else {
                return;
            };
            let fallback_id = dialog.editing_connection_id.unwrap_or(DatabaseConnectionId(self.ide_panel.database.next_connection_id));
            let Ok(connection) = dialog.build_config(fallback_id) else {
                return;
            };
            (connection, dialog.secret_bundle())
        };
        let job_id = self.ide_panel.database.allocate_job_id();
        let pending = DatabasePendingJob { id: job_id, kind: DatabasePendingJobKind::TestConnection, connection_id: connection.id, database_name: None, table_name: None };
        let settings = self.ide_panel.database.settings().clone();
        self.send_database_command(DatabaseCommand::TestConnection {
            job_id,
            connection,
            secrets: Some(secrets),
            settings,
            ssh_options: crate::app::database::host_key_options(policy),
        }, pending);
    }

    fn apply_database_event(&mut self, event: DatabaseEvent) {
        let event_job_id = match &event {
            DatabaseEvent::ConnectionTested { job_id, .. }
            | DatabaseEvent::DatabasesLoaded { job_id, .. }
            | DatabaseEvent::PublicTablesLoaded { job_id, .. }
            | DatabaseEvent::MetadataLoaded { job_id, .. }
            | DatabaseEvent::DdlLoaded { job_id, .. }
            | DatabaseEvent::TableCountLoaded { job_id, .. }
            | DatabaseEvent::TableChunkLoaded { job_id, .. }
            | DatabaseEvent::QueryCompletionLoaded { job_id, .. }
            | DatabaseEvent::QueryTransactionPrepared { job_id, .. }
            | DatabaseEvent::QueryTransactionCommitted { job_id, .. }
            | DatabaseEvent::QueryTransactionRolledBack { job_id, .. }
            | DatabaseEvent::QueryFailed { job_id, .. }
            | DatabaseEvent::TransactionPrepared { job_id, .. }
            | DatabaseEvent::TransactionCommitted { job_id, .. }
            | DatabaseEvent::TransactionRolledBack { job_id, .. }
            | DatabaseEvent::ConnectionSecretsSaved { job_id, .. }
            | DatabaseEvent::ConnectionSecretsDeleted { job_id, .. }
            | DatabaseEvent::HostKeyConfirmationRequired { job_id, .. }
            | DatabaseEvent::JobFailed { job_id, .. }
            | DatabaseEvent::JobCancelled { job_id } => Some(*job_id),
            DatabaseEvent::Busy { requested_job_id, .. } => Some(*requested_job_id),
            DatabaseEvent::TransactionExpired { .. }
            | DatabaseEvent::QueryTransactionExpired { .. } => None,
        };
        let pending = self.ide_panel.database.pending_job.clone();
        if let (Some(event_job_id), Some(pending)) = (event_job_id, pending.as_ref())
            && event_job_id != pending.id
        {
            return;
        }

        match event {
            DatabaseEvent::ConnectionTested { result, .. } => {
                if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
                    dialog.test_status = Some(format!(
                        "Подключено: {} · {}",
                        result.server_version, result.current_database
                    ));
                }
                if let Some(pending) = pending.as_ref()
                    && let Some(node) = self.ide_panel.database.connection_mut(pending.connection_id)
                {
                    apply_connection_notices(node, result.ssh_backend, &result.notices);
                    node.status = connection_status(result.ssh_backend);
                }
                self.ide_panel.database.pending_job = None;
            }
            DatabaseEvent::DatabasesLoaded { result, .. } => {
                let expanded = self.ide_panel.database.persisted.expanded_databases.clone();
                if let Some(pending) = pending.as_ref()
                    && let Some(node) = self.ide_panel.database.connection_mut(pending.connection_id)
                {
                    node.loading = false;
                    node.databases_loaded = true;
                    node.databases = result
                        .databases
                        .into_iter()
                        .map(|database| {
                            let mut node = DatabaseDatabaseNode::new(database.name);
                            node.expanded = expanded.contains(&(pending.connection_id, node.name.clone()));
                            node
                        })
                        .collect();
                    node.status = connection_status(result.ssh_backend);
                    apply_connection_notices(node, result.ssh_backend, &result.notices);
                }
                self.ide_panel.database.pending_job = None;
            }
            DatabaseEvent::PublicTablesLoaded { database_name, result, .. } => {
                let connection_id = pending.as_ref().map(|pending| pending.connection_id);
                if let Some(connection_id) = connection_id
                    && let Some(node) = self.ide_panel.database.connection_mut(connection_id)
                {
                    if let Some(database) = node.databases.iter_mut().find(|db| db.name == database_name) {
                        database.loading = false;
                        database.tables_loaded = true;
                        database.tables = result.tables;
                        database.error = None;
                    }
                    node.status = connection_status(result.ssh_backend);
                    apply_connection_notices(node, result.ssh_backend, &result.notices);
                }
                self.ide_panel.database.pending_job = None;
                let should_refresh_completion = connection_id.is_some_and(|connection_id| {
                    self.tabs.get(self.active_tab).is_some_and(|tab| {
                        matches!(&tab.kind,
                            EditorTabKind::DatabaseQuery(meta, state)
                                if meta.connection_id == connection_id
                                    && meta.database_name == database_name
                                    && !state.completion_loaded)
                    })
                });
                if should_refresh_completion {
                    self.request_active_database_query_completion();
                }
            }
            DatabaseEvent::MetadataLoaded { connection_id, result, .. } => {
                let mut loaded_tabs = Vec::new();
                for tab in &mut self.tabs {
                    if let EditorTabKind::DatabaseTable(meta, state) = &mut tab.kind
                        && meta.connection_id == connection_id
                        && meta.database_name == result.database_name
                        && meta.table_name == result.table_name
                    {
                        state.metadata = Some(result.clone());
                        state.loading = false;
                        state.error = None;
                        state.clear_unavailable_selection();
                        loaded_tabs.push(meta.tab_id);
                    }
                }
                self.ide_panel.database.pending_job = None;
                for tab_id in loaded_tabs {
                    self.queue_database_table_initial_load(tab_id);
                }
            }
            DatabaseEvent::DdlLoaded { connection_id, result, .. } => {
                let ddl = format!(
                    "-- Реконструированный DDL: public.{}\n\n{}",
                    result.table_name, result.ddl
                );
                let line_count = ddl.lines().count().max(1);
                let spans = crate::highlighter::highlight_sql_text(&ddl);
                *self.ide_panel.database.ddl_hover.borrow_mut() = Some(DatabaseDdlHoverState {
                    connection_id,
                    database_name: result.database_name,
                    table_name: result.table_name,
                    popup: crate::app::mouse::HoverPopup {
                        text: ddl,
                        spans,
                        line_kinds: vec![crate::lsp::HoverLineKindPublic::Code; line_count],
                        inline_code_ranges: Vec::new(),
                        byte_offset: 0,
                        anchor_x: (self.window_width as f32 * 0.5).max(100.0),
                        anchor_y: 90.0,
                        offset_x: None,
                        offset_y: None,
                        anim_progress: 0.0,
                        scroll: ScrollState::new(15.0),
                        layout_cache: None,
                    },
                    rect: None,
                    max_scroll: 0.0,
                    selection_anchor: None,
                    selection_cursor: None,
                    selecting: false,
                });
                self.ide_panel.database.pending_job = None;
            }
            DatabaseEvent::TableCountLoaded { connection_id, result, .. } => {
                self.ide_panel.database.pending_job = None;
                self.on_database_table_count_loaded(connection_id, result);
            }
            DatabaseEvent::TableChunkLoaded { connection_id, result, .. } => {
                self.ide_panel.database.pending_job = None;
                self.on_database_table_chunk_loaded(connection_id, result);
            }
            DatabaseEvent::QueryCompletionLoaded { result, .. } => {
                let mut refresh_analysis = false;
                if let Some(index) = self.tabs.iter().position(|tab| match &tab.kind {
                    EditorTabKind::DatabaseQuery(meta, _) => {
                        meta.connection_id == result.connection_id
                            && meta.console_id == result.console_id
                            && meta.database_name == result.database_name
                    }
                    _ => false,
                }) && let EditorTabKind::DatabaseQuery(_, state) = &mut self.tabs[index].kind
                {
                    state.completion = result.metadata;
                    state.completion_loaded = true;
                    state.analysis_editor_version = None;
                    state.error = None;
                    refresh_analysis = index == self.active_tab;
                }
                self.ide_panel.database.pending_job = None;
                self.ide_panel.database.pending_query_mode = None;
                if refresh_analysis {
                    self.refresh_active_database_query_analysis();
                }
            }
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
            } => {
                let mut completed_history = None;
                if let Some(index) = self.tabs.iter().position(|tab| match &tab.kind {
                    EditorTabKind::DatabaseQuery(meta, _) => {
                        meta.connection_id == connection_id && meta.console_id == console_id
                    }
                    _ => false,
                }) {
                    let (editor_text, line_offsets) = if index == self.active_tab {
                        (self.editor.get_full_text(), self.editor.line_offsets.clone())
                    } else {
                        (
                            self.tabs[index].editor.get_full_text(),
                            self.tabs[index].editor.line_offsets.clone(),
                        )
                    };
                    if let EditorTabKind::DatabaseQuery(_, state) = &mut self.tabs[index].kind {
                        state.running = false;
                        state.running_sql = None;
                        state.running_started_unix_ms = 0;
                        state.error = None;
                        state.diagnostic = None;
                        state.diagnostic_editor_version = None;
                        state.editor_diagnostics =
                            crate::app::database::database_query_editor_diagnostics(
                                &state.analysis,
                                None,
                                &editor_text,
                                &line_offsets,
                            );
                        state.results = result_sets;
                        state.messages = messages;
                        state.result_view.active_result = 0;
                        state.result_view.reset_scroll();
                        state.last_duration_ms = duration_ms;
                        state.last_returned_rows = returned_rows;
                        state.last_changed_rows = changed_rows;
                        if requires_review {
                            state.review = Some(crate::app::database::DatabaseQueryReviewState {
                                transaction_id,
                                sql,
                                source_offset,
                                started_unix_ms,
                                deadline_unix_ms,
                                duration_ms,
                                returned_rows,
                                changed_rows,
                                mode,
                            });
                        } else {
                            completed_history = Some(
                                crate::app::database::DatabaseQueryHistoryEntry {
                                    connection_id,
                                    database_name: database_name.clone(),
                                    console_id,
                                    sql,
                                    started_unix_ms,
                                    duration_ms,
                                    succeeded: true,
                                    affected_rows: changed_rows,
                                    error_summary: None,
                                },
                            );
                            state.review = None;
                        }
                    }
                    self.tabs[index].syntax_errors.clear();
                    if index == self.active_tab {
                        self.highlighter.syntax_errors.clear();
                    }
                }
                self.ide_panel.database.pending_job = None;
                self.ide_panel.database.pending_query_mode = None;
                if requires_review {
                    self.ide_panel.is_resizing_left = false;
                    self.ide_panel.is_resizing_bottom = false;
                    self.ide_panel.git.graph_resizing = false;
                }
                if let Some(history) = completed_history {
                    self.record_database_query_history(history);
                }
            }
            DatabaseEvent::QueryTransactionCommitted {
                connection_id,
                database_name,
                console_id,
                ..
            } => {
                self.ide_panel.database.pending_job = None;
                let mut history = None;
                let mut refresh_metadata = false;
                if let Some(index) = self.tabs.iter().position(|tab| match &tab.kind {
                    EditorTabKind::DatabaseQuery(meta, _) => {
                        meta.connection_id == connection_id && meta.console_id == console_id
                    }
                    _ => false,
                }) && let EditorTabKind::DatabaseQuery(_, state) = &mut self.tabs[index].kind
                {
                    if let Some(review) = state.review.take() {
                        refresh_metadata = crate::languages::sql::scan_statements(&review.sql)
                            .iter()
                            .any(|statement| matches!(statement.kind, crate::languages::sql::SqlStatementKind::Definition));
                        history = Some(crate::app::database::DatabaseQueryHistoryEntry {
                            connection_id,
                            database_name: database_name.clone(),
                            console_id,
                            sql: review.sql,
                            started_unix_ms: review.started_unix_ms,
                            duration_ms: review.duration_ms,
                            succeeded: true,
                            affected_rows: review.changed_rows,
                            error_summary: None,
                        });
                    }
                    state.running = false;
                    if refresh_metadata {
                        state.completion_loaded = false;
                    }
                }
                if let Some(history) = history {
                    self.record_database_query_history(history);
                }
                if refresh_metadata && self.ide_panel.database.pending_job.is_none() {
                    self.load_public_database_tables(
                        connection_id,
                        &database_name,
                        crate::app::database::SshHostKeyPolicy::Strict,
                    );
                }
            }
            DatabaseEvent::QueryTransactionRolledBack {
                connection_id,
                database_name,
                console_id,
                ..
            } => {
                self.ide_panel.database.pending_job = None;
                let mut history = None;
                if let Some(index) = self.tabs.iter().position(|tab| match &tab.kind {
                    EditorTabKind::DatabaseQuery(meta, _) => {
                        meta.connection_id == connection_id && meta.console_id == console_id
                    }
                    _ => false,
                }) && let EditorTabKind::DatabaseQuery(_, state) = &mut self.tabs[index].kind
                {
                    if let Some(review) = state.review.take() {
                        history = Some(crate::app::database::DatabaseQueryHistoryEntry {
                            connection_id,
                            database_name,
                            console_id,
                            sql: review.sql,
                            started_unix_ms: review.started_unix_ms,
                            duration_ms: review.duration_ms,
                            succeeded: false,
                            affected_rows: review.changed_rows,
                            error_summary: Some("Транзакция отменена пользователем".to_string()),
                        });
                    }
                    state.running = false;
                }
                if let Some(history) = history {
                    self.record_database_query_history(history);
                }
            }
            DatabaseEvent::QueryTransactionExpired {
                connection_id,
                database_name,
                console_id,
                ..
            } => {
                self.ide_panel.database.pending_job = None;
                let mut history = None;
                if let Some(index) = self.tabs.iter().position(|tab| match &tab.kind {
                    EditorTabKind::DatabaseQuery(meta, _) => {
                        meta.connection_id == connection_id && meta.console_id == console_id
                    }
                    _ => false,
                }) && let EditorTabKind::DatabaseQuery(_, state) = &mut self.tabs[index].kind
                {
                    if let Some(review) = state.review.take() {
                        history = Some(crate::app::database::DatabaseQueryHistoryEntry {
                            connection_id,
                            database_name: database_name.clone(),
                            console_id,
                            sql: review.sql,
                            started_unix_ms: review.started_unix_ms,
                            duration_ms: review.duration_ms,
                            succeeded: false,
                            affected_rows: review.changed_rows,
                            error_summary: Some("Транзакция автоматически отменена по таймауту".to_string()),
                        });
                    }
                    state.running = false;
                    state.error = Some("Транзакция SQL-консоли автоматически отменена по таймауту".to_string());
                }
                if let Some(history) = history {
                    self.record_database_query_history(history);
                }
            }
            DatabaseEvent::QueryFailed {
                connection_id,
                database_name,
                console_id,
                sql,
                started_unix_ms,
                duration_ms,
                message,
                diagnostic,
                ..
            } => {
                if let Some(index) = self.tabs.iter().position(|tab| match &tab.kind {
                    EditorTabKind::DatabaseQuery(meta, _) => {
                        meta.connection_id == connection_id && meta.console_id == console_id
                    }
                    _ => false,
                }) {
                    let (editor_text, line_offsets) = if index == self.active_tab {
                        (self.editor.get_full_text(), self.editor.line_offsets.clone())
                    } else {
                        (
                            self.tabs[index].editor.get_full_text(),
                            self.tabs[index].editor.line_offsets.clone(),
                        )
                    };
                    let diagnostic_editor_version = diagnostic.as_ref().map(|_| {
                        if index == self.active_tab {
                            self.editor.version
                        } else {
                            self.tabs[index].editor.version
                        }
                    });
                    let syntax_errors = diagnostic
                        .as_ref()
                        .map(|diagnostic| vec![(diagnostic.start_byte, diagnostic.end_byte)])
                        .unwrap_or_default();
                    if let EditorTabKind::DatabaseQuery(_, state) = &mut self.tabs[index].kind {
                        state.running = false;
                        state.running_sql = None;
                        state.running_started_unix_ms = 0;
                        state.error = Some(message.clone());
                        state.messages.clear();
                        state.diagnostic = diagnostic;
                        state.diagnostic_editor_version = diagnostic_editor_version;
                        state.editor_diagnostics =
                            crate::app::database::database_query_editor_diagnostics(
                                &state.analysis,
                                state.diagnostic.as_ref(),
                                &editor_text,
                                &line_offsets,
                            );
                        state.review = None;
                        state.result_view.active_result = state.results.len();
                        state.result_view.reset_scroll();
                    }
                    self.tabs[index].syntax_errors = syntax_errors.clone();
                    if index == self.active_tab {
                        self.highlighter.syntax_errors = syntax_errors;
                    }
                }
                self.record_database_query_history(crate::app::database::DatabaseQueryHistoryEntry {
                    connection_id,
                    database_name,
                    console_id,
                    sql,
                    started_unix_ms,
                    duration_ms,
                    succeeded: false,
                    affected_rows: 0,
                    error_summary: Some(message.clone()),
                });
                self.ide_panel.database.global_error = Some(message);
                self.ide_panel.database.pending_job = None;
                self.ide_panel.database.pending_query_mode = None;
            }
            DatabaseEvent::TransactionPrepared { connection_id, transaction_id, database_name, table_name, summary, deadline_unix_ms, .. } => {
                let tab_id = self.tabs.iter().find_map(|tab| match &tab.kind {
                    EditorTabKind::DatabaseTable(meta, _)
                        if meta.connection_id == connection_id
                            && meta.database_name == database_name
                            && meta.table_name == table_name => Some(meta.tab_id),
                    _ => None,
                });
                if let Some(tab_id) = tab_id {
                    let close_after_commit = self.database_table_meta_state(tab_id)
                        .is_some_and(|(_, state)| state.grid.pending_close_after_save);
                    self.ide_panel.database.table_modal = Some(DatabaseTableModal::Review {
                        tab_id,
                        state: crate::app::database::DatabaseTableReviewState {
                            transaction_id,
                            summary,
                            deadline_unix_ms,
                            committing: false,
                            close_after_commit,
                        },
                        scroll: crate::scroll::ScrollState::new(15.0),
                    });
                }
                self.ide_panel.database.pending_job = None;
            }
            DatabaseEvent::TransactionCommitted { connection_id, database_name, table_name, .. } => {
                self.ide_panel.database.pending_job = None;
                self.finish_committed_database_table(connection_id, &database_name, &table_name);
            }
            DatabaseEvent::TransactionRolledBack { .. } => {
                self.ide_panel.database.pending_job = None;
                self.finish_rolled_back_database_table();
            }
            DatabaseEvent::TransactionExpired { connection_id, database_name, table_name, .. } => {
                self.ide_panel.database.pending_job = None;
                self.ide_panel.database.table_modal = None;
                for tab in &mut self.tabs {
                    if let EditorTabKind::DatabaseTable(meta, state) = &mut tab.kind
                        && meta.connection_id == connection_id
                        && meta.database_name == database_name
                        && meta.table_name == table_name
                    {
                        state.grid.pending_close_after_save = false;
                    }
                }
                self.ide_panel.database.global_error = Some(format!(
                    "Транзакция public.{table_name} в базе {database_name} автоматически отменена по таймауту"
                ));
            }
            DatabaseEvent::ConnectionSecretsSaved { connection, .. } => {
                if let Some(index) = self.ide_panel.database.connection_index(connection.id) {
                    self.ide_panel.database.connections[index] = DatabaseConnectionNode::new(connection);
                } else {
                    self.ide_panel.database.connections.push(DatabaseConnectionNode::new(connection));
                }
                self.ide_panel.database.dialog = None;
                self.ide_panel.database.pending_job = None;
                self.save_database_panel_state();
            }
            DatabaseEvent::ConnectionSecretsDeleted { connection_id, .. } => {
                self.ide_panel.database.connections.retain(|node| node.config.id != connection_id);
                self.ide_panel.database.persisted.table_views.retain(|view| view.key.connection_id != connection_id);
                self.ide_panel.database.persisted.consoles.retain(|console| console.connection_id != connection_id);
                self.ide_panel.database.session_secrets.remove(&connection_id);
                self.ide_panel.database.selected_connection = None;
                self.ide_panel.database.selected_database = None;
                self.ide_panel.database.delete_prompt = None;
                self.ide_panel.database.pending_job = None;
                self.save_database_panel_state();
            }
            DatabaseEvent::HostKeyConfirmationRequired { job_id, host, port, algorithm, fingerprint } => {
                self.ide_panel.database.host_key_prompt = Some(DatabaseHostKeyPrompt { job_id, host, port, algorithm, fingerprint });
            }
            DatabaseEvent::JobFailed { message, .. } => {
                let mut handled_locally = false;
                if let Some(pending) = pending.as_ref() {
                    if let Some(node) = self.ide_panel.database.connection_mut(pending.connection_id) {
                        node.loading = false;
                        node.status = DatabaseConnectionStatus::Error;
                        node.status_message = Some(message.clone());
                        if let Some(database_name) = pending.database_name.as_ref()
                            && let Some(database) = node.databases.iter_mut().find(|db| &db.name == database_name)
                        {
                            database.loading = false;
                            database.error = Some(message.clone());
                        }
                    }

                    for tab in &mut self.tabs {
                        let EditorTabKind::DatabaseTable(meta, state) = &mut tab.kind else { continue; };
                        if meta.connection_id != pending.connection_id
                            || pending.database_name.as_deref().is_some_and(|name| name != meta.database_name)
                            || pending.table_name.as_deref().is_some_and(|name| name != meta.table_name)
                        {
                            continue;
                        }
                        match pending.kind {
                            DatabasePendingJobKind::CountRows => {
                                handled_locally = true;
                                let filter_target = state.grid.pending_filter_error_target(false);
                                state.grid.loading_count = false;
                                state.grid.finish_refresh();
                                state.grid.count_error = Some(message.clone());
                                if state.grid.post_commit_refresh_pending {
                                    state.error = Some(format!(
                                        "Изменения успешно применены, но обновить данные не удалось: {message}"
                                    ));
                                    state.grid.post_commit_refresh_pending = false;
                                } else if let Some(target) = filter_target {
                                    state.grid.filter_error = Some((target, message.clone()));
                                    state.error = None;
                                } else {
                                    state.error = Some(message.clone());
                                }
                                state.grid.abort_pending_view();
                            }
                            DatabasePendingJobKind::LoadChunk => {
                                handled_locally = true;
                                let filter_target = state.grid.pending_filter_error_target(true);
                                state.grid.loading_chunk = false;
                                state.grid.finish_refresh();
                                state.grid.in_flight_chunk = None;
                                state.grid.desired_chunk = None;
                                if state.grid.post_commit_refresh_pending {
                                    state.error = Some(format!(
                                        "Изменения успешно применены, но обновить данные не удалось: {message}"
                                    ));
                                    state.grid.post_commit_refresh_pending = false;
                                } else if let Some(target) = filter_target {
                                    state.grid.filter_error = Some((target, message.clone()));
                                    state.error = None;
                                } else {
                                    state.error = Some(message.clone());
                                }
                                state.grid.abort_pending_view();
                            }
                            DatabasePendingJobKind::BeginTableSave
                            | DatabasePendingJobKind::CommitTransaction
                            | DatabasePendingJobKind::RollbackTransaction => {
                                handled_locally = true;
                                state.grid.pending_close_after_save = false;
                                state.error = Some(message.clone());
                                self.ide_panel.database.table_modal = None;
                            }
                            DatabasePendingJobKind::LoadMetadata => {
                                handled_locally = true;
                                state.loading = false;
                                state.error = Some(message.clone());
                                state.set_unavailable_text(message.clone());
                            }
                            _ => {}
                        }
                    }
                    if matches!(pending.kind, DatabasePendingJobKind::LoadQueryCompletion | DatabasePendingJobKind::RunUserSql) {
                        for tab in &mut self.tabs {
                            if let EditorTabKind::DatabaseQuery(meta, state) = &mut tab.kind
                                && meta.connection_id == pending.connection_id
                                && pending.database_name.as_deref().is_none_or(|name| name == meta.database_name)
                            {
                                handled_locally = true;
                                state.running = false;
                                state.error = Some(message.clone());
                            }
                        }
                    }
                }
                if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
                    handled_locally = true;
                    dialog.error = Some(message.clone());
                    dialog.test_status = None;
                }
                self.ide_panel.database.global_error = if handled_locally {
                    None
                } else {
                    Some(message)
                };
                self.ide_panel.database.pending_job = None;
            }
            DatabaseEvent::JobCancelled { .. } => {
                let mut cancelled_query_history = None;
                if let Some(pending) = pending.as_ref() {
                    for tab in &mut self.tabs {
                        let EditorTabKind::DatabaseTable(meta, state) = &mut tab.kind else { continue; };
                        if meta.connection_id != pending.connection_id { continue; }
                        match pending.kind {
                            DatabasePendingJobKind::CountRows => {
                                state.grid.loading_count = false;
                                state.grid.finish_refresh();
                                state.grid.abort_pending_view();
                            },
                            DatabasePendingJobKind::LoadChunk => {
                                state.grid.loading_chunk = false;
                                state.grid.finish_refresh();
                                state.grid.in_flight_chunk = None;
                                state.grid.abort_pending_view();
                            }
                            DatabasePendingJobKind::BeginTableSave
                            | DatabasePendingJobKind::CommitTransaction
                            | DatabasePendingJobKind::RollbackTransaction => {
                                state.grid.pending_close_after_save = false;
                                self.ide_panel.database.table_modal = None;
                            }
                            _ => {}
                        }
                    }
                    if matches!(pending.kind, DatabasePendingJobKind::RunUserSql) {
                        for tab in &mut self.tabs {
                            let EditorTabKind::DatabaseQuery(meta, state) = &mut tab.kind else {
                                continue;
                            };
                            if meta.connection_id != pending.connection_id
                                || pending
                                    .database_name
                                    .as_deref()
                                    .is_some_and(|name| name != meta.database_name)
                                || !state.running
                            {
                                continue;
                            }
                            cancelled_query_history = state.take_cancelled_history(
                                meta.connection_id,
                                &meta.database_name,
                                meta.console_id,
                            );
                            break;
                        }
                    }
                }
                for tab in &mut self.tabs {
                    if let EditorTabKind::DatabaseQuery(_, state) = &mut tab.kind {
                        state.running = false;
                    }
                }
                if let Some(history) = cancelled_query_history {
                    self.record_database_query_history(history);
                }
                self.ide_panel.database.notice = Some("Запрос отменён".to_string());
                self.ide_panel.database.pending_job = None;
                self.ide_panel.database.pending_query_mode = None;
            }
            DatabaseEvent::Busy { active_job_id, .. } => {
                let message = format!(
                    "Сейчас уже выполняется запрос {:?}. Отмените его или дождитесь завершения.",
                    active_job_id
                );
                if let Some(pending) = pending.as_ref()
                    && matches!(
                        pending.kind,
                        DatabasePendingJobKind::CountRows | DatabasePendingJobKind::LoadChunk
                    )
                {
                    for tab in &mut self.tabs {
                        let EditorTabKind::DatabaseTable(meta, state) = &mut tab.kind else {
                            continue;
                        };
                        if meta.connection_id != pending.connection_id
                            || pending
                                .database_name
                                .as_deref()
                                .is_some_and(|name| name != meta.database_name)
                            || pending
                                .table_name
                                .as_deref()
                                .is_some_and(|name| name != meta.table_name)
                        {
                            continue;
                        }
                        state.grid.loading_count = false;
                        state.grid.loading_chunk = false;
                        state.grid.in_flight_chunk = None;
                        state.grid.desired_chunk = None;
                        state.grid.finish_refresh();
                        state.grid.abort_pending_view();
                        state.error = None;
                        state.show_timed_notice(message.clone());
                    }
                }
                self.ide_panel.database.global_error = None;
                self.ide_panel.database.pending_job = None;
            }
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}

fn connection_status(backend: Option<SshBackendKind>) -> DatabaseConnectionStatus {
    match backend {
        Some(SshBackendKind::Builtin) => DatabaseConnectionStatus::BuiltinSsh,
        _ => DatabaseConnectionStatus::Ready,
    }
}

fn apply_connection_notices(
    node: &mut DatabaseConnectionNode,
    backend: Option<SshBackendKind>,
    notices: &[DatabaseBackendNotice],
) {
    node.fallback_reason = notices.iter().find_map(|notice| match notice {
        DatabaseBackendNotice::BuiltinSshFallback { reason } => Some(reason.clone()),
        _ => None,
    });
    node.status_message = notices.first().map(|notice| match notice {
        DatabaseBackendNotice::BuiltinSshFallback { reason } => {
            format!("Используется встроенный SSH. Причина: {reason}")
        }
        DatabaseBackendNotice::NativeCertificateWarnings { count } => {
            format!("Системное хранилище сертификатов вернуло предупреждений: {count}")
        }
    });
    node.status = connection_status(backend);
}
