use crate::app::{App, AutocompleteMode};
use std::time::Instant;
use winit::event::{ElementState, MouseScrollDelta};
use winit::event_loop::ActiveEventLoop;

fn panel_scroll_rect(
    is_top: bool,
    scale: f32,
    sidebar_w: f32,
    left_width: f32,
    bottom_height: f32,
    window_width: f32,
    window_height: f32,
) -> (f32, f32, f32, f32) {
    let title_h = 32.0 * scale;
    if is_top {
        (
            sidebar_w,
            title_h,
            left_width * scale,
            window_height
                - title_h
                - bottom_height
                - crate::render_view::ide_status_bar_height(scale),
        )
    } else {
        let tab_h = 32.0 * scale;
        let panel_y = crate::render_view::ide_bottom_panel_y(window_height, bottom_height, scale);
        (
            sidebar_w,
            panel_y + 1.0 + tab_h,
            window_width - sidebar_w,
            bottom_height - 1.0 - tab_h,
        )
    }
}

pub(crate) fn app_panel_scroll_rect(
    app: &App,
    panel_id: crate::app::PanelId,
    scale: f32,
) -> (f32, f32, f32, f32, f32) {
    let renderer = app
        .renderer
        .as_ref()
        .expect("panel input requires renderer");
    let window_w = renderer.width;
    let window_h = renderer.height;
    let sidebar_w = 48.0 * scale;
    let panel_bottom_h = if app.ide_panel.any_bottom_open() {
        app.ide_panel.bottom_height * scale
    } else {
        0.0
    };
    let is_top = app
        .ide_panel
        .slots
        .iter()
        .any(|slot| slot.id == panel_id && slot.group == crate::app::PanelGroup::Top);
    let (cx, cy, cw, ch) = panel_scroll_rect(
        is_top,
        scale,
        sidebar_w,
        app.ide_panel.left_width,
        panel_bottom_h,
        window_w,
        window_h,
    );
    (cx, cy, cw, ch, window_h)
}

pub(crate) fn begin_scrollbar_drag(
    scroll: &mut crate::scroll::ScrollState,
    pointer: f32,
    track_start: f32,
    track_len: f32,
    max_scroll: f32,
    min_thumb_len: f32,
) -> bool {
    let Some(thumb) = crate::scroll::scrollbar_thumb(
        track_start,
        track_len,
        track_len,
        track_len + max_scroll,
        scroll.current,
        min_thumb_len,
    ) else {
        return false;
    };
    let Some((drag_offset, target)) = crate::scroll::scrollbar_drag_target(
        pointer,
        track_start,
        track_len,
        thumb,
        max_scroll,
        None,
    ) else {
        return false;
    };
    scroll.jump_to(target);
    scroll.drag_offset = drag_offset;
    scroll.is_dragging = true;
    true
}

pub(crate) fn update_scrollbar_drag(
    scroll: &mut crate::scroll::ScrollState,
    pointer: f32,
    track_start: f32,
    track_len: f32,
    max_scroll: f32,
    min_thumb_len: f32,
) -> bool {
    if !scroll.is_dragging {
        return false;
    }
    let Some(thumb) = crate::scroll::scrollbar_thumb(
        track_start,
        track_len,
        track_len,
        track_len + max_scroll,
        scroll.current,
        min_thumb_len,
    ) else {
        scroll.end_drag();
        return false;
    };
    let drag_offset = scroll.drag_offset;
    let Some((_, target)) = crate::scroll::scrollbar_drag_target(
        pointer,
        track_start,
        track_len,
        thumb,
        max_scroll,
        Some(drag_offset),
    ) else {
        scroll.end_drag();
        return false;
    };
    scroll.jump_to(target);
    scroll.drag_offset = drag_offset;
    scroll.is_dragging = true;
    true
}

fn explorer_scrollbar_layout(
    app: &App,
    scale: f32,
) -> Option<crate::app::file_tree::FileTreeScrollbarLayout> {
    if !app.ide_panel.is_open(crate::app::PanelId::Explorer) {
        return None;
    }
    let (panel_x, panel_y, panel_w, panel_h, _) =
        app_panel_scroll_rect(app, crate::app::PanelId::Explorer, scale);
    crate::app::file_tree::file_tree_scrollbar_layout(
        panel_x,
        panel_y,
        panel_w,
        panel_h,
        scale,
        app.ide_panel.file_tree_nodes.len(),
        app.ide_panel.explorer_scroll.current,
    )
}

#[derive(Clone, Copy, Debug)]
struct ProblemsScrollbarLayout {
    content_x: f32,
    content_y: f32,
    content_w: f32,
    content_h: f32,
    list_y: f32,
    track_h: f32,
    total_h: f32,
}

fn problems_scrollbar_layout(app: &App, scale: f32) -> Option<ProblemsScrollbarLayout> {
    if !app.ide_panel.is_open(crate::app::PanelId::Problems) {
        return None;
    }
    let (content_x, content_y, content_w, content_h, _) =
        app_panel_scroll_rect(app, crate::app::PanelId::Problems, scale);
    let list_y = content_y + 40.0 * scale;
    let track_h = (content_h - 40.0 * scale).max(0.0);
    let total_h = crate::app::problems_scroll_content_height(
        app.ide_panel.visible_problem_row_count(app.lsp.as_ref()),
        24.0 * scale,
    );
    Some(ProblemsScrollbarLayout {
        content_x,
        content_y,
        content_w,
        content_h,
        list_y,
        track_h,
        total_h,
    })
}

