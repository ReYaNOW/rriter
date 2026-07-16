use super::{
    DatabaseColumnInfo, DatabaseColumnWidth, DatabaseGeneration, DatabaseSortDirection,
    DatabaseTableMetadata, DatabaseTableViewState, MAX_BYTEA_PREVIEW_BYTES,
    MAX_CACHED_CHUNKS_PER_TAB, MAX_DISPLAY_CELL_BYTES, MAX_TABLE_CACHE_BYTES,
};
use crate::scroll::ScrollState;
use std::collections::{BTreeMap, VecDeque};

pub const DATABASE_GRID_ROW_HEIGHT: f32 = 28.0;
pub const DATABASE_GRID_HEADER_HEIGHT: f32 = 30.0;
pub const DATABASE_GRID_MIN_COLUMN_WIDTH: f32 = 60.0;
pub const DATABASE_GRID_DEFAULT_COLUMN_WIDTH: f32 = 150.0;
pub const DATABASE_GRID_MAX_COLUMN_WIDTH: f32 = 4096.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseByteaPreview {
    pub total_bytes: usize,
    pub hex_preview: String,
    pub truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DatabaseCellValue {
    Null,
    Default,
    Text(String),
    Boolean(bool),
    Enum(String),
    DateTime(String),
    ByteaPreview(DatabaseByteaPreview),
}

impl DatabaseCellValue {
    pub fn display_text(&self) -> String {
        match self {
            Self::Null => "<NULL>".to_string(),
            Self::Default => "<default>".to_string(),
            Self::Text(value) | Self::Enum(value) | Self::DateTime(value) => {
                truncate_display(value)
            }
            Self::Boolean(value) => value.to_string(),
            Self::ByteaPreview(preview) => {
                if preview.truncated {
                    format!("<bytea {} bytes: {}…>", preview.total_bytes, preview.hex_preview)
                } else {
                    format!("<bytea {} bytes: {}>", preview.total_bytes, preview.hex_preview)
                }
            }
        }
    }

    pub fn copy_text(&self) -> String {
        match self {
            Self::Null => "<NULL>".to_string(),
            Self::Default => "<default>".to_string(),
            Self::Text(value) | Self::Enum(value) | Self::DateTime(value) => value.clone(),
            Self::Boolean(value) => value.to_string(),
            Self::ByteaPreview(preview) => format!("<bytea {} bytes>", preview.total_bytes),
        }
    }

    pub fn estimated_bytes(&self) -> usize {
        match self {
            Self::Null | Self::Default | Self::Boolean(_) => 8,
            Self::Text(value) | Self::Enum(value) | Self::DateTime(value) => value.len(),
            Self::ByteaPreview(value) => value.hex_preview.len().saturating_add(32),
        }
    }
}

fn truncate_display(value: &str) -> String {
    if value.len() <= MAX_DISPLAY_CELL_BYTES {
        return value.to_string();
    }
    let mut end = MAX_DISPLAY_CELL_BYTES;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… <{} bytes>", &value[..end], value.len())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatabaseRowState {
    Clean,
    Added,
    Deleted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseGridCell {
    pub original: DatabaseCellValue,
    pub value: DatabaseCellValue,
    pub dirty: bool,
}

impl DatabaseGridCell {
    pub fn new(value: DatabaseCellValue) -> Self {
        Self {
            original: value.clone(),
            value,
            dirty: false,
        }
    }

    pub fn set(&mut self, value: DatabaseCellValue) {
        self.dirty = value != self.original;
        self.value = value;
    }

    pub fn undo(&mut self) {
        self.value = self.original.clone();
        self.dirty = false;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseGridRow {
    pub absolute_index: usize,
    pub cells: Vec<DatabaseGridCell>,
    pub xmin: Option<String>,
    pub state: DatabaseRowState,
}

impl DatabaseGridRow {
    pub fn is_dirty(&self) -> bool {
        self.state != DatabaseRowState::Clean || self.cells.iter().any(|cell| cell.dirty)
    }

    pub fn estimated_bytes(&self) -> usize {
        self.cells
            .iter()
            .map(|cell| cell.value.estimated_bytes() + cell.original.estimated_bytes())
            .sum::<usize>()
            .saturating_add(self.xmin.as_ref().map_or(0, String::len))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseTableChunk {
    pub generation: DatabaseGeneration,
    pub chunk_index: usize,
    pub rows: Vec<DatabaseGridRow>,
    pub estimated_bytes: usize,
}

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
            if let Some(index) = self.selected_rows.iter().position(|selected| *selected == row) {
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
}

#[derive(Clone, Debug)]
pub struct DatabaseTableGridState {
    pub view: DatabaseTableViewState,
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
    pub where_input: super::DatabaseDialogInput,
    pub order_by_input: super::DatabaseDialogInput,
    pub cell_editor: Option<DatabaseCellEditorState>,
    pub refresh_prompt: Option<DatabaseTableRefreshPrompt>,
    pub review: Option<DatabaseTableReviewState>,
    pub sql_preview: Option<String>,
    pub pending_close_after_save: bool,
    pub pending_reload: Option<DatabaseTableReloadAction>,
    pub post_commit_refresh_pending: bool,
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
            && self.where_input.text() == other.where_input.text()
            && self.order_by_input.text() == other.order_by_input.text()
            && self.cell_editor == other.cell_editor
            && self.refresh_prompt == other.refresh_prompt
            && self.review == other.review
            && self.sql_preview == other.sql_preview
            && self.pending_close_after_save == other.pending_close_after_save
            && self.pending_reload == other.pending_reload
            && self.post_commit_refresh_pending == other.post_commit_refresh_pending
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
            view,
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
            cell_editor: None,
            refresh_prompt: None,
            review: None,
            sql_preview: None,
            pending_close_after_save: false,
            pending_reload: None,
            post_commit_refresh_pending: false,
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
        self.chunks
            .values()
            .flat_map(|chunk| &chunk.rows)
            .chain(self.added_rows.iter())
            .find(|row| row.absolute_index == absolute_index)
    }

    pub fn row_mut(&mut self, absolute_index: usize) -> Option<&mut DatabaseGridRow> {
        for chunk in self.chunks.values_mut() {
            if let Some(row) = chunk.rows.iter_mut().find(|row| row.absolute_index == absolute_index)
            {
                return Some(row);
            }
        }
        self.added_rows
            .iter_mut()
            .find(|row| row.absolute_index == absolute_index)
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
        self.view
            .column_widths
            .iter()
            .find(|entry| entry.column_name == name)
            .map_or(DATABASE_GRID_DEFAULT_COLUMN_WIDTH, |entry| entry.width_px as f32)
            .clamp(DATABASE_GRID_MIN_COLUMN_WIDTH, DATABASE_GRID_MAX_COLUMN_WIDTH)
    }

    pub fn set_column_width(&mut self, name: &str, width: f32) {
        let width = width
            .clamp(DATABASE_GRID_MIN_COLUMN_WIDTH, DATABASE_GRID_MAX_COLUMN_WIDTH)
            .round() as u16;
        if let Some(entry) = self
            .view
            .column_widths
            .iter_mut()
            .find(|entry| entry.column_name == name)
        {
            entry.width_px = width;
        } else {
            self.view.column_widths.push(DatabaseColumnWidth {
                column_name: name.to_string(),
                width_px: width,
            });
        }
    }

    pub fn content_width(&self, metadata: &DatabaseTableMetadata) -> f32 {
        metadata
            .columns
            .iter()
            .map(|column| self.column_width(&column.name))
            .sum()
    }

    pub fn logical_row_count(&self) -> usize {
        let page_base = self.view.current_page.saturating_mul(self.view.limit);
        let server_rows = self.count.map_or(self.view.limit, |count| {
            (count as usize).saturating_sub(page_base).min(self.view.limit)
        });
        server_rows.saturating_add(self.added_rows.len())
    }

    pub fn cycle_sort(&mut self, column: &DatabaseColumnInfo) {
        match (
            self.view.sorted_column.as_deref(),
            self.view.sort_direction,
        ) {
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
            rows.extend(start.row..=end.row);
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
                if self.restore_selection_keys.iter().any(|pending| pending == &key) {
                    matched.push((row.absolute_index, key));
                }
            }
        }
        if matched.is_empty() {
            return;
        }
        if let Some(column) = self.restore_selection_column {
            for (index, (row, _)) in matched.iter().enumerate() {
                self.selection.select_cell(
                    DatabaseCellPosition { row: *row, column },
                    index > 0,
                );
            }
        } else {
            self.selection.selected_rows.extend(matched.iter().map(|(row, _)| *row));
            self.selection.selected_rows.sort_unstable();
            self.selection.selected_rows.dedup();
        }
        for (_, key) in matched {
            if let Some(index) = self.restore_selection_keys.iter().position(|pending| pending == &key) {
                self.restore_selection_keys.remove(index);
            }
        }
        if self.restore_selection_keys.is_empty() {
            self.restore_selection_column = None;
        }
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

fn primary_key_values(
    metadata: &DatabaseTableMetadata,
    row: &DatabaseGridRow,
) -> Vec<String> {
    metadata
        .primary_key_columns
        .iter()
        .filter_map(|name| {
            let index = metadata.columns.iter().position(|column| &column.name == name)?;
            row.cells.get(index).map(|cell| cell.original.copy_text())
        })
        .collect()
}

pub fn parse_editor_value(
    text: &str,
    column: &DatabaseColumnInfo,
    literal: bool,
) -> Result<DatabaseCellValue, String> {
    if !literal && text.eq_ignore_ascii_case("<null>") {
        if column.nullable {
            return Ok(DatabaseCellValue::Null);
        }
        return Err("Столбец не допускает NULL".to_string());
    }
    if !literal && (text.eq_ignore_ascii_case("<default>") || text.eq_ignore_ascii_case("<def>")) {
        if column.default_expression.is_some() || column.identity || column.generated {
            return Ok(DatabaseCellValue::Default);
        }
        return Err("У столбца нет значения DEFAULT".to_string());
    }
    match column.type_kind {
        super::DatabaseTypeKind::Boolean => match text.trim().to_ascii_lowercase().as_str() {
            "true" | "t" | "1" | "yes" | "on" => Ok(DatabaseCellValue::Boolean(true)),
            "false" | "f" | "0" | "no" | "off" => Ok(DatabaseCellValue::Boolean(false)),
            _ => Err("Ожидается true или false".to_string()),
        },
        super::DatabaseTypeKind::Enum => {
            if column.enum_values.iter().any(|value| value == text) {
                Ok(DatabaseCellValue::Enum(text.to_string()))
            } else {
                Err("Значение отсутствует в PostgreSQL enum".to_string())
            }
        }
        super::DatabaseTypeKind::Date
        | super::DatabaseTypeKind::Time
        | super::DatabaseTypeKind::Timestamp
        | super::DatabaseTypeKind::TimestampTz => Ok(DatabaseCellValue::DateTime(text.to_string())),
        super::DatabaseTypeKind::Bytea => Err("Редактирование bytea отключено".to_string()),
        _ => Ok(DatabaseCellValue::Text(text.to_string())),
    }
}

pub fn civil_date_from_unix_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe as i32 + era as i32 * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (mp + if mp < 10 { 3 } else { -9 }) as u32;
    year += (month <= 2) as i32;
    (year, month, day)
}

pub fn parse_bytea_preview(value: &str) -> DatabaseByteaPreview {
    let Some((size, hex)) = value.split_once(':') else {
        return DatabaseByteaPreview {
            total_bytes: 0,
            hex_preview: String::new(),
            truncated: false,
        };
    };
    let total_bytes = size.parse::<usize>().unwrap_or(0);
    let max_hex = MAX_BYTEA_PREVIEW_BYTES.saturating_mul(2);
    let mut end = hex.len().min(max_hex);
    while end > 0 && !hex.is_char_boundary(end) {
        end -= 1;
    }
    DatabaseByteaPreview {
        total_bytes,
        hex_preview: hex[..end].to_string(),
        truncated: total_bytes > MAX_BYTEA_PREVIEW_BYTES || hex.len() > end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::database::{DatabaseTableViewKey, DatabaseTypeKind};

    fn column(kind: DatabaseTypeKind) -> DatabaseColumnInfo {
        DatabaseColumnInfo {
            ordinal: 1,
            name: "value".to_string(),
            type_name: "text".to_string(),
            type_oid: 25,
            type_kind: kind,
            nullable: true,
            default_expression: Some("'x'::text".to_string()),
            identity: false,
            generated: false,
            primary_key: false,
            enum_values: vec!["a".to_string(), "b".to_string()],
        }
    }

    fn grid() -> DatabaseTableGridState {
        DatabaseTableGridState::new(DatabaseTableViewState {
            key: DatabaseTableViewKey {
                connection_id: super::super::DatabaseConnectionId(1),
                database_name: "db".to_string(),
                table_name: "items".to_string(),
            },
            ..DatabaseTableViewState::default()
        })
    }

    #[test]
    fn null_default_boolean_and_enum_tokens_are_typed() {
        assert_eq!(
            parse_editor_value("<NULL>", &column(DatabaseTypeKind::Other), false).unwrap(),
            DatabaseCellValue::Null
        );
        assert_eq!(
            parse_editor_value("<def>", &column(DatabaseTypeKind::Other), false).unwrap(),
            DatabaseCellValue::Default
        );
        assert_eq!(
            parse_editor_value("yes", &column(DatabaseTypeKind::Boolean), false).unwrap(),
            DatabaseCellValue::Boolean(true)
        );
        assert!(parse_editor_value("c", &column(DatabaseTypeKind::Enum), false).is_err());
        assert_eq!(
            parse_editor_value("<NULL>", &column(DatabaseTypeKind::Other), true).unwrap(),
            DatabaseCellValue::Text("<NULL>".to_string())
        );
    }

    #[test]
    fn selection_supports_rectangles_and_multiple_rows() {
        let mut selection = DatabaseGridSelection::default();
        selection.select_cell(DatabaseCellPosition { row: 3, column: 4 }, false);
        selection.select_cell(DatabaseCellPosition { row: 5, column: 2 }, true);
        assert!(selection.contains_cell(4, 3));
        assert!(!selection.contains_cell(2, 3));

        selection.select_row(2, false, false);
        selection.select_row(4, false, true);
        assert!(selection.contains_row(2));
        assert!(selection.contains_row(4));
    }

    #[test]
    fn sort_cycles_and_replaces_manual_order_by() {
        let mut grid = grid();
        let column = column(DatabaseTypeKind::Other);
        grid.view.order_by = "manual DESC".to_string();
        grid.cycle_sort(&column);
        assert_eq!(grid.view.order_by, "\"value\" ASC");
        grid.cycle_sort(&column);
        assert_eq!(grid.view.order_by, "\"value\" DESC");
        grid.cycle_sort(&column);
        assert!(grid.view.order_by.is_empty());
    }

    #[test]
    fn chunk_cache_evicts_clean_lru_but_keeps_dirty_rows() {
        let mut grid = grid();
        for index in 0..10 {
            let mut row = DatabaseGridRow {
                absolute_index: index,
                cells: vec![DatabaseGridCell::new(DatabaseCellValue::Text("x".to_string()))],
                xmin: Some("1".to_string()),
                state: DatabaseRowState::Clean,
            };
            if index == 0 {
                row.cells[0].set(DatabaseCellValue::Text("dirty".to_string()));
            }
            grid.insert_chunk(DatabaseTableChunk {
                generation: DatabaseGeneration(1),
                chunk_index: index,
                estimated_bytes: row.estimated_bytes(),
                rows: vec![row],
            });
        }
        assert!(grid.chunks.len() <= MAX_CACHED_CHUNKS_PER_TAB + 1);
        assert!(grid.chunks.contains_key(&0));
    }

    #[test]
    fn last_page_uses_only_remaining_server_rows() {
        let mut grid = grid();
        grid.view.limit = 100;
        grid.view.current_page = 5;
        grid.count = Some(550);
        assert_eq!(grid.logical_row_count(), 50);
        grid.added_rows.push(DatabaseGridRow {
            absolute_index: 550,
            cells: vec![DatabaseGridCell::new(DatabaseCellValue::Null)],
            xmin: None,
            state: DatabaseRowState::Added,
        });
        assert_eq!(grid.logical_row_count(), 51);
    }

    #[test]
    fn cache_keeps_visible_and_selected_chunks() {
        let mut grid = grid();
        grid.viewport_height = DATABASE_GRID_ROW_HEIGHT * 2.0;
        grid.selection.select_row(100, false, false);
        for index in 0..12 {
            let absolute_index = index * 100;
            let row = DatabaseGridRow {
                absolute_index,
                cells: vec![DatabaseGridCell::new(DatabaseCellValue::Text(index.to_string()))],
                xmin: Some("1".to_string()),
                state: DatabaseRowState::Clean,
            };
            grid.insert_chunk(DatabaseTableChunk {
                generation: DatabaseGeneration(1),
                chunk_index: index,
                estimated_bytes: row.estimated_bytes(),
                rows: vec![row],
            });
        }
        assert!(grid.chunks.contains_key(&0), "visible chunk must stay cached");
        assert!(grid.chunks.contains_key(&1), "selected chunk must stay cached");
        assert!(grid.chunks.len() <= MAX_CACHED_CHUNKS_PER_TAB + 2);
    }

    #[test]
    fn selection_is_restored_by_primary_key_after_reload() {
        let metadata = DatabaseTableMetadata {
            database_name: "db".to_string(),
            table_name: "items".to_string(),
            columns: vec![
                DatabaseColumnInfo {
                    ordinal: 1,
                    name: "id".to_string(),
                    type_name: "integer".to_string(),
                    type_oid: 23,
                    type_kind: DatabaseTypeKind::Other,
                    nullable: false,
                    default_expression: None,
                    identity: false,
                    generated: false,
                    primary_key: true,
                    enum_values: Vec::new(),
                },
                column(DatabaseTypeKind::Other),
            ],
            primary_key_columns: vec!["id".to_string()],
            editable: true,
            read_only_reason: None,
            notices: Vec::new(),
        };
        let mut grid = grid();
        grid.insert_chunk(DatabaseTableChunk {
            generation: DatabaseGeneration(1),
            chunk_index: 0,
            estimated_bytes: 64,
            rows: vec![DatabaseGridRow {
                absolute_index: 2,
                cells: vec![
                    DatabaseGridCell::new(DatabaseCellValue::Text("7".to_string())),
                    DatabaseGridCell::new(DatabaseCellValue::Text("old".to_string())),
                ],
                xmin: Some("1".to_string()),
                state: DatabaseRowState::Clean,
            }],
        });
        grid.selection.select_cell(DatabaseCellPosition { row: 2, column: 1 }, false);
        grid.prepare_selection_restore(&metadata);
        grid.clear_loaded_rows();
        grid.insert_chunk(DatabaseTableChunk {
            generation: DatabaseGeneration(2),
            chunk_index: 0,
            estimated_bytes: 64,
            rows: vec![DatabaseGridRow {
                absolute_index: 10,
                cells: vec![
                    DatabaseGridCell::new(DatabaseCellValue::Text("7".to_string())),
                    DatabaseGridCell::new(DatabaseCellValue::Text("fresh".to_string())),
                ],
                xmin: Some("2".to_string()),
                state: DatabaseRowState::Clean,
            }],
        });
        grid.restore_pending_selection(&metadata);
        assert_eq!(
            grid.selection.cell_range(),
            Some((
                DatabaseCellPosition { row: 10, column: 1 },
                DatabaseCellPosition { row: 10, column: 1 },
            ))
        );
    }

    #[test]
    fn bytea_preview_never_exposes_unbounded_payload() {
        let preview = parse_bytea_preview(&format!("70000:{}", "aa".repeat(70_000)));
        assert_eq!(preview.total_bytes, 70_000);
        assert!(preview.truncated);
        assert_eq!(preview.hex_preview.len(), MAX_BYTEA_PREVIEW_BYTES * 2);
    }
}
