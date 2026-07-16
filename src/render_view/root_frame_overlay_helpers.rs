#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    fn draw_ide_modal_overlays(
        &mut self,
        s: f32,
        ide_panel: &crate::app::IdePanelState,
        editor: &crate::editor::Editor,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        blink_alpha: f32,
    ) -> bool {
        let mut drew = false;
        let api = &ide_panel.api;
        if api.mock_python_runtime_open {
            self.draw_api_mock_python_overlay(s, api, ui_registry, mx, my, blink_alpha);
            drew = true;
        }
        if api.mock_guide_open {
            self.draw_api_mock_guide_overlay(0.0, 0.0, self.width, self.height, s, api, ui_registry, mx, my);
            drew = true;
        }
        if api.mock_server_detail_open {
            self.draw_api_mock_server_detail_overlay(
                0.0,
                0.0,
                self.width,
                self.height,
                s,
                api,
                ui_registry,
                mx,
                my,
            );
            drew = true;
        }
        if ide_panel.project_search.help_open {
            self.draw_project_search_help_overlay(ide_panel, ui_registry, mx, my, s);
            drew = true;
        }
        if self.draw_database_overlays(s, ide_panel, editor, ui_registry, mx, my, blink_alpha) {
            drew = true;
        }
        drew
    }

    fn draw_empty_ide_frame(
        &mut self,
        ide_panel: &crate::app::IdePanelState,
        editor: &crate::editor::Editor,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        blink_alpha: f32,
        panel_left_w: f32,
        s: f32,
    ) -> (bool, Vec<(usize, usize)>) {
        self.draw_empty_ide(panel_left_w);
        if self.draw_ide_modal_overlays(s, ide_panel, editor, ui_registry, mx, my, blink_alpha) {
            self.flush();
            return (ui_registry.wants_pointer(), Vec::new());
        }
        if should_draw_empty_ide_file_tree_overlay(
            true,
            true,
            crate::app::file_tree::file_tree_overlay_active_for_panel(ide_panel),
        ) {
            let wants_pointer =
                self.draw_file_tree_overlays(ide_panel, ui_registry, mx, my, blink_alpha);
            self.flush();
            return (wants_pointer | ui_registry.wants_pointer(), Vec::new());
        }
        (false, Vec::new())
    }

    fn draw_ide_welcome_bounce(
        &mut self,
        gutter_x: f32,
        panel_bottom_h: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        s: f32,
    ) {
        let anim_w = self.width - gutter_x;
        let anim_h = self.height - panel_bottom_h;
        self.push_rect(gutter_x, 0.0, anim_w, anim_h, [0.173, 0.180, 0.224, 1.0]);

        ui_registry.register_blocker(
            crate::ui_system::UiId::BottomPanelBody,
            gutter_x,
            0.0,
            anim_w,
            anim_h,
            mx,
            my,
        );

        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f32();
        let text = "RRiter";
        let sub_text = "Нет открытых файлов";
        let scale_t = 3.0;
        let scale_sub = 1.2;
        let text_w = self
            .measure_ui_width(text, scale_t)
            .max(self.measure_ui_width(sub_text, scale_sub));
        let text_h = 70.0 * s;

        let eff_w = (anim_w - text_w).max(1.0);
        let eff_h = (anim_h - text_h).max(1.0);
        let px = (t * 100.0 * s) % (eff_w * 2.0);
        let rx = if px < eff_w { px } else { eff_w * 2.0 - px };
        let py = (t * 75.0 * s) % (eff_h * 2.0);
        let ry = if py < eff_h { py } else { eff_h * 2.0 - py };

        let r = (t * 2.0).sin() * 0.2 + 0.6;
        let g = (t * 3.0).sin() * 0.2 + 0.6;
        let b = (t * 5.0).sin() * 0.2 + 0.8;

        let draw_x = gutter_x + rx;
        let draw_y = ry + 40.0 * s;
        self.draw_string_scaled(text, draw_x, draw_y, [r, g, b, 1.0], scale_t);
        self.draw_string_scaled(
            sub_text,
            draw_x,
            draw_y + 30.0 * s,
            [0.5, 0.5, 0.6, 1.0],
            scale_sub,
        );
        self.flush();
    }

    fn draw_editor_horizontal_scrollbar(
        &mut self,
        render_scroll_x: f32,
        scrollbar_x: f32,
        editor_bottom_h: f32,
        is_ide_mode: bool,
        real_height: f32,
        s: f32,
    ) {
        if self.max_scroll_x <= 0.0 {
            return;
        }
        let track_w = scrollbar_x - self.left_padding;
        let track_h_bg = 14.0 * s;
        let status_bar_h = if is_ide_mode {
            ide_status_bar_height(s)
        } else {
            0.0
        };
        let track_y_bg = real_height - editor_bottom_h - status_bar_h - track_h_bg;

        self.push_rect(
            self.left_padding,
            track_y_bg,
            track_w,
            track_h_bg,
            [self.theme.bg[0], self.theme.bg[1], self.theme.bg[2], 1.0],
        );

        let thumb_w = (track_w / (self.max_scroll_x + track_w).max(1.0) * track_w).max(40.0 * s);
        let scroll_ratio_x = (render_scroll_x / self.max_scroll_x).clamp(0.0, 1.0);
        let thumb_x = self.left_padding + scroll_ratio_x * (track_w - thumb_w);

        let thumb_h = 6.0 * s;
        let thumb_y = track_y_bg + (track_h_bg - thumb_h) / 2.0;

        self.push_rounded_rect(
            thumb_x,
            thumb_y,
            thumb_w,
            thumb_h,
            3.0 * s,
            [0.7, 0.33, 0.54, 1.0],
        );
    }

    fn register_root_resize_blockers(
        &mut self,
        ide_panel: &crate::app::IdePanelState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        s: f32,
        mx: f32,
        my: f32,
        panel_left_w: f32,
        panel_bottom_h: f32,
        is_ui_disabled: bool,
        modal_overlay_open: bool,
        real_height: f32,
    ) {
        if panel_left_w > 0.0 && !is_ui_disabled && !modal_overlay_open {
            let resize_x = 48.0 * s + panel_left_w;
            let resize_hit = 3.0 * s;
            let resize_h = if panel_bottom_h > 0.0 && ide_panel.bottom_panel_blocks_editor_hover()
            {
                ide_bottom_panel_y(self.height, panel_bottom_h, s)
            } else {
                real_height
            };
            ui_registry.register_blocker(
                crate::ui_system::UiId::ResizeLeft,
                resize_x - resize_hit,
                0.0,
                resize_hit * 2.0,
                resize_h,
                mx,
                my,
            );
        }
        if panel_bottom_h > 0.0 && !is_ui_disabled && !modal_overlay_open {
            let panel_y = ide_bottom_panel_y(self.height, panel_bottom_h, s);
            ui_registry.register_blocker(
                crate::ui_system::UiId::ResizeBottom,
                48.0 * s,
                panel_y - 8.0 * s,
                self.width - 48.0 * s,
                16.0 * s,
                mx,
                my,
            );
        }
    }

    fn draw_readonly_notice(&mut self, tab_bar_h: f32, s: f32) {
        let text = "Файл открыт в режиме только чтение";
        let text_w = self.measure_ui_width(text, 1.0);
        let pad_x = 16.0 * s;
        let w = text_w + pad_x * 2.0;
        let h = 32.0 * s;
        let x = ((self.width - w) * 0.5).max(8.0 * s).round();
        let y = (tab_bar_h + 10.0 * s).round();
        self.push_rounded_rect(x, y, w, h, 6.0 * s, [0.10, 0.11, 0.14, 0.94]);
        self.push_rounded_rect_border(
            x,
            y,
            w,
            h,
            6.0 * s,
            (1.0 * s).max(1.0),
            [1.0, 1.0, 1.0, 0.16],
            [0.10, 0.11, 0.14, 0.94],
        );
        self.draw_string_scaled(text, x + pad_x, y + h * 0.5 + 5.0 * s, self.theme.fg, 1.0);
    }

    fn register_editor_horizontal_scrollbar(
        &mut self,
        ui_registry: &mut crate::ui_system::UiRegistry,
        scrollbar_x: f32,
        editor_bottom_h: f32,
        is_ide_mode: bool,
        real_height: f32,
        s: f32,
    ) {
        if self.max_scroll_x <= 0.0 {
            return;
        }
        let track_w = scrollbar_x - self.left_padding;
        let status_bar_h = if is_ide_mode {
            ide_status_bar_height(s)
        } else {
            0.0
        };
        ui_registry.register_rect(
            crate::ui_system::UiId::EditorScrollbarX,
            self.left_padding,
            real_height - editor_bottom_h - status_bar_h - 14.0 * s,
            track_w,
            14.0 * s,
            self.last_mouse_x,
            self.last_mouse_y,
        );
    }

    fn draw_search_panel_if_visible(
        &mut self,
        show_search: bool,
        search_anim_y: f32,
        search_editor: &Editor,
        search_focused: bool,
        search_case_sensitive: bool,
        search_results: &[(usize, usize)],
        search_current_idx: Option<usize>,
        blink_alpha: f32,
        scrollbar_width: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
    ) -> bool {
        if search_anim_y <= -100.0 * self.scale_factor {
            return false;
        }
        self.draw_search_panel(
            show_search,
            search_anim_y,
            search_editor,
            search_focused,
            search_case_sensitive,
            search_results,
            search_current_idx,
            blink_alpha,
            scrollbar_width,
            ui_registry,
        )
    }

    fn draw_dialog_dim_if_open(&mut self, dialog_window_open: bool) {
        if dialog_window_open {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.6]);
        }
    }
}
