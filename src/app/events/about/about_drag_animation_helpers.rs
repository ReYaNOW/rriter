const DRAG_AUTOSCROLL_EDGE_PX: f32 = 58.0;
const MARKDOWN_READ_SELECTION_AUTOSCROLL_EDGE_PX: f32 = 24.0;
const DRAG_AUTOSCROLL_MIN_SPEED: f32 = 360.0;
const DRAG_AUTOSCROLL_MAX_SPEED: f32 = 7200.0;
const DRAG_AUTOSCROLL_ACCEL: f32 = 0.40;
const DRAG_AUTOSCROLL_TOP_BOOST: f32 = 1.22;

#[inline(always)]
fn animation_dt(raw_dt: f32) -> f32 {
    raw_dt.min(0.016)
}

fn update_sticky_animation(
    current: &mut Vec<(usize, usize)>,
    target: &[(usize, usize)],
    progress: &mut f32,
    is_adding: &mut bool,
    dt: f32,
) -> bool {
    let mut needs_redraw = false;
    if current.as_slice() != target {
        let old_len = current.len();
        let new_len = target.len();

        if new_len > old_len {
            *progress = 0.0;
            *is_adding = true;
            current.clear();
            current.extend_from_slice(target);
        } else if new_len < old_len {
            if *is_adding || *progress >= 1.0 {
                *progress = 0.0;
                *is_adding = false;
            }
        } else {
            *progress = 1.0;
            current.clear();
            current.extend_from_slice(target);
        }
        needs_redraw = true;
    }

    if *progress < 1.0 {
        *progress += dt * 6.0;
        if *progress >= 0.99 {
            *progress = 1.0;
            if !*is_adding {
                current.clear();
                current.extend_from_slice(target);
            }
        }
        needs_redraw = true;
    }

    needs_redraw
}

fn drag_autoscroll_delta(pos: f32, start: f32, end: f32, edge: f32) -> f32 {
    if pos < start {
        pos - start
    } else if pos < start + edge {
        pos - start - edge
    } else if pos > end {
        pos - end
    } else if pos > end - edge {
        pos - end + edge
    } else {
        0.0
    }
}

#[inline(always)]
fn markdown_read_selection_autoscroll_edge(viewport_height: f32, scale: f32) -> f32 {
    if !viewport_height.is_finite()
        || !scale.is_finite()
        || viewport_height <= 0.0
        || scale <= 0.0
    {
        return 0.0;
    }
    (MARKDOWN_READ_SELECTION_AUTOSCROLL_EDGE_PX * scale).min(viewport_height * 0.25)
}

#[inline(always)]
fn markdown_read_selection_autoscroll_delta(
    pos: f32,
    start: f32,
    end: f32,
    edge: f32,
) -> f32 {
    let delta = drag_autoscroll_delta(pos, start, end, edge);
    if delta == 0.0 || !edge.is_finite() || edge <= 0.0 {
        return delta;
    }
    if pos < start {
        delta - edge
    } else if pos > end {
        delta + edge
    } else {
        delta
    }
}

#[inline(always)]
fn selection_drag_autoscroll_delta(pos: f32, start: f32, end: f32) -> f32 {
    if pos < start {
        pos - start
    } else if pos > end {
        pos - end
    } else {
        0.0
    }
}

#[inline(always)]
pub(super) fn selection_drag_active_on_cursor_leave(
    is_dragging: bool,
    show_settings: bool,
    is_dragging_terminal: bool,
    last_click_ui_id: Option<crate::ui_system::UiId>,
) -> bool {
    is_dragging
        && !show_settings
        && matches!(
            (last_click_ui_id, is_dragging_terminal),
            (Some(crate::ui_system::UiId::EditorTextBody), false)
                | (Some(crate::ui_system::UiId::TerminalBody), true)
        )
}

#[inline(always)]
pub(super) fn project_cursor_outside_window_on_leave(
    x: f32,
    y: f32,
    window_w: f32,
    window_h: f32,
) -> (f32, f32) {
    if !x.is_finite()
        || !y.is_finite()
        || !window_w.is_finite()
        || !window_h.is_finite()
        || window_w <= 0.0
        || window_h <= 0.0
        || x < 0.0
        || x > window_w
        || y < 0.0
        || y > window_h
    {
        return (x, y);
    }

    let left = x;
    let right = window_w - x;
    let top = y;
    let bottom = window_h - y;

    if left <= right && left <= top && left <= bottom {
        (-1.0, y)
    } else if right <= top && right <= bottom {
        (window_w + 1.0, y)
    } else if top <= bottom {
        (x, -1.0)
    } else {
        (x, window_h + 1.0)
    }
}

fn drag_autoscroll_speed(delta: f32, is_top_edge: bool) -> f32 {
    let amount = delta.abs();
    let speed = (amount * amount * DRAG_AUTOSCROLL_ACCEL)
        .clamp(DRAG_AUTOSCROLL_MIN_SPEED, DRAG_AUTOSCROLL_MAX_SPEED);
    if is_top_edge {
        (speed * DRAG_AUTOSCROLL_TOP_BOOST).min(DRAG_AUTOSCROLL_MAX_SPEED)
    } else {
        speed
    }
}

fn terminal_drag_cell(
    mx: f32,
    my: f32,
    panel_x: f32,
    term_y: f32,
    term_h: f32,
    scroll_offset: f32,
    char_w: f32,
    char_h: f32,
    scale: f32,
    cols: usize,
    total_lines: usize,
) -> (usize, usize) {
    let (_, bottom_pad) = crate::render_view::terminal_ui::terminal_text_padding(scale);
    let offset_from_bottom =
        (term_y + term_h - bottom_pad - my + scroll_offset) / char_h.max(0.0001);
    let cell_y = total_lines
        .saturating_sub(1)
        .saturating_sub(offset_from_bottom.max(0.0).floor() as usize)
        .min(total_lines.saturating_sub(1));
    let cell_x = ((mx - panel_x) / char_w.max(0.0001)).floor().max(0.0) as usize;
    (cell_x.min(cols.saturating_sub(1)), cell_y)
}

