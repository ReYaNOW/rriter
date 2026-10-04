// Ordered owner table for the main keyboard route.
use super::*;
use super::main_keys::{
    apply_problems_alt_w_shortcut, apply_terminal_alt_q_shortcut,
    default_file_tree_chord_for_hit, git_logs_keyboard_copy_eligible,
    is_terminal_tab_close_shortcut, should_suppress_hover_for_keyboard,
    MarkdownGlobalToggleAction,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RouteId {
    MarkdownToc,
    HoverSuppress,
    InstallerLog,
    FileTreeModal,
    KeymapSettings,
    QueryReview,
    TerminalCloseToggle,
    ConfirmModal,
    DatabaseQuery,
    DatabaseTableCommands,
    DatabaseTablePdfImage,
    DatabaseDialog,
    DatabasePrompt,
    DatabasePopupClose,
    DatabaseDdlHover,
    ProjectSearchHelp,
    GitInlinePopup,
    HoverClear,
    GitCommitMenus,
    ApiMockConstraintMenu,
    ApiOutputExampleMenu,
    MarkdownToggle,
    Rest,
}

pub(super) struct KeyCtx<'a> {
    pub(super) event_loop: &'a HostLoop<'a>,
    pub(super) key_event: &'a KeyInput,
    pub(super) chord: Option<crate::keymap::Chord>,
    pub(super) ctrl: bool,
    pub(super) alt: bool,
    pub(super) markdown_toggle: Option<MarkdownGlobalToggleAction>,
    pub(super) defer_file_tree_text_input: bool,
}

