use crate::app::database::{
    DatabaseCellEditorKind, DatabaseCellEditorState, DatabaseCellPosition,
    DatabaseChangePlanOperation, DatabaseGeneration, DatabaseTableInputTarget,
    DatabaseTableModal, DatabaseTableReloadAction, DatabaseTableViewKey, DatabaseTableViewState,
};

pub(crate) fn database_table_input_padding(ui_scale: f32, cell_editor: bool) -> f32 {
    if cell_editor {
        (8.0 * ui_scale).round()
    } else {
        (10.0 * ui_scale).round()
    }
}

pub(crate) fn database_table_input_text_geometry(
    input_x: f32,
    input_w: f32,
    ui_scale: f32,
    cell_editor: bool,
) -> crate::app::single_line_input::SingleLineTextGeometry {
    let padding = database_table_input_padding(ui_scale, cell_editor);
    crate::app::single_line_input::single_line_text_geometry(input_x, input_w, padding, 0.0)
}

impl App {
    pub(crate) fn show_active_database_table_filter_completion(
        &mut self,
        target: DatabaseTableInputTarget,
        explicit: bool,
    ) {
        if !matches!(target, DatabaseTableInputTarget::Where | DatabaseTableInputTarget::OrderBy) {
            self.close_autocomplete();
            return;
        }
        let Some(tab_id) = self.active_database_table_tab_id() else {
            self.close_autocomplete();
            return;
        };
        let Some((_, state)) = self.database_table_meta_state(tab_id) else {
            self.close_autocomplete();
            return;
        };
        let Some(metadata) = state.metadata.as_ref() else {
            self.close_autocomplete();
            return;
        };
        let input = match target {
            DatabaseTableInputTarget::Where => &state.grid.where_input,
            DatabaseTableInputTarget::OrderBy => &state.grid.order_by_input,
            DatabaseTableInputTarget::Cell => unreachable!(),
        };
        let text = input.text().to_string();
        let cursor = input.cursor;
        let context =
            crate::app::database::database_table_view_state::database_table_filter_completion_context(
                target, &text, cursor,
            );
        if !explicit && !context.automatic {
            self.close_autocomplete();
            return;
        }
        let words =
            crate::app::database::database_table_view_state::database_table_filter_completion_words(
                metadata, target, &context,
            );
        if words.is_empty() {
            self.close_autocomplete();
            return;
        }
        let input_id = match target {
            DatabaseTableInputTarget::Where => crate::ui_system::UiId::DatabaseTableWhereInput,
            DatabaseTableInputTarget::OrderBy => crate::ui_system::UiId::DatabaseTableOrderInput,
            DatabaseTableInputTarget::Cell => unreachable!(),
        };
        let anchor = self.ui_registry.rect_for(input_id).map(|rect| {
            let text_scale = crate::app::database::DATABASE_TABLE_INPUT_TEXT_SCALE;
            let ui_scale = self
                .renderer
                .as_ref()
                .map_or(1.0, |renderer| renderer.scale_factor);
            let text_geometry =
                database_table_input_text_geometry(rect.0, rect.2, ui_scale, false);
            let edge_pad = crate::app::single_line_input::single_line_cursor_edge_pad(ui_scale);
            let cursor_geometry = self.renderer.as_mut().map(|renderer| {
                crate::app::single_line_input::single_line_cursor_geometry(
                    &text,
                    cursor,
                    text_geometry.content_w,
                    0.0,
                    edge_pad,
                    edge_pad,
                    |ch| renderer.one_line_ui_advance(ch, text_scale),
                )
            });
            let (scroll_x, cursor_x) = cursor_geometry
                .map(|geometry| (geometry.scroll_x, geometry.cursor_x))
                .unwrap_or((0.0, 0.0));
            (
                crate::app::single_line_input::single_line_rendered_x(
                    text_geometry,
                    cursor_x,
                    scroll_x,
                )
                .round(),
                (rect.1 + rect.3).round(),
            )
        });

        let context_key = format!("table-filter:{}:{:?}:{}", tab_id.0, target, context.context_key());
        let same_context = self.autocomplete_active
            && self.autocomplete_mode == AutocompleteMode::Sql
            && self.autocomplete_pending_context_key.as_deref() == Some(context_key.as_str());
        let selected_word = self.autocomplete_options
            .get(self.autocomplete_selected_idx)
            .map(|(item, _)| item.word.clone());
        self.autocomplete_options = words;
        self.autocomplete_selected_idx = selected_word
            .as_deref()
            .and_then(|word| self.autocomplete_options.iter().position(|(item, _)| item.word == word))
            .unwrap_or(0);
        self.autocomplete_hovered_idx = None;
        self.autocomplete_mode = AutocompleteMode::Sql;
        self.autocomplete_pending_context_key = Some(context_key);
        if !same_context {
            self.autocomplete_scroll.reset();
            self.autocomplete_anim_progress = 0.0;
        }
        self.autocomplete_anchor = anchor;
        self.autocomplete_detail_popup = None;
        self.autocomplete_detail_rect = None;
        self.autocomplete_detail_placement = None;
        self.autocomplete_detail_max_scroll = 0.0;
        self.reset_autocomplete_detail_size();
        self.autocomplete_active = !self.autocomplete_options.is_empty();
        if self.autocomplete_active {
            self.refresh_autocomplete_detail_popup();
        }
    }

