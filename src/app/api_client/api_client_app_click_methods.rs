fn api_mock_constraint_menu_contains_ui_id(
    menu: Option<crate::app::api_client::ApiMockContractConstraintMenu>,
    id: Option<crate::ui_system::UiId>,
) -> bool {
    let Some(menu) = menu else {
        return false;
    };
    match id {
        Some(crate::ui_system::UiId::ApiMockContractFieldAddConstraint(
            route_idx,
            group,
            field_idx,
        ))
        | Some(crate::ui_system::UiId::ApiMockContractFieldAddConstraintOption(
            route_idx,
            group,
            field_idx,
            _,
        )) => {
            route_idx == menu.route_idx && group == menu.group && field_idx == menu.field_idx
        }
        _ => false,
    }
}

type ApiClickPointerMetrics = ((f32, f32, f32, f32), f32, f32);

impl crate::app::App {
    /// Pointer position and scale for a scrollbar id, as the click handler needs
    /// them: the registry rect, the click y and the UI scale.
    fn api_click_pointer_metrics(
        &self,
        id: crate::ui_system::UiId,
    ) -> Option<ApiClickPointerMetrics> {
        let rect = self.ui_registry.rect_for(id)?;
        let (scale, pointer_y) = match self.renderer.as_ref() {
            Some(renderer) => (renderer.scale_factor, renderer.last_mouse_y),
            None => (1.0, rect.1),
        };
        Some((rect, pointer_y, scale))
    }

