impl crate::app::App {
    fn pulse_api_cursor_blink(&mut self) {
        self.last_action = std::time::Instant::now();
        self.last_blink_state = true;
    }

    fn queue_api_body_json_validation(&mut self) {
        let Some(ApiFocus::Body { spec_id, route_idx }) = self.ide_panel.api.focused else {
            return;
        };
        let version = self.ide_panel.api.input_editor.version;
        if self
            .ide_panel
            .api
            .body_json_validation
            .is_some_and(|state| {
                state.spec_id == spec_id && state.route_idx == route_idx && state.version == version
            })
            || self.ide_panel.api.body_json_validation_pending
                == Some((spec_id, route_idx, version))
        {
            return;
        }
        let text = self.ide_panel.api.input_editor.get_full_text();
        let (tx, rx) = mpsc::channel();
        self.ide_panel.api.body_json_validation_pending = Some((spec_id, route_idx, version));
        self.ide_panel.api.body_json_validation_rx = Some(rx);
        std::thread::spawn(move || {
            let valid = json_body_is_valid(&text);
            let _ = tx.send(ApiJsonValidationResult {
                spec_id,
                route_idx,
                version,
                valid,
            });
        });
    }

    fn api_text_scroll_for_ui(&self, id: crate::ui_system::UiId) -> f32 {
        let Some((_, state)) = self.active_api_tab() else {
            return 0.0;
        };
        match id {
            crate::ui_system::UiId::ApiBodyInput(route_idx)
            | crate::ui_system::UiId::ApiMockStaticResponseInput(route_idx)
                if state.route_idx == Some(route_idx) =>
            {
                state.body_scroll.current
            }
            crate::ui_system::UiId::ApiResponseBody(route_idx)
                if state.route_idx == Some(route_idx) =>
            {
                state.response_scroll.current
            }
            crate::ui_system::UiId::ApiMockPreludeInput(route_idx) => self
                .ide_panel
                .api
                .mock_python_scrolls
                .get(&(route_idx, ApiMockSourcePart::Prelude))
                .map(|scroll| scroll.current)
                .unwrap_or(0.0),
            crate::ui_system::UiId::ApiMockBodyInput(route_idx) => self
                .ide_panel
                .api
                .mock_python_scrolls
                .get(&(route_idx, ApiMockSourcePart::Body))
                .map(|scroll| scroll.current)
                .unwrap_or(0.0),
            crate::ui_system::UiId::ApiMockSignatureInput(route_idx) => self
                .ide_panel
                .api
                .mock_python_scrolls
                .get(&(route_idx, ApiMockSourcePart::Signature))
                .map(|scroll| scroll.current)
                .unwrap_or(0.0),
            _ => 0.0,
        }
    }

    fn api_text_scroll_x_for_ui(&self, id: crate::ui_system::UiId) -> f32 {
        let Some((_, state)) = self.active_api_tab() else {
            return 0.0;
        };
        match id {
            crate::ui_system::UiId::ApiBodyInput(route_idx)
                if state.route_idx == Some(route_idx) =>
            {
                state.body_scroll_x.current
            }
            crate::ui_system::UiId::ApiResponseBody(route_idx)
                if state.route_idx == Some(route_idx) =>
            {
                state.response_scroll_x.current
            }
            crate::ui_system::UiId::ApiMockPreludeInput(route_idx) => self
                .ide_panel
                .api
                .mock_python_scrolls_x
                .get(&(route_idx, ApiMockSourcePart::Prelude))
                .map(|scroll| scroll.current)
                .unwrap_or(0.0),
            crate::ui_system::UiId::ApiMockBodyInput(route_idx) => self
                .ide_panel
                .api
                .mock_python_scrolls_x
                .get(&(route_idx, ApiMockSourcePart::Body))
                .map(|scroll| scroll.current)
                .unwrap_or(0.0),
            crate::ui_system::UiId::ApiMockSignatureInput(route_idx) => self
                .ide_panel
                .api
                .mock_python_scrolls_x
                .get(&(route_idx, ApiMockSourcePart::Signature))
                .map(|scroll| scroll.current)
                .unwrap_or(0.0),
            _ => 0.0,
        }
    }

