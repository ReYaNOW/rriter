use crate::app::IdePanelState;
use crate::lsp::Diagnostic;
use crate::renderer::Renderer;
use crate::ui_system::UiRegistry;

pub struct DiagChar {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub byte_offset: usize,
}

thread_local! {
    static TS_SPANS_CACHE: std::cell::RefCell<std::collections::HashMap<String, Vec<crate::highlighter::ColorSpan>>> = std::cell::RefCell::new(std::collections::HashMap::new());
    pub static DIAG_CHARS: std::cell::RefCell<Vec<DiagChar>> = std::cell::RefCell::new(Vec::new());
}

pub fn diag_popup_byte_at(mx: f32, my: f32) -> usize {
    DIAG_CHARS.with(|chars| {
        let chars = chars.borrow();
        if chars.is_empty() {
            return 0;
        }

        let mut best_y_dist = f32::MAX;
        let mut best_y = chars[0].y;
        for c in chars.iter() {
            let dist = (my - (c.y + c.h / 2.0)).abs();
            if dist < best_y_dist {
                best_y_dist = dist;
                best_y = c.y;
            }
        }

        let mut closest = 0;
        let mut best_x_dist = f32::MAX;
        for c in chars.iter() {
            if (c.y - best_y).abs() < 1.0 {
                let cx = c.x + c.w / 2.0;
                let dist = (mx - cx).abs();
                if dist < best_x_dist {
                    best_x_dist = dist;
                    closest = if mx > cx {
                        c.byte_offset + c.w as usize * 0 + 1
                    } else {
                        c.byte_offset
                    };
                }
            }
        }
        closest
    })
}

