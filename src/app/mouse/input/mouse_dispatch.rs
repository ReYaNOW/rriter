use super::*;

impl App {
    /// Mouse button dispatcher. Phases run in a fixed order; a phase returning
    /// `true` consumed the event. Phase bodies live in sibling modules:
    /// `mouse_drag_capture` (release captures, panel DnD/resize end),
    /// `mouse_overlay_press` (popups, menus, modal overlays, settings),
    /// `mouse_editor_terminal_press` (autocomplete, Ctrl+click, terminal mouse),
    /// `mouse_declarative_press` (LSP menu, pre-dispatch) and
    /// `mouse_ui_element_press` (press on a UI registry element).
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(super) fn handle_main_mouse_input_inner(
        &mut self,
        _event_loop: &HostLoop,
        state: ElementState,
        button: winit::event::MouseButton,
    ) {
        let mx = self.renderer.as_ref().unwrap().last_mouse_x;
        let my = self.renderer.as_ref().unwrap().last_mouse_y;
        if self.markdown_toc.open
            && state == ElementState::Pressed
            && !self.markdown_toc.rect.is_some_and(|rect| crate::ui_system::point_in_rect(mx, my, rect))
        {
            self.markdown_toc.close();
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            return;
        }
        if self.finish_text_captures_on_release(state, button, mx, my) {
            return;
        }
        if self.handle_image_mouse(state, button, mx, my) { return; }
        if self.prepare_left_press(state, button, mx, my) {
            return;
        }
        if self.finish_popup_captures_on_release(state) {
            return;
        }

        self.dismiss_hover_popup_on_press(state, mx, my);

        if self.handle_database_ddl_popup_click(state, button, mx, my) {
            return;
        }

        if self.handle_modal_overlay_press(state, button, mx, my) {
            return;
        }

        if self.handle_menu_dismiss_press(state, button, mx, my) {
            return;
        }
        if self.handle_autocomplete_and_definition_press(state, button, mx, my) {
            return;
        }

        self.forward_terminal_mouse_report(state, button, mx, my);

        if state == ElementState::Pressed
            && button == winit::event::MouseButton::Left
            && self.handle_lsp_actions_menu_press(state, mx, my)
        {
            return;
        }

        if state == ElementState::Pressed
            && button == winit::event::MouseButton::Left
            && self.dispatch_declarative_ui_press(state, button, mx, my)
        {
            return;
        }

        // Clicks routed through UI system

        if self.handle_modal_window_and_settings_click(state) {
            return;
        }

        if self.finish_panel_drags_on_release(state, button) {
            return;
        }

        if state == ElementState::Pressed {
            let last_mouse_x = self.renderer.as_ref().unwrap().last_mouse_x;
            let last_mouse_y = self.renderer.as_ref().unwrap().last_mouse_y;

            // Sidebar processing and resizing moved to ui_registry
            // Обработка кликов в дереве файлов теперь выполняется через ui_registry
            // Search input handled by ui_registry

            if self.autocomplete_active {
                if let Some((rx, ry, rw, rh)) = self.autocomplete_rect {
                    if last_mouse_x >= rx
                        && last_mouse_x <= rx + rw
                        && last_mouse_y >= ry
                        && last_mouse_y <= ry + rh
                    {
                        self.close_autocomplete();
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    } else {
                        self.close_autocomplete();
                        self.window.as_ref().unwrap().request_redraw();
                    }
                }
            }

            // Sticky lines, Folding, Scrollbars and Text Selection handled by ui_registry
            self.last_action = Instant::now();
        }
        self.window.as_ref().unwrap().request_redraw();
    }
}
