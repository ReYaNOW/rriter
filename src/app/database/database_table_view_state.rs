use super::{
    DatabaseCellValue, DatabaseGeneration, DatabaseGridCell, DatabaseGridRow, DatabaseRowState,
    DatabasePanelState, DatabaseTableInputTarget, DatabaseTableMetadata, DatabaseTableReloadAction,
    DatabaseTableTabMeta, DatabaseTableTabState, DatabaseTableViewKey, DatabaseTableViewState,
};

pub(crate) struct DatabaseTableChunkTransition {
    pub committed_view: bool,
    pub next_chunk: Option<usize>,
}

#[derive(Clone, Copy)]
pub(crate) enum DatabaseTablePageStep {
    Previous,
    Next,
    Last,
}

impl DatabaseTableTabState {
    pub(crate) fn apply_filter_autocomplete(
        &mut self,
        target: DatabaseTableInputTarget,
        selected: &str,
    ) {
        let input = match target {
            DatabaseTableInputTarget::Where => &mut self.grid.where_input,
            DatabaseTableInputTarget::OrderBy => &mut self.grid.order_by_input,
            DatabaseTableInputTarget::Cell => return,
        };
        let context = database_table_filter_completion_context(target, input.text(), input.cursor);
        input.replace_range(
            context.replace_range.start,
            context.replace_range.end,
            selected,
            64 * 1024,
        );
        self.grid.filter_error = None;
    }

    pub(crate) fn fail_view_reload(&mut self, message: String) {
        self.grid.loading_count = false;
        self.grid.loading_chunk = false;
        self.grid.in_flight_chunk = None;
        self.grid.desired_chunk = None;
        self.grid.finish_refresh();
        self.grid.abort_pending_view();
        self.error = Some(message);
    }

    pub(crate) fn prepare_initial_view_load(
        &mut self,
        meta: &DatabaseTableTabMeta,
    ) -> Option<(DatabaseTableTabMeta, DatabaseGeneration)> {
        self.metadata.as_ref()?;
        self.generation = self.generation.next();
        if !self.grid.chunks.is_empty() {
            self.grid.loading_chunk = false;
            self.grid.in_flight_chunk = None;
            self.grid.desired_chunk = None;
            self.grid.start_refresh(std::time::Instant::now());
        } else {
            self.grid.clear_loaded_rows();
            self.grid.count = None;
        }
        self.grid.count_error = None;
        self.grid.pending_count = None;
        self.grid.loading_count = true;
        Some((meta.clone(), self.generation))
    }

    pub(crate) fn commit_view_count(&mut self, count: u64) {
        self.grid.pending_count = Some(count);
        self.grid.loading_count = false;
        self.grid.count_error = None;
        let request_view = self
            .grid
            .pending_view
            .as_mut()
            .unwrap_or(&mut self.grid.view);
        let last_page = if count == 0 {
            0
        } else {
            (count as usize - 1) / request_view.limit
        };
        request_view.current_page = request_view.current_page.min(last_page);
    }

    pub(crate) fn commit_view_chunk(
        &mut self,
        chunk: super::DatabaseTableChunk,
    ) -> DatabaseTableChunkTransition {
        if self.grid.refreshing {
            self.grid.clear_loaded_rows();
            self.grid.finish_refresh();
        }
        let committed_view = self.grid.commit_pending_view();
        self.grid.insert_chunk(chunk);
        if let Some(metadata) = self.metadata.clone() {
            self.grid.restore_pending_selection(&metadata);
        }
        self.grid.post_commit_refresh_pending = false;
        self.error = None;
        self.grid.filter_error = None;
        self.clear_notice();
        DatabaseTableChunkTransition {
            committed_view,
            next_chunk: self.grid.desired_chunk.take(),
        }
    }

    pub(crate) fn request_view_reload(
        &mut self,
        action: DatabaseTableReloadAction,
    ) -> Option<DatabaseTableReloadAction> {
        // None: local edits are pending, the action waits for the refresh prompt.
        if self.grid.dirty() {
            self.grid.pending_reload = Some(action);
            None
        } else {
            Some(action)
        }
    }

