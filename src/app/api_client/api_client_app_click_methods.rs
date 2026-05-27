impl crate::app::App {
    pub fn handle_api_client_click(&mut self, id: crate::ui_system::UiId) -> bool {
        if matches!(
            id,
            crate::ui_system::UiId::ApiImportUrlInput
                | crate::ui_system::UiId::ApiMockProxyBaseInput
                | crate::ui_system::UiId::ApiMockPythonUvPathInput
                | crate::ui_system::UiId::ApiMockPythonCustomPathInput
                | crate::ui_system::UiId::ApiMockManualRoutePath(_)
                | crate::ui_system::UiId::ApiAuthValue(_)
                | crate::ui_system::UiId::ApiAuthRefreshToken(_)
                | crate::ui_system::UiId::ApiAuthUsername(_)
                | crate::ui_system::UiId::ApiAuthPassword(_)
                | crate::ui_system::UiId::ApiPathParamInput(_, _)
                | crate::ui_system::UiId::ApiQueryParamInput(_, _)
                | crate::ui_system::UiId::ApiBodyInput(_)
                | crate::ui_system::UiId::ApiBodyFieldInput(_, _)
                | crate::ui_system::UiId::ApiResponseBody(_)
                | crate::ui_system::UiId::ApiMockStaticResponseInput(_)
                | crate::ui_system::UiId::ApiMockContractInput(_)
                | crate::ui_system::UiId::ApiMockSignatureInput(_)
                | crate::ui_system::UiId::ApiMockPreludeInput(_)
                | crate::ui_system::UiId::ApiMockBodyInput(_)
        ) {
            self.is_dragging = true;
            self.ide_panel.is_dragging_terminal = false;
        }
        match id {
            crate::ui_system::UiId::ApiImportAdd => {
                self.ide_panel.api.import_menu_open = !self.ide_panel.api.import_menu_open;
            }
            crate::ui_system::UiId::ApiImportFile => {
                self.ide_panel.api.import_menu_open = false;
                self.trigger_api_file_picker();
            }
            crate::ui_system::UiId::ApiImportUrl => {
                self.ide_panel.api.import_menu_open = false;
                self.ide_panel.api.import_url_open = true;
                self.focus_api_input(ApiFocus::ImportUrl);
            }
            crate::ui_system::UiId::ApiImportUrlInput => {
                self.ide_panel.api.import_url_open = true;
                self.focus_api_input(ApiFocus::ImportUrl);
                self.place_api_cursor_from_last_click(id, false);
            }
            crate::ui_system::UiId::ApiImportUrlConfirm => {
                self.commit_api_focus();
                self.start_api_url_import_from_input();
            }
            crate::ui_system::UiId::ApiMockServerToggle => {
                self.toggle_api_mock_server();
            }
            crate::ui_system::UiId::ApiMockServerDetails => {
                self.commit_api_focus();
                self.ide_panel.api.mock_server_detail_open = true;
                self.ide_panel.api.mock_guide_open = false;
                self.ide_panel.api.mock_python_runtime_open = false;
            }
            crate::ui_system::UiId::ApiMockServerDetailsClose => {
                self.ide_panel.api.mock_server_detail_open = false;
            }
            crate::ui_system::UiId::ApiMockServerLogArea => {
                self.commit_api_focus();
                self.ide_panel.api.focused = None;
            }
            crate::ui_system::UiId::ApiMockServerLogScrollY => {
                self.commit_api_focus();
                self.ide_panel.api.focused = None;
                let mx = self
                    .renderer
                    .as_ref()
                    .map(|renderer| renderer.last_mouse_y)
                    .unwrap_or(0.0);
                if let Some(rect) = self
                    .ui_registry
                    .rect_for(crate::ui_system::UiId::ApiMockServerLogArea)
                {
                    let max_scroll = api_mock_server_log_max_scroll(
                        self.ide_panel.api.mock_server_logs.len(),
                        rect.3,
                        self.renderer
                            .as_ref()
                            .map(|renderer| renderer.scale_factor)
                            .unwrap_or(1.0),
                    );
                    let ratio = ((mx - rect.1) / rect.3.max(1.0)).clamp(0.0, 1.0);
                    self.ide_panel.api.mock_server_log_scroll.target = ratio * max_scroll;
                    self.ide_panel.api.mock_server_log_scroll.current =
                        self.ide_panel.api.mock_server_log_scroll.target;
                }
            }
            crate::ui_system::UiId::ApiMockModeSelect => {
                self.commit_api_focus();
                self.ide_panel.api.mock.mode = match self.ide_panel.api.mock.mode {
                    crate::app::api_mock::types::ApiMockMode::MockAll => {
                        crate::app::api_mock::types::ApiMockMode::MockSelectedOnly
                    }
                    crate::app::api_mock::types::ApiMockMode::MockSelectedOnly => {
                        crate::app::api_mock::types::ApiMockMode::MockSelectedProxyRest
                    }
                    crate::app::api_mock::types::ApiMockMode::MockSelectedProxyRest => {
                        crate::app::api_mock::types::ApiMockMode::MockAll
                    }
                };
                self.ide_panel.api.persist();
            }
            crate::ui_system::UiId::ApiMockProxyBaseInput => {
                self.focus_api_input(ApiFocus::MockProxyBase);
                self.place_api_cursor_from_last_click(id, false);
            }
            crate::ui_system::UiId::ApiMockGuideOpen => {
                self.commit_api_focus();
                self.ide_panel.api.mock_guide_open = true;
                self.ide_panel.api.mock_server_detail_open = false;
                self.ide_panel.api.mock_python_runtime_open = false;
            }
            crate::ui_system::UiId::ApiMockGuideClose => {
                self.ide_panel.api.mock_guide_open = false;
            }
            crate::ui_system::UiId::ApiMockGuideBody
            | crate::ui_system::UiId::ApiMockGuideScrollY => {
                self.commit_api_focus();
                self.ide_panel.api.focused = None;
            }
            crate::ui_system::UiId::ApiMockPythonManage => {
                self.commit_api_focus();
                clear_legacy_api_python_runtime_message(&mut self.ide_panel.api);
                self.ide_panel.api.mock_python_runtime_open = true;
                self.ide_panel.api.mock_guide_open = false;
                self.ide_panel.api.mock_server_detail_open = false;
                if matches!(
                    self.ide_panel.api.mock.uv.mode,
                    crate::app::api_mock::types::ApiPythonRuntimeMode::UvManaged
                ) && self.ide_panel.api.mock.uv.selected_uv_path().is_none()
                {
                    crate::app::api_mock::python_bootstrap::refresh_uv_status(
                        &mut self.ide_panel.api.mock.uv,
                    );
                }
            }
            crate::ui_system::UiId::ApiMockPythonManageClose => {
                self.commit_api_focus();
                self.ide_panel.api.mock_python_runtime_open = false;
                self.ide_panel.api.mock_python_version_picker_open = false;
            }
            crate::ui_system::UiId::ApiMockPythonModeToggle => {
                self.commit_api_focus();
                clear_legacy_api_python_runtime_message(&mut self.ide_panel.api);
                self.ide_panel.api.mock.uv.mode = match self.ide_panel.api.mock.uv.mode {
                    crate::app::api_mock::types::ApiPythonRuntimeMode::UvManaged => {
                        crate::app::api_mock::types::ApiPythonRuntimeMode::CustomPython
                    }
                    crate::app::api_mock::types::ApiPythonRuntimeMode::CustomPython => {
                        crate::app::api_mock::types::ApiPythonRuntimeMode::UvManaged
                    }
                };
                self.ide_panel.api.mock_python_version_picker_open = false;
                self.ide_panel.api.persist();
            }
            crate::ui_system::UiId::ApiMockPythonCheckRuntime => {
                self.commit_api_focus();
                crate::app::api_mock::python_bootstrap::refresh_python_runtime_status(
                    &mut self.ide_panel.api.mock.uv,
                );
                self.ide_panel.api.persist();
            }
            crate::ui_system::UiId::ApiMockPythonPrepareVersion => {
                self.commit_api_focus();
                self.trigger_api_python_install();
            }
            crate::ui_system::UiId::ApiMockPythonPickUvPath => {
                self.commit_api_focus();
                self.trigger_api_python_path_picker(ApiPythonPathPickKind::Uv);
            }
            crate::ui_system::UiId::ApiMockPythonUvPathInput => {
                self.focus_api_input(ApiFocus::MockPythonUvPath);
                self.place_api_cursor_from_last_click(id, false);
            }
            crate::ui_system::UiId::ApiMockPythonVersionInput => {
                self.commit_api_focus();
                if self.ide_panel.api.mock_python_version_picker_open {
                    self.ide_panel.api.mock_python_version_picker_open = false;
                } else {
                    self.trigger_api_python_version_list();
                }
            }
            crate::ui_system::UiId::ApiMockPythonVersionOption(idx) => {
                self.commit_api_focus();
                if let Some(row) = self.ide_panel.api.mock_python_versions.get(idx) {
                    self.ide_panel.api.mock.uv.python_version = row.version.clone();
                    self.ide_panel.api.mock_python_version_picker_open = false;
                    self.ide_panel.api.mock_python_versions_scroll.current = 0.0;
                    self.ide_panel.api.mock_python_versions_scroll.target = 0.0;
                    self.ide_panel.api.persist();
                }
            }
            crate::ui_system::UiId::ApiMockPythonPickCustomPath => {
                self.commit_api_focus();
                self.trigger_api_python_path_picker(ApiPythonPathPickKind::CustomPython);
            }
            crate::ui_system::UiId::ApiMockPythonCustomPathInput => {
                self.focus_api_input(ApiFocus::MockPythonCustomPath);
                self.place_api_cursor_from_last_click(id, false);
            }
            crate::ui_system::UiId::ApiMockManualRoutePath(manual_idx) => {
                self.focus_api_input(ApiFocus::MockManualPath { manual_idx });
                self.place_api_cursor_from_last_click(id, false);
            }
            crate::ui_system::UiId::ApiMockRouteEnable(route_idx) => {
                self.toggle_api_route_mock(route_idx);
            }
            crate::ui_system::UiId::ApiMockRouteDetailsToggle(route_idx) => {
                self.commit_api_focus();
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let key = (meta.spec_id, route_idx);
                if self.ide_panel.api.expanded_mock_routes.contains(&key) {
                    self.ide_panel.api.expanded_mock_routes.remove(&key);
                } else {
                    self.ide_panel.api.expanded_mock_routes.insert(key);
                    self.start_api_mock_route_tools_now(route_idx);
                }
            }
            crate::ui_system::UiId::ApiMockRoutePythonToggle(route_idx) => {
                if self.toggle_api_route_python(route_idx) {
                    self.start_api_mock_route_tools_now(route_idx);
                }
            }
            crate::ui_system::UiId::ApiMockExportOpenApi => {
                self.trigger_api_mock_export_openapi();
            }
            crate::ui_system::UiId::ApiMockContractQueryToggle(route_idx) => {
                self.toggle_api_mock_contract_query(route_idx);
            }
            crate::ui_system::UiId::ApiMockContractBodyToggle(route_idx) => {
                self.toggle_api_mock_contract_body(route_idx);
            }
            crate::ui_system::UiId::ApiMockContractQueryFieldToggle(route_idx, field_idx) => {
                self.toggle_api_mock_contract_query_field(route_idx, field_idx);
            }
            crate::ui_system::UiId::ApiMockContractBodyFieldToggle(route_idx, field_idx) => {
                self.toggle_api_mock_contract_body_field(route_idx, field_idx);
            }
            crate::ui_system::UiId::ApiMockStaticResponseInput(route_idx) => {
                self.focus_api_input(ApiFocus::MockStaticResponse { route_idx });
                self.place_api_cursor_from_last_click(id, true);
            }
            crate::ui_system::UiId::ApiMockCombinedPython(_) => {}
            crate::ui_system::UiId::ApiMockContractInput(route_idx) => {
                self.focus_api_input(ApiFocus::MockContract { route_idx });
                self.place_api_cursor_from_last_click(id, true);
            }
            crate::ui_system::UiId::ApiMockSignatureInput(route_idx) => {
                self.focus_api_input(ApiFocus::MockSignature { route_idx });
                self.place_api_cursor_from_last_click(id, true);
            }
            crate::ui_system::UiId::ApiMockAddManualRoute => {
                self.add_api_manual_route();
            }
            crate::ui_system::UiId::ApiMockManualRouteOpen(manual_idx) => {
                self.open_api_manual_route(manual_idx);
            }
            crate::ui_system::UiId::ApiMockPreludeInput(route_idx) => {
                self.focus_api_input(ApiFocus::MockPrelude { route_idx });
                self.place_api_cursor_from_last_click(id, true);
            }
            crate::ui_system::UiId::ApiMockBodyInput(route_idx) => {
                self.focus_api_input(ApiFocus::MockBody { route_idx });
                self.place_api_cursor_from_last_click(id, true);
            }
            crate::ui_system::UiId::ApiMockPreludeReset(route_idx) => {
                self.reset_api_route_python_part(route_idx, ApiMockSourcePart::Prelude);
            }
            crate::ui_system::UiId::ApiMockContractReset(route_idx) => {
                self.reset_api_route_python_part(route_idx, ApiMockSourcePart::Contract);
            }
            crate::ui_system::UiId::ApiMockBodyReset(route_idx) => {
                self.reset_api_route_python_part(route_idx, ApiMockSourcePart::Body);
            }
            crate::ui_system::UiId::ApiMockManualRouteMethod(manual_idx) => {
                self.commit_api_focus();
                if let Some(route) = self.ide_panel.api.mock.manual_routes.get_mut(manual_idx) {
                    route.method = match route.method {
                        ApiMethod::Get => ApiMethod::Post,
                        ApiMethod::Post => ApiMethod::Put,
                        ApiMethod::Put => ApiMethod::Patch,
                        ApiMethod::Patch => ApiMethod::Delete,
                        ApiMethod::Delete => ApiMethod::Get,
                        ApiMethod::Head | ApiMethod::Options | ApiMethod::Trace => ApiMethod::Get,
                    };
                    self.sync_api_manual_route_tabs();
                    self.ide_panel.api.persist();
                }
            }
            crate::ui_system::UiId::ApiMockAddInputField(_)
            | crate::ui_system::UiId::ApiMockAddOutputField(_) => {}
            crate::ui_system::UiId::ApiMockManualRouteRemove(idx) => {
                if idx < self.ide_panel.api.mock.manual_routes.len() {
                    self.ide_panel.api.mock.manual_routes.remove(idx);
                    self.sync_api_manual_route_tabs();
                    self.ide_panel.api.persist();
                }
            }
            crate::ui_system::UiId::ApiSpecOpen(idx) => {
                if let Some(id) = self.ide_panel.api.specs.get(idx).map(|entry| entry.id) {
                    self.open_api_spec_tab(id);
                }
            }
            crate::ui_system::UiId::ApiSpecRefresh(idx) => {
                if let Some(id) = self.ide_panel.api.specs.get(idx).map(|entry| entry.id) {
                    self.refresh_api_spec(id);
                }
            }
            crate::ui_system::UiId::ApiSpecRemove(idx) => {
                if let Some(id) = self.ide_panel.api.remove_spec(idx) {
                    let mut tab_idxs = self
                        .tabs
                        .iter()
                        .enumerate()
                        .filter_map(|(idx, tab)| match &tab.kind {
                            crate::app::EditorTabKind::ApiClient(meta, _) if meta.spec_id == id => {
                                Some(idx)
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    while let Some(tab_idx) = tab_idxs.pop() {
                        self.close_tab_at(tab_idx);
                    }
                }
            }
            crate::ui_system::UiId::ApiSpecSelect(idx) => {
                if let Some(id) = self.ide_panel.api.specs.get(idx).map(|entry| entry.id) {
                    let already_selected = self.ide_panel.api.selected_spec == Some(id);
                    self.ide_panel.api.select_spec(id);
                    self.ensure_api_model_loaded(id);
                    if already_selected {
                        if self.ide_panel.api.collapsed_route_roots.contains(&id) {
                            self.ide_panel.api.collapsed_route_roots.remove(&id);
                        } else {
                            self.ide_panel.api.collapsed_route_roots.insert(id);
                        }
                    }
                }
            }
            crate::ui_system::UiId::ApiAuthRoot => {
                if let Some(spec_id) = self.ide_panel.api.selected_spec {
                    self.open_api_auth_tab(spec_id);
                }
            }
            crate::ui_system::UiId::ApiRouteTag(group_idx) => {
                if let Some(spec_id) = self.ide_panel.api.selected_spec {
                    if let Some(model) = self.ide_panel.api.models.get(&spec_id) {
                        let groups = grouped_route_ranges(
                            &model.routes,
                            &self.ide_panel.api.collapsed_tags,
                            spec_id,
                        );
                        if let Some((tag, _, _, _)) = groups.get(group_idx) {
                            let key = (spec_id, tag.clone());
                            if self.ide_panel.api.collapsed_tags.contains(&key) {
                                self.ide_panel.api.collapsed_tags.remove(&key);
                            } else {
                                self.ide_panel.api.collapsed_tags.insert(key);
                            }
                        }
                    }
                }
            }
            crate::ui_system::UiId::ApiRoutesRoot => {
                if let Some(spec_id) = self.ide_panel.api.selected_spec {
                    if self.ide_panel.api.collapsed_route_roots.contains(&spec_id) {
                        self.ide_panel.api.collapsed_route_roots.remove(&spec_id);
                    } else {
                        self.ide_panel.api.collapsed_route_roots.insert(spec_id);
                    }
                }
            }
            crate::ui_system::UiId::ApiRouteRow(route_idx) => {
                if let Some(spec_id) = self.ide_panel.api.selected_spec {
                    self.open_api_route(spec_id, route_idx);
                }
            }
            crate::ui_system::UiId::ApiServerSelect(idx) => {
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
                    state.server_idx = idx;
                }
            }
            crate::ui_system::UiId::ApiAuthValue(scheme_idx)
            | crate::ui_system::UiId::ApiAuthRefreshToken(scheme_idx)
            | crate::ui_system::UiId::ApiAuthUsername(scheme_idx)
            | crate::ui_system::UiId::ApiAuthPassword(scheme_idx) => {
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                let scheme = self
                    .ide_panel
                    .api
                    .models
                    .get(&spec_id)
                    .and_then(|model| model.security_schemes.get(scheme_idx))
                    .map(|scheme| scheme.name.clone())
                    .unwrap_or_default();
                if scheme.is_empty() {
                    return true;
                }
                let focus = match id {
                    crate::ui_system::UiId::ApiAuthUsername(_) => {
                        ApiFocus::AuthUsername { spec_id, scheme }
                    }
                    crate::ui_system::UiId::ApiAuthPassword(_) => {
                        ApiFocus::AuthPassword { spec_id, scheme }
                    }
                    crate::ui_system::UiId::ApiAuthRefreshToken(_) => {
                        ApiFocus::AuthRefreshToken { spec_id, scheme }
                    }
                    _ => ApiFocus::AuthValue { spec_id, scheme },
                };
                self.focus_api_input(focus);
                self.place_api_cursor_from_last_click(id, false);
            }
            crate::ui_system::UiId::ApiAuthSave(_) => {
                self.commit_api_focus();
                self.ide_panel.api.focused = None;
                self.ide_panel.api.persist();
            }
            crate::ui_system::UiId::ApiAuthAccessSave(_)
            | crate::ui_system::UiId::ApiAuthRefreshSave(_) => {
                self.commit_api_focus();
                self.ide_panel.api.focused = None;
                self.ide_panel.api.persist();
            }
            crate::ui_system::UiId::ApiAuthAccessClear(scheme_idx) => {
                self.commit_api_focus();
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                if let Some(scheme) = self
                    .ide_panel
                    .api
                    .models
                    .get(&spec_id)
                    .and_then(|model| model.security_schemes.get(scheme_idx))
                    .map(|scheme| scheme.name.clone())
                {
                    let entry = self.ide_panel.api.auth.entry_mut(spec_id, &scheme);
                    entry.access_token.clear();
                    entry.value.clear();
                    self.ide_panel.api.focused = None;
                    self.ide_panel.api.persist();
                }
            }
            crate::ui_system::UiId::ApiAuthRefreshClear(scheme_idx) => {
                self.commit_api_focus();
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                if let Some(scheme) = self
                    .ide_panel
                    .api
                    .models
                    .get(&spec_id)
                    .and_then(|model| model.security_schemes.get(scheme_idx))
                    .map(|scheme| scheme.name.clone())
                {
                    self.ide_panel
                        .api
                        .auth
                        .entry_mut(spec_id, &scheme)
                        .refresh_token
                        .clear();
                    self.ide_panel.api.focused = None;
                    self.ide_panel.api.persist();
                }
            }
            crate::ui_system::UiId::ApiAuthClear(scheme_idx) => {
                self.commit_api_focus();
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                if let Some(scheme) = self
                    .ide_panel
                    .api
                    .models
                    .get(&spec_id)
                    .and_then(|model| model.security_schemes.get(scheme_idx))
                    .map(|scheme| scheme.name.clone())
                {
                    self.ide_panel.api.auth.remove(spec_id, &scheme);
                    self.ide_panel.api.focused = None;
                    self.ide_panel.api.persist();
                }
            }
            crate::ui_system::UiId::ApiTryRequest => {
                self.start_active_api_request();
            }
            crate::ui_system::UiId::ApiPathParamAllowedValue(route_idx, param_idx, value_idx)
            | crate::ui_system::UiId::ApiQueryParamAllowedValue(route_idx, param_idx, value_idx) => {
                self.commit_api_focus();
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                let picked = self
                    .ide_panel
                    .api
                    .models
                    .get(&spec_id)
                    .and_then(|model| model.routes.get(route_idx))
                    .and_then(|route| match id {
                        crate::ui_system::UiId::ApiPathParamAllowedValue(_, _, _) => {
                            route.path_params.get(param_idx).map(|param| (true, param))
                        }
                        _ => route
                            .query_params
                            .get(param_idx)
                            .map(|param| (false, param)),
                    })
                    .and_then(|(path, param)| {
                        let values = if param.enum_values.is_empty() {
                            &param.examples
                        } else {
                            &param.enum_values
                        };
                        Some((
                            path,
                            param.name.clone(),
                            matches!(param.primitive_type, ApiPrimitiveType::Array),
                            values.get(value_idx)?.clone(),
                        ))
                    });
                if let Some((path, name, is_array, value)) = picked
                    && let Some((_, state)) = self.active_api_tab_mut_for(spec_id)
                    && state.route_idx == Some(route_idx)
                {
                    let values = if path {
                        &mut state.path_values
                    } else {
                        &mut state.query_values
                    };
                    if let Some(field) = values.iter_mut().find(|field| field.name == name) {
                        if is_array {
                            push_api_array_value(&mut field.value, &value);
                        } else {
                            field.value = value;
                        }
                    }
                    self.ide_panel.api.focused = None;
                }
            }
            crate::ui_system::UiId::ApiResponseBodyTab(route_idx)
            | crate::ui_system::UiId::ApiResponseHeadersTab(route_idx) => {
                let Some((meta, state)) = self.active_api_tab() else {
                    return true;
                };
                if state.route_idx != Some(route_idx) {
                    return true;
                }
                let spec_id = meta.spec_id;
                let view = match id {
                    crate::ui_system::UiId::ApiResponseBodyTab(_) => ApiResponseView::Body,
                    crate::ui_system::UiId::ApiResponseHeadersTab(_) => ApiResponseView::Headers,
                    _ => ApiResponseView::Body,
                };
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
                    state.response_view = view;
                    state.response_scroll.current = 0.0;
                    state.response_scroll.target = 0.0;
                    state.response_scroll_x.current = 0.0;
                    state.response_scroll_x.target = 0.0;
                }
                if matches!(
                    self.ide_panel.api.focused,
                    Some(ApiFocus::Response {
                        spec_id: focused_spec,
                        route_idx: focused_route,
                    }) if focused_spec == spec_id && focused_route == route_idx
                ) {
                    self.focus_api_input(ApiFocus::Response { spec_id, route_idx });
                }
            }
            crate::ui_system::UiId::ApiResponseUseAccessToken(route_idx, scheme_idx) => {
                self.apply_response_token_to_auth(route_idx, scheme_idx, true, false);
            }
            crate::ui_system::UiId::ApiResponseSaveRefreshToken(route_idx, scheme_idx) => {
                self.apply_response_token_to_auth(route_idx, scheme_idx, false, true);
            }
            crate::ui_system::UiId::ApiPathParamInput(route_idx, param_idx) => {
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                let name = self
                    .ide_panel
                    .api
                    .models
                    .get(&spec_id)
                    .and_then(|model| model.routes.get(route_idx))
                    .and_then(|route| route.path_params.get(param_idx))
                    .map(|param| param.name.clone())
                    .unwrap_or_default();
                self.focus_api_input(ApiFocus::PathParam {
                    spec_id,
                    route_idx,
                    name,
                });
                self.place_api_cursor_from_last_click(id, false);
            }
            crate::ui_system::UiId::ApiQueryParamInput(route_idx, param_idx) => {
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                let name = self
                    .ide_panel
                    .api
                    .models
                    .get(&spec_id)
                    .and_then(|model| model.routes.get(route_idx))
                    .and_then(|route| route.query_params.get(param_idx))
                    .map(|param| param.name.clone())
                    .unwrap_or_default();
                self.focus_api_input(ApiFocus::QueryParam {
                    spec_id,
                    route_idx,
                    name,
                });
                self.place_api_cursor_from_last_click(id, false);
            }
            crate::ui_system::UiId::ApiBodyInput(route_idx) => {
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                self.is_dragging = true;
                self.ide_panel.is_dragging_terminal = false;
                self.focus_api_input(ApiFocus::Body { spec_id, route_idx });
                self.place_api_cursor_from_last_click(id, true);
            }
            crate::ui_system::UiId::ApiBodyScrollX(route_idx) => {
                let Some((meta, state)) = self.active_api_tab() else {
                    return true;
                };
                if state.route_idx != Some(route_idx) {
                    return true;
                }
                let spec_id = meta.spec_id;
                let max_scroll = self.api_text_max_scroll_x_for_ui(id);
                let mx = self
                    .renderer
                    .as_ref()
                    .map(|renderer| renderer.last_mouse_x)
                    .unwrap_or(0.0);
                if let Some(rect) = self.ui_registry.rect_for(id)
                    && let Some((_, state)) = self.active_api_tab_mut_for(spec_id)
                {
                    state.body_scroll_x.is_dragging = true;
                    state.body_scroll_x.drag_offset = 0.0;
                    let ratio = (mx - rect.0) / rect.2.max(0.0001);
                    state.body_scroll_x.target = (ratio * max_scroll).clamp(0.0, max_scroll);
                    state.body_scroll_x.current = state.body_scroll_x.target;
                }
            }
            crate::ui_system::UiId::ApiBodyFieldInput(route_idx, prop_idx) => {
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                let name = self
                    .ide_panel
                    .api
                    .models
                    .get(&spec_id)
                    .and_then(|model| model.routes.get(route_idx))
                    .and_then(|route| route.request_body.as_ref())
                    .and_then(|body| body.schema)
                    .and_then(|schema_ref| {
                        self.ide_panel
                            .api
                            .models
                            .get(&spec_id)
                            .and_then(|model| model.schema_arena.get(schema_ref.0))
                    })
                    .and_then(|schema| schema.properties.get(prop_idx))
                    .map(|prop| prop.name.clone())
                    .unwrap_or_default();
                self.focus_api_input(ApiFocus::BodyField {
                    spec_id,
                    route_idx,
                    name,
                });
                self.place_api_cursor_from_last_click(id, false);
            }
            crate::ui_system::UiId::ApiBodyAllowedValue(route_idx, prop_idx, value_idx) => {
                self.commit_api_focus();
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                let picked = self
                    .ide_panel
                    .api
                    .models
                    .get(&spec_id)
                    .and_then(|model| model.routes.get(route_idx).map(|route| (model, route)))
                    .and_then(|(model, route)| {
                        let root = route.request_body.as_ref()?.schema?;
                        let prop = model.schema_arena.get(root.0)?.properties.get(prop_idx)?;
                        let schema = model.schema_arena.get(prop.schema.0)?;
                        let allowed = api_schema_allowed_values(schema, model);
                        let values = if allowed.is_empty() {
                            schema.examples.as_slice()
                        } else {
                            allowed
                        };
                        Some((
                            prop.name.clone(),
                            api_schema_is_array_input(schema),
                            values.get(value_idx)?.clone(),
                        ))
                    });
                let mut applied = None;
                if let Some((name, is_array, value)) = picked
                    && let Some((_, state)) = self.active_api_tab_mut_for(spec_id)
                    && let Some(field) = state
                        .body_values
                        .iter_mut()
                        .find(|field| field.name == name)
                {
                    if is_array {
                        push_api_array_value(&mut field.value, &value);
                    } else {
                        field.value = value.clone();
                    }
                    applied = Some((field.name.clone(), field.value.clone(), is_array));
                }
                if let Some((field_name, value, _)) = &applied
                    && matches!(
                        self.ide_panel.api.focused,
                        Some(ApiFocus::BodyField {
                            spec_id: f_spec,
                            route_idx: f_route,
                            ref name,
                        }) if f_spec == spec_id && f_route == route_idx && name == field_name
                    )
                {
                    let old_version = self.ide_panel.api.input_editor.version;
                    self.ide_panel.api.input_editor.set_text_clean(value);
                    self.ide_panel.api.input_editor.version = old_version.saturating_add(1);
                }
                if applied.is_some_and(|(_, _, is_array)| is_array) {
                    self.ide_panel.api.focused = None;
                }
            }
            crate::ui_system::UiId::ApiBodyFilePick(route_idx, prop_idx) => {
                self.commit_api_focus();
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                let picked = self
                    .ide_panel
                    .api
                    .models
                    .get(&spec_id)
                    .and_then(|model| model.routes.get(route_idx).map(|route| (model, route)))
                    .and_then(|(model, route)| {
                        let root = route.request_body.as_ref()?.schema?;
                        let prop = model.schema_arena.get(root.0)?.properties.get(prop_idx)?;
                        let schema = model.schema_arena.get(prop.schema.0)?;
                        Some((
                            prop.name.clone(),
                            api_schema_is_multi_file_input(schema, model),
                        ))
                    });
                if let Some((name, multi)) = picked {
                    self.trigger_api_body_file_picker(spec_id, route_idx, name, multi);
                }
            }
            crate::ui_system::UiId::ApiResponseBody(route_idx) => {
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                self.is_dragging = true;
                self.ide_panel.is_dragging_terminal = false;
                self.focus_api_input(ApiFocus::Response { spec_id, route_idx });
                self.place_api_cursor_from_last_click(id, true);
            }
            crate::ui_system::UiId::ApiResponseScrollX(route_idx) => {
                let Some((meta, state)) = self.active_api_tab() else {
                    return true;
                };
                if state.route_idx != Some(route_idx) {
                    return true;
                }
                let spec_id = meta.spec_id;
                let max_scroll = self.api_text_max_scroll_x_for_ui(id);
                let mx = self
                    .renderer
                    .as_ref()
                    .map(|renderer| renderer.last_mouse_x)
                    .unwrap_or(0.0);
                if let Some(rect) = self.ui_registry.rect_for(id)
                    && let Some((_, state)) = self.active_api_tab_mut_for(spec_id)
                {
                    state.response_scroll_x.is_dragging = true;
                    state.response_scroll_x.drag_offset = 0.0;
                    let ratio = (mx - rect.0) / rect.2.max(0.0001);
                    state.response_scroll_x.target = (ratio * max_scroll).clamp(0.0, max_scroll);
                    state.response_scroll_x.current = state.response_scroll_x.target;
                }
            }
            crate::ui_system::UiId::ApiTabBody => {
                self.commit_api_focus();
                self.ide_panel.api.focused = None;
            }
            _ => return false,
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
        self.pulse_api_cursor_blink();
        true
    }
}
