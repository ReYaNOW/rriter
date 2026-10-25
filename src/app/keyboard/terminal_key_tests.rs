// Terminal key routing tests: host key events go through the main key router, and the bytes
// the terminal would write to its PTY are read back from `written_input`.
// Expected bytes follow the kitty keyboard protocol (CSI u, flags 1|2|8 = 11, modifiers 1 + bitmask).

use super::*;
use winit::keyboard::SmolStr;

/// Primary shortcut modifier of the current platform (Ctrl, or Cmd on macOS).
fn primary_modifiers() -> ModifiersState {
    if crate::platform::CURRENT_PLATFORM == crate::platform::PlatformKind::Macos {
        ModifiersState::SUPER
    } else {
        ModifiersState::CONTROL
    }
}

/// App with an open, focused IDE terminal that has no PTY process, under kitty `flags`.
fn terminal_app(flags: u8) -> App {
    let Some(mut app) = crate::app::app_behavior_tests::test_app() else {
        panic!("test app must initialize");
    };
    app.is_ide_mode = true;
    app.show_welcome = false;
    app.ide_panel.open(crate::app::PanelId::Terminal);
    app.ide_panel.terminal_focused = true;
    let terminal = crate::app::terminal::Terminal::test_without_process(80, 24);
    crate::app::terminal::advance_test_output(
        &mut crate::app::terminal::lock_terminal_grid(&terminal.grid),
        format!("\x1b[>{flags}u").as_bytes(),
    );
    app.ide_panel.terminals.push(terminal);
    app
}

fn key(code: KeyCode, unshifted: Option<char>, text: Option<&str>, state: ElementState, repeat: bool) -> KeyInput {
    KeyInput {
        physical_key: PhysicalKey::Code(code),
        logical_text: text.map(SmolStr::new),
        unshifted,
        text: None,
        state,
        repeat,
    }
}

fn send(app: &mut App, input: KeyInput, modifiers: ModifiersState) {
    app.modifiers = modifiers;
    let state = crate::app::events::host_loop::HeadlessLoopState::default();
    app.handle_main_key_input(&crate::app::events::host_loop::HostLoop::headless(&state), input);
}

/// Bytes the active terminal has received since the last call.
fn written(app: &App) -> Vec<u8> {
    std::mem::take(&mut *crate::platform::lock_recover(&app.ide_panel.terminals[0].written_input))
}

fn grid_with_flags(flags: u8) -> ronsole_term::Term {
    let mut term = crate::app::terminal::test_term(80, 24);
    crate::app::terminal::advance_test_output(&mut term, format!("\x1b[>{flags}u").as_bytes());
    term
}

#[test]
fn release_and_repeat_reach_encoder_flag11() {
    let mut app = terminal_app(11);
    send(&mut app, key(KeyCode::KeyA, Some('a'), Some("a"), ElementState::Pressed, false), ModifiersState::empty());
    assert_eq!(written(&app), b"\x1b[97u");
    send(&mut app, key(KeyCode::KeyA, Some('a'), Some("a"), ElementState::Pressed, true), ModifiersState::empty());
    assert_eq!(written(&app), b"\x1b[97;1:2u");
    send(&mut app, key(KeyCode::KeyA, Some('a'), Some("a"), ElementState::Released, false), ModifiersState::empty());
    assert_eq!(written(&app), b"\x1b[97;1:3u");
}

#[test]
fn ctrl_c_with_selection_copies_and_release_not_sent() {
    let mut app = terminal_app(11);
    {
        let terminal = &app.ide_panel.terminals[0];
        let mut grid = crate::app::terminal::lock_terminal_grid(&terminal.grid);
        crate::app::terminal::advance_test_output(&mut grid, b"hello");
        grid.set_selection(Some((0, 0, 4, 0)));
    }
    let copy = key(KeyCode::KeyC, Some('c'), Some("c"), ElementState::Pressed, false);
    send(&mut app, copy, primary_modifiers());
    assert!(written(&app).is_empty(), "the host copy must not reach the PTY");
    assert_eq!(crate::app::terminal::lock_terminal_grid(&app.ide_panel.terminals[0].grid).selection(), None);

    send(&mut app, key(KeyCode::KeyC, Some('c'), Some("c"), ElementState::Pressed, true), primary_modifiers());
    send(&mut app, key(KeyCode::KeyC, Some('c'), Some("c"), ElementState::Released, false), primary_modifiers());
    assert!(written(&app).is_empty(), "repeat and release of a consumed Ctrl+C must not reach the PTY");

    // A new Press without a selection goes to the encoder again.
    send(&mut app, key(KeyCode::KeyC, Some('c'), Some("c"), ElementState::Pressed, false), ModifiersState::CONTROL);
    assert_eq!(written(&app), b"\x1b[99;5u");
}

