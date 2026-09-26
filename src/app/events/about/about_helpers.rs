#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AboutWaitPlan {
    Wait,
    WaitUntil(Instant),
}

const PYTHON_INLAY_HINT_IDLE_DELAY: std::time::Duration = std::time::Duration::from_millis(180);
const PYTHON_INLAY_FULL_FILE_MAX_LINES: usize = 2_500;
const PYTHON_INLAY_VISIBLE_MARGIN_LINES: usize = 80;

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

/// Whether the app itself wants the next frame (as opposed to an idle `Wait`).
pub(crate) fn wait_plan_wants_frame(needs_redraw: bool, show_welcome: bool, is_ide_mode: bool) -> bool {
    needs_redraw || (show_welcome && is_ide_mode)
}

pub(crate) fn compute_about_wait_plan(
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
    if wait_plan_wants_frame(needs_redraw, show_welcome, is_ide_mode) {
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

fn suspended_about_wait_plan(now: Instant, database_job_pending: bool) -> AboutWaitPlan {
    if database_job_pending {
        AboutWaitPlan::WaitUntil(now + std::time::Duration::from_millis(100))
    } else {
        AboutWaitPlan::Wait
    }
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
impl App {
    pub(crate) fn reviewer_tick_markdown_read_selection_autoscroll(
        &mut self,
        dt: f32,
        shared_scroll_updated: bool,
    ) -> bool {
        update_markdown_read_selection_autoscroll(self, dt, shared_scroll_updated)
    }
}
