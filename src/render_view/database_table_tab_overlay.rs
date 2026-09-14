use crate::renderer::Renderer;
use crate::ui_system::{UiId, UiRegistry};
use crate::widgets::ButtonView;
use glow::HasContext;

pub(crate) fn database_table_review_body_height(
    viewport_w: f32,
    viewport_h: f32,
    scale: f32,
) -> f32 {
    let fitted = crate::ui_system::fit_centered_rect(
        viewport_w,
        viewport_h,
        780.0 * scale,
        620.0 * scale,
        16.0 * scale,
    );
    (fitted.h - (180.0 * scale).round()).max(0.0)
}

fn database_table_review_line_step(scale: f32) -> f32 {
    (22.0 * scale).round().max(1.0)
}

fn database_table_review_content_height(scale: f32, line_count: usize) -> f32 {
    line_count as f32 * database_table_review_line_step(scale)
}

fn database_table_review_visible_range(
    line_count: usize,
    body_h: f32,
    scroll_y: f32,
    scale: f32,
) -> std::ops::Range<usize> {
    if line_count == 0 || body_h <= 0.0 {
        return 0..0;
    }
    let step = database_table_review_line_step(scale);
    let first = ((scroll_y.max(0.0) / step).floor() as usize).saturating_sub(1);
    let visible = ((body_h.max(0.0) / step).ceil() as usize).saturating_add(3);
    first.min(line_count)..first.saturating_add(visible).min(line_count)
}

fn visit_database_table_review_lines(
    summary: &crate::app::database::DatabaseTableReviewSummary,
    range: std::ops::Range<usize>,
    mut visitor: impl FnMut(usize, bool, &str),
) {
    for index in range {
        if let Some(notice) = summary.notices.get(index) {
            visitor(index, true, notice);
            continue;
        }
        let detail_index = index.saturating_sub(summary.notices.len());
        if let Some(detail) = summary.detail_rows.get(detail_index) {
            visitor(index, false, detail);
        }
    }
}

fn database_sql_visible_line_range(
    line_count: usize,
    viewport_h: f32,
    scroll_y: f32,
    line_h: f32,
) -> std::ops::Range<usize> {
    if line_count == 0 || viewport_h <= 0.0 || line_h <= 0.0 {
        return 0..0;
    }
    let first = (scroll_y.max(0.0) / line_h).floor() as usize;
    let visible = (viewport_h / line_h).ceil() as usize + 2;
    first.min(line_count)..first.saturating_add(visible).min(line_count)
}

fn database_sql_visual_baseline(
    outer_y: f32,
    line_index: usize,
    line_h: f32,
    render_scroll_y: f32,
    scale: f32,
) -> f32 {
    (outer_y + (line_index as f32 + 1.0) * line_h
        - render_scroll_y
        - (4.0 * scale).round())
    .round()
}

fn database_sql_preview_outer_rect(x: f32, y: f32, w: f32, h: f32, scale: f32) -> crate::ui_system::UiClipRect {
    crate::ui_system::UiClipRect::new(
        (x + 18.0 * scale).round(),
        (y + 50.0 * scale).round(),
        (w - 36.0 * scale).round().max(1.0),
        (h - 116.0 * scale).round().max(1.0),
    )
}

