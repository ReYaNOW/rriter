use crate::app::{
    App, AutocompleteMode, AutocompletePopupKeyResult, PendingAction,
    cursor_after_python_member_dot, cursor_inside_python_call_parens,
};
use crate::editor::Editor;
use std::borrow::Cow;
#[cfg(target_os = "linux")]
use std::path::{Path, PathBuf};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Instant;
use ronsole_term::{KeyEvent, KeyKind, KeyMods, KeyOutcome, KpKey, ModKey, TermKey};
use winit::event::ElementState;
use crate::app::events::host_loop::HostLoop;
use winit::keyboard::{KeyCode, ModifiersState, NativeKeyCode, PhysicalKey};

mod editor_keys;
mod key_input;
mod main_keys;
mod commands;
mod key_routes;
pub(crate) mod input_owner;
#[cfg(test)]
mod key_route_char_a;
#[cfg(test)]
mod key_route_char_b;
#[cfg(test)]
mod key_route_char_c;
#[cfg(test)]
mod terminal_key_tests;
pub(crate) use editor_keys::paired_editor_insert_text;
pub(crate) use key_input::{KeyComboHold, KeyInput};

#[cfg(target_os = "linux")]
fn terminal_clipboard_paste_bytes(
    file_list: Option<&[PathBuf]>,
    text: Option<&str>,
) -> Option<Vec<u8>> {
    if let Some(paths) = file_list.filter(|paths| !paths.is_empty()) {
        let mut out = Vec::new();
        for (index, path) in paths.iter().enumerate() {
            if index > 0 {
                out.push(b' ');
            }
            terminal_shell_escape_path(path, &mut out);
        }
        return Some(out);
    }
    text.map(|text| text.as_bytes().to_vec())
}

#[cfg(target_os = "linux")]
fn terminal_shell_escape_path(path: &Path, out: &mut Vec<u8>) {
    use std::os::unix::ffi::OsStrExt;

    let bytes = path.as_os_str().as_bytes();
    if bytes.is_empty() {
        out.extend_from_slice(b"''");
        return;
    }
    for &byte in bytes {
        match byte {
            b'\n' | b'\r' => {
                out.extend_from_slice(&[b'\'', byte, b'\'']);
                continue;
            }
            b' ' | b'\t' | b'\\' | b'\'' | b'"' | b'`' | b'$' | b'&' | b';' | b'|' | b'<'
            | b'>' | b'(' | b')' | b'[' | b']' | b'{' | b'}' | b'*' | b'?' | b'!' | b'#' | b'~'
            | b'^' | b'%' | b'=' => out.push(b'\\'),
            _ => {}
        }
        out.push(byte);
    }
}

/// Stable opaque id of a physical key, the same for its press, repeat and release.
/// Keys identified by `KeyCode` set the top bit; native codes use tags 1..=5 in bits 32..35.
fn terminal_physical_id(key: PhysicalKey) -> u64 {
    const HIGH_BIT: u64 = 1 << 63;
    match key {
        PhysicalKey::Code(code) => {
            let mut hasher = DefaultHasher::new();
            code.hash(&mut hasher);
            HIGH_BIT | (hasher.finish() & !HIGH_BIT)
        }
        PhysicalKey::Unidentified(native) => match native {
            NativeKeyCode::Android(code) => (1_u64 << 32) | u64::from(code),
            NativeKeyCode::MacOS(code) => (2_u64 << 32) | u64::from(code),
            NativeKeyCode::Windows(code) => (3_u64 << 32) | u64::from(code),
            NativeKeyCode::Xkb(code) => (4_u64 << 32) | u64::from(code),
            NativeKeyCode::Unidentified => 5_u64 << 32,
        },
    }
}

/// Host modifiers as the terminal encoder sees them. Ctrl+Alt is dropped on Windows
/// (AltGr), so a composed character reaches the encoder as plain text.
fn terminal_key_mods(platform: crate::platform::PlatformKind, modifiers: ModifiersState) -> KeyMods {
    let (ctrl, alt) = crate::platform::terminal_modifiers_for_platform(
        platform,
        modifiers.control_key(),
        modifiers.alt_key(),
    );
    KeyMods {
        shift: modifiers.shift_key(),
        ctrl,
        alt,
        super_key: modifiers.super_key(),
    }
}

/// Named or character key for the encoder. Keys without a name use the unshifted character;
/// `None` when neither exists, so no placeholder character reaches the encoder.
fn terminal_key(input: &KeyInput) -> Option<TermKey> {
    let PhysicalKey::Code(code) = input.physical_key else {
        return input.unshifted.map(TermKey::Char);
    };
    named_terminal_key(code).or_else(|| input.unshifted.map(TermKey::Char))
}

