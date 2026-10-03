use crate::keymap::{Chord, Mods};
use crate::platform::PlatformKind;
use winit::keyboard::KeyCode;

pub(super) fn terminal_intercepts(chord: Chord, platform: PlatformKind) -> bool {
    let ctrl_letter = chord.mods.contains(Mods::CTRL)
        && matches!(
            chord.key,
            KeyCode::KeyA
                | KeyCode::KeyB
                | KeyCode::KeyC
                | KeyCode::KeyD
                | KeyCode::KeyE
                | KeyCode::KeyF
                | KeyCode::KeyG
                | KeyCode::KeyH
                | KeyCode::KeyI
                | KeyCode::KeyJ
                | KeyCode::KeyK
                | KeyCode::KeyL
                | KeyCode::KeyM
                | KeyCode::KeyN
                | KeyCode::KeyO
                | KeyCode::KeyP
                | KeyCode::KeyQ
                | KeyCode::KeyR
                | KeyCode::KeyS
                | KeyCode::KeyT
                | KeyCode::KeyU
                | KeyCode::KeyV
                | KeyCode::KeyW
                | KeyCode::KeyX
                | KeyCode::KeyY
                | KeyCode::KeyZ
        );
    let primary = if platform == PlatformKind::Macos {
        Mods::SUPER
    } else {
        Mods::CTRL
    };
    let primary_terminal_shortcut = chord.mods.contains(primary)
        && matches!(chord.key, KeyCode::KeyC | KeyCode::KeyF | KeyCode::KeyV);
    ctrl_letter || primary_terminal_shortcut
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(platform: PlatformKind, text: &str) -> Chord {
        Chord::parse(platform, text).expect("test chord")
    }

    #[test]
    fn terminal_intercepts_control_bytes_and_primary_copy() {
        assert!(terminal_intercepts(chord(PlatformKind::Linux, "ctrl+c"), PlatformKind::Linux));
        assert!(terminal_intercepts(chord(PlatformKind::Macos, "cmd+c"), PlatformKind::Macos));
        assert!(terminal_intercepts(chord(PlatformKind::Macos, "ctrl+c"), PlatformKind::Macos));
        assert!(!terminal_intercepts(chord(PlatformKind::Linux, "ctrl+space"), PlatformKind::Linux));
    }
}