pub(crate) fn database_table_review_max_scroll(
    viewport_w: f32,
    viewport_h: f32,
    scale: f32,
    line_count: usize,
) -> f32 {
    let body_h = database_table_review_body_height(viewport_w, viewport_h, scale);
    (database_table_review_content_height(scale, line_count) - body_h).max(0.0)
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_database_table_modal(
        &mut self,
        s: f32,
        modal: &crate::app::database::DatabaseTableModal,
        layout_cache: &std::cell::RefCell<crate::app::database::DatabaseMultilineLayoutCache>,
        ui: &mut UiRegistry,
        mx: f32,
        my: f32,
        blink_alpha: f32,
    ) {
        ui.mark_overlay_start();
        self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.66]);
        ui.register_blocker(UiId::DatabaseTableModalBackdrop, 0.0, 0.0, self.width, self.height, mx, my);
        let (title, width, height) = match modal {
            crate::app::database::DatabaseTableModal::SqlPreview { .. } => ("Предпросмотр SQL", 980.0, 700.0),
            crate::app::database::DatabaseTableModal::RefreshPrompt { close_after_save, .. } => (
                if *close_after_save { "Закрыть изменённую таблицу?" } else { "Обновить изменённую таблицу?" },
                560.0,
                210.0,
            ),
            crate::app::database::DatabaseTableModal::CustomLimit { .. } => ("Количество строк", 460.0, 210.0),
            crate::app::database::DatabaseTableModal::MultilineEditor { .. } => ("Редактор значения", 820.0, 620.0),
            crate::app::database::DatabaseTableModal::Review { .. } => ("Проверка транзакции", 780.0, 620.0),
        };
        let fitted = crate::ui_system::fit_centered_rect(
            self.width, self.height, width * s, height * s, 16.0 * s,
        );
        let width = fitted.w;
        let height = fitted.h;
        let x = fitted.x;
        let y = fitted.y;
        self.push_rounded_rect_border(
            x,
            y,
            width,
            height,
            8.0 * s,
            1.0,
            [1.0, 1.0, 1.0, 0.17],
            [0.105, 0.11, 0.145, 1.0],
        );
        ui.register_blocker(UiId::DatabaseTableModalBody, x, y, width, height, mx, my);
        self.draw_string_scaled_pixel_snapped(title, x + 20.0 * s, y + 32.0 * s, self.theme.fg, 1.0);

        match modal {
            crate::app::database::DatabaseTableModal::SqlPreview {
                text,
                cursor,
                selection_anchor,
                spans,
                scroll_x,
                scroll_y,
                ..
            } => {
                self.draw_database_sql_preview(
                    x,
                    y,
                    width,
                    height,
                    s,
                    text,
                    spans,
                    *cursor,
                    *selection_anchor,
                    scroll_x.current,
                    scroll_y.current,
                    layout_cache,
                    blink_alpha,
                    ui,
                    mx,
                    my,
                );
                draw_modal_buttons(
                    self,
                    ui,
                    x,
                    y,
                    width,
                    height,
                    s,
                    &[
                        (UiId::DatabaseTableModalTertiary, "Копировать"),
                        (UiId::DatabaseTableModalSecondary, "Закрыть"),
                    ],
                    mx,
                    my,
                );
            }
            crate::app::database::DatabaseTableModal::RefreshPrompt { .. } => {
                self.draw_string_scaled_pixel_snapped(
                    "Есть несохранённые изменения. Выберите безопасное действие.",
                    x + 20.0 * s,
                    y + 78.0 * s,
                    [0.80, 0.82, 0.88, 1.0],
                    0.82,
                );
                draw_modal_buttons(
                    self,
                    ui,
                    x,
                    y,
                    width,
                    height,
                    s,
                    &[
                        (UiId::DatabaseTableModalPrimary, "Сохранить"),
                        (UiId::DatabaseTableModalTertiary, "Отбросить"),
                        (UiId::DatabaseTableModalSecondary, "Отмена"),
                    ],
                    mx,
                    my,
                );
            }
            crate::app::database::DatabaseTableModal::CustomLimit { input, error, .. } => {
                let input_x = (x + 20.0 * s).round();
                let input_y = (y + 68.0 * s).round();
                let input_w = (width - 40.0 * s).round();
                let input_h = (34.0 * s).round();
                ui.register_text_input(
                    UiId::DatabaseTableModalInput,
                    input_x,
                    input_y,
                    input_w,
                    input_h,
                    mx,
                    my,
                );
                let padding = (8.0 * s).round();
                let text_geometry = crate::app::single_line_input::single_line_text_geometry(
                    input_x, input_w, padding, 0.0,
                );
                let edge_pad = crate::app::single_line_input::single_line_cursor_edge_pad(s);
                let scroll_x = crate::app::single_line_input::single_line_cursor_geometry(
                    input.text(),
                    input.cursor,
                    text_geometry.content_w,
                    0.0,
                    edge_pad,
                    edge_pad,
                    |ch| self.one_line_ui_advance(ch, 0.82),
                )
                .scroll_x;
                self.draw_one_line_dialog_input(
                    input.text(),
                    input.cursor,
                    input.selection_anchor,
                    false,
                    true,
                    input_x,
                    input_y,
                    input_w,
                    input_h,
                    scroll_x,
                    blink_alpha,
                    0.82,
                    0.0,
                );
                if let Some(error) = error.as_deref() {
                    self.draw_string_scaled_pixel_snapped(error, x + 20.0 * s, y + 126.0 * s, [0.95, 0.38, 0.42, 1.0], 0.76);
                } else {
                    self.draw_string_scaled_pixel_snapped("Допустимо: 1–10 000", x + 20.0 * s, y + 126.0 * s, self.theme.line_num, 0.74);
                }
                draw_modal_buttons(self, ui, x, y, width, height, s, &[(UiId::DatabaseTableModalPrimary, "Применить"), (UiId::DatabaseTableModalSecondary, "Отмена")], mx, my);
            }
            crate::app::database::DatabaseTableModal::MultilineEditor {
                input,
                scroll_x,
                scroll_y,
                error,
                ..
            } => {
                self.draw_database_sql_preview(
                    x,
                    y,
                    width,
                    height,
                    s,
                    input.text(),
                    &[],
                    input.cursor,
                    input.selection_anchor,
                    scroll_x.current,
                    scroll_y.current,
                    layout_cache,
                    blink_alpha,
                    ui,
                    mx,
                    my,
                );
                if let Some(error) = error.as_deref() {
                    self.draw_string_scaled_pixel_snapped(
                        error,
                        x + 20.0 * s,
                        y + height - 70.0 * s,
                        [0.95, 0.38, 0.42, 1.0],
                        0.72,
                    );
                }
                draw_modal_buttons(self, ui, x, y, width, height, s, &[(UiId::DatabaseTableModalPrimary, "Применить"), (UiId::DatabaseTableModalTertiary, "Как текст"), (UiId::DatabaseTableModalSecondary, "Отмена")], mx, my);
            }
            crate::app::database::DatabaseTableModal::Review { state, scroll, .. } => {
                let remaining = state.deadline_unix_ms.saturating_sub(now_unix_ms()) / 1000;
                let summary = &state.summary;
                self.draw_string_scaled_pixel_snapped(
                    &format!("Добавлено: {}   Изменено: {}   Ячеек: {}   Удалено: {}", summary.inserted_rows, summary.updated_rows, summary.changed_cells, summary.deleted_rows),
                    x + 20.0 * s,
                    y + 66.0 * s,
                    self.theme.fg,
                    0.82,
                );
                self.draw_string_scaled_pixel_snapped(
                    &format!("До автоматического rollback: {}:{:02}", remaining / 60, remaining % 60),
                    x + 20.0 * s,
                    y + 92.0 * s,
                    if remaining < 30 { [0.95,0.38,0.42,1.0] } else { [0.95,0.72,0.28,1.0] },
                    0.78,
                );
                let body_y = (y + 112.0 * s).round();
                let body_h = database_table_review_body_height(self.width, self.height, s);
                let body_x = (x + 18.0 * s).round();
                let body_w = (width - 36.0 * s).round().max(0.0);
                self.push_rect(body_x, body_y, body_w, body_h, [0.06, 0.065, 0.085,1.0]);
                ui.register_rect(UiId::DatabaseTableModalScroll, body_x + body_w - 12.0 * s, body_y, 12.0 * s, body_h, mx, my);
                self.flush();
                unsafe {
                    self.gl.enable(glow::SCISSOR_TEST);
                    self.gl.scissor(
                        body_x.round() as i32,
                        (self.height - (body_y + body_h)).round().max(0.0) as i32,
                        body_w.round().max(0.0) as i32,
                        body_h.round().max(0.0) as i32,
                    );
                }
                let total_lines = summary.notices.len() + summary.detail_rows.len();
                let max_scroll = database_table_review_max_scroll(
                    self.width,
                    self.height,
                    s,
                    total_lines,
                );
                let logical_scroll = scroll.current.clamp(0.0, max_scroll);
                let render_scroll = logical_scroll.round();
                let line_step = database_table_review_line_step(s);
                let baseline_offset = (23.0 * s).round();
                let visible_range = database_table_review_visible_range(
                    total_lines,
                    body_h,
                    logical_scroll,
                    s,
                );
                let text_x = (body_x + 10.0 * s).round();
                let text_w = (body_w - 32.0 * s).max(0.0);
                let mut clip_scratch = String::new();
                let mut notice_scratch = String::new();
                visit_database_table_review_lines(summary, visible_range, |index, is_notice, text| {
                    let baseline =
                        (body_y + baseline_offset + index as f32 * line_step - render_scroll)
                            .round();
                    let (label, color) = if is_notice {
                        notice_scratch.clear();
                        notice_scratch.push_str("⚠ ");
                        notice_scratch.push_str(text);
                        (notice_scratch.as_str(), [0.95, 0.72, 0.28, 1.0])
                    } else {
                        (text, self.theme.fg)
                    };
                    self.draw_tree_label_clipped(
                        label,
                        text_x,
                        baseline,
                        text_w,
                        color,
                        0.72,
                        &mut clip_scratch,
                    );
                });
                self.flush();
                unsafe { self.gl.disable(glow::SCISSOR_TEST) };

                let content_h = database_table_review_content_height(s, total_lines);
                if content_h > body_h {
                    let thumb_h = (body_h / content_h * body_h)
                        .max((30.0 * s).round())
                        .min(body_h)
                        .round();
                    let ratio = (logical_scroll / max_scroll.max(1.0)).clamp(0.0, 1.0);
                    self.push_rounded_rect(
                        (body_x + body_w - 8.0 * s).round(),
                        (body_y + ratio * (body_h - thumb_h)).round(),
                        (5.0 * s).round().max(1.0),
                        thumb_h,
                        (2.5 * s).round().max(1.0),
                        [0.62, 0.38, 0.82, 0.9],
                    );
                }
                if summary.truncated_details {
                    self.push_rect(body_x, body_y + body_h - 27.0 * s, body_w, 27.0 * s, [0.16,0.12,0.05,0.95]);
                    self.draw_string_scaled_pixel_snapped(
                        "Подробности ограничены; агрегаты рассчитаны полностью.",
                        body_x + 10.0 * s,
                        body_y + body_h - 8.0 * s,
                        [0.95,0.72,0.28,1.0],
                        0.7,
                    );
                }
                draw_modal_buttons(self, ui, x, y, width, height, s, &[(UiId::DatabaseTableModalPrimary, if state.committing { "Применение…" } else { "Применить" }), (UiId::DatabaseTableModalSecondary, "Отмена")], mx, my);
            }
        }
    }

    fn database_code_text_width(&mut self, text: &str) -> f32 {
        text.chars().map(|ch| self.char_advance(ch)).sum()
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_database_sql_preview(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        text: &str,
        spans: &[crate::highlighter::ColorSpan],
        cursor: usize,
        selection_anchor: Option<usize>,
        scroll_x: f32,
        scroll_y: f32,
        layout_cache: &std::cell::RefCell<crate::app::database::DatabaseMultilineLayoutCache>,
        blink_alpha: f32,
        ui: &mut UiRegistry,
        mx: f32,
        my: f32,
    ) {
        let outer = database_sql_preview_outer_rect(x, y, w, h, s);
        let outer_x = outer.x;
        let outer_y = outer.y;
        let outer_w = outer.w;
        let outer_h = outer.h;
        let scrollbar = (10.0 * s).round().max(10.0);
        let gutter_w = (54.0 * s).round();
        let line_h = (crate::app::database::DATABASE_SQL_PREVIEW_LINE_HEIGHT * s)
            .round()
            .max(1.0);
        let mut layout_cache = layout_cache.borrow_mut();
        layout_cache.ensure(text, s, true, |line| self.database_code_text_width(line));
        let line_count = layout_cache.line_count();
        let content_h = line_count as f32 * line_h;
        let content_w = layout_cache.max_line_width() + (18.0 * s).round();
        let mut viewport_w = outer_w;
        let mut viewport_h = outer_h;
        let mut show_y = content_h > viewport_h;
        if show_y {
            viewport_w = (viewport_w - scrollbar).max(1.0);
        }
        let show_x = content_w > (viewport_w - gutter_w).max(1.0);
        if show_x {
            viewport_h = (viewport_h - scrollbar).max(1.0);
            if !show_y && content_h > viewport_h {
                show_y = true;
                viewport_w = (viewport_w - scrollbar).max(1.0);
            }
        }
        let code_w = (viewport_w - gutter_w).max(1.0);
        let max_x = (content_w - code_w).max(0.0);
        let max_y = (content_h - viewport_h).max(0.0);
        let scroll_x = scroll_x.clamp(0.0, max_x);
        let scroll_y = scroll_y.clamp(0.0, max_y);
        let render_scroll_y = scroll_y.round().clamp(0.0, max_y);
        let code_x = outer_x + gutter_w;
        ui.register_text_input(
            UiId::DatabaseTableModalInput,
            code_x,
            outer_y,
            code_w,
            viewport_h,
            mx,
            my,
        );
        let (selection_start, selection_end) = selection_anchor
            .map(|anchor| (anchor.min(cursor), anchor.max(cursor)))
            .unwrap_or((cursor, cursor));

        self.push_rect(outer_x, outer_y, outer_w, outer_h, [0.045, 0.05, 0.07, 1.0]);
        self.push_rect(outer_x, outer_y, gutter_w, viewport_h, [0.065, 0.07, 0.09, 1.0]);
        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            self.gl.scissor(
                outer_x as i32,
                (self.height - (outer_y + viewport_h)).round().max(0.0) as i32,
                viewport_w.round().max(0.0) as i32,
                viewport_h.round().max(0.0) as i32,
            );
        }
        let visible_lines =
            database_sql_visible_line_range(line_count, viewport_h, scroll_y, line_h);
        for line_index in visible_lines {
            let Some((byte_offset, line_end)) = layout_cache.line_range(line_index) else {
                continue;
            };
            let Some(line) = text.get(byte_offset..line_end) else {
                continue;
            };
            let baseline =
                database_sql_visual_baseline(outer_y, line_index, line_h, render_scroll_y, s);
            self.draw_string_scaled_pixel_snapped(
                &(line_index + 1).to_string(),
                outer_x + (8.0 * s).round(),
                baseline,
                self.theme.line_num,
                0.78,
            );
            let selected_start = selection_start.max(byte_offset).min(line_end);
            let selected_end = selection_end.max(byte_offset).min(line_end);
            let text_x = code_x + (8.0 * s).round() - scroll_x;
            if selected_start < selected_end {
                let prefix = &line[..selected_start - byte_offset];
                let selected = &line[selected_start - byte_offset..selected_end - byte_offset];
                let selected_x = text_x + self.database_code_text_width(prefix);
                let selected_w = self.database_code_text_width(selected).max(1.0);
                self.push_rect(
                    selected_x.round(),
                    (baseline - 19.0 * s).round(),
                    selected_w.round(),
                    line_h,
                    self.theme.sel,
                );
            }
            if cursor >= byte_offset && cursor <= line_end && selection_start == selection_end {
                let prefix = &line[..cursor.min(line_end) - byte_offset];
                let caret_x = text_x + self.database_code_text_width(prefix);
                self.push_rect(
                    caret_x.round(),
                    (baseline - 19.0 * s).round(),
                    (1.0 * s).round().max(1.0),
                    line_h,
                    [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], blink_alpha],
                );
            }
            self.draw_database_sql_line(
                line,
                byte_offset,
                spans,
                text_x,
                baseline,
                outer_x + viewport_w - (6.0 * s).round(),
            );
        }
        self.flush();
        unsafe { self.gl.disable(glow::SCISSOR_TEST) };

        let track_color = [0.055, 0.058, 0.075, 1.0];
        let thumb_color = [0.35, 0.68, 0.94, 0.92];
        if show_y {
            let track_x = outer_x + viewport_w;
            self.push_rect(track_x, outer_y, scrollbar, viewport_h, track_color);
            ui.register_rect(
                UiId::DatabaseTableModalScroll,
                track_x,
                outer_y,
                scrollbar,
                viewport_h,
                mx,
                my,
            );
            if let Some(thumb) = crate::scroll::scrollbar_thumb(
                outer_y,
                viewport_h,
                viewport_h,
                content_h,
                scroll_y,
                (28.0 * s).round(),
            ) {
                self.push_rounded_rect(
                    track_x + (2.0 * s).round(),
                    thumb.start.round(),
                    (scrollbar - 4.0 * s).max(4.0).round(),
                    thumb.len.round(),
                    (3.0 * s).round(),
                    thumb_color,
                );
            }
        }
        if show_x {
            let track_y = outer_y + viewport_h;
            self.push_rect(outer_x + gutter_w, track_y, code_w, scrollbar, track_color);
            ui.register_rect(
                UiId::DatabaseTableModalScrollX,
                outer_x + gutter_w,
                track_y,
                code_w,
                scrollbar,
                mx,
                my,
            );
            if let Some(thumb) = crate::scroll::scrollbar_thumb(
                outer_x + gutter_w,
                code_w,
                code_w,
                content_w,
                scroll_x,
                (36.0 * s).round(),
            ) {
                self.push_rounded_rect(
                    thumb.start.round(),
                    track_y + (2.0 * s).round(),
                    thumb.len.round(),
                    (scrollbar - 4.0 * s).max(4.0).round(),
                    (3.0 * s).round(),
                    thumb_color,
                );
            }
        }
    }

}

