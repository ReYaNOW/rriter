use super::{HoverPopup, HoverState};

thread_local! {
    pub static HOVER_STATE: std::cell::RefCell<HoverState> = std::cell::RefCell::new(HoverState::default());
}

pub const HOVER_REQUEST_DELAY_SEC: f32 = 0.34;
pub const HOVER_POPUP_ANIM_SPEED: f32 = 8.0;
pub const HOVER_POPUP_ANIM_SNAP_EPS: f32 = 0.0005;

pub fn advance_hover_anim_progress(progress: f32, dt: f32) -> f32 {
    if progress >= 1.0 {
        return 1.0;
    }
    let next = progress + (1.0 - progress) * HOVER_POPUP_ANIM_SPEED * dt;
    if 1.0 - next <= HOVER_POPUP_ANIM_SNAP_EPS {
        1.0
    } else {
        next
    }
}

#[allow(dead_code)]
pub fn compute_hover_visibility(
    is_error_hovered: bool,
    error_timer_ready: bool,
    has_type_popup: bool,
    hovered_diag_type_target: Option<usize>,
    type_popup_byte: Option<usize>,
    hover_byte_offset: Option<usize>,
    stale_combined_popup: bool,
) -> (bool, bool, bool) {
    let diagnostic_needs_type = is_error_hovered && hovered_diag_type_target.is_some();
    let type_matches_diag = hovered_diag_type_target == type_popup_byte;
    let hover_matches_diag = hovered_diag_type_target == hover_byte_offset;
    let type_matches_hover = type_popup_byte == hover_byte_offset;

    compute_hover_visibility_from_matches(
        is_error_hovered,
        error_timer_ready,
        has_type_popup,
        diagnostic_needs_type,
        type_matches_diag,
        hover_matches_diag,
        type_matches_hover,
        stale_combined_popup,
    )
}

#[allow(dead_code)]
pub fn compute_hover_visibility_from_matches(
    is_error_hovered: bool,
    error_timer_ready: bool,
    has_type_popup: bool,
    diagnostic_needs_type: bool,
    type_matches_diag: bool,
    hover_matches_diag: bool,
    type_matches_hover: bool,
    stale_combined_popup: bool,
) -> (bool, bool, bool) {
    let show_stale_combined =
        stale_combined_popup && diagnostic_needs_type && has_type_popup && type_matches_diag;
    let show_combined = (diagnostic_needs_type
        && hover_matches_diag
        && has_type_popup
        && type_matches_diag
        && error_timer_ready)
        || show_stale_combined;

    let mut show_error = if diagnostic_needs_type {
        if hover_matches_diag {
            show_combined
        } else if show_stale_combined {
            true
        } else {
            false
        }
    } else {
        is_error_hovered && error_timer_ready
    };

    let show_type = if diagnostic_needs_type {
        if hover_matches_diag {
            has_type_popup && type_matches_diag && error_timer_ready
        } else if show_stale_combined {
            true
        } else {
            has_type_popup && type_matches_hover
        }
    } else {
        has_type_popup && type_matches_hover
    };

    // Строгое правило 1 окна: если всплывают два независимых попапа,
    // скрываем ошибку в угоду детальной информации (типу) того слова, на которое наведен курсор.
    if show_error && show_type && !show_combined {
        show_error = false;
    }

    (show_error, show_type, show_combined)
}

#[cfg(test)]
pub(crate) fn diagnostic_hover_target_byte_on_line(
    editor: &crate::editor::Editor,
    line: usize,
    start_col: u32,
    end_col: u32,
) -> Option<usize> {
    diagnostic_hover_byte_range_on_line(editor, line, start_col, end_col)
        .map(|(_, _, type_target)| type_target)
}

pub fn clear_hover_popup(_renderer: Option<&mut crate::renderer::Renderer>) -> bool {
    HOVER_STATE.with(|state| {
        let mut state = state.borrow_mut();
        let had_popup = state.popup.is_some()
            || state.request_id.is_some()
            || state.definition_request_id.is_some()
            || state.byte_offset.is_some()
            || state.rect.is_some()
            || state.diag_rect.is_some();
        state.request_id = None;
        state.definition_request_id = None;
        state.popup = None;
        state.pending_popup = None;
        state.timer = 0.0;
        state.byte_offset = None;
        state.rect = None;
        state.max_scroll = 0.0;
        state.selection_anchor = None;
        state.selection_cursor = None;
        state.selecting = false;
        state.diag_selection_anchor = None;
        state.diag_selection_cursor = None;
        state.diag_selecting = false;
        state.reset_diagnostic_popup();
        had_popup
    })
}

