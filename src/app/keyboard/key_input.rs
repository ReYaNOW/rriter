use crate::app::App;
use crate::app::events::host_loop::HostLoop;
use crate::keymap::parse_key_token;
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey, SmolStr};
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;

#[derive(Clone, Debug, PartialEq)]
pub struct KeyInput {
    pub physical_key: PhysicalKey,
    pub logical_text: Option<SmolStr>,
    /// First character of the key without modifiers (layout-dependent), used as the
    /// unshifted key by the terminal key encoder.
    pub unshifted: Option<char>,
    pub text: Option<SmolStr>,
    pub state: ElementState,
    pub repeat: bool,
}

impl From<&KeyEvent> for KeyInput {
    fn from(event: &KeyEvent) -> Self {
        Self {
            physical_key: event.physical_key,
            logical_text: event.logical_key.to_text().map(SmolStr::new),
            unshifted: event.key_without_modifiers().to_text().and_then(|text| text.chars().next()),
            text: event.text.clone(),
            state: event.state,
            repeat: event.repeat,
        }
    }
}

impl KeyInput {
    pub fn released(&self) -> Self {
        let mut released = self.clone();
        released.state = ElementState::Released;
        released.repeat = false;
        released
    }

    /// Parses a `+`-separated modifier list (`"ctrl+shift"`); the empty string is no modifiers.
    pub fn parse_modifiers(spec: &str) -> Result<ModifiersState, String> {
        let mut modifiers = ModifiersState::empty();
        for token in spec.split('+').map(str::trim).filter(|token| !token.is_empty()) {
            let Some(modifier) = modifier_token(token) else {
                return Err(format!("unknown modifier '{token}'"));
            };
            if modifiers.contains(modifier) {
                return Err(format!("duplicate modifier '{token}'"));
            }
            modifiers |= modifier;
        }
        Ok(modifiers)
    }

    pub fn parse_combo(combo: &str) -> Result<(Self, ModifiersState), String> {
        if combo.trim().is_empty() {
            return Err("empty key combo".to_string());
        }
        let mut tokens = combo.split('+').map(str::trim).peekable();
        let mut modifiers = ModifiersState::empty();
        let key = loop {
            let token = tokens.next().unwrap_or_default();
            if token.is_empty() {
                return Err("empty key token".to_string());
            }
            if tokens.peek().is_none() {
                break token;
            }
            let Some(modifier) = modifier_token(token) else {
                return Err(format!("unknown key token '{token}'"));
            };
            if modifiers.contains(modifier) {
                return Err(format!("duplicate modifier '{token}'"));
            }
            modifiers |= modifier;
        };
        let (code, logical) = parse_key_token(key)?;
        // Combos have no layout: the plain character is the key's logical text, or the ASCII
        // letter or digit itself, lowercased.
        let plain = logical.and_then(|text| text.chars().next()).or_else(|| {
            (key.len() == 1 && key.as_bytes()[0].is_ascii_alphanumeric()).then(|| key.as_bytes()[0].to_ascii_lowercase() as char)
        });
        let physical_key = PhysicalKey::Code(code);
        let (logical_text, text) = key_texts(physical_key, plain, modifiers);
        Ok((Self { physical_key, logical_text, unshifted: plain, text, state: ElementState::Pressed, repeat: false }, modifiers))
    }

    /// Derives `logical_text` and `text` from `unshifted` for a press under `mods`, the effective
    /// modifiers (held plus combo). Used by the headless `key` command, whose text is not known
    /// until the modifiers it runs under are.
    pub(crate) fn apply_mods(&mut self, mods: ModifiersState) {
        let (logical_text, text) = key_texts(self.physical_key, self.unshifted, mods);
        self.logical_text = logical_text;
        self.text = text;
    }

}

fn modifier_token(token: &str) -> Option<ModifiersState> {
    match token.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Some(ModifiersState::CONTROL),
        "shift" => Some(ModifiersState::SHIFT),
        "alt" => Some(ModifiersState::ALT),
        "super" | "meta" => Some(ModifiersState::SUPER),
        _ => None,
    }
}

