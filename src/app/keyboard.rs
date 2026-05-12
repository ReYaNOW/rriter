use crate::app::{
    App, AutocompleteMode, PendingAction, cursor_after_python_member_dot,
    cursor_inside_python_call_parens,
};
use crate::editor::Editor;
use std::io::Write;
use std::time::Instant;
use winit::event::{ElementState, KeyEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};

mod editor_keys;
mod main_keys;

fn terminal_key_sequence(
    physical_key: PhysicalKey,
    logical_text: Option<&str>,
    shift: bool,
    ctrl: bool,
    alt: bool,
    super_key: bool,
    app_cursor_keys: bool,
) -> Option<Vec<u8>> {
    let seq = match physical_key {
        PhysicalKey::Code(KeyCode::Enter) | PhysicalKey::Code(KeyCode::NumpadEnter) => {
            terminal_alt_prefixed(b"\r", alt)
        }
        PhysicalKey::Code(KeyCode::Backspace) if ctrl => terminal_alt_prefixed(b"\x17", alt),
        PhysicalKey::Code(KeyCode::Backspace) if shift => terminal_alt_prefixed(b"\x08", alt),
        PhysicalKey::Code(KeyCode::Backspace) => terminal_alt_prefixed(b"\x7f", alt),
        PhysicalKey::Code(KeyCode::Tab) if shift => b"\x1b[Z".to_vec(),
        PhysicalKey::Code(KeyCode::Tab) => terminal_alt_prefixed(b"\t", alt),
        PhysicalKey::Code(KeyCode::Escape) => b"\x1b".to_vec(),
        PhysicalKey::Code(KeyCode::Insert) => terminal_tilde_key(2, shift, alt, ctrl),
        PhysicalKey::Code(KeyCode::Delete) => terminal_tilde_key(3, shift, alt, ctrl),
        PhysicalKey::Code(KeyCode::PageUp) => terminal_tilde_key(5, shift, alt, ctrl),
        PhysicalKey::Code(KeyCode::PageDown) => terminal_tilde_key(6, shift, alt, ctrl),
        PhysicalKey::Code(KeyCode::Home) => {
            terminal_cursor_key(b'H', app_cursor_keys, shift, alt, ctrl)
        }
        PhysicalKey::Code(KeyCode::End) => {
            terminal_cursor_key(b'F', app_cursor_keys, shift, alt, ctrl)
        }
        PhysicalKey::Code(KeyCode::ArrowUp) => {
            terminal_cursor_key(b'A', app_cursor_keys, shift, alt, ctrl)
        }
        PhysicalKey::Code(KeyCode::ArrowDown) => {
            terminal_cursor_key(b'B', app_cursor_keys, shift, alt, ctrl)
        }
        PhysicalKey::Code(KeyCode::ArrowRight) => {
            terminal_cursor_key(b'C', app_cursor_keys, shift, alt, ctrl)
        }
        PhysicalKey::Code(KeyCode::ArrowLeft) => {
            terminal_cursor_key(b'D', app_cursor_keys, shift, alt, ctrl)
        }
        PhysicalKey::Code(KeyCode::F1) => terminal_function_key(11, b'P', shift, alt, ctrl),
        PhysicalKey::Code(KeyCode::F2) => terminal_function_key(12, b'Q', shift, alt, ctrl),
        PhysicalKey::Code(KeyCode::F3) => terminal_function_key(13, b'R', shift, alt, ctrl),
        PhysicalKey::Code(KeyCode::F4) => terminal_function_key(14, b'S', shift, alt, ctrl),
        PhysicalKey::Code(KeyCode::F5) => terminal_function_key(15, 0, shift, alt, ctrl),
        PhysicalKey::Code(KeyCode::F6) => terminal_function_key(17, 0, shift, alt, ctrl),
        PhysicalKey::Code(KeyCode::F7) => terminal_function_key(18, 0, shift, alt, ctrl),
        PhysicalKey::Code(KeyCode::F8) => terminal_function_key(19, 0, shift, alt, ctrl),
        PhysicalKey::Code(KeyCode::F9) => terminal_function_key(20, 0, shift, alt, ctrl),
        PhysicalKey::Code(KeyCode::F10) => terminal_function_key(21, 0, shift, alt, ctrl),
        PhysicalKey::Code(KeyCode::F11) => terminal_function_key(23, 0, shift, alt, ctrl),
        PhysicalKey::Code(KeyCode::F12) => terminal_function_key(24, 0, shift, alt, ctrl),
        PhysicalKey::Code(KeyCode::Space) if ctrl => terminal_alt_prefixed(b"\x00", alt),
        PhysicalKey::Code(KeyCode::Digit2) if ctrl => terminal_alt_prefixed(b"\x00", alt),
        PhysicalKey::Code(KeyCode::Digit6) if ctrl => terminal_alt_prefixed(b"\x1e", alt),
        PhysicalKey::Code(KeyCode::Minus) if ctrl => terminal_alt_prefixed(b"\x1f", alt),
        PhysicalKey::Code(KeyCode::Slash) if ctrl => terminal_alt_prefixed(b"\x1f", alt),
        PhysicalKey::Code(KeyCode::BracketLeft) if ctrl => terminal_alt_prefixed(b"\x1b", alt),
        PhysicalKey::Code(KeyCode::Backslash) if ctrl => terminal_alt_prefixed(b"\x1c", alt),
        PhysicalKey::Code(KeyCode::BracketRight) if ctrl => terminal_alt_prefixed(b"\x1d", alt),
        PhysicalKey::Code(KeyCode::KeyA) if ctrl => terminal_alt_prefixed(b"\x01", alt),
        PhysicalKey::Code(KeyCode::KeyB) if ctrl => terminal_alt_prefixed(b"\x02", alt),
        PhysicalKey::Code(KeyCode::KeyC) if ctrl => terminal_alt_prefixed(b"\x03", alt),
        PhysicalKey::Code(KeyCode::KeyD) if ctrl => terminal_alt_prefixed(b"\x04", alt),
        PhysicalKey::Code(KeyCode::KeyE) if ctrl => terminal_alt_prefixed(b"\x05", alt),
        PhysicalKey::Code(KeyCode::KeyG) if ctrl => terminal_alt_prefixed(b"\x07", alt),
        PhysicalKey::Code(KeyCode::KeyH) if ctrl => terminal_alt_prefixed(b"\x08", alt),
        PhysicalKey::Code(KeyCode::KeyI) if ctrl => terminal_alt_prefixed(b"\x09", alt),
        PhysicalKey::Code(KeyCode::KeyJ) if ctrl => terminal_alt_prefixed(b"\x0a", alt),
        PhysicalKey::Code(KeyCode::KeyK) if ctrl => terminal_alt_prefixed(b"\x0b", alt),
        PhysicalKey::Code(KeyCode::KeyL) if ctrl => terminal_alt_prefixed(b"\x0c", alt),
        PhysicalKey::Code(KeyCode::KeyM) if ctrl => terminal_alt_prefixed(b"\x0d", alt),
        PhysicalKey::Code(KeyCode::KeyN) if ctrl => terminal_alt_prefixed(b"\x0e", alt),
        PhysicalKey::Code(KeyCode::KeyO) if ctrl => terminal_alt_prefixed(b"\x0f", alt),
        PhysicalKey::Code(KeyCode::KeyP) if ctrl => terminal_alt_prefixed(b"\x10", alt),
        PhysicalKey::Code(KeyCode::KeyQ) if ctrl => terminal_alt_prefixed(b"\x11", alt),
        PhysicalKey::Code(KeyCode::KeyR) if ctrl => terminal_alt_prefixed(b"\x12", alt),
        PhysicalKey::Code(KeyCode::KeyS) if ctrl => terminal_alt_prefixed(b"\x13", alt),
        PhysicalKey::Code(KeyCode::KeyT) if ctrl => terminal_alt_prefixed(b"\x14", alt),
        PhysicalKey::Code(KeyCode::KeyU) if ctrl => terminal_alt_prefixed(b"\x15", alt),
        PhysicalKey::Code(KeyCode::KeyW) if ctrl => terminal_alt_prefixed(b"\x17", alt),
        PhysicalKey::Code(KeyCode::KeyX) if ctrl => terminal_alt_prefixed(b"\x18", alt),
        PhysicalKey::Code(KeyCode::KeyY) if ctrl => terminal_alt_prefixed(b"\x19", alt),
        PhysicalKey::Code(KeyCode::KeyZ) if ctrl => terminal_alt_prefixed(b"\x1a", alt),
        _ => {
            if !ctrl && !super_key {
                if let Some(txt) = logical_text {
                    if alt {
                        let mut out = Vec::with_capacity(txt.len() + 1);
                        out.push(0x1b);
                        out.extend_from_slice(txt.as_bytes());
                        out
                    } else {
                        txt.as_bytes().to_vec()
                    }
                } else {
                    return None;
                }
            } else {
                return None;
            }
        }
    };

    Some(seq)
}

