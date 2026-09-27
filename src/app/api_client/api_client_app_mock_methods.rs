impl crate::app::App {
    pub fn scroll_api_python_runtime_overlay(&mut self, dy: f32) -> bool {
        let Some(renderer) = self.renderer.as_ref() else {
            return false;
        };
        let s = renderer.scale_factor;
        let layout = api_python_runtime_dialog_layout(renderer.width, renderer.height, s);
        let mx = renderer.last_mouse_x;
        let my = renderer.last_mouse_y;
        self.ide_panel
            .api
            .scroll_api_python_runtime_overlay(dy, mx, my, layout, s)
    }

    pub fn toggle_api_mock_server(&mut self) {
        self.commit_api_focus();
        if matches!(
            self.ide_panel.api.mock.server_status,
            crate::app::api_mock::types::ApiMockServerStatus::Running { .. }
                | crate::app::api_mock::types::ApiMockServerStatus::Starting
        ) {
            self.ide_panel.api.stop_api_mock_server();
            return;
        }
        self.sync_api_mock_proxy_base_to_active_server();
        self.ide_panel.api.start_api_mock_server();
    }

    fn api_route_override(
        &self,
        route_idx: usize,
    ) -> Option<&crate::app::api_mock::types::ApiMockRouteOverride> {
        let (meta, _) = self.active_api_tab()?;
        self.ide_panel
            .api
            .api_route_override(meta.spec_id, route_idx)
    }

    /// Active API tab identity passed to `ApiClientState` mock route lookups.
    pub(crate) fn api_active_route(&self) -> Option<ApiActiveRoute> {
        let (meta, state) = self.active_api_tab()?;
        let manual_stable_id = match &meta.route_identity {
            Some(ApiClientRouteIdentity::Manual { stable_id }) => Some(stable_id.clone()),
            _ => None,
        };
        Some(ApiActiveRoute {
            spec_id: meta.spec_id,
            route_idx: state.route_idx,
            manual_stable_id,
        })
    }

    fn active_manual_mock_route(
        &self,
        route_idx: usize,
    ) -> Option<&crate::app::api_mock::types::ApiManualRoute> {
        let active = self.api_active_route();
        self.ide_panel
            .api
            .active_manual_mock_route(active.as_ref(), route_idx)
    }

    fn active_manual_mock_route_mut(
        &mut self,
        route_idx: usize,
    ) -> Option<&mut crate::app::api_mock::types::ApiManualRoute> {
        let active = self.api_active_route();
        self.ide_panel
            .api
            .active_manual_mock_route_mut(active.as_ref(), route_idx)
    }

    fn api_route_override_mut(
        &mut self,
        route_idx: usize,
    ) -> Option<&mut crate::app::api_mock::types::ApiMockRouteOverride> {
        let (meta, _) = self.active_api_tab()?;
        self.ide_panel.api.api_route_override_mut(meta.spec_id, route_idx)
    }

    fn ensure_api_route_override(&mut self, route_idx: usize) {
        let Some((meta, _)) = self.active_api_tab() else {
            return;
        };
        self.ide_panel
            .api
            .ensure_api_route_override(meta.spec_id, route_idx);
    }

    fn api_route_python_script(
        &self,
        route_idx: usize,
    ) -> Option<&crate::app::api_mock::types::ApiMockPythonScript> {
        let active = self.api_active_route();
        self.ide_panel
            .api
            .api_route_python_script(active.as_ref(), route_idx)
    }

    fn api_route_python_script_mut(
        &mut self,
        route_idx: usize,
    ) -> Option<&mut crate::app::api_mock::types::ApiMockPythonScript> {
        let active = self.api_active_route();
        self.ide_panel
            .api
            .api_route_python_script_mut(active.as_ref(), route_idx)
    }

    fn focus_previous_api_mock_python_part(
        &mut self,
        route_idx: usize,
        part: ApiMockSourcePart,
    ) -> bool {
        let Some(previous) = ApiClientState::previous_api_mock_python_focus(route_idx, part) else {
            return false;
        };
        self.focus_api_input(previous);
        true
    }

    pub(crate) fn api_mock_route_context(
        &self,
        route_idx: usize,
    ) -> Option<(ApiMethod, String, ApiRouteRow, ApiSpecModel)> {
        let active = self.api_active_route();
        self.ide_panel
            .api
            .api_mock_route_context(active.as_ref(), route_idx)
    }

    pub(crate) fn api_mock_script_for_tools(
        &self,
        route_idx: usize,
    ) -> Option<crate::app::api_mock::types::ApiMockPythonScript> {
        let active = self.api_active_route();
        self.ide_panel
            .api
            .api_mock_script_for_tools(active.as_ref(), route_idx)
    }

    fn api_mock_edit_text_for_part(
        &self,
        route_idx: usize,
        part: ApiMockSourcePart,
        script: &crate::app::api_mock::types::ApiMockPythonScript,
    ) -> String {
        if self.ide_panel.api.api_mock_python_focus_target() == Some((route_idx, part)) {
            self.ide_panel.api.input_editor.get_full_text()
        } else {
            match part {
                ApiMockSourcePart::Contract => self
                    .api_mock_contract_source_for_route(route_idx)
                    .unwrap_or_default(),
                ApiMockSourcePart::Prelude => script.prelude.clone(),
                ApiMockSourcePart::Signature => self
                    .api_mock_signature_for_route(route_idx)
                    .unwrap_or_default(),
                ApiMockSourcePart::Body => script.body.clone(),
            }
        }
    }

    fn ensure_api_mock_hover_editor(&mut self, route_idx: usize, part: ApiMockSourcePart) -> bool {
        if self.ide_panel.api.api_mock_python_focus_target() == Some((route_idx, part)) {
            return true;
        }
        let key = (route_idx, part);
        if self.ide_panel.api.mock_python_editors.contains_key(&key) {
            return true;
        }
        let Some(script) = self.api_mock_script_for_tools(route_idx) else {
            return false;
        };
        let text = self.api_mock_edit_text_for_part(route_idx, part, &script);
        let mut editor = Editor::new(text.len().saturating_add(512));
        editor.set_text_clean(&text);
        self.ide_panel.api.mock_python_editors.insert(key, editor);
        true
    }

    fn api_mock_hover_module_path(&self, route_idx: usize) -> Option<String> {
        let (meta, _) = self.active_api_tab()?;
        let (method, path, _, model) = self.api_mock_route_context(route_idx)?;
        let spec_title = self
            .ide_panel
            .api
            .specs
            .iter()
            .find(|entry| entry.id == meta.spec_id)
            .map(|entry| entry.title.as_str())
            .filter(|title| !title.trim().is_empty())
            .unwrap_or(model.title.as_str());
        let spec = ApiClientState::api_mock_module_segment(spec_title, "spec");
        let route =
            ApiClientState::api_mock_module_segment(&format!("{}_{}", method.as_str(), path), "route");
        Some(format!("api_mock.{spec}.{route}"))
    }

    pub(crate) fn notify_api_mock_lsp_source(
        &mut self,
        virtual_path: &PathBuf,
        source: &str,
        base_version: i32,
    ) -> bool {
        let (doc_open, version) = self
            .ide_panel
            .api
            .mock_lsp_opened
            .get(virtual_path)
            .map(|opened_version| (true, base_version.max(opened_version.saturating_add(1))))
            .unwrap_or((false, base_version));
        let Some(lsp) = self.lsp.as_mut() else {
            return false;
        };
        if doc_open {
            lsp.notify_change(virtual_path, "py", source, version);
        } else {
            lsp.notify_open(virtual_path, "py", source, version);
        }
        self.ide_panel
            .api
            .mock_lsp_opened
            .insert(virtual_path.clone(), version);
        true
    }

    fn move_api_mock_hover_to_empty_space(&mut self) {
        self.ide_panel.api.mock_hover_request = None;
        let clear_target = crate::app::mouse::HOVER_STATE.with(|state| {
            let mut state = state.borrow_mut();
            crate::app::mouse::move_type_hover_to_empty_space(&mut state);
            state.byte_offset.is_none()
                && state.popup.is_none()
                && state.pending_popup.is_none()
                && state.rect.is_none()
                && state.diagnostic_popup_cache_is_empty()
        });
        if clear_target {
            self.ide_panel.api.mock_hover_target = None;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn api_mock_hover_anchor_for_target(
        &mut self,
        target: &ApiMockHoverTarget,
    ) -> Option<(f32, f32)> {
        let mut renderer = self.renderer.take()?;
        let anchor = (|| {
            let id = ApiClientState::api_mock_ui_for_part(target.route_idx, target.part);
            let rect = self.ui_registry.rect_for(id)?;
            let scale = renderer.scale_factor;
            let text_x = ApiClientState::api_multiline_cursor_left_x(id, rect, scale);
            let top_y = ApiClientState::api_multiline_cursor_top_y(id, rect, scale);
            let scroll_y = self.api_text_scroll_for_ui(id).round();
            let scroll_x = self.api_text_scroll_x_for_ui(id).round();
            let editor = self
                .ide_panel
                .api
                .api_mock_hover_editor(target.route_idx, target.part)?;
            let render_scroll_y = scroll_y - top_y;
            Some(ApiClientState::with_api_mock_hover_renderer_context(
                &mut renderer,
                editor,
                text_x,
                scroll_x,
                scale,
                |renderer| {
                    crate::app::mouse::hover_anchor_for_byte(
                        renderer,
                        editor,
                        target.edit_byte,
                        render_scroll_y,
                    )
                },
            ))
        })();
        self.renderer = Some(renderer);
        anchor
    }

    pub(crate) fn update_api_mock_hover_from_cursor(
        &mut self,
        mx: f32,
        my: f32,
        in_hover_popup: bool,
        in_hover_source_line: bool,
    ) -> bool {
        if !self.active_tab_is_api_client() {
            return false;
        }
        if in_hover_popup && !in_hover_source_line {
            return true;
        }

        let Some(focus) = self.ui_registry.find_at(mx, my) else {
            self.move_api_mock_hover_to_empty_space();
            return true;
        };
        let Some((route_idx, part)) = ApiClientState::api_mock_part_for_ui(focus) else {
            self.move_api_mock_hover_to_empty_space();
            return true;
        };
        let Some(rect) = self.ui_registry.rect_for(focus) else {
            self.ide_panel.api.reset_api_mock_hover_tracking();
            return true;
        };
        if self
            .ide_panel
            .api
            .mock_hover_target
            .as_ref()
            .is_some_and(|target| target.route_idx != route_idx || target.part != part)
        {
            self.ide_panel.api.reset_api_mock_hover_tracking();
        }
        if mx < rect.0 || mx > rect.0 + rect.2 || my < rect.1 || my > rect.1 + rect.3 {
            if !in_hover_popup {
                self.move_api_mock_hover_to_empty_space();
            }
            return true;
        }

        let scroll_y = self.api_text_scroll_for_ui(focus).round();
        let scroll_x = self.api_text_scroll_x_for_ui(focus).round();
        let scale = self
            .renderer
            .as_ref()
            .map(|renderer| renderer.scale_factor)
            .unwrap_or(1.0);
        let text_x = ApiClientState::api_multiline_cursor_left_x(focus, rect, scale);
        let top_y = ApiClientState::api_multiline_cursor_top_y(focus, rect, scale);
        let text_y = ApiClientState::api_mock_text_baseline_y(focus, rect, scale);
        if !self.ensure_api_mock_hover_editor(route_idx, part) {
            self.ide_panel.api.reset_api_mock_hover_tracking();
            return true;
        }
        let ty_diagnostics = if matches!(
            self.ide_panel.api.mock.check_status,
            crate::app::api_mock::types::ApiMockCheckStatus::Failed {
                route_idx: checked,
                ..
            } if checked == route_idx
        ) {
            Some(self.ide_panel.api.mock_ty_diagnostics.as_slice())
        } else {
            None
        };
        let diag_hover = if let Some(ty_diagnostics) = ty_diagnostics {
            let Some(editor) = self.ide_panel.api.api_mock_hover_editor(route_idx, part) else {
                self.ide_panel.api.reset_api_mock_hover_tracking();
                return true;
            };
            let text = editor.get_full_text();
            if let Some(renderer) = self.renderer.as_mut() {
                ApiClientState::api_mock_ty_diag_hover_at_point(
                    renderer,
                    &text,
                    ty_diagnostics,
                    part,
                    rect,
                    text_x,
                    text_y,
                    scale,
                    scroll_y,
                    scroll_x,
                    mx,
                    my,
                )
            } else {
                None
            }
        } else {
            None
        };
        let hover_byte = if let Some((_, byte)) = diag_hover.as_ref() {
            Some(*byte)
        } else {
            if self.ide_panel.api.api_mock_python_focus_target() == Some((route_idx, part)) {
                if let Some(renderer) = self.renderer.as_mut() {
                    ApiClientState::api_mock_hover_byte_at_point(
                        &self.ide_panel.api.input_editor,
                        renderer,
                        text_x,
                        top_y,
                        mx,
                        my,
                        scale,
                        scroll_y,
                        scroll_x,
                    )
                } else {
                    None
                }
            } else {
                let key = (route_idx, part);
                let Some(editor) = self.ide_panel.api.mock_python_editors.remove(&key) else {
                    self.ide_panel.api.reset_api_mock_hover_tracking();
                    return true;
                };
                let byte = if let Some(renderer) = self.renderer.as_mut() {
                    ApiClientState::api_mock_hover_byte_at_point(
                        &editor, renderer, text_x, top_y, mx, my, scale, scroll_y, scroll_x,
                    )
                } else {
                    None
                };
                self.ide_panel.api.mock_python_editors.insert(key, editor);
                byte
            }
        };
        let Some(hover_byte) = hover_byte else {
            if !in_hover_popup {
                self.move_api_mock_hover_to_empty_space();
            }
            return true;
        };
        let Some(hover_editor) = self.ide_panel.api.api_mock_hover_editor(route_idx, part) else {
            self.ide_panel.api.reset_api_mock_hover_tracking();
            return true;
        };
        let mut target = ApiMockHoverTarget {
            route_idx,
            part,
            edit_byte: hover_byte,
            version: hover_editor.version,
        };
        let mut reset_diag_popup = false;
        let mut accepted_hover_target = false;
        let mut clear_mock_hover_request = false;
        let mut stable_target_byte = target.edit_byte;
        crate::app::mouse::HOVER_STATE.with(|state| {
            let mut state = state.borrow_mut();
            match crate::app::mouse::update_editor_hover_state_for_cursor(
                &mut state,
                hover_editor,
                target.edit_byte,
                diag_hover.as_ref().map(|(_, byte)| *byte),
                true,
                in_hover_popup,
                in_hover_source_line,
                false,
            ) {
                Some(should_reset_diag_popup) => {
                    accepted_hover_target = true;
                    reset_diag_popup = should_reset_diag_popup;
                }
                None => {
                    return;
                }
            }
            clear_mock_hover_request = state.request_id.is_none();
            if reset_diag_popup {
                state.reset_diagnostic_popup();
            }
            if crate::app::mouse::hover_bytes_share_token(
                hover_editor,
                state.byte_offset,
                Some(target.edit_byte),
            ) && let Some(byte_offset) = state.byte_offset
            {
                stable_target_byte = byte_offset;
            }
            if let Some((diagnostic, type_target)) = diag_hover {
                state.record_hovered_diagnostic(diagnostic, Some(type_target));
                state.update_hovered_diag_type_target_for_frame(Some(type_target));
            } else if !state.stale_combined_popup {
                state.hovered_diags_cache.clear();
                state.hovered_diags.clear();
                state.hovered_diag_type_target = None;
            }
        });
        if !accepted_hover_target {
            return true;
        }
        target.edit_byte = stable_target_byte;
        self.ide_panel.api.mock_hover_target = Some(target.clone());
        if clear_mock_hover_request {
            self.ide_panel.api.mock_hover_request = None;
        }
        true
    }

    fn api_mock_virtual_hover_source(
        &self,
        target: &ApiMockHoverTarget,
    ) -> Option<(String, usize)> {
        let (method, path, route, model) = self.api_mock_route_context(target.route_idx)?;
        let script = self.api_mock_script_for_tools(target.route_idx)?;
        let edit_text = self
            .ide_panel
            .api
            .api_mock_hover_editor(target.route_idx, target.part)?
            .get_full_text();
        let virtual_source = build_api_mock_virtual_source(method, &path, &route, &model, &script);
        let source_cursor =
            virtual_source.edit_offset_to_source(target.part, &edit_text, target.edit_byte);
        Some((virtual_source.source, source_cursor))
    }

    fn clear_api_mock_hover_response(&mut self, target: &ApiMockHoverTarget) {
        crate::app::mouse::HOVER_STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.request_id = None;
            state.definition_request_id = None;
            state.pending_popup = None;
            if state.byte_offset == Some(target.edit_byte) {
                state.popup = None;
                state.rect = None;
            }
        });
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub(crate) fn request_active_api_mock_hover(&mut self) -> bool {
        let Some(target) = self.ide_panel.api.mock_hover_target.clone() else {
            return false;
        };
        if self.ide_panel.api.mock_hover_request.is_some() {
            return true;
        }
        if !self.ensure_api_mock_hover_editor(target.route_idx, target.part) {
            self.clear_api_mock_hover_response(&target);
            return false;
        }
        let Some((source, source_cursor)) = self.api_mock_virtual_hover_source(&target) else {
            self.clear_api_mock_hover_response(&target);
            return false;
        };
        let anchor = self
            .api_mock_hover_anchor_for_target(&target)
            .unwrap_or((0.0, 0.0));
        let mut line_offsets = vec![0usize];
        for (idx, b) in source.bytes().enumerate() {
            if b == b'\n' {
                line_offsets.push(idx + 1);
            }
        }
        let (line, col) = crate::lsp::offset_to_lsp_pos(&source, source_cursor, &line_offsets);
        let Some(spec_id) = self.active_api_tab().map(|(meta, _)| meta.spec_id) else {
            return false;
        };
        let virtual_path = ApiClientState::api_mock_virtual_path_for(spec_id, target.route_idx);
        let base_version = crate::editor::lsp_document_version(target.version);
        if !self.notify_api_mock_lsp_source(&virtual_path, &source, base_version) {
            return false;
        }
        let Some(lsp) = self.lsp.as_mut() else {
            return false;
        };
        let Some(request_id) = lsp.request_hover(&virtual_path, "py", line, col) else {
            return false;
        };
        let target_edit_byte = target.edit_byte;
        self.ide_panel.api.mock_hover_request = Some(ApiMockHoverRequest {
            request_id,
            target,
            source,
            source_cursor,
            anchor,
        });
        crate::app::mouse::HOVER_STATE.with(|state| {
            let mut state = state.borrow_mut();
            if state.byte_offset == Some(target_edit_byte) {
                state.request_id = Some(request_id);
            }
        });
        true
    }

    pub(crate) fn apply_api_mock_hover_response(
        &mut self,
        request_id: i32,
        text: Option<String>,
    ) -> bool {
        let Some(request) = self.ide_panel.api.mock_hover_request.clone() else {
            return false;
        };
        if request.request_id != request_id {
            return false;
        }
        self.ide_panel.api.mock_hover_request = None;
        let target = request.target;
        let module_path = self.api_mock_hover_module_path(target.route_idx);
        let mut editor = Editor::new(request.source.len().saturating_add(512));
        editor.set_text_clean(&request.source);
        crate::app::mouse::HOVER_STATE.with(|state| {
            let mut state = state.borrow_mut();
            if self.ide_panel.api.mock_hover_target.as_ref() != Some(&target) {
                return;
            }
            if crate::app::events::apply_source_hover_response_to_state(
                &mut state,
                request_id,
                &editor,
                target.edit_byte,
                request.source_cursor,
                text,
                module_path.as_deref(),
                request.anchor,
                || None,
            ) && let (Some(module_path), Some(popup)) =
                (module_path.as_deref(), state.popup.as_mut())
            {
                crate::app::events::prepend_hover_module_path(popup, module_path);
            }
        });
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
        true
    }

    fn refresh_api_mock_python_highlight(&mut self, route_idx: usize, part: ApiMockSourcePart) {
        let Some((method, path, route, model)) = self.api_mock_route_context(route_idx) else {
            return;
        };
        let Some(script) = self.api_mock_script_for_tools(route_idx) else {
            return;
        };
        let virtual_source = build_api_mock_virtual_source(method, &path, &route, &model, &script);
        let edit_text = self.api_mock_edit_text_for_part(route_idx, part, &script);
        let version = self.ide_panel.api.input_editor.version;
        let edit_cursor = if self.ide_panel.api.api_mock_python_focus_target() == Some((route_idx, part)) {
            self.ide_panel.api.input_editor.cursor
        } else {
            0
        };
        let source_cursor = virtual_source.edit_offset_to_source(part, &edit_text, edit_cursor);
        self.ide_panel.api.mock_highlight_spans = self
            .ide_panel
            .api
            .mock_highlight_cache
            .get(&(route_idx, part))
            .cloned()
            .unwrap_or_default();
        self.ide_panel.api.mock_highlight_target = Some((route_idx, part, version));
        let source = virtual_source.source.clone();
        self.ide_panel.api.mock_highlighter.spans.clear();
        self.ide_panel
            .api
            .mock_highlighter
            .reset(version, source, "py".to_string(), source_cursor);
        if self
            .ide_panel
            .api
            .mock_highlighter
            .sync_highlight_after_edit(version, None, None, None, None, Duration::from_millis(4))
        {
            let spans = self.ide_panel.api.mock_highlighter.spans.clone();
            self.ide_panel.api.refresh_api_mock_highlight_cache_for_spans(route_idx, &spans, &virtual_source);
            self.ide_panel.api.mock_highlight_spans = self
                .ide_panel
                .api
                .mock_highlight_cache
                .get(&(route_idx, part))
                .cloned()
                .unwrap_or_default();
        }
    }

    fn queue_api_mock_python_tools(&mut self, route_idx: usize) {
        if let Some((focused_route, part)) = self.ide_panel.api.api_mock_python_focus_target()
            && focused_route == route_idx
        {
            self.refresh_api_mock_python_highlight(route_idx, part);
            self.ide_panel.api.mock_ty_due = Some(Instant::now() + Duration::from_millis(450));
        }
    }

    fn api_mock_route_tools_version(&mut self, route_idx: usize) -> Option<u64> {
        let mut version = 0;
        for part in [
            ApiMockSourcePart::Contract,
            ApiMockSourcePart::Prelude,
            ApiMockSourcePart::Signature,
            ApiMockSourcePart::Body,
        ] {
            if self.ide_panel.api.api_mock_python_focus_target() == Some((route_idx, part)) {
                version = version.max(self.ide_panel.api.input_editor.version);
                continue;
            }
            if !self.ensure_api_mock_hover_editor(route_idx, part) {
                return None;
            }
            let editor = self
                .ide_panel
                .api
                .mock_python_editors
                .get(&(route_idx, part))?;
            version = version.max(editor.version);
        }
        Some(version)
    }

    fn start_api_mock_route_tools_now(&mut self, route_idx: usize) -> bool {
        if self.api_mock_script_for_tools(route_idx).is_none() || self.api_mock_ty_rx.is_some() {
            return false;
        }
        let part = self.ide_panel.api.api_mock_python_focus_target()
            .filter(|(focused_route, _)| *focused_route == route_idx)
            .map(|(_, part)| part)
            .unwrap_or(ApiMockSourcePart::Body);
        self.refresh_api_mock_python_highlight(route_idx, part);
        let Some(version) = self.api_mock_route_tools_version(route_idx) else {
            return false;
        };
        self.ide_panel.api.mock_ty_due = None;
        self.start_api_mock_ty_check_now(route_idx, version);
        true
    }

    fn ensure_active_api_mock_highlight(&mut self) -> bool {
        let Some((spec_id, route_idx)) = self
            .active_api_tab()
            .and_then(|(meta, state)| state.route_idx.map(|route_idx| (meta.spec_id, route_idx)))
        else {
            return false;
        };
        if !self
            .ide_panel
            .api
            .expanded_mock_routes
            .contains(&(spec_id, route_idx))
        {
            return false;
        }
        if self.api_route_python_script(route_idx).is_none() {
            return false;
        }
        for part in [
            ApiMockSourcePart::Contract,
            ApiMockSourcePart::Prelude,
            ApiMockSourcePart::Signature,
            ApiMockSourcePart::Body,
        ] {
            self.ensure_api_mock_hover_editor(route_idx, part);
        }
        if self
            .ide_panel
            .api
            .mock_highlight_target
            .is_some_and(|(highlight_route, _, _)| highlight_route == route_idx)
        {
            return false;
        }
        let missing_cache = [
            ApiMockSourcePart::Contract,
            ApiMockSourcePart::Prelude,
            ApiMockSourcePart::Signature,
            ApiMockSourcePart::Body,
        ]
        .into_iter()
        .any(|part| {
            !self
                .ide_panel
                .api
                .mock_highlight_cache
                .contains_key(&(route_idx, part))
        });
        if !missing_cache {
            return false;
        }
        self.refresh_api_mock_python_highlight(route_idx, ApiMockSourcePart::Body);
        true
    }

    pub fn toggle_api_route_mock(&mut self, route_idx: usize) {
        self.commit_api_focus();
        let Some((meta, _)) = self.active_api_tab() else {
            return;
        };
        let spec_id = meta.spec_id;
        let Some(entry) = self
            .ide_panel
            .api
            .specs
            .iter()
            .find(|entry| entry.id == spec_id)
            .cloned()
        else {
            return;
        };
        let Some(route) = self
            .ide_panel
            .api
            .models
            .get(&spec_id)
            .and_then(|model| model.routes.get(route_idx))
            .cloned()
        else {
            return;
        };
        let source_key = crate::app::api_mock::types::api_mock_source_key(&entry);
        let focused_this_route = self.ide_panel.api.api_mock_python_focus_target()
            .is_some_and(|(focused_route, _)| focused_route == route_idx);
        let mut disabled_active_script = false;
        let enabled_route;
        if let Some(override_route) =
            self.ide_panel
                .api
                .mock
                .route_overrides
                .iter_mut()
                .find(|item| {
                    item.source_key == source_key
                        && item.method == route.method
                        && item.path == route.path
                })
        {
            let will_enable = !override_route.enabled;
            override_route.enabled = will_enable;
            override_route.proxy_when_disabled = !will_enable;
            enabled_route = will_enable;
            if !will_enable && let Some(script) = override_route.python.as_mut() {
                disabled_active_script = script.enabled;
                script.enabled = false;
            }
        } else {
            self.ide_panel.api.mock.route_overrides.push(
                crate::app::api_mock::types::ApiMockRouteOverride {
                    source_key,
                    method: route.method,
                    path: route.path,
                    enabled: true,
                    proxy_when_disabled: false,
                    response: crate::app::api_mock::types::ApiMockResponse::Generated,
                    python: None,
                    extra_input_fields: Vec::new(),
                    extra_output_fields: Vec::new(),
                },
            );
            enabled_route = true;
        }
        if enabled_route {
            self.ide_panel.api.mock.mode =
                crate::app::api_mock::types::ApiMockMode::MockSelectedProxyRest;
        }
        if disabled_active_script && focused_this_route {
            self.ide_panel.api.stash_active_api_mock_editor();
            self.ide_panel.api.focused = None;
        }
        self.ide_panel.api.commit_mock_config();
    }

    pub fn toggle_api_route_python(&mut self, route_idx: usize) -> bool {
        self.commit_api_focus();
        if self.active_manual_mock_route(route_idx).is_some() {
            let default_contract = self
                .active_manual_mock_route(route_idx)
                .map(|route| {
                    crate::app::api_mock::types::default_contract_for_manual_route(&route.path)
                })
                .unwrap_or_default();
            let focused_this_route = self.ide_panel.api.api_mock_python_focus_target()
                .is_some_and(|(focused_route, _)| focused_route == route_idx);
            let mut disabled_active_script = false;
            let mut enabled_script = false;
            if let Some(route) = self.active_manual_mock_route_mut(route_idx) {
                route.enabled = true;
                if let Some(script) = route.python.as_mut() {
                    if script.contract.is_empty() {
                        script.contract = default_contract.clone();
                    } else if !script.contract.response.enabled
                        && script.contract.response.fields.is_empty()
                    {
                        script.contract.response = default_contract.response.clone();
                    }
                    let will_enable = !script.enabled;
                    if will_enable && is_legacy_api_mock_python_body(&script.body) {
                        script.body = api_mock_default_handler_body(&script.contract);
                    }
                    script.enabled = will_enable;
                    disabled_active_script = !script.enabled;
                    enabled_script = script.enabled;
                } else {
                    let mut script = default_api_mock_python_script();
                    script.contract = default_contract;
                    script.body = api_mock_default_handler_body(&script.contract);
                    route.python = Some(script);
                    enabled_script = true;
                }
            }
            if disabled_active_script && focused_this_route {
                self.ide_panel.api.stash_active_api_mock_editor();
                self.ide_panel.api.focused = None;
            }
            if enabled_script {
                self.ide_panel.api.mock.mode =
                    crate::app::api_mock::types::ApiMockMode::MockSelectedProxyRest;
            }
            self.ide_panel.api.commit_mock_config();
            return enabled_script;
        }
        let Some((meta, _)) = self.active_api_tab() else {
            return false;
        };
        let spec_id = meta.spec_id;
        let Some(entry) = self
            .ide_panel
            .api
            .specs
            .iter()
            .find(|entry| entry.id == spec_id)
            .cloned()
        else {
            return false;
        };
        let Some(model) = self.ide_panel.api.models.get(&spec_id).cloned() else {
            return false;
        };
        let Some(route) = model.routes.get(route_idx).cloned() else {
            return false;
        };
        let default_contract = default_contract_from_route(&route, &model);
        let source_key = crate::app::api_mock::types::api_mock_source_key(&entry);
        let idx = if let Some(idx) =
            self.ide_panel
                .api
                .mock
                .route_overrides
                .iter()
                .position(|item| {
                    item.source_key == source_key
                        && item.method == route.method
                        && item.path == route.path
                }) {
            idx
        } else {
            self.ide_panel.api.mock.route_overrides.push(
                crate::app::api_mock::types::ApiMockRouteOverride {
                    source_key,
                    method: route.method,
                    path: route.path,
                    enabled: false,
                    proxy_when_disabled: false,
                    response: crate::app::api_mock::types::ApiMockResponse::Generated,
                    python: None,
                    extra_input_fields: Vec::new(),
                    extra_output_fields: Vec::new(),
                },
            );
            self.ide_panel
                .api
                .mock
                .route_overrides
                .len()
                .saturating_sub(1)
        };
        let focused_this_route = self.ide_panel.api.api_mock_python_focus_target()
            .is_some_and(|(focused_route, _)| focused_route == route_idx);
        let mut disabled_active_script = false;
        let mut enabled_script = false;
        if let Some(override_route) = self.ide_panel.api.mock.route_overrides.get_mut(idx) {
            if let Some(script) = override_route.python.as_mut() {
                if script.contract.is_empty() {
                    script.contract = default_contract.clone();
                } else if !script.contract.response.enabled
                    && script.contract.response.fields.is_empty()
                {
                    script.contract.response = default_contract.response.clone();
                }
                let will_enable = !script.enabled;
                if will_enable && is_legacy_api_mock_python_body(&script.body) {
                    script.body = api_mock_default_handler_body(&script.contract);
                }
                script.enabled = will_enable;
                disabled_active_script = !script.enabled;
                enabled_script = script.enabled;
                if will_enable {
                    override_route.enabled = true;
                    override_route.proxy_when_disabled = false;
                }
            } else {
                let mut script = default_api_mock_python_script();
                script.contract = default_contract;
                script.body = api_mock_default_handler_body(&script.contract);
                override_route.python = Some(script);
                override_route.enabled = true;
                override_route.proxy_when_disabled = false;
                enabled_script = true;
            }
        }
        if disabled_active_script && focused_this_route {
            self.ide_panel.api.stash_active_api_mock_editor();
            self.ide_panel.api.focused = None;
        }
        if enabled_script {
            self.ide_panel.api.mock.mode =
                crate::app::api_mock::types::ApiMockMode::MockSelectedProxyRest;
        }
        self.ide_panel.api.commit_mock_config();
        enabled_script
    }

    pub fn reset_api_route_python_part(&mut self, route_idx: usize, part: ApiMockSourcePart) {
        self.commit_api_focus();
        let default_contract = if part == ApiMockSourcePart::Contract {
            self.api_mock_route_context(route_idx)
                .map(|(_, _, route, model)| {
                    crate::app::api_mock::types::default_contract_from_route(&route, &model)
                })
        } else {
            None
        };
        let Some(script) = self.api_route_python_script_mut(route_idx) else {
            return;
        };
        match part {
            ApiMockSourcePart::Contract => {
                script.contract = default_contract.unwrap_or_default();
                script.contract_source.clear();
            }
            ApiMockSourcePart::Prelude => script.prelude.clear(),
            ApiMockSourcePart::Signature => return,
            ApiMockSourcePart::Body => {
                script.body = api_mock_default_handler_body(&script.contract)
            }
        }
        self.ide_panel
            .api
            .mock_python_editors
            .remove(&(route_idx, part));
        self.ide_panel
            .api
            .mock_highlight_cache
            .retain(|(cached_route, _), _| *cached_route != route_idx);
        self.ide_panel.api.mock_highlight_target = None;
        self.ide_panel.api.mock_highlight_spans.clear();
        self.ide_panel.api.mock_ty_diagnostics.clear();
        if self.ide_panel.api.api_mock_python_focus_target() == Some((route_idx, part)) {
            let text = match part {
                ApiMockSourcePart::Contract => self
                    .api_mock_contract_source_for_route(route_idx)
                    .unwrap_or_default(),
                ApiMockSourcePart::Prelude | ApiMockSourcePart::Signature => String::new(),
                ApiMockSourcePart::Body => self
                    .api_route_python_script(route_idx)
                    .map(|script| api_mock_default_handler_body(&script.contract))
                    .unwrap_or_else(default_api_mock_python_body),
            };
            let old_version = self.ide_panel.api.input_editor.version;
            self.ide_panel.api.input_editor.set_text_clean(&text);
            self.ide_panel.api.input_editor.version = crate::editor::next_editor_version(old_version);
            self.ide_panel.api.input_editor.cursor = self.ide_panel.api.input_editor.len();
            self.ide_panel.api.input_editor.selection_anchor =
                Some(self.ide_panel.api.input_editor.cursor);
        }
        self.ide_panel.api.commit_mock_config();
        self.queue_api_mock_python_tools(route_idx);
    }

    pub fn add_api_manual_route(&mut self) {
        self.commit_api_focus();
        self.ide_panel.api.mock.mode =
            crate::app::api_mock::types::ApiMockMode::MockSelectedProxyRest;
        let next = self
            .ide_panel
            .api
            .mock
            .manual_routes
            .len()
            .saturating_add(1);
        self.ide_panel
            .api
            .mock
            .manual_routes
            .push(crate::app::api_mock::types::ApiManualRoute {
                stable_id: format!("manual-{}-{}", now_epoch_secs(), next),
                method: ApiMethod::Get,
                path: format!("/mock-{}", next),
                enabled: true,
                response: crate::app::api_mock::types::ApiMockResponse::Generated,
                python: None,
                input_fields: Vec::new(),
                output_fields: Vec::new(),
            });
        self.ide_panel.api.commit_mock_config();
        self.open_api_manual_route(next.saturating_sub(1));
    }

    fn start_api_mock_ty_check_now(&mut self, route_idx: usize, version: u64) {
        let Some((method, path, route, model)) = self.api_mock_route_context(route_idx) else {
            return;
        };
        let Some(script) = self.api_mock_script_for_tools(route_idx) else {
            return;
        };
        self.ide_panel.api.mock.check_status =
            crate::app::api_mock::types::ApiMockCheckStatus::Pending { route_idx, version };
        self.ide_panel.api.mock_ty_diagnostics.clear();
        self.ide_panel.api.mock_ty_pending = Some((route_idx, version));
        let runtime = self.ide_panel.api.mock.uv.runtime_config();
        self.api_mock_ty_rx = Some(spawn_api_mock_ty_check(
            route_idx, version, runtime, method, path, route, model, script,
        ));
    }

    pub(crate) fn api_mock_autocomplete_anchor(&mut self) -> Option<(f32, f32)> {
        let focus = self.ide_panel.api.focused.as_ref()?;
        let (id, multiline) = self.ide_panel.api.api_focus_ui_target(focus)?;
        let rect = self.ui_registry.rect_for(id)?;
        let text = self.ide_panel.api.input_editor.get_full_text();
        let cursor = self.ide_panel.api.input_editor.cursor;
        let scroll_x = if multiline {
            self.api_text_scroll_x_for_ui(id).round()
        } else {
            self.ide_panel.api.input_scroll_x.current.round()
        };
        let renderer = self.renderer.as_mut()?;
        let scale = renderer.scale_factor;
        Some(ApiClientState::api_mock_autocomplete_anchor_for_text(
            id,
            rect,
            scale,
            &text,
            cursor,
            scroll_x,
            |line_prefix| renderer.measure_ui_width(line_prefix, API_BODY_TEXT_SCALE),
        ))
    }

    pub(crate) fn update_api_mock_tree_sitter_autocomplete(&mut self) {
        let Some(source) = self.active_api_mock_autocomplete_source() else {
            return;
        };
        self.update_tree_sitter_autocomplete_for_source(source);
    }

    pub(crate) fn request_api_mock_ty_autocomplete(&mut self, trigger: Option<&str>) {
        let Some(source) = self.active_api_mock_autocomplete_source() else {
            return;
        };
        self.request_ty_autocomplete_for_source(
            source,
            crate::app::AutocompleteMode::TyContext,
            trigger,
        );
    }

    pub(crate) fn update_api_mock_ty_autocomplete(
        &mut self,
        items: Vec<crate::lsp::LspCompletionItem>,
    ) {
        let Some(source) = self.active_api_mock_autocomplete_source() else {
            return;
        };
        self.update_ty_autocomplete_for_source(source, items);
    }

    pub(crate) fn update_api_mock_ty_signature_help_autocomplete(
        &mut self,
        parameters: Vec<String>,
    ) {
        let Some(source) = self.active_api_mock_autocomplete_source() else {
            return;
        };
        self.update_ty_signature_help_autocomplete_for_source(source, parameters);
    }

    pub(crate) fn apply_api_mock_autocomplete(&mut self) -> bool {
        let Some((route_idx, part)) = self.ide_panel.api.api_mock_python_focus_target() else {
            return false;
        };
        if !self.autocomplete_active || self.autocomplete_options.is_empty() {
            return true;
        }
        let Some((item, _)) = self
            .autocomplete_options
            .get(self.autocomplete_selected_idx)
            .or_else(|| self.autocomplete_options.first())
        else {
            self.close_autocomplete();
            return true;
        };
        let item = item.clone();
        let edit_text = self.ide_panel.api.input_editor.get_full_text();
        let virtual_source =
            self.api_mock_route_context(route_idx)
                .and_then(|(method, path, route, model)| {
                    self.api_mock_script_for_tools(route_idx).map(|script| {
                        build_api_mock_virtual_source(method, &path, &route, &model, &script)
                    })
                });
        let mut ops = Vec::new();
        let mut prelude_imports = Vec::new();
        if let Some(virtual_source) = virtual_source.as_ref() {
            if let Some(main_edit) = item.text_edit.as_ref()
                && let Some(op) = api_mock_lsp_edit_to_input_op(
                    virtual_source,
                    part,
                    &virtual_source.source,
                    main_edit,
                )
            {
                ops.push(op);
            }
            for edit in &item.additional_text_edits {
                if let Some(text) = api_mock_import_text(&edit.new_text) {
                    prelude_imports.push(text.to_string());
                    continue;
                }
                if let Some(op) = api_mock_lsp_edit_to_input_op(
                    virtual_source,
                    part,
                    &virtual_source.source,
                    edit,
                ) {
                    ops.push(op);
                }
            }
        }
        let selected = item
            .insert_text
            .as_deref()
            .unwrap_or(&item.word)
            .to_string();
        let prefix_len = self
            .active_api_mock_autocomplete_source()
            .and_then(|source| self.active_autocomplete_source_snapshot(source))
            .map(|snapshot| snapshot.current_word_prefix().len())
            .unwrap_or(0);
        let primary_start = ops.first().map(|op| op.start);
        let target_cursor = ops
            .first()
            .map(|op| op.start.saturating_add(op.new_text.len()));
        crate::app::apply_completion_plan_to_editor(
            &mut self.ide_panel.api.input_editor,
            crate::app::CompletionApplyPlan {
                ops,
                primary_start,
                target_cursor,
                fallback_insert: selected,
                fallback_prefix_len: prefix_len,
            },
        );
        if !prelude_imports.is_empty() {
            if part == ApiMockSourcePart::Prelude {
                let cursor_after_apply = self.ide_panel.api.input_editor.cursor;
                let mut insert = String::new();
                if !edit_text.trim().is_empty() && !edit_text.ends_with('\n') {
                    insert.push('\n');
                }
                for text in prelude_imports {
                    insert.push_str(&text);
                    insert.push('\n');
                }
                let end = self.ide_panel.api.input_editor.len();
                let _ = self
                    .ide_panel
                    .api
                    .input_editor
                    .replace_range(end, end, &insert);
                self.ide_panel.api.input_editor.cursor =
                    cursor_after_apply.min(self.ide_panel.api.input_editor.len());
                self.ide_panel.api.input_editor.selection_anchor = None;
            } else if let Some(script) = self.api_route_python_script_mut(route_idx) {
                for text in prelude_imports {
                    if !script.prelude.trim().is_empty() && !script.prelude.ends_with('\n') {
                        script.prelude.push('\n');
                    }
                    script.prelude.push_str(&text);
                    script.prelude.push('\n');
                }
                self.ide_panel.api.commit_mock_config();
            }
        }
        self.commit_api_focus();
        self.queue_api_mock_python_tools(route_idx);
        self.close_autocomplete();
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
        true
    }
}
