pub(crate) fn native_picker_can_start<T>(
    receiver: &Option<std::sync::mpsc::Receiver<T>>,
) -> bool {
    crate::platform::receiver_slot_available(receiver)
}

impl crate::app::App {
    fn active_api_route_text(&self, field: ApiRouteTextField) -> Option<String> {
        let (meta, state) = self.active_api_tab()?;
        self.ide_panel.api.api_route_text_for_tab(
            meta.spec_id,
            state.route_idx,
            meta.route_identity.as_ref(),
            field,
        )
    }

    pub(crate) fn begin_api_route_text_selection(
        &mut self,
        field: ApiRouteTextField,
        route_idx: usize,
    ) -> bool {
        let Some(text) = self.active_api_route_text(field) else {
            return false;
        };
        let id = ApiClientState::api_route_text_ui_id(field, route_idx);
        let Some(rect) = self.ui_registry.rect_for(id) else {
            return false;
        };
        let (mx, my, scale) = self
            .renderer
            .as_ref()
            .map(|renderer| {
                (
                    renderer.last_mouse_x,
                    renderer.last_mouse_y,
                    renderer.scale_factor,
                )
            })
            .unwrap_or((0.0, 0.0, 1.0));
        let Some(byte) = self.renderer.as_mut().map(|renderer| {
            renderer.api_route_text_byte_at(field, &text, rect, mx, my, scale)
        }) else {
            return false;
        };
        let Some(spec_id) = self.active_api_tab().map(|(meta, _)| meta.spec_id) else {
            return false;
        };
        let Some((_, state)) = self.active_api_tab_mut_for(spec_id) else {
            return false;
        };
        if !state.begin_route_text_selection(route_idx, field, byte) {
            return false;
        }
        self.is_dragging = false;
        self.is_editor_drag_pending = false;
        self.ide_panel.is_dragging_terminal = false;
        true
    }

    pub(crate) fn drag_api_route_text_selection_from_last_mouse(&mut self) -> bool {
        let Some((route_idx, selection)) = self.active_api_tab().and_then(|(_, state)| {
            let route_idx = state.route_idx?;
            let selection = state.route_text_selection?;
            selection.selecting.then_some((route_idx, selection))
        }) else {
            return false;
        };
        let Some(text) = self.active_api_route_text(selection.field) else {
            return false;
        };
        let id = ApiClientState::api_route_text_ui_id(selection.field, route_idx);
        let Some(rect) = self.ui_registry.rect_for(id) else {
            return false;
        };
        let (mx, my, scale) = self
            .renderer
            .as_ref()
            .map(|renderer| {
                (
                    renderer.last_mouse_x,
                    renderer.last_mouse_y,
                    renderer.scale_factor,
                )
            })
            .unwrap_or((0.0, 0.0, 1.0));
        let Some(byte) = self.renderer.as_mut().map(|renderer| {
            renderer.api_route_text_byte_at(selection.field, &text, rect, mx, my, scale)
        }) else {
            return false;
        };
        let spec_id = self.active_api_tab().map(|(meta, _)| meta.spec_id);
        if let Some(spec_id) = spec_id
            && let Some((_, state)) = self.active_api_tab_mut_for(spec_id)
        {
            state.drag_route_text_selection(byte);
        }
        true
    }

    pub(crate) fn finish_api_route_text_selection(&mut self) -> bool {
        let Some(spec_id) = self.active_api_tab().map(|(meta, _)| meta.spec_id) else {
            return false;
        };
        let Some((_, state)) = self.active_api_tab_mut_for(spec_id) else {
            return false;
        };
        state.finish_route_text_selection()
    }

    pub(crate) fn copy_api_route_text_selection(&mut self) -> bool {
        let Some((_, state)) = self.active_api_tab() else {
            return false;
        };
        let Some(selection) = state.route_text_selection else {
            return false;
        };
        let Some(text) = self.active_api_route_text(selection.field) else {
            return false;
        };
        let Some(selected) = state.selected_route_text(&text) else {
            return false;
        };
        self.set_clipboard_text(selected.to_string());
        true
    }

    fn pulse_api_cursor_blink(&mut self) {
        self.last_action = std::time::Instant::now();
        self.last_blink_state = true;
    }

