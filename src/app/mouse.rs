use crate::app::{App, AutocompleteMode};
use std::io::Write;
use std::time::Instant;
use winit::event::{ElementState, MouseScrollDelta};
use winit::event_loop::ActiveEventLoop;

mod cursor;
mod hover_mouse_logic;
#[cfg(test)]
mod hover_mouse_tests;
mod hover_state_core;
mod input;
mod wheel;

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
};
pub(crate) use hover_mouse_logic::{
    diagnostic_hover_byte_range_on_line, diagnostic_hover_type_target_at_x,
    hover_bytes_share_token, hover_token_bounds, normalize_hover_byte,
};
pub use hover_state_core::{
    HoverLayoutCache, HoverPopup, HoverState, HoverVisualLine, HoveredDiagnostic,
    hover_source_line_y_band, is_in_hover_popup_or_bridge,
};
