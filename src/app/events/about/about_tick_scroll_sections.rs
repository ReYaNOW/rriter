// Per-frame `about_to_wait` sections, included into `events/about.rs`.
// Each section runs in the same order as before and reports whether it needs a redraw.

/// Scroll render benchmark driver; `None` means the event loop was asked to exit.
#[cfg_attr(coverage_nightly, coverage(off))]
fn about_to_wait_scroll_bench(
    app: &mut App,
    event_loop: &host_loop::HostLoop,
    now: Instant,
) -> Option<bool> {
    let mut needs_redraw = false;
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
            return None;
        }
        needs_redraw = true;
        if scrolling_phase {
            app.last_action = now;
        }
    }
    Some(needs_redraw)
}

/// Sticky header animation and autocomplete popup scrolls.
#[cfg_attr(coverage_nightly, coverage(off))]
fn about_to_wait_popup_scrolls(app: &mut App, dt: f32) -> bool {
    let mut needs_redraw = false;
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
    needs_redraw
}

/// Hover request/hide timers and hover popup scroll.
/// Returns `(needs_redraw, hover_wake_at, hover_poll_pending)`.
#[cfg_attr(coverage_nightly, coverage(off))]
fn about_to_wait_hover_timer(
    app: &mut App,
    raw_dt: f32,
    dt: f32,
    now: Instant,
) -> (bool, Option<Instant>, bool) {
    let mut needs_redraw = false;
    let mut hover_wake_at: Option<Instant> = None;
    let mut hover_poll_pending = false;
    let mut api_mock_hover_request_due = false;
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
    (needs_redraw, hover_wake_at, hover_poll_pending)
}

/// Settings page scroll animations.
#[cfg_attr(coverage_nightly, coverage(off))]
fn about_to_wait_settings_scrolls(app: &mut App, dt: f32) -> bool {
    let mut needs_redraw = false;
    if app.show_settings && app.settings_tab == 0 && app.settings_ide_scroll.update(dt) {
        app.window.as_ref().unwrap().request_redraw();
    }
    if app.show_settings && app.settings_tab == 4 && app.settings_scroll.update(dt) {
        needs_redraw = true;
    }
    if app.show_settings
        && app.settings_tab == 1
        && app.settings_general_scroll.update(dt)
    {
        needs_redraw = true;
    }
    if app.show_settings
        && app.settings_tab == 5
        && app.settings_database_scroll.update(dt)
    {
        needs_redraw = true;
    }
    needs_redraw
}

/// Shared vertical/horizontal editor scroll and Markdown Reader scroll.
#[cfg_attr(coverage_nightly, coverage(off))]
fn about_to_wait_main_scroll(app: &mut App, dt: f32) -> bool {
    let mut needs_redraw = false;
    let markdown_read = app.markdown_mode() == crate::app::MarkdownMode::Read;
    let mut shared_scroll_updated = false;
    if app.scroll_y.update(dt) {
        shared_scroll_updated = true;
        if markdown_read {
            app.markdown.on_shared_vertical_scroll_changed();
        }
        needs_redraw = true;
    }
    if markdown_read && update_markdown_read_selection_autoscroll(app, dt, shared_scroll_updated) {
        needs_redraw = true;
    }
    if markdown_read && app.markdown.update_code_scroll_x(dt) {
        needs_redraw = true;
    }

    if !markdown_read && app.scroll_x.update(dt) {
        needs_redraw = true;
    }
    needs_redraw
}

