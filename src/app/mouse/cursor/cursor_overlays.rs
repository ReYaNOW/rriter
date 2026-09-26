//! Cursor-move phases: modal overlays, database text drags, autocomplete popups.
use super::cursor_helpers::{
    autocomplete_drag_target, autocomplete_hovered_index, inline_git_popup_blocks_hover,
};
use super::*;

impl App {
    /// Modal dialogs, database drags, DDL/detail popups, overlays; `true` = handled.
    #[inline]
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(super) fn cursor_moved_modal_overlays(&mut self, px: f32, py: f32) -> bool {
        if self.update_markdown_code_copy_hover_at(px, py)
            && let Some(window) = self.window.as_ref()
        {
            window.request_redraw();
        }

        if self.modal_dialog_open() {
            return true;
        }

        let unavailable_text_dragging = self
            .active_database_table_tab_id()
            .and_then(|tab_id| self.database_table_meta_state(tab_id))
            .is_some_and(|(_, state)| state.unavailable_text_dragging);
        if unavailable_text_dragging {
            if let Some(target_index) = self.database_table_unavailable_text_index_at(px) {
                self.set_database_table_unavailable_text_cursor(target_index, true);
            }
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            return true;
        }

        let database_drag = self.update_database_table_drag(px, py);
        if database_drag.changed() {
            if let Some(tab_id) = database_drag.table_tab_id() {
                self.request_database_table_chunk_for_scroll(tab_id);
            }
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            return true;
        }

        if self.update_database_dialog_scroll_drag(py) {
            clear_hover_popup(self.renderer.as_mut());
            self.update_ctrl_definition_hover(None);
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }

        let ddl_scale = self
            .renderer
            .as_ref()
            .map_or(1.0, |renderer| renderer.scale_factor);
        if let Ok(mut ddl) = self.ide_panel.database.ddl_hover.try_borrow_mut()
            && let Some(state) = ddl.as_mut()
            && state.popup.scroll.is_dragging
            && let Some(rect) = state.rect
        {
            if let Some((drag_offset, target)) =
                crate::app::mouse::hover_popup_scrollbar_drag_target(
                    rect,
                    state.max_scroll,
                    state.popup.scroll.current,
                    py,
                    ddl_scale,
                    Some(state.popup.scroll.drag_offset),
                )
            {
                crate::app::mouse::apply_scrollbar_drag_target(
                    &mut state.popup.scroll,
                    target,
                    drag_offset,
                );
            } else {
                state.popup.scroll.end_drag();
            }
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            return true;
        }

        if let Ok(mut ddl) = self.ide_panel.database.ddl_hover.try_borrow_mut()
            && let Some(state) = ddl.as_mut()
            && state.selecting
            && let Some(rect) = state.rect
        {
            let byte = crate::app::mouse::hover_popup_byte_at(
                self.renderer.as_mut().unwrap(),
                &state.popup,
                rect,
                px,
                py,
            );
            state.selection_cursor = Some(byte);
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            return true;
        }

        if self
            .autocomplete_detail_popup
            .as_ref()
            .is_some_and(|popup| popup.scroll.is_dragging)
        {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            if let Some(rect) = self.autocomplete_detail_rect {
                let max_scroll = self.autocomplete_detail_max_scroll;
                if let Some(popup) = &mut self.autocomplete_detail_popup
                    && let Some((drag_offset, target)) =
                        crate::app::mouse::hover_popup_scrollbar_drag_target(
                            rect,
                            max_scroll,
                            popup.scroll.current,
                            py,
                            s,
                            Some(popup.scroll.drag_offset),
                        )
                {
                    let _ = crate::app::mouse::apply_scrollbar_drag_target(
                        &mut popup.scroll,
                        target,
                        drag_offset,
                    );
                }
            }
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }

        if !self.autocomplete_detail_selecting && self.autocomplete_window_contains(px, py) {
            clear_hover_popup(self.renderer.as_mut());
            self.update_ctrl_definition_hover(None);
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }

        if let Some(field) = self
            .ide_panel
            .database
            .dialog
            .as_ref()
            .and_then(|dialog| dialog.dragging_field)
        {
            if let Some(target_idx) = self.database_dialog_input_index_at(field, px) {
                self.set_database_dialog_input_cursor(field, target_idx, true);
            }
            clear_hover_popup(self.renderer.as_mut());
            self.update_ctrl_definition_hover(None);
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }

        if self.ide_panel.database.table_modal_input_dragging {
            if let Some(target_index) = self.database_table_modal_input_index_at(px, py) {
                self.set_database_table_modal_input_cursor(target_index, true);
            }
            clear_hover_popup(self.renderer.as_mut());
            self.update_ctrl_definition_hover(None);
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }

        if let Some(target) = self
            .active_database_table_tab_id()
            .and_then(|tab_id| self.database_table_meta_state(tab_id))
            .and_then(|(_, state)| state.grid.text_drag)
        {
            if let Some(target_index) = self.database_table_input_index_at(target, px) {
                self.set_database_table_input_cursor(target, target_index, true);
            }
            clear_hover_popup(self.renderer.as_mut());
            self.update_ctrl_definition_hover(None);
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }

        if self.database_blocking_modal_open() {
            clear_hover_popup(self.renderer.as_mut());
            self.update_ctrl_definition_hover(None);
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }

        if self.file_tree_overlay_active() {
            if let Some(kind) = self.ide_panel.file_tree_dialog_input_drag {
                if let Some(target_idx) = self.file_tree_dialog_input_index_at(kind, px) {
                    self.set_file_tree_dialog_input_cursor(kind, target_idx, false);
                }
            }
            self.ide_panel.file_tree_hovered_idx = None;
            clear_hover_popup(self.renderer.as_mut());
            self.update_ctrl_definition_hover(None);
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }

        if self.api_python_runtime_overlay_active() {
            clear_hover_popup(self.renderer.as_mut());
            self.update_ctrl_definition_hover(None);
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }
        false
    }

