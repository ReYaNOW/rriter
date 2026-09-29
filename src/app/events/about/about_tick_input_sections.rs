// Per-frame `about_to_wait` sections, included into `events/about.rs`.
// Each section runs in the same order as before and reports whether it needs a redraw.

/// Terminal presentation intents, closed terminal cleanup and active terminal scroll.
#[cfg_attr(coverage_nightly, coverage(off))]
fn about_to_wait_terminals(app: &mut App, dt: f32) -> bool {
    let mut needs_redraw = false;
    if app.is_ide_mode && app.process_terminal_presentation_intents() {
        needs_redraw = true;
    }

    if app.is_ide_mode && app.ide_panel.is_open(crate::app::PanelId::Terminal) {
        let mut closed_terminals = Vec::new();
        for (i, term) in app.ide_panel.terminals.iter_mut().enumerate() {
            if term.is_closed() {
                closed_terminals.push(i);
            }
        }

        for idx in closed_terminals.into_iter().rev() {
            let active = app.ide_panel.active_terminal;
            app.ide_panel.terminals.remove(idx);
            needs_redraw = true;
            if app.ide_panel.terminals.is_empty() {
                app.add_terminal();
            } else {
                app.ide_panel.active_terminal = crate::app::active_index_after_remove(
                    active,
                    idx,
                    app.ide_panel.terminals.len(),
                );
            }
        }
        app.defer_terminal_panel_until_ready();

        if app.ide_panel.terminals.is_empty() {
            app.add_terminal();
        }
        let active = app.ide_panel.active_terminal;
        if let Some(t) = app.ide_panel.terminals.get_mut(active) {
            if t.scroll_y.update(dt) {
                needs_redraw = true;
            }
            if crate::app::terminal::lock_terminal_grid(&t.grid).dirty {
                needs_redraw = true;
            }
        }
    }
    needs_redraw
}

/// Autocomplete, context menu, hover, settings and search slide animations.
#[cfg_attr(coverage_nightly, coverage(off))]
fn about_to_wait_overlay_animations(app: &mut App, dt: f32, now: Instant) -> bool {
    let mut needs_redraw = false;
    if app.autocomplete_active && app.autocomplete_anim_progress < 1.0 {
        app.autocomplete_anim_progress += (1.0 - app.autocomplete_anim_progress) * 10.0 * dt;
        if app.autocomplete_anim_progress > 0.997 {
            app.autocomplete_anim_progress = 1.0;
        }
        needs_redraw = true;
    }

    let context_menu_opened_at = active_context_menu_opened_at(&app.ide_panel);
    if context_menu_opened_at.is_some_and(|opened_at| {
        crate::app::context_menu::context_menu_anim_progress(opened_at, now) < 1.0
    }) {
        needs_redraw = true;
    }

    {
        let s = &mut app.hover;
        if s.diag_rect.is_some() && s.diag_anim_progress < 1.0 {
            s.diag_anim_progress =
                crate::app::mouse::advance_hover_anim_progress(s.diag_anim_progress, dt);
            needs_redraw = true;
        }
        if let Some(ref mut p) = s.popup {
            if p.anim_progress < 1.0 {
                p.anim_progress =
                    crate::app::mouse::advance_hover_anim_progress(p.anim_progress, dt);
                needs_redraw = true;
            }
        }
    }

    let s = app.renderer.as_ref().map(|r| r.scale_factor).unwrap_or(1.0);
    let window_height = app.window.as_ref().unwrap().inner_size().height as f32;
    let h = (700.0_f32 * s).min(window_height - 40.0 * s);
    let start_y = window_height + 100.0 * s;
    let open_y = (window_height - h) / 2.0;
    let target_y = if app.show_settings { open_y } else { start_y };

    let diff = target_y - app.settings_y;
    if diff.abs() > 1.5 {
        app.settings_y += diff * 10.0 * dt;
        let total_distance = (start_y - open_y).max(1.0);
        app.settings_anim_progress = ((start_y - app.settings_y) / total_distance).clamp(0.0, 1.0);
        needs_redraw = true;
    } else if !app.show_settings && app.settings_anim_progress > 0.0 {
        app.settings_y = start_y;
        app.settings_anim_progress = 0.0;
        needs_redraw = true;
    }

    let s = app.renderer.as_ref().map(|r| r.scale_factor).unwrap_or(1.0);
    let tab_bar_h = app.editor_top_inset(s);
    let target_search_y = if app.show_search {
        tab_bar_h + 10.0 * s
    } else {
        -120.0 * s
    };
    let search_diff = target_search_y - app.search_anim_y;
    if search_diff.abs() > 1.5 {
        let speed = if app.show_search { 20.0 } else { 7.0 };
        app.search_anim_y += search_diff * speed * dt;
        needs_redraw = true;
    }
    needs_redraw
}

