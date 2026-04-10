use crate::app::{App, PendingAction};
use crate::editor::Editor;
use crate::widgets::IconButton;
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

        if self.autocomplete_active && self.autocomplete_rect.is_some() {
            let (rx, ry, rw, rh) = self.autocomplete_rect.unwrap();
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
            if state == ElementState::Pressed {
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
                    let pad_top = 35.0 * s;
                    let pad_h = 40.0 * s;
                    let ix = x + pad_h;
                    let iy = y + pad_top;
                    let sidebar_w = 200.0 * s;
                    if mx >= ix + 10.0 * s && mx <= ix + sidebar_w - 10.0 * s {
                        let mut tab_y = iy + 20.0 * s;
                        for i in 0..5 {
                            if my >= tab_y && my <= tab_y + 36.0 * s {
                                self.settings_tab = i;
                                break;
                            }
                            tab_y += 40.0 * s;
                        }
                    } else if self.settings_tab == 0 {
                        let mut loop_btn_y = iy + 86.0 * s;
                        for (idx, _path) in self.ide_workspaces.clone().iter().enumerate() {
                            if mx >= ix + sidebar_w + 30.0 * s + 300.0 * s
                                && mx <= ix + sidebar_w + 30.0 * s + 330.0 * s
                                && my >= loop_btn_y
                                && my <= loop_btn_y + 24.0 * s
                            {
                                self.ide_workspaces.remove(idx);
                                let w = self.window.as_ref().unwrap();
                                let maximized = w.is_maximized();
                                crate::save_config(&crate::Config {
                                    window_width: self.window_width,
                                    window_height: self.window_height,
                                    maximized,
                                    ide_workspaces: self.ide_workspaces.clone(),
                                });
                                break;
                            }
                            loop_btn_y += 34.0 * s;
                        }

                        let add_btn_y = iy + 86.0 * s + self.ide_workspaces.len() as f32 * 34.0 * s;
                        if mx >= ix + sidebar_w + 30.0 * s
                            && mx <= ix + sidebar_w + 30.0 * s + 190.0 * s
                            && my >= add_btn_y
                            && my <= add_btn_y + 36.0 * s
                        {
                            self.trigger_folder_picker();
                        }
                    }
                }
            }
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        if self.show_welcome {
            if state == ElementState::Pressed {
                let last_mouse_x = self.renderer.as_ref().unwrap().last_mouse_x;
                let last_mouse_y = self.renderer.as_ref().unwrap().last_mouse_y;
                let s = self.renderer.as_ref().unwrap().scale_factor;

                let content_x = 40.0 * s;
                let content_y = 40.0 * s;
                let title_x = content_x + 40.0 * s;
                let cw = self.window.as_ref().unwrap().inner_size().width as f32 - 80.0 * s;

                let mut y = content_y + 60.0 * s;
                y += 40.0 * s;
                y += 60.0 * s;

                let (btn_new, btn_open, btn_ide) = crate::widgets::get_welcome_buttons(
                    cw,
                    title_x,
                    y,
                    s,
                    self.renderer.as_mut().unwrap(),
                );

                if btn_new.is_hovered(last_mouse_x, last_mouse_y) {
                    self.show_welcome = false;
                    self.is_ide_mode = false;
                    self.file_path = None;
                    self.base_title = "Безымянный".to_string();
                    let old_version = self.editor.version;
                    self.editor = Editor::new(8192);
                    self.editor.version = old_version + 1;
                    self.editor.set_original_text();
                    self.editor.sync_edits.clear();
                    self.highlighter
                        .reset(self.editor.version, "".to_string(), "".to_string());
                    App::update_window_title(
                        self.window.as_ref().unwrap(),
                        &self.base_title,
                        false,
                    );
                } else if btn_open.is_hovered(last_mouse_x, last_mouse_y) {
                    self.is_ide_mode = false;
                    self.trigger_file_picker();
                } else if btn_ide.is_hovered(last_mouse_x, last_mouse_y) {
                    self.show_welcome = false;
                    self.is_ide_mode = true;
                    self.file_path = None;
                    self.base_title = "Режим IDE".to_string();
                    let old_version = self.editor.version;
                    self.editor = Editor::new(8192);
                    self.editor.version = old_version + 1;
                    self.editor.set_original_text();
                    self.editor.sync_edits.clear();
                    self.highlighter
                        .reset(self.editor.version, "".to_string(), "".to_string());
                    App::update_window_title(
                        self.window.as_ref().unwrap(),
                        &self.base_title,
                        false,
                    );
                } else {
                    y += 80.0 * s;
                    y += 35.0 * s;

                    let mut current_y = y;
                    let item_h = 44.0 * s;
                    let mut selected_path = None;

                    for path in &self.recent_files {
                        if last_mouse_x >= title_x - 10.0 * s
                            && last_mouse_x <= title_x + cw - 70.0 * s
                            && last_mouse_y >= current_y
                            && last_mouse_y < current_y + item_h
                        {
                            selected_path = Some(path.clone());
                            break;
                        }
                        current_y += item_h;
                    }
                    if let Some(p) = selected_path {
                        self.is_ide_mode = false;
                        self.load_file(p);
                    }
                }
            }
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        if state == ElementState::Released {
            self.is_dragging = false;
            self.scroll_y.is_dragging = false;
            self.is_dragging_search = false;
            self.autocomplete_scroll.is_dragging = false;
            self.scroll_x.is_dragging = false;
            self.scroll_y.target = self.scroll_y.target.round();
            self.scroll_x.target = self.scroll_x.target.round();
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        if state == ElementState::Pressed {
            let last_mouse_x = self.renderer.as_ref().unwrap().last_mouse_x;
            let last_mouse_y = self.renderer.as_ref().unwrap().last_mouse_y;
            let s = self.renderer.as_ref().unwrap().scale_factor;

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
                    let btn_y = self.search_anim_y + 8.0 * s;
                    let btn_size = 36.0 * s;

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
                    } else {
                        let mut current_btn_x = search_x + search_w - 10.0 * s;

                                            current_btn_x -= btn_size;
                    let btn_close = IconButton {
                        x: current_btn_x,
                        y: btn_y,
                        size: btn_size,
                        icon: None,
                        is_active: false,
                        icon_size: Some(26.0 * s),
                        active_square_width: None,
                    };
                        current_btn_x -= 10.0 * s;

                                            current_btn_x -= btn_size;
                    let btn_down = IconButton {
                        x: current_btn_x,
                        y: btn_y,
                        size: btn_size,
                        icon: None,
                        is_active: false,
                        icon_size: Some(26.0 * s),
                        active_square_width: None,
                    };
                        current_btn_x -= 10.0 * s;

                                            current_btn_x -= btn_size;
                    let btn_up = IconButton {
                        x: current_btn_x,
                        y: btn_y,
                        size: btn_size,
                        icon: None,
                        is_active: false,
                        icon_size: Some(26.0 * s),
                        active_square_width: None,
                    };
                        current_btn_x -= 10.0 * s;

                                            current_btn_x -= btn_size;
                    let btn_case = IconButton {
                        x: current_btn_x,
                        y: btn_y,
                        size: btn_size,
                        icon: None,
                        is_active: false,
                        icon_size: Some(26.0 * s),
                        active_square_width: None,
                    };

                        if btn_case.is_hovered(last_mouse_x, last_mouse_y) {
                            self.search_case_sensitive = !self.search_case_sensitive;
                            self.update_search();
                            self.jump_to_search_result();
                        } else if btn_up.is_hovered(last_mouse_x, last_mouse_y) {
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
                        } else if btn_down.is_hovered(last_mouse_x, last_mouse_y) {
                            if !self.search_results.is_empty() {
                                if let Some(idx) = self.search_current_idx {
                                    self.search_current_idx =
                                        Some((idx + 1) % self.search_results.len());
                                }
                                self.jump_to_search_result();
                            }
                        } else if btn_close.is_hovered(last_mouse_x, last_mouse_y) {
                            self.show_search = false;
                            self.search_focused = false;
                            self.search_results.clear();
                            self.search_current_idx = None;
                        }
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

        if self.autocomplete_active && self.autocomplete_rect.is_some() {
            let (rx, ry, rw, rh) = self.autocomplete_rect.unwrap();
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
                self.window
                    .as_ref()
                    .unwrap()
                    .set_cursor(winit::window::CursorIcon::Pointer);

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

        if self.is_dragging_search {
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
        let ctrl = self.modifiers.control_key();
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
        let ctrl = self.modifiers.control_key();
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
            if let PhysicalKey::Code(KeyCode::Escape) = key_event.physical_key {
                if self.show_settings {
                    self.show_settings = false;
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
            }

            if let PhysicalKey::Code(KeyCode::F8) = key_event.physical_key {
                self.show_fps = !self.show_fps;
                self.window.as_ref().unwrap().request_redraw();
                return;
            }

            if self.search_focused {
                self.handle_search_keyboard_input(key_event);
            } else {
                self.handle_editor_keyboard_input(event_loop, key_event);
            }
        }
    }
}