#[allow(clippy::too_many_arguments)]
fn draw_modal_buttons(renderer: &mut Renderer, ui: &mut UiRegistry, x: f32, y: f32, w: f32, h: f32, s: f32, buttons: &[(UiId, &str)], mx: f32, my: f32) {
    let Some((mut bx, button_y, button_w, button_h, gap)) =
        database_modal_button_layout(x, y, w, h, s, buttons.len())
    else {
        return;
    };
    for (id, label) in buttons {
        ui.register_button_view(*id, ButtonView { x: bx, y: button_y, w: button_w, h: button_h, text: label, icon: None, text_scale: 0.82, icon_size: 0.0 }, renderer, mx, my, s, false);
        bx += button_w + gap;
    }
}

fn database_modal_button_layout(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    s: f32,
    button_count: usize,
) -> Option<(f32, f32, f32, f32, f32)> {
    if button_count == 0 || w <= 0.0 || h <= 0.0 {
        return None;
    }
    let s = s.max(0.01);
    let side_padding = (20.0 * s).min(w * 0.25);
    let available = (w - side_padding * 2.0).max(0.0);
    let gap_count = button_count.saturating_sub(1) as f32;
    let gap = if gap_count > 0.0 {
        (8.0 * s).min(available / (button_count as f32 * 4.0).max(1.0))
    } else {
        0.0
    };
    let button_w = ((available - gap * gap_count).max(0.0) / button_count as f32)
        .min(118.0 * s);
    let total = button_w * button_count as f32 + gap * gap_count;
    let button_h = (36.0 * s).min(h).max(0.0);
    let button_y = (y + h - 18.0 * s - button_h)
        .clamp(y, (y + h - button_h).max(y));
    Some((x + side_padding + (available - total).max(0.0), button_y, button_w, button_h, gap))
}

