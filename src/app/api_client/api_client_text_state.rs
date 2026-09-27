impl ApiClientState {
    fn set_input_cursor(&mut self, cursor: usize, keep_anchor: bool) {
        if !keep_anchor || self.input_editor.selection_anchor.is_none() {
            self.input_editor.selection_anchor = Some(if keep_anchor {
                self.input_editor.cursor
            } else {
                cursor
            });
        }
        self.input_editor.cursor = cursor;
    }

    fn api_multiline_text_for_ui(
        &self,
        id: crate::ui_system::UiId,
        spec_id: ApiSpecId,
        state: &ApiClientTabState,
        focused: Option<&ApiFocus>,
        input_editor: &crate::editor::Editor,
        active: Option<&ApiActiveRoute>,
    ) -> Option<String> {
        match id {
            crate::ui_system::UiId::ApiBodyScrollY(route_idx)
            | crate::ui_system::UiId::ApiBodyScrollX(route_idx)
            | crate::ui_system::UiId::ApiBodyInput(route_idx)
                if state.route_idx == Some(route_idx) =>
            {
                Some(if matches!(
                    focused,
                    Some(ApiFocus::Body { spec_id: focused_spec, route_idx: focused_route })
                        if *focused_spec == spec_id && *focused_route == route_idx
                ) {
                    input_editor.get_full_text()
                } else {
                    state.body_json.clone()
                })
            }
            crate::ui_system::UiId::ApiInputSchemaBody(route_idx)
                if state.route_idx == Some(route_idx) =>
            {
                Some(
                    self.api_mock_input_schema_text_for_focus_route(active, spec_id, route_idx)
                        .or_else(|| {
                            self.models.get(&spec_id).and_then(|model| {
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
                        .unwrap_or_default(),
                )
            }
            crate::ui_system::UiId::ApiOutputScrollY(route_idx)
            | crate::ui_system::UiId::ApiOutputScrollX(route_idx)
            | crate::ui_system::UiId::ApiOutputSchemaBody(route_idx)
                if state.route_idx == Some(route_idx) =>
            {
                Some(
                    self.models
                        .get(&spec_id)
                        .and_then(|model| {
                            model.routes.get(route_idx).map(|route| match state.output_doc_view {
                                ApiOutputDocView::Example => api_route_output_example_text_for(
                                    route,
                                    model,
                                    state.output_status_idx,
                                    state.output_example_idx,
                                ),
                                ApiOutputDocView::Schema => api_route_output_schema_text_for(
                                    route,
                                    model,
                                    state.output_status_idx,
                                    state.output_schema_idx,
                                    &state.output_schema_collapsed,
                                ),
                            })
                        })
                        .unwrap_or_default(),
                )
            }
            crate::ui_system::UiId::ApiMockStaticResponseScrollY(route_idx)
            | crate::ui_system::UiId::ApiMockStaticResponseScrollX(route_idx)
            | crate::ui_system::UiId::ApiMockStaticResponseInput(route_idx)
                if state.route_idx == Some(route_idx) =>
            {
                Some(if matches!(
                    focused,
                    Some(ApiFocus::MockStaticResponse { route_idx: focused_route })
                        if *focused_route == route_idx
                ) {
                    input_editor.get_full_text()
                } else {
                    self.active_manual_mock_route(active, route_idx)
                        .map(|route| &route.response)
                        .or_else(|| {
                            self.api_route_override_for_active(active, route_idx)
                                .map(|route| &route.response)
                        })
                        .map(|response| match response {
                            crate::app::api_mock::types::ApiMockResponse::Generated => self
                                .api_mock_generated_preview(active, route_idx)
                                .unwrap_or_else(|| "{}".to_string()),
                            crate::app::api_mock::types::ApiMockResponse::Json(text)
                            | crate::app::api_mock::types::ApiMockResponse::Text(text) => {
                                text.clone()
                            }
                        })
                        .unwrap_or_else(|| {
                            self.api_mock_generated_preview(active, route_idx)
                                .unwrap_or_else(|| "{}".to_string())
                        })
                })
            }
            crate::ui_system::UiId::ApiResponseScrollY(route_idx)
            | crate::ui_system::UiId::ApiResponseScrollX(route_idx)
            | crate::ui_system::UiId::ApiResponseBody(route_idx)
                if state.route_idx == Some(route_idx) =>
            {
                Some(if matches!(
                    focused,
                    Some(ApiFocus::Response {
                        spec_id: focused_spec,
                        route_idx: focused_route,
                    }) if *focused_spec == spec_id && *focused_route == route_idx
                ) {
                    input_editor.get_full_text()
                } else {
                    state
                        .response
                        .as_ref()
                        .map(|response| api_response_text(response, state.response_view).to_string())
                        .unwrap_or_default()
                })
            }
            crate::ui_system::UiId::ApiMockPreludeInput(route_idx) => self
                .api_route_python_script(active, route_idx)
                .map(|script| {
                    if self.api_mock_python_focus_target()
                        == Some((route_idx, ApiMockSourcePart::Prelude))
                    {
                        input_editor.get_full_text()
                    } else {
                        script.prelude.clone()
                    }
                }),
            crate::ui_system::UiId::ApiMockContractInput(route_idx) => self
                .api_route_python_script(active, route_idx)
                .map(|_| {
                    if self.api_mock_python_focus_target()
                        == Some((route_idx, ApiMockSourcePart::Contract))
                    {
                        input_editor.get_full_text()
                    } else {
                        self.api_mock_contract_source_for_route(active, route_idx)
                            .unwrap_or_default()
                    }
                }),
            crate::ui_system::UiId::ApiMockBodyInput(route_idx) => self
                .api_route_python_script(active, route_idx)
                .map(|script| {
                    if self.api_mock_python_focus_target()
                        == Some((route_idx, ApiMockSourcePart::Body))
                    {
                        input_editor.get_full_text()
                    } else {
                        api_mock_body_editor_text(&script.body)
                    }
                }),
            crate::ui_system::UiId::ApiMockSignatureInput(route_idx) => {
                self.api_mock_signature_for_route(active, route_idx)
            }
            _ => None,
        }
    }

    fn api_tab_scroll_for_ui(state: &ApiClientTabState, id: crate::ui_system::UiId) -> f32 {
        match id {
            crate::ui_system::UiId::ApiBodyInput(route_idx)
            | crate::ui_system::UiId::ApiInputSchemaBody(route_idx)
                if state.route_idx == Some(route_idx) => state.body_scroll.current,
            crate::ui_system::UiId::ApiOutputScrollX(route_idx)
            | crate::ui_system::UiId::ApiOutputSchemaBody(route_idx)
                if state.route_idx == Some(route_idx) => state.output_scroll.current,
            crate::ui_system::UiId::ApiMockStaticResponseScrollX(route_idx)
            | crate::ui_system::UiId::ApiMockStaticResponseInput(route_idx)
                if state.route_idx == Some(route_idx) => {
                    state.mock_static_response_scroll.current
                }
            crate::ui_system::UiId::ApiResponseBody(route_idx)
                if state.route_idx == Some(route_idx) => state.response_scroll.current,
            _ => 0.0,
        }
    }

    fn api_tab_scroll_x_for_ui(state: &ApiClientTabState, id: crate::ui_system::UiId) -> f32 {
        match id {
            crate::ui_system::UiId::ApiBodyInput(route_idx)
            | crate::ui_system::UiId::ApiInputSchemaBody(route_idx)
                if state.route_idx == Some(route_idx) => state.body_scroll_x.current,
            crate::ui_system::UiId::ApiOutputScrollX(route_idx)
            | crate::ui_system::UiId::ApiOutputSchemaBody(route_idx)
                if state.route_idx == Some(route_idx) => state.output_scroll_x.current,
            crate::ui_system::UiId::ApiMockStaticResponseScrollX(route_idx)
            | crate::ui_system::UiId::ApiMockStaticResponseInput(route_idx)
                if state.route_idx == Some(route_idx) => {
                    state.mock_static_response_scroll_x.current
                }
            crate::ui_system::UiId::ApiResponseBody(route_idx)
                if state.route_idx == Some(route_idx) => state.response_scroll_x.current,
            _ => 0.0,
        }
    }

    fn api_mock_scroll_x_for_ui(&self, id: crate::ui_system::UiId) -> f32 {
        let Some((route_idx, part)) = Self::api_mock_part_for_ui(id) else {
            return 0.0;
        };
        self.mock_python_scrolls_x
            .get(&(route_idx, part))
            .map(|scroll| scroll.current)
            .unwrap_or(0.0)
    }

    fn sync_api_tab_inputs(
        &self,
        spec_id: ApiSpecId,
        state: &mut ApiClientTabState,
        route_idx: usize,
    ) -> bool {
        let Some(model) = self.models.get(&spec_id) else {
            return false;
        };
        let Some(route) = model.routes.get(route_idx) else {
            return false;
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
        state.path_values = path_values;
        state.query_values = query_values;
        state.body_values = body_values;
        state.body_file_paths.clear();
        state.body_json = body_json;
        state.body_scroll.reset();
        state.body_scroll_x.reset();
        state.output_scroll.reset();
        state.output_scroll_x.reset();
        state.mock_static_response_scroll.reset();
        state.mock_static_response_scroll_x.reset();
        state.response_scroll.reset();
        state.response_scroll_x.reset();
        state.focused_schema_pane = None;
        true
    }
    fn api_route_text_for_tab(
        &self,
        spec_id: ApiSpecId,
        route_idx: Option<usize>,
        identity: Option<&ApiClientRouteIdentity>,
        field: ApiRouteTextField,
    ) -> Option<String> {
        match identity {
            Some(ApiClientRouteIdentity::Manual { stable_id }) => {
                let route = self
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
                .models
                .get(spec_id)
                .and_then(|model| model.routes.get(*route_idx))
                .map(|route| Self::api_route_row_text(route, field)),
            None => self
                .models
                .get(&spec_id)
                .and_then(|model| route_idx.and_then(|idx| model.routes.get(idx)))
                .map(|route| Self::api_route_row_text(route, field)),
        }
    }

    fn api_spec_title(&self, id: ApiSpecId) -> String {
        self.specs
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.title.clone())
            .unwrap_or_else(|| "API".to_string())
    }

    fn apply_api_body_file_pick(
        &mut self,
        state: Option<&mut ApiClientTabState>,
        result: ApiBodyFilePickResult,
    ) {
        if result.paths.is_empty() {
            return;
        }
        let new_value = result
            .paths
            .iter()
            .map(|path| path.to_string_lossy())
            .collect::<Vec<_>>()
            .join("\n");
        if let Some(state) = state
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
            self.focused,
            Some(ApiFocus::BodyField {
                spec_id,
                route_idx,
                ref name,
            }) if spec_id == result.spec_id && route_idx == result.route_idx && name == &result.name
        ) {
            let old_version = self.input_editor.version;
            self.input_editor.set_text_clean(&new_value);
            self.input_editor.version = crate::editor::next_editor_version(old_version);
        }
    }

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

    fn api_one_line_text_scale_for_ui(id: crate::ui_system::UiId) -> f32 {
        match id {
            crate::ui_system::UiId::ApiRouteFilterInput => 0.78,
            crate::ui_system::UiId::ApiMockPythonUvPathInput
            | crate::ui_system::UiId::ApiMockPythonCustomPathInput => {
                crate::app::file_tree::FILE_TREE_DIALOG_INPUT_TEXT_SCALE
            }
            _ => crate::render_view::api_client_tab::API_ONE_LINE_INPUT_SCALE,
        }
    }

    fn api_mock_part_for_ui(
        id: crate::ui_system::UiId,
    ) -> Option<(usize, ApiMockSourcePart)> {
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

    fn api_mock_combined_max_scroll_for_route(
        &self,
        active: Option<&ApiActiveRoute>,
        route_idx: usize,
        scale: f32,
    ) -> f32 {
        let Some((_, _, route, model)) = self.api_mock_route_context(active, route_idx) else {
            return 0.0;
        };
        let Some(script) = self.api_mock_script_for_tools(active, route_idx) else {
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
            self.input_editor.get_full_text()
        } else {
            self.api_mock_contract_source_for_route(active, route_idx)
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

    fn api_focus_ui_target(
        &self,
        focus: &ApiFocus,
    ) -> Option<(crate::ui_system::UiId, bool)> {
        match focus {
            ApiFocus::ImportUrl => Some((crate::ui_system::UiId::ApiImportUrlInput, false)),
            ApiFocus::RouteFilter => Some((crate::ui_system::UiId::ApiRouteFilterInput, false)),
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
                    .models
                    .get(spec_id)?
                    .routes
                    .get(*route_idx)?
                    .path_params
                    .iter()
                    .position(|param| param.name == *name)?;
                Some((crate::ui_system::UiId::ApiPathParamInput(*route_idx, idx), false))
            }
            ApiFocus::QueryParam {
                spec_id,
                route_idx,
                name,
            } => {
                let idx = self
                    .models
                    .get(spec_id)?
                    .routes
                    .get(*route_idx)?
                    .query_params
                    .iter()
                    .position(|param| param.name == *name)?;
                Some((crate::ui_system::UiId::ApiQueryParamInput(*route_idx, idx), false))
            }
            ApiFocus::BodyField {
                spec_id,
                route_idx,
                name,
            } => {
                let model = self.models.get(spec_id)?;
                let route = model.routes.get(*route_idx)?;
                let root = route.request_body.as_ref()?.schema?;
                let idx = model
                    .schema_arena
                    .get(root.0)?
                    .properties
                    .iter()
                    .position(|prop| prop.name == *name)?;
                Some((crate::ui_system::UiId::ApiBodyFieldInput(*route_idx, idx), false))
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

    fn queue_api_body_json_validation(&mut self) {
        let Some(ApiFocus::Body { spec_id, route_idx }) = self.focused else {
            return;
        };
        let version = self.input_editor.version;
        if self
            .body_json_validation
            .is_some_and(|state| {
                state.spec_id == spec_id && state.route_idx == route_idx && state.version == version
            })
            || self.body_json_validation_pending == Some((spec_id, route_idx, version))
        {
            return;
        }
        let text = self.input_editor.get_full_text();
        let (tx, rx) = mpsc::channel();
        self.body_json_validation_pending = Some((spec_id, route_idx, version));
        self.body_json_validation_rx = Some(rx);
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

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn trigger_api_python_version_list(&mut self) {
        if let Some(cancel) = self.python_version_list_cancel.take() {
            cancel.store(true, Ordering::Release);
        }
        let Some(uv_path) = self.mock.uv.selected_uv_path() else {
            self.mock.uv.status = crate::app::api_mock::types::ApiPythonRuntimeStatus::Missing;
            self.mock.uv.last_error = "uv не найден. Укажите путь к uv.".to_string();
            return;
        };
        let (tx, rx) = mpsc::channel();
        self.python_version_list_rx = Some(rx);
        self.mock_python_versions_loading = true;
        self.mock_python_version_picker_open = true;
        self.mock_python_versions_scroll.reset();
        let cancel = Arc::new(AtomicBool::new(false));
        self.python_version_list_cancel = Some(cancel.clone());
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
        if self.mock_python_install_running {
            return;
        }
        let Some(uv_path) = self.mock.uv.selected_uv_path() else {
            self.mock.uv.last_error = "uv не найден. Укажите путь к uv.".to_string();
            return;
        };
        let version = self.mock.uv.python_version.trim().to_string();
        if version.is_empty() {
            self.mock.uv.last_error = "Выберите версию Python.".to_string();
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.python_install_rx = Some(rx);
        let cancel = Arc::new(AtomicBool::new(false));
        self.python_install_cancel = Some(cancel.clone());
        self.mock_python_install_running = true;
        self.mock_python_install_log.clear();
        self.mock_python_install_log.push(ApiPythonInstallLogLine {
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
}

impl ApiClientTabState {
    fn begin_route_text_selection(
        &mut self,
        route_idx: usize,
        field: ApiRouteTextField,
        byte: usize,
    ) -> bool {
        if self.route_idx != Some(route_idx) {
            return false;
        }
        self.route_text_selection = Some(ApiRouteTextSelection {
            field,
            anchor: byte,
            cursor: byte,
            selecting: true,
        });
        true
    }

    fn drag_route_text_selection(&mut self, byte: usize) -> bool {
        let Some(selection) = self.route_text_selection.as_mut() else {
            return false;
        };
        selection.cursor = byte;
        true
    }

    fn finish_route_text_selection(&mut self) -> bool {
        let Some(selection) = self.route_text_selection.as_mut() else {
            return false;
        };
        if !selection.selecting {
            return false;
        }
        selection.selecting = false;
        true
    }

    fn selected_route_text<'a>(&self, text: &'a str) -> Option<&'a str> {
        api_route_selected_text(self.route_text_selection?, text)
    }

    fn sync_multiline_scroll_target(
        &mut self,
        id: crate::ui_system::UiId,
        target_y: f32,
        target_x: f32,
        immediate: bool,
    ) -> bool {
        let (route_idx, scroll_y, scroll_x) = match id {
            crate::ui_system::UiId::ApiBodyInput(route_idx)
            | crate::ui_system::UiId::ApiInputSchemaBody(route_idx) => {
                (route_idx, &mut self.body_scroll, &mut self.body_scroll_x)
            }
            crate::ui_system::UiId::ApiOutputSchemaBody(route_idx) => {
                (route_idx, &mut self.output_scroll, &mut self.output_scroll_x)
            }
            crate::ui_system::UiId::ApiMockStaticResponseInput(route_idx) => (
                route_idx,
                &mut self.mock_static_response_scroll,
                &mut self.mock_static_response_scroll_x,
            ),
            crate::ui_system::UiId::ApiResponseBody(route_idx) => {
                (route_idx, &mut self.response_scroll, &mut self.response_scroll_x)
            }
            _ => return false,
        };
        if self.route_idx != Some(route_idx) {
            return true;
        }
        scroll_y.animate_to(target_y);
        scroll_x.animate_to(target_x);
        if immediate {
            scroll_y.jump_to(target_y);
            scroll_x.jump_to(target_x);
        }
        true
    }

    fn scrollbar_drag_offset(&self, id: crate::ui_system::UiId, route_idx: usize) -> Option<f32> {
        if self.route_idx != Some(route_idx) {
            return None;
        }
        let scroll = match id {
            crate::ui_system::UiId::ApiBodyScrollX(_) => &self.body_scroll_x,
            crate::ui_system::UiId::ApiOutputScrollX(_) => &self.output_scroll_x,
            crate::ui_system::UiId::ApiMockStaticResponseScrollX(_) => {
                &self.mock_static_response_scroll_x
            }
            crate::ui_system::UiId::ApiResponseScrollX(_) => &self.response_scroll_x,
            crate::ui_system::UiId::ApiBodyScrollY(_) => &self.body_scroll,
            crate::ui_system::UiId::ApiOutputScrollY(_) => &self.output_scroll,
            crate::ui_system::UiId::ApiMockStaticResponseScrollY(_) => {
                &self.mock_static_response_scroll
            }
            crate::ui_system::UiId::ApiResponseScrollY(_) => &self.response_scroll,
            _ => return None,
        };
        Some(scroll.drag_offset)
    }

    fn drag_text_scrollbar(
        &mut self,
        id: crate::ui_system::UiId,
        route_idx: usize,
        rect: (f32, f32, f32, f32),
        max_scroll: f32,
        pointer: f32,
        scale: f32,
        previous_drag_offset: Option<f32>,
    ) -> bool {
        if self.route_idx != Some(route_idx) {
            return false;
        }
        let horizontal = matches!(
            id,
            crate::ui_system::UiId::ApiBodyScrollX(_)
                | crate::ui_system::UiId::ApiOutputScrollX(_)
                | crate::ui_system::UiId::ApiMockStaticResponseScrollX(_)
                | crate::ui_system::UiId::ApiResponseScrollX(_)
        );
        let scroll = match id {
            crate::ui_system::UiId::ApiBodyScrollX(_) => &mut self.body_scroll_x,
            crate::ui_system::UiId::ApiOutputScrollX(_) => &mut self.output_scroll_x,
            crate::ui_system::UiId::ApiMockStaticResponseScrollX(_) => {
                &mut self.mock_static_response_scroll_x
            }
            crate::ui_system::UiId::ApiResponseScrollX(_) => &mut self.response_scroll_x,
            crate::ui_system::UiId::ApiBodyScrollY(_) => &mut self.body_scroll,
            crate::ui_system::UiId::ApiOutputScrollY(_) => &mut self.output_scroll,
            crate::ui_system::UiId::ApiMockStaticResponseScrollY(_) => {
                &mut self.mock_static_response_scroll
            }
            crate::ui_system::UiId::ApiResponseScrollY(_) => &mut self.response_scroll,
            _ => return false,
        };
        let target = if horizontal {
            api_text_scrollbar_x_drag_target(
                rect,
                scroll.current,
                max_scroll,
                pointer,
                scale,
                previous_drag_offset,
            )
        } else {
            api_text_scrollbar_y_drag_target(
                rect,
                scroll.current,
                max_scroll,
                pointer,
                scale,
                previous_drag_offset,
            )
        };
        let Some((drag_offset, target)) = target else {
            if previous_drag_offset.is_some() {
                scroll.end_drag();
            }
            return false;
        };
        crate::app::mouse::apply_scrollbar_drag_target(scroll, target, drag_offset);
        true
    }
}
