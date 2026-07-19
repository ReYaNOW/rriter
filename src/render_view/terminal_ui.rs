use crate::app::IdePanelState;
use crate::renderer::Renderer;
use crate::ui_system::UiRegistry;
use glow::HasContext;

pub(crate) const TERMINAL_TEXT_SCALE: f32 = 1.05;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TerminalTabsMetrics {
    pub available: f32,
    pub gap: f32,
    pub per_tab: f32,
    pub add_size: f32,
    pub add_x: f32,
}

pub(crate) fn terminal_tabs_metrics(
    panel_x: f32,
    panel_w: f32,
    tab_count: usize,
    scale: f32,
) -> TerminalTabsMetrics {
    let panel_w = panel_w.max(0.0);
    let add_reserve = (34.0 * scale).min(panel_w);
    let available = (panel_w - 16.0 * scale - add_reserve).max(0.0);
    let gap = if tab_count > 1 {
        (4.0 * scale).min(available / (tab_count - 1) as f32)
    } else {
        0.0
    };
    let per_tab = if tab_count == 0 {
        0.0
    } else {
        ((available - gap * tab_count.saturating_sub(1) as f32) / tab_count as f32)
            .max(0.0)
    };
    let add_size = (20.0 * scale).min(panel_w);
    let add_x = (panel_x + panel_w - 8.0 * scale - add_size).max(panel_x);
    TerminalTabsMetrics { available, gap, per_tab, add_size, add_x }
}

