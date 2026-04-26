use super::*;

impl App {
    pub fn handle_main_cursor_moved(&mut self, position: winit::dpi::PhysicalPosition<f64>) {
        self.renderer.as_mut().unwrap().last_mouse_x = position.x as f32;
        self.renderer.as_mut().unwrap().last_mouse_y = position.y as f32;

        if self.dialog_window.is_some() {
            return;
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

        let editor_text_selecting =
            self.is_dragging && !self.ide_panel.is_dragging_terminal && !self.show_settings;
        if editor_text_selecting {
            clear_hover_popup(self.renderer.as_mut());
            if let Some(r) = self.renderer.as_mut() {
                r.hide_popups_until_mouse_move = true;
            }
        }

        let s = self.renderer.as_ref().unwrap().scale_factor;

        if let (true, Some((rx, ry, rw, rh))) = (self.autocomplete_active, self.autocomplete_rect) {
            let px = position.x as f32;
            let py = position.y as f32;

            if self.autocomplete_scroll.is_dragging {
                self.autocomplete_scroll.anim_speed = 15.0;
                let step = 36.0 * s;
                let total_items = self.autocomplete_options.len() as f32;
                let visible_items = total_items.min(7.0);

                let track_h = rh - 8.0 * s;
                let total_h = total_items * step + 16.0 * s;
                let thumb_h = (rh / total_h * track_h).max(20.0 * s);
                let max_scroll = ((total_items - visible_items) * step).max(0.0);

                let ratio = (py - ry - 4.0 * s - self.autocomplete_scroll.drag_offset)
                    / (track_h - thumb_h).max(1.0);
                self.autocomplete_scroll.target = (ratio * max_scroll).clamp(0.0, max_scroll);
                self.window.as_ref().unwrap().request_redraw();
                return;
            }

            if px >= rx && px <= rx + rw && py >= ry && py <= ry + rh {
                let scroll_x = rx + rw - 14.0 * s;
                if px < scroll_x {
                    let item_h = 36.0 * s;
                    let scroll = self.autocomplete_scroll.current;
                    let content_y = py - ry + scroll - (4.0 * s);
                    if content_y >= 0.0 {
                        let idx = (content_y / item_h) as usize;
                        if idx < self.autocomplete_options.len() {
                            self.autocomplete_hovered_idx = Some(idx);
                        } else {
                            self.autocomplete_hovered_idx = None;
                        }
                    }
                } else {
                    self.autocomplete_hovered_idx = None;
                }
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

            if self.ide_panel.is_resizing_left {
                let sb_w = 48.0 * s;
                let ww = self.window.as_ref().unwrap().inner_size().width as f32;
                let max_w = ((ww - sb_w) / s) - 300.0;
                let new_w = ((px - sb_w) / s).max(80.0).min(max_w.max(80.0));
                self.ide_panel.left_width = new_w;
                self.window.as_ref().unwrap().request_redraw();
                return;
            }

            if self.ide_panel.is_resizing_bottom {
                let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                let max_h = (wh / s) - 50.0;
                let new_h = ((wh - py) / s).max(60.0).min(max_h.max(60.0));
                self.ide_panel.bottom_height = new_h;
                self.window.as_ref().unwrap().request_redraw();
                return;
            }
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
                (
                    48.0 * s,
                    wh - panel_bottom_h + 1.0 + tab_h,
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
        let mut in_hover_popup = false;
        let mut in_hover_source_line = false;

        let (type_rect, popup_meta) = HOVER_STATE.with(|state| {
            let state = state.borrow();
            (
                state.rect,
                state
                    .popup
                    .as_ref()
                    .map(|popup| (popup.anchor_x, popup.anchor_y, popup.byte_offset)),
            )
        });
        let diag_rect_full = HOVER_STATE.with(|s| s.borrow().diag_rect);
        let diag_rect = diag_rect_full.map(|(x, y, w, h, _, _, _)| (x, y, w, h));

        if type_rect.is_some() || diag_rect.is_some() {
            let mut union_rect = diag_rect.unwrap_or_else(|| type_rect.unwrap());
            if let (Some(r1), Some(r2)) = (diag_rect, type_rect) {
                let x_min = r1.0.min(r2.0);
                let y_min = r1.1.min(r2.1);
                let x_max = (r1.0 + r1.2).max(r2.0 + r2.2);
                let y_max = (r1.1 + r1.3).max(r2.1 + r2.3);
                union_rect = (x_min, y_min, x_max - x_min, y_max - y_min);
            }
            let pad = 24.0 * s;
            if position.x as f32 >= union_rect.0 - pad
                && position.x as f32 <= union_rect.0 + union_rect.2 + pad
                && position.y as f32 >= union_rect.1 - pad
                && position.y as f32 <= union_rect.1 + union_rect.3 + pad
            {
                in_hover_popup = true;
            }
        }

        if type_rect.is_some() || diag_rect_full.is_some() {
            if let (Some((rx, ry, rw, rh)), Some((anchor_x, anchor_y, popup_byte_offset))) =
                (type_rect, popup_meta)
            {
                let phys_line = self
                    .editor
                    .line_offsets
                    .partition_point(|&o| o <= popup_byte_offset)
                    .saturating_sub(1);
                let vis_line_idx = self
                    .renderer
                    .as_ref()
                    .unwrap()
                    .phys_to_visual
                    .get(phys_line)
                    .copied()
                    .unwrap_or(0) as f32;
                let line_top_y = (vis_line_idx * self.renderer.as_ref().unwrap().line_height)
                    - (self.scroll_y.current.round()
                        - if self.show_welcome || !self.is_ide_mode {
                            0.0
                        } else {
                            38.0 * s
                        });
                let line_bottom_y = line_top_y + self.renderer.as_ref().unwrap().line_height;

                let px = position.x as f32;
                let py = position.y as f32;
                if is_in_hover_popup_or_bridge(
                    px,
                    py,
                    (rx, ry, rw, rh),
                    anchor_x,
                    anchor_y,
                    line_top_y,
                    line_bottom_y,
                    self.renderer.as_ref().unwrap().width,
                    s,
                ) {
                    in_hover_popup = true;
                    if py >= line_top_y && py <= line_bottom_y {
                        in_hover_source_line = true;
                    }
                }
            }

            if !in_hover_popup {
                if let Some((rx, ry, rw, rh, anchor_x_start, anchor_x_end, anchor_y)) =
                    diag_rect_full
                {
                    let anchor_x = (anchor_x_start + anchor_x_end) * 0.5;
                    let px = position.x as f32;
                    let py = position.y as f32;
                    if is_in_hover_popup_or_bridge(
                        px,
                        py,
                        (rx, ry, rw, rh),
                        anchor_x,
                        anchor_y,
                        anchor_y - 10.0 * s,
                        anchor_y + 10.0 * s,
                        self.renderer.as_ref().unwrap().width,
                        s,
                    ) {
                        in_hover_popup = true;
                        if py >= anchor_y - 10.0 * s && py <= anchor_y + 10.0 * s {
                            in_hover_source_line = true;
                        }
                    }
                }
            }
        }

        let minimap_w = self.renderer.as_ref().unwrap().minimap_width;
        let padding = self.renderer.as_ref().unwrap().left_padding;
        let window_size = self.window.as_ref().unwrap().inner_size();

        if !in_hover_popup || in_hover_source_line {
            let tab_bar_h = if self.show_welcome || !self.is_ide_mode {
                0.0
            } else {
                38.0 * s
            };
            let render_scroll_y = self.scroll_y.current.round() - tab_bar_h;
            let px = position.x as f32;
            let py = position.y as f32;
            let mut diag_hover_byte = None;

            if let (Some(lsp), Some(path)) = (self.lsp.as_ref(), self.file_path.as_ref()) {
                let (_, diagnostics) = lsp.get_instant_diagnostics_with_version(path);
                let render_scroll_x = self.scroll_x.current.round();
                let line_h = self.renderer.as_ref().unwrap().line_height;
                let left_padding = self.renderer.as_ref().unwrap().left_padding;
                let last_line = self.editor.line_offsets.len().saturating_sub(1);

                'diag_scan: for diag in diagnostics {
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
                        let line_top_y = (vis_line_idx * line_h) - render_scroll_y;
                        let line_bottom_y = line_top_y + line_h;

                        if py < line_top_y || py > line_bottom_y {
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

                        let Some((hit_start_byte, hit_end_byte, type_target)) =
                            diagnostic_hover_byte_range_on_line(
                                &self.editor,
                                line,
                                start_col,
                                end_col,
                            )
                        else {
                            continue;
                        };

                        let mut hit_x_start_px = x_start_px;
                        let mut hit_x_end_px = x_end_px;
                        let mut hit_cur_x = 0.0f32;
                        let mut hit_start_found = false;
                        let mut hit_end_found = false;
                        self.editor
                            .utf16_col_to_byte_advance(line, |ch, _utf16_before, pos| {
                                if !hit_start_found && pos >= hit_start_byte {
                                    hit_x_start_px = hit_cur_x;
                                    hit_start_found = true;
                                }
                                if !hit_end_found && pos >= hit_end_byte {
                                    hit_x_end_px = hit_cur_x;
                                    hit_end_found = true;
                                }
                                hit_cur_x += if ch == '\t' {
                                    self.renderer.as_mut().unwrap().char_advance(' ') * 4.0
                                } else {
                                    self.renderer.as_mut().unwrap().char_advance(ch)
                                };
                            });
                        if !hit_start_found {
                            hit_x_start_px = hit_cur_x;
                        }
                        if !hit_end_found {
                            hit_x_end_px = hit_cur_x.max(hit_x_start_px + avg_adv * 4.0);
                        }

                        let hit_x_start = left_padding + hit_x_start_px - render_scroll_x;
                        let hit_x_end = left_padding + hit_x_end_px - render_scroll_x;
                        let hit_w = (hit_x_end - hit_x_start).max(avg_adv / 2.0);

                        if px < hit_x_start || px > hit_x_start + hit_w {
                            continue;
                        }

                        diag_hover_byte = Some(type_target);
                        break 'diag_scan;
                    }
                }
            }

            let byte_offset = if let Some(byte) = diag_hover_byte {
                byte
            } else {
                self.renderer.as_mut().unwrap().get_byte_at_xy(
                    &self.editor,
                    px,
                    py + render_scroll_y,
                )
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
                && position.x as f32 > padding
                && (position.x as f32) < (window_size.width as f32 - minimap_w);

            let mut clear_diag_popup = false;
            HOVER_STATE.with(|state| {
                let mut state = state.borrow_mut();
                if editor_text_selecting {
                    return;
                }
                if is_text_area {
                    let normalized = normalize_hover_byte(&self.editor, byte_offset);
                    if normalized.is_none() {
                        if let Some(diag_byte) = diag_hover_byte {
                            if !in_hover_popup {
                                // We are over a diagnostic squiggle, but not a text word.
                                // Trigger hover for the diagnostic!
                                if state.byte_offset != Some(diag_byte) {
                                    let keep_visible = state.popup.is_some();
                                    state.byte_offset = Some(diag_byte);
                                    state.timer = 0.0;
                                    state.request_id = None;
                                    state.definition_request_id = None;
                                    state.pending_popup = None;
                                    state.selection_anchor = None;
                                    state.selection_cursor = None;
                                    state.selecting = false;
                                    if !keep_visible {
                                        state.popup = None;
                                        state.rect = None;
                                    }
                                }
                            }
                        } else if !in_hover_popup {
                            if state.keep_active_combined_popup_on_empty_space() {
                                return;
                            }
                            if state.byte_offset.is_some()
                                && !state.should_keep_popup_through_empty_space()
                            {
                                println!("[HOVER DEBUG] cursor -> empty space. byte_offset=None. start 0.25s hide timer.");
                                state.byte_offset = None;
                                state.timer = 0.0;
                                state.request_id = None;
                                state.definition_request_id = None;
                            }
                        }
                        return;
                    }
                    let byte_offset = normalized.unwrap_or(byte_offset);
                    let mut same_word = false;
                    if let Some(old_byte) = state.byte_offset {
                        let (old_start, old_end) = hover_token_bounds(&self.editor, old_byte);
                        let (new_start, new_end) = hover_token_bounds(&self.editor, byte_offset);
                        same_word = old_start == new_start && old_end == new_end;
                    }
                    if !same_word && (!in_hover_popup || in_hover_source_line) {
                        let keep_visible_popup = state.popup.is_some();
                        println!("[HOVER DEBUG] cursor -> new word ({}). old_byte: {:?}. keep_old_popup: {}. start 0.34s request timer.", byte_offset, state.byte_offset, keep_visible_popup);
                        clear_diag_popup = state.begin_type_hover_transition(byte_offset);
                    }
                } else if !in_hover_popup {
                    if state.byte_offset.is_some()
                        && !state.should_keep_popup_through_empty_space()
                    {
                        println!("[HOVER DEBUG] cursor out of bounds. byte_offset=None. start 0.25s hide timer.");
                        state.byte_offset = None;
                        state.timer = 0.0;
                        state.request_id = None;
                        state.definition_request_id = None;
                    }
                }
            });
            if clear_diag_popup {
                HOVER_STATE.with(|s| s.borrow_mut().reset_diagnostic_popup());
            }
        }
        let wh = window_size.height as f32;

        let max_scroll = self
            .renderer
            .as_mut()
            .unwrap()
            .get_max_scroll(&self.editor, wh);
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
                if let Some((cx, cy, _cw, _ch)) = self.lsp_panel_bounds() {
                    let pad_x = 12.0 * s;
                    let btn_h = 24.0 * s;
                    let scroll_y = self.ide_panel.lsp_scroll_y.current.round();
                    let mut cur_y = cy + 8.0 * s - scroll_y;

                    for srv in self.ide_panel.lsp_servers.clone().iter() {
                        let logs_h = self.lsp_server_logs_h(srv, s);
                        let is_exp = logs_h > 0.0;
                        let row_h = 136.0 * s + logs_h;

                        if srv.name == focused_name.as_str() && is_exp {
                            let card_x = cx + 12.0 * s;
                            let btn_y1 = cur_y + 56.0 * s;
                            let btn_y2 = btn_y1 + btn_h + 8.0 * s;
                            let log_bg_x = card_x + pad_x;
                            let log_bg_y = btn_y2 + btn_h + 10.0 * s;

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
                let content_y = self.window.as_ref().unwrap().inner_size().height as f32 - bottom_h
                    + 1.0
                    + tab_h;
                let content_h = bottom_h - 1.0 - tab_h;
                let term_content_y = content_y + 32.0 * s;
                let term_content_h = content_h - 32.0 * s;

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
            let cy = wh - bottom_h;

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
                    if let Some((cx, cy, cw, _ch)) = self.lsp_panel_bounds() {
                        let scroll_y = self.ide_panel.lsp_scroll_y.current.round();
                        let mut current_y = cy + 8.0 * s - scroll_y;
                        for (i, srv) in self.ide_panel.lsp_servers.iter().enumerate() {
                            if i == idx {
                                break;
                            }
                            let logs_h = self.lsp_server_logs_h(srv, s);
                            current_y += 136.0 * s + logs_h + 16.0 * s;
                        }

                        let logs_h = self.lsp_server_logs_h(info, s);
                        let btn_y1 = current_y + 56.0 * s;
                        let btn_h = 24.0 * s;
                        let btn_y2 = btn_y1 + btn_h + 8.0 * s;
                        let log_bg_y = btn_y2 + btn_h + 10.0 * s;
                        let log_bg_x = cx + 24.0 * s;
                        let log_bg_w = cw - 48.0 * s;
                        let log_bg_h = logs_h - 18.0 * s;

                        let (inner_total_h, inner_max_w) = self.lsp_server_inner_size(info, s);

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
            let input_x = if self.ide_panel.term_search_focused {
                let panel_w = self.window.as_ref().unwrap().inner_size().width as f32 - 48.0 * s;
                48.0 * s + panel_w - search_w - 20.0 * s + 10.0 * s
            } else {
                scrollbar_x - search_w - 20.0 * s + 10.0 * s
            };

            let text = if self.ide_panel.term_search_focused {
                self.ide_panel.term_search_editor.get_full_text()
            } else {
                self.search_editor.get_full_text()
            };

            let x_offset = (position.x as f32 - (input_x + 5.0 * s)).max(0.0);
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
            if self.ide_panel.term_search_focused {
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
                let editor_height = wh - tab_bar_h;
                let minimap_w = r.minimap_width;

                let is_minimap_drag = self.last_click_pos.0
                    >= (self.window.as_ref().unwrap().inner_size().width as f32 - minimap_w);

                let thumb_h = if is_minimap_drag {
                    let total_lines_f32 = self.editor.line_offsets.len() as f32;
                    let visible_minimap_lines = total_lines_f32.min(900.0);
                    let minimap_line_h =
                        (editor_height / (visible_minimap_lines + 2.0).max(1.0)).max(1.5);
                    let visible_lines = editor_height / r.line_height;
                    (visible_lines * minimap_line_h).max(4.0)
                } else {
                    let total_content_height =
                        (self.editor.line_offsets.len() as f32 + 2.0) * r.line_height;
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
                let content_y = self.window.as_ref().unwrap().inner_size().height as f32 - bottom_h
                    + 1.0
                    + tab_h;
                let content_h = bottom_h - 1.0 - tab_h;
                let term_content_y = content_y + 32.0 * s;
                let term_content_h = content_h - 32.0 * s;

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
