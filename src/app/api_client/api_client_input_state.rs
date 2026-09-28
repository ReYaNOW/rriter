#[derive(Default)]
pub(crate) struct ApiFocusCommitResult {
    pub(crate) mock_config_changed: bool,
    pub(crate) credentials_changed: bool,
    pub(crate) invalidate_contract_tools: bool,
    pub(crate) text_for_app: Option<String>,
}

impl crate::app::api_client::ApiClientState {
    pub(crate) fn normalized_api_focus_text(&self, focus: &ApiFocus) -> String {
        let text = self.input_editor.get_full_text();
        if self.api_focus_is_array_input(focus) {
            split_api_array_values(&text).join("\n")
        } else {
            text
        }
    }

    pub(crate) fn commit_api_focus_state(
        &mut self,
        focus: &ApiFocus,
        text: String,
        active: Option<&ApiActiveRoute>,
    ) -> ApiFocusCommitResult {
        let mut result = ApiFocusCommitResult::default();
        match focus {
            ApiFocus::ImportUrl => {}
            ApiFocus::RouteFilter => self.route_filter = text,
            ApiFocus::MockProxyBase => {
                self.mock.proxy_base_url = text.trim().to_string();
                result.mock_config_changed = true;
            }
            ApiFocus::MockPythonUvPath => {
                self.mock.uv.configured_path = non_empty_path(&text);
                result.mock_config_changed = true;
            }
            ApiFocus::MockPythonVersion => {
                let version = text.trim();
                self.mock.uv.python_version = if version.is_empty() {
                    "3.13".to_string()
                } else {
                    version.to_string()
                };
                result.mock_config_changed = true;
            }
            ApiFocus::MockPythonCustomPath => {
                self.mock.uv.custom_python_path = non_empty_path(&text);
                result.mock_config_changed = true;
            }
            ApiFocus::MockManualPath { manual_idx } => {
                let mut path = text.trim().to_string();
                if !path.starts_with('/') {
                    path.insert(0, '/');
                }
                if let Some(route) = self.mock.manual_routes.get_mut(*manual_idx) {
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
                        result.invalidate_contract_tools = true;
                    }
                    result.mock_config_changed = true;
                }
            }
            ApiFocus::MockContract { route_idx } => {
                let Some((_, _, route, model)) = self.api_mock_route_context(active, *route_idx)
                else {
                    return result;
                };
                let default_contract =
                    crate::app::api_mock::types::default_contract_from_route(&route, &model);
                if let Some(script) = self.api_route_python_script_mut(active, *route_idx) {
                    let base = if script.contract.is_empty() {
                        default_contract
                    } else {
                        script.contract.clone()
                    };
                    let contract = crate::app::api_mock::contract::api_mock_contract_from_state_text(
                        &base, &text,
                    );
                    let generated =
                        crate::app::api_mock::contract::api_mock_contract_state_text(&contract);
                    let contract_source = if text.trim() == generated.trim() {
                        String::new()
                    } else {
                        text
                    };
                    if base != contract || script.contract_source != contract_source {
                        script.contract = contract;
                        script.contract_source = contract_source;
                        result.invalidate_contract_tools = true;
                        result.mock_config_changed = true;
                    }
                }
            }
            ApiFocus::MockPrelude { route_idx } => {
                if let Some(script) = self.api_route_python_script_mut(active, *route_idx) {
                    script.prelude = text;
                    result.mock_config_changed = true;
                }
            }
            ApiFocus::MockBody { route_idx } => {
                if let Some(script) = self.api_route_python_script_mut(active, *route_idx) {
                    script.body = text;
                    result.mock_config_changed = true;
                }
            }
            ApiFocus::MockSignature { .. } => {}
            ApiFocus::MockStaticResponse { route_idx } => {
                let generated = self
                    .api_mock_generated_preview(active, *route_idx)
                    .unwrap_or_else(|| "{}".to_string());
                if let Some(route) = self.active_manual_mock_route_mut(active, *route_idx) {
                    route.enabled = true;
                    route.response = if text.trim() == generated.trim() {
                        crate::app::api_mock::types::ApiMockResponse::Generated
                    } else {
                        crate::app::api_mock::types::ApiMockResponse::Json(text)
                    };
                    result.mock_config_changed = true;
                } else {
                    let Some(spec_id) = active.map(|active| active.spec_id) else {
                        return result;
                    };
                    self.ensure_api_route_override(spec_id, *route_idx);
                    if let Some(route) = self.api_route_override_mut(spec_id, *route_idx) {
                        let was_enabled = route.enabled;
                        route.response = if text.trim() == generated.trim() {
                            crate::app::api_mock::types::ApiMockResponse::Generated
                        } else {
                            crate::app::api_mock::types::ApiMockResponse::Json(text)
                        };
                        route.enabled = was_enabled;
                        result.mock_config_changed = true;
                    }
                }
            }
            ApiFocus::AuthValue { spec_id, scheme } => {
                let previous = self.auth.entry(*spec_id, scheme).cloned();
                self.auth.set_value(*spec_id, scheme, text);
                result.credentials_changed = self.auth.entry(*spec_id, scheme) != previous.as_ref();
            }
            ApiFocus::AuthRefreshToken { spec_id, scheme } => {
                let previous = self.auth.entry(*spec_id, scheme).cloned();
                let entry = self.auth.entry_mut(*spec_id, scheme);
                entry.refresh_token = text;
                result.credentials_changed = self.auth.entry(*spec_id, scheme) != previous.as_ref();
            }
            ApiFocus::AuthUsername { spec_id, scheme } => {
                let previous = self.auth.entry(*spec_id, scheme).cloned();
                let entry = self.auth.entry_mut(*spec_id, scheme);
                entry.username = text;
                result.credentials_changed = self.auth.entry(*spec_id, scheme) != previous.as_ref();
            }
            ApiFocus::AuthPassword { spec_id, scheme } => {
                let previous = self.auth.entry(*spec_id, scheme).cloned();
                let entry = self.auth.entry_mut(*spec_id, scheme);
                entry.password = text;
                result.credentials_changed = self.auth.entry(*spec_id, scheme) != previous.as_ref();
            }
            ApiFocus::PathParam { .. }
            | ApiFocus::QueryParam { .. }
            | ApiFocus::BodyField { .. }
            | ApiFocus::Body { .. }
            | ApiFocus::MockContractField { .. } => result.text_for_app = Some(text),
            ApiFocus::InputSchema { .. }
            | ApiFocus::OutputSchema { .. }
            | ApiFocus::Response { .. } => {}
        }
        if result.credentials_changed {
            self.persist_credentials();
        }
        result
    }

    fn api_focus_order_for_view(
        spec_id: crate::app::api_client::ApiSpecId,
        model: &crate::app::api_client::ApiSpecModel,
        state: &crate::app::api_client::ApiClientTabState,
    ) -> Vec<crate::app::api_client::ApiFocus> {
        api_focus_order_for_view(spec_id, model, state)
    }

    pub(crate) fn next_api_focus(
        &self,
        spec_id: crate::app::api_client::ApiSpecId,
        model: &crate::app::api_client::ApiSpecModel,
        state: &crate::app::api_client::ApiClientTabState,
        reverse: bool,
    ) -> Option<crate::app::api_client::ApiFocus> {
        let order = Self::api_focus_order_for_view(spec_id, model, state);
        if order.is_empty() {
            return None;
        }
        let current_idx = self
            .focused
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
        Some(order[next_idx].clone())
    }

    pub(crate) fn api_mock_input_schema_text_for_focus_route(
        &self,
        active: Option<&ApiActiveRoute>,
        spec_id: ApiSpecId,
        route_idx: usize,
    ) -> Option<String> {
        if spec_id == API_MANUAL_MOCK_SPEC_ID {
            let route = self.active_manual_mock_route(active, route_idx)?;
            let script = route.python.as_ref().filter(|script| script.enabled)?;
            let model = api_manual_route_model(route);
            let route = model.routes.first()?;
            let contract = crate::app::api_mock::types::api_mock_effective_contract(
                script, route, &model,
            );
            return Some(api_mock_input_schema_text(&contract));
        }
        let model = self.models.get(&spec_id)?;
        let route = model.routes.get(route_idx)?;
        let script = self
            .active_manual_mock_route(active, route_idx)
            .and_then(|route| route.python.as_ref())
            .or_else(|| {
                self.api_route_override(active?.spec_id, route_idx)
                    .and_then(|route| route.python.as_ref())
            })
            .filter(|script| script.enabled)?;
        let contract = crate::app::api_mock::types::api_mock_effective_contract(script, route, model);
        Some(api_mock_input_schema_text(&contract))
    }

    pub(crate) fn api_focus_text(
        &self,
        focus: &ApiFocus,
        active: Option<&ApiActiveRoute>,
        tab_state: Option<&ApiClientTabState>,
    ) -> String {
        match focus {
            ApiFocus::ImportUrl => self.input_editor.get_full_text(),
            ApiFocus::RouteFilter => self.route_filter.clone(),
            ApiFocus::MockProxyBase => self.mock.proxy_base_url.clone(),
            ApiFocus::MockPythonUvPath => self
                .mock
                .uv
                .selected_uv_path()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default(),
            ApiFocus::MockPythonVersion => self.mock.uv.python_version.clone(),
            ApiFocus::MockPythonCustomPath => self
                .mock
                .uv
                .custom_python_path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default(),
            ApiFocus::MockManualPath { manual_idx } => self
                .mock
                .manual_routes
                .get(*manual_idx)
                .map(|route| route.path.clone())
                .unwrap_or_default(),
            ApiFocus::MockContract { route_idx } => self
                .api_mock_contract_source_for_route(active, *route_idx)
                .unwrap_or_default(),
            ApiFocus::MockPrelude { route_idx } => self
                .api_route_python_script(active, *route_idx)
                .map(|script| script.prelude.clone())
                .unwrap_or_default(),
            ApiFocus::MockBody { route_idx } => self
                .api_route_python_script(active, *route_idx)
                .map(|script| api_mock_body_editor_text(&script.body))
                .unwrap_or_default(),
            ApiFocus::MockSignature { route_idx } => self
                .api_mock_signature_for_route(active, *route_idx)
                .unwrap_or_default(),
            ApiFocus::MockStaticResponse { route_idx } => self
                .active_manual_mock_route(active, *route_idx)
                .map(|route| &route.response)
                .or_else(|| {
                    self.api_route_override_for_active(active, *route_idx)
                        .map(|route| &route.response)
                })
                .map(|response| match response {
                    crate::app::api_mock::types::ApiMockResponse::Generated => self
                        .api_mock_generated_preview(active, *route_idx)
                        .unwrap_or_else(|| "{}".to_string()),
                    crate::app::api_mock::types::ApiMockResponse::Json(text)
                    | crate::app::api_mock::types::ApiMockResponse::Text(text) => text.clone(),
                })
                .unwrap_or_else(|| {
                    self.api_mock_generated_preview(active, *route_idx)
                        .unwrap_or_else(|| "{}".to_string())
                }),
            ApiFocus::MockContractField {
                route_idx,
                group,
                field_idx,
                prop,
            } => {
                self.api_mock_contract_field_prop_text(
                    active, *route_idx, *group, *field_idx, *prop,
                )
            }
            ApiFocus::AuthValue { spec_id, scheme } => self
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
                .auth
                .entry(*spec_id, scheme)
                .map(|entry| entry.refresh_token.clone())
                .unwrap_or_default(),
            ApiFocus::AuthUsername { spec_id, scheme } => self
                .auth
                .entry(*spec_id, scheme)
                .map(|entry| entry.username.clone())
                .unwrap_or_default(),
            ApiFocus::AuthPassword { spec_id, scheme } => self
                .auth
                .entry(*spec_id, scheme)
                .map(|entry| entry.password.clone())
                .unwrap_or_default(),
            ApiFocus::PathParam {
                spec_id,
                route_idx,
                name,
            } => tab_state
                .filter(|state| {
                    active.is_some_and(|active| {
                        active.spec_id == *spec_id && state.route_idx == Some(*route_idx)
                    })
                })
                .and_then(|state| state.path_values.iter().find(|value| value.name == *name))
                .map(|value| value.value.clone())
                .unwrap_or_default(),
            ApiFocus::QueryParam {
                spec_id,
                route_idx,
                name,
            } => tab_state
                .filter(|state| {
                    active.is_some_and(|active| {
                        active.spec_id == *spec_id && state.route_idx == Some(*route_idx)
                    })
                })
                .and_then(|state| state.query_values.iter().find(|value| value.name == *name))
                .map(|value| value.value.clone())
                .unwrap_or_default(),
            ApiFocus::BodyField {
                spec_id,
                route_idx,
                name,
            } => tab_state
                .filter(|state| {
                    active.is_some_and(|active| {
                        active.spec_id == *spec_id && state.route_idx == Some(*route_idx)
                    })
                })
                .and_then(|state| state.body_values.iter().find(|value| value.name == *name))
                .map(|value| value.value.clone())
                .unwrap_or_default(),
            ApiFocus::Body { spec_id, route_idx } => tab_state
                .filter(|state| {
                    active.is_some_and(|active| {
                        active.spec_id == *spec_id && state.route_idx == Some(*route_idx)
                    })
                })
                .map(|state| state.body_json.clone())
                .unwrap_or_default(),
            ApiFocus::InputSchema { spec_id, route_idx } => self
                .api_mock_input_schema_text_for_focus_route(active, *spec_id, *route_idx)
                .or_else(|| {
                    tab_state
                        .filter(|state| {
                            active.is_some_and(|active| {
                                active.spec_id == *spec_id
                                    && state.route_idx == Some(*route_idx)
                            })
                        })
                        .and_then(|state| {
                            self.models.get(spec_id).and_then(|model| {
                                model.routes.get(*route_idx).map(|route| {
                                    api_route_input_schema_text(
                                        route,
                                        model,
                                        state.input_schema_idx,
                                        &state.input_schema_collapsed,
                                    )
                                })
                            })
                        })
                })
                .unwrap_or_default(),
            ApiFocus::OutputSchema { spec_id, route_idx } => tab_state
                .filter(|state| {
                    active.is_some_and(|active| {
                        active.spec_id == *spec_id && state.route_idx == Some(*route_idx)
                    })
                })
                .and_then(|state| {
                    self.models
                        .get(spec_id)
                        .and_then(|model| model.routes.get(*route_idx).map(|route| {
                            match state.output_doc_view {
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
                            }
                        }))
                })
                .unwrap_or_default(),
            ApiFocus::Response { spec_id, route_idx } => tab_state
                .filter(|state| {
                    active.is_some_and(|active| {
                        active.spec_id == *spec_id && state.route_idx == Some(*route_idx)
                    })
                })
                .and_then(|state| {
                    state.response.as_ref().map(|response| {
                        api_response_text(response, state.response_view).to_string()
                    })
                })
                .unwrap_or_default(),
        }
    }

    fn api_route_override_for_active(
        &self,
        active: Option<&ApiActiveRoute>,
        route_idx: usize,
    ) -> Option<&crate::app::api_mock::types::ApiMockRouteOverride> {
        let spec_id = active?.spec_id;
        self.api_route_override(spec_id, route_idx)
    }

}

