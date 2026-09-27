impl ApiClientState {
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
