pub mod core_text;
pub mod ui;

use crate::editor::Editor;
use crate::highlighter::ColorSpan;
use crate::renderer::{Renderer, Vertex};
use crate::widgets::IconButton;
use glow::HasContext;

#[derive(Clone, Copy)]
pub struct ModInterval {
    pub top: f32,
    pub bottom: f32,
    pub state: crate::editor::LineModState,
}

impl Renderer {
    pub fn draw(
        &mut self,
        editor: &mut Editor,
        scroll_x: f32,
        scroll_y: f32,
        blink_alpha: f32,
        show_fps: bool,
        spans: &[ColorSpan],
        show_quit_dialog: bool,
        is_resizing: bool,
        search_results: &[(usize, usize)],
        search_current_idx: Option<usize>,
        search_anim_y: f32,
        search_editor: &Editor,
        search_focused: bool,
        search_case_sensitive: bool,
        show_welcome: bool,
        recent_files: &[std::path::PathBuf],
        sticky_scroll_alpha: f32,
    ) -> bool {
        if show_welcome {
            return self.draw_welcome(recent_files);
        }

        let mut wants_pointer = false;

        let now = std::time::Instant::now();
        if let Some(last) = self.last_frame_time {
            let dt = now.duration_since(last).as_secs_f32();
            self.frame_count += 1;
            self.time_acc += dt;
            if self.time_acc >= 0.5 {
                self.fps = self.frame_count as f32 / self.time_acc;
                self.frame_count = 0;
                self.time_acc = 0.0;

                use std::fmt::Write;
                self.fps_string.clear();
                let _ = write!(&mut self.fps_string, "FPS: {:.0}", self.fps);
            }
        }
        self.last_frame_time = Some(now);

        let cursor_phys_line = editor
            .line_offsets
            .partition_point(|&o| o <= editor.cursor)
            .saturating_sub(1);

        let mut visible_lines_count = 0;
        let mut visible_cursor_line = 0;
        let mut temp_phys = 0;
        while temp_phys < editor.line_offsets.len() {
            if temp_phys == cursor_phys_line {
                visible_cursor_line = visible_lines_count;
            }
            let is_folded = editor.folded_lines.contains(&temp_phys)
                && editor.foldable_lines.contains_key(&temp_phys);
            let fold_end = if is_folded {
                editor.foldable_lines.get(&temp_phys).copied()
            } else {
                None
            };
            visible_lines_count += 1;
            if let Some(end) = fold_end {
                if cursor_phys_line > temp_phys && cursor_phys_line <= end {
                    visible_cursor_line = visible_lines_count - 1;
                }
                temp_phys = end;
            }
            temp_phys += 1;
        }

        let total_lines = visible_lines_count.max(1);
        let s = self.scale_factor;

        let target_minimap_w = 119.0 * s;

        if (self.minimap_width - target_minimap_w).abs() > 0.5 {
            self.minimap_width = target_minimap_w;
            self.visual_lines.clear();
        }

        let digits = editor.line_offsets.len().to_string().len().max(3);
        let target_padding = (30.0 * s + digits as f32 * 10.0 * s).round();
        if (self.left_padding - target_padding).abs() > 0.5 {
            self.left_padding = target_padding;
            self.visual_lines.clear();
        }

        self.update_cache(editor, scroll_x, scroll_y, is_resizing);

        let render_scroll_x = scroll_x.round();
        let render_scroll_y = scroll_y.round();

        if self.last_editor_version_for_scroll_x != editor.version
            || (self.last_width - self.width).abs() > 0.5
        {
            let longest_idx = editor.longest_line_idx;
            let start_byte = editor.line_offsets.get(longest_idx).copied().unwrap_or(0);
            let end_byte = editor
                .line_offsets
                .get(longest_idx + 1)
                .copied()
                .unwrap_or(editor.len());
            let (first, second) = editor.text_parts();
            let longest_width = self.measure_width(first, second, start_byte, end_byte);
            let view_w = self.width - self.minimap_width - self.left_padding;
            self.max_scroll_x = (longest_width - view_w + 100.0).max(0.0);
            self.last_editor_version_for_scroll_x = editor.version;
        }

        unsafe {
            self.gl.bind_vertex_array(Some(self.vao));
            self.gl.use_program(Some(self.program));
            self.gl.active_texture(glow::TEXTURE0);
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            self.gl
                .clear_color(self.theme.bg[0], self.theme.bg[1], self.theme.bg[2], 1.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
        }

        editor.ensure_indent_cache_updated();
        let indent_levels = editor.get_cached_indent_levels();
        let (first, second) = editor.text_parts();

        let first_len = first.len();
        let len = first_len + second.len();

        // --- Подсветка скобок ---
        let mut bracket_pairs = None;
        let find_matching_bracket = |pos: usize, b: u8| -> Option<usize> {
            let (open, close, dir) = match b {
                b'(' => (b'(', b')', 1isize),
                b'[' => (b'[', b']', 1isize),
                b'{' => (b'{', b'}', 1isize),
                b')' => (b')', b'(', -1isize),
                b']' => (b']', b'[', -1isize),
                b'}' => (b'}', b'{', -1isize),
                _ => return None,
            };
            let mut depth = 1;
            let mut curr = pos as isize + dir;
            while curr >= 0 && curr < len as isize {
                let cb = editor.byte_at(curr as usize);
                if cb == open { depth += 1; }
                else if cb == close {
                    depth -= 1;
                    if depth == 0 { return Some(curr as usize); }
                }
                curr += dir;
            }
            None
        };

        if editor.cursor < len {
            let b = editor.byte_at(editor.cursor);
            if let Some(matching) = find_matching_bracket(editor.cursor, b) {
                bracket_pairs = Some((editor.cursor, matching));
            }
        }
        if bracket_pairs.is_none() && editor.cursor > 0 {
            let b = editor.byte_at(editor.cursor - 1);
            if let Some(matching) = find_matching_bracket(editor.cursor - 1, b) {
                bracket_pairs = Some((editor.cursor - 1, matching));
            }
        }

        let sel_start = editor.selection_anchor.map(|a| a.min(editor.cursor)).unwrap_or(editor.cursor);
        let sel_end = editor.selection_anchor.map(|a| a.max(editor.cursor)).unwrap_or(editor.cursor);

        // --- Одинаковые слова (Word Highlighting) ---
        let mut identical_words = Vec::new();
        let mut target_word = None;
        let is_valid_word = |s: &str| -> bool {
            !s.is_empty() && s.as_bytes().iter().all(|&b| b.is_ascii_alphanumeric() || b == b'_') && !s.chars().next().unwrap().is_ascii_digit()
        };

        if sel_start != sel_end {
            let slen = sel_end - sel_start;
            if slen < 100 {
                if let Some(text) = editor.get_selection() {
                    if is_valid_word(&text) {
                        target_word = Some(text);
                    }
                }
            }
        } else {
            let mut p_start = editor.cursor;
            while p_start > 0 {
                let b = editor.byte_at(p_start - 1);
                if !(b.is_ascii_alphanumeric() || b == b'_') { break; }
                p_start -= 1;
            }
            let mut p_end = editor.cursor;
            while p_end < len {
                let b = editor.byte_at(p_end);
                if !(b.is_ascii_alphanumeric() || b == b'_') { break; }
                p_end += 1;
            }
            if p_end > p_start {
                let slen = p_end - p_start;
                let mut res = Vec::with_capacity(slen);
                for i in p_start..p_end { res.push(editor.byte_at(i)); }
                let w = String::from_utf8_lossy(&res).into_owned();
                if is_valid_word(&w) {
                    target_word = Some(w);
                }
            }
        }

        if let Some(word) = target_word {
            let full_text = editor.get_full_text();
            let mut start = 0;
            let w_len = word.len();
            while let Some(idx) = full_text[start..].find(&word) {
                let abs_idx = start + idx;
                let left_ok = if abs_idx == 0 { true } else {
                    let b = full_text.as_bytes()[abs_idx - 1];
                    !(b.is_ascii_alphanumeric() || b == b'_')
                };
                let right_ok = if abs_idx + w_len == len { true } else {
                    let b = full_text.as_bytes()[abs_idx + w_len];
                    !(b.is_ascii_alphanumeric() || b == b'_')
                };
                if left_ok && right_ok {
                    identical_words.push((abs_idx, abs_idx + w_len));
                }
                start = abs_idx + w_len;
            }
        }

        let max_scroll = self.get_max_scroll(editor, self.height);
        let scrollbar_width = if max_scroll > 0.0 { 10.0 * s } else { 0.0 };

        let minimap_w = self.minimap_width;
        let minimap_x = self.width - minimap_w;
        let scrollbar_x = minimap_x - scrollbar_width;

        let solid_minimap_bg = [
            self.theme.minimap_bg[0],
            self.theme.minimap_bg[1],
            self.theme.minimap_bg[2],
            1.0,
        ];

        let cursor_line_y = self.baseline_offset - render_scroll_y
            + (visible_cursor_line as f32 * self.line_height);

        if cursor_line_y > -self.line_height * 2.0 && cursor_line_y < self.height + self.line_height
        {
            self.push_rect(
                self.left_padding,
                cursor_line_y - self.baseline_offset + 2.0,
                scrollbar_x - self.left_padding,
                self.line_height,
                [0.9, 0.9, 0.9, 0.12],
            );
        }

        let skip_visual_lines = 0;
        let end_visual_line = self.visual_lines.len();

        let guide_color = [self.theme.fg[0], self.theme.fg[1], self.theme.fg[2], 0.15];
        let space_adv = self.char_advance(' ');

        for i in skip_visual_lines..end_visual_line {
            let v_line = self.visual_lines[i];
            let phys_idx = v_line.physical_line - 1;

            if let Some(&depth) = indent_levels.get(phys_idx) {
                if depth > 0 {
                    let y_top = v_line.y_offset - render_scroll_y;
                    let text_start_x = self.left_padding + v_line.whitespace_px_width;
                    let text_end_x = text_start_x + v_line.text_px_width;

                    for level in 1..=depth {
                        let guide_x = self.left_padding + (level as f32 * 4.0 * space_adv);
                        let margin = space_adv * 0.5;
                        let overlaps = v_line.text_px_width > 0.0
                            && text_start_x <= guide_x + margin
                            && text_end_x >= guide_x - margin;

                        if !overlaps {
                            self.push_rect(
                                (guide_x - render_scroll_x).round(),
                                y_top,
                                1.0,
                                self.line_height,
                                guide_color,
                            );
                        }
                    }
                }
            }
        }

        let mut intervals = Vec::with_capacity(64);
        let mut last_phys_line = None;
        let mut last_bottom_y = 0.0;

        for i in skip_visual_lines..end_visual_line {
            let v_line = self.visual_lines[i];
            let phys_idx = v_line.physical_line - 1;
            let y_top = v_line.y_offset - render_scroll_y;
            let y_bottom = y_top + self.line_height;
            last_bottom_y = y_bottom;

            if !v_line.is_soft_wrap {
                if let Some(st) = editor.deleted_gaps.get(phys_idx).copied().flatten() {
                    intervals.push(ModInterval {
                        top: y_top - 3.0,
                        bottom: y_top + 3.0,
                        state: st,
                    });
                }
            }

            if let Some(st) = editor.get_line_modification_state(phys_idx) {
                intervals.push(ModInterval {
                    top: y_top,
                    bottom: y_bottom,
                    state: st,
                });
            }
            last_phys_line = Some(phys_idx);
        }

        if end_visual_line == self.visual_lines.len() {
            if let Some(phys_idx) = last_phys_line {
                if let Some(st) = editor.deleted_gaps.get(phys_idx + 1).copied().flatten() {
                    intervals.push(ModInterval {
                        top: last_bottom_y - 3.0,
                        bottom: last_bottom_y + 3.0,
                        state: st,
                    });
                }
            }
        }

        let mut merged: Vec<ModInterval> = Vec::with_capacity(64);
        for int in intervals {
            if let Some(last) = merged.last_mut() {
                if int.top <= last.bottom + 0.1 && int.state == last.state {
                    last.bottom = last.bottom.max(int.bottom);
                    continue;
                }
            }
            merged.push(int);
        }

        let mut cursor_pos = None;

        for i in skip_visual_lines..end_visual_line {
            let v_line_info = self.visual_lines[i];
            let start_byte = v_line_info.byte_idx;

            let end_byte = if v_line_info.is_folded {
                let phys_idx = v_line_info.physical_line - 1;
                if phys_idx + 1 < editor.line_offsets.len() {
                    editor.line_offsets[phys_idx + 1].saturating_sub(1)
                } else {
                    len
                }
            } else if i + 1 < self.visual_lines.len() {
                self.visual_lines[i + 1].byte_idx
            } else {
                let phys_idx = v_line_info.physical_line - 1;
                if phys_idx + 1 < editor.line_offsets.len() {
                    editor.line_offsets[phys_idx + 1]
                } else {
                    len
                }
            };

            let y = self.baseline_offset + v_line_info.y_offset - render_scroll_y;
            let mut x = self.left_padding;

            let mut span_idx = match spans.binary_search_by_key(&start_byte, |s| s.start) {
                Ok(idx) => idx,
                Err(idx) => idx.saturating_sub(1),
            };

            let mut search_idx = search_results.partition_point(|&(_, e)| e <= start_byte);
            let mut identical_idx = identical_words.partition_point(|&(_, e)| e <= start_byte);

            let mut current_offset = start_byte;
            let mut current_chunk_offset = start_byte;

            let mut out_of_bounds = false;

            while current_chunk_offset < end_byte {
                if self.vertices.len() > crate::renderer::MAX_VERTICES - 2000 {
                    self.flush();
                }

                let s = if current_chunk_offset < first_len {
                    let s_end = end_byte.min(first_len);
                    &first[current_chunk_offset..s_end]
                } else {
                    let s_start = current_chunk_offset - first_len;
                    let s_end = end_byte - first_len;
                    &second[s_start..s_end]
                };

                for c in s.chars() {
                    if x - render_scroll_x > self.width + 150.0 {
                        out_of_bounds = true;
                        break;
                    }

                    let char_len = c.len_utf8();

                    if cursor_pos.is_none()
                        && editor.cursor >= current_offset
                        && editor.cursor < current_offset + char_len
                    {
                        cursor_pos = Some((x - render_scroll_x, y));
                    }

                    while span_idx < spans.len() && spans[span_idx].end <= current_offset {
                        span_idx += 1;
                    }

                    while search_idx < search_results.len()
                        && search_results[search_idx].1 <= current_offset
                    {
                        search_idx += 1;
                    }
                    while identical_idx < identical_words.len()
                        && identical_words[identical_idx].1 <= current_offset
                    {
                        identical_idx += 1;
                    }

                    let is_newline = c == '\n';
                    let is_hidden = c == '\u{FE0F}' || c == '\u{200D}';
                    let adv = if is_newline || is_hidden {
                        0.0
                    } else {
                        self.char_advance(c)
                    };

                    let mut is_search_res = false;
                    let mut is_active_search = false;

                    if search_idx < search_results.len()
                        && current_offset >= search_results[search_idx].0
                    {
                        is_search_res = true;
                        if Some(search_idx) == search_current_idx {
                            is_active_search = true;
                        }
                    }

                    let is_identical = identical_idx < identical_words.len() && current_offset >= identical_words[identical_idx].0;

                    let is_bracket = if let Some((b1, b2)) = bracket_pairs {
                        current_offset == b1 || current_offset == b2
                    } else {
                        false
                    };

                    // Приоритеты фонов: 1. Выделение, 2. Поиск, 3. Одинаковые слова
                    if current_offset >= sel_start && current_offset < sel_end {
                        let w = if is_newline { 10.0 } else { adv };
                        self.push_rect(
                            x - render_scroll_x,
                            y - self.baseline_offset + 2.0,
                            w,
                            self.line_height,
                            self.theme.sel,
                        );
                    } else if is_search_res {
                        let w = if is_newline { 10.0 } else { adv };
                        let color = if is_active_search {
                            [1.0, 0.6, 0.0, 0.5]
                        } else {
                            [0.6, 0.6, 0.6, 0.35]
                        };
                        self.push_rect(
                            x - render_scroll_x,
                            y - self.baseline_offset + 2.0,
                            w,
                            self.line_height,
                            color,
                        );
                    } else if is_identical {
                        let w = if is_newline { 10.0 } else { adv };
                        self.push_rect(
                            x - render_scroll_x,
                            y - self.baseline_offset + 2.0,
                            w,
                            self.line_height,
                            [self.theme.sel[0], self.theme.sel[1], self.theme.sel[2], 0.3],
                        );
                    }

                    if is_bracket && !is_newline && !is_hidden {
                        self.push_rect(
                            x - render_scroll_x,
                            y - self.baseline_offset + 2.0,
                            adv,
                            self.line_height,
                            [0.6, 0.6, 0.6, 0.3],
                        );
                    }

                    if !is_newline && !is_hidden && c != ' ' && c != '\t' {
                        if x - render_scroll_x + adv > 0.0 {
                            if let Some(g) = self.get_glyph(c) {
                                let mut current_color = self.theme.fg;
                                if span_idx < spans.len() && spans[span_idx].start <= current_offset
                                {
                                    current_color = spans[span_idx].color;
                                }

                                self.push_quad(
                                    x - render_scroll_x + g.offset_x,
                                    y - g.offset_y,
                                    g.width,
                                    g.height,
                                    g.u,
                                    g.v,
                                    g.uw,
                                    g.vh,
                                    current_color,
                                    g.is_emoji,
                                );

                                if c == '.' || c == ':' {
                                    self.push_quad(
                                        x - render_scroll_x + g.offset_x + 1.0,
                                        y - g.offset_y,
                                        g.width,
                                        g.height,
                                        g.u,
                                        g.v,
                                        g.uw,
                                        g.vh,
                                        current_color,
                                        g.is_emoji,
                                    );
                                }
                            }
                        }
                    }

                    x += adv;
                    current_offset += char_len;
                }

                if out_of_bounds {
                    break;
                }

                if current_chunk_offset < first_len {
                    current_chunk_offset = first_len;
                } else {
                    current_chunk_offset = end_byte;
                }
            }

            if v_line_info.is_folded {
                let dots_str = "...";
                let dots_adv = self.measure_ui_width(dots_str, 1.0);

                let phys_idx = v_line_info.physical_line - 1;
                let actual_end_byte = if let Some(&fold_end) = editor.foldable_lines.get(&phys_idx)
                {
                    if fold_end + 1 < editor.line_offsets.len() {
                        editor.line_offsets[fold_end + 1].saturating_sub(1)
                    } else {
                        len
                    }
                } else {
                    end_byte
                };

                let is_dots_selected = sel_start != sel_end
                    && sel_start <= actual_end_byte.saturating_sub(1)
                    && sel_end >= actual_end_byte.saturating_sub(1);

                let dots_bg = if is_dots_selected {
                    self.theme.sel
                } else {
                    [
                        self.theme.bg[0] + 0.08,
                        self.theme.bg[1] + 0.08,
                        self.theme.bg[2] + 0.12,
                        1.0,
                    ]
                };

                let box_x = x - render_scroll_x + 2.0 * s;
                let box_w = dots_adv + 6.0 * s;
                let box_y_draw = y - self.baseline_offset + 4.0 * s;
                let box_h_draw = self.line_height - 8.0 * s;

                let next_x = box_x + box_w + 2.0 * s;
                let mut final_x = next_x;
                if let Some(c) = v_line_info.fold_char {
                    final_x += self.char_advance(c);
                }

                let hit_y_top = y - self.line_height;
                let hit_y_bottom = y + 5.0 * s;
                if self.last_mouse_x >= box_x - 2.0 * s
                    && self.last_mouse_x <= next_x + 10.0 * s
                    && self.last_mouse_y >= hit_y_top
                    && self.last_mouse_y <= hit_y_bottom
                {
                    wants_pointer = true;
                }

                if cursor_pos.is_none()
                    && editor.cursor >= end_byte
                    && editor.cursor <= actual_end_byte
                {
                    cursor_pos = Some((final_x, y));
                }

                self.push_rounded_rect(box_x, box_y_draw, box_w, box_h_draw, 4.0 * s, dots_bg);

                self.draw_string_scaled(dots_str, box_x + 3.0 * s, y, self.theme.fg, 1.0);

                if let Some(c) = v_line_info.fold_char {
                    let c_adv = self.char_advance(c);

                    if is_dots_selected {
                        self.push_rect(
                            next_x,
                            y - self.baseline_offset + 2.0,
                            c_adv,
                            self.line_height,
                            self.theme.sel,
                        );
                    }

                    if let Some(g) = self.get_glyph(c) {
                        self.push_quad(
                            next_x + g.offset_x,
                            y - g.offset_y,
                            g.width,
                            g.height,
                            g.u,
                            g.v,
                            g.uw,
                            g.vh,
                            self.theme.fg,
                            g.is_emoji,
                        );
                    }
                }
            }
        }

        if cursor_pos.is_none() && editor.cursor == len {
            if let Some(last_line) = self.visual_lines.last() {
                let y = self.baseline_offset + last_line.y_offset - render_scroll_y;
                let (first, second) = editor.text_parts();
                let x = self.left_padding
                    + self.measure_width(first, second, last_line.byte_idx, editor.cursor);
                cursor_pos = Some((x - render_scroll_x, y));
            }
        }

        if let Some((cx_screen, cy)) = cursor_pos {
            if sel_start == sel_end && blink_alpha > 0.5 && !show_quit_dialog && !search_focused {
                if cy > -self.line_height
                    && cy < self.height + self.line_height
                    && cx_screen < scrollbar_x
                    && cx_screen >= self.left_padding
                {
                    self.push_rect(
                        cx_screen,
                        cy - self.baseline_offset + 2.0,
                        2.0,
                        self.line_height - 2.0,
                        self.theme.fg,
                    );
                }
            }
        }

        self.flush();

        self.push_rect(0.0, 0.0, self.left_padding, self.height, solid_minimap_bg);

        for i in skip_visual_lines..end_visual_line {
            let v_line = self.visual_lines[i];
            let y = self.baseline_offset + v_line.y_offset - render_scroll_y;
            let phys_idx = v_line.physical_line - 1;

            if editor.foldable_lines.contains_key(&phys_idx) {
                let arrow_x = self.left_padding - 18.0 * s;
                let is_folded = editor.folded_lines.contains(&phys_idx);
                let arrow_str = if is_folded { "▶" } else { "▼" };
                self.draw_string_scaled(arrow_str, arrow_x, y - 2.0 * s, self.theme.line_num, 0.9);
            }

            let mut n = v_line.physical_line;
            let mut buf = [0u8; 20];
            let mut idx = 20;
            if n == 0 {
                idx -= 1;
                buf[idx] = b'0';
            } else {
                while n > 0 {
                    idx -= 1;
                    buf[idx] = b'0' + (n % 10) as u8;
                    n /= 10;
                }
            }
            if let Ok(num_str) = std::str::from_utf8(&buf[idx..]) {
                let num_w = self.measure_ui_width(num_str, 1.0);
                let draw_x = self.left_padding - 24.0 * s - num_w;
                self.draw_string_scaled(num_str, draw_x, y, self.theme.line_num, 1.0);
            }
        }

        for m in merged {
            if m.bottom < 0.0 || m.top > self.height {
                continue;
            }
            let color = if m.state == crate::editor::LineModState::ModifiedUnsaved {
                self.theme.modified_unsaved
            } else {
                self.theme.modified_saved
            };
            let draw_top = m.top + 2.0;
            let draw_bottom = m.bottom + 2.0;
            let draw_h = (draw_bottom - draw_top).max(4.0);
            self.push_rounded_rect(
                self.left_padding - 4.0 * s,
                draw_top,
                4.0 * s,
                draw_h,
                2.0 * s,
                color,
            );
        }

        self.flush();

        self.push_rect(minimap_x, 0.0, minimap_w, self.height, solid_minimap_bg);

        if scrollbar_width > 0.0 {
            self.push_rect(
                scrollbar_x,
                0.0,
                scrollbar_width,
                self.height,
                [0.0, 0.0, 0.0, 0.2],
            );
            let scroll_ratio_y = (render_scroll_y / max_scroll).clamp(0.0, 1.0);
            let total_content_height = (total_lines as f32 + 2.0) * self.line_height;
            let thumb_h =
                (self.height / total_content_height.max(self.height) * self.height).max(20.0 * s);
            let thumb_y = scroll_ratio_y * (self.height - thumb_h);
            self.push_rounded_rect(
                scrollbar_x + 1.0 * s,
                thumb_y,
                scrollbar_width - 2.0 * s,
                thumb_h,
                (scrollbar_width - 2.0 * s) / 2.0,
                [0.7, 0.33, 0.54, 0.8],
            );
        }

        let scroll_ratio_y = if max_scroll > 0.0 {
            (render_scroll_y / max_scroll).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let total_lines_f32 = total_lines as f32;
        let minimap_line_h = (self.height / (total_lines_f32 + 2.0))
            .max(self.height / 1250.0)
            .max(1.5);
        let max_minimap_scroll = ((total_lines_f32 + 2.0) * minimap_line_h - self.height).max(0.0);
        let current_minimap_scroll = (scroll_ratio_y * max_minimap_scroll).round();

        let map_bg = self.theme.minimap_bg;
        let mut current_y: f32 = 0.0;
        let mut phys_line = 0;
        let rect_h = minimap_line_h.ceil().max(1.0);

        let view_top = current_minimap_scroll;
        let view_bottom = current_minimap_scroll + self.height;

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

                let y1 = (current_y - current_minimap_scroll).round();
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

                        let v1 = Vertex {
                            pos: [x1, y1],
                            uv: [-1.0, -1.0],
                            color,
                            is_emoji: 0.0,
                        };
                        let v2 = Vertex {
                            pos: [x2, y1],
                            uv: [-1.0, -1.0],
                            color,
                            is_emoji: 0.0,
                        };
                        let v3 = Vertex {
                            pos: [x2, y2],
                            uv: [-1.0, -1.0],
                            color,
                            is_emoji: 0.0,
                        };
                        let v4 = Vertex {
                            pos: [x1, y2],
                            uv: [-1.0, -1.0],
                            color,
                            is_emoji: 0.0,
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

        let y_cursor = visible_cursor_line as f32 * minimap_line_h - current_minimap_scroll;
        self.push_rect(
            minimap_x,
            y_cursor,
            minimap_w,
            2.0,
            self.theme.minimap_cursor,
        );

        let current_visible_top_line = render_scroll_y / self.line_height;
        let viewport_y =
            (current_visible_top_line * minimap_line_h - current_minimap_scroll).round();
        let visible_lines = self.height / self.line_height;
        let viewport_h = (visible_lines * minimap_line_h).max(4.0);

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

        if self.max_scroll_x > 0.0 {
            let track_w = scrollbar_x - self.left_padding;
            let track_h_bg = 14.0 * s;
            let track_y_bg = self.height - track_h_bg;

            self.push_rect(
                self.left_padding,
                track_y_bg,
                track_w,
                track_h_bg,
                [self.theme.bg[0], self.theme.bg[1], self.theme.bg[2], 1.0],
            );

            let thumb_w =
                (track_w / (self.max_scroll_x + track_w).max(1.0) * track_w).max(40.0 * s);
            let scroll_ratio_x = (render_scroll_x / self.max_scroll_x).clamp(0.0, 1.0);
            let thumb_x = self.left_padding + scroll_ratio_x * (track_w - thumb_w);

            let thumb_y = self.height - 10.0 * s;
            let thumb_h = 6.0 * s;

            self.push_rounded_rect(
                thumb_x,
                thumb_y,
                thumb_w,
                thumb_h,
                3.0 * s,
                [0.7, 0.33, 0.54, 1.0],
            );
        }

        self.sticky_scroll_rects.clear();
        let mut active_ranges = Vec::new();

        for &(start_b, end_b, is_sticky) in &editor.foldable_ranges_bytes {
            if !is_sticky { continue; }
            let sl = editor.line_offsets.partition_point(|&o| o <= start_b).saturating_sub(1);
            let el = editor.line_offsets.partition_point(|&o| o <= end_b).saturating_sub(1);
            if sl < el {
                active_ranges.push((sl, el));
            }
        }
        active_ranges.sort_unstable_by_key(|&(sl, _)| sl);

        let mut sticky_lines = Vec::new();
        let mut sticky_stack_height = 0.0;
        for &(sl, el) in &active_ranges {
            let line_y = sl as f32 * self.line_height - render_scroll_y;
            let end_line_y = el as f32 * self.line_height - render_scroll_y;

            if line_y <= sticky_stack_height + 1.0 && end_line_y > sticky_stack_height {
                if !sticky_lines.iter().any(|&(s, _)| s == sl) {
                    sticky_lines.push((sl, el));
                    sticky_stack_height += self.line_height;
                }
            }
        }

        if sticky_lines.len() > 5 {
            let skip = sticky_lines.len() - 5;
            sticky_lines.drain(0..skip);
        }

        if !sticky_lines.is_empty() {
            let mut y_positions = vec![0.0; sticky_lines.len()];
            let mut current_bottom = f32::MAX;
            for i in (0..sticky_lines.len()).rev() {
                let (_, el) = sticky_lines[i];
                let end_y = el as f32 * self.line_height - render_scroll_y;
                let normal_y = i as f32 * self.line_height;
                let mut actual_y = normal_y;

                if end_y < actual_y + self.line_height {
                    actual_y = end_y - self.line_height;
                }
                if actual_y + self.line_height > current_bottom {
                    actual_y = current_bottom - self.line_height;
                }
                y_positions[i] = actual_y;
                current_bottom = actual_y;
            }

            let rect_w = self.width - minimap_w;
            let sticky_bg = [self.theme.minimap_bg[0], self.theme.minimap_bg[1], self.theme.minimap_bg[2], sticky_scroll_alpha];
            let shadow_color = [0.0, 0.0, 0.0, 0.5 * sticky_scroll_alpha];

            for (i, &(s_line, _)) in sticky_lines.iter().enumerate() {
                let rect_y = y_positions[i];
                
                if rect_y + self.line_height < 0.0 {
                    continue;
                }

                self.push_rect(0.0, rect_y, rect_w, self.line_height, sticky_bg);
                if i == sticky_lines.len() - 1 {
                    self.push_rect(0.0, rect_y + self.line_height, rect_w, 2.0, shadow_color);
                }

                let mut n = s_line + 1;
                let mut buf = [0u8; 20];
                let mut idx = 20;
                while n > 0 {
                    idx -= 1;
                    buf[idx] = b'0' + (n % 10) as u8;
                    n /= 10;
                }
                if let Ok(num_str) = std::str::from_utf8(&buf[idx..]) {
                    let num_w = self.measure_ui_width(num_str, 1.0);
                    let draw_x = self.left_padding - 24.0 * s - num_w;
                    let mut num_color = self.theme.line_num;
                    num_color[3] *= sticky_scroll_alpha;
                    self.draw_string_scaled(num_str, draw_x, rect_y + self.baseline_offset, num_color, 1.0);
                }

                let start_byte = editor.line_offsets[s_line];
                let end_byte = *editor.line_offsets.get(s_line + 1).unwrap_or(&editor.len());
                let mut x = self.left_padding - render_scroll_x;

                let mut span_idx = match spans.binary_search_by_key(&start_byte, |s| s.start) {
                    Ok(idx) => idx,
                    Err(idx) => idx.saturating_sub(1),
                };

                let mut current_offset = start_byte;
                while current_offset < end_byte {
                    let chunk = if current_offset < first_len {
                        let e = end_byte.min(first_len);
                        &first[current_offset..e]
                    } else {
                        let st = current_offset - first_len;
                        let e = end_byte - first_len;
                        &second[st..e]
                    };
                    for c in chunk.chars() {
                        let char_len = c.len_utf8();
                        while span_idx < spans.len() && spans[span_idx].end <= current_offset {
                            span_idx += 1;
                        }
                        let adv = if c == '\n' || c == '\r' || c == '\u{FE0F}' || c == '\u{200D}' { 0.0 } else { self.char_advance(c) };
                        if adv > 0.0 && c != ' ' && c != '\t' {
                            if x + adv > 0.0 && x < self.width - minimap_w - 20.0 {
                                if let Some(g) = self.get_glyph(c) {
                                    let mut color = self.theme.fg;
                                    if span_idx < spans.len() && spans[span_idx].start <= current_offset {
                                        color = spans[span_idx].color;
                                    }
                                    color[3] *= sticky_scroll_alpha;
                                    self.push_quad(
                                        x + g.offset_x,
                                        rect_y + self.baseline_offset - g.offset_y,
                                        g.width,
                                        g.height,
                                        g.u, g.v, g.uw, g.vh,
                                        color, g.is_emoji
                                    );
                                }
                            }
                        }
                        x += adv;
                        current_offset += char_len;
                    }
                    if x > self.width - minimap_w - 20.0 { break; }
                }

                self.sticky_scroll_rects.push((0.0, rect_y, rect_w, self.line_height, start_byte));
            }
            self.flush();
        }

        if show_fps {
            let center_x = (self.width - minimap_w) / 2.0;
            self.push_rect(center_x - 45.0, 5.0, 90.0, 25.0, [0.1, 0.1, 0.1, 0.8]);

            let fps_text = std::mem::take(&mut self.fps_string);
            self.draw_string(&fps_text, center_x - 40.0, 24.0, [0.0, 1.0, 0.0, 1.0]);
            self.fps_string = fps_text;
        }

        if search_anim_y > -70.0 {
            let search_w = 480.0 * s;
            let search_h = 46.0 * s;
            let search_x = scrollbar_x - search_w - 20.0 * s;

            self.push_rounded_rect(
                search_x,
                search_anim_y,
                search_w,
                search_h,
                6.0 * s,
                [0.18, 0.20, 0.22, 1.0],
            );
            self.push_rounded_rect(
                search_x - 1.0,
                search_anim_y - 1.0,
                search_w + 2.0,
                search_h + 2.0,
                6.0 * s,
                [self.theme.sel[0], self.theme.sel[1], self.theme.sel[2], 0.6],
            );

            self.push_rounded_rect(
                search_x,
                search_anim_y,
                search_w,
                search_h,
                6.0 * s,
                [
                    self.theme.minimap_bg[0],
                    self.theme.minimap_bg[1],
                    self.theme.minimap_bg[2],
                    1.0,
                ],
            );

            let input_x = search_x + 10.0 * s;
            let input_y = search_anim_y + 8.0 * s;
            let input_w = 260.0 * s;
            let input_h = 30.0 * s;

            let input_bg = self.theme.bg;
            let input_border = if search_focused {
                self.theme.sel
            } else {
                [0.3, 0.3, 0.3, 1.0]
            };
            self.push_rounded_rect(
                input_x - 1.0,
                input_y - 1.0,
                input_w + 2.0,
                input_h + 2.0,
                4.0 * s,
                input_border,
            );
            self.push_rounded_rect(input_x, input_y, input_w, input_h, 4.0 * s, input_bg);

            self.flush();
            unsafe {
                let text = search_editor.get_full_text();
                let text_y = input_y + input_h / 2.0 + 6.0 * s;
                let text_start_x = input_x + 5.0 * s;
                let visible_width = input_w - 10.0 * s;

                let mut cursor_total_x = 0.0;
                let mut total_text_width = 0.0;
                for (byte_idx, c) in text.char_indices() {
                    let char_to_measure = if c == '\n' { '↵' } else { c };
                    let adv = self
                        .get_ui_glyph(char_to_measure)
                        .map(|g| g.advance)
                        .unwrap_or(10.0);
                    if byte_idx < search_editor.cursor {
                        cursor_total_x += adv;
                    }
                    total_text_width += adv;
                }

                if cursor_total_x - self.search_scroll_x > visible_width {
                    self.search_scroll_x = cursor_total_x - visible_width;
                }
                if cursor_total_x - self.search_scroll_x < 0.0 {
                    self.search_scroll_x = cursor_total_x;
                }
                self.search_scroll_x = self
                    .search_scroll_x
                    .min(total_text_width - visible_width)
                    .max(0.0);

                self.gl.enable(glow::SCISSOR_TEST);
                let scissor_y = self.height - (input_y + input_h);
                self.gl.scissor(
                    input_x as i32,
                    scissor_y as i32,
                    input_w as i32,
                    input_h as i32,
                );

                let sel_start = search_editor
                    .selection_anchor
                    .unwrap_or(search_editor.cursor)
                    .min(search_editor.cursor);
                let sel_end = search_editor
                    .selection_anchor
                    .unwrap_or(search_editor.cursor)
                    .max(search_editor.cursor);

                let mut current_x = text_start_x - self.search_scroll_x;
                let mut byte_idx = 0;
                let mut cursor_draw_x = current_x;

                for c in text.chars() {
                    if byte_idx == search_editor.cursor {
                        cursor_draw_x = current_x;
                    }

                    let char_to_render = if c == '\n' { '↵' } else { c };
                    let adv = self
                        .get_ui_glyph(char_to_render)
                        .map(|g| g.advance)
                        .unwrap_or(10.0);

                    if byte_idx >= sel_start && byte_idx < sel_end {
                        self.push_rect(
                            current_x,
                            input_y + 4.0 * s,
                            adv,
                            input_h - 8.0 * s,
                            self.theme.sel,
                        );
                    }

                    if let Some(g) = self.get_ui_glyph(char_to_render) {
                        self.push_quad(
                            current_x + g.offset_x,
                            text_y - g.offset_y,
                            g.width,
                            g.height,
                            g.u,
                            g.v,
                            g.uw,
                            g.vh,
                            self.theme.fg,
                            g.is_emoji,
                        );
                    }

                    current_x += adv;
                    byte_idx += c.len_utf8();
                }
                if byte_idx == search_editor.cursor {
                    cursor_draw_x = current_x;
                }

                if search_focused && sel_start == sel_end && blink_alpha > 0.5 {
                    self.push_rect(
                        cursor_draw_x,
                        input_y + 4.0 * s,
                        2.0 * s,
                        input_h - 8.0 * s,
                        self.theme.fg,
                    );
                }

                self.flush();
                self.gl.disable(glow::SCISSOR_TEST);
            }

            let text_y = input_y + input_h / 2.0 + 6.0 * s;
            let btn_y = input_y;
            let btn_size = 30.0 * s;

            let mut current_x = search_x + search_w - 10.0 * s;

            current_x -= btn_size;
            let btn_close = IconButton {
                x: current_x,
                y: btn_y,
                size: btn_size,
                icon: self.icon_close,
                is_active: false,
            };
            current_x -= 8.0 * s;

            current_x -= btn_size;
            let btn_down = IconButton {
                x: current_x,
                y: btn_y,
                size: btn_size,
                icon: self.icon_down,
                is_active: false,
            };
            current_x -= 4.0 * s;

            current_x -= btn_size;
            let btn_up = IconButton {
                x: current_x,
                y: btn_y,
                size: btn_size,
                icon: self.icon_up,
                is_active: false,
            };
            current_x -= 4.0 * s;

            current_x -= btn_size;
            let btn_case = IconButton {
                x: current_x,
                y: btn_y,
                size: btn_size,
                icon: self.icon_case_match,
                is_active: search_case_sensitive,
            };

            if search_results.len() != self.last_search_len
                || search_current_idx != self.last_search_idx
            {
                self.search_res_string.clear();
                if !search_results.is_empty() {
                    use std::fmt::Write;
                    let _ = write!(
                        &mut self.search_res_string,
                        "{}/{}",
                        search_current_idx.unwrap_or(0) + 1,
                        search_results.len()
                    );
                }
                self.last_search_len = search_results.len();
                self.last_search_idx = search_current_idx;
            }

            let temp_res_text = std::mem::take(&mut self.search_res_string);

            let res_text = if search_results.is_empty() {
                if search_editor.get_full_text().is_empty() {
                    ""
                } else {
                    "Нет"
                }
            } else {
                &temp_res_text
            };

            if !res_text.is_empty() {
                let counter_x = input_x + input_w + 10.0 * s;
                self.draw_string_scaled(res_text, counter_x, text_y, [0.6, 0.6, 0.6, 1.0], 0.9);
            }

            self.search_res_string = temp_res_text;

            let mx = self.last_mouse_x;
            let my = self.last_mouse_y;

            wants_pointer |= btn_case.render(self, mx, my, s, false);
            wants_pointer |= btn_up.render(self, mx, my, s, false);
            wants_pointer |= btn_down.render(self, mx, my, s, false);
            wants_pointer |= btn_close.render(self, mx, my, s, false);
        }

        if show_quit_dialog {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.6]);
        }
        self.flush();

        wants_pointer
    }
}