    fn api_text_scroll_for_ui(&self, id: crate::ui_system::UiId) -> f32 {
        let Some((_, state)) = self.active_api_tab() else {
            return 0.0;
        };
        ApiClientState::api_tab_scroll_for_ui(state, id)
    }

    fn api_text_scroll_x_for_ui(&self, id: crate::ui_system::UiId) -> f32 {
        let Some((_, state)) = self.active_api_tab() else {
            return 0.0;
        };
        if ApiClientState::api_mock_part_for_ui(id).is_some() {
            self.ide_panel.api.api_mock_scroll_x_for_ui(id)
        } else {
            ApiClientState::api_tab_scroll_x_for_ui(state, id)
        }
    }

    pub(crate) fn api_multiline_text_for_ui(
        &self,
        id: crate::ui_system::UiId,
    ) -> Option<String> {
        let (meta, state) = self.active_api_tab()?;
        let needs_active_route = matches!(
            id,
            crate::ui_system::UiId::ApiInputSchemaBody(_)
                | crate::ui_system::UiId::ApiMockStaticResponseScrollY(_)
                | crate::ui_system::UiId::ApiMockStaticResponseScrollX(_)
                | crate::ui_system::UiId::ApiMockStaticResponseInput(_)
                | crate::ui_system::UiId::ApiMockPreludeInput(_)
                | crate::ui_system::UiId::ApiMockContractInput(_)
                | crate::ui_system::UiId::ApiMockBodyInput(_)
                | crate::ui_system::UiId::ApiMockSignatureInput(_)
        );
        let active = needs_active_route.then(|| self.api_active_route()).flatten();
        self.ide_panel.api.api_multiline_text_for_ui(
            id,
            meta.spec_id,
            state,
            self.ide_panel.api.focused.as_ref(),
            &self.ide_panel.api.input_editor,
            active.as_ref(),
        )
    }

    pub(crate) fn api_text_max_scroll_x_for_ui(&mut self, id: crate::ui_system::UiId) -> f32 {
        let Some(rect) = self.ui_registry.rect_for(id) else {
            return 0.0;
        };
        let Some(text) = self.api_multiline_text_for_ui(id) else {
            return 0.0;
        };
        let visible_w = (rect.2
            - 20.0
                * self
                    .renderer
                    .as_ref()
                    .map(|r| r.scale_factor)
                    .unwrap_or(1.0))
        .max(1.0);
        let Some(renderer) = self.renderer.as_mut() else {
            return 0.0;
        };
        let use_mono_width = ApiClientState::api_mock_part_for_ui(id).is_some();
        api_text_area_max_scroll_x(&text, visible_w, |line| {
            if use_mono_width {
                line.chars().map(|ch| renderer.char_advance(ch)).sum()
            } else {
                renderer.measure_ui_width(line, API_BODY_TEXT_SCALE)
            }
        })
    }

    pub(crate) fn api_text_max_scroll_y_for_ui(&mut self, id: crate::ui_system::UiId) -> f32 {
        let Some(rect) = self.ui_registry.rect_for(id) else {
            return 0.0;
        };
        let Some(text) = self.api_multiline_text_for_ui(id) else {
            return 0.0;
        };
        let scale = self
            .renderer
            .as_ref()
            .map(|renderer| renderer.scale_factor)
            .unwrap_or(1.0);
        let schema_body_id = match id {
            crate::ui_system::UiId::ApiBodyScrollY(route_idx)
            | crate::ui_system::UiId::ApiInputSchemaBody(route_idx)
                if self
                    .active_api_tab()
                    .is_some_and(|(_, state)| state.input_doc_view == ApiInputDocView::Schema) =>
            {
                Some(crate::ui_system::UiId::ApiInputSchemaBody(route_idx))
            }
            crate::ui_system::UiId::ApiOutputScrollY(route_idx)
            | crate::ui_system::UiId::ApiOutputSchemaBody(route_idx)
                if self
                    .active_api_tab()
                    .is_some_and(|(_, state)| state.output_doc_view == ApiOutputDocView::Schema) =>
            {
                Some(crate::ui_system::UiId::ApiOutputSchemaBody(route_idx))
            }
            _ => None,
        };
        if let Some(body_id) = schema_body_id {
            let Some(body_rect) = self.ui_registry.rect_for(body_id) else {
                return 0.0;
            };
            return self
                .renderer
                .as_mut()
                .map(|renderer| {
                    renderer.api_schema_text_max_scroll(
                        &text,
                        (body_rect.2 - 20.0 * scale).max(1.0),
                        rect.3,
                        scale,
                    )
                })
                .unwrap_or(0.0);
        }
        api_text_area_max_scroll(&text, rect.3, scale)
    }

