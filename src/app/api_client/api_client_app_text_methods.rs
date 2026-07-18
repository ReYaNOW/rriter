pub(crate) fn native_picker_can_start<T>(
    receiver: &Option<std::sync::mpsc::Receiver<T>>,
) -> bool {
    crate::platform::receiver_slot_available(receiver)
}

impl crate::app::App {
    fn api_route_row_text(route: &ApiRouteRow, field: ApiRouteTextField) -> String {
        match field {
            ApiRouteTextField::Path => {
                let mut display = String::with_capacity(route.path.len().saturating_add(8));
                write_api_path_display(&route.path, &mut display);
                display
            }
            ApiRouteTextField::Summary => route.summary.clone(),
            ApiRouteTextField::Description => route.description.clone(),
        }
    }

    fn active_api_route_text(&self, field: ApiRouteTextField) -> Option<String> {
        let (meta, state) = self.active_api_tab()?;
        match &meta.route_identity {
            Some(ApiClientRouteIdentity::Manual { stable_id }) => {
                let route = self
                    .ide_panel
                    .api
                    .mock
                    .manual_routes
                    .iter()
                    .find(|route| route.stable_id == *stable_id)?;
                Some(match field {
                    ApiRouteTextField::Path => {
                        let mut display = String::with_capacity(route.path.len().saturating_add(8));
                        write_api_path_display(&route.path, &mut display);
                        display
                    }
                    ApiRouteTextField::Summary => "Manual mock route".to_string(),
                    ApiRouteTextField::Description => String::new(),
                })
            }
            Some(ApiClientRouteIdentity::OpenApi { spec_id, route_idx }) => self
                .ide_panel
                .api
                .models
                .get(spec_id)
                .and_then(|model| model.routes.get(*route_idx))
                .map(|route| Self::api_route_row_text(route, field)),
            None => self
                .ide_panel
                .api
                .models
                .get(&meta.spec_id)
                .and_then(|model| state.route_idx.and_then(|idx| model.routes.get(idx)))
                .map(|route| Self::api_route_row_text(route, field)),
        }
    }

    fn api_route_text_ui_id(
        field: ApiRouteTextField,
        route_idx: usize,
    ) -> crate::ui_system::UiId {
        match field {
            ApiRouteTextField::Path => crate::ui_system::UiId::ApiRoutePathText(route_idx),
            ApiRouteTextField::Summary => crate::ui_system::UiId::ApiRouteSummaryText(route_idx),
            ApiRouteTextField::Description => {
                crate::ui_system::UiId::ApiRouteDescriptionText(route_idx)
            }
        }
    }

    pub(crate) fn begin_api_route_text_selection(
        &mut self,
        field: ApiRouteTextField,
        route_idx: usize,
    ) -> bool {
        let Some(text) = self.active_api_route_text(field) else {
            return false;
        };
        let id = Self::api_route_text_ui_id(field, route_idx);
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
        if state.route_idx != Some(route_idx) {
            return false;
        }
        state.route_text_selection = Some(ApiRouteTextSelection {
            field,
            anchor: byte,
            cursor: byte,
            selecting: true,
        });
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
        let id = Self::api_route_text_ui_id(selection.field, route_idx);
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
            && let Some(selection) = state.route_text_selection.as_mut()
        {
            selection.cursor = byte;
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
        let Some(selection) = state.route_text_selection.as_mut() else {
            return false;
        };
        if !selection.selecting {
            return false;
        }
        selection.selecting = false;
        true
    }

    pub(crate) fn copy_api_route_text_selection(&mut self) -> bool {
        let Some(selection) = self
            .active_api_tab()
            .and_then(|(_, state)| state.route_text_selection)
        else {
            return false;
        };
        let Some(text) = self.active_api_route_text(selection.field) else {
            return false;
        };
        let Some(selected) = api_route_selected_text(selection, &text) else {
            return false;
        };
        self.set_clipboard_text(selected.to_string());
        true
    }

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
        let worker_tx = tx.clone();
        if let Err(err) = crate::platform::spawn_named("rriter-api-json-validation", move || {
            let valid = json_body_is_valid(&text);
            let _ = worker_tx.send(ApiJsonValidationResult {
                spec_id, route_idx, version, valid,
            });
        }) {
            eprintln!("RRiter: не удалось запустить JSON validation worker: {err}");
            let _ = tx.send(ApiJsonValidationResult {
                spec_id, route_idx, version, valid: false,
            });
        }
    }

