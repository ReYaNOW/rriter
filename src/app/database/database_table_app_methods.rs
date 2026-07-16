use crate::app::database::{
    DatabaseCellEditorKind, DatabaseCellEditorState, DatabaseCellPosition, DatabaseCellValue,
    DatabaseChangePlanOperation, DatabaseGeneration, DatabaseGridCell, DatabaseGridRow,
    DatabaseRowState, DatabaseTableInputTarget, DatabaseTableModal, DatabaseTableReloadAction,
    DatabaseTableViewKey, DatabaseTableViewState,
};

fn database_filter_completion_context(
    target: DatabaseTableInputTarget,
    text: &str,
    cursor: usize,
) -> crate::languages::sql_analysis::SqlCompletionContext {
    let prefix = match target {
        DatabaseTableInputTarget::Where => "SELECT * FROM __rriter_table WHERE ",
        DatabaseTableInputTarget::OrderBy => "SELECT * FROM __rriter_table ORDER BY ",
        DatabaseTableInputTarget::Cell => "",
    };
    let mut source = String::with_capacity(prefix.len() + text.len());
    source.push_str(prefix);
    source.push_str(text);
    let prefix_len = prefix.len();
    let mut context = crate::languages::sql_analysis::completion_context(
        &source,
        prefix_len + cursor.min(text.len()),
    );
    context.replace_range.start = context.replace_range.start.saturating_sub(prefix_len).min(text.len());
    context.replace_range.end = context.replace_range.end.saturating_sub(prefix_len).min(text.len());
    context.scope = 0..text.len();
    context
}

