use crate::editor::Editor;
use crate::renderer::Renderer;
use crate::widgets::IconButton;
use glow::HasContext;

impl Renderer {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_search_panel(
        &mut self,
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
        let wants_pointer = false;
        let s = self.scale_factor;
        let scrollbar_x = self.width - self.minimap_width - scrollbar_width;
        let search_w = 480.0 * s;
        let search_h = 52.0 * s;
        let search_x = scrollbar_x - search_w - 20.0 * s;

        self.push_rounded_rect(
            search_x,
            search_anim_y,
            search_w,
            search_h,
            6.0 * s,
            [0.18, 0.20, 0.22, 1.0],
        );
        self.push_rounded_rect(
            search_x - 1.0,
            search_anim_y - 1.0,
            search_w + 2.0,
            search_h + 2.0,
            6.0 * s,
            [self.theme.sel[0], self.theme.sel[1], self.theme.sel[2], 0.6],
        );

        self.push_rounded_rect(
            search_x,
            search_anim_y,
            search_w,
            search_h,
            6.0 * s,
            [
                self.theme.minimap_bg[0],
                self.theme.minimap_bg[1],
                self.theme.minimap_bg[2],
                1.0,
            ],
        );

        let input_x = search_x + 10.0 * s;
        let input_y = search_anim_y + 11.0 * s;
        let input_w = 215.0 * s;
        let input_h = 30.0 * s;

        let input_bg = self.theme.bg;
        let input_border = if search_focused {
            self.theme.sel
        } else {
            [0.3, 0.3, 0.3, 1.0]
        };
        self.push_rounded_rect(
            input_x - 1.0,
            input_y - 1.0,
            input_w + 2.0,
            input_h + 2.0,
            4.0 * s,
            input_border,
        );
        self.push_rounded_rect(input_x, input_y, input_w, input_h, 4.0 * s, input_bg);

        ui_registry.register_text_input(
            crate::ui_system::UiId::SearchInput,
            input_x,
            input_y,
            input_w,
            input_h,
            self.last_mouse_x,
            self.last_mouse_y,
        );

        self.flush();
        unsafe {
            let text = search_editor.get_full_text();
            let text_y = input_y + input_h / 2.0 + 6.0 * s;
            let text_start_x = input_x + 5.0 * s;
            let visible_width = input_w - 10.0 * s;

            let mut cursor_total_x = 0.0;
            let mut total_text_width = 0.0;
            for (byte_idx, c) in text.char_indices() {
                let char_to_measure = if c == '\n' { '↵' } else { c };
                let adv = self
                    .get_ui_glyph(char_to_measure)
                    .map(|g| g.advance)
                    .unwrap_or(10.0);
                if byte_idx < search_editor.cursor {
                    cursor_total_x += adv;
                }
                total_text_width += adv;
            }

            if cursor_total_x - self.search_scroll_x > visible_width {
                self.search_scroll_x = cursor_total_x - visible_width;
            }
            if cursor_total_x - self.search_scroll_x < 0.0 {
                self.search_scroll_x = cursor_total_x;
            }
            self.search_scroll_x = self
                .search_scroll_x
                .min(total_text_width - visible_width)
                .max(0.0);

            self.gl.enable(glow::SCISSOR_TEST);
            let scissor_y = self.height - (input_y + input_h);
            self.gl.scissor(
                input_x as i32,
                scissor_y as i32,
                input_w as i32,
                input_h as i32,
            );

            let sel_start = search_editor
                .selection_anchor
                .unwrap_or(search_editor.cursor)
                .min(search_editor.cursor);
            let sel_end = search_editor
                .selection_anchor
                .unwrap_or(search_editor.cursor)
                .max(search_editor.cursor);

            let mut current_x = text_start_x - self.search_scroll_x;
            let mut byte_idx = 0;
            let mut cursor_draw_x = current_x;

            for c in text.chars() {
                if byte_idx == search_editor.cursor {
                    cursor_draw_x = current_x;
                }

                let char_to_render = if c == '\n' { '↵' } else { c };
                let adv = self
                    .get_ui_glyph(char_to_render)
                    .map(|g| g.advance)
                    .unwrap_or(10.0);

                if byte_idx >= sel_start && byte_idx < sel_end {
                    self.push_rect(
                        current_x,
                        input_y + 4.0 * s,
                        adv,
                        input_h - 8.0 * s,
                        self.theme.sel,
                    );
                }

                if let Some(g) = self.get_ui_glyph(char_to_render) {
                    self.push_quad(
                        current_x + g.offset_x,
                        text_y - g.offset_y,
                        g.width,
                        g.height,
                        g.u,
                        g.v,
                        g.uw,
                        g.vh,
                        self.theme.fg,
                        g.is_emoji,
                    );
                }

                current_x += adv;
                byte_idx += c.len_utf8();
            }
            if byte_idx == search_editor.cursor {
                cursor_draw_x = current_x;
            }

            if search_focused && sel_start == sel_end && blink_alpha > 0.5 {
                self.push_rect(
                    cursor_draw_x,
                    input_y + 4.0 * s,
                    2.0 * s,
                    input_h - 8.0 * s,
                    self.theme.fg,
                );
            }

            self.flush();
            self.gl.disable(glow::SCISSOR_TEST);
        }

        let text_y = input_y + input_h / 2.0 + 6.0 * s;
        let btn_y = search_anim_y + 8.0 * s;
        let btn_size = 36.0 * s;

        let mut current_x = search_x + search_w - 10.0 * s;

        current_x -= btn_size;
        let btn_close = IconButton {
            x: current_x,
            y: btn_y,
            size: btn_size,
            icon: Some(crate::widgets::IconType::Close),
            is_active: false,
            icon_size: Some(26.0 * s),
            active_square_width: None,
            custom_color: None,
        };
        current_x -= 10.0 * s;

        current_x -= btn_size;
        let btn_down = IconButton {
            x: current_x,
            y: btn_y,
            size: btn_size,
            icon: Some(crate::widgets::IconType::Down),
            is_active: false,
            icon_size: Some(37.0 * s),
            active_square_width: None,
            custom_color: None,
        };
        current_x -= 10.0 * s;

        current_x -= btn_size;
        let btn_up = IconButton {
            x: current_x,
            y: btn_y,
            size: btn_size,
            icon: Some(crate::widgets::IconType::Up),
            is_active: false,
            icon_size: Some(37.0 * s),
            active_square_width: None,
            custom_color: None,
        };
        current_x -= 10.0 * s;

        current_x -= btn_size;
        let btn_case = IconButton {
            x: current_x,
            y: btn_y,
            size: btn_size,
            icon: Some(crate::widgets::IconType::CaseMatch),
            is_active: search_case_sensitive,
            icon_size: Some(30.0 * s),
            active_square_width: None,
            custom_color: None,
        };

        if search_results.len() != self.last_search_len
            || search_current_idx != self.last_search_idx
        {
            self.search_res_string.clear();
            if !search_results.is_empty() {
                use std::fmt::Write;
                let _ = write!(
                    &mut self.search_res_string,
                    "{}/{}",
                    search_current_idx.unwrap_or(0) + 1,
                    search_results.len()
                );
            }
            self.last_search_len = search_results.len();
            self.last_search_idx = search_current_idx;
        }

        let temp_res_text = std::mem::take(&mut self.search_res_string);

        let (res_text, text_color) = if search_results.is_empty() {
            if search_editor.get_full_text().is_empty() {
                ("", [0.6, 0.6, 0.6, 1.0])
            } else {
                ("Нет", [0.95, 0.35, 0.45, 1.0])
            }
        } else {
            (temp_res_text.as_str(), [0.6, 0.6, 0.6, 1.0])
        };

        if !res_text.is_empty() {
            let counter_x = input_x + input_w + 10.0 * s;
            self.draw_string_scaled(res_text, counter_x, text_y, text_color, 0.9);
        }

        self.search_res_string = temp_res_text;

        let mx = self.last_mouse_x;
        let my = self.last_mouse_y;

        ui_registry.register_icon_button(
            crate::ui_system::UiId::SearchCaseToggle,
            &btn_case,
            self,
            mx,
            my,
            s,
            false,
        );
        ui_registry.register_icon_button(
            crate::ui_system::UiId::SearchPrev,
            &btn_up,
            self,
            mx,
            my,
            s,
            false,
        );
        ui_registry.register_icon_button(
            crate::ui_system::UiId::SearchNext,
            &btn_down,
            self,
            mx,
            my,
            s,
            false,
        );
        ui_registry.register_icon_button(
            crate::ui_system::UiId::SearchClose,
            &btn_close,
            self,
            mx,
            my,
            s,
            false,
        );

        wants_pointer || ui_registry.wants_pointer()
    }
}
