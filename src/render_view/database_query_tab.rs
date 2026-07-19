use crate::renderer::Renderer;
use crate::ui_system::{UiClipRect, UiId, UiRegistry};
use crate::widgets::{ButtonStyle, ButtonView, IconType};
use glow::HasContext;

const QUERY_TOOLBAR_H: f32 = 40.0;
const QUERY_BUTTON_TEXT_SCALE: f32 = 0.78;
const DATABASE_SQL_SPANS_CACHE_MAX_ENTRIES: usize = 256;
const DATABASE_SQL_SPANS_CACHE_MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Default)]
struct DatabaseSqlSpansCache {
    entries: std::collections::HashMap<String, Vec<crate::highlighter::ColorSpan>>,
    retained_bytes: usize,
}

impl DatabaseSqlSpansCache {
    fn entry_bytes(sql: &str, spans: &[crate::highlighter::ColorSpan]) -> usize {
        sql.len().saturating_add(
            spans
                .len()
                .saturating_mul(std::mem::size_of::<crate::highlighter::ColorSpan>()),
        )
    }

    fn should_cache(&self, sql: &str, spans: &[crate::highlighter::ColorSpan]) -> bool {
        Self::entry_bytes(sql, spans) <= DATABASE_SQL_SPANS_CACHE_MAX_BYTES
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.retained_bytes = 0;
    }
}

thread_local! {
    static DATABASE_SQL_SPANS_CACHE: std::cell::RefCell<DatabaseSqlSpansCache> =
        std::cell::RefCell::new(DatabaseSqlSpansCache::default());
}

