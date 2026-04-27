use super::*;

#[cfg_attr(coverage_nightly, coverage(off))]
impl App {
    pub fn handle_main_mouse_input(
        &mut self,
        _event_loop: &ActiveEventLoop,
        state: ElementState,
        button: winit::event::MouseButton,
    ) {
        let mx = self.renderer.as_ref().unwrap().last_mouse_x;
        let my = self.renderer.as_ref().unwrap().last_mouse_y;
        if state == ElementState::Pressed && button == winit::event::MouseButton::Left {
            let mut in_hover_popup = false;
            let type_rect = HOVER_STATE.with(|s| s.borrow().rect);
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
                let pad = 24.0 * self.renderer.as_ref().unwrap().scale_factor;
                if mx >= union_rect.0 - pad
                    && mx <= union_rect.0 + union_rect.2 + pad
                    && my >= union_rect.1 - pad
                    && my <= union_rect.1 + union_rect.3 + pad
                {
                    in_hover_popup = true;
                }
            }

            if !in_hover_popup {
                clear_hover_popup(self.renderer.as_mut());
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
                    let btn_code = match button {
                        winit::event::MouseButton::Left => 0,
                        winit::event::MouseButton::Middle => 1,
                        winit::event::MouseButton::Right => 2,
                        _ => 0,
                    };
                    let is_pressed = state == ElementState::Pressed;
                    let s = self.renderer.as_ref().unwrap().scale_factor;
                    let panel_x = 48.0 * s + 10.0 * s;
                    let char_w = self.renderer.as_mut().unwrap().char_advance('A') * 1.05;
                    let char_h = self.renderer.as_ref().unwrap().line_height * 1.05;
                    let bottom_h = self.ide_panel.bottom_height * s;
                    let tab_h = 32.0 * s;
                    let content_y = self.window.as_ref().unwrap().inner_size().height as f32
                        - bottom_h
                        + 1.0
                        + tab_h;
                    let content_h = bottom_h - 1.0 - tab_h;
                    let term_content_y = content_y + 32.0 * s;
                    let term_content_h = content_h - 32.0 * s;

                    let mut cell_x = ((mx - panel_x).max(0.0) / char_w).floor() as usize;
                    cell_x += 1;

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
                        let offset_from_bottom = (term_content_y + term_content_h - 8.0 * s - my
                            + scroll_offset)
                            / char_h;
                        let visible_row_0_based = grid
                            .visible_rows
                            .saturating_sub(1)
                            .saturating_sub(offset_from_bottom.max(0.0).floor() as usize);
                        cell_y = visible_row_0_based + 1;

                        if is_pressed {
                            grid.selection = None;
                        } else if let Some((sx, sy, ex, ey)) = grid.selection {
                            if sx != ex || sy != ey {
                                is_drag = true;
                            }
                        }
                    }

                    if !is_drag {
                        let end_char = if is_pressed { 'M' } else { 'm' };
                        let seq = format!("\x1b[<{};{};{}{}", btn_code, cell_x, cell_y, end_char);
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
                        (
                            sb_w,
                            wh - panel_bottom_h + 1.0 + tab_h,
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

                    let mut effective_bottom_h = panel_bottom_h;
                    if self.ide_panel.is_open(crate::app::PanelId::Terminal)
                        && !self.ide_panel.terminal_focused
                    {
                        effective_bottom_h = 0.0;
                    }

                    let mut manual_resize = false;
                    if panel_left_w > 0.0 {
                        let resize_x = sb_w + panel_left_w;
                        if (mx - resize_x).abs() < 6.0 * s
                            && my >= 0.0
                            && my < wh - effective_bottom_h
                        {
                            self.ide_panel.is_resizing_left = true;
                            manual_resize = true;
                        }
                    }
                    if panel_bottom_h > 0.0 && !manual_resize {
                        let resize_y = wh - panel_bottom_h;
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

                if let Some(clicked_id) = self.ui_registry.find_at(mx, my) {
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
                        crate::ui_system::UiId::ResizeLeft | crate::ui_system::UiId::ResizeBottom
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

                        let mut paths: Vec<Option<&std::path::PathBuf>> =
                            self.tabs.iter().map(|t| t.file_path.as_ref()).collect();
                        paths[self.active_tab] = self.file_path.as_ref();

                        let mut display_titles = vec![String::new(); self.tabs.len()];
                        for i in 0..self.tabs.len() {
                            if let Some(p1) = paths[i] {
                                let mut diff_level = 0;
                                let mut collision = false;
                                for j in 0..self.tabs.len() {
                                    if i == j {
                                        continue;
                                    }
                                    if let Some(p2) = paths[j] {
                                        if p1.file_name() == p2.file_name() {
                                            collision = true;
                                            let mut it1 = p1.components().rev();
                                            let mut it2 = p2.components().rev();
                                            let mut level = 0;
                                            loop {
                                                let c1 = it1.next();
                                                let c2 = it2.next();
                                                if c1 != c2 {
                                                    diff_level = diff_level.max(level);
                                                    break;
                                                }
                                                if c1.is_none() && c2.is_none() {
                                                    break;
                                                }
                                                level += 1;
                                            }
                                        }
                                    }
                                }
                                if collision && diff_level > 0 {
                                    let comps: Vec<_> = p1.components().rev().collect();
                                    if diff_level < comps.len() {
                                        let diff_dir =
                                            comps[diff_level].as_os_str().to_string_lossy();
                                        let file_name = comps[0].as_os_str().to_string_lossy();
                                        if diff_level == 1 {
                                            display_titles[i] =
                                                format!("{}/{}", diff_dir, file_name);
                                        } else {
                                            display_titles[i] =
                                                format!("{}/.../{}", diff_dir, file_name);
                                        }
                                    } else {
                                        display_titles[i] = p1.to_string_lossy().into_owned();
                                    }
                                } else {
                                    display_titles[i] = p1
                                        .file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .into_owned();
                                }
                            } else {
                                let bt = if i == self.active_tab {
                                    &self.base_title
                                } else {
                                    &self.tabs[i].base_title
                                };
                                display_titles[i] = if bt.is_empty() {
                                    "Безымянный".to_string()
                                } else {
                                    bt.to_string()
                                };
                            }
                        }

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
                        let max_scroll = self
                            .renderer
                            .as_mut()
                            .unwrap()
                            .get_max_scroll(&self.editor, wh);
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
                if self.ide_panel.is_resizing_left || self.ide_panel.is_resizing_bottom {
                    self.ide_panel.is_resizing_left = false;
                    self.ide_panel.is_resizing_bottom = false;
                    crate::save_panel_state(&self.ide_panel);
                }
            }
            self.is_dragging = false;
            self.ide_panel.is_dragging_terminal = false;
            self.scroll_y.is_dragging = false;
            self.is_dragging_search = false;
            self.is_dragging_settings_ignore = false;
            self.is_dragging_lsp_log = false;
            self.autocomplete_scroll.is_dragging = false;
            self.scroll_x.is_dragging = false;
            for term in &mut self.ide_panel.terminals {
                term.scroll_y.is_dragging = false;
            }
            self.ide_panel.lsp_scroll_x.is_dragging = false;
            self.ide_panel.lsp_scroll_y.is_dragging = false;
            self.ide_panel.problems_scroll.is_dragging = false;
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
            let s = self.renderer.as_ref().unwrap().scale_factor;

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
                        let scroll_x = rx + rw - 14.0 * s;
                        let step = 36.0 * s;
                        let total_items = self.autocomplete_options.len() as f32;
                        let visible_items = total_items.min(7.0);
                        let total_h = total_items * step + 16.0 * s;

                        if last_mouse_x >= scroll_x && total_h > rh {
                            self.autocomplete_scroll.is_dragging = true;
                            let max_scroll = ((total_items - visible_items) * step).max(0.0);
                            let scroll_ratio = (self.autocomplete_scroll.current
                                / max_scroll.max(1.0))
                            .clamp(0.0, 1.0);

                            let track_h = rh - 8.0 * s;
                            let thumb_h = (rh / total_h * track_h).max(20.0 * s);
                            let thumb_start_y = ry + 4.0 * s + scroll_ratio * (track_h - thumb_h);

                            if last_mouse_y >= thumb_start_y
                                && last_mouse_y <= thumb_start_y + thumb_h
                            {
                                self.autocomplete_scroll.drag_offset = last_mouse_y - thumb_start_y;
                            } else {
                                self.autocomplete_scroll.anim_speed = 15.0;
                                self.autocomplete_scroll.drag_offset = thumb_h / 2.0;
                                let new_ratio = (last_mouse_y
                                    - ry
                                    - 4.0 * s
                                    - self.autocomplete_scroll.drag_offset)
                                    / (track_h - thumb_h).max(1.0);
                                self.autocomplete_scroll.target =
                                    (new_ratio * max_scroll).clamp(0.0, max_scroll);
                            }
                        } else if let Some(idx) = self.autocomplete_hovered_idx {
                            self.autocomplete_selected_idx = idx;
                            self.apply_autocomplete();
                        }
                        return;
                    } else {
                        self.autocomplete_active = false;
                        self.autocomplete_selected_idx = 0;
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
