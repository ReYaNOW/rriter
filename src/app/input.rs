use crate::app::{App, PendingAction};
use crate::editor::Editor;
use crate::widgets::IconButton;
use std::time::Instant;
use winit::event::{ElementState, KeyEvent, MouseScrollDelta};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::CursorIcon;

impl App {
    pub fn handle_main_mouse_wheel(&mut self, delta: MouseScrollDelta) {
        if self.show_quit_dialog || self.show_welcome {
            return;
        }
        self.scroll_anim_speed = 7.0;
        let lh = self.renderer.as_ref().unwrap().line_height;
        match delta {
            MouseScrollDelta::LineDelta(_, y) => {
                self.target_scroll_y -= y * 4.0 * lh;
            }
            MouseScrollDelta::PixelDelta(pos) => {
                self.target_scroll_y -= pos.y as f32;
            }
        }
        let wh = self.window.as_ref().unwrap().inner_size().height as f32;
        let max_scroll = self
            .renderer
            .as_mut()
            .unwrap()
            .get_max_scroll(&self.editor, wh);
        self.target_scroll_y = self.target_scroll_y.clamp(0.0, max_scroll).round();
        self.window.as_ref().unwrap().request_redraw();
    }

    pub fn handle_main_mouse_input(&mut self, state: ElementState) {
        if self.show_quit_dialog {
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

                // Точно отзеркаливаем математику Y-координат из ui.rs
                let mut y = content_y + 60.0 * s;
                y += 40.0 * s; // "Добро пожаловать в RRiter"
                y += 60.0 * s; // "Молниеносный текстовый редактор..."

                let (btn_new, btn_open) = crate::widgets::get_welcome_buttons(
                    cw,
                    title_x,
                    y, // Y для кнопок, теперь полностью совпадает
                    s,
                    self.renderer.as_mut().unwrap(),
                );

                if btn_new.is_hovered(last_mouse_x, last_mouse_y) {
                    self.show_welcome = false;
                    self.file_path = None;
                    self.base_title = "Безымянный".to_string();
                    let old_version = self.editor.version;
                    self.editor = Editor::new(8192);
                    self.editor.version = old_version + 1;
                    self.editor.set_original_text();
                    self.highlighter.request_update(
                        self.editor.version,
                        "".to_string(),
                        "".to_string(),
                    );
                    App::update_window_title(
                        self.window.as_ref().unwrap(),
                        &self.base_title,
                        false,
                    );
                } else if btn_open.is_hovered(last_mouse_x, last_mouse_y) {
                    self.trigger_file_picker();
                } else {
                    y += 80.0 * s; // "Недавние файлы"
                    y += 35.0 * s; // Отступ после заголовка и линии

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
                        self.load_file(p);
                    }
                }
            }
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        if state == ElementState::Released {
            self.is_dragging = false;
            self.is_dragging_minimap = false;
            self.is_dragging_search = false;
            self.target_scroll_y = self.target_scroll_y.round();
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        if state == ElementState::Pressed {
            let last_mouse_x = self.renderer.as_ref().unwrap().last_mouse_x;
            let last_mouse_y = self.renderer.as_ref().unwrap().last_mouse_y;
            let minimap_w = self.renderer.as_ref().unwrap().minimap_width;
            let window_width = self.window.as_ref().unwrap().inner_size().width as f32;
            let s = self.renderer.as_ref().unwrap().scale_factor;

            if self.show_search && self.search_anim_y > -10.0 {
                let search_w = 480.0 * s;
                let search_h = 46.0 * s;
                let search_x = window_width - minimap_w - search_w - 20.0 * s;

                if last_mouse_x >= search_x
                    && last_mouse_x <= search_x + search_w
                    && last_mouse_y >= self.search_anim_y
                    && last_mouse_y <= self.search_anim_y + search_h
                {
                    let input_x = search_x + 10.0 * s;
                    let input_w = 260.0 * s;
                    let btn_y = self.search_anim_y + 8.0 * s;
                    let btn_size = 30.0 * s;

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
                        let mut current_btn_x = input_x + input_w + 10.0 * s;
                        let btn_case = IconButton {
                            x: current_btn_x,
                            y: btn_y,
                            size: btn_size,
                            icon: None,
                            is_active: false,
                        };
                        current_btn_x += btn_size + 4.0 * s;
                        let btn_up = IconButton {
                            x: current_btn_x,
                            y: btn_y,
                            size: btn_size,
                            icon: None,
                            is_active: false,
                        };
                        current_btn_x += btn_size + 4.0 * s;
                        let btn_down = IconButton {
                            x: current_btn_x,
                            y: btn_y,
                            size: btn_size,
                            icon: None,
                            is_active: false,
                        };
                        current_btn_x += btn_size + 8.0 * s;
                        let btn_close = IconButton {
                            x: current_btn_x,
                            y: btn_y,
                            size: btn_size,
                            icon: None,
                            is_active: false,
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

            if last_mouse_x >= window_width - minimap_w {
                let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                let max_scroll = self
                    .renderer
                    .as_mut()
                    .unwrap()
                    .get_max_scroll(&self.editor, wh);

                let total_lines_f32 =
                    self.renderer.as_ref().unwrap().visual_lines.len().max(1) as f32;
                let minimap_line_h = (wh / total_lines_f32).min(3.0);
                let track_h = (total_lines_f32 * minimap_line_h).min(wh);
                let visible_lines = wh / self.renderer.as_ref().unwrap().line_height;
                let viewport_h = (visible_lines * minimap_line_h).max(10.0).min(track_h);

                let scroll_ratio = if max_scroll > 0.0 {
                    (self.scroll_y / max_scroll).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let current_viewport_y = scroll_ratio * (track_h - viewport_h).max(0.0);

                self.is_dragging_minimap = true;
                self.last_click_pos = (last_mouse_x, last_mouse_y);
                self.last_click_time = Instant::now();

                if last_mouse_y >= current_viewport_y
                    && last_mouse_y <= current_viewport_y + viewport_h
                {
                    self.minimap_drag_offset_y = last_mouse_y - current_viewport_y;
                } else {
                    self.minimap_drag_offset_y = viewport_h / 2.0;
                    let new_scroll_ratio = (last_mouse_y - self.minimap_drag_offset_y)
                        / (track_h - viewport_h).max(0.0001);
                    self.target_scroll_y = (new_scroll_ratio * max_scroll)
                        .clamp(0.0, max_scroll)
                        .round();
                    self.scroll_y = self.target_scroll_y;
                }
            } else {
                self.is_dragging = true;
                self.scroll_anim_speed = 15.0;

                self.target_scroll_y = self.scroll_y.round();
                self.scroll_y = self.target_scroll_y;
                self.scroll_velocity = 0.0;

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
                    last_mouse_y + self.scroll_y,
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
        if self.show_quit_dialog {
            return;
        }

        self.renderer.as_mut().unwrap().last_mouse_x = position.x as f32;
        self.renderer.as_mut().unwrap().last_mouse_y = position.y as f32;

        if self.show_welcome {
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        let minimap_w = self.renderer.as_ref().unwrap().minimap_width;
        let padding = self.renderer.as_ref().unwrap().left_padding;
        let window_size = self.window.as_ref().unwrap().inner_size();
        let s = self.renderer.as_ref().unwrap().scale_factor;

        let mut is_text_cursor = false;
        if position.x as f32 > padding
            && (position.x as f32) < (window_size.width as f32 - minimap_w)
        {
            is_text_cursor = true;
        }

        if self.show_search && self.search_anim_y > -10.0 {
            let search_w = 480.0 * s;
            let search_h = 46.0 * s;
            let search_x = window_size.width as f32 - minimap_w - search_w - 20.0 * s;
            let input_x = search_x + 10.0 * s;
            let input_y = self.search_anim_y + 8.0 * s;
            let input_w = 260.0 * s;
            let input_h = 30.0 * s;

            if position.x as f32 >= search_x
                && position.x as f32 <= search_x + search_w
                && position.y as f32 >= self.search_anim_y
                && position.y as f32 <= self.search_anim_y + search_h
            {
                self.window.as_ref().unwrap().request_redraw();

                if position.x as f32 >= input_x
                    && position.x as f32 <= input_x + input_w
                    && position.y as f32 >= input_y
                    && position.y as f32 <= input_y + input_h
                {
                    is_text_cursor = true;
                } else {
                    is_text_cursor = false;
                }
            }
        }

        if is_text_cursor {
            self.window.as_ref().unwrap().set_cursor(CursorIcon::Text);
        } else {
            self.window
                .as_ref()
                .unwrap()
                .set_cursor(CursorIcon::Default);
        }

        if self.is_dragging_search {
            let search_w = 426.0 * s;
            let search_x = window_size.width as f32 - minimap_w - search_w - 20.0 * s;
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
            self.window.as_ref().unwrap().request_redraw();
        } else if self.is_dragging_minimap {
            let now = std::time::Instant::now();
            let elapsed = now.duration_since(self.last_click_time).as_millis();
            let dy = (position.y as f32 - self.last_click_pos.1).abs();

            if elapsed > 120 || dy > 10.0 {
                let wh = window_size.height as f32;
                let max_scroll = self
                    .renderer
                    .as_mut()
                    .unwrap()
                    .get_max_scroll(&self.editor, wh);

                let total_lines_f32 =
                    self.renderer.as_ref().unwrap().visual_lines.len().max(1) as f32;
                let minimap_line_h = (wh / total_lines_f32).min(3.0);
                let track_h = (total_lines_f32 * minimap_line_h).min(wh);
                let visible_lines = wh / self.renderer.as_ref().unwrap().line_height;
                let viewport_h = (visible_lines * minimap_line_h).max(10.0).min(track_h);

                let last_mouse_y = self.renderer.as_ref().unwrap().last_mouse_y;

                let scroll_ratio = (last_mouse_y - self.minimap_drag_offset_y)
                    / (track_h - viewport_h).max(0.0001);

                self.target_scroll_y = (scroll_ratio * max_scroll).clamp(0.0, max_scroll).round();

                self.scroll_anim_speed = 15.0;
                self.window.as_ref().unwrap().request_redraw();
            }
        } else if self.is_dragging {
            let last_mouse_x = self.renderer.as_ref().unwrap().last_mouse_x;
            let last_mouse_y = self.renderer.as_ref().unwrap().last_mouse_y;
            self.editor.set_cursor_at_pos(
                last_mouse_x,
                last_mouse_y + self.scroll_y,
                self.renderer.as_mut().unwrap(),
                false,
            );
            self.window.as_ref().unwrap().request_redraw();
        }
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
                self.search_editor.move_left(shift);
            }
            PhysicalKey::Code(KeyCode::ArrowRight) => {
                self.search_editor.move_right(shift);
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
                    self.search_editor.delete_selection(&[]);
                    is_edit = true;
                }
            }
            PhysicalKey::Code(KeyCode::KeyV) if ctrl => {
                if let Ok(text) = self.clipboard.get_text() {
                    self.search_editor.insert_str(&text, &[]);
                    is_edit = true;
                }
            }
            PhysicalKey::Code(KeyCode::Backspace) => {
                if self.search_editor.backspace(&[]).is_some() {
                    is_edit = true;
                }
            }
            PhysicalKey::Code(KeyCode::Delete) => {
                if self.search_editor.delete_forward(&[]).is_some() {
                    is_edit = true;
                }
            }
            _ => {
                if !ctrl && !self.modifiers.alt_key() && !self.modifiers.super_key() {
                    if let Some(txt) = key_event.logical_key.to_text() {
                        let clean_txt = txt.replace('\n', "");
                        if !clean_txt.is_empty() {
                            self.search_editor.insert_str(&clean_txt, &[]);
                            is_edit = true;
                        }
                    }
                }
            }
        }
        if is_edit {
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
                    event_loop.exit();
                }
                _ => {}
            }
            return;
        }

        let mut cursor_moved = false;
        let mut is_edit = false;
        let mut typed_dot = false;
        let old_cursor_y = self
            .renderer
            .as_mut()
            .unwrap()
            .get_cursor_xy(&self.editor)
            .1;

        match key_event.physical_key {
            PhysicalKey::Code(KeyCode::F1) => {
                self.faq_scroll_y = 0.0;
                self.faq_target_scroll_y = 0.0;
                self.show_action_dialog(event_loop, PendingAction::Faq);
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
                if let Some(delta) = self.editor.undo(&self.highlighter.spans) {
                    match delta {
                        crate::editor::UndoRedoDelta::Insert(offset, len, text, restored) => {
                            self.highlighter
                                .shift_insert(offset, len, Some(&text), restored);
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
                        crate::editor::UndoRedoDelta::Insert(offset, len, text, restored) => {
                            self.highlighter
                                .shift_insert(offset, len, Some(&text), restored);
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
                self.editor.move_left(shift);
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::ArrowRight) => {
                self.editor.move_right(shift);
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
                self.target_scroll_y -= step;
                self.editor
                    .move_page_up(self.renderer.as_mut().unwrap(), shift, step);
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::PageDown) => {
                let step = self.window.as_ref().unwrap().inner_size().height as f32 * 0.8;
                self.target_scroll_y += step;
                self.editor
                    .move_page_down(self.renderer.as_mut().unwrap(), shift, step);
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::Backspace) if ctrl => {
                if let Some((offset, len)) =
                    self.editor.delete_word_backward(&self.highlighter.spans)
                {
                    self.highlighter.shift_delete(offset, len);
                    is_edit = true;
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::Delete) if ctrl => {
                if let Some((offset, len)) =
                    self.editor.delete_word_forward(&self.highlighter.spans)
                {
                    self.highlighter.shift_delete(offset, len);
                    is_edit = true;
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::Backspace) => {
                if let Some((offset, len)) = self.editor.backspace(&self.highlighter.spans) {
                    self.highlighter.shift_delete(offset, len);
                    is_edit = true;
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::Delete) => {
                if let Some((offset, len)) = self.editor.delete_forward(&self.highlighter.spans) {
                    self.highlighter.shift_delete(offset, len);
                    is_edit = true;
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::Enter) => {
                let indent = self.editor.get_auto_indent();
                let insert_text = format!("\n{}", indent);
                let (del_info, ins_len) = self
                    .editor
                    .insert_str(&insert_text, &self.highlighter.spans);
                if let Some((offset, len)) = del_info {
                    self.highlighter.shift_delete(offset, len);
                }
                self.highlighter.shift_insert(
                    self.editor.cursor - ins_len,
                    ins_len,
                    Some(&insert_text),
                    None,
                );
                cursor_moved = true;
                is_edit = true;
            }
            PhysicalKey::Code(KeyCode::Tab) => {
                let (del_info, ins_len) = self.editor.insert_str("    ", &self.highlighter.spans);
                if let Some((offset, len)) = del_info {
                    self.highlighter.shift_delete(offset, len);
                }
                self.highlighter.shift_insert(
                    self.editor.cursor - ins_len,
                    ins_len,
                    Some("    "),
                    None,
                );
                cursor_moved = true;
                is_edit = true;
            }
            PhysicalKey::Code(KeyCode::Space) => {
                let (del_info, ins_len) = self.editor.insert_str(" ", &self.highlighter.spans);
                if let Some((offset, len)) = del_info {
                    self.highlighter.shift_delete(offset, len);
                }
                self.highlighter.shift_insert(
                    self.editor.cursor - ins_len,
                    ins_len,
                    Some(" "),
                    None,
                );
                cursor_moved = true;
                is_edit = true;
            }
            PhysicalKey::Code(KeyCode::KeyC) if ctrl => {
                if let Some(text) = self.editor.get_selection() {
                    let _ = self.clipboard.set_text(text);
                }
            }
            PhysicalKey::Code(KeyCode::KeyX) if ctrl => {
                if let Some(text) = self.editor.get_selection() {
                    let _ = self.clipboard.set_text(text);
                    if let Some((offset, len)) =
                        self.editor.delete_selection(&self.highlighter.spans)
                    {
                        self.highlighter.shift_delete(offset, len);
                        is_edit = true;
                    }
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::KeyV) if ctrl => {
                if let Ok(text) = self.clipboard.get_text() {
                    let (del_info, ins_len) =
                        self.editor.insert_str(&text, &self.highlighter.spans);
                    if let Some((offset, len)) = del_info {
                        self.highlighter.shift_delete(offset, len);
                    }
                    self.highlighter.shift_insert(
                        self.editor.cursor - ins_len,
                        ins_len,
                        Some(&text),
                        None,
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
                        if txt == "." {
                            typed_dot = true;
                        }
                        let insert_txt = match txt {
                            "(" => "()",
                            "[" => "[]",
                            "{" => "{}",
                            _ => txt,
                        };
                        let (del_info, ins_len) =
                            self.editor.insert_str(insert_txt, &self.highlighter.spans);
                        if let Some((offset, len)) = del_info {
                            self.highlighter.shift_delete(offset, len);
                        }
                        self.highlighter.shift_insert(
                            self.editor.cursor - ins_len,
                            ins_len,
                            Some(insert_txt),
                            None,
                        );
                        if txt == "(" || txt == "[" || txt == "{" {
                            self.editor.move_left(false);
                        }
                        cursor_moved = true;
                        is_edit = true;
                    }
                }
            }
        }

        if typed_dot {
            self.skip_highlight_update = true;
        } else if is_edit {
            self.skip_highlight_update = false;
        }

        if is_edit {
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

            if !self.skip_highlight_update {
                self.highlighter.request_update(
                    self.editor.version,
                    self.editor.get_full_text(),
                    self.file_extension.clone(),
                );
                self.last_sent_version = self.editor.version;

                let start_wait = std::time::Instant::now();
                while start_wait.elapsed().as_millis() < 20 {
                    if self.highlighter.poll(self.editor.version) {
                        self.is_highlighted_once = true;
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
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
                self.scroll_anim_speed = 10.0;
            } else if is_page {
                self.scroll_anim_speed = 7.0;
            } else {
                self.scroll_anim_speed = 25.0;
            }

            let wh = self.window.as_ref().unwrap().inner_size().height as f32;
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
                self.target_scroll_y += delta_y;
                self.scroll_y += delta_y;
                let max_scroll = self
                    .renderer
                    .as_mut()
                    .unwrap()
                    .get_max_scroll(&self.editor, wh);
                self.target_scroll_y = self.target_scroll_y.clamp(0.0, max_scroll).round();
                self.scroll_y = self.scroll_y.clamp(0.0, max_scroll);
            } else {
                let old_target = self.target_scroll_y;
                App::ensure_cursor_visible(
                    &mut self.target_scroll_y,
                    &self.editor,
                    self.renderer.as_mut().unwrap(),
                    wh,
                );

                if key_event.repeat && !is_arrow && !is_page {
                    self.scroll_y += self.target_scroll_y - old_target;
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
        if self.show_quit_dialog {
            if let Some(dw) = &self.dialog_window {
                dw.focus_window();
            }
            return;
        }

        if key_event.state == ElementState::Pressed {
            if self.search_focused {
                self.handle_search_keyboard_input(key_event);
            } else {
                self.handle_editor_keyboard_input(event_loop, key_event);
            }
        }
    }
}
