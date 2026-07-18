#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DatabaseDragUpdate {
    None,
    Query,
    Table(crate::app::database::DatabaseTabId),
}

impl DatabaseDragUpdate {
    pub(crate) fn table_tab_id(self) -> Option<crate::app::database::DatabaseTabId> {
        match self {
            Self::Table(tab_id) => Some(tab_id),
            Self::None | Self::Query => None,
        }
    }

    pub(crate) fn changed(self) -> bool {
        !matches!(self, Self::None)
    }
}

#[allow(clippy::too_many_arguments)]
fn database_table_scroll_drag_target(
    pointer: f32,
    track_start: f32,
    track_len: f32,
    viewport_len: f32,
    content_len: f32,
    current_scroll: f32,
    min_thumb_len: f32,
    drag_offset: Option<f32>,
    scale: f32,
) -> Option<(f32, f32)> {
    let scale = scale.max(f32::EPSILON);
    let max_scroll = (content_len - viewport_len).max(0.0);
    let thumb = crate::scroll::scrollbar_thumb(
        track_start,
        track_len,
        viewport_len,
        content_len,
        current_scroll * scale,
        min_thumb_len,
    )?;
    let (offset, target) = crate::scroll::scrollbar_drag_target(
        pointer,
        track_start,
        track_len,
        thumb,
        max_scroll,
        drag_offset,
    )?;
    Some((offset, target / scale))
}

impl App {
    pub(crate) fn database_table_unavailable_text_index_at(
        &mut self,
        mouse_x: f32,
    ) -> Option<usize> {
        let tab_id = self.active_database_table_tab_id()?;
        let (text, cursor) = {
            let (_, state) = self.database_table_meta_state(tab_id)?;
            if state.loading || state.metadata.is_some() {
                return None;
            }
            (
                state.unavailable_text.text().to_string(),
                state.unavailable_text.cursor,
            )
        };
        let rect = self
            .ui_registry
            .rect_for(crate::ui_system::UiId::DatabaseTableUnavailableText)?;
        let renderer = self.renderer.as_mut()?;
        let text_scale = 0.76;
        let visible_width = rect.2.max(1.0);
        let scroll_x = crate::app::file_tree::file_tree_name_input_scroll_x(
            &text,
            cursor,
            visible_width,
            |ch| {
                renderer
                    .get_ui_glyph(ch)
                    .map(|glyph| {
                        crate::renderer::Renderer::snapped_text_advance(
                            glyph.advance,
                            text_scale,
                        )
                    })
                    .unwrap_or_else(|| (8.0 * text_scale).round().max(1.0))
            },
        );
        let x_offset = (mouse_x - rect.0 + scroll_x).max(0.0);
        Some(crate::app::file_tree::file_tree_name_input_hit_index(
            &text,
            x_offset,
            |ch| {
                renderer
                    .get_ui_glyph(ch)
                    .map(|glyph| {
                        crate::renderer::Renderer::snapped_text_advance(
                            glyph.advance,
                            text_scale,
                        )
                    })
                    .unwrap_or_else(|| (8.0 * text_scale).round().max(1.0))
            },
        ))
    }

    pub(crate) fn set_database_table_unavailable_text_cursor(
        &mut self,
        target_index: usize,
        selecting: bool,
    ) {
        let Some(tab_id) = self.active_database_table_tab_id() else {
            return;
        };
        let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else {
            return;
        };
        if state.loading || state.metadata.is_some() {
            return;
        }
        state
            .unavailable_text
            .set_cursor(target_index, selecting);
        state.unavailable_text_focused = true;
    }

    pub(crate) fn database_table_input_index_at(
        &mut self,
        target: DatabaseTableInputTarget,
        mouse_x: f32,
    ) -> Option<usize> {
        let tab_id = self.active_database_table_tab_id()?;
        let (text, cursor, id) = {
            let (_, state) = self.database_table_meta_state(tab_id)?;
            let input = match target {
                DatabaseTableInputTarget::Where => &state.grid.where_input,
                DatabaseTableInputTarget::OrderBy => &state.grid.order_by_input,
                DatabaseTableInputTarget::Cell => &state.grid.cell_editor.as_ref()?.input,
            };
            let id = match target {
                DatabaseTableInputTarget::Where => crate::ui_system::UiId::DatabaseTableWhereInput,
                DatabaseTableInputTarget::OrderBy => crate::ui_system::UiId::DatabaseTableOrderInput,
                DatabaseTableInputTarget::Cell => crate::ui_system::UiId::DatabaseTableCellEditor,
            };
            (input.text().to_string(), input.cursor, id)
        };
        let rect = self.ui_registry.rect_for(id)?;
        let renderer = self.renderer.as_mut()?;
        let scale = renderer.scale_factor;
        let text_scale = crate::app::database::DATABASE_TABLE_INPUT_TEXT_SCALE;
        let padding = if target == DatabaseTableInputTarget::Cell {
            (8.0 * scale).round()
        } else {
            (10.0 * scale).round()
        };
        let visible_width = (rect.2 - padding * 2.0).max(1.0);
        let scroll_x = crate::app::file_tree::file_tree_name_input_scroll_x(
            &text,
            cursor,
            visible_width,
            |ch| {
                renderer
                    .get_ui_glyph(ch)
                    .map(|glyph| crate::renderer::Renderer::snapped_text_advance(glyph.advance, text_scale))
                    .unwrap_or_else(|| (8.0 * text_scale).round().max(1.0))
            },
        );
        let x_offset = (mouse_x - rect.0 - padding + scroll_x).max(0.0);
        Some(crate::app::file_tree::file_tree_name_input_hit_index(
            &text,
            x_offset,
            |ch| {
                renderer
                    .get_ui_glyph(ch)
                    .map(|glyph| crate::renderer::Renderer::snapped_text_advance(glyph.advance, text_scale))
                    .unwrap_or_else(|| (8.0 * text_scale).round().max(1.0))
            },
        ))
    }

