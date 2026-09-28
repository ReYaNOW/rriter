#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DatabaseCellPosition {
    pub row: usize,
    pub column: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DatabaseGridSelection {
    pub anchor: Option<DatabaseCellPosition>,
    pub cursor: Option<DatabaseCellPosition>,
    pub selected_rows: Vec<usize>,
}

impl DatabaseGridSelection {
    pub fn clear(&mut self) {
        self.anchor = None;
        self.cursor = None;
        self.selected_rows.clear();
    }

    pub fn select_cell(&mut self, position: DatabaseCellPosition, extend: bool) {
        if !extend || self.anchor.is_none() {
            self.anchor = Some(position);
        }
        self.cursor = Some(position);
        self.selected_rows.clear();
    }

    pub fn select_row(&mut self, row: usize, extend: bool, toggle: bool) {
        self.anchor = None;
        self.cursor = None;
        if toggle {
            if let Some(index) = self
                .selected_rows
                .iter()
                .position(|selected| *selected == row)
            {
                self.selected_rows.remove(index);
            } else {
                self.selected_rows.push(row);
                self.selected_rows.sort_unstable();
            }
            return;
        }
        if extend {
            let start = self.selected_rows.first().copied().unwrap_or(row).min(row);
            let end = self.selected_rows.last().copied().unwrap_or(row).max(row);
            self.selected_rows = (start..=end).collect();
        } else {
            self.selected_rows.clear();
            self.selected_rows.push(row);
        }
    }

    pub fn select_row_from_ordered(
        &mut self,
        row: usize,
        extend: bool,
        toggle: bool,
        ordered_rows: &[usize],
    ) {
        if toggle || !extend {
            self.select_row(row, extend, toggle);
            return;
        }
        let anchor = self.selected_rows.first().copied().unwrap_or(row);
        let Some(anchor_index) = ordered_rows
            .iter()
            .position(|candidate| *candidate == anchor)
        else {
            self.select_row(row, false, false);
            return;
        };
        let Some(row_index) = ordered_rows.iter().position(|candidate| *candidate == row) else {
            self.select_row(row, false, false);
            return;
        };
        let (start, end) = if anchor_index <= row_index {
            (anchor_index, row_index)
        } else {
            (row_index, anchor_index)
        };
        self.anchor = None;
        self.cursor = None;
        self.selected_rows = ordered_rows[start..=end].to_vec();
        self.selected_rows.sort_unstable();
    }

    pub fn cell_range(&self) -> Option<(DatabaseCellPosition, DatabaseCellPosition)> {
        let a = self.anchor?;
        let b = self.cursor?;
        Some((
            DatabaseCellPosition {
                row: a.row.min(b.row),
                column: a.column.min(b.column),
            },
            DatabaseCellPosition {
                row: a.row.max(b.row),
                column: a.column.max(b.column),
            },
        ))
    }

    pub fn contains_cell(&self, row: usize, column: usize) -> bool {
        self.cell_range().is_some_and(|(start, end)| {
            row >= start.row && row <= end.row && column >= start.column && column <= end.column
        })
    }