fn database_filter_completion_words(
    metadata: &crate::app::database::DatabaseTableMetadata,
    target: DatabaseTableInputTarget,
    context: &crate::languages::sql_analysis::SqlCompletionContext,
) -> Vec<(crate::app::AutocompleteItem, Vec<usize>)> {
    use crate::languages::sql_analysis::SqlCompletionKind;
    let mut words: Vec<(String, String, crate::highlighter::SymbolKind)> = Vec::new();
    match context.kind {
        SqlCompletionKind::Column => {
            words.extend(metadata.columns.iter().map(|column| {
                (
                    column.name.clone(),
                    crate::app::database::quote_pg_identifier(&column.name),
                    crate::highlighter::SymbolKind::Property,
                )
            }));
        }
        SqlCompletionKind::Operator if target == DatabaseTableInputTarget::Where => {
            for word in ["=", "<>", "!=", "<", ">", "<=", ">=", "IS NULL", "IS NOT NULL", "LIKE", "ILIKE", "IN", "BETWEEN"] {
                words.push((word.to_string(), word.to_string(), crate::highlighter::SymbolKind::Keyword));
            }
        }
        SqlCompletionKind::Value if target == DatabaseTableInputTarget::Where => {
            for word in ["NULL", "TRUE", "FALSE", "CURRENT_DATE", "CURRENT_TIMESTAMP"] {
                words.push((word.to_string(), word.to_string(), crate::highlighter::SymbolKind::Builtin));
            }
            for column in &metadata.columns {
                if column.type_kind == crate::app::database::DatabaseTypeKind::Enum {
                    for value in &column.enum_values {
                        let inserted = format!("'{}'", value.replace('\'', "''"));
                        words.push((value.clone(), inserted, crate::highlighter::SymbolKind::Builtin));
                    }
                }
            }
        }
        SqlCompletionKind::Direction if target == DatabaseTableInputTarget::OrderBy => {
            for word in ["ASC", "DESC"] {
                words.push((word.to_string(), word.to_string(), crate::highlighter::SymbolKind::Keyword));
            }
        }
        SqlCompletionKind::NullOrdering if target == DatabaseTableInputTarget::OrderBy => {
            for word in ["NULLS FIRST", "NULLS LAST"] {
                words.push((word.to_string(), word.to_string(), crate::highlighter::SymbolKind::Keyword));
            }
        }
        SqlCompletionKind::Keyword if target == DatabaseTableInputTarget::Where => {
            for word in ["AND", "OR", "NOT"] {
                words.push((word.to_string(), word.to_string(), crate::highlighter::SymbolKind::Keyword));
            }
        }
        _ => {}
    }
    let prefix = context.prefix.trim_matches('"').to_ascii_lowercase();
    words
        .into_iter()
        .filter_map(|(word, insert_text, kind)| {
            let lower = word.trim_matches('"').trim_matches('\'').to_ascii_lowercase();
            if !prefix.is_empty() && !lower.contains(&prefix) {
                return None;
            }
            let indices = if prefix.is_empty() {
                Vec::new()
            } else {
                lower
                    .match_indices(&prefix)
                    .next()
                    .map(|(start, _)| (start..start + prefix.len()).collect())
                    .unwrap_or_default()
            };
            Some((
                crate::app::AutocompleteItem {
                    word,
                    kind,
                    scope_start: context.scope.start,
                    scope_end: context.scope.end,
                    module: None,
                    module_path: None,
                    detail: Some(match kind {
                        crate::highlighter::SymbolKind::Property => "column".to_string(),
                        crate::highlighter::SymbolKind::Builtin => "value".to_string(),
                        _ => "PostgreSQL".to_string(),
                    }),
                    insert_text: Some(insert_text),
                    text_edit: None,
                    additional_text_edits: Vec::new(),
                },
                indices,
            ))
        })
        .take(64)
        .collect()
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
        let context = database_filter_completion_context(target, &text, cursor);
        if !explicit && !context.automatic {
            self.close_autocomplete();
            return;
        }
        let words = database_filter_completion_words(metadata, target, &context);
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
            let scale = crate::app::database::DATABASE_TABLE_INPUT_TEXT_SCALE;
            let ui_scale = self
                .renderer
                .as_ref()
                .map_or(1.0, |renderer| renderer.scale_factor);
            let padding = 10.0 * ui_scale;
            let visible_w = (rect.2 - padding * 2.0).max(1.0);
            let (scroll_x, prefix_w) = self.renderer.as_mut().map_or((0.0, 0.0), |renderer| {
                let scroll_x = crate::app::file_tree::file_tree_name_input_scroll_x(
                    &text,
                    cursor,
                    visible_w,
                    |ch| {
                        renderer
                            .get_ui_glyph(ch)
                            .map(|glyph| Renderer::snapped_text_advance(glyph.advance, scale))
                            .unwrap_or_else(|| (8.0 * scale).round().max(1.0))
                    },
                );
                let prefix_w = text.get(..cursor).map_or(0.0, |prefix| {
                    prefix
                        .chars()
                        .map(|ch| {
                            renderer
                                .get_ui_glyph(ch)
                                .map(|glyph| Renderer::snapped_text_advance(glyph.advance, scale))
                                .unwrap_or_else(|| (8.0 * scale).round().max(1.0))
                        })
                        .sum::<f32>()
                });
                (scroll_x, prefix_w)
            });
            (
                (rect.0 + padding + prefix_w - scroll_x).round(),
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
            self.autocomplete_scroll.current = 0.0;
            self.autocomplete_scroll.target = 0.0;
            self.autocomplete_scroll.velocity = 0.0;
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
        let selected = self.autocomplete_options[self.autocomplete_selected_idx]
            .0
            .insert_text
            .clone()
            .unwrap_or_else(|| {
                self.autocomplete_options[self.autocomplete_selected_idx]
                    .0
                    .word
                    .clone()
            });
        if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
            let input = match target {
                DatabaseTableInputTarget::Where => &mut state.grid.where_input,
                DatabaseTableInputTarget::OrderBy => &mut state.grid.order_by_input,
                DatabaseTableInputTarget::Cell => unreachable!(),
            };
            let context = database_filter_completion_context(target, input.text(), input.cursor);
            input.replace_range(
                context.replace_range.start,
                context.replace_range.end,
                &selected,
                64 * 1024,
            );
            state.grid.filter_error = None;
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

    fn fail_database_table_reload(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
        message: String,
    ) {
        let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else {
            return;
        };
        state.grid.loading_count = false;
        state.grid.loading_chunk = false;
        state.grid.in_flight_chunk = None;
        state.grid.desired_chunk = None;
        state.grid.finish_refresh();
        state.grid.abort_pending_view();
        state.error = Some(message);
    }

    fn show_database_table_busy_notice(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
        message: String,
    ) {
        let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else {
            return;
        };
        state.grid.loading_count = false;
        state.grid.loading_chunk = false;
        state.grid.in_flight_chunk = None;
        state.grid.desired_chunk = None;
        state.grid.finish_refresh();
        state.grid.abort_pending_view();
        state.error = None;
        state.show_timed_notice(message);
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
        let had_rows = !state.grid.chunks.is_empty();
        if had_rows {
            state.grid.loading_chunk = false;
            state.grid.in_flight_chunk = None;
            state.grid.desired_chunk = None;
            state.grid.refreshing = true;
            state.grid.refresh_started = Some(std::time::Instant::now());
        } else {
            state.grid.clear_loaded_rows();
            state.grid.count = None;
        }
        state.grid.count_error = None;
        state.grid.pending_count = None;
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
            self.show_database_table_busy_notice(
                meta.tab_id,
                "Сейчас уже выполняется другой запрос к базе данных".to_string(),
            );
            return;
        }
        let Some(connection) = self
            .ide_panel
            .database
            .connection(meta.connection_id)
            .map(|node| node.config.clone())
        else {
            self.fail_database_table_reload(
                meta.tab_id,
                "Подключение к базе данных недоступно".to_string(),
            );
            return;
        };
        let Some((_, state)) = self.database_table_meta_state(meta.tab_id) else {
            return;
        };
        let Some(metadata) = state.metadata.clone() else {
            self.fail_database_table_reload(
                meta.tab_id,
                "Метаданные таблицы недоступны".to_string(),
            );
            return;
        };
        let where_clause = state.grid.request_view().where_clause.clone();
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
            self.fail_database_table_reload(meta.tab_id, message);
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
                    if self.ide_panel.database.pending_job.is_some() {
                        if state.grid.refreshing {
                            state.grid.finish_refresh();
                            state.grid.abort_pending_view();
                            state.error = None;
                            state.show_timed_notice(
                                "Сейчас уже выполняется другой запрос к базе данных",
                            );
                        } else {
                            state.grid.desired_chunk = Some(chunk_index);
                        }
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
            self.fail_database_table_reload(
                meta.tab_id,
                "Подключение к базе данных недоступно".to_string(),
            );
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
            self.fail_database_table_reload(meta.tab_id, message);
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
            state.grid.pending_count = Some(result.count);
            state.grid.loading_count = false;
            state.grid.count_error = None;
            let request_view = state
                .grid
                .pending_view
                .as_mut()
                .unwrap_or(&mut state.grid.view);
            let last_page = if result.count == 0 {
                0
            } else {
                (result.count as usize - 1) / request_view.limit
            };
            request_view.current_page = request_view.current_page.min(last_page);
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
        let mut committed_view = false;
        let next = if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
            if state.grid.refreshing {
                state.grid.clear_loaded_rows();
                state.grid.finish_refresh();
            }
            committed_view = state.grid.commit_pending_view();
            state.grid.insert_chunk(result.chunk);
            if let Some(metadata) = state.metadata.clone() {
                state.grid.restore_pending_selection(&metadata);
            }
            state.grid.post_commit_refresh_pending = false;
            state.error = None;
            state.grid.filter_error = None;
            state.clear_notice();
            state.grid.desired_chunk.take()
        } else {
            None
        };
        if committed_view {
            self.persist_database_table_view(tab_id);
        }
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
                    state.grid.where_input.set_text(view.where_clause.clone());
                    state.grid.order_by_input.set_text(view.order_by.clone());
                    state.grid.begin_pending_view(view, false, false);
                    if vertical_context_changed {
                        state.grid.scroll_y.current = 0.0;
                        state.grid.scroll_y.target = 0.0;
                    }
                }
                DatabaseTableReloadAction::ApplyFilterView(view) => {
                    let vertical_context_changed = state.grid.view.current_page != view.current_page
                        || state.grid.view.limit != view.limit
                        || state.grid.view.where_clause != view.where_clause
                        || state.grid.view.order_by != view.order_by;
                    let where_changed = state.grid.view.where_clause != view.where_clause;
                    let order_by_changed = state.grid.view.order_by != view.order_by;
                    state
                        .grid
                        .begin_pending_view(view, where_changed, order_by_changed);
                    if vertical_context_changed {
                        state.grid.scroll_y.current = 0.0;
                        state.grid.scroll_y.target = 0.0;
                    }
                }
            }
            state.grid.pending_reload = None;
            state.error = None;
            state.clear_notice();
        }
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
                .err()
                .map(|error| (DatabaseTableInputTarget::Where, error))
                .or_else(|| {
                    crate::app::database::validate_table_fragment(
                        state.grid.order_by_input.text(),
                        "ORDER BY",
                    )
                    .err()
                    .map(|error| (DatabaseTableInputTarget::OrderBy, error))
                })
        });
        if let Some(error) = validation {
            if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
                state.grid.filter_error = Some(error);
                state.error = None;
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
        if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
            state.grid.filter_error = None;
        }
        view.sorted_column = None;
        view.sort_direction = None;
        view.current_page = 0;
        self.request_database_table_reload(
            tab_id,
            DatabaseTableReloadAction::ApplyFilterView(view),
        );
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
    use super::*;

    fn metadata() -> crate::app::database::DatabaseTableMetadata {
        crate::app::database::DatabaseTableMetadata {
            database_name: "db".to_string(),
            table_name: "items".to_string(),
            columns: vec![
                crate::app::database::DatabaseColumnInfo {
                    ordinal: 1,
                    name: "id".to_string(),
                    type_name: "integer".to_string(),
                    type_oid: 23,
                    type_kind: crate::app::database::DatabaseTypeKind::Other,
                    nullable: false,
                    default_expression: None,
                    identity: false,
                    generated: false,
                    primary_key: true,
                    enum_values: Vec::new(),
                },
                crate::app::database::DatabaseColumnInfo {
                    ordinal: 2,
                    name: "User ID".to_string(),
                    type_name: "text".to_string(),
                    type_oid: 25,
                    type_kind: crate::app::database::DatabaseTypeKind::Other,
                    nullable: true,
                    default_expression: None,
                    identity: false,
                    generated: false,
                    primary_key: false,
                    enum_values: Vec::new(),
                },
            ],
            primary_key_columns: vec!["id".to_string()],
            editable: true,
            read_only_reason: None,
            notices: Vec::new(),
        }
    }

    #[test]
    fn page_math_stays_inside_count() {
        let count = 201usize;
        let limit = 100usize;
        assert_eq!(count.saturating_sub(1) / limit, 2);
        assert_eq!(0usize.saturating_sub(1) / limit, 0);
    }

    #[test]
    fn filter_completion_reuses_columns_and_quotes_complex_identifiers() {
        let context = database_filter_completion_context(
            DatabaseTableInputTarget::Where,
            "Us",
            2,
        );
        assert_eq!(
            context.kind,
            crate::languages::sql_analysis::SqlCompletionKind::Column
        );
        let options = database_filter_completion_words(
            &metadata(),
            DatabaseTableInputTarget::Where,
            &context,
        );
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].0.word, "User ID");
        assert_eq!(options[0].0.insert_text.as_deref(), Some("\"User ID\""));
    }

    #[test]
    fn order_by_completion_includes_directions() {
        let context = crate::languages::sql_analysis::SqlCompletionContext {
            kind: crate::languages::sql_analysis::SqlCompletionKind::Direction,
            prefix: "DE".to_string(),
            replace_range: 5..7,
            scope: 0..7,
            automatic: true,
            ..crate::languages::sql_analysis::SqlCompletionContext::default()
        };
        let options = database_filter_completion_words(
            &metadata(),
            DatabaseTableInputTarget::OrderBy,
            &context,
        );
        assert!(options.iter().any(|(item, _)| item.word == "DESC"));
    }

    #[test]
    fn where_completion_after_operator_waits_for_a_value() {
        let context = database_filter_completion_context(
            DatabaseTableInputTarget::Where,
            "\"id\" = ",
            7,
        );
        assert_eq!(
            context.kind,
            crate::languages::sql_analysis::SqlCompletionKind::Value
        );
        assert!(!context.automatic);
    }

    #[test]
    fn filter_completion_replaces_only_current_sql_word() {
        let context = database_filter_completion_context(
            DatabaseTableInputTarget::Where,
            "id = Us",
            7,
        );
        assert_eq!(context.replace_range, 5..7);
        assert_eq!(context.prefix, "Us");
    }
}