    pub(crate) fn apply_view_reload(
        &mut self,
        action: DatabaseTableReloadAction,
    ) {
        match action {
            DatabaseTableReloadAction::Refresh => {}
            DatabaseTableReloadAction::ApplyView(view) => {
                let vertical_context_changed = self.grid.view.current_page != view.current_page
                    || self.grid.view.limit != view.limit
                    || self.grid.view.where_clause != view.where_clause
                    || self.grid.view.order_by != view.order_by;
                self.grid.where_input.set_text(view.where_clause.clone());
                self.grid.order_by_input.set_text(view.order_by.clone());
                self.grid.begin_pending_view(view, false, false);
                if vertical_context_changed {
                    self.grid.scroll_y.reset();
                }
            }
            DatabaseTableReloadAction::ApplyFilterView(view) => {
                let vertical_context_changed = self.grid.view.current_page != view.current_page
                    || self.grid.view.limit != view.limit
                    || self.grid.view.where_clause != view.where_clause
                    || self.grid.view.order_by != view.order_by;
                let where_changed = self.grid.view.where_clause != view.where_clause;
                let order_by_changed = self.grid.view.order_by != view.order_by;
                self.grid
                    .begin_pending_view(view, where_changed, order_by_changed);
                if vertical_context_changed {
                    self.grid.scroll_y.reset();
                }
            }
        }
        self.grid.pending_reload = None;
        self.error = None;
        self.clear_notice();
    }

    pub(crate) fn discard_local_changes(&mut self) {
        self.grid.added_rows.clear();
        for chunk in self.grid.chunks.values_mut() {
            for row in &mut chunk.rows {
                row.state = DatabaseRowState::Clean;
                for cell in &mut row.cells {
                    cell.undo();
                }
            }
        }
        self.grid.cell_editor = None;
        self.grid.focused_input = None;
    }

    pub(crate) fn validate_and_prepare_filter_view(
        &mut self,
    ) -> Result<DatabaseTableReloadAction, (DatabaseTableInputTarget, String)> {
        if let Err(error) = super::validate_table_fragment(self.grid.where_input.text(), "WHERE") {
            return Err((DatabaseTableInputTarget::Where, error));
        }
        if let Err(error) =
            super::validate_table_fragment(self.grid.order_by_input.text(), "ORDER BY")
        {
            return Err((DatabaseTableInputTarget::OrderBy, error));
        }
        let mut view = self.grid.view.clone();
        view.where_clause = self.grid.where_input.text().to_string();
        view.order_by = self.grid.order_by_input.text().to_string();
        view.sorted_column = None;
        view.sort_direction = None;
        view.current_page = 0;
        self.grid.filter_error = None;
        Ok(DatabaseTableReloadAction::ApplyFilterView(view))
    }

    pub(crate) fn table_page_target(&self, step: DatabaseTablePageStep) -> Option<usize> {
        match step {
            DatabaseTablePageStep::Previous => Some(self.grid.view.current_page.saturating_sub(1)),
            DatabaseTablePageStep::Next => {
                if !self.grid.can_page_next() {
                    return Some(self.grid.view.current_page);
                }
                Some(match self.grid.count {
                    Some(count) => {
                        let last = (count as usize).saturating_sub(1) / self.grid.view.limit;
                        self.grid.view.current_page.saturating_add(1).min(last)
                    }
                    None => self.grid.view.current_page.saturating_add(1),
                })
            }
            DatabaseTablePageStep::Last => self.grid.count.map(|count| {
                (count as usize).saturating_sub(1) / self.grid.view.limit
            }),
        }
    }

    pub(crate) fn set_table_page(&self, page: usize) -> Option<DatabaseTableReloadAction> {
        if self.grid.view.current_page == page && !self.grid.chunks.is_empty() {
            return None;
        }
        let mut view = self.grid.view.clone();
        view.current_page = page;
        Some(DatabaseTableReloadAction::ApplyView(view))
    }

    pub(crate) fn set_table_limit(&self, limit: usize) -> DatabaseTableReloadAction {
        let mut view = self.grid.view.clone();
        view.limit = limit;
        view.current_page = 0;
        DatabaseTableReloadAction::ApplyView(view)
    }

