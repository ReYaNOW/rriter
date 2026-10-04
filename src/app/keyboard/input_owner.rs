use crate::keymap::{Chord, Mods};
use crate::platform::PlatformKind;
use winit::keyboard::KeyCode;

pub(super) fn file_tree_text_input_owns_keyboard_context(panels: &crate::app::IdePanelState) -> bool {
    let hard_modal_open = panels.api.mock_contract_field_delete_dialog.is_some()
        || panels.api.mock_route_reset_dialog.is_some()
        || panels.git.confirm_dialog.is_some()
        || panels.file_tree_context_menu.is_some()
        || panels.file_tree_move_dialog.is_some()
        || panels.file_tree_delete_dialog.is_some();
    !hard_modal_open
        && (panels.file_tree_rename_dialog.is_some() || panels.file_tree_create_dialog.is_some())
}

pub(super) fn terminal_keyboard_owner(
    is_ide_mode: bool,
    show_settings: bool,
    show_search: bool,
    search_focused: bool,
    panels: &crate::app::IdePanelState,
) -> bool {
    if !is_ide_mode || !panels.is_open(crate::app::PanelId::Terminal) {
        return false;
    }

    let higher_priority_non_terminal_owner = show_settings
        || file_tree_text_input_owns_keyboard_context(panels)
        || (panels.is_open(crate::app::PanelId::Search) && panels.project_search.focused.is_some())
        || (panels.is_open(crate::app::PanelId::LspServers) && panels.lsp_log_filter_focused)
        || (panels.is_open(crate::app::PanelId::Git) && panels.git.message_focused)
        || (panels.is_open(crate::app::PanelId::ApiClient) && panels.api.focused.is_some())
        || (panels.is_open(crate::app::PanelId::LspServers) && panels.lsp_logs_focused.is_some());
    if higher_priority_non_terminal_owner {
        return false;
    }
    if panels.term_show_search && panels.term_search_focused {
        return true;
    }
    if show_search && search_focused {
        return false;
    }
    panels.terminal_focused
}

pub(crate) fn is_default_chord(command: crate::keymap::Command, chord: crate::keymap::Chord, platform: crate::platform::PlatformKind) -> bool {
    crate::keymap::COMMANDS
        .get(command as usize)
        .is_some_and(|info| info.defaults.iter().filter_map(|text| crate::keymap::Chord::parse(platform, text).ok()).any(|default| default == chord))
}

pub(crate) fn terminal_intercepts(chord: Chord, platform: PlatformKind) -> bool {
    let ctrl_byte = chord.mods.contains(Mods::CTRL)
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
                | KeyCode::Space
                | KeyCode::Digit2
                | KeyCode::Digit6
                | KeyCode::Minus
                | KeyCode::Slash
                | KeyCode::BracketLeft
                | KeyCode::Backslash
                | KeyCode::BracketRight
        );
    let primary = if platform == PlatformKind::Macos {
        Mods::SUPER
    } else {
        Mods::CTRL
    };
    let primary_terminal_shortcut = chord.mods.contains(primary)
        && matches!(chord.key, KeyCode::KeyC | KeyCode::KeyF | KeyCode::KeyV);
    ctrl_byte || primary_terminal_shortcut
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(platform: PlatformKind, text: &str) -> Chord {
        Chord::parse(platform, text).expect("test chord")
    }

    #[test]
    fn terminal_intercepts_control_bytes_and_primary_copy() {
        let linux_ctrl_c = chord(PlatformKind::Linux, "ctrl+c");
        assert!(terminal_intercepts(linux_ctrl_c, PlatformKind::Linux), "PTY consumes Ctrl+C: {linux_ctrl_c:?}");
        let mac_cmd_c = chord(PlatformKind::Macos, "cmd+c");
        assert!(terminal_intercepts(mac_cmd_c, PlatformKind::Macos), "PTY consumes Cmd+C: {mac_cmd_c:?}");
        let mac_ctrl_c = chord(PlatformKind::Macos, "ctrl+c");
        assert!(terminal_intercepts(mac_ctrl_c, PlatformKind::Macos), "PTY consumes Ctrl+C on macOS: {mac_ctrl_c:?}");
        for text in ["ctrl+space", "ctrl+2", "ctrl+6", "ctrl+-", "ctrl+/", "ctrl+[", "ctrl+\\", "ctrl+]"] {
            let key = chord(PlatformKind::Linux, text);
            assert!(terminal_intercepts(key, PlatformKind::Linux), "PTY consumes {text}: {key:?}");
        }
    }

    #[test]
    fn terminal_command_exception_matches_only_its_original_chord() {
        let default = chord(PlatformKind::Linux, "ctrl+shift+f");
        let reassigned = chord(PlatformKind::Linux, "ctrl+p");
        assert!(terminal_intercepts(default, PlatformKind::Linux), "PTY consumes default Ctrl+Shift+F: {default:?}");
        assert!(terminal_intercepts(reassigned, PlatformKind::Linux), "PTY consumes reassigned Ctrl+P as a control byte: {reassigned:?}");
        assert!(is_default_chord(crate::keymap::Command::SearchProjectOpen, default, PlatformKind::Linux), "default search chord matches: {default:?}");
        assert!(!is_default_chord(crate::keymap::Command::SearchProjectOpen, reassigned, PlatformKind::Linux), "reassigned chord is not the search default: {reassigned:?}");
    }
}
