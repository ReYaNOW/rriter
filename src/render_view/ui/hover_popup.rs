use super::*;

impl Renderer {
    pub(crate) fn build_hover_popup_layout(
        &mut self,
        popup: &crate::app::mouse::HoverPopup,
        max_text_w: f32,
        line_h: f32,
    ) -> crate::app::mouse::HoverLayoutCache {
        let s = self.scale_factor;
        let mut lines: Vec<crate::app::mouse::HoverVisualLine> = Vec::new();
        let mut cur_line_w = 0.0;
        let mut cur_line: Vec<(char, [f32; 4], usize)> = Vec::new();
        let mut last_space_idx = None;
        let mut raw_line_no = 0usize;
        let mut leading_spaces = 0;
        let mut counting_leading = true;
        let mut span_idx = 0usize;

        for (offset, c) in popup.text.char_indices() {
            let kind = popup
                .line_kinds
                .get(raw_line_no)
                .copied()
                .unwrap_or(crate::lsp::HoverLineKindPublic::Text);
            let scale_mul = match kind {
                crate::lsp::HoverLineKindPublic::Header1 => 1.3,
                crate::lsp::HoverLineKindPublic::Header2 => 1.15,
                _ => 1.0,
            };

            if c == '\n' {
                lines.push(crate::app::mouse::HoverVisualLine {
                    glyphs: std::mem::take(&mut cur_line),
                    kind,
                });
                cur_line_w = 0.0;
                last_space_idx = None;
                raw_line_no += 1;
                counting_leading = true;
                leading_spaces = 0;
                continue;
            }

            if counting_leading {
                if c == ' ' {
                    leading_spaces += 1;
                } else {
                    counting_leading = false;
                }
            }

            let adv = self.char_advance(c) * scale_mul;
            if cur_line_w + adv > max_text_w && cur_line_w > 40.0 * s {
                if let Some(space_pos) = last_space_idx {
                    let mut remainder = cur_line.split_off(space_pos);
                    if !remainder.is_empty() && remainder[0].0 == ' ' {
                        remainder.remove(0);
                    }

                    let hanging_spaces = (leading_spaces + 4).min(20);
                    let mut new_remainder = Vec::with_capacity(hanging_spaces + remainder.len());
                    for _ in 0..hanging_spaces {
                        new_remainder.push((' ', [0.0, 0.0, 0.0, 0.0], offset));
                    }
                    new_remainder.extend(remainder);
                    remainder = new_remainder;

                    lines.push(crate::app::mouse::HoverVisualLine {
                        glyphs: std::mem::take(&mut cur_line),
                        kind,
                    });
                    cur_line = remainder;
                    cur_line_w = cur_line
                        .iter()
                        .map(|&(ch, _, _)| self.char_advance(ch) * scale_mul)
                        .sum();
                } else {
                    lines.push(crate::app::mouse::HoverVisualLine {
                        glyphs: std::mem::take(&mut cur_line),
                        kind,
                    });
                    cur_line_w = 0.0;
                }
                last_space_idx = None;
            }

            while span_idx < popup.spans.len() && offset >= popup.spans[span_idx].end {
                span_idx += 1;
            }
            let color = popup
                .spans
                .get(span_idx)
                .filter(|span| offset >= span.start && offset < span.end)
                .map(|span| span.color)
                .unwrap_or([0.972, 0.972, 0.949, 1.0]);

            cur_line.push((c, color, offset));
            cur_line_w += adv;

            if c == ' ' {
                last_space_idx = Some(cur_line.len() - 1);
            }
        }
        if !cur_line.is_empty() {
            let kind = popup
                .line_kinds
                .get(raw_line_no)
                .copied()
                .unwrap_or(crate::lsp::HoverLineKindPublic::Text);
            lines.push(crate::app::mouse::HoverVisualLine {
                glyphs: cur_line,
                kind,
            });
        }

        while let Some(line) = lines.last() {
            if line.glyphs.is_empty() {
                lines.pop();
            } else {
                break;
            }
        }

        let mut max_line_w: f32 = 0.0;
        let mut total_text_h: f32 = 0.0;
        for line in &lines {
            let scale_mul = match line.kind {
                crate::lsp::HoverLineKindPublic::Header1 => 1.15,
                crate::lsp::HoverLineKindPublic::Header2 => 1.05,
                _ => 1.0,
            };
            let w = if matches!(
                line.kind,
                crate::lsp::HoverLineKindPublic::Header1 | crate::lsp::HoverLineKindPublic::Header2
            ) {
                let mut s_buf = String::new();
                for &(c, _, _) in &line.glyphs {
                    s_buf.push(c);
                }
                self.measure_ui_width(&s_buf, scale_mul)
            } else {
                let mut w: f32 = line
                    .glyphs
                    .iter()
                    .map(|&(ch, _, _)| self.char_advance(ch))
                    .sum();
                if line.kind == crate::lsp::HoverLineKindPublic::Code {
                    w += 16.0 * s;
                }
                w
            };
            max_line_w = max_line_w.max(w);
            total_text_h += line_h * scale_mul;
        }

        crate::app::mouse::HoverLayoutCache {
            scale_factor: self.scale_factor,
            max_text_w,
            span_count: popup.spans.len(),
            text_len: popup.text.len(),
            lines,
            max_line_w,
            total_text_h,
        }
    }

