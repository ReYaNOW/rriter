fn api_mock_ty_diagnostics_as_lsp(
    diagnostics: &[crate::app::api_mock::ty_check::ApiMockTyDiagnostic],
) -> Vec<crate::lsp::Diagnostic> {
    diagnostics
        .iter()
        .map(|diag| crate::lsp::Diagnostic {
            start_line: diag.line as u32,
            start_col: diag.start_col as u32,
            end_line: diag.line as u32,
            end_col: diag.end_col as u32,
            severity: crate::lsp::DiagSeverity::Error,
            code: None,
            code_href: None,
            message: diag.message.clone(),
            source: Some("ty".to_string()),
            quickfixes: Vec::new(),
            tags: Vec::new(),
            spans: Vec::new(),
        })
        .collect()
}

impl Renderer {
    fn draw_api_mock_locked_signature_line(
        &mut self,
        signature: &str,
        block_h: f32,
        spans: &[crate::highlighter::ColorSpan],
        x: f32,
        y: f32,
        w: f32,
        s: f32,
    ) {
        let line_h = api_text_area_line_height(s).round();
        self.draw_python_text_area(
            signature,
            spans,
            x,
            y + api_text_area_baseline_offset(s),
            w,
            block_h,
            s,
            0.0,
            0.0,
        );
        let sep_y = y + signature.split('\n').count() as f32 * line_h + 2.0 * s;
        self.push_rect(x, sep_y.round(), w, 1.0, [1.0, 1.0, 1.0, 0.08]);
    }

    fn draw_api_line_number_gutter(&mut self, x: f32, y: f32, w: f32, h: f32, s: f32) {
        let bg = [
            (self.theme.bg[0] + 0.018).min(1.0),
            (self.theme.bg[1] + 0.018).min(1.0),
            (self.theme.bg[2] + 0.022).min(1.0),
            1.0,
        ];
        self.push_rect(x, y, w, h, bg);
        self.push_rect(
            (x + w).round(),
            y,
            1.0_f32.max(s.round()),
            h.max(0.0),
            [1.0, 1.0, 1.0, 0.10],
        );
    }

