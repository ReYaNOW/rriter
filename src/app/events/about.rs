pub(crate) fn file_watcher_disconnect_message() -> &'static str {
    "Наблюдение за файлами неожиданно завершилось; выполняется перезапуск"
}

fn lsp_action_selection_after_prepend(
    old_len: usize,
    old_selected: usize,
    prepended_len: usize,
) -> usize {
    if old_len == 0 {
        0
    } else {
        old_selected.min(old_len - 1).saturating_add(prepended_len)
    }
}

use super::*;

include!("about/about_drag_animation_helpers.rs");
include!("about/about_helpers.rs");
include!("about/about_tick_scroll_sections.rs");
include!("about/about_tick_input_sections.rs");
include!("about/about_tick_lsp_sections.rs");

#[cfg(test)]
mod about_animation_tests;
#[cfg(test)]
mod about_markdown_reader_tests;
#[cfg(test)]
mod about_selection_drag_tests;

pub(crate) fn update_cursor_blink(app: &mut App, now: Instant, needs_redraw: &mut bool) {
    if app.is_focused && !app.headless_mode {
        let blink_state = (now.duration_since(app.last_action).as_millis() / 500) % 2 == 0;
        if blink_state != app.last_blink_state {
            app.last_blink_state = blink_state;
            *needs_redraw = true;
        }
    }
}

pub(crate) fn idle_blink_enabled(app: &App) -> bool {
    app.is_focused && !app.modal_dialog_open() && !app.headless_mode
}

fn update_markdown_read_selection_autoscroll(
    app: &mut App,
    dt: f32,
    shared_scroll_updated: bool,
) -> bool {
    if app.markdown_mode() != crate::app::MarkdownMode::Read || !app.markdown.read_selecting {
        return app.settle_markdown_read_selection_autoscroll();
    }
    if app.scroll_y.is_dragging {
        return app.finish_markdown_read_selection_gesture();
    }
    let Some((_, frame_y, _, frame_h)) = app
        .ui_registry
        .rect_for(crate::ui_system::UiId::MarkdownReadBody)
    else {
        return app.finish_markdown_read_selection_gesture();
    };
    let Some(renderer) = app.renderer.as_ref() else {
        return app.finish_markdown_read_selection_gesture();
    };
    let scale = renderer.scale_factor;
    let mouse_x = renderer.last_mouse_x;
    let mouse_y = renderer.last_mouse_y;
    let edge = markdown_read_selection_autoscroll_edge(frame_h, scale);
    if edge <= 0.0 {
        return app.settle_markdown_read_selection_autoscroll();
    }

    let was_autoscrolling = app.markdown.read_selection_autoscrolling;
    let mut changed = false;
    if shared_scroll_updated && was_autoscrolling {
        changed |= app.update_markdown_read_selection_at(mouse_x, mouse_y);
    }

    let drag_delta =
        markdown_read_selection_autoscroll_delta(mouse_y, frame_y, frame_y + frame_h, edge);
    if drag_delta == 0.0 {
        changed |= app.settle_markdown_read_selection_autoscroll();
        return changed;
    }

    // CursorLeft has no fresh Wayland coordinates. Its projected top/bottom position
    // still needs to update the Reader endpoint even before the first scroll tick moves.
    changed |= app.update_markdown_read_selection_at(mouse_x, mouse_y);

    let Some(max_scroll) = app.markdown.read_scroll_bounds() else {
        changed |= app.finish_markdown_read_selection_gesture();
        return changed;
    };
    if max_scroll <= 0.0 {
        changed |= app.settle_markdown_read_selection_autoscroll();
        return changed;
    }

    let speed = drag_autoscroll_speed(drag_delta, drag_delta < 0.0);
    let old_target = app.scroll_y.target;
    crate::app::markdown::scroll_markdown_read(
        &mut app.scroll_y,
        Some(max_scroll),
        drag_delta.signum() * speed * dt,
    );
    if app.scroll_y.target != old_target {
        app.markdown.read_selection_autoscrolling = true;
        changed = true;
    } else if app.markdown.read_selection_autoscrolling
        && app.scroll_y.target == app.scroll_y.current
        && app.scroll_y.velocity == 0.0
    {
        app.markdown.read_selection_autoscrolling = false;
    }
    changed
}

