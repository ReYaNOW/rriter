// State transitions for `handle_api_client_click` arms. Each method owns the
// fields that must change together, so the click handler only routes: it picks
// the tab, measures the UI (rects, scale, pointer) and runs the effects that
// belong to App (focus, dialogs, clipboard, threads, tabs, redraw).

impl ApiClientTabState {
    /// Response view switch: the view and both of its scrolls move together.
    fn set_response_view(&mut self, view: ApiResponseView) {
        self.response_view = view;
        self.response_scroll.reset();
        self.response_scroll_x.reset();
    }

    /// Input example/schema switch, including closing the media menu and
    /// dropping both body scrolls.
    fn select_input_doc_view(&mut self, view: ApiInputDocView) {
        self.input_doc_view = view;
        self.input_schema_menu_open = false;
        self.body_scroll.reset();
        self.body_scroll_x.reset();
    }

    fn select_input_schema_media(&mut self, media_idx: usize) {
        self.input_schema_idx = media_idx;
        self.input_schema_menu_open = false;
        self.body_scroll.reset();
        self.body_scroll_x.reset();
    }

    fn select_output_doc_view(&mut self, view: ApiOutputDocView) {
        self.output_doc_view = view;
        self.output_scroll.reset();
        self.output_scroll_x.reset();
        if view != ApiOutputDocView::Example {
            self.output_schema_menu_open = false;
        }
    }

    /// Status switch resets the picked example/schema and every dependent
    /// scroll, because all of them are indexed by the status.
    fn select_output_status(&mut self, status_idx: usize) {
        self.output_status_idx = status_idx;
        self.output_example_idx = 0;
        self.output_schema_idx = 0;
        self.output_schema_menu_open = false;
        self.output_schema_menu_scroll.reset();
        self.output_scroll.reset();
        self.output_scroll_x.reset();
    }

    fn select_output_media(&mut self, media_idx: usize) {
        if self.output_doc_view == ApiOutputDocView::Example {
            self.output_example_idx = media_idx;
        } else {
            self.output_schema_idx = media_idx;
        }
        self.output_schema_menu_open = false;
        self.output_scroll.reset();
        self.output_scroll_x.reset();
    }

    fn toggle_output_schema_menu(&mut self, can_open: bool) {
        if can_open {
            self.output_schema_menu_open = !self.output_schema_menu_open;
            self.output_schema_menu_scroll.reset();
        } else {
            self.output_schema_menu_open = false;
        }
    }

    fn apply_input_schema_fold(&mut self, key: String) {
        self.focused_schema_pane = Some(ApiSchemaPaneFocus::Input);
        if !self.input_schema_collapsed.remove(&key) {
            self.input_schema_collapsed.insert(key);
        }
    }

    fn apply_output_schema_fold(&mut self, key: String) {
        self.focused_schema_pane = Some(ApiSchemaPaneFocus::Output);
        if !self.output_schema_collapsed.remove(&key) {
            self.output_schema_collapsed.insert(key);
        }
    }

    /// Drops the marks that a click on the tab body clears.
    fn clear_focus_marks(&mut self) {
        self.focused_schema_pane = None;
        self.route_text_selection = None;
    }

    /// Writes an enum/example value picked for a path or query parameter.
    fn apply_allowed_param_value(
        &mut self,
        path: bool,
        name: String,
        is_array: bool,
        value: String,
    ) {
        let values = if path {
            &mut self.path_values
        } else {
            &mut self.query_values
        };
        if let Some(field) = values.iter_mut().find(|field| field.name == name) {
            if is_array {
                push_api_array_value(&mut field.value, &value);
            } else {
                field.value = value;
            }
        }
    }
}

impl ApiClientState {
    fn open_mock_server_details(&mut self) {
        self.mock_server_detail_open = true;
        self.mock_guide_open = false;
        self.mock_python_runtime_open = false;
    }

    fn open_mock_guide(&mut self) {
        self.mock_guide_open = true;
        self.mock_server_detail_open = false;
        self.mock_python_runtime_open = false;
    }

    fn open_mock_python_runtime(&mut self) {
        clear_legacy_api_python_runtime_message(self);
        self.mock_python_runtime_open = true;
        self.mock_guide_open = false;
        self.mock_server_detail_open = false;
        if matches!(
            self.mock.uv.mode,
            crate::app::api_mock::types::ApiPythonRuntimeMode::UvManaged
        ) && self.mock.uv.selected_uv_path().is_none()
        {
            crate::app::api_mock::python_bootstrap::refresh_uv_status(&mut self.mock.uv);
        }
    }

