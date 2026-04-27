use crate::editor::Editor;
use crate::highlighter::ColorSpan;
use crate::renderer::{Renderer, Vertex};

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    pub fn draw_minimap(
        &mut self,
        editor: &Editor,
        spans: &[ColorSpan],
        render_scroll_y: f32,
        max_scroll: f32,
        total_lines: usize,
        visible_cursor_line: usize,
        editor_height: f32,
        tab_bar_h: f32,
    ) {
        let scroll_ratio_y = if max_scroll > 0.0 {
            (render_scroll_y / max_scroll).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let minimap_w = self.minimap_width;
        let minimap_x = self.width - minimap_w;
        let total_lines_f32 = total_lines as f32;

        let visible_minimap_lines = total_lines_f32.min(900.0);
        let minimap_line_h = (editor_height / (visible_minimap_lines + 2.0).max(1.0)).max(1.5);

        let max_minimap_scroll =
            ((total_lines_f32 + 2.0) * minimap_line_h - editor_height).max(0.0);
        let current_minimap_scroll = (scroll_ratio_y * max_minimap_scroll).round();

        let current_visible_top_line = render_scroll_y / self.line_height;
        let viewport_y = tab_bar_h
            + (current_visible_top_line * minimap_line_h - current_minimap_scroll).round();
        let visible_lines = editor_height / self.line_height;
        let viewport_h = if max_scroll <= 0.0 {
            editor_height
        } else {
            (visible_lines * minimap_line_h).max(4.0)
        };

        let view_bg = [
            self.theme.sel[0],
            self.theme.sel[1],
            self.theme.sel[2],
            0.15,
        ];
        let view_border = [self.theme.sel[0], self.theme.sel[1], self.theme.sel[2], 1.0];

        self.push_rect(minimap_x, viewport_y, minimap_w, viewport_h, view_bg);
        self.push_rect(minimap_x, viewport_y, minimap_w, 2.0, view_border);
        self.push_rect(
            minimap_x,
            viewport_y + viewport_h - 2.0,
            minimap_w,
            2.0,
            view_border,
        );
        self.push_rect(minimap_x, viewport_y, 2.0, viewport_h, view_border);
        self.flush();

        let map_bg = self.theme.minimap_bg;
        let mut current_y: f32 = 0.0;
        let mut phys_line = 0;
        let rect_h = minimap_line_h.ceil().max(1.0);

        let view_top = current_minimap_scroll;
        let view_bottom = current_minimap_scroll + editor_height;

        let (first, second) = editor.text_parts();
        let first_bytes = first.as_bytes();
        let second_bytes = second.as_bytes();
        let first_len = first.len();

        while phys_line < editor.line_offsets.len() {
            let start_byte = editor.line_offsets[phys_line];
            let is_folded = editor.folded_lines.contains(&phys_line)
                && editor.foldable_lines.contains_key(&phys_line);

            if current_y > view_bottom {
                break;
            }

            if current_y + minimap_line_h >= view_top {
                let mut end_byte = if phys_line + 1 < editor.line_offsets.len() {
                    editor.line_offsets[phys_line + 1]
                } else {
                    editor.len()
                };

                if is_folded {
                    end_byte -= 1;
                }

                let minimap_char_width = 1.5;
                let line_start_x = minimap_x + 5.0;
                let minimap_max_x = minimap_x + self.minimap_width - 5.0;

                let y1 = tab_bar_h + (current_y - current_minimap_scroll).round();
                let y2 = y1 + rect_h;

                let mut cur_byte_abs = start_byte;
                let mut cur_char_idx = 0;

                let mut span_idx_mini = match spans.binary_search_by_key(&start_byte, |s| s.start) {
                    Ok(idx) => idx,
                    Err(idx) => idx.saturating_sub(1),
                };

                while cur_byte_abs < end_byte {
                    // Find current span and color
                    while span_idx_mini < spans.len() && spans[span_idx_mini].end <= cur_byte_abs {
                        span_idx_mini += 1;
                    }
                    let (span_end, raw_color) = if span_idx_mini < spans.len() {
                        let sp = &spans[span_idx_mini];
                        if sp.start <= cur_byte_abs {
                            (sp.end.min(end_byte), sp.color)
                        } else {
                            (sp.start.min(end_byte), self.theme.fg)
                        }
                    } else {
                        (end_byte, self.theme.fg)
                    };
                    let color = [
                        raw_color[0] * 0.8 + map_bg[0] * 0.2,
                        raw_color[1] * 0.8 + map_bg[1] * 0.2,
                        raw_color[2] * 0.8 + map_bg[2] * 0.2,
                        1.0,
                    ];

                    let mut byte_in_span = cur_byte_abs;
                    let span_end_abs = span_end;

                    // Process this span in chunks of up to 96 chars
                    while byte_in_span < span_end_abs {
                        let quad_start_char_idx = cur_char_idx;
                        let mut masks: [u32; 3] = [0; 3];
                        let mut chars_in_mask = 0;
                        let mut bytes_processed = 0;

                        for _ in 0..96 {
                            let current_byte_to_check = byte_in_span + bytes_processed;
                            if current_byte_to_check >= span_end_abs {
                                break;
                            }

                            let b = if current_byte_to_check < first_len {
                                first_bytes[current_byte_to_check]
                            } else {
                                second_bytes[current_byte_to_check - first_len]
                            };

                            if !b.is_ascii_whitespace() {
                                let mask_idx = chars_in_mask / 32;
                                let bit_idx = chars_in_mask % 32;
                                masks[mask_idx] |= 1 << bit_idx;
                            }
                            chars_in_mask += 1;
                            bytes_processed += 1;
                        }

                        let x1 = line_start_x + quad_start_char_idx as f32 * minimap_char_width;
                        if x1 >= minimap_max_x {
                            cur_byte_abs = end_byte; // Fast-forward to end of line
                            break;
                        }

                        let quad_width =
                            (chars_in_mask as f32 * minimap_char_width).min(minimap_max_x - x1);

                        let is_empty = masks[0] == 0 && masks[1] == 0 && masks[2] == 0;

                        if quad_width > 0.01 && !is_empty {
                            let x2 = x1 + quad_width;
                            let sdf_params = [
                                f32::from_bits(masks[0]),
                                f32::from_bits(masks[1]),
                                f32::from_bits(masks[2]),
                            ];

                            let uv_x_end =
                                (quad_width / minimap_char_width).min(chars_in_mask as f32);

                            let v1 = Vertex {
                                pos: [x1, y1],
                                uv: [0.0, 0.0],
                                color,
                                mode: 7.0,
                                sdf_params,
                            };
                            let v2 = Vertex {
                                pos: [x2, y1],
                                uv: [uv_x_end, 0.0],
                                color,
                                mode: 7.0,
                                sdf_params,
                            };
                            let v3 = Vertex {
                                pos: [x2, y2],
                                uv: [uv_x_end, 0.0],
                                color,
                                mode: 7.0,
                                sdf_params,
                            };
                            let v4 = Vertex {
                                pos: [x1, y2],
                                uv: [0.0, 0.0],
                                color,
                                mode: 7.0,
                                sdf_params,
                            };

                            self.vertices.extend_from_slice(&[v1, v2, v3, v1, v3, v4]);
                            if self.vertices.len() >= crate::renderer::MAX_VERTICES - 6 {
                                self.flush();
                            }
                        }

                        byte_in_span += bytes_processed;
                        cur_char_idx += chars_in_mask;
                    }
                    if cur_byte_abs == end_byte {
                        break;
                    }
                    cur_byte_abs = span_end_abs;
                }
            }

            current_y += minimap_line_h;

            if is_folded {
                if let Some(&fold_end) = editor.foldable_lines.get(&phys_line) {
                    phys_line = fold_end;
                }
            }
            phys_line += 1;
        }

        self.flush();

        let y_cursor = tab_bar_h
            + (visible_cursor_line as f32 * minimap_line_h - current_minimap_scroll).round();
        self.push_rect(
            minimap_x,
            y_cursor,
            minimap_w,
            2.0,
            self.theme.minimap_cursor,
        );
    }
}
