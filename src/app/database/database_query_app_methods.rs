use crate::app::database::{
    DatabaseQueryHistoryEntry, DatabaseQueryMode, SqlConsoleId, analysis_error_ranges,
    analyze_database_query_sql, completion_words_for_context,
    database_query_editor_diagnostics,
    database_query_completion_context, format_database_sql, history_started_now,
};

impl App {
    pub(crate) fn jump_to_active_database_query_diagnostic(&mut self, index: usize) {
        let text = self.editor.get_full_text();
        let offset = self
            .active_database_query_meta_state()
            .and_then(|(_, state)| state.editor_diagnostics.get(index))
            .map(|diagnostic| database_query_diagnostic_navigation_offset(&text, diagnostic));
        let Some(offset) = offset else {
            return;
        };
        self.editor.cursor = offset.min(self.editor.len());
        self.editor.selection_anchor = None;
        self.reprioritize_highlighter_around_cursor();
        if let (Some(window), Some(renderer)) = (self.window.as_ref(), self.renderer.as_mut()) {
            let size = window.inner_size();
            let top_inset = crate::render_view::editor_content_top_inset(
                self.show_welcome,
                self.is_ide_mode,
                true,
                renderer.scale_factor,
            );
            App::ensure_cursor_visible(
                &mut self.scroll_y.target,
                &mut self.scroll_x.target,
                &self.editor,
                renderer,
                size.width as f32,
                size.height as f32,
                top_inset,
            );
            window.request_redraw();
        }
        self.last_action = std::time::Instant::now();
    }

    pub(crate) fn jump_to_next_active_database_query_diagnostic(&mut self) {
        let text = self.editor.get_full_text();
        let cursor = self.editor.cursor;
        let Some((_, state)) = self.active_database_query_meta_state() else {
            return;
        };
        let Some(offset) = crate::app::database::next_database_query_diagnostic_offset(
            &state.editor_diagnostics,
            &text,
            cursor,
        ) else {
            return;
        };
        self.editor.cursor = offset.min(self.editor.len());
        self.editor.selection_anchor = None;
        self.reprioritize_highlighter_around_cursor();
        if let (Some(window), Some(renderer)) = (self.window.as_ref(), self.renderer.as_mut()) {
            let size = window.inner_size();
            let tab_bar_h = crate::render_view::editor_content_top_inset(
                self.show_welcome,
                self.is_ide_mode,
                true,
                renderer.scale_factor,
            );
            App::ensure_cursor_visible(
                &mut self.scroll_y.target,
                &mut self.scroll_x.target,
                &self.editor,
                renderer,
                size.width as f32,
                size.height as f32,
                tab_bar_h,
            );
            window.request_redraw();
        }
        self.last_action = std::time::Instant::now();
    }

