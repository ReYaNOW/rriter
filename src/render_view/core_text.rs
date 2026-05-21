use super::editor_text_layer::folded_import_display_end;
use crate::editor::Editor;
use crate::renderer::{Renderer, Vertex, VisualLine, glyph_quad_rect};
use glow::HasContext;

fn code_end_before_line_comment(editor: &Editor, line_start: usize, line_end: usize) -> usize {
    let mut p = line_start;
    let mut code_end = line_end;
    while p < line_end {
        let b = editor.byte_at(p);
        if b == b'#' {
            code_end = p;
            break;
        }
        if b == b'/' && p + 1 < line_end && editor.byte_at(p + 1) == b'/' {
            code_end = p;
            break;
        }
        p += 1;
    }
    while code_end > line_start {
        let b = editor.byte_at(code_end - 1);
        if b != b' ' && b != b'\t' && b != b'\r' && b != b'\n' {
            break;
        }
        code_end -= 1;
    }
    code_end
}

fn folded_block_suffix(editor: &Editor, phys_line: usize, fold_end: usize) -> ([char; 4], u8) {
    let mut fold_suffix = ['\0'; 4];
    let mut fold_suffix_len = 0;
    let start_line_start = editor.line_offsets[phys_line];
    let start_line_end = if phys_line + 1 < editor.line_offsets.len() {
        editor.line_offsets[phys_line + 1]
    } else {
        editor.len()
    };
    let start_code_end = code_end_before_line_comment(editor, start_line_start, start_line_end);

    let mut p_start = start_code_end;
    let mut last_start_char = 0;
    while p_start > start_line_start {
        p_start -= 1;
        let b = editor.byte_at(p_start);
        if b != b' ' && b != b'\t' && b != b'\r' && b != b'\n' {
            last_start_char = b;
            break;
        }
    }

    if last_start_char != b'{' && last_start_char != b'[' && last_start_char != b'(' {
        return (fold_suffix, fold_suffix_len);
    }

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
    let mut p_scan = code_end_before_line_comment(editor, end_line_start, end_line_end);
    let mut suffix_bytes_rev = [0u8; 4];
    let mut suffix_len = 0;
    while p_scan > end_line_start && suffix_len < 4 {
        p_scan -= 1;
        let b = editor.byte_at(p_scan);
        if b == b' ' || b == b'\t' {
            break;
        }
        suffix_bytes_rev[suffix_len] = b;
        suffix_len += 1;
    }

    if let Some(pos_in_rev) = suffix_bytes_rev[..suffix_len]
        .iter()
        .position(|&x| x == expected_close)
    {
        for i in (0..=pos_in_rev).rev() {
            let b = suffix_bytes_rev[i];
            if fold_suffix_len < 4 {
                fold_suffix[fold_suffix_len as usize] = b as char;
                fold_suffix_len += 1;
            }
        }
    }
    (fold_suffix, fold_suffix_len)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor_with_text(text: &str) -> Editor {
        let mut editor = Editor::new(text.len() + 16);
        let _ = editor.insert_str(text);
        editor
    }

    #[test]
    fn folded_block_suffix_keeps_comma_before_inline_comment() {
        let editor = editor_with_text("exception_handlers={\n    Exception: handler,\n},  # ty\n");
        let (suffix, len) = folded_block_suffix(&editor, 0, 2);
        assert_eq!(len, 2);
        assert_eq!(&suffix[..2], &['}', ',']);
    }

    #[test]
    fn folded_block_suffix_keeps_plain_closer_and_comma() {
        let editor = editor_with_text("type_encoders={\n    Any: encoder,\n},\n");
        let (suffix, len) = folded_block_suffix(&editor, 0, 2);
        assert_eq!(len, 2);
        assert_eq!(&suffix[..2], &['}', ',']);
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
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
                if is_folded {
                    end_byte = folded_import_display_end(editor, start_byte, end_byte);
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

                        if self.left_padding + whitespace_px_width + text_px_width
                            > scroll_x + self.width + 2000.0
                        {
                            out_of_bounds = true;
                            break;
                        }
                    }

                    if out_of_bounds {
                        break;
                    }
                    current += chunk.len();
                }

                let (fold_suffix, fold_suffix_len) = if is_folded {
                    editor
                        .foldable_lines
                        .get(&phys_line)
                        .map(|&fold_end| folded_block_suffix(editor, phys_line, fold_end))
                        .unwrap_or((['\0'; 4], 0))
                } else {
                    (['\0'; 4], 0)
                };

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
                    let mut first_line_end = if phys_line + 1 < editor.line_offsets.len() {
                        editor.line_offsets[phys_line + 1].saturating_sub(1)
                    } else {
                        editor.len()
                    };
                    first_line_end = folded_import_display_end(editor, line_start, first_line_end);

                    if editor.cursor >= first_line_end {
                        x += self.measure_width(first, second, line_start, first_line_end);

                        let mut dots_w =
                            self.measure_ui_width("...", 1.0) + 10.0 * self.scale_factor;

                        let (fold_suffix, fold_suffix_len) =
                            folded_block_suffix(editor, phys_line, fold_end_line);
                        for i in 0..fold_suffix_len {
                            dots_w += self.char_advance(fold_suffix[i as usize]);
                        }

                        x += dots_w;
                    } else {
                        x += self.measure_width(first, second, line_start, editor.cursor);
                    }
                } else {
                    x += self.measure_width(first, second, line_start, editor.cursor);
                    x += self.current_inlay_width_before(line_start, editor.cursor);
                }

                return (x - self.last_scroll_x, current_y);
            }

            current_y += self.line_height;
            phys_line = fold_end_line + 1;
        }

        (self.left_padding - self.last_scroll_x, current_y)
    }

    pub(crate) fn current_inlay_width_before(
        &mut self,
        line_start: usize,
        byte_offset: usize,
    ) -> f32 {
        let mut width = 0.0;
        let pad_w = 8.0 * self.scale_factor;
        let value_gap_w = self.char_advance(' ');
        for idx in 0..self.current_python_inlay_hints.len() {
            let (hint_offset, hint_label) = {
                let hint = &self.current_python_inlay_hints[idx];
                (hint.byte_offset, hint.label.clone())
            };
            if hint_offset >= byte_offset {
                break;
            }
            if hint_offset >= line_start {
                width += self.measure_ui_width(hint_label.trim_end(), 0.92) + pad_w + value_gap_w;
            }
        }
        width
    }

    pub(crate) fn is_inlay_hint_at_xy(
        &mut self,
        editor: &Editor,
        target_x: f32,
        target_y: f32,
    ) -> bool {
        let target_y = target_y.max(0.0);
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
        if is_folded {
            end_byte = folded_import_display_end(editor, start_byte, end_byte);
        }

        let mut current_x = self.left_padding - self.last_scroll_x;
        let mut last_valid_byte = start_byte;
        let mut inlay_idx = self
            .current_python_inlay_hints
            .partition_point(|hint| hint.byte_offset < start_byte);
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
                while inlay_idx < self.current_python_inlay_hints.len()
                    && self.current_python_inlay_hints[inlay_idx].byte_offset < last_valid_byte
                {
                    inlay_idx += 1;
                }
                while inlay_idx < self.current_python_inlay_hints.len()
                    && self.current_python_inlay_hints[inlay_idx].byte_offset == last_valid_byte
                {
                    let hint_label = self.current_python_inlay_hints[inlay_idx].label.clone();
                    let hint_w = self.measure_ui_width(hint_label.trim_end(), 0.92)
                        + 8.0 * self.scale_factor
                        + self.char_advance(' ');
                    if target_x >= current_x && target_x <= current_x + hint_w {
                        return true;
                    }
                    current_x += hint_w;
                    inlay_idx += 1;
                }

                if c == '\n' {
                    return false;
                }
                if c != '\u{FE0F}' && c != '\u{200D}' {
                    current_x += self.char_advance(c);
                }
                last_valid_byte += c.len_utf8();
            }
        }

        false
    }

    pub fn get_byte_at_xy(&mut self, editor: &Editor, target_x: f32, target_y: f32) -> usize {
        let target_y = target_y.max(0.0);
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
        if is_folded {
            end_byte = folded_import_display_end(editor, start_byte, end_byte);
        }

        let mut current_x = self.left_padding - self.last_scroll_x;
        let mut last_valid_byte = start_byte;
        let mut inlay_idx = self
            .current_python_inlay_hints
            .partition_point(|hint| hint.byte_offset < start_byte);
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
                while inlay_idx < self.current_python_inlay_hints.len()
                    && self.current_python_inlay_hints[inlay_idx].byte_offset < last_valid_byte
                {
                    inlay_idx += 1;
                }
                while inlay_idx < self.current_python_inlay_hints.len()
                    && self.current_python_inlay_hints[inlay_idx].byte_offset == last_valid_byte
                {
                    let hint_label = self.current_python_inlay_hints[inlay_idx].label.clone();
                    let value_gap_w = self.char_advance(' ');
                    let hint_w = self.measure_ui_width(hint_label.trim_end(), 0.92)
                        + 8.0 * self.scale_factor
                        + value_gap_w;
                    if target_x <= current_x + hint_w {
                        return last_valid_byte;
                    }
                    current_x += hint_w;
                    inlay_idx += 1;
                }
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
        super::editor_max_scroll_for_lines(
            editor.get_visible_lines_count(),
            self.line_height,
            window_height,
        )
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
            self.gl
                .uniform_matrix_4_f32_slice(self.proj_loc.as_ref(), false, &proj);
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
        x = x.round();
        let y = y.round();
        for c in text.chars() {
            if c == '\n' || c == '\r' || c == '\u{FE0F}' || c == '\u{200D}' {
                continue;
            }
            if let Some(g) = self.get_glyph(c) {
                let (q_x, q_y, q_w, q_h) = glyph_quad_rect(x, y, g, 1.0);
                self.push_quad(q_x, q_y, q_w, q_h, g.u, g.v, g.uw, g.vh, color, g.is_emoji);
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
        x = x.round();
        let y = y.round();
        for c in text.chars() {
            if c == '\n' || c == '\r' || c == '\u{FE0F}' || c == '\u{200D}' {
                continue;
            }
            if let Some(g) = self.get_ui_glyph(c) {
                let (q_x, q_y, q_w, q_h) = glyph_quad_rect(x, y, g, scale);
                self.push_quad(q_x, q_y, q_w, q_h, g.u, g.v, g.uw, g.vh, color, g.is_emoji);
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
        x = x.round();
        let y = y.round();
        for c in text.chars() {
            if c == '\n' || c == '\r' || c == '\u{FE0F}' || c == '\u{200D}' {
                continue;
            }
            if let Some(g) = self.get_glyph(c) {
                let (q_x, q_y, q_w, q_h) = glyph_quad_rect(x, y, g, scale);
                self.push_quad(q_x, q_y, q_w, q_h, g.u, g.v, g.uw, g.vh, color, g.is_emoji);
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

    pub fn push_rounded_rect_outline(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: f32,
        border_w: f32,
        color: [f32; 4],
    ) {
        let w_round = w.round();
        let h_round = h.round();
        let x1 = x.round();
        let y1 = y.round();
        let x2 = x1 + w_round;
        let y2 = y1 + h_round;

        let half_w = w_round / 2.0;
        let half_h = h_round / 2.0;
        let border_w = border_w.round().clamp(1.0, 32.0);
        let sdf_params = [half_w, half_h, r + border_w * 1024.0];

        let v1 = Vertex {
            pos: [x1, y1],
            uv: [-half_w, -half_h],
            color,
            mode: 9.0,
            sdf_params,
        };
        let v2 = Vertex {
            pos: [x2, y1],
            uv: [half_w, -half_h],
            color,
            mode: 9.0,
            sdf_params,
        };
        let v3 = Vertex {
            pos: [x2, y2],
            uv: [half_w, half_h],
            color,
            mode: 9.0,
            sdf_params,
        };
        let v4 = Vertex {
            pos: [x1, y2],
            uv: [-half_w, half_h],
            color,
            mode: 9.0,
            sdf_params,
        };

        self.vertices.extend_from_slice(&[v1, v2, v3, v1, v3, v4]);
    }

    pub fn push_rounded_rect_border(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: f32,
        border_w: f32,
        border_color: [f32; 4],
        fill_color: [f32; 4],
    ) {
        let x = x.round();
        let y = y.round();
        let w = w.round();
        let h = h.round();
        let border_w = border_w.round().max(1.0);
        self.push_rounded_rect(x, y, w, h, r.round(), border_color);
        self.push_rounded_rect(
            x + border_w,
            y + border_w,
            (w - border_w * 2.0).max(0.0),
            (h - border_w * 2.0).max(0.0),
            (r.round() - border_w).max(1.0),
            fill_color,
        );
    }
}
