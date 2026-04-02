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
        editor: &Editor,
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
    ) {
        if show_welcome {
            self.draw_welcome(recent_files);
            return;
        }

        let now = std::time::Instant::now();
        if let Some(last) = self.last_frame_time {
            let dt = now.duration_since(last).as_secs_f32();
            self.frame_count += 1;
            self.time_acc += dt;
            if self.time_acc >= 0.5 {
                self.fps = self.frame_count as f32 / self.time_acc;
                self.frame_count = 0;
                self.time_acc = 0.0;
            }
        }
        self.last_frame_time = Some(now);
        self.update_cache(editor, is_resizing);

        let render_scroll_y = scroll_y.round();

        unsafe {
            self.gl.bind_vertex_array(Some(self.vao));
            self.gl.use_program(Some(self.program));
            self.gl.active_texture(glow::TEXTURE0);
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.texture));
            self.gl
                .clear_color(self.theme.bg[0], self.theme.bg[1], self.theme.bg[2], 1.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
        }

        let minimap_w = self.minimap_width;
        let minimap_x = self.width - minimap_w;
        let solid_minimap_bg = [
            self.theme.minimap_bg[0],
            self.theme.minimap_bg[1],
            self.theme.minimap_bg[2],
            1.0,
        ];

        self.push_rect(
            0.0,
            0.0,
            self.left_padding - 10.0,
            self.height,
            solid_minimap_bg,
        );

        let cursor_line_idx = match self
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

        let cursor_line_y =
            self.baseline_offset - render_scroll_y + (cursor_line_idx as f32 * self.line_height);

        if cursor_line_y > -self.line_height * 2.0 && cursor_line_y < self.height + self.line_height
        {
            self.push_rect(
                self.left_padding,
                cursor_line_y - self.baseline_offset + 2.0,
                minimap_x - self.left_padding,
                self.line_height,
                [0.9, 0.9, 0.9, 0.12],
            );
        }

        let skip_visual_lines =
            ((render_scroll_y / self.line_height).max(0.0) as usize).saturating_sub(2);
        let visible_lines_count = ((self.height / self.line_height).ceil() as usize) + 4;
        let end_visual_line =
            (skip_visual_lines + visible_lines_count).min(self.visual_lines.len());

        let mut intervals = Vec::new();
        let mut last_phys_line = None;
        let mut last_bottom_y = 0.0;

        for i in skip_visual_lines..end_visual_line {
            let v_line = self.visual_lines[i];
            let phys_idx = v_line.physical_line - 1;
            let y_top = (i as f32 * self.line_height) - render_scroll_y;
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

        let mut merged: Vec<ModInterval> = Vec::new();
        for int in intervals {
            if let Some(last) = merged.last_mut() {
                if int.top <= last.bottom + 0.1 && int.state == last.state {
                    last.bottom = last.bottom.max(int.bottom);
                    continue;
                }
            }
            merged.push(int);
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
            self.push_rounded_rect(self.left_padding - 7.0, draw_top, 7.0, draw_h, 2.0, color);
        }

        let (first, second) = editor.text_parts();
        let first_len = first.len();
        let len = first_len + second.len();
        let sel_start = editor
            .selection_anchor
            .map(|a| a.min(editor.cursor))
            .unwrap_or(editor.cursor);
        let sel_end = editor
            .selection_anchor
            .map(|a| a.max(editor.cursor))
            .unwrap_or(editor.cursor);

        let mut cursor_pos = None;

        for i in skip_visual_lines..end_visual_line {
            let v_line_info = self.visual_lines[i];
            let start_byte = v_line_info.byte_idx;
            let end_byte = if i + 1 < self.visual_lines.len() {
                self.visual_lines[i + 1].byte_idx
            } else {
                len
            };

            let y = self.baseline_offset - render_scroll_y + (i as f32 * self.line_height);
            let mut x = self.left_padding;

            if v_line_info.is_soft_wrap {
                self.draw_string("↪", self.left_padding * 0.5, y, self.theme.line_num);
            } else {
                self.draw_string(
                    &v_line_info.physical_line.to_string(),
                    10.0,
                    y,
                    self.theme.line_num,
                );
            }

            let mut span_idx = match spans.binary_search_by_key(&start_byte, |s| s.start) {
                Ok(idx) => idx,
                Err(idx) => idx.saturating_sub(1),
            };

            let mut search_idx = search_results.partition_point(|&(_, e)| e <= start_byte);

            let mut current_offset = start_byte;
            let mut current_chunk_offset = start_byte;

            while current_chunk_offset < end_byte {
                let s = if current_chunk_offset < first_len {
                    let s_end = end_byte.min(first_len);
                    &first[current_chunk_offset..s_end]
                } else {
                    let s_start = current_chunk_offset - first_len;
                    let s_end = end_byte - first_len;
                    &second[s_start..s_end]
                };

                for c in s.chars() {
                    if cursor_pos.is_none() && current_offset >= editor.cursor {
                        cursor_pos = Some((x, y));
                    }

                    while span_idx < spans.len() && spans[span_idx].end <= current_offset {
                        span_idx += 1;
                    }

                    while search_idx < search_results.len()
                        && search_results[search_idx].1 <= current_offset
                    {
                        search_idx += 1;
                    }

                    let char_len = c.len_utf8();
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

                    if is_search_res {
                        let w = if is_newline { 10.0 } else { adv };
                        let color = if is_active_search {
                            [1.0, 0.6, 0.0, 0.5]
                        } else {
                            [0.6, 0.6, 0.6, 0.35]
                        };
                        self.push_rect(
                            x,
                            y - self.baseline_offset + 2.0,
                            w,
                            self.line_height,
                            color,
                        );
                    } else if current_offset >= sel_start && current_offset < sel_end {
                        let w = if is_newline { 10.0 } else { adv };
                        self.push_rect(
                            x,
                            y - self.baseline_offset + 2.0,
                            w,
                            self.line_height,
                            self.theme.sel,
                        );
                    }

                    if !is_newline && !is_hidden && c != ' ' && c != '\t' {
                        if let Some(g) = self.get_glyph(c) {
                            let mut current_color = self.theme.fg;
                            if span_idx < spans.len() && spans[span_idx].start <= current_offset {
                                current_color = spans[span_idx].color;
                            }

                            self.push_quad(
                                x + g.offset_x,
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
                                    x + g.offset_x + 1.0,
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

                    x += adv;
                    current_offset += char_len;
                }

                if current_chunk_offset < first_len {
                    current_chunk_offset = first_len;
                } else {
                    current_chunk_offset = end_byte;
                }
            }
        }

        if cursor_pos.is_none() && editor.cursor == len {
            let last_line_idx = (self.visual_lines.len() - 1).max(0);
            let y =
                self.baseline_offset - render_scroll_y + (last_line_idx as f32 * self.line_height);
            let (first, second) = editor.text_parts();
            let x = self.left_padding
                + self.measure_width(
                    first,
                    second,
                    self.visual_lines[last_line_idx].byte_idx,
                    editor.cursor,
                );
            cursor_pos = Some((x, y));
        }

        if let Some((cx, cy)) = cursor_pos {
            if sel_start == sel_end && blink_alpha > 0.5 && !show_quit_dialog && !search_focused {
                if cy > -self.line_height && cy < self.height + self.line_height {
                    self.push_rect(
                        cx,
                        cy - self.baseline_offset + 2.0,
                        2.0,
                        self.line_height - 2.0,
                        self.theme.fg,
                    );
                }
            }
        }

        let total_lines_f32 = self.visual_lines.len().max(1) as f32;
        let minimap_line_h = (self.height / total_lines_f32).min(3.0);
        let track_h = (total_lines_f32 * minimap_line_h).min(self.height);

        self.push_rect(
            minimap_x,
            0.0,
            minimap_w,
            self.height,
            [self.theme.bg[0], self.theme.bg[1], self.theme.bg[2], 1.0],
        );
        self.push_rect(minimap_x, 0.0, minimap_w, track_h, solid_minimap_bg);

        let current_spans_ver =
            (spans.len() as u64) ^ (spans.last().map(|s| s.end).unwrap_or(0) as u64);

        if self.last_minimap_editor_version != editor.version
            || self.last_minimap_spans_version != current_spans_ver
            || self.minimap_vertices.is_empty()
            || (self.last_minimap_width - self.width).abs() > 0.5
        {
            self.minimap_vertices.clear();
            let map_bg = self.theme.minimap_bg;

            let push_mini =
                |verts: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]| {
                    let x1 = x.round();
                    let y1 = y.round();
                    let x2 = (x + w).round();
                    let y2 = (y + h).round();
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
                    verts.extend_from_slice(&[v1, v2, v3, v1, v3, v4]);
                };

            let step = if minimap_line_h < 1.0 {
                (1.0 / minimap_line_h).ceil() as usize
            } else {
                1
            };

            let draw_h = (minimap_line_h * step as f32).max(1.0);

            for i in (0..self.visual_lines.len()).step_by(step) {
                let y_pixel = (i as f32) * minimap_line_h;
                if y_pixel > self.height {
                    break;
                }

                let start_byte = self.visual_lines[i].byte_idx;
                let end_byte = if i + 1 < self.visual_lines.len() {
                    self.visual_lines[i + 1].byte_idx
                } else {
                    editor.len()
                };

                let mut current_x = minimap_x + 5.0;
                let mut cur_byte = start_byte;

                let mut span_idx_mini = match spans.binary_search_by_key(&cur_byte, |s| s.start) {
                    Ok(idx) => idx,
                    Err(idx) => idx.saturating_sub(1),
                };

                while cur_byte < end_byte {
                    let b = editor.byte_at(cur_byte);
                    if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
                        current_x += 1.2;
                        cur_byte += 1;
                        continue;
                    }

                    while span_idx_mini < spans.len() && spans[span_idx_mini].end <= cur_byte {
                        span_idx_mini += 1;
                    }

                    let (span_end, raw_color) = if span_idx_mini < spans.len() {
                        let s = &spans[span_idx_mini];
                        if s.start <= cur_byte {
                            (s.end.min(end_byte), s.color)
                        } else {
                            (s.start.min(end_byte), self.theme.fg)
                        }
                    } else {
                        (end_byte, self.theme.fg)
                    };

                    let color = [
                        raw_color[0] * 0.7 + map_bg[0] * 0.3,
                        raw_color[1] * 0.7 + map_bg[1] * 0.3,
                        raw_color[2] * 0.7 + map_bg[2] * 0.3,
                        1.0,
                    ];

                    let mut chunk_end = cur_byte;
                    while chunk_end < span_end {
                        let b_next = editor.byte_at(chunk_end);
                        if b_next == b' ' || b_next == b'\t' || b_next == b'\n' || b_next == b'\r' {
                            break;
                        }
                        chunk_end += 1;
                    }

                    let byte_len = chunk_end.saturating_sub(cur_byte);
                    let w = (byte_len as f32 * 1.2).min(minimap_x + minimap_w - 5.0 - current_x);

                    if w > 0.0 {
                        push_mini(
                            &mut self.minimap_vertices,
                            current_x,
                            y_pixel,
                            w,
                            draw_h,
                            color,
                        );
                        current_x += w;
                    }

                    cur_byte = chunk_end;
                    if current_x >= minimap_x + minimap_w - 5.0 {
                        break;
                    }
                }
            }
            self.last_minimap_editor_version = editor.version;
            self.last_minimap_spans_version = current_spans_ver;
            self.last_minimap_width = self.width;
        }

        self.vertices.extend_from_slice(&self.minimap_vertices);

        self.push_rect(
            minimap_x,
            (cursor_line_idx as f32) * minimap_line_h,
            minimap_w,
            2.0,
            self.theme.minimap_cursor,
        );

        let visible_lines = self.height / self.line_height;
        let viewport_h = (visible_lines * minimap_line_h).max(10.0).min(track_h);
        let max_scroll = self.get_max_scroll(editor, self.height);
        let scroll_ratio = if max_scroll > 0.0 {
            (render_scroll_y / max_scroll).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let viewport_y = scroll_ratio * (track_h - viewport_h).max(0.0);

        let view_bg = [
            self.theme.minimap_bg[0] * 0.7 + self.theme.sel[0] * 0.3,
            self.theme.minimap_bg[1] * 0.7 + self.theme.sel[1] * 0.3,
            self.theme.minimap_bg[2] * 0.7 + self.theme.sel[2] * 0.3,
            0.3,
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

        if show_fps {
            let fps_str = format!("FPS: {:.0}", self.fps);
            let center_x = (self.width - minimap_w) / 2.0;
            self.push_rect(center_x - 45.0, 5.0, 90.0, 25.0, [0.1, 0.1, 0.1, 0.8]);
            self.draw_string(&fps_str, center_x - 40.0, 24.0, [0.0, 1.0, 0.0, 1.0]);
        }

        if search_anim_y > -70.0 {
            let s = self.scale_factor;
            let search_w = 480.0 * s;
            let search_h = 46.0 * s;
            let search_x = minimap_x - search_w - 20.0 * s;

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

            let res_text = if search_results.is_empty() {
                if search_editor.get_full_text().is_empty() {
                    String::new()
                } else {
                    "Нет".to_string()
                }
            } else {
                format!(
                    "{}/{}",
                    search_current_idx.unwrap_or(0) + 1,
                    search_results.len()
                )
            };

            if !res_text.is_empty() {
                let counter_x = input_x + input_w + 10.0 * s;
                self.draw_string_scaled(&res_text, counter_x, text_y, [0.6, 0.6, 0.6, 1.0], 0.9);
            }

            let mx = self.last_mouse_x;
            let my = self.last_mouse_y;

            btn_case.render(self, mx, my, s, false);
            btn_up.render(self, mx, my, s, false);
            btn_down.render(self, mx, my, s, false);
            btn_close.render(self, mx, my, s, false);
        }

        if show_quit_dialog {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.6]);
        }
        self.flush();
    }
}
