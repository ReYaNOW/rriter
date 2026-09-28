impl ApiClientState {
    pub(crate) fn handle_json_validation_disconnect(&mut self) {
        self.body_json_validation_pending = None;
        self.body_json_validation = None;
        self.body_json_validation_rx = None;
        self.import_error = Some("Проверка JSON неожиданно завершилась".to_string());
    }

    pub(crate) fn handle_python_path_disconnect(&mut self) {
        self.python_path_pick_rx = None;
        self.mock.uv.last_error = "Окно выбора Python/uv неожиданно завершилось".to_string();
    }

    pub(crate) fn handle_python_versions_disconnect(&mut self) {
        self.python_version_list_rx = None;
        self.mock_python_versions_loading = false;
        self.python_version_list_cancel = None;
        self.mock_python_versions.clear();
        self.mock.uv.last_error = "Загрузка списка Python неожиданно завершилась".to_string();
    }

    pub(crate) fn handle_python_install_disconnect(&mut self) {
        self.python_install_rx = None;
        self.mock_python_install_running = false;
        self.python_install_cancel = None;
        self.mock.uv.status = crate::app::api_mock::types::ApiPythonRuntimeStatus::Invalid;
        let message = "Установка Python неожиданно завершилась".to_string();
        self.mock.uv.last_error = message.clone();
        push_api_python_install_log(
            self,
            ApiPythonInstallLogLine {
                text: message,
                kind: ApiPythonInstallLogKind::Error,
            },
        );
    }

    fn insert_api_client_text(
        &mut self,
        text: &str,
        is_body: bool,
        is_array: bool,
        paired: bool,
    ) -> String {
        let clean = if is_body {
            text.to_string()
        } else if is_array {
            text.replace('\r', "")
        } else {
            text.replace(['\n', '\r'], "")
        };
        if !clean.is_empty() {
            let (insert_text, move_inside_pair) = if paired {
                crate::app::keyboard::paired_editor_insert_text(&clean)
            } else {
                (clean.as_str(), false)
            };
            self.input_editor.insert_str(insert_text);
            if move_inside_pair {
                self.input_editor.move_left(false);
            }
        }
        clean
    }

    fn api_mock_python_vertical_target(
        &self,
        route_idx: usize,
        part: ApiMockSourcePart,
        down: bool,
        shift: bool,
    ) -> Option<ApiFocus> {
        if shift || !api_editor_at_vertical_edge(&self.input_editor, down) {
            return None;
        }
        api_mock_adjacent_python_part(part, down)
            .and_then(|next_part| api_mock_focus_for_part(route_idx, next_part))
    }

    fn poll_body_json_validation(&mut self) -> bool {
        let Some(mut rx) = self.body_json_validation_rx.take() else {
            return false;
        };
        match rx.poll() {
            crate::ui_waker::OneShotState::Ready(result) => {
                if self.body_json_validation_pending
                    == Some((result.spec_id, result.route_idx, result.version))
                {
                    self.body_json_validation_pending = None;
                }
                self.body_json_validation = Some(ApiJsonValidationState {
                    spec_id: result.spec_id,
                    route_idx: result.route_idx,
                    version: result.version,
                    valid: result.valid,
                });
                true
            }
            crate::ui_waker::OneShotState::Closed => {
                self.handle_json_validation_disconnect();
                true
            }
            crate::ui_waker::OneShotState::Pending => {
                self.body_json_validation_rx = Some(rx);
                false
            }
        }
    }

    fn poll_python_path_pick(&mut self) -> bool {
        let Some(mut rx) = self.python_path_pick_rx.take() else {
            return false;
        };
        match rx.poll() {
            crate::ui_waker::OneShotState::Ready(result) => {
                self.apply_python_path_pick(result);
                true
            }
            crate::ui_waker::OneShotState::Pending => {
                self.python_path_pick_rx = Some(rx);
                false
            }
            crate::ui_waker::OneShotState::Closed => {
                self.handle_python_path_disconnect();
                true
            }
        }
    }

    pub(crate) fn apply_python_path_pick(&mut self, result: ApiPythonPathPickResult) {
        if let Some(path) = result.path {
            match result.kind {
                ApiPythonPathPickKind::Uv => {
                    self.mock.uv.configured_path = Some(path);
                    crate::app::api_mock::python_bootstrap::refresh_uv_status(&mut self.mock.uv);
                }
                ApiPythonPathPickKind::CustomPython => {
                    self.mock.uv.custom_python_path = Some(path);
                    crate::app::api_mock::python_bootstrap::refresh_python_runtime_status(
                        &mut self.mock.uv,
                    );
                }
            }
            self.commit_mock_config();
        }
    }

    fn poll_python_version_list(&mut self) -> bool {
        let Some(mut rx) = self.python_version_list_rx.take() else {
            return false;
        };
        match rx.poll() {
            crate::ui_waker::OneShotState::Ready(result) => {
                self.mock_python_versions_loading = false;
                self.python_version_list_cancel = None;
                if let Some(error) = result.error {
                    self.mock.uv.last_error = error;
                } else {
                    self.mock_python_versions = result.rows;
                    self.mock.uv.last_error.clear();
                }
                true
            }
            crate::ui_waker::OneShotState::Pending => {
                self.python_version_list_rx = Some(rx);
                false
            }
            crate::ui_waker::OneShotState::Closed => {
                self.handle_python_versions_disconnect();
                true
            }
        }
    }

    fn poll_python_install(&mut self) -> bool {
        let Some(rx) = self.python_install_rx.take() else {
            return false;
        };
        let mut changed = false;
        let mut keep = true;
        loop {
            match rx.try_recv() {
                Ok(ApiPythonInstallEvent::Line(line)) => {
                    push_api_python_install_log(self, line);
                    changed = true;
                }
                Ok(ApiPythonInstallEvent::Done(result)) => {
                    self.mock_python_install_running = false;
                    self.python_install_cancel = None;
                    keep = false;
                    match result {
                        Ok(()) => {
                            self.mock.uv.status =
                                crate::app::api_mock::types::ApiPythonRuntimeStatus::Ready;
                            self.mock.uv.last_error.clear();
                            push_api_python_install_log(
                                self,
                                ApiPythonInstallLogLine {
                                    text: "Готово".to_string(),
                                    kind: ApiPythonInstallLogKind::Ok,
                                },
                            );
                        }
                        Err(err) => {
                            self.mock.uv.status =
                                crate::app::api_mock::types::ApiPythonRuntimeStatus::Invalid;
                            self.mock.uv.last_error = err.clone();
                            push_api_python_install_log(
                                self,
                                ApiPythonInstallLogLine {
                                    text: err,
                                    kind: ApiPythonInstallLogKind::Error,
                                },
                            );
                        }
                    }
                    self.commit_mock_config();
                    changed = true;
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.handle_python_install_disconnect();
                    keep = false;
                    changed = true;
                    break;
                }
            }
        }
        if keep && self.mock_python_install_running {
            self.python_install_rx = Some(rx);
            changed = true;
        }
        changed
    }

    fn poll_python_receivers(&mut self) -> bool {
        let mut changed = self.poll_python_path_pick();
        changed |= self.poll_python_version_list();
        changed |= self.poll_python_install();
        changed
    }

    fn api_mock_request_wants_server(&self, spec_id: ApiSpecId, route_idx: usize) -> bool {
        let route_override = self.api_route_override(spec_id, route_idx);
        api_mock_route_wants_server(self.mock.mode, route_override)
    }

    fn api_mock_job_target(&self, spec_id: ApiSpecId, route_idx: usize) -> ApiJobMockTarget {
        match self.mock.mode.canonical() {
            crate::app::api_mock::types::ApiMockMode::MockSelectedProxyRest
            | crate::app::api_mock::types::ApiMockMode::MockSelectedOnly => {
                if self
                    .api_route_override(spec_id, route_idx)
                    .is_some_and(|route| route.enabled)
                {
                    ApiJobMockTarget::Mock
                } else {
                    ApiJobMockTarget::Proxy
                }
            }
            crate::app::api_mock::types::ApiMockMode::MockAll => ApiJobMockTarget::Mock,
        }
    }

    fn api_server_proxy_base_url(server: &ApiServer) -> String {
        let mut server_url = server.url.clone();
        for var in &server.variables {
            let needle = format!("{{{}}}", var.name);
            server_url = server_url.replace(&needle, &var.default_value);
        }
        if server_url == "/" {
            server_url = "http://localhost".to_string();
        }
        server_url.trim_end_matches('/').to_string()
    }

    fn sync_api_mock_proxy_base_to_server(&mut self, server: &ApiServer) -> bool {
        let proxy_base_url = Self::api_server_proxy_base_url(server);
        if proxy_base_url.is_empty() || self.mock.proxy_base_url == proxy_base_url {
            return false;
        }
        self.mock.sync_proxy_base_url(proxy_base_url);
        self.commit_mock_config();
        true
    }

    fn build_api_job_request(
        &self,
        spec_id: ApiSpecId,
        route_idx: usize,
        state: &ApiClientTabState,
        request_id: u64,
        use_mock_server: bool,
    ) -> Result<ApiJobRequest, ApiLoadError> {
        let model = self.models.get(&spec_id).ok_or_else(|| {
            ApiLoadError::new(ApiLoadErrorKind::Other, "API model не найден")
        })?;
        let route = model.routes.get(route_idx).ok_or_else(|| {
            ApiLoadError::new(ApiLoadErrorKind::Other, "API route не найден")
        })?;
        let selected_server = model
            .servers
            .get(state.server_idx)
            .or_else(|| model.servers.first())
            .ok_or_else(|| ApiLoadError::new(ApiLoadErrorKind::Other, "API server не найден"))?;
        let server = if use_mock_server {
            ApiServer {
                url: api_mock_lan_url(&self.mock),
                description: String::new(),
                variables: Vec::new(),
            }
        } else {
            selected_server.clone()
        };
        let method = route.method;
        let path_values = state.path_values.clone();
        let query_values = state.query_values.clone();
        let body_values = state.body_values.clone();
        let body_file_paths = state.body_file_paths.clone();
        let body_json_text = state.body_json.clone();
        let body_content_type = route
            .request_body
            .as_ref()
            .filter(|body| !body.is_multipart && !body.is_form_urlencoded)
            .map(|body| body.content_type.trim().to_string())
            .filter(|content_type| !content_type.is_empty());
        let is_json_body = body_content_type
            .as_deref()
            .is_some_and(api_content_type_is_json);
        let is_multipart_body = route
            .request_body
            .as_ref()
            .is_some_and(|body| body.is_multipart);
        let is_form_body = route
            .request_body
            .as_ref()
            .is_some_and(|body| body.is_form_urlencoded);
        let auth_parts = prepared_auth_for_route(model, route, &self.auth);
        let proxy_url_for_reach = if use_mock_server {
            build_request_url(selected_server, &route.path, &path_values, &query_values)
                .ok()
                .map(|mut url| {
                    append_auth_query(&mut url, &auth_parts);
                    url
                })
        } else {
            None
        };
        let body_multipart = (method.can_send_body() && is_multipart_body)
            .then(|| api_multipart_parts_for_route(route, model, &body_values, &body_file_paths));
        let body_form = (method.can_send_body() && is_form_body).then_some(body_values);
        let body_json = (method.can_send_body() && body_content_type.is_some())
            .then_some(body_json_text.clone())
            .filter(|body| !body.trim().is_empty());
        if method.can_send_body() && is_json_body && !json_body_is_valid(&body_json_text) {
            return Err(ApiLoadError::new(
                ApiLoadErrorKind::InvalidJson,
                "JSON body невалиден",
            ));
        }
        let mut url = build_request_url(&server, &route.path, &path_values, &query_values)?;
        append_auth_query(&mut url, &auth_parts);
        Ok(ApiJobRequest {
            request_id,
            spec_id,
            route_idx,
            method,
            resolved_host: proxy_url_for_reach
                .as_ref()
                .and_then(|url| resolve_api_url_host(url))
                .or_else(|| resolve_api_url_host(&url)),
            url,
            mock_target: if use_mock_server {
                self.api_mock_job_target(spec_id, route_idx)
            } else {
                ApiJobMockTarget::None
            },
            auth_parts,
            body_content_type: method.can_send_body().then_some(body_content_type).flatten(),
            body_json,
            body_form,
            body_multipart,
        })
    }

    fn build_manual_api_job_request(
        &self,
        spec_id: ApiSpecId,
        route_idx: usize,
        route: &crate::app::api_mock::types::ApiManualRoute,
        path_values: &[ApiInputValue],
        query_values: &[ApiInputValue],
    ) -> Result<ApiJobRequest, ApiLoadError> {
        let server = ApiServer {
            url: api_mock_lan_url(&self.mock),
            description: String::new(),
            variables: Vec::new(),
        };
        let url = build_manual_api_request_url(&server, route, path_values, query_values)?;
        Ok(ApiJobRequest {
            request_id: 0,
            spec_id,
            route_idx,
            method: route.method,
            resolved_host: resolve_api_url_host(&url),
            url,
            mock_target: ApiJobMockTarget::Mock,
            auth_parts: Vec::new(),
            body_content_type: None,
            body_json: None,
            body_form: None,
            body_multipart: None,
        })
    }

    fn apply_api_job_response_to_tab(
        &self,
        spec_id: ApiSpecId,
        state: &mut ApiClientTabState,
        result: &ApiJobResponse,
    ) -> (bool, Option<String>) {
        if state.pending_request_id == Some(result.request_id) {
            state.pending = false;
            state.pending_request_id = None;
            state.response_scroll.reset();
            state.response_scroll_x.reset();
            let focused_text = state
                .route_idx
                .filter(|&route_idx| {
                    self.focused == Some(ApiFocus::Response { spec_id, route_idx })
                })
                .map(|_| api_response_text(result, state.response_view).to_string());
            state.response = Some(result.clone());
            return (true, focused_text);
        }
        if let Some(saved) = state
            .route_states
            .iter_mut()
            .find(|saved| saved.pending_request_id == Some(result.request_id))
        {
            saved.pending = false;
            saved.pending_request_id = None;
            saved.response = Some(result.clone());
            return (true, None);
        }
        (false, None)
    }
}