impl crate::app::api_client::ApiClientTabState {
    pub(crate) fn commit_api_tab_focus(&mut self, focus: ApiFocus, text: String) {
        match focus {
            ApiFocus::PathParam { route_idx, name, .. }
                if self.route_idx == Some(route_idx) =>
            {
                if let Some(value) = self.path_values.iter_mut().find(|value| value.name == name) {
                    value.value = text;
                }
            }
            ApiFocus::QueryParam { route_idx, name, .. }
                if self.route_idx == Some(route_idx) =>
            {
                if let Some(value) = self.query_values.iter_mut().find(|value| value.name == name) {
                    value.value = text;
                }
            }
            ApiFocus::BodyField { route_idx, name, .. }
                if self.route_idx == Some(route_idx) =>
            {
                if let Some(value) = self.body_values.iter_mut().find(|value| value.name == name) {
                    value.value = text;
                    self.body_file_paths.remove(&name);
                }
            }
            ApiFocus::Body { route_idx, .. } if self.route_idx == Some(route_idx) => {
                self.body_json = text;
            }
            _ => {}
        }
    }
}

impl crate::app::api_client::ApiClientState {
    pub(crate) fn api_mock_constraint_menu_contains_ui_id(
        &self,
        id: Option<crate::ui_system::UiId>,
    ) -> bool {
        api_mock_constraint_menu_contains_ui_id(self.mock_contract_constraint_menu, id)
    }