fn now_unix_ms() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |value| value.as_millis())
}

#[cfg(test)]
mod tests {
    use super::{
        database_modal_button_layout, database_sql_preview_outer_rect,
        database_sql_visible_line_range, database_sql_visual_baseline,
        database_table_review_body_height, database_table_review_content_height,
        database_table_review_line_step, database_table_review_max_scroll,
        database_table_review_visible_range, visit_database_table_review_lines,
    };

    fn review_summary(
        notices: usize,
        details: usize,
    ) -> crate::app::database::DatabaseTableReviewSummary {
        crate::app::database::DatabaseTableReviewSummary {
            inserted_rows: 0,
            updated_rows: 0,
            changed_cells: 0,
            deleted_rows: 0,
            detail_rows: (0..details).map(|index| format!("detail-{index}")).collect(),
            notices: (0..notices).map(|index| format!("notice-{index}")).collect(),
            truncated_details: false,
        }
    }

    #[test]
    fn database_review_scroll_uses_fitted_modal_body_height() {
        let body_h = database_table_review_body_height(900.0, 700.0, 1.0);
        assert_eq!(body_h, 440.0);
        assert_eq!(database_table_review_max_scroll(900.0, 700.0, 1.0, 30), 220.0);

        let compact_body_h = database_table_review_body_height(500.0, 400.0, 1.0);
        assert!(compact_body_h < body_h);
        assert_eq!(
            database_table_review_max_scroll(500.0, 400.0, 1.0, 30),
            (database_table_review_content_height(1.0, 30) - compact_body_h).max(0.0)
        );
    }

