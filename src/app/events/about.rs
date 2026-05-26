use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AboutWaitPlan {
    Wait,
    WaitUntil(Instant),
}

const DRAG_AUTOSCROLL_EDGE_PX: f32 = 58.0;
const DRAG_AUTOSCROLL_BOTTOM_GAP_PX: f32 = 72.0;
const DRAG_AUTOSCROLL_MIN_SPEED: f32 = 360.0;
const DRAG_AUTOSCROLL_MAX_SPEED: f32 = 7200.0;
const DRAG_AUTOSCROLL_ACCEL: f32 = 0.40;
const DRAG_AUTOSCROLL_TOP_BOOST: f32 = 1.22;
const PYTHON_INLAY_HINT_IDLE_DELAY: std::time::Duration = std::time::Duration::from_millis(180);

fn request_python_inlay_hints_if_needed(app: &mut App) {
    if !app.is_ide_mode || !matches!(app.file_extension.as_str(), "py" | "pyi") {
        app.python_inlay_hints.clear();
        app.python_inlay_hint_path = None;
        app.python_inlay_hint_pending_request_id = None;
        app.python_inlay_hint_pending_path = None;
        return;
    }
    let Some(path) = app.file_path.clone() else {
        app.python_inlay_hints.clear();
        app.python_inlay_hint_path = None;
        return;
    };
    if let Some((version, hints)) = app.python_inlay_hint_cache.get(&path)
        && *version == app.editor.version
    {
        if app.python_inlay_hint_path.as_ref() != Some(&path)
            || app.python_inlay_hint_version != app.editor.version
        {
            app.python_inlay_hints.clear();
            app.python_inlay_hints.extend_from_slice(hints);
            app.python_inlay_hint_path = Some(path);
            app.python_inlay_hint_version = *version;
        }
        return;
    }
    if app.python_inlay_hint_pending_request_id.is_some()
        || app.python_inlay_hint_path.as_ref() == Some(&path)
            && app.python_inlay_hint_version == app.editor.version
        || app.last_action.elapsed() < PYTHON_INLAY_HINT_IDLE_DELAY
    {
        return;
    }

    let Some(lsp) = app.lsp.as_mut() else {
        return;
    };
    let text = app.editor.get_full_text();
    let (end_line, end_col) =
        crate::lsp::offset_to_lsp_pos(&text, text.len(), &app.editor.line_offsets);
    if let Some(id) =
        lsp.request_ty_inlay_hints(&path, &app.file_extension, 0, 0, end_line, end_col)
    {
        app.python_inlay_hint_pending_request_id = Some(id);
        app.python_inlay_hint_pending_path = Some(path);
        app.python_inlay_hint_pending_version = app.editor.version;
    }
}

fn update_sticky_animation(
    current: &mut Vec<(usize, usize)>,
    target: &[(usize, usize)],
    progress: &mut f32,
    is_adding: &mut bool,
    dt: f32,
) -> bool {
    let mut needs_redraw = false;
    if current.as_slice() != target {
        let old_len = current.len();
        let new_len = target.len();

        if new_len > old_len {
            *progress = 0.0;
            *is_adding = true;
            current.clear();
            current.extend_from_slice(target);
        } else if new_len < old_len {
            if *is_adding || *progress >= 1.0 {
                *progress = 0.0;
                *is_adding = false;
            }
        } else {
            *progress = 1.0;
            current.clear();
            current.extend_from_slice(target);
        }
        needs_redraw = true;
    }

    if *progress < 1.0 {
        *progress += dt * 6.0;
        if *progress >= 0.99 {
            *progress = 1.0;
            if !*is_adding {
                current.clear();
                current.extend_from_slice(target);
            }
        }
        needs_redraw = true;
    }

    needs_redraw
}

fn drag_autoscroll_delta(pos: f32, start: f32, end: f32, edge: f32) -> f32 {
    if pos < start {
        pos - start
    } else if pos < start + edge {
        pos - start - edge
    } else if pos > end {
        pos - end
    } else if pos > end - edge {
        pos - end + edge
    } else {
        0.0
    }
}

fn drag_autoscroll_speed(delta: f32, is_top_edge: bool) -> f32 {
    let amount = delta.abs();
    let speed = (amount * amount * DRAG_AUTOSCROLL_ACCEL)
        .clamp(DRAG_AUTOSCROLL_MIN_SPEED, DRAG_AUTOSCROLL_MAX_SPEED);
    if is_top_edge {
        (speed * DRAG_AUTOSCROLL_TOP_BOOST).min(DRAG_AUTOSCROLL_MAX_SPEED)
    } else {
        speed
    }
}

