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

    pub fn draw_atlas_icon(
        &mut self,
        icon: crate::widgets::IconType,
        x: f32,
        y: f32,
        size: f32,
        color: [f32; 4],
    ) {
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

    /// Рисует SVG-иконку из кэша file_icon_cache.
    /// Загружает текстуру при первом обращении (не в draw-цикле — только при промахе кэша).
    pub fn draw_file_icon(
        &mut self,
        key: &'static str,
        _is_folder: bool,
        x: f32,
        y: f32,
        size: f32,
    ) {
        if !self.file_icon_cache.contains_key(key) {
            let mut cache = crate::app::file_tree::RASTERIZED_ICONS.lock().unwrap();

            if let Some(state) = cache.get(key) {
                if let Some(data) = state {
                    let data_clone = data.clone();
                    cache.remove(key); // Remove only when we are consuming it
                    drop(cache);

                    let target = 64i32;
                    let tex = unsafe {
                        let tex = self.gl.create_texture().unwrap();
                        self.gl.bind_texture(glow::TEXTURE_2D, Some(tex));
                        self.gl.tex_image_2d(
                            glow::TEXTURE_2D,
                            0,
                            glow::RGBA8 as i32,
                            target,
                            target,
                            0,
                            glow::RGBA,
                            glow::UNSIGNED_BYTE,
                            glow::PixelUnpackData::Slice(Some(&data_clone)),
                        );
                        self.gl.generate_mipmap(glow::TEXTURE_2D);
                        self.gl.tex_parameter_i32(
                            glow::TEXTURE_2D,
                            glow::TEXTURE_MIN_FILTER,
                            glow::LINEAR_MIPMAP_LINEAR as i32,
                        );
                        self.gl.tex_parameter_i32(
                            glow::TEXTURE_2D,
                            glow::TEXTURE_MAG_FILTER,
                            glow::LINEAR as i32,
                        );
                        self.gl.tex_parameter_i32(
                            glow::TEXTURE_2D,
                            glow::TEXTURE_WRAP_S,
                            glow::CLAMP_TO_EDGE as i32,
                        );
                        self.gl.tex_parameter_i32(
                            glow::TEXTURE_2D,
                            glow::TEXTURE_WRAP_T,
                            glow::CLAMP_TO_EDGE as i32,
                        );
                        self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
                        tex
                    };
                    self.file_icon_cache.insert(key, tex);
                } else {
                    // It's currently loading (None). Do nothing and wait.
                    return;
                }
            } else {
                // Not in cache at all. Mark as loading and spawn a thread.
                cache.insert(key, None);
                drop(cache);

                std::thread::spawn(move || {
                    // This function handles rendering the SVG and storing Some(data) back
                    crate::app::file_tree::pre_rasterize_icon(key, _is_folder);
                });
                return;
            }
        }

        if let Some(&tex) = self.file_icon_cache.get(key) {
            self.flush();
            unsafe {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(tex));
            }
            self.push_quad(
                x,
                y,
                size,
                size,
                0.0,
                0.0,
                1.0,
                1.0,
                [1.0, 1.0, 1.0, 1.0],
                5.0,
            );
            self.flush();
            unsafe {
                self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
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

        self.push_rounded_rect(
            content_x - 1.0,
            content_y - 1.0,
            content_w + 2.0,
            content_h + 2.0,
            8.0 * s,
            [0.224, 0.231, 0.251, 0.8],
        );
        self.push_rounded_rect(
            content_x,
            content_y,
            content_w,
            content_h,
            8.0 * s,
            [0.15, 0.16, 0.20, 1.0],
        );

        let msg1 = format!("Документ «{}» был изменен.", base_title);
        let msg2 = "Сохранить или отклонить изменения?";

        let icon_sz = 120.0 * s;
        let gap = 45.0 * s;
        let padding_inner = 20.0 * s;

        let icon_x = content_x + padding_inner;
        let icon_y = content_y + (content_h - icon_sz) / 2.0;

        self.draw_atlas_icon(
            crate::widgets::IconType::Warning,
            icon_x,
            icon_y,
            icon_sz,
            [1.0, 1.0, 1.0, 1.0],
        );

        let text_x = icon_x + icon_sz + gap;
        let fg = self.theme.fg;
        let text_scale = 1.05;
        let line_h = 28.0 * s;
        let text_block_h = line_h * 2.0;
        let text_y_start = content_y + (content_h - text_block_h) / 2.0 + line_h * 0.85;

        self.draw_string_scaled(&msg1, text_x, text_y_start, fg, text_scale);
        self.draw_string_scaled(
            msg2,
            text_x,
            text_y_start + line_h,
            [0.75, 0.75, 0.80, 1.0],
            text_scale,
        );

        let (btn_save, btn_discard, btn_cancel) =
            crate::widgets::get_dialog_buttons(box_x, box_y, box_w, box_h, s, self);

        let mx = self.last_mouse_x;
        let my = self.last_mouse_y;

        // Регистрируем через UI систему — убирает дублирование хитбоксов в input.rs
        let mut ui_reg = crate::ui_system::UiRegistry::new();
        ui_reg.register_button(
            crate::ui_system::UiId::DialogSave,
            &btn_save,
            self,
            mx,
            my,
            s,
            false,
        );
        ui_reg.register_button(
            crate::ui_system::UiId::DialogDiscard,
            &btn_discard,
            self,
            mx,
            my,
            s,
            false,
        );
        ui_reg.register_button(
            crate::ui_system::UiId::DialogCancel,
            &btn_cancel,
            self,
            mx,
            my,
            s,
            false,
        );

        self.flush();
        ui_reg.wants_pointer()
    }

    pub fn draw_welcome(
        &mut self,
        recent_files: &[std::path::PathBuf],
        ui_registry: &mut crate::ui_system::UiRegistry,
    ) -> bool {
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

        let mx = self.last_mouse_x;
        let my = self.last_mouse_y;

        // Регистрируем кнопки через UI систему
        ui_registry.register_button(
            crate::ui_system::UiId::WelcomeNewFile,
            &btn_new,
            self,
            mx,
            my,
            scale,
            false,
        );
        ui_registry.register_button(
            crate::ui_system::UiId::WelcomeOpenFile,
            &btn_open,
            self,
            mx,
            my,
            scale,
            false,
        );
        ui_registry.register_button(
            crate::ui_system::UiId::WelcomeIdeMode,
            &btn_ide,
            self,
            mx,
            my,
            scale,
            false,
        );

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
        for (idx, path) in recent_files.iter().enumerate() {
            if y + item_h > content_y + content_h - 60.0 * scale {
                break;
            }

            // Регистрируем кликабельную область для недавнего файла
            ui_registry.register_rect(
                crate::ui_system::UiId::WelcomeRecentFile(idx),
                title_x - 10.0 * scale,
                y,
                content_w - 60.0 * scale,
                item_h,
                mx,
                my,
            );

            let is_hovered = mx >= title_x - 10.0 * scale
                && mx <= title_x + content_w - 70.0 * scale
                && my >= y
                && my < y + item_h;

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
        ui_registry.wants_pointer()
    }

    /// Рисует индикаторы ошибок и предупреждений слева от скроллбара
    pub fn draw_diagnostics_ruler(
        &mut self,
        editor: &crate::editor::Editor,
        lsp_diags: &[crate::lsp::Diagnostic],
        window_height: f32,
    ) {
        if lsp_diags.is_empty() || editor.line_offsets.is_empty() {
            return;
        }

                let s = self.scale_factor;
        let minimap_w = self.minimap_width;

        let tab_bar_h = 44.0 * s;
        let track_y = tab_bar_h;
        let track_h = window_height - tab_bar_h;

        let max_scroll = self.get_max_scroll(editor, track_h);
        let scrollbar_w = if max_scroll > 0.0 { 10.0 * s } else { 0.0 };

        let total_vis_lines = self.phys_to_visual.last().copied().unwrap_or(0) as f32 + 2.0;
        if total_vis_lines < 1.0 {
            return;
        }

        // Полоса слева от скроллбара
        let bar_w = (4.0 * s).max(2.0);
        let bar_x = self.width - minimap_w - scrollbar_w - bar_w;

        // Группируем, чтобы не рисовать черточки друг на друге
        let mut lines_with_errors = std::collections::HashSet::new();
        let mut lines_with_warnings = std::collections::HashSet::new();

        for diag in lsp_diags {
            match diag.severity {
                crate::lsp::DiagSeverity::Error => {
                    lines_with_errors.insert(diag.start_line);
                }
                crate::lsp::DiagSeverity::Warning => {
                    lines_with_warnings.insert(diag.start_line);
                }
                _ => {}
            }
        }

                let indicator_h = (2.0 * s).max(1.0);

        // Сначала рисуем предупреждения
        for &line_num in &lines_with_warnings {
            if !lines_with_errors.contains(&line_num) {
                let vis_line = *self.phys_to_visual.get(line_num as usize).unwrap_or(&0) as f32;
                let y = (track_y + (vis_line / total_vis_lines * track_h)).round();
                self.push_rect(bar_x, y, bar_w, indicator_h, self.theme.diag_warn);
            }
        }

        // Потом ошибки (поверх)
        for &line_num in &lines_with_errors {
            let vis_line = *self.phys_to_visual.get(line_num as usize).unwrap_or(&0) as f32;
            let y = (track_y + (vis_line / total_vis_lines * track_h)).round();
            self.push_rect(bar_x, y, bar_w, indicator_h, self.theme.diag_error);
        }
    }
    /// Рисует весёлый cowsay-экран когда в IDE-режиме нет открытых вкладок.
    /// Сайдбар уже нарисован до вызова, рисуем только зону редактора.
    pub fn draw_empty_ide(&mut self, panel_left_w: f32) {
        let s = self.scale_factor;
        let sb_w = 48.0 * s;
        let editor_x = sb_w + panel_left_w;
        let editor_w = self.width - editor_x;
        let editor_h = self.height;

        // Фон области редактора
        self.push_rect(
            editor_x,
            0.0,
            editor_w,
            editor_h,
            [self.theme.bg[0], self.theme.bg[1], self.theme.bg[2], 1.0],
        );

        let arts: &[&[&str]] = &[
            &[
                " _________________________ ",
                "< Открой файл и погнали!  >",
                " ------------------------- ",
                "        \\   ^__^           ",
                "         \\  (oo)\\_______   ",
                "            (__)\\       )\\/\\",
                "                ||----w |  ",
                "                ||     || ",
            ],
            &[
                " ________________________________ ",
                "< Мяу! Код сам себя не напишет... >",
                " -------------------------------- ",
                "  \\",
                "   \\   /\\_/\\",
                "      ( o.o )",
                "       > ^ <",
            ],
            &[
                " _________________________ ",
                "< Прыгаем в код!           >",
                " ------------------------- ",
                "   \\",
                "    \\   //",
                "       ( ' )",
                "      /  _  \\",
                "     (__)(_)(__)",
            ],
            &[
                " _________________________ ",
                "< Судо, открой файл!       >",
                " ------------------------- ",
                "   \\",
                "    \\    .--.",
                "        |o_o |",
                "        |:_/ |",
                "       //   \\ \\",
                "      (|     | )",
                "     /'\\_   _/`\\",
                "     \\___)=(___/",
            ],
        ];

        if !self.was_empty_ide {
            self.was_empty_ide = true;
            let epoch = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as usize;
            self.empty_ide_art_idx = epoch % arts.len();
        }
        let current_art = arts[self.empty_ide_art_idx];

        let hint_lines = ["Ctrl+O  — открыть файл", "Кликни в дереве файлов слева"];

        // Измеряем ширину для центрирования
        let mono_scale = 0.95_f32;
        let line_h = 22.0 * s;

        let art_total_h = current_art.len() as f32 * line_h;
        let hint_gap = 32.0 * s;
        let hint_total_h = hint_lines.len() as f32 * (line_h + 4.0 * s);
        let total_block_h = art_total_h + hint_gap + hint_total_h;

        let start_y = (editor_h - total_block_h) / 2.0;

        // Рисуем арт
        let art_color = [0.55_f32, 0.50, 0.75, 0.9];
        for (i, line) in current_art.iter().enumerate() {
            let lw = self.measure_ui_width(line, mono_scale);
            let lx = (editor_x + (editor_w - lw) / 2.0).round();
            let ly = (start_y + i as f32 * line_h + line_h * 0.75).round();
            self.draw_string_scaled(line, lx, ly, art_color, mono_scale);
        }

        // Разделитель
        let sep_y = start_y + art_total_h + hint_gap / 2.0;
        let sep_w = 200.0 * s;
        let sep_x = editor_x + (editor_w - sep_w) / 2.0;
        self.push_rect(sep_x, sep_y, sep_w, 1.0, [1.0, 1.0, 1.0, 0.06]);

        // Подсказки
        let hint_y_start = start_y + art_total_h + hint_gap;
        for (i, line) in hint_lines.iter().enumerate() {
            let lw = self.measure_ui_width(line, 0.9);
            let lx = (editor_x + (editor_w - lw) / 2.0).round();
            let ly = (hint_y_start + i as f32 * (line_h + 4.0 * s) + line_h * 0.75).round();
            self.draw_string_scaled(line, lx, ly, [0.45, 0.45, 0.52, 1.0], 0.9);
        }

        self.flush();
    }

    pub fn draw_hover_popup(
        &mut self,
        popup: &crate::app::mouse::HoverPopup,
        selection: Option<(usize, usize)>,
        editor: &crate::editor::Editor,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        render_scroll_y: f32,
        wants_pointer: &mut bool,
    ) -> (f32, f32, f32, f32, f32) {
        let s = self.scale_factor;
        let pad = 12.0 * s;
        let line_h = 22.0 * s;
        let max_text_w = (self.width - 80.0 * s).max(400.0 * s).min(self.width - 40.0 * s);

        let mut lines: Vec<(Vec<(char,[f32; 4], usize)>, crate::lsp::HoverLineKindPublic)> = Vec::new();
        let mut cur_line_w = 0.0;
        let mut cur_line: Vec<(char,[f32; 4], usize)> = Vec::new();
        let mut last_space_idx = None;
        let mut raw_line_no = 0usize;

        for (offset, c) in popup.text.char_indices() {
            if c == '\n' {
                let kind = popup.line_kinds.get(raw_line_no).copied().unwrap_or(crate::lsp::HoverLineKindPublic::Text);
                lines.push((std::mem::take(&mut cur_line), kind));
                cur_line_w = 0.0;
                last_space_idx = None;
                raw_line_no += 1;
                continue;
            }

            let adv = self.char_advance(c);
            if cur_line_w + adv > max_text_w && cur_line_w > 40.0 * s {
                if let Some(space_pos) = last_space_idx {
                    let mut remainder = cur_line.split_off(space_pos);
                    if !remainder.is_empty() && remainder[0].0 == ' ' {
                        remainder.remove(0);
                    }
                    let kind = popup.line_kinds.get(raw_line_no).copied().unwrap_or(crate::lsp::HoverLineKindPublic::Text);
                    lines.push((std::mem::take(&mut cur_line), kind));
                    cur_line = remainder;
                    cur_line_w = cur_line.iter().map(|&(ch, _, _)| self.char_advance(ch)).sum();
                } else {
                    let kind = popup.line_kinds.get(raw_line_no).copied().unwrap_or(crate::lsp::HoverLineKindPublic::Text);
                    lines.push((std::mem::take(&mut cur_line), kind));
                    cur_line_w = 0.0;
                }
                last_space_idx = None;
            }

            let mut color =[0.972, 0.972, 0.949, 1.0];
            for span in &popup.spans {
                if offset >= span.start && offset < span.end {
                    color = span.color;
                }
            }

            cur_line.push((c, color, offset));
            cur_line_w += adv;

            if c == ' ' {
                last_space_idx = Some(cur_line.len() - 1);
            }
        }
        if !cur_line.is_empty() {
            let kind = popup.line_kinds.get(raw_line_no).copied().unwrap_or(crate::lsp::HoverLineKindPublic::Text);
            lines.push((cur_line, kind));
        }

        let mut max_line_w = 0.0;
        for (line, _) in &lines {
            let w: f32 = line.iter().map(|&(ch, _, _)| self.char_advance(ch)).sum();
            if w > max_line_w { max_line_w = w; }
        }

        let box_w = max_line_w + pad * 2.0;
        let total_text_h = lines.len() as f32 * line_h;
        let max_visible_h = (self.height * 0.45).min(total_text_h + pad * 2.0);
        let box_h = max_visible_h;

        let mut bx = popup.anchor_x;
        if bx + box_w > self.width - 20.0 * s {
            bx = self.width - box_w - 20.0 * s;
        }
        if bx < 20.0 * s {
            bx = 20.0 * s;
        }

        let phys_line = editor.line_offsets.partition_point(|&o| o <= popup.byte_offset).saturating_sub(1);
        let vis_line_idx = self.phys_to_visual.get(phys_line).copied().unwrap_or(0) as f32;
        let line_y = self.baseline_offset + (vis_line_idx * self.line_height) - render_scroll_y;

        let mut by = line_y + self.line_height + 4.0 * s;
        if by + box_h > self.height - 20.0 * s {
            by = line_y - box_h - 4.0 * s;
        }
        if by < 0.0 {
            by = 10.0 * s;
        }

        ui_registry.register_blocker(
            crate::ui_system::UiId::BottomPanelBody,
            bx, by, box_w, box_h, mx, my
        );
        let popup_hovered = mx >= bx && mx <= bx + box_w && my >= by && my <= by + box_h;
        if popup_hovered && !*wants_pointer {
            ui_registry.reset_cursor_state();
        }

        let max_scroll = (total_text_h + pad * 2.0 - box_h).max(0.0);
        let scroll_y = popup.scroll.current;

        self.push_rounded_rect(bx.round() - 1.0, by.round() - 1.0, box_w.round() + 2.0, box_h.round() + 2.0, 6.0 * s,[0.4, 0.4, 0.45, 0.6]);
        self.push_rounded_rect(bx.round(), by.round(), box_w.round(), box_h.round(), 6.0 * s,[self.theme.minimap_bg[0], self.theme.minimap_bg[1], self.theme.minimap_bg[2], 1.0]);

        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            let sy = (self.height - (by + box_h)).round() as i32;
            self.gl.scissor(bx.round() as i32, sy, box_w.round() as i32, box_h.round() as i32);
        }

        let mut text_y = by + pad + line_h * 0.75 - scroll_y;
        let selected = selection.filter(|(a, b)| a != b);
        let mut idx = 0usize;
        while idx < lines.len() {
            let (line, line_kind) = &lines[idx];
            if text_y > by - line_h && text_y < by + box_h + line_h {
                let is_separator = line
                    .iter()
                    .all(|(c, _, _)| *c == '-' || c.is_ascii_whitespace())
                    && line.iter().any(|(c, _, _)| *c == '-');
                if is_separator {
                    self.push_rect(
                        (bx + pad).round(),
                        (text_y - line_h * 0.35).round(),
                        (box_w - pad * 2.0).round(),
                        1.0_f32.max(s.round()),
                        [1.0, 1.0, 1.0, 0.10],
                    );
                    text_y += line_h;
                    idx += 1;
                    continue;
                }

                if *line_kind == crate::lsp::HoverLineKindPublic::Code {
                    let mut run_len = 1usize;
                    while idx + run_len < lines.len()
                        && lines[idx + run_len].1 == crate::lsp::HoverLineKindPublic::Code
                    {
                        run_len += 1;
                    }
                    self.push_rounded_rect(
                        (bx + pad - 4.0 * s).round(),
                        (text_y - line_h * 0.82).round(),
                        (box_w - pad * 2.0 + 8.0 * s).round(),
                        (line_h * run_len as f32 - 2.0 * s).round(),
                        4.0 * s,
                        [0.18, 0.20, 0.26, 0.96],
                    );
                }

                let mut draw_x = (bx + pad).round();
                for &(c, color, offset) in line {
                    let adv = self.char_advance(c);
                    if popup
                        .inline_code_ranges
                        .iter()
                        .any(|&(start, end)| offset >= start && offset < end)
                    {
                        self.push_rounded_rect(
                            draw_x - 1.0 * s,
                            (text_y - line_h * 0.74).round(),
                            adv + 2.0 * s,
                            (line_h - 6.0 * s).round(),
                            3.0 * s,
                            [0.26, 0.28, 0.34, 0.98],
                        );
                    }
                    if let Some((sel_start, sel_end)) = selected {
                        if offset >= sel_start && offset < sel_end {
                            self.push_rect(
                                draw_x,
                                (text_y - line_h * 0.75 + 2.0 * s).round(),
                                adv,
                                (line_h - 3.0 * s).round(),
                                self.theme.sel,
                            );
                        }
                    }
                    let mut b =[0; 4];
                    let s_str = c.encode_utf8(&mut b);
                    self.draw_string_mono_scaled(s_str, draw_x, text_y.round(), color, 1.0);
                    draw_x += adv;
                }
            }
            text_y += line_h;
            idx += 1;
        }

        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }

        if max_scroll > 0.0 {
            let track_h = box_h - 16.0 * s;
            let thumb_h = (box_h / (total_text_h + pad * 2.0) * track_h).max(20.0 * s);
            let thumb_y = by + 8.0 * s + (scroll_y / max_scroll) * (track_h - thumb_h);

            self.push_rounded_rect(bx + box_w - 8.0 * s, thumb_y.round(), 4.0 * s, thumb_h, 2.0 * s,[1.0, 1.0, 1.0, 0.2]);

            ui_registry.register_rect(
                crate::ui_system::UiId::HoverPopupScroll,
                bx + box_w - 12.0 * s, by, 12.0 * s, box_h, mx, my
            );
            if ui_registry.hovered() == Some(crate::ui_system::UiId::HoverPopupScroll) {
                ui_registry.reset_cursor_state();
            }
        }

        (bx, by, box_w, box_h, max_scroll)
    }

    pub fn draw_problems_panel(
        &mut self,
        content_x: f32,
        content_y: f32,
        content_w: f32,
        content_h: f32,
        s: f32,
        lsp: Option<&crate::lsp::LspManager>,
        ide_panel: &crate::app::IdePanelState,
        ui_registry: &mut crate::ui_system::UiRegistry,
    ) {
        let pad_x = 12.0 * s;
        let text_scale = 0.92;
        let mx = self.last_mouse_x;
        let my = self.last_mouse_y;

        let mut tab_x = content_x + pad_x;
        let tab_y = content_y + 8.0 * s;
        let tab_h = 24.0 * s;

        let tabs = ["Текущий файл", "Все"];
        for (i, t) in tabs.iter().enumerate() {
            let tw = self.measure_ui_width(t, text_scale) + 16.0 * s;
            let is_active = ide_panel.problems_tab == i;
            let bg = if is_active {
                [1.0, 1.0, 1.0, 0.12]
            } else {
                [1.0, 1.0, 1.0, 0.0]
            };
            let fg = if is_active {
                self.theme.fg
            } else {
                [0.65, 0.65, 0.65, 1.0]
            };

            let is_hovered = ui_registry.register_rect(
                crate::ui_system::UiId::ProblemsTab(i),
                tab_x,
                tab_y,
                tw,
                tab_h,
                mx,
                my,
            );

            if is_active || is_hovered {
                let draw_bg = if is_hovered && !is_active {
                    [1.0, 1.0, 1.0, 0.06]
                } else {
                    bg
                };

                self.flush();
                unsafe {
                    self.gl.enable(glow::SCISSOR_TEST);
                    let sy = (self.height - (tab_y + tab_h)).round() as i32;
                    self.gl.scissor(
                        tab_x.round() as i32,
                        sy,
                        tw.round() as i32,
                        tab_h.round() as i32,
                    );
                }

                self.push_rounded_rect(
                    tab_x.round(),
                    tab_y.round(),
                    tw,
                    tab_h + 4.0 * s,
                    4.0 * s,
                    draw_bg,
                );

                self.flush();
                unsafe {
                    self.gl.disable(glow::SCISSOR_TEST);
                }
            }

            if is_active {
                self.push_rect(
                    tab_x.round(),
                    (tab_y + tab_h).round(),
                    tw,
                    2.0 * s,
                    [0.741, 0.576, 0.976, 1.0],
                );
            }

            self.draw_string_scaled(
                t,
                tab_x + 8.0 * s,
                (tab_y + tab_h / 2.0 + 4.0 * s).round(),
                fg,
                text_scale,
            );
            tab_x += tw + 8.0 * s;
        }

        let header_bottom_y = tab_y + tab_h + 2.0 * s;
        self.push_rect(
            content_x,
            header_bottom_y.round(),
            content_w,
            1.0,
            [1.0, 1.0, 1.0, 0.08],
        );

        self.flush();

        let list_y = header_bottom_y + 6.0 * s;
        let list_h = content_h - (list_y - content_y);

        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            let sy = (self.height - (list_y + list_h)).round() as i32;
            self.gl.scissor(
                content_x.round() as i32,
                sy,
                content_w.round() as i32,
                list_h.round() as i32,
            );
        }

        let scroll_y = ide_panel.problems_scroll.current.round();

        if ide_panel.flat_diags.is_empty() {
            let hint = "Нет ляпов";
            let tw = self.measure_ui_width(hint, text_scale);
            self.draw_string_scaled(
                hint,
                content_x + (content_w - tw) / 2.0,
                (list_y + 32.0 * s).round(),
                [0.45, 0.45, 0.45, 1.0],
                text_scale,
            );
        } else {
            let mut current_y = list_y - scroll_y;
            let item_h = 24.0 * s;

            for (idx, (path, diag_idx)) in ide_panel.flat_diags.iter().enumerate() {
                if *diag_idx == usize::MAX {
                    if current_y + item_h > content_y && current_y < content_y + content_h {
                        ui_registry.register_rect(
                            crate::ui_system::UiId::ProblemFileToggle(idx),
                            content_x,
                            current_y,
                            content_w,
                            item_h,
                            self.last_mouse_x,
                            self.last_mouse_y,
                        );
                        if ui_registry.hovered()
                            == Some(crate::ui_system::UiId::ProblemFileToggle(idx))
                        {
                            self.push_rect(
                                content_x,
                                current_y,
                                content_w,
                                item_h,
                                [1.0, 1.0, 1.0, 0.05],
                            );
                        }

                        let is_collapsed = ide_panel.problems_collapsed.contains(path);
                        let arrow_icon = if is_collapsed {
                            crate::widgets::IconType::Up
                        } else {
                            crate::widgets::IconType::Down
                        };

                        let icon_sz = 22.0 * s;
                        let icon_x = content_x + pad_x - 3.0 * s;
                        let icon_y = current_y + (item_h - icon_sz) / 2.0;
                        self.draw_atlas_icon(
                            arrow_icon,
                            icon_x,
                            icon_y,
                            icon_sz,
                            [0.6, 0.6, 0.6, 1.0],
                        );

                        let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                        let text_x = icon_x + icon_sz + 2.0 * s;
                        let text_y = current_y + item_h * 0.7;

                        let (err_count, warn_count) = if let Some(l) = lsp {
                            let diags = l.get_diagnostics(path);
                            let e = diags
                                .iter()
                                .filter(|d| matches!(d.severity, crate::lsp::DiagSeverity::Error))
                                .count();
                            let w = diags
                                .iter()
                                .filter(|d| matches!(d.severity, crate::lsp::DiagSeverity::Warning))
                                .count();
                            (e, w)
                        } else {
                            (0, 0)
                        };

                        let mut scratch = std::mem::take(&mut self.scratch_buffer);
                        scratch.clear();
                        let _ =
                            std::fmt::Write::write_fmt(&mut scratch, format_args!("{}", file_name));
                        let title_w = self.measure_ui_width(&scratch, text_scale);
                        self.draw_string_scaled(
                            &scratch,
                            text_x.round(),
                            text_y.round(),
                            self.theme.fg,
                            text_scale,
                        );

                        let mut badges_x = text_x.round() + title_w + 16.0 * s;
                        if err_count > 0 {
                            scratch.clear();
                            let _ = std::fmt::Write::write_fmt(
                                &mut scratch,
                                format_args!("{} Ошибок", err_count),
                            );
                            let ew = self.measure_ui_width(&scratch, text_scale);
                            self.draw_string_scaled(
                                &scratch,
                                badges_x,
                                text_y.round(),
                                self.theme.diag_error,
                                text_scale,
                            );
                            badges_x += ew + 12.0 * s;
                        }
                        if warn_count > 0 {
                            scratch.clear();
                            let _ = std::fmt::Write::write_fmt(
                                &mut scratch,
                                format_args!("{} Предупреждений", warn_count),
                            );
                            self.draw_string_scaled(
                                &scratch,
                                badges_x,
                                text_y.round(),
                                self.theme.diag_warn,
                                text_scale,
                            );
                        }
                        self.scratch_buffer = scratch;
                    }
                    current_y += item_h;
                    continue;
                }

                let diag = if let Some(l) = lsp {
                    if let Some(d) = l.get_diagnostics(path).get(*diag_idx) {
                        d
                    } else {
                        continue;
                    }
                } else {
                    continue;
                };
                if current_y + item_h > content_y && current_y < content_y + content_h {
                    let is_all_tab = ide_panel.problems_tab == 1;
                    let indent = if is_all_tab { 24.0 * s } else { 0.0 };
                    let icon_sz = 16.0 * s;
                    let icon_x = content_x + pad_x + indent;
                    let icon_y = current_y + (item_h - icon_sz) / 2.0;

                    ui_registry.register_rect(
                        crate::ui_system::UiId::ProblemJump(idx),
                        content_x,
                        current_y,
                        content_w - 14.0 * s,
                        item_h,
                        self.last_mouse_x,
                        self.last_mouse_y,
                    );
                    if ui_registry.hovered() == Some(crate::ui_system::UiId::ProblemJump(idx)) {
                        self.push_rect(
                            content_x,
                            current_y,
                            content_w - 14.0 * s,
                            item_h,
                            [1.0, 1.0, 1.0, 0.05],
                        );
                    }

                    let (icon, color) = match diag.severity {
                        crate::lsp::DiagSeverity::Error => {
                            (crate::widgets::IconType::Close, self.theme.diag_error)
                        }
                        crate::lsp::DiagSeverity::Warning => {
                            (crate::widgets::IconType::Warning, self.theme.diag_warn)
                        }
                        _ => (crate::widgets::IconType::Problems, [0.5, 0.5, 0.5, 1.0]),
                    };

                    self.draw_atlas_icon(icon, icon_x, icon_y, icon_sz, color);

                    let text_x = icon_x + icon_sz + 8.0 * s;
                    let text_y = current_y + item_h * 0.7;

                    let mut scratch = std::mem::take(&mut self.scratch_buffer);
                    scratch.clear();
                    let _ = std::fmt::Write::write_fmt(
                        &mut scratch,
                        format_args!("Строка {}: ", diag.start_line + 1),
                    );
                    let prefix_w = self.measure_ui_width(&scratch, text_scale).round();
                    self.draw_string_scaled(
                        &scratch,
                        text_x.round(),
                        text_y.round(),
                        self.theme.fg,
                        text_scale,
                    );

                    let mut current_tx = text_x.round() + prefix_w;
                    scratch.clear();
                    for ch in diag.message.lines().next().unwrap_or("").chars() {
                        scratch.push(if ch == '\t' { ' ' } else { ch });
                    }
                    let msg_w = self.measure_ui_width(&scratch, text_scale).round();
                    self.draw_string_scaled(
                        &scratch,
                        current_tx,
                        text_y.round(),
                        self.theme.fg,
                        text_scale,
                    );
                    current_tx += msg_w + self.measure_ui_width(" ", text_scale).round();

                    scratch.clear();
                    match (&diag.source, &diag.code) {
                        (Some(src), Some(_)) => {
                            let _ =
                                std::fmt::Write::write_fmt(&mut scratch, format_args!("({} ", src));
                        }
                        (Some(src), None) => {
                            let _ =
                                std::fmt::Write::write_fmt(&mut scratch, format_args!("({})", src));
                        }
                        (None, Some(_)) => scratch.push('('),
                        (None, None) => scratch.push_str("(LSP)"),
                    };

                    let p_w = self.measure_ui_width(&scratch, text_scale).round();
                    self.draw_string_scaled(
                        &scratch,
                        current_tx,
                        text_y.round(),
                        [0.55, 0.55, 0.6, 1.0],
                        text_scale,
                    );
                    self.scratch_buffer = scratch;

                    if let Some(code) = &diag.code {
                        let sfx_x = current_tx + p_w;
                        let sfx_w = self.measure_ui_width(code, text_scale).round();
                        let link_color = [0.72, 0.52, 1.0, 1.0];
                        let sfx_color = if diag.code_href.is_some() {
                            link_color
                        } else {
                            [link_color[0], link_color[1], link_color[2], 0.85]
                        };

                        self.draw_string_scaled(code, sfx_x, text_y.round(), sfx_color, text_scale);
                        self.draw_string_scaled(
                            ")",
                            sfx_x + sfx_w,
                            text_y.round(),
                            [0.55, 0.55, 0.6, 1.0],
                            text_scale,
                        );

                        if diag.code_href.is_some() {
                            ui_registry.register_rect(
                                crate::ui_system::UiId::ProblemUrl(idx),
                                sfx_x - 1.0,
                                current_y,
                                sfx_w + 2.0,
                                item_h,
                                self.last_mouse_x,
                                self.last_mouse_y,
                            );
                            if ui_registry.hovered()
                                == Some(crate::ui_system::UiId::ProblemUrl(idx))
                            {
                                self.push_rect(
                                    sfx_x,
                                    text_y.round() + 1.0,
                                    sfx_w,
                                    1.0,
                                    [link_color[0], link_color[1], link_color[2], 0.9],
                                );
                            } else {
                                self.push_rect(
                                    sfx_x,
                                    text_y.round() + 1.0,
                                    sfx_w,
                                    1.0,
                                    [link_color[0], link_color[1], link_color[2], 0.55],
                                );
                            }
                        }
                    }
                }
                current_y += item_h;
            }

            let total_h = ide_panel.flat_diags.len() as f32 * item_h;
            let track_h = content_h - 40.0 * s;
            if total_h > track_h {
                let max_scroll = total_h - track_h;
                let scroll_ratio = (scroll_y / max_scroll).clamp(0.0, 1.0);
                let thumb_h = (track_h / total_h * track_h).max(20.0 * s);
                let thumb_y = list_y + scroll_ratio * (track_h - thumb_h);

                self.push_rounded_rect(
                    content_x + content_w - 12.0 * s,
                    thumb_y.round(),
                    6.0 * s,
                    thumb_h,
                    3.0 * s,
                    [0.45, 0.45, 0.55, 0.5],
                );
            }
        }

        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }
    }
}