    fn close_mock_python_runtime(&mut self) {
        self.mock_python_runtime_open = false;
        self.mock_python_version_picker_open = false;
    }

    fn toggle_python_runtime_mode(&mut self) {
        clear_legacy_api_python_runtime_message(self);
        self.mock.uv.mode = match self.mock.uv.mode {
            crate::app::api_mock::types::ApiPythonRuntimeMode::UvManaged => {
                crate::app::api_mock::types::ApiPythonRuntimeMode::CustomPython
            }
            crate::app::api_mock::types::ApiPythonRuntimeMode::CustomPython => {
                crate::app::api_mock::types::ApiPythonRuntimeMode::UvManaged
            }
        };
        self.mock_python_version_picker_open = false;
        self.commit_mock_config();
    }

    fn check_python_runtime(&mut self) {
        crate::app::api_mock::python_bootstrap::refresh_python_runtime_status(&mut self.mock.uv);
        self.commit_mock_config();
    }

    /// Closes the version picker; `false` when it was already closed and the
    /// caller has to fetch the version list instead.
    fn close_mock_python_version_picker(&mut self) -> bool {
        if !self.mock_python_version_picker_open {
            return false;
        }
        self.mock_python_version_picker_open = false;
        true
    }

    /// Picks a version row: the picker, the version and its scroll move together
    /// and the mock config is committed once.
    fn apply_python_version_option(&mut self, idx: usize) {
        let Some(row) = self.mock_python_versions.get(idx) else {
            return;
        };
        let version = row.version.clone();
        self.mock.uv.python_version = version;
        self.mock_python_version_picker_open = false;
        self.mock_python_versions_scroll.reset();
        self.commit_mock_config();
    }

    fn cycle_mock_mode(&mut self) -> crate::app::api_mock::types::ApiMockMode {
        let next_mode = match self.mock.mode.canonical() {
            crate::app::api_mock::types::ApiMockMode::MockAll => {
                crate::app::api_mock::types::ApiMockMode::MockSelectedProxyRest
            }
            crate::app::api_mock::types::ApiMockMode::MockSelectedProxyRest
            | crate::app::api_mock::types::ApiMockMode::MockSelectedOnly => {
                crate::app::api_mock::types::ApiMockMode::MockAll
            }
        };
        self.mock.mode = next_mode;
        next_mode
    }

    /// Toggles the route's mock detail panel; `true` when it is now expanded and
    /// the caller has to start the route's ty tools.
    fn toggle_expanded_mock_route(&mut self, spec_id: ApiSpecId, route_idx: usize) -> bool {
        let key = (spec_id, route_idx);
        if self.expanded_mock_routes.contains(&key) {
            self.expanded_mock_routes.remove(&key);
            false
        } else {
            self.expanded_mock_routes.insert(key);
            true
        }
    }

    fn toggle_mock_contract_constraint_menu(
        &mut self,
        route_idx: usize,
        group: crate::ui_system::ApiMockContractFieldGroup,
        field_idx: usize,
    ) {
        let current = self.mock_contract_constraint_menu;
        let next = ApiMockContractConstraintMenu {
            route_idx,
            group,
            field_idx,
        };
        self.mock_contract_constraint_menu = (current != Some(next)).then_some(next);
    }

    fn start_mock_server_log_scroll_drag(
        &mut self,
        rect: (f32, f32, f32, f32),
        pointer_y: f32,
        scale: f32,
    ) {
        if let Some((drag_offset, target)) = api_mock_server_log_scrollbar_drag_target(
            rect,
            self.mock_server_logs.len(),
            self.mock_server_log_scroll.current,
            pointer_y,
            scale,
            None,
        ) {
            crate::app::mouse::apply_scrollbar_drag_target(
                &mut self.mock_server_log_scroll,
                target,
                drag_offset,
            );
        }
    }

    fn start_mock_guide_scroll_drag(
        &mut self,
        rect: (f32, f32, f32, f32),
        pointer_y: f32,
        scale: f32,
    ) {
        let geometry = api_mock_guide_scrollbar(rect, self.mock_guide_scroll.current, scale)
            .geometry(scale);
        if let Some((drag_offset, target)) =
            geometry.and_then(|geometry| geometry.press_target(pointer_y))
        {
            crate::app::mouse::apply_scrollbar_drag_target(
                &mut self.mock_guide_scroll,
                target,
                drag_offset,
            );
        }
    }

