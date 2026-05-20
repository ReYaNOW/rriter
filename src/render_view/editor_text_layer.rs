use crate::editor::Editor;
use crate::highlighter::ColorSpan;
use crate::render_view::ModInterval;
use crate::renderer::Renderer;

fn folded_import_keyword_range(
    editor: &Editor,
    line_start: usize,
    line_end: usize,
) -> Option<(usize, usize)> {
    let mut start = line_start;
    while start < line_end {
        let b = editor.byte_at(start);
        if b != b' ' && b != b'\t' {
            break;
        }
        start += 1;
    }

    if starts_with_word(editor, start, line_end, b"from")
        || starts_with_word(editor, start, line_end, b"import")
        || starts_with_word(editor, start, line_end, b"use")
    {
        let end = word_end(editor, start, line_end);
        return (end > start).then_some((start, end));
    }

    if starts_with_word(editor, start, line_end, b"pub") {
        let mut p = word_end(editor, start, line_end);
        while p < line_end && (editor.byte_at(p) == b' ' || editor.byte_at(p) == b'\t') {
            p += 1;
        }
        if p < line_end && editor.byte_at(p) == b'(' {
            while p < line_end && editor.byte_at(p) != b')' {
                p += 1;
            }
            if p < line_end {
                p += 1;
            }
            while p < line_end && (editor.byte_at(p) == b' ' || editor.byte_at(p) == b'\t') {
                p += 1;
            }
        }
        if starts_with_word(editor, p, line_end, b"use") {
            let end = word_end(editor, p, line_end);
            return (end > p).then_some((p, end));
        }
    }

    None
}

pub(super) fn folded_import_display_end(
    editor: &Editor,
    line_start: usize,
    line_end: usize,
) -> usize {
    folded_import_keyword_range(editor, line_start, line_end)
        .map(|(_, end)| end)
        .unwrap_or(line_end)
}

fn starts_with_word(editor: &Editor, start: usize, line_end: usize, word: &[u8]) -> bool {
    if start + word.len() > line_end {
        return false;
    }
    for (i, b) in word.iter().enumerate() {
        if editor.byte_at(start + i) != *b {
            return false;
        }
    }
    let end = start + word.len();
    end == line_end || !is_ident_byte(editor.byte_at(end))
}

