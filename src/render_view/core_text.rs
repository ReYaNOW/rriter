use super::editor_text_layer::folded_import_display_end;
use crate::editor::Editor;
use crate::renderer::{Renderer, Vertex, VisualLine, glyph_quad_rect};
use glow::HasContext;

fn pixel_snapped_ui_glyph_rect(
    draw_x: f32,
    baseline_y: f32,
    offset_x: f32,
    offset_y: f32,
    width: f32,
    height: f32,
    scale: f32,
) -> Option<(f32, f32, f32, f32)> {
    let glyph_w = if width > 0.0 {
        (width * scale).round().max(1.0)
    } else {
        0.0
    };
    let glyph_h = if height > 0.0 {
        (height * scale).round().max(1.0)
    } else {
        0.0
    };
    if glyph_w <= 0.0 || glyph_h <= 0.0 {
        return None;
    }
    Some((
        (draw_x + (offset_x * scale).round()).round(),
        (baseline_y - (offset_y * scale).round()).round(),
        glyph_w,
        glyph_h,
    ))
}

fn for_each_spanned_ui_char(
    text: &str,
    spans: &[crate::highlighter::ColorSpan],
    base_offset: Option<usize>,
    mut callback: impl FnMut(char, [f32; 4]),
) {
    let mut current_offset = base_offset.unwrap_or(usize::MAX);
    let mut span_index = base_offset
        .map(
            |offset| match spans.binary_search_by_key(&offset, |span| span.start) {
                Ok(index) => index,
                Err(index) => index.saturating_sub(1),
            },
        )
        .unwrap_or(0);
    for ch in text.chars() {
        if matches!(ch, '\n' | '\r') {
            break;
        }
        let color = if base_offset.is_some() {
            while span_index < spans.len() && spans[span_index].end <= current_offset {
                span_index += 1;
            }
            if span_index < spans.len()
                && spans[span_index].start <= current_offset
                && current_offset < spans[span_index].end
            {
                spans[span_index].color
            } else {
                [f32::NAN; 4]
            }
        } else {
            [f32::NAN; 4]
        };
        callback(ch, color);
        current_offset = current_offset.saturating_add(ch.len_utf8());
    }
}

pub(crate) fn wrapped_text_ranges(
    text: &str,
    max_width: f32,
    mut advance: impl FnMut(char) -> f32,
) -> Vec<(usize, usize)> {
    wrapped_text_ranges_with_offsets(text, max_width, |_, ch| advance(ch))
}

