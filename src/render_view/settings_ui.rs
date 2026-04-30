use crate::editor::Editor;
use crate::renderer::Renderer;
use glow::HasContext;

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
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

    pub fn draw_settings(
        &mut self,
        anim_progress: f32,
        active_tab: usize,
        faq_editor: &Editor,
        scroll_y: f32,
        ide_workspaces: &[std::path::PathBuf],
        ide_ignore_patterns: &[String],
        settings_ignore_editor: &Editor,
        settings_ignore_focused: bool,
        settings_ignore_scroll_x: &mut f32,
        ide_scroll_y: f32,
        blink_alpha: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
    ) -> u8 {
        if anim_progress <= 0.0 {
            return 0;
        }
        // Элементы основного draw() (сайдбар, панели) не должны влиять
        // на курсор внутри модального окна настроек.
        ui_registry.reset_cursor_state();
        let s = self.scale_factor;
        // Smoothstep для предотвращения "вспышки" (резкого скачка яркости) при отсечении анимации в конце
        let smooth_p = anim_progress * anim_progress * (3.0 - 2.0 * anim_progress);
        let overlay_alpha = (smooth_p * 0.6).clamp(0.0, 1.0);
        self.push_rect(
            0.0,
            0.0,
            self.width,
            self.height,
            [0.0, 0.0, 0.0, overlay_alpha],
        );

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
        self.push_rounded_rect(
            x - 1.0,
            y - 1.0,
            w + 2.0,
            h + 2.0,
            10.0 * s,
            [0.224, 0.231, 0.251, 1.0],
        );
        self.push_rounded_rect_gradient(x, y, w, h, 10.0 * s, top_color, bottom_color);

        // 2. Внутренняя панель
        let pad_top = 35.0 * s;
        let pad_bottom = 30.0 * s;
        let pad_h = 40.0 * s;
        let ix = x + pad_h;
        let iy = y + pad_top;
        let iw = w - pad_h * 2.0;
        let ih = h - pad_top - pad_bottom;

        self.push_rounded_rect(
            ix - 1.0,
            iy - 1.0,
            iw + 2.0,
            ih + 2.0,
            8.0 * s,
            [0.224, 0.231, 0.251, 0.8],
        );
        self.push_rounded_rect(ix, iy, iw, ih, 8.0 * s, [0.15, 0.16, 0.20, 1.0]);

        self.flush();

        let sidebar_w = 200.0 * s;
        self.push_rect(ix + sidebar_w, iy, 1.0, ih, [1.0, 1.0, 1.0, 0.05]);

        let tabs = ["IDE", "Основные", "Редактор", "Внешний вид", "Помощь"];
        let mut tab_y = iy + 20.0 * s;
        for (i, title) in tabs.iter().enumerate() {
            let tab_rect_y = tab_y;
            let tab_rect_h = 36.0 * s;

            let is_hovered = ui_registry.register_rect(
                crate::ui_system::UiId::SettingsTab(i),
                ix + 10.0 * s,
                tab_rect_y,
                sidebar_w - 20.0 * s,
                tab_rect_h,
                self.last_mouse_x,
                self.last_mouse_y,
            );

            if i == active_tab {
                self.push_rounded_rect(
                    ix + 10.0 * s,
                    tab_rect_y,
                    sidebar_w - 20.0 * s,
                    tab_rect_h,
                    6.0 * s,
                    [1.0, 1.0, 1.0, 0.1],
                );
            } else if is_hovered {
                self.push_rounded_rect(
                    ix + 10.0 * s,
                    tab_rect_y,
                    sidebar_w - 20.0 * s,
                    tab_rect_h,
                    6.0 * s,
                    [1.0, 1.0, 1.0, 0.05],
                );
            }

            let color = if i == active_tab {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                [0.7, 0.7, 0.7, 1.0]
            };
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
        self.push_rounded_rect(
            content_title_x - 1.0,
            pill_y - 1.0,
            pill_w + 2.0,
            pill_h + 2.0,
            6.0 * s,
            [0.35, 0.26, 0.48, 1.0],
        );
        self.push_rounded_rect(
            content_title_x,
            pill_y,
            pill_w,
            pill_h,
            6.0 * s,
            [0.26, 0.20, 0.36, 1.0],
        );
        self.draw_string_scaled(
            tab_title,
            content_title_x + 14.0 * s,
            content_y,
            [1.0, 1.0, 1.0, 1.0],
            1.1,
        );
        content_y += if active_tab == 4 { 30.0 * s } else { 46.0 * s };

        if active_tab == 0 {
            // ── Scissor для скролла вкладки IDE ──────────────────────────────
            // Начало scissor = iy + 52.0 * s (ниже пилюли заголовка iy+18..iy+48)
            let ide_content_area_x = ix + sidebar_w;
            let ide_content_area_w = iw - sidebar_w;
            let ide_content_area_h = ih - 52.0 * s;
            self.flush();
            unsafe {
                self.gl.enable(glow::SCISSOR_TEST);
                let scissor_y = self.height - (iy + 52.0 * s + ide_content_area_h);
                self.gl.scissor(
                    ide_content_area_x.round() as i32,
                    scissor_y.round() as i32,
                    ide_content_area_w.round() as i32,
                    ide_content_area_h.round() as i32,
                );
            }

            content_y -= ide_scroll_y.round();

            self.draw_string_scaled(
                "Рабочие области",
                content_x,
                content_y,
                [0.8, 0.8, 0.8, 1.0],
                1.0,
            );
            content_y += 40.0 * s;

            for (ws_idx, path) in ide_workspaces.iter().enumerate() {
                let path_str = path.to_string_lossy();
                let item_w = 460.0 * s;
                let item_h = 36.0 * s;

                self.push_rounded_rect(
                    content_x - 1.0,
                    content_y - 1.0,
                    item_w + 2.0,
                    item_h + 2.0,
                    6.0 * s,
                    [0.306, 0.318, 0.341, 1.0],
                );
                self.push_rounded_rect(
                    content_x,
                    content_y,
                    item_w,
                    item_h,
                    6.0 * s,
                    [0.224, 0.231, 0.251, 1.0],
                );

                self.draw_string_scaled(
                    &path_str,
                    (content_x + 10.0 * s).round(),
                    (content_y + item_h * 0.70).round(),
                    self.theme.fg,
                    0.85,
                );

                let del_btn_x = content_x + item_w - 34.0 * s;
                let del_btn_y = content_y + 3.0 * s;
                let del_btn_size = 30.0 * s;
                ui_registry.register_rect(
                    crate::ui_system::UiId::SettingsIdeRemoveWorkspace(ws_idx),
                    del_btn_x,
                    del_btn_y,
                    del_btn_size,
                    del_btn_size,
                    self.last_mouse_x,
                    self.last_mouse_y,
                );
                let btn_del = crate::widgets::IconButton {
                    x: del_btn_x,
                    y: del_btn_y,
                    size: del_btn_size,
                    icon: Some(crate::widgets::IconType::Discard),
                    is_active: false,
                    icon_size: Some(18.0 * s),
                    active_square_width: None,
                    custom_color: None,
                };
                btn_del.render(self, self.last_mouse_x, self.last_mouse_y, s, false);
                content_y += 46.0 * s;
            }

            let add_btn_y_reg = content_y.round();
            ui_registry.register_rect(
                crate::ui_system::UiId::SettingsIdeAddWorkspace,
                content_x,
                add_btn_y_reg,
                190.0 * s,
                36.0 * s,
                self.last_mouse_x,
                self.last_mouse_y,
            );
            let btn_add = crate::widgets::Button {
                x: content_x,
                y: add_btn_y_reg,
                w: 190.0 * s,
                h: 36.0 * s,
                text: "Добавить папку".to_string(),
                icon: Some(crate::widgets::IconType::Plus),
                text_scale: 1.0,
                icon_size: 20.0 * s,
            };
            btn_add.render(self, self.last_mouse_x, self.last_mouse_y, s, false);
            content_y += 56.0 * s;
            // ── Разделитель ───────────────────────────────────────────────
            self.push_rect(content_x, content_y, 460.0 * s, 1.0, [1.0, 1.0, 1.0, 0.07]);
            content_y += 20.0 * s;

            // ── Заголовок секции игноров ──────────────────────────────────
            self.draw_string_scaled(
                "Игнорируемые файлы и папки",
                content_x,
                content_y.round(),
                [0.8, 0.8, 0.8, 1.0],
                1.0,
            );
            content_y += 28.0 * s;

            // Пояснение
            self.draw_string_scaled(
                "Эти файлы и папки не будут показаны в дереве проекта.",
                content_x,
                content_y.round(),
                [0.45, 0.47, 0.55, 1.0],
                0.85,
            );
            content_y += 22.0 * s;
            self.draw_string_scaled(
                "Примеры: *.log  temp/  .DS_Store  *.min.js  build  dist",
                content_x,
                content_y.round(),
                [0.35, 0.37, 0.44, 1.0],
                0.82,
            );
            content_y += 20.0 * s;

            // ── Поле ввода + кнопка «Добавить» ───────────────────────────
            let input_w = 330.0 * s;
            let input_h = 34.0 * s;
            let text_scale_input = 0.95f32; // Округленный скейл для ровного бейзлайна

            let input_hovered = ui_registry.register_text_input(
                crate::ui_system::UiId::SettingsIdeIgnoreInput,
                content_x,
                content_y,
                input_w,
                input_h,
                self.last_mouse_x,
                self.last_mouse_y,
            );

            let border_col = if settings_ignore_focused {
                [0.55, 0.35, 0.80, 1.0]
            } else if input_hovered {
                [0.40, 0.28, 0.60, 1.0]
            } else {
                [0.28, 0.29, 0.35, 1.0]
            };
            self.push_rounded_rect(
                content_x - 1.0,
                content_y - 1.0,
                input_w + 2.0,
                input_h + 2.0,
                6.0 * s,
                border_col,
            );
            self.push_rounded_rect(
                content_x,
                content_y,
                input_w,
                input_h,
                6.0 * s,
                [0.11, 0.12, 0.16, 1.0],
            );

            let text_y_mid = (content_y + input_h * 0.70).round();
            let start_x = (content_x + 8.0 * s).round();
            let full_text = settings_ignore_editor.get_full_text();

            if full_text.is_empty() {
                self.draw_string_scaled(
                    "Паттерн или имя файла...",
                    start_x,
                    text_y_mid,
                    [0.30, 0.32, 0.40, 1.0],
                    text_scale_input,
                );
                if settings_ignore_focused && blink_alpha > 0.5 {
                    self.push_rect(
                        start_x,
                        (content_y + 6.0 * s).round(),
                        (1.5 * s).max(1.0),
                        input_h - 12.0 * s,
                        [0.75, 0.45, 1.0, 1.0],
                    );
                }
            } else {
                let mut cursor_total_x = 0.0;
                let mut total_text_width = 0.0;
                for (byte_idx, c) in full_text.char_indices() {
                    let adv =
                        self.get_ui_glyph(c).map(|g| g.advance).unwrap_or(10.0) * text_scale_input;
                    if byte_idx < settings_ignore_editor.cursor {
                        cursor_total_x += adv;
                    }
                    total_text_width += adv;
                }

                let max_text_w_exact = input_w - 16.0 * s;
                if cursor_total_x - *settings_ignore_scroll_x > max_text_w_exact {
                    *settings_ignore_scroll_x = cursor_total_x - max_text_w_exact;
                }
                if cursor_total_x - *settings_ignore_scroll_x < 0.0 {
                    *settings_ignore_scroll_x = cursor_total_x;
                }
                *settings_ignore_scroll_x = (*settings_ignore_scroll_x)
                    .min((total_text_width - max_text_w_exact).max(0.0))
                    .max(0.0);

                self.flush();
                unsafe {
                    self.gl.enable(glow::SCISSOR_TEST);
                    let scissor_y = self.height - (content_y + input_h);
                    self.gl.scissor(
                        content_x.round() as i32,
                        scissor_y.round() as i32,
                        input_w.round() as i32,
                        input_h.round() as i32,
                    );
                }

                let sel_start = settings_ignore_editor
                    .selection_anchor
                    .unwrap_or(settings_ignore_editor.cursor)
                    .min(settings_ignore_editor.cursor);
                let sel_end = settings_ignore_editor
                    .selection_anchor
                    .unwrap_or(settings_ignore_editor.cursor)
                    .max(settings_ignore_editor.cursor);

                let mut current_x = (start_x - *settings_ignore_scroll_x).round();
                let mut byte_idx = 0;
                let mut cursor_draw_x = current_x;

                for c in full_text.chars() {
                    if byte_idx == settings_ignore_editor.cursor {
                        cursor_draw_x = current_x;
                    }
                    let adv =
                        self.get_ui_glyph(c).map(|g| g.advance).unwrap_or(10.0) * text_scale_input;

                    if byte_idx >= sel_start && byte_idx < sel_end {
                        self.push_rect(
                            current_x,
                            (content_y + 4.0 * s).round(),
                            adv.ceil() + 1.0,
                            input_h - 8.0 * s,
                            [0.55, 0.35, 0.80, 0.50],
                        );
                    }

                    if let Some(g) = self.get_ui_glyph(c) {
                        self.push_quad(
                            current_x + g.offset_x * text_scale_input,
                            text_y_mid - g.offset_y * text_scale_input,
                            g.width * text_scale_input,
                            g.height * text_scale_input,
                            g.u,
                            g.v,
                            g.uw,
                            g.vh,
                            [0.90, 0.88, 0.95, 1.0],
                            g.is_emoji,
                        );
                    }
                    current_x += adv;
                    byte_idx += c.len_utf8();
                }
                if byte_idx == settings_ignore_editor.cursor {
                    cursor_draw_x = current_x;
                }

                if settings_ignore_focused && sel_start == sel_end && blink_alpha > 0.5 {
                    self.push_rect(
                        cursor_draw_x,
                        (content_y + 6.0 * s).round(),
                        (1.5 * s).max(1.0),
                        input_h - 12.0 * s,
                        [0.75, 0.45, 1.0, 1.0],
                    );
                }

                self.flush();
                unsafe {
                    self.gl.disable(glow::SCISSOR_TEST);
                }
            }

            // Кнопка «Добавить» — неактивна если поле пустое или только пробелы
            let trimmed_input = full_text.trim();
            let btn_add_x = content_x + input_w + 10.0 * s;
            let btn_add_y = content_y;
            let btn_add_w = 110.0 * s;

            // Регистрируем поле ввода и кнопку добавления через ui_registry
            ui_registry.register_rect(
                crate::ui_system::UiId::SettingsIdeIgnoreInput,
                content_x,
                content_y,
                input_w,
                input_h,
                self.last_mouse_x,
                self.last_mouse_y,
            );
            if !trimmed_input.is_empty() {
                ui_registry.register_rect(
                    crate::ui_system::UiId::SettingsIdeAddIgnore,
                    btn_add_x,
                    btn_add_y,
                    btn_add_w,
                    input_h,
                    self.last_mouse_x,
                    self.last_mouse_y,
                );
            }

            if trimmed_input.is_empty() {
                // Строго копируем математику округления из widgets.rs (Button::render)
                let bx = btn_add_x.round();
                let by = btn_add_y.round();
                let bw = btn_add_w.round();
                let bh = input_h.round();

                self.push_rounded_rect(
                    bx - 1.0,
                    by - 1.0,
                    bw + 2.0,
                    bh + 2.0,
                    6.0 * s,
                    [0.20, 0.21, 0.26, 1.0],
                );
                self.push_rounded_rect(bx, by, bw, bh, 6.0 * s, [0.15, 0.16, 0.20, 1.0]);

                let icon_sz = 15.0 * s;
                let text_scale = 0.88;
                let text_w = self.measure_ui_width("Добавить", text_scale);
                let content_w = text_w + icon_sz + 8.0 * s;

                let mut content_x = bx + (bw - content_w) / 2.0;
                let icon_y = by + (bh - icon_sz) / 2.0;
                let text_y = by + bh / 2.0 + 5.0 * s;

                self.draw_atlas_icon(
                    crate::widgets::IconType::Plus,
                    content_x,
                    icon_y,
                    icon_sz,
                    [0.35, 0.36, 0.42, 1.0],
                );
                content_x += icon_sz + 8.0 * s;

                self.draw_string_scaled(
                    "Добавить",
                    content_x,
                    text_y,
                    [0.35, 0.36, 0.42, 1.0],
                    text_scale,
                );
            } else {
                let btn_ignore_add = crate::widgets::Button {
                    x: btn_add_x,
                    y: btn_add_y,
                    w: btn_add_w,
                    h: input_h,
                    text: "Добавить".to_string(),
                    icon: Some(crate::widgets::IconType::Plus),
                    text_scale: 0.88,
                    icon_size: 15.0 * s,
                };
                btn_ignore_add.render(self, self.last_mouse_x, self.last_mouse_y, s, false);
            }
            content_y += input_h + 16.0 * s;

            // ── Чипы пользовательских паттернов ──────────────────────────
            let chip_h = 28.0 * s;
            let chip_r = chip_h / 2.0;
            let pad_x = 12.0 * s;
            let chip_gap_x = 8.0 * s;
            let chip_gap_y = 8.0 * s;
            let max_row_w = 460.0 * s;
            let mut chip_x = content_x;

            for (chip_idx, pattern) in ide_ignore_patterns.iter().enumerate() {
                let text_w = self.measure_ui_width(pattern, 0.88);
                let close_area = 22.0 * s;
                let chip_w = text_w + pad_x * 2.0 + close_area;

                if chip_x + chip_w > content_x + max_row_w && chip_x > content_x {
                    chip_x = content_x;
                    content_y += chip_h + chip_gap_y;
                }

                let chip_hov = self.last_mouse_x >= chip_x
                    && self.last_mouse_x <= chip_x + chip_w
                    && self.last_mouse_y >= content_y
                    && self.last_mouse_y <= content_y + chip_h;

                let close_hov = ui_registry.register_rect(
                    crate::ui_system::UiId::SettingsIdeRemoveIgnore(chip_idx),
                    chip_x + chip_w - close_area - 2.0 * s,
                    content_y,
                    close_area + 2.0 * s,
                    chip_h,
                    self.last_mouse_x,
                    self.last_mouse_y,
                );

                let bg = if chip_hov {
                    [0.30, 0.18, 0.44, 1.0]
                } else {
                    [0.20, 0.13, 0.30, 1.0]
                };
                let border = if chip_hov {
                    [0.58, 0.34, 0.82, 1.0]
                } else {
                    [0.35, 0.22, 0.52, 1.0]
                };

                self.push_rounded_rect(
                    chip_x - 1.0,
                    content_y - 1.0,
                    chip_w + 2.0,
                    chip_h + 2.0,
                    chip_r + 1.0,
                    border,
                );
                self.push_rounded_rect(chip_x, content_y, chip_w, chip_h, chip_r, bg);

                self.draw_string_scaled(
                    pattern,
                    chip_x + pad_x,
                    (content_y + chip_h * 0.70).round(),
                    [0.82, 0.68, 1.0, 1.0],
                    0.88,
                );

                let cross_color = if close_hov {
                    [1.0, 0.38, 0.58, 1.0]
                } else {
                    [0.50, 0.40, 0.65, 1.0]
                };
                self.draw_string_scaled(
                    "×",
                    chip_x + chip_w - close_area + 1.0 * s,
                    (content_y + chip_h * 0.70).round(),
                    cross_color,
                    0.95,
                );

                chip_x += chip_w + chip_gap_x;
            }

            if ide_ignore_patterns.is_empty() {
                self.draw_string_scaled(
                    "Нет пользовательских правил",
                    content_x,
                    (content_y + chip_h * 0.70).round(),
                    [0.28, 0.30, 0.36, 1.0],
                    0.88,
                );
            }

            self.flush();
            unsafe {
                self.gl.disable(glow::SCISSOR_TEST);
            }

            // ── Скроллбар для вкладки IDE ─────────────────────────────────
            let ide_total_h = {
                let workspace_h = ide_workspaces.len() as f32 * 46.0 * s + 126.0 * s;
                let ignore_h = {
                    let chip_rows = if ide_ignore_patterns.is_empty() {
                        1
                    } else {
                        let mut rows = 1usize;
                        let mut cx2 = 0.0f32;
                        for p in ide_ignore_patterns.iter() {
                            let tw = self.measure_ui_width(p, 0.88);
                            let cw2 = tw + pad_x * 2.0 + 22.0 * s;
                            if cx2 + cw2 > max_row_w && cx2 > 0.0 {
                                rows += 1;
                                cx2 = 0.0;
                            }
                            cx2 += cw2 + chip_gap_x;
                        }
                        rows
                    };
                    // Убрана плашка «Скрыты всегда» (-dlabel_h - 18.0 * s)
                    160.0 * s + chip_rows as f32 * (chip_h + chip_gap_y)
                };
                workspace_h + ignore_h
            };
            let max_scroll = (ide_total_h - ide_content_area_h).max(0.0);
            if max_scroll > 0.0 {
                let ratio = (ide_scroll_y / max_scroll).clamp(0.0, 1.0);
                let track_h = ide_content_area_h;
                let thumb_h = (ide_content_area_h / ide_total_h * track_h).max(40.0 * s);
                let thumb_y = (iy + 52.0 * s + ratio * (track_h - thumb_h)).round();
                let sb_x = (ix + iw - 14.0 * s).round();
                self.push_rounded_rect(
                    sb_x,
                    thumb_y,
                    6.0 * s,
                    thumb_h,
                    3.0 * s,
                    [0.7, 0.33, 0.54, 1.0],
                );
            }
        } else if active_tab == 1 {
            self.draw_string_scaled(
                "Скоро здесь появятся настройки...",
                content_x,
                content_y,
                [0.6, 0.6, 0.6, 1.0],
                1.0,
            );
        } else if active_tab == 2 {
            self.draw_string_scaled(
                "Размер шрифта: 14px",
                content_x,
                content_y,
                [0.8, 0.8, 0.8, 1.0],
                1.0,
            );
            content_y += 30.0 * s;
            self.draw_string_scaled(
                "Межстрочный интервал: 1.5",
                content_x,
                content_y,
                [0.8, 0.8, 0.8, 1.0],
                1.0,
            );
        } else if active_tab == 3 {
            self.draw_string_scaled(
                "Тема: Dracula (По умолчанию)",
                content_x,
                content_y,
                [0.8, 0.8, 0.8, 1.0],
                1.0,
            );
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
                        self.draw_string_scaled(
                            header_text,
                            main_header_x + 12.0 * s,
                            text_y,
                            [1.0, 1.0, 1.0, 1.0],
                            1.05,
                        );
                        main_header_drawn = true;
                    } else {
                        let sep_y = text_y + 10.0 * s;
                        let sep_x = main_header_x;
                        let sep_w = (cw - 10.0 * s).max(0.0);
                        self.draw_string_scaled(
                            header_text,
                            start_x,
                            text_y,
                            [0.875, 0.882, 0.902, 1.0],
                            1.05,
                        );
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
                    self.draw_string_scaled(
                        description,
                        start_x + left_col_w,
                        text_y,
                        desc_color,
                        1.0,
                    );

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
        if ui_registry.wants_text() {
            2
        } else if ui_registry.wants_pointer() {
            1
        } else {
            0
        }
    }
}
