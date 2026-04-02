use crate::editor::Editor;
use crate::renderer::{Renderer, Vertex, VisualLine};
use glow::HasContext;

impl Renderer {
    pub fn update_cache(&mut self, editor: &Editor, is_resizing: bool) {
        if self.width < 10.0 {
            return;
        }

        let size_changed = !is_resizing
            && ((self.last_height - self.height).abs() > 0.5
                || (self.last_width - self.width).abs() > 0.5);

        let needs_update = self.last_editor_version != editor.version || size_changed;

        if needs_update || self.visual_lines.is_empty() {
            self.visual_lines.clear();
            self.visual_lines.push(VisualLine {
                byte_idx: 0,
                physical_line: 1,
                is_soft_wrap: false,
            });

            let (first, second) = editor.text_parts();
            let minimap_w = self.minimap_width;
            let max_x = (self.width - minimap_w - 20.0).max(self.left_padding + 100.0);
            let mut x = self.left_padding;
            let mut char_idx = 0;
            let mut physical_line = 1;

            for part in [first, second] {
                for c in part.chars() {
                    let is_newline = c == '\n';
                    let is_hidden = c == '\u{FE0F}' || c == '\u{200D}';
                    let adv = if is_newline || is_hidden {
                        0.0
                    } else {
                        self.char_advance(c)
                    };

                    if !is_newline && x + adv > max_x && x > self.left_padding {
                        self.visual_lines.push(VisualLine {
                            byte_idx: char_idx,
                            physical_line,
                            is_soft_wrap: true,
                        });
                        x = self.left_padding;
                    }
                    if is_newline {
                        physical_line += 1;
                        self.visual_lines.push(VisualLine {
                            byte_idx: char_idx + c.len_utf8(),
                            physical_line,
                            is_soft_wrap: false,
                        });
                        x = self.left_padding;
                    } else {
                        x += adv;
                    }
                    char_idx += c.len_utf8();
                }
            }

            self.last_editor_version = editor.version;
            self.last_height = self.height;
            self.last_width = self.width;
        }
    }

    pub fn measure_width(&mut self, first: &str, second: &str, start: usize, end: usize) -> f32 {
        let mut w = 0.0;
        let mut current = start;
        let first_len = first.len();
        while current < end {
            let s = if current < first_len {
                let end_chunk = end.min(first_len);
                let chunk = &first[current..end_chunk];
                current = end_chunk;
                chunk
            } else {
                let c_start = current - first_len;
                let c_end = end - first_len;
                let chunk = &second[c_start..c_end];
                current = end;
                chunk
            };
            for c in s.chars() {
                if c != '\n' && c != '\u{FE0F}' && c != '\u{200D}' {
                    w += self.char_advance(c);
                }
            }
        }
        w
    }

    pub fn get_cursor_xy(&mut self, editor: &Editor) -> (f32, f32) {
        self.update_cache(editor, false);
        let idx = match self
            .visual_lines
            .binary_search_by_key(&editor.cursor, |v| v.byte_idx)
        {
            Ok(i) => i,
            Err(i) => {
                if i > 0 {
                    i - 1
                } else {
                    0
                }
            }
        };
        let start_info = self.visual_lines[idx];
        let y = self.baseline_offset + (idx as f32) * self.line_height;
        let (first, second) = editor.text_parts();
        let x = self.left_padding
            + self.measure_width(first, second, start_info.byte_idx, editor.cursor);
        (x, y)
    }

    pub fn get_byte_at_xy(&mut self, editor: &Editor, target_x: f32, target_y: f32) -> usize {
        self.update_cache(editor, false);
        let line_idx = (target_y / self.line_height).floor() as isize;
        let line_idx = line_idx.max(0) as usize;

        if line_idx >= self.visual_lines.len() {
            return editor.len();
        }

        let start_info = self.visual_lines[line_idx];
        let end_byte = if line_idx + 1 < self.visual_lines.len() {
            self.visual_lines[line_idx + 1].byte_idx
        } else {
            editor.len()
        };

        let mut current_x = self.left_padding;
        let mut last_valid_byte = start_info.byte_idx;
        let mut current = start_info.byte_idx;
        let (first, second) = editor.text_parts();
        let first_len = first.len();

        while current < end_byte {
            let s = if current < first_len {
                let end_chunk = end_byte.min(first_len);
                let chunk = &first[current..end_chunk];
                current = end_chunk;
                chunk
            } else {
                let c_start = current - first_len;
                let c_end = end_byte - first_len;
                let chunk = &second[c_start..c_end];
                current = end_byte;
                chunk
            };

            for c in s.chars() {
                let is_hidden = c == '\u{FE0F}' || c == '\u{200D}';
                let is_newline = c == '\n';
                let adv = if is_newline || is_hidden {
                    0.0
                } else {
                    self.char_advance(c)
                };

                if target_x <= current_x + (adv / 2.0) && !is_hidden && !is_newline {
                    return last_valid_byte;
                }
                if is_newline {
                    return last_valid_byte;
                }

                current_x += adv;
                last_valid_byte += c.len_utf8();
            }
        }

        if last_valid_byte > end_byte {
            end_byte
        } else {
            last_valid_byte
        }
    }