fn drag_autoscroll_editor_bottom(window_height: f32, editor_top: f32, scale: f32) -> f32 {
    let bottom_gap = DRAG_AUTOSCROLL_BOTTOM_GAP_PX * scale;
    (window_height - bottom_gap).max(editor_top + 24.0 * scale)
}

fn terminal_content_bounds(window_height: f32, bottom_height: f32, scale: f32) -> (f32, f32) {
    let bottom_h = bottom_height * scale;
    let tab_h = 32.0 * scale;
    let content_y =
        crate::render_view::ide_bottom_panel_y(window_height, bottom_h, scale) + 1.0 + tab_h;
    let content_h = bottom_h - 1.0 - tab_h;
    crate::render_view::terminal_ui::terminal_body_rect(content_y, content_h, scale)
}

fn terminal_drag_cell(
    mx: f32,
    my: f32,
    panel_x: f32,
    term_y: f32,
    term_h: f32,
    scroll_offset: f32,
    char_w: f32,
    char_h: f32,
    scale: f32,
    cols: usize,
    total_lines: usize,
) -> (usize, usize) {
    let offset_from_bottom =
        (term_y + term_h - 8.0 * scale - my + scroll_offset) / char_h.max(0.0001);
    let cell_y = total_lines
        .saturating_sub(1)
        .saturating_sub(offset_from_bottom.max(0.0).floor() as usize)
        .min(total_lines.saturating_sub(1));
    let cell_x = ((mx - panel_x) / char_w.max(0.0001)).floor().max(0.0) as usize;
    (cell_x.min(cols.saturating_sub(1)), cell_y)
}

fn earliest_wake(base: Instant, a: Option<Instant>, b: Option<Instant>) -> Instant {
    let mut wake_at = base;
    if let Some(t) = a {
        if t < wake_at {
            wake_at = t;
        }
    }
    if let Some(t) = b {
        if t < wake_at {
            wake_at = t;
        }
    }
    wake_at
}

fn compute_about_wait_plan(
    now: Instant,
    last_action: Instant,
    needs_redraw: bool,
    show_welcome: bool,
    is_ide_mode: bool,
    is_highlighting: bool,
    idle_blink_enabled: bool,
    hover_wake_at: Option<Instant>,
    hover_poll_pending: bool,
    api_poll_pending: bool,
) -> AboutWaitPlan {
    if needs_redraw || (show_welcome && is_ide_mode) {
        return AboutWaitPlan::Wait;
    }

    let hover_poll_wake_at =
        hover_poll_pending.then_some(now + std::time::Duration::from_millis(16));
    let api_poll_wake_at = api_poll_pending.then_some(now + std::time::Duration::from_millis(16));

    if is_highlighting {
        return AboutWaitPlan::WaitUntil(earliest_wake(
            now + std::time::Duration::from_millis(5),
            hover_wake_at,
            earliest_optional_wake(hover_poll_wake_at, api_poll_wake_at),
        ));
    }

    if !idle_blink_enabled {
        return if let Some(wake_at) = earliest_optional_wake(
            hover_wake_at,
            earliest_optional_wake(hover_poll_wake_at, api_poll_wake_at),
        ) {
            AboutWaitPlan::WaitUntil(wake_at)
        } else {
            AboutWaitPlan::Wait
        };
    }

    let next_blink = last_action
        + std::time::Duration::from_millis(
            (now.duration_since(last_action).as_millis() / 500 + 1) as u64 * 500,
        );

    AboutWaitPlan::WaitUntil(earliest_wake(
        next_blink,
        hover_wake_at,
        earliest_optional_wake(hover_poll_wake_at, api_poll_wake_at),
    ))
}

