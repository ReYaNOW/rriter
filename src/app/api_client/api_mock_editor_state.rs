#[cfg(test)]
fn api_mock_hover_content_y_at_point(
    my: f32,
    top_y: f32,
    scroll_y: f32,
    line_h: f32,
) -> Option<f32> {
    crate::app::mouse::embedded_editor_hover_content_y_at_point(my, top_y, scroll_y, line_h)
}

fn api_mock_import_text(text: &str) -> Option<&str> {
    let text = text.trim_matches(|c| c == '\n' || c == '\r');
    (text.starts_with("import ") || text.starts_with("from ")).then_some(text)
}

fn api_mock_lsp_edit_to_input_op(
    virtual_source: &crate::app::api_mock::ty_check::ApiMockVirtualSource,
    part: ApiMockSourcePart,
    source: &str,
    edit: &crate::lsp::TextChange,
) -> Option<crate::app::CompletionTextEditOp> {
    let start = crate::lsp::lsp_pos_to_offset(source, edit.start_line, edit.start_col);
    let end = crate::lsp::lsp_pos_to_offset(source, edit.end_line, edit.end_col);
    let start = virtual_source.source_offset_to_edit(part, start)?;
    let end = virtual_source.source_offset_to_edit(part, end)?;
    (start <= end).then(|| crate::app::CompletionTextEditOp {
        start,
        end,
        new_text: edit.new_text.clone(),
    })
}

impl ApiClientState {
    pub(crate) fn set_api_mock_hover_request(&mut self, request: ApiMockHoverRequest) {
        self.mock_hover_request = Some(request);
    }

    pub(crate) fn take_api_mock_hover_request(
        &mut self,
        request_id: i32,
    ) -> Option<ApiMockHoverRequest> {
        if self
            .mock_hover_request
            .as_ref()
            .is_some_and(|request| request.request_id == request_id)
        {
            self.mock_hover_request.take()
        } else {
            None
        }
    }

    pub(crate) fn accept_api_mock_hover_target(
        &mut self,
        target: ApiMockHoverTarget,
        clear_request: bool,
    ) {
        self.mock_hover_target = Some(target);
        if clear_request {
            self.mock_hover_request = None;
        }
    }

    pub(crate) fn apply_api_mock_completion(
        &mut self,
        active: Option<&ApiActiveRoute>,
        route_idx: usize,
        part: ApiMockSourcePart,
        edit_text: &str,
        plan: crate::app::CompletionApplyPlan,
        prelude_imports: Vec<String>,
    ) {
        crate::app::apply_completion_plan_to_editor(&mut self.input_editor, plan);
        if prelude_imports.is_empty() {
            return;
        }
        if part == ApiMockSourcePart::Prelude {
            let cursor_after_apply = self.input_editor.cursor;
            let mut insert = String::new();
            if !edit_text.trim().is_empty() && !edit_text.ends_with('\n') {
                insert.push('\n');
            }
            for text in prelude_imports {
                insert.push_str(&text);
                insert.push('\n');
            }
            let end = self.input_editor.len();
            let _ = self.input_editor.replace_range(end, end, &insert);
            self.input_editor.cursor = cursor_after_apply.min(self.input_editor.len());
            self.input_editor.selection_anchor = None;
        } else if let Some(script) = self.api_route_python_script_mut(active, route_idx) {
            for text in prelude_imports {
                if !script.prelude.trim().is_empty() && !script.prelude.ends_with('\n') {
                    script.prelude.push('\n');
                }
                script.prelude.push_str(&text);
                script.prelude.push('\n');
            }
            self.commit_mock_config();
        }
    }

    fn previous_api_mock_python_focus(
        route_idx: usize,
        part: ApiMockSourcePart,
    ) -> Option<ApiFocus> {
        match part {
            ApiMockSourcePart::Contract => Some(ApiFocus::MockPrelude { route_idx }),
            ApiMockSourcePart::Body => Some(ApiFocus::MockContract { route_idx }),
            ApiMockSourcePart::Prelude | ApiMockSourcePart::Signature => None,
        }
    }

    fn api_mock_autocomplete_anchor_for_text(
        id: crate::ui_system::UiId,
        rect: (f32, f32, f32, f32),
        scale: f32,
        text: &str,
        cursor: usize,
        scroll_x: f32,
        mut measure_line_prefix: impl FnMut(&str) -> f32,
    ) -> (f32, f32) {
        let cursor = cursor.min(text.len());
        let line_start = text[..cursor].rfind('\n').map(|idx| idx + 1).unwrap_or(0);
        let line_idx = text[..line_start].bytes().filter(|b| *b == b'\n').count();
        let x = Self::api_multiline_cursor_left_x(id, rect, scale)
            + measure_line_prefix(&text[line_start..cursor])
            - scroll_x;
        let y = Self::api_multiline_cursor_top_y(id, rect, scale)
            + line_idx as f32 * api_text_area_line_height(scale)
            + api_text_area_baseline_offset(scale);
        (x, y)
    }

