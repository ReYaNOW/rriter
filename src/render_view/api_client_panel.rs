use crate::app::api_client::{
    ApiFocus, ApiMethod, ApiSpecSource, api_mock_guide_max_scroll,
    api_mock_server_log_max_scroll, api_route_matches_filter, api_timing_visible_at,
    format_api_secs, format_last_loaded_at, now_epoch_secs,
};
use crate::app::api_mock::types::{ApiMockMode, ApiMockServerStatus, ApiPythonRuntimeMode};
use crate::render_view::tree_ui::{TREE_INDENT_W, TREE_ROW_H, TREE_TEXT_SCALE};
use crate::renderer::Renderer;
use crate::widgets::{ButtonView, IconButton, IconType};
use glow::HasContext;


include!("api_client_panel/api_client_panel_main_renderer.rs");
include!("api_client_panel/api_client_panel_overlay_renderer.rs");

fn api_panel_row_text_y(row_y: f32, row_h: f32, scale: f32) -> f32 {
    (row_y.round() + row_h.round() * 0.5 + (4.5 * scale).round()).round()
}

fn api_panel_label_width(right_edge: f32, text_x: f32) -> f32 {
    (right_edge - text_x).max(0.0)
}
