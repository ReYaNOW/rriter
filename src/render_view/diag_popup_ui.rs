use crate::app::IdePanelState;
use crate::lsp::Diagnostic;
use crate::renderer::Renderer;
use crate::ui_system::UiRegistry;

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
        let max_text_w = (self.width - 100.0 * s).max(300.0 * s).min(700.0 * s);

                let mut global_max_w = 180.0 * s;
        let mut total_h = pad * 2.0;

        let mut parsed_diags = Vec::new();

        for i in 0..self.hovered_diags_cache.len() {
            let (idx, _, _, _) = self.hovered_diags_cache[i];
            let diag = &lsp_diagnostics[idx];
                                    let clean_msg = diag.message.replace('\r', "");

            let mut words_meta: Vec<(usize, usize, bool, bool)> = Vec::new();
            let mut current_offset = 0;

            for (line_idx, line_str) in clean_msg.split('\n').enumerate() {
                if line_idx > 0 {
                    if let Some(last) = words_meta.last_mut() {
                        last.3 = true;
                    } else {
                        words_meta.push((current_offset, 0, false, true));
                    }
                    current_offset += 1;
                }

                let mut start = 0;
                let mut in_space = None;
                for (i, c) in line_str.char_indices() {
                    let is_space = c == ' ';
                    if in_space.is_none() {
                        in_space = Some(is_space);
                    } else if in_space != Some(is_space) {
                        words_meta.push((current_offset + start, i - start, in_space.unwrap(), false));
                        start = i;
                        in_space = Some(is_space);
                    }
                }
                if start < line_str.len() {
                    words_meta.push((current_offset + start, line_str.len() - start, in_space.unwrap(), false));
                }
            }

            let mut spans = Vec::new();
            let mut in_backticks = false;
            let chars: Vec<(usize, char)> = clean_msg.char_indices().collect();
            let mut char_idx = 0;

            while char_idx < chars.len() {
                let (offset, c) = chars[char_idx];
                if c == '`' || c == '\'' {
                    in_backticks = !in_backticks;
                    spans.push(crate::highlighter::ColorSpan { start: offset, end: offset + c.len_utf8(), color: [0.45, 0.85, 0.95, 1.0] });
                } else if c == '├' || c == '─' || c == '│' || c == '└' {
                    spans.push(crate::highlighter::ColorSpan { start: offset, end: offset + c.len_utf8(), color: [0.45, 0.45, 0.50, 1.0] });
                } else if c.is_alphanumeric() || c == '_' {
                    let w_start = offset;
                    let mut w_end = offset + c.len_utf8();
                    let mut j = char_idx + 1;
                    while j < chars.len() {
                        let (o2, c2) = chars[j];
                        if c2.is_alphanumeric() || c2 == '_' {
                            w_end = o2 + c2.len_utf8();
                            j += 1;
                        } else {
                            break;
                        }
                    }
                    let word = &clean_msg[w_start..w_end];
                    let color = match word {
                        "type" | "protocol" | "union" | "Expected" | "found" | "Argument" | "incorrect" | "assignable" | "member" | "defined" | "element" | "not" | "is" | "to" | "of" | "the" | "on" => Some([1.0, 0.47, 0.77, 1.0]),
                        "list" | "dict" | "set" | "tuple" | "str" | "int" | "bool" | "float" | "None" | "Any" | "Unknown" | "Sequence" | "Callable" | "Generator" | "AsyncGenerator" | "Coroutine" => Some([0.54, 0.91, 0.99, 1.0]),
                        _ if in_backticks => Some([0.72, 0.52, 1.0, 1.0]),
                        _ => None,
                    };
                    if let Some(c_color) = color {
                        spans.push(crate::highlighter::ColorSpan { start: w_start, end: w_end, color: c_color });
                    }
                    char_idx = j - 1;
                } else if in_backticks {
                    let color = match c {
                        '[' | ']' | '(' | ')' | '{' | '}' => Some([0.95, 0.90, 0.55, 1.0]),
                        '-' | '>' | ':' | '|' | ',' | '.' | '/' | '=' => Some([1.0, 0.47, 0.77, 1.0]),
                        _ => None,
                    };
                    if let Some(c_color) = color {
                        spans.push(crate::highlighter::ColorSpan { start: offset, end: offset + c.len_utf8(), color: c_color });
                    }
                }
                char_idx += 1;
            }

            parsed_diags.push((clean_msg, words_meta, spans));

            let source_str = diag.source.as_deref().unwrap_or("LSP");
            let code_str = diag.code.as_deref().unwrap_or("");

            let prefix_w = self.measure_ui_width("(", 1.0) + self.measure_ui_width(source_str, 1.0);
            let suffix_w = if !code_str.is_empty() {
                self.measure_ui_width(" ", 1.0)
                    + self.measure_ui_width(code_str, 1.0)
                    + self.measure_ui_width(")", 1.0)
            } else {
                self.measure_ui_width(")", 1.0)
            };
            let source_full_w = prefix_w + suffix_w;

            let mut cur_line_w = 0.0;
            let mut line_count = 1;
            let mut max_line_w = 0.0;

            let (msg_ref, meta_ref, _) = &parsed_diags[i];

            for &(start, len, is_space, has_newline) in meta_ref {
                let chunk = &msg_ref[start..start + len];
                let w = self.measure_ui_width(chunk, 1.0);

                if cur_line_w + w > max_text_w && cur_line_w > 0.0 && !is_space {
                    if cur_line_w > max_line_w {
                        max_line_w = cur_line_w;
                    }
                    line_count += 1;
                    cur_line_w = w;
                } else {
                    cur_line_w += w;
                }

                if has_newline {
                    if cur_line_w > max_line_w {
                        max_line_w = cur_line_w;
                    }
                    line_count += 1;
                    cur_line_w = 0.0;
                }
            }
            if cur_line_w > max_line_w {
                max_line_w = cur_line_w;
            }

            let source_on_new_line = cur_line_w + source_full_w + 10.0 * s > max_text_w;
            if source_on_new_line {
                line_count += 1;
                if source_full_w > max_line_w {
                    max_line_w = source_full_w;
                }
            } else {
                let combined = cur_line_w + 8.0 * s + source_full_w;
                if combined > max_line_w {
                    max_line_w = combined;
                }
            }

            let item_w = max_line_w + pad * 2.0 + icon_sz + 16.0 * s;
            if item_w > global_max_w {
                global_max_w = item_w;
            }
            let text_h = line_count as f32 * line_h;
            total_h += text_h;
        }

        total_h += (self.hovered_diags_cache.len() as f32 - 1.0) * (line_h * 0.5);
        let box_w = global_max_w;

        let (_, first_diag_x, first_line_y_top, first_diag_y_bottom) = self.hovered_diags_cache[0];
        let mut bx = first_diag_x;
        if bx + box_w > self.width - 20.0 * s {
            bx = self.width - box_w - 20.0 * s;
        }
        let mut by = first_line_y_top - total_h - 8.0 * s;
        if by < 0.0 {
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

            let prefix_w = self.measure_ui_width("(", 1.0) + self.measure_ui_width(source_str, 1.0);
            let suffix_w = if !code_str.is_empty() {
                self.measure_ui_width(" ", 1.0)
                    + self.measure_ui_width(code_str, 1.0)
                    + self.measure_ui_width(")", 1.0)
            } else {
                self.measure_ui_width(")", 1.0)
            };
                        let source_full_w = prefix_w + suffix_w;

            let mut lines_count = 1;
            let mut cur_line_w = 0.0;
            let mut text_y = current_y + line_h * 0.75;
            let mut draw_x = (bx + pad).round();

                        let (msg_ref, meta_ref, spans) = &parsed_diags[i];

            for &(start, len, is_space, has_newline) in meta_ref {
                let chunk = &msg_ref[start..start + len];
                let w = self.measure_ui_width(chunk, 1.0);

                if cur_line_w + w > max_text_w && cur_line_w > 0.0 && !is_space {
                    lines_count += 1;
                    cur_line_w = w;
                    text_y += line_h;
                    draw_x = (bx + pad).round();
                } else {
                    cur_line_w += w;
                }

                let mut current_byte = start;
                for c in chunk.chars() {
                    let mut color = [0.9, 0.9, 0.9, 1.0];
                    for span in spans {
                        if current_byte >= span.start && current_byte < span.end {
                            color = span.color;
                            break;
                        }
                    }

                    let mut b = [0; 4];
                    let s = c.encode_utf8(&mut b);
                    self.draw_string_scaled(s, draw_x, text_y.round(), color, 1.0);
                    draw_x += self.measure_ui_width(s, 1.0);
                    current_byte += c.len_utf8();
                }

                if has_newline {
                    lines_count += 1;
                    text_y += line_h;
                    draw_x = (bx + pad).round();
                    cur_line_w = 0.0;
                }
            }

            let source_on_new_line = cur_line_w + source_full_w + 10.0 * s > max_text_w;
            if source_on_new_line {
                lines_count += 1;
                text_y += line_h;
                draw_x = (bx + pad).round();
            } else {
                draw_x += 8.0 * s;
            }

            self.draw_string_scaled("(", draw_x, text_y.round(), [0.55, 0.55, 0.6, 1.0], 1.0);
            draw_x += self.measure_ui_width("(", 1.0);
            self.draw_string_scaled(
                source_str,
                draw_x,
                text_y.round(),
                [0.55, 0.55, 0.6, 1.0],
                1.0,
            );
            draw_x += self.measure_ui_width(source_str, 1.0);

            if !code_str.is_empty() {
                self.draw_string_scaled(" ", draw_x, text_y.round(), [0.55, 0.55, 0.6, 1.0], 1.0);
                draw_x += self.measure_ui_width(" ", 1.0);

                let sfx_w = self.measure_ui_width(code_str, 1.0);
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
                self.draw_string_scaled(code_str, draw_x, text_y.round(), sfx_color, 1.0);
                draw_x += sfx_w;
            }

            self.draw_string_scaled(")", draw_x, text_y.round(), [0.55, 0.55, 0.6, 1.0], 1.0);

            let total_text_h = lines_count as f32 * line_h;
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
