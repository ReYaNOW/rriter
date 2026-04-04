// --- START OF FILE core_text.rs ---
use crate::editor::Editor;
use crate::renderer::{Renderer, Vertex, VisualLine};
use glow::HasContext;

impl Renderer {
    pub fn update_cache(
        &mut self,
        editor: &Editor,
        scroll_x: f32,
        scroll_y: f32,
        is_resizing: bool,
    ) {
        if self.width < 10.0 {
            return;
        }

        let size_changed = !is_resizing
            && ((self.last_height - self.height).abs() > 0.5
                || (self.last_width - self.width).abs() > 0.5);

        let needs_update = self.last_editor_version != editor.version
            || size_changed
            || (self.last_scroll_y - scroll_y).abs() > 0.5
            || (self.last_scroll_x - scroll_x).abs() > 0.5;

        if !needs_update && !self.visual_lines.is_empty() {
            return;
        }

        self.visual_lines.clear();

        let start_line = (scroll_y / self.line_height).floor().max(0.0) as usize;
        let start_line = start_line.min(editor.line_offsets.len().saturating_sub(1));

        let visible_lines = (self.height / self.line_height).ceil() as usize + 15;
        let end_line = (start_line + visible_lines).min(editor.line_offsets.len());

        let (first, second) = editor.text_parts();
        let first_len = first.len();

        for phys_line in start_line..end_line {
            let start_byte = editor.line_offsets[phys_line];
            let end_byte = if phys_line + 1 < editor.line_offsets.len() {
                editor.line_offsets[phys_line + 1]
            } else {
                editor.len()
            };

            let mut whitespace_px_width = 0.0;
            let mut text_px_width = 0.0;
            let mut in_whitespace = true;
            let mut current = start_byte;

            while current < end_byte {
                let chunk = if current < first_len {
                    let end_chunk = end_byte.min(first_len);
                    &first[current..end_chunk]
                } else {
                    let c_start = current - first_len;
                    let c_end = end_byte - first_len;
                    &second[c_start..c_end]
                };

                let mut out_of_bounds = false;

                for c in chunk.chars() {
                    let is_newline = c == '\n';
                    let is_hidden = c == '\u{FE0F}' || c == '\u{200D}';
                    let adv = if is_newline || is_hidden {
                        0.0
                    } else {
                        self.char_advance(c)
                    };

                    if in_whitespace && (c == ' ' || c == '\t') {
                        whitespace_px_width += adv;
                    } else {
                        in_whitespace = false;
                        if !is_hidden && !is_newline {
                            text_px_width += adv;
                        }
                    }

                    // Даем запас в 50000 пикселей (чтобы горизонтальный скроллбар был адекватным),
                    // но прерываемся на безумно длинных минифицированных строках
                    if self.left_padding + whitespace_px_width + text_px_width > 50000.0 {
                        out_of_bounds = true;
                        break;
                    }
                }

                if out_of_bounds {
                    break;
                }
                current += chunk.len();
            }

            let y_offset = (phys_line as f32) * self.line_height;

            self.visual_lines.push(VisualLine {
                byte_idx: start_byte,
                physical_line: phys_line + 1,
                is_soft_wrap: false,
                whitespace_px_width,
                text_px_width,
                y_offset,
            });
        }

        self.last_editor_version = editor.version;
        self.last_height = self.height;
        self.last_width = self.width;
        self.last_scroll_y = scroll_y;
        self.last_scroll_x = scroll_x;
    }

    // Возвращает координаты в ЭКРАННОМ ПРОСТРАНСТВЕ (с учетом scroll_x)
    pub fn get_cursor_xy(&mut self, editor: &Editor) -> (f32, f32) {
        let phys_line = editor
            .line_offsets
            .partition_point(|&o| o <= editor.cursor)
            .saturating_sub(1);
        let y = self.baseline_offset + (phys_line as f32) * self.line_height;
        let line_start = editor.line_offsets[phys_line];
        let (first, second) = editor.text_parts();

        let x_absolute =
            self.left_padding + self.measure_width(first, second, line_start, editor.cursor);
        (x_absolute - self.last_scroll_x, y)
    }

    // target_x подается в ЭКРАННЫХ КООРДИНАТАХ
    pub fn get_byte_at_xy(&mut self, editor: &Editor, target_x: f32, target_y: f32) -> usize {
        let phys_line = (target_y / self.line_height).floor() as isize;
        let phys_line = phys_line.max(0) as usize;

        if phys_line >= editor.line_offsets.len() {
            return editor.len();
        }

        let start_byte = editor.line_offsets[phys_line];
        let end_byte = if phys_line + 1 < editor.line_offsets.len() {
            editor.line_offsets[phys_line + 1]
        } else {
            editor.len()
        };

        // Стартуем не от края экрана, а от отступа с учетом прокрутки
        let mut current_x = self.left_padding - self.last_scroll_x;
        let mut last_valid_byte = start_byte;
        let mut current = start_byte;
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

    pub fn get_max_scroll(&mut self, editor: &Editor, window_height: f32) -> f32 {
        let total_height = editor.line_offsets.len() as f32 * self.line_height;
        let raw_max = (total_height - window_height + self.line_height * 2.0).max(0.0);
        (raw_max / self.line_height).ceil() * self.line_height
    }

    pub fn flush(&mut self) {
        if self.vertices.is_empty() {
            return;
        }

        let vertex_count = self.vertices.len().min(crate::renderer::MAX_VERTICES);

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

            self.gl.buffer_sub_data_u8_slice(
                glow::ARRAY_BUFFER,
                0,
                bytemuck::cast_slice(&self.vertices[..vertex_count]),
            );

            self.gl.draw_arrays(glow::TRIANGLES, 0, vertex_count as i32);
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
        let cx = x + w / 2.0;
        let cy = y + h / 2.0;
        let center = Vertex {
            pos: [cx, cy],
            uv: [-1.0, -1.0],
            color,
            is_emoji: 0.0,
        };

        let segments = 32;
        self.temp_edge_buffer.clear();

        let mut add_arc = |corner_cx: f32, corner_cy: f32, start_angle: f32| {
            for i in 0..=segments {
                let a = start_angle + (i as f32 * std::f32::consts::PI / 2.0 / segments as f32);
                self.temp_edge_buffer.push(Vertex {
                    pos: [corner_cx + a.cos() * r, corner_cy + a.sin() * r],
                    uv: [-1.0, -1.0],
                    color,
                    is_emoji: 0.0,
                });
            }
        };

        add_arc(x + w - r, y + h - r, 0.0);
        add_arc(x + r, y + h - r, std::f32::consts::PI / 2.0);
        add_arc(x + r, y + r, std::f32::consts::PI);
        add_arc(x + w - r, y + r, 3.0 * std::f32::consts::PI / 2.0);

        for i in 0..self.temp_edge_buffer.len() {
            let next_i = (i + 1) % self.temp_edge_buffer.len();
            self.vertices.push(center);
            self.vertices.push(self.temp_edge_buffer[i]);
            self.vertices.push(self.temp_edge_buffer[next_i]);
        }
    }
}
