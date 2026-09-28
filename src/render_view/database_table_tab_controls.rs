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
    let padding = crate::app::database_table_input_padding(s, cell_editor);
    let text_geometry = crate::app::database_table_input_text_geometry(x, w, s, cell_editor);
    let edge_pad = crate::app::single_line_input::single_line_cursor_edge_pad(s);
    let scroll_x = crate::app::single_line_input::single_line_cursor_geometry(
        input.text(),
        input.cursor,
        text_geometry.content_w,
        0.0,
        edge_pad,
        edge_pad,
        |ch| renderer.one_line_ui_advance(ch, text_scale),
    )
    .scroll_x;
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
    let loaded = state.grid.loaded_server_row_count_on_page();
    match state.grid.count {
        Some(0) => "0 из 0".to_string(),
        Some(count) if loaded > 0 => state
            .grid
            .loaded_server_row_bounds_on_page()
            .map_or_else(
                || format!("Загружено {loaded} · всего {count}"),
                |(first, last)| format!("{}–{} из {}", first + 1, last + 1, count),
            ),
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
        .map_or(0, |duration| {
            (duration.as_millis() / crate::app::database::DATABASE_REFRESH_INDICATOR_STEP_MS % 8)
                as usize
        });
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
