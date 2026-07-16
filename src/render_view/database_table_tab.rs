use crate::renderer::Renderer;
use crate::ui_system::{UiId, UiRegistry};
use crate::widgets::{ButtonView, IconType};
use glow::HasContext;

const TABLE_TOOLBAR_H: f32 = 42.0;
const TABLE_FILTER_H: f32 = 40.0;
const ROW_GUTTER_W: f32 = 54.0;
const SCROLLBAR_W: f32 = 12.0;

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
    ) {
        self.push_rect(x, y, w, h, self.theme.bg);
        ui_registry.register_blocker(UiId::DatabaseTableBody, x, y, w, h, mx, my);
        if state.loading {
            self.draw_string_scaled_stable(
                "Загрузка структуры таблицы…",
                x + 24.0 * s,
                y + 42.0 * s,
                self.theme.line_num,
                0.9,
            );
            return;
        }
        if let Some(error) = state.error.as_deref() {
            self.draw_string_scaled_stable(
                error,
                x + 24.0 * s,
                y + 42.0 * s,
                [0.95, 0.38, 0.42, 1.0],
                0.82,
            );
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
        );
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
        self.push_rect(x, y, w, TABLE_TOOLBAR_H * s, [0.105, 0.11, 0.14, 1.0]);
        let editable = state.metadata.as_ref().is_some_and(|metadata| metadata.editable);
        let dirty = state.grid.dirty();
        let mut bx = x + 8.0 * s;
        for (id, text, icon, active, width) in [
            (UiId::DatabaseTableAddRow, "Добавить", Some(IconType::Plus), editable, 92.0),
            (UiId::DatabaseTableDeleteRows, "Удалить", Some(IconType::GitMinus), editable, 92.0),
            (UiId::DatabaseTableUndo, "Отменить", Some(IconType::Rollback), dirty, 98.0),
            (UiId::DatabaseTableSave, "Сохранить", Some(IconType::Save), dirty, 106.0),
            (UiId::DatabaseTablePreview, "SQL", Some(IconType::Eye), dirty, 72.0),
            (UiId::DatabaseTableRefresh, "", Some(IconType::Reload), true, 42.0),
        ] {
            draw_database_table_button(self, ui, id, bx, y + 6.0 * s, width * s, 30.0 * s, text, icon, active, mx, my, s);
            bx += (width + 6.0) * s;
        }

        let nav_w = 38.0 * s;
        let nav_x = (x + w - 430.0 * s).max(bx + 8.0 * s);
        for (index, (id, label, active)) in [
            (UiId::DatabaseTablePageFirst, "≪", state.grid.view.current_page > 0),
            (UiId::DatabaseTablePagePrevious, "‹", state.grid.view.current_page > 0),
            (UiId::DatabaseTablePageNext, "›", database_table_has_next(state)),
            (UiId::DatabaseTablePageLast, "≫", database_table_has_next(state)),
        ]
        .into_iter()
        .enumerate()
        {
            draw_database_table_button(
                self,
                ui,
                id,
                nav_x + index as f32 * (nav_w + 4.0 * s),
                y + 6.0 * s,
                nav_w,
                30.0 * s,
                label,
                None,
                active,
                mx,
                my,
                s,
            );
        }
        let limit_x = nav_x + 4.0 * (nav_w + 4.0 * s) + 6.0 * s;
        draw_database_table_button(
            self,
            ui,
            UiId::DatabaseTableLimit,
            limit_x,
            y + 6.0 * s,
            84.0 * s,
            30.0 * s,
            &format!("{} строк", state.grid.view.limit),
            None,
            true,
            mx,
            my,
            s,
        );
        let page_text = database_table_page_status(state);
        self.draw_string_scaled_stable(
            &page_text,
            limit_x + 94.0 * s,
            y + 26.0 * s,
            self.theme.line_num,
            0.72,
        );
        self.draw_string_scaled_stable(
            &format!("{} / public.{}", meta.database_name, meta.table_name),
            x + 10.0 * s,
            y + TABLE_TOOLBAR_H * s - 3.0 * s,
            [0.52, 0.55, 0.64, 1.0],
            0.62,
        );
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
    ) {
        self.push_rect(x, y, w, TABLE_FILTER_H * s, [0.085, 0.09, 0.115, 1.0]);
        let label_w = 64.0 * s;
        let gap = 12.0 * s;
        let field_w = ((w - 2.0 * label_w - gap - 30.0 * s) * 0.5).max(120.0 * s);
        let input_y = y + 6.0 * s;
        self.draw_string_scaled_stable("WHERE", x + 10.0 * s, y + 25.0 * s, self.theme.line_num, 0.72);
        draw_database_table_input(
            self,
            ui,
            UiId::DatabaseTableWhereInput,
            x + label_w,
            input_y,
            field_w,
            28.0 * s,
            state.grid.where_input.text(),
            state.grid.focused_input == Some(crate::app::database::DatabaseTableInputTarget::Where),
            mx,
            my,
            s,
        );
        let order_label_x = x + label_w + field_w + gap;
        self.draw_string_scaled_stable("ORDER", order_label_x, y + 25.0 * s, self.theme.line_num, 0.72);
        draw_database_table_input(
            self,
            ui,
            UiId::DatabaseTableOrderInput,
            order_label_x + label_w,
            input_y,
            field_w,
            28.0 * s,
            state.grid.order_by_input.text(),
            state.grid.focused_input == Some(crate::app::database::DatabaseTableInputTarget::OrderBy),
            mx,
            my,
            s,
        );
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
    ) {
        use crate::app::database::{DATABASE_GRID_HEADER_HEIGHT, DATABASE_GRID_ROW_HEIGHT};
        let scroll_w = SCROLLBAR_W * s;
        let horizontal_h = SCROLLBAR_W * s;
        let body_w = (w - scroll_w).max(0.0);
        let body_h = (h - horizontal_h).max(0.0);
        let header_h = DATABASE_GRID_HEADER_HEIGHT * s;
        let rows_y = y + header_h;
        let rows_h = (body_h - header_h).max(0.0);
        let gutter_w = ROW_GUTTER_W * s;
        let data_x = x + gutter_w;
        let data_w = (body_w - gutter_w).max(0.0);
        ui.register_blocker(UiId::DatabaseTableGridBody, x, y, body_w, body_h, mx, my);
        self.push_rect(x, y, body_w, body_h, [0.075, 0.078, 0.098, 1.0]);
        self.push_rect(x, y, gutter_w, body_h, [0.095, 0.10, 0.125, 1.0]);
        self.push_rect(x, y, body_w, header_h, [0.125, 0.13, 0.16, 1.0]);

        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            self.gl.scissor(
                data_x.round() as i32,
                (self.height - (y + body_h)).round().max(0.0) as i32,
                data_w.round().max(0.0) as i32,
                body_h.round().max(0.0) as i32,
            );
        }
        let visible_columns = database_visible_columns(metadata, &state.grid, data_w / s);
        let mut scratch = String::new();
        for column_index in visible_columns.clone() {
            let column = &metadata.columns[column_index];
            let (column_x, column_w) = database_column_geometry(metadata, &state.grid, column_index);
            let draw_x = data_x + column_x * s - state.grid.scroll_x.current * s;
            let draw_w = column_w * s;
            if draw_x + draw_w < data_x || draw_x > data_x + data_w {
                continue;
            }
            let selected_sort = state.grid.view.sorted_column.as_deref() == Some(column.name.as_str());
            if selected_sort {
                self.push_rect(draw_x, y, draw_w, header_h, [0.35, 0.22, 0.52, 0.36]);
            }
            ui.register_rect(UiId::DatabaseTableHeader(column_index), draw_x, y, draw_w, header_h, mx, my);
            self.draw_tree_label_clipped(
                &database_column_header(column, state),
                draw_x + 8.0 * s,
                y + 20.0 * s,
                (draw_w - 16.0 * s).max(4.0),
                self.theme.fg,
                0.74,
                &mut scratch,
            );
            let divider_x = draw_x + draw_w - 3.0 * s;
            ui.register_rect(
                UiId::DatabaseTableColumnResize(column_index),
                divider_x,
                y,
                6.0 * s,
                body_h,
                mx,
                my,
            );
            self.push_rect(draw_x + draw_w - 1.0, y, 1.0, body_h, [1.0, 1.0, 1.0, 0.09]);
        }

        let relative_first = (state.grid.scroll_y.current.max(0.0) / DATABASE_GRID_ROW_HEIGHT)
            .floor() as usize;
        let visible_count = (rows_h / (DATABASE_GRID_ROW_HEIGHT * s)).ceil() as usize + 2;
        let server_rows = database_server_rows_on_page(state);
        let page_base = state.grid.view.current_page.saturating_mul(state.grid.view.limit);
        for relative in relative_first.saturating_sub(1)..relative_first.saturating_add(visible_count) {
            let row_y = rows_y + relative as f32 * DATABASE_GRID_ROW_HEIGHT * s
                - state.grid.scroll_y.current * s;
            if row_y + DATABASE_GRID_ROW_HEIGHT * s < rows_y || row_y > rows_y + rows_h {
                continue;
            }
            let row = if relative < server_rows {
                state.grid.row(page_base.saturating_add(relative))
            } else {
                state.grid.added_rows.get(relative.saturating_sub(server_rows))
            };
            let Some(row) = row else {
                continue;
            };
            let bg = match row.state {
                crate::app::database::DatabaseRowState::Added => [0.10, 0.28, 0.16, 0.72],
                crate::app::database::DatabaseRowState::Deleted => [0.28, 0.28, 0.30, 0.58],
                crate::app::database::DatabaseRowState::Clean => {
                    if relative % 2 == 0 { [0.08, 0.084, 0.105, 1.0] } else { [0.095, 0.098, 0.12, 1.0] }
                }
            };
            self.push_rect(data_x, row_y, data_w, DATABASE_GRID_ROW_HEIGHT * s, bg);
            for column_index in visible_columns.clone() {
                let Some(cell) = row.cells.get(column_index) else { continue; };
                let (column_x, column_w) = database_column_geometry(metadata, &state.grid, column_index);
                let draw_x = data_x + column_x * s - state.grid.scroll_x.current * s;
                let draw_w = column_w * s;
                if state.grid.selection.contains_cell(row.absolute_index, column_index) {
                    self.push_rect(draw_x, row_y, draw_w, DATABASE_GRID_ROW_HEIGHT * s, [0.42, 0.25, 0.63, 0.48]);
                }
                if cell.dirty {
                    self.push_rect(draw_x, row_y + DATABASE_GRID_ROW_HEIGHT * s - 2.0 * s, draw_w, 2.0 * s, [0.32, 0.90, 0.48, 1.0]);
                }
                ui.register_rect(
                    UiId::DatabaseTableCell(row.absolute_index, column_index),
                    draw_x,
                    row_y,
                    draw_w,
                    DATABASE_GRID_ROW_HEIGHT * s,
                    mx,
                    my,
                );
                self.draw_tree_label_clipped(
                    &cell.value.display_text(),
                    draw_x + 7.0 * s,
                    row_y + 19.0 * s,
                    (draw_w - 14.0 * s).max(4.0),
                    if matches!(cell.value, crate::app::database::DatabaseCellValue::Null | crate::app::database::DatabaseCellValue::Default) {
                        [0.52, 0.55, 0.62, 1.0]
                    } else {
                        self.theme.fg
                    },
                    0.72,
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
                        draw_x + 2.0 * s,
                        row_y + 2.0 * s,
                        (draw_w - 4.0 * s).max(20.0),
                        (DATABASE_GRID_ROW_HEIGHT - 4.0) * s,
                        editor.input.text(),
                        true,
                        mx,
                        my,
                        s,
                    );
                    if editor.kind == crate::app::database::DatabaseCellEditorKind::DateTime {
                        draw_database_date_picker(
                            self,
                            ui,
                            draw_x,
                            row_y + DATABASE_GRID_ROW_HEIGHT * s,
                            editor,
                            &metadata.columns[column_index],
                            mx,
                            my,
                            s,
                        );
                    }
                    if editor.kind == crate::app::database::DatabaseCellEditorKind::Enum {
                        for (option_index, option) in metadata.columns[column_index]
                            .enum_values
                            .iter()
                            .take(10)
                            .enumerate()
                        {
                            let option_y = row_y + DATABASE_GRID_ROW_HEIGHT * s + option_index as f32 * 25.0 * s;
                            self.push_rect(draw_x, option_y, draw_w, 25.0 * s, [0.13, 0.14, 0.18, 1.0]);
                            ui.register_rect(UiId::DatabaseTableEnumOption(option_index), draw_x, option_y, draw_w, 25.0 * s, mx, my);
                            self.draw_tree_label_clipped(option, draw_x + 6.0 * s, option_y + 18.0 * s, draw_w - 12.0 * s, self.theme.fg, 0.7, &mut scratch);
                        }
                    }
                }
            }
        }
        self.flush();
        unsafe { self.gl.disable(glow::SCISSOR_TEST) };

        self.push_rect(x, y, gutter_w, header_h, [0.115, 0.12, 0.15, 1.0]);
        self.draw_string_scaled_stable("#", x + 18.0 * s, y + 20.0 * s, self.theme.line_num, 0.72);
        for relative in relative_first.saturating_sub(1)..relative_first.saturating_add(visible_count) {
            let row_y = rows_y + relative as f32 * DATABASE_GRID_ROW_HEIGHT * s
                - state.grid.scroll_y.current * s;
            if row_y + DATABASE_GRID_ROW_HEIGHT * s < rows_y || row_y > rows_y + rows_h { continue; }
            let absolute = if relative < server_rows {
                page_base.saturating_add(relative)
            } else if let Some(row) = state.grid.added_rows.get(relative.saturating_sub(server_rows)) {
                row.absolute_index
            } else { continue; };
            if state.grid.selection.contains_row(absolute) {
                self.push_rect(x, row_y, gutter_w, DATABASE_GRID_ROW_HEIGHT * s, [0.42, 0.25, 0.63, 0.55]);
            }
            ui.register_rect(UiId::DatabaseGridRow(absolute), x, row_y, gutter_w, DATABASE_GRID_ROW_HEIGHT * s, mx, my);
            self.draw_string_scaled_stable(&(absolute + 1).to_string(), x + 8.0 * s, row_y + 19.0 * s, self.theme.line_num, 0.68);
        }

        draw_database_table_scrollbars(self, ui, x, y, body_w, body_h, data_x, data_w, rows_y, rows_h, metadata, state, mx, my, s);
        if let Some(reason) = metadata.read_only_reason.as_deref() {
            self.push_rect(x, y + body_h - 28.0 * s, body_w, 28.0 * s, [0.30, 0.18, 0.06, 0.94]);
            self.draw_string_scaled_stable(reason, x + 10.0 * s, y + body_h - 9.0 * s, [0.98, 0.72, 0.30, 1.0], 0.72);
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
    if active {
        ui.register_button_view(id, ButtonView { x, y, w, h, text, icon, text_scale: 0.7, icon_size: 15.0 * s }, renderer, mx, my, s, false);
    } else {
        renderer.push_rounded_rect_border(x, y, w, h, 4.0 * s, 1.0, [1.0,1.0,1.0,0.07], [0.10,0.105,0.13,1.0]);
        renderer.draw_string_scaled_stable(text, x + 8.0 * s, y + 20.0 * s, [0.40,0.42,0.48,1.0], 0.66);
    }
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
    text: &str,
    focused: bool,
    mx: f32,
    my: f32,
    s: f32,
) {
    renderer.push_rounded_rect_border(
        x,
        y,
        w,
        h,
        4.0 * s,
        1.0,
        if focused { [0.62,0.38,0.90,1.0] } else { [1.0,1.0,1.0,0.13] },
        [0.055,0.06,0.08,1.0],
    );
    ui.register_text_input(id, x, y, w, h, mx, my);
    let mut scratch = String::new();
    renderer.draw_tree_label_clipped(text, x + 7.0 * s, y + 19.0 * s, w - 14.0 * s, renderer.theme.fg, 0.72, &mut scratch);
}

