use crate::renderer::Renderer;
use crate::ui_system::{UiId, UiRegistry};
use crate::widgets::{ButtonStyle, ButtonView, IconType};
use glow::HasContext;

const QUERY_TOOLBAR_H: f32 = 40.0;
thread_local! {
    static DATABASE_SQL_SPANS_CACHE: std::cell::RefCell<
        std::collections::HashMap<String, Vec<crate::highlighter::ColorSpan>>
    > = std::cell::RefCell::new(std::collections::HashMap::new());
}

fn with_cached_database_sql_spans<R>(
    sql: &str,
    callback: impl FnOnce(&[crate::highlighter::ColorSpan]) -> R,
) -> R {
    DATABASE_SQL_SPANS_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() > 256 && !cache.contains_key(sql) {
            cache.clear();
        }
        let spans = cache
            .entry(sql.to_owned())
            .or_insert_with(|| crate::highlighter::highlight_sql_text(sql));
        callback(spans)
    })
}

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
            self.draw_database_query_result_resize(
                x,
                results_y,
                w,
                s,
                state.result_view.is_resizing_height,
                ui,
                mx,
                my,
            );
        }
        if let Some(review) = state.review.as_ref() {
            self.draw_database_query_review(
                meta,
                state,
                history,
                review,
                s,
                ui,
                modal_mx,
                modal_my,
            );
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
        let x = x.round();
        let y = y.round();
        let w = w.round();
        let toolbar_h = (QUERY_TOOLBAR_H * s).round();
        self.push_rect(x, y, w, toolbar_h, [0.10, 0.105, 0.13, 1.0]);
        let can_run = !state.running && state.review.is_none();
        let can_cancel = state.running || state.review.is_some();
        let mut bx = x + (8.0 * s).round();
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
                y + (5.0 * s).round(),
                (width * s).round(),
                (30.0 * s).round(),
                text,
                icon,
                active,
                primary,
                mx,
                my,
                s,
            );
            bx = (bx + (width + 5.0) * s).round();
        }
        let analysis_errors = state
            .editor_diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == crate::lsp::DiagSeverity::Error)
            .count();
        let analysis_warnings = state
            .editor_diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.severity == crate::lsp::DiagSeverity::Warning)
            .count();
        let status = if state.running {
            "Выполняется…".to_string()
        } else if state.review.is_some() {
            "Ожидает применения или отмены".to_string()
        } else if let Some(error) = state.error.as_deref() {
            format!("Ошибка: {error}")
        } else if analysis_errors > 0 || analysis_warnings > 0 {
            format!("SQL-анализ: {analysis_errors} ошибок · {analysis_warnings} предупреждений")
        } else if state.last_duration_ms > 0 {
            format!(
                "{} мс · получено {} · изменено {}",
                state.last_duration_ms, state.last_returned_rows, state.last_changed_rows
            )
        } else {
            format!("{} · SQL-консоль", meta.database_name)
        };
        let status_w = self.measure_ui_width(&status, 0.68);
        let status_x = (x + w - status_w - 14.0 * s)
            .max(bx)
            .round();
        self.draw_string_scaled_pixel_snapped(
            &status,
            status_x,
            Self::tree_row_text_y(y, toolbar_h, s),
            if state.error.is_some() || analysis_errors > 0 {
                [0.95, 0.38, 0.42, 1.0]
            } else if analysis_warnings > 0 {
                [0.95, 0.72, 0.30, 1.0]
            } else {
                self.theme.line_num
            },
            0.68,
        );
        if !state.editor_diagnostics.is_empty() {
            ui.register_rect(
                UiId::DatabaseQueryNextDiagnostic,
                status_x,
                y,
                status_w,
                toolbar_h,
                mx,
                my,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_database_query_result_resize(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        s: f32,
        resizing: bool,
        ui: &mut UiRegistry,
        mx: f32,
        my: f32,
    ) {
        let hit_h = (10.0 * s).round().max(8.0);
        let hit_y = (y - hit_h * 0.5).round();
        let hovered = mx >= x && mx <= x + w && my >= hit_y && my <= hit_y + hit_h;
        let line_h = if hovered || resizing { 2.0 } else { 1.0 };
        self.push_rect(
            x.round(),
            (y - line_h * 0.5).round(),
            w.round(),
            line_h,
            if hovered || resizing {
                crate::render_view::IDE_RESIZE_HIGHLIGHT_COLOR
            } else {
                [0.35, 0.38, 0.46, 0.65]
            },
        );
        ui.register_blocker(
            UiId::DatabaseQueryResultResize,
            x.round(),
            hit_y,
            w.round(),
            hit_h,
            mx,
            my,
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
        let x = x.round();
        let y = y.round();
        let w = w.round();
        let h = h.round();
        self.push_rect(x, y, w, h, [0.07, 0.073, 0.092, 1.0]);
        let tabs_h = (36.0 * s).round();
        self.push_rect(x, y, w, tabs_h, [0.105, 0.11, 0.14, 1.0]);
        let mut tab_x = x + (8.0 * s).round();
        for (index, result) in state.results.iter().enumerate() {
            let width = (self.measure_ui_width(&result.title, 0.76) + 26.0 * s)
                .max(90.0 * s)
                .round();
            draw_query_tab(
                self,
                ui,
                UiId::DatabaseQueryResultTab(index),
                tab_x,
                y + (4.0 * s).round(),
                width,
                (28.0 * s).round(),
                &result.title,
                state.result_view.active_result == index && !state.history_open,
                mx,
                my,
                s,
            );
            tab_x = (tab_x + width + 4.0 * s).round();
        }

        let result_active = !state.history_open
            && state.result_view.active_result < state.results.len();
        let summary_h = if result_active { (30.0 * s).round() } else { 0.0 };
        if result_active {
            let result = &state.results[state.result_view.active_result];
            self.push_rect(x, y + tabs_h, w, summary_h, [0.085, 0.09, 0.115, 1.0]);
            let summary = database_query_execution_summary(state, result);
            let mut scratch = String::new();
            self.draw_tree_label_clipped(
                &summary,
                x + (10.0 * s).round(),
                Self::tree_row_text_y(y + tabs_h, summary_h, s),
                (w - 20.0 * s).max(4.0),
                self.theme.line_num,
                0.72,
                &mut scratch,
            );
        }

        let grid_y = y + tabs_h + summary_h;
        let grid_h = (h - tabs_h - summary_h).max(0.0);
        let scrollbar = (10.0 * s).round().max(10.0);
        if state.history_open {
            let content_h = history
                .iter()
                .filter(|entry| {
                    entry.connection_id == meta.connection_id
                        && entry.database_name == meta.database_name
                })
                .map(|entry| {
                    crate::app::database::database_query_history_entry_height(&entry.sql) * s
                })
                .sum::<f32>();
            let layout = crate::app::database::database_grid_layout(
                x, grid_y, w, grid_h, 0.0, scrollbar, 0.0, 0.0, content_h,
            );
            let max_y = (content_h - layout.body_rect.h).max(0.0);
            ui.register_blocker(
                UiId::DatabaseQueryResultBody,
                layout.body_rect.x,
                layout.body_rect.y,
                layout.body_rect.w,
                layout.body_rect.h,
                mx,
                my,
            );
            self.draw_database_query_history(
                layout.body_rect.x,
                layout.body_rect.y,
                layout.body_rect.w,
                layout.body_rect.h,
                s,
                meta,
                history,
                state.result_view.scroll_y.current.clamp(0.0, max_y),
                ui,
                mx,
                my,
            );
            self.draw_database_query_scrollbars(
                layout, 0.0, max_y, state, ui, mx, my, s,
            );
            return;
        }

        let Some(result) = state.results.get(state.result_view.active_result) else {
            ui.register_blocker(UiId::DatabaseQueryResultBody, x, grid_y, w, grid_h, mx, my);
            let hint = if state.editor_diagnostics.is_empty() {
                "Нет результатов"
            } else {
                "Ошибки SQL доступны в панели «Ляпы»"
            };
            self.draw_string_scaled_pixel_snapped(
                hint,
                x + (14.0 * s).round(),
                Self::tree_row_text_y(grid_y, (36.0 * s).round(), s),
                self.theme.line_num,
                0.80,
            );
            return;
        };

        let header_h = if result.columns.is_empty() {
            0.0
        } else {
            (crate::app::database::DATABASE_GRID_HEADER_HEIGHT * s).round()
        };
        let row_h = (crate::app::database::DATABASE_GRID_ROW_HEIGHT * s).round();
        let content_w = crate::app::database::database_columns_content_width(
            &state.result_view.column_widths,
            result.columns.iter().map(String::as_str),
        ) * s;
        let total_rows_h = result.rows.len() as f32 * row_h;
        let layout = crate::app::database::database_grid_layout(
            x, grid_y, w, grid_h, 0.0, scrollbar, header_h, content_w, total_rows_h,
        );
        let max_x = (content_w - layout.body_rect.w).max(0.0);
        let max_y = crate::app::database::database_grid_max_scroll(
            result.rows.len(),
            row_h,
            layout.body_rect.h,
        );
        ui.register_blocker(
            UiId::DatabaseQueryResultBody,
            layout.body_rect.x,
            layout.body_rect.y,
            layout.body_rect.w,
            layout.body_rect.h,
            mx,
            my,
        );
        self.draw_database_query_result_set(
            layout,
            row_h,
            s,
            result,
            state,
            state.result_view.scroll_x.current.clamp(0.0, max_x),
            state.result_view.scroll_y.current.clamp(0.0, max_y),
            ui,
            mx,
            my,
        );
        self.draw_database_query_scrollbars(
            layout, max_x, max_y, state, ui, mx, my, s,
        );
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
        let x = x.round();
        let y = y.round();
        let w = w.round();
        let h = h.round();
        let padding = (10.0 * s).round();
        let mut content_y = 0.0f32;
        let mut matching = 0usize;
        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            self.gl.scissor(
                x as i32,
                (self.height - (y + h)).round().max(0.0) as i32,
                w.round().max(0.0) as i32,
                h.round().max(0.0) as i32,
            );
        }
        for (visible_index, entry) in history
            .iter()
            .rev()
            .filter(|entry| {
                entry.connection_id == meta.connection_id
                    && entry.database_name == meta.database_name
            })
            .enumerate()
        {
            matching += 1;
            let entry_h =
                (crate::app::database::database_query_history_entry_height(&entry.sql) * s)
                    .round();
            let row_y = (y + content_y - scroll_y).round();
            content_y += entry_h;
            if row_y + entry_h <= y || row_y >= y + h {
                continue;
            }
            let hovered = mx >= x && mx <= x + w && my >= row_y && my <= row_y + entry_h;
            self.push_rect(
                x,
                row_y,
                w,
                entry_h,
                if hovered {
                    [0.16, 0.17, 0.21, 1.0]
                } else {
                    [0.085, 0.09, 0.115, 1.0]
                },
            );
            self.push_rect(
                x,
                row_y + entry_h - 1.0,
                w,
                1.0,
                [0.52, 0.55, 0.62, 0.16],
            );
            ui.register_rect(
                UiId::DatabaseQueryHistoryEntry(visible_index),
                x,
                row_y,
                w,
                entry_h,
                mx,
                my,
            );
            let status = if entry.succeeded { "OK" } else { "ERR" };
            let meta_text = format!(
                "{status} · {} мс · {} строк",
                entry.duration_ms, entry.affected_rows
            );
            self.draw_string_scaled_pixel_snapped(
                &meta_text,
                x + padding,
                row_y + (21.0 * s).round(),
                if entry.succeeded {
                    [0.48, 0.83, 0.58, 1.0]
                } else {
                    [0.95, 0.38, 0.42, 1.0]
                },
                0.76,
            );
            with_cached_database_sql_spans(&entry.sql, |spans| {
                let mut byte_offset = 0usize;
                let mut line_y = row_y + (43.0 * s).round();
                let mut drew_line = false;
                for raw_line in entry.sql.split_inclusive('\n').take(20) {
                    drew_line = true;
                    let line = raw_line.trim_end_matches(&['\r', '\n'][..]);
                    self.draw_database_sql_line(
                        line,
                        byte_offset,
                        spans,
                        x + padding,
                        line_y,
                        x + w - padding,
                    );
                    byte_offset = byte_offset.saturating_add(raw_line.len());
                    line_y += (20.0 * s).round();
                }
                if !drew_line {
                    self.draw_database_sql_line(
                        "",
                        0,
                        spans,
                        x + padding,
                        line_y,
                        x + w - padding,
                    );
                }
                if crate::app::database::database_query_history_is_truncated(&entry.sql) {
                    self.draw_string_scaled_pixel_snapped(
                        "…",
                        x + padding,
                        line_y,
                        self.theme.line_num,
                        0.80,
                    );
                }
            });
        }
        self.flush();
        unsafe { self.gl.disable(glow::SCISSOR_TEST) };
        if matching == 0 {
            self.draw_string_scaled_pixel_snapped(
                "История запросов для этой базы пуста",
                x + (14.0 * s).round(),
                Self::tree_row_text_y(y, (36.0 * s).round(), s),
                self.theme.line_num,
                0.80,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_database_query_result_set(
        &mut self,
        layout: crate::app::database::DatabaseGridLayout,
        row_h: f32,
        s: f32,
        result: &crate::app::database::DatabaseQueryResultSet,
        state: &crate::app::database::DatabaseQueryTabState,
        scroll_x: f32,
        scroll_y: f32,
        ui: &mut UiRegistry,
        mx: f32,
        my: f32,
    ) {
        let x = layout.header_rect.x.round();
        let y = layout.header_rect.y.round();
        let w = layout.header_rect.w.round();
        let header_h = layout.header_rect.h.round();
        let body_h = layout.body_rect.h.round();
        if result.columns.is_empty() {
            self.draw_string_scaled_pixel_snapped(
                if result.command_kind.is_empty() {
                    "Команда выполнена"
                } else {
                    &result.command_kind
                },
                x + (14.0 * s).round(),
                Self::tree_row_text_y(y, (30.0 * s).round(), s),
                self.theme.fg,
                0.82,
            );
            return;
        }
        let scroll_x = scroll_x.round();
        let scroll_y = scroll_y.max(0.0);
        let content_start = scroll_x / s.max(f32::EPSILON);
        let content_end = content_start + w / s.max(f32::EPSILON);
        let mut column_x = 0.0;
        let mut visible_columns = Vec::new();
        for (index, name) in result.columns.iter().enumerate() {
            let width = crate::app::database::database_column_width(
                &state.result_view.column_widths,
                name,
            );
            if column_x + width >= content_start && column_x <= content_end {
                visible_columns.push((index, column_x, width));
            }
            column_x += width;
        }
        let visible_rows = crate::app::database::database_grid_visible_row_range(
            scroll_y,
            row_h,
            body_h,
            result.rows.len(),
        );
        let body_y = layout.body_rect.y.round();
        let guide = [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.15];
        let mut scratch = String::new();

        self.push_rect(x, y, w, header_h, [0.12, 0.125, 0.155, 1.0]);
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
        let header_baseline = Self::tree_row_text_y(y, header_h, s).round();
        for (column, column_x, column_width) in &visible_columns {
            let cx = (x + column_x * s - scroll_x).round();
            let draw_w = (column_width * s).round().max(1.0);
            self.draw_tree_label_clipped(
                &result.columns[*column],
                cx + (8.0 * s).round(),
                header_baseline,
                (draw_w - 16.0 * s).max(4.0),
                self.theme.fg,
                0.78,
                &mut scratch,
            );
            let divider_x = (cx + draw_w - 3.0 * s).round();
            ui.register_rect(
                UiId::DatabaseQueryColumnResize(*column),
                divider_x,
                y,
                (6.0 * s).round(),
                header_h,
                mx,
                my,
            );
            self.push_rect((cx + draw_w - 1.0).round(), y, 1.0, header_h, guide);
        }
        self.flush();
        unsafe { self.gl.disable(glow::SCISSOR_TEST) };

        self.push_rect(x, body_y, w, body_h, [0.075, 0.078, 0.098, 1.0]);
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
        for row_index in visible_rows {
            let row_y = (body_y + row_index as f32 * row_h - scroll_y).round();
            if row_y + row_h <= body_y || row_y >= body_y + body_h {
                continue;
            }
            if row_index % 2 == 1 {
                self.push_rect(x, row_y, w, row_h, [1.0, 1.0, 1.0, 0.025]);
            }
            let row_baseline = Self::tree_row_text_y(row_y, row_h, s).round();
            for (column, column_x, column_width) in &visible_columns {
                let cx = (x + column_x * s - scroll_x).round();
                let draw_w = (column_width * s).round().max(1.0);
                if let Some(cell) = result.rows[row_index].get(*column) {
                    self.draw_tree_label_clipped(
                        cell.display_text(),
                        cx + (7.0 * s).round(),
                        row_baseline,
                        (draw_w - 14.0 * s).max(4.0),
                        if cell.value.is_some() { self.theme.fg } else { self.theme.line_num },
                        0.75,
                        &mut scratch,
                    );
                }
                self.push_rect((cx + draw_w - 1.0).round(), body_y, 1.0, body_h, guide);
            }
        }
        self.flush();
        unsafe { self.gl.disable(glow::SCISSOR_TEST) };
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_database_query_scrollbars(
        &mut self,
        layout: crate::app::database::DatabaseGridLayout,
        max_x: f32,
        max_y: f32,
        state: &crate::app::database::DatabaseQueryTabState,
        ui: &mut UiRegistry,
        mx: f32,
        my: f32,
        s: f32,
    ) {
        let track_color = [0.055, 0.058, 0.075, 1.0];
        let thumb_color = [0.62, 0.38, 0.82, 0.9];
        if let Some(rect) = layout.vertical_scrollbar_rect {
            self.push_rect(rect.x, rect.y, rect.w, rect.h, track_color);
            ui.register_rect(
                UiId::DatabaseQueryScrollY,
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                mx,
                my,
            );
            if let Some(thumb) = crate::scroll::scrollbar_thumb(
                rect.y,
                rect.h,
                layout.body_rect.h,
                layout.body_rect.h + max_y,
                state.result_view.scroll_y.current,
                (28.0 * s).round(),
            ) {
                self.push_rounded_rect(
                    rect.x + (2.0 * s).round(),
                    thumb.start.round(),
                    (rect.w - 4.0 * s).max(4.0).round(),
                    thumb.len.round(),
                    (3.0 * s).round(),
                    thumb_color,
                );
            }
        }
        if let Some(rect) = layout.horizontal_scrollbar_rect {
            self.push_rect(rect.x, rect.y, rect.w, rect.h, track_color);
            ui.register_rect(
                UiId::DatabaseQueryScrollX,
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                mx,
                my,
            );
            if let Some(thumb) = crate::scroll::scrollbar_thumb(
                rect.x,
                rect.w,
                layout.body_rect.w,
                layout.body_rect.w + max_x,
                state.result_view.scroll_x.current,
                (36.0 * s).round(),
            ) {
                self.push_rounded_rect(
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

    pub(crate) fn draw_database_sql_line(
        &mut self,
        line: &str,
        base_offset: usize,
        spans: &[crate::highlighter::ColorSpan],
        x: f32,
        y: f32,
        max_x: f32,
    ) {
        let mut draw_x = x;
        let mut offset = base_offset;
        let mut span_index = match spans.binary_search_by_key(&base_offset, |span| span.start) {
            Ok(index) => index,
            Err(index) => index.saturating_sub(1),
        };
        for ch in line.chars() {
            if draw_x > max_x {
                break;
            }
            let advance = self.char_advance(ch);
            if ch != ' ' && ch != '\t'
                && let Some(glyph) = self.get_glyph(ch)
            {
                while span_index < spans.len() && spans[span_index].end <= offset {
                    span_index += 1;
                }
                let color = if span_index < spans.len()
                    && spans[span_index].start <= offset
                    && offset < spans[span_index].end
                {
                    spans[span_index].color
                } else {
                    self.theme.fg
                };
                self.push_quad(
                    draw_x + glyph.offset_x,
                    y - glyph.offset_y,
                    glyph.width,
                    glyph.height,
                    glyph.u,
                    glyph.v,
                    glyph.uw,
                    glyph.vh,
                    color,
                    glyph.is_emoji,
                );
            }
            draw_x += advance;
            offset = offset.saturating_add(ch.len_utf8());
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_database_query_review(
        &mut self,
        meta: &crate::app::database::DatabaseQueryTabMeta,
        state: &crate::app::database::DatabaseQueryTabState,
        history: &[crate::app::database::DatabaseQueryHistoryEntry],
        review: &crate::app::database::DatabaseQueryReviewState,
        s: f32,
        ui: &mut UiRegistry,
        mx: f32,
        my: f32,
    ) {
        ui.mark_overlay_start();
        let width = self.width;
        let height = self.height;
        self.push_rect(0.0, 0.0, width, height, [0.0, 0.0, 0.0, 0.66]);
        ui.register_blocker(
            UiId::DatabaseQueryReviewBackdrop,
            0.0,
            0.0,
            width,
            height,
            mx,
            my,
        );
        let max_w = (width - 40.0 * s).max(1.0);
        let max_h = (height - 40.0 * s).max(1.0);
        let w = (920.0 * s).min(max_w).round();
        let h = (680.0 * s).min(max_h).round();
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

        let padding = (24.0 * s).round();
        let text_w = (w - padding * 2.0).max(40.0);
        let title = "Результат выполнен в неподтверждённой транзакции";
        let title_line_h = (25.0 * s).round().max(20.0);
        let title_ranges = crate::render_view::core_text::wrapped_text_ranges(
            title,
            text_w,
            |ch| self.char_advance(ch),
        );
        let mut text_y = (y + 18.0 * s).round();
        for (start, end) in title_ranges {
            self.draw_string_scaled_pixel_snapped(
                &title[start..end],
                x + padding,
                Self::tree_row_text_y(text_y, title_line_h, s),
                self.theme.fg,
                1.0,
            );
            text_y = (text_y + title_line_h).round();
        }
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
            "Получено строк: {} · Изменено строк: {} · Сообщений: {} · {} мс · автоотмена через {} сек.",
            review.returned_rows,
            review.changed_rows,
            state.messages.len(),
            review.duration_ms,
            remaining,
        );
        let summary_line_h = (20.0 * s).round().max(16.0);
        for (start, end) in crate::render_view::core_text::wrapped_text_ranges(
            &summary,
            text_w,
            |ch| self.char_advance(ch) * 0.76,
        ) {
            self.draw_string_scaled_pixel_snapped(
                &summary[start..end],
                x + padding,
                Self::tree_row_text_y(text_y, summary_line_h, s),
                self.theme.line_num,
                0.76,
            );
            text_y = (text_y + summary_line_h).round();
        }

        let button_h = (58.0 * s).round();
        let button_y = (y + h - button_h + 10.0 * s).round();
        let content_y = (text_y + 8.0 * s).round();
        let content_bottom = (button_y - 10.0 * s).round();
        let content_h = (content_bottom - content_y).max(180.0 * s);
        let grid_h = (content_h * 0.54).round().max(150.0 * s);
        let grid_x = x + padding;
        let grid_w = (w - padding * 2.0).max(1.0);
        self.draw_database_query_results(
            grid_x,
            content_y,
            grid_w,
            grid_h,
            s,
            meta,
            state,
            history,
            ui,
            mx,
            my,
        );

        let messages_y = (content_y + grid_h + 10.0 * s).round();
        let messages_h = (content_bottom - messages_y).max(70.0 * s);
        self.draw_database_query_review_messages(
            grid_x,
            messages_y,
            grid_w,
            messages_h,
            s,
            state,
            ui,
            mx,
            my,
        );

        draw_query_button(
            self,
            ui,
            UiId::DatabaseQueryCommit,
            x + w - 286.0 * s,
            button_y,
            130.0 * s,
            36.0 * s,
            "Применить",
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
            x + w - 144.0 * s,
            button_y,
            120.0 * s,
            36.0 * s,
            "Отмена",
            Some(IconType::Cancel),
            true,
            false,
            mx,
            my,
            s,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_database_query_review_messages(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        state: &crate::app::database::DatabaseQueryTabState,
        ui: &mut UiRegistry,
        mx: f32,
        my: f32,
    ) {
        let x = x.round();
        let y = y.round();
        let w = w.round();
        let h = h.round();
        let header_h = (30.0 * s).round();
        let scrollbar_w = (10.0 * s).round().max(10.0);
        self.push_rounded_rect_border(
            x,
            y,
            w,
            h,
            6.0 * s,
            1.0,
            [0.35, 0.38, 0.46, 0.65],
            [0.075, 0.078, 0.098, 1.0],
        );
        self.draw_string_scaled_pixel_snapped(
            "Предупреждения и ошибки",
            x + (10.0 * s).round(),
            Self::tree_row_text_y(y, header_h, s),
            self.theme.fg,
            0.78,
        );
        let body_y = y + header_h;
        let body_h = (h - header_h).max(1.0);
        ui.register_blocker(
            UiId::DatabaseQueryReviewMessagesBody,
            x,
            body_y,
            w,
            body_h,
            mx,
            my,
        );

        let items = database_query_review_message_items(state);
        let pad = (10.0 * s).round();
        let line_h = (20.0 * s).round().max(16.0);
        let item_gap = (8.0 * s).round();
        let max_text_w = (w - pad * 2.0 - scrollbar_w).max(40.0);
        let mut layouts = Vec::with_capacity(items.len());
        let mut total_h = pad;
        for (text, color) in items {
            let ranges = crate::render_view::core_text::wrapped_text_ranges(
                &text,
                max_text_w,
                |ch| self.char_advance(ch) * 0.72,
            );
            total_h += ranges.len() as f32 * line_h + item_gap;
            layouts.push((text, color, ranges));
        }
        total_h += pad;
        let max_scroll = (total_h - body_h).max(0.0);
        state.result_view.review_message_max_scroll.set(max_scroll);
        let scroll_y = state
            .result_view
            .review_message_scroll_y
            .current
            .clamp(0.0, max_scroll);

        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            self.gl.scissor(
                x as i32,
                (self.height - (body_y + body_h)).round().max(0.0) as i32,
                (w - if max_scroll > 0.5 { scrollbar_w } else { 0.0 })
                    .round()
                    .max(0.0) as i32,
                body_h.round().max(0.0) as i32,
            );
        }
        let mut draw_y = (body_y + pad - scroll_y).round();
        for (text, color, ranges) in layouts {
            for (start, end) in ranges {
                if draw_y + line_h >= body_y && draw_y <= body_y + body_h {
                    self.draw_string_scaled_pixel_snapped(
                        &text[start..end],
                        x + pad,
                        Self::tree_row_text_y(draw_y, line_h, s),
                        color,
                        0.72,
                    );
                }
                draw_y = (draw_y + line_h).round();
            }
            draw_y = (draw_y + item_gap).round();
        }
        self.flush();
        unsafe { self.gl.disable(glow::SCISSOR_TEST) };

        if max_scroll > 0.5 {
            let track_x = (x + w - scrollbar_w).round();
            self.push_rect(
                track_x,
                body_y,
                scrollbar_w,
                body_h,
                [0.055, 0.058, 0.075, 1.0],
            );
            ui.register_rect(
                UiId::DatabaseQueryReviewMessagesScrollY,
                track_x,
                body_y,
                scrollbar_w,
                body_h,
                mx,
                my,
            );
            if let Some(thumb) = crate::scroll::scrollbar_thumb(
                body_y,
                body_h,
                body_h,
                body_h + max_scroll,
                scroll_y,
                (28.0 * s).round(),
            ) {
                self.push_rounded_rect(
                    track_x + (2.0 * s).round(),
                    thumb.start.round(),
                    (scrollbar_w - 4.0 * s).max(4.0).round(),
                    thumb.len.round(),
                    (3.0 * s).round(),
                    [0.62, 0.38, 0.82, 0.9],
                );
            }
        }
    }
}

fn database_query_review_message_items(
    state: &crate::app::database::DatabaseQueryTabState,
) -> Vec<(String, [f32; 4])> {
    let mut items = Vec::with_capacity(
        state
            .analysis
            .diagnostics
            .len()
            .saturating_add(state.messages.len())
            .saturating_add(1),
    );
    items.push((
        "Изменения ещё не подтверждены. Примените транзакцию или отмените её.".to_string(),
        [0.95, 0.72, 0.30, 1.0],
    ));
    items.extend(
        database_query_notice_items(state)
            .into_iter()
            .map(|(text, color, _)| (text, color)),
    );
    items
}

fn database_query_execution_summary(
    state: &crate::app::database::DatabaseQueryTabState,
    result: &crate::app::database::DatabaseQueryResultSet,
) -> String {
    let mut parts = vec![format!("Выполнено за {} мс", state.last_duration_ms)];
    let reports_returned = result.returned_rows > 0
        || !result.columns.is_empty()
        || matches!(result.command_kind.as_str(), "SELECT" | "EXPLAIN");
    if reports_returned {
        parts.push(format!("Получено строк: {}", result.returned_rows));
    }
    if result.affected_rows > 0 {
        parts.push(format!("Изменено строк: {}", result.affected_rows));
    }
    if !result.command_kind.is_empty() {
        parts.push(result.command_kind.clone());
    }
    if !state.messages.is_empty() {
        parts.push(format!("Уведомлений: {}", state.messages.len()));
    }
    if result.truncated {
        parts.push(format!("Показаны первые {}", result.rows.len()));
    }
    parts.join(" · ")
}

fn database_query_notice_items(
    state: &crate::app::database::DatabaseQueryTabState,
) -> Vec<(String, [f32; 4], Option<usize>)> {
    state
        .messages
        .iter()
        .map(|message| {
            let mut text = format!("{}: {}", message.severity, message.message);
            if let Some(detail) = message.detail.as_deref() {
                text.push_str(" · ");
                text.push_str(detail);
            }
            if let Some(hint) = message.hint.as_deref() {
                text.push_str(" · Подсказка: ");
                text.push_str(hint);
            }
            (text, [0.82, 0.84, 0.90, 1.0], None)
        })
        .collect()
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
    let view = ButtonView {
        x: x.round(),
        y: y.round(),
        w: w.round(),
        h: h.round(),
        text,
        icon,
        text_scale: 0.72,
        icon_size: (16.0 * s).round(),
    };
    if active {
        if primary {
            let _ = view.render_styled(
                renderer,
                mx,
                my,
                s,
                false,
                ButtonStyle {
                    border: [0.32, 0.76, 0.43, 1.0],
                    background: [0.16, 0.48, 0.26, 1.0],
                    hover_background: [0.20, 0.58, 0.31, 1.0],
                    pressed_background: [0.12, 0.40, 0.22, 1.0],
                    content: renderer.theme.fg,
                },
            );
        } else {
            let _ = view.render(renderer, mx, my, s, false);
        }
        ui.register_rect(id, view.x, view.y, view.w, view.h, mx, my);
    } else {
        view.render_disabled(renderer, s);
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
    let x = x.round();
    let y = y.round();
    let w = w.round();
    let h = h.round();
    let hovered = mx >= x && mx <= x + w && my >= y && my <= y + h;
    renderer.push_rounded_rect(
        x,
        y,
        w,
        h,
        (5.0 * s).round(),
        if active {
            [0.28, 0.24, 0.38, 1.0]
        } else if hovered {
            [0.18, 0.19, 0.24, 1.0]
        } else {
            [0.13, 0.135, 0.17, 1.0]
        },
    );
    renderer.draw_string_scaled_pixel_snapped(
        text,
        x + (10.0 * s).round(),
        Renderer::tree_row_text_y(y, h, s),
        renderer.theme.fg,
        0.68,
    );
    ui.register_rect(id, x, y, w, h, mx, my);
}


#[cfg(test)]
mod tests {
    use super::database_query_execution_summary;

    #[test]
    fn query_execution_summary_reports_time_rows_command_and_truncation() {
        let state = crate::app::database::DatabaseQueryTabState {
            last_duration_ms: 17,
            ..crate::app::database::DatabaseQueryTabState::default()
        };
        let result = crate::app::database::DatabaseQueryResultSet {
            returned_rows: 100,
            affected_rows: 0,
            command_kind: "SELECT".to_string(),
            rows: vec![Vec::new(); 100],
            truncated: true,
            ..crate::app::database::DatabaseQueryResultSet::default()
        };
        let summary = database_query_execution_summary(&state, &result);
        assert!(summary.contains("Выполнено за 17 мс"));
        assert!(summary.contains("Получено строк: 100"));
        assert!(summary.contains(" · SELECT"));
        assert_eq!(summary.matches("Получено строк: 100").count(), 1);
        assert!(!summary.contains("SELECT 100"));
        assert!(summary.contains("Показаны первые 100"));
        assert!(!summary.contains("Изменено строк"));
    }

    #[test]
    fn update_returning_summary_keeps_changed_and_returned_counts_separate() {
        let state = crate::app::database::DatabaseQueryTabState {
            last_duration_ms: 9,
            ..crate::app::database::DatabaseQueryTabState::default()
        };
        let result = crate::app::database::DatabaseQueryResultSet {
            returned_rows: 5,
            affected_rows: 5,
            command_kind: "UPDATE".to_string(),
            columns: vec!["id".to_string()],
            rows: vec![Vec::new(); 5],
            ..crate::app::database::DatabaseQueryResultSet::default()
        };
        let summary = database_query_execution_summary(&state, &result);
        assert_eq!(summary.matches("Получено строк: 5").count(), 1);
        assert_eq!(summary.matches("Изменено строк: 5").count(), 1);
        assert!(summary.ends_with("UPDATE"));
    }
}
