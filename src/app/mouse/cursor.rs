use super::*;
mod cursor_drag_updates;
mod cursor_helpers;
mod cursor_hover;
mod cursor_overlays;
mod cursor_scroll_drags;
use cursor_helpers::cursor_position_allows_editor_hover;

impl App {
    /// Cursor-move pipeline. Phases run in a fixed order; each phase that returns
    /// "handled" ends the move exactly where the original early `return` did.
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn handle_main_cursor_moved(&mut self, position: winit::dpi::PhysicalPosition<f64>) {
        let px = position.x as f32;
        let py = position.y as f32;
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.last_mouse_x = px;
            renderer.last_mouse_y = py;
        }
        if self.ui_registry.find_at(px, py) == Some(crate::ui_system::UiId::EditorBlameInline) {
            self.request_inline_blame_commit_message();
        }
        self.cancel_markdown_link_press_after_drag(px, py);
        if self.tabs.get(self.active_tab).is_some_and(|tab| tab.kind.is_image())
            && self.tabs.get_mut(self.active_tab).and_then(|tab| tab.image.as_deref_mut()).is_some_and(|image| image.drag_to(px, py))
        {
            if let Some(window) = self.window.as_ref() { window.request_redraw(); }
            return;
        }
        {
            let renderer = self.renderer.as_mut().unwrap();
            renderer.last_mouse_x = px;
            renderer.last_mouse_y = py;
            if renderer.popups_waiting_for_mouse_move_at(px, py) {
                return;
            }
            renderer.update_popup_mouse_move_gate();
        }

        if self.cursor_moved_modal_overlays(px, py) {
            return;
        }

        let window_size = self.window.as_ref().unwrap().inner_size();
        if !cursor_position_allows_editor_hover(
            position.x as f32,
            position.y as f32,
            window_size.width as f32,
            window_size.height as f32,
        ) {
            clear_hover_popup(&mut self.hover);
            self.update_ctrl_definition_hover(None);
            return;
        }

        if self.cursor_moved_autocomplete_detail(position, px, py) {
            return;
        }

        if self.cursor_moved_scroll_drags(position, px, py) {
            return;
        }

        if self.is_editor_drag_pending
            && !self.ide_panel.is_dragging_terminal
            && !self.show_settings
        {
            let dx = px - self.last_click_pos.0;
            let dy = py - self.last_click_pos.1;
            let threshold = 4.0 * self.renderer.as_ref().unwrap().scale_factor;
            if dx * dx + dy * dy > threshold * threshold {
                self.is_editor_drag_pending = false;
                self.is_dragging = true;
            }
        }

        let editor_text_selecting =
            self.is_dragging && !self.ide_panel.is_dragging_terminal && !self.show_settings;
        if self.drag_api_text_scrollbar_y_from_last_mouse()
            || self.drag_api_text_scrollbar_x_from_last_mouse()
        {
            self.window.as_ref().unwrap().request_redraw();
            return;
        }
        if editor_text_selecting && self.drag_api_text_cursor_from_last_mouse() {
            clear_hover_popup(&mut self.hover);
            self.window.as_ref().unwrap().request_redraw();
            return;
        }
        if editor_text_selecting {
            clear_hover_popup(&mut self.hover);
            if let Some(r) = self.renderer.as_mut() {
                r.suppress_popups_until_next_mouse_move();
            }
        }

        let s = self.renderer.as_ref().unwrap().scale_factor;

        if self.cursor_moved_autocomplete_list(position, s) {
            return;
        }

        if self.cursor_moved_ide_panel_drags(position, s) {
            return;
        }

        let Some((s, minimap_w, padding)) =
            self.cursor_moved_hover(position, px, py, window_size, editor_text_selecting)
        else {
            return;
        };

        if self.cursor_moved_drag_updates(position, s, window_size, minimap_w, padding) {
            return;
        }

        self.window.as_ref().unwrap().request_redraw();
    }
}
