pub(crate) fn api_request_disconnect_message(request_id: u64) -> String {
    format!("HTTP-запрос #{request_id} неожиданно завершился")
}

fn api_request_disconnect_response(
    request_id: u64,
    spec_id: ApiSpecId,
    route_idx: usize,
) -> ApiJobResponse {
    ApiJobResponse {
        request_id,
        spec_id,
        route_idx,
        status: None,
        elapsed_ms: 0,
        server_reach_ms: None,
        timing_text: String::new(),
        headers: Vec::new(),
        headers_text: String::new(),
        curl_text: String::new(),
        body: String::new(),
        truncated: false,
        error: Some(ApiLoadError::new(
            ApiLoadErrorKind::Io,
            api_request_disconnect_message(request_id),
        )),
        resolved_host: None,
    }
}

fn build_manual_api_request_url(
    server: &ApiServer,
    route: &crate::app::api_mock::types::ApiManualRoute,
    path_values: &[ApiInputValue],
    query_values: &[ApiInputValue],
) -> Result<String, ApiLoadError> {
    build_request_url(server, &route.path, path_values, query_values)
}

fn api_mock_ty_disconnect_status(
    pending: Option<(usize, u64)>,
) -> Option<crate::app::api_mock::types::ApiMockCheckStatus> {
    let (route_idx, version) = pending?;
    Some(crate::app::api_mock::types::ApiMockCheckStatus::Failed {
        route_idx,
        version,
        message: "Ty check worker stopped".to_string(),
    })
}

fn api_editor_at_vertical_edge(editor: &Editor, down: bool) -> bool {
    let line_idx = editor
        .line_offsets
        .partition_point(|&offset| offset <= editor.cursor)
        .saturating_sub(1);
    if down {
        line_idx >= editor.line_offsets.len().saturating_sub(1)
    } else {
        line_idx == 0
    }
}

fn api_mock_adjacent_python_part(part: ApiMockSourcePart, down: bool) -> Option<ApiMockSourcePart> {
    match (part, down) {
        (ApiMockSourcePart::Prelude, true) => Some(ApiMockSourcePart::Contract),
        (ApiMockSourcePart::Contract, true) => Some(ApiMockSourcePart::Body),
        (ApiMockSourcePart::Body, false) => Some(ApiMockSourcePart::Contract),
        (ApiMockSourcePart::Contract, false) => Some(ApiMockSourcePart::Prelude),
        _ => None,
    }
}

fn api_mock_focus_for_part(route_idx: usize, part: ApiMockSourcePart) -> Option<ApiFocus> {
    match part {
        ApiMockSourcePart::Contract => Some(ApiFocus::MockContract { route_idx }),
        ApiMockSourcePart::Prelude => Some(ApiFocus::MockPrelude { route_idx }),
        ApiMockSourcePart::Body => Some(ApiFocus::MockBody { route_idx }),
        ApiMockSourcePart::Signature => None,
    }
}

fn api_mock_alt_enter_route_target(
    mock_python_target: Option<(usize, ApiMockSourcePart)>,
    alt: bool,
    is_enter: bool,
) -> Option<usize> {
    if alt && is_enter {
        mock_python_target.map(|(route_idx, _)| route_idx)
    } else {
        None
    }
}

fn api_mock_tools_queue_route_after_key(
    before: Option<(usize, ApiMockSourcePart)>,
    after: Option<(usize, ApiMockSourcePart)>,
    version_before: u64,
    version_after: u64,
) -> Option<usize> {
    if before == after && version_before != version_after {
        before.map(|(route_idx, _)| route_idx)
    } else {
        None
    }
}

fn api_mock_request_requires_stopped_server(
    mode: crate::app::api_mock::types::ApiMockMode,
    route: Option<&crate::app::api_mock::types::ApiMockRouteOverride>,
) -> bool {
    api_mock_route_wants_server(mode, route)
}

