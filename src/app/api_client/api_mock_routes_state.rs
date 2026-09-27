impl ApiClientState {
    fn manual_mock_route(
        &self,
        stable_id: &str,
    ) -> Option<&crate::app::api_mock::types::ApiManualRoute> {
        self.mock
            .manual_routes
            .iter()
            .find(|route| route.stable_id == stable_id)
    }

    fn manual_mock_route_mut(
        &mut self,
        stable_id: &str,
    ) -> Option<&mut crate::app::api_mock::types::ApiManualRoute> {
        self.mock
            .manual_routes
            .iter_mut()
            .find(|route| route.stable_id == stable_id)
    }

    fn api_route_override(
        &self,
        spec_id: ApiSpecId,
        route_idx: usize,
    ) -> Option<&crate::app::api_mock::types::ApiMockRouteOverride> {
        let entry = self.specs.iter().find(|entry| entry.id == spec_id)?;
        let route = self.models.get(&spec_id)?.routes.get(route_idx)?;
        let source_key = crate::app::api_mock::types::api_mock_source_key(entry);
        self.mock.route_overrides.iter().find(|item| {
            item.source_key == source_key && item.method == route.method && item.path == route.path
        })
    }

    fn api_route_override_mut(
        &mut self,
        spec_id: ApiSpecId,
        route_idx: usize,
    ) -> Option<&mut crate::app::api_mock::types::ApiMockRouteOverride> {
        let entry = self.specs.iter().find(|entry| entry.id == spec_id).cloned()?;
        let route = self.models.get(&spec_id)?.routes.get(route_idx).cloned()?;
        let source_key = crate::app::api_mock::types::api_mock_source_key(&entry);
        self.mock.route_overrides.iter_mut().find(|item| {
            item.source_key == source_key && item.method == route.method && item.path == route.path
        })
    }

    fn ensure_api_route_override(&mut self, spec_id: ApiSpecId, route_idx: usize) {
        if self.api_route_override(spec_id, route_idx).is_some() {
            return;
        }
        self.add_api_route_override(spec_id, route_idx, false);
    }

    fn add_api_route_override(&mut self, spec_id: ApiSpecId, route_idx: usize, enabled: bool) {
        let Some(entry) = self.specs.iter().find(|entry| entry.id == spec_id).cloned() else {
            return;
        };
        let Some(route) = self
            .models
            .get(&spec_id)
            .and_then(|model| model.routes.get(route_idx))
            .cloned()
        else {
            return;
        };
        self.mock.route_overrides.push(
            crate::app::api_mock::types::ApiMockRouteOverride {
                source_key: crate::app::api_mock::types::api_mock_source_key(&entry),
                method: route.method,
                path: route.path,
                enabled,
                proxy_when_disabled: false,
                response: crate::app::api_mock::types::ApiMockResponse::Generated,
                python: None,
                extra_input_fields: Vec::new(),
                extra_output_fields: Vec::new(),
            },
        );
    }

    fn active_manual_mock_route(
        &self,
        active: Option<&ApiActiveRoute>,
        route_idx: usize,
    ) -> Option<&crate::app::api_mock::types::ApiManualRoute> {
        let active = active?;
        if active.route_idx != Some(route_idx) {
            return None;
        }
        self.manual_mock_route(active.manual_stable_id.as_deref()?)
    }

    fn active_manual_mock_route_mut(
        &mut self,
        active: Option<&ApiActiveRoute>,
        route_idx: usize,
    ) -> Option<&mut crate::app::api_mock::types::ApiManualRoute> {
        let active = active?;
        if active.route_idx != Some(route_idx) {
            return None;
        }
        self.manual_mock_route_mut(active.manual_stable_id.as_deref()?)
    }

    fn api_route_python_script(
        &self,
        active: Option<&ApiActiveRoute>,
        route_idx: usize,
    ) -> Option<&crate::app::api_mock::types::ApiMockPythonScript> {
        if let Some(route) = self.active_manual_mock_route(active, route_idx) {
            return route.python.as_ref().filter(|script| script.enabled);
        }
        let spec_id = active?.spec_id;
        let entry = self.specs.iter().find(|entry| entry.id == spec_id)?;
        let route = self.models.get(&spec_id)?.routes.get(route_idx)?;
        let source_key = crate::app::api_mock::types::api_mock_source_key(entry);
        self.mock.route_overrides.iter().find_map(|item| {
            (item.source_key == source_key
                && item.method == route.method
                && item.path == route.path)
                .then_some(item.python.as_ref().filter(|script| script.enabled))
                .flatten()
        })
    }

    fn api_route_python_script_mut(
        &mut self,
        active: Option<&ApiActiveRoute>,
        route_idx: usize,
    ) -> Option<&mut crate::app::api_mock::types::ApiMockPythonScript> {
        if self.active_manual_mock_route(active, route_idx).is_some() {
            return self.active_manual_mock_route_mut(active, route_idx)?.python.as_mut()
                .filter(|script| script.enabled);
        }
        let spec_id = active?.spec_id;
        let entry = self.specs.iter().find(|entry| entry.id == spec_id).cloned()?;
        let route = self.models.get(&spec_id)?.routes.get(route_idx).cloned()?;
        let source_key = crate::app::api_mock::types::api_mock_source_key(&entry);
        self.mock.route_overrides.iter_mut().find_map(|item| {
            (item.source_key == source_key
                && item.method == route.method
                && item.path == route.path)
                .then_some(item.python.as_mut().filter(|script| script.enabled))
                .flatten()
        })
    }

    pub(crate) fn api_mock_route_context(
        &self,
        active: Option<&ApiActiveRoute>,
        route_idx: usize,
    ) -> Option<(ApiMethod, String, ApiRouteRow, ApiSpecModel)> {
        if let Some(route) = self.active_manual_mock_route(active, route_idx) {
            let model = api_manual_route_model(route);
            let route = model.routes.first()?.clone();
            return Some((route.method, route.path.clone(), route, model));
        }
        let spec_id = active?.spec_id;
        let model = self.models.get(&spec_id)?.clone();
        let route = model.routes.get(route_idx)?.clone();
        Some((route.method, route.path.clone(), route, model))
    }

    pub(crate) fn api_mock_script_for_tools(
        &self,
        active: Option<&ApiActiveRoute>,
        route_idx: usize,
    ) -> Option<crate::app::api_mock::types::ApiMockPythonScript> {
        let mut script = self.api_route_python_script(active, route_idx)?.clone();
        script.body = api_mock_body_editor_text(&script.body);
        if let Some((focused_route, part)) = self.api_mock_python_focus_target()
            && focused_route == route_idx
        {
            let text = self.input_editor.get_full_text();
            match part {
                ApiMockSourcePart::Contract => {
                    if let Some((_, _, route, model)) = self.api_mock_route_context(active, route_idx) {
                        let base = if script.contract.is_empty() {
                            crate::app::api_mock::types::default_contract_from_route(&route, &model)
                        } else {
                            script.contract.clone()
                        };
                        script.contract =
                            crate::app::api_mock::contract::api_mock_contract_from_state_text(
                                &base, &text,
                            );
                    }
                    script.contract_source = text;
                }
                ApiMockSourcePart::Prelude => script.prelude = text,
                ApiMockSourcePart::Signature => {}
                ApiMockSourcePart::Body => script.body = text,
            }
        }
        Some(script)
    }

    /// Drops the route's mock override; the caller commits the API focus first.
    fn reset_api_route_mock(&mut self, active: Option<&ApiActiveRoute>, route_idx: usize) {
        if self.active_manual_mock_route(active, route_idx).is_some() {
            if let Some(route) = self.active_manual_mock_route_mut(active, route_idx) {
                route.enabled = true;
                route.response = crate::app::api_mock::types::ApiMockResponse::Generated;
                route.python = None;
            }
            if self.api_mock_python_focus_target()
                .is_some_and(|(focused_route, _)| focused_route == route_idx)
            {
                self.focused = None;
                self.input_editor = Editor::new(512);
            }
            self.mock_python_editors
                .retain(|(cached_route, _), _| *cached_route != route_idx);
            self.mock_highlight_cache
                .retain(|(cached_route, _), _| *cached_route != route_idx);
            self.mock_highlight_target = None;
            self.mock_highlight_spans.clear();
            self.mock_ty_diagnostics.clear();
            self.mock_contract_constraint_menu = None;
            self.reset_api_mock_hover_tracking();
            self.commit_mock_config();
            return;
        }
        let Some(spec_id) = active.map(|active| active.spec_id) else {
            return;
        };
        let Some(entry) = self.specs.iter().find(|entry| entry.id == spec_id).cloned() else {
            return;
        };
        let Some(route) = self
            .models
            .get(&spec_id)
            .and_then(|model| model.routes.get(route_idx))
            .cloned()
        else {
            return;
        };
        let source_key = crate::app::api_mock::types::api_mock_source_key(&entry);
        let old_len = self.mock.route_overrides.len();
        self.mock.route_overrides.retain(|item| {
            !(item.source_key == source_key
                && item.method == route.method
                && item.path == route.path)
        });
        if old_len == self.mock.route_overrides.len() {
            return;
        }
        if self.api_mock_python_focus_target()
            .is_some_and(|(focused_route, _)| focused_route == route_idx)
        {
            self.focused = None;
            self.input_editor = Editor::new(512);
        }
        self.mock_python_editors
            .retain(|(cached_route, _), _| *cached_route != route_idx);
        self.mock_highlight_cache
            .retain(|(cached_route, _), _| *cached_route != route_idx);
        self.mock_highlight_target = None;
        self.mock_highlight_spans.clear();
        self.mock_ty_diagnostics.clear();
        self.mock_contract_constraint_menu = None;
        let reset_status = match &self.mock.check_status {
            crate::app::api_mock::types::ApiMockCheckStatus::Pending {
                route_idx: checked, ..
            }
            | crate::app::api_mock::types::ApiMockCheckStatus::Ok {
                route_idx: checked, ..
            }
            | crate::app::api_mock::types::ApiMockCheckStatus::Failed {
                route_idx: checked, ..
            } => *checked == route_idx,
            crate::app::api_mock::types::ApiMockCheckStatus::Idle => false,
        };
        if reset_status {
            self.mock.check_status =
                crate::app::api_mock::types::ApiMockCheckStatus::Idle;
            self.mock_ty_pending = None;
        }
        self.reset_api_mock_hover_tracking();
        self.commit_mock_config();
    }
}

