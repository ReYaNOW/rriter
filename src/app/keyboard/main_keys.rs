use super::*;

#[cfg_attr(coverage_nightly, coverage(off))]
impl App {
    pub fn handle_main_keyboard_input(
        &mut self,
        event_loop: &ActiveEventLoop,
        key_event: KeyEvent,
    ) {
        if key_event.state == ElementState::Pressed
            && self.modifiers.alt_key()
            && key_event.physical_key == PhysicalKey::Code(KeyCode::KeyQ)
        {
            if self.is_ide_mode {
                let shift = self.modifiers.shift_key();
                let is_open = self.ide_panel.is_open(crate::app::PanelId::Terminal);

                if shift {
                    if is_open {
                        if let Some(slot) = self
                            .ide_panel
                            .slots
                            .iter_mut()
                            .find(|s| s.id == crate::app::PanelId::Terminal)
                        {
                            slot.open = false;
                        }
                        self.ide_panel.terminal_focused = false;
                    } else {
                        if let Some(slot) = self
                            .ide_panel
                            .slots
                            .iter_mut()
                            .find(|s| s.id == crate::app::PanelId::Terminal)
                        {
                            slot.open = true;
                        }
                        if self.ide_panel.terminals.is_empty() {
                            self.ide_panel
                                .terminals
                                .push(crate::app::terminal::Terminal::spawn(self.window.clone()));
                            self.ide_panel.active_terminal = 0;
                        }
                        self.ide_panel.terminal_focused = true;
                    }
                } else {
                    if !is_open {
                        if let Some(slot) = self
                            .ide_panel
                            .slots
                            .iter_mut()
                            .find(|s| s.id == crate::app::PanelId::Terminal)
                        {
                            slot.open = true;
                        }
                        if self.ide_panel.terminals.is_empty() {
                            self.ide_panel
                                .terminals
                                .push(crate::app::terminal::Terminal::spawn(self.window.clone()));
                            self.ide_panel.active_terminal = 0;
                        }
                        self.ide_panel.terminal_focused = true;
                    } else {
                        self.ide_panel.terminal_focused = !self.ide_panel.terminal_focused;
                    }
                }

                self.last_action = std::time::Instant::now();
                if let Some(w) = self.window.as_ref() {
                    w.request_redraw();
                }
                return;
            }
        }

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
            if key_event.physical_key == PhysicalKey::Code(KeyCode::Escape)
                && crate::app::mouse::clear_hover_popup(self.renderer.as_mut())
            {
                self.window.as_ref().unwrap().request_redraw();
            }
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
                                enable_telemetry: crate::render_view::TELEMETRY_ENABLED
                                    .load(std::sync::atomic::Ordering::Relaxed),
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

            let term_focused = self.is_ide_mode
                && self.ide_panel.terminal_focused
                && self.ide_panel.is_open(crate::app::PanelId::Terminal);
            if let PhysicalKey::Code(KeyCode::F8) = key_event.physical_key {
                if !term_focused {
                    self.show_fps = !self.show_fps;
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
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

            if self.ide_panel.term_show_search && self.ide_panel.term_search_focused {
                self.handle_terminal_search_keyboard_input(key_event);
            } else if self.show_search && self.search_focused {
                self.handle_search_keyboard_input(key_event);
            } else if self.is_ide_mode
                && self.ide_panel.terminal_focused
                && self.ide_panel.is_open(crate::app::PanelId::Terminal)
            {
                self.handle_terminal_keyboard_input(key_event);
            } else {
                self.handle_editor_keyboard_input(event_loop, key_event);
            }
        }
    }
}