/// Encoder name of a physical key; `None` for keys the encoder does not name.
fn named_terminal_key(code: KeyCode) -> Option<TermKey> {
    Some(match code {
        KeyCode::Enter => TermKey::Enter,
        KeyCode::Tab => TermKey::Tab,
        KeyCode::Backspace => TermKey::Backspace,
        KeyCode::Escape => TermKey::Escape,
        KeyCode::Insert => TermKey::Insert,
        KeyCode::Delete => TermKey::Delete,
        KeyCode::Home => TermKey::Home,
        KeyCode::End => TermKey::End,
        KeyCode::PageUp => TermKey::PageUp,
        KeyCode::PageDown => TermKey::PageDown,
        KeyCode::ArrowUp => TermKey::Up,
        KeyCode::ArrowDown => TermKey::Down,
        KeyCode::ArrowLeft => TermKey::Left,
        KeyCode::ArrowRight => TermKey::Right,
        KeyCode::F1 => TermKey::F(1),
        KeyCode::F2 => TermKey::F(2),
        KeyCode::F3 => TermKey::F(3),
        KeyCode::F4 => TermKey::F(4),
        KeyCode::F5 => TermKey::F(5),
        KeyCode::F6 => TermKey::F(6),
        KeyCode::F7 => TermKey::F(7),
        KeyCode::F8 => TermKey::F(8),
        KeyCode::F9 => TermKey::F(9),
        KeyCode::F10 => TermKey::F(10),
        KeyCode::F11 => TermKey::F(11),
        KeyCode::F12 => TermKey::F(12),
        KeyCode::Numpad0 => TermKey::Kp(KpKey::Kp0),
        KeyCode::Numpad1 => TermKey::Kp(KpKey::Kp1),
        KeyCode::Numpad2 => TermKey::Kp(KpKey::Kp2),
        KeyCode::Numpad3 => TermKey::Kp(KpKey::Kp3),
        KeyCode::Numpad4 => TermKey::Kp(KpKey::Kp4),
        KeyCode::Numpad5 => TermKey::Kp(KpKey::Kp5),
        KeyCode::Numpad6 => TermKey::Kp(KpKey::Kp6),
        KeyCode::Numpad7 => TermKey::Kp(KpKey::Kp7),
        KeyCode::Numpad8 => TermKey::Kp(KpKey::Kp8),
        KeyCode::Numpad9 => TermKey::Kp(KpKey::Kp9),
        KeyCode::NumpadDecimal => TermKey::Kp(KpKey::Decimal),
        KeyCode::NumpadDivide => TermKey::Kp(KpKey::Divide),
        KeyCode::NumpadMultiply => TermKey::Kp(KpKey::Multiply),
        KeyCode::NumpadSubtract => TermKey::Kp(KpKey::Subtract),
        KeyCode::NumpadAdd => TermKey::Kp(KpKey::Add),
        KeyCode::NumpadEnter => TermKey::Kp(KpKey::Enter),
        KeyCode::NumpadEqual => TermKey::Kp(KpKey::Equal),
        KeyCode::NumpadComma => TermKey::Kp(KpKey::Separator),
        KeyCode::ShiftLeft => TermKey::Modifier(ModKey::LeftShift),
        KeyCode::ShiftRight => TermKey::Modifier(ModKey::RightShift),
        KeyCode::ControlLeft => TermKey::Modifier(ModKey::LeftCtrl),
        KeyCode::ControlRight => TermKey::Modifier(ModKey::RightCtrl),
        KeyCode::AltLeft => TermKey::Modifier(ModKey::LeftAlt),
        KeyCode::AltRight => TermKey::Modifier(ModKey::RightAlt),
        KeyCode::SuperLeft => TermKey::Modifier(ModKey::LeftSuper),
        KeyCode::SuperRight => TermKey::Modifier(ModKey::RightSuper),
        KeyCode::CapsLock => TermKey::Modifier(ModKey::CapsLock),
        KeyCode::NumLock => TermKey::Modifier(ModKey::NumLock),
        _ => return None,
    })
}

/// Base character of a key on a US layout, used by the encoder for non-ASCII layouts.
fn us_base_char(key: PhysicalKey) -> Option<char> {
    let PhysicalKey::Code(code) = key else {
        return None;
    };
    Some(match code {
        KeyCode::KeyA => 'a',
        KeyCode::KeyB => 'b',
        KeyCode::KeyC => 'c',
        KeyCode::KeyD => 'd',
        KeyCode::KeyE => 'e',
        KeyCode::KeyF => 'f',
        KeyCode::KeyG => 'g',
        KeyCode::KeyH => 'h',
        KeyCode::KeyI => 'i',
        KeyCode::KeyJ => 'j',
        KeyCode::KeyK => 'k',
        KeyCode::KeyL => 'l',
        KeyCode::KeyM => 'm',
        KeyCode::KeyN => 'n',
        KeyCode::KeyO => 'o',
        KeyCode::KeyP => 'p',
        KeyCode::KeyQ => 'q',
        KeyCode::KeyR => 'r',
        KeyCode::KeyS => 's',
        KeyCode::KeyT => 't',
        KeyCode::KeyU => 'u',
        KeyCode::KeyV => 'v',
        KeyCode::KeyW => 'w',
        KeyCode::KeyX => 'x',
        KeyCode::KeyY => 'y',
        KeyCode::KeyZ => 'z',
        KeyCode::Digit0 => '0',
        KeyCode::Digit1 => '1',
        KeyCode::Digit2 => '2',
        KeyCode::Digit3 => '3',
        KeyCode::Digit4 => '4',
        KeyCode::Digit5 => '5',
        KeyCode::Digit6 => '6',
        KeyCode::Digit7 => '7',
        KeyCode::Digit8 => '8',
        KeyCode::Digit9 => '9',
        KeyCode::Backquote => '`',
        KeyCode::Minus => '-',
        KeyCode::Equal => '=',
        KeyCode::BracketLeft => '[',
        KeyCode::BracketRight => ']',
        KeyCode::Backslash => '\\',
        KeyCode::Semicolon => ';',
        KeyCode::Quote => '\'',
        KeyCode::Comma => ',',
        KeyCode::Period => '.',
        KeyCode::Slash => '/',
        KeyCode::Space => ' ',
        _ => return None,
    })
}

fn terminal_key_kind(input: &KeyInput) -> KeyKind {
    match input.state {
        ElementState::Released => KeyKind::Release,
        ElementState::Pressed if input.repeat => KeyKind::Repeat,
        ElementState::Pressed => KeyKind::Press,
    }
}

/// Builds the encoder event for one host key event under `platform`'s modifier rules.
/// `None` when the key has no encoder identity.
fn terminal_key_event(
    platform: crate::platform::PlatformKind,
    input: &KeyInput,
    modifiers: ModifiersState,
) -> Option<KeyEvent<'_>> {
    Some(KeyEvent {
        physical: terminal_physical_id(input.physical_key),
        key: terminal_key(input)?,
        base: us_base_char(input.physical_key),
        mods: terminal_key_mods(platform, modifiers),
        kind: terminal_key_kind(input),
        text: input.logical_text.as_deref(),
    })
}

