use super::*;

include!("about/about_helpers.rs");

#[cfg_attr(coverage_nightly, coverage(off))]
pub(super) fn about_to_wait(app: &mut App, event_loop: &ActiveEventLoop) {
    if app.run_ide_on_startup {
        app.run_ide_on_startup = false;
        app.enter_ide_mode();
        return; // Пропускаем один кадр, чтобы избежать гонок состояний
    }

    let now = Instant::now();
    if app.render_suspended {
        app.last_frame = now;
        event_loop.set_control_flow(ControlFlow::Wait);
        return;
    }

    let raw_dt = (now - app.last_frame).as_secs_f32();
    let dt = raw_dt.min(0.016);
    app.last_frame = now;

    let mut needs_redraw = false;
    let mut hover_wake_at: Option<Instant> = None;
    let mut hover_poll_pending = false;
    let mut api_mock_hover_request_due = false;

    if app.scroll_render_bench.is_some() && app.is_ready {
        let max_scroll = app.renderer.as_ref().map_or(0.0, |renderer| {
            (app.editor.line_offsets.len() as f32 * renderer.line_height - renderer.height).max(0.0)
        });
        let mut finished = false;
        let scrolling_phase;
        {
            let bench = app.scroll_render_bench.as_mut().unwrap();
            let started_at = *bench.started_at.get_or_insert(now);
            let elapsed = now.duration_since(started_at).as_secs_f32();
            if !bench.announced {
                bench.announced = true;
                println!(
                    "SCROLL_BENCH_START duration={:.1}s lines={} bytes={} spans={}",
                    bench.duration_secs,
                    app.editor.line_offsets.len(),
                    app.editor.len(),
                    app.highlighter.spans.len(),
                );
            }
            let first_scroll_end = (bench.duration_secs - 2.0) * 0.5;
            let second_scroll_start = first_scroll_end + 2.0;
            scrolling_phase = elapsed < first_scroll_end || elapsed >= second_scroll_start;
            if elapsed >= bench.duration_secs {
                finished = true;
            } else if scrolling_phase {
                while bench.next_impulse_secs <= elapsed {
                    app.scroll_y.scroll_by(36.0 * bench.direction);
                    if app.scroll_y.target >= max_scroll {
                        app.scroll_y.target = max_scroll;
                        bench.direction = -1.0;
                    } else if app.scroll_y.target <= 0.0 {
                        app.scroll_y.target = 0.0;
                        bench.direction = 1.0;
                    }
                    bench.next_impulse_secs += 1.0 / 120.0;
                    bench.impulses += 1;
                }
            } else {
                bench.next_impulse_secs = elapsed + 1.0 / 120.0;
            }
        }
        if finished {
            let bench = app.scroll_render_bench.as_ref().unwrap();
            println!(
                "SCROLL_BENCH_DONE duration={:.1}s impulses={} spans={} highlight_complete={}",
                bench.duration_secs,
                bench.impulses,
                app.highlighter.spans.len(),
                app.highlighter.is_complete,
            );
            app.shutdown_background_services();
            event_loop.exit();
            return;
        }
        needs_redraw = true;
        if scrolling_phase {
            app.last_action = now;
        }
    }

    needs_redraw |= update_sticky_animation(
        &mut app.current_sticky_lines,
        &app.target_sticky_lines,
        &mut app.sticky_anim_progress,
        &mut app.sticky_anim_is_adding,
        dt,
    );

    if app.autocomplete_active && app.autocomplete_scroll.update(dt) {
        needs_redraw = true;
    }
    if app.autocomplete_active {
        if let Some(popup) = &mut app.autocomplete_detail_popup {
            popup
                .scroll
                .clamp_target(0.0, app.autocomplete_detail_max_scroll);
            popup
                .scroll
                .clamp_current(0.0, app.autocomplete_detail_max_scroll);
            if popup.scroll.update(dt) {
                needs_redraw = true;
            }
        }
    }

    let api_mock_hover_byte = if app.active_tab_is_api_client() {
        app.ide_panel
            .api
            .mock_hover_target
            .as_ref()
            .map(|target| target.edit_byte)
    } else {
        None
    };

    crate::app::mouse::HOVER_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Some(popup) = &mut state.popup {
            if popup.scroll.update(dt) {
                needs_redraw = true;
            }
        }
        if let Some(byte_offset) = state.byte_offset {
            let is_api_mock_hover = api_mock_hover_byte == Some(byte_offset);
            let popup_matches_byte = state
                .popup
                .as_ref()
                .is_some_and(|popup| popup.byte_offset == byte_offset);
            let pending_popup_matches_byte = state
                .pending_popup
                .as_ref()
                .is_some_and(|popup| popup.byte_offset == byte_offset);

            if !popup_matches_byte
                && !pending_popup_matches_byte
                && state.request_id.is_none()
                && state.definition_request_id.is_none()
            {
                state.timer += raw_dt;
                if state.timer >= crate::app::mouse::HOVER_REQUEST_DELAY_SEC {
                    state.timer = 0.0;
                    if is_api_mock_hover {
                        api_mock_hover_request_due = true;
                    } else if app.is_ide_mode {
                        let target = if app.active_tab_is_git_diff() {
                            app.active_git_diff_lsp_hover_target(byte_offset)
                        } else {
                            app.file_path.clone().map(|path| {
                                let (line, col) = crate::lsp::offset_to_lsp_pos(
                                    &app.editor.get_full_text(),
                                    byte_offset,
                                    &app.editor.line_offsets,
                                );
                                (path, line, col)
                            })
                        };
                        if let Some(lsp) = &mut app.lsp {
                            if let Some((path, line, col)) = target {
                                state.request_id =
                                    lsp.request_hover(&path, &app.file_extension, line, col);
                                if crate::render_view::hover_trace_enabled() {
                                    println!(
                                        "[HOVER DEBUG] 0.34s expired. Sent hover request. id: {:?}",
                                        state.request_id
                                    );
                                }
                                if state.request_id.is_some() {
                                    hover_poll_pending = true;
                                }
                            }
                        }
                    }
                } else {
                    hover_wake_at = Some(
                        now + std::time::Duration::from_secs_f32(
                            crate::app::mouse::HOVER_REQUEST_DELAY_SEC - state.timer,
                        ),
                    );
                }
            } else if state.request_id.is_some() || state.definition_request_id.is_some() {
                hover_poll_pending = true;
            }
        } else if state.popup.is_some() || state.pending_popup.is_some() {
            state.timer += raw_dt;
            if state.timer >= 0.25 {
                if crate::render_view::hover_trace_enabled() {
                    println!("[HOVER DEBUG] 0.25s hide timer expired. Clearing popup.");
                }
                state.popup = None;
                state.pending_popup = None;
                state.rect = None;
                state.clear_type_popup_transition_markers();
                needs_redraw = true;
            } else {
                hover_wake_at =
                    Some(now + std::time::Duration::from_secs_f32((0.25 - state.timer).max(0.0)));
            }
        }
    });
    if api_mock_hover_request_due && app.request_active_api_mock_hover() {
        hover_poll_pending = true;
    }

    if app.show_settings && app.settings_tab == 0 && app.settings_ide_scroll.update(dt) {
        app.window.as_ref().unwrap().request_redraw();
    }
    if app.show_settings && app.settings_tab == 4 && app.settings_scroll.update(dt) {
        needs_redraw = true;
    }

    if app.scroll_y.update(dt) {
        needs_redraw = true;
    }

    if app.scroll_x.update(dt) {
        needs_redraw = true;
    }

    if app.ide_panel.tab_drag.is_some() {
        needs_redraw = true;
    }

    if app.ide_panel.tab_drag.is_some() {
        if let Some(r) = app.renderer.as_ref() {
            let s = r.scale_factor;
            let tab_x = (48.0 * s + app.ide_panel.left_width * s).round() + 1.0;
            let tab_w = (r.width - tab_x).max(0.0);
            let mx = r.last_mouse_x;
            let edge = (DRAG_AUTOSCROLL_EDGE_PX * s).max(28.0);
            let drag_delta = drag_autoscroll_delta(mx, tab_x, tab_x + tab_w, edge);
            let max_scroll = r.max_tab_scroll_x;

            if drag_delta != 0.0 && max_scroll > 0.0 {
                let speed = drag_autoscroll_speed(drag_delta, false);
                let old_scroll = app.tab_scroll.current;
                let new_scroll =
                    (old_scroll + drag_delta.signum() * speed * dt).clamp(0.0, max_scroll);
                let scroll_delta = new_scroll - old_scroll;

                if scroll_delta != 0.0 {
                    app.tab_scroll.current = new_scroll;
                    app.tab_scroll.target = new_scroll;
                    if let Some(drag) = &mut app.ide_panel.tab_drag {
                        drag.start_x -= scroll_delta;
                    }
                    needs_redraw = true;
                }
            }
        }
    }

    if app.tab_scroll.update(dt) {
        needs_redraw = true;
    }

    if app.poll_file_tree() {
        needs_redraw = true;
    }
    if app.poll_project_search() {
        needs_redraw = true;
    }
    if app.poll_project_search_previews() {
        needs_redraw = true;
    }
    if app.queue_visible_project_search_previews() || app.project_search_has_pending_previews() {
        needs_redraw = true;
    }
    if app.poll_git_panel() {
        needs_redraw = true;
    }
    if app.poll_git_diff_tabs() {
        needs_redraw = true;
    }
    if app.poll_api_client() {
        needs_redraw = true;
    }
    let api_now = crate::app::api_client::now_epoch_secs();
    if let Some(at) = app.ide_panel.api.import_error_at {
        if api_now.saturating_sub(at) < 5 {
            needs_redraw = true;
        } else {
            app.ide_panel.api.import_error = None;
            app.ide_panel.api.import_error_at = None;
            needs_redraw = true;
        }
    }
    if app
        .ide_panel
        .api
        .specs
        .iter()
        .any(|spec| crate::app::api_client::api_timing_visible_at(spec.last_loaded, api_now))
    {
        needs_redraw = true;
    }
    if app.poll_inline_git_diff_popup() {
        needs_redraw = true;
    }
    if let Some(until) = app.readonly_notice_until {
        if now < until {
            needs_redraw = true;
        } else {
            app.readonly_notice_until = None;
            needs_redraw = true;
        }
    }

    // Watcher сигнализирует об изменениях на диске — обновляем дерево
    {
        let mut fs_changed = false;
        let mut watcher_disconnected = false;
        if let Some(rx) = &app.file_tree_notify_rx {
            loop {
                match rx.try_recv() {
                    Ok(()) => fs_changed = true,
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        watcher_disconnected = true;
                        break;
                    }
                }
            }
        }
        if watcher_disconnected {
            app.file_tree_notify_rx = None;
            app.file_tree_watcher_stop_tx = None;
            app.file_tree_watched_dirs.clear();
        }
        if fs_changed {
            app.refresh_file_tree();
            app.start_file_watcher();
            if app.ide_panel.is_open(crate::app::PanelId::Git) {
                app.refresh_git_panel();
            }
            app.start_external_changes_check();
            needs_redraw = true;
        }
    }
    if app.poll_external_changes() {
        needs_redraw = true;
    }
    if app.ide_panel.explorer_scroll.update(dt) {
        needs_redraw = true;
    }
    if let Some(layout) = app.project_search_panel_layout()
        && let Some(scale) = app.renderer.as_ref().map(|renderer| renderer.scale_factor)
    {
        app.ide_panel
            .project_search
            .clamp_query_scrolls(layout.query, scale);
    }
    if app.ide_panel.project_search.scroll.update(dt) {
        needs_redraw = true;
    }
    if app.ide_panel.project_search.query_scroll_y.update(dt) {
        needs_redraw = true;
    }
    if app.ide_panel.project_search.query_scroll_x.update(dt) {
        needs_redraw = true;
    }
    if app.queue_visible_project_search_previews() || app.project_search_has_pending_previews() {
        needs_redraw = true;
    }
    if app.ide_panel.git.scroll.update(dt) {
        needs_redraw = true;
    }
    if app.ide_panel.git.graph_scroll.update(dt) {
        needs_redraw = true;
    }
    if app.ide_panel.problems_scroll.update(dt) {
        needs_redraw = true;
    }
    if app.ide_panel.lsp_scroll_y.update(dt) {
        needs_redraw = true;
    }
    if app.ide_panel.lsp_scroll_x.update(dt) {
        needs_redraw = true;
    }
    if app.ide_panel.api.panel_scroll.update(dt) {
        needs_redraw = true;
    }
    if app.ide_panel.api.route_scroll.update(dt) {
        needs_redraw = true;
    }
    if app.ide_panel.api.input_scroll_x.update(dt) {
        needs_redraw = true;
    }
    if let Some(dialog) = app.ide_panel.file_tree_rename_dialog.as_mut()
        && dialog.input_scroll_x.update(dt)
    {
        needs_redraw = true;
    }
    if app.ide_panel.api.mock_python_versions_scroll.update(dt) {
        needs_redraw = true;
    }
    if app.ide_panel.api.mock_guide_scroll.update(dt) {
        needs_redraw = true;
    }
    if app.ide_panel.api.mock_python_install_log_scroll.update(dt) {
        needs_redraw = true;
    }
    if app.ide_panel.api.mock_server_log_scroll.update(dt) {
        needs_redraw = true;
    }
    for scroll in app.ide_panel.api.mock_python_scrolls.values_mut() {
        if scroll.update(dt) {
            needs_redraw = true;
        }
    }
    for scroll in app.ide_panel.api.mock_python_scrolls_x.values_mut() {
        if scroll.update(dt) {
            needs_redraw = true;
        }
    }
    for tab in &mut app.tabs {
        if let crate::app::EditorTabKind::ApiClient(_, state) = &mut tab.kind {
            if state.tab_scroll.update(dt) {
                needs_redraw = true;
            }
            if state.body_scroll.update(dt) {
                needs_redraw = true;
            }
            if state.body_scroll_x.update(dt) {
                needs_redraw = true;
            }
            if state.response_scroll.update(dt) {
                needs_redraw = true;
            }
            if state.response_scroll_x.update(dt) {
                needs_redraw = true;
            }
            let target_menu_anim = if state.output_schema_menu_open
                && state.output_doc_view == crate::app::api_client::ApiOutputDocView::Example
            {
                1.0
            } else {
                0.0
            };
            let menu_diff = target_menu_anim - state.output_schema_menu_anim;
            if menu_diff.abs() > 0.001 {
                state.output_schema_menu_anim += menu_diff * 10.0 * dt;
                if (target_menu_anim - state.output_schema_menu_anim).abs() <= 0.01 {
                    state.output_schema_menu_anim = target_menu_anim;
                }
                needs_redraw = true;
            }
            if state.output_schema_menu_scroll.update(dt) {
                needs_redraw = true;
            }
        }
    }
    for scroll in app.ide_panel.lsp_logs_scroll_y.values_mut() {
        if scroll.update(dt) {
            needs_redraw = true;
        }
    }
    for scroll in app.ide_panel.lsp_logs_scroll_x.values_mut() {
        if scroll.update(dt) {
            needs_redraw = true;
        }
    }

    if app.is_ide_mode && app.ide_panel.is_open(crate::app::PanelId::Terminal) {
        let mut closed_terminals = Vec::new();
        for (i, term) in app.ide_panel.terminals.iter_mut().enumerate() {
            if term.is_closed() {
                closed_terminals.push(i);
            }
        }

        for idx in closed_terminals.into_iter().rev() {
            app.ide_panel.terminals.remove(idx);
            needs_redraw = true;
            if app.ide_panel.terminals.is_empty() {
                app.add_terminal();
            } else if app.ide_panel.active_terminal >= app.ide_panel.terminals.len() {
                app.ide_panel.active_terminal = app.ide_panel.terminals.len().saturating_sub(1);
            }
        }

        if app.ide_panel.terminals.is_empty() {
            app.add_terminal();
            app.ide_panel.terminal_focused = true;
        }
        let active = app.ide_panel.active_terminal;
        if let Some(t) = app.ide_panel.terminals.get_mut(active) {
            if t.scroll_y.update(dt) {
                needs_redraw = true;
            }
            if t.grid.lock().unwrap().dirty {
                needs_redraw = true;
            }
        }
    }

    if app.autocomplete_active && app.autocomplete_anim_progress < 1.0 {
        app.autocomplete_anim_progress += (1.0 - app.autocomplete_anim_progress) * 10.0 * dt;
        if app.autocomplete_anim_progress > 0.997 {
            app.autocomplete_anim_progress = 1.0;
        }
        needs_redraw = true;
    }

    if let Some(menu) = &app.ide_panel.file_tree_context_menu {
        if crate::app::file_tree::file_tree_context_menu_anim_progress(menu.opened_at, now) < 1.0 {
            needs_redraw = true;
        }
    }

    crate::app::mouse::HOVER_STATE.with(|state| {
        let mut s = state.borrow_mut();
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
    });

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
    let tab_bar_h = if app.show_welcome || !app.is_ide_mode {
        0.0
    } else {
        38.0 * s
    };
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

    if app.ide_panel.is_dragging_terminal && app.is_dragging && !app.show_settings {
        if let Some(w) = app.window.as_ref() {
            if let Some(r) = app.renderer.as_mut() {
                let s = r.scale_factor;
                let wh = w.inner_size().height as f32;
                let mx = r.last_mouse_x;
                let my = r.last_mouse_y;
                let panel_x = 48.0 * s + 10.0 * s;
                let char_w = r.char_advance('A') * 1.05;
                let char_h = r.line_height * 1.05;
                let (term_y, term_h) = terminal_content_bounds(wh, app.ide_panel.bottom_height, s);
                let edge = (DRAG_AUTOSCROLL_EDGE_PX * s).max(28.0);
                let drag_delta = drag_autoscroll_delta(my, term_y, term_y + term_h, edge);

                if drag_delta != 0.0 {
                    let active = app.ide_panel.active_terminal;
                    if let Some(term) = app.ide_panel.terminals.get_mut(active) {
                        let mut grid = term.grid.lock().unwrap();
                        if !grid.is_alt {
                            let total_lines = grid.scrollback.len() + grid.lines.len();
                            let max_scroll = ((total_lines as f32 * char_h) - term_h).max(0.0);
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
        if let Some(w) = app.window.as_ref() {
            let wh = w.inner_size().height as f32;
            let ww = w.inner_size().width as f32;
            let my = app.renderer.as_ref().unwrap().last_mouse_y;
            let mx = app.renderer.as_ref().unwrap().last_mouse_x;
            let minimap_w = app.renderer.as_ref().unwrap().minimap_width;
            let padding = app.renderer.as_ref().unwrap().left_padding;
            let editor_top = tab_bar_h;
            let editor_bottom = drag_autoscroll_editor_bottom(wh, editor_top, s);
            let edge = (DRAG_AUTOSCROLL_EDGE_PX * s).max(28.0);

            let drag_scroll_delta_y = drag_autoscroll_delta(my, editor_top, editor_bottom, edge);

            let view_right_edge = ww - minimap_w;
            let drag_scroll_delta_x = drag_autoscroll_delta(mx, padding, view_right_edge, edge);

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

                let tab_bar_h = if app.show_welcome || !app.is_ide_mode {
                    0.0
                } else {
                    38.0 * app.renderer.as_ref().unwrap().scale_factor
                };
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

    if let Some(w) = app.window.as_ref() {
        let s = app.renderer.as_ref().map(|r| r.scale_factor).unwrap_or(1.0);
        let tab_bar_h = if app.show_welcome || !app.is_ide_mode {
            0.0
        } else {
            38.0 * s
        };
        let panel_bottom_h = if app.is_ide_mode && app.ide_panel.any_bottom_open() {
            app.ide_panel.bottom_height * s
        } else {
            0.0
        };
        let visible_h = crate::render_view::editor_view_height(
            w.inner_size().height as f32,
            tab_bar_h,
            panel_bottom_h,
            app.is_ide_mode,
            s,
        );
        let max_scroll_y = app
            .renderer
            .as_mut()
            .unwrap()
            .get_max_scroll(&app.editor, visible_h);
        app.scroll_y.clamp_target(0.0, max_scroll_y);
        app.scroll_y.clamp_current(0.0, max_scroll_y);

        let max_scroll_x = app.renderer.as_ref().unwrap().max_scroll_x;
        app.scroll_x.clamp_target(0.0, max_scroll_x);
        app.scroll_x.clamp_current(0.0, max_scroll_x);
    }

    if let Some(rx) = &app.open_folder_rx {
        if let Ok(result) = rx.try_recv() {
            app.open_folder_rx = None;
            if let Some(path) = result {
                app.apply_selected_workspace_folder(path);
                needs_redraw = true;
            }
        }
    }

    if let Some(rx) = &app.settings_tool_picker_rx {
        match rx.try_recv() {
            Ok((kind, path)) => {
                app.settings_tool_picker_rx = None;
                if path.is_some() {
                    app.apply_tool_path_selection(kind, path);
                    needs_redraw = true;
                }
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                app.settings_tool_picker_rx = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
    }

    if let Some(rx) = &app.open_file_rx {
        if let Ok(result) = rx.try_recv() {
            app.open_file_rx = None;
            if let Some(path) = result {
                app.open_file_in_tab(path, true);
            }
        }
    }

    if let Some(rx) = &app.save_file_rx {
        if let Ok(result) = rx.try_recv() {
            app.save_file_rx = None;
            if let Some(path) = result {
                app.apply_save_as_path(path);
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

    // LSP: опрашиваем события (диагностика, code actions) — раз в кадр, не блокирует
    let mut lsp_events = Vec::new();
    if app.is_ide_mode {
        if let Some(lsp) = &mut app.lsp {
            lsp_events = lsp.poll();
        }
    }

    for event in lsp_events {
        match event {
            crate::lsp::LspEvent::Diagnostics { .. } => {
                // lsp.diagnostics обновлены внутри poll() автоматически
                if let Some(w) = app.window.as_ref() {
                    w.request_redraw();
                }
            }
            crate::lsp::LspEvent::CodeActions {
                request_id,
                actions,
            } => {
                // Проверяем: это ответ на Alt+Enter меню?
                let is_for_menu = app
                    .lsp_actions_menu
                    .as_ref()
                    .and_then(|m| m.pending_request_id)
                    .map(|id| id == request_id)
                    .unwrap_or(false);

                if is_for_menu {
                    if let Some(menu) = &mut app.lsp_actions_menu {
                        let new_items: Vec<crate::app::LspActionItem> = actions
                            .into_iter()
                            .filter(|a| {
                                a.edit.is_some()
                                    && !a.title.to_lowercase().contains("fix all")
                                    && !a.title.to_lowercase().contains("organize imports")
                            })
                            .map(crate::app::LspActionItem::CodeAction)
                            .collect();
                        let mut combined = new_items;
                        combined.extend(menu.items.drain(..));
                        menu.items = combined;
                        menu.pending_request_id = None;
                    }
                    if let Some(w) = app.window.as_ref() {
                        w.request_redraw();
                    }
                } else if app.pending_fix_all_id == Some(request_id) {
                    // Fix All из панели LSP серверов
                    app.pending_fix_all_id = None;
                    let mut merged_edit = crate::lsp::WorkspaceEdit::default();
                    for action in actions {
                        if let Some(edit) = action.edit {
                            for (path, changes) in edit.changes {
                                merged_edit.changes.entry(path).or_default().extend(changes);
                            }
                        }
                    }
                    if !merged_edit.changes.is_empty() {
                        app.apply_workspace_edit(&merged_edit, true);
                    }
                    if let Some(w) = app.window.as_ref() {
                        w.request_redraw();
                    }
                }
            }
            crate::lsp::LspEvent::CompletionResponse { request_id, items } => {
                if app.autocomplete_pending_request_id == Some(request_id) {
                    app.autocomplete_pending_request_id = None;
                    app.remember_ty_autocomplete_cache(items.clone());
                    app.update_ty_autocomplete(items);
                    if let Some(w) = app.window.as_ref() {
                        w.request_redraw();
                    }
                } else if app.autocomplete_detail_request_id == Some(request_id) {
                    if app.autocomplete_active {
                        app.remember_autocomplete_detail_cache(&items);
                        app.merge_autocomplete_details(items);
                    } else {
                        app.remember_autocomplete_detail_cache(&items);
                        app.finish_autocomplete_detail_request();
                    }
                    if let Some(w) = app.window.as_ref() {
                        w.request_redraw();
                    }
                }
            }
            crate::lsp::LspEvent::SignatureHelpResponse {
                request_id,
                parameters,
            } => {
                if app.autocomplete_signature_request_id == Some(request_id) {
                    app.autocomplete_signature_request_id = None;
                    app.update_ty_signature_help_autocomplete(parameters);
                    if let Some(w) = app.window.as_ref() {
                        w.request_redraw();
                    }
                }
            }
            crate::lsp::LspEvent::InlayHintsResponse { request_id, hints } => {
                if app.python_inlay_hint_pending_request_id == Some(request_id) {
                    app.python_inlay_hint_pending_request_id = None;
                    let Some(path) = app.python_inlay_hint_pending_path.take() else {
                        app.python_inlay_hint_pending_range = None;
                        continue;
                    };
                    let Some(range) = app.python_inlay_hint_pending_range.take() else {
                        continue;
                    };
                    let version = app.python_inlay_hint_pending_version;
                    if app.file_path.as_ref() == Some(&path) && app.editor.version == version {
                        let text = app.editor.get_full_text();
                        let parsed =
                            crate::app::python_positional_inlay_hints_from_lsp_with_offsets(
                                &text,
                                &app.editor.line_offsets,
                                &hints,
                            );
                        app.python_inlay_hint_cache
                            .insert(path.clone(), (version, range, parsed.clone()));
                        app.python_inlay_hints = parsed;
                        app.python_inlay_hint_path = Some(path);
                        app.python_inlay_hint_range = Some(range);
                        app.python_inlay_hint_version = version;
                        if let Some(w) = app.window.as_ref() {
                            w.request_redraw();
                        }
                    }
                }
            }
            crate::lsp::LspEvent::ServerReady => {}
            crate::lsp::LspEvent::StatusChanged { .. } => {}
            crate::lsp::LspEvent::ConfigurationServed { .. } => {}
            crate::lsp::LspEvent::WorkspaceDiagnosticsDone { .. } => {}
            crate::lsp::LspEvent::Log { .. } => {} // Fix All ответ
            crate::lsp::LspEvent::HoverResponse { request_id, text } => {
                if let Some(ref t) = text {
                    if crate::render_view::hover_trace_enabled() {
                        println!("--- HOVER TEXT ---\n{}\n------------------", t);
                    }
                }
                if app.apply_api_mock_hover_response(request_id, text.clone()) {
                    continue;
                }
                crate::app::mouse::HOVER_STATE.with(|state| {
                    let mut state = state.borrow_mut();
                    if state.request_id == Some(request_id) {
                        if crate::render_view::hover_trace_enabled() {
                            println!(
                                "[HOVER DEBUG] Received response for req id: {}. Has text: {}",
                                request_id,
                                text.is_some()
                            );
                        }
                        if let Some(bo) = state.byte_offset {
                            let current_mod = app.file_path.as_ref().and_then(|p| {
                                module_path_from_definition_path(p, &app.ide_workspaces)
                            });
                            let tab_bar_h = if app.show_welcome || !app.is_ide_mode {
                                0.0
                            } else {
                                38.0 * app.renderer.as_ref().map(|r| r.scale_factor).unwrap_or(1.0)
                            };
                            let render_scroll_y = app.scroll_y.current.round() - tab_bar_h;
                            let (anchor_x, anchor_y) = if let Some(renderer) = app.renderer.as_mut()
                            {
                                crate::app::mouse::hover_anchor_for_byte(
                                    renderer,
                                    &app.editor,
                                    bo,
                                    render_scroll_y,
                                )
                            } else {
                                (0.0, 0.0)
                            };

                            let definition_request = || {
                                let path = app.file_path.clone()?;
                                let (line, col) = crate::lsp::offset_to_lsp_pos(
                                    &app.editor.get_full_text(),
                                    bo,
                                    &app.editor.line_offsets,
                                );
                                app.lsp.as_mut().and_then(|lsp| {
                                    lsp.request_definition(&path, &app.file_extension, line, col)
                                })
                            };
                            if apply_source_hover_response_to_state(
                                &mut state,
                                request_id,
                                &app.editor,
                                bo,
                                bo,
                                text,
                                current_mod.as_deref(),
                                (anchor_x, anchor_y),
                                definition_request,
                            ) {
                                if let Some(w) = app.window.as_ref() {
                                    w.request_redraw();
                                }
                            }
                        }
                    }
                });
            }
            crate::lsp::LspEvent::DefinitionResponse { request_id, target } => {
                let path = target.as_ref().map(|target| target.path.clone());
                if app.ctrl_definition.request_id == Some(request_id) {
                    app.ctrl_definition.request_id = None;
                    app.ctrl_definition.target =
                        app.ctrl_definition_target_from_lsp(target.map(|target| {
                            crate::app::DefinitionJumpTarget {
                                path: target.path,
                                line: target.line,
                                col: target.col,
                            }
                        }));
                    if let Some(w) = app.window.as_ref() {
                        w.request_redraw();
                    }
                    continue;
                }
                crate::app::mouse::HOVER_STATE.with(|state| {
                    let mut state = state.borrow_mut();
                    if state.definition_request_id == Some(request_id) {
                        state.definition_request_id = None;
                        let mut popup = state.pending_popup.take();
                        if popup.is_none() {
                            popup = state.popup.take();
                        }
                        if let Some(path) = path {
                            if let Some(module_path) =
                                module_path_from_definition_path(&path, &app.ide_workspaces)
                            {
                                if let Some(popup) = &mut popup {
                                    let mut text_changed_by_class = false;
                                    if let Some(symbol) =
                                        symbol_at_offset(&app.editor, popup.byte_offset)
                                    {
                                        if let Some(class_sig) =
                                            source_class_signature_from_definition_file(
                                                &path, &symbol,
                                            )
                                        {
                                            let mut new_text = popup.text.clone();
                                            let class_prefix = format!("class {}", symbol);

                                            if new_text.starts_with(&class_prefix) {
                                                new_text =
                                                    new_text.replacen(&class_prefix, &class_sig, 1);
                                            } else if new_text
                                                .contains(&format!("\n{class_prefix}"))
                                            {
                                                new_text =
                                                    new_text.replace(&class_prefix, &class_sig);
                                            } else if new_text == symbol {
                                                new_text = class_sig.clone();
                                            } else if new_text.starts_with(&format!("{symbol}\n")) {
                                                new_text =
                                                    new_text.replacen(&symbol, &class_sig, 1);
                                            }

                                            if new_text != popup.text {
                                                let (clean, spans, kinds, inline) =
                                                    crate::lsp::highlight_hover_text(&new_text);
                                                popup.text = clean;
                                                popup.spans = spans;
                                                popup.line_kinds = kinds;
                                                popup.inline_code_ranges = inline;
                                                text_changed_by_class = true;
                                            }
                                        }
                                    }

                                    if !text_changed_by_class
                                        && should_replace_simple_type_hover(&popup.text)
                                    {
                                        if let Some(symbol) =
                                            symbol_at_offset(&app.editor, popup.byte_offset)
                                        {
                                            if let Some(attr_hover) =
                                                source_attribute_hover_from_definition_file(
                                                    &path,
                                                    &symbol,
                                                    &module_path,
                                                    Some(&popup.text),
                                                )
                                            {
                                                let (clean, spans, kinds, inline) =
                                                    crate::lsp::highlight_hover_text(&attr_hover);
                                                popup.text = clean;
                                                popup.spans = spans;
                                                popup.line_kinds = kinds;
                                                popup.inline_code_ranges = inline;
                                            }
                                        }
                                    }

                                    if popup.text.starts_with("class ")
                                        && !popup.text.starts_with(&module_path)
                                        && !popup.text.starts_with(HOVER_MODULE_PREFIX)
                                    {
                                        prepend_hover_module_path(popup, &module_path);
                                    }
                                }
                            }
                        }
                        if let Some(popup) = popup {
                            state.finish_stale_combined_transition();
                            state.popup = Some(popup);
                            if let Some(w) = app.window.as_ref() {
                                w.request_redraw();
                            }
                        }
                    }
                });
            }
        }
    }

    request_python_inlay_hints_if_needed(app);

    if app.is_ide_mode {
        if let Some(lsp) = &mut app.lsp {
            // Умная синхронизация без аллокаций каждый кадр.
            // Обновляем UI только если статус или логи реально изменились.
            let filter = app.ide_panel.current_lsp_log_filter();
            let needs_update = {
                let summaries = lsp.server_summaries();
                app.ide_panel.lsp_log_filter_dirty
                    || app.ide_panel.lsp_log_filter_applied.as_ref() != Some(&filter)
                    || app.ide_panel.lsp_servers.len() != summaries.len()
                    || summaries.iter().any(|info| {
                        app.ide_panel
                            .lsp_servers
                            .iter()
                            .find(|ui| ui.name == info.name)
                            .is_none_or(|ui| &ui.status != info.status)
                            || app
                                .ide_panel
                                .lsp_log_source_counts
                                .get(info.name)
                                .copied()
                                .unwrap_or(0)
                                != info.log_count
                    })
            };

            if needs_update {
                let raw_servers = lsp.servers_info();
                app.ide_panel.lsp_log_source_counts.clear();
                let mut ui_servers = raw_servers;
                for info in &mut ui_servers {
                    app.ide_panel
                        .lsp_log_source_counts
                        .insert(info.name.to_string(), info.logs.len());
                    info.logs.retain(|log| filter.matches(log));
                }
                app.ide_panel.lsp_servers = ui_servers;
                app.ide_panel.lsp_log_filter_applied = Some(filter);
                app.ide_panel.lsp_log_filter_dirty = false;
                // Синхронизируем Editor для логов (для выделения и копирования)
                for info in &app.ide_panel.lsp_servers {
                    let new_text = info
                        .logs
                        .iter()
                        .map(|l| l.text.as_str())
                        .collect::<Vec<_>>()
                        .join("\n");
                    let focused = app.ide_panel.lsp_logs_focused.as_deref() == Some(info.name);
                    let entry = app
                        .ide_panel
                        .lsp_log_editors
                        .entry(info.name.to_string())
                        .or_insert_with(|| crate::editor::Editor::new(new_text.len().max(512)));
                    // Пересоздаём только если текст изменился (иначе сбросим выделение)
                    if entry.get_full_text() != new_text {
                        let saved_cursor = if focused { Some(entry.cursor) } else { None };
                        let saved_anchor = if focused {
                            entry.selection_anchor
                        } else {
                            None
                        };
                        *entry = crate::editor::Editor::new(new_text.len().max(512));
                        let _ = entry.insert_str(&new_text);

                        entry.foldable_ranges_bytes.clear();
                        let mut autofold_starts = Vec::new();
                        let mut byte_offset = 0;
                        for log in &info.logs {
                            for &(s, e, depth) in &log.folds {
                                let start = byte_offset + s;
                                entry
                                    .foldable_ranges_bytes
                                    .push((start, byte_offset + e, false));
                                if depth == 2 {
                                    autofold_starts.push(start);
                                }
                            }
                            byte_offset += log.text.len() + 1;
                        }
                        entry.rebuild_line_offsets();

                        for &(s, e, _) in &entry.foldable_ranges_bytes {
                            let sl = entry
                                .line_offsets
                                .partition_point(|&x| x <= s)
                                .saturating_sub(1);
                            let el = entry
                                .line_offsets
                                .partition_point(|&x| x <= e)
                                .saturating_sub(1);
                            if el > sl {
                                entry.foldable_lines.insert(sl, el);
                                if autofold_starts.contains(&s) {
                                    entry.folded_lines.insert(sl);
                                    entry.folded_start_bytes.insert(entry.line_offsets[sl]);
                                }
                            }
                        }

                        if let Some(c) = saved_cursor {
                            entry.cursor = c.min(new_text.len());
                            entry.selection_anchor = saved_anchor.map(|a| a.min(new_text.len()));
                        } else {
                            // По умолчанию курсор в конце (хвост лога)
                            entry.cursor = new_text.len();
                            entry.selection_anchor = None;
                        }
                    }
                }
                if let Some(w) = app.window.as_ref() {
                    w.request_redraw();
                }
            }
        }
    }

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

    let git_progress_animating =
        app.ide_panel.git.pending && app.ide_panel.git.pending_label.is_some();
    if git_progress_animating {
        needs_redraw = true;
    }

    if app.is_focused {
        let blink_state = (now.duration_since(app.last_action).as_millis() / 500) % 2 == 0;
        if blink_state != app.last_blink_state {
            app.last_blink_state = blink_state;
            needs_redraw = true;
        }
    }

    let is_highlighting =
        !app.is_highlighted_once || app.highlighter.has_pending_priority_highlight();
    let idle_blink_enabled = app.is_focused && app.dialog_window.is_none();
    let autocomplete_animating = app.autocomplete_active && app.autocomplete_anim_progress < 1.0;
    let scroll_animating = !app.scroll_y.is_settled() || !app.scroll_x.is_settled();
    match compute_about_wait_plan(
        now,
        app.last_action,
        needs_redraw,
        app.show_welcome,
        app.is_ide_mode,
        is_highlighting,
        idle_blink_enabled,
        hover_wake_at,
        hover_poll_pending,
        !app.api_request_rx.is_empty()
            || app.api_mock_ty_rx.is_some()
            || app.api_runtime_poll_pending(),
    ) {
        AboutWaitPlan::Wait => {
            if let Some(w) = app.window.as_ref() {
                w.request_redraw();
            }
            if needs_continuous_poll(
                autocomplete_animating,
                git_progress_animating,
                scroll_animating,
            ) {
                event_loop.set_control_flow(ControlFlow::Poll);
            } else {
                event_loop.set_control_flow(ControlFlow::Wait);
            }
        }
        AboutWaitPlan::WaitUntil(wake_at) => {
            event_loop.set_control_flow(ControlFlow::WaitUntil(wake_at));
        }
    }
}