#[test]
fn ime_commit_is_text_under_kitty_flags() {
    let mut app = terminal_app(31);
    app.handle_main_ime_commit("привет");
    assert_eq!(written(&app), "привет".as_bytes());
}

#[test]
fn key_without_identity_sends_nothing() {
    for flags in [0_u8, 8] {
        let mut term = grid_with_flags(flags);
        let input = key(KeyCode::F13, None, None, ElementState::Pressed, false);
        assert_eq!(
            terminal_key_bytes(crate::platform::CURRENT_PLATFORM, &mut term, &input, ModifiersState::ALT),
            None,
            "flags={flags}"
        );
    }
}

#[test]
fn windows_ctrl_alt_without_text_strips_ctrl_alt() {
    let mut term = grid_with_flags(8);
    let input = key(KeyCode::KeyA, Some('a'), None, ElementState::Pressed, false);
    let altgr = ModifiersState::CONTROL | ModifiersState::ALT;
    assert_eq!(
        terminal_key_bytes(crate::platform::PlatformKind::Windows, &mut term, &input, altgr),
        Some(b"\x1b[97u".to_vec())
    );
    assert_eq!(
        terminal_key_bytes(crate::platform::PlatformKind::Linux, &mut term, &input, altgr),
        Some(b"\x1b[97;7u".to_vec())
    );
}

#[test]
fn parsed_combo_carries_unshifted_for_encoder() {
    let mut term = grid_with_flags(8);
    let (input, modifiers) = KeyInput::parse_combo("a").unwrap();
    assert_eq!(
        terminal_key_bytes(crate::platform::CURRENT_PLATFORM, &mut term, &input, modifiers),
        Some(b"\x1b[97u".to_vec())
    );
    let (input, modifiers) = KeyInput::parse_combo("shift+a").unwrap();
    assert_eq!(
        terminal_key_bytes(crate::platform::CURRENT_PLATFORM, &mut term, &input, modifiers),
        Some(b"\x1b[97;2u".to_vec())
    );
}

// Expected bytes: kitty codes LeftShift 57441 and RightShift 57447 (Ronsole
// docs/superpowers/specs/2026-10-21-terminal-protocol-ref.md:108,112); press sets its own
// shift bit and release clears it, flags 8|2 (Ronsole plans doc :320, input.rs:2072).

#[test]
fn left_shift_press_and_release_ignore_modifiers_event_order() {
    // KeyboardInput before ModifiersChanged: the host state still lacks shift on press
    // and still reports it on release.
    let mut app = terminal_app(10);
    send(&mut app, key(KeyCode::ShiftLeft, None, None, ElementState::Pressed, false), ModifiersState::empty());
    assert_eq!(written(&app), b"\x1b[57441;2u");
    send(&mut app, key(KeyCode::ShiftLeft, None, None, ElementState::Released, false), ModifiersState::SHIFT);
    assert_eq!(written(&app), b"\x1b[57441;1:3u");

    // ModifiersChanged before KeyboardInput: the host state already has shift on press
    // and has dropped it on release.
    send(&mut app, key(KeyCode::ShiftLeft, None, None, ElementState::Pressed, false), ModifiersState::SHIFT);
    assert_eq!(written(&app), b"\x1b[57441;2u");
    send(&mut app, key(KeyCode::ShiftLeft, None, None, ElementState::Released, false), ModifiersState::empty());
    assert_eq!(written(&app), b"\x1b[57441;1:3u");
}

