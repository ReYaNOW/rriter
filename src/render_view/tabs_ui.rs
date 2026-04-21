use crate::editor::Editor;
use crate::renderer::Renderer;
use glow::HasContext;

impl Renderer {
    pub fn draw_tab_bar(
        &mut self,
        tabs: &[crate::app::EditorTab],
        active_tab: usize,
        editor: &Editor,
        editor_title: &str,
        _editor_path: Option<&std::path::PathBuf>,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        mx: f32,
        my: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
        tab_scroll_x: f32,
    ) {
        let tab_bar_bg = self.theme.minimap_bg;
        self.push_rect(x, y, w, h, tab_bar_bg);

        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            let sy = (self.height - (y + h)).round() as i32;
            self.gl
                .scissor(x.round() as i32, sy, w.round() as i32, h.round() as i32);
        }

        let mut current_x = x - tab_scroll_x;
        let tab_pad = 16.0 * s;

        for (i, tab) in tabs.iter().enumerate() {
            let is_active = i == active_tab;
            let title = if is_active {
                if editor_title.is_empty() {
                    "Безымянный"
                } else {
                    editor_title
                }
            } else {
                if tab.base_title.is_empty() {
                    "Безымянный"
                } else {
                    &tab.base_title
                }
            };

            let title_w = self.measure_ui_width(title, 1.0);
            let icon_size_tab = 20.0 * s;
            let tab_w = tab_pad * 2.0 + icon_size_tab + 8.0 * s + title_w + 30.0 * s;

            let is_hovered = mx >= current_x.max(x)
                && mx <= (current_x + tab_w).min(x + w)
                && my >= y
                && my <= y + h;

            let bg_color = if is_active {
                [self.theme.bg[0], self.theme.bg[1], self.theme.bg[2], 1.0]
            } else if is_hovered {
                [
                    self.theme.bg[0] + 0.02,
                    self.theme.bg[1] + 0.02,
                    self.theme.bg[2] + 0.02,
                    1.0,
                ]
            } else {
                [0.0, 0.0, 0.0, 0.0]
            };

            if bg_color[3] > 0.0 {
                self.push_rect(current_x, y, tab_w, h, bg_color);
            }

            if is_active {
                self.push_rect(
                    current_x,
                    y + h - 2.0 * s,
                    tab_w,
                    2.0 * s,
                    [0.60, 0.35, 0.85, 1.0],
                );
            }

            let is_dirty = if is_active {
                editor.is_dirty()
            } else {
                tab.editor.is_dirty()
            };

            let icon_key = if is_active {
                crate::app::file_icons::file_icon_key(&title.to_lowercase())
            } else {
                tab.icon_key
            };

            let icon_y = (y + (h - icon_size_tab) / 2.0 - 1.5 * s).round();
            self.draw_file_icon(icon_key, false, current_x + tab_pad, icon_y, icon_size_tab);

            let text_color = if is_active {
                self.theme.fg
            } else {
                self.theme.line_num
            };
            let text_x = current_x + tab_pad + icon_size_tab + 8.0 * s;
            self.draw_string_scaled(title, text_x, y + h / 2.0 + 5.0 * s, text_color, 1.0);

            let tab_right = current_x + tab_w;
            if tab_right > x && current_x < x + w {
                ui_registry.register_rect(
                    crate::ui_system::UiId::EditorTab(i),
                    current_x.max(x),
                    y,
                    (tab_right.min(x + w) - current_x.max(x)).max(0.0),
                    h,
                    mx,
                    my,
                );
            }

            {
                let close_size = 20.0 * s;
                let close_x = current_x + tab_w - tab_pad - close_size;
                let close_y = (y + (h - close_size) / 2.0 - 1.5 * s).round();

                let close_rect_x = close_x - 4.0 * s;
                let close_rect_y = close_y - 4.0 * s;
                let close_rect_w = close_size + 8.0 * s;
                let close_rect_h = close_size + 8.0 * s;
                let close_rect_right = close_rect_x + close_rect_w;

                let close_hovered = mx >= close_rect_x.max(x)
                    && mx <= close_rect_right.min(x + w)
                    && my >= close_rect_y
                    && my <= close_rect_y + close_rect_h;

                let show_close = is_active || is_hovered;
                if show_close {
                    if is_dirty && !close_hovered {
                        // Точка вместо крестика (VS Code стиль)
                        self.draw_string_scaled(
                            "●",
                            close_x + close_size / 2.0 - 4.0 * s,
                            close_y + close_size / 2.0 + 4.0 * s,
                            [0.9, 0.9, 0.9, 1.0],
                            0.8,
                        );
                    } else {
                        if close_hovered {
                            self.push_rounded_rect(
                                close_rect_x,
                                close_rect_y,
                                close_rect_w,
                                close_rect_h,
                                4.0 * s,
                                [1.0, 1.0, 1.0, 0.1],
                            );
                        }
                        let icon_col = if close_hovered {
                            [1.0, 1.0, 1.0, 1.0]
                        } else {
                            [1.0, 1.0, 1.0, 0.8]
                        };
                        self.draw_atlas_icon(
                            crate::widgets::IconType::Close,
                            close_x,
                            close_y,
                            close_size,
                            icon_col,
                        );
                    }
                }

                if close_rect_right > x && close_rect_x < x + w {
                    ui_registry.register_rect(
                        crate::ui_system::UiId::EditorTabClose(i),
                        close_rect_x.max(x),
                        close_rect_y,
                        (close_rect_right.min(x + w) - close_rect_x.max(x)).max(0.0),
                        close_rect_h,
                        mx,
                        my,
                    );
                }
            }

            current_x += tab_w;
        }

        self.max_tab_scroll_x = (current_x + tab_scroll_x - x - w).max(0.0);

        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }

        let base_shadow_alpha = 0.4;
        let transparent = [0.0, 0.0, 0.0, 0.0];
        let fade_w = 40.0 * s;

        let left_alpha = (tab_scroll_x / fade_w).clamp(0.0, 1.0) * base_shadow_alpha;
        if left_alpha > 0.001 {
            let shadow_color = [0.0, 0.0, 0.0, left_alpha];
            self.push_horizontal_gradient(x, y, fade_w, h, shadow_color, transparent);
        }

        if self.max_tab_scroll_x > 0.0 {
            let right_alpha = ((self.max_tab_scroll_x - tab_scroll_x) / fade_w).clamp(0.0, 1.0)
                * base_shadow_alpha;
            if right_alpha > 0.001 {
                let shadow_color = [0.0, 0.0, 0.0, right_alpha];
                self.push_horizontal_gradient(
                    x + w - fade_w,
                    y,
                    fade_w,
                    h,
                    transparent,
                    shadow_color,
                );
            }
        }
    }
}