    fn draw_api_editor_line_numbers(
        &mut self,
        text: &str,
        x: f32,
        w: f32,
        y: f32,
        h: f32,
        s: f32,
        scroll_y: f32,
        first_line_no: usize,
    ) {
        let line_h = api_text_area_line_height(s).round();
        let first_line = (scroll_y / line_h).floor() as usize;
        let line_offset = scroll_y - first_line as f32 * line_h;
        let max_lines = (h / line_h).ceil().max(1.0) as usize + 1;
        let start_y = y - line_offset;
        let viewport_h = self.height.max(1.0);
        let first_visible = ((0.0 - start_y) / line_h).floor().max(0.0) as usize;
        let last_visible = ((viewport_h - start_y) / line_h).ceil().max(0.0) as usize + 1;
        let line_count = text.split('\n').count();
        for visible_idx in first_visible..last_visible.min(max_lines) {
            let line_idx = first_line + visible_idx;
            if line_idx >= line_count {
                break;
            }
            let text_y = (y - line_offset + visible_idx as f32 * line_h).round();
            self.draw_editor_line_number_centered(first_line_no + line_idx, x, w, text_y, 1.0);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_api_mock_ty_squiggles(
        &mut self,
        text: &str,
        diagnostics: &[crate::app::api_mock::ty_check::ApiMockTyDiagnostic],
        part: ApiMockSourcePart,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        scroll_y: f32,
        scroll_x: f32,
    ) {
        for diag in diagnostics {
            if let Some(layout) = crate::app::api_client::api_mock_ty_diag_layout(
                text,
                diag,
                part,
                x,
                y,
                w,
                h,
                s,
                scroll_y,
                scroll_x,
                |prefix| self.api_mono_width(prefix),
            ) {
                self.push_squiggle(
                    layout.x_start,
                    layout.squiggle_y,
                    layout.squiggle_w,
                    [1.0, 0.36, 0.36, 1.0],
                );
            }
        }
    }

    fn draw_existing_api_mock_ty_popup(
        &mut self,
        source_editor: Option<&crate::editor::Editor>,
        diagnostics: &[crate::app::api_mock::ty_check::ApiMockTyDiagnostic],
        text_x: f32,
        source_top_y: f32,
        scroll_y: f32,
        scroll_x: f32,
        ide_panel: &crate::app::IdePanelState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        clip_rect: Option<(f32, f32, f32, f32)>,
        mx: f32,
        my: f32,
    ) -> bool {
        let Some(editor) = source_editor else {
            return false;
        };
        let should_draw = crate::app::mouse::HOVER_STATE.with(|state| {
            let state = state.borrow();
            let target_matches = ide_panel.api.mock_hover_target.as_ref().is_some_and(|target| {
                state.byte_offset == Some(target.edit_byte)
                    || state
                        .popup
                        .as_ref()
                        .is_some_and(|popup| popup.byte_offset == target.edit_byte)
                    || state
                        .pending_popup
                        .as_ref()
                        .is_some_and(|popup| popup.byte_offset == target.edit_byte)
                    || state.combined_type_target() == Some(target.edit_byte)
            });
            target_matches
                && (state.popup.is_some()
                    || !state.diagnostic_popup_cache_is_empty()
                    || state.rect.is_none()
                    || state
                        .popup_safe_area_contains(mx, my, self.width, self.scale_factor)
                        .0)
        });
        if !should_draw {
            return false;
        }
        let lsp_diagnostics = api_mock_ty_diagnostics_as_lsp(diagnostics);
        let old_line_height = self.line_height;
        let old_left_padding = self.left_padding;
        let old_last_scroll_x = self.last_scroll_x;
        let old_visual_lines = std::mem::take(&mut self.visual_lines);
        let old_phys_to_visual = std::mem::take(&mut self.phys_to_visual);
        let old_lsp_diagnostic_indices = std::mem::take(&mut self.lsp_diagnostic_indices);

        self.line_height = api_text_area_line_height(self.scale_factor).round();
        self.left_padding = text_x;
        self.last_scroll_x = scroll_x;
        self.phys_to_visual.extend(0..editor.line_offsets.len());
        self.visual_lines
            .reserve(editor.line_offsets.len().saturating_sub(self.visual_lines.len()));
        for (line_idx, &byte_idx) in editor.line_offsets.iter().enumerate() {
            self.visual_lines.push(crate::renderer::VisualLine {
                byte_idx,
                physical_line: line_idx + 1,
                is_soft_wrap: false,
                whitespace_px_width: 0.0,
                text_px_width: 0.0,
                y_offset: line_idx as f32 * self.line_height,
                is_folded: false,
                fold_suffix: ['\0'; 4],
                fold_suffix_len: 0,
            });
        }
        if let Some(target) = ide_panel.api.mock_hover_target.as_ref() {
            self.lsp_diagnostic_indices.extend(
                diagnostics
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, diag)| (diag.part == target.part).then_some(idx)),
            );
        }
        let render_scroll_y = scroll_y - source_top_y;
        if let Some(target) = ide_panel.api.mock_hover_target.as_ref() {
            let (anchor_x, anchor_y) = crate::app::mouse::hover_anchor_for_byte(
                self,
                editor,
                target.edit_byte,
                render_scroll_y,
            );
            crate::app::mouse::HOVER_STATE.with(|state| {
                let mut state = state.borrow_mut();
                if let Some(popup) = state.popup.as_mut() {
                    if popup.byte_offset == target.edit_byte
                        && (popup.offset_x.is_none() || popup.offset_y.is_none())
                    {
                        popup.anchor_x = anchor_x;
                        popup.anchor_y = anchor_y;
                    }
                }
            });
        }
        let hovered_diag_type_target = crate::app::mouse::HOVER_STATE.with(|state| {
            let state = state.borrow();
            state.hovered_diag_type_target
        });

        let mut wants_pointer = false;
        self.draw_hover_overlays(
            editor,
            &lsp_diagnostics,
            ide_panel,
            ui_registry,
            mx,
            my,
            scroll_x,
            render_scroll_y,
            hovered_diag_type_target,
            &mut wants_pointer,
            clip_rect,
        );
        if let Some((x, y, w, h)) = clip_rect {
            self.flush();
            unsafe {
                self.gl.enable(glow::SCISSOR_TEST);
                self.gl.scissor(
                    x.round() as i32,
                    (self.height - (y + h)).round() as i32,
                    w.round() as i32,
                    h.round() as i32,
                );
            }
        }

        self.line_height = old_line_height;
        self.left_padding = old_left_padding;
        self.last_scroll_x = old_last_scroll_x;
        self.visual_lines = old_visual_lines;
        self.phys_to_visual = old_phys_to_visual;
        self.lsp_diagnostic_indices = old_lsp_diagnostic_indices;
        true
    }

