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
    let offset_from_bottom =
        (term_content_y + term_content_h - 8.0 * scale - my + scroll_offset) / char_h;
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

#[cfg(test)]
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

impl App {
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn handle_main_mouse_input(
        &mut self,
        _event_loop: &ActiveEventLoop,
        state: ElementState,
        button: winit::event::MouseButton,
    ) {
        let mx = self.renderer.as_ref().unwrap().last_mouse_x;
        let my = self.renderer.as_ref().unwrap().last_mouse_y;
        if state == ElementState::Released && self.autocomplete_detail_selecting {
            self.autocomplete_detail_selecting = false;
            self.window.as_ref().unwrap().request_redraw();
            return;
        }
        if state == ElementState::Released {
            if let Some(popup) = &mut self.autocomplete_detail_popup {
                popup.scroll.is_dragging = false;
            }
            if let Some(renderer) = self.renderer.as_mut()
                && renderer.git_graph_tooltip_selecting
            {
                renderer.git_graph_tooltip_selecting = false;
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
            && button != winit::event::MouseButton::Left
            && self.autocomplete_window_contains(mx, my)
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
                            let (_, by, _, box_h) = rect;
                            let track_h = box_h - 16.0 * s;
                            let thumb_h = (box_h / (box_h + max_scroll) * track_h).max(20.0 * s);
                            let thumb_y = by
                                + 8.0 * s
                                + (popup.scroll.current / max_scroll) * (track_h - thumb_h);
                            if my >= thumb_y && my <= thumb_y + thumb_h {
                                popup.scroll.is_dragging = true;
                                popup.scroll.drag_offset = my - thumb_y;
                            } else {
                                popup.scroll.anim_speed = 15.0;
                                popup.scroll.drag_offset = thumb_h / 2.0;
                                let ratio = (my - by - 8.0 * s - popup.scroll.drag_offset)
                                    / (track_h - thumb_h).max(0.0001);
                                popup.scroll.target = (ratio * max_scroll).clamp(0.0, max_scroll);
                                popup.scroll.current = popup.scroll.target;
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
                                self.request_autocomplete_detail_for_index(idx);
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
                    if term.grid.lock().unwrap().mouse_tracking {
                        tracking = true;
                    }
                }
                if tracking {
                    let btn_code = terminal_mouse_button_code(button);
                    let is_pressed = state == ElementState::Pressed;
                    let s = self.renderer.as_ref().unwrap().scale_factor;
                    let panel_x = 48.0 * s + 10.0 * s;
                    let char_w = self.renderer.as_mut().unwrap().char_advance('A') * 1.05;
                    let char_h = self.renderer.as_ref().unwrap().line_height * 1.05;
                    let bottom_h = self.ide_panel.bottom_height * s;
                    let tab_h = 32.0 * s;
                    let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                    let content_y =
                        crate::render_view::ide_bottom_panel_y(wh, bottom_h, s) + 1.0 + tab_h;
                    let content_h = bottom_h - 1.0 - tab_h;
                    let (term_content_y, term_content_h) =
                        crate::render_view::terminal_ui::terminal_body_rect(
                            content_y, content_h, s,
                        );

                    let cell_x = terminal_mouse_cell_x(mx, panel_x, char_w);

                    let mut is_drag = false;
                    let mut cell_y = 1;
                    if let Some(term) = self.ide_panel.terminals.get_mut(active) {
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
                            if let Ok(mut w) = term.writer.lock() {
                                let _ = w.write_all(seq.as_bytes());
                                let _ = w.flush();
                            }
                        }
                    }
                }
            }
        }

