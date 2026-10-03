use crate::app::keyboard::KeyInput;
use crate::platform::{word_modifier_for_platform, CURRENT_PLATFORM, PlatformKind};
use serde_json::Value;
use std::collections::BTreeMap;
use winit::event::ElementState;
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};

mod defaults;
pub use defaults::{COMMANDS, Command};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyContext { Global, Editor, FileTree, Terminal, Pdf, Image, Git, DatabaseQuery, DatabaseTable, ApiClient, Markdown, Settings, Welcome, ProjectSearch }

impl KeyContext {
    pub fn exclusive(a: KeyContext, b: KeyContext) -> bool {
        matches!((a, b), (KeyContext::Pdf, KeyContext::Image) | (KeyContext::Image, KeyContext::Pdf)
            | (KeyContext::Welcome, KeyContext::Editor) | (KeyContext::Editor, KeyContext::Welcome)
            | (KeyContext::Markdown, KeyContext::Pdf) | (KeyContext::Pdf, KeyContext::Markdown)
            | (KeyContext::Markdown, KeyContext::Image) | (KeyContext::Image, KeyContext::Markdown)
            | (KeyContext::Markdown, KeyContext::DatabaseQuery) | (KeyContext::DatabaseQuery, KeyContext::Markdown)
            | (KeyContext::Markdown, KeyContext::DatabaseTable) | (KeyContext::DatabaseTable, KeyContext::Markdown)
            | (KeyContext::DatabaseQuery, KeyContext::DatabaseTable) | (KeyContext::DatabaseTable, KeyContext::DatabaseQuery)
            | (KeyContext::Terminal, KeyContext::Editor) | (KeyContext::Editor, KeyContext::Terminal))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CommandInfo { pub command: Command, pub id: &'static str, pub label: &'static str, pub context: KeyContext, pub defaults: &'static [&'static str] }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Mods(u8);

impl Mods {
    pub const CTRL: Self = Self(1);
    pub const ALT: Self = Self(2);
    pub const SHIFT: Self = Self(4);
    pub const SUPER: Self = Self(8);
    const ALL: u8 = 15;
    pub const fn empty() -> Self { Self(0) }
    pub const fn bits(self) -> u8 { self.0 }
    pub const fn contains(self, other: Self) -> bool { self.0 & other.0 == other.0 }
    pub const fn is_empty(self) -> bool { self.0 == 0 }
    fn insert(&mut self, other: Self) { self.0 |= other.0; }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Chord { pub key: KeyCode, pub mods: Mods }

pub(crate) fn parse_key_token(key: &str) -> Result<(KeyCode, Option<&'static str>), String> {
    let lower = key.to_ascii_lowercase();
    let code = match lower.as_str() {
        "enter" => return Ok((KeyCode::Enter, Some("\r"))),
        "numpadenter" => return Ok((KeyCode::NumpadEnter, None)),
        "tab" => return Ok((KeyCode::Tab, Some("\t"))),
        "escape" | "esc" => return Ok((KeyCode::Escape, Some("\x1b"))),
        "backspace" => return Ok((KeyCode::Backspace, Some("\x08"))),
        "delete" => return Ok((KeyCode::Delete, None)), "space" => return Ok((KeyCode::Space, Some(" "))),
        "up" => return Ok((KeyCode::ArrowUp, None)), "down" => return Ok((KeyCode::ArrowDown, None)),
        "left" => return Ok((KeyCode::ArrowLeft, None)), "right" => return Ok((KeyCode::ArrowRight, None)),
        "home" => return Ok((KeyCode::Home, None)), "end" => return Ok((KeyCode::End, None)),
        "pageup" => return Ok((KeyCode::PageUp, None)), "pagedown" => return Ok((KeyCode::PageDown, None)),
        "insert" => return Ok((KeyCode::Insert, None)), "pause" => return Ok((KeyCode::Pause, None)),
        "f1" => return Ok((KeyCode::F1, None)), "f2" => return Ok((KeyCode::F2, None)),
        "f3" => return Ok((KeyCode::F3, None)), "f4" => return Ok((KeyCode::F4, None)),
        "f5" => return Ok((KeyCode::F5, None)), "f6" => return Ok((KeyCode::F6, None)),
        "f7" => return Ok((KeyCode::F7, None)), "f8" => return Ok((KeyCode::F8, None)),
        "f9" => return Ok((KeyCode::F9, None)), "f10" => return Ok((KeyCode::F10, None)),
        "f11" => return Ok((KeyCode::F11, None)), "f12" => return Ok((KeyCode::F12, None)),
        "." => return Ok((KeyCode::Period, Some("."))), "," => return Ok((KeyCode::Comma, Some(","))),
        "/" => return Ok((KeyCode::Slash, Some("/"))), "-" => return Ok((KeyCode::Minus, Some("-"))),
        "=" => return Ok((KeyCode::Equal, Some("="))), ";" => return Ok((KeyCode::Semicolon, Some(";"))),
        "'" => return Ok((KeyCode::Quote, Some("'"))), "[" => return Ok((KeyCode::BracketLeft, Some("["))),
        "]" => return Ok((KeyCode::BracketRight, Some("]"))), "\\" => return Ok((KeyCode::Backslash, Some("\\"))),
        "`" => return Ok((KeyCode::Backquote, Some("`"))),
        _ => {
            let bytes = lower.as_bytes();
            if bytes.len() == 1 && bytes[0].is_ascii_lowercase() { LETTER_CODES[(bytes[0] - b'a') as usize] }
            else if bytes.len() == 1 && bytes[0].is_ascii_digit() { DIGIT_CODES[(bytes[0] - b'0') as usize] }
            else { return Err(format!("unknown key token '{key}'")); }
        }
    };
    Ok((code, None))
}

const LETTER_CODES: [KeyCode; 26] = [
    KeyCode::KeyA, KeyCode::KeyB, KeyCode::KeyC, KeyCode::KeyD, KeyCode::KeyE, KeyCode::KeyF, KeyCode::KeyG, KeyCode::KeyH, KeyCode::KeyI, KeyCode::KeyJ, KeyCode::KeyK, KeyCode::KeyL, KeyCode::KeyM, KeyCode::KeyN, KeyCode::KeyO, KeyCode::KeyP, KeyCode::KeyQ, KeyCode::KeyR, KeyCode::KeyS, KeyCode::KeyT, KeyCode::KeyU, KeyCode::KeyV, KeyCode::KeyW, KeyCode::KeyX, KeyCode::KeyY, KeyCode::KeyZ,
];
const DIGIT_CODES: [KeyCode; 10] = [KeyCode::Digit0, KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8, KeyCode::Digit9];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChordParseError { Empty, EmptyToken, DuplicateModifier(String), UnknownModifier(String), UnknownKey(String) }

impl Chord {
    pub fn parse(platform: PlatformKind, input: &str) -> Result<Self, ChordParseError> {
        if input.trim().is_empty() { return Err(ChordParseError::Empty); }
        let parts: Vec<_> = input.split('+').map(str::trim).collect();
        let Some((key_token, modifiers)) = parts.split_last() else { return Err(ChordParseError::Empty); };
        if key_token.is_empty() || modifiers.iter().any(|part| part.is_empty()) { return Err(ChordParseError::EmptyToken); }
        let mut mods = Mods::empty();
        for token in modifiers {
            let modifier = match token.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => Mods::CTRL,
                "alt" => Mods::ALT,
                "shift" => Mods::SHIFT,
                "cmd" | "super" | "meta" => Mods::SUPER,
                "mod" => if platform == PlatformKind::Macos { Mods::SUPER } else { Mods::CTRL },
                _ => return Err(ChordParseError::UnknownModifier((*token).to_string())),
            };
            if mods.contains(modifier) { return Err(ChordParseError::DuplicateModifier((*token).to_string())); }
            mods.insert(modifier);
        }
        let (key, _) = parse_key_token(key_token).map_err(|_| ChordParseError::UnknownKey((*key_token).to_string()))?;
        Ok(Self { key, mods })
    }