fn api_mock_route_wants_server(
    mode: crate::app::api_mock::types::ApiMockMode,
    route: Option<&crate::app::api_mock::types::ApiMockRouteOverride>,
) -> bool {
    match mode.canonical() {
        crate::app::api_mock::types::ApiMockMode::MockAll => true,
        crate::app::api_mock::types::ApiMockMode::MockSelectedProxyRest
        | crate::app::api_mock::types::ApiMockMode::MockSelectedOnly => {
            route.is_some_and(|route| route.enabled)
        }
    }
}

fn api_route_index_by_identity(
    model: &ApiSpecModel,
    method: ApiMethod,
    path: &str,
) -> Option<usize> {
    model
        .routes
        .iter()
        .position(|route| route.method == method && route.path == path)
}

impl crate::app::App {
    fn allocate_api_request_id(&mut self) -> u64 {
        let mut candidate = self.ide_panel.api.next_request_id.max(1);
        loop {
            let active_receiver = self
                .api_request_rx
                .iter()
                .any(|(request_id, _)| *request_id == candidate);
            let active_tab = self.tabs.iter().any(|tab| match &tab.kind {
                crate::app::EditorTabKind::ApiClient(_, state) => {
                    state.pending_request_id == Some(candidate)
                }
                _ => false,
            });
            let next = candidate.wrapping_add(1).max(1);
            if !active_receiver && !active_tab {
                self.ide_panel.api.next_request_id = next;
                return candidate;
            }
            candidate = next;
        }
    }

    pub(crate) fn sync_api_mock_proxy_base_to_active_server(&mut self) -> bool {
        let Some((meta, state)) = self.active_api_tab() else {
            return false;
        };
        let selected_server = self
            .ide_panel
            .api
            .models
            .get(&meta.spec_id)
            .and_then(|model| {
                model
                    .servers
                    .get(state.server_idx)
                    .or_else(|| model.servers.first())
            })
            .cloned();
        selected_server
            .as_ref()
            .is_some_and(|server| self.ide_panel.api.sync_api_mock_proxy_base_to_server(server))
    }

    pub(crate) fn copy_hover_popup_selection_or_diagnostic(&mut self) -> bool {
        let mouse = self
            .renderer
            .as_ref()
            .map(|renderer| (renderer.last_mouse_x, renderer.last_mouse_y));
        let mut copied_text: Option<String> = None;
        crate::app::mouse::HOVER_STATE.with(|state| {
            let mut state = state.borrow_mut();
            if let (Some(popup), Some(a), Some(b)) = (
                state.popup.as_ref(),
                state.selection_anchor,
                state.selection_cursor,
            ) {
                let start = a.min(b);
                let end = a.max(b);
                if start < end
                    && end <= popup.text.len()
                    && popup.text.is_char_boundary(start)
                    && popup.text.is_char_boundary(end)
                {
                    copied_text = Some(popup.text[start..end].to_string());
                    state.selection_anchor = None;
                    state.selection_cursor = None;
                    state.selecting = false;
                }
            }
            if copied_text.is_none()
                && let (Some(a), Some(b)) =
                    (state.diag_selection_anchor, state.diag_selection_cursor)
            {
                let start = a.min(b);
                let end = a.max(b);
                if start < end
                    && end <= state.diag_text.len()
                    && state.diag_text.is_char_boundary(start)
                    && state.diag_text.is_char_boundary(end)
                {
                    copied_text = Some(state.diag_text[start..end].to_string());
                    state.diag_selection_anchor = None;
                    state.diag_selection_cursor = None;
                    state.diag_selecting = false;
                }
            }
            if copied_text.is_none()
                && let (Some((mx, my)), Some((rx, ry, rw, rh, _, _, _))) = (mouse, state.diag_rect)
                && mx >= rx
                && mx <= rx + rw
                && my >= ry
                && my <= ry + rh
                && !state.diag_text.is_empty()
            {
                copied_text = Some(state.diag_text.clone());
            }
        });
        if let Some(text) = copied_text {
            self.set_clipboard_text(text);
            true
        } else {
            false
        }
    }