    pub fn draw_hover_popup(
        &mut self,
        popup: &mut crate::app::mouse::HoverPopup,
        selection: Option<(usize, usize)>,
        editor: &crate::editor::Editor,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        render_scroll_y: f32,
        wants_pointer: &mut bool,
    ) -> (f32, f32, f32, f32, f32) {
        let s = self.scale_factor;
        let pad = 12.0 * s;
        let line_h = 22.0 * s;
        let max_text_w = (self.width - 80.0 * s).min(820.0 * s).max(320.0 * s);

        let cache_valid = popup.layout_cache.as_ref().is_some_and(|cache| {
            cache.scale_factor == self.scale_factor
                && cache.max_text_w == max_text_w
                && cache.span_count == popup.spans.len()
                && cache.text_len == popup.text.len()
        });
        if !cache_valid {
            popup.layout_cache = Some(self.build_hover_popup_layout(popup, max_text_w, line_h));
        }
        let layout = popup.layout_cache.as_ref().unwrap();
        let lines = &layout.lines;
        let module_prefix_chars: Vec<char> = "[[MODULE]] ".chars().collect();

        let attached_diag = self
            .last_diag_popup_rect
            .map(|(rx, ry, rw, rh, _, _, _)| (rx, ry, rw, rh));
        let mut box_w = layout.max_line_w + pad * 2.0;
        if let Some((_, _, diag_w, _)) = attached_diag {
            box_w = box_w.max(diag_w);
        }
        let max_visible_h = (self.height * 0.35).min(layout.total_text_h + pad * 2.0);
        let box_h = max_visible_h;

        let phys_line = editor
            .line_offsets
            .partition_point(|&o| o <= popup.byte_offset)
            .saturating_sub(1);
        let vis_line_idx = self.phys_to_visual.get(phys_line).copied().unwrap_or(0) as f32;
        let line_top_y = (vis_line_idx * self.line_height) - render_scroll_y;

        let mut bx = popup.anchor_x;
        let mut by = line_top_y;

        if let Some(ox) = popup.offset_x {
            bx += ox;
        }
        if let Some(oy) = popup.offset_y {
            by += oy;
        }

        if popup.offset_x.is_none() || popup.offset_y.is_none() || attached_diag.is_some() {
            let orig_bx = bx;
            let orig_by = by;
            if let Some((rx, _, _, _)) = attached_diag {
                bx = rx;
            }
            let mut target_by = (line_top_y - box_h - 8.0 * s).max(10.0 * s);
            if let Some((_, diag_y, _, diag_h)) = attached_diag {
                target_by = diag_y + diag_h;
            } else if target_by + box_h > line_top_y - 8.0 * s {
                target_by = (line_top_y - box_h - 8.0 * s).max(10.0 * s);
            }
            by = target_by;

            if bx + box_w > self.width - 20.0 * s {
                bx = self.width - box_w - 20.0 * s;
            }
            if bx < 20.0 * s {
                bx = 20.0 * s;
            }

            if attached_diag.is_none() {
                popup.offset_x = Some(bx - orig_bx);
                popup.offset_y = Some(by - orig_by);
            }
        }
        ui_registry.register_blocker(
            crate::ui_system::UiId::BottomPanelBody,
            bx,
            by,
            box_w,
            box_h,
            mx,
            my,
        );
        let popup_hovered = mx >= bx && mx <= bx + box_w && my >= by && my <= by + box_h;
        if popup_hovered && !*wants_pointer {
            ui_registry.reset_cursor_state();
        }

        let max_scroll = (layout.total_text_h + pad * 2.0 - box_h).max(0.0);
        let scroll_y = popup.scroll.current;

        if attached_diag.is_none() {
            self.push_rounded_rect(
                bx.round() - 1.0,
                by.round() - 1.0,
                box_w.round() + 2.0,
                box_h.round() + 2.0,
                6.0 * s,
                [0.4, 0.4, 0.45, 0.6],
            );
            self.push_rounded_rect(
                bx.round(),
                by.round(),
                box_w.round(),
                box_h.round(),
                6.0 * s,
                [
                    self.theme.minimap_bg[0],
                    self.theme.minimap_bg[1],
                    self.theme.minimap_bg[2],
                    1.0,
                ],
            );
        }

        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            let sy = (self.height - (by + box_h)).round() as i32;
            self.gl.scissor(
                bx.round() as i32,
                sy,
                box_w.round() as i32,
                box_h.round() as i32,
            );
        }