    /// Cycles the method of a manual mock route; `false` when the index is gone
    /// and there is nothing to re-sync or commit.
    fn cycle_manual_route_method(&mut self, manual_idx: usize) -> bool {
        let Some(route) = self.mock.manual_routes.get_mut(manual_idx) else {
            return false;
        };
        route.method = match route.method {
            ApiMethod::Get => ApiMethod::Post,
            ApiMethod::Post => ApiMethod::Put,
            ApiMethod::Put => ApiMethod::Patch,
            ApiMethod::Patch => ApiMethod::Delete,
            ApiMethod::Delete => ApiMethod::Get,
            ApiMethod::Head | ApiMethod::Options | ApiMethod::Trace => ApiMethod::Get,
        };
        true
    }

    fn remove_manual_route(&mut self, idx: usize) -> bool {
        if idx >= self.mock.manual_routes.len() {
            return false;
        }
        self.mock.manual_routes.remove(idx);
        true
    }

    fn open_spec_remove_dialog(&mut self, idx: usize) {
        let (spec_id, title, source) = {
            let Some(entry) = self.specs.get(idx) else {
                return;
            };
            let source = match &entry.source {
                ApiSpecSource::Local(path) => path.to_string_lossy().into_owned(),
                ApiSpecSource::Url(url) => url.clone(),
            };
            (entry.id, entry.title.clone(), source)
        };
        self.spec_remove_dialog = Some(ApiSpecRemoveDialog {
            spec_id,
            title,
            source,
        });
    }

    /// Confirms the remove dialog: takes it, drops the spec and returns the id
    /// whose tabs the caller has to close.
    fn take_spec_remove_dialog_id(&mut self) -> Option<ApiSpecId> {
        let dialog = self.spec_remove_dialog.take()?;
        let idx = self
            .specs
            .iter()
            .position(|entry| entry.id == dialog.spec_id)?;
        self.remove_spec(idx)
    }

    fn toggle_route_root_collapsed(&mut self, spec_id: ApiSpecId) {
        if self.collapsed_route_roots.contains(&spec_id) {
            self.collapsed_route_roots.remove(&spec_id);
        } else {
            self.collapsed_route_roots.insert(spec_id);
        }
    }

    /// Collapses/expands the tag group of the selected spec's route list.
    fn toggle_selected_route_tag_collapsed(&mut self, group_idx: usize) {
        let Some(spec_id) = self.selected_spec else {
            return;
        };
        let tag = self
            .models
            .get(&spec_id)
            .and_then(|model| {
                let group = model.route_groups.get(group_idx)?;
                model.routes.get(group.start)
            })
            .map(|route| route.tag.clone());
        if let Some(tag) = tag {
            self.toggle_tag_collapsed(spec_id, tag.as_str());
        }
    }

    /// Clears the route filter; `true` when the filter was the focused input and
    /// the caller has to restart the cursor blink.
    fn clear_route_filter(&mut self) -> bool {
        self.route_filter.clear();
        if !matches!(self.focused, Some(ApiFocus::RouteFilter)) {
            return false;
        }
        let old_version = self.input_editor.version;
        self.input_editor.set_text_clean("");
        self.input_editor.version = crate::editor::next_editor_version(old_version);
        self.input_editor.cursor = 0;
        self.input_editor.selection_anchor = None;
        self.input_scroll_x.reset();
        true
    }

    /// Name of the security scheme a click landed on, when the spec still has it.
    fn auth_scheme_name(&self, spec_id: ApiSpecId, scheme_idx: usize) -> Option<String> {
        self.models
            .get(&spec_id)
            .and_then(|model| model.security_schemes.get(scheme_idx))
            .map(|scheme| scheme.name.clone())
    }

    fn clear_auth_access(&mut self, spec_id: ApiSpecId, scheme_idx: usize) {
        let Some(scheme) = self.auth_scheme_name(spec_id, scheme_idx) else {
            return;
        };
        let entry = self.auth.entry_mut(spec_id, &scheme);
        entry.access_token.clear();
        entry.value.clear();
        self.focused = None;
        self.persist_credentials();
    }

    fn clear_auth_refresh(&mut self, spec_id: ApiSpecId, scheme_idx: usize) {
        let Some(scheme) = self.auth_scheme_name(spec_id, scheme_idx) else {
            return;
        };
        self.auth
            .entry_mut(spec_id, &scheme)
            .refresh_token
            .clear();
        self.focused = None;
        self.persist_credentials();
    }

