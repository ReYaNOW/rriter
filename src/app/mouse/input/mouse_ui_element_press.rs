// Press on a concrete UI registry element: markdown scrollbars and selection,
// hover popup body, terminal focus, sidebar / tab drag start, panel scrollbars.
use super::*;

impl App {
    /// Handles a press on the UI registry element `clicked_id`; the caller always consumes the event afterwards.
    #[cfg_attr(coverage_nightly, coverage(off))]
    #[inline]
    pub(super) fn handle_ui_element_press(
        &mut self,
        clicked_id: crate::ui_system::UiId,
        state: ElementState,
        button: winit::event::MouseButton,
        mx: f32,
        my: f32,
    ) {
        if let crate::ui_system::UiId::MarkdownCodeScrollbarX(block_id) = clicked_id
            && button == winit::event::MouseButton::Left
        {
            if state == ElementState::Pressed {
                self.focus_document_text_surface();
                let _ = self.begin_markdown_code_scrollbar_drag_at(block_id, mx);
            }
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            return;
        }
        if clicked_id == crate::ui_system::UiId::MarkdownReadScrollbar
            && button == winit::event::MouseButton::Left
        {
            if state == ElementState::Pressed {
                self.focus_document_text_surface();
                let _ = self.begin_markdown_read_scrollbar_drag_at(my);
            }
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            return;
        }
        if clicked_id == crate::ui_system::UiId::MarkdownReadBody
            && button == winit::event::MouseButton::Left
        {
            if state == ElementState::Pressed {
                self.focus_document_text_surface();
                let _ = self.begin_markdown_read_selection_at(mx, my);
            }
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            return;
        }
        if clicked_id == crate::ui_system::UiId::GitWorkspaceScroll
            && button == winit::event::MouseButton::Left
        {
            if state == ElementState::Pressed {
                let geometry = self.renderer.as_ref().and_then(|renderer| {
                    let mut bar = renderer.git_workspace_scrollbar?;
                    bar.extent.offset = self.ide_panel.git.scroll.current;
                    bar.geometry(renderer.scale_factor)
                });
                let _ = crate::app::mouse::press_scrollbar(
                    &mut self.ide_panel.git.scroll,
                    geometry,
                    mx,
                    my,
                );
            }
            self.handle_ui_click(clicked_id);
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            return;
        }
        if let crate::ui_system::UiId::ApiMockCombinedScrollY(route_idx) = clicked_id
            && button == winit::event::MouseButton::Left
        {
            if state == ElementState::Pressed {
                let geometry = self
                    .ui_registry
                    .rect_for(clicked_id)
                    .and_then(|lane| {
                        let renderer = self.renderer.as_ref()?;
                        let viewport = self
                            .ui_registry
                            .rect_for(crate::ui_system::UiId::ApiMockCombinedPython(route_idx))?;
                        let scale = renderer.scale_factor;
                        let max_scroll = self.ide_panel.api.api_mock_combined_max_scroll_for_route(
                            self.api_active_route().as_ref(),
                            route_idx,
                            scale,
                        );
                        crate::app::api_client::api_mock_combined_editor_scrollbar(
                            lane,
                            viewport.3,
                            viewport.3 + max_scroll,
                            self.ide_panel
                                .api
                                .mock_python_scrolls
                                .get(&(route_idx, crate::app::api_mock::ty_check::ApiMockSourcePart::Body))
                                .map_or(0.0, |scroll| scroll.current),
                        )
                        .geometry(scale)
                    });
                let scroll = self
                    .ide_panel
                    .api
                    .mock_python_scrolls
                    .entry((route_idx, crate::app::api_mock::ty_check::ApiMockSourcePart::Body))
                    .or_insert_with(|| crate::scroll::ScrollState::new(7.0));
                let _ = crate::app::mouse::press_scrollbar(scroll, geometry, mx, my);
            }
            self.handle_ui_click(clicked_id);
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            return;
        }
        if matches!(
            clicked_id,
            crate::ui_system::UiId::EditorTab(_)
                | crate::ui_system::UiId::EditorTabClose(_)
        ) && let Some(renderer) = self.renderer.as_mut()
        {
            renderer.suppress_popups_until_next_mouse_move();
            renderer.reset_delayed_tooltip_anchor();
        }

        let in_hover_popup_body = clicked_id == crate::ui_system::UiId::BottomPanelBody
            && if let Some((x, y, w, h)) = self.hover.rect {
                mx >= x && mx <= x + w && my >= y && my <= y + h
            } else {
                false
            };
        let in_diag_popup_body = clicked_id == crate::ui_system::UiId::BottomPanelBody
            && self.hover.diag_rect.map_or(false, |(x, y, w, h, _, _, _)| {
                mx >= x && mx <= x + w && my >= y && my <= y + h
            });

        if in_hover_popup_body || in_diag_popup_body {
            if button == winit::event::MouseButton::Left {
                if in_hover_popup_body {
                    let hs = &mut self.hover;
                    if let (Some(rect), Some(popup), Some(renderer)) =
                        (hs.rect, hs.popup.as_ref(), self.renderer.as_mut())
                    {
                        let byte = hover_popup_byte_at(renderer, popup, rect, mx, my);
                        if state == ElementState::Pressed {
                            hs.selection_anchor = Some(byte);
                            hs.selection_cursor = Some(byte);
                            hs.selecting = true;
                        } else {
                            hs.selecting = false;
                        }
                    }
                } else {
                    let hs = &mut self.hover;
                    let byte = crate::render_view::ui::diag_popup_byte_at(mx, my);
                    if state == ElementState::Pressed {
                        hs.diag_selection_anchor = Some(byte);
                        hs.diag_selection_cursor = Some(byte);
                        hs.diag_selecting = true;
                    } else {
                        hs.diag_selecting = false;
                    }
                }
                self.window.as_ref().unwrap().request_redraw();
            }
            return;
        } else if clicked_id == crate::ui_system::UiId::BottomPanelBody {
            self.handle_ui_click(clicked_id);
            return;
        }
        if clicked_id == crate::ui_system::UiId::HoverPopupScroll {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            let state = &mut self.hover;
            if let Some(rect) = state.rect {
                let max_scroll = state.max_scroll;
                if let Some(popup) = &mut state.popup {
                    let geometry = crate::app::mouse::hover_popup_scrollbar(
                        rect,
                        max_scroll,
                        popup.scroll.current,
                        s,
                    )
                    .geometry(s);
                    let _ = crate::app::mouse::press_scrollbar(
                        &mut popup.scroll,
                        geometry,
                        mx,
                        my,
                    );
                }
            }
            self.window.as_ref().unwrap().request_redraw();
            return;
        }
        let is_term = matches!(
            clicked_id,
            crate::ui_system::UiId::TerminalBody
                | crate::ui_system::UiId::TerminalScrollY
                | crate::ui_system::UiId::TerminalTab(_)
                | crate::ui_system::UiId::TerminalTabClose(_)
                | crate::ui_system::UiId::TerminalAdd
                | crate::ui_system::UiId::TerminalSearchInput
                | crate::ui_system::UiId::TerminalSearchClose
                | crate::ui_system::UiId::TerminalSearchNext
                | crate::ui_system::UiId::TerminalSearchPrev
                | crate::ui_system::UiId::TerminalSearchCaseToggle
        );
        let is_resize = matches!(
            clicked_id,
            crate::ui_system::UiId::ResizeLeft
                | crate::ui_system::UiId::ResizeBottom
                | crate::ui_system::UiId::GitGraphResize
        );

        if is_term {
            self.ide_panel.terminal_focused = true;
        } else if !is_resize {
            self.ide_panel.terminal_focused = false;
        }

        if let crate::ui_system::UiId::SidebarSlot(panel_id) = clicked_id {
            self.ide_panel.drag = Some(crate::app::PanelDragState {
                panel_id,
                start_y: my,
                current_y: my,
                threshold_passed: false,
            });
        } else if let crate::ui_system::UiId::EditorTab(idx) = clicked_id {
            self.ide_panel.terminal_tab_drag = None;
            self.handle_ui_click(clicked_id);
            self.ide_panel.tab_drag = Some(crate::app::TabDragState {
                start_idx: idx,
                start_x: mx,
                current_x: mx,
                threshold_passed: false,
            });
        } else if let crate::ui_system::UiId::TerminalTab(idx) = clicked_id {
            self.ide_panel.tab_drag = None;
            self.ide_panel.terminal_tab_drag = Some(crate::app::TabDragState {
                start_idx: idx,
                start_x: mx,
                current_x: mx,
                threshold_passed: false,
            });
            self.handle_ui_click(clicked_id);
        } else if clicked_id == crate::ui_system::UiId::GitGraphResize {
            self.ide_panel.git.graph_resizing = true;
            self.handle_ui_click(clicked_id);
        } else if clicked_id == crate::ui_system::UiId::FileTreeScrollY {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            let geometry = super::explorer_scrollbar_geometry(self, s);
            let _ = crate::app::mouse::press_scrollbar(
                &mut self.ide_panel.explorer_scroll,
                geometry,
                mx,
                my,
            );
            self.handle_ui_click(clicked_id);
        } else if clicked_id == crate::ui_system::UiId::GitLogsScroll {
            if state == ElementState::Pressed
                && button == winit::event::MouseButton::Left
                && let Some((metrics, s)) = self.renderer.as_ref().and_then(|renderer| {
                    renderer
                        .git_logs_layout_metrics()
                        .map(|metrics| (metrics, renderer.scale_factor))
                })
            {
                let scroll = &mut self.ide_panel.git.logs_scroll;
                let geometry = metrics.scrollbar(scroll.current).geometry(s);
                if crate::app::mouse::press_scrollbar(scroll, geometry, mx, my).is_some() {
                    self.ide_panel
                        .git
                        .refresh_git_logs_follow_tail(metrics.max_scroll);
                }
            }
            self.handle_ui_click(clicked_id);
        } else if clicked_id == crate::ui_system::UiId::GitLogsBody
            && button == winit::event::MouseButton::Left
        {
            if state == ElementState::Pressed {
                let point = {
                    let logs = &self.ide_panel.git.git_logs;
                    self.renderer.as_mut().and_then(|renderer| {
                        renderer.git_logs_text_point_at(logs, mx, my)
                    })
                };
                if let Some(point) = point {
                    let selected = begin_git_logs_text_selection(self, point);
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.git_logs_selecting = selected;
                    }
                }
            }
            self.handle_ui_click(clicked_id);
        } else if clicked_id == crate::ui_system::UiId::GitGraphScroll {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            let geometry = super::git_graph_scrollbar_geometry(self, s);
            let max_scroll = geometry.map_or(0.0, |g| g.max_scroll);
            if let Some(target) = crate::app::mouse::press_scrollbar(
                &mut self.ide_panel.git.graph_scroll,
                geometry,
                mx,
                my,
            ) && self.ide_panel.git.graph_has_more
                && crate::app::git_panel::git_graph_near_load_more(target, max_scroll, s)
            {
                self.load_more_git_graph_commits();
            }
            self.handle_ui_click(clicked_id);
        } else if clicked_id == crate::ui_system::UiId::TerminalScrollY {
            let geometry = active_terminal_scrollbar_geometry(self);
            let active = self.ide_panel.active_terminal;
            if let Some(term) = self.ide_panel.terminals.get_mut(active) {
                let _ = crate::app::mouse::press_scrollbar(&mut term.scroll_y, geometry, mx, my);
            }
            self.handle_ui_click(clicked_id);
        } else {
            if clicked_id == crate::ui_system::UiId::ProjectSearchQueryScrollbarY {
                if self.start_project_search_query_scrollbar_drag(
                    crate::app::project_search::ProjectSearchQueryScrollAxis::Vertical,
                    my,
                ) {
                    self.window.as_ref().unwrap().request_redraw();
                }
                return;
            }
            if clicked_id == crate::ui_system::UiId::ProjectSearchQueryScrollbarX {
                if self.start_project_search_query_scrollbar_drag(
                    crate::app::project_search::ProjectSearchQueryScrollAxis::Horizontal,
                    mx,
                ) {
                    self.window.as_ref().unwrap().request_redraw();
                }
                return;
            }
            if clicked_id == crate::ui_system::UiId::ProjectSearchScrollbar {
                if self.start_project_search_scrollbar_drag(my) {
                    let _ = self.queue_visible_project_search_previews();
                    self.window.as_ref().unwrap().request_redraw();
                }
                return;
            }
            if let Some(field) =
                crate::app::project_search_app::project_search_field_for_ui_id(
                    clicked_id,
                )
            {
                if field != crate::app::project_search::ProjectSearchField::Filter
                    || self.ide_panel.project_search.filter_enabled()
                {
                    self.ide_panel.project_search.dragging_field = Some(field);
                }
            }
            if clicked_id == crate::ui_system::UiId::TerminalBody {
                self.ide_panel.is_dragging_terminal = true;
            } else {
                self.ide_panel.is_dragging_terminal = false;
            }
            self.handle_ui_click(clicked_id);
        }
    }
}