    fn api_mock_python_focus_target(&self) -> Option<(usize, ApiMockSourcePart)> {
        match self.focused {
            Some(ApiFocus::MockContract { route_idx }) => {
                Some((route_idx, ApiMockSourcePart::Contract))
            }
            Some(ApiFocus::MockPrelude { route_idx }) => {
                Some((route_idx, ApiMockSourcePart::Prelude))
            }
            Some(ApiFocus::MockBody { route_idx }) => Some((route_idx, ApiMockSourcePart::Body)),
            _ => None,
        }
    }

    fn api_mock_hover_editor(
        &self,
        route_idx: usize,
        part: ApiMockSourcePart,
    ) -> Option<&Editor> {
        if self.api_mock_python_focus_target() == Some((route_idx, part)) {
            Some(&self.input_editor)
        } else {
            self.mock_python_editors.get(&(route_idx, part))
        }
    }

    fn api_mock_editor_key_for_focus(focus: &ApiFocus) -> Option<(usize, ApiMockSourcePart)> {
        match focus {
            ApiFocus::MockContract { route_idx } => Some((*route_idx, ApiMockSourcePart::Contract)),
            ApiFocus::MockPrelude { route_idx } => Some((*route_idx, ApiMockSourcePart::Prelude)),
            ApiFocus::MockBody { route_idx } => Some((*route_idx, ApiMockSourcePart::Body)),
            _ => None,
        }
    }
    fn stash_active_api_mock_editor(&mut self) {
        let Some(key) = self
            .focused
            .as_ref()
            .and_then(Self::api_mock_editor_key_for_focus)
        else {
            return;
        };
        let editor = std::mem::replace(&mut self.input_editor, Editor::new(512));
        self.mock_python_editors.insert(key, editor);
    }
    pub(crate) fn api_mock_completion_focus(&self) -> Option<(usize, ApiMockSourcePart)> {
        self.api_mock_python_focus_target()
    }