    fn api_one_line_max_scroll_x_for_ui(&mut self, id: crate::ui_system::UiId) -> f32 {
        let Some(rect) = self.ui_registry.rect_for(id) else {
            return 0.0;
        };
        let scale = self
            .renderer
            .as_ref()
            .map(|renderer| renderer.scale_factor)
            .unwrap_or(1.0);
        let visible_w = (rect.2 - 16.0 * scale).max(1.0);
        let text = self.ide_panel.api.input_editor.get_full_text();
        let Some(renderer) = self.renderer.as_mut() else {
            return 0.0;
        };
        crate::app::one_line_input_max_scroll_x(
            renderer,
            &text,
            visible_w,
            ApiClientState::api_one_line_text_scale_for_ui(id),
            20.0 * scale,
        )
    }

    fn sync_api_one_line_scroll_target(&mut self, immediate: bool) {
        let Some(focus) = self.ide_panel.api.focused.clone() else {
            return;
        };
        if self.ide_panel.api.api_focus_is_array_input(&focus) {
            return;
        }
        let Some((id, false)) = self.ide_panel.api.api_focus_ui_target(&focus) else {
            return;
        };
        let Some(rect) = self.ui_registry.rect_for(id) else {
            return;
        };
        let scale = self
            .renderer
            .as_ref()
            .map(|renderer| renderer.scale_factor)
            .unwrap_or(1.0);
        let visible_w = (rect.2 - 16.0 * scale).max(1.0);
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        crate::app::sync_one_line_input_scroll_target(
            renderer,
            &self.ide_panel.api.input_editor,
            &mut self.ide_panel.api.input_scroll_x,
            visible_w,
            ApiClientState::api_one_line_text_scale_for_ui(id),
            10.0 * scale,
            immediate,
        );
    }

