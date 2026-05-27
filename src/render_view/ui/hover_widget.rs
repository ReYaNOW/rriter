use super::*;
use crate::app::IdePanelState;
use crate::lsp::Diagnostic;
use crate::ui_system::UiRegistry;

pub struct DiagChar {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub byte_offset: usize,
}

thread_local! {
    static TS_SPANS_CACHE: std::cell::RefCell<std::collections::HashMap<String, Vec<crate::highlighter::ColorSpan>>> = std::cell::RefCell::new(std::collections::HashMap::new());
    static DIAG_CHARS: std::cell::RefCell<Vec<DiagChar>> = std::cell::RefCell::new(Vec::new());
}

fn normalize_diagnostic_message(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    let mut chars = message.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' {
            continue;
        }
        if c == '\\' {
            match chars.peek().copied() {
                Some('n') => {
                    chars.next();
                    out.push('\n');
                }
                Some('t') => {
                    chars.next();
                    out.push_str("    ");
                }
                _ => out.push(c),
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn compute_hover_y_position(
    line_top_y: f32,
    line_height: f32,
    box_h: f32,
    screen_height: f32,
    scale_factor: f32,
) -> f32 {
    let margin = 8.0 * scale_factor;
    let min_y = 40.0 * scale_factor; // Учитываем высоту таб-бара (38.0) + микро-отступ
    let max_y = screen_height - 10.0 * scale_factor;

    let mut target_by = line_top_y - box_h - margin;

    if target_by < min_y {
        let below_y = line_top_y + line_height + margin;
        if below_y + box_h <= max_y {
            target_by = below_y;
        } else {
            // Если ни сверху, ни снизу попап полностью не влезает,
            // выбираем ту сторону, где места больше.
            let space_above = line_top_y - min_y;
            let space_below = max_y - (line_top_y + line_height);
            if space_below > space_above {
                target_by = below_y;
            } else {
                target_by = min_y; // Прижимаем к верхнему краю экрана
            }
        }
    }
    target_by
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

#[derive(Clone, Copy)]
struct HoverPopupPop {
    frame: (f32, f32, f32, f32),
    content_scissor: (f32, f32, f32, f32),
    scrollbar_alpha: f32,
}

fn smooth_hover_width_progress(anim_progress: f32, target_w: f32) -> f32 {
    if target_w >= 320.0 {
        smooth_hover_anim_progress((anim_progress / 0.94).clamp(0.0, 1.0))
    } else {
        smooth_hover_anim_progress(anim_progress)
    }
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
    let width_progress = smooth_hover_width_progress(anim_progress, target_w);
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

fn compute_hover_scissor_rect(
    anim_rect: (f32, f32, f32, f32),
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    min_y: Option<f32>,
) -> (f32, f32, f32, f32) {
    let (anim_x, anim_y, anim_w, anim_h) = anim_rect;
    let cx1 = x.max(anim_x);
    let cy1 = y.max(anim_y).max(min_y.unwrap_or(f32::NEG_INFINITY));
    let cx2 = (x + w).min(anim_x + anim_w);
    let cy2 = (y + h).min(anim_y + anim_h);
    (cx1, cy1, (cx2 - cx1).max(0.0), (cy2 - cy1).max(0.0))
}

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
                        c.byte_offset + c.w as usize * 0 + 1
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
    diagnostics_len: usize,
) -> Vec<crate::app::mouse::HoveredDiagnostic> {
    cache
        .into_iter()
        .filter(|(idx, _, _, _, _)| *idx < diagnostics_len)
        .collect()
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    fn push_hover_popup_frame(
        &mut self,
        frame_x: f32,
        frame_y: f32,
        frame_w: f32,
        frame_h: f32,
        radius: f32,
        alpha: f32,
    ) {
        if frame_w <= 0.0 || frame_h <= 0.0 {
            return;
        }
        let alpha = alpha.clamp(0.0, 1.0);

        let border_w = (radius / 3.0).round().max(1.0);
        self.push_rounded_rect(
            frame_x.round(),
            frame_y.round(),
            frame_w.round(),
            frame_h.round(),
            radius,
            [
                self.theme.minimap_bg[0],
                self.theme.minimap_bg[1],
                self.theme.minimap_bg[2],
                alpha,
            ],
        );
        self.push_rounded_rect_outline(
            frame_x.round() - border_w,
            frame_y.round() - border_w,
            frame_w.round() + border_w * 2.0,
            frame_h.round() + border_w * 2.0,
            radius,
            border_w,
            [self.theme.sel[0], self.theme.sel[1], self.theme.sel[2], alpha],
        );
    }

    pub fn draw_diagnostic_popup(
        &mut self,
        lsp_diagnostics: &[Diagnostic],
        ide_panel: &IdePanelState,
        ui_registry: &mut UiRegistry,
        attached_hover_w: f32,
        attached_hover_h: f32,
        mx: f32,
        my: f32,
        wants_pointer: &mut bool,
    ) {
        let hovered_diags_cache = crate::app::mouse::HOVER_STATE.with(|s| {
            let s = s.borrow();
            s.diagnostic_popup_cache().to_vec()
        });
        let hovered_diags_cache =
            valid_diagnostic_popup_cache(hovered_diags_cache, lsp_diagnostics.len());
        if hovered_diags_cache.is_empty() {
            crate::app::mouse::HOVER_STATE.with(|state| {
                state.borrow_mut().reset_diagnostic_popup();
            });
            return;
        }

        let s = self.scale_factor;
        let pad = 12.0 * s;
        let line_h = 22.0 * s;
        let icon_sz = 20.0 * s;
        let max_text_w = (self.width - 80.0 * s)
            .max(400.0 * s)
            .min(self.width - 40.0 * s);

        let mut global_max_w = 180.0 * s;
        let mut total_h = pad * 2.0;
        let mut parsed_diags = Vec::new();
        let mut popup_text = String::new();

        DIAG_CHARS.with(|c| c.borrow_mut().clear());
        let mut global_byte_offset = 0;
        let (sel_anchor, sel_cursor) = crate::app::mouse::HOVER_STATE.with(|s| {
            let s = s.borrow();
            (s.diag_selection_anchor, s.diag_selection_cursor)
        });
        let sel_start = sel_anchor.unwrap_or(0).min(sel_cursor.unwrap_or(0));
        let sel_end = sel_anchor.unwrap_or(0).max(sel_cursor.unwrap_or(0));
        let has_sel = sel_anchor.is_some() && sel_cursor.is_some() && sel_start != sel_end;

        for (i, &(idx, _, _, _, _)) in hovered_diags_cache.iter().enumerate() {
            let diag = &lsp_diagnostics[idx];
            let clean_msg = normalize_diagnostic_message(&diag.message);
            if i > 0 {
                popup_text.push_str("\n\n");
            }
            popup_text.push_str(&clean_msg);

            let spans = TS_SPANS_CACHE.with(|cache| {
                let mut cache = cache.borrow_mut();
                if let Some(cached) = cache.get(&clean_msg) {
                    cached.clone()
                } else {
                    let mut parsed = crate::lsp::highlight_diagnostic_message(&clean_msg);
                    parsed.sort_by_key(|s| s.start);
                    if cache.len() > 100 {
                        cache.clear();
                    }
                    cache.insert(clean_msg.clone(), parsed.clone());
                    parsed
                }
            });

            let mut lines = Vec::new();
            let mut cur_line_w = 0.0;
            let mut cur_line: Vec<(char, [f32; 4])> = Vec::new();
            let mut last_space_idx = None;
            let mut current_indent: Vec<(char, [f32; 4])> = Vec::new();
            let mut counting_indent = true;

            for (offset, c) in clean_msg.char_indices() {
                if c == '\n' {
                    lines.push(std::mem::take(&mut cur_line));
                    cur_line_w = 0.0;
                    last_space_idx = None;
                    current_indent.clear();
                    counting_indent = true;
                    continue;
                }

                if counting_indent {
                    if c == ' ' || c == '│' || c == '├' || c == '└' || c == '─' {
                        current_indent.push((' ', [0.0, 0.0, 0.0, 0.0]));
                    } else {
                        counting_indent = false;
                    }
                }

                let adv = self.char_advance(c);
                if cur_line_w + adv > max_text_w && cur_line_w > 40.0 * s {
                    if let Some(space_pos) = last_space_idx {
                        let mut remainder = cur_line.split_off(space_pos);
                        if !remainder.is_empty() && remainder[0].0 == ' ' {
                            remainder.remove(0);
                        }
                        lines.push(std::mem::take(&mut cur_line));
                        cur_line = current_indent.clone();
                        cur_line.extend(remainder);
                        cur_line_w = cur_line.iter().map(|&(ch, _)| self.char_advance(ch)).sum();
                    } else {
                        lines.push(std::mem::take(&mut cur_line));
                        cur_line = current_indent.clone();
                        cur_line_w = cur_line.iter().map(|&(ch, _)| self.char_advance(ch)).sum();
                    }
                    last_space_idx = None;
                }

                let mut color = [0.972, 0.972, 0.949, 1.0];
                for span in &spans {
                    if offset >= span.start && offset < span.end {
                        color = span.color;
                        break;
                    }
                }

                cur_line.push((c, color));
                cur_line_w += adv;

                if c == ' ' {
                    last_space_idx = Some(cur_line.len() - 1);
                }
            }
            if !cur_line.is_empty() {
                lines.push(cur_line);
            }

            let source_str = diag.source.as_deref().unwrap_or("LSP");
            let code_str = diag.code.as_deref().unwrap_or("");
            let prefix_w =
                self.measure_mono_width("(", 1.0) + self.measure_mono_width(source_str, 1.0);
            let suffix_w = if !code_str.is_empty() {
                self.measure_mono_width(" ", 1.0)
                    + self.measure_mono_width(code_str, 1.0)
                    + self.measure_mono_width(")", 1.0)
            } else {
                self.measure_mono_width(")", 1.0)
            };
            let source_full_w = prefix_w + suffix_w;

            let mut max_line_w = 0.0;
            for line in &lines {
                let w: f32 = line.iter().map(|&(ch, _)| self.char_advance(ch)).sum();
                if w > max_line_w {
                    max_line_w = w;
                }
            }

            let last_line_w = lines
                .last()
                .map(|l| l.iter().map(|&(ch, _)| self.char_advance(ch)).sum::<f32>())
                .unwrap_or(0.0);
            let mut line_count = lines.len();
            let source_on_new_line = last_line_w + source_full_w + 10.0 * s > max_text_w;

            if source_on_new_line {
                line_count += 1;
                if source_full_w > max_line_w {
                    max_line_w = source_full_w;
                }
            } else {
                let combined = last_line_w + 8.0 * s + source_full_w;
                if combined > max_line_w {
                    max_line_w = combined;
                }
            }

            let item_w = max_line_w + pad * 2.0 + icon_sz + 16.0 * s;
            if item_w > global_max_w {
                global_max_w = item_w;
            }
            total_h += line_count as f32 * line_h;
            parsed_diags.push((lines, source_on_new_line, last_line_w, line_count));
        }

        total_h += (hovered_diags_cache.len() as f32 - 1.0) * (line_h * 0.5);
        let total_content_h = total_h;
        let total_h = total_content_h
            .min(self.height * 0.30)
            .min(self.height - 60.0 * s);
        let scroll_y = crate::app::mouse::HOVER_STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.diag_max_scroll = (total_content_h - total_h).max(0.0);
            let max_scroll = state.diag_max_scroll;
            state.diag_scroll.clamp_target(0.0, max_scroll);
            state.diag_text = popup_text;
            state.diag_scroll.current.round()
        });
        let mut box_w = global_max_w.max(attached_hover_w);
        let combined_hover_h = attached_hover_h;

        let (_, first_diag_x, first_line_y_top, first_diag_y_bottom, first_diag_x_end) =
            hovered_diags_cache[0];

        let popup_anchor_x =
            crate::app::mouse::HOVER_STATE.with(|s| s.borrow().popup.as_ref().map(|p| p.anchor_x));

        box_w = box_w.round();
        let combined_h = total_h + combined_hover_h;

        let (bx, by) = compute_diagnostic_layout(
            first_line_y_top,
            line_h,
            box_w,
            combined_h,
            self.width,
            self.height,
            s,
            first_diag_x,
            popup_anchor_x,
        );

        crate::app::mouse::HOVER_STATE.with(|state| {
            state.borrow_mut().diag_rect = Some((
                bx,
                by,
                box_w,
                total_h,
                first_diag_x,
                first_diag_x_end,
                (first_line_y_top + first_diag_y_bottom) * 0.5,
            ));
        });

        ui_registry.register_blocker(
            crate::ui_system::UiId::BottomPanelBody,
            bx,
            by,
            box_w,
            total_h,
            mx,
            my,
        );

        let popup_hovered = mx >= bx && mx <= bx + box_w && my >= by && my <= by + total_h;
        if popup_hovered && !*wants_pointer {
            ui_registry.reset_cursor_state();
        }

        let anim_progress = crate::app::mouse::HOVER_STATE.with(|s| s.borrow().diag_anim_progress);
        let source_anchor_x = popup_anchor_x.unwrap_or((first_diag_x + first_diag_x_end) * 0.5);
        let source_anchor_y = (first_line_y_top + first_diag_y_bottom) * 0.5;
        let (anim_mx, anim_my) =
            stable_hover_animation_mouse(mx, my, source_anchor_x, source_anchor_y, anim_progress);
        let pop = compute_hover_popup_pop(
            anim_mx,
            anim_my,
            bx,
            by,
            box_w,
            combined_h,
            anim_progress,
            None,
            anim_progress,
        );
        let (anim_sc_x, anim_sc_y, anim_sc_w, anim_sc_h) = pop.content_scissor;

        let apply_scissor = |gl: &glow::Context,
                             height: f32,
                             x: f32,
                             y: f32,
                             w: f32,
                             h: f32,
                             min_y: Option<f32>| {
            let (cx1, cy1, cw, ch) = compute_hover_scissor_rect(
                (anim_sc_x, anim_sc_y, anim_sc_w, anim_sc_h),
                x,
                y,
                w,
                h,
                min_y,
            );
            unsafe {
                gl.enable(glow::SCISSOR_TEST);
                let sy = (height - (cy1 + ch)).round() as i32;
                gl.scissor(cx1.round() as i32, sy, cw.round() as i32, ch.round() as i32);
            }
        };

        self.flush();
        let (frame_x, frame_y, frame_w, frame_h) = pixel_stable_hover_popup_frame(
            pop.frame,
            (bx, by, box_w, combined_h),
            (source_anchor_x, source_anchor_y),
        );
        self.push_hover_popup_frame(frame_x, frame_y, frame_w, frame_h, 6.0 * s, 1.0);

        self.flush();
        let bg_min_y = if combined_hover_h > 0.0 {
            Some(frame_y)
        } else {
            None
        };
        apply_scissor(&self.gl, self.height, bx, by, box_w, total_h, bg_min_y);
        self.push_rounded_rect(
            bx.round(),
            by.round(),
            box_w.round(),
            combined_h.round(),
            6.0 * s,
            [
                (self.theme.minimap_bg[0] + 0.035).min(1.0),
                (self.theme.minimap_bg[1] + 0.035).min(1.0),
                (self.theme.minimap_bg[2] + 0.035).min(1.0),
                1.0,
            ],
        );
        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }

        if combined_hover_h > 0.0 {
            let sep_y = (by + total_h).round();
            if let Some((sep_x, sep_w)) = compute_combined_separator_visible_rect(
                bx, sep_y, box_w, frame_x, frame_y, frame_w, frame_h,
            ) {
                self.push_rect(
                    sep_x.round(),
                    sep_y,
                    sep_w.round(),
                    1.0_f32.max(s.round()),
                    [1.0, 1.0, 1.0, 0.10],
                );
            }
        }

        self.flush();
        apply_scissor(&self.gl, self.height, bx, by, box_w, total_h, None);

        let mut current_y = by + pad - scroll_y;

        for (i, &(idx, _, _, _, _)) in hovered_diags_cache.iter().enumerate() {
            let diag = &lsp_diagnostics[idx];
            let border_color = match diag.severity {
                crate::lsp::DiagSeverity::Error => [0.96, 0.26, 0.21, 1.0],
                crate::lsp::DiagSeverity::Warning => [0.95, 0.9, 0.3, 1.0],
                crate::lsp::DiagSeverity::Info => [0.26, 0.73, 0.90, 1.0],
                crate::lsp::DiagSeverity::Hint => [0.50, 0.50, 0.50, 1.0],
            };

            let source_str = diag.source.as_deref().unwrap_or("LSP");
            let code_str = diag.code.as_deref().unwrap_or("");
            let (lines, source_on_new_line, last_line_w, line_count) = &parsed_diags[i];

            let mut text_y = current_y + line_h * 0.75;
            let mut draw_x = (bx + pad).round();

            for line in lines {
                for &(c, color) in line {
                    let adv = self.char_advance(c);
                    let char_len = c.len_utf8();
                    let ch_y = text_y.round() - line_h * 0.75;

                    DIAG_CHARS.with(|chars| {
                        chars.borrow_mut().push(DiagChar {
                            x: draw_x,
                            y: ch_y,
                            w: adv,
                            h: line_h,
                            byte_offset: global_byte_offset,
                        });
                    });

                    if has_sel && global_byte_offset >= sel_start && global_byte_offset < sel_end {
                        self.push_rect(draw_x, ch_y, adv, line_h, self.theme.sel);
                    }

                    let mut b = [0; 4];
                    let s_str = c.encode_utf8(&mut b);
                    self.draw_string_mono_scaled(s_str, draw_x, text_y.round(), color, 1.0);
                    draw_x += adv;
                    global_byte_offset += char_len;
                }
                text_y += line_h;
                draw_x = (bx + pad).round();
                global_byte_offset += 1;
            }

            if !*source_on_new_line {
                text_y -= line_h;
                draw_x = (bx + pad).round() + *last_line_w + 8.0 * s;
            }

            self.draw_string_mono_scaled("(", draw_x, text_y.round(), [0.55, 0.55, 0.6, 1.0], 1.0);
            draw_x += self.measure_mono_width("(", 1.0);
            self.draw_string_mono_scaled(
                source_str,
                draw_x,
                text_y.round(),
                [0.55, 0.55, 0.6, 1.0],
                1.0,
            );
            draw_x += self.measure_mono_width(source_str, 1.0);

            if !code_str.is_empty() {
                self.draw_string_mono_scaled(
                    " ",
                    draw_x,
                    text_y.round(),
                    [0.55, 0.55, 0.6, 1.0],
                    1.0,
                );
                draw_x += self.measure_mono_width(" ", 1.0);

                let sfx_w = self.measure_mono_width(code_str, 1.0);
                let has_href = diag.code_href.is_some();
                let sfx_hovered = has_href
                    && mx >= draw_x - 1.0
                    && mx <= draw_x + sfx_w + 1.0
                    && my >= text_y.round() - line_h
                    && my <= text_y.round() + 2.0 * s;

                let link_color: [f32; 4] = [0.72, 0.52, 1.0, 1.0];
                let sfx_color = if sfx_hovered {
                    link_color
                } else {
                    [link_color[0], link_color[1], link_color[2], 0.85]
                };

                if has_href {
                    let ul_alpha = if sfx_hovered { 0.9 } else { 0.55 };
                    self.push_rect(
                        draw_x,
                        text_y.round() + 1.0,
                        sfx_w,
                        1.0,
                        [link_color[0], link_color[1], link_color[2], ul_alpha],
                    );
                    if sfx_hovered {
                        *wants_pointer = true;
                        crate::app::mouse::HOVER_STATE.with(|state| {
                            state.borrow_mut().diag_href = diag.code_href.clone();
                        });
                    }

                    ui_registry.register_rect(
                        crate::ui_system::UiId::PopupOpenDiagUrl(idx),
                        draw_x - 1.0,
                        text_y.round() - line_h,
                        sfx_w + 2.0,
                        line_h + 2.0 * s,
                        mx,
                        my,
                    );
                }
                self.draw_string_mono_scaled(code_str, draw_x, text_y.round(), sfx_color, 1.0);
                draw_x += sfx_w;
            }

            self.draw_string_mono_scaled(")", draw_x, text_y.round(), [0.55, 0.55, 0.6, 1.0], 1.0);

            let total_text_h = *line_count as f32 * line_h;
            self.push_rect(bx + 4.0 * s, current_y, 3.0 * s, total_text_h, border_color);

            let is_copied = ide_panel.diag_copied_idx == Some(idx);
            let btn_x = (bx + box_w - pad - icon_sz).round();
            let btn_y = (current_y + (total_text_h - icon_sz) / 2.0).round();
            let btn_hovered = mx >= btn_x - 4.0 * s
                && mx <= btn_x + icon_sz + 4.0 * s
                && my >= btn_y - 2.0 * s
                && my <= btn_y + icon_sz + 4.0 * s;

            if btn_hovered {
                self.push_rounded_rect(
                    btn_x - 4.0 * s,
                    btn_y - 2.0 * s,
                    icon_sz + 8.0 * s,
                    icon_sz + 4.0 * s,
                    4.0 * s,
                    [1.0, 1.0, 1.0, 0.1],
                );
                *wants_pointer = true;
            }
            let icon_type = if is_copied {
                crate::widgets::IconType::Check
            } else {
                crate::widgets::IconType::Copy
            };
            let icon_color = if is_copied {
                [0.3, 0.9, 0.4, 1.0]
            } else {
                self.theme.fg
            };
            let icon_render_sz = 16.0 * s;
            let offset = (icon_sz - icon_render_sz) / 2.0;
            self.draw_atlas_icon(
                icon_type,
                btn_x + offset,
                btn_y + offset,
                icon_render_sz,
                icon_color,
            );

            ui_registry.register_rect(
                crate::ui_system::UiId::PopupCopyDiagnostic(idx),
                btn_x - 4.0 * s,
                btn_y - 2.0 * s,
                icon_sz + 8.0 * s,
                icon_sz + 4.0 * s,
                mx,
                my,
            );

            current_y += total_text_h + line_h * 0.5;
        }

        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }
    }

    pub(crate) fn build_hover_popup_layout(
        &mut self,
        popup: &crate::app::mouse::HoverPopup,
        max_text_w: f32,
        line_h: f32,
    ) -> crate::app::mouse::HoverLayoutCache {
        let s = self.scale_factor;
        let mut lines: Vec<crate::app::mouse::HoverVisualLine> = Vec::new();
        let mut cur_line_w = 0.0;
        let mut cur_line: Vec<(char, [f32; 4], usize)> = Vec::new();
        let mut last_space_idx = None;
        let mut raw_line_no = 0usize;
        let mut leading_spaces = 0;
        let mut counting_leading = true;
        let mut span_idx = 0usize;

        for (offset, c) in popup.text.char_indices() {
            let kind = popup
                .line_kinds
                .get(raw_line_no)
                .copied()
                .unwrap_or(crate::lsp::HoverLineKindPublic::Text);
            let scale_mul = match kind {
                crate::lsp::HoverLineKindPublic::Header1 => 1.3,
                crate::lsp::HoverLineKindPublic::Header2 => 1.15,
                _ => 1.0,
            };

            if c == '\n' {
                lines.push(crate::app::mouse::HoverVisualLine {
                    glyphs: std::mem::take(&mut cur_line),
                    kind,
                });
                cur_line_w = 0.0;
                last_space_idx = None;
                raw_line_no += 1;
                counting_leading = true;
                leading_spaces = 0;
                continue;
            }

            if counting_leading {
                if c == ' ' {
                    leading_spaces += 1;
                } else {
                    counting_leading = false;
                }
            }

            let adv = self.char_advance(c) * scale_mul;
            if cur_line_w + adv > max_text_w && cur_line_w > 40.0 * s {
                if let Some(space_pos) = last_space_idx {
                    let mut remainder = cur_line.split_off(space_pos);
                    if !remainder.is_empty() && remainder[0].0 == ' ' {
                        remainder.remove(0);
                    }

                    let hanging_spaces = (leading_spaces + 4).min(20);
                    let mut new_remainder = Vec::with_capacity(hanging_spaces + remainder.len());
                    for _ in 0..hanging_spaces {
                        new_remainder.push((' ', [0.0, 0.0, 0.0, 0.0], offset));
                    }
                    new_remainder.extend(remainder);
                    remainder = new_remainder;

                    lines.push(crate::app::mouse::HoverVisualLine {
                        glyphs: std::mem::take(&mut cur_line),
                        kind,
                    });
                    cur_line = remainder;
                    cur_line_w = cur_line
                        .iter()
                        .map(|&(ch, _, _)| self.char_advance(ch) * scale_mul)
                        .sum();
                } else {
                    lines.push(crate::app::mouse::HoverVisualLine {
                        glyphs: std::mem::take(&mut cur_line),
                        kind,
                    });
                    cur_line_w = 0.0;
                }
                last_space_idx = None;
            }

            while span_idx < popup.spans.len() && offset >= popup.spans[span_idx].end {
                span_idx += 1;
            }
            let color = popup
                .spans
                .get(span_idx)
                .filter(|span| offset >= span.start && offset < span.end)
                .map(|span| span.color)
                .unwrap_or([0.972, 0.972, 0.949, 1.0]);

            cur_line.push((c, color, offset));
            cur_line_w += adv;

            if c == ' ' && hover_wrap_space_can_break(cur_line.len()) {
                last_space_idx = Some(cur_line.len() - 1);
            }
        }
        if !cur_line.is_empty() {
            let kind = popup
                .line_kinds
                .get(raw_line_no)
                .copied()
                .unwrap_or(crate::lsp::HoverLineKindPublic::Text);
            lines.push(crate::app::mouse::HoverVisualLine {
                glyphs: cur_line,
                kind,
            });
        }

        while let Some(line) = lines.last() {
            if line.glyphs.is_empty() {
                lines.pop();
            } else {
                break;
            }
        }

        let mut max_line_w: f32 = 0.0;
        let mut total_text_h: f32 = 0.0;
        for line in &lines {
            let scale_mul = match line.kind {
                crate::lsp::HoverLineKindPublic::Header1 => 1.15,
                crate::lsp::HoverLineKindPublic::Header2 => 1.05,
                _ => 1.0,
            };
            let w = if matches!(
                line.kind,
                crate::lsp::HoverLineKindPublic::Header1 | crate::lsp::HoverLineKindPublic::Header2
            ) {
                let mut s_buf = String::new();
                for &(c, _, _) in &line.glyphs {
                    s_buf.push(c);
                }
                self.measure_ui_width(&s_buf, scale_mul)
            } else {
                let mut w: f32 = line
                    .glyphs
                    .iter()
                    .map(|&(ch, _, _)| self.char_advance(ch))
                    .sum();
                if line.kind == crate::lsp::HoverLineKindPublic::Code {
                    w += 16.0 * s;
                }
                w
            };
            max_line_w = max_line_w.max(w);
            total_text_h += line_h * scale_mul;
        }

        crate::app::mouse::HoverLayoutCache {
            scale_factor: self.scale_factor,
            max_text_w,
            span_count: popup.spans.len(),
            text_len: popup.text.len(),
            lines,
            max_line_w,
            total_text_h,
        }
    }

    pub fn draw_hover_popup(
        &mut self,
        popup: &mut crate::app::mouse::HoverPopup,
        attached_diag: Option<(f32, f32, f32, f32)>,
        selection: Option<(usize, usize)>,
        editor: &crate::editor::Editor,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        render_scroll_y: f32,
        wants_pointer: &mut bool,
        opacity: f32,
        stable_size: Option<(f32, f32)>,
    ) -> (f32, f32, f32, f32, f32) {
        let s = self.scale_factor;
        let opacity = opacity.clamp(0.0, 1.0);
        let pad = 12.0 * s;
        let line_h = 22.0 * s;
        let max_text_w = (self.width - 80.0 * s).min(820.0 * s).max(320.0 * s);

        let cache_valid = popup.layout_cache.as_ref().is_some_and(|cache| {
            cache.scale_factor == self.scale_factor
                && cache.max_text_w == max_text_w
                && cache.span_count == popup.spans.len()
                && cache.text_len == popup.text.len()
        });
        if !cache_valid {
            popup.layout_cache = Some(self.build_hover_popup_layout(popup, max_text_w, line_h));
        }
        let layout = popup.layout_cache.as_ref().unwrap();
        let lines = &layout.lines;
        let module_prefix_chars: Vec<char> = "[[MODULE]] ".chars().collect();

        let mut box_w = layout.max_line_w + pad * 2.0;
        if let Some((_, _, diag_w, _)) = attached_diag {
            box_w = box_w.max(diag_w);
        }
        let fixed_visible_size = stable_size.is_some();
        let max_visible_h = (self.height * 0.35).min(layout.total_text_h + pad * 2.0);
        let mut box_h = max_visible_h;
        if let Some((stable_w, stable_h)) = stable_size {
            box_w = box_w.max(stable_w);
            box_h = stable_h.max(0.0);
        }

        let phys_line = editor
            .line_offsets
            .partition_point(|&o| o <= popup.byte_offset)
            .saturating_sub(1);
        let vis_line_idx = self.phys_to_visual.get(phys_line).copied().unwrap_or(0) as f32;
        let line_top_y = (vis_line_idx * self.line_height) - render_scroll_y;

        let mut bx = popup.anchor_x;
        let mut by = line_top_y;

        if let Some(ox) = popup.offset_x {
            bx += ox;
        }
        if let Some(oy) = popup.offset_y {
            by += oy;
        }

        if popup.offset_x.is_none() || popup.offset_y.is_none() || attached_diag.is_some() {
            let orig_bx = bx;
            let orig_by = by;
            if let Some((rx, _, _, _)) = attached_diag {
                bx = rx;
            }
            let target_by = if let Some((_, diag_y, _, diag_h)) = attached_diag {
                diag_y + diag_h
            } else {
                compute_hover_y_position(line_top_y, self.line_height, box_h, self.height, s)
            };
            by = target_by;

            if bx + box_w > self.width - 20.0 * s {
                bx = self.width - box_w - 20.0 * s;
            }
            if bx < 20.0 * s {
                bx = 20.0 * s;
            }

            if attached_diag.is_none() {
                popup.offset_x = Some(bx - orig_bx);
                popup.offset_y = Some(by - orig_by);
            }
        }
        ui_registry.register_blocker(
            crate::ui_system::UiId::BottomPanelBody,
            bx,
            by,
            box_w,
            box_h,
            mx,
            my,
        );
        let popup_hovered = mx >= bx && mx <= bx + box_w && my >= by && my <= by + box_h;
        if popup_hovered && !*wants_pointer {
            ui_registry.reset_cursor_state();
        }

        let max_scroll = (layout.total_text_h + pad * 2.0 - box_h).max(0.0);
        let scroll_y = popup.scroll.current.round();

        let anim_progress = popup.anim_progress;
        let attached_anim_progress = if attached_diag.is_some() {
            crate::app::mouse::HOVER_STATE.with(|s| s.borrow().diag_anim_progress)
        } else {
            anim_progress
        };
        let (anim_mx, anim_my) = stable_hover_animation_mouse(
            mx,
            my,
            popup.anchor_x,
            popup.anchor_y,
            attached_anim_progress,
        );
        let pop = compute_hover_popup_pop(
            anim_mx,
            anim_my,
            bx,
            by,
            box_w,
            box_h,
            anim_progress,
            attached_diag,
            attached_anim_progress,
        );
        let (anim_sc_x, anim_sc_y, anim_sc_w, anim_sc_h) = pop.content_scissor;

        let apply_scissor = |gl: &glow::Context, height: f32, x: f32, y: f32, w: f32, h: f32| {
            let (cx1, cy1, cw, ch) = compute_hover_scissor_rect(
                (anim_sc_x, anim_sc_y, anim_sc_w, anim_sc_h),
                x,
                y,
                w,
                h,
                None,
            );
            unsafe {
                gl.enable(glow::SCISSOR_TEST);
                let sy = (height - (cy1 + ch)).round() as i32;
                gl.scissor(cx1.round() as i32, sy, cw.round() as i32, ch.round() as i32);
            }
        };

        if attached_diag.is_none() {
            self.flush();
            let (frame_x, frame_y, frame_w, frame_h) = pixel_stable_hover_popup_frame(
                pop.frame,
                (bx, by, box_w, box_h),
                (popup.anchor_x, popup.anchor_y),
            );
            self.push_hover_popup_frame(frame_x, frame_y, frame_w, frame_h, 6.0 * s, opacity);
        }

        self.flush();
        let content_rect =
            compute_hover_frame_content_rect(bx, by, box_w, box_h, 1.0_f32.max(s.round()));
        apply_scissor(
            &self.gl,
            self.height,
            content_rect.0,
            content_rect.1,
            content_rect.2,
            content_rect.3,
        );

        let mut current_top = by + pad - scroll_y;
        let selected = selection.filter(|(a, b)| a != b);
        let mut idx = 0usize;
        while idx < lines.len() {
            let visual_line = &lines[idx];
            let line = &visual_line.glyphs;
            let line_kind = visual_line.kind;
            let scale_mul = match line_kind {
                crate::lsp::HoverLineKindPublic::Header1 => 1.15,
                crate::lsp::HoverLineKindPublic::Header2 => 1.05,
                _ => 1.0,
            };
            let cur_line_h = line_h * scale_mul;

            let rounded_top = current_top.round();
            let text_y = if fixed_visible_size {
                current_top + cur_line_h * 0.75
            } else {
                rounded_top + (cur_line_h * 0.75).round()
            };

            if current_top + cur_line_h > by && current_top < by + box_h {
                let is_separator = line
                    .iter()
                    .all(|(c, _, _)| *c == '-' || c.is_ascii_whitespace())
                    && line.iter().any(|(c, _, _)| *c == '-');
                if is_separator {
                    self.push_rect(
                        (bx + pad).round(),
                        rounded_top + (cur_line_h * 0.5).round(),
                        (box_w - pad * 2.0).round(),
                        1.0_f32.max(s.round()),
                        [1.0, 1.0, 1.0, 0.10 * opacity],
                    );
                    current_top += cur_line_h;
                    idx += 1;
                    continue;
                }

                if line_kind == crate::lsp::HoverLineKindPublic::Code {
                    let mut run_len = 1usize;
                    while idx + run_len < lines.len()
                        && lines[idx + run_len].kind == crate::lsp::HoverLineKindPublic::Code
                    {
                        run_len += 1;
                    }
                    let code_x = (bx + pad - 4.0 * s).round();
                    let code_y = rounded_top;
                    let code_w = (box_w - pad * 2.0 + 8.0 * s).round();
                    let code_h = (line_h * run_len as f32).round();
                    let (code_x, code_y, code_w, code_h) = compute_hover_scissor_rect(
                        (anim_sc_x, anim_sc_y, anim_sc_w, anim_sc_h),
                        code_x,
                        code_y,
                        code_w,
                        code_h,
                        None,
                    );
                    if code_w > 0.0 && code_h > 0.0 {
                        self.push_rounded_rect(
                            code_x.round(),
                            code_y.round(),
                            code_w.round(),
                            code_h.round(),
                            4.0 * s,
                            [0.15, 0.16, 0.20, 0.96 * opacity],
                        );
                    }
                }

                let is_module_header = line_kind == crate::lsp::HoverLineKindPublic::Text
                    && line.len() >= module_prefix_chars.len()
                    && line
                        .iter()
                        .zip(module_prefix_chars.iter())
                        .all(|((ch, _, _), marker)| ch == marker);
                let mut glyph_start = 0usize;
                let start_x = if line_kind == crate::lsp::HoverLineKindPublic::Code {
                    (bx + pad + 8.0 * s).round()
                } else if is_module_header {
                    let icon_size = 18.0 * s;
                    let icon_x = (bx + pad).round();
                    let icon_y = rounded_top + ((cur_line_h - icon_size) * 0.5).round();
                    self.draw_file_icon("folder", true, icon_x, icon_y, icon_size);
                    glyph_start = module_prefix_chars.len();
                    (bx + pad + icon_size + 4.0 * s).round()
                } else {
                    (bx + pad).round()
                };
                let mut draw_x = start_x;
                let is_header = matches!(
                    line_kind,
                    crate::lsp::HoverLineKindPublic::Header1
                        | crate::lsp::HoverLineKindPublic::Header2
                );

                if is_header {
                    for &(c, color, offset) in line.iter().skip(glyph_start) {
                        let mut adv = 0.0;
                        if let Some(g) = self.get_ui_glyph(c) {
                            adv = g.advance * scale_mul;
                            if let Some((sel_start, sel_end)) = selected {
                                if offset >= sel_start && offset < sel_end {
                                    self.push_rect(
                                        draw_x,
                                        rounded_top,
                                        adv.ceil() + 1.0,
                                        cur_line_h.ceil() + 1.0,
                                        fade_hover_color(self.theme.sel, opacity),
                                    );
                                }
                            }
                            let glyph_x = (draw_x + g.offset_x * scale_mul).round();
                            let glyph_y = text_y - g.offset_y * scale_mul;
                            if fixed_visible_size {
                                self.push_quad_subpixel_y(
                                    glyph_x,
                                    glyph_y,
                                    g.width * scale_mul,
                                    g.height * scale_mul,
                                    g.u,
                                    g.v,
                                    g.uw,
                                    g.vh,
                                    fade_hover_color(color, opacity),
                                    g.is_emoji,
                                );
                            } else {
                                self.push_quad(
                                    glyph_x,
                                    glyph_y.round(),
                                    g.width * scale_mul,
                                    g.height * scale_mul,
                                    g.u,
                                    g.v,
                                    g.uw,
                                    g.vh,
                                    fade_hover_color(color, opacity),
                                    g.is_emoji,
                                );
                            }
                        }
                        draw_x += adv;
                    }
                } else {
                    let mut inline_run_start_x: Option<f32> = None;
                    for &(c, _, offset) in line.iter().skip(glyph_start) {
                        let adv = self.char_advance(c);
                        let in_inline = popup
                            .inline_code_ranges
                            .iter()
                            .any(|&(start, end)| offset >= start && offset < end);
                        if in_inline && inline_run_start_x.is_none() {
                            inline_run_start_x = Some(draw_x - 1.0 * s);
                        } else if !in_inline {
                            if let Some(run_x) = inline_run_start_x.take() {
                                self.push_rounded_rect(
                                    run_x,
                                    rounded_top + (cur_line_h * 0.1).round(),
                                    (draw_x - run_x + 1.0 * s).max(2.0 * s),
                                    (cur_line_h - 2.0 * s).round(),
                                    3.0 * s,
                                    [0.22, 0.23, 0.28, 0.98 * opacity],
                                );
                            }
                        }
                        draw_x += adv;
                    }
                    if let Some(run_x) = inline_run_start_x.take() {
                        self.push_rounded_rect(
                            run_x,
                            rounded_top + (cur_line_h * 0.1).round(),
                            (draw_x - run_x + 1.0 * s).max(2.0 * s),
                            (cur_line_h - 2.0 * s).round(),
                            3.0 * s,
                            [0.22, 0.23, 0.28, 0.98 * opacity],
                        );
                    }

                    draw_x = start_x;
                    for &(c, color, offset) in line.iter().skip(glyph_start) {
                        let adv = self.char_advance(c);
                        if let Some((sel_start, sel_end)) = selected {
                            if offset >= sel_start && offset < sel_end {
                                self.push_rect(
                                    draw_x,
                                    rounded_top,
                                    adv.ceil() + 1.0,
                                    cur_line_h.ceil() + 1.0,
                                    fade_hover_color(self.theme.sel, opacity),
                                );
                            }
                        }
                        let mut b = [0; 4];
                        let s_str = c.encode_utf8(&mut b);
                        if fixed_visible_size {
                            if let Some(g) = self.get_glyph(c) {
                                self.push_quad_subpixel_y(
                                    (draw_x + g.offset_x).round(),
                                    text_y - g.offset_y,
                                    g.width,
                                    g.height,
                                    g.u,
                                    g.v,
                                    g.uw,
                                    g.vh,
                                    fade_hover_color(color, opacity),
                                    g.is_emoji,
                                );
                            }
                        } else {
                            self.draw_string_mono_scaled(
                                s_str,
                                draw_x,
                                text_y,
                                fade_hover_color(color, opacity),
                                1.0,
                            );
                        }
                        draw_x += adv;
                    }
                }
            }
            current_top += cur_line_h;
            idx += 1;
        }

        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }

        let scrollbar_alpha = pop.scrollbar_alpha;
        if max_scroll > 0.0 && scrollbar_alpha > 0.0 {
            let track_h = box_h - 16.0 * s;
            let thumb_h = (box_h / (layout.total_text_h + pad * 2.0) * track_h).max(20.0 * s);
            let thumb_y = by + 8.0 * s + (scroll_y / max_scroll) * (track_h - thumb_h);
            let thumb_alpha = if fixed_visible_size { 0.34 } else { 0.2 };

            self.push_rounded_rect(
                bx + box_w - 8.0 * s,
                thumb_y.round(),
                4.0 * s,
                thumb_h,
                2.0 * s,
                [1.0, 1.0, 1.0, thumb_alpha * scrollbar_alpha * opacity],
            );

            ui_registry.register_rect(
                crate::ui_system::UiId::HoverPopupScroll,
                bx + box_w - 12.0 * s,
                by,
                12.0 * s,
                box_h,
                mx,
                my,
            );
            if ui_registry.hovered() == Some(crate::ui_system::UiId::HoverPopupScroll) {
                ui_registry.reset_cursor_state();
            }
        }

        (bx, by, box_w, box_h, max_scroll)
    }
}

#[cfg(test)]
#[path = "hover_widget_tests.rs"]
mod hover_widget_tests;
