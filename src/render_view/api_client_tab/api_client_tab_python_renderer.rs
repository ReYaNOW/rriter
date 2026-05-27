impl Renderer {
    fn draw_api_mock_locked_signature_line(
        &mut self,
        path: &str,
        spans: &[crate::highlighter::ColorSpan],
        x: f32,
        y: f32,
        w: f32,
        s: f32,
    ) {
        let line_h = api_text_area_line_height(s);
        let signature = api_mock_signature_text(path);
        self.draw_python_text_area(
            &signature,
            spans,
            x,
            y + (line_h * 0.75).round(),
            w,
            api_mock_signature_block_height(path, s),
            s,
            0.0,
            0.0,
        );
        let sep_y = y + (6 + api_mock_path_param_count(path)) as f32 * line_h + 2.0 * s;
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
        let line_h = api_text_area_line_height(s);
        let first_line = (scroll_y / line_h).floor() as usize;
        let line_offset = scroll_y - first_line as f32 * line_h;
        let max_lines = (h / line_h).ceil().max(1.0) as usize + 1;
        let line_count = text.split('\n').count();
        for visible_idx in 0..max_lines {
            let line_idx = first_line + visible_idx;
            if line_idx >= line_count {
                break;
            }
            let text_y = y - line_offset + visible_idx as f32 * line_h;
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
        mx: f32,
        my: f32,
    ) -> Option<(String, (f32, f32, f32, f32))> {
        let line_h = api_text_area_line_height(s);
        let first_line = (scroll_y / line_h).floor() as usize;
        let line_offset = scroll_y - first_line as f32 * line_h;
        let max_lines = (h / line_h).ceil().max(1.0) as usize + 1;
        let mut hovered = None;
        for diag in diagnostics {
            if diag.part != part || diag.line < first_line {
                continue;
            }
            let visible_idx = diag.line - first_line;
            if visible_idx >= max_lines {
                continue;
            }
            let Some(line) = text.split('\n').nth(diag.line) else {
                continue;
            };
            let start_byte = byte_offset_for_char_col(line, diag.start_col);
            let end_byte = byte_offset_for_char_col(line, diag.end_col);
            let x_start = x + self.api_mono_width(&line[..start_byte]) - scroll_x;
            let x_end = x + self.api_mono_width(&line[..end_byte]) - scroll_x;
            let base_y = y - line_offset + visible_idx as f32 * line_h;
            let line_top = base_y - 19.0 * s;
            let squiggle_y = base_y + 3.0 * s;
            let squiggle_w = (x_end - x_start).max(8.0 * s).min(w);
            self.push_squiggle(
                x_start.round(),
                squiggle_y.round(),
                squiggle_w,
                [1.0, 0.36, 0.36, 1.0],
            );
            let hit_top = base_y - 14.0 * s;
            if mx >= x_start
                && mx <= x_start + squiggle_w
                && my >= hit_top
                && my <= hit_top + line_h
            {
                hovered = Some((
                    diag.message.clone(),
                    (x_start.round(), line_top.round(), squiggle_w, line_h),
                ));
            }
        }
        hovered
    }

    fn draw_api_mock_ty_popup(
        &mut self,
        message: &str,
        rect: (f32, f32, f32, f32),
        editor: &crate::editor::Editor,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
    ) {
        let text = message.lines().next().unwrap_or(message);
        let (text, spans, line_kinds, inline_code_ranges) = crate::lsp::highlight_hover_text(text);
        let source_anchor_x = rect.0 + rect.2 * 0.5;
        let source_anchor_y = rect.1 + rect.3 * 0.5;
        let scroll = crate::app::mouse::HOVER_STATE.with(|state| {
            state
                .borrow()
                .popup
                .as_ref()
                .filter(|popup| {
                    popup.byte_offset == API_MOCK_TY_POPUP_BYTE && popup.text == text
                })
                .map(|popup| popup.scroll.clone())
                .unwrap_or_else(|| crate::scroll::ScrollState::new(15.0))
        });
        let mut popup = crate::app::mouse::HoverPopup {
            text,
            spans,
            line_kinds,
            inline_code_ranges,
            byte_offset: API_MOCK_TY_POPUP_BYTE,
            anchor_x: source_anchor_x,
            anchor_y: source_anchor_y,
            offset_x: None,
            offset_y: None,
            anim_progress: 1.0,
            scroll,
            layout_cache: None,
        };
        let render_scroll_y =
            self.api_mock_ty_hover_render_scroll_y(editor, popup.byte_offset, rect.1);
        let mut wants_pointer = false;
        let (bx, by, bw, bh, max_scroll) = self.draw_hover_popup(
            &mut popup,
            None,
            None,
            editor,
            ui_registry,
            mx,
            my,
            render_scroll_y,
            &mut wants_pointer,
            1.0,
            None,
        );
        crate::app::mouse::HOVER_STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.byte_offset = Some(API_MOCK_TY_POPUP_BYTE);
            state.put_type_popup_after_draw(Some(popup), Some((bx, by, bw, bh)), max_scroll);
        });
    }

    fn api_mock_ty_hover_render_scroll_y(
        &self,
        editor: &crate::editor::Editor,
        byte_offset: usize,
        source_line_top: f32,
    ) -> f32 {
        let phys_line = editor
            .line_offsets
            .partition_point(|&offset| offset <= byte_offset)
            .saturating_sub(1);
        let vis_line_idx = self.phys_to_visual.get(phys_line).copied().unwrap_or(0) as f32;
        vis_line_idx * self.line_height - source_line_top
    }

    fn draw_existing_api_mock_ty_popup(
        &mut self,
        editor: &crate::editor::Editor,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
    ) -> bool {
        let should_draw = crate::app::mouse::HOVER_STATE.with(|state| {
            let state = state.borrow();
            state
                .popup
                .as_ref()
                .is_some_and(|popup| popup.byte_offset == API_MOCK_TY_POPUP_BYTE)
                && state
                    .popup_or_bridge_contains(mx, my, self.width, self.scale_factor)
                    .0
        });
        if !should_draw {
            crate::app::mouse::HOVER_STATE.with(|state| {
                let mut state = state.borrow_mut();
                if state
                    .popup
                    .as_ref()
                    .is_some_and(|popup| popup.byte_offset == API_MOCK_TY_POPUP_BYTE)
                {
                    state.put_type_popup_after_draw(None, None, 0.0);
                    state.byte_offset = None;
                }
            });
            return false;
        }
        let Some(mut popup) = crate::app::mouse::HOVER_STATE.with(|state| {
            let mut state = state.borrow_mut();
            let (popup, _, _) = state.take_type_popup_for_draw(false);
            popup
        }) else {
            return false;
        };
        let render_scroll_y = self.api_mock_ty_hover_render_scroll_y(
            editor,
            popup.byte_offset,
            popup.anchor_y - api_text_area_line_height(self.scale_factor) * 0.5,
        );
        let mut wants_pointer = false;
        let (bx, by, bw, bh, max_scroll) = self.draw_hover_popup(
            &mut popup,
            None,
            None,
            editor,
            ui_registry,
            mx,
            my,
            render_scroll_y,
            &mut wants_pointer,
            1.0,
            None,
        );
        crate::app::mouse::HOVER_STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.byte_offset = Some(API_MOCK_TY_POPUP_BYTE);
            state.put_type_popup_after_draw(Some(popup), Some((bx, by, bw, bh)), max_scroll);
        });
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
        let line_h = api_text_area_line_height(s);
        let scroll_y = scroll_y.clamp(
            0.0,
            crate::app::api_client::api_text_area_max_scroll(text, h, s),
        );
        let first_line = (scroll_y / line_h).floor() as usize;
        let line_offset = scroll_y - first_line as f32 * line_h;
        let max_lines = (h / line_h).ceil().max(1.0) as usize + 1;
        let mut byte_idx = 0usize;
        for (line_idx, line) in text.split('\n').enumerate() {
            let line_start = byte_idx;
            let line_end = line_start + line.len();
            if line_idx < first_line {
                byte_idx = line_end.saturating_add(1);
                continue;
            }
            let visible_idx = line_idx - first_line;
            if visible_idx >= max_lines {
                break;
            }
            let draw_y = y - line_offset + visible_idx as f32 * line_h;
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
