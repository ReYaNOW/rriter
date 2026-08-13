use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::json;
use winit::dpi::PhysicalSize;
use winit::event_loop::ActiveEventLoop;

use crate::app::api_client::ApiFocus;
use crate::app::{App, PanelId};

pub const PGO_AUTOMATION_SCENARIO_VERSION: u32 = 12;

const TIMED_SCROLL_HZ: f32 = 120.0;
const TIMED_SCROLL_PAUSE_SECS: f32 = 2.0;
const GIT_FIXTURE_COMMIT_COUNT: usize = 1_000;
const GIT_FIXTURE_BRANCH_COUNT: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq)]
struct TimedScrollPlan {
    expected_impulses: u32,
    direction: f32,
    done: bool,
}

fn timed_scroll_plan(elapsed: f32, duration_secs: u16) -> TimedScrollPlan {
    let duration = f32::from(duration_secs).max(TIMED_SCROLL_PAUSE_SECS + 1.0);
    if elapsed >= duration {
        return TimedScrollPlan {
            expected_impulses: ((duration - TIMED_SCROLL_PAUSE_SECS) * TIMED_SCROLL_HZ) as u32,
            direction: 0.0,
            done: true,
        };
    }

    let first_scroll_end = (duration - TIMED_SCROLL_PAUSE_SECS) * 0.5;
    let second_scroll_start = first_scroll_end + TIMED_SCROLL_PAUSE_SECS;
    let (active_elapsed, direction) = if elapsed < first_scroll_end {
        (elapsed, 1.0)
    } else if elapsed < second_scroll_start {
        (first_scroll_end, 0.0)
    } else {
        (first_scroll_end + (elapsed - second_scroll_start), -1.0)
    };
    TimedScrollPlan {
        expected_impulses: (active_elapsed * TIMED_SCROLL_HZ).floor() as u32,
        direction,
        done: false,
    }
}

