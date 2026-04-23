use crate::app::App;
use std::io::Write;
use std::time::Instant;
use winit::event::{ElementState, MouseScrollDelta};
use winit::event_loop::ActiveEventLoop;

#[derive(Debug, Clone)]
pub struct HoverPopup {
    pub text: String,
    pub spans: Vec<crate::highlighter::ColorSpan>,
    pub byte_offset: usize,
    pub anchor_x: f32,
    pub scroll: crate::scroll::ScrollState,
}

pub struct HoverState {
    pub request_id: Option<i32>,
    pub popup: Option<HoverPopup>,
    pub timer: f32,
    pub byte_offset: Option<usize>,
    pub rect: Option<(f32, f32, f32, f32)>,
    pub max_scroll: f32,
}

impl Default for HoverState {
    fn default() -> Self {
        Self {
            request_id: None,
            popup: None,
            timer: 0.0,
            byte_offset: None,
            rect: None,
            max_scroll: 0.0,
        }
    }
}

thread_local! {
    pub static HOVER_STATE: std::cell::RefCell<HoverState> = std::cell::RefCell::new(HoverState::default());
}

pub fn clear_hover_popup() -> bool {
    HOVER_STATE.with(|state| {
        let mut state = state.borrow_mut();
        let had_popup = state.popup.is_some()
            || state.request_id.is_some()
            || state.byte_offset.is_some()
            || state.rect.is_some();
        state.request_id = None;
        state.popup = None;
        state.timer = 0.0;
        state.byte_offset = None;
        state.rect = None;
        state.max_scroll = 0.0;
        had_popup
    })
}

