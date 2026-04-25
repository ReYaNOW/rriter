use crate::app::{App, PendingAction};
use crate::editor::Editor;
use std::io::Write;
use std::time::Instant;
use winit::event::{ElementState, KeyEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};

mod editor_keys;
mod main_keys;
impl App {
    pub fn handle_terminal_keyboard_input(&mut self, key_event: KeyEvent) {
        let ctrl = self.modifiers.control_key() || self.modifiers.super_key();

        if key_event.state == winit::event::ElementState::Pressed
            && ctrl
            && key_event.physical_key == PhysicalKey::Code(KeyCode::KeyF)
        {
            self.ide_panel.term_show_search = true;
            self.ide_panel.term_search_focused = true;
            self.ide_panel.term_search_editor.select_all();
            self.ide_panel.term_search_current_idx = None;
            self.update_terminal_search();
            self.jump_to_terminal_search_result();
            self.last_action = std::time::Instant::now();
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        let active = self.ide_panel.active_terminal;
        if let Some(term) = self.ide_panel.terminals.get_mut(active) {
            let mut grid = term.grid.lock().unwrap();
            if key_event.state == winit::event::ElementState::Pressed {
                let mut w = term.writer.lock().unwrap();
                match key_event.physical_key {
                    PhysicalKey::Code(KeyCode::KeyC) if ctrl => {
                        if let Some((sx, sy, ex, ey)) = grid.selection {
                            let mut res = String::new();
                            let scrollback_len = if grid.is_alt {
                                0
                            } else {
                                grid.scrollback.len()
                            };
                            let total_lines = scrollback_len + grid.lines.len();
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

                            for y in start_y..=end_y {
                                if y >= total_lines {
                                    continue;
                                }
                                let row = if grid.is_alt {
                                    &grid.lines[y]
                                } else {
                                    if y < grid.scrollback.len() {
                                        &grid.scrollback[y]
                                    } else {
                                        &grid.lines[y - grid.scrollback.len()]
                                    }
                                };
                                let line_start = if y == start_y { start_x } else { 0 };
                                let line_end = if y == end_y {
                                    end_x
                                } else {
                                    grid.cols.saturating_sub(1)
                                };

                                for x in line_start..=line_end {
                                    if x < row.len() {
                                        res.push(row[x].c);
                                    }
                                }
                                if y != end_y {
                                    res.push('\n');
                                }
                            }

                            let _ = self.clipboard.set_text(res.trim_end().to_string());
                            grid.selection = None;
                        } else {
                            let _ = w.write_all(b"\x03");
                        }
                    }
                    PhysicalKey::Code(KeyCode::KeyV) if ctrl => {
                        if let Ok(text) = self.clipboard.get_text() {
                            let _ = w.write_all(text.as_bytes());
                        }
                    }
                    PhysicalKey::Code(KeyCode::Enter) | PhysicalKey::Code(KeyCode::NumpadEnter) => {
                        let _ = w.write_all(b"\r");
                    }
                    PhysicalKey::Code(KeyCode::Backspace) => {
                        let _ = w.write_all(b"\x08");
                    }
                    PhysicalKey::Code(KeyCode::Tab) => {
                        let _ = w.write_all(b"\t");
                    }
                    PhysicalKey::Code(KeyCode::F1) => {
                        let _ = w.write_all(b"\x1bOP");
                    }
                    PhysicalKey::Code(KeyCode::F2) => {
                        let _ = w.write_all(b"\x1bOQ");
                    }
                    PhysicalKey::Code(KeyCode::F3) => {
                        let _ = w.write_all(b"\x1bOR");
                    }
                    PhysicalKey::Code(KeyCode::F4) => {
                        let _ = w.write_all(b"\x1bOS");
                    }
                    PhysicalKey::Code(KeyCode::F5) => {
                        let _ = w.write_all(b"\x1b[15~");
                    }
                    PhysicalKey::Code(KeyCode::F6) => {
                        let _ = w.write_all(b"\x1b[17~");
                    }
                    PhysicalKey::Code(KeyCode::F7) => {
                        let _ = w.write_all(b"\x1b[18~");
                    }
                    PhysicalKey::Code(KeyCode::F8) => {
                        let _ = w.write_all(b"\x1b[19~");
                    }
                    PhysicalKey::Code(KeyCode::F9) => {
                        let _ = w.write_all(b"\x1b[20~");
                    }
                    PhysicalKey::Code(KeyCode::F10) => {
                        let _ = w.write_all(b"\x1b[21~");
                    }
                    PhysicalKey::Code(KeyCode::F11) => {
                        let _ = w.write_all(b"\x1b[23~");
                    }
                    PhysicalKey::Code(KeyCode::F12) => {
                        let _ = w.write_all(b"\x1b[24~");
                    }
                    PhysicalKey::Code(KeyCode::ArrowUp) => {
                        let _ = w.write_all(if grid.is_alt { b"\x1bOA" } else { b"\x1b[A" });
                    }
                    PhysicalKey::Code(KeyCode::ArrowDown) => {
                        let _ = w.write_all(if grid.is_alt { b"\x1bOB" } else { b"\x1b[B" });
                    }
                    PhysicalKey::Code(KeyCode::ArrowLeft) => {
                        let _ = w.write_all(if grid.is_alt { b"\x1bOD" } else { b"\x1b[D" });
                    }
                    PhysicalKey::Code(KeyCode::ArrowRight) => {
                        let _ = w.write_all(if grid.is_alt { b"\x1bOC" } else { b"\x1b[C" });
                    }
                    PhysicalKey::Code(KeyCode::Escape) => {
                        let _ = w.write_all(b"\x1b");
                    }
                    PhysicalKey::Code(KeyCode::KeyA) if ctrl => {
                        let _ = w.write_all(b"\x01");
                    }
                    PhysicalKey::Code(KeyCode::KeyB) if ctrl => {
                        let _ = w.write_all(b"\x02");
                    }
                    PhysicalKey::Code(KeyCode::KeyD) if ctrl => {
                        let _ = w.write_all(b"\x04");
                    }
                    PhysicalKey::Code(KeyCode::KeyE) if ctrl => {
                        let _ = w.write_all(b"\x05");
                    }
                    PhysicalKey::Code(KeyCode::KeyG) if ctrl => {
                        let _ = w.write_all(b"\x07");
                    }
                    PhysicalKey::Code(KeyCode::KeyH) if ctrl => {
                        let _ = w.write_all(b"\x08");
                    }
                    PhysicalKey::Code(KeyCode::KeyI) if ctrl => {
                        let _ = w.write_all(b"\x09");
                    }
                    PhysicalKey::Code(KeyCode::KeyJ) if ctrl => {
                        let _ = w.write_all(b"\x0a");
                    }
                    PhysicalKey::Code(KeyCode::KeyK) if ctrl => {
                        let _ = w.write_all(b"\x0b");
                    }
                    PhysicalKey::Code(KeyCode::KeyL) if ctrl => {
                        let _ = w.write_all(b"\x0c");
                    }
                    PhysicalKey::Code(KeyCode::KeyM) if ctrl => {
                        let _ = w.write_all(b"\x0d");
                    }
                    PhysicalKey::Code(KeyCode::KeyN) if ctrl => {
                        let _ = w.write_all(b"\x0e");
                    }
                    PhysicalKey::Code(KeyCode::KeyO) if ctrl => {
                        let _ = w.write_all(b"\x0f");
                    }
                    PhysicalKey::Code(KeyCode::KeyP) if ctrl => {
                        let _ = w.write_all(b"\x10");
                    }
                    PhysicalKey::Code(KeyCode::KeyQ) if ctrl => {
                        let _ = w.write_all(b"\x11");
                    }
                    PhysicalKey::Code(KeyCode::KeyR) if ctrl => {
                        let _ = w.write_all(b"\x12");
                    }
                    PhysicalKey::Code(KeyCode::KeyS) if ctrl => {
                        let _ = w.write_all(b"\x13");
                    }
                    PhysicalKey::Code(KeyCode::KeyT) if ctrl => {
                        let _ = w.write_all(b"\x14");
                    }
                    PhysicalKey::Code(KeyCode::KeyU) if ctrl => {
                        let _ = w.write_all(b"\x15");
                    }
                    PhysicalKey::Code(KeyCode::KeyW) if ctrl => {
                        let _ = w.write_all(b"\x17");
                    }
                    PhysicalKey::Code(KeyCode::KeyX) if ctrl => {
                        let _ = w.write_all(b"\x18");
                    }
                    PhysicalKey::Code(KeyCode::KeyY) if ctrl => {
                        let _ = w.write_all(b"\x19");
                    }
                    PhysicalKey::Code(KeyCode::KeyZ) if ctrl => {
                        let _ = w.write_all(b"\x1a");
                    }
                    _ => {
                        if !ctrl && !self.modifiers.alt_key() && !self.modifiers.super_key() {
                            if let Some(txt) = key_event.logical_key.to_text() {
                                let _ = w.write_all(txt.as_bytes());
                            }
                        }
                    }
                }
                w.flush().ok();
            }
        }
        self.last_action = std::time::Instant::now();
        self.window.as_ref().unwrap().request_redraw();
    }

    pub fn handle_terminal_search_keyboard_input(&mut self, key_event: KeyEvent) {
        if key_event.state == winit::event::ElementState::Pressed {
            let ctrl = self.modifiers.control_key() || self.modifiers.super_key();
            let shift = self.modifiers.shift_key();
            let mut is_edit = false;

            match key_event.physical_key {
                PhysicalKey::Code(KeyCode::Escape) => {
                    self.ide_panel.term_show_search = false;
                    self.ide_panel.term_search_focused = false;
                    self.ide_panel.term_search_results.clear();
                    self.ide_panel.term_search_current_idx = None;
                    if let Some(term) = self
                        .ide_panel
                        .terminals
                        .get_mut(self.ide_panel.active_terminal)
                    {
                        term.grid.lock().unwrap().selection = None;
                    }
                }
                PhysicalKey::Code(KeyCode::Enter) => {
                    if !self.ide_panel.term_search_results.is_empty() {
                        if let Some(idx) = self.ide_panel.term_search_current_idx {
                            if shift {
                                self.ide_panel.term_search_current_idx = Some(if idx == 0 {
                                    self.ide_panel.term_search_results.len() - 1
                                } else {
                                    idx - 1
                                });
                            } else {
                                self.ide_panel.term_search_current_idx =
                                    Some((idx + 1) % self.ide_panel.term_search_results.len());
                            }
                        }
                        self.jump_to_terminal_search_result();
                    }
                }
                PhysicalKey::Code(KeyCode::ArrowUp) => {
                    if !self.ide_panel.term_search_results.is_empty() {
                        if let Some(idx) = self.ide_panel.term_search_current_idx {
                            self.ide_panel.term_search_current_idx = Some(if idx == 0 {
                                self.ide_panel.term_search_results.len() - 1
                            } else {
                                idx - 1
                            });
                        }
                        self.jump_to_terminal_search_result();
                    }
                }
                PhysicalKey::Code(KeyCode::ArrowDown) => {
                    if !self.ide_panel.term_search_results.is_empty() {
                        if let Some(idx) = self.ide_panel.term_search_current_idx {
                            self.ide_panel.term_search_current_idx =
                                Some((idx + 1) % self.ide_panel.term_search_results.len());
                        }
                        self.jump_to_terminal_search_result();
                    }
                }
                PhysicalKey::Code(KeyCode::ArrowLeft) => {
                    if ctrl {
                        self.ide_panel.term_search_editor.move_word_left(shift);
                    } else {
                        self.ide_panel.term_search_editor.move_left(shift);
                    }
                }
                PhysicalKey::Code(KeyCode::ArrowRight) => {
                    if ctrl {
                        self.ide_panel.term_search_editor.move_word_right(shift);
                    } else {
                        self.ide_panel.term_search_editor.move_right(shift);
                    }
                }
                PhysicalKey::Code(KeyCode::Home) => {
                    self.ide_panel.term_search_editor.move_home(shift);
                }
                PhysicalKey::Code(KeyCode::End) => {
                    self.ide_panel.term_search_editor.move_end(shift);
                }
                PhysicalKey::Code(KeyCode::KeyA) if ctrl => {
                    self.ide_panel.term_search_editor.select_all();
                }
                PhysicalKey::Code(KeyCode::KeyC) if ctrl => {
                    if let Some(text) = self.ide_panel.term_search_editor.get_selection() {
                        let _ = self.clipboard.set_text(text);
                    }
                }
                PhysicalKey::Code(KeyCode::KeyX) if ctrl => {
                    if let Some(text) = self.ide_panel.term_search_editor.get_selection() {
                        let _ = self.clipboard.set_text(text);
                        self.ide_panel.term_search_editor.delete_selection();
                        is_edit = true;
                    }
                }
                PhysicalKey::Code(KeyCode::KeyV) if ctrl => {
                    if let Ok(text) = self.clipboard.get_text() {
                        self.ide_panel.term_search_editor.insert_str(&text);
                        is_edit = true;
                    }
                }
                PhysicalKey::Code(KeyCode::Backspace) => {
                    if self.ide_panel.term_search_editor.backspace().is_some() {
                        is_edit = true;
                    }
                }
                PhysicalKey::Code(KeyCode::Delete) => {
                    if self.ide_panel.term_search_editor.delete_forward().is_some() {
                        is_edit = true;
                    }
                }
                _ => {
                    if !ctrl && !self.modifiers.alt_key() && !self.modifiers.super_key() {
                        if let Some(txt) = key_event.logical_key.to_text() {
                            let clean_txt = txt.replace('\n', "");
                            if !clean_txt.is_empty() {
                                self.ide_panel.term_search_editor.insert_str(&clean_txt);
                                is_edit = true;
                            }
                        }
                    }
                }
            }
            if is_edit {
                self.ide_panel.term_search_editor.sync_edits.clear();
                self.update_terminal_search();
                self.jump_to_terminal_search_result();
            }
            self.last_action = std::time::Instant::now();
            self.window.as_ref().unwrap().request_redraw();
        }
    }

    pub fn handle_search_keyboard_input(&mut self, key_event: KeyEvent) {
        if key_event.state == ElementState::Pressed {
            let ctrl = self.modifiers.control_key() || self.modifiers.super_key();
            let shift = self.modifiers.shift_key();
            let mut is_edit = false;

            match key_event.physical_key {
                PhysicalKey::Code(KeyCode::Escape) => {
                    self.show_search = false;
                    self.search_focused = false;
                    self.search_results.clear();
                    self.search_current_idx = None;
                    self.window.as_ref().unwrap().request_redraw();
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
                                self.search_current_idx =
                                    Some((idx + 1) % self.search_results.len());
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
    }
}