pub(crate) fn is_hover_target_byte(editor: &crate::editor::Editor, byte_offset: usize) -> bool {
    if byte_offset >= editor.len() {
        return false;
    }
    let b = editor.byte_at(byte_offset);
    matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_') || b >= 0x80
}

pub(crate) fn is_python_hover_keyword(token: &str) -> bool {
    matches!(
        token,
        "False"
            | "None"
            | "True"
            | "and"
            | "as"
            | "assert"
            | "async"
            | "await"
            | "break"
            | "class"
            | "continue"
            | "def"
            | "del"
            | "elif"
            | "else"
            | "except"
            | "finally"
            | "for"
            | "from"
            | "global"
            | "if"
            | "import"
            | "in"
            | "is"
            | "lambda"
            | "nonlocal"
            | "not"
            | "or"
            | "pass"
            | "raise"
            | "return"
            | "try"
            | "while"
            | "with"
            | "yield"
    )
}

pub(crate) fn hover_token_text(
    editor: &crate::editor::Editor,
    byte_offset: usize,
) -> Option<String> {
    let (start, end) = hover_token_bounds(editor, byte_offset);
    let text = editor.get_full_text();
    text.get(start..end).map(|s| s.to_string())
}

fn is_escaped_quote(bytes: &[u8], quote_pos: usize) -> bool {
    let mut slash_count = 0;
    let mut pos = quote_pos;
    while pos > 0 && bytes[pos - 1] == b'\\' {
        slash_count += 1;
        pos -= 1;
    }
    slash_count % 2 == 1
}

pub(crate) fn diagnostic_hover_byte_range_on_line(
    editor: &crate::editor::Editor,
    line: usize,
    start_col: u32,
    end_col: u32,
) -> Option<(usize, usize, usize)> {
    if line >= editor.line_offsets.len() {
        return None;
    }

    let line_start = editor.line_offsets[line];
    let line_end = editor
        .line_offsets
        .get(line + 1)
        .copied()
        .unwrap_or(editor.len());

    let mut start_byte = line_start;
    let mut end_byte = line_end;
    let mut start_byte_found = false;
    let mut end_byte_found = false;

    editor.utf16_col_to_byte_advance(line, |_ch, utf16_before, pos| {
        if !start_byte_found && utf16_before >= start_col {
            start_byte = pos;
            start_byte_found = true;
        }
        if !end_byte_found && utf16_before >= end_col {
            end_byte = pos;
            end_byte_found = true;
        }
    });

    if !start_byte_found {
        start_byte = line_start;
    }
    if !end_byte_found {
        end_byte = line_end;
    }

    let scan_start = start_byte.min(line_end);
    let scan_end = end_byte.max(scan_start).min(line_end);
    let mut target_raw_byte = None;
    let mut target_byte = None;

    for byte in scan_start..scan_end {
        if let Some(normalized) = normalize_hover_byte(editor, byte) {
            target_raw_byte = Some(byte);
            target_byte = Some(normalized);
            break;
        }
    }

    let target_raw_byte = target_raw_byte?;
    let target_byte = target_byte?;

    let mut range_start = target_byte;
    let mut range_end = target_byte.saturating_add(1).min(line_end);

    while range_start > line_start && is_hover_target_byte(editor, range_start - 1) {
        range_start -= 1;
    }
    while range_end < line_end && is_hover_target_byte(editor, range_end) {
        range_end += 1;
    }

    let mut quote_start = None;
    let mut scan = target_raw_byte.min(line_end.saturating_sub(1));
    loop {
        let b = editor.byte_at(scan);
        if b == b'\'' || b == b'"' {
            quote_start = Some(scan);
            break;
        }
        if scan == line_start {
            break;
        }
        scan -= 1;
    }

    if quote_start.is_none() {
        let mut forward_scan = target_raw_byte;
        while forward_scan < target_raw_byte + 4 && forward_scan < line_end {
            let b = editor.byte_at(forward_scan);
            if b == b'\'' || b == b'"' {
                quote_start = Some(forward_scan);
                break;
            }
            forward_scan += 1;
        }
    }

    if let Some(qs) = quote_start {
        let quote = editor.byte_at(qs);
        let mut quote_end = None;
        let mut qe = qs + 1;
        while qe < line_end {
            if editor.byte_at(qe) == quote {
                quote_end = Some(qe);
                break;
            }
            qe += 1;
        }

        if let Some(qe) = quote_end {
            let mut prefix_start = qs;
            while prefix_start > line_start {
                let b = editor.byte_at(prefix_start - 1);
                if matches!(b, b'f' | b'F' | b'r' | b'R' | b'u' | b'U' | b'b' | b'B') {
                    prefix_start -= 1;
                } else {
                    break;
                }
            }

            if target_raw_byte >= prefix_start && target_raw_byte < qe {
                range_start = prefix_start;
                range_end = (qe + 1).min(line_end);
            }
        }
    }

    Some((range_start, range_end, target_byte))
}