    #[test]
    fn review_content_height_and_max_scroll_share_one_rounded_step() {
        for scale in [1.0_f32, 1.25, 1.5, 1.75, 1.37] {
            let step = database_table_review_line_step(scale);
            assert_eq!(step.fract(), 0.0, "scale={scale}");
            let body_h = database_table_review_body_height(913.0, 677.0, scale);
            let content_h = database_table_review_content_height(scale, 37);
            assert_eq!(content_h, step * 37.0, "scale={scale}");
            assert_eq!(
                database_table_review_max_scroll(913.0, 677.0, scale, 37),
                (content_h - body_h).max(0.0),
                "scale={scale}"
            );
        }
    }

    #[test]
    fn review_visible_range_handles_top_middle_bottom_tiny_and_empty() {
        let scale = 1.0;
        let step = database_table_review_line_step(scale);
        assert_eq!(database_table_review_visible_range(0, 44.0, 0.0, scale), 0..0);
        assert_eq!(database_table_review_visible_range(100, 44.0, 0.0, scale), 0..5);
        assert_eq!(
            database_table_review_visible_range(100, 44.0, step * 10.0, scale),
            9..14
        );
        let bottom = database_table_review_content_height(scale, 100) - 44.0;
        assert_eq!(
            database_table_review_visible_range(100, 44.0, bottom, scale),
            97..100
        );
        assert_eq!(database_table_review_visible_range(100, 1.0, 0.0, scale), 0..4);
    }

