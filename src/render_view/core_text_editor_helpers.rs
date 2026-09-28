fn pixel_stable_glyph_rect(
    draw_x: f32,
    baseline_y: f32,
    glyph: crate::renderer::GlyphInfo,
    scale: f32,
) -> Option<(f32, f32, f32, f32)> {
    if glyph.width <= 0.0 || glyph.height <= 0.0 {
        return None;
    }
    Some(glyph_quad_rect(
        draw_x.round(),
        baseline_y.round(),
        glyph,
        scale,
    ))
}

#[inline(always)]
fn editor_glyph_pass_x_positions(ch: char, q_x: f32) -> ([f32; 2], usize) {
    if matches!(ch, '.' | ':') {
        ([q_x, q_x + 1.0], 2)
    } else {
        ([q_x, q_x], 1)
    }
}

#[inline(always)]
pub(crate) fn text_char_is_non_rendering_control(ch: char) -> bool {
    matches!(ch, '\u{FE0F}' | '\u{200D}')
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
        let ch_len = ch.len_utf8();
        if text_char_is_non_rendering_control(ch) {
            current_offset = current_offset.saturating_add(ch_len);
            continue;
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
        current_offset = current_offset.saturating_add(ch_len);
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