#[cfg(test)]
pub(crate) fn diagnostic_hover_range_on_line(
    editor: &crate::editor::Editor,
    line: usize,
    start_col: u32,
    end_col: u32,
) -> Option<(u32, u32, usize)> {
    let (range_start, range_end, target_byte) =
        diagnostic_hover_byte_range_on_line(editor, line, start_col, end_col)?;

    let mut out_start_col = start_col;
    let mut out_end_col = end_col;
    let mut out_start_found = false;
    let mut out_end_found = false;

    editor.utf16_col_to_byte_advance(line, |_ch, utf16_before, pos| {
        if !out_start_found && pos >= range_start {
            out_start_col = utf16_before;
            out_start_found = true;
        }
        if !out_end_found && pos >= range_end {
            out_end_col = utf16_before;
            out_end_found = true;
        }
    });

    if !out_start_found {
        out_start_col = start_col;
    }
    if !out_end_found {
        out_end_col = end_col.max(out_start_col + 1);
    }

    Some((out_start_col, out_end_col, target_byte))
}

pub(crate) fn normalize_hover_byte(
    editor: &crate::editor::Editor,
    byte_offset: usize,
) -> Option<usize> {
    let normalized = is_hover_target_byte(editor, byte_offset).then_some(byte_offset)?;

    if hover_token_text(editor, normalized)
        .as_deref()
        .is_some_and(is_python_hover_keyword)
    {
        return None;
    }

    Some(normalized)
}

pub(crate) fn hover_token_bounds(
    editor: &crate::editor::Editor,
    byte_offset: usize,
) -> (usize, usize) {
    let text = editor.get_full_text();
    if !text.is_empty() {
        let bytes = text.as_bytes();
        let idx = byte_offset.min(bytes.len().saturating_sub(1));
        let line_start = bytes[..=idx]
            .iter()
            .rposition(|&b| b == b'\n')
            .map(|pos| pos + 1)
            .unwrap_or(0);
        let line_end = bytes[idx..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|pos| idx + pos)
            .unwrap_or(bytes.len());

        let mut quote_pos = None;
        if matches!(bytes[idx], b'\'' | b'"') && !is_escaped_quote(bytes, idx) {
            quote_pos = Some(idx);
        } else {
            let mut pos = idx;
            while pos > line_start {
                pos -= 1;
                if matches!(bytes[pos], b'\'' | b'"') && !is_escaped_quote(bytes, pos) {
                    quote_pos = Some(pos);
                    break;
                }
            }
            if quote_pos.is_none()
                && idx + 1 < line_end
                && matches!(bytes[idx + 1], b'\'' | b'"')
                && !is_escaped_quote(bytes, idx + 1)
            {
                quote_pos = Some(idx + 1);
            }
        }

        if let Some(quote_start) = quote_pos {
            let quote = bytes[quote_start];
            let mut quote_end = quote_start + 1;
            while quote_end < line_end {
                if bytes[quote_end] == quote && !is_escaped_quote(bytes, quote_end) {
                    break;
                }
                quote_end += 1;
            }

            if quote_end < line_end {
                let mut prefix_start = quote_start;
                while prefix_start > line_start {
                    let prev = bytes[prefix_start - 1];
                    if matches!(prev, b'f' | b'F' | b'r' | b'R' | b'b' | b'B' | b'u' | b'U') {
                        prefix_start -= 1;
                    } else {
                        break;
                    }
                }

                if idx >= prefix_start && idx <= quote_end {
                    return (prefix_start, (quote_end + 1).min(line_end));
                }
            }
        }
    }

    let len = editor.len();
    let mut start = byte_offset.min(len);
    while start > 0 && is_hover_target_byte(editor, start - 1) {
        start -= 1;
    }

    let mut end = byte_offset.saturating_add(1).min(len);
    while end < len && is_hover_target_byte(editor, end) {
        end += 1;
    }

    (start, end)
}