fn word_end(editor: &Editor, mut p: usize, line_end: usize) -> usize {
    while p < line_end && is_ident_byte(editor.byte_at(p)) {
        p += 1;
    }
    p
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor_with_text(text: &str) -> Editor {
        let mut editor = Editor::new(text.len() + 16);
        editor.insert_str(text);
        editor
    }

    #[test]
    fn folded_import_keyword_range_covers_collapsed_click_words() {
        let editor = editor_with_text("from os import path\nimport sys\n");
        assert_eq!(folded_import_keyword_range(&editor, 0, 19), Some((0, 4)));
        assert_eq!(folded_import_display_end(&editor, 0, 19), 4);

        let rust = editor_with_text("pub(crate) use crate::x;\n");
        assert_eq!(folded_import_keyword_range(&rust, 0, 24), Some((11, 14)));
        assert_eq!(folded_import_display_end(&rust, 0, 24), 14);
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    fn inlay_text_bounds_y(&mut self, text: &str, scale: f32) -> Option<(f32, f32)> {
        let mut top = 0.0f32;
        let mut bottom = 0.0f32;
        let mut seen = false;
        for c in text.chars() {
            if c == '\n' || c == '\r' || c == '\u{FE0F}' || c == '\u{200D}' {
                continue;
            }
            if let Some(g) = self.get_ui_glyph(c) {
                let glyph_top = -g.offset_y * scale;
                let glyph_bottom = (g.height - g.offset_y) * scale;
                if seen {
                    top = top.min(glyph_top);
                    bottom = bottom.max(glyph_bottom);
                } else {
                    top = glyph_top;
                    bottom = glyph_bottom;
                    seen = true;
                }
            }
        }
        seen.then_some((top, bottom))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_editor_visible_text(
        &mut self,
        editor: &Editor,
        spans: &[ColorSpan],
        search_results: &[(usize, usize)],
        search_current_idx: Option<usize>,
        first: &str,
        second: &str,
        indent_levels: &[u8],
        first_len: usize,
        len: usize,
        bracket_pairs: Option<(usize, usize)>,
        sel_start: usize,
        sel_end: usize,
        render_scroll_x: f32,
        render_scroll_y: f32,
        scrollbar_x: f32,
        blink_alpha: f32,
        dialog_window_open: bool,
        editor_cursor_blocked: bool,
        show_settings: bool,
        s: f32,
        skip_visual_lines: usize,
        end_visual_line: usize,
        ui_registry: &mut crate::ui_system::UiRegistry,
        ctrl_definition_range: Option<(usize, usize)>,
        diff_line_kinds: Option<&[crate::app::git_diff::DiffLineKind]>,
        python_inlay_hints: &[crate::app::PythonInlayHint],
    ) {
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
                        let screen_x = guide_x - render_scroll_x;

                        if screen_x > self.width {
                            break;
                        }
                        if screen_x < self.left_padding - 10.0 {
                            continue;
                        }

                        let margin = space_adv * 0.5;
                        let overlaps = v_line.text_px_width > 0.0
                            && text_start_x <= guide_x + margin
                            && text_end_x >= guide_x - margin;

                        if !overlaps {
                            self.push_rect(
                                screen_x.round(),
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

        self.mod_intervals_cache.clear();
        self.merged_intervals_cache.clear();
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
                    self.mod_intervals_cache.push(ModInterval {
                        top: y_top - 3.0,
                        bottom: y_top + 3.0,
                        state: st,
                    });
                }
            }

            if let Some(st) = editor.get_line_modification_state(phys_idx) {
                self.mod_intervals_cache.push(ModInterval {
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
                    self.mod_intervals_cache.push(ModInterval {
                        top: last_bottom_y - 3.0,
                        bottom: last_bottom_y + 3.0,
                        state: st,
                    });
                }
            }
        }

        for int in &self.mod_intervals_cache {
            let mut merged = false;
            if let Some(last) = self.merged_intervals_cache.last_mut() {
                if int.top <= last.bottom + 0.1 && int.state == last.state {
                    last.bottom = last.bottom.max(int.bottom);
                    merged = true;
                }
            }
            if !merged {
                self.merged_intervals_cache.push(*int);
            }
        }

        let mut cursor_pos = None;
        let hint_bg = [
            (self.theme.bg[0] + 0.055).min(1.0),
            (self.theme.bg[1] + 0.055).min(1.0),
            (self.theme.bg[2] + 0.070).min(1.0),
            1.0,
        ];

        for i in skip_visual_lines..end_visual_line {
            let v_line_info = self.visual_lines[i];
            let start_byte = v_line_info.byte_idx;
            let phys_idx = v_line_info.physical_line - 1;

            let mut end_byte = if v_line_info.is_folded {
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
            if v_line_info.is_folded {
                end_byte = folded_import_display_end(editor, start_byte, end_byte);
            }

            let y = self.baseline_offset + v_line_info.y_offset - render_scroll_y;
            let mut x = self.left_padding;

            if !v_line_info.is_soft_wrap
                && let Some(kind) = diff_line_kinds
                    .and_then(|kinds| kinds.get(phys_idx))
                    .copied()
            {
                let color = match kind {
                    crate::app::git_diff::DiffLineKind::Added
                    | crate::app::git_diff::DiffLineKind::ModifiedNew => {
                        Some([0.18, 0.82, 0.34, 0.26])
                    }
                    crate::app::git_diff::DiffLineKind::Deleted
                    | crate::app::git_diff::DiffLineKind::ModifiedOld => {
                        Some([0.76, 0.78, 0.84, 0.24])
                    }
                    crate::app::git_diff::DiffLineKind::Context => None,
                };
                if let Some(color) = color {
                    self.push_rect(
                        self.left_padding,
                        y - self.baseline_offset,
                        (scrollbar_x - self.left_padding).max(0.0),
                        self.line_height,
                        color,
                    );
                }
            }

            let mut span_idx = match spans.binary_search_by_key(&start_byte, |s| s.start) {
                Ok(idx) => idx,
                Err(idx) => idx.saturating_sub(1),
            };

            let mut search_idx = search_results.partition_point(|&(_, e)| e <= start_byte);
            let mut identical_idx = self
                .identical_words_cache
                .partition_point(|&(_, e)| e <= start_byte);
            let mut unused_idx = self
                .unused_spans_cache
                .partition_point(|&(_, e)| e <= start_byte);
            let mut inlay_idx = python_inlay_hints.partition_point(|hint| {
                hint.byte_offset < start_byte
            });

            let mut current_offset = start_byte;
            let mut current_chunk_offset = start_byte;
            let folded_keyword_range = if v_line_info.is_folded {
                folded_import_keyword_range(editor, start_byte, end_byte)
            } else {
                None
            };

            let mut out_of_bounds = false;

            while current_chunk_offset < end_byte {
                if self.vertices.len() > crate::renderer::MAX_VERTICES - 2000 {
                    self.flush();
                }

                let chunk = if current_chunk_offset < first_len {
                    let s_end = end_byte.min(first_len);
                    &first[current_chunk_offset..s_end]
                } else {
                    let s_start = current_chunk_offset - first_len;
                    let s_end = end_byte - first_len;
                    &second[s_start..s_end]
                };

                for c in chunk.chars() {
                    if x - render_scroll_x > self.width + 150.0 {
                        out_of_bounds = true;
                        break;
                    }

                    let char_len = c.len_utf8();
                    if cursor_pos.is_none() && editor.cursor == current_offset {
                        cursor_pos = Some((x - render_scroll_x, y));
                    }
                    while inlay_idx < python_inlay_hints.len()
                        && python_inlay_hints[inlay_idx].byte_offset < current_offset
                    {
                        inlay_idx += 1;
                    }
                    while inlay_idx < python_inlay_hints.len()
                        && python_inlay_hints[inlay_idx].byte_offset == current_offset
                    {
                        let hint = &python_inlay_hints[inlay_idx];
                        let hint_label = hint.label.trim_end();
                        let hint_scale = 0.92;
                        let pad = 4.0 * s;
                        let hint_w = self.measure_ui_width(hint_label, hint_scale);
                        let pill_w = hint_w + pad * 2.0;
                        let value_gap_w = self.char_advance(' ');
                        if x - render_scroll_x < self.width + 150.0 {
                            let (text_top, text_bottom) = self
                                .inlay_text_bounds_y(hint_label, hint_scale)
                                .unwrap_or((-self.baseline_offset * hint_scale, 0.0));
                            let text_center_y = y.round() + (text_top + text_bottom) * 0.5;
                            let pill_h = (text_bottom - text_top) + pad * 2.0;
                            let pill_y = (text_center_y - pill_h * 0.5).round();
                            self.push_rounded_rect(
                                (x - render_scroll_x).round(),
                                pill_y,
                                pill_w,
                                pill_h,
                                4.0 * s,
                                hint_bg,
                            );
                            self.draw_string_scaled(
                                hint_label,
                                (x - render_scroll_x + pad).round(),
                                y.round(),
                                self.theme.fg,
                                hint_scale,
                            );
                        }
                        x += pill_w + value_gap_w;
                        inlay_idx += 1;
                    }

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
                    while identical_idx < self.identical_words_cache.len()
                        && self.identical_words_cache[identical_idx].1 <= current_offset
                    {
                        identical_idx += 1;
                    }
                    while unused_idx < self.unused_spans_cache.len()
                        && self.unused_spans_cache[unused_idx].1 <= current_offset
                    {
                        unused_idx += 1;
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

                    let is_identical = identical_idx < self.identical_words_cache.len()
                        && current_offset >= self.identical_words_cache[identical_idx].0;

                    let is_unused = unused_idx < self.unused_spans_cache.len()
                        && current_offset >= self.unused_spans_cache[unused_idx].0;
                    let is_ctrl_definition = ctrl_definition_range.is_some_and(|(start, end)| {
                        current_offset >= start && current_offset < end
                    });

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

                    if is_ctrl_definition && !is_newline && !is_hidden && adv > 0.0 {
                        self.push_rect(
                            x - render_scroll_x,
                            y - self.baseline_offset + self.line_height - 3.0 * s,
                            adv,
                            1.5 * s,
                            [0.545, 0.913, 0.992, 0.95],
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
                                if folded_keyword_range.is_some_and(|(start, end)| {
                                    current_offset >= start && current_offset < end
                                }) {
                                    current_color = [0.55, 0.62, 0.80, 1.0];
                                }

                                if is_unused {
                                    current_color = self.theme.unused;
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
                for i in 0..v_line_info.fold_suffix_len {
                    final_x += self.char_advance(v_line_info.fold_suffix[i as usize]);
                }

                let hit_y_top = y - self.line_height;
                let hit_y_bottom = y + 5.0 * s;
                if let Some((word_start, word_end)) =
                    folded_import_keyword_range(editor, start_byte, end_byte)
                {
                    let word_x = self.left_padding
                        + self.measure_width(first, second, start_byte, word_start)
                        - render_scroll_x;
                    let word_w = self.measure_width(first, second, word_start, word_end);
                    ui_registry.register_rect(
                        crate::ui_system::UiId::EditorFoldDots(phys_idx),
                        word_x - 2.0 * s,
                        hit_y_top,
                        word_w + 4.0 * s,
                        hit_y_bottom - hit_y_top,
                        self.last_mouse_x,
                        self.last_mouse_y,
                    );
                }
                let hit_w = next_x + 10.0 * s - (box_x - 2.0 * s);
                ui_registry.register_rect(
                    crate::ui_system::UiId::EditorFoldDots(phys_idx),
                    box_x - 2.0 * s,
                    hit_y_top,
                    hit_w,
                    hit_y_bottom - hit_y_top,
                    self.last_mouse_x,
                    self.last_mouse_y,
                );

                if cursor_pos.is_none()
                    && editor.cursor >= end_byte
                    && editor.cursor <= actual_end_byte
                {
                    cursor_pos = Some((final_x, y));
                }

                self.push_rounded_rect(box_x, box_y_draw, box_w, box_h_draw, 4.0 * s, dots_bg);

                self.draw_string_scaled(
                    dots_str,
                    box_x + 3.0 * s,
                    y,
                    crate::highlighter::DRACULA_COMMENT,
                    1.0,
                );

                let mut suffix_draw_x = next_x;
                for i in 0..v_line_info.fold_suffix_len {
                    let c = v_line_info.fold_suffix[i as usize];
                    let c_adv = self.char_advance(c);

                    if is_dots_selected {
                        self.push_rect(
                            suffix_draw_x,
                            y - self.baseline_offset + 2.0,
                            c_adv,
                            self.line_height,
                            self.theme.sel,
                        );
                    }

                    if let Some(g) = self.get_glyph(c) {
                        self.push_quad(
                            suffix_draw_x + g.offset_x,
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
                    suffix_draw_x += c_adv;
                }
            }
        }

        if cursor_pos.is_none() && editor.cursor == len {
            if let Some(last_line) = self.visual_lines.last() {
                let last_byte_idx = last_line.byte_idx;
                let last_y_offset = last_line.y_offset;
                let y = self.baseline_offset + last_y_offset - render_scroll_y;
                let (first, second) = editor.text_parts();
                let x = self.left_padding
                    + self.measure_width(first, second, last_byte_idx, editor.cursor)
                    + self.current_inlay_width_before(last_byte_idx, editor.cursor);
                cursor_pos = Some((x - render_scroll_x, y));
            }
        }

        if let Some((cx_screen, cy)) = cursor_pos {
            if sel_start == sel_end
                && blink_alpha > 0.5
                && !dialog_window_open
                && !editor_cursor_blocked
                && !show_settings
            {
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
    }
}
