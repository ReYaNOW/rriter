impl App {
    pub fn copy_database_table_selection(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
    ) {
        let Some((_, state)) = self.database_table_meta_state(tab_id) else {
            return;
        };
        let mut output = String::new();
        if !state.grid.selection.selected_rows.is_empty() {
            for (line, row_index) in state.grid.selection.selected_rows.iter().enumerate() {
                if let Some(row) = state.grid.row(*row_index) {
                    if line > 0 {
                        output.push('\n');
                    }
                    for (column, cell) in row.cells.iter().enumerate() {
                        if column > 0 {
                            output.push('\t');
                        }
                        output.push_str(&cell.value.copy_text());
                    }
                }
            }
        } else if let Some((start, end)) = state.grid.selection.cell_range() {
            for row_index in start.row..=end.row {
                if row_index > start.row {
                    output.push('\n');
                }
                if let Some(row) = state.grid.row(row_index) {
                    for column in start.column..=end.column {
                        if column > start.column {
                            output.push('\t');
                        }
                        if let Some(cell) = row.cells.get(column) {
                            output.push_str(&cell.value.copy_text());
                        }
                    }
                }
            }
        }
        if !output.is_empty() {
            self.set_clipboard_text(output);
        }
    }

    pub fn start_database_table_cell_edit(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
        position: DatabaseCellPosition,
    ) {
        let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else {
            return;
        };
        let Some(metadata) = state.metadata.as_ref() else {
            return;
        };
        let Some(column) = metadata.columns.get(position.column).cloned() else {
            return;
        };
        if !metadata.editable || !column.editable() {
            state.error = Some(if column.primary_key {
                "Редактирование primary key пока отключено".to_string()
            } else if column.type_kind == crate::app::database::DatabaseTypeKind::Bytea {
                "Редактирование bytea отключено".to_string()
            } else {
                metadata
                    .read_only_reason
                    .clone()
                    .unwrap_or_else(|| "Ячейка доступна только для чтения".to_string())
            });
            return;
        }
        let Some(value) = state
            .grid
            .row(position.row)
            .and_then(|row| row.cells.get(position.column))
            .map(|cell| cell.value.copy_text())
        else {
            return;
        };
        if column.type_kind == crate::app::database::DatabaseTypeKind::Boolean {
            if let Some(row) = state.grid.row_mut(position.row)
                && let Some(cell) = row.cells.get_mut(position.column)
            {
                let next = match cell.value {
                    DatabaseCellValue::Boolean(true) => DatabaseCellValue::Boolean(false),
                    DatabaseCellValue::Boolean(false) if column.nullable => DatabaseCellValue::Null,
                    _ => DatabaseCellValue::Boolean(true),
                };
                cell.set(next);
            }
            return;
        }
        let kind = match column.type_kind {
            crate::app::database::DatabaseTypeKind::Enum => DatabaseCellEditorKind::Enum,
            crate::app::database::DatabaseTypeKind::Date
            | crate::app::database::DatabaseTypeKind::Time
            | crate::app::database::DatabaseTypeKind::Timestamp
            | crate::app::database::DatabaseTypeKind::TimestampTz => {
                DatabaseCellEditorKind::DateTime
            }
            crate::app::database::DatabaseTypeKind::Json
            | crate::app::database::DatabaseTypeKind::Jsonb => DatabaseCellEditorKind::Multiline,
            _ if value.len() > 256 || value.contains('\n') => DatabaseCellEditorKind::Multiline,
            _ => DatabaseCellEditorKind::Inline,
        };
        if kind == DatabaseCellEditorKind::Multiline {
            self.ide_panel.database.table_modal = Some(DatabaseTableModal::MultilineEditor {
                tab_id,
                position,
                input: crate::app::database::DatabaseDialogInput::new(value),
                scroll: crate::scroll::ScrollState::new(15.0),
                error: None,
            });
        } else {
            let (calendar_year, calendar_month) =
                crate::app::database::database_calendar_year_month(&value).unwrap_or_else(|| {
                    let days = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |duration| duration.as_secs() / 86_400);
                    let (year, month, _) =
                        crate::app::database::civil_date_from_unix_days(days as i64);
                    (year, month)
                });
            state.grid.cell_editor = Some(DatabaseCellEditorState {
                position,
                kind,
                input: crate::app::database::DatabaseDialogInput::new(value),
                enum_index: 0,
                calendar_year,
                calendar_month,
                error: None,
            });
            state.grid.focused_input = Some(DatabaseTableInputTarget::Cell);
        }
    }

    pub fn commit_database_table_cell_editor(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
        literal: bool,
    ) {
        let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else {
            return;
        };
        let Some(editor) = state.grid.cell_editor.clone() else {
            return;
        };
        let Some(column) = state
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.columns.get(editor.position.column))
            .cloned()
        else {
            return;
        };
        match crate::app::database::parse_editor_value(editor.input.text(), &column, literal) {
            Ok(value) => {
                if let Some(row) = state.grid.row_mut(editor.position.row)
                    && let Some(cell) = row.cells.get_mut(editor.position.column)
                {
                    cell.set(value);
                }
                state.grid.cell_editor = None;
                state.grid.focused_input = None;
            }
            Err(error) => {
                if let Some(editor) = state.grid.cell_editor.as_mut() {
                    editor.error = Some(error);
                }
            }
        }
    }

    pub fn commit_database_table_multiline_editor(&mut self, literal: bool) {
        let Some(DatabaseTableModal::MultilineEditor {
            tab_id,
            position,
            input,
            ..
        }) = self.ide_panel.database.table_modal.as_ref()
        else {
            return;
        };
        let tab_id = *tab_id;
        let position = *position;
        let text = input.text().to_string();
        let column = self
            .database_table_meta_state(tab_id)
            .and_then(|(_, state)| {
                state
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.columns.get(position.column))
            })
            .cloned();
        let Some(column) = column else {
            return;
        };
        if matches!(column.type_kind, crate::app::database::DatabaseTypeKind::Json | crate::app::database::DatabaseTypeKind::Jsonb)
            && serde_json::from_str::<serde_json::Value>(&text).is_err()
        {
            if let Some(DatabaseTableModal::MultilineEditor { error, .. }) =
                self.ide_panel.database.table_modal.as_mut()
            {
                *error = Some("JSON содержит синтаксическую ошибку".to_string());
            }
            return;
        }
        match crate::app::database::parse_editor_value(&text, &column, literal) {
            Ok(value) => {
                if let Some((_, state)) = self.database_table_meta_state_mut(tab_id)
                    && let Some(row) = state.grid.row_mut(position.row)
                    && let Some(cell) = row.cells.get_mut(position.column)
                {
                    cell.set(value);
                }
                self.ide_panel.database.table_modal = None;
            }
            Err(message) => {
                if let Some(DatabaseTableModal::MultilineEditor { error, .. }) =
                    self.ide_panel.database.table_modal.as_mut()
                {
                    *error = Some(message);
                }
            }
        }
    }

    fn database_table_change_plan(
        &self,
        tab_id: crate::app::database::DatabaseTabId,
    ) -> Result<crate::app::database::DatabaseChangePlan, String> {
        let Some((meta, state)) = self.database_table_meta_state(tab_id) else {
            return Err("Вкладка таблицы закрыта".to_string());
        };
        let metadata = state
            .metadata
            .as_ref()
            .ok_or_else(|| "Metadata таблицы ещё не загружены".to_string())?;
        let mut operations = Vec::new();
        for chunk in state.grid.chunks.values() {
            for row in &chunk.rows {
                if row.state == DatabaseRowState::Deleted {
                    operations.push(DatabaseChangePlanOperation::Delete(row.clone()));
                } else if row.cells.iter().any(|cell| cell.dirty) {
                    operations.push(DatabaseChangePlanOperation::Update(row.clone()));
                }
            }
        }
        for row in &state.grid.added_rows {
            operations.push(DatabaseChangePlanOperation::Insert(row.clone()));
        }
        crate::app::database::build_table_change_plan(
            metadata,
            &meta.database_name,
            &meta.table_name,
            operations,
        )
    }

    pub fn preview_database_table_changes(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
    ) {
        match self.database_table_change_plan(tab_id) {
            Ok(plan) => {
                self.ide_panel.database.table_modal = Some(DatabaseTableModal::SqlPreview {
                    tab_id,
                    text: plan.preview,
                    scroll: crate::scroll::ScrollState::new(15.0),
                });
            }
            Err(error) => {
                if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
                    state.error = Some(error);
                }
            }
        }
    }

    pub fn save_database_table_changes(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
        close_after_commit: bool,
    ) {
        if self.ide_panel.database.pending_job.is_some() {
            self.ide_panel.database.global_error =
                Some("Сейчас уже выполняется другой запрос к базе данных".to_string());
            return;
        }
        let plan = match self.database_table_change_plan(tab_id) {
            Ok(plan) => plan,
            Err(error) => {
                if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
                    state.error = Some(error);
                }
                return;
            }
        };
        let Some((meta, _)) = self.database_table_meta_state(tab_id) else {
            return;
        };
        let meta = meta.clone();
        let Some(connection) = self
            .ide_panel
            .database
            .connection(meta.connection_id)
            .map(|node| node.config.clone())
        else {
            return;
        };
        if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
            state.grid.pending_close_after_save = close_after_commit;
            state.error = None;
        }
        let settings = self.ide_panel.database.settings().clone();
        let secrets = self.connection_job_secrets(meta.connection_id);
        let job_id = self.ide_panel.database.allocate_job_id();
        let pending = DatabasePendingJob {
            id: job_id,
            kind: DatabasePendingJobKind::BeginTableSave,
            connection_id: meta.connection_id,
            database_name: Some(meta.database_name.clone()),
            table_name: Some(meta.table_name.clone()),
        };
        self.ide_panel.database.table_modal = None;
        let host_key_policy = self
            .ide_panel
            .database
            .host_key_policy_override
            .take()
            .unwrap_or(SshHostKeyPolicy::Strict);
        self.send_database_command(
            DatabaseCommand::BeginTableSave {
                job_id,
                connection,
                plan,
                secrets,
                settings,
                ssh_options: crate::app::database::host_key_options(host_key_policy),
            },
            pending,
        );
    }

    pub fn commit_database_table_transaction(&mut self) {
        let Some(DatabaseTableModal::Review { tab_id, state, .. }) =
            self.ide_panel.database.table_modal.as_mut()
        else {
            return;
        };
        if state.committing {
            return;
        }
        state.committing = true;
        let transaction_id = state.transaction_id;
        let tab_id = *tab_id;
        let Some((meta, _)) = self.database_table_meta_state(tab_id) else {
            return;
        };
        let meta = meta.clone();
        let job_id = self.ide_panel.database.allocate_job_id();
        let pending = DatabasePendingJob {
            id: job_id,
            kind: DatabasePendingJobKind::CommitTransaction,
            connection_id: meta.connection_id,
            database_name: Some(meta.database_name.clone()),
            table_name: Some(meta.table_name.clone()),
        };
        self.send_database_command(
            DatabaseCommand::CommitTransaction {
                job_id,
                transaction_id,
            },
            pending,
        );
    }

    pub fn rollback_database_table_transaction(&mut self) {
        let Some(DatabaseTableModal::Review { tab_id, state, .. }) =
            self.ide_panel.database.table_modal.as_ref()
        else {
            return;
        };
        let transaction_id = state.transaction_id;
        let tab_id = *tab_id;
        let Some((meta, _)) = self.database_table_meta_state(tab_id) else {
            return;
        };
        let meta = meta.clone();
        let job_id = self.ide_panel.database.allocate_job_id();
        let pending = DatabasePendingJob {
            id: job_id,
            kind: DatabasePendingJobKind::RollbackTransaction,
            connection_id: meta.connection_id,
            database_name: Some(meta.database_name.clone()),
            table_name: Some(meta.table_name.clone()),
        };
        self.send_database_command(
            DatabaseCommand::RollbackTransaction {
                job_id,
                transaction_id,
            },
            pending,
        );
    }

    pub fn request_database_table_refresh(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
    ) {
        self.request_database_table_reload(
            tab_id,
            crate::app::database::DatabaseTableReloadAction::Refresh,
        );
    }

    pub(crate) fn request_database_table_close(&mut self, idx: usize) -> bool {
        let Some((tab_id, dirty)) = self.tabs.get(idx).and_then(|tab| match &tab.kind {
            EditorTabKind::DatabaseTable(meta, state) => Some((meta.tab_id, state.grid.dirty())),
            _ => None,
        }) else {
            return false;
        };
        if !dirty {
            return false;
        }
        if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
            state.grid.pending_reload = None;
        }
        self.ide_panel.database.table_modal = Some(DatabaseTableModal::RefreshPrompt {
            tab_id,
            close_after_save: true,
        });
        true
    }

    pub fn resolve_database_table_refresh_prompt(&mut self, action: usize) {
        let Some(DatabaseTableModal::RefreshPrompt {
            tab_id,
            close_after_save,
        }) = self.ide_panel.database.table_modal.as_ref()
        else {
            return;
        };
        let tab_id = *tab_id;
        let close_after_save = *close_after_save;
        match action {
            0 => self.save_database_table_changes(tab_id, close_after_save),
            2 => {
                self.ide_panel.database.table_modal = None;
                let pending_reload = self
                    .database_table_meta_state_mut(tab_id)
                    .and_then(|(_, state)| state.grid.pending_reload.take());
                self.discard_database_table_local_changes(tab_id);
                if close_after_save {
                    if let Some(index) = self.database_table_index(tab_id) {
                        self.close_tab_at(index);
                    }
                } else {
                    self.apply_database_table_reload(
                        tab_id,
                        pending_reload.unwrap_or(
                            crate::app::database::DatabaseTableReloadAction::Refresh,
                        ),
                    );
                }
            }
            _ => {
                self.ide_panel.database.table_modal = None;
                if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
                    state.grid.pending_reload = None;
                    state.grid.where_input.set_text(state.grid.view.where_clause.clone());
                    state.grid.order_by_input.set_text(state.grid.view.order_by.clone());
                }
            }
        }
    }

    pub(crate) fn finish_committed_database_table(
        &mut self,
        connection_id: crate::app::database::DatabaseConnectionId,
        database_name: &str,
        table_name: &str,
    ) {
        let tab_id = self.tabs.iter().find_map(|tab| match &tab.kind {
            EditorTabKind::DatabaseTable(meta, _)
                if meta.connection_id == connection_id
                    && meta.database_name == database_name
                    && meta.table_name == table_name =>
            {
                Some(meta.tab_id)
            }
            _ => None,
        });
        let Some(tab_id) = tab_id else {
            self.ide_panel.database.table_modal = None;
            return;
        };
        let close_after = matches!(
            self.ide_panel.database.table_modal.as_ref(),
            Some(DatabaseTableModal::Review { tab_id: modal_tab_id, state, .. })
                if *modal_tab_id == tab_id && state.close_after_commit
        );
        self.ide_panel.database.table_modal = None;
        let pending_reload = if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
            if let Some(metadata) = state.metadata.clone() {
                state.grid.prepare_selection_restore(&metadata);
            }
            state.grid.added_rows.clear();
            state.grid.clear_loaded_rows();
            // Both ScrollState values stay untouched. The next autocommit chunk is selected
            // from the current vertical viewport and logical selection is restored by PK.
            state.grid.pending_close_after_save = false;
            state.grid.post_commit_refresh_pending = true;
            state.error = None;
            state.grid.pending_reload.take()
        } else {
            None
        };
        if close_after {
            if let Some(index) = self.database_table_index(tab_id) {
                self.close_tab_at(index);
            }
        } else if let Some(action) = pending_reload {
            self.apply_database_table_reload(tab_id, action);
        } else {
            self.queue_database_table_initial_load(tab_id);
        }
    }

    pub(crate) fn finish_rolled_back_database_table(&mut self) {
        self.ide_panel.database.table_modal = None;
        self.ide_panel.database.notice = Some("Транзакция отменена; локальные изменения сохранены".to_string());
    }

    pub(crate) fn auto_size_database_table_column(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
        column_index: usize,
    ) {
        let width = self.database_table_meta_state(tab_id).and_then(|(_, state)| {
            let metadata = state.metadata.as_ref()?;
            let column = metadata.columns.get(column_index)?;
            let mut max_chars = column.name.chars().count().saturating_add(3);
            for chunk in state.grid.chunks.values() {
                for row in chunk.rows.iter().take(100) {
                    if let Some(cell) = row.cells.get(column_index) {
                        max_chars = max_chars.max(cell.value.display_text().chars().count().min(160));
                    }
                }
            }
            Some((max_chars as f32 * 8.0 + 24.0).clamp(
                crate::app::database::DATABASE_GRID_MIN_COLUMN_WIDTH,
                crate::app::database::DATABASE_GRID_MAX_COLUMN_WIDTH,
            ))
        });
        let Some(width) = width else { return; };
        if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
            let Some(metadata) = state.metadata.as_ref() else { return; };
            let Some(column) = metadata.columns.get(column_index) else { return; };
            state.grid.set_column_width(&column.name, width);
        }
        self.persist_database_table_view(tab_id);
    }

    pub(crate) fn start_database_table_column_resize(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
        column_index: usize,
        mouse_x: f32,
    ) {
        if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
            let Some(column) = state
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.columns.get(column_index))
            else {
                return;
            };
            let width = state.grid.column_width(&column.name);
            state.grid.column_resize = Some((column_index, mouse_x, width));
        }
    }

    pub(crate) fn update_database_table_drag(&mut self, mouse_x: f32, mouse_y: f32) -> bool {
        let Some(tab_id) = self.active_database_table_tab_id() else { return false; };
        let vertical_rect = self.ui_registry.rect_for(crate::ui_system::UiId::DatabaseTableScrollY);
        let horizontal_rect = self.ui_registry.rect_for(crate::ui_system::UiId::DatabaseTableScrollX);
        let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else { return false; };
        if let Some((column_index, start_x, start_width)) = state.grid.column_resize {
            if let Some(column) = state.metadata.as_ref().and_then(|metadata| metadata.columns.get(column_index)).cloned() {
                state.grid.set_column_width(&column.name, start_width + mouse_x - start_x);
                return true;
            }
        }
        if state.grid.scroll_y.is_dragging {
            let Some((_, rect_y, _, rect_h)) = vertical_rect else { return false; };
            let max_scroll = (state.grid.logical_row_count() as f32
                * crate::app::database::DATABASE_GRID_ROW_HEIGHT
                - state.grid.viewport_height).max(0.0);
            let ratio = ((mouse_y - rect_y) / rect_h.max(1.0)).clamp(0.0, 1.0);
            state.grid.scroll_y.target = ratio * max_scroll;
            state.grid.scroll_y.current = state.grid.scroll_y.target;
            return true;
        }
        if state.grid.scroll_x.is_dragging {
            let Some((rect_x, _, rect_w, _)) = horizontal_rect else { return false; };
            let content = state.metadata.as_ref().map_or(0.0, |metadata| state.grid.content_width(metadata));
            let max_scroll = (content - state.grid.viewport_width).max(0.0);
            let ratio = ((mouse_x - rect_x) / rect_w.max(1.0)).clamp(0.0, 1.0);
            state.grid.scroll_x.target = ratio * max_scroll;
            state.grid.scroll_x.current = state.grid.scroll_x.target;
            return true;
        }
        false
    }

    pub(crate) fn handle_database_table_key(
        &mut self,
        key_event: &winit::event::KeyEvent,
    ) -> bool {
        use winit::event::ElementState;
        use winit::keyboard::{KeyCode, PhysicalKey};

        let has_modal = self.ide_panel.database.table_modal.is_some();
        let has_table = self.active_database_table_tab_id().is_some();
        if !has_modal && !has_table {
            return false;
        }
        if key_event.state != ElementState::Pressed {
            return true;
        }

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

        if self.ide_panel.database.table_modal.is_some() {
            match key_event.physical_key {
                PhysicalKey::Code(KeyCode::Escape) => self.activate_database_table_modal_action(1),
                PhysicalKey::Code(KeyCode::Enter | KeyCode::NumpadEnter) => {
                    if matches!(
                        self.ide_panel.database.table_modal,
                        Some(DatabaseTableModal::MultilineEditor { .. })
                    ) && !primary
                    {
                        if let Some(DatabaseTableModal::MultilineEditor { input, error, .. }) =
                            self.ide_panel.database.table_modal.as_mut()
                        {
                            input.insert("\n", crate::app::database::MAX_EDITABLE_MULTILINE_BYTES);
                            *error = None;
                        }
                    } else {
                        self.activate_database_table_modal_action(0);
                    }
                }
                physical_key => {
                    let multiline = matches!(
                        self.ide_panel.database.table_modal,
                        Some(DatabaseTableModal::MultilineEditor { .. })
                    );
                    if let Some(input) = database_table_modal_input_mut(
                        &mut self.ide_panel.database.table_modal,
                    ) {
                        copy_text = edit_database_table_input(
                            input,
                            physical_key,
                            key_event.logical_key.to_text(),
                            primary,
                            word,
                            shift,
                            text_input_allowed,
                            paste_text,
                            if multiline {
                                crate::app::database::MAX_EDITABLE_MULTILINE_BYTES
                            } else {
                                16
                            },
                            multiline,
                        );
                    }
                }
            }
            if let Some(text) = copy_text {
                self.set_clipboard_text(text);
            }
            return true;
        }

        let Some(tab_id) = self.active_database_table_tab_id() else {
            return false;
        };
        let focus = self
            .database_table_meta_state(tab_id)
            .and_then(|(_, state)| state.grid.focused_input);
        match key_event.physical_key {
            PhysicalKey::Code(KeyCode::Escape) => {
                if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
                    state.grid.cell_editor = None;
                    state.grid.focused_input = None;
                }
            }
            PhysicalKey::Code(KeyCode::Enter | KeyCode::NumpadEnter) => match focus {
                Some(DatabaseTableInputTarget::Where | DatabaseTableInputTarget::OrderBy) => {
                    self.apply_database_table_filters(tab_id)
                }
                Some(DatabaseTableInputTarget::Cell) => {
                    self.commit_database_table_cell_editor(tab_id, primary)
                }
                None => return false,
            },
            PhysicalKey::Code(KeyCode::KeyC) if primary && focus.is_none() => {
                self.copy_database_table_selection(tab_id)
            }
            PhysicalKey::Code(KeyCode::KeyZ) if primary && focus.is_none() => {
                self.undo_database_table_selection(tab_id)
            }
            PhysicalKey::Code(KeyCode::Delete) if focus.is_none() => {
                self.delete_database_table_selection(tab_id)
            }
            PhysicalKey::Code(KeyCode::Insert) if focus.is_none() => {
                self.add_database_table_row(tab_id)
            }
            physical_key => {
                let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else {
                    return false;
                };
                let input = match focus {
                    Some(DatabaseTableInputTarget::Where) => Some(&mut state.grid.where_input),
                    Some(DatabaseTableInputTarget::OrderBy) => {
                        Some(&mut state.grid.order_by_input)
                    }
                    Some(DatabaseTableInputTarget::Cell) => state
                        .grid
                        .cell_editor
                        .as_mut()
                        .map(|editor| &mut editor.input),
                    None => None,
                };
                let Some(input) = input else {
                    return false;
                };
                copy_text = edit_database_table_input(
                    input,
                    physical_key,
                    key_event.logical_key.to_text(),
                    primary,
                    word,
                    shift,
                    text_input_allowed,
                    paste_text,
                    if matches!(focus, Some(DatabaseTableInputTarget::Cell)) {
                        crate::app::database::MAX_EDITABLE_MULTILINE_BYTES
                    } else {
                        64 * 1024
                    },
                    false,
                );
            }
        }
        if let Some(text) = copy_text {
            self.set_clipboard_text(text);
        }
        true
    }

    pub(crate) fn handle_database_table_cell_click(&mut self, row: usize, column: usize) {
        let Some(tab_id) = self.active_database_table_tab_id() else { return; };
        let now = std::time::Instant::now();
        let position = self.renderer.as_ref().map_or((0.0, 0.0), |renderer| (renderer.last_mouse_x, renderer.last_mouse_y));
        let double = now.duration_since(self.last_click_time).as_millis() < 400
            && (position.0 - self.last_click_pos.0).powi(2) + (position.1 - self.last_click_pos.1).powi(2) < 25.0;
        self.last_click_time = now;
        self.last_click_pos = position;
        let extend = self.modifiers.shift_key();
        if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
            state.grid.selection.select_cell(DatabaseCellPosition { row, column }, extend);
            state.grid.focused_input = None;
        }
        if double {
            self.start_database_table_cell_edit(tab_id, DatabaseCellPosition { row, column });
        }
    }

    pub(crate) fn select_database_table_enum_option(&mut self, option: usize) {
        let Some(tab_id) = self.active_database_table_tab_id() else { return; };
        let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else { return; };
        let Some(editor) = state.grid.cell_editor.as_mut() else { return; };
        let Some(column) = state.metadata.as_ref().and_then(|metadata| metadata.columns.get(editor.position.column)) else { return; };
        let Some(value) = column.enum_values.get(option).cloned() else { return; };
        editor.input.set_text(value);
        self.commit_database_table_cell_editor(tab_id, false);
    }

    pub(crate) fn shift_database_table_calendar_month(&mut self, delta: i32) {
        let Some(tab_id) = self.active_database_table_tab_id() else { return; };
        let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else { return; };
        let Some(editor) = state.grid.cell_editor.as_mut() else { return; };
        let (year, month) = crate::app::database::database_shift_calendar_month(
            editor.calendar_year,
            editor.calendar_month,
            delta,
        );
        editor.calendar_year = year;
        editor.calendar_month = month;
    }

    pub(crate) fn select_database_table_calendar_day(&mut self, day: u32) {
        let Some(tab_id) = self.active_database_table_tab_id() else { return; };
        let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else { return; };
        let Some(editor) = state.grid.cell_editor.as_mut() else { return; };
        if day == 0
            || day > crate::app::database::database_days_in_month(
                editor.calendar_year,
                editor.calendar_month,
            )
        {
            return;
        }
        let current = editor.input.text().to_string();
        let suffix = current.get(10..).filter(|_| {
            crate::app::database::database_calendar_year_month(&current).is_some()
        });
        editor.input.set_text(format!(
            "{:04}-{:02}-{day:02}{}",
            editor.calendar_year,
            editor.calendar_month,
            suffix.unwrap_or("")
        ));
    }

    pub(crate) fn set_database_table_date_today(&mut self) {
        let Some(tab_id) = self.active_database_table_tab_id() else { return; };
        let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else { return; };
        let Some(editor) = state.grid.cell_editor.as_mut() else { return; };
        let days = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs() / 86_400);
        let (year, month, day) = crate::app::database::civil_date_from_unix_days(days as i64);
        let current = editor.input.text().to_string();
        let suffix = current.get(10..).filter(|_| {
            crate::app::database::database_calendar_year_month(&current).is_some()
        });
        editor.calendar_year = year;
        editor.calendar_month = month;
        editor.input.set_text(format!(
            "{year:04}-{month:02}-{day:02}{}",
            suffix.unwrap_or("")
        ));
    }

    pub(crate) fn set_database_table_time_now_utc(&mut self) {
        let Some(tab_id) = self.active_database_table_tab_id() else { return; };
        let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else { return; };
        let Some(metadata) = state.metadata.as_ref() else { return; };
        let Some(editor) = state.grid.cell_editor.as_mut() else { return; };
        let Some(column) = metadata.columns.get(editor.position.column) else { return; };
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs());
        let days = (seconds / 86_400) as i64;
        let seconds_in_day = seconds % 86_400;
        let hour = seconds_in_day / 3_600;
        let minute = (seconds_in_day % 3_600) / 60;
        let second = seconds_in_day % 60;
        let (year, month, day) = crate::app::database::civil_date_from_unix_days(days);
        editor.calendar_year = year;
        editor.calendar_month = month;
        let text = match column.type_kind {
            crate::app::database::DatabaseTypeKind::Time => {
                format!("{hour:02}:{minute:02}:{second:02}")
            }
            crate::app::database::DatabaseTypeKind::TimestampTz => {
                format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}+00")
            }
            _ => format!(
                "{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}"
            ),
        };
        editor.input.set_text(text);
    }

    pub(crate) fn start_database_table_scroll_drag(&mut self, horizontal: bool) {
        let Some(tab_id) = self.active_database_table_tab_id() else { return; };
        let mouse = self.renderer.as_ref().map_or((0.0, 0.0), |renderer| (renderer.last_mouse_x, renderer.last_mouse_y));
        let vertical_rect = self.ui_registry.rect_for(crate::ui_system::UiId::DatabaseTableScrollY);
        let horizontal_rect = self.ui_registry.rect_for(crate::ui_system::UiId::DatabaseTableScrollX);
        let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else { return; };
        if horizontal {
            state.grid.scroll_x.is_dragging = true;
            state.grid.scroll_x.drag_offset = mouse.0 - horizontal_rect.map_or(mouse.0, |rect| rect.0);
        } else {
            state.grid.scroll_y.is_dragging = true;
            state.grid.scroll_y.drag_offset = mouse.1 - vertical_rect.map_or(mouse.1, |rect| rect.1);
        }
    }

    pub(crate) fn activate_database_table_modal_action(&mut self, action: usize) {
        let Some(modal) = self.ide_panel.database.table_modal.clone() else { return; };
        match modal {
            DatabaseTableModal::SqlPreview { .. } => {
                self.ide_panel.database.table_modal = None;
            }
            DatabaseTableModal::RefreshPrompt { .. } => {
                self.resolve_database_table_refresh_prompt(action);
            }
            DatabaseTableModal::CustomLimit { .. } => {
                if action == 0 { self.apply_database_table_limit_dialog(); }
                else { self.ide_panel.database.table_modal = None; }
            }
            DatabaseTableModal::MultilineEditor { .. } => match action {
                0 => self.commit_database_table_multiline_editor(false),
                2 => self.commit_database_table_multiline_editor(true),
                _ => self.ide_panel.database.table_modal = None,
            },
            DatabaseTableModal::Review { .. } => {
                if action == 0 { self.commit_database_table_transaction(); }
                else { self.rollback_database_table_transaction(); }
            }
        }
    }

    pub(crate) fn finish_database_table_drag(&mut self) {
        let Some(tab_id) = self.active_database_table_tab_id() else {
            return;
        };
        let persist = if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
            let resized = state.grid.column_resize.take().is_some();
            state.grid.scroll_x.is_dragging = false;
            state.grid.scroll_y.is_dragging = false;
            resized
        } else {
            false
        };
        if persist {
            self.persist_database_table_view(tab_id);
        }
    }
}

