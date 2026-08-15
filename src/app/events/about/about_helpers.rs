#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AboutWaitPlan {
    Wait,
    WaitUntil(Instant),
}

const DRAG_AUTOSCROLL_EDGE_PX: f32 = 58.0;
const DRAG_AUTOSCROLL_MIN_SPEED: f32 = 360.0;
const DRAG_AUTOSCROLL_MAX_SPEED: f32 = 7200.0;
const DRAG_AUTOSCROLL_ACCEL: f32 = 0.40;
const DRAG_AUTOSCROLL_TOP_BOOST: f32 = 1.22;
const PYTHON_INLAY_HINT_IDLE_DELAY: std::time::Duration = std::time::Duration::from_millis(180);
const PYTHON_INLAY_FULL_FILE_MAX_LINES: usize = 2_500;
const PYTHON_INLAY_VISIBLE_MARGIN_LINES: usize = 80;

#[inline(always)]
fn animation_dt(raw_dt: f32) -> f32 {
    raw_dt.min(0.016)
}

fn clear_python_inlay_hint_state(app: &mut App) {
    app.python_inlay_hints.clear();
    app.python_inlay_hint_path = None;
    app.python_inlay_hint_range = None;
    app.python_inlay_hint_pending_request_id = None;
    app.python_inlay_hint_pending_path = None;
    app.python_inlay_hint_pending_range = None;
}

fn python_inlay_final_line_col(app: &App, line: usize) -> u32 {
    app.editor
        .line_text_owned(line)
        .trim_end_matches(|ch| ch == '\r' || ch == '\n')
        .chars()
        .map(|ch| ch.len_utf16() as u32)
        .sum()
}

fn python_inlay_hint_request_range(
    app: &App,
) -> Option<(u32, u32, u32, u32, crate::app::PythonInlayHintLineRange)> {
    let line_count = app.editor.line_offsets.len().max(1);
    if line_count <= PYTHON_INLAY_FULL_FILE_MAX_LINES {
        let text = app.editor.get_full_text();
        let (end_line, end_col) =
            crate::lsp::offset_to_lsp_pos(&text, text.len(), &app.editor.line_offsets);
        return Some((0, 0, end_line, end_col, (0, line_count as u32)));
    }

    let renderer = app.renderer.as_ref()?;
    let s = renderer.scale_factor;
    let tab_bar_h = app.editor_top_inset(s);
    let editor_bottom_h = if app.is_ide_mode {
        app.ide_panel.editor_reserved_bottom_height(s)
    } else {
        0.0
    };
    let editor_height = crate::render_view::editor_view_height(
        renderer.height,
        tab_bar_h,
        editor_bottom_h,
        app.is_ide_mode,
        s,
    );
    let line_height = renderer.line_height.max(1.0);
    let first_visible = (app.scroll_y.current.max(0.0) / line_height).floor() as usize;
    let first_visible = first_visible.min(line_count.saturating_sub(1));
    let visible_lines = (editor_height / line_height).ceil().max(1.0) as usize + 1;
    let start_line = first_visible.saturating_sub(PYTHON_INLAY_VISIBLE_MARGIN_LINES);
    let mut end_exclusive = first_visible
        .saturating_add(visible_lines)
        .saturating_add(PYTHON_INLAY_VISIBLE_MARGIN_LINES)
        .min(line_count);
    if end_exclusive <= start_line {
        end_exclusive = (start_line + 1).min(line_count);
    }

    let (end_line, end_col) = if end_exclusive < line_count {
        (end_exclusive as u32, 0)
    } else {
        let last_line = line_count.saturating_sub(1);
        (last_line as u32, python_inlay_final_line_col(app, last_line))
    };

    Some((
        start_line as u32,
        0,
        end_line,
        end_col,
        (start_line as u32, end_exclusive as u32),
    ))
}