    pub(crate) fn api_text_max_scroll_x_for_ui(&mut self, id: crate::ui_system::UiId) -> f32 {
        let Some(rect) = self.ui_registry.rect_for(id) else {
            return 0.0;
        };
        let Some((meta, state)) = self.active_api_tab() else {
            return 0.0;
        };
        let text = match id {
            crate::ui_system::UiId::ApiBodyScrollX(route_idx)
            | crate::ui_system::UiId::ApiBodyInput(route_idx)
                if state.route_idx == Some(route_idx) =>
            {
                if matches!(
                    self.ide_panel.api.focused,
                    Some(ApiFocus::Body { spec_id, route_idx: focused_route })
                        if spec_id == meta.spec_id && focused_route == route_idx
                ) {
                    self.ide_panel.api.input_editor.get_full_text()
                } else {
                    state.body_json.clone()
                }
            }
            crate::ui_system::UiId::ApiResponseScrollX(route_idx)
            | crate::ui_system::UiId::ApiResponseBody(route_idx)
                if state.route_idx == Some(route_idx) =>
            {
                if matches!(
                    self.ide_panel.api.focused,
                    Some(ApiFocus::Response { spec_id, route_idx: focused_route })
                        if spec_id == meta.spec_id && focused_route == route_idx
                ) {
                    self.ide_panel.api.input_editor.get_full_text()
                } else {
                    state
                        .response
                        .as_ref()
                        .map(|response| {
                            api_response_text(response, state.response_view).to_string()
                        })
                        .unwrap_or_default()
                }
            }
            crate::ui_system::UiId::ApiMockPreludeInput(route_idx) => self
                .api_route_python_script(route_idx)
                .map(|script| {
                    if self.api_mock_python_focus_target()
                        == Some((route_idx, ApiMockSourcePart::Prelude))
                    {
                        self.ide_panel.api.input_editor.get_full_text()
                    } else {
                        script.prelude.clone()
                    }
                })
                .unwrap_or_default(),
            crate::ui_system::UiId::ApiMockBodyInput(route_idx) => self
                .api_route_python_script(route_idx)
                .map(|script| {
                    if self.api_mock_python_focus_target()
                        == Some((route_idx, ApiMockSourcePart::Body))
                    {
                        self.ide_panel.api.input_editor.get_full_text()
                    } else {
                        api_mock_body_editor_text(&script.body)
                    }
                })
                .unwrap_or_default(),
            crate::ui_system::UiId::ApiMockSignatureInput(route_idx) => {
                self.api_mock_signature_for_route(route_idx).unwrap_or_default()
            }
            _ => return 0.0,
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
        api_text_area_max_scroll_x(&text, visible_w, |line| {
            renderer.measure_ui_width(line, API_BODY_TEXT_SCALE)
        })
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
        let text_w = renderer.measure_ui_width(&text, 0.88);
        (text_w - visible_w + 20.0 * scale).max(0.0)
    }

    fn sync_api_one_line_scroll_target(&mut self, immediate: bool) {
        let Some(focus) = self.ide_panel.api.focused.clone() else {
            return;
        };
        if self.api_focus_is_array_input(&focus) {
            return;
        }
        let Some((id, false)) = self.api_focus_ui_target(&focus) else {
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
        let text = self.ide_panel.api.input_editor.get_full_text();
        let cursor = self.ide_panel.api.input_editor.cursor.min(text.len());
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let cursor_x = renderer.measure_ui_width(&text[..cursor], 0.88);
        let max_scroll = self.api_one_line_max_scroll_x_for_ui(id);
        let scroll = &mut self.ide_panel.api.input_scroll_x;
        let mut target = scroll.target;
        if cursor_x - target > visible_w {
            target = cursor_x - visible_w + 10.0 * scale;
        } else if cursor_x < target {
            target = cursor_x;
        }
        scroll.target = target.clamp(0.0, max_scroll);
        if immediate {
            scroll.current = scroll.target;
            scroll.velocity = 0.0;
        }
    }

    fn api_mock_part_for_ui(id: crate::ui_system::UiId) -> Option<(usize, ApiMockSourcePart)> {
        match id {
            crate::ui_system::UiId::ApiMockPreludeInput(route_idx) => {
                Some((route_idx, ApiMockSourcePart::Prelude))
            }
            crate::ui_system::UiId::ApiMockBodyInput(route_idx) => {
                Some((route_idx, ApiMockSourcePart::Body))
            }
            crate::ui_system::UiId::ApiMockSignatureInput(route_idx) => {
                Some((route_idx, ApiMockSourcePart::Signature))
            }
            _ => None,
        }
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
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let cursor_x = renderer.measure_ui_width(cursor_line_text, API_BODY_TEXT_SCALE);
        let max_scroll_x = api_text_area_max_scroll_x(&text, visible_w, |line| {
            renderer.measure_ui_width(line, API_BODY_TEXT_SCALE)
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

        match id {
            crate::ui_system::UiId::ApiBodyInput(route_idx)
            | crate::ui_system::UiId::ApiMockStaticResponseInput(route_idx) => {
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id)
                    && state.route_idx == Some(route_idx)
                {
                    state.body_scroll.target = target_y;
                    state.body_scroll_x.target = target_x;
                    if immediate {
                        state.body_scroll.current = target_y;
                        state.body_scroll.velocity = 0.0;
                        state.body_scroll_x.current = target_x;
                        state.body_scroll_x.velocity = 0.0;
                    }
                }
            }
            crate::ui_system::UiId::ApiResponseBody(route_idx) => {
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id)
                    && state.route_idx == Some(route_idx)
                {
                    state.response_scroll.target = target_y;
                    state.response_scroll_x.target = target_x;
                    if immediate {
                        state.response_scroll.current = target_y;
                        state.response_scroll.velocity = 0.0;
                        state.response_scroll_x.current = target_x;
                        state.response_scroll_x.velocity = 0.0;
                    }
                }
            }
            _ => {
                if let Some(key) = Self::api_mock_part_for_ui(id) {
                    let scroll_y = self
                        .ide_panel
                        .api
                        .mock_python_scrolls
                        .entry(key)
                        .or_insert_with(|| ScrollState::new(7.0));
                    scroll_y.target = target_y;
                    if immediate {
                        scroll_y.current = target_y;
                        scroll_y.velocity = 0.0;
                    }
                    let scroll_x = self
                        .ide_panel
                        .api
                        .mock_python_scrolls_x
                        .entry(key)
                        .or_insert_with(|| ScrollState::new(7.0));
                    scroll_x.target = target_x;
                    if immediate {
                        scroll_x.current = target_x;
                        scroll_x.velocity = 0.0;
                    }
                }
            }
        }
    }

    pub(crate) fn drag_api_text_scrollbar_x_from_last_mouse(&mut self) -> bool {
        let Some((id, body)) = self.active_api_tab().and_then(|(_, state)| {
            let route_idx = state.route_idx?;
            if state.body_scroll_x.is_dragging {
                Some((crate::ui_system::UiId::ApiBodyScrollX(route_idx), true))
            } else if state.response_scroll_x.is_dragging {
                Some((crate::ui_system::UiId::ApiResponseScrollX(route_idx), false))
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
        if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
            let scroll = if body {
                &mut state.body_scroll_x
            } else {
                &mut state.response_scroll_x
            };
            let ratio = (mx - rect.0 - scroll.drag_offset) / rect.2.max(0.0001);
            scroll.target = (ratio * max_scroll).clamp(0.0, max_scroll);
            scroll.current = scroll.target;
        }
        true
    }

    fn api_focus_ui_target(&self, focus: &ApiFocus) -> Option<(crate::ui_system::UiId, bool)> {
        match focus {
            ApiFocus::ImportUrl => Some((crate::ui_system::UiId::ApiImportUrlInput, false)),
            ApiFocus::MockProxyBase => Some((crate::ui_system::UiId::ApiMockProxyBaseInput, false)),
            ApiFocus::MockPythonUvPath => {
                Some((crate::ui_system::UiId::ApiMockPythonUvPathInput, false))
            }
            ApiFocus::MockPythonVersion => {
                Some((crate::ui_system::UiId::ApiMockPythonVersionInput, false))
            }
            ApiFocus::MockPythonCustomPath => Some((
                crate::ui_system::UiId::ApiMockPythonCustomPathInput,
                false,
            )),
            ApiFocus::MockManualPath { manual_idx } => Some((
                crate::ui_system::UiId::ApiMockManualRoutePath(*manual_idx),
                false,
            )),
            ApiFocus::MockPrelude { route_idx } => Some((
                crate::ui_system::UiId::ApiMockPreludeInput(*route_idx),
                true,
            )),
            ApiFocus::MockBody { route_idx } => {
                Some((crate::ui_system::UiId::ApiMockBodyInput(*route_idx), true))
            }
            ApiFocus::MockSignature { route_idx } => Some((
                crate::ui_system::UiId::ApiMockSignatureInput(*route_idx),
                true,
            )),
            ApiFocus::MockStaticResponse { route_idx } => Some((
                crate::ui_system::UiId::ApiMockStaticResponseInput(*route_idx),
                true,
            )),
            ApiFocus::Body { route_idx, .. } => {
                Some((crate::ui_system::UiId::ApiBodyInput(*route_idx), true))
            }
            ApiFocus::Response { route_idx, .. } => {
                Some((crate::ui_system::UiId::ApiResponseBody(*route_idx), true))
            }
            ApiFocus::AuthValue { spec_id, scheme }
            | ApiFocus::AuthRefreshToken { spec_id, scheme }
            | ApiFocus::AuthUsername { spec_id, scheme }
            | ApiFocus::AuthPassword { spec_id, scheme } => {
                let idx = self
                    .ide_panel
                    .api
                    .models
                    .get(spec_id)?
                    .security_schemes
                    .iter()
                    .position(|item| item.name == *scheme)?;
                let id = match focus {
                    ApiFocus::AuthUsername { .. } => crate::ui_system::UiId::ApiAuthUsername(idx),
                    ApiFocus::AuthPassword { .. } => crate::ui_system::UiId::ApiAuthPassword(idx),
                    ApiFocus::AuthRefreshToken { .. } => {
                        crate::ui_system::UiId::ApiAuthRefreshToken(idx)
                    }
                    _ => crate::ui_system::UiId::ApiAuthValue(idx),
                };
                Some((id, false))
            }
            ApiFocus::PathParam {
                spec_id,
                route_idx,
                name,
            } => {
                let idx = self
                    .ide_panel
                    .api
                    .models
                    .get(spec_id)?
                    .routes
                    .get(*route_idx)?
                    .path_params
                    .iter()
                    .position(|param| param.name == *name)?;
                Some((
                    crate::ui_system::UiId::ApiPathParamInput(*route_idx, idx),
                    false,
                ))
            }
            ApiFocus::QueryParam {
                spec_id,
                route_idx,
                name,
            } => {
                let idx = self
                    .ide_panel
                    .api
                    .models
                    .get(spec_id)?
                    .routes
                    .get(*route_idx)?
                    .query_params
                    .iter()
                    .position(|param| param.name == *name)?;
                Some((
                    crate::ui_system::UiId::ApiQueryParamInput(*route_idx, idx),
                    false,
                ))
            }
            ApiFocus::BodyField {
                spec_id,
                route_idx,
                name,
            } => {
                let model = self.ide_panel.api.models.get(spec_id)?;
                let route = model.routes.get(*route_idx)?;
                let root = route.request_body.as_ref()?.schema?;
                let idx = model
                    .schema_arena
                    .get(root.0)?
                    .properties
                    .iter()
                    .position(|prop| prop.name == *name)?;
                Some((
                    crate::ui_system::UiId::ApiBodyFieldInput(*route_idx, idx),
                    false,
                ))
            }
        }
    }

    fn place_api_cursor_from_last_click(&mut self, id: crate::ui_system::UiId, multiline: bool) {
        let Some(rect) = self.ui_registry.rect_for(id) else {
            return;
        };
        if self
            .ide_panel
            .api
            .focused
            .as_ref()
            .is_some_and(|focus| self.api_focus_is_array_input(focus))
        {
            self.ide_panel.api.input_editor.cursor = self.ide_panel.api.input_editor.len();
            self.ide_panel.api.input_editor.selection_anchor =
                Some(self.ide_panel.api.input_editor.cursor);
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
        let cursor = if multiline {
            set_api_multiline_cursor_at_pointer(
                &mut self.ide_panel.api.input_editor,
                renderer,
                rect,
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
            api_line_byte_at_x(renderer, &text, target_x)
        };
        self.ide_panel.api.input_editor.cursor = cursor;
        self.ide_panel.api.input_editor.selection_anchor = Some(cursor);
        let now = std::time::Instant::now();
        let dx = mx - self.last_click_pos.0;
        let dy = my - self.last_click_pos.1;
        if now.duration_since(self.last_click_time).as_millis() < 400 && dx * dx + dy * dy < 25.0 {
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
        self.queue_api_body_json_validation();
    }

    pub(crate) fn drag_api_text_cursor_from_last_mouse(&mut self) -> bool {
        let Some(focus) = self.ide_panel.api.focused.clone() else {
            return false;
        };
        let Some((id, multiline)) = self.api_focus_ui_target(&focus) else {
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
        let cursor = if multiline {
            set_api_multiline_cursor_at_pointer(
                &mut self.ide_panel.api.input_editor,
                renderer,
                rect,
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
            api_line_byte_at_x(renderer, &text, target_x)
        };
        if self.ide_panel.api.input_editor.selection_anchor.is_none() {
            self.ide_panel.api.input_editor.selection_anchor =
                Some(self.ide_panel.api.input_editor.cursor);
        }
        self.ide_panel.api.input_editor.cursor = cursor;
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
        self.queue_api_body_json_validation();
        true
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn trigger_api_file_picker(&mut self) {
        let (tx, rx) = mpsc::channel();
        self.api_import_file_rx = Some(rx);
        std::thread::spawn(move || {
            let file = rfd::FileDialog::new()
                .set_title("Импорт openapi.json")
                .add_filter("OpenAPI JSON", &["json"])
                .pick_file();
            let _ = tx.send(file);
        });
    }

    fn trigger_api_body_file_picker(
        &mut self,
        spec_id: ApiSpecId,
        route_idx: usize,
        name: String,
        multi: bool,
    ) {
        let (tx, rx) = mpsc::channel();
        self.api_body_file_rx = Some(rx);
        std::thread::spawn(move || {
            let dialog = rfd::FileDialog::new().set_title("Выбрать файл");
            let paths = if multi {
                dialog.pick_files().unwrap_or_default()
            } else {
                dialog.pick_file().into_iter().collect()
            };
            let _ = tx.send(ApiBodyFilePickResult {
                spec_id,
                route_idx,
                name,
                paths,
            });
        });
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn trigger_api_python_path_picker(&mut self, kind: ApiPythonPathPickKind) {
        let (tx, rx) = mpsc::channel();
        self.ide_panel.api.python_path_pick_rx = Some(rx);
        std::thread::spawn(move || {
            let title = match kind {
                ApiPythonPathPickKind::Uv => "Выбрать исполняемый файл uv",
                ApiPythonPathPickKind::CustomPython => "Выбрать исполняемый файл Python",
            };
            let path = rfd::FileDialog::new().set_title(title).pick_file();
            let _ = tx.send(ApiPythonPathPickResult { kind, path });
        });
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn trigger_api_python_version_list(&mut self) {
        let Some(uv_path) = self.ide_panel.api.mock.uv.selected_uv_path() else {
            self.ide_panel.api.mock.uv.status =
                crate::app::api_mock::types::ApiPythonRuntimeStatus::Missing;
            self.ide_panel.api.mock.uv.last_error = "uv не найден. Укажите путь к uv.".to_string();
            return;
        };
        let (tx, rx) = mpsc::channel();
        self.ide_panel.api.python_version_list_rx = Some(rx);
        self.ide_panel.api.mock_python_versions_loading = true;
        self.ide_panel.api.mock_python_version_picker_open = true;
        self.ide_panel.api.mock_python_versions_scroll.current = 0.0;
        self.ide_panel.api.mock_python_versions_scroll.target = 0.0;
        std::thread::spawn(move || {
            let result = Command::new(uv_path)
                .arg("python")
                .arg("list")
                .arg("--all-versions")
                .output();
            let payload = match result {
                Ok(output) if output.status.success() => ApiPythonVersionListResult {
                    rows: parse_uv_python_list(&String::from_utf8_lossy(&output.stdout)),
                    error: None,
                },
                Ok(output) => ApiPythonVersionListResult {
                    rows: Vec::new(),
                    error: Some(format!(
                        "Ошибка списка версий: {}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    )),
                },
                Err(err) => ApiPythonVersionListResult {
                    rows: Vec::new(),
                    error: Some(format!("Ошибка запуска uv: {err}")),
                },
            };
            let _ = tx.send(payload);
        });
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn trigger_api_python_install(&mut self) {
        if self.ide_panel.api.mock_python_install_running {
            return;
        }
        let Some(uv_path) = self.ide_panel.api.mock.uv.selected_uv_path() else {
            self.ide_panel.api.mock.uv.last_error = "uv не найден. Укажите путь к uv.".to_string();
            return;
        };
        let version = self.ide_panel.api.mock.uv.python_version.trim().to_string();
        if version.is_empty() {
            self.ide_panel.api.mock.uv.last_error = "Выберите версию Python.".to_string();
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.ide_panel.api.python_install_rx = Some(rx);
        self.ide_panel.api.mock_python_install_running = true;
        self.ide_panel.api.mock_python_install_log.clear();
        self.ide_panel
            .api
            .mock_python_install_log
            .push(ApiPythonInstallLogLine {
                text: format!("uv python install {version}"),
                kind: ApiPythonInstallLogKind::Info,
            });
        std::thread::spawn(move || {
            let spawn = Command::new(uv_path)
                .arg("python")
                .arg("install")
                .arg(&version)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn();
            let mut child = match spawn {
                Ok(child) => child,
                Err(err) => {
                    let _ = tx.send(ApiPythonInstallEvent::Done(Err(format!(
                        "Ошибка запуска uv: {err}"
                    ))));
                    return;
                }
            };
            if let Some(stdout) = child.stdout.take() {
                spawn_api_python_log_reader(stdout, tx.clone(), ApiPythonInstallLogKind::Info);
            }
            if let Some(stderr) = child.stderr.take() {
                spawn_api_python_log_reader(stderr, tx.clone(), ApiPythonInstallLogKind::Error);
            }
            match child.wait() {
                Ok(status) if status.success() => {
                    let _ = tx.send(ApiPythonInstallEvent::Done(Ok(())));
                }
                Ok(status) => {
                    let _ = tx.send(ApiPythonInstallEvent::Done(Err(format!(
                        "uv завершился с кодом {:?}",
                        status.code()
                    ))));
                }
                Err(err) => {
                    let _ = tx.send(ApiPythonInstallEvent::Done(Err(format!(
                        "Ошибка ожидания uv: {err}"
                    ))));
                }
            }
        });
    }

    fn apply_api_body_file_pick(&mut self, result: ApiBodyFilePickResult) {
        if result.paths.is_empty() {
            return;
        }
        let new_value = result
            .paths
            .iter()
            .map(|path| path.to_string_lossy())
            .collect::<Vec<_>>()
            .join("\n");
        if let Some((_, state)) = self.active_api_tab_mut_for(result.spec_id)
            && state.route_idx == Some(result.route_idx)
            && let Some(value) = state
                .body_values
                .iter_mut()
                .find(|value| value.name == result.name)
        {
            value.value = new_value.clone();
        }
        if matches!(
            self.ide_panel.api.focused,
            Some(ApiFocus::BodyField {
                spec_id,
                route_idx,
                ref name,
            }) if spec_id == result.spec_id && route_idx == result.route_idx && name == &result.name
        ) {
            let old_version = self.ide_panel.api.input_editor.version;
            self.ide_panel.api.input_editor.set_text_clean(&new_value);
            self.ide_panel.api.input_editor.version = old_version.saturating_add(1);
        }
    }

    pub fn start_api_local_import(&mut self, path: PathBuf) {
        let id = self.ide_panel.api.alloc_spec_id();
        self.ide_panel.api.loading.insert(id);
        self.api_load_rx.push(spawn_load_local(id, path));
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub fn start_api_url_import_from_input(&mut self) {
        let raw = self.ide_panel.api.input_editor.get_full_text();
        let url = match validate_api_url(&raw) {
            Ok(url) => url.to_string(),
            Err(err) => {
                self.ide_panel.api.import_error = Some(err.message);
                self.ide_panel.api.import_error_at = Some(now_epoch_secs());
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
                return;
            }
        };
        let id = self.ide_panel.api.alloc_spec_id();
        self.ide_panel.api.import_error = None;
        self.ide_panel.api.import_error_at = None;
        self.ide_panel.api.import_url_open = false;
        self.ide_panel.api.focused = None;
        self.ide_panel.api.loading.insert(id);
        self.api_load_rx.push(spawn_load_url(id, url));
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub fn refresh_api_spec(&mut self, id: ApiSpecId) {
        let Some(entry) = self
            .ide_panel
            .api
            .specs
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
        else {
            return;
        };
        self.ide_panel.api.loading.insert(id);
        match entry.source {
            ApiSpecSource::Local(path) => self.api_load_rx.push(spawn_load_local(id, path)),
            ApiSpecSource::Url(url) => self.api_load_rx.push(spawn_load_url(id, url)),
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub fn ensure_api_model_loaded(&mut self, id: ApiSpecId) {
        if self.ide_panel.api.models.contains_key(&id) || self.ide_panel.api.loading.contains(&id) {
            return;
        }
        let Some(entry) = self
            .ide_panel
            .api
            .specs
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
        else {
            return;
        };
        self.ide_panel.api.loading.insert(id);
        match entry.source {
            ApiSpecSource::Local(path) => self.api_load_rx.push(spawn_load_local(id, path)),
            ApiSpecSource::Url(url) => self.api_load_rx.push(spawn_load_cached_url(id, url)),
        }
    }

    pub fn open_api_spec_tab(&mut self, id: ApiSpecId) {
        self.ide_panel.api.select_spec(id);
        self.ensure_api_model_loaded(id);
        let title = self
            .ide_panel
            .api
            .specs
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.title.clone())
            .unwrap_or_else(|| "API".to_string());

        if let Some(idx) = self.tabs.iter().position(|tab| {
            matches!(
                &tab.kind,
                crate::app::EditorTabKind::ApiClient(meta, state)
                    if meta.spec_id == id && !state.auth_view
            )
        }) {
            self.switch_to_tab(idx);
            return;
        }

        let mut api_state = ApiClientTabState::default();
        if let Some(model) = self.ide_panel.api.models.get(&id)
            && let Some(route) = model.routes.first()
        {
            api_state.route_idx = Some(0);
            fill_api_tab_inputs(&mut api_state, route, model);
        }

        let tab = crate::app::EditorTab {
            editor: Editor::new(16),
            file_path: None,
            base_title: title.clone(),
            file_extension: String::new(),
            scroll_y: ScrollState::new(7.0),
            scroll_x: ScrollState::new(7.0),
            spans: Vec::new(),
            completions: Vec::new(),
            foldable_ranges: Vec::new(),
            syntax_errors: Vec::new(),
            last_sent_version: u64::MAX,
            search_results: Vec::new(),
            search_current_idx: None,
            is_highlighted_once: true,
            is_highlight_complete: true,
            icon_key: "api",
            kind: crate::app::EditorTabKind::ApiClient(
                ApiClientTabMeta {
                    spec_id: id,
                    title,
                    route_identity: api_state.route_idx.map(|route_idx| {
                        ApiClientRouteIdentity::OpenApi {
                            spec_id: id,
                            route_idx,
                        }
                    }),
                },
                api_state,
            ),
        };

        if self.tabs.is_empty() {
            self.editor = Editor::new(16);
            self.file_path = None;
            self.base_title = tab.base_title.clone();
            self.file_extension.clear();
            self.scroll_y = ScrollState::new(7.0);
            self.scroll_x = ScrollState::new(7.0);
            self.tabs.push(tab);
            self.active_tab = 0;
        } else {
            self.sync_active_tab();
            self.tabs.push(tab);
            self.active_tab = self.tabs.len().saturating_sub(1);
            self.sync_active_tab();
        }
        while self.highlighter.rx.try_recv().is_ok() {}
        self.autocomplete_active = false;
        self.show_welcome = false;
        self.reveal_tab_now(self.active_tab);
        if let Some(window) = self.window.as_ref() {
            crate::app::App::update_window_title(window, &self.base_title, false);
            window.request_redraw();
        }
        self.save_tabs_state();
    }

    pub fn open_api_auth_tab(&mut self, id: ApiSpecId) {
        self.ide_panel.api.select_spec(id);
        self.ensure_api_model_loaded(id);
        let title = self
            .ide_panel
            .api
            .specs
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| format!("Auth · {}", entry.title))
            .unwrap_or_else(|| "API Auth".to_string());

        if let Some(idx) = self.tabs.iter().position(|tab| {
            matches!(
                &tab.kind,
                crate::app::EditorTabKind::ApiClient(meta, state)
                    if meta.spec_id == id && state.auth_view
            )
        }) {
            self.switch_to_tab(idx);
            return;
        }

        let api_state = ApiClientTabState {
            auth_view: true,
            ..Default::default()
        };
        let tab = crate::app::EditorTab {
            editor: Editor::new(16),
            file_path: None,
            base_title: title.clone(),
            file_extension: String::new(),
            scroll_y: ScrollState::new(7.0),
            scroll_x: ScrollState::new(7.0),
            spans: Vec::new(),
            completions: Vec::new(),
            foldable_ranges: Vec::new(),
            syntax_errors: Vec::new(),
            last_sent_version: u64::MAX,
            search_results: Vec::new(),
            search_current_idx: None,
            is_highlighted_once: true,
            is_highlight_complete: true,
            icon_key: "api",
            kind: crate::app::EditorTabKind::ApiClient(
                ApiClientTabMeta {
                    spec_id: id,
                    title,
                    route_identity: None,
                },
                api_state,
            ),
        };

        if self.tabs.is_empty() {
            self.editor = Editor::new(16);
            self.file_path = None;
            self.base_title = tab.base_title.clone();
            self.file_extension.clear();
            self.scroll_y = ScrollState::new(7.0);
            self.scroll_x = ScrollState::new(7.0);
            self.tabs.push(tab);
            self.active_tab = 0;
        } else {
            self.sync_active_tab();
            self.tabs.push(tab);
            self.active_tab = self.tabs.len().saturating_sub(1);
            self.sync_active_tab();
        }
        self.autocomplete_active = false;
        self.show_welcome = false;
        self.reveal_tab_now(self.active_tab);
        if let Some(window) = self.window.as_ref() {
            crate::app::App::update_window_title(window, &self.base_title, false);
            window.request_redraw();
        }
        self.save_tabs_state();
    }

    pub fn open_api_route(&mut self, spec_id: ApiSpecId, route_idx: usize) {
        self.open_api_spec_tab(spec_id);
        let mut needs_input_sync = false;
        if let Some((meta, state)) = self.active_api_tab_mut_for(spec_id) {
            state.remember_view_scroll();
            state.remember_route_state();
            state.auth_view = false;
            meta.route_identity = Some(ApiClientRouteIdentity::OpenApi { spec_id, route_idx });
            if !state.restore_route_state(route_idx) {
                state.route_idx = Some(route_idx);
                state.response = None;
                state.response_view = ApiResponseView::Body;
                state.pending = false;
                state.pending_request_id = None;
                state.body_scroll.current = 0.0;
                state.body_scroll.target = 0.0;
                state.body_scroll_x.current = 0.0;
                state.body_scroll_x.target = 0.0;
                state.response_scroll.current = 0.0;
                state.response_scroll.target = 0.0;
                state.response_scroll_x.current = 0.0;
                state.response_scroll_x.target = 0.0;
                needs_input_sync = true;
            }
            state.restore_view_scroll(false, Some(route_idx));
        }
        if needs_input_sync {
            self.sync_api_tab_inputs(spec_id, route_idx);
        }
        self.save_tabs_state();
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub fn open_api_manual_route(&mut self, manual_idx: usize) {
        self.commit_api_focus();
        let Some(route) = self
            .ide_panel
            .api
            .mock
            .manual_routes
            .get(manual_idx)
            .cloned()
        else {
            return;
        };
        let stable_id = route.stable_id.clone();
        let title = api_manual_route_title(route.method, &route.path);
        if let Some(idx) = self.tabs.iter().position(|tab| {
            matches!(
                &tab.kind,
                crate::app::EditorTabKind::ApiClient(
                    ApiClientTabMeta {
                        route_identity:
                            Some(ApiClientRouteIdentity::Manual { stable_id: tab_id }),
                        ..
                    },
                    _
                ) if tab_id == &stable_id
            )
        }) {
            self.switch_to_tab(idx);
            if let Some((meta, state)) = self.active_api_tab_mut_for(API_MANUAL_MOCK_SPEC_ID) {
                meta.title = title.clone();
                state.route_idx = Some(manual_idx);
            }
            self.base_title = title;
            if let Some(window) = self.window.as_ref() {
                crate::app::App::update_window_title(window, &self.base_title, false);
                window.request_redraw();
            }
            return;
        }

        let api_state = ApiClientTabState {
            route_idx: Some(manual_idx),
            ..Default::default()
        };
        let tab = crate::app::EditorTab {
            editor: Editor::new(16),
            file_path: None,
            base_title: title.clone(),
            file_extension: String::new(),
            scroll_y: ScrollState::new(7.0),
            scroll_x: ScrollState::new(7.0),
            spans: Vec::new(),
            completions: Vec::new(),
            foldable_ranges: Vec::new(),
            syntax_errors: Vec::new(),
            last_sent_version: u64::MAX,
            search_results: Vec::new(),
            search_current_idx: None,
            is_highlighted_once: true,
            is_highlight_complete: true,
            icon_key: "api",
            kind: crate::app::EditorTabKind::ApiClient(
                ApiClientTabMeta {
                    spec_id: API_MANUAL_MOCK_SPEC_ID,
                    title,
                    route_identity: Some(ApiClientRouteIdentity::Manual { stable_id }),
                },
                api_state,
            ),
        };

        if self.tabs.is_empty() {
            self.editor = Editor::new(16);
            self.file_path = None;
            self.base_title = tab.base_title.clone();
            self.file_extension.clear();
            self.scroll_y = ScrollState::new(7.0);
            self.scroll_x = ScrollState::new(7.0);
            self.tabs.push(tab);
            self.active_tab = 0;
        } else {
            self.sync_active_tab();
            self.tabs.push(tab);
            self.active_tab = self.tabs.len().saturating_sub(1);
            self.sync_active_tab();
        }
        self.autocomplete_active = false;
        self.show_welcome = false;
        self.reveal_tab_now(self.active_tab);
        if let Some(window) = self.window.as_ref() {
            crate::app::App::update_window_title(window, &self.base_title, false);
            window.request_redraw();
        }
        self.save_tabs_state();
    }

    fn sync_api_manual_route_tabs(&mut self) {
        let routes = self
            .ide_panel
            .api
            .mock
            .manual_routes
            .iter()
            .enumerate()
            .map(|(idx, route)| {
                (
                    idx,
                    route.stable_id.clone(),
                    api_manual_route_title(route.method, &route.path),
                )
            })
            .collect::<Vec<_>>();
        for (tab_idx, tab) in self.tabs.iter_mut().enumerate() {
            let crate::app::EditorTabKind::ApiClient(meta, state) = &mut tab.kind else {
                continue;
            };
            let Some(ApiClientRouteIdentity::Manual { stable_id }) = &meta.route_identity else {
                continue;
            };
            if let Some((manual_idx, _, title)) = routes.iter().find(|(_, id, _)| id == stable_id) {
                meta.title = title.clone();
                state.route_idx = Some(*manual_idx);
                tab.base_title = title.clone();
                if tab_idx == self.active_tab {
                    self.base_title = title.clone();
                }
            } else {
                meta.title = "Mock removed".to_string();
                state.route_idx = None;
                tab.base_title = meta.title.clone();
                if tab_idx == self.active_tab {
                    self.base_title = meta.title.clone();
                }
            }
        }
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

    pub(crate) fn sync_api_tab_inputs(&mut self, spec_id: ApiSpecId, route_idx: usize) {
        let Some(model) = self.ide_panel.api.models.get(&spec_id) else {
            return;
        };
        let Some(route) = model.routes.get(route_idx) else {
            return;
        };
        let path_values = route
            .path_params
            .iter()
            .map(|param| ApiInputValue {
                name: param.name.clone(),
                value: param
                    .default_value
                    .clone()
                    .or_else(|| param.example.clone())
                    .unwrap_or_default(),
            })
            .collect::<Vec<_>>();
        let query_values = route
            .query_params
            .iter()
            .map(|param| ApiInputValue {
                name: param.name.clone(),
                value: param
                    .default_value
                    .clone()
                    .or_else(|| param.example.clone())
                    .unwrap_or_default(),
            })
            .collect::<Vec<_>>();
        let body_values = default_body_values_for_route(route, model);
        let body_json = default_body_for_route(route, model);
        if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
            state.path_values = path_values;
            state.query_values = query_values;
            state.body_values = body_values;
            state.body_json = body_json;
            state.body_scroll.current = 0.0;
            state.body_scroll.target = 0.0;
            state.body_scroll_x.current = 0.0;
            state.body_scroll_x.target = 0.0;
            state.response_scroll.current = 0.0;
            state.response_scroll.target = 0.0;
            state.response_scroll_x.current = 0.0;
            state.response_scroll_x.target = 0.0;
        }
    }

    pub fn focus_api_input(&mut self, focus: ApiFocus) {
        let focus_changed = self.ide_panel.api.focused.as_ref() != Some(&focus);
        if focus_changed {
            self.commit_api_focus();
            self.stash_active_api_mock_editor();
            let is_array = self.api_focus_is_array_input(&focus);
            let mut text = self.api_focus_text(&focus);
            if is_array {
                text = api_array_editor_text(&text);
            }
            let old_version = self.ide_panel.api.input_editor.version;
            if let Some(key) = Self::api_mock_editor_key_for_focus(&focus)
                && let Some(editor) = self.ide_panel.api.mock_python_editors.remove(&key)
            {
                self.ide_panel.api.input_editor = editor;
            } else {
                self.ide_panel.api.input_editor.set_text_clean(&text);
                self.ide_panel.api.input_editor.version = old_version.saturating_add(1);
            }
            self.ide_panel.api.input_editor.cursor = self.ide_panel.api.input_editor.len();
            self.ide_panel.api.input_editor.selection_anchor =
                Some(self.ide_panel.api.input_editor.cursor);
            self.ide_panel.api.input_scroll_x.current = 0.0;
            self.ide_panel.api.input_scroll_x.target = 0.0;
            self.ide_panel.api.input_scroll_x.velocity = 0.0;
        }
        self.ide_panel.api.focused = Some(focus);
        self.search_focused = false;
        self.settings_ignore_focused = false;
        self.ide_panel.term_search_focused = false;
        self.ide_panel.git.message_focused = false;
        self.ide_panel.lsp_log_filter_focused = false;
        self.ide_panel.file_tree_focused = false;
        self.pulse_api_cursor_blink();
        self.queue_api_body_json_validation();
        if focus_changed && let Some((route_idx, _)) = self.api_mock_python_focus_target() {
            self.queue_api_mock_python_tools(route_idx);
        }
    }

}