/// Encodes one host key event with the terminal's current DECCKM and Kitty flags.
/// Returns the bytes to write to the PTY, or `None` when nothing is sent.
fn terminal_key_bytes(
    platform: crate::platform::PlatformKind,
    grid: &mut ronsole_term::Term,
    input: &KeyInput,
    modifiers: ModifiersState,
) -> Option<Vec<u8>> {
    let event = terminal_key_event(platform, input, modifiers)?;
    let mut out = Vec::new();
    match grid.encode_key(&event, &mut out) {
        KeyOutcome::Sent => Some(out),
        KeyOutcome::Text => input.logical_text.as_ref().map(|text| text.as_bytes().to_vec()),
        KeyOutcome::Ignore => None,
    }
}

/// Held-mask bit of a terminal modifier key, the flag it sets in the encoder's modifiers, and
/// the family mask of both keys of that modifier. `None` for keys that are not modifiers.
fn modifier_key_bits(modifier: ModKey) -> Option<(u8, ModifiersState, u8)> {
    const SHIFT_KEYS: u8 = 0b0001_0001;
    const CONTROL_KEYS: u8 = 0b0010_0010;
    const ALT_KEYS: u8 = 0b0100_0100;
    const SUPER_KEYS: u8 = 0b1000_1000;
    Some(match modifier {
        ModKey::LeftShift => (0b0000_0001, ModifiersState::SHIFT, SHIFT_KEYS),
        ModKey::RightShift => (0b0001_0000, ModifiersState::SHIFT, SHIFT_KEYS),
        ModKey::LeftCtrl => (0b0000_0010, ModifiersState::CONTROL, CONTROL_KEYS),
        ModKey::RightCtrl => (0b0010_0000, ModifiersState::CONTROL, CONTROL_KEYS),
        ModKey::LeftAlt => (0b0000_0100, ModifiersState::ALT, ALT_KEYS),
        ModKey::RightAlt => (0b0100_0000, ModifiersState::ALT, ALT_KEYS),
        ModKey::LeftSuper => (0b0000_1000, ModifiersState::SUPER, SUPER_KEYS),
        ModKey::RightSuper => (0b1000_0000, ModifiersState::SUPER, SUPER_KEYS),
        _ => return None,
    })
}

impl App {
    /// Records a physical terminal modifier key as held or released. Called for every host key
    /// before any early return, because the held state is physical, not routed.
    fn note_terminal_modifier_key(&mut self, input: &KeyInput) {
        let Some(TermKey::Modifier(modifier)) = terminal_key(input) else {
            return;
        };
        let Some((bit, _, _)) = modifier_key_bits(modifier) else {
            return;
        };
        let held = &mut self.ide_panel.terminal_held_modifier_keys;
        if input.state == ElementState::Pressed {
            *held |= bit;
        } else {
            *held &= !bit;
        }
    }

    /// Clears the held mask when the host reports no modifiers at all: a release that never
    /// arrived (for example after focus loss) must not leave its key marked held.
    pub(crate) fn sync_terminal_held_modifiers(&mut self) {
        if self.modifiers.is_empty() {
            self.ide_panel.terminal_held_modifier_keys = 0;
        }
    }

    /// Modifiers for the encoder event of `input`. The host learns the state after a key only
    /// from the following modifiers event, so a modifier key's own flag is set here: on press and
    /// repeat, and on release only while its other key of the same family is still held.
    fn terminal_event_modifiers(&self, input: &KeyInput) -> ModifiersState {
        let current = self.modifiers;
        let Some(TermKey::Modifier(modifier)) = terminal_key(input) else {
            return current;
        };
        let Some((_, flag, family)) = modifier_key_bits(modifier) else {
            return current;
        };
        let on = input.state == ElementState::Pressed || self.ide_panel.terminal_held_modifier_keys & family != 0;
        let mut modifiers = current;
        modifiers.remove(flag);
        if on {
            modifiers.insert(flag);
        }
        modifiers
    }
}

fn single_line_ime_text(text: &str) -> Cow<'_, str> {
    if text.contains(['\n', '\r']) {
        Cow::Owned(text.replace(['\n', '\r'], ""))
    } else {
        Cow::Borrowed(text)
    }
}

impl App {
    pub fn handle_main_ime_commit(&mut self, text: &str) {
        if text.is_empty() || self.startup_blocks_text_input() || self.markdown_toc.open {
            return;
        }
        let editor_was_focused = self.editor_has_input_focus();
        let consumed = self.handle_main_ime_commit_inner(text);
        if consumed {
            self.last_action = Instant::now();
            self.last_blink_state = true;
            self.request_redraw();
        }
        self.autosave_after_editor_focus_change(editor_was_focused);
    }

    fn handle_main_ime_commit_inner(&mut self, text: &str) -> bool {
        if self.handle_file_tree_modal_ime_commit(text)
            || self.modal_dialog_open()
            || self.ide_panel.project_search.help_open
        {
            return true;
        }

        if self.handle_database_ime_commit(text)
            || self.handle_settings_ime_commit(text)
            || self.handle_project_search_ime_commit(text)
            || self.handle_lsp_log_filter_ime_commit(text)
            || self.handle_git_message_ime_commit(text)
            || self.handle_api_client_ime_commit(text)
            || self.handle_lsp_logs_ime_commit()
            || self.handle_terminal_search_ime_commit(text)
            || self.handle_search_ime_commit(text)
            || self.handle_terminal_ime_commit(text)
        {
            return true;
        }

        self.handle_editor_ime_commit(text);
        true
    }