    pub(crate) fn sorted_table_view(
        &self,
        column_index: usize,
    ) -> Option<DatabaseTableReloadAction> {
        let column = self.metadata.as_ref()?.columns.get(column_index)?;
        let mut view = self.grid.view.clone();
        match (view.sorted_column.as_deref(), view.sort_direction) {
            (Some(name), Some(super::DatabaseSortDirection::Asc)) if name == column.name => {
                view.sort_direction = Some(super::DatabaseSortDirection::Desc);
                view.order_by = format!(
                    "{} DESC",
                    super::quote_pg_identifier(&column.name)
                );
            }
            (Some(name), Some(super::DatabaseSortDirection::Desc)) if name == column.name => {
                view.sorted_column = None;
                view.sort_direction = None;
                view.order_by.clear();
            }
            _ => {
                view.sorted_column = Some(column.name.clone());
                view.sort_direction = Some(super::DatabaseSortDirection::Asc);
                view.order_by = format!(
                    "{} ASC",
                    super::quote_pg_identifier(&column.name)
                );
            }
        }
        view.current_page = 0;
        Some(DatabaseTableReloadAction::ApplyView(view))
    }

    pub(crate) fn add_local_row(&mut self) {
        let Some(metadata) = self.metadata.as_ref() else {
            return;
        };
        if !metadata.editable {
            self.error = metadata.read_only_reason.clone();
            return;
        }
        let absolute_index = self.grid.next_added_row_index();
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
        self.grid.added_rows.push(DatabaseGridRow {
            absolute_index,
            cells,
            xmin: None,
            state: DatabaseRowState::Added,
        });
        self.grid.select_row(absolute_index, false, false);
    }

