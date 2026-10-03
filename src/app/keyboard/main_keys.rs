use super::*;

fn apply_terminal_alt_q_shortcut(
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

fn should_suppress_hover_for_keyboard(physical_key: PhysicalKey, ctrl: bool, alt: bool) -> bool {
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

fn apply_problems_alt_w_shortcut(panels: &mut crate::app::IdePanelState) {
    panels.toggle(crate::app::PanelId::Problems);
}

fn is_terminal_tab_close_shortcut(
    panels: &crate::app::IdePanelState,
) -> bool {
    panels.is_open(crate::app::PanelId::Terminal)
        && (panels.terminal_focused || (panels.term_show_search && panels.term_search_focused))
}

fn default_file_tree_chord_for_hit(keymap: &crate::keymap::Keymap, chord: crate::keymap::Chord) -> bool {
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
enum MarkdownGlobalToggleAction {
    ToggleMode,
    Consume,
}

#[path = "main_keys_vcs_copy.rs"]
mod vcs_copy;
use vcs_copy::git_logs_keyboard_copy_eligible;

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

        if self.markdown_toc.open {
            self.handle_markdown_toc_key(key_event.physical_key, key_event.state);
            return;
        }

        if key_event.state == ElementState::Pressed
            && should_suppress_hover_for_keyboard(key_event.physical_key, ctrl, alt)
        {
            let renderer = self.renderer.as_mut();
            let had_hover =
                crate::app::mouse::suppress_hover_popup_until_mouse_move(&mut self.hover, renderer);
            if had_hover {
                if let Some(w) = self.window.as_ref() {
                    w.request_redraw();
                }
            }
        }

        if self.show_settings && self.tool_installer.is_log_open() {
            if key_event.state == ElementState::Pressed {
                match key_event.physical_key {
                    PhysicalKey::Code(KeyCode::Escape) => {
                        self.tool_installer.close_log();
                        if let Some(window) = self.window.as_ref() {
                            window.request_redraw();
                        }
                    }
                    _ if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::SettingsCopyInstallerLog, chord)) => {
                        let log = self.tool_installer.full_log();
                        if !log.is_empty() {
                            self.set_clipboard_text(log);
                        }
                        if let Some(window) = self.window.as_ref() {
                            window.request_redraw();
                        }
                    }
                    _ => {}
                }
            }
            return;
        }

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
        if !defer_file_tree_text_input && self.handle_file_tree_modal_keyboard(&key_event) {
            return;
        }

        if self.handle_keymap_settings_key(&key_event, chord) { return; }

        let query_review_open = self
            .active_database_query_meta_state()
            .is_some_and(|(_, state)| state.review.is_some());
        if query_review_open && !self.show_settings {
            if key_event.state == ElementState::Pressed {
                match key_event.physical_key {
                    PhysicalKey::Code(KeyCode::Escape) => {
                        self.rollback_active_database_query();
                    }
                    PhysicalKey::Code(KeyCode::Enter) | PhysicalKey::Code(KeyCode::NumpadEnter) => {
                        self.commit_active_database_query();
                    }
                    _ => {}
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            return;
        }

        let terminal_owns_chord = chord.is_some_and(|chord| {
            super::input_owner::terminal_intercepts(chord, crate::platform::CURRENT_PLATFORM)
        }) && super::input_owner::terminal_keyboard_owner(
            self.is_ide_mode, self.show_settings, self.show_search, self.search_focused, &self.ide_panel,
        ) && !(self.ide_panel.term_show_search && self.ide_panel.term_search_focused);
        let terminal_close = chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::TerminalClose, chord));
        let terminal_toggle = chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::TerminalToggleFocus, chord));
        if key_event.state == ElementState::Pressed
            && (terminal_close || terminal_toggle)
            && (!terminal_owns_chord || chord.is_some_and(|chord| {
                super::input_owner::is_default_chord(
                    if terminal_close { crate::keymap::Command::TerminalClose } else { crate::keymap::Command::TerminalToggleFocus },
                    chord,
                    crate::platform::CURRENT_PLATFORM,
                )
            }))
        {
            if self.is_ide_mode {
                let has_terminal = !self.ide_panel.terminals.is_empty();
                let needs_terminal = apply_terminal_alt_q_shortcut(
                    &mut self.ide_panel,
                    terminal_close,
                    has_terminal,
                );
                if needs_terminal {
                    self.add_terminal();
                }
                self.defer_terminal_panel_until_ready();

                self.last_action = std::time::Instant::now();
                if let Some(w) = self.window.as_ref() {
                    w.request_redraw();
                }
                return;
            }
        }

        if self.modal_dialog_open() {
            if key_event.state == ElementState::Pressed {
                if key_event.physical_key == PhysicalKey::Code(KeyCode::Escape) {
                    self.cancel_pending_action();
                } else {
                    if let Some(dw) = self.confirm_dialog.window() {
                        dw.focus_window();
                        dw.request_redraw();
                    }
                }
            }
            return;
        }

        if key_event.state == ElementState::Pressed {
            if self.active_tab_is_database_query() {
                let history_open = self
                    .active_database_query_meta_state()
                    .is_some_and(|(_, state)| state.history_open);
                if history_open {
                    match key_event.physical_key {
                        PhysicalKey::Code(KeyCode::ArrowDown) => {
                            self.move_active_database_query_history_selection(1);
                            if let Some(window) = self.window.as_ref() {
                                window.request_redraw();
                            }
                            return;
                        }
                        PhysicalKey::Code(KeyCode::ArrowUp) => {
                            self.move_active_database_query_history_selection(-1);
                            if let Some(window) = self.window.as_ref() {
                                window.request_redraw();
                            }
                            return;
                        }
                        PhysicalKey::Code(KeyCode::Home) => {
                            self.set_active_database_query_history_selection(false);
                            if let Some(window) = self.window.as_ref() {
                                window.request_redraw();
                            }
                            return;
                        }
                        PhysicalKey::Code(KeyCode::End) => {
                            self.set_active_database_query_history_selection(true);
                            if let Some(window) = self.window.as_ref() {
                                window.request_redraw();
                            }
                            return;
                        }
                        PhysicalKey::Code(KeyCode::Enter)
                        | PhysicalKey::Code(KeyCode::NumpadEnter)
                            if !ctrl =>
                        {
                            let selected = self
                                .active_database_query_meta_state()
                                .map_or(0, |(_, state)| state.history_selected);
                            self.load_database_query_history_entry(selected);
                            if let Some(window) = self.window.as_ref() {
                                window.request_redraw();
                            }
                            return;
                        }
                        _ => {}
                    }
                }
                match key_event.physical_key {
                    PhysicalKey::Code(KeyCode::Escape) => {
                        let (running, reviewing, history_open) = self
                            .active_database_query_meta_state()
                            .map_or((false, false, false), |(_, state)| {
                                (state.running, state.review.is_some(), state.history_open)
                            });
                        if reviewing {
                            self.rollback_active_database_query();
                            if let Some(window) = self.window.as_ref() {
                                window.request_redraw();
                            }
                            return;
                        }
                        if running {
                            self.cancel_active_database_query();
                            if let Some(window) = self.window.as_ref() {
                                window.request_redraw();
                            }
                            return;
                        }
                        if history_open {
                            self.toggle_active_database_query_history();
                            if let Some(window) = self.window.as_ref() {
                                window.request_redraw();
                            }
                            return;
                        }
                    }
                    _ if !self.show_settings && !terminal_owns_chord && chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::DatabaseQueryRun, chord)) =>
                    {
                        self.run_active_database_query(
                            crate::app::database::DatabaseQueryMode::Run,
                        );
                        if let Some(window) = self.window.as_ref() {
                            window.request_redraw();
                        }
                        return;
                    }
                    _ if !self.show_settings && !terminal_owns_chord && chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::DatabaseQueryComplete, chord)) => {
                        self.show_active_database_query_completion();
                        if let Some(window) = self.window.as_ref() {
                            window.request_redraw();
                        }
                        return;
                    }
                    _ => {}
                }
            }
            if key_event.state == ElementState::Pressed
                && !self.show_settings
                && self.database_table_command_context_unowned()
                && self.run_bound_commands(
                    chord,
                    Some(crate::keymap::KeyContext::DatabaseTable),
                    key_event.repeat,
                )
            {
                return;
            }
            if (!self.show_settings && self.handle_database_table_key(&key_event, chord, terminal_owns_chord))
                || self.handle_pdf_key(&key_event, chord)
                || self.handle_image_key(&key_event, chord)
            {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
                return;
            }
            if self.handle_database_dialog_keyboard(&key_event) {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
                return;
            }
            if self.ide_panel.database.delete_prompt.is_some()
                || self.ide_panel.database.host_key_prompt.is_some()
            {
                if key_event.physical_key == PhysicalKey::Code(KeyCode::Escape) {
                    self.ide_panel.database.delete_prompt = None;
                    if self.ide_panel.database.host_key_prompt.is_some() {
                        self.cancel_database_host_key_prompt();
                    }
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                }
                return;
            }
            if key_event.physical_key == PhysicalKey::Code(KeyCode::Escape) {
                if self.ide_panel.database.context_menu.take().is_some()
                    || self
                        .ide_panel
                        .database
                        .ddl_hover
                        .borrow_mut()
                        .take()
                        .is_some()
                {
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                    return;
                }
            }
        }

        if key_event.state == ElementState::Pressed
            && self.ide_panel.database.ddl_hover.borrow().is_some()
        {
            match key_event.physical_key {
                PhysicalKey::Code(KeyCode::Escape) => {
                    *self.ide_panel.database.ddl_hover.borrow_mut() = None;
                }
                PhysicalKey::Code(KeyCode::KeyA) if ctrl => {
                    if let Some(state) = self.ide_panel.database.ddl_hover.borrow_mut().as_mut() {
                        state.selection_anchor = Some(0);
                        state.selection_cursor = Some(state.popup.text.len());
                    }
                }
                PhysicalKey::Code(KeyCode::KeyC) if ctrl => {
                    let selected = self
                        .ide_panel
                        .database
                        .ddl_hover
                        .borrow()
                        .as_ref()
                        .and_then(|state| {
                            let (a, b) = (state.selection_anchor?, state.selection_cursor?);
                            let (start, end) = (a.min(b), a.max(b));
                            state.popup.text.get(start..end).map(str::to_string)
                        });
                    if let Some(selected) = selected.filter(|text| !text.is_empty()) {
                        self.set_clipboard_text(selected);
                    }
                }
                _ => {}
            }
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            return;
        }

        if self.ide_panel.project_search.help_open {
            if key_event.state == ElementState::Pressed
                && key_event.physical_key == PhysicalKey::Code(KeyCode::Escape)
            {
                self.ide_panel.project_search.help_open = false;
                if let Some(w) = self.window.as_ref() {
                    w.request_redraw();
                }
            }
            return;
        }

        if key_event.state == ElementState::Pressed {
            if key_event.physical_key == PhysicalKey::Code(KeyCode::Escape)
                && (self.inline_git_popup.take().is_some()
                    || self.inline_git_diff_rx.take().is_some())
            {
                self.window.as_ref().unwrap().request_redraw();
                return;
            }
            if key_event.physical_key == PhysicalKey::Code(KeyCode::Escape)
                && crate::app::mouse::clear_hover_popup(&mut self.hover)
            {
                self.window.as_ref().unwrap().request_redraw();
            }
            if key_event.physical_key == PhysicalKey::Code(KeyCode::Escape)
                && self.ide_panel.git.close_commit_menus()
            {
                self.window.as_ref().unwrap().request_redraw();
                return;
            }
            if key_event.physical_key == PhysicalKey::Code(KeyCode::Escape)
                && self.ide_panel.api.close_api_mock_constraint_menu()
            {
                self.window.as_ref().unwrap().request_redraw();
                return;
            }
            if key_event.physical_key == PhysicalKey::Code(KeyCode::Escape)
                && self.close_active_api_output_example_menu()
            {
                self.window.as_ref().unwrap().request_redraw();
                return;
            }

            if let Some(action) = markdown_toggle {
                if action == MarkdownGlobalToggleAction::ToggleMode {
                    self.toggle_markdown_mode();
                }
                return;
            }

            // ── Ввод в поле игнора настроек ──────────────────────────────
            if self.show_settings && self.settings_tab == 0 && self.settings_ignore_focused {
                self.last_action = std::time::Instant::now();
                let ctrl = crate::platform::primary_shortcut_modifier(self.modifiers);
                let word = crate::platform::word_navigation_modifier(self.modifiers);
                let shift = self.modifiers.shift_key();
                match key_event.physical_key {
                    PhysicalKey::Code(KeyCode::Escape) => {
                        self.settings_ignore_focused = false;
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                    PhysicalKey::Code(KeyCode::Enter) | PhysicalKey::Code(KeyCode::NumpadEnter) => {
                        let trimmed = self
                            .settings_ignore_editor
                            .get_full_text()
                            .trim()
                            .to_string();
                        if !trimmed.is_empty() && !self.ide_ignore_patterns.contains(&trimmed) {
                            self.ide_ignore_patterns.push(trimmed);
                            self.settings_ignore_editor.select_all();
                            self.settings_ignore_editor.delete_selection();
                            self.save_current_config();
                            self.refresh_file_tree();
                        }
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                    PhysicalKey::Code(KeyCode::KeyA) if ctrl => {
                        self.settings_ignore_editor.select_all();
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                    PhysicalKey::Code(KeyCode::KeyC) if ctrl => {
                        if let Some(text) = self.settings_ignore_editor.get_selection() {
                            self.set_clipboard_text(text);
                        }
                        return;
                    }
                    PhysicalKey::Code(KeyCode::KeyX) if ctrl => {
                        if let Some(text) = self.settings_ignore_editor.get_selection() {
                            self.set_clipboard_text(text);
                            self.settings_ignore_editor.delete_selection();
                            self.window.as_ref().unwrap().request_redraw();
                        }
                        return;
                    }
                    PhysicalKey::Code(KeyCode::KeyV) if ctrl => {
                        if let Some(text) = self.get_clipboard_text() {
                            let clean = text.replace('\n', "").replace('\r', "");
                            if !clean.is_empty() {
                                self.settings_ignore_editor.insert_str(&clean);
                                self.window.as_ref().unwrap().request_redraw();
                            }
                        }
                        return;
                    }
                    PhysicalKey::Code(KeyCode::Backspace) => {
                        if word {
                            self.settings_ignore_editor.delete_word_backward();
                        } else {
                            self.settings_ignore_editor.backspace();
                        }
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                    PhysicalKey::Code(KeyCode::Delete) => {
                        if word {
                            self.settings_ignore_editor.delete_word_forward();
                        } else {
                            self.settings_ignore_editor.delete_forward();
                        }
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                    PhysicalKey::Code(KeyCode::ArrowLeft) => {
                        if word {
                            self.settings_ignore_editor.move_word_left(shift);
                        } else {
                            self.settings_ignore_editor.move_left(shift);
                        }
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                    PhysicalKey::Code(KeyCode::ArrowRight) => {
                        if word {
                            self.settings_ignore_editor.move_word_right(shift);
                        } else {
                            self.settings_ignore_editor.move_right(shift);
                        }
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                    PhysicalKey::Code(KeyCode::Home) => {
                        self.settings_ignore_editor.move_home(shift);
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                    PhysicalKey::Code(KeyCode::End) => {
                        self.settings_ignore_editor.move_end(shift);
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                    _ => {
                        if crate::platform::text_input_modifiers_allowed(self.modifiers) {
                            if let Some(txt) = key_event.logical_text.as_deref() {
                                let clean_txt = txt.replace('\n', "");
                                if !clean_txt.is_empty() {
                                    self.settings_ignore_editor.insert_str(&clean_txt);
                                    self.window.as_ref().unwrap().request_redraw();
                                    return;
                                }
                            }
                        }
                    }
                }
            }

            if let PhysicalKey::Code(KeyCode::Escape) = key_event.physical_key {
                if self.show_settings {
                    self.set_settings_visible(false);
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
            }

            if self.show_settings {
                if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::SettingsToggle, chord)) {
                    self.set_settings_visible(false);
                    self.window.as_ref().unwrap().request_redraw();
                }
                return;
            }

            let term_focused = self.is_ide_mode
                && self.ide_panel.terminal_focused
                && self.ide_panel.is_open(crate::app::PanelId::Terminal);
            if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::SettingsToggle, chord)) && !term_focused {
                self.set_settings_visible(!self.show_settings);
                self.is_dragging = false;
                self.window.as_ref().unwrap().request_redraw();
                return;
            }
            if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::ViewToggleFps, chord)) {
                if !term_focused {
                    self.show_fps = !self.show_fps;
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
            }

            if self.is_ide_mode && chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::ViewToggleProblems, chord))
                && (!terminal_owns_chord || chord.is_some_and(|chord| super::input_owner::is_default_chord(crate::keymap::Command::ViewToggleProblems, chord, crate::platform::CURRENT_PLATFORM)))
            {
                apply_problems_alt_w_shortcut(&mut self.ide_panel);
                crate::save_panel_state(&self.ide_panel);
                self.last_action = std::time::Instant::now();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
                return;
            }

            if self.is_ide_mode
                && chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::SearchProjectOpen, chord))
                && (!terminal_owns_chord || chord.is_some_and(|chord| super::input_owner::is_default_chord(crate::keymap::Command::SearchProjectOpen, chord, crate::platform::CURRENT_PLATFORM)))
            {
                self.open_project_search_panel();
                self.last_action = std::time::Instant::now();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
                return;
            }

            if self.is_ide_mode
                && self.ide_panel.is_open(crate::app::PanelId::Search)
                && self.ide_panel.project_search.focused.is_some()
            {
                self.handle_project_search_keyboard_input(key_event, chord);
                return;
            }

            // File-tree focus is exclusive. Handle F2/Delete/clipboard shortcuts
            // before stale editor/API focus can consume the key on another OS.
            let default_file_tree_chord = chord.is_some_and(|chord| default_file_tree_chord_for_hit(&self.keymap, chord));
            if (!terminal_owns_chord || default_file_tree_chord)
                && self.handle_file_tree_shortcut_with_chord(key_event.physical_key, ctrl, chord) {
                return;
            }

            if self.ide_panel.is_open(crate::app::PanelId::LspServers)
                && self.ide_panel.lsp_log_filter_focused
            {
                self.handle_lsp_log_filter_keyboard_input(key_event);
                return;
            }

            if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::GitCopySelection, chord))
                && (!terminal_owns_chord || chord.is_some_and(|chord| super::input_owner::is_default_chord(crate::keymap::Command::GitCopySelection, chord, crate::platform::CURRENT_PLATFORM))) {
                let graph_copy = self
                    .renderer
                    .as_ref()
                    .and_then(|renderer| renderer.selected_git_graph_tooltip_text());
                if let Some(text) = graph_copy {
                    self.set_clipboard_text(text);
                    if let Some(renderer) = self.renderer.as_mut() {
                        renderer.git_graph_tooltip_selection_anchor = None;
                        renderer.git_graph_tooltip_selection_cursor = None;
                        renderer.git_graph_tooltip_selecting = false;
                    }
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }

                let api_keyboard_surface_visible = self.active_tab_is_api_client()
                    || self.ide_panel.is_open(crate::app::PanelId::ApiClient);
                let vcs_copy = if git_logs_keyboard_copy_eligible(
                    self.is_ide_mode,
                    self.show_search,
                    self.search_focused,
                    api_keyboard_surface_visible,
                    &self.ide_panel,
                ) {
                    self.ide_panel.git.copy_owned_git_logs_selection()
                } else {
                    None
                };
                if let Some(text) = vcs_copy {
                    self.set_clipboard_text(text);
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
            }

            if self.ide_panel.git.message_focused
                && self.ide_panel.is_open(crate::app::PanelId::Git)
            {
                self.handle_git_message_keyboard_input(key_event);
                return;
            }

            if !self.show_settings && self.handle_api_client_keyboard_input_with_chord(&key_event, chord) {
                return;
            }

            if terminal_owns_chord {
                self.handle_terminal_keyboard_input(key_event);
                return;
            }

            if self.ide_panel.is_open(crate::app::PanelId::LspServers)
                && let Some(focused_name) = self.ide_panel.lsp_logs_focused.clone()
            {
                if let Some(ed) = self.ide_panel.lsp_log_editors.get_mut(&focused_name) {
                    let ctrl = crate::platform::primary_shortcut_modifier(self.modifiers);
                    let word = crate::platform::word_navigation_modifier(self.modifiers);
                    let shift = self.modifiers.shift_key();
                    match key_event.physical_key {
                        PhysicalKey::Code(KeyCode::KeyC) if ctrl => {
                            if let Some(text) = ed.get_selection() {
                                self.set_clipboard_text(text);
                            }
                            return;
                        }
                        PhysicalKey::Code(KeyCode::KeyA) if ctrl => {
                            ed.select_all();
                            self.window.as_ref().unwrap().request_redraw();
                            return;
                        }
                        PhysicalKey::Code(KeyCode::ArrowLeft) => {
                            if word {
                                ed.move_word_left(shift);
                            } else {
                                ed.move_left(shift);
                            }
                            self.window.as_ref().unwrap().request_redraw();
                            return;
                        }
                        PhysicalKey::Code(KeyCode::ArrowRight) => {
                            if word {
                                ed.move_word_right(shift);
                            } else {
                                ed.move_right(shift);
                            }
                            self.window.as_ref().unwrap().request_redraw();
                            return;
                        }
                        PhysicalKey::Code(KeyCode::Escape) => {
                            self.ide_panel.lsp_logs_focused = None;
                            self.window.as_ref().unwrap().request_redraw();
                            return;
                        }
                        _ => {}
                    }
                }
            }

            if key_event.state == ElementState::Pressed && chord.is_some_and(|chord|
                self.keymap.hit(crate::keymap::Command::TabsSwitchNext, chord)
                    || self.keymap.hit(crate::keymap::Command::TabsSwitchPrevious, chord))
            {
                let physical_key = if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::TabsSwitchNext, chord)) {
                    PhysicalKey::Code(KeyCode::PageDown)
                } else { PhysicalKey::Code(KeyCode::PageUp) };
                self.switch_tab_from_keyboard(physical_key);
                return;
            }

            if is_terminal_tab_close_shortcut(&self.ide_panel)
                && chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::TerminalCloseTab, chord)) {
                self.close_terminal_tab_at(self.ide_panel.active_terminal);
                self.last_action = std::time::Instant::now();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
                return;
            }

            if self.run_bound_commands(chord, None, key_event.repeat) {
                return;
            }

            if self.ide_panel.is_open(crate::app::PanelId::Terminal)
                && self.ide_panel.term_show_search
                && self.ide_panel.term_search_focused
            {
                self.handle_terminal_search_keyboard_input(key_event);
            } else if self.show_search && self.search_focused {
                self.handle_search_keyboard_input(key_event, chord);
            } else if super::input_owner::terminal_keyboard_owner(
                self.is_ide_mode,
                self.show_settings,
                self.show_search,
                self.search_focused,
                &self.ide_panel,
            ) && !(self.ide_panel.term_show_search && self.ide_panel.term_search_focused)
            {
                self.handle_terminal_keyboard_input(key_event);
            } else {
                self.handle_editor_keyboard_input(event_loop, key_event, chord);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keymap_recording_precedes_database_query_review_and_early_routes_respect_settings() {
        let source = include_str!("main_keys.rs");
        let handler = source.find("fn handle_main_keyboard_input_inner").expect("main handler exists");
        let source = &source[handler..];
        let capture = source.find("if self.handle_keymap_settings_key(&key_event, chord)").expect("recording capture route exists");
        let query_review = source.find("let query_review_open =").expect("query review route exists");
        assert!(capture < query_review, "recording capture must reject Enter before SQL review commits it: capture={capture}, review={query_review}");
        assert!(source.contains("&& !self.show_settings\n                && self.database_table_command_context_unowned()"), "DatabaseTable early dispatch must defer while Settings is open");
        assert!(source.contains("if !self.show_settings && self.handle_api_client_keyboard_input_with_chord"), "API early dispatch must defer while Settings is open");
    }

    #[test]
    fn api_early_command_route_is_after_modal_and_text_input_owners() {
        let source = include_str!("main_keys.rs");
        let handler = source.find("fn handle_main_keyboard_input_inner").expect("main handler exists");
        let source = &source[handler..];
        let modal = source.find("if self.modal_dialog_open()").expect("modal route exists");
        let database_table = source.find("self.database_table_command_context_unowned()").expect("database route exists");
        let api = source.find("self.handle_api_client_keyboard_input_with_chord").expect("API route exists");
        let generic = source.find("self.run_bound_commands(chord, None").expect("generic command route exists");
        assert!(modal < database_table, "modal must own keyboard before DatabaseTable early dispatch: modal={modal}, table={database_table}");
        assert!(database_table < api, "database panel routing precedes API early dispatch: table={database_table}, api={api}");
        assert!(api < generic, "API handler owns keyboard before generic command dispatch: api={api}, generic={generic}");
        assert!(source.contains("self.database_table_command_context_unowned()"), "DatabaseTable route checks modal and text-input ownership in its predicate");
    }

    #[test]
    fn api_global_command_does_not_fire_through_main_handler_while_settings_is_open() {
        let Some(mut app) = crate::app::app_behavior_tests::test_app() else {
            panic!("test app must initialize");
        };
        app.is_ide_mode = true;
        app.show_welcome = false;
        app.show_settings = true;
        app.ide_panel.open(crate::app::PanelId::ApiClient);
        let chord = crate::keymap::Chord::parse(crate::platform::CURRENT_PLATFORM, "mod+alt+g")
            .expect("test chord parses");
        let mut overrides = crate::keymap::KeymapOverrides::default();
        overrides.add_chord(crate::platform::CURRENT_PLATFORM, crate::keymap::Command::GitToggleGraph, chord);
        app.keymap = crate::keymap::Keymap::build(&overrides);
        assert!(app.active_tab_is_api_client() || app.ide_panel.is_open(crate::app::PanelId::ApiClient), "Settings regression fixture must retain a visible API Client surface: active_api={} open_api={}", app.active_tab_is_api_client(), app.ide_panel.is_open(crate::app::PanelId::ApiClient));
        app.modifiers = if crate::platform::CURRENT_PLATFORM == crate::platform::PlatformKind::Macos {
            winit::keyboard::ModifiersState::SUPER | winit::keyboard::ModifiersState::ALT
        } else {
            winit::keyboard::ModifiersState::CONTROL | winit::keyboard::ModifiersState::ALT
        };
        let state = crate::app::events::host_loop::HeadlessLoopState::default();
        let event = KeyInput {
            physical_key: PhysicalKey::Code(KeyCode::KeyG),
            logical_text: None,
            text: None,
            state: ElementState::Pressed,
            repeat: false,
        };
        app.handle_main_key_input(&crate::app::events::host_loop::HostLoop::headless(&state), event);
        assert_eq!(app.ide_panel.git.bottom_pane, crate::app::git_panel::GitBottomPane::Closed, "Settings must own the key before API/global command dispatch: pane={:?}", app.ide_panel.git.bottom_pane);
    }

    fn database_table_hotkey_app() -> (App, crate::keymap::Chord) {
        let Some(mut app) = crate::app::app_behavior_tests::test_app() else {
            panic!("test app must initialize");
        };
        app.is_ide_mode = true;
        app.show_welcome = false;
        let tab_id = crate::app::database::DatabaseTabId(7);
        let mut tab = crate::app::app_behavior_tests::tab_with("items", None, "");
        let mut table = crate::app::database::DatabaseTableTabState::default();
        table.loading = false;
        table.metadata = Some(crate::app::database::DatabaseTableMetadata {
            database_name: "test".to_string(),
            table_name: "items".to_string(),
            columns: vec![crate::app::database::DatabaseColumnInfo {
                ordinal: 1,
                name: "value".to_string(),
                type_name: "text".to_string(),
                type_oid: 25,
                type_kind: crate::app::database::DatabaseTypeKind::Other,
                nullable: true,
                default_expression: None,
                identity: false,
                generated: false,
                primary_key: false,
                enum_values: Vec::new(),
            }],
            primary_key_columns: Vec::new(),
            editable: true,
            read_only_reason: None,
            notices: Vec::new(),
        });
        tab.kind = crate::app::EditorTabKind::DatabaseTable(
            crate::app::database::DatabaseTableTabMeta {
                tab_id,
                connection_id: crate::app::database::DatabaseConnectionId(3),
                database_name: "test".to_string(),
                table_name: "items".to_string(),
            },
            table,
        );
        app.tabs = vec![tab];
        app.active_tab = 0;
        let chord = crate::keymap::Chord::parse(crate::platform::CURRENT_PLATFORM, "mod+alt+g")
            .expect("test chord parses");
        let mut overrides = crate::keymap::KeymapOverrides::default();
        overrides.add_chord(crate::platform::CURRENT_PLATFORM, crate::keymap::Command::DatabaseTableAddRow, chord);
        app.keymap = crate::keymap::Keymap::build(&overrides);
        app.modifiers = if crate::platform::CURRENT_PLATFORM == crate::platform::PlatformKind::Macos {
            winit::keyboard::ModifiersState::SUPER | winit::keyboard::ModifiersState::ALT
        } else {
            winit::keyboard::ModifiersState::CONTROL | winit::keyboard::ModifiersState::ALT
        };
        (app, chord)
    }

    fn press_database_table_hotkey(app: &mut App) {
        let state = crate::app::events::host_loop::HeadlessLoopState::default();
        app.handle_main_key_input(
            &crate::app::events::host_loop::HostLoop::headless(&state),
            KeyInput {
                physical_key: PhysicalKey::Code(KeyCode::KeyG),
                logical_text: None,
                text: None,
                state: ElementState::Pressed,
                repeat: false,
            },
        );
    }

    fn assert_active_database_table(app: &App, scenario: &str) {
        assert!(app.active_tab_is_database_table(), "{scenario} fixture must have the DatabaseTable surface active: active={}", app.active_tab);
    }

    fn added_database_rows(app: &App) -> usize {
        let Some(crate::app::EditorTabKind::DatabaseTable(_, state)) = app.tabs.get(app.active_tab).map(|tab| &tab.kind) else {
            panic!("active DatabaseTable fixture must retain its table state");
        };
        state.grid.added_rows.len()
    }

    #[test]
    fn database_table_chord_does_not_dispatch_behind_modal() {
        let (mut app, _) = database_table_hotkey_app();
        assert_active_database_table(&app, "modal");
        app.ide_panel.database.table_modal = Some(crate::app::database::DatabaseTableModal::RefreshPrompt {
            tab_id: crate::app::database::DatabaseTabId(7),
            close_after_save: false,
        });
        assert!(app.ide_panel.database.table_modal.is_some(), "modal fixture must be open: modal={:?}", app.ide_panel.database.table_modal);
        press_database_table_hotkey(&mut app);
        assert_eq!(added_database_rows(&app), 0, "DatabaseTable chord must not add a row behind its modal: added={}", added_database_rows(&app));
    }

    #[test]
    fn database_table_chord_does_not_dispatch_while_text_field_focused() {
        let (mut app, _) = database_table_hotkey_app();
        assert_active_database_table(&app, "text field");
        let Some(crate::app::EditorTabKind::DatabaseTable(_, state)) = app.tabs.get_mut(app.active_tab).map(|tab| &mut tab.kind) else {
            panic!("active DatabaseTable fixture must retain its table state");
        };
        state.grid.focused_input = Some(crate::app::database::DatabaseTableInputTarget::Where);
        assert_eq!(state.grid.focused_input, Some(crate::app::database::DatabaseTableInputTarget::Where), "text input fixture must be focused: focused={:?}", state.grid.focused_input);
        press_database_table_hotkey(&mut app);
        assert_eq!(added_database_rows(&app), 0, "DatabaseTable chord must not add a row while a text field owns input: added={}", added_database_rows(&app));
    }

    #[test]
    fn database_table_chord_does_not_dispatch_while_settings_is_open() {
        let (mut app, _) = database_table_hotkey_app();
        assert_active_database_table(&app, "Settings");
        app.show_settings = true;
        assert!(app.show_settings, "Settings fixture must be open: show_settings={}", app.show_settings);
        press_database_table_hotkey(&mut app);
        assert_eq!(added_database_rows(&app), 0, "DatabaseTable chord must not add a row while Settings is open: added={}", added_database_rows(&app));
    }

    #[test]
    fn database_table_chord_dispatches_when_table_owns_input() {
        let (mut app, _) = database_table_hotkey_app();
        assert_active_database_table(&app, "positive control");
        assert!(app.ide_panel.database.table_modal.is_none(), "positive control must not have a modal: modal={:?}", app.ide_panel.database.table_modal);
        assert!(!app.show_settings, "positive control must have Settings closed: show_settings={}", app.show_settings);
        assert!(app.database_table_command_context_unowned(), "positive control must have an unowned table context: unowned={}", app.database_table_command_context_unowned());
        press_database_table_hotkey(&mut app);
        assert_eq!(added_database_rows(&app), 1, "DatabaseTable chord must add a row when the table owns input: added={}", added_database_rows(&app));
    }

    #[test]
    fn settings_prevents_enter_from_committing_database_query_review() {
        let Some(mut app) = crate::app::app_behavior_tests::test_app() else {
            panic!("test app must initialize");
        };
        app.is_ide_mode = true;
        app.show_welcome = false;
        let mut tab = crate::app::app_behavior_tests::tab_with("SQL Console", None, "select 1");
        let mut query = crate::app::database::DatabaseQueryTabState::default();
        query.review = Some(crate::app::database::DatabaseQueryReviewState {
            transaction_id: crate::app::database::DatabaseTransactionId(2),
            sql: "select 1".to_string(),
            source_offset: 0,
            started_unix_ms: 1,
            deadline_unix_ms: 2,
            duration_ms: 1,
            returned_rows: 0,
            changed_rows: 0,
            mode: crate::app::database::DatabaseQueryMode::Run,
            finishing: false,
        });
        tab.kind = crate::app::EditorTabKind::DatabaseQuery(
            crate::app::database::DatabaseQueryTabMeta {
                console_id: crate::app::database::SqlConsoleId(4),
                connection_id: crate::app::database::DatabaseConnectionId(3),
                database_name: "test".to_string(),
                title: "SQL Console".to_string(),
            },
            query,
        );
        app.tabs = vec![tab];
        app.active_tab = 0;
        app.show_settings = true;
        assert!(app.active_tab_is_database_query(), "review fixture must have the SQL Console surface active: active={}", app.active_tab);
        assert!(app.active_database_query_meta_state().is_some_and(|(_, state)| state.review.is_some()), "review fixture must start in SQL review: review={:?}", app.active_database_query_meta_state().map(|(_, state)| &state.review));
        let state = crate::app::events::host_loop::HeadlessLoopState::default();
        app.handle_main_key_input(
            &crate::app::events::host_loop::HostLoop::headless(&state),
            KeyInput {
                physical_key: PhysicalKey::Code(KeyCode::Enter),
                logical_text: None,
                text: None,
                state: ElementState::Pressed,
                repeat: false,
            },
        );
        assert!(app.active_database_query_meta_state().is_some_and(|(_, state)| state.review.is_some()), "Enter must not commit a SQL review behind Settings: review={:?}", app.active_database_query_meta_state().map(|(_, state)| &state.review));
    }

    fn terminal_open(panels: &crate::app::IdePanelState) -> bool {
        panels.is_open(crate::app::PanelId::Terminal)
    }

    fn relocated_top_terminal_with_explorer_open() -> crate::app::IdePanelState {
        let mut panels = crate::app::IdePanelState::default();
        let terminal = panels
            .slots
            .iter_mut()
            .find(|slot| slot.id == crate::app::PanelId::Terminal)
            .unwrap();
        terminal.group = crate::app::PanelGroup::Top;
        terminal.open = false;
        panels.open(crate::app::PanelId::Explorer);
        panels
    }

    #[test]
    fn terminal_alt_q_opens_terminal_and_requests_spawn_when_missing() {
        let mut panels = crate::app::IdePanelState::default();

        let needs_spawn = apply_terminal_alt_q_shortcut(&mut panels, false, false);

        assert!(needs_spawn);
        assert!(terminal_open(&panels));
        assert!(panels.terminal_focused);
    }

    #[test]
    fn terminal_alt_q_focuses_existing_closed_terminal_without_spawn() {
        let mut panels = crate::app::IdePanelState::default();

        let needs_spawn = apply_terminal_alt_q_shortcut(&mut panels, false, true);

        assert!(!needs_spawn);
        assert!(terminal_open(&panels));
        assert!(panels.terminal_focused);
    }

    #[test]
    fn terminal_alt_q_opens_relocated_top_terminal_in_its_current_group() {
        let mut panels = relocated_top_terminal_with_explorer_open();

        assert!(panels.is_open(crate::app::PanelId::Explorer));
        assert!(!terminal_open(&panels));

        let needs_spawn = apply_terminal_alt_q_shortcut(&mut panels, false, true);

        assert!(!needs_spawn);
        assert!(terminal_open(&panels));
        assert!(!panels.is_open(crate::app::PanelId::Explorer));
        assert!(panels.terminal_focused);
    }

    #[test]
    fn terminal_alt_q_relocated_top_terminal_requests_spawn_when_missing() {
        let mut panels = relocated_top_terminal_with_explorer_open();

        assert!(panels.is_open(crate::app::PanelId::Explorer));
        assert!(!terminal_open(&panels));

        let needs_spawn = apply_terminal_alt_q_shortcut(&mut panels, false, false);

        assert!(needs_spawn);
        assert!(terminal_open(&panels));
        assert!(!panels.is_open(crate::app::PanelId::Explorer));
        assert!(panels.terminal_focused);
    }

    #[test]
    fn terminal_alt_q_toggles_focus_when_open() {
        let mut panels = crate::app::IdePanelState::default();
        panels.toggle(crate::app::PanelId::Terminal);
        panels.terminal_focused = true;

        assert!(!apply_terminal_alt_q_shortcut(&mut panels, false, true));
        assert!(terminal_open(&panels));
        assert!(!panels.terminal_focused);

        panels.git.message_focused = true;
        panels.term_search_focused = true;
        assert!(!apply_terminal_alt_q_shortcut(&mut panels, false, true));
        assert!(terminal_open(&panels));
        assert!(panels.terminal_focused);
        assert!(!panels.git.message_focused);
        assert!(!panels.term_search_focused);
    }

    #[test]
    fn terminal_alt_shift_q_closes_or_opens_without_focus_toggle() {
        let mut panels = crate::app::IdePanelState::default();
        panels.toggle(crate::app::PanelId::Terminal);
        panels.terminal_focused = true;

        assert!(!apply_terminal_alt_q_shortcut(&mut panels, true, true));
        assert!(!terminal_open(&panels));
        assert!(!panels.terminal_focused);

        panels.git.message_focused = true;
        assert!(apply_terminal_alt_q_shortcut(&mut panels, true, false));
        assert!(terminal_open(&panels));
        assert!(panels.terminal_focused);
        assert!(!panels.git.message_focused);
    }

    #[test]
    fn terminal_alt_q_closes_bottom_peer_panel_before_opening_terminal() {
        let mut panels = crate::app::IdePanelState::default();
        panels.toggle(crate::app::PanelId::Problems);

        assert!(!panels.is_open(crate::app::PanelId::Terminal));
        assert!(panels.is_open(crate::app::PanelId::Problems));

        assert!(!apply_terminal_alt_q_shortcut(&mut panels, false, true));
        assert!(panels.is_open(crate::app::PanelId::Terminal));
        assert!(!panels.is_open(crate::app::PanelId::Problems));
        assert!(panels.terminal_focused);

        panels.toggle(crate::app::PanelId::Problems);
        assert!(!apply_terminal_alt_q_shortcut(&mut panels, true, true));
        assert!(panels.is_open(crate::app::PanelId::Terminal));
        assert!(!panels.is_open(crate::app::PanelId::Problems));
        assert!(panels.terminal_focused);

        panels.toggle(crate::app::PanelId::Problems);
        assert!(apply_terminal_alt_q_shortcut(&mut panels, true, false));
        assert!(panels.is_open(crate::app::PanelId::Terminal));
        assert!(!panels.is_open(crate::app::PanelId::Problems));
        assert!(panels.terminal_focused);
    }

    #[test]
    fn remapped_terminal_close_tab_uses_focus_and_keymap_chord() {
        let overrides = crate::keymap::KeymapOverrides::from_value(serde_json::json!({
            "terminal.close_tab": ["alt+4"]
        }));
        let keymap = crate::keymap::Keymap::build_for(crate::platform::PlatformKind::Linux, &overrides);
        let chord = crate::keymap::Chord::parse(crate::platform::PlatformKind::Linux, "alt+4").unwrap();
        let mut panels = crate::app::IdePanelState::default();
        panels.open(crate::app::PanelId::Terminal);
        panels.terminal_focused = true;

        assert!(is_terminal_tab_close_shortcut(&panels) && keymap.hit(crate::keymap::Command::TerminalCloseTab, chord));

        panels.terminal_focused = false;
        assert!(!is_terminal_tab_close_shortcut(&panels) && keymap.hit(crate::keymap::Command::TerminalCloseTab, chord));

        panels.term_show_search = true;
        panels.term_search_focused = true;
        assert!(is_terminal_tab_close_shortcut(&panels) && keymap.hit(crate::keymap::Command::TerminalCloseTab, chord));
    }

    #[test]
    fn terminal_focus_only_allows_default_file_tree_chord_for_its_hit_command() {
        let overrides = crate::keymap::KeymapOverrides::from_value(serde_json::json!({
            "file_tree.paste": ["ctrl+z"]
        }));
        let keymap = crate::keymap::Keymap::build_for(crate::platform::PlatformKind::Linux, &overrides);
        let chord = crate::keymap::Chord::parse(crate::platform::PlatformKind::Linux, "ctrl+z").unwrap();
        assert!(keymap.hit(crate::keymap::Command::FileTreePaste, chord));
        let mut panels = crate::app::IdePanelState::default();
        panels.open(crate::app::PanelId::Terminal);
        panels.terminal_focused = true;
        assert!(super::input_owner::terminal_keyboard_owner(true, false, false, false, &panels));
        assert!(!default_file_tree_chord_for_hit(&keymap, chord));
    }

    #[test]
    fn reassigned_project_search_open_passes_through_terminal_focus() {
        let overrides = crate::keymap::KeymapOverrides::from_value(serde_json::json!({
            "search.project.open": ["ctrl+p"]
        }));
        let keymap = crate::keymap::Keymap::build_for(crate::platform::PlatformKind::Linux, &overrides);
        let chord = crate::keymap::Chord::parse(crate::platform::PlatformKind::Linux, "ctrl+p").unwrap();
        let mut panels = crate::app::IdePanelState::default();
        panels.open(crate::app::PanelId::Terminal);
        panels.terminal_focused = true;
        let terminal_owns_chord = super::input_owner::terminal_keyboard_owner(true, false, false, false, &panels);

        assert!(keymap.hit(crate::keymap::Command::SearchProjectOpen, chord));
        assert!(terminal_owns_chord);
        assert!(!(!terminal_owns_chord
            || super::input_owner::is_default_chord(crate::keymap::Command::SearchProjectOpen, chord, crate::platform::PlatformKind::Linux)));
    }

    #[test]
    fn problems_alt_w_toggles_without_terminal_clickthrough_focus_mode() {
        let mut panels = crate::app::IdePanelState::default();

        apply_problems_alt_w_shortcut(&mut panels);
        assert!(panels.is_open(crate::app::PanelId::Problems));
        assert!(!panels.is_open(crate::app::PanelId::Terminal));
        assert!(!panels.terminal_focused);
        assert!(panels.bottom_panel_blocks_editor_hover());

        apply_problems_alt_w_shortcut(&mut panels);
        assert!(!panels.is_open(crate::app::PanelId::Problems));
        assert!(!panels.bottom_panel_blocks_editor_hover());
    }

    #[test]
    fn hover_keyboard_suppression_only_allows_escape_and_arrows() {
        assert!(!should_suppress_hover_for_keyboard(
            PhysicalKey::Code(KeyCode::KeyC),
            true,
            false,
        ));
        assert!(!should_suppress_hover_for_keyboard(
            PhysicalKey::Code(KeyCode::KeyQ),
            false,
            true,
        ));
        assert!(!should_suppress_hover_for_keyboard(
            PhysicalKey::Code(KeyCode::AltLeft),
            false,
            false,
        ));
        assert!(!should_suppress_hover_for_keyboard(
            PhysicalKey::Code(KeyCode::Tab),
            false,
            true,
        ));
        assert!(!should_suppress_hover_for_keyboard(
            PhysicalKey::Code(KeyCode::KeyW),
            true,
            false,
        ));
        assert!(should_suppress_hover_for_keyboard(
            PhysicalKey::Code(KeyCode::Escape),
            false,
            false,
        ));
        assert!(should_suppress_hover_for_keyboard(
            PhysicalKey::Code(KeyCode::ArrowLeft),
            false,
            false,
        ));
    }

    fn markdown_toggle_for_test(
        panels: &crate::app::IdePanelState,
        show_settings: bool,
        show_search: bool,
        search_focused: bool,
        primary: bool,
        shift: bool,
        repeat: bool,
    ) -> Option<MarkdownGlobalToggleAction> {
        markdown_global_toggle_action(
            true,
            true,
            show_settings,
            show_search,
            search_focused,
            panels,
            PhysicalKey::Code(KeyCode::KeyV),
            primary,
            shift,
            repeat,
        )
    }

    fn stale_terminal_focus_state() -> crate::app::IdePanelState {
        let mut panels = crate::app::IdePanelState::default();
        panels.open(crate::app::PanelId::Terminal);
        panels.terminal_focused = true;
        panels
    }

    #[test]
    fn markdown_global_toggle_matches_only_primary_shift_v_and_consumes_repeat() {
        let panels = crate::app::IdePanelState::default();
        assert_eq!(
            markdown_toggle_for_test(&panels, false, false, false, true, true, false),
            Some(MarkdownGlobalToggleAction::ToggleMode)
        );
        assert_eq!(
            markdown_toggle_for_test(&panels, false, false, false, true, true, true),
            Some(MarkdownGlobalToggleAction::Consume)
        );
        assert_eq!(
            markdown_toggle_for_test(&panels, false, false, false, true, false, false),
            None,
            "plain Primary+V must keep focused-field paste semantics"
        );
        assert_eq!(
            markdown_global_toggle_action(
                false,
                true,
                false,
                false,
                false,
                &panels,
                PhysicalKey::Code(KeyCode::KeyV),
                true,
                true,
                false,
            ),
            None,
            "non-Markdown documents must not gain a mode action"
        );
        assert_eq!(
            markdown_global_toggle_action(
                true,
                true,
                false,
                false,
                false,
                &panels,
                PhysicalKey::Code(KeyCode::KeyC),
                true,
                true,
                false,
            ),
            None
        );
    }

    #[test]
    fn markdown_global_toggle_uses_actual_terminal_keyboard_owner() {
        let mut panels = stale_terminal_focus_state();
        assert!(super::input_owner::terminal_keyboard_owner(
            true, false, false, false, &panels
        ));
        assert_eq!(
            markdown_toggle_for_test(&panels, false, false, false, true, true, false),
            None,
            "actual terminal body owner keeps terminal semantics"
        );

        panels.terminal_focused = false;
        panels.term_show_search = true;
        panels.term_search_focused = true;
        assert!(super::input_owner::terminal_keyboard_owner(
            true, false, false, false, &panels
        ));
        assert_eq!(
            markdown_toggle_for_test(&panels, false, false, false, true, true, false),
            None,
            "actual terminal-search owner keeps terminal-search semantics"
        );

        panels.term_search_focused = false;
        assert!(!super::input_owner::terminal_keyboard_owner(
            true, false, false, false, &panels
        ));
        assert_eq!(
            markdown_toggle_for_test(&panels, false, false, false, true, true, false),
            Some(MarkdownGlobalToggleAction::ToggleMode),
            "open terminal without keyboard ownership must not block Markdown toggle"
        );
    }

    #[test]
    fn markdown_global_toggle_beats_stale_terminal_focus_for_project_search() {
        let mut panels = stale_terminal_focus_state();
        panels.open(crate::app::PanelId::Search);
        panels.project_search.focused = Some(crate::app::project_search::ProjectSearchField::Query);

        assert!(panels.is_open(crate::app::PanelId::Terminal));
        assert!(
            panels.terminal_focused,
            "fixture must preserve stale terminal focus"
        );
        assert_eq!(
            panels.project_search.focused,
            Some(crate::app::project_search::ProjectSearchField::Query)
        );
        assert!(!super::input_owner::terminal_keyboard_owner(
            true, false, false, false, &panels
        ));
        assert_eq!(
            markdown_toggle_for_test(&panels, false, false, false, true, true, false),
            Some(MarkdownGlobalToggleAction::ToggleMode),
            "central route must consume Primary+Shift+V before project-search KeyV paste"
        );
        assert_eq!(
            markdown_toggle_for_test(&panels, false, false, false, true, true, true),
            Some(MarkdownGlobalToggleAction::Consume),
            "repeat must remain consumed instead of reaching project-search paste"
        );
        assert_eq!(
            markdown_toggle_for_test(&panels, false, false, false, true, false, false),
            None,
            "plain Primary+V remains available to project-search paste"
        );
    }

    #[test]
    fn markdown_global_toggle_beats_stale_terminal_focus_for_other_text_owners() {
        let mut global_search = stale_terminal_focus_state();
        assert_eq!(
            markdown_toggle_for_test(&global_search, false, true, true, true, true, false),
            Some(MarkdownGlobalToggleAction::ToggleMode),
            "global Search outranks stale terminal body focus"
        );

        global_search.term_show_search = true;
        global_search.term_search_focused = true;
        assert_eq!(
            markdown_toggle_for_test(&global_search, false, true, true, true, true, false),
            None,
            "actual terminal Search still outranks global Search"
        );

        let mut git = stale_terminal_focus_state();
        git.open(crate::app::PanelId::Git);
        git.git.message_focused = true;
        assert_eq!(
            markdown_toggle_for_test(&git, false, false, false, true, true, false),
            Some(MarkdownGlobalToggleAction::ToggleMode),
            "Git message focus must outrank stale terminal body focus"
        );

        let settings = stale_terminal_focus_state();
        assert_eq!(
            markdown_toggle_for_test(&settings, true, false, false, true, true, false),
            Some(MarkdownGlobalToggleAction::ToggleMode),
            "Settings keyboard ownership must outrank stale terminal body focus"
        );
    }

    #[test]
    fn file_tree_name_inputs_defer_to_global_toggle_but_hard_modals_keep_priority() {
        let mut panels = crate::app::IdePanelState::default();
        panels.file_tree_create_dialog = Some(crate::app::file_tree::FileTreeCreateDialog {
            kind: crate::app::file_tree::FileTreeCreateKind::File,
            parent_dir: std::path::PathBuf::from("/tmp"),
            editor: crate::editor::Editor::new(64),
            error: None,
        });
        assert!(super::input_owner::file_tree_text_input_owns_keyboard_context(&panels));

        panels.file_tree_delete_dialog = Some(crate::app::file_tree::FileTreeDeleteDialog {
            paths: vec![std::path::PathBuf::from("/tmp/example.md")],
            error: None,
        });
        assert!(
            !super::input_owner::file_tree_text_input_owns_keyboard_context(&panels),
            "hard confirmation must retain priority over the underlying name field"
        );
    }

    #[test]
    fn graph_tooltip_copy_keeps_priority_over_owned_vcs_console_copy() {
        let source = include_str!("main_keys.rs").split("\n#[cfg(test)]").next().unwrap();
        let copy_route = &source[source
            .find("if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::GitCopySelection, chord))")
            .expect("global copy route")..];
        let graph = copy_route
            .find("selected_git_graph_tooltip_text")
            .expect("Git Graph tooltip copy");
        let vcs = copy_route
            .find("copy_owned_git_logs_selection")
            .expect("owned VCS Console copy");
        let message = copy_route
            .find("self.handle_git_message_keyboard_input(key_event)")
            .expect("Git message route");
        assert!(
            graph < vcs,
            "Git Graph tooltip must keep higher copy priority"
        );
        assert!(
            vcs < message,
            "owned VCS copy must stay before Git message routing"
        );
    }

    #[test]
    fn reassigned_editor_command_routes_through_main_keyboard_input() {
        let Some(mut app) = crate::app::app_behavior_tests::test_app() else { return; };
        app.is_ide_mode = true;
        app.show_welcome = false;
        app.tabs.push(crate::app::app_behavior_tests::tab_with(
            "hotkeys.rs",
            Some("/tmp/hotkeys.rs"),
            "",
        ));
        app.active_tab = 0;
        let chord = crate::keymap::Chord::parse(crate::platform::CURRENT_PLATFORM, "mod+alt+g")
            .expect("test chord");
        let mut overrides = crate::keymap::KeymapOverrides::default();
        overrides.add_chord(
            crate::platform::CURRENT_PLATFORM,
            crate::keymap::Command::EditorGitDiffPrevHunk,
            chord,
        );
        app.keymap = crate::keymap::Keymap::build(&overrides);
        app.modifiers = if crate::platform::CURRENT_PLATFORM == crate::platform::PlatformKind::Macos {
            winit::keyboard::ModifiersState::SUPER | winit::keyboard::ModifiersState::ALT
        } else {
            winit::keyboard::ModifiersState::CONTROL | winit::keyboard::ModifiersState::ALT
        };
        let state = crate::app::events::host_loop::HeadlessLoopState::default();
        app.handle_main_key_input(
            &crate::app::events::host_loop::HostLoop::headless(&state),
            KeyInput {
                physical_key: PhysicalKey::Code(KeyCode::KeyG),
                logical_text: None,
                text: None,
                state: ElementState::Pressed,
                repeat: false,
            },
        );
        assert_eq!(app.readonly_notice_text, "Нет изменений Git в документе");
    }

    #[test]
    fn reassigned_sql_command_routes_through_main_keyboard_input() {
        fn sql_app() -> Option<App> {
            let text = "select  'a;  b'  from  t where x=$tag$keep  spaces$tag$; -- keep  comment";
            let mut app = crate::app::app_behavior_tests::test_app()?;
            app.is_ide_mode = true;
            app.show_welcome = false;
            app.editor = crate::app::app_behavior_tests::editor_with(text);
            let mut tab = crate::app::app_behavior_tests::tab_with("SQL Console", None, text);
            tab.kind = crate::app::EditorTabKind::DatabaseQuery(
                crate::app::database::DatabaseQueryTabMeta {
                    console_id: crate::app::database::SqlConsoleId(7),
                    connection_id: crate::app::database::DatabaseConnectionId(3),
                    database_name: "postgres".to_string(),
                    title: "SQL Console".to_string(),
                },
                crate::app::database::DatabaseQueryTabState::default(),
            );
            app.tabs = vec![tab];
            app.active_tab = 0;
            Some(app)
        }

        let chord = crate::keymap::Chord::parse(crate::platform::CURRENT_PLATFORM, "mod+alt+g")
            .expect("test chord");
        let mut overrides = crate::keymap::KeymapOverrides::default();
        overrides.add_chord(
            crate::platform::CURRENT_PLATFORM,
            crate::keymap::Command::DatabaseQueryFormat,
            chord,
        );

        let Some(mut direct_app) = sql_app() else { return; };
        direct_app.keymap = crate::keymap::Keymap::build(&overrides);
        assert!(direct_app.active_tab_is_database_query());
        assert!(direct_app.run_bound_commands(Some(chord), None, false));
        assert_ne!(
            direct_app.editor.get_full_text(),
            "select  'a;  b'  from  t where x=$tag$keep  spaces$tag$; -- keep  comment"
        );

        let Some(mut app) = sql_app() else { return; };
        app.keymap = crate::keymap::Keymap::build(&overrides);
        app.modifiers = if crate::platform::CURRENT_PLATFORM == crate::platform::PlatformKind::Macos {
            winit::keyboard::ModifiersState::SUPER | winit::keyboard::ModifiersState::ALT
        } else {
            winit::keyboard::ModifiersState::CONTROL | winit::keyboard::ModifiersState::ALT
        };
        let state = crate::app::events::host_loop::HeadlessLoopState::default();
        app.handle_main_key_input(
            &crate::app::events::host_loop::HostLoop::headless(&state),
            KeyInput {
                physical_key: PhysicalKey::Code(KeyCode::KeyG),
                logical_text: None,
                text: None,
                state: ElementState::Pressed,
                repeat: false,
            },
        );
        assert_ne!(
            app.editor.get_full_text(),
            "select  'a;  b'  from  t where x=$tag$keep  spaces$tag$; -- keep  comment"
        );
    }

    #[test]
    fn markdown_global_toggle_precedes_non_terminal_text_field_routes() {
        let source = include_str!("main_keys.rs");
        let source = &source[source
            .find("fn handle_main_keyboard_input_inner")
            .expect("main keyboard handler")..];
        let primary = source
            .find("primary_shortcut_modifier(self.modifiers)")
            .expect("platform primary modifier");
        let route_match = source
            .find("let markdown_toggle = if key_event.state == ElementState::Pressed")
            .expect("central markdown toggle match");
        let file_tree_defer = source
            .find("let defer_file_tree_text_input = markdown_toggle.is_some()")
            .expect("file-tree text input deferral");
        let file_tree_modal = source
            .find("self.handle_file_tree_modal_keyboard(&key_event)")
            .expect("file-tree modal route");
        let route_apply = source
            .find("if let Some(action) = markdown_toggle")
            .expect("central markdown toggle apply");
        assert!(primary < route_match);
        assert!(route_match < file_tree_defer);
        assert!(file_tree_defer < file_tree_modal);
        assert!(file_tree_modal < route_apply);

        for marker in [
            "if self.show_settings && self.settings_tab == 0 && self.settings_ignore_focused",
            "self.handle_project_search_keyboard_input(key_event, chord);",
            "self.handle_lsp_log_filter_keyboard_input(key_event);",
            "self.handle_git_message_keyboard_input(key_event);",
            "if self.handle_api_client_keyboard_input_with_chord(&key_event, chord)",
            "self.handle_terminal_search_keyboard_input(key_event);",
            "self.handle_search_keyboard_input(key_event, chord);",
            "self.handle_terminal_keyboard_input(key_event);",
            "self.handle_editor_keyboard_input(event_loop, key_event, chord);",
        ] {
            let routed_field = source
                .find(marker)
                .unwrap_or_else(|| panic!("missing route: {marker}"));
            assert!(
                route_apply < routed_field,
                "Markdown toggle must precede {marker}"
            );
        }

        let api_route = source
            .find("if !self.show_settings && self.handle_api_client_keyboard_input_with_chord(&key_event, chord)")
            .expect("API Client keyboard owner");
        let terminal_route = source
            .find("self.handle_terminal_keyboard_input(key_event);")
            .expect("terminal keyboard owner");
        let editor_route = source
            .find("self.handle_editor_keyboard_input(event_loop, key_event, chord);")
            .expect("editor keyboard route");
        assert!(api_route < editor_route);
        assert!(api_route < terminal_route);
        assert!(terminal_route < editor_route);

        let dialog_window = source
            .find("if self.modal_dialog_open()")
            .expect("dialog route");
        assert!(dialog_window < route_apply);
        assert!(
            include_str!("../app_bootstrap.rs")
                .contains("{cmd:markdown.toggle_mode}\\tMarkdown: чтение / редактирование")
        );
    }
}