/// Identity of the active API tab that mock route lookups resolve against;
/// built on the App side by `App::api_active_route`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ApiActiveRoute {
    pub spec_id: ApiSpecId,
    pub route_idx: Option<usize>,
    pub manual_stable_id: Option<String>,
}

impl ApiClientState {
    pub fn scroll_api_python_runtime_overlay(
        &mut self,
        dy: f32,
        mx: f32,
        my: f32,
        layout: ApiPythonRuntimeDialogLayout,
        scale: f32,
    ) -> bool {
        if self.mock_python_version_picker_open {
            let rect = api_python_version_list_rect(layout, scale);
            if crate::ui_system::point_in_rect(mx, my, rect) {
                let max_scroll = api_python_version_list_max_scroll(
                    self.mock_python_versions.len(),
                    rect.3,
                    scale,
                );
                self.mock_python_versions_scroll.anim_speed = 7.0;
                self.mock_python_versions_scroll.scroll_by(dy);
                self.mock_python_versions_scroll.clamp_target(0.0, max_scroll);
                return true;
            }
        }
        if api_python_install_log_visible(self) {
            let rect = api_python_install_log_rect(layout, scale);
            if crate::ui_system::point_in_rect(mx, my, rect) {
                let max_scroll = api_python_install_log_max_scroll(
                    self.mock_python_install_log.len(),
                    rect.3,
                    scale,
                );
                self.mock_python_install_log_scroll.anim_speed = 7.0;
                self.mock_python_install_log_scroll.scroll_by(dy);
                self.mock_python_install_log_scroll.clamp_target(0.0, max_scroll);
                return true;
            }
        }
        true
    }