type KeyRouteFn = fn(&mut App, &KeyCtx<'_>) -> bool;

pub(super) const KEY_ROUTES: &[(RouteId, KeyRouteFn)] = &[
    (RouteId::MarkdownToc, route_markdown_toc),
    (RouteId::HoverSuppress, route_hover_suppress),
    (RouteId::InstallerLog, route_installer_log),
    (RouteId::FileTreeModal, route_file_tree_modal),
    (RouteId::KeymapSettings, route_keymap_settings),
    (RouteId::QueryReview, route_query_review),
    (RouteId::TerminalCloseToggle, route_terminal_close_toggle),
    (RouteId::ConfirmModal, route_confirm_modal),
    (RouteId::DatabaseQuery, route_database_query),
    (RouteId::DatabaseTableCommands, route_database_table_commands),
    (RouteId::DatabaseTablePdfImage, route_database_table_pdf_image),
    (RouteId::DatabaseDialog, route_database_dialog),
    (RouteId::DatabasePrompt, route_database_prompt),
    (RouteId::DatabasePopupClose, route_database_popup_close),
    (RouteId::DatabaseDdlHover, route_database_ddl_hover),
    (RouteId::ProjectSearchHelp, route_project_search_help),
    (RouteId::GitInlinePopup, route_git_inline_popup),
    (RouteId::HoverClear, route_hover_clear),
    (RouteId::GitCommitMenus, route_git_commit_menus),
    (RouteId::ApiMockConstraintMenu, route_api_mock_constraint_menu),
    (RouteId::ApiOutputExampleMenu, route_api_output_example_menu),
    (RouteId::MarkdownToggle, route_markdown_toggle),
    (RouteId::Rest, route_rest),
];

impl App {
    pub(super) fn route_main_key(&mut self, ctx: &KeyCtx<'_>) -> Option<RouteId> {
        for (id, route) in KEY_ROUTES {
            if route(self, ctx) {
                return Some(*id);
            }
        }
        None
    }
}

#[cfg(test)]
pub(super) fn route_position(id: RouteId) -> usize {
    KEY_ROUTES.iter().position(|(route_id, _)| *route_id == id).unwrap_or(KEY_ROUTES.len())
}

fn route_markdown_toc(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    if app.markdown_toc.open {
        app.handle_markdown_toc_key(ctx.key_event.physical_key, ctx.key_event.state);
        true
    } else {
        false
    }
}

fn route_hover_suppress(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    if ctx.key_event.state == ElementState::Pressed
        && should_suppress_hover_for_keyboard(ctx.key_event.physical_key, ctx.ctrl, ctx.alt)
    {
        let renderer = app.renderer.as_mut();
        let had_hover = crate::app::mouse::suppress_hover_popup_until_mouse_move(&mut app.hover, renderer);
        if had_hover {
            if let Some(w) = app.window.as_ref() {
                w.request_redraw();
            }
        }
    }
    false
}

fn route_installer_log(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    let key_event = ctx.key_event;
    if app.show_settings && app.tool_installer.is_log_open() {
        if key_event.state == ElementState::Pressed {
            match key_event.physical_key {
                PhysicalKey::Code(KeyCode::Escape) => {
                    app.tool_installer.close_log();
                    if let Some(window) = app.window.as_ref() {
                        window.request_redraw();
                    }
                }
                _ if ctx.chord.is_some_and(|chord| app.keymap.hit(crate::keymap::Command::SettingsCopyInstallerLog, chord)) => {
                    let log = app.tool_installer.full_log();
                    if !log.is_empty() {
                        app.set_clipboard_text(log);
                    }
                    if let Some(window) = app.window.as_ref() {
                        window.request_redraw();
                    }
                }
                _ => {}
            }
        }
        true
    } else {
        false
    }
}

fn route_file_tree_modal(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    !ctx.defer_file_tree_text_input && app.handle_file_tree_modal_keyboard(ctx.key_event)
}

fn route_keymap_settings(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    app.handle_keymap_settings_key(ctx.key_event, ctx.chord)
}

fn route_query_review(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    let query_review_open = app.active_database_query_meta_state().is_some_and(|(_, state)| state.review.is_some());
    if query_review_open && !app.show_settings {
        if ctx.key_event.state == ElementState::Pressed {
            match ctx.key_event.physical_key {
                PhysicalKey::Code(KeyCode::Escape) => app.rollback_active_database_query(),
                PhysicalKey::Code(KeyCode::Enter) | PhysicalKey::Code(KeyCode::NumpadEnter) => app.commit_active_database_query(),
                _ => {}
            }
            if let Some(window) = app.window.as_ref() {
                window.request_redraw();
            }
        }
        true
    } else {
        false
    }
}

fn terminal_owns_chord(app: &App, chord: Option<crate::keymap::Chord>) -> bool {
    chord.is_some_and(|chord| super::input_owner::terminal_intercepts(chord, crate::platform::CURRENT_PLATFORM))
        && super::input_owner::terminal_keyboard_owner(
            app.is_ide_mode, app.show_settings, app.show_search, app.search_focused, &app.ide_panel,
        )
        && !(app.ide_panel.term_show_search && app.ide_panel.term_search_focused)
}

fn route_terminal_close_toggle(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    let chord = ctx.chord;
    let terminal_owns_chord = terminal_owns_chord(app, chord);
    let terminal_close = chord.is_some_and(|chord| app.keymap.hit(crate::keymap::Command::TerminalClose, chord));
    let terminal_toggle = chord.is_some_and(|chord| app.keymap.hit(crate::keymap::Command::TerminalToggleFocus, chord));
    if ctx.key_event.state == ElementState::Pressed
        && (terminal_close || terminal_toggle)
        && (!terminal_owns_chord || chord.is_some_and(|chord| {
            super::input_owner::is_default_chord(
                if terminal_close { crate::keymap::Command::TerminalClose } else { crate::keymap::Command::TerminalToggleFocus },
                chord,
                crate::platform::CURRENT_PLATFORM,
            )
        }))
    {
        if app.is_ide_mode {
            let has_terminal = !app.ide_panel.terminals.is_empty();
            let needs_terminal = apply_terminal_alt_q_shortcut(&mut app.ide_panel, terminal_close, has_terminal);
            if needs_terminal {
                app.add_terminal();
            }
            app.defer_terminal_panel_until_ready();
            app.last_action = std::time::Instant::now();
            if let Some(w) = app.window.as_ref() {
                w.request_redraw();
            }
            return true;
        }
    }
    false
}

fn route_confirm_modal(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    if app.modal_dialog_open() {
        if ctx.key_event.state == ElementState::Pressed {
            if ctx.key_event.physical_key == PhysicalKey::Code(KeyCode::Escape) {
                app.cancel_pending_action();
            } else if let Some(dw) = app.confirm_dialog.window() {
                dw.focus_window();
                dw.request_redraw();
            }
        }
        true
    } else {
        false
    }
}

fn route_database_query(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    let key_event = ctx.key_event;
    let chord = ctx.chord;
    let ctrl = ctx.ctrl;
    let terminal_owns_chord = terminal_owns_chord(app, chord);
    if key_event.state == ElementState::Pressed && app.active_tab_is_database_query() && !app.show_settings {
        let history_open = app.active_database_query_meta_state().is_some_and(|(_, state)| state.history_open);
        if history_open {
            match key_event.physical_key {
                PhysicalKey::Code(KeyCode::ArrowDown) => {
                    app.move_active_database_query_history_selection(1);
                    if let Some(window) = app.window.as_ref() { window.request_redraw(); }
                    return true;
                }
                PhysicalKey::Code(KeyCode::ArrowUp) => {
                    app.move_active_database_query_history_selection(-1);
                    if let Some(window) = app.window.as_ref() { window.request_redraw(); }
                    return true;
                }
                PhysicalKey::Code(KeyCode::Home) => {
                    app.set_active_database_query_history_selection(false);
                    if let Some(window) = app.window.as_ref() { window.request_redraw(); }
                    return true;
                }
                PhysicalKey::Code(KeyCode::End) => {
                    app.set_active_database_query_history_selection(true);
                    if let Some(window) = app.window.as_ref() { window.request_redraw(); }
                    return true;
                }
                PhysicalKey::Code(KeyCode::Enter) | PhysicalKey::Code(KeyCode::NumpadEnter) if !ctrl => {
                    let selected = app.active_database_query_meta_state().map_or(0, |(_, state)| state.history_selected);
                    app.load_database_query_history_entry(selected);
                    if let Some(window) = app.window.as_ref() { window.request_redraw(); }
                    return true;
                }
                _ => {}
            }
        }
        match key_event.physical_key {
            PhysicalKey::Code(KeyCode::Escape) => {
                let (running, reviewing, history_open) = app.active_database_query_meta_state().map_or(
                    (false, false, false),
                    |(_, state)| (state.running, state.review.is_some(), state.history_open),
                );
                if reviewing {
                    app.rollback_active_database_query();
                    if let Some(window) = app.window.as_ref() { window.request_redraw(); }
                    return true;
                }
                if running {
                    app.cancel_active_database_query();
                    if let Some(window) = app.window.as_ref() { window.request_redraw(); }
                    return true;
                }
                if history_open {
                    app.toggle_active_database_query_history();
                    if let Some(window) = app.window.as_ref() { window.request_redraw(); }
                    return true;
                }
            }
            _ if !app.show_settings && !terminal_owns_chord && chord.is_some_and(|chord| app.keymap.hit(crate::keymap::Command::DatabaseQueryRun, chord)) => {
                app.run_active_database_query(crate::app::database::DatabaseQueryMode::Run);
                if let Some(window) = app.window.as_ref() { window.request_redraw(); }
                return true;
            }
            _ if !app.show_settings && !terminal_owns_chord && chord.is_some_and(|chord| app.keymap.hit(crate::keymap::Command::DatabaseQueryComplete, chord)) => {
                app.show_active_database_query_completion();
                if let Some(window) = app.window.as_ref() { window.request_redraw(); }
                return true;
            }
            _ => {}
        }
    }
    false
}

fn route_database_table_commands(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    ctx.key_event.state == ElementState::Pressed
        && !app.show_settings
        && app.database_table_command_context_unowned()
        && app.run_bound_commands(ctx.chord, Some(crate::keymap::KeyContext::DatabaseTable), ctx.key_event.repeat)
}

fn route_database_table_pdf_image(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    let terminal_owns_chord = terminal_owns_chord(app, ctx.chord);
    if (!app.show_settings && app.handle_database_table_key(ctx.key_event, ctx.chord, terminal_owns_chord))
        || app.handle_pdf_key(ctx.key_event, ctx.chord)
        || app.handle_image_key(ctx.key_event, ctx.chord)
    {
        if let Some(window) = app.window.as_ref() { window.request_redraw(); }
        true
    } else {
        false
    }
}

fn route_database_dialog(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    if app.handle_database_dialog_keyboard(ctx.key_event) {
        if let Some(window) = app.window.as_ref() { window.request_redraw(); }
        true
    } else {
        false
    }
}

fn route_database_prompt(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    if app.ide_panel.database.delete_prompt.is_some() || app.ide_panel.database.host_key_prompt.is_some() {
        if ctx.key_event.physical_key == PhysicalKey::Code(KeyCode::Escape) {
            app.ide_panel.database.delete_prompt = None;
            if app.ide_panel.database.host_key_prompt.is_some() {
                app.cancel_database_host_key_prompt();
            }
            if let Some(window) = app.window.as_ref() { window.request_redraw(); }
        }
        true
    } else {
        false
    }
}

fn route_database_popup_close(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    if ctx.key_event.state == ElementState::Pressed
        && ctx.key_event.physical_key == PhysicalKey::Code(KeyCode::Escape)
        && (app.ide_panel.database.context_menu.take().is_some()
            || app.ide_panel.database.ddl_hover.borrow_mut().take().is_some())
    {
        if let Some(window) = app.window.as_ref() { window.request_redraw(); }
        true
    } else {
        false
    }
}

fn route_database_ddl_hover(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    let key_event = ctx.key_event;
    if key_event.state == ElementState::Pressed && app.ide_panel.database.ddl_hover.borrow().is_some() {
        match key_event.physical_key {
            PhysicalKey::Code(KeyCode::Escape) => *app.ide_panel.database.ddl_hover.borrow_mut() = None,
            PhysicalKey::Code(KeyCode::KeyA) if ctx.ctrl => {
                if let Some(state) = app.ide_panel.database.ddl_hover.borrow_mut().as_mut() {
                    state.selection_anchor = Some(0);
                    state.selection_cursor = Some(state.popup.text.len());
                }
            }
            PhysicalKey::Code(KeyCode::KeyC) if ctx.ctrl => {
                let selected = app.ide_panel.database.ddl_hover.borrow().as_ref().and_then(|state| {
                    let (a, b) = (state.selection_anchor?, state.selection_cursor?);
                    let (start, end) = (a.min(b), a.max(b));
                    state.popup.text.get(start..end).map(str::to_string)
                });
                if let Some(selected) = selected.filter(|text| !text.is_empty()) {
                    app.set_clipboard_text(selected);
                }
            }
            _ => {}
        }
        if let Some(window) = app.window.as_ref() { window.request_redraw(); }
        true
    } else {
        false
    }
}

fn route_project_search_help(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    if app.ide_panel.project_search.help_open {
        if ctx.key_event.state == ElementState::Pressed
            && ctx.key_event.physical_key == PhysicalKey::Code(KeyCode::Escape)
        {
            app.ide_panel.project_search.help_open = false;
            if let Some(w) = app.window.as_ref() { w.request_redraw(); }
        }
        true
    } else {
        false
    }
}

fn route_git_inline_popup(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    if ctx.key_event.state == ElementState::Pressed
        && ctx.key_event.physical_key == PhysicalKey::Code(KeyCode::Escape)
        && (app.inline_git_popup.take().is_some() || app.inline_git_diff_rx.take().is_some())
    {
        app.window.as_ref().unwrap().request_redraw();
        true
    } else {
        false
    }
}

fn route_hover_clear(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    if ctx.key_event.state == ElementState::Pressed
        && ctx.key_event.physical_key == PhysicalKey::Code(KeyCode::Escape)
        && crate::app::mouse::clear_hover_popup(&mut app.hover)
    {
        app.window.as_ref().unwrap().request_redraw();
    }
    false
}

fn route_git_commit_menus(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    if ctx.key_event.state == ElementState::Pressed
        && ctx.key_event.physical_key == PhysicalKey::Code(KeyCode::Escape)
        && app.ide_panel.git.close_commit_menus()
    {
        app.window.as_ref().unwrap().request_redraw();
        true
    } else {
        false
    }
}

fn route_api_mock_constraint_menu(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    if ctx.key_event.state == ElementState::Pressed
        && ctx.key_event.physical_key == PhysicalKey::Code(KeyCode::Escape)
        && app.ide_panel.api.close_api_mock_constraint_menu()
    {
        app.window.as_ref().unwrap().request_redraw();
        true
    } else {
        false
    }
}

fn route_api_output_example_menu(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    if ctx.key_event.state == ElementState::Pressed
        && ctx.key_event.physical_key == PhysicalKey::Code(KeyCode::Escape)
        && app.close_active_api_output_example_menu()
    {
        app.window.as_ref().unwrap().request_redraw();
        true
    } else {
        false
    }
}

fn route_markdown_toggle(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    if ctx.key_event.state == ElementState::Pressed {
        if let Some(action) = ctx.markdown_toggle {
            if action == MarkdownGlobalToggleAction::ToggleMode {
                app.toggle_markdown_mode();
            }
            return true;
        }
    }
    false
}

fn route_rest(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    app.route_rest(ctx)
}

impl App {
    fn route_rest(&mut self, ctx: &KeyCtx<'_>) -> bool {
        let key_event = ctx.key_event;
        let chord = ctx.chord;
        let ctrl = ctx.ctrl;
        let terminal_owns_chord = terminal_owns_chord(self, chord);
        let event_loop = ctx.event_loop;
        if key_event.state == ElementState::Pressed {
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
                        return true;
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
                        return true;
                    }
                    PhysicalKey::Code(KeyCode::KeyA) if ctrl => {
                        self.settings_ignore_editor.select_all();
                        self.window.as_ref().unwrap().request_redraw();
                        return true;
                    }
                    PhysicalKey::Code(KeyCode::KeyC) if ctrl => {
                        if let Some(text) = self.settings_ignore_editor.get_selection() {
                            self.set_clipboard_text(text);
                        }
                        return true;
                    }
                    PhysicalKey::Code(KeyCode::KeyX) if ctrl => {
                        if let Some(text) = self.settings_ignore_editor.get_selection() {
                            self.set_clipboard_text(text);
                            self.settings_ignore_editor.delete_selection();
                            self.window.as_ref().unwrap().request_redraw();
                        }
                        return true;
                    }
                    PhysicalKey::Code(KeyCode::KeyV) if ctrl => {
                        if let Some(text) = self.get_clipboard_text() {
                            let clean = text.replace('\n', "").replace('\r', "");
                            if !clean.is_empty() {
                                self.settings_ignore_editor.insert_str(&clean);
                                self.window.as_ref().unwrap().request_redraw();
                            }
                        }
                        return true;
                    }
                    PhysicalKey::Code(KeyCode::Backspace) => {
                        if word {
                            self.settings_ignore_editor.delete_word_backward();
                        } else {
                            self.settings_ignore_editor.backspace();
                        }
                        self.window.as_ref().unwrap().request_redraw();
                        return true;
                    }
                    PhysicalKey::Code(KeyCode::Delete) => {
                        if word {
                            self.settings_ignore_editor.delete_word_forward();
                        } else {
                            self.settings_ignore_editor.delete_forward();
                        }
                        self.window.as_ref().unwrap().request_redraw();
                        return true;
                    }
                    PhysicalKey::Code(KeyCode::ArrowLeft) => {
                        if word {
                            self.settings_ignore_editor.move_word_left(shift);
                        } else {
                            self.settings_ignore_editor.move_left(shift);
                        }
                        self.window.as_ref().unwrap().request_redraw();
                        return true;
                    }
                    PhysicalKey::Code(KeyCode::ArrowRight) => {
                        if word {
                            self.settings_ignore_editor.move_word_right(shift);
                        } else {
                            self.settings_ignore_editor.move_right(shift);
                        }
                        self.window.as_ref().unwrap().request_redraw();
                        return true;
                    }
                    PhysicalKey::Code(KeyCode::Home) => {
                        self.settings_ignore_editor.move_home(shift);
                        self.window.as_ref().unwrap().request_redraw();
                        return true;
                    }
                    PhysicalKey::Code(KeyCode::End) => {
                        self.settings_ignore_editor.move_end(shift);
                        self.window.as_ref().unwrap().request_redraw();
                        return true;
                    }
                    _ => {
                        if crate::platform::text_input_modifiers_allowed(self.modifiers) {
                            if let Some(txt) = key_event.logical_text.as_deref() {
                                let clean_txt = txt.replace('\n', "");
                                if !clean_txt.is_empty() {
                                    self.settings_ignore_editor.insert_str(&clean_txt);
                                    self.window.as_ref().unwrap().request_redraw();
                                    return true;
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
                    return true;
                }
            }

            if self.show_settings {
                if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::SettingsToggle, chord)) {
                    self.set_settings_visible(false);
                    self.window.as_ref().unwrap().request_redraw();
                }
                return true;
            }

            let term_focused = self.is_ide_mode
                && self.ide_panel.terminal_focused
                && self.ide_panel.is_open(crate::app::PanelId::Terminal);
            if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::SettingsToggle, chord)) && !term_focused {
                self.set_settings_visible(!self.show_settings);
                self.is_dragging = false;
                self.window.as_ref().unwrap().request_redraw();
                return true;
            }
            if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::ViewToggleFps, chord)) {
                if !term_focused {
                    self.show_fps = !self.show_fps;
                    self.window.as_ref().unwrap().request_redraw();
                    return true;
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
                return true;
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
                return true;
            }

            if self.is_ide_mode
                && self.ide_panel.is_open(crate::app::PanelId::Search)
                && self.ide_panel.project_search.focused.is_some()
            {
                self.handle_project_search_keyboard_input(key_event.clone(), chord);
                return true;
            }

            // File-tree focus is exclusive. Handle F2/Delete/clipboard shortcuts
            // before stale editor/API focus can consume the key on another OS.
            let default_file_tree_chord = chord.is_some_and(|chord| default_file_tree_chord_for_hit(&self.keymap, chord));
            if (!terminal_owns_chord || default_file_tree_chord)
                && self.handle_file_tree_shortcut_with_chord(key_event.physical_key, ctrl, chord) {
                return true;
            }

            if self.ide_panel.is_open(crate::app::PanelId::LspServers)
                && self.ide_panel.lsp_log_filter_focused
            {
                self.handle_lsp_log_filter_keyboard_input(key_event.clone());
                return true;
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
                    return true;
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
                    return true;
                }
            }

            if self.ide_panel.git.message_focused
                && self.ide_panel.is_open(crate::app::PanelId::Git)
            {
                self.handle_git_message_keyboard_input(key_event.clone());
                return true;
            }

            if !self.show_settings && self.handle_api_client_keyboard_input_with_chord(&key_event, chord) {
                return true;
            }

            if terminal_owns_chord {
                self.handle_terminal_keyboard_input(key_event.clone());
                return true;
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
                            return true;
                        }
                        PhysicalKey::Code(KeyCode::KeyA) if ctrl => {
                            ed.select_all();
                            self.window.as_ref().unwrap().request_redraw();
                            return true;
                        }
                        PhysicalKey::Code(KeyCode::ArrowLeft) => {
                            if word {
                                ed.move_word_left(shift);
                            } else {
                                ed.move_left(shift);
                            }
                            self.window.as_ref().unwrap().request_redraw();
                            return true;
                        }
                        PhysicalKey::Code(KeyCode::ArrowRight) => {
                            if word {
                                ed.move_word_right(shift);
                            } else {
                                ed.move_right(shift);
                            }
                            self.window.as_ref().unwrap().request_redraw();
                            return true;
                        }
                        PhysicalKey::Code(KeyCode::Escape) => {
                            self.ide_panel.lsp_logs_focused = None;
                            self.window.as_ref().unwrap().request_redraw();
                            return true;
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
                return true;
            }

            if is_terminal_tab_close_shortcut(&self.ide_panel)
                && chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::TerminalCloseTab, chord)) {
                self.close_terminal_tab_at(self.ide_panel.active_terminal);
                self.last_action = std::time::Instant::now();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
                return true;
            }

            if self.run_bound_commands(chord, None, key_event.repeat) {
                return true;
            }

            if self.ide_panel.is_open(crate::app::PanelId::Terminal)
                && self.ide_panel.term_show_search
                && self.ide_panel.term_search_focused
            {
                self.handle_terminal_search_keyboard_input(key_event.clone());
            } else if self.show_search && self.search_focused {
                self.handle_search_keyboard_input(key_event.clone(), chord);
            } else if super::input_owner::terminal_keyboard_owner(
                self.is_ide_mode,
                self.show_settings,
                self.show_search,
                self.search_focused,
                &self.ide_panel,
            ) && (!self.ide_panel.term_show_search || !self.ide_panel.term_search_focused)
            {
                self.handle_terminal_keyboard_input(key_event.clone());
            } else {
                self.handle_editor_keyboard_input(event_loop, key_event.clone(), chord);
            }
        }
        false
    }
}
