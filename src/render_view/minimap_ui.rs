use crate::editor::Editor;
use crate::highlighter::ColorSpan;
use crate::renderer::{Renderer, Vertex};

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

        let minimap_line_h = (editor_height / (total_lines_f32 + 2.0).max(200.0))
            .max(editor_height / 1250.0)
            .max(1.5);

        let max_minimap_scroll =
            ((total_lines_f32 + 2.0) * minimap_line_h - editor_height).max(0.0);
        let current_minimap_scroll = (scroll_ratio_y * max_minimap_scroll).round();

        let current_visible_top_line = render_scroll_y / self.line_height;
        let viewport_y = tab_bar_h
            + (current_visible_top_line * minimap_line_h - current_minimap_scroll).round();
        let visible_lines = editor_height / self.line_height;
        let max_viewport_lines = visible_lines.min(total_lines_f32 + 2.0);
        let viewport_h = (max_viewport_lines * minimap_line_h).max(4.0);

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

                let mut current_x = minimap_x + 5.0;
                let mut cur_byte = start_byte;

                let mut span_idx_mini = match spans.binary_search_by_key(&cur_byte, |s| s.start) {
                    Ok(idx) => idx,
                    Err(idx) => idx.saturating_sub(1),
                };

                let y1 = tab_bar_h + (current_y - current_minimap_scroll).round();
                let y2 = y1 + rect_h;

                while cur_byte < end_byte {
                    let text_chunk = if cur_byte < first_len {
                        &first[cur_byte..end_byte.min(first_len)]
                    } else {
                        &second[cur_byte - first_len..end_byte - first_len]
                    };

                    let mut spaces_len = 0;
                    for c in text_chunk.chars() {
                        if c == ' ' || c == '\t' || c == '\n' || c == '\r' {
                            spaces_len += c.len_utf8();
                        } else {
                            break;
                        }
                    }

                    if spaces_len > 0 {
                        let capped_spaces = spaces_len.min(5);
                        current_x += 1.5 * (capped_spaces as f32);
                        cur_byte += spaces_len;
                        if current_x >= minimap_x + minimap_w - 5.0 {
                            break;
                        }
                        continue;
                    }

                    while span_idx_mini < spans.len() && spans[span_idx_mini].end <= cur_byte {
                        span_idx_mini += 1;
                    }

                    let (span_end, raw_color) = if span_idx_mini < spans.len() {
                        let sp = &spans[span_idx_mini];
                        if sp.start <= cur_byte {
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

                    let mut word_len = 0;
                    for c in text_chunk.chars() {
                        if cur_byte + word_len >= span_end
                            || c == ' '
                            || c == '\t'
                            || c == '\n'
                            || c == '\r'
                        {
                            break;
                        }
                        word_len += c.len_utf8();
                    }

                    if word_len == 0 {
                        if let Some(c) = text_chunk.chars().next() {
                            word_len = c.len_utf8();
                        }
                    }

                    let w = (word_len as f32 * 1.5).min(minimap_x + minimap_w - 5.0 - current_x);

                    if w > 0.0 {
                        let x1 = current_x.round();
                        let x2 = (current_x + w).round();

                        let sdf = [0.0, 0.0, 0.0];
                        let v1 = Vertex {
                            pos: [x1, y1],
                            uv: [-1.0, -1.0],
                            color,
                            mode: 2.0,
                            sdf_params: sdf,
                        };
                        let v2 = Vertex {
                            pos: [x2, y1],
                            uv: [-1.0, -1.0],
                            color,
                            mode: 2.0,
                            sdf_params: sdf,
                        };
                        let v3 = Vertex {
                            pos: [x2, y2],
                            uv: [-1.0, -1.0],
                            color,
                            mode: 2.0,
                            sdf_params: sdf,
                        };
                        let v4 = Vertex {
                            pos: [x1, y2],
                            uv: [-1.0, -1.0],
                            color,
                            mode: 2.0,
                            sdf_params: sdf,
                        };

                        self.vertices.extend_from_slice(&[v1, v2, v3, v1, v3, v4]);
                        if self.vertices.len() >= crate::renderer::MAX_VERTICES - 6 {
                            self.flush();
                        }
                        current_x += w;
                    }

                    cur_byte += word_len;
                    if current_x >= minimap_x + minimap_w - 5.0 {
                        break;
                    }
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
