impl App {
    #[cold]
    #[inline(never)]
    pub(crate) fn advance_automation(
        &mut self,
        event_loop: &HostLoop,
        now: Instant,
    ) -> Option<AutomationTick> {
        let mut automation = self.automation.take()?;
        let tick = automation.tick(self, event_loop, now);
        // Keep the controller until ApplicationHandler::exiting so shutdown can
        // distinguish a disposable automation run from a normal user session.
        self.automation = Some(automation);
        Some(tick)
    }

    pub(crate) fn is_automation_mode(&self) -> bool {
        self.automation.is_some()
    }

    /// Automation whose scenario reads the saved session (`startup`); false without automation.
    pub(crate) fn automation_restores_session(&self) -> bool {
        self.automation
            .as_ref()
            .is_some_and(|automation| automation.scenario().restores_session())
    }

    /// Exit-time session save allowed (see `AutomationController::saves_session_now`);
    /// false without automation and in the GUI.
    pub(crate) fn automation_saves_session(&self) -> bool {
        self.automation
            .as_ref()
            .is_some_and(|automation| automation.saves_session_now(self.headless_mode))
    }

    pub(crate) fn write_interrupted_automation_report(&mut self, reason: &str) {
        let Some(mut automation) = self.automation.take() else {
            return;
        };
        automation.write_interrupted_report(reason);
        self.automation = Some(automation);
    }
}

fn resolve_target(app: &App, target: AutomationTarget) -> Result<(f32, f32), String> {
    match target {
        AutomationTarget::Find(find) => {
            find(app).ok_or_else(|| "find target returned no position".to_string())
        }
        AutomationTarget::Ui(id) => app
            .ui_registry
            .element_hits()
            .find_map(|(hit_id, _, rect, _)| if hit_id == id { rect } else { None })
            .map(|rect| (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0))
            .ok_or_else(|| format!("ui element not found: {id:?}")),
    }
}

fn move_pointer(app: &mut App, (x, y): (f32, f32)) {
    app.handle_main_cursor_moved(winit::dpi::PhysicalPosition::new(f64::from(x), f64::from(y)));
}

fn winit_button(button: AutomationButton) -> winit::event::MouseButton {
    match button {
        AutomationButton::Left => winit::event::MouseButton::Left,
    }
}

