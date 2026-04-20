use crate::editor::Editor;
use crate::renderer::{Renderer, Vertex, VisualLine};
use glow::HasContext;

impl Renderer {
    pub fn update_cache(
        &mut self,
        editor: &Editor,
        scroll_x: f32,
        scroll_y: f32,
        _is_resizing: bool,
    ) {
        if self.width < 10.0 {
            return;
        }

        self.visual_lines.clear();

        let (first, second) = editor.text_parts();
        let first_len = first.len();

        let mut current_y = 0.0;
        let mut phys_line = 0;

        while phys_line < editor.line_offsets.len() {
            let is_folded = editor.folded_lines.contains(&phys_line)
                && editor.foldable_lines.contains_key(&phys_line);

            if current_y + self.line_height > scroll_y - self.line_height * 5.0
                && current_y < scroll_y + self.height + self.line_height * 5.0
            {
                let start_byte = editor.line_offsets[phys_line];
                let mut end_byte = if phys_line + 1 < editor.line_offsets.len() {
                    editor.line_offsets[phys_line + 1]
                } else {
                    editor.len()
                };

                if is_folded && end_byte > start_byte && editor.byte_at(end_byte - 1) == b'\n' {
                    end_byte -= 1;
                    if end_byte > start_byte && editor.byte_at(end_byte - 1) == b'\r' {
                        end_byte -= 1;
                    }
                }

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

                let mut fold_suffix = ['\0'; 4];
                let mut fold_suffix_len = 0;
                if is_folded {
                    if let Some(&fold_end) = editor.foldable_lines.get(&phys_line) {
                        let start_line_start = editor.line_offsets[phys_line];
                        let start_line_end = if phys_line + 1 < editor.line_offsets.len() {
                            editor.line_offsets[phys_line + 1]
                        } else {
                            editor.len()
                        };

                        let mut p_start = start_line_end;
                        let mut last_start_char = 0;
                        while p_start > start_line_start {
                            p_start -= 1;
                            let b = editor.byte_at(p_start);
                            if b != b' ' && b != b'\t' && b != b'\r' && b != b'\n' {
                                last_start_char = b;
                                break;
                            }
                        }

                        if last_start_char == b'{'
                            || last_start_char == b'['
                            || last_start_char == b'('
                        {
                            let expected_close = match last_start_char {
                                b'{' => b'}',
                                b'[' => b']',
                                b'(' => b')',
                                _ => 0,
                            };

                            let end_line_start = editor.line_offsets[fold_end];
                            let end_line_end = if fold_end + 1 < editor.line_offsets.len() {
                                editor.line_offsets[fold_end + 1]
                            } else {
                                editor.len()
                            };

                            let mut p = end_line_end;
                            while p > end_line_start {
                                p -= 1;
                                let b = editor.byte_at(p);
                                if b != b' ' && b != b'\t' && b != b'\r' && b != b'\n' {
                                    p += 1;
                                    break;
                                }
                            }
                            let mut suffix_bytes = Vec::new();
                            let mut p_scan = p;
                            while p_scan > end_line_start && suffix_bytes.len() < 4 {
                                p_scan -= 1;
                                let b = editor.byte_at(p_scan);
                                if b == b' ' || b == b'\t' {
                                    break;
                                }
                                suffix_bytes.push(b);
                            }
                            suffix_bytes.reverse();
                            if suffix_bytes.contains(&expected_close) {
                                if let Some(pos) =
                                    suffix_bytes.iter().position(|&x| x == expected_close)
                                {
                                    for &b in &suffix_bytes[pos..] {
                                        if fold_suffix_len < 4 {
                                            fold_suffix[fold_suffix_len as usize] = b as char;
                                            fold_suffix_len += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                let dots_width = if is_folded {
                    let mut w = self.measure_ui_width("...", 1.0) + 10.0 * self.scale_factor;
                    for i in 0..fold_suffix_len {
                        w += self.char_advance(fold_suffix[i as usize]);
                    }
                    w
                } else {
                    0.0
                };

                self.visual_lines.push(VisualLine {
                    byte_idx: start_byte,
                    physical_line: phys_line + 1,
                    is_soft_wrap: false,
                    whitespace_px_width,
                    text_px_width: text_px_width + dots_width,
                    y_offset: current_y,
                    is_folded,
                    fold_suffix,
                    fold_suffix_len,
                });
            }

            current_y += self.line_height;

            if is_folded {
                if let Some(&fold_end) = editor.foldable_lines.get(&phys_line) {
                    phys_line = fold_end;
                }
            }
            phys_line += 1;
        }

        self.last_editor_version = editor.version;
        self.last_height = self.height;
        self.last_width = self.width;
        self.last_scroll_y = scroll_y;
        self.last_scroll_x = scroll_x;
    }

    pub fn get_cursor_xy(&mut self, editor: &Editor) -> (f32, f32) {
        let mut current_y = self.baseline_offset;
        let mut phys_line = 0;

        while phys_line < editor.line_offsets.len() {
            let is_folded = editor.folded_lines.contains(&phys_line)
                && editor.foldable_lines.contains_key(&phys_line);
            let fold_end_line = if is_folded {
                editor
                    .foldable_lines
                    .get(&phys_line)
                    .copied()
                    .unwrap_or(phys_line)
            } else {
                phys_line
            };

            let line_start = editor.line_offsets[phys_line];
            let next_line_start = if fold_end_line + 1 < editor.line_offsets.len() {
                editor.line_offsets[fold_end_line + 1]
            } else {
                editor.len() + 1
            };

            if editor.cursor >= line_start && editor.cursor < next_line_start
                || (editor.cursor == editor.len() && next_line_start > editor.len())
            {
                let (first, second) = editor.text_parts();
                let mut x = self.left_padding;

                if is_folded {
                    let first_line_end = if phys_line + 1 < editor.line_offsets.len() {
                        editor.line_offsets[phys_line + 1].saturating_sub(1)
                    } else {
                        editor.len()
                    };

                    if editor.cursor >= first_line_end {
                        x += self.measure_width(first, second, line_start, first_line_end);

                        let mut dots_w =
                            self.measure_ui_width("...", 1.0) + 10.0 * self.scale_factor;

                        let end_line_start = editor.line_offsets[fold_end_line];
                        let end_line_end = if fold_end_line + 1 < editor.line_offsets.len() {
                            editor.line_offsets[fold_end_line + 1]
                        } else {
                            editor.len()
                        };
                        let mut fold_suffix = ['\0'; 4];
                        let mut fold_suffix_len = 0;
                        let start_line_start = editor.line_offsets[phys_line];
                        let start_line_end = if phys_line + 1 < editor.line_offsets.len() {
                            editor.line_offsets[phys_line + 1]
                        } else {
                            editor.len()
                        };

                        let mut p_start = start_line_end;
                        let mut last_start_char = 0;
                        while p_start > start_line_start {
                            p_start -= 1;
                            let b = editor.byte_at(p_start);
                            if b != b' ' && b != b'\t' && b != b'\r' && b != b'\n' {
                                last_start_char = b;
                                break;
                            }
                        }

                        if last_start_char == b'{'
                            || last_start_char == b'['
                            || last_start_char == b'('
                        {
                            let expected_close = match last_start_char {
                                b'{' => b'}',
                                b'[' => b']',
                                b'(' => b')',
                                _ => 0,
                            };

                            let mut p = end_line_end;
                            while p > end_line_start {
                                p -= 1;
                                let b = editor.byte_at(p);
                                if b != b' ' && b != b'\t' && b != b'\r' && b != b'\n' {
                                    p += 1;
                                    break;
                                }
                            }
                            let mut suffix_bytes = Vec::new();
                            let mut p_scan = p;
                            while p_scan > end_line_start && suffix_bytes.len() < 4 {
                                p_scan -= 1;
                                let b = editor.byte_at(p_scan);
                                if b == b' ' || b == b'\t' {
                                    break;
                                }
                                suffix_bytes.push(b);
                            }
                            suffix_bytes.reverse();
                            if suffix_bytes.contains(&expected_close) {
                                if let Some(pos) =
                                    suffix_bytes.iter().position(|&x| x == expected_close)
                                {
                                    for &b in &suffix_bytes[pos..] {
                                        if fold_suffix_len < 4 {
                                            fold_suffix[fold_suffix_len as usize] = b as char;
                                            fold_suffix_len += 1;
                                        }
                                    }
                                }
                            }
                        }
                        for i in 0..fold_suffix_len {
                            dots_w += self.char_advance(fold_suffix[i as usize]);
                        }

                        x += dots_w;
                    } else {
                        x += self.measure_width(first, second, line_start, editor.cursor);
                    }
                } else {
                    x += self.measure_width(first, second, line_start, editor.cursor);
                }

                return (x - self.last_scroll_x, current_y);
            }

            current_y += self.line_height;
            phys_line = fold_end_line + 1;
        }

        (self.left_padding - self.last_scroll_x, current_y)
    }

    pub fn get_byte_at_xy(&mut self, editor: &Editor, target_x: f32, target_y: f32) -> usize {
        let mut current_y = 0.0;
        let mut phys_line = 0;
        let mut target_phys_line = 0;

        while phys_line < editor.line_offsets.len() {
            if target_y >= current_y && target_y < current_y + self.line_height {
                target_phys_line = phys_line;
                break;
            }
            current_y += self.line_height;
            if editor.folded_lines.contains(&phys_line)
                && editor.foldable_lines.contains_key(&phys_line)
            {
                if let Some(&end_l) = editor.foldable_lines.get(&phys_line) {
                    phys_line = end_l;
                }
            }
            phys_line += 1;
        }

        if target_phys_line == 0 && phys_line >= editor.line_offsets.len() {
            target_phys_line = editor.line_offsets.len().saturating_sub(1);
        }

        let start_byte = editor.line_offsets[target_phys_line];
        let mut end_byte = if target_phys_line + 1 < editor.line_offsets.len() {
            editor.line_offsets[target_phys_line + 1]
        } else {
            editor.len()
        };

        let is_folded = editor.folded_lines.contains(&target_phys_line)
            && editor.foldable_lines.contains_key(&target_phys_line);
        if is_folded && end_byte > start_byte && editor.byte_at(end_byte - 1) == b'\n' {
            end_byte -= 1;
            if end_byte > start_byte && editor.byte_at(end_byte - 1) == b'\r' {
                end_byte -= 1;
            }
        }

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

        if is_folded {
            if let Some(&fold_end) = editor.foldable_lines.get(&target_phys_line) {
                return if fold_end + 1 < editor.line_offsets.len() {
                    editor.line_offsets[fold_end + 1].saturating_sub(1)
                } else {
                    editor.len()
                };
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
        let mut phys_line = 0;
        let mut lines_count = 0;
        while phys_line < editor.line_offsets.len() {
            lines_count += 1;
            if editor.folded_lines.contains(&phys_line)
                && editor.foldable_lines.contains_key(&phys_line)
            {
                if let Some(&end_l) = editor.foldable_lines.get(&phys_line) {
                    phys_line = end_l;
                }
            }
            phys_line += 1;
        }
        let total_height = lines_count as f32 * self.line_height;
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
        let x2 = x1 + w.round();
        let y2 = y1 + h.round();

        let sdf_params = [0.0, 0.0, 0.0];

        let v1 = Vertex {
            pos: [x1, y1],
            uv: [0.0, 0.0],
            color: top,
            mode: 2.0,
            sdf_params,
        };
        let v2 = Vertex {
            pos: [x2, y1],
            uv: [0.0, 0.0],
            color: top,
            mode: 2.0,
            sdf_params,
        };
        let v3 = Vertex {
            pos: [x2, y2],
            uv: [0.0, 0.0],
            color: bottom,
            mode: 2.0,
            sdf_params,
        };
        let v4 = Vertex {
            pos: [x1, y2],
            uv: [0.0, 0.0],
            color: bottom,
            mode: 2.0,
            sdf_params,
        };

        self.vertices.extend_from_slice(&[v1, v2, v3, v1, v3, v4]);
    }

    pub fn push_horizontal_gradient(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        left: [f32; 4],
        right: [f32; 4],
    ) {
        let x1 = x.round();
        let y1 = y.round();
        let x2 = x1 + w.round();
        let y2 = y1 + h.round();

        let sdf_params = [0.0, 0.0, 0.0];

        let v1 = Vertex {
            pos: [x1, y1],
            uv: [0.0, 0.0],
            color: left,
            mode: 2.0,
            sdf_params,
        };
        let v2 = Vertex {
            pos: [x2, y1],
            uv: [0.0, 0.0],
            color: right,
            mode: 2.0,
            sdf_params,
        };
        let v3 = Vertex {
            pos: [x2, y2],
            uv: [0.0, 0.0],
            color: right,
            mode: 2.0,
            sdf_params,
        };
        let v4 = Vertex {
            pos: [x1, y2],
            uv: [0.0, 0.0],
            color: left,
            mode: 2.0,
            sdf_params,
        };

        self.vertices.extend_from_slice(&[v1, v2, v3, v1, v3, v4]);
    }

            pub fn draw_string(&mut self, text: &str, mut x: f32, y: f32, color: [f32; 4]) {
        for c in text.chars() {
            if c == '\n' || c == '\r' || c == '\u{FE0F}' || c == '\u{200D}' {
                continue;
            }
            if let Some(g) = self.get_glyph(c) {
                let q_x = (x + g.offset_x).round();
                let q_y = (y - g.offset_y).round();
                let q_w = (x + g.offset_x + g.width).round() - q_x;
                let q_h = (y - g.offset_y + g.height).round() - q_y;
                self.push_quad(
                    q_x,
                    q_y,
                    q_w,
                    q_h,
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
                let q_x = (x + g.offset_x * scale).round();
                let q_y = (y - g.offset_y * scale).round();
                let q_w = (x + g.offset_x * scale + g.width * scale).round() - q_x;
                let q_h = (y - g.offset_y * scale + g.height * scale).round() - q_y;
                self.push_quad(
                    q_x,
                    q_y,
                    q_w,
                    q_h,
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

            pub fn draw_string_mono_scaled(
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
            if let Some(g) = self.get_glyph(c) {
                let q_x = (x + g.offset_x * scale).round();
                let q_y = (y - g.offset_y * scale).round();
                let q_w = (x + g.offset_x * scale + g.width * scale).round() - q_x;
                let q_h = (y - g.offset_y * scale + g.height * scale).round() - q_y;
                self.push_quad(
                    q_x,
                    q_y,
                    q_w,
                    q_h,
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

    pub fn measure_mono_width(&mut self, text: &str, scale: f32) -> f32 {
        let mut w = 0.0;
        for c in text.chars() {
            if c == '\n' || c == '\r' || c == '\u{FE0F}' || c == '\u{200D}' {
                continue;
            }
            w += self.char_advance(c) * scale;
        }
        w
    }

    pub fn push_rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, color: [f32; 4]) {
        let w_round = w.round();
        let h_round = h.round();
        let x1 = x.round();
        let y1 = y.round();
        let x2 = x1 + w_round;
        let y2 = y1 + h_round;

        let half_w = w_round / 2.0;
        let half_h = h_round / 2.0;
        let sdf_params = [half_w, half_h, r];

        let v1 = Vertex {
            pos: [x1, y1],
            uv: [-half_w, -half_h],
            color,
            mode: 3.0,
            sdf_params,
        };
        let v2 = Vertex {
            pos: [x2, y1],
            uv: [half_w, -half_h],
            color,
            mode: 3.0,
            sdf_params,
        };
        let v3 = Vertex {
            pos: [x2, y2],
            uv: [half_w, half_h],
            color,
            mode: 3.0,
            sdf_params,
        };
        let v4 = Vertex {
            pos: [x1, y2],
            uv: [-half_w, half_h],
            color,
            mode: 3.0,
            sdf_params,
        };

        self.vertices.extend_from_slice(&[v1, v2, v3, v1, v3, v4]);
    }
}
