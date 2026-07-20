use super::*;

type Rect = (f32, f32, f32, f32);

#[cfg(test)]
fn union_rect(a: Option<Rect>, b: Option<Rect>) -> Option<Rect> {
    match (a, b) {
        (None, None) => None,
        (Some(r), None) | (None, Some(r)) => Some(r),
        (Some(r1), Some(r2)) => {
            let x_min = r1.0.min(r2.0);
            let y_min = r1.1.min(r2.1);
            let x_max = (r1.0 + r1.2).max(r2.0 + r2.2);
            let y_max = (r1.1 + r1.3).max(r2.1 + r2.3);
            Some((x_min, y_min, x_max - x_min, y_max - y_min))
        }
    }
}

#[cfg(test)]
fn point_in_padded_rect(mx: f32, my: f32, rect: Rect, pad: f32) -> bool {
    mx >= rect.0 - pad
        && mx <= rect.0 + rect.2 + pad
        && my >= rect.1 - pad
        && my <= rect.1 + rect.3 + pad
}

fn terminal_mouse_button_code(button: winit::event::MouseButton) -> u8 {
    match button {
        winit::event::MouseButton::Left => 0,
        winit::event::MouseButton::Middle => 1,
        winit::event::MouseButton::Right => 2,
        _ => 0,
    }
}

fn terminal_mouse_cell_x(mx: f32, panel_x: f32, char_w: f32) -> usize {
    ((mx - panel_x).max(0.0) / char_w).floor() as usize + 1
}

fn terminal_mouse_cell_y(
    my: f32,
    term_content_y: f32,
    term_content_h: f32,
    scroll_offset: f32,
    char_h: f32,
    scale: f32,
    visible_rows: usize,
) -> usize {
    let (_, bottom_pad) = crate::render_view::terminal_ui::terminal_text_padding(scale);
    let offset_from_bottom =
        (term_content_y + term_content_h - bottom_pad - my + scroll_offset) / char_h;
    visible_rows
        .saturating_sub(1)
        .saturating_sub(offset_from_bottom.max(0.0).floor() as usize)
        + 1
}

fn terminal_mouse_sgr_sequence(
    btn_code: u8,
    cell_x: usize,
    cell_y: usize,
    is_pressed: bool,
) -> String {
    let end_char = if is_pressed { 'M' } else { 'm' };
    format!("\x1b[<{};{};{}{}", btn_code, cell_x, cell_y, end_char)
}

fn autocomplete_item_index_at(
    px: f32,
    py: f32,
    rect: Rect,
    current_scroll: f32,
    total_items: usize,
    scale: f32,
) -> Option<usize> {
    let (rx, ry, rw, rh) = rect;
    if px < rx || px > rx + rw || py < ry || py > ry + rh {
        return None;
    }
    if px >= rx + rw - 14.0 * scale {
        return None;
    }
    let content_y = py - ry + current_scroll;
    if content_y < 0.0 {
        return None;
    }
    let idx = (content_y / (36.0 * scale)) as usize;
    (idx < total_items).then_some(idx)
}

fn stop_scroll_anim(scroll: &mut crate::scroll::ScrollState) {
    if !scroll.is_dragging {
        scroll.stop_anim();
    }
}

fn stop_api_tab_scroll_anims(state: &mut crate::app::api_client::ApiClientTabState) {
    stop_scroll_anim(&mut state.output_schema_menu_scroll);
    stop_scroll_anim(&mut state.tab_scroll);
    stop_scroll_anim(&mut state.body_scroll);
    stop_scroll_anim(&mut state.body_scroll_x);
    stop_scroll_anim(&mut state.output_scroll);
    stop_scroll_anim(&mut state.output_scroll_x);
    stop_scroll_anim(&mut state.mock_static_response_scroll);
    stop_scroll_anim(&mut state.mock_static_response_scroll_x);
    stop_scroll_anim(&mut state.response_scroll);
    stop_scroll_anim(&mut state.response_scroll_x);
}

pub(crate) fn stop_click_scroll_anims(app: &mut App) {
    stop_scroll_anim(&mut app.settings_scroll);
    stop_scroll_anim(&mut app.tab_scroll);
    stop_scroll_anim(&mut app.scroll_y);
    stop_scroll_anim(&mut app.scroll_x);
    stop_scroll_anim(&mut app.autocomplete_scroll);
    stop_scroll_anim(&mut app.settings_ide_scroll);
    if let Some(popup) = &mut app.autocomplete_detail_popup {
        stop_scroll_anim(&mut popup.scroll);
    }

    stop_scroll_anim(&mut app.ide_panel.explorer_scroll);
    if let Some(dialog) = app.ide_panel.file_tree_rename_dialog.as_mut() {
        stop_scroll_anim(&mut dialog.input_scroll_x);
    }
    stop_scroll_anim(&mut app.ide_panel.project_search.scroll);
    stop_scroll_anim(&mut app.ide_panel.project_search.query_scroll_y);
    stop_scroll_anim(&mut app.ide_panel.project_search.query_scroll_x);
    stop_scroll_anim(&mut app.ide_panel.git.scroll);
    stop_scroll_anim(&mut app.ide_panel.git.graph_scroll);
    stop_scroll_anim(&mut app.ide_panel.database.scroll);
    if let Some(dialog) = app.ide_panel.database.dialog.as_mut() {
        stop_scroll_anim(&mut dialog.scroll);
    }
    if let Ok(mut ddl) = app.ide_panel.database.ddl_hover.try_borrow_mut()
        && let Some(state) = ddl.as_mut()
    {
        stop_scroll_anim(&mut state.popup.scroll);
    }
    app.stop_database_table_modal_scroll_anims();
    stop_scroll_anim(&mut app.ide_panel.lsp_scroll_y);
    stop_scroll_anim(&mut app.ide_panel.lsp_scroll_x);
    for scroll in app.ide_panel.lsp_logs_scroll_y.values_mut() {
        stop_scroll_anim(scroll);
    }
    for scroll in app.ide_panel.lsp_logs_scroll_x.values_mut() {
        stop_scroll_anim(scroll);
    }
    stop_scroll_anim(&mut app.ide_panel.problems_scroll);
    for terminal in &mut app.ide_panel.terminals {
        stop_scroll_anim(&mut terminal.scroll_y);
    }
    app.tool_installer.stop_log_scroll_anim();

    let api = &mut app.ide_panel.api;
    stop_scroll_anim(&mut api.panel_scroll);
    stop_scroll_anim(&mut api.route_scroll);
    stop_scroll_anim(&mut api.input_scroll_x);
    stop_scroll_anim(&mut api.mock_guide_scroll);
    stop_scroll_anim(&mut api.mock_server_log_scroll);
    stop_scroll_anim(&mut api.mock_python_versions_scroll);
    stop_scroll_anim(&mut api.mock_python_install_log_scroll);
    for scroll in api.mock_python_scrolls.values_mut() {
        stop_scroll_anim(scroll);
    }
    for scroll in api.mock_python_scrolls_x.values_mut() {
        stop_scroll_anim(scroll);
    }

    for tab in &mut app.tabs {
        match &mut tab.kind {
            crate::app::EditorTabKind::ApiClient(_, state) => stop_api_tab_scroll_anims(state),
            crate::app::EditorTabKind::DatabaseTable(_, state) => {
                stop_scroll_anim(&mut state.grid.scroll_x);
                stop_scroll_anim(&mut state.grid.scroll_y);
            }
            crate::app::EditorTabKind::DatabaseQuery(_, state) => {
                stop_scroll_anim(&mut state.result_view.scroll_x);
                stop_scroll_anim(&mut state.result_view.scroll_y);
                stop_scroll_anim(&mut state.result_view.review_message_scroll_y);
            }
            crate::app::EditorTabKind::Normal | crate::app::EditorTabKind::GitDiff(_, _) => {}
        }
    }

    crate::app::mouse::HOVER_STATE.with(|state| {
        let mut state = state.borrow_mut();
        stop_scroll_anim(&mut state.diag_scroll);
        if let Some(popup) = state.popup.as_mut() {
            stop_scroll_anim(&mut popup.scroll);
        }
    });
}