impl AutomationController {
    /// Executes `WaitUntil`, `Call`, the input steps and the group markers. Input steps take
    /// two or more ticks (pointer move / key press first, the rest after a rendered frame),
    /// the same order as the headless driver commands.
    fn run_group_step(
        &mut self,
        app: &mut App,
        event_loop: &HostLoop,
        step: &AutomationStep,
    ) -> StepResult {
        let progress = self.step_progress;
        match step {
            AutomationStep::WaitUntil { check, .. } => {
                if check(app) {
                    StepResult::Done
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::Call { what, run } => match run(app, &self.options.workspace) {
                Ok(()) => StepResult::Done,
                Err(error) => StepResult::Failed(format!("{what}: {error}")),
            },
            AutomationStep::Key(combo) => match self.key_hold.take() {
                None => {
                    let (input, mods) = match KeyInput::parse_combo(combo) {
                        Ok(parsed) => parsed,
                        Err(error) => return StepResult::Failed(format!("key {combo:?}: {error}")),
                    };
                    self.key_hold = Some(app.press_key_combo(event_loop, input, mods));
                    StepResult::Pending
                }
                Some(hold) => {
                    app.release_key_combo(event_loop, &hold);
                    app.end_key_combo(hold);
                    StepResult::Done
                }
            },
            AutomationStep::Wheel { at, dx, dy } => {
                if progress == 0 {
                    match resolve_target(app, *at) {
                        Ok(point) => move_pointer(app, point),
                        Err(error) => return StepResult::Failed(error),
                    }
                    self.step_progress = 1;
                    return StepResult::Pending;
                }
                app.handle_main_mouse_wheel(winit::event::MouseScrollDelta::LineDelta(*dx, *dy));
                StepResult::Done
            }
            AutomationStep::Click { at, button, mods, clicks } => {
                if progress == 0 {
                    match resolve_target(app, *at) {
                        Ok(point) => move_pointer(app, point),
                        Err(error) => return StepResult::Failed(error),
                    }
                    self.step_progress = 1;
                    return StepResult::Pending;
                }
                let mods = match KeyInput::parse_modifiers(mods) {
                    Ok(mods) => mods,
                    Err(error) => return StepResult::Failed(format!("click {mods:?}: {error}")),
                };
                let saved = std::mem::replace(&mut app.modifiers, mods);
                let button = winit_button(*button);
                for _ in 0..*clicks {
                    for state in [winit::event::ElementState::Pressed, winit::event::ElementState::Released] {
                        app.handle_main_mouse_input(event_loop, state, button);
                    }
                }
                app.modifiers = saved;
                StepResult::Done
            }
            AutomationStep::Drag { from, to, steps } => {
                let steps = u32::from((*steps).max(1));
                if progress == 0 {
                    match resolve_target(app, *from) {
                        Ok(point) => {
                            move_pointer(app, point);
                            self.drag_from = Some(point);
                        }
                        Err(error) => return StepResult::Failed(error),
                    }
                } else if progress == 1 {
                    app.handle_main_mouse_input(
                        event_loop,
                        winit::event::ElementState::Pressed,
                        winit::event::MouseButton::Left,
                    );
                } else {
                    let (Some(start), Ok(end)) = (self.drag_from, resolve_target(app, *to)) else {
                        return StepResult::Failed("drag target returned no position".to_string());
                    };
                    let done = progress - 1;
                    let t = done.min(steps) as f32 / steps as f32;
                    move_pointer(
                        app,
                        (start.0 + (end.0 - start.0) * t, start.1 + (end.1 - start.1) * t),
                    );
                    if done >= steps {
                        app.handle_main_mouse_input(
                            event_loop,
                            winit::event::ElementState::Released,
                            winit::event::MouseButton::Left,
                        );
                        self.drag_from = None;
                        return StepResult::Done;
                    }
                }
                self.step_progress = progress + 1;
                StepResult::Pending
            }
            AutomationStep::GroupStart { name, requires } => {
                let Some(Err(reason)) = requires.map(|requires| requires(app)) else {
                    return StepResult::Done;
                };
                let end = self.steps[self.step_index..]
                    .iter()
                    .position(|step| matches!(step, AutomationStep::GroupEnd));
                let Some(end) = end else {
                    return StepResult::Failed(format!("group {name} has no GroupEnd"));
                };
                println!("PGO_AUTOMATION_GROUP_SKIPPED group={name} reason={reason:?}");
                self.skipped_groups.push(((*name).to_string(), reason));
                // `advance` steps over the `GroupEnd` this lands on.
                self.step_index += end;
                StepResult::Done
            }
            _ => StepResult::Done,
        }
    }
}

#[derive(Debug)]
enum StepResult {
    Pending,
    Done,
    Failed(String),
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalPanelOpenState {
    Open,
    WaitingForPresentation,
    ClosedUnexpectedly,
}

fn duration_ms(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn begin_open_panel_action(step_progress: &mut u32) -> bool {
    if *step_progress != 0 {
        return false;
    }
    *step_progress = 1;
    true
}

fn terminal_panel_open_state_from_values(
    panel_open: bool,
    terminals: impl IntoIterator<Item = (crate::app::terminal::TerminalPresentationIntent, bool)>,
) -> TerminalPanelOpenState {
    if panel_open {
        return TerminalPanelOpenState::Open;
    }
    if terminals.into_iter().any(|(intent, _presentation_ready)| {
        intent == crate::app::terminal::TerminalPresentationIntent::OpenPanelWhenReady
    }) {
        TerminalPanelOpenState::WaitingForPresentation
    } else {
        TerminalPanelOpenState::ClosedUnexpectedly
    }
}

fn terminal_panel_open_state(app: &App) -> TerminalPanelOpenState {
    terminal_panel_open_state_from_values(
        app.ide_panel.is_open(PanelId::Terminal),
        app.ide_panel.terminals.iter().map(|terminal| {
            let ready = crate::app::terminal::lock_terminal_grid(&terminal.grid).presentation_ready;
            (terminal.presentation_intent, ready)
        }),
    )
}

fn panel_failure_diagnostics(app: &App, requested_panel: PanelId) -> String {
    let open_top_panel = app
        .ide_panel
        .slots
        .iter()
        .find(|slot| slot.open && slot.group == crate::app::app_state::PanelGroup::Top)
        .map(|slot| format!("{:?}", slot.id))
        .unwrap_or_else(|| "none".to_string());
    let open_bottom_panel = app
        .ide_panel
        .open_bottom_panel_id()
        .map(|panel| format!("{panel:?}"))
        .unwrap_or_else(|| "none".to_string());
    let active_terminal = if app.ide_panel.terminals.is_empty() {
        "none".to_string()
    } else {
        app.ide_panel.active_terminal.to_string()
    };
    let terminal_presentation = app
        .ide_panel
        .terminals
        .get(app.ide_panel.active_terminal)
        .map_or_else(
            || "active_terminal_state=none".to_string(),
            |terminal| {
                let grid = crate::app::terminal::lock_terminal_grid(&terminal.grid);
                format!(
                    "presentation_ready={} presentation_layout_ready={} presentation_intent={:?}",
                    grid.presentation_ready,
                    grid.presentation_layout_ready,
                    terminal.presentation_intent
                )
            },
        );
    format!(
        "requested_panel={requested_panel:?} open_top_panel={open_top_panel} open_bottom_panel={open_bottom_panel} requested_panel_open={} terminal_count={} active_terminal={active_terminal} terminal_focused={} term_search_focused={} {terminal_presentation} {}",
        app.ide_panel.is_open(requested_panel),
        app.ide_panel.terminals.len(),
        app.ide_panel.terminal_focused,
        app.ide_panel.term_search_focused,
        crate::app::automation_database::transient_diagnostics(app),
    )
}

fn step_failure_context(
    app: &App,
    step: &AutomationStep,
    step_progress: u32,
    hover_last_anchor: Option<(f32, f32)>,
) -> String {
    match step {
        AutomationStep::OpenPanel(panel) => panel_failure_diagnostics(app, *panel),
        AutomationStep::Database(_) => crate::app::automation_database::diagnostics(app),
        AutomationStep::WaitUntil { what, .. } if what.starts_with("api mock") => {
            crate::app::automation_api_mock::diagnostics(app)
        }
        AutomationStep::ShowHover { needle, .. } => hover_failure_diagnostics(
            app,
            app.editor.get_full_text().find(needle),
            step_progress,
            hover_last_anchor,
        ),
        AutomationStep::ScrollHoverTimed { .. } => hover_state_diagnostics(&app.hover),
        AutomationStep::Dart(DartAutomationStep::WaitClosingHints { minimum_count }) => {
            crate::app::automation_dart::diagnostics(app, *minimum_count)
        }
        _ => String::new(),
    }
}

impl App {
    pub(super) fn request_redraw(&self) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}

fn reset_scroll(scroll: &mut crate::scroll::ScrollState) {
    scroll.reset();
}

fn focus_main_editor_semantic(app: &mut App) {
    app.search_focused = false;
    app.ide_panel.project_search.focused = None;
    app.ide_panel.term_search_focused = false;
    app.ide_panel.terminal_focused = false;
    app.ide_panel.git.message_focused = false;
    app.ide_panel.file_tree_focused = false;
    app.ide_panel.lsp_log_filter_focused = false;
    app.ide_panel.lsp_logs_focused = None;
    app.ide_panel.api.focused = None;
}

fn open_panel_semantic(app: &mut App, panel: PanelId) {
    app.ide_panel.open(panel);
    match panel {
        PanelId::Terminal => {
            app.ide_panel.terminal_focused = true;
            app.ide_panel.term_search_focused = false;
            if app.ide_panel.terminals.is_empty() {
                app.add_terminal();
            }
        }
        PanelId::Explorer => {
            if app.ide_panel.file_tree_nodes.is_empty() {
                app.refresh_file_tree();
                app.start_file_watcher();
            }
        }
        PanelId::Git => app.refresh_git_panel(),
        PanelId::Search => {
            app.ide_panel.project_search.focused =
                Some(crate::app::project_search::ProjectSearchField::Query);
        }
        _ => {}
    }
    crate::save_panel_state(&app.ide_panel);
    app.request_redraw();
}

fn open_file_tree_context_semantic(app: &mut App) -> StepResult {
    let Some(node) = app.ide_panel.file_tree_nodes.first().cloned() else {
        return StepResult::Pending;
    };
    app.ide_panel.file_tree_selection.clear();
    app.ide_panel.file_tree_selection.insert(node.path.clone());
    app.ide_panel.file_tree_focused = true;
    let target_dir = if node.is_dir {
        Some(node.path.clone())
    } else {
        node.path.parent().map(Path::to_path_buf)
    };
    app.ide_panel.file_tree_context_menu = Some(crate::app::file_tree::FileTreeContextMenu {
        x: 96.0,
        y: 96.0,
        target_dir,
        target_path: Some(node.path),
        target_is_dir: node.is_dir,
        entries: vec![
            crate::app::file_tree::FileTreeMenuAction::CreateFile,
            crate::app::file_tree::FileTreeMenuAction::CreateDirectory,
            crate::app::file_tree::FileTreeMenuAction::Delete,
            crate::app::file_tree::FileTreeMenuAction::Copy,
            crate::app::file_tree::FileTreeMenuAction::Cut,
            crate::app::file_tree::FileTreeMenuAction::Rename,
            crate::app::file_tree::FileTreeMenuAction::OpenContainedFolder,
            crate::app::file_tree::FileTreeMenuAction::CopyAbsolutePath,
            crate::app::file_tree::FileTreeMenuAction::CopyRelativePath,
        ],
        opened_at: Instant::now(),
    });
    app.request_redraw();
    StepResult::Done
}

fn hover_popup_status(state: &crate::app::mouse::HoverState, byte_offset: usize) -> (bool, bool) {
    let matching = state
        .popup
        .as_ref()
        .is_some_and(|popup| popup.byte_offset == byte_offset)
        && state.byte_offset == Some(byte_offset);
    (matching, matching && state.rect.is_some())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HoverPrerequisites {
    active_file_matches: bool,
    autocomplete_inactive: bool,
    editor_clean: bool,
    highlight_current: bool,
    external_changes_idle: bool,
    renderer_ready: bool,
}

impl HoverPrerequisites {
    fn ready(self) -> bool {
        self.active_file_matches
            && self.autocomplete_inactive
            && self.editor_clean
            && self.highlight_current
            && self.external_changes_idle
            && self.renderer_ready
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HoverProgressAction {
    WaitStable,
    WaitDraw,
    Done,
    Install,
    FailRepeatedClear,
}

fn hover_progress_action(
    prerequisites: HoverPrerequisites,
    popup_status: (bool, bool),
    install_attempts: u32,
) -> HoverProgressAction {
    if !prerequisites.ready() {
        return HoverProgressAction::WaitStable;
    }
    let (matching_popup, drawn_rect) = popup_status;
    if drawn_rect {
        HoverProgressAction::Done
    } else if matching_popup {
        HoverProgressAction::WaitDraw
    } else if install_attempts < PGO_HOVER_MAX_INSTALL_ATTEMPTS {
        HoverProgressAction::Install
    } else {
        HoverProgressAction::FailRepeatedClear
    }
}

fn hover_state_diagnostics(state: &crate::app::mouse::HoverState) -> String {
    format!(
        "popup={} pending={} rect={:?} byte_offset={:?} request_id={:?} definition_request_id={:?} timer={:.3} max_scroll={:.1}",
        state.popup.is_some(),
        state.pending_popup.is_some(),
        state.rect,
        state.byte_offset,
        state.request_id,
        state.definition_request_id,
        state.timer,
        state.max_scroll,
    )
}

fn hover_highlight_is_current(
    app_highlight_complete: bool,
    highlighter_complete: bool,
    highlighter_version: u64,
    editor_version: u64,
) -> bool {
    app_highlight_complete && highlighter_complete && highlighter_version == editor_version
}

fn hover_prerequisites(app: &App, expected_file: &Path) -> HoverPrerequisites {
    HoverPrerequisites {
        active_file_matches: app
            .file_path
            .as_deref()
            .is_some_and(|path| crate::platform::paths_equal(path, expected_file)),
        autocomplete_inactive: !app.autocomplete_active,
        editor_clean: !app.editor.is_dirty(),
        highlight_current: hover_highlight_is_current(
            app.is_highlight_complete,
            app.highlighter.is_complete,
            app.highlighter.current_version,
            app.editor.version,
        ),
        external_changes_idle: app.external_changes_rx.is_none(),
        renderer_ready: app.renderer.is_some(),
    }
}

fn hover_blocker_diagnostics(app: &App) -> String {
    let database_query_modal_open = app.tabs.get(app.active_tab).is_some_and(|tab| {
        matches!(
            &tab.kind,
            crate::app::EditorTabKind::DatabaseQuery(_, state) if state.review.is_some()
        )
    });
    let file_tree_overlay_open =
        crate::app::file_tree::file_tree_overlay_active_for_panel(&app.ide_panel);
    let api_blocking_popup = app.ide_panel.api.mock_python_runtime_open
        || app.ide_panel.api.mock_guide_open
        || app.ide_panel.api.mock_server_detail_open;
    format!(
        "terminal_focused={} file_tree_overlay_open={} database_modal={} database_query_modal={} project_search_help={} api_blocking_popup={} settings_open={} dialog_window_open={}",
        app.ide_panel.terminal_focused,
        file_tree_overlay_open,
        app.ide_panel.database.modal_open(),
        database_query_modal_open,
        app.ide_panel.project_search.help_open,
        api_blocking_popup,
        app.show_settings,
        app.modal_dialog_open(),
    )
}

fn hover_failure_diagnostics(
    app: &App,
    target_byte_offset: Option<usize>,
    install_attempts: u32,
    derived_anchor: Option<(f32, f32)>,
) -> String {
    let active_file = app
        .file_path
        .as_deref()
        .map_or_else(|| "<none>".to_string(), |path| path.display().to_string());
    let renderer = app.renderer.as_ref().map_or_else(
        || "renderer=none".to_string(),
        |renderer| {
            format!(
                "renderer_mouse=({:.1},{:.1}) hide_popups_until_mouse_move={} renderer_scroll=({:.1},{:.1})",
                renderer.last_mouse_x,
                renderer.last_mouse_y,
                renderer.hide_popups_until_mouse_move,
                renderer.last_scroll_x,
                renderer.last_scroll_y,
            )
        },
    );
    format!(
        "active_file={} editor_version={} editor_dirty={} app_highlight_complete={} highlighter_version={} highlighter_complete={} highlight_current={} autocomplete_active={} external_changes_pending={} target_byte_offset={:?} install_attempts={} derived_anchor={:?} {} {} {}",
        active_file,
        app.editor.version,
        app.editor.is_dirty(),
        app.is_highlight_complete,
        app.highlighter.current_version,
        app.highlighter.is_complete,
        hover_highlight_is_current(
            app.is_highlight_complete,
            app.highlighter.is_complete,
            app.highlighter.current_version,
            app.editor.version,
        ),
        app.autocomplete_active,
        app.external_changes_rx.is_some(),
        target_byte_offset,
        install_attempts,
        derived_anchor,
        hover_state_diagnostics(&app.hover),
        renderer,
        hover_blocker_diagnostics(app),
    )
}

fn prepare_automation_hover_pointer(app: &mut App, byte_offset: usize) -> Option<(f32, f32)> {
    let scale = app.renderer.as_ref()?.scale_factor;
    let editor_top_inset = app.editor_top_inset(scale);
    let render_scroll_y = app.scroll_y.current.round() - editor_top_inset;
    let renderer = app.renderer.as_mut()?;
    let anchor = crate::app::mouse::hover_anchor_for_byte(
        renderer,
        &app.editor,
        byte_offset,
        render_scroll_y,
    );

    renderer.last_mouse_x = anchor.0;
    renderer.last_mouse_y = anchor.1;
    renderer.update_popup_mouse_move_gate();
    if renderer.hide_popups_until_mouse_move {
        // Exact overlap with last_known_mouse is still "no move" to production gate.
        // Nudge internal semantic pointer by one pixel, trip same gate, then restore target.
        renderer.last_mouse_x = anchor.0 + 1.0;
        renderer.update_popup_mouse_move_gate();
        renderer.last_mouse_x = anchor.0;
        renderer.last_mouse_y = anchor.1;
        renderer.update_popup_mouse_move_gate();
    }
    Some(anchor)
}

fn install_automation_hover_popup(
    state: &mut crate::app::mouse::HoverState,
    byte_offset: usize,
    popup: crate::app::mouse::HoverPopup,
) {
    *state = crate::app::mouse::HoverState::default();
    state.byte_offset = Some(byte_offset);
    state.timer = 1.0;
    state.popup = Some(popup);
}

fn show_hover_semantic(
    app: &mut App,
    workspace: &Path,
    needle: &str,
    text: &str,
    install_attempts: &mut u32,
    last_anchor: &mut Option<(f32, f32)>,
) -> StepResult {
    if *install_attempts == 0 {
        *last_anchor = None;
    }

    let expected_file = workspace.join("tests/pgo_completion_hover.py");
    let prerequisites = hover_prerequisites(app, &expected_file);
    if !prerequisites.active_file_matches {
        return StepResult::Pending;
    }

    let editor_text = app.editor.get_full_text();
    let Some(byte_offset) = editor_text.find(needle) else {
        return StepResult::Failed(format!("hover target was not found: {needle}"));
    };

    let popup_status = hover_popup_status(&app.hover, byte_offset);
    match hover_progress_action(prerequisites, popup_status, *install_attempts) {
        HoverProgressAction::WaitStable => StepResult::Pending,
        HoverProgressAction::Done => StepResult::Done,
        HoverProgressAction::WaitDraw => {
            app.request_redraw();
            StepResult::Pending
        }
        HoverProgressAction::FailRepeatedClear => StepResult::Failed(format!(
            "synthetic hover disappeared before draw after {} install attempts; {}",
            *install_attempts,
            hover_failure_diagnostics(app, Some(byte_offset), *install_attempts, *last_anchor),
        )),
        HoverProgressAction::Install => {
            let Some(anchor) = prepare_automation_hover_pointer(app, byte_offset) else {
                return StepResult::Pending;
            };
            *last_anchor = Some(anchor);
            *install_attempts = (*install_attempts).saturating_add(1);

            let mut popup = crate::app::events::source_hover_popup_for_editor(
                &app.editor,
                byte_offset,
                text.to_string(),
                Some("tests.pgo_completion_hover"),
                anchor,
            );
            popup.anim_progress = 1.0;
            install_automation_hover_popup(&mut app.hover, byte_offset, popup);
            println!(
                "PGO_AUTOMATION_HOVER install_attempt={} target={} byte_offset={} derived_anchor=({:.1},{:.1}) {}",
                *install_attempts,
                needle,
                byte_offset,
                anchor.0,
                anchor.1,
                hover_failure_diagnostics(app, Some(byte_offset), *install_attempts, *last_anchor),
            );
            app.request_redraw();
            StepResult::Pending
        }
    }
}

fn scroll_active_api_tab(app: &mut App, delta: f32) {
    let scale = app
        .renderer
        .as_ref()
        .map_or(1.0, |renderer| renderer.scale_factor);
    let visible_h = app.renderer.as_ref().map_or(720.0, |renderer| {
        crate::render_view::api_tab_viewport_height(
            renderer.height,
            app.show_welcome,
            app.is_ide_mode,
            scale,
        )
    });
    let Some((meta, state)) = app.active_api_tab() else {
        return;
    };
    let spec_id = meta.spec_id;
    let max_scroll = crate::app::api_client::api_tab_max_scroll(
        app.ide_panel.api.models.get(&spec_id),
        state,
        Some(&app.ide_panel.api),
        visible_h,
        scale,
    );
    let Some(tab) = app.tabs.get_mut(app.active_tab) else {
        return;
    };
    let crate::app::EditorTabKind::ApiClient(_, state) = &mut tab.kind else {
        return;
    };
    state.tab_scroll.scroll_by(delta);
    state.tab_scroll.clamp_target(0.0, max_scroll);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TerminalTuiState {
    alternate_screen: bool,
    non_blank_cells: usize,
}

fn terminal_grid_tui_state(grid: &crate::app::terminal::TermGrid) -> TerminalTuiState {
    TerminalTuiState {
        alternate_screen: grid.is_alt,
        non_blank_cells: grid
            .lines
            .iter()
            .flat_map(|line| line.iter())
            .filter(|cell| !cell.c.is_whitespace())
            .count(),
    }
}

fn active_terminal_tui_state(app: &App) -> Option<TerminalTuiState> {
    let terminal = app.ide_panel.terminals.get(app.ide_panel.active_terminal)?;
    let grid = terminal.grid.try_lock().ok()?;
    Some(terminal_grid_tui_state(&grid))
}

const TERMINAL_BASIC_OUTPUT: &str = "RRiter";

fn terminal_basic_command() -> &'static [u8] {
    b"echo RRiter\r"
}

fn terminal_grid_contains(grid: &crate::app::terminal::TermGrid, needle: &str) -> bool {
    grid.scrollback.iter().chain(grid.lines.iter()).any(|line| {
        line.iter()
            .map(|cell| cell.c)
            .collect::<String>()
            .contains(needle)
    })
}

fn active_terminal_contains(app: &App, needle: &str) -> Option<bool> {
    let terminal = app.ide_panel.terminals.get(app.ide_panel.active_terminal)?;
    let grid = terminal.grid.try_lock().ok()?;
    Some(terminal_grid_contains(&grid, needle))
}

fn write_terminal_semantic(app: &mut App, bytes: &[u8]) -> StepResult {
    app.ide_panel.terminal_focused = true;
    let Some(terminal) = app.ide_panel.terminals.get(app.ide_panel.active_terminal) else {
        return StepResult::Pending;
    };
    match terminal.write_input(bytes) {
        Ok(()) => StepResult::Done,
        Err(error) => StepResult::Failed(format!("terminal input failed: {error}")),
    }
}
