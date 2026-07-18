use crate::editor::Editor;
use crate::renderer::Renderer;
use crate::widgets::IconButton;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SearchPanelGeometry {
    pub x: f32,
    pub w: f32,
    pub input_w: f32,
    pub close_x: f32,
    pub close_size: f32,
    pub counter_reserve: f32,
}

pub(crate) fn search_panel_geometry(scrollbar_x: f32, scale: f32) -> SearchPanelGeometry {
    let w = (480.0 * scale).min((scrollbar_x - 8.0 * scale).max(0.0));
    let x = (scrollbar_x - w - 8.0 * scale).max(0.0);
    let btn_size = 36.0 * scale;
    let gap = (10.0 * scale).min(w * 0.025);
    let show_nav = w >= 250.0 * scale;
    let show_case = w >= 330.0 * scale;
    let count = 1 + usize::from(show_nav) * 2 + usize::from(show_case);
    let controls = count as f32 * btn_size + count.saturating_sub(1) as f32 * gap;
    let counter = if w >= 235.0 * scale { 52.0 * scale } else { 0.0 };
    let input_w = (w - 20.0 * scale - controls - counter - 8.0 * scale).max(0.0);
    let close_size = btn_size.min(w.max(0.0));
    let close_x = (x + w - 10.0 * scale - close_size).max(x);
    SearchPanelGeometry { x, w, input_w, close_x, close_size, counter_reserve: counter }
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_search_panel(
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
        let wants_pointer = false;
        let s = self.scale_factor;
        let scrollbar_x = self.width - self.minimap_width - scrollbar_width;
        let geometry = search_panel_geometry(scrollbar_x, s);
        let search_w = geometry.w;
        let search_h = 52.0 * s;
        let search_x = geometry.x;

        if search_w > 0.0 && search_h > 0.0 {
            ui_registry.register_blocker(
                crate::ui_system::UiId::SearchPanelBody,
                search_x,
                search_anim_y,
                search_w,
                search_h,
                self.last_mouse_x,
                self.last_mouse_y,
            );
        }

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
        let input_h = 30.0 * s;
        let btn_size = 36.0 * s;
        let btn_gap = (10.0 * s).min(search_w * 0.025);
        let show_nav = search_w >= 250.0 * s;
        let show_case = search_w >= 330.0 * s;
        let input_w = geometry.input_w;
        let counter_reserve = geometry.counter_reserve;

        if input_w > 0.0 {
            ui_registry.register_text_input(
                crate::ui_system::UiId::SearchInput,
                input_x,
                input_y,
                input_w,
                input_h,
                self.last_mouse_x,
                self.last_mouse_y,
            );
        }
        let text = search_editor.get_full_text();
        self.search_scroll_x = self.one_line_scroll_for_cursor(
            &text,
            search_editor.cursor,
            1.0,
            (input_w - 10.0 * s).max(0.0),
            self.search_scroll_x,
        );
        self.draw_one_line_input_with_chrome(
            &text,
            search_editor.cursor,
            search_editor.selection_anchor,
            false,
            search_focused,
            input_x,
            input_y,
            input_w,
            input_h,
            self.search_scroll_x,
            blink_alpha,
            1.0,
            0.0,
            5.0 * s,
            4.0 * s,
        );

        let text_y = input_y + input_h / 2.0 + 6.0 * s;
        let btn_y = search_anim_y + 8.0 * s;

        let close_size = geometry.close_size;
        let mut current_x = geometry.close_x;
        let btn_close = IconButton {
            x: current_x,
            y: btn_y,
            size: close_size,
            icon: Some(crate::widgets::IconType::Close),
            is_active: false,
            icon_size: Some(26.0 * s),
            active_square_width: None,
            custom_color: None,
        };
        current_x -= btn_gap;

        let btn_down = if show_nav {
            current_x -= btn_size;
            let button = IconButton {
                x: current_x, y: btn_y, size: btn_size,
                icon: Some(crate::widgets::IconType::Down), is_active: false,
                icon_size: Some(37.0 * s), active_square_width: None, custom_color: None,
            };
            current_x -= btn_gap;
            Some(button)
        } else { None };
        let btn_up = if show_nav {
            current_x -= btn_size;
            let button = IconButton {
                x: current_x, y: btn_y, size: btn_size,
                icon: Some(crate::widgets::IconType::Up), is_active: false,
                icon_size: Some(37.0 * s), active_square_width: None, custom_color: None,
            };
            current_x -= btn_gap;
            Some(button)
        } else { None };
        let btn_case = if show_case {
            current_x -= btn_size;
            Some(IconButton {
                x: current_x, y: btn_y, size: btn_size,
                icon: Some(crate::widgets::IconType::CaseMatch), is_active: search_case_sensitive,
                icon_size: Some(30.0 * s), active_square_width: None, custom_color: None,
            })
        } else { None };

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

        let (res_text, text_color) = if !show_search {
            ("", [0.6, 0.6, 0.6, 1.0])
        } else if search_results.is_empty() {
            if search_editor.get_full_text().is_empty() {
                ("", [0.6, 0.6, 0.6, 1.0])
            } else {
                ("Нет", [0.95, 0.35, 0.45, 1.0])
            }
        } else {
            (temp_res_text.as_str(), [0.6, 0.6, 0.6, 1.0])
        };

        if counter_reserve > 0.0 && !res_text.is_empty() {
            let counter_x = input_x + input_w + 10.0 * s;
            self.draw_string_mono_scaled(res_text, counter_x, text_y, text_color, 0.9);
        }

        self.search_res_string = temp_res_text;

        let mx = self.last_mouse_x;
        let my = self.last_mouse_y;

        if let Some(btn_case) = &btn_case {
            ui_registry.register_icon_button(
                crate::ui_system::UiId::SearchCaseToggle, btn_case, self, mx, my, s, false,
            );
        }
        if let Some(btn_up) = &btn_up {
            ui_registry.register_icon_button(
                crate::ui_system::UiId::SearchPrev, btn_up, self, mx, my, s, false,
            );
        }
        if let Some(btn_down) = &btn_down {
            ui_registry.register_icon_button(
                crate::ui_system::UiId::SearchNext, btn_down, self, mx, my, s, false,
            );
        }
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