    pub(crate) fn apply_database_table_filter_autocomplete(&mut self) -> bool {
        if !self.autocomplete_active || self.autocomplete_options.is_empty() {
            return false;
        }
        let Some(tab_id) = self.active_database_table_tab_id() else {
            return false;
        };
        let target = self
            .database_table_meta_state(tab_id)
            .and_then(|(_, state)| state.grid.focused_input);
        let Some(target @ (DatabaseTableInputTarget::Where | DatabaseTableInputTarget::OrderBy)) =
            target
        else {
            return false;
        };
        let Some((selected_item, _)) = self
            .autocomplete_options
            .get(self.autocomplete_selected_idx)
            .or_else(|| self.autocomplete_options.first())
        else {
            self.close_autocomplete();
            return false;
        };
        let selected = selected_item
            .insert_text
            .clone()
            .unwrap_or_else(|| selected_item.word.clone());
        if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
            state.apply_filter_autocomplete(target, &selected);
        }
        self.close_autocomplete();
        true
    }

    pub(crate) fn database_table_view_state(
        &mut self,
        connection_id: crate::app::database::DatabaseConnectionId,
        database_name: &str,
        table_name: &str,
    ) -> DatabaseTableViewState {
        let key = DatabaseTableViewKey {
            connection_id,
            database_name: database_name.to_string(),
            table_name: table_name.to_string(),
        };
        let default_limit = self.ide_panel.database.settings().default_table_limit;
        self.ide_panel.database.table_view_snapshot(key, default_limit)
    }

    pub(crate) fn persist_database_table_view(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
    ) {
        let Some(view) = self
            .tabs
            .iter()
            .find_map(|tab| match &tab.kind {
                EditorTabKind::DatabaseTable(meta, state) if meta.tab_id == tab_id => {
                    Some(state.grid.view.clone())
                }
                _ => None,
            })
        else {
            return;
        };
        self.ide_panel.database.upsert_table_view(view);
        self.save_database_panel_state();
    }

    pub(crate) fn database_table_index(
        &self,
        tab_id: crate::app::database::DatabaseTabId,
    ) -> Option<usize> {
        self.tabs.iter().position(|tab| {
            matches!(&tab.kind, EditorTabKind::DatabaseTable(meta, _) if meta.tab_id == tab_id)
        })
    }

    pub(crate) fn active_database_table_tab_id(
        &self,
    ) -> Option<crate::app::database::DatabaseTabId> {
        self.tabs.get(self.active_tab).and_then(|tab| match &tab.kind {
            EditorTabKind::DatabaseTable(meta, _) => Some(meta.tab_id),
            _ => None,
        })
    }

    pub(crate) fn database_table_meta_state(
        &self,
        tab_id: crate::app::database::DatabaseTabId,
    ) -> Option<(&DatabaseTableTabMeta, &DatabaseTableTabState)> {
        let index = self.database_table_index(tab_id)?;
        match &self.tabs[index].kind {
            EditorTabKind::DatabaseTable(meta, state) => Some((meta, state)),
            _ => None,
        }
    }

    pub(crate) fn database_table_meta_state_mut(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
    ) -> Option<(&DatabaseTableTabMeta, &mut DatabaseTableTabState)> {
        let index = self.database_table_index(tab_id)?;
        match &mut self.tabs[index].kind {
            EditorTabKind::DatabaseTable(meta, state) => Some((meta, state)),
            _ => None,
        }
    }

    pub(crate) fn queue_database_table_initial_load(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
    ) {
        let prepared = self.database_table_meta_state_mut(tab_id).and_then(|(meta, state)| {
            state.prepare_initial_view_load(meta)
        });
        let Some((meta, generation)) = prepared else {
            return;
        };
        self.queue_database_table_count(meta, generation);
    }

    fn queue_database_table_count(
        &mut self,
        meta: DatabaseTableTabMeta,
        generation: DatabaseGeneration,
    ) {
        let Some(connection) = self
            .ide_panel
            .database
            .connection(meta.connection_id)
            .map(|node| node.config.clone())
        else {
            if let Some((_, state)) = self.database_table_meta_state_mut(meta.tab_id) {
                state.fail_view_reload("Подключение к базе данных недоступно".to_string());
            }
            return;
        };
        let Some((_, state)) = self.database_table_meta_state(meta.tab_id) else {
            return;
        };
        let Some(metadata) = state.metadata.clone() else {
            if let Some((_, state)) = self.database_table_meta_state_mut(meta.tab_id) {
                state.fail_view_reload("Метаданные таблицы недоступны".to_string());
            }
            return;
        };
        let where_clause = state.grid.request_view().where_clause.clone();
        let settings = self.ide_panel.database.settings().clone();
        let secrets = self.connection_job_secrets(meta.connection_id);
        let job_id = self.ide_panel.database.allocate_job_id();
        let pending = DatabasePendingJob {
            id: job_id,
            kind: DatabasePendingJobKind::CountRows,
            owner: crate::app::database::DatabaseJobOwner::Table(meta.tab_id),
            connection_id: meta.connection_id,
            database_name: Some(meta.database_name.clone()),
            table_name: Some(meta.table_name.clone()),
        };
        let host_key_policy = self
            .ide_panel
            .database
            .host_key_policy_override
            .take()
            .unwrap_or(SshHostKeyPolicy::Strict);
        let started = self.send_database_command(
            DatabaseCommand::CountRows {
                job_id,
                connection,
                database_name: meta.database_name,
                metadata,
                where_clause,
                generation,
                secrets,
                settings,
                ssh_options: crate::app::database::host_key_options(host_key_policy),
            },
            pending,
        );
        if !started {
            let message = self
                .ide_panel
                .database
                .global_error
                .clone()
                .unwrap_or_else(|| "Не удалось запустить обновление таблицы".to_string());
            if let Some((_, state)) = self.database_table_meta_state_mut(meta.tab_id) {
                state.fail_view_reload(message);
            }
        }
    }

    pub(crate) fn queue_database_table_chunk(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
        chunk_index: usize,
    ) {
        let Some(index) = self.database_table_index(tab_id) else {
            return;
        };
        let (meta, metadata, generation, where_clause, order_by, page, limit) =
            match &mut self.tabs[index].kind {
                EditorTabKind::DatabaseTable(meta, state) => {
                    if state.grid.can_reuse_loaded_chunk(chunk_index) {
                        state.grid.touch_chunk(chunk_index);
                        return;
                    }
                    if state.grid.loading_chunk {
                        state.grid.desired_chunk = Some(chunk_index);
                        return;
                    }
                    let Some(metadata) = state.metadata.clone() else {
                        return;
                    };
                    let request_view = state.grid.request_view().clone();
                    state.grid.loading_chunk = true;
                    state.grid.in_flight_chunk = Some(chunk_index);
                    (
                        meta.clone(),
                        metadata,
                        state.generation,
                        request_view.where_clause.clone(),
                        crate::app::database::database_table_effective_order_by(&request_view),
                        request_view.current_page,
                        request_view.limit,
                    )
                }
                _ => return,
            };
        let Some(connection) = self
            .ide_panel
            .database
            .connection(meta.connection_id)
            .map(|node| node.config.clone())
        else {
            if let Some((_, state)) = self.database_table_meta_state_mut(meta.tab_id) {
                state.fail_view_reload("Подключение к базе данных недоступно".to_string());
            }
            return;
        };
        let settings = self.ide_panel.database.settings().clone();
        let secrets = self.connection_job_secrets(meta.connection_id);
        let job_id = self.ide_panel.database.allocate_job_id();
        let pending = DatabasePendingJob {
            id: job_id,
            kind: DatabasePendingJobKind::LoadChunk,
            owner: crate::app::database::DatabaseJobOwner::Table(meta.tab_id),
            connection_id: meta.connection_id,
            database_name: Some(meta.database_name.clone()),
            table_name: Some(meta.table_name.clone()),
        };
        let host_key_policy = self
            .ide_panel
            .database
            .host_key_policy_override
            .take()
            .unwrap_or(SshHostKeyPolicy::Strict);
        let started = self.send_database_command(
            DatabaseCommand::LoadChunk {
                job_id,
                connection,
                database_name: meta.database_name,
                metadata,
                where_clause,
                order_by,
                page,
                limit,
                chunk_index,
                generation,
                secrets,
                settings,
                ssh_options: crate::app::database::host_key_options(host_key_policy),
            },
            pending,
        );
        if !started {
            let message = self
                .ide_panel
                .database
                .global_error
                .clone()
                .unwrap_or_else(|| "Не удалось загрузить данные таблицы".to_string());
            if let Some((_, state)) = self.database_table_meta_state_mut(meta.tab_id) {
                state.fail_view_reload(message);
            }
        }
    }

    pub(crate) fn request_database_table_chunk_for_scroll(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
    ) {
        let chunk_index = self
            .database_table_meta_state(tab_id)
            .map(|(_, state)| {
                let relative_row = (state.grid.scroll_y.target.max(0.0)
                    / crate::app::database::DATABASE_GRID_ROW_HEIGHT)
                    .floor() as usize;
                relative_row / crate::app::database::DATABASE_CHUNK_SIZE
            })
            .unwrap_or(0);
        self.queue_database_table_chunk(tab_id, chunk_index);
    }

    pub(crate) fn on_database_table_count_loaded(
        &mut self,
        connection_id: crate::app::database::DatabaseConnectionId,
        result: crate::app::database::DatabaseTableCountResult,
    ) {
        let tab_id = self.tabs.iter().find_map(|tab| match &tab.kind {
            EditorTabKind::DatabaseTable(meta, state)
                if meta.connection_id == connection_id
                    && meta.database_name == result.database_name
                    && meta.table_name == result.table_name
                    && state.generation == result.generation => Some(meta.tab_id),
            _ => None,
        });
        let Some(tab_id) = tab_id else {
            return;
        };
        if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
            state.commit_view_count(result.count);
        }
        let target_chunk = self.database_table_meta_state(tab_id).map_or(0, |(_, state)| {
            let relative_row = (state.grid.scroll_y.target.max(state.grid.scroll_y.current).max(0.0)
                / crate::app::database::DATABASE_GRID_ROW_HEIGHT)
                .floor() as usize;
            relative_row / crate::app::database::DATABASE_CHUNK_SIZE
        });
        self.queue_database_table_chunk(tab_id, target_chunk);
    }

    pub(crate) fn on_database_table_chunk_loaded(
        &mut self,
        connection_id: crate::app::database::DatabaseConnectionId,
        result: crate::app::database::DatabaseTableChunkResult,
    ) {
        let tab_id = self.tabs.iter().find_map(|tab| match &tab.kind {
            EditorTabKind::DatabaseTable(meta, state)
                if meta.connection_id == connection_id
                    && meta.database_name == result.database_name
                    && meta.table_name == result.table_name
                    && state.generation == result.generation => Some(meta.tab_id),
            _ => None,
        });
        let Some(tab_id) = tab_id else {
            return;
        };
        let Some(transition) = self
            .database_table_meta_state_mut(tab_id)
            .map(|(_, state)| state.commit_view_chunk(result.chunk))
        else {
            return;
        };
        if transition.committed_view {
            self.persist_database_table_view(tab_id);
        }
        if let Some(next) = transition.next_chunk {
            self.queue_database_table_chunk(tab_id, next);
        }
    }

    fn request_database_table_reload(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
        action: DatabaseTableReloadAction,
    ) {
        let Some(apply) = self
            .database_table_meta_state_mut(tab_id)
            .map(|(_, state)| state.request_view_reload(action))
        else {
            return;
        };
        match apply {
            Some(action) => self.apply_database_table_reload(tab_id, action),
            None => {
                self.ide_panel.database.table_modal = Some(DatabaseTableModal::RefreshPrompt {
                    tab_id,
                    close_after_save: false,
                });
            }
        }
    }

    pub(crate) fn apply_database_table_reload(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
        action: DatabaseTableReloadAction,
    ) {
        let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else {
            return;
        };
        state.apply_view_reload(action);
        self.queue_database_table_initial_load(tab_id);
    }

    pub(crate) fn discard_database_table_local_changes(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
    ) {
        if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
            state.discard_local_changes();
        }
    }

    pub fn apply_database_table_filters(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
    ) {
        let result = self
            .database_table_meta_state_mut(tab_id)
            .map(|(_, state)| state.validate_and_prepare_filter_view());
        match result {
            Some(Ok(action)) => self.request_database_table_reload(tab_id, action),
            Some(Err((target, error))) => {
                if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
                    state.grid.filter_error = Some((target, error));
                    state.error = None;
                }
            }
            None => {}
        }
    }

    pub fn database_table_page_first(&mut self, tab_id: crate::app::database::DatabaseTabId) {
        self.set_database_table_page(tab_id, 0);
    }

    pub fn database_table_page_previous(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
    ) {
        let page = self
            .database_table_meta_state(tab_id)
            .and_then(|(_, state)| {
                state.table_page_target(
                    crate::app::database::database_table_view_state::DatabaseTablePageStep::Previous,
                )
            });
        if let Some(page) = page {
            self.set_database_table_page(tab_id, page);
        }
    }

    pub fn database_table_page_next(&mut self, tab_id: crate::app::database::DatabaseTabId) {
        let page = self
            .database_table_meta_state(tab_id)
            .and_then(|(_, state)| {
                state.table_page_target(
                    crate::app::database::database_table_view_state::DatabaseTablePageStep::Next,
                )
            });
        if let Some(page) = page {
            self.set_database_table_page(tab_id, page);
        }
    }

    pub fn database_table_page_last(&mut self, tab_id: crate::app::database::DatabaseTabId) {
        let Some(page) = self
            .database_table_meta_state(tab_id)
            .and_then(|(_, state)| {
                state.table_page_target(
                    crate::app::database::database_table_view_state::DatabaseTablePageStep::Last,
                )
            })
        else {
            return;
        };
        self.set_database_table_page(tab_id, page);
    }

    fn set_database_table_page(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
        page: usize,
    ) {
        let action = self
            .database_table_meta_state(tab_id)
            .and_then(|(_, state)| state.set_table_page(page));
        if let Some(action) = action {
            self.request_database_table_reload(tab_id, action);
        }
    }

    pub fn open_database_table_limit_dialog(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
    ) {
        let limit = self
            .database_table_meta_state(tab_id)
            .map(|(_, state)| state.grid.view.limit)
            .unwrap_or(crate::app::database::DEFAULT_TABLE_LIMIT);
        self.ide_panel.database.open_custom_table_limit(tab_id, limit);
    }

    pub fn apply_database_table_limit_dialog(&mut self) {
        let Some(Ok((tab_id, limit))) = self.ide_panel.database.take_custom_table_limit() else {
            return;
        };
        let action = self
            .database_table_meta_state(tab_id)
            .map(|(_, state)| state.set_table_limit(limit));
        if let Some(action) = action {
            self.request_database_table_reload(tab_id, action);
        }
    }

    pub fn cycle_database_table_sort(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
        column_index: usize,
    ) {
        let action = self
            .database_table_meta_state(tab_id)
            .and_then(|(_, state)| state.sorted_table_view(column_index));
        let Some(action) = action else {
            return;
        };
        self.request_database_table_reload(tab_id, action);
    }

    pub fn add_database_table_row(&mut self, tab_id: crate::app::database::DatabaseTabId) {
        let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else {
            return;
        };
        state.add_local_row();
    }

    pub fn delete_database_table_selection(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
    ) {
        let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else {
            return;
        };
        state.delete_local_selection();
    }

    pub fn undo_database_table_selection(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
    ) {
        let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else {
            return;
        };
        state.undo_local_selection();
    }
}

