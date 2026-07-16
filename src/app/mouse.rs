use crate::app::{App, AutocompleteMode};
use std::time::Instant;
use winit::event::{ElementState, MouseScrollDelta};
use winit::event_loop::ActiveEventLoop;

fn git_graph_rows_bounds(
    ide_panel: &crate::app::IdePanelState,
    window_h: f32,
    scale: f32,
) -> Option<(f32, f32)> {
    if !ide_panel.git.graph_open {
        return None;
    }
    let title_h = 32.0 * scale;
    let controls_h = crate::app::git_panel::GIT_GRAPH_CONTROLS_H * scale;
    let panel_bottom_h = if ide_panel.any_bottom_open() {
        ide_panel.bottom_height * scale
    } else {
        0.0
    };
    let content_bottom = crate::render_view::ide_bottom_panel_y(window_h, panel_bottom_h, scale);
    let content_h = (content_bottom - title_h).max(0.0);
    let full_list_h = (content_h - controls_h).max(40.0 * scale);
    let (list_h, divider_h, graph_h) = crate::app::git_panel::git_graph_split_heights(
        full_list_h,
        ide_panel.git.graph_height_ratio,
        scale,
    );
    let rows_y = title_h + controls_h + list_h + divider_h + 34.0 * scale;
    Some((rows_y, (graph_h - 34.0 * scale).max(0.0)))
}

fn active_terminal_scrollbar_layout(
    app: &App,
) -> Option<crate::render_view::terminal_ui::TerminalScrollbarLayout> {
    if !app.ide_panel.is_open(crate::app::PanelId::Terminal) {
        return None;
    }
    let renderer = app.renderer.as_ref()?;
    let terminal = app
        .ide_panel
        .terminals
        .get(app.ide_panel.active_terminal)?;
    let grid = terminal.grid.lock().ok()?;
    if grid.is_alt {
        return None;
    }
    let total_lines = grid.scrollback.len() + grid.lines.len();
    drop(grid);

    let scale = renderer.scale_factor;
    let panel_x = 48.0 * scale;
    let panel_w = (renderer.width - panel_x).max(0.0);
    let bottom_h = app.ide_panel.bottom_height * scale;
    let panel_y = crate::render_view::ide_bottom_panel_y(renderer.height, bottom_h, scale);
    let panel_tab_h = 32.0 * scale;
    let content_y = panel_y + 1.0 + panel_tab_h;
    let content_h = bottom_h - 1.0 - panel_tab_h;
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
    diagnostic_visual_byte_range_on_line,
    embedded_editor_hover_byte_at_point, hover_bytes_share_token, hover_content_y_in_line_hitbox,
    hover_screen_y_to_content_y, hover_token_bounds, move_type_hover_to_empty_space,
    normalize_hover_byte, update_editor_hover_state_for_cursor,
    with_embedded_editor_hover_renderer_context,
};
pub use hover_state_core::{
    HoverLayoutCache, HoverPopup, HoverState, HoverVisualLine, HoveredDiagnostic,
};
#[cfg(test)]
pub use hover_state_core::{hover_source_line_y_band, is_in_hover_popup_or_bridge};