fn terminal_modifier(shift: bool, alt: bool, ctrl: bool) -> Option<u8> {
    let value = 1 + u8::from(shift) + u8::from(alt) * 2 + u8::from(ctrl) * 4;
    (value != 1).then_some(value)
}

fn terminal_alt_prefixed(bytes: &[u8], alt: bool) -> Vec<u8> {
    if alt {
        let mut out = Vec::with_capacity(bytes.len() + 1);
        out.push(0x1b);
        out.extend_from_slice(bytes);
        out
    } else {
        bytes.to_vec()
    }
}

fn terminal_cursor_key(
    final_byte: u8,
    app_cursor_keys: bool,
    shift: bool,
    alt: bool,
    ctrl: bool,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(8);
    out.push(0x1b);
    if let Some(modifier) = terminal_modifier(shift, alt, ctrl) {
        out.extend_from_slice(b"[1;");
        out.push(b'0' + modifier);
        out.push(final_byte);
    } else if app_cursor_keys {
        out.push(b'O');
        out.push(final_byte);
    } else {
        out.push(b'[');
        out.push(final_byte);
    }
    out
}

fn terminal_tilde_key(code: u8, shift: bool, alt: bool, ctrl: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(8);
    out.extend_from_slice(b"\x1b[");
    out.push(b'0' + code);
    if let Some(modifier) = terminal_modifier(shift, alt, ctrl) {
        out.push(b';');
        out.push(b'0' + modifier);
    }
    out.push(b'~');
    out
}

