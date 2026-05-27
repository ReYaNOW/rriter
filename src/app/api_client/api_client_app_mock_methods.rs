impl crate::app::App {
    pub fn api_python_runtime_overlay_active(&self) -> bool {
        self.ide_panel.api.mock_python_runtime_open
    }

    pub fn api_runtime_poll_pending(&self) -> bool {
        self.ide_panel.api.python_version_list_rx.is_some()
            || self.ide_panel.api.python_install_rx.is_some()
            || self.ide_panel.api.python_path_pick_rx.is_some()
    }

    pub fn scroll_api_python_runtime_overlay(&mut self, dy: f32) -> bool {
        let Some(renderer) = self.renderer.as_ref() else {
            return false;
        };
        let s = renderer.scale_factor;
        let layout = api_python_runtime_dialog_layout(renderer.width, renderer.height, s);
        let mx = renderer.last_mouse_x;
        let my = renderer.last_mouse_y;
        if self.ide_panel.api.mock_python_version_picker_open {
            let rect = api_python_version_list_rect(layout, s);
            if api_point_in_rect(mx, my, rect) {
                let max_scroll =
                    api_python_version_list_max_scroll(self.ide_panel.api.mock_python_versions.len(), s);
                self.ide_panel.api.mock_python_versions_scroll.anim_speed = 7.0;
                self.ide_panel.api.mock_python_versions_scroll.scroll_by(dy);
                self.ide_panel
                    .api
                    .mock_python_versions_scroll
                    .clamp_target(0.0, max_scroll);
                return true;
            }
        }
        if api_python_install_log_visible(&self.ide_panel.api) {
            let rect = api_python_install_log_rect(layout, s);
            if api_point_in_rect(mx, my, rect) {
                let max_scroll = api_python_install_log_max_scroll(
                    self.ide_panel.api.mock_python_install_log.len(),
                    rect.3,
                    s,
                );
                self.ide_panel.api.mock_python_install_log_scroll.anim_speed = 7.0;
                self.ide_panel.api.mock_python_install_log_scroll.scroll_by(dy);
                self.ide_panel
                    .api
                    .mock_python_install_log_scroll
                    .clamp_target(0.0, max_scroll);
                return true;
            }
        }
        true
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
                | crate::ui_system::UiId::ApiMockPythonUvPathInput
                | crate::ui_system::UiId::ApiMockPythonVersionInput
                | crate::ui_system::UiId::ApiMockPythonCustomPathInput
        )
    }

    pub fn toggle_api_mock_server(&mut self) {
        self.commit_api_focus();
        if matches!(
            self.ide_panel.api.mock.server_status,
            crate::app::api_mock::types::ApiMockServerStatus::Running { .. }
                | crate::app::api_mock::types::ApiMockServerStatus::Starting
        ) {
            self.ide_panel.api.mock.server_status =
                crate::app::api_mock::types::ApiMockServerStatus::Stopping;
            push_api_mock_server_log(&mut self.ide_panel.api, "server stop requested".to_string());
            stop_api_mock_server();
            return;
        }
        let snapshot = self.ide_panel.api.mock_server_snapshot();
        self.ide_panel.api.mock.server_status =
            crate::app::api_mock::types::ApiMockServerStatus::Starting;
        push_api_mock_server_log(
            &mut self.ide_panel.api,
            format!("server start requested {}:{}", snapshot.bind_host, snapshot.port),
        );
        if let Err(err) = start_api_mock_server(snapshot) {
            self.ide_panel.api.mock.server_status =
                crate::app::api_mock::types::ApiMockServerStatus::Failed(err.clone());
            push_api_mock_server_log(&mut self.ide_panel.api, format!("server start failed: {err}"));
        }
    }

    fn api_route_override(
        &self,
        route_idx: usize,
    ) -> Option<&crate::app::api_mock::types::ApiMockRouteOverride> {
        let (meta, _) = self.active_api_tab()?;
        let spec_id = meta.spec_id;
        let entry = self
            .ide_panel
            .api
            .specs
            .iter()
            .find(|entry| entry.id == spec_id)?;
        let route = self
            .ide_panel
            .api
            .models
            .get(&spec_id)?
            .routes
            .get(route_idx)?;
        let source_key = crate::app::api_mock::types::api_mock_source_key(entry);
        self.ide_panel.api.mock.route_overrides.iter().find(|item| {
            item.source_key == source_key && item.method == route.method && item.path == route.path
        })
    }

    fn api_route_override_mut(
        &mut self,
        route_idx: usize,
    ) -> Option<&mut crate::app::api_mock::types::ApiMockRouteOverride> {
        let (meta, _) = self.active_api_tab()?;
        let spec_id = meta.spec_id;
        let entry = self
            .ide_panel
            .api
            .specs
            .iter()
            .find(|entry| entry.id == spec_id)
            .cloned()?;
        let route = self
            .ide_panel
            .api
            .models
            .get(&spec_id)?
            .routes
            .get(route_idx)
            .cloned()?;
        let source_key = crate::app::api_mock::types::api_mock_source_key(&entry);
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
    }

    fn ensure_api_route_override(&mut self, route_idx: usize) {
        if self.api_route_override(route_idx).is_some() {
            return;
        }
        self.add_api_route_override(route_idx, false);
    }

    fn add_api_route_override(&mut self, route_idx: usize, enabled: bool) {
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
        self.ide_panel.api.mock.route_overrides.push(
            crate::app::api_mock::types::ApiMockRouteOverride {
                source_key: crate::app::api_mock::types::api_mock_source_key(&entry),
                method: route.method,
                path: route.path,
                enabled,
                response: crate::app::api_mock::types::ApiMockResponse::Generated,
                python: None,
                extra_input_fields: Vec::new(),
                extra_output_fields: Vec::new(),
            },
        );
    }

    fn api_route_python_script(
        &self,
        route_idx: usize,
    ) -> Option<&crate::app::api_mock::types::ApiMockPythonScript> {
        let (meta, _) = self.active_api_tab()?;
        let spec_id = meta.spec_id;
        let entry = self
            .ide_panel
            .api
            .specs
            .iter()
            .find(|entry| entry.id == spec_id)?;
        let route = self
            .ide_panel
            .api
            .models
            .get(&spec_id)?
            .routes
            .get(route_idx)?;
        let source_key = crate::app::api_mock::types::api_mock_source_key(entry);
        self.ide_panel
            .api
            .mock
            .route_overrides
            .iter()
            .find_map(|item| {
                (item.source_key == source_key
                    && item.method == route.method
                    && item.path == route.path)
                .then_some(item.python.as_ref().filter(|script| script.enabled))
                .flatten()
            })
    }

    fn api_route_python_script_mut(
        &mut self,
        route_idx: usize,
    ) -> Option<&mut crate::app::api_mock::types::ApiMockPythonScript> {
        let (meta, _) = self.active_api_tab()?;
        let spec_id = meta.spec_id;
        let entry = self
            .ide_panel
            .api
            .specs
            .iter()
            .find(|entry| entry.id == spec_id)
            .cloned()?;
        let route = self
            .ide_panel
            .api
            .models
            .get(&spec_id)?
            .routes
            .get(route_idx)
            .cloned()?;
        let source_key = crate::app::api_mock::types::api_mock_source_key(&entry);
        self.ide_panel
            .api
            .mock
            .route_overrides
            .iter_mut()
            .find_map(|item| {
                (item.source_key == source_key
                    && item.method == route.method
                    && item.path == route.path)
                .then_some(item.python.as_mut().filter(|script| script.enabled))
                .flatten()
            })
    }

    fn api_mock_python_focus_target(&self) -> Option<(usize, ApiMockSourcePart)> {
        match self.ide_panel.api.focused {
            Some(ApiFocus::MockPrelude { route_idx }) => {
                Some((route_idx, ApiMockSourcePart::Prelude))
            }
            Some(ApiFocus::MockBody { route_idx }) => Some((route_idx, ApiMockSourcePart::Body)),
            _ => None,
        }
    }

    fn api_mock_editor_key_for_focus(focus: &ApiFocus) -> Option<(usize, ApiMockSourcePart)> {
        match focus {
            ApiFocus::MockPrelude { route_idx } => Some((*route_idx, ApiMockSourcePart::Prelude)),
            ApiFocus::MockBody { route_idx } => Some((*route_idx, ApiMockSourcePart::Body)),
            _ => None,
        }
    }

    fn stash_active_api_mock_editor(&mut self) {
        let Some(key) = self
            .ide_panel
            .api
            .focused
            .as_ref()
            .and_then(Self::api_mock_editor_key_for_focus)
        else {
            return;
        };
        let editor = std::mem::replace(&mut self.ide_panel.api.input_editor, Editor::new(512));
        self.ide_panel.api.mock_python_editors.insert(key, editor);
    }

    fn api_mock_route_context(
        &self,
        route_idx: usize,
    ) -> Option<(ApiMethod, String, ApiRouteRow, ApiSpecModel)> {
        let (meta, _) = self.active_api_tab()?;
        let spec_id = meta.spec_id;
        let model = self.ide_panel.api.models.get(&spec_id)?.clone();
        let route = model.routes.get(route_idx)?.clone();
        Some((route.method, route.path.clone(), route, model))
    }

    fn api_mock_signature_for_route(&self, route_idx: usize) -> Option<String> {
        let (_, path, _, _) = self.api_mock_route_context(route_idx)?;
        let mut out = String::from("def handler(\n    req: Request,");
        for name in api_mock_path_param_names(&path) {
            out.push_str("\n    ");
            out.push_str(&api_mock_sanitize_python_param(&name));
            out.push_str(": str,");
        }
        out.push_str("\n    query: Query,\n    body: Body | None,\n    fields: Fields,\n) -> dict[str, Any]:");
        Some(out)
    }

    fn api_mock_script_for_tools(
        &self,
        route_idx: usize,
    ) -> Option<crate::app::api_mock::types::ApiMockPythonScript> {
        let mut script = self.api_route_python_script(route_idx)?.clone();
        script.body = api_mock_body_editor_text(&script.body);
        if let Some((focused_route, part)) = self.api_mock_python_focus_target()
            && focused_route == route_idx
        {
            let text = self.ide_panel.api.input_editor.get_full_text();
            match part {
                ApiMockSourcePart::Prelude => script.prelude = text,
                ApiMockSourcePart::Signature => {}
                ApiMockSourcePart::Body => script.body = text,
            }
        }
        Some(script)
    }

    fn api_mock_edit_text_for_part(
        &self,
        route_idx: usize,
        part: ApiMockSourcePart,
        script: &crate::app::api_mock::types::ApiMockPythonScript,
    ) -> String {
        if self.api_mock_python_focus_target() == Some((route_idx, part)) {
            self.ide_panel.api.input_editor.get_full_text()
        } else {
            match part {
                ApiMockSourcePart::Prelude => script.prelude.clone(),
                ApiMockSourcePart::Signature => {
                    self.api_mock_signature_for_route(route_idx).unwrap_or_default()
                }
                ApiMockSourcePart::Body => script.body.clone(),
            }
        }
    }

    fn api_mock_virtual_path(route_idx: usize) -> PathBuf {
        std::env::temp_dir().join(format!("rriter_api_mock_route_{route_idx}.py"))
    }

    fn map_api_mock_spans_to_edit(
        spans: &[ColorSpan],
        virtual_source: &crate::app::api_mock::ty_check::ApiMockVirtualSource,
        part: ApiMockSourcePart,
    ) -> Vec<ColorSpan> {
        let mut out = Vec::with_capacity(spans.len().min(128));
        for span in spans {
            match part {
                ApiMockSourcePart::Prelude => {
                    let start = span.start.max(virtual_source.prelude_start);
                    let end = span.end.min(virtual_source.prelude_end);
                    if start < end {
                        out.push(ColorSpan {
                            start: start - virtual_source.prelude_start,
                            end: end - virtual_source.prelude_start,
                            color: span.color,
                        });
                    }
                }
                ApiMockSourcePart::Signature => {
                    let start = span.start.max(virtual_source.signature_start);
                    let end = span.end.min(virtual_source.signature_end);
                    if start < end {
                        out.push(ColorSpan {
                            start: start - virtual_source.signature_start,
                            end: end - virtual_source.signature_start,
                            color: span.color,
                        });
                    }
                }
                ApiMockSourcePart::Body => {
                    for line in &virtual_source.body_lines {
                        let start = span.start.max(line.source_start);
                        let end = span.end.min(line.source_end);
                        if start < end {
                            out.push(ColorSpan {
                                start: line.edit_start + start - line.source_start,
                                end: line.edit_start + end - line.source_start,
                                color: span.color,
                            });
                        }
                    }
                }
            }
        }
        out
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
        let edit_cursor = if self.api_mock_python_focus_target() == Some((route_idx, part)) {
            self.ide_panel.api.input_editor.cursor
        } else {
            0
        };
        let source_cursor = virtual_source.edit_offset_to_source(
            part,
            &edit_text,
            edit_cursor,
        );
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
        if self.ide_panel.api.mock_highlighter.sync_highlight_after_edit(
            version,
            None,
            None,
            None,
            None,
            Duration::from_millis(4),
        ) {
            let spans = self.ide_panel.api.mock_highlighter.spans.clone();
            for cache_part in [
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
        }
    }

    fn queue_api_mock_python_tools(&mut self, route_idx: usize) {
        if let Some((focused_route, part)) = self.api_mock_python_focus_target()
            && focused_route == route_idx
        {
            self.refresh_api_mock_python_highlight(route_idx, part);
            self.ide_panel.api.mock_ty_due = Some(Instant::now() + Duration::from_millis(450));
        }
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
        if self
            .ide_panel
            .api
            .mock_highlight_target
            .is_some_and(|(highlight_route, _, _)| highlight_route == route_idx)
        {
            return false;
        }
        let missing_cache = [
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
            override_route.enabled = !override_route.enabled;
        } else {
            self.ide_panel.api.mock.route_overrides.push(
                crate::app::api_mock::types::ApiMockRouteOverride {
                    source_key,
                    method: route.method,
                    path: route.path,
                    enabled: true,
                    response: crate::app::api_mock::types::ApiMockResponse::Generated,
                    python: None,
                    extra_input_fields: Vec::new(),
                    extra_output_fields: Vec::new(),
                },
            );
        }
        self.ide_panel.api.persist();
    }

    pub fn toggle_api_route_python(&mut self, route_idx: usize) {
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
        let focused_this_route = self
            .api_mock_python_focus_target()
            .is_some_and(|(focused_route, _)| focused_route == route_idx);
        let mut disabled_active_script = false;
        if let Some(override_route) = self.ide_panel.api.mock.route_overrides.get_mut(idx) {
            if let Some(script) = override_route.python.as_mut() {
                script.enabled = !script.enabled;
                disabled_active_script = !script.enabled;
            } else {
                override_route.python = Some(default_api_mock_python_script());
            }
        }
        if disabled_active_script && focused_this_route {
            self.stash_active_api_mock_editor();
            self.ide_panel.api.focused = None;
        }
        self.ide_panel.api.persist();
    }

    pub fn reset_api_route_python_part(&mut self, route_idx: usize, part: ApiMockSourcePart) {
        self.commit_api_focus();
        let Some(script) = self.api_route_python_script_mut(route_idx) else {
            return;
        };
        match part {
            ApiMockSourcePart::Prelude => script.prelude.clear(),
            ApiMockSourcePart::Signature => return,
            ApiMockSourcePart::Body => script.body = default_api_mock_python_body(),
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
        if self.api_mock_python_focus_target() == Some((route_idx, part)) {
            let text = match part {
                ApiMockSourcePart::Prelude | ApiMockSourcePart::Signature => String::new(),
                ApiMockSourcePart::Body => default_api_mock_python_body(),
            };
            let old_version = self.ide_panel.api.input_editor.version;
            self.ide_panel.api.input_editor.set_text_clean(&text);
            self.ide_panel.api.input_editor.version = old_version.saturating_add(1);
            self.ide_panel.api.input_editor.cursor = self.ide_panel.api.input_editor.len();
            self.ide_panel.api.input_editor.selection_anchor =
                Some(self.ide_panel.api.input_editor.cursor);
        }
        self.ide_panel.api.persist();
        self.queue_api_mock_python_tools(route_idx);
    }

    pub fn add_api_manual_route(&mut self) {
        self.commit_api_focus();
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
        self.ide_panel.api.persist();
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
        self.api_mock_ty_rx = Some(spawn_api_mock_ty_check(
            route_idx,
            version,
            method,
            path,
            route,
            model,
            script,
        ));
    }

    pub(crate) fn api_mock_completion_focus(&self) -> Option<(usize, ApiMockSourcePart)> {
        self.api_mock_python_focus_target()
    }

    pub(crate) fn api_input_current_word_prefix(&self) -> String {
        let editor = &self.ide_panel.api.input_editor;
        let mut p = editor.cursor;
        while p > 0 {
            let b = editor.byte_at(p - 1);
            if !(b.is_ascii_alphanumeric() || b == b'_') {
                break;
            }
            p -= 1;
        }
        if p == editor.cursor {
            return String::new();
        }
        let mut out = Vec::with_capacity(editor.cursor - p);
        for i in p..editor.cursor {
            out.push(editor.byte_at(i));
        }
        String::from_utf8(out).unwrap_or_default()
    }

    fn api_input_after_python_member_dot(&self) -> bool {
        let text = self.ide_panel.api.input_editor.get_full_text();
        let cursor = self.ide_panel.api.input_editor.cursor.min(text.len());
        let line_start = text[..cursor].rfind('\n').map(|idx| idx + 1).unwrap_or(0);
        let prefix = &text[line_start..cursor];
        let bytes = prefix.as_bytes();
        let mut idx = bytes.len();
        while idx > 0 && (bytes[idx - 1].is_ascii_alphanumeric() || bytes[idx - 1] == b'_') {
            idx -= 1;
        }
        idx >= 2 && bytes.get(idx - 1) == Some(&b'.')
            && bytes
                .get(idx - 2)
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
    }

    fn api_mock_completion_indices(prefix: &str, word: &str) -> Option<Vec<usize>> {
        if prefix.is_empty() {
            return Some(Vec::new());
        }
        let prefix = prefix.to_lowercase();
        let word_lower = word.to_lowercase();
        if let Some(start) = word_lower.find(&prefix) {
            return Some((start..start + prefix.len()).collect());
        }
        None
    }

    fn api_mock_autocomplete_anchor(&mut self) -> Option<(f32, f32)> {
        let focus = self.ide_panel.api.focused.as_ref()?;
        let (id, multiline) = self.api_focus_ui_target(focus)?;
        let rect = self.ui_registry.rect_for(id)?;
        let renderer = self.renderer.as_mut()?;
        let scale = renderer.scale_factor;
        let text = self.ide_panel.api.input_editor.get_full_text();
        let cursor = self.ide_panel.api.input_editor.cursor.min(text.len());
        let line_start = text[..cursor].rfind('\n').map(|idx| idx + 1).unwrap_or(0);
        let line_idx = text[..line_start].bytes().filter(|b| *b == b'\n').count();
        let x = rect.0
            + 10.0 * scale
            + renderer.measure_ui_width(&text[line_start..cursor], API_BODY_TEXT_SCALE);
        let y = if multiline {
            rect.1 + 10.0 * scale + line_idx as f32 * api_text_area_line_height(scale)
        } else {
            rect.1 + rect.3 * 0.55
        };
        Some((x, y))
    }

    pub(crate) fn request_api_mock_ty_autocomplete(&mut self, trigger: Option<&str>) {
        let Some((route_idx, part)) = self.api_mock_python_focus_target() else {
            return;
        };
        let prefix = self.api_input_current_word_prefix();
        if trigger.is_none() && prefix.is_empty() && !self.api_input_after_python_member_dot() {
            self.close_autocomplete();
            return;
        }
        let Some((method, path, route, model)) = self.api_mock_route_context(route_idx) else {
            return;
        };
        let Some(script) = self.api_mock_script_for_tools(route_idx) else {
            return;
        };
        let virtual_source = build_api_mock_virtual_source(method, &path, &route, &model, &script);
        let edit_text = self.ide_panel.api.input_editor.get_full_text();
        let source_cursor = virtual_source.edit_offset_to_source(
            part,
            &edit_text,
            self.ide_panel.api.input_editor.cursor,
        );
        let mut line_offsets = vec![0usize];
        for (idx, b) in virtual_source.source.bytes().enumerate() {
            if b == b'\n' {
                line_offsets.push(idx + 1);
            }
        }
        let (line, col) =
            crate::lsp::offset_to_lsp_pos(&virtual_source.source, source_cursor, &line_offsets);
        let path = Self::api_mock_virtual_path(route_idx);
        let Some(lsp) = self.lsp.as_mut() else {
            return;
        };
        lsp.notify_change(
            &path,
            "py",
            &virtual_source.source,
            self.ide_panel.api.input_editor.version as i32,
        );
        if let Some(id) = lsp.request_ty_completion(&path, "py", line, col, trigger) {
            self.autocomplete_mode = crate::app::AutocompleteMode::TyContext;
            self.autocomplete_pending_request_id = Some(id);
            self.autocomplete_pending_request_mode = None;
            self.autocomplete_pending_request_path = None;
            self.autocomplete_pending_context_key = None;
            self.autocomplete_apply_pending_response = false;
            if !self.autocomplete_active {
                self.autocomplete_anim_progress = 0.0;
            }
            self.autocomplete_anchor = self.api_mock_autocomplete_anchor();
            self.autocomplete_detail_popup = None;
            self.autocomplete_detail_rect = None;
        }
    }

    pub(crate) fn update_api_mock_ty_autocomplete(
        &mut self,
        items: Vec<crate::lsp::LspCompletionItem>,
    ) {
        if self.api_mock_python_focus_target().is_none() {
            return;
        }
        let prefix = self.api_input_current_word_prefix();
        if prefix.is_empty() && !self.api_input_after_python_member_dot() {
            self.close_autocomplete();
            return;
        }
        let prefix_lower = prefix.to_lowercase();
        let mut out = Vec::new();
        for item in items.into_iter().take(120) {
            let item: crate::app::AutocompleteItem = item.into();
            if prefix_lower.is_empty() || item.word.to_lowercase().contains(&prefix_lower) {
                if let Some(indices) = Self::api_mock_completion_indices(&prefix, &item.word) {
                    out.push((item, indices));
                }
            }
        }
        out.sort_unstable_by_key(|(item, _)| {
            let lower = item.word.to_lowercase();
            (
                !lower.starts_with(&prefix_lower),
                matches!(item.kind, crate::highlighter::SymbolKind::Unknown),
                item.word.len(),
            )
        });
        out.truncate(60);
        self.autocomplete_options = out;
        self.autocomplete_active = !self.autocomplete_options.is_empty();
        if !self.autocomplete_active {
            self.autocomplete_detail_popup = None;
            self.autocomplete_detail_rect = None;
            return;
        }
        self.autocomplete_mode = crate::app::AutocompleteMode::TyContext;
        self.autocomplete_selected_idx = 0;
        self.autocomplete_hovered_idx = None;
        self.autocomplete_scroll.current = 0.0;
        self.autocomplete_scroll.target = 0.0;
        self.autocomplete_anchor = self.api_mock_autocomplete_anchor();
    }

    pub(crate) fn apply_api_mock_autocomplete(&mut self) -> bool {
        let Some((route_idx, _)) = self.api_mock_python_focus_target() else {
            return false;
        };
        if !self.autocomplete_active || self.autocomplete_options.is_empty() {
            return true;
        }
        let item = self.autocomplete_options[self.autocomplete_selected_idx]
            .0
            .clone();
        let selected = item
            .insert_text
            .as_deref()
            .or_else(|| item.text_edit.as_ref().map(|edit| edit.new_text.as_str()))
            .unwrap_or(&item.word)
            .to_string();
        let prefix_len = self.api_input_current_word_prefix().len();
        for _ in 0..prefix_len {
            self.ide_panel.api.input_editor.backspace();
        }
        let _ = self.ide_panel.api.input_editor.insert_str(&selected);
        if !item.additional_text_edits.is_empty()
            && let Some(script) = self.api_route_python_script_mut(route_idx)
        {
            for edit in item.additional_text_edits {
                let text = edit.new_text.trim_matches(|c| c == '\n' || c == '\r');
                if text.starts_with("import ") || text.starts_with("from ") {
                    if !script.prelude.trim().is_empty() && !script.prelude.ends_with('\n') {
                        script.prelude.push('\n');
                    }
                    script.prelude.push_str(text);
                    script.prelude.push('\n');
                }
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