/// Selection drag autoscroll for the terminal and the editor.
#[cfg_attr(coverage_nightly, coverage(off))]
fn about_to_wait_selection_drag_autoscroll(app: &mut App, dt: f32) -> bool {
    let mut needs_redraw = false;
    if app.ide_panel.is_dragging_terminal && app.is_dragging && !app.show_settings {
        if app.window.is_some() {
            let terminal_body = app
                .ui_registry
                .rect_for(crate::ui_system::UiId::TerminalBody);
            if let (Some(r), Some((term_x, term_y, _, term_h))) =
                (app.renderer.as_mut(), terminal_body)
            {
                let s = r.scale_factor;
                let mx = r.last_mouse_x;
                let my = r.last_mouse_y;
                let panel_x = term_x + 10.0 * s;
                let char_w =
                    r.char_advance('A') * crate::render_view::terminal_ui::TERMINAL_TEXT_SCALE;
                let char_h = r.line_height * crate::render_view::terminal_ui::TERMINAL_TEXT_SCALE;
                let drag_delta = selection_drag_autoscroll_delta(my, term_y, term_y + term_h);

                if drag_delta != 0.0 {
                    let active = app.ide_panel.active_terminal;
                    if let Some(term) = app.ide_panel.terminals.get_mut(active) {
                        let mut grid = crate::app::terminal::lock_terminal_grid(&term.grid);
                        if !grid.is_alt {
                            let total_lines = grid.scrollback.len() + grid.lines.len();
                            let max_scroll = crate::render_view::terminal_ui::terminal_max_scroll(
                                total_lines,
                                char_h,
                                term_h,
                                s,
                            );
                            if max_scroll > 0.0 && total_lines > 0 {
                                let speed = drag_autoscroll_speed(drag_delta, drag_delta < 0.0);
                                term.scroll_y.target = (term.scroll_y.target
                                    - drag_delta.signum() * speed * dt)
                                    .clamp(0.0, max_scroll);
                                term.scroll_y.anim_speed = 15.0;

                                let (cell_x, cell_y) = terminal_drag_cell(
                                    mx,
                                    my,
                                    panel_x,
                                    term_y,
                                    term_h,
                                    term.scroll_y.target.min(max_scroll).round(),
                                    char_w,
                                    char_h,
                                    s,
                                    grid.cols,
                                    total_lines,
                                );
                                if let Some((sx, sy, _, _)) = grid.selection {
                                    grid.selection = Some((sx, sy, cell_x, cell_y));
                                } else {
                                    grid.selection = Some((cell_x, cell_y, cell_x, cell_y));
                                }
                                needs_redraw = true;
                            }
                        }
                    }
                }
            }
        }
    }

    if app.is_dragging && !app.ide_panel.is_dragging_terminal && !app.scroll_y.is_dragging {
        if let Some((editor_x, editor_y, editor_w, editor_h)) = app
            .ui_registry
            .rect_for(crate::ui_system::UiId::EditorTextBody)
        {
            let my = app.renderer.as_ref().unwrap().last_mouse_y;
            let mx = app.renderer.as_ref().unwrap().last_mouse_x;
            let drag_scroll_delta_y =
                selection_drag_autoscroll_delta(my, editor_y, editor_y + editor_h);
            let drag_scroll_delta_x =
                selection_drag_autoscroll_delta(mx, editor_x, editor_x + editor_w);

            if drag_scroll_delta_y != 0.0 || drag_scroll_delta_x != 0.0 {
                if drag_scroll_delta_y != 0.0 {
                    let speed =
                        drag_autoscroll_speed(drag_scroll_delta_y, drag_scroll_delta_y < 0.0);
                    app.scroll_y.target += drag_scroll_delta_y.signum() * speed * dt;
                }

                if drag_scroll_delta_x != 0.0 {
                    let speed = drag_autoscroll_speed(drag_scroll_delta_x, false);
                    app.scroll_x.target += drag_scroll_delta_x.signum() * speed * dt;
                }

                let tab_bar_h = crate::render_view::editor_content_top_inset(
                    app.show_welcome,
                    app.is_ide_mode,
                    app.active_tab_is_database_query(),
                    app.renderer.as_ref().unwrap().scale_factor,
                );
                app.editor.set_cursor_at_pos(
                    mx,
                    my - tab_bar_h + app.scroll_y.target,
                    app.renderer.as_mut().unwrap(),
                    false,
                );
                needs_redraw = true;
            }
        }
    }
    needs_redraw
}