    pub(crate) fn stop_api_mock_server(&mut self) {
        self.mock.server_status =
            crate::app::api_mock::types::ApiMockServerStatus::Stopping;
        push_api_mock_server_log(self, "server stop requested".to_string());
        self.mock.server.stop();
    }

    pub(crate) fn start_api_mock_server(&mut self) {
        let snapshot = self.mock_server_snapshot();
        self.mock.server_status =
            crate::app::api_mock::types::ApiMockServerStatus::Starting;
        push_api_mock_server_log(
            self,
            format!(
                "server start requested {}:{}",
                snapshot.bind_host, snapshot.port
            ),
        );
        if let Err(err) = self.mock.server.start(snapshot) {
            self.mock.server_status =
                crate::app::api_mock::types::ApiMockServerStatus::Failed(err.clone());
            push_api_mock_server_log(self, format!("server start failed: {err}"));
        }
    }

    pub fn api_python_runtime_overlay_active(&self) -> bool {
        self.mock_python_runtime_open
    }
    pub fn api_runtime_poll_pending(&self) -> bool {
        self.python_version_list_rx.is_some()
            || self.python_install_rx.is_some()
            || self.python_path_pick_rx.is_some()
    }
    pub fn ui_id_is_api_python_runtime_overlay(id: crate::ui_system::UiId) -> bool {
        matches!(
            id,
            crate::ui_system::UiId::ApiMockPythonManageClose
                | crate::ui_system::UiId::ApiMockPythonModeToggle
                | crate::ui_system::UiId::ApiMockPythonCheckRuntime
                | crate::ui_system::UiId::ApiMockPythonPrepareVersion
                | crate::ui_system::UiId::ApiMockPythonPickUvPath
                | crate::ui_system::UiId::ApiMockPythonPickCustomPath
                | crate::ui_system::UiId::ApiMockPythonVersionOption(_)
                | crate::ui_system::UiId::ApiMockPythonVersionsScrollY
                | crate::ui_system::UiId::ApiMockPythonInstallLogScrollY
                | crate::ui_system::UiId::ApiMockPythonUvPathInput
                | crate::ui_system::UiId::ApiMockPythonVersionInput
                | crate::ui_system::UiId::ApiMockPythonCustomPathInput
        )
    }
}
