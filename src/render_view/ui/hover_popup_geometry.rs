pub fn compute_hover_y_position(
    line_top_y: f32,
    line_height: f32,
    box_h: f32,
    screen_height: f32,
    scale_factor: f32,
) -> f32 {
    let margin = 8.0 * scale_factor;
    let min_y = 40.0 * scale_factor;
    let max_y = screen_height - 10.0 * scale_factor;

    let mut target_by = line_top_y - box_h - margin;

    if target_by < min_y {
        let below_y = line_top_y + line_height + margin;
        if below_y + box_h <= max_y {
            target_by = below_y;
        } else {
            let space_above = line_top_y - min_y;
            let space_below = max_y - (line_top_y + line_height);
            if space_below > space_above {
                target_by = below_y;
            } else {
                target_by = min_y;
            }
        }
    }

    let max_top = (max_y - box_h).max(min_y);
    target_by.clamp(min_y, max_top)
}

#[cfg(test)]
pub fn compute_animated_scissor(
    mx: f32,
    my: f32,
    bx: f32,
    by: f32,
    box_w: f32,
    box_h: f32,
    anim_progress: f32,
) -> (f32, f32, f32, f32) {
    let target_x = bx - 4.0;
    let target_y = by - 4.0;
    let target_w = box_w + 8.0;
    let target_h = box_h + 8.0;

    compute_hover_popup_anim_rect(
        mx,
        my,
        target_x,
        target_y,
        target_w,
        target_h,
        anim_progress,
    )
}

fn smooth_hover_anim_progress(anim_progress: f32) -> f32 {
    let p = anim_progress.clamp(0.0, 1.0);
    p * p * (3.0 - 2.0 * p)
}

fn fade_hover_color(mut color: [f32; 4], alpha: f32) -> [f32; 4] {
    color[3] *= alpha;
    color
}