        if state == ElementState::Pressed && button == winit::event::MouseButton::Left {
            if let Some(menu) = self.lsp_actions_menu.as_ref() {
                let s = self.renderer.as_ref().unwrap().scale_factor;
                let mut clicked_inside = false;
                if state == ElementState::Pressed {
                    let item_h = 36.0 * s;
                    let menu_w = 320.0 * s;
                    let menu_h = menu.items.len() as f32 * item_h + 8.0 * s;
                    let tab_bar_h = if self.show_welcome || !self.is_ide_mode {
                        0.0
                    } else {
                        38.0 * s
                    };
                    let menu_y = menu.menu_y + tab_bar_h;
                    if mx >= menu.menu_x
                        && mx <= menu.menu_x + menu_w
                        && my >= menu_y
                        && my <= menu_y + menu_h
                    {
                        clicked_inside = true;
                        let rel_y = my - menu_y - 4.0 * s;
                        let idx = (rel_y / item_h) as usize;
                        if idx < menu.items.len() {
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
                if self.is_ide_mode && self.ide_panel.is_open(crate::app::PanelId::Problems) {
                    let s = self.renderer.as_ref().unwrap().scale_factor;
                    let wh = self.window.as_ref().unwrap().inner_size().height as f32;

                    let is_top = self.ide_panel.slots.iter().any(|sl| {
                        sl.id == crate::app::PanelId::Problems
                            && sl.group == crate::app::PanelGroup::Top
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

                    let (cx, cy, cw, ch) = if is_top {
                        let panel_left_w = self.ide_panel.left_width * s;
                        let title_h = 32.0 * s;
                        (
                            sb_w,
                            title_h,
                            panel_left_w,
                            wh - title_h - effective_bottom_h,
                        )
                    } else {
                        let ww = self.window.as_ref().unwrap().inner_size().width as f32;
                        let tab_h = 32.0 * s;
                        let panel_y = crate::render_view::ide_bottom_panel_y(wh, panel_bottom_h, s);
                        (
                            sb_w,
                            panel_y + 1.0 + tab_h,
                            ww - sb_w,
                            panel_bottom_h - 1.0 - tab_h,
                        )
                    };

                    if mx >= cx && mx <= cx + cw && my >= cy && my <= cy + ch {
                        let scroll_x = cx + cw - 12.0 * s;
                        if mx >= scroll_x {
                            let item_h = 24.0 * s;
                            let total_h = self.ide_panel.flat_diags.len() as f32 * item_h;
                            let track_h = ch - 40.0 * s;
                            if total_h > track_h {
                                let max_scroll = total_h - track_h;
                                let scroll_ratio = (self.ide_panel.problems_scroll.current
                                    / max_scroll)
                                    .clamp(0.0, 1.0);
                                let thumb_h = (track_h / total_h * track_h).max(20.0 * s);
                                let list_y = cy + 40.0 * s;
                                let thumb_y = list_y + scroll_ratio * (track_h - thumb_h);

                                if my >= thumb_y && my <= thumb_y + thumb_h {
                                    self.ide_panel.problems_scroll.is_dragging = true;
                                    self.ide_panel.problems_scroll.drag_offset = my - thumb_y;
                                    return;
                                } else if my >= list_y && my <= list_y + track_h {
                                    self.ide_panel.problems_scroll.anim_speed = 15.0;
                                    self.ide_panel.problems_scroll.drag_offset = thumb_h / 2.0;
                                    let new_ratio = (my - list_y - thumb_h / 2.0)
                                        / (track_h - thumb_h).max(1.0);
                                    self.ide_panel.problems_scroll.target =
                                        (new_ratio * max_scroll).clamp(0.0, max_scroll);
                                    self.ide_panel.problems_scroll.is_dragging = true;
                                    self.window.as_ref().unwrap().request_redraw();
                                    return;
                                }
                            }
                        }
                    }
                }

                if self.is_ide_mode {
                    let s = self.renderer.as_ref().unwrap().scale_factor;
                    let sb_w = 48.0 * s;
                    let panel_left_w = self.ide_panel.left_width * s;
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
                        if (mx - resize_x).abs() < 6.0 * s && my >= 0.0 && my < resize_bottom_limit
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
                                    let (_, by, _, box_h) = rect;
                                    let track_h = box_h - 16.0 * s;
                                    let thumb_h =
                                        (box_h / (box_h + max_scroll) * track_h).max(20.0 * s);
                                    let thumb_y = by
                                        + 8.0 * s
                                        + (popup.scroll.current / max_scroll) * (track_h - thumb_h);
                                    if my >= thumb_y && my <= thumb_y + thumb_h {
                                        popup.scroll.is_dragging = true;
                                        popup.scroll.drag_offset = my - thumb_y;
                                    } else {
                                        popup.scroll.anim_speed = 15.0;
                                        popup.scroll.drag_offset = thumb_h / 2.0;
                                        let ratio = (my - by - 8.0 * s - popup.scroll.drag_offset)
                                            / (track_h - thumb_h).max(0.0001);
                                        popup.scroll.target =
                                            (ratio * max_scroll).clamp(0.0, max_scroll);
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
                    } else if clicked_id == crate::ui_system::UiId::GitGraphScroll {
                        let s = self.renderer.as_ref().unwrap().scale_factor;
                        let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                        if let Some((rows_y, rows_h)) =
                            super::git_graph_rows_bounds(&self.ide_panel, wh, s)
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
                            self.ide_panel.git.graph_scroll.drag_offset = drag_offset;
                            self.ide_panel.git.graph_scroll.target = target;
                            self.ide_panel.git.graph_scroll.velocity = 0.0;
                            self.ide_panel.git.graph_scroll.is_dragging = true;
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
                    } else {
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

        if self.show_settings {
            if state == ElementState::Released {
                self.is_dragging_settings_ignore = false;
                self.is_dragging_lsp_log = false;
            } else if state == ElementState::Pressed {
                let s = self.renderer.as_ref().unwrap().scale_factor;
                let w = (1000.0 * s)
                    .min(self.window.as_ref().unwrap().inner_size().width as f32 - 40.0 * s);
                let h = (700.0 * s)
                    .min(self.window.as_ref().unwrap().inner_size().height as f32 - 40.0 * s);
                let x = (self.window.as_ref().unwrap().inner_size().width as f32 - w) / 2.0;
                let y = (self.window.as_ref().unwrap().inner_size().height as f32 - h) / 2.0;

                let mx = self.renderer.as_ref().unwrap().last_mouse_x;
                let my = self.renderer.as_ref().unwrap().last_mouse_y;

                if mx < x || mx > x + w || my < y || my > y + h {
                    self.show_settings = false;
                } else {
                    // Ищем только среди оверлейных элементов настроек,
                    // чтобы фоновые элементы редактора не реагировали на клики.
                    if let Some(clicked_id) = self.ui_registry.find_overlay_at(mx, my) {
                        match clicked_id {
                            crate::ui_system::UiId::SettingsIdeIgnoreInput => {
                                // Специальная обработка: позиционирование курсора по клику
                                self.settings_ignore_focused = true;
                                self.is_dragging_settings_ignore = true;
                                let s = self.renderer.as_ref().unwrap().scale_factor;
                                let pad_h = 40.0 * s;
                                let sidebar_w = 200.0 * s;
                                let ix = x + pad_h;
                                let content_x = ix + sidebar_w + 30.0 * s;
                                let text = self.settings_ignore_editor.get_full_text();
                                let start_x = content_x + 8.0 * s;
                                let x_offset =
                                    (mx - start_x + self.settings_ignore_scroll_x).max(0.0);
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
                    if drag.threshold_passed && self.tabs.len() > 1 {
                        let s = self.renderer.as_ref().unwrap().scale_factor;
                        let tab_pad = 16.0 * s;
                        let icon_size_tab = 20.0 * s;

                        let start_cx = if self.is_ide_mode {
                            let panel_left_w = self.ide_panel.left_width * s;
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
                        for (i, _tab) in self.tabs.iter().enumerate() {
                            let title = &display_titles[i];
                            let title_w =
                                self.renderer.as_mut().unwrap().measure_ui_width(title, 1.0);
                            let tab_w =
                                tab_pad * 2.0 + icon_size_tab + 8.0 * s + title_w + 30.0 * s;
                            widths.push(tab_w);
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
            self.is_dragging = false;
            self.ide_panel.is_dragging_terminal = false;
            self.scroll_y.is_dragging = false;
            self.is_dragging_search = false;
            self.ide_panel.file_tree_dialog_input_drag = None;
            self.is_dragging_settings_ignore = false;
            self.is_dragging_lsp_log = false;
            self.autocomplete_scroll.is_dragging = false;
            if let Some(popup) = &mut self.autocomplete_detail_popup {
                popup.scroll.is_dragging = false;
            }
            self.scroll_x.is_dragging = false;
            for tab in &mut self.tabs {
                if let crate::app::EditorTabKind::ApiClient(_, state) = &mut tab.kind {
                    state.body_scroll_x.is_dragging = false;
                    state.response_scroll_x.is_dragging = false;
                }
            }
            for term in &mut self.ide_panel.terminals {
                term.scroll_y.is_dragging = false;
            }
            self.ide_panel.lsp_scroll_x.is_dragging = false;
            self.ide_panel.lsp_scroll_y.is_dragging = false;
            self.ide_panel.problems_scroll.is_dragging = false;
            self.ide_panel.git.graph_scroll.is_dragging = false;
            for scroll in self.ide_panel.lsp_logs_scroll_y.values_mut() {
                scroll.is_dragging = false;
            }
            for scroll in self.ide_panel.lsp_logs_scroll_x.values_mut() {
                scroll.is_dragging = false;
            }
            crate::app::mouse::HOVER_STATE.with(|s| {
                let mut state = s.borrow_mut();
                if let Some(popup) = &mut state.popup {
                    popup.scroll.is_dragging = false;
                }
                state.selecting = false;
                state.diag_selecting = false;
            });
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