impl ApiClientTabState {
    fn update_after_model_load(
        &mut self,
        meta: &mut ApiClientTabMeta,
        previous_routes: &[(ApiMethod, String)],
        model: &ApiSpecModel,
    ) {
        self.remap_route_memories(previous_routes, model);
        if model.routes.is_empty() {
            self.reset_route_content(None);
            self.tab_scroll.reset();
            meta.route_identity = None;
            meta.route_method = None;
            meta.route_path.clear();
            return;
        }

        let previous_identity = meta
            .route_method
            .map(|method| (method, meta.route_path.as_str()))
            .filter(|(_, path)| !path.is_empty())
            .or_else(|| {
                self.route_idx
                    .and_then(|route_idx| previous_routes.get(route_idx))
                    .map(|(method, path)| (*method, path.as_str()))
            });
        let remapped_route_idx = previous_identity
            .and_then(|(method, path)| api_route_index_by_identity(model, method, path));
        let route_idx = remapped_route_idx.unwrap_or(0);
        if remapped_route_idx.is_some() {
            self.route_idx = Some(route_idx);
        } else {
            self.reset_route_content(Some(route_idx));
            fill_api_tab_inputs(self, &model.routes[route_idx], model);
        }

        if !self.auth_view {
            let route = &model.routes[route_idx];
            meta.route_identity = Some(ApiClientRouteIdentity::OpenApi {
                spec_id: meta.spec_id,
                route_idx,
            });
            meta.route_method = Some(route.method);
            meta.route_path = route.path.clone();
        }
    }