fn database_table_has_next(state: &crate::app::database::DatabaseTableTabState) -> bool {
    state.grid.count.is_some_and(|count| {
        (state.grid.view.current_page + 1).saturating_mul(state.grid.view.limit) < count as usize
    })
}

fn database_server_rows_on_page(state: &crate::app::database::DatabaseTableTabState) -> usize {
    let base = state.grid.view.current_page.saturating_mul(state.grid.view.limit);
    state.grid.count.map_or(state.grid.view.limit, |count| {
        (count as usize).saturating_sub(base).min(state.grid.view.limit)
    })
}

fn database_table_page_status(state: &crate::app::database::DatabaseTableTabState) -> String {
    let base = state.grid.view.current_page.saturating_mul(state.grid.view.limit);
    let visible = database_server_rows_on_page(state);
    match state.grid.count {
        Some(count) if count == 0 => "0 из 0".to_string(),
        Some(count) => format!("{}–{} из {}", base + 1, base + visible, count),
        None if state.grid.loading_count => format!("{}–{}, подсчёт…", base + 1, base + visible),
        None => state.grid.count_error.clone().unwrap_or_else(|| "общее число неизвестно".to_string()),
    }
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
    let is_time_only = column.type_kind == DatabaseTypeKind::Time;
    if is_time_only {
        let w = 112.0 * s;
        let h = 28.0 * s;
        renderer.push_rounded_rect(x, y, w, h, 4.0 * s, [0.13, 0.14, 0.18, 1.0]);
        ui.register_rect(UiId::DatabaseTableDateNow, x, y, w, h, mx, my);
        renderer.draw_string_scaled_stable("Сейчас UTC", x + 8.0 * s, y + 19.0 * s, renderer.theme.fg, 0.68);
        return;
    }

    const MONTHS: [&str; 12] = [
        "Январь", "Февраль", "Март", "Апрель", "Май", "Июнь",
        "Июль", "Август", "Сентябрь", "Октябрь", "Ноябрь", "Декабрь",
    ];
    const WEEKDAYS: [&str; 7] = ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Вс"];
    let cell_w = 28.0 * s;
    let cell_h = 24.0 * s;
    let width = cell_w * 7.0;
    let header_h = 30.0 * s;
    let weekday_h = 20.0 * s;
    let footer_h = 28.0 * s;
    let height = header_h + weekday_h + cell_h * 6.0 + footer_h;
    renderer.push_rounded_rect_border(
        x,
        y,
        width,
        height,
        5.0 * s,
        1.0,
        [0.32, 0.34, 0.42, 1.0],
        [0.095, 0.10, 0.13, 1.0],
    );

    let arrow_w = 30.0 * s;
    ui.register_rect(UiId::DatabaseTableDatePreviousMonth, x, y, arrow_w, header_h, mx, my);
    ui.register_rect(
        UiId::DatabaseTableDateNextMonth,
        x + width - arrow_w,
        y,
        arrow_w,
        header_h,
        mx,
        my,
    );
    renderer.draw_string_scaled_stable("‹", x + 10.0 * s, y + 21.0 * s, renderer.theme.fg, 0.9);
    renderer.draw_string_scaled_stable("›", x + width - 20.0 * s, y + 21.0 * s, renderer.theme.fg, 0.9);
    let month_name = MONTHS
        .get(editor.calendar_month.saturating_sub(1) as usize)
        .copied()
        .unwrap_or("?");
    let title = format!("{month_name} {}", editor.calendar_year);
    let title_w = title.chars().count() as f32 * 7.0 * s;
    renderer.draw_string_scaled_stable(
        &title,
        x + (width - title_w) * 0.5,
        y + 20.0 * s,
        renderer.theme.fg,
        0.68,
    );

    let weekdays_y = y + header_h;
    for (index, label) in WEEKDAYS.iter().enumerate() {
        renderer.draw_string_scaled_stable(
            label,
            x + index as f32 * cell_w + 7.0 * s,
            weekdays_y + 15.0 * s,
            renderer.theme.line_num,
            0.58,
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
    for day in 1..=days {
        let slot = first_weekday + day as usize - 1;
        let col = slot % 7;
        let row = slot / 7;
        let dx = x + col as f32 * cell_w;
        let dy = grid_y + row as f32 * cell_h;
        if mx >= dx && mx <= dx + cell_w && my >= dy && my <= dy + cell_h {
            renderer.push_rect(dx + 1.0 * s, dy + 1.0 * s, cell_w - 2.0 * s, cell_h - 2.0 * s, [0.22, 0.18, 0.32, 1.0]);
        }
        ui.register_rect(UiId::DatabaseTableDateDay(day as u8), dx, dy, cell_w, cell_h, mx, my);
        renderer.draw_string_scaled_stable(
            &day.to_string(),
            dx + 8.0 * s,
            dy + 17.0 * s,
            renderer.theme.fg,
            0.62,
        );
    }

    let footer_y = y + height - footer_h;
    let today_w = width * 0.52;
    ui.register_rect(UiId::DatabaseTableDateToday, x, footer_y, today_w, footer_h, mx, my);
    renderer.draw_string_scaled_stable("Сегодня", x + 9.0 * s, footer_y + 19.0 * s, renderer.theme.fg, 0.65);
    if matches!(column.type_kind, DatabaseTypeKind::Timestamp | DatabaseTypeKind::TimestampTz) {
        ui.register_rect(UiId::DatabaseTableDateNow, x + today_w, footer_y, width - today_w, footer_h, mx, my);
        renderer.draw_string_scaled_stable("Сейчас UTC", x + today_w + 7.0 * s, footer_y + 19.0 * s, renderer.theme.fg, 0.58);
    }
}

fn database_column_header(
    column: &crate::app::database::DatabaseColumnInfo,
    state: &crate::app::database::DatabaseTableTabState,
) -> String {
    let sort = if state.grid.view.sorted_column.as_deref() == Some(column.name.as_str()) {
        match state.grid.view.sort_direction {
            Some(crate::app::database::DatabaseSortDirection::Asc) => " ↑",
            Some(crate::app::database::DatabaseSortDirection::Desc) => " ↓",
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
    x: f32,
    y: f32,
    body_w: f32,
    body_h: f32,
    data_x: f32,
    data_w: f32,
    rows_y: f32,
    rows_h: f32,
    metadata: &crate::app::database::DatabaseTableMetadata,
    state: &crate::app::database::DatabaseTableTabState,
    mx: f32,
    my: f32,
    s: f32,
) {
    let track = SCROLLBAR_W * s;
    let total_h = state.grid.logical_row_count() as f32 * crate::app::database::DATABASE_GRID_ROW_HEIGHT * s;
    let max_y = (total_h - rows_h).max(0.0);
    let sy_x = x + body_w;
    renderer.push_rect(sy_x, y, track, body_h, [0.055,0.058,0.075,1.0]);
    ui.register_rect(UiId::DatabaseTableScrollY, sy_x, rows_y, track, rows_h, mx, my);
    if max_y > 0.0 {
        let thumb_h = (rows_h / total_h.max(1.0) * rows_h).max(28.0 * s).min(rows_h);
        let ratio = (state.grid.scroll_y.current * s / max_y).clamp(0.0,1.0);
        renderer.push_rounded_rect(sy_x + 3.0 * s, rows_y + ratio * (rows_h - thumb_h), 6.0 * s, thumb_h, 3.0 * s, [0.62,0.38,0.82,0.9]);
    }
    let content_w = state.grid.content_width(metadata) * s;
    let max_x = (content_w - data_w).max(0.0);
    let sx_y = y + body_h;
    renderer.push_rect(x, sx_y, body_w, track, [0.055,0.058,0.075,1.0]);
    ui.register_rect(UiId::DatabaseTableScrollX, data_x, sx_y, data_w, track, mx, my);
    if max_x > 0.0 {
        let thumb_w = (data_w / content_w.max(1.0) * data_w).max(36.0 * s).min(data_w);
        let ratio = (state.grid.scroll_x.current * s / max_x).clamp(0.0,1.0);
        renderer.push_rounded_rect(data_x + ratio * (data_w - thumb_w), sx_y + 3.0 * s, thumb_w, 6.0 * s, 3.0 * s, [0.62,0.38,0.82,0.9]);
    }
}