pub(crate) fn hover_bytes_share_token(
    editor: &crate::editor::Editor,
    first: Option<usize>,
    second: Option<usize>,
) -> bool {
    match (first, second) {
        (Some(first), Some(second)) => {
            hover_token_bounds(editor, first) == hover_token_bounds(editor, second)
        }
        _ => false,
    }
}

pub fn hover_anchor_for_byte(
    renderer: &mut crate::renderer::Renderer,
    editor: &crate::editor::Editor,
    byte_offset: usize,
    render_scroll_y: f32,
) -> (f32, f32) {
    let (start, end) = hover_token_bounds(editor, byte_offset);
    let text = editor.get_full_text();
    if text.is_empty() {
        return (renderer.last_mouse_x, renderer.last_mouse_y);
    }

    let phys_line = editor
        .line_offsets
        .partition_point(|&o| o <= start)
        .saturating_sub(1);
    let line_start = editor.line_offsets.get(phys_line).copied().unwrap_or(0);

    let mut token_start = start.min(text.len());
    while token_start > line_start && !text.is_char_boundary(token_start) {
        token_start -= 1;
    }

    let mut token_end = end.min(text.len());
    while token_end < text.len() && !text.is_char_boundary(token_end) {
        token_end += 1;
    }

        if token_start > token_end || line_start > token_start {
        return (renderer.last_mouse_x, renderer.last_mouse_y);
    }

    let target_w = compute_hover_x_offset(
        &text,
        line_start,
        token_start,
        token_end,
        byte_offset,
        |c| renderer.char_advance(c),
    );

    let vis_line_idx = renderer
        .phys_to_visual
        .get(phys_line)
        .copied()
        .unwrap_or(phys_line) as f32;
    let x = renderer.left_padding - renderer.last_scroll_x + target_w;
    let y = (vis_line_idx * renderer.line_height) - render_scroll_y + renderer.line_height * 0.5;

    (x, y)
}

pub(crate) fn compute_hover_x_offset<F>(
    text: &str,
    line_start: usize,
    token_start: usize,
    token_end: usize,
    byte_offset: usize,
    mut char_advance: F,
) -> f32
where
    F: FnMut(char) -> f32,
{
    let mut target_byte = byte_offset.clamp(token_start, token_end);
    while target_byte > token_start && !text.is_char_boundary(target_byte) {
        target_byte -= 1;
    }

    let mut target_w = 0.0;
    if let Some(prefix) = text.get(line_start..target_byte) {
        for c in prefix.chars() {
            if c != '\n' && c != '\u{FE0F}' && c != '\u{200D}' {
                target_w += char_advance(c);
            }
        }
    }
    target_w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_hover_x_offset_uses_exact_byte_for_anchor() {
        let text = "    add_default_users_and_roles_session";
        let line_start = 0;
        let token_start = 4;
        let token_end = text.len();

        let advance = |_c: char| 10.0;

        // Hovering over 'd' in "default" (index 8)
        let w1 = compute_hover_x_offset(text, line_start, token_start, token_end, 8, advance);
        assert_eq!(w1, 80.0);

        // Hovering over 's' in "session" (index 28)
        let w2 = compute_hover_x_offset(text, line_start, token_start, token_end, 28, advance);
        assert_eq!(w2, 280.0);

        // Hovering exactly at token start
        let w3 = compute_hover_x_offset(text, line_start, token_start, token_end, 4, advance);
        assert_eq!(w3, 40.0);
    }
}