    pub(crate) fn close_api_mock_constraint_menu(&mut self) -> bool {
        self.mock_contract_constraint_menu.take().is_some()
    }

    pub(crate) fn start_api_python_runtime_scroll_drag(
        &mut self,
        id: crate::ui_system::UiId,
        rect: (f32, f32, f32, f32),
        pointer_y: f32,
        scale: f32,
    ) -> bool {
        let (scroll, max_scroll) = match id {
            crate::ui_system::UiId::ApiMockPythonVersionsScrollY
                if self.mock_python_version_picker_open =>
            {
                let max_scroll = crate::app::api_client::api_python_version_list_max_scroll(
                    self.mock_python_versions.len(),
                    rect.3 + 12.0 * scale,
                    scale,
                );
                (&mut self.mock_python_versions_scroll, max_scroll)
            }
            crate::ui_system::UiId::ApiMockPythonInstallLogScrollY
                if crate::app::api_client::api_python_install_log_visible(self) =>
            {
                let max_scroll = crate::app::api_client::api_python_install_log_max_scroll(
                    self.mock_python_install_log.len(),
                    rect.3 + 12.0 * scale,
                    scale,
                );
                (&mut self.mock_python_install_log_scroll, max_scroll)
            }
            _ => return false,
        };
        let Some((drag_offset, target)) = crate::app::api_client::api_python_scrollbar_drag_target(
            (rect.0, rect.1 - 6.0 * scale, rect.2, rect.3 + 12.0 * scale),
            scroll.current,
            max_scroll,
            pointer_y,
            scale,
            None,
        ) else {
            return false;
        };
        crate::app::mouse::apply_scrollbar_drag_target(scroll, target, drag_offset)
    }