/// Logical text and text of a key pressed under `mods`, the effective modifiers (held plus
/// combo). `plain` is the key's character without Shift. Shift upper-cases it when the uppercase
/// form is one character. Ctrl and Super give no text, as Ronsole's `should_emit_xkb_text` does,
/// and Escape and Backspace never give text.
pub(crate) fn key_texts(
    physical_key: PhysicalKey,
    plain: Option<char>,
    mods: ModifiersState,
) -> (Option<SmolStr>, Option<SmolStr>) {
    let logical = plain.map(|c| {
        let shifted = if mods.contains(ModifiersState::SHIFT) {
            let mut upper = c.to_uppercase();
            match (upper.next(), upper.next()) {
                (Some(single), None) => single,
                _ => c,
            }
        } else {
            c
        };
        let mut buf = [0u8; 4];
        SmolStr::new(shifted.encode_utf8(&mut buf))
    });
    let text = if mods.intersects(ModifiersState::CONTROL | ModifiersState::SUPER)
        || matches!(physical_key, PhysicalKey::Code(KeyCode::Escape | KeyCode::Backspace))
    {
        None
    } else {
        logical.clone()
    };
    (logical, text)
}

/// A key combo held down by [`App::press_key_combo`]: the matching release and the modifiers
/// to put back when the combo ends.
pub struct KeyComboHold {
    release: KeyInput,
    saved: ModifiersState,
}

impl App {
    pub fn handle_main_keyboard_input(&mut self, host: &HostLoop, event: KeyEvent) {
        self.handle_main_key_input(host, KeyInput::from(&event));
    }

    /// Shared by the headless `key` command and the automation `Key` step: substitutes the
    /// modifiers with `mods` and delivers the press; the caller renders a frame, then calls
    /// `release_key_combo` and `end_key_combo`.
    pub fn press_key_combo(
        &mut self,
        host: &HostLoop,
        input: KeyInput,
        mods: ModifiersState,
    ) -> KeyComboHold {
        let hold = KeyComboHold { release: input.released(), saved: self.modifiers };
        self.modifiers = mods;
        self.handle_main_key_input(host, input);
        hold
    }

    pub fn release_key_combo(&mut self, host: &HostLoop, hold: &KeyComboHold) {
        self.handle_main_key_input(host, hold.release.clone());
    }