    pub(crate) fn delete_local_selection(&mut self) {
        if !self.metadata.as_ref().is_some_and(|metadata| metadata.editable) {
            self.error = self
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.read_only_reason.clone());
            return;
        }
        let mut rows = self.grid.selection.selected_rows.clone();
        if rows.is_empty()
            && let Some((start, end)) = self.grid.selection.cell_range()
        {
            rows.extend(self.grid.row_indices_between(start.row, end.row));
        }
        rows.sort_unstable();
        rows.dedup();
        let removed_added: std::collections::HashSet<_> = self
            .grid
            .added_rows
            .iter()
            .filter_map(|row| rows.contains(&row.absolute_index).then_some(row.absolute_index))
            .collect();
        self.grid
            .added_rows
            .retain(|row| !removed_added.contains(&row.absolute_index));
        for row_index in rows.iter().copied() {
            if removed_added.contains(&row_index) {
                continue;
            }
            if let Some(row) = self.grid.row_mut(row_index) {
                row.state = if row.state == DatabaseRowState::Deleted {
                    DatabaseRowState::Clean
                } else {
                    DatabaseRowState::Deleted
                };
            }
        }
        self.grid.selection.clear();
    }

    pub(crate) fn undo_local_selection(&mut self) {
        if !self.grid.selection.selected_rows.is_empty() {
            let rows = self.grid.selection.selected_rows.clone();
            let added: std::collections::HashSet<_> = self
                .grid
                .added_rows
                .iter()
                .filter_map(|row| rows.contains(&row.absolute_index).then_some(row.absolute_index))
                .collect();
            self.grid
                .added_rows
                .retain(|row| !added.contains(&row.absolute_index));
            for row_index in rows {
                if added.contains(&row_index) {
                    continue;
                }
                if let Some(row) = self.grid.row_mut(row_index) {
                    row.state = DatabaseRowState::Clean;
                    for cell in &mut row.cells {
                        cell.undo();
                    }
                }
            }
            self.grid.selection.clear();
            return;
        }
        if let Some((start, end)) = self.grid.selection.cell_range() {
            for row_index in self.grid.row_indices_between(start.row, end.row) {
                if let Some(row) = self.grid.row_mut(row_index) {
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

impl DatabasePanelState {
    pub(crate) fn table_view_snapshot(
        &mut self,
        key: DatabaseTableViewKey,
        default_limit: usize,
    ) -> DatabaseTableViewState {
        if let Some(view) = self.persisted.table_views.iter().find(|view| view.key == key) {
            return view.clone();
        }
        let view = DatabaseTableViewState {
            key,
            limit: default_limit,
            ..DatabaseTableViewState::default()
        };
        self.persisted.table_views.push(view.clone());
        view
    }

    pub(crate) fn upsert_table_view(&mut self, view: DatabaseTableViewState) {
        if let Some(existing) = self
            .persisted
            .table_views
            .iter_mut()
            .find(|existing| existing.key == view.key)
        {
            *existing = view;
        } else {
            self.persisted.table_views.push(view);
        }
    }

    pub(crate) fn open_custom_table_limit(
        &mut self,
        tab_id: super::DatabaseTabId,
        limit: usize,
    ) {
        self.table_modal = Some(super::DatabaseTableModal::CustomLimit {
            tab_id,
            input: super::DatabaseDialogInput::new(limit.to_string()),
            error: None,
        });
    }

    pub(crate) fn take_custom_table_limit(
        &mut self,
    ) -> Option<Result<(super::DatabaseTabId, usize), String>> {
        let Some(super::DatabaseTableModal::CustomLimit { tab_id, input, .. }) =
            self.table_modal.as_ref()
        else {
            return None;
        };
        let tab_id = *tab_id;
        let parsed = input.text().trim().parse::<usize>();
        let limit = match parsed {
            Ok(value) if (1..=super::MAX_CUSTOM_TABLE_LIMIT).contains(&value) => value,
            _ => {
                if let Some(super::DatabaseTableModal::CustomLimit { error, .. }) =
                    self.table_modal.as_mut()
                {
                    *error = Some("Лимит должен быть от 1 до 10000".to_string());
                }
                return Some(Err("Лимит должен быть от 1 до 10000".to_string()));
            }
        };
        self.table_modal = None;
        Some(Ok((tab_id, limit)))
    }
}

pub(crate) fn database_table_filter_completion_context(
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

pub(crate) fn database_table_filter_completion_words(
    metadata: &DatabaseTableMetadata,
    target: DatabaseTableInputTarget,
    context: &crate::languages::sql_analysis::SqlCompletionContext,
) -> Vec<(crate::app::AutocompleteItem, Vec<usize>)> {
    use crate::languages::sql_analysis::SqlCompletionKind;
    let mut words: Vec<(String, String, crate::highlighter::SymbolKind)> = Vec::new();
    match context.kind {
        SqlCompletionKind::Column => words.extend(metadata.columns.iter().map(|column| {
            (
                column.name.clone(),
                super::quote_pg_identifier(&column.name),
                crate::highlighter::SymbolKind::Property,
            )
        })),
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
                if column.type_kind == super::DatabaseTypeKind::Enum {
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
    words.into_iter().filter_map(|(word, insert_text, kind)| {
        let lower = word.trim_matches('"').trim_matches('\'').to_ascii_lowercase();
        if !prefix.is_empty() && !lower.contains(&prefix) {
            return None;
        }
        let indices = if prefix.is_empty() {
            Vec::new()
        } else {
            lower.match_indices(&prefix).next()
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
    }).take(64).collect()
}

#[cfg(test)]
mod database_table_view_state_tests {
    use super::*;

    fn metadata() -> DatabaseTableMetadata {
        DatabaseTableMetadata {
            database_name: "db".to_string(),
            table_name: "items".to_string(),
            columns: vec![
                super::super::DatabaseColumnInfo {
                    ordinal: 1,
                    name: "id".to_string(),
                    type_name: "integer".to_string(),
                    type_oid: 23,
                    type_kind: super::super::DatabaseTypeKind::Other,
                    nullable: false,
                    default_expression: None,
                    identity: false,
                    generated: false,
                    primary_key: true,
                    enum_values: Vec::new(),
                },
                super::super::DatabaseColumnInfo {
                    ordinal: 2,
                    name: "User ID".to_string(),
                    type_name: "text".to_string(),
                    type_oid: 25,
                    type_kind: super::super::DatabaseTypeKind::Other,
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
        let mut state = DatabaseTableTabState::default();
        state.grid.count = Some(201);
        assert_eq!(state.table_page_target(DatabaseTablePageStep::Last), Some(2));
        state.grid.count = Some(0);
        assert_eq!(state.table_page_target(DatabaseTablePageStep::Last), Some(0));
    }

    #[test]
    fn filter_completion_reuses_columns_and_quotes_complex_identifiers() {
        let context = database_table_filter_completion_context(
            DatabaseTableInputTarget::Where,
            "Us",
            2,
        );
        assert_eq!(
            context.kind,
            crate::languages::sql_analysis::SqlCompletionKind::Column
        );
        let options = database_table_filter_completion_words(
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
        let options = database_table_filter_completion_words(
            &metadata(),
            DatabaseTableInputTarget::OrderBy,
            &context,
        );
        assert!(options.iter().any(|(item, _)| item.word == "DESC"));
    }

    #[test]
    fn where_completion_after_operator_waits_for_a_value() {
        let context = database_table_filter_completion_context(
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
        let context = database_table_filter_completion_context(
            DatabaseTableInputTarget::Where,
            "id = Us",
            7,
        );
        assert_eq!(context.replace_range, 5..7);
        assert_eq!(context.prefix, "Us");
    }
}
