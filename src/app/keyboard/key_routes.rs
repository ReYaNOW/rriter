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
    SettingsIgnoreField,
    SettingsEscape,
    Settings,
    SettingsToggle,
    FpsToggle,
    ProblemsToggle,
    SearchProjectOpen,
    ProjectSearchField,
    FileTreeShortcut,
    LspLogFilter,
    GitCopySelection,
    GitMessage,
    ApiClient,
    TerminalGate,
    LspLogEditor,
    TabsSwitch,
    TerminalCloseTab,
    BoundCommands,
    FinalRoute,
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
    (RouteId::SettingsIgnoreField, route_settings_ignore_field),
    (RouteId::SettingsEscape, route_settings_escape),
    (RouteId::Settings, route_settings),
    (RouteId::SettingsToggle, route_settings_toggle),
    (RouteId::FpsToggle, route_fps_toggle),
    (RouteId::ProblemsToggle, route_problems_toggle),
    (RouteId::SearchProjectOpen, route_search_project_open),
    (RouteId::ProjectSearchField, route_project_search_field),
    (RouteId::FileTreeShortcut, route_file_tree_shortcut),
    (RouteId::LspLogFilter, route_lsp_log_filter),
    (RouteId::GitCopySelection, route_git_copy_selection),
    (RouteId::GitMessage, route_git_message),
    (RouteId::ApiClient, route_api_client),
    (RouteId::TerminalGate, route_terminal_gate),
    (RouteId::LspLogEditor, route_lsp_log_editor),
    (RouteId::TabsSwitch, route_tabs_switch),
    (RouteId::TerminalCloseTab, route_terminal_close_tab),
    (RouteId::BoundCommands, route_bound_commands),
    (RouteId::FinalRoute, route_final_route),
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
            app.request_redraw();
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
                    app.request_redraw();
                }
                _ if ctx.chord.is_some_and(|chord| app.keymap.hit(crate::keymap::Command::SettingsCopyInstallerLog, chord)) => {
                    let log = app.tool_installer.full_log();
                    if !log.is_empty() {
                        app.set_clipboard_text(log);
                    }
                    app.request_redraw();
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
            app.request_redraw();
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
            app.request_redraw();
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
                    app.request_redraw();
                    return true;
                }
                PhysicalKey::Code(KeyCode::ArrowUp) => {
                    app.move_active_database_query_history_selection(-1);
                    app.request_redraw();
                    return true;
                }
                PhysicalKey::Code(KeyCode::Home) => {
                    app.set_active_database_query_history_selection(false);
                    app.request_redraw();
                    return true;
                }
                PhysicalKey::Code(KeyCode::End) => {
                    app.set_active_database_query_history_selection(true);
                    app.request_redraw();
                    return true;
                }
                PhysicalKey::Code(KeyCode::Enter) | PhysicalKey::Code(KeyCode::NumpadEnter) if !ctrl => {
                    let selected = app.active_database_query_meta_state().map_or(0, |(_, state)| state.history_selected);
                    app.load_database_query_history_entry(selected);
                    app.request_redraw();
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
                    app.request_redraw();
                    return true;
                }
                if running {
                    app.cancel_active_database_query();
                    app.request_redraw();
                    return true;
                }
                if history_open {
                    app.toggle_active_database_query_history();
                    app.request_redraw();
                    return true;
                }
            }
            _ if !app.show_settings && !terminal_owns_chord && chord.is_some_and(|chord| app.keymap.hit(crate::keymap::Command::DatabaseQueryRun, chord)) => {
                app.run_active_database_query(crate::app::database::DatabaseQueryMode::Run);
                app.request_redraw();
                return true;
            }
            _ if !app.show_settings && !terminal_owns_chord && chord.is_some_and(|chord| app.keymap.hit(crate::keymap::Command::DatabaseQueryComplete, chord)) => {
                app.show_active_database_query_completion();
                app.request_redraw();
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
    if ctx.key_event.state != ElementState::Pressed {
        return false;
    }
    let terminal_owns_chord = terminal_owns_chord(app, ctx.chord);
    if (!app.show_settings && app.handle_database_table_key(ctx.key_event, ctx.chord, terminal_owns_chord))
        || app.handle_pdf_key(ctx.key_event, ctx.chord)
        || app.handle_image_key(ctx.key_event, ctx.chord)
    {
        app.request_redraw();
        true
    } else {
        false
    }
}

