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

    pub fn draw_dialog(&mut self, base_title: &str) {
        unsafe {
            self.gl.bind_vertex_array(Some(self.vao));
            self.gl.use_program(Some(self.program));
            self.gl.active_texture(glow::TEXTURE0);
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            self.gl.clear_color(
                self.theme.titlebar_bg[0],
                self.theme.titlebar_bg[1],
                self.theme.titlebar_bg[2],
                1.0,
            );
            self.gl.clear(glow::COLOR_BUFFER_BIT);
        }

        let scale = self.scale_factor;
        let msg1 = format!("Документ «{}» был изменен.", base_title);
        let msg2 = "Сохранить или отклонить изменения?";

        let w1 = self.measure_ui_width(&msg1, 1.0);
        let w2 = self.measure_ui_width(msg2, 1.0);
        let text_w = w1.max(w2);

        let icon_sz = 110.0 * scale;
        let gap = 20.0 * scale;
        let total_content_w = icon_sz + gap + text_w;

        let start_x = (self.width - total_content_w) / 2.0;
        let icon_y = 15.0 * scale;

        if let Some(tex) = self.icon_warning {
            self.draw_icon(&tex, start_x, icon_y, icon_sz, icon_sz);
        }

        let text_x = start_x + icon_sz + gap;
        let fg = self.theme.fg;

        self.draw_string_scaled(&msg1, text_x, 65.0 * scale, fg, 1.0);
        self.draw_string_scaled(msg2, text_x, 95.0 * scale, fg, 1.0);

        let (btn_save, btn_discard, btn_cancel) =
            crate::widgets::get_dialog_buttons(self.width, self.height, scale, self);

        let mx = self.dialog_mouse_x;
        let my = self.dialog_mouse_y;
        let pressed = self.dialog_mouse_pressed;
        btn_save.render(self, mx, my, scale, pressed);
        btn_discard.render(self, mx, my, scale, pressed);
        btn_cancel.render(self, mx, my, scale, pressed);

        self.flush();
    }

    pub fn draw_faq(&mut self, faq_editor: &Editor, scroll_y: f32) {
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

        let content_x = 30.0 * scale;
        let content_y = 30.0 * scale;
        let content_w = self.width - 60.0 * scale;
        let content_h = self.height - 110.0 * scale;

        let card_bg = [0.169, 0.176, 0.188, 1.0];
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
        self.flush();

        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            let scissor_y = self.height - (content_y + content_h);
            self.gl.scissor(
                content_x as i32,
                scissor_y as i32,
                content_w as i32,
                content_h as i32,
            );
        }

        let start_x = content_x + 30.0 * scale;
        let mut y = content_y + 40.0 * scale - scroll_y;
        let text = faq_editor.get_full_text();

        let left_col_w = 260.0 * scale;

        for line in text.split('\n') {
            let is_header = line.starts_with("# ");

            if is_header {
                let text_color = [1.0, 1.0, 1.0, 1.0];
                let header_text = &line[2..];
                self.draw_string_scaled(header_text, start_x, y, text_color, 1.15);

                let line_y = y + 16.0 * scale;
                self.push_rect(
                    start_x,
                    line_y,
                    content_w - 60.0 * scale,
                    1.0,
                    [1.0, 1.0, 1.0, 0.08],
                );

                y += 50.0 * scale;
                continue;
            }

            if let Some(tab_idx) = line.find('\t') {
                let shortcut = &line[..tab_idx];
                let description = &line[tab_idx + 1..];

                let kbd_bg = [0.224, 0.231, 0.251, 1.0];
                let kbd_border = [0.306, 0.318, 0.341, 1.0];
                let kbd_text_color = [0.875, 0.882, 0.902, 1.0];

                let kbd_w = self.measure_ui_width(shortcut, 0.95) + 20.0 * scale;
                let kbd_h = 24.0 * scale;
                let kbd_x = start_x;
                let kbd_y = y - 18.0 * scale;

                self.push_rounded_rect(
                    kbd_x - 1.0,
                    kbd_y - 1.0,
                    kbd_w + 2.0,
                    kbd_h + 2.0,
                    4.0 * scale,
                    kbd_border,
                );
                self.push_rounded_rect(kbd_x, kbd_y, kbd_w, kbd_h, 4.0 * scale, kbd_bg);
                self.draw_string_scaled(
                    shortcut,
                    kbd_x + 10.0 * scale,
                    y - 1.0 * scale,
                    kbd_text_color,
                    0.95,
                );

                let desc_color = [0.663, 0.690, 0.729, 1.0];
                self.draw_string_scaled(description, start_x + left_col_w, y, desc_color, 1.0);

                y += 38.0 * scale;
                continue;
            }

            if !line.trim().is_empty() {
                let normal_color = [0.875, 0.882, 0.902, 1.0];
                self.draw_string_scaled(line.trim(), start_x, y, normal_color, 1.0);
                y += 30.0 * scale;
            } else {
                y += 15.0 * scale;
            }
        }

        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }

        let max_scroll = self.get_faq_max_scroll(faq_editor, self.height);
        let total_content_h = content_h + max_scroll;

        if max_scroll > 0.0 {
            let scroll_ratio = (scroll_y / max_scroll).clamp(0.0, 1.0);
            let track_h = content_h - 16.0 * scale;
            let thumb_h = (content_h / total_content_h * track_h).max(40.0 * scale);
            let thumb_y = content_y + 8.0 * scale + scroll_ratio * (track_h - thumb_h);
            let scroll_x = content_x + content_w - 14.0 * scale;

            self.push_rounded_rect(
                scroll_x,
                thumb_y,
                6.0 * scale,
                thumb_h,
                3.0 * scale,
                [0.40, 0.42, 0.46, 1.0],
            );
        }

        let btn_ok = crate::widgets::get_faq_button(self.width, self.height, scale, self);
        btn_ok.render(
            self,
            self.dialog_mouse_x,
            self.dialog_mouse_y,
            scale,
            self.dialog_mouse_pressed,
        );

        self.flush();
    }

    pub fn get_faq_byte_at(
        &mut self,
        faq_editor: &Editor,
        _target_x: f32,
        target_y: f32,
        scroll_y: f32,
    ) -> usize {
        let scale = self.scale_factor;
        let content_y = 30.0 * scale;
        let mut y = content_y + 40.0 * scale - scroll_y;

        let text = faq_editor.get_full_text();
        let mut last_valid = 0;

        for line in text.split('\n') {
            let line_h = if line.starts_with("# ") {
                50.0 * scale
            } else if line.contains('\t') {
                38.0 * scale
            } else if !line.trim().is_empty() {
                30.0 * scale
            } else {
                15.0 * scale
            };

            if target_y >= y - 25.0 * scale && target_y < y - 25.0 * scale + line_h {
                return last_valid;
            }

            last_valid += line.len() + 1;
            y += line_h;
        }

        last_valid.saturating_sub(1)
    }

    pub fn get_faq_max_scroll(&mut self, faq_editor: &Editor, dialog_height: f32) -> f32 {
        let scale = self.scale_factor;
        let mut total_h = 40.0 * scale;

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
        let content_h = dialog_height - 110.0 * scale;

        (total_h - content_h).max(0.0)
    }

    pub fn draw_welcome(&mut self, recent_files: &[std::path::PathBuf]) {
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

        self.draw_string_scaled(
            "Добро пожаловать в RRiter",
            title_x,
            y,
            [1.0, 1.0, 1.0, 1.0],
            1.5,
        );
        y += 40.0 * scale;
        self.draw_string_scaled(
            "Молниеносный текстовый редактор с GPU-рендерингом",
            title_x,
            y,
            [0.7, 0.7, 0.75, 1.0],
            1.0,
        );

        y += 60.0 * scale;
        let (btn_new, btn_open) =
            crate::widgets::get_welcome_buttons(content_w, title_x, y, scale, self);

        btn_new.render(self, self.last_mouse_x, self.last_mouse_y, scale, false);
        btn_open.render(self, self.last_mouse_x, self.last_mouse_y, scale, false);

        y += 80.0 * scale;
        self.draw_string_scaled("Недавние файлы", title_x, y, [1.0, 1.0, 1.0, 1.0], 1.1);

        // Линия-разделитель под заголовком (как в FAQ)
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
            let is_hovered = self.last_mouse_x >= title_x - 10.0 * scale
                && self.last_mouse_x <= title_x + content_w - 70.0 * scale
                && self.last_mouse_y >= y
                && self.last_mouse_y < y + item_h;

            if is_hovered {
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
                0.85,
            );

            // Тонкий разделитель между элементами списка
            self.push_rect(
                title_x,
                y + item_h - 1.0,
                content_w - 80.0 * scale,
                1.0,
                [1.0, 1.0, 1.0, 0.04],
            );

            y += item_h;
        }

        self.flush();
    }
}
