// Left-press pre-dispatch: LSP actions menu, problems scrollbar, panel edge
// resize start and menu dismissal before the UI registry element click.
use super::*;

impl App {
    /// LSP actions menu press: apply the clicked action or close the menu. Returns true when the event is consumed.
    #[cfg_attr(coverage_nightly, coverage(off))]
    #[inline]
    pub(super) fn handle_lsp_actions_menu_press(
        &mut self,
        state: ElementState,
        mx: f32,
        my: f32,
    ) -> bool {
        if let Some(menu) = self.lsp_actions_menu.as_ref() {
            let menu_snapshot = menu.clone();
            let layout = self
                .renderer
                .as_mut()
                .unwrap()
                .lsp_actions_menu_layout(&menu_snapshot);
            let mut clicked_inside = false;
            if state == ElementState::Pressed {
                if mx >= layout.x
                    && mx <= layout.x + layout.w
                    && my >= layout.y
                    && my <= layout.y + layout.h
                {
                    clicked_inside = true;
                    let rel_y =
                        my - layout.y - 4.0 * self.renderer.as_ref().unwrap().scale_factor;
                    if rel_y >= 0.0 {
                        let visible_idx = (rel_y / layout.item_h) as usize;
                        if visible_idx >= layout.visible_items {
                            return true;
                        }
                        let idx = layout.first_visible + visible_idx;
                        if idx >= menu.items.len() {
                            return true;
                        }
                        if let Some(menu) = self.lsp_actions_menu.as_mut() {
                            menu.selected = idx;
                        }
                        self.apply_selected_lsp_action();
                        return true;
                    }
                }
            }

            if !clicked_inside {
                self.lsp_actions_menu = None;
                self.window.as_ref().unwrap().request_redraw();
            } else {
                return true;
            }
        }
        false
    }