fn database_table_modal_input_mut(
    modal: &mut Option<DatabaseTableModal>,
) -> Option<&mut crate::app::database::DatabaseDialogInput> {
    match modal.as_mut()? {
        DatabaseTableModal::CustomLimit { input, .. }
        | DatabaseTableModal::MultilineEditor { input, .. } => Some(input),
        _ => None,
    }
}

fn edit_database_table_input(
    input: &mut crate::app::database::DatabaseDialogInput,
    physical_key: winit::keyboard::PhysicalKey,
    logical_text: Option<&str>,
    primary: bool,
    word: bool,
    shift: bool,
    text_input_allowed: bool,
    paste_text: Option<String>,
    max_bytes: usize,
    multiline: bool,
) -> Option<String> {
    use winit::keyboard::{KeyCode, PhysicalKey};
    match physical_key {
        PhysicalKey::Code(KeyCode::KeyA | KeyCode::KeyF) if primary => {
            input.select_all();
            None
        }
        PhysicalKey::Code(KeyCode::KeyC) if primary => input.selected_text().map(str::to_owned),
        PhysicalKey::Code(KeyCode::KeyX) if primary => {
            let selected = input.selected_text().map(str::to_owned);
            if selected.is_some() {
                input.delete_selection();
            }
            selected
        }
        PhysicalKey::Code(KeyCode::KeyV) if primary => {
            if let Some(text) = paste_text {
                let clean = if multiline {
                    text
                } else {
                    text.replace(['\n', '\r'], "")
                };
                input.insert(&clean, max_bytes);
            }
            None
        }
        PhysicalKey::Code(KeyCode::Backspace) => {
            if word {
                input.delete_word_backward();
            } else {
                input.backspace();
            }
            None
        }
        PhysicalKey::Code(KeyCode::Delete) => {
            if word {
                input.delete_word_forward();
            } else {
                input.delete_forward();
            }
            None
        }
        PhysicalKey::Code(KeyCode::ArrowLeft) => {
            if word {
                input.move_word_left(shift);
            } else {
                input.move_left(shift);
            }
            None
        }
        PhysicalKey::Code(KeyCode::ArrowRight) => {
            if word {
                input.move_word_right(shift);
            } else {
                input.move_right(shift);
            }
            None
        }
        PhysicalKey::Code(KeyCode::Home) => {
            input.move_home(shift);
            None
        }
        PhysicalKey::Code(KeyCode::End) => {
            input.move_end(shift);
            None
        }
        _ if text_input_allowed => {
            if let Some(text) = logical_text {
                let clean = if multiline {
                    text.to_string()
                } else {
                    text.replace(['\n', '\r'], "")
                };
                if !clean.is_empty() {
                    input.insert(&clean, max_bytes);
                }
            }
            None
        }
        _ => None,
    }
}
