use crate::ui_system::UiId;
use std::time::Instant;

pub const CONTEXT_MENU_ANIM_SECS: f32 = 0.28;
const CONTEXT_MENU_CURSOR_OFFSET: f32 = 10.0;

pub(crate) fn context_menu_anchor(mx: f32, my: f32, scale: f32) -> (f32, f32) {
    let offset = CONTEXT_MENU_CURSOR_OFFSET * scale;
    (mx + offset, my + offset)
}

pub(crate) fn context_menu_anim_progress(opened_at: Instant, now: Instant) -> f32 {
    let elapsed = now
        .checked_duration_since(opened_at)
        .unwrap_or_default()
        .as_secs_f32();
    let progress = (elapsed / CONTEXT_MENU_ANIM_SECS).clamp(0.0, 1.0);
    progress * progress * progress * (progress * (progress * 6.0 - 15.0) + 10.0)
}

pub(crate) fn context_menu_cursor(hovered_overlay: Option<UiId>) -> winit::window::CursorIcon {
    if matches!(
        hovered_overlay,
        Some(UiId::FileTreeMenuItem(_) | UiId::DatabaseContextItem(_))
    ) {
        winit::window::CursorIcon::Pointer
    } else {
        winit::window::CursorIcon::Default
    }
}
