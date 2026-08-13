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

pub(crate) fn context_menu_visible_height(menu_h: f32, min_h: f32, anim_progress: f32) -> f32 {
    let min_px = min_h.round();
    let full_px = menu_h.round().max(min_px);
    if anim_progress <= 0.0 {
        min_px
    } else {
        (min_px + (full_px - min_px) * anim_progress)
            .ceil()
            .min(full_px)
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn context_menu_visible_height_has_no_initial_border_only_dead_zone() {
        let min_h = 4.0;
        assert_eq!(context_menu_visible_height(32.0, min_h, 0.0), 4.0);
        assert!(context_menu_visible_height(32.0, min_h, 0.001) >= 5.0);

        for menu_h in [32.0, 60.0, 88.0] {
            let visible_h = context_menu_visible_height(menu_h, min_h, 0.04);
            assert!(
                visible_h >= 5.0,
                "menu_h={menu_h} did not advance one visible pixel"
            );
        }

        assert_eq!(context_menu_visible_height(32.0, min_h, 1.0), 32.0);
    }

    #[test]
    fn context_menu_visible_height_first_refresh_frames_advance_one_pixel() {
        let start = Instant::now();
        for frame in [Duration::from_micros(8_333), Duration::from_micros(16_667)] {
            let progress = context_menu_anim_progress(start, start + frame);
            assert!(progress > 0.0);
            assert!(
                context_menu_visible_height(32.0, 4.0, progress) >= 5.0,
                "frame={frame:?}, progress={progress} stayed at border-only height"
            );
        }
    }

    #[test]
    fn context_menu_visible_height_is_monotonic_bounded_and_ends_at_rounded_target() {
        let min_h: f32 = 4.0;
        let menu_h: f32 = 35.2;
        let full_px = menu_h.round();
        let progresses = [0.0, 0.001, 0.01, 0.04, 0.1, 0.5, 0.9, 0.999, 1.0];
        let mut previous = min_h.round();

        for progress in progresses {
            let visible_h = context_menu_visible_height(menu_h, min_h, progress);
            assert!(visible_h >= min_h.round());
            assert!(visible_h <= full_px);
            assert!(visible_h >= previous);
            previous = visible_h;
        }

        assert_eq!(context_menu_visible_height(menu_h, min_h, 0.999), full_px);
        assert_eq!(context_menu_visible_height(menu_h, min_h, 1.0), full_px);
        assert_eq!(full_px, 35.0);
    }
}
