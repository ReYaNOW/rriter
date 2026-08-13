use super::*;

fn wheel_delta(delta: MouseScrollDelta, line_height: f32) -> (f32, f32) {
    match delta {
        MouseScrollDelta::LineDelta(x, y) => (-x * 4.0 * line_height, -y * 4.0 * line_height),
        MouseScrollDelta::PixelDelta(pos) => (-pos.x as f32, -pos.y as f32),
    }
}

fn autocomplete_max_scroll(total_items: usize, scale: f32) -> f32 {
    let step = 36.0 * scale;
    let total_items = total_items as f32;
    let visible_items = total_items.min(7.0);
    ((total_items - visible_items) * step).max(0.0)
}

fn scroll_autocomplete_list(
    scroll: &mut crate::scroll::ScrollState,
    dy: f32,
    total_items: usize,
    scale: f32,
) {
    scroll.anim_speed = 7.0;
    scroll.scroll_by(dy);
    scroll.clamp_target(0.0, autocomplete_max_scroll(total_items, scale));
}

fn scroll_database_dialog_form(
    scroll: &mut crate::scroll::ScrollState,
    dy: f32,
    max_scroll: f32,
    pointer_inside_viewport: bool,
) {
    if pointer_inside_viewport {
        scroll.anim_speed = 7.0;
        scroll.scroll_by(dy);
    }
    scroll.clamp_target(0.0, max_scroll.max(0.0));
}

fn git_changes_total_height(git: &crate::app::git_panel::GitPanelState, scale: f32) -> f32 {
    let mut total_h = 0.0;
    for workspace in &git.snapshot.workspaces {
        total_h += 30.0 * scale;
        if git.collapsed_workspaces.contains(&workspace.workspace_idx) {
            continue;
        }
        total_h += if workspace.error.is_some() {
            crate::render_view::tree_ui::TREE_ROW_H * scale
        } else {
            crate::app::git_panel::git_visible_tree_row_count(
                workspace.workspace_idx,
                &workspace.tree,
                &git.collapsed_dirs,
            ) as f32
                * crate::render_view::tree_ui::TREE_ROW_H
                * scale
        };
    }
    total_h
}