    fn sync_api_multiline_scroll_target(&mut self, id: crate::ui_system::UiId, immediate: bool) {
        let Some(rect) = self.ui_registry.rect_for(id) else {
            return;
        };
        let scale = self
            .renderer
            .as_ref()
            .map(|renderer| renderer.scale_factor)
            .unwrap_or(1.0);
        let text = self.ide_panel.api.input_editor.get_full_text();
        let cursor = self.ide_panel.api.input_editor.cursor.min(text.len());
        let line_h = api_text_area_line_height(scale);
        let visible_h = (rect.3 - 16.0 * scale).max(line_h);
        let visible_w = (rect.2 - 20.0 * scale).max(1.0);
        let cursor_line = text[..cursor].bytes().filter(|byte| *byte == b'\n').count();
        let line_start = text[..cursor]
            .rfind('\n')
            .map(|idx| idx.saturating_add(1))
            .unwrap_or(0);
        let cursor_line_text = &text[line_start..cursor];
        let mock_part = ApiClientState::api_mock_part_for_ui(id);
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let cursor_x = if mock_part.is_some() {
            cursor_line_text
                .chars()
                .map(|ch| renderer.char_advance(ch))
                .sum()
        } else {
            renderer.measure_ui_width(cursor_line_text, API_BODY_TEXT_SCALE)
        };
        let max_scroll_x = api_text_area_max_scroll_x(&text, visible_w, |line| {
            if mock_part.is_some() {
                line.chars().map(|ch| renderer.char_advance(ch)).sum()
            } else {
                renderer.measure_ui_width(line, API_BODY_TEXT_SCALE)
            }
        });
        let max_scroll_y = api_text_area_max_scroll(&text, visible_h, scale);

        let cursor_y = cursor_line as f32 * line_h;
        let Some((meta, _)) = self.active_api_tab() else {
            return;
        };
        let spec_id = meta.spec_id;
        let mut scroll_y_current = self.api_text_scroll_for_ui(id);
        let mut scroll_x_current = self.api_text_scroll_x_for_ui(id);
        let edge = 10.0 * scale;
        if cursor_y + line_h - scroll_y_current > visible_h {
            scroll_y_current = cursor_y + line_h - visible_h + edge;
        } else if cursor_y < scroll_y_current {
            scroll_y_current = cursor_y;
        }
        if cursor_x - scroll_x_current > visible_w {
            scroll_x_current = cursor_x - visible_w + edge;
        } else if cursor_x < scroll_x_current {
            scroll_x_current = cursor_x;
        }
        let target_y = scroll_y_current.clamp(0.0, max_scroll_y);
        let target_x = scroll_x_current.clamp(0.0, max_scroll_x);

        if let Some((_, state)) = self.active_api_tab_mut_for(spec_id)
            && state.sync_multiline_scroll_target(id, target_y, target_x, immediate)
        {
            return;
        }
        if let Some(key @ (route_idx, _)) = mock_part {
            let scroll_x = self
                .ide_panel
                .api
                .mock_python_scrolls_x
                .entry(key)
                .or_insert_with(|| ScrollState::new(7.0));
            scroll_x.animate_to(target_x);
            if immediate {
                scroll_x.jump_to(target_x);
            }
            if let Some(viewport) = self
                .ui_registry
                .rect_for(crate::ui_system::UiId::ApiMockCombinedPython(route_idx))
            {
                let active = self.api_active_route();
                let max_scroll = self.ide_panel.api.api_mock_combined_max_scroll_for_route(
                    active.as_ref(),
                    route_idx,
                    scale,
                );
                let text_top_y = ApiClientState::api_multiline_cursor_top_y(id, rect, scale);
                let cursor_top = text_top_y + cursor_y;
                let cursor_bottom = cursor_top + line_h;
                let top_limit = viewport.1 + edge;
                let bottom_limit = viewport.1 + viewport.3 - edge;
                let scroll_y = self
                    .ide_panel
                    .api
                    .mock_python_scrolls
                    .entry((route_idx, ApiMockSourcePart::Body))
                    .or_insert_with(|| ScrollState::new(7.0));
                if cursor_bottom > bottom_limit {
                    scroll_y.animate_to(scroll_y.current + cursor_bottom - bottom_limit);
                } else if cursor_top < top_limit {
                    scroll_y.animate_to(scroll_y.current - (top_limit - cursor_top));
                }
                scroll_y.target = scroll_y.target.clamp(0.0, max_scroll);
                if immediate {
                    scroll_y.jump_to(scroll_y.target);
                }
            }
        }
    }

    pub(crate) fn start_api_text_scrollbar_x_drag(
        &mut self,
        id: crate::ui_system::UiId,
    ) -> bool {
        let route_idx = match id {
            crate::ui_system::UiId::ApiBodyScrollX(route_idx)
            | crate::ui_system::UiId::ApiOutputScrollX(route_idx)
            | crate::ui_system::UiId::ApiMockStaticResponseScrollX(route_idx)
            | crate::ui_system::UiId::ApiResponseScrollX(route_idx) => route_idx,
            _ => return false,
        };
        let Some(rect) = self.ui_registry.rect_for(id) else {
            return false;
        };
        let max_scroll = self.api_text_max_scroll_x_for_ui(id);
        let Some((meta, state)) = self.active_api_tab() else {
            return false;
        };
        if state.route_idx != Some(route_idx) {
            return false;
        }
        let spec_id = meta.spec_id;
        let mx = self
            .renderer
            .as_ref()
            .map(|renderer| renderer.last_mouse_x)
            .unwrap_or(0.0);
        let scale = self
            .renderer
            .as_ref()
            .map(|renderer| renderer.scale_factor)
            .unwrap_or(1.0);
        let Some((_, state)) = self.active_api_tab_mut_for(spec_id) else {
            return false;
        };
        state.drag_text_scrollbar(id, route_idx, rect, max_scroll, mx, scale, None)
    }