    pub(crate) fn clear_stale_active_database_query_diagnostic(&mut self) -> bool {
        let editor_version = self.editor.version;
        let text = self.editor.get_full_text();
        let line_offsets = self.editor.line_offsets.clone();
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
        state.editor_diagnostics = database_query_editor_diagnostics(
            &state.analysis,
            None,
            &text,
            &line_offsets,
        );
        let local_ranges = analysis_error_ranges(&state.analysis);
        tab.syntax_errors = local_ranges.clone();
        self.highlighter.syntax_errors = local_ranges;
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

    fn active_database_query_meta_state_mut(
        &mut self,
    ) -> Option<(&DatabaseQueryTabMeta, &mut DatabaseQueryTabState)> {
        self.tabs
            .get_mut(self.active_tab)
            .and_then(|tab| match &mut tab.kind {
                EditorTabKind::DatabaseQuery(meta, state) => Some((&*meta, state)),
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

    pub(crate) fn refresh_active_database_query_analysis(&mut self) -> bool {
        let text = self.editor.get_full_text();
        let editor_version = self.editor.version;
        let Some((meta, state)) = self.active_database_query_meta_state() else {
            return false;
        };
        if state.analysis_editor_version == Some(editor_version) {
            return false;
        }
        let connection_id = meta.connection_id;
        let console_id = meta.console_id;
        let metadata = state.completion.clone();
        let analysis = analyze_database_query_sql(&metadata, &text);
        let editor_diagnostics = database_query_editor_diagnostics(
            &analysis,
            None,
            &text,
            &self.editor.line_offsets,
        );
        let error_ranges = analysis_error_ranges(&analysis);
        let Some(index) = self.database_query_tab_index(connection_id, console_id) else {
            return false;
        };
        if let EditorTabKind::DatabaseQuery(_, state) = &mut self.tabs[index].kind {
            state.analysis = analysis;
            state.analysis_editor_version = Some(editor_version);
            state.editor_diagnostics = editor_diagnostics;
        }
        self.tabs[index].syntax_errors = error_ranges.clone();
        if index == self.active_tab {
            self.highlighter.syntax_errors = error_ranges;
        }
        true
    }

    pub(crate) fn update_active_database_query_completion(&mut self, explicit: bool) {
        let Some((meta, state)) = self.active_database_query_meta_state() else {
            return;
        };
        if !state.completion_loaded {
            self.request_active_database_query_completion();
            if !explicit {
                self.close_autocomplete();
            }
            return;
        }
        let connection_id = meta.connection_id;
        let console_id = meta.console_id;
        let metadata = state.completion.clone();
        let text = self.editor.get_full_text();
        let cursor = self.editor.cursor.min(text.len());
        let mut analysis = if state.analysis_editor_version == Some(self.editor.version) {
            state.analysis.clone()
        } else {
            analyze_database_query_sql(&metadata, &text)
        };
        let context = database_query_completion_context(&text, cursor);
        if let Some(recovered) = crate::app::database::completion_recovery_analysis(
            &metadata,
            &text,
            cursor,
            &context,
            &analysis,
        ) {
            analysis = recovered;
        }
        if !explicit && !context.automatic {
            self.close_autocomplete();
            return;
        }
        let words = completion_words_for_context(&metadata, &analysis, &context, cursor);
        if words.is_empty() {
            self.close_autocomplete();
            return;
        }

        let context_key = database_query_completion_session_key(
            connection_id,
            console_id,
            &context,
        );
        let (start_line, start_col) = crate::lsp::offset_to_lsp_pos(
            &text,
            context.replace_range.start,
            &self.editor.line_offsets,
        );
        let (end_line, end_col) = crate::lsp::offset_to_lsp_pos(
            &text,
            context.replace_range.end,
            &self.editor.line_offsets,
        );
        let prefix = context.prefix.clone();
        let options = words
            .into_iter()
            .map(|(word, detail)| {
                let kind = match detail.as_str() {
                    "table" | "CTE" => SymbolKind::Class,
                    "function" => SymbolKind::Function,
                    "operator" | "SQL" | "ORDER BY" => SymbolKind::Keyword,
                    "enum" | "value" => SymbolKind::Builtin,
                    _ => SymbolKind::Property,
                };
                let text_edit = crate::lsp::TextChange {
                    start_line,
                    start_col,
                    end_line,
                    end_col,
                    new_text: word.clone(),
                };
                let visible_word = word.trim_matches('"').trim_matches('\'');
                let match_indices = crate::app::autocomplete_match_candidate(&prefix, visible_word)
                    .map(|(_, indices)| indices)
                    .unwrap_or_default();
                (
                    AutocompleteItem {
                        word,
                        kind,
                        scope_start: context.scope.start,
                        scope_end: context.scope.end,
                        module: None,
                        module_path: None,
                        detail: Some(detail),
                        insert_text: None,
                        text_edit: Some(text_edit),
                        additional_text_edits: Vec::new(),
                    },
                    match_indices,
                )
            })
            .collect();
        let anchor = self.database_query_autocomplete_anchor();
        self.update_autocomplete_session(
            AutocompleteMode::Sql,
            Some(context_key),
            options,
            anchor,
            false,
        );
    }

    fn database_query_autocomplete_anchor(&mut self) -> Option<(f32, f32)> {
        let renderer = self.renderer.as_mut()?;
        let scale = renderer.scale_factor;
        let content_top = crate::render_view::editor_content_top_inset(
            self.show_welcome,
            self.is_ide_mode,
            true,
            scale,
        );
        let render_scroll_y = self.scroll_y.current.round() - content_top;
        let (cursor_x, cursor_y) = renderer.get_cursor_xy(&self.editor);
        Some((
            (cursor_x + 2.0 * scale).round(),
            (cursor_y - render_scroll_y + renderer.line_height * 0.72).round(),
        ))
    }

    pub(crate) fn request_active_database_query_completion(&mut self) {
        let Some((meta, state)) = self.active_database_query_meta_state() else {
            return;
        };
        if state.completion_loaded {
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
            owner: crate::app::database::DatabaseJobOwner::Query(meta.console_id),
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
        let Some(index) = self.database_query_tab_index(meta.connection_id, meta.console_id) else {
            return;
        };
        let plan = match &mut self.tabs[index].kind {
            EditorTabKind::DatabaseQuery(_, state) => state.database_query_prepare_execution(
                &text,
                selection,
                self.editor.cursor,
                self.editor.version,
                &self.editor.line_offsets,
            ),
            _ => None,
        };
        let Some(plan) = plan else {
            self.ide_panel.database.global_error = Some("SQL-консоль пуста".to_string());
            return;
        };
        self.tabs[index].syntax_errors = plan.error_ranges.clone();
        if index == self.active_tab {
            self.highlighter.syntax_errors = plan.error_ranges;
        }
        if plan.has_errors {
            return;
        }
        let sql = plan.sql;
        let source_offset = plan.source_offset;
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
            owner: crate::app::database::DatabaseJobOwner::Query(meta.console_id),
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
        if !database_query_transaction_finish_allowed(review.finishing) {
            return;
        }
        let meta = meta.clone();
        let transaction_id = review.transaction_id;
        if let Some(index) = self.database_query_tab_index(meta.connection_id, meta.console_id)
            && let EditorTabKind::DatabaseQuery(_, state) = &mut self.tabs[index].kind
            && let Some(review) = state.review.as_mut()
        {
            review.finishing = true;
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
            owner: crate::app::database::DatabaseJobOwner::Query(meta.console_id),
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
        if let Some(index) = self.database_query_tab_index(meta.connection_id, meta.console_id)
            && let EditorTabKind::DatabaseQuery(_, state) = &mut self.tabs[index].kind
        {
            state.database_query_toggle_history();
        }
    }

    pub fn select_active_database_query_result(&mut self, index: usize) {
        let Some((meta, _)) = self.active_database_query_meta_state() else {
            return;
        };
        if let Some(tab_index) = self.database_query_tab_index(meta.connection_id, meta.console_id)
            && let EditorTabKind::DatabaseQuery(_, state) = &mut self.tabs[tab_index].kind
        {
            state.database_query_select_result(index);
        }
    }

    pub fn load_database_query_history_entry(&mut self, visible_index: usize) {
        let Some((meta, _)) = self.active_database_query_meta_state() else {
            return;
        };
        let connection_id = meta.connection_id;
        let console_id = meta.console_id;
        let database_name = meta.database_name.clone();
        let entry = self.ide_panel.database.database_query_history_entry(
            connection_id,
            &database_name,
            visible_index,
        );
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

    fn active_database_query_history_len(&self) -> usize {
        let Some((meta, _)) = self.active_database_query_meta_state() else {
            return 0;
        };
        self.ide_panel.database.database_query_history_len(
            meta.connection_id,
            &meta.database_name,
        )
    }

    pub(crate) fn move_active_database_query_history_selection(&mut self, delta: i32) {
        let len = self.active_database_query_history_len();
        if len == 0 {
            return;
        }
        let Some((meta, _)) = self.active_database_query_meta_state() else {
            return;
        };
        if let Some(index) = self.database_query_tab_index(meta.connection_id, meta.console_id)
            && let EditorTabKind::DatabaseQuery(_, state) = &mut self.tabs[index].kind
        {
            state.database_query_move_history_selection(delta, len);
        }
    }

    pub(crate) fn set_active_database_query_history_selection(&mut self, last: bool) {
        let len = self.active_database_query_history_len();
        let Some((meta, _)) = self.active_database_query_meta_state() else {
            return;
        };
        if let Some(index) = self.database_query_tab_index(meta.connection_id, meta.console_id)
            && let EditorTabKind::DatabaseQuery(_, state) = &mut self.tabs[index].kind
        {
            state.database_query_set_history_selection(last, len);
        }
    }

    pub fn show_active_database_query_completion(&mut self) {
        self.refresh_active_database_query_analysis();
        self.update_active_database_query_completion(true);
    }

    pub(crate) fn record_database_query_history(
        &mut self,
        entry: DatabaseQueryHistoryEntry,
    ) {
        if !self.ide_panel.database.database_query_record_history(entry) {
            return;
        }
        for tab in &mut self.tabs {
            if let EditorTabKind::DatabaseQuery(_, state) = &mut tab.kind {
                state.result_view.invalidate_history_layout();
            }
        }
        self.save_database_panel_state();
    }

    pub(crate) fn adjust_database_setting(&mut self, setting: usize, delta: i32) {
        if self
            .ide_panel
            .database
            .database_query_adjust_setting(setting, delta)
        {
            self.save_database_panel_state();
        }
    }
    pub(crate) fn scroll_active_database_query_review_messages_to_pointer(&mut self) {
        let Some(rect) = self
            .ui_registry
            .rect_for(crate::ui_system::UiId::DatabaseQueryReviewMessagesScrollY)
        else {
            return;
        };
        let pointer = self
            .renderer
            .as_ref()
            .map_or(rect.1, |renderer| renderer.last_mouse_y);
        let scale = self
            .renderer
            .as_ref()
            .map_or(1.0, |renderer| renderer.scale_factor);
        let Some((_, state)) = self.active_database_query_meta_state_mut() else {
            return;
        };
        let max_scroll = state.result_view.review_message_max_scroll.get();
        if max_scroll <= 0.0 || rect.3 <= 0.0 {
            return;
        }
        let Some(thumb) = crate::scroll::scrollbar_thumb(
            rect.1,
            rect.3,
            rect.3,
            rect.3 + max_scroll,
            state.result_view.review_message_scroll_y.current,
            (28.0 * scale).round(),
        ) else {
            return;
        };
        let Some((drag_offset, target)) = crate::scroll::scrollbar_drag_target(
            pointer,
            rect.1,
            rect.3,
            thumb,
            max_scroll,
            None,
        ) else {
            return;
        };
        let scroll = &mut state.result_view.review_message_scroll_y;
        crate::app::mouse::apply_scrollbar_drag_target(scroll, target, drag_offset);
    }

    pub(crate) fn start_database_query_result_resize(&mut self) {
        if let Some((_, state)) = self.active_database_query_meta_state_mut() {
            state.database_query_start_result_resize();
        }
    }

    fn update_database_query_result_resize(&mut self, mouse_y: f32) -> bool {
        let Some((_, state)) = self.active_database_query_meta_state() else {
            return false;
        };
        if !state.result_view.is_resizing_height {
            return false;
        }
        let Some(renderer) = self.renderer.as_ref() else {
            return false;
        };
        let Some(window) = self.window.as_ref() else {
            return false;
        };
        let scale = renderer.scale_factor.max(f32::EPSILON);
        let window_height = window.inner_size().height as f32;
        let status_bar_height = crate::render_view::ide_status_bar_height(scale);
        let reserved_bottom = self.ide_panel.editor_reserved_bottom_height(scale);
        let results_bottom = (window_height - status_bar_height - reserved_bottom).max(0.0);
        let panel_bottom_height = if self.ide_panel.any_bottom_open() {
            self.ide_panel.bottom_height * scale
        } else {
            0.0
        };
        let requested_height = ((results_bottom - mouse_y) / scale)
            .max(crate::app::database::DATABASE_QUERY_RESULTS_MIN_HEIGHT);
        let preferred_height = crate::app::database::database_query_results_height(
            requested_height,
            window_height,
            panel_bottom_height,
            scale,
        ) / scale;
        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return false;
        };
        let EditorTabKind::DatabaseQuery(_, state) = &mut tab.kind else {
            return false;
        };
        state.database_query_set_result_height(preferred_height);
        true
    }

    pub(crate) fn auto_size_active_database_query_column(&mut self, column_index: usize) {
        if let Some((_, state)) = self.active_database_query_meta_state_mut() {
            state.database_query_auto_size_column(column_index);
        }
    }

    pub(crate) fn start_database_query_column_resize(
        &mut self,
        column_index: usize,
        mouse_x: f32,
    ) {
        if let Some((_, state)) = self.active_database_query_meta_state_mut() {
            state.database_query_start_column_resize(column_index, mouse_x);
        }
    }

    pub(crate) fn start_database_query_scroll_drag(&mut self, horizontal: bool) {
        let mouse = self.renderer.as_ref().map_or((0.0, 0.0), |renderer| {
            (renderer.last_mouse_x, renderer.last_mouse_y)
        });
        let body_rect = self
            .ui_registry
            .rect_for(crate::ui_system::UiId::DatabaseQueryResultBody);
        let vertical_rect = self
            .ui_registry
            .rect_for(crate::ui_system::UiId::DatabaseQueryScrollY);
        let horizontal_rect = self
            .ui_registry
            .rect_for(crate::ui_system::UiId::DatabaseQueryScrollX);
        let scale = self.renderer.as_ref().map_or(1.0, |renderer| renderer.scale_factor);
        let history = &self.ide_panel.database.persisted.query_history;
        let Some((meta, state)) = self.active_database_query_meta_state() else {
            return;
        };
        let (viewport_w, viewport_h) = database_query_scroll_viewport(
            body_rect,
            horizontal_rect,
            vertical_rect,
            scale,
        );
        let (max_x, max_y) = crate::app::database::database_query_scroll_limits(
            meta,
            state,
            history,
            viewport_w,
            viewport_h,
            scale,
        );
        let (rect, pointer, viewport, max_scroll, current, min_thumb) = if horizontal {
            (
                horizontal_rect,
                mouse.0,
                viewport_w,
                max_x,
                state.result_view.scroll_x.current,
                (36.0 * scale).round(),
            )
        } else {
            (
                vertical_rect,
                mouse.1,
                viewport_h,
                max_y,
                state.result_view.scroll_y.current,
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
        let Some((_, state)) = self.active_database_query_meta_state_mut() else {
            return;
        };
        let scroll = if horizontal {
            &mut state.result_view.scroll_x
        } else {
            &mut state.result_view.scroll_y
        };
        crate::app::mouse::apply_scrollbar_drag_target(scroll, target, drag_offset);
    }

    pub(crate) fn update_database_query_scroll_drag(
        &mut self,
        mouse_x: f32,
        mouse_y: f32,
    ) -> bool {
        if self.update_database_query_result_resize(mouse_y) {
            return true;
        }
        if let Some((_, state)) = self.active_database_query_meta_state_mut()
            && state.database_query_update_column_resize(mouse_x)
        {
            return true;
        }
        let scale = self.renderer.as_ref().map_or(1.0, |renderer| renderer.scale_factor);
        let review_scroll_rect = self
            .ui_registry
            .rect_for(crate::ui_system::UiId::DatabaseQueryReviewMessagesScrollY);
        if let Some((_, state)) = self.active_database_query_meta_state()
            && state.result_view.review_message_scroll_y.is_dragging
        {
            let Some((_, track_y, _, track_h)) = review_scroll_rect else {
                return false;
            };
            let max_scroll = state.result_view.review_message_max_scroll.get();
            let Some(thumb) = crate::scroll::scrollbar_thumb(
                track_y,
                track_h,
                track_h,
                track_h + max_scroll,
                state.result_view.review_message_scroll_y.current,
                (28.0 * scale).round(),
            ) else {
                return false;
            };
            let Some((drag_offset, target)) = crate::scroll::scrollbar_drag_target(
                mouse_y,
                track_y,
                track_h,
                thumb,
                max_scroll,
                Some(state.result_view.review_message_scroll_y.drag_offset),
            ) else {
                return false;
            };
            let Some((_, state)) = self.active_database_query_meta_state_mut() else {
                return false;
            };
            let scroll = &mut state.result_view.review_message_scroll_y;
            crate::app::mouse::apply_scrollbar_drag_target(scroll, target, drag_offset);
            return true;
        }
        let body_rect = self
            .ui_registry
            .rect_for(crate::ui_system::UiId::DatabaseQueryResultBody);
        let vertical_rect = self
            .ui_registry
            .rect_for(crate::ui_system::UiId::DatabaseQueryScrollY);
        let horizontal_rect = self
            .ui_registry
            .rect_for(crate::ui_system::UiId::DatabaseQueryScrollX);
        let history = &self.ide_panel.database.persisted.query_history;
        let Some((meta, state)) = self.active_database_query_meta_state() else {
            return false;
        };
        let (viewport_w, viewport_h) = database_query_scroll_viewport(
            body_rect,
            horizontal_rect,
            vertical_rect,
            scale,
        );
        let (max_x, max_y) = crate::app::database::database_query_scroll_limits(
            meta,
            state,
            history,
            viewport_w,
            viewport_h,
            scale,
        );
        let dragging_y = state.result_view.scroll_y.is_dragging;
        let dragging_x = state.result_view.scroll_x.is_dragging;
        let current_y = state.result_view.scroll_y.current;
        let current_x = state.result_view.scroll_x.current;
        let offset_y = state.result_view.scroll_y.drag_offset;
        let offset_x = state.result_view.scroll_x.drag_offset;
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
        let Some((_, state)) = self.active_database_query_meta_state_mut() else {
            return false;
        };
        let scroll = if horizontal {
            &mut state.result_view.scroll_x
        } else {
            &mut state.result_view.scroll_y
        };
        let drag_offset = scroll.drag_offset;
        crate::app::mouse::apply_scrollbar_drag_target(scroll, target, drag_offset);
        true
    }

    pub(crate) fn finish_database_query_scroll_drag(&mut self) {
        if let Some((_, state)) = self.active_database_query_meta_state_mut() {
            state.database_query_finish_scroll_drag();
        }
    }
}

fn database_query_completion_session_key(
    connection_id: crate::app::database::DatabaseConnectionId,
    console_id: SqlConsoleId,
    context: &crate::languages::sql_analysis::SqlCompletionContext,
) -> String {
    format!(
        "{}:{}:sql:{:?}:{}:{}:{}",
        connection_id.0,
        console_id.0,
        context.kind,
        context.scope.start,
        context.replace_range.start,
        context.qualifier.as_deref().unwrap_or("")
    )
}

fn normalize_database_query_text_offset(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn database_query_diagnostic_navigation_offset(
    text: &str,
    diagnostic: &crate::lsp::Diagnostic,
) -> usize {
    let start = crate::lsp::lsp_pos_to_offset(text, diagnostic.start_line, diagnostic.start_col);
    let end = crate::lsp::lsp_pos_to_offset(text, diagnostic.end_line, diagnostic.end_col);
    normalize_database_query_text_offset(text, if start == end { start } else { end })
}

fn database_query_scroll_viewport(
    body_rect: Option<(f32, f32, f32, f32)>,
    _horizontal_rect: Option<(f32, f32, f32, f32)>,
    _vertical_rect: Option<(f32, f32, f32, f32)>,
    _scale: f32,
) -> (f32, f32) {
    body_rect.map_or((1.0, 1.0), |rect| (rect.2.max(1.0), rect.3.max(1.0)))
}

fn database_query_transaction_finish_allowed(finishing: bool) -> bool {
    !finishing
}

fn database_query_diagnostic_is_stale(
    diagnostic_version: Option<u64>,
    editor_version: u64,
) -> bool {
    diagnostic_version.is_some_and(|version| version != editor_version)
}

#[cfg(test)]
mod database_query_app_method_tests {
    use super::*;

    fn editor_diagnostics(text: &str) -> Vec<crate::lsp::Diagnostic> {
        let mut editor = crate::editor::Editor::new(text.len().saturating_add(1));
        let _ = editor.insert_str(text);
        let analysis = crate::languages::sql_analysis::analyze_sql(text);
        database_query_editor_diagnostics(
            &analysis,
            None,
            text,
            &editor.line_offsets,
        )
    }

    fn diagnostic(start_col: u32, end_col: u32) -> crate::lsp::Diagnostic {
        crate::lsp::Diagnostic {
            start_line: 0,
            start_col,
            end_line: 0,
            end_col,
            severity: crate::lsp::DiagSeverity::Error,
            code: None,
            code_href: None,
            message: std::sync::Arc::from("error"),
            source: None,
            quickfixes: Box::new([]),
            tags: Box::new([]),
        }
    }

    #[test]
    fn query_problem_navigation_uses_range_end_and_preserves_zero_range() {
        let text = "0123456789abcdefghij";
        assert_eq!(database_query_diagnostic_navigation_offset(text, &diagnostic(10, 20)), 20);
        assert_eq!(database_query_diagnostic_navigation_offset(text, &diagnostic(10, 10)), 10);
    }

    #[test]
    fn query_problem_navigation_never_returns_inside_utf8_codepoint() {
        let text = "aЖb";
        let inside = 2;
        let normalized = normalize_database_query_text_offset(text, inside);
        assert_eq!(normalized, 1);
        assert!(text.is_char_boundary(normalized));
    }

    #[test]
    fn query_problem_navigation_uses_the_selected_sql_warning_end() {
        let text = "SELECT *\nFROM \"public\".\"car__model\"\nLIMIT 100;";
        let diagnostics = editor_diagnostics(text);
        let public_start = text.find("public").expect("public");
        let public_range = public_start..public_start + "public".len();

        for (code, expected_token) in [("SQL119", "*"), ("SQL117", "LIMIT 100")] {
            let index = diagnostics
                .iter()
                .position(|diagnostic| diagnostic.code.as_deref() == Some(code))
                .expect("warning index");
            let diagnostic = &diagnostics[index];
            let start = crate::lsp::lsp_pos_to_offset(
                text,
                diagnostic.start_line,
                diagnostic.start_col,
            );
            let offset = database_query_diagnostic_navigation_offset(text, diagnostic);

            assert_eq!(text.get(start..offset), Some(expected_token));
            assert!(!public_range.contains(&offset));
        }
    }

    #[test]
    fn query_problem_navigation_places_cursor_after_sql004_comma() {
        let text = "SELECT Ж, FROM car__body_type";
        let diagnostics = editor_diagnostics(text);
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code.as_deref() == Some("SQL004"))
            .expect("SQL004");
        let comma = text.find(',').expect("comma");

        assert_eq!(
            database_query_diagnostic_navigation_offset(text, diagnostic),
            comma + 1
        );
    }

    #[test]
    fn query_problem_navigation_preserves_sorted_row_identity() {
        let text = "SELECT *\nFROM \"public\".\"car__model\"\nLIMIT 100;";
        let diagnostics = editor_diagnostics(text);
        let flat_diags = (0..diagnostics.len()).collect::<Vec<_>>();

        for code in ["SQL119", "SQL117"] {
            let row = flat_diags
                .iter()
                .position(|index| diagnostics[*index].code.as_deref() == Some(code))
                .expect("problem row");
            let diagnostic_index = flat_diags[row];
            let diagnostic = &diagnostics[diagnostic_index];
            let expected_end = crate::lsp::lsp_pos_to_offset(
                text,
                diagnostic.end_line,
                diagnostic.end_col,
            );

            assert_eq!(
                database_query_diagnostic_navigation_offset(text, diagnostic),
                expected_end
            );
            assert_eq!(diagnostic.code.as_deref(), Some(code));
        }
    }

    #[test]
    fn query_problem_navigation_handles_unicode_before_sql119() {
        let text = "SELECT 'Ж', *\nFROM \"public\".\"car__model\"\nLIMIT 100;";
        let diagnostics = editor_diagnostics(text);
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code.as_deref() == Some("SQL119"))
            .expect("SQL119");
        let offset = database_query_diagnostic_navigation_offset(text, diagnostic);

        assert_eq!(text.get(offset.saturating_sub(1)..offset), Some("*"));
        assert!(text.is_char_boundary(offset));
        let public_start = text.find("public").expect("public");
        assert!(offset < public_start);
    }

    #[test]
    fn query_result_and_review_scrollbar_drag_are_target_only() {
        for (track_start, track_len, viewport, max_scroll, min_thumb) in [
            (10.0, 220.0, 220.0, 540.0, 28.0),
            (20.0, 320.0, 320.0, 900.0, 36.0),
        ] {
            let current = max_scroll * 0.4;
            let thumb = crate::scroll::scrollbar_thumb(
                track_start,
                track_len,
                viewport,
                viewport + max_scroll,
                current,
                min_thumb,
            )
            .expect("query thumb");
            let pointer = thumb.start + 5.0;
            let (offset, _) = crate::scroll::scrollbar_drag_target(
                pointer,
                track_start,
                track_len,
                thumb,
                max_scroll,
                None,
            )
            .expect("query drag starts");
            let (_, target) = crate::scroll::scrollbar_drag_target(
                pointer + 30.0,
                track_start,
                track_len,
                thumb,
                max_scroll,
                Some(offset),
            )
            .expect("query drag moves");
            let mut scroll = crate::scroll::ScrollState::new(7.0);
            scroll.jump_to(current);
            assert!(crate::app::mouse::apply_scrollbar_drag_target(
                &mut scroll,
                target,
                offset
            ));
            assert_eq!(scroll.current, current);
            assert_eq!(scroll.target, target);
            scroll.end_drag();
            assert!(!scroll.is_dragging);
            assert_eq!(scroll.current, current);
            assert_eq!(scroll.target, target);
        }
    }

    #[test]
    fn query_scroll_viewport_excludes_tabs_and_visible_scrollbars() {
        let viewport = database_query_scroll_viewport(
            Some((0.0, 0.0, 500.0, 300.0)),
            Some((0.0, 254.0, 490.0, 10.0)),
            Some((490.0, 36.0, 10.0, 218.0)),
            1.0,
        );
        assert_eq!(viewport, (500.0, 300.0));
    }

    #[test]
    fn query_scroll_viewport_does_not_reserve_scrollbars_twice() {
        assert_eq!(
            database_query_scroll_viewport(
                Some((0.0, 0.0, 490.0, 300.0)),
                None,
                Some((490.0, 0.0, 10.0, 300.0)),
                1.0,
            ),
            (490.0, 300.0),
        );
        assert_eq!(
            database_query_scroll_viewport(
                Some((0.0, 0.0, 500.0, 290.0)),
                Some((0.0, 290.0, 500.0, 10.0)),
                None,
                1.0,
            ),
            (500.0, 290.0),
        );
    }

    #[test]
    fn server_diagnostic_becomes_stale_after_document_edit() {
        assert!(!database_query_diagnostic_is_stale(None, 2));
        assert!(!database_query_diagnostic_is_stale(Some(2), 2));
        assert!(database_query_diagnostic_is_stale(Some(2), 3));
    }
    #[test]
    fn bug_58_query_transaction_finish_rejects_duplicate_commit_or_rollback() {
        assert!(database_query_transaction_finish_allowed(false));
        assert!(!database_query_transaction_finish_allowed(true));
    }

}