fn compute_hover_scrollbar_alpha(anim_progress: f32) -> f32 {
    let p = ((anim_progress.clamp(0.0, 1.0) - 0.88) / 0.12).clamp(0.0, 1.0);
    p * p * (3.0 - 2.0 * p)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct HoverSurfaceLayout {
    outer_rect: (f32, f32, f32, f32),
    inner_rect: (f32, f32, f32, f32),
    outer_radius: f32,
    inner_radius: f32,
    border_width: f32,
    clip_rect: (f32, f32, f32, f32),
}

fn hover_surface_layout(
    frame: (f32, f32, f32, f32),
    radius: f32,
    border_width: f32,
) -> HoverSurfaceLayout {
    let outer_rect = (
        frame.0.round(),
        frame.1.round(),
        frame.2.round().max(0.0),
        frame.3.round().max(0.0),
    );
    let max_inset = (outer_rect.2.min(outer_rect.3) * 0.5).max(0.0);
    let requested_border_width = border_width.round().max(0.0);
    let border_width = if requested_border_width > 0.0 && max_inset >= 1.0 {
        requested_border_width.min(max_inset)
    } else {
        0.0
    };
    let outer_radius = radius.round().max(0.0).min(max_inset);
    let inner_rect = (
        outer_rect.0 + border_width,
        outer_rect.1 + border_width,
        (outer_rect.2 - border_width * 2.0).max(0.0),
        (outer_rect.3 - border_width * 2.0).max(0.0),
    );
    let inner_max_radius = (inner_rect.2.min(inner_rect.3) * 0.5).max(0.0);
    let inner_radius = (outer_radius - border_width)
        .max(0.0)
        .min(inner_max_radius);

    HoverSurfaceLayout {
        outer_rect,
        inner_rect,
        outer_radius,
        inner_radius,
        border_width,
        clip_rect: inner_rect,
    }
}

#[derive(Clone, Copy)]
struct HoverPopupPop {
    frame: (f32, f32, f32, f32),
    content_scissor: (f32, f32, f32, f32),
    scrollbar_alpha: f32,
}

fn smooth_hover_width_progress(anim_progress: f32) -> f32 {
    smooth_hover_anim_progress((anim_progress / 0.94).clamp(0.0, 1.0))
}

fn smooth_hover_height_progress(anim_progress: f32) -> f32 {
    smooth_hover_anim_progress((anim_progress / 0.94).clamp(0.0, 1.0))
}

fn compute_hover_popup_anim_rect(
    mx: f32,
    my: f32,
    target_x: f32,
    target_y: f32,
    target_w: f32,
    target_h: f32,
    anim_progress: f32,
) -> (f32, f32, f32, f32) {
    let right = target_x + target_w;
    let bottom = target_y + target_h;
    let anchor_left = (mx - target_x).abs() <= (mx - right).abs();
    let anchor_top = if my <= target_y {
        true
    } else if my >= bottom {
        false
    } else {
        (my - target_y).abs() <= (my - bottom).abs()
    };
    let width_progress = smooth_hover_width_progress(anim_progress);
    let height_progress = smooth_hover_height_progress(anim_progress);
    let anim_w = target_w * width_progress;
    let anim_h = target_h * height_progress;
    let anim_x = if anchor_left {
        target_x
    } else {
        right - anim_w
    };
    let anim_y = if anchor_top {
        target_y
    } else {
        bottom - anim_h
    };

    (anim_x, anim_y, anim_w, anim_h)
}

fn pixel_stable_hover_popup_frame(
    frame: (f32, f32, f32, f32),
    target: (f32, f32, f32, f32),
    anchor: (f32, f32),
) -> (f32, f32, f32, f32) {
    let (frame_x, frame_y, frame_w, frame_h) = frame;
    let (target_x, target_y, target_w, target_h) = target;
    let (anchor_x, anchor_y) = anchor;
    let target_right = target_x + target_w;
    let target_bottom = target_y + target_h;
    let anchor_left = (anchor_x - target_x).abs() <= (anchor_x - target_right).abs();
    let anchor_top = if anchor_y <= target_y {
        true
    } else if anchor_y >= target_bottom {
        false
    } else {
        (anchor_y - target_y).abs() <= (anchor_y - target_bottom).abs()
    };

    let w = frame_w.round();
    let h = frame_h.round();
    let x = if anchor_left {
        frame_x.round()
    } else {
        target_right.round() - w
    };
    let y = if anchor_top {
        frame_y.round()
    } else {
        target_bottom.round() - h
    };

    (x, y, w, h)
}

fn stable_hover_animation_mouse(
    live_mx: f32,
    live_my: f32,
    anchor_x: f32,
    anchor_y: f32,
    anim_progress: f32,
) -> (f32, f32) {
    if anim_progress < 1.0 {
        (anchor_x, anchor_y)
    } else {
        (live_mx, live_my)
    }
}

fn hover_wrap_space_can_break(cur_line_len_after_space: usize) -> bool {
    cur_line_len_after_space != "[[MODULE]]".chars().count() + 1
}

pub fn compute_animated_popup_frame(
    mx: f32,
    my: f32,
    bx: f32,
    by: f32,
    box_w: f32,
    box_h: f32,
    anim_progress: f32,
) -> (f32, f32, f32, f32) {
    compute_hover_popup_anim_rect(mx, my, bx, by, box_w, box_h, anim_progress)
}

#[cfg(test)]
fn compute_combined_popup_frame(
    mx: f32,
    my: f32,
    bx: f32,
    by: f32,
    box_w: f32,
    box_h: f32,
    anim_progress: f32,
    _has_attached_hover: bool,
) -> (f32, f32, f32, f32) {
    compute_animated_popup_frame(mx, my, bx, by, box_w, box_h, anim_progress)
}

fn compute_combined_separator_visible_rect(
    bx: f32,
    sep_y: f32,
    box_w: f32,
    frame_x: f32,
    frame_y: f32,
    frame_w: f32,
    frame_h: f32,
) -> Option<(f32, f32)> {
    if sep_y < frame_y || sep_y > frame_y + frame_h {
        return None;
    }

    let x1 = bx.max(frame_x);
    let x2 = (bx + box_w).min(frame_x + frame_w);
    let w = x2 - x1;
    if w <= 0.0 { None } else { Some((x1, w)) }
}

fn compute_hover_scissor_rect(anim_rect: (f32, f32, f32, f32), x: f32, y: f32, w: f32, h: f32, min_y: Option<f32>, clip_rect: Option<(f32, f32, f32, f32)>) -> (f32, f32, f32, f32) {
    let (anim_x, anim_y, anim_w, anim_h) = anim_rect;
    let (clip_x, clip_y, clip_w, clip_h) = clip_rect.unwrap_or((f32::NEG_INFINITY, f32::NEG_INFINITY, f32::INFINITY, f32::INFINITY));
    let cx1 = x.max(anim_x).max(clip_x);
    let cy1 = y.max(anim_y).max(min_y.unwrap_or(f32::NEG_INFINITY)).max(clip_y);
    let cx2 = (x + w).min(anim_x + anim_w).min(clip_x + clip_w);
    let cy2 = (y + h).min(anim_y + anim_h).min(clip_y + clip_h);
    (cx1, cy1, (cx2 - cx1).max(0.0), (cy2 - cy1).max(0.0))
}

#[cfg(test)]
fn compute_hover_frame_content_rect(
    bx: f32,
    by: f32,
    box_w: f32,
    box_h: f32,
    border_px: f32,
) -> (f32, f32, f32, f32) {
    let inset = border_px.max(0.0);
    (
        bx + inset,
        by + inset,
        (box_w - inset * 2.0).max(0.0),
        (box_h - inset * 2.0).max(0.0),
    )
}

pub fn compute_hover_content_scissor(
    mx: f32,
    my: f32,
    bx: f32,
    by: f32,
    box_w: f32,
    box_h: f32,
    anim_progress: f32,
    attached_diag: Option<(f32, f32, f32, f32)>,
    attached_anim_progress: f32,
) -> (f32, f32, f32, f32) {
    if let Some((diag_x, diag_y, diag_w, diag_h)) = attached_diag {
        compute_animated_popup_frame(
            mx,
            my,
            diag_x,
            diag_y,
            box_w.max(diag_w),
            diag_h + box_h,
            attached_anim_progress,
        )
    } else {
        compute_animated_popup_frame(mx, my, bx, by, box_w, box_h, anim_progress)
    }
}

fn compute_hover_popup_pop(
    mx: f32,
    my: f32,
    bx: f32,
    by: f32,
    box_w: f32,
    box_h: f32,
    anim_progress: f32,
    attached_diag: Option<(f32, f32, f32, f32)>,
    attached_anim_progress: f32,
) -> HoverPopupPop {
    HoverPopupPop {
        frame: compute_animated_popup_frame(mx, my, bx, by, box_w, box_h, anim_progress),
        content_scissor: compute_hover_content_scissor(
            mx,
            my,
            bx,
            by,
            box_w,
            box_h,
            anim_progress,
            attached_diag,
            attached_anim_progress,
        ),
        scrollbar_alpha: compute_hover_scrollbar_alpha(anim_progress),
    }
}

pub fn compute_diagnostic_layout(
    first_line_y_top: f32,
    line_height: f32,
    box_w: f32,
    combined_h: f32,
    screen_width: f32,
    screen_height: f32,
    scale_factor: f32,
    first_diag_x: f32,
    popup_anchor_x: Option<f32>,
) -> (f32, f32) {
    let mut bx = first_diag_x;
    if let Some(ax) = popup_anchor_x {
        bx = bx.min(ax);
    }

    if bx + box_w > screen_width - 20.0 * scale_factor {
        bx = screen_width - box_w - 20.0 * scale_factor;
    }
    if bx < 20.0 * scale_factor {
        bx = 20.0 * scale_factor;
    }

    let by = compute_hover_y_position(
        first_line_y_top,
        line_height,
        combined_h,
        screen_height,
        scale_factor,
    );

    (bx, by)
}

pub fn diag_popup_byte_at(mx: f32, my: f32) -> usize {
    DIAG_CHARS.with(|chars| {
        let chars = chars.borrow();
        if chars.is_empty() {
            return 0;
        }

        let mut best_y_dist = f32::MAX;
        let mut best_y = chars[0].y;
        for c in chars.iter() {
            let dist = (my - (c.y + c.h / 2.0)).abs();
            if dist < best_y_dist {
                best_y_dist = dist;
                best_y = c.y;
            }
        }

        let mut closest = 0;
        let mut best_x_dist = f32::MAX;
        for c in chars.iter() {
            if (c.y - best_y).abs() < 1.0 {
                let cx = c.x + c.w / 2.0;
                let dist = (mx - cx).abs();
                if dist < best_x_dist {
                    best_x_dist = dist;
                    closest = if mx > cx {
                        c.byte_offset.saturating_add(c.byte_len)
                    } else {
                        c.byte_offset
                    };
                }
            }
        }
        closest
    })
}

fn valid_diagnostic_popup_cache(
    cache: Vec<crate::app::mouse::HoveredDiagnostic>,
    diagnostics: &[&Diagnostic],
) -> Vec<crate::app::mouse::HoveredDiagnostic> {
    let mut valid: Vec<crate::app::mouse::HoveredDiagnostic> =
        Vec::with_capacity(cache.len());
    for diagnostic in cache {
        let idx = diagnostic.0;
        let Some(candidate) = diagnostics.get(idx).copied() else {
            continue;
        };
        let duplicate = valid.iter().any(|existing| {
            diagnostics
                .get(existing.0)
                .is_some_and(|existing_diag| *existing_diag == candidate)
        });
        if !duplicate {
            valid.push(diagnostic);
        }
    }
    valid
}

