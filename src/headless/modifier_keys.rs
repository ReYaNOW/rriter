//! Modifier key events of the headless `mods` and `key` commands. The left modifier keys go
//! through the normal key path, as Ronsole's `move_modifiers` sends them.

use crate::app::App;
use crate::app::events::host_loop::HostLoop;
use crate::app::keyboard::KeyInput;
use winit::event::ElementState;
use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};

/// Modifier flags with their left key, in press order; releases run in reverse.
const MODIFIER_KEYS: [(ModifiersState, KeyCode); 4] = [
    (ModifiersState::CONTROL, KeyCode::ControlLeft),
    (ModifiersState::SHIFT, KeyCode::ShiftLeft),
    (ModifiersState::ALT, KeyCode::AltLeft),
    (ModifiersState::SUPER, KeyCode::SuperLeft),
];

/// Turns the held modifiers into `to`: a press event per added modifier in press order, then a
/// release event per removed modifier in reverse order. Each event is followed by the held set
/// it leaves, so the app sees the same intermediate states as with native modifier changes.
pub(crate) fn move_modifiers(app: &mut App, host: &HostLoop, to: ModifiersState) {
    let from = app.modifiers;
    let mut held = from;
    for (flag, code) in MODIFIER_KEYS {
        if to.contains(flag) && !from.contains(flag) {
            app.handle_main_key_input(host, modifier_key_input(code, ElementState::Pressed));
            held |= flag;
            app.handle_main_modifiers_changed(held);
        }
    }
    for (flag, code) in MODIFIER_KEYS.into_iter().rev() {
        if from.contains(flag) && !to.contains(flag) {
            app.handle_main_key_input(host, modifier_key_input(code, ElementState::Released));
            held.remove(flag);
            app.handle_main_modifiers_changed(held);
        }
    }
}

/// Event of a left modifier key as the Wayland path delivers it: the physical key, no text.
fn modifier_key_input(code: KeyCode, state: ElementState) -> KeyInput {
    KeyInput {
        physical_key: PhysicalKey::Code(code),
        logical_text: None,
        unshifted: None,
        text: None,
        state,
        repeat: false,
    }
}
