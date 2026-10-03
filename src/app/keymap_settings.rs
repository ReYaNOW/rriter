use crate::keymap::{Chord, Command, KeyContext, KeymapOverrides};
use crate::app::App;
use crate::app::keyboard::KeyInput;
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
    pub has_override: bool,
    pub array_override: bool,
    pub conflicted: bool,
    pub terminal_warning: bool,
    pub search: String,
}

impl Default for KeymapSettingsState {
    fn default() -> Self {
        Self {
            filter: String::new(),
            filter_lower: String::new(),
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
    pub fn refresh(&mut self, keymap: &crate::keymap::Keymap, overrides: &KeymapOverrides) {
        let raw = overrides.to_value();
        self.rows.clear();
        for info in crate::keymap::COMMANDS {
            let chords = keymap.chords(info.command).iter().map(|chord| chord.display(crate::platform::CURRENT_PLATFORM)).collect::<Vec<_>>();
            let mut search = format!("{} {}", info.label, info.id).to_lowercase();
            for chord in &chords { search.push(' '); search.push_str(&chord.to_lowercase()); }
            self.rows.push(KeymapSettingsRow {
                command: info.command,
                context: info.context,
                label: info.label,
                id: info.id,
                chords,
                has_override: overrides.has(info.command),
                array_override: raw.get(info.id).is_none_or(serde_json::Value::is_array),
                conflicted: keymap.conflicted(info.command),
                terminal_warning: info.context == KeyContext::Global && keymap.chords(info.command).iter().any(|chord| crate::app::keyboard::input_owner::terminal_intercepts(*chord, crate::platform::CURRENT_PLATFORM)),
                search,
            });
        }
        self.skipped_label = (!keymap.skipped().is_empty()).then(|| format!("Пропущено {} записей keymap", keymap.skipped().len()));
    }

    pub fn update_filter(&mut self, filter: String) {
        self.filter_lower = filter.to_lowercase();
        self.filter = filter;
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
        if key == PhysicalKey::Code(KeyCode::Escape) {
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
            if key_event.physical_key == PhysicalKey::Code(KeyCode::Escape) { self.keymap_settings.cancel_recording(); }
            if let Some(window) = self.window.as_ref() { window.request_redraw(); }
            return true;
        }
        if self.keymap_settings.filter_focused && self.keymap_settings.recording.is_none() {
            match key_event.physical_key {
                PhysicalKey::Code(KeyCode::Escape) => self.keymap_settings.filter_focused = false,
                PhysicalKey::Code(KeyCode::Backspace) => { let mut filter = self.keymap_settings.filter.clone(); filter.pop(); self.keymap_settings.update_filter(filter); }
                _ if !self.modifiers.control_key() && !self.modifiers.alt_key() && !self.modifiers.super_key() => {
                    if let Some(text) = key_event.text.as_deref() {
                        if !text.chars().any(char::is_control) { self.keymap_settings.filter.push_str(text); self.keymap_settings.filter_lower = self.keymap_settings.filter.to_lowercase(); }
                    }
                }
                _ => {}
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
}