fn autocomplete_failure_diagnostics(app: &App, expected: &str) -> String {
    let prefix = app.get_current_word_prefix();
    let cursor = app.editor.cursor.min(app.editor.len());
    let option_summary = app
        .autocomplete_options
        .iter()
        .take(12)
        .map(|(item, _)| {
            format!(
                "{}:{:?}:{}..{}",
                item.word, item.kind, item.scope_start, item.scope_end
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let source_summary = app
        .highlighter
        .completions
        .iter()
        .filter(|item| {
            item.word == expected
                || (!prefix.is_empty() && item.word.starts_with(&prefix))
                || item.word.starts_with("pgo_")
        })
        .take(20)
        .map(|item| {
            format!(
                "{}:{:?}:{}..{}",
                item.word, item.kind, item.scope_start, item.scope_end
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "completion {expected} was not produced; prefix={prefix:?} cursor={cursor} editor_len={} active={} mode={:?} options={} option_items=[{}] highlighter_complete={} app_highlight_complete={} source_items={} relevant_source=[{}] file={}",
        app.editor.len(),
        app.autocomplete_active,
        app.autocomplete_mode,
        app.autocomplete_options.len(),
        option_summary,
        app.highlighter.is_complete,
        app.is_highlight_complete,
        app.highlighter.completions.len(),
        source_summary,
        app.file_path
            .as_deref()
            .map_or_else(|| "<none>".to_string(), |path| path.display().to_string()),
    )
}

#[derive(Debug, Clone)]
pub struct AutomationOptions {
    pub workspace: PathBuf,
    pub report_path: PathBuf,
    pub timeout: Duration,
}

#[derive(Debug, Clone)]
enum AutomationStep {
    WaitReady,
    ResizeWindow {
        width: u32,
        height: u32,
    },
    ApplyWorkspace,
    WaitFileTree,
    WaitFrames(u16),
    WaitMillis(u64),
    OpenPanel(PanelId),
    ExpandWorkspaceRoot,
    OpenFile(PathBuf),
    SwitchToFile(PathBuf),
    WaitHighlight,
    FocusEditor,
    TypeText(&'static str),
    SaveCurrentFile,
    OpenActiveTabContext,
    OpenFileTreeContext,
    CloseContextMenu,
    OpenSearch,
    SetSearchQuery(&'static str),
    ToggleSearchCase,
    NextSearchResult,
    PreviousSearchResult,
    CloseSearch,
    ScrollEditorTimed {
        duration_secs: u16,
    },
    JumpMinimap(f32),
    ToggleFirstFold,
    SetEditorCursorAfter(&'static str),
    TriggerAutocomplete(&'static str),
    SelectAutocomplete(&'static str),
    ApplyAutocomplete(&'static str),
    ShowHover {
        needle: &'static str,
        text: &'static str,
    },
    ScrollHoverTimed {
        duration_secs: u16,
    },
    ClearHover,
    SetProjectSearchQuery(&'static str),
    RunProjectSearch,
    WaitProjectSearch,
    JumpFirstProjectSearchMatch,
    WaitGit,
    ToggleGitGraph,
    WaitGitGraph,
    LoadGitGraph {
        min_commits: usize,
    },
    ScrollGitGraphTimed {
        duration_secs: u16,
    },
    WaitTerminal,
    RunTerminalHtop,
    WaitTerminalHtopVisible,
    InterruptTerminal,
    WaitTerminalHtopExit,
    RunTerminalBasicCommand,
    WaitTerminalBasicCommandVisible,
    ImportApiSpec,
    WaitApiSpec,
    WaitApiRoutesPanel,
    ScrollApiRoutesTimed {
        duration_secs: u16,
    },
    ResetApiPanelScroll,
    SetApiRouteFilter(&'static str),
    WaitApiRouteFilter(&'static str),
    OpenApiRouteMatching(&'static str),
    WaitApiRouteOpen(&'static str),
    ScrollApiTabTimed {
        duration_secs: u16,
    },
    OpenApiAuth,
    FocusApiAuth(&'static str),
    SetApiAuthValue {
        scheme: &'static str,
        value: &'static str,
    },
    SaveApiAuth {
        scheme: &'static str,
        value: &'static str,
    },
    StartApiRequest,
    WaitApiResponse {
        expected_status: u16,
        body_marker: &'static str,
    },
    ResetApiTabScroll,
    ClearApiRouteFilter,
    ShowSettings(bool),
    SetSettingsTab(usize),
    AddSettingsIgnore(&'static str),
    RemoveSettingsIgnore(&'static str),
    RefreshSettingsTools,
    Finish,
}

impl AutomationStep {
    fn name(&self) -> String {
        match self {
            Self::WaitReady => "wait-ready".to_string(),
            Self::ResizeWindow { width, height } => format!("resize-{width}x{height}"),
            Self::ApplyWorkspace => "apply-workspace".to_string(),
            Self::WaitFileTree => "wait-file-tree".to_string(),
            Self::WaitFrames(frames) => format!("wait-{frames}-frames"),
            Self::WaitMillis(millis) => format!("wait-{millis}ms"),
            Self::OpenPanel(panel) => format!("open-panel:{panel:?}"),
            Self::ExpandWorkspaceRoot => "expand-workspace-root".to_string(),
            Self::OpenFile(path) => format!("open-file:{}", path.display()),
            Self::SwitchToFile(path) => format!("switch-to-file:{}", path.display()),
            Self::WaitHighlight => "wait-highlight".to_string(),
            Self::FocusEditor => "focus-editor".to_string(),
            Self::TypeText(_) => "type-text".to_string(),
            Self::SaveCurrentFile => "save-current-file".to_string(),
            Self::OpenActiveTabContext => "open-active-tab-context".to_string(),
            Self::OpenFileTreeContext => "open-file-tree-context".to_string(),
            Self::CloseContextMenu => "close-context-menu".to_string(),
            Self::OpenSearch => "open-search".to_string(),
            Self::SetSearchQuery(query) => format!("set-search-query:{query}"),
            Self::ToggleSearchCase => "toggle-search-case".to_string(),
            Self::NextSearchResult => "next-search-result".to_string(),
            Self::PreviousSearchResult => "previous-search-result".to_string(),
            Self::CloseSearch => "close-search".to_string(),
            Self::ScrollEditorTimed { duration_secs } => {
                format!("scroll-editor-timed:{duration_secs}s")
            }
            Self::JumpMinimap(fraction) => format!("jump-minimap:{fraction:.2}"),
            Self::ToggleFirstFold => "toggle-first-fold".to_string(),
            Self::SetEditorCursorAfter(needle) => format!("set-cursor-after:{needle}"),
            Self::TriggerAutocomplete(word) => format!("trigger-autocomplete:{word}"),
            Self::SelectAutocomplete(word) => format!("select-autocomplete:{word}"),
            Self::ApplyAutocomplete(word) => format!("apply-autocomplete:{word}"),
            Self::ShowHover { needle, .. } => format!("show-hover:{needle}"),
            Self::ScrollHoverTimed { duration_secs } => {
                format!("scroll-hover-timed:{duration_secs}s")
            }
            Self::ClearHover => "clear-hover".to_string(),
            Self::SetProjectSearchQuery(query) => format!("project-search-query:{query}"),
            Self::RunProjectSearch => "run-project-search".to_string(),
            Self::WaitProjectSearch => "wait-project-search".to_string(),
            Self::JumpFirstProjectSearchMatch => "jump-first-project-search-match".to_string(),
            Self::WaitGit => "wait-git".to_string(),
            Self::ToggleGitGraph => "toggle-git-graph".to_string(),
            Self::WaitGitGraph => "wait-git-graph".to_string(),
            Self::LoadGitGraph { min_commits } => format!("load-git-graph:{min_commits}"),
            Self::ScrollGitGraphTimed { duration_secs } => {
                format!("scroll-git-graph-timed:{duration_secs}s")
            }
            Self::WaitTerminal => "wait-terminal".to_string(),
            Self::RunTerminalHtop => "run-terminal-htop".to_string(),
            Self::WaitTerminalHtopVisible => "wait-terminal-htop-visible".to_string(),
            Self::InterruptTerminal => "interrupt-terminal".to_string(),
            Self::WaitTerminalHtopExit => "wait-terminal-htop-exit".to_string(),
            Self::RunTerminalBasicCommand => "run-terminal-basic-command".to_string(),
            Self::WaitTerminalBasicCommandVisible => {
                "wait-terminal-basic-command-visible".to_string()
            }
            Self::ImportApiSpec => "import-api-spec".to_string(),
            Self::WaitApiSpec => "wait-api-spec".to_string(),
            Self::WaitApiRoutesPanel => "wait-api-routes-panel".to_string(),
            Self::ScrollApiRoutesTimed { duration_secs } => {
                format!("scroll-api-routes-timed:{duration_secs}s")
            }
            Self::ResetApiPanelScroll => "reset-api-panel-scroll".to_string(),
            Self::SetApiRouteFilter(needle) => format!("set-api-route-filter:{needle}"),
            Self::WaitApiRouteFilter(needle) => format!("wait-api-route-filter:{needle}"),
            Self::OpenApiRouteMatching(needle) => format!("open-api-route:{needle}"),
            Self::WaitApiRouteOpen(needle) => format!("wait-api-route-open:{needle}"),
            Self::ScrollApiTabTimed { duration_secs } => {
                format!("scroll-api-tab-timed:{duration_secs}s")
            }
            Self::OpenApiAuth => "open-api-auth".to_string(),
            Self::FocusApiAuth(scheme) => format!("focus-api-auth:{scheme}"),
            Self::SetApiAuthValue { scheme, .. } => format!("set-api-auth-value:{scheme}"),
            Self::SaveApiAuth { scheme, .. } => format!("save-api-auth:{scheme}"),
            Self::StartApiRequest => "start-api-request".to_string(),
            Self::WaitApiResponse {
                expected_status, ..
            } => {
                format!("wait-api-response:{expected_status}")
            }
            Self::ResetApiTabScroll => "reset-api-tab-scroll".to_string(),
            Self::ClearApiRouteFilter => "clear-api-route-filter".to_string(),
            Self::ShowSettings(show) => format!("show-settings:{show}"),
            Self::SetSettingsTab(tab) => format!("settings-tab:{tab}"),
            Self::AddSettingsIgnore(pattern) => format!("add-settings-ignore:{pattern}"),
            Self::RemoveSettingsIgnore(pattern) => format!("remove-settings-ignore:{pattern}"),
            Self::RefreshSettingsTools => "refresh-settings-tools".to_string(),
            Self::Finish => "finish".to_string(),
        }
    }

    fn timeout(&self) -> Duration {
        match self {
            Self::WaitReady | Self::WaitFileTree => Duration::from_secs(45),
            Self::WaitHighlight => Duration::from_secs(90),
            Self::WaitProjectSearch | Self::WaitGit | Self::WaitGitGraph => Duration::from_secs(60),
            Self::LoadGitGraph { .. } => Duration::from_secs(120),
            Self::WaitTerminal
            | Self::WaitTerminalHtopVisible
            | Self::WaitTerminalHtopExit
            | Self::WaitTerminalBasicCommandVisible
            | Self::WaitApiSpec
            | Self::WaitApiRoutesPanel
            | Self::WaitApiRouteFilter(_)
            | Self::WaitApiRouteOpen(_)
            | Self::WaitApiResponse { .. }
            | Self::TriggerAutocomplete(_)
            | Self::ShowHover { .. } => Duration::from_secs(30),
            Self::ScrollEditorTimed { duration_secs }
            | Self::ScrollGitGraphTimed { duration_secs }
            | Self::ScrollApiRoutesTimed { duration_secs }
            | Self::ScrollApiTabTimed { duration_secs }
            | Self::ScrollHoverTimed { duration_secs } => {
                Duration::from_secs(u64::from(*duration_secs) + 5)
            }
            _ => Duration::from_secs(12),
        }
    }

    fn optional(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutomationTick {
    Running,
    Exit,
}

pub struct AutomationController {
    options: AutomationOptions,
    steps: Vec<AutomationStep>,
    step_index: usize,
    step_progress: u32,
    started_at: Instant,
    step_started_at: Instant,
    completed: Vec<String>,
    skipped: Vec<String>,
    failure: Option<String>,
    report_written: bool,
}

impl AutomationController {
    pub fn new(options: AutomationOptions) -> Self {
        let now = Instant::now();
        let steps = full_pgo_scenario(&options.workspace);
        Self {
            options,
            steps,
            step_index: 0,
            step_progress: 0,
            started_at: now,
            step_started_at: now,
            completed: Vec::with_capacity(256),
            skipped: Vec::with_capacity(8),
            failure: None,
            report_written: false,
        }
    }

    pub fn tick(
        &mut self,
        app: &mut App,
        event_loop: &ActiveEventLoop,
        now: Instant,
    ) -> AutomationTick {
        if now.saturating_duration_since(self.started_at) > self.options.timeout {
            return self.fail_and_exit(format!(
                "global timeout after {:.1}s at {}",
                self.options.timeout.as_secs_f32(),
                self.current_step_name()
            ));
        }

        let Some(step) = self.steps.get(self.step_index).cloned() else {
            return self.finish_and_exit();
        };
        if now.saturating_duration_since(self.step_started_at) > step.timeout() {
            let mut message = format!(
                "step timeout after {:.1}s: {}",
                step.timeout().as_secs_f32(),
                step.name()
            );
            if matches!(
                step,
                AutomationStep::ShowHover { .. } | AutomationStep::ScrollHoverTimed { .. }
            ) {
                message.push_str("; ");
                message.push_str(&hover_state_diagnostics());
            }
            if step.optional() {
                println!("PGO_AUTOMATION_SKIP {message}");
                self.skipped.push(message);
                self.advance(step.name(), now);
                return AutomationTick::Running;
            }
            return self.fail_and_exit(message);
        }

        let result = self.run_step(app, event_loop, &step, now);
        match result {
            StepResult::Pending => AutomationTick::Running,
            StepResult::Done => {
                self.advance(step.name(), now);
                AutomationTick::Running
            }
            StepResult::Failed(message) if step.optional() => {
                println!("PGO_AUTOMATION_SKIP {message}");
                self.skipped.push(message);
                self.advance(step.name(), now);
                AutomationTick::Running
            }
            StepResult::Failed(message) => self.fail_and_exit(message),
            StepResult::Exit => self.finish_and_exit(),
        }
    }

    fn run_step(
        &mut self,
        app: &mut App,
        _event_loop: &ActiveEventLoop,
        step: &AutomationStep,
        now: Instant,
    ) -> StepResult {
        match step {
            AutomationStep::WaitReady => {
                if app.is_ready && app.is_ide_mode && app.window.is_some() && app.renderer.is_some()
                {
                    StepResult::Done
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::ResizeWindow { width, height } => {
                let Some(window) = app.window.as_ref() else {
                    return StepResult::Pending;
                };
                let _ = window.request_inner_size(PhysicalSize::new(*width, *height));
                window.request_redraw();
                StepResult::Done
            }
            AutomationStep::ApplyWorkspace => {
                if !self.options.workspace.is_dir() {
                    return StepResult::Failed(format!(
                        "automation workspace does not exist: {}",
                        self.options.workspace.display()
                    ));
                }
                if let Err(error) = ensure_fixture_repository(&self.options.workspace) {
                    return StepResult::Failed(error);
                }
                app.apply_selected_workspace_folder(self.options.workspace.clone());
                StepResult::Done
            }
            AutomationStep::WaitFileTree => {
                if app.file_tree_rx.is_none() && !app.ide_panel.file_tree_nodes.is_empty() {
                    StepResult::Done
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::WaitFrames(frames) => {
                self.step_progress = self.step_progress.saturating_add(1);
                if self.step_progress >= u32::from(*frames) {
                    StepResult::Done
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::WaitMillis(millis) => {
                if now.saturating_duration_since(self.step_started_at)
                    >= Duration::from_millis(*millis)
                {
                    StepResult::Done
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::OpenPanel(panel) => {
                open_panel_semantic(app, *panel);
                if app.ide_panel.is_open(*panel) {
                    StepResult::Done
                } else {
                    StepResult::Failed(format!("panel did not open: {panel:?}"))
                }
            }
            AutomationStep::ExpandWorkspaceRoot => {
                let node_idx = app
                    .ide_panel
                    .file_tree_nodes
                    .iter()
                    .position(|node| node.is_dir && node.path == self.options.workspace)
                    .or_else(|| {
                        app.ide_panel
                            .file_tree_nodes
                            .iter()
                            .position(|node| node.is_dir)
                    });
                let Some(node_idx) = node_idx else {
                    return StepResult::Pending;
                };
                if !app.ide_panel.file_tree_nodes[node_idx].is_expanded {
                    app.toggle_file_tree_dir(node_idx);
                }
                StepResult::Done
            }
            AutomationStep::OpenFile(relative) => {
                let path = self.options.workspace.join(relative);
                if !path.is_file() {
                    return StepResult::Failed(format!(
                        "fixture file is missing: {}",
                        path.display()
                    ));
                }
                app.open_file_in_tab(path, false);
                StepResult::Done
            }
            AutomationStep::SwitchToFile(relative) => {
                let target = self.options.workspace.join(relative);
                let Some(index) = app.tabs.iter().position(|tab| {
                    tab.file_path
                        .as_ref()
                        .is_some_and(|path| crate::platform::paths_equal(path, &target))
                }) else {
                    return StepResult::Failed(format!("open tab not found: {}", target.display()));
                };
                app.switch_to_tab(index);
                StepResult::Done
            }
            AutomationStep::WaitHighlight => {
                if app.is_highlight_complete || app.highlighter.is_complete {
                    StepResult::Done
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::FocusEditor => {
                focus_main_editor_semantic(app);
                StepResult::Done
            }
            AutomationStep::TypeText(text) => {
                app.handle_main_ime_commit(text);
                StepResult::Done
            }
            AutomationStep::SaveCurrentFile => {
                if app.save_current_file() {
                    StepResult::Done
                } else {
                    StepResult::Failed("saving the automation fixture failed".to_string())
                }
            }
            AutomationStep::OpenActiveTabContext => {
                if app.open_tab_context_menu(app.active_tab, 96.0, 64.0) {
                    StepResult::Done
                } else {
                    StepResult::Failed("active tab context menu did not open".to_string())
                }
            }
            AutomationStep::OpenFileTreeContext => open_file_tree_context_semantic(app),
            AutomationStep::CloseContextMenu => {
                app.ide_panel.file_tree_context_menu = None;
                StepResult::Done
            }
            AutomationStep::OpenSearch => {
                app.show_search = true;
                app.search_focused = true;
                app.search_editor.select_all();
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::SetSearchQuery(query) => {
                app.search_editor.set_text_clean(query);
                app.search_editor.cursor = query.len();
                app.search_focused = true;
                app.update_search();
                app.jump_to_search_result();
                request_redraw(app);
                if app.search_results.is_empty() {
                    StepResult::Failed(format!("editor search returned no results: {query}"))
                } else {
                    StepResult::Done
                }
            }
            AutomationStep::ToggleSearchCase => {
                app.search_case_sensitive = !app.search_case_sensitive;
                app.update_search();
                app.jump_to_search_result();
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::NextSearchResult => {
                if app.search_results.is_empty() {
                    return StepResult::Failed("editor search results disappeared".to_string());
                }
                let next = app
                    .search_current_idx
                    .map_or(0, |idx| (idx + 1) % app.search_results.len());
                app.search_current_idx = Some(next);
                app.jump_to_search_result();
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::PreviousSearchResult => {
                if app.search_results.is_empty() {
                    return StepResult::Failed("editor search results disappeared".to_string());
                }
                let previous = app.search_current_idx.map_or(0, |idx| {
                    if idx == 0 {
                        app.search_results.len() - 1
                    } else {
                        idx - 1
                    }
                });
                app.search_current_idx = Some(previous);
                app.jump_to_search_result();
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::CloseSearch => {
                app.show_search = false;
                app.search_focused = false;
                app.search_results.clear();
                app.search_current_idx = None;
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::ScrollEditorTimed { duration_secs } => {
                self.timed_scroll(app, now, *duration_secs, |app, direction| {
                    let max_scroll = app.renderer.as_ref().map_or(0.0, |renderer| {
                        (app.editor.line_offsets.len() as f32 * renderer.line_height
                            - renderer.height)
                            .max(0.0)
                    });
                    app.scroll_y.scroll_by(36.0 * direction);
                    app.scroll_y.clamp_target(0.0, max_scroll);
                })
            }
            AutomationStep::JumpMinimap(fraction) => {
                let Some(renderer) = app.renderer.as_mut() else {
                    return StepResult::Pending;
                };
                let height = app
                    .window
                    .as_ref()
                    .map_or(renderer.height, |window| window.inner_size().height as f32);
                let max_scroll = renderer.get_max_scroll(&app.editor, height);
                let target = (max_scroll * fraction.clamp(0.0, 1.0)).round();
                app.scroll_y.jump_to(target);
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::ToggleFirstFold => {
                let Some(line) = app.editor.foldable_lines.keys().copied().min() else {
                    return StepResult::Pending;
                };
                if !app.editor.folded_lines.remove(&line) {
                    app.editor.folded_lines.insert(line);
                    if let Some(offset) = app.editor.line_offsets.get(line).copied() {
                        app.editor.folded_start_bytes.insert(offset);
                    }
                } else if let Some(offset) = app.editor.line_offsets.get(line).copied() {
                    app.editor.folded_start_bytes.remove(&offset);
                }
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::SetEditorCursorAfter(needle) => {
                let text = app.editor.get_full_text();
                let Some(offset) = text.rfind(needle).map(|offset| offset + needle.len()) else {
                    return StepResult::Failed(format!("editor marker not found: {needle}"));
                };
                app.editor.cursor = offset;
                app.editor.selection_anchor = None;
                focus_main_editor_semantic(app);
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::TriggerAutocomplete(expected) => {
                app.update_autocomplete();
                if app.autocomplete_active
                    && app
                        .autocomplete_options
                        .iter()
                        .any(|(item, _)| item.word == *expected)
                {
                    request_redraw(app);
                    StepResult::Done
                } else if app.is_highlight_complete || app.highlighter.is_complete {
                    StepResult::Failed(autocomplete_failure_diagnostics(app, expected))
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::SelectAutocomplete(expected) => {
                let Some(index) = app
                    .autocomplete_options
                    .iter()
                    .position(|(item, _)| item.word == *expected)
                else {
                    return StepResult::Pending;
                };
                app.autocomplete_selected_idx = index;
                app.ensure_autocomplete_visible();
                app.request_active_autocomplete_detail_for_index(index);
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::ApplyAutocomplete(expected) => {
                app.apply_autocomplete();
                if !app.autocomplete_active && app.editor.get_full_text().contains(expected) {
                    StepResult::Done
                } else {
                    StepResult::Failed(format!("completion was not applied: {expected}"))
                }
            }
            AutomationStep::ShowHover { needle, text } => show_hover_semantic(app, needle, text),
            AutomationStep::ScrollHoverTimed { duration_secs } => {
                let hover_ready = crate::app::mouse::HOVER_STATE.with(|state| {
                    let state = state.borrow();
                    state.popup.is_some() && state.rect.is_some()
                });
                if !hover_ready {
                    return StepResult::Failed(format!(
                        "hover popup disappeared before scrolling; {}",
                        hover_state_diagnostics()
                    ));
                }
                self.timed_scroll(app, now, *duration_secs, |_app, direction| {
                    crate::app::mouse::HOVER_STATE.with(|state| {
                        let mut state = state.borrow_mut();
                        let max_scroll = state.max_scroll.max(480.0);
                        if let Some(popup) = state.popup.as_mut() {
                            popup.scroll.scroll_by(24.0 * direction);
                            popup.scroll.clamp_target(0.0, max_scroll);
                        }
                    });
                })
            }
            AutomationStep::ClearHover => {
                crate::app::mouse::HOVER_STATE.with(|state| {
                    *state.borrow_mut() = crate::app::mouse::HoverState::default();
                });
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::SetProjectSearchQuery(query) => {
                open_panel_semantic(app, PanelId::Search);
                app.ide_panel
                    .project_search
                    .query_editor
                    .set_text_clean(query);
                app.ide_panel.project_search.query_editor.cursor = query.len();
                app.ide_panel.project_search.focused =
                    Some(crate::app::project_search::ProjectSearchField::Query);
                app.ide_panel.project_search.dirty = true;
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::RunProjectSearch => {
                app.start_project_search();
                StepResult::Done
            }
            AutomationStep::WaitProjectSearch => {
                let search = &app.ide_panel.project_search;
                if search.has_run && search.running_generation.is_none() && search.rx.is_none() {
                    if let Some(error) = &search.error {
                        StepResult::Failed(format!("project search failed: {error}"))
                    } else if search.total_matches == 0 {
                        StepResult::Failed(
                            "project search completed without the fixture marker".to_string(),
                        )
                    } else {
                        StepResult::Done
                    }
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::JumpFirstProjectSearchMatch => {
                if app
                    .ide_panel
                    .project_search
                    .results
                    .first()
                    .is_some_and(|file| !file.matches.is_empty())
                {
                    app.handle_project_search_match_click(0, 0);
                    StepResult::Done
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::WaitGit => {
                if !app.ide_panel.is_open(PanelId::Git) {
                    return StepResult::Failed("Git panel did not open".to_string());
                }
                if self.step_progress == 0 {
                    app.refresh_git_panel();
                    self.step_progress = 1;
                    return StepResult::Pending;
                }
                let git = &app.ide_panel.git;
                if !git.pending && !git.status_loading() {
                    if git.snapshot.workspaces.is_empty() {
                        StepResult::Failed(
                            "Git panel did not discover the fixture repository".to_string(),
                        )
                    } else {
                        StepResult::Done
                    }
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::ToggleGitGraph => {
                if !app.ide_panel.git.graph_open() {
                    app.toggle_git_graph();
                }
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::WaitGitGraph => {
                let git = &app.ide_panel.git;
                if !git.graph_pending {
                    if git.graph_snapshot.is_empty() {
                        StepResult::Failed("Git graph produced no commits".to_string())
                    } else {
                        StepResult::Done
                    }
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::LoadGitGraph { min_commits } => {
                if app.ide_panel.git.graph_pending {
                    return StepResult::Pending;
                }
                let loaded = app.ide_panel.git.graph_snapshot.len();
                if loaded >= *min_commits {
                    return StepResult::Done;
                }
                if !app.ide_panel.git.graph_has_more {
                    return StepResult::Failed(format!(
                        "Git graph ended at {loaded} commits; expected at least {min_commits}"
                    ));
                }
                app.load_more_git_graph_commits();
                StepResult::Pending
            }
            AutomationStep::ScrollGitGraphTimed { duration_secs } => {
                self.timed_scroll(app, now, *duration_secs, |app, direction| {
                    let scale = app
                        .renderer
                        .as_ref()
                        .map_or(1.0, |renderer| renderer.scale_factor);
                    let view_h = app.renderer.as_ref().map_or(600.0 * scale, |renderer| {
                        (renderer.height * 0.55).max(240.0 * scale)
                    });
                    let max_scroll = crate::app::git_panel::git_graph_max_scroll(
                        app.ide_panel.git.graph_snapshot.len(),
                        view_h,
                        scale,
                    );
                    app.ide_panel.git.graph_scroll.anim_speed = 7.0;
                    app.ide_panel.git.graph_scroll.scroll_by(120.0 * direction);
                    app.ide_panel.git.graph_scroll.clamp_target(0.0, max_scroll);
                })
            }
            AutomationStep::WaitTerminal => {
                if app.ide_panel.is_open(PanelId::Terminal) && !app.ide_panel.terminals.is_empty() {
                    StepResult::Done
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::RunTerminalHtop => write_terminal_semantic(app, b"htop\r"),
            AutomationStep::WaitTerminalHtopVisible => match active_terminal_tui_state(app) {
                Some(state) if state.alternate_screen && state.non_blank_cells >= 20 => {
                    StepResult::Done
                }
                _ => StepResult::Pending,
            },
            AutomationStep::InterruptTerminal => write_terminal_semantic(app, b"\x03"),
            AutomationStep::WaitTerminalHtopExit => match active_terminal_tui_state(app) {
                Some(state) if !state.alternate_screen => StepResult::Done,
                _ => StepResult::Pending,
            },
            AutomationStep::RunTerminalBasicCommand => {
                write_terminal_semantic(app, terminal_basic_command())
            }
            AutomationStep::WaitTerminalBasicCommandVisible => {
                match active_terminal_contains(app, TERMINAL_BASIC_OUTPUT) {
                    Some(true) => StepResult::Done,
                    Some(false) | None => StepResult::Pending,
                }
            }
            AutomationStep::ImportApiSpec => {
                let path = self.options.workspace.join("openapi.json");
                if !path.is_file() {
                    return StepResult::Failed(format!(
                        "OpenAPI fixture is missing: {}",
                        path.display()
                    ));
                }
                app.start_api_local_import(path);
                StepResult::Done
            }
            AutomationStep::WaitApiSpec => {
                let api = &app.ide_panel.api;
                if api.loading.is_empty() && !api.specs.is_empty() && !api.models.is_empty() {
                    StepResult::Done
                } else if api.loading.is_empty() && api.import_error.is_some() {
                    StepResult::Failed(format!(
                        "OpenAPI import failed: {}",
                        api.import_error.as_deref().unwrap_or("unknown error")
                    ))
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::WaitApiRoutesPanel => {
                let Some(model) = app.ide_panel.api.selected_model() else {
                    return StepResult::Pending;
                };
                if model.routes.is_empty() {
                    StepResult::Failed("OpenAPI import completed without routes".to_string())
                } else if app.ide_panel.is_open(PanelId::ApiClient) {
                    StepResult::Done
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::ScrollApiRoutesTimed { duration_secs } => {
                self.timed_scroll(app, now, *duration_secs, |app, direction| {
                    let scale = app
                        .renderer
                        .as_ref()
                        .map_or(1.0, |renderer| renderer.scale_factor);
                    let visible_h = app
                        .renderer
                        .as_ref()
                        .map_or(720.0, |renderer| renderer.height);
                    let max_scroll = crate::app::api_client::api_panel_max_scroll(
                        &app.ide_panel.api,
                        visible_h,
                        scale,
                    );
                    app.ide_panel.api.panel_scroll.scroll_by(72.0 * direction);
                    app.ide_panel.api.panel_scroll.clamp_target(0.0, max_scroll);
                })
            }
            AutomationStep::ResetApiPanelScroll => {
                reset_scroll(&mut app.ide_panel.api.panel_scroll);
                StepResult::Done
            }
            AutomationStep::SetApiRouteFilter(needle) => {
                app.focus_api_input(ApiFocus::RouteFilter);
                app.ide_panel.api.input_editor.set_text_clean(needle);
                app.ide_panel.api.input_editor.cursor = needle.len();
                app.commit_api_focus();
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::WaitApiRouteFilter(needle) => {
                let Some(model) = app.ide_panel.api.selected_model() else {
                    return StepResult::Pending;
                };
                let matching = model.routes.iter().any(|route| {
                    route.operation_id.contains(needle)
                        || route.summary.contains(needle)
                        || route.path.contains(needle)
                });
                if !matching {
                    StepResult::Failed(format!("OpenAPI fixture route not found: {needle}"))
                } else if app.ide_panel.api.route_filter.contains(needle) {
                    StepResult::Done
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::OpenApiRouteMatching(needle) => {
                let Some(model) = app.ide_panel.api.selected_model() else {
                    return StepResult::Pending;
                };
                let spec_id = model.id;
                let route_idx = model.routes.iter().position(|route| {
                    route.operation_id.contains(needle)
                        || route.summary.contains(needle)
                        || route.path.contains(needle)
                });
                let Some(route_idx) = route_idx else {
                    return StepResult::Failed(format!(
                        "OpenAPI fixture route not found: {needle}"
                    ));
                };
                app.commit_api_focus();
                app.ide_panel.api.focused = None;
                app.open_api_route(spec_id, route_idx);
                if app.active_api_tab().is_some_and(|(meta, state)| {
                    meta.spec_id == spec_id && state.route_idx == Some(route_idx)
                }) {
                    StepResult::Done
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::WaitApiRouteOpen(needle) => {
                let opened = app.active_api_tab().is_some_and(|(meta, state)| {
                    state.route_idx.is_some()
                        && (meta.route_path.contains(needle)
                            || app
                                .ide_panel
                                .api
                                .models
                                .get(&meta.spec_id)
                                .and_then(|model| {
                                    state.route_idx.and_then(|idx| model.routes.get(idx))
                                })
                                .is_some_and(|route| {
                                    route.operation_id.contains(needle)
                                        || route.summary.contains(needle)
                                }))
                });
                if opened {
                    StepResult::Done
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::ScrollApiTabTimed { duration_secs } => {
                self.timed_scroll(app, now, *duration_secs, |app, direction| {
                    scroll_active_api_tab(app, 72.0 * direction)
                })
            }
            AutomationStep::OpenApiAuth => {
                let Some((meta, _)) = app.active_api_tab() else {
                    return StepResult::Pending;
                };
                let spec_id = meta.spec_id;
                app.open_api_auth_tab(spec_id);
                if app.active_api_tab().is_some_and(|(active_meta, state)| {
                    active_meta.spec_id == spec_id && state.auth_view
                }) {
                    StepResult::Done
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::FocusApiAuth(scheme) => {
                let Some((meta, _)) = app.active_api_tab() else {
                    return StepResult::Pending;
                };
                let spec_id = meta.spec_id;
                let Some(_scheme_idx) = app.ide_panel.api.models.get(&spec_id).and_then(|model| {
                    model
                        .security_schemes
                        .iter()
                        .position(|candidate| candidate.name == *scheme)
                }) else {
                    return StepResult::Failed(format!("OpenAPI auth scheme not found: {scheme}"));
                };
                app.focus_api_input(ApiFocus::AuthValue {
                    spec_id,
                    scheme: (*scheme).to_string(),
                });
                if matches!(
                    app.ide_panel.api.focused,
                    Some(ApiFocus::AuthValue { spec_id: focused_spec, scheme: ref focused_scheme })
                        if focused_spec == spec_id && focused_scheme == *scheme
                ) {
                    StepResult::Done
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::SetApiAuthValue { scheme, value } => {
                let Some((meta, state)) = app.active_api_tab() else {
                    return StepResult::Pending;
                };
                if !state.auth_view {
                    return StepResult::Failed("OpenAPI auth tab is not active".to_string());
                }
                let spec_id = meta.spec_id;
                let focus = ApiFocus::AuthValue {
                    spec_id,
                    scheme: (*scheme).to_string(),
                };
                if app.ide_panel.api.focused.as_ref() != Some(&focus) {
                    app.focus_api_input(focus);
                }
                app.ide_panel.api.input_editor.set_text_clean(value);
                app.ide_panel.api.input_editor.cursor = app.ide_panel.api.input_editor.len();
                app.ide_panel.api.input_editor.selection_anchor = None;
                app.ide_panel.api.input_editor.sync_edits.clear();
                request_redraw(app);
                let actual = app.ide_panel.api.input_editor.get_full_text();
                if actual == *value {
                    StepResult::Done
                } else {
                    StepResult::Failed(format!(
                        "OpenAPI auth editor did not accept value for scheme {scheme}; expected_len={} actual_len={} focus={:?}",
                        value.len(),
                        actual.len(),
                        app.ide_panel.api.focused
                    ))
                }
            }
            AutomationStep::SaveApiAuth { scheme, value } => {
                let Some((meta, _)) = app.active_api_tab() else {
                    return StepResult::Pending;
                };
                let spec_id = meta.spec_id;
                let Some(_scheme_idx) = app.ide_panel.api.models.get(&spec_id).and_then(|model| {
                    model
                        .security_schemes
                        .iter()
                        .position(|candidate| candidate.name == *scheme)
                }) else {
                    return StepResult::Failed(format!("OpenAPI auth scheme not found: {scheme}"));
                };
                let editor_value = app.ide_panel.api.input_editor.get_full_text();
                app.ide_panel
                    .api
                    .auth
                    .set_value(spec_id, scheme, editor_value.clone());
                app.ide_panel.api.focused = None;
                app.ide_panel.api.persist();
                if app
                    .ide_panel
                    .api
                    .auth
                    .entry(spec_id, scheme)
                    .is_some_and(|entry| entry.value == *value)
                {
                    StepResult::Done
                } else {
                    let saved = app
                        .ide_panel
                        .api
                        .auth
                        .entry(spec_id, scheme)
                        .map(|entry| {
                            format!(
                                "value_len={} token_type={:?}",
                                entry.value.len(),
                                entry.token_type
                            )
                        })
                        .unwrap_or_else(|| "missing entry".to_string());
                    StepResult::Failed(format!(
                        "OpenAPI auth value was not saved for scheme {scheme}; expected_len={} editor_len={} saved={saved}",
                        value.len(),
                        editor_value.len()
                    ))
                }
            }
            AutomationStep::StartApiRequest => {
                app.start_active_api_request();
                let Some((_, state)) = app.active_api_tab() else {
                    return StepResult::Failed(
                        "API request started without an active API tab".to_string(),
                    );
                };
                if state.pending || state.pending_request_id.is_some() || state.response.is_some() {
                    StepResult::Done
                } else {
                    StepResult::Failed("API request did not enter pending state".to_string())
                }
            }
            AutomationStep::WaitApiResponse {
                expected_status,
                body_marker,
            } => {
                let Some((_, state)) = app.active_api_tab() else {
                    return StepResult::Pending;
                };
                if state.pending || state.pending_request_id.is_some() {
                    return StepResult::Pending;
                }
                let Some(response) = state.response.as_ref() else {
                    return StepResult::Pending;
                };
                if let Some(error) = response.error.as_ref() {
                    return StepResult::Failed(format!(
                        "local API request failed: {:?}: {}",
                        error.kind, error.message
                    ));
                }
                if response.status != Some(*expected_status) {
                    return StepResult::Failed(format!(
                        "local API request returned {:?}, expected {expected_status}; body={:?}",
                        response.status, response.body
                    ));
                }
                if !response.body.contains(body_marker) {
                    return StepResult::Failed(format!(
                        "local API response is missing marker {body_marker:?}; body={:?}",
                        response.body
                    ));
                }
                StepResult::Done
            }
            AutomationStep::ResetApiTabScroll => {
                let Some(active_tab) = app.tabs.get_mut(app.active_tab) else {
                    return StepResult::Pending;
                };
                let crate::app::EditorTabKind::ApiClient(_, state) = &mut active_tab.kind else {
                    return StepResult::Pending;
                };
                reset_scroll(&mut state.tab_scroll);
                StepResult::Done
            }
            AutomationStep::ClearApiRouteFilter => {
                app.ide_panel.api.route_filter.clear();
                if matches!(app.ide_panel.api.focused, Some(ApiFocus::RouteFilter)) {
                    app.ide_panel.api.input_editor.set_text_clean("");
                    app.ide_panel.api.input_editor.cursor = 0;
                    app.ide_panel.api.input_editor.selection_anchor = None;
                }
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::ShowSettings(show) => {
                app.show_settings = *show;
                app.settings_anim_progress = if *show { 0.0 } else { 1.0 };
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::SetSettingsTab(tab) => {
                app.settings_tab = *tab;
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::AddSettingsIgnore(pattern) => {
                if !app
                    .ide_ignore_patterns
                    .iter()
                    .any(|candidate| candidate == pattern)
                {
                    app.ide_ignore_patterns.push((*pattern).to_string());
                    app.save_current_config();
                    app.refresh_file_tree();
                    request_redraw(app);
                }
                if app
                    .ide_ignore_patterns
                    .iter()
                    .any(|candidate| candidate == pattern)
                {
                    StepResult::Done
                } else {
                    StepResult::Failed(format!("settings ignore pattern was not added: {pattern}"))
                }
            }
            AutomationStep::RemoveSettingsIgnore(pattern) => {
                let Some(index) = app
                    .ide_ignore_patterns
                    .iter()
                    .position(|candidate| candidate == pattern)
                else {
                    return StepResult::Failed(format!(
                        "settings ignore pattern not found: {pattern}"
                    ));
                };
                app.ide_ignore_patterns.remove(index);
                app.save_current_config();
                app.refresh_file_tree();
                request_redraw(app);
                if app
                    .ide_ignore_patterns
                    .iter()
                    .all(|candidate| candidate != pattern)
                {
                    StepResult::Done
                } else {
                    StepResult::Failed(format!(
                        "settings ignore pattern was not removed: {pattern}"
                    ))
                }
            }
            AutomationStep::RefreshSettingsTools => {
                crate::platform::refresh_tool_resolutions();
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::Finish => StepResult::Exit,
        }
    }

    fn timed_scroll(
        &mut self,
        app: &mut App,
        now: Instant,
        duration_secs: u16,
        mut impulse: impl FnMut(&mut App, f32),
    ) -> StepResult {
        let elapsed = now
            .saturating_duration_since(self.step_started_at)
            .as_secs_f32();
        let plan = timed_scroll_plan(elapsed, duration_secs);
        if plan.done {
            return StepResult::Done;
        }
        while self.step_progress < plan.expected_impulses {
            impulse(app, plan.direction);
            self.step_progress = self.step_progress.saturating_add(1);
        }
        StepResult::Pending
    }

    fn advance(&mut self, name: String, now: Instant) {
        println!(
            "PGO_AUTOMATION_STEP index={} name={} status=ok",
            self.step_index, name
        );
        self.completed.push(name);
        self.step_index += 1;
        self.step_progress = 0;
        self.step_started_at = now;
    }

    fn current_step_name(&self) -> String {
        self.steps
            .get(self.step_index)
            .map(AutomationStep::name)
            .unwrap_or_else(|| "complete".to_string())
    }

    fn fail_and_exit(&mut self, message: String) -> AutomationTick {
        eprintln!("PGO_AUTOMATION_FAILED {message}");
        self.failure = Some(message);
        self.write_report("failed");
        AutomationTick::Exit
    }

    fn finish_and_exit(&mut self) -> AutomationTick {
        self.write_report("success");
        println!(
            "PGO_AUTOMATION_DONE completed={} skipped={} duration_ms={}",
            self.completed.len(),
            self.skipped.len(),
            self.started_at.elapsed().as_millis()
        );
        AutomationTick::Exit
    }

    fn write_interrupted_report(&mut self, reason: &str) {
        if self.report_written {
            return;
        }
        let message = format!(
            "{reason}; current_step={} index={}",
            self.current_step_name(),
            self.step_index
        );
        eprintln!("PGO_AUTOMATION_INTERRUPTED {message}");
        self.failure = Some(message);
        self.write_report("failed");
    }

    fn write_report(&mut self, status: &str) {
        if self.report_written {
            return;
        }
        self.report_written = true;
        let report = json!({
            "status": status,
            "scenario_version": PGO_AUTOMATION_SCENARIO_VERSION,
            "driver": "semantic-internal-actions",
            "platform": std::env::consts::OS,
            "architecture": std::env::consts::ARCH,
            "workspace": self.options.workspace,
            "fixture_git_commits": GIT_FIXTURE_COMMIT_COUNT,
            "fixture_git_feature_branches": GIT_FIXTURE_BRANCH_COUNT,
            "fixture_python_files": fixture_python_tests(&self.options.workspace).len() + 1,
            "duration_ms": self.started_at.elapsed().as_millis(),
            "completed_steps": self.completed,
            "skipped_steps": self.skipped,
            "failed_step": self.failure,
        });
        if let Some(parent) = self.options.report_path.parent() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                eprintln!(
                    "PGO_AUTOMATION_REPORT_ERROR create {}: {error}",
                    parent.display()
                );
                return;
            }
        }
        match serde_json::to_vec_pretty(&report)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                std::fs::write(&self.options.report_path, bytes).map_err(|error| error.to_string())
            }) {
            Ok(()) => println!(
                "PGO_AUTOMATION_REPORT {}",
                self.options.report_path.display()
            ),
            Err(error) => eprintln!(
                "PGO_AUTOMATION_REPORT_ERROR write {}: {error}",
                self.options.report_path.display()
            ),
        }
    }
}

impl App {
    #[cold]
    #[inline(never)]
    pub(crate) fn advance_automation(
        &mut self,
        event_loop: &ActiveEventLoop,
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

    pub(crate) fn write_interrupted_automation_report(&mut self, reason: &str) {
        let Some(mut automation) = self.automation.take() else {
            return;
        };
        automation.write_interrupted_report(reason);
        self.automation = Some(automation);
    }
}

#[derive(Debug)]
enum StepResult {
    Pending,
    Done,
    Failed(String),
    Exit,
}

fn request_redraw(app: &App) {
    if let Some(window) = app.window.as_ref() {
        window.request_redraw();
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
    request_redraw(app);
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
    request_redraw(app);
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

fn hover_state_diagnostics() -> String {
    crate::app::mouse::HOVER_STATE.with(|state| {
        let state = state.borrow();
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
    })
}

fn show_hover_semantic(app: &mut App, needle: &str, text: &str) -> StepResult {
    let editor_text = app.editor.get_full_text();
    let Some(byte_offset) = editor_text.find(needle) else {
        return StepResult::Failed(format!("hover target was not found: {needle}"));
    };

    let (has_matching_popup, has_drawn_rect) = crate::app::mouse::HOVER_STATE
        .with(|state| hover_popup_status(&state.borrow(), byte_offset));
    if has_drawn_rect {
        return StepResult::Done;
    }
    if has_matching_popup {
        request_redraw(app);
        return StepResult::Pending;
    }

    let scale = app
        .renderer
        .as_ref()
        .map_or(1.0, |renderer| renderer.scale_factor);
    let anchor = (340.0 * scale, 180.0 * scale);
    let mut popup = crate::app::events::source_hover_popup_for_editor(
        &app.editor,
        byte_offset,
        text.to_string(),
        Some("tests.pgo_completion_hover"),
        anchor,
    );
    popup.anim_progress = 1.0;
    crate::app::mouse::HOVER_STATE.with(|state| {
        let mut state = state.borrow_mut();
        *state = crate::app::mouse::HoverState::default();
        state.byte_offset = Some(byte_offset);
        state.timer = 1.0;
        state.popup = Some(popup);
    });
    println!(
        "PGO_AUTOMATION_HOVER installed target={needle} byte_offset={byte_offset} {}",
        hover_state_diagnostics()
    );
    request_redraw(app);
    StepResult::Pending
}

fn scroll_active_api_tab(app: &mut App, delta: f32) {
    let scale = app
        .renderer
        .as_ref()
        .map_or(1.0, |renderer| renderer.scale_factor);
    let visible_h = app
        .renderer
        .as_ref()
        .map_or(720.0, |renderer| renderer.height);
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

fn fixture_commit(
    repository: &git2::Repository,
    tree: &git2::Tree<'_>,
    parents: &[git2::Oid],
    message: &str,
    timestamp: i64,
) -> Result<git2::Oid, String> {
    let time = git2::Time::new(timestamp, 0);
    let signature = git2::Signature::new("RRiter PGO", "pgo@rriter.invalid", &time)
        .map_err(|error| format!("failed to create fixture Git signature: {error}"))?;
    let parent_commits = parents
        .iter()
        .map(|oid| repository.find_commit(*oid))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to load fixture Git parent: {error}"))?;
    let parent_refs = parent_commits.iter().collect::<Vec<_>>();
    repository
        .commit(None, &signature, &signature, message, tree, &parent_refs)
        .map_err(|error| format!("failed to create fixture Git commit: {error}"))
}

fn fixture_head_commit_count(repository: &git2::Repository) -> usize {
    let Ok(mut walk) = repository.revwalk() else {
        return 0;
    };
    if walk.push_head().is_err() {
        return 0;
    }
    walk.filter_map(Result::ok).count()
}

fn ensure_fixture_repository(workspace: &Path) -> Result<(), String> {
    if workspace.join(".git").exists() {
        let repository = git2::Repository::open(workspace)
            .map_err(|error| format!("failed to open fixture Git repository: {error}"))?;
        if fixture_head_commit_count(&repository) >= GIT_FIXTURE_COMMIT_COUNT {
            return Ok(());
        }
        return Err(
            "fixture Git repository exists but does not contain the 1000-commit graph".to_string(),
        );
    }

    let repository = git2::Repository::init(workspace)
        .map_err(|error| format!("failed to initialize fixture Git repository: {error}"))?;
    let mut index = repository
        .index()
        .map_err(|error| format!("failed to open fixture Git index: {error}"))?;
    index
        .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
        .map_err(|error| format!("failed to stage fixture files: {error}"))?;
    index
        .write()
        .map_err(|error| format!("failed to write fixture Git index: {error}"))?;
    let tree_id = index
        .write_tree()
        .map_err(|error| format!("failed to write fixture Git tree: {error}"))?;
    let tree = repository
        .find_tree(tree_id)
        .map_err(|error| format!("failed to load fixture Git tree: {error}"))?;

    let mut timestamp = 1_700_000_000i64;
    let root = fixture_commit(
        &repository,
        &tree,
        &[],
        "Create deterministic PGO fixture",
        timestamp,
    )?;
    let mut main_tip = root;
    for branch_index in 0..GIT_FIXTURE_BRANCH_COUNT {
        let branch_base = main_tip;
        for main_index in 0..13 {
            timestamp += 1;
            main_tip = fixture_commit(
                &repository,
                &tree,
                &[main_tip],
                &format!("main cycle {branch_index:02} commit {main_index:02}"),
                timestamp,
            )?;
        }
        let mut feature_tip = branch_base;
        for feature_index in 0..5 {
            timestamp += 1;
            feature_tip = fixture_commit(
                &repository,
                &tree,
                &[feature_tip],
                &format!("feature {branch_index:02} commit {feature_index:02}"),
                timestamp,
            )?;
        }
        repository
            .reference(
                &format!("refs/heads/feature-{branch_index:02}"),
                feature_tip,
                true,
                "RRiter PGO feature branch",
            )
            .map_err(|error| format!("failed to create fixture Git branch: {error}"))?;
        timestamp += 1;
        main_tip = fixture_commit(
            &repository,
            &tree,
            &[main_tip, feature_tip],
            &format!("merge feature-{branch_index:02}"),
            timestamp,
        )?;
    }
    for tail_index in 0..49 {
        timestamp += 1;
        main_tip = fixture_commit(
            &repository,
            &tree,
            &[main_tip],
            &format!("main tail commit {tail_index:02}"),
            timestamp,
        )?;
    }
    repository
        .reference("refs/heads/main", main_tip, true, "RRiter PGO main branch")
        .map_err(|error| format!("failed to update fixture Git main branch: {error}"))?;
    repository
        .set_head("refs/heads/main")
        .map_err(|error| format!("failed to set fixture Git HEAD: {error}"))?;

    let count = fixture_head_commit_count(&repository);
    if count != GIT_FIXTURE_COMMIT_COUNT {
        return Err(format!(
            "fixture Git graph has {count} commits, expected {GIT_FIXTURE_COMMIT_COUNT}"
        ));
    }
    Ok(())
}

fn fixture_python_tests(workspace: &Path) -> Vec<PathBuf> {
    let tests_dir = workspace.join("tests");
    let Ok(entries) = std::fs::read_dir(tests_dir) else {
        return Vec::new();
    };
    let mut files = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().and_then(|extension| extension.to_str()) == Some("py")
                && path.file_name().and_then(|name| name.to_str())
                    != Some("pgo_completion_hover.py")
        })
        .filter_map(|path| {
            let size = path.metadata().ok()?.len();
            Some((size, PathBuf::from("tests").join(path.file_name()?)))
        })
        .collect::<Vec<_>>();
    files.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    files.into_iter().map(|(_, path)| path).collect()
}

fn full_pgo_scenario(workspace: &Path) -> Vec<AutomationStep> {
    use AutomationStep as S;
    let mut steps = vec![
        S::WaitReady,
        S::ResizeWindow {
            width: 1280,
            height: 800,
        },
        S::ApplyWorkspace,
        S::WaitFileTree,
        S::OpenPanel(PanelId::Explorer),
        S::ExpandWorkspaceRoot,
        S::WaitFileTree,
        S::OpenFile(PathBuf::from("src/main.rs")),
        S::WaitHighlight,
        S::ToggleFirstFold,
        S::WaitMillis(350),
        S::ToggleFirstFold,
        S::WaitMillis(350),
        S::OpenFile(PathBuf::from("src/worker.py")),
        S::WaitHighlight,
        S::WaitMillis(350),
        S::OpenFile(PathBuf::from("README.md")),
        S::WaitHighlight,
        S::WaitMillis(350),
        S::SwitchToFile(PathBuf::from("src/main.rs")),
        S::WaitFrames(2),
        S::SwitchToFile(PathBuf::from("src/worker.py")),
        S::OpenActiveTabContext,
        S::WaitMillis(750),
        S::CloseContextMenu,
        S::FocusEditor,
        S::SetEditorCursorAfter("return sum(item.weight for item in items)"),
        S::TypeText("\n# RRITER_PGO_AUTOMATION_MARKER\nmessage = 'Привет PGO 🚀'\n"),
        S::WaitHighlight,
        S::SaveCurrentFile,
        S::OpenSearch,
        S::SetSearchQuery("RRITER_PGO_AUTOMATION_MARKER"),
        S::ToggleSearchCase,
        S::NextSearchResult,
        S::WaitMillis(250),
        S::PreviousSearchResult,
        S::WaitMillis(250),
        S::CloseSearch,
        S::OpenFile(PathBuf::from("src/large.rs")),
        S::WaitHighlight,
        S::ScrollEditorTimed { duration_secs: 22 },
        S::JumpMinimap(0.72),
        S::WaitMillis(500),
    ];

    for python_test in fixture_python_tests(workspace) {
        let should_scroll = python_test
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("perf_"));
        steps.push(S::OpenFile(python_test));
        steps.push(S::WaitHighlight);
        steps.push(S::WaitMillis(300));
        if should_scroll {
            steps.push(S::ScrollEditorTimed { duration_secs: 5 });
        }
    }

    steps.extend([
        S::OpenFile(PathBuf::from("tests/pgo_completion_hover.py")),
        S::WaitHighlight,
        S::SetEditorCursorAfter("pgo_completion_result = pri"),
        S::TriggerAutocomplete("print"),
        S::WaitMillis(900),
        S::SelectAutocomplete("print"),
        S::WaitMillis(900),
        S::ApplyAutocomplete("print"),
        S::SaveCurrentFile,
        S::ShowHover {
            needle: "pgo_hover_target",
            text: "async def pgo_hover_target(model: PgoCompletionModel) -> dict[str, int]\n\nDeterministic source hover used to train layout, syntax spans, inline code and scrolling.\n\n- Resolves a Python model\n- Produces a normalized mapping\n- Exercises the complete hover renderer\n\n`PgoCompletionModel` is defined in the same fixture module.",
        },
        S::ScrollHoverTimed { duration_secs: 5 },
        S::ClearHover,
        S::SetProjectSearchQuery("RRITER_PGO_AUTOMATION_MARKER"),
        S::RunProjectSearch,
        S::WaitProjectSearch,
        S::JumpFirstProjectSearchMatch,
        S::OpenPanel(PanelId::Git),
        S::WaitGit,
        S::WaitMillis(750),
        S::ToggleGitGraph,
        S::WaitGitGraph,
        S::LoadGitGraph { min_commits: GIT_FIXTURE_COMMIT_COUNT },
        S::WaitMillis(1000),
        S::ScrollGitGraphTimed { duration_secs: 24 },
        S::OpenPanel(PanelId::ApiClient),
        S::WaitMillis(500),
        S::ImportApiSpec,
        S::WaitApiSpec,
        S::WaitMillis(1000),
        S::WaitApiRoutesPanel,
        S::ScrollApiRoutesTimed { duration_secs: 12 },
        S::ResetApiPanelScroll,
        S::WaitMillis(500),
        S::SetApiRouteFilter("PGO_FEATURED_WRITE"),
        S::WaitApiRouteFilter("PGO_FEATURED_WRITE"),
        S::WaitMillis(500),
        S::OpenApiRouteMatching("PGO_FEATURED_WRITE"),
        S::WaitApiRouteOpen("PGO_FEATURED_WRITE"),
        S::WaitMillis(750),
        S::ScrollApiTabTimed { duration_secs: 10 },
        S::ResetApiTabScroll,
        S::ResetApiPanelScroll,
        S::ClearApiRouteFilter,
        S::WaitMillis(500),
        S::OpenApiAuth,
        S::WaitMillis(750),
        S::FocusApiAuth("BearerAuth"),
        S::SetApiAuthValue {
            scheme: "BearerAuth",
            value: "rriter-pgo-bearer-token",
        },
        S::SaveApiAuth { scheme: "BearerAuth", value: "rriter-pgo-bearer-token" },
        S::WaitMillis(500),
        S::SetApiRouteFilter("PGO_LOCAL_SERVER_PING"),
        S::WaitApiRouteFilter("PGO_LOCAL_SERVER_PING"),
        S::OpenApiRouteMatching("PGO_LOCAL_SERVER_PING"),
        S::WaitApiRouteOpen("PGO_LOCAL_SERVER_PING"),
        S::StartApiRequest,
        S::WaitApiResponse {
            expected_status: 200,
            body_marker: "RRITER_PGO_LOCAL_API_OK",
        },
        S::WaitMillis(500),
        S::OpenPanel(PanelId::Explorer),
        S::WaitFrames(3),
        S::OpenFileTreeContext,
        S::WaitFrames(4),
        S::CloseContextMenu,
        S::OpenPanel(PanelId::LspServers),
        S::WaitFrames(8),
        S::OpenPanel(PanelId::Problems),
        S::WaitFrames(8),
        S::OpenPanel(PanelId::Terminal),
        S::WaitTerminal,
    ]);
    steps.extend(terminal_workload_steps_for(std::env::consts::OS));
    steps.extend([
        S::WaitMillis(500),
        S::ShowSettings(true),
        S::WaitFrames(5),
        S::SetSettingsTab(0),
        S::WaitMillis(350),
        S::SetSettingsTab(1),
        S::WaitMillis(350),
        S::AddSettingsIgnore(".rriter-pgo-ignore/**"),
        S::WaitFileTree,
        S::WaitMillis(500),
        S::RemoveSettingsIgnore(".rriter-pgo-ignore/**"),
        S::WaitFileTree,
        S::SetSettingsTab(2),
        S::WaitMillis(350),
        S::RefreshSettingsTools,
        S::SetSettingsTab(3),
        S::WaitMillis(350),
        S::SetSettingsTab(4),
        S::WaitFrames(5),
        S::ShowSettings(false),
        S::WaitFrames(5),
        S::Finish,
    ]);
    steps
}

fn terminal_workload_steps_for(os: &str) -> Vec<AutomationStep> {
    use AutomationStep as S;
    if os == "linux" {
        vec![
            S::RunTerminalHtop,
            S::WaitTerminalHtopVisible,
            S::WaitMillis(10_000),
            S::InterruptTerminal,
            S::WaitTerminalHtopExit,
        ]
    } else {
        vec![
            S::RunTerminalBasicCommand,
            S::WaitTerminalBasicCommandVisible,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_scenario_uses_semantic_actions_and_covers_major_features() {
        let unique = format!("rriter-pgo-scenario-{}", std::process::id());
        let root = std::env::temp_dir().join(unique);
        let tests_dir = root.join("tests");
        std::fs::create_dir_all(&tests_dir).unwrap();
        std::fs::write(tests_dir.join("perf_alpha.py"), "value = 1\n").unwrap();
        std::fs::write(tests_dir.join("test_beta.py"), "value = 2\n").unwrap();

        let steps = full_pgo_scenario(&root);
        assert!(matches!(steps.first(), Some(AutomationStep::WaitReady)));
        assert!(matches!(steps.last(), Some(AutomationStep::Finish)));
        assert!(steps.iter().any(|step| matches!(
            step,
            AutomationStep::LoadGitGraph {
                min_commits: GIT_FIXTURE_COMMIT_COUNT
            }
        )));
        assert!(steps.iter().any(|step| matches!(
            step,
            AutomationStep::ScrollGitGraphTimed { duration_secs: 24 }
        )));
        assert!(
            steps
                .iter()
                .any(|step| matches!(step, AutomationStep::TriggerAutocomplete("print")))
        );
        assert!(steps.iter().any(|step| matches!(
            step,
            AutomationStep::SetEditorCursorAfter("pgo_completion_result = pri")
        )));
        assert!(
            steps
                .iter()
                .any(|step| matches!(step, AutomationStep::ShowHover { .. }))
        );
        assert!(!steps.iter().any(|step| step.name() == "wait-hover"));
        assert!(steps.iter().any(|step| matches!(
            step,
            AutomationStep::SetApiAuthValue {
                scheme: "BearerAuth",
                value: "rriter-pgo-bearer-token"
            }
        )));
        assert!(steps.iter().any(|step| matches!(
            step,
            AutomationStep::WaitApiResponse {
                expected_status: 200,
                body_marker: "RRITER_PGO_LOCAL_API_OK"
            }
        )));
        if cfg!(target_os = "linux") {
            assert!(
                steps
                    .iter()
                    .any(|step| matches!(step, AutomationStep::RunTerminalHtop))
            );
            assert!(
                steps
                    .iter()
                    .any(|step| matches!(step, AutomationStep::WaitTerminalHtopVisible))
            );
            assert!(
                steps
                    .iter()
                    .any(|step| matches!(step, AutomationStep::WaitTerminalHtopExit))
            );
            assert!(
                steps
                    .iter()
                    .any(|step| matches!(step, AutomationStep::WaitMillis(10_000)))
            );
        } else {
            assert!(
                steps
                    .iter()
                    .any(|step| matches!(step, AutomationStep::RunTerminalBasicCommand))
            );
            assert!(
                steps
                    .iter()
                    .any(|step| matches!(step, AutomationStep::WaitTerminalBasicCommandVisible))
            );
            assert!(
                !steps
                    .iter()
                    .any(|step| matches!(step, AutomationStep::RunTerminalHtop))
            );
        }
        assert!(
            !steps
                .iter()
                .any(|step| step.name().contains("terminal-listings"))
        );
        assert!(
            !steps
                .iter()
                .any(|step| step.name().contains("terminal-ready"))
        );
        assert!(steps.iter().any(|step| matches!(
            step,
            AutomationStep::AddSettingsIgnore(".rriter-pgo-ignore/**")
        )));
        for panel in [
            PanelId::Explorer,
            PanelId::Git,
            PanelId::ApiClient,
            PanelId::Terminal,
            PanelId::Problems,
            PanelId::LspServers,
        ] {
            assert!(steps.iter().any(|step| matches!(
                step,
                AutomationStep::OpenPanel(candidate) if *candidate == panel
            )));
        }
        assert!(steps.iter().any(|step| matches!(
            step,
            AutomationStep::SetProjectSearchQuery("RRITER_PGO_AUTOMATION_MARKER")
        )));
        for tab in 0..=4 {
            assert!(steps.iter().any(|step| matches!(
                step,
                AutomationStep::SetSettingsTab(candidate) if *candidate == tab
            )));
        }
        assert!(steps.iter().any(|step| matches!(
            step,
            AutomationStep::OpenFile(path) if path == Path::new("tests/perf_alpha.py")
        )));
        assert!(steps.iter().any(|step| matches!(
            step,
            AutomationStep::OpenFile(path) if path == Path::new("tests/test_beta.py")
        )));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn terminal_tui_state_requires_alt_screen_and_visible_content() {
        let mut grid = crate::app::terminal::TermGrid::new(12, 3);
        assert_eq!(
            terminal_grid_tui_state(&grid),
            TerminalTuiState {
                alternate_screen: false,
                non_blank_cells: 0,
            }
        );
        grid.is_alt = true;
        grid.lines[0][0].c = 'h';
        grid.lines[0][1].c = 't';
        grid.lines[0][2].c = 'o';
        grid.lines[0][3].c = 'p';
        assert_eq!(
            terminal_grid_tui_state(&grid),
            TerminalTuiState {
                alternate_screen: true,
                non_blank_cells: 4,
            }
        );
    }

    #[test]
    fn terminal_basic_output_is_detected_in_visible_or_scrollback_lines() {
        let mut grid = crate::app::terminal::TermGrid::new(16, 2);
        assert!(!terminal_grid_contains(&grid, TERMINAL_BASIC_OUTPUT));
        for (index, ch) in TERMINAL_BASIC_OUTPUT.chars().enumerate() {
            grid.lines[0][index].c = ch;
        }
        assert!(terminal_grid_contains(&grid, TERMINAL_BASIC_OUTPUT));

        grid.scrollback.push_back(grid.lines[0].clone());
        grid.lines[0].fill(crate::app::terminal::Cell::default());
        assert!(terminal_grid_contains(&grid, TERMINAL_BASIC_OUTPUT));
    }

    #[test]
    fn terminal_workload_is_htop_only_on_linux() {
        let linux = terminal_workload_steps_for("linux");
        assert!(
            linux
                .iter()
                .any(|step| matches!(step, AutomationStep::RunTerminalHtop))
        );
        assert!(
            linux
                .iter()
                .any(|step| matches!(step, AutomationStep::WaitMillis(10_000)))
        );
        assert!(
            !linux
                .iter()
                .any(|step| matches!(step, AutomationStep::RunTerminalBasicCommand))
        );

        for os in ["macos", "windows"] {
            let steps = terminal_workload_steps_for(os);
            assert_eq!(steps.len(), 2);
            assert!(matches!(steps[0], AutomationStep::RunTerminalBasicCommand));
            assert!(matches!(
                steps[1],
                AutomationStep::WaitTerminalBasicCommandVisible
            ));
            assert!(
                !steps
                    .iter()
                    .any(|step| matches!(step, AutomationStep::RunTerminalHtop))
            );
        }
    }

    #[test]
    fn automation_source_has_no_coordinate_or_ui_click_driver() {
        let source = include_str!("automation.rs");
        assert!(!source.contains(concat!("handle_", "ui_click")));
        assert!(!source.contains(concat!("Physical", "Position")));
        assert!(!source.contains(concat!("Mouse", "Button")));
        assert!(!source.contains(concat!("MouseScroll", "Delta")));
        assert!(!source.contains(concat!("ui_", "registry")));
        assert!(!source.contains(concat!("rect_", "for")));
    }

    #[test]
    fn scenario_has_no_optional_steps() {
        let steps = full_pgo_scenario(Path::new("/nonexistent"));
        let optional = steps
            .iter()
            .filter(|step| step.optional())
            .map(AutomationStep::name)
            .collect::<Vec<_>>();
        assert!(optional.is_empty());
    }

    #[test]
    fn fixture_repository_contains_thousand_commits_and_many_branches() {
        let unique = format!(
            "rriter-pgo-fixture-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();

        ensure_fixture_repository(&root).unwrap();
        let repository = git2::Repository::open(&root).unwrap();
        assert_eq!(
            fixture_head_commit_count(&repository),
            GIT_FIXTURE_COMMIT_COUNT
        );
        let branch_count = repository
            .branches(Some(git2::BranchType::Local))
            .unwrap()
            .count();
        assert_eq!(branch_count, GIT_FIXTURE_BRANCH_COUNT + 1);
        let mut walk = repository.revwalk().unwrap();
        walk.push_head().unwrap();
        let merge_count = walk
            .filter_map(Result::ok)
            .filter(|oid| {
                repository
                    .find_commit(*oid)
                    .is_ok_and(|commit| commit.parent_count() > 1)
            })
            .count();
        assert_eq!(merge_count, GIT_FIXTURE_BRANCH_COUNT);
        let first = repository.head().unwrap().target().unwrap();
        ensure_fixture_repository(&root).unwrap();
        let second = repository.head().unwrap().target().unwrap();
        assert_eq!(first, second);

        drop(repository);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn python_fixture_discovery_is_size_ordered_and_excludes_completion_fixture() {
        let root = std::env::temp_dir().join(format!("rriter-pgo-python-{}", std::process::id()));
        let tests = root.join("tests");
        std::fs::create_dir_all(&tests).unwrap();
        std::fs::write(tests.join("small.py"), "x\n").unwrap();
        std::fs::write(tests.join("large.py"), "x".repeat(100)).unwrap();
        std::fs::write(tests.join("pgo_completion_hover.py"), "fixture\n").unwrap();
        let files = fixture_python_tests(&root);
        assert_eq!(
            files,
            vec![
                PathBuf::from("tests/large.py"),
                PathBuf::from("tests/small.py")
            ]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hover_step_waits_for_a_matching_popup_to_be_drawn() {
        let mut state = crate::app::mouse::HoverState::default();
        let popup = crate::app::events::source_hover_popup_for_editor(
            &crate::editor::Editor::new(64),
            0,
            "hover text".to_string(),
            None,
            (0.0, 0.0),
        );
        state.byte_offset = Some(0);
        state.popup = Some(popup);
        assert_eq!(hover_popup_status(&state, 0), (true, false));
        state.rect = Some((1.0, 2.0, 3.0, 4.0));
        assert_eq!(hover_popup_status(&state, 0), (true, true));
        assert_eq!(hover_popup_status(&state, 1), (false, false));
    }

    #[test]
    fn interrupted_automation_writes_the_current_step_report() {
        let root = std::env::temp_dir().join(format!(
            "rriter-pgo-interrupted-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let report_path = root.join("report.json");
        let mut controller = AutomationController::new(AutomationOptions {
            workspace: root.clone(),
            report_path: report_path.clone(),
            timeout: Duration::from_secs(1),
        });
        controller.write_interrupted_report("test shutdown");
        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&report_path).unwrap()).unwrap();
        assert_eq!(report["status"], "failed");
        assert!(
            report["failed_step"]
                .as_str()
                .is_some_and(|message| message.contains("current_step=wait-ready"))
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn timed_scroll_uses_wall_clock_pause_and_reverse_motion() {
        let forward = timed_scroll_plan(1.0, 22);
        assert_eq!(forward.expected_impulses, 120);
        assert_eq!(forward.direction, 1.0);
        assert!(!forward.done);

        let pause = timed_scroll_plan(11.0, 22);
        assert_eq!(pause.expected_impulses, 1200);
        assert_eq!(pause.direction, 0.0);

        let reverse = timed_scroll_plan(13.0, 22);
        assert_eq!(reverse.expected_impulses, 1320);
        assert_eq!(reverse.direction, -1.0);

        let done = timed_scroll_plan(22.0, 22);
        assert_eq!(done.expected_impulses, 2400);
        assert!(done.done);
    }
}
