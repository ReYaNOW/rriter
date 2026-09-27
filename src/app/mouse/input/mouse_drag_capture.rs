// Pointer capture end: markdown / git-logs / popup selections and scrollbar
// drags finished on release, IDE panel DnD, tab reorder and resize finalization.
use super::*;

impl App {
    /// Ends markdown read selection, markdown scrollbar drags and git-logs selection on left release. Returns true when the event is consumed.
    #[cfg_attr(coverage_nightly, coverage(off))]
    #[inline]
    pub(super) fn finish_text_captures_on_release(
        &mut self,
        state: ElementState,
        button: winit::event::MouseButton,
        mx: f32,
        my: f32,
    ) -> bool {
        let left_released =
            state == ElementState::Released && button == winit::event::MouseButton::Left;
        let mut finished_markdown_pointer = false;
        if left_released && self.markdown.read_selecting {
            let _ = self.update_markdown_read_selection_at(mx, my);
            finished_markdown_pointer |= self.finish_markdown_read_selection_gesture();
        }
        let finished_read_scrollbar_drag = left_released
            && self.markdown_mode() == crate::app::MarkdownMode::Read
            && self.scroll_y.is_dragging;
        if finished_read_scrollbar_drag {
            self.scroll_y.end_drag();
            finished_markdown_pointer = true;
        }
        // Захваченный drag code-скроллбара завершается до UI release dispatch,
        // даже если курсор уже вне thumb (или режим сменился во время drag).
        let finished_code_scrollbar_drag = left_released && self.markdown.end_code_scroll_drag();
        finished_markdown_pointer |= finished_code_scrollbar_drag;
        if finished_markdown_pointer && let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
        if finished_read_scrollbar_drag || finished_code_scrollbar_drag {
            return true;
        }
        let finished_git_logs_pointer = left_released
            && (self.ide_panel.git.logs_scroll.is_dragging
                || self
                    .renderer
                    .as_ref()
                    .is_some_and(|renderer| renderer.git_logs_selecting));
        if finished_git_logs_pointer {
            self.ide_panel.git.logs_scroll.end_drag();
            if let Some(renderer) = self.renderer.as_mut() {
                renderer.git_logs_selecting = false;
            }
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            return true;
        }
        false
    }

    /// Ends database table drag and popup text selections on release. Returns true when the event is consumed.
    #[cfg_attr(coverage_nightly, coverage(off))]
    #[inline]
    pub(super) fn finish_popup_captures_on_release(
        &mut self,
        state: ElementState,
    ) -> bool {
        if state == ElementState::Released {
            self.finish_database_table_drag();
        }
        if state == ElementState::Released && self.autocomplete_detail_selecting {
            self.cancel_pointer_interactions();
            self.window.as_ref().unwrap().request_redraw();
            return true;
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
                return true;
            }
        }
        false
    }

    /// Finalizes file-tree DnD, editor/terminal tab reorder, sidebar slot DnD and panel resize on release. Returns true when the event is consumed.
    #[cfg_attr(coverage_nightly, coverage(off))]
    #[inline]
    pub(super) fn finish_panel_drags_on_release(
        &mut self,
        state: ElementState,
        button: winit::event::MouseButton,
    ) -> bool {
        if state == ElementState::Released {
            if button == winit::event::MouseButton::Left && self.ide_panel.is_dragging_terminal {
                if let Some(term) = self
                    .ide_panel
                    .terminals
                    .get(self.ide_panel.active_terminal)
                {
                    let mut grid = crate::app::terminal::lock_terminal_grid(&term.grid);
                    if !grid.mouse_tracking
                        && grid
                            .selection
                            .is_some_and(|(sx, sy, ex, ey)| sx == ex && sy == ey)
                    {
                        grid.selection = None;
                    }
                }
            }
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

                        if let Some(placement) =
                            crate::app::tab_drag_placement(start_cx, &widths, Some(&drag))
                        {
                            let new_idx = placement.destination;
                            if new_idx != drag.start_idx {
                                self.sync_active_tab();
                                let tab = self.tabs.remove(drag.start_idx);
                                self.tabs.insert(new_idx, tab);
                                self.active_tab = crate::app::active_index_after_move(
                                    self.active_tab,
                                    drag.start_idx,
                                    new_idx,
                                );
                                self.sync_active_tab();
                                self.save_tabs_state();
                            }
                        }
                    }
                }
                if let Some(drag) = self.ide_panel.terminal_tab_drag.take() {
                    if drag.threshold_passed
                        && self.ide_panel.terminals.len() > 1
                        && drag.start_idx < self.ide_panel.terminals.len()
                    {
                        let s = self.renderer.as_ref().unwrap().scale_factor;
                        let (panel_x, _, panel_w, _, _) =
                            super::app_panel_scroll_rect(self, crate::app::PanelId::Terminal, s);
                        let mut title = String::new();
                        let mut widths = Vec::with_capacity(self.ide_panel.terminals.len());
                        for terminal in &self.ide_panel.terminals {
                            terminal.write_display_title(&mut title);
                            let title_w = self
                                .renderer
                                .as_mut()
                                .unwrap()
                                .measure_ui_width(&title, 1.0);
                            widths.push(
                                crate::render_view::terminal_ui::terminal_tab_width_from_title_width(
                                    title_w,
                                    s,
                                ),
                            );
                        }
                        let add_size =
                            crate::render_view::terminal_ui::terminal_tab_add_size(panel_w, s);
                        let max_scroll =
                            crate::render_view::terminal_ui::terminal_tab_strip_max_scroll(
                                panel_w,
                                widths.iter().sum(),
                                add_size,
                                s,
                            );
                        let start_cx = crate::render_view::terminal_ui::terminal_tab_base_x(
                            panel_x,
                            self.ide_panel.terminal_tab_scroll.current,
                            max_scroll,
                            s,
                        );
                        if let Some(placement) =
                            crate::app::tab_drag_placement(start_cx, &widths, Some(&drag))
                        {
                            let new_idx = placement.destination;
                            if new_idx != drag.start_idx {
                                let active = crate::app::active_index_after_move(
                                    self.ide_panel.active_terminal,
                                    drag.start_idx,
                                    new_idx,
                                );
                                let terminal = self.ide_panel.terminals.remove(drag.start_idx);
                                self.ide_panel.terminals.insert(new_idx, terminal);
                                self.ide_panel.active_terminal = active;
                                self.reveal_active_terminal_tab_now();
                            }
                        }
                    }
                }
                if let Some(drag) = self.ide_panel.drag.take() {
                    if !drag.threshold_passed {
                        // Клик без движения → переключить панель
                        self.toggle_sidebar_panel_from_click(drag.panel_id);
                        // Edit bounds are not valid for the shared Read coordinate system,
                        // including a pending Read -> Edit rebase before the first Edit frame.
                        if self.markdown.shared_vertical_scroll_uses_editor_bounds() {
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
                        }
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
            return true;
        }
        false
    }
}
