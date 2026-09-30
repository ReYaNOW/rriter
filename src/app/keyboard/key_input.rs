use crate::app::App;
use crate::app::events::host_loop::HostLoop;
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey, SmolStr};

#[derive(Clone, Debug, PartialEq)]
pub struct KeyInput {
    pub physical_key: PhysicalKey,
    pub logical_text: Option<SmolStr>,
    pub text: Option<SmolStr>,
    pub state: ElementState,
    pub repeat: bool,
}

impl From<&KeyEvent> for KeyInput {
    fn from(event: &KeyEvent) -> Self {
        Self {
            physical_key: event.physical_key,
            logical_text: event.logical_key.to_text().map(SmolStr::new),
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
        let lower = key.to_ascii_lowercase();
        let (code, logical) = match lower.as_str() {
            "enter" => (KeyCode::Enter, Some("\r")),
            "tab" => (KeyCode::Tab, Some("\t")),
            "escape" | "esc" => (KeyCode::Escape, Some("\x1b")),
            "backspace" => (KeyCode::Backspace, Some("\x08")),
            "delete" => (KeyCode::Delete, None),
            "space" => (KeyCode::Space, Some(" ")),
            "up" => (KeyCode::ArrowUp, None),
            "down" => (KeyCode::ArrowDown, None),
            "left" => (KeyCode::ArrowLeft, None),
            "right" => (KeyCode::ArrowRight, None),
            "home" => (KeyCode::Home, None),
            "end" => (KeyCode::End, None),
            "pageup" => (KeyCode::PageUp, None),
            "pagedown" => (KeyCode::PageDown, None),
            "insert" => (KeyCode::Insert, None),
            "f1" => (KeyCode::F1, None),
            "f2" => (KeyCode::F2, None),
            "f3" => (KeyCode::F3, None),
            "f4" => (KeyCode::F4, None),
            "f5" => (KeyCode::F5, None),
            "f6" => (KeyCode::F6, None),
            "f7" => (KeyCode::F7, None),
            "f8" => (KeyCode::F8, None),
            "f9" => (KeyCode::F9, None),
            "f10" => (KeyCode::F10, None),
            "f11" => (KeyCode::F11, None),
            "f12" => (KeyCode::F12, None),
            "." => (KeyCode::Period, Some(".")),
            "," => (KeyCode::Comma, Some(",")),
            "/" => (KeyCode::Slash, Some("/")),
            "-" => (KeyCode::Minus, Some("-")),
            "=" => (KeyCode::Equal, Some("=")),
            ";" => (KeyCode::Semicolon, Some(";")),
            "'" => (KeyCode::Quote, Some("'")),
            "[" => (KeyCode::BracketLeft, Some("[")),
            "]" => (KeyCode::BracketRight, Some("]")),
            "\\" => (KeyCode::Backslash, Some("\\")),
            "`" => (KeyCode::Backquote, Some("`")),
            _ => {
                let bytes = lower.as_bytes();
                if bytes.len() == 1 && bytes[0].is_ascii_lowercase() {
                    let code = LETTER_CODES[(bytes[0] - b'a') as usize];
                    let text = if modifiers.contains(ModifiersState::SHIFT) {
                        key.to_ascii_uppercase()
                    } else {
                        lower
                    };
                    return Ok((Self::pressed(code, &text, modifiers), modifiers));
                }
                if bytes.len() == 1 && bytes[0].is_ascii_digit() {
                    let code = DIGIT_CODES[(bytes[0] - b'0') as usize];
                    return Ok((Self::pressed(code, &lower, modifiers), modifiers));
                }
                return Err(format!("unknown key token '{key}'"));
            }
        };
        let logical_text = logical.map(SmolStr::new);
        let text = if modifiers.intersects(ModifiersState::CONTROL | ModifiersState::ALT | ModifiersState::SUPER)
            || matches!(code, KeyCode::Escape | KeyCode::Backspace)
        {
            None
        } else {
            logical_text.clone()
        };
        Ok((Self { physical_key: PhysicalKey::Code(code), logical_text, text, state: ElementState::Pressed, repeat: false }, modifiers))
    }

    fn pressed(code: KeyCode, logical: &str, modifiers: ModifiersState) -> Self {
        Self {
            physical_key: PhysicalKey::Code(code),
            logical_text: Some(SmolStr::new(logical)),
            text: if modifiers.intersects(ModifiersState::CONTROL | ModifiersState::ALT | ModifiersState::SUPER) {
                None
            } else {
                Some(SmolStr::new(logical))
            },
            state: ElementState::Pressed,
            repeat: false,
        }
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

const LETTER_CODES: [KeyCode; 26] = [
    KeyCode::KeyA, KeyCode::KeyB, KeyCode::KeyC, KeyCode::KeyD, KeyCode::KeyE,
    KeyCode::KeyF, KeyCode::KeyG, KeyCode::KeyH, KeyCode::KeyI, KeyCode::KeyJ,
    KeyCode::KeyK, KeyCode::KeyL, KeyCode::KeyM, KeyCode::KeyN, KeyCode::KeyO,
    KeyCode::KeyP, KeyCode::KeyQ, KeyCode::KeyR, KeyCode::KeyS, KeyCode::KeyT,
    KeyCode::KeyU, KeyCode::KeyV, KeyCode::KeyW, KeyCode::KeyX, KeyCode::KeyY,
    KeyCode::KeyZ,
];
const DIGIT_CODES: [KeyCode; 10] = [
    KeyCode::Digit0, KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4,
    KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8, KeyCode::Digit9,
];

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
