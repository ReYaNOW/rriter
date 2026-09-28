use super::{
    DatabaseColumnInfo, DatabaseColumnWidth, DatabaseGeneration, DatabaseSortDirection,
    DatabaseTableMetadata, DatabaseTableViewState, MAX_BYTEA_PREVIEW_BYTES,
    MAX_CACHED_CHUNKS_PER_TAB, MAX_DISPLAY_CELL_BYTES, MAX_TABLE_CACHE_BYTES,
};
use crate::scroll::ScrollState;
use std::collections::{BTreeMap, VecDeque};
use std::fmt::Write as _;

pub const DATABASE_GRID_ROW_HEIGHT: f32 = 38.0;
pub const DATABASE_GRID_HEADER_HEIGHT: f32 = 40.0;
pub const DATABASE_TABLE_INPUT_TEXT_SCALE: f32 = 0.9;
pub const DATABASE_GRID_MIN_COLUMN_WIDTH: f32 = 60.0;
pub const DATABASE_GRID_DEFAULT_COLUMN_WIDTH: f32 = 150.0;
pub const DATABASE_GRID_MAX_COLUMN_WIDTH: f32 = 4096.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DatabaseGridViewport {
    pub show_x: bool,
    pub show_y: bool,
    pub body_w: f32,
    pub body_h: f32,
    pub data_w: f32,
    pub rows_h: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DatabaseGridRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DatabaseGridLayout {
    pub outer_rect: DatabaseGridRect,
    pub header_rect: DatabaseGridRect,
    pub body_rect: DatabaseGridRect,
    pub vertical_scrollbar_rect: Option<DatabaseGridRect>,
    pub horizontal_scrollbar_rect: Option<DatabaseGridRect>,
    pub viewport: DatabaseGridViewport,
}

pub fn database_grid_viewport(
    width: f32,
    height: f32,
    gutter_w: f32,
    scrollbar_w: f32,
    header_h: f32,
    content_w: f32,
    total_rows_h: f32,
) -> DatabaseGridViewport {
    let mut show_y = total_rows_h > (height - header_h).max(0.0);
    let mut data_w = (width - gutter_w - if show_y { scrollbar_w } else { 0.0 }).max(0.0);
    let mut show_x = content_w > data_w;
    let mut body_h = (height - if show_x { scrollbar_w } else { 0.0 }).max(0.0);
    let mut rows_h = (body_h - header_h).max(0.0);
    if !show_y && total_rows_h > rows_h {
        show_y = true;
        data_w = (width - gutter_w - scrollbar_w).max(0.0);
        show_x = content_w > data_w;
        body_h = (height - if show_x { scrollbar_w } else { 0.0 }).max(0.0);
        rows_h = (body_h - header_h).max(0.0);
    }
    DatabaseGridViewport {
        show_x,
        show_y,
        body_w: (width - if show_y { scrollbar_w } else { 0.0 }).max(0.0),
        body_h,
        data_w,
        rows_h,
    }
}

pub fn database_grid_layout(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    gutter_width: f32,
    scrollbar_width: f32,
    header_height: f32,
    content_width: f32,
    total_rows_height: f32,
) -> DatabaseGridLayout {
    let viewport = database_grid_viewport(
        width,
        height,
        gutter_width,
        scrollbar_width,
        header_height,
        content_width,
        total_rows_height,
    );
    let data_x = x + gutter_width;
    let header_rect = DatabaseGridRect {
        x: data_x,
        y,
        w: viewport.data_w,
        h: header_height.min(viewport.body_h).max(0.0),
    };
    let body_rect = DatabaseGridRect {
        x: data_x,
        y: y + header_rect.h,
        w: viewport.data_w,
        h: viewport.rows_h,
    };
    let vertical_scrollbar_rect = viewport.show_y.then_some(DatabaseGridRect {
        x: x + viewport.body_w,
        y: body_rect.y,
        w: scrollbar_width,
        h: body_rect.h,
    });
    let horizontal_scrollbar_rect = viewport.show_x.then_some(DatabaseGridRect {
        x: data_x,
        y: y + viewport.body_h,
        w: viewport.data_w,
        h: scrollbar_width,
    });
    DatabaseGridLayout {
        outer_rect: DatabaseGridRect {
            x,
            y,
            w: width,
            h: height,
        },
        header_rect,
        body_rect,
        vertical_scrollbar_rect,
        horizontal_scrollbar_rect,
        viewport,
    }
}

pub fn database_grid_max_scroll(row_count: usize, row_height: f32, viewport_height: f32) -> f32 {
    (row_count as f32 * row_height.max(0.0) - viewport_height.max(0.0)).max(0.0)
}

/// Render height of one grid row. Renders round it so glyphs stay pixel-stable;
/// scroll math must use the same rounded value or the last rows stay unreachable
/// at fractional scales.
pub fn database_grid_row_height_px(scale: f32) -> f32 {
    (DATABASE_GRID_ROW_HEIGHT * scale).round()
}

pub fn database_grid_header_height_px(scale: f32) -> f32 {
    (DATABASE_GRID_HEADER_HEIGHT * scale).round()
}

/// Row height in logical (unscaled) units, matching the render's rounded pixels.
pub fn database_grid_row_height_logical(scale: f32) -> f32 {
    database_grid_row_height_px(scale) / scale.max(0.001)
}

/// Logical viewport size of the grid body rect as produced by the layout.
pub fn database_grid_viewport_from_body_rect(width: f32, height: f32, scale: f32) -> (f32, f32) {
    let s = scale.max(0.001);
    (
        (width / s - 54.0).max(0.0),
        ((height - database_grid_header_height_px(scale)) / s).max(0.0),
    )
}

pub fn database_grid_visible_row_range(
    scroll_y: f32,
    row_height: f32,
    viewport_height: f32,
    row_count: usize,
) -> std::ops::Range<usize> {
    if row_count == 0 || row_height <= f32::EPSILON || viewport_height <= 0.0 {
        return 0..0;
    }
    let start = (scroll_y.max(0.0) / row_height).floor() as usize;
    let end = ((scroll_y.max(0.0) + viewport_height.max(0.0)) / row_height).ceil() as usize;
    start.min(row_count)..end.min(row_count)
}

pub fn database_column_width(widths: &[DatabaseColumnWidth], name: &str) -> f32 {
    widths
        .iter()
        .find(|entry| entry.column_name == name)
        .map_or(DATABASE_GRID_DEFAULT_COLUMN_WIDTH, |entry| {
            entry.width_px as f32
        })
        .clamp(
            DATABASE_GRID_MIN_COLUMN_WIDTH,
            DATABASE_GRID_MAX_COLUMN_WIDTH,
        )
}

pub fn set_database_column_width(widths: &mut Vec<DatabaseColumnWidth>, name: &str, width: f32) {
    let width = width
        .clamp(
            DATABASE_GRID_MIN_COLUMN_WIDTH,
            DATABASE_GRID_MAX_COLUMN_WIDTH,
        )
        .round() as u16;
    if let Some(entry) = widths.iter_mut().find(|entry| entry.column_name == name) {
        entry.width_px = width;
    } else {
        widths.push(DatabaseColumnWidth {
            column_name: name.to_string(),
            width_px: width,
        });
    }
}

pub fn database_columns_content_width(
    widths: &[DatabaseColumnWidth],
    columns: impl IntoIterator<Item = impl AsRef<str>>,
) -> f32 {
    columns
        .into_iter()
        .map(|column| database_column_width(widths, column.as_ref()))
        .sum()
}

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
        let mut scratch = String::new();
        self.display_text_into(&mut scratch).to_owned()
    }

    pub(crate) fn display_text_into<'a>(&'a self, scratch: &'a mut String) -> &'a str {
        match self {
            Self::Null => "<NULL>",
            Self::Default => "<default>",
            Self::Text(value) | Self::Enum(value) | Self::DateTime(value) => {
                truncate_display(value, scratch)
            }
            Self::Boolean(false) => "false",
            Self::Boolean(true) => "true",
            Self::ByteaPreview(preview) => {
                scratch.clear();
                let _ = write!(scratch, "<bytea {} bytes: ", preview.total_bytes);
                scratch.push_str(&preview.hex_preview);
                if preview.truncated {
                    scratch.push('…');
                }
                scratch.push('>');
                scratch.as_str()
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

fn truncate_display<'a>(value: &'a str, scratch: &'a mut String) -> &'a str {
    if value.len() <= MAX_DISPLAY_CELL_BYTES {
        return value;
    }
    let mut end = MAX_DISPLAY_CELL_BYTES;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    scratch.clear();
    scratch.push_str(&value[..end]);
    let _ = write!(scratch, "… <{} bytes>", value.len());
    scratch.as_str()
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

include!("database_grid_selection_state.rs");

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

    fn clean_row(absolute_index: usize) -> DatabaseGridRow {
        DatabaseGridRow {
            absolute_index,
            cells: vec![DatabaseGridCell::new(DatabaseCellValue::Text(
                absolute_index.to_string(),
            ))],
            xmin: None,
            state: DatabaseRowState::Clean,
        }
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
                cells: vec![DatabaseGridCell::new(DatabaseCellValue::Text(
                    "x".to_string(),
                ))],
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
    fn refresh_does_not_reuse_stale_cached_chunk() {
        let mut grid = grid();
        grid.insert_chunk(DatabaseTableChunk {
            generation: DatabaseGeneration(1),
            chunk_index: 0,
            estimated_bytes: 1,
            rows: Vec::new(),
        });
        assert!(grid.can_reuse_loaded_chunk(0));

        grid.start_refresh(std::time::Instant::now());
        assert!(!grid.can_reuse_loaded_chunk(0));
    }

    #[test]
    fn finishing_refresh_clears_delayed_overlay_state() {
        let mut grid = grid();
        grid.start_refresh(std::time::Instant::now());
        grid.refresh_indicator_last_drawn_step = Some(42);

        grid.finish_refresh();

        assert!(!grid.refreshing);
        assert!(grid.refresh_started.is_none());
        assert!(grid.refresh_indicator_last_drawn_step.is_none());
    }

    #[test]
    fn refresh_indicator_tick_redraws_once_per_visible_step() {
        let mut grid = grid();
        let started = std::time::Instant::now();
        grid.start_refresh(started);

        let (redraw, wake) = grid.refresh_indicator_tick(
            started + crate::app::database::DATABASE_REFRESH_INDICATOR_DELAY
                - std::time::Duration::from_millis(1),
            1_234,
        );
        assert!(!redraw);
        assert_eq!(wake, Some(started + crate::app::database::DATABASE_REFRESH_INDICATOR_DELAY));

        let (redraw, wake) = grid.refresh_indicator_tick(
            started + crate::app::database::DATABASE_REFRESH_INDICATOR_DELAY,
            1_234,
        );
        assert!(redraw);
        assert_eq!(wake, Some(started + crate::app::database::DATABASE_REFRESH_INDICATOR_DELAY + std::time::Duration::from_millis(66)));

        let (redraw, _) = grid.refresh_indicator_tick(
            started + crate::app::database::DATABASE_REFRESH_INDICATOR_DELAY
                + std::time::Duration::from_millis(10),
            1_235,
        );
        assert!(!redraw);

        let (redraw, _) = grid.refresh_indicator_tick(
            started + crate::app::database::DATABASE_REFRESH_INDICATOR_DELAY
                + std::time::Duration::from_millis(66),
            1_300,
        );
        assert!(redraw);
    }

    #[test]
    fn pending_filter_view_commits_only_after_successful_chunk() {
        let mut grid = grid();
        grid.count = Some(25);
        let mut pending = grid.view.clone();
        pending.where_clause = "id=10".to_string();
        pending.order_by = "id DESC".to_string();
        grid.begin_pending_view(pending.clone(), true, true);
        grid.pending_count = Some(1);

        assert!(grid.commit_pending_view());
        assert_eq!(grid.view, pending);
        assert_eq!(grid.count, Some(1));
        assert!(grid.pending_view.is_none());
    }

    #[test]
    fn failed_pending_filter_preserves_last_successful_view() {
        let mut grid = grid();
        let applied = grid.view.clone();
        let mut pending = applied.clone();
        pending.where_clause = "missing=10".to_string();
        grid.begin_pending_view(pending, true, false);

        assert_eq!(
            grid.pending_filter_error_target(false),
            Some(DatabaseTableInputTarget::Where)
        );
        grid.abort_pending_view();
        assert_eq!(grid.view, applied);
        assert!(grid.pending_view.is_none());
    }

    #[test]
    fn chunk_error_prefers_order_by_when_both_filters_changed() {
        let mut grid = grid();
        let mut pending = grid.view.clone();
        pending.where_clause = "id=10".to_string();
        pending.order_by = "missing DESC".to_string();
        grid.begin_pending_view(pending, true, true);
        assert_eq!(
            grid.pending_filter_error_target(true),
            Some(DatabaseTableInputTarget::OrderBy)
        );
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
    fn bug_63_unknown_count_uses_loaded_extent_instead_of_full_limit() {
        let mut grid = grid();
        grid.view.limit = 100;
        grid.count = None;
        assert_eq!(grid.logical_row_count(), 0);
        grid.chunks.insert(
            0,
            DatabaseTableChunk {
                generation: DatabaseGeneration(1),
                chunk_index: 0,
                rows: vec![DatabaseGridRow {
                    absolute_index: 7,
                    cells: vec![DatabaseGridCell::new(DatabaseCellValue::Text(
                        "row".to_string(),
                    ))],
                    xmin: None,
                    state: DatabaseRowState::Clean,
                }],
                estimated_bytes: 0,
            },
        );
        assert_eq!(grid.logical_row_count(), 8);
        assert_ne!(grid.logical_row_count(), grid.view.limit);
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
                cells: vec![DatabaseGridCell::new(DatabaseCellValue::Text(
                    index.to_string(),
                ))],
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
        assert!(
            grid.chunks.contains_key(&0),
            "visible chunk must stay cached"
        );
        assert!(
            grid.chunks.contains_key(&1),
            "selected chunk must stay cached"
        );
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
        grid.selection
            .select_cell(DatabaseCellPosition { row: 2, column: 1 }, false);
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

    #[test]
    fn cell_display_borrows_common_values_and_reuses_format_scratch() {
        let mut scratch = String::with_capacity(128);
        scratch.push_str("sentinel");
        let text = DatabaseCellValue::Text("hello".to_string());
        let text_ptr = match &text {
            DatabaseCellValue::Text(value) => value.as_ptr(),
            _ => unreachable!(),
        };
        {
            let rendered = text.display_text_into(&mut scratch);
            assert_eq!(rendered, "hello");
            assert_eq!(rendered.as_ptr(), text_ptr);
        }
        assert_eq!(scratch, "sentinel");

        let enum_value = DatabaseCellValue::Enum("ready".to_string());
        assert_eq!(enum_value.display_text_into(&mut scratch), "ready");
        assert_eq!(scratch, "sentinel");
        let datetime = DatabaseCellValue::DateTime("2026-09-12 01:10:00".to_string());
        assert_eq!(
            datetime.display_text_into(&mut scratch),
            "2026-09-12 01:10:00"
        );
        assert_eq!(scratch, "sentinel");

        for (value, expected) in [
            (DatabaseCellValue::Null, "<NULL>"),
            (DatabaseCellValue::Default, "<default>"),
            (DatabaseCellValue::Boolean(false), "false"),
            (DatabaseCellValue::Boolean(true), "true"),
        ] {
            assert_eq!(value.display_text_into(&mut scratch), expected);
            assert_eq!(scratch, "sentinel");
        }

        scratch.clear();
        let capacity = scratch.capacity();
        let bytea = DatabaseCellValue::ByteaPreview(DatabaseByteaPreview {
            total_bytes: 3,
            hex_preview: "010203".to_string(),
            truncated: false,
        });
        assert_eq!(
            bytea.display_text_into(&mut scratch),
            "<bytea 3 bytes: 010203>"
        );
        assert_eq!(scratch.capacity(), capacity);
    }

    #[test]
    fn cell_display_truncation_keeps_utf8_boundary_and_byte_suffix() {
        let mut value = "a".repeat(MAX_DISPLAY_CELL_BYTES - 1);
        value.push('Ж');
        let cell = DatabaseCellValue::Text(value);
        let mut scratch = String::with_capacity(MAX_DISPLAY_CELL_BYTES + 32);
        let capacity = scratch.capacity();
        let rendered = cell.display_text_into(&mut scratch);
        let suffix = format!("… <{} bytes>", MAX_DISPLAY_CELL_BYTES + 1);
        let prefix = rendered.strip_suffix(&suffix).expect("truncation suffix");
        assert_eq!(prefix.len(), MAX_DISPLAY_CELL_BYTES - 1);
        assert!(prefix.bytes().all(|byte| byte == b'a'));
        assert_eq!(scratch.capacity(), capacity);
    }

    #[test]
    fn row_lookup_uses_expected_page_chunk_and_handles_boundaries() {
        let mut grid = grid();
        grid.view.limit = 250;
        grid.view.current_page = 2;
        for (chunk_index, start, end) in [(0, 500, 600), (1, 600, 700), (2, 700, 725)] {
            grid.chunks.insert(
                chunk_index,
                DatabaseTableChunk {
                    generation: DatabaseGeneration(1),
                    chunk_index,
                    rows: (start..end).map(clean_row).collect(),
                    estimated_bytes: 0,
                },
            );
        }

        for absolute_index in [500, 550, 599, 600, 699, 700, 724] {
            assert_eq!(
                grid.row(absolute_index).map(|row| row.absolute_index),
                Some(absolute_index)
            );
        }
        assert!(grid.row(499).is_none());
        assert!(grid.row(725).is_none());
        grid.chunks.remove(&1);
        assert!(grid.row(600).is_none());

        grid.added_rows.push(DatabaseGridRow {
            absolute_index: usize::MAX,
            cells: Vec::new(),
            xmin: None,
            state: DatabaseRowState::Added,
        });
        assert_eq!(
            grid.row(usize::MAX).map(|row| row.state),
            Some(DatabaseRowState::Added)
        );
    }

    #[test]
    fn row_lookup_preserves_sparse_chunk_fallback_and_mutation() {
        let mut grid = grid();
        grid.view.limit = 100;
        grid.chunks.insert(
            0,
            DatabaseTableChunk {
                generation: DatabaseGeneration(1),
                chunk_index: 0,
                rows: vec![clean_row(10), clean_row(42)],
                estimated_bytes: 0,
            },
        );
        assert_eq!(grid.row(42).map(|row| row.absolute_index), Some(42));
        grid.row_mut(42).expect("sparse row").state = DatabaseRowState::Deleted;
        assert_eq!(
            grid.row(42).map(|row| row.state),
            Some(DatabaseRowState::Deleted)
        );
    }

    #[test]
    fn row_lookup_returns_none_after_chunk_eviction() {
        let mut grid = grid();
        grid.view.limit = 1_200;
        grid.viewport_height = DATABASE_GRID_ROW_HEIGHT * 2.0;
        for chunk_index in 0..10 {
            let absolute_index = chunk_index * super::super::DATABASE_CHUNK_SIZE;
            let row = clean_row(absolute_index);
            grid.insert_chunk(DatabaseTableChunk {
                generation: DatabaseGeneration(1),
                chunk_index,
                estimated_bytes: row.estimated_bytes(),
                rows: vec![row],
            });
        }
        assert!(
            grid.chunks.contains_key(&0),
            "visible chunk stays protected"
        );
        let evicted = (1..10)
            .find(|chunk_index| !grid.chunks.contains_key(chunk_index))
            .expect("one non-visible chunk should be evicted");
        assert!(
            grid.row(evicted * super::super::DATABASE_CHUNK_SIZE)
                .is_none()
        );
    }

    #[test]
    fn shared_column_width_helpers_match_table_resize_rules() {
        let mut widths = Vec::new();
        assert_eq!(
            database_column_width(&widths, "name"),
            DATABASE_GRID_DEFAULT_COLUMN_WIDTH
        );
        set_database_column_width(&mut widths, "name", 12.0);
        assert_eq!(
            database_column_width(&widths, "name"),
            DATABASE_GRID_MIN_COLUMN_WIDTH
        );
        set_database_column_width(&mut widths, "name", 320.0);
        assert_eq!(database_column_width(&widths, "name"), 320.0);
        set_database_column_width(&mut widths, "other", DATABASE_GRID_MAX_COLUMN_WIDTH + 100.0);
        assert_eq!(
            database_column_width(&widths, "other"),
            DATABASE_GRID_MAX_COLUMN_WIDTH
        );
        assert_eq!(
            database_columns_content_width(&widths, ["name", "other"]),
            320.0 + DATABASE_GRID_MAX_COLUMN_WIDTH
        );
    }

    #[test]
    fn shared_grid_visible_range_keeps_last_of_one_hundred_rows() {
        let row_h = 38.0;
        let viewport_h = row_h * 9.5;
        let max_scroll = database_grid_max_scroll(100, row_h, viewport_h);
        let range = database_grid_visible_row_range(max_scroll, row_h, viewport_h, 100);
        assert_eq!(range.end, 100);
        assert!(range.contains(&99));
    }

    #[test]
    fn shared_grid_visible_range_handles_empty_tiny_and_fractional_viewports() {
        assert_eq!(database_grid_visible_row_range(0.0, 38.0, 100.0, 0), 0..0);
        assert_eq!(database_grid_visible_row_range(0.0, 38.0, 1.0, 1), 0..1);
        let scale = 1.33;
        let row_h = DATABASE_GRID_ROW_HEIGHT * scale;
        let viewport_h = row_h * 7.25;
        let max_scroll = database_grid_max_scroll(100, row_h, viewport_h);
        assert!(database_grid_visible_row_range(max_scroll, row_h, viewport_h, 100).contains(&99));
    }

    #[test]
    fn shared_grid_layout_reserves_header_body_and_both_scrollbars_once() {
        let layout = database_grid_layout(10.0, 20.0, 500.0, 300.0, 0.0, 10.0, 40.0, 700.0, 500.0);
        assert!(layout.viewport.show_x);
        assert!(layout.viewport.show_y);
        assert_eq!(
            layout.header_rect,
            DatabaseGridRect {
                x: 10.0,
                y: 20.0,
                w: 490.0,
                h: 40.0
            }
        );
        assert_eq!(
            layout.body_rect,
            DatabaseGridRect {
                x: 10.0,
                y: 60.0,
                w: 490.0,
                h: 250.0
            }
        );
        assert_eq!(
            layout.vertical_scrollbar_rect,
            Some(DatabaseGridRect {
                x: 500.0,
                y: 60.0,
                w: 10.0,
                h: 250.0
            }),
        );
        assert_eq!(
            layout.horizontal_scrollbar_rect,
            Some(DatabaseGridRect {
                x: 10.0,
                y: 310.0,
                w: 490.0,
                h: 10.0
            }),
        );
    }

    #[test]
    fn unknown_count_pages_only_after_a_full_loaded_page() {
        let mut grid = grid();
        grid.view.limit = 2;
        grid.insert_chunk(DatabaseTableChunk {
            generation: DatabaseGeneration(1),
            chunk_index: 0,
            rows: vec![
                DatabaseGridRow {
                    absolute_index: 0,
                    cells: Vec::new(),
                    xmin: None,
                    state: DatabaseRowState::Clean,
                },
                DatabaseGridRow {
                    absolute_index: 1,
                    cells: Vec::new(),
                    xmin: None,
                    state: DatabaseRowState::Clean,
                },
            ],
            estimated_bytes: 0,
        });
        assert!(grid.can_page_next());
        grid.chunks.get_mut(&0).unwrap().rows.pop();
        assert!(!grid.can_page_next());
    }

    #[test]
    fn a4_b003_maximum_page_never_overflows_next_page_check() {
        let mut grid = grid();
        grid.view.current_page = usize::MAX;
        grid.view.limit = 100;
        grid.count = Some(u64::MAX);
        assert!(!grid.can_page_next());
    }

    #[test]
    fn added_row_indices_remain_unique_after_middle_deletion() {
        let mut grid = grid();
        grid.added_rows = vec![
            DatabaseGridRow {
                absolute_index: 5,
                cells: Vec::new(),
                xmin: None,
                state: DatabaseRowState::Added,
            },
            DatabaseGridRow {
                absolute_index: 7,
                cells: Vec::new(),
                xmin: None,
                state: DatabaseRowState::Added,
            },
        ];
        assert_eq!(grid.next_added_row_index(), usize::MAX);
        grid.added_rows.push(DatabaseGridRow {
            absolute_index: usize::MAX,
            cells: Vec::new(),
            xmin: None,
            state: DatabaseRowState::Added,
        });
        assert_eq!(grid.next_added_row_index(), usize::MAX - 1);
    }

    #[test]
    fn synthetic_added_row_ids_use_display_order_for_range_selection() {
        let mut grid = grid();
        grid.view.limit = 2;
        grid.insert_chunk(DatabaseTableChunk {
            generation: DatabaseGeneration(1),
            chunk_index: 0,
            rows: vec![DatabaseGridRow {
                absolute_index: 0,
                cells: Vec::new(),
                xmin: None,
                state: DatabaseRowState::Clean,
            }],
            estimated_bytes: 0,
        });
        grid.added_rows = vec![
            DatabaseGridRow {
                absolute_index: usize::MAX,
                cells: Vec::new(),
                xmin: None,
                state: DatabaseRowState::Added,
            },
            DatabaseGridRow {
                absolute_index: usize::MAX - 1,
                cells: Vec::new(),
                xmin: None,
                state: DatabaseRowState::Added,
            },
        ];

        grid.select_row(0, false, false);
        grid.select_row(usize::MAX - 1, true, false);

        assert_eq!(
            grid.selection.selected_rows,
            vec![0, usize::MAX - 1, usize::MAX]
        );
        assert_eq!(
            grid.row_indices_between(0, usize::MAX - 1),
            vec![0, usize::MAX, usize::MAX - 1]
        );
    }

    #[test]
    fn unknown_count_logical_rows_include_loaded_extent_and_added_rows() {
        let mut grid = grid();
        grid.view.limit = 100;
        grid.insert_chunk(DatabaseTableChunk {
            generation: DatabaseGeneration(1),
            chunk_index: 0,
            rows: vec![DatabaseGridRow {
                absolute_index: 8,
                cells: Vec::new(),
                xmin: None,
                state: DatabaseRowState::Clean,
            }],
            estimated_bytes: 0,
        });
        grid.added_rows.push(DatabaseGridRow {
            absolute_index: 9,
            cells: Vec::new(),
            xmin: None,
            state: DatabaseRowState::Added,
        });
        assert_eq!(grid.logical_row_count(), 10);
    }
}