#[inline(always)]
pub(crate) fn clamp_terminal_pty_dimension(value: usize) -> u16 {
    value.min(u16::MAX as usize) as u16
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TerminalSearchGeometry {
    pub x: f32,
    pub w: f32,
    pub input_w: f32,
    pub close_x: f32,
    pub close_size: f32,
    pub text_viewport_w: f32,
    pub show_nav: bool,
    pub show_case: bool,
    pub counter_reserve: f32,
}

pub(crate) fn terminal_search_geometry(
    panel_x: f32,
    panel_w: f32,
    scale: f32,
) -> TerminalSearchGeometry {
    let panel_w = panel_w.max(0.0);
    let w = (480.0 * scale).min((panel_w - 16.0 * scale).max(0.0));
    let x = (panel_x + panel_w - w - 8.0 * scale).max(panel_x);
    let btn_size = 36.0 * scale;
    let btn_gap = (10.0 * scale).min(w * 0.025);
    let show_nav = w >= 250.0 * scale;
    let show_case = w >= 330.0 * scale;
    let button_count = 1 + usize::from(show_nav) * 2 + usize::from(show_case);
    let controls_w = button_count as f32 * btn_size
        + button_count.saturating_sub(1) as f32 * btn_gap;
    let counter_reserve = if w >= 235.0 * scale { 52.0 * scale } else { 0.0 };
    let input_w = (w - 20.0 * scale - controls_w - counter_reserve - 8.0 * scale)
        .max(0.0);
    let close_size = btn_size.min(w);
    let close_x = (x + w - close_size).max(x);
    TerminalSearchGeometry {
        x,
        w,
        input_w,
        close_x,
        close_size,
        text_viewport_w: (input_w - 10.0 * scale).max(0.0),
        show_nav,
        show_case,
        counter_reserve,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TerminalScrollbarLayout {
    pub track_x: f32,
    pub track_y: f32,
    pub track_w: f32,
    pub track_h: f32,
    pub thumb_y: f32,
    pub thumb_h: f32,
    pub max_scroll: f32,
}

pub(crate) fn terminal_body_rect(content_y: f32, content_h: f32, scale: f32) -> (f32, f32) {
    let tab_top_pad = 6.0 * scale;
    let tab_h = 32.0 * scale;
    let tab_bottom_gap = 4.0 * scale;
    let body_offset = tab_top_pad + tab_h + tab_bottom_gap;
    (content_y + body_offset, (content_h - body_offset).max(0.0))
}

#[inline(always)]
pub(crate) fn terminal_tab_body_width(tab_w: f32, close_visible: bool, scale: f32) -> f32 {
    (tab_w - if close_visible { 32.0 * scale } else { 0.0 }).max(0.0)
}

#[inline(always)]
pub(crate) fn terminal_body_hitbox(
    panel_x: f32,
    term_y: f32,
    panel_w: f32,
    term_h: f32,
) -> Option<crate::ui_system::UiClipRect> {
    (panel_w > 0.0 && term_h > 0.0)
        .then(|| crate::ui_system::UiClipRect::new(panel_x, term_y, panel_w, term_h))
}

#[inline(always)]
pub(crate) fn terminal_text_padding(scale: f32) -> (f32, f32) {
    (8.0 * scale, 8.0 * scale)
}

#[inline(always)]
pub(crate) fn terminal_text_viewport_height(term_h: f32, scale: f32) -> f32 {
    let (top, bottom) = terminal_text_padding(scale);
    (term_h - top - bottom).max(0.0)
}

#[inline(always)]
pub(crate) fn terminal_visible_rows(term_h: f32, char_h: f32, scale: f32) -> usize {
    (terminal_text_viewport_height(term_h, scale) / char_h.max(0.0001))
        .floor()
        .max(2.0) as usize
}

#[inline(always)]
pub(crate) fn terminal_max_scroll(
    total_lines: usize,
    char_h: f32,
    term_h: f32,
    scale: f32,
) -> f32 {
    (total_lines as f32 * char_h - terminal_text_viewport_height(term_h, scale)).max(0.0)
}

#[inline(always)]
pub(crate) fn terminal_render_scroll_offset(
    current_scroll: f32,
    max_scroll: f32,
    is_alt: bool,
) -> f32 {
    if is_alt {
        0.0
    } else {
        current_scroll.min(max_scroll).round()
    }
}

pub(crate) fn terminal_scrollbar_layout(
    panel_x: f32,
    panel_w: f32,
    term_y: f32,
    term_h: f32,
    scale: f32,
    char_h: f32,
    total_lines: usize,
    current_scroll: f32,
) -> Option<TerminalScrollbarLayout> {
    let max_scroll = terminal_max_scroll(total_lines, char_h, term_h, scale);
    if max_scroll <= 0.0 {
        return None;
    }

    let frame_inset = 4.0 * scale;
    let track_w = 8.0 * scale;
    let track_x = panel_x + panel_w - frame_inset - track_w;
    let track_y = term_y + frame_inset;
    let track_h = (term_h - frame_inset * 2.0).max(1.0);
    let viewport_h = terminal_text_viewport_height(term_h, scale);
    let content_h = total_lines as f32 * char_h;
    let scroll_from_top = max_scroll - current_scroll.clamp(0.0, max_scroll);
    let thumb = crate::scroll::scrollbar_thumb(
        track_y,
        track_h,
        viewport_h,
        content_h,
        scroll_from_top,
        20.0 * scale,
    )?;
    Some(TerminalScrollbarLayout {
        track_x,
        track_y,
        track_w,
        track_h,
        thumb_y: thumb.start,
        thumb_h: thumb.len,
        max_scroll,
    })
}

pub(crate) fn terminal_scrollbar_drag_target(
    pointer_y: f32,
    layout: TerminalScrollbarLayout,
    drag_offset: Option<f32>,
) -> Option<(f32, f32)> {
    let thumb = crate::scroll::ScrollbarThumb {
        start: layout.thumb_y,
        len: layout.thumb_h,
    };
    let (offset, scroll_from_top) = crate::scroll::scrollbar_drag_target(
        pointer_y,
        layout.track_y,
        layout.track_h,
        thumb,
        layout.max_scroll,
        drag_offset,
    )?;
    Some((offset, layout.max_scroll - scroll_from_top))
}

fn terminal_glyph_anchor(
    c: char,
    glyph: crate::renderer::GlyphInfo,
    cell_x: f32,
    row_y: f32,
    cell_w: f32,
    char_h: f32,
    baseline_y: f32,
    scale: f32,
) -> (f32, f32, f32) {
    if !crate::renderer::terminal_force_text_presentation(c) || glyph.is_emoji != 0.0 {
        return (cell_x, baseline_y, scale);
    }

    let max_w = cell_w * 0.70;
    let max_h = char_h * 0.58;
    let fit_scale = scale
        .min(max_w / glyph.width.max(1.0))
        .min(max_h / glyph.height.max(1.0));
    let fitted_w = glyph.width * fit_scale;
    let fitted_h = glyph.height * fit_scale;
    let x = cell_x + (cell_w - fitted_w) * 0.5 - glyph.offset_x * fit_scale;
    let y = row_y + (char_h - fitted_h) * 0.5 + glyph.offset_y * fit_scale;
    (x, y, fit_scale)
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    pub fn draw_terminal_panel(
        &mut self,
        panel_x: f32,
        content_y: f32,
        panel_w: f32,
        content_h: f32,
        s: f32,
        ide_panel: &IdePanelState,
        ui_registry: &mut UiRegistry,
        mx: f32,
        my: f32,
    ) {
        let term_tab_h = 32.0 * s;
        let mut cx = panel_x + 8.0 * s;
        let cy = content_y + 6.0 * s;
        let mut scratch = std::mem::take(&mut self.scratch_buffer);
        let tab_count = ide_panel.terminals.len();
        let tab_metrics = terminal_tabs_metrics(panel_x, panel_w, tab_count, s);
        let tab_gap = tab_metrics.gap;
        let per_tab_w = tab_metrics.per_tab;
        ui_registry.push_clip(crate::ui_system::UiClipRect::new(
            panel_x,
            content_y,
            panel_w,
            term_tab_h + 12.0 * s,
        ));

        for i in 0..ide_panel.terminals.len() {
            let is_active = i == ide_panel.active_terminal;
            scratch.clear();
            let _ = std::fmt::Write::write_fmt(
                &mut scratch,
                format_args!("{} {}", ide_panel.terminals[i].title, i + 1),
            );
            let title_w = self.measure_ui_width(&scratch, 0.9);
            let tab_w = (title_w + 56.0 * s).min(per_tab_w).max(0.0);
            if tab_w <= 0.0 {
                continue;
            }

            let is_hovered = mx >= cx && mx <= cx + tab_w && my >= cy && my <= cy + term_tab_h;
            let bg_color = if is_active {
                [
                    (self.theme.bg[0] + 0.20).min(1.0),
                    (self.theme.bg[1] + 0.20).min(1.0),
                    (self.theme.bg[2] + 0.20).min(1.0),
                    1.0,
                ]
            } else if is_hovered {
                [
                    (self.theme.bg[0] + 0.12).min(1.0),
                    (self.theme.bg[1] + 0.12).min(1.0),
                    (self.theme.bg[2] + 0.12).min(1.0),
                    1.0,
                ]
            } else {
                [
                    (self.theme.bg[0] + 0.04).min(1.0),
                    (self.theme.bg[1] + 0.04).min(1.0),
                    (self.theme.bg[2] + 0.04).min(1.0),
                    1.0,
                ]
            };

            if bg_color[3] > 0.0 {
                self.push_rounded_rect(cx, cy, tab_w, term_tab_h, 4.0 * s, bg_color);
            }

            let text_color = if is_active {
                self.theme.fg
            } else {
                self.theme.line_num
            };
            let close_visible = tab_w >= 46.0 * s;
            let title_max_w = (tab_w - if close_visible { 38.0 * s } else { 18.0 * s })
                .max(0.0);
            let mut title_scratch = String::new();
            self.draw_tree_label_clipped(
                &scratch,
                cx + 12.0 * s,
                cy + term_tab_h / 2.0 + 4.0 * s,
                title_max_w,
                text_color,
                0.9,
                &mut title_scratch,
            );

            let close_sz = 18.0 * s;
            let close_x = cx + tab_w - 12.0 * s - close_sz;
            let close_y = (cy + (term_tab_h - close_sz) / 2.0).round();
            let c_hovered = mx >= close_x - 2.0 * s
                && mx <= close_x + close_sz + 2.0 * s
                && my >= close_y - 2.0 * s
                && my <= close_y + close_sz + 2.0 * s;
            if close_visible && c_hovered {
                self.push_rounded_rect(
                    close_x - 2.0 * s,
                    close_y - 2.0 * s,
                    close_sz + 4.0 * s,
                    close_sz + 4.0 * s,
                    2.0 * s,
                    [1.0, 1.0, 1.0, 0.2],
                );
            }
            if close_visible {
                self.draw_atlas_icon(
                    crate::widgets::IconType::Close,
                    close_x,
                    close_y,
                    close_sz,
                    text_color,
                );
                ui_registry.register_rect(
                    crate::ui_system::UiId::TerminalTabClose(i),
                    close_x - 2.0 * s,
                    close_y - 2.0 * s,
                    close_sz + 4.0 * s,
                    close_sz + 4.0 * s,
                    mx,
                    my,
                );
            }
            ui_registry.register_rect(
                crate::ui_system::UiId::TerminalTab(i),
                cx,
                cy,
                terminal_tab_body_width(tab_w, close_visible, s),
                term_tab_h,
                mx,
                my,
            );

            cx += tab_w + tab_gap;
        }
        self.scratch_buffer = scratch;

        let add_sz = tab_metrics.add_size;
        let add_x = tab_metrics.add_x;
        let add_y = (cy + (term_tab_h - add_sz) / 2.0).round();
        let add_hovered = add_sz > 0.0
            && mx >= add_x && mx <= add_x + add_sz && my >= add_y && my <= add_y + add_sz;
        if add_hovered {
            self.push_rounded_rect(
                add_x - 2.0 * s,
                add_y - 2.0 * s,
                add_sz + 4.0 * s,
                add_sz + 4.0 * s,
                2.0 * s,
                [1.0, 1.0, 1.0, 0.1],
            );
        }
        self.draw_atlas_icon(
            crate::widgets::IconType::Plus,
            add_x,
            add_y,
            add_sz,
            self.theme.fg,
        );
        ui_registry.register_rect(
            crate::ui_system::UiId::TerminalAdd,
            add_x - 2.0 * s,
            add_y - 2.0 * s,
            (add_sz + 4.0 * s).min(panel_w.max(0.0)),
            add_sz + 4.0 * s,
            mx,
            my,
        );
        ui_registry.pop_clip();

        let (term_content_y, term_content_h) = terminal_body_rect(content_y, content_h, s);

        if let Some(hitbox) = terminal_body_hitbox(
            panel_x,
            term_content_y,
            panel_w,
            term_content_h,
        ) {
            ui_registry.register_blocker(
                crate::ui_system::UiId::TerminalBody,
                hitbox.x,
                hitbox.y,
                hitbox.w,
                hitbox.h,
                mx,
                my,
            );
        }

        let active = ide_panel.active_terminal;
        if let Some(term) = ide_panel.terminals.get(active) {
            let mut grid = crate::app::terminal::lock_terminal_grid(&term.grid);
            let term_scale = TERMINAL_TEXT_SCALE;
            let char_w = self.char_advance('A') * term_scale;
            let char_h = self.line_height * term_scale;
            let new_cols = ((panel_w - 20.0 * s) / char_w).floor().max(10.0) as usize;
            let new_rows = terminal_visible_rows(term_content_h, char_h, s);

            if grid.cols != new_cols || grid.visible_rows != new_rows {
                grid.resize(new_cols, new_rows);
                term.resize_pty(
                    clamp_terminal_pty_dimension(new_cols),
                    clamp_terminal_pty_dimension(new_rows),
                );
            }
            grid.dirty = false;

            let ansi_colors = [
                [0.10, 0.10, 0.10, 1.0],
                [0.95, 0.30, 0.30, 1.0],
                [0.30, 0.85, 0.30, 1.0],
                [0.90, 0.85, 0.20, 1.0],
                [0.30, 0.60, 1.00, 1.0],
                [0.90, 0.35, 0.90, 1.0],
                [0.20, 0.85, 0.85, 1.0],
                [0.90, 0.90, 0.90, 1.0],
                [0.45, 0.45, 0.45, 1.0],
                [1.00, 0.40, 0.40, 1.0],
                [0.40, 1.00, 0.40, 1.0],
                [1.00, 1.00, 0.40, 1.0],
                [0.50, 0.70, 1.00, 1.0],
                [1.00, 0.50, 1.00, 1.0],
                [0.40, 1.00, 1.00, 1.0],
                [1.00, 1.00, 1.00, 1.0],
            ];

            let scrollback_len = if grid.is_alt {
                0
            } else {
                grid.scrollback.len()
            };
            let total_lines = scrollback_len + grid.lines.len();
            let max_scroll = if grid.is_alt {
                0.0
            } else {
                terminal_max_scroll(total_lines, char_h, term_content_h, s)
            };

            let scroll_offset = terminal_render_scroll_offset(
                term.scroll_y.current,
                max_scroll,
                grid.is_alt,
            );
            let draw_x = panel_x + 10.0 * s;
            let (_, term_pad_bottom) = terminal_text_padding(s);

            self.flush();
            unsafe {
                self.gl.enable(glow::SCISSOR_TEST);
                let sy = (self.height - (term_content_y + term_content_h)).round() as i32;
                self.gl.scissor(
                    panel_x.round() as i32,
                    sy,
                    panel_w.round() as i32,
                    term_content_h.round() as i32,
                );
            }

            let mut row_search_results = std::mem::take(&mut self.terminal_row_search_results);

            for i in 0..total_lines {
                let offset_from_bottom = total_lines - 1 - i;
                let draw_y = term_content_y + term_content_h
                    - term_pad_bottom
                    - char_h
                    - (offset_from_bottom as f32 * char_h)
                    + scroll_offset;

                if draw_y + char_h < term_content_y || draw_y > term_content_y + term_content_h {
                    continue;
                }

                if self.vertices.len() > 30_000 {
                    self.flush();
                }

                let row = if i < scrollback_len {
                    &grid.scrollback[i]
                } else {
                    &grid.lines[i - scrollback_len]
                };

                row_search_results.clear();
                if ide_panel.term_show_search {
                    row_search_results.extend(
                        ide_panel
                            .term_search_results
                            .iter()
                            .copied()
                            .enumerate()
                            .filter(|&(_, (_sx, sy, _ex, ey))| {
                                let start_y = sy.min(ey);
                                let end_y = sy.max(ey);
                                i >= start_y && i <= end_y
                            }),
                    );
                }

                for (c_idx, cell) in row.iter().enumerate() {
                    if c_idx >= grid.cols {
                        break;
                    }
                    let cx = (draw_x + c_idx as f32 * char_w).round();
                    let next_cx = (draw_x + (c_idx + 1) as f32 * char_w).round();
                    let cell_w = next_cx - cx;
                    let mut bg_color = if cell.bg != 0 && cell.bg < 16 {
                        Some(ansi_colors[cell.bg as usize])
                    } else {
                        None
                    };

                    let mut in_sel = false;
                    if let Some((sx, sy, ex, ey)) = grid.selection {
                        let (start_x, start_y, end_x, end_y) =
                            crate::app::terminal::normalized_selection_bounds(sx, sy, ex, ey);
                        in_sel = if i > start_y && i < end_y {
                            true
                        } else if i == start_y && i == end_y {
                            c_idx >= start_x && c_idx <= end_x
                        } else if i == start_y {
                            c_idx >= start_x
                        } else if i == end_y {
                            c_idx <= end_x
                        } else {
                            false
                        };
                    }

                    let mut is_search_res = false;
                    let mut is_active_search = false;
                    for &(idx, (sx, sy, ex, ey)) in &row_search_results {
                        let (start_x, start_y, end_x, end_y) =
                            crate::app::terminal::normalized_selection_bounds(sx, sy, ex, ey);

                        let in_res = if i > start_y && i < end_y {
                            true
                        } else if i == start_y && i == end_y {
                            c_idx >= start_x && c_idx <= end_x
                        } else if i == start_y {
                            c_idx >= start_x
                        } else if i == end_y {
                            c_idx <= end_x
                        } else {
                            false
                        };

                        if in_res {
                            is_search_res = true;
                            if Some(idx) == ide_panel.term_search_current_idx {
                                is_active_search = true;
                            }
                        }
                    }

                    if is_active_search {
                        bg_color = Some([1.0, 0.6, 0.0, 0.5]);
                    } else if in_sel {
                        bg_color = Some(self.theme.sel);
                    } else if is_search_res {
                        bg_color = Some([0.6, 0.6, 0.6, 0.35]);
                    }

                    if let Some(bg) = bg_color {
                        self.push_rect(cx, draw_y, cell_w, char_h, bg);
                    }
                    if cell.c != ' ' {
                        let fg_color = if cell.fg < 16 {
                            ansi_colors[cell.fg as usize]
                        } else {
                            self.theme.fg
                        };
                        let prefer_color = match cell.presentation {
                            crate::app::terminal::CELL_PRESENTATION_TEXT => Some(false),
                            crate::app::terminal::CELL_PRESENTATION_EMOJI => Some(true),
                            _ => None,
                        };
                        if let Some(g) = self.get_terminal_glyph(cell.c, prefer_color) {
                            let baseline_y = draw_y + self.baseline_offset * term_scale;
                            let (glyph_x, glyph_y, glyph_scale) = terminal_glyph_anchor(
                                cell.c, g, cx, draw_y, cell_w, char_h, baseline_y, term_scale,
                            );
                            let (q_x, q_y, q_w, q_h) =
                                crate::renderer::glyph_quad_rect(glyph_x, glyph_y, g, glyph_scale);
                            self.push_quad(
                                q_x, q_y, q_w, q_h, g.u, g.v, g.uw, g.vh, fg_color, g.is_emoji,
                            );
                        }
                    }
                }
            }

            self.terminal_row_search_results = row_search_results;

            if ide_panel.terminal_focused {
                let cursor_offset_from_bottom = grid
                    .lines
                    .len()
                    .saturating_sub(1)
                    .saturating_sub(grid.cur_y);
                let cursor_px_y = term_content_y + term_content_h
                    - term_pad_bottom
                    - char_h
                    - (cursor_offset_from_bottom as f32 * char_h)
                    + scroll_offset;
                if grid.cursor_visible
                    && cursor_px_y + char_h >= term_content_y
                    && cursor_px_y <= term_content_y + term_content_h
                {
                    let cursor_px_x = (draw_x + grid.cur_x as f32 * char_w).round();
                    let cursor_next_x = (draw_x + (grid.cur_x + 1) as f32 * char_w).round();
                    self.push_rect(
                        cursor_px_x,
                        cursor_px_y,
                        cursor_next_x - cursor_px_x,
                        char_h,
                        [1.0, 1.0, 1.0, 0.5],
                    );
                }

                let border_color = self.theme.sel;
                self.push_rect(panel_x, term_content_y, panel_w, 2.0 * s, border_color);
                self.push_rect(
                    panel_x,
                    term_content_y + term_content_h - 2.0 * s,
                    panel_w,
                    2.0 * s,
                    border_color,
                );
                self.push_rect(
                    panel_x,
                    term_content_y,
                    2.0 * s,
                    term_content_h,
                    border_color,
                );
                self.push_rect(
                    panel_x + panel_w - 2.0 * s,
                    term_content_y,
                    2.0 * s,
                    term_content_h,
                    border_color,
                );
            }

            if let Some(scrollbar) = terminal_scrollbar_layout(
                panel_x,
                panel_w,
                term_content_y,
                term_content_h,
                s,
                char_h,
                total_lines,
                term.scroll_y.current,
            ) {
                self.push_rounded_rect(
                    scrollbar.track_x,
                    scrollbar.thumb_y,
                    scrollbar.track_w,
                    scrollbar.thumb_h,
                    scrollbar.track_w / 2.0,
                    [0.7, 0.33, 0.54, 0.8],
                );
                ui_registry.register_rect(
                    crate::ui_system::UiId::TerminalScrollY,
                    scrollbar.track_x,
                    scrollbar.track_y,
                    scrollbar.track_w,
                    scrollbar.track_h,
                    mx,
                    my,
                );
            }

            self.flush();
            unsafe {
                self.gl.disable(glow::SCISSOR_TEST);
            }
        }

        if ide_panel.term_show_search {
            let geometry = terminal_search_geometry(panel_x, panel_w, s);
            let search_w = geometry.w;
            let search_h = 52.0 * s;
            let search_x = geometry.x;
            let search_y = term_content_y + 10.0 * s;

            self.push_rounded_rect(
                search_x,
                search_y,
                search_w,
                search_h,
                6.0 * s,
                [0.18, 0.20, 0.22, 1.0],
            );
            self.push_rounded_rect(
                search_x - 1.0,
                search_y - 1.0,
                search_w + 2.0,
                search_h + 2.0,
                6.0 * s,
                [self.theme.sel[0], self.theme.sel[1], self.theme.sel[2], 0.6],
            );
            self.push_rounded_rect(
                search_x,
                search_y,
                search_w,
                search_h,
                6.0 * s,
                [
                    self.theme.minimap_bg[0],
                    self.theme.minimap_bg[1],
                    self.theme.minimap_bg[2],
                    1.0,
                ],
            );

            let input_x = search_x + 10.0 * s;
            let input_y = search_y + 11.0 * s;
            let input_h = 30.0 * s;
            let btn_size = 36.0 * s;
            let btn_gap = (10.0 * s).min(search_w * 0.025);
            let show_nav = geometry.show_nav;
            let show_case = geometry.show_case;
            let counter_reserve = geometry.counter_reserve;
            let input_w = geometry.input_w;

            if input_w > 0.0 {
                ui_registry.register_text_input(
                    crate::ui_system::UiId::TerminalSearchInput,
                    input_x,
                    input_y,
                    input_w,
                    input_h,
                    mx,
                    my,
                );
            }
            let text = ide_panel.term_search_editor.get_full_text();
            let text_empty = text.is_empty();
            self.terminal_search_scroll_x = self.one_line_scroll_for_cursor(
                &text,
                ide_panel.term_search_editor.cursor,
                1.0,
                geometry.text_viewport_w,
                self.terminal_search_scroll_x,
            );
            self.draw_one_line_input_with_chrome(
                &text,
                ide_panel.term_search_editor.cursor,
                ide_panel.term_search_editor.selection_anchor,
                false,
                ide_panel.term_search_focused,
                input_x,
                input_y,
                input_w,
                input_h,
                self.terminal_search_scroll_x,
                1.0,
                1.0,
                0.0,
                5.0 * s,
                4.0 * s,
            );
            let text_y = input_y + input_h / 2.0 + 6.0 * s;

            let btn_y = search_y + 8.0 * s;
            let close_size = geometry.close_size;
            let mut btn_x = geometry.close_x;

            let btn_close = crate::widgets::IconButton {
                x: btn_x,
                y: btn_y,
                size: close_size,
                icon: Some(crate::widgets::IconType::Close),
                is_active: false,
                icon_size: Some(26.0 * s),
                active_square_width: None,
                custom_color: None,
            };
            if close_size > 0.0 {
                ui_registry.register_icon_button(
                    crate::ui_system::UiId::TerminalSearchClose,
                    &btn_close,
                    self,
                    mx,
                    my,
                    s,
                    false,
                );
            }
            btn_x -= close_size + btn_gap;

            if show_nav {
            let btn_down = crate::widgets::IconButton {
                x: btn_x,
                y: btn_y,
                size: btn_size,
                icon: Some(crate::widgets::IconType::Down),
                is_active: false,
                icon_size: Some(37.0 * s),
                active_square_width: None,
                custom_color: None,
            };
            ui_registry.register_icon_button(
                crate::ui_system::UiId::TerminalSearchNext,
                &btn_down,
                self,
                mx,
                my,
                s,
                false,
            );
            btn_x -= btn_size + 10.0 * s;

            let btn_up = crate::widgets::IconButton {
                x: btn_x,
                y: btn_y,
                size: btn_size,
                icon: Some(crate::widgets::IconType::Up),
                is_active: false,
                icon_size: Some(37.0 * s),
                active_square_width: None,
                custom_color: None,
            };
            ui_registry.register_icon_button(
                crate::ui_system::UiId::TerminalSearchPrev,
                &btn_up,
                self,
                mx,
                my,
                s,
                false,
            );
            btn_x -= btn_size + btn_gap;
            }

            if show_case {
            let btn_case = crate::widgets::IconButton {
                x: btn_x,
                y: btn_y,
                size: btn_size,
                icon: Some(crate::widgets::IconType::CaseMatch),
                is_active: ide_panel.term_search_case_sensitive,
                icon_size: Some(30.0 * s),
                active_square_width: None,
                custom_color: None,
            };
            ui_registry.register_icon_button(
                crate::ui_system::UiId::TerminalSearchCaseToggle,
                &btn_case,
                self,
                mx,
                my,
                s,
                false,
            );
            }

            if counter_reserve > 0.0 && ide_panel.term_search_results.is_empty() {
                if !text_empty {
                    self.draw_string_mono_scaled(
                        "Нет",
                        input_x + input_w + 10.0 * s,
                        text_y,
                        [0.6, 0.6, 0.6, 1.0],
                        0.9,
                    );
                }
            } else if !ide_panel.term_search_results.is_empty() {
                let mut scratch = std::mem::take(&mut self.scratch_buffer);
                scratch.clear();
                let _ = std::fmt::Write::write_fmt(
                    &mut scratch,
                    format_args!(
                        "{}/{}",
                        ide_panel.term_search_current_idx.unwrap_or(0) + 1,
                        ide_panel.term_search_results.len()
                    ),
                );
                self.draw_string_mono_scaled(
                    &scratch,
                    input_x + input_w + 10.0 * s,
                    text_y,
                    [0.6, 0.6, 0.6, 1.0],
                    0.9,
                );
                self.scratch_buffer = scratch;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyph(
        width: f32,
        height: f32,
        offset_x: f32,
        offset_y: f32,
        is_emoji: f32,
    ) -> crate::renderer::GlyphInfo {
        crate::renderer::GlyphInfo {
            u: 0.0,
            v: 0.0,
            uw: 0.0,
            vh: 0.0,
            width,
            height,
            offset_x,
            offset_y,
            advance: width,
            is_emoji,
        }
    }

    #[test]
    fn terminal_glyph_anchor_shrinks_check_mark_inside_cell() {
        let g = glyph(18.0, 24.0, 1.0, 20.0, 0.0);
        let (x, y, scale) = terminal_glyph_anchor('✔', g, 100.0, 40.0, 12.0, 28.0, 60.0, 1.05);
        let (q_x, q_y, q_w, q_h) = crate::renderer::glyph_quad_rect(x, y, g, scale);

        assert!(scale < 1.05);
        assert!(q_x >= 100.0);
        assert!(q_x + q_w <= 112.0);
        assert!(q_y >= 40.0);
        assert!(q_y + q_h <= 68.0);
    }

    #[test]
    fn terminal_glyph_anchor_leaves_regular_and_emoji_glyphs_unchanged() {
        let regular = glyph(9.0, 18.0, 0.0, 15.0, 0.0);
        assert_eq!(
            terminal_glyph_anchor('A', regular, 10.0, 20.0, 12.0, 28.0, 40.0, 1.05),
            (10.0, 40.0, 1.05)
        );

        let emoji = glyph(20.0, 20.0, 0.0, 17.0, 1.0);
        assert_eq!(
            terminal_glyph_anchor('✅', emoji, 10.0, 20.0, 12.0, 28.0, 40.0, 1.05),
            (10.0, 40.0, 1.05)
        );
    }

    #[test]
    fn terminal_rows_keep_padding_above_the_first_full_screen_row() {
        let term_h = 300.0;
        let char_h = 26.0;
        let rows = terminal_visible_rows(term_h, char_h, 1.0);
        let (top, bottom) = terminal_text_padding(1.0);
        let first_row_y = term_h - bottom - rows as f32 * char_h;

        assert!(first_row_y >= top);
        assert!(first_row_y < top + char_h);
        assert_eq!(terminal_max_scroll(rows, char_h, term_h, 1.0), 0.0);
    }

    #[test]
    fn terminal_drag_mapping_uses_the_rendered_rounded_scroll_offset() {
        assert_eq!(terminal_render_scroll_offset(0.6, 20.0, false), 1.0);
        assert_eq!(terminal_render_scroll_offset(20.6, 20.2, false), 20.0);
        assert_eq!(terminal_render_scroll_offset(20.6, 20.2, true), 0.0);
    }

    #[test]
    fn terminal_tab_body_hitbox_stops_before_close_control() {
        assert_eq!(terminal_tab_body_width(120.0, true, 1.0), 88.0);
        assert_eq!(terminal_tab_body_width(120.0, false, 1.0), 120.0);
        assert_eq!(terminal_tab_body_width(20.0, true, 1.0), 0.0);
    }

    #[test]
    fn terminal_body_hitbox_is_available_even_before_terminal_has_focus() {
        let hitbox = terminal_body_hitbox(48.0, 400.0, 952.0, 300.0).expect("body hitbox");

        assert_eq!(hitbox, crate::ui_system::UiClipRect::new(48.0, 400.0, 952.0, 300.0));
        assert!(terminal_body_hitbox(48.0, 400.0, 0.0, 300.0).is_none());
        assert!(terminal_body_hitbox(48.0, 400.0, 952.0, 0.0).is_none());
    }

    #[test]
    fn terminal_scrollbar_is_inset_from_focus_frame_and_drags_without_jumping() {
        let layout = terminal_scrollbar_layout(
            48.0, 952.0, 400.0, 300.0, 1.0, 26.0, 40, 260.0,
        )
        .expect("scrollbar");
        assert!(layout.track_x > 48.0);
        assert!(layout.track_x + layout.track_w < 1000.0 - 2.0);
        assert!(layout.track_y > 400.0 + 2.0);
        assert!(layout.track_y + layout.track_h < 700.0 - 2.0);

        let pointer = layout.thumb_y + layout.thumb_h * 0.25;
        let (offset, target) = terminal_scrollbar_drag_target(pointer, layout, None).unwrap();
        assert!((offset - layout.thumb_h * 0.25).abs() < 0.001);
        assert!((target - 260.0).abs() < 0.001);

        let (_, top_target) =
            terminal_scrollbar_drag_target(layout.track_y, layout, Some(0.0)).unwrap();
        let (_, bottom_target) = terminal_scrollbar_drag_target(
            layout.track_y + layout.track_h,
            layout,
            Some(layout.thumb_h),
        )
        .unwrap();
        assert!((top_target - layout.max_scroll).abs() < 0.001);
        assert!(bottom_target.abs() < 0.001);
    }

    #[test]
    fn terminal_scrollbar_registered_after_body_wins_hit_testing() {
        let mut registry = crate::ui_system::UiRegistry::new();
        registry.register_blocker(
            crate::ui_system::UiId::TerminalBody,
            0.0,
            0.0,
            200.0,
            100.0,
            190.0,
            50.0,
        );
        registry.register_rect(
            crate::ui_system::UiId::TerminalScrollY,
            184.0,
            4.0,
            8.0,
            92.0,
            190.0,
            50.0,
        );
        assert_eq!(
            registry.find_at(190.0, 50.0),
            Some(crate::ui_system::UiId::TerminalScrollY)
        );
    }
}