fn request_python_inlay_hints_if_needed(app: &mut App) {
    if !app.is_ide_mode || !matches!(app.file_extension.as_str(), "py" | "pyi" | "dart") {
        clear_python_inlay_hint_state(app);
        return;
    }
    let Some(path) = app.file_path.clone() else {
        clear_python_inlay_hint_state(app);
        return;
    };
    let Some((start_line, start_col, end_line, end_col, range)) =
        python_inlay_hint_request_range(app)
    else {
        return;
    };
    let cache_key = (path.clone(), app.file_extension.clone());
    if let Some((version, cached_range, hints)) = app.python_inlay_hint_cache.get(&cache_key)
        && *version == app.editor.version
        && *cached_range == range
    {
        if app.python_inlay_hint_path.as_ref() != Some(&path)
            || app.python_inlay_hint_range != Some(range)
            || app.python_inlay_hint_version != app.editor.version
        {
            app.python_inlay_hints.clear();
            app.python_inlay_hints.extend_from_slice(hints);
            app.python_inlay_hint_path = Some(path.clone());
            app.python_inlay_hint_range = Some(range);
            app.python_inlay_hint_version = *version;
        }
        return;
    }
    if app.python_inlay_hint_pending_request_id.is_some()
        || app.python_inlay_hint_path.as_ref() == Some(&path)
            && app.python_inlay_hint_range == Some(range)
            && app.python_inlay_hint_version == app.editor.version
        || app.last_action.elapsed() < PYTHON_INLAY_HINT_IDLE_DELAY
    {
        return;
    }

    let Some(lsp) = app.lsp.as_mut() else {
        return;
    };
    if let Some(id) = lsp.request_inlay_hints(
        &path,
        &app.file_extension,
        start_line,
        start_col,
        end_line,
        end_col,
    )
    {
        app.python_inlay_hint_pending_request_id = Some(id);
        app.python_inlay_hint_pending_path = Some(path);
        app.python_inlay_hint_pending_range = Some(range);
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

#[inline(always)]
fn selection_drag_autoscroll_delta(pos: f32, start: f32, end: f32) -> f32 {
    if pos < start {
        pos - start
    } else if pos > end {
        pos - end
    } else {
        0.0
    }
}

#[inline(always)]
pub(super) fn selection_drag_active_on_cursor_leave(
    is_dragging: bool,
    show_settings: bool,
    is_dragging_terminal: bool,
    last_click_ui_id: Option<crate::ui_system::UiId>,
) -> bool {
    is_dragging
        && !show_settings
        && matches!(
            (last_click_ui_id, is_dragging_terminal),
            (Some(crate::ui_system::UiId::EditorTextBody), false)
                | (Some(crate::ui_system::UiId::TerminalBody), true)
        )
}

#[inline(always)]
pub(super) fn project_cursor_outside_window_on_leave(
    x: f32,
    y: f32,
    window_w: f32,
    window_h: f32,
) -> (f32, f32) {
    if !x.is_finite()
        || !y.is_finite()
        || !window_w.is_finite()
        || !window_h.is_finite()
        || window_w <= 0.0
        || window_h <= 0.0
        || x < 0.0
        || x > window_w
        || y < 0.0
        || y > window_h
    {
        return (x, y);
    }

    let left = x;
    let right = window_w - x;
    let top = y;
    let bottom = window_h - y;

    if left <= right && left <= top && left <= bottom {
        (-1.0, y)
    } else if right <= top && right <= bottom {
        (window_w + 1.0, y)
    } else if top <= bottom {
        (x, -1.0)
    } else {
        (x, window_h + 1.0)
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
    let (_, bottom_pad) = crate::render_view::terminal_ui::terminal_text_padding(scale);
    let offset_from_bottom =
        (term_y + term_h - bottom_pad - my + scroll_offset) / char_h.max(0.0001);
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

#[inline(always)]
fn tab_drag_animation_active(ide_panel: &crate::app::IdePanelState) -> bool {
    ide_panel.tab_drag.is_some() || ide_panel.terminal_tab_drag.is_some()
}

#[inline(always)]
fn needs_continuous_poll(
    autocomplete_animating: bool,
    git_progress_animating: bool,
    scroll_animating: bool,
) -> bool {
    autocomplete_animating || git_progress_animating || scroll_animating
}

fn active_context_menu_opened_at(ide_panel: &crate::app::IdePanelState) -> Option<Instant> {
    [
        ide_panel
            .file_tree_context_menu
            .as_ref()
            .map(|menu| menu.opened_at),
        ide_panel
            .database
            .context_menu
            .as_ref()
            .map(|menu| menu.opened_at),
        ide_panel.git.commit_menu_opened_at,
        ide_panel.git.commit_options_menu_opened_at,
        ide_panel.git.active_repo_action_menu_opened_at(),
    ]
    .into_iter()
    .flatten()
    .max()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_context_menu_open_times_participate_in_animation_redraw_selection() {
        let mut ide_panel = crate::app::IdePanelState::default();
        let start = Instant::now();

        ide_panel.git.commit_menu_opened_at = Some(start);
        assert_eq!(active_context_menu_opened_at(&ide_panel), Some(start));

        let options = start + std::time::Duration::from_millis(2);
        ide_panel.git.commit_menu_opened_at = None;
        ide_panel.git.commit_options_menu_opened_at = Some(options);
        assert_eq!(active_context_menu_opened_at(&ide_panel), Some(options));

        let repo = options + std::time::Duration::from_millis(2);
        ide_panel.git.commit_options_menu_opened_at = None;
        ide_panel.git.toggle_repo_action_menu(0, repo);
        assert_eq!(active_context_menu_opened_at(&ide_panel), Some(repo));

        assert!(crate::app::context_menu::context_menu_anim_progress(
            repo,
            repo + std::time::Duration::from_millis(50),
        ) < 1.0);
        assert_eq!(
            crate::app::context_menu::context_menu_anim_progress(
                repo,
                repo + std::time::Duration::from_secs_f32(
                    crate::app::context_menu::CONTEXT_MENU_ANIM_SECS,
                ),
            ),
            1.0
        );
    }

    #[test]
    fn editor_and_terminal_tab_drag_share_continuous_redraw_lifecycle() {
        let drag = crate::app::TabDragState {
            start_idx: 0,
            start_x: 10.0,
            current_x: 20.0,
            threshold_passed: true,
        };
        let mut ide_panel = crate::app::IdePanelState::default();
        assert!(!tab_drag_animation_active(&ide_panel));

        ide_panel.tab_drag = Some(drag.clone());
        assert!(tab_drag_animation_active(&ide_panel));

        ide_panel.tab_drag = None;
        ide_panel.terminal_tab_drag = Some(drag);
        assert!(tab_drag_animation_active(&ide_panel));
    }

    #[test]
    fn active_animation_keeps_event_loop_polling() {
        for (autocomplete, git_progress, scroll) in
            [(true, false, false), (false, true, false), (false, false, true)]
        {
            assert!(needs_continuous_poll(autocomplete, git_progress, scroll));
        }
        assert!(!needs_continuous_poll(false, false, false));
    }

    #[test]
    fn animation_dt_restores_smooth_idle_scroll_start() {
        assert_eq!(animation_dt(1.0 / 60.0), 0.016);
        assert!((animation_dt(1.0 / 240.0) - 1.0 / 240.0).abs() < f32::EPSILON);
        assert_eq!(animation_dt(0.5), 0.016);
        assert_eq!(animation_dt(0.0), 0.0);
    }

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
    fn tab_drag_autoscroll_keeps_inside_edge_band_and_outside_window_distance() {
        assert_eq!(drag_autoscroll_delta(50.0, 100.0, 500.0, 40.0), -50.0);
        assert_eq!(drag_autoscroll_delta(120.0, 100.0, 500.0, 40.0), -20.0);
        assert_eq!(drag_autoscroll_delta(480.0, 100.0, 500.0, 40.0), 20.0);
        assert_eq!(drag_autoscroll_delta(540.0, 100.0, 500.0, 40.0), 40.0);
        assert_eq!(drag_autoscroll_delta(250.0, 100.0, 500.0, 40.0), 0.0);
        assert!(drag_autoscroll_speed(30.0, false) >= DRAG_AUTOSCROLL_MIN_SPEED);
        assert!(drag_autoscroll_speed(-30.0, true) > drag_autoscroll_speed(30.0, false));
    }

    #[test]
    fn selection_drag_autoscroll_starts_only_outside_viewport() {
        for pos in [100.0, 101.0, 120.0, 480.0, 499.0, 500.0] {
            assert_eq!(selection_drag_autoscroll_delta(pos, 100.0, 500.0), 0.0);
        }

        assert_eq!(selection_drag_autoscroll_delta(99.0, 100.0, 500.0), -1.0);
        assert_eq!(selection_drag_autoscroll_delta(80.0, 100.0, 500.0), -20.0);
        assert_eq!(selection_drag_autoscroll_delta(501.0, 100.0, 500.0), 1.0);
        assert_eq!(selection_drag_autoscroll_delta(540.0, 100.0, 500.0), 40.0);
    }

    #[test]
    fn selection_drag_autoscroll_preserves_far_outside_distance() {
        assert_eq!(selection_drag_autoscroll_delta(-200.0, 100.0, 500.0), -300.0);
        assert_eq!(selection_drag_autoscroll_delta(1400.0, 100.0, 500.0), 900.0);
    }

    #[test]
    fn cursor_leave_projection_crosses_nearest_window_edge() {
        assert_eq!(
            project_cursor_outside_window_on_leave(400.0, 599.0, 800.0, 600.0),
            (400.0, 601.0)
        );
        assert_eq!(
            project_cursor_outside_window_on_leave(400.0, 1.0, 800.0, 600.0),
            (400.0, -1.0)
        );
        assert_eq!(
            project_cursor_outside_window_on_leave(1.0, 300.0, 800.0, 600.0),
            (-1.0, 300.0)
        );
        assert_eq!(
            project_cursor_outside_window_on_leave(799.0, 300.0, 800.0, 600.0),
            (801.0, 300.0)
        );
        assert_eq!(
            project_cursor_outside_window_on_leave(0.0, 0.0, 800.0, 600.0),
            (-1.0, 0.0)
        );
        assert_eq!(
            project_cursor_outside_window_on_leave(400.0, 601.0, 800.0, 600.0),
            (400.0, 601.0)
        );

        let (_, left_exit_y) =
            project_cursor_outside_window_on_leave(1.0, 300.0, 800.0, 600.0);
        let (_, right_exit_y) =
            project_cursor_outside_window_on_leave(799.0, 300.0, 800.0, 600.0);
        assert_eq!(selection_drag_autoscroll_delta(left_exit_y, 100.0, 500.0), 0.0);
        assert_eq!(selection_drag_autoscroll_delta(right_exit_y, 100.0, 500.0), 0.0);
    }

    #[test]
    fn cursor_leave_projection_is_selection_only() {
        use crate::ui_system::UiId;

        assert!(selection_drag_active_on_cursor_leave(
            true,
            false,
            false,
            Some(UiId::EditorTextBody),
        ));
        assert!(selection_drag_active_on_cursor_leave(
            true,
            false,
            true,
            Some(UiId::TerminalBody),
        ));

        for (is_terminal, id) in [
            (false, UiId::EditorScrollbarY),
            (false, UiId::EditorMinimap),
            (true, UiId::TerminalTab(0)),
        ] {
            assert!(!selection_drag_active_on_cursor_leave(
                true,
                false,
                is_terminal,
                Some(id),
            ));
        }
        assert!(!selection_drag_active_on_cursor_leave(
            false,
            false,
            false,
            Some(UiId::EditorTextBody),
        ));
        assert!(!selection_drag_active_on_cursor_leave(
            true,
            true,
            false,
            Some(UiId::EditorTextBody),
        ));
        assert!(!selection_drag_active_on_cursor_leave(
            true,
            false,
            true,
            Some(UiId::EditorTextBody),
        ));
        assert!(!selection_drag_active_on_cursor_leave(
            true,
            false,
            false,
            Some(UiId::TerminalBody),
        ));
    }

    #[test]
    fn non_ide_editor_bottom_autoscroll_starts_after_native_cursor_leave() {
        let window_w = 800.0;
        let window_h = 600.0;
        let editor_top = 38.0;
        let editor_h = crate::render_view::editor_view_height(
            window_h,
            editor_top,
            0.0,
            false,
            1.0,
        );
        let editor_bottom = editor_top + editor_h;
        assert_eq!(editor_bottom, window_h);

        let last_inside_y = window_h - 1.0;
        assert_eq!(
            selection_drag_autoscroll_delta(last_inside_y, editor_top, editor_bottom),
            0.0
        );

        let (_, projected_y) = project_cursor_outside_window_on_leave(
            window_w * 0.5,
            last_inside_y,
            window_w,
            window_h,
        );
        assert!(projected_y > window_h);
        assert!(selection_drag_autoscroll_delta(projected_y, editor_top, editor_bottom) > 0.0);
    }

    #[test]
    fn selection_autoscroll_uses_registered_body_rects_and_updates_endpoints() {
        let about = include_str!("../about.rs");
        let terminal = about
            .split("if app.ide_panel.is_dragging_terminal && app.is_dragging && !app.show_settings")
            .nth(1)
            .unwrap()
            .split("if app.is_dragging && !app.ide_panel.is_dragging_terminal")
            .next()
            .unwrap();
        assert!(terminal.contains("rect_for(crate::ui_system::UiId::TerminalBody)"));
        assert!(terminal.contains(
            "selection_drag_autoscroll_delta(my, term_y, term_y + term_h)"
        ));
        assert!(terminal.contains("term.scroll_y.target ="));
        assert!(terminal.contains("terminal_drag_cell("));
        assert!(terminal.contains("grid.selection = Some"));
        assert!(!terminal.contains("DRAG_AUTOSCROLL_EDGE_PX"));
        assert!(!terminal.contains("terminal_body_rect("));

        let editor = about
            .split(
                "if app.is_dragging && !app.ide_panel.is_dragging_terminal && !app.scroll_y.is_dragging",
            )
            .nth(1)
            .unwrap()
            .split("if let Some(w) = app.window.as_ref()")
            .next()
            .unwrap();
        assert!(editor.contains("rect_for(crate::ui_system::UiId::EditorTextBody)"));
        assert!(editor.contains(
            "selection_drag_autoscroll_delta(my, editor_y, editor_y + editor_h)"
        ));
        assert!(editor.contains(
            "selection_drag_autoscroll_delta(mx, editor_x, editor_x + editor_w)"
        ));
        assert!(editor.contains("app.scroll_y.target +="));
        assert!(editor.contains("app.scroll_x.target +="));
        assert!(editor.contains("app.editor.set_cursor_at_pos("));
        assert!(!editor.contains("DRAG_AUTOSCROLL_EDGE_PX"));
        assert!(!editor.contains("drag_autoscroll_editor_bottom("));
    }

    #[test]
    fn selection_drag_lifecycle_projects_cursor_leave_until_release_or_focus_loss() {
        let cursor = include_str!("../../mouse/cursor.rs");
        let cursor_move = cursor
            .split("pub fn handle_main_cursor_moved")
            .nth(1)
            .unwrap()
            .split("if self.dialog_window.is_some()")
            .next()
            .unwrap();
        assert!(cursor_move.contains("renderer.last_mouse_x = px;"));
        assert!(cursor_move.contains("renderer.last_mouse_y = py;"));

        let events = include_str!("../../events.rs");
        assert!(events.contains(
            "WindowEvent::CursorMoved { position, .. } => self.handle_main_cursor_moved(position)"
        ));
        let cursor_left = events
            .split("WindowEvent::CursorLeft { .. } => {")
            .nth(1)
            .unwrap()
            .split("WindowEvent::Ime")
            .next()
            .unwrap();
        assert!(cursor_left.contains("about::selection_drag_active_on_cursor_leave("));
        assert!(cursor_left.contains("about::project_cursor_outside_window_on_leave("));
        assert!(!cursor_left.contains("cancel_pointer_interactions"));
        let focus = events
            .split("WindowEvent::Focused(focused) =>")
            .nth(1)
            .unwrap()
            .split("WindowEvent::Occluded")
            .next()
            .unwrap();
        assert!(focus.contains("self.cancel_pointer_interactions();"));
        assert!(focus.contains("self.render_suspended = false;"));
        assert!(focus.contains("self.render_suspended = true;"));
        let occluded = events
            .split("WindowEvent::Occluded(occluded) =>")
            .nth(1)
            .unwrap()
            .split("WindowEvent::ScaleFactorChanged")
            .next()
            .unwrap();
        assert!(occluded.contains("self.render_suspended = occluded;"));
        assert!(occluded.contains("if !occluded"));

        let input = include_str!("../../mouse/input.rs");
        let release = input
            .split("// Завершаем DnD и ресайз IDE-панелей")
            .nth(1)
            .unwrap()
            .split("if state == ElementState::Pressed")
            .next()
            .unwrap();
        assert!(release.contains("self.cancel_pointer_interactions();"));
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
