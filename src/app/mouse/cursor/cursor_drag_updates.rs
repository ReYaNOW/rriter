//! Cursor-move phases: IDE panel DnD/resize and in-progress drags after hover.
use super::cursor_helpers::{resized_bottom_height, resized_left_width};
use super::*;
use crate::render_view::{editor_bottom_blank_lines, editor_scroll_content_height};

impl App {
    /// IDE panel DnD, tab drags, panel resize and git panel scrollbars; `true` = handled.
    #[inline]
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(super) fn cursor_moved_ide_panel_drags(
        &mut self,
        position: winit::dpi::PhysicalPosition<f64>,
        s: f32,
    ) -> bool {
        // DnD и ресайз IDE-панелей (обработка движения мыши)
        if self.is_ide_mode {
            let px = position.x as f32;
            let py = position.y as f32;

            if self.ide_panel.file_tree_drag.is_some() {
                let target_idx = self.file_tree_node_at(px, py);
                if let Some(ref mut drag) = self.ide_panel.file_tree_drag {
                    drag.current_x = px;
                    drag.current_y = py;
                    let dx = px - drag.start_x;
                    let dy = py - drag.start_y;
                    if dx * dx + dy * dy > (5.0 * s) * (5.0 * s) {
                        drag.threshold_passed = true;
                    }
                    drag.target_idx = target_idx;
                }
                self.window.as_ref().unwrap().request_redraw();
                return true;
            }

            if let Some(ref mut drag) = self.ide_panel.drag {
                drag.current_y = py;
                if (py - drag.start_y).abs() > 5.0 * s {
                    drag.threshold_passed = true;
                }
                self.window.as_ref().unwrap().request_redraw();
                return true;
            }

            if let Some(ref mut drag) = self.ide_panel.tab_drag {
                drag.current_x = px;
                if (px - drag.start_x).abs() > 5.0 * s {
                    drag.threshold_passed = true;
                }
                self.window.as_ref().unwrap().request_redraw();
                return true;
            }

            if let Some(ref mut drag) = self.ide_panel.terminal_tab_drag {
                drag.current_x = px;
                if (px - drag.start_x).abs() > 5.0 * s {
                    drag.threshold_passed = true;
                }
                self.window.as_ref().unwrap().request_redraw();
                return true;
            }

            if self.ide_panel.git.graph_resizing {
                let (_, content_y, _, content_h, _) =
                    super::app_panel_scroll_rect(self, crate::app::PanelId::Git, s);
                let controls_h = crate::app::git_panel::GIT_GRAPH_CONTROLS_H * s;
                let list_y = content_y + controls_h;
                let full_list_h = (content_h - controls_h).max(40.0 * s);
                let divider_h = crate::app::git_panel::git_graph_divider_h(s);
                let usable_h = (full_list_h - divider_h).max(1.0);
                let min_graph_h = (160.0 * s).min(usable_h);
                let min_changes_h = (72.0 * s).min(usable_h);
                let max_graph_h = (usable_h - min_changes_h).max(min_graph_h);
                let graph_h =
                    (list_y + full_list_h - py - divider_h / 2.0).clamp(min_graph_h, max_graph_h);
                self.ide_panel.git.graph_height_ratio = (graph_h / usable_h).clamp(0.25, 0.78);
                self.window.as_ref().unwrap().request_redraw();
                return true;
            }

            if self.ide_panel.is_resizing_left {
                let ww = self.window.as_ref().unwrap().inner_size().width as f32;
                self.ide_panel.left_width = resized_left_width(px, ww, s);
                self.window.as_ref().unwrap().request_redraw();
                return true;
            }

            if self.ide_panel.is_resizing_bottom {
                let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                self.ide_panel.bottom_height = resized_bottom_height(py, wh, s);
                self.window.as_ref().unwrap().request_redraw();
                return true;
            }

            if self.ide_panel.git.logs_scroll.is_dragging {
                if let Some(metrics) = self
                    .renderer
                    .as_ref()
                    .and_then(|renderer| renderer.git_logs_layout_metrics())
                {
                    let (_, track_y, _, track_h) = metrics.track_rect;
                    if crate::app::mouse::update_scrollbar_drag(
                        &mut self.ide_panel.git.logs_scroll,
                        py,
                        track_y,
                        track_h,
                        metrics.max_scroll,
                        10.0 * s,
                    ) {
                        self.ide_panel
                            .git
                            .refresh_git_logs_follow_tail(metrics.max_scroll);
                    }
                }
                self.window.as_ref().unwrap().request_redraw();
                return true;
            }

            if self.ide_panel.git.graph_scroll.is_dragging {
                if let Some((rows_y, rows_h)) = super::git_graph_rows_bounds(self, s)
                    && let Some((_, target)) = crate::app::git_panel::git_graph_scroll_drag_target(
                        py,
                        rows_y,
                        rows_h,
                        self.ide_panel.git.graph_snapshot.len(),
                        self.ide_panel.git.graph_scroll.current,
                        Some(self.ide_panel.git.graph_scroll.drag_offset),
                        s,
                    )
                {
                    let drag_offset = self.ide_panel.git.graph_scroll.drag_offset;
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
                        && crate::app::git_panel::git_graph_near_load_more(target, max_scroll, s)
                    {
                        self.load_more_git_graph_commits();
                    }
                }
                self.window.as_ref().unwrap().request_redraw();
                return true;
            }
        }
        false
    }