    #[test]
    fn review_logical_index_space_maps_notices_then_details_without_offscreen_visits() {
        let summary = review_summary(2, 8);
        let mut visited = Vec::new();
        visit_database_table_review_lines(&summary, 1..5, |index, notice, text| {
            visited.push((index, notice, text.to_string()));
        });
        assert_eq!(
            visited,
            vec![
                (1, true, "notice-1".to_string()),
                (2, false, "detail-0".to_string()),
                (3, false, "detail-1".to_string()),
                (4, false, "detail-2".to_string()),
            ]
        );

        for (summary, expected_notice) in [
            (review_summary(4, 0), true),
            (review_summary(0, 4), false),
        ] {
            let mut seen = Vec::new();
            visit_database_table_review_lines(&summary, 0..4, |_, notice, _| seen.push(notice));
            assert_eq!(seen, vec![expected_notice; 4]);
        }

        let large = review_summary(500, 1_500);
        let mut calls = 0usize;
        visit_database_table_review_lines(&large, 1_250..1_258, |_, _, _| calls += 1);
        assert_eq!(calls, 8, "render helper must visit only requested visible/overscan rows");
    }

    #[test]
    fn sql_visible_range_is_bounded_at_top_middle_and_bottom() {
        let line_h = 33.0;
        assert_eq!(database_sql_visible_line_range(0, 100.0, 0.0, line_h), 0..0);
        assert_eq!(database_sql_visible_line_range(1_000, 66.0, 0.0, line_h), 0..4);
        assert_eq!(
            database_sql_visible_line_range(1_000, 66.0, line_h * 500.0, line_h),
            500..504
        );
        assert_eq!(
            database_sql_visible_line_range(1_000, 66.0, line_h * 999.0, line_h),
            999..1000
        );
    }

