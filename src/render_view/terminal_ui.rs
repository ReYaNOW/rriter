use crate::app::IdePanelState;
use crate::renderer::Renderer;
use crate::ui_system::UiRegistry;
use glow::HasContext;

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
        let term_tab_h = 24.0 * s;
        let mut cx = panel_x + 8.0 * s;
        let cy = content_y + 4.0 * s;

        for i in 0..ide_panel.terminals.len() {
            let is_active = i == ide_panel.active_terminal;
            let title = format!("{} {}", ide_panel.terminals[i].title, i + 1);
            let title_w = self.measure_ui_width(&title, 0.85);
            let tab_w = title_w + 24.0 * s + 16.0 * s;

            let is_hovered = mx >= cx && mx <= cx + tab_w && my >= cy && my <= cy + term_tab_h;
            let bg_color = if is_active {
                [
                    (self.theme.bg[0] + 0.15).min(1.0),
                    (self.theme.bg[1] + 0.15).min(1.0),
                    (self.theme.bg[2] + 0.15).min(1.0),
                    1.0,
                ]
            } else if is_hovered {
                [
                    (self.theme.bg[0] + 0.05).min(1.0),
                    (self.theme.bg[1] + 0.05).min(1.0),
                    (self.theme.bg[2] + 0.05).min(1.0),
                    1.0,
                ]
            } else {
                [0.0, 0.0, 0.0, 0.0]
            };

            if bg_color[3] > 0.0 {
                self.push_rounded_rect(cx, cy, tab_w, term_tab_h, 4.0 * s, bg_color);
            }

            let text_color = if is_active {
                self.theme.fg
            } else {
                self.theme.line_num
            };
            self.draw_string_scaled(
                &title,
                cx + 8.0 * s,
                cy + term_tab_h / 2.0 + 4.0 * s,
                text_color,
                0.85,
            );

            let close_sz = 14.0 * s;
            let close_x = cx + tab_w - 8.0 * s - close_sz;
            let close_y = cy + (term_tab_h - close_sz) / 2.0;
            let c_hovered = mx >= close_x - 2.0 * s
                && mx <= close_x + close_sz + 2.0 * s
                && my >= close_y - 2.0 * s
                && my <= close_y + close_sz + 2.0 * s;
            if c_hovered {
                self.push_rounded_rect(
                    close_x - 2.0 * s,
                    close_y - 2.0 * s,
                    close_sz + 4.0 * s,
                    close_sz + 4.0 * s,
                    2.0 * s,
                    [1.0, 1.0, 1.0, 0.2],
                );
            }
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
            ui_registry.register_rect(
                crate::ui_system::UiId::TerminalTab(i),
                cx,
                cy,
                tab_w - close_sz - 4.0 * s,
                term_tab_h,
                mx,
                my,
            );

            cx += tab_w + 4.0 * s;
        }

        let add_sz = 16.0 * s;
        let add_y = cy + (term_tab_h - add_sz) / 2.0;
        let add_hovered = mx >= cx && mx <= cx + add_sz && my >= add_y && my <= add_y + add_sz;
        if add_hovered {
            self.push_rounded_rect(
                cx - 2.0 * s,
                add_y - 2.0 * s,
                add_sz + 4.0 * s,
                add_sz + 4.0 * s,
                2.0 * s,
                [1.0, 1.0, 1.0, 0.1],
            );
        }
        self.draw_atlas_icon(
            crate::widgets::IconType::Plus,
            cx,
            add_y,
            add_sz,
            self.theme.fg,
        );
        ui_registry.register_rect(
            crate::ui_system::UiId::TerminalAdd,
            cx - 2.0 * s,
            add_y - 2.0 * s,
            add_sz + 4.0 * s,
            add_sz + 4.0 * s,
            mx,
            my,
        );

        let term_content_y = cy + term_tab_h + 4.0 * s;
        let term_content_h = content_h - (term_content_y - content_y);

        let active = ide_panel.active_terminal;
        if let Some(term) = ide_panel.terminals.get(active) {
            let mut grid = term.grid.lock().unwrap();
            let term_scale = 1.05;
            let char_w = self.char_advance('A') * term_scale;
            let char_h = self.line_height * term_scale;
            let new_cols = ((panel_w - 20.0 * s) / char_w).floor().max(10.0) as usize;
            let term_pad_bottom = 8.0 * s;
            let new_rows = ((term_content_h - term_pad_bottom) / char_h)
                .floor()
                .max(2.0) as usize;

            if grid.cols != new_cols || grid.visible_rows != new_rows {
                grid.resize(new_cols, new_rows);
                term.resize_pty(new_cols as u16, new_rows as u16);
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
                ((total_lines as f32 * char_h) - term_content_h).max(0.0)
            };

            let scroll_offset = if grid.is_alt {
                0.0
            } else {
                term.scroll_y.current.min(max_scroll).round()
            };
            let draw_x = panel_x + 10.0 * s;

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

            for i in 0..total_lines {
                let offset_from_bottom = total_lines - 1 - i;
                let draw_y = term_content_y + term_content_h
                    - 8.0 * s
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
                    if let Some((sx, sy, ex, ey)) = grid.selection {
                        let start_y = sy.min(ey);
                        let end_y = sy.max(ey);
                        let start_x = if sy < ey {
                            sx
                        } else if sy > ey {
                            ex
                        } else {
                            sx.min(ex)
                        };
                        let end_x = if sy < ey {
                            ex
                        } else if sy > ey {
                            sx
                        } else {
                            sx.max(ex)
                        };
                        let in_sel = if i > start_y && i < end_y {
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
                        if in_sel {
                            bg_color = Some(self.theme.sel);
                        }
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
                        self.draw_string_mono_scaled(
                            &cell.c.to_string(),
                            cx,
                            draw_y + self.baseline_offset * term_scale,
                            fg_color,
                            term_scale,
                        );
                    }
                }
            }

            if ide_panel.terminal_focused {
                let cursor_offset_from_bottom = grid
                    .lines
                    .len()
                    .saturating_sub(1)
                    .saturating_sub(grid.cur_y);
                let cursor_px_y = term_content_y + term_content_h
                    - 8.0 * s
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

            if max_scroll > 0.0 {
                let track_h = term_content_h;
                let handle_h = (term_content_h / (total_lines as f32 * char_h)) * track_h;
                let handle_h = handle_h.max(20.0 * s);
                let scroll_progress = term.scroll_y.current / max_scroll;
                let handle_y = term_content_y + term_content_h
                    - handle_h
                    - scroll_progress * (track_h - handle_h);
                let sb_w = 10.0 * s;
                let thumb_w = sb_w - 2.0 * s;
                self.push_rounded_rect(
                    panel_x + panel_w - sb_w + 1.0 * s,
                    handle_y,
                    thumb_w,
                    handle_h,
                    thumb_w / 2.0,
                    [0.7, 0.33, 0.54, 0.8],
                );
                ui_registry.register_rect(
                    crate::ui_system::UiId::TerminalScrollY,
                    panel_x + panel_w - sb_w,
                    term_content_y,
                    sb_w,
                    term_content_h,
                    mx,
                    my,
                );
            }

            self.flush();
            unsafe {
                self.gl.disable(glow::SCISSOR_TEST);
            }
        }

                        if ide_panel.terminal_focused {
            ui_registry.register_blocker(
                crate::ui_system::UiId::TerminalBody,
                panel_x,
                term_content_y,
                panel_w,
                term_content_h,
                mx,
                my,
            );
        }
    }
}