impl Renderer {
    pub fn draw_diagnostic_popup(
        &mut self,
        lsp_diagnostics: &[Diagnostic],
        ide_panel: &IdePanelState,
        ui_registry: &mut UiRegistry,
        mx: f32,
        my: f32,
        wants_pointer: &mut bool,
    ) {
        let s = self.scale_factor;
        let pad = 12.0 * s;
        let line_h = 22.0 * s;
        let icon_sz = 20.0 * s;
        let max_text_w = (self.width - 80.0 * s)
            .max(400.0 * s)
            .min(self.width - 40.0 * s);

        let mut global_max_w = 180.0 * s;
        let mut total_h = pad * 2.0;

        let mut parsed_diags = Vec::new();

        DIAG_CHARS.with(|c| c.borrow_mut().clear());
        let mut global_byte_offset = 0;
        let (sel_anchor, sel_cursor) = crate::app::mouse::HOVER_STATE.with(|s| {
            let s = s.borrow();
            (s.diag_selection_anchor, s.diag_selection_cursor)
        });
        let sel_start = sel_anchor.unwrap_or(0).min(sel_cursor.unwrap_or(0));
        let sel_end = sel_anchor.unwrap_or(0).max(sel_cursor.unwrap_or(0));
        let has_sel = sel_anchor.is_some() && sel_cursor.is_some() && sel_start != sel_end;

        for i in 0..self.hovered_diags_cache.len() {
            let (idx, _, _, _) = self.hovered_diags_cache[i];
            let diag = &lsp_diagnostics[idx];
            let clean_msg = diag
                .message
                .replace('\r', "")
                .replace("\\n", "\n")
                .replace("\\t", "    ");

            let mut spans = TS_SPANS_CACHE.with(|cache| {
                let mut cache = cache.borrow_mut();
                if let Some(cached) = cache.get(&clean_msg) {
                    cached.clone()
                } else {
                    let parsed = crate::lsp::highlight_diagnostic_message(&clean_msg);
                    if cache.len() > 100 {
                        cache.clear();
                    }
                    cache.insert(clean_msg.clone(), parsed.clone());
                    parsed
                }
            });

            spans.sort_by_key(|s| s.start);

            let mut lines = Vec::new();
            let mut cur_line_w = 0.0;
            let mut cur_line: Vec<(char, [f32; 4])> = Vec::new();
            let mut last_space_idx = None;
            let mut current_indent: Vec<(char, [f32; 4])> = Vec::new();
            let mut counting_indent = true;

            for (offset, c) in clean_msg.char_indices() {
                if c == '\n' {
                    lines.push(std::mem::take(&mut cur_line));
                    cur_line_w = 0.0;
                    last_space_idx = None;
                    current_indent.clear();
                    counting_indent = true;
                    continue;
                }

                if counting_indent {
                    if c == ' ' || c == '│' || c == '├' || c == '└' || c == '─' {
                        let blank_color = [0.0, 0.0, 0.0, 0.0];
                        current_indent.push((' ', blank_color));
                    } else {
                        counting_indent = false;
                    }
                }

                let adv = self.char_advance(c);
                if cur_line_w + adv > max_text_w && cur_line_w > 40.0 * s {
                    if let Some(space_pos) = last_space_idx {
                        let mut remainder = cur_line.split_off(space_pos);
                        if !remainder.is_empty() && remainder[0].0 == ' ' {
                            remainder.remove(0);
                        }
                        lines.push(std::mem::take(&mut cur_line));
                        cur_line = current_indent.clone();
                        cur_line.extend(remainder);
                        cur_line_w = cur_line.iter().map(|&(ch, _)| self.char_advance(ch)).sum();
                    } else {
                        lines.push(std::mem::take(&mut cur_line));
                        cur_line = current_indent.clone();
                        cur_line_w = cur_line.iter().map(|&(ch, _)| self.char_advance(ch)).sum();
                    }
                    last_space_idx = None;
                }

                let mut color = [0.972, 0.972, 0.949, 1.0];
                for span in &spans {
                    if offset >= span.start && offset < span.end {
                        color = span.color;
                        break;
                    }
                }

                cur_line.push((c, color));
                cur_line_w += adv;

                if c == ' ' {
                    last_space_idx = Some(cur_line.len() - 1);
                }
            }
            if !cur_line.is_empty() {
                lines.push(cur_line);
            }

            let source_str = diag.source.as_deref().unwrap_or("LSP");
            let code_str = diag.code.as_deref().unwrap_or("");

            let prefix_w =
                self.measure_mono_width("(", 1.0) + self.measure_mono_width(source_str, 1.0);
            let suffix_w = if !code_str.is_empty() {
                self.measure_mono_width(" ", 1.0)
                    + self.measure_mono_width(code_str, 1.0)
                    + self.measure_mono_width(")", 1.0)
            } else {
                self.measure_mono_width(")", 1.0)
            };
            let source_full_w = prefix_w + suffix_w;

            let mut max_line_w = 0.0;
            for line in &lines {
                let w: f32 = line.iter().map(|&(ch, _)| self.char_advance(ch)).sum();
                if w > max_line_w {
                    max_line_w = w;
                }
            }

            let last_line_w = lines
                .last()
                .map(|l| l.iter().map(|&(ch, _)| self.char_advance(ch)).sum::<f32>())
                .unwrap_or(0.0);
            let mut line_count = lines.len();
            let source_on_new_line = last_line_w + source_full_w + 10.0 * s > max_text_w;

            if source_on_new_line {
                line_count += 1;
                if source_full_w > max_line_w {
                    max_line_w = source_full_w;
                }
            } else {
                let combined = last_line_w + 8.0 * s + source_full_w;
                if combined > max_line_w {
                    max_line_w = combined;
                }
            }

            let item_w = max_line_w + pad * 2.0 + icon_sz + 16.0 * s;
            if item_w > global_max_w {
                global_max_w = item_w;
            }
            total_h += line_count as f32 * line_h;

            parsed_diags.push((lines, source_on_new_line, last_line_w, line_count));
        }

        total_h += (self.hovered_diags_cache.len() as f32 - 1.0) * (line_h * 0.5);
        total_h = total_h.min(self.height - 60.0 * s);
        let box_w = global_max_w;

        let (_, first_diag_x, first_line_y_top, first_diag_y_bottom) = self.hovered_diags_cache[0];
        let mut bx = first_diag_x;

        crate::app::mouse::HOVER_STATE.with(|s| {
            if let Some(popup) = &s.borrow().popup {
                bx = bx.min(popup.anchor_x);
            }
        });

        if bx + box_w > self.width - 20.0 * s {
            bx = self.width - box_w - 20.0 * s;
        }
        if bx < 20.0 * s {
            bx = 20.0 * s;
        }
        let prefer_below = first_line_y_top - (self.height * 0.45) - 8.0 * s < 0.0;
        let mut by = if prefer_below {
            first_diag_y_bottom + 8.0 * s
        } else {
            first_line_y_top - total_h - 8.0 * s
        };
        if !prefer_below && by < 0.0 {
            by = first_diag_y_bottom + 8.0 * s;
        }

        self.last_diag_popup_rect = Some((bx, by, box_w, total_h));

        ui_registry.register_blocker(
            crate::ui_system::UiId::BottomPanelBody,
            bx,
            by,
            box_w,
            total_h,
            mx,
            my,
        );

        let popup_hovered = mx >= bx && mx <= bx + box_w && my >= by && my <= by + total_h;
        if popup_hovered && !*wants_pointer {
            ui_registry.reset_cursor_state();
        }

        self.push_rounded_rect(
            bx.round() - 1.0,
            by.round() - 1.0,
            box_w.round() + 2.0,
            total_h.round() + 2.0,
            6.0 * s,
            [0.4, 0.4, 0.45, 0.6],
        );
        self.push_rounded_rect(
            bx.round(),
            by.round(),
            box_w.round(),
            total_h.round(),
            6.0 * s,
            [
                self.theme.minimap_bg[0],
                self.theme.minimap_bg[1],
                self.theme.minimap_bg[2],
                1.0,
            ],
        );

        let mut current_y = by + pad;

        for i in 0..self.hovered_diags_cache.len() {
            let (idx, _, _, _) = self.hovered_diags_cache[i];
            let diag = &lsp_diagnostics[idx];
            let border_color = match diag.severity {
                crate::lsp::DiagSeverity::Error => [0.96, 0.26, 0.21, 1.0],
                crate::lsp::DiagSeverity::Warning => [0.95, 0.9, 0.3, 1.0],
                crate::lsp::DiagSeverity::Info => [0.26, 0.73, 0.90, 1.0],
                crate::lsp::DiagSeverity::Hint => [0.50, 0.50, 0.50, 1.0],
            };

            let source_str = diag.source.as_deref().unwrap_or("LSP");
            let code_str = diag.code.as_deref().unwrap_or("");

            let (lines, source_on_new_line, last_line_w, line_count) = &parsed_diags[i];

            let mut text_y = current_y + line_h * 0.75;
            let mut draw_x = (bx + pad).round();

            for line in lines {
                for &(c, color) in line {
                    let adv = self.char_advance(c);
                    let char_len = c.len_utf8();
                    let ch_y = text_y.round() - line_h * 0.75;

                    DIAG_CHARS.with(|chars| {
                        chars.borrow_mut().push(DiagChar {
                            x: draw_x,
                            y: ch_y,
                            w: adv,
                            h: line_h,
                            byte_offset: global_byte_offset,
                        });
                    });

                    if has_sel && global_byte_offset >= sel_start && global_byte_offset < sel_end {
                        self.push_rect(draw_x, ch_y, adv, line_h, self.theme.sel);
                    }

                    let mut b = [0; 4];
                    let s_str = c.encode_utf8(&mut b);
                    self.draw_string_mono_scaled(s_str, draw_x, text_y.round(), color, 1.0);
                    draw_x += adv;
                    global_byte_offset += char_len;
                }
                text_y += line_h;
                draw_x = (bx + pad).round();
                global_byte_offset += 1;
            }

            if !*source_on_new_line {
                text_y -= line_h;
                draw_x = (bx + pad).round() + *last_line_w + 8.0 * s;
            }

            self.draw_string_mono_scaled("(", draw_x, text_y.round(), [0.55, 0.55, 0.6, 1.0], 1.0);
            draw_x += self.measure_mono_width("(", 1.0);
            self.draw_string_mono_scaled(
                source_str,
                draw_x,
                text_y.round(),
                [0.55, 0.55, 0.6, 1.0],
                1.0,
            );
            draw_x += self.measure_mono_width(source_str, 1.0);

            if !code_str.is_empty() {
                self.draw_string_mono_scaled(
                    " ",
                    draw_x,
                    text_y.round(),
                    [0.55, 0.55, 0.6, 1.0],
                    1.0,
                );
                draw_x += self.measure_mono_width(" ", 1.0);

                let sfx_w = self.measure_mono_width(code_str, 1.0);
                let has_href = diag.code_href.is_some();
                let sfx_hovered = has_href
                    && mx >= draw_x - 1.0
                    && mx <= draw_x + sfx_w + 1.0
                    && my >= text_y.round() - line_h
                    && my <= text_y.round() + 2.0 * s;

                let link_color: [f32; 4] = [0.72, 0.52, 1.0, 1.0];
                let sfx_color = if sfx_hovered {
                    link_color
                } else {
                    [link_color[0], link_color[1], link_color[2], 0.85]
                };

                if has_href {
                    let ul_alpha = if sfx_hovered { 0.9 } else { 0.55 };
                    self.push_rect(
                        draw_x,
                        text_y.round() + 1.0,
                        sfx_w,
                        1.0,
                        [link_color[0], link_color[1], link_color[2], ul_alpha],
                    );
                    if sfx_hovered {
                        *wants_pointer = true;
                        self.last_diag_href = diag.code_href.clone();
                    }

                    ui_registry.register_rect(
                        crate::ui_system::UiId::PopupOpenDiagUrl(idx),
                        draw_x - 1.0,
                        text_y.round() - line_h,
                        sfx_w + 2.0,
                        line_h + 2.0 * s,
                        mx,
                        my,
                    );
                }
                self.draw_string_mono_scaled(code_str, draw_x, text_y.round(), sfx_color, 1.0);
                draw_x += sfx_w;
            }

            self.draw_string_mono_scaled(")", draw_x, text_y.round(), [0.55, 0.55, 0.6, 1.0], 1.0);

            let total_text_h = *line_count as f32 * line_h;
            self.push_rect(bx + 4.0 * s, current_y, 3.0 * s, total_text_h, border_color);

            let is_copied = ide_panel.diag_copied_idx == Some(idx);
            let btn_x = (bx + box_w - pad - icon_sz).round();
            let btn_y = (current_y + (total_text_h - icon_sz) / 2.0).round();
            let btn_hovered = mx >= btn_x - 4.0 * s
                && mx <= btn_x + icon_sz + 4.0 * s
                && my >= btn_y - 2.0 * s
                && my <= btn_y + icon_sz + 4.0 * s;

            if btn_hovered {
                self.push_rounded_rect(
                    btn_x - 4.0 * s,
                    btn_y - 2.0 * s,
                    icon_sz + 8.0 * s,
                    icon_sz + 4.0 * s,
                    4.0 * s,
                    [1.0, 1.0, 1.0, 0.1],
                );
                *wants_pointer = true;
            }
            let icon_type = if is_copied {
                crate::widgets::IconType::Check
            } else {
                crate::widgets::IconType::Copy
            };
            let icon_color = if is_copied {
                [0.3, 0.9, 0.4, 1.0]
            } else {
                self.theme.fg
            };
            let icon_render_sz = 16.0 * s;
            let offset = (icon_sz - icon_render_sz) / 2.0;
            self.draw_atlas_icon(
                icon_type,
                btn_x + offset,
                btn_y + offset,
                icon_render_sz,
                icon_color,
            );

            ui_registry.register_rect(
                crate::ui_system::UiId::PopupCopyDiagnostic(idx),
                btn_x - 4.0 * s,
                btn_y - 2.0 * s,
                icon_sz + 8.0 * s,
                icon_sz + 4.0 * s,
                mx,
                my,
            );

            current_y += total_text_h + line_h * 0.5;
        }
    }
}