fn with_cached_database_sql_spans<R>(
    sql: &str,
    callback: impl FnOnce(&[crate::highlighter::ColorSpan]) -> R,
) -> R {
    DATABASE_SQL_SPANS_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(spans) = cache.entries.get(sql) {
            return callback(spans);
        }
        let spans = crate::highlighter::highlight_sql_text(sql);
        if !cache.should_cache(sql, &spans) {
            return callback(&spans);
        }
        let entry_bytes = DatabaseSqlSpansCache::entry_bytes(sql, &spans);
        if cache.entries.len() >= DATABASE_SQL_SPANS_CACHE_MAX_ENTRIES
            || cache.retained_bytes.saturating_add(entry_bytes)
                > DATABASE_SQL_SPANS_CACHE_MAX_BYTES
        {
            cache.clear();
        }
        cache.retained_bytes = cache.retained_bytes.saturating_add(entry_bytes);
        cache.entries.insert(sql.to_owned(), spans);
        if let Some(spans) = cache.entries.get(sql) {
            callback(spans)
        } else {
            let spans = crate::highlighter::highlight_sql_text(sql);
            callback(&spans)
        }
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct QueryToolbarLayout {
    compact: bool,
    button_widths: [f32; 6],
    gap: f32,
    status_x: f32,
    status_w: f32,
}

fn query_toolbar_layout(x: f32, w: f32, scale: f32) -> QueryToolbarLayout {
    let scale = scale.max(0.1);
    let viewport_right = x + w.max(0.0);
    let left = (x + (8.0 * scale).round()).min(viewport_right);
    let right = (viewport_right - 8.0 * scale).max(left).min(viewport_right);
    let normal = [106.0, 92.0, 82.0, 84.0, 88.0, 96.0].map(|v| (v * scale).round());
    let compact = [82.0, 72.0, 44.0, 44.0, 54.0, 44.0].map(|v| (v * scale).round());
    let normal_gap = (5.0 * scale).round();
    let compact_gap = (3.0 * scale).round().max(1.0);
    let normal_total = normal.iter().sum::<f32>() + normal_gap * 5.0;
    let available = (right - left).max(0.0);
    let use_compact = normal_total + (120.0 * scale).round() > available;
    let mut widths = if use_compact { compact } else { normal };
    let mut gap = if use_compact { compact_gap } else { normal_gap };
    let total = widths.iter().sum::<f32>() + gap * 5.0;
    if total > available && total > 0.0 {
        gap = gap.min((available / 11.0).floor().max(0.0));
        let distributable = (available - gap * 5.0).max(0.0);
        let source_width = widths.iter().sum::<f32>();
        let ratio = if source_width > 0.0 {
            (distributable / source_width).clamp(0.0, 1.0)
        } else {
            0.0
        };
        for width in &mut widths {
            *width = (*width * ratio).floor().max(0.0);
        }
        let mut spare = (distributable - widths.iter().sum::<f32>()).floor() as usize;
        for width in &mut widths {
            if spare == 0 {
                break;
            }
            *width += 1.0;
            spare -= 1;
        }
    }
    let buttons_end = left + widths.iter().sum::<f32>() + gap * 5.0;
    let status_x = (buttons_end + (8.0 * scale).round()).min(right);
    QueryToolbarLayout {
        compact: use_compact,
        button_widths: widths,
        gap,
        status_x,
        status_w: (right - status_x).max(0.0),
    }
}

fn query_tabs_offset(widths: &[f32], active: usize, viewport_w: f32, gap: f32) -> f32 {
    if widths.is_empty() || viewport_w <= 0.0 {
        return 0.0;
    }
    let total = widths.iter().sum::<f32>() + gap * widths.len().saturating_sub(1) as f32;
    if total <= viewport_w {
        return 0.0;
    }
    let active = active.min(widths.len() - 1);
    let active_start = widths[..active].iter().sum::<f32>() + gap * active as f32;
    let active_end = active_start + widths[active];
    (active_end - viewport_w)
        .max(0.0)
        .min(active_start)
        .min(total - viewport_w)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct QueryReviewLayout {
    content_y: f32,
    content_bottom: f32,
    grid_h: f32,
    messages_y: f32,
    messages_h: f32,
    button_y: f32,
}

fn query_review_layout(y: f32, h: f32, text_bottom: f32, scale: f32) -> QueryReviewLayout {
    let button_h = (46.0 * scale).round().max(34.0);
    let button_y = (y + h - button_h - (8.0 * scale).round()).max(y).round();
    let content_y = (text_bottom + (8.0 * scale).round()).min(button_y).round();
    let content_bottom = (button_y - (8.0 * scale).round()).max(content_y).round();
    let content_h = (content_bottom - content_y).max(0.0);
    let gap = if content_h >= 100.0 { (8.0 * scale).round() } else { 0.0 };
    let messages_h = if content_h >= 120.0 {
        (content_h * 0.32).round().clamp(42.0, content_h)
    } else {
        0.0
    };
    let grid_h = (content_h - messages_h - gap).max(0.0).round();
    let messages_y = (content_y + grid_h + gap).round();
    QueryReviewLayout {
        content_y,
        content_bottom,
        grid_h,
        messages_y,
        messages_h,
        button_y,
    }
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
        let layout = query_toolbar_layout(x, w, s);
        let mut bx = x + (8.0 * s).round();
        let labels = if layout.compact {
            ["Run", "Stop", "EX", "AN", "Fmt", ""]
        } else {
            ["Выполнить", "Отмена", "Explain", "Analyze", "Формат", "История"]
        };
        let specs = [
            (UiId::DatabaseQueryRun, Some(IconType::Run), can_run, true),
            (UiId::DatabaseQueryCancel, Some(IconType::Cancel), can_cancel, false),
            (UiId::DatabaseQueryExplain, None, can_run, false),
            (
                UiId::DatabaseQueryExplainAnalyze,
                None,
                can_run,
                false,
            ),
            (UiId::DatabaseQueryFormat, None, can_run, false),
            (UiId::DatabaseQueryHistory, Some(IconType::Time), true, false),
        ];
        let toolbar_clip = UiClipRect::new(x, y, w, toolbar_h);
        for (index, (id, icon, active, primary)) in specs.into_iter().enumerate() {
            let width = layout.button_widths[index];
            draw_query_button(
                self,
                ui,
                id,
                bx,
                y + (5.0 * s).round(),
                width,
                (30.0 * s).round(),
                labels[index],
                icon,
                active,
                primary,
                mx,
                my,
                s,
                Some(toolbar_clip),
            );
            bx = (bx + width + layout.gap).round();
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
            "Готово · SQL-консоль".to_string()
        } else {
            format!("{} · SQL-консоль", meta.database_name)
        };
        let status_x = layout.status_x.round();
        let status_w = layout.status_w.round();
        let mut scratch = String::new();
        self.draw_tree_label_clipped(
            &status,
            status_x,
            Self::tree_row_text_y(y, toolbar_h, s),
            status_w,
            if state.error.is_some() || analysis_errors > 0 {
                [0.95, 0.38, 0.42, 1.0]
            } else if analysis_warnings > 0 {
                [0.95, 0.72, 0.30, 1.0]
            } else {
                self.theme.line_num
            },
            0.68,
            &mut scratch,
        );
        if !state.editor_diagnostics.is_empty() && status_w > 0.0 {
            ui.register_rect_clipped(
                UiId::DatabaseQueryNextDiagnostic,
                status_x,
                y,
                status_w,
                toolbar_h,
                toolbar_clip,
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
        let tab_pad = (8.0 * s).round();
        let tab_gap = (4.0 * s).round();
        let tab_widths = state
            .results
            .iter()
            .map(|result| {
                (self.measure_ui_width(&result.title, 0.76) + 26.0 * s)
                    .max(90.0 * s)
                    .round()
            })
            .collect::<Vec<_>>();
        let tabs_view_w = (w - tab_pad * 2.0).max(0.0);
        let tab_offset = query_tabs_offset(
            &tab_widths,
            state.result_view.active_result,
            tabs_view_w,
            tab_gap,
        );
        let tabs_clip = UiClipRect::new(x, y, w, tabs_h);
        let mut tab_x = x + tab_pad - tab_offset;
        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            self.gl.scissor(
                x as i32,
                (self.height - (y + tabs_h)).round().max(0.0) as i32,
                w.max(0.0) as i32,
                tabs_h.max(0.0) as i32,
            );
        }
        for (index, result) in state.results.iter().enumerate() {
            let width = tab_widths[index];
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
                tabs_clip,
            );
            tab_x = (tab_x + width + tab_gap).round();
        }
        self.flush();
        unsafe { self.gl.disable(glow::SCISSOR_TEST) };

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
            let content_h = crate::app::database::database_query_history_content_height(
                history.iter().filter(|entry| {
                    entry.connection_id == meta.connection_id
                        && entry.database_name == meta.database_name
                }),
                s,
            );
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
                state.history_selected,
                state.result_view.scroll_y.current.clamp(0.0, max_y),
                state.result_view.scroll_y.is_settled(),
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
        history_selected: usize,
        scroll_y: f32,
        hover_settled: bool,
        ui: &mut UiRegistry,
        mx: f32,
        my: f32,
    ) {
        let x = x.round();
        let y = y.round();
        let w = w.round();
        let h = h.round();
        let padding = (10.0 * s).round();
        let clip = UiClipRect::new(x, y, w, h);
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
                crate::app::database::database_query_history_entry_height_px(&entry.sql, s);
            let row_y = (y + content_y - scroll_y).round();
            content_y += entry_h;
            if row_y + entry_h <= y || row_y >= y + h {
                continue;
            }
            let hovered = hover_settled
                && clip.intersect(x, row_y, w, entry_h).is_some_and(|rect| {
                    mx >= rect.x
                        && mx <= rect.x + rect.w
                        && my >= rect.y
                        && my <= rect.y + rect.h
                });
            let selected = visible_index == history_selected;
            self.push_rect(
                x,
                row_y,
                w,
                entry_h,
                if hovered {
                    [0.16, 0.17, 0.21, 1.0]
                } else if selected {
                    [0.20, 0.16, 0.28, 1.0]
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
            if hover_settled {
                ui.register_rect_clipped(
                    UiId::DatabaseQueryHistoryEntry(visible_index),
                    x,
                    row_y,
                    w,
                    entry_h,
                    clip,
                    mx,
                    my,
                );
            }
            let status = if entry.succeeded { "OK" } else { "ERR" };
            let row_count = if entry.returned_rows > 0 {
                entry.returned_rows
            } else {
                entry.affected_rows
            };
            let meta_text = format!(
                "{status} · {} мс · {} строк",
                entry.duration_ms, row_count
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
        let header_clip = UiClipRect::new(
            layout.header_rect.x,
            layout.header_rect.y,
            layout.header_rect.w,
            layout.header_rect.h,
        );
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
            ui.register_rect_clipped(
                UiId::DatabaseQueryColumnResize(*column),
                divider_x,
                y,
                (6.0 * s).round(),
                header_h,
                header_clip,
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
        self.draw_spanned_ui_line_pixel_snapped(
            line,
            spans,
            Some(base_offset),
            x,
            y,
            max_x,
            1.0,
        );
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

        let layout = query_review_layout(y, h, text_y, s);
        let grid_x = x + padding;
        let grid_w = (w - padding * 2.0).max(1.0);
        if layout.grid_h > 1.0 {
            self.draw_database_query_results(
                grid_x,
                layout.content_y,
                grid_w,
                layout.grid_h,
                s,
                meta,
                state,
                history,
                ui,
                mx,
                my,
            );
        }

        if layout.messages_h > (30.0 * s).round() {
            self.draw_database_query_review_messages(
                grid_x,
                layout.messages_y,
                grid_w,
                layout.messages_h,
                s,
                state,
                ui,
                mx,
                my,
            );
        }

        let button_gap = (10.0 * s).round().max(4.0);
        let available_button_w = (w - padding * 2.0 - button_gap).max(2.0);
        let primary_w = (available_button_w * 0.52).round();
        let secondary_w = (available_button_w - primary_w).max(1.0);
        let button_h = (36.0 * s).round().min((y + h - layout.button_y).max(1.0));
        let button_clip = UiClipRect::new(x, y, w, h);

        draw_query_button(
            self,
            ui,
            UiId::DatabaseQueryCommit,
            x + padding,
            layout.button_y,
            primary_w,
            button_h,
            "Применить",
            Some(IconType::Check),
            true,
            true,
            mx,
            my,
            s,
            Some(button_clip),
        );
        draw_query_button(
            self,
            ui,
            UiId::DatabaseQueryRollback,
            x + padding + primary_w + button_gap,
            layout.button_y,
            secondary_w,
            button_h,
            "Отмена",
            Some(IconType::Cancel),
            true,
            false,
            mx,
            my,
            s,
            Some(button_clip),
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
    clip: Option<UiClipRect>,
) {
    let view = ButtonView {
        x: x.round(),
        y: y.round(),
        w: w.round(),
        h: h.round(),
        text,
        icon,
        text_scale: QUERY_BUTTON_TEXT_SCALE,
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
        if let Some(clip) = clip {
            ui.register_rect_clipped(id, view.x, view.y, view.w, view.h, clip, mx, my);
        } else {
            ui.register_rect(id, view.x, view.y, view.w, view.h, mx, my);
        }
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
    clip: UiClipRect,
) {
    let x = x.round();
    let y = y.round();
    let w = w.round();
    let h = h.round();
    let visible = clip.intersect(x, y, w, h);
    let hovered = visible.is_some_and(|rect| {
        mx >= rect.x && mx <= rect.x + rect.w && my >= rect.y && my <= rect.y + rect.h
    });
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
    let mut scratch = String::new();
    renderer.draw_tree_label_clipped(
        text,
        x + (10.0 * s).round(),
        Renderer::tree_row_text_y(y, h, s),
        (w - 20.0 * s).max(4.0),
        renderer.theme.fg,
        0.68,
        &mut scratch,
    );
    ui.register_rect_clipped(id, x, y, w, h, clip, mx, my);
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bug_5_query_toolbar_buttons_fit_narrow_viewport() {
        let layout = query_toolbar_layout(0.0, 280.0, 1.0);
        let total = layout.button_widths.iter().sum::<f32>() + layout.gap * 5.0;
        assert!(layout.compact);
        assert!(total <= 264.0 + 1.0);
        assert!(layout.status_x <= 272.0);
    }

    #[test]
    fn bug_6_query_status_width_never_crosses_toolbar_clip() {
        for width in [0.0, 40.0, 180.0, 900.0] {
            let layout = query_toolbar_layout(10.0, width, 1.25);
            assert!(layout.status_w.is_finite());
            assert!(layout.status_w >= 0.0);
            assert!(layout.status_x + layout.status_w <= 10.0 + width.max(0.0) + 0.5);
        }
    }

    #[test]
    fn a4_b013_query_toolbar_buttons_never_cross_narrow_clip() {
        for scale in [0.75, 1.0, 1.25, 2.0] {
            for width in [0.0, 20.0, 40.0, 80.0, 120.0, 180.0, 280.0] {
                let x = 10.0;
                let layout = query_toolbar_layout(x, width, scale);
                let viewport_right = x + width;
                let left = (x + (8.0 * scale).round()).min(viewport_right);
                let right = (viewport_right - 8.0 * scale)
                    .max(left)
                    .min(viewport_right);
                let available = (right - left).max(0.0);
                let total = layout.button_widths.iter().sum::<f32>() + layout.gap * 5.0;
                assert!(layout.button_widths.iter().all(|value| value.is_finite() && *value >= 0.0));
                assert!(layout.gap.is_finite() && layout.gap >= 0.0);
                assert!(total <= available + 0.01, "scale={scale} width={width}: {total} > {available}");
            }
        }
    }

    #[test]
    fn bug_7_query_toolbar_uses_standard_button_typography() {
        assert_eq!(QUERY_BUTTON_TEXT_SCALE, 0.78);
    }

    #[test]
    fn bug_8_active_result_tab_is_scrolled_into_view() {
        let widths = [120.0, 140.0, 160.0, 180.0];
        let viewport = 220.0;
        let offset = query_tabs_offset(&widths, 3, viewport, 4.0);
        let start = widths[..3].iter().sum::<f32>() + 12.0 - offset;
        let end = start + widths[3];
        assert!(start >= -0.5);
        assert!(end <= viewport + 0.5);
    }

    #[test]
    fn bug_9_query_execution_summary_reports_metrics_once() {
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
    fn bug_11_query_history_hover_waits_for_scroll_to_settle() {
        let mut scroll = crate::scroll::ScrollState::new(15.0);
        scroll.current = 10.0;
        scroll.target = 25.0;
        assert!(!scroll.is_settled());
        scroll.current = scroll.target;
        assert!(scroll.is_settled());
    }

    #[test]
    fn bug_12_history_hitbox_is_intersected_with_visible_body() {
        let clip = UiClipRect::new(0.0, 20.0, 100.0, 80.0);
        assert_eq!(clip.intersect(0.0, 0.0, 100.0, 40.0), Some(UiClipRect::new(0.0, 20.0, 100.0, 20.0)));
    }

    #[test]
    fn bug_13_query_column_resize_hitbox_cannot_escape_header() {
        let header = UiClipRect::new(10.0, 30.0, 200.0, 40.0);
        let hit = header.intersect(205.0, 25.0, 12.0, 60.0).expect("visible divider strip");
        assert_eq!(hit, UiClipRect::new(205.0, 30.0, 5.0, 40.0));
    }

    #[test]
    fn bug_14_query_review_layout_stays_inside_tiny_modal() {
        let layout = query_review_layout(0.0, 120.0, 70.0, 1.0);
        assert!(layout.content_y <= layout.content_bottom);
        assert!(layout.messages_y + layout.messages_h <= layout.button_y + 0.5);
        assert!(layout.button_y <= 120.0);
    }

    #[test]
    fn bug_15_sql_span_cache_respects_entry_and_byte_caps() {
        let mut cache = DatabaseSqlSpansCache::default();
        let huge = "x".repeat(DATABASE_SQL_SPANS_CACHE_MAX_BYTES + 1);
        assert!(!cache.should_cache(&huge, &[]));
        cache.entries.insert("small".to_string(), Vec::new());
        cache.retained_bytes = DATABASE_SQL_SPANS_CACHE_MAX_BYTES;
        cache.clear();
        assert!(cache.entries.is_empty());
        assert_eq!(cache.retained_bytes, 0);
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