        let mut current_top = by + pad - scroll_y;
        let selected = selection.filter(|(a, b)| a != b);
        let mut idx = 0usize;
        while idx < lines.len() {
            let visual_line = &lines[idx];
            let line = &visual_line.glyphs;
            let line_kind = visual_line.kind;
            let scale_mul = match line_kind {
                crate::lsp::HoverLineKindPublic::Header1 => 1.15,
                crate::lsp::HoverLineKindPublic::Header2 => 1.05,
                _ => 1.0,
            };
            let cur_line_h = line_h * scale_mul;

            let rounded_top = current_top.round();
            let text_y = rounded_top + (cur_line_h * 0.75).round();

            if current_top + cur_line_h > by && current_top < by + box_h {
                let is_separator = line
                    .iter()
                    .all(|(c, _, _)| *c == '-' || c.is_ascii_whitespace())
                    && line.iter().any(|(c, _, _)| *c == '-');
                if is_separator {
                    self.push_rect(
                        (bx + pad).round(),
                        rounded_top + (cur_line_h * 0.5).round(),
                        (box_w - pad * 2.0).round(),
                        1.0_f32.max(s.round()),
                        [1.0, 1.0, 1.0, 0.10],
                    );
                    current_top += cur_line_h;
                    idx += 1;
                    continue;
                }

                if line_kind == crate::lsp::HoverLineKindPublic::Code {
                    let mut run_len = 1usize;
                    while idx + run_len < lines.len()
                        && lines[idx + run_len].kind == crate::lsp::HoverLineKindPublic::Code
                    {
                        run_len += 1;
                    }
                    self.push_rounded_rect(
                        (bx + pad - 4.0 * s).round(),
                        rounded_top,
                        (box_w - pad * 2.0 + 8.0 * s).round(),
                        (line_h * run_len as f32).round(),
                        4.0 * s,
                        [0.15, 0.16, 0.20, 0.96],
                    );
                }

                let is_module_header = line_kind == crate::lsp::HoverLineKindPublic::Text
                    && line.len() >= module_prefix_chars.len()
                    && line
                        .iter()
                        .zip(module_prefix_chars.iter())
                        .all(|((ch, _, _), marker)| ch == marker);
                let mut glyph_start = 0usize;
                let start_x = if line_kind == crate::lsp::HoverLineKindPublic::Code {
                    (bx + pad + 8.0 * s).round()
                } else if is_module_header {
                    let icon_size = 18.0 * s;
                    let icon_x = (bx + pad).round();
                    let icon_y = rounded_top + ((cur_line_h - icon_size) * 0.5).round();
                    self.draw_file_icon("folder", true, icon_x, icon_y, icon_size);
                    glyph_start = module_prefix_chars.len();
                    (bx + pad + icon_size + 4.0 * s).round()
                } else {
                    (bx + pad).round()
                };
                let mut draw_x = start_x;
                let is_header = matches!(
                    line_kind,
                    crate::lsp::HoverLineKindPublic::Header1
                        | crate::lsp::HoverLineKindPublic::Header2
                );

                if is_header {
                    for &(c, color, offset) in line.iter().skip(glyph_start) {
                        let mut adv = 0.0;
                        if let Some(g) = self.get_ui_glyph(c) {
                            adv = g.advance * scale_mul;
                            if let Some((sel_start, sel_end)) = selected {
                                if offset >= sel_start && offset < sel_end {
                                    self.push_rect(
                                        draw_x,
                                        rounded_top,
                                        adv,
                                        cur_line_h.round(),
                                        self.theme.sel,
                                    );
                                }
                            }
                            self.push_quad(
                                (draw_x + g.offset_x * scale_mul).round(),
                                (text_y - g.offset_y * scale_mul).round(),
                                g.width * scale_mul,
                                g.height * scale_mul,
                                g.u,
                                g.v,
                                g.uw,
                                g.vh,
                                color,
                                g.is_emoji,
                            );
                        }
                        draw_x += adv;
                    }
                } else {
                    let mut inline_run_start_x: Option<f32> = None;
                    for &(c, _, offset) in line.iter().skip(glyph_start) {
                        let adv = self.char_advance(c);
                        let in_inline = popup
                            .inline_code_ranges
                            .iter()
                            .any(|&(start, end)| offset >= start && offset < end);
                        if in_inline && inline_run_start_x.is_none() {
                            inline_run_start_x = Some(draw_x - 1.0 * s);
                        } else if !in_inline {
                            if let Some(run_x) = inline_run_start_x.take() {
                                self.push_rounded_rect(
                                    run_x,
                                    rounded_top + (cur_line_h * 0.1).round(),
                                    (draw_x - run_x + 1.0 * s).max(2.0 * s),
                                    (cur_line_h - 2.0 * s).round(),
                                    3.0 * s,
                                    [0.22, 0.23, 0.28, 0.98],
                                );
                            }
                        }
                        draw_x += adv;
                    }
                    if let Some(run_x) = inline_run_start_x.take() {
                        self.push_rounded_rect(
                            run_x,
                            rounded_top + (cur_line_h * 0.1).round(),
                            (draw_x - run_x + 1.0 * s).max(2.0 * s),
                            (cur_line_h - 2.0 * s).round(),
                            3.0 * s,
                            [0.22, 0.23, 0.28, 0.98],
                        );
                    }

                    draw_x = start_x;
                    for &(c, color, offset) in line.iter().skip(glyph_start) {
                        let adv = self.char_advance(c);
                        if let Some((sel_start, sel_end)) = selected {
                            if offset >= sel_start && offset < sel_end {
                                self.push_rect(
                                    draw_x,
                                    rounded_top,
                                    adv,
                                    cur_line_h.round(),
                                    self.theme.sel,
                                );
                            }
                        }
                        let mut b = [0; 4];
                        let s_str = c.encode_utf8(&mut b);
                        self.draw_string_mono_scaled(s_str, draw_x, text_y, color, 1.0);
                        draw_x += adv;
                    }
                }
            }
            current_top += cur_line_h;
            idx += 1;
        }

        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }

        if max_scroll > 0.0 {
            let track_h = box_h - 16.0 * s;
            let thumb_h = (box_h / (layout.total_text_h + pad * 2.0) * track_h).max(20.0 * s);
            let thumb_y = by + 8.0 * s + (scroll_y / max_scroll) * (track_h - thumb_h);

            self.push_rounded_rect(
                bx + box_w - 8.0 * s,
                thumb_y.round(),
                4.0 * s,
                thumb_h,
                2.0 * s,
                [1.0, 1.0, 1.0, 0.2],
            );

            ui_registry.register_rect(
                crate::ui_system::UiId::HoverPopupScroll,
                bx + box_w - 12.0 * s,
                by,
                12.0 * s,
                box_h,
                mx,
                my,
            );
            if ui_registry.hovered() == Some(crate::ui_system::UiId::HoverPopupScroll) {
                ui_registry.reset_cursor_state();
            }
        }

        (bx, by, box_w, box_h, max_scroll)
    }
}
