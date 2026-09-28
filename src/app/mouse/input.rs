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

fn git_logs_copy_owner_survives_left_press(target: Option<crate::ui_system::UiId>) -> bool {
    matches!(
        target,
        Some(crate::ui_system::UiId::GitLogsBody | crate::ui_system::UiId::GitLogsScroll)
    )
}

fn update_git_logs_copy_owner_on_left_press(
    git: &mut crate::app::git_panel::GitPanelState,
    target: Option<crate::ui_system::UiId>,
) {
    if !git_logs_copy_owner_survives_left_press(target) {
        git.revoke_git_logs_copy_owner();
    }
}

fn begin_git_logs_text_selection(
    app: &mut App,
    point: crate::app::git_panel::GitLogTextPoint,
) -> bool {
    app.focus_document_text_surface();
    let selected = app.ide_panel.git.git_logs.set_selection(point, point);
    if selected {
        app.ide_panel.git.claim_git_logs_copy_owner();
    }
    selected
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

pub(crate) fn stop_click_scroll_anims(app: &mut App, preserve_main_vertical: bool) {
    stop_scroll_anim(&mut app.settings_scroll);
    stop_scroll_anim(&mut app.tab_scroll);
    stop_scroll_anim(&mut app.ide_panel.terminal_tab_scroll);
    if !preserve_main_vertical {
        stop_scroll_anim(&mut app.scroll_y);
        app.markdown.settle_pending_target_navigation_as_motion();
    }
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
    stop_scroll_anim(&mut app.ide_panel.git.logs_scroll);
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

    let state = &mut app.hover;
    stop_scroll_anim(&mut state.diag_scroll);
    if let Some(popup) = state.popup.as_mut() {
        stop_scroll_anim(&mut popup.scroll);
    }
}

fn project_search_help_captures_pressed_click(app: &App) -> bool {
    app.ide_panel.project_search.help_open
}

fn database_ddl_captures_left_click(app: &App) -> Option<(f32, f32, f32, f32)> {
    app.ide_panel
        .database
        .ddl_hover
        .try_borrow()
        .ok()
        .and_then(|ddl| ddl.as_ref().and_then(|state| state.rect))
}

fn preserve_main_vertical_scroll_for_click(app: &App, mx: f32, my: f32) -> bool {
    !project_search_help_captures_pressed_click(app)
        && database_ddl_captures_left_click(app).is_none()
        && matches!(
            app.ui_registry.find_at(mx, my),
            Some(
                crate::ui_system::UiId::MarkdownModeToggle
                    | crate::ui_system::UiId::EditorScrollbarY
            )
        )
}

/// Autocomplete list scrollbar for the popup `rect`: 14 px lane at the right edge (the press
/// zone), track inset 3 px, 6 px thumb; at most 7 rows are visible. Shared by the press and
/// drag handlers and the list renderer, so both use the same geometry.
pub(crate) fn autocomplete_scrollbar(
    rect: (f32, f32, f32, f32),
    total_items: usize,
    current_scroll: f32,
    scale: f32,
) -> crate::render_view::scrollbar_widget::Scrollbar {
    use crate::render_view::scrollbar_widget::{
        Scrollbar, ScrollbarAxis, ScrollbarExtent, ScrollbarStyle,
    };
    let step = 36.0 * scale;
    let total_items = total_items as f32;
    let (x, y, w, h) = rect;
    let lane_w = 14.0 * scale;
    Scrollbar {
        style: ScrollbarStyle {
            thumb_thickness: 6.0,
            track_pad: 3.0,
            thumb_color: [0.7, 0.33, 0.54, 1.0],
            ..ScrollbarStyle::BASE
        },
        axis: ScrollbarAxis::Vertical,
        lane: (x + w - lane_w, y, lane_w, h),
        extent: ScrollbarExtent {
            max_scroll: ((total_items - total_items.min(7.0)) * step).max(0.0),
            ..ScrollbarExtent::new(h, total_items * step, current_scroll)
        },
    }
}

#[cfg(test)]
fn finish_markdown_read_selection_on_left_release(
    markdown: &mut crate::app::markdown::MarkdownTabState,
    state: ElementState,
    button: winit::event::MouseButton,
) -> bool {
    if state != ElementState::Released
        || button != winit::event::MouseButton::Left
        || !markdown.read_selecting
    {
        return false;
    }
    markdown.finish_read_selection();
    true
}

impl App {
    pub(crate) fn cancel_pointer_interactions(&mut self) {
        self.finish_database_table_drag();
        self.finish_markdown_read_selection_gesture();
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
        self.ide_panel.terminal_tab_drag = None;
        self.ide_panel.drag = None;
        self.ide_panel.project_search.dragging_field = None;
        self.ide_panel.file_tree_dialog_input_drag = None;
        if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
            dialog.dragging_field = None;
            dialog.scroll.end_drag();
        }
        if let Ok(mut ddl) = self.ide_panel.database.ddl_hover.try_borrow_mut()
            && let Some(state) = ddl.as_mut()
        {
            state.selecting = false;
            state.popup.scroll.end_drag();
        }
        self.ide_panel.database.table_modal_input_dragging = false;

        self.scroll_y.end_drag();
        self.markdown.end_code_scroll_drag();
        self.scroll_x.end_drag();
        self.settings_scroll.end_drag();
        self.settings_ide_scroll.end_drag();
        self.settings_general_scroll.end_drag();
        self.settings_database_scroll.end_drag();
        self.autocomplete_scroll.end_drag();
        self.ide_panel.explorer_scroll.end_drag();
        self.ide_panel.project_search.scroll.end_drag();
        self.ide_panel.project_search.query_scroll_y.end_drag();
        self.ide_panel.project_search.query_scroll_x.end_drag();
        self.ide_panel.lsp_scroll_x.end_drag();
        self.ide_panel.lsp_scroll_y.end_drag();
        self.ide_panel.api.mock_guide_scroll.end_drag();
        self.ide_panel.api.mock_server_log_scroll.end_drag();
        self.ide_panel.api.mock_python_versions_scroll.end_drag();
        self.ide_panel.api.mock_python_install_log_scroll.end_drag();
        self.ide_panel.problems_scroll.end_drag();
        self.ide_panel.git.scroll.end_drag();
        self.ide_panel.git.graph_scroll.end_drag();
        self.ide_panel.git.logs_scroll.end_drag();
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
            renderer.git_logs_selecting = false;
        }
        let state = &mut self.hover;
        if let Some(popup) = &mut state.popup {
            popup.scroll.end_drag();
        }
        state.selecting = false;
        state.diag_selecting = false;
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn handle_main_mouse_input(
        &mut self,
        event_loop: &HostLoop,
        state: ElementState,
        button: winit::event::MouseButton,
    ) {
        let editor_was_focused = self.editor_has_input_focus();
        self.handle_main_mouse_input_inner(event_loop, state, button);
        self.autosave_after_editor_focus_change(editor_was_focused);
    }

    /// Window-free part of a sidebar slot click released without drag threshold.
    pub(crate) fn toggle_sidebar_panel_from_click(&mut self, panel_id: crate::app::PanelId) {
        let toggled_open = {
            let slot = self.ide_panel.slots.iter().find(|sl| sl.id == panel_id);
            slot.map(|s| !s.open).unwrap_or(false)
        };
        let toggled_group = {
            let slot = self.ide_panel.slots.iter().find(|sl| sl.id == panel_id);
            slot.map(|s| s.group.clone())
        };
        self.ide_panel.toggle(panel_id);
        if toggled_open && panel_id == crate::app::PanelId::Terminal {
            self.ide_panel.terminal_focused = true;
            self.ide_panel.term_search_focused = false;
            if self.ide_panel.terminals.is_empty() {
                self.add_terminal();
            }
            self.defer_terminal_panel_until_ready();
        }
        // При открытии Explorer — запускаем скан файлов
        if toggled_open && panel_id == crate::app::PanelId::Explorer {
            self.refresh_file_tree();
        }
        if toggled_open && panel_id == crate::app::PanelId::Search {
            self.ide_panel.project_search.focused =
                Some(crate::app::project_search::ProjectSearchField::Query);
        }
        // Взаимоисключение: при открытии кнопки закрываем остальные в той же группе
        if toggled_open {
            if let Some(group) = toggled_group {
                for sl in self.ide_panel.slots.iter_mut() {
                    if sl.id != panel_id && sl.group == group {
                        sl.open = false;
                    }
                }
            }
        }
        // Restored expanded connections wait for catalog loads started on panel open.
        if toggled_open && panel_id == crate::app::PanelId::Database {
            self.reconcile_expanded_database_connections();
        }
        // Same load as the `UiId::SidebarSlot` branch of `handle_ui_click`; a refresh already
        // in flight is not duplicated, `begin_status_refresh` marks a follow-up instead.
        if toggled_open && panel_id == crate::app::PanelId::Git {
            self.refresh_git_panel();
        }
    }

}

#[cfg(test)]
#[path = "input/input_tests.rs"]
mod tests;
#[cfg(all(test, target_os = "linux"))]
#[path = "input/input_modal_tests.rs"]
mod reviewer_stage2_modal_tests;
mod mouse_dispatch;
mod mouse_drag_capture;
mod mouse_overlay_press;
mod mouse_editor_terminal_press;
mod mouse_declarative_press;
mod mouse_ui_element_press;
