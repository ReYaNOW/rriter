impl crate::app::App {
    fn focus_next_api_input(&mut self, reverse: bool) -> bool {
        let Some((meta, state)) = self.active_api_tab() else {
            return false;
        };
        let spec_id = meta.spec_id;
        let Some(model) = self.ide_panel.api.models.get(&spec_id) else {
            return false;
        };
        let order = api_focus_order_for_view(spec_id, model, state);
        if order.is_empty() {
            return false;
        }
        let current = self.ide_panel.api.focused.clone();
        let current_idx = current
            .as_ref()
            .and_then(|focus| order.iter().position(|item| item == focus));
        let next_idx = if reverse {
            current_idx
                .unwrap_or(0)
                .checked_sub(1)
                .unwrap_or(order.len() - 1)
        } else {
            current_idx.map(|idx| (idx + 1) % order.len()).unwrap_or(0)
        };
        let next = order[next_idx].clone();
        self.focus_api_input(next);
        self.sync_api_one_line_scroll_target(true);
        true
    }

    fn api_mock_input_schema_text_for_focus_route(
        &self,
        spec_id: ApiSpecId,
        route_idx: usize,
    ) -> Option<String> {
        if spec_id == API_MANUAL_MOCK_SPEC_ID {
            let manual_route = self.active_manual_mock_route(route_idx)?;
            let script = manual_route
                .python
                .as_ref()
                .filter(|script| script.enabled)?;
            let model = api_manual_route_model(manual_route);
            let route = model.routes.first()?;
            let contract =
                crate::app::api_mock::types::api_mock_effective_contract(script, route, &model);
            return Some(api_mock_input_schema_text(&contract));
        }
        let model = self.ide_panel.api.models.get(&spec_id)?;
        let route = model.routes.get(route_idx)?;
        let script = self
            .active_manual_mock_route(route_idx)
            .and_then(|route| route.python.as_ref())
            .or_else(|| {
                self.api_route_override(route_idx)
                    .and_then(|route| route.python.as_ref())
            })
            .filter(|script| script.enabled)?;
        let contract =
            crate::app::api_mock::types::api_mock_effective_contract(script, route, model);
        Some(api_mock_input_schema_text(&contract))
    }

    fn api_focus_text(&self, focus: &ApiFocus) -> String {
        match focus {
            ApiFocus::ImportUrl => self.ide_panel.api.input_editor.get_full_text(),
            ApiFocus::RouteFilter => self.ide_panel.api.route_filter.clone(),
            ApiFocus::MockProxyBase => self.ide_panel.api.mock.proxy_base_url.clone(),
            ApiFocus::MockPythonUvPath => self
                .ide_panel
                .api
                .mock
                .uv
                .selected_uv_path()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default(),
            ApiFocus::MockPythonVersion => self.ide_panel.api.mock.uv.python_version.clone(),
            ApiFocus::MockPythonCustomPath => self
                .ide_panel
                .api
                .mock
                .uv
                .custom_python_path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default(),
            ApiFocus::MockManualPath { manual_idx } => self
                .ide_panel
                .api
                .mock
                .manual_routes
                .get(*manual_idx)
                .map(|route| route.path.clone())
                .unwrap_or_default(),
            ApiFocus::MockContract { route_idx } => self
                .api_mock_contract_source_for_route(*route_idx)
                .unwrap_or_default(),
            ApiFocus::MockPrelude { route_idx } => self
                .api_route_python_script(*route_idx)
                .map(|script| script.prelude.clone())
                .unwrap_or_default(),
            ApiFocus::MockBody { route_idx } => self
                .api_route_python_script(*route_idx)
                .map(|script| api_mock_body_editor_text(&script.body))
                .unwrap_or_default(),
            ApiFocus::MockSignature { route_idx } => self
                .api_mock_signature_for_route(*route_idx)
                .unwrap_or_default(),
            ApiFocus::MockStaticResponse { route_idx } => self
                .active_manual_mock_route(*route_idx)
                .map(|route| &route.response)
                .or_else(|| {
                    self.api_route_override(*route_idx)
                        .map(|route| &route.response)
                })
                .map(|response| match response {
                    crate::app::api_mock::types::ApiMockResponse::Generated => self
                        .api_mock_generated_preview(*route_idx)
                        .unwrap_or_else(|| "{}".to_string()),
                    crate::app::api_mock::types::ApiMockResponse::Json(text)
                    | crate::app::api_mock::types::ApiMockResponse::Text(text) => text.clone(),
                })
                .unwrap_or_else(|| {
                    self.api_mock_generated_preview(*route_idx)
                        .unwrap_or_else(|| "{}".to_string())
                }),
            ApiFocus::MockContractField {
                route_idx,
                group,
                field_idx,
                prop,
            } => self.api_mock_contract_field_prop_text(*route_idx, *group, *field_idx, *prop),
            ApiFocus::AuthValue { spec_id, scheme } => self
                .ide_panel
                .api
                .auth
                .entry(*spec_id, scheme)
                .map(|entry| {
                    if !entry.access_token.is_empty() {
                        entry.access_token.clone()
                    } else {
                        entry.value.clone()
                    }
                })
                .unwrap_or_default(),
            ApiFocus::AuthRefreshToken { spec_id, scheme } => self
                .ide_panel
                .api
                .auth
                .entry(*spec_id, scheme)
                .map(|entry| entry.refresh_token.clone())
                .unwrap_or_default(),
            ApiFocus::AuthUsername { spec_id, scheme } => self
                .ide_panel
                .api
                .auth
                .entry(*spec_id, scheme)
                .map(|entry| entry.username.clone())
                .unwrap_or_default(),
            ApiFocus::AuthPassword { spec_id, scheme } => self
                .ide_panel
                .api
                .auth
                .entry(*spec_id, scheme)
                .map(|entry| entry.password.clone())
                .unwrap_or_default(),
            ApiFocus::PathParam {
                spec_id,
                route_idx,
                name,
            } => self
                .tabs
                .get(self.active_tab)
                .and_then(|tab| match &tab.kind {
                    crate::app::EditorTabKind::ApiClient(meta, state)
                        if meta.spec_id == *spec_id && state.route_idx == Some(*route_idx) =>
                    {
                        state.path_values.iter().find(|v| v.name == *name)
                    }
                    _ => None,
                })
                .map(|v| v.value.clone())
                .unwrap_or_default(),
            ApiFocus::QueryParam {
                spec_id,
                route_idx,
                name,
            } => self
                .tabs
                .get(self.active_tab)
                .and_then(|tab| match &tab.kind {
                    crate::app::EditorTabKind::ApiClient(meta, state)
                        if meta.spec_id == *spec_id && state.route_idx == Some(*route_idx) =>
                    {
                        state.query_values.iter().find(|v| v.name == *name)
                    }
                    _ => None,
                })
                .map(|v| v.value.clone())
                .unwrap_or_default(),
            ApiFocus::BodyField {
                spec_id,
                route_idx,
                name,
            } => self
                .tabs
                .get(self.active_tab)
                .and_then(|tab| match &tab.kind {
                    crate::app::EditorTabKind::ApiClient(meta, state)
                        if meta.spec_id == *spec_id && state.route_idx == Some(*route_idx) =>
                    {
                        state.body_values.iter().find(|v| v.name == *name)
                    }
                    _ => None,
                })
                .map(|v| v.value.clone())
                .unwrap_or_default(),
            ApiFocus::Body { spec_id, route_idx } => self
                .tabs
                .get(self.active_tab)
                .and_then(|tab| match &tab.kind {
                    crate::app::EditorTabKind::ApiClient(meta, state)
                        if meta.spec_id == *spec_id && state.route_idx == Some(*route_idx) =>
                    {
                        Some(state.body_json.clone())
                    }
                    _ => None,
                })
                .unwrap_or_default(),
            ApiFocus::InputSchema { spec_id, route_idx } => self
                .api_mock_input_schema_text_for_focus_route(*spec_id, *route_idx)
                .or_else(|| {
                    self.tabs
                        .get(self.active_tab)
                        .and_then(|tab| match &tab.kind {
                            crate::app::EditorTabKind::ApiClient(meta, state)
                                if meta.spec_id == *spec_id
                                    && state.route_idx == Some(*route_idx) =>
                            {
                                self.ide_panel.api.models.get(spec_id).and_then(|model| {
                                    model.routes.get(*route_idx).map(|route| {
                                        api_route_input_schema_text(
                                            route,
                                            model,
                                            state.input_schema_idx,
                                            &state.input_schema_collapsed,
                                        )
                                    })
                                })
                            }
                            _ => None,
                        })
                })
                .unwrap_or_default(),
            ApiFocus::OutputSchema { spec_id, route_idx } => self
                .tabs
                .get(self.active_tab)
                .and_then(|tab| match &tab.kind {
                    crate::app::EditorTabKind::ApiClient(meta, state)
                        if meta.spec_id == *spec_id && state.route_idx == Some(*route_idx) =>
                    {
                        self.ide_panel.api.models.get(spec_id).and_then(|model| {
                            model
                                .routes
                                .get(*route_idx)
                                .map(|route| match state.output_doc_view {
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
                    }
                    _ => None,
                })
                .unwrap_or_default(),
            ApiFocus::Response { spec_id, route_idx } => self
                .tabs
                .get(self.active_tab)
                .and_then(|tab| match &tab.kind {
                    crate::app::EditorTabKind::ApiClient(meta, state)
                        if meta.spec_id == *spec_id && state.route_idx == Some(*route_idx) =>
                    {
                        state.response.as_ref().map(|response| {
                            api_response_text(response, state.response_view).to_string()
                        })
                    }
                    _ => None,
                })
                .unwrap_or_default(),
        }
    }

    fn api_mock_generated_preview(&self, route_idx: usize) -> Option<String> {
        let (_, _, route, model) = self.api_mock_route_context(route_idx)?;
        Some(api_generated_response_for_route(&route, &model).2)
    }

    fn apply_response_token_to_auth(
        &mut self,
        route_idx: usize,
        scheme_idx: usize,
        save_access: bool,
        save_refresh: bool,
    ) {
        let Some((meta, state)) = self.active_api_tab() else {
            return;
        };
        if state.route_idx != Some(route_idx) {
            return;
        }
        let spec_id = meta.spec_id;
        let Some(response) = state.response.as_ref() else {
            return;
        };
        let Ok(json) = serde_json::from_str::<Value>(&response.body) else {
            return;
        };
        let access_token = json.get("access_token").and_then(Value::as_str);
        let refresh_token = json.get("refresh_token").and_then(Value::as_str);
        if (!save_access || access_token.is_none()) && (!save_refresh || refresh_token.is_none()) {
            return;
        }
        let token_type = json
            .get("token_type")
            .and_then(Value::as_str)
            .unwrap_or("Bearer")
            .to_string();
        let expires_at = json
            .get("expires_in")
            .and_then(Value::as_u64)
            .map(|secs| now_epoch_secs().saturating_add(secs));
        let Some(scheme_name) = self
            .ide_panel
            .api
            .models
            .get(&spec_id)
            .and_then(|model| model.security_schemes.get(scheme_idx))
            .filter(|scheme| scheme.token_capable())
            .map(|scheme| scheme.name.clone())
        else {
            return;
        };
        let entry = self.ide_panel.api.auth.entry_mut(spec_id, &scheme_name);
        if save_access && let Some(token) = access_token {
            entry.access_token = token.to_string();
            entry.value = token.to_string();
        }
        if save_refresh && let Some(token) = refresh_token {
            entry.refresh_token = token.to_string();
            entry.value = token.to_string();
        }
        entry.token_type = token_type;
        entry.expires_at = expires_at;
        self.ide_panel.api.persist();
    }

    pub fn commit_api_focus(&mut self) {
        let Some(focus) = self.ide_panel.api.focused.clone() else {
            return;
        };
        let mut text = self.ide_panel.api.input_editor.get_full_text();
        if self.api_focus_is_array_input(&focus) {
            text = split_api_array_values(&text).join("\n");
        }
        match focus {
            ApiFocus::ImportUrl => {}
            ApiFocus::RouteFilter => {
                self.ide_panel.api.route_filter = text;
            }
            ApiFocus::MockProxyBase => {
                self.ide_panel.api.mock.proxy_base_url = text.trim().to_string();
                self.ide_panel.api.persist();
            }
            ApiFocus::MockPythonUvPath => {
                self.ide_panel.api.mock.uv.configured_path = non_empty_path(&text);
                self.ide_panel.api.persist();
            }
            ApiFocus::MockPythonVersion => {
                let version = text.trim();
                self.ide_panel.api.mock.uv.python_version = if version.is_empty() {
                    "3.13".to_string()
                } else {
                    version.to_string()
                };
                self.ide_panel.api.persist();
            }
            ApiFocus::MockPythonCustomPath => {
                self.ide_panel.api.mock.uv.custom_python_path = non_empty_path(&text);
                self.ide_panel.api.persist();
            }
            ApiFocus::MockManualPath { manual_idx } => {
                let mut path = text.trim().to_string();
                if !path.starts_with('/') {
                    path.insert(0, '/');
                }
                let mut contract_path_changed = false;
                if let Some(route) = self.ide_panel.api.mock.manual_routes.get_mut(manual_idx) {
                    route.path = if path == "/" {
                        format!("/mock-{}", manual_idx.saturating_add(1))
                    } else {
                        path
                    };
                    if let Some(script) = route.python.as_mut() {
                        if script.contract.is_empty() {
                            script.contract =
                                crate::app::api_mock::types::default_contract_for_manual_route(
                                    &route.path,
                                );
                        } else {
                            crate::app::api_mock::types::sync_contract_path_params_from_path(
                                &mut script.contract,
                                &route.path,
                            );
                        }
                        script.contract_source =
                            crate::app::api_mock::contract::api_mock_contract_state_text(
                                &script.contract,
                            );
                        contract_path_changed = true;
                    }
                    self.sync_api_manual_route_tabs();
                    self.ide_panel.api.persist();
                    if contract_path_changed {
                        self.invalidate_api_mock_contract_tools(manual_idx);
                    }
                    self.refresh_api_mock_server_snapshot();
                }
            }
            ApiFocus::MockContract { route_idx } => {
                let Some((_, _, route, model)) = self.api_mock_route_context(route_idx) else {
                    return;
                };
                let default_contract =
                    crate::app::api_mock::types::default_contract_from_route(&route, &model);
                if let Some(script) = self.api_route_python_script_mut(route_idx) {
                    let base = if script.contract.is_empty() {
                        default_contract
                    } else {
                        script.contract.clone()
                    };
                    let contract =
                        crate::app::api_mock::contract::api_mock_contract_from_state_text(
                            &base, &text,
                        );
                    let generated =
                        crate::app::api_mock::contract::api_mock_contract_state_text(&contract);
                    let contract_source = if text.trim() == generated.trim() {
                        String::new()
                    } else {
                        text
                    };
                    let changed = base != contract || script.contract_source != contract_source;
                    if changed {
                        script.contract = contract;
                        script.contract_source = contract_source;
                        self.ide_panel.api.persist();
                        self.invalidate_api_mock_contract_tools(route_idx);
                    }
                }
            }
            ApiFocus::MockPrelude { route_idx } => {
                if let Some(script) = self.api_route_python_script_mut(route_idx) {
                    script.prelude = text;
                    self.ide_panel.api.persist();
                }
            }
            ApiFocus::MockBody { route_idx } => {
                if let Some(script) = self.api_route_python_script_mut(route_idx) {
                    script.body = text;
                    self.ide_panel.api.persist();
                }
            }
            ApiFocus::MockSignature { .. } => {}
            ApiFocus::MockStaticResponse { route_idx } => {
                let generated = self
                    .api_mock_generated_preview(route_idx)
                    .unwrap_or_else(|| "{}".to_string());
                if let Some(route) = self.active_manual_mock_route_mut(route_idx) {
                    route.enabled = true;
                    route.response = if text.trim() == generated.trim() {
                        crate::app::api_mock::types::ApiMockResponse::Generated
                    } else {
                        crate::app::api_mock::types::ApiMockResponse::Json(text)
                    };
                    self.ide_panel.api.persist();
                    self.refresh_api_mock_server_snapshot();
                    return;
                }
                self.ensure_api_route_override(route_idx);
                if let Some(override_route) = self.api_route_override_mut(route_idx) {
                    let was_enabled = override_route.enabled;
                    override_route.response = if text.trim() == generated.trim() {
                        crate::app::api_mock::types::ApiMockResponse::Generated
                    } else {
                        crate::app::api_mock::types::ApiMockResponse::Json(text)
                    };
                    override_route.enabled = was_enabled;
                    self.ide_panel.api.persist();
                }
            }
            ApiFocus::MockContractField {
                route_idx,
                group,
                field_idx,
                prop,
            } => {
                self.commit_api_mock_contract_field_prop(route_idx, group, field_idx, prop, &text);
            }
            ApiFocus::AuthValue { spec_id, scheme } => {
                self.ide_panel.api.auth.set_value(spec_id, &scheme, text);
                self.ide_panel.api.persist();
            }
            ApiFocus::AuthRefreshToken { spec_id, scheme } => {
                self.ide_panel
                    .api
                    .auth
                    .entry_mut(spec_id, &scheme)
                    .refresh_token = text;
                self.ide_panel.api.persist();
            }
            ApiFocus::AuthUsername { spec_id, scheme } => {
                self.ide_panel.api.auth.entry_mut(spec_id, &scheme).username = text;
                self.ide_panel.api.persist();
            }
            ApiFocus::AuthPassword { spec_id, scheme } => {
                self.ide_panel.api.auth.entry_mut(spec_id, &scheme).password = text;
                self.ide_panel.api.persist();
            }
            ApiFocus::PathParam {
                spec_id,
                route_idx,
                name,
            } => {
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id)
                    && state.route_idx == Some(route_idx)
                    && let Some(value) = state.path_values.iter_mut().find(|v| v.name == name)
                {
                    value.value = text;
                }
            }
            ApiFocus::QueryParam {
                spec_id,
                route_idx,
                name,
            } => {
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id)
                    && state.route_idx == Some(route_idx)
                    && let Some(value) = state.query_values.iter_mut().find(|v| v.name == name)
                {
                    value.value = text;
                }
            }
            ApiFocus::BodyField {
                spec_id,
                route_idx,
                name,
            } => {
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id)
                    && state.route_idx == Some(route_idx)
                    && let Some(value) = state.body_values.iter_mut().find(|v| v.name == name)
                {
                    value.value = text;
                    state.body_file_paths.remove(&name);
                }
            }
            ApiFocus::Body { spec_id, route_idx } => {
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id)
                    && state.route_idx == Some(route_idx)
                {
                    state.body_json = text;
                }
            }
            ApiFocus::InputSchema { .. } | ApiFocus::OutputSchema { .. } => {}
            ApiFocus::Response { .. } => {}
        }
    }

    fn api_focus_is_array_input(&self, focus: &ApiFocus) -> bool {
        match focus {
            ApiFocus::PathParam {
                spec_id,
                route_idx,
                name,
            } => self
                .ide_panel
                .api
                .models
                .get(spec_id)
                .and_then(|model| model.routes.get(*route_idx))
                .and_then(|route| route.path_params.iter().find(|param| param.name == *name))
                .is_some_and(|param| matches!(param.primitive_type, ApiPrimitiveType::Array)),
            ApiFocus::QueryParam {
                spec_id,
                route_idx,
                name,
            } => self
                .ide_panel
                .api
                .models
                .get(spec_id)
                .and_then(|model| model.routes.get(*route_idx))
                .and_then(|route| route.query_params.iter().find(|param| param.name == *name))
                .is_some_and(|param| matches!(param.primitive_type, ApiPrimitiveType::Array)),
            ApiFocus::BodyField {
                spec_id,
                route_idx,
                name,
            } => self
                .ide_panel
                .api
                .models
                .get(spec_id)
                .and_then(|model| model.routes.get(*route_idx).map(|route| (model, route)))
                .and_then(|(model, route)| {
                    let root = route.request_body.as_ref()?.schema?;
                    let prop = model
                        .schema_arena
                        .get(root.0)?
                        .properties
                        .iter()
                        .find(|prop| prop.name == *name)?;
                    model.schema_arena.get(prop.schema.0)
                })
                .is_some_and(api_schema_is_array_input),
            ApiFocus::MockContractField { prop, .. } => {
                matches!(prop, crate::ui_system::ApiMockContractFieldProp::Enum)
            }
            _ => false,
        }
    }
}
