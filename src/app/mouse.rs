use crate::app::App;
use std::io::Write;
use std::time::Instant;
use winit::event::{ElementState, MouseScrollDelta};
use winit::event_loop::ActiveEventLoop;

mod cursor;
mod input;
mod wheel;
#[derive(Debug, Clone)]
pub struct HoverPopup {
    pub text: String,
    pub spans: Vec<crate::highlighter::ColorSpan>,
    pub line_kinds: Vec<crate::lsp::HoverLineKindPublic>,
    pub inline_code_ranges: Vec<(usize, usize)>,
    pub byte_offset: usize,
    pub anchor_x: f32,
    pub anchor_y: f32,
    pub scroll: crate::scroll::ScrollState,
    pub layout_cache: Option<HoverLayoutCache>,
}

#[derive(Debug, Clone)]
pub struct HoverVisualLine {
    pub glyphs: Vec<(char, [f32; 4], usize)>,
    pub kind: crate::lsp::HoverLineKindPublic,
}

#[derive(Debug, Clone)]
pub struct HoverLayoutCache {
    pub scale_factor: f32,
    pub max_text_w: f32,
    pub span_count: usize,
    pub text_len: usize,
    pub lines: Vec<HoverVisualLine>,
    pub max_line_w: f32,
    pub total_text_h: f32,
}

pub struct HoverState {
    pub request_id: Option<i32>,
    pub definition_request_id: Option<i32>,
    pub popup: Option<HoverPopup>,
    pub pending_popup: Option<HoverPopup>,
    pub timer: f32,
    pub byte_offset: Option<usize>,
    pub rect: Option<(f32, f32, f32, f32)>,
    pub max_scroll: f32,
    pub selection_anchor: Option<usize>,
    pub selection_cursor: Option<usize>,
    pub selecting: bool,
    pub diag_selection_anchor: Option<usize>,
    pub diag_selection_cursor: Option<usize>,
    pub diag_selecting: bool,
}