    /// In-progress drags after hover (inputs, LSP, terminal, editor); `true` = handled.
    #[inline]
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(super) fn cursor_moved_drag_updates(
        &mut self,
        position: winit::dpi::PhysicalPosition<f64>,
        s: f32,
        window_size: winit::dpi::PhysicalSize<u32>,
        minimap_w: f32,
        padding: f32,
    ) -> bool {
        let wh = window_size.height as f32;
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
        let editor_visible_h = crate::render_view::editor_view_height(
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
            .get_max_scroll(&self.editor, editor_visible_h);
        let scrollbar_w = if max_scroll > 0.0 { 10.0 * s } else { 0.0 };
        let scrollbar_x = window_size.width as f32 - minimap_w - scrollbar_w;

        if self.is_dragging_settings_ignore {
            let window_size = self.window.as_ref().unwrap().inner_size();
            let layout = crate::render_view::settings_ui::animated_settings_modal_layout(
                window_size.width as f32,
                window_size.height as f32,
                s,
                self.settings_anim_progress,
            );
            let input = crate::render_view::settings_ui::settings_ignore_input_rect(
                layout,
                s,
                self.ide_workspaces.len(),
                self.settings_ide_scroll.current,
            );
            let text = self.settings_ignore_editor.get_full_text();
            let x_offset =
                (position.x as f32 - (input.x + 8.0 * s) + self.settings_ignore_scroll_x).max(0.0);
            let target_idx = self
                .renderer
                .as_mut()
                .unwrap()
                .one_line_cursor_from_x(&text, x_offset, 0.95);
            self.settings_ignore_editor.cursor = target_idx;
        } else if self.is_dragging_lsp_log {
            // Drag-selection в логах LSP
            if let Some(focused_name) = self.ide_panel.lsp_logs_focused.clone() {
                if let Some((cx, cy, _cw, ch)) = self.lsp_panel_bounds() {
                    let pad_x = 12.0 * s;
                    let btn_h = 24.0 * s;
                    let scroll_y = self.ide_panel.lsp_scroll_y.current.round();
                    let mut cur_y = cy + 8.0 * s - scroll_y;

                    for srv in self.ide_panel.lsp_servers.clone().iter() {
                        let layout_logs_h = self.lsp_server_logs_h(srv, s);
                        let (inner_total_h, _) = self.lsp_server_inner_size(srv, s);
                        let logs_h = crate::app::lsp_actions::lsp_server_logs_h_for_row(
                            inner_total_h,
                            cy,
                            ch,
                            cur_y,
                            s,
                        );
                        let is_exp = logs_h > 0.0;
                        let row_h = 136.0 * s + layout_logs_h;

                        if srv.name == focused_name.as_str() && is_exp {
                            let card_x = cx + 12.0 * s;
                            let btn_y1 = cur_y + 56.0 * s;
                            let btn_y2 = btn_y1 + btn_h + 8.0 * s;
                            let log_bg_x = card_x + pad_x;
                            let log_bg_y = btn_y2 + btn_h + 44.0 * s;

                            let inner_scroll_y = self
                                .ide_panel
                                .lsp_logs_scroll_y
                                .get(srv.name)
                                .map(|ss| ss.current)
                                .unwrap_or(0.0)
                                .round();
                            let inner_scroll_x = self
                                .ide_panel
                                .lsp_logs_scroll_x
                                .get(srv.name)
                                .map(|ss| ss.current)
                                .unwrap_or(0.0)
                                .round();
                            let mut text_y = log_bg_y + 16.0 * s - inner_scroll_y;
                            let line_h = 16.0 * s;
                            let my_drag = position.y as f32;

                            if let Some(ed) = self
                                .ide_panel
                                .lsp_log_editors
                                .get_mut(focused_name.as_str())
                            {
                                let mut phys_line = 0;
                                let (first, second) = ed.text_parts();
                                let first_len = first.len();

                                while phys_line < ed.line_offsets.len() {
                                    let is_folded = ed.folded_lines.contains(&phys_line);
                                    let fold_end = if is_folded {
                                        ed.foldable_lines.get(&phys_line).copied()
                                    } else {
                                        None
                                    };

                                    if my_drag >= text_y - line_h && my_drag <= text_y {
                                        let start_byte = ed.line_offsets[phys_line];
                                        let end_byte = if phys_line + 1 < ed.line_offsets.len() {
                                            ed.line_offsets[phys_line + 1].saturating_sub(1)
                                        } else {
                                            ed.len()
                                        };

                                        let click_x_in_line =
                                            (position.x as f32 - log_bg_x - 20.0 * s
                                                + inner_scroll_x)
                                                .max(0.0);
                                        let r = self.renderer.as_mut().unwrap();

                                        let mut current_x = 0.0;
                                        let mut best_dist = click_x_in_line.abs();
                                        let mut byte_off = start_byte;
                                        let mut current_chunk_offset = start_byte;

                                        while current_chunk_offset < end_byte {
                                            let chunk = if current_chunk_offset < first_len {
                                                &first
                                                    [current_chunk_offset..end_byte.min(first_len)]
                                            } else {
                                                &second[current_chunk_offset - first_len
                                                    ..end_byte - first_len]
                                            };

                                            for c in chunk.chars() {
                                                let adv = if c == '\n'
                                                    || c == '\u{FE0F}'
                                                    || c == '\u{200D}'
                                                {
                                                    0.0
                                                } else {
                                                    r.char_advance(c) * 0.7
                                                };
                                                let dist = (current_x - click_x_in_line).abs();
                                                if dist < best_dist {
                                                    best_dist = dist;
                                                    byte_off = current_chunk_offset;
                                                }
                                                current_x += adv;
                                                current_chunk_offset += c.len_utf8();
                                            }
                                        }
                                        if (current_x - click_x_in_line).abs() < best_dist {
                                            byte_off = end_byte;
                                        }

                                        if ed.selection_anchor.is_none() {
                                            ed.selection_anchor = Some(byte_off);
                                        }
                                        ed.cursor = byte_off;
                                        break;
                                    }

                                    if is_folded {
                                        phys_line = fold_end.unwrap();
                                    }
                                    phys_line += 1;
                                    text_y += line_h;
                                }
                            }
                            break;
                        }
                        cur_y += row_h + 16.0 * s;
                    }
                }
            }
        } else if self.ide_panel.lsp_scroll_x.is_dragging {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            if let Some((cx, _, cw, _)) = self.lsp_panel_bounds() {
                let track_w = cw - 30.0 * s;
                let max_x = 0.0;
                let thumb_w = track_w;
                let ratio =
                    (position.x as f32 - cx - 10.0 * s - self.ide_panel.lsp_scroll_x.drag_offset)
                        / (track_w - thumb_w).max(0.0001);
                self.ide_panel.lsp_scroll_x.target = (ratio * max_x).clamp(0.0, max_x);
                self.ide_panel.lsp_scroll_x.current = self.ide_panel.lsp_scroll_x.target;
            }
        } else if self
            .ide_panel
            .terminals
            .iter()
            .any(|t| t.scroll_y.is_dragging)
        {
            let layout = active_terminal_scrollbar_layout(self);
            let active = self.ide_panel.active_terminal;
            if let (Some(layout), Some(term)) = (layout, self.ide_panel.terminals.get_mut(active))
                && let Some((_, target)) =
                    crate::render_view::terminal_ui::terminal_scrollbar_drag_target(
                        position.y as f32,
                        layout,
                        Some(term.scroll_y.drag_offset),
                    )
            {
                let drag_offset = term.scroll_y.drag_offset;
                let _ = crate::app::mouse::apply_scrollbar_drag_target(
                    &mut term.scroll_y,
                    target,
                    drag_offset,
                );
                self.window.as_ref().unwrap().request_redraw();
            }
        } else if self.ide_panel.problems_scroll.is_dragging {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            if let Some(layout) = super::problems_scrollbar_layout(self, s)
                && let Some(thumb) = crate::scroll::scrollbar_thumb(
                    layout.list_y,
                    layout.track_h,
                    layout.track_h,
                    layout.total_h,
                    self.ide_panel.problems_scroll.current,
                    20.0 * s,
                )
                && let Some((drag_offset, target)) = crate::scroll::scrollbar_drag_target(
                    position.y as f32,
                    layout.list_y,
                    layout.track_h,
                    thumb,
                    (layout.total_h - layout.track_h).max(0.0),
                    Some(self.ide_panel.problems_scroll.drag_offset),
                )
            {
                let _ = crate::app::mouse::apply_scrollbar_drag_target(
                    &mut self.ide_panel.problems_scroll,
                    target,
                    drag_offset,
                );
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            return true;
        } else if crate::app::mouse::HOVER_STATE.with(|s| {
            s.borrow()
                .popup
                .as_ref()
                .map(|p| p.scroll.is_dragging)
                .unwrap_or(false)
        }) {
            crate::app::mouse::HOVER_STATE.with(|hover_state| {
                let mut state = hover_state.borrow_mut();
                if let Some(rect) = state.rect {
                    let max_scroll = state.max_scroll;
                    if let Some(popup) = &mut state.popup
                        && let Some((drag_offset, target)) =
                            crate::app::mouse::hover_popup_scrollbar_drag_target(
                                rect,
                                max_scroll,
                                popup.scroll.current,
                                position.y as f32,
                                s,
                                Some(popup.scroll.drag_offset),
                            )
                    {
                        let _ = crate::app::mouse::apply_scrollbar_drag_target(
                            &mut popup.scroll,
                            target,
                            drag_offset,
                        );
                    }
                }
            });
            self.window.as_ref().unwrap().request_redraw();
            return true;
        } else if self.ide_panel.lsp_scroll_y.is_dragging {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            if let Some((_, cy, _, ch)) = self.lsp_panel_bounds() {
                let total_h = self.lsp_panel_total_h(s);
                let track_start = cy + 5.0 * s;
                let track_h = (ch - 10.0 * s).max(0.0);
                let max_y = (total_h - ch).max(0.0);
                if let Some(thumb) = crate::scroll::scrollbar_thumb(
                    track_start,
                    track_h,
                    ch,
                    total_h,
                    self.ide_panel.lsp_scroll_y.current,
                    40.0 * s,
                ) && let Some((drag_offset, target)) = crate::scroll::scrollbar_drag_target(
                    position.y as f32,
                    track_start,
                    track_h,
                    thumb,
                    max_y,
                    Some(self.ide_panel.lsp_scroll_y.drag_offset),
                ) {
                    let _ = crate::app::mouse::apply_scrollbar_drag_target(
                        &mut self.ide_panel.lsp_scroll_y,
                        target,
                        drag_offset,
                    );
                }
            }
        } else if self.ide_panel.lsp_servers.iter().any(|info| {
            self.ide_panel
                .lsp_logs_scroll_y
                .get(info.name)
                .map(|s| s.is_dragging)
                .unwrap_or(false)
                || self
                    .ide_panel
                    .lsp_logs_scroll_x
                    .get(info.name)
                    .map(|s| s.is_dragging)
                    .unwrap_or(false)
        }) {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            for (idx, info) in self.ide_panel.lsp_servers.clone().iter().enumerate() {
                let name = info.name.to_string();
                let is_drag_y = self
                    .ide_panel
                    .lsp_logs_scroll_y
                    .get(&name)
                    .map(|s| s.is_dragging)
                    .unwrap_or(false);
                let is_drag_x = self
                    .ide_panel
                    .lsp_logs_scroll_x
                    .get(&name)
                    .map(|s| s.is_dragging)
                    .unwrap_or(false);

                if is_drag_y || is_drag_x {
                    if let Some((cx, cy, cw, ch)) = self.lsp_panel_bounds() {
                        let scroll_y = self.ide_panel.lsp_scroll_y.current.round();
                        let mut current_y = cy + 8.0 * s - scroll_y;
                        for (i, srv) in self.ide_panel.lsp_servers.iter().enumerate() {
                            if i == idx {
                                break;
                            }
                            let logs_h = self.lsp_server_logs_h(srv, s);
                            current_y += 136.0 * s + logs_h + 16.0 * s;
                        }

                        let (inner_total_h, inner_max_w) = self.lsp_server_inner_size(info, s);
                        let logs_h = crate::app::lsp_actions::lsp_server_logs_h_for_row(
                            inner_total_h,
                            cy,
                            ch,
                            current_y,
                            s,
                        );
                        if logs_h <= 0.0 {
                            continue;
                        }
                        let btn_y1 = current_y + 56.0 * s;
                        let btn_h = 24.0 * s;
                        let btn_y2 = btn_y1 + btn_h + 8.0 * s;
                        let log_bg_y = btn_y2 + btn_h + 44.0 * s;
                        let log_bg_x = cx + 24.0 * s;
                        let log_bg_w = cw - 48.0 * s;
                        let log_bg_h = logs_h - 52.0 * s;

                        if is_drag_y {
                            let sy = self.ide_panel.lsp_logs_scroll_y.get_mut(&name).unwrap();
                            if let Some((drag_offset, target)) =
                                crate::app::lsp_actions::lsp_log_scrollbar_drag_target(
                                    position.y as f32,
                                    log_bg_y + 7.0 * s,
                                    (log_bg_h - 14.0 * s).max(0.0),
                                    log_bg_h,
                                    inner_total_h,
                                    sy.current,
                                    s,
                                    Some(sy.drag_offset),
                                )
                            {
                                let _ = crate::app::mouse::apply_scrollbar_drag_target(
                                    sy,
                                    target,
                                    drag_offset,
                                );
                            }
                        } else if is_drag_x {
                            let sx = self.ide_panel.lsp_logs_scroll_x.get_mut(&name).unwrap();
                            if let Some((drag_offset, target)) =
                                crate::app::lsp_actions::lsp_log_scrollbar_drag_target(
                                    position.x as f32,
                                    log_bg_x + 7.0 * s,
                                    (log_bg_w - 14.0 * s).max(0.0),
                                    log_bg_w,
                                    inner_max_w + 20.0 * s,
                                    sx.current,
                                    s,
                                    Some(sx.drag_offset),
                                )
                            {
                                let _ = crate::app::mouse::apply_scrollbar_drag_target(
                                    sx,
                                    target,
                                    drag_offset,
                                );
                            }
                        }
                    }
                    break;
                }
            }
        } else if self.is_dragging_search {
            let (input_id, scroll_x, text) = if self.ide_panel.git.message_focused {
                (
                    crate::ui_system::UiId::GitMessageInput,
                    self.renderer
                        .as_ref()
                        .map_or(0.0, |renderer| renderer.git_commit_scroll_x),
                    self.ide_panel.git.message_editor.get_full_text(),
                )
            } else if self.ide_panel.term_search_focused {
                (
                    crate::ui_system::UiId::TerminalSearchInput,
                    self.renderer
                        .as_ref()
                        .map_or(0.0, |renderer| renderer.terminal_search_scroll_x),
                    self.ide_panel.term_search_editor.get_full_text(),
                )
            } else {
                (
                    crate::ui_system::UiId::SearchInput,
                    self.renderer
                        .as_ref()
                        .map_or(0.0, |renderer| renderer.search_scroll_x),
                    self.search_editor.get_full_text(),
                )
            };

            if let Some(rect) = self.ui_registry.rect_for(input_id) {
                let x_offset = (position.x as f32 - (rect.0 + 5.0 * s) + scroll_x).max(0.0);
                let target_idx = self
                    .renderer
                    .as_mut()
                    .unwrap()
                    .one_line_cursor_from_x(&text, x_offset, 1.0);
                if self.ide_panel.git.message_focused {
                    self.ide_panel.git.message_editor.cursor = target_idx;
                } else if self.ide_panel.term_search_focused {
                    self.ide_panel.term_search_editor.cursor = target_idx;
                } else {
                    self.search_editor.cursor = target_idx;
                }
            }
        } else if self.scroll_x.is_dragging {
            let r = self.renderer.as_ref().unwrap();
            let track_w = scrollbar_x - padding;
            let max_x = r.max_scroll_x;
            let thumb_w = (track_w / (max_x + track_w).max(1.0) * track_w)
                .max(40.0 * s)
                .min(track_w.max(0.0));
            let ratio = (position.x as f32 - padding - self.scroll_x.drag_offset)
                / (track_w - thumb_w).max(0.0001);
            let target = (ratio * max_x).clamp(0.0, max_x);
            let drag_offset = self.scroll_x.drag_offset;
            let _ = crate::app::mouse::apply_scrollbar_drag_target(
                &mut self.scroll_x,
                target,
                drag_offset,
            );
        } else if self.scroll_y.is_dragging {
            let now = std::time::Instant::now();
            let elapsed = now.duration_since(self.last_click_time).as_millis();
            let dy = (position.y as f32 - self.last_click_pos.1).abs();

            if elapsed > 120 || dy > 10.0 {
                let r = self.renderer.as_ref().unwrap();
                let s = r.scale_factor;
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
                let editor_height = crate::render_view::editor_view_height(
                    wh,
                    tab_bar_h,
                    editor_bottom_h,
                    self.is_ide_mode,
                    s,
                );
                let minimap_w = r.minimap_width;

                let is_minimap_drag = self.last_click_pos.0
                    >= (self.window.as_ref().unwrap().inner_size().width as f32 - minimap_w);

                let track_h = editor_height;
                let thumb_h = if is_minimap_drag {
                    let total_lines_f32 = self.editor.line_offsets.len() as f32;
                    let bottom_blank_lines =
                        editor_bottom_blank_lines(editor_height, r.line_height);
                    let visible_minimap_lines = total_lines_f32.min(900.0);
                    let minimap_line_h = (editor_height
                        / (visible_minimap_lines + bottom_blank_lines).max(1.0))
                    .max(1.5);
                    let visible_lines = editor_height / r.line_height;
                    (visible_lines * minimap_line_h).max(4.0)
                } else {
                    let total_content_height = editor_scroll_content_height(
                        self.editor.get_visible_lines_count(),
                        r.line_height,
                        editor_height,
                    );
                    (editor_height / total_content_height.max(editor_height) * editor_height)
                        .max(20.0 * s)
                        .min(track_h.max(0.0))
                };

                let track_start_y = tab_bar_h;
                let last_mouse_y = r.last_mouse_y;

                let scroll_ratio = (last_mouse_y - track_start_y - self.scroll_y.drag_offset)
                    / (track_h - thumb_h).max(0.0001);

                self.scroll_y.target = (scroll_ratio * max_scroll).clamp(0.0, max_scroll).round();
                self.scroll_y.anim_speed = 15.0;
            }
        } else if self.ide_panel.is_dragging_terminal && self.is_dragging && !self.show_settings {
            let active = self.ide_panel.active_terminal;
            let s = self.renderer.as_ref().unwrap().scale_factor;
            let (terminal_panel_x, content_y, _, content_h, _) =
                super::app_panel_scroll_rect(self, crate::app::PanelId::Terminal, s);
            let (term_content_y, term_content_h) =
                crate::render_view::terminal_ui::terminal_body_rect(content_y, content_h, s);
            let lh = self.renderer.as_ref().unwrap().line_height;
            let char_h = lh * crate::render_view::terminal_ui::TERMINAL_TEXT_SCALE;
            let char_w = self.renderer.as_mut().unwrap().char_advance('A')
                * crate::render_view::terminal_ui::TERMINAL_TEXT_SCALE;
            let panel_x = terminal_panel_x + 10.0 * s;
            if let Some(term) = self.ide_panel.terminals.get_mut(active) {
                let py = position.y as f32;
                let px = position.x as f32;

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

                let scroll_offset = crate::render_view::terminal_ui::terminal_render_scroll_offset(
                    term.scroll_y.current,
                    max_scroll,
                    grid.is_alt,
                );
                let (_, bottom_pad) = crate::render_view::terminal_ui::terminal_text_padding(s);
                let offset_from_bottom =
                    (term_content_y + term_content_h - bottom_pad - py + scroll_offset) / char_h;
                let mut cell_y = total_lines
                    .saturating_sub(1)
                    .saturating_sub(offset_from_bottom.max(0.0).floor() as usize);
                let mut cell_x = ((px - panel_x) / char_w).floor() as usize;

                cell_y = cell_y.min(total_lines.saturating_sub(1));
                cell_x = cell_x.min(grid.cols.saturating_sub(1));

                if let Some((sx, sy, _, _)) = grid.selection {
                    grid.selection = Some((sx, sy, cell_x, cell_y));
                } else {
                    grid.selection = Some((cell_x, cell_y, cell_x, cell_y));
                }
                self.window.as_ref().unwrap().request_redraw();
            }
        } else if self.is_dragging && !self.ide_panel.is_dragging_terminal && !self.show_settings {
            let last_mouse_x = self.renderer.as_ref().unwrap().last_mouse_x;
            let last_mouse_y = self.renderer.as_ref().unwrap().last_mouse_y;
            let scale = self
                .renderer
                .as_ref()
                .map_or(1.0, |renderer| renderer.scale_factor);
            let tab_bar_h = crate::render_view::editor_content_top_inset(
                self.show_welcome,
                self.is_ide_mode,
                self.active_tab_is_database_query(),
                scale,
            );
            self.editor.set_cursor_at_pos(
                last_mouse_x,
                last_mouse_y - tab_bar_h + self.scroll_y.current,
                self.renderer.as_mut().unwrap(),
                false,
            );
            clear_hover_popup(self.renderer.as_mut());
        }
        false
    }
}
