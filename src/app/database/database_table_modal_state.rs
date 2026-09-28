impl crate::app::database::DatabasePanelState {
    pub(crate) fn stop_table_modal_scroll_anims(&mut self) {
        if let Some((scroll_x, scroll_y)) = self.table_modal.as_mut().and_then(table_modal_scrolls_mut) {
            if !scroll_x.is_dragging { scroll_x.stop_anim(); }
            if !scroll_y.is_dragging { scroll_y.stop_anim(); }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct DatabaseTextModalScrollSnapshot {
    pub(crate) current_x: f32,
    pub(crate) current_y: f32,
    pub(crate) dragging_x: bool,
    pub(crate) dragging_y: bool,
    pub(crate) line_count: usize,
    pub(crate) max_line_width: f32,
}

pub(crate) fn text_modal_scroll_snapshot(
    modal: &crate::app::database::DatabaseTableModal,
    layout_cache: &std::cell::RefCell<crate::app::database::DatabaseMultilineLayoutCache>,
    scale: f32,
    renderer: Option<&mut crate::renderer::Renderer>,
) -> Option<DatabaseTextModalScrollSnapshot> {
    use crate::app::database::DatabaseTableModal;
    let (text, scroll_x, scroll_y) = match modal {
        DatabaseTableModal::SqlPreview { text, scroll_x, scroll_y, .. } => (text.as_str(), scroll_x, scroll_y),
        DatabaseTableModal::MultilineEditor { input, scroll_x, scroll_y, .. } => (input.text(), scroll_x, scroll_y),
        _ => return None,
    };
    let mut layout_cache = layout_cache.borrow_mut();
    if let Some(renderer) = renderer {
        layout_cache.ensure(text, scale, true, |line| line.chars().map(|ch| renderer.char_advance(ch)).sum());
    } else {
        let fallback_advance = (9.0 * scale).round().max(1.0);
        layout_cache.ensure(text, scale, false, |line| line.chars().count() as f32 * fallback_advance);
    }
    Some(DatabaseTextModalScrollSnapshot {
        current_x: scroll_x.current,
        current_y: scroll_y.current,
        dragging_x: scroll_x.is_dragging,
        dragging_y: scroll_y.is_dragging,
        line_count: layout_cache.line_count(),
        max_line_width: layout_cache.max_line_width(),
    })
}

pub(crate) fn sql_preview_scroll_metrics(
    line_count: usize,
    max_line_width: f32,
    input_rect: Option<(f32, f32, f32, f32)>,
    horizontal_rect: Option<(f32, f32, f32, f32)>,
    vertical_rect: Option<(f32, f32, f32, f32)>,
    scale: f32,
) -> (f32, f32, f32, f32) {
    let viewport_w = horizontal_rect.map(|rect| rect.2).or_else(|| input_rect.map(|rect| rect.2)).unwrap_or(1.0).max(1.0);
    let viewport_h = vertical_rect.map(|rect| rect.3).or_else(|| input_rect.map(|rect| rect.3)).unwrap_or(1.0).max(1.0);
    let line_h = (crate::app::database::DATABASE_SQL_PREVIEW_LINE_HEIGHT * scale).round().max(1.0);
    let content_h = line_count as f32 * line_h;
    let content_w = max_line_width + (18.0 * scale).round();
    (viewport_w, viewport_h, (content_w - viewport_w).max(0.0), (content_h - viewport_h).max(0.0))
}

pub(crate) fn modal_input_mut(
    modal: &mut Option<crate::app::database::DatabaseTableModal>,
) -> Option<&mut crate::app::database::DatabaseDialogInput> {
    use crate::app::database::DatabaseTableModal;
    match modal.as_mut()? {
        DatabaseTableModal::CustomLimit { input, .. } | DatabaseTableModal::MultilineEditor { input, .. } => Some(input),
        _ => None,
    }
}

pub(crate) fn table_modal_scrolls_mut(
    modal: &mut crate::app::database::DatabaseTableModal,
) -> Option<(&mut crate::scroll::ScrollState, &mut crate::scroll::ScrollState)> {
    use crate::app::database::DatabaseTableModal;
    match modal {
        DatabaseTableModal::SqlPreview { scroll_x, scroll_y, .. }
        | DatabaseTableModal::MultilineEditor { scroll_x, scroll_y, .. } => Some((scroll_x, scroll_y)),
        _ => None,
    }
}

pub(crate) fn sql_preview_copy_text(
    modal: &crate::app::database::DatabaseTableModal,
) -> Option<String> {
    use crate::app::database::DatabaseTableModal;
    let DatabaseTableModal::SqlPreview { text, cursor, selection_anchor, .. } = modal else { return None; };
    let Some(anchor) = selection_anchor else { return Some(text.clone()); };
    let start = (*anchor).min(*cursor);
    let end = (*anchor).max(*cursor);
    if start == end { return Some(text.clone()); }
    text.get(start..end).map(str::to_owned)
}

pub(crate) fn database_text_modal_scrolls_mut(
    modal: &mut crate::app::database::DatabaseTableModal,
) -> Option<(&mut crate::scroll::ScrollState, &mut crate::scroll::ScrollState)> {
    table_modal_scrolls_mut(modal)
}

pub(crate) fn database_sql_preview_copy_text(
    modal: &crate::app::database::DatabaseTableModal,
) -> Option<String> {
    sql_preview_copy_text(modal)
}

pub(crate) fn database_sql_preview_scroll_metrics(
    line_count: usize,
    max_line_width: f32,
    input_rect: Option<(f32, f32, f32, f32)>,
    horizontal_rect: Option<(f32, f32, f32, f32)>,
    vertical_rect: Option<(f32, f32, f32, f32)>,
    scale: f32,
) -> (f32, f32, f32, f32) {
    sql_preview_scroll_metrics(line_count, max_line_width, input_rect, horizontal_rect, vertical_rect, scale)
}

pub(crate) fn database_table_modal_input_mut(
    modal: &mut Option<crate::app::database::DatabaseTableModal>,
) -> Option<&mut crate::app::database::DatabaseDialogInput> {
    modal_input_mut(modal)
}

pub(crate) fn move_read_only_cursor(
    cursor: &mut usize,
    selection_anchor: &mut Option<usize>,
    target: usize,
    selecting: bool,
) {
    let old_cursor = *cursor;
    if selecting {
        if selection_anchor.is_none() {
            *selection_anchor = Some(old_cursor);
        }
    } else {
        *selection_anchor = None;
    }
    *cursor = target;
}

pub(crate) fn scroll_text_modal(
    modal: &mut Option<crate::app::database::DatabaseTableModal>,
    dx: f32,
    dy: f32,
    shift: bool,
    max_x: f32,
    max_y: f32,
) -> bool {
    let Some((scroll_x, scroll_y)) = modal.as_mut().and_then(table_modal_scrolls_mut) else { return false; };
    if shift {
        scroll_x.anim_speed = 7.0;
        scroll_x.scroll_by(dy);
        scroll_x.clamp_target(0.0, max_x);
    } else {
        scroll_y.anim_speed = 7.0;
        scroll_y.scroll_by(dy);
        scroll_y.clamp_target(0.0, max_y);
        scroll_x.anim_speed = 7.0;
        scroll_x.scroll_by(dx);
        scroll_x.clamp_target(0.0, max_x);
    }
    true
}

#[cfg(test)]
mod database_table_modal_state_tests {
    use super::*;

    fn sql_preview(text: &str, cursor: usize, anchor: Option<usize>) -> crate::app::database::DatabaseTableModal {
        crate::app::database::DatabaseTableModal::SqlPreview {
            tab_id: crate::app::database::DatabaseTabId(1), text: text.to_string(), cursor,
            selection_anchor: anchor, spans: Vec::new(),
            scroll_x: crate::scroll::ScrollState::new(15.0),
            scroll_y: crate::scroll::ScrollState::new(15.0),
        }
    }

    #[test]
    fn sql_preview_copy_prefers_the_selected_unicode_range() {
        let text = "SELECT 'Ж';";
        let start = text.find('Ж').unwrap();
        let end = start + 'Ж'.len_utf8();
        let modal = sql_preview(text, end, Some(start));
        assert_eq!(database_sql_preview_copy_text(&modal).as_deref(), Some("Ж"));
    }

    #[test]
    fn sql_preview_copy_without_selection_returns_the_full_query() {
        let modal = sql_preview("SELECT 1;", 4, None);
        assert_eq!(database_sql_preview_copy_text(&modal).as_deref(), Some("SELECT 1;"));
    }

    #[test]
    fn multiline_scroll_metrics_count_the_trailing_empty_line() {
        let (_, _, _, max_y) = database_sql_preview_scroll_metrics(2, 9.0, Some((0.0, 0.0, 100.0, 26.0)), None, None, 1.0);
        assert_eq!(max_y, 26.0);
    }

    #[test]
    fn database_modal_scrollbar_drag_is_target_only_for_both_axes() {
        for (track_start, track_len, viewport, max_scroll, pointer_delta, horizontal) in [
            (20.0, 240.0, 180.0, 420.0, 28.0, false),
            (40.0, 360.0, 280.0, 760.0, 44.0, true),
        ] {
            let current = max_scroll * 0.35;
            let lane = if horizontal {
                (track_start, 0.0, track_len, 0.0)
            } else {
                (0.0, track_start, 0.0, track_len)
            };
            let bar = crate::render_view::database_table_tab_overlay::database_table_modal_scrollbar(
                lane, viewport, viewport + max_scroll, current, horizontal,
            );
            let geometry = bar.geometry(1.0).expect("modal geometry");
            let pointer = geometry.thumb.start + 6.0;
            let (offset, _) = geometry.press_target(pointer).expect("modal drag starts");
            let target = geometry.drag_target(pointer + pointer_delta, offset).expect("modal drag moves");
            let mut scroll = crate::scroll::ScrollState::new(7.0);
            scroll.jump_to(current);
            assert!(crate::app::mouse::apply_scrollbar_drag_target(&mut scroll, target, offset));
            assert_eq!(scroll.current, current);
            assert!(scroll.target > current);
        }
    }
}