    fn finish_api_text_edit(
        &mut self,
        input_version_before: u64,
        mock_python_target: Option<(usize, ApiMockSourcePart)>,
        typed_text: Option<&str>,
        is_array: bool,
    ) {
        if let Some(route_idx) = api_mock_tools_queue_route_after_key(
            mock_python_target,
            self.ide_panel.api.api_mock_python_focus_target(),
            input_version_before,
            self.ide_panel.api.input_editor.version,
        ) {
            self.queue_api_mock_python_tools(route_idx);
            if let Some(text) = typed_text
                && (matches!(text, "." | "(" | ",")
                    || text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
            {
                let mock_source = self.active_api_mock_autocomplete_source();
                if matches!(text, ".")
                    || mock_source.is_some_and(|source| {
                        self.source_after_python_member_dot(source)
                            || self.source_inside_python_call_parens(source)
                    })
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
        if matches!(
            self.ide_panel.api.focused,
            Some(ApiFocus::RouteFilter | ApiFocus::MockContractField { .. })
        ) && self.ide_panel.api.input_editor.version != input_version_before
        {
            self.commit_api_focus();
        }
        if let Some((id, multiline)) = self
            .ide_panel
            .api
            .focused
            .as_ref()
            .and_then(|focus| self.ide_panel.api.api_focus_ui_target(focus))
        {
            if multiline {
                self.sync_api_multiline_scroll_target(id, false);
            } else if !is_array {
                self.sync_api_one_line_scroll_target(false);
            }
        }
        self.pulse_api_cursor_blink();
        self.ide_panel.api.queue_api_body_json_validation(&self.ui_waker);
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    fn api_client_keyboard_surface_visible(&self) -> bool {
        self.active_tab_is_api_client()
            || self.ide_panel.is_open(crate::app::PanelId::ApiClient)
    }

    pub fn handle_api_client_ime_commit(&mut self, text: &str) -> bool {
        if !self.api_client_keyboard_surface_visible() {
            self.ide_panel.api.focused = None;
            return false;
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

        let mock_python_target = self.ide_panel.api.api_mock_python_focus_target();
        let is_body = matches!(
            self.ide_panel.api.focused,
            Some(
                ApiFocus::Body { .. }
                    | ApiFocus::MockContract { .. }
                    | ApiFocus::MockPrelude { .. }
                    | ApiFocus::MockBody { .. }
                    | ApiFocus::MockStaticResponse { .. }
            )
        );
        let is_readonly = matches!(
            self.ide_panel.api.focused,
            Some(
                ApiFocus::Response { .. }
                    | ApiFocus::InputSchema { .. }
                    | ApiFocus::OutputSchema { .. }
                    | ApiFocus::MockSignature { .. }
            )
        );
        if is_readonly {
            return true;
        }
        let is_array = self
            .ide_panel
            .api
            .focused
            .as_ref()
            .is_some_and(|focus| self.ide_panel.api.api_focus_is_array_input(focus));
        let input_version_before = self.ide_panel.api.input_editor.version;
        let clean = self.ide_panel.api.insert_api_client_text(
            text,
            is_body,
            is_array,
            mock_python_target.is_some(),
        );
        if clean.is_empty() {
            return true;
        }

        self.finish_api_text_edit(
            input_version_before,
            mock_python_target,
            Some(&clean),
            is_array,
        );
        true
    }

    pub fn handle_api_client_keyboard_input(&mut self, key_event: &crate::app::keyboard::KeyInput) -> bool {
        if !self.api_client_keyboard_surface_visible() {
            self.ide_panel.api.focused = None;
            return false;
        }
        let ctrl = crate::platform::primary_shortcut_modifier(self.modifiers);
        let word = crate::platform::word_navigation_modifier(self.modifiers);
        if key_event.state == winit::event::ElementState::Pressed
            && ctrl
            && key_event.physical_key
                == winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyC)
            && self.active_tab_is_api_client()
            && (self.copy_api_route_text_selection()
                || self.copy_hover_popup_selection_or_diagnostic())
        {
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            return true;
        }
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
            if key_event.state == winit::event::ElementState::Pressed
                && key_event.physical_key
                    == winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::F2)
            {
                return false;
            }
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
        let mock_python_target = self.ide_panel.api.api_mock_python_focus_target();
        let is_enter_key = matches!(
            key_event.physical_key,
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Enter)
                | winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::NumpadEnter)
        );
        if let Some(route_idx) = api_mock_alt_enter_route_target(
            mock_python_target,
            self.modifiers.alt_key(),
            is_enter_key,
        ) {
            self.start_api_mock_route_tools_now(route_idx);
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            return true;
        }
        if mock_python_target.is_some() && self.autocomplete_active {
            match self.handle_active_autocomplete_key(key_event.physical_key, ctrl) {
                crate::app::AutocompletePopupKeyResult::Consumed => return true,
                crate::app::AutocompletePopupKeyResult::Continue
                | crate::app::AutocompletePopupKeyResult::NotHandled => {}
            }
        }
        if mock_python_target.is_some()
            && self.mark_pending_autocomplete_apply_for_key(key_event.physical_key)
        {
            return true;
        }
        let input_version_before = self.ide_panel.api.input_editor.version;
        let mut typed_text: Option<String> = None;
        let is_body = matches!(
            self.ide_panel.api.focused,
            Some(
                ApiFocus::Body { .. }
                    | ApiFocus::MockContract { .. }
                    | ApiFocus::MockPrelude { .. }
                    | ApiFocus::MockBody { .. }
                    | ApiFocus::MockStaticResponse { .. }
            )
        );
        let is_signature = matches!(
            self.ide_panel.api.focused,
            Some(ApiFocus::MockSignature { .. })
        );
        let is_response = matches!(
            self.ide_panel.api.focused,
            Some(
                ApiFocus::Response { .. }
                    | ApiFocus::InputSchema { .. }
                    | ApiFocus::OutputSchema { .. }
            )
        );
        let is_readonly = is_response || is_signature;
        let is_array = self
            .ide_panel
            .api
            .focused
            .as_ref()
            .is_some_and(|focus| self.ide_panel.api.api_focus_is_array_input(focus));
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
                    let clean = self.ide_panel.api.insert_api_client_text(
                        &text,
                        is_body,
                        is_array,
                        false,
                    );
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
                } else if is_array && !word {
                    backspace_api_array_editor(&mut self.ide_panel.api.input_editor);
                } else if !word
                    && matches!(mock_python_target, Some((_, ApiMockSourcePart::Body)))
                {
                    backspace_api_mock_body_editor(&mut self.ide_panel.api.input_editor);
                } else if !word
                    && self.ide_panel.api.input_editor.cursor == 0
                    && self
                        .ide_panel
                        .api
                        .input_editor
                        .selection_anchor
                        .is_none_or(|anchor| anchor == 0)
                    && let Some((route_idx, part)) = mock_python_target
                    && self.focus_previous_api_mock_python_part(route_idx, part)
                {
                } else if word {
                    self.ide_panel.api.input_editor.delete_word_backward();
                } else {
                    self.ide_panel.api.input_editor.backspace();
                }
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Delete) => {
                if is_readonly {
                } else if word {
                    self.ide_panel.api.input_editor.delete_word_forward();
                } else {
                    self.ide_panel.api.input_editor.delete_forward();
                }
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowLeft) => {
                if word {
                    self.ide_panel.api.input_editor.move_word_left(shift);
                } else {
                    self.ide_panel.api.input_editor.move_left(shift);
                }
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowRight) => {
                if word {
                    self.ide_panel.api.input_editor.move_word_right(shift);
                } else {
                    self.ide_panel.api.input_editor.move_right(shift);
                }
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowUp)
                if mock_python_target.is_some() =>
            {
                let jumped = mock_python_target.is_some_and(|(route_idx, part)| {
                    self.move_api_mock_python_vertical_or_focus(route_idx, part, false, shift)
                });
                if !jumped {
                    move_api_input_vertical(&mut self.ide_panel.api.input_editor, false, shift);
                }
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowDown)
                if mock_python_target.is_some() =>
            {
                let jumped = mock_python_target.is_some_and(|(route_idx, part)| {
                    self.move_api_mock_python_vertical_or_focus(route_idx, part, true, shift)
                });
                if !jumped {
                    move_api_input_vertical(&mut self.ide_panel.api.input_editor, true, shift);
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
                && crate::platform::text_input_modifiers_allowed(self.modifiers) =>
            {
                if let Some(text) = key_event
                    .text
                    .as_ref()
                    .and_then(|s| (!s.is_empty()).then_some(s))
                {
                    let clean = self.ide_panel.api.insert_api_client_text(
                        text,
                        is_body,
                        is_array,
                        mock_python_target.is_some(),
                    );
                    typed_text = Some(clean);
                }
            }
            _ => {}
        }
        self.finish_api_text_edit(
            input_version_before,
            mock_python_target,
            typed_text.as_deref(),
            is_array,
        );
        true
    }