    pub fn serialize(&self, platform: PlatformKind) -> String {
        let mut parts = Vec::with_capacity(6);
        let primary = if platform == PlatformKind::Macos { Mods::SUPER } else { Mods::CTRL };
        if self.mods.contains(primary) { parts.push("mod".to_string()); }
        for (flag, name) in [(Mods::CTRL, "ctrl"), (Mods::SUPER, "cmd"), (Mods::ALT, "alt"), (Mods::SHIFT, "shift")] {
            if self.mods.contains(flag) && flag != primary { parts.push(name.to_string()); }
        }
        parts.push(key_token(self.key).to_string());
        parts.join("+")
    }

    pub fn display(&self, platform: PlatformKind) -> String {
        let mut out = String::new();
        if platform == PlatformKind::Macos {
            for (flag, glyph) in [(Mods::CTRL, "⌃"), (Mods::ALT, "⌥"), (Mods::SHIFT, "⇧"), (Mods::SUPER, "⌘")] { if self.mods.contains(flag) { out.push_str(glyph); } }
            out.push_str(&display_key(self.key));
        } else {
            let mut labels = Vec::new();
            for (flag, name) in [(Mods::CTRL, "Ctrl"), (Mods::ALT, "Alt"), (Mods::SHIFT, "Shift"), (Mods::SUPER, "Super")] { if self.mods.contains(flag) { labels.push(name.to_string()); } }
            labels.push(display_key(self.key));
            out = labels.join("+");
        }
        out
    }

