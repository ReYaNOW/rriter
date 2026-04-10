use crate::editor::Editor;
use crate::renderer::Renderer;
use glow::HasContext;

impl Renderer {
    pub fn draw_icon(&mut self, tex: &glow::Texture, x: f32, y: f32, w: f32, h: f32) {
        self.flush();
        unsafe {
            self.gl.bind_texture(glow::TEXTURE_2D, Some(*tex));
        }
        self.push_quad(x, y, w, h, 0.0, 0.0, 1.0, 1.0, [1.0, 1.0, 1.0, 1.0], 1.0);
        self.flush();
        unsafe {
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
        }
    }

    pub fn draw_atlas_icon(&mut self, icon: crate::widgets::IconType, x: f32, y: f32, size: f32, color: [f32; 4]) {
        if let Some(&tex) = self.icons.get(&icon) {
            self.flush(); // Сбрасываем батч, чтобы сменить текстуру
            unsafe {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            }
            self.push_quad(x, y, size, size, 0.0, 0.0, 1.0, 1.0, color, 5.0);
            self.flush();
            unsafe {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture)); // Возвращаем шрифтовой атлас
            }
        }
    }

    // (функции удалены)

    pub fn draw_autocomplete(
        &mut self,
        x: f32,
        mut y: f32,
        options: &[(crate::highlighter::CompletionItem, Vec<usize>)],
        selected_idx: usize,
        anim_progress: f32,
        scroll_y: f32,
        hovered_idx: Option<usize>,
    ) -> (f32, f32, f32, f32) {
        let scale = self.scale_factor;

        let step = 36.0 * scale;
        let item_h = 28.0 * scale;
        let padding_top = 8.0 * scale;
        let padding_bottom = 8.0 * scale;

        let mut max_w = 195.0 * scale;
        for (opt, _) in options {
            let w = self.measure_width(opt.word.as_str(), "", 0, opt.word.len());
            if w + 60.0 * scale > max_w {
                max_w = w + 60.0 * scale;
            }
        }

        max_w = max_w.min(450.0 * scale);

        let visible_items = options.len().min(7);

        let target_h = visible_items as f32 * step + padding_top + padding_bottom;
        let total_h = options.len() as f32 * step + padding_top + padding_bottom;

        let current_h = target_h * anim_progress;

        if y + target_h > self.height {
            y -= target_h + 10.0 * scale;
        } else {
            y += 10.0 * scale;
        }

        // --- 1. Отрисовка Тени ---
        for i in 1..=5 {
            let offset = i as f32 * scale;
            let alpha = (0.15 - (i as f32 * 0.03)) * anim_progress;
            self.push_rounded_rect(
                x - offset,
                y - offset,
                max_w + offset * 2.0,
                current_h + offset * 2.0,
                6.0 * scale,
                [0.0, 0.0, 0.0, alpha],
            );
        }

        // --- 2. Рамка и Фон ---
        let bg_color = [0.15, 0.16, 0.20, 1.0];
        let border_color = [self.theme.sel[0], self.theme.sel[1], self.theme.sel[2], 0.8];

        // ИСПРАВЛЕНИЕ: Делаем рамку толще и математически синхронизируем внутренний радиус
        let border_width = 1.5 * scale;
        self.push_rounded_rect(
            x - border_width,
            y - border_width,
            max_w + border_width * 2.0,
            current_h + border_width * 2.0,
            5.5 * scale, // Внешний радиус
            border_color,
        );
        self.push_rounded_rect(
            x,
            y,
            max_w,
            current_h,
            4.0 * scale, // Внутренний радиус (ровно 5.5 - 1.5), чтобы не было "точек" на углах
            bg_color,
        );

        self.flush();

        // --- 3. Scissor Test ---
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            let sy = (self.height - (y + current_h)).round() as i32;
            self.gl.scissor(
                x.round() as i32,
                sy,
                max_w.round() as i32,
                current_h.round() as i32,
            );
        }

        // --- 4. Отрисовка элементов ---
        let mut current_y = y + padding_top - scroll_y;

        for (i, (item, matches)) in options.iter().enumerate() {
            if current_y + step < y || current_y > y + current_h {
                current_y += step;
                continue;
            }

            let sel_rect_y = (current_y + (step - item_h) / 2.0).round();

            if i == selected_idx {
                self.push_rounded_rect(
                    x + 4.0 * scale,
                    sel_rect_y,
                    max_w - 8.0 * scale,
                    item_h,
                    4.0 * scale,
                    [0.25, 0.27, 0.35, 1.0],
                );
            } else if Some(i) == hovered_idx {
                self.push_rounded_rect(
                    x + 4.0 * scale,
                    sel_rect_y,
                    max_w - 8.0 * scale,
                    item_h,
                    4.0 * scale,
                    [0.20, 0.21, 0.28, 1.0],
                );
            }

            let mut cx = x + 12.0 * scale;

            let (icon_char, icon_fg) = match item.kind {
                crate::highlighter::SymbolKind::Class => ("\u{f03d7}", [0.8, 0.9, 1.0, 1.0]),
                crate::highlighter::SymbolKind::Function => ("\u{f0295}", [0.8, 1.0, 0.8, 1.0]),
                crate::highlighter::SymbolKind::Variable => ("\u{f0ae7}", [0.9, 0.8, 1.0, 1.0]),
                crate::highlighter::SymbolKind::Parameter => ("\u{f03ea}", [1.0, 0.9, 0.8, 1.0]),
                crate::highlighter::SymbolKind::Keyword => ("\u{f030b}", [1.0, 0.8, 0.9, 1.0]),
                crate::highlighter::SymbolKind::Unknown => ("\u{f03d7}", [0.65, 0.65, 0.65, 1.0]),
            };

            let icon_sz = 20.0 * scale;

            if let Some(g) = self.get_glyph(icon_char.chars().next().unwrap()) {
                let char_scale = 0.8;
                let actual_w = g.width * char_scale * scale;
                let actual_h = g.height * char_scale * scale;

                let char_x = cx + (icon_sz - actual_w) / 2.0;
                let char_y = sel_rect_y + (item_h - actual_h) / 2.0;

                self.push_quad(
                    char_x.round(),
                    char_y.round(),
                    actual_w,
                    actual_h,
                    g.u,
                    g.v,
                    g.uw,
                    g.vh,
                    icon_fg,
                    0.0,
                );
            }
            cx += icon_sz + 8.0 * scale;

            let cy = sel_rect_y + item_h * 0.72;

            let mut truncated = false;
            for (j, c) in item.word.chars().enumerate() {
                if let Some(g) = self.get_glyph(c) {
                    if cx + g.advance > x + max_w - 30.0 * scale {
                        truncated = true;
                        break;
                    }

                    let color = if matches.contains(&j) {
                        [1.0, 0.474, 0.776, 1.0]
                    } else {
                        self.theme.fg
                    };

                    self.push_quad(
                        (cx + g.offset_x).round(),
                        (cy - g.offset_y).round(),
                        g.width,
                        g.height,
                        g.u,
                        g.v,
                        g.uw,
                        g.vh,
                        color,
                        g.is_emoji,
                    );
                    cx += g.advance;
                }
            }

            if truncated {
                self.draw_string_scaled("...", cx.round(), cy.round(), [0.5, 0.5, 0.55, 1.0], 1.0);
            }

            current_y += step;
        }

        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }

        // --- 5. Отрисовка Скроллбара (стиль как в главном окне) ---
        if total_h > target_h {
            let max_scroll = (total_h - target_h).max(0.0);
            let scroll_ratio = (scroll_y / max_scroll).clamp(0.0, 1.0);

            let track_margin = 8.0 * scale;
            let track_h = current_h - track_margin * 2.0;
            let thumb_h = (current_h / total_h * track_h).max(20.0 * scale);
            let thumb_y = y + track_margin + scroll_ratio * (track_h - thumb_h);

            let alpha = (anim_progress * 1.5).clamp(0.0, 0.8);

            self.push_rounded_rect(
                x + max_w - 10.0 * scale,
                thumb_y,
                6.0 * scale,
                thumb_h,
                3.0 * scale,
                [0.7, 0.33, 0.54, alpha],
            );
        }

        self.flush();

        (x, y, max_w, current_h)
    }

                pub fn draw_dialog_window(&mut self, base_title: &str) -> bool {
        let s = self.scale_factor;
        let box_w = 660.0 * s;
        let box_h = 260.0 * s;
        let box_x = 0.0;
        let box_y = 0.0;

        let top_color = [0.26, 0.20, 0.36, 1.0];
        let bottom_color = [0.12, 0.13, 0.22, 1.0];

        self.push_vertical_gradient(box_x, box_y, box_w, box_h, top_color, bottom_color);

        let pad_h = 24.0 * s;
        let pad_v = 18.0 * s;
        let btn_h = 44.0 * s;
        let btn_margin = 12.0 * s;
        let content_x = (box_x + pad_h).round();
        let content_y = (box_y + pad_v).round();
        let content_w = (box_w - pad_h * 2.0).round();
        let content_h = (box_h - pad_v - btn_h - btn_margin * 2.0 - pad_v).round();

        self.push_rounded_rect(content_x - 1.0, content_y - 1.0, content_w + 2.0, content_h + 2.0, 8.0 * s, [0.224, 0.231, 0.251, 0.8]);
        self.push_rounded_rect(content_x, content_y, content_w, content_h, 8.0 * s, [0.15, 0.16, 0.20, 1.0]);

        let msg1 = format!("Документ «{}» был изменен.", base_title);
        let msg2 = "Сохранить или отклонить изменения?";

                let icon_sz = 120.0 * s;
        let gap = 45.0 * s;
        let padding_inner = 20.0 * s;

        let icon_x = content_x + padding_inner;
        let icon_y = content_y + (content_h - icon_sz) / 2.0;

        self.draw_atlas_icon(crate::widgets::IconType::Warning, icon_x, icon_y, icon_sz, [1.0, 1.0, 1.0, 1.0]);

        let text_x = icon_x + icon_sz + gap;
        let fg = self.theme.fg;
        let text_scale = 1.05;
        let line_h = 28.0 * s;
        let text_block_h = line_h * 2.0;
        let text_y_start = content_y + (content_h - text_block_h) / 2.0 + line_h * 0.85;

        self.draw_string_scaled(&msg1, text_x, text_y_start, fg, text_scale);
        self.draw_string_scaled(msg2, text_x, text_y_start + line_h, [0.75, 0.75, 0.80, 1.0], text_scale);

        let (btn_save, btn_discard, btn_cancel) =
            crate::widgets::get_dialog_buttons(box_x, box_y, box_w, box_h, s, self);

        let mx = self.last_mouse_x;
        let my = self.last_mouse_y;

        let mut wants_pointer = false;
        wants_pointer |= btn_save.render(self, mx, my, s, false);
        wants_pointer |= btn_discard.render(self, mx, my, s, false);
        wants_pointer |= btn_cancel.render(self, mx, my, s, false);

        self.flush();
        wants_pointer
    }

    pub fn get_faq_max_scroll(&mut self, faq_editor: &Editor, dialog_height: f32) -> f32 {
        let scale = self.scale_factor;
        let mut total_h = 0.0;

        for line in faq_editor.get_full_text().split('\n') {
            if line.starts_with("# ") {
                total_h += 50.0 * scale;
            } else if line.contains('\t') {
                total_h += 38.0 * scale;
            } else if !line.trim().is_empty() {
                total_h += 30.0 * scale;
            } else {
                total_h += 15.0 * scale;
            }
        }

                        total_h += 80.0 * scale;
        let pad_top = 35.0 * scale;
        let pad_bottom = 30.0 * scale;
        let title_h = 40.0 * scale;
        let content_h = dialog_height - pad_top - pad_bottom - title_h - 20.0 * scale;

        (total_h - content_h).max(0.0)
    }

    pub fn draw_welcome(&mut self, recent_files: &[std::path::PathBuf]) -> bool {
        let scale = self.scale_factor;

        let top_color = [0.26, 0.20, 0.36, 1.0];
        let bottom_color = [0.12, 0.13, 0.22, 1.0];

        unsafe {
            self.gl.bind_vertex_array(Some(self.vao));
            self.gl.use_program(Some(self.program));
            self.gl.active_texture(glow::TEXTURE0);
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            self.gl
                .clear_color(bottom_color[0], bottom_color[1], bottom_color[2], 1.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
        }

        self.push_vertical_gradient(
            -1.0,
            -1.0,
            self.width + 2.0,
            self.height + 2.0,
            top_color,
            bottom_color,
        );
        self.flush();

        let content_x = 40.0 * scale;
        let content_y = 40.0 * scale;
        let content_w = self.width - 80.0 * scale;
        let content_h = self.height - 80.0 * scale;

        let card_bg = [0.169, 0.176, 0.188, 0.95];
        let card_border = [0.224, 0.231, 0.251, 1.0];

        self.push_rounded_rect(
            content_x - 1.0,
            content_y - 1.0,
            content_w + 2.0,
            content_h + 2.0,
            10.0 * scale,
            card_border,
        );
        self.push_rounded_rect(
            content_x,
            content_y,
            content_w,
            content_h,
            10.0 * scale,
            card_bg,
        );

        let title_x = content_x + 40.0 * scale;
        let mut y = content_y + 60.0 * scale;

        if let Some(tex) = self.icon_logo {
            let icon_y = y - 40.0 * scale;
            self.draw_icon(&tex, title_x, icon_y, 110.0 * scale, 110.0 * scale);
        }

        self.draw_string_scaled(
            "Добро пожаловать в RRiter",
            title_x + 130.0 * scale,
            y,
            [0.741, 0.576, 0.976, 1.0],
            1.0,
        );
        y += 40.0 * scale;
        self.draw_string_scaled(
            "Молниеносный текстовый редактор с GPU-рендерингом",
            title_x + 130.0 * scale,
            y,
            [0.7, 0.7, 0.75, 1.0],
            1.0,
        );

                y += 60.0 * scale;
        let (btn_new, btn_open, btn_ide) =
            crate::widgets::get_welcome_buttons(content_w, title_x, y, scale, self);

        let mut wants_pointer = false;
        wants_pointer |= btn_new.render(self, self.last_mouse_x, self.last_mouse_y, scale, false);
        wants_pointer |= btn_open.render(self, self.last_mouse_x, self.last_mouse_y, scale, false);
        wants_pointer |= btn_ide.render(self, self.last_mouse_x, self.last_mouse_y, scale, false);

        y += 80.0 * scale;
        self.draw_string_scaled(
            "Недавние файлы",
            title_x,
            y,
            [0.741, 0.576, 0.976, 1.0],
            1.0,
        );

        let line_y = y + 20.0 * scale;
        self.push_rect(
            title_x,
            line_y,
            content_w - 80.0 * scale,
            1.0,
            [1.0, 1.0, 1.0, 0.08],
        );

        y += 35.0 * scale;

        let item_h = 44.0 * scale;
        for path in recent_files {
            if y + item_h > content_y + content_h - 60.0 * scale {
                break;
            }

            let is_hovered = self.last_mouse_x >= title_x - 10.0 * scale
                && self.last_mouse_x <= title_x + content_w - 70.0 * scale
                && self.last_mouse_y >= y
                && self.last_mouse_y < y + item_h;

            if is_hovered {
                wants_pointer = true;
                self.push_rounded_rect(
                    title_x - 10.0 * scale,
                    y,
                    content_w - 60.0 * scale,
                    item_h,
                    6.0 * scale,
                    [1.0, 1.0, 1.0, 0.05],
                );
            }

            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let full_dir = path
                .parent()
                .unwrap_or(std::path::Path::new(""))
                .to_string_lossy();

            self.draw_string_scaled(&name, title_x, y + 25.0 * scale, [0.9, 0.9, 0.9, 1.0], 1.0);
            let name_w = self.measure_ui_width(&name, 1.0);
            self.draw_string_scaled(
                &full_dir,
                title_x + name_w + 15.0 * scale,
                y + 25.0 * scale,
                [0.5, 0.5, 0.5, 1.0],
                0.95,
            );

            self.push_rect(
                title_x,
                y + item_h - 1.0,
                content_w - 80.0 * scale,
                1.0,
                [1.0, 1.0, 1.0, 0.04],
            );

            y += item_h;
        }

                let hint_str_1 = "F1";
        let hint_str_2 = " — Настройки редактора";
        let scale_hint = 0.9;

        let w1 = self.measure_ui_width(hint_str_1, scale_hint) + 16.0 * scale;
        let w2 = self.measure_ui_width(hint_str_2, scale_hint);
        let hint_total_w = w1 + w2;

        let hint_x = content_x + content_w - hint_total_w - 30.0 * scale;
        let hint_y = content_y + content_h - 30.0 * scale;

        let kbd_bg = [0.224, 0.231, 0.251, 1.0];
        let kbd_border = [0.306, 0.318, 0.341, 1.0];
        let kbd_text_color = [0.875, 0.882, 0.902, 1.0];

        let kbd_h = 22.0 * scale;
        let kbd_draw_y = hint_y - 16.0 * scale;

        self.push_rounded_rect(
            hint_x - 1.0,
            kbd_draw_y - 1.0,
            w1 + 2.0,
            kbd_h + 2.0,
            4.0 * scale,
            kbd_border,
        );
        self.push_rounded_rect(hint_x, kbd_draw_y, w1, kbd_h, 4.0 * scale, kbd_bg);

        self.draw_string_scaled(
            hint_str_1,
            hint_x + 8.0 * scale,
            hint_y,
            kbd_text_color,
            scale_hint,
        );

        self.draw_string_scaled(
            hint_str_2,
            hint_x + w1,
            hint_y,
            [0.5, 0.5, 0.55, 1.0],
            scale_hint,
        );

        self.flush();
        wants_pointer
    }

            pub fn draw_settings(&mut self, anim_progress: f32, active_tab: usize, faq_editor: &Editor, scroll_y: f32, ide_workspaces: &[std::path::PathBuf]) -> bool {
        if anim_progress <= 0.0 { return false; }
        let s = self.scale_factor;
        let mut wants_pointer = false;

        self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.4 * anim_progress]);

                let w = (1000.0 * s).min(self.width - 40.0 * s);
        let h = (700.0 * s).min(self.height - 40.0 * s);

                let start_y = self.height + 100.0 * s;
        let target_y = (self.height - h) / 2.0;
        let raw_y = start_y + (target_y - start_y) * anim_progress;
        let y = raw_y.round();
        let x = ((self.width - w) / 2.0).round();

        let top_color = [0.26, 0.20, 0.36, 1.0];
        let bottom_color = [0.12, 0.13, 0.22, 1.0];

        // 1. Внешнее окно с градиентом
        self.push_rounded_rect(x - 1.0, y - 1.0, w + 2.0, h + 2.0, 10.0 * s, [0.224, 0.231, 0.251, 1.0]);
        self.push_rounded_rect_gradient(x, y, w, h, 10.0 * s, top_color, bottom_color);

        // 2. Внутренняя панель
        let pad_top = 35.0 * s;
        let pad_bottom = 30.0 * s;
        let pad_h = 40.0 * s;
        let ix = x + pad_h;
        let iy = y + pad_top;
        let iw = w - pad_h * 2.0;
        let ih = h - pad_top - pad_bottom;

        self.push_rounded_rect(ix - 1.0, iy - 1.0, iw + 2.0, ih + 2.0, 8.0 * s, [0.224, 0.231, 0.251, 0.8]);
        self.push_rounded_rect(ix, iy, iw, ih, 8.0 * s, [0.15, 0.16, 0.20, 1.0]);

        self.flush();

                let sidebar_w = 200.0 * s;
        self.push_rect(ix + sidebar_w, iy, 1.0, ih, [1.0, 1.0, 1.0, 0.05]);

        let tabs = ["IDE", "Основные", "Редактор", "Внешний вид", "Помощь"];
        let mut tab_y = iy + 20.0 * s;
        for (i, title) in tabs.iter().enumerate() {
            let tab_rect_y = tab_y;
            let tab_rect_h = 36.0 * s;

            let is_hovered = self.last_mouse_x >= ix + 10.0 * s && self.last_mouse_x <= ix + sidebar_w - 10.0 * s 
                          && self.last_mouse_y >= tab_rect_y && self.last_mouse_y <= tab_rect_y + tab_rect_h;

            if is_hovered { wants_pointer = true; }

            if i == active_tab {
                self.push_rounded_rect(ix + 10.0 * s, tab_rect_y, sidebar_w - 20.0 * s, tab_rect_h, 6.0 * s, [1.0, 1.0, 1.0, 0.1]);
            } else if is_hovered {
                self.push_rounded_rect(ix + 10.0 * s, tab_rect_y, sidebar_w - 20.0 * s, tab_rect_h, 6.0 * s, [1.0, 1.0, 1.0, 0.05]);
            }

            let color = if i == active_tab { [1.0, 1.0, 1.0, 1.0] } else { [0.7, 0.7, 0.7, 1.0] };
            self.draw_string_scaled(title, ix + 25.0 * s, tab_y + 24.0 * s, color, 0.95);
            tab_y += tab_rect_h + 4.0 * s;
        }

        let content_x = ix + sidebar_w + 30.0 * s;
        let content_title_x = content_x - 14.0 * s;
        let mut content_y = iy + 40.0 * s;

        let tab_title = tabs[active_tab];
        let pill_w = self.measure_ui_width(tab_title, 1.1) + 28.0 * s;
        let pill_h = 30.0 * s;
        let pill_y = content_y - 22.0 * s;
                self.push_rounded_rect(content_title_x - 1.0, pill_y - 1.0, pill_w + 2.0, pill_h + 2.0, 6.0 * s, [0.35, 0.26, 0.48, 1.0]);
        self.push_rounded_rect(content_title_x, pill_y, pill_w, pill_h, 6.0 * s, [0.26, 0.20, 0.36, 1.0]);
        self.draw_string_scaled(tab_title, content_title_x + 14.0 * s, content_y, [1.0, 1.0, 1.0, 1.0], 1.1);
        content_y += if active_tab == 4 { 30.0 * s } else { 46.0 * s };

        if active_tab == 0 {
            self.draw_string_scaled("Рабочие области (Воркспэйсы)", content_x, content_y, [0.8, 0.8, 0.8, 1.0], 1.0);
            content_y += 40.0 * s;

            for path in ide_workspaces {
                let path_str = path.to_string_lossy();
                self.draw_string_scaled(&path_str, content_x, content_y + 18.0 * s, [0.9, 0.9, 0.9, 1.0], 1.0);
                self.push_rounded_rect(content_x + 300.0 * s, content_y, 30.0 * s, 24.0 * s, 4.0 * s, [0.8, 0.3, 0.3, 1.0]);
                self.draw_string_scaled("-", content_x + 310.0 * s, content_y + 18.0 * s, [1.0, 1.0, 1.0, 1.0], 1.2);
                content_y += 34.0 * s;
            }

                                    let btn_add = crate::widgets::Button {
                x: content_x,
                y: content_y.round(),
                w: 190.0 * s,
                h: 36.0 * s,
                text: "Добавить папку".to_string(),
                icon: Some(crate::widgets::IconType::Plus),
                text_scale: 1.0,
                icon_size: 20.0 * s,
            };
            wants_pointer |= btn_add.render(self, self.last_mouse_x, self.last_mouse_y, s, false);

        } else if active_tab == 1 {
            self.draw_string_scaled("Скоро здесь появятся настройки...", content_x, content_y, [0.6, 0.6, 0.6, 1.0], 1.0);
        } else if active_tab == 2 {
            self.draw_string_scaled("Размер шрифта: 14px", content_x, content_y, [0.8, 0.8, 0.8, 1.0], 1.0);
            content_y += 30.0 * s;
            self.draw_string_scaled("Межстрочный интервал: 1.5", content_x, content_y, [0.8, 0.8, 0.8, 1.0], 1.0);
        } else if active_tab == 3 {
            self.draw_string_scaled("Тема: Dracula (По умолчанию)", content_x, content_y, [0.8, 0.8, 0.8, 1.0], 1.0);
        } else if active_tab == 4 {
            self.flush();
            let text_area_y = content_y;
            let text_area_h = ih - (text_area_y - iy) - 20.0 * s;

            unsafe {
                self.gl.enable(glow::SCISSOR_TEST);
                let scissor_y = self.height - (text_area_y + text_area_h);
                self.gl.scissor(
                    (content_x - 10.0 * s).round() as i32,
                    scissor_y.round() as i32,
                    (iw - sidebar_w - 10.0 * s).round() as i32,
                    text_area_h.round() as i32,
                );
            }

            let start_x = content_x;
            let main_header_x = content_x - 14.0 * s;
            let render_scroll_y = scroll_y.round();
            let mut text_y = text_area_y + 20.0 * s - render_scroll_y;
            let text = faq_editor.get_full_text();

            let left_col_w = 260.0 * s;
            let cw = iw - sidebar_w - 76.0 * s;
            let mut main_header_drawn = false;

            for line in text.split('\n') {
                let is_header = line.starts_with("# ");

                if is_header {
                    let header_text = &line[2..];
                    let is_main = !main_header_drawn && header_text == tab_title;

                    if is_main {
                        let pill_w = self.measure_ui_width(header_text, 1.05) + 24.0 * s;
                        let pill_h = 26.0 * s;
                        let pill_y = text_y - 19.0 * s;

                        self.push_rounded_rect(
                            main_header_x - 1.0,
                            pill_y - 1.0,
                            pill_w + 2.0,
                            pill_h + 2.0,
                            5.0 * s,
                            [0.35, 0.26, 0.48, 1.0],
                        );
                        self.push_rounded_rect(
                            main_header_x,
                            pill_y,
                            pill_w,
                            pill_h,
                            5.0 * s,
                            [0.26, 0.20, 0.36, 1.0],
                        );
                        self.draw_string_scaled(header_text, main_header_x + 12.0 * s, text_y, [1.0, 1.0, 1.0, 1.0], 1.05);
                        main_header_drawn = true;
                    } else {
                        let sep_y = text_y + 10.0 * s;
                        let sep_x = start_x + 8.0 * s;
                        let sep_w = (cw - 32.0 * s).max(0.0);
                        self.draw_string_scaled(header_text, start_x, text_y, [0.875, 0.882, 0.902, 1.0], 1.05);
                        self.push_rect(sep_x, sep_y, sep_w, 1.0, [1.0, 1.0, 1.0, 0.10]);
                    }

                    text_y += 50.0 * s;
                    continue;
                }

                if let Some(tab_idx) = line.find('\t') {
                    let shortcut = &line[..tab_idx];
                    let description = &line[tab_idx + 1..];

                    let kbd_bg = [0.224, 0.231, 0.251, 1.0];
                    let kbd_border = [0.306, 0.318, 0.341, 1.0];
                    let kbd_text_color = [0.875, 0.882, 0.902, 1.0];

                    let kbd_w = self.measure_ui_width(shortcut, 0.95) + 20.0 * s;
                    let kbd_h = 24.0 * s;
                    let kbd_x = start_x;
                    let kbd_y = text_y - 18.0 * s;

                    self.push_rounded_rect(
                        kbd_x - 1.0,
                        kbd_y - 1.0,
                        kbd_w + 2.0,
                        kbd_h + 2.0,
                        4.0 * s,
                        kbd_border,
                    );
                    self.push_rounded_rect(kbd_x, kbd_y, kbd_w, kbd_h, 4.0 * s, kbd_bg);
                    self.draw_string_scaled(
                        shortcut,
                        kbd_x + 10.0 * s,
                        text_y - 1.0 * s,
                        kbd_text_color,
                        0.95,
                    );

                    let desc_color = [0.663, 0.690, 0.729, 1.0];
                    self.draw_string_scaled(description, start_x + left_col_w, text_y, desc_color, 1.0);

                    text_y += 38.0 * s;
                    continue;
                }

                if !line.trim().is_empty() {
                    let normal_color = [0.875, 0.882, 0.902, 1.0];
                    self.draw_string_scaled(line.trim(), start_x, text_y, normal_color, 1.0);
                    text_y += 30.0 * s;
                } else {
                    text_y += 15.0 * s;
                }
            }

            self.flush();
            unsafe {
                self.gl.disable(glow::SCISSOR_TEST);
            }

                                    let max_scroll = self.get_faq_max_scroll(faq_editor, h);
            let total_content_h = text_area_h + max_scroll;

            if max_scroll > 0.0 {
                let scroll_ratio = (scroll_y / max_scroll).clamp(0.0, 1.0);
                let track_h = text_area_h;
                let thumb_h = (text_area_h / total_content_h * track_h).max(40.0 * s);
                let thumb_y = (text_area_y + scroll_ratio * (track_h - thumb_h)).round();
                                let scroll_x = (start_x + cw + 5.0 * s).round();

                self.push_rounded_rect(
                    scroll_x,
                    thumb_y,
                    6.0 * s,
                    thumb_h,
                    3.0 * s,
                    [0.7, 0.33, 0.54, 1.0],
                );
            }
        }

        self.flush();
        wants_pointer
    }
}
