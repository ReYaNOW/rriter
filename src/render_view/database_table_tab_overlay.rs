use crate::renderer::Renderer;
use crate::ui_system::{UiId, UiRegistry};
use crate::widgets::ButtonView;
use glow::HasContext;

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
    ) {
        ui.mark_overlay_start();
        self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.66]);
        ui.register_blocker(UiId::DatabaseTableModalBackdrop, 0.0, 0.0, self.width, self.height, mx, my);
        let (title, width, height) = match modal {
            crate::app::database::DatabaseTableModal::SqlPreview { .. } => ("SQL preview", 880.0, 650.0),
            crate::app::database::DatabaseTableModal::RefreshPrompt { close_after_save, .. } => (
                if *close_after_save { "Закрыть изменённую таблицу?" } else { "Обновить изменённую таблицу?" },
                560.0,
                210.0,
            ),
            crate::app::database::DatabaseTableModal::CustomLimit { .. } => ("Количество строк", 460.0, 210.0),
            crate::app::database::DatabaseTableModal::MultilineEditor { .. } => ("Редактор значения", 820.0, 620.0),
            crate::app::database::DatabaseTableModal::Review { .. } => ("Проверка транзакции", 780.0, 620.0),
        };
        let width = (width * s).min(self.width - 32.0 * s).max(360.0 * s);
        let height = (height * s).min(self.height - 32.0 * s).max(180.0 * s);
        let x = ((self.width - width) * 0.5).round();
        let y = ((self.height - height) * 0.5).round();
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
        self.draw_string_scaled_stable(title, x + 20.0 * s, y + 32.0 * s, self.theme.fg, 1.0);

        match modal {
            crate::app::database::DatabaseTableModal::SqlPreview { text, scroll, .. } => {
                self.draw_database_table_modal_text(x, y, width, height, s, text, scroll.current, ui, mx, my);
                draw_modal_buttons(self, ui, x, y, width, height, s, &[(UiId::DatabaseTableModalSecondary, "Закрыть")], mx, my);
            }
            crate::app::database::DatabaseTableModal::RefreshPrompt { .. } => {
                self.draw_string_scaled_stable(
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
                draw_modal_input(self, ui, x + 20.0 * s, y + 68.0 * s, width - 40.0 * s, 34.0 * s, input.text(), mx, my, s);
                if let Some(error) = error.as_deref() {
                    self.draw_string_scaled_stable(error, x + 20.0 * s, y + 126.0 * s, [0.95, 0.38, 0.42, 1.0], 0.76);
                } else {
                    self.draw_string_scaled_stable("Допустимо: 1–10 000", x + 20.0 * s, y + 126.0 * s, self.theme.line_num, 0.74);
                }
                draw_modal_buttons(self, ui, x, y, width, height, s, &[(UiId::DatabaseTableModalPrimary, "Применить"), (UiId::DatabaseTableModalSecondary, "Отмена")], mx, my);
            }
            crate::app::database::DatabaseTableModal::MultilineEditor { input, scroll, error, .. } => {
                let body_x = x + 18.0 * s;
                let body_y = y + 50.0 * s;
                let body_w = width - 36.0 * s;
                let body_h = height - 112.0 * s;
                self.push_rounded_rect_border(body_x, body_y, body_w, body_h, 4.0 * s, 1.0, [0.62,0.38,0.90,0.8], [0.055,0.06,0.08,1.0]);
                ui.register_text_input(UiId::DatabaseTableModalInput, body_x, body_y, body_w, body_h, mx, my);
                self.flush();
                unsafe {
                    self.gl.enable(glow::SCISSOR_TEST);
                    self.gl.scissor(body_x as i32, (self.height - (body_y + body_h)).max(0.0) as i32, body_w as i32, body_h as i32);
                }
                let mut line_y = body_y + 22.0 * s - scroll.current;
                for (line_index, line) in input.text().lines().enumerate() {
                    self.draw_string_scaled_stable(&(line_index + 1).to_string(), body_x + 8.0 * s, line_y, self.theme.line_num, 0.66);
                    self.draw_string_scaled_stable(line, body_x + 48.0 * s, line_y, self.theme.fg, 0.76);
                    line_y += 21.0 * s;
                }
                self.flush();
                unsafe { self.gl.disable(glow::SCISSOR_TEST) };
                ui.register_rect(UiId::DatabaseTableModalScroll, body_x + body_w - 12.0 * s, body_y, 12.0 * s, body_h, mx, my);
                if let Some(error) = error.as_deref() {
                    self.draw_string_scaled_stable(error, x + 20.0 * s, y + height - 70.0 * s, [0.95,0.38,0.42,1.0], 0.72);
                }
                draw_modal_buttons(self, ui, x, y, width, height, s, &[(UiId::DatabaseTableModalPrimary, "Применить"), (UiId::DatabaseTableModalTertiary, "Как текст"), (UiId::DatabaseTableModalSecondary, "Отмена")], mx, my);
            }
            crate::app::database::DatabaseTableModal::Review { state, scroll, .. } => {
                let remaining = state.deadline_unix_ms.saturating_sub(now_unix_ms()) / 1000;
                let summary = &state.summary;
                self.draw_string_scaled_stable(
                    &format!("Добавлено: {}   Изменено: {}   Ячеек: {}   Удалено: {}", summary.inserted_rows, summary.updated_rows, summary.changed_cells, summary.deleted_rows),
                    x + 20.0 * s,
                    y + 66.0 * s,
                    self.theme.fg,
                    0.82,
                );
                self.draw_string_scaled_stable(
                    &format!("До автоматического rollback: {}:{:02}", remaining / 60, remaining % 60),
                    x + 20.0 * s,
                    y + 92.0 * s,
                    if remaining < 30 { [0.95,0.38,0.42,1.0] } else { [0.95,0.72,0.28,1.0] },
                    0.78,
                );
                let body_y = y + 112.0 * s;
                let body_h = height - 180.0 * s;
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
                    let max_scroll = (content_h - body_h).max(1.0);
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
                    self.draw_string_scaled_stable(
                        "Подробности ограничены; агрегаты рассчитаны полностью.",
                        body_x + 10.0 * s,
                        body_y + body_h - 8.0 * s,
                        [0.95,0.72,0.28,1.0],
                        0.7,
                    );
                }
                draw_modal_buttons(self, ui, x, y, width, height, s, &[(UiId::DatabaseTableModalPrimary, if state.committing { "Применение…" } else { "Apply" }), (UiId::DatabaseTableModalSecondary, "Rollback")], mx, my);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_database_table_modal_text(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        text: &str,
        scroll: f32,
        ui: &mut UiRegistry,
        mx: f32,
        my: f32,
    ) {
        let body_x = x + 18.0 * s;
        let body_y = y + 48.0 * s;
        let body_w = w - 36.0 * s;
        let body_h = h - 112.0 * s;
        self.push_rect(body_x, body_y, body_w, body_h, [0.055,0.06,0.08,1.0]);
        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            self.gl.scissor(body_x as i32, (self.height - (body_y + body_h)).max(0.0) as i32, body_w as i32, body_h as i32);
        }
        let mut cy = body_y + 22.0 * s - scroll;
        for line in text.lines() {
            self.draw_string_scaled_stable(line, body_x + 10.0 * s, cy, self.theme.fg, 0.74);
            cy += 21.0 * s;
        }
        self.flush();
        unsafe { self.gl.disable(glow::SCISSOR_TEST) };
        ui.register_rect(UiId::DatabaseTableModalScroll, body_x + body_w - 12.0 * s, body_y, 12.0 * s, body_h, mx, my);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_modal_input(renderer: &mut Renderer, ui: &mut UiRegistry, x: f32, y: f32, w: f32, h: f32, text: &str, mx: f32, my: f32, s: f32) {
    renderer.push_rounded_rect_border(x, y, w, h, 4.0 * s, 1.0, [0.62,0.38,0.90,1.0], [0.055,0.06,0.08,1.0]);
    ui.register_text_input(UiId::DatabaseTableModalInput, x, y, w, h, mx, my);
    renderer.draw_string_scaled_stable(text, x + 8.0 * s, y + 22.0 * s, renderer.theme.fg, 0.8);
}

#[allow(clippy::too_many_arguments)]
fn draw_modal_buttons(renderer: &mut Renderer, ui: &mut UiRegistry, x: f32, y: f32, w: f32, h: f32, s: f32, buttons: &[(UiId, &str)], mx: f32, my: f32) {
    let button_w = 110.0 * s;
    let total = buttons.len() as f32 * button_w + buttons.len().saturating_sub(1) as f32 * 8.0 * s;
    let mut bx = x + w - 20.0 * s - total;
    for (id, label) in buttons {
        ui.register_button_view(*id, ButtonView { x: bx, y: y + h - 48.0 * s, w: button_w, h: 30.0 * s, text: label, icon: None, text_scale: 0.75, icon_size: 0.0 }, renderer, mx, my, s, false);
        bx += button_w + 8.0 * s;
    }
}

fn now_unix_ms() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |value| value.as_millis())
}