    pub fn from_event(platform: PlatformKind, event: &KeyInput, modifiers: ModifiersState) -> Option<Self> {
        if event.state != ElementState::Pressed { return None; }
        if platform == PlatformKind::Windows && modifiers.control_key() && modifiers.alt_key()
            && event.text.as_deref().is_some_and(|text| !text.is_empty() && !text.chars().all(char::is_control)) { return None; }
        let PhysicalKey::Code(key) = event.physical_key else { return None; };
        let mut mods = Mods::empty();
        if modifiers.control_key() { mods.insert(Mods::CTRL); }
        if modifiers.alt_key() { mods.insert(Mods::ALT); }
        if modifiers.shift_key() { mods.insert(Mods::SHIFT); }
        if modifiers.super_key() { mods.insert(Mods::SUPER); }
        Some(Self { key, mods })
    }
}

fn key_token(key: KeyCode) -> &'static str {
    match key {
        KeyCode::KeyA => "a", KeyCode::KeyB => "b", KeyCode::KeyC => "c", KeyCode::KeyD => "d", KeyCode::KeyE => "e", KeyCode::KeyF => "f", KeyCode::KeyG => "g", KeyCode::KeyH => "h", KeyCode::KeyI => "i", KeyCode::KeyJ => "j", KeyCode::KeyK => "k", KeyCode::KeyL => "l", KeyCode::KeyM => "m", KeyCode::KeyN => "n", KeyCode::KeyO => "o", KeyCode::KeyP => "p", KeyCode::KeyQ => "q", KeyCode::KeyR => "r", KeyCode::KeyS => "s", KeyCode::KeyT => "t", KeyCode::KeyU => "u", KeyCode::KeyV => "v", KeyCode::KeyW => "w", KeyCode::KeyX => "x", KeyCode::KeyY => "y", KeyCode::KeyZ => "z",
        KeyCode::Digit0 => "0", KeyCode::Digit1 => "1", KeyCode::Digit2 => "2", KeyCode::Digit3 => "3", KeyCode::Digit4 => "4", KeyCode::Digit5 => "5", KeyCode::Digit6 => "6", KeyCode::Digit7 => "7", KeyCode::Digit8 => "8", KeyCode::Digit9 => "9",
        KeyCode::Enter => "enter", KeyCode::NumpadEnter => "numpadenter", KeyCode::Tab => "tab", KeyCode::Escape => "escape", KeyCode::Backspace => "backspace", KeyCode::Delete => "delete", KeyCode::Space => "space", KeyCode::ArrowUp => "up", KeyCode::ArrowDown => "down", KeyCode::ArrowLeft => "left", KeyCode::ArrowRight => "right", KeyCode::Home => "home", KeyCode::End => "end", KeyCode::PageUp => "pageup", KeyCode::PageDown => "pagedown", KeyCode::Insert => "insert", KeyCode::Pause => "pause",
        KeyCode::F1 => "f1", KeyCode::F2 => "f2", KeyCode::F3 => "f3", KeyCode::F4 => "f4", KeyCode::F5 => "f5", KeyCode::F6 => "f6", KeyCode::F7 => "f7", KeyCode::F8 => "f8", KeyCode::F9 => "f9", KeyCode::F10 => "f10", KeyCode::F11 => "f11", KeyCode::F12 => "f12",
        KeyCode::Period => ".", KeyCode::Comma => ",", KeyCode::Slash => "/", KeyCode::Minus => "-", KeyCode::Equal => "=", KeyCode::Semicolon => ";", KeyCode::Quote => "'", KeyCode::BracketLeft => "[", KeyCode::BracketRight => "]", KeyCode::Backslash => "\\", KeyCode::Backquote => "`", _ => "?",
    }
}

