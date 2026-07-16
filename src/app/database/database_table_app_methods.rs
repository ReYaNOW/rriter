use crate::app::database::{
    DatabaseCellEditorKind, DatabaseCellEditorState, DatabaseCellPosition, DatabaseCellValue,
    DatabaseChangePlanOperation, DatabaseGeneration, DatabaseGridCell, DatabaseGridRow,
    DatabaseRowState, DatabaseTableInputTarget, DatabaseTableModal, DatabaseTableReloadAction,
    DatabaseTableViewKey, DatabaseTableViewState,
};

impl App {
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
        if let Some(view) = self
            .ide_panel
            .database
            .persisted
            .table_views
            .iter()
            .find(|view| view.key == key)
        {
            return view.clone();
        }
        let view = DatabaseTableViewState {
            key,
            limit: self.ide_panel.database.settings().default_table_limit,
            ..DatabaseTableViewState::default()
        };
        self.ide_panel.database.persisted.table_views.push(view.clone());
        view
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
        if let Some(existing) = self
            .ide_panel
            .database
            .persisted
            .table_views
            .iter_mut()
            .find(|existing| existing.key == view.key)
        {
            *existing = view;
        } else {
            self.ide_panel.database.persisted.table_views.push(view);
        }
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
        let Some((meta, state)) = self.database_table_meta_state_mut(tab_id) else {
            return;
        };
        if state.metadata.is_none() {
            return;
        }
        state.generation = state.generation.next();
        state.grid.clear_loaded_rows();
        state.grid.count = None;
        state.grid.count_error = None;
        state.grid.loading_count = true;
        let generation = state.generation;
        let meta = meta.clone();
        self.queue_database_table_count(meta, generation);
    }

    fn queue_database_table_count(
        &mut self,
        meta: DatabaseTableTabMeta,
        generation: DatabaseGeneration,
    ) {
        if self.ide_panel.database.pending_job.is_some() {
            if let Some((_, state)) = self.database_table_meta_state_mut(meta.tab_id) {
                state.grid.loading_count = false;
                state.grid.count_error = Some(
                    "Сейчас уже выполняется другой запрос к базе данных".to_string(),
                );
            }
            return;
        }
        let Some(connection) = self
            .ide_panel
            .database
            .connection(meta.connection_id)
            .map(|node| node.config.clone())
        else {
            return;
        };
        let Some((_, state)) = self.database_table_meta_state(meta.tab_id) else {
            return;
        };
        let Some(metadata) = state.metadata.clone() else {
            return;
        };
        let where_clause = state.grid.view.where_clause.clone();
        let settings = self.ide_panel.database.settings().clone();
        let secrets = self.connection_job_secrets(meta.connection_id);
        let job_id = self.ide_panel.database.allocate_job_id();
        let pending = DatabasePendingJob {
            id: job_id,
            kind: DatabasePendingJobKind::CountRows,
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
        self.send_database_command(
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
                    if state.grid.chunks.contains_key(&chunk_index) {
                        state.grid.touch_chunk(chunk_index);
                        return;
                    }
                    if state.grid.loading_chunk || self.ide_panel.database.pending_job.is_some() {
                        state.grid.desired_chunk = Some(chunk_index);
                        return;
                    }
                    let Some(metadata) = state.metadata.clone() else {
                        return;
                    };
                    state.grid.loading_chunk = true;
                    state.grid.in_flight_chunk = Some(chunk_index);
                    (
                        meta.clone(),
                        metadata,
                        state.generation,
                        state.grid.view.where_clause.clone(),
                        state.grid.view.order_by.clone(),
                        state.grid.view.current_page,
                        state.grid.view.limit,
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
            return;
        };
        let settings = self.ide_panel.database.settings().clone();
        let secrets = self.connection_job_secrets(meta.connection_id);
        let job_id = self.ide_panel.database.allocate_job_id();
        let pending = DatabasePendingJob {
            id: job_id,
            kind: DatabasePendingJobKind::LoadChunk,
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
        self.send_database_command(
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
            state.grid.count = Some(result.count);
            state.grid.loading_count = false;
            state.grid.count_error = None;
            let last_page = if result.count == 0 {
                0
            } else {
                (result.count as usize - 1) / state.grid.view.limit
            };
            state.grid.view.current_page = state.grid.view.current_page.min(last_page);
        }
        let target_chunk = self.database_table_meta_state(tab_id).map_or(0, |(_, state)| {
            let relative_row = (state.grid.scroll_y.target.max(state.grid.scroll_y.current).max(0.0)
                / crate::app::database::DATABASE_GRID_ROW_HEIGHT)
                .floor() as usize;
            relative_row / crate::app::database::DATABASE_CHUNK_SIZE
        });
        self.persist_database_table_view(tab_id);
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
        let next = if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
            state.grid.insert_chunk(result.chunk);
            if let Some(metadata) = state.metadata.clone() {
                state.grid.restore_pending_selection(&metadata);
            }
            state.grid.post_commit_refresh_pending = false;
            state.error = None;
            state.grid.desired_chunk.take()
        } else {
            None
        };
        if let Some(next) = next {
            self.queue_database_table_chunk(tab_id, next);
        }
    }

    fn request_database_table_reload(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
        action: DatabaseTableReloadAction,
    ) {
        let dirty = self
            .database_table_meta_state(tab_id)
            .is_some_and(|(_, state)| state.grid.dirty());
        if dirty {
            if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
                state.grid.pending_reload = Some(action);
            }
            self.ide_panel.database.table_modal = Some(DatabaseTableModal::RefreshPrompt {
                tab_id,
                close_after_save: false,
            });
            return;
        }
        self.apply_database_table_reload(tab_id, action);
    }

    pub(crate) fn apply_database_table_reload(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
        action: DatabaseTableReloadAction,
    ) {
        if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
            match action {
                DatabaseTableReloadAction::Refresh => {}
                DatabaseTableReloadAction::ApplyView(view) => {
                    let vertical_context_changed = state.grid.view.current_page != view.current_page
                        || state.grid.view.limit != view.limit
                        || state.grid.view.where_clause != view.where_clause
                        || state.grid.view.order_by != view.order_by;
                    state.grid.view = view;
                    state.grid.where_input.set_text(state.grid.view.where_clause.clone());
                    state.grid.order_by_input.set_text(state.grid.view.order_by.clone());
                    if vertical_context_changed {
                        state.grid.scroll_y.current = 0.0;
                        state.grid.scroll_y.target = 0.0;
                    }
                }
            }
            state.grid.pending_reload = None;
            state.error = None;
        }
        self.persist_database_table_view(tab_id);
        self.queue_database_table_initial_load(tab_id);
    }

    pub(crate) fn discard_database_table_local_changes(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
    ) {
        if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
            state.grid.added_rows.clear();
            for chunk in state.grid.chunks.values_mut() {
                for row in &mut chunk.rows {
                    row.state = DatabaseRowState::Clean;
                    for cell in &mut row.cells {
                        cell.undo();
                    }
                }
            }
            state.grid.cell_editor = None;
            state.grid.focused_input = None;
        }
    }

    pub fn apply_database_table_filters(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
    ) {
        let validation = self.database_table_meta_state(tab_id).and_then(|(_, state)| {
            crate::app::database::validate_table_fragment(state.grid.where_input.text(), "WHERE")
                .and_then(|_| {
                    crate::app::database::validate_table_fragment(
                        state.grid.order_by_input.text(),
                        "ORDER BY",
                    )
                })
                .err()
        });
        if let Some(error) = validation {
            if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
                state.error = Some(error);
            }
            return;
        }
        let Some(mut view) = self
            .database_table_meta_state(tab_id)
            .map(|(_, state)| state.grid.view.clone())
        else {
            return;
        };
        if let Some((_, state)) = self.database_table_meta_state(tab_id) {
            view.where_clause = state.grid.where_input.text().to_string();
            view.order_by = state.grid.order_by_input.text().to_string();
        }
        view.sorted_column = None;
        view.sort_direction = None;
        view.current_page = 0;
        self.request_database_table_reload(tab_id, DatabaseTableReloadAction::ApplyView(view));
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
            .map(|(_, state)| state.grid.view.current_page.saturating_sub(1))
            .unwrap_or(0);
        self.set_database_table_page(tab_id, page);
    }

    pub fn database_table_page_next(&mut self, tab_id: crate::app::database::DatabaseTabId) {
        let page = self
            .database_table_meta_state(tab_id)
            .map(|(_, state)| {
                let count = state.grid.count.unwrap_or(0) as usize;
                let last = count.saturating_sub(1) / state.grid.view.limit;
                (state.grid.view.current_page + 1).min(last)
            })
            .unwrap_or(0);
        self.set_database_table_page(tab_id, page);
    }

    pub fn database_table_page_last(&mut self, tab_id: crate::app::database::DatabaseTabId) {
        let page = self
            .database_table_meta_state(tab_id)
            .map(|(_, state)| {
                let count = state.grid.count.unwrap_or(0) as usize;
                count.saturating_sub(1) / state.grid.view.limit
            })
            .unwrap_or(0);
        self.set_database_table_page(tab_id, page);
    }

    fn set_database_table_page(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
        page: usize,
    ) {
        let Some(mut view) = self
            .database_table_meta_state(tab_id)
            .map(|(_, state)| state.grid.view.clone())
        else {
            return;
        };
        if view.current_page == page
            && self
                .database_table_meta_state(tab_id)
                .is_some_and(|(_, state)| !state.grid.chunks.is_empty())
        {
            return;
        }
        view.current_page = page;
        self.request_database_table_reload(tab_id, DatabaseTableReloadAction::ApplyView(view));
    }

    pub fn open_database_table_limit_dialog(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
    ) {
        let limit = self
            .database_table_meta_state(tab_id)
            .map(|(_, state)| state.grid.view.limit)
            .unwrap_or(crate::app::database::DEFAULT_TABLE_LIMIT);
        self.ide_panel.database.table_modal = Some(DatabaseTableModal::CustomLimit {
            tab_id,
            input: crate::app::database::DatabaseDialogInput::new(limit.to_string()),
            error: None,
        });
    }

    pub fn apply_database_table_limit_dialog(&mut self) {
        let Some(DatabaseTableModal::CustomLimit { tab_id, input, .. }) =
            self.ide_panel.database.table_modal.as_ref()
        else {
            return;
        };
        let tab_id = *tab_id;
        let parsed = input.text().trim().parse::<usize>();
        let limit = match parsed {
            Ok(value) if (1..=crate::app::database::MAX_CUSTOM_TABLE_LIMIT).contains(&value) => value,
            _ => {
                if let Some(DatabaseTableModal::CustomLimit { error, .. }) =
                    self.ide_panel.database.table_modal.as_mut()
                {
                    *error = Some("Лимит должен быть от 1 до 10000".to_string());
                }
                return;
            }
        };
        self.ide_panel.database.table_modal = None;
        let Some(mut view) = self
            .database_table_meta_state(tab_id)
            .map(|(_, state)| state.grid.view.clone())
        else {
            return;
        };
        view.limit = limit;
        view.current_page = 0;
        self.request_database_table_reload(tab_id, DatabaseTableReloadAction::ApplyView(view));
    }

    pub fn cycle_database_table_sort(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
        column_index: usize,
    ) {
        let Some((column, mut view)) = self.database_table_meta_state(tab_id).and_then(|(_, state)| {
            let column = state.metadata.as_ref()?.columns.get(column_index)?.clone();
            Some((column, state.grid.view.clone()))
        }) else {
            return;
        };
        match (view.sorted_column.as_deref(), view.sort_direction) {
            (Some(name), Some(crate::app::database::DatabaseSortDirection::Asc))
                if name == column.name =>
            {
                view.sort_direction = Some(crate::app::database::DatabaseSortDirection::Desc);
                view.order_by = format!(
                    "{} DESC",
                    crate::app::database::quote_pg_identifier(&column.name)
                );
            }
            (Some(name), Some(crate::app::database::DatabaseSortDirection::Desc))
                if name == column.name =>
            {
                view.sorted_column = None;
                view.sort_direction = None;
                view.order_by.clear();
            }
            _ => {
                view.sorted_column = Some(column.name.clone());
                view.sort_direction = Some(crate::app::database::DatabaseSortDirection::Asc);
                view.order_by = format!(
                    "{} ASC",
                    crate::app::database::quote_pg_identifier(&column.name)
                );
            }
        }
        view.current_page = 0;
        self.request_database_table_reload(tab_id, DatabaseTableReloadAction::ApplyView(view));
    }

    pub fn add_database_table_row(&mut self, tab_id: crate::app::database::DatabaseTabId) {
        let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else {
            return;
        };
        let Some(metadata) = state.metadata.as_ref() else {
            return;
        };
        if !metadata.editable {
            state.error = metadata.read_only_reason.clone();
            return;
        }
        let page_base = state.grid.view.current_page.saturating_mul(state.grid.view.limit);
        let server_rows = state.grid.count.map_or(state.grid.view.limit, |count| {
            (count as usize).saturating_sub(page_base).min(state.grid.view.limit)
        });
        let absolute_index = page_base
            .saturating_add(server_rows)
            .saturating_add(state.grid.added_rows.len());
        let cells = metadata
            .columns
            .iter()
            .map(|column| {
                let value = if column.identity || column.generated || column.default_expression.is_some() {
                    DatabaseCellValue::Default
                } else {
                    DatabaseCellValue::Null
                };
                DatabaseGridCell::new(value)
            })
            .collect();
        state.grid.added_rows.push(DatabaseGridRow {
            absolute_index,
            cells,
            xmin: None,
            state: DatabaseRowState::Added,
        });
        state.grid.selection.select_row(absolute_index, false, false);
    }

    pub fn delete_database_table_selection(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
    ) {
        let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else {
            return;
        };
        if !state.metadata.as_ref().is_some_and(|metadata| metadata.editable) {
            state.error = state
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.read_only_reason.clone());
            return;
        }
        let mut rows = state.grid.selection.selected_rows.clone();
        if rows.is_empty()
            && let Some((start, end)) = state.grid.selection.cell_range()
        {
            rows.extend(start.row..=end.row);
        }
        rows.sort_unstable();
        rows.dedup();
        state.grid.added_rows.retain(|row| !rows.contains(&row.absolute_index));
        for row_index in rows {
            if let Some(row) = state.grid.row_mut(row_index) {
                row.state = if row.state == DatabaseRowState::Deleted {
                    DatabaseRowState::Clean
                } else {
                    DatabaseRowState::Deleted
                };
            }
        }
    }

    pub fn undo_database_table_selection(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
    ) {
        let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else {
            return;
        };
        if !state.grid.selection.selected_rows.is_empty() {
            for row_index in state.grid.selection.selected_rows.clone() {
                if let Some(row) = state.grid.row_mut(row_index) {
                    row.state = DatabaseRowState::Clean;
                    for cell in &mut row.cells {
                        cell.undo();
                    }
                }
            }
            return;
        }
        if let Some((start, end)) = state.grid.selection.cell_range() {
            for row_index in start.row..=end.row {
                if let Some(row) = state.grid.row_mut(row_index) {
                    for column in start.column..=end.column {
                        if let Some(cell) = row.cells.get_mut(column) {
                            cell.undo();
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod database_table_app_tests {
    #[test]
    fn page_math_stays_inside_count() {
        let count = 201usize;
        let limit = 100usize;
        assert_eq!(count.saturating_sub(1) / limit, 2);
        assert_eq!(0usize.saturating_sub(1) / limit, 0);
    }
}
