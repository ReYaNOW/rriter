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
        let max_scroll = self.get_max_scroll(editor, window_height);
        let scrollbar_w = if max_scroll > 0.0 { 10.0 * s } else { 0.0 };

        let total_lines = editor.line_offsets.len() as f32;
        if total_lines < 1.0 {
            return;
        }

        let track_h = window_height;

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
                let y = (line_num as f32 / total_lines * track_h).round();
                self.push_rect(bar_x, y, bar_w, indicator_h, self.theme.diag_warn);
            }
        }

        // Потом ошибки (поверх)
        for &line_num in &lines_with_errors {
            let y = (line_num as f32 / total_lines * track_h).round();
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

        let cow_lines = [
            " _________________________ ",
            "< Открой файл и погнали!  >",
            " ------------------------- ",
            "        \\   ^__^           ",
            "         \\  (oo)\\_______   ",
            "            (__)\\       )\\/\\",
            "                ||----w |  ",
            "                ||     || ",
        ];

        let hint_lines = [
            "Ctrl+O  — открыть файл",
            "Ctrl+N  — новый файл (Ctrl+T)",
            "Кликни в дереве файлов слева",
        ];

        // Измеряем ширину для центрирования
        let mono_scale = 0.95_f32;
        let line_h = 22.0 * s;

        let cow_total_h = cow_lines.len() as f32 * line_h;
        let hint_gap = 32.0 * s;
        let hint_total_h = hint_lines.len() as f32 * (line_h + 4.0 * s);
        let total_block_h = cow_total_h + hint_gap + hint_total_h;

        let start_y = (editor_h - total_block_h) / 2.0;

        // Рисуем корову
        let cow_color = [0.55_f32, 0.50, 0.75, 0.9];
        for (i, line) in cow_lines.iter().enumerate() {
            let lw = self.measure_ui_width(line, mono_scale);
            let lx = (editor_x + (editor_w - lw) / 2.0).round();
            let ly = (start_y + i as f32 * line_h + line_h * 0.75).round();
            self.draw_string_scaled(line, lx, ly, cow_color, mono_scale);
        }

        // Разделитель
        let sep_y = start_y + cow_total_h + hint_gap / 2.0;
        let sep_w = 200.0 * s;
        let sep_x = editor_x + (editor_w - sep_w) / 2.0;
        self.push_rect(sep_x, sep_y, sep_w, 1.0, [1.0, 1.0, 1.0, 0.06]);

        // Подсказки
        let hint_y_start = start_y + cow_total_h + hint_gap;
        for (i, line) in hint_lines.iter().enumerate() {
            let lw = self.measure_ui_width(line, 0.9);
            let lx = (editor_x + (editor_w - lw) / 2.0).round();
            let ly = (hint_y_start + i as f32 * (line_h + 4.0 * s) + line_h * 0.75).round();
            self.draw_string_scaled(line, lx, ly, [0.45, 0.45, 0.52, 1.0], 0.9);
        }

        self.flush();
    }
}
