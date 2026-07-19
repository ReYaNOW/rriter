use crate::app::database::{
    DatabaseBackendNotice, DatabaseCommand, DatabaseConnectionDialog, DatabaseConnectionId,
    DatabaseConnectionNode, DatabaseConnectionStatus, DatabaseContextAction, DatabaseContextMenu,
    DatabaseContextTarget, DatabaseDatabaseNode, DatabaseDeletePrompt, DatabaseDdlHoverState,
    DatabaseEvent, DatabaseHostKeyPrompt, DatabaseJobOwner, DatabasePanelState,
    DatabasePendingJob, DatabasePendingJobKind, DatabaseQueryTabMeta, DatabaseQueryTabState,
    DatabaseRuntime, DatabaseSecretBundle, DatabaseTableTabMeta, DatabaseTableTabState,
    SshBackendKind, SshConnectOptions, SshHostKeyPolicy,
};
use crate::scroll::ScrollState;
use std::io;

fn database_command_should_queue(
    has_pending: bool,
    host_key_prompt_open: bool,
    transaction_review_open: bool,
    can_finish_transaction: bool,
) -> bool {
    has_pending
        || host_key_prompt_open
        || (transaction_review_open && !can_finish_transaction)
}

fn database_dialog_scrollbar_hit(
    track: crate::ui_system::UiClipRect,
    pointer_x: f32,
    pointer_y: f32,
) -> bool {
    track.contains(pointer_x, pointer_y)
}

impl App {
    pub(crate) fn database_connection_dialog_scroll_metrics(
        &self,
    ) -> Option<(
        crate::ui_system::UiClipRect,
        f32,
        f32,
        Option<(crate::ui_system::UiClipRect, crate::scroll::ScrollbarThumb)>,
    )> {
        let renderer = self.renderer.as_ref()?;
        let dialog = self.ide_panel.database.dialog.as_ref()?;
        Some(renderer.database_connection_dialog_scroll_metrics(
            dialog.visible_fields().count(),
            dialog.scroll.current,
        ))
    }