    #[test]
    fn large_sql_layout_is_built_once_and_visible_lookup_uses_cached_offsets() {
        let text = (0..4_096)
            .map(|index| format!("row_{index:04} SELECT 'Ж';"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut cache = crate::app::database::DatabaseMultilineLayoutCache::default();
        let mut measured = 0usize;
        assert!(cache.ensure(&text, 1.25, true, |line| {
            measured += 1;
            line.chars().count() as f32 * 11.0
        }));
        assert_eq!(measured, 4_096);
        let count = cache.line_count();
        for scroll_line in [0usize, 2_048, 4_090] {
            let range = database_sql_visible_line_range(
                count,
                4.0 * 33.0,
                scroll_line as f32 * 33.0,
                33.0,
            );
            assert!(range.len() <= 6);
            for index in range {
                let (start, end) = cache.line_range(index).expect("cached line range");
                assert!(text.get(start..end).is_some());
            }
        }
        assert!(!cache.ensure(&text, 1.25, true, |_| {
            measured += 1;
            0.0
        }));
        assert_eq!(measured, 4_096, "scroll-only reuse must not rescan the large text");
    }

    #[test]
    fn sql_visual_baseline_is_pixel_stable_and_uses_rounded_line_step() {
        for scale in [1.0_f32, 1.25, 1.5, 1.75, 1.37] {
            let line_h = (crate::app::database::DATABASE_SQL_PREVIEW_LINE_HEIGHT * scale)
                .round()
                .max(1.0);
            let render_scroll = 37.42_f32.round();
            let first = database_sql_visual_baseline(51.0, 17, line_h, render_scroll, scale);
            let again = database_sql_visual_baseline(51.0, 17, line_h, render_scroll, scale);
            let next = database_sql_visual_baseline(51.0, 18, line_h, render_scroll, scale);
            assert_eq!(first, again, "scale={scale}");
            assert_eq!(first.fract(), 0.0, "scale={scale}");
            assert_eq!(next - first, line_h, "scale={scale}");
            let selection_y = (first - 19.0 * scale).round();
            let caret_y = (first - 19.0 * scale).round();
            assert_eq!(selection_y, caret_y, "scale={scale}");
        }
    }

    #[test]
    fn refresh_prompt_geometry_is_repeatable_at_fractional_scales() {
        for scale in [1.0_f32, 1.25, 1.5, 1.75, 1.37] {
            for (viewport_w, viewport_h) in [(1280.0, 720.0), (913.0, 677.0), (604.0, 333.0)] {
                let geometry = || {
                    let fitted = crate::ui_system::fit_centered_rect(
                        viewport_w,
                        viewport_h,
                        560.0 * scale,
                        210.0 * scale,
                        16.0 * scale,
                    );
                    let title_baseline = (fitted.y + 32.0 * scale).round();
                    let body_baseline = (fitted.y + 78.0 * scale).round();
                    let buttons = database_modal_button_layout(
                        fitted.x,
                        fitted.y,
                        fitted.w,
                        fitted.h,
                        scale,
                        3,
                    )
                    .expect("refresh prompt buttons");
                    (
                        fitted.x,
                        fitted.y,
                        fitted.w,
                        fitted.h,
                        title_baseline,
                        body_baseline,
                        buttons,
                    )
                };
                let first = geometry();
                for _ in 0..8 {
                    assert_eq!(geometry(), first, "scale={scale} viewport={viewport_w}x{viewport_h}");
                }
                assert_eq!(first.4.fract(), 0.0);
                assert_eq!(first.5.fract(), 0.0);
                let (_, button_y, _, button_h, _) = first.6;
                assert!(button_y >= first.1);
                assert!(button_y + button_h <= first.1 + first.3 + 0.01);
            }
        }
    }

    #[test]
    fn sql_preview_outer_rect_stays_inside_narrow_modal() {
        for scale in [1.0_f32, 1.25, 1.5, 1.75, 1.37] {
            let x = 11.0;
            let y = 17.0;
            let w = 180.0 * scale;
            let h = 180.0 * scale;
            let outer = database_sql_preview_outer_rect(x, y, w, h, scale);
            assert!(outer.x >= x);
            assert!(outer.y >= y);
            assert!(outer.w > 0.0);
            assert!(outer.h > 0.0);
            assert!(outer.x + outer.w <= x + w + 0.01, "scale={scale}");
            assert!(outer.y + outer.h <= y + h + 0.01, "scale={scale}");
            let (_, button_y, _, _, _) =
                database_modal_button_layout(x, y, w, h, scale, 2).expect("sql modal buttons");
            assert!(outer.y + outer.h <= button_y + 0.01, "scale={scale}");
        }
    }

    #[test]
    fn a4_b014_modal_buttons_stay_inside_fitted_modal() {
        for (w, h) in [(120.0, 80.0), (200.0, 120.0), (300.0, 180.0)] {
            for count in 1..=3 {
                let (x, y, button_w, button_h, gap) =
                    database_modal_button_layout(10.0, 20.0, w, h, 1.0, count).unwrap();
                let right = x + button_w * count as f32 + gap * count.saturating_sub(1) as f32;
                assert!(x >= 10.0);
                assert!(right <= 10.0 + w + 0.01);
                assert!(y >= 20.0);
                assert!(y + button_h <= 20.0 + h + 0.01);
            }
        }
    }
}