    pub fn end_key_combo(&mut self, hold: KeyComboHold) {
        self.modifiers = hold.saved;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_input_parse_combo_classes() {
        let cases = [
            ("a", KeyCode::KeyA, Some("a")),
            ("shift+a", KeyCode::KeyA, Some("A")),
            ("7", KeyCode::Digit7, Some("7")),
            ("enter", KeyCode::Enter, Some("\r")),
            ("esc", KeyCode::Escape, Some("\x1b")),
            ("f5", KeyCode::F5, None),
            ("pageup", KeyCode::PageUp, None),
            (".", KeyCode::Period, Some(".")),
            ("`", KeyCode::Backquote, Some("`")),
        ];
        for (combo, code, logical) in cases {
            let (input, _) = KeyInput::parse_combo(combo).unwrap();
            assert_eq!(input.physical_key, PhysicalKey::Code(code), "{combo}");
            assert_eq!(input.logical_text.as_deref(), logical, "{combo}");
            if logical.is_none() { assert_eq!(input.text, None, "{combo}"); }
        }
        for combo in ["delete", "up", "insert", "pause", "numpadenter"] {
            let (input, _) = KeyInput::parse_combo(combo).unwrap();
            assert_eq!(input.logical_text, None, "{combo}");
            assert_eq!(input.text, None, "{combo}");
        }
        let (input, modifiers) = KeyInput::parse_combo("ctrl+shift+p").unwrap();
        assert_eq!(modifiers, ModifiersState::CONTROL | ModifiersState::SHIFT);
        assert_eq!(input.physical_key, PhysicalKey::Code(KeyCode::KeyP));
        assert_eq!(input.logical_text.as_deref(), Some("P"));
        assert_eq!(input.text, None);
        let (input, modifiers) = KeyInput::parse_combo("SUPER+Q").unwrap();
        assert_eq!(modifiers, ModifiersState::SUPER);
        assert_eq!(input.physical_key, PhysicalKey::Code(KeyCode::KeyQ));
        assert_eq!(input.text, None);
    }

    #[test]
    fn key_texts_follow_effective_modifiers() {
        // Expected values follow spec §4.2 and Ronsole's `should_emit_xkb_text`: Shift
        // upper-cases, Ctrl and Super give no text, Alt does not block it.
        let key = PhysicalKey::Code(KeyCode::KeyA);
        let cases = [
            (Some('a'), ModifiersState::empty(), Some("a"), Some("a")),
            (Some('a'), ModifiersState::SHIFT, Some("A"), Some("A")),
            (Some('с'), ModifiersState::SHIFT, Some("С"), Some("С")),
            (Some('a'), ModifiersState::CONTROL, Some("a"), None),
            (Some('a'), ModifiersState::SUPER, Some("a"), None),
            (Some('a'), ModifiersState::ALT, Some("a"), Some("a")),
        ];
        for (plain, mods, logical, text) in cases {
            let (got_logical, got_text) = key_texts(key, plain, mods);
            assert_eq!(got_logical.as_deref(), logical, "{plain:?} {mods:?}");
            assert_eq!(got_text.as_deref(), text, "{plain:?} {mods:?}");
        }
        let (logical, text) = key_texts(PhysicalKey::Code(KeyCode::Escape), Some('\x1b'), ModifiersState::empty());
        assert_eq!((logical.as_deref(), text), (Some("\x1b"), None));
        let (input, _) = KeyInput::parse_combo("alt+a").unwrap();
        assert_eq!(input.text.as_deref(), Some("a"));
    }

    #[test]
    fn key_input_parse_combo_rejects_invalid() {
        for combo in ["ctrl+", "+", "ctrl++a"] {
            assert_eq!(KeyInput::parse_combo(combo).unwrap_err(), "empty key token");
        }
        assert_eq!(KeyInput::parse_combo("").unwrap_err(), "empty key combo");
        for combo in ["ctrl+foo", "ctrl+ж"] {
            assert_eq!(KeyInput::parse_combo(combo).unwrap_err(), format!("unknown key token '{}'", combo.split('+').last().unwrap()));
        }
        assert!(KeyInput::parse_combo(&"x".repeat(10_000)).is_err());
        for combo in ["ctrl+ctrl", "ctrl+ctrl+a", "ctrl"] {
            assert!(KeyInput::parse_combo(combo).is_err(), "{combo}");
        }
    }

    #[test]
    fn key_input_released_changes_only_state_and_repeat() {
        let (mut input, _) = KeyInput::parse_combo("shift+a").unwrap();
        input.repeat = true;
        let released = input.released();
        assert_eq!(released.state, ElementState::Released);
        assert!(!released.repeat);
        input.state = ElementState::Released;
        input.repeat = false;
        assert_eq!(released, input);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn key_input_enter_inserts_newline() {
        use crate::app::events::host_loop::HeadlessLoopState;
        use crate::platform::offscreen_gl::test_support::offscreen_test_app;

        let (_context, mut app) = offscreen_test_app(1280, 800, 1.0);
        app.editor = crate::app::reviewer_stage2_editor_with("ab");
        app.editor.cursor = 1;
        app.show_welcome = false;
        let state = HeadlessLoopState::default();
        let (input, _) = KeyInput::parse_combo("enter").unwrap();
        app.handle_main_key_input(&HostLoop::headless(&state), input);
        assert_eq!(app.editor.get_full_text(), "a\nb");
    }
}