impl Default for HoverState {
    fn default() -> Self {
        Self {
            request_id: None,
            definition_request_id: None,
            popup: None,
            pending_popup: None,
            timer: 0.0,
            byte_offset: None,
            rect: None,
            max_scroll: 0.0,
            selection_anchor: None,
            selection_cursor: None,
            selecting: false,
            diag_selection_anchor: None,
            diag_selection_cursor: None,
            diag_selecting: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{is_in_hover_popup_or_bridge, normalize_hover_byte};

    #[test]
    fn hover_byte_includes_identifier_edges_next_to_whitespace() {
        let mut editor = crate::editor::Editor::new(128);
        editor.insert_str("    handlers\n");
        let text = editor.get_full_text();
        let handlers = text.find("handlers").unwrap();
        let after_handlers = handlers + "handlers".len();

        assert_eq!(normalize_hover_byte(&editor, handlers), Some(handlers));
        assert_eq!(
            normalize_hover_byte(&editor, after_handlers),
            Some(after_handlers - 1)
        );
        assert_eq!(normalize_hover_byte(&editor, handlers - 1), Some(handlers));
    }

    #[test]
    fn hover_byte_ignores_python_keywords_so_diagnostics_can_show() {
        let mut editor = crate::editor::Editor::new(128);
        editor.insert_str("    else:\n        raise ValueError()\n");
        let text = editor.get_full_text();
        let else_offset = text.find("else").unwrap();

        assert_eq!(normalize_hover_byte(&editor, else_offset), None);
        assert_eq!(normalize_hover_byte(&editor, else_offset + 2), None);
    }

    #[test]
    fn hover_bridge_reaches_full_source_line_when_popup_is_above() {
        let popup_rect = (220.0, 100.0, 500.0, 180.0);
        let line_top_y = 288.0;
        let line_bottom_y = 316.0;

        assert!(is_in_hover_popup_or_bridge(
            450.0,
            305.0,
            popup_rect,
            460.0,
            305.0,
            line_top_y,
            line_bottom_y,
            1000.0,
            1.0,
        ));
    }

    #[test]
    fn hover_bridge_handles_popup_shifted_away_from_anchor() {
        let popup_rect = (20.0, 100.0, 520.0, 180.0);
        let line_top_y = 288.0;
        let line_bottom_y = 316.0;

        assert!(is_in_hover_popup_or_bridge(
            520.0,
            247.0,
            popup_rect,
            760.0,
            305.0,
            line_top_y,
            line_bottom_y,
            800.0,
            1.0,
        ));
    }

    #[test]
    fn hover_bridge_keeps_popup_when_moving_up_from_token_anchor() {
        let popup_rect = (96.0, 80.0, 760.0, 210.0);
        let line_top_y = 340.0;
        let line_bottom_y = 368.0;

        assert!(is_in_hover_popup_or_bridge(
            620.0,
            318.0,
            popup_rect,
            620.0,
            354.0,
            line_top_y,
            line_bottom_y,
            1000.0,
            1.0,
        ));
    }

    #[test]
    fn hover_bridge_does_not_capture_next_line_when_popup_is_above() {
        let popup_rect = (96.0, 80.0, 760.0, 210.0);
        let line_top_y = 340.0;
        let line_bottom_y = 368.0;

        assert!(!is_in_hover_popup_or_bridge(
            620.0,
            394.0,
            popup_rect,
            620.0,
            354.0,
            line_top_y,
            line_bottom_y,
            1000.0,
            1.0,
        ));
    }

    #[test]
    fn hover_bridge_keeps_popup_when_cursor_moves_slightly_sideways() {
        let popup_rect = (96.0, 80.0, 760.0, 210.0);
        let line_top_y = 340.0;
        let line_bottom_y = 368.0;

        assert!(is_in_hover_popup_or_bridge(
            676.0,
            300.0,
            popup_rect,
            620.0,
            354.0,
            line_top_y,
            line_bottom_y,
            1000.0,
            1.0,
        ));
    }
}

thread_local! {
    pub static HOVER_STATE: std::cell::RefCell<HoverState> = std::cell::RefCell::new(HoverState::default());
}

pub const HOVER_REQUEST_DELAY_SEC: f32 = 0.34;

pub fn clear_hover_popup(renderer: Option<&mut crate::renderer::Renderer>) -> bool {
    HOVER_STATE.with(|state| {
        let mut state = state.borrow_mut();
        let had_popup = state.popup.is_some()
            || state.request_id.is_some()
            || state.definition_request_id.is_some()
            || state.byte_offset.is_some()
            || state.rect.is_some();
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
        if let Some(r) = renderer {
            r.last_diag_popup_rect = None;
            r.last_hovered_diags.clear();
            r.hovered_diags_cache.clear();
            r.diag_hover_timer = 0.0;
            r.diag_hover_timer_idx = None;
        }
        had_popup
    })
}

fn is_hover_target_byte(editor: &crate::editor::Editor, byte_offset: usize) -> bool {
    if byte_offset >= editor.len() {
        return false;
    }
    let b = editor.byte_at(byte_offset);
    matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_') || b >= 0x80
}

fn is_python_hover_keyword(token: &str) -> bool {
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

fn hover_token_text(editor: &crate::editor::Editor, byte_offset: usize) -> Option<String> {
    let (start, end) = hover_token_bounds(editor, byte_offset);
    let text = editor.get_full_text();
    text.get(start..end + 1).map(|s| s.to_string())
}

fn normalize_hover_byte(editor: &crate::editor::Editor, byte_offset: usize) -> Option<usize> {
    let normalized = if is_hover_target_byte(editor, byte_offset) {
        Some(byte_offset)
    } else if byte_offset > 0 && is_hover_target_byte(editor, byte_offset - 1) {
        Some(byte_offset - 1)
    } else if byte_offset + 1 < editor.len() && is_hover_target_byte(editor, byte_offset + 1) {
        Some(byte_offset + 1)
    } else {
        None
    }?;

    if hover_token_text(editor, normalized)
        .as_deref()
        .is_some_and(is_python_hover_keyword)
    {
        return None;
    }

    Some(normalized)
}

fn hover_token_bounds(editor: &crate::editor::Editor, byte_offset: usize) -> (usize, usize) {
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
        if matches!(bytes[idx], b'\'' | b'"') {
            quote_pos = Some(idx);
        } else {
            let mut pos = idx;
            while pos > line_start {
                pos -= 1;
                if matches!(bytes[pos], b'\'' | b'"') {
                    quote_pos = Some(pos);
                    break;
                }
            }
            if quote_pos.is_none() && idx + 1 < line_end && matches!(bytes[idx + 1], b'\'' | b'"') {
                quote_pos = Some(idx + 1);
            }
        }

        if let Some(quote_start) = quote_pos {
            let quote = bytes[quote_start];
            let mut quote_end = quote_start + 1;
            while quote_end < line_end {
                if bytes[quote_end] == quote
                    && bytes.get(quote_end.saturating_sub(1)) != Some(&b'\\')
                {
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
                    return (prefix_start, quote_end);
                }
            }
        }
    }

    let mut start = byte_offset;
    while start > 0 && is_hover_target_byte(editor, start - 1) {
        start -= 1;
    }

    let mut end = byte_offset;
    while end + 1 < editor.len() && is_hover_target_byte(editor, end + 1) {
        end += 1;
    }

    (start, end)
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

    let mut token_end = end.saturating_add(1).min(text.len());
    while token_end < text.len() && !text.is_char_boundary(token_end) {
        token_end += 1;
    }

    if token_start > token_end || line_start > token_start {
        return (renderer.last_mouse_x, renderer.last_mouse_y);
    }

    let mut prefix_w = 0.0;
    if let Some(prefix) = text.get(line_start..token_start) {
        for c in prefix.chars() {
            if c != '\n' && c != '\u{FE0F}' && c != '\u{200D}' {
                prefix_w += renderer.char_advance(c);
            }
        }
    }

    let mut token_w = 0.0;
    if let Some(token) = text.get(token_start..token_end) {
        for c in token.chars() {
            if c != '\n' && c != '\u{FE0F}' && c != '\u{200D}' {
                token_w += renderer.char_advance(c);
            }
        }
    }

    let vis_line_idx = renderer
        .phys_to_visual
        .get(phys_line)
        .copied()
        .unwrap_or(phys_line) as f32;
    let x = renderer.left_padding - renderer.last_scroll_x + prefix_w + token_w * 0.5;
    let y = (vis_line_idx * renderer.line_height) - render_scroll_y + renderer.line_height * 0.5;

    (x, y)
}

pub fn is_in_hover_popup_or_bridge(
    px: f32,
    py: f32,
    popup_rect: (f32, f32, f32, f32),
    anchor_x: f32,
    anchor_y: f32,
    line_top_y: f32,
    line_bottom_y: f32,
    _viewport_w: f32,
    scale: f32,
) -> bool {
    let (rx, ry, rw, rh) = popup_rect;
    if px >= rx && px <= rx + rw && py >= ry && py <= ry + rh {
        return true;
    }

    let bridge_radius = 72.0 * scale;
    let bridge_margin = 16.0 * scale;

    if ry + rh <= line_top_y {
        if py > line_bottom_y + bridge_margin {
            return false;
        }
    } else if ry >= line_bottom_y && py < line_top_y - bridge_margin {
        return false;
    }

    let target_x = anchor_x.clamp(rx, rx + rw);
    let target_y = anchor_y.clamp(ry, ry + rh);

    let dx = target_x - anchor_x;
    let dy = target_y - anchor_y;
    let len_sq = dx * dx + dy * dy;

    let t = if len_sq == 0.0 {
        0.0
    } else {
        ((px - anchor_x) * dx + (py - anchor_y) * dy) / len_sq
    };

    let t = t.clamp(0.0, 1.0);
    let proj_x = anchor_x + t * dx;
    let proj_y = anchor_y + t * dy;

    let dist_sq = (px - proj_x) * (px - proj_x) + (py - proj_y) * (py - proj_y);

    if dist_sq <= bridge_radius * bridge_radius {
        return true;
    }

    let on_line_x = (px - anchor_x).abs() < bridge_radius;
    let on_line_y = if ry + rh <= line_top_y {
        py >= ry + rh - bridge_radius * 0.5 && py <= line_bottom_y + bridge_margin
    } else if ry >= line_bottom_y {
        py >= line_top_y - bridge_margin && py <= ry + bridge_radius * 0.5
    } else {
        py >= line_top_y - bridge_radius * 0.5 && py <= line_bottom_y + bridge_radius * 0.5
    };

    on_line_x && on_line_y
}

fn hover_popup_byte_at(
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

    let mut lines: Vec<(Vec<(char, usize)>, crate::lsp::HoverLineKindPublic)> = Vec::new();
    let mut cur_line_w = 0.0;
    let mut cur_line: Vec<(char, usize)> = Vec::new();
    let mut last_space_idx = None;
    let mut raw_line_no = 0usize;

    let push_line = |lines: &mut Vec<_>,
                     cur_line: Vec<(char, usize)>,
                     kind: crate::lsp::HoverLineKindPublic| {
        lines.push((cur_line, kind));
    };

    for (offset, c) in popup.text.char_indices() {
        let kind = popup
            .line_kinds
            .get(raw_line_no)
            .copied()
            .unwrap_or(crate::lsp::HoverLineKindPublic::Text);
        let scale_mul = match kind {
            crate::lsp::HoverLineKindPublic::Header1 => 1.15,
            crate::lsp::HoverLineKindPublic::Header2 => 1.05,
            _ => 1.0,
        };

        if c == '\n' {
            push_line(&mut lines, std::mem::take(&mut cur_line), kind);
            cur_line_w = 0.0;
            last_space_idx = None;
            raw_line_no += 1;
            continue;
        }

        let adv = renderer.char_advance(c) * scale_mul;
        if cur_line_w + adv > max_text_w && cur_line_w > 40.0 * s {
            if let Some(space_pos) = last_space_idx {
                let mut remainder = cur_line.split_off(space_pos);
                if !remainder.is_empty() && remainder[0].0 == ' ' {
                    remainder.remove(0);
                }
                push_line(&mut lines, std::mem::take(&mut cur_line), kind);
                cur_line = remainder;
                cur_line_w = cur_line
                    .iter()
                    .map(|&(ch, _)| renderer.char_advance(ch) * scale_mul)
                    .sum();
            } else {
                push_line(&mut lines, std::mem::take(&mut cur_line), kind);
                cur_line_w = 0.0;
            }
            last_space_idx = None;
        }

        cur_line.push((c, offset));
        cur_line_w += adv;

        if c == ' ' {
            last_space_idx = Some(cur_line.len() - 1);
        }
    }
    if !cur_line.is_empty() {
        let kind = popup
            .line_kinds
            .get(raw_line_no)
            .copied()
            .unwrap_or(crate::lsp::HoverLineKindPublic::Text);
        push_line(&mut lines, cur_line, kind);
    }

    while let Some((line, _)) = lines.last() {
        if line.is_empty() {
            lines.pop();
        } else {
            break;
        }
    }

    if lines.is_empty() {
        return 0;
    }

    let mut current_top = by + pad - popup.scroll.current;
    let mut found_line_idx = lines.len().saturating_sub(1);

    for (i, (_line, kind)) in lines.iter().enumerate() {
        let scale_mul = match kind {
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

    let (found_line, found_kind) = &lines[found_line_idx];
    let found_scale = match found_kind {
        crate::lsp::HoverLineKindPublic::Header1 => 1.15,
        crate::lsp::HoverLineKindPublic::Header2 => 1.05,
        _ => 1.0,
    };

    if found_line.is_empty() {
        if found_line_idx > 0 {
            if let Some((prev_line, _)) = lines.get(found_line_idx - 1) {
                if let Some(&(prev_ch, prev_off)) = prev_line.last() {
                    return prev_off + prev_ch.len_utf8();
                }
            }
        }
        for next_idx in (found_line_idx + 1)..lines.len() {
            if let Some(&(_next_ch, next_off)) = lines[next_idx].0.first() {
                return next_off;
            }
        }
        return 0;
    }

    let is_code = *found_kind == crate::lsp::HoverLineKindPublic::Code;
    let is_module_header = *found_kind == crate::lsp::HoverLineKindPublic::Text
        && found_line.len() >= 11
        && found_line
            .iter()
            .take(11)
            .map(|&(c, _)| c)
            .collect::<String>()
            == "[[MODULE]] ";
    let is_header = matches!(
        *found_kind,
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
        let (ch, off) = found_line[i];
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
    let (last_ch, last_off) = found_line[found_line.len() - 1];
    last_off + last_ch.len_utf8()
}