    fn remove_auth_entry(&mut self, spec_id: ApiSpecId, scheme_idx: usize) {
        let Some(scheme) = self.auth_scheme_name(spec_id, scheme_idx) else {
            return;
        };
        self.auth.remove(spec_id, &scheme);
        self.focused = None;
        self.persist_credentials();
    }

    /// Applies a clicked enum/example value to the tab's path or query input;
    /// `false` when the model or route no longer has that value.
    fn apply_param_allowed_value_click(
        &mut self,
        state: &mut ApiClientTabState,
        spec_id: ApiSpecId,
        route_idx: usize,
        param_idx: usize,
        value_idx: usize,
        path: bool,
    ) -> bool {
        let picked = self
            .models
            .get(&spec_id)
            .and_then(|model| model.routes.get(route_idx))
            .and_then(|route| {
                if path {
                    route.path_params.get(param_idx).map(|param| (true, param))
                } else {
                    route.query_params.get(param_idx).map(|param| (false, param))
                }
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
        let Some((path, name, is_array, value)) = picked else {
            return false;
        };
        state.apply_allowed_param_value(path, name, is_array, value);
        true
    }

    /// Applies a clicked enum/example value to the tab's body input, mirroring it
    /// into the focused body field editor.
    fn apply_body_allowed_value_click(
        &mut self,
        state: &mut ApiClientTabState,
        spec_id: ApiSpecId,
        route_idx: usize,
        prop_idx: usize,
        value_idx: usize,
    ) {
        let picked = self
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
        let Some((name, is_array, value)) = picked else {
            return;
        };
        let mut applied = None;
        if let Some(field) = state
            .body_values
            .iter_mut()
            .find(|field| field.name == name)
        {
            if is_array {
                push_api_array_value(&mut field.value, &value);
            } else {
                field.value = value.clone();
            }
            state.body_file_paths.remove(&name);
            applied = Some((field.name.clone(), field.value.clone(), is_array));
        }
        if let Some((field_name, value, _)) = &applied
            && matches!(
                self.focused,
                Some(ApiFocus::BodyField {
                    spec_id: f_spec,
                    route_idx: f_route,
                    ref name,
                }) if f_spec == spec_id && f_route == route_idx && name == field_name
            )
        {
            let old_version = self.input_editor.version;
            self.input_editor.set_text_clean(value);
            self.input_editor.version = crate::editor::next_editor_version(old_version);
        }
        if applied.is_some_and(|(_, _, is_array)| is_array) {
            self.focused = None;
        }
    }

    /// Body property a file pick was requested for: name plus whether the schema
    /// accepts several files.
    fn body_file_pick_target(
        &self,
        spec_id: ApiSpecId,
        route_idx: usize,
        prop_idx: usize,
    ) -> Option<(String, bool)> {
        self.models
            .get(&spec_id)
            .and_then(|model| model.routes.get(route_idx).map(|route| (model, route)))
            .and_then(|(model, route)| {
                let root = route.request_body.as_ref()?.schema?;
                let prop = model.schema_arena.get(root.0)?.properties.get(prop_idx)?;
                let schema = model.schema_arena.get(prop.schema.0)?;
                Some((prop.name.clone(), api_schema_is_multi_file_input(schema, model)))
            })
    }

    /// Foldable schema key under the clicked input-schema line, if any.
    fn input_schema_fold_key(
        &self,
        state: &ApiClientTabState,
        spec_id: ApiSpecId,
        route_idx: usize,
        line_idx: usize,
    ) -> Option<String> {
        self.models
            .get(&spec_id)
            .and_then(|model| {
                model.routes.get(route_idx).and_then(|route| {
                    api_route_input_schema_fold_key_at_line(
                        route,
                        model,
                        state.input_schema_idx,
                        &state.input_schema_collapsed,
                        line_idx,
                    )
                })
            })
    }

    /// Foldable schema key under the clicked output-schema line, if any.
    fn output_schema_fold_key(
        &self,
        state: &ApiClientTabState,
        spec_id: ApiSpecId,
        route_idx: usize,
        line_idx: usize,
    ) -> Option<String> {
        self.models
            .get(&spec_id)
            .and_then(|model| {
                model.routes.get(route_idx).and_then(|route| {
                    api_route_output_schema_fold_key_at_line(
                        route,
                        model,
                        state.output_status_idx,
                        state.output_schema_idx,
                        &state.output_schema_collapsed,
                        line_idx,
                    )
                })
            })
    }
}
