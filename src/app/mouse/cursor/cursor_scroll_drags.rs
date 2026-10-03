//! Cursor-move phase: scrollbar drags and popup/log text selection.
use super::*;

impl App {
    /// Scrollbar drags and popup/log text selection; `true` = handled.
    #[inline]
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(super) fn cursor_moved_scroll_drags(
        &mut self,
        position: winit::dpi::PhysicalPosition<f64>,
        px: f32,
        py: f32,
    ) -> bool {
        if self.active_pdf_tab().is_some_and(|pdf| pdf.scroll.is_dragging) {
            let _ = self.update_pdf_scrollbar_drag(px, py);
            if let Some(window) = self.window.as_ref() { window.request_redraw(); }
            return true;
        }
        if self.update_pdf_drag(px, py) {
            return true;
        }
        if self.ide_panel.explorer_scroll.is_dragging {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            let geometry = super::explorer_scrollbar_geometry(self, s);
            let _ = crate::app::mouse::drag_scrollbar(
                &mut self.ide_panel.explorer_scroll,
                geometry,
                px,
                py,
            );
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }

        if self.ide_panel.project_search.query_scroll_y.is_dragging {
            self.drag_project_search_query_scrollbar_to(
                crate::app::project_search::ProjectSearchQueryScrollAxis::Vertical,
                py,
            );
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }
        if self.ide_panel.project_search.query_scroll_x.is_dragging {
            self.drag_project_search_query_scrollbar_to(
                crate::app::project_search::ProjectSearchQueryScrollAxis::Horizontal,
                px,
            );
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }
        if let Some(field) = self.ide_panel.project_search.dragging_field {
            self.drag_project_search_cursor_to(field, px, py);
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }
        if self.ide_panel.project_search.scroll.is_dragging {
            if self.drag_project_search_scrollbar_to(py) {
                let _ = self.queue_visible_project_search_previews();
            }
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }

        let mut popup_selecting = false;
        {
            let hs = &mut self.hover;
            if hs.selecting {
                if let (Some(rect), Some(popup), Some(renderer)) =
                    (hs.rect, hs.popup.as_ref(), self.renderer.as_mut())
                {
                    let byte = hover_popup_byte_at(
                        renderer,
                        popup,
                        rect,
                        position.x as f32,
                        position.y as f32,
                    );
                    hs.selection_cursor = Some(byte);
                    popup_selecting = true;
                }
            } else if hs.diag_selecting {
                let byte = crate::render_view::ui::diag_popup_byte_at(
                    position.x as f32,
                    position.y as f32,
                );
                hs.diag_selection_cursor = Some(byte);
                popup_selecting = true;
            }
        }
        if popup_selecting {
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }

        if self
            .renderer
            .as_ref()
            .is_some_and(|renderer| renderer.git_graph_tooltip_selecting)
        {
            if let Some(renderer) = self.renderer.as_mut() {
                let byte = renderer.git_graph_tooltip_byte_at(position.x as f32, position.y as f32);
                renderer.git_graph_tooltip_selection_cursor = Some(byte);
            }
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }

        if self.update_api_output_schema_menu_scroll_drag(position.y as f32) {
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }

        if self.update_api_python_runtime_scroll_drag(position.y as f32) {
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }

        if self
            .renderer
            .as_ref()
            .is_some_and(|renderer| renderer.git_logs_selecting)
        {
            let anchor = self
                .ide_panel
                .git
                .git_logs
                .selection()
                .map(|selection| selection.anchor);
            let point = {
                let logs = &self.ide_panel.git.git_logs;
                self.renderer.as_mut().and_then(|renderer| {
                    renderer.git_logs_text_point_at(logs, position.x as f32, position.y as f32)
                })
            };
            if let (Some(anchor), Some(point)) = (anchor, point) {
                let _ = self.ide_panel.git.git_logs.set_selection(anchor, point);
            } else if let Some(renderer) = self.renderer.as_mut() {
                renderer.git_logs_selecting = false;
            }
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }

        if self.ide_panel.api.mock_server_log_scroll.is_dragging {
            if let Some(rect) = self
                .ui_registry
                .rect_for(crate::ui_system::UiId::ApiMockServerLogScrollY)
            {
                let scale = self.renderer.as_ref().unwrap().scale_factor;
                if let Some((drag_offset, target)) =
                    crate::app::api_client::api_mock_server_log_scrollbar_drag_target(
                        rect,
                        self.ide_panel.api.mock_server_logs.len(),
                        self.ide_panel.api.mock_server_log_scroll.current,
                        position.y as f32,
                        scale,
                        Some(self.ide_panel.api.mock_server_log_scroll.drag_offset),
                    )
                {
                    let scroll = &mut self.ide_panel.api.mock_server_log_scroll;
                    crate::app::mouse::apply_scrollbar_drag_target(scroll, target, drag_offset);
                }
            }
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }

        if self.ide_panel.api.mock_guide_scroll.is_dragging {
            if let Some(rect) = self
                .ui_registry
                .rect_for(crate::ui_system::UiId::ApiMockGuideScrollY)
            {
                let s = self.renderer.as_ref().unwrap().scale_factor;
                let scroll = &mut self.ide_panel.api.mock_guide_scroll;
                let geometry = crate::app::api_client::api_mock_guide_scrollbar(
                    rect,
                    scroll.current,
                    s,
                )
                .geometry(s);
                let _ = crate::app::mouse::drag_scrollbar(scroll, geometry, px, py);
            }
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }

        if self.settings_ide_scroll.is_dragging {
            if let Some(rect) = self
                .ui_registry
                .rect_for(crate::ui_system::UiId::SettingsIdeScrollY)
            {
                let s = self.renderer.as_ref().unwrap().scale_factor;
                let window_size = self.window.as_ref().unwrap().inner_size();
                let layout = crate::render_view::settings_ui::animated_settings_modal_layout(
                    window_size.width as f32,
                    window_size.height as f32,
                    s,
                    self.settings_anim_progress,
                );
                let pad_x = 12.0 * s;
                let max_scroll = crate::render_view::settings_ui::settings_ide_max_scroll(
                    layout,
                    self.ide_workspaces.len(),
                    self.ide_ignore_patterns.iter().map(|pattern| {
                        self.renderer
                            .as_mut()
                            .unwrap()
                            .measure_ui_width(pattern, 0.88)
                            + pad_x * 2.0
                            + 22.0 * s
                    }),
                    s,
                );
                let bar = crate::render_view::settings_ui::settings_scrollbar(
                    rect, rect.3, max_scroll, self.settings_ide_scroll.current,
                    6.0, 40.0, [0.7, 0.33, 0.54, 1.0],
                );
                let geometry = bar.geometry(s);
                crate::app::mouse::drag_scrollbar(
                    &mut self.settings_ide_scroll, geometry, 0.0, position.y as f32,
                );
            }
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }

        if self.settings_scroll.is_dragging {
            if let Some(rect) = self
                .ui_registry
                .rect_for(crate::ui_system::UiId::SettingsFaqScrollY)
            {
                let s = self.renderer.as_ref().unwrap().scale_factor;
                let max_scroll = self
                    .renderer
                    .as_mut()
                    .unwrap()
                    .get_faq_max_scroll(&self.faq_editor, rect.3);
                let bar = crate::render_view::settings_ui::settings_scrollbar(
                    rect, rect.3, max_scroll, self.settings_scroll.current,
                    6.0, 40.0, [0.7, 0.33, 0.54, 1.0],
                );
                let geometry = bar.geometry(s);
                crate::app::mouse::drag_scrollbar(
                    &mut self.settings_scroll, geometry, 0.0, position.y as f32,
                );
            }
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }

        if self.settings_general_scroll.is_dragging {
            if let Some(rect) = self.ui_registry.rect_for(crate::ui_system::UiId::SettingsGeneralScrollY) {
                let s = self.renderer.as_ref().unwrap().scale_factor;
                let bar = crate::render_view::settings_ui::settings_scrollbar(
                    rect, rect.3, self.settings_general_max_scroll,
                    self.settings_general_scroll.current, 6.0, 40.0,
                    [0.7, 0.33, 0.54, 1.0],
                );
                let geometry = bar.geometry(s);
                crate::app::mouse::drag_scrollbar(
                    &mut self.settings_general_scroll, geometry, 0.0, position.y as f32,
                );
            }
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }

        if self.settings_database_scroll.is_dragging {
            if let Some(rect) = self.ui_registry.rect_for(crate::ui_system::UiId::SettingsDatabaseScrollY) {
                let s = self.renderer.as_ref().unwrap().scale_factor;
                let bar = crate::render_view::settings_ui::settings_scrollbar(
                    rect, rect.3, self.settings_database_max_scroll,
                    self.settings_database_scroll.current, 6.0, 40.0,
                    [0.7, 0.33, 0.54, 1.0],
                );
                let geometry = bar.geometry(s);
                crate::app::mouse::drag_scrollbar(
                    &mut self.settings_database_scroll, geometry, 0.0, position.y as f32,
                );
            }
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }

        if self.keymap_settings.scroll.is_dragging {
            if let Some(rect) = self.ui_registry.rect_for(crate::ui_system::UiId::SettingsKeymapScrollY) {
                let s = self.renderer.as_ref().map(|renderer| renderer.scale_factor).unwrap_or(1.0);
                let bar = crate::render_view::settings_ui::settings_scrollbar(
                    rect, rect.3, self.keymap_settings.max_scroll,
                    self.keymap_settings.scroll.current, 6.0, 36.0,
                    [0.7, 0.33, 0.54, 1.0],
                );
                crate::app::mouse::drag_scrollbar(
                    &mut self.keymap_settings.scroll, bar.geometry(s), 0.0, py,
                );
            }
            if let Some(window) = self.window.as_ref() { window.request_redraw(); }
            return true;
        }

        if self.tool_installer.log_scroll_is_dragging() {
            if let Some(rect) = self
                .ui_registry
                .rect_for(crate::ui_system::UiId::SettingsToolInstallLogScrollY)
            {
                let s = self.renderer.as_ref().unwrap().scale_factor;
                let line_h = crate::app::tool_installer::log_line_height(s);
                let content_h = (self.tool_installer.logs().len().max(1) as f32 * line_h
                    + (12.0 * s).round())
                .round();
                self.tool_installer.drag_log_scroll(
                    position.y as f32,
                    rect.1 + 6.0 * s,
                    rect.3 - 12.0 * s,
                    rect.3,
                    content_h,
                    28.0 * s,
                );
            }
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }

        if self.drag_api_route_text_selection_from_last_mouse() {
            clear_hover_popup(&mut self.hover);
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }

        if self.markdown_mode() == crate::app::MarkdownMode::Read
            && self.markdown.code_scroll_drag.is_some()
        {
            let _ = self.drag_markdown_code_scrollbar_to(px);
            clear_hover_popup(&mut self.hover);
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            return true;
        }

        if self.markdown_mode() == crate::app::MarkdownMode::Read && self.scroll_y.is_dragging {
            let _ = self.drag_markdown_read_scrollbar_to(py);
            clear_hover_popup(&mut self.hover);
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }

        if self.markdown.read_selecting {
            let _ = self.update_markdown_read_selection_at(px, py);
            clear_hover_popup(&mut self.hover);
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }
        false
    }
}
