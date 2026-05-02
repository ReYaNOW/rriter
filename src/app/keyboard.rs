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
    ctrl: bool,
    alt: bool,
    super_key: bool,
    is_alt: bool,
) -> Option<Vec<u8>> {
    let bytes: Option<&'static [u8]> = match physical_key {
        PhysicalKey::Code(KeyCode::Enter) | PhysicalKey::Code(KeyCode::NumpadEnter) => Some(b"\r"),
        PhysicalKey::Code(KeyCode::Backspace) => Some(b"\x08"),
        PhysicalKey::Code(KeyCode::Tab) => Some(b"\t"),
        PhysicalKey::Code(KeyCode::F1) => Some(b"\x1bOP"),
        PhysicalKey::Code(KeyCode::F2) => Some(b"\x1bOQ"),
        PhysicalKey::Code(KeyCode::F3) => Some(b"\x1bOR"),
        PhysicalKey::Code(KeyCode::F4) => Some(b"\x1bOS"),
        PhysicalKey::Code(KeyCode::F5) => Some(b"\x1b[15~"),
        PhysicalKey::Code(KeyCode::F6) => Some(b"\x1b[17~"),
        PhysicalKey::Code(KeyCode::F7) => Some(b"\x1b[18~"),
        PhysicalKey::Code(KeyCode::F8) => Some(b"\x1b[19~"),
        PhysicalKey::Code(KeyCode::F9) => Some(b"\x1b[20~"),
        PhysicalKey::Code(KeyCode::F10) => Some(b"\x1b[21~"),
        PhysicalKey::Code(KeyCode::F11) => Some(b"\x1b[23~"),
        PhysicalKey::Code(KeyCode::F12) => Some(b"\x1b[24~"),
        PhysicalKey::Code(KeyCode::ArrowUp) => Some(if is_alt { b"\x1bOA" } else { b"\x1b[A" }),
        PhysicalKey::Code(KeyCode::ArrowDown) => Some(if is_alt { b"\x1bOB" } else { b"\x1b[B" }),
        PhysicalKey::Code(KeyCode::ArrowLeft) => Some(if is_alt { b"\x1bOD" } else { b"\x1b[D" }),
        PhysicalKey::Code(KeyCode::ArrowRight) => Some(if is_alt { b"\x1bOC" } else { b"\x1b[C" }),
        PhysicalKey::Code(KeyCode::Escape) => Some(b"\x1b"),
        PhysicalKey::Code(KeyCode::KeyA) if ctrl => Some(b"\x01"),
        PhysicalKey::Code(KeyCode::KeyB) if ctrl => Some(b"\x02"),
        PhysicalKey::Code(KeyCode::KeyC) if ctrl => Some(b"\x03"),
        PhysicalKey::Code(KeyCode::KeyD) if ctrl => Some(b"\x04"),
        PhysicalKey::Code(KeyCode::KeyE) if ctrl => Some(b"\x05"),
        PhysicalKey::Code(KeyCode::KeyG) if ctrl => Some(b"\x07"),
        PhysicalKey::Code(KeyCode::KeyH) if ctrl => Some(b"\x08"),
        PhysicalKey::Code(KeyCode::KeyI) if ctrl => Some(b"\x09"),
        PhysicalKey::Code(KeyCode::KeyJ) if ctrl => Some(b"\x0a"),
        PhysicalKey::Code(KeyCode::KeyK) if ctrl => Some(b"\x0b"),
        PhysicalKey::Code(KeyCode::KeyL) if ctrl => Some(b"\x0c"),
        PhysicalKey::Code(KeyCode::KeyM) if ctrl => Some(b"\x0d"),
        PhysicalKey::Code(KeyCode::KeyN) if ctrl => Some(b"\x0e"),
        PhysicalKey::Code(KeyCode::KeyO) if ctrl => Some(b"\x0f"),
        PhysicalKey::Code(KeyCode::KeyP) if ctrl => Some(b"\x10"),
        PhysicalKey::Code(KeyCode::KeyQ) if ctrl => Some(b"\x11"),
        PhysicalKey::Code(KeyCode::KeyR) if ctrl => Some(b"\x12"),
        PhysicalKey::Code(KeyCode::KeyS) if ctrl => Some(b"\x13"),
        PhysicalKey::Code(KeyCode::KeyT) if ctrl => Some(b"\x14"),
        PhysicalKey::Code(KeyCode::KeyU) if ctrl => Some(b"\x15"),
        PhysicalKey::Code(KeyCode::KeyW) if ctrl => Some(b"\x17"),
        PhysicalKey::Code(KeyCode::KeyX) if ctrl => Some(b"\x18"),
        PhysicalKey::Code(KeyCode::KeyY) if ctrl => Some(b"\x19"),
        PhysicalKey::Code(KeyCode::KeyZ) if ctrl => Some(b"\x1a"),
        _ => None,
    };

    if let Some(bytes) = bytes {
        return Some(bytes.to_vec());
    }

    if !ctrl && !alt && !super_key {
        return logical_text.map(|txt| txt.as_bytes().to_vec());
    }

    None
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
                        let is_alt_mode = grid.is_alt;
                        if let Some(bytes) = terminal_key_sequence(
                            key_event.physical_key,
                            key_event.logical_key.to_text(),
                            ctrl,
                            self.modifiers.alt_key(),
                            self.modifiers.super_key(),
                            is_alt_mode,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seq(code: KeyCode, ctrl: bool, alt: bool, super_key: bool, is_alt: bool) -> Option<Vec<u8>> {
        terminal_key_sequence(PhysicalKey::Code(code), None, ctrl, alt, super_key, is_alt)
    }

    #[test]
    fn terminal_key_sequence_covers_basic_control_and_text_input() {
        assert_eq!(
            seq(KeyCode::Enter, false, false, false, false),
            Some(b"\r".to_vec())
        );
        assert_eq!(
            seq(KeyCode::NumpadEnter, false, false, false, false),
            Some(b"\r".to_vec())
        );
        assert_eq!(
            seq(KeyCode::Backspace, false, false, false, false),
            Some(b"\x08".to_vec())
        );
        assert_eq!(
            seq(KeyCode::Tab, false, false, false, false),
            Some(b"\t".to_vec())
        );
        assert_eq!(
            seq(KeyCode::Escape, false, false, false, false),
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
            ),
            Some(b"a".to_vec())
        );
        assert_eq!(
            terminal_key_sequence(
                PhysicalKey::Code(KeyCode::KeyA),
                Some("a"),
                false,
                true,
                false,
                false,
            ),
            None
        );
        assert_eq!(
            terminal_key_sequence(
                PhysicalKey::Code(KeyCode::KeyA),
                Some("a"),
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
                seq(key, false, false, false, false),
                Some(expected.to_vec())
            );
        }
    }

    #[test]
    fn terminal_key_sequence_covers_arrows_in_normal_and_alt_modes() {
        assert_eq!(
            seq(KeyCode::ArrowUp, false, false, false, false),
            Some(b"\x1b[A".to_vec())
        );
        assert_eq!(
            seq(KeyCode::ArrowDown, false, false, false, false),
            Some(b"\x1b[B".to_vec())
        );
        assert_eq!(
            seq(KeyCode::ArrowLeft, false, false, false, false),
            Some(b"\x1b[D".to_vec())
        );
        assert_eq!(
            seq(KeyCode::ArrowRight, false, false, false, false),
            Some(b"\x1b[C".to_vec())
        );

        assert_eq!(
            seq(KeyCode::ArrowUp, false, false, false, true),
            Some(b"\x1bOA".to_vec())
        );
        assert_eq!(
            seq(KeyCode::ArrowDown, false, false, false, true),
            Some(b"\x1bOB".to_vec())
        );
        assert_eq!(
            seq(KeyCode::ArrowLeft, false, false, false, true),
            Some(b"\x1bOD".to_vec())
        );
        assert_eq!(
            seq(KeyCode::ArrowRight, false, false, false, true),
            Some(b"\x1bOC".to_vec())
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
            assert_eq!(seq(key, true, false, false, false), Some(vec![byte]));
        }

        assert_eq!(seq(KeyCode::KeyF, true, false, false, false), None);
        assert_eq!(seq(KeyCode::KeyV, true, false, false, false), None);
    }
}