    fn map_api_mock_spans_to_edit(
        spans: &[ColorSpan],
        virtual_source: &crate::app::api_mock::ty_check::ApiMockVirtualSource,
        part: ApiMockSourcePart,
    ) -> Vec<ColorSpan> {
        let mut out = Vec::with_capacity(spans.len().min(128));
        for span in spans {
            match part {
                ApiMockSourcePart::Contract => {
                    let start = span.start.max(virtual_source.contract_start);
                    let end = span.end.min(virtual_source.contract_end);
                    if start < end
                        && let Some((start, end)) =
                            virtual_source.contract_source_span_to_edit(start, end)
                    {
                        out.push(ColorSpan {
                            start,
                            end,
                            color: span.color,
                        });
                    }
                }
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
    pub(crate) fn refresh_api_mock_highlight_cache_for_spans(
        &mut self,
        route_idx: usize,
        spans: &[ColorSpan],
        virtual_source: &crate::app::api_mock::ty_check::ApiMockVirtualSource,
    ) {
        for cache_part in [
            ApiMockSourcePart::Contract,
            ApiMockSourcePart::Prelude,
            ApiMockSourcePart::Signature,
            ApiMockSourcePart::Body,
        ] {
            let edit_spans = Self::map_api_mock_spans_to_edit(spans, virtual_source, cache_part);
            self.mock_highlight_cache
                .insert((route_idx, cache_part), edit_spans);
        }
    }

    /// Drops the API mock hover target and request. The editor hover popup lives in
    /// `App::hover`, out of reach here, so the old target is parked in
    /// `released_mock_hover_target`; `App::release_api_mock_hover` clears the popup it owns.
    fn reset_api_mock_hover_tracking(&mut self) {
        let old_target = self.mock_hover_target.take();
        self.mock_hover_request = None;
        if old_target.is_some() {
            self.released_mock_hover_target = old_target;
        }
    }

    /// Clears `state` when it still shows the hover of `target` (a released API mock target).
    pub(crate) fn release_api_mock_hover_state(
        state: &mut crate::app::mouse::HoverState,
        target: &ApiMockHoverTarget,
    ) {
        let owns_hover = state.byte_offset == Some(target.edit_byte)
            || state
                .popup
                .as_ref()
                .is_some_and(|popup| popup.byte_offset == target.edit_byte)
            || state
                .pending_popup
                .as_ref()
                .is_some_and(|popup| popup.byte_offset == target.edit_byte)
            || state.hovered_diag_type_target == Some(target.edit_byte)
            || state.popup_diag_type_target == Some(target.edit_byte);
        if owns_hover {
            state.request_id = None;
            state.definition_request_id = None;
            state.popup = None;
            state.pending_popup = None;
            state.timer = 0.0;
            state.byte_offset = None;
            state.rect = None;
            state.max_scroll = 0.0;
            state.selection_anchor = None;
            state.selection_cursor = None;
            state.selecting = false;
            state.reset_diagnostic_popup();
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn with_api_mock_hover_renderer_context<R>(
        renderer: &mut crate::renderer::Renderer,
        editor: &Editor,
        left_x: f32,
        scroll_x: f32,
        scale: f32,
        f: impl FnOnce(&mut crate::renderer::Renderer) -> R,
    ) -> R {
        crate::app::mouse::with_embedded_editor_hover_renderer_context(
            renderer,
            editor,
            left_x,
            scroll_x,
            api_text_area_line_height(scale),
            f,
        )
    }

    pub(crate) fn api_mock_virtual_path_for(
        spec_id: crate::app::api_client::ApiSpecId,
        route_idx: usize,
    ) -> PathBuf {
        std::env::temp_dir().join(format!(
            "rriter_api_mock_spec_{}_route_{}.py",
            spec_id.0, route_idx
        ))
    }
    fn api_mock_module_segment(text: &str, fallback: &str) -> String {
        let mut out = String::with_capacity(text.len().max(fallback.len()));
        let mut last_sep = false;
        for ch in text.chars() {
            if ch.is_ascii_alphanumeric() {
                out.push(ch.to_ascii_lowercase());
                last_sep = false;
            } else if !last_sep && !out.is_empty() {
                out.push('_');
                last_sep = true;
            }
        }
        while out.ends_with('_') {
            out.pop();
        }
        if out.is_empty() {
            out.push_str(fallback);
        }
        if out
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_digit())
        {
            out.insert(0, '_');
        }
        out
    }
    fn api_mock_text_baseline_y(
        id: crate::ui_system::UiId,
        rect: (f32, f32, f32, f32),
        s: f32,
    ) -> f32 {
        match id {
            crate::ui_system::UiId::ApiMockSignatureInput(_) => {
                rect.1 + api_text_area_baseline_offset(s)
            }
            _ => rect.1 + 29.0 * s,
        }
    }
    fn api_mock_ui_for_part(route_idx: usize, part: ApiMockSourcePart) -> crate::ui_system::UiId {
        match part {
            ApiMockSourcePart::Contract => crate::ui_system::UiId::ApiMockContractInput(route_idx),
            ApiMockSourcePart::Prelude => crate::ui_system::UiId::ApiMockPreludeInput(route_idx),
            ApiMockSourcePart::Signature => {
                crate::ui_system::UiId::ApiMockSignatureInput(route_idx)
            }
            ApiMockSourcePart::Body => crate::ui_system::UiId::ApiMockBodyInput(route_idx),
        }
    }
    fn api_mock_hover_byte_at_point(
        editor: &Editor,
        renderer: &mut crate::renderer::Renderer,
        left_x: f32,
        top_y: f32,
        mx: f32,
        my: f32,
        scale: f32,
        scroll_y: f32,
        scroll_x: f32,
    ) -> Option<usize> {
        crate::app::mouse::embedded_editor_hover_byte_at_point(
            editor,
            renderer,
            left_x,
            top_y,
            mx,
            my,
            api_text_area_line_height(scale),
            scroll_y,
            scroll_x,
        )
    }
    fn api_mock_ty_diag_hover_at_point(
        renderer: &mut crate::renderer::Renderer,
        text: &str,
        diagnostics: &[ApiMockTyDiagnostic],
        part: ApiMockSourcePart,
        rect: (f32, f32, f32, f32),
        text_x: f32,
        text_y: f32,
        scale: f32,
        scroll_y: f32,
        scroll_x: f32,
        mx: f32,
        my: f32,
    ) -> Option<(crate::app::mouse::HoveredDiagnostic, usize)> {
        for (diag_idx, diag) in diagnostics.iter().enumerate() {
            let Some(layout) = api_mock_ty_diag_layout(
                text,
                diag,
                part,
                text_x,
                text_y,
                rect.2,
                rect.3,
                scale,
                scroll_y,
                scroll_x,
                |prefix| {
                    prefix
                        .chars()
                        .map(|ch| renderer.char_advance(ch))
                        .sum::<f32>()
                },
            ) else {
                continue;
            };
            if mx >= layout.x_start
                && mx <= layout.x_start + layout.squiggle_w
                && crate::app::mouse::hover_content_y_in_line_hitbox(
                    my,
                    layout.line_top,
                    layout.line_h,
                )
            {
                return Some((
                    (
                        diag_idx,
                        layout.x_start,
                        layout.line_top,
                        layout.line_top + layout.line_h,
                        layout.x_start + layout.squiggle_w,
                    ),
                    layout.byte_offset,
                ));
            }
        }
        None
    }
}
