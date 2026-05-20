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
            panels.open_terminal_exclusive();
            !has_terminal
        }
    } else if !is_open {
        panels.open_terminal_exclusive();
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

impl App {
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn handle_main_keyboard_input(
        &mut self,
        event_loop: &ActiveEventLoop,
        key_event: KeyEvent,
    ) {
        let ctrl = self.modifiers.control_key() || self.modifiers.super_key();
        let alt = self.modifiers.alt_key();

        if key_event.state == ElementState::Pressed
            && should_suppress_hover_for_keyboard(key_event.physical_key, ctrl, alt)
        {
            let had_hover =
                crate::app::mouse::suppress_hover_popup_until_mouse_move(self.renderer.as_mut());
            if had_hover {
                if let Some(w) = self.window.as_ref() {
                    w.request_redraw();
                }
            }
        }

        if self.handle_file_tree_modal_keyboard(&key_event) {
            return;
        }

        if key_event.state == ElementState::Pressed
            && alt
            && key_event.physical_key == PhysicalKey::Code(KeyCode::KeyQ)
        {
            if self.is_ide_mode {
                let has_terminal = !self.ide_panel.terminals.is_empty();
                let needs_terminal = apply_terminal_alt_q_shortcut(
                    &mut self.ide_panel,
                    self.modifiers.shift_key(),
                    has_terminal,
                );
                if needs_terminal {
                    self.ide_panel
                        .terminals
                        .push(crate::app::terminal::Terminal::spawn(self.window.clone()));
                    self.ide_panel.active_terminal = 0;
                }

                self.last_action = std::time::Instant::now();
                if let Some(w) = self.window.as_ref() {
                    w.request_redraw();
                }
                return;
            }
        }

        if self.dialog_window.is_some() {
            if key_event.state == ElementState::Pressed {
                if key_event.physical_key == PhysicalKey::Code(KeyCode::Escape) {
                    self.close_dialog();
                } else {
                    if let Some(dw) = self.dialog_window.as_ref() {
                        dw.focus_window();
                        dw.request_redraw();
                    }
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
                && crate::app::mouse::clear_hover_popup(self.renderer.as_mut())
            {
                self.window.as_ref().unwrap().request_redraw();
            }
            if key_event.physical_key == PhysicalKey::Code(KeyCode::Escape)
                && self.ide_panel.git.commit_menu_open
            {
                self.ide_panel.git.commit_menu_open = false;
                self.window.as_ref().unwrap().request_redraw();
                return;
            }
            // ── Ввод в поле игнора настроек ──────────────────────────────
            if self.show_settings && self.settings_tab == 0 && self.settings_ignore_focused {
                self.last_action = std::time::Instant::now();
                let ctrl = self.modifiers.control_key();
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
                            let w = self.window.as_ref().unwrap();
                            let maximized = w.is_maximized();
                            crate::save_config(&crate::Config {
                                window_width: self.window_width,
                                window_height: self.window_height,
                                maximized,
                                ide_workspaces: self.ide_workspaces.clone(),
                                ide_ignore_patterns: self.ide_ignore_patterns.clone(),
                                enable_telemetry: crate::render_view::TELEMETRY_ENABLED
                                    .load(std::sync::atomic::Ordering::Relaxed),
                            });
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
                        if ctrl {
                            self.settings_ignore_editor.delete_word_backward();
                        } else {
                            self.settings_ignore_editor.backspace();
                        }
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                    PhysicalKey::Code(KeyCode::Delete) => {
                        if ctrl {
                            self.settings_ignore_editor.delete_word_forward();
                        } else {
                            self.settings_ignore_editor.delete_forward();
                        }
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                    PhysicalKey::Code(KeyCode::ArrowLeft) => {
                        if ctrl {
                            self.settings_ignore_editor.move_word_left(shift);
                        } else {
                            self.settings_ignore_editor.move_left(shift);
                        }
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                    PhysicalKey::Code(KeyCode::ArrowRight) => {
                        if ctrl {
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
                        if !ctrl && !self.modifiers.alt_key() && !self.modifiers.super_key() {
                            if let Some(txt) = key_event.logical_key.to_text() {
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
                    self.show_settings = false;
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
            }

            if self.show_settings {
                if let PhysicalKey::Code(KeyCode::F1) = key_event.physical_key {
                    self.show_settings = false;
                    self.window.as_ref().unwrap().request_redraw();
                }
                return;
            }

            let term_focused = self.is_ide_mode
                && self.ide_panel.terminal_focused
                && self.ide_panel.is_open(crate::app::PanelId::Terminal);
            if let PhysicalKey::Code(KeyCode::F8) = key_event.physical_key {
                if !term_focused {
                    self.show_fps = !self.show_fps;
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
            }

            if self.is_ide_mode && alt && key_event.physical_key == PhysicalKey::Code(KeyCode::KeyW)
            {
                apply_problems_alt_w_shortcut(&mut self.ide_panel);
                crate::save_panel_state(&self.ide_panel);
                self.last_action = std::time::Instant::now();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
                return;
            }

            if self.ide_panel.lsp_log_filter_focused {
                self.handle_lsp_log_filter_keyboard_input(key_event);
                return;
            }

            if ctrl && key_event.physical_key == PhysicalKey::Code(KeyCode::KeyC) {
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
            }

            if self.ide_panel.git.message_focused
                && self.ide_panel.is_open(crate::app::PanelId::Git)
            {
                self.handle_git_message_keyboard_input(key_event);
                return;
            }

            if let Some(focused_name) = self.ide_panel.lsp_logs_focused.clone() {
                if let Some(ed) = self.ide_panel.lsp_log_editors.get_mut(&focused_name) {
                    let ctrl = self.modifiers.control_key() || self.modifiers.super_key();
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
                            if ctrl {
                                ed.move_word_left(shift);
                            } else {
                                ed.move_left(shift);
                            }
                            self.window.as_ref().unwrap().request_redraw();
                            return;
                        }
                        PhysicalKey::Code(KeyCode::ArrowRight) => {
                            if ctrl {
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

            if self.ide_panel.term_show_search && self.ide_panel.term_search_focused {
                self.handle_terminal_search_keyboard_input(key_event);
            } else if self.show_search && self.search_focused {
                self.handle_search_keyboard_input(key_event);
            } else if self.is_ide_mode
                && self.ide_panel.terminal_focused
                && self.ide_panel.is_open(crate::app::PanelId::Terminal)
            {
                self.handle_terminal_keyboard_input(key_event);
            } else if self.handle_file_tree_shortcut(key_event.physical_key, ctrl) {
                return;
            } else {
                self.handle_editor_keyboard_input(event_loop, key_event);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terminal_open(panels: &crate::app::IdePanelState) -> bool {
        panels.is_open(crate::app::PanelId::Terminal)
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
}