    fn apply_request_disconnect(&mut self, spec_id: ApiSpecId, request_id: u64) -> bool {
        if self.pending_request_id == Some(request_id) {
            let route_idx = self.route_idx.unwrap_or(0);
            self.pending = false;
            self.pending_request_id = None;
            self.response_scroll.reset();
            self.response_scroll_x.reset();
            self.response = Some(api_request_disconnect_response(request_id, spec_id, route_idx));
            return true;
        }
        let Some(saved) = self
            .route_states
            .iter_mut()
            .find(|saved| saved.pending_request_id == Some(request_id))
        else {
            return false;
        };
        saved.pending = false;
        saved.pending_request_id = None;
        saved.response = Some(api_request_disconnect_response(
            request_id,
            spec_id,
            saved.route_idx,
        ));
        true
    }

    fn remap_route_memories(
        &mut self,
        previous_routes: &[(ApiMethod, String)],
        model: &ApiSpecModel,
    ) {
        self.route_states = self
            .route_states
            .drain(..)
            .filter_map(|mut saved| {
                let (method, path) = previous_routes.get(saved.route_idx)?;
                saved.route_idx = api_route_index_by_identity(model, *method, path)?;
                Some(saved)
            })
            .collect();
        self.view_scrolls = self
            .view_scrolls
            .drain(..)
            .filter_map(|mut saved| {
                if let Some(route_idx) = saved.route_idx {
                    let (method, path) = previous_routes.get(route_idx)?;
                    saved.route_idx = Some(api_route_index_by_identity(model, *method, path)?);
                }
                Some(saved)
            })
            .collect();
    }

    fn mark_request_pending(&mut self, request_id: u64) {
        self.pending = true;
        self.pending_request_id = Some(request_id);
    }
}

impl crate::app::api_mock::types::ApiMockState {
    fn api_mock_server_running(&self) -> bool {
        matches!(
            self.server_status,
            crate::app::api_mock::types::ApiMockServerStatus::Running { .. }
        )
    }

    fn sync_proxy_base_url(&mut self, proxy_base_url: String) {
        self.proxy_base_url = proxy_base_url;
    }
}