fn autocomplete_scroll_click_target(
    mouse_y: f32,
    rect_y: f32,
    rect_h: f32,
    current_scroll: f32,
    total_items: usize,
    scale: f32,
) -> Option<(f32, f32)> {
    let step = 36.0 * scale;
    let total_items = total_items as f32;
    let visible_items = total_items.min(7.0);
    let total_h = total_items * step;
    if total_h <= rect_h {
        return None;
    }

    let max_scroll = ((total_items - visible_items) * step).max(0.0);
    let scroll_ratio = (current_scroll / max_scroll.max(1.0)).clamp(0.0, 1.0);
    let track_margin = 3.0 * scale;
    let track_h = (rect_h - track_margin * 2.0).max(1.0);
    let thumb_h = (rect_h / total_h * track_h).max(20.0 * scale);
    let thumb_start_y = rect_y + track_margin + scroll_ratio * (track_h - thumb_h);

    if mouse_y >= thumb_start_y && mouse_y <= thumb_start_y + thumb_h {
        Some((mouse_y - thumb_start_y, current_scroll))
    } else {
        let drag_offset = thumb_h / 2.0;
        let new_ratio =
            (mouse_y - rect_y - track_margin - drag_offset) / (track_h - thumb_h).max(1.0);
        Some((drag_offset, (new_ratio * max_scroll).clamp(0.0, max_scroll)))
    }
}

pub(super) fn apply_autocomplete_scroll_drag(
    scroll: &mut crate::scroll::ScrollState,
    target: f32,
    drag_offset: f32,
) {
    scroll.jump_to(target);
    scroll.drag_offset = drag_offset;
    scroll.anim_speed = 15.0;
    scroll.is_dragging = true;
}

impl App {
    pub(crate) fn cancel_pointer_interactions(&mut self) {
        self.finish_database_table_drag();
        self.is_dragging = false;
        self.is_editor_drag_pending = false;
        self.is_dragging_search = false;
        self.is_dragging_settings_ignore = false;
        self.is_dragging_lsp_log = false;
        self.autocomplete_detail_selecting = false;
        self.ide_panel.is_dragging_terminal = false;
        self.ide_panel.is_resizing_left = false;
        self.ide_panel.is_resizing_bottom = false;
        self.ide_panel.git.graph_resizing = false;
        self.ide_panel.file_tree_drag = None;
        self.ide_panel.tab_drag = None;
        self.ide_panel.drag = None;
        self.ide_panel.project_search.dragging_field = None;
        self.ide_panel.file_tree_dialog_input_drag = None;
        if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
            dialog.dragging_field = None;
            dialog.scroll.end_drag();
        }
        self.ide_panel.database.table_modal_input_dragging = false;

