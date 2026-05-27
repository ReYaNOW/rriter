impl crate::app::App {
    pub fn handle_api_client_keyboard_input(&mut self, key_event: &winit::event::KeyEvent) -> bool {
        let ctrl = self.modifiers.control_key() || self.modifiers.super_key();
        if key_event.state == winit::event::ElementState::Pressed
            && ctrl
            && key_event.physical_key
                == winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Digit4)
            && self.active_tab_is_api_client()
        {
            self.close_tab_at(self.active_tab);
            return true;
        }
        if self.ide_panel.api.focused.is_none() {
            return self.active_tab_is_api_client();
        }
        let active = self
            .active_api_tab()
            .map(|(meta, state)| (meta.spec_id, state.route_idx));
        if !self.ide_panel.api.clear_stale_keyboard_focus(active) {
            return false;
        }
        if key_event.state != winit::event::ElementState::Pressed {
            return true;
        }
        let shift = self.modifiers.shift_key();
        if self.api_mock_python_focus_target().is_some() && self.autocomplete_active {
            match key_event.physical_key {
                winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape) => {
                    self.close_autocomplete();
                    return true;
                }
                winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowDown) => {
                    let len = self.autocomplete_options.len();
                    if len > 0 {
                        self.autocomplete_selected_idx = (self.autocomplete_selected_idx + 1) % len;
                        self.ensure_autocomplete_visible();
                    }
                    return true;
                }
                winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowUp) => {
                    let len = self.autocomplete_options.len();
                    if len > 0 {
                        self.autocomplete_selected_idx = if self.autocomplete_selected_idx == 0 {
                            len.saturating_sub(1)
                        } else {
                            self.autocomplete_selected_idx.saturating_sub(1)
                        };
                        self.ensure_autocomplete_visible();
                    }
                    return true;
                }
                winit::keyboard::PhysicalKey::Code(
                    winit::keyboard::KeyCode::Enter
                    | winit::keyboard::KeyCode::NumpadEnter
                    | winit::keyboard::KeyCode::Tab,
                ) => {
                    self.apply_api_mock_autocomplete();
                    return true;
                }
                _ => {}
            }
        }
        let mock_python_target = self.api_mock_python_focus_target();
        let input_version_before = self.ide_panel.api.input_editor.version;
        let mut typed_text: Option<String> = None;
        let is_body = matches!(
            self.ide_panel.api.focused,
            Some(
                ApiFocus::Body { .. }
                    | ApiFocus::MockPrelude { .. }
                    | ApiFocus::MockBody { .. }
                    | ApiFocus::MockStaticResponse { .. }
            )
        );
        let is_signature = matches!(
            self.ide_panel.api.focused,
            Some(ApiFocus::MockSignature { .. })
        );
        let is_response = matches!(self.ide_panel.api.focused, Some(ApiFocus::Response { .. }));
        let is_readonly = is_response || is_signature;
        let is_array = self
            .ide_panel
            .api
            .focused
            .as_ref()
            .is_some_and(|focus| self.api_focus_is_array_input(focus));
        match key_event.physical_key {
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape) => {
                self.commit_api_focus();
                self.ide_panel.api.focused = None;
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Tab) => {
                if mock_python_target.is_some() {
                    let _ = self.ide_panel.api.input_editor.insert_str("    ");
                    typed_text = Some("    ".to_string());
                } else {
                    self.focus_next_api_input(shift);
                }
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Enter)
            | winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::NumpadEnter) => {
                if matches!(self.ide_panel.api.focused, Some(ApiFocus::ImportUrl)) {
                    self.commit_api_focus();
                    self.start_api_url_import_from_input();
                } else if is_array {
                    finish_api_array_editor_draft(&mut self.ide_panel.api.input_editor);
                } else if is_signature {
                } else if mock_python_target.is_some() {
                    let _ = self.ide_panel.api.input_editor.insert_str("\n");
                    typed_text = Some("\n".to_string());
                } else if is_body && shift {
                    let _ = self.ide_panel.api.input_editor.insert_str("\n");
                } else {
                    self.commit_api_focus();
                    self.ide_panel.api.focused = None;
                }
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyA) if ctrl => {
                self.ide_panel.api.input_editor.select_all();
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyW)
                if ctrl && mock_python_target.is_some() =>
            {
                let text = self.ide_panel.api.input_editor.get_full_text();
                if let Some((start, end)) = crate::highlighter::ast_select_expand_range(
                    &text,
                    "py",
                    self.ide_panel.api.input_editor.cursor,
                    self.ide_panel.api.input_editor.selection_anchor,
                ) {
                    self.ide_panel.api.input_editor.selection_anchor = Some(start);
                    self.ide_panel.api.input_editor.cursor = end;
                } else {
                    self.ide_panel.api.input_editor.select_expand();
                }
                self.close_autocomplete();
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyC) if ctrl => {
                if let Some(text) = self.ide_panel.api.input_editor.get_selection() {
                    self.set_clipboard_text(text);
                }
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyX) if ctrl => {
                if !is_readonly && let Some(text) = self.ide_panel.api.input_editor.get_selection()
                {
                    self.set_clipboard_text(text);
                    self.ide_panel.api.input_editor.delete_selection();
                }
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyV) if ctrl => {
                if !is_readonly && let Some(text) = self.get_clipboard_text() {
                    let clean = if is_body {
                        text
                    } else if is_array {
                        text.replace('\r', "")
                    } else {
                        text.replace('\n', "").replace('\r', "")
                    };
                    let _ = self.ide_panel.api.input_editor.insert_str(&clean);
                    typed_text = Some(clean);
                }
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyZ)
                if ctrl && shift && !is_readonly =>
            {
                let _ = self.ide_panel.api.input_editor.redo();
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyZ)
                if ctrl && !is_readonly =>
            {
                let _ = self.ide_panel.api.input_editor.undo();
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyY)
                if ctrl && !is_readonly =>
            {
                let _ = self.ide_panel.api.input_editor.redo();
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Backspace) => {
                if is_readonly {
                } else if is_array && !ctrl {
                    backspace_api_array_editor(&mut self.ide_panel.api.input_editor);
                } else if ctrl {
                    self.ide_panel.api.input_editor.delete_word_backward();
                } else {
                    self.ide_panel.api.input_editor.backspace();
                }
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Delete) => {
                if is_readonly {
                } else if ctrl {
                    self.ide_panel.api.input_editor.delete_word_forward();
                } else {
                    self.ide_panel.api.input_editor.delete_forward();
                }
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowLeft) => {
                if ctrl {
                    self.ide_panel.api.input_editor.move_word_left(shift);
                } else {
                    self.ide_panel.api.input_editor.move_left(shift);
                }
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowRight) => {
                if ctrl {
                    self.ide_panel.api.input_editor.move_word_right(shift);
                } else {
                    self.ide_panel.api.input_editor.move_right(shift);
                }
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowUp)
                if is_body || is_readonly =>
            {
                move_api_input_vertical(&mut self.ide_panel.api.input_editor, false, shift);
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowDown)
                if is_body || is_readonly =>
            {
                move_api_input_vertical(&mut self.ide_panel.api.input_editor, true, shift);
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Home) => {
                self.ide_panel.api.input_editor.move_home(shift);
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::End) => {
                self.ide_panel.api.input_editor.move_end(shift);
            }
            _ if !is_readonly
                && !ctrl
                && !self.modifiers.alt_key()
                && !self.modifiers.super_key() =>
            {
                if let Some(text) = key_event
                    .text
                    .as_ref()
                    .and_then(|s| (!s.is_empty()).then_some(s))
                {
                    let clean = if is_body {
                        text.to_string()
                    } else if is_array {
                        text.replace('\r', "")
                    } else {
                        text.replace('\n', "").replace('\r', "")
                    };
                    let _ = self.ide_panel.api.input_editor.insert_str(&clean);
                    typed_text = Some(clean);
                }
            }
            _ => {}
        }
        if let Some((route_idx, _)) = mock_python_target
            && self.ide_panel.api.input_editor.version != input_version_before
        {
            self.queue_api_mock_python_tools(route_idx);
            if let Some(text) = typed_text.as_deref()
                && (matches!(text, "." | "(" | ",")
                    || text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
            {
                if matches!(text, ".")
                    || self.api_input_after_python_member_dot()
                    || self.api_input_inside_python_call_parens()
                {
                    self.request_api_mock_ty_autocomplete(
                        matches!(text, "." | "(" | ",").then_some(text),
                    );
                } else {
                    self.update_api_mock_tree_sitter_autocomplete();
                }
            } else if self.autocomplete_active {
                if self.autocomplete_mode == crate::app::AutocompleteMode::TreeSitter {
                    self.update_api_mock_tree_sitter_autocomplete();
                } else {
                    self.request_api_mock_ty_autocomplete(None);
                }
            }
        }
        if let Some((id, multiline)) = self
            .ide_panel
            .api
            .focused
            .as_ref()
            .and_then(|focus| self.api_focus_ui_target(focus))
        {
            if multiline {
                self.sync_api_multiline_scroll_target(id, false);
            } else if !is_array {
                self.sync_api_one_line_scroll_target(false);
            }
        }
        self.pulse_api_cursor_blink();
        self.queue_api_body_json_validation();
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
        true
    }

    pub fn start_active_api_request(&mut self) {
        self.commit_api_focus();
        let Some((meta, state)) = self.active_api_tab() else {
            return;
        };
        if state.pending_request_id.is_some() || state.pending {
            return;
        }
        if let Some(ApiClientRouteIdentity::Manual { stable_id }) = &meta.route_identity {
            self.start_active_manual_api_request(stable_id.clone());
            return;
        }
        let spec_id = meta.spec_id;
        let requested_route_idx = state.route_idx;
        let needs_input_sync = requested_route_idx.is_none()
            || (state.path_values.is_empty()
                && state.query_values.is_empty()
                && state.body_values.is_empty()
                && state.body_json == ApiClientTabState::default().body_json);
        let route_idx = {
            let Some(model) = self.ide_panel.api.models.get(&spec_id) else {
                return;
            };
            let route_idx = requested_route_idx.unwrap_or(0);
            if model.routes.get(route_idx).is_none() {
                return;
            }
            route_idx
        };
        if needs_input_sync {
            self.sync_api_tab_inputs(spec_id, route_idx);
            if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
                state.route_idx = Some(route_idx);
            }
        }
        let Some((_, state)) = self.active_api_tab() else {
            return;
        };
        if state.pending_request_id.is_some() || state.pending {
            return;
        }
        let Some(model) = self.ide_panel.api.models.get(&spec_id) else {
            return;
        };
        let Some(route) = model.routes.get(route_idx) else {
            return;
        };
        let Some(server) = model
            .servers
            .get(state.server_idx)
            .or_else(|| model.servers.first())
        else {
            return;
        };
        let method = route.method;
        let path = route.path.clone();
        let is_json_body = route
            .request_body
            .as_ref()
            .is_some_and(|body| !body.is_multipart && !body.is_form_urlencoded);
        let is_multipart_body = route
            .request_body
            .as_ref()
            .is_some_and(|body| body.is_multipart);
        let is_form_body = route
            .request_body
            .as_ref()
            .is_some_and(|body| body.is_form_urlencoded);
        let path_values = state.path_values.clone();
        let query_values = state.query_values.clone();
        let body_values = state.body_values.clone();
        let body_json_text = state.body_json.clone();
        let server = server.clone();
        if route.method.can_send_body() && is_json_body && !json_body_is_valid(&body_json_text) {
            if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
                state.response = Some(ApiJobResponse {
                    request_id: 0,
                    spec_id,
                    route_idx,
                    status: None,
                    elapsed_ms: 0,
                    server_reach_ms: None,
                    timing_text: String::new(),
                    headers: Vec::new(),
                    headers_text: String::new(),
                    body: String::new(),
                    truncated: false,
                    error: Some(ApiLoadError::new(
                        ApiLoadErrorKind::InvalidJson,
                        "JSON body невалиден",
                    )),
                    resolved_host: None,
                });
            }
            return;
        }
        let mut url = match build_request_url(&server, &path, &path_values, &query_values) {
            Ok(url) => url,
            Err(err) => {
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
                    state.response = Some(ApiJobResponse {
                        request_id: 0,
                        spec_id,
                        route_idx,
                        status: None,
                        elapsed_ms: 0,
                        server_reach_ms: None,
                        timing_text: String::new(),
                        headers: Vec::new(),
                        headers_text: String::new(),
                        body: String::new(),
                        truncated: false,
                        error: Some(err),
                        resolved_host: None,
                    });
                }
                return;
            }
        };
        let auth_parts = prepared_auth_for_route(model, route, &self.ide_panel.api.auth);
        append_auth_query(&mut url, &auth_parts);
        let body_multipart = (method.can_send_body() && is_multipart_body)
            .then(|| api_multipart_parts_for_route(route, model, &body_values));
        let body_form = (method.can_send_body() && is_form_body).then_some(body_values);
        let body_json = (method.can_send_body() && is_json_body)
            .then_some(body_json_text)
            .filter(|body| !body.trim().is_empty());
        let request_id = self.ide_panel.api.next_request_id.max(1);
        self.ide_panel.api.next_request_id = request_id.saturating_add(1).max(1);
        let job = ApiJobRequest {
            request_id,
            spec_id,
            route_idx,
            method,
            resolved_host: resolve_api_url_host(&url),
            url,
            auth_parts,
            body_json,
            body_form,
            body_multipart,
        };
        if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
            state.pending = true;
            state.pending_request_id = Some(request_id);
        }
        self.api_request_rx
            .push((request_id, spawn_api_request(job)));
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    fn start_active_manual_api_request(&mut self, stable_id: String) {
        let Some((manual_idx, route)) = self
            .ide_panel
            .api
            .mock
            .manual_routes
            .iter()
            .enumerate()
            .find(|(_, route)| route.stable_id == stable_id)
            .map(|(idx, route)| (idx, route.clone()))
        else {
            return;
        };
        let Some((meta, state)) = self.active_api_tab() else {
            return;
        };
        if state.pending_request_id.is_some() || state.pending {
            return;
        }
        let spec_id = meta.spec_id;
        let server = ApiServer {
            url: api_mock_lan_url(&self.ide_panel.api.mock),
            description: String::new(),
            variables: Vec::new(),
        };
        let url = match build_request_url(&server, &route.path, &[], &[]) {
            Ok(url) => url,
            Err(err) => {
                if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
                    state.route_idx = Some(manual_idx);
                    state.response = Some(ApiJobResponse {
                        request_id: 0,
                        spec_id,
                        route_idx: manual_idx,
                        status: None,
                        elapsed_ms: 0,
                        server_reach_ms: None,
                        timing_text: String::new(),
                        headers: Vec::new(),
                        headers_text: String::new(),
                        body: String::new(),
                        truncated: false,
                        error: Some(err),
                        resolved_host: None,
                    });
                }
                return;
            }
        };
        let request_id = self.ide_panel.api.next_request_id.max(1);
        self.ide_panel.api.next_request_id = request_id.saturating_add(1).max(1);
        let job = ApiJobRequest {
            request_id,
            spec_id,
            route_idx: manual_idx,
            method: route.method,
            resolved_host: resolve_api_url_host(&url),
            url,
            auth_parts: Vec::new(),
            body_json: None,
            body_form: None,
            body_multipart: None,
        };
        if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
            state.route_idx = Some(manual_idx);
            state.pending = true;
            state.pending_request_id = Some(request_id);
        }
        self.api_request_rx
            .push((request_id, spawn_api_request(job)));
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub fn poll_api_client(&mut self) -> bool {
        let mut changed = false;
        let events = drain_api_mock_server_events();
        if !events.is_empty() {
            for event in events {
                match event {
                    ApiMockServerEvent::Log { text } => {
                        push_api_mock_server_log(&mut self.ide_panel.api, text);
                    }
                    ApiMockServerEvent::Request {
                        method,
                        path,
                        status,
                        action,
                    } => {
                        push_api_mock_server_log(
                            &mut self.ide_panel.api,
                            format!("{method} {path} -> {status} · {action}"),
                        );
                    }
                    other => {
                        push_api_mock_server_log(
                            &mut self.ide_panel.api,
                            api_mock_server_event_text(&other),
                        );
                        apply_api_mock_server_event(
                            &mut self.ide_panel.api.mock.server_status,
                            other,
                        );
                    }
                }
            }
            changed = true;
        }
        if self.ensure_active_api_mock_highlight() {
            changed = true;
        }
        if let Some((route_idx, part, version)) = self.ide_panel.api.mock_highlight_target
            && self.ide_panel.api.mock_highlighter.poll(version)
        {
            let spans = self.ide_panel.api.mock_highlighter.spans.clone();
            if let Some((method, path, route, model)) = self.api_mock_route_context(route_idx)
                && let Some(script) = self.api_mock_script_for_tools(route_idx)
            {
                let virtual_source =
                    build_api_mock_virtual_source(method, &path, &route, &model, &script);
                for cache_part in [
                    ApiMockSourcePart::Contract,
                    ApiMockSourcePart::Prelude,
                    ApiMockSourcePart::Signature,
                    ApiMockSourcePart::Body,
                ] {
                    let edit_spans =
                        Self::map_api_mock_spans_to_edit(&spans, &virtual_source, cache_part);
                    self.ide_panel
                        .api
                        .mock_highlight_cache
                        .insert((route_idx, cache_part), edit_spans);
                }
                self.ide_panel.api.mock_highlight_spans = self
                    .ide_panel
                    .api
                    .mock_highlight_cache
                    .get(&(route_idx, part))
                    .cloned()
                    .unwrap_or_default();
                if self.api_mock_completion_focus() == Some((route_idx, part))
                    && self.autocomplete_active
                {
                    self.update_api_mock_tree_sitter_autocomplete();
                }
            }
            changed = true;
        }
        if self.api_mock_ty_rx.is_none()
            && let Some(due) = self.ide_panel.api.mock_ty_due
        {
            if Instant::now() >= due {
                self.ide_panel.api.mock_ty_due = None;
                if let Some((route_idx, _)) = self.api_mock_python_focus_target() {
                    if let Some(version) = self.api_mock_route_tools_version(route_idx) {
                        self.start_api_mock_ty_check_now(route_idx, version);
                    }
                }
            }
            changed = true;
        }
        if let Some(rx) = self.api_mock_ty_rx.take() {
            match rx.try_recv() {
                Ok(result) => {
                    if self.ide_panel.api.mock_ty_pending
                        == Some((result.route_idx, result.version))
                    {
                        self.ide_panel.api.mock_ty_pending = None;
                        self.ide_panel.api.mock_ty_diagnostics = result.diagnostics;
                        self.ide_panel.api.mock.check_status = if result.ok {
                            crate::app::api_mock::types::ApiMockCheckStatus::Ok {
                                route_idx: result.route_idx,
                                version: result.version,
                                message: result.message,
                            }
                        } else {
                            crate::app::api_mock::types::ApiMockCheckStatus::Failed {
                                route_idx: result.route_idx,
                                version: result.version,
                                message: result.message,
                            }
                        };
                        changed = true;
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.api_mock_ty_rx = Some(rx);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.ide_panel.api.mock_ty_diagnostics.clear();
                    self.ide_panel.api.mock.check_status =
                        crate::app::api_mock::types::ApiMockCheckStatus::Failed {
                            route_idx: 0,
                            version: 0,
                            message: "Ty check worker stopped".to_string(),
                        };
                    self.ide_panel.api.mock_ty_pending = None;
                    changed = true;
                }
            }
        }
        if let Some(rx) = self.ide_panel.api.body_json_validation_rx.take() {
            match rx.try_recv() {
                Ok(result) => {
                    if self.ide_panel.api.body_json_validation_pending
                        == Some((result.spec_id, result.route_idx, result.version))
                    {
                        self.ide_panel.api.body_json_validation_pending = None;
                    }
                    self.ide_panel.api.body_json_validation = Some(ApiJsonValidationState {
                        spec_id: result.spec_id,
                        route_idx: result.route_idx,
                        version: result.version,
                        valid: result.valid,
                    });
                    changed = true;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.ide_panel.api.body_json_validation_pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.ide_panel.api.body_json_validation_rx = Some(rx);
                }
            }
        }
        if let Some(rx) = &self.api_import_file_rx {
            if let Ok(result) = rx.try_recv() {
                self.api_import_file_rx = None;
                if let Some(path) = result {
                    self.start_api_local_import(path);
                }
                changed = true;
            }
        }
        if let Some(rx) = &self.api_body_file_rx {
            if let Ok(result) = rx.try_recv() {
                self.api_body_file_rx = None;
                self.apply_api_body_file_pick(result);
                changed = true;
            }
        }
        if let Some(rx) = self.ide_panel.api.python_path_pick_rx.take() {
            match rx.try_recv() {
                Ok(result) => {
                    if let Some(path) = result.path {
                        match result.kind {
                            ApiPythonPathPickKind::Uv => {
                                self.ide_panel.api.mock.uv.configured_path = Some(path);
                                crate::app::api_mock::python_bootstrap::refresh_uv_status(
                                    &mut self.ide_panel.api.mock.uv,
                                );
                            }
                            ApiPythonPathPickKind::CustomPython => {
                                self.ide_panel.api.mock.uv.custom_python_path = Some(path);
                                crate::app::api_mock::python_bootstrap::refresh_python_runtime_status(
                                    &mut self.ide_panel.api.mock.uv,
                                );
                            }
                        }
                        self.ide_panel.api.persist();
                    }
                    changed = true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.ide_panel.api.python_path_pick_rx = Some(rx);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    changed = true;
                }
            }
        }
        if let Some(rx) = self.ide_panel.api.python_version_list_rx.take() {
            match rx.try_recv() {
                Ok(result) => {
                    self.ide_panel.api.mock_python_versions_loading = false;
                    if let Some(error) = result.error {
                        self.ide_panel.api.mock.uv.last_error = error;
                    } else {
                        self.ide_panel.api.mock_python_versions = result.rows;
                        self.ide_panel.api.mock.uv.last_error.clear();
                    }
                    changed = true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.ide_panel.api.python_version_list_rx = Some(rx);
                    changed = true;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.ide_panel.api.mock_python_versions_loading = false;
                    changed = true;
                }
            }
        }
        if let Some(rx) = self.ide_panel.api.python_install_rx.take() {
            let mut keep = true;
            loop {
                match rx.try_recv() {
                    Ok(ApiPythonInstallEvent::Line(line)) => {
                        push_api_python_install_log(&mut self.ide_panel.api, line);
                        changed = true;
                    }
                    Ok(ApiPythonInstallEvent::Done(result)) => {
                        self.ide_panel.api.mock_python_install_running = false;
                        keep = false;
                        match result {
                            Ok(()) => {
                                self.ide_panel.api.mock.uv.status =
                                    crate::app::api_mock::types::ApiPythonRuntimeStatus::Ready;
                                self.ide_panel.api.mock.uv.last_error.clear();
                                push_api_python_install_log(
                                    &mut self.ide_panel.api,
                                    ApiPythonInstallLogLine {
                                        text: "Готово".to_string(),
                                        kind: ApiPythonInstallLogKind::Ok,
                                    },
                                );
                            }
                            Err(err) => {
                                self.ide_panel.api.mock.uv.status =
                                    crate::app::api_mock::types::ApiPythonRuntimeStatus::Invalid;
                                self.ide_panel.api.mock.uv.last_error = err.clone();
                                push_api_python_install_log(
                                    &mut self.ide_panel.api,
                                    ApiPythonInstallLogLine {
                                        text: err,
                                        kind: ApiPythonInstallLogKind::Error,
                                    },
                                );
                            }
                        }
                        self.ide_panel.api.persist();
                        changed = true;
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        self.ide_panel.api.mock_python_install_running = false;
                        keep = false;
                        changed = true;
                        break;
                    }
                }
            }
            if keep && self.ide_panel.api.mock_python_install_running {
                self.ide_panel.api.python_install_rx = Some(rx);
                changed = true;
            }
        }

        let mut idx = 0usize;
        while idx < self.api_load_rx.len() {
            match self.api_load_rx[idx].try_recv() {
                Ok(result) => {
                    self.api_load_rx.remove(idx);
                    match result.result {
                        Ok(payload) => {
                            let id = payload.entry.id;
                            self.ide_panel.api.upsert_loaded(payload);
                            self.update_api_tabs_after_model_load(id);
                        }
                        Err(err) => self.ide_panel.api.mark_load_error(result.id, err),
                    }
                    changed = true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => idx += 1,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.api_load_rx.remove(idx);
                    changed = true;
                }
            }
        }

        let mut idx = 0usize;
        while idx < self.api_request_rx.len() {
            let request_id = self.api_request_rx[idx].0;
            match self.api_request_rx[idx].1.try_recv() {
                Ok(result) => {
                    self.api_request_rx.remove(idx);
                    self.apply_api_job_response(result);
                    changed = true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => idx += 1,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.api_request_rx.remove(idx);
                    self.clear_api_pending_request(request_id);
                    changed = true;
                }
            }
        }
        changed
    }

    fn update_api_tabs_after_model_load(&mut self, id: ApiSpecId) {
        for tab in &mut self.tabs {
            if let crate::app::EditorTabKind::ApiClient(meta, state) = &mut tab.kind
                && meta.spec_id == id
            {
                if let Some(entry) = self.ide_panel.api.specs.iter().find(|entry| entry.id == id) {
                    meta.title = entry.title.clone();
                    tab.base_title = entry.title.clone();
                }
                if let Some(model) = self.ide_panel.api.models.get(&id)
                    && !model.routes.is_empty()
                {
                    let route_idx = state.route_idx.unwrap_or(0).min(model.routes.len() - 1);
                    state.route_idx = Some(route_idx);
                    if state.path_values.is_empty()
                        && state.query_values.is_empty()
                        && state.body_values.is_empty()
                        && state.body_json == ApiClientTabState::default().body_json
                    {
                        fill_api_tab_inputs(state, &model.routes[route_idx], model);
                    }
                }
            }
        }
    }

    fn apply_api_job_response(&mut self, result: ApiJobResponse) {
        let resolved = result.resolved_host.clone();
        let focused_response = matches!(
            self.ide_panel.api.focused,
            Some(ApiFocus::Response { spec_id, route_idx })
                if spec_id == result.spec_id && route_idx == result.route_idx
        );
        let mut focused_text = None;
        let mut applied = false;
        for tab in &mut self.tabs {
            if let crate::app::EditorTabKind::ApiClient(meta, state) = &mut tab.kind
                && meta.spec_id == result.spec_id
            {
                if state.route_idx == Some(result.route_idx)
                    && state.pending_request_id == Some(result.request_id)
                {
                    state.pending = false;
                    state.pending_request_id = None;
                    state.response_scroll.current = 0.0;
                    state.response_scroll.target = 0.0;
                    state.response_scroll_x.current = 0.0;
                    state.response_scroll_x.target = 0.0;
                    if focused_response {
                        focused_text =
                            Some(api_response_text(&result, state.response_view).to_string());
                    }
                    state.response = Some(result.clone());
                    applied = true;
                    break;
                }
                if let Some(saved) = state.route_states.iter_mut().find(|saved| {
                    saved.route_idx == result.route_idx
                        && saved.pending_request_id == Some(result.request_id)
                }) {
                    saved.pending = false;
                    saved.pending_request_id = None;
                    saved.response = Some(result.clone());
                    applied = true;
                    break;
                }
            }
        }
        if !applied {
            return;
        }
        if let Some(resolved) = resolved {
            self.ide_panel.api.last_resolved_host = Some(resolved);
            self.ide_panel.api.persist();
        }
        if let Some(text) = focused_text {
            let old_version = self.ide_panel.api.input_editor.version;
            self.ide_panel.api.input_editor.set_text_clean(&text);
            self.ide_panel.api.input_editor.version = old_version.saturating_add(1);
        }
    }

    fn clear_api_pending_request(&mut self, request_id: u64) {
        for tab in &mut self.tabs {
            if let crate::app::EditorTabKind::ApiClient(_, state) = &mut tab.kind
                && state.pending_request_id == Some(request_id)
            {
                state.pending = false;
                state.pending_request_id = None;
                break;
            }
            if let crate::app::EditorTabKind::ApiClient(_, state) = &mut tab.kind
                && let Some(saved) = state
                    .route_states
                    .iter_mut()
                    .find(|saved| saved.pending_request_id == Some(request_id))
            {
                saved.pending = false;
                saved.pending_request_id = None;
                break;
            }
        }
    }
}