fn route_database_dialog(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    if ctx.key_event.state == ElementState::Pressed && app.handle_database_dialog_keyboard(ctx.key_event) {
        app.request_redraw();
        true
    } else {
        false
    }
}

fn route_database_prompt(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    if ctx.key_event.state == ElementState::Pressed
        && (app.ide_panel.database.delete_prompt.is_some() || app.ide_panel.database.host_key_prompt.is_some())
    {
        if ctx.key_event.physical_key == PhysicalKey::Code(KeyCode::Escape) {
            app.ide_panel.database.delete_prompt = None;
            if app.ide_panel.database.host_key_prompt.is_some() {
                app.cancel_database_host_key_prompt();
            }
            app.request_redraw();
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
        app.request_redraw();
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
        app.request_redraw();
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
            app.request_redraw();
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
        app.request_redraw();
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
        app.request_redraw();
    }
    false
}

fn route_git_commit_menus(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    if ctx.key_event.state == ElementState::Pressed
        && ctx.key_event.physical_key == PhysicalKey::Code(KeyCode::Escape)
        && app.ide_panel.git.close_commit_menus()
    {
        app.request_redraw();
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
        app.request_redraw();
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
        app.request_redraw();
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

fn route_settings_ignore_field(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    let key_event = ctx.key_event;
    if key_event.state != ElementState::Pressed
        || !(app.show_settings && app.settings_tab == 0 && app.settings_ignore_focused)
    {
        return false;
    }
    app.last_action = std::time::Instant::now();
    let ctrl = crate::platform::primary_shortcut_modifier(app.modifiers);
    let word = crate::platform::word_navigation_modifier(app.modifiers);
    let shift = app.modifiers.shift_key();
    match key_event.physical_key {
        PhysicalKey::Code(KeyCode::Escape) => {
            app.settings_ignore_focused = false;
            app.request_redraw();
            true
        }
        PhysicalKey::Code(KeyCode::Enter) | PhysicalKey::Code(KeyCode::NumpadEnter) => {
            let trimmed = app.settings_ignore_editor.get_full_text().trim().to_string();
            if !trimmed.is_empty() && !app.ide_ignore_patterns.contains(&trimmed) {
                app.ide_ignore_patterns.push(trimmed);
                app.settings_ignore_editor.select_all();
                app.settings_ignore_editor.delete_selection();
                app.save_current_config();
                app.refresh_file_tree();
            }
            app.request_redraw();
            true
        }
        PhysicalKey::Code(KeyCode::KeyA) if ctrl => {
            app.settings_ignore_editor.select_all();
            app.request_redraw();
            true
        }
        PhysicalKey::Code(KeyCode::KeyC) if ctrl => {
            if let Some(text) = app.settings_ignore_editor.get_selection() {
                app.set_clipboard_text(text);
            }
            true
        }
        PhysicalKey::Code(KeyCode::KeyX) if ctrl => {
            if let Some(text) = app.settings_ignore_editor.get_selection() {
                app.set_clipboard_text(text);
                app.settings_ignore_editor.delete_selection();
                app.request_redraw();
            }
            true
        }
        PhysicalKey::Code(KeyCode::KeyV) if ctrl => {
            if let Some(text) = app.get_clipboard_text() {
                let clean = text.replace('\n', "").replace('\r', "");
                if !clean.is_empty() {
                    app.settings_ignore_editor.insert_str(&clean);
                    app.request_redraw();
                }
            }
            true
        }
        PhysicalKey::Code(KeyCode::Backspace) => {
            if word { app.settings_ignore_editor.delete_word_backward(); }
            else { app.settings_ignore_editor.backspace(); }
            app.request_redraw();
            true
        }
        PhysicalKey::Code(KeyCode::Delete) => {
            if word { app.settings_ignore_editor.delete_word_forward(); }
            else { app.settings_ignore_editor.delete_forward(); }
            app.request_redraw();
            true
        }
        PhysicalKey::Code(KeyCode::ArrowLeft) => {
            if word { app.settings_ignore_editor.move_word_left(shift); }
            else { app.settings_ignore_editor.move_left(shift); }
            app.request_redraw();
            true
        }
        PhysicalKey::Code(KeyCode::ArrowRight) => {
            if word { app.settings_ignore_editor.move_word_right(shift); }
            else { app.settings_ignore_editor.move_right(shift); }
            app.request_redraw();
            true
        }
        PhysicalKey::Code(KeyCode::Home) => {
            app.settings_ignore_editor.move_home(shift);
            app.request_redraw();
            true
        }
        PhysicalKey::Code(KeyCode::End) => {
            app.settings_ignore_editor.move_end(shift);
            app.request_redraw();
            true
        }
        _ => {
            if crate::platform::text_input_modifiers_allowed(app.modifiers) {
                if let Some(txt) = key_event.logical_text.as_deref() {
                    let clean_txt = txt.replace('\n', "");
                    if !clean_txt.is_empty() {
                        app.settings_ignore_editor.insert_str(&clean_txt);
                        app.request_redraw();
                        return true;
                    }
                }
            }
            false
        }
    }
}

fn route_settings_escape(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    if ctx.key_event.state == ElementState::Pressed
        && ctx.key_event.physical_key == PhysicalKey::Code(KeyCode::Escape)
        && app.show_settings
    {
        app.set_settings_visible(false);
        app.request_redraw();
        true
    } else {
        false
    }
}

fn route_settings(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    if ctx.key_event.state == ElementState::Pressed && app.show_settings {
        if ctx.key_event.state == ElementState::Pressed
            && ctx.chord.is_some_and(|chord| app.keymap.hit(crate::keymap::Command::SettingsToggle, chord))
        {
            app.set_settings_visible(false);
            app.request_redraw();
        }
        true
    } else {
        false
    }
}

fn route_settings_toggle(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    if ctx.key_event.state != ElementState::Pressed { return false; }
    let term_focused = app.is_ide_mode
        && app.ide_panel.terminal_focused
        && app.ide_panel.is_open(crate::app::PanelId::Terminal);
    if ctx.chord.is_some_and(|chord| app.keymap.hit(crate::keymap::Command::SettingsToggle, chord))
        && !term_focused
    {
        app.set_settings_visible(!app.show_settings);
        app.is_dragging = false;
        app.request_redraw();
        true
    } else {
        false
    }
}

fn route_fps_toggle(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    if ctx.key_event.state != ElementState::Pressed
        || !ctx.chord.is_some_and(|chord| app.keymap.hit(crate::keymap::Command::ViewToggleFps, chord))
    {
        return false;
    }
    let term_focused = app.is_ide_mode
        && app.ide_panel.terminal_focused
        && app.ide_panel.is_open(crate::app::PanelId::Terminal);
    if !term_focused {
        app.show_fps = !app.show_fps;
        app.request_redraw();
        true
    } else {
        false
    }
}

fn route_problems_toggle(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    let terminal_owns_chord = terminal_owns_chord(app, ctx.chord);
    if ctx.key_event.state == ElementState::Pressed
        && app.is_ide_mode
        && ctx.chord.is_some_and(|chord| app.keymap.hit(crate::keymap::Command::ViewToggleProblems, chord))
        && (!terminal_owns_chord || ctx.chord.is_some_and(|chord| super::input_owner::is_default_chord(crate::keymap::Command::ViewToggleProblems, chord, crate::platform::CURRENT_PLATFORM)))
    {
        apply_problems_alt_w_shortcut(&mut app.ide_panel);
        crate::save_panel_state(&app.ide_panel);
        app.last_action = std::time::Instant::now();
        app.request_redraw();
        true
    } else { false }
}

fn route_search_project_open(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    let terminal_owns_chord = terminal_owns_chord(app, ctx.chord);
    if ctx.key_event.state == ElementState::Pressed
        && app.is_ide_mode
        && ctx.chord.is_some_and(|chord| app.keymap.hit(crate::keymap::Command::SearchProjectOpen, chord))
        && (!terminal_owns_chord || ctx.chord.is_some_and(|chord| super::input_owner::is_default_chord(crate::keymap::Command::SearchProjectOpen, chord, crate::platform::CURRENT_PLATFORM)))
    {
        app.open_project_search_panel();
        app.last_action = std::time::Instant::now();
        app.request_redraw();
        true
    } else { false }
}

fn route_project_search_field(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    if ctx.key_event.state == ElementState::Pressed
        && app.is_ide_mode
        && app.ide_panel.is_open(crate::app::PanelId::Search)
        && app.ide_panel.project_search.focused.is_some()
    {
        app.handle_project_search_keyboard_input(ctx.key_event.clone(), ctx.chord);
        true
    } else { false }
}

fn route_file_tree_shortcut(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    if ctx.key_event.state != ElementState::Pressed { return false; }
    let terminal_owns_chord = terminal_owns_chord(app, ctx.chord);
    let ctrl = crate::platform::primary_shortcut_modifier(app.modifiers);
    let default_file_tree_chord = ctx.chord.is_some_and(|chord| default_file_tree_chord_for_hit(&app.keymap, chord));
    (!terminal_owns_chord || default_file_tree_chord)
        && app.handle_file_tree_shortcut_with_chord(ctx.key_event.physical_key, ctrl, ctx.chord)
}

fn route_lsp_log_filter(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    if ctx.key_event.state == ElementState::Pressed
        && app.ide_panel.is_open(crate::app::PanelId::LspServers)
        && app.ide_panel.lsp_log_filter_focused
    {
        app.handle_lsp_log_filter_keyboard_input(ctx.key_event.clone());
        true
    } else { false }
}

fn route_git_copy_selection(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    if ctx.key_event.state != ElementState::Pressed { return false; }
    let terminal_owns_chord = terminal_owns_chord(app, ctx.chord);
    if !ctx.chord.is_some_and(|chord| app.keymap.hit(crate::keymap::Command::GitCopySelection, chord))
        || (terminal_owns_chord && !ctx.chord.is_some_and(|chord| super::input_owner::is_default_chord(crate::keymap::Command::GitCopySelection, chord, crate::platform::CURRENT_PLATFORM)))
    {
        return false;
    }
    let graph_copy = app.renderer.as_ref().and_then(|renderer| renderer.selected_git_graph_tooltip_text());
    if let Some(text) = graph_copy {
        app.set_clipboard_text(text);
        if let Some(renderer) = app.renderer.as_mut() {
            renderer.git_graph_tooltip_selection_anchor = None;
            renderer.git_graph_tooltip_selection_cursor = None;
            renderer.git_graph_tooltip_selecting = false;
        }
        app.request_redraw();
        return true;
    }

    let api_keyboard_surface_visible = app.active_tab_is_api_client()
        || app.ide_panel.is_open(crate::app::PanelId::ApiClient);
    let vcs_copy = if git_logs_keyboard_copy_eligible(
        app.is_ide_mode,
        app.show_search,
        app.search_focused,
        api_keyboard_surface_visible,
        &app.ide_panel,
    ) {
        app.ide_panel.git.copy_owned_git_logs_selection()
    } else {
        None
    };
    if let Some(text) = vcs_copy {
        app.set_clipboard_text(text);
        app.request_redraw();
        true
    } else {
        false
    }
}

fn route_git_message(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    if ctx.key_event.state == ElementState::Pressed
        && app.ide_panel.git.message_focused
        && app.ide_panel.is_open(crate::app::PanelId::Git)
    {
        app.handle_git_message_keyboard_input(ctx.key_event.clone());
        true
    } else { false }
}

fn route_api_client(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    ctx.key_event.state == ElementState::Pressed
        && !app.show_settings
        && app.handle_api_client_keyboard_input_with_chord(ctx.key_event, ctx.chord)
}

fn route_terminal_gate(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    let terminal_owns_chord = terminal_owns_chord(app, ctx.chord);
    if ctx.key_event.state == ElementState::Pressed && terminal_owns_chord {
        app.handle_terminal_keyboard_input(ctx.key_event.clone());
        true
    } else { false }
}

fn route_lsp_log_editor(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    if ctx.key_event.state != ElementState::Pressed || !app.ide_panel.is_open(crate::app::PanelId::LspServers) {
        return false;
    }
    let Some(focused_name) = app.ide_panel.lsp_logs_focused.clone() else { return false; };
    let Some(ed) = app.ide_panel.lsp_log_editors.get_mut(&focused_name) else { return false; };
    let ctrl = crate::platform::primary_shortcut_modifier(app.modifiers);
    let word = crate::platform::word_navigation_modifier(app.modifiers);
    let shift = app.modifiers.shift_key();
    match ctx.key_event.physical_key {
        PhysicalKey::Code(KeyCode::KeyC) if ctrl => {
            if let Some(text) = ed.get_selection() { app.set_clipboard_text(text); }
            true
        }
        PhysicalKey::Code(KeyCode::KeyA) if ctrl => {
            ed.select_all();
            app.request_redraw();
            true
        }
        PhysicalKey::Code(KeyCode::ArrowLeft) => {
            if word { ed.move_word_left(shift); } else { ed.move_left(shift); }
            app.request_redraw();
            true
        }
        PhysicalKey::Code(KeyCode::ArrowRight) => {
            if word { ed.move_word_right(shift); } else { ed.move_right(shift); }
            app.request_redraw();
            true
        }
        PhysicalKey::Code(KeyCode::Escape) => {
            app.ide_panel.lsp_logs_focused = None;
            app.request_redraw();
            true
        }
        _ => false,
    }
}

fn route_tabs_switch(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    if ctx.key_event.state == ElementState::Pressed
        && ctx.chord.is_some_and(|chord| app.keymap.hit(crate::keymap::Command::TabsSwitchNext, chord)
            || app.keymap.hit(crate::keymap::Command::TabsSwitchPrevious, chord))
    {
        let physical_key = if ctx.chord.is_some_and(|chord| app.keymap.hit(crate::keymap::Command::TabsSwitchNext, chord)) {
            PhysicalKey::Code(KeyCode::PageDown)
        } else { PhysicalKey::Code(KeyCode::PageUp) };
        app.switch_tab_from_keyboard(physical_key);
        true
    } else { false }
}

fn route_terminal_close_tab(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    if ctx.key_event.state == ElementState::Pressed
        && is_terminal_tab_close_shortcut(&app.ide_panel)
        && ctx.chord.is_some_and(|chord| app.keymap.hit(crate::keymap::Command::TerminalCloseTab, chord))
    {
        app.close_terminal_tab_at(app.ide_panel.active_terminal);
        app.last_action = std::time::Instant::now();
        app.request_redraw();
        true
    } else { false }
}

fn route_bound_commands(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    ctx.key_event.state == ElementState::Pressed
        && app.run_bound_commands(ctx.chord, None, ctx.key_event.repeat)
}

fn route_final_route(app: &mut App, ctx: &KeyCtx<'_>) -> bool {
    if ctx.key_event.state == ElementState::Pressed {
        if app.ide_panel.is_open(crate::app::PanelId::Terminal)
            && app.ide_panel.term_show_search
            && app.ide_panel.term_search_focused
        {
            app.handle_terminal_search_keyboard_input(ctx.key_event.clone());
        } else if app.show_search && app.search_focused {
            app.handle_search_keyboard_input(ctx.key_event.clone(), ctx.chord);
        } else if super::input_owner::terminal_keyboard_owner(
            app.is_ide_mode,
            app.show_settings,
            app.show_search,
            app.search_focused,
            &app.ide_panel,
        ) && (!app.ide_panel.term_show_search || !app.ide_panel.term_search_focused)
        {
            app.handle_terminal_keyboard_input(ctx.key_event.clone());
        } else {
            app.handle_editor_keyboard_input(ctx.event_loop, ctx.key_event.clone(), ctx.chord);
        }
        return true;
    }
    false
}