    fn handle_database_ime_commit(&mut self, text: &str) -> bool {
        if let Some(modal) = self.ide_panel.database.table_modal.as_mut() {
            let mut invalidate_table_modal_layout = false;
            match modal {
                crate::app::database::DatabaseTableModal::CustomLimit { input, error, .. } => {
                    let clean = single_line_ime_text(text);
                    if !clean.is_empty() {
                        input.insert(&clean, 16);
                        *error = None;
                    }
                }
                crate::app::database::DatabaseTableModal::MultilineEditor {
                    input, error, ..
                } => {
                    input.insert(text, crate::app::database::MAX_EDITABLE_MULTILINE_BYTES);
                    invalidate_table_modal_layout = true;
                    *error = None;
                }
                _ => {}
            }
            if invalidate_table_modal_layout {
                self.ide_panel
                    .database
                    .table_modal_layout_cache
                    .get_mut()
                    .invalidate();
            }
            return true;
        }
        if let Some(tab_id) = self.active_database_table_tab_id() {
            let mut completion_target = None;
            if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
                if !state.loading && state.metadata.is_none() && state.unavailable_text_focused {
                    return true;
                }
                let clean = single_line_ime_text(text);
                match state.grid.focused_input {
                    Some(crate::app::database::DatabaseTableInputTarget::Where) => {
                        if !clean.is_empty() {
                            state.grid.where_input.insert(&clean, 64 * 1024);
                        }
                        completion_target =
                            Some(crate::app::database::DatabaseTableInputTarget::Where);
                    }
                    Some(crate::app::database::DatabaseTableInputTarget::OrderBy) => {
                        if !clean.is_empty() {
                            state.grid.order_by_input.insert(&clean, 64 * 1024);
                        }
                        completion_target =
                            Some(crate::app::database::DatabaseTableInputTarget::OrderBy);
                    }
                    Some(crate::app::database::DatabaseTableInputTarget::Cell) => {
                        if let Some(editor) = state.grid.cell_editor.as_mut() {
                            if !clean.is_empty() {
                                editor.input.insert(
                                    &clean,
                                    crate::app::database::MAX_EDITABLE_MULTILINE_BYTES,
                                );
                            }
                            editor.error = None;
                        }
                        return true;
                    }
                    None => {}
                }
            }
            if let Some(target) = completion_target {
                self.show_active_database_table_filter_completion(target, false);
                return true;
            }
        }