        self.scroll_y.end_drag();
        self.scroll_x.end_drag();
        self.settings_scroll.end_drag();
        self.settings_ide_scroll.end_drag();
        self.autocomplete_scroll.end_drag();
        self.ide_panel.explorer_scroll.end_drag();
        self.ide_panel.project_search.scroll.end_drag();
        self.ide_panel.project_search.query_scroll_y.end_drag();
        self.ide_panel.project_search.query_scroll_x.end_drag();
        self.ide_panel.lsp_scroll_x.end_drag();
        self.ide_panel.lsp_scroll_y.end_drag();
        self.ide_panel.api.mock_guide_scroll.end_drag();
        self.ide_panel.api.mock_server_log_scroll.end_drag();
        self.ide_panel.api.mock_python_install_log_scroll.end_drag();
        self.ide_panel.problems_scroll.end_drag();
        self.ide_panel.git.graph_scroll.end_drag();
        for scroll in self.ide_panel.lsp_logs_scroll_y.values_mut() {
            scroll.end_drag();
        }
        for scroll in self.ide_panel.lsp_logs_scroll_x.values_mut() {
            scroll.end_drag();
        }
        for scroll in self.ide_panel.api.mock_python_scrolls.values_mut() {
            scroll.end_drag();
        }
        for scroll in self.ide_panel.api.mock_python_scrolls_x.values_mut() {
            scroll.end_drag();
        }
        for terminal in &mut self.ide_panel.terminals {
            terminal.scroll_y.end_drag();
        }
        if let Some(popup) = &mut self.autocomplete_detail_popup {
            popup.scroll.end_drag();
        }
        for tab in &mut self.tabs {
            match &mut tab.kind {
                crate::app::EditorTabKind::ApiClient(_, state) => {
                    state.body_scroll.end_drag();
                    state.body_scroll_x.end_drag();
                    state.output_scroll.end_drag();
                    state.output_scroll_x.end_drag();
                    state.mock_static_response_scroll.end_drag();
                    state.mock_static_response_scroll_x.end_drag();
                    state.response_scroll.end_drag();
                    state.response_scroll_x.end_drag();
                    state.output_schema_menu_scroll.end_drag();
                }
                crate::app::EditorTabKind::DatabaseTable(_, state) => {
                    state.grid.text_drag = None;
                    state.grid.scroll_x.end_drag();
                    state.grid.scroll_y.end_drag();
                    state.unavailable_text_dragging = false;
                    state.grid.column_resize = None;
                }
                crate::app::EditorTabKind::DatabaseQuery(_, state) => {
                    state.result_view.scroll_x.end_drag();
                    state.result_view.scroll_y.end_drag();
                    state.result_view.review_message_scroll_y.end_drag();
                    state.result_view.is_resizing_height = false;
                    state.result_view.column_resize = None;
                }
                _ => {}
            }
        }
        self.tool_installer.end_log_scroll_drag();
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.git_graph_tooltip_selecting = false;
        }
        crate::app::mouse::HOVER_STATE.with(|state| {
            let mut state = state.borrow_mut();
            if let Some(popup) = &mut state.popup {
                popup.scroll.end_drag();
            }
            state.selecting = false;
            state.diag_selecting = false;
        });
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn handle_main_mouse_input(
        &mut self,
        event_loop: &ActiveEventLoop,
        state: ElementState,
        button: winit::event::MouseButton,
    ) {
        let editor_was_focused = self.editor_has_input_focus();
        self.handle_main_mouse_input_inner(event_loop, state, button);
        self.autosave_after_editor_focus_change(editor_was_focused);
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn handle_main_mouse_input_inner(
        &mut self,
        _event_loop: &ActiveEventLoop,
        state: ElementState,
        button: winit::event::MouseButton,
    ) {
        let mx = self.renderer.as_ref().unwrap().last_mouse_x;
        let my = self.renderer.as_ref().unwrap().last_mouse_y;
        if state == ElementState::Pressed && button == winit::event::MouseButton::Left {
            stop_click_scroll_anims(self);
            if self.ide_panel.database.dialog.is_some() {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.suppress_database_dialog_tooltip_after_click();
                }
                if self.start_database_dialog_scroll_drag(mx, my) {
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
            }
            if self
                .ui_registry
                .find_at(mx, my)
                .and_then(crate::app::project_search_app::project_search_field_for_ui_id)
                .is_none()
            {
                self.ide_panel.project_search.focused = None;
            }
            if self.ide_panel.api.mock_contract_constraint_menu.is_some() {
                let clicked_id = self.ui_registry.find_at(mx, my);
                if !self.api_mock_constraint_menu_contains_ui_id(clicked_id) {
                    self.close_api_mock_constraint_menu();
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                }
            }
        }
        if state == ElementState::Released {
            self.finish_database_table_drag();
        }
        if state == ElementState::Released && self.autocomplete_detail_selecting {
            self.cancel_pointer_interactions();
            self.window.as_ref().unwrap().request_redraw();
            return;
        }
        if state == ElementState::Released && self.finish_api_route_text_selection() {
            self.window.as_ref().unwrap().request_redraw();
        }
        if state == ElementState::Released {
            if let Some(popup) = &mut self.autocomplete_detail_popup {
                popup.scroll.end_drag();
            }
            if self
                .renderer
                .as_ref()
                .is_some_and(|renderer| renderer.git_graph_tooltip_selecting)
            {
                self.cancel_pointer_interactions();
                self.window.as_ref().unwrap().request_redraw();
                return;
            }
        }

        if state == ElementState::Pressed {
            let in_hover_popup = HOVER_STATE.with(|hover_state| {
                hover_state
                    .borrow()
                    .popup_or_bridge_contains(
                        mx,
                        my,
                        self.renderer.as_ref().unwrap().width,
                        self.renderer.as_ref().unwrap().scale_factor,
                    )
                    .0
            });

            if !in_hover_popup && clear_hover_popup(self.renderer.as_mut()) {
                self.window.as_ref().unwrap().request_redraw();
            }
        }

        if button == winit::event::MouseButton::Left {
            let ddl_rect = self
                .ide_panel
                .database
                .ddl_hover
                .try_borrow()
                .ok()
                .and_then(|ddl| ddl.as_ref().and_then(|state| state.rect));
            if let Some(rect) = ddl_rect {
                let inside =
                    mx >= rect.0 && mx <= rect.0 + rect.2 && my >= rect.1 && my <= rect.1 + rect.3;
                if state == ElementState::Pressed {
                    if inside {
                        if let Ok(mut ddl) = self.ide_panel.database.ddl_hover.try_borrow_mut()
                            && let Some(ddl) = ddl.as_mut()
                        {
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
                    } else {
                        *self.ide_panel.database.ddl_hover.borrow_mut() = None;
                    }
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                    return;
                }
                if state == ElementState::Released {
                    if let Ok(mut ddl) = self.ide_panel.database.ddl_hover.try_borrow_mut()
                        && let Some(ddl) = ddl.as_mut()
                    {
                        ddl.selecting = false;
                    }
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                    return;
                }
            }
        }

        if state == ElementState::Pressed && self.api_python_runtime_overlay_active() {
            if button == winit::event::MouseButton::Left {
                let clicked_id = self.ui_registry.find_overlay_at(mx, my);
                if let Some(clicked_id) = clicked_id
                    && crate::app::App::ui_id_is_api_python_runtime_overlay(clicked_id)
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
            return;
        }

        if state == ElementState::Pressed && self.ide_panel.project_search.help_open {
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
            return;
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
            return;
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
                return;
            }
            if !keep {
                self.ide_panel.database.context_menu = None;
                if button != winit::event::MouseButton::Left {
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                    return;
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
            return;
        }

        if state == ElementState::Pressed && self.ide_panel.git.commit_menu_open {
            let clicked_id = self.ui_registry.find_at(mx, my);
            let keep_git_menu = matches!(
                clicked_id,
                Some(
                    crate::ui_system::UiId::GitCommitMenuToggle
                        | crate::ui_system::UiId::GitCommitMenuItem(_)
                )
            );
            if !keep_git_menu {
                self.ide_panel.git.commit_menu_open = false;
                if clicked_id.is_none() {
                    self.window.as_ref().unwrap().request_redraw();
                    return;
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
                self.ide_panel.git.repo_action_menu_workspace_idx = None;
                if clicked_id.is_none() {
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                    return;
                }
            }
        }

        if state == ElementState::Pressed
            && button != winit::event::MouseButton::Left
            && self.autocomplete_window_contains(mx, my)
        {
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        if state == ElementState::Pressed
            && button == winit::event::MouseButton::Right
            && let Some(id) = self.ui_registry.find_at(mx, my)
            && self.open_database_context_menu_for_hit(id, mx, my)
        {
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            return;
        }

        if state == ElementState::Pressed
            && button == winit::event::MouseButton::Right
            && let Some(id) = self.ui_registry.find_at(mx, my)
            && self.open_tab_context_menu_for_hit(id, mx, my)
        {
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        if state == ElementState::Pressed
            && button == winit::event::MouseButton::Right
            && self.file_tree_panel_contains(mx, my)
        {
            self.open_file_tree_context_menu(mx, my);
            self.window.as_ref().unwrap().request_redraw();
            return;
        }
        if state == ElementState::Pressed && button == winit::event::MouseButton::Left {
            if self.autocomplete_active {
                let in_main = self
                    .autocomplete_rect
                    .is_some_and(|(x, y, w, h)| mx >= x && mx <= x + w && my >= y && my <= y + h);
                let in_detail = self
                    .autocomplete_detail_rect
                    .is_some_and(|(x, y, w, h)| mx >= x && mx <= x + w && my >= y && my <= y + h);
                if in_detail {
                    if self.autocomplete_detail_max_scroll > 0.0
                        && self.ui_registry.find_at(mx, my)
                            == Some(crate::ui_system::UiId::HoverPopupScroll)
                    {
                        let s = self.renderer.as_ref().unwrap().scale_factor;
                        let max_scroll = self.autocomplete_detail_max_scroll;
                        if let (Some(rect), Some(popup)) = (
                            self.autocomplete_detail_rect,
                            self.autocomplete_detail_popup.as_mut(),
                        ) {
                            if let Some((drag_offset, target)) =
                                crate::app::mouse::hover_popup_scrollbar_drag_target(
                                    rect,
                                    max_scroll,
                                    popup.scroll.current,
                                    my,
                                    s,
                                    None,
                                )
                            {
                                popup.scroll.jump_to(target);
                                popup.scroll.drag_offset = drag_offset;
                                popup.scroll.anim_speed = 15.0;
                                popup.scroll.is_dragging = true;
                            }
                        }
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                    if let (Some(rect), Some(popup)) = (
                        self.autocomplete_detail_rect,
                        self.autocomplete_detail_popup.as_ref(),
                    ) {
                        let byte = hover_popup_byte_at(
                            self.renderer.as_mut().unwrap(),
                            popup,
                            rect,
                            mx,
                            my,
                        );
                        self.autocomplete_detail_selection_anchor = Some(byte);
                        self.autocomplete_detail_selection_cursor = Some(byte);
                        self.autocomplete_detail_selecting = true;
                    }
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
                if in_main {
                    if let Some(rect) = self.autocomplete_rect {
                        let s = self.renderer.as_ref().unwrap().scale_factor;
                        if mx >= rect.0 + rect.2 - 14.0 * s {
                            if let Some((drag_offset, target)) = autocomplete_scroll_click_target(
                                my,
                                rect.1,
                                rect.3,
                                self.autocomplete_scroll.current,
                                self.autocomplete_options.len(),
                                s,
                            ) {
                                apply_autocomplete_scroll_drag(
                                    &mut self.autocomplete_scroll,
                                    target,
                                    drag_offset,
                                );
                            }
                            self.window.as_ref().unwrap().request_redraw();
                            return;
                        }
                        if let Some(idx) = autocomplete_item_index_at(
                            mx,
                            my,
                            rect,
                            self.autocomplete_scroll.current,
                            self.autocomplete_options.len(),
                            s,
                        ) {
                            if idx == self.autocomplete_selected_idx {
                                self.apply_autocomplete();
                            } else {
                                self.autocomplete_selected_idx = idx;
                                self.autocomplete_hovered_idx = None;
                                self.request_active_autocomplete_detail_for_index(idx);
                            }
                        }
                    }
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
                self.close_autocomplete();
                self.window.as_ref().unwrap().request_redraw();
            }
            let in_hover_popup = HOVER_STATE.with(|hover_state| {
                hover_state
                    .borrow()
                    .popup_or_bridge_contains(
                        mx,
                        my,
                        self.renderer.as_ref().unwrap().width,
                        self.renderer.as_ref().unwrap().scale_factor,
                    )
                    .0
            });

            if !in_hover_popup {
                clear_hover_popup(self.renderer.as_mut());
            }

            if self.modifiers.control_key() {
                if let Some(target) = self.ctrl_definition_target_under_mouse() {
                    self.jump_to_definition_target(target);
                    return;
                }
            }
        }

        if self.is_ide_mode
            && self.ide_panel.is_open(crate::app::PanelId::Terminal)
            && self.ide_panel.terminal_focused
        {
            if let Some(crate::ui_system::UiId::TerminalBody) = self.ui_registry.find_at(mx, my) {
                let active = self.ide_panel.active_terminal;
                let mut tracking = false;
                if let Some(term) = self.ide_panel.terminals.get_mut(active) {
                    if crate::app::terminal::lock_terminal_grid(&term.grid).mouse_tracking {
                        tracking = true;
                    }
                }
                if tracking {
                    let btn_code = terminal_mouse_button_code(button);
                    let is_pressed = state == ElementState::Pressed;
                    let s = self.renderer.as_ref().unwrap().scale_factor;
                    let (terminal_panel_x, content_y, _, content_h, _) =
                        super::app_panel_scroll_rect(self, crate::app::PanelId::Terminal, s);
                    let panel_x = terminal_panel_x + 10.0 * s;
                    let char_w = self.renderer.as_mut().unwrap().char_advance('A')
                        * crate::render_view::terminal_ui::TERMINAL_TEXT_SCALE;
                    let char_h = self.renderer.as_ref().unwrap().line_height
                        * crate::render_view::terminal_ui::TERMINAL_TEXT_SCALE;
                    let (term_content_y, term_content_h) =
                        crate::render_view::terminal_ui::terminal_body_rect(
                            content_y, content_h, s,
                        );

                    let cell_x = terminal_mouse_cell_x(mx, panel_x, char_w);

                    let mut is_drag = false;
                    let mut cell_y = 1;
                    if let Some(term) = self.ide_panel.terminals.get_mut(active) {
                        let mut grid = crate::app::terminal::lock_terminal_grid(&term.grid);
                        let scrollback_len = if grid.is_alt {
                            0
                        } else {
                            grid.scrollback.len()
                        };
                        let total_lines = scrollback_len + grid.lines.len();
                        let max_scroll = if grid.is_alt {
                            0.0
                        } else {
                            crate::render_view::terminal_ui::terminal_max_scroll(
                                total_lines,
                                char_h,
                                term_content_h,
                                s,
                            )
                        };
                        let scroll_offset = if grid.is_alt {
                            0.0
                        } else {
                            term.scroll_y.current.min(max_scroll).round()
                        };
                        cell_y = terminal_mouse_cell_y(
                            my,
                            term_content_y,
                            term_content_h,
                            scroll_offset,
                            char_h,
                            s,
                            grid.visible_rows,
                        );

                        if is_pressed {
                            grid.selection = None;
                        } else if let Some((sx, sy, ex, ey)) = grid.selection {
                            if sx != ex || sy != ey {
                                is_drag = true;
                            }
                        }
                    }

                    if !is_drag {
                        let seq = terminal_mouse_sgr_sequence(btn_code, cell_x, cell_y, is_pressed);
                        if let Some(term) = self.ide_panel.terminals.get_mut(active) {
                            let _ = term.write_input(seq.as_bytes());
                        }
                    }
                }
            }
        }

        if state == ElementState::Pressed && button == winit::event::MouseButton::Left {
            if let Some(menu) = self.lsp_actions_menu.as_ref() {
                let menu_snapshot = menu.clone();
                let layout = self
                    .renderer
                    .as_mut()
                    .unwrap()
                    .lsp_actions_menu_layout(&menu_snapshot);
                let mut clicked_inside = false;
                if state == ElementState::Pressed {
                    if mx >= layout.x
                        && mx <= layout.x + layout.w
                        && my >= layout.y
                        && my <= layout.y + layout.h
                    {
                        clicked_inside = true;
                        let rel_y =
                            my - layout.y - 4.0 * self.renderer.as_ref().unwrap().scale_factor;
                        if rel_y >= 0.0 {
                            let visible_idx = (rel_y / layout.item_h) as usize;
                            if visible_idx >= layout.visible_items {
                                return;
                            }
                            let idx = layout.first_visible + visible_idx;
                            if idx >= menu.items.len() {
                                return;
                            }
                            let menu_clone = self.lsp_actions_menu.take().unwrap();
                            let item = menu_clone.items[idx].clone();
                            let cursor_line = menu_clone.cursor_line;
                            drop(menu_clone);
                            match item {
                                crate::app::LspActionItem::CodeAction(action) => {
                                    if let Some(edit) = action.edit {
                                        self.apply_workspace_edit(&edit, false);
                                    }
                                }
                                crate::app::LspActionItem::AddNoqa { codes } => {
                                    self.insert_noqa_comment(cursor_line, &codes);
                                }
                                crate::app::LspActionItem::AddNoqaAll => {
                                    self.insert_noqa_comment(cursor_line, &[]);
                                }
                                crate::app::LspActionItem::FixAll => {
                                    if let Some(lsp) = &mut self.lsp {
                                        if let Some(path) = self.file_path.clone() {
                                            if let Some(id) =
                                                lsp.request_fix_all(&path, &self.file_extension)
                                            {
                                                self.pending_fix_all_id = Some(id);
                                            }
                                        }
                                    }
                                }
                                crate::app::LspActionItem::OrganizeImports => {
                                    if let Some(lsp) = &mut self.lsp {
                                        if let Some(path) = self.file_path.clone() {
                                            if let Some(id) = lsp.request_organize_imports(
                                                &path,
                                                &self.file_extension,
                                            ) {
                                                self.pending_fix_all_id = Some(id);
                                            }
                                        }
                                    }
                                }
                                crate::app::LspActionItem::CompleteImports => {
                                    self.request_ty_autocomplete(AutocompleteMode::TyImports, None);
                                }
                            }
                            self.window.as_ref().unwrap().request_redraw();
                            return;
                        }
                    }
                }

                if !clicked_inside {
                    self.lsp_actions_menu = None;
                    self.window.as_ref().unwrap().request_redraw();
                } else {
                    return;
                }
            }

            // Глобальная обработка декларативного UI
            if !self.show_settings && self.dialog_window.is_none() {
                if self.is_ide_mode
                    && let Some(layout) = super::problems_scrollbar_layout(
                        self,
                        self.renderer.as_ref().unwrap().scale_factor,
                    )
                    && crate::ui_system::point_in_rect(
                        mx,
                        my,
                        (
                            layout.content_x,
                            layout.content_y,
                            layout.content_w,
                            layout.content_h,
                        ),
                    )
                {
                    let s = self.renderer.as_ref().unwrap().scale_factor;
                    let scroll_x = layout.content_x + layout.content_w - 12.0 * s;
                    if mx >= scroll_x
                        && let Some(thumb) = crate::scroll::scrollbar_thumb(
                            layout.list_y,
                            layout.track_h,
                            layout.track_h,
                            layout.total_h,
                            self.ide_panel.problems_scroll.current,
                            20.0 * s,
                        )
                    {
                        let max_scroll = (layout.total_h - layout.track_h).max(0.0);
                        let drag_offset = if my >= thumb.start && my <= thumb.start + thumb.len {
                            my - thumb.start
                        } else if my >= layout.list_y && my <= layout.list_y + layout.track_h {
                            let Some((offset, target)) = crate::scroll::scrollbar_drag_target(
                                my,
                                layout.list_y,
                                layout.track_h,
                                thumb,
                                max_scroll,
                                None,
                            ) else {
                                return;
                            };
                            self.ide_panel.problems_scroll.jump_to(target);
                            offset
                        } else {
                            return;
                        };
                        self.ide_panel.problems_scroll.anim_speed = 15.0;
                        self.ide_panel.problems_scroll.drag_offset = drag_offset;
                        self.ide_panel.problems_scroll.is_dragging = true;
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                }

                if self.is_ide_mode {
                    let s = self.renderer.as_ref().unwrap().scale_factor;
                    let sb_w = 48.0 * s;
                    let panel_left_w = self.ide_panel.visible_left_width(s);
                    let panel_bottom_h = if self.ide_panel.any_bottom_open() {
                        self.ide_panel.bottom_height * s
                    } else {
                        0.0
                    };
                    let wh = self.window.as_ref().unwrap().inner_size().height as f32;

                    let resize_bottom_limit = if panel_bottom_h > 0.0
                        && self.ide_panel.bottom_panel_blocks_editor_hover()
                    {
                        crate::render_view::ide_bottom_panel_y(wh, panel_bottom_h, s)
                    } else {
                        wh
                    };

                    let mut manual_resize = false;
                    if panel_left_w > 0.0 {
                        let resize_x = sb_w + panel_left_w;
                        if (mx - resize_x).abs() < 3.0 * s && my >= 0.0 && my < resize_bottom_limit
                        {
                            self.ide_panel.is_resizing_left = true;
                            manual_resize = true;
                        }
                    }
                    if panel_bottom_h > 0.0 && !manual_resize {
                        let resize_y =
                            crate::render_view::ide_bottom_panel_y(wh, panel_bottom_h, s);
                        if (my - resize_y).abs() < 6.0 * s && mx >= sb_w {
                            self.ide_panel.is_resizing_bottom = true;
                            manual_resize = true;
                        }
                    }

                    if manual_resize {
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                }

                let clicked_id = self.ui_registry.find_at(mx, my);
                if state == ElementState::Pressed
                    && button == winit::event::MouseButton::Left
                    && !matches!(
                        clicked_id,
                        Some(
                            crate::ui_system::UiId::ApiOutputSchemaMenu(_)
                                | crate::ui_system::UiId::ApiOutputSchemaMenuItem(_, _)
                        )
                    )
                    && self.close_active_api_output_example_menu()
                {
                    self.window.as_ref().unwrap().request_redraw();
                    if clicked_id.is_none() {
                        return;
                    }
                }
                if self.inline_git_popup.is_some()
                    && button == winit::event::MouseButton::Left
                    && state == ElementState::Pressed
                    && !matches!(
                        clicked_id,
                        Some(
                            crate::ui_system::UiId::InlineGitPanelBody
                                | crate::ui_system::UiId::InlineGitPrevHunk
                                | crate::ui_system::UiId::InlineGitNextHunk
                                | crate::ui_system::UiId::InlineGitRollbackHunk
                                | crate::ui_system::UiId::EditorGitHunk(_, _)
                        )
                    )
                {
                    self.inline_git_popup = None;
                    self.inline_git_diff_rx = None;
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
                let in_graph_tooltip_body = self
                    .renderer
                    .as_ref()
                    .and_then(|renderer| renderer.git_graph_tooltip_hover)
                    .is_some_and(|hover| hover.contains(mx, my));
                if in_graph_tooltip_body
                    && button == winit::event::MouseButton::Left
                    && state == ElementState::Pressed
                    && !matches!(
                        clicked_id,
                        Some(crate::ui_system::UiId::GitGraphCopyCommit(_, _))
                            | Some(crate::ui_system::UiId::GitGraphOpenCommit(_, _))
                    )
                {
                    if let Some(renderer) = self.renderer.as_mut() {
                        let byte = renderer.git_graph_tooltip_byte_at(mx, my);
                        renderer.git_graph_tooltip_selection_anchor = Some(byte);
                        renderer.git_graph_tooltip_selection_cursor = Some(byte);
                        renderer.git_graph_tooltip_selecting = true;
                    }
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
                if button == winit::event::MouseButton::Left
                    && state == ElementState::Pressed
                    && (self.ide_panel.api.import_url_open || self.ide_panel.api.import_menu_open)
                    && !matches!(
                        clicked_id,
                        Some(
                            crate::ui_system::UiId::ApiImportAdd
                                | crate::ui_system::UiId::ApiImportFile
                                | crate::ui_system::UiId::ApiImportUrl
                                | crate::ui_system::UiId::ApiImportUrlInput
                                | crate::ui_system::UiId::ApiImportUrlConfirm
                        )
                    )
                {
                    if matches!(
                        self.ide_panel.api.focused,
                        Some(crate::app::api_client::ApiFocus::ImportUrl)
                    ) {
                        self.commit_api_focus();
                        self.ide_panel.api.focused = None;
                    }
                    self.ide_panel.api.import_url_open = false;
                    self.ide_panel.api.import_menu_open = false;
                    self.window.as_ref().unwrap().request_redraw();
                    if clicked_id.is_none() {
                        return;
                    }
                }
                if let Some(clicked_id) = clicked_id {
                    if matches!(
                        clicked_id,
                        crate::ui_system::UiId::EditorTab(_)
                            | crate::ui_system::UiId::EditorTabClose(_)
                    ) && let Some(renderer) = self.renderer.as_mut()
                    {
                        renderer.suppress_popups_until_next_mouse_move();
                        renderer.reset_delayed_tooltip_anchor();
                    }

                    let in_hover_popup_body = clicked_id == crate::ui_system::UiId::BottomPanelBody
                        && HOVER_STATE.with(|hover_state| {
                            if let Some((x, y, w, h)) = hover_state.borrow().rect {
                                mx >= x && mx <= x + w && my >= y && my <= y + h
                            } else {
                                false
                            }
                        });
                    let in_diag_popup_body = clicked_id == crate::ui_system::UiId::BottomPanelBody
                        && HOVER_STATE.with(|s| {
                            s.borrow().diag_rect.map_or(false, |(x, y, w, h, _, _, _)| {
                                mx >= x && mx <= x + w && my >= y && my <= y + h
                            })
                        });

                    if in_hover_popup_body || in_diag_popup_body {
                        if button == winit::event::MouseButton::Left {
                            if in_hover_popup_body {
                                HOVER_STATE.with(|hover_state| {
                                    let mut hs = hover_state.borrow_mut();
                                    if let (Some(rect), Some(popup)) = (hs.rect, hs.popup.as_ref())
                                    {
                                        let byte = hover_popup_byte_at(
                                            self.renderer.as_mut().unwrap(),
                                            popup,
                                            rect,
                                            mx,
                                            my,
                                        );
                                        if state == ElementState::Pressed {
                                            hs.selection_anchor = Some(byte);
                                            hs.selection_cursor = Some(byte);
                                            hs.selecting = true;
                                        } else {
                                            hs.selecting = false;
                                        }
                                    }
                                });
                            } else {
                                HOVER_STATE.with(|hover_state| {
                                    let mut hs = hover_state.borrow_mut();
                                    let byte = crate::render_view::ui::diag_popup_byte_at(mx, my);
                                    if state == ElementState::Pressed {
                                        hs.diag_selection_anchor = Some(byte);
                                        hs.diag_selection_cursor = Some(byte);
                                        hs.diag_selecting = true;
                                    } else {
                                        hs.diag_selecting = false;
                                    }
                                });
                            }
                            self.window.as_ref().unwrap().request_redraw();
                        }
                        return;
                    } else if clicked_id == crate::ui_system::UiId::BottomPanelBody {
                        self.handle_ui_click(clicked_id);
                        return;
                    }
                    if clicked_id == crate::ui_system::UiId::HoverPopupScroll {
                        let s = self.renderer.as_ref().unwrap().scale_factor;
                        crate::app::mouse::HOVER_STATE.with(|hover_state| {
                            let mut state = hover_state.borrow_mut();
                            if let Some(rect) = state.rect {
                                let max_scroll = state.max_scroll;
                                if let Some(popup) = &mut state.popup {
                                    if let Some((drag_offset, target)) =
                                        crate::app::mouse::hover_popup_scrollbar_drag_target(
                                            rect,
                                            max_scroll,
                                            popup.scroll.current,
                                            my,
                                            s,
                                            None,
                                        )
                                    {
                                        popup.scroll.jump_to(target);
                                        popup.scroll.drag_offset = drag_offset;
                                        popup.scroll.anim_speed = 15.0;
                                        popup.scroll.is_dragging = true;
                                    }
                                }
                            }
                        });
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                    let is_term = matches!(
                        clicked_id,
                        crate::ui_system::UiId::TerminalBody
                            | crate::ui_system::UiId::TerminalScrollY
                            | crate::ui_system::UiId::TerminalTab(_)
                            | crate::ui_system::UiId::TerminalTabClose(_)
                            | crate::ui_system::UiId::TerminalAdd
                            | crate::ui_system::UiId::TerminalSearchInput
                            | crate::ui_system::UiId::TerminalSearchClose
                            | crate::ui_system::UiId::TerminalSearchNext
                            | crate::ui_system::UiId::TerminalSearchPrev
                            | crate::ui_system::UiId::TerminalSearchCaseToggle
                    );
                    let is_resize = matches!(
                        clicked_id,
                        crate::ui_system::UiId::ResizeLeft
                            | crate::ui_system::UiId::ResizeBottom
                            | crate::ui_system::UiId::GitGraphResize
                    );

                    if is_term {
                        self.ide_panel.terminal_focused = true;
                    } else if !is_resize {
                        self.ide_panel.terminal_focused = false;
                    }

                    if let crate::ui_system::UiId::SidebarSlot(panel_id) = clicked_id {
                        self.ide_panel.drag = Some(crate::app::PanelDragState {
                            panel_id,
                            start_y: my,
                            current_y: my,
                            threshold_passed: false,
                        });
                    } else if let crate::ui_system::UiId::EditorTab(idx) = clicked_id {
                        self.ide_panel.tab_drag = Some(crate::app::TabDragState {
                            start_idx: idx,
                            start_x: mx,
                            current_x: mx,
                            threshold_passed: false,
                        });
                        self.handle_ui_click(clicked_id);
                    } else if clicked_id == crate::ui_system::UiId::GitGraphResize {
                        self.ide_panel.git.graph_resizing = true;
                        self.handle_ui_click(clicked_id);
                    } else if clicked_id == crate::ui_system::UiId::FileTreeScrollY {
                        let s = self.renderer.as_ref().unwrap().scale_factor;
                        if let Some(layout) = super::explorer_scrollbar_layout(self, s)
                            && let Some((drag_offset, target)) =
                                crate::scroll::scrollbar_drag_target(
                                    my,
                                    layout.track_y,
                                    layout.track_h,
                                    layout.thumb,
                                    layout.max_scroll,
                                    None,
                                )
                        {
                            self.ide_panel.explorer_scroll.jump_to(target);
                            self.ide_panel.explorer_scroll.drag_offset = drag_offset;
                            self.ide_panel.explorer_scroll.is_dragging = true;
                        }
                        self.handle_ui_click(clicked_id);
                    } else if clicked_id == crate::ui_system::UiId::GitGraphScroll {
                        let s = self.renderer.as_ref().unwrap().scale_factor;
                        if let Some((rows_y, rows_h)) = super::git_graph_rows_bounds(self, s)
                            && let Some((drag_offset, target)) =
                                crate::app::git_panel::git_graph_scroll_drag_target(
                                    my,
                                    rows_y,
                                    rows_h,
                                    self.ide_panel.git.graph_snapshot.len(),
                                    self.ide_panel.git.graph_scroll.current,
                                    None,
                                    s,
                                )
                        {
                            crate::app::git_panel::apply_git_graph_scroll_drag(
                                &mut self.ide_panel.git.graph_scroll,
                                target,
                                drag_offset,
                            );
                            let max_scroll = crate::app::git_panel::git_graph_max_scroll(
                                self.ide_panel.git.graph_snapshot.len(),
                                rows_h,
                                s,
                            );
                            if self.ide_panel.git.graph_has_more
                                && crate::app::git_panel::git_graph_near_load_more(
                                    target, max_scroll, s,
                                )
                            {
                                self.load_more_git_graph_commits();
                            }
                        }
                        self.handle_ui_click(clicked_id);
                    } else if clicked_id == crate::ui_system::UiId::TerminalScrollY {
                        let layout = active_terminal_scrollbar_layout(self);
                        let active = self.ide_panel.active_terminal;
                        if let (Some(layout), Some(term)) =
                            (layout, self.ide_panel.terminals.get_mut(active))
                            && let Some((drag_offset, target)) =
                                crate::render_view::terminal_ui::terminal_scrollbar_drag_target(
                                    my, layout, None,
                                )
                        {
                            term.scroll_y.drag_offset = drag_offset;
                            term.scroll_y.current = target;
                            term.scroll_y.target = target;
                            term.scroll_y.velocity = 0.0;
                            term.scroll_y.is_dragging = true;
                        }
                        self.handle_ui_click(clicked_id);
                    } else {
                        if clicked_id == crate::ui_system::UiId::ProjectSearchQueryScrollbarY {
                            if self.start_project_search_query_scrollbar_drag(
                                crate::app::project_search::ProjectSearchQueryScrollAxis::Vertical,
                                my,
                            ) {
                                self.window.as_ref().unwrap().request_redraw();
                            }
                            return;
                        }
                        if clicked_id == crate::ui_system::UiId::ProjectSearchQueryScrollbarX {
                            if self.start_project_search_query_scrollbar_drag(
                                crate::app::project_search::ProjectSearchQueryScrollAxis::Horizontal,
                                mx,
                            ) {
                                self.window.as_ref().unwrap().request_redraw();
                            }
                            return;
                        }
                        if clicked_id == crate::ui_system::UiId::ProjectSearchScrollbar {
                            if self.start_project_search_scrollbar_drag(my) {
                                let _ = self.queue_visible_project_search_previews();
                                self.window.as_ref().unwrap().request_redraw();
                            }
                            return;
                        }
                        if let Some(field) =
                            crate::app::project_search_app::project_search_field_for_ui_id(
                                clicked_id,
                            )
                        {
                            if field != crate::app::project_search::ProjectSearchField::Filter
                                || self.ide_panel.project_search.filter_enabled()
                            {
                                self.ide_panel.project_search.dragging_field = Some(field);
                            }
                        }
                        if clicked_id == crate::ui_system::UiId::TerminalBody {
                            self.ide_panel.is_dragging_terminal = true;
                        } else {
                            self.ide_panel.is_dragging_terminal = false;
                        }
                        self.handle_ui_click(clicked_id);
                    }
                    return;
                }
            }
        }

        // Clicks routed through UI system

        if self.dialog_window.is_some() {
            if state == ElementState::Pressed {
                if let Some(dw) = self.dialog_window.as_ref() {
                    dw.focus_window();
                    dw.request_redraw();
                }
            }
            return;
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
            return;
        }

        if self.show_settings {
            if state == ElementState::Released {
                self.is_dragging_settings_ignore = false;
                self.is_dragging_lsp_log = false;
                self.settings_scroll.end_drag();
                self.settings_ide_scroll.end_drag();
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
                    }
                }
            }
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        if state == ElementState::Released {
            // Завершаем DnD и ресайз IDE-панелей
            if self.is_ide_mode {
                if let Some(drag) = self.ide_panel.file_tree_drag.take() {
                    if drag.threshold_passed {
                        if let Some(target_dir) = self.file_tree_drop_target_dir(drag.target_idx) {
                            let has_valid_move = drag.paths.iter().any(|src| {
                                src.parent() != Some(target_dir.as_path())
                                    && !(src.is_dir() && target_dir.starts_with(src))
                            });
                            if has_valid_move {
                                self.ide_panel.file_tree_move_dialog =
                                    Some(crate::app::file_tree::FileTreeMoveDialog {
                                        sources: drag.paths,
                                        target_dir,
                                        error: None,
                                    });
                            }
                        }
                    }
                }
                if let Some(drag) = self.ide_panel.tab_drag.take() {
                    if drag.threshold_passed
                        && self.tabs.len() > 1
                        && drag.start_idx < self.tabs.len()
                    {
                        let s = self.renderer.as_ref().unwrap().scale_factor;
                        let start_cx = if self.is_ide_mode {
                            let panel_left_w = self.ide_panel.visible_left_width(s);
                            (48.0 * s + panel_left_w).round() + 1.0 - self.tab_scroll.current
                        } else {
                            -self.tab_scroll.current
                        };

                        let display_titles = crate::app::tab_display_titles_for(
                            &self.tabs,
                            self.active_tab,
                            self.file_path.as_ref(),
                            &self.base_title,
                        );

                        let mut widths = Vec::new();
                        for (i, tab) in self.tabs.iter().enumerate() {
                            let title = &display_titles[i];
                            widths.push(
                                self.renderer
                                    .as_mut()
                                    .unwrap()
                                    .editor_tab_width(tab, title, s),
                            );
                        }

                        let mut initial_xs = vec![0.0; self.tabs.len()];
                        let mut cx = start_cx;
                        for i in 0..self.tabs.len() {
                            initial_xs[i] = cx;
                            cx += widths[i];
                        }

                        let dragged_x =
                            initial_xs[drag.start_idx] + (drag.current_x - drag.start_x);
                        let dragged_w = widths[drag.start_idx];

                        let mut new_idx = drag.start_idx;
                        let dragged_center = dragged_x + dragged_w / 2.0;

                        for i in 0..self.tabs.len() {
                            if i == drag.start_idx {
                                continue;
                            }
                            let other_center = initial_xs[i] + widths[i] / 2.0;

                            if i < drag.start_idx {
                                if dragged_center < other_center {
                                    new_idx = new_idx.min(i);
                                }
                            } else {
                                if dragged_center > other_center {
                                    new_idx = new_idx.max(i);
                                }
                            }
                        }

                        if new_idx != drag.start_idx {
                            self.sync_active_tab();
                            let tab = self.tabs.remove(drag.start_idx);
                            self.tabs.insert(new_idx, tab);

                            if self.active_tab == drag.start_idx {
                                self.active_tab = new_idx;
                            } else if self.active_tab > drag.start_idx && self.active_tab <= new_idx
                            {
                                self.active_tab -= 1;
                            } else if self.active_tab < drag.start_idx && self.active_tab >= new_idx
                            {
                                self.active_tab += 1;
                            }
                            self.sync_active_tab();
                            self.save_tabs_state();
                        }
                    }
                }
                if let Some(drag) = self.ide_panel.drag.take() {
                    if !drag.threshold_passed {
                        // Клик без движения → переключить панель
                        let toggled_open = {
                            let slot = self
                                .ide_panel
                                .slots
                                .iter()
                                .find(|sl| sl.id == drag.panel_id);
                            slot.map(|s| !s.open).unwrap_or(false)
                        };
                        let toggled_group = {
                            let slot = self
                                .ide_panel
                                .slots
                                .iter()
                                .find(|sl| sl.id == drag.panel_id);
                            slot.map(|s| s.group.clone())
                        };
                        self.ide_panel.toggle(drag.panel_id);
                        // При открытии Explorer — запускаем скан файлов
                        if toggled_open && drag.panel_id == crate::app::PanelId::Explorer {
                            self.refresh_file_tree();
                        }
                        if toggled_open && drag.panel_id == crate::app::PanelId::Search {
                            self.ide_panel.project_search.focused =
                                Some(crate::app::project_search::ProjectSearchField::Query);
                        }
                        // Взаимоисключение: при открытии кнопки закрываем остальные в той же группе
                        if toggled_open {
                            if let Some(group) = toggled_group {
                                for sl in self.ide_panel.slots.iter_mut() {
                                    if sl.id != drag.panel_id && sl.group == group {
                                        sl.open = false;
                                    }
                                }
                            }
                        }
                        // Clamp scroll_y к новому max_scroll после изменения высоты панелей
                        let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                        let s = self.renderer.as_ref().unwrap().scale_factor;
                        let tab_bar_h = crate::render_view::editor_content_top_inset(
                            self.show_welcome,
                            self.is_ide_mode,
                            self.active_tab_is_database_query(),
                            s,
                        );
                        let editor_bottom_h = if self.is_ide_mode {
                            self.ide_panel.editor_reserved_bottom_height(s)
                        } else {
                            0.0
                        };
                        let visible_h = crate::render_view::editor_view_height(
                            wh,
                            tab_bar_h,
                            editor_bottom_h,
                            self.is_ide_mode,
                            s,
                        );
                        let max_scroll = self
                            .renderer
                            .as_mut()
                            .unwrap()
                            .get_max_scroll(&self.editor, visible_h);
                        self.scroll_y.clamp_target(0.0, max_scroll);
                        self.scroll_y.clamp_current(0.0, max_scroll);
                    } else {
                        // DnD завершён — определяем новую группу по позиции и сортируем
                        let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                        let new_group = if drag.current_y < wh / 2.0 {
                            crate::app::PanelGroup::Top
                        } else {
                            crate::app::PanelGroup::Bottom
                        };

                        let s = self.renderer.as_ref().unwrap().scale_factor;
                        let btn_size = 48.0 * s;
                        let btn_gap = 0.0;
                        let top_start_y = 0.0;

                        let mut top_items = Vec::new();
                        let mut bottom_items = Vec::new();
                        let mut top_idx = 0;
                        let mut bottom_idx = 0;

                        // Назначаем виртуальные Y-координаты всем элементам для сортировки
                        for mut slot in self.ide_panel.slots.drain(..) {
                            if slot.id == drag.panel_id {
                                slot.group = new_group.clone();
                                if matches!(new_group, crate::app::PanelGroup::Top) {
                                    top_items.push((drag.current_y, slot));
                                } else {
                                    bottom_items.push((drag.current_y, slot));
                                }
                            } else {
                                if matches!(slot.group, crate::app::PanelGroup::Top) {
                                    let y = top_start_y + top_idx as f32 * (btn_size + btn_gap);
                                    top_items.push((y, slot));
                                    top_idx += 1;
                                } else {
                                    let y =
                                        wh - btn_size - bottom_idx as f32 * (btn_size + btn_gap);
                                    bottom_items.push((y, slot));
                                    bottom_idx += 1;
                                }
                            }
                        }

                        // Сортируем: для Top сверху вниз (по возрастанию Y), для Bottom снизу вверх (по убыванию Y)
                        top_items.sort_by(|a, b| {
                            a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)
                        });
                        bottom_items.sort_by(|a, b| {
                            b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal)
                        });

                        // Собираем массив обратно
                        self.ide_panel
                            .slots
                            .extend(top_items.into_iter().map(|(_, s)| s));
                        self.ide_panel
                            .slots
                            .extend(bottom_items.into_iter().map(|(_, s)| s));
                        self.ide_panel.reconcile_moved_panel(drag.panel_id);
                    }
                    crate::save_panel_state(&self.ide_panel);
                }
                if self.ide_panel.is_resizing_left
                    || self.ide_panel.is_resizing_bottom
                    || self.ide_panel.git.graph_resizing
                {
                    self.ide_panel.is_resizing_left = false;
                    self.ide_panel.is_resizing_bottom = false;
                    self.ide_panel.git.graph_resizing = false;
                    crate::save_panel_state(&self.ide_panel);
                }
            }
            self.cancel_pointer_interactions();
            self.scroll_y.target = self.scroll_y.target.round();
            self.scroll_x.target = self.scroll_x.target.round();
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        if state == ElementState::Pressed {
            let last_mouse_x = self.renderer.as_ref().unwrap().last_mouse_x;
            let last_mouse_y = self.renderer.as_ref().unwrap().last_mouse_y;

            // Sidebar processing and resizing moved to ui_registry
            // Обработка кликов в дереве файлов теперь выполняется через ui_registry
            // Search input handled by ui_registry

            if self.autocomplete_active {
                if let Some((rx, ry, rw, rh)) = self.autocomplete_rect {
                    if last_mouse_x >= rx
                        && last_mouse_x <= rx + rw
                        && last_mouse_y >= ry
                        && last_mouse_y <= ry + rh
                    {
                        self.close_autocomplete();
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    } else {
                        self.close_autocomplete();
                        self.window.as_ref().unwrap().request_redraw();
                    }
                }
            }

            // Sticky lines, Folding, Scrollbars and Text Selection handled by ui_registry
            self.last_action = Instant::now();
        }
        self.window.as_ref().unwrap().request_redraw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_rect_helpers_union_and_padding_are_inclusive() {
        assert_eq!(union_rect(None, None), None);
        assert_eq!(
            union_rect(Some((1.0, 2.0, 3.0, 4.0)), None),
            Some((1.0, 2.0, 3.0, 4.0)),
        );
        assert_eq!(
            union_rect(
                Some((10.0, 10.0, 20.0, 20.0)),
                Some((25.0, 5.0, 20.0, 10.0)),
            ),
            Some((10.0, 5.0, 35.0, 25.0)),
        );
        assert!(point_in_padded_rect(
            8.0,
            8.0,
            (10.0, 10.0, 20.0, 20.0),
            2.0,
        ));
        assert!(!point_in_padded_rect(
            7.9,
            8.0,
            (10.0, 10.0, 20.0, 20.0),
            2.0,
        ));
    }

    #[test]
    fn terminal_mouse_helpers_match_sgr_protocol_edges() {
        assert_eq!(
            terminal_mouse_button_code(winit::event::MouseButton::Left),
            0,
        );
        assert_eq!(
            terminal_mouse_button_code(winit::event::MouseButton::Middle),
            1,
        );
        assert_eq!(
            terminal_mouse_button_code(winit::event::MouseButton::Right),
            2,
        );
        assert_eq!(terminal_mouse_cell_x(0.0, 50.0, 10.0), 1);
        assert_eq!(terminal_mouse_cell_x(75.0, 50.0, 10.0), 3);
        assert_eq!(
            terminal_mouse_cell_y(172.0, 100.0, 100.0, 0.0, 20.0, 1.0, 5),
            4,
        );
        assert_eq!(terminal_mouse_sgr_sequence(0, 3, 4, true), "\x1b[<0;3;4M",);
        assert_eq!(terminal_mouse_sgr_sequence(2, 1, 1, false), "\x1b[<2;1;1m",);
    }

    #[test]
    fn autocomplete_scroll_click_target_keeps_thumb_or_pages_to_pointer() {
        assert_eq!(
            autocomplete_scroll_click_target(20.0, 10.0, 200.0, 0.0, 3, 1.0),
            None,
        );

        let (drag_offset, target) =
            autocomplete_scroll_click_target(13.0, 10.0, 160.0, 0.0, 20, 1.0).unwrap();
        assert_eq!(drag_offset, 0.0);
        assert_eq!(target, 0.0);

        let (_, paged_target) =
            autocomplete_scroll_click_target(140.0, 10.0, 160.0, 0.0, 20, 1.0).unwrap();
        assert!(paged_target > 0.0);
    }

    #[test]
    fn autocomplete_scroll_drag_updates_rendered_position_immediately() {
        let mut scroll = crate::scroll::ScrollState::new(7.0);
        scroll.current = 12.0;
        scroll.target = 20.0;
        scroll.velocity = 9.0;
        apply_autocomplete_scroll_drag(&mut scroll, 144.0, 8.0);
        assert_eq!(scroll.current, 144.0);
        assert_eq!(scroll.target, 144.0);
        assert_eq!(scroll.velocity, 0.0);
        assert_eq!(scroll.drag_offset, 8.0);
        assert!(scroll.is_dragging);
    }

    #[test]
    fn autocomplete_item_index_at_ignores_scrollbar_and_accounts_for_scroll() {
        let rect = (10.0, 20.0, 200.0, 260.0);
        assert_eq!(
            autocomplete_item_index_at(20.0, 20.0, rect, 0.0, 10, 1.0),
            Some(0)
        );
        assert_eq!(
            autocomplete_item_index_at(20.0, 60.0, rect, 0.0, 10, 1.0),
            Some(1)
        );
        assert_eq!(
            autocomplete_item_index_at(20.0, 60.0, rect, 72.0, 10, 1.0),
            Some(3)
        );
        assert_eq!(
            autocomplete_item_index_at(202.0, 60.0, rect, 0.0, 10, 1.0),
            None
        );
    }
}
