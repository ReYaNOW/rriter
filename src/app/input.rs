use crate::app::{App, LspActionItem, PendingAction};
use std::time::Instant;
use winit::event::{ElementState, KeyEvent, MouseScrollDelta};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};

impl App {
    pub fn handle_main_mouse_wheel(&mut self, delta: MouseScrollDelta) {
        let lh = self.renderer.as_ref().unwrap().line_height;
        let s = self.renderer.as_ref().unwrap().scale_factor;
        let shift = self.modifiers.shift_key();

        // Единая дельта как эталон для всех скролл-панелей в редакторе
        let (dx, dy) = match delta {
            MouseScrollDelta::LineDelta(x, y) => (-x * 4.0 * lh, -y * 4.0 * lh),
            MouseScrollDelta::PixelDelta(pos) => (-pos.x as f32, -pos.y as f32),
        };

        // Скролл в области проводника файлов — перехватываем до всего остального
        if self.is_ide_mode && self.ide_panel.is_open(crate::app::PanelId::Explorer) {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            let sb_w = 48.0 * s;
            let panel_left_w = self.ide_panel.left_width * s;
            let mx = self.renderer.as_ref().unwrap().last_mouse_x;
            let my = self.renderer.as_ref().unwrap().last_mouse_y;
            let title_h = 32.0 * s;
            if mx >= sb_w && mx <= sb_w + panel_left_w && my >= title_h {
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

        if self.is_ide_mode && self.ide_panel.is_open(crate::app::PanelId::LspServers) {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            let mx = self.renderer.as_ref().unwrap().last_mouse_x;
            let my = self.renderer.as_ref().unwrap().last_mouse_y;

            let mut lsp_bounds = None;
            let is_top = self.ide_panel.slots.iter().any(|sl| {
                sl.id == crate::app::PanelId::LspServers && sl.group == crate::app::PanelGroup::Top
            });
            if is_top {
                let sb_w = 48.0 * s;
                let title_h = 32.0 * s;
                let panel_left_w = self.ide_panel.left_width * s;
                let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                lsp_bounds = Some((sb_w, title_h, panel_left_w, wh - title_h));
            } else {
                let open_bottom: Vec<_> = self
                    .ide_panel
                    .slots
                    .iter()
                    .filter(|sl| sl.group == crate::app::PanelGroup::Bottom && sl.open)
                    .collect();
                if let Some(first) = open_bottom.first() {
                    if first.id == crate::app::PanelId::LspServers {
                        let sb_w = 48.0 * s;
                        let tab_h = 32.0 * s;
                        let panel_bottom_h = self.ide_panel.bottom_height * s;
                        let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                        let ww = self.window.as_ref().unwrap().inner_size().width as f32;
                        lsp_bounds = Some((
                            sb_w,
                            wh - panel_bottom_h + 1.0 + tab_h,
                            ww - sb_w,
                            panel_bottom_h - 1.0 - tab_h,
                        ));
                    }
                }
            }

            if let Some((cx, cy, cw, ch)) = lsp_bounds {
                if mx >= cx && mx <= cx + cw && my >= cy && my <= cy + ch {
                    self.ide_panel.lsp_scroll_y.anim_speed = 7.0;
                    self.ide_panel.lsp_scroll_x.anim_speed = 7.0;

                    if shift {
                        self.ide_panel.lsp_scroll_x.scroll_by(dy);
                    } else {
                        self.ide_panel.lsp_scroll_y.scroll_by(dy);
                        self.ide_panel.lsp_scroll_x.scroll_by(dx);
                    }

                                                                                let mut total_h = 8.0 * s;
                                                            let mut max_log_w = 0.0f32;
                                                            for info in &self.ide_panel.lsp_servers {
                                                                let is_expanded = self.ide_panel.lsp_logs_expanded.contains(info.name);
                                                                let mut logs_h = 0.0;
                                                                if is_expanded {
                                                                    let mut lines = 0;
                                                                    let mut skip_until = None;
                                                                    if let Some(ed) = self.ide_panel.lsp_log_editors.get(info.name) {
                                                                        for i in 0..ed.line_offsets.len() {
                                                                            if let Some(tgt) = skip_until {
                                                                                if i < tgt { continue; }
                                                                                skip_until = None;
                                                                            }
                                                                            lines += 1;
                                                                            if ed.folded_lines.contains(&i) {
                                                                                skip_until = Some(ed.foldable_lines[&i]);
                                                                            }
                                                                        }
                                                                    } else {
                                                                        for entry in &info.logs {
                                                                            lines += entry.text.split('\n').count();
                                                                        }
                                                                    }
                                                                    logs_h = (lines as f32 * 16.0 * s).max(50.0 * s) + 20.0 * s;
                                                                }
                                                                total_h += 136.0 * s + logs_h + 16.0 * s;
                                                                if is_expanded {
                            for line in &info.logs {
                                let mut draw_str = line.text.as_str();
                                if draw_str.len() > 250 {
                                    draw_str = &draw_str[..250];
                                }
                                let lw = self
                                    .renderer
                                    .as_mut()
                                    .unwrap()
                                    .measure_mono_width(draw_str, 0.7);
                                if lw > max_log_w {
                                    max_log_w = lw;
                                }
                            }
                        }
                    }

                    let max_scroll_y = (total_h - ch).max(0.0);
                    self.ide_panel.lsp_scroll_y.clamp_target(0.0, max_scroll_y);

                    let max_scroll_x = (max_log_w + 20.0 * s - (cw - 32.0 * s)).max(0.0);
                    self.ide_panel.lsp_scroll_x.clamp_target(0.0, max_scroll_x);

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

        self.scroll_y.anim_speed = 7.0;
        self.scroll_x.anim_speed = 7.0;

        if shift {
            self.scroll_x.scroll_by(dy); // Shift конвертирует вертикальный скролл в горизонтальный
        } else {
            self.scroll_y.scroll_by(dy);
            self.scroll_x.scroll_by(dx);
        }

        let wh = self.window.as_ref().unwrap().inner_size().height as f32;
        let max_scroll_y = self
            .renderer
            .as_mut()
            .unwrap()
            .get_max_scroll(&self.editor, wh);
        let max_scroll_x = self.renderer.as_ref().unwrap().max_scroll_x;

        self.scroll_y.clamp_target(0.0, max_scroll_y);
        self.scroll_y.target = self.scroll_y.target.round();
        self.scroll_x.clamp_target(0.0, max_scroll_x);
        self.scroll_x.target = self.scroll_x.target.round();
        self.window.as_ref().unwrap().request_redraw();
    }

        pub fn handle_main_mouse_input(&mut self, _event_loop: &ActiveEventLoop, state: ElementState) {
        if state == ElementState::Pressed {
            let mx = self.renderer.as_ref().unwrap().last_mouse_x;
            let my = self.renderer.as_ref().unwrap().last_mouse_y;

            // Глобальная обработка декларативного UI
            if !self.show_welcome && !self.show_settings && self.dialog_window.is_none() {
                if let Some(clicked_id) = self.ui_registry.find_at(mx, my) {
                    match clicked_id {
                        crate::ui_system::UiId::SidebarSlot(_) => {
                            // Оставляем для обработки DnD ниже
                        }
                        _ => {
                            self.handle_ui_click(clicked_id);
                            return;
                        }
                    }
                }
            }
        }

        // Клик вне меню LSP — закрываем меню
        if state == ElementState::Pressed && self.lsp_actions_menu.is_some() {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            let mx = self.renderer.as_ref().unwrap().last_mouse_x;
            let my = self.renderer.as_ref().unwrap().last_mouse_y;
            if let Some(menu) = &self.lsp_actions_menu {
                let item_h = 36.0 * s;
                let menu_w = 320.0 * s;
                let menu_h = menu.items.len() as f32 * item_h + 8.0 * s;
                let in_menu = mx >= menu.menu_x
                    && mx <= menu.menu_x + menu_w
                    && my >= menu.menu_y
                    && my <= menu.menu_y + menu_h;
                if in_menu {
                    // Клик внутри меню — выбираем элемент
                    let rel_y = my - menu.menu_y - 4.0 * s;
                    let idx = (rel_y / item_h) as usize;
                    if idx < menu.items.len() {
                        let menu_clone = self.lsp_actions_menu.take().unwrap();
                        let item = menu_clone.items[idx].clone();
                        let cursor_line = menu_clone.cursor_line;
                        drop(menu_clone);
                        match item {
                            LspActionItem::CodeAction(action) => {
                                if let (Some(edit), Some(path)) =
                                    (action.edit, self.file_path.clone())
                                {
                                    let new_text = crate::lsp::apply_workspace_edit_to_text(
                                        &self.editor.get_full_text(),
                                        &edit,
                                        &path,
                                    );
                                    if new_text != self.editor.get_full_text() {
                                        self.apply_full_text_replacement(new_text);
                                    }
                                }
                            }
                            LspActionItem::AddNoqa { codes } => {
                                self.insert_noqa_comment(cursor_line, &codes);
                            }
                            LspActionItem::AddNoqaAll => {
                                self.insert_noqa_comment(cursor_line, &[]);
                            }
                        }
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                } else {
                    self.lsp_actions_menu = None;
                    self.window.as_ref().unwrap().request_redraw();
                }
            }
        }

        // Клики по кнопкам LSP-панели
        if state == ElementState::Pressed
            && self.is_ide_mode
            && self.ide_panel.is_open(crate::app::PanelId::LspServers)
        {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            let mx = self.renderer.as_ref().unwrap().last_mouse_x;
            let my = self.renderer.as_ref().unwrap().last_mouse_y;

            let mut lsp_bounds = None;
            let is_top = self.ide_panel.slots.iter().any(|sl| {
                sl.id == crate::app::PanelId::LspServers && sl.group == crate::app::PanelGroup::Top
            });
            if is_top {
                let sb_w = 48.0 * s;
                let title_h = 32.0 * s;
                let panel_left_w = self.ide_panel.left_width * s;
                let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                lsp_bounds = Some((sb_w, title_h, panel_left_w, wh - title_h));
            } else {
                let open_bottom: Vec<_> = self
                    .ide_panel
                    .slots
                    .iter()
                    .filter(|sl| sl.group == crate::app::PanelGroup::Bottom && sl.open)
                    .collect();
                if let Some(first) = open_bottom.first() {
                    if first.id == crate::app::PanelId::LspServers {
                        let sb_w = 48.0 * s;
                        let tab_h = 32.0 * s;
                        let panel_bottom_h = self.ide_panel.bottom_height * s;
                        let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                        let ww = self.window.as_ref().unwrap().inner_size().width as f32;
                        lsp_bounds = Some((
                            sb_w,
                            wh - panel_bottom_h + 1.0 + tab_h,
                            ww - sb_w,
                            panel_bottom_h - 1.0 - tab_h,
                        ));
                    }
                }
            }

                        if let Some((cx, cy, cw, ch)) = lsp_bounds {
                                if mx >= cx && mx <= cx + cw && my >= cy && my <= cy + ch {
                    let scroll_y = self.ide_panel.lsp_scroll_y.current.round();
                    let scroll_x = self.ide_panel.lsp_scroll_x.current;

                                                                                let mut total_h = 8.0 * s;
                    let mut max_log_w = 0.0f32;
                                        for info in self.ide_panel.lsp_servers.iter() {
                        let is_expanded = self.ide_panel.lsp_logs_expanded.contains(info.name);
                        let mut logs_h = 0.0;
                        if is_expanded {
                            let mut lines = 0;
                            let mut skip_until = None;
                            if let Some(ed) = self.ide_panel.lsp_log_editors.get(info.name) {
                                for i in 0..ed.line_offsets.len() {
                                    if let Some(tgt) = skip_until {
                                        if i < tgt { continue; }
                                        skip_until = None;
                                    }
                                    lines += 1;
                                    if ed.folded_lines.contains(&i) {
                                        skip_until = Some(ed.foldable_lines[&i]);
                                    }
                                }
                            } else {
                                for entry in &info.logs {
                                    lines += entry.text.split('\n').count();
                                }
                            }
                            logs_h = (lines as f32 * 16.0 * s).max(50.0 * s) + 20.0 * s;
                        }
                        total_h += 136.0 * s + logs_h + 16.0 * s;
                        if is_expanded {
                            for line in &info.logs {
                                let mut draw_str = line.text.as_str();
                                if draw_str.len() > 250 {
                                    draw_str = &draw_str[..250];
                                }
                                let lw = self.renderer.as_mut().unwrap().measure_mono_width(draw_str, 0.7);
                                if lw > max_log_w { max_log_w = lw; }
                            }
                        }
                    }

                    let max_scroll_x = (max_log_w + 20.0 * s - (cw - 32.0 * s)).max(0.0);
                    if max_scroll_x > 0.0 && my >= cy + ch - 16.0 * s {
                        self.ide_panel.lsp_scroll_x.is_dragging = true;
                        let track_w = cw - 30.0 * s;
                        let thumb_w = (cw / (max_log_w + 20.0 * s) * track_w).max(40.0 * s);
                        let ratio = (scroll_x / max_scroll_x).clamp(0.0, 1.0);
                        let thumb_x = cx + 10.0 * s + ratio * (track_w - thumb_w);

                        if mx >= thumb_x && mx <= thumb_x + thumb_w {
                            self.ide_panel.lsp_scroll_x.drag_offset = mx - thumb_x;
                        } else {
                            self.ide_panel.lsp_scroll_x.drag_offset = thumb_w / 2.0;
                            let new_ratio = (mx - cx - 10.0 * s - self.ide_panel.lsp_scroll_x.drag_offset) / (track_w - thumb_w).max(0.0001);
                            self.ide_panel.lsp_scroll_x.target = (new_ratio * max_scroll_x).clamp(0.0, max_scroll_x);
                            self.ide_panel.lsp_scroll_x.current = self.ide_panel.lsp_scroll_x.target;
                        }
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }

                    let max_scroll_y = (total_h - ch).max(0.0);
                    if max_scroll_y > 0.0 && mx >= cx + cw - 16.0 * s {
                        self.ide_panel.lsp_scroll_y.is_dragging = true;
                        let track_h = ch - 10.0 * s;
                        let thumb_h = (ch / total_h * track_h).max(40.0 * s);
                        let ratio = (scroll_y / max_scroll_y).clamp(0.0, 1.0);
                        let thumb_y = cy + 5.0 * s + ratio * (track_h - thumb_h);

                        if my >= thumb_y && my <= thumb_y + thumb_h {
                            self.ide_panel.lsp_scroll_y.drag_offset = my - thumb_y;
                        } else {
                            self.ide_panel.lsp_scroll_y.drag_offset = thumb_h / 2.0;
                            let new_ratio = (my - cy - 5.0 * s - self.ide_panel.lsp_scroll_y.drag_offset) / (track_h - thumb_h).max(0.0001);
                            self.ide_panel.lsp_scroll_y.target = (new_ratio * max_scroll_y).clamp(0.0, max_scroll_y);
                            self.ide_panel.lsp_scroll_y.current = self.ide_panel.lsp_scroll_y.target;
                        }
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }

                                        let servers_copy = self.ide_panel.lsp_servers.clone();
                    let mut current_y = cy + 8.0 * s - scroll_y;

                    for info in servers_copy.iter() {
                        let is_expanded = self.ide_panel.lsp_logs_expanded.contains(info.name);
                        let mut logs_h = 0.0;
                        if is_expanded {
                            let mut lines = 0;
                            let mut skip_until = None;
                            if let Some(ed) = self.ide_panel.lsp_log_editors.get(info.name) {
                                for i in 0..ed.line_offsets.len() {
                                    if let Some(tgt) = skip_until {
                                        if i < tgt { continue; }
                                        skip_until = None;
                                    }
                                    lines += 1;
                                    if ed.folded_lines.contains(&i) {
                                        skip_until = Some(ed.foldable_lines[&i]);
                                    }
                                }
                            } else {
                                for entry in &info.logs {
                                    lines += entry.text.split('\n').count();
                                }
                            }
                            logs_h = (lines as f32 * 16.0 * s).max(50.0 * s) + 20.0 * s;
                        }
                        let base_h = 136.0 * s;
                        let row_h = base_h + logs_h;

                        if current_y + row_h > cy && current_y < cy + ch {
                            let card_x = cx + 12.0 * s;
                            let card_w = cw - 24.0 * s;
                            let pad_x = 12.0 * s;
                            let btn_h = 24.0 * s;
                            let btn_y1 = current_y + 56.0 * s;
                            let btn_y2 = btn_y1 + btn_h + 8.0 * s;

                            if is_expanded {
                                let log_bg_y = btn_y2 + btn_h + 10.0 * s;
                                let box_h = logs_h - 18.0 * s;
                                let log_bg_x = card_x + pad_x;
                                let log_bg_w = card_w - pad_x * 2.0;

                                if mx >= log_bg_x && mx <= log_bg_x + log_bg_w && my >= log_bg_y && my <= log_bg_y + box_h {
                                    self.ide_panel.lsp_logs_focused = Some(info.name.to_string());

                                    let mut text_y = log_bg_y + 16.0 * s;
                                    let mut global_line_count = 0;
                                    let mut skip_until = None;
                                    let line_h = 16.0 * s;
                                    let scroll_x = self.ide_panel.lsp_scroll_x.current;

                                    for entry in &info.logs {
                                        for line in entry.text.split('\n') {
                                            if let Some(tgt) = skip_until {
                                                if global_line_count < tgt {
                                                    global_line_count += 1;
                                                    continue;
                                                } else { skip_until = None; }
                                            }

                                            if my >= text_y - line_h && my <= text_y {
                                                if let Some(ed) = self.ide_panel.lsp_log_editors.get_mut(info.name) {
                                                    let click_x_in_line = (mx - log_bg_x - 20.0 * s + scroll_x).max(0.0);

                                                    let r = self.renderer.as_mut().unwrap();
                                                    let mut best_pos = 0usize;
                                                    let mut best_dist = f32::MAX;
                                                    let mut ci = 0usize;
                                                    for c in line.chars() {
                                                        let x = r.measure_mono_width(&line[..ci], 0.7);
                                                        if (x - click_x_in_line).abs() < best_dist {
                                                            best_dist = (x - click_x_in_line).abs();
                                                            best_pos = ci;
                                                        }
                                                        ci += c.len_utf8();
                                                    }
                                                    let x_end = r.measure_mono_width(line, 0.7);
                                                    if (x_end - click_x_in_line).abs() < best_dist {
                                                        best_pos = line.len();
                                                    }
                                                    let line_start_byte = ed.line_offsets[global_line_count];
                                                    let byte_off = (line_start_byte + best_pos).min(ed.len());

                                                    let shift = self.modifiers.shift_key();
                                                    let now = std::time::Instant::now();
                                                    let dx = mx - self.last_click_pos.0;
                                                    let dy = my - self.last_click_pos.1;
                                                    let dist_sq = dx * dx + dy * dy;

                                                    if now.duration_since(self.last_click_time).as_millis() < 400 && dist_sq < 25.0 {
                                                        self.click_count += 1;
                                                    } else {
                                                        self.click_count = 1;
                                                    }

                                                    self.last_click_time = now;
                                                    self.last_click_pos = (mx, my);

                                                    if shift {
                                                        if ed.selection_anchor.is_none() {
                                                            ed.selection_anchor = Some(ed.cursor);
                                                        }
                                                    } else {
                                                        ed.selection_anchor = None;
                                                    }
                                                    ed.cursor = byte_off;
                                                    self.is_dragging_lsp_log = true;

                                                    if self.click_count == 2 {
                                                        ed.select_word();
                                                    } else if self.click_count >= 3 {
                                                        ed.select_line();
                                                        self.click_count = 3;
                                                    }
                                                }
                                                self.window.as_ref().unwrap().request_redraw();
                                                return;
                                            }

                                            if let Some(ed) = self.ide_panel.lsp_log_editors.get(info.name) {
                                                if ed.folded_lines.contains(&global_line_count) {
                                                    skip_until = Some(ed.foldable_lines[&global_line_count]);
                                                }
                                            }

                                            text_y += line_h;
                                            global_line_count += 1;
                                        }
                                    }
                                }
                            }
                        }

                        current_y += row_h + 16.0 * s;
                    }

                    return;
                }
            }
        }

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
                    // Все клики по кнопкам и табам обрабатываются через ui_registry
                    if let Some(clicked_id) = self.ui_registry.find_at(mx, my) {
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
                                let x_offset = (mx - start_x + self.settings_ignore_scroll_x).max(0.0);
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

        if self.show_welcome {
            if state == ElementState::Pressed {
                let mx = self.renderer.as_ref().unwrap().last_mouse_x;
                let my = self.renderer.as_ref().unwrap().last_mouse_y;

                // Используем UI registry для обработки кликов
                if let Some(clicked_id) = self.ui_registry.find_at(mx, my) {
                    self.handle_ui_click(clicked_id);
                }
            }
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        if state == ElementState::Released {
            // Завершаем DnD и ресайз IDE-панелей
            if self.is_ide_mode {
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
                        let btn_size = 36.0 * s;
                        let btn_gap = 8.0 * s;
                        let top_start_y = 6.0 * s;

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
                                    let y = wh
                                        - 6.0 * s
                                        - btn_size
                                        - bottom_idx as f32 * (btn_size + btn_gap);
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
            self.scroll_y.is_dragging = false;
            self.is_dragging_search = false;
            self.is_dragging_settings_ignore = false;
            self.is_dragging_lsp_log = false;
            self.autocomplete_scroll.is_dragging = false;
            self.scroll_x.is_dragging = false;
            self.ide_panel.lsp_scroll_x.is_dragging = false;
            self.ide_panel.lsp_scroll_y.is_dragging = false;
            self.scroll_y.target = self.scroll_y.target.round();
            self.scroll_x.target = self.scroll_x.target.round();
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

                        if state == ElementState::Pressed {
            let last_mouse_x = self.renderer.as_ref().unwrap().last_mouse_x;
            let last_mouse_y = self.renderer.as_ref().unwrap().last_mouse_y;
            let s = self.renderer.as_ref().unwrap().scale_factor;

            // Обработка кликов/DnD по кнопкам IDE-сайдбара и ресайза панелей
            if self.is_ide_mode {
                let sb_w = 48.0 * s;
                let wh = self.window.as_ref().unwrap().inner_size().height as f32;

                if last_mouse_x <= sb_w {
                    let btn_size = 36.0 * s;
                    let btn_gap = 8.0 * s;
                    let top_start_y = 6.0 * s;
                    let mut top_idx = 0usize;
                    let mut bottom_idx = 0usize;
                    let mut hit_id: Option<crate::app::PanelId> = None;

                    for slot in &self.ide_panel.slots {
                        let btn_y = if slot.group == crate::app::PanelGroup::Top {
                            let y = top_start_y + top_idx as f32 * (btn_size + btn_gap);
                            top_idx += 1;
                            y
                        } else {
                            let y =
                                wh - 6.0 * s - btn_size - bottom_idx as f32 * (btn_size + btn_gap);
                            bottom_idx += 1;
                            y
                        };

                        let icon_center = btn_y + btn_size / 2.0;
                        let sq_w = sb_w;
                        let sq_y = (icon_center - sq_w / 2.0).round();

                        if last_mouse_x >= 0.0
                            && last_mouse_x <= sq_w
                            && last_mouse_y >= sq_y
                            && last_mouse_y <= sq_y + sq_w
                        {
                            hit_id = Some(slot.id);
                            break;
                        }
                    }

                    if let Some(panel_id) = hit_id {
                        self.ide_panel.drag = Some(crate::app::PanelDragState {
                            panel_id,
                            start_y: last_mouse_y,
                            current_y: last_mouse_y,
                            threshold_passed: false,
                        });
                    }

                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }

                // Ресайз левой панели
                let panel_left_w = if self.ide_panel.any_top_open() {
                    self.ide_panel.left_width * s
                } else {
                    0.0
                };
                let panel_bottom_h = if self.ide_panel.any_bottom_open() {
                    self.ide_panel.bottom_height * s
                } else {
                    0.0
                };

                if panel_left_w > 0.0 {
                    let resize_x = sb_w + panel_left_w;
                    if (last_mouse_x - resize_x).abs() < 6.0 * s
                        && last_mouse_y < wh - panel_bottom_h
                    {
                        self.ide_panel.is_resizing_left = true;
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                }
                if panel_bottom_h > 0.0 {
                    let resize_y = wh - panel_bottom_h;
                    if (last_mouse_y - resize_y).abs() < 6.0 * s && last_mouse_x >= sb_w {
                        self.ide_panel.is_resizing_bottom = true;
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                }

                                // Обработка кликов в дереве файлов теперь выполняется через ui_registry
            }

            let window_width = self.window.as_ref().unwrap().inner_size().width as f32;
            let minimap_w = self.renderer.as_ref().unwrap().minimap_width;
            let wh = self.window.as_ref().unwrap().inner_size().height as f32;
            let max_scroll = self
                .renderer
                .as_mut()
                .unwrap()
                .get_max_scroll(&self.editor, wh);
            let scrollbar_w = if max_scroll > 0.0 { 10.0 * s } else { 0.0 };
            let scrollbar_x = window_width - minimap_w - scrollbar_w;

                        if self.show_search && self.search_anim_y > -10.0 {
                let search_w = 480.0 * s;
                let search_h = 52.0 * s;
                let search_x = scrollbar_x - search_w - 20.0 * s;

                if last_mouse_x >= search_x
                    && last_mouse_x <= search_x + search_w
                    && last_mouse_y >= self.search_anim_y
                    && last_mouse_y <= self.search_anim_y + search_h
                {
                    let input_x = search_x + 10.0 * s;
                    let input_w = 260.0 * s;

                    if last_mouse_x >= input_x && last_mouse_x <= input_x + input_w {
                        self.search_focused = true;
                        self.is_dragging_search = true;

                        let text = self.search_editor.get_full_text();
                        let x_offset = (last_mouse_x - (input_x + 5.0 * s)).max(0.0);
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
                        self.search_editor.cursor = target_idx;
                        self.search_editor.selection_anchor = Some(target_idx);
                    } else if let Some(clicked_id) = self.ui_registry.find_at(last_mouse_x, last_mouse_y) {
                        self.handle_ui_click(clicked_id);
                    }
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                } else {
                    self.search_focused = false;
                }
            }

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

            for &(rx, ry, rw, rh, target_byte) in
                &self.renderer.as_ref().unwrap().sticky_scroll_rects
            {
                if last_mouse_x >= rx
                    && last_mouse_x <= rx + rw
                    && last_mouse_y >= ry
                    && last_mouse_y <= ry + rh
                {
                    self.editor.cursor = target_byte;
                    self.editor.selection_anchor = None;

                    let phys_line = self
                        .editor
                        .line_offsets
                        .partition_point(|&o| o <= target_byte)
                        .saturating_sub(1);
                    let visual_line = self
                        .renderer
                        .as_ref()
                        .unwrap()
                        .phys_to_visual
                        .get(phys_line)
                        .copied()
                        .unwrap_or(phys_line);
                    let line_y = visual_line as f32 * self.renderer.as_ref().unwrap().line_height;
                    let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                    let max_scroll = self
                        .renderer
                        .as_mut()
                        .unwrap()
                        .get_max_scroll(&self.editor, wh);

                    let padding = self.renderer.as_ref().unwrap().line_height * 3.0;
                    self.scroll_y.target = (line_y - ry - padding)
                        .max(0.0)
                        .clamp(0.0, max_scroll)
                        .round();
                    self.scroll_y.anim_speed = 15.0;
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
            }

            if let Some(r) = self.renderer.as_mut() {
                let mut fold_toggled = false;
                let visual_lines = r.visual_lines.clone();

                for v_line in &visual_lines {
                    let y = r.baseline_offset + v_line.y_offset - self.scroll_y.current;
                    let phys_idx = v_line.physical_line - 1;

                    if self.editor.foldable_lines.contains_key(&phys_idx) {
                        let arrow_x = r.left_padding - 18.0 * s;
                        if last_mouse_x >= arrow_x - 5.0 * s
                            && last_mouse_x <= arrow_x + 15.0 * s
                            && last_mouse_y >= y - r.line_height
                            && last_mouse_y <= y + 5.0 * s
                        {
                            if self.editor.folded_lines.contains(&phys_idx) {
                                self.editor.folded_lines.remove(&phys_idx);
                                self.editor
                                    .folded_start_bytes
                                    .remove(&self.editor.line_offsets[phys_idx]);
                            } else {
                                self.editor.folded_lines.insert(phys_idx);
                                self.editor
                                    .folded_start_bytes
                                    .insert(self.editor.line_offsets[phys_idx]);
                            }
                            fold_toggled = true;
                            break;
                        }
                    }

                    if v_line.is_folded {
                        let fold_str_width = r.measure_ui_width("...", 1.0);
                        let button_width = fold_str_width + 10.0 * s;
                        let mut full_fold_width = button_width;
                        for i in 0..v_line.fold_suffix_len {
                            full_fold_width += r.char_advance(v_line.fold_suffix[i as usize]);
                        }

                        let dots_x =
                            r.left_padding + v_line.whitespace_px_width + v_line.text_px_width
                                - full_fold_width
                                - self.scroll_x.current;

                        if last_mouse_x >= dots_x
                            && last_mouse_x <= dots_x + button_width
                            && last_mouse_y >= y - r.line_height
                            && last_mouse_y <= y + 5.0 * s
                        {
                            self.editor.folded_lines.remove(&phys_idx);
                            self.editor
                                .folded_start_bytes
                                .remove(&self.editor.line_offsets[phys_idx]);
                            fold_toggled = true;
                            break;
                        }
                    }
                }

                if fold_toggled {
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
            }

            let window_width = self.window.as_ref().unwrap().inner_size().width as f32;
            let minimap_w = self.renderer.as_ref().unwrap().minimap_width;
            let wh = self.window.as_ref().unwrap().inner_size().height as f32;
            let max_scroll = self
                .renderer
                .as_mut()
                .unwrap()
                .get_max_scroll(&self.editor, wh);
            let scrollbar_w = if max_scroll > 0.0 { 10.0 * s } else { 0.0 };
            let scrollbar_x = window_width - minimap_w - scrollbar_w;

            let left_pad = self.renderer.as_ref().unwrap().left_padding;

            if self.renderer.as_ref().unwrap().max_scroll_x > 0.0 && last_mouse_y > wh - 14.0 * s {
                if last_mouse_x > left_pad && last_mouse_x < scrollbar_x {
                    self.scroll_x.is_dragging = true;
                    let track_w = scrollbar_x - left_pad;
                    let max_x = self.renderer.as_ref().unwrap().max_scroll_x;
                    let thumb_w = (track_w / (max_x + track_w).max(1.0) * track_w).max(40.0 * s);

                    let scroll_ratio = if max_x > 0.0 {
                        (self.scroll_x.current / max_x).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let thumb_start_x = left_pad + scroll_ratio * (track_w - thumb_w);

                    if last_mouse_x >= thumb_start_x && last_mouse_x <= thumb_start_x + thumb_w {
                        self.scroll_x.drag_offset = last_mouse_x - thumb_start_x;
                    } else {
                        self.scroll_x.drag_offset = thumb_w / 2.0;
                        let ratio = (last_mouse_x - left_pad - self.scroll_x.drag_offset)
                            / (track_w - thumb_w).max(0.0001);
                        self.scroll_x.target = (ratio * max_x).clamp(0.0, max_x);
                        self.scroll_x.current = self.scroll_x.target;
                    }
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
            }

            if last_mouse_x >= scrollbar_x {
                let total_content_height = (self.editor.line_offsets.len() as f32 + 2.0)
                    * self.renderer.as_ref().unwrap().line_height;
                let thumb_h = (wh / total_content_height.max(wh) * wh).max(20.0 * s);

                let track_start_y = 0.0;
                let track_h = wh;

                self.scroll_y.is_dragging = true;
                self.last_click_pos = (last_mouse_x, last_mouse_y);
                self.last_click_time = Instant::now();

                let scroll_ratio = if max_scroll > 0.0 {
                    (self.scroll_y.current / max_scroll).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let thumb_start_y = scroll_ratio * (wh - thumb_h);

                if last_mouse_y >= thumb_start_y && last_mouse_y <= thumb_start_y + thumb_h {
                    self.scroll_y.drag_offset = last_mouse_y - thumb_start_y;
                } else {
                    self.scroll_y.drag_offset = thumb_h / 2.0;
                    let new_scroll_ratio =
                        (last_mouse_y - track_start_y - self.scroll_y.drag_offset)
                            / (track_h - thumb_h).max(0.0001);
                    self.scroll_y.target = (new_scroll_ratio * max_scroll)
                        .clamp(0.0, max_scroll)
                        .round();
                    self.scroll_y.current = self.scroll_y.target;
                }
                        } else {
                self.is_dragging = true;
                self.scroll_y.anim_speed = 15.0;

                self.scroll_y.stop_anim();

                self.ide_panel.lsp_logs_focused = None;
                self.search_focused = false;
                self.settings_ignore_focused = false;

                let now = Instant::now();
                let dx = last_mouse_x - self.last_click_pos.0;
                let dy = last_mouse_y - self.last_click_pos.1;
                let dist_sq = dx * dx + dy * dy;

                if now.duration_since(self.last_click_time).as_millis() < 400 && dist_sq < 25.0 {
                    self.click_count += 1;
                } else {
                    self.click_count = 1;
                }

                self.last_click_time = now;
                self.last_click_pos = (last_mouse_x, last_mouse_y);

                self.editor.set_cursor_at_pos(
                    last_mouse_x,
                    last_mouse_y + self.scroll_y.current,
                    self.renderer.as_mut().unwrap(),
                    true,
                );

                if self.click_count == 2 {
                    self.editor.select_word();
                } else if self.click_count >= 3 {
                    self.editor.select_line();
                    self.click_count = 3;
                }
            }
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

        if self.show_welcome {
            self.window.as_ref().unwrap().request_redraw();
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

            if self.ide_panel.is_resizing_left {
                let sb_w = 48.0 * s;
                let new_w = ((px - sb_w) / s).max(80.0).min(600.0);
                self.ide_panel.left_width = new_w;
                self.window.as_ref().unwrap().request_redraw();
                return;
            }

            if self.ide_panel.is_resizing_bottom {
                let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                let new_h = ((wh - py) / s).max(60.0).min(500.0);
                self.ide_panel.bottom_height = new_h;
                self.window.as_ref().unwrap().request_redraw();
                return;
            }
        }

        // Hover над узлами дерева файлов
        if self.is_ide_mode && self.ide_panel.is_open(crate::app::PanelId::Explorer) {
            let new_hover = self.file_tree_node_at(position.x as f32, position.y as f32);
            if new_hover != self.ide_panel.file_tree_hovered_idx {
                self.ide_panel.file_tree_hovered_idx = new_hover;
                self.window.as_ref().unwrap().request_redraw();
            }
        }

        let minimap_w = self.renderer.as_ref().unwrap().minimap_width;
        let padding = self.renderer.as_ref().unwrap().left_padding;
        let window_size = self.window.as_ref().unwrap().inner_size();
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
                let mut lsp_bounds = None;
                let is_top = self.ide_panel.slots.iter().any(|sl| {
                    sl.id == crate::app::PanelId::LspServers
                        && sl.group == crate::app::PanelGroup::Top
                });
                if is_top {
                    let sb_w = 48.0 * s;
                    let title_h = 32.0 * s;
                    let panel_left_w = self.ide_panel.left_width * s;
                    let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                    lsp_bounds = Some((sb_w, title_h, panel_left_w, wh - title_h));
                } else {
                    let open_bottom: Vec<_> = self
                        .ide_panel
                        .slots
                        .iter()
                        .filter(|sl| sl.group == crate::app::PanelGroup::Bottom && sl.open)
                        .collect();
                    if let Some(first) = open_bottom.first() {
                        if first.id == crate::app::PanelId::LspServers {
                            let sb_w = 48.0 * s;
                            let tab_h = 32.0 * s;
                            let panel_bottom_h = self.ide_panel.bottom_height * s;
                            let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                            let ww = self.window.as_ref().unwrap().inner_size().width as f32;
                            lsp_bounds = Some((
                                sb_w,
                                wh - panel_bottom_h + 1.0 + tab_h,
                                ww - sb_w,
                                panel_bottom_h - 1.0 - tab_h,
                            ));
                        }
                    }
                }

                if let Some((cx, cy, _cw, _ch)) = lsp_bounds {
                    let pad_x = 12.0 * s;
                    let btn_h = 24.0 * s;
                    let scroll_y = self.ide_panel.lsp_scroll_y.current.round();
                    let scroll_x = self.ide_panel.lsp_scroll_x.current;
                    let mut cur_y = cy + 8.0 * s - scroll_y;

                    for srv in self.ide_panel.lsp_servers.clone().iter() {
                        let is_exp = self.ide_panel.lsp_logs_expanded.contains(srv.name);
                        let mut logs_h = 0.0;
                        if is_exp {
                            let mut lines = 0;
                            let mut skip_until = None;
                            if let Some(ed) = self.ide_panel.lsp_log_editors.get(srv.name) {
                                for i in 0..ed.line_offsets.len() {
                                    if let Some(tgt) = skip_until {
                                        if i < tgt { continue; }
                                        skip_until = None;
                                    }
                                    lines += 1;
                                    if ed.folded_lines.contains(&i) {
                                        skip_until = Some(ed.foldable_lines[&i]);
                                    }
                                }
                            } else {
                                for entry in &srv.logs {
                                    lines += entry.text.split('\n').count();
                                }
                            }
                            logs_h = (lines as f32 * 16.0 * s).max(50.0 * s) + 20.0 * s;
                        }
                        let row_h = 136.0 * s + logs_h;

                        if srv.name == focused_name.as_str() && is_exp {
                            let card_x = cx + 12.0 * s;
                            let btn_y1 = cur_y + 56.0 * s;
                            let btn_y2 = btn_y1 + btn_h + 8.0 * s;
                            let log_bg_x = card_x + pad_x;
                            let log_bg_y = btn_y2 + btn_h + 10.0 * s;

                            let mut text_y = log_bg_y + 16.0 * s;
                            let mut global_line_count = 0;
                            let mut skip_until = None;
                            let line_h = 16.0 * s;
                            let my_drag = position.y as f32;

                            for entry in &srv.logs {
                                for line in entry.text.split('\n') {
                                    if let Some(tgt) = skip_until {
                                        if global_line_count < tgt {
                                            global_line_count += 1;
                                            continue;
                                        } else { skip_until = None; }
                                    }

                                    if my_drag >= text_y - line_h && my_drag <= text_y {
                                        if let Some(ed) = self.ide_panel.lsp_log_editors.get_mut(focused_name.as_str()) {
                                            let click_x_in_line = (position.x as f32 - log_bg_x - 20.0 * s + scroll_x).max(0.0);
                                            let r = self.renderer.as_mut().unwrap();
                                            let mut best_pos = 0usize;
                                            let mut best_dist = f32::MAX;
                                            let mut ci = 0usize;
                                            for c in line.chars() {
                                                let x = r.measure_mono_width(&line[..ci], 0.7);
                                                if (x - click_x_in_line).abs() < best_dist {
                                                    best_dist = (x - click_x_in_line).abs();
                                                    best_pos = ci;
                                                }
                                                ci += c.len_utf8();
                                            }
                                            let x_end = r.measure_mono_width(line, 0.7);
                                            if (x_end - click_x_in_line).abs() < best_dist {
                                                best_pos = line.len();
                                            }
                                            let line_start_byte = ed.line_offsets[global_line_count];
                                            let byte_off = (line_start_byte + best_pos).min(ed.len());

                                            if ed.selection_anchor.is_none() {
                                                ed.selection_anchor = Some(ed.cursor);
                                            }
                                            ed.cursor = byte_off;
                                        }
                                        break;
                                    }

                                    if let Some(ed) = self.ide_panel.lsp_log_editors.get(srv.name) {
                                        if ed.folded_lines.contains(&global_line_count) {
                                            skip_until = Some(ed.foldable_lines[&global_line_count]);
                                        }
                                    }

                                    text_y += line_h;
                                    global_line_count += 1;
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
            let mut lsp_bounds = None;
            let is_top = self.ide_panel.slots.iter().any(|sl| {
                sl.id == crate::app::PanelId::LspServers && sl.group == crate::app::PanelGroup::Top
            });
            if is_top {
                let sb_w = 48.0 * s;
                let title_h = 32.0 * s;
                let panel_left_w = self.ide_panel.left_width * s;
                let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                lsp_bounds = Some((sb_w, title_h, panel_left_w, wh - title_h));
            } else {
                let open_bottom: Vec<_> = self.ide_panel.slots.iter().filter(|sl| sl.group == crate::app::PanelGroup::Bottom && sl.open).collect();
                if let Some(first) = open_bottom.first() {
                    if first.id == crate::app::PanelId::LspServers {
                        let sb_w = 48.0 * s;
                        let tab_h = 32.0 * s;
                        let panel_bottom_h = self.ide_panel.bottom_height * s;
                        let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                        let ww = self.window.as_ref().unwrap().inner_size().width as f32;
                        lsp_bounds = Some((sb_w, wh - panel_bottom_h + 1.0 + tab_h, ww - sb_w, panel_bottom_h - 1.0 - tab_h));
                    }
                }
            }
            if let Some((cx, _, cw, _)) = lsp_bounds {
                let mut max_log_w = 0.0f32;
                for info in self.ide_panel.lsp_servers.iter() {
                    let is_expanded = self.ide_panel.lsp_logs_expanded.contains(info.name);
                                        if is_expanded {
                        for line in &info.logs {
                            let mut draw_str = line.text.as_str();
                            if draw_str.len() > 250 {
                                draw_str = &draw_str[..250];
                            }
                            let lw = self.renderer.as_mut().unwrap().measure_mono_width(draw_str, 0.7);
                            if lw > max_log_w { max_log_w = lw; }
                        }
                    }
                }
                let track_w = cw - 30.0 * s;
                let max_x = (max_log_w + 20.0 * s - (cw - 32.0 * s)).max(0.0);
                let thumb_w = (cw / (max_log_w + 20.0 * s) * track_w).max(40.0 * s);
                let ratio = (position.x as f32 - cx - 10.0 * s - self.ide_panel.lsp_scroll_x.drag_offset) / (track_w - thumb_w).max(0.0001);
                self.ide_panel.lsp_scroll_x.target = (ratio * max_x).clamp(0.0, max_x);
                self.ide_panel.lsp_scroll_x.current = self.ide_panel.lsp_scroll_x.target;
            }
        } else if self.ide_panel.lsp_scroll_y.is_dragging {
            let s = self.renderer.as_ref().unwrap().scale_factor;
                        let mut lsp_bounds = None;
                        let is_top = self.ide_panel.slots.iter().any(|sl| {
                            sl.id == crate::app::PanelId::LspServers && sl.group == crate::app::PanelGroup::Top
                        });
                        if is_top {
                            let sb_w = 48.0 * s;
                            let title_h = 32.0 * s;
                            let panel_left_w = self.ide_panel.left_width * s;
                            let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                            lsp_bounds = Some((sb_w, title_h, panel_left_w, wh - title_h));
                        } else {
                            let open_bottom: Vec<_> = self.ide_panel.slots.iter().filter(|sl| sl.group == crate::app::PanelGroup::Bottom && sl.open).collect();
                            if let Some(first) = open_bottom.first() {
                                if first.id == crate::app::PanelId::LspServers {
                                    let sb_w = 48.0 * s;
                                    let tab_h = 32.0 * s;
                                    let panel_bottom_h = self.ide_panel.bottom_height * s;
                                    let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                                    let ww = self.window.as_ref().unwrap().inner_size().width as f32;
                                    lsp_bounds = Some((sb_w, wh - panel_bottom_h + 1.0 + tab_h, ww - sb_w, panel_bottom_h - 1.0 - tab_h));
                                }
                            }
                        }
                                    if let Some((_, cy, _, ch)) = lsp_bounds {
                            let mut total_h = 8.0 * s;
                            for info in self.ide_panel.lsp_servers.iter() {
                                let is_expanded = self.ide_panel.lsp_logs_expanded.contains(info.name);
                                let mut logs_h = 0.0;
                                if is_expanded {
                                    let mut lines = 0;
                                    let mut skip_until = None;
                                    if let Some(ed) = self.ide_panel.lsp_log_editors.get(info.name) {
                                        for i in 0..ed.line_offsets.len() {
                                            if let Some(tgt) = skip_until {
                                                if i < tgt { continue; }
                                                skip_until = None;
                                            }
                                            lines += 1;
                                            if ed.folded_lines.contains(&i) {
                                                skip_until = Some(ed.foldable_lines[&i]);
                                            }
                                        }
                                    } else {
                                        for entry in &info.logs {
                                            lines += entry.text.split('\n').count();
                                        }
                                    }
                                    logs_h = (lines as f32 * 16.0 * s).max(50.0 * s) + 20.0 * s;
                                }
                                total_h += 136.0 * s + logs_h + 16.0 * s;
                            }
                let track_h = ch - 10.0 * s;
                let max_y = (total_h - ch).max(0.0);
                let thumb_h = (ch / total_h * track_h).max(40.0 * s);
                let ratio = (position.y as f32 - cy - 5.0 * s - self.ide_panel.lsp_scroll_y.drag_offset) / (track_h - thumb_h).max(0.0001);
                self.ide_panel.lsp_scroll_y.target = (ratio * max_y).clamp(0.0, max_y);
                self.ide_panel.lsp_scroll_y.current = self.ide_panel.lsp_scroll_y.target;
            }
        } else if self.is_dragging_search {
            let search_w = 480.0 * s;
            let search_x = scrollbar_x - search_w - 20.0 * s;
            let input_x = search_x + 10.0 * s;

            let text = self.search_editor.get_full_text();
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
            self.search_editor.cursor = target_idx;
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
                let total_content_height = (self.editor.line_offsets.len() as f32 + 2.0)
                    * self.renderer.as_ref().unwrap().line_height;
                let thumb_h = (wh / total_content_height.max(wh) * wh).max(20.0 * s);
                let track_h = wh;
                let track_start_y = 0.0;

                let last_mouse_y = self.renderer.as_ref().unwrap().last_mouse_y;

                let scroll_ratio = (last_mouse_y - track_start_y - self.scroll_y.drag_offset)
                    / (track_h - thumb_h).max(0.0001);

                self.scroll_y.target = (scroll_ratio * max_scroll).clamp(0.0, max_scroll).round();

                self.scroll_y.anim_speed = 15.0;
            }
        } else if self.is_dragging {
            let last_mouse_x = self.renderer.as_ref().unwrap().last_mouse_x;
            let last_mouse_y = self.renderer.as_ref().unwrap().last_mouse_y;
            self.editor.set_cursor_at_pos(
                last_mouse_x,
                last_mouse_y + self.scroll_y.current,
                self.renderer.as_mut().unwrap(),
                false,
            );
        }

        self.window.as_ref().unwrap().request_redraw();
    }

        pub fn handle_search_keyboard_input(&mut self, key_event: KeyEvent) {
        let ctrl = self.modifiers.control_key() || self.modifiers.super_key();
        let shift = self.modifiers.shift_key();
        let mut is_edit = false;

        match key_event.physical_key {
            PhysicalKey::Code(KeyCode::Escape) => {
                self.show_search = false;
                self.search_focused = false;
                self.search_results.clear();
                self.search_current_idx = None;
            }
            PhysicalKey::Code(KeyCode::KeyF) if ctrl => {
                self.search_editor.select_all();
            }
            PhysicalKey::Code(KeyCode::Enter) => {
                if !self.search_results.is_empty() {
                    if let Some(idx) = self.search_current_idx {
                        if shift {
                            self.search_current_idx = Some(if idx == 0 {
                                self.search_results.len() - 1
                            } else {
                                idx - 1
                            });
                        } else {
                            self.search_current_idx = Some((idx + 1) % self.search_results.len());
                        }
                    }
                    self.jump_to_search_result();
                }
            }
            PhysicalKey::Code(KeyCode::ArrowUp) => {
                if !self.search_results.is_empty() {
                    if let Some(idx) = self.search_current_idx {
                        self.search_current_idx = Some(if idx == 0 {
                            self.search_results.len() - 1
                        } else {
                            idx - 1
                        });
                    }
                    self.jump_to_search_result();
                }
            }
            PhysicalKey::Code(KeyCode::ArrowDown) => {
                if !self.search_results.is_empty() {
                    if let Some(idx) = self.search_current_idx {
                        self.search_current_idx = Some((idx + 1) % self.search_results.len());
                    }
                    self.jump_to_search_result();
                }
            }
            PhysicalKey::Code(KeyCode::ArrowLeft) => {
                if ctrl {
                    self.search_editor.move_word_left(shift);
                } else {
                    self.search_editor.move_left(shift);
                }
            }
            PhysicalKey::Code(KeyCode::ArrowRight) => {
                if ctrl {
                    self.search_editor.move_word_right(shift);
                } else {
                    self.search_editor.move_right(shift);
                }
            }
            PhysicalKey::Code(KeyCode::Home) => {
                self.search_editor.move_home(shift);
            }
            PhysicalKey::Code(KeyCode::End) => {
                self.search_editor.move_end(shift);
            }
            PhysicalKey::Code(KeyCode::KeyA) if ctrl => {
                self.search_editor.select_all();
            }
            PhysicalKey::Code(KeyCode::KeyC) if ctrl => {
                if let Some(text) = self.search_editor.get_selection() {
                    let _ = self.clipboard.set_text(text);
                }
            }
            PhysicalKey::Code(KeyCode::KeyX) if ctrl => {
                if let Some(text) = self.search_editor.get_selection() {
                    let _ = self.clipboard.set_text(text);
                    self.search_editor.delete_selection();
                    is_edit = true;
                }
            }
            PhysicalKey::Code(KeyCode::KeyV) if ctrl => {
                if let Ok(text) = self.clipboard.get_text() {
                    self.search_editor.insert_str(&text);
                    is_edit = true;
                }
            }
            PhysicalKey::Code(KeyCode::Backspace) => {
                if self.search_editor.backspace().is_some() {
                    is_edit = true;
                }
            }
            PhysicalKey::Code(KeyCode::Delete) => {
                if self.search_editor.delete_forward().is_some() {
                    is_edit = true;
                }
            }
            _ => {
                if !ctrl && !self.modifiers.alt_key() && !self.modifiers.super_key() {
                    if let Some(txt) = key_event.logical_key.to_text() {
                        let clean_txt = txt.replace('\n', "");
                        if !clean_txt.is_empty() {
                            self.search_editor.insert_str(&clean_txt);
                            is_edit = true;
                        }
                    }
                }
            }
        }
        if is_edit {
            self.search_editor.sync_edits.clear();
            self.update_search();
            self.jump_to_search_result();
        }
        self.last_action = Instant::now();
        self.window.as_ref().unwrap().request_redraw();
    }

        pub fn handle_editor_keyboard_input(
        &mut self,
        event_loop: &ActiveEventLoop,
        key_event: KeyEvent,
    ) {
        let ctrl = self.modifiers.control_key() || self.modifiers.super_key();
        let shift = self.modifiers.shift_key();

        if self.show_welcome {
            match key_event.physical_key {
                PhysicalKey::Code(KeyCode::KeyO) if ctrl => {
                    self.trigger_file_picker();
                }
                PhysicalKey::Code(KeyCode::KeyQ) if ctrl => {
                    let w = self.window.as_ref().unwrap();
                    let maximized = w.is_maximized();
                    let (width, height) = if maximized {
                        (self.window_width, self.window_height)
                    } else {
                        let scale = w.scale_factor();
                        let size = w.inner_size().to_logical::<f64>(scale);
                        (size.width, size.height)
                    };
                    crate::save_config(&crate::Config {
                        window_width: width,
                        window_height: height,
                        maximized,
                        ide_workspaces: self.ide_workspaces.clone(),
                        ide_ignore_patterns: self.ide_ignore_patterns.clone(),
                    });
                    event_loop.exit();
                }
                _ => {}
            }
            return;
        }

        if self.autocomplete_active && !self.autocomplete_options.is_empty() {
            match key_event.physical_key {
                PhysicalKey::Code(KeyCode::Escape)
                | PhysicalKey::Code(KeyCode::ArrowLeft)
                | PhysicalKey::Code(KeyCode::ArrowRight) => {
                    self.autocomplete_active = false;
                    self.autocomplete_selected_idx = 0;
                    self.window.as_ref().unwrap().request_redraw();
                    if matches!(key_event.physical_key, PhysicalKey::Code(KeyCode::Escape)) {
                        return;
                    }
                }
                PhysicalKey::Code(KeyCode::ArrowDown) => {
                    self.autocomplete_selected_idx =
                        (self.autocomplete_selected_idx + 1) % self.autocomplete_options.len();
                    self.ensure_autocomplete_visible();
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
                PhysicalKey::Code(KeyCode::ArrowUp) => {
                    if self.autocomplete_selected_idx == 0 {
                        self.autocomplete_selected_idx = self.autocomplete_options.len() - 1;
                    } else {
                        self.autocomplete_selected_idx -= 1;
                    }
                    self.ensure_autocomplete_visible();
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
                PhysicalKey::Code(KeyCode::Enter) | PhysicalKey::Code(KeyCode::Tab) => {
                    self.apply_autocomplete();
                    return;
                }
                _ => {}
            }
        }

        // Alt+Enter — меню быстрых действий LSP
        if self.modifiers.alt_key() {
            if let PhysicalKey::Code(KeyCode::Enter) = key_event.physical_key {
                self.open_lsp_actions_menu();
                return;
            }
        }

        // Навигация в открытом меню LSP
        if self.lsp_actions_menu.is_some() {
            match key_event.physical_key {
                PhysicalKey::Code(KeyCode::Escape) => {
                    self.lsp_actions_menu = None;
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
                PhysicalKey::Code(KeyCode::ArrowUp) => {
                    if let Some(menu) = &mut self.lsp_actions_menu {
                        if menu.selected > 0 {
                            menu.selected -= 1;
                        } else {
                            menu.selected = menu.items.len().saturating_sub(1);
                        }
                    }
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
                PhysicalKey::Code(KeyCode::ArrowDown) => {
                    if let Some(menu) = &mut self.lsp_actions_menu {
                        if !menu.items.is_empty() {
                            menu.selected = (menu.selected + 1) % menu.items.len();
                        }
                    }
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
                PhysicalKey::Code(KeyCode::Enter) | PhysicalKey::Code(KeyCode::NumpadEnter) => {
                    self.apply_selected_lsp_action();
                    return;
                }
                _ => {}
            }
        }

        let mut cursor_moved = false;
        let mut is_edit = false;
        let mut should_trigger_autocomplete = false;

        let old_cursor_y = self
            .renderer
            .as_mut()
            .unwrap()
            .get_cursor_xy(&self.editor)
            .1;

        match key_event.physical_key {
            PhysicalKey::Code(KeyCode::KeyQ) if ctrl => {
                if self.editor.is_dirty() {
                    self.show_action_dialog(event_loop, PendingAction::CloseFile);
                } else {
                    let w = self.window.as_ref().unwrap();
                    let maximized = w.is_maximized();
                    let (width, height) = if maximized {
                        (self.window_width, self.window_height)
                    } else {
                        let scale = w.scale_factor();
                        let size = w.inner_size().to_logical::<f64>(scale);
                        (size.width, size.height)
                    };
                    crate::save_config(&crate::Config {
                        window_width: width,
                        window_height: height,
                        maximized,
                        ide_workspaces: self.ide_workspaces.clone(),
                        ide_ignore_patterns: self.ide_ignore_patterns.clone(),
                    });
                    self.close_current_file();
                }
                return;
            }
            PhysicalKey::Code(KeyCode::F1) => {
                self.show_settings = true;
                return;
            }
            PhysicalKey::Code(KeyCode::KeyF) if ctrl => {
                self.show_search = true;
                self.search_focused = true;
                self.search_editor.select_all();
                self.search_current_idx = None;
                self.update_search();
                self.jump_to_search_result();

                self.window.as_ref().unwrap().request_redraw();
                return;
            }
            PhysicalKey::Code(KeyCode::Escape) => {
                if self.show_search {
                    self.show_search = false;
                    self.search_focused = false;
                    self.search_results.clear();
                    self.search_current_idx = None;
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
            }
            PhysicalKey::Code(KeyCode::KeyS) if ctrl => {
                if self.save_current_file() {
                    App::update_window_title(
                        self.window.as_ref().unwrap(),
                        &self.base_title,
                        self.editor.is_dirty(),
                    );
                }
            }
            PhysicalKey::Code(KeyCode::KeyO) if ctrl => {
                if self.editor.is_dirty() {
                    self.show_action_dialog(event_loop, PendingAction::OpenFile);
                } else {
                    self.trigger_file_picker();
                }
            }
            PhysicalKey::Code(KeyCode::KeyZ) if ctrl => {
                if let Some(delta) = self.editor.undo() {
                    match delta {
                        crate::editor::UndoRedoDelta::Insert(offset, len, text) => {
                            self.highlighter.shift_insert(offset, len, Some(&text));
                        }
                        crate::editor::UndoRedoDelta::Delete(offset, len) => {
                            self.highlighter.shift_delete(offset, len);
                        }
                    }
                    cursor_moved = true;
                    is_edit = true;
                }
            }
            PhysicalKey::Code(KeyCode::KeyY) if ctrl => {
                if let Some(delta) = self.editor.redo() {
                    match delta {
                        crate::editor::UndoRedoDelta::Insert(offset, len, text) => {
                            self.highlighter.shift_insert(offset, len, Some(&text));
                        }
                        crate::editor::UndoRedoDelta::Delete(offset, len) => {
                            self.highlighter.shift_delete(offset, len);
                        }
                    }
                    cursor_moved = true;
                    is_edit = true;
                }
            }
            PhysicalKey::Code(KeyCode::ArrowLeft) => {
                if ctrl {
                    self.editor.move_word_left(shift);
                } else {
                    self.editor.move_left(shift);
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::ArrowRight) => {
                if ctrl {
                    self.editor.move_word_right(shift);
                } else {
                    self.editor.move_right(shift);
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::ArrowUp) => {
                self.editor.move_up(self.renderer.as_mut().unwrap(), shift);
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::ArrowDown) => {
                self.editor
                    .move_down(self.renderer.as_mut().unwrap(), shift);
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::Home) => {
                if ctrl {
                    self.editor.move_start_of_file(shift);
                } else {
                    self.editor.move_home(shift);
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::End) => {
                if ctrl {
                    self.editor.move_end_of_file(shift);
                } else {
                    self.editor.move_end(shift);
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::PageUp) => {
                let step = self.window.as_ref().unwrap().inner_size().height as f32 * 0.8;
                self.scroll_y.scroll_by(-step);
                self.editor
                    .move_page_up(self.renderer.as_mut().unwrap(), shift, step);
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::PageDown) => {
                let step = self.window.as_ref().unwrap().inner_size().height as f32 * 0.8;
                self.scroll_y.scroll_by(step);
                self.editor
                    .move_page_down(self.renderer.as_mut().unwrap(), shift, step);
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::Backspace) if ctrl => {
                if let Some((offset, len)) = self.editor.delete_word_backward() {
                    self.highlighter.shift_delete(offset, len);
                    is_edit = true;
                    if self.autocomplete_active {
                        should_trigger_autocomplete = true;
                    }
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::Delete) if ctrl => {
                if let Some((offset, len)) = self.editor.delete_word_forward() {
                    self.highlighter.shift_delete(offset, len);
                    is_edit = true;
                    if self.autocomplete_active {
                        should_trigger_autocomplete = true;
                    }
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::Backspace) => {
                if let Some((offset, len)) = self.editor.backspace() {
                    self.highlighter.shift_delete(offset, len);
                    is_edit = true;
                    if self.autocomplete_active {
                        should_trigger_autocomplete = true;
                    }
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::Delete) => {
                if let Some((offset, len)) = self.editor.delete_forward() {
                    self.highlighter.shift_delete(offset, len);
                    is_edit = true;
                    if self.autocomplete_active {
                        should_trigger_autocomplete = true;
                    }
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::Enter) => {
                let indent = self.editor.get_auto_indent();
                let insert_text = format!("\n{}", indent);
                let (del_info, ins_len) = self.editor.insert_str(&insert_text);
                if let Some((offset, len)) = del_info {
                    self.highlighter.shift_delete(offset, len);
                }
                self.highlighter.shift_insert(
                    self.editor.cursor - ins_len,
                    ins_len,
                    Some(&insert_text),
                );
                cursor_moved = true;
                is_edit = true;
            }
            PhysicalKey::Code(KeyCode::Tab) => {
                let (del_info, ins_len) = self.editor.insert_str("    ");
                if let Some((offset, len)) = del_info {
                    self.highlighter.shift_delete(offset, len);
                }
                self.highlighter
                    .shift_insert(self.editor.cursor - ins_len, ins_len, Some("    "));
                cursor_moved = true;
                is_edit = true;
            }
            PhysicalKey::Code(KeyCode::Space) => {
                let (del_info, ins_len) = self.editor.insert_str(" ");
                if let Some((offset, len)) = del_info {
                    self.highlighter.shift_delete(offset, len);
                }
                self.highlighter
                    .shift_insert(self.editor.cursor - ins_len, ins_len, Some(" "));
                cursor_moved = true;
                is_edit = true;
            }
            PhysicalKey::Code(KeyCode::KeyW) if ctrl => {
                self.editor.select_expand();
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::KeyC) if ctrl => {
                if let Some(text) = self.editor.get_selection() {
                    let _ = self.clipboard.set_text(text);
                }
            }
            PhysicalKey::Code(KeyCode::KeyX) if ctrl => {
                if let Some(text) = self.editor.get_selection() {
                    let _ = self.clipboard.set_text(text);
                    if let Some((offset, len)) = self.editor.delete_selection() {
                        self.highlighter.shift_delete(offset, len);
                        is_edit = true;
                    }
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::KeyV) if ctrl => {
                if let Ok(text) = self.clipboard.get_text() {
                    let (del_info, ins_len) = self.editor.insert_str(&text);
                    if let Some((offset, len)) = del_info {
                        self.highlighter.shift_delete(offset, len);
                    }
                    self.highlighter.shift_insert(
                        self.editor.cursor - ins_len,
                        ins_len,
                        Some(&text),
                    );
                    is_edit = true;
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::KeyA) if ctrl => {
                self.editor.select_all();
                cursor_moved = true;
            }
            _ => {
                if !ctrl && !self.modifiers.alt_key() && !self.modifiers.super_key() {
                    if let Some(txt) = key_event.logical_key.to_text() {
                        let insert_txt = match txt {
                            "(" => "()",
                            "[" => "[]",
                            "{" => "{}",
                            _ => txt,
                        };
                        let (del_info, ins_len) = self.editor.insert_str(insert_txt);
                        if let Some((offset, len)) = del_info {
                            self.highlighter.shift_delete(offset, len);
                        }
                        self.highlighter.shift_insert(
                            self.editor.cursor - ins_len,
                            ins_len,
                            Some(insert_txt),
                        );
                        if txt == "(" || txt == "[" || txt == "{" {
                            self.editor.move_left(false);
                        }
                        cursor_moved = true;
                        is_edit = true;
                        should_trigger_autocomplete = true;
                    }
                }
            }
        }

        if cursor_moved && !is_edit {
            self.autocomplete_active = false;
            self.autocomplete_selected_idx = 0;
        }

        if is_edit {
            if should_trigger_autocomplete {
                self.update_autocomplete();
            } else {
                self.autocomplete_active = false;
                self.autocomplete_selected_idx = 0;
            }

            App::update_window_title(
                self.window.as_ref().unwrap(),
                &self.base_title,
                self.editor.is_dirty(),
            );
            if self.show_search && !self.search_editor.get_full_text().is_empty() {
                self.update_search();
            } else {
                self.search_results.clear();
            }

            if !self.editor.sync_edits.is_empty() {
                let edits = std::mem::take(&mut self.editor.sync_edits);
                // LSP didChange — отправляем полный текст только если файл Python и IDE режим
                if self.is_ide_mode {
                    if let (Some(lsp), Some(path)) = (&mut self.lsp, &self.file_path) {
                        let text = self.editor.get_full_text();
                        let ext = self.file_extension.clone();
                        let path = path.clone();
                        lsp.notify_change(&path, &ext, &text, self.editor.version as i32);
                    }
                }
                self.highlighter.apply_edits(self.editor.version, edits);
            }
            self.last_sent_version = self.editor.version;

            let start_wait = std::time::Instant::now();
            while start_wait.elapsed().as_millis() < 3 {
                if self.highlighter.poll(self.editor.version) {
                    self.editor.foldable_lines.clear();
                    self.editor.foldable_ranges_bytes.clear();
                    for &(start_b, end_b, is_autofold, is_sticky) in
                        &self.highlighter.foldable_ranges
                    {
                        self.editor
                            .foldable_ranges_bytes
                            .push((start_b, end_b, is_sticky));
                        let sl = self
                            .editor
                            .line_offsets
                            .partition_point(|&x| x <= start_b)
                            .saturating_sub(1);
                        let el = self
                            .editor
                            .line_offsets
                            .partition_point(|&x| x <= end_b)
                            .saturating_sub(1);
                        if el > sl {
                            self.editor.foldable_lines.insert(sl, el);
                            if is_autofold && el - sl >= 2 && !self.is_highlighted_once {
                                self.editor.folded_lines.insert(sl);
                                self.editor
                                    .folded_start_bytes
                                    .insert(self.editor.line_offsets[sl]);
                            }
                        }
                    }

                    self.is_highlighted_once = true;
                    if self.autocomplete_active {
                        self.update_autocomplete();
                    }
                    break;
                }
                std::thread::sleep(std::time::Duration::from_micros(500));
            }
        }

        if cursor_moved {
            let is_arrow = matches!(
                key_event.physical_key,
                PhysicalKey::Code(
                    KeyCode::ArrowUp
                        | KeyCode::ArrowDown
                        | KeyCode::ArrowLeft
                        | KeyCode::ArrowRight
                )
            );
            let is_page = matches!(
                key_event.physical_key,
                PhysicalKey::Code(KeyCode::PageUp | KeyCode::PageDown)
            );

            if is_arrow {
                self.scroll_y.anim_speed = 10.0;
                self.scroll_x.anim_speed = 10.0;
            } else if is_page {
                self.scroll_y.anim_speed = 7.0;
                self.scroll_x.anim_speed = 7.0;
            } else {
                self.scroll_y.anim_speed = 25.0;
                self.scroll_x.anim_speed = 25.0;
            }

            let wh_width = self.window.as_ref().unwrap().inner_size().width as f32;
            let wh_height = self.window.as_ref().unwrap().inner_size().height as f32;

            let is_enter_or_backspace = matches!(
                key_event.physical_key,
                PhysicalKey::Code(KeyCode::Enter | KeyCode::Backspace | KeyCode::Delete)
            );

            if is_enter_or_backspace && key_event.repeat {
                let new_cursor_y = self
                    .renderer
                    .as_mut()
                    .unwrap()
                    .get_cursor_xy(&self.editor)
                    .1;
                let delta_y = new_cursor_y - old_cursor_y;
                self.scroll_y.target += delta_y;
                self.scroll_y.current += delta_y;
                let max_scroll = self
                    .renderer
                    .as_mut()
                    .unwrap()
                    .get_max_scroll(&self.editor, wh_height);
                self.scroll_y.clamp_target(0.0, max_scroll);
                self.scroll_y.target = self.scroll_y.target.round();
                self.scroll_y.clamp_current(0.0, max_scroll);
            } else {
                let old_target_y = self.scroll_y.target;
                let old_target_x = self.scroll_x.target;

                App::ensure_cursor_visible(
                    &mut self.scroll_y.target,
                    &mut self.scroll_x.target,
                    &self.editor,
                    self.renderer.as_mut().unwrap(),
                    wh_width,
                    wh_height,
                );

                if key_event.repeat && !is_arrow && !is_page {
                    self.scroll_y.current += self.scroll_y.target - old_target_y;
                    self.scroll_x.current += self.scroll_x.target - old_target_x;
                }
            }
        }

        self.last_action = Instant::now();
        self.window.as_ref().unwrap().request_redraw();
    }

    /// Открывает меню быстрых действий LSP для текущей строки
    fn open_lsp_actions_menu(&mut self) {
        if !self.is_ide_mode || self.show_welcome {
            return;
        }
        let cursor = self.editor.cursor;
        let cursor_line = self
            .editor
            .line_offsets
            .partition_point(|&o| o <= cursor)
            .saturating_sub(1) as u32;

        // Собираем диагностики текущей строки
        let diags: Vec<crate::lsp::Diagnostic> = if let Some(lsp) = &self.lsp {
            lsp.diagnostics_for_line(cursor_line)
                .into_iter()
                .cloned()
                .collect()
        } else {
            Vec::new()
        };

        // Вычисляем позицию меню (под курсором)
        let (cx, cy) = self.renderer.as_mut().unwrap().get_cursor_xy(&self.editor);
        let _s = self.renderer.as_ref().unwrap().scale_factor;
        let menu_x = cx.max(self.renderer.as_ref().unwrap().left_padding);
        let menu_y = cy - self.scroll_y.current + self.renderer.as_ref().unwrap().line_height;

        // Начальные элементы: noqa варианты
        let mut items: Vec<crate::app::LspActionItem> = Vec::new();

        if !diags.is_empty() {
            // Сначала "Добавить # noqa: CODES" для конкретных кодов
            let codes: Vec<String> = diags.iter().filter_map(|d| d.code.clone()).collect();
            if !codes.is_empty() {
                items.push(crate::app::LspActionItem::AddNoqa {
                    codes: codes.clone(),
                });
            }
            // Затем "Добавить # noqa" (всё отключить)
            items.push(crate::app::LspActionItem::AddNoqaAll);
        }

        // Запрашиваем code actions от LSP
        let pending_id = if !diags.is_empty() {
            if let Some(lsp) = &mut self.lsp {
                let ext = self.file_extension.clone();
                let sl = cursor_line;
                let el = cursor_line;
                let sc = diags.iter().map(|d| d.start_col).min().unwrap_or(0);
                let ec = diags.iter().map(|d| d.end_col).max().unwrap_or(0);
                lsp.request_code_actions(&ext, sl, sc, el, ec, &diags)
            } else {
                None
            }
        } else {
            None
        };

        if items.is_empty() && pending_id.is_none() {
            return; // нечего показывать
        }

        self.lsp_actions_menu = Some(crate::app::LspActionsMenu {
            cursor_line,
            items,
            selected: 0,
            menu_x,
            menu_y,
            pending_request_id: pending_id,
        });

        self.window.as_ref().unwrap().request_redraw();
    }

    /// Применяет выбранный элемент меню LSP
    pub fn apply_selected_lsp_action(&mut self) {
        let menu = match self.lsp_actions_menu.take() {
            Some(m) => m,
            None => return,
        };
        if menu.items.is_empty() {
            return;
        }
        let item = menu.items[menu.selected.min(menu.items.len() - 1)].clone();

        match item {
            crate::app::LspActionItem::CodeAction(action) => {
                if let (Some(edit), Some(path)) = (action.edit, self.file_path.clone()) {
                    let new_text = crate::lsp::apply_workspace_edit_to_text(
                        &self.editor.get_full_text(),
                        &edit,
                        &path,
                    );
                    if new_text != self.editor.get_full_text() {
                        self.apply_full_text_replacement(new_text);
                    }
                }
            }
            crate::app::LspActionItem::AddNoqa { codes } => {
                self.insert_noqa_comment(menu.cursor_line, &codes);
            }
            crate::app::LspActionItem::AddNoqaAll => {
                self.insert_noqa_comment(menu.cursor_line, &[]);
            }
        }

        self.window.as_ref().unwrap().request_redraw();
    }

    /// Вставляет/обновляет # noqa комментарий на указанной строке
    fn insert_noqa_comment(&mut self, line: u32, codes: &[String]) {
        let line = line as usize;
        let line_end = if line + 1 < self.editor.line_offsets.len() {
            self.editor.line_offsets[line + 1] - 1 // позиция перед \n
        } else {
            self.editor.len()
        };

        // Читаем текущую строку
        let line_start = self.editor.line_offsets.get(line).copied().unwrap_or(0);
        let mut line_bytes = Vec::with_capacity(line_end - line_start);
        for i in line_start..line_end {
            line_bytes.push(self.editor.byte_at(i));
        }
        let line_text = String::from_utf8_lossy(&line_bytes);

        // Вычисляем куда вставить
        if let Some(noqa_pos_in_line) = line_text.find("# noqa") {
            // Уже есть noqa — добавляем коды если нужно
            if codes.is_empty() {
                return; // Уже есть # noqa, всё ок
            }
            // Парсим существующие коды
            let noqa_byte_start = line_start + noqa_pos_in_line;
            let noqa_text = &line_text[noqa_pos_in_line..];
            let existing = if let Some(colon) = noqa_text.find(": ") {
                noqa_text[colon + 2..]
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };

            let mut merged = existing.clone();
            for code in codes {
                if !merged.contains(code) {
                    merged.push(code.clone());
                }
            }

            // Заменяем noqa блок
            let new_noqa = format!("# noqa: {}", merged.join(", "));
            let old_noqa_len = line_text.len() - noqa_pos_in_line;

            // Удаляем старый noqa
            self.editor.cursor = noqa_byte_start;
            for _ in 0..old_noqa_len {
                let _ = self.editor.delete_forward();
            }
            // Вставляем новый
            let ins_start = self.editor.cursor;
            let (del_info, ins_len) = self.editor.insert_str(&new_noqa);
            if let Some((off, len)) = del_info {
                self.highlighter.shift_delete(off, len);
            }
            self.highlighter
                .shift_insert(ins_start, ins_len, Some(&new_noqa));
        } else {
            // Нет noqa — добавляем в конец строки
            self.editor.cursor = line_end;
            let noqa = if codes.is_empty() {
                "  # noqa".to_string()
            } else {
                format!("  # noqa: {}", codes.join(", "))
            };
            let ins_start = self.editor.cursor;
            let (del_info, ins_len) = self.editor.insert_str(&noqa);
            if let Some((off, len)) = del_info {
                self.highlighter.shift_delete(off, len);
            }
            self.highlighter
                .shift_insert(ins_start, ins_len, Some(&noqa));
        }

        // Синхронизируем с LSP и подсветчиком
        if !self.editor.sync_edits.is_empty() {
            let edits = std::mem::take(&mut self.editor.sync_edits);
            if self.is_ide_mode {
                if let (Some(lsp), Some(path)) = (&mut self.lsp, &self.file_path) {
                    let text = self.editor.get_full_text();
                    let ext = self.file_extension.clone();
                    let path = path.clone();
                    lsp.notify_change(&path, &ext, &text, self.editor.version as i32);
                }
            }
            self.highlighter.apply_edits(self.editor.version, edits);
        }

        App::update_window_title(
            self.window.as_ref().unwrap(),
            &self.base_title,
            self.editor.is_dirty(),
        );
    }

    /// Заменяет весь текст редактора новым (для workspace edit)
    pub(crate) fn apply_full_text_replacement(&mut self, new_text: String) {
        let version = self.editor.version + 1;
        self.editor = crate::editor::Editor::new(new_text.len() + 8192);
        self.editor.version = version;
        let _ = self.editor.insert_str(&new_text);
        self.editor.cursor = 0;
        self.editor.set_original_text();
        self.highlighter
            .reset(version, new_text.clone(), self.file_extension.clone());
        if let (Some(lsp), Some(path)) = (&mut self.lsp, &self.file_path) {
            lsp.notify_change(
                path,
                &self.file_extension.clone(),
                &new_text,
                version as i32,
            );
        }
        App::update_window_title(self.window.as_ref().unwrap(), &self.base_title, true);
    }

    pub fn handle_main_keyboard_input(
        &mut self,
        event_loop: &ActiveEventLoop,
        key_event: KeyEvent,
    ) {
        if self.dialog_window.is_some() {
            if key_event.state == ElementState::Pressed {
                if key_event.physical_key == PhysicalKey::Code(KeyCode::Escape) {
                    self.close_dialog();
                } else {
                    if let Some(dw) = self.dialog_window.as_ref() {
                        dw.focus_window();
                        dw.request_redraw();
                    }
                }
            }
            return;
        }

        if key_event.state == ElementState::Pressed {
            // ── Ввод в поле игнора настроек ──────────────────────────────
            if self.show_settings && self.settings_tab == 0 && self.settings_ignore_focused {
                self.last_action = std::time::Instant::now();
                let ctrl = self.modifiers.control_key();
                let shift = self.modifiers.shift_key();
                match key_event.physical_key {
                    PhysicalKey::Code(KeyCode::Escape) => {
                        self.settings_ignore_focused = false;
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                    PhysicalKey::Code(KeyCode::Enter) | PhysicalKey::Code(KeyCode::NumpadEnter) => {
                        let trimmed = self
                            .settings_ignore_editor
                            .get_full_text()
                            .trim()
                            .to_string();
                        if !trimmed.is_empty() && !self.ide_ignore_patterns.contains(&trimmed) {
                            self.ide_ignore_patterns.push(trimmed);
                            self.settings_ignore_editor.select_all();
                            self.settings_ignore_editor.delete_selection();
                            let w = self.window.as_ref().unwrap();
                            let maximized = w.is_maximized();
                            crate::save_config(&crate::Config {
                                window_width: self.window_width,
                                window_height: self.window_height,
                                maximized,
                                ide_workspaces: self.ide_workspaces.clone(),
                                ide_ignore_patterns: self.ide_ignore_patterns.clone(),
                            });
                            self.refresh_file_tree();
                        }
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                    PhysicalKey::Code(KeyCode::KeyA) if ctrl => {
                        self.settings_ignore_editor.select_all();
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                    PhysicalKey::Code(KeyCode::KeyC) if ctrl => {
                        if let Some(text) = self.settings_ignore_editor.get_selection() {
                            let _ = self.clipboard.set_text(text);
                        }
                        return;
                    }
                    PhysicalKey::Code(KeyCode::KeyX) if ctrl => {
                        if let Some(text) = self.settings_ignore_editor.get_selection() {
                            let _ = self.clipboard.set_text(text);
                            self.settings_ignore_editor.delete_selection();
                            self.window.as_ref().unwrap().request_redraw();
                        }
                        return;
                    }
                    PhysicalKey::Code(KeyCode::KeyV) if ctrl => {
                        if let Ok(text) = self.clipboard.get_text() {
                            let clean = text.replace('\n', "").replace('\r', "");
                            if !clean.is_empty() {
                                self.settings_ignore_editor.insert_str(&clean);
                                self.window.as_ref().unwrap().request_redraw();
                            }
                        }
                        return;
                    }
                    PhysicalKey::Code(KeyCode::Backspace) => {
                        if ctrl {
                            self.settings_ignore_editor.delete_word_backward();
                        } else {
                            self.settings_ignore_editor.backspace();
                        }
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                    PhysicalKey::Code(KeyCode::Delete) => {
                        if ctrl {
                            self.settings_ignore_editor.delete_word_forward();
                        } else {
                            self.settings_ignore_editor.delete_forward();
                        }
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                    PhysicalKey::Code(KeyCode::ArrowLeft) => {
                        if ctrl {
                            self.settings_ignore_editor.move_word_left(shift);
                        } else {
                            self.settings_ignore_editor.move_left(shift);
                        }
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                    PhysicalKey::Code(KeyCode::ArrowRight) => {
                        if ctrl {
                            self.settings_ignore_editor.move_word_right(shift);
                        } else {
                            self.settings_ignore_editor.move_right(shift);
                        }
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                    PhysicalKey::Code(KeyCode::Home) => {
                        self.settings_ignore_editor.move_home(shift);
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                    PhysicalKey::Code(KeyCode::End) => {
                        self.settings_ignore_editor.move_end(shift);
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                    _ => {
                        if !ctrl && !self.modifiers.alt_key() && !self.modifiers.super_key() {
                            if let Some(txt) = key_event.logical_key.to_text() {
                                let clean_txt = txt.replace('\n', "");
                                if !clean_txt.is_empty() {
                                    self.settings_ignore_editor.insert_str(&clean_txt);
                                    self.window.as_ref().unwrap().request_redraw();
                                    return;
                                }
                            }
                        }
                    }
                }
            }

            if let PhysicalKey::Code(KeyCode::Escape) = key_event.physical_key {
                if self.show_settings {
                    self.show_settings = false;
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
            }

            if self.show_settings {
                return;
            }

            if let PhysicalKey::Code(KeyCode::F8) = key_event.physical_key {
                self.show_fps = !self.show_fps;
                self.window.as_ref().unwrap().request_redraw();
                return;
            }

                        if let Some(focused_name) = self.ide_panel.lsp_logs_focused.clone() {
                if let Some(ed) = self.ide_panel.lsp_log_editors.get_mut(&focused_name) {
                    let ctrl = self.modifiers.control_key() || self.modifiers.super_key();
                    let shift = self.modifiers.shift_key();
                    match key_event.physical_key {
                        PhysicalKey::Code(KeyCode::KeyC) if ctrl => {
                            if let Some(text) = ed.get_selection() {
                                let _ = self.clipboard.set_text(text);
                            }
                            return;
                        }
                        PhysicalKey::Code(KeyCode::KeyA) if ctrl => {
                            ed.select_all();
                            self.window.as_ref().unwrap().request_redraw();
                            return;
                        }
                        PhysicalKey::Code(KeyCode::ArrowLeft) => {
                            if ctrl {
                                ed.move_word_left(shift);
                            } else {
                                ed.move_left(shift);
                            }
                            self.window.as_ref().unwrap().request_redraw();
                            return;
                        }
                        PhysicalKey::Code(KeyCode::ArrowRight) => {
                            if ctrl {
                                ed.move_word_right(shift);
                            } else {
                                ed.move_right(shift);
                            }
                            self.window.as_ref().unwrap().request_redraw();
                            return;
                        }
                        PhysicalKey::Code(KeyCode::Escape) => {
                            self.ide_panel.lsp_logs_focused = None;
                            self.window.as_ref().unwrap().request_redraw();
                            return;
                        }
                        _ => {}
                    }
                }
            }

            if self.search_focused {
                self.handle_search_keyboard_input(key_event);
            } else {
                self.handle_editor_keyboard_input(event_loop, key_event);
            }
        }
    }
}