/// Editor and terminal tab strip scroll, including tab-drag autoscroll.
#[cfg_attr(coverage_nightly, coverage(off))]
fn about_to_wait_tab_strip_scroll(app: &mut App, dt: f32) -> bool {
    let mut needs_redraw = false;
    if tab_drag_animation_active(&app.ide_panel) {
        needs_redraw = true;
    }

    if app.ide_panel.tab_drag.is_some() {
        if let Some(r) = app.renderer.as_ref() {
            let s = r.scale_factor;
            let tab_x = (48.0 * s + app.ide_panel.visible_left_width(s)).round() + 1.0;
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
    if let Some(r) = app.renderer.as_ref() {
        let max = r.max_terminal_tab_scroll_x;
        app.ide_panel.terminal_tab_scroll.clamp_target(0.0, max);
        app.ide_panel.terminal_tab_scroll.clamp_current(0.0, max);
    }
    if app.ide_panel.terminal_tab_scroll.update(dt) {
        needs_redraw = true;
    }
    needs_redraw
}

/// Background job polls and timed notices.
/// Returns `(needs_redraw, api_label_wake_at)`.
#[cfg_attr(coverage_nightly, coverage(off))]
fn about_to_wait_background_polls(app: &mut App, now: Instant) -> (bool, Option<Instant>) {
    let mut needs_redraw = false;
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
    if app.clear_stale_active_database_query_diagnostic() {
        needs_redraw = true;
    }
    app.poll_database_runtime();
    if app.ide_panel.database.pending_job.is_some()
        || app.ide_panel.database.ddl_hover.borrow().is_some()
    {
        needs_redraw = true;
    }
    let (api_labels_changed, api_label_expiry) =
        app.ide_panel.api.tick_timed_labels(crate::app::api_client::now_epoch_secs());
    needs_redraw |= api_labels_changed;
    let api_label_wake_at = api_label_expiry.map(|at| epoch_secs_wake_at(now, at));
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
    for tab in &mut app.tabs {
        let crate::app::EditorTabKind::DatabaseTable(_, state) = &mut tab.kind else {
            continue;
        };
        if let Some(until) = state.notice_until {
            if now < until {
                needs_redraw = true;
            } else {
                state.clear_notice();
                needs_redraw = true;
            }
        }
    }
    (needs_redraw, api_label_wake_at)
}

/// File watcher notifications, external changes and Markdown read model refresh.
#[cfg_attr(coverage_nightly, coverage(off))]
fn about_to_wait_file_watcher(app: &mut App) -> bool {
    let mut needs_redraw = false;
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
            app.ide_panel.file_tree_error = Some(file_watcher_disconnect_message().to_string());
            app.start_file_watcher();
            needs_redraw = true;
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
    if app.refresh_markdown_read_model_if_stale() {
        needs_redraw = true;
    }
    needs_redraw
}

/// Side panel scroll animations (explorer, search, git, LSP, API panel).
#[cfg_attr(coverage_nightly, coverage(off))]
fn about_to_wait_panel_scrolls(app: &mut App, dt: f32) -> bool {
    let mut needs_redraw = false;
    if app.ide_panel.explorer_scroll.update(dt) {
        needs_redraw = true;
    }
    if app.ide_panel.database.dialog.is_some() {
        app.clamp_database_dialog_scroll_to_layout();
        if let Some(dialog) = app.ide_panel.database.dialog.as_mut()
            && dialog.scroll.update(dt)
        {
            needs_redraw = true;
        }
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
    if app.ide_panel.git.logs_open()
        && app.ide_panel.is_open(crate::app::PanelId::Git)
        && let Some(max_scroll) = app
            .renderer
            .as_ref()
            .and_then(|renderer| renderer.git_logs_layout_metrics())
            .map(|metrics| metrics.max_scroll)
        && app.ide_panel.git.update_git_logs_scroll(dt, max_scroll)
    {
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
    needs_redraw
}

/// Per-tab API client / database scrolls, database modals and LSP log scrolls.
#[cfg_attr(coverage_nightly, coverage(off))]
fn about_to_wait_tab_content_scrolls(app: &mut App, dt: f32) -> bool {
    let mut needs_redraw = false;
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
            if state.output_scroll.update(dt) {
                needs_redraw = true;
            }
            if state.output_scroll_x.update(dt) {
                needs_redraw = true;
            }
            if state.mock_static_response_scroll.update(dt) {
                needs_redraw = true;
            }
            if state.mock_static_response_scroll_x.update(dt) {
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
    for tab in &mut app.tabs {
        match &mut tab.kind {
            crate::app::EditorTabKind::DatabaseTable(_, state) => {
                if state.grid.scroll_x.update(dt) {
                    needs_redraw = true;
                }
                if state.grid.scroll_y.update(dt) {
                    needs_redraw = true;
                }
                if state.grid.refreshing {
                    needs_redraw = true;
                }
            }
            crate::app::EditorTabKind::DatabaseQuery(_, state) => {
                if state.result_view.scroll_x.update(dt) {
                    needs_redraw = true;
                }
                if state.result_view.scroll_y.update(dt) {
                    needs_redraw = true;
                }
                if state.result_view.review_message_scroll_y.update(dt) {
                    needs_redraw = true;
                }
            }
            _ => {}
        }
    }
    if let Some(modal) = app.ide_panel.database.table_modal.as_mut() {
        match modal {
            crate::app::database::DatabaseTableModal::SqlPreview {
                scroll_x, scroll_y, ..
            } => {
                if scroll_x.update(dt) {
                    needs_redraw = true;
                }
                if scroll_y.update(dt) {
                    needs_redraw = true;
                }
            }
            crate::app::database::DatabaseTableModal::MultilineEditor {
                scroll_x,
                scroll_y,
                ..
            } => {
                if scroll_x.update(dt) {
                    needs_redraw = true;
                }
                if scroll_y.update(dt) {
                    needs_redraw = true;
                }
            }
            crate::app::database::DatabaseTableModal::Review { scroll, .. } => {
                if scroll.update(dt) {
                    needs_redraw = true;
                }
            }
            _ => {}
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
    needs_redraw
}
