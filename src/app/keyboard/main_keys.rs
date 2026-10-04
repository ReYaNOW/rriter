use super::*;
use super::key_routes::KeyCtx;

pub(super) fn apply_terminal_alt_q_shortcut(
    panels: &mut crate::app::IdePanelState,
    shift: bool,
    has_terminal: bool,
) -> bool {
    let is_open = panels.is_open(crate::app::PanelId::Terminal);

    if shift {
        if is_open {
            if let Some(slot) = panels
                .slots
                .iter_mut()
                .find(|s| s.id == crate::app::PanelId::Terminal)
            {
                slot.open = false;
            }
            panels.terminal_focused = false;
            panels.enforce_single_open_per_group();
            false
        } else {
            panels.open(crate::app::PanelId::Terminal);
            !has_terminal
        }
    } else if !is_open {
        panels.open(crate::app::PanelId::Terminal);
        !has_terminal
    } else {
        panels.terminal_focused = !panels.terminal_focused;
        if panels.terminal_focused {
            panels.git.message_focused = false;
            panels.term_search_focused = false;
        }
        false
    }
}

pub(super) fn should_suppress_hover_for_keyboard(physical_key: PhysicalKey, ctrl: bool, alt: bool) -> bool {
    let _ = (ctrl, alt);
    matches!(
        physical_key,
        PhysicalKey::Code(
            KeyCode::Escape
                | KeyCode::ArrowLeft
                | KeyCode::ArrowRight
                | KeyCode::ArrowUp
                | KeyCode::ArrowDown
        )
    )
}

pub(super) fn apply_problems_alt_w_shortcut(panels: &mut crate::app::IdePanelState) {
    panels.toggle(crate::app::PanelId::Problems);
}

pub(super) fn is_terminal_tab_close_shortcut(
    panels: &crate::app::IdePanelState,
) -> bool {
    panels.is_open(crate::app::PanelId::Terminal)
        && (panels.terminal_focused || (panels.term_show_search && panels.term_search_focused))
}

pub(super) fn default_file_tree_chord_for_hit(keymap: &crate::keymap::Keymap, chord: crate::keymap::Chord) -> bool {
    let mut has_hit = false;
    for command in [
        crate::keymap::Command::FileTreeUndo,
        crate::keymap::Command::FileTreeRename,
        crate::keymap::Command::FileTreeCopy,
        crate::keymap::Command::FileTreeCut,
        crate::keymap::Command::FileTreePaste,
    ] {
        if keymap.hit(command, chord) {
            has_hit = true;
            if !super::input_owner::is_default_chord(command, chord, crate::platform::CURRENT_PLATFORM) {
                return false;
            }
        }
    }
    has_hit
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum MarkdownGlobalToggleAction {
    ToggleMode,
    Consume,
}

#[path = "main_keys_vcs_copy.rs"]
mod vcs_copy;
use vcs_copy::git_logs_keyboard_copy_eligible as git_logs_keyboard_copy_eligible_inner;

pub(super) fn git_logs_keyboard_copy_eligible(
    is_ide_mode: bool,
    show_search: bool,
    search_focused: bool,
    api_keyboard_surface_visible: bool,
    panels: &crate::app::IdePanelState,
) -> bool {
    git_logs_keyboard_copy_eligible_inner(
        is_ide_mode,
        show_search,
        search_focused,
        api_keyboard_surface_visible,
        panels,
    )
}

fn markdown_global_toggle_action(
    markdown_document: bool,
    is_ide_mode: bool,
    show_settings: bool,
    show_search: bool,
    search_focused: bool,
    panels: &crate::app::IdePanelState,
    physical_key: PhysicalKey,
    primary: bool,
    shift: bool,
    repeat: bool,
) -> Option<MarkdownGlobalToggleAction> {
    if !markdown_document
        || super::input_owner::terminal_keyboard_owner(
            is_ide_mode,
            show_settings,
            show_search,
            search_focused,
            panels,
        )
        || !primary
        || !shift
        || physical_key != PhysicalKey::Code(KeyCode::KeyV)
    {
        return None;
    }
    Some(if repeat {
        MarkdownGlobalToggleAction::Consume
    } else {
        MarkdownGlobalToggleAction::ToggleMode
    })
}

impl App {
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn handle_main_key_input(
        &mut self,
        event_loop: &HostLoop,
        key_event: KeyInput,
    ) {
        let chord = crate::keymap::Chord::from_event(
            crate::platform::CURRENT_PLATFORM,
            &key_event,
            self.modifiers,
        );
        if key_event.physical_key == PhysicalKey::Code(KeyCode::ShiftLeft) {
            self.left_shift_down = key_event.state == ElementState::Pressed;
        }
        if self.startup_blocks_key_input_with_chord(&key_event, chord) {
            return;
        }
        let editor_was_focused = self.editor_has_input_focus();
        self.handle_main_keyboard_input_inner(event_loop, key_event, chord);
        self.autosave_after_editor_focus_change(editor_was_focused);
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn handle_main_keyboard_input_inner(
        &mut self,
        event_loop: &HostLoop,
        key_event: KeyInput,
        chord: Option<crate::keymap::Chord>,
    ) {
        let ctrl = crate::platform::primary_shortcut_modifier(self.modifiers);
        let alt = self.modifiers.alt_key();
        let markdown_toggle = if key_event.state == ElementState::Pressed {
            let toggle_bound = chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::MarkdownToggleMode, chord));
            markdown_global_toggle_action(
                self.active_document_is_markdown(),
                self.is_ide_mode,
                self.show_settings,
                self.show_search,
                self.search_focused,
                &self.ide_panel,
                if toggle_bound { PhysicalKey::Code(KeyCode::KeyV) } else { key_event.physical_key },
                toggle_bound,
                toggle_bound,
                key_event.repeat,
            )
        } else {
            None
        };
        let defer_file_tree_text_input = markdown_toggle.is_some()
            && super::input_owner::file_tree_text_input_owns_keyboard_context(&self.ide_panel);
        let ctx = KeyCtx {
            event_loop,
            key_event: &key_event,
            chord,
            ctrl,
            alt,
            markdown_toggle,
            defer_file_tree_text_input,
        };
        let _ = self.route_main_key(&ctx);
    }
}

#[cfg(test)]
#[path = "main_keys_tests.rs"]
mod tests;
