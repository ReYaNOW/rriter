use crate::keymap::{Chord, Command, KeyContext, KeymapOverrides};
use crate::app::App;
use crate::app::keyboard::KeyInput;
use crate::editor::Editor;
use winit::event::ElementState;
use winit::keyboard::{KeyCode, PhysicalKey};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Recording {
    pub command: Command,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Conflict {
    pub command: Command,
    pub chord: Chord,
    pub owners: Vec<Command>,
    pub owner_labels: String,
}

pub(crate) struct KeymapSettingsState {
    pub filter: String,
    pub(crate) filter_lower: String,
    pub(crate) filter_input: Editor,
    pub filter_focused: bool,
    pub recording: Option<Recording>,
    pub pending_conflict: Option<Conflict>,
    pub hint: Option<&'static str>,
    pub scroll: crate::scroll::ScrollState,
    pub max_scroll: f32,
    pub(crate) rows: Vec<KeymapSettingsRow>,
    pub(crate) skipped_label: Option<String>,
}

pub(crate) struct KeymapSettingsRow {
    pub command: Command,
    pub context: KeyContext,
    pub label: &'static str,
    pub id: &'static str,
    pub chords: Vec<String>,
    pub override_state: OverrideState,
    pub warning: RowWarning,
    pub search: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OverrideState {
    Default,
    Array,
    Scalar,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowWarning {
    None,
    Conflict,
    TerminalIntercept,
    ConflictAndTerminalIntercept,
}

pub(crate) struct KeymapSettingsClick {
    pub redraw: bool,
    pub reset_all: bool,
    pub overrides: Option<KeymapOverrides>,
}

impl Default for KeymapSettingsState {
    fn default() -> Self {
        Self {
            filter: String::new(),
            filter_lower: String::new(),
            filter_input: Editor::new(256),
            filter_focused: false,
            recording: None,
            pending_conflict: None,
            hint: None,
            scroll: crate::scroll::ScrollState::new(7.0),
            max_scroll: 0.0,
            rows: Vec::new(),
            skipped_label: None,
        }
    }
}

impl KeymapSettingsState {
    pub(crate) fn handle_settings_click(
        &mut self,
        id: crate::ui_system::UiId,
        keymap: &crate::keymap::Keymap,
        current_overrides: &KeymapOverrides,
        scrollbar: Option<((f32, f32, f32, f32), f32, f32)>,
    ) -> Option<KeymapSettingsClick> {
        let mut result = KeymapSettingsClick { redraw: false, reset_all: false, overrides: None };
        match id {
            crate::ui_system::UiId::SettingsKeymapFilter => {
                self.filter_focused = true;
                result.redraw = true;
            }
            crate::ui_system::UiId::SettingsKeymapAdd(index) => {
                if let Some(info) = crate::keymap::COMMANDS.get(index) {
                    self.begin_recording(info.command);
                    result.redraw = true;
                }
            }
            crate::ui_system::UiId::SettingsKeymapRemove(index, chord_index) => {
                if let Some(info) = crate::keymap::COMMANDS.get(index)
                    && let Some(chord) = keymap.chords(info.command).get(chord_index).copied()
                {
                    let mut overrides = current_overrides.clone();
                    overrides.remove_chord(crate::platform::CURRENT_PLATFORM, info.command, chord);
                    result.overrides = Some(overrides);
                }
            }
            crate::ui_system::UiId::SettingsKeymapReset(index) => {
                if let Some(info) = crate::keymap::COMMANDS.get(index) {
                    let mut overrides = current_overrides.clone();
                    overrides.reset_command(info.command);
                    result.overrides = Some(overrides);
                }
            }
            crate::ui_system::UiId::SettingsKeymapResetAll => {
                self.cancel_recording();
                result.reset_all = true;
            }
            crate::ui_system::UiId::SettingsKeymapConflictCancel => {
                self.cancel_recording();
                result.redraw = true;
            }
            crate::ui_system::UiId::SettingsKeymapConflictAccept => {
                if let Some((command, chord, owners)) = self.confirm_conflict() {
                    let mut overrides = current_overrides.clone();
                    overrides.reassign(crate::platform::CURRENT_PLATFORM, chord, command, &owners);
                    result.overrides = Some(overrides);
                }
            }
            crate::ui_system::UiId::SettingsKeymapScrollY => {
                if let Some((rect, scale, pointer)) = scrollbar {
                    let bar = crate::render_view::settings_ui::settings_scrollbar(rect, rect.3, self.max_scroll, self.scroll.current, 6.0, crate::render_view::settings_ui::KEYMAP_SCROLLBAR_MIN_THUMB, [0.7, 0.33, 0.54, 1.0]);
                    crate::app::mouse::press_scrollbar(&mut self.scroll, bar.geometry(scale), 0.0, pointer);
                }
            }
            _ => return None,
        }
        Some(result)
    }

    pub fn refresh(&mut self, keymap: &crate::keymap::Keymap, overrides: &KeymapOverrides) {
        let raw = overrides.to_value();
        self.rows.clear();
        for info in crate::keymap::COMMANDS {
            let chords = keymap.chords(info.command).iter().map(|chord| chord.display(crate::platform::CURRENT_PLATFORM)).collect::<Vec<_>>();
            let mut search = format!("{} {}", info.label, info.id).to_lowercase();
            for chord in keymap.chords(info.command) {
                search.push(' ');
                search.push_str(&chord.serialize(crate::platform::CURRENT_PLATFORM).to_lowercase());
            }
            for chord in &chords { search.push(' '); search.push_str(&chord.to_lowercase()); }
            self.rows.push(KeymapSettingsRow {
                command: info.command,
                context: info.context,
                label: info.label,
                id: info.id,
                chords,
                override_state: match (overrides.has(info.command), raw.get(info.id).is_some_and(|value| !value.is_array())) {
                    (false, _) => OverrideState::Default,
                    (true, true) => OverrideState::Scalar,
                    (true, false) => OverrideState::Array,
                },
                warning: match (
                    keymap.conflicted(info.command),
                    matches!(info.context, KeyContext::Global | KeyContext::Terminal)
                        && keymap.chords(info.command).iter().any(|chord| crate::app::keyboard::input_owner::terminal_intercepts(*chord, crate::platform::CURRENT_PLATFORM)),
                ) {
                    (false, false) => RowWarning::None,
                    (true, false) => RowWarning::Conflict,
                    (false, true) => RowWarning::TerminalIntercept,
                    (true, true) => RowWarning::ConflictAndTerminalIntercept,
                },
                search,
            });
        }
        self.skipped_label = (!keymap.skipped().is_empty()).then(|| format!("Пропущено {} записей keymap", keymap.skipped().len()));
    }

    pub fn update_filter(&mut self, filter: String) {
        self.set_filter(filter);
        self.filter_input.set_text_clean(&self.filter);
    }

    pub fn set_filter(&mut self, filter: String) {
        if self.filter != filter {
            self.filter_lower = filter.to_lowercase();
            self.filter = filter;
            self.scroll.current = 0.0;
            self.scroll.target = 0.0;
            self.scroll.velocity = 0.0;
        }
    }

    pub fn row_matches(&self, row: &KeymapSettingsRow) -> bool {
        self.filter_lower.is_empty() || row.search.contains(&self.filter_lower)
    }

    pub fn begin_recording(&mut self, command: Command) {
        self.filter_focused = false;
        self.recording = Some(Recording { command });
        self.pending_conflict = None;
        self.hint = Some("Нажмите сочетание…");
    }

    pub fn cancel_recording(&mut self) {
        self.recording = None;
        self.pending_conflict = None;
        self.hint = None;
    }

    pub fn record(&mut self, chord: Chord, keymap: &crate::keymap::Keymap) -> RecordResult {
        let Some(recording) = self.recording else { return RecordResult::Ignored; };
        match crate::keymap::validate(crate::platform::CURRENT_PLATFORM, chord) {
            Ok(()) => {}
            Err(crate::keymap::ChordError::Reserved) => {
                self.hint = Some("зарезервировано");
                return RecordResult::Rejected;
            }
            Err(crate::keymap::ChordError::NeedsModifier) => {
                self.hint = Some("нужен Ctrl/Alt/⌘");
                return RecordResult::Rejected;
            }
            Err(crate::keymap::ChordError::UnknownKey) => return RecordResult::Rejected,
        }
        if keymap.chords(recording.command).contains(&chord) {
            self.cancel_recording();
            return RecordResult::Unchanged;
        }
        let owners = keymap.conflicting_commands(chord, recording.command);
        if owners.is_empty() {
            self.cancel_recording();
            if command_context(recording.command) == KeyContext::Global
                && crate::app::keyboard::input_owner::terminal_intercepts(chord, crate::platform::CURRENT_PLATFORM)
            {
                self.hint = Some("В фокусе терминала сочетание перехватывает терминал");
            }
            return RecordResult::Add { command: recording.command, chord };
        }
        let owner_labels = owners.iter().map(|command| crate::keymap::COMMANDS[*command as usize].label).collect::<Vec<_>>().join(", ");
        self.pending_conflict = Some(Conflict { command: recording.command, chord, owners, owner_labels });
        self.hint = Some("Занято. Переназначить?");
        RecordResult::Conflict
    }

    pub fn confirm_conflict(&mut self) -> Option<(Command, Chord, Vec<Command>)> {
        let conflict = self.pending_conflict.take()?;
        self.recording = None;
        self.hint = None;
        Some((conflict.command, conflict.chord, conflict.owners))
    }

    pub fn handle_recording_key(&mut self, key: PhysicalKey, chord: Option<Chord>, keymap: &crate::keymap::Keymap) -> RecordResult {
        if self.recording.is_none() { return RecordResult::Ignored; }
        if key == PhysicalKey::Code(KeyCode::Escape) && chord.is_some_and(|chord| chord.mods.is_empty()) {
            self.cancel_recording();
            return RecordResult::Cancelled;
        }
        if matches!(key, PhysicalKey::Code(KeyCode::ControlLeft | KeyCode::ControlRight | KeyCode::AltLeft | KeyCode::AltRight | KeyCode::ShiftLeft | KeyCode::ShiftRight | KeyCode::SuperLeft | KeyCode::SuperRight)) {
            return RecordResult::Ignored;
        }
        chord.map_or(RecordResult::Rejected, |chord| self.record(chord, keymap))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecordResult {
    Ignored,
    Cancelled,
    Rejected,
    Unchanged,
    Conflict,
    Add { command: Command, chord: Chord },
}

fn command_context(command: Command) -> KeyContext {
    crate::keymap::COMMANDS[command as usize].context
}

impl App {
    pub(crate) fn handle_keymap_settings_key(&mut self, key_event: &KeyInput, chord: Option<Chord>) -> bool {
        if !self.show_settings || self.settings_tab != 6 || key_event.state != ElementState::Pressed { return false; }
        if self.keymap_settings.pending_conflict.is_some() {
            if key_event.physical_key == PhysicalKey::Code(KeyCode::Escape) && chord.is_some_and(|chord| chord.mods.is_empty()) { self.keymap_settings.cancel_recording(); }
            if let Some(window) = self.window.as_ref() { window.request_redraw(); }
            return true;
        }
        if self.keymap_settings.filter_focused && self.keymap_settings.recording.is_none() {
            if key_event.physical_key == PhysicalKey::Code(KeyCode::F1) {
                return false;
            }
            if key_event.physical_key == PhysicalKey::Code(KeyCode::Escape) && chord.is_some_and(|chord| chord.mods.is_empty()) {
                self.keymap_settings.filter_focused = false;
            } else {
                let primary = crate::platform::primary_shortcut_modifier(self.modifiers);
                let word = crate::platform::word_navigation_modifier(self.modifiers);
                let shift = self.modifiers.shift_key();
                let is_paste = primary && key_event.physical_key == PhysicalKey::Code(KeyCode::KeyV);
                let paste_text = is_paste.then(|| self.get_clipboard_text()).flatten();
                let copied = crate::app::single_line_input::handle_single_line_input(
                    &mut self.keymap_settings.filter_input,
                    key_event.physical_key,
                    key_event.text.as_deref(),
                    primary,
                    word,
                    shift,
                    crate::platform::text_input_modifiers_allowed(self.modifiers),
                    paste_text.as_deref(),
                    256,
                );
                if let Some(copied) = copied { self.set_clipboard_text(copied); }
                let filter = self.keymap_settings.filter_input.get_full_text();
                self.keymap_settings.set_filter(filter);
            }
            if let Some(window) = self.window.as_ref() { window.request_redraw(); }
            return true;
        }
        let result = self.keymap_settings.handle_recording_key(key_event.physical_key, chord, &self.keymap);
        match result {
            RecordResult::Add { command, chord } => {
                let mut overrides = self.keymap_overrides.clone();
                overrides.add_chord(crate::platform::CURRENT_PLATFORM, command, chord);
                self.set_keymap_overrides(overrides);
            }
            RecordResult::Ignored => return false,
            RecordResult::Cancelled | RecordResult::Rejected | RecordResult::Unchanged | RecordResult::Conflict => {}
        }
        if let Some(window) = self.window.as_ref() { window.request_redraw(); }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{KeymapSettingsState, RecordResult};
    use crate::keymap::{Chord, Command, Keymap, KeymapOverrides};
    use winit::keyboard::{KeyCode, PhysicalKey};

    fn chord(text: &str) -> Chord {
        Chord::parse(crate::platform::CURRENT_PLATFORM, text).expect("test chord parses")
    }

    #[test]
    fn recording_rejects_reserved_chords_without_ending_capture() {
        let keymap = Keymap::build(&KeymapOverrides::default());
        let mut state = KeymapSettingsState::default();
        state.begin_recording(Command::FileOpen);
        assert_eq!(state.record(chord("escape"), &keymap), RecordResult::Rejected);
        assert!(state.recording.is_some());
        assert_eq!(state.hint, Some("зарезервировано"));
    }

    #[test]
    fn recording_requires_confirmation_for_existing_owners() {
        let keymap = Keymap::build(&KeymapOverrides::default());
        let mut state = KeymapSettingsState::default();
        state.begin_recording(Command::FileOpen);
        assert_eq!(state.record(chord("mod+s"), &keymap), RecordResult::Conflict);
        assert_eq!(state.pending_conflict.as_ref().map(|conflict| conflict.owners.as_slice()), Some(&[Command::FileSave][..]));
        assert_eq!(state.confirm_conflict(), Some((Command::FileOpen, chord("mod+s"), vec![Command::FileSave])));
    }

    #[test]
    fn recording_adds_unowned_chord_and_warns_for_terminal_global_chords() {
        let keymap = Keymap::build(&KeymapOverrides::default());
        let mut state = KeymapSettingsState::default();
        state.begin_recording(Command::GitRefresh);
        assert_eq!(state.record(chord("ctrl+e"), &keymap), RecordResult::Add { command: Command::GitRefresh, chord: chord("ctrl+e") });
        assert_eq!(state.hint, Some("В фокусе терминала сочетание перехватывает терминал"));
    }

    #[test]
    fn terminal_context_chords_warn_when_pty_intercepts_them() {
        let mut overrides = KeymapOverrides::default();
        overrides.add_chord(crate::platform::PlatformKind::Linux, Command::TerminalCloseTab, chord("ctrl+c"));
        let keymap = Keymap::build_for(crate::platform::PlatformKind::Linux, &overrides);
        let mut state = KeymapSettingsState::default();
        state.refresh(&keymap, &overrides);
        let row = state.rows.iter().find(|row| row.command == Command::TerminalCloseTab);
        let warning = row.is_some_and(|row| matches!(row.warning, super::RowWarning::TerminalIntercept | super::RowWarning::ConflictAndTerminalIntercept));
        assert!(warning, "TerminalCloseTab terminal warning: {warning}, row_found={}", row.is_some());
    }

    #[test]
    fn filter_matches_save_label_id_and_serialized_chord_text() {
        let keymap = Keymap::build(&KeymapOverrides::default());
        let mut state = KeymapSettingsState::default();
        state.refresh(&keymap, &KeymapOverrides::default());
        state.scroll.current = 420.0;
        state.scroll.target = 420.0;

        state.update_filter("save".into());
        assert_eq!(state.scroll.current, 0.0);
        assert_eq!(state.scroll.target, 0.0);
        let save_row = state.rows.iter().find(|row| row.command == Command::FileSave);
        assert!(save_row.is_some_and(|row| state.row_matches(row)), "FileSave matched save filter");

        state.update_filter("сохранить".into());
        let save_row = state.rows.iter().find(|row| row.command == Command::FileSave);
        assert!(save_row.is_some_and(|row| state.row_matches(row)), "FileSave matched Russian label substring");

        state.update_filter("mod+s".into());
        let save_row = state.rows.iter().find(|row| row.command == Command::FileSave);
        assert!(save_row.is_some_and(|row| state.row_matches(row)), "FileSave matched serialized mod+s search");
    }

    #[test]
    fn modified_escape_is_recorded_as_a_chord() {
        let keymap = Keymap::build(&KeymapOverrides::default());
        let mut state = KeymapSettingsState::default();
        state.begin_recording(Command::GitRefresh);
        let ctrl_escape = chord("ctrl+escape");
        let result = state.handle_recording_key(PhysicalKey::Code(KeyCode::Escape), Some(ctrl_escape), &keymap);
        assert_eq!(result, RecordResult::Add { command: Command::GitRefresh, chord: ctrl_escape }, "Ctrl+Escape recording result and active recording: {result:?}, {:?}", state.recording);
        assert!(state.recording.is_none(), "recording remained active after Ctrl+Escape: {:?}", state.recording);
    }
}