fn terminal_function_key(code: u8, ss3_final: u8, shift: bool, alt: bool, ctrl: bool) -> Vec<u8> {
    if ss3_final != 0 && terminal_modifier(shift, alt, ctrl).is_none() {
        return vec![0x1b, b'O', ss3_final];
    }

    let mut out = Vec::with_capacity(9);
    out.extend_from_slice(b"\x1b[");
    if ss3_final != 0 {
        out.push(b'1');
    } else {
        if code >= 10 {
            out.push(b'0' + code / 10);
        }
        out.push(b'0' + code % 10);
    }
    if let Some(modifier) = terminal_modifier(shift, alt, ctrl) {
        out.push(b';');
        out.push(b'0' + modifier);
    }
    if ss3_final != 0 {
        out.push(ss3_final);
    } else {
        out.push(b'~');
    }
    out
}

impl App {
    #[cfg_attr(coverage_nightly, coverage(off))]
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

                            if let Some(clipboard) = self.clipboard.as_mut() {
                                let _ = clipboard.set_text(res.trim_end().to_string());
                            }
                            grid.selection = None;
                        } else {
                            let _ = w.write_all(b"\x03");
                        }
                    }
                    PhysicalKey::Code(KeyCode::KeyV) if ctrl => {
                        if let Some(text) = self
                            .clipboard
                            .as_mut()
                            .and_then(|clipboard| clipboard.get_text().ok())
                        {
                            let _ = w.write_all(text.as_bytes());
                        }
                    }
                    _ => {
                        if let Some(bytes) = terminal_key_sequence(
                            key_event.physical_key,
                            key_event.logical_key.to_text(),
                            self.modifiers.shift_key(),
                            ctrl,
                            self.modifiers.alt_key(),
                            self.modifiers.super_key(),
                            grid.app_cursor_keys,
                        ) {
                            let _ = w.write_all(&bytes);
                        }
                    }
                }
                w.flush().ok();
            }
        }
        self.last_action = std::time::Instant::now();
        self.window.as_ref().unwrap().request_redraw();
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
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
                        self.set_clipboard_text(text);
                    }
                }
                PhysicalKey::Code(KeyCode::KeyX) if ctrl => {
                    if let Some(text) = self.ide_panel.term_search_editor.get_selection() {
                        self.set_clipboard_text(text);
                        self.ide_panel.term_search_editor.delete_selection();
                        is_edit = true;
                    }
                }
                PhysicalKey::Code(KeyCode::KeyV) if ctrl => {
                    if let Some(text) = self.get_clipboard_text() {
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

    #[cfg_attr(coverage_nightly, coverage(off))]
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
                        self.set_clipboard_text(text);
                    }
                }
                PhysicalKey::Code(KeyCode::KeyX) if ctrl => {
                    if let Some(text) = self.search_editor.get_selection() {
                        self.set_clipboard_text(text);
                        self.search_editor.delete_selection();
                        is_edit = true;
                    }
                }
                PhysicalKey::Code(KeyCode::KeyV) if ctrl => {
                    if let Some(text) = self.get_clipboard_text() {
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

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn handle_lsp_log_filter_keyboard_input(&mut self, key_event: KeyEvent) {
        if key_event.state == ElementState::Pressed {
            let ctrl = self.modifiers.control_key() || self.modifiers.super_key();
            let shift = self.modifiers.shift_key();
            let mut is_edit = false;

            match key_event.physical_key {
                PhysicalKey::Code(KeyCode::Escape) => {
                    self.ide_panel.lsp_log_filter_focused = false;
                }
                PhysicalKey::Code(KeyCode::ArrowLeft) => {
                    if ctrl {
                        self.ide_panel.lsp_log_filter_editor.move_word_left(shift);
                    } else {
                        self.ide_panel.lsp_log_filter_editor.move_left(shift);
                    }
                }
                PhysicalKey::Code(KeyCode::ArrowRight) => {
                    if ctrl {
                        self.ide_panel.lsp_log_filter_editor.move_word_right(shift);
                    } else {
                        self.ide_panel.lsp_log_filter_editor.move_right(shift);
                    }
                }
                PhysicalKey::Code(KeyCode::Home) => {
                    self.ide_panel.lsp_log_filter_editor.move_home(shift);
                }
                PhysicalKey::Code(KeyCode::End) => {
                    self.ide_panel.lsp_log_filter_editor.move_end(shift);
                }
                PhysicalKey::Code(KeyCode::KeyA) if ctrl => {
                    self.ide_panel.lsp_log_filter_editor.select_all();
                }
                PhysicalKey::Code(KeyCode::KeyC) if ctrl => {
                    if let Some(text) = self.ide_panel.lsp_log_filter_editor.get_selection() {
                        self.set_clipboard_text(text);
                    }
                }
                PhysicalKey::Code(KeyCode::KeyX) if ctrl => {
                    if let Some(text) = self.ide_panel.lsp_log_filter_editor.get_selection() {
                        self.set_clipboard_text(text);
                        self.ide_panel.lsp_log_filter_editor.delete_selection();
                        is_edit = true;
                    }
                }
                PhysicalKey::Code(KeyCode::KeyV) if ctrl => {
                    if let Some(text) = self.get_clipboard_text() {
                        self.ide_panel.lsp_log_filter_editor.insert_str(&text);
                        is_edit = true;
                    }
                }
                PhysicalKey::Code(KeyCode::Backspace) => {
                    if self.ide_panel.lsp_log_filter_editor.backspace().is_some() {
                        is_edit = true;
                    }
                }
                PhysicalKey::Code(KeyCode::Delete) => {
                    if self
                        .ide_panel
                        .lsp_log_filter_editor
                        .delete_forward()
                        .is_some()
                    {
                        is_edit = true;
                    }
                }
                _ => {
                    if !ctrl && !self.modifiers.alt_key() && !self.modifiers.super_key() {
                        if let Some(txt) = key_event.logical_key.to_text() {
                            let clean_txt = txt.replace('\n', "");
                            if !clean_txt.is_empty() {
                                self.ide_panel.lsp_log_filter_editor.insert_str(&clean_txt);
                                is_edit = true;
                            }
                        }
                    }
                }
            }

            if is_edit {
                self.ide_panel.lsp_log_filter_editor.sync_edits.clear();
                self.ide_panel.lsp_log_filter_dirty = true;
            }
            self.last_action = Instant::now();
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn handle_git_message_keyboard_input(&mut self, key_event: KeyEvent) {
        if key_event.state == ElementState::Pressed {
            let ctrl = self.modifiers.control_key() || self.modifiers.super_key();
            let shift = self.modifiers.shift_key();
            let mut is_edit = false;

            match key_event.physical_key {
                PhysicalKey::Code(KeyCode::Escape) => {
                    self.ide_panel.git.message_focused = false;
                }
                PhysicalKey::Code(KeyCode::Enter) | PhysicalKey::Code(KeyCode::NumpadEnter) => {
                    self.commit_git_panel();
                }
                PhysicalKey::Code(KeyCode::ArrowLeft) => {
                    if ctrl {
                        self.ide_panel.git.message_editor.move_word_left(shift);
                    } else {
                        self.ide_panel.git.message_editor.move_left(shift);
                    }
                }
                PhysicalKey::Code(KeyCode::ArrowRight) => {
                    if ctrl {
                        self.ide_panel.git.message_editor.move_word_right(shift);
                    } else {
                        self.ide_panel.git.message_editor.move_right(shift);
                    }
                }
                PhysicalKey::Code(KeyCode::Home) => {
                    self.ide_panel.git.message_editor.move_home(shift);
                }
                PhysicalKey::Code(KeyCode::End) => {
                    self.ide_panel.git.message_editor.move_end(shift);
                }
                PhysicalKey::Code(KeyCode::KeyA) if ctrl => {
                    self.ide_panel.git.message_editor.select_all();
                }
                PhysicalKey::Code(KeyCode::KeyC) if ctrl => {
                    if let Some(text) = self.ide_panel.git.message_editor.get_selection() {
                        self.set_clipboard_text(text);
                    }
                }
                PhysicalKey::Code(KeyCode::KeyX) if ctrl => {
                    if let Some(text) = self.ide_panel.git.message_editor.get_selection() {
                        self.set_clipboard_text(text);
                        self.ide_panel.git.message_editor.delete_selection();
                        is_edit = true;
                    }
                }
                PhysicalKey::Code(KeyCode::KeyV) if ctrl => {
                    if let Some(text) = self.get_clipboard_text() {
                        let clean = text.replace('\n', "").replace('\r', "");
                        self.ide_panel.git.message_editor.insert_str(&clean);
                        is_edit = true;
                    }
                }
                PhysicalKey::Code(KeyCode::Backspace) => {
                    if ctrl {
                        self.ide_panel.git.message_editor.delete_word_backward();
                        is_edit = true;
                    } else if self.ide_panel.git.message_editor.backspace().is_some() {
                        is_edit = true;
                    }
                }
                PhysicalKey::Code(KeyCode::Delete) => {
                    if ctrl {
                        self.ide_panel.git.message_editor.delete_word_forward();
                        is_edit = true;
                    } else if self.ide_panel.git.message_editor.delete_forward().is_some() {
                        is_edit = true;
                    }
                }
                _ => {
                    if !ctrl && !self.modifiers.alt_key() && !self.modifiers.super_key() {
                        if let Some(txt) = key_event.logical_key.to_text() {
                            let clean_txt = txt.replace('\n', "");
                            if !clean_txt.is_empty() {
                                self.ide_panel.git.message_editor.insert_str(&clean_txt);
                                is_edit = true;
                            }
                        }
                    }
                }
            }

            if is_edit {
                self.ide_panel.git.message_editor.sync_edits.clear();
            }
            self.last_action = Instant::now();
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seq(
        code: KeyCode,
        shift: bool,
        ctrl: bool,
        alt: bool,
        super_key: bool,
        app_cursor_keys: bool,
    ) -> Option<Vec<u8>> {
        terminal_key_sequence(
            PhysicalKey::Code(code),
            None,
            shift,
            ctrl,
            alt,
            super_key,
            app_cursor_keys,
        )
    }

    #[test]
    fn terminal_key_sequence_covers_basic_control_and_text_input() {
        assert_eq!(
            seq(KeyCode::Enter, false, false, false, false, false),
            Some(b"\r".to_vec())
        );
        assert_eq!(
            seq(KeyCode::NumpadEnter, false, false, false, false, false),
            Some(b"\r".to_vec())
        );
        assert_eq!(
            seq(KeyCode::Backspace, false, false, false, false, false),
            Some(b"\x7f".to_vec())
        );
        assert_eq!(
            seq(KeyCode::Backspace, true, false, false, false, false),
            Some(b"\x08".to_vec())
        );
        assert_eq!(
            seq(KeyCode::Backspace, false, true, false, false, false),
            Some(b"\x17".to_vec())
        );
        assert_eq!(
            seq(KeyCode::Tab, false, false, false, false, false),
            Some(b"\t".to_vec())
        );
        assert_eq!(
            seq(KeyCode::Tab, true, false, false, false, false),
            Some(b"\x1b[Z".to_vec())
        );
        assert_eq!(
            seq(KeyCode::Escape, false, false, false, false, false),
            Some(b"\x1b".to_vec())
        );

        assert_eq!(
            terminal_key_sequence(
                PhysicalKey::Code(KeyCode::KeyA),
                Some("a"),
                false,
                false,
                false,
                false,
                false,
            ),
            Some(b"a".to_vec())
        );
        assert_eq!(
            terminal_key_sequence(
                PhysicalKey::Code(KeyCode::KeyA),
                Some("a"),
                false,
                false,
                true,
                false,
                false,
            ),
            Some(b"\x1ba".to_vec())
        );
        assert_eq!(
            terminal_key_sequence(
                PhysicalKey::Code(KeyCode::KeyA),
                Some("a"),
                false,
                false,
                false,
                true,
                false,
            ),
            None
        );
    }

    #[test]
    fn terminal_key_sequence_covers_function_keys() {
        let cases = [
            (KeyCode::F1, b"\x1bOP".as_slice()),
            (KeyCode::F2, b"\x1bOQ".as_slice()),
            (KeyCode::F3, b"\x1bOR".as_slice()),
            (KeyCode::F4, b"\x1bOS".as_slice()),
            (KeyCode::F5, b"\x1b[15~".as_slice()),
            (KeyCode::F6, b"\x1b[17~".as_slice()),
            (KeyCode::F7, b"\x1b[18~".as_slice()),
            (KeyCode::F8, b"\x1b[19~".as_slice()),
            (KeyCode::F9, b"\x1b[20~".as_slice()),
            (KeyCode::F10, b"\x1b[21~".as_slice()),
            (KeyCode::F11, b"\x1b[23~".as_slice()),
            (KeyCode::F12, b"\x1b[24~".as_slice()),
        ];

        for (key, expected) in cases {
            assert_eq!(
                seq(key, false, false, false, false, false),
                Some(expected.to_vec())
            );
        }

        assert_eq!(
            seq(KeyCode::F1, false, true, false, false, false),
            Some(b"\x1b[1;5P".to_vec())
        );
        assert_eq!(
            seq(KeyCode::F5, true, false, false, false, false),
            Some(b"\x1b[15;2~".to_vec())
        );
    }

    #[test]
    fn terminal_key_sequence_covers_navigation_and_modifiers() {
        assert_eq!(
            seq(KeyCode::ArrowUp, false, false, false, false, false),
            Some(b"\x1b[A".to_vec())
        );
        assert_eq!(
            seq(KeyCode::ArrowDown, false, false, false, false, false),
            Some(b"\x1b[B".to_vec())
        );
        assert_eq!(
            seq(KeyCode::ArrowLeft, false, false, false, false, false),
            Some(b"\x1b[D".to_vec())
        );
        assert_eq!(
            seq(KeyCode::ArrowRight, false, false, false, false, false),
            Some(b"\x1b[C".to_vec())
        );

        assert_eq!(
            seq(KeyCode::ArrowUp, false, false, false, false, true),
            Some(b"\x1bOA".to_vec())
        );
        assert_eq!(
            seq(KeyCode::ArrowDown, false, false, false, false, true),
            Some(b"\x1bOB".to_vec())
        );
        assert_eq!(
            seq(KeyCode::ArrowLeft, false, false, false, false, true),
            Some(b"\x1bOD".to_vec())
        );
        assert_eq!(
            seq(KeyCode::ArrowRight, false, false, false, false, true),
            Some(b"\x1bOC".to_vec())
        );
        assert_eq!(
            seq(KeyCode::ArrowRight, false, true, false, false, false),
            Some(b"\x1b[1;5C".to_vec())
        );
        assert_eq!(
            seq(KeyCode::ArrowLeft, true, true, false, false, false),
            Some(b"\x1b[1;6D".to_vec())
        );
        assert_eq!(
            seq(KeyCode::Home, false, false, false, false, false),
            Some(b"\x1b[H".to_vec())
        );
        assert_eq!(
            seq(KeyCode::End, false, false, false, false, false),
            Some(b"\x1b[F".to_vec())
        );
        assert_eq!(
            seq(KeyCode::Delete, false, false, false, false, false),
            Some(b"\x1b[3~".to_vec())
        );
        assert_eq!(
            seq(KeyCode::Delete, false, true, false, false, false),
            Some(b"\x1b[3;5~".to_vec())
        );
        assert_eq!(
            seq(KeyCode::PageUp, false, false, true, false, false),
            Some(b"\x1b[5;3~".to_vec())
        );
    }

    #[test]
    fn terminal_key_sequence_covers_ctrl_letter_bytes() {
        let cases = [
            (KeyCode::KeyA, 0x01),
            (KeyCode::KeyB, 0x02),
            (KeyCode::KeyC, 0x03),
            (KeyCode::KeyD, 0x04),
            (KeyCode::KeyE, 0x05),
            (KeyCode::KeyG, 0x07),
            (KeyCode::KeyH, 0x08),
            (KeyCode::KeyI, 0x09),
            (KeyCode::KeyJ, 0x0a),
            (KeyCode::KeyK, 0x0b),
            (KeyCode::KeyL, 0x0c),
            (KeyCode::KeyM, 0x0d),
            (KeyCode::KeyN, 0x0e),
            (KeyCode::KeyO, 0x0f),
            (KeyCode::KeyP, 0x10),
            (KeyCode::KeyQ, 0x11),
            (KeyCode::KeyR, 0x12),
            (KeyCode::KeyS, 0x13),
            (KeyCode::KeyT, 0x14),
            (KeyCode::KeyU, 0x15),
            (KeyCode::KeyW, 0x17),
            (KeyCode::KeyX, 0x18),
            (KeyCode::KeyY, 0x19),
            (KeyCode::KeyZ, 0x1a),
        ];

        for (key, byte) in cases {
            assert_eq!(seq(key, false, true, false, false, false), Some(vec![byte]));
        }

        assert_eq!(seq(KeyCode::KeyF, false, true, false, false, false), None);
        assert_eq!(seq(KeyCode::KeyV, false, true, false, false, false), None);
        assert_eq!(
            seq(KeyCode::Slash, false, true, false, false, false),
            Some(b"\x1f".to_vec())
        );
        assert_eq!(
            seq(KeyCode::BracketLeft, false, true, false, false, false),
            Some(b"\x1b".to_vec())
        );
    }
}
