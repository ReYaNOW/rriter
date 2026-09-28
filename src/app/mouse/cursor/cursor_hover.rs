//! Cursor-move phase: file tree, API mock, bottom panel and editor hover.
use super::cursor_helpers::{
    diagnostic_hover_byte_at, should_suppress_editor_hover_for_scroll_drag,
};
use super::*;

impl App {
    /// Hover updates; `None` = handled, else `(s, minimap_w, padding)` for drags.
    #[inline]
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(super) fn cursor_moved_hover(
        &mut self,
        position: winit::dpi::PhysicalPosition<f64>,
        px: f32,
        py: f32,
        window_size: winit::dpi::PhysicalSize<u32>,
        editor_text_selecting: bool,
    ) -> Option<(f32, f32, f32)> {
        let suppress_editor_hover = should_suppress_editor_hover_for_scroll_drag(
            self.scroll_y.is_dragging,
            self.scroll_x.is_dragging,
            self.markdown_mode() == crate::app::MarkdownMode::Read,
        );
        if suppress_editor_hover {
            clear_hover_popup(&mut self.hover);
            self.update_ctrl_definition_hover(None);
        }

        // Hover над узлами дерева файлов
        if self.is_ide_mode && self.ide_panel.is_open(crate::app::PanelId::Explorer) {
            let new_hover = self.file_tree_node_at(position.x as f32, position.y as f32);

            if new_hover != self.ide_panel.file_tree_hovered_idx {
                self.ide_panel.file_tree_hovered_idx = new_hover;
                self.window.as_ref().unwrap().request_redraw();
            }
        }

        let s = self.renderer.as_ref().unwrap().scale_factor;
        let (in_hover_popup, in_hover_source_line) = self.hover.popup_safe_area_contains(
            position.x as f32,
            position.y as f32,
            self.renderer.as_ref().unwrap().width,
            s,
        );
        if self.update_api_mock_hover_from_cursor(
            position.x as f32,
            position.y as f32,
            in_hover_popup,
            in_hover_source_line,
        ) {
            self.update_ctrl_definition_hover(None);
            self.window.as_ref().unwrap().request_redraw();
            return None;
        }

        let minimap_w = self.renderer.as_ref().unwrap().minimap_width;
        let padding = self.renderer.as_ref().unwrap().left_padding;
        let bottom_panel_h = if self.is_ide_mode && self.ide_panel.any_bottom_open() {
            self.ide_panel.bottom_height * s
        } else {
            0.0
        };
        let bottom_panel_y =
            crate::render_view::ide_bottom_panel_y(window_size.height as f32, bottom_panel_h, s);
        let in_blocking_bottom_panel = self.is_ide_mode
            && bottom_panel_h > 0.0
            && self.ide_panel.bottom_panel_blocks_editor_hover()
            && position.y as f32 >= bottom_panel_y
            && position.y as f32 <= bottom_panel_y + bottom_panel_h;
        let in_database_query_results = self.active_tab_is_database_query()
            && crate::app::mouse::HoverState::database_query_results_block_hover_at(
                &self.ui_registry,
                px,
                py,
            );

        if in_blocking_bottom_panel || in_database_query_results {
            clear_hover_popup(&mut self.hover);
            self.update_ctrl_definition_hover(None);
        }

        if !suppress_editor_hover
            && !in_blocking_bottom_panel
            && !in_database_query_results
            && (!in_hover_popup || in_hover_source_line)
        {
            let tab_bar_h = crate::render_view::editor_content_top_inset(
                self.show_welcome,
                self.is_ide_mode,
                self.active_tab_is_database_query(),
                s,
            );
            let render_scroll_y = self.scroll_y.current.round() - tab_bar_h;
            let px = position.x as f32;
            let py = position.y as f32;
            let mut diag_hover_byte = None;
            let line_h = self.renderer.as_ref().unwrap().line_height;
            let baseline_offset = self.renderer.as_ref().unwrap().baseline_offset;
            let hover_content_y =
                hover_screen_y_to_content_y(py, render_scroll_y, line_h, baseline_offset)
                    .unwrap_or(0.0);

            let render_scroll_x = self.scroll_x.current.round();
            let left_padding = self
                .renderer
                .as_ref()
                .map_or(0.0, |renderer| renderer.left_padding);
            let cursor_phys_line = self
                .editor
                .line_offsets
                .partition_point(|&offset| offset <= self.editor.cursor)
                .saturating_sub(1);
            if self.active_tab_is_database_query() {
                let diagnostics = self
                    .tabs
                    .get(self.active_tab)
                    .and_then(|tab| match &tab.kind {
                        crate::app::EditorTabKind::DatabaseQuery(_, state) => {
                            Some(state.editor_diagnostics.as_slice())
                        }
                        _ => None,
                    });
                if let Some(diagnostics) = diagnostics
                    && let Some(renderer) = self.renderer.as_mut()
                {
                    diag_hover_byte = diagnostic_hover_byte_at(
                        &self.editor,
                        renderer,
                        diagnostics.iter(),
                        cursor_phys_line,
                        px,
                        hover_content_y,
                        render_scroll_x,
                        left_padding,
                        line_h,
                    );
                }
            } else if let (Some(lsp), Some(path)) = (self.lsp.as_ref(), self.file_path.as_ref()) {
                let (_, diagnostics) = lsp.instant_merged_diagnostics(path);
                if let Some(renderer) = self.renderer.as_mut() {
                    diag_hover_byte = diagnostic_hover_byte_at(
                        &self.editor,
                        renderer,
                        diagnostics,
                        cursor_phys_line,
                        px,
                        hover_content_y,
                        render_scroll_x,
                        left_padding,
                        line_h,
                    );
                }
            }

            let byte_offset = if let Some(byte) = diag_hover_byte {
                byte
            } else {
                let line_top_y = (hover_content_y / line_h).floor() * line_h;
                if hover_content_y_in_line_hitbox(hover_content_y, line_top_y, line_h) {
                    self.renderer.as_mut().unwrap().get_byte_at_xy(
                        &self.editor,
                        px,
                        hover_content_y,
                    )
                } else {
                    self.editor.len()
                }
            };
            let hover_on_inlay_hint = if diag_hover_byte.is_none() {
                let line_top_y = (hover_content_y / line_h).floor() * line_h;
                hover_content_y_in_line_hitbox(hover_content_y, line_top_y, line_h)
                    && self.renderer.as_mut().unwrap().is_inlay_hint_at_xy(
                        &self.editor,
                        px,
                        hover_content_y,
                    )
            } else {
                false
            };
            let cleared_inlay_hover = if hover_on_inlay_hint {
                clear_hover_popup(&mut self.hover)
            } else {
                false
            };
            let in_diag_popup = self
                .hover
                .diag_rect
                .map(|(rx, ry, rw, rh, _, _, _)| {
                    position.x as f32 >= rx
                        && position.x as f32 <= rx + rw
                        && position.y as f32 >= ry
                        && position.y as f32 <= ry + rh
                })
                .unwrap_or(false);
            let is_text_area = (!in_diag_popup || in_hover_popup)
                && !hover_on_inlay_hint
                && position.x as f32 > padding
                && (position.x as f32) < (window_size.width as f32 - minimap_w);
            let ctrl_definition_byte = if is_text_area {
                normalize_hover_byte(&self.editor, byte_offset)
            } else {
                None
            };

            let mut clear_diag_popup = false;
            {
                let state = &mut self.hover;
                let keep_visible_popup = state.popup.is_some();
                let old_byte = state.byte_offset;
                if let Some(should_clear_diag) =
                    crate::app::mouse::update_editor_hover_state_for_cursor(
                        state,
                        &self.editor,
                        byte_offset,
                        diag_hover_byte,
                        is_text_area,
                        in_hover_popup,
                        in_hover_source_line,
                        editor_text_selecting,
                    )
                {
                    if should_clear_diag && crate::render_view::hover_trace_enabled() {
                        println!("[HOVER DEBUG] cursor -> new word ({}). old_byte: {:?}. keep_old_popup: {}. start 0.34s request timer.", byte_offset, old_byte, keep_visible_popup);
                    }
                    clear_diag_popup = should_clear_diag;
                }
            }
            if clear_diag_popup {
                self.hover.reset_diagnostic_popup();
            }
            self.update_ctrl_definition_hover(ctrl_definition_byte);
            if cleared_inlay_hover {
                self.window.as_ref().unwrap().request_redraw();
            }
        }
        Some((s, minimap_w, padding))
    }
}