    pub(crate) fn drag_api_text_scrollbar_x_from_last_mouse(&mut self) -> bool {
        let Some(id) = self.active_api_tab().and_then(|(_, state)| {
            let route_idx = state.route_idx?;
            if state.body_scroll_x.is_dragging {
                Some(crate::ui_system::UiId::ApiBodyScrollX(route_idx))
            } else if state.output_scroll_x.is_dragging {
                Some(crate::ui_system::UiId::ApiOutputScrollX(route_idx))
            } else if state.mock_static_response_scroll_x.is_dragging {
                Some(crate::ui_system::UiId::ApiMockStaticResponseScrollX(
                    route_idx,
                ))
            } else if state.response_scroll_x.is_dragging {
                Some(crate::ui_system::UiId::ApiResponseScrollX(route_idx))
            } else {
                None
            }
        }) else {
            return false;
        };
        let Some(rect) = self.ui_registry.rect_for(id) else {
            return false;
        };
        let max_scroll = self.api_text_max_scroll_x_for_ui(id);
        let Some((meta, _)) = self.active_api_tab() else {
            return false;
        };
        let spec_id = meta.spec_id;
        let mx = self
            .renderer
            .as_ref()
            .map(|renderer| renderer.last_mouse_x)
            .unwrap_or(0.0);
        let scale = self
            .renderer
            .as_ref()
            .map(|renderer| renderer.scale_factor)
            .unwrap_or(1.0);
        if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
            let route_idx = match id {
                crate::ui_system::UiId::ApiBodyScrollX(route_idx)
                | crate::ui_system::UiId::ApiOutputScrollX(route_idx)
                | crate::ui_system::UiId::ApiMockStaticResponseScrollX(route_idx)
                | crate::ui_system::UiId::ApiResponseScrollX(route_idx) => route_idx,
                _ => return false,
            };
            let Some(drag_offset) = state.scrollbar_drag_offset(id, route_idx) else {
                return false;
            };
            if !state.drag_text_scrollbar(
                id,
                route_idx,
                rect,
                max_scroll,
                mx,
                scale,
                Some(drag_offset),
            ) {
                return false;
            }
        }
        true
    }

    pub(crate) fn start_api_text_scrollbar_y_drag(
        &mut self,
        id: crate::ui_system::UiId,
    ) -> bool {
        let route_idx = match id {
            crate::ui_system::UiId::ApiBodyScrollY(route_idx)
            | crate::ui_system::UiId::ApiOutputScrollY(route_idx)
            | crate::ui_system::UiId::ApiMockStaticResponseScrollY(route_idx)
            | crate::ui_system::UiId::ApiResponseScrollY(route_idx) => route_idx,
            _ => return false,
        };
        let Some(rect) = self.ui_registry.rect_for(id) else {
            return false;
        };
        let max_scroll = self.api_text_max_scroll_y_for_ui(id);
        let Some((meta, state)) = self.active_api_tab() else {
            return false;
        };
        if state.route_idx != Some(route_idx) {
            return false;
        }
        let spec_id = meta.spec_id;
        let pointer_y = self
            .renderer
            .as_ref()
            .map(|renderer| renderer.last_mouse_y)
            .unwrap_or(rect.1);
        let scale = self
            .renderer
            .as_ref()
            .map(|renderer| renderer.scale_factor)
            .unwrap_or(1.0);
        let Some((_, state)) = self.active_api_tab_mut_for(spec_id) else {
            return false;
        };
        state.drag_text_scrollbar(id, route_idx, rect, max_scroll, pointer_y, scale, None)
    }

    pub(crate) fn drag_api_text_scrollbar_y_from_last_mouse(&mut self) -> bool {
        let Some(id) = self.active_api_tab().and_then(|(_, state)| {
            let route_idx = state.route_idx?;
            if state.body_scroll.is_dragging {
                Some(crate::ui_system::UiId::ApiBodyScrollY(route_idx))
            } else if state.output_scroll.is_dragging {
                Some(crate::ui_system::UiId::ApiOutputScrollY(route_idx))
            } else if state.mock_static_response_scroll.is_dragging {
                Some(crate::ui_system::UiId::ApiMockStaticResponseScrollY(
                    route_idx,
                ))
            } else if state.response_scroll.is_dragging {
                Some(crate::ui_system::UiId::ApiResponseScrollY(route_idx))
            } else {
                None
            }
        }) else {
            return false;
        };
        let Some(rect) = self.ui_registry.rect_for(id) else {
            return false;
        };
        let max_scroll = self.api_text_max_scroll_y_for_ui(id);
        let Some((meta, _)) = self.active_api_tab() else {
            return false;
        };
        let spec_id = meta.spec_id;
        let pointer_y = self
            .renderer
            .as_ref()
            .map(|renderer| renderer.last_mouse_y)
            .unwrap_or(rect.1);
        let scale = self
            .renderer
            .as_ref()
            .map(|renderer| renderer.scale_factor)
            .unwrap_or(1.0);
        if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
            let route_idx = match id {
                crate::ui_system::UiId::ApiBodyScrollY(route_idx)
                | crate::ui_system::UiId::ApiOutputScrollY(route_idx)
                | crate::ui_system::UiId::ApiMockStaticResponseScrollY(route_idx)
                | crate::ui_system::UiId::ApiResponseScrollY(route_idx) => route_idx,
                _ => return false,
            };
            let Some(drag_offset) = state.scrollbar_drag_offset(id, route_idx) else {
                return false;
            };
            if !state.drag_text_scrollbar(
                id,
                route_idx,
                rect,
                max_scroll,
                pointer_y,
                scale,
                Some(drag_offset),
            ) {
                return false;
            }
        }
        true
    }

    fn place_api_cursor_from_last_click(
        &mut self,
        id: crate::ui_system::UiId,
        multiline: bool,
        same_click_target: bool,
    ) {
        let Some(rect) = self.ui_registry.rect_for(id) else {
            return;
        };
        if self
            .ide_panel
            .api
            .focused
            .as_ref()
            .is_some_and(|focus| self.ide_panel.api.api_focus_is_array_input(focus))
        {
            self.ide_panel.api.set_input_cursor(self.ide_panel.api.input_editor.len(), false);
            self.pulse_api_cursor_blink();
            return;
        }
        let scroll_y = if multiline {
            self.api_text_scroll_for_ui(id)
        } else {
            0.0
        };
        let scroll_x = if multiline {
            self.api_text_scroll_x_for_ui(id)
        } else {
            self.ide_panel.api.input_scroll_x.current
        };
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let mx = renderer.last_mouse_x;
        let my = renderer.last_mouse_y;
        let scale = renderer.scale_factor;
        let ui_text_input = matches!(
            id,
            crate::ui_system::UiId::ApiInputSchemaBody(_)
                | crate::ui_system::UiId::ApiOutputSchemaBody(_)
                | crate::ui_system::UiId::ApiBodyInput(_)
                | crate::ui_system::UiId::ApiResponseBody(_)
                | crate::ui_system::UiId::ApiMockStaticResponseInput(_)
        );
        let cursor = if multiline && ui_text_input {
            api_multiline_ui_byte_at_pointer(
                &self.ide_panel.api.input_editor,
                renderer,
                ApiClientState::api_multiline_cursor_left_x(id, rect, scale),
                ApiClientState::api_multiline_cursor_top_y(id, rect, scale),
                mx,
                my,
                scale,
                scroll_y,
                scroll_x,
                API_BODY_TEXT_SCALE,
            )
        } else if multiline {
            set_api_multiline_cursor_at_pointer(
                &mut self.ide_panel.api.input_editor,
                renderer,
                ApiClientState::api_multiline_cursor_left_x(id, rect, scale),
                ApiClientState::api_multiline_cursor_top_y(id, rect, scale),
                mx,
                my,
                scale,
                scroll_y,
                scroll_x,
                true,
            );
            self.ide_panel.api.input_editor.cursor
        } else {
            let text = self.ide_panel.api.input_editor.get_full_text();
            let visible_w = (rect.2 - 16.0 * scale).max(0.0);
            let target_x = if mx <= rect.0 {
                scroll_x
            } else if mx >= rect.0 + rect.2 {
                scroll_x + visible_w
            } else {
                scroll_x + (mx - (rect.0 + 8.0 * scale)).clamp(0.0, visible_w)
            };
            api_line_byte_at_x(
                renderer,
                &text,
                target_x,
                ApiClientState::api_one_line_text_scale_for_ui(id),
            )
        };
        self.ide_panel.api.set_input_cursor(cursor, false);
        let now = std::time::Instant::now();
        let dx = mx - self.last_click_pos.0;
        let dy = my - self.last_click_pos.1;
        if crate::app::ui_handlers::repeated_ui_click(
            same_click_target,
            now.duration_since(self.last_click_time),
            dx,
            dy,
        ) {
            self.click_count = self.click_count.saturating_add(1);
        } else {
            self.click_count = 1;
        }
        self.last_click_time = now;
        self.last_click_pos = (mx, my);
        if self.click_count == 2 {
            self.ide_panel.api.input_editor.select_word();
        }
        if multiline {
            self.sync_api_multiline_scroll_target(id, true);
        } else {
            self.sync_api_one_line_scroll_target(true);
        }
        self.pulse_api_cursor_blink();
        self.ide_panel.api.queue_api_body_json_validation();
    }

    pub(crate) fn drag_api_text_cursor_from_last_mouse(&mut self) -> bool {
        let Some(focus) = self.ide_panel.api.focused.clone() else {
            return false;
        };
        let Some((id, multiline)) = self.ide_panel.api.api_focus_ui_target(&focus) else {
            return false;
        };
        let Some(rect) = self.ui_registry.rect_for(id) else {
            return false;
        };
        let scroll_y = self.api_text_scroll_for_ui(id);
        let scroll_x = if multiline {
            self.api_text_scroll_x_for_ui(id)
        } else {
            self.ide_panel.api.input_scroll_x.current
        };
        let Some(renderer) = self.renderer.as_mut() else {
            return false;
        };
        let mx = renderer.last_mouse_x;
        let my = renderer.last_mouse_y;
        let scale = renderer.scale_factor;
        let ui_text_input = matches!(
            id,
            crate::ui_system::UiId::ApiInputSchemaBody(_)
                | crate::ui_system::UiId::ApiOutputSchemaBody(_)
                | crate::ui_system::UiId::ApiBodyInput(_)
                | crate::ui_system::UiId::ApiResponseBody(_)
                | crate::ui_system::UiId::ApiMockStaticResponseInput(_)
        );
        let cursor = if multiline && ui_text_input {
            api_multiline_ui_byte_at_pointer(
                &self.ide_panel.api.input_editor,
                renderer,
                ApiClientState::api_multiline_cursor_left_x(id, rect, scale),
                ApiClientState::api_multiline_cursor_top_y(id, rect, scale),
                mx,
                my,
                scale,
                scroll_y,
                scroll_x,
                API_BODY_TEXT_SCALE,
            )
        } else if multiline {
            set_api_multiline_cursor_at_pointer(
                &mut self.ide_panel.api.input_editor,
                renderer,
                ApiClientState::api_multiline_cursor_left_x(id, rect, scale),
                ApiClientState::api_multiline_cursor_top_y(id, rect, scale),
                mx,
                my,
                scale,
                scroll_y,
                scroll_x,
                false,
            );
            self.ide_panel.api.input_editor.cursor
        } else {
            let text = self.ide_panel.api.input_editor.get_full_text();
            let visible_w = (rect.2 - 16.0 * scale).max(0.0);
            let target_x = if mx <= rect.0 {
                scroll_x
            } else if mx >= rect.0 + rect.2 {
                scroll_x + visible_w
            } else {
                scroll_x + (mx - (rect.0 + 8.0 * scale)).clamp(0.0, visible_w)
            };
            api_line_byte_at_x(
                renderer,
                &text,
                target_x,
                ApiClientState::api_one_line_text_scale_for_ui(id),
            )
        };
        self.ide_panel.api.set_input_cursor(cursor, true);
        if multiline {
            self.sync_api_multiline_scroll_target(id, false);
        } else {
            let max_scroll = self.api_one_line_max_scroll_x_for_ui(id);
            let edge = 18.0 * scale;
            let scroll = &mut self.ide_panel.api.input_scroll_x;
            scroll.anim_speed = 7.0;
            if mx < rect.0 + edge {
                scroll.scroll_by(-edge);
                scroll.clamp_target(0.0, max_scroll);
            } else if mx > rect.0 + rect.2 - edge {
                scroll.scroll_by(edge);
                scroll.clamp_target(0.0, max_scroll);
            }
        }
        self.pulse_api_cursor_blink();
        self.ide_panel.api.queue_api_body_json_validation();
        true
    }

    pub fn active_tab_is_api_client(&self) -> bool {
        self.tabs
            .get(self.active_tab)
            .is_some_and(|tab| tab.kind.is_api_client())
    }

    pub fn active_api_tab(&self) -> Option<(&ApiClientTabMeta, &ApiClientTabState)> {
        let tab = self.tabs.get(self.active_tab)?;
        match &tab.kind {
            crate::app::EditorTabKind::ApiClient(meta, state) => Some((meta, state)),
            _ => None,
        }
    }

    pub(crate) fn active_api_tab_mut_for(
        &mut self,
        spec_id: ApiSpecId,
    ) -> Option<(&mut ApiClientTabMeta, &mut ApiClientTabState)> {
        let tab = self.tabs.get_mut(self.active_tab)?;
        match &mut tab.kind {
            crate::app::EditorTabKind::ApiClient(meta, state) if meta.spec_id == spec_id => {
                Some((meta, state))
            }
            _ => None,
        }
    }

    /// Active API tab plus the shared API state, borrowed disjointly so tab-level
    /// transitions can run as `ApiClientTabState` methods.
    pub(crate) fn active_api_tab_and_state_mut(
        &mut self,
    ) -> Option<(&mut ApiClientTabMeta, &mut ApiClientTabState, &mut ApiClientState)> {
        let tab = self.tabs.get_mut(self.active_tab)?;
        match &mut tab.kind {
            crate::app::EditorTabKind::ApiClient(meta, state) => {
                Some((meta, state, &mut self.ide_panel.api))
            }
            _ => None,
        }
    }

    pub(crate) fn sync_api_tab_inputs(&mut self, spec_id: ApiSpecId, route_idx: usize) {
        if let Some((meta, state, api)) = self.active_api_tab_and_state_mut()
            && meta.spec_id == spec_id
        {
            api.sync_api_tab_inputs(spec_id, state, route_idx);
        }
    }

    pub fn focus_api_input(&mut self, focus: ApiFocus) {
        let active_spec_id = self.active_api_tab().map(|(meta, _)| meta.spec_id);
        if let Some(spec_id) = active_spec_id
            && let Some((_, state)) = self.active_api_tab_mut_for(spec_id)
        {
            state.focused_schema_pane = None;
        }
        let focus_changed = self.ide_panel.api.focused.as_ref() != Some(&focus);
        let dynamic_readonly_focus = matches!(
            focus,
            ApiFocus::InputSchema { .. }
                | ApiFocus::OutputSchema { .. }
                | ApiFocus::Response { .. }
        );
        if focus_changed {
            self.commit_api_focus();
            self.ide_panel.api.stash_active_api_mock_editor();
        }
        if focus_changed || dynamic_readonly_focus {
            let is_array = self.ide_panel.api.api_focus_is_array_input(&focus);
            let mut text = self.api_focus_text(&focus);
            if is_array {
                text = api_array_editor_text(&text);
            }
            let old_version = self.ide_panel.api.input_editor.version;
            if let Some(key) = ApiClientState::api_mock_editor_key_for_focus(&focus)
                && let Some(editor) = self.ide_panel.api.mock_python_editors.remove(&key)
            {
                self.ide_panel.api.input_editor = editor;
            } else {
                self.ide_panel.api.input_editor.set_text_clean(&text);
                self.ide_panel.api.input_editor.version = crate::editor::next_editor_version(old_version);
            }
            self.ide_panel.api.input_editor.cursor = self.ide_panel.api.input_editor.len();
            self.ide_panel.api.input_editor.selection_anchor =
                Some(self.ide_panel.api.input_editor.cursor);
            self.ide_panel.api.input_scroll_x.reset();
        }
        self.ide_panel.api.focused = Some(focus);
        if let Some(focus) = self.ide_panel.api.focused.clone() {
            match focus {
                ApiFocus::InputSchema { spec_id, .. } => {
                    if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
                        state.focused_schema_pane =
                            Some(crate::app::api_client::ApiSchemaPaneFocus::Input);
                    }
                }
                ApiFocus::OutputSchema { spec_id, .. } => {
                    if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
                        state.focused_schema_pane =
                            Some(crate::app::api_client::ApiSchemaPaneFocus::Output);
                    }
                }
                _ => {}
            }
        }
        self.search_focused = false;
        self.settings_ignore_focused = false;
        self.ide_panel.term_search_focused = false;
        self.ide_panel.git.message_focused = false;
        self.ide_panel.lsp_log_filter_focused = false;
        self.ide_panel.file_tree_focused = false;
        self.pulse_api_cursor_blink();
        self.ide_panel.api.queue_api_body_json_validation();
    }

}