fn earliest_optional_wake(a: Option<Instant>, b: Option<Instant>) -> Option<Instant> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

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

    crate::app::mouse::HOVER_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Some(popup) = &mut state.popup {
            if popup.scroll.update(dt) {
                needs_redraw = true;
            }
        }
        if let Some(byte_offset) = state.byte_offset {
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
                    if app.is_ide_mode {
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
        if let Some(rx) = &app.file_tree_notify_rx {
            while rx.try_recv().is_ok() {
                fs_changed = true;
            }
        }
        if fs_changed {
            app.refresh_file_tree();
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
        for (i, term) in app.ide_panel.terminals.iter().enumerate() {
            if let Ok(mut child) = term.child.lock() {
                if let Ok(Some(_)) = child.try_wait() {
                    closed_terminals.push(i);
                }
            }
        }

        for idx in closed_terminals.into_iter().rev() {
            app.ide_panel.terminals.remove(idx);
            needs_redraw = true;
            if app.ide_panel.terminals.is_empty() {
                app.ide_panel
                    .terminals
                    .push(crate::app::terminal::Terminal::spawn(app.window.clone()));
                app.ide_panel.active_terminal = 0;
            } else if app.ide_panel.active_terminal >= app.ide_panel.terminals.len() {
                app.ide_panel.active_terminal = app.ide_panel.terminals.len().saturating_sub(1);
            }
        }

        if app.ide_panel.terminals.is_empty() {
            app.ide_panel
                .terminals
                .push(crate::app::terminal::Terminal::spawn(app.window.clone()));
            app.ide_panel.active_terminal = 0;
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
                app.ide_workspaces.push(path.clone());
                app.ide_panel.file_tree_expanded.insert(path.clone());
                app.refresh_file_tree();
                app.start_file_watcher();
                let w = app.window.as_ref().unwrap();
                let maximized = w.is_maximized();
                let (width, height) = if maximized {
                    (app.window_width, app.window_height)
                } else {
                    let scale = w.scale_factor();
                    let size = w.inner_size().to_logical::<f64>(scale);
                    (size.width, size.height)
                };
                crate::save_config(&crate::Config {
                    window_width: width,
                    window_height: height,
                    maximized,
                    ide_workspaces: app.ide_workspaces.clone(),
                    ide_ignore_patterns: app.ide_ignore_patterns.clone(),
                    enable_telemetry: crate::render_view::TELEMETRY_ENABLED
                        .load(std::sync::atomic::Ordering::Relaxed),
                });
            }
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
                app.file_path = Some(path.clone());
                let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                app.base_title = file_name.into_owned();

                if let Some(e) = path.extension() {
                    app.file_extension = e.to_string_lossy().to_string();
                } else {
                    app.file_extension = String::new();
                }

                app.add_recent_file(path);

                if app.save_current_file() {
                    if let Some(w) = app.window.as_ref() {
                        App::update_window_title(w, &app.base_title, app.editor.is_dirty());
                    }
                    app.highlighter.reset(
                        app.editor.version,
                        app.editor.get_full_text(),
                        app.file_extension.clone(),
                        app.editor.cursor,
                    );
                }
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
                    app.remember_autocomplete_detail_cache(&items);
                    if app.autocomplete_active {
                        app.merge_autocomplete_details(items);
                    } else {
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
                        continue;
                    };
                    let version = app.python_inlay_hint_pending_version;
                    if app.file_path.as_ref() == Some(&path) && app.editor.version == version {
                        let text = app.editor.get_full_text();
                        let parsed =
                            crate::app::python_positional_inlay_hints_from_lsp(&text, &hints);
                        app.python_inlay_hint_cache
                            .insert(path.clone(), (version, parsed.clone()));
                        app.python_inlay_hints = parsed;
                        app.python_inlay_hint_path = Some(path);
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
                crate::app::mouse::HOVER_STATE.with(|state| {
                    let mut state = state.borrow_mut();
                    if state.request_id == Some(request_id) {
                        if crate::render_view::hover_trace_enabled() {
                            println!("[HOVER DEBUG] Received response for req id: {}. Has text: {}", request_id, text.is_some());
                        }
                        state.request_id = None;
                        let Some(t) = text else {
                            if crate::render_view::hover_trace_enabled() {
                                println!("[HOVER DEBUG] Text is empty. Clearing popup.");
                            }
                            state.popup = None;
                            state.pending_popup = None;
                            state.rect = None;
                            if let Some(w) = app.window.as_ref() {
                                w.request_redraw();
                            }
                            return;
                        };
                        if let Some(bo) = state.byte_offset {
                            let (clean_msg, spans, line_kinds, inline_code_ranges) =
                                crate::lsp::highlight_hover_text(&t);
                            let hovered_symbol = symbol_at_offset(&app.editor, bo);
                            if clean_msg.trim() == "None"
                                && hovered_symbol.as_deref() == Some("await")
                            {
                                state.popup = None;
                                state.pending_popup = None;
                                state.rect = None;
                                if let Some(w) = app.window.as_ref() {
                                    w.request_redraw();
                                }
                                return;
                            }
                            let is_simple_type = should_replace_simple_type_hover(&clean_msg);
                            let (clean_msg, spans, line_kinds, inline_code_ranges) =
                                if should_replace_hover_with_source_signature(&clean_msg) {
                                    let lsp_ty = if is_simple_type {
                                        Some(clean_msg.as_str())
                                    } else {
                                        None
                                    };
                                    let current_mod = app.file_path.as_ref().and_then(|p| {
                                        module_path_from_definition_path(p, &app.ide_workspaces)
                                    });
                                    if let Some(sig) = source_signature_for_hover(
                                        &app.editor,
                                        bo,
                                        !is_simple_type,
                                        lsp_ty,
                                        current_mod.as_deref(),
                                    ) {
                                        crate::lsp::highlight_hover_text(&sig)
                                    } else {
                                        (clean_msg, spans, line_kinds, inline_code_ranges)
                                    }
                                } else {
                                    (clean_msg, spans, line_kinds, inline_code_ranges)
                                };
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

                            let popup = crate::app::mouse::HoverPopup {
                                text: clean_msg,
                                spans,
                                line_kinds,
                                inline_code_ranges,
                                byte_offset: bo,
                                anchor_x,
                                anchor_y,
                                offset_x: None,
                                offset_y: None, anim_progress: 0.0,
                                scroll: crate::scroll::ScrollState::new(15.0),
                                layout_cache: None,
                            };
                            state.pending_popup = None;
                            state.definition_request_id = None;
                            state.hide_diagnostic_popup_until_ready();
                            if let Some(path) = app.file_path.clone() {
                                let (line, col) = crate::lsp::offset_to_lsp_pos(
                                    &app.editor.get_full_text(),
                                    bo,
                                    &app.editor.line_offsets,
                                );
                                state.definition_request_id = app.lsp.as_mut().and_then(|lsp| {
                                    lsp.request_definition(&path, &app.file_extension, line, col)
                                });
                            } else {
                                state.definition_request_id = None;
                            }
                            if state.definition_request_id.is_some() {
                                if crate::render_view::hover_trace_enabled() {
                                    println!("[HOVER DEBUG] Hover processed, waiting for definition req id: {:?}", state.definition_request_id);
                                }
                                state.pending_popup = Some(popup);
                            } else {
                                if crate::render_view::hover_trace_enabled() {
                                    println!("[HOVER DEBUG] Hover processed, showing popup instantly.");
                                }
                                state.finish_stale_combined_transition();
                                state.popup = Some(popup);
                            }
                            state.selection_anchor = None;
                            state.selection_cursor = None;
                            state.selecting = false;
                            if let Some(w) = app.window.as_ref() {
                                w.request_redraw();
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
            let raw_servers = lsp.servers_info();
            let filter = app.ide_panel.current_lsp_log_filter();
            let needs_update = app.ide_panel.lsp_log_filter_dirty
                || app.ide_panel.lsp_log_filter_applied.as_ref() != Some(&filter)
                || app.ide_panel.lsp_servers.len() != raw_servers.len()
                || raw_servers.iter().any(|info| {
                    app.ide_panel
                        .lsp_servers
                        .iter()
                        .find(|ui| ui.name == info.name)
                        .is_none_or(|ui| ui.status != info.status)
                        || app
                            .ide_panel
                            .lsp_log_source_counts
                            .get(info.name)
                            .copied()
                            .unwrap_or(0)
                            != info.logs.len()
                });

            if needs_update {
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

    let is_highlighting = !app.is_highlighted_once;
    let idle_blink_enabled = app.is_focused && app.dialog_window.is_none();
    let autocomplete_animating = app.autocomplete_active && app.autocomplete_anim_progress < 1.0;
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
        !app.api_request_rx.is_empty() || app.api_mock_ty_rx.is_some(),
    ) {
        AboutWaitPlan::Wait => {
            if let Some(w) = app.window.as_ref() {
                w.request_redraw();
            }
            if autocomplete_animating || git_progress_animating {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sticky_animation_add_remove_and_equal_length_are_pure_state_transitions() {
        let mut current = vec![];
        let target = vec![(1, 2), (3, 4)];
        let mut progress = 1.0;
        let mut adding = false;

        assert!(update_sticky_animation(
            &mut current,
            &target,
            &mut progress,
            &mut adding,
            0.01,
        ));
        assert_eq!(current, target);
        assert!(adding);
        assert!(progress > 0.0 && progress < 1.0);

        let target = vec![(1, 2)];
        progress = 1.0;
        assert!(update_sticky_animation(
            &mut current,
            &target,
            &mut progress,
            &mut adding,
            0.20,
        ));
        assert_eq!(current, target);
        assert!(!adding);
        assert_eq!(progress, 1.0);

        let target = vec![(9, 9)];
        assert!(update_sticky_animation(
            &mut current,
            &target,
            &mut progress,
            &mut adding,
            0.01,
        ));
        assert_eq!(current, target);
        assert_eq!(progress, 1.0);
    }

    #[test]
    fn drag_autoscroll_uses_inside_edge_band_and_outside_window_distance() {
        assert_eq!(drag_autoscroll_delta(50.0, 100.0, 500.0, 40.0), -50.0);
        assert_eq!(drag_autoscroll_delta(120.0, 100.0, 500.0, 40.0), -20.0);
        assert_eq!(drag_autoscroll_delta(480.0, 100.0, 500.0, 40.0), 20.0);
        assert_eq!(drag_autoscroll_delta(540.0, 100.0, 500.0, 40.0), 40.0);
        assert_eq!(drag_autoscroll_delta(250.0, 100.0, 500.0, 40.0), 0.0);
        assert!(drag_autoscroll_speed(30.0, false) >= DRAG_AUTOSCROLL_MIN_SPEED);
        assert!(drag_autoscroll_speed(-30.0, true) > drag_autoscroll_speed(30.0, false));
    }

    #[test]
    fn drag_autoscroll_bottom_edge_ignores_open_bottom_panel() {
        let scale = 1.0;
        let window_height = 900.0;
        let editor_top = 38.0;
        let bottom_panel_h = 260.0;
        let edge = DRAG_AUTOSCROLL_EDGE_PX * scale;

        let editor_bottom = drag_autoscroll_editor_bottom(window_height, editor_top, scale);
        let old_panel_sensitive_bottom =
            window_height - bottom_panel_h - DRAG_AUTOSCROLL_BOTTOM_GAP_PX * scale;
        let y_near_old_panel_edge = old_panel_sensitive_bottom + edge + 1.0;

        assert_eq!(
            editor_bottom,
            window_height - DRAG_AUTOSCROLL_BOTTOM_GAP_PX * scale
        );
        assert_eq!(
            drag_autoscroll_delta(y_near_old_panel_edge, editor_top, editor_bottom, edge),
            0.0
        );
    }

    #[test]
    fn about_wait_plan_prioritizes_redraw_highlight_hover_and_blink() {
        let now = Instant::now();
        let last_action = now - std::time::Duration::from_millis(1250);

        assert_eq!(
            compute_about_wait_plan(
                now,
                last_action,
                true,
                false,
                false,
                false,
                true,
                None,
                false,
                false,
            ),
            AboutWaitPlan::Wait,
        );
        assert_eq!(
            compute_about_wait_plan(
                now,
                last_action,
                false,
                true,
                true,
                false,
                true,
                None,
                false,
                false,
            ),
            AboutWaitPlan::Wait,
        );
        assert_eq!(
            compute_about_wait_plan(
                now,
                last_action,
                false,
                false,
                false,
                true,
                true,
                None,
                false,
                false,
            ),
            AboutWaitPlan::WaitUntil(now + std::time::Duration::from_millis(5)),
        );
        assert_eq!(
            compute_about_wait_plan(
                now,
                last_action,
                false,
                false,
                false,
                true,
                true,
                Some(now + std::time::Duration::from_millis(2)),
                true,
                false,
            ),
            AboutWaitPlan::WaitUntil(now + std::time::Duration::from_millis(2)),
        );
        assert_eq!(
            compute_about_wait_plan(
                now,
                last_action,
                false,
                false,
                false,
                false,
                true,
                None,
                false,
                false,
            ),
            AboutWaitPlan::WaitUntil(last_action + std::time::Duration::from_millis(1500)),
        );
        assert_eq!(
            compute_about_wait_plan(
                now,
                last_action,
                false,
                false,
                false,
                false,
                false,
                None,
                false,
                false,
            ),
            AboutWaitPlan::Wait,
        );
        assert_eq!(
            compute_about_wait_plan(
                now,
                last_action,
                false,
                false,
                false,
                false,
                false,
                Some(now + std::time::Duration::from_millis(20)),
                true,
                false,
            ),
            AboutWaitPlan::WaitUntil(now + std::time::Duration::from_millis(16)),
        );
    }
}