fn git_graph_rows_bounds(app: &App, scale: f32) -> Option<(f32, f32)> {
    if !app.ide_panel.git.graph_open {
        return None;
    }
    let (_, content_y, _, content_h, _) =
        app_panel_scroll_rect(app, crate::app::PanelId::Git, scale);
    let controls_h = crate::app::git_panel::GIT_GRAPH_CONTROLS_H * scale;
    let full_list_h = (content_h - controls_h).max(40.0 * scale);
    let (list_h, divider_h, graph_h) = crate::app::git_panel::git_graph_split_heights(
        full_list_h,
        app.ide_panel.git.graph_height_ratio,
        scale,
    );
    let rows_y = content_y + controls_h + list_h + divider_h + 34.0 * scale;
    Some((rows_y, (graph_h - 34.0 * scale).max(0.0)))
}

fn active_terminal_scrollbar_layout(
    app: &App,
) -> Option<crate::render_view::terminal_ui::TerminalScrollbarLayout> {
    if !app.ide_panel.is_open(crate::app::PanelId::Terminal) {
        return None;
    }
    let renderer = app.renderer.as_ref()?;
    let terminal = app.ide_panel.terminals.get(app.ide_panel.active_terminal)?;
    let grid = crate::platform::lock_recover(&terminal.grid);
    if grid.is_alt {
        return None;
    }
    let total_lines = grid.scrollback.len() + grid.lines.len();
    drop(grid);

    let scale = renderer.scale_factor;
    let (panel_x, content_y, panel_w, content_h, _) =
        app_panel_scroll_rect(app, crate::app::PanelId::Terminal, scale);
    let (term_y, term_h) =
        crate::render_view::terminal_ui::terminal_body_rect(content_y, content_h, scale);
    let char_h = renderer.line_height * crate::render_view::terminal_ui::TERMINAL_TEXT_SCALE;
    crate::render_view::terminal_ui::terminal_scrollbar_layout(
        panel_x,
        panel_w,
        term_y,
        term_h,
        scale,
        char_h,
        total_lines,
        terminal.scroll_y.current,
    )
}

mod cursor;
mod hover_mouse_logic;
#[cfg(test)]
mod hover_mouse_tests;
mod hover_state_core;
mod input;
mod wheel;

#[cfg(test)]
pub(crate) use hover_mouse_logic::embedded_editor_hover_content_y_at_point;
pub(crate) use hover_mouse_logic::hover_popup_byte_at;
pub use hover_mouse_logic::{
    HOVER_REQUEST_DELAY_SEC, HOVER_STATE, advance_hover_anim_progress, clear_hover_popup,
    compute_hover_visibility_from_matches, hover_anchor_for_byte,
    suppress_hover_popup_until_mouse_move,
};
#[cfg(test)]
pub(super) use hover_mouse_logic::{
    compute_hover_visibility, diagnostic_hover_range_on_line, diagnostic_hover_target_byte_on_line,
    hover_byte_on_line_at_x, hover_token_text, is_hover_target_byte, is_python_hover_keyword,
    type_hover_screen_y_matches_byte_line,
};
pub(crate) use hover_mouse_logic::{
    diagnostic_hover_byte_range_on_line, diagnostic_hover_type_target_at_x,
    diagnostic_visual_byte_range_on_line, embedded_editor_hover_byte_at_point,
    hover_bytes_share_token, hover_content_y_in_line_hitbox, hover_screen_y_to_content_y,
    hover_token_bounds, move_type_hover_to_empty_space, normalize_hover_byte,
    update_editor_hover_state_for_cursor, with_embedded_editor_hover_renderer_context,
};
pub use hover_state_core::{
    HoverLayoutCache, HoverPopup, HoverState, HoverVisualLine, HoveredDiagnostic,
};
pub(crate) use hover_state_core::{hover_popup_scrollbar_drag_target, hover_popup_scrollbar_thumb};
#[cfg(test)]
pub use hover_state_core::{hover_source_line_y_band, is_in_hover_popup_or_bridge};

#[cfg(test)]
mod panel_geometry_tests {
    use super::{begin_scrollbar_drag, panel_scroll_rect, update_scrollbar_drag};

    #[test]
    fn shared_scrollbar_drag_preserves_pointer_offset_across_moves() {
        let mut scroll = crate::scroll::ScrollState::new(15.0);
        scroll.jump_to(100.0);
        assert!(begin_scrollbar_drag(
            &mut scroll,
            75.0,
            0.0,
            200.0,
            300.0,
            20.0
        ));
        let offset = scroll.drag_offset;
        assert!(offset > 0.0);
        assert!(update_scrollbar_drag(
            &mut scroll,
            95.0,
            0.0,
            200.0,
            300.0,
            20.0
        ));
        assert_eq!(scroll.drag_offset, offset);
        assert!(scroll.is_dragging);
    }

    #[test]
    fn top_panel_input_stops_above_every_visible_bottom_panel() {
        let (_, y, _, h) = panel_scroll_rect(true, 1.0, 48.0, 240.0, 180.0, 1200.0, 900.0);
        assert_eq!(y, 32.0);
        assert_eq!(h, 658.0);
    }
}
