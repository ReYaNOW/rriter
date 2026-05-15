use super::*;

fn wheel_delta(delta: MouseScrollDelta, line_height: f32) -> (f32, f32) {
    match delta {
        MouseScrollDelta::LineDelta(x, y) => (-x * 4.0 * line_height, -y * 4.0 * line_height),
        MouseScrollDelta::PixelDelta(pos) => (-pos.x as f32, -pos.y as f32),
    }
}

fn point_in_rect(mx: f32, my: f32, rect: (f32, f32, f32, f32)) -> bool {
    mx >= rect.0 && mx <= rect.0 + rect.2 && my >= rect.1 && my <= rect.1 + rect.3
}

fn panel_scroll_rect(
    is_top: bool,
    scale: f32,
    sidebar_w: f32,
    left_width: f32,
    bottom_height: f32,
    effective_bottom_height: f32,
    window_width: f32,
    window_height: f32,
) -> (f32, f32, f32, f32) {
    let title_h = 32.0 * scale;
    if is_top {
        (
            sidebar_w,
            title_h,
            left_width * scale,
            window_height
                - title_h
                - effective_bottom_height
                - crate::render_view::ide_status_bar_height(scale),
        )
    } else {
        let tab_h = 32.0 * scale;
        let panel_y = crate::render_view::ide_bottom_panel_y(window_height, bottom_height, scale);
        (
            sidebar_w,
            panel_y + 1.0 + tab_h,
            window_width - sidebar_w,
            bottom_height - 1.0 - tab_h,
        )
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

fn settings_ide_max_scroll(
    workspace_count: usize,
    ignore_chip_widths: impl IntoIterator<Item = f32>,
    scale: f32,
    window_height: f32,
) -> f32 {
    let ide_h = (700.0 * scale).min(window_height - 40.0 * scale);
    let ih = ide_h - 35.0 * scale - 30.0 * scale;
    let ide_content_area_h = ih - 52.0 * scale;

    let workspace_h = workspace_count as f32 * 46.0 * scale + 126.0 * scale;
    let chip_h = 28.0 * scale;
    let chip_gap_y = 8.0 * scale;
    let chip_gap_x = 8.0 * scale;
    let max_row_w = 460.0 * scale;

    let mut chip_rows = 1usize;
    let mut cx = 0.0f32;
    for cw in ignore_chip_widths {
        if cx + cw > max_row_w && cx > 0.0 {
            chip_rows += 1;
            cx = 0.0;
        }
        cx += cw + chip_gap_x;
    }

    let ignore_h = 200.0 * scale + chip_rows as f32 * (chip_h + chip_gap_y);
    let ide_total_h = workspace_h + ignore_h;
    (ide_total_h - ide_content_area_h).max(0.0)
}

impl App {
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn handle_main_mouse_wheel(&mut self, delta: MouseScrollDelta) {
        self.lsp_actions_menu = None;
        let closed_git_menu = self.ide_panel.git.commit_menu_open;
        self.ide_panel.git.commit_menu_open = false;
        let lh = self.renderer.as_ref().unwrap().line_height;
        let s = self.renderer.as_ref().unwrap().scale_factor;
        let shift = self.modifiers.shift_key();

        // Единая дельта как эталон для всех скролл-панелей в редакторе
        let (dx, dy) = wheel_delta(delta, lh);
        let mx = self.renderer.as_ref().unwrap().last_mouse_x;
        let my = self.renderer.as_ref().unwrap().last_mouse_y;
        if self.autocomplete_active {
            if let (Some(rect), Some(popup)) = (
                self.autocomplete_detail_rect,
                self.autocomplete_detail_popup.as_mut(),
            ) {
                if point_in_rect(mx, my, rect) {
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
                if point_in_rect(mx, my, rect) {
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
                if point_in_rect(mx, my, (rect.0, rect.1, rect.2, rect.3)) {
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
                if point_in_rect(mx, my, (rect.0, rect.1, rect.2, rect.3)) {
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
            let sb_w = 48.0 * s;
            let mx = self.renderer.as_ref().unwrap().last_mouse_x;
            let my = self.renderer.as_ref().unwrap().last_mouse_y;
            let title_h = 32.0 * s;
            let wh = self.window.as_ref().unwrap().inner_size().height as f32;
            let panel_bottom_h = if self.ide_panel.any_bottom_open() {
                self.ide_panel.bottom_height * s
            } else {
                0.0
            };
            let is_top = self.ide_panel.slots.iter().any(|sl| {
                sl.id == crate::app::PanelId::Explorer && sl.group == crate::app::PanelGroup::Top
            });

            let mut effective_bottom_h = panel_bottom_h;
            if self.ide_panel.is_open(crate::app::PanelId::Terminal)
                && !self.ide_panel.terminal_focused
            {
                effective_bottom_h = 0.0;
            }

            let ww = self.window.as_ref().unwrap().inner_size().width as f32;
            let (cx, cy, cw, ch) = panel_scroll_rect(
                is_top,
                s,
                sb_w,
                self.ide_panel.left_width,
                panel_bottom_h,
                effective_bottom_h,
                ww,
                wh,
            );

            if point_in_rect(mx, my, (cx, cy, cw, ch)) {
                self.ide_panel.explorer_scroll.anim_speed = 7.0;
                self.ide_panel.explorer_scroll.scroll_by(dy);
                let row_h = 28.0 * s;
                let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                let total_h = self.ide_panel.file_tree_nodes.len() as f32 * row_h;
                let max_scroll = (total_h - (wh - title_h)).max(0.0);
                self.ide_panel.explorer_scroll.clamp_target(0.0, max_scroll);
                self.window.as_ref().unwrap().request_redraw();
                return;
            }
        }

        if self.is_ide_mode && self.ide_panel.is_open(crate::app::PanelId::Git) {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            let sb_w = 48.0 * s;
            let mx = self.renderer.as_ref().unwrap().last_mouse_x;
            let my = self.renderer.as_ref().unwrap().last_mouse_y;
            let wh = self.window.as_ref().unwrap().inner_size().height as f32;
            let panel_bottom_h = if self.ide_panel.any_bottom_open() {
                self.ide_panel.bottom_height * s
            } else {
                0.0
            };
            let is_top = self.ide_panel.slots.iter().any(|sl| {
                sl.id == crate::app::PanelId::Git && sl.group == crate::app::PanelGroup::Top
            });
            let ww = self.window.as_ref().unwrap().inner_size().width as f32;
            let (cx, cy, cw, ch) = panel_scroll_rect(
                is_top,
                s,
                sb_w,
                self.ide_panel.left_width,
                panel_bottom_h,
                panel_bottom_h,
                ww,
                wh,
            );
            if point_in_rect(mx, my, (cx, cy, cw, ch)) {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.suppress_popups_until_next_mouse_move();
                    renderer.reset_git_file_tooltip_overlay();
                }
                let controls_h = crate::app::git_panel::GIT_GRAPH_CONTROLS_H * s;
                let list_y = cy + controls_h;
                let full_list_h = (ch - controls_h).max(40.0 * s);
                let (changes_h, divider_h, graph_h) = if self.ide_panel.git.graph_open {
                    crate::app::git_panel::git_graph_split_heights(
                        full_list_h,
                        self.ide_panel.git.graph_height_ratio,
                        s,
                    )
                } else {
                    (full_list_h, 0.0, 0.0)
                };
                if self.ide_panel.git.graph_open {
                    let graph_y = list_y + changes_h + divider_h;
                    let graph_header_h = 34.0 * s;
                    if point_in_rect(mx, my, (cx, graph_y, cw, graph_header_h)) {
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
                    if point_in_rect(mx, my, (cx, graph_y, cw, graph_h)) {
                        self.ide_panel.git.graph_scroll.anim_speed = 7.0;
                        self.ide_panel.git.graph_scroll.scroll_by(dy);
                        let rows_h = (graph_h - graph_header_h).max(0.0);
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
                            && self.ide_panel.git.graph_scroll.target
                                >= (max_scroll - crate::app::git_panel::GIT_GRAPH_ROW_H * s * 3.0)
                                    .max(0.0)
                        {
                            self.load_more_git_graph_commits();
                        }
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                    if !point_in_rect(mx, my, (cx, list_y, cw, changes_h)) {
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }
                }
                self.ide_panel.git.scroll.anim_speed = 7.0;
                self.ide_panel.git.scroll.scroll_by(dy);
                let mut total_h = 0.0;
                let staged_workspace = self.ide_panel.git.staged_workspace_lock();
                for workspace in &self.ide_panel.git.snapshot.workspaces {
                    if staged_workspace.is_some_and(|idx| idx != workspace.workspace_idx) {
                        continue;
                    }
                    if staged_workspace.is_none()
                        && workspace.files.is_empty()
                        && workspace.error.is_none()
                        && workspace.ahead == 0
                    {
                        continue;
                    }
                    total_h += 30.0 * s;
                    total_h += if workspace.error.is_some() {
                        crate::render_view::tree_ui::TREE_ROW_H * s
                    } else {
                        crate::app::git_panel::git_visible_tree_row_count(
                            workspace.workspace_idx,
                            &workspace.tree,
                            &self.ide_panel.git.collapsed_dirs,
                        ) as f32
                            * crate::render_view::tree_ui::TREE_ROW_H
                            * s
                    };
                }
                let max_scroll = (total_h - changes_h).max(0.0);
                self.ide_panel.git.scroll.clamp_target(0.0, max_scroll);
                self.window.as_ref().unwrap().request_redraw();
                return;
            }
        }
        if closed_git_menu {
            self.window.as_ref().unwrap().request_redraw();
        }

        if self.is_ide_mode && self.ide_panel.is_open(crate::app::PanelId::Problems) {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            let mx = self.renderer.as_ref().unwrap().last_mouse_x;
            let my = self.renderer.as_ref().unwrap().last_mouse_y;
            let wh = self.window.as_ref().unwrap().inner_size().height as f32;

            let is_top = self.ide_panel.slots.iter().any(|sl| {
                sl.id == crate::app::PanelId::Problems && sl.group == crate::app::PanelGroup::Top
            });
            let sb_w = 48.0 * s;
            let panel_bottom_h = if self.ide_panel.any_bottom_open() {
                self.ide_panel.bottom_height * s
            } else {
                0.0
            };

            let mut effective_bottom_h = panel_bottom_h;
            if self.ide_panel.is_open(crate::app::PanelId::Terminal)
                && !self.ide_panel.terminal_focused
            {
                effective_bottom_h = 0.0;
            }

            let ww = self.window.as_ref().unwrap().inner_size().width as f32;
            let (cx, cy, cw, ch) = panel_scroll_rect(
                is_top,
                s,
                sb_w,
                self.ide_panel.left_width,
                panel_bottom_h,
                effective_bottom_h,
                ww,
                wh,
            );

            if point_in_rect(mx, my, (cx, cy, cw, ch)) {
                self.ide_panel.problems_scroll.anim_speed = 7.0;
                self.ide_panel.problems_scroll.scroll_by(dy);
                let row_h = 24.0 * s;
                let total_h = self.ide_panel.flat_diags.len() as f32 * row_h;
                let track_h = ch - 40.0 * s;
                let max_scroll = (total_h - track_h).max(0.0);
                self.ide_panel.problems_scroll.clamp_target(0.0, max_scroll);
                self.window.as_ref().unwrap().request_redraw();
                return;
            }
        }

        if self.is_ide_mode && self.ide_panel.is_open(crate::app::PanelId::Terminal) {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            let mx = self.renderer.as_ref().unwrap().last_mouse_x;
            let my = self.renderer.as_ref().unwrap().last_mouse_y;
            let wh = self.window.as_ref().unwrap().inner_size().height as f32;

            let is_top = self.ide_panel.slots.iter().any(|sl| {
                sl.id == crate::app::PanelId::Terminal && sl.group == crate::app::PanelGroup::Top
            });
            let sb_w = 48.0 * s;
            let panel_bottom_h = if self.ide_panel.any_bottom_open() {
                self.ide_panel.bottom_height * s
            } else {
                0.0
            };

            let mut effective_bottom_h = panel_bottom_h;
            if self.ide_panel.is_open(crate::app::PanelId::Terminal)
                && !self.ide_panel.terminal_focused
            {
                effective_bottom_h = 0.0;
            }

            let ww = self.window.as_ref().unwrap().inner_size().width as f32;
            let (cx, cy, cw, ch) = panel_scroll_rect(
                is_top,
                s,
                sb_w,
                self.ide_panel.left_width,
                panel_bottom_h,
                effective_bottom_h,
                ww,
                wh,
            );

            if point_in_rect(mx, my, (cx, cy, cw, ch)) {
                if self.ide_panel.terminal_focused {
                    let active = self.ide_panel.active_terminal;
                    if let Some(term) = self.ide_panel.terminals.get_mut(active) {
                        let grid = term.grid.lock().unwrap();
                        let is_alt = grid.is_alt;
                        let app_cursor = grid.app_cursor_keys;
                        let mouse_tracking = grid.mouse_tracking;
                        let total_lines = grid.scrollback.len() + grid.lines.len();
                        drop(grid);

                        if is_alt {
                            if let Ok(mut w) = term.writer.lock() {
                                if mouse_tracking {
                                    let btn = if dy < 0.0 { 64 } else { 65 };
                                    let seq = format!("\x1b[<{};1;1M", btn);
                                    let steps = (dy.abs() / 20.0).max(1.0) as usize;
                                    for _ in 0..steps.min(3) {
                                        let _ = w.write_all(seq.as_bytes());
                                    }
                                } else {
                                    let seq = if dy < 0.0 {
                                        if app_cursor { b"\x1BOA" } else { b"\x1B[A" }
                                    } else {
                                        if app_cursor { b"\x1BOB" } else { b"\x1B[B" }
                                    };
                                    let steps = (dy.abs() / 20.0).max(1.0) as usize;
                                    for _ in 0..steps.min(3) {
                                        let _ = w.write_all(seq);
                                    }
                                }
                                let _ = w.flush();
                            }
                            return;
                        }

                        term.scroll_y.anim_speed = 7.0;
                        term.scroll_y.scroll_by(-dy); // -dy because scroll_y=0 is bottom

                        let lh = self.renderer.as_ref().unwrap().line_height;
                        let term_scale = 1.05;
                        let char_h = lh * term_scale;

                        let (_, term_content_h) =
                            crate::render_view::terminal_ui::terminal_body_rect(cy, ch, s);
                        let max_scroll = ((total_lines as f32 * char_h) - term_content_h).max(0.0);

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
                if point_in_rect(mx, my, (cx, cy, cw, ch)) {
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

                            if point_in_rect(mx, my, (log_bg_x, log_bg_y, log_bg_w, log_bg_h)) {
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
            let h = self.window.as_ref().unwrap().inner_size().height as f32;
            let pad_x = 12.0 * s;
            let max_scroll = settings_ide_max_scroll(
                self.ide_workspaces.len(),
                self.ide_ignore_patterns.iter().map(|p| {
                    self.renderer.as_mut().unwrap().measure_ui_width(p, 0.88)
                        + pad_x * 2.0
                        + 22.0 * s
                }),
                s,
                h,
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
            let box_h = (700.0 * s)
                .min(self.window.as_ref().unwrap().inner_size().height as f32 - 40.0 * s);
            let max_scroll = self
                .renderer
                .as_mut()
                .unwrap()
                .get_faq_max_scroll(&self.faq_editor, box_h);
            self.settings_scroll.clamp_target(0.0, max_scroll);
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        if self.show_welcome || self.show_settings || self.dialog_window.is_some() {
            return;
        }

        let s = self.renderer.as_ref().unwrap().scale_factor;
        let tab_bar_h = if self.show_welcome || !self.is_ide_mode {
            0.0
        } else {
            38.0 * s
        };

        let my = self.renderer.as_ref().unwrap().last_mouse_y;
        if my >= 0.0 && my <= tab_bar_h && !self.tabs.is_empty() {
            self.tab_scroll.anim_speed = 7.0;
            self.tab_scroll.scroll_by(dy);
            let max_scroll = self.renderer.as_ref().unwrap().max_tab_scroll_x;
            self.tab_scroll.clamp_target(0.0, max_scroll);
            self.window.as_ref().unwrap().request_redraw();
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
        let tab_bar_h = if self.show_welcome || !self.is_ide_mode {
            0.0
        } else {
            38.0 * s
        };
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
        assert!(point_in_rect(10.0, 20.0, rect));
        assert!(point_in_rect(40.0, 60.0, rect));
        assert!(point_in_rect(25.0, 45.0, rect));
        assert!(!point_in_rect(9.9, 45.0, rect));
        assert!(!point_in_rect(25.0, 60.1, rect));
    }

    #[test]
    fn panel_scroll_rect_covers_top_and_bottom_layouts() {
        assert_eq!(
            panel_scroll_rect(true, 2.0, 96.0, 240.0, 360.0, 0.0, 1600.0, 1000.0),
            (96.0, 64.0, 480.0, 936.0)
        );
        assert_eq!(
            panel_scroll_rect(false, 2.0, 96.0, 240.0, 360.0, 360.0, 1600.0, 1000.0),
            (96.0, 645.0, 1504.0, 295.0)
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
    fn settings_ide_max_scroll_counts_chip_wrapping() {
        assert_eq!(
            settings_ide_max_scroll(0, std::iter::empty(), 1.0, 900.0),
            0.0
        );

        let no_wrap = settings_ide_max_scroll(2, [100.0, 120.0], 1.0, 500.0);
        let wrapped = settings_ide_max_scroll(2, [430.0, 120.0, 450.0], 1.0, 500.0);

        assert!(no_wrap > 0.0);
        assert!(wrapped > no_wrap);
    }
}
