use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn diagnostic_hover_byte_at<'a>(
    editor: &crate::editor::Editor,
    renderer: &mut crate::renderer::Renderer,
    diagnostics: impl IntoIterator<Item = &'a crate::lsp::Diagnostic>,
    cursor_phys_line: usize,
    px: f32,
    hover_content_y: f32,
    render_scroll_x: f32,
    left_padding: f32,
    line_h: f32,
) -> Option<usize> {
    let last_line = editor.line_offsets.len().saturating_sub(1);
    for diag in diagnostics {
        if crate::render_view::should_suppress_active_line_useless_expression(
            diag,
            cursor_phys_line,
        ) {
            continue;
        }
        let start_line = (diag.start_line as usize).min(last_line);
        let end_line = (diag.end_line as usize).min(last_line);
        for line in start_line..=end_line {
            let vis_line_idx = renderer
                .phys_to_visual
                .get(line)
                .copied()
                .map_or(0.0, |value| value as f32);
            let line_top_y = vis_line_idx * line_h;
            if !hover_content_y_in_line_hitbox(hover_content_y, line_top_y, line_h) {
                continue;
            }
            let start_col = if line == diag.start_line as usize {
                diag.start_col
            } else {
                0
            };
            let end_col = if line == diag.end_line as usize {
                diag.end_col
            } else {
                u32::MAX
            };
            let avg_adv = renderer.char_advance('a');
            let Some((start_byte, end_byte)) =
                diagnostic_visual_byte_range_on_line(editor, line, start_col, end_col)
            else {
                continue;
            };
            let line_start = editor
                .line_offsets
                .get(line)
                .copied()
                .map_or(0, |value| value);
            let x_start_px =
                renderer.visual_x_for_byte_offset(editor, line_start, start_byte, true);
            let mut x_end_px =
                renderer.visual_x_for_byte_offset(editor, line_start, end_byte, false);
            if line != diag.end_line as usize {
                x_end_px = x_end_px.max(x_start_px + avg_adv * 4.0);
            }
            let line_x = px - left_padding + render_scroll_x;
            let hit_visual_text = renderer.visual_text_range_contains_x(
                editor,
                line_start,
                start_byte,
                end_byte,
                line_x,
                avg_adv / 2.0,
            );
            let logical_line_x = renderer.text_x_for_visual_line_x(editor, line, line_x);
            let type_target = diagnostic_hover_byte_range_on_line(editor, line, start_col, end_col)
                .map_or(start_byte, |range| range.2);
            let x_start = left_padding + x_start_px - render_scroll_x;
            let x_end = left_padding + x_end_px - render_scroll_x;
            let squiggle_w = (x_end - x_start).max(avg_adv / 2.0);
            if px < x_start || px > x_start + squiggle_w || !hit_visual_text {
                continue;
            }
            return diagnostic_hover_type_target_at_x(
                editor,
                line,
                logical_line_x,
                Some(type_target),
                |ch| renderer.char_advance(ch),
            );
        }
    }
    None
}

pub(super) fn autocomplete_drag_target(
    py: f32,
    rect_y: f32,
    rect_h: f32,
    drag_offset: f32,
    total_items: usize,
    scale: f32,
) -> f32 {
    let step = 36.0 * scale;
    let total_items = total_items as f32;
    let visible_items = total_items.min(7.0);

    let track_margin = autocomplete_scrollbar_track_margin(scale);
    let track_h = (rect_h - track_margin * 2.0).max(1.0);
    let total_h = total_items * step;
    let thumb_h = (rect_h / total_h * track_h)
        .max(20.0 * scale)
        .min(track_h.max(0.0));
    let max_scroll = ((total_items - visible_items) * step).max(0.0);

    let ratio = (py - rect_y - track_margin - drag_offset) / (track_h - thumb_h).max(1.0);
    (ratio * max_scroll).clamp(0.0, max_scroll)
}

fn autocomplete_scrollbar_track_margin(scale: f32) -> f32 {
    3.0 * scale
}

pub(super) fn autocomplete_hovered_index(
    px: f32,
    py: f32,
    rect: (f32, f32, f32, f32),
    current_scroll: f32,
    total_items: usize,
    scale: f32,
) -> Option<usize> {
    let (rx, ry, rw, rh) = rect;
    if px < rx || px > rx + rw || py < ry || py > ry + rh {
        return None;
    }

    let scroll_x = rx + rw - 14.0 * scale;
    if px >= scroll_x {
        return None;
    }
    let item_h = 36.0 * scale;
    let content_y = py - ry + current_scroll;
    if content_y < 0.0 {
        return None;
    }

    let idx = (content_y / item_h) as usize;
    (idx < total_items).then_some(idx)
}

pub(super) fn resized_left_width(px: f32, window_width: f32, scale: f32) -> f32 {
    let sb_w = 48.0 * scale;
    let max_w = ((window_width - sb_w) / scale) - 300.0;
    ((px - sb_w) / scale).max(80.0).min(max_w.max(80.0))
}