#[cfg_attr(coverage_nightly, coverage(off))]
pub(crate) fn about_to_wait(app: &mut App, event_loop: &host_loop::HostLoop) {
    if app.startup_deferred_pending && (app.is_ready || app.run_ide_on_startup) {
        app.startup_deferred_pending = false;
        app.refresh_dart_tool_state();
    }

    if app.run_ide_on_startup {
        app.run_ide_on_startup = false;
        app.enter_ide_mode();
        // This one-shot pass drains nothing: a wake it consumed must come back.
        app.ui_waker.redeliver_pending();
        return; // Пропускаем один кадр, чтобы избежать гонок состояний
    }

    // `close_tab_at` arms the question without the event loop; the window is created here.
    if let Some(action) = app.confirm_dialog.needs_window() {
        app.show_action_dialog(event_loop, action);
        app.ui_waker.redeliver_pending();
        return;
    }

    if let Some(action) = app.confirm_dialog.take_ready() {
        match action {
            PendingAction::None => {}
            PendingAction::Quit => {
                window_runtime::save_state_and_exit(app, event_loop);
                return;
            }
            PendingAction::OpenFile => app.trigger_file_picker(),
            PendingAction::CloseFile => app.close_current_file(),
            PendingAction::CloseTab(index) => app.close_tab_at_unchecked(index),
            PendingAction::CloseAllTabs => app.close_all_tabs_unchecked(),
        }
    }

    let now = Instant::now();
    let wall_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    let automation_running = if app.automation.is_some() {
        match app.advance_automation(event_loop, now) {
            Some(crate::app::automation::AutomationTick::Exit) => {
                app.shutdown_background_services();
                event_loop.exit();
                return;
            }
            Some(crate::app::automation::AutomationTick::Running) => true,
            None => false,
        }
    } else {
        false
    };
    if app.render_suspended && !automation_running {
        app.last_frame = now;
        // Reset before draining (see `ui_waker`). Other channels wait for resume: their
        // results need a frame anyway, and re-waking for them here would spin while hidden.
        app.ui_waker.begin_drain();
        app.poll_database_runtime();
        event_loop.set_awaiting_background(app.ide_panel.database.pending_job.is_some());
        let cancel_deadline = app.ide_panel.database.cancel_deadline();
        event_loop.set_control_flow(match suspended_about_wait_plan(cancel_deadline) {
            AboutWaitPlan::Wait => ControlFlow::Wait,
            AboutWaitPlan::WaitUntil(at) => ControlFlow::WaitUntil(at),
        });
        return;
    }
    if automation_running {
        app.render_suspended = false;
    }

    let raw_dt = (now - app.last_frame).as_secs_f32();
    let dt = animation_dt(raw_dt);
    app.last_frame = now;

    let mut needs_redraw = automation_running;

    // Reset the coalesced wake BEFORE the sections below walk the background channels:
    // a result sent during the walk then posts a fresh event instead of being stranded.
    app.ui_waker.begin_drain();

    // Each section only ever raises `needs_redraw`, so OR-ing keeps the old semantics.
    let Some(bench_redraw) = about_to_wait_scroll_bench(app, event_loop, now) else {
        return;
    };
    needs_redraw |= bench_redraw;
    needs_redraw |= about_to_wait_popup_scrolls(app, dt);
    let (hover_redraw, hover_wake_at, hover_poll_pending) =
        about_to_wait_hover_timer(app, raw_dt, dt, now);
    needs_redraw |= hover_redraw;
    needs_redraw |= about_to_wait_settings_scrolls(app, dt);
    needs_redraw |= about_to_wait_main_scroll(app, dt);
    needs_redraw |= about_to_wait_tab_strip_scroll(app, dt);
    let (polls_redraw, background_wake_at) = about_to_wait_background_polls(app, now);
    needs_redraw |= polls_redraw;
    needs_redraw |= about_to_wait_file_watcher(app);
    needs_redraw |= about_to_wait_panel_scrolls(app, dt);
    let (tab_content_redraw, database_refresh_wake_at) =
        about_to_wait_tab_content_scrolls(app, dt, now, wall_ms);
    needs_redraw |= tab_content_redraw;
    needs_redraw |= about_to_wait_terminals(app, dt);
    needs_redraw |= about_to_wait_overlay_animations(app, dt, now);
    needs_redraw |= about_to_wait_selection_drag_autoscroll(app, dt);
    about_to_wait_clamp_editor_scroll(app);
    needs_redraw |= about_to_wait_picker_receivers(app, dt, now);

    about_to_wait_lsp_events(app);

    request_python_inlay_hints_if_needed(app);

    about_to_wait_lsp_server_logs(app);

    needs_redraw |= about_to_wait_highlight(app);

    let git_progress_animating =
        app.ide_panel.git.pending && app.ide_panel.git.pending_label.is_some();
    if git_progress_animating {
        needs_redraw = true;
    }

    update_cursor_blink(app, now, &mut needs_redraw);

    let is_highlighting =
        !app.is_highlighted_once || app.highlighter.has_pending_priority_highlight();
    let idle_blink_enabled = idle_blink_enabled(app);
    let autocomplete_animating = app.autocomplete_active && app.autocomplete_anim_progress < 1.0;
    let scroll_animating = !app.scroll_y.is_settled() || !app.scroll_x.is_settled();
    // Results of these jobs arrive through `UiWaker`; no timer polls for them. Headless
    // `settle` still needs to know that one is outstanding.
    event_loop.set_awaiting_background(
        is_highlighting
            || hover_poll_pending
            || !app.api_request_rx.is_empty()
            || app.api_mock_ty_rx.is_some()
            || app.ide_panel.api.api_runtime_poll_pending()
            || app.ide_panel.database.pending_job.is_some(),
    );
    let deadline_wake_at = earliest_optional_wake(
        hover_wake_at,
        earliest_optional_wake(
            background_wake_at,
            earliest_optional_wake(
                app.ide_panel.database.cancel_deadline(),
                database_refresh_wake_at,
            ),
        ),
    );
    match compute_about_wait_plan(
        now,
        app.last_action,
        needs_redraw,
        app.show_welcome,
        app.is_ide_mode,
        idle_blink_enabled,
        deadline_wake_at,
    ) {
        AboutWaitPlan::Wait => {
            // Headless has no compositor pacing: an idle `Wait` must not spin frames.
            if let Some(w) = app.window.as_ref()
                && (!app.headless_mode
                    || wait_plan_wants_frame(needs_redraw, app.show_welcome, app.is_ide_mode))
            {
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

#[cfg(test)]
mod lsp_action_merge_tests {
    use super::lsp_action_selection_after_prepend;

    #[test]
    fn async_lsp_actions_keep_the_same_existing_item_selected() {
        assert_eq!(lsp_action_selection_after_prepend(4, 2, 3), 5);
        assert_eq!(lsp_action_selection_after_prepend(4, 99, 2), 5);
        assert_eq!(lsp_action_selection_after_prepend(0, 0, 3), 0);
    }
}

#[cfg(test)]
mod markdown_shared_scroll_tick_tests {
    #[test]
    fn main_vertical_scroll_is_ticked_once_and_drives_continuous_polling() {
        // `about_to_wait` runs its tick sections from the about/ fragments.
        let orchestrator = include_str!("about.rs");
        let source = concat!(
            include_str!("about.rs"),
            include_str!("about/about_tick_scroll_sections.rs"),
            include_str!("about/about_tick_input_sections.rs"),
            include_str!("about/about_tick_lsp_sections.rs"),
        );
        let update_call = ["if app.scroll_y.", "update(dt)"].concat();
        assert_eq!(source.matches(&update_call).count(), 1);
        let main_scroll_call = ["about_to_wait_main_scroll", "(app, dt)"].concat();
        assert_eq!(orchestrator.matches(&main_scroll_call).count(), 1);
        let legacy_update = ["update_read_scroll", "(dt)"].concat();
        assert!(!source.contains(&legacy_update));
        assert!(source.contains(
            "let scroll_animating = !app.scroll_y.is_settled() || !app.scroll_x.is_settled();"
        ));
    }
}