fn display_key(key: KeyCode) -> String {
    let token = key_token(key);
    if token.starts_with('f') && token[1..].parse::<u8>().is_ok() { token.to_uppercase() } else if token.len() == 1 { token.to_uppercase() } else { match token { "up" => "↑".into(), "down" => "↓".into(), "left" => "←".into(), "right" => "→".into(), _ => { let mut chars = token.chars(); chars.next().map(|c| c.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default() } } }
}

pub fn validate(platform: PlatformKind, chord: Chord) -> Result<(), ChordError> {
    if key_token(chord.key) == "?" { return Err(ChordError::UnknownKey); }
    if is_reserved(platform, chord) { return Err(ChordError::Reserved); }
    let needs_modifier = match chord.key {
        KeyCode::KeyA | KeyCode::KeyB | KeyCode::KeyC | KeyCode::KeyD | KeyCode::KeyE | KeyCode::KeyF | KeyCode::KeyG | KeyCode::KeyH | KeyCode::KeyI | KeyCode::KeyJ | KeyCode::KeyK | KeyCode::KeyL | KeyCode::KeyM | KeyCode::KeyN | KeyCode::KeyO | KeyCode::KeyP | KeyCode::KeyQ | KeyCode::KeyR | KeyCode::KeyS | KeyCode::KeyT | KeyCode::KeyU | KeyCode::KeyV | KeyCode::KeyW | KeyCode::KeyX | KeyCode::KeyY | KeyCode::KeyZ | KeyCode::Digit0 | KeyCode::Digit1 | KeyCode::Digit2 | KeyCode::Digit3 | KeyCode::Digit4 | KeyCode::Digit5 | KeyCode::Digit6 | KeyCode::Digit7 | KeyCode::Digit8 | KeyCode::Digit9 | KeyCode::Period | KeyCode::Comma | KeyCode::Slash | KeyCode::Minus | KeyCode::Equal | KeyCode::Semicolon | KeyCode::Quote | KeyCode::BracketLeft | KeyCode::BracketRight | KeyCode::Backslash | KeyCode::Backquote => true,
        _ => false,
    };
    let unmodified_only_allowed = matches!(chord.key, KeyCode::F1 | KeyCode::F2 | KeyCode::F3 | KeyCode::F4 | KeyCode::F5 | KeyCode::F6 | KeyCode::F7 | KeyCode::F8 | KeyCode::F9 | KeyCode::F10 | KeyCode::F11 | KeyCode::F12 | KeyCode::Insert | KeyCode::Pause);
    if chord.mods.is_empty() && !unmodified_only_allowed { return Err(ChordError::NeedsModifier); }
    if needs_modifier && chord.mods.bits() & !Mods::SHIFT.0 == 0 { return Err(ChordError::NeedsModifier); }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChordError { UnknownKey, Reserved, NeedsModifier }

pub fn is_reserved(platform: PlatformKind, chord: Chord) -> bool {
    let k = chord.key;
    let m = chord.mods;
    let shift = m == Mods::SHIFT;
    let none = m.is_empty();
    let word_modifier = if platform == PlatformKind::Macos { Mods::ALT } else { Mods::CTRL };
    let word = word_modifier_for_platform(platform, m == Mods::CTRL, m == Mods::ALT);
    let primary = if platform == PlatformKind::Macos { Mods::SUPER } else { Mods::CTRL };
    let modkey = m == primary;
    match k {
        KeyCode::ArrowLeft | KeyCode::ArrowRight => none || shift || word || m == Mods(word_modifier.0 | Mods::SHIFT.0),
        KeyCode::ArrowUp | KeyCode::ArrowDown | KeyCode::PageUp | KeyCode::PageDown | KeyCode::Tab | KeyCode::Space => none || shift,
        KeyCode::Home | KeyCode::End => none || shift || modkey || m == Mods(primary.0 | Mods::SHIFT.0),
        KeyCode::Backspace | KeyCode::Delete => none || word,
        KeyCode::Enter | KeyCode::NumpadEnter => none || shift,
        KeyCode::Escape => none,
        _ => false,
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeymapOverrides { raw: BTreeMap<String, Value> }

impl KeymapOverrides {
    pub fn from_value(value: Value) -> Self { Self { raw: value.as_object().map(|obj| obj.iter().map(|(k,v)| (k.clone(),v.clone())).collect()).unwrap_or_default() } }
    pub fn to_value(&self) -> Value { Value::Object(self.raw.clone().into_iter().collect()) }
    pub fn has(&self, command: Command) -> bool { self.raw.contains_key(command_info(command).id) }
    pub fn reset_command(&mut self, command: Command) { self.raw.remove(command_info(command).id); }
    pub fn reset_all(&mut self) { for info in COMMANDS { self.raw.remove(info.id); } }
    pub fn add_chord(&mut self, platform: PlatformKind, command: Command, chord: Chord) {
        let id = command_info(command).id;
        if self.raw.get(id).is_some_and(|value| !value.is_array()) { return; }
        let mut active = effective_chords(platform, command, self.raw.get(id));
        if !active.contains(&chord) { active.push(chord); }
        self.store_active(platform, command, active);
    }
    pub fn remove_chord(&mut self, platform: PlatformKind, command: Command, chord: Chord) {
        let id = command_info(command).id;
        if self.raw.get(id).is_some_and(|value| !value.is_array()) { return; }
        let active = effective_chords(platform, command, self.raw.get(id)).into_iter().filter(|c| *c != chord).collect();
        self.store_active(platform, command, active);
    }
    pub fn reassign(&mut self, platform: PlatformKind, chord: Chord, to: Command, from: &[Command]) { for command in from { if *command != to && !has_default_chord(platform, *command, chord) { self.remove_chord(platform, *command, chord); } } self.add_chord(platform, to, chord); }
    fn store_active(&mut self, platform: PlatformKind, command: Command, active: Vec<Chord>) {
        let info = command_info(command);
        let mut values = Vec::new();
        let mut active_index = 0;
        let mut seen = Vec::new();
        if let Some(entries) = self.raw.get(info.id).and_then(Value::as_array) {
            for entry in entries {
                let parsed = entry.as_str().and_then(|text| Chord::parse(platform, text).ok()).filter(|chord| validate(platform, *chord).is_ok());
                let Some(parsed) = parsed else { values.push(entry.clone()); continue; };
                if seen.contains(&parsed) { continue; }
                seen.push(parsed);
                if let Some(replacement) = active.get(active_index) {
                    values.push(Value::String(replacement.serialize(platform)));
                    active_index += 1;
                }
            }
        }
        values.extend(active[active_index..].iter().map(|c| Value::String(c.serialize(platform))));
        let defaults: Vec<_> = info.defaults.iter().filter_map(|s| Chord::parse(platform, s).ok()).collect();
        let has_skipped = self.raw.get(info.id).and_then(Value::as_array).is_some_and(|entries| entries.len() > seen.len());
        if !has_skipped && same_chords(&active, &defaults) { self.raw.remove(info.id); return; }
        self.raw.insert(info.id.to_string(), Value::Array(values));
    }
}

fn effective_chords(platform: PlatformKind, command: Command, value: Option<&Value>) -> Vec<Chord> {
    let entries: Vec<Chord> = match value.and_then(Value::as_array) {
        Some(entries) => entries.iter().filter_map(|entry| parse_value_chord(platform, entry)).collect(),
        None => command_info(command).defaults.iter().filter_map(|s| Chord::parse(platform, s).ok()).filter(|c| validate(platform, *c).is_ok()).collect(),
    };
    let mut unique = Vec::with_capacity(entries.len());
    for chord in entries {
        if !unique.contains(&chord) { unique.push(chord); }
    }
    unique
}
fn parse_value_chord(platform: PlatformKind, value: &Value) -> Option<Chord> { value.as_str().and_then(|s| Chord::parse(platform, s).ok()).filter(|c| validate(platform, *c).is_ok()) }
fn same_chords(a: &[Chord], b: &[Chord]) -> bool { a.len() == b.len() && a.iter().all(|c| b.contains(c)) }
fn has_default_chord(platform: PlatformKind, command: Command, chord: Chord) -> bool { command_info(command).defaults.iter().filter_map(|text| Chord::parse(platform, text).ok()).any(|default| default == chord) }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkippedEntry { pub id: String, pub value: String, pub reason: &'static str }

pub struct Keymap { platform: PlatformKind, chords: Vec<Vec<Chord>>, labels: Vec<String>, skipped: Vec<SkippedEntry> }

impl Keymap {
    pub fn build(overrides: &KeymapOverrides) -> Self { Self::build_for(CURRENT_PLATFORM, overrides) }
    pub fn build_for(platform: PlatformKind, overrides: &KeymapOverrides) -> Self {
        let mut chords = vec![Vec::new(); COMMANDS.len()];
        let mut skipped = Vec::new();
        for info in COMMANDS {
            let slot = &mut chords[info.command as usize];
            if let Some(value) = overrides.raw.get(info.id) {
                let Some(entries) = value.as_array() else {
                    skipped.push(SkippedEntry { id: info.id.into(), value: value.to_string(), reason: "expected array of strings" });
                    for default in info.defaults { if let Ok(chord) = Chord::parse(platform, default) { slot.push(chord); } }
                    continue;
                };
                for entry in entries {
                    let Some(text) = entry.as_str() else { skipped.push(SkippedEntry { id: info.id.into(), value: entry.to_string(), reason: "expected string" }); continue; };
                    match Chord::parse(platform, text) {
                        Err(_) => skipped.push(SkippedEntry { id: info.id.into(), value: text.into(), reason: "invalid chord" }),
                        Ok(chord) => match validate(platform, chord) {
                            Err(ChordError::Reserved) => skipped.push(SkippedEntry { id: info.id.into(), value: text.into(), reason: "reserved chord" }),
                            Err(ChordError::NeedsModifier) => skipped.push(SkippedEntry { id: info.id.into(), value: text.into(), reason: "modifier required" }),
                            Err(ChordError::UnknownKey) => skipped.push(SkippedEntry { id: info.id.into(), value: text.into(), reason: "unknown key" }),
                            Ok(()) if slot.contains(&chord) => skipped.push(SkippedEntry { id: info.id.into(), value: text.into(), reason: "duplicate chord" }),
                            Ok(()) => slot.push(chord),
                        },
                    }
                }
            } else {
                for default in info.defaults { if let Ok(chord) = Chord::parse(platform, default) { slot.push(chord); } }
            }
        }
        for (id, value) in &overrides.raw { if !COMMANDS.iter().any(|info| info.id == id) { skipped.push(SkippedEntry { id: id.clone(), value: value.to_string(), reason: "unknown command" }); } }
        let labels = chords.iter().map(|list| list.first().map(|c| c.display(platform)).unwrap_or_else(|| "не назначено".to_string())).collect();
        Self { platform, chords, labels, skipped }
    }
    pub fn hit(&self, command: Command, chord: Chord) -> bool { self.chords[command as usize].contains(&chord) }
    pub fn chords(&self, command: Command) -> &[Chord] { &self.chords[command as usize] }
    pub fn label(&self, command: Command) -> &str { &self.labels[command as usize] }
    pub fn skipped(&self) -> &[SkippedEntry] { &self.skipped }
    pub fn conflicted(&self, command: Command) -> bool { let Some(info) = COMMANDS.get(command as usize) else { return false; }; self.chords(command).iter().any(|ch| COMMANDS.iter().any(|other| other.command != command && !KeyContext::exclusive(info.context, other.context) && self.chords(other.command).contains(ch) && !(has_default_chord(self.platform, command, *ch) && has_default_chord(self.platform, other.command, *ch)))) }
    pub fn owners_of(&self, chord: Chord, context: KeyContext) -> Vec<Command> { COMMANDS.iter().filter(|info| !KeyContext::exclusive(context, info.context) && self.chords(info.command).contains(&chord) && COMMANDS.iter().any(|other| other.command != info.command && !KeyContext::exclusive(context, other.context) && self.chords(other.command).contains(&chord) && !(has_default_chord(self.platform, info.command, chord) && has_default_chord(self.platform, other.command, chord)))).map(|info| info.command).collect() }
}

fn command_info(command: Command) -> &'static CommandInfo { &COMMANDS[command as usize] }

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn chord(platform: PlatformKind, text: &str) -> Chord { Chord::parse(platform, text).unwrap() }

    #[test]
    fn command_indexes_and_defaults_are_valid() {
        let mut ids = std::collections::HashSet::new();
        for (index, info) in COMMANDS.iter().enumerate() {
            assert_eq!(info.command as usize, index);
            assert!(ids.insert(info.id));
            for default in info.defaults {
                let parsed = chord(PlatformKind::Linux, default);
                assert_eq!(validate(PlatformKind::Linux, parsed), Ok(()), "{}: {default}", info.id);
            }
        }
        assert_eq!(COMMANDS.len(), Command::DatabaseQueryNextDiagnostic as usize + 1);
    }

    #[test]
    fn chord_parsing_serialization_and_display_are_platform_aware() {
        let linux = chord(PlatformKind::Linux, "shift+mod+f");
        assert_eq!(linux.serialize(PlatformKind::Linux), "mod+shift+f");
        assert_eq!(linux.display(PlatformKind::Linux), "Ctrl+Shift+F");
        let mac = chord(PlatformKind::Macos, "shift+mod+f");
        assert_eq!(mac.serialize(PlatformKind::Macos), "mod+shift+f");
        assert_eq!(mac.display(PlatformKind::Macos), "⇧⌘F");
        assert_eq!(chord(PlatformKind::Macos, "ctrl+alt+shift+cmd+q").display(PlatformKind::Macos), "⌃⌥⇧⌘Q");
        assert_eq!(chord(PlatformKind::Macos, "ctrl+cmd+q").serialize(PlatformKind::Macos), "mod+ctrl+q");
        for input in ["", "ctrl+", "ctrl++a", "ctrl+ctrl+a", "mod+mod+a", "wat+a", "ctrl+unknown"] {
            assert!(Chord::parse(PlatformKind::Linux, input).is_err(), "{input}");
        }
        assert_eq!(chord(PlatformKind::Linux, "numpadenter").key, KeyCode::NumpadEnter);
        assert_eq!(chord(PlatformKind::Linux, "pause").key, KeyCode::Pause);
        assert_eq!(chord(PlatformKind::Linux, "cmd+s"), Chord { key: KeyCode::KeyS, mods: Mods::SUPER });
        let cmd_save = Keymap::build_for(PlatformKind::Linux, &KeymapOverrides::from_value(json!({"file.save": ["cmd+s"]})));
        assert_eq!(cmd_save.label(Command::FileSave), "Super+S");
    }

    #[test]
    fn reserved_and_modifier_validation_match_fixed_keys() {
        for input in ["left", "shift+right", "ctrl+left", "ctrl+shift+left", "up", "shift+down", "home", "mod+shift+end", "ctrl+backspace", "delete", "enter", "shift+numpadenter", "tab", "space", "escape"] {
            assert_eq!(validate(PlatformKind::Linux, chord(PlatformKind::Linux, input)), Err(ChordError::Reserved), "{input}");
        }
        for input in ["ctrl+escape", "alt+space", "ctrl+enter", "alt+left", "alt+backspace", "super+home"] { assert_eq!(validate(PlatformKind::Linux, chord(PlatformKind::Linux, input)), Ok(()), "{input}"); }
        assert_eq!(validate(PlatformKind::Macos, chord(PlatformKind::Macos, "alt+left")), Err(ChordError::Reserved));
        for input in ["a", "shift+a", "4", "."] { assert_eq!(validate(PlatformKind::Linux, chord(PlatformKind::Linux, input)), Err(ChordError::NeedsModifier), "{input}"); }
        for input in ["f5", "insert", "pause"] { assert_eq!(validate(PlatformKind::Linux, chord(PlatformKind::Linux, input)), Ok(()), "{input}"); }
        assert_eq!(validate(PlatformKind::Linux, Chord { key: KeyCode::F13, mods: Mods::CTRL }), Err(ChordError::UnknownKey));
        assert_eq!(Keymap::build_for(PlatformKind::Linux, &KeymapOverrides::from_value(json!({"file.save": ["mod+numpad5"]}))).skipped()[0].reason, "invalid chord");
    }

    #[test]
    fn windows_altgr_event_is_not_an_application_chord() {
        let mut event = KeyInput { physical_key: PhysicalKey::Code(KeyCode::KeyQ), logical_text: None, text: Some("@".into()), state: ElementState::Pressed, repeat: false };
        let altgr = ModifiersState::CONTROL | ModifiersState::ALT;
        assert_eq!(Chord::from_event(PlatformKind::Windows, &event, altgr), None);
        event.text = None;
        assert_eq!(Chord::from_event(PlatformKind::Windows, &event, altgr), Some(chord(PlatformKind::Windows, "ctrl+alt+q")));
        assert_eq!(Chord::from_event(PlatformKind::Linux, &event, altgr), Some(chord(PlatformKind::Linux, "ctrl+alt+q")));
        event.state = ElementState::Released;
        assert_eq!(Chord::from_event(PlatformKind::Linux, &event, altgr), None);
        event.state = ElementState::Pressed;
        event.physical_key = PhysicalKey::Unidentified(winit::keyboard::NativeKeyCode::Unidentified);
        assert_eq!(Chord::from_event(PlatformKind::Linux, &event, altgr), None);
    }

    #[test]
    fn overrides_skip_bad_entries_preserve_unknown_values_and_reset_known_ids() {
        let overrides = KeymapOverrides::from_value(json!({
            "file.save": ["ctrl+unknown", 7, "ctrl+escape", "ctrl+escape"],
            "editor.undo": "wrong type",
            "removed.command": {"keep": true},
            "edit.copy": []
        }));
        let map = Keymap::build_for(PlatformKind::Linux, &overrides);
        assert_eq!(map.chords(Command::FileSave), &[chord(PlatformKind::Linux, "ctrl+escape")]);
        assert_eq!(map.chords(Command::EditorUndo), &[chord(PlatformKind::Linux, "mod+z")]);
        assert!(map.chords(Command::EditCopy).is_empty());
        assert_eq!(map.skipped().len(), 5);
        assert_eq!(KeymapOverrides::from_value(overrides.to_value()).to_value(), overrides.to_value());
        let mut reset = overrides.clone();
        reset.reset_command(Command::FileSave);
        reset.reset_all();
        assert!(reset.to_value().get("removed.command").is_some());
        assert_eq!(reset.to_value().as_object().map(serde_json::Map::len), Some(1));
    }

    #[test]
    fn edits_preserve_invalid_values_and_remove_default_equivalent_overrides() {
        let mut overrides = KeymapOverrides::from_value(json!({"file.save": ["invalid", "mod+s"]}));
        overrides.remove_chord(PlatformKind::Linux, Command::FileSave, chord(PlatformKind::Linux, "mod+s"));
        assert_eq!(overrides.to_value()["file.save"], json!(["invalid"]));
        overrides.reset_command(Command::FileSave);
        overrides.add_chord(PlatformKind::Linux, Command::FileSave, chord(PlatformKind::Linux, "ctrl+s"));
        assert_eq!(overrides.to_value().get("file.save"), None);
        overrides.remove_chord(PlatformKind::Linux, Command::EditorRedo, chord(PlatformKind::Linux, "mod+y"));
        assert_eq!(effective_chords(PlatformKind::Linux, Command::EditorRedo, overrides.raw.get("editor.redo")), vec![chord(PlatformKind::Linux, "mod+shift+z")]);
        overrides = KeymapOverrides::from_value(json!({"editor.undo": ["bad", "mod+z", "ctrl+escape"]}));
        overrides.reassign(PlatformKind::Linux, chord(PlatformKind::Linux, "ctrl+escape"), Command::FileSave, &[Command::EditorUndo]);
        assert_eq!(overrides.to_value()["editor.undo"], json!(["bad", "mod+z"]));
        assert_eq!(effective_chords(PlatformKind::Linux, Command::FileSave, overrides.raw.get("file.save")), vec![chord(PlatformKind::Linux, "mod+s"), chord(PlatformKind::Linux, "ctrl+escape")]);
        let map = Keymap::build_for(PlatformKind::Linux, &overrides);
        assert!(map.hit(Command::FileSave, chord(PlatformKind::Linux, "ctrl+escape")));
        assert!(!map.hit(Command::FileSave, chord(PlatformKind::Linux, "ctrl+shift+escape")));
        let mut with_duplicate = KeymapOverrides::from_value(json!({"file.save": ["invalid", "ctrl+escape", "ctrl+escape"]}));
        with_duplicate.add_chord(PlatformKind::Linux, Command::FileSave, chord(PlatformKind::Linux, "ctrl+shift+escape"));
        assert_eq!(effective_chords(PlatformKind::Linux, Command::FileSave, with_duplicate.raw.get("file.save")), vec![chord(PlatformKind::Linux, "ctrl+escape"), chord(PlatformKind::Linux, "ctrl+shift+escape")]);
        with_duplicate.remove_chord(PlatformKind::Linux, Command::FileSave, chord(PlatformKind::Linux, "ctrl+escape"));
        assert_eq!(with_duplicate.to_value()["file.save"], json!(["invalid", "mod+shift+escape"]));
    }

    #[test]
    fn owners_respect_only_documented_exclusive_contexts() {
        let map = Keymap::build_for(PlatformKind::Linux, &KeymapOverrides::default());
        let copy = chord(PlatformKind::Linux, "mod+c");
        let owners = map.owners_of(copy, KeyContext::Editor);
        assert!(owners.is_empty());
        assert!(!KeyContext::exclusive(KeyContext::DatabaseTable, KeyContext::Editor));
        assert!(KeyContext::exclusive(KeyContext::Welcome, KeyContext::Editor));
    }

    #[test]
    fn default_chords_have_no_conflicts() {
        let map = Keymap::build_for(PlatformKind::Linux, &KeymapOverrides::default());
        for info in COMMANDS { assert!(!map.conflicted(info.command), "{} has a default conflict", info.id); }
    }

    #[test]
    fn user_chord_conflicts_with_default_in_overlapping_context() {
        let mut overrides = KeymapOverrides::default();
        overrides.add_chord(PlatformKind::Linux, Command::LspFixAll, chord(PlatformKind::Linux, "mod+c"));
        let map = Keymap::build_for(PlatformKind::Linux, &overrides);
        assert!(map.conflicted(Command::LspFixAll));
        assert!(map.owners_of(chord(PlatformKind::Linux, "mod+c"), KeyContext::Editor).contains(&Command::EditCopy));
        overrides.reassign(PlatformKind::Linux, chord(PlatformKind::Linux, "mod+c"), Command::LspFixAll, &[Command::EditCopy]);
        assert!(overrides.to_value().get("edit.copy").is_none());
        assert_eq!(effective_chords(PlatformKind::Linux, Command::EditCopy, overrides.raw.get("edit.copy")), vec![chord(PlatformKind::Linux, "mod+c")]);
    }

    #[test]
    fn every_existing_command_has_a_dispatcher_hit() {
        let dispatchers = [
            include_str!("app/keyboard/main_keys.rs"),
            include_str!("app/keyboard/editor_keys.rs"),
            include_str!("app/keyboard.rs"),
            include_str!("app/pdf_tab/input.rs"),
            include_str!("app/image_tab.rs"),
            include_str!("app/file_tree_dialog.rs"),
            include_str!("app/database/database_table_edit_methods.rs"),
            include_str!("app/database/database_table_key_methods.rs"),
            include_str!("app/api_client/api_client_app_request_methods.rs"),
        ];
        for info in COMMANDS.iter().filter(|info| !info.defaults.is_empty()) {
            let hit = format!("hit(crate::keymap::Command::{:?}", info.command);
            assert!(dispatchers.iter().any(|source| source.contains(&hit)), "{} has no dispatcher hit", info.id);
        }
    }
}