    pub(crate) fn close_active_api_output_example_menu(&mut self) -> bool {
        let Some((meta, state)) = self.active_api_tab() else {
            return false;
        };
        if !state.output_schema_menu_open {
            return false;
        }
        let spec_id = meta.spec_id;
        if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
            state.output_schema_menu_open = false;
            state.output_schema_menu_scroll.end_drag();
            true
        } else {
            false
        }
    }

    pub(crate) fn start_api_output_schema_menu_scroll_drag(&mut self, route_idx: usize) -> bool {
        let id = crate::ui_system::UiId::ApiOutputSchemaMenuScrollY(route_idx);
        let Some(rect) = self.ui_registry.rect_for(id) else {
            return false;
        };
        let pointer_y = self
            .renderer
            .as_ref()
            .map_or(rect.1, |renderer| renderer.last_mouse_y);
        let scale = self
            .renderer
            .as_ref()
            .map_or(1.0, |renderer| renderer.scale_factor);
        let Some((meta, state)) = self.active_api_tab() else {
            return false;
        };
        if state.route_idx != Some(route_idx)
            || !state.output_schema_menu_open
            || state.output_doc_view != crate::app::api_client::ApiOutputDocView::Example
        {
            return false;
        }
        let spec_id = meta.spec_id;
        let example_count = self
            .ide_panel
            .api
            .models
            .get(&spec_id)
            .and_then(|model| model.routes.get(route_idx))
            .map(|route| {
                crate::app::api_client::api_route_output_example_count(
                    route,
                    state.output_status_idx,
                )
            })
            .unwrap_or(0)
            .max(1);
        let current = state.output_schema_menu_scroll.current;
        let Some((drag_offset, target)) =
            crate::app::api_client::api_output_schema_menu_scrollbar_drag_target(
                rect,
                example_count,
                current,
                pointer_y,
                scale,
                None,
            )
        else {
            return false;
        };
        let Some((_, state)) = self.active_api_tab_mut_for(spec_id) else {
            return false;
        };
        crate::app::mouse::apply_scrollbar_drag_target(
            &mut state.output_schema_menu_scroll,
            target,
            drag_offset,
        )
    }

    pub(crate) fn update_api_output_schema_menu_scroll_drag(&mut self, pointer_y: f32) -> bool {
        let Some((meta, state)) = self.active_api_tab() else {
            return false;
        };
        if !state.output_schema_menu_scroll.is_dragging {
            return false;
        }
        let Some(route_idx) = state.route_idx else {
            return false;
        };
        let spec_id = meta.spec_id;
        let id = crate::ui_system::UiId::ApiOutputSchemaMenuScrollY(route_idx);
        let Some(rect) = self.ui_registry.rect_for(id) else {
            if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
                state.output_schema_menu_scroll.end_drag();
            }
            return false;
        };
        let scale = self
            .renderer
            .as_ref()
            .map_or(1.0, |renderer| renderer.scale_factor);
        let example_count = self
            .ide_panel
            .api
            .models
            .get(&spec_id)
            .and_then(|model| model.routes.get(route_idx))
            .map(|route| {
                crate::app::api_client::api_route_output_example_count(
                    route,
                    state.output_status_idx,
                )
            })
            .unwrap_or(0)
            .max(1);
        let current = state.output_schema_menu_scroll.current;
        let drag_offset = state.output_schema_menu_scroll.drag_offset;
        let Some((drag_offset, target)) =
            crate::app::api_client::api_output_schema_menu_scrollbar_drag_target(
                rect,
                example_count,
                current,
                pointer_y,
                scale,
                Some(drag_offset),
            )
        else {
            if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
                state.output_schema_menu_scroll.end_drag();
            }
            return false;
        };
        let Some((_, state)) = self.active_api_tab_mut_for(spec_id) else {
            return false;
        };
        crate::app::mouse::apply_scrollbar_drag_target(
            &mut state.output_schema_menu_scroll,
            target,
            drag_offset,
        )
    }

    pub(crate) fn start_api_python_runtime_scroll_drag(
        &mut self,
        id: crate::ui_system::UiId,
    ) -> bool {
        let Some(rect) = self.ui_registry.rect_for(id) else {
            return false;
        };
        let scale = self
            .renderer
            .as_ref()
            .map_or(1.0, |renderer| renderer.scale_factor);
        let pointer_y = self
            .renderer
            .as_ref()
            .map_or(rect.1, |renderer| renderer.last_mouse_y);
        self.ide_panel
            .api
            .start_api_python_runtime_scroll_drag(id, rect, pointer_y, scale)
    }

    pub(crate) fn update_api_python_runtime_scroll_drag(&mut self, pointer_y: f32) -> bool {
        let id = if self.ide_panel.api.mock_python_versions_scroll.is_dragging {
            crate::ui_system::UiId::ApiMockPythonVersionsScrollY
        } else if self
            .ide_panel
            .api
            .mock_python_install_log_scroll
            .is_dragging
        {
            crate::ui_system::UiId::ApiMockPythonInstallLogScrollY
        } else {
            return false;
        };
        let scale = self
            .renderer
            .as_ref()
            .map_or(1.0, |renderer| renderer.scale_factor);
        let rect = self.ui_registry.rect_for(id);
        self.ide_panel
            .api
            .update_api_python_runtime_scroll_drag(id, rect, pointer_y, scale)
    }

    pub fn handle_api_client_click(
        &mut self,
        id: crate::ui_system::UiId,
        same_click_target: bool,
    ) -> bool {
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
                | crate::ui_system::UiId::ApiInputSchemaBody(_)
                | crate::ui_system::UiId::ApiOutputSchemaBody(_)
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
                self.place_api_cursor_from_last_click(id, false, same_click_target);
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
                self.ide_panel.api.open_mock_server_details();
            }
            crate::ui_system::UiId::ApiMockServerCopyUrl => {
                self.commit_api_focus();
                if let Some(url) = self.ide_panel.api.mock.server_status.running_url() {
                    self.set_clipboard_text(url.to_string());
                    self.ide_panel.api.mock_server_url_copied_at =
                        Some(std::time::Instant::now());
                }
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
                if let Some((rect, pointer_y, scale)) = self.api_click_pointer_metrics(id) {
                    self.ide_panel
                        .api
                        .start_mock_server_log_scroll_drag(rect, pointer_y, scale);
                }
            }
            crate::ui_system::UiId::ApiMockModeSelect => {
                self.commit_api_focus();
                let next_mode = self.ide_panel.api.cycle_mock_mode();
                if next_mode == crate::app::api_mock::types::ApiMockMode::MockSelectedProxyRest {
                    self.sync_api_mock_proxy_base_to_active_server();
                }
                self.ide_panel.api.commit_mock_config();
            }
            crate::ui_system::UiId::ApiMockProxyBaseInput => {
                self.focus_api_input(ApiFocus::MockProxyBase);
                self.place_api_cursor_from_last_click(id, false, same_click_target);
            }
            crate::ui_system::UiId::ApiMockGuideOpen => {
                self.commit_api_focus();
                self.ide_panel.api.open_mock_guide();
            }
            crate::ui_system::UiId::ApiMockGuideClose => {
                self.ide_panel.api.mock_guide_open = false;
            }
            crate::ui_system::UiId::ApiMockGuideBody => {
                self.commit_api_focus();
                self.ide_panel.api.focused = None;
            }
            crate::ui_system::UiId::ApiMockGuideScrollY => {
                self.commit_api_focus();
                self.ide_panel.api.focused = None;
                if let Some((rect, pointer_y, scale)) = self.api_click_pointer_metrics(id) {
                    self.ide_panel
                        .api
                        .start_mock_guide_scroll_drag(rect, pointer_y, scale);
                }
            }
            crate::ui_system::UiId::ApiMockPythonManage => {
                self.commit_api_focus();
                self.ide_panel.api.open_mock_python_runtime();
            }
            crate::ui_system::UiId::ApiMockPythonManageClose => {
                self.commit_api_focus();
                self.ide_panel.api.close_mock_python_runtime();
            }
            crate::ui_system::UiId::ApiMockPythonModeToggle => {
                self.commit_api_focus();
                self.ide_panel.api.toggle_python_runtime_mode();
            }
            crate::ui_system::UiId::ApiMockPythonCheckRuntime => {
                self.commit_api_focus();
                self.ide_panel.api.check_python_runtime();
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
                self.place_api_cursor_from_last_click(id, false, same_click_target);
            }
            crate::ui_system::UiId::ApiMockPythonVersionInput => {
                self.commit_api_focus();
                if !self.ide_panel.api.close_mock_python_version_picker() {
                    self.trigger_api_python_version_list();
                }
            }
            crate::ui_system::UiId::ApiMockPythonVersionsScrollY
            | crate::ui_system::UiId::ApiMockPythonInstallLogScrollY => {
                self.start_api_python_runtime_scroll_drag(id);
            }
            crate::ui_system::UiId::ApiMockPythonVersionOption(idx) => {
                self.commit_api_focus();
                self.ide_panel.api.apply_python_version_option(idx);
            }
            crate::ui_system::UiId::ApiMockPythonPickCustomPath => {
                self.commit_api_focus();
                self.trigger_api_python_path_picker(ApiPythonPathPickKind::CustomPython);
            }
            crate::ui_system::UiId::ApiMockPythonCustomPathInput => {
                self.focus_api_input(ApiFocus::MockPythonCustomPath);
                self.place_api_cursor_from_last_click(id, false, same_click_target);
            }
            crate::ui_system::UiId::ApiMockManualRoutePath(manual_idx) => {
                self.focus_api_input(ApiFocus::MockManualPath { manual_idx });
                self.place_api_cursor_from_last_click(id, false, same_click_target);
            }
            crate::ui_system::UiId::ApiMockRouteEnable(route_idx) => {
                self.toggle_api_route_mock(route_idx);
            }
            crate::ui_system::UiId::ApiMockRouteDetailsToggle(route_idx) => {
                self.commit_api_focus();
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                if self.ide_panel.api.toggle_expanded_mock_route(spec_id, route_idx) {
                    self.start_api_mock_route_tools_now(route_idx);
                }
            }
            crate::ui_system::UiId::ApiMockRoutePythonToggle(route_idx) => {
                if self.toggle_api_route_python(route_idx) {
                    self.start_api_mock_route_tools_now(route_idx);
                }
            }
            crate::ui_system::UiId::ApiMockRouteReset(route_idx) => {
                self.open_api_mock_route_reset_dialog(route_idx);
            }
            crate::ui_system::UiId::ApiMockRouteResetConfirm => {
                self.confirm_api_mock_route_reset();
            }
            crate::ui_system::UiId::ApiMockRouteResetCancel => {
                self.ide_panel.api.mock_route_reset_dialog = None;
            }
            crate::ui_system::UiId::ApiMockExportOpenApi => {
                self.trigger_api_mock_export_openapi();
            }
            crate::ui_system::UiId::ApiMockContractPathToggle(route_idx) => {
                self.edit_api_mock_contract(route_idx, |api, active| {
                    api.toggle_api_mock_contract_path(active, route_idx)
                });
            }
            crate::ui_system::UiId::ApiMockContractQueryToggle(route_idx) => {
                self.edit_api_mock_contract(route_idx, |api, active| {
                    api.toggle_api_mock_contract_query(active, route_idx)
                });
            }
            crate::ui_system::UiId::ApiMockContractBodyToggle(route_idx) => {
                self.edit_api_mock_contract(route_idx, |api, active| {
                    api.toggle_api_mock_contract_body(active, route_idx)
                });
            }
            crate::ui_system::UiId::ApiMockContractPathFieldToggle(route_idx, field_idx) => {
                self.edit_api_mock_contract(route_idx, |api, active| {
                    api.toggle_api_mock_contract_path_field(active, route_idx, field_idx)
                });
            }
            crate::ui_system::UiId::ApiMockContractQueryFieldToggle(route_idx, field_idx) => {
                self.edit_api_mock_contract(route_idx, |api, active| {
                    api.toggle_api_mock_contract_query_field(active, route_idx, field_idx)
                });
            }
            crate::ui_system::UiId::ApiMockContractBodyFieldToggle(route_idx, field_idx) => {
                self.edit_api_mock_contract(route_idx, |api, active| {
                    api.toggle_api_mock_contract_body_field(active, route_idx, field_idx)
                });
            }
            crate::ui_system::UiId::ApiMockContractFieldRequired(route_idx, group, field_idx) => {
                self.ide_panel.api.mock_contract_constraint_menu = None;
                self.edit_api_mock_contract(route_idx, |api, active| {
                    api.toggle_api_mock_contract_field_required(active, route_idx, group, field_idx)
                });
            }
            crate::ui_system::UiId::ApiMockContractFieldNullable(route_idx, group, field_idx) => {
                self.ide_panel.api.mock_contract_constraint_menu = None;
                self.edit_api_mock_contract(route_idx, |api, active| {
                    api.toggle_api_mock_contract_field_nullable(active, route_idx, group, field_idx)
                });
            }
            crate::ui_system::UiId::ApiMockContractFieldRemove(route_idx, group, field_idx) => {
                self.open_api_mock_contract_field_delete_dialog(route_idx, group, field_idx);
            }
            crate::ui_system::UiId::ApiMockContractFieldRemoveConfirm => {
                self.confirm_api_mock_contract_field_delete();
            }
            crate::ui_system::UiId::ApiMockContractFieldRemoveCancel => {
                self.ide_panel.api.mock_contract_field_delete_dialog = None;
            }
            crate::ui_system::UiId::ApiMockContractFieldAddConstraint(
                route_idx,
                group,
                field_idx,
            ) => {
                self.ide_panel
                    .api
                    .toggle_mock_contract_constraint_menu(route_idx, group, field_idx);
            }
            crate::ui_system::UiId::ApiMockContractFieldAddConstraintOption(
                route_idx,
                group,
                field_idx,
                prop,
            ) => {
                self.add_api_mock_contract_field_constraint(route_idx, group, field_idx, prop);
            }
            crate::ui_system::UiId::ApiMockContractFieldPropInput(
                route_idx,
                group,
                field_idx,
                prop,
            ) => {
                self.ide_panel.api.mock_contract_constraint_menu = None;
                self.focus_api_input(ApiFocus::MockContractField {
                    route_idx,
                    group,
                    field_idx,
                    prop,
                });
                self.place_api_cursor_from_last_click(id, false, same_click_target);
            }
            crate::ui_system::UiId::ApiMockStaticResponseInput(route_idx) => {
                self.focus_api_input(ApiFocus::MockStaticResponse { route_idx });
                self.place_api_cursor_from_last_click(id, true, same_click_target);
            }
            crate::ui_system::UiId::ApiMockCombinedPython(_) => {}
            crate::ui_system::UiId::ApiMockCombinedScrollY(_) => {}
            crate::ui_system::UiId::ApiMockContractInput(route_idx) => {
                self.focus_api_input(ApiFocus::MockContract { route_idx });
                self.place_api_cursor_from_last_click(id, true, same_click_target);
            }
            crate::ui_system::UiId::ApiMockSignatureInput(route_idx) => {
                self.focus_api_input(ApiFocus::MockSignature { route_idx });
                self.place_api_cursor_from_last_click(id, true, same_click_target);
            }
            crate::ui_system::UiId::ApiMockAddManualRoute => {
                self.add_api_manual_route();
            }
            crate::ui_system::UiId::ApiMockManualRouteOpen(manual_idx) => {
                self.open_api_manual_route(manual_idx);
            }
            crate::ui_system::UiId::ApiMockPreludeInput(route_idx) => {
                self.focus_api_input(ApiFocus::MockPrelude { route_idx });
                self.place_api_cursor_from_last_click(id, true, same_click_target);
            }
            crate::ui_system::UiId::ApiMockBodyInput(route_idx) => {
                self.focus_api_input(ApiFocus::MockBody { route_idx });
                self.place_api_cursor_from_last_click(id, true, same_click_target);
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
                if self.ide_panel.api.cycle_manual_route_method(manual_idx) {
                    self.sync_api_manual_route_tabs();
                    self.ide_panel.api.commit_mock_config();
                }
            }
            crate::ui_system::UiId::ApiMockAddInputField(_)
            | crate::ui_system::UiId::ApiMockAddOutputField(_) => {}
            crate::ui_system::UiId::ApiMockManualRouteRemove(idx) => {
                if self.ide_panel.api.remove_manual_route(idx) {
                    self.sync_api_manual_route_tabs();
                    self.ide_panel.api.commit_mock_config();
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
                self.ide_panel.api.open_spec_remove_dialog(idx);
            }
            crate::ui_system::UiId::ApiSpecRemoveConfirm => {
                let Some(id) = self.ide_panel.api.take_spec_remove_dialog_id() else {
                    return true;
                };
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
                self.ide_panel.api.refresh_mock_server();
            }
            crate::ui_system::UiId::ApiSpecRemoveCancel => {
                self.ide_panel.api.spec_remove_dialog = None;
            }
            crate::ui_system::UiId::ApiSpecSelect(idx) => {
                if let Some(id) = self.ide_panel.api.specs.get(idx).map(|entry| entry.id) {
                    let already_selected = self.ide_panel.api.selected_spec == Some(id);
                    self.ide_panel.api.select_spec(id);
                    self.ensure_api_model_loaded(id);
                    if already_selected {
                        self.ide_panel.api.toggle_route_root_collapsed(id);
                    }
                    self.ide_panel.api.refresh_mock_server();
                }
            }
            crate::ui_system::UiId::ApiAuthRoot => {
                if let Some(spec_id) = self.ide_panel.api.selected_spec {
                    self.open_api_auth_tab(spec_id);
                }
            }
            crate::ui_system::UiId::ApiRouteTag(group_idx) => {
                self.ide_panel
                    .api
                    .toggle_selected_route_tag_collapsed(group_idx);
            }
            crate::ui_system::UiId::ApiRoutesRoot => {
                if let Some(spec_id) = self.ide_panel.api.selected_spec {
                    self.ide_panel.api.toggle_route_root_collapsed(spec_id);
                }
            }
            crate::ui_system::UiId::ApiRouteFilterInput => {
                self.focus_api_input(ApiFocus::RouteFilter);
            }
            crate::ui_system::UiId::ApiRouteFilterClear => {
                if self.ide_panel.api.clear_route_filter() {
                    self.pulse_api_cursor_blink();
                }
            }
            crate::ui_system::UiId::ApiRouteRow(route_idx) => {
                if let Some(spec_id) = self.ide_panel.api.selected_spec {
                    if self.modifiers.control_key() {
                        self.open_api_route_with_new_tab(spec_id, route_idx, true);
                    } else {
                        self.open_api_route(spec_id, route_idx);
                    }
                }
            }
            crate::ui_system::UiId::ApiRoutePathText(route_idx) => {
                self.commit_api_focus();
                self.ide_panel.api.focused = None;
                self.begin_api_route_text_selection(ApiRouteTextField::Path, route_idx);
            }
            crate::ui_system::UiId::ApiRouteSummaryText(route_idx) => {
                self.commit_api_focus();
                self.ide_panel.api.focused = None;
                self.begin_api_route_text_selection(ApiRouteTextField::Summary, route_idx);
            }
            crate::ui_system::UiId::ApiRouteDescriptionText(route_idx) => {
                self.commit_api_focus();
                self.ide_panel.api.focused = None;
                self.begin_api_route_text_selection(ApiRouteTextField::Description, route_idx);
            }
            crate::ui_system::UiId::ApiServerSelect(idx) => {
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                let selected_server = self
                    .ide_panel
                    .api
                    .models
                    .get(&spec_id)
                    .and_then(|model| model.servers.get(idx))
                    .cloned();
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
                    state.server_idx = idx;
                }
                if let Some(server) = selected_server {
                    self.ide_panel.api.sync_api_mock_proxy_base_to_server(&server);
                    self.ide_panel.api.refresh_mock_server();
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
                let Some(scheme) = self.ide_panel.api.auth_scheme_name(spec_id, scheme_idx) else {
                    return true;
                };
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
                self.place_api_cursor_from_last_click(id, false, same_click_target);
            }
            crate::ui_system::UiId::ApiAuthSave(_) => {
                self.commit_api_focus();
                self.ide_panel.api.focused = None;
            }
            crate::ui_system::UiId::ApiAuthAccessSave(_)
            | crate::ui_system::UiId::ApiAuthRefreshSave(_) => {
                self.commit_api_focus();
                self.ide_panel.api.focused = None;
            }
            crate::ui_system::UiId::ApiAuthAccessClear(scheme_idx) => {
                self.commit_api_focus();
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                self.ide_panel.api.clear_auth_access(spec_id, scheme_idx);
            }
            crate::ui_system::UiId::ApiAuthRefreshClear(scheme_idx) => {
                self.commit_api_focus();
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                self.ide_panel.api.clear_auth_refresh(spec_id, scheme_idx);
            }
            crate::ui_system::UiId::ApiAuthClear(scheme_idx) => {
                self.commit_api_focus();
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                self.ide_panel.api.remove_auth_entry(spec_id, scheme_idx);
            }
            crate::ui_system::UiId::ApiTryRequest => {
                self.start_active_api_request();
            }
            crate::ui_system::UiId::ApiPathParamAllowedValue(route_idx, param_idx, value_idx)
            | crate::ui_system::UiId::ApiQueryParamAllowedValue(route_idx, param_idx, value_idx) => {
                self.commit_api_focus();
                let path = matches!(
                    id,
                    crate::ui_system::UiId::ApiPathParamAllowedValue(_, _, _)
                );
                let Some((meta, state, api)) = self.active_api_tab_and_state_mut() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                let route_shown = state.route_idx == Some(route_idx);
                if route_shown
                    && api.apply_param_allowed_value_click(
                        state,
                        spec_id,
                        route_idx,
                        param_idx,
                        value_idx,
                        path,
                    )
                {
                    api.focused = None;
                }
            }
            crate::ui_system::UiId::ApiResponseBodyTab(route_idx)
            | crate::ui_system::UiId::ApiResponseHeadersTab(route_idx)
            | crate::ui_system::UiId::ApiResponseCurlTab(route_idx) => {
                let Some((meta, state, _)) = self.active_api_tab_and_state_mut() else {
                    return true;
                };
                if state.route_idx != Some(route_idx) {
                    return true;
                }
                let spec_id = meta.spec_id;
                let view = match id {
                    crate::ui_system::UiId::ApiResponseHeadersTab(_) => ApiResponseView::Headers,
                    crate::ui_system::UiId::ApiResponseCurlTab(_) => ApiResponseView::Curl,
                    _ => ApiResponseView::Body,
                };
                state.set_response_view(view);
                self.focus_api_input(ApiFocus::Response { spec_id, route_idx });
            }
            crate::ui_system::UiId::ApiInputExampleTab(route_idx)
            | crate::ui_system::UiId::ApiInputSchemaTab(route_idx) => {
                let Some((meta, state)) = self.active_api_tab() else {
                    return true;
                };
                if state.route_idx != Some(route_idx) {
                    return true;
                }
                let spec_id = meta.spec_id;
                let view = match id {
                    crate::ui_system::UiId::ApiInputSchemaTab(_) => ApiInputDocView::Schema,
                    _ => ApiInputDocView::Input,
                };
                self.commit_api_focus();
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
                    state.select_input_doc_view(view);
                }
                if matches!(view, ApiInputDocView::Schema) {
                    self.focus_api_input(ApiFocus::InputSchema { spec_id, route_idx });
                }
            }
            crate::ui_system::UiId::ApiInputSchemaMenu(route_idx) => {
                let Some((meta, state)) = self.active_api_tab() else {
                    return true;
                };
                if state.route_idx != Some(route_idx) {
                    return true;
                }
                let spec_id = meta.spec_id;
                self.commit_api_focus();
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
                    state.input_schema_menu_open = !state.input_schema_menu_open;
                }
            }
            crate::ui_system::UiId::ApiInputSchemaMenuItem(route_idx, media_idx) => {
                let Some((meta, state)) = self.active_api_tab() else {
                    return true;
                };
                if state.route_idx != Some(route_idx) {
                    return true;
                }
                let spec_id = meta.spec_id;
                self.commit_api_focus();
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
                    state.select_input_schema_media(media_idx);
                }
            }
            crate::ui_system::UiId::ApiOutputExampleTab(route_idx)
            | crate::ui_system::UiId::ApiOutputSchemaTab(route_idx) => {
                let Some((meta, state)) = self.active_api_tab() else {
                    return true;
                };
                if state.route_idx != Some(route_idx) {
                    return true;
                }
                let spec_id = meta.spec_id;
                let view = match id {
                    crate::ui_system::UiId::ApiOutputSchemaTab(_) => ApiOutputDocView::Schema,
                    _ => ApiOutputDocView::Example,
                };
                self.commit_api_focus();
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
                    state.select_output_doc_view(view);
                }
                if matches!(
                    self.ide_panel.api.focused,
                    Some(ApiFocus::OutputSchema {
                        spec_id: focused_spec,
                        route_idx: focused_route,
                    }) if focused_spec == spec_id && focused_route == route_idx
                ) {
                    self.focus_api_input(ApiFocus::OutputSchema { spec_id, route_idx });
                }
            }
            crate::ui_system::UiId::ApiOutputStatusTab(route_idx, status_idx) => {
                let Some((meta, state)) = self.active_api_tab() else {
                    return true;
                };
                if state.route_idx != Some(route_idx) {
                    return true;
                }
                let spec_id = meta.spec_id;
                self.commit_api_focus();
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
                    state.select_output_status(status_idx);
                }
                if matches!(
                    self.ide_panel.api.focused,
                    Some(ApiFocus::OutputSchema {
                        spec_id: focused_spec,
                        route_idx: focused_route,
                    }) if focused_spec == spec_id && focused_route == route_idx
                ) {
                    self.focus_api_input(ApiFocus::OutputSchema { spec_id, route_idx });
                }
            }
            crate::ui_system::UiId::ApiOutputSchemaMenu(route_idx) => {
                let Some((meta, state)) = self.active_api_tab() else {
                    return true;
                };
                if state.route_idx != Some(route_idx) {
                    return true;
                }
                let spec_id = meta.spec_id;
                let can_open = state.output_doc_view == ApiOutputDocView::Example
                    && self
                        .ide_panel
                        .api
                        .models
                        .get(&spec_id)
                        .and_then(|model| {
                            model.routes.get(route_idx).map(|route| {
                                crate::app::api_client::api_route_output_example_count(
                                    route,
                                    state.output_status_idx,
                                )
                            })
                        })
                        .unwrap_or(0)
                        > 1;
                self.commit_api_focus();
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
                    state.toggle_output_schema_menu(can_open);
                }
            }
            crate::ui_system::UiId::ApiOutputSchemaMenuScrollY(route_idx) => {
                self.start_api_output_schema_menu_scroll_drag(route_idx);
            }
            crate::ui_system::UiId::ApiOutputSchemaMenuItem(route_idx, media_idx) => {
                let Some((meta, state)) = self.active_api_tab() else {
                    return true;
                };
                if state.route_idx != Some(route_idx) {
                    return true;
                }
                let spec_id = meta.spec_id;
                self.commit_api_focus();
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
                    state.select_output_media(media_idx);
                }
                if matches!(
                    self.ide_panel.api.focused,
                    Some(ApiFocus::OutputSchema {
                        spec_id: focused_spec,
                        route_idx: focused_route,
                    }) if focused_spec == spec_id && focused_route == route_idx
                ) {
                    self.focus_api_input(ApiFocus::OutputSchema { spec_id, route_idx });
                }
            }
            crate::ui_system::UiId::ApiInputSchemaBody(route_idx) => {
                let Some((meta, state)) = self.active_api_tab() else {
                    return true;
                };
                if state.route_idx != Some(route_idx) {
                    return true;
                }
                let spec_id = meta.spec_id;
                self.focus_api_input(ApiFocus::InputSchema { spec_id, route_idx });
                self.place_api_cursor_from_last_click(id, true, same_click_target);
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
                    state.focused_schema_pane =
                        Some(crate::app::api_client::ApiSchemaPaneFocus::Input);
                }
            }
            crate::ui_system::UiId::ApiInputSchemaFold(route_idx, line_idx) => {
                let Some((meta, state)) = self.active_api_tab() else {
                    return true;
                };
                if state.route_idx != Some(route_idx) {
                    return true;
                }
                let spec_id = meta.spec_id;
                let key = self
                    .ide_panel
                    .api
                    .input_schema_fold_key(state, spec_id, route_idx, line_idx);
                if let Some(key) = key
                    && let Some((_, state)) = self.active_api_tab_mut_for(spec_id)
                {
                    state.apply_input_schema_fold(key);
                }
            }
            crate::ui_system::UiId::ApiOutputSchemaBody(route_idx) => {
                let Some((meta, state)) = self.active_api_tab() else {
                    return true;
                };
                if state.route_idx != Some(route_idx) {
                    return true;
                }
                let spec_id = meta.spec_id;
                self.focus_api_input(ApiFocus::OutputSchema { spec_id, route_idx });
                self.place_api_cursor_from_last_click(id, true, same_click_target);
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
                    state.focused_schema_pane =
                        Some(crate::app::api_client::ApiSchemaPaneFocus::Output);
                }
            }
            crate::ui_system::UiId::ApiOutputSchemaFold(route_idx, line_idx) => {
                let Some((meta, state)) = self.active_api_tab() else {
                    return true;
                };
                if state.route_idx != Some(route_idx) {
                    return true;
                }
                let spec_id = meta.spec_id;
                let key = self
                    .ide_panel
                    .api
                    .output_schema_fold_key(state, spec_id, route_idx, line_idx);
                if let Some(key) = key
                    && let Some((_, state)) = self.active_api_tab_mut_for(spec_id)
                {
                    state.apply_output_schema_fold(key);
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
                self.place_api_cursor_from_last_click(id, false, same_click_target);
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
                self.place_api_cursor_from_last_click(id, false, same_click_target);
            }
            crate::ui_system::UiId::ApiBodyInput(route_idx) => {
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                self.is_dragging = true;
                self.ide_panel.is_dragging_terminal = false;
                self.focus_api_input(ApiFocus::Body { spec_id, route_idx });
                self.place_api_cursor_from_last_click(id, true, same_click_target);
            }
            crate::ui_system::UiId::ApiBodyScrollX(_)
            | crate::ui_system::UiId::ApiOutputScrollX(_)
            | crate::ui_system::UiId::ApiMockStaticResponseScrollX(_)
            | crate::ui_system::UiId::ApiResponseScrollX(_) => {
                self.start_api_text_scrollbar_x_drag(id);
            }
            crate::ui_system::UiId::ApiBodyScrollY(_)
            | crate::ui_system::UiId::ApiOutputScrollY(_)
            | crate::ui_system::UiId::ApiMockStaticResponseScrollY(_)
            | crate::ui_system::UiId::ApiResponseScrollY(_) => {
                self.start_api_text_scrollbar_y_drag(id);
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
                self.place_api_cursor_from_last_click(id, false, same_click_target);
            }
            crate::ui_system::UiId::ApiBodyAllowedValue(route_idx, prop_idx, value_idx) => {
                self.commit_api_focus();
                let Some((meta, state, api)) = self.active_api_tab_and_state_mut() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                api.apply_body_allowed_value_click(state, spec_id, route_idx, prop_idx, value_idx);
            }
            crate::ui_system::UiId::ApiBodyFilePick(route_idx, prop_idx) => {
                self.commit_api_focus();
                let Some((meta, _)) = self.active_api_tab() else {
                    return true;
                };
                let spec_id = meta.spec_id;
                if let Some((name, multi)) =
                    self.ide_panel.api.body_file_pick_target(spec_id, route_idx, prop_idx)
                {
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
                self.place_api_cursor_from_last_click(id, true, same_click_target);
            }
            crate::ui_system::UiId::ApiTabBody => {
                self.commit_api_focus();
                self.ide_panel.api.focused = None;
                if let Some((_, state, _)) = self.active_api_tab_and_state_mut() {
                    state.clear_focus_marks();
                }
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

#[cfg(test)]
mod constraint_menu_tests {
    use super::api_mock_constraint_menu_contains_ui_id;
    use crate::app::api_client::ApiMockContractConstraintMenu;
    use crate::ui_system::{ApiMockContractFieldGroup, ApiMockContractFieldProp, UiId};

    #[test]
    fn constraint_menu_only_keeps_its_own_button_and_options_inside() {
        let menu = Some(ApiMockContractConstraintMenu {
            route_idx: 2,
            group: ApiMockContractFieldGroup::Body,
            field_idx: 4,
        });

        assert!(api_mock_constraint_menu_contains_ui_id(menu, Some(
            UiId::ApiMockContractFieldAddConstraint(2, ApiMockContractFieldGroup::Body, 4),
        )));
        assert!(api_mock_constraint_menu_contains_ui_id(menu, Some(
            UiId::ApiMockContractFieldAddConstraintOption(
                2,
                ApiMockContractFieldGroup::Body,
                4,
                ApiMockContractFieldProp::Minimum,
            ),
        )));
        assert!(!api_mock_constraint_menu_contains_ui_id(menu, Some(
            UiId::ApiMockContractFieldAddConstraint(3, ApiMockContractFieldGroup::Body, 4),
        )));
        assert!(!api_mock_constraint_menu_contains_ui_id(menu, None));
    }
}
