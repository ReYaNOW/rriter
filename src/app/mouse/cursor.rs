use super::*;
use crate::render_view::{editor_bottom_blank_lines, editor_scroll_content_height};

fn autocomplete_drag_target(
    py: f32,
    rect_y: f32,
    rect_h: f32,
    drag_offset: f32,
    total_items: usize,
    scale: f32,
) -> f32 {
    let step = 36.0 * scale;
    let total_items = total_items as f32;
    let visible_items = total_items.min(7.0);

    let track_margin = autocomplete_scrollbar_track_margin(scale);
    let track_h = (rect_h - track_margin * 2.0).max(1.0);
    let total_h = total_items * step;
    let thumb_h = (rect_h / total_h * track_h).max(20.0 * scale);
    let max_scroll = ((total_items - visible_items) * step).max(0.0);

    let ratio = (py - rect_y - track_margin - drag_offset) / (track_h - thumb_h).max(1.0);
    (ratio * max_scroll).clamp(0.0, max_scroll)
}

fn autocomplete_scrollbar_track_margin(scale: f32) -> f32 {
    3.0 * scale
}

fn autocomplete_hovered_index(
    px: f32,
    py: f32,
    rect: (f32, f32, f32, f32),
    current_scroll: f32,
    total_items: usize,
    scale: f32,
) -> Option<usize> {
    let (rx, ry, rw, rh) = rect;
    if px < rx || px > rx + rw || py < ry || py > ry + rh {
        return None;
    }

    let scroll_x = rx + rw - 14.0 * scale;
    if px >= scroll_x {
        return None;
    }
    let item_h = 36.0 * scale;
    let content_y = py - ry + current_scroll;
    if content_y < 0.0 {
        return None;
    }

    let idx = (content_y / item_h) as usize;
    (idx < total_items).then_some(idx)
}

fn resized_left_width(px: f32, window_width: f32, scale: f32) -> f32 {
    let sb_w = 48.0 * scale;
    let max_w = ((window_width - sb_w) / scale) - 300.0;
    ((px - sb_w) / scale).max(80.0).min(max_w.max(80.0))
}

fn resized_bottom_height(py: f32, window_height: f32, scale: f32) -> f32 {
    let status_bar_h = crate::render_view::ide_status_bar_height(scale);
    let available_h = (window_height - status_bar_h).max(0.0);
    let max_h = (available_h / scale) - 50.0;
    ((available_h - py) / scale).max(60.0).min(max_h.max(60.0))
}

fn cursor_position_allows_editor_hover(
    px: f32,
    py: f32,
    window_width: f32,
    window_height: f32,
) -> bool {
    px >= 0.0 && py >= 0.0 && px < window_width && py < window_height
}

fn should_suppress_editor_hover_for_scroll_drag(
    scroll_y_dragging: bool,
    scroll_x_dragging: bool,
) -> bool {
    scroll_y_dragging || scroll_x_dragging
}

fn inline_git_popup_blocks_hover(id: Option<crate::ui_system::UiId>) -> bool {
    matches!(
        id,
        Some(
            crate::ui_system::UiId::InlineGitPanelBody
                | crate::ui_system::UiId::InlineGitPrevHunk
                | crate::ui_system::UiId::InlineGitNextHunk
                | crate::ui_system::UiId::InlineGitRollbackHunk
        )
    )
}

