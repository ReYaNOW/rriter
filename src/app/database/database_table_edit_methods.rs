use crate::app::database::database_table_cell_edit_state::database_table_transaction_finish_allowed;
use crate::app::database::database_table_modal_state::{
    database_sql_preview_scroll_metrics, database_text_modal_scrolls_mut,
};

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
    drag_offset: Option<f32>,
    horizontal: bool,
    scale: f32,
) -> Option<(f32, f32)> {
    let scale = scale.max(f32::EPSILON);
    let lane = if horizontal {
        (track_start, 0.0, track_len, 0.0)
    } else {
        (0.0, track_start, 0.0, track_len)
    };
    let bar = crate::render_view::database_table_tab::database_table_scrollbar(
        lane, viewport_len, content_len,
        current_scroll * scale, horizontal,
    );
    let geometry = bar.geometry(scale)?;
    let (offset, target) = if let Some(offset) = drag_offset {
        (offset, geometry.drag_target(pointer, offset)?)
    } else {
        let (offset, _) = geometry.press_target(pointer)?;
        (offset, geometry.drag_target(pointer, offset)?)
    };
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
        let text_scale = 0.84;
        let text_geometry = crate::app::single_line_input::single_line_text_geometry(
            rect.0, rect.2, 0.0, 0.0,
        );
        let cursor_geometry = crate::app::single_line_input::single_line_cursor_geometry(
            &text,
            cursor,
            text_geometry.content_w,
            0.0,
            0.0,
            0.0,
            |ch| renderer.one_line_ui_advance(ch, text_scale),
        );
        let x_offset =
            (mouse_x - text_geometry.text_start_x + cursor_geometry.scroll_x).max(0.0);
        Some(crate::app::single_line_input::single_line_hit_index(
            &text,
            x_offset,
            |ch| renderer.one_line_ui_advance(ch, text_scale),
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
        let text_geometry = crate::app::database_table_input_text_geometry(
            rect.0,
            rect.2,
            scale,
            target == DatabaseTableInputTarget::Cell,
        );
        let edge_pad = crate::app::single_line_input::single_line_cursor_edge_pad(scale);
        let cursor_geometry = crate::app::single_line_input::single_line_cursor_geometry(
            &text,
            cursor,
            text_geometry.content_w,
            0.0,
            edge_pad,
            edge_pad,
            |ch| renderer.one_line_ui_advance(ch, text_scale),
        );
        let x_offset = crate::app::single_line_input::single_line_hit_offset(
            text_geometry,
            mouse_x,
            cursor_geometry.scroll_x,
        );
        Some(crate::app::single_line_input::single_line_hit_index(
            &text,
            x_offset,
            |ch| renderer.one_line_ui_advance(ch, text_scale),
        ))
    }

    pub(crate) fn database_table_modal_input_index_at(
        &mut self,
        mouse_x: f32,
        mouse_y: f32,
    ) -> Option<usize> {
        enum ModalInputSnapshot {
            SingleLine { text: String, cursor: usize },
            CodeText {
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
            } => ModalInputSnapshot::CodeText {
                text: text.clone(),
                scroll_x: scroll_x.current,
                scroll_y: scroll_y.current,
            },
            DatabaseTableModal::MultilineEditor {
                input,
                scroll_x,
                scroll_y,
                ..
            } => ModalInputSnapshot::CodeText {
                text: input.text().to_string(),
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
                let padding = (8.0 * scale).round();
                let text_geometry = crate::app::single_line_input::single_line_text_geometry(
                    rect.0, rect.2, padding, 0.0,
                );
                let edge_pad = crate::app::single_line_input::single_line_cursor_edge_pad(scale);
                let cursor_geometry = crate::app::single_line_input::single_line_cursor_geometry(
                    &text,
                    cursor,
                    text_geometry.content_w,
                    0.0,
                    edge_pad,
                    edge_pad,
                    |ch| renderer.one_line_ui_advance(ch, text_scale),
                );
                let x_offset =
                    (mouse_x - text_geometry.text_start_x + cursor_geometry.scroll_x).max(0.0);
                Some(crate::app::single_line_input::single_line_hit_index(
                    &text,
                    x_offset,
                    |ch| renderer.one_line_ui_advance(ch, text_scale),
                ))
            }
            ModalInputSnapshot::CodeText {
                text,
                scroll_x,
                scroll_y,
            } => {
                let line_h = (crate::app::database::DATABASE_SQL_PREVIEW_LINE_HEIGHT * scale)
                    .round()
                    .max(1.0);
                let line_index = ((mouse_y - rect.1 + scroll_y).max(0.0) / line_h)
                    .floor() as usize;
                let Some((line_start, line)) =
                    crate::app::database::database_multiline_lines(&text).nth(line_index)
                else {
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
            Some(DatabaseTableModal::MultilineEditor { input, .. }) => {
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
        if let crate::app::database::database_table_cell_edit_state::DatabaseTableCellEditStart::Multiline {
            position, text,
        } = state.start_cell_edit(position) {
            self.ide_panel.database.table_modal_layout_cache.get_mut().invalidate();
            self.ide_panel.database.table_modal = Some(DatabaseTableModal::MultilineEditor {
                tab_id,
                position,
                input: crate::app::database::DatabaseDialogInput::new(text),
                scroll_x: crate::scroll::ScrollState::new(15.0),
                scroll_y: crate::scroll::ScrollState::new(15.0),
                error: None,
            });
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
        state.commit_cell_edit(literal);
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
        state.change_plan(meta)
    }

    pub fn preview_database_table_changes(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
    ) {
        match self.database_table_change_plan(tab_id) {
            Ok(plan) => {
                let text = crate::app::database::format_database_sql(&plan.preview)
                    .unwrap_or(plan.preview);
                self.ide_panel
                    .database
                    .table_modal_layout_cache
                    .get_mut()
                    .invalidate();
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
            let row_h = crate::app::database::database_grid_row_height_px(scale);
            let content_h = state.grid.logical_row_count() as f32 * row_h;
            let Some((drag_offset, target)) = database_table_scroll_drag_target(
                mouse_y,
                rect_y,
                rect_h,
                rect_h,
                content_h,
                state.grid.scroll_y.current,
                Some(state.grid.scroll_y.drag_offset),
                false,
                scale,
            ) else {
                return DatabaseDragUpdate::None;
            };
            crate::app::mouse::apply_scrollbar_drag_target(
                &mut state.grid.scroll_y, target, drag_offset,
            );
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
            let Some((drag_offset, target)) = database_table_scroll_drag_target(
                mouse_x,
                rect_x,
                rect_w,
                rect_w,
                content_w,
                state.grid.scroll_x.current,
                Some(state.grid.scroll_x.drag_offset),
                true,
                scale,
            ) else {
                return DatabaseDragUpdate::None;
            };
            crate::app::mouse::apply_scrollbar_drag_target(
                &mut state.grid.scroll_x, target, drag_offset,
            );
            return DatabaseDragUpdate::Table(tab_id);
        }
        DatabaseDragUpdate::None
    }

    pub(crate) fn handle_database_table_cell_click(
        &mut self,
        row: usize,
        column: usize,
        double: bool,
    ) {
        let Some(tab_id) = self.active_database_table_tab_id() else { return; };
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
        state.page_cell_edit_enum_options(next);
    }

    pub(crate) fn select_database_table_enum_option(&mut self, option: usize) {
        let Some(tab_id) = self.active_database_table_tab_id() else { return; };
        let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else { return; };
        let selected = state.select_cell_edit_enum_option(option);
        if selected {
            self.commit_database_table_cell_editor(tab_id, false);
        }
    }

    pub(crate) fn shift_database_table_calendar_month(&mut self, delta: i32) {
        let Some(tab_id) = self.active_database_table_tab_id() else { return; };
        let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else { return; };
        state.shift_cell_edit_calendar_month(delta);
    }

    pub(crate) fn select_database_table_calendar_day(&mut self, day: u32) {
        let Some(tab_id) = self.active_database_table_tab_id() else { return; };
        let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else { return; };
        state.select_cell_edit_calendar_day(day);
    }

    pub(crate) fn set_database_table_date_today(&mut self) {
        let Some(tab_id) = self.active_database_table_tab_id() else { return; };
        let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else { return; };
        let days = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs() / 86_400);
        let (year, month, day) = crate::app::database::civil_date_from_unix_days(days as i64);
        state.set_cell_edit_date(year, month, day);
    }

    pub(crate) fn set_database_table_time_now_utc(&mut self) {
        let Some(tab_id) = self.active_database_table_tab_id() else { return; };
        let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else { return; };
        let Some(metadata) = state.metadata.as_ref() else { return; };
        let Some(editor) = state.grid.cell_editor.as_ref() else { return; };
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
        state.set_cell_edit_time_text(text, year, month);
    }

    pub(crate) fn start_database_sql_preview_scroll_drag(&mut self, horizontal: bool) {
        let mouse = self.renderer.as_ref().map_or((0.0, 0.0), |r| (r.last_mouse_x, r.last_mouse_y));
        let input_rect = self.ui_registry.rect_for(crate::ui_system::UiId::DatabaseTableModalInput);
        let vertical_rect = self.ui_registry.rect_for(crate::ui_system::UiId::DatabaseTableModalScroll);
        let horizontal_rect = self.ui_registry.rect_for(crate::ui_system::UiId::DatabaseTableModalScrollX);
        let scale = self.renderer.as_ref().map_or(1.0, |renderer| renderer.scale_factor);
        let Some(snapshot) = self.ide_panel.database.table_modal.as_ref().and_then(|modal| crate::app::database::database_table_modal_state::text_modal_scroll_snapshot(modal, &self.ide_panel.database.table_modal_layout_cache, scale, self.renderer.as_mut())) else { return; };
        let (viewport_w, viewport_h, max_x, max_y) = database_sql_preview_scroll_metrics(snapshot.line_count, snapshot.max_line_width, input_rect, horizontal_rect, vertical_rect, scale);
        let (lane, viewport, max_scroll, current, x, y) = if horizontal {
            (horizontal_rect, viewport_w, max_x, snapshot.current_x, mouse.0, mouse.1)
        } else {
            (vertical_rect, viewport_h, max_y, snapshot.current_y, mouse.0, mouse.1)
        };
        let Some((lane_x, lane_y, lane_w, lane_h)) = lane else { return; };
        let bar = crate::render_view::database_table_tab_overlay::database_table_modal_scrollbar(
            (lane_x, lane_y, lane_w, lane_h), viewport, viewport + max_scroll,
            current, horizontal,
        );
        let geometry = bar.geometry(scale);
        let Some((scroll_x, scroll_y)) = self.ide_panel.database.table_modal.as_mut().and_then(database_text_modal_scrolls_mut) else { return; };
        let scroll = if horizontal { scroll_x } else { scroll_y };
        crate::app::mouse::press_scrollbar(scroll, geometry, x, y);
    }

    fn update_database_sql_preview_scroll_drag(
        &mut self,
        mouse_x: f32,
        mouse_y: f32,
    ) -> bool {
        let input_rect = self.ui_registry.rect_for(crate::ui_system::UiId::DatabaseTableModalInput);
        let vertical_rect = self.ui_registry.rect_for(crate::ui_system::UiId::DatabaseTableModalScroll);
        let horizontal_rect = self.ui_registry.rect_for(crate::ui_system::UiId::DatabaseTableModalScrollX);
        let scale = self.renderer.as_ref().map_or(1.0, |renderer| renderer.scale_factor);
        let Some(snapshot) = self.ide_panel.database.table_modal.as_ref().and_then(|modal| crate::app::database::database_table_modal_state::text_modal_scroll_snapshot(modal, &self.ide_panel.database.table_modal_layout_cache, scale, self.renderer.as_mut())) else { return false; };
        let (viewport_w, viewport_h, max_x, max_y) = database_sql_preview_scroll_metrics(snapshot.line_count, snapshot.max_line_width, input_rect, horizontal_rect, vertical_rect, scale);
        let horizontal = snapshot.dragging_x;
        if !horizontal && !snapshot.dragging_y { return false; }
        let (lane, viewport, max_scroll, current) = if horizontal {
            (horizontal_rect, viewport_w, max_x, snapshot.current_x)
        } else {
            (vertical_rect, viewport_h, max_y, snapshot.current_y)
        };
        let Some((lane_x, lane_y, lane_w, lane_h)) = lane else { return false; };
        let bar = crate::render_view::database_table_tab_overlay::database_table_modal_scrollbar(
            (lane_x, lane_y, lane_w, lane_h), viewport, viewport + max_scroll,
            current, horizontal,
        );
        let geometry = bar.geometry(scale);
        let Some((scroll_x, scroll_y)) = self.ide_panel.database.table_modal.as_mut().and_then(database_text_modal_scrolls_mut) else { return false; };
        let scroll = if horizontal { scroll_x } else { scroll_y };
        crate::app::mouse::drag_scrollbar(
            scroll, geometry, mouse_x, mouse_y,
        ).is_some()
    }

    pub(crate) fn scroll_database_text_modal(
        &mut self,
        dx: f32,
        dy: f32,
        shift: bool,
    ) -> bool {
        let Some(input_rect) = self.ui_registry.rect_for(crate::ui_system::UiId::DatabaseTableModalInput) else { return false; };
        let vertical_rect = self.ui_registry.rect_for(crate::ui_system::UiId::DatabaseTableModalScroll);
        let horizontal_rect = self.ui_registry.rect_for(crate::ui_system::UiId::DatabaseTableModalScrollX);
        let scale = self.renderer.as_ref().map_or(1.0, |renderer| renderer.scale_factor);
        let Some(snapshot) = self.ide_panel.database.table_modal.as_ref().and_then(|modal| crate::app::database::database_table_modal_state::text_modal_scroll_snapshot(modal, &self.ide_panel.database.table_modal_layout_cache, scale, self.renderer.as_mut())) else { return false; };
        let (_, _, max_x, max_y) = database_sql_preview_scroll_metrics(snapshot.line_count, snapshot.max_line_width, Some(input_rect), horizontal_rect, vertical_rect, scale);
        crate::app::database::database_table_modal_state::scroll_text_modal(
            &mut self.ide_panel.database.table_modal, dx, dy, shift, max_x, max_y,
        )
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
            let Some((drag_offset, target)) = database_table_scroll_drag_target(
                mouse.0,
                track_x,
                track_w,
                track_w,
                content_w,
                state.grid.scroll_x.current,
                None,
                true,
                scale,
            ) else { return; };
            crate::app::mouse::apply_scrollbar_drag_target(
                &mut state.grid.scroll_x, target, drag_offset,
            );
        } else {
            let Some((_, track_y, _, track_h)) = vertical_rect else { return; };
            let row_h = crate::app::database::database_grid_row_height_px(scale);
            let content_h = state.grid.logical_row_count() as f32 * row_h;
            let Some((drag_offset, target)) = database_table_scroll_drag_target(
                mouse.1,
                track_y,
                track_h,
                track_h,
                content_h,
                state.grid.scroll_y.current,
                None,
                false,
                scale,
            ) else { return; };
            crate::app::mouse::apply_scrollbar_drag_target(
                &mut state.grid.scroll_y, target, drag_offset,
            );
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

    pub(crate) fn stop_database_table_modal_scroll_anims(&mut self) {
        self.ide_panel.database.stop_table_modal_scroll_anims();
    }

    pub(crate) fn finish_database_table_drag(&mut self) {
        if let Some((scroll_x, scroll_y)) = self
            .ide_panel
            .database
            .table_modal
            .as_mut()
            .and_then(database_text_modal_scrolls_mut)
        {
            scroll_x.end_drag();
            scroll_y.end_drag();
        }
        self.finish_database_query_scroll_drag();
        let Some(tab_id) = self.active_database_table_tab_id() else {
            return;
        };
        let persist = if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
            let resized = state.grid.column_resize.take().is_some();
            state.grid.scroll_x.end_drag();
            state.grid.scroll_y.end_drag();
            resized
        } else {
            false
        };
        if persist {
            self.persist_database_table_view(tab_id);
        }
    }
}


#[cfg(test)]
mod database_table_edit_method_tests {
    use super::*;

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
            None,
            false,
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
            Some(offset),
            false,
            scale,
        )
        .expect("scrollbar drag continues");
        assert!(target > current);
        assert_eq!(target, 200.0);

        let mut scroll = crate::scroll::ScrollState::new(7.0);
        scroll.jump_to(current);
        assert!(crate::app::mouse::apply_scrollbar_drag_target(
            &mut scroll,
            target,
            offset
        ));
        assert_eq!(scroll.current, current);
        assert_eq!(scroll.target, target);
        assert_eq!(scroll.drag_offset, offset);
        scroll.update(1.0 / 60.0);
        assert!(scroll.current > current);
        assert!(scroll.current < target);
    }

}
