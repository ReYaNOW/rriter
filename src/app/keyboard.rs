use crate::app::{App, PendingAction};
use crate::editor::Editor;
use std::time::Instant;
use winit::event::{ElementState, KeyEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};

impl App {
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
        let physical_key = key_event.physical_key;
        let is_dot = key_event.logical_key.to_text() == Some(".");

        if self.show_welcome {
            match physical_key {
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
            match physical_key {
                PhysicalKey::Code(KeyCode::Escape)
                | PhysicalKey::Code(KeyCode::ArrowLeft)
                | PhysicalKey::Code(KeyCode::ArrowRight) => {
                    self.autocomplete_active = false;
                    self.autocomplete_selected_idx = 0;
                    self.window.as_ref().unwrap().request_redraw();
                    if matches!(physical_key, PhysicalKey::Code(KeyCode::Escape)) {
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
            if let PhysicalKey::Code(KeyCode::Enter) = physical_key {
                self.open_lsp_actions_menu();
                return;
            }
        }

        // Навигация в открытом меню LSP
        if self.lsp_actions_menu.is_some() {
            match physical_key {
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
        let mut should_sync = true;

        let old_cursor_y = self
            .renderer
            .as_mut()
            .unwrap()
            .get_cursor_xy(&self.editor)
            .1;

        match physical_key {
            PhysicalKey::Code(KeyCode::KeyQ) if ctrl => {
                if self.is_ide_mode {
                    // TODO: Спросить о сохранении несохраненных вкладок
                    // Закрываем все вкладки и возвращаемся на Welcome Screen

                    // 1. Уведомить LSP о закрытии всех файлов
                    self.sync_active_tab(); // Синхронизируем последнюю активную вкладку
                    if let Some(lsp) = &mut self.lsp {
                        for tab in &self.tabs {
                            if let Some(p) = &tab.file_path {
                                lsp.notify_close(p, &tab.file_extension);
                            }
                        }
                    }

                    // 2. Очистить все вкладки и сбросить состояние редактора до "пустого"
                    self.tabs.clear();
                    self.active_tab = 0;
                    self.file_path = None;
                    self.base_title = "Добро пожаловать".to_string();
                    self.editor = Editor::new(8192);
                    self.editor.set_original_text();
                    self.highlighter
                        .reset(self.editor.version, "".to_string(), "".to_string());
                    self.show_welcome = true;
                    self.autocomplete_active = false;
                    self.scroll_y.stop_anim();
                    self.scroll_x.stop_anim();
                    self.save_tabs_state();
                } else {
                    if self.editor.is_dirty() {
                        self.show_action_dialog(event_loop, PendingAction::CloseFile);
                    } else {
                        self.close_current_file();
                    }
                }
                return;
            }
            PhysicalKey::Code(KeyCode::F1) => {
                self.show_settings = !self.show_settings;
                self.is_dragging = false;
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
                        crate::editor::UndoRedoDelta::Replace(
                            offset,
                            del_len,
                            old_text,
                            _new_text,
                        ) => {
                            self.highlighter.shift_delete(offset, del_len);
                            self.highlighter
                                .shift_insert(offset, old_text.len(), Some(&old_text));
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
                        crate::editor::UndoRedoDelta::Replace(
                            offset,
                            del_len,
                            new_text,
                            _old_text,
                        ) => {
                            self.highlighter.shift_delete(offset, del_len);
                            self.highlighter
                                .shift_insert(offset, new_text.len(), Some(&new_text));
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
                should_sync = false;
            }
            PhysicalKey::Code(KeyCode::Digit4) if ctrl => {
                self.close_tab_at(self.active_tab);
                return;
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
                self.autocomplete_active = false;
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

                        if txt.chars().all(|c| c.is_alphanumeric() || c == '_') || txt == "." {
                            should_trigger_autocomplete = true;
                        }
                        if txt == "=" {
                            should_sync = false;
                        }
                    }
                }
            }
        }

        if cursor_moved && !is_edit {
            self.autocomplete_active = false;
            self.autocomplete_selected_idx = 0;
            self.lsp_actions_menu = None;
        }

        if is_edit {
            self.lsp_actions_menu = None;
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

            if should_sync {
                if !self.editor.sync_edits.is_empty() {
                    let edits = std::mem::take(&mut self.editor.sync_edits);
                    let is_backspace_or_delete = matches!(
                        physical_key,
                        PhysicalKey::Code(KeyCode::Backspace | KeyCode::Delete)
                    );
                    // LSP didChange — отправляем полный текст только если файл Python и IDE режим
                    if self.is_ide_mode {
                        if !is_backspace_or_delete && !is_dot {
                            if let (Some(lsp), Some(path)) = (&mut self.lsp, &self.file_path) {
                                let text = self.editor.get_full_text();
                                let ext = self.file_extension.clone();
                                let path = path.clone();
                                lsp.notify_change(&path, &ext, &text, self.editor.version as i32);
                            }
                        } else {
                            if let Some(lsp) = &mut self.lsp {
                                lsp.diagnostics.clear();
                                lsp.suppress_diagnostics = true;
                            }
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
        }

        if cursor_moved {
            let is_arrow = matches!(
                physical_key,
                PhysicalKey::Code(
                    KeyCode::ArrowUp
                        | KeyCode::ArrowDown
                        | KeyCode::ArrowLeft
                        | KeyCode::ArrowRight
                )
            );
            let is_page = matches!(
                physical_key,
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
                physical_key,
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

                let tab_bar_h = if self.show_welcome {
                    0.0
                } else {
                    38.0 * self.renderer.as_ref().unwrap().scale_factor
                };
                App::ensure_cursor_visible(
                    &mut self.scroll_y.target,
                    &mut self.scroll_x.target,
                    &self.editor,
                    self.renderer.as_mut().unwrap(),
                    wh_width,
                    wh_height,
                    tab_bar_h,
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
                if let PhysicalKey::Code(KeyCode::F1) = key_event.physical_key {
                    self.show_settings = false;
                    self.window.as_ref().unwrap().request_redraw();
                }
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
