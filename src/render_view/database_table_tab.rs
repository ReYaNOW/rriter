use crate::renderer::Renderer;
use crate::ui_system::{UiClipRect, UiId, UiRegistry};
use crate::widgets::{ButtonStyle, ButtonView, IconType};
use glow::HasContext;

const TABLE_TOOLBAR_H: f32 = 86.0;
const TABLE_FILTER_H: f32 = 54.0;
const ROW_GUTTER_W: f32 = 62.0;
const SCROLLBAR_W: f32 = 12.0;
const TABLE_CELL_TEXT_SCALE: f32 = 0.9;
const TABLE_HEADER_TEXT_SCALE: f32 = 0.84;

#[derive(Clone, Copy, Debug, PartialEq)]
struct DatabasePopupPlacement {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    scale: f32,
}

fn database_enum_option_text_layout(
    popup_x: f32,
    popup_w: f32,
    option_y: f32,
    option_h: f32,
    scale: f32,
) -> (f32, f32, f32) {
    (
        popup_x + (7.0 * scale).round(),
        Renderer::tree_row_text_y(option_y, option_h, scale),
        (popup_w - 14.0 * scale).max(4.0),
    )
}

#[allow(clippy::too_many_arguments)]
fn database_popup_placement(
    anchor_x: f32,
    anchor_y: f32,
    _anchor_w: f32,
    anchor_h: f32,
    desired_w: f32,
    desired_h: f32,
    bounds_x: f32,
    bounds_y: f32,
    bounds_w: f32,
    bounds_h: f32,
    base_scale: f32,
) -> DatabasePopupPlacement {
    let fit_scale = (bounds_w / desired_w.max(1.0))
        .min(bounds_h / desired_h.max(1.0))
        .min(1.0)
        .max(0.1);
    let scale = base_scale * fit_scale;
    let w = (desired_w * fit_scale).min(bounds_w).max(1.0).round();
    let h = (desired_h * fit_scale).min(bounds_h).max(1.0).round();
    let x = anchor_x.clamp(bounds_x, (bounds_x + bounds_w - w).max(bounds_x));
    let below = anchor_y + anchor_h;
    let y = if below + h <= bounds_y + bounds_h {
        below
    } else {
        (anchor_y - h).max(bounds_y)
    };
    DatabasePopupPlacement {
        x: x.round(),
        y: y.round(),
        w,
        h,
        scale,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DatabaseEnumPage {
    start: usize,
    end: usize,
    previous: bool,
    next: bool,
}

fn database_enum_page(option_count: usize, requested_start: usize, visible_rows: usize) -> DatabaseEnumPage {
    if option_count == 0 || visible_rows == 0 {
        return DatabaseEnumPage {
            start: 0,
            end: 0,
            previous: false,
            next: false,
        };
    }
    let page_size = visible_rows.max(1);
    let max_start = option_count.saturating_sub(page_size);
    let start = requested_start.min(max_start);
    let end = start.saturating_add(page_size).min(option_count);
    DatabaseEnumPage {
        start,
        end,
        previous: start > 0,
        next: end < option_count,
    }
}
fn database_table_unavailable_message(
    state: &crate::app::database::DatabaseTableTabState,
) -> Option<&str> {
    if state.loading || state.metadata.is_some() {
        None
    } else {
        Some(state.unavailable_text.text())
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_database_table_tab(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        meta: &crate::app::database::DatabaseTableTabMeta,
        state: &crate::app::database::DatabaseTableTabState,
        ui_registry: &mut UiRegistry,
        mx: f32,
        my: f32,
        blink_alpha: f32,
    ) {
        self.push_rect(x, y, w, h, self.theme.bg);
        ui_registry.register_blocker(UiId::DatabaseTableBody, x, y, w, h, mx, my);
        if state.loading {
            self.draw_string_scaled_pixel_snapped(
                "Загрузка структуры таблицы…",
                x + 24.0 * s,
                y + 42.0 * s,
                self.theme.line_num,
                0.9,
            );
            return;
        }
        if let Some(message) = database_table_unavailable_message(state) {
            let card_w = (w - 24.0 * s).clamp(1.0, 720.0 * s).round();
            let card_h = (108.0 * s).round();
            let card_x = (x + (w - card_w) * 0.5).round();
            let card_y = (y + (h - card_h) * 0.5).round();
            self.push_rounded_rect_border(
                card_x,
                card_y,
                card_w,
                card_h,
                (6.0 * s).round(),
                1.0,
                [0.35, 0.37, 0.46, 0.9],
                [0.085, 0.09, 0.115, 0.98],
            );
            self.draw_string_scaled_pixel_snapped(
                "Таблица недоступна",
                card_x + (18.0 * s).round(),
                card_y + (34.0 * s).round(),
                self.theme.fg,
                0.92,
            );
            let text_x = card_x + (18.0 * s).round();
            let text_y = card_y + (45.0 * s).round();
            let text_w = (card_w - 36.0 * s).max(40.0).round();
            let text_h = (44.0 * s).round();
            ui_registry.register_default_cursor_text_region(
                UiId::DatabaseTableUnavailableText,
                text_x,
                text_y,
                text_w,
                text_h,
                mx,
                my,
            );
            let text_scale = 0.84;
            let scroll_x = crate::app::file_tree::file_tree_name_input_scroll_x(
                message,
                state.unavailable_text.cursor,
                text_w,
                |ch| {
                    self.get_ui_glyph(ch)
                        .map(|glyph| Renderer::snapped_text_advance(glyph.advance, text_scale))
                        .unwrap_or_else(|| (8.0 * text_scale).round().max(1.0))
                },
            );
            self.draw_one_line_selectable_text(
                message,
                state.unavailable_text.cursor,
                state.unavailable_text.selection_anchor,
                false,
                false,
                text_x,
                text_y,
                text_w,
                text_h,
                scroll_x,
                0.0,
                text_scale,
                if state.error.is_some() {
                    [0.98, 0.67, 0.69, 1.0]
                } else {
                    self.theme.line_num
                },
                0.0,
                0.0,
            );
            return;
        }
        let Some(metadata) = state.metadata.as_ref() else {
            return;
        };

        self.draw_database_table_toolbar(x, y, w, s, meta, state, ui_registry, mx, my);
        self.draw_database_table_filters(
            x,
            y + TABLE_TOOLBAR_H * s,
            w,
            s,
            state,
            ui_registry,
            mx,
            my,
            blink_alpha,
        );
        let grid_y = y + (TABLE_TOOLBAR_H + TABLE_FILTER_H) * s;
        let grid_h = (h - (TABLE_TOOLBAR_H + TABLE_FILTER_H) * s).max(0.0);
        self.draw_database_grid(
            x,
            grid_y,
            w,
            grid_h,
            s,
            metadata,
            state,
            ui_registry,
            mx,
            my,
            blink_alpha,
        );
        if let Some((target, error)) = state.grid.filter_error.as_ref() {
            let id = match target {
                crate::app::database::DatabaseTableInputTarget::Where => UiId::DatabaseTableWhereInput,
                crate::app::database::DatabaseTableInputTarget::OrderBy => UiId::DatabaseTableOrderInput,
                crate::app::database::DatabaseTableInputTarget::Cell => UiId::DatabaseTableCellEditor,
            };
            if let Some(rect) = ui_registry.rect_for(id) {
                draw_database_table_error_hint(self, rect, error, x, y, w, h, s);
            }
        } else if let Some(notice) = state.active_notice() {
            let banner_h = (36.0 * s).round();
            let banner_y = (grid_y + 6.0 * s).round();
            self.push_rounded_rect_border(
                x + (12.0 * s).round(),
                banner_y,
                (w - 24.0 * s).max(40.0),
                banner_h,
                (4.0 * s).round(),
                1.0,
                [0.95, 0.66, 0.22, 0.85],
                [0.18, 0.12, 0.04, 0.96],
            );
            self.draw_tree_label_clipped(
                notice,
                x + (22.0 * s).round(),
                Self::tree_row_text_y(banner_y, banner_h, s),
                (w - 44.0 * s).max(20.0),
                [1.0, 0.82, 0.42, 1.0],
                0.82,
                &mut String::new(),
            );
        } else if let Some(error) = state.error.as_deref() {
            let banner_h = (32.0 * s).round();
            let banner_y = (grid_y + 6.0 * s).round();
            self.push_rounded_rect_border(
                x + (12.0 * s).round(),
                banner_y,
                (w - 24.0 * s).max(40.0),
                banner_h,
                (4.0 * s).round(),
                1.0,
                [0.95, 0.38, 0.42, 0.85],
                [0.20, 0.07, 0.09, 0.96],
            );
            self.draw_tree_label_clipped(
                error,
                x + (22.0 * s).round(),
                Self::tree_row_text_y(banner_y, banner_h, s),
                (w - 44.0 * s).max(20.0),
                [0.98, 0.72, 0.74, 1.0],
                0.82,
                &mut String::new(),
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_database_table_toolbar(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        s: f32,
        meta: &crate::app::database::DatabaseTableTabMeta,
        state: &crate::app::database::DatabaseTableTabState,
        ui: &mut UiRegistry,
        mx: f32,
        my: f32,
    ) {
        let toolbar_h = (TABLE_TOOLBAR_H * s).round();
        let x = x.round();
        let y = y.round();
        let w = w.round().max(0.0);
        self.push_rect(x, y, w, toolbar_h, [0.105, 0.11, 0.14, 1.0]);
        let title = format!("{} / public.{}", meta.database_name, meta.table_name);
        let page_text = database_table_page_status(state);
        let page_w = self.measure_ui_width(&page_text, 0.82).round();
        let show_page = w >= 230.0 * s;
        let page_reserve = if show_page { page_w + 32.0 * s } else { 16.0 * s };
        let mut title_scratch = String::new();
        self.draw_tree_label_clipped(
            &title,
            x + (10.0 * s).round(),
            Self::tree_row_text_y(y, (34.0 * s).round(), s),
            (w - page_reserve).max(1.0),
            [0.66, 0.69, 0.78, 1.0],
            0.88,
            &mut title_scratch,
        );
        if show_page {
            self.draw_string_scaled_pixel_snapped(
                &page_text,
                (x + w - page_w - 12.0 * s).round(),
                Self::tree_row_text_y(y, (34.0 * s).round(), s),
                self.theme.line_num,
                0.82,
            );
        }

        let editable = state.metadata.as_ref().is_some_and(|metadata| metadata.editable);
        let dirty = state.grid.dirty();
        let button_y = y + (39.0 * s).round();
        let button_h = 38.0 * s;
        let gap = (4.0 * s).round();
        let mut bx = x + (8.0 * s).round();

        let wide = w >= 920.0 * s;
        let medium = w >= 520.0 * s;
        let narrow = w >= 340.0 * s;
        let nav_w = if wide { 40.0 * s } else { 36.0 * s };
        let nav_gap = gap;
        let nav_items: &[(UiId, &str, bool)] = if medium || wide {
            &[
                (UiId::DatabaseTablePageFirst, "≪", state.grid.view.current_page > 0),
                (UiId::DatabaseTablePagePrevious, "‹", state.grid.view.current_page > 0),
                (UiId::DatabaseTablePageNext, "›", database_table_has_next(state)),
                (
                    UiId::DatabaseTablePageLast,
                    "≫",
                    state.grid.count.is_some() && database_table_has_next(state),
                ),
            ]
        } else {
            &[
                (UiId::DatabaseTablePagePrevious, "‹", state.grid.view.current_page > 0),
                (UiId::DatabaseTablePageNext, "›", database_table_has_next(state)),
            ]
        };
        let limit_w = if wide || medium { 84.0 * s } else { 0.0 };
        let nav_total = nav_items.len() as f32 * nav_w
            + nav_items.len().saturating_sub(1) as f32 * nav_gap
            + if limit_w > 0.0 { nav_gap + limit_w } else { 0.0 };
        let nav_start = (x + w - 8.0 * s - nav_total).max(x + 8.0 * s).round();
        let action_end = (nav_start - gap).max(x + 8.0 * s);

        let action_w = if wide { 0.0 } else if medium { 40.0 * s } else { 36.0 * s };
        let actions: &[(UiId, &str, IconType, bool, f32)] = if wide {
            &[
                (UiId::DatabaseTableAddRow, "Добавить", IconType::Plus, editable, 108.0),
                (UiId::DatabaseTableDeleteRows, "Удалить", IconType::GitMinus, editable, 106.0),
                (UiId::DatabaseTableUndo, "Отменить", IconType::Rollback, dirty, 112.0),
                (UiId::DatabaseTableSave, "Сохранить", IconType::Save, dirty, 124.0),
                (UiId::DatabaseTablePreview, "SQL", IconType::Eye, dirty, 82.0),
                (UiId::DatabaseTableRefresh, "", IconType::Reload, true, 48.0),
            ]
        } else if medium {
            &[
                (UiId::DatabaseTableAddRow, "", IconType::Plus, editable, 0.0),
                (UiId::DatabaseTableDeleteRows, "", IconType::GitMinus, editable, 0.0),
                (UiId::DatabaseTableUndo, "", IconType::Rollback, dirty, 0.0),
                (UiId::DatabaseTableSave, "", IconType::Save, dirty, 0.0),
                (UiId::DatabaseTablePreview, "", IconType::Eye, dirty, 0.0),
                (UiId::DatabaseTableRefresh, "", IconType::Reload, true, 0.0),
            ]
        } else if narrow {
            &[
                (UiId::DatabaseTableAddRow, "", IconType::Plus, editable, 0.0),
                (UiId::DatabaseTableDeleteRows, "", IconType::GitMinus, editable, 0.0),
                (UiId::DatabaseTableUndo, "", IconType::Rollback, dirty, 0.0),
                (UiId::DatabaseTableSave, "", IconType::Save, dirty, 0.0),
                (UiId::DatabaseTableRefresh, "", IconType::Reload, true, 0.0),
            ]
        } else {
            &[
                (UiId::DatabaseTableAddRow, "", IconType::Plus, editable, 0.0),
                (UiId::DatabaseTableSave, "", IconType::Save, dirty, 0.0),
                (UiId::DatabaseTableRefresh, "", IconType::Reload, true, 0.0),
            ]
        };
        for &(id, text, icon, active, normal_w) in actions {
            let width = if wide { normal_w * s } else { action_w };
            if bx + width > action_end {
                break;
            }
            draw_database_table_button(
                self, ui, id, bx, button_y, width, button_h, text, Some(icon), active, mx, my, s,
            );
            bx += width + gap;
        }

        let mut nav_x = nav_start;
        for &(id, label, active) in nav_items {
            if nav_x + nav_w > x + w - 8.0 * s {
                break;
            }
            draw_database_table_nav_button(
                self, ui, id, nav_x, button_y, nav_w, button_h, label, active, mx, my, s,
            );
            nav_x += nav_w + nav_gap;
        }
        if limit_w > 0.0 && nav_x + limit_w <= x + w - 8.0 * s {
            draw_database_table_button(
                self,
                ui,
                UiId::DatabaseTableLimit,
                nav_x,
                button_y,
                limit_w,
                button_h,
                &format!("{} строк", state.grid.view.limit),
                None,
                true,
                mx,
                my,
                s,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_database_table_filters(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        s: f32,
        state: &crate::app::database::DatabaseTableTabState,
        ui: &mut UiRegistry,
        mx: f32,
        my: f32,
        blink_alpha: f32,
    ) {
        let w = w.max(0.0);
        self.push_rect(
            x.round(),
            y.round(),
            w.round(),
            (TABLE_FILTER_H * s).round(),
            [0.085, 0.09, 0.115, 1.0],
        );
        let pad = 8.0 * s;
        let gap = 8.0 * s;
        let input_y = y + 8.0 * s;
        let input_h = 38.0 * s;
        let wide = w >= 620.0 * s;
        let label_w = if wide { 90.0 * s } else { 24.0 * s };
        let available = (w - pad * 2.0 - gap - label_w * 2.0).max(0.0);
        let field_w = (available * 0.5).max(0.0);
        let where_label = if wide { "WHERE" } else { "W" };
        let order_label = if wide { "ORDER BY" } else { "O" };
        self.draw_string_scaled_pixel_snapped(
            where_label,
            x + pad,
            Self::tree_row_text_y(y, TABLE_FILTER_H * s, s),
            self.theme.line_num,
            0.86,
        );
        let where_x = x + pad + label_w;
        if field_w >= 1.0 {
            draw_database_table_input(
                self,
                ui,
                UiId::DatabaseTableWhereInput,
                where_x,
                input_y,
                field_w,
                input_h,
                &state.grid.where_input,
                state.grid.focused_input
                    == Some(crate::app::database::DatabaseTableInputTarget::Where),
                mx,
                my,
                s,
                blink_alpha,
            );
        }
        let order_label_x = where_x + field_w + gap;
        self.draw_string_scaled_pixel_snapped(
            order_label,
            order_label_x,
            Self::tree_row_text_y(y, TABLE_FILTER_H * s, s),
            self.theme.line_num,
            0.86,
        );
        let order_x = order_label_x + label_w;
        if field_w >= 1.0 {
            draw_database_table_input(
                self,
                ui,
                UiId::DatabaseTableOrderInput,
                order_x,
                input_y,
                field_w,
                input_h,
                &state.grid.order_by_input,
                state.grid.focused_input
                    == Some(crate::app::database::DatabaseTableInputTarget::OrderBy),
                mx,
                my,
                s,
                blink_alpha,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_database_grid(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        metadata: &crate::app::database::DatabaseTableMetadata,
        state: &crate::app::database::DatabaseTableTabState,
        ui: &mut UiRegistry,
        mx: f32,
        my: f32,
        blink_alpha: f32,
    ) {
        use crate::app::database::{DATABASE_GRID_HEADER_HEIGHT, DATABASE_GRID_ROW_HEIGHT};
        let x = x.round();
        let y = y.round();
        let w = w.round().max(0.0);
        let h = h.round().max(0.0);
        let track = (SCROLLBAR_W * s).round().max(10.0);
        let header_h = (DATABASE_GRID_HEADER_HEIGHT * s).round();
        let row_h = (DATABASE_GRID_ROW_HEIGHT * s).round();
        let gutter_w = (ROW_GUTTER_W * s).round();
        let content_w = (state.grid.content_width(metadata) * s).round();
        let total_rows_h = state.grid.logical_row_count() as f32 * row_h;
        let layout = crate::app::database::database_grid_layout(
            x,
            y,
            w,
            h,
            gutter_w,
            track,
            header_h,
            content_w,
            total_rows_h,
        );
        let viewport = layout.viewport;
        let data_w = viewport.data_w;
        let body_h = viewport.body_h;
        let rows_h = viewport.rows_h;
        let body_w = viewport.body_w;
        let data_x = layout.header_rect.x;
        let rows_y = layout.body_rect.y;

        ui.register_blocker(UiId::DatabaseTableGridBody, x, y, body_w, body_h, mx, my);
        self.push_rect(x, y, body_w, body_h, [0.075, 0.078, 0.098, 1.0]);
        self.push_rect(x, y, gutter_w, body_h, [0.095, 0.10, 0.125, 1.0]);
        self.push_rect(x, y, body_w, header_h, [0.125, 0.13, 0.16, 1.0]);

        let visible_columns = database_visible_columns(metadata, &state.grid, data_w / s);
        let scroll_x = (state.grid.scroll_x.current * s).round();
        let header_baseline = Self::tree_row_text_y(y, header_h, s).round();
        let mut scratch = String::new();
        let header_clip = UiClipRect::new(
            layout.header_rect.x,
            layout.header_rect.y,
            layout.header_rect.w,
            layout.header_rect.h,
        );
        let body_clip = UiClipRect::new(
            layout.body_rect.x,
            layout.body_rect.y,
            layout.body_rect.w,
            layout.body_rect.h,
        );
        let gutter_clip = UiClipRect::new(x, rows_y, gutter_w, rows_h);

        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            self.gl.scissor(
                layout.header_rect.x as i32,
                (self.height - (layout.header_rect.y + layout.header_rect.h))
                    .round()
                    .max(0.0) as i32,
                layout.header_rect.w.round().max(0.0) as i32,
                layout.header_rect.h.round().max(0.0) as i32,
            );
        }
        for column_index in visible_columns.clone() {
            let column = &metadata.columns[column_index];
            let (column_x, column_w) = database_column_geometry(metadata, &state.grid, column_index);
            let draw_x = (data_x + column_x * s - scroll_x).round();
            let draw_w = (column_w * s).round().max(1.0);
            if draw_x + draw_w < data_x || draw_x > data_x + data_w {
                continue;
            }
            if state.grid.view.sorted_column.as_deref() == Some(column.name.as_str()) {
                self.push_rect(draw_x, y, draw_w, header_h, [0.35, 0.22, 0.52, 0.36]);
            }
            ui.register_rect_clipped(
                UiId::DatabaseTableHeader(column_index),
                draw_x,
                y,
                draw_w,
                header_h,
                header_clip,
                mx,
                my,
            );
            self.draw_tree_label_clipped(
                &database_column_header(column, state),
                draw_x + (8.0 * s).round(),
                header_baseline,
                (draw_w - 16.0 * s).max(4.0),
                self.theme.fg,
                TABLE_HEADER_TEXT_SCALE,
                &mut scratch,
            );
            let divider_x = (draw_x + draw_w - 3.0 * s).round();
            ui.register_rect_clipped(
                UiId::DatabaseTableColumnResize(column_index),
                divider_x,
                y,
                (6.0 * s).round(),
                header_h,
                header_clip,
                mx,
                my,
            );
            self.push_rect((draw_x + draw_w - 1.0).round(), y, 1.0, body_h, [1.0, 1.0, 1.0, 0.09]);
        }
        self.flush();
        unsafe { self.gl.disable(glow::SCISSOR_TEST) };

        let server_rows = database_server_rows_on_page(state);
        let visible_rows = crate::app::database::database_grid_visible_row_range(
            state.grid.scroll_y.current * s,
            row_h,
            rows_h,
            state.grid.logical_row_count(),
        );
        let page_base = state.grid.view.current_page.saturating_mul(state.grid.view.limit);
        let scroll_y = (state.grid.scroll_y.current * s).round();
        let mut editor_popup: Option<(f32, f32, f32, usize)> = None;

        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            self.gl.scissor(
                layout.body_rect.x as i32,
                (self.height - (layout.body_rect.y + layout.body_rect.h))
                    .round()
                    .max(0.0) as i32,
                layout.body_rect.w.round().max(0.0) as i32,
                layout.body_rect.h.round().max(0.0) as i32,
            );
        }
        for relative in visible_rows.clone() {
            let row_y = (rows_y + relative as f32 * row_h - scroll_y).round();
            if row_y + row_h <= rows_y || row_y >= rows_y + rows_h {
                continue;
            }
            let row = if relative < server_rows {
                state.grid.row(page_base.saturating_add(relative))
            } else {
                state.grid.added_rows.get(relative.saturating_sub(server_rows))
            };
            let Some(row) = row else { continue; };
            let bg = match row.state {
                crate::app::database::DatabaseRowState::Added => [0.10, 0.28, 0.16, 0.72],
                crate::app::database::DatabaseRowState::Deleted => [0.28, 0.28, 0.30, 0.58],
                crate::app::database::DatabaseRowState::Clean => {
                    if relative % 2 == 0 { [0.08, 0.084, 0.105, 1.0] } else { [0.095, 0.098, 0.12, 1.0] }
                }
            };
            self.push_rect(data_x, row_y, data_w, row_h, bg);
            let row_baseline = Self::tree_row_text_y(row_y, row_h, s).round();
            for column_index in visible_columns.clone() {
                let Some(cell) = row.cells.get(column_index) else { continue; };
                let (column_x, column_w) = database_column_geometry(metadata, &state.grid, column_index);
                let draw_x = (data_x + column_x * s - scroll_x).round();
                let draw_w = (column_w * s).round().max(1.0);
                if state.grid.selection.contains_cell(row.absolute_index, column_index) {
                    self.push_rect(draw_x, row_y, draw_w, row_h, [0.42, 0.25, 0.63, 0.48]);
                }
                if cell.dirty {
                    self.push_rect(draw_x, row_y + row_h - (2.0 * s).round(), draw_w, (2.0 * s).round().max(1.0), [0.32, 0.90, 0.48, 1.0]);
                }
                ui.register_rect_clipped(
                    UiId::DatabaseTableCell(row.absolute_index, column_index),
                    draw_x,
                    row_y,
                    draw_w,
                    row_h,
                    body_clip,
                    mx,
                    my,
                );
                self.draw_tree_label_clipped(
                    &cell.value.display_text(),
                    draw_x + (7.0 * s).round(),
                    row_baseline,
                    (draw_w - 14.0 * s).max(4.0),
                    if matches!(cell.value, crate::app::database::DatabaseCellValue::Null | crate::app::database::DatabaseCellValue::Default) {
                        [0.52, 0.55, 0.62, 1.0]
                    } else {
                        self.theme.fg
                    },
                    TABLE_CELL_TEXT_SCALE,
                    &mut scratch,
                );
                if let Some(editor) = state.grid.cell_editor.as_ref()
                    && editor.position.row == row.absolute_index
                    && editor.position.column == column_index
                {
                    draw_database_table_input(
                        self,
                        ui,
                        UiId::DatabaseTableCellEditor,
                        draw_x,
                        row_y,
                        draw_w.max(20.0),
                        row_h.max(20.0),
                        &editor.input,
                        true,
                        mx,
                        my,
                        s,
                        blink_alpha,
                    );
                    if matches!(
                        editor.kind,
                        crate::app::database::DatabaseCellEditorKind::DateTime
                            | crate::app::database::DatabaseCellEditorKind::Enum
                    ) {
                        editor_popup = Some((draw_x, row_y, draw_w, column_index));
                    }
                }
            }
        }
        self.flush();
        unsafe { self.gl.disable(glow::SCISSOR_TEST) };

        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            self.gl.scissor(
                data_x as i32,
                (self.height - (rows_y + rows_h)).round().max(0.0) as i32,
                data_w.round().max(0.0) as i32,
                rows_h.round().max(0.0) as i32,
            );
        }
        for column_index in visible_columns.clone() {
            let (column_x, column_w) = database_column_geometry(metadata, &state.grid, column_index);
            let divider_x = (data_x + (column_x + column_w) * s - scroll_x - 1.0).round();
            self.push_rect(divider_x, rows_y, 1.0, rows_h, [0.52, 0.55, 0.62, 0.20]);
        }
        self.flush();
        unsafe { self.gl.disable(glow::SCISSOR_TEST) };

        if let Some((anchor_x, row_y, anchor_w, column_index)) = editor_popup
            && let Some(editor) = state.grid.cell_editor.as_ref()
        {
            match editor.kind {
                crate::app::database::DatabaseCellEditorKind::DateTime => {
                    let is_time_only = metadata.columns[column_index].type_kind
                        == crate::app::database::DatabaseTypeKind::Time;
                    let (desired_w, desired_h) = if is_time_only {
                        (142.0 * s, 38.0 * s)
                    } else {
                        database_date_picker_size(s)
                    };
                    let placement = database_popup_placement(
                        anchor_x,
                        row_y,
                        anchor_w,
                        row_h,
                        desired_w,
                        desired_h,
                        data_x,
                        rows_y,
                        data_w,
                        rows_h,
                        s,
                    );
                    draw_database_date_picker(
                        self,
                        ui,
                        placement.x,
                        placement.y,
                        editor,
                        &metadata.columns[column_index],
                        mx,
                        my,
                        placement.scale,
                    );
                }
                crate::app::database::DatabaseCellEditorKind::Enum => {
                    let options = &metadata.columns[column_index].enum_values;
                    let base_option_h = (28.0 * s).round().max(1.0);
                    let max_rows = (rows_h / base_option_h).floor().max(1.0) as usize;
                    let page_size = if options.len() > max_rows {
                        max_rows.saturating_sub(2).max(1)
                    } else {
                        max_rows
                    };
                    let page = database_enum_page(options.len(), editor.enum_index, page_size);
                    let control_rows = page.previous as usize + page.next as usize;
                    let row_count = page.end.saturating_sub(page.start) + control_rows;
                    let desired_h = base_option_h * row_count.max(1) as f32;
                    let desired_w = anchor_w.max((150.0 * s).round()).min(data_w);
                    let placement = database_popup_placement(
                        anchor_x,
                        row_y,
                        anchor_w,
                        row_h,
                        desired_w,
                        desired_h,
                        data_x,
                        rows_y,
                        data_w,
                        rows_h,
                        s,
                    );
                    let popup_x = placement.x;
                    let popup_y = placement.y;
                    let popup_w = placement.w;
                    let popup_h = placement.h;
                    let option_h = (28.0 * placement.scale).round().max(1.0);
                    let popup_clip = UiClipRect::new(data_x, rows_y, data_w, rows_h);
                    self.push_rounded_rect_border(
                        popup_x,
                        popup_y,
                        popup_w,
                        popup_h.max(option_h),
                        (4.0 * s).round(),
                        1.0,
                        [0.32, 0.34, 0.42, 1.0],
                        [0.095, 0.10, 0.13, 1.0],
                    );
                    let mut visual_row = 0usize;
                    if page.previous {
                        let option_y = popup_y;
                        let hovered = ui.register_rect_clipped(
                            UiId::DatabaseTableEnumPreviousPage,
                            popup_x,
                            option_y,
                            popup_w,
                            option_h,
                            popup_clip,
                            mx,
                            my,
                        );
                        if hovered {
                            self.push_rect(
                                popup_x + 1.0,
                                option_y + 1.0,
                                popup_w - 2.0,
                                option_h - 2.0,
                                [0.20, 0.18, 0.29, 1.0],
                            );
                        }
                        let (text_x, text_y, text_w) = database_enum_option_text_layout(
                            popup_x,
                            popup_w,
                            option_y,
                            option_h,
                            placement.scale,
                        );
                        self.draw_tree_label_clipped(
                            "↑ Предыдущие",
                            text_x,
                            text_y,
                            text_w,
                            self.theme.line_num,
                            0.84,
                            &mut scratch,
                        );
                        visual_row += 1;
                    }
                    for option_index in page.start..page.end {
                        let option = &options[option_index];
                        let option_y = (popup_y + visual_row as f32 * option_h).round();
                        if mx >= popup_x && mx <= popup_x + popup_w && my >= option_y && my <= option_y + option_h {
                            self.push_rect(popup_x + 1.0, option_y + 1.0, popup_w - 2.0, option_h - 2.0, [0.20, 0.18, 0.29, 1.0]);
                        }
                        ui.register_rect_clipped(
                            UiId::DatabaseTableEnumOption(option_index),
                            popup_x,
                            option_y,
                            popup_w,
                            option_h,
                            popup_clip,
                            mx,
                            my,
                        );
                        let (text_x, text_y, text_w) = database_enum_option_text_layout(
                            popup_x,
                            popup_w,
                            option_y,
                            option_h,
                            placement.scale,
                        );
                        self.draw_tree_label_clipped(
                            option,
                            text_x,
                            text_y,
                            text_w,
                            self.theme.fg,
                            0.84,
                            &mut scratch,
                        );
                        visual_row += 1;
                    }
                    if page.next {
                        let option_y = (popup_y + visual_row as f32 * option_h).round();
                        let hovered = ui.register_rect_clipped(
                            UiId::DatabaseTableEnumNextPage,
                            popup_x,
                            option_y,
                            popup_w,
                            option_h,
                            popup_clip,
                            mx,
                            my,
                        );
                        if hovered {
                            self.push_rect(
                                popup_x + 1.0,
                                option_y + 1.0,
                                popup_w - 2.0,
                                option_h - 2.0,
                                [0.20, 0.18, 0.29, 1.0],
                            );
                        }
                        let (text_x, text_y, text_w) = database_enum_option_text_layout(
                            popup_x,
                            popup_w,
                            option_y,
                            option_h,
                            placement.scale,
                        );
                        self.draw_tree_label_clipped(
                            "Следующие ↓",
                            text_x,
                            text_y,
                            text_w,
                            self.theme.line_num,
                            0.84,
                            &mut scratch,
                        );
                    }
                }
                _ => {}
            }
        }

        self.push_rect(x, y, gutter_w, header_h, [0.115, 0.12, 0.15, 1.0]);
        self.draw_string_scaled_pixel_snapped(
            "#",
            x + (18.0 * s).round(),
            header_baseline,
            self.theme.line_num,
            0.82,
        );
        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            self.gl.scissor(
                x as i32,
                (self.height - (rows_y + rows_h)).round().max(0.0) as i32,
                gutter_w.round().max(0.0) as i32,
                rows_h.round().max(0.0) as i32,
            );
        }
        for relative in visible_rows {
            let row_y = (rows_y + relative as f32 * row_h - scroll_y).round();
            if row_y + row_h <= rows_y || row_y >= rows_y + rows_h { continue; }
            let absolute = if relative < server_rows {
                page_base.saturating_add(relative)
            } else if let Some(row) = state.grid.added_rows.get(relative.saturating_sub(server_rows)) {
                row.absolute_index
            } else { continue; };
            if state.grid.selection.contains_row(absolute) {
                self.push_rect(x, row_y, gutter_w, row_h, [0.42, 0.25, 0.63, 0.55]);
            }
            ui.register_rect_clipped(
                UiId::DatabaseGridRow(absolute),
                x,
                row_y,
                gutter_w,
                row_h,
                gutter_clip,
                mx,
                my,
            );
            let display_number = page_base.saturating_add(relative).saturating_add(1);
            self.draw_string_scaled_pixel_snapped(
                &display_number.to_string(),
                x + (8.0 * s).round(),
                Self::tree_row_text_y(row_y, row_h, s).round(),
                self.theme.line_num,
                0.78,
            );
        }
        self.flush();
        unsafe { self.gl.disable(glow::SCISSOR_TEST) };

        draw_database_table_scrollbars(
            self,
            ui,
            &layout,
            metadata,
            state,
            mx,
            my,
            s,
        );
        if let Some(reason) = metadata.read_only_reason.as_deref() {
            let banner_h = (28.0 * s).round();
            let banner_y = (y + body_h - banner_h).round();
            self.push_rect(x, banner_y, body_w, banner_h, [0.30, 0.18, 0.06, 0.94]);
            self.draw_string_scaled_pixel_snapped(
                reason,
                x + (10.0 * s).round(),
                Self::tree_row_text_y(banner_y, banner_h, s).round(),
                [0.98, 0.72, 0.30, 1.0],
                0.8,
            );
        }
        if state.grid.refreshing
            && state.grid.refresh_started.is_some_and(|started| {
                started.elapsed() >= std::time::Duration::from_millis(70)
            })
        {
            draw_database_refresh_overlay(self, x, y, body_w, body_h, s);
        }
    }

}

#[allow(clippy::too_many_arguments)]
fn draw_database_table_button(
    renderer: &mut Renderer,
    ui: &mut UiRegistry,
    id: UiId,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    text: &str,
    icon: Option<IconType>,
    active: bool,
    mx: f32,
    my: f32,
    s: f32,
) {
    let button = ButtonView {
        x: x.round(),
        y: y.round(),
        w: w.round(),
        h: h.round(),
        text,
        icon,
        text_scale: 0.82,
        icon_size: (22.0 * s).round(),
    };
    if active {
        if id == UiId::DatabaseTablePreview {
            button.render_styled(
                renderer,
                mx,
                my,
                s,
                false,
                ButtonStyle {
                    border: [0.24, 0.58, 0.86, 0.75],
                    background: [0.10, 0.15, 0.21, 1.0],
                    hover_background: [0.13, 0.23, 0.32, 1.0],
                    pressed_background: [0.16, 0.31, 0.44, 1.0],
                    content: [0.35, 0.72, 0.98, 1.0],
                },
            );
            ui.register_rect(id, button.x, button.y, button.w, button.h, mx, my);
        } else {
            ui.register_button_view(id, button, renderer, mx, my, s, false);
        }
    } else {
        button.render_disabled(renderer, s);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_database_table_nav_button(
    renderer: &mut Renderer,
    ui: &mut UiRegistry,
    id: UiId,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    text: &str,
    active: bool,
    mx: f32,
    my: f32,
    s: f32,
) {
    let x = x.round();
    let y = y.round();
    let w = w.round();
    let h = h.round();
    let hovered = active && ui.register_rect(id, x, y, w, h, mx, my);
    renderer.push_rounded_rect_border(
        x,
        y,
        w,
        h,
        (4.0 * s).round(),
        (1.0 * s).round().max(1.0),
        if hovered { renderer.theme.sel } else { [1.0, 1.0, 1.0, 0.10] },
        if active { [0.15, 0.16, 0.20, 1.0] } else { [0.10, 0.105, 0.13, 1.0] },
    );
    let text_scale = 1.08;
    let text_w = renderer.measure_ui_width(text, text_scale);
    renderer.draw_string_scaled_pixel_snapped(
        text,
        (x + (w - text_w) * 0.5).round(),
        Renderer::tree_row_text_y(y, h, s),
        if active { renderer.theme.fg } else { [0.40, 0.42, 0.48, 1.0] },
        text_scale,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_database_table_input(
    renderer: &mut Renderer,
    ui: &mut UiRegistry,
    id: UiId,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    input: &crate::app::database::DatabaseDialogInput,
    focused: bool,
    mx: f32,
    my: f32,
    s: f32,
    blink_alpha: f32,
) {
    let x = x.round();
    let y = y.round();
    let w = w.round().max(1.0);
    let h = h.round().max(1.0);
    ui.register_text_input(id, x, y, w, h, mx, my);
    let cell_editor = id == UiId::DatabaseTableCellEditor;
    let text_scale = crate::app::database::DATABASE_TABLE_INPUT_TEXT_SCALE;
    let padding = if cell_editor { (8.0 * s).round() } else { (10.0 * s).round() };
    let visible_width = (w - padding * 2.0).max(1.0);
    let scroll_x = crate::app::file_tree::file_tree_name_input_scroll_x(
        input.text(),
        input.cursor,
        visible_width,
        |ch| {
            renderer
                .get_ui_glyph(ch)
                .map(|glyph| Renderer::snapped_text_advance(glyph.advance, text_scale))
                .unwrap_or_else(|| (8.0 * text_scale).round().max(1.0))
        },
    );
    renderer.draw_one_line_input_with_chrome(
        input.text(),
        input.cursor,
        input.selection_anchor,
        false,
        focused,
        x,
        y,
        w,
        h,
        scroll_x,
        if focused { blink_alpha } else { 0.0 },
        text_scale,
        0.0,
        padding,
        if cell_editor { 0.0 } else { (5.0 * s).round() },
    );
}

fn database_table_has_next(state: &crate::app::database::DatabaseTableTabState) -> bool {
    state.grid.can_page_next()
}

fn database_server_rows_on_page(state: &crate::app::database::DatabaseTableTabState) -> usize {
    let base = state.grid.view.current_page.saturating_mul(state.grid.view.limit);
    state.grid.count.map_or_else(|| state.grid.loaded_server_row_extent_on_page(), |count| {
        (count as usize).saturating_sub(base).min(state.grid.view.limit)
    })
}

fn database_table_page_status(state: &crate::app::database::DatabaseTableTabState) -> String {
    let base = state.grid.view.current_page.saturating_mul(state.grid.view.limit);
    let loaded = state.grid.loaded_server_row_count_on_page();
    match state.grid.count {
        Some(0) => "0 из 0".to_string(),
        Some(count) if loaded > 0 => format!("{}–{} из {}", base + 1, base + loaded, count),
        Some(count) if state.grid.loading_chunk => format!("Загрузка… · всего {count}"),
        Some(count) => format!("0 загружено · всего {count}"),
        None if state.grid.loading_count && loaded > 0 => {
            format!("Загружено {loaded} · подсчёт…")
        }
        None if state.grid.loading_count => "Подсчёт…".to_string(),
        None if loaded > 0 => format!("Загружено {loaded} · общее число неизвестно"),
        None => state
            .grid
            .count_error
            .clone()
            .unwrap_or_else(|| "общее число неизвестно".to_string()),
    }
}

fn database_date_picker_size(s: f32) -> (f32, f32) {
    (238.0 * s, 300.0 * s)
}

fn database_calendar_centered_square(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    size: f32,
) -> (f32, f32, f32, f32) {
    let size = size.min(w).min(h).round().max(1.0);
    (
        (x + (w - size) * 0.5).round(),
        (y + (h - size) * 0.5).round(),
        size,
        size,
    )
}

#[allow(clippy::too_many_arguments)]
fn draw_database_calendar_footer_button(
    renderer: &mut Renderer,
    ui: &mut UiRegistry,
    id: UiId,
    label: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    scale: f32,
    mx: f32,
    my: f32,
    s: f32,
) {
    let hovered = ui.register_rect(id, x, y, w, h, mx, my);
    if hovered {
        renderer.push_rounded_rect(
            x + 3.0,
            y + 3.0,
            w - 6.0,
            h - 6.0,
            (5.0 * s).round(),
            [0.20, 0.18, 0.29, 1.0],
        );
    }
    let text_w = renderer.measure_ui_width(label, scale).round();
    renderer.draw_string_scaled_pixel_snapped(
        label,
        (x + (w - text_w) * 0.5).round(),
        Renderer::tree_row_text_y(y, h, s),
        renderer.theme.fg,
        scale,
    );
}

fn draw_database_refresh_overlay(
    renderer: &mut Renderer,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    s: f32,
) {
    renderer.push_rect(x, y, w, h, [0.02, 0.025, 0.04, 0.34]);
    let cx = (x + w * 0.5).round();
    let cy = (y + h * 0.5).round();
    let radius = (15.0 * s).round();
    let dot = (4.0 * s).round().max(2.0);
    let phase = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| (duration.as_millis() % 800) as usize / 100);
    const OFFSETS: [(f32, f32); 8] = [
        (0.0, -1.0), (0.707, -0.707), (1.0, 0.0), (0.707, 0.707),
        (0.0, 1.0), (-0.707, 0.707), (-1.0, 0.0), (-0.707, -0.707),
    ];
    for (index, (dx, dy)) in OFFSETS.iter().copied().enumerate() {
        let distance = (index + 8 - phase) % 8;
        let alpha = 1.0 - distance as f32 * 0.09;
        renderer.push_rounded_rect(
            (cx + dx * radius - dot * 0.5).round(),
            (cy + dy * radius - dot * 0.5).round(),
            dot,
            dot,
            dot * 0.5,
            [0.42, 0.76, 1.0, alpha.clamp(0.30, 1.0)],
        );
    }
}

fn draw_database_table_error_hint(
    renderer: &mut Renderer,
    anchor: (f32, f32, f32, f32),
    error: &str,
    viewport_x: f32,
    viewport_y: f32,
    viewport_w: f32,
    viewport_h: f32,
    s: f32,
) {
    let max_w = (viewport_w - 24.0 * s).max(180.0 * s);
    let hint_w = (renderer.measure_ui_width(error, 0.74) + 24.0 * s)
        .min(max_w)
        .max(220.0 * s)
        .round();
    let hint_h = (40.0 * s).round();
    let hint_x = anchor.0.clamp(
        viewport_x + 8.0 * s,
        (viewport_x + viewport_w - hint_w - 8.0 * s).max(viewport_x + 8.0 * s),
    );
    let below = anchor.1 + anchor.3 + 4.0 * s;
    let hint_y = if below + hint_h <= viewport_y + viewport_h {
        below
    } else {
        (anchor.1 - hint_h - 4.0 * s).max(viewport_y + 4.0 * s)
    };
    renderer.push_rounded_rect_border(
        hint_x.round(),
        hint_y.round(),
        hint_w,
        hint_h,
        (5.0 * s).round(),
        1.0,
        [0.95, 0.38, 0.42, 0.95],
        [0.19, 0.06, 0.09, 0.98],
    );
    renderer.draw_tree_label_clipped(
        error,
        hint_x + (10.0 * s).round(),
        Renderer::tree_row_text_y(hint_y, hint_h, s),
        (hint_w - 20.0 * s).max(8.0),
        [0.99, 0.76, 0.78, 1.0],
        0.82,
        &mut String::new(),
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_database_date_picker(
    renderer: &mut Renderer,
    ui: &mut UiRegistry,
    x: f32,
    y: f32,
    editor: &crate::app::database::DatabaseCellEditorState,
    column: &crate::app::database::DatabaseColumnInfo,
    mx: f32,
    my: f32,
    s: f32,
) {
    use crate::app::database::DatabaseTypeKind;
    let x = x.round();
    let y = y.round();
    let is_time_only = column.type_kind == DatabaseTypeKind::Time;
    if is_time_only {
        let w = (142.0 * s).round();
        let h = (38.0 * s).round();
        let hovered = ui.register_rect(UiId::DatabaseTableDateNow, x, y, w, h, mx, my);
        renderer.push_rounded_rect_border(
            x,
            y,
            w,
            h,
            (5.0 * s).round(),
            1.0,
            if hovered { renderer.theme.sel } else { [0.32, 0.34, 0.42, 1.0] },
            if hovered { [0.18, 0.20, 0.28, 1.0] } else { [0.13, 0.14, 0.18, 1.0] },
        );
        let label = "Сейчас UTC";
        let scale = 0.86;
        let text_w = renderer.measure_ui_width(label, scale).round();
        renderer.draw_string_scaled_pixel_snapped(
            label,
            (x + (w - text_w) * 0.5).round(),
            Renderer::tree_row_text_y(y, h, s),
            renderer.theme.fg,
            scale,
        );
        return;
    }

    const MONTHS: [&str; 12] = [
        "Январь", "Февраль", "Март", "Апрель", "Май", "Июнь",
        "Июль", "Август", "Сентябрь", "Октябрь", "Ноябрь", "Декабрь",
    ];
    const WEEKDAYS: [&str; 7] = ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Вс"];
    let cell_w = (34.0 * s).round();
    let cell_h = (32.0 * s).round();
    let width = cell_w * 7.0;
    let header_h = (40.0 * s).round();
    let weekday_h = (28.0 * s).round();
    let footer_h = (40.0 * s).round();
    let height = header_h + weekday_h + cell_h * 6.0 + footer_h;
    renderer.push_rounded_rect_border(
        x,
        y,
        width,
        height,
        (6.0 * s).round(),
        1.0,
        [0.32, 0.34, 0.42, 1.0],
        [0.095, 0.10, 0.13, 1.0],
    );

    let arrow_w = (40.0 * s).round();
    let previous_hovered = ui.register_rect(
        UiId::DatabaseTableDatePreviousMonth,
        x,
        y,
        arrow_w,
        header_h,
        mx,
        my,
    );
    let next_x = x + width - arrow_w;
    let next_hovered = ui.register_rect(
        UiId::DatabaseTableDateNextMonth,
        next_x,
        y,
        arrow_w,
        header_h,
        mx,
        my,
    );
    for (button_x, hovered) in [(x, previous_hovered), (next_x, next_hovered)] {
        if hovered {
            renderer.push_rounded_rect(
                button_x + 3.0,
                y + 3.0,
                arrow_w - 6.0,
                header_h - 6.0,
                (5.0 * s).round(),
                [0.20, 0.18, 0.29, 1.0],
            );
        }
    }
    let arrow_scale = 1.0;
    for (label, button_x) in [("‹", x), ("›", next_x)] {
        let text_w = renderer.measure_ui_width(label, arrow_scale).round();
        renderer.draw_string_scaled_pixel_snapped(
            label,
            (button_x + (arrow_w - text_w) * 0.5).round(),
            Renderer::tree_row_text_y(y, header_h, s),
            renderer.theme.fg,
            arrow_scale,
        );
    }

    let month_name = MONTHS
        .get(editor.calendar_month.saturating_sub(1) as usize)
        .copied()
        .unwrap_or("?");
    let title = format!("{month_name} {}", editor.calendar_year);
    let title_scale = 0.90;
    let title_w = renderer.measure_ui_width(&title, title_scale).round();
    renderer.draw_string_scaled_pixel_snapped(
        &title,
        (x + (width - title_w) * 0.5).round(),
        Renderer::tree_row_text_y(y, header_h, s),
        renderer.theme.fg,
        title_scale,
    );

    let weekdays_y = y + header_h;
    let weekday_scale = 0.78;
    for (index, label) in WEEKDAYS.iter().enumerate() {
        let label_w = renderer.measure_ui_width(label, weekday_scale).round();
        renderer.draw_string_scaled_pixel_snapped(
            label,
            (x + index as f32 * cell_w + (cell_w - label_w) * 0.5).round(),
            Renderer::tree_row_text_y(weekdays_y, weekday_h, s),
            renderer.theme.line_num,
            weekday_scale,
        );
    }

    let first_weekday = crate::app::database::database_calendar_weekday_monday(
        editor.calendar_year,
        editor.calendar_month,
        1,
    ) as usize;
    let days = crate::app::database::database_days_in_month(
        editor.calendar_year,
        editor.calendar_month,
    );
    let grid_y = weekdays_y + weekday_h;
    let day_scale = 0.84;
    let hover_size = (27.0 * s).round();
    for day in 1..=days {
        let slot = first_weekday + day as usize - 1;
        let col = slot % 7;
        let row = slot / 7;
        let dx = (x + col as f32 * cell_w).round();
        let dy = (grid_y + row as f32 * cell_h).round();
        let hovered = ui.register_rect(
            UiId::DatabaseTableDateDay(day as u8),
            dx,
            dy,
            cell_w,
            cell_h,
            mx,
            my,
        );
        if hovered {
            let (hx, hy, hw, hh) =
                database_calendar_centered_square(dx, dy, cell_w, cell_h, hover_size);
            renderer.push_rounded_rect(hx, hy, hw, hh, (4.0 * s).round(), [0.22, 0.18, 0.32, 1.0]);
        }
        let day_text = day.to_string();
        let day_w = renderer.measure_ui_width(&day_text, day_scale).round();
        renderer.draw_string_scaled_pixel_snapped(
            &day_text,
            (dx + (cell_w - day_w) * 0.5).round(),
            Renderer::tree_row_text_y(dy, cell_h, s),
            renderer.theme.fg,
            day_scale,
        );
    }

    let footer_y = y + height - footer_h;
    let today_w = (width * 0.5).round();
    let show_now = matches!(
        column.type_kind,
        DatabaseTypeKind::Timestamp | DatabaseTypeKind::TimestampTz
    );
    if show_now {
        draw_database_calendar_footer_button(
            renderer,
            ui,
            UiId::DatabaseTableDateToday,
            "Сегодня",
            x,
            footer_y,
            today_w,
            footer_h,
            0.86,
            mx,
            my,
            s,
        );
        draw_database_calendar_footer_button(
            renderer,
            ui,
            UiId::DatabaseTableDateNow,
            "Сейчас UTC",
            x + today_w,
            footer_y,
            width - today_w,
            footer_h,
            0.80,
            mx,
            my,
            s,
        );
    } else {
        draw_database_calendar_footer_button(
            renderer,
            ui,
            UiId::DatabaseTableDateToday,
            "Сегодня",
            x,
            footer_y,
            width,
            footer_h,
            0.86,
            mx,
            my,
            s,
        );
    }
}

fn database_column_header(
    column: &crate::app::database::DatabaseColumnInfo,
    state: &crate::app::database::DatabaseTableTabState,
) -> String {
    let sort = if state.grid.view.sorted_column.as_deref() == Some(column.name.as_str()) {
        match state.grid.view.sort_direction {
            Some(crate::app::database::DatabaseSortDirection::Asc) => " ↓",
            Some(crate::app::database::DatabaseSortDirection::Desc) => " ↑",
            None => "",
        }
    } else { "" };
    format!("{}{}{}", column.name, if column.primary_key { " 🔑" } else { "" }, sort)
}

fn database_column_geometry(
    metadata: &crate::app::database::DatabaseTableMetadata,
    grid: &crate::app::database::DatabaseTableGridState,
    column_index: usize,
) -> (f32, f32) {
    let x = metadata.columns[..column_index]
        .iter()
        .map(|column| grid.column_width(&column.name))
        .sum();
    (x, grid.column_width(&metadata.columns[column_index].name))
}

fn database_visible_columns(
    metadata: &crate::app::database::DatabaseTableMetadata,
    grid: &crate::app::database::DatabaseTableGridState,
    viewport_width: f32,
) -> std::ops::Range<usize> {
    let start = grid.scroll_x.current.max(0.0);
    let end = start + viewport_width.max(0.0);
    let mut x = 0.0;
    let mut first = 0;
    let mut last = metadata.columns.len();
    let mut found = false;
    for (index, column) in metadata.columns.iter().enumerate() {
        let width = grid.column_width(&column.name);
        if !found && x + width >= start { first = index.saturating_sub(1); found = true; }
        if found && x > end { last = (index + 1).min(metadata.columns.len()); break; }
        x += width;
    }
    first..last
}

#[allow(clippy::too_many_arguments)]
fn draw_database_table_scrollbars(
    renderer: &mut Renderer,
    ui: &mut UiRegistry,
    layout: &crate::app::database::DatabaseGridLayout,
    metadata: &crate::app::database::DatabaseTableMetadata,
    state: &crate::app::database::DatabaseTableTabState,
    mx: f32,
    my: f32,
    s: f32,
) {
    let track_color = [0.055, 0.058, 0.075, 1.0];
    let thumb_color = [0.62, 0.38, 0.82, 0.9];
    let (vertical_rect, horizontal_rect) = database_table_scrollbar_rects(layout);
    if let Some(rect) = vertical_rect {
        let total_h = state.grid.logical_row_count() as f32
            * (crate::app::database::DATABASE_GRID_ROW_HEIGHT * s).round();
        renderer.push_rect(rect.x, rect.y, rect.w, rect.h, track_color);
        ui.register_rect(UiId::DatabaseTableScrollY, rect.x, rect.y, rect.w, rect.h, mx, my);
        if let Some(thumb) = crate::scroll::scrollbar_thumb(
            rect.y,
            rect.h,
            layout.body_rect.h,
            total_h,
            state.grid.scroll_y.current * s,
            (28.0 * s).round(),
        ) {
            renderer.push_rounded_rect(
                rect.x + (2.0 * s).round(),
                thumb.start.round(),
                (rect.w - 4.0 * s).max(4.0).round(),
                thumb.len.round(),
                (3.0 * s).round(),
                thumb_color,
            );
        }
    }
    if let Some(rect) = horizontal_rect {
        let content_w = (state.grid.content_width(metadata) * s).round();
        renderer.push_rect(rect.x, rect.y, rect.w, rect.h, track_color);
        ui.register_rect(UiId::DatabaseTableScrollX, rect.x, rect.y, rect.w, rect.h, mx, my);
        if let Some(thumb) = crate::scroll::scrollbar_thumb(
            rect.x,
            rect.w,
            layout.body_rect.w,
            content_w,
            state.grid.scroll_x.current * s,
            (36.0 * s).round(),
        ) {
            renderer.push_rounded_rect(
                thumb.start.round(),
                rect.y + (2.0 * s).round(),
                thumb.len.round(),
                (rect.h - 4.0 * s).max(4.0).round(),
                (3.0 * s).round(),
                thumb_color,
            );
        }
    }
}

fn database_table_scrollbar_rects(
    layout: &crate::app::database::DatabaseGridLayout,
) -> (
    Option<crate::app::database::DatabaseGridRect>,
    Option<crate::app::database::DatabaseGridRect>,
) {
    (
        layout.vertical_scrollbar_rect,
        layout.horizontal_scrollbar_rect,
    )
}


#[cfg(test)]
mod database_table_renderer_tests {
    use super::*;

    #[test]
    fn bug_22_enum_popup_can_reach_options_after_the_first_ten() {
        let first = database_enum_page(25, 0, 8);
        assert_eq!(first.start, 0);
        assert_eq!(first.end, 8);
        assert!(first.next);
        let later = database_enum_page(25, 17, 8);
        assert_eq!(later.start, 17);
        assert_eq!(later.end, 25);
        assert!(later.previous);
        assert!(!later.next);
    }

    #[test]
    fn bug_23_date_picker_is_scaled_and_repositioned_inside_rows_viewport() {
        let placement = database_popup_placement(
            180.0, 160.0, 80.0, 30.0, 238.0, 300.0, 10.0, 20.0, 220.0, 140.0, 1.0,
        );
        assert!(placement.x >= 10.0);
        assert!(placement.y >= 20.0);
        assert!(placement.x + placement.w <= 230.0 + 0.5);
        assert!(placement.y + placement.h <= 160.0 + 0.5);
        assert!(placement.scale < 1.0);
    }

    #[test]
    fn bug_24_enum_popup_is_scaled_and_repositioned_inside_rows_viewport() {
        let placement = database_popup_placement(
            0.0, 95.0, 60.0, 28.0, 180.0, 280.0, 0.0, 0.0, 140.0, 120.0, 1.0,
        );
        assert_eq!(placement.x, 0.0);
        assert!(placement.y >= 0.0);
        assert!(placement.w <= 140.0);
        assert!(placement.h <= 120.0);
        assert!(placement.y + placement.h <= 120.0 + 0.5);
    }

    #[test]
    fn a4_b002_enum_option_text_uses_fitted_popup_scale() {
        let (x, y, w) = database_enum_option_text_layout(10.0, 100.0, 20.0, 14.0, 0.5);
        assert_eq!(x, 14.0);
        assert_eq!(w, 93.0);
        assert_eq!(y, Renderer::tree_row_text_y(20.0, 14.0, 0.5));
    }

    #[test]
    fn a4_b001_table_scrollbar_tracks_match_shared_hitbox_rects() {
        let layout = crate::app::database::database_grid_layout(
            0.0, 10.0, 500.0, 300.0, 50.0, 12.0, 40.0, 700.0, 800.0,
        );
        let (vertical, horizontal) = database_table_scrollbar_rects(&layout);
        let vertical = vertical.unwrap();
        let horizontal = horizontal.unwrap();
        assert_eq!(vertical.y, layout.body_rect.y);
        assert_eq!(vertical.h, layout.body_rect.h);
        assert_eq!(horizontal.x, layout.body_rect.x);
        assert_eq!(horizontal.w, layout.body_rect.w);
    }

    #[test]
    fn grid_does_not_reserve_black_scrollbar_strips_without_overflow() {
        let viewport = crate::app::database::database_grid_viewport(800.0, 500.0, 54.0, 12.0, 28.0, 600.0, 300.0);
        assert!(!viewport.show_x);
        assert!(!viewport.show_y);
        assert_eq!(viewport.body_w, 800.0);
        assert_eq!(viewport.body_h, 500.0);
    }

    #[test]
    fn horizontal_overflow_reserves_only_bottom_scrollbar() {
        let viewport = crate::app::database::database_grid_viewport(800.0, 500.0, 54.0, 12.0, 28.0, 900.0, 300.0);
        assert!(viewport.show_x);
        assert!(!viewport.show_y);
        assert_eq!(viewport.body_w, 800.0);
        assert_eq!(viewport.body_h, 488.0);
    }

    #[test]
    fn vertical_scrollbar_can_trigger_horizontal_overflow() {
        let viewport = crate::app::database::database_grid_viewport(800.0, 500.0, 54.0, 12.0, 28.0, 740.0, 900.0);
        assert!(viewport.show_y);
        assert!(viewport.show_x);
        assert_eq!(viewport.data_w, 734.0);
        assert_eq!(viewport.body_h, 488.0);
    }

    #[test]
    fn inline_cell_editor_keeps_the_same_text_baseline_as_display_mode() {
        let scale: f32 = 1.25;
        let row_y: f32 = 73.0;
        let row_h = (crate::app::database::DATABASE_GRID_ROW_HEIGHT * scale).round();
        let display = Renderer::tree_row_text_y(row_y, row_h, scale);
        let editing = Renderer::tree_row_text_y(row_y + 1.0, row_h - 2.0, scale);
        assert_eq!(display, editing);
    }

    #[test]
    fn toolbar_title_row_is_above_button_row() {
        let scale: f32 = 1.25;
        let title_baseline = (22.0 * scale).round();
        let button_top = (39.0 * scale).round();
        assert!(title_baseline < button_top);
        assert!((TABLE_TOOLBAR_H * scale).round() >= button_top + (38.0 * scale).round());
    }

    #[test]
    fn calendar_hover_square_is_centered_in_day_cell() {
        let rect = database_calendar_centered_square(10.0, 20.0, 34.0, 32.0, 27.0);
        assert_eq!(rect, (14.0, 23.0, 27.0, 27.0));
        assert!((rect.0 + rect.2 * 0.5 - (10.0 + 34.0 * 0.5)).abs() <= 0.5);
        assert!((rect.1 + rect.3 * 0.5 - (20.0 + 32.0 * 0.5)).abs() <= 0.5);
    }

    #[test]
    fn table_typography_and_toolbar_icons_use_larger_metrics() {
        assert!(crate::app::database::DATABASE_GRID_ROW_HEIGHT >= 38.0);
        assert!(crate::app::database::DATABASE_GRID_HEADER_HEIGHT >= 40.0);
        assert!(crate::app::database::DATABASE_TABLE_INPUT_TEXT_SCALE >= 0.9);
        assert!(TABLE_CELL_TEXT_SCALE >= 0.9);
        assert_eq!(database_date_picker_size(1.0), (238.0, 300.0));
    }

    #[test]
    fn unavailable_table_shows_connection_message_instead_of_blank_body() {
        let mut state = crate::app::database::DatabaseTableTabState::default();
        state.loading = false;

        assert_eq!(
            database_table_unavailable_message(&state),
            Some(crate::app::database::DATABASE_TABLE_DISCONNECTED_MESSAGE)
        );
    }

    #[test]
    fn unavailable_table_prefers_backend_error_message() {
        let mut state = crate::app::database::DatabaseTableTabState::default();
        state.loading = false;
        state.error = Some("connection refused".to_string());
        state.unavailable_text.set_text("connection refused");

        assert_eq!(
            database_table_unavailable_message(&state),
            Some("connection refused")
        );
    }

    #[test]
    fn sort_header_uses_down_arrow_for_asc_and_up_arrow_for_desc() {
        let column = crate::app::database::DatabaseColumnInfo {
            ordinal: 1,
            name: "id".to_string(),
            type_name: "bigint".to_string(),
            type_oid: 20,
            type_kind: crate::app::database::DatabaseTypeKind::Other,
            nullable: false,
            default_expression: None,
            identity: false,
            generated: false,
            primary_key: false,
            enum_values: Vec::new(),
        };
        let mut state = crate::app::database::DatabaseTableTabState::default();
        state.grid.view.sorted_column = Some("id".to_string());
        state.grid.view.sort_direction = Some(crate::app::database::DatabaseSortDirection::Asc);
        assert!(database_column_header(&column, &state).ends_with(" ↓"));

        state.grid.view.sort_direction = Some(crate::app::database::DatabaseSortDirection::Desc);
        assert!(database_column_header(&column, &state).ends_with(" ↑"));
    }

    #[test]
    fn bug_62_page_status_uses_loaded_rows_not_theoretical_limit() {
        let view = crate::app::database::DatabaseTableViewState {
            key: crate::app::database::DatabaseTableViewKey {
                connection_id: crate::app::database::DatabaseConnectionId(1),
                database_name: "db".to_string(),
                table_name: "items".to_string(),
            },
            limit: 100,
            ..crate::app::database::DatabaseTableViewState::default()
        };
        let mut state = crate::app::database::DatabaseTableTabState::new(view);
        state.grid.count = Some(100);
        let rows = (0..99)
            .map(|absolute_index| crate::app::database::DatabaseGridRow {
                absolute_index,
                cells: Vec::new(),
                xmin: None,
                state: crate::app::database::DatabaseRowState::Clean,
            })
            .collect::<Vec<_>>();
        state.grid.chunks.insert(
            0,
            crate::app::database::DatabaseTableChunk {
                generation: crate::app::database::DatabaseGeneration(1),
                chunk_index: 0,
                rows,
                estimated_bytes: 0,
            },
        );
        assert_eq!(database_table_page_status(&state), "1–99 из 100");
    }
}
