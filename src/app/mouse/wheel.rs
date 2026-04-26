use super::*;

impl App {
    pub fn handle_main_mouse_wheel(&mut self, delta: MouseScrollDelta) {
        self.lsp_actions_menu = None;
        let lh = self.renderer.as_ref().unwrap().line_height;
        let s = self.renderer.as_ref().unwrap().scale_factor;
        let shift = self.modifiers.shift_key();

        // Единая дельта как эталон для всех скролл-панелей в редакторе
        let (dx, dy) = match delta {
            MouseScrollDelta::LineDelta(x, y) => (-x * 4.0 * lh, -y * 4.0 * lh),
            MouseScrollDelta::PixelDelta(pos) => (-pos.x as f32, -pos.y as f32),
        };
        let mut consumed_by_diag = false;
        HOVER_STATE.with(|state| {
            let mut state = state.borrow_mut();
            if let Some(rect) = state.diag_rect {
                let mx = self.renderer.as_ref().unwrap().last_mouse_x;
                let my = self.renderer.as_ref().unwrap().last_mouse_y;
                if mx >= rect.0 && mx <= rect.0 + rect.2 && my >= rect.1 && my <= rect.1 + rect.3 {
                    state.diag_scroll.anim_speed = 7.0;
                    state.diag_scroll.scroll_by(dy);
                    let max_scroll = state.diag_max_scroll;
                    state.diag_scroll.clamp_target(0.0, max_scroll);
                    consumed_by_diag = true;
                }
            }
        });
        if consumed_by_diag {
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        let mut consumed_by_hover = false;
        HOVER_STATE.with(|state| {
            let mut state = state.borrow_mut();
            if let Some(rect) = state.rect {
                let mx = self.renderer.as_ref().unwrap().last_mouse_x;
                let my = self.renderer.as_ref().unwrap().last_mouse_y;
                if mx >= rect.0 && mx <= rect.0 + rect.2 && my >= rect.1 && my <= rect.1 + rect.3 {
                    let max_scroll = state.max_scroll;
                    if let Some(popup) = &mut state.popup {
                        popup.scroll.anim_speed = 7.0;
                        popup.scroll.scroll_by(dy);
                        popup.scroll.clamp_target(0.0, max_scroll);
                        consumed_by_hover = true;
                    }
                }
            }
        });
        if consumed_by_hover {
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        // Скролл в области проводника файлов — перехватываем до всего остального
        if self.is_ide_mode && self.ide_panel.is_open(crate::app::PanelId::Explorer) {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            let sb_w = 48.0 * s;
            let panel_left_w = self.ide_panel.left_width * s;
            let mx = self.renderer.as_ref().unwrap().last_mouse_x;
            let my = self.renderer.as_ref().unwrap().last_mouse_y;
            let title_h = 32.0 * s;
            let wh = self.window.as_ref().unwrap().inner_size().height as f32;
            let panel_bottom_h = if self.ide_panel.any_bottom_open() {
                self.ide_panel.bottom_height * s
            } else {
                0.0
            };
            let is_top = self.ide_panel.slots.iter().any(|sl| {
                sl.id == crate::app::PanelId::Explorer && sl.group == crate::app::PanelGroup::Top
            });

            let mut effective_bottom_h = panel_bottom_h;
            if self.ide_panel.is_open(crate::app::PanelId::Terminal)
                && !self.ide_panel.terminal_focused
            {
                effective_bottom_h = 0.0;
            }

            let (cx, cy, cw, ch) = if is_top {
                (
                    sb_w,
                    title_h,
                    panel_left_w,
                    wh - title_h - effective_bottom_h,
                )
            } else {
                let ww = self.window.as_ref().unwrap().inner_size().width as f32;
                let tab_h = 32.0 * s;
                (
                    sb_w,
                    wh - panel_bottom_h + 1.0 + tab_h,
                    ww - sb_w,
                    panel_bottom_h - 1.0 - tab_h,
                )
            };

            if mx >= cx && mx <= cx + cw && my >= cy && my <= cy + ch {
                self.ide_panel.explorer_scroll.anim_speed = 7.0;
                self.ide_panel.explorer_scroll.scroll_by(dy);
                let row_h = 28.0 * s;
                let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                let total_h = self.ide_panel.file_tree_nodes.len() as f32 * row_h;
                let max_scroll = (total_h - (wh - title_h)).max(0.0);
                self.ide_panel.explorer_scroll.clamp_target(0.0, max_scroll);
                self.window.as_ref().unwrap().request_redraw();
                return;
            }
        }

        if self.is_ide_mode && self.ide_panel.is_open(crate::app::PanelId::Problems) {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            let mx = self.renderer.as_ref().unwrap().last_mouse_x;
            let my = self.renderer.as_ref().unwrap().last_mouse_y;
            let wh = self.window.as_ref().unwrap().inner_size().height as f32;

            let is_top = self.ide_panel.slots.iter().any(|sl| {
                sl.id == crate::app::PanelId::Problems && sl.group == crate::app::PanelGroup::Top
            });
            let sb_w = 48.0 * s;
            let panel_bottom_h = if self.ide_panel.any_bottom_open() {
                self.ide_panel.bottom_height * s
            } else {
                0.0
            };

            let mut effective_bottom_h = panel_bottom_h;
            if self.ide_panel.is_open(crate::app::PanelId::Terminal)
                && !self.ide_panel.terminal_focused
            {
                effective_bottom_h = 0.0;
            }

            let (cx, cy, cw, ch) = if is_top {
                let panel_left_w = self.ide_panel.left_width * s;
                let title_h = 32.0 * s;
                (
                    sb_w,
                    title_h,
                    panel_left_w,
                    wh - title_h - effective_bottom_h,
                )
            } else {
                let ww = self.window.as_ref().unwrap().inner_size().width as f32;
                let tab_h = 32.0 * s;
                (
                    sb_w,
                    wh - panel_bottom_h + 1.0 + tab_h,
                    ww - sb_w,
                    panel_bottom_h - 1.0 - tab_h,
                )
            };

            if mx >= cx && mx <= cx + cw && my >= cy && my <= cy + ch {
                self.ide_panel.problems_scroll.anim_speed = 7.0;
                self.ide_panel.problems_scroll.scroll_by(dy);
                let row_h = 24.0 * s;
                let total_h = self.ide_panel.flat_diags.len() as f32 * row_h;
                let track_h = ch - 40.0 * s;
                let max_scroll = (total_h - track_h).max(0.0);
                self.ide_panel.problems_scroll.clamp_target(0.0, max_scroll);
                self.window.as_ref().unwrap().request_redraw();
                return;
            }
        }

        if self.is_ide_mode && self.ide_panel.is_open(crate::app::PanelId::Terminal) {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            let mx = self.renderer.as_ref().unwrap().last_mouse_x;
            let my = self.renderer.as_ref().unwrap().last_mouse_y;
            let wh = self.window.as_ref().unwrap().inner_size().height as f32;

            let is_top = self.ide_panel.slots.iter().any(|sl| {
                sl.id == crate::app::PanelId::Terminal && sl.group == crate::app::PanelGroup::Top
            });
            let sb_w = 48.0 * s;
            let panel_bottom_h = if self.ide_panel.any_bottom_open() {
                self.ide_panel.bottom_height * s
            } else {
                0.0
            };

            let mut effective_bottom_h = panel_bottom_h;
            if self.ide_panel.is_open(crate::app::PanelId::Terminal)
                && !self.ide_panel.terminal_focused
            {
                effective_bottom_h = 0.0;
            }

            let (cx, cy, cw, ch) = if is_top {
                let panel_left_w = self.ide_panel.left_width * s;
                let title_h = 32.0 * s;
                (
                    sb_w,
                    title_h,
                    panel_left_w,
                    wh - title_h - effective_bottom_h,
                )
            } else {
                let ww = self.window.as_ref().unwrap().inner_size().width as f32;
                let tab_h = 32.0 * s;
                (
                    sb_w,
                    wh - panel_bottom_h + 1.0 + tab_h,
                    ww - sb_w,
                    panel_bottom_h - 1.0 - tab_h,
                )
            };

            if mx >= cx && mx <= cx + cw && my >= cy && my <= cy + ch {
                if self.ide_panel.terminal_focused {
                    let active = self.ide_panel.active_terminal;
                    if let Some(term) = self.ide_panel.terminals.get_mut(active) {
                        let grid = term.grid.lock().unwrap();
                        let is_alt = grid.is_alt;
                        let app_cursor = grid.app_cursor_keys;
                        let mouse_tracking = grid.mouse_tracking;
                        let total_lines = grid.scrollback.len() + grid.lines.len();
                        drop(grid);

                        if is_alt {
                            if let Ok(mut w) = term.writer.lock() {
                                if mouse_tracking {
                                    let btn = if dy < 0.0 { 64 } else { 65 };
                                    let seq = format!("\x1b[<{};1;1M", btn);
                                    let steps = (dy.abs() / 20.0).max(1.0) as usize;
                                    for _ in 0..steps.min(3) {
                                        let _ = w.write_all(seq.as_bytes());
                                    }
                                } else {
                                    let seq = if dy < 0.0 {
                                        if app_cursor {
                                            b"\x1BOA"
                                        } else {
                                            b"\x1B[A"
                                        }
                                    } else {
                                        if app_cursor {
                                            b"\x1BOB"
                                        } else {
                                            b"\x1B[B"
                                        }
                                    };
                                    let steps = (dy.abs() / 20.0).max(1.0) as usize;
                                    for _ in 0..steps.min(3) {
                                        let _ = w.write_all(seq);
                                    }
                                }
                                let _ = w.flush();
                            }
                            return;
                        }

                        term.scroll_y.anim_speed = 7.0;
                        term.scroll_y.scroll_by(-dy); // -dy because scroll_y=0 is bottom

                        let lh = self.renderer.as_ref().unwrap().line_height;
                        let term_scale = 1.05;
                        let char_h = lh * term_scale;

                        let term_content_h = ch - 32.0 * s;
                        let max_scroll = ((total_lines as f32 * char_h) - term_content_h).max(0.0);

                        term.scroll_y.clamp_target(0.0, max_scroll);
                        self.window.as_ref().unwrap().request_redraw();
                    }
                    return;
                }
            }
        }

        if self.is_ide_mode && self.ide_panel.is_open(crate::app::PanelId::LspServers) {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            let mx = self.renderer.as_ref().unwrap().last_mouse_x;
            let my = self.renderer.as_ref().unwrap().last_mouse_y;
            if let Some((cx, cy, cw, ch)) = self.lsp_panel_bounds() {
                if mx >= cx && mx <= cx + cw && my >= cy && my <= cy + ch {
                    let mut over_inner = false;
                    let scroll_y = self.ide_panel.lsp_scroll_y.current.round();
                    let mut current_y = cy + 8.0 * s - scroll_y;
                    for info in &self.ide_panel.lsp_servers {
                        let is_expanded = self.ide_panel.lsp_logs_expanded.contains(info.name);
                        let logs_h = self.lsp_server_logs_h(info, s);
                        let row_h = 136.0 * s + logs_h;

                        if is_expanded {
                            let btn_y1 = current_y + 56.0 * s;
                            let btn_h = 24.0 * s;
                            let btn_y2 = btn_y1 + btn_h + 8.0 * s;
                            let log_bg_y = btn_y2 + btn_h + 10.0 * s;
                            let log_bg_x = cx + 24.0 * s;
                            let log_bg_w = cw - 48.0 * s;
                            let log_bg_h = logs_h - 18.0 * s;

                            if mx >= log_bg_x
                                && mx <= log_bg_x + log_bg_w
                                && my >= log_bg_y
                                && my <= log_bg_y + log_bg_h
                            {
                                let (inner_total_h, inner_max_w) =
                                    self.lsp_server_inner_size(info, s);
                                let name = info.name.to_string();

                                let inner_y = self
                                    .ide_panel
                                    .lsp_logs_scroll_y
                                    .entry(name.clone())
                                    .or_insert_with(|| crate::scroll::ScrollState::new(7.0));
                                inner_y.anim_speed = 7.0;
                                if !shift {
                                    inner_y.scroll_by(dy);
                                }
                                inner_y.clamp_target(0.0, (inner_total_h - log_bg_h).max(0.0));

                                let inner_x = self
                                    .ide_panel
                                    .lsp_logs_scroll_x
                                    .entry(name)
                                    .or_insert_with(|| crate::scroll::ScrollState::new(7.0));
                                inner_x.anim_speed = 7.0;
                                if shift {
                                    inner_x.scroll_by(dy);
                                } else {
                                    inner_x.scroll_by(dx);
                                }
                                inner_x.clamp_target(
                                    0.0,
                                    (inner_max_w + 20.0 * s - log_bg_w).max(0.0),
                                );

                                over_inner = true;
                                break;
                            }
                        }
                        current_y += row_h + 16.0 * s;
                    }

                    if !over_inner {
                        self.ide_panel.lsp_scroll_y.anim_speed = 7.0;
                        self.ide_panel.lsp_scroll_x.anim_speed = 7.0;
                        if shift {
                            self.ide_panel.lsp_scroll_x.scroll_by(dy);
                        } else {
                            self.ide_panel.lsp_scroll_y.scroll_by(dy);
                            self.ide_panel.lsp_scroll_x.scroll_by(dx);
                        }
                        let total_h = self.lsp_panel_total_h(s);
                        self.ide_panel
                            .lsp_scroll_y
                            .clamp_target(0.0, (total_h - ch).max(0.0));
                        self.ide_panel.lsp_scroll_x.clamp_target(0.0, 0.0);
                    }

                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
            }
        }

        if let (true, Some((rx, ry, rw, rh))) = (self.autocomplete_active, self.autocomplete_rect) {
            let mx = self.renderer.as_ref().unwrap().last_mouse_x;
            let my = self.renderer.as_ref().unwrap().last_mouse_y;
            if mx >= rx && mx <= rx + rw && my >= ry && my <= ry + rh {
                self.autocomplete_scroll.anim_speed = 7.0;
                self.autocomplete_scroll.scroll_by(dy);
                let step = 36.0 * s;
                let total_items = self.autocomplete_options.len() as f32;
                let visible_items = total_items.min(7.0);
                let max_scroll = ((total_items - visible_items) * step).max(0.0);
                self.autocomplete_scroll.clamp_target(0.0, max_scroll);
                self.window.as_ref().unwrap().request_redraw();
                return;
            }
        }

        if self.show_settings && self.settings_tab == 0 {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            let h = self.window.as_ref().unwrap().inner_size().height as f32;
            let ide_h = (700.0 * s).min(h - 40.0 * s);
            let ih = ide_h - 35.0 * s - 30.0 * s;
            let ide_content_area_h = ih - 52.0 * s;

            // Точный подсчёт высоты контента (как в draw_settings)
            let workspace_h = self.ide_workspaces.len() as f32 * 46.0 * s + 126.0 * s;
            let chip_h = 28.0 * s;
            let chip_gap_y = 8.0 * s;
            let chip_gap_x = 8.0 * s;
            let pad_x = 12.0 * s;
            let max_row_w = 460.0 * s;
            let chip_rows = if self.ide_ignore_patterns.is_empty() {
                1usize
            } else {
                let mut rows = 1usize;
                let mut cx = 0.0f32;
                for p in &self.ide_ignore_patterns {
                    let tw = self.renderer.as_mut().unwrap().measure_ui_width(p, 0.88);
                    let cw = tw + pad_x * 2.0 + 22.0 * s;
                    if cx + cw > max_row_w && cx > 0.0 {
                        rows += 1;
                        cx = 0.0;
                    }
                    cx += cw + chip_gap_x;
                }
                rows
            };
            let ignore_h = 200.0 * s + chip_rows as f32 * (chip_h + chip_gap_y);
            let ide_total_h = workspace_h + ignore_h;
            let max_scroll = (ide_total_h - ide_content_area_h).max(0.0);

            if max_scroll > 0.0 {
                self.settings_ide_scroll.anim_speed = 7.0;
                self.settings_ide_scroll.scroll_by(dy);
                self.settings_ide_scroll.clamp_target(0.0, max_scroll);
                self.window.as_ref().unwrap().request_redraw();
            }
            return;
        }
        if self.show_settings && self.settings_tab == 4 {
            self.settings_scroll.anim_speed = 7.0;
            self.settings_scroll.scroll_by(dy);
            let box_h = (700.0 * s)
                .min(self.window.as_ref().unwrap().inner_size().height as f32 - 40.0 * s);
            let max_scroll = self
                .renderer
                .as_mut()
                .unwrap()
                .get_faq_max_scroll(&self.faq_editor, box_h);
            self.settings_scroll.clamp_target(0.0, max_scroll);
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        if self.show_welcome || self.show_settings || self.dialog_window.is_some() {
            return;
        }

        let s = self.renderer.as_ref().unwrap().scale_factor;
        let tab_bar_h = if self.show_welcome || !self.is_ide_mode {
            0.0
        } else {
            38.0 * s
        };

        let my = self.renderer.as_ref().unwrap().last_mouse_y;
        if my >= 0.0 && my <= tab_bar_h && !self.tabs.is_empty() {
            self.tab_scroll.anim_speed = 7.0;
            self.tab_scroll.scroll_by(dy);
            let max_scroll = self.renderer.as_ref().unwrap().max_tab_scroll_x;
            self.tab_scroll.clamp_target(0.0, max_scroll);
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        self.scroll_y.anim_speed = 7.0;
        self.scroll_x.anim_speed = 7.0;

        // При скролле основного редактора hover-popup с типом должен скрываться,
        // так же как исчезает popup с диагностикой.
        clear_hover_popup(self.renderer.as_mut());

        if shift {
            self.scroll_x.scroll_by(dy); // Shift конвертирует вертикальный скролл в горизонтальный
        } else {
            self.scroll_y.scroll_by(dy);
            self.scroll_x.scroll_by(dx);
        }

        let wh = self.window.as_ref().unwrap().inner_size().height as f32;
        let s = self.renderer.as_ref().unwrap().scale_factor;
        let tab_bar_h = if self.show_welcome || !self.is_ide_mode {
            0.0
        } else {
            38.0 * s
        };
        let max_scroll_y = self
            .renderer
            .as_mut()
            .unwrap()
            .get_max_scroll(&self.editor, wh - tab_bar_h);
        let max_scroll_x = self.renderer.as_ref().unwrap().max_scroll_x;

        self.scroll_y.clamp_target(0.0, max_scroll_y);
        self.scroll_y.target = self.scroll_y.target.round();
        self.scroll_x.clamp_target(0.0, max_scroll_x);
        self.scroll_x.target = self.scroll_x.target.round();
        self.window.as_ref().unwrap().request_redraw();
    }
}