#[test]
fn held_right_shift_keeps_shift_bit_after_left_shift_release() {
    let mut app = terminal_app(10);
    send(&mut app, key(KeyCode::ShiftRight, None, None, ElementState::Pressed, false), ModifiersState::SHIFT);
    assert_eq!(written(&app), b"\x1b[57447;2u");
    send(&mut app, key(KeyCode::ShiftLeft, None, None, ElementState::Pressed, false), ModifiersState::SHIFT);
    assert_eq!(written(&app), b"\x1b[57441;2u");
    send(&mut app, key(KeyCode::ShiftLeft, None, None, ElementState::Released, false), ModifiersState::SHIFT);
    assert_eq!(written(&app), b"\x1b[57441;2:3u");
}

#[test]
fn empty_modifiers_event_clears_held_modifier_keys() {
    let mut app = terminal_app(10);
    send(&mut app, key(KeyCode::ShiftRight, None, None, ElementState::Pressed, false), ModifiersState::SHIFT);
    assert_eq!(written(&app), b"\x1b[57447;2u");
    // The release of RightShift never arrives; the host then reports no modifiers at all.
    assert_eq!(app.ide_panel.terminal_held_modifier_keys, 0b0001_0000);
    // A release is only encoded after its press was sent, so LeftShift is pressed first.
    send(&mut app, key(KeyCode::ShiftLeft, None, None, ElementState::Pressed, false), ModifiersState::SHIFT);
    assert_eq!(written(&app), b"\x1b[57441;2u");
    app.modifiers = ModifiersState::empty();
    app.sync_terminal_held_modifiers();
    assert_eq!(app.ide_panel.terminal_held_modifier_keys, 0);
    // With the stale RightShift bit cleared, releasing LeftShift no longer reports shift.
    send(&mut app, key(KeyCode::ShiftLeft, None, None, ElementState::Released, false), ModifiersState::empty());
    assert_eq!(written(&app), b"\x1b[57441;1:3u");
}

#[test]
fn clear_terminal_held_keys_resets_host_key_and_modifier_mask() {
    let mut app = terminal_app(10);
    send(&mut app, key(KeyCode::ShiftRight, None, None, ElementState::Pressed, false), ModifiersState::SHIFT);
    app.ide_panel.terminal_host_held_key = Some(PhysicalKey::Code(KeyCode::KeyC));
    assert_eq!(app.ide_panel.terminal_held_modifier_keys, 0b0001_0000);
    app.clear_terminal_held_keys();
    assert_eq!(app.ide_panel.terminal_host_held_key, None);
    assert_eq!(app.ide_panel.terminal_held_modifier_keys, 0);
}

#[test]
fn clear_terminal_held_keys_on_empty_state_is_noop() {
    let mut app = terminal_app(10);
    app.clear_terminal_held_keys();
    app.clear_terminal_held_keys();
    assert_eq!(app.ide_panel.terminal_host_held_key, None);
    assert_eq!(app.ide_panel.terminal_held_modifier_keys, 0);
}

#[test]
fn closing_active_terminal_tab_clears_held_terminal_keys() {
    let mut app = terminal_app(10);
    app.ide_panel
        .terminals
        .push(crate::app::terminal::Terminal::test_without_process(80, 24));
    app.ide_panel.active_terminal = 1;
    app.ide_panel.terminal_held_modifier_keys = 0b0001_0000;
    app.ide_panel.terminal_host_held_key = Some(PhysicalKey::Code(KeyCode::KeyC));
    app.close_terminal_tab_at(0);
    assert_eq!(app.ide_panel.terminal_host_held_key, None);
    assert_eq!(app.ide_panel.terminal_held_modifier_keys, 0);
}

#[test]
fn focus_loss_and_terminal_tab_changes_call_clear_terminal_held_keys() {
    // Window focus and the tab handlers need an ActiveEventLoop or a live tab set, so the call
    // sites are checked in source, as the other event-handler tests in this crate do.
    let events = include_str!("../events.rs");
    assert!(events.contains("if !focused {\n                    self.clear_terminal_held_keys();\n                }"));
    let tabs = include_str!("../app_ide_tab_methods.rs");
    assert!(tabs.contains("self.ide_panel.active_terminal = idx;\n        self.clear_terminal_held_keys();"));
    assert!(tabs.contains("self.reveal_active_terminal_tab_now();\n            self.clear_terminal_held_keys();"));
    assert!(tabs.contains("self.ide_panel.terminals.remove(idx);\n        self.clear_terminal_held_keys();"));
}