    /// Inline git popup block and autocomplete detail text selection; `true` = handled.
    #[inline]
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(super) fn cursor_moved_autocomplete_detail(
        &mut self,
        position: winit::dpi::PhysicalPosition<f64>,
        px: f32,
        py: f32,
    ) -> bool {
        if self.inline_git_popup.is_some()
            && inline_git_popup_blocks_hover(self.ui_registry.find_at(px, py))
        {
            clear_hover_popup(self.renderer.as_mut());
            self.update_ctrl_definition_hover(None);
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }

        if self.autocomplete_detail_selecting {
            if let (Some(rect), Some(popup)) = (
                self.autocomplete_detail_rect,
                self.autocomplete_detail_popup.as_ref(),
            ) {
                let byte = hover_popup_byte_at(
                    self.renderer.as_mut().unwrap(),
                    popup,
                    rect,
                    position.x as f32,
                    position.y as f32,
                );
                self.autocomplete_detail_selection_cursor = Some(byte);
                self.window.as_ref().unwrap().request_redraw();
                return true;
            }
        }

        if self.autocomplete_active {
            let px = position.x as f32;
            let py = position.y as f32;
            let in_detail = self
                .autocomplete_detail_rect
                .is_some_and(|(rx, ry, rw, rh)| {
                    px >= rx && px <= rx + rw && py >= ry && py <= ry + rh
                });
            if in_detail {
                clear_hover_popup(self.renderer.as_mut());
                self.window.as_ref().unwrap().request_redraw();
                return true;
            }
        }
        false
    }

    /// Autocomplete list scrollbar drag and item hover; `true` = handled.
    #[inline]
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(super) fn cursor_moved_autocomplete_list(
        &mut self,
        position: winit::dpi::PhysicalPosition<f64>,
        s: f32,
    ) -> bool {
        if let (true, Some((rx, ry, rw, rh))) = (self.autocomplete_active, self.autocomplete_rect) {
            let px = position.x as f32;
            let py = position.y as f32;

            if self.autocomplete_scroll.is_dragging {
                let drag_offset = self.autocomplete_scroll.drag_offset;
                let target = autocomplete_drag_target(
                    py,
                    ry,
                    rh,
                    drag_offset,
                    self.autocomplete_options.len(),
                    s,
                );
                super::input::apply_autocomplete_scroll_drag(
                    &mut self.autocomplete_scroll,
                    target,
                    drag_offset,
                );
                self.window.as_ref().unwrap().request_redraw();
                return true;
            }

            if px >= rx && px <= rx + rw && py >= ry && py <= ry + rh {
                clear_hover_popup(self.renderer.as_mut());
                self.autocomplete_hovered_idx = autocomplete_hovered_index(
                    px,
                    py,
                    (rx, ry, rw, rh),
                    self.autocomplete_scroll.current,
                    self.autocomplete_options.len(),
                    s,
                );
                self.window.as_ref().unwrap().request_redraw();
                return true;
            } else {
                self.autocomplete_hovered_idx = None;
            }
        }
        false
    }
}