    pub fn get_max_scroll(&mut self, editor: &Editor, window_height: f32) -> f32 {
        self.update_cache(editor, false);
        let total_height = self.visual_lines.len() as f32 * self.line_height;
        let raw_max = (total_height - window_height + self.line_height * 2.0).max(0.0);
        (raw_max / self.line_height).ceil() * self.line_height
    }

    pub fn flush(&mut self) {
        if self.vertices.is_empty() {
            return;
        }
        unsafe {
            let proj = [
                2.0 / self.width,
                0.0,
                0.0,
                0.0,
                0.0,
                -2.0 / self.height,
                0.0,
                0.0,
                0.0,
                0.0,
                -1.0,
                0.0,
                -1.0,
                1.0,
                0.0,
                1.0,
            ];
            let proj_loc = self.gl.get_uniform_location(self.program, "proj");
            self.gl
                .uniform_matrix_4_f32_slice(proj_loc.as_ref(), false, &proj);
            self.gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.vbo));
            self.gl.buffer_data_u8_slice(
                glow::ARRAY_BUFFER,
                bytemuck::cast_slice(&self.vertices),
                glow::DYNAMIC_DRAW,
            );
            self.gl
                .draw_arrays(glow::TRIANGLES, 0, self.vertices.len() as i32);
        }
        self.vertices.clear();
    }

    pub fn push_vertical_gradient(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        top: [f32; 4],
        bottom: [f32; 4],
    ) {
        let x1 = x.round();
        let y1 = y.round();
        let x2 = (x + w).round();
        let y2 = (y + h).round();

        let v1 = Vertex {
            pos: [x1, y1],
            uv: [-1.0, -1.0],
            color: top,
            is_emoji: 0.0,
        };
        let v2 = Vertex {
            pos: [x2, y1],
            uv: [-1.0, -1.0],
            color: top,
            is_emoji: 0.0,
        };
        let v3 = Vertex {
            pos: [x2, y2],
            uv: [-1.0, -1.0],
            color: bottom,
            is_emoji: 0.0,
        };
        let v4 = Vertex {
            pos: [x1, y2],
            uv: [-1.0, -1.0],
            color: bottom,
            is_emoji: 0.0,
        };
        self.vertices.extend_from_slice(&[v1, v2, v3, v1, v3, v4]);
    }

    pub fn draw_string(&mut self, text: &str, mut x: f32, y: f32, color: [f32; 4]) {
        for c in text.chars() {
            if c == '\n' || c == '\r' || c == '\u{FE0F}' || c == '\u{200D}' {
                continue;
            }
            if let Some(g) = self.get_glyph(c) {
                self.push_quad(
                    x + g.offset_x,
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
                x += g.advance;
            }
        }
    }

    pub fn draw_string_scaled(
        &mut self,
        text: &str,
        mut x: f32,
        y: f32,
        color: [f32; 4],
        scale: f32,
    ) {
        for c in text.chars() {
            if c == '\n' || c == '\r' || c == '\u{FE0F}' || c == '\u{200D}' {
                continue;
            }
            if let Some(g) = self.get_ui_glyph(c) {
                self.push_quad(
                    (x + g.offset_x * scale).round(),
                    (y - g.offset_y * scale).round(),
                    g.width * scale,
                    g.height * scale,
                    g.u,
                    g.v,
                    g.uw,
                    g.vh,
                    color,
                    g.is_emoji,
                );
                x += g.advance * scale;
            }
        }
    }

    pub fn push_rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, color: [f32; 4]) {
        let overlap = 0.5;
        self.push_rect(x + r - overlap, y, w - 2.0 * r + 2.0 * overlap, h, color);
        self.push_rect(x, y + r - overlap, r, h - 2.0 * r + 2.0 * overlap, color);
        self.push_rect(
            x + w - r,
            y + r - overlap,
            r,
            h - 2.0 * r + 2.0 * overlap,
            color,
        );

        let segments = 32;
        let add_corner = |vertices: &mut Vec<Vertex>, cx: f32, cy: f32, start_angle: f32| {
            for i in 0..segments {
                let a1 = start_angle + (i as f32 * std::f32::consts::PI / 2.0 / segments as f32);
                let a2 =
                    start_angle + ((i + 1) as f32 * std::f32::consts::PI / 2.0 / segments as f32);
                vertices.push(Vertex {
                    pos: [cx, cy],
                    uv: [-1.0, -1.0],
                    color,
                    is_emoji: 0.0,
                });
                vertices.push(Vertex {
                    pos: [cx + a1.cos() * r, cy + a1.sin() * r],
                    uv: [-1.0, -1.0],
                    color,
                    is_emoji: 0.0,
                });
                vertices.push(Vertex {
                    pos: [cx + a2.cos() * r, cy + a2.sin() * r],
                    uv: [-1.0, -1.0],
                    color,
                    is_emoji: 0.0,
                });
            }
        };

        add_corner(&mut self.vertices, x + w - r, y + h - r, 0.0);
        add_corner(
            &mut self.vertices,
            x + r,
            y + h - r,
            std::f32::consts::PI / 2.0,
        );
        add_corner(&mut self.vertices, x + r, y + r, std::f32::consts::PI);
        add_corner(
            &mut self.vertices,
            x + w - r,
            y + r,
            3.0 * std::f32::consts::PI / 2.0,
        );
    }
}