fn is_hover_target_byte(editor: &crate::editor::Editor, byte_offset: usize) -> bool {
    if byte_offset >= editor.len() {
        return false;
    }
    let b = editor.byte_at(byte_offset);
    matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_') || b >= 0x80
}

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
        let mut consumed_by_hover = false;
        HOVER_STATE.with(|state| {
            let mut state = state.borrow_mut();
            if let Some(rect) = state.rect {
                let mx = self.renderer.as_ref().unwrap().last_mouse_x;
                let my = self.renderer.as_ref().unwrap().last_mouse_y;
                let pad = 40.0 * s;
                if mx >= rect.0 - pad
                    && mx <= rect.0 + rect.2 + pad
                    && my >= rect.1 - pad
                    && my <= rect.1 + rect.3 + pad
                {
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

    pub fn handle_main_mouse_input(
        &mut self,
        _event_loop: &ActiveEventLoop,
        state: ElementState,
        button: winit::event::MouseButton,
    ) {
        let mx = self.renderer.as_ref().unwrap().last_mouse_x;
        let my = self.renderer.as_ref().unwrap().last_mouse_y;
        if state == ElementState::Pressed && button == winit::event::MouseButton::Left {
            let mut in_hover_popup = false;
            let s = self.renderer.as_ref().unwrap().scale_factor;
            HOVER_STATE.with(|hover_state| {
                if let Some((x, y, w, h)) = hover_state.borrow().rect {
                    let pad = 40.0 * s;
                    if mx >= x - pad && mx <= x + w + pad && my >= y - pad && my <= y + h + pad {
                        in_hover_popup = true;
                    }
                }
            });
            if !in_hover_popup {
                clear_hover_popup();
            }
        }

        if self.is_ide_mode
            && self.ide_panel.is_open(crate::app::PanelId::Terminal)
            && self.ide_panel.terminal_focused
        {
            if let Some(crate::ui_system::UiId::TerminalBody) = self.ui_registry.find_at(mx, my) {
                let active = self.ide_panel.active_terminal;
                let mut tracking = false;
                if let Some(term) = self.ide_panel.terminals.get_mut(active) {
                    if term.grid.lock().unwrap().mouse_tracking {
                        tracking = true;
                    }
                }
                if tracking {
                    let btn_code = match button {
                        winit::event::MouseButton::Left => 0,
                        winit::event::MouseButton::Middle => 1,
                        winit::event::MouseButton::Right => 2,
                        _ => 0,
                    };
                    let is_pressed = state == ElementState::Pressed;
                    let s = self.renderer.as_ref().unwrap().scale_factor;
                    let panel_x = 48.0 * s + 10.0 * s;
                    let char_w = self.renderer.as_mut().unwrap().char_advance('A') * 1.05;
                    let char_h = self.renderer.as_ref().unwrap().line_height * 1.05;
                    let bottom_h = self.ide_panel.bottom_height * s;
                    let tab_h = 32.0 * s;
                    let content_y = self.window.as_ref().unwrap().inner_size().height as f32
                        - bottom_h
                        + 1.0
                        + tab_h;
                    let content_h = bottom_h - 1.0 - tab_h;
                    let term_content_y = content_y + 32.0 * s;
                    let term_content_h = content_h - 32.0 * s;

                    let mut cell_x = ((mx - panel_x).max(0.0) / char_w).floor() as usize;
                    cell_x += 1;

                    let mut is_drag = false;
                    let mut cell_y = 1;
                    if let Some(term) = self.ide_panel.terminals.get_mut(active) {
                        let mut grid = term.grid.lock().unwrap();
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
                        let offset_from_bottom = (term_content_y + term_content_h - 8.0 * s - my
                            + scroll_offset)
                            / char_h;
                        let visible_row_0_based = grid
                            .visible_rows
                            .saturating_sub(1)
                            .saturating_sub(offset_from_bottom.max(0.0).floor() as usize);
                        cell_y = visible_row_0_based + 1;

                        if is_pressed {
                            grid.selection = None;
                        } else if let Some((sx, sy, ex, ey)) = grid.selection {
                            if sx != ex || sy != ey {
                                is_drag = true;
                            }
                        }
                    }

                    if !is_drag {
                        let end_char = if is_pressed { 'M' } else { 'm' };
                        let seq = format!("\x1b[<{};{};{}{}", btn_code, cell_x, cell_y, end_char);
                        if let Some(term) = self.ide_panel.terminals.get_mut(active) {
                            if let Ok(mut w) = term.writer.lock() {
                                let _ = w.write_all(seq.as_bytes());
                                let _ = w.flush();
                            }
                        }
                    }
                }
            }
        }

        if state == ElementState::Pressed && button == winit::event::MouseButton::Left {
            if let Some(menu) = self.lsp_actions_menu.as_ref() {
                let s = self.renderer.as_ref().unwrap().scale_factor;
                let mut clicked_inside = false;
                if state == ElementState::Pressed {
                    let item_h = 36.0 * s;
                    let menu_w = 320.0 * s;
                    let menu_h = menu.items.len() as f32 * item_h + 8.0 * s;
                    let tab_bar_h = if self.show_welcome || !self.is_ide_mode {
                        0.0
                    } else {
                        38.0 * s
                    };
                    let menu_y = menu.menu_y + tab_bar_h;
                    if mx >= menu.menu_x
                        && mx <= menu.menu_x + menu_w
                        && my >= menu_y
                        && my <= menu_y + menu_h
                    {
                        clicked_inside = true;
                        let rel_y = my - menu_y - 4.0 * s;
                        let idx = (rel_y / item_h) as usize;
                        if idx < menu.items.len() {
                            let menu_clone = self.lsp_actions_menu.take().unwrap();
                            let item = menu_clone.items[idx].clone();
                            let cursor_line = menu_clone.cursor_line;
                            drop(menu_clone);
                            match item {
                                crate::app::LspActionItem::CodeAction(action) => {
                                    if let Some(edit) = action.edit {
                                        self.apply_workspace_edit(&edit, false);
                                    }
                                }
                                crate::app::LspActionItem::AddNoqa { codes } => {
                                    self.insert_noqa_comment(cursor_line, &codes);
                                }
                                crate::app::LspActionItem::AddNoqaAll => {
                                    self.insert_noqa_comment(cursor_line, &[]);
                                }
                                crate::app::LspActionItem::FixAll => {
                                    if let Some(lsp) = &mut self.lsp {
                                        if let Some(path) = self.file_path.clone() {
                                            if let Some(id) =
                                                lsp.request_fix_all(&path, &self.file_extension)
                                            {
                                                self.pending_fix_all_id = Some(id);
                                            }
                                        }
                                    }
                                }
                                crate::app::LspActionItem::OrganizeImports => {
                                    if let Some(lsp) = &mut self.lsp {
                                        if let Some(path) = self.file_path.clone() {
                                            if let Some(id) = lsp.request_organize_imports(
                                                &path,
                                                &self.file_extension,
                                            ) {
                                                self.pending_fix_all_id = Some(id);
                                            }
                                        }
                                    }
                                }
                            }
                            self.window.as_ref().unwrap().request_redraw();
                            return;
                        }
                    }
                }

                if !clicked_inside {
                    self.lsp_actions_menu = None;
                    self.window.as_ref().unwrap().request_redraw();
                } else {
                    return;
                }
            }

            // Глобальная обработка декларативного UI
            if !self.show_settings && self.dialog_window.is_none() {
                if self.is_ide_mode && self.ide_panel.is_open(crate::app::PanelId::Problems) {
                    let s = self.renderer.as_ref().unwrap().scale_factor;
                    let wh = self.window.as_ref().unwrap().inner_size().height as f32;

                    let is_top = self.ide_panel.slots.iter().any(|sl| {
                        sl.id == crate::app::PanelId::Problems
                            && sl.group == crate::app::PanelGroup::Top
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
                        let scroll_x = cx + cw - 12.0 * s;
                        if mx >= scroll_x {
                            let item_h = 24.0 * s;
                            let total_h = self.ide_panel.flat_diags.len() as f32 * item_h;
                            let track_h = ch - 40.0 * s;
                            if total_h > track_h {
                                let max_scroll = total_h - track_h;
                                let scroll_ratio = (self.ide_panel.problems_scroll.current
                                    / max_scroll)
                                    .clamp(0.0, 1.0);
                                let thumb_h = (track_h / total_h * track_h).max(20.0 * s);
                                let list_y = cy + 40.0 * s;
                                let thumb_y = list_y + scroll_ratio * (track_h - thumb_h);

                                if my >= thumb_y && my <= thumb_y + thumb_h {
                                    self.ide_panel.problems_scroll.is_dragging = true;
                                    self.ide_panel.problems_scroll.drag_offset = my - thumb_y;
                                    return;
                                } else if my >= list_y && my <= list_y + track_h {
                                    self.ide_panel.problems_scroll.anim_speed = 15.0;
                                    self.ide_panel.problems_scroll.drag_offset = thumb_h / 2.0;
                                    let new_ratio = (my - list_y - thumb_h / 2.0)
                                        / (track_h - thumb_h).max(1.0);
                                    self.ide_panel.problems_scroll.target =
                                        (new_ratio * max_scroll).clamp(0.0, max_scroll);
                                    self.ide_panel.problems_scroll.is_dragging = true;
                                    self.window.as_ref().unwrap().request_redraw();
                                    return;
                                }
                            }
                        }
                    }
                }

                if self.is_ide_mode {
                    let s = self.renderer.as_ref().unwrap().scale_factor;
                    let sb_w = 48.0 * s;
                    let panel_left_w = self.ide_panel.left_width * s;
                    let panel_bottom_h = if self.ide_panel.any_bottom_open() {
                        self.ide_panel.bottom_height * s
                    } else {
                        0.0
                    };
                    let wh = self.window.as_ref().unwrap().inner_size().height as f32;

                    let mut effective_bottom_h = panel_bottom_h;
                    if self.ide_panel.is_open(crate::app::PanelId::Terminal)
                        && !self.ide_panel.terminal_focused
                    {
                        effective_bottom_h = 0.0;
                    }

                    let mut manual_resize = false;
                    if panel_left_w > 0.0 {
                        let resize_x = sb_w + panel_left_w;
                        if (mx - resize_x).abs() < 6.0 * s
                            && my >= 0.0
                            && my < wh - effective_bottom_h
                        {
                            self.ide_panel.is_resizing_left = true;
                            manual_resize = true;
                        }
                    }
                    if panel_bottom_h > 0.0 && !manual_resize {
                        let resize_y = wh - panel_bottom_h;
                        if (my - resize_y).abs() < 6.0 * s && mx >= sb_w {
                            self.ide_panel.is_resizing_bottom = true;
                            manual_resize = true;
                        }
                    }

                    if manual_resize {
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                }

                if let Some(clicked_id) = self.ui_registry.find_at(mx, my) {
                    let in_hover_popup_body = clicked_id == crate::ui_system::UiId::BottomPanelBody
                        && HOVER_STATE.with(|hover_state| {
                            if let Some((x, y, w, h)) = hover_state.borrow().rect {
                                mx >= x && mx <= x + w && my >= y && my <= y + h
                            } else {
                                false
                            }
                        });
                    if in_hover_popup_body {
                        // Не блокируем выделение текста: клики по hover popup должны
                        // проходить в редактор, как и по обычному текстовому слою.
                    } else if clicked_id == crate::ui_system::UiId::BottomPanelBody {
                        self.handle_ui_click(clicked_id);
                        return;
                    }
                    if in_hover_popup_body {
                        // Пропускаем обработку UI-элемента и даём нижележащему
                        // editor-механизму обработать клик/drag-selection.
                    } else
                    if clicked_id == crate::ui_system::UiId::HoverPopupScroll {
                        let s = self.renderer.as_ref().unwrap().scale_factor;
                        crate::app::mouse::HOVER_STATE.with(|hover_state| {
                            let mut state = hover_state.borrow_mut();
                            if let Some(rect) = state.rect {
                                let max_scroll = state.max_scroll;
                                if let Some(popup) = &mut state.popup {
                                    let (_, by, _, box_h) = rect;
                                    let track_h = box_h - 16.0 * s;
                                    let thumb_h =
                                        (box_h / (box_h + max_scroll) * track_h).max(20.0 * s);
                                    let thumb_y = by
                                        + 8.0 * s
                                        + (popup.scroll.current / max_scroll) * (track_h - thumb_h);
                                    if my >= thumb_y && my <= thumb_y + thumb_h {
                                        popup.scroll.is_dragging = true;
                                        popup.scroll.drag_offset = my - thumb_y;
                                    } else {
                                        popup.scroll.anim_speed = 15.0;
                                        popup.scroll.drag_offset = thumb_h / 2.0;
                                        let ratio = (my - by - 8.0 * s - popup.scroll.drag_offset)
                                            / (track_h - thumb_h).max(0.0001);
                                        popup.scroll.target =
                                            (ratio * max_scroll).clamp(0.0, max_scroll);
                                        popup.scroll.is_dragging = true;
                                    }
                                }
                            }
                        });
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                    if in_hover_popup_body {
                        // no-op
                    } else {
                    let is_term = matches!(
                        clicked_id,
                        crate::ui_system::UiId::TerminalBody
                            | crate::ui_system::UiId::TerminalScrollY
                            | crate::ui_system::UiId::TerminalTab(_)
                            | crate::ui_system::UiId::TerminalTabClose(_)
                            | crate::ui_system::UiId::TerminalAdd
                            | crate::ui_system::UiId::TerminalSearchInput
                            | crate::ui_system::UiId::TerminalSearchClose
                            | crate::ui_system::UiId::TerminalSearchNext
                            | crate::ui_system::UiId::TerminalSearchPrev
                            | crate::ui_system::UiId::TerminalSearchCaseToggle
                    );
                    let is_resize = matches!(
                        clicked_id,
                        crate::ui_system::UiId::ResizeLeft | crate::ui_system::UiId::ResizeBottom
                    );

                    if is_term {
                        self.ide_panel.terminal_focused = true;
                    } else if !is_resize {
                        self.ide_panel.terminal_focused = false;
                    }

                    if let crate::ui_system::UiId::SidebarSlot(panel_id) = clicked_id {
                        self.ide_panel.drag = Some(crate::app::PanelDragState {
                            panel_id,
                            start_y: my,
                            current_y: my,
                            threshold_passed: false,
                        });
                    } else if let crate::ui_system::UiId::EditorTab(idx) = clicked_id {
                        self.ide_panel.tab_drag = Some(crate::app::TabDragState {
                            start_idx: idx,
                            start_x: mx,
                            current_x: mx,
                            threshold_passed: false,
                        });
                        self.handle_ui_click(clicked_id);
                    } else {
                        if clicked_id == crate::ui_system::UiId::TerminalBody {
                            self.ide_panel.is_dragging_terminal = true;
                        } else {
                            self.ide_panel.is_dragging_terminal = false;
                        }
                        self.handle_ui_click(clicked_id);
                    }
                    return;
                    }
                }
            }
        }

        // Clicks routed through UI system

        if self.dialog_window.is_some() {
            if state == ElementState::Pressed {
                if let Some(dw) = self.dialog_window.as_ref() {
                    dw.focus_window();
                    dw.request_redraw();
                }
            }
            return;
        }

        if self.show_settings {
            if state == ElementState::Released {
                self.is_dragging_settings_ignore = false;
                self.is_dragging_lsp_log = false;
            } else if state == ElementState::Pressed {
                let s = self.renderer.as_ref().unwrap().scale_factor;
                let w = (1000.0 * s)
                    .min(self.window.as_ref().unwrap().inner_size().width as f32 - 40.0 * s);
                let h = (700.0 * s)
                    .min(self.window.as_ref().unwrap().inner_size().height as f32 - 40.0 * s);
                let x = (self.window.as_ref().unwrap().inner_size().width as f32 - w) / 2.0;
                let y = (self.window.as_ref().unwrap().inner_size().height as f32 - h) / 2.0;

                let mx = self.renderer.as_ref().unwrap().last_mouse_x;
                let my = self.renderer.as_ref().unwrap().last_mouse_y;

                if mx < x || mx > x + w || my < y || my > y + h {
                    self.show_settings = false;
                } else {
                    // Ищем только среди оверлейных элементов настроек,
                    // чтобы фоновые элементы редактора не реагировали на клики.
                    if let Some(clicked_id) = self.ui_registry.find_overlay_at(mx, my) {
                        match clicked_id {
                            crate::ui_system::UiId::SettingsIdeIgnoreInput => {
                                // Специальная обработка: позиционирование курсора по клику
                                self.settings_ignore_focused = true;
                                self.is_dragging_settings_ignore = true;
                                let s = self.renderer.as_ref().unwrap().scale_factor;
                                let pad_h = 40.0 * s;
                                let sidebar_w = 200.0 * s;
                                let ix = x + pad_h;
                                let content_x = ix + sidebar_w + 30.0 * s;
                                let text = self.settings_ignore_editor.get_full_text();
                                let start_x = content_x + 8.0 * s;
                                let x_offset =
                                    (mx - start_x + self.settings_ignore_scroll_x).max(0.0);
                                let mut current_x = 0.0;
                                let mut target_idx = text.len();
                                let mut byte_idx = 0;
                                for c in text.chars() {
                                    let adv = self
                                        .renderer
                                        .as_mut()
                                        .unwrap()
                                        .get_ui_glyph(c)
                                        .map(|g| g.advance)
                                        .unwrap_or(10.0)
                                        * 0.95;
                                    if x_offset <= current_x + adv / 2.0 {
                                        target_idx = byte_idx;
                                        break;
                                    }
                                    current_x += adv;
                                    byte_idx += c.len_utf8();
                                }
                                self.settings_ignore_editor.cursor = target_idx;
                                self.settings_ignore_editor.selection_anchor = Some(target_idx);
                            }
                            other => {
                                // Снимаем фокус с поля ввода при клике в другое место
                                self.settings_ignore_focused = false;
                                self.handle_ui_click(other);
                            }
                        }
                    } else {
                        // Клик мимо любого элемента — снимаем фокус
                        self.settings_ignore_focused = false;
                    }
                }
            }
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        if state == ElementState::Released {
            // Завершаем DnD и ресайз IDE-панелей
            if self.is_ide_mode {
                if let Some(drag) = self.ide_panel.tab_drag.take() {
                    if drag.threshold_passed && self.tabs.len() > 1 {
                        let s = self.renderer.as_ref().unwrap().scale_factor;
                        let tab_pad = 16.0 * s;
                        let icon_size_tab = 20.0 * s;

                        let start_cx = if self.is_ide_mode {
                            let panel_left_w = self.ide_panel.left_width * s;
                            (48.0 * s + panel_left_w).round() + 1.0 - self.tab_scroll.current
                        } else {
                            -self.tab_scroll.current
                        };

                        let mut paths: Vec<Option<&std::path::PathBuf>> =
                            self.tabs.iter().map(|t| t.file_path.as_ref()).collect();
                        paths[self.active_tab] = self.file_path.as_ref();

                        let mut display_titles = vec![String::new(); self.tabs.len()];
                        for i in 0..self.tabs.len() {
                            if let Some(p1) = paths[i] {
                                let mut diff_level = 0;
                                let mut collision = false;
                                for j in 0..self.tabs.len() {
                                    if i == j {
                                        continue;
                                    }
                                    if let Some(p2) = paths[j] {
                                        if p1.file_name() == p2.file_name() {
                                            collision = true;
                                            let mut it1 = p1.components().rev();
                                            let mut it2 = p2.components().rev();
                                            let mut level = 0;
                                            loop {
                                                let c1 = it1.next();
                                                let c2 = it2.next();
                                                if c1 != c2 {
                                                    diff_level = diff_level.max(level);
                                                    break;
                                                }
                                                if c1.is_none() && c2.is_none() {
                                                    break;
                                                }
                                                level += 1;
                                            }
                                        }
                                    }
                                }
                                if collision && diff_level > 0 {
                                    let comps: Vec<_> = p1.components().rev().collect();
                                    if diff_level < comps.len() {
                                        let diff_dir =
                                            comps[diff_level].as_os_str().to_string_lossy();
                                        let file_name = comps[0].as_os_str().to_string_lossy();
                                        if diff_level == 1 {
                                            display_titles[i] =
                                                format!("{}/{}", diff_dir, file_name);
                                        } else {
                                            display_titles[i] =
                                                format!("{}/.../{}", diff_dir, file_name);
                                        }
                                    } else {
                                        display_titles[i] = p1.to_string_lossy().into_owned();
                                    }
                                } else {
                                    display_titles[i] = p1
                                        .file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .into_owned();
                                }
                            } else {
                                let bt = if i == self.active_tab {
                                    &self.base_title
                                } else {
                                    &self.tabs[i].base_title
                                };
                                display_titles[i] = if bt.is_empty() {
                                    "Безымянный".to_string()
                                } else {
                                    bt.to_string()
                                };
                            }
                        }

                        let mut widths = Vec::new();
                        for (i, _tab) in self.tabs.iter().enumerate() {
                            let title = &display_titles[i];
                            let title_w =
                                self.renderer.as_mut().unwrap().measure_ui_width(title, 1.0);
                            let tab_w =
                                tab_pad * 2.0 + icon_size_tab + 8.0 * s + title_w + 30.0 * s;
                            widths.push(tab_w);
                        }

                        let mut initial_xs = vec![0.0; self.tabs.len()];
                        let mut cx = start_cx;
                        for i in 0..self.tabs.len() {
                            initial_xs[i] = cx;
                            cx += widths[i];
                        }

                        let dragged_x =
                            initial_xs[drag.start_idx] + (drag.current_x - drag.start_x);
                        let dragged_w = widths[drag.start_idx];

                        let mut new_idx = drag.start_idx;
                        let dragged_center = dragged_x + dragged_w / 2.0;

                        for i in 0..self.tabs.len() {
                            if i == drag.start_idx {
                                continue;
                            }
                            let other_center = initial_xs[i] + widths[i] / 2.0;

                            if i < drag.start_idx {
                                if dragged_center < other_center {
                                    new_idx = new_idx.min(i);
                                }
                            } else {
                                if dragged_center > other_center {
                                    new_idx = new_idx.max(i);
                                }
                            }
                        }

                        if new_idx != drag.start_idx {
                            self.sync_active_tab();
                            let tab = self.tabs.remove(drag.start_idx);
                            self.tabs.insert(new_idx, tab);

                            if self.active_tab == drag.start_idx {
                                self.active_tab = new_idx;
                            } else if self.active_tab > drag.start_idx && self.active_tab <= new_idx
                            {
                                self.active_tab -= 1;
                            } else if self.active_tab < drag.start_idx && self.active_tab >= new_idx
                            {
                                self.active_tab += 1;
                            }
                            self.sync_active_tab();
                            self.save_tabs_state();
                        }
                    }
                }
                if let Some(drag) = self.ide_panel.drag.take() {
                    if !drag.threshold_passed {
                        // Клик без движения → переключить панель
                        let toggled_open = {
                            let slot = self
                                .ide_panel
                                .slots
                                .iter()
                                .find(|sl| sl.id == drag.panel_id);
                            slot.map(|s| !s.open).unwrap_or(false)
                        };
                        let toggled_group = {
                            let slot = self
                                .ide_panel
                                .slots
                                .iter()
                                .find(|sl| sl.id == drag.panel_id);
                            slot.map(|s| s.group.clone())
                        };
                        self.ide_panel.toggle(drag.panel_id);
                        // При открытии Explorer — запускаем скан файлов
                        if toggled_open && drag.panel_id == crate::app::PanelId::Explorer {
                            self.refresh_file_tree();
                        }
                        // Взаимоисключение: при открытии кнопки закрываем остальные в той же группе
                        if toggled_open {
                            if let Some(group) = toggled_group {
                                for sl in self.ide_panel.slots.iter_mut() {
                                    if sl.id != drag.panel_id && sl.group == group {
                                        sl.open = false;
                                    }
                                }
                            }
                        }
                        // Clamp scroll_y к новому max_scroll после изменения высоты панелей
                        let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                        let max_scroll = self
                            .renderer
                            .as_mut()
                            .unwrap()
                            .get_max_scroll(&self.editor, wh);
                        self.scroll_y.clamp_target(0.0, max_scroll);
                        self.scroll_y.clamp_current(0.0, max_scroll);
                    } else {
                        // DnD завершён — определяем новую группу по позиции и сортируем
                        let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                        let new_group = if drag.current_y < wh / 2.0 {
                            crate::app::PanelGroup::Top
                        } else {
                            crate::app::PanelGroup::Bottom
                        };

                        let s = self.renderer.as_ref().unwrap().scale_factor;
                        let btn_size = 48.0 * s;
                        let btn_gap = 0.0;
                        let top_start_y = 0.0;

                        let mut top_items = Vec::new();
                        let mut bottom_items = Vec::new();
                        let mut top_idx = 0;
                        let mut bottom_idx = 0;

                        // Назначаем виртуальные Y-координаты всем элементам для сортировки
                        for mut slot in self.ide_panel.slots.drain(..) {
                            if slot.id == drag.panel_id {
                                slot.group = new_group.clone();
                                if matches!(new_group, crate::app::PanelGroup::Top) {
                                    top_items.push((drag.current_y, slot));
                                } else {
                                    bottom_items.push((drag.current_y, slot));
                                }
                            } else {
                                if matches!(slot.group, crate::app::PanelGroup::Top) {
                                    let y = top_start_y + top_idx as f32 * (btn_size + btn_gap);
                                    top_items.push((y, slot));
                                    top_idx += 1;
                                } else {
                                    let y =
                                        wh - btn_size - bottom_idx as f32 * (btn_size + btn_gap);
                                    bottom_items.push((y, slot));
                                    bottom_idx += 1;
                                }
                            }
                        }

                        // Сортируем: для Top сверху вниз (по возрастанию Y), для Bottom снизу вверх (по убыванию Y)
                        top_items.sort_by(|a, b| {
                            a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)
                        });
                        bottom_items.sort_by(|a, b| {
                            b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal)
                        });

                        // Собираем массив обратно
                        self.ide_panel
                            .slots
                            .extend(top_items.into_iter().map(|(_, s)| s));
                        self.ide_panel
                            .slots
                            .extend(bottom_items.into_iter().map(|(_, s)| s));
                    }
                    crate::save_panel_state(&self.ide_panel);
                }
                if self.ide_panel.is_resizing_left || self.ide_panel.is_resizing_bottom {
                    self.ide_panel.is_resizing_left = false;
                    self.ide_panel.is_resizing_bottom = false;
                    crate::save_panel_state(&self.ide_panel);
                }
            }
            self.is_dragging = false;
            self.ide_panel.is_dragging_terminal = false;
            self.scroll_y.is_dragging = false;
            self.is_dragging_search = false;
            self.is_dragging_settings_ignore = false;
            self.is_dragging_lsp_log = false;
            self.autocomplete_scroll.is_dragging = false;
            self.scroll_x.is_dragging = false;
            for term in &mut self.ide_panel.terminals {
                term.scroll_y.is_dragging = false;
            }
            self.ide_panel.lsp_scroll_x.is_dragging = false;
            self.ide_panel.lsp_scroll_y.is_dragging = false;
            self.ide_panel.problems_scroll.is_dragging = false;
            for scroll in self.ide_panel.lsp_logs_scroll_y.values_mut() {
                scroll.is_dragging = false;
            }
            for scroll in self.ide_panel.lsp_logs_scroll_x.values_mut() {
                scroll.is_dragging = false;
            }
            crate::app::mouse::HOVER_STATE.with(|s| {
                if let Some(popup) = &mut s.borrow_mut().popup {
                    popup.scroll.is_dragging = false;
                }
            });
            self.scroll_y.target = self.scroll_y.target.round();
            self.scroll_x.target = self.scroll_x.target.round();
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        if state == ElementState::Pressed {
            let last_mouse_x = self.renderer.as_ref().unwrap().last_mouse_x;
            let last_mouse_y = self.renderer.as_ref().unwrap().last_mouse_y;
            let s = self.renderer.as_ref().unwrap().scale_factor;

            // Sidebar processing and resizing moved to ui_registry
            // Обработка кликов в дереве файлов теперь выполняется через ui_registry
            // Search input handled by ui_registry

            if self.autocomplete_active {
                if let Some((rx, ry, rw, rh)) = self.autocomplete_rect {
                    if last_mouse_x >= rx
                        && last_mouse_x <= rx + rw
                        && last_mouse_y >= ry
                        && last_mouse_y <= ry + rh
                    {
                        let scroll_x = rx + rw - 14.0 * s;
                        let step = 36.0 * s;
                        let total_items = self.autocomplete_options.len() as f32;
                        let visible_items = total_items.min(7.0);
                        let total_h = total_items * step + 16.0 * s;

                        if last_mouse_x >= scroll_x && total_h > rh {
                            self.autocomplete_scroll.is_dragging = true;
                            let max_scroll = ((total_items - visible_items) * step).max(0.0);
                            let scroll_ratio = (self.autocomplete_scroll.current
                                / max_scroll.max(1.0))
                            .clamp(0.0, 1.0);

                            let track_h = rh - 8.0 * s;
                            let thumb_h = (rh / total_h * track_h).max(20.0 * s);
                            let thumb_start_y = ry + 4.0 * s + scroll_ratio * (track_h - thumb_h);

                            if last_mouse_y >= thumb_start_y
                                && last_mouse_y <= thumb_start_y + thumb_h
                            {
                                self.autocomplete_scroll.drag_offset = last_mouse_y - thumb_start_y;
                            } else {
                                self.autocomplete_scroll.anim_speed = 15.0;
                                self.autocomplete_scroll.drag_offset = thumb_h / 2.0;
                                let new_ratio = (last_mouse_y
                                    - ry
                                    - 4.0 * s
                                    - self.autocomplete_scroll.drag_offset)
                                    / (track_h - thumb_h).max(1.0);
                                self.autocomplete_scroll.target =
                                    (new_ratio * max_scroll).clamp(0.0, max_scroll);
                            }
                        } else if let Some(idx) = self.autocomplete_hovered_idx {
                            self.autocomplete_selected_idx = idx;
                            self.apply_autocomplete();
                        }
                        return;
                    } else {
                        self.autocomplete_active = false;
                        self.autocomplete_selected_idx = 0;
                        self.window.as_ref().unwrap().request_redraw();
                    }
                }
            }

            // Sticky lines, Folding, Scrollbars and Text Selection handled by ui_registry
            self.last_action = Instant::now();
        }
        self.window.as_ref().unwrap().request_redraw();
    }

    pub fn handle_main_cursor_moved(&mut self, position: winit::dpi::PhysicalPosition<f64>) {
        self.renderer.as_mut().unwrap().last_mouse_x = position.x as f32;
        self.renderer.as_mut().unwrap().last_mouse_y = position.y as f32;

        if self.dialog_window.is_some() {
            return;
        }

        let s = self.renderer.as_ref().unwrap().scale_factor;

        if let (true, Some((rx, ry, rw, rh))) = (self.autocomplete_active, self.autocomplete_rect) {
            let px = position.x as f32;
            let py = position.y as f32;

            if self.autocomplete_scroll.is_dragging {
                self.autocomplete_scroll.anim_speed = 15.0;
                let step = 36.0 * s;
                let total_items = self.autocomplete_options.len() as f32;
                let visible_items = total_items.min(7.0);

                let track_h = rh - 8.0 * s;
                let total_h = total_items * step + 16.0 * s;
                let thumb_h = (rh / total_h * track_h).max(20.0 * s);
                let max_scroll = ((total_items - visible_items) * step).max(0.0);

                let ratio = (py - ry - 4.0 * s - self.autocomplete_scroll.drag_offset)
                    / (track_h - thumb_h).max(1.0);
                self.autocomplete_scroll.target = (ratio * max_scroll).clamp(0.0, max_scroll);
                self.window.as_ref().unwrap().request_redraw();
                return;
            }

            if px >= rx && px <= rx + rw && py >= ry && py <= ry + rh {
                let scroll_x = rx + rw - 14.0 * s;
                if px < scroll_x {
                    let item_h = 36.0 * s;
                    let scroll = self.autocomplete_scroll.current;
                    let content_y = py - ry + scroll - (4.0 * s);
                    if content_y >= 0.0 {
                        let idx = (content_y / item_h) as usize;
                        if idx < self.autocomplete_options.len() {
                            self.autocomplete_hovered_idx = Some(idx);
                        } else {
                            self.autocomplete_hovered_idx = None;
                        }
                    }
                } else {
                    self.autocomplete_hovered_idx = None;
                }
                self.window.as_ref().unwrap().request_redraw();
                return;
            } else {
                self.autocomplete_hovered_idx = None;
            }
        }

        // DnD и ресайз IDE-панелей (обработка движения мыши)
        if self.is_ide_mode {
            let px = position.x as f32;
            let py = position.y as f32;

            if let Some(ref mut drag) = self.ide_panel.drag {
                drag.current_y = py;
                if (py - drag.start_y).abs() > 5.0 * s {
                    drag.threshold_passed = true;
                }
                self.window.as_ref().unwrap().request_redraw();
                return;
            }

            if let Some(ref mut drag) = self.ide_panel.tab_drag {
                drag.current_x = px;
                if (px - drag.start_x).abs() > 5.0 * s {
                    drag.threshold_passed = true;
                }
                self.window.as_ref().unwrap().request_redraw();
                return;
            }

            if self.ide_panel.is_resizing_left {
                let sb_w = 48.0 * s;
                let ww = self.window.as_ref().unwrap().inner_size().width as f32;
                let max_w = ((ww - sb_w) / s) - 300.0;
                let new_w = ((px - sb_w) / s).max(80.0).min(max_w.max(80.0));
                self.ide_panel.left_width = new_w;
                self.window.as_ref().unwrap().request_redraw();
                return;
            }

            if self.ide_panel.is_resizing_bottom {
                let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                let max_h = (wh / s) - 50.0;
                let new_h = ((wh - py) / s).max(60.0).min(max_h.max(60.0));
                self.ide_panel.bottom_height = new_h;
                self.window.as_ref().unwrap().request_redraw();
                return;
            }
        }

        // Hover над узлами дерева файлов
        if self.is_ide_mode && self.ide_panel.is_open(crate::app::PanelId::Explorer) {
            let mut new_hover = self.file_tree_node_at(position.x as f32, position.y as f32);

            let s = self.renderer.as_ref().unwrap().scale_factor;
            let wh = self.window.as_ref().unwrap().inner_size().height as f32;
            let is_top = self.ide_panel.slots.iter().any(|sl| {
                sl.id == crate::app::PanelId::Explorer && sl.group == crate::app::PanelGroup::Top
            });
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

            let (ecx, ecy, ecw, ech) = if is_top {
                let panel_left_w = self.ide_panel.left_width * s;
                let title_h = 32.0 * s;
                (
                    48.0 * s,
                    title_h,
                    panel_left_w,
                    wh - title_h - effective_bottom_h,
                )
            } else {
                let ww = self.window.as_ref().unwrap().inner_size().width as f32;
                let tab_h = 32.0 * s;
                (
                    48.0 * s,
                    wh - panel_bottom_h + 1.0 + tab_h,
                    ww - 48.0 * s,
                    panel_bottom_h - 1.0 - tab_h,
                )
            };

            let px = position.x as f32;
            let py = position.y as f32;
            if px < ecx || px > ecx + ecw || py < ecy || py > ecy + ech {
                new_hover = None;
            }

            if new_hover != self.ide_panel.file_tree_hovered_idx {
                self.ide_panel.file_tree_hovered_idx = new_hover;
                self.window.as_ref().unwrap().request_redraw();
            }
        }

        let s = self.renderer.as_ref().unwrap().scale_factor;
        let mut in_hover_popup = false;
        HOVER_STATE.with(|state| {
            let state = state.borrow();
            if let Some(rect) = state.rect {
                let pad = 40.0 * s;
                if position.x as f32 >= rect.0 - pad
                    && position.x as f32 <= rect.0 + rect.2 + pad
                    && position.y as f32 >= rect.1 - pad
                    && position.y as f32 <= rect.1 + rect.3 + pad
                {
                    in_hover_popup = true;
                }
            }
        });

        let minimap_w = self.renderer.as_ref().unwrap().minimap_width;
        let padding = self.renderer.as_ref().unwrap().left_padding;
        let window_size = self.window.as_ref().unwrap().inner_size();

        if !in_hover_popup {
            let tab_bar_h = if self.show_welcome || !self.is_ide_mode {
                0.0
            } else {
                38.0 * s
            };
            let render_scroll_y = self.scroll_y.current.round() - tab_bar_h;
            let byte_offset = self.renderer.as_mut().unwrap().get_byte_at_xy(
                &self.editor,
                position.x as f32,
                position.y as f32 + render_scroll_y,
            );
            let in_diag_popup = self
                .renderer
                .as_ref()
                .unwrap()
                .last_diag_popup_rect
                .map(|(rx, ry, rw, rh)| {
                    position.x as f32 >= rx
                        && position.x as f32 <= rx + rw
                        && position.y as f32 >= ry
                        && position.y as f32 <= ry + rh
                })
                .unwrap_or(false);
            let is_text_area = !in_diag_popup
                && position.x as f32 > padding
                && (position.x as f32) < (window_size.width as f32 - minimap_w);

            HOVER_STATE.with(|state| {
                let mut state = state.borrow_mut();
                if is_text_area && is_hover_target_byte(&self.editor, byte_offset) {
                    if state.byte_offset != Some(byte_offset) {
                        state.byte_offset = Some(byte_offset);
                        state.timer = 0.0;
                        state.request_id = None;
                        state.popup = None;
                        state.rect = None;
                    }
                } else {
                    state.byte_offset = None;
                    state.timer = 0.0;
                    state.request_id = None;
                    state.popup = None;
                    state.rect = None;
                }
            });
        }
        let wh = window_size.height as f32;

        let max_scroll = self
            .renderer
            .as_mut()
            .unwrap()
            .get_max_scroll(&self.editor, wh);
        let scrollbar_w = if max_scroll > 0.0 { 10.0 * s } else { 0.0 };
        let scrollbar_x = window_size.width as f32 - minimap_w - scrollbar_w;

        if self.is_dragging_settings_ignore {
            let w = (1000.0 * s)
                .min(self.window.as_ref().unwrap().inner_size().width as f32 - 40.0 * s);
            let x = ((self.window.as_ref().unwrap().inner_size().width as f32 - w) / 2.0).round();
            let content_x = x + 40.0 * s + 200.0 * s + 30.0 * s;
            let start_x = content_x + 8.0 * s;
            let text = self.settings_ignore_editor.get_full_text();
            let x_offset = (position.x as f32 - start_x + self.settings_ignore_scroll_x).max(0.0);
            let mut current_x = 0.0;
            let mut target_idx = text.len();
            let mut byte_idx = 0;
            for c in text.chars() {
                let adv = self
                    .renderer
                    .as_mut()
                    .unwrap()
                    .get_ui_glyph(c)
                    .map(|g| g.advance)
                    .unwrap_or(10.0)
                    * 0.95;
                if x_offset <= current_x + adv / 2.0 {
                    target_idx = byte_idx;
                    break;
                }
                current_x += adv;
                byte_idx += c.len_utf8();
            }
            self.settings_ignore_editor.cursor = target_idx;
        } else if self.is_dragging_lsp_log {
            // Drag-selection в логах LSP
            if let Some(focused_name) = self.ide_panel.lsp_logs_focused.clone() {
                if let Some((cx, cy, _cw, _ch)) = self.lsp_panel_bounds() {
                    let pad_x = 12.0 * s;
                    let btn_h = 24.0 * s;
                    let scroll_y = self.ide_panel.lsp_scroll_y.current.round();
                    let mut cur_y = cy + 8.0 * s - scroll_y;

                    for srv in self.ide_panel.lsp_servers.clone().iter() {
                        let logs_h = self.lsp_server_logs_h(srv, s);
                        let is_exp = logs_h > 0.0;
                        let row_h = 136.0 * s + logs_h;

                        if srv.name == focused_name.as_str() && is_exp {
                            let card_x = cx + 12.0 * s;
                            let btn_y1 = cur_y + 56.0 * s;
                            let btn_y2 = btn_y1 + btn_h + 8.0 * s;
                            let log_bg_x = card_x + pad_x;
                            let log_bg_y = btn_y2 + btn_h + 10.0 * s;

                            let inner_scroll_y = self
                                .ide_panel
                                .lsp_logs_scroll_y
                                .get(srv.name)
                                .map(|ss| ss.current)
                                .unwrap_or(0.0)
                                .round();
                            let inner_scroll_x = self
                                .ide_panel
                                .lsp_logs_scroll_x
                                .get(srv.name)
                                .map(|ss| ss.current)
                                .unwrap_or(0.0)
                                .round();
                            let mut text_y = log_bg_y + 16.0 * s - inner_scroll_y;
                            let line_h = 16.0 * s;
                            let my_drag = position.y as f32;

                            if let Some(ed) = self
                                .ide_panel
                                .lsp_log_editors
                                .get_mut(focused_name.as_str())
                            {
                                let mut phys_line = 0;
                                let (first, second) = ed.text_parts();
                                let first_len = first.len();

                                while phys_line < ed.line_offsets.len() {
                                    let is_folded = ed.folded_lines.contains(&phys_line);
                                    let fold_end = if is_folded {
                                        ed.foldable_lines.get(&phys_line).copied()
                                    } else {
                                        None
                                    };

                                    if my_drag >= text_y - line_h && my_drag <= text_y {
                                        let start_byte = ed.line_offsets[phys_line];
                                        let end_byte = if phys_line + 1 < ed.line_offsets.len() {
                                            ed.line_offsets[phys_line + 1].saturating_sub(1)
                                        } else {
                                            ed.len()
                                        };

                                        let click_x_in_line =
                                            (position.x as f32 - log_bg_x - 20.0 * s
                                                + inner_scroll_x)
                                                .max(0.0);
                                        let r = self.renderer.as_mut().unwrap();

                                        let mut current_x = 0.0;
                                        let mut best_dist = click_x_in_line.abs();
                                        let mut byte_off = start_byte;
                                        let mut current_chunk_offset = start_byte;

                                        while current_chunk_offset < end_byte {
                                            let chunk = if current_chunk_offset < first_len {
                                                &first
                                                    [current_chunk_offset..end_byte.min(first_len)]
                                            } else {
                                                &second[current_chunk_offset - first_len
                                                    ..end_byte - first_len]
                                            };

                                            for c in chunk.chars() {
                                                let adv = if c == '\n'
                                                    || c == '\u{FE0F}'
                                                    || c == '\u{200D}'
                                                {
                                                    0.0
                                                } else {
                                                    r.char_advance(c) * 0.7
                                                };
                                                let dist = (current_x - click_x_in_line).abs();
                                                if dist < best_dist {
                                                    best_dist = dist;
                                                    byte_off = current_chunk_offset;
                                                }
                                                current_x += adv;
                                                current_chunk_offset += c.len_utf8();
                                            }
                                        }
                                        if (current_x - click_x_in_line).abs() < best_dist {
                                            byte_off = end_byte;
                                        }

                                        if ed.selection_anchor.is_none() {
                                            ed.selection_anchor = Some(byte_off);
                                        }
                                        ed.cursor = byte_off;
                                        break;
                                    }

                                    if is_folded {
                                        phys_line = fold_end.unwrap();
                                    }
                                    phys_line += 1;
                                    text_y += line_h;
                                }
                            }
                            break;
                        }
                        cur_y += row_h + 16.0 * s;
                    }
                }
            }
        } else if self.ide_panel.lsp_scroll_x.is_dragging {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            if let Some((cx, _, cw, _)) = self.lsp_panel_bounds() {
                let track_w = cw - 30.0 * s;
                let max_x = 0.0;
                let thumb_w = track_w;
                let ratio =
                    (position.x as f32 - cx - 10.0 * s - self.ide_panel.lsp_scroll_x.drag_offset)
                        / (track_w - thumb_w).max(0.0001);
                self.ide_panel.lsp_scroll_x.target = (ratio * max_x).clamp(0.0, max_x);
                self.ide_panel.lsp_scroll_x.current = self.ide_panel.lsp_scroll_x.target;
            }
        } else if self
            .ide_panel
            .terminals
            .iter()
            .any(|t| t.scroll_y.is_dragging)
        {
            let active = self.ide_panel.active_terminal;
            if let Some(term) = self.ide_panel.terminals.get_mut(active) {
                let grid = term.grid.lock().unwrap();
                let is_alt = grid.is_alt;
                drop(grid);
                if is_alt {
                    term.scroll_y.is_dragging = false;
                    return;
                }

                let s = self.renderer.as_ref().unwrap().scale_factor;
                let bottom_h = self.ide_panel.bottom_height * s;
                let tab_h = 32.0 * s;
                let content_y = self.window.as_ref().unwrap().inner_size().height as f32 - bottom_h
                    + 1.0
                    + tab_h;
                let content_h = bottom_h - 1.0 - tab_h;
                let term_content_y = content_y + 32.0 * s;
                let term_content_h = content_h - 32.0 * s;

                let lh = self.renderer.as_ref().unwrap().line_height;
                let char_h = lh * 1.05;

                let grid = term.grid.lock().unwrap();
                let is_alt = grid.is_alt;
                let scrollback_len = if is_alt { 0 } else { grid.scrollback.len() };
                let total_lines = scrollback_len + grid.lines.len();
                drop(grid);

                let max_scroll = if is_alt {
                    0.0
                } else {
                    ((total_lines as f32 * char_h) - term_content_h).max(0.0)
                };
                if max_scroll > 0.0 {
                    let track_h = term_content_h;
                    let ratio = ((position.y as f32 - term_content_y) / track_h).clamp(0.0, 1.0);
                    let progress = 1.0 - ratio;
                    term.scroll_y.target = progress * max_scroll;
                    term.scroll_y.current = term.scroll_y.target;
                    self.window.as_ref().unwrap().request_redraw();
                }
            }
        } else if self.ide_panel.problems_scroll.is_dragging {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            let wh = self.window.as_ref().unwrap().inner_size().height as f32;
            let bottom_h = self.ide_panel.bottom_height * s;
            let cy = wh - bottom_h;

            let item_h = 24.0 * s;
            let total_h = self.ide_panel.flat_diags.len() as f32 * item_h;
            let track_h = bottom_h - 40.0 * s;
            let max_scroll = (total_h - track_h).max(0.0);
            let thumb_h = (track_h / total_h * track_h).max(20.0 * s);
            let list_y = cy + 40.0 * s;

            let ratio = (position.y as f32 - list_y - self.ide_panel.problems_scroll.drag_offset)
                / (track_h - thumb_h).max(0.0001);
            self.ide_panel.problems_scroll.target = (ratio * max_scroll).clamp(0.0, max_scroll);
            self.ide_panel.problems_scroll.current = self.ide_panel.problems_scroll.target;
            self.window.as_ref().unwrap().request_redraw();
            return;
        } else if crate::app::mouse::HOVER_STATE.with(|s| {
            s.borrow()
                .popup
                .as_ref()
                .map(|p| p.scroll.is_dragging)
                .unwrap_or(false)
        }) {
            crate::app::mouse::HOVER_STATE.with(|hover_state| {
                let mut state = hover_state.borrow_mut();
                if let Some(rect) = state.rect {
                    let (_, by, _, box_h) = rect;
                    let max_scroll = state.max_scroll;
                    let track_h = box_h - 16.0 * s;
                    let thumb_h = (box_h / (box_h + max_scroll) * track_h).max(20.0 * s);
                    if let Some(popup) = &mut state.popup {
                        let ratio = (position.y as f32 - by - 8.0 * s - popup.scroll.drag_offset)
                            / (track_h - thumb_h).max(0.0001);
                        popup.scroll.target = (ratio * max_scroll).clamp(0.0, max_scroll);
                        popup.scroll.current = popup.scroll.target;
                    }
                }
            });
            self.window.as_ref().unwrap().request_redraw();
            return;
        } else if self.ide_panel.lsp_scroll_y.is_dragging {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            if let Some((_, cy, _, ch)) = self.lsp_panel_bounds() {
                let total_h = self.lsp_panel_total_h(s);
                let track_h = ch - 10.0 * s;
                let max_y = (total_h - ch).max(0.0);
                let thumb_h = (ch / total_h * track_h).max(40.0 * s);
                let ratio =
                    (position.y as f32 - cy - 5.0 * s - self.ide_panel.lsp_scroll_y.drag_offset)
                        / (track_h - thumb_h).max(0.0001);
                self.ide_panel.lsp_scroll_y.target = (ratio * max_y).clamp(0.0, max_y);
                self.ide_panel.lsp_scroll_y.current = self.ide_panel.lsp_scroll_y.target;
            }
        } else if self.ide_panel.lsp_servers.iter().any(|info| {
            self.ide_panel
                .lsp_logs_scroll_y
                .get(info.name)
                .map(|s| s.is_dragging)
                .unwrap_or(false)
                || self
                    .ide_panel
                    .lsp_logs_scroll_x
                    .get(info.name)
                    .map(|s| s.is_dragging)
                    .unwrap_or(false)
        }) {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            for (idx, info) in self.ide_panel.lsp_servers.clone().iter().enumerate() {
                let name = info.name.to_string();
                let is_drag_y = self
                    .ide_panel
                    .lsp_logs_scroll_y
                    .get(&name)
                    .map(|s| s.is_dragging)
                    .unwrap_or(false);
                let is_drag_x = self
                    .ide_panel
                    .lsp_logs_scroll_x
                    .get(&name)
                    .map(|s| s.is_dragging)
                    .unwrap_or(false);

                if is_drag_y || is_drag_x {
                    if let Some((cx, cy, cw, _ch)) = self.lsp_panel_bounds() {
                        let scroll_y = self.ide_panel.lsp_scroll_y.current.round();
                        let mut current_y = cy + 8.0 * s - scroll_y;
                        for (i, srv) in self.ide_panel.lsp_servers.iter().enumerate() {
                            if i == idx {
                                break;
                            }
                            let logs_h = self.lsp_server_logs_h(srv, s);
                            current_y += 136.0 * s + logs_h + 16.0 * s;
                        }

                        let logs_h = self.lsp_server_logs_h(info, s);
                        let btn_y1 = current_y + 56.0 * s;
                        let btn_h = 24.0 * s;
                        let btn_y2 = btn_y1 + btn_h + 8.0 * s;
                        let log_bg_y = btn_y2 + btn_h + 10.0 * s;
                        let log_bg_x = cx + 24.0 * s;
                        let log_bg_w = cw - 48.0 * s;
                        let log_bg_h = logs_h - 18.0 * s;

                        let (inner_total_h, inner_max_w) = self.lsp_server_inner_size(info, s);

                        if is_drag_y {
                            let max_y = (inner_total_h - log_bg_h).max(0.0);
                            let track_h = log_bg_h - 14.0 * s;
                            let thumb_h = (log_bg_h / inner_total_h * track_h).max(20.0 * s);
                            let sy = self.ide_panel.lsp_logs_scroll_y.get_mut(&name).unwrap();
                            let ratio = (position.y as f32 - log_bg_y - 2.0 * s - sy.drag_offset)
                                / (track_h - thumb_h).max(0.0001);
                            sy.target = (ratio * max_y).clamp(0.0, max_y);
                            sy.current = sy.target;
                        } else if is_drag_x {
                            let max_x = (inner_max_w + 20.0 * s - log_bg_w).max(0.0);
                            let track_w = log_bg_w - 14.0 * s;
                            let thumb_w =
                                (log_bg_w / (inner_max_w + 20.0 * s) * track_w).max(20.0 * s);
                            let sx = self.ide_panel.lsp_logs_scroll_x.get_mut(&name).unwrap();
                            let ratio = (position.x as f32 - log_bg_x - 2.0 * s - sx.drag_offset)
                                / (track_w - thumb_w).max(0.0001);
                            sx.target = (ratio * max_x).clamp(0.0, max_x);
                            sx.current = sx.target;
                        }
                    }
                    break;
                }
            }
        } else if self.is_dragging_search {
            let search_w = 480.0 * s;
            let input_x = if self.ide_panel.term_search_focused {
                let panel_w = self.window.as_ref().unwrap().inner_size().width as f32 - 48.0 * s;
                48.0 * s + panel_w - search_w - 20.0 * s + 10.0 * s
            } else {
                scrollbar_x - search_w - 20.0 * s + 10.0 * s
            };

            let text = if self.ide_panel.term_search_focused {
                self.ide_panel.term_search_editor.get_full_text()
            } else {
                self.search_editor.get_full_text()
            };

            let x_offset = (position.x as f32 - (input_x + 5.0 * s)).max(0.0);
            let mut current_x = 0.0;
            let mut target_idx = text.len();
            let mut byte_idx = 0;

            for c in text.chars() {
                let adv = self
                    .renderer
                    .as_mut()
                    .unwrap()
                    .get_ui_glyph(c)
                    .map(|g| g.advance)
                    .unwrap_or(10.0);
                if x_offset <= current_x + adv / 2.0 {
                    target_idx = byte_idx;
                    break;
                }
                current_x += adv;
                byte_idx += c.len_utf8();
            }
            if self.ide_panel.term_search_focused {
                self.ide_panel.term_search_editor.cursor = target_idx;
            } else {
                self.search_editor.cursor = target_idx;
            }
        } else if self.scroll_x.is_dragging {
            let r = self.renderer.as_ref().unwrap();
            let track_w = scrollbar_x - padding;
            let max_x = r.max_scroll_x;
            let thumb_w = (track_w / (max_x + track_w).max(1.0) * track_w).max(40.0 * s);
            let ratio = (position.x as f32 - padding - self.scroll_x.drag_offset)
                / (track_w - thumb_w).max(0.0001);
            self.scroll_x.target = (ratio * max_x).clamp(0.0, max_x);
            self.scroll_x.current = self.scroll_x.target;
        } else if self.scroll_y.is_dragging {
            let now = std::time::Instant::now();
            let elapsed = now.duration_since(self.last_click_time).as_millis();
            let dy = (position.y as f32 - self.last_click_pos.1).abs();

            if elapsed > 120 || dy > 10.0 {
                let r = self.renderer.as_ref().unwrap();
                let s = r.scale_factor;
                let tab_bar_h = if self.show_welcome || !self.is_ide_mode {
                    0.0
                } else {
                    38.0 * s
                };
                let editor_height = wh - tab_bar_h;
                let minimap_w = r.minimap_width;

                let is_minimap_drag = self.last_click_pos.0
                    >= (self.window.as_ref().unwrap().inner_size().width as f32 - minimap_w);

                let thumb_h = if is_minimap_drag {
                    let total_lines_f32 = self.editor.line_offsets.len() as f32;
                    let visible_minimap_lines = total_lines_f32.min(900.0);
                    let minimap_line_h =
                        (editor_height / (visible_minimap_lines + 2.0).max(1.0)).max(1.5);
                    let visible_lines = editor_height / r.line_height;
                    (visible_lines * minimap_line_h).max(4.0)
                } else {
                    let total_content_height =
                        (self.editor.line_offsets.len() as f32 + 2.0) * r.line_height;
                    (editor_height / total_content_height.max(editor_height) * editor_height)
                        .max(20.0 * s)
                };

                let track_h = editor_height;
                let track_start_y = tab_bar_h;
                let last_mouse_y = r.last_mouse_y;

                let scroll_ratio = (last_mouse_y - track_start_y - self.scroll_y.drag_offset)
                    / (track_h - thumb_h).max(0.0001);

                self.scroll_y.target = (scroll_ratio * max_scroll).clamp(0.0, max_scroll).round();
                self.scroll_y.anim_speed = 15.0;
            }
        } else if self.ide_panel.is_dragging_terminal && self.is_dragging && !self.show_settings {
            let active = self.ide_panel.active_terminal;
            if let Some(term) = self.ide_panel.terminals.get_mut(active) {
                let s = self.renderer.as_ref().unwrap().scale_factor;
                let bottom_h = self.ide_panel.bottom_height * s;
                let tab_h = 32.0 * s;
                let content_y = self.window.as_ref().unwrap().inner_size().height as f32 - bottom_h
                    + 1.0
                    + tab_h;
                let content_h = bottom_h - 1.0 - tab_h;
                let term_content_y = content_y + 32.0 * s;
                let term_content_h = content_h - 32.0 * s;

                let lh = self.renderer.as_ref().unwrap().line_height;
                let char_h = lh * 1.05;
                let char_w = self.renderer.as_mut().unwrap().char_advance('A') * 1.05;
                let panel_x = 48.0 * s + 10.0 * s;

                let py = position.y as f32;
                let px = position.x as f32;

                let mut grid = term.grid.lock().unwrap();
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
                    term.scroll_y.current.min(max_scroll)
                };
                let offset_from_bottom =
                    (term_content_y + term_content_h - 8.0 * s - py + scroll_offset) / char_h;
                let mut cell_y = total_lines
                    .saturating_sub(1)
                    .saturating_sub(offset_from_bottom.max(0.0).floor() as usize);
                let mut cell_x = ((px - panel_x) / char_w).floor() as usize;

                cell_y = cell_y.min(total_lines.saturating_sub(1));
                cell_x = cell_x.min(grid.cols.saturating_sub(1));

                if let Some((sx, sy, _, _)) = grid.selection {
                    grid.selection = Some((sx, sy, cell_x, cell_y));
                } else {
                    grid.selection = Some((cell_x, cell_y, cell_x, cell_y));
                }
                self.window.as_ref().unwrap().request_redraw();
            }
        } else if self.is_dragging && !self.ide_panel.is_dragging_terminal && !self.show_settings {
            let last_mouse_x = self.renderer.as_ref().unwrap().last_mouse_x;
            let last_mouse_y = self.renderer.as_ref().unwrap().last_mouse_y;
            let tab_bar_h = if self.show_welcome || !self.is_ide_mode {
                0.0
            } else {
                38.0 * self.renderer.as_ref().unwrap().scale_factor
            };
            self.editor.set_cursor_at_pos(
                last_mouse_x,
                last_mouse_y - tab_bar_h + self.scroll_y.current,
                self.renderer.as_mut().unwrap(),
                false,
            );
        }

        self.window.as_ref().unwrap().request_redraw();
    }
}