pub(super) fn resized_bottom_height(py: f32, window_height: f32, scale: f32) -> f32 {
    let status_bar_h = crate::render_view::ide_status_bar_height(scale);
    let available_h = (window_height - status_bar_h).max(0.0);
    let max_h = (available_h / scale) - 50.0;
    ((available_h - py) / scale).max(60.0).min(max_h.max(60.0))
}

pub(super) fn cursor_position_allows_editor_hover(
    px: f32,
    py: f32,
    window_width: f32,
    window_height: f32,
) -> bool {
    px >= 0.0 && py >= 0.0 && px < window_width && py < window_height
}

pub(super) fn should_suppress_editor_hover_for_scroll_drag(
    scroll_y_dragging: bool,
    scroll_x_dragging: bool,
    markdown_read: bool,
) -> bool {
    markdown_read || scroll_y_dragging || scroll_x_dragging
}

pub(super) fn inline_git_popup_blocks_hover(id: Option<crate::ui_system::UiId>) -> bool {
    matches!(
        id,
        Some(
            crate::ui_system::UiId::InlineGitPanelBody
                | crate::ui_system::UiId::InlineGitPrevHunk
                | crate::ui_system::UiId::InlineGitNextHunk
                | crate::ui_system::UiId::InlineGitRollbackHunk
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autocomplete_drag_target_clamps_to_available_scroll() {
        assert_eq!(autocomplete_scrollbar_track_margin(1.0), 3.0);
        assert_eq!(autocomplete_drag_target(0.0, 10.0, 200.0, 0.0, 4, 1.0), 0.0);

        let mid = autocomplete_drag_target(100.0, 10.0, 200.0, 0.0, 12, 1.0);
        let max = autocomplete_drag_target(999.0, 10.0, 200.0, 0.0, 12, 1.0);

        assert!(mid > 0.0);
        assert_eq!(max, (12.0 - 7.0) * 36.0);
    }

    #[test]
    fn autocomplete_hovered_index_ignores_scrollbar_and_outside_rect() {
        let rect = (10.0, 20.0, 200.0, 160.0);

        assert_eq!(
            autocomplete_hovered_index(20.0, 30.0, rect, 0.0, 10, 1.0),
            Some(0)
        );
        assert_eq!(
            autocomplete_hovered_index(20.0, 70.0, rect, 0.0, 10, 1.0),
            Some(1)
        );
        assert_eq!(
            autocomplete_hovered_index(20.0, 30.0, rect, 72.0, 10, 1.0),
            Some(2)
        );

        assert_eq!(
            autocomplete_hovered_index(999.0, 30.0, rect, 0.0, 10, 1.0),
            None
        );
        assert_eq!(
            autocomplete_hovered_index(205.0, 30.0, rect, 0.0, 10, 1.0),
            None
        );
        assert_eq!(
            autocomplete_hovered_index(20.0, 500.0, rect, 0.0, 10, 1.0),
            None
        );
        assert_eq!(
            autocomplete_hovered_index(20.0, 30.0, rect, 0.0, 0, 1.0),
            None
        );
    }

    #[test]
    fn resize_helpers_clamp_left_width_and_bottom_height() {
        assert_eq!(resized_left_width(0.0, 1200.0, 1.0), 80.0);
        assert_eq!(resized_left_width(248.0, 1200.0, 1.0), 200.0);
        assert_eq!(resized_left_width(5000.0, 1200.0, 1.0), 852.0);

        assert_eq!(resized_bottom_height(2000.0, 900.0, 1.0), 60.0);
        assert_eq!(resized_bottom_height(690.0, 900.0, 1.0), 180.0);
        assert_eq!(resized_bottom_height(600.0, 900.0, 1.0), 270.0);
        assert_eq!(resized_bottom_height(0.0, 900.0, 1.0), 820.0);
    }

    #[test]
    fn cursor_hover_requires_pointer_inside_window() {
        assert!(cursor_position_allows_editor_hover(0.0, 0.0, 800.0, 600.0));
        assert!(cursor_position_allows_editor_hover(
            799.0, 599.0, 800.0, 600.0
        ));
        assert!(!cursor_position_allows_editor_hover(
            -1.0, 10.0, 800.0, 600.0
        ));
        assert!(!cursor_position_allows_editor_hover(
            10.0, -1.0, 800.0, 600.0
        ));
        assert!(!cursor_position_allows_editor_hover(
            800.0, 10.0, 800.0, 600.0
        ));
        assert!(!cursor_position_allows_editor_hover(
            10.0, 600.0, 800.0, 600.0
        ));
    }

    #[test]
    fn editor_scrollbar_drag_suppresses_hover_only_while_dragging() {
        assert!(!should_suppress_editor_hover_for_scroll_drag(
            false, false, false
        ));
        assert!(should_suppress_editor_hover_for_scroll_drag(
            true, false, false
        ));
        assert!(should_suppress_editor_hover_for_scroll_drag(
            false, true, false
        ));
        assert!(should_suppress_editor_hover_for_scroll_drag(
            true, true, false
        ));
    }

    #[test]
    fn markdown_reader_never_arms_hidden_editor_hover() {
        assert!(should_suppress_editor_hover_for_scroll_drag(
            false, false, true
        ));
    }
}