    pub(crate) fn database_table_modal_input_index_at(
        &mut self,
        mouse_x: f32,
        mouse_y: f32,
    ) -> Option<usize> {
        enum ModalInputSnapshot {
            SingleLine { text: String, cursor: usize },
            SqlPreview {
                text: String,
                scroll_x: f32,
                scroll_y: f32,
            },
        }
        let snapshot = match self.ide_panel.database.table_modal.as_ref()? {
            DatabaseTableModal::CustomLimit { input, .. } => ModalInputSnapshot::SingleLine {
                text: input.text().to_string(),
                cursor: input.cursor,
            },
            DatabaseTableModal::SqlPreview {
                text,
                scroll_x,
                scroll_y,
                ..
            } => ModalInputSnapshot::SqlPreview {
                text: text.clone(),
                scroll_x: scroll_x.current,
                scroll_y: scroll_y.current,
            },
            _ => return None,
        };
        let rect = self
            .ui_registry
            .rect_for(crate::ui_system::UiId::DatabaseTableModalInput)?;
        let renderer = self.renderer.as_mut()?;
        let scale = renderer.scale_factor;
        match snapshot {
            ModalInputSnapshot::SingleLine { text, cursor } => {
                let text_scale = 0.82;
                let visible_width = (rect.2 - 16.0 * scale).max(1.0);
                let scroll_x = crate::app::file_tree::file_tree_name_input_scroll_x(
                    &text,
                    cursor,
                    visible_width,
                    |ch| {
                        renderer
                            .get_ui_glyph(ch)
                            .map(|glyph| {
                                crate::renderer::Renderer::snapped_text_advance(
                                    glyph.advance,
                                    text_scale,
                                )
                            })
                            .unwrap_or_else(|| (10.0 * text_scale).round().max(1.0))
                    },
                );
                let x_offset = (mouse_x - rect.0 - 8.0 * scale + scroll_x).max(0.0);
                Some(crate::app::file_tree::file_tree_name_input_hit_index(
                    &text,
                    x_offset,
                    |ch| {
                        renderer
                            .get_ui_glyph(ch)
                            .map(|glyph| {
                                crate::renderer::Renderer::snapped_text_advance(
                                    glyph.advance,
                                    text_scale,
                                )
                            })
                            .unwrap_or_else(|| (10.0 * text_scale).round().max(1.0))
                    },
                ))
            }
            ModalInputSnapshot::SqlPreview {
                text,
                scroll_x,
                scroll_y,
            } => {
                let line_h = (crate::app::database::DATABASE_SQL_PREVIEW_LINE_HEIGHT * scale)
                    .round()
                    .max(1.0);
                let line_index = ((mouse_y - rect.1 + scroll_y).max(0.0) / line_h)
                    .floor() as usize;
                let mut line_start = 0usize;
                let mut selected_line = None;
                for (index, raw_line) in text.split_inclusive('\n').enumerate() {
                    if index == line_index {
                        selected_line = Some(raw_line.trim_end_matches(&['\r', '\n'][..]));
                        break;
                    }
                    line_start = line_start.saturating_add(raw_line.len());
                }
                let Some(line) = selected_line else {
                    return Some(text.len());
                };
                let x_offset = (mouse_x - rect.0 - 8.0 * scale + scroll_x).max(0.0);
                let within_line = crate::app::file_tree::file_tree_name_input_hit_index(
                    line,
                    x_offset,
                    |ch| {
                        renderer
                            .get_glyph(ch)
                            .map(|glyph| glyph.advance.round().max(1.0))
                            .unwrap_or_else(|| (9.0 * scale).round().max(1.0))
                    },
                );
                Some((line_start + within_line).min(text.len()))
            }
        }
    }