    pub fn contains_row(&self, row: usize) -> bool {
        self.selected_rows.binary_search(&row).is_ok()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatabaseTableInputTarget {
    Where,
    OrderBy,
    Cell,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DatabaseCellEditorKind {
    Inline,
    Multiline,
    Boolean,
    Enum,
    DateTime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseCellEditorState {
    pub position: DatabaseCellPosition,
    pub kind: DatabaseCellEditorKind,
    pub input: super::DatabaseDialogInput,
    pub enum_index: usize,
    pub calendar_year: i32,
    pub calendar_month: u32,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseTableRefreshPrompt {
    pub close_after_save: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseTableReviewSummary {
    pub inserted_rows: usize,
    pub updated_rows: usize,
    pub changed_cells: usize,
    pub deleted_rows: usize,
    pub detail_rows: Vec<String>,
    pub notices: Vec<String>,
    pub truncated_details: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseTableReviewState {
    pub transaction_id: super::DatabaseTransactionId,
    pub summary: DatabaseTableReviewSummary,
    pub deadline_unix_ms: u128,
    pub committing: bool,
    pub close_after_commit: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DatabaseTableReloadAction {
    Refresh,
    ApplyView(DatabaseTableViewState),
    ApplyFilterView(DatabaseTableViewState),
}

#[derive(Clone, Debug)]
pub struct DatabaseTableGridState {
    pub view: DatabaseTableViewState,
    pub pending_view: Option<DatabaseTableViewState>,
    pub pending_count: Option<u64>,
    pub pending_where_changed: bool,
    pub pending_order_by_changed: bool,
    pub count: Option<u64>,
    pub count_error: Option<String>,
    pub loading_count: bool,
    pub loading_chunk: bool,
    pub in_flight_chunk: Option<usize>,
    pub desired_chunk: Option<usize>,
    pub chunks: BTreeMap<usize, DatabaseTableChunk>,
    lru: VecDeque<usize>,
    pub cache_bytes: usize,
    pub added_rows: Vec<DatabaseGridRow>,
    pub scroll_x: ScrollState,
    pub scroll_y: ScrollState,
    pub selection: DatabaseGridSelection,
    pub focused_input: Option<DatabaseTableInputTarget>,
    pub text_drag: Option<DatabaseTableInputTarget>,
    pub where_input: super::DatabaseDialogInput,
    pub order_by_input: super::DatabaseDialogInput,
    pub filter_error: Option<(DatabaseTableInputTarget, String)>,
    pub cell_editor: Option<DatabaseCellEditorState>,
    pub refresh_prompt: Option<DatabaseTableRefreshPrompt>,
    pub review: Option<DatabaseTableReviewState>,
    pub sql_preview: Option<String>,
    pub pending_close_after_save: bool,
    pub pending_reload: Option<DatabaseTableReloadAction>,
    pub post_commit_refresh_pending: bool,
    pub refresh_started: Option<std::time::Instant>,
    pub refreshing: bool,
    pub restore_selection_keys: Vec<Vec<String>>,
    pub restore_selection_column: Option<usize>,
    pub column_resize: Option<(usize, f32, f32)>,
    pub viewport_width: f32,
    pub viewport_height: f32,
}

impl PartialEq for DatabaseTableGridState {
    fn eq(&self, other: &Self) -> bool {
        self.view == other.view
            && self.count == other.count
            && self.pending_view == other.pending_view
            && self.pending_count == other.pending_count
            && self.pending_where_changed == other.pending_where_changed
            && self.pending_order_by_changed == other.pending_order_by_changed
            && self.count_error == other.count_error
            && self.loading_count == other.loading_count
            && self.loading_chunk == other.loading_chunk
            && self.in_flight_chunk == other.in_flight_chunk
            && self.desired_chunk == other.desired_chunk
            && self.chunks == other.chunks
            && self.cache_bytes == other.cache_bytes
            && self.added_rows == other.added_rows
            && self.selection == other.selection
            && self.focused_input == other.focused_input
            && self.text_drag == other.text_drag
            && self.where_input.text() == other.where_input.text()
            && self.order_by_input.text() == other.order_by_input.text()
            && self.filter_error == other.filter_error
            && self.cell_editor == other.cell_editor
            && self.refresh_prompt == other.refresh_prompt
            && self.review == other.review
            && self.sql_preview == other.sql_preview
            && self.pending_close_after_save == other.pending_close_after_save
            && self.pending_reload == other.pending_reload
            && self.post_commit_refresh_pending == other.post_commit_refresh_pending
            && self.refreshing == other.refreshing
            && self.restore_selection_keys == other.restore_selection_keys
            && self.restore_selection_column == other.restore_selection_column
    }
}

impl Eq for DatabaseTableGridState {}

impl DatabaseTableGridState {
    pub fn new(view: DatabaseTableViewState) -> Self {
        Self {
            where_input: super::DatabaseDialogInput::new(view.where_clause.clone()),
            order_by_input: super::DatabaseDialogInput::new(view.order_by.clone()),
            filter_error: None,
            view,
            pending_view: None,
            pending_count: None,
            pending_where_changed: false,
            pending_order_by_changed: false,
            count: None,
            count_error: None,
            loading_count: false,
            loading_chunk: false,
            in_flight_chunk: None,
            desired_chunk: None,
            chunks: BTreeMap::new(),
            lru: VecDeque::new(),
            cache_bytes: 0,
            added_rows: Vec::new(),
            scroll_x: ScrollState::new(15.0),
            scroll_y: ScrollState::new(15.0),
            selection: DatabaseGridSelection::default(),
            focused_input: None,
            text_drag: None,
            cell_editor: None,
            refresh_prompt: None,
            review: None,
            sql_preview: None,
            pending_close_after_save: false,
            pending_reload: None,
            post_commit_refresh_pending: false,
            refresh_started: None,
            refreshing: false,
            restore_selection_keys: Vec::new(),
            restore_selection_column: None,
            column_resize: None,
            viewport_width: 0.0,
            viewport_height: 0.0,
        }
    }

    pub fn dirty(&self) -> bool {
        !self.added_rows.is_empty()
            || self
                .chunks
                .values()
                .flat_map(|chunk| &chunk.rows)
                .any(DatabaseGridRow::is_dirty)
    }

    pub fn request_view(&self) -> &DatabaseTableViewState {
        self.pending_view.as_ref().unwrap_or(&self.view)
    }

    pub fn begin_pending_view(
        &mut self,
        view: DatabaseTableViewState,
        where_changed: bool,
        order_by_changed: bool,
    ) {
        self.pending_view = Some(view);
        self.pending_count = None;
        self.pending_where_changed = where_changed;
        self.pending_order_by_changed = order_by_changed;
    }

    pub fn commit_pending_view(&mut self) -> bool {
        let Some(view) = self.pending_view.take() else {
            if let Some(count) = self.pending_count.take() {
                self.count = Some(count);
            }
            self.pending_where_changed = false;
            self.pending_order_by_changed = false;
            return false;
        };
        self.view = view;
        if let Some(count) = self.pending_count.take() {
            self.count = Some(count);
        }
        self.pending_where_changed = false;
        self.pending_order_by_changed = false;
        true
    }

    pub fn abort_pending_view(&mut self) {
        self.pending_view = None;
        self.pending_count = None;
        self.pending_where_changed = false;
        self.pending_order_by_changed = false;
    }

    pub fn pending_filter_error_target(
        &self,
        load_chunk: bool,
    ) -> Option<DatabaseTableInputTarget> {
        if load_chunk && self.pending_order_by_changed {
            Some(DatabaseTableInputTarget::OrderBy)
        } else if self.pending_where_changed {
            Some(DatabaseTableInputTarget::Where)
        } else {
            None
        }
    }

    pub fn insert_chunk(&mut self, chunk: DatabaseTableChunk) {
        let index = chunk.chunk_index;
        if let Some(previous) = self.chunks.remove(&index) {
            self.cache_bytes = self.cache_bytes.saturating_sub(previous.estimated_bytes);
        }
        self.cache_bytes = self.cache_bytes.saturating_add(chunk.estimated_bytes);
        self.chunks.insert(index, chunk);
        self.touch_chunk(index);
        self.evict_chunks();
        self.loading_chunk = false;
        self.in_flight_chunk = None;
    }

    pub fn touch_chunk(&mut self, index: usize) {
        self.lru.retain(|entry| *entry != index);
        self.lru.push_back(index);
    }

    fn evict_chunks(&mut self) {
        let selection = self.selection.clone();
        let visible_rows = self.visible_absolute_row_range();
        while self.chunks.len() > MAX_CACHED_CHUNKS_PER_TAB
            || self.cache_bytes > MAX_TABLE_CACHE_BYTES
        {
            let Some(candidate) = self.lru.pop_front() else {
                break;
            };
            let protected = self
                .chunks
                .get(&candidate)
                .is_some_and(|chunk| chunk_is_protected(chunk, &selection, &visible_rows));
            if protected {
                self.lru.push_back(candidate);
                if self.lru.iter().all(|index| {
                    self.chunks
                        .get(index)
                        .is_some_and(|chunk| chunk_is_protected(chunk, &selection, &visible_rows))
                }) {
                    break;
                }
                continue;
            }
            if let Some(removed) = self.chunks.remove(&candidate) {
                self.cache_bytes = self.cache_bytes.saturating_sub(removed.estimated_bytes);
            }
        }
    }

    pub fn row(&self, absolute_index: usize) -> Option<&DatabaseGridRow> {
        if let Some((chunk_index, row_offset)) = self.server_row_location(absolute_index)
            && let Some(chunk) = self.chunks.get(&chunk_index)
        {
            if let Some(row) = chunk
                .rows
                .get(row_offset)
                .filter(|row| row.absolute_index == absolute_index)
            {
                return Some(row);
            }
            if let Some(row) = chunk
                .rows
                .iter()
                .find(|row| row.absolute_index == absolute_index)
            {
                return Some(row);
            }
        }
        self.added_rows
            .iter()
            .find(|row| row.absolute_index == absolute_index)
    }

    pub fn row_mut(&mut self, absolute_index: usize) -> Option<&mut DatabaseGridRow> {
        if let Some((chunk_index, row_offset)) = self.server_row_location(absolute_index)
            && let Some(chunk) = self.chunks.get_mut(&chunk_index)
        {
            if chunk
                .rows
                .get(row_offset)
                .is_some_and(|row| row.absolute_index == absolute_index)
            {
                return chunk.rows.get_mut(row_offset);
            }
            if let Some(row_offset) = chunk
                .rows
                .iter()
                .position(|row| row.absolute_index == absolute_index)
            {
                return chunk.rows.get_mut(row_offset);
            }
        }
        self.added_rows
            .iter_mut()
            .find(|row| row.absolute_index == absolute_index)
    }

    fn server_row_location(&self, absolute_index: usize) -> Option<(usize, usize)> {
        let page_base = self.view.current_page.saturating_mul(self.view.limit);
        let relative = absolute_index.checked_sub(page_base)?;
        if relative >= self.view.limit {
            return None;
        }
        Some((
            relative / super::DATABASE_CHUNK_SIZE,
            relative % super::DATABASE_CHUNK_SIZE,
        ))
    }

    pub fn visible_row_range(&self) -> std::ops::Range<usize> {
        let first = (self.scroll_y.current.max(0.0) / DATABASE_GRID_ROW_HEIGHT)
            .floor()
            .max(0.0) as usize;
        let count = (self.viewport_height / DATABASE_GRID_ROW_HEIGHT).ceil() as usize + 2;
        first.saturating_sub(1)..first.saturating_add(count)
    }

    pub fn visible_absolute_row_range(&self) -> std::ops::Range<usize> {
        let page_base = self.view.current_page.saturating_mul(self.view.limit);
        let relative = self.visible_row_range();
        page_base.saturating_add(relative.start)..page_base.saturating_add(relative.end)
    }

    pub fn visible_column_range(&self, metadata: &DatabaseTableMetadata) -> std::ops::Range<usize> {
        let mut x = 0.0;
        let start_x = self.scroll_x.current.max(0.0);
        let end_x = start_x + self.viewport_width.max(0.0);
        let mut first = 0;
        let mut last = metadata.columns.len();
        let mut found_first = false;
        for (index, column) in metadata.columns.iter().enumerate() {
            let width = self.column_width(&column.name);
            if !found_first && x + width >= start_x {
                first = index.saturating_sub(1);
                found_first = true;
            }
            if found_first && x > end_x {
                last = (index + 1).min(metadata.columns.len());
                break;
            }
            x += width;
        }
        first..last
    }

    pub fn column_width(&self, name: &str) -> f32 {
        database_column_width(&self.view.column_widths, name)
    }

    pub fn set_column_width(&mut self, name: &str, width: f32) {
        set_database_column_width(&mut self.view.column_widths, name, width);
    }

    pub fn content_width(&self, metadata: &DatabaseTableMetadata) -> f32 {
        database_columns_content_width(
            &self.view.column_widths,
            metadata.columns.iter().map(|column| column.name.as_str()),
        )
    }

    pub fn loaded_server_row_count_on_page(&self) -> usize {
        let page_base = self.view.current_page.saturating_mul(self.view.limit);
        let page_end = page_base.saturating_add(self.view.limit);
        self.chunks
            .values()
            .flat_map(|chunk| chunk.rows.iter())
            .filter(|row| row.absolute_index >= page_base && row.absolute_index < page_end)
            .count()
    }

    pub fn loaded_server_row_bounds_on_page(&self) -> Option<(usize, usize)> {
        let page_base = self.view.current_page.saturating_mul(self.view.limit);
        let page_end = page_base.saturating_add(self.view.limit);
        self.chunks
            .values()
            .flat_map(|chunk| chunk.rows.iter())
            .filter_map(|row| {
                (row.absolute_index >= page_base && row.absolute_index < page_end)
                    .then_some(row.absolute_index)
            })
            .fold(None, |bounds, row| {
                Some(bounds.map_or((row, row), |(first, last): (usize, usize)| {
                    (first.min(row), last.max(row))
                }))
            })
    }

    pub fn loaded_server_row_extent_on_page(&self) -> usize {
        let page_base = self.view.current_page.saturating_mul(self.view.limit);
        let page_end = page_base.saturating_add(self.view.limit);
        self.chunks
            .values()
            .flat_map(|chunk| chunk.rows.iter())
            .filter_map(|row| {
                (row.absolute_index >= page_base && row.absolute_index < page_end).then_some(
                    row.absolute_index
                        .saturating_sub(page_base)
                        .saturating_add(1),
                )
            })
            .max()
            .unwrap_or(0)
    }

    pub fn logical_row_count(&self) -> usize {
        let page_base = self.view.current_page.saturating_mul(self.view.limit);
        let server_rows = self.count.map_or_else(
            || self.loaded_server_row_extent_on_page(),
            |count| {
                (count as usize)
                    .saturating_sub(page_base)
                    .min(self.view.limit)
            },
        );
        server_rows.saturating_add(self.added_rows.len())
    }

    pub fn active_row_indices(&self) -> Vec<usize> {
        let page_base = self.view.current_page.saturating_mul(self.view.limit);
        let page_end = page_base.saturating_add(self.view.limit);
        let mut rows = self
            .chunks
            .values()
            .flat_map(|chunk| chunk.rows.iter())
            .filter(|row| row.absolute_index >= page_base && row.absolute_index < page_end)
            .map(|row| row.absolute_index)
            .collect::<Vec<_>>();
        rows.sort_unstable();
        rows.dedup();
        rows.extend(self.added_rows.iter().map(|row| row.absolute_index));
        rows
    }

    pub fn row_indices_between(&self, first: usize, second: usize) -> Vec<usize> {
        let rows = self.active_row_indices();
        let first_index = rows.iter().position(|row| *row == first);
        let second_index = rows.iter().position(|row| *row == second);
        match (first_index, second_index) {
            (Some(first_index), Some(second_index)) => {
                let (start, end) = if first_index <= second_index {
                    (first_index, second_index)
                } else {
                    (second_index, first_index)
                };
                rows[start..=end].to_vec()
            }
            _ if first == second && self.row(first).is_some() => vec![first],
            _ => [first, second]
                .into_iter()
                .filter(|row| self.row(*row).is_some())
                .collect(),
        }
    }

    pub fn select_row(&mut self, row: usize, extend: bool, toggle: bool) {
        let ordered_rows = self.active_row_indices();
        self.selection
            .select_row_from_ordered(row, extend, toggle, &ordered_rows);
    }

    pub fn next_added_row_index(&self) -> usize {
        let used = self
            .chunks
            .values()
            .flat_map(|chunk| chunk.rows.iter())
            .chain(self.added_rows.iter())
            .map(|row| row.absolute_index)
            .collect::<std::collections::HashSet<_>>();
        (0..=used.len())
            .filter_map(|offset| usize::MAX.checked_sub(offset))
            .find(|candidate| !used.contains(candidate))
            .unwrap_or(usize::MAX)
    }

    pub fn can_page_next(&self) -> bool {
        match self.count {
            Some(count) => {
                self.view
                    .current_page
                    .saturating_add(1)
                    .saturating_mul(self.view.limit)
                    < count as usize
            }
            None => self.loaded_server_row_extent_on_page() >= self.view.limit,
        }
    }

    pub fn cycle_sort(&mut self, column: &DatabaseColumnInfo) {
        match (self.view.sorted_column.as_deref(), self.view.sort_direction) {
            (Some(name), Some(DatabaseSortDirection::Asc)) if name == column.name => {
                self.view.sort_direction = Some(DatabaseSortDirection::Desc);
                self.view.order_by = format!("{} DESC", super::quote_pg_identifier(&column.name));
            }
            (Some(name), Some(DatabaseSortDirection::Desc)) if name == column.name => {
                self.view.sorted_column = None;
                self.view.sort_direction = None;
                self.view.order_by.clear();
            }
            _ => {
                self.view.sorted_column = Some(column.name.clone());
                self.view.sort_direction = Some(DatabaseSortDirection::Asc);
                self.view.order_by = format!("{} ASC", super::quote_pg_identifier(&column.name));
            }
        }
        self.order_by_input.set_text(self.view.order_by.clone());
    }

    pub fn prepare_selection_restore(&mut self, metadata: &DatabaseTableMetadata) {
        let mut rows = self.selection.selected_rows.clone();
        let column = self.selection.cell_range().map(|(start, end)| {
            rows.extend(self.row_indices_between(start.row, end.row));
            start.column
        });
        rows.sort_unstable();
        rows.dedup();
        self.restore_selection_keys = rows
            .into_iter()
            .filter_map(|row_index| self.row(row_index))
            .map(|row| primary_key_values(metadata, row))
            .filter(|key| !key.is_empty())
            .collect();
        self.restore_selection_column = column;
        self.selection.clear();
    }

    pub fn restore_pending_selection(&mut self, metadata: &DatabaseTableMetadata) {
        if self.restore_selection_keys.is_empty() {
            self.restore_selection_column = None;
            return;
        }
        let mut matched = Vec::new();
        for chunk in self.chunks.values() {
            for row in &chunk.rows {
                let key = primary_key_values(metadata, row);
                if self
                    .restore_selection_keys
                    .iter()
                    .any(|pending| pending == &key)
                {
                    matched.push((row.absolute_index, key));
                }
            }
        }
        if matched.is_empty() {
            return;
        }
        if let Some(column) = self.restore_selection_column {
            for (index, (row, _)) in matched.iter().enumerate() {
                self.selection
                    .select_cell(DatabaseCellPosition { row: *row, column }, index > 0);
            }
        } else {
            self.selection
                .selected_rows
                .extend(matched.iter().map(|(row, _)| *row));
            self.selection.selected_rows.sort_unstable();
            self.selection.selected_rows.dedup();
        }
        for (_, key) in matched {
            if let Some(index) = self
                .restore_selection_keys
                .iter()
                .position(|pending| pending == &key)
            {
                self.restore_selection_keys.remove(index);
            }
        }
        if self.restore_selection_keys.is_empty() {
            self.restore_selection_column = None;
        }
    }

    pub fn can_reuse_loaded_chunk(&self, chunk_index: usize) -> bool {
        !self.refreshing && self.chunks.contains_key(&chunk_index)
    }

    pub fn finish_refresh(&mut self) {
        self.refreshing = false;
        self.refresh_started = None;
    }

    pub fn clear_loaded_rows(&mut self) {
        self.chunks.clear();
        self.lru.clear();
        self.cache_bytes = 0;
        self.loading_chunk = false;
        self.in_flight_chunk = None;
        self.desired_chunk = None;
    }
}

fn chunk_is_protected(
    chunk: &DatabaseTableChunk,
    selection: &DatabaseGridSelection,
    visible_rows: &std::ops::Range<usize>,
) -> bool {
    chunk.rows.iter().any(DatabaseGridRow::is_dirty)
        || chunk.rows.iter().any(|row| {
            selection.contains_row(row.absolute_index)
                || selection.cell_range().is_some_and(|(start, end)| {
                    row.absolute_index >= start.row && row.absolute_index <= end.row
                })
                || visible_rows.contains(&row.absolute_index)
        })
}

fn primary_key_values(metadata: &DatabaseTableMetadata, row: &DatabaseGridRow) -> Vec<String> {
    metadata
        .primary_key_columns
        .iter()
        .filter_map(|name| {
            let index = metadata
                .columns
                .iter()
                .position(|column| &column.name == name)?;
            row.cells.get(index).map(|cell| cell.original.copy_text())
        })
        .collect()
}
