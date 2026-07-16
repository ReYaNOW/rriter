use crate::renderer::Renderer;
use crate::ui_system::{UiId, UiRegistry};
use crate::widgets::{ButtonView, IconType};

const QUERY_TOOLBAR_H: f32 = 40.0;
const QUERY_RESULT_ROW_H: f32 = 24.0;

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_database_query_chrome(
        &mut self,
        x: f32,
        toolbar_y: f32,
        w: f32,
        results_y: f32,
        results_h: f32,
        s: f32,
        meta: &crate::app::database::DatabaseQueryTabMeta,
        state: &crate::app::database::DatabaseQueryTabState,
        history: &[crate::app::database::DatabaseQueryHistoryEntry],
        ui: &mut UiRegistry,
        mx: f32,
        my: f32,
        modal_mx: f32,
        modal_my: f32,
    ) {
        self.draw_database_query_toolbar(x, toolbar_y, w, s, meta, state, ui, mx, my);
        if results_h > 0.0 {
            self.draw_database_query_results(
                x,
                results_y,
                w,
                results_h,
                s,
                meta,
                state,
                history,
                ui,
                mx,
                my,
            );
        }
        if let Some(review) = state.review.as_ref() {
            self.draw_database_query_review(review, s, ui, modal_mx, modal_my);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_database_query_toolbar(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        s: f32,
        meta: &crate::app::database::DatabaseQueryTabMeta,
        state: &crate::app::database::DatabaseQueryTabState,
        ui: &mut UiRegistry,
        mx: f32,
        my: f32,
    ) {
        self.push_rect(x, y, w, QUERY_TOOLBAR_H * s, [0.10, 0.105, 0.13, 1.0]);
        let can_run = !state.running && state.review.is_none();
        let can_cancel = state.running || state.review.is_some();
        let mut bx = x + 8.0 * s;
        for (id, text, icon, active, primary, width) in [
            (UiId::DatabaseQueryRun, "Выполнить", Some(IconType::Run), can_run, true, 106.0),
            (UiId::DatabaseQueryCancel, "Отмена", Some(IconType::Cancel), can_cancel, false, 92.0),
            (UiId::DatabaseQueryExplain, "Explain", None, can_run, false, 82.0),
            (
                UiId::DatabaseQueryExplainAnalyze,
                "Analyze",
                None,
                can_run,
                false,
                84.0,
            ),
            (UiId::DatabaseQueryFormat, "Формат", None, can_run, false, 88.0),
            (UiId::DatabaseQueryHistory, "История", Some(IconType::Time), true, false, 96.0),
        ] {
            draw_query_button(
                self,
                ui,
                id,
                bx,
                y + 5.0 * s,
                width * s,
                30.0 * s,
                text,
                icon,
                active,
                primary,
                mx,
                my,
                s,
            );
            bx += (width + 5.0) * s;
        }
        let status = if state.running {
            "Выполняется…".to_string()
        } else if state.review.is_some() {
            "Ожидает Apply / Cancel".to_string()
        } else if let Some(error) = state.error.as_deref() {
            format!("Ошибка: {error}")
        } else if state.last_duration_ms > 0 {
            format!("{} мс · {} строк", state.last_duration_ms, state.last_affected_rows)
        } else {
            format!("{} · SQL", meta.database_name)
        };
        let status_x = (x + w - self.measure_ui_width(&status, 0.68) - 14.0 * s).max(bx);
        self.draw_string_scaled_stable(
            &status,
            status_x,
            y + 25.0 * s,
            if state.error.is_some() {
                [0.95, 0.38, 0.42, 1.0]
            } else {
                self.theme.line_num
            },
            0.68,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_database_query_results(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        meta: &crate::app::database::DatabaseQueryTabMeta,
        state: &crate::app::database::DatabaseQueryTabState,
        history: &[crate::app::database::DatabaseQueryHistoryEntry],
        ui: &mut UiRegistry,
        mx: f32,
        my: f32,
    ) {
        ui.register_blocker(UiId::DatabaseQueryResultBody, x, y, w, h, mx, my);
        self.push_rect(x, y, w, h, [0.07, 0.073, 0.092, 1.0]);
        let tabs_h = 34.0 * s;
        self.push_rect(x, y, w, tabs_h, [0.105, 0.11, 0.14, 1.0]);
        let mut tab_x = x + 8.0 * s;
        for (index, result) in state.results.iter().enumerate() {
            let width = (self.measure_ui_width(&result.title, 0.72) + 26.0 * s).max(86.0 * s);
            draw_query_tab(
                self,
                ui,
                UiId::DatabaseQueryResultTab(index),
                tab_x,
                y + 4.0 * s,
                width,
                26.0 * s,
                &result.title,
                state.result_view.active_result == index && !state.history_open,
                mx,
                my,
                s,
            );
            tab_x += width + 4.0 * s;
        }
        draw_query_tab(
            self,
            ui,
            UiId::DatabaseQueryMessagesTab,
            tab_x,
            y + 4.0 * s,
            94.0 * s,
            26.0 * s,
            "Messages",
            state.result_view.active_result >= state.results.len() && !state.history_open,
            mx,
            my,
            s,
        );

        let body_y = y + tabs_h;
        let body_h = (h - tabs_h).max(0.0);
        if state.history_open {
            self.draw_database_query_history(
                x,
                body_y,
                w,
                body_h,
                s,
                meta,
                history,
                state.result_view.scroll_y.max(0) as f32,
                ui,
                mx,
                my,
            );
        } else if state.result_view.active_result < state.results.len() {
            self.draw_database_query_result_set(
                x,
                body_y,
                w,
                body_h,
                s,
                &state.results[state.result_view.active_result],
                state,
            );
        } else {
            self.draw_database_query_messages(x, body_y, w, body_h, s, state);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_database_query_history(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        meta: &crate::app::database::DatabaseQueryTabMeta,
        history: &[crate::app::database::DatabaseQueryHistoryEntry],
        scroll_y: f32,
        ui: &mut UiRegistry,
        mx: f32,
        my: f32,
    ) {
        let row_h = 34.0 * s;
        let first = (scroll_y / row_h).floor() as usize;
        let mut visible = 0usize;
        for (index, entry) in history
            .iter()
            .rev()
            .filter(|entry| {
                entry.connection_id == meta.connection_id
                    && entry.database_name == meta.database_name
            })
            .skip(first)
            .take((h / row_h).ceil() as usize + 1)
            .enumerate()
        {
            let row_y = y + index as f32 * row_h;
            let hovered = mx >= x && mx <= x + w && my >= row_y && my <= row_y + row_h;
            if hovered {
                self.push_rect(x, row_y, w, row_h, [0.16, 0.17, 0.21, 1.0]);
            }
            ui.register_rect(
                UiId::DatabaseQueryHistoryEntry(first + index),
                x,
                row_y,
                w,
                row_h,
                mx,
                my,
            );
            let status = if entry.succeeded { "OK" } else { "ERR" };
            let one_line = entry.sql.lines().next().unwrap_or_default();
            self.draw_string_scaled_stable(
                &format!("{status} · {} мс · {}", entry.duration_ms, one_line),
                x + 10.0 * s,
                row_y + 22.0 * s,
                if entry.succeeded {
                    [0.48, 0.83, 0.58, 1.0]
                } else {
                    [0.95, 0.38, 0.42, 1.0]
                },
                0.68,
            );
            visible += 1;
        }
        if visible == 0 {
            self.draw_string_scaled_stable(
                "История запросов для этой базы пуста",
                x + 14.0 * s,
                y + 28.0 * s,
                self.theme.line_num,
                0.75,
            );
        }
    }

    fn draw_database_query_result_set(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        result: &crate::app::database::DatabaseQueryResultSet,
        state: &crate::app::database::DatabaseQueryTabState,
    ) {
        if result.columns.is_empty() {
            self.draw_string_scaled_stable(
                if result.command_tag.is_empty() {
                    "Команда выполнена"
                } else {
                    &result.command_tag
                },
                x + 14.0 * s,
                y + 28.0 * s,
                self.theme.fg,
                0.78,
            );
            return;
        }
        let row_h = QUERY_RESULT_ROW_H * s;
        let header_h = 28.0 * s;
        let column_w = 180.0 * s;
        self.push_rect(x, y, w, header_h, [0.12, 0.125, 0.155, 1.0]);
        let scroll_x = state.result_view.scroll_x.max(0) as f32;
        let scroll_y = state.result_view.scroll_y.max(0) as usize;
        let first_column = (scroll_x / column_w).floor() as usize;
        let first_row = (scroll_y as f32 / row_h).floor() as usize;
        let visible_columns = ((w / column_w).ceil() as usize + 1)
            .min(result.columns.len().saturating_sub(first_column));
        let visible_rows = (((h - header_h) / row_h).ceil() as usize + 1)
            .min(result.rows.len().saturating_sub(first_row));
        for column in first_column..first_column + visible_columns {
            let cx = x + (column - first_column) as f32 * column_w;
            self.draw_string_scaled_stable(
                &result.columns[column],
                cx + 8.0 * s,
                y + 20.0 * s,
                self.theme.fg,
                0.70,
            );
            self.push_rect(cx + column_w - 1.0, y, 1.0, h, [1.0, 1.0, 1.0, 0.08]);
        }
        for row_index in first_row..first_row + visible_rows {
            let ry = y + header_h + (row_index - first_row) as f32 * row_h;
            if row_index % 2 == 1 {
                self.push_rect(x, ry, w, row_h, [1.0, 1.0, 1.0, 0.025]);
            }
            for column in first_column..first_column + visible_columns {
                let cx = x + (column - first_column) as f32 * column_w;
                if let Some(cell) = result.rows[row_index].get(column) {
                    let text = cell.display_text();
                    self.draw_string_scaled_stable(
                        text,
                        cx + 8.0 * s,
                        ry + 17.0 * s,
                        if cell.value.is_some() {
                            self.theme.fg
                        } else {
                            self.theme.line_num
                        },
                        0.67,
                    );
                }
            }
        }
    }

    fn draw_database_query_messages(
        &mut self,
        x: f32,
        y: f32,
        _w: f32,
        h: f32,
        s: f32,
        state: &crate::app::database::DatabaseQueryTabState,
    ) {
        let row_h = 28.0 * s;
        let first = (state.result_view.scroll_y.max(0) as f32 / row_h).floor() as usize;
        for (index, message) in state
            .messages
            .iter()
            .skip(first)
            .take((h / row_h).ceil() as usize + 1)
            .enumerate()
        {
            let text = match (&message.detail, &message.hint) {
                (Some(detail), Some(hint)) => format!(
                    "{}: {} · {} · {}",
                    message.severity, message.message, detail, hint
                ),
                (Some(detail), None) => {
                    format!("{}: {} · {}", message.severity, message.message, detail)
                }
                _ => format!("{}: {}", message.severity, message.message),
            };
            self.draw_string_scaled_stable(
                &text,
                x + 10.0 * s,
                y + (index as f32 + 1.0) * row_h - 7.0 * s,
                self.theme.fg,
                0.68,
            );
        }
        if state.messages.is_empty() {
            self.draw_string_scaled_stable(
                "Сообщений PostgreSQL нет",
                x + 12.0 * s,
                y + 28.0 * s,
                self.theme.line_num,
                0.72,
            );
        }
    }

    fn draw_database_query_review(
        &mut self,
        review: &crate::app::database::DatabaseQueryReviewState,
        s: f32,
        ui: &mut UiRegistry,
        mx: f32,
        my: f32,
    ) {
        let width = self.width;
        let height = self.height;
        self.push_rect(0.0, 0.0, width, height, [0.0, 0.0, 0.0, 0.66]);
        ui.register_blocker(UiId::DatabaseQueryReviewBackdrop, 0.0, 0.0, width, height, mx, my);
        let w = (700.0 * s).min(width - 40.0 * s).max(320.0 * s);
        let h = (430.0 * s).min(height - 40.0 * s).max(260.0 * s);
        let x = ((width - w) * 0.5).round();
        let y = ((height - h) * 0.5).round();
        self.push_rounded_rect_border(
            x,
            y,
            w,
            h,
            10.0 * s,
            1.0,
            [0.35, 0.38, 0.46, 1.0],
            [0.10, 0.105, 0.13, 1.0],
        );
        ui.register_blocker(UiId::DatabaseQueryReviewBody, x, y, w, h, mx, my);
        self.draw_string_scaled_stable(
            "Результат выполнен в неподтверждённой транзакции",
            x + 24.0 * s,
            y + 42.0 * s,
            self.theme.fg,
            1.0,
        );
        let remaining = review
            .deadline_unix_ms
            .saturating_sub(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis(),
            )
            / 1_000;
        let summary = format!(
            "{} result set(s) · {} affected row(s) · {} notice(s) · {} ms · rollback через {} сек.",
            review.result_sets.len(),
            review.affected_rows,
            review.messages.len(),
            review.duration_ms,
            remaining
        );
        self.draw_string_scaled_stable(
            &summary,
            x + 24.0 * s,
            y + 76.0 * s,
            self.theme.line_num,
            0.78,
        );
        let mut detail_y = y + 112.0 * s;
        for result in review.result_sets.iter().take(6) {
            self.draw_string_scaled_stable(
                &format!(
                    "{}: {} row(s){}",
                    result.title,
                    result.rows.len().max(result.affected_rows as usize),
                    if result.truncated { " (ограничено)" } else { "" }
                ),
                x + 28.0 * s,
                detail_y,
                self.theme.fg,
                0.73,
            );
            detail_y += 23.0 * s;
            if let Some(row) = result.rows.first() {
                let preview = row
                    .iter()
                    .take(4)
                    .map(|cell| {
                        let text = cell.display_text();
                        if text.chars().count() > 48 {
                            format!("{}…", text.chars().take(48).collect::<String>())
                        } else {
                            text.to_string()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" | ");
                self.draw_string_scaled_stable(
                    &preview,
                    x + 42.0 * s,
                    detail_y,
                    self.theme.line_num,
                    0.66,
                );
                detail_y += 22.0 * s;
            }
        }
        for notice in review.messages.iter().take(3) {
            self.draw_string_scaled_stable(
                &format!("{}: {}", notice.severity, notice.message),
                x + 28.0 * s,
                detail_y,
                [0.95, 0.72, 0.32, 1.0],
                0.68,
            );
            detail_y += 22.0 * s;
        }
        let button_y = y + h - 54.0 * s;
        draw_query_button(
            self,
            ui,
            UiId::DatabaseQueryCommit,
            x + w - 246.0 * s,
            button_y,
            104.0 * s,
            34.0 * s,
            "Apply",
            Some(IconType::Check),
            true,
            true,
            mx,
            my,
            s,
        );
        draw_query_button(
            self,
            ui,
            UiId::DatabaseQueryRollback,
            x + w - 130.0 * s,
            button_y,
            104.0 * s,
            34.0 * s,
            "Cancel",
            Some(IconType::Cancel),
            true,
            false,
            mx,
            my,
            s,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_query_button(
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
    primary: bool,
    mx: f32,
    my: f32,
    s: f32,
) {
    let hit_mx = if active { mx } else { -1.0 };
    let hit_my = if active { my } else { -1.0 };
    let view = ButtonView {
        x,
        y,
        w,
        h,
        text,
        icon,
        text_scale: 0.72,
        icon_size: 16.0 * s,
    };
    let hovered = if primary {
        let hovered = view.is_hovered(hit_mx, hit_my);
        let background = if !active {
            [0.10, 0.20, 0.14, 0.75]
        } else if hovered {
            [0.20, 0.58, 0.31, 1.0]
        } else {
            [0.16, 0.48, 0.26, 1.0]
        };
        renderer.push_rounded_rect_border(
            x.round(),
            y.round(),
            w.round(),
            h.round(),
            4.0 * s,
            (1.0 * s).round().max(1.0),
            [0.32, 0.76, 0.43, 1.0],
            background,
        );
        let text_width = renderer.measure_ui_width(text, 0.72);
        let icon_size = 16.0 * s;
        let content_width = text_width
            + if icon.is_some() {
                icon_size + if text.is_empty() { 0.0 } else { 8.0 * s }
            } else {
                0.0
            };
        let mut content_x = x + (w - content_width) * 0.5;
        if let Some(icon) = icon {
            renderer.draw_atlas_icon(
                icon,
                content_x,
                y + (h - icon_size) * 0.5,
                icon_size,
                renderer.theme.fg,
            );
            content_x += icon_size + if text.is_empty() { 0.0 } else { 8.0 * s };
        }
        renderer.draw_string_scaled_stable(
            text,
            content_x,
            y + h * 0.5 + 5.0 * s,
            renderer.theme.fg,
            0.72,
        );
        hovered
    } else {
        view.render(renderer, hit_mx, hit_my, s, false)
    };
    if !active {
        renderer.push_rounded_rect(x, y, w, h, 4.0 * s, [0.08, 0.08, 0.09, 0.55]);
    }
    if active {
        ui.register_rect(id, x, y, w, h, mx, my);
    } else if hovered {
        ui.register_blocker(id, x, y, w, h, mx, my);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_query_tab(
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
    let hovered = mx >= x && mx <= x + w && my >= y && my <= y + h;
    renderer.push_rounded_rect(
        x,
        y,
        w,
        h,
        5.0 * s,
        if active {
            [0.28, 0.24, 0.38, 1.0]
        } else if hovered {
            [0.18, 0.19, 0.24, 1.0]
        } else {
            [0.13, 0.135, 0.17, 1.0]
        },
    );
    renderer.draw_string_scaled_stable(
        text,
        x + 10.0 * s,
        y + 18.0 * s,
        renderer.theme.fg,
        0.68,
    );
    ui.register_rect(id, x, y, w, h, mx, my);
}