    pub(crate) fn set_database_table_modal_input_cursor(
        &mut self,
        target_index: usize,
        selecting: bool,
    ) {
        match self.ide_panel.database.table_modal.as_mut() {
            Some(DatabaseTableModal::CustomLimit { input, .. }) => {
                input.set_cursor(target_index, selecting);
            }
            Some(DatabaseTableModal::SqlPreview {
                text,
                cursor,
                selection_anchor,
                ..
            }) => {
                let mut target = target_index.min(text.len());
                while target > 0 && !text.is_char_boundary(target) {
                    target -= 1;
                }
                let old_cursor = *cursor;
                *cursor = target;
                if selecting {
                    if selection_anchor.is_none() {
                        *selection_anchor = Some(old_cursor);
                    }
                } else {
                    *selection_anchor = None;
                }
            }
            _ => return,
        }
        self.last_action = std::time::Instant::now();
        self.last_blink_state = true;
    }

    pub(crate) fn set_database_table_input_cursor(
        &mut self,
        target: DatabaseTableInputTarget,
        target_index: usize,
        selecting: bool,
    ) {
        let Some(tab_id) = self.active_database_table_tab_id() else {
            return;
        };
        let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else {
            return;
        };
        let input = match target {
            DatabaseTableInputTarget::Where => Some(&mut state.grid.where_input),
            DatabaseTableInputTarget::OrderBy => Some(&mut state.grid.order_by_input),
            DatabaseTableInputTarget::Cell => state
                .grid
                .cell_editor
                .as_mut()
                .map(|editor| &mut editor.input),
        };
        if let Some(input) = input {
            input.set_cursor(target_index, selecting);
            state.grid.focused_input = Some(target);
            self.last_action = std::time::Instant::now();
            self.last_blink_state = true;
        }
    }

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
            for (line, row_index) in state
                .grid
                .row_indices_between(start.row, end.row)
                .into_iter()
                .enumerate()
            {
                if line > 0 {
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
        self.last_action = std::time::Instant::now();
        self.last_blink_state = true;
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
            let enum_index = if kind == DatabaseCellEditorKind::Enum {
                column
                    .enum_values
                    .iter()
                    .position(|option| option == &value)
                    .unwrap_or(0)
            } else {
                0
            };
            state.grid.cell_editor = Some(DatabaseCellEditorState {
                position,
                kind,
                input: crate::app::database::DatabaseDialogInput::new(value),
                enum_index,
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
            if row.state == DatabaseRowState::Added {
                operations.push(DatabaseChangePlanOperation::Insert(row.clone()));
            }
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
                let text = crate::app::database::format_database_sql(&plan.preview)
                    .unwrap_or(plan.preview);
                self.ide_panel.database.table_modal = Some(DatabaseTableModal::SqlPreview {
                    tab_id,
                    spans: crate::highlighter::highlight_sql_text(&text),
                    text,
                    cursor: 0,
                    selection_anchor: None,
                    scroll_x: crate::scroll::ScrollState::new(15.0),
                    scroll_y: crate::scroll::ScrollState::new(15.0),
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
            if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
                state.show_timed_notice(
                    "Сейчас уже выполняется другой запрос к базе данных",
                );
                state.error = None;
            }
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
            owner: crate::app::database::DatabaseJobOwner::Table(meta.tab_id),
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

    fn finish_database_table_transaction(&mut self, commit: bool) {
        let Some(DatabaseTableModal::Review { tab_id, state, .. }) =
            self.ide_panel.database.table_modal.as_ref()
        else {
            return;
        };
        if !database_table_transaction_finish_allowed(state.committing) {
            return;
        }
        let transaction_id = state.transaction_id;
        let tab_id = *tab_id;
        let Some((meta, _)) = self.database_table_meta_state(tab_id) else {
            return;
        };
        let meta = meta.clone();
        if let Some(DatabaseTableModal::Review { state, .. }) =
            self.ide_panel.database.table_modal.as_mut()
        {
            state.committing = true;
        }
        let job_id = self.ide_panel.database.allocate_job_id();
        let kind = if commit {
            DatabasePendingJobKind::CommitTransaction
        } else {
            DatabasePendingJobKind::RollbackTransaction
        };
        let pending = DatabasePendingJob {
            id: job_id,
            kind,
            owner: crate::app::database::DatabaseJobOwner::Table(meta.tab_id),
            connection_id: meta.connection_id,
            database_name: Some(meta.database_name.clone()),
            table_name: Some(meta.table_name.clone()),
        };
        let command = if commit {
            DatabaseCommand::CommitTransaction {
                job_id,
                transaction_id,
            }
        } else {
            DatabaseCommand::RollbackTransaction {
                job_id,
                transaction_id,
            }
        };
        self.send_database_command(command, pending);
    }

    pub fn commit_database_table_transaction(&mut self) {
        self.finish_database_table_transaction(true);
    }

    pub fn rollback_database_table_transaction(&mut self) {
        self.finish_database_table_transaction(false);
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

    pub(crate) fn update_database_table_drag(
        &mut self,
        mouse_x: f32,
        mouse_y: f32,
    ) -> DatabaseDragUpdate {
        if self.update_database_sql_preview_scroll_drag(mouse_x, mouse_y) {
            return DatabaseDragUpdate::Query;
        }
        let Some(tab_id) = self.active_database_table_tab_id() else {
            return if self.update_database_query_scroll_drag(mouse_x, mouse_y) {
                DatabaseDragUpdate::Query
            } else {
                DatabaseDragUpdate::None
            };
        };
        let vertical_rect = self
            .ui_registry
            .rect_for(crate::ui_system::UiId::DatabaseTableScrollY);
        let horizontal_rect = self
            .ui_registry
            .rect_for(crate::ui_system::UiId::DatabaseTableScrollX);
        let scale = self
            .renderer
            .as_ref()
            .map_or(1.0, |renderer| renderer.scale_factor)
            .max(f32::EPSILON);
        let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else {
            return DatabaseDragUpdate::None;
        };
        if let Some((column_index, start_x, start_width)) = state.grid.column_resize
            && let Some(column) = state
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.columns.get(column_index))
                .cloned()
        {
            state
                .grid
                .set_column_width(&column.name, start_width + mouse_x - start_x);
            return DatabaseDragUpdate::Table(tab_id);
        }
        if state.grid.scroll_y.is_dragging {
            let Some((_, rect_y, _, rect_h)) = vertical_rect else {
                return DatabaseDragUpdate::None;
            };
            let row_h = (crate::app::database::DATABASE_GRID_ROW_HEIGHT * scale).round();
            let content_h = state.grid.logical_row_count() as f32 * row_h;
            let Some((_, target)) = database_table_scroll_drag_target(
                mouse_y,
                rect_y,
                rect_h,
                rect_h,
                content_h,
                state.grid.scroll_y.current,
                (28.0 * scale).round(),
                Some(state.grid.scroll_y.drag_offset),
                scale,
            ) else {
                return DatabaseDragUpdate::None;
            };
            state.grid.scroll_y.target = target;
            state.grid.scroll_y.current = target;
            state.grid.scroll_y.velocity = 0.0;
            return DatabaseDragUpdate::Table(tab_id);
        }
        if state.grid.scroll_x.is_dragging {
            let Some((rect_x, _, rect_w, _)) = horizontal_rect else {
                return DatabaseDragUpdate::None;
            };
            let content_w = state
                .metadata
                .as_ref()
                .map_or(0.0, |metadata| state.grid.content_width(metadata) * scale);
            let Some((_, target)) = database_table_scroll_drag_target(
                mouse_x,
                rect_x,
                rect_w,
                rect_w,
                content_w,
                state.grid.scroll_x.current,
                (36.0 * scale).round(),
                Some(state.grid.scroll_x.drag_offset),
                scale,
            ) else {
                return DatabaseDragUpdate::None;
            };
            state.grid.scroll_x.target = target;
            state.grid.scroll_x.current = target;
            state.grid.scroll_x.velocity = 0.0;
            return DatabaseDragUpdate::Table(tab_id);
        }
        DatabaseDragUpdate::None
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

        if self.ide_panel.database.table_modal.is_some() {
            if matches!(
                self.ide_panel.database.table_modal,
                Some(DatabaseTableModal::SqlPreview { .. })
            ) {
                if primary && key_event.physical_key == PhysicalKey::Code(KeyCode::KeyC) {
                    let selected = self
                        .ide_panel
                        .database
                        .table_modal
                        .as_ref()
                        .and_then(database_sql_preview_copy_text);
                    if let Some(selected) = selected {
                        self.set_clipboard_text(selected);
                    }
                    return true;
                }
                if key_event.physical_key == PhysicalKey::Code(KeyCode::Escape) {
                    self.ide_panel.database.table_modal = None;
                    return true;
                }
                if let Some(DatabaseTableModal::SqlPreview {
                    text,
                    cursor,
                    selection_anchor,
                    ..
                }) = self.ide_panel.database.table_modal.as_mut()
                {
                    let target = match key_event.physical_key {
                        PhysicalKey::Code(KeyCode::KeyA) if primary => {
                            *selection_anchor = Some(0);
                            *cursor = text.len();
                            return true;
                        }
                        PhysicalKey::Code(KeyCode::ArrowLeft) => {
                            previous_char_boundary(text, *cursor)
                        }
                        PhysicalKey::Code(KeyCode::ArrowRight) => {
                            next_char_boundary(text, *cursor)
                        }
                        PhysicalKey::Code(KeyCode::Home) => line_start_boundary(text, *cursor),
                        PhysicalKey::Code(KeyCode::End) => line_end_boundary(text, *cursor),
                        _ => return true,
                    };
                    move_read_only_cursor(cursor, selection_anchor, target, shift);
                }
                return true;
            }
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
        let unavailable_focused = self
            .database_table_meta_state(tab_id)
            .is_some_and(|(_, state)| {
                !state.loading
                    && state.metadata.is_none()
                    && state.unavailable_text_focused
            });
        if unavailable_focused {
            let mut copied = None;
            if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
                if key_event.physical_key == PhysicalKey::Code(KeyCode::Escape) {
                    state.clear_unavailable_selection();
                } else {
                    let input = &mut state.unavailable_text;
                    match key_event.physical_key {
                        PhysicalKey::Code(KeyCode::KeyA) if primary => input.select_all(),
                        PhysicalKey::Code(KeyCode::KeyC) if primary => {
                            copied = input.selected_text().map(str::to_owned);
                        }
                        PhysicalKey::Code(KeyCode::ArrowLeft) => {
                            let target = previous_char_boundary(input.text(), input.cursor);
                            input.set_cursor(target, shift);
                        }
                        PhysicalKey::Code(KeyCode::ArrowRight) => {
                            let target = next_char_boundary(input.text(), input.cursor);
                            input.set_cursor(target, shift);
                        }
                        PhysicalKey::Code(KeyCode::Home) => input.set_cursor(0, shift),
                        PhysicalKey::Code(KeyCode::End) => {
                            input.set_cursor(input.text().len(), shift);
                        }
                        _ => {}
                    }
                }
            }
            if let Some(text) = copied {
                self.set_clipboard_text(text);
            }
            return true;
        }
        let focus = self
            .database_table_meta_state(tab_id)
            .and_then(|(_, state)| state.grid.focused_input);
        let filter_focus = matches!(
            focus,
            Some(DatabaseTableInputTarget::Where | DatabaseTableInputTarget::OrderBy)
        );
        if filter_focus && self.autocomplete_active {
            match self.handle_active_autocomplete_key(key_event.physical_key, primary) {
                crate::app::AutocompletePopupKeyResult::Consumed => return true,
                crate::app::AutocompletePopupKeyResult::Continue
                | crate::app::AutocompletePopupKeyResult::NotHandled => {}
            }
        }
        if filter_focus
            && primary
            && key_event.physical_key == PhysicalKey::Code(KeyCode::Space)
        {
            if let Some(target) = focus {
                self.show_active_database_table_filter_completion(target, true);
            }
            return true;
        }
        let before_filter_text = if filter_focus {
            self.database_table_meta_state(tab_id).and_then(|(_, state)| match focus {
                Some(DatabaseTableInputTarget::Where) => {
                    Some(state.grid.where_input.text().to_string())
                }
                Some(DatabaseTableInputTarget::OrderBy) => {
                    Some(state.grid.order_by_input.text().to_string())
                }
                _ => None,
            })
        } else {
            None
        };
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
                if matches!(focus, Some(DatabaseTableInputTarget::Where | DatabaseTableInputTarget::OrderBy)) {
                    state.grid.filter_error = None;
                }
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
        if let (Some(target), Some(before)) = (focus, before_filter_text) {
            let changed = self.database_table_meta_state(tab_id).is_some_and(|(_, state)| {
                let after = match target {
                    DatabaseTableInputTarget::Where => state.grid.where_input.text(),
                    DatabaseTableInputTarget::OrderBy => state.grid.order_by_input.text(),
                    DatabaseTableInputTarget::Cell => return false,
                };
                after != before
            });
            if changed {
                self.show_active_database_table_filter_completion(target, false);
            }
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

    pub(crate) fn page_database_table_enum_options(&mut self, next: bool) {
        let Some(tab_id) = self.active_database_table_tab_id() else {
            return;
        };
        let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else {
            return;
        };
        let Some((kind, column_index)) = state
            .grid
            .cell_editor
            .as_ref()
            .map(|editor| (editor.kind.clone(), editor.position.column))
        else {
            return;
        };
        if kind != DatabaseCellEditorKind::Enum {
            return;
        }
        let option_count = state
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.columns.get(column_index))
            .map_or(0, |column| column.enum_values.len());
        let Some(editor) = state.grid.cell_editor.as_mut() else {
            return;
        };
        let max_start = option_count.saturating_sub(1);
        editor.enum_index = if next {
            editor.enum_index.saturating_add(1).min(max_start)
        } else {
            editor.enum_index.saturating_sub(1)
        };
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

    pub(crate) fn start_database_sql_preview_scroll_drag(&mut self, horizontal: bool) {
        let mouse = self.renderer.as_ref().map_or((0.0, 0.0), |renderer| {
            (renderer.last_mouse_x, renderer.last_mouse_y)
        });
        let input_rect = self
            .ui_registry
            .rect_for(crate::ui_system::UiId::DatabaseTableModalInput);
        let vertical_rect = self
            .ui_registry
            .rect_for(crate::ui_system::UiId::DatabaseTableModalScroll);
        let horizontal_rect = self
            .ui_registry
            .rect_for(crate::ui_system::UiId::DatabaseTableModalScrollX);
        let Some(DatabaseTableModal::SqlPreview {
            text,
            scroll_x,
            scroll_y,
            ..
        }) = self.ide_panel.database.table_modal.as_ref()
        else {
            return;
        };
        let text = text.clone();
        let current_x = scroll_x.current;
        let current_y = scroll_y.current;
        let scale = self.renderer.as_ref().map_or(1.0, |renderer| renderer.scale_factor);
        let (viewport_w, viewport_h, max_x, max_y) = database_sql_preview_scroll_metrics(
            &text,
            input_rect,
            horizontal_rect,
            vertical_rect,
            scale,
            self.renderer.as_mut(),
        );
        let (rect, pointer, viewport, max_scroll, current, min_thumb) = if horizontal {
            (
                horizontal_rect,
                mouse.0,
                viewport_w,
                max_x,
                current_x,
                (36.0 * scale).round(),
            )
        } else {
            (
                vertical_rect,
                mouse.1,
                viewport_h,
                max_y,
                current_y,
                (28.0 * scale).round(),
            )
        };
        let Some(rect) = rect else { return; };
        let track_start = if horizontal { rect.0 } else { rect.1 };
        let track_len = if horizontal { rect.2 } else { rect.3 };
        let Some(thumb) = crate::scroll::scrollbar_thumb(
            track_start,
            track_len,
            viewport,
            viewport + max_scroll,
            current,
            min_thumb,
        ) else {
            return;
        };
        let Some((drag_offset, target)) = crate::scroll::scrollbar_drag_target(
            pointer,
            track_start,
            track_len,
            thumb,
            max_scroll,
            None,
        ) else {
            return;
        };
        let Some(DatabaseTableModal::SqlPreview { scroll_x, scroll_y, .. }) =
            self.ide_panel.database.table_modal.as_mut()
        else {
            return;
        };
        let scroll = if horizontal { scroll_x } else { scroll_y };
        scroll.current = target;
        scroll.target = target;
        scroll.velocity = 0.0;
        scroll.drag_offset = drag_offset;
        scroll.is_dragging = true;
    }

    fn update_database_sql_preview_scroll_drag(
        &mut self,
        mouse_x: f32,
        mouse_y: f32,
    ) -> bool {
        let input_rect = self
            .ui_registry
            .rect_for(crate::ui_system::UiId::DatabaseTableModalInput);
        let vertical_rect = self
            .ui_registry
            .rect_for(crate::ui_system::UiId::DatabaseTableModalScroll);
        let horizontal_rect = self
            .ui_registry
            .rect_for(crate::ui_system::UiId::DatabaseTableModalScrollX);
        let Some(DatabaseTableModal::SqlPreview {
            text,
            scroll_x,
            scroll_y,
            ..
        }) = self.ide_panel.database.table_modal.as_ref()
        else {
            return false;
        };
        let text = text.clone();
        let dragging_y = scroll_y.is_dragging;
        let dragging_x = scroll_x.is_dragging;
        let current_y = scroll_y.current;
        let current_x = scroll_x.current;
        let offset_y = scroll_y.drag_offset;
        let offset_x = scroll_x.drag_offset;
        let scale = self.renderer.as_ref().map_or(1.0, |renderer| renderer.scale_factor);
        let (viewport_w, viewport_h, max_x, max_y) = database_sql_preview_scroll_metrics(
            &text,
            input_rect,
            horizontal_rect,
            vertical_rect,
            scale,
            self.renderer.as_mut(),
        );
        let target = if dragging_y {
            let Some((_, track_y, _, track_h)) = vertical_rect else {
                return false;
            };
            let Some(thumb) = crate::scroll::scrollbar_thumb(
                track_y,
                track_h,
                viewport_h,
                viewport_h + max_y,
                current_y,
                (28.0 * scale).round(),
            ) else {
                return false;
            };
            crate::scroll::scrollbar_drag_target(
                mouse_y,
                track_y,
                track_h,
                thumb,
                max_y,
                Some(offset_y),
            )
            .map(|(_, target)| (false, target))
        } else if dragging_x {
            let Some((track_x, _, track_w, _)) = horizontal_rect else {
                return false;
            };
            let Some(thumb) = crate::scroll::scrollbar_thumb(
                track_x,
                track_w,
                viewport_w,
                viewport_w + max_x,
                current_x,
                (36.0 * scale).round(),
            ) else {
                return false;
            };
            crate::scroll::scrollbar_drag_target(
                mouse_x,
                track_x,
                track_w,
                thumb,
                max_x,
                Some(offset_x),
            )
            .map(|(_, target)| (true, target))
        } else {
            None
        };
        let Some((horizontal, target)) = target else {
            return false;
        };
        let Some(DatabaseTableModal::SqlPreview { scroll_x, scroll_y, .. }) =
            self.ide_panel.database.table_modal.as_mut()
        else {
            return false;
        };
        let scroll = if horizontal { scroll_x } else { scroll_y };
        scroll.current = target;
        scroll.target = target;
        scroll.velocity = 0.0;
        true
    }

    pub(crate) fn start_database_table_scroll_drag(&mut self, horizontal: bool) {
        let Some(tab_id) = self.active_database_table_tab_id() else { return; };
        let mouse = self.renderer.as_ref().map_or((0.0, 0.0), |renderer| (renderer.last_mouse_x, renderer.last_mouse_y));
        let vertical_rect = self.ui_registry.rect_for(crate::ui_system::UiId::DatabaseTableScrollY);
        let horizontal_rect = self.ui_registry.rect_for(crate::ui_system::UiId::DatabaseTableScrollX);
        let scale = self
            .renderer
            .as_ref()
            .map_or(1.0, |renderer| renderer.scale_factor)
            .max(f32::EPSILON);
        let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else { return; };
        if horizontal {
            let Some((track_x, _, track_w, _)) = horizontal_rect else { return; };
            let content_w = state
                .metadata
                .as_ref()
                .map_or(0.0, |metadata| state.grid.content_width(metadata) * scale);
            let Some((offset, _)) = database_table_scroll_drag_target(
                mouse.0,
                track_x,
                track_w,
                track_w,
                content_w,
                state.grid.scroll_x.current,
                (36.0 * scale).round(),
                None,
                scale,
            ) else { return; };
            state.grid.scroll_x.is_dragging = true;
            state.grid.scroll_x.drag_offset = offset;
        } else {
            let Some((_, track_y, _, track_h)) = vertical_rect else { return; };
            let row_h = (crate::app::database::DATABASE_GRID_ROW_HEIGHT * scale).round();
            let content_h = state.grid.logical_row_count() as f32 * row_h;
            let Some((offset, _)) = database_table_scroll_drag_target(
                mouse.1,
                track_y,
                track_h,
                track_h,
                content_h,
                state.grid.scroll_y.current,
                (28.0 * scale).round(),
                None,
                scale,
            ) else { return; };
            state.grid.scroll_y.is_dragging = true;
            state.grid.scroll_y.drag_offset = offset;
        }
    }

    pub(crate) fn activate_database_table_modal_action(&mut self, action: usize) {
        let Some(modal) = self.ide_panel.database.table_modal.clone() else { return; };
        match modal {
            DatabaseTableModal::SqlPreview { .. } => {
                if action == 2 {
                    if let Some(text) = database_sql_preview_copy_text(&modal) {
                        self.set_clipboard_text(text);
                    }
                } else {
                    self.ide_panel.database.table_modal = None;
                }
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
        if let Some(DatabaseTableModal::SqlPreview { scroll_x, scroll_y, .. }) =
            self.ide_panel.database.table_modal.as_mut()
        {
            scroll_x.is_dragging = false;
            scroll_y.is_dragging = false;
        }
        self.finish_database_query_scroll_drag();
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


fn database_sql_preview_scroll_metrics(
    text: &str,
    input_rect: Option<(f32, f32, f32, f32)>,
    horizontal_rect: Option<(f32, f32, f32, f32)>,
    vertical_rect: Option<(f32, f32, f32, f32)>,
    scale: f32,
    renderer: Option<&mut crate::renderer::Renderer>,
) -> (f32, f32, f32, f32) {
    let viewport_w = horizontal_rect
        .map(|rect| rect.2)
        .or_else(|| input_rect.map(|rect| rect.2))
        .unwrap_or(1.0)
        .max(1.0);
    let viewport_h = vertical_rect
        .map(|rect| rect.3)
        .or_else(|| input_rect.map(|rect| rect.3))
        .unwrap_or(1.0)
        .max(1.0);
    let line_h = (crate::app::database::DATABASE_SQL_PREVIEW_LINE_HEIGHT * scale)
        .round()
        .max(1.0);
    let content_h = text.lines().count().max(1) as f32 * line_h;
    let content_w = renderer.map_or_else(
        || {
            text.lines()
                .map(|line| line.chars().count() as f32 * (9.0 * scale).round())
                .fold(0.0_f32, f32::max)
        },
        |renderer| {
            text.lines()
                .map(|line| line.chars().map(|ch| renderer.char_advance(ch)).sum())
                .fold(0.0_f32, f32::max)
        },
    ) + (18.0 * scale).round();
    (
        viewport_w,
        viewport_h,
        (content_w - viewport_w).max(0.0),
        (content_h - viewport_h).max(0.0),
    )
}

fn database_sql_preview_copy_text(modal: &DatabaseTableModal) -> Option<String> {
    let DatabaseTableModal::SqlPreview {
        text,
        cursor,
        selection_anchor,
        ..
    } = modal
    else {
        return None;
    };
    let Some(anchor) = selection_anchor else {
        return Some(text.clone());
    };
    let start = (*anchor).min(*cursor);
    let end = (*anchor).max(*cursor);
    if start == end {
        return Some(text.clone());
    }
    text.get(start..end).map(str::to_owned)
}

fn previous_char_boundary(text: &str, cursor: usize) -> usize {
    let mut cursor = cursor.min(text.len());
    if cursor == 0 {
        return 0;
    }
    cursor -= 1;
    while cursor > 0 && !text.is_char_boundary(cursor) {
        cursor -= 1;
    }
    cursor
}

fn next_char_boundary(text: &str, cursor: usize) -> usize {
    let mut cursor = cursor.min(text.len());
    if cursor >= text.len() {
        return text.len();
    }
    cursor += 1;
    while cursor < text.len() && !text.is_char_boundary(cursor) {
        cursor += 1;
    }
    cursor
}

fn line_start_boundary(text: &str, cursor: usize) -> usize {
    text[..cursor.min(text.len())]
        .rfind('\n')
        .map_or(0, |index| index + 1)
}

fn line_end_boundary(text: &str, cursor: usize) -> usize {
    let cursor = cursor.min(text.len());
    text[cursor..]
        .find('\n')
        .map_or(text.len(), |offset| cursor + offset)
}

fn move_read_only_cursor(
    cursor: &mut usize,
    selection_anchor: &mut Option<usize>,
    target: usize,
    selecting: bool,
) {
    let old_cursor = *cursor;
    if selecting {
        if selection_anchor.is_none() {
            *selection_anchor = Some(old_cursor);
        }
    } else {
        *selection_anchor = None;
    }
    *cursor = target;
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
    if !multiline {
        return crate::app::single_line_input::handle_single_line_input(
            input,
            physical_key,
            logical_text,
            primary,
            word,
            shift,
            text_input_allowed,
            paste_text.as_deref(),
            max_bytes,
        );
    }
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


fn database_table_transaction_finish_allowed(committing: bool) -> bool {
    !committing
}

#[cfg(test)]
mod database_table_edit_method_tests {
    use super::*;

    fn sql_preview(text: &str, cursor: usize, anchor: Option<usize>) -> DatabaseTableModal {
        DatabaseTableModal::SqlPreview {
            tab_id: crate::app::database::DatabaseTabId(1),
            text: text.to_string(),
            cursor,
            selection_anchor: anchor,
            spans: Vec::new(),
            scroll_x: crate::scroll::ScrollState::new(15.0),
            scroll_y: crate::scroll::ScrollState::new(15.0),
        }
    }

    #[test]
    fn query_drag_never_requires_a_database_table_tab() {
        let query = DatabaseDragUpdate::Query;
        assert!(query.changed());
        assert_eq!(query.table_tab_id(), None);

        let tab_id = crate::app::database::DatabaseTabId(7);
        let table = DatabaseDragUpdate::Table(tab_id);
        assert_eq!(table.table_tab_id(), Some(tab_id));
    }

    #[test]
    fn sql_preview_copy_prefers_the_selected_unicode_range() {
        let text = "SELECT 'Ж';";
        let start = text.find('Ж').unwrap();
        let end = start + 'Ж'.len_utf8();
        let modal = sql_preview(text, end, Some(start));
        assert_eq!(database_sql_preview_copy_text(&modal).as_deref(), Some("Ж"));
    }

    #[test]
    fn sql_preview_copy_without_selection_returns_the_full_query() {
        let modal = sql_preview("SELECT 1;", 4, None);
        assert_eq!(
            database_sql_preview_copy_text(&modal).as_deref(),
            Some("SELECT 1;")
        );
    }

    #[test]
    fn read_only_cursor_helpers_preserve_utf8_boundaries_and_lines() {
        let text = "Жx\nSELECT";
        assert_eq!(next_char_boundary(text, 0), 'Ж'.len_utf8());
        assert_eq!(previous_char_boundary(text, 'Ж'.len_utf8()), 0);
        assert_eq!(line_start_boundary(text, text.len()), 4);
        assert_eq!(line_end_boundary(text, 0), 3);
    }

    #[test]
    fn bug_17_table_scrollbar_drag_preserves_pointer_offset_inside_thumb() {
        let scale = 1.0;
        let track_start = 10.0;
        let track_len = 200.0;
        let viewport = 100.0;
        let content = 400.0;
        let current = 150.0;
        let pointer = 100.0;
        let (offset, initial_target) = database_table_scroll_drag_target(
            pointer,
            track_start,
            track_len,
            viewport,
            content,
            current,
            20.0,
            None,
            scale,
        )
        .expect("scrollbar drag starts");
        assert_eq!(initial_target, current);

        let (_, target) = database_table_scroll_drag_target(
            pointer + 25.0,
            track_start,
            track_len,
            viewport,
            content,
            current,
            20.0,
            Some(offset),
            scale,
        )
        .expect("scrollbar drag continues");
        assert!(target > current);
        assert_eq!(target, 200.0);
    }
    #[test]
    fn bug_59_table_transaction_finish_rejects_duplicate_commit_or_rollback() {
        assert!(database_table_transaction_finish_allowed(false));
        assert!(!database_table_transaction_finish_allowed(true));
    }

}