        if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
            if let Some(field) = dialog.focused {
                let clean = single_line_ime_text(text);
                if !clean.is_empty() {
                    let max_bytes = if field.is_secret() { 4096 } else { 8192 };
                    dialog.input_mut(field).insert(&clean, max_bytes);
                    dialog.error = None;
                }
            }
            return true;
        }
        if self.database_blocking_modal_open() {
            return true;
        }
        false
    }

    fn handle_settings_ime_commit(&mut self, text: &str) -> bool {
        if self.show_settings && self.settings_tab == 0 && self.settings_ignore_focused {
            let clean = single_line_ime_text(text);
            if !clean.is_empty() {
                self.settings_ignore_editor.insert_str(&clean);
                self.settings_ignore_editor.sync_edits.clear();
            }
            return true;
        }
        if self.show_settings && self.settings_tab == 6 && self.keymap_settings.filter_focused {
            let clean = single_line_ime_text(text);
            if !clean.is_empty() && !clean.chars().any(char::is_control) {
                let mut filter = self.keymap_settings.filter.clone();
                filter.push_str(&clean);
                self.keymap_settings.update_filter(filter);
            }
            return true;
        }
        if self.show_settings {
            return true;
        }
        false
    }

    fn handle_project_search_ime_commit(&mut self, text: &str) -> bool {
        if !self.ide_panel.is_open(crate::app::PanelId::Search) {
            self.ide_panel.project_search.focused = None;
        }
        if let Some(field) = self.ide_panel.project_search.focused {
            if field == crate::app::project_search::ProjectSearchField::Filter
                && !self.ide_panel.project_search.filter_enabled()
            {
                self.ide_panel.project_search.focused = None;
                return true;
            }
            let clean = if field == crate::app::project_search::ProjectSearchField::Query {
                Cow::Borrowed(text)
            } else {
                single_line_ime_text(text)
            };
            if !clean.is_empty() {
                self.ide_panel.project_search.project_search_editor_mut(field).insert_str(&clean);
                self.ide_panel.project_search.project_search_editor_mut(field).sync_edits.clear();
                if field == crate::app::project_search::ProjectSearchField::Filter {
                    self.ide_panel.project_search.apply_live_filter();
                } else {
                    self.ide_panel.project_search.dirty = true;
                }
                if matches!(
                    field,
                    crate::app::project_search::ProjectSearchField::Include
                        | crate::app::project_search::ProjectSearchField::Exclude
                ) {
                    crate::save_panel_state(&self.ide_panel);
                }
                if field == crate::app::project_search::ProjectSearchField::Query {
                    self.sync_project_search_query_scroll(true);
                }
            }
            return true;
        }
        false
    }

    fn handle_lsp_log_filter_ime_commit(&mut self, text: &str) -> bool {
        if !self.ide_panel.is_open(crate::app::PanelId::LspServers) {
            self.ide_panel.lsp_log_filter_focused = false;
            self.ide_panel.lsp_logs_focused = None;
        }
        if self.ide_panel.lsp_log_filter_focused {
            let clean = single_line_ime_text(text);
            if !clean.is_empty() {
                self.ide_panel.lsp_log_filter_editor.insert_str(&clean);
                self.ide_panel.lsp_log_filter_editor.sync_edits.clear();
                self.ide_panel.lsp_log_filter_dirty = true;
            }
            return true;
        }
        false
    }

    fn handle_git_message_ime_commit(&mut self, text: &str) -> bool {
        if self.ide_panel.git.message_focused && self.ide_panel.is_open(crate::app::PanelId::Git) {
            let clean = single_line_ime_text(text);
            if !clean.is_empty() {
                self.ide_panel.git.message_editor.insert_str(&clean);
                self.ide_panel.git.message_editor.sync_edits.clear();
            }
            return true;
        }
        false
    }

    fn handle_lsp_logs_ime_commit(&mut self) -> bool {
        if self.ide_panel.is_open(crate::app::PanelId::LspServers)
            && self.ide_panel.lsp_logs_focused.is_some()
        {
            return true;
        }
        false
    }

    fn handle_terminal_search_ime_commit(&mut self, text: &str) -> bool {
        if self.ide_panel.is_open(crate::app::PanelId::Terminal)
            && self.ide_panel.term_show_search
            && self.ide_panel.term_search_focused
        {
            let clean = single_line_ime_text(text);
            if !clean.is_empty() {
                self.ide_panel.term_search_editor.insert_str(&clean);
                self.ide_panel.term_search_editor.sync_edits.clear();
                self.update_terminal_search();
                self.jump_to_terminal_search_result();
            }
            return true;
        }
        false
    }

    fn handle_search_ime_commit(&mut self, text: &str) -> bool {
        if self.show_search && self.search_focused {
            let clean = single_line_ime_text(text);
            if !clean.is_empty() {
                self.search_editor.insert_str(&clean);
                self.search_editor.sync_edits.clear();
                self.update_search();
                self.jump_to_search_result();
            }
            return true;
        }
        false
    }

    fn handle_terminal_ime_commit(&mut self, text: &str) -> bool {
        if self.is_ide_mode
            && self.ide_panel.terminal_focused
            && self.ide_panel.is_open(crate::app::PanelId::Terminal)
        {
            if let Some(terminal) = self.ide_panel.terminals.get(self.ide_panel.active_terminal) {
                let _ = terminal.write_input(text.as_bytes());
            }
            return true;
        }
        false
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn handle_terminal_keyboard_input(&mut self, key_event: KeyInput) {
        let primary = crate::platform::primary_shortcut_modifier(self.modifiers);
        let terminal_ctrl = crate::platform::terminal_control_modifier(self.modifiers);

        if key_event.state == winit::event::ElementState::Pressed
            && primary
            && key_event.physical_key == PhysicalKey::Code(KeyCode::KeyF)
        {
            self.ide_panel.term_show_search = true;
            self.ide_panel.term_search_focused = true;
            self.ide_panel.term_search_editor.select_all();
            self.ide_panel.term_search_current_idx = None;
            self.update_terminal_search();
            self.jump_to_terminal_search_result();
            self.last_action = std::time::Instant::now();
            self.request_redraw();
            return;
        }

        let physical = key_event.physical_key;
        let pressed = key_event.state == winit::event::ElementState::Pressed;
        let event_modifiers = self.terminal_event_modifiers(&key_event);
        // A Press consumed by a host chord (Ctrl+C with a selection, Ctrl+V) acts once: its
        // Repeat and Release never reach the encoder, until that key is released.
        if self.ide_panel.terminal_host_held_key == Some(physical) {
            if !pressed {
                self.ide_panel.terminal_host_held_key = None;
            }
            if !pressed || key_event.repeat {
                return;
            }
        }
        if pressed && !key_event.repeat {
            self.ide_panel.terminal_host_held_key = None;
        }
        let paste = if pressed && primary && key_event.physical_key == PhysicalKey::Code(KeyCode::KeyV) {
            #[cfg(target_os = "linux")]
            {
                let file_list = self.get_clipboard_file_list();
                terminal_clipboard_paste_bytes(file_list.as_deref(), None).or_else(|| {
                    let text = self.get_clipboard_text();
                    terminal_clipboard_paste_bytes(None, text.as_deref())
                })
            }
            #[cfg(not(target_os = "linux"))]
            {
                self.get_clipboard_text().map(String::into_bytes)
            }
        } else {
            None
        };
        let active = self.ide_panel.active_terminal;
        let mut clipboard_text = None;
        let mut host_consumed = false;
        let mut redraw = false;
        if let Some(term) = self.ide_panel.terminals.get_mut(active) {
            let mut grid = term.lock_grid_for_key();
            let input = match physical {
                PhysicalKey::Code(KeyCode::KeyC) if pressed && (primary || terminal_ctrl) => {
                    if primary && grid.selection().is_some() {
                        clipboard_text = crate::app::terminal::terminal_selection_text(&grid);
                        grid.set_selection(None);
                        host_consumed = true;
                        redraw = true;
                        None
                    } else if terminal_ctrl {
                        terminal_key_bytes(crate::platform::CURRENT_PLATFORM, &mut grid, &key_event, event_modifiers)
                    } else {
                        None
                    }
                }
                PhysicalKey::Code(KeyCode::KeyV) if pressed && primary => {
                    host_consumed = true;
                    paste.map(|payload| {
                        ronsole_term::bracketed_paste_bytes(payload, grid.bracketed_paste())
                    })
                }
                _ => terminal_key_bytes(crate::platform::CURRENT_PLATFORM, &mut grid, &key_event, event_modifiers),
            };
            drop(grid);
            if let Some(input) = input {
                let _ = term.write_input(&input);
                redraw = true;
            }
        }
        if host_consumed {
            self.ide_panel.terminal_host_held_key = Some(physical);
        }
        if let Some(text) = clipboard_text {
            self.set_clipboard_text(text);
        }
        if redraw {
            self.last_action = std::time::Instant::now();
            self.request_redraw();
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn handle_terminal_search_keyboard_input(&mut self, key_event: KeyInput) {
        if key_event.state == winit::event::ElementState::Pressed {
            let ctrl = crate::platform::primary_shortcut_modifier(self.modifiers);
            let word = crate::platform::word_navigation_modifier(self.modifiers);
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
                        crate::app::terminal::lock_terminal_grid(&term.grid).set_selection(None);
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
                    if word {
                        self.ide_panel.term_search_editor.move_word_left(shift);
                    } else {
                        self.ide_panel.term_search_editor.move_left(shift);
                    }
                }
                PhysicalKey::Code(KeyCode::ArrowRight) => {
                    if word {
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
                    if crate::platform::text_input_modifiers_allowed(self.modifiers) {
                        if let Some(txt) = key_event.logical_text.as_deref() {
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
            self.request_redraw();
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn handle_search_keyboard_input(
        &mut self,
        key_event: KeyInput,
        chord: Option<crate::keymap::Chord>,
    ) {
        if key_event.state == ElementState::Pressed {
            let ctrl = crate::platform::primary_shortcut_modifier(self.modifiers);
            let word = crate::platform::word_navigation_modifier(self.modifiers);
            let shift = self.modifiers.shift_key();
            let mut is_edit = false;

            match key_event.physical_key {
                PhysicalKey::Code(KeyCode::Escape) => {
                    self.show_search = false;
                    self.search_focused = false;
                    self.search_results.clear();
                    self.search_current_idx = None;
                    self.pdf_restart_search_if_open();
                    self.request_redraw();
                }
                _ if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::SearchEditorOpen, chord)) => {
                    self.search_editor.select_all();
                }
                PhysicalKey::Code(KeyCode::Enter | KeyCode::ArrowUp | KeyCode::ArrowDown)
                    if self.active_pdf_tab().is_some() =>
                {
                    let backward = key_event.physical_key == PhysicalKey::Code(KeyCode::ArrowUp)
                        || (key_event.physical_key == PhysicalKey::Code(KeyCode::Enter) && shift);
                    self.pdf_jump_search(!backward);
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
                    if word {
                        self.search_editor.move_word_left(shift);
                    } else {
                        self.search_editor.move_left(shift);
                    }
                }
                PhysicalKey::Code(KeyCode::ArrowRight) => {
                    if word {
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
                    if crate::platform::text_input_modifiers_allowed(self.modifiers) {
                        if let Some(txt) = key_event.logical_text.as_deref() {
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
            self.request_redraw();
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn handle_project_search_keyboard_input(
        &mut self,
        key_event: KeyInput,
        chord: Option<crate::keymap::Chord>,
    ) {
        if key_event.state != ElementState::Pressed {
            return;
        }
        let Some(field) = self.ide_panel.project_search.focused_keyboard_field() else {
            return;
        };
        let ctrl = crate::platform::primary_shortcut_modifier(self.modifiers);
        let word = crate::platform::word_navigation_modifier(self.modifiers);
        let shift = self.modifiers.shift_key();
        let is_paste = ctrl
            && matches!(
                key_event.physical_key,
                PhysicalKey::Code(KeyCode::KeyV)
            );
        let clipboard_text = is_paste.then(|| self.get_clipboard_text()).flatten();
        let outcome = self.ide_panel.project_search.apply_keyboard_input(
            field,
            key_event.physical_key,
            key_event.logical_text.as_deref(),
            ctrl,
            chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::SearchProjectRun, chord)),
            word,
            shift,
            crate::platform::text_input_modifiers_allowed(self.modifiers),
            clipboard_text,
        );
        let mut is_edit = outcome.edited;
        if let Some(clipboard) = outcome.clipboard {
            self.set_clipboard_text(clipboard.text);
            if clipboard.cut {
                self.ide_panel
                    .project_search
                    .project_search_editor_mut(field)
                    .delete_selection();
                is_edit = true;
            }
        }
        let post_edit = self
            .ide_panel
            .project_search
            .finish_keyboard_edit(field, is_edit);
        if post_edit.persist_panel_state {
            crate::save_panel_state(&self.ide_panel);
        }
        if field == crate::app::project_search::ProjectSearchField::Query
            && self.ide_panel.project_search.focused
                == Some(crate::app::project_search::ProjectSearchField::Query)
        {
            self.sync_project_search_query_scroll(post_edit.refresh_query_width);
        }
        if outcome.should_run {
            self.start_project_search();
        }
        self.last_action = Instant::now();
        self.request_redraw();
    }

    pub fn handle_lsp_log_filter_keyboard_input(&mut self, key_event: KeyInput) {
        if key_event.state == ElementState::Pressed {
            let ctrl = crate::platform::primary_shortcut_modifier(self.modifiers);
            let word = crate::platform::word_navigation_modifier(self.modifiers);
            let shift = self.modifiers.shift_key();
            let mut is_edit = false;

            match key_event.physical_key {
                PhysicalKey::Code(KeyCode::Escape) => {
                    self.ide_panel.lsp_log_filter_focused = false;
                }
                PhysicalKey::Code(KeyCode::ArrowLeft) => {
                    if word {
                        self.ide_panel.lsp_log_filter_editor.move_word_left(shift);
                    } else {
                        self.ide_panel.lsp_log_filter_editor.move_left(shift);
                    }
                }
                PhysicalKey::Code(KeyCode::ArrowRight) => {
                    if word {
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
                    if crate::platform::text_input_modifiers_allowed(self.modifiers) {
                        if let Some(txt) = key_event.logical_text.as_deref() {
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
            self.request_redraw();
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn handle_git_message_keyboard_input(&mut self, key_event: KeyInput) {
        if key_event.state == ElementState::Pressed {
            let ctrl = crate::platform::primary_shortcut_modifier(self.modifiers);
            let word = crate::platform::word_navigation_modifier(self.modifiers);
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
                    if word {
                        self.ide_panel.git.message_editor.move_word_left(shift);
                    } else {
                        self.ide_panel.git.message_editor.move_left(shift);
                    }
                }
                PhysicalKey::Code(KeyCode::ArrowRight) => {
                    if word {
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
                    if word {
                        self.ide_panel.git.message_editor.delete_word_backward();
                        is_edit = true;
                    } else if self.ide_panel.git.message_editor.backspace().is_some() {
                        is_edit = true;
                    }
                }
                PhysicalKey::Code(KeyCode::Delete) => {
                    if word {
                        self.ide_panel.git.message_editor.delete_word_forward();
                        is_edit = true;
                    } else if self.ide_panel.git.message_editor.delete_forward().is_some() {
                        is_edit = true;
                    }
                }
                _ => {
                    if crate::platform::text_input_modifiers_allowed(self.modifiers) {
                        if let Some(txt) = key_event.logical_text.as_deref() {
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
            self.request_redraw();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn terminal_file_list_paste_keeps_simple_unicode_paths_raw_without_submit_bytes() {
        for path in [
            "/home/reyan/test.txt",
            "/home/reyan/Загрузки/test.patch",
            "/home/reyan/Документы/test.txt",
            "/tmp/файл.txt",
        ] {
            let pasted = terminal_clipboard_paste_bytes(Some(&[PathBuf::from(path)]), None)
                .expect("file-list path should paste");
            assert_eq!(pasted, path.as_bytes(), "path={path:?}");
            assert_ne!(pasted.first(), Some(&b'\''), "path={path:?}");
            assert_ne!(pasted.last(), Some(&b'\''), "path={path:?}");
            assert!(!pasted.contains(&b'\r') && !pasted.contains(&b'\n'));
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn terminal_file_list_paste_escapes_shell_syntax_and_keeps_multiple_arguments() {
        for (path, expected) in [
            (
                "/home/reyan/My File.txt",
                b"/home/reyan/My\\ File.txt".as_slice(),
            ),
            ("/tmp/O'Brien.txt", b"/tmp/O\\'Brien.txt".as_slice()),
            ("/tmp/back\\slash", b"/tmp/back\\\\slash".as_slice()),
            ("/tmp/$HOME;file", b"/tmp/\\$HOME\\;file".as_slice()),
        ] {
            assert_eq!(
                terminal_clipboard_paste_bytes(Some(&[PathBuf::from(path)]), None),
                Some(expected.to_vec()),
                "path={path:?}"
            );
        }

        let files = [
            PathBuf::from("/tmp/файл.txt"),
            PathBuf::from("/home/reyan/My Directory"),
        ];
        assert_eq!(
            terminal_clipboard_paste_bytes(Some(&files), Some("ignored text")),
            Some(
                "/tmp/файл.txt /home/reyan/My\\ Directory"
                    .as_bytes()
                    .to_vec()
            )
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn terminal_file_list_paste_normalizes_uri_list_transport_cr_before_formatting() {
        let expected = "/home/reyan/Загрузки/file.patch";
        let mut files = [PathBuf::from(format!("{expected}\r"))];
        crate::platform::normalize_linux_arboard_file_list(&mut files);
        let pasted = terminal_clipboard_paste_bytes(Some(&files), None).unwrap();
        assert_eq!(pasted, expected.as_bytes());
        assert!(!pasted.contains(&b'\r') && !pasted.contains(&b'\n'));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn editor_paste_stays_on_text_clipboard_path() {
        let editor_keys = include_str!("keyboard/editor_keys.rs");
        assert!(editor_keys.contains("self.get_clipboard_text()"));
        assert!(!editor_keys.contains("get_clipboard_file_list"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn terminal_file_list_paste_falls_back_to_ordinary_text() {
        assert_eq!(
            terminal_clipboard_paste_bytes(None, Some("echo hello")),
            Some(b"echo hello".to_vec())
        );
        assert_eq!(
            terminal_clipboard_paste_bytes(Some(&[]), Some("echo hello")),
            Some(b"echo hello".to_vec())
        );
        assert_eq!(
            terminal_clipboard_paste_bytes(None, Some("echo one\necho two")),
            Some(b"echo one\necho two".to_vec())
        );
        assert_eq!(terminal_clipboard_paste_bytes(None, None), None);
    }

    #[test]
    fn ime_single_line_text_preserves_unicode_and_removes_line_breaks() {
        assert_eq!(single_line_ime_text("Привет 🌍"), "Привет 🌍");
        assert_eq!(single_line_ime_text("one\r\ntwo\nthree"), "onetwothree");
        assert_eq!(single_line_ime_text("\r\n"), "");
    }

    fn host_key(
        code: KeyCode,
        unshifted: Option<char>,
        text: Option<&str>,
        state: ElementState,
        repeat: bool,
    ) -> KeyInput {
        KeyInput {
            physical_key: PhysicalKey::Code(code),
            logical_text: text.map(winit::keyboard::SmolStr::new),
            unshifted,
            text: None,
            state,
            repeat,
        }
    }

    fn term_with_kitty_flags(flags: u8) -> ronsole_term::Term {
        let mut term = crate::app::terminal::test_term(80, 24);
        crate::app::terminal::advance_test_output(&mut term, format!("\x1b[>{flags}u").as_bytes());
        term
    }

    #[test]
    fn shift_enter_legacy_is_esc_cr() {
        let mut term = term_with_kitty_flags(0);
        let input = host_key(KeyCode::Enter, None, Some("\r"), ElementState::Pressed, false);
        assert_eq!(
            terminal_key_bytes(crate::platform::CURRENT_PLATFORM, &mut term, &input, ModifiersState::SHIFT),
            Some(b"\x1b\r".to_vec())
        );
    }

    #[test]
    fn shift_enter_kitty_flag1() {
        let mut term = term_with_kitty_flags(1);
        let input = host_key(KeyCode::Enter, None, Some("\r"), ElementState::Pressed, false);
        assert_eq!(
            terminal_key_bytes(crate::platform::CURRENT_PLATFORM, &mut term, &input, ModifiersState::SHIFT),
            Some(b"\x1b[13;2u".to_vec())
        );
    }

    #[test]
    fn ctrl_c_without_selection_through_encoder() {
        let mut term = term_with_kitty_flags(0);
        let input = host_key(KeyCode::KeyC, Some('c'), Some("c"), ElementState::Pressed, false);
        assert_eq!(
            terminal_key_bytes(crate::platform::CURRENT_PLATFORM, &mut term, &input, ModifiersState::CONTROL),
            Some(vec![0x03])
        );
    }

    #[test]
    fn windows_altgr_text_not_csi_u() {
        let mut term = term_with_kitty_flags(1);
        let input = host_key(KeyCode::KeyQ, Some('q'), Some("@"), ElementState::Pressed, false);
        let altgr = ModifiersState::CONTROL | ModifiersState::ALT;
        assert_eq!(
            terminal_key_bytes(crate::platform::PlatformKind::Windows, &mut term, &input, altgr),
            Some(b"@".to_vec())
        );
    }

    #[test]
    fn linux_ctrl_alt_letter_is_csi_u_flag1() {
        let mut term = term_with_kitty_flags(1);
        let input = host_key(KeyCode::KeyA, Some('a'), Some("a"), ElementState::Pressed, false);
        let ctrl_alt = ModifiersState::CONTROL | ModifiersState::ALT;
        assert_eq!(
            terminal_key_bytes(crate::platform::PlatformKind::Linux, &mut term, &input, ctrl_alt),
            Some(b"\x1b[97;7u".to_vec())
        );
    }

    #[test]
    fn windows_ctrl_alt_without_text_strips_modifiers() {
        let altgr = ModifiersState::CONTROL | ModifiersState::ALT;
        let windows = terminal_key_mods(crate::platform::PlatformKind::Windows, altgr);
        assert!(!windows.ctrl && !windows.alt);
        let linux = terminal_key_mods(crate::platform::PlatformKind::Linux, altgr);
        assert!(linux.ctrl && linux.alt);
    }

    #[test]
    fn physical_id_stable_and_tagged() {
        let key_a = terminal_physical_id(PhysicalKey::Code(KeyCode::KeyA));
        assert_eq!(key_a, terminal_physical_id(PhysicalKey::Code(KeyCode::KeyA)));
        assert_ne!(key_a, terminal_physical_id(PhysicalKey::Code(KeyCode::KeyB)));
        assert_eq!(key_a >> 63, 1);
        assert_eq!(
            terminal_physical_id(PhysicalKey::Unidentified(NativeKeyCode::Xkb(5))),
            (4_u64 << 32) | 5
        );
    }

    #[test]
    fn bracketed_paste_wraps_when_enabled() {
        let mut term = term_with_kitty_flags(0);
        crate::app::terminal::advance_test_output(&mut term, b"\x1b[?2004h");
        assert_eq!(
            ronsole_term::bracketed_paste_bytes(b"ls".to_vec(), term.bracketed_paste()),
            b"\x1b[200~ls\x1b[201~".to_vec()
        );
    }

    /// Encodes `input` against the terminal's grid, the way the key handler does under its lock.
    fn terminal_key_bytes_on(
        term: &crate::app::terminal::Terminal,
        input: &KeyInput,
        modifiers: ModifiersState,
    ) -> Option<Vec<u8>> {
        let mut grid = crate::app::terminal::lock_terminal_grid(&term.grid);
        terminal_key_bytes(crate::platform::CURRENT_PLATFORM, &mut grid, input, modifiers)
    }

    /// A focus change clears the kitty sent set before the key handler encodes the next key, so the
    /// release of a key pressed before the change is ignored (ronsole-term tests/keys.rs,
    /// `focus_loss_clears_sent_set`).
    #[test]
    fn focus_loss_drops_release_of_key_pressed_before_it() {
        let mut term = crate::app::terminal::Terminal::test_without_process(80, 24);
        {
            let mut grid = crate::app::terminal::lock_terminal_grid(&term.grid);
            // Kitty flags 3: disambiguate and report event types.
            crate::app::terminal::advance_test_output(&mut grid, b"\x1b[>3u");
        }
        let press = host_key(KeyCode::KeyA, Some('a'), Some("a"), ElementState::Pressed, false);
        let release = host_key(KeyCode::KeyA, Some('a'), Some("a"), ElementState::Released, false);

        term.report_focus(true);
        let _ = term.take_focus_key_reset();
        // Ctrl+A press, CSI u with modifier 5 (ronsole-term tests/keys.rs, `c` case: `CSI 99;5u`).
        assert_eq!(
            terminal_key_bytes_on(&term, &press, ModifiersState::CONTROL),
            Some(b"\x1b[97;5u".to_vec())
        );

        term.report_focus(false);
        // Release of the key pressed before the focus loss is not sent.
        {
            let mut grid = term.lock_grid_for_key();
            assert_eq!(
                terminal_key_bytes(crate::platform::CURRENT_PLATFORM, &mut grid, &release, ModifiersState::empty()),
                None
            );
        }
    }

    /// Control for `focus_loss_drops_release_of_key_pressed_before_it`: without a focus change the
    /// release of the same press is sent, so the ignored release above comes from the cleared set.
    /// Expected form `CSI 97;1:3u` (ronsole-term tests/keys.rs, release of `q`: `CSI 113;1:3u`).
    #[test]
    fn release_without_focus_change_is_sent() {
        let mut term = crate::app::terminal::Terminal::test_without_process(80, 24);
        {
            let mut grid = crate::app::terminal::lock_terminal_grid(&term.grid);
            crate::app::terminal::advance_test_output(&mut grid, b"\x1b[>3u");
        }
        let press = host_key(KeyCode::KeyA, Some('a'), Some("a"), ElementState::Pressed, false);
        let release = host_key(KeyCode::KeyA, Some('a'), Some("a"), ElementState::Released, false);

        term.report_focus(true);
        let _ = term.take_focus_key_reset();
        assert_eq!(
            terminal_key_bytes_on(&term, &press, ModifiersState::CONTROL),
            Some(b"\x1b[97;5u".to_vec())
        );
        assert_eq!(
            terminal_key_bytes_on(&term, &release, ModifiersState::empty()),
            Some(b"\x1b[97;1:3u".to_vec())
        );
    }
}