    fn api_text_scroll_for_ui(&self, id: crate::ui_system::UiId) -> f32 {
        let Some((_, state)) = self.active_api_tab() else {
            return 0.0;
        };
        match id {
            crate::ui_system::UiId::ApiBodyInput(route_idx)
            | crate::ui_system::UiId::ApiInputSchemaBody(route_idx)
            | crate::ui_system::UiId::ApiMockStaticResponseInput(route_idx)
                if state.route_idx == Some(route_idx) =>
            {
                state.body_scroll.current
            }
            crate::ui_system::UiId::ApiResponseBody(route_idx)
            | crate::ui_system::UiId::ApiOutputSchemaBody(route_idx)
                if state.route_idx == Some(route_idx) =>
            {
                state.response_scroll.current
            }
            crate::ui_system::UiId::ApiMockContractInput(_)
            | crate::ui_system::UiId::ApiMockPreludeInput(_)
            | crate::ui_system::UiId::ApiMockBodyInput(_)
            | crate::ui_system::UiId::ApiMockSignatureInput(_) => 0.0,
            _ => 0.0,
        }
    }

    fn api_text_scroll_x_for_ui(&self, id: crate::ui_system::UiId) -> f32 {
        let Some((_, state)) = self.active_api_tab() else {
            return 0.0;
        };
        match id {
            crate::ui_system::UiId::ApiBodyInput(route_idx)
            | crate::ui_system::UiId::ApiInputSchemaBody(route_idx)
                if state.route_idx == Some(route_idx) =>
            {
                state.body_scroll_x.current
            }
            crate::ui_system::UiId::ApiResponseBody(route_idx)
            | crate::ui_system::UiId::ApiOutputSchemaBody(route_idx)
                if state.route_idx == Some(route_idx) =>
            {
                state.response_scroll_x.current
            }
            crate::ui_system::UiId::ApiMockContractInput(route_idx) => self
                .ide_panel
                .api
                .mock_python_scrolls_x
                .get(&(route_idx, ApiMockSourcePart::Contract))
                .map(|scroll| scroll.current)
                .unwrap_or(0.0),
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
            crate::ui_system::UiId::ApiInputSchemaBody(route_idx)
                if state.route_idx == Some(route_idx) =>
            {
                self.api_mock_input_schema_text_for_focus_route(meta.spec_id, route_idx)
                    .or_else(|| {
                        self.ide_panel
                            .api
                            .models
                            .get(&meta.spec_id)
                            .and_then(|model| {
                                model.routes.get(route_idx).map(|route| {
                                    api_route_input_schema_text(
                                        route,
                                        model,
                                        state.input_schema_idx,
                                        &state.input_schema_collapsed,
                                    )
                                })
                            })
                    })
                    .unwrap_or_default()
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
            crate::ui_system::UiId::ApiOutputSchemaBody(route_idx)
                if state.route_idx == Some(route_idx) =>
            {
                self.ide_panel
                    .api
                    .models
                    .get(&meta.spec_id)
                    .and_then(|model| {
                        model.routes.get(route_idx).map(|route| {
                            match state.output_doc_view {
                                ApiOutputDocView::Example => {
                                    api_route_output_example_text_for(
                                        route,
                                        model,
                                        state.output_status_idx,
                                        state.output_example_idx,
                                    )
                                }
                                ApiOutputDocView::Schema => {
                                    api_route_output_schema_text_for(
                                        route,
                                        model,
                                        state.output_status_idx,
                                        state.output_schema_idx,
                                        &state.output_schema_collapsed,
                                    )
                                }
                            }
                        })
                    })
                    .unwrap_or_default()
            }
            crate::ui_system::UiId::ApiMockStaticResponseInput(route_idx)
                if state.route_idx == Some(route_idx) =>
            {
                if matches!(
                    self.ide_panel.api.focused,
                    Some(ApiFocus::MockStaticResponse { route_idx: focused_route })
                        if focused_route == route_idx
                ) {
                    self.ide_panel.api.input_editor.get_full_text()
                } else {
                    self.active_manual_mock_route(route_idx)
                        .map(|route| &route.response)
                        .or_else(|| self.api_route_override(route_idx).map(|route| &route.response))
                        .map(|response| match response {
                            crate::app::api_mock::types::ApiMockResponse::Generated => self
                                .api_mock_generated_preview(route_idx)
                                .unwrap_or_else(|| "{}".to_string()),
                            crate::app::api_mock::types::ApiMockResponse::Json(text)
                            | crate::app::api_mock::types::ApiMockResponse::Text(text) => {
                                text.clone()
                            }
                        })
                        .unwrap_or_else(|| {
                            self.api_mock_generated_preview(route_idx)
                                .unwrap_or_else(|| "{}".to_string())
                        })
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
            crate::ui_system::UiId::ApiMockContractInput(route_idx) => self
                .api_route_python_script(route_idx)
                .map(|_| {
                    if self.api_mock_python_focus_target()
                        == Some((route_idx, ApiMockSourcePart::Contract))
                    {
                        self.ide_panel.api.input_editor.get_full_text()
                    } else {
                        self.api_mock_contract_source_for_route(route_idx)
                            .unwrap_or_default()
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
        let use_mono_width = Self::api_mock_part_for_ui(id).is_some();
        api_text_area_max_scroll_x(&text, visible_w, |line| {
            if use_mono_width {
                line.chars().map(|ch| renderer.char_advance(ch)).sum()
            } else {
                renderer.measure_ui_width(line, API_BODY_TEXT_SCALE)
            }
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
        crate::app::one_line_input_max_scroll_x(renderer, &text, visible_w, 0.88, 20.0 * scale)
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
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        crate::app::sync_one_line_input_scroll_target(
            renderer,
            &self.ide_panel.api.input_editor,
            &mut self.ide_panel.api.input_scroll_x,
            visible_w,
            0.88,
            10.0 * scale,
            immediate,
        );
    }

    fn api_mock_part_for_ui(id: crate::ui_system::UiId) -> Option<(usize, ApiMockSourcePart)> {
        match id {
            crate::ui_system::UiId::ApiMockContractInput(route_idx) => {
                Some((route_idx, ApiMockSourcePart::Contract))
            }
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

    fn api_mock_combined_max_scroll_for_route(&self, route_idx: usize, scale: f32) -> f32 {
        let Some((_, _, route, model)) = self.api_mock_route_context(route_idx) else {
            return 0.0;
        };
        let Some(script) = self.api_mock_script_for_tools(route_idx) else {
            return 0.0;
        };
        let contract = crate::app::api_mock::types::api_mock_effective_contract(
            &script, &route, &model,
        );
        let signature_text =
            crate::app::api_mock::contract::api_mock_handler_signature_text(&contract);
        let contract_text = if self.api_mock_python_focus_target()
            == Some((route_idx, ApiMockSourcePart::Contract))
        {
            self.ide_panel.api.input_editor.get_full_text()
        } else {
            self.api_mock_contract_source_for_route(route_idx)
                .unwrap_or_default()
        };
        let content_h = api_mock_combined_editor_content_height(
            &script.prelude,
            &contract_text,
            &signature_text,
            &script.body,
            scale,
        );
        let viewport_h = api_mock_combined_editor_viewport_height(&signature_text, scale);
        (content_h - viewport_h).max(0.0)
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
        let mock_part = Self::api_mock_part_for_ui(id);
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
                if let Some(key @ (route_idx, _)) = mock_part {
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
                    if let Some(viewport) = self
                        .ui_registry
                        .rect_for(crate::ui_system::UiId::ApiMockCombinedPython(route_idx))
                    {
                        let max_scroll = self.api_mock_combined_max_scroll_for_route(route_idx, scale);
                        let text_top_y = Self::api_multiline_cursor_top_y(id, rect, scale);
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
                            scroll_y.target = scroll_y.current + cursor_bottom - bottom_limit;
                        } else if cursor_top < top_limit {
                            scroll_y.target = scroll_y.current - (top_limit - cursor_top);
                        }
                        scroll_y.target = scroll_y.target.clamp(0.0, max_scroll);
                        if immediate {
                            scroll_y.current = scroll_y.target;
                            scroll_y.velocity = 0.0;
                        }
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
            ApiFocus::RouteFilter => {
                Some((crate::ui_system::UiId::ApiRouteFilterInput, false))
            }
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
            ApiFocus::MockContract { route_idx } => Some((
                crate::ui_system::UiId::ApiMockContractInput(*route_idx),
                true,
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
            ApiFocus::MockContractField {
                route_idx,
                group,
                field_idx,
                prop,
            } => Some((
                crate::ui_system::UiId::ApiMockContractFieldPropInput(
                    *route_idx, *group, *field_idx, *prop,
                ),
                false,
            )),
            ApiFocus::Body { route_idx, .. } => {
                Some((crate::ui_system::UiId::ApiBodyInput(*route_idx), true))
            }
            ApiFocus::InputSchema { route_idx, .. } => Some((
                crate::ui_system::UiId::ApiInputSchemaBody(*route_idx),
                true,
            )),
            ApiFocus::OutputSchema { route_idx, .. } => Some((
                crate::ui_system::UiId::ApiOutputSchemaBody(*route_idx),
                true,
            )),
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

    fn api_multiline_cursor_top_y(
        id: crate::ui_system::UiId,
        rect: (f32, f32, f32, f32),
        scale: f32,
    ) -> f32 {
        match id {
            crate::ui_system::UiId::ApiMockSignatureInput(_) => rect.1,
            crate::ui_system::UiId::ApiMockContractInput(_)
            | crate::ui_system::UiId::ApiMockPreludeInput(_)
            | crate::ui_system::UiId::ApiMockBodyInput(_) => {
                api_text_area_top_from_baseline(
                    Self::api_mock_text_baseline_y(id, rect, scale),
                    scale,
                )
            }
            crate::ui_system::UiId::ApiInputSchemaBody(_)
            | crate::ui_system::UiId::ApiOutputSchemaBody(_)
            | crate::ui_system::UiId::ApiBodyInput(_)
            | crate::ui_system::UiId::ApiResponseBody(_)
            | crate::ui_system::UiId::ApiMockStaticResponseInput(_) => {
                api_text_area_top_from_baseline(rect.1 + 29.0 * scale, scale)
            }
            _ => rect.1 + 10.0 * scale,
        }
    }

    fn api_multiline_cursor_left_x(
        id: crate::ui_system::UiId,
        rect: (f32, f32, f32, f32),
        scale: f32,
    ) -> f32 {
        match id {
            crate::ui_system::UiId::ApiMockSignatureInput(_) => rect.0,
            _ => rect.0 + 10.0 * scale,
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
                Self::api_multiline_cursor_left_x(id, rect, scale),
                Self::api_multiline_cursor_top_y(id, rect, scale),
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
                Self::api_multiline_cursor_left_x(id, rect, scale),
                Self::api_multiline_cursor_top_y(id, rect, scale),
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
            api_line_byte_at_x(renderer, &text, target_x, 0.88)
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
                Self::api_multiline_cursor_left_x(id, rect, scale),
                Self::api_multiline_cursor_top_y(id, rect, scale),
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
                Self::api_multiline_cursor_left_x(id, rect, scale),
                Self::api_multiline_cursor_top_y(id, rect, scale),
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
            api_line_byte_at_x(renderer, &text, target_x, 0.88)
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
        if !native_picker_can_start(&self.api_import_file_rx) {
            self.ide_panel.api.import_error = Some("Окно выбора OpenAPI уже открыто".to_string());
            return;
        }
        if crate::platform::native_dialog_requires_main_thread() {
            if let Some(path) = crate::platform::pick_file_with_filter(
                "Импорт openapi.json",
                "OpenAPI JSON",
                &["json"],
            ) {
                self.start_api_local_import(path);
            }
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.api_import_file_rx = Some(rx);
        let worker_tx = tx.clone();
        if let Err(err) = crate::platform::spawn_named("rriter-api-file-picker", move || {
            let file = crate::platform::pick_file_with_filter(
                "Импорт openapi.json", "OpenAPI JSON", &["json"],
            );
            let _ = worker_tx.send(file);
        }) {
            self.ide_panel.api.import_error = Some(format!("Не удалось открыть выбор OpenAPI: {err}"));
            self.api_import_file_rx = None;
        }
    }

    fn trigger_api_body_file_picker(
        &mut self,
        spec_id: ApiSpecId,
        route_idx: usize,
        name: String,
        multi: bool,
    ) {
        if !native_picker_can_start(&self.api_body_file_rx) {
            self.ide_panel.api.import_error = Some("Окно выбора body-файла уже открыто".to_string());
            return;
        }
        if crate::platform::native_dialog_requires_main_thread() {
            let paths = if multi {
                crate::platform::pick_files("Выбрать файл")
            } else {
                crate::platform::pick_file("Выбрать файл")
                    .into_iter()
                    .collect()
            };
            self.apply_api_body_file_pick(ApiBodyFilePickResult {
                spec_id,
                route_idx,
                name,
                paths,
            });
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.api_body_file_rx = Some(rx);
        let worker_tx = tx.clone();
        let fallback_name = name.clone();
        if let Err(err) = crate::platform::spawn_named("rriter-api-body-file-picker", move || {
            let paths = if multi {
                crate::platform::pick_files("Выбрать файл")
            } else {
                crate::platform::pick_file("Выбрать файл").into_iter().collect()
            };
            let _ = worker_tx.send(ApiBodyFilePickResult {
                spec_id, route_idx, name, paths,
            });
        }) {
            self.ide_panel.api.import_error = Some(format!("Не удалось открыть выбор body-файла: {err}"));
            self.api_body_file_rx = None;
            let _ = (spec_id, route_idx, fallback_name);
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn trigger_api_python_path_picker(&mut self, kind: ApiPythonPathPickKind) {
        if !native_picker_can_start(&self.ide_panel.api.python_path_pick_rx) {
            self.ide_panel.api.mock.uv.last_error = "Окно выбора Python/uv уже открыто".to_string();
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.ide_panel.api.python_path_pick_rx = Some(rx);
        let title = match kind {
            ApiPythonPathPickKind::Uv => "Выбрать исполняемый файл uv",
            ApiPythonPathPickKind::CustomPython => "Выбрать исполняемый файл Python",
        };
        if crate::platform::native_dialog_requires_main_thread() {
            let path = crate::platform::pick_file(title);
            let _ = tx.send(ApiPythonPathPickResult { kind, path });
            return;
        }
        let worker_tx = tx.clone();
        if let Err(err) = crate::platform::spawn_named("rriter-api-python-path-picker", move || {
            let path = crate::platform::pick_file(title);
            let _ = worker_tx.send(ApiPythonPathPickResult { kind, path });
        }) {
            self.ide_panel.api.mock.uv.last_error = format!("Не удалось открыть выбор пути: {err}");
            self.ide_panel.api.python_path_pick_rx = None;
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn trigger_api_python_version_list(&mut self) {
        if let Some(cancel) = self.ide_panel.api.python_version_list_cancel.take() {
            cancel.store(true, Ordering::Release);
        }
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
        let cancel = Arc::new(AtomicBool::new(false));
        self.ide_panel.api.python_version_list_cancel = Some(cancel.clone());
        let worker_tx = tx.clone();
        if let Err(err) = crate::platform::spawn_named("rriter-api-python-list", move || {
            let mut command = Command::new(uv_path);
            command.arg("python").arg("list").arg("--all-versions");
            let result = crate::platform::run_command_output_cancelable(
                &mut command,
                API_PYTHON_LIST_TIMEOUT,
                &cancel,
            );
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
                Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {
                    ApiPythonVersionListResult {
                        rows: Vec::new(),
                        error: Some("Получение списка версий Python отменено.".to_string()),
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::TimedOut => {
                    ApiPythonVersionListResult {
                        rows: Vec::new(),
                        error: Some("uv python list превысил лимит времени.".to_string()),
                    }
                }
                Err(err) => ApiPythonVersionListResult {
                    rows: Vec::new(),
                    error: Some(format!("Ошибка запуска uv: {err}")),
                },
            };
            let _ = worker_tx.send(payload);
        }) {
            let _ = tx.send(ApiPythonVersionListResult {
                rows: Vec::new(),
                error: Some(format!("не удалось запустить worker списка Python: {err}")),
            });
        }
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
        let cancel = Arc::new(AtomicBool::new(false));
        self.ide_panel.api.python_install_cancel = Some(cancel.clone());
        self.ide_panel.api.mock_python_install_running = true;
        self.ide_panel.api.mock_python_install_log.clear();
        self.ide_panel
            .api
            .mock_python_install_log
            .push(ApiPythonInstallLogLine {
                text: format!("uv python install {version}"),
                kind: ApiPythonInstallLogKind::Info,
            });
        let worker_tx = tx.clone();
        if let Err(err) = crate::platform::spawn_named("rriter-api-python-install", move || {
            let mut command = Command::new(uv_path);
            command.arg("python").arg("install").arg(&version);
            let result = crate::platform::run_command_streaming_cancelable(
                &mut command,
                API_PYTHON_INSTALL_TIMEOUT,
                &cancel,
                |stream, line| {
                    if line.trim().is_empty() {
                        return;
                    }
                    let kind = match stream {
                        crate::platform::ProcessOutputStream::Stdout => {
                            ApiPythonInstallLogKind::Info
                        }
                        crate::platform::ProcessOutputStream::Stderr => {
                            ApiPythonInstallLogKind::Error
                        }
                    };
                    let _ = worker_tx.send(ApiPythonInstallEvent::Line(ApiPythonInstallLogLine {
                        text: line,
                        kind,
                    }));
                },
            )
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::Interrupted => "Установка Python отменена.".to_string(),
                std::io::ErrorKind::TimedOut => {
                    "uv python install превысил лимит времени.".to_string()
                }
                _ => format!("Ошибка запуска uv: {error}"),
            })
            .and_then(|status| {
                if status.success() {
                    Ok(())
                } else {
                    Err(format!("uv завершился с кодом {:?}", status.code()))
                }
            });
            let _ = worker_tx.send(ApiPythonInstallEvent::Done(result));
        }) {
            let _ = tx.send(ApiPythonInstallEvent::Done(Err(format!(
                "не удалось запустить worker установки Python: {err}"
            ))));
        }
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
            state
                .body_file_paths
                .insert(result.name.clone(), result.paths.clone());
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
            self.ide_panel.api.input_editor.version = crate::editor::next_editor_version(old_version);
        }
    }

    pub fn start_api_local_import(&mut self, path: PathBuf) {
        let id = self.ide_panel.api.alloc_spec_id();
        let generation = self.ide_panel.api.begin_load(id, true);
        self.api_load_rx.push(crate::app::api_client::ApiLoadReceiver {
            id,
            generation,
            rx: spawn_load_local(id, generation, path),
        });
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
        let generation = self.ide_panel.api.begin_load(id, true);
        self.api_load_rx.push(crate::app::api_client::ApiLoadReceiver {
            id,
            generation,
            rx: spawn_load_url(id, generation, url),
        });
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
        let generation = self.ide_panel.api.begin_load(id, false);
        match entry.source {
            ApiSpecSource::Local(path) => self.api_load_rx.push(
                crate::app::api_client::ApiLoadReceiver {
                    id,
                    generation,
                    rx: spawn_load_local(id, generation, path),
                },
            ),
            ApiSpecSource::Url(url) => self.api_load_rx.push(
                crate::app::api_client::ApiLoadReceiver {
                    id,
                    generation,
                    rx: spawn_load_url(id, generation, url),
                },
            ),
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
        let generation = self.ide_panel.api.begin_load(id, false);
        match entry.source {
            ApiSpecSource::Local(path) => self.api_load_rx.push(
                crate::app::api_client::ApiLoadReceiver {
                    id,
                    generation,
                    rx: spawn_load_local(id, generation, path),
                },
            ),
            ApiSpecSource::Url(url) => self.api_load_rx.push(
                crate::app::api_client::ApiLoadReceiver {
                    id,
                    generation,
                    rx: spawn_load_cached_url(id, generation, url),
                },
            ),
        }
    }

    fn push_api_client_tab(&mut self, tab: crate::app::EditorTab, clear_highlighter_rx: bool) {
        if self.tabs.is_empty() {
            self.editor = Editor::new(16);
            self.file_path = None;
            self.file_key = None;
            self.text_file_format = crate::platform::TextFileFormat::default();
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
        if clear_highlighter_rx {
            while self.highlighter.rx.try_recv().is_ok() {}
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

    fn api_spec_title(&self, id: ApiSpecId) -> String {
        self.ide_panel
            .api
            .specs
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.title.clone())
            .unwrap_or_else(|| "API".to_string())
    }

    fn last_api_route_tab_idx(&self, id: ApiSpecId) -> Option<usize> {
        self.tabs.iter().rposition(|tab| {
            matches!(
                &tab.kind,
                crate::app::EditorTabKind::ApiClient(meta, state)
                    if meta.spec_id == id && !state.auth_view
            )
        })
    }

    fn open_new_api_spec_tab(&mut self, id: ApiSpecId) {
        let title = self.api_spec_title(id);
        let mut api_state = ApiClientTabState::default();
        let mut route_method = None;
        let mut route_path = String::new();
        if let Some(model) = self.ide_panel.api.models.get(&id)
            && let Some(route) = model.routes.first()
        {
            api_state.route_idx = Some(0);
            route_method = Some(route.method);
            route_path = route.path.clone();
            fill_api_tab_inputs(&mut api_state, route, model);
        }

        let tab = crate::app::EditorTab {
            editor: Editor::new(16),
            file_path: None,
            file_key: None,
            text_file_format: crate::platform::TextFileFormat::default(),
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
                    route_method,
                    route_path,
                },
                api_state,
            ),
        };

        self.push_api_client_tab(tab, true);
    }

    pub fn open_api_spec_tab(&mut self, id: ApiSpecId) {
        self.ide_panel.api.select_spec(id);
        self.ensure_api_model_loaded(id);
        self.refresh_api_mock_server_snapshot();

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

        self.open_new_api_spec_tab(id);
    }

    pub fn open_api_auth_tab(&mut self, id: ApiSpecId) {
        self.ide_panel.api.select_spec(id);
        self.ensure_api_model_loaded(id);
        self.refresh_api_mock_server_snapshot();
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
            file_key: None,
            text_file_format: crate::platform::TextFileFormat::default(),
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
                    route_method: None,
                    route_path: String::new(),
                },
                api_state,
            ),
        };

        self.push_api_client_tab(tab, false);
    }

    pub fn open_api_route(&mut self, spec_id: ApiSpecId, route_idx: usize) {
        self.open_api_route_with_new_tab(spec_id, route_idx, false);
    }

    pub fn open_api_route_with_new_tab(
        &mut self,
        spec_id: ApiSpecId,
        route_idx: usize,
        force_new_tab: bool,
    ) {
        self.ide_panel.api.select_spec(spec_id);
        self.ensure_api_model_loaded(spec_id);
        self.refresh_api_mock_server_snapshot();
        if force_new_tab {
            self.open_new_api_spec_tab(spec_id);
        } else if let Some(idx) = self.last_api_route_tab_idx(spec_id) {
            self.switch_to_tab(idx);
        } else {
            self.open_new_api_spec_tab(spec_id);
        }
        let mut needs_input_sync = false;
        let route_header = self
            .ide_panel
            .api
            .models
            .get(&spec_id)
            .and_then(|model| model.routes.get(route_idx))
            .map(|route| (route.method, route.path.clone()));
        if let Some((meta, state)) = self.active_api_tab_mut_for(spec_id) {
            state.remember_view_scroll();
            state.remember_route_state();
            state.auth_view = false;
            meta.route_identity = Some(ApiClientRouteIdentity::OpenApi { spec_id, route_idx });
            if let Some((method, path)) = route_header {
                meta.route_method = Some(method);
                meta.route_path = path;
            }
            if !state.restore_route_state(route_idx) {
                state.route_idx = Some(route_idx);
                state.route_text_selection = None;
                state.response = None;
                state.response_view = ApiResponseView::Body;
                state.input_doc_view = ApiInputDocView::Input;
                state.input_schema_idx = 0;
                state.input_schema_menu_open = false;
                state.output_doc_view = ApiOutputDocView::Example;
                state.output_status_idx = 0;
                state.output_example_idx = 0;
                state.output_schema_idx = 0;
                state.output_schema_menu_open = false;
                state.output_schema_menu_scroll.current = 0.0;
                state.output_schema_menu_scroll.target = 0.0;
                state.input_schema_collapsed.clear();
                state.output_schema_collapsed.clear();
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
                meta.title = "Mock".to_string();
                meta.route_method = Some(route.method);
                meta.route_path = route.path.clone();
                state.route_idx = Some(manual_idx);
                state.route_text_selection = None;
            }
            self.ide_panel
                .api
                .expanded_mock_routes
                .insert((API_MANUAL_MOCK_SPEC_ID, manual_idx));
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
        self.ide_panel
            .api
            .expanded_mock_routes
            .insert((API_MANUAL_MOCK_SPEC_ID, manual_idx));
        let tab = crate::app::EditorTab {
            editor: Editor::new(16),
            file_path: None,
            file_key: None,
            text_file_format: crate::platform::TextFileFormat::default(),
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
                    title: "Mock".to_string(),
                    route_identity: Some(ApiClientRouteIdentity::Manual { stable_id }),
                    route_method: Some(route.method),
                    route_path: route.path.clone(),
                },
                api_state,
            ),
        };

        if self.tabs.is_empty() {
            self.editor = Editor::new(16);
            self.file_path = None;
            self.file_key = None;
            self.text_file_format = crate::platform::TextFileFormat::default();
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
                    route.method,
                    route.path.clone(),
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
            if let Some((manual_idx, _, method, path, title)) =
                routes.iter().find(|(_, id, _, _, _)| id == stable_id)
            {
                meta.title = "Mock".to_string();
                meta.route_method = Some(*method);
                meta.route_path = path.clone();
                if state.route_idx != Some(*manual_idx) {
                    state.route_text_selection = None;
                }
                state.route_idx = Some(*manual_idx);
                tab.base_title = title.clone();
                if tab_idx == self.active_tab {
                    self.base_title = title.clone();
                }
            } else {
                meta.title = "Mock removed".to_string();
                meta.route_method = None;
                meta.route_path.clear();
                state.route_idx = None;
                state.route_text_selection = None;
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
            state.body_file_paths.clear();
            state.body_json = body_json;
            state.body_scroll.current = 0.0;
            state.body_scroll.target = 0.0;
            state.body_scroll_x.current = 0.0;
            state.body_scroll_x.target = 0.0;
            state.response_scroll.current = 0.0;
            state.response_scroll.target = 0.0;
            state.response_scroll_x.current = 0.0;
            state.response_scroll_x.target = 0.0;
            state.focused_schema_pane = None;
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
            self.stash_active_api_mock_editor();
        }
        if focus_changed || dynamic_readonly_focus {
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
                self.ide_panel.api.input_editor.version = crate::editor::next_editor_version(old_version);
            }
            self.ide_panel.api.input_editor.cursor = self.ide_panel.api.input_editor.len();
            self.ide_panel.api.input_editor.selection_anchor =
                Some(self.ide_panel.api.input_editor.cursor);
            self.ide_panel.api.input_scroll_x.current = 0.0;
            self.ide_panel.api.input_scroll_x.target = 0.0;
            self.ide_panel.api.input_scroll_x.velocity = 0.0;
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
        self.queue_api_body_json_validation();
    }

}
