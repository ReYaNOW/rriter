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
    (fitted.h - 180.0 * scale).max(0.0)
}

pub(crate) fn database_table_review_max_scroll(
    viewport_w: f32,
    viewport_h: f32,
    scale: f32,
    line_count: usize,
) -> f32 {
    let body_h = database_table_review_body_height(viewport_w, viewport_h, scale);
    (line_count as f32 * 22.0 * scale - body_h).max(0.0)
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_database_table_modal(
        &mut self,
        s: f32,
        modal: &crate::app::database::DatabaseTableModal,
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
                let visible_width = (input_w - 16.0 * s).max(1.0);
                let scroll_x = crate::app::file_tree::file_tree_name_input_scroll_x(
                    input.text(),
                    input.cursor,
                    visible_width,
                    |ch| {
                        self.get_ui_glyph(ch)
                            .map(|glyph| Self::snapped_text_advance(glyph.advance, 0.82))
                            .unwrap_or_else(|| (10.0_f32 * 0.82).round().max(1.0))
                    },
                );
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
                let body_y = y + 112.0 * s;
                let body_h = database_table_review_body_height(self.width, self.height, s);
                let body_x = x + 18.0 * s;
                let body_w = width - 36.0 * s;
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
                let mut cy = body_y + 23.0 * s - scroll.current;
                for notice in &summary.notices {
                    let mut scratch = String::new();
                    self.draw_tree_label_clipped(
                        &format!("⚠ {notice}"),
                        body_x + 10.0 * s,
                        cy,
                        body_w - 32.0 * s,
                        [0.95, 0.72, 0.28, 1.0],
                        0.72,
                        &mut scratch,
                    );
                    cy += 22.0 * s;
                }
                for detail in &summary.detail_rows {
                    let mut scratch = String::new();
                    self.draw_tree_label_clipped(
                        detail,
                        body_x + 10.0 * s,
                        cy,
                        body_w - 32.0 * s,
                        self.theme.fg,
                        0.72,
                        &mut scratch,
                    );
                    cy += 22.0 * s;
                }
                self.flush();
                unsafe { self.gl.disable(glow::SCISSOR_TEST) };

                let total_lines = summary.notices.len() + summary.detail_rows.len();
                let content_h = total_lines as f32 * 22.0 * s;
                if content_h > body_h {
                    let thumb_h = (body_h / content_h * body_h).max(30.0 * s).min(body_h);
                    let max_scroll = database_table_review_max_scroll(
                        self.width,
                        self.height,
                        s,
                        total_lines,
                    )
                    .max(1.0);
                    let ratio = (scroll.current / max_scroll).clamp(0.0, 1.0);
                    self.push_rounded_rect(
                        body_x + body_w - 8.0 * s,
                        body_y + ratio * (body_h - thumb_h),
                        5.0 * s,
                        thumb_h,
                        2.5 * s,
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
        blink_alpha: f32,
        ui: &mut UiRegistry,
        mx: f32,
        my: f32,
    ) {
        let outer_x = (x + 18.0 * s).round();
        let outer_y = (y + 50.0 * s).round();
        let outer_w = (w - 36.0 * s).round();
        let outer_h = (h - 116.0 * s).round();
        let scrollbar = (10.0 * s).round().max(10.0);
        let gutter_w = (54.0 * s).round();
        let line_h = (crate::app::database::DATABASE_SQL_PREVIEW_LINE_HEIGHT * s).round();
        let line_count = crate::app::database::database_multiline_line_count(text);
        let content_h = line_count as f32 * line_h;
        let content_w = crate::app::database::database_multiline_lines(text)
            .map(|(_, line)| self.database_code_text_width(line))
            .fold(0.0_f32, f32::max)
            + (18.0 * s).round();
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
        let first_line = (scroll_y / line_h).floor() as usize;
        let last_line = (first_line + (viewport_h / line_h).ceil() as usize + 2).min(line_count);
        for (line_index, (byte_offset, line)) in
            crate::app::database::database_multiline_lines(text).enumerate()
        {
            if line_index >= last_line {
                break;
            }
            if line_index < first_line {
                continue;
            }
            let baseline = outer_y
                + (line_index as f32 + 1.0) * line_h
                - scroll_y
                - (4.0 * s).round();
            self.draw_string_scaled_pixel_snapped(
                &(line_index + 1).to_string(),
                outer_x + (8.0 * s).round(),
                baseline,
                self.theme.line_num,
                0.78,
            );
            let line_end = byte_offset.saturating_add(line.len());
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
        database_modal_button_layout, database_table_review_body_height,
        database_table_review_max_scroll,
    };

    #[test]
    fn database_review_scroll_uses_fitted_modal_body_height() {
        let body_h = database_table_review_body_height(900.0, 700.0, 1.0);
        assert_eq!(body_h, 440.0);
        assert_eq!(database_table_review_max_scroll(900.0, 700.0, 1.0, 30), 220.0);

        let compact_body_h = database_table_review_body_height(500.0, 400.0, 1.0);
        assert!(compact_body_h < body_h);
        assert_eq!(
            database_table_review_max_scroll(500.0, 400.0, 1.0, 30),
            (30.0 * 22.0 - compact_body_h).max(0.0)
        );
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