    pub(crate) fn update_api_python_runtime_scroll_drag(
        &mut self,
        id: crate::ui_system::UiId,
        rect: Option<(f32, f32, f32, f32)>,
        pointer_y: f32,
        scale: f32,
    ) -> bool {
        let Some(rect) = rect else {
            match id {
                crate::ui_system::UiId::ApiMockPythonVersionsScrollY => {
                    self.mock_python_versions_scroll.end_drag();
                }
                crate::ui_system::UiId::ApiMockPythonInstallLogScrollY => {
                    self.mock_python_install_log_scroll.end_drag();
                }
                _ => {}
            }
            return false;
        };
        let source_rect = (rect.0, rect.1 - 6.0 * scale, rect.2, rect.3 + 12.0 * scale);
        let (scroll, max_scroll) = match id {
            crate::ui_system::UiId::ApiMockPythonVersionsScrollY => {
                let max_scroll = crate::app::api_client::api_python_version_list_max_scroll(
                    self.mock_python_versions.len(),
                    source_rect.3,
                    scale,
                );
                (&mut self.mock_python_versions_scroll, max_scroll)
            }
            crate::ui_system::UiId::ApiMockPythonInstallLogScrollY => {
                let max_scroll = crate::app::api_client::api_python_install_log_max_scroll(
                    self.mock_python_install_log.len(),
                    source_rect.3,
                    scale,
                );
                (&mut self.mock_python_install_log_scroll, max_scroll)
            }
            _ => return false,
        };
        let Some((drag_offset, target)) = crate::app::api_client::api_python_scrollbar_drag_target(
            source_rect,
            scroll.current,
            max_scroll,
            pointer_y,
            scale,
            Some(scroll.drag_offset),
        ) else {
            scroll.end_drag();
            return false;
        };
        crate::app::mouse::apply_scrollbar_drag_target(scroll, target, drag_offset)
    }

    fn api_mock_generated_preview(
        &self,
        active: Option<&ApiActiveRoute>,
        route_idx: usize,
    ) -> Option<String> {
        let (_, _, route, model) = self.api_mock_route_context(active, route_idx)?;
        Some(api_generated_response_for_route(&route, &model).2)
    }

    fn api_focus_is_array_input(&self, focus: &ApiFocus) -> bool {
        match focus {
            ApiFocus::PathParam {
                spec_id,
                route_idx,
                name,
            } => self
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

    fn apply_response_token_to_auth(
        &mut self,
        spec_id: ApiSpecId,
        response_body: &str,
        scheme_idx: usize,
        save_access: bool,
        save_refresh: bool,
    ) {
        let Ok(json) = serde_json::from_str::<Value>(response_body) else {
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
            .models
            .get(&spec_id)
            .and_then(|model| model.security_schemes.get(scheme_idx))
            .filter(|scheme| scheme.token_capable())
            .map(|scheme| scheme.name.clone())
        else {
            return;
        };
        let entry = self.auth.entry_mut(spec_id, &scheme_name);
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
        self.persist_credentials();
    }
}