/// Clamps the editor scroll to the current layout.
#[cfg_attr(coverage_nightly, coverage(off))]
fn about_to_wait_clamp_editor_scroll(app: &mut App) {
    if let Some(w) = app.window.as_ref() {
        let s = app.renderer.as_ref().map(|r| r.scale_factor).unwrap_or(1.0);
        let tab_bar_h = crate::render_view::editor_content_top_inset(
            app.show_welcome,
            app.is_ide_mode,
            app.active_tab_is_database_query(),
            s,
        );
        let window_h = w.inner_size().height as f32;
        let full_bottom_panel_h = if app.is_ide_mode && app.ide_panel.any_bottom_open() {
            app.ide_panel.bottom_height * s
        } else {
            0.0
        };
        let query_results_h = app
            .tabs
            .get(app.active_tab)
            .map_or(0.0, |tab| match &tab.kind {
                crate::app::EditorTabKind::DatabaseQuery(_, state)
                    if crate::app::database::database_query_results_visible(state) =>
                {
                    crate::app::database::database_query_results_height(
                        state.result_view.preferred_height,
                        window_h,
                        full_bottom_panel_h,
                        s,
                    )
                }
                _ => 0.0,
            });
        let editor_bottom_h = if app.is_ide_mode {
            app.ide_panel.editor_reserved_bottom_height(s) + query_results_h
        } else {
            0.0
        };
        let visible_h = crate::render_view::editor_view_height(
            window_h,
            tab_bar_h,
            editor_bottom_h,
            app.is_ide_mode,
            s,
        );
        if app.markdown.shared_vertical_scroll_uses_editor_bounds() {
            let max_scroll_y = app
                .renderer
                .as_mut()
                .unwrap()
                .get_max_scroll(&app.editor, visible_h);
            app.scroll_y.clamp_target(0.0, max_scroll_y);
            app.scroll_y.clamp_current(0.0, max_scroll_y);
        }

        let max_scroll_x = app.renderer.as_ref().unwrap().max_scroll_x;
        app.scroll_x.clamp_target(0.0, max_scroll_x);
        app.scroll_x.clamp_current(0.0, max_scroll_x);
    }
}

/// Native picker receivers, tool installer polls and resize settle.
#[cfg_attr(coverage_nightly, coverage(off))]
fn about_to_wait_picker_receivers(app: &mut App, dt: f32, now: Instant) -> bool {
    let mut needs_redraw = false;
    if let Some(rx) = app.open_folder_rx.take() {
        match rx.try_recv() {
            Ok(result) => {
                if let Some(path) = result {
                    app.apply_selected_workspace_folder(path);
                    needs_redraw = true;
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                app.open_folder_rx = Some(rx);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                app.ide_panel.file_tree_error =
                    Some("Диалог выбора папки неожиданно завершился".to_string());
                needs_redraw = true;
            }
        }
    }

    if let Some(rx) = &app.settings_tool_picker_rx {
        match rx.try_recv() {
            Ok((kind, path)) => {
                app.settings_tool_picker_rx = None;
                if path.is_some() && !app.tool_installer.is_running() {
                    app.apply_tool_path_selection(kind, path);
                    needs_redraw = true;
                }
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                app.settings_tool_picker_rx = None;
                app.tool_installer
                    .report_external_error("Диалог выбора инструмента неожиданно завершился");
                needs_redraw = true;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
    }

    if app.poll_tool_installer() {
        needs_redraw = true;
    }
    if app.poll_pdf_worker() { needs_redraw = true; }
    if app.poll_dart_tool_state() {
        needs_redraw = true;
    }
    if app.tool_installer.is_log_open() {
        let max_scroll = app.tool_install_log_max_scroll();
        if app.tool_installer.update_log_scroll(dt, max_scroll) {
            needs_redraw = true;
        }
    }

    if let Some(rx) = app.open_file_rx.take() {
        match rx.try_recv() {
            Ok(result) => {
                if let Some(path) = result {
                    app.open_file_in_tab(path, true);
                    needs_redraw = true;
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => app.open_file_rx = Some(rx),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                app.ide_panel.file_tree_error =
                    Some("Диалог выбора файла неожиданно завершился".to_string());
                needs_redraw = true;
            }
        }
    }

    if let Some(rx) = app.save_file_rx.take() {
        match rx.try_recv() {
            Ok(result) => {
                app.handle_save_as_selection(result);
                needs_redraw = true;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => app.save_file_rx = Some(rx),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                // No picker will answer: the confirmation flow waiting for it ends here.
                app.confirm_dialog.abort_save_as();
                app.ide_panel.file_tree_error =
                    Some("Диалог сохранения неожиданно завершился".to_string());
                needs_redraw = true;
            }
        }
    }

    if let Some(last_resize) = app.last_resize_time {
        if now.duration_since(last_resize).as_millis() > 150 {
            app.last_resize_time = None;
            needs_redraw = true;
        } else {
            needs_redraw = true;
        }
    }
    needs_redraw
}

/// Syntax highlight results and visible priority highlight requests.
#[cfg_attr(coverage_nightly, coverage(off))]
fn about_to_wait_highlight(app: &mut App) -> bool {
    let mut needs_redraw = false;
    if app.highlighter.poll(app.editor.version) {
        app.apply_highlight_results();
        if app.autocomplete_active {
            app.update_autocomplete();
        }
        needs_redraw = true;
    }
    if app.request_visible_priority_highlight() {
        needs_redraw = true;
    }
    needs_redraw
}