    fn move_api_mock_python_vertical_or_focus(
        &mut self,
        route_idx: usize,
        part: ApiMockSourcePart,
        down: bool,
        shift: bool,
    ) -> bool {
        let Some(next_focus) = self
            .ide_panel
            .api
            .api_mock_python_vertical_target(route_idx, part, down, shift)
        else {
            return false;
        };
        self.focus_api_input(next_focus);
        if down {
            self.ide_panel.api.input_editor.cursor = 0;
        } else {
            self.ide_panel.api.input_editor.cursor = self.ide_panel.api.input_editor.len();
        }
        self.ide_panel.api.input_editor.selection_anchor = None;
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
        let Some(selected_server) = self
            .ide_panel
            .api
            .models
            .get(&spec_id)
            .and_then(|model| {
                model
                    .servers
                    .get(state.server_idx)
                    .or_else(|| model.servers.first())
                    .cloned()
            })
        else {
            return;
        };
        let mock_server_running = self.ide_panel.api.mock.api_mock_server_running();
        let wants_mock_server = self
            .ide_panel
            .api
            .api_mock_request_wants_server(spec_id, route_idx);
        let use_mock_server = wants_mock_server && mock_server_running;
        if wants_mock_server && !use_mock_server {
            if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
                state.response = Some(api_request_disconnect_response(0, spec_id, route_idx));
                if let Some(response) = &mut state.response {
                    response.error = Some(ApiLoadError::new(
                        ApiLoadErrorKind::Other,
                        "Мок-сервер не запущен",
                    ));
                }
            }
            return;
        }
        if matches!(
            self.ide_panel.api.mock.server_status,
            crate::app::api_mock::types::ApiMockServerStatus::Running { .. }
        ) {
            self.ide_panel
                .api
                .sync_api_mock_proxy_base_to_server(&selected_server);
            self.ide_panel.api.refresh_mock_server();
        }
        // Re-borrow the tab after the mock-config commit above; tabs are unchanged.
        let Some((_, state)) = self.active_api_tab() else {
            return;
        };
        let mut job = match self
            .ide_panel
            .api
            .build_api_job_request(spec_id, route_idx, state, 0, use_mock_server)
        {
            Ok(job) => job,
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
                        curl_text: String::new(),
                        body: String::new(),
                        truncated: false,
                        error: Some(err),
                        resolved_host: None,
                    });
                }
                return;
            }
        };
        let request_id = self.allocate_api_request_id();
        job.request_id = request_id;
        if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
            state.mark_request_pending(request_id);
        }
        match spawn_api_request(job, &self.ui_waker) {
            Ok(rx) => self.api_request_rx.push((request_id, rx)),
            Err(response) => self.apply_api_job_response(response),
        }
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
        let (spec_id, path_values, query_values) = {
            let Some((meta, state)) = self.active_api_tab() else {
                return;
            };
            if state.pending_request_id.is_some() || state.pending {
                return;
            }
            (
                meta.spec_id,
                state.path_values.clone(),
                state.query_values.clone(),
            )
        };
        if !self.ide_panel.api.mock.api_mock_server_running() {
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
                    curl_text: String::new(),
                    body: String::new(),
                    truncated: false,
                    error: Some(ApiLoadError::new(
                        ApiLoadErrorKind::Other,
                        "Мок-сервер не запущен",
                    )),
                    resolved_host: None,
                });
            }
            return;
        }
        self.ide_panel.api.refresh_mock_server();
        let mut job = match self.ide_panel.api.build_manual_api_job_request(
            spec_id,
            manual_idx,
            &route,
            &path_values,
            &query_values,
        ) {
            Ok(job) => job,
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
                        curl_text: String::new(),
                        body: String::new(),
                        truncated: false,
                        error: Some(err),
                        resolved_host: None,
                    });
                }
                return;
            }
        };
        let request_id = self.allocate_api_request_id();
        job.request_id = request_id;
        if let Some((_, state)) = self.active_api_tab_mut_for(spec_id) {
            state.route_idx = Some(manual_idx);
            state.mark_request_pending(request_id);
        }
        match spawn_api_request(job, &self.ui_waker) {
            Ok(rx) => self.api_request_rx.push((request_id, rx)),
            Err(response) => self.apply_api_job_response(response),
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub fn poll_api_client(&mut self) -> bool {
        let mut changed = false;
        match poll_api_one_shot(&mut self.api_openapi_export_rx) {
            crate::platform::ReceiverPoll::Item(Ok(_)) => changed = true,
            crate::platform::ReceiverPoll::Item(Err(error)) => {
                self.ide_panel.api.persistence_error = Some(error);
                changed = true;
            }
            crate::platform::ReceiverPoll::Empty => {}
            crate::platform::ReceiverPoll::Disconnected => {
                self.ide_panel.api.persistence_error =
                    Some("Экспорт OpenAPI неожиданно завершился".to_string());
                changed = true;
            }
        }
        let events = self.ide_panel.api.mock.server.drain_events();
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
                self.ide_panel.api.refresh_api_mock_highlight_cache_for_spans(route_idx, &spans, &virtual_source);
                self.ide_panel.api.mock_highlight_spans = self
                    .ide_panel
                    .api
                    .mock_highlight_cache
                    .get(&(route_idx, part))
                    .cloned()
                    .unwrap_or_default();
                if self.ide_panel.api.api_mock_completion_focus() == Some((route_idx, part))
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
                if let Some((route_idx, _)) = self.ide_panel.api.api_mock_python_focus_target() {
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
                    if let Some(status) =
                        api_mock_ty_disconnect_status(self.ide_panel.api.mock_ty_pending.take())
                    {
                        self.ide_panel.api.mock.check_status = status;
                        changed = true;
                    }
                }
            }
        }
        changed |= self.ide_panel.api.poll_body_json_validation();
        match poll_api_one_shot(&mut self.api_import_file_rx) {
            crate::platform::ReceiverPoll::Item(result) => {
                if let Some(path) = result {
                    self.start_api_local_import(path);
                }
                changed = true;
            }
            crate::platform::ReceiverPoll::Empty => {}
            crate::platform::ReceiverPoll::Disconnected => {
                self.ide_panel.api.import_error = Some(
                    "Окно выбора OpenAPI неожиданно завершилось".to_string(),
                );
                changed = true;
            }
        }
        match poll_api_one_shot(&mut self.api_body_file_rx) {
            crate::platform::ReceiverPoll::Item(result) => {
                self.apply_api_body_file_pick(result);
                changed = true;
            }
            crate::platform::ReceiverPoll::Empty => {}
            crate::platform::ReceiverPoll::Disconnected => {
                self.ide_panel.api.import_error = Some(
                    "Окно выбора body-файла неожиданно завершилось".to_string(),
                );
                changed = true;
            }
        }
        changed |= self.ide_panel.api.poll_python_receivers();

        let mut idx = 0usize;
        while idx < self.api_load_rx.len() {
            match self.api_load_rx[idx].rx.poll() {
                crate::ui_waker::OneShotState::Ready(result) => {
                    self.api_load_rx.remove(idx);
                    let Some(ticket) = self
                        .ide_panel
                        .api
                        .finish_load(result.id, result.generation)
                    else {
                        changed = true;
                        continue;
                    };
                    match result.result {
                        Ok(payload) => {
                            let id = payload.entry.id;
                            let previous_routes = self
                                .ide_panel
                                .api
                                .models
                                .get(&id)
                                .map(|model| {
                                    model
                                        .routes
                                        .iter()
                                        .map(|route| (route.method, route.path.clone()))
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default();
                            self.ide_panel
                                .api
                                .upsert_loaded(payload, ticket.select_on_success);
                            self.update_api_tabs_after_model_load(id, &previous_routes);
                            self.ide_panel.api.refresh_mock_server();
                        }
                        Err(err) => self.ide_panel.api.mark_load_error(result.id, err),
                    }
                    changed = true;
                }
                crate::ui_waker::OneShotState::Pending => idx += 1,
                crate::ui_waker::OneShotState::Closed => {
                    let failed = self.api_load_rx.remove(idx);
                    self.ide_panel.api.handle_load_disconnect(failed.id, failed.generation);
                    changed = true;
                }
            }
        }

        let mut idx = 0usize;
        while idx < self.api_request_rx.len() {
            let request_id = self.api_request_rx[idx].0;
            match self.api_request_rx[idx].1.poll() {
                crate::ui_waker::OneShotState::Ready(result) => {
                    self.api_request_rx.remove(idx);
                    self.apply_api_job_response(result);
                    changed = true;
                }
                crate::ui_waker::OneShotState::Pending => idx += 1,
                crate::ui_waker::OneShotState::Closed => {
                    self.api_request_rx.remove(idx);
                    for tab in &mut self.tabs {
                        let crate::app::EditorTabKind::ApiClient(meta, state) = &mut tab.kind else {
                            continue;
                        };
                        if state.apply_request_disconnect(meta.spec_id, request_id) {
                            break;
                        }
                    }
                    changed = true;
                }
            }
        }
        changed
    }

    fn update_api_tabs_after_model_load(
        &mut self,
        id: ApiSpecId,
        previous_routes: &[(ApiMethod, String)],
    ) {
        let title = self
            .ide_panel
            .api
            .specs
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.title.clone());
        let Some(model) = self.ide_panel.api.models.get(&id) else {
            return;
        };
        for tab in &mut self.tabs {
            let crate::app::EditorTabKind::ApiClient(meta, state) = &mut tab.kind else {
                continue;
            };
            if meta.spec_id != id {
                continue;
            }
            if let Some(title) = &title {
                meta.title = title.clone();
                tab.base_title = title.clone();
            }
            state.update_after_model_load(meta, previous_routes, model);
        }
    }

    fn apply_api_job_response(&mut self, result: ApiJobResponse) {
        let resolved = result.resolved_host.clone();
        let mut focused_text = None;
        let mut applied = false;
        for tab in &mut self.tabs {
            if let crate::app::EditorTabKind::ApiClient(meta, state) = &mut tab.kind
                && meta.spec_id == result.spec_id
            {
                let (did_apply, text) = self
                    .ide_panel
                    .api
                    .apply_api_job_response_to_tab(meta.spec_id, state, &result);
                if did_apply {
                    focused_text = text;
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
            self.ide_panel.api.input_editor.version = crate::editor::next_editor_version(old_version);
        }
    }

}