impl App {
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn handle_main_mouse_wheel(&mut self, delta: MouseScrollDelta) {
        self.lsp_actions_menu = None;
        let closed_git_menu = self.ide_panel.git.commit_menu_open()
            || self.ide_panel.git.commit_options_menu_open();
        self.ide_panel.git.close_commit_menus();
        let lh = self.renderer.as_ref().unwrap().line_height;
        let s = self.renderer.as_ref().unwrap().scale_factor;
        let shift = self.modifiers.shift_key();

        // Единая дельта как эталон для всех скролл-панелей в редакторе
        let (dx, dy) = wheel_delta(delta, lh);
        let mx = self.renderer.as_ref().unwrap().last_mouse_x;
        let my = self.renderer.as_ref().unwrap().last_mouse_y;
        if self.show_settings && self.tool_installer.is_log_open() {
            let max_scroll = self.tool_install_log_max_scroll();
            self.tool_installer.scroll_log_by(dy, max_scroll);
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            return;
        }
        if self.ide_panel.project_search.help_open {
            self.window.as_ref().unwrap().request_redraw();
            return;
        }
        if self.scroll_database_text_modal(dx, dy, shift) {
            self.window.as_ref().unwrap().request_redraw();
            return;
        }
        if let Some(modal) = self.ide_panel.database.table_modal.as_mut() {
            if let crate::app::database::DatabaseTableModal::Review { state, scroll, .. } = modal {
                scroll.anim_speed = 7.0;
                scroll.scroll_by(dy);
                let lines = state.summary.notices.len() + state.summary.detail_rows.len();
                let renderer = self.renderer.as_ref().unwrap();
                let max = crate::render_view::database_table_tab_overlay::database_table_review_max_scroll(
                    renderer.width,
                    renderer.height,
                    s,
                    lines,
                );
                scroll.clamp_target(0.0, max);
            }
            self.window.as_ref().unwrap().request_redraw();
            return;
        }
        if self.ide_panel.database.dialog.is_some() {
            let metrics = self.database_connection_dialog_scroll_metrics();
            if let (Some((form_clip, _, max_scroll, _)), Some(dialog)) =
                (metrics, self.ide_panel.database.dialog.as_mut())
            {
                scroll_database_dialog_form(
                    &mut dialog.scroll,
                    dy,
                    max_scroll,
                    form_clip.contains(mx, my),
                );
            }
            self.window.as_ref().unwrap().request_redraw();
            return;
        }
        if let Ok(mut ddl) = self.ide_panel.database.ddl_hover.try_borrow_mut()
            && let Some(state) = ddl.as_mut()
            && state
                .rect
                .is_some_and(|rect| crate::ui_system::point_in_rect(mx, my, rect))
        {
            state.popup.scroll.anim_speed = 7.0;
            state.popup.scroll.scroll_by(dy);
            state.popup.scroll.clamp_target(0.0, state.max_scroll);
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        if let Some(tab_id) = self.active_database_table_tab_id()
            && matches!(
                self.ui_registry.find_at(mx, my),
                Some(
                    crate::ui_system::UiId::DatabaseTableGridBody
                        | crate::ui_system::UiId::DatabaseTableCell(_, _)
                        | crate::ui_system::UiId::DatabaseGridRow(_)
                        | crate::ui_system::UiId::DatabaseTableHeader(_)
                        | crate::ui_system::UiId::DatabaseTableScrollY
                        | crate::ui_system::UiId::DatabaseTableScrollX
                )
            )
        {
            let grid_rect = self
                .ui_registry
                .rect_for(crate::ui_system::UiId::DatabaseTableGridBody);
            if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
                if let Some((_, _, width, height)) = grid_rect {
                    state.grid.viewport_width = (width / s - 54.0).max(0.0);
                    state.grid.viewport_height =
                        (height / s - crate::app::database::DATABASE_GRID_HEADER_HEIGHT).max(0.0);
                }
                if shift || dx.abs() > dy.abs() {
                    let amount = if shift { dy } else { dx } / s.max(0.001);
                    let max = state.metadata.as_ref().map_or(0.0, |metadata| {
                        (state.grid.content_width(metadata) - state.grid.viewport_width).max(0.0)
                    });
                    state.grid.scroll_x.anim_speed = 7.0;
                    state.grid.scroll_x.scroll_by(amount);
                    state.grid.scroll_x.clamp_target(0.0, max);
                } else {
                    let max = (state.grid.logical_row_count() as f32
                        * crate::app::database::DATABASE_GRID_ROW_HEIGHT
                        - state.grid.viewport_height)
                        .max(0.0);
                    state.grid.scroll_y.anim_speed = 7.0;
                    state.grid.scroll_y.scroll_by(dy / s.max(0.001));
                    state.grid.scroll_y.clamp_target(0.0, max);
                }
            }
            self.request_database_table_chunk_for_scroll(tab_id);
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        if self.api_python_runtime_overlay_active() {
            self.scroll_api_python_runtime_overlay(dy);
            self.window.as_ref().unwrap().request_redraw();
            return;
        }
        if self.inline_git_popup.is_some() {
            let hovered_id = self.ui_registry.find_at(mx, my);
            if matches!(
                hovered_id,
                Some(
                    crate::ui_system::UiId::InlineGitPanelBody
                        | crate::ui_system::UiId::InlineGitPrevHunk
                        | crate::ui_system::UiId::InlineGitNextHunk
                        | crate::ui_system::UiId::InlineGitRollbackHunk
                )
            ) {
                self.window.as_ref().unwrap().request_redraw();
                return;
            }
            self.inline_git_popup = None;
            self.inline_git_diff_rx = None;
            self.window.as_ref().unwrap().request_redraw();
            return;
        }
        if self.autocomplete_active {
            if let (Some(rect), Some(popup)) = (
                self.autocomplete_detail_rect,
                self.autocomplete_detail_popup.as_mut(),
            ) {
                if crate::ui_system::point_in_rect(mx, my, rect) {
                    popup.scroll.anim_speed = 7.0;
                    popup.scroll.scroll_by(dy);
                    popup
                        .scroll
                        .clamp_target(0.0, self.autocomplete_detail_max_scroll);
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
            }
            if let Some(rect) = self.autocomplete_rect {
                if crate::ui_system::point_in_rect(mx, my, rect) {
                    scroll_autocomplete_list(
                        &mut self.autocomplete_scroll,
                        dy,
                        self.autocomplete_options.len(),
                        s,
                    );
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
            }
            self.close_autocomplete();
            self.window.as_ref().unwrap().request_redraw();
            return;
        }
        let mut consumed_by_diag = false;
        HOVER_STATE.with(|state| {
            let mut state = state.borrow_mut();
            if let Some(rect) = state.diag_rect {
                if crate::ui_system::point_in_rect(mx, my, (rect.0, rect.1, rect.2, rect.3)) {
                    state.diag_scroll.anim_speed = 7.0;
                    state.diag_scroll.scroll_by(dy);
                    let max_scroll = state.diag_max_scroll;
                    state.diag_scroll.clamp_target(0.0, max_scroll);
                    consumed_by_diag = true;
                }
            }
        });
        if consumed_by_diag {
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        let mut consumed_by_hover = false;
        HOVER_STATE.with(|state| {
            let mut state = state.borrow_mut();
            if let Some(rect) = state.rect {
                if crate::ui_system::point_in_rect(mx, my, (rect.0, rect.1, rect.2, rect.3)) {
                    let max_scroll = state.max_scroll;
                    if let Some(popup) = &mut state.popup {
                        popup.scroll.anim_speed = 7.0;
                        popup.scroll.scroll_by(dy);
                        popup.scroll.clamp_target(0.0, max_scroll);
                        consumed_by_hover = true;
                    }
                }
            }
        });
        if consumed_by_hover {
            self.window.as_ref().unwrap().request_redraw();
            return;
        }
        if clear_hover_popup(self.renderer.as_mut()) {
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        // Скролл в области проводника файлов — перехватываем до всего остального
        if self.is_ide_mode && self.ide_panel.is_open(crate::app::PanelId::Explorer) {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            let mx = self.renderer.as_ref().unwrap().last_mouse_x;
            let my = self.renderer.as_ref().unwrap().last_mouse_y;
            let (cx, cy, cw, ch, _) = app_panel_scroll_rect(self, crate::app::PanelId::Explorer, s);

            if crate::ui_system::point_in_rect(mx, my, (cx, cy, cw, ch)) {
                self.ide_panel.explorer_scroll.anim_speed = 7.0;
                self.ide_panel.explorer_scroll.scroll_by(dy);
                let row_h = crate::render_view::tree_ui::TREE_ROW_H * s;
                let total_h = self.ide_panel.file_tree_nodes.len() as f32 * row_h;
                let max_scroll = (total_h - ch).max(0.0);
                self.ide_panel.explorer_scroll.clamp_target(0.0, max_scroll);
                self.window.as_ref().unwrap().request_redraw();
                return;
            }
        }

        if self.is_ide_mode && self.ide_panel.is_open(crate::app::PanelId::Search) {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            let mx = self.renderer.as_ref().unwrap().last_mouse_x;
            let my = self.renderer.as_ref().unwrap().last_mouse_y;
            let (cx, cy, cw, ch, _) = app_panel_scroll_rect(self, crate::app::PanelId::Search, s);
            if crate::ui_system::point_in_rect(mx, my, (cx, cy, cw, ch)) {
                if let Some(layout) = self.project_search_panel_layout() {
                    if crate::ui_system::point_in_rect(
                        mx,
                        my,
                        (
                            layout.query.x,
                            layout.query.y,
                            layout.query.w,
                            layout.query.h,
                        ),
                    ) {
                        self.ide_panel
                            .project_search
                            .scroll_query_y_by(layout.query, s, dy);
                    } else {
                        self.ide_panel.project_search.scroll.anim_speed = 7.0;
                        self.ide_panel.project_search.scroll.scroll_by(dy);
                        let max_scroll = self.ide_panel.project_search.max_scroll(layout.list.h, s);
                        self.ide_panel
                            .project_search
                            .scroll
                            .clamp_target(0.0, max_scroll);
                    }
                    self.window.as_ref().unwrap().request_redraw();
                }
                return;
            }
        }

        if self.is_ide_mode && self.ide_panel.is_open(crate::app::PanelId::Git) {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            let mx = self.renderer.as_ref().unwrap().last_mouse_x;
            let my = self.renderer.as_ref().unwrap().last_mouse_y;
            let (cx, cy, cw, ch, _) = app_panel_scroll_rect(self, crate::app::PanelId::Git, s);
            if crate::ui_system::point_in_rect(mx, my, (cx, cy, cw, ch)) {
                let controls_h = crate::app::git_panel::GIT_GRAPH_CONTROLS_H * s;
                let list_y = cy + controls_h;
                let full_list_h = (ch - controls_h).max(40.0 * s);
                let (changes_h, divider_h, bottom_h) = if self.ide_panel.git.bottom_pane
                    != crate::app::git_panel::GitBottomPane::Closed
                {
                    crate::app::git_panel::git_graph_split_heights(
                        full_list_h,
                        self.ide_panel.git.graph_height_ratio,
                        s,
                    )
                } else {
                    (full_list_h, 0.0, 0.0)
                };
                if self.ide_panel.git.graph_open() {
                    let graph_y = list_y + changes_h + divider_h;
                    let graph_header_h = 34.0 * s;
                    if crate::ui_system::point_in_rect(mx, my, (cx, graph_y, cw, graph_header_h)) {
                        let mut total_w = 0.0f32;
                        let renderer = self.renderer.as_mut().unwrap();
                        for workspace in self
                            .ide_panel
                            .git
                            .snapshot
                            .workspaces
                            .iter()
                            .filter(|workspace| workspace.repo_root.is_some())
                        {
                            let name = workspace
                                .root
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("workspace");
                            total_w += (renderer.measure_ui_width(name, 0.76) + 18.0 * s)
                                .max(48.0 * s)
                                + 6.0 * s;
                        }
                        let max_scroll = (total_w - (cw - 20.0 * s).max(0.0)).max(0.0);
                        self.ide_panel.git.graph_workspace_scroll_x =
                            (self.ide_panel.git.graph_workspace_scroll_x + dy)
                                .clamp(0.0, max_scroll);
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                    if crate::ui_system::point_in_rect(mx, my, (cx, graph_y, cw, bottom_h)) {
                        self.ide_panel.git.graph_scroll.anim_speed = 7.0;
                        self.ide_panel.git.graph_scroll.scroll_by(dy);
                        let rows_h = (bottom_h - graph_header_h).max(0.0);
                        let max_scroll = crate::app::git_panel::git_graph_max_scroll(
                            self.ide_panel.git.graph_snapshot.len(),
                            rows_h,
                            s,
                        );
                        self.ide_panel
                            .git
                            .graph_scroll
                            .clamp_target(0.0, max_scroll);
                        if self.ide_panel.git.graph_has_more
                            && crate::app::git_panel::git_graph_near_load_more(
                                self.ide_panel.git.graph_scroll.target,
                                max_scroll,
                                s,
                            )
                        {
                            self.load_more_git_graph_commits();
                        }
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                    if !crate::ui_system::point_in_rect(mx, my, (cx, list_y, cw, changes_h)) {
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                } else if self.ide_panel.git.logs_open() {
                    let logs_y = list_y + changes_h + divider_h;
                    if crate::ui_system::point_in_rect(mx, my, (cx, logs_y, cw, bottom_h)) {
                        let max_scroll = crate::app::git_panel::git_logs_max_scroll(
                            self.ide_panel.git.git_logs.line_count(),
                            bottom_h,
                            s,
                        );
                        self.ide_panel.git.scroll_git_logs_by(dy, max_scroll);
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                    if !crate::ui_system::point_in_rect(mx, my, (cx, list_y, cw, changes_h)) {
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                }
                self.ide_panel.git.scroll.anim_speed = 7.0;
                self.ide_panel.git.scroll.scroll_by(dy);
                let total_h = git_changes_total_height(&self.ide_panel.git, s);
                let max_scroll = (total_h - changes_h).max(0.0);
                self.ide_panel.git.scroll.clamp_target(0.0, max_scroll);
                self.window.as_ref().unwrap().request_redraw();
                return;
            }
        }
        if closed_git_menu {
            self.window.as_ref().unwrap().request_redraw();
        }

        if self.is_ide_mode && self.ide_panel.is_open(crate::app::PanelId::Database) {
            let (cx, cy, cw, ch, _) = app_panel_scroll_rect(self, crate::app::PanelId::Database, s);
            if crate::ui_system::point_in_rect(mx, my, (cx, cy, cw, ch)) {
                self.ide_panel.database.scroll.anim_speed = 7.0;
                self.ide_panel.database.scroll.scroll_by(dy);
                let max_scroll = self.ide_panel.database.max_tree_scroll(ch, s);
                self.ide_panel.database.scroll.clamp_target(0.0, max_scroll);
                self.window.as_ref().unwrap().request_redraw();
                return;
            }
        }

        if self.is_ide_mode && self.ide_panel.is_open(crate::app::PanelId::ApiClient) {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            let mx = self.renderer.as_ref().unwrap().last_mouse_x;
            let my = self.renderer.as_ref().unwrap().last_mouse_y;
            let (cx, cy, cw, ch, _) =
                app_panel_scroll_rect(self, crate::app::PanelId::ApiClient, s);
            if crate::ui_system::point_in_rect(mx, my, (cx, cy, cw, ch)) {
                self.ide_panel.api.panel_scroll.anim_speed = 7.0;
                self.ide_panel.api.panel_scroll.scroll_by(dy);
                let max_scroll =
                    crate::app::api_client::api_panel_max_scroll(&self.ide_panel.api, ch, s);
                self.ide_panel
                    .api
                    .panel_scroll
                    .clamp_target(0.0, max_scroll);
                self.window.as_ref().unwrap().request_redraw();
                return;
            }
        }

        if self.is_ide_mode && self.ide_panel.is_open(crate::app::PanelId::Problems) {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            let mx = self.renderer.as_ref().unwrap().last_mouse_x;
            let my = self.renderer.as_ref().unwrap().last_mouse_y;
            let Some(layout) = problems_scrollbar_layout(self, s) else {
                return;
            };

            if crate::ui_system::point_in_rect(
                mx,
                my,
                (
                    layout.content_x,
                    layout.content_y,
                    layout.content_w,
                    layout.content_h,
                ),
            ) {
                self.ide_panel.problems_scroll.anim_speed = 7.0;
                self.ide_panel.problems_scroll.scroll_by(dy);
                let max_scroll = (layout.total_h - layout.track_h).max(0.0);
                self.ide_panel.problems_scroll.clamp_target(0.0, max_scroll);
                self.window.as_ref().unwrap().request_redraw();
                return;
            }
        }

        if self.is_ide_mode && self.ide_panel.is_open(crate::app::PanelId::Terminal) {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            let mx = self.renderer.as_ref().unwrap().last_mouse_x;
            let my = self.renderer.as_ref().unwrap().last_mouse_y;
            let (cx, cy, cw, ch, _) = app_panel_scroll_rect(self, crate::app::PanelId::Terminal, s);

            if crate::ui_system::point_in_rect(mx, my, (cx, cy, cw, ch)) {
                if self.ide_panel.terminal_focused {
                    let active = self.ide_panel.active_terminal;
                    if let Some(term) = self.ide_panel.terminals.get_mut(active) {
                        let grid = crate::app::terminal::lock_terminal_grid(&term.grid);
                        let is_alt = grid.is_alt;
                        let app_cursor = grid.app_cursor_keys;
                        let mouse_tracking = grid.mouse_tracking;
                        let total_lines = grid.scrollback.len() + grid.lines.len();
                        drop(grid);

                        if is_alt {
                            let steps = (dy.abs() / 20.0).max(1.0) as usize;
                            let mut input = Vec::new();
                            if mouse_tracking {
                                let button = if dy < 0.0 { 64 } else { 65 };
                                let sequence = format!("\x1b[<{button};1;1M");
                                for _ in 0..steps.min(3) {
                                    input.extend_from_slice(sequence.as_bytes());
                                }
                            } else {
                                let sequence = if dy < 0.0 {
                                    if app_cursor { b"\x1BOA" } else { b"\x1B[A" }
                                } else if app_cursor {
                                    b"\x1BOB"
                                } else {
                                    b"\x1B[B"
                                };
                                for _ in 0..steps.min(3) {
                                    input.extend_from_slice(sequence);
                                }
                            }
                            let _ = term.write_input(&input);
                            return;
                        }

                        term.scroll_y.anim_speed = 7.0;
                        term.scroll_y.scroll_by(-dy); // -dy because scroll_y=0 is bottom

                        let lh = self.renderer.as_ref().unwrap().line_height;
                        let char_h = lh * crate::render_view::terminal_ui::TERMINAL_TEXT_SCALE;

                        let (_, term_content_h) =
                            crate::render_view::terminal_ui::terminal_body_rect(cy, ch, s);
                        let max_scroll = crate::render_view::terminal_ui::terminal_max_scroll(
                            total_lines,
                            char_h,
                            term_content_h,
                            s,
                        );

                        term.scroll_y.clamp_target(0.0, max_scroll);
                        self.window.as_ref().unwrap().request_redraw();
                    }
                    return;
                }
            }
        }

        if self.is_ide_mode && self.ide_panel.is_open(crate::app::PanelId::LspServers) {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            let mx = self.renderer.as_ref().unwrap().last_mouse_x;
            let my = self.renderer.as_ref().unwrap().last_mouse_y;
            if let Some((cx, cy, cw, ch)) = self.lsp_panel_bounds() {
                if crate::ui_system::point_in_rect(mx, my, (cx, cy, cw, ch)) {
                    let mut over_inner = false;
                    let scroll_y = self.ide_panel.lsp_scroll_y.current.round();
                    let mut current_y = cy + 8.0 * s - scroll_y;
                    for info in &self.ide_panel.lsp_servers {
                        let is_expanded = self.ide_panel.lsp_logs_expanded.contains(info.name);
                        let layout_logs_h = self.lsp_server_logs_h(info, s);
                        let row_h = 136.0 * s + layout_logs_h;

                        if is_expanded {
                            let (inner_total_h, inner_max_w) = self.lsp_server_inner_size(info, s);
                            let logs_h = crate::app::lsp_actions::lsp_server_logs_h_for_row(
                                inner_total_h,
                                cy,
                                ch,
                                current_y,
                                s,
                            );
                            if logs_h <= 0.0 {
                                current_y += row_h + 16.0 * s;
                                continue;
                            }
                            let btn_y1 = current_y + 56.0 * s;
                            let btn_h = 24.0 * s;
                            let btn_y2 = btn_y1 + btn_h + 8.0 * s;
                            let log_bg_y = btn_y2 + btn_h + 44.0 * s;
                            let log_bg_x = cx + 24.0 * s;
                            let log_bg_w = cw - 48.0 * s;
                            let log_bg_h = logs_h - 52.0 * s;

                            if crate::ui_system::point_in_rect(
                                mx,
                                my,
                                (log_bg_x, log_bg_y, log_bg_w, log_bg_h),
                            ) {
                                let name = info.name.to_string();

                                let inner_y = self
                                    .ide_panel
                                    .lsp_logs_scroll_y
                                    .entry(name.clone())
                                    .or_insert_with(|| crate::scroll::ScrollState::new(7.0));
                                inner_y.anim_speed = 7.0;
                                if !shift {
                                    inner_y.scroll_by(dy);
                                }
                                inner_y.clamp_target(0.0, (inner_total_h - log_bg_h).max(0.0));

                                let inner_x = self
                                    .ide_panel
                                    .lsp_logs_scroll_x
                                    .entry(name)
                                    .or_insert_with(|| crate::scroll::ScrollState::new(7.0));
                                inner_x.anim_speed = 7.0;
                                if shift {
                                    inner_x.scroll_by(dy);
                                } else {
                                    inner_x.scroll_by(dx);
                                }
                                inner_x.clamp_target(
                                    0.0,
                                    (inner_max_w + 20.0 * s - log_bg_w).max(0.0),
                                );

                                over_inner = true;
                                break;
                            }
                        }
                        current_y += row_h + 16.0 * s;
                    }

                    if !over_inner {
                        self.ide_panel.lsp_scroll_y.anim_speed = 7.0;
                        self.ide_panel.lsp_scroll_x.anim_speed = 7.0;
                        if shift {
                            self.ide_panel.lsp_scroll_x.scroll_by(dy);
                        } else {
                            self.ide_panel.lsp_scroll_y.scroll_by(dy);
                            self.ide_panel.lsp_scroll_x.scroll_by(dx);
                        }
                        let total_h = self.lsp_panel_total_h(s);
                        self.ide_panel
                            .lsp_scroll_y
                            .clamp_target(0.0, (total_h - ch).max(0.0));
                        self.ide_panel.lsp_scroll_x.clamp_target(0.0, 0.0);
                    }

                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
            }
        }

        if self.show_settings && self.settings_tab == 0 {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            let window_size = self.window.as_ref().unwrap().inner_size();
            let layout = crate::render_view::settings_ui::animated_settings_modal_layout(
                window_size.width as f32,
                window_size.height as f32,
                s,
                self.settings_anim_progress,
            );
            let pad_x = 12.0 * s;
            let max_scroll = crate::render_view::settings_ui::settings_ide_max_scroll(
                layout,
                self.ide_workspaces.len(),
                self.ide_ignore_patterns.iter().map(|p| {
                    self.renderer.as_mut().unwrap().measure_ui_width(p, 0.88)
                        + pad_x * 2.0
                        + 22.0 * s
                }),
                s,
            );

            if max_scroll > 0.0 {
                self.settings_ide_scroll.anim_speed = 7.0;
                self.settings_ide_scroll.scroll_by(dy);
                self.settings_ide_scroll.clamp_target(0.0, max_scroll);
                self.window.as_ref().unwrap().request_redraw();
            }
            return;
        }
        if self.show_settings && self.settings_tab == 4 {
            self.settings_scroll.anim_speed = 7.0;
            self.settings_scroll.scroll_by(dy);
            let window_size = self.window.as_ref().unwrap().inner_size();
            let layout = crate::render_view::settings_ui::animated_settings_modal_layout(
                window_size.width as f32,
                window_size.height as f32,
                s,
                self.settings_anim_progress,
            );
            let viewport_h =
                crate::render_view::settings_ui::settings_faq_viewport_height(layout, s);
            let max_scroll = self
                .renderer
                .as_mut()
                .unwrap()
                .get_faq_max_scroll(&self.faq_editor, viewport_h);
            self.settings_scroll.clamp_target(0.0, max_scroll);
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        if self.show_welcome || self.show_settings || self.dialog_window.is_some() {
            return;
        }

        let Some((s, mouse_x, mouse_y)) = self.renderer.as_ref().map(|renderer| {
            (
                renderer.scale_factor,
                renderer.last_mouse_x,
                renderer.last_mouse_y,
            )
        }) else {
            return;
        };
        let Some(window_size) = self.window.as_ref().map(|window| window.inner_size()) else {
            return;
        };
        let window_h = window_size.height as f32;
        let window_w = window_size.width as f32;
        if self
            .ui_registry
            .rect_for(crate::ui_system::UiId::DatabaseQueryReviewMessagesBody)
            .is_some_and(|rect| {
                mouse_x >= rect.0
                    && mouse_x <= rect.0 + rect.2
                    && mouse_y >= rect.1
                    && mouse_y <= rect.1 + rect.3
            })
        {
            if let Some(crate::app::EditorTabKind::DatabaseQuery(_, state)) =
                self.tabs.get_mut(self.active_tab).map(|tab| &mut tab.kind)
            {
                let max_scroll = state.result_view.review_message_max_scroll.get();
                state.result_view.review_message_scroll_y.anim_speed = 7.0;
                state.result_view.review_message_scroll_y.scroll_by(dy);
                state
                    .result_view
                    .review_message_scroll_y
                    .clamp_target(0.0, max_scroll);
            }
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            return;
        }
        let panel_bottom_h = if self.ide_panel.any_bottom_open() {
            self.ide_panel.bottom_height * s
        } else {
            0.0
        };
        let query_results_h = self
            .tabs
            .get(self.active_tab)
            .and_then(|tab| match &tab.kind {
                crate::app::EditorTabKind::DatabaseQuery(_, state)
                    if crate::app::database::database_query_results_visible(state) =>
                {
                    Some(crate::app::database::database_query_results_height(
                        state.result_view.preferred_height,
                        window_h,
                        panel_bottom_h,
                        s,
                    ))
                }
                _ => None,
            });
        if let Some(results_h) = query_results_h {
            let viewport = self
                .ui_registry
                .rect_for(crate::ui_system::UiId::DatabaseQueryResultBody);
            let over_viewport = viewport.is_some_and(|rect| {
                mouse_x >= rect.0
                    && mouse_x <= rect.0 + rect.2
                    && mouse_y >= rect.1
                    && mouse_y <= rect.1 + rect.3
            });
            if over_viewport {
                let viewport_w = viewport.map_or(window_w, |rect| rect.2).max(1.0);
                let viewport_h = viewport.map_or(results_h.max(1.0), |rect| rect.3.max(1.0));
                let history = self.ide_panel.database.persisted.query_history.clone();
                if let Some(crate::app::EditorTabKind::DatabaseQuery(meta, state)) =
                    self.tabs.get_mut(self.active_tab).map(|tab| &mut tab.kind)
                {
                    let (max_x, max_y) = crate::app::database::database_query_scroll_limits(
                        meta, state, &history, viewport_w, viewport_h, s,
                    );
                    if shift {
                        state.result_view.scroll_x.anim_speed = 7.0;
                        state.result_view.scroll_x.scroll_by(dy);
                        state.result_view.scroll_x.clamp_target(0.0, max_x);
                    } else {
                        state.result_view.scroll_y.anim_speed = 7.0;
                        state.result_view.scroll_y.scroll_by(dy);
                        state.result_view.scroll_y.clamp_target(0.0, max_y);
                        state.result_view.scroll_x.anim_speed = 7.0;
                        state.result_view.scroll_x.scroll_by(dx);
                        state.result_view.scroll_x.clamp_target(0.0, max_x);
                    }
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
                return;
            }
        }
        if self.database_blocking_modal_open() {
            return;
        }
        let tab_bar_h =
            crate::render_view::ide_tab_bar_height(self.show_welcome, self.is_ide_mode, s);

        let my = self.renderer.as_ref().unwrap().last_mouse_y;
        if my >= 0.0 && my <= tab_bar_h && !self.tabs.is_empty() {
            self.tab_scroll.anim_speed = 7.0;
            self.tab_scroll.scroll_by(dy);
            let max_scroll = self.renderer.as_ref().unwrap().max_tab_scroll_x;
            self.tab_scroll.clamp_target(0.0, max_scroll);
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        if self.active_tab_is_api_client() {
            let mx = self.renderer.as_ref().unwrap().last_mouse_x;
            let my = self.renderer.as_ref().unwrap().last_mouse_y;
            let hovered_id = self.ui_registry.find_at(mx, my);
            if self.ide_panel.api.mock_guide_open
                && matches!(
                    hovered_id,
                    Some(
                        crate::ui_system::UiId::ApiMockGuideBody
                            | crate::ui_system::UiId::ApiMockGuideScrollY
                    )
                )
            {
                if let Some((_, _, _, guide_h)) = self
                    .ui_registry
                    .rect_for(crate::ui_system::UiId::ApiMockGuideBody)
                {
                    let max_scroll = crate::app::api_client::api_mock_guide_max_scroll(guide_h, s);
                    self.ide_panel.api.mock_guide_scroll.anim_speed = 7.0;
                    self.ide_panel.api.mock_guide_scroll.scroll_by(dy);
                    self.ide_panel
                        .api
                        .mock_guide_scroll
                        .clamp_target(0.0, max_scroll);
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
            }
            if self.ide_panel.api.mock_server_detail_open
                && matches!(
                    hovered_id,
                    Some(
                        crate::ui_system::UiId::ApiMockServerLogArea
                            | crate::ui_system::UiId::ApiMockServerLogScrollY
                    )
                )
            {
                let rect = self
                    .ui_registry
                    .rect_for(crate::ui_system::UiId::ApiMockServerLogArea)
                    .or_else(|| hovered_id.and_then(|id| self.ui_registry.rect_for(id)));
                if let Some((_, _, _, log_h)) = rect {
                    let max_scroll = crate::app::api_client::api_mock_server_log_max_scroll(
                        self.ide_panel.api.mock_server_logs.len(),
                        log_h,
                        s,
                    );
                    self.ide_panel.api.mock_server_log_scroll.anim_speed = 7.0;
                    self.ide_panel.api.mock_server_log_scroll.scroll_by(dy);
                    self.ide_panel
                        .api
                        .mock_server_log_scroll
                        .clamp_target(0.0, max_scroll);
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
            }
            let api_inner_scroll = hovered_id.and_then(|id| {
                if let crate::ui_system::UiId::ApiOutputSchemaMenu(route_idx)
                | crate::ui_system::UiId::ApiOutputSchemaMenuItem(route_idx, _) = id
                {
                    let (meta, state) = self.active_api_tab()?;
                    if state.route_idx != Some(route_idx)
                        || !state.output_schema_menu_open
                        || state.output_doc_view
                            != crate::app::api_client::ApiOutputDocView::Example
                    {
                        return None;
                    }
                    let example_count = self
                        .ide_panel
                        .api
                        .models
                        .get(&meta.spec_id)
                        .and_then(|model| {
                            model.routes.get(route_idx).map(|route| {
                                crate::app::api_client::api_route_output_example_count(
                                    route,
                                    state.output_status_idx,
                                )
                            })
                        })
                        .unwrap_or(0)
                        .max(1);
                    let row_h = 30.0 * s;
                    let max_scroll = (example_count as f32 * row_h - row_h * 6.0).max(0.0);
                    return Some((meta.spec_id, route_idx, id, None, -max_scroll - 1.0));
                }
                let rect_id = match id {
                    crate::ui_system::UiId::ApiInputSchemaFold(route_idx, _) => {
                        crate::ui_system::UiId::ApiInputSchemaBody(route_idx)
                    }
                    crate::ui_system::UiId::ApiOutputSchemaFold(route_idx, _) => {
                        crate::ui_system::UiId::ApiOutputSchemaBody(route_idx)
                    }
                    _ => id,
                };
                self.ui_registry.rect_for(rect_id)?;
                let (spec_id, active_route_idx) = {
                    let (meta, state) = self.active_api_tab()?;
                    (meta.spec_id, state.route_idx)
                };
                match id {
                    crate::ui_system::UiId::ApiBodyInput(route_idx)
                    | crate::ui_system::UiId::ApiInputSchemaBody(route_idx)
                    | crate::ui_system::UiId::ApiInputSchemaFold(route_idx, _)
                        if active_route_idx == Some(route_idx) =>
                    {
                        let scroll_id = crate::ui_system::UiId::ApiBodyScrollY(route_idx);
                        let max_scroll = self.api_text_max_scroll_y_for_ui(scroll_id);
                        Some((
                            spec_id,
                            route_idx,
                            crate::ui_system::UiId::ApiBodyInput(route_idx),
                            None,
                            max_scroll,
                        ))
                    }
                    crate::ui_system::UiId::ApiOutputSchemaBody(route_idx)
                    | crate::ui_system::UiId::ApiOutputSchemaFold(route_idx, _)
                        if active_route_idx == Some(route_idx) =>
                    {
                        let scroll_id = crate::ui_system::UiId::ApiOutputScrollY(route_idx);
                        let max_scroll = self.api_text_max_scroll_y_for_ui(scroll_id);
                        Some((
                            spec_id,
                            route_idx,
                            crate::ui_system::UiId::ApiOutputSchemaBody(route_idx),
                            None,
                            max_scroll,
                        ))
                    }
                    crate::ui_system::UiId::ApiMockStaticResponseInput(route_idx)
                        if active_route_idx == Some(route_idx) =>
                    {
                        let scroll_id =
                            crate::ui_system::UiId::ApiMockStaticResponseScrollY(route_idx);
                        let max_scroll = self.api_text_max_scroll_y_for_ui(scroll_id);
                        Some((
                            spec_id,
                            route_idx,
                            crate::ui_system::UiId::ApiMockStaticResponseInput(route_idx),
                            None,
                            max_scroll,
                        ))
                    }
                    crate::ui_system::UiId::ApiResponseBody(route_idx)
                        if active_route_idx == Some(route_idx) =>
                    {
                        let scroll_id = crate::ui_system::UiId::ApiResponseScrollY(route_idx);
                        let max_scroll = self.api_text_max_scroll_y_for_ui(scroll_id);
                        Some((
                            spec_id,
                            route_idx,
                            crate::ui_system::UiId::ApiResponseBody(route_idx),
                            None,
                            max_scroll,
                        ))
                    }
                    crate::ui_system::UiId::ApiMockCombinedPython(route_idx)
                    | crate::ui_system::UiId::ApiMockContractInput(route_idx)
                    | crate::ui_system::UiId::ApiMockSignatureInput(route_idx)
                    | crate::ui_system::UiId::ApiMockPreludeInput(route_idx)
                    | crate::ui_system::UiId::ApiMockBodyInput(route_idx) => {
                        let focused_part = match self.ide_panel.api.focused {
                            Some(crate::app::api_client::ApiFocus::MockContract {
                                route_idx: focused_route,
                            }) if focused_route == route_idx => {
                                Some(crate::app::api_mock::ty_check::ApiMockSourcePart::Contract)
                            }
                            Some(crate::app::api_client::ApiFocus::MockPrelude {
                                route_idx: focused_route,
                            }) if focused_route == route_idx => {
                                Some(crate::app::api_mock::ty_check::ApiMockSourcePart::Prelude)
                            }
                            Some(crate::app::api_client::ApiFocus::MockBody {
                                route_idx: focused_route,
                            }) if focused_route == route_idx => {
                                Some(crate::app::api_mock::ty_check::ApiMockSourcePart::Body)
                            }
                            Some(crate::app::api_client::ApiFocus::MockSignature {
                                route_idx: focused_route,
                            }) if focused_route == route_idx => {
                                Some(crate::app::api_mock::ty_check::ApiMockSourcePart::Signature)
                            }
                            _ => None,
                        };
                        focused_part?;
                        let route = self
                            .ide_panel
                            .api
                            .models
                            .get(&spec_id)
                            .and_then(|model| model.routes.get(route_idx))?;
                        let script = self
                            .ide_panel
                            .api
                            .mock
                            .route_overrides
                            .iter()
                            .find(|item| item.method == route.method && item.path == route.path)
                            .and_then(|item| item.python.as_ref())?;
                        let prelude_text = if focused_part
                            == Some(crate::app::api_mock::ty_check::ApiMockSourcePart::Prelude)
                        {
                            self.ide_panel.api.input_editor.get_full_text()
                        } else {
                            script.prelude.clone()
                        };
                        let contract_text = if focused_part
                            == Some(crate::app::api_mock::ty_check::ApiMockSourcePart::Contract)
                        {
                            self.ide_panel.api.input_editor.get_full_text()
                        } else {
                            self.api_mock_contract_source_for_route(route_idx)
                                .unwrap_or_default()
                        };
                        let body_text = if focused_part
                            == Some(crate::app::api_mock::ty_check::ApiMockSourcePart::Body)
                        {
                            self.ide_panel.api.input_editor.get_full_text()
                        } else {
                            crate::app::api_client::api_mock_body_editor_text(&script.body)
                        };
                        let model = self.ide_panel.api.models.get(&spec_id)?;
                        let contract = crate::app::api_mock::types::api_mock_effective_contract(
                            script, route, model,
                        );
                        let signature_text =
                            crate::app::api_mock::contract::api_mock_handler_signature_text(
                                &contract,
                            );
                        let content_h =
                            crate::app::api_client::api_mock_combined_editor_content_height(
                                &prelude_text,
                                &contract_text,
                                &signature_text,
                                &body_text,
                                s,
                            );
                        let viewport_h =
                            crate::app::api_client::api_mock_combined_editor_viewport_height(
                                &signature_text,
                                s,
                            );
                        Some((
                            spec_id,
                            route_idx,
                            crate::ui_system::UiId::ApiMockBodyInput(route_idx),
                            Some(crate::app::api_mock::ty_check::ApiMockSourcePart::Body),
                            (content_h - viewport_h).max(0.0),
                        ))
                    }
                    _ => None,
                }
            });
            if let Some((spec_id, route_idx, scroll_target, mock_part, max_scroll)) =
                api_inner_scroll
            {
                if max_scroll < 0.0 {
                    let menu_max_scroll = (-max_scroll - 1.0).max(0.0);
                    if let Some((_, state)) = self.active_api_tab_mut_for(spec_id)
                        && state.route_idx == Some(route_idx)
                    {
                        state.output_schema_menu_scroll.anim_speed = 7.0;
                        state.output_schema_menu_scroll.scroll_by(dy);
                        state
                            .output_schema_menu_scroll
                            .clamp_target(0.0, menu_max_scroll);
                        self.window.as_ref().unwrap().request_redraw();
                    }
                    return;
                }
                let horizontal_delta = if shift { dy } else { dx };
                let prefer_horizontal = shift || dx.abs() > dy.abs();
                if mock_part.is_none() && prefer_horizontal && horizontal_delta.abs() > 0.0 {
                    let scroll_id = match scroll_target {
                        crate::ui_system::UiId::ApiBodyInput(_)
                        | crate::ui_system::UiId::ApiInputSchemaBody(_) => {
                            crate::ui_system::UiId::ApiBodyScrollX(route_idx)
                        }
                        crate::ui_system::UiId::ApiOutputSchemaBody(_) => {
                            crate::ui_system::UiId::ApiOutputScrollX(route_idx)
                        }
                        crate::ui_system::UiId::ApiMockStaticResponseInput(_) => {
                            crate::ui_system::UiId::ApiMockStaticResponseScrollX(route_idx)
                        }
                        crate::ui_system::UiId::ApiResponseBody(_) => {
                            crate::ui_system::UiId::ApiResponseScrollX(route_idx)
                        }
                        _ => return,
                    };
                    let max_scroll_x = self.api_text_max_scroll_x_for_ui(scroll_id);
                    if max_scroll_x > 0.0
                        && let Some((_, state)) = self.active_api_tab_mut_for(spec_id)
                        && state.route_idx == Some(route_idx)
                    {
                        let scroll = match scroll_target {
                            crate::ui_system::UiId::ApiBodyInput(_)
                            | crate::ui_system::UiId::ApiInputSchemaBody(_) => {
                                &mut state.body_scroll_x
                            }
                            crate::ui_system::UiId::ApiOutputSchemaBody(_) => {
                                &mut state.output_scroll_x
                            }
                            crate::ui_system::UiId::ApiMockStaticResponseInput(_) => {
                                &mut state.mock_static_response_scroll_x
                            }
                            crate::ui_system::UiId::ApiResponseBody(_) => {
                                &mut state.response_scroll_x
                            }
                            _ => return,
                        };
                        scroll.anim_speed = 7.0;
                        scroll.scroll_by(horizontal_delta);
                        scroll.clamp_target(0.0, max_scroll_x);
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                }
                let force_page_scroll = shift && mock_part.is_some();
                if max_scroll > 0.0 && !force_page_scroll {
                    if let Some(part) = mock_part {
                        let scroll = self
                            .ide_panel
                            .api
                            .mock_python_scrolls
                            .entry((route_idx, part))
                            .or_insert_with(|| crate::scroll::ScrollState::new(7.0));
                        scroll.anim_speed = 7.0;
                        scroll.scroll_by(dy);
                        scroll.clamp_target(0.0, max_scroll);
                    } else if let Some((_, state)) = self.active_api_tab_mut_for(spec_id)
                        && state.route_idx == Some(route_idx)
                    {
                        let scroll = match scroll_target {
                            crate::ui_system::UiId::ApiBodyInput(_)
                            | crate::ui_system::UiId::ApiInputSchemaBody(_) => {
                                &mut state.body_scroll
                            }
                            crate::ui_system::UiId::ApiOutputSchemaBody(_) => {
                                &mut state.output_scroll
                            }
                            crate::ui_system::UiId::ApiMockStaticResponseInput(_) => {
                                &mut state.mock_static_response_scroll
                            }
                            crate::ui_system::UiId::ApiResponseBody(_) => {
                                &mut state.response_scroll
                            }
                            _ => return,
                        };
                        scroll.anim_speed = 7.0;
                        scroll.scroll_by(dy);
                        scroll.clamp_target(0.0, max_scroll);
                    }
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
            }
            let wh = self.window.as_ref().unwrap().inner_size().height as f32;
            let status_h = crate::render_view::ide_status_bar_height(s);
            let visible_h = (wh - tab_bar_h - status_h).max(0.0);
            let max_scroll = self
                .tabs
                .get(self.active_tab)
                .and_then(|tab| match &tab.kind {
                    crate::app::EditorTabKind::ApiClient(meta, state) => {
                        let manual_model;
                        let model = match &meta.route_identity {
                            Some(crate::app::api_client::ApiClientRouteIdentity::Manual {
                                stable_id,
                            }) => {
                                let route = self
                                    .ide_panel
                                    .api
                                    .mock
                                    .manual_routes
                                    .iter()
                                    .find(|route| route.stable_id == *stable_id)?;
                                manual_model =
                                    crate::app::api_client::api_manual_route_model(route);
                                Some(&manual_model)
                            }
                            _ => self.ide_panel.api.models.get(&meta.spec_id),
                        };
                        Some(crate::app::api_client::api_tab_max_scroll(
                            model,
                            state,
                            Some(&self.ide_panel.api),
                            visible_h,
                            s,
                        ))
                    }
                    _ => None,
                })
                .unwrap_or(0.0);
            if let Some(crate::app::EditorTabKind::ApiClient(_, state)) =
                self.tabs.get_mut(self.active_tab).map(|tab| &mut tab.kind)
            {
                state.tab_scroll.anim_speed = 7.0;
                state.tab_scroll.scroll_by(dy);
                state.tab_scroll.clamp_target(0.0, max_scroll);
                self.window.as_ref().unwrap().request_redraw();
            }
            return;
        }

        self.scroll_y.anim_speed = 7.0;
        self.scroll_x.anim_speed = 7.0;

        // При скролле основного редактора hover-popup с типом должен скрываться,
        // так же как исчезает popup с диагностикой.
        clear_hover_popup(self.renderer.as_mut());

        if shift {
            self.scroll_x.scroll_by(dy); // Shift конвертирует вертикальный скролл в горизонтальный
        } else {
            self.scroll_y.scroll_by(dy);
            self.scroll_x.scroll_by(dx);
        }

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
        let max_scroll_y = self
            .renderer
            .as_mut()
            .unwrap()
            .get_max_scroll(&self.editor, visible_h);
        let max_scroll_x = self.renderer.as_ref().unwrap().max_scroll_x;

        self.scroll_y.clamp_target(0.0, max_scroll_y);
        self.scroll_y.target = self.scroll_y.target.round();
        self.scroll_x.clamp_target(0.0, max_scroll_x);
        self.scroll_x.target = self.scroll_x.target.round();
        self.window.as_ref().unwrap().request_redraw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_delta_handles_line_and_pixel_units() {
        assert_eq!(
            wheel_delta(MouseScrollDelta::LineDelta(2.0, -3.0), 10.0),
            (-80.0, 120.0)
        );
        assert_eq!(
            wheel_delta(
                MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition::new(12.5, -8.0)),
                10.0,
            ),
            (-12.5, 8.0)
        );
    }

    #[test]
    fn point_in_rect_uses_inclusive_edges() {
        let rect = (10.0, 20.0, 30.0, 40.0);
        assert!(crate::ui_system::point_in_rect(10.0, 20.0, rect));
        assert!(crate::ui_system::point_in_rect(40.0, 60.0, rect));
        assert!(crate::ui_system::point_in_rect(25.0, 45.0, rect));
        assert!(!crate::ui_system::point_in_rect(9.9, 45.0, rect));
        assert!(!crate::ui_system::point_in_rect(25.0, 60.1, rect));
    }

    #[test]
    fn panel_scroll_rect_covers_top_and_bottom_layouts() {
        assert_eq!(
            panel_scroll_rect(true, 2.0, 96.0, 240.0, 360.0, 1600.0, 1000.0),
            (96.0, 64.0, 480.0, 516.0)
        );
        assert_eq!(
            panel_scroll_rect(false, 2.0, 96.0, 240.0, 360.0, 1600.0, 1000.0),
            (96.0, 645.0, 1504.0, 295.0)
        );
    }

    #[test]
    fn git_changes_total_height_keeps_disabled_workspaces_scrollable_when_staged() {
        let mut state = crate::app::git_panel::GitPanelState::default();
        state.snapshot = crate::app::git_panel::GitStatusSnapshot {
            workspaces: vec![
                git_workspace_for_wheel_test(0, true),
                git_workspace_for_wheel_test(1, false),
            ],
        };

        assert_eq!(state.staged_workspace_lock(), Some(0));
        assert_eq!(
            git_changes_total_height(&state, 1.0),
            60.0 + crate::render_view::tree_ui::TREE_ROW_H * 2.0
        );
    }

    #[test]
    fn autocomplete_max_scroll_matches_visible_limit() {
        assert_eq!(autocomplete_max_scroll(0, 1.0), 0.0);
        assert_eq!(autocomplete_max_scroll(7, 1.0), 0.0);
        assert_eq!(autocomplete_max_scroll(10, 1.0), 108.0);
        assert_eq!(autocomplete_max_scroll(9, 2.0), 144.0);
    }

    #[test]
    fn scroll_autocomplete_list_clamps_without_closing_state() {
        let mut scroll = crate::scroll::ScrollState::new(15.0);
        scroll_autocomplete_list(&mut scroll, 300.0, 10, 1.0);
        assert_eq!(scroll.target, 108.0);

        scroll_autocomplete_list(&mut scroll, -500.0, 10, 1.0);
        assert_eq!(scroll.target, 0.0);
    }

    #[test]
    fn database_dialog_wheel_only_moves_inside_scrollable_form_viewport() {
        let mut scroll = crate::scroll::ScrollState::new(15.0);
        scroll_database_dialog_form(&mut scroll, 120.0, 400.0, false);
        assert_eq!(scroll.target, 0.0);

        scroll_database_dialog_form(&mut scroll, 120.0, 400.0, true);
        assert_eq!(scroll.target, 120.0);
        assert_eq!(scroll.current, 0.0);
        assert_eq!(scroll.anim_speed, 7.0);
        assert!(scroll.update(0.016));
        assert!(scroll.current > 0.0 && scroll.current < scroll.target);

        scroll_database_dialog_form(&mut scroll, 500.0, 400.0, true);
        assert_eq!(scroll.target, 400.0);
    }

    fn git_workspace_for_wheel_test(
        workspace_idx: usize,
        staged: bool,
    ) -> crate::app::git_panel::GitWorkspaceStatus {
        let rel_path = format!("src/file_{workspace_idx}.rs");
        crate::app::git_panel::GitWorkspaceStatus {
            workspace_idx,
            root: std::path::PathBuf::from(format!("/repo_{workspace_idx}")),
            repo_root: Some(std::path::PathBuf::from(format!("/repo_{workspace_idx}"))),
            branch_name: None,
            files: vec![crate::app::git_panel::GitFileEntry {
                workspace_idx,
                rel_path: rel_path.clone().into_boxed_str(),
                old_rel_path: None,
                display_path: rel_path.clone().into_boxed_str(),
                depth: 0,
                staged,
                status: crate::app::git_panel::GitFileStatus::Modified,
            }],
            tree: vec![crate::app::git_panel::GitTreeRow {
                name: rel_path.into_boxed_str(),
                path: format!("src/file_{workspace_idx}.rs").into_boxed_str(),
                depth: 0,
                file_idx: Some(0),
                icon_key: "default_file",
            }],
            ahead: 0,
            error: None,
        }
    }

    #[test]
    fn settings_ide_max_scroll_counts_chip_wrapping() {
        assert_eq!(
            crate::render_view::settings_ui::settings_ide_max_scroll(
                crate::render_view::settings_ui::settings_modal_layout(1000.0, 900.0, 1.0),
                0,
                std::iter::empty(),
                1.0,
            ),
            0.0
        );

        let layout = crate::render_view::settings_ui::settings_modal_layout(1000.0, 500.0, 1.0);
        let no_wrap = crate::render_view::settings_ui::settings_ide_max_scroll(
            layout,
            2,
            [100.0, 120.0],
            1.0,
        );
        let wrapped = crate::render_view::settings_ui::settings_ide_max_scroll(
            layout,
            2,
            [430.0, 120.0, 450.0],
            1.0,
        );

        assert!(no_wrap > 0.0);
        assert!(wrapped > no_wrap);
    }
}
