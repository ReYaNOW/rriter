// Press routing for popups, menus and modal overlays that capture or dismiss
// a click before regular UI dispatch, plus the settings / dialog modals.
use super::*;

impl App {
    /// Left-press bookkeeping: git-logs copy owner, scroll anims, database dialog scroll drag, focus drops. Returns true when the event is consumed.
    #[cfg_attr(coverage_nightly, coverage(off))]
    #[inline]
    pub(super) fn prepare_left_press(
        &mut self,
        state: ElementState,
        button: winit::event::MouseButton,
        mx: f32,
        my: f32,
    ) -> bool {
        if state == ElementState::Pressed && button == winit::event::MouseButton::Left {
            let clicked_id = self.ui_registry.find_at(mx, my);
            update_git_logs_copy_owner_on_left_press(&mut self.ide_panel.git, clicked_id);
            let preserve_main_vertical = preserve_main_vertical_scroll_for_click(self, mx, my);
            stop_click_scroll_anims(self, preserve_main_vertical);
            if self.ide_panel.database.dialog.is_some() {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.suppress_database_dialog_tooltip_after_click();
                }
                if self.start_database_dialog_scroll_drag(mx, my) {
                    self.window.as_ref().unwrap().request_redraw();
                    return true;
                }
            }
            if clicked_id
                .and_then(crate::app::project_search_app::project_search_field_for_ui_id)
                .is_none()
            {
                self.ide_panel.project_search.focused = None;
            }
            if self.ide_panel.api.mock_contract_constraint_menu.is_some() {
                let clicked_id = self.ui_registry.find_at(mx, my);
                if !self.ide_panel.api.api_mock_constraint_menu_contains_ui_id(clicked_id) {
                    self.ide_panel.api.close_api_mock_constraint_menu();
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                }
            }
        }
        false
    }

    /// Clears the hover popup when a press lands outside it.
    #[cfg_attr(coverage_nightly, coverage(off))]
    #[inline]
    pub(super) fn dismiss_hover_popup_on_press(
        &mut self,
        state: ElementState,
        mx: f32,
        my: f32,
    ) {
        if state == ElementState::Pressed {
            let in_hover_popup = self
                .hover
                .popup_or_bridge_contains(
                    mx,
                    my,
                    self.renderer.as_ref().unwrap().width,
                    self.renderer.as_ref().unwrap().scale_factor,
                )
                .0;

            if !in_hover_popup && clear_hover_popup(&mut self.hover) {
                self.window.as_ref().unwrap().request_redraw();
            }
        }
    }

    /// Database DDL hover popup: scrollbar drag, text selection, dismissal. Returns true when the event is consumed.
    #[cfg_attr(coverage_nightly, coverage(off))]
    #[inline]
    pub(super) fn handle_database_ddl_popup_click(
        &mut self,
        state: ElementState,
        button: winit::event::MouseButton,
        mx: f32,
        my: f32,
    ) -> bool {
        if button == winit::event::MouseButton::Left {
            if let Some(rect) = database_ddl_captures_left_click(self) {
                let inside =
                    mx >= rect.0 && mx <= rect.0 + rect.2 && my >= rect.1 && my <= rect.1 + rect.3;
                if state == ElementState::Pressed {
                    if inside {
                        let clicked_id = self.ui_registry.find_at(mx, my);
                        let scale = self
                            .renderer
                            .as_ref()
                            .map_or(1.0, |renderer| renderer.scale_factor);
                        if let Ok(mut ddl) = self.ide_panel.database.ddl_hover.try_borrow_mut()
                            && let Some(ddl) = ddl.as_mut()
                        {
                            if clicked_id == Some(crate::ui_system::UiId::DatabaseDdlScroll) {
                                let geometry = crate::app::mouse::hover_popup_scrollbar(
                                    rect,
                                    ddl.max_scroll,
                                    ddl.popup.scroll.current,
                                    scale,
                                )
                                .geometry(scale);
                                if crate::app::mouse::press_scrollbar(
                                    &mut ddl.popup.scroll,
                                    geometry,
                                    mx,
                                    my,
                                )
                                .is_some()
                                {
                                    ddl.selecting = false;
                                }
                            } else {
                                let byte = crate::app::mouse::hover_popup_byte_at(
                                    self.renderer.as_mut().unwrap(),
                                    &ddl.popup,
                                    rect,
                                    mx,
                                    my,
                                );
                                ddl.selection_anchor = Some(byte);
                                ddl.selection_cursor = Some(byte);
                                ddl.selecting = true;
                            }
                        }
                    } else {
                        *self.ide_panel.database.ddl_hover.borrow_mut() = None;
                    }
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                    return true;
                }
                if state == ElementState::Released {
                    if let Ok(mut ddl) = self.ide_panel.database.ddl_hover.try_borrow_mut()
                        && let Some(ddl) = ddl.as_mut()
                    {
                        ddl.selecting = false;
                        ddl.popup.scroll.end_drag();
                    }
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                    return true;
                }
            }
        }
        false
    }

    /// Python runtime overlay, project search help, database modals / context menu, file-tree overlay. Returns true when the event is consumed.
    #[cfg_attr(coverage_nightly, coverage(off))]
    #[inline]
    pub(super) fn handle_modal_overlay_press(
        &mut self,
        state: ElementState,
        button: winit::event::MouseButton,
        mx: f32,
        my: f32,
    ) -> bool {
        if state == ElementState::Pressed && self.ide_panel.api.api_python_runtime_overlay_active() {
            if button == winit::event::MouseButton::Left {
                let clicked_id = self.ui_registry.find_overlay_at(mx, my);
                if let Some(clicked_id) = clicked_id
                    && crate::app::api_client::ApiClientState::ui_id_is_api_python_runtime_overlay(
                        clicked_id,
                    )
                {
                    self.handle_ui_click(clicked_id);
                }
                if !matches!(
                    clicked_id,
                    Some(
                        crate::ui_system::UiId::ApiMockPythonUvPathInput
                            | crate::ui_system::UiId::ApiMockPythonCustomPathInput
                    )
                ) && self.ide_panel.api.focused.as_ref().is_some_and(|focus| {
                    matches!(
                        focus,
                        crate::app::api_client::ApiFocus::MockPythonUvPath
                            | crate::app::api_client::ApiFocus::MockPythonCustomPath
                    )
                }) {
                    self.commit_api_focus();
                    self.ide_panel.api.focused = None;
                }
            }
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }

        if state == ElementState::Pressed && project_search_help_captures_pressed_click(self) {
            if button == winit::event::MouseButton::Left {
                match self.ui_registry.find_overlay_at(mx, my) {
                    Some(crate::ui_system::UiId::ProjectSearchHelp) => {
                        self.handle_ui_click(crate::ui_system::UiId::ProjectSearchHelp);
                    }
                    Some(crate::ui_system::UiId::ProjectSearchHelpPopup) => {}
                    _ => {
                        self.ide_panel.project_search.help_open = false;
                    }
                }
            }
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }

        if state == ElementState::Pressed && self.database_blocking_modal_open() {
            if button == winit::event::MouseButton::Left
                && let Some(clicked_id) = self.ui_registry.find_overlay_at(mx, my)
            {
                self.handle_ui_click(clicked_id);
            }
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            return true;
        }

        if state == ElementState::Pressed && self.ide_panel.database.context_menu.is_some() {
            let clicked_id = self.ui_registry.find_overlay_at(mx, my);
            let keep = matches!(
                clicked_id,
                Some(crate::ui_system::UiId::DatabaseContextItem(_))
            );
            if button == winit::event::MouseButton::Left && keep {
                if let Some(clicked_id) = clicked_id {
                    self.handle_ui_click(clicked_id);
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
                return true;
            }
            if !keep {
                self.ide_panel.database.context_menu = None;
                if button != winit::event::MouseButton::Left {
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                    return true;
                }
            }
        }

        if state == ElementState::Pressed && self.file_tree_overlay_active() {
            match button {
                winit::event::MouseButton::Left => {
                    if let Some(clicked_id) = self.ui_registry.find_at(mx, my) {
                        if crate::app::App::ui_id_is_file_tree_overlay(clicked_id) {
                            self.handle_ui_click(clicked_id);
                        } else if self.ide_panel.file_tree_context_menu.is_some() {
                            self.ide_panel.file_tree_context_menu = None;
                        }
                    } else if self.ide_panel.file_tree_context_menu.is_some() {
                        self.ide_panel.file_tree_context_menu = None;
                    }
                }
                winit::event::MouseButton::Right
                    if self.ide_panel.file_tree_context_menu.is_some() =>
                {
                    self.ide_panel.file_tree_context_menu = None;
                }
                _ => {}
            }
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }
        false
    }

    /// Git menu dismissal, autocomplete non-left press, right-click context menus. Returns true when the event is consumed.
    #[cfg_attr(coverage_nightly, coverage(off))]
    #[inline]
    pub(super) fn handle_menu_dismiss_press(
        &mut self,
        state: ElementState,
        button: winit::event::MouseButton,
        mx: f32,
        my: f32,
    ) -> bool {
        if state == ElementState::Pressed
            && (self.ide_panel.git.commit_menu_open()
                || self.ide_panel.git.commit_options_menu_open())
        {
            let clicked_id = self.ui_registry.find_at(mx, my);
            let keep_git_menu = matches!(
                clicked_id,
                Some(
                    crate::ui_system::UiId::GitCommitMenuToggle
                        | crate::ui_system::UiId::GitCommitMenuItem(_)
                        | crate::ui_system::UiId::GitCommitOptionsToggle
                        | crate::ui_system::UiId::GitCommitOptionsItem(_)
                )
            );
            if !keep_git_menu {
                self.ide_panel.git.close_commit_menus();
                if clicked_id.is_none() {
                    self.window.as_ref().unwrap().request_redraw();
                    return true;
                }
            }
        }

        if state == ElementState::Pressed
            && self.ide_panel.git.repo_action_menu_workspace_idx.is_some()
        {
            let clicked_id = self.ui_registry.find_at(mx, my);
            let keep_git_menu = matches!(
                clicked_id,
                Some(
                    crate::ui_system::UiId::GitRepoActionMenu(_)
                        | crate::ui_system::UiId::GitFetch(_)
                        | crate::ui_system::UiId::GitPull(_)
                )
            );
            if !keep_git_menu {
                self.ide_panel.git.close_repo_action_menu();
                if clicked_id.is_none() {
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                    return true;
                }
            }
        }

        if state == ElementState::Pressed
            && button != winit::event::MouseButton::Left
            && self.autocomplete_window_contains(mx, my)
        {
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }

        if state == ElementState::Pressed
            && button == winit::event::MouseButton::Right
            && let Some(id) = self.ui_registry.find_at(mx, my)
            && self.open_database_context_menu_for_hit(id, mx, my)
        {
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            return true;
        }

        if state == ElementState::Pressed
            && button == winit::event::MouseButton::Right
            && let Some(id) = self.ui_registry.find_at(mx, my)
            && self.open_tab_context_menu_for_hit(id, mx, my)
        {
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }

        if state == ElementState::Pressed
            && button == winit::event::MouseButton::Right
            && self.file_tree_panel_contains(mx, my)
        {
            self.open_file_tree_context_menu(mx, my);
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }
        false
    }

    /// Modal dialog window focus, tool-installer log and settings modal clicks. Returns true when the event is consumed.
    #[cfg_attr(coverage_nightly, coverage(off))]
    #[inline]
    pub(super) fn handle_modal_window_and_settings_click(
        &mut self,
        state: ElementState,
    ) -> bool {
        if self.modal_dialog_open() {
            if state == ElementState::Pressed {
                if let Some(dw) = self.confirm_dialog.window() {
                    dw.focus_window();
                    dw.request_redraw();
                }
            }
            return true;
        }

        if self.show_settings && self.tool_installer.is_log_open() {
            if state == ElementState::Pressed
                && let Some(renderer) = self.renderer.as_ref()
            {
                let mx = renderer.last_mouse_x;
                let my = renderer.last_mouse_y;
                if let Some(clicked_id) = self.ui_registry.find_overlay_at(mx, my) {
                    self.handle_ui_click(clicked_id);
                }
            }
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            return true;
        }

        if self.show_settings {
            if state == ElementState::Released {
                self.is_dragging_settings_ignore = false;
                self.is_dragging_lsp_log = false;
                self.settings_scroll.end_drag();
                self.settings_ide_scroll.end_drag();
                self.settings_general_scroll.end_drag();
                self.settings_database_scroll.end_drag();
                self.keymap_settings.scroll.end_drag();
            } else if state == ElementState::Pressed {
                let s = self.renderer.as_ref().unwrap().scale_factor;
                let window_size = self.window.as_ref().unwrap().inner_size();
                let layout = crate::render_view::settings_ui::animated_settings_modal_layout(
                    window_size.width as f32,
                    window_size.height as f32,
                    s,
                    self.settings_anim_progress,
                );
                let outer = layout.outer;

                let mx = self.renderer.as_ref().unwrap().last_mouse_x;
                let my = self.renderer.as_ref().unwrap().last_mouse_y;

                if !outer.contains(mx, my) {
                    self.keymap_settings.cancel_recording();
                    self.set_settings_visible(false);
                } else {
                    // Ищем только среди оверлейных элементов настроек,
                    // чтобы фоновые элементы редактора не реагировали на клики.
                    if let Some(clicked_id) = self.ui_registry.find_overlay_at(mx, my) {
                        match clicked_id {
                            crate::ui_system::UiId::SettingsIdeIgnoreInput => {
                                // Специальная обработка: позиционирование курсора по клику
                                self.settings_ignore_focused = true;
                                self.is_dragging_settings_ignore = true;
                                let input =
                                    crate::render_view::settings_ui::settings_ignore_input_rect(
                                        layout,
                                        s,
                                        self.ide_workspaces.len(),
                                        self.settings_ide_scroll.current,
                                    );
                                let text = self.settings_ignore_editor.get_full_text();
                                let x_offset = (mx - (input.x + 8.0 * s)
                                    + self.settings_ignore_scroll_x)
                                    .max(0.0);
                                let target_idx = self
                                    .renderer
                                    .as_mut()
                                    .unwrap()
                                    .one_line_cursor_from_x(&text, x_offset, 0.95);
                                self.settings_ignore_editor.cursor = target_idx;
                                self.settings_ignore_editor.selection_anchor = Some(target_idx);
                            }
                            other => {
                                // Снимаем фокус с поля ввода при клике в другое место
                                self.settings_ignore_focused = false;
                                self.handle_ui_click(other);
                            }
                        }
                    } else {
                        // Клик мимо любого элемента — снимаем фокус
                        self.settings_ignore_focused = false;
                        if self.settings_tab == 6 {
                            self.keymap_settings.cancel_recording();
                        }
                    }
                }
            }
            self.window.as_ref().unwrap().request_redraw();
            return true;
        }
        false
    }
}