#[cfg(test)]
mod database_table_app_tests {
    use super::*;

    #[test]
    fn filter_completion_anchor_uses_scrolled_visible_cursor_and_shared_padding() {
        for (target, text) in [
            (
                DatabaseTableInputTarget::Where,
                "customer_name ILIKE '%длинный фильтр%' AND status = active",
            ),
            (
                DatabaseTableInputTarget::OrderBy,
                "customer_name DESC, created_at DESC, id ASC",
            ),
        ] {
            for scale in [1.25_f32, 1.75_f32] {
                let rect_x = 31.4;
                let rect_w = (112.0 * scale).round();
                let text_geometry =
                    database_table_input_text_geometry(rect_x, rect_w, scale, false);
                let expected_padding = (10.0 * scale).round();
                assert_eq!(database_table_input_padding(scale, false), expected_padding);
                assert_eq!(text_geometry.text_start_x, rect_x.round() + expected_padding);
                assert_eq!(
                    text_geometry.content_w,
                    (rect_w.round() - expected_padding * 2.0).max(1.0)
                );

                let advance = |ch: char| {
                    let base = if ch.is_ascii() { 7.2 } else { 9.4 };
                    (base * scale).round().max(1.0)
                };
                let edge_pad = crate::app::single_line_input::single_line_cursor_edge_pad(scale);
                let cursor_geometry = crate::app::single_line_input::single_line_cursor_geometry(
                    text,
                    text.len(),
                    text_geometry.content_w,
                    0.0,
                    edge_pad,
                    edge_pad,
                    advance,
                );
                assert!(cursor_geometry.scroll_x > 0.0, "target={target:?} scale={scale}");

                let anchor_x = crate::app::single_line_input::single_line_rendered_x(
                    text_geometry,
                    cursor_geometry.cursor_x,
                    cursor_geometry.scroll_x,
                )
                .round();
                let unclipped_logical_x =
                    (text_geometry.text_start_x + cursor_geometry.cursor_x).round();
                let clip_right = text_geometry.text_start_x + text_geometry.content_w;
                let caret_w = crate::app::single_line_input::single_line_caret_width(scale);

                assert!(unclipped_logical_x > clip_right, "target={target:?} scale={scale}");
                assert!(anchor_x < unclipped_logical_x, "target={target:?} scale={scale}");
                assert!(anchor_x >= text_geometry.text_start_x, "target={target:?} scale={scale}");
                assert!(anchor_x + caret_w < clip_right, "target={target:?} scale={scale}");
            }
        }
    }
}