    fn draw_python_text_area(
        &mut self,
        text: &str,
        spans: &[crate::highlighter::ColorSpan],
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        scroll_y: f32,
        scroll_x: f32,
    ) {
        let line_h = api_text_area_line_height(s).round();
        let scroll_y = scroll_y.clamp(
            0.0,
            crate::app::api_client::api_text_area_max_scroll(text, h, s),
        );
        let first_line = (scroll_y / line_h).floor() as usize;
        let line_offset = scroll_y - first_line as f32 * line_h;
        let max_lines = (h / line_h).ceil().max(1.0) as usize + 1;
        let start_y = y - line_offset;
        let viewport_h = self.height.max(1.0);
        let first_visible = ((0.0 - start_y) / line_h).floor().max(0.0) as usize;
        let last_visible = ((viewport_h - start_y) / line_h).ceil().max(0.0) as usize + 1;
        let mut byte_idx = 0usize;
        for (line_idx, line) in text.split('\n').enumerate() {
            let line_start = byte_idx;
            let line_end = line_start + line.len();
            if line_idx < first_line {
                byte_idx = line_end.saturating_add(1);
                continue;
            }
            let visible_idx = line_idx - first_line;
            if visible_idx >= last_visible.min(max_lines) {
                break;
            }
            if visible_idx < first_visible {
                byte_idx = line_end.saturating_add(1);
                continue;
            }
            let draw_y = (y - line_offset + visible_idx as f32 * line_h).round();
            self.draw_spanned_api_line(
                line,
                spans,
                line_start,
                x - scroll_x,
                draw_y,
                w + scroll_x,
            );
            byte_idx = line_end.saturating_add(1);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_embedded_python_editor(
        &mut self,
        editor: &crate::editor::Editor,
        spans: &[crate::highlighter::ColorSpan],
        x: f32,
        baseline_y: f32,
        w: f32,
        scroll_y: f32,
        scroll_x: f32,
        focused: bool,
        blink_alpha: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
    ) {
        let baseline_y = baseline_y.round();
        let scroll_y = scroll_y.round();
        let old_line_height = self.line_height;
        let old_baseline_offset = self.baseline_offset;
        let old_left_padding = self.left_padding;
        let old_last_scroll_x = self.last_scroll_x;
        let old_visual_lines = std::mem::take(&mut self.visual_lines);
        let old_phys_to_visual = std::mem::take(&mut self.phys_to_visual);
        let old_inlay_hints = std::mem::take(&mut self.current_python_inlay_hints);

        self.line_height = api_text_area_line_height(self.scale_factor).round();
        self.baseline_offset = api_text_area_baseline_offset(self.scale_factor);
        self.left_padding = x;
        self.last_scroll_x = scroll_x;
        self.update_cache(editor, scroll_x, scroll_y, false);

        let (first, second) = editor.text_parts();
        let (sel_start, sel_end) = editor
            .selection_anchor
            .map(|anchor| (anchor.min(editor.cursor), anchor.max(editor.cursor)))
            .unwrap_or((editor.cursor, editor.cursor));
        let render_scroll_y = (scroll_y + self.baseline_offset - baseline_y).round();
        let empty_search: &[(usize, usize)] = &[];
        let empty_inlay: &[crate::app::PythonInlayHint] = &[];
        if focused {
            self.refresh_identical_words_cache(
                editor,
                first,
                second,
                first.len(),
                editor.len(),
                sel_start,
                sel_end,
            );
        } else {
            self.identical_words_cache.clear();
            self.identical_words_cache_editor = 0;
            self.identical_words_cache_version = u64::MAX;
            self.identical_words_cache_cursor = usize::MAX;
            self.identical_words_cache_selection_anchor = None;
        }

        self.draw_editor_visible_text(
            editor,
            spans,
            empty_search,
            None,
            first,
            second,
            editor.get_cached_indent_levels(),
            first.len(),
            editor.len(),
            None,
            sel_start,
            sel_end,
            scroll_x,
            render_scroll_y,
            x + w,
            if focused { blink_alpha } else { 0.0 },
            false,
            !focused,
            false,
            self.scale_factor,
            0,
            self.visual_lines.len(),
            ui_registry,
            None,
            None,
            empty_inlay,
        );

        self.line_height = old_line_height;
        self.baseline_offset = old_baseline_offset;
        self.left_padding = old_left_padding;
        self.last_scroll_x = old_last_scroll_x;
        self.visual_lines = old_visual_lines;
        self.phys_to_visual = old_phys_to_visual;
        self.current_python_inlay_hints = old_inlay_hints;
    }

    fn draw_spanned_api_line(
        &mut self,
        line: &str,
        spans: &[crate::highlighter::ColorSpan],
        base_offset: usize,
        x: f32,
        y: f32,
        w: f32,
    ) {
        let mut draw_x = x;
        let mut offset = base_offset;
        let mut span_idx = match spans.binary_search_by_key(&base_offset, |s| s.start) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };
        for ch in line.chars() {
            if draw_x > x + w {
                break;
            }
            let adv = self.char_advance(ch);
            if ch != ' ' && ch != '\t'
                && let Some(g) = self.get_glyph(ch)
            {
                while span_idx < spans.len() && spans[span_idx].end <= offset {
                    span_idx += 1;
                }
                let color = if span_idx < spans.len() && spans[span_idx].start <= offset {
                    spans[span_idx].color
                } else {
                    self.theme.fg
                };
                self.push_quad(
                    draw_x + g.offset_x,
                    y - g.offset_y,
                    g.width,
                    g.height,
                    g.u,
                    g.v,
                    g.uw,
                    g.vh,
                    color,
                    g.is_emoji,
                );
            }
            draw_x += adv;
            offset = offset.saturating_add(ch.len_utf8());
        }
    }

    fn draw_json_lexed_line(&mut self, line: &str, x: f32, y: f32, w: f32) {
        let mut draw_x = x;
        let bytes = line.as_bytes();
        let mut idx = 0usize;
        while idx < line.len() {
            if draw_x > x + w {
                break;
            }
            let b = bytes[idx];
            if b == b'"' {
                let end = json_string_end(line, idx);
                let color = if json_string_is_property(line, end) {
                    crate::highlighter::DRACULA_CYAN
                } else {
                    crate::highlighter::DRACULA_YELLOW
                };
                self.draw_json_colored_segment(&line[idx..end], color, x, y, w, &mut draw_x);
                idx = end;
                continue;
            }
            if b == b'-' || b.is_ascii_digit() {
                let end = json_number_end(line, idx);
                self.draw_json_colored_segment(
                    &line[idx..end],
                    crate::highlighter::DRACULA_PURPLE,
                    x,
                    y,
                    w,
                    &mut draw_x,
                );
                idx = end;
                continue;
            }
            if let Some(end) = json_keyword_end(line, idx) {
                self.draw_json_colored_segment(
                    &line[idx..end],
                    crate::highlighter::DRACULA_PINK,
                    x,
                    y,
                    w,
                    &mut draw_x,
                );
                idx = end;
                continue;
            }
            let ch = line[idx..].chars().next().unwrap_or(' ');
            let end = idx + ch.len_utf8();
            self.draw_json_colored_segment(&line[idx..end], self.theme.fg, x, y, w, &mut draw_x);
            idx = end;
        }
    }

    fn draw_header_lexed_line(&mut self, line: &str, x: f32, y: f32, w: f32) {
        let Some(colon_idx) = line.find(':') else {
            let mut draw_x = x;
            self.draw_json_colored_segment(line, [0.70, 0.72, 0.78, 1.0], x, y, w, &mut draw_x);
            return;
        };
        let (key, rest) = line.split_at(colon_idx);
        let value_start = rest
            .as_bytes()
            .iter()
            .position(|b| !matches!(*b, b':' | b' ' | b'\t'))
            .unwrap_or(rest.len());
        let mut draw_x = x;
        self.draw_json_colored_segment(key, [1.0, 0.68, 0.26, 1.0], x, y, w, &mut draw_x);
        self.draw_json_colored_segment(
            &rest[..value_start],
            [0.86, 0.87, 0.91, 1.0],
            x,
            y,
            w,
            &mut draw_x,
        );
        let value = &rest[value_start..];
        let value_color = if header_value_is_number(value) {
            crate::highlighter::DRACULA_PURPLE
        } else {
            [0.70, 0.72, 0.78, 1.0]
        };
        self.draw_json_colored_segment(value, value_color, x, y, w, &mut draw_x);
    }

    fn draw_json_colored_segment(
        &mut self,
        segment: &str,
        color: [f32; 4],
        x: f32,
        y: f32,
        w: f32,
        draw_x: &mut f32,
    ) {
        for ch in segment.chars() {
            if *draw_x > x + w {
                break;
            }
            let mut buf = [0u8; 4];
            self.draw_string_scaled_stable(
                ch.encode_utf8(&mut buf),
                *draw_x,
                y,
                color,
                API_BODY_TEXT_SCALE,
            );
            *draw_x += self
                .get_ui_glyph(ch)
                .map(|g| Self::snapped_text_advance(g.advance, API_BODY_TEXT_SCALE))
                .unwrap_or(8.0);
        }
    }
}