    pub(crate) fn clamp_database_dialog_scroll_to_layout(&mut self) {
        let Some((_, _, max_scroll, _)) = self.database_connection_dialog_scroll_metrics() else {
            return;
        };
        if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
            dialog.clamp_scroll(max_scroll);
        }
    }

    pub(crate) fn ensure_database_dialog_focus_visible(&mut self) {
        let Some((form_clip, row_h, max_scroll, _)) =
            self.database_connection_dialog_scroll_metrics()
        else {
            return;
        };
        let Some(dialog) = self.ide_panel.database.dialog.as_mut() else {
            return;
        };
        let Some(row) = dialog.focused.and_then(|field| dialog.visible_field_index(field)) else {
            dialog.clamp_scroll(max_scroll);
            return;
        };
        dialog.ensure_row_visible(row, row_h, form_clip.h, max_scroll);
    }

    pub(crate) fn start_database_dialog_scroll_drag(
        &mut self,
        pointer_x: f32,
        pointer_y: f32,
    ) -> bool {
        let Some((_, _, max_scroll, scrollbar)) =
            self.database_connection_dialog_scroll_metrics()
        else {
            return false;
        };
        let Some((track, thumb)) = scrollbar else { return false; };
        if !database_dialog_scrollbar_hit(track, pointer_x, pointer_y) {
            return false;
        }
        let Some((drag_offset, target)) = crate::scroll::scrollbar_drag_target(
            pointer_y,
            track.y,
            track.h,
            thumb,
            max_scroll,
            None,
        ) else {
            return false;
        };
        let Some(dialog) = self.ide_panel.database.dialog.as_mut() else {
            return false;
        };
        dialog.scroll.jump_to(target);
        dialog.scroll.drag_offset = drag_offset;
        dialog.scroll.is_dragging = true;
        true
    }

    pub(crate) fn update_database_dialog_scroll_drag(&mut self, pointer_y: f32) -> bool {
        let drag_offset = match self.ide_panel.database.dialog.as_ref() {
            Some(dialog) if dialog.scroll.is_dragging => dialog.scroll.drag_offset,
            _ => return false,
        };
        let Some((_, _, max_scroll, scrollbar)) =
            self.database_connection_dialog_scroll_metrics()
        else {
            return false;
        };
        let Some((track, thumb)) = scrollbar else { return false; };
        let Some((drag_offset, target)) = crate::scroll::scrollbar_drag_target(
            pointer_y,
            track.y,
            track.h,
            thumb,
            max_scroll,
            Some(drag_offset),
        ) else {
            return false;
        };
        let Some(dialog) = self.ide_panel.database.dialog.as_mut() else {
            return false;
        };
        dialog.scroll.jump_to(target);
        dialog.scroll.drag_offset = drag_offset;
        dialog.scroll.is_dragging = true;
        true
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) fn database_dialog_input_index_at(
        &mut self,
        field: crate::app::database::DatabaseFormField,
        mouse_x: f32,
    ) -> Option<usize> {
        let rect = self
            .ui_registry
            .rect_for(crate::ui_system::UiId::DatabaseDialogField(field))?;
        let renderer = self.renderer.as_mut()?;
        let input = self.ide_panel.database.dialog.as_ref()?.input(field);
        let s = renderer.scale_factor;
        let text_scale = 0.82;
        let eye_size = if field.is_secret() { (28.0 * s).round() } else { 0.0 };
        let visible_width = (rect.2 - 16.0 * s - eye_size).max(1.0);
        let secret = field.is_secret()
            && !self
                .ide_panel
                .database
                .dialog
                .as_ref()
                .is_some_and(|dialog| dialog.secret_is_revealed(field));
        let scroll_x = crate::app::file_tree::file_tree_name_input_scroll_x(
            input.text(),
            input.cursor,
            visible_width,
            |ch| {
                let rendered = if secret { '•' } else { ch };
                renderer
                    .get_ui_glyph(rendered)
                    .map(|glyph| glyph.advance * text_scale)
                    .unwrap_or(10.0 * text_scale)
            },
        );
        let x_offset = (mouse_x - rect.0 - 8.0 * s + scroll_x).max(0.0);
        Some(crate::app::file_tree::file_tree_name_input_hit_index(
            input.text(),
            x_offset,
            |ch| {
                let rendered = if secret { '•' } else { ch };
                renderer
                    .get_ui_glyph(rendered)
                    .map(|glyph| glyph.advance * text_scale)
                    .unwrap_or(10.0 * text_scale)
            },
        ))
    }

    pub(crate) fn set_database_dialog_input_cursor(
        &mut self,
        field: crate::app::database::DatabaseFormField,
        target: usize,
        selecting: bool,
    ) {
        if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
            dialog.focused = Some(field);
            dialog.input_mut(field).set_cursor(target, selecting);
            self.last_action = std::time::Instant::now();
            self.last_blink_state = true;
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) fn handle_database_dialog_keyboard(
        &mut self,
        key_event: &winit::event::KeyEvent,
    ) -> bool {
        use winit::event::ElementState;
        use winit::keyboard::{KeyCode, PhysicalKey};

        if self.ide_panel.database.dialog.is_none() {
            return false;
        }
        if key_event.state != ElementState::Pressed {
            return true;
        }
        self.last_action = std::time::Instant::now();
        self.last_blink_state = true;

        let primary = crate::platform::primary_shortcut_modifier(self.modifiers);
        let word = crate::platform::word_navigation_modifier(self.modifiers);
        let shift = self.modifiers.shift_key();
        let text_input_allowed = crate::platform::text_input_modifiers_allowed(self.modifiers);
        let paste_text = if primary
            && key_event.physical_key == PhysicalKey::Code(KeyCode::KeyV)
        {
            self.get_clipboard_text()
        } else {
            None
        };
        let mut copy_text = None;
        let mut cancel = false;
        let mut save = false;
        let mut focus_moved = false;

        if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
            match key_event.physical_key {
                PhysicalKey::Code(KeyCode::Escape) => cancel = true,
                PhysicalKey::Code(KeyCode::Tab) => {
                    dialog.focus_next(shift);
                    focus_moved = true;
                }
                PhysicalKey::Code(KeyCode::Enter | KeyCode::NumpadEnter) => save = true,
                physical_key => {
                    let Some(field) = dialog.focused else {
                        return true;
                    };
                    copy_text = crate::app::single_line_input::handle_single_line_input(
                        dialog.input_mut(field),
                        physical_key,
                        key_event.logical_key.to_text(),
                        primary,
                        word,
                        shift,
                        text_input_allowed,
                        paste_text.as_deref(),
                        database_dialog_field_max_bytes(field),
                    );
                    dialog.error = None;
                    dialog.test_status = None;
                }
            }
        }

        if let Some(text) = copy_text {
            self.set_clipboard_text(text);
        }
        if focus_moved {
            self.ensure_database_dialog_focus_visible();
        }
        if cancel {
            self.cancel_database_dialog();
        } else if save {
            self.save_database_connection_dialog();
        }
        true
    }

    pub fn active_tab_is_database_table(&self) -> bool {
        self.tabs
            .get(self.active_tab)
            .is_some_and(|tab| tab.kind.is_database_table())
    }

    pub fn active_tab_is_database_query(&self) -> bool {
        self.tabs
            .get(self.active_tab)
            .is_some_and(|tab| tab.kind.is_database_query())
    }

    pub fn active_tab_is_database(&self) -> bool {
        self.active_tab_is_database_table() || self.active_tab_is_database_query()
    }

    pub fn database_blocking_modal_open(&self) -> bool {
        self.ide_panel.database.modal_open()
            || self.tabs.get(self.active_tab).is_some_and(|tab| {
                matches!(
                    &tab.kind,
                    EditorTabKind::DatabaseQuery(_, state) if state.review.is_some()
                )
            })
    }

    pub(crate) fn load_database_panel_state(&mut self) {
        let state = match crate::app::database::load_database_state(
            &crate::app::database::database_state_path(),
        ) {
            Ok(state) => state,
            Err(error) => {
                let mut panel = DatabasePanelState::default();
                panel.global_error = Some(format!("Не удалось загрузить базы данных: {error}"));
                self.ide_panel.database = panel;
                return;
            }
        };
        self.ide_panel.database = DatabasePanelState::from_persisted(state);
    }

    pub(crate) fn save_database_panel_state(&mut self) {
        self.ide_panel.database.sync_persisted_connections();
        if let Err(error) = crate::app::database::save_database_state(
            &crate::app::database::database_state_path(),
            &self.ide_panel.database.persisted,
        ) {
            self.ide_panel.database.global_error =
                Some(format!("Не удалось сохранить базы данных: {error}"));
        }
    }

    pub(crate) fn reconcile_expanded_database_connections(&mut self) {
        let connection_ids = self
            .ide_panel
            .database
            .begin_expanded_connection_catalog_loads();
        for connection_id in connection_ids {
            self.load_connection_databases(connection_id, SshHostKeyPolicy::Strict);
        }
    }

    fn ensure_database_runtime(&mut self) -> io::Result<&DatabaseRuntime> {
        if self.database_runtime.is_none() {
            self.database_runtime = Some(DatabaseRuntime::spawn()?);
        }
        self.database_runtime.as_ref().ok_or_else(|| {
            io::Error::new(io::ErrorKind::Other, "database runtime was not initialized")
        })
    }

    pub(crate) fn shutdown_database_runtime(&mut self) {
        if let Some(mut runtime) = self.database_runtime.take() {
            runtime.shutdown();
        }
    }

    pub(crate) fn poll_database_runtime(&mut self) {
        let mut events = Vec::new();
        let runtime_alive = self
            .database_runtime
            .as_ref()
            .is_none_or(|runtime| runtime.drain_events(&mut events));
        for event in events {
            self.apply_database_event(event);
        }
        let now = std::time::Instant::now();
        self.ide_panel.database.prune_cancelled_jobs(now);
        let cancel_timed_out = self.ide_panel.database.cancel_timed_out(
            now,
            std::time::Duration::from_secs(2),
        );
        if !runtime_alive || cancel_timed_out {
            let message = if runtime_alive {
                "Database Tools не подтвердил отмену вовремя"
            } else {
                "Database Tools неожиданно завершился"
            };
            if let Some(pending) = self.ide_panel.database.pending_job.clone() {
                self.recover_database_pending_job(&pending, message, false);
            }
            let queued = self
                .ide_panel
                .database
                .queued_commands
                .drain(..)
                .map(|(_, pending)| pending)
                .collect::<Vec<_>>();
            for pending in queued {
                self.recover_database_pending_job(&pending, message, false);
            }
            self.ide_panel.database.clear_active_command();
            self.ide_panel.database.cancel_requested_at = None;
            self.ide_panel.database.host_key_retry = None;
            self.ide_panel.database.host_key_prompt = None;
            self.ide_panel.database.global_error = Some(message.to_string());
            self.database_runtime = None;
        }
    }

    fn database_transaction_review_open(&self) -> bool {
        self.tabs.iter().any(|tab| {
            matches!(&tab.kind, EditorTabKind::DatabaseQuery(_, state) if state.review.is_some())
        }) || matches!(
            self.ide_panel.database.table_modal,
            Some(DatabaseTableModal::Review { .. })
        )
    }

    fn database_command_can_finish_transaction(pending: &DatabasePendingJob) -> bool {
        matches!(
            pending.kind,
            DatabasePendingJobKind::CommitTransaction
                | DatabasePendingJobKind::RollbackTransaction
        )
    }

    pub(crate) fn start_database_command_now(
        &mut self,
        command: DatabaseCommand,
        pending: DatabasePendingJob,
    ) -> bool {
        let command_copy = command.clone();
        match self
            .ensure_database_runtime()
            .and_then(|runtime| runtime.send(command))
        {
            Ok(()) => {
                self.ide_panel
                    .database
                    .activate_command(command_copy, pending);
                true
            }
            Err(error) => {
                let message = format!("Не удалось запустить Database Tools: {error}");
                self.recover_database_pending_job(&pending, &message, false);
                self.ide_panel.database.global_error = Some(message);
                false
            }
        }
    }

    fn send_database_command(
        &mut self,
        command: DatabaseCommand,
        pending: DatabasePendingJob,
    ) -> bool {
        let must_wait = database_command_should_queue(
            self.ide_panel.database.pending_job.is_some(),
            self.ide_panel.database.host_key_prompt.is_some(),
            self.database_transaction_review_open(),
            Self::database_command_can_finish_transaction(&pending),
        );
        if must_wait {
            return match self.ide_panel.database.queue_command(command, pending.clone()) {
                crate::app::database::DatabaseQueueResult::Queued => true,
                crate::app::database::DatabaseQueueResult::Full => {
                    let message = "Очередь Database Tools переполнена. Дождитесь завершения текущих операций.";
                    self.recover_database_pending_job(&pending, message, false);
                    self.ide_panel.database.global_error = Some(message.to_string());
                    false
                }
            };
        }
        self.start_database_command_now(command, pending)
    }

    pub(crate) fn finish_database_active_job(&mut self, drain_queue: bool) {
        self.ide_panel.database.clear_active_command();
        self.ide_panel.database.cancel_requested_at = None;
        if drain_queue {
            self.drain_database_command_queue();
        }
    }

    pub(crate) fn drain_database_command_queue(&mut self) {
        if self.ide_panel.database.pending_job.is_some()
            || self.ide_panel.database.host_key_prompt.is_some()
            || self.database_transaction_review_open()
        {
            return;
        }
        while let Some((command, pending)) = self.ide_panel.database.pop_queued_command() {
            if self.start_database_command_now(command, pending) {
                break;
            }
        }
    }

    pub(crate) fn recover_database_pending_job(
        &mut self,
        pending: &DatabasePendingJob,
        message: &str,
        cancelled: bool,
    ) {
        let targets = pending.kind.recovery_targets();
        if matches!(pending.kind, DatabasePendingJobKind::SaveConnection) {
            self.ide_panel
                .database
                .pending_session_secrets
                .remove(&pending.connection_id);
        }
        if let Some(node) = self.ide_panel.database.connection_mut(pending.connection_id) {
            if targets.connection_loading {
                node.loading = false;
                node.status = if cancelled {
                    DatabaseConnectionStatus::Disconnected
                } else {
                    DatabaseConnectionStatus::Error
                };
                node.status_message = (!cancelled).then(|| message.to_string());
            }
            if targets.database_loading
                && let Some(database_name) = pending.database_name.as_deref()
                && let Some(database) = node
                    .databases
                    .iter_mut()
                    .find(|database| database.name == database_name)
            {
                database.loading = false;
                database.error = (!cancelled).then(|| message.to_string());
            }
        }

        if targets.dialog_status
            && let Some(dialog) = self.ide_panel.database.dialog.as_mut()
        {
            dialog.test_status = None;
            dialog.error = (!cancelled).then(|| message.to_string());
        }

        for tab in &mut self.tabs {
            match &mut tab.kind {
                EditorTabKind::DatabaseTable(meta, state)
                    if meta.connection_id == pending.connection_id
                        && pending
                            .database_name
                            .as_deref()
                            .is_none_or(|name| name == meta.database_name)
                        && pending
                            .table_name
                            .as_deref()
                            .is_none_or(|name| name == meta.table_name) =>
                {
                    if targets.metadata_loading {
                            state.loading = false;
                            if !cancelled {
                                state.error = Some(message.to_string());
                                state.set_unavailable_text(message.to_string());
                            }
                    }
                    if targets.count_loading {
                            state.grid.loading_count = false;
                            state.grid.finish_refresh();
                            state.grid.abort_pending_view();
                            if !cancelled {
                                state.grid.count_error = Some(message.to_string());
                            }
                    }
                    if targets.chunk_loading {
                            state.grid.loading_chunk = false;
                            state.grid.in_flight_chunk = None;
                            state.grid.desired_chunk = None;
                            state.grid.finish_refresh();
                            state.grid.abort_pending_view();
                            if !cancelled {
                                state.error = Some(message.to_string());
                            }
                    }
                    if targets.table_transaction {
                            state.grid.pending_close_after_save = false;
                            if !cancelled {
                                state.error = Some(message.to_string());
                            }
                    }
                }
                EditorTabKind::DatabaseQuery(meta, state)
                    if meta.connection_id == pending.connection_id
                        && pending
                            .database_name
                            .as_deref()
                            .is_none_or(|name| name == meta.database_name) =>
                {
                    if targets.query_running {
                            state.running = false;
                            state.running_sql = None;
                            state.running_started_unix_ms = 0;
                            if !cancelled {
                                state.error = Some(message.to_string());
                            }
                    }
                    if targets.query_transaction {
                            if let Some(review) = state.review.as_mut() {
                                review.finishing = false;
                            }
                            if !cancelled {
                                state.error = Some(message.to_string());
                            }
                    }
                }
                _ => {}
            }
        }

        if let Some(DatabaseTableModal::Review { state, .. }) =
            self.ide_panel.database.table_modal.as_mut()
            && targets.table_transaction
        {
            state.committing = false;
        }
    }

    pub fn open_database_connection_dialog(&mut self) {
        let color = self.ide_panel.database.settings().default_connection_color;
        let session_id = self.ide_panel.database.allocate_dialog_id();
        let mut dialog = DatabaseConnectionDialog::new(color);
        dialog.session_id = session_id;
        self.ide_panel.database.dialog = Some(dialog);
        self.ide_panel.database.context_menu = None;
        self.ide_panel.database.ddl_hover.borrow_mut().take();
        crate::app::mouse::suppress_hover_popup_until_mouse_move(self.renderer.as_mut());
    }

    pub fn edit_database_connection(&mut self, connection_id: DatabaseConnectionId) {
        let Some(connection) = self
            .ide_panel
            .database
            .connection(connection_id)
            .map(|node| node.config.clone())
        else {
            return;
        };
        let session_id = self.ide_panel.database.allocate_dialog_id();
        let mut dialog = DatabaseConnectionDialog::from_connection(&connection);
        dialog.session_id = session_id;
        self.ide_panel.database.dialog = Some(dialog);
        self.ide_panel.database.context_menu = None;
        self.ide_panel.database.ddl_hover.borrow_mut().take();
        crate::app::mouse::suppress_hover_popup_until_mouse_move(self.renderer.as_mut());
    }

    pub(crate) fn cancel_database_owner(&mut self, owner: DatabaseJobOwner) {
        let removed = self.ide_panel.database.remove_queued_owner(owner);
        for pending in removed {
            if matches!(pending.kind, DatabasePendingJobKind::SaveConnection) {
                self.ide_panel.database.pending_session_secrets.remove(&pending.connection_id);
            }
        }
        let active = self
            .ide_panel
            .database
            .pending_job
            .as_ref()
            .filter(|pending| pending.owner == owner)
            .cloned();
        if let Some(pending) = active {
            self.ide_panel
                .database
                .mark_job_cancelled(pending.id, std::time::Instant::now());
            self.recover_database_pending_job(&pending, "Операция отменена", true);
            let cancel_sent = self
                .database_runtime
                .as_ref()
                .is_some_and(|runtime| runtime.send(DatabaseCommand::CancelJob { job_id: pending.id }).is_ok());
            if cancel_sent {
                self.ide_panel.database.cancel_requested_at = Some(std::time::Instant::now());
            } else {
                self.ide_panel.database.clear_active_command();
                self.ide_panel.database.cancel_requested_at = None;
                self.drain_database_command_queue();
            }
        } else {
            self.drain_database_command_queue();
        }
    }

    pub fn cancel_database_dialog(&mut self) {
        let Some(session_id) = self.ide_panel.database.dialog.as_ref().map(|dialog| dialog.session_id) else {
            return;
        };
        let owner = DatabaseJobOwner::Dialog(session_id);
        self.cancel_database_owner(owner);
        self.ide_panel.database.dialog = None;
    }

    pub fn test_database_dialog_connection(&mut self) {
        let (connection, secrets, dialog_session_id) = {
            let panel = &mut self.ide_panel.database;
            let Some(dialog) = panel.dialog.as_mut() else {
                return;
            };
            let fallback_id = dialog
                .editing_connection_id
                .unwrap_or_else(|| DatabaseConnectionId(panel.next_connection_id));
            let connection = match dialog.build_config(fallback_id) {
                Ok(connection) => connection,
                Err(error) => {
                    dialog.error = Some(error);
                    return;
                }
            };
            dialog.error = None;
            dialog.test_status = Some("Проверка подключения…".to_string());
            (connection, dialog.secret_bundle(), dialog.session_id)
        };
        let job_id = self.ide_panel.database.allocate_job_id();
        let pending = DatabasePendingJob {
            id: job_id,
            kind: DatabasePendingJobKind::TestConnection,
            owner: DatabaseJobOwner::Dialog(dialog_session_id),
            connection_id: connection.id,
            database_name: None,
            table_name: None,
        };
        let settings = self.ide_panel.database.settings().clone();
        self.send_database_command(
            DatabaseCommand::TestConnection {
                job_id,
                connection,
                secrets: Some(secrets),
                settings,
                ssh_options: SshConnectOptions::default(),
            },
            pending,
        );
    }

    pub fn save_database_connection_dialog(&mut self) {
        let needs_connection_id = self.ide_panel.database.dialog.as_ref().is_some_and(|dialog| {
            dialog.editing_connection_id.is_none() && dialog.reserved_connection_id.is_none()
        });
        let allocated_id = needs_connection_id.then(|| self.ide_panel.database.allocate_connection_id());
        let (connection, secrets, dialog_session_id) = {
            let panel = &mut self.ide_panel.database;
            let Some(dialog) = panel.dialog.as_mut() else {
                return;
            };
            if dialog.reserved_connection_id.is_none() {
                dialog.reserved_connection_id = dialog.editing_connection_id.or(allocated_id);
            }
            let fallback_id = dialog
                .editing_connection_id
                .or(dialog.reserved_connection_id)
                .unwrap_or(DatabaseConnectionId(1));
            let connection = match dialog.build_config(fallback_id) {
                Ok(connection) => connection,
                Err(error) => {
                    dialog.error = Some(error);
                    return;
                }
            };
            let secrets = dialog.secret_bundle();
            dialog.error = None;
            dialog.test_status = Some("Сохранение…".to_string());
            (connection, secrets, dialog.session_id)
        };
        let job_id = self.ide_panel.database.allocate_job_id();
        let pending = DatabasePendingJob {
            id: job_id,
            kind: DatabasePendingJobKind::SaveConnection,
            owner: DatabaseJobOwner::Dialog(dialog_session_id),
            connection_id: connection.id,
            database_name: None,
            table_name: None,
        };
        let session_secrets = secrets.clone_for_job();
        let connection_id = connection.id;
        if self.send_database_command(
            DatabaseCommand::SaveConnectionSecrets {
                job_id,
                connection,
                supplied: secrets,
            },
            pending,
        ) {
            self.ide_panel
                .database
                .pending_session_secrets
                .insert(connection_id, session_secrets);
        }
    }

    pub fn request_delete_database_connection(&mut self, connection_id: DatabaseConnectionId) {
        let blocked_open_tabs = self
            .tabs
            .iter()
            .filter(|tab| match &tab.kind {
                EditorTabKind::DatabaseTable(meta, _) => meta.connection_id == connection_id,
                EditorTabKind::DatabaseQuery(meta, _) => meta.connection_id == connection_id,
                _ => false,
            })
            .count();
        self.ide_panel.database.delete_prompt = Some(DatabaseDeletePrompt {
            connection_id,
            blocked_open_tabs,
        });
    }

    pub fn confirm_delete_database_connection(&mut self) {
        let Some(prompt) = self.ide_panel.database.delete_prompt.clone() else {
            return;
        };
        if prompt.blocked_open_tabs > 0 {
            self.ide_panel.database.global_error = Some(format!(
                "Нельзя удалить подключение: открыто вкладок — {}",
                prompt.blocked_open_tabs
            ));
            return;
        }
        let job_id = self.ide_panel.database.allocate_job_id();
        let pending = DatabasePendingJob {
            id: job_id,
            kind: DatabasePendingJobKind::DeleteConnection,
            owner: DatabaseJobOwner::Connection(prompt.connection_id),
            connection_id: prompt.connection_id,
            database_name: None,
            table_name: None,
        };
        self.send_database_command(
            DatabaseCommand::DeleteConnectionSecrets {
                job_id,
                connection_id: prompt.connection_id,
            },
            pending,
        );
    }

    pub fn cancel_delete_database_connection(&mut self) {
        self.ide_panel.database.delete_prompt = None;
    }

    pub fn select_database_connection(&mut self, connection_id: DatabaseConnectionId) {
        self.ide_panel.database.selected_connection = Some(connection_id);
        self.ide_panel.database.selected_database = None;
    }

    pub fn toggle_database_connection(&mut self, connection_id: DatabaseConnectionId) {
        self.select_database_connection(connection_id);
        let should_load = self
            .ide_panel
            .database
            .toggle_connection_expansion(connection_id);
        if should_load {
            self.load_connection_databases(connection_id, SshHostKeyPolicy::Strict);
        }
    }

    pub fn refresh_selected_database(&mut self) {
        if let Some((connection_id, database_name)) = self.ide_panel.database.selected_database.clone() {
            self.load_public_database_tables(
                connection_id,
                &database_name,
                SshHostKeyPolicy::Strict,
            );
        } else if let Some(connection_id) = self.ide_panel.database.selected_connection {
            self.load_connection_databases(connection_id, SshHostKeyPolicy::Strict);
        }
    }

    pub fn toggle_database_node(&mut self, connection_id: DatabaseConnectionId, database_idx: usize) {
        let (selected_name, database_name) = {
            let Some(connection) = self.ide_panel.database.connection_mut(connection_id) else {
                return;
            };
            let Some(database) = connection.databases.get_mut(database_idx) else {
                return;
            };
            database.expanded = !database.expanded;
            let selected_name = database.name.clone();
            let load_name = if database.expanded && !database.tables_loaded && !database.loading {
                Some(selected_name.clone())
            } else {
                None
            };
            (selected_name, load_name)
        };
        self.ide_panel.database.selected_connection = Some(connection_id);
        self.ide_panel.database.selected_database = Some((connection_id, selected_name));
        if let Some(database_name) = database_name {
            self.load_public_database_tables(
                connection_id,
                &database_name,
                SshHostKeyPolicy::Strict,
            );
        }
    }

    fn connection_job_secrets(&self, connection_id: DatabaseConnectionId) -> Option<DatabaseSecretBundle> {
        self.ide_panel
            .database
            .session_secrets
            .get(&connection_id)
            .map(DatabaseSecretBundle::clone_for_job)
    }

    fn load_connection_databases(
        &mut self,
        connection_id: DatabaseConnectionId,
        host_key_policy: SshHostKeyPolicy,
    ) {
        let Some(connection) = self
            .ide_panel
            .database
            .connection(connection_id)
            .map(|node| node.config.clone())
        else {
            return;
        };
        if let Some(node) = self.ide_panel.database.connection_mut(connection_id) {
            node.loading = true;
            node.catalog_load_attempted = true;
            node.status = DatabaseConnectionStatus::Connecting;
            node.status_message = None;
        }
        let job_id = self.ide_panel.database.allocate_job_id();
        let pending = DatabasePendingJob {
            id: job_id,
            kind: DatabasePendingJobKind::LoadDatabases,
            owner: DatabaseJobOwner::Connection(connection_id),
            connection_id,
            database_name: None,
            table_name: None,
        };
        let settings = self.ide_panel.database.settings().clone();
        let secrets = self.connection_job_secrets(connection_id);
        self.send_database_command(
            DatabaseCommand::LoadDatabases {
                job_id,
                connection,
                secrets,
                settings,
                ssh_options: crate::app::database::host_key_options(host_key_policy),
            },
            pending,
        );
    }

    fn load_public_database_tables(
        &mut self,
        connection_id: DatabaseConnectionId,
        database_name: &str,
        host_key_policy: SshHostKeyPolicy,
    ) {
        let Some(connection) = self
            .ide_panel
            .database
            .connection(connection_id)
            .map(|node| node.config.clone())
        else {
            return;
        };
        if let Some(node) = self.ide_panel.database.connection_mut(connection_id)
            && let Some(database) = node.databases.iter_mut().find(|db| db.name == database_name)
        {
            database.loading = true;
            database.error = None;
        }
        let job_id = self.ide_panel.database.allocate_job_id();
        let pending = DatabasePendingJob {
            id: job_id,
            kind: DatabasePendingJobKind::LoadTables,
            owner: DatabaseJobOwner::Connection(connection_id),
            connection_id,
            database_name: Some(database_name.to_string()),
            table_name: None,
        };
        let settings = self.ide_panel.database.settings().clone();
        let secrets = self.connection_job_secrets(connection_id);
        self.send_database_command(
            DatabaseCommand::LoadPublicTables {
                job_id,
                connection,
                database_name: database_name.to_string(),
                secrets,
                settings,
                ssh_options: crate::app::database::host_key_options(host_key_policy),
            },
            pending,
        );
    }

    pub fn open_database_context_menu(
        &mut self,
        target: DatabaseContextTarget,
        x: f32,
        y: f32,
    ) {
        self.ide_panel.file_tree_context_menu = None;
        let entries = match target {
            DatabaseContextTarget::Connection(_) => vec![
                DatabaseContextAction::Refresh,
                DatabaseContextAction::TestConnection,
                DatabaseContextAction::EditConnection,
                DatabaseContextAction::DeleteConnection,
            ],
            DatabaseContextTarget::Database(_, _) => vec![
                DatabaseContextAction::OpenSql,
                DatabaseContextAction::NewSqlConsole,
                DatabaseContextAction::Refresh,
                DatabaseContextAction::CloseConnection,
            ],
            DatabaseContextTarget::Table(_, _, _) => vec![
                DatabaseContextAction::ShowDdl,
                DatabaseContextAction::EditData,
                DatabaseContextAction::OpenSql,
            ],
        };
        let scale = self
            .renderer
            .as_ref()
            .map_or(1.0, |renderer| renderer.scale_factor);
        let (x, y) = crate::app::context_menu::context_menu_anchor(x, y, scale);
        self.ide_panel.database.context_menu = Some(DatabaseContextMenu {
            target,
            x,
            y,
            entries,
            opened_at: std::time::Instant::now(),
        });
    }

    pub fn activate_database_context_action(&mut self, action_idx: usize) {
        let Some(menu) = self.ide_panel.database.context_menu.take() else {
            return;
        };
        let Some(action) = menu.entries.get(action_idx).copied() else {
            return;
        };
        match (menu.target, action) {
            (DatabaseContextTarget::Connection(id), DatabaseContextAction::Refresh) => {
                self.load_connection_databases(id, SshHostKeyPolicy::Strict)
            }
            (DatabaseContextTarget::Connection(id), DatabaseContextAction::TestConnection) => {
                self.edit_database_connection(id);
                self.test_database_dialog_connection();
            }
            (DatabaseContextTarget::Connection(id), DatabaseContextAction::EditConnection) => {
                self.edit_database_connection(id)
            }
            (DatabaseContextTarget::Connection(id), DatabaseContextAction::DeleteConnection) => {
                self.request_delete_database_connection(id)
            }
            (DatabaseContextTarget::Database(id, database_idx), DatabaseContextAction::Refresh) => {
                let name = self
                    .ide_panel
                    .database
                    .connection(id)
                    .and_then(|node| node.databases.get(database_idx))
                    .map(|database| database.name.clone());
                if let Some(name) = name {
                    self.load_public_database_tables(id, &name, SshHostKeyPolicy::Strict);
                }
            }
            (DatabaseContextTarget::Database(id, _), DatabaseContextAction::CloseConnection) => {
                if let Some(connection) = self.ide_panel.database.connection_mut(id) {
                    connection.status = DatabaseConnectionStatus::Disconnected;
                    connection.status_message = Some("Соединение закрыто".to_string());
                    connection.loading = false;
                    for database in &mut connection.databases {
                        database.loading = false;
                    }
                }
            }
            (DatabaseContextTarget::Database(id, database_idx), DatabaseContextAction::OpenSql)
            | (DatabaseContextTarget::Database(id, database_idx), DatabaseContextAction::NewSqlConsole) => {
                let database_name = self
                    .ide_panel
                    .database
                    .connection(id)
                    .and_then(|node| node.databases.get(database_idx))
                    .map(|database| database.name.clone());
                if let Some(database_name) = database_name {
                    self.open_database_query_tab(id, &database_name, action == DatabaseContextAction::NewSqlConsole, None);
                }
            }
            (DatabaseContextTarget::Table(id, database_idx, table_idx), action) => {
                let target = self
                    .ide_panel
                    .database
                    .connection(id)
                    .and_then(|node| node.databases.get(database_idx))
                    .and_then(|database| {
                        database.tables.get(table_idx).map(|table| {
                            (database.name.clone(), table.name.clone())
                        })
                    });
                if let Some((database_name, table_name)) = target {
                    match action {
                        DatabaseContextAction::ShowDdl => {
                            self.load_database_ddl(id, &database_name, &table_name, SshHostKeyPolicy::Strict)
                        }
                        DatabaseContextAction::EditData => {
                            self.open_database_table_tab(id, &database_name, &table_name)
                        }
                        DatabaseContextAction::OpenSql => {
                            let template = format!(
                                "SELECT *\nFROM \"public\".{}\nLIMIT 100;\n",
                                crate::app::database::quote_pg_identifier(&table_name)
                            );
                            self.open_database_query_tab(id, &database_name, false, Some(&template));
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn load_database_ddl(
        &mut self,
        connection_id: DatabaseConnectionId,
        database_name: &str,
        table_name: &str,
        host_key_policy: SshHostKeyPolicy,
    ) {
        let Some(connection) = self
            .ide_panel
            .database
            .connection(connection_id)
            .map(|node| node.config.clone())
        else {
            return;
        };
        let job_id = self.ide_panel.database.allocate_job_id();
        let pending = DatabasePendingJob {
            id: job_id,
            kind: DatabasePendingJobKind::LoadDdl,
            owner: DatabaseJobOwner::Connection(connection_id),
            connection_id,
            database_name: Some(database_name.to_string()),
            table_name: Some(table_name.to_string()),
        };
        let settings = self.ide_panel.database.settings().clone();
        let secrets = self.connection_job_secrets(connection_id);
        self.send_database_command(
            DatabaseCommand::LoadDdl {
                job_id,
                connection,
                database_name: database_name.to_string(),
                table_name: table_name.to_string(),
                secrets,
                settings,
                ssh_options: crate::app::database::host_key_options(host_key_policy),
            },
            pending,
        );
    }

    pub fn open_database_table_tab(
        &mut self,
        connection_id: DatabaseConnectionId,
        database_name: &str,
        table_name: &str,
    ) {
        if let Some(idx) = self.tabs.iter().position(|tab| matches!(
            &tab.kind,
            EditorTabKind::DatabaseTable(meta, _)
                if meta.connection_id == connection_id
                    && meta.database_name == database_name
                    && meta.table_name == table_name
        )) {
            self.switch_to_tab(idx);
            return;
        }
        let tab_id = self.ide_panel.database.allocate_tab_id();
        let meta = DatabaseTableTabMeta {
            tab_id,
            connection_id,
            database_name: database_name.to_string(),
            table_name: table_name.to_string(),
        };
        let title = meta.title();
        let view = self.database_table_view_state(connection_id, database_name, table_name);
        let tab = database_tab(
            title,
            "",
            EditorTabKind::DatabaseTable(meta.clone(), DatabaseTableTabState::new(view)),
            "database-table",
            true,
        );
        self.push_database_tab(tab, None);
        self.ide_panel.database.open_table_ids.insert(tab_id);
        self.ide_panel.database.open_table_keys.insert((
            connection_id,
            database_name.to_string(),
            table_name.to_string(),
        ));
        self.load_database_table_metadata(&meta, SshHostKeyPolicy::Strict);
    }

    fn load_database_table_metadata(
        &mut self,
        meta: &DatabaseTableTabMeta,
        host_key_policy: SshHostKeyPolicy,
    ) {
        let Some(connection) = self
            .ide_panel
            .database
            .connection(meta.connection_id)
            .map(|node| node.config.clone())
        else {
            if let Some((_, state)) = self.database_table_meta_state_mut(meta.tab_id) {
                state.loading = false;
                state.error = None;
                state.set_unavailable_text(
                    crate::app::database::DATABASE_TABLE_DISCONNECTED_MESSAGE,
                );
            }
            return;
        };
        let job_id = self.ide_panel.database.allocate_job_id();
        let pending = DatabasePendingJob {
            id: job_id,
            kind: DatabasePendingJobKind::LoadMetadata,
            owner: DatabaseJobOwner::Table(meta.tab_id),
            connection_id: meta.connection_id,
            database_name: Some(meta.database_name.clone()),
            table_name: Some(meta.table_name.clone()),
        };
        let settings = self.ide_panel.database.settings().clone();
        let secrets = self.connection_job_secrets(meta.connection_id);
        self.send_database_command(
            DatabaseCommand::LoadMetadata {
                job_id,
                connection,
                database_name: meta.database_name.clone(),
                table_name: meta.table_name.clone(),
                secrets,
                settings,
                ssh_options: crate::app::database::host_key_options(host_key_policy),
            },
            pending,
        );
    }

    pub fn open_database_query_tab(
        &mut self,
        connection_id: DatabaseConnectionId,
        database_name: &str,
        force_new: bool,
        template: Option<&str>,
    ) {
        self.open_database_query_tab_internal(
            connection_id,
            database_name,
            force_new,
            template,
            None,
        );
    }

    pub(crate) fn restore_database_query_tab(
        &mut self,
        connection_id: DatabaseConnectionId,
        database_name: &str,
        console_id: crate::app::database::SqlConsoleId,
    ) {
        self.open_database_query_tab_internal(
            connection_id,
            database_name,
            true,
            None,
            Some(console_id),
        );
    }

    fn open_database_query_tab_internal(
        &mut self,
        connection_id: DatabaseConnectionId,
        database_name: &str,
        force_new: bool,
        template: Option<&str>,
        preferred_console_id: Option<crate::app::database::SqlConsoleId>,
    ) {
        if !force_new
            && let Some(idx) = self.tabs.iter().position(|tab| matches!(
                &tab.kind,
                EditorTabKind::DatabaseQuery(meta, _)
                    if meta.connection_id == connection_id && meta.database_name == database_name
            ))
        {
            self.switch_to_tab(idx);
            if let Some(template) = template
                && self.editor.len() == 0
            {
                self.editor.set_text_clean(template);
            }
            self.request_active_database_query_completion();
            return;
        }

        let existing_count = self
            .tabs
            .iter()
            .filter(|tab| matches!(
                &tab.kind,
                EditorTabKind::DatabaseQuery(meta, _)
                    if meta.connection_id == connection_id && meta.database_name == database_name
            ))
            .count();
        let console_id = preferred_console_id
            .unwrap_or_else(|| self.ide_panel.database.allocate_console_id());
        self.ide_panel.database.next_console_id = self
            .ide_panel
            .database
            .next_console_id
            .max(console_id.0.saturating_add(1));
        let title = if existing_count == 0 {
            format!("{database_name} — SQL")
        } else {
            format!("{database_name} — SQL {}", existing_count + 1)
        };
        let path = crate::app::database::database_console_path(
            connection_id,
            database_name,
            console_id,
        );
        let text = match database_console_initial_text(
            crate::app::database::load_database_console(&path),
            template,
        ) {
            Ok(text) => text,
            Err(error) => {
                self.ide_panel.database.global_error = Some(error);
                return;
            }
        };
        let meta = DatabaseQueryTabMeta {
            console_id,
            connection_id,
            database_name: database_name.to_string(),
            title: title.clone(),
        };
        let tab = database_tab(
            title.clone(),
            "sql",
            EditorTabKind::DatabaseQuery(meta, DatabaseQueryTabState::default()),
            "database",
            false,
        );
        self.push_database_tab(tab, Some(text));
        if let Some(console) = self
            .ide_panel
            .database
            .persisted
            .consoles
            .iter_mut()
            .find(|console| console.id == console_id)
        {
            console.open = true;
            console.title = title;
        } else {
            self.ide_panel.database.persisted.consoles.push(
                crate::app::database::DatabaseConsoleState {
                    id: console_id,
                    connection_id,
                    database_name: database_name.to_string(),
                    title,
                    open: true,
                },
            );
        }
        let open_ids = self
            .ide_panel
            .database
            .open_console_keys
            .entry((connection_id, database_name.to_string()))
            .or_default();
        if !open_ids.contains(&console_id.0) {
            open_ids.push(console_id.0);
        }
        self.normalize_open_database_query_titles();
        self.save_database_panel_state();
        self.request_active_database_query_completion();
    }

    fn push_database_tab(&mut self, tab: EditorTab, active_text: Option<String>) {
        if self.tabs.is_empty() {
            self.editor = Editor::new(active_text.as_ref().map_or(16, |text| text.len() + 64));
            if let Some(text) = active_text.as_deref() {
                self.editor.set_text_clean(text);
            }
            self.file_path = None;
            self.file_key = None;
            self.text_file_format = crate::platform::TextFileFormat::default();
            self.base_title = tab.base_title.clone();
            self.file_extension = tab.file_extension.clone();
            self.scroll_y = ScrollState::new(15.0);
            self.scroll_x = ScrollState::new(15.0);
            self.tabs.push(tab);
            self.active_tab = 0;
        } else {
            self.sync_active_tab();
            self.tabs.push(tab);
            self.active_tab = self.tabs.len() - 1;
            self.sync_active_tab();
            if let Some(text) = active_text.as_deref() {
                self.editor.set_text_clean(text);
            }
        }
        self.show_welcome = false;
        self.autocomplete_active = false;
        if self.active_tab_is_database_query() {
            self.reset_highlighter_with_text(self.editor.get_full_text(), false);
        } else {
            while self.highlighter.rx.try_recv().is_ok() {}
        }
        self.reveal_tab_now(self.active_tab);
        self.save_tabs_state();
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub(crate) fn normalize_open_database_query_titles(&mut self) {
        let mut totals = std::collections::HashMap::<
            (DatabaseConnectionId, String),
            usize,
        >::new();
        for tab in &self.tabs {
            if let EditorTabKind::DatabaseQuery(meta, _) = &tab.kind {
                *totals
                    .entry((meta.connection_id, meta.database_name.clone()))
                    .or_default() += 1;
            }
        }
        let mut positions = std::collections::HashMap::<
            (DatabaseConnectionId, String),
            usize,
        >::new();
        for tab in &mut self.tabs {
            let EditorTabKind::DatabaseQuery(meta, _) = &mut tab.kind else {
                continue;
            };
            let key = (meta.connection_id, meta.database_name.clone());
            let position = positions.entry(key.clone()).or_default();
            *position += 1;
            let title = if totals.get(&key).copied().unwrap_or(0) <= 1 || *position == 1 {
                format!("{} — SQL", meta.database_name)
            } else {
                format!("{} — SQL {}", meta.database_name, *position)
            };
            meta.title.clone_from(&title);
            tab.base_title.clone_from(&title);
            if let Some(console) = self
                .ide_panel
                .database
                .persisted
                .consoles
                .iter_mut()
                .find(|console| console.id == meta.console_id)
            {
                console.title = title;
            }
        }
        if let Some(tab) = self.tabs.get(self.active_tab)
            && matches!(tab.kind, EditorTabKind::DatabaseQuery(_, _))
        {
            self.base_title.clone_from(&tab.base_title);
        }
    }

    pub(crate) fn prepare_database_tab_close(&mut self, idx: usize) {
        let owner = match self.tabs.get(idx).map(|tab| &tab.kind) {
            Some(EditorTabKind::DatabaseTable(meta, _)) => Some(DatabaseJobOwner::Table(meta.tab_id)),
            Some(EditorTabKind::DatabaseQuery(meta, _)) => Some(DatabaseJobOwner::Query(meta.console_id)),
            _ => None,
        };
        if let Some(owner) = owner {
            self.cancel_database_owner(owner);
        }
        let Some(tab) = self.tabs.get(idx) else { return; };
        match &tab.kind {
            EditorTabKind::DatabaseTable(meta, _) => {
                self.ide_panel.database.open_table_ids.remove(&meta.tab_id);
                self.ide_panel.database.open_table_keys.remove(&(
                    meta.connection_id,
                    meta.database_name.clone(),
                    meta.table_name.clone(),
                ));
            }
            EditorTabKind::DatabaseQuery(meta, _) => {
                let text = if idx == self.active_tab {
                    self.editor.get_full_text()
                } else {
                    tab.editor.get_full_text()
                };
                let path = crate::app::database::database_console_path(
                    meta.connection_id,
                    &meta.database_name,
                    meta.console_id,
                );
                if let Err(error) = crate::app::database::save_database_console(&path, &text) {
                    self.ide_panel.database.global_error =
                        Some(format!("Не удалось сохранить SQL-консоль: {error}"));
                }
                if let Some(console) = self
                    .ide_panel
                    .database
                    .persisted
                    .consoles
                    .iter_mut()
                    .find(|console| console.id == meta.console_id)
                {
                    console.open = false;
                }
                if let Some(open) = self
                    .ide_panel
                    .database
                    .open_console_keys
                    .get_mut(&(meta.connection_id, meta.database_name.clone()))
                {
                    open.retain(|id| *id != meta.console_id.0);
                }
            }
            _ => return,
        }
        self.save_database_panel_state();
    }

    pub(crate) fn prepare_all_database_tabs_close(&mut self) {
        for idx in 0..self.tabs.len() {
            self.prepare_database_tab_close(idx);
        }
    }

    pub(crate) fn save_active_database_query(&mut self) {
        if !self.active_tab_is_database_query() {
            return;
        }
        let Some(EditorTabKind::DatabaseQuery(meta, _)) = self.tabs.get(self.active_tab).map(|tab| &tab.kind) else {
            return;
        };
        let path = crate::app::database::database_console_path(
            meta.connection_id,
            &meta.database_name,
            meta.console_id,
        );
        if let Err(error) = crate::app::database::save_database_console(&path, &self.editor.get_full_text()) {
            self.ide_panel.database.global_error = Some(format!("Не удалось сохранить SQL-консоль: {error}"));
        }
    }


}


fn database_console_initial_text(
    loaded: std::io::Result<String>,
    template: Option<&str>,
) -> Result<String, String> {
    let mut text = loaded.map_err(|error| format!("Не удалось открыть SQL-консоль: {error}"))?;
    if text.is_empty()
        && let Some(template) = template
    {
        text.push_str(template);
    }
    Ok(text)
}

#[cfg(test)]
mod round3_database_console_tests {
    use super::*;

    #[test]
    fn database_dialog_scrollbar_drag_requires_pointer_inside_track_on_both_axes() {
        let track = crate::ui_system::UiClipRect::new(760.0, 100.0, 8.0, 240.0);
        assert!(database_dialog_scrollbar_hit(track, 764.0, 220.0));
        assert!(!database_dialog_scrollbar_hit(track, 300.0, 220.0));
        assert!(!database_dialog_scrollbar_hit(track, 764.0, 360.0));
    }

    #[test]
    fn r3_107_unreadable_sql_console_does_not_become_empty_editable_text() {
        let error = database_console_initial_text(
            Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied")),
            Some("SELECT 1;"),
        )
        .unwrap_err();
        assert!(error.contains("Не удалось открыть SQL-консоль"));
        assert!(error.contains("denied"));
    }
}

fn database_dialog_field_max_bytes(field: crate::app::database::DatabaseFormField) -> usize {
    if field.is_secret() { 4096 } else { 8192 }
}

fn database_tab(
    title: String,
    extension: &str,
    kind: EditorTabKind,
    icon_key: &'static str,
    highlighted: bool,
) -> EditorTab {
    EditorTab {
        editor: Editor::new(16),
        file_path: None,
        file_key: None,
        text_file_format: crate::platform::TextFileFormat::default(),
        base_title: title,
        file_extension: extension.to_string(),
        scroll_y: ScrollState::new(15.0),
        scroll_x: ScrollState::new(15.0),
        spans: Vec::new(),
        completions: Vec::new(),
        foldable_ranges: Vec::new(),
        syntax_errors: Vec::new(),
        last_sent_version: u64::MAX,
        search_results: Vec::new(),
        search_current_idx: None,
        is_highlighted_once: highlighted,
        is_highlight_complete: highlighted,
        icon_key,
        kind,
    }
}