pub(crate) fn wrapped_text_ranges_with_offsets(
    text: &str,
    max_width: f32,
    mut advance: impl FnMut(usize, char) -> f32,
) -> Vec<(usize, usize)> {
    if text.is_empty() {
        return vec![(0, 0)];
    }
    let max_width = max_width.max(1.0);
    let mut lines = Vec::new();
    let mut line_start = 0usize;
    while line_start < text.len() {
        let mut cursor = line_start;
        let mut width = 0.0f32;
        let mut last_break = None;
        let mut line_end = text.len();
        let mut next_start = text.len();

        while cursor < text.len() {
            let ch = text[cursor..].chars().next().unwrap_or('\0');
            let next = cursor + ch.len_utf8();
            if ch == '\n' {
                line_end = cursor;
                next_start = next;
                break;
            }
            let next_width = width + advance(cursor, ch);
            if next_width > max_width && cursor > line_start {
                line_end = last_break.filter(|&offset| offset > line_start).unwrap_or(cursor);
                next_start = line_end;
                break;
            }
            width = next_width;
            cursor = next;
            if ch.is_whitespace() || matches!(ch, ',' | ':' | ';' | ')' | ']') {
                last_break = Some(cursor);
            }
        }

        let mut visible_end = line_end;
        while visible_end > line_start {
            let Some(ch) = text[..visible_end].chars().next_back() else {
                break;
            };
            if !ch.is_whitespace() {
                break;
            }
            visible_end -= ch.len_utf8();
        }
        lines.push((line_start, visible_end));

        line_start = next_start;
        while line_start < text.len() {
            let ch = text[line_start..].chars().next().unwrap_or('\0');
            if ch == '\n' || !ch.is_whitespace() {
                break;
            }
            line_start += ch.len_utf8();
        }
    }
    if text.ends_with('\n') {
        lines.push((text.len(), text.len()));
    }
    lines
}

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
    fn compact_ui_glyphs_snap_offsets_and_sizes_independently() {
        let rect = pixel_snapped_ui_glyph_rect(10.0, 100.0, 0.4, 12.4, 7.4, 10.4, 0.74);

        assert_eq!(rect, Some((10.0, 91.0, 5.0, 8.0)));
    }

    #[test]
    fn compact_ui_glyphs_skip_empty_quads() {
        assert_eq!(
            pixel_snapped_ui_glyph_rect(10.0, 100.0, 0.0, 0.0, 0.0, 10.0, 0.74),
            None
        );
    }

    #[test]
    fn compact_tree_label_stable_geometry_preserves_shared_glyph_edge() {
        let scale = 0.86;
        let baseline = 100.0;
        let glyph = |offset_y: f32, height: f32| crate::renderer::GlyphInfo {
            u: 0.0,
            v: 0.0,
            uw: 1.0,
            vh: 1.0,
            width: 6.0,
            height,
            offset_x: 0.0,
            offset_y,
            advance: 8.0,
            is_emoji: 0.0,
        };
        let stable_bottom = |x: f32, glyph| {
            let (x, y, w, h) = crate::renderer::glyph_quad_rect(x, baseline, glyph, scale);
            crate::renderer::quad_vertices(
                x, y, w, h, 0.0, 0.0, 1.0, 1.0, [1.0; 4], 0.0,
            )[2]
                .pos[1]
        };
        let first = glyph(8.0, 5.98);
        let second = glyph(8.42, 6.4);

        assert_eq!(stable_bottom(10.0, first), stable_bottom(18.0, second));

        let old_first = pixel_snapped_ui_glyph_rect(
            10.0,
            baseline,
            first.offset_x,
            first.offset_y,
            first.width,
            first.height,
            scale,
        )
        .expect("visible first glyph");
        let old_second = pixel_snapped_ui_glyph_rect(
            18.0,
            baseline,
            second.offset_x,
            second.offset_y,
            second.width,
            second.height,
            scale,
        )
        .expect("visible second glyph");
        assert_ne!(old_first.1 + old_first.3, old_second.1 + old_second.3);
    }

    #[test]
    fn compact_tree_label_stable_geometry_is_repeatable_at_fractional_dpi() {
        let glyph = crate::renderer::GlyphInfo {
            u: 0.0,
            v: 0.0,
            uw: 1.0,
            vh: 1.0,
            width: 7.2,
            height: 8.1,
            offset_x: 0.25,
            offset_y: 9.35,
            advance: 8.0,
            is_emoji: 0.0,
        };
        for dpi in [1.0, 1.25, 1.5, 1.75] {
            let scale = 0.86 * dpi;
            let rect = crate::renderer::glyph_quad_rect(12.0, 80.0, glyph, scale);
            let first = crate::renderer::quad_vertices(
                rect.0, rect.1, rect.2, rect.3, 0.0, 0.0, 1.0, 1.0, [1.0; 4], 0.0,
            )
            .map(|vertex| vertex.pos);
            let second = crate::renderer::quad_vertices(
                rect.0, rect.1, rect.2, rect.3, 0.0, 0.0, 1.0, 1.0, [0.5; 4], 0.0,
            )
            .map(|vertex| vertex.pos);
            assert_eq!(first, second);
        }
    }

    #[test]
    fn spanned_ui_chars_keep_utf8_offsets_and_exact_span_colors() {
        let expected = [0.1, 0.2, 0.3, 1.0];
        let spans = vec![crate::highlighter::ColorSpan {
            start: 2,
            end: 6,
            color: expected,
        }];
        let mut seen = Vec::new();
        for_each_spanned_ui_char("xабy", &spans, Some(1), |ch, color| {
            seen.push((ch, color));
        });
        assert!(seen[0].1[0].is_nan());
        assert_eq!(seen[1], ('а', expected));
        assert_eq!(seen[2], ('б', expected));
        assert!(seen[3].1[0].is_nan());
    }

    #[test]
    fn bug_1_database_sql_renderer_uses_shared_spanned_utf8_walk() {
        let expected = [0.1, 0.2, 0.3, 1.0];
        let spans = [crate::highlighter::ColorSpan {
            start: 7,
            end: 9,
            color: expected,
        }];
        let mut seen = Vec::new();
        for_each_spanned_ui_char("SELECT Ж", &spans, Some(0), |ch, color| seen.push((ch, color)));
        assert_eq!(seen.last(), Some(&('Ж', expected)));
    }

    #[test]
    fn bug_2_api_python_renderer_uses_shared_utf8_byte_offsets() {
        let expected = [0.9, 0.4, 0.2, 1.0];
        let spans = [crate::highlighter::ColorSpan {
            start: 2,
            end: 6,
            color: expected,
        }];
        let mut colored = Vec::new();
        for_each_spanned_ui_char("xабy", &spans, Some(1), |ch, color| {
            if !color[0].is_nan() {
                colored.push(ch);
            }
        });
        assert_eq!(colored, vec!['а', 'б']);
    }

    #[test]
    fn bug_3_inline_git_renderer_emits_one_callback_per_character() {
        let text = "a.Ж:b";
        let mut visited = String::new();
        for_each_spanned_ui_char(text, &[], None, |ch, _| visited.push(ch));
        assert_eq!(visited, text);
    }

    #[test]
    fn bug_4_punctuation_is_visited_once_without_duplicate_quad_workaround() {
        let mut chars = Vec::new();
        for_each_spanned_ui_char("a.:b", &[], None, |ch, _| chars.push(ch));
        assert_eq!(chars, vec!['a', '.', ':', 'b']);
        assert_eq!(chars.iter().filter(|&&ch| ch == '.').count(), 1);
        assert_eq!(chars.iter().filter(|&&ch| ch == ':').count(), 1);
    }

    #[test]
    fn wrapped_text_uses_breaks_and_keeps_unicode_boundaries() {
        let text = "Ошибка: очень длинное предупреждение";
        let lines = wrapped_text_ranges(text, 10.0, |_| 1.0);
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|&(start, end)| {
            start <= end && text.is_char_boundary(start) && text.is_char_boundary(end)
        }));
        let mut rebuilt = String::new();
        let mut previous_end = 0usize;
        for &(start, end) in &lines {
            rebuilt.push_str(&text[previous_end..start]);
            rebuilt.push_str(&text[start..end]);
            previous_end = end;
        }
        rebuilt.push_str(&text[previous_end..]);
        assert_eq!(rebuilt, text);
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
    #[inline(always)]
    pub(crate) fn push_editor_glyph(
        &mut self,
        ch: char,
        x: f32,
        baseline_y: f32,
        color: [f32; 4],
    ) {
        let Some(glyph) = self.get_glyph(ch) else {
            return;
        };
        self.push_quad(
            x + glyph.offset_x,
            baseline_y - glyph.offset_y,
            glyph.width,
            glyph.height,
            glyph.u,
            glyph.v,
            glyph.uw,
            glyph.vh,
            color,
            glyph.is_emoji,
        );
        if matches!(ch, '.' | ':') {
            self.push_quad(
                x + glyph.offset_x + 1.0,
                baseline_y - glyph.offset_y,
                glyph.width,
                glyph.height,
                glyph.u,
                glyph.v,
                glyph.uw,
                glyph.vh,
                color,
                glyph.is_emoji,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_spanned_editor_line_alpha(
        &mut self,
        text: &str,
        spans: &[crate::highlighter::ColorSpan],
        base_offset: Option<usize>,
        x: f32,
        y: f32,
        max_x: f32,
        alpha: f32,
    ) -> f32 {
        let mut draw_x = x;
        let alpha = alpha.clamp(0.0, 1.0);
        for_each_spanned_ui_char(text, spans, base_offset, |ch, span_color| {
            if draw_x > max_x {
                return;
            }
            let advance = self.char_advance(ch);
            if !matches!(ch, ' ' | '\t') {
                let mut color = if span_color[0].is_nan() {
                    self.theme.fg
                } else {
                    span_color
                };
                color[3] *= alpha;
                self.push_editor_glyph(ch, draw_x, y, color);
            }
            draw_x += advance;
        });
        draw_x
    }

    #[inline]
    pub(crate) fn snapped_text_advance(advance: f32, scale: f32) -> f32 {
        let px = (advance * scale).round();
        if px <= 0.0 && advance > 0.0 { 1.0 } else { px }
    }

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

        let view_top = (scroll_y - self.line_height * 5.0).max(0.0);
        let view_bottom = scroll_y + self.height + self.line_height * 5.0;
        let mut phys_line = 0;
        let mut current_y = 0.0;
        if self.line_height > 0.0 && self.phys_to_visual.len() == editor.line_offsets.len() {
            let first_visible = (view_top / self.line_height).floor() as usize;
            phys_line = self
                .phys_to_visual
                .partition_point(|&visual_line| visual_line < first_visible)
                .min(editor.line_offsets.len());
            current_y = self
                .phys_to_visual
                .get(phys_line)
                .copied()
                .unwrap_or(first_visible) as f32
                * self.line_height;
        }

        while phys_line < editor.line_offsets.len() {
            let is_folded = editor.folded_lines.contains(&phys_line)
                && editor.foldable_lines.contains_key(&phys_line);

            if current_y > view_bottom {
                break;
            }

            if current_y + self.line_height > view_top && current_y < view_bottom {
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
        self.current_inlay_width_until(line_start, byte_offset, false)
    }

    fn inlay_hint_visual_width(&mut self, idx: usize) -> f32 {
        let hint_label = std::sync::Arc::clone(&self.current_python_inlay_hints[idx].label);
        self.measure_ui_width(hint_label.trim_end(), 0.92)
            + 8.0 * self.scale_factor
            + self.char_advance(' ')
    }

    fn current_inlay_width_until(
        &mut self,
        line_start: usize,
        byte_offset: usize,
        include_at_offset: bool,
    ) -> f32 {
        let mut width = 0.0;
        for idx in 0..self.current_python_inlay_hints.len() {
            let hint_offset = self.current_python_inlay_hints[idx].byte_offset;
            if hint_offset > byte_offset || (!include_at_offset && hint_offset == byte_offset) {
                break;
            }
            if hint_offset >= line_start {
                width += self.inlay_hint_visual_width(idx);
            }
        }
        width
    }

    pub(crate) fn visual_x_for_byte_offset(
        &mut self,
        editor: &Editor,
        line_start: usize,
        byte_offset: usize,
        include_inlays_at_offset: bool,
    ) -> f32 {
        let (first, second) = editor.text_parts();
        self.measure_width(first, second, line_start, byte_offset)
            + self.current_inlay_width_until(line_start, byte_offset, include_inlays_at_offset)
    }

    pub(crate) fn visual_text_range_contains_x(
        &mut self,
        editor: &Editor,
        line_start: usize,
        start_byte: usize,
        end_byte: usize,
        target_x: f32,
        min_width: f32,
    ) -> bool {
        if end_byte < start_byte {
            return false;
        }

        let mut segment_start_x =
            self.visual_x_for_byte_offset(editor, line_start, start_byte, true);
        let mut hint_idx = self
            .current_python_inlay_hints
            .partition_point(|hint| hint.byte_offset < start_byte);

        while hint_idx < self.current_python_inlay_hints.len() {
            let hint_offset = self.current_python_inlay_hints[hint_idx].byte_offset;
            if hint_offset >= end_byte {
                break;
            }

            let before_hint_x =
                self.visual_x_for_byte_offset(editor, line_start, hint_offset, false);
            if target_x >= segment_start_x && target_x <= before_hint_x.max(segment_start_x) {
                return true;
            }
            while hint_idx < self.current_python_inlay_hints.len()
                && self.current_python_inlay_hints[hint_idx].byte_offset == hint_offset
            {
                hint_idx += 1;
            }
            segment_start_x = self.visual_x_for_byte_offset(editor, line_start, hint_offset, true);
        }

        let end_x = self.visual_x_for_byte_offset(editor, line_start, end_byte, false);
        let segment_end_x = end_x.max(segment_start_x + min_width);
        target_x >= segment_start_x && target_x <= segment_end_x
    }

    pub(crate) fn text_x_for_visual_line_x(
        &mut self,
        editor: &Editor,
        line_idx: usize,
        visual_x: f32,
    ) -> f32 {
        let line_start = editor.line_offsets.get(line_idx).copied().unwrap_or(0);
        let line_end = editor
            .line_offsets
            .get(line_idx + 1)
            .map(|&o| o.saturating_sub(1))
            .unwrap_or_else(|| editor.len());
        let mut hint_idx = self
            .current_python_inlay_hints
            .partition_point(|hint| hint.byte_offset < line_start);
        let mut text_x = 0.0f32;
        let mut visual_cursor_x = 0.0f32;
        let mut result = 0.0f32;
        let mut found = false;

        editor.utf16_col_to_byte_advance(line_idx, |ch, _utf16_before, pos| {
            if found {
                return;
            }
            while hint_idx < self.current_python_inlay_hints.len()
                && self.current_python_inlay_hints[hint_idx].byte_offset < pos
            {
                visual_cursor_x += self.inlay_hint_visual_width(hint_idx);
                hint_idx += 1;
            }
            while hint_idx < self.current_python_inlay_hints.len()
                && self.current_python_inlay_hints[hint_idx].byte_offset == pos
            {
                let hint_w = self.inlay_hint_visual_width(hint_idx);
                if visual_x <= visual_cursor_x + hint_w {
                    result = text_x;
                    found = true;
                    return;
                }
                visual_cursor_x += hint_w;
                hint_idx += 1;
            }

            let adv = self.char_advance(ch);
            if visual_x <= visual_cursor_x + adv {
                result = text_x + (visual_x - visual_cursor_x).clamp(0.0, adv);
                found = true;
                return;
            }
            visual_cursor_x += adv;
            text_x += adv;
        });

        while !found
            && hint_idx < self.current_python_inlay_hints.len()
            && self.current_python_inlay_hints[hint_idx].byte_offset <= line_end
        {
            let hint_w = self.inlay_hint_visual_width(hint_idx);
            if visual_x <= visual_cursor_x + hint_w {
                result = text_x;
                found = true;
                break;
            }
            visual_cursor_x += hint_w;
            hint_idx += 1;
        }

        if found { result } else { text_x }
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
                    let hint_label =
                        std::sync::Arc::clone(&self.current_python_inlay_hints[inlay_idx].label);
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
                    let hint_label =
                        std::sync::Arc::clone(&self.current_python_inlay_hints[inlay_idx].label);
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
        let telemetry_start = super::TELEMETRY_ENABLED
            .load(std::sync::atomic::Ordering::Relaxed)
            .then(std::time::Instant::now);

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
        if let Some(start) = telemetry_start {
            super::TELEMETRY.with(|telemetry| {
                let mut telemetry = telemetry.borrow_mut();
                let elapsed = start.elapsed().as_secs_f32();
                telemetry.flush_time += elapsed;
                telemetry.flush_count += 1;
                telemetry.flush_max_time = telemetry.flush_max_time.max(elapsed);
                telemetry.flush_vertices += vertex_count as u64;
            });
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

        self.ensure_vertex_capacity(6);
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

        self.ensure_vertex_capacity(6);
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

    /// UI text restriction: draw on integer baselines and move by integer advances.
    /// Fractional UI text placement makes glyphs shimmer/thin during blink, scroll and hover redraws.
    pub fn draw_string_scaled(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        color: [f32; 4],
        scale: f32,
    ) {
        self.draw_string_scaled_stable(text, x, y, color, scale);
    }

    pub fn draw_string_scaled_stable(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        color: [f32; 4],
        scale: f32,
    ) {
        let mut draw_x = x.round();
        let y = y.round();
        for c in text.chars() {
            if c == '\n' || c == '\r' || c == '\u{FE0F}' || c == '\u{200D}' {
                continue;
            }
            if let Some(g) = self.get_ui_glyph(c) {
                let (q_x, q_y, q_w, q_h) = glyph_quad_rect(draw_x, y, g, scale);
                self.push_quad(q_x, q_y, q_w, q_h, g.u, g.v, g.uw, g.vh, color, g.is_emoji);
                draw_x += Self::snapped_text_advance(g.advance, scale);
            }
        }
    }

    pub(crate) fn draw_string_scaled_pixel_snapped(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        color: [f32; 4],
        scale: f32,
    ) {
        let mut draw_x = x.round();
        let baseline_y = y.round();
        for c in text.chars() {
            if c == '\n' || c == '\r' || c == '\u{FE0F}' || c == '\u{200D}' {
                continue;
            }
            if let Some(g) = self.get_ui_glyph(c) {
                if let Some((q_x, q_y, q_w, q_h)) = pixel_snapped_ui_glyph_rect(
                    draw_x, baseline_y, g.offset_x, g.offset_y, g.width, g.height, scale,
                ) {
                    self.push_quad(
                        q_x, q_y, q_w, q_h, g.u, g.v, g.uw, g.vh, color, g.is_emoji,
                    );
                }
                draw_x += Self::snapped_text_advance(g.advance, scale);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_spanned_ui_line_pixel_snapped(
        &mut self,
        text: &str,
        spans: &[crate::highlighter::ColorSpan],
        base_offset: Option<usize>,
        x: f32,
        y: f32,
        max_x: f32,
        scale: f32,
    ) {
        let _ = self.draw_spanned_ui_line_pixel_snapped_alpha(
            text, spans, base_offset, x, y, max_x, scale, 1.0,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_spanned_ui_line_pixel_snapped_alpha(
        &mut self,
        text: &str,
        spans: &[crate::highlighter::ColorSpan],
        base_offset: Option<usize>,
        x: f32,
        y: f32,
        max_x: f32,
        scale: f32,
        alpha: f32,
    ) -> f32 {
        let mut draw_x = x.round();
        let baseline_y = y.round();
        let alpha = alpha.clamp(0.0, 1.0);
        for_each_spanned_ui_char(text, spans, base_offset, |ch, span_color| {
            if draw_x > max_x {
                return;
            }
            if let Some(glyph) = self.get_ui_glyph(ch) {
                let mut color = if span_color[0].is_nan() { self.theme.fg } else { span_color };
                color[3] *= alpha;
                if ch != ' ' && ch != '\t'
                    && let Some((q_x, q_y, q_w, q_h)) = pixel_snapped_ui_glyph_rect(
                        draw_x,
                        baseline_y,
                        glyph.offset_x,
                        glyph.offset_y,
                        glyph.width,
                        glyph.height,
                        scale,
                    )
                {
                    self.push_quad(
                        q_x,
                        q_y,
                        q_w,
                        q_h,
                        glyph.u,
                        glyph.v,
                        glyph.uw,
                        glyph.vh,
                        color,
                        glyph.is_emoji,
                    );
                }
                draw_x += Self::snapped_text_advance(glyph.advance, scale);
            }
        });
        draw_x
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

        self.ensure_vertex_capacity(6);
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

        self.ensure_vertex_capacity(6);
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