impl App {
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn handle_main_cursor_moved(&mut self, position: winit::dpi::PhysicalPosition<f64>) {
        let px = position.x as f32;
        let py = position.y as f32;
        {
            let renderer = self.renderer.as_mut().unwrap();
            renderer.last_mouse_x = px;
            renderer.last_mouse_y = py;
            if renderer.popups_waiting_for_mouse_move_at(px, py) {
                return;
            }
            renderer.update_popup_mouse_move_gate();
        }

        if self.dialog_window.is_some() {
            return;
        }

        if self
            .autocomplete_detail_popup
            .as_ref()
            .is_some_and(|popup| popup.scroll.is_dragging)
        {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            if let Some(rect) = self.autocomplete_detail_rect {
                let (_, by, _, box_h) = rect;
                let max_scroll = self.autocomplete_detail_max_scroll;
                if max_scroll > 0.0 {
                    let track_h = box_h - 16.0 * s;
                    let thumb_h = (box_h / (box_h + max_scroll) * track_h).max(20.0 * s);
                    if let Some(popup) = &mut self.autocomplete_detail_popup {
                        let ratio = (py - by - 8.0 * s - popup.scroll.drag_offset)
                            / (track_h - thumb_h).max(0.0001);
                        popup.scroll.target = (ratio * max_scroll).clamp(0.0, max_scroll);
                        popup.scroll.current = popup.scroll.target;
                    }
                }
            }
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        if !self.autocomplete_detail_selecting && self.autocomplete_window_contains(px, py) {
            clear_hover_popup(self.renderer.as_mut());
            self.update_ctrl_definition_hover(None);
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        if self.file_tree_overlay_active() {
            if let Some(kind) = self.ide_panel.file_tree_dialog_input_drag {
                if let Some(target_idx) = self.file_tree_dialog_input_index_at(kind, px) {
                    self.set_file_tree_dialog_input_cursor(kind, target_idx, false);
                }
            }
            self.ide_panel.file_tree_hovered_idx = None;
            clear_hover_popup(self.renderer.as_mut());
            self.update_ctrl_definition_hover(None);
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        if self.api_python_runtime_overlay_active() {
            clear_hover_popup(self.renderer.as_mut());
            self.update_ctrl_definition_hover(None);
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        let window_size = self.window.as_ref().unwrap().inner_size();
        if !cursor_position_allows_editor_hover(
            position.x as f32,
            position.y as f32,
            window_size.width as f32,
            window_size.height as f32,
        ) {
            clear_hover_popup(self.renderer.as_mut());
            self.update_ctrl_definition_hover(None);
            return;
        }

        if self.inline_git_popup.is_some()
            && inline_git_popup_blocks_hover(self.ui_registry.find_at(px, py))
        {
            clear_hover_popup(self.renderer.as_mut());
            self.update_ctrl_definition_hover(None);
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        if self.autocomplete_detail_selecting {
            if let (Some(rect), Some(popup)) = (
                self.autocomplete_detail_rect,
                self.autocomplete_detail_popup.as_ref(),
            ) {
                let byte = hover_popup_byte_at(
                    self.renderer.as_mut().unwrap(),
                    popup,
                    rect,
                    position.x as f32,
                    position.y as f32,
                );
                self.autocomplete_detail_selection_cursor = Some(byte);
                self.window.as_ref().unwrap().request_redraw();
                return;
            }
        }

        if self.autocomplete_active {
            let px = position.x as f32;
            let py = position.y as f32;
            let in_detail = self
                .autocomplete_detail_rect
                .is_some_and(|(rx, ry, rw, rh)| {
                    px >= rx && px <= rx + rw && py >= ry && py <= ry + rh
                });
            if in_detail {
                clear_hover_popup(self.renderer.as_mut());
                self.window.as_ref().unwrap().request_redraw();
                return;
            }
        }

        let mut popup_selecting = false;
        HOVER_STATE.with(|hover_state| {
            let mut hs = hover_state.borrow_mut();
            if hs.selecting {
                if let (Some(rect), Some(popup)) = (hs.rect, hs.popup.as_ref()) {
                    let byte = hover_popup_byte_at(
                        self.renderer.as_mut().unwrap(),
                        popup,
                        rect,
                        position.x as f32,
                        position.y as f32,
                    );
                    hs.selection_cursor = Some(byte);
                    popup_selecting = true;
                }
            } else if hs.diag_selecting {
                let byte = crate::render_view::ui::diag_popup_byte_at(
                    position.x as f32,
                    position.y as f32,
                );
                hs.diag_selection_cursor = Some(byte);
                popup_selecting = true;
            }
        });
        if popup_selecting {
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        if self
            .renderer
            .as_ref()
            .is_some_and(|renderer| renderer.git_graph_tooltip_selecting)
        {
            if let Some(renderer) = self.renderer.as_mut() {
                let byte = renderer.git_graph_tooltip_byte_at(position.x as f32, position.y as f32);
                renderer.git_graph_tooltip_selection_cursor = Some(byte);
            }
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        let editor_text_selecting =
            self.is_dragging && !self.ide_panel.is_dragging_terminal && !self.show_settings;
        if self.drag_api_text_scrollbar_x_from_last_mouse() {
            self.window.as_ref().unwrap().request_redraw();
            return;
        }
        if editor_text_selecting && self.drag_api_text_cursor_from_last_mouse() {
            clear_hover_popup(self.renderer.as_mut());
            self.window.as_ref().unwrap().request_redraw();
            return;
        }
        if editor_text_selecting {
            clear_hover_popup(self.renderer.as_mut());
            if let Some(r) = self.renderer.as_mut() {
                r.suppress_popups_until_next_mouse_move();
            }
        }

        let s = self.renderer.as_ref().unwrap().scale_factor;

        if let (true, Some((rx, ry, rw, rh))) = (self.autocomplete_active, self.autocomplete_rect) {
            let px = position.x as f32;
            let py = position.y as f32;

            if self.autocomplete_scroll.is_dragging {
                self.autocomplete_scroll.anim_speed = 15.0;
                self.autocomplete_scroll.target = autocomplete_drag_target(
                    py,
                    ry,
                    rh,
                    self.autocomplete_scroll.drag_offset,
                    self.autocomplete_options.len(),
                    s,
                );
                self.window.as_ref().unwrap().request_redraw();
                return;
            }

            if px >= rx && px <= rx + rw && py >= ry && py <= ry + rh {
                clear_hover_popup(self.renderer.as_mut());
                self.autocomplete_hovered_idx = autocomplete_hovered_index(
                    px,
                    py,
                    (rx, ry, rw, rh),
                    self.autocomplete_scroll.current,
                    self.autocomplete_options.len(),
                    s,
                );
                self.window.as_ref().unwrap().request_redraw();
                return;
            } else {
                self.autocomplete_hovered_idx = None;
            }
        }

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
                return;
            }

            if let Some(ref mut drag) = self.ide_panel.drag {
                drag.current_y = py;
                if (py - drag.start_y).abs() > 5.0 * s {
                    drag.threshold_passed = true;
                }
                self.window.as_ref().unwrap().request_redraw();
                return;
            }

            if let Some(ref mut drag) = self.ide_panel.tab_drag {
                drag.current_x = px;
                if (px - drag.start_x).abs() > 5.0 * s {
                    drag.threshold_passed = true;
                }
                self.window.as_ref().unwrap().request_redraw();
                return;
            }

            if self.ide_panel.git.graph_resizing {
                let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                let title_h = 32.0 * s;
                let controls_h = crate::app::git_panel::GIT_GRAPH_CONTROLS_H * s;
                let list_y = title_h + controls_h;
                let panel_bottom_h = if self.ide_panel.any_bottom_open() {
                    self.ide_panel.bottom_height * s
                } else {
                    0.0
                };
                let content_bottom = crate::render_view::ide_bottom_panel_y(wh, panel_bottom_h, s);
                let content_h = (content_bottom - title_h).max(0.0);
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
                return;
            }

            if self.ide_panel.is_resizing_left {
                let ww = self.window.as_ref().unwrap().inner_size().width as f32;
                self.ide_panel.left_width = resized_left_width(px, ww, s);
                self.window.as_ref().unwrap().request_redraw();
                return;
            }

            if self.ide_panel.is_resizing_bottom {
                let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                self.ide_panel.bottom_height = resized_bottom_height(py, wh, s);
                self.window.as_ref().unwrap().request_redraw();
                return;
            }

            if self.ide_panel.git.graph_scroll.is_dragging {
                let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                if let Some((rows_y, rows_h)) = super::git_graph_rows_bounds(&self.ide_panel, wh, s)
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
                    self.ide_panel.git.graph_scroll.target = target;
                    self.ide_panel.git.graph_scroll.velocity = 0.0;
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
                return;
            }
        }

        let suppress_editor_hover = should_suppress_editor_hover_for_scroll_drag(
            self.scroll_y.is_dragging,
            self.scroll_x.is_dragging,
        );
        if suppress_editor_hover {
            clear_hover_popup(self.renderer.as_mut());
            self.update_ctrl_definition_hover(None);
        }

        // Hover над узлами дерева файлов
        if self.is_ide_mode && self.ide_panel.is_open(crate::app::PanelId::Explorer) {
            let mut new_hover = self.file_tree_node_at(position.x as f32, position.y as f32);

            let s = self.renderer.as_ref().unwrap().scale_factor;
            let wh = self.window.as_ref().unwrap().inner_size().height as f32;
            let is_top = self.ide_panel.slots.iter().any(|sl| {
                sl.id == crate::app::PanelId::Explorer && sl.group == crate::app::PanelGroup::Top
            });
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

            let (ecx, ecy, ecw, ech) = if is_top {
                let panel_left_w = self.ide_panel.left_width * s;
                let title_h = 32.0 * s;
                (
                    48.0 * s,
                    title_h,
                    panel_left_w,
                    wh - title_h - effective_bottom_h,
                )
            } else {
                let ww = self.window.as_ref().unwrap().inner_size().width as f32;
                let tab_h = 32.0 * s;
                let panel_y = crate::render_view::ide_bottom_panel_y(wh, panel_bottom_h, s);
                (
                    48.0 * s,
                    panel_y + 1.0 + tab_h,
                    ww - 48.0 * s,
                    panel_bottom_h - 1.0 - tab_h,
                )
            };

            let px = position.x as f32;
            let py = position.y as f32;
            if px < ecx || px > ecx + ecw || py < ecy || py > ecy + ech {
                new_hover = None;
            }

            if new_hover != self.ide_panel.file_tree_hovered_idx {
                self.ide_panel.file_tree_hovered_idx = new_hover;
                self.window.as_ref().unwrap().request_redraw();
            }
        }

        let s = self.renderer.as_ref().unwrap().scale_factor;
        let (in_hover_popup, in_hover_source_line) = HOVER_STATE.with(|state| {
            state.borrow().popup_safe_area_contains(
                position.x as f32,
                position.y as f32,
                self.renderer.as_ref().unwrap().width,
                s,
            )
        });
        if self.update_api_mock_hover_from_cursor(
            position.x as f32,
            position.y as f32,
            in_hover_popup,
            in_hover_source_line,
        ) {
            self.update_ctrl_definition_hover(None);
            self.window.as_ref().unwrap().request_redraw();
            return;
        }

        let minimap_w = self.renderer.as_ref().unwrap().minimap_width;
        let padding = self.renderer.as_ref().unwrap().left_padding;
        let bottom_panel_h = if self.is_ide_mode && self.ide_panel.any_bottom_open() {
            self.ide_panel.bottom_height * s
        } else {
            0.0
        };
        let bottom_panel_y =
            crate::render_view::ide_bottom_panel_y(window_size.height as f32, bottom_panel_h, s);
        let in_blocking_bottom_panel = self.is_ide_mode
            && bottom_panel_h > 0.0
            && self.ide_panel.bottom_panel_blocks_editor_hover()
            && position.y as f32 >= bottom_panel_y
            && position.y as f32 <= bottom_panel_y + bottom_panel_h;

        if in_blocking_bottom_panel {
            clear_hover_popup(self.renderer.as_mut());
            self.update_ctrl_definition_hover(None);
        }

        if !suppress_editor_hover
            && !in_blocking_bottom_panel
            && (!in_hover_popup || in_hover_source_line)
        {
            let tab_bar_h = if self.show_welcome || !self.is_ide_mode {
                0.0
            } else {
                38.0 * s
            };
            let render_scroll_y = self.scroll_y.current.round() - tab_bar_h;
            let px = position.x as f32;
            let py = position.y as f32;
            let mut diag_hover_byte = None;
            let line_h = self.renderer.as_ref().unwrap().line_height;
            let baseline_offset = self.renderer.as_ref().unwrap().baseline_offset;
            let hover_content_y =
                hover_screen_y_to_content_y(py, render_scroll_y, line_h, baseline_offset)
                    .unwrap_or(0.0);

            if let (Some(lsp), Some(path)) = (self.lsp.as_ref(), self.file_path.as_ref()) {
                let (_, diagnostics) = lsp.get_instant_diagnostics_with_version(path);
                let render_scroll_x = self.scroll_x.current.round();
                let left_padding = self.renderer.as_ref().unwrap().left_padding;
                let last_line = self.editor.line_offsets.len().saturating_sub(1);
                let cursor_phys_line = self
                    .editor
                    .line_offsets
                    .partition_point(|&o| o <= self.editor.cursor)
                    .saturating_sub(1);

                'diag_scan: for diag in diagnostics {
                    if crate::render_view::should_suppress_active_line_useless_expression(
                        diag,
                        cursor_phys_line,
                    ) {
                        continue;
                    }
                    let start_line = (diag.start_line as usize).min(last_line);
                    let end_line = (diag.end_line as usize).min(last_line);

                    for line in start_line..=end_line {
                        let vis_line_idx = self
                            .renderer
                            .as_ref()
                            .unwrap()
                            .phys_to_visual
                            .get(line)
                            .copied()
                            .unwrap_or(0) as f32;
                        let line_top_y = vis_line_idx * line_h;

                        if !hover_content_y_in_line_hitbox(hover_content_y, line_top_y, line_h) {
                            continue;
                        }

                        let start_col = if line == diag.start_line as usize {
                            diag.start_col
                        } else {
                            0
                        };
                        let end_col = if line == diag.end_line as usize {
                            diag.end_col
                        } else {
                            u32::MAX
                        };

                        let avg_adv = self.renderer.as_mut().unwrap().char_advance('a');
                        let mut x_start_px = 0.0f32;
                        let mut x_end_px = 0.0f32;
                        let mut cur_x = 0.0f32;
                        let mut start_found = false;
                        let mut end_found = false;

                        self.editor
                            .utf16_col_to_byte_advance(line, |ch, utf16_before, _pos| {
                                if !start_found && utf16_before >= start_col {
                                    x_start_px = cur_x;
                                    start_found = true;
                                }
                                if !end_found && utf16_before >= end_col {
                                    x_end_px = cur_x;
                                    end_found = true;
                                }
                                cur_x += if ch == '\t' {
                                    self.renderer.as_mut().unwrap().char_advance(' ') * 4.0
                                } else {
                                    self.renderer.as_mut().unwrap().char_advance(ch)
                                };
                            });

                        if !start_found {
                            x_start_px = cur_x;
                        }
                        if !end_found {
                            x_end_px = cur_x.max(x_start_px + avg_adv * 4.0);
                        }

                        let Some((_, _, type_target)) = diagnostic_hover_byte_range_on_line(
                            &self.editor,
                            line,
                            start_col,
                            end_col,
                        ) else {
                            continue;
                        };

                        let x_start = left_padding + x_start_px - render_scroll_x;
                        let x_end = left_padding + x_end_px - render_scroll_x;
                        let squiggle_w = (x_end - x_start).max(avg_adv / 2.0);

                        if px < x_start || px > x_start + squiggle_w {
                            continue;
                        }

                        let line_x = px - left_padding + render_scroll_x;
                        let type_target_under_cursor = {
                            let renderer = self.renderer.as_mut().unwrap();
                            diagnostic_hover_type_target_at_x(
                                &self.editor,
                                line,
                                line_x,
                                Some(type_target),
                                |ch| renderer.char_advance(ch),
                            )
                        };
                        diag_hover_byte = type_target_under_cursor;
                        break 'diag_scan;
                    }
                }
            }

            let byte_offset = if let Some(byte) = diag_hover_byte {
                byte
            } else {
                let line_top_y = (hover_content_y / line_h).floor() * line_h;
                if hover_content_y_in_line_hitbox(hover_content_y, line_top_y, line_h) {
                    self.renderer.as_mut().unwrap().get_byte_at_xy(
                        &self.editor,
                        px,
                        hover_content_y,
                    )
                } else {
                    self.editor.len()
                }
            };
            let hover_on_inlay_hint = if diag_hover_byte.is_none() {
                let line_top_y = (hover_content_y / line_h).floor() * line_h;
                hover_content_y_in_line_hitbox(hover_content_y, line_top_y, line_h)
                    && self.renderer.as_mut().unwrap().is_inlay_hint_at_xy(
                        &self.editor,
                        px,
                        hover_content_y,
                    )
            } else {
                false
            };
            let cleared_inlay_hover = if hover_on_inlay_hint {
                clear_hover_popup(self.renderer.as_mut())
            } else {
                false
            };
            let in_diag_popup = HOVER_STATE
                .with(|s| s.borrow().diag_rect)
                .map(|(rx, ry, rw, rh, _, _, _)| {
                    position.x as f32 >= rx
                        && position.x as f32 <= rx + rw
                        && position.y as f32 >= ry
                        && position.y as f32 <= ry + rh
                })
                .unwrap_or(false);
            let is_text_area = (!in_diag_popup || in_hover_popup)
                && !hover_on_inlay_hint
                && position.x as f32 > padding
                && (position.x as f32) < (window_size.width as f32 - minimap_w);
            let ctrl_definition_byte = if is_text_area {
                normalize_hover_byte(&self.editor, byte_offset)
            } else {
                None
            };

            let mut clear_diag_popup = false;
            HOVER_STATE.with(|state| {
                let mut state = state.borrow_mut();
                let keep_visible_popup = state.popup.is_some();
                let old_byte = state.byte_offset;
                if let Some(should_clear_diag) =
                    crate::app::mouse::update_editor_hover_state_for_cursor(
                        &mut state,
                        &self.editor,
                        byte_offset,
                        diag_hover_byte,
                        is_text_area,
                        in_hover_popup,
                        in_hover_source_line,
                        editor_text_selecting,
                    )
                {
                    if should_clear_diag && crate::render_view::hover_trace_enabled() {
                        println!("[HOVER DEBUG] cursor -> new word ({}). old_byte: {:?}. keep_old_popup: {}. start 0.34s request timer.", byte_offset, old_byte, keep_visible_popup);
                    }
                    clear_diag_popup = should_clear_diag;
                }
            });
            if clear_diag_popup {
                HOVER_STATE.with(|s| s.borrow_mut().reset_diagnostic_popup());
            }
            self.update_ctrl_definition_hover(ctrl_definition_byte);
            if cleared_inlay_hover {
                self.window.as_ref().unwrap().request_redraw();
            }
        }
        let wh = window_size.height as f32;
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
            let w = (1000.0 * s)
                .min(self.window.as_ref().unwrap().inner_size().width as f32 - 40.0 * s);
            let x = ((self.window.as_ref().unwrap().inner_size().width as f32 - w) / 2.0).round();
            let content_x = x + 40.0 * s + 200.0 * s + 30.0 * s;
            let start_x = content_x + 8.0 * s;
            let text = self.settings_ignore_editor.get_full_text();
            let x_offset = (position.x as f32 - start_x + self.settings_ignore_scroll_x).max(0.0);
            let mut current_x = 0.0;
            let mut target_idx = text.len();
            let mut byte_idx = 0;
            for c in text.chars() {
                let adv = self
                    .renderer
                    .as_mut()
                    .unwrap()
                    .get_ui_glyph(c)
                    .map(|g| g.advance)
                    .unwrap_or(10.0)
                    * 0.95;
                if x_offset <= current_x + adv / 2.0 {
                    target_idx = byte_idx;
                    break;
                }
                current_x += adv;
                byte_idx += c.len_utf8();
            }
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
            let active = self.ide_panel.active_terminal;
            if let Some(term) = self.ide_panel.terminals.get_mut(active) {
                let grid = term.grid.lock().unwrap();
                let is_alt = grid.is_alt;
                drop(grid);
                if is_alt {
                    term.scroll_y.is_dragging = false;
                    return;
                }

                let s = self.renderer.as_ref().unwrap().scale_factor;
                let bottom_h = self.ide_panel.bottom_height * s;
                let tab_h = 32.0 * s;
                let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                let content_y =
                    crate::render_view::ide_bottom_panel_y(wh, bottom_h, s) + 1.0 + tab_h;
                let content_h = bottom_h - 1.0 - tab_h;
                let (term_content_y, term_content_h) =
                    crate::render_view::terminal_ui::terminal_body_rect(content_y, content_h, s);

                let lh = self.renderer.as_ref().unwrap().line_height;
                let char_h = lh * 1.05;

                let grid = term.grid.lock().unwrap();
                let is_alt = grid.is_alt;
                let scrollback_len = if is_alt { 0 } else { grid.scrollback.len() };
                let total_lines = scrollback_len + grid.lines.len();
                drop(grid);

                let max_scroll = if is_alt {
                    0.0
                } else {
                    ((total_lines as f32 * char_h) - term_content_h).max(0.0)
                };
                if max_scroll > 0.0 {
                    let track_h = term_content_h;
                    let ratio = ((position.y as f32 - term_content_y) / track_h).clamp(0.0, 1.0);
                    let progress = 1.0 - ratio;
                    term.scroll_y.target = progress * max_scroll;
                    term.scroll_y.current = term.scroll_y.target;
                    self.window.as_ref().unwrap().request_redraw();
                }
            }
        } else if self.ide_panel.problems_scroll.is_dragging {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            let wh = self.window.as_ref().unwrap().inner_size().height as f32;
            let bottom_h = self.ide_panel.bottom_height * s;
            let cy = crate::render_view::ide_bottom_panel_y(wh, bottom_h, s);

            let item_h = 24.0 * s;
            let total_h = self.ide_panel.flat_diags.len() as f32 * item_h;
            let track_h = bottom_h - 40.0 * s;
            let max_scroll = (total_h - track_h).max(0.0);
            let thumb_h = (track_h / total_h * track_h).max(20.0 * s);
            let list_y = cy + 40.0 * s;

            let ratio = (position.y as f32 - list_y - self.ide_panel.problems_scroll.drag_offset)
                / (track_h - thumb_h).max(0.0001);
            self.ide_panel.problems_scroll.target = (ratio * max_scroll).clamp(0.0, max_scroll);
            self.ide_panel.problems_scroll.current = self.ide_panel.problems_scroll.target;
            self.window.as_ref().unwrap().request_redraw();
            return;
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
                    let (_, by, _, box_h) = rect;
                    let max_scroll = state.max_scroll;
                    let track_h = box_h - 16.0 * s;
                    let thumb_h = (box_h / (box_h + max_scroll) * track_h).max(20.0 * s);
                    if let Some(popup) = &mut state.popup {
                        let ratio = (position.y as f32 - by - 8.0 * s - popup.scroll.drag_offset)
                            / (track_h - thumb_h).max(0.0001);
                        popup.scroll.target = (ratio * max_scroll).clamp(0.0, max_scroll);
                        popup.scroll.current = popup.scroll.target;
                    }
                }
            });
            self.window.as_ref().unwrap().request_redraw();
            return;
        } else if self.ide_panel.lsp_scroll_y.is_dragging {
            let s = self.renderer.as_ref().unwrap().scale_factor;
            if let Some((_, cy, _, ch)) = self.lsp_panel_bounds() {
                let total_h = self.lsp_panel_total_h(s);
                let track_h = ch - 10.0 * s;
                let max_y = (total_h - ch).max(0.0);
                let thumb_h = (ch / total_h * track_h).max(40.0 * s);
                let ratio =
                    (position.y as f32 - cy - 5.0 * s - self.ide_panel.lsp_scroll_y.drag_offset)
                        / (track_h - thumb_h).max(0.0001);
                self.ide_panel.lsp_scroll_y.target = (ratio * max_y).clamp(0.0, max_y);
                self.ide_panel.lsp_scroll_y.current = self.ide_panel.lsp_scroll_y.target;
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
                            let max_y = (inner_total_h - log_bg_h).max(0.0);
                            let track_h = log_bg_h - 14.0 * s;
                            let thumb_h = (log_bg_h / inner_total_h * track_h).max(20.0 * s);
                            let sy = self.ide_panel.lsp_logs_scroll_y.get_mut(&name).unwrap();
                            let ratio = (position.y as f32 - log_bg_y - 2.0 * s - sy.drag_offset)
                                / (track_h - thumb_h).max(0.0001);
                            sy.target = (ratio * max_y).clamp(0.0, max_y);
                            sy.current = sy.target;
                        } else if is_drag_x {
                            let max_x = (inner_max_w + 20.0 * s - log_bg_w).max(0.0);
                            let track_w = log_bg_w - 14.0 * s;
                            let thumb_w =
                                (log_bg_w / (inner_max_w + 20.0 * s) * track_w).max(20.0 * s);
                            let sx = self.ide_panel.lsp_logs_scroll_x.get_mut(&name).unwrap();
                            let ratio = (position.x as f32 - log_bg_x - 2.0 * s - sx.drag_offset)
                                / (track_w - thumb_w).max(0.0001);
                            sx.target = (ratio * max_x).clamp(0.0, max_x);
                            sx.current = sx.target;
                        }
                    }
                    break;
                }
            }
        } else if self.is_dragging_search {
            let search_w = 480.0 * s;
            let input_x = if self.ide_panel.git.message_focused {
                let panel_w = self.ide_panel.left_width * s;
                let pad = (10.0 * s).min((panel_w * 0.15).max(0.0));
                48.0 * s + pad
            } else if self.ide_panel.term_search_focused {
                let panel_w = self.window.as_ref().unwrap().inner_size().width as f32 - 48.0 * s;
                48.0 * s + panel_w - search_w - 20.0 * s + 10.0 * s
            } else {
                scrollbar_x - search_w - 20.0 * s + 10.0 * s
            };

            let text = if self.ide_panel.git.message_focused {
                self.ide_panel.git.message_editor.get_full_text()
            } else if self.ide_panel.term_search_focused {
                self.ide_panel.term_search_editor.get_full_text()
            } else {
                self.search_editor.get_full_text()
            };

            let x_offset = (position.x as f32 - (input_x + 5.0 * s)
                + self
                    .renderer
                    .as_ref()
                    .map(|r| {
                        if self.ide_panel.git.message_focused {
                            r.search_scroll_x
                        } else {
                            0.0
                        }
                    })
                    .unwrap_or(0.0))
            .max(0.0);
            let mut current_x = 0.0;
            let mut target_idx = text.len();
            let mut byte_idx = 0;

            for c in text.chars() {
                let adv = self
                    .renderer
                    .as_mut()
                    .unwrap()
                    .get_ui_glyph(c)
                    .map(|g| g.advance)
                    .unwrap_or(10.0);
                if x_offset <= current_x + adv / 2.0 {
                    target_idx = byte_idx;
                    break;
                }
                current_x += adv;
                byte_idx += c.len_utf8();
            }
            if self.ide_panel.git.message_focused {
                self.ide_panel.git.message_editor.cursor = target_idx;
            } else if self.ide_panel.term_search_focused {
                self.ide_panel.term_search_editor.cursor = target_idx;
            } else {
                self.search_editor.cursor = target_idx;
            }
        } else if self.scroll_x.is_dragging {
            let r = self.renderer.as_ref().unwrap();
            let track_w = scrollbar_x - padding;
            let max_x = r.max_scroll_x;
            let thumb_w = (track_w / (max_x + track_w).max(1.0) * track_w).max(40.0 * s);
            let ratio = (position.x as f32 - padding - self.scroll_x.drag_offset)
                / (track_w - thumb_w).max(0.0001);
            self.scroll_x.target = (ratio * max_x).clamp(0.0, max_x);
            self.scroll_x.current = self.scroll_x.target;
        } else if self.scroll_y.is_dragging {
            let now = std::time::Instant::now();
            let elapsed = now.duration_since(self.last_click_time).as_millis();
            let dy = (position.y as f32 - self.last_click_pos.1).abs();

            if elapsed > 120 || dy > 10.0 {
                let r = self.renderer.as_ref().unwrap();
                let s = r.scale_factor;
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
                };

                let track_h = editor_height;
                let track_start_y = tab_bar_h;
                let last_mouse_y = r.last_mouse_y;

                let scroll_ratio = (last_mouse_y - track_start_y - self.scroll_y.drag_offset)
                    / (track_h - thumb_h).max(0.0001);

                self.scroll_y.target = (scroll_ratio * max_scroll).clamp(0.0, max_scroll).round();
                self.scroll_y.anim_speed = 15.0;
            }
        } else if self.ide_panel.is_dragging_terminal && self.is_dragging && !self.show_settings {
            let active = self.ide_panel.active_terminal;
            if let Some(term) = self.ide_panel.terminals.get_mut(active) {
                let s = self.renderer.as_ref().unwrap().scale_factor;
                let bottom_h = self.ide_panel.bottom_height * s;
                let tab_h = 32.0 * s;
                let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                let content_y =
                    crate::render_view::ide_bottom_panel_y(wh, bottom_h, s) + 1.0 + tab_h;
                let content_h = bottom_h - 1.0 - tab_h;
                let (term_content_y, term_content_h) =
                    crate::render_view::terminal_ui::terminal_body_rect(content_y, content_h, s);

                let lh = self.renderer.as_ref().unwrap().line_height;
                let char_h = lh * 1.05;
                let char_w = self.renderer.as_mut().unwrap().char_advance('A') * 1.05;
                let panel_x = 48.0 * s + 10.0 * s;

                let py = position.y as f32;
                let px = position.x as f32;

                let mut grid = term.grid.lock().unwrap();
                let scrollback_len = if grid.is_alt {
                    0
                } else {
                    grid.scrollback.len()
                };
                let total_lines = scrollback_len + grid.lines.len();
                let max_scroll = if grid.is_alt {
                    0.0
                } else {
                    ((total_lines as f32 * char_h) - term_content_h).max(0.0)
                };

                let scroll_offset = if grid.is_alt {
                    0.0
                } else {
                    term.scroll_y.current.min(max_scroll)
                };
                let offset_from_bottom =
                    (term_content_y + term_content_h - 8.0 * s - py + scroll_offset) / char_h;
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
            let tab_bar_h = if self.show_welcome || !self.is_ide_mode {
                0.0
            } else {
                38.0 * self.renderer.as_ref().unwrap().scale_factor
            };
            self.editor.set_cursor_at_pos(
                last_mouse_x,
                last_mouse_y - tab_bar_h + self.scroll_y.current,
                self.renderer.as_mut().unwrap(),
                false,
            );
            clear_hover_popup(self.renderer.as_mut());
        }

        self.window.as_ref().unwrap().request_redraw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autocomplete_drag_target_clamps_to_available_scroll() {
        assert_eq!(autocomplete_scrollbar_track_margin(1.0), 3.0);
        assert_eq!(autocomplete_drag_target(0.0, 10.0, 200.0, 0.0, 4, 1.0), 0.0);

        let mid = autocomplete_drag_target(100.0, 10.0, 200.0, 0.0, 12, 1.0);
        let max = autocomplete_drag_target(999.0, 10.0, 200.0, 0.0, 12, 1.0);

        assert!(mid > 0.0);
        assert_eq!(max, (12.0 - 7.0) * 36.0);
    }

    #[test]
    fn autocomplete_hovered_index_ignores_scrollbar_and_outside_rect() {
        let rect = (10.0, 20.0, 200.0, 160.0);

        assert_eq!(
            autocomplete_hovered_index(20.0, 30.0, rect, 0.0, 10, 1.0),
            Some(0)
        );
        assert_eq!(
            autocomplete_hovered_index(20.0, 70.0, rect, 0.0, 10, 1.0),
            Some(1)
        );
        assert_eq!(
            autocomplete_hovered_index(20.0, 30.0, rect, 72.0, 10, 1.0),
            Some(2)
        );

        assert_eq!(
            autocomplete_hovered_index(999.0, 30.0, rect, 0.0, 10, 1.0),
            None
        );
        assert_eq!(
            autocomplete_hovered_index(205.0, 30.0, rect, 0.0, 10, 1.0),
            None
        );
        assert_eq!(
            autocomplete_hovered_index(20.0, 500.0, rect, 0.0, 10, 1.0),
            None
        );
        assert_eq!(
            autocomplete_hovered_index(20.0, 30.0, rect, 0.0, 0, 1.0),
            None
        );
    }

    #[test]
    fn resize_helpers_clamp_left_width_and_bottom_height() {
        assert_eq!(resized_left_width(0.0, 1200.0, 1.0), 80.0);
        assert_eq!(resized_left_width(248.0, 1200.0, 1.0), 200.0);
        assert_eq!(resized_left_width(5000.0, 1200.0, 1.0), 852.0);

        assert_eq!(resized_bottom_height(2000.0, 900.0, 1.0), 60.0);
        assert_eq!(resized_bottom_height(690.0, 900.0, 1.0), 180.0);
        assert_eq!(resized_bottom_height(600.0, 900.0, 1.0), 270.0);
        assert_eq!(resized_bottom_height(0.0, 900.0, 1.0), 820.0);
    }

    #[test]
    fn cursor_hover_requires_pointer_inside_window() {
        assert!(cursor_position_allows_editor_hover(0.0, 0.0, 800.0, 600.0));
        assert!(cursor_position_allows_editor_hover(
            799.0, 599.0, 800.0, 600.0
        ));
        assert!(!cursor_position_allows_editor_hover(
            -1.0, 10.0, 800.0, 600.0
        ));
        assert!(!cursor_position_allows_editor_hover(
            10.0, -1.0, 800.0, 600.0
        ));
        assert!(!cursor_position_allows_editor_hover(
            800.0, 10.0, 800.0, 600.0
        ));
        assert!(!cursor_position_allows_editor_hover(
            10.0, 600.0, 800.0, 600.0
        ));
    }

    #[test]
    fn editor_scrollbar_drag_suppresses_hover_only_while_dragging() {
        assert!(!should_suppress_editor_hover_for_scroll_drag(false, false));
        assert!(should_suppress_editor_hover_for_scroll_drag(true, false));
        assert!(should_suppress_editor_hover_for_scroll_drag(false, true));
        assert!(should_suppress_editor_hover_for_scroll_drag(true, true));
    }
}