    /// Declarative UI press outside settings/modals; hands the hit element to `handle_ui_element_press`. Returns true when the event is consumed.
    #[cfg_attr(coverage_nightly, coverage(off))]
    #[inline]
    pub(super) fn dispatch_declarative_ui_press(
        &mut self,
        state: ElementState,
        button: winit::event::MouseButton,
        mx: f32,
        my: f32,
    ) -> bool {
        // Глобальная обработка декларативного UI
        if !self.show_settings && !self.modal_dialog_open() {
            if self.is_ide_mode
                && let Some(layout) = super::problems_scrollbar_layout(
                    self,
                    self.renderer.as_ref().unwrap().scale_factor,
                )
                && crate::ui_system::point_in_rect(
                    mx,
                    my,
                    (
                        layout.content_x,
                        layout.content_y,
                        layout.content_w,
                        layout.content_h,
                    ),
                )
            {
                let s = self.renderer.as_ref().unwrap().scale_factor;
                let scroll_x = layout.content_x + layout.content_w - 12.0 * s;
                if mx >= scroll_x
                    && let Some(thumb) = crate::scroll::scrollbar_thumb(
                        layout.list_y,
                        layout.track_h,
                        layout.track_h,
                        layout.total_h,
                        self.ide_panel.problems_scroll.current,
                        20.0 * s,
                    )
                {
                    if my < layout.list_y || my > layout.list_y + layout.track_h {
                        return true;
                    }
                    let max_scroll = (layout.total_h - layout.track_h).max(0.0);
                    let Some((drag_offset, target)) = crate::scroll::scrollbar_drag_target(
                        my,
                        layout.list_y,
                        layout.track_h,
                        thumb,
                        max_scroll,
                        None,
                    ) else {
                        return true;
                    };
                    let _ = crate::app::mouse::apply_scrollbar_drag_target(
                        &mut self.ide_panel.problems_scroll,
                        target,
                        drag_offset,
                    );
                    self.window.as_ref().unwrap().request_redraw();
                    return true;
                }
            }

            if self.is_ide_mode {
                let s = self.renderer.as_ref().unwrap().scale_factor;
                let sb_w = 48.0 * s;
                let panel_left_w = self.ide_panel.visible_left_width(s);
                let panel_bottom_h = if self.ide_panel.any_bottom_open() {
                    self.ide_panel.bottom_height * s
                } else {
                    0.0
                };
                let wh = self.window.as_ref().unwrap().inner_size().height as f32;

                let resize_bottom_limit = if panel_bottom_h > 0.0
                    && self.ide_panel.bottom_panel_blocks_editor_hover()
                {
                    crate::render_view::ide_bottom_panel_y(wh, panel_bottom_h, s)
                } else {
                    wh
                };

                let mut manual_resize = false;
                if panel_left_w > 0.0 {
                    let resize_x = sb_w + panel_left_w;
                    if (mx - resize_x).abs() < 3.0 * s && my >= 0.0 && my < resize_bottom_limit
                    {
                        self.ide_panel.is_resizing_left = true;
                        manual_resize = true;
                    }
                }
                if panel_bottom_h > 0.0 && !manual_resize {
                    let resize_y =
                        crate::render_view::ide_bottom_panel_y(wh, panel_bottom_h, s);
                    if (my - resize_y).abs() < 6.0 * s && mx >= sb_w {
                        self.ide_panel.is_resizing_bottom = true;
                        manual_resize = true;
                    }
                }

                if manual_resize {
                    self.window.as_ref().unwrap().request_redraw();
                    return true;
                }
            }

            let clicked_id = self.ui_registry.find_at(mx, my);
            if state == ElementState::Pressed
                && button == winit::event::MouseButton::Left
                && !matches!(
                    clicked_id,
                    Some(
                        crate::ui_system::UiId::ApiOutputSchemaMenu(_)
                            | crate::ui_system::UiId::ApiOutputSchemaMenuItem(_, _)
                    )
                )
                && self.close_active_api_output_example_menu()
            {
                self.window.as_ref().unwrap().request_redraw();
                if clicked_id.is_none() {
                    return true;
                }
            }
            if self.inline_git_popup.is_some()
                && button == winit::event::MouseButton::Left
                && state == ElementState::Pressed
                && !matches!(
                    clicked_id,
                    Some(
                        crate::ui_system::UiId::InlineGitPanelBody
                            | crate::ui_system::UiId::InlineGitPrevHunk
                            | crate::ui_system::UiId::InlineGitNextHunk
                            | crate::ui_system::UiId::InlineGitRollbackHunk
                            | crate::ui_system::UiId::EditorGitHunk(_, _)
                    )
                )
            {
                self.inline_git_popup = None;
                self.inline_git_diff_rx = None;
                self.window.as_ref().unwrap().request_redraw();
                return true;
            }
            let in_graph_tooltip_body = self
                .renderer
                .as_ref()
                .and_then(|renderer| renderer.git_graph_tooltip_hover)
                .is_some_and(|hover| hover.contains(mx, my));
            if in_graph_tooltip_body
                && button == winit::event::MouseButton::Left
                && state == ElementState::Pressed
                && !matches!(
                    clicked_id,
                    Some(crate::ui_system::UiId::GitGraphCopyCommit(_, _))
                        | Some(crate::ui_system::UiId::GitGraphOpenCommit(_, _))
                )
            {
                if let Some(renderer) = self.renderer.as_mut() {
                    let byte = renderer.git_graph_tooltip_byte_at(mx, my);
                    renderer.git_graph_tooltip_selection_anchor = Some(byte);
                    renderer.git_graph_tooltip_selection_cursor = Some(byte);
                    renderer.git_graph_tooltip_selecting = true;
                }
                self.window.as_ref().unwrap().request_redraw();
                return true;
            }
            if button == winit::event::MouseButton::Left
                && state == ElementState::Pressed
                && (self.ide_panel.api.import_url_open || self.ide_panel.api.import_menu_open)
                && !matches!(
                    clicked_id,
                    Some(
                        crate::ui_system::UiId::ApiImportAdd
                            | crate::ui_system::UiId::ApiImportFile
                            | crate::ui_system::UiId::ApiImportUrl
                            | crate::ui_system::UiId::ApiImportUrlInput
                            | crate::ui_system::UiId::ApiImportUrlConfirm
                    )
                )
            {
                if matches!(
                    self.ide_panel.api.focused,
                    Some(crate::app::api_client::ApiFocus::ImportUrl)
                ) {
                    self.commit_api_focus();
                    self.ide_panel.api.focused = None;
                }
                self.ide_panel.api.import_url_open = false;
                self.ide_panel.api.import_menu_open = false;
                self.window.as_ref().unwrap().request_redraw();
                if clicked_id.is_none() {
                    return true;
                }
            }
            if let Some(clicked_id) = clicked_id {
                self.handle_ui_element_press(clicked_id, state, button, mx, my);
                return true;
            }
        }
        false
    }
}