pub(crate) fn hover_popup_byte_at(
    renderer: &mut crate::renderer::Renderer,
    popup: &HoverPopup,
    rect: (f32, f32, f32, f32),
    x: f32,
    y: f32,
) -> usize {
    let s = renderer.scale_factor;
    let pad = 12.0 * s;
    let line_h = 22.0 * s;
    let max_text_w = (renderer.width - 80.0 * s).min(820.0 * s).max(320.0 * s);
    let (bx, by, _bw, _bh) = rect;

    let computed_layout;
    let layout = if let Some(cache) = popup.layout_cache.as_ref().filter(|cache| {
        cache.scale_factor == renderer.scale_factor
            && cache.max_text_w == max_text_w
            && cache.span_count == popup.spans.len()
            && cache.text_len == popup.text.len()
    }) {
        cache
    } else {
        computed_layout = renderer.build_hover_popup_layout(popup, max_text_w, line_h);
        &computed_layout
    };
    let lines = &layout.lines;

    if lines.is_empty() {
        return 0;
    }

    let mut current_top = by + pad - popup.scroll.current;
    let mut found_line_idx = lines.len().saturating_sub(1);

    for (i, line) in lines.iter().enumerate() {
        let scale_mul = match line.kind {
            crate::lsp::HoverLineKindPublic::Header1 => 1.15,
            crate::lsp::HoverLineKindPublic::Header2 => 1.05,
            _ => 1.0,
        };
        let cur_line_h = line_h * scale_mul;

        if y >= current_top && y < current_top + cur_line_h {
            found_line_idx = i;
            break;
        }
        current_top += cur_line_h;
    }

    let found_visual_line = &lines[found_line_idx];
    let found_line = &found_visual_line.glyphs;
    let found_kind = found_visual_line.kind;
    let found_scale = match found_kind {
        crate::lsp::HoverLineKindPublic::Header1 => 1.15,
        crate::lsp::HoverLineKindPublic::Header2 => 1.05,
        _ => 1.0,
    };

    if found_line.is_empty() {
        if found_line_idx > 0 {
            if let Some(prev_line) = lines.get(found_line_idx - 1) {
                if let Some(&(prev_ch, _, prev_off)) = prev_line.glyphs.last() {
                    return prev_off + prev_ch.len_utf8();
                }
            }
        }
        for next_idx in (found_line_idx + 1)..lines.len() {
            if let Some(&(_next_ch, _, next_off)) = lines[next_idx].glyphs.first() {
                return next_off;
            }
        }
        return 0;
    }

    let is_code = found_kind == crate::lsp::HoverLineKindPublic::Code;
    let is_module_header = found_kind == crate::lsp::HoverLineKindPublic::Text
        && found_line.len() >= 11
        && found_line
            .iter()
            .take(11)
            .map(|&(c, _, _)| c)
            .collect::<String>()
            == "[[MODULE]] ";
    let is_header = matches!(
        found_kind,
        crate::lsp::HoverLineKindPublic::Header1 | crate::lsp::HoverLineKindPublic::Header2
    );

    let mut start_x = if is_code {
        bx + pad + 8.0 * s
    } else {
        bx + pad
    };
    let mut glyph_start = 0;

    if is_module_header {
        let icon_size = 18.0 * s;
        start_x = bx + pad + icon_size + 4.0 * s;
        glyph_start = 11;
    }

    let target_x = (x - start_x).max(0.0);
    let mut draw_x = 0.0;

    for i in glyph_start..found_line.len() {
        let (ch, _, off) = found_line[i];
        let adv = if is_header {
            renderer.get_ui_glyph(ch).map(|g| g.advance).unwrap_or(10.0) * found_scale
        } else {
            renderer.char_advance(ch) * found_scale
        };
        if target_x <= draw_x + adv * 0.5 {
            return off;
        }
        draw_x += adv;
    }
    let (last_ch, _, last_off) = found_line[found_line.len() - 1];
    last_off + last_ch.len_utf8()
}
