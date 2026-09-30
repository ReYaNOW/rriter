use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::json;
use winit::dpi::PhysicalSize;
use crate::app::events::host_loop::HostLoop;

use crate::app::api_client::ApiFocus;
use crate::app::keyboard::KeyInput;
use crate::app::automation_dart::{DartAutomationStep, DartStepResult};
use crate::app::automation_database::{DatabaseAutomationStep, DatabaseStepResult};
use crate::app::automation_markdown::{MarkdownAutomationStep, MarkdownStepResult};
use crate::app::{App, PanelId};

pub const PGO_AUTOMATION_SCENARIO_VERSION: u32 = 17;

const TIMED_SCROLL_HZ: f32 = 120.0;
const TIMED_SCROLL_PAUSE_SECS: f32 = 2.0;
const GIT_FIXTURE_COMMIT_COUNT: usize = 1_000;
const GIT_FIXTURE_BRANCH_COUNT: usize = 50;
const PGO_HOVER_MAX_INSTALL_ATTEMPTS: u32 = 4;

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

/// Which automation scenario a run executes; parsed from `--pgo-scenario`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PgoScenario {
    Full,
    Startup,
    Welcome,
    Smoke,
    Group(String),
}

impl PgoScenario {
    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "full" => Ok(Self::Full),
            "startup" => Ok(Self::Startup),
            "welcome" => Ok(Self::Welcome),
            "smoke" => Ok(Self::Smoke),
            _ => match text.strip_prefix("group:") {
                Some("") => Err(format!("PGO scenario {text:?} has no group name")),
                Some(name) => Ok(Self::Group(name.to_string())),
                None => Err(format!("unknown PGO scenario {text:?}")),
            },
        }
    }

    pub fn as_str(&self) -> String {
        match self {
            Self::Full => "full".to_string(),
            Self::Startup => "startup".to_string(),
            Self::Welcome => "welcome".to_string(),
            Self::Smoke => "smoke".to_string(),
            Self::Group(name) => format!("group:{name}"),
        }
    }

    /// Only the GUI-equivalent training run leaves its open tabs behind for `startup`.
    pub fn saves_session_on_exit(&self) -> bool {
        matches!(self, Self::Full)
    }

    /// Only `startup` reads the saved session; every other scenario starts from a clean slate.
    pub fn restores_session(&self) -> bool {
        matches!(self, Self::Startup)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AutomationOptions {
    pub workspace: PathBuf,
    pub report_path: PathBuf,
    pub timeout: Duration,
    pub scenario: PgoScenario,
}

#[derive(Debug, Clone)]
pub(super) enum AutomationStep {
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
    Markdown(MarkdownAutomationStep),
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
    Dart(DartAutomationStep),
    Database(DatabaseAutomationStep),
    ShowSettings(bool),
    SetSettingsTab(usize),
    AddSettingsIgnore(&'static str),
    RemoveSettingsIgnore(&'static str),
    RefreshSettingsTools,
    /// Polled once per frame until `check` holds; the step fails with name `what` after `timeout_ms`.
    WaitUntil {
        what: &'static str,
        check: fn(&App) -> bool,
        timeout_ms: u64,
    },
    /// Runs once and must not block the frame for long: slow work goes to a thread and a
    /// following `WaitUntil` waits for the feature state. `Err(e)` fails the step as `what: e`.
    Call {
        what: &'static str,
        run: fn(&mut App, &Path) -> Result<(), String>,
    },
    /// A key combo in `KeyInput::parse_combo` syntax, delivered like the driver's `key` command.
    Key(&'static str),
    Wheel {
        at: AutomationTarget,
        dx: f32,
        dy: f32,
    },
    /// `mods` is a `+`-separated modifier list (`""`, `"ctrl"`, `"ctrl+shift"`).
    Click {
        at: AutomationTarget,
        button: AutomationButton,
        mods: &'static str,
        clicks: u8,
    },
    #[allow(dead_code)]
    Drag {
        from: AutomationTarget,
        to: AutomationTarget,
        steps: u16,
    },
    /// Opens a group; a `requires` returning `Err(reason)` skips everything up to `GroupEnd`.
    GroupStart {
        name: &'static str,
        requires: Option<fn(&App) -> Result<(), String>>,
    },
    GroupEnd,
    Finish,
}

/// Where a mouse step acts, in physical pixels.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub(super) enum AutomationTarget {
    /// Centre of the element's rectangle in the last frame's `ui_registry`.
    Ui(crate::ui_system::UiId),
    Point(f32, f32),
    /// Computed from the app state each time the step needs it; `None` fails the step.
    Find(fn(&App) -> Option<(f32, f32)>),
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AutomationButton {
    Left,
    Right,
    Middle,
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
            Self::Markdown(step) => step.name(),
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
            Self::Dart(step) => step.name(),
            Self::Database(step) => step.name(),
            Self::ShowSettings(show) => format!("show-settings:{show}"),
            Self::SetSettingsTab(tab) => format!("settings-tab:{tab}"),
            Self::AddSettingsIgnore(pattern) => format!("add-settings-ignore:{pattern}"),
            Self::RemoveSettingsIgnore(pattern) => format!("remove-settings-ignore:{pattern}"),
            Self::RefreshSettingsTools => "refresh-settings-tools".to_string(),
            Self::WaitUntil { what, .. } | Self::Call { what, .. } => (*what).to_string(),
            Self::Key(combo) => format!("key:{combo}"),
            Self::Wheel { .. } => "wheel".to_string(),
            Self::Click { .. } => "click".to_string(),
            Self::Drag { .. } => "drag".to_string(),
            Self::GroupStart { name, .. } => (*name).to_string(),
            Self::GroupEnd => "group-end".to_string(),
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
            Self::WaitUntil { timeout_ms, .. } => Duration::from_millis(*timeout_ms),
            Self::Dart(step) => step.timeout(),
            Self::Database(step) => step.timeout(),
            Self::Markdown(step) => step.timeout(),
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct AutomationFailure {
    index: usize,
    name: String,
    step_elapsed_ms: u64,
    reason: String,
    previous_completed_step: Option<String>,
    context: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutomationFailureKind {
    Failed,
    Timeout,
    GlobalTimeout,
}

pub struct AutomationController {
    options: AutomationOptions,
    steps: Vec<AutomationStep>,
    step_index: usize,
    step_progress: u32,
    step_start_logged: bool,
    started_at: Instant,
    step_started_at: Instant,
    completed: Vec<String>,
    skipped: Vec<String>,
    failure: Option<AutomationFailure>,
    report_written: bool,
    hover_last_anchor: Option<(f32, f32)>,
    /// Ticks seen so far; the headless runner ticks once per frame.
    frames: u64,
    finished: bool,
    scenario_error: Option<String>,
    /// `(group, reason)` of every group whose `requires` failed.
    skipped_groups: Vec<(String, String)>,
    /// Combo pressed by a `Key` step and released on its next tick.
    key_hold: Option<crate::app::keyboard::KeyComboHold>,
    /// Where the running `Drag` step pressed the button.
    drag_from: Option<(f32, f32)>,
}

impl AutomationController {
    pub fn new(options: AutomationOptions) -> Self {
        let now = Instant::now();
        let (steps, scenario_error) = match scenario_steps(&options.scenario, &options.workspace) {
            Ok(steps) => (steps, None),
            Err(error) => (Vec::new(), Some(error)),
        };
        Self {
            options,
            steps,
            step_index: 0,
            step_progress: 0,
            step_start_logged: false,
            started_at: now,
            step_started_at: now,
            completed: Vec::with_capacity(256),
            skipped: Vec::with_capacity(8),
            failure: None,
            report_written: false,
            hover_last_anchor: None,
            frames: 0,
            finished: false,
            scenario_error,
            skipped_groups: Vec::new(),
            key_hold: None,
            drag_from: None,
        }
    }

    /// `None` while the run is in progress; `Err(step name)` once a step failed or timed out.
    pub fn outcome(&self) -> Option<Result<(), String>> {
        if let Some(failure) = &self.failure {
            return Some(Err(failure.name.clone()));
        }
        self.finished.then_some(Ok(()))
    }

    pub fn scenario(&self) -> &PgoScenario {
        &self.options.scenario
    }

    /// Whether the exit path may write the user's tab list: only a headless run of a
    /// session-leaving scenario that succeeded. The GUI `--pgo-train` run (also `Full`)
    /// must never touch the real `tabs_ide.txt`, and a failed run must not leave a partial one.
    pub fn saves_session_now(&self, headless: bool) -> bool {
        headless && self.options.scenario.saves_session_on_exit() && self.outcome() == Some(Ok(()))
    }

    /// Ticks seen so far (one per frame in the headless runner).
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// The global timeout the controller enforces itself.
    pub fn timeout(&self) -> Duration {
        self.options.timeout
    }

    pub fn tick(
        &mut self,
        app: &mut App,
        event_loop: &HostLoop,
        now: Instant,
    ) -> AutomationTick {
        self.frames += 1;
        if let Some(reason) = self.scenario_error.take() {
            return self.fail_and_exit(
                self.options.scenario.as_str(),
                reason,
                None,
                now,
                AutomationFailureKind::Failed,
            );
        }
        if now.saturating_duration_since(self.started_at) > self.options.timeout {
            let step = self.steps.get(self.step_index).cloned();
            let name = step
                .as_ref()
                .map(AutomationStep::name)
                .unwrap_or_else(|| "complete".to_string());
            let context = step
                .as_ref()
                .map(|step| {
                    step_failure_context(app, step, self.step_progress, self.hover_last_anchor)
                })
                .filter(|context| !context.is_empty());
            return self.fail_and_exit(
                name,
                format!(
                    "global timeout after {:.1}s",
                    self.options.timeout.as_secs_f32()
                ),
                context,
                now,
                AutomationFailureKind::GlobalTimeout,
            );
        }

        let Some(step) = self.steps.get(self.step_index).cloned() else {
            return self.finish_and_exit();
        };
        self.log_step_start(&step);
        if now.saturating_duration_since(self.step_started_at) > step.timeout() {
            let message = format!("step timeout after {:.1}s", step.timeout().as_secs_f32());
            let context =
                step_failure_context(app, &step, self.step_progress, self.hover_last_anchor);
            if step.optional() {
                println!(
                    "PGO_AUTOMATION_SKIP index={} name={:?} reason={:?} context={:?}",
                    self.step_index,
                    step.name(),
                    message,
                    context
                );
                self.skipped.push(format!("{}: {message}", step.name()));
                self.advance(step.name(), now);
                return AutomationTick::Running;
            }
            return self.fail_and_exit(
                step.name(),
                message,
                (!context.is_empty()).then_some(context),
                now,
                AutomationFailureKind::Timeout,
            );
        }

        let result = self.run_step(app, event_loop, &step, now);
        match result {
            StepResult::Pending => AutomationTick::Running,
            StepResult::Done => {
                self.advance(step.name(), now);
                AutomationTick::Running
            }
            StepResult::Failed(message) if step.optional() => {
                let context =
                    step_failure_context(app, &step, self.step_progress, self.hover_last_anchor);
                println!(
                    "PGO_AUTOMATION_SKIP index={} name={:?} reason={:?} context={:?}",
                    self.step_index,
                    step.name(),
                    message,
                    context
                );
                self.skipped.push(format!("{}: {message}", step.name()));
                self.advance(step.name(), now);
                AutomationTick::Running
            }
            StepResult::Failed(message) => {
                let context =
                    step_failure_context(app, &step, self.step_progress, self.hover_last_anchor);
                self.fail_and_exit(
                    step.name(),
                    message,
                    (!context.is_empty()).then_some(context),
                    now,
                    AutomationFailureKind::Failed,
                )
            }
            StepResult::Exit => self.finish_and_exit(),
        }
    }

}

include!("automation_controller_steps.rs");

impl AutomationController {

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

    fn log_step_start(&mut self, step: &AutomationStep) -> bool {
        if self.step_start_logged {
            return false;
        }
        println!(
            "PGO_AUTOMATION_STEP_START index={} name={} timeout_ms={}",
            self.step_index,
            step.name(),
            step.timeout().as_millis()
        );
        self.step_start_logged = true;
        true
    }

    fn advance(&mut self, name: String, now: Instant) {
        println!(
            "PGO_AUTOMATION_STEP index={} name={} status=ok",
            self.step_index, name
        );
        self.completed.push(name);
        self.step_index += 1;
        self.step_progress = 0;
        self.step_start_logged = false;
        self.step_started_at = now;
    }

    fn current_step_name(&self) -> String {
        self.steps
            .get(self.step_index)
            .map(AutomationStep::name)
            .unwrap_or_else(|| "complete".to_string())
    }

    fn fail_and_exit(
        &mut self,
        name: String,
        reason: String,
        context: Option<String>,
        now: Instant,
        kind: AutomationFailureKind,
    ) -> AutomationTick {
        let failure = AutomationFailure {
            index: self.step_index,
            name,
            step_elapsed_ms: duration_ms(now.saturating_duration_since(self.step_started_at)),
            reason,
            previous_completed_step: self.completed.last().cloned(),
            context,
        };
        let prefix = match kind {
            AutomationFailureKind::Timeout => "PGO_AUTOMATION_TIMEOUT",
            AutomationFailureKind::Failed | AutomationFailureKind::GlobalTimeout => {
                "PGO_AUTOMATION_FAILED"
            }
        };
        let kind_field = if kind == AutomationFailureKind::GlobalTimeout {
            " kind=global-timeout"
        } else {
            ""
        };
        eprintln!(
            "{prefix}{kind_field} index={} name={} step_elapsed_ms={} total_elapsed_ms={} previous_step={:?} reason={:?} context={:?} report={:?}",
            failure.index,
            failure.name,
            failure.step_elapsed_ms,
            duration_ms(now.saturating_duration_since(self.started_at)),
            failure.previous_completed_step.as_deref().unwrap_or("none"),
            failure.reason,
            failure.context.as_deref().unwrap_or(""),
            self.options.report_path
        );
        self.failure = Some(failure);
        self.write_report("failed");
        AutomationTick::Exit
    }

    fn finish_and_exit(&mut self) -> AutomationTick {
        self.finished = true;
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
        self.failure = Some(AutomationFailure {
            index: self.step_index,
            name: self.current_step_name(),
            step_elapsed_ms: duration_ms(self.step_started_at.elapsed()),
            reason: message,
            previous_completed_step: self.completed.last().cloned(),
            context: None,
        });
        self.write_report("failed");
    }

    fn write_report(&mut self, status: &str) {
        if self.report_written {
            return;
        }
        self.report_written = true;
        let failure = self.failure.as_ref();
        let report = json!({
            "status": status,
            "scenario_version": PGO_AUTOMATION_SCENARIO_VERSION,
            "scenario": self.options.scenario.as_str(),
            "frames": self.frames,
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
            "skipped_groups": self
                .skipped_groups
                .iter()
                .map(|(group, reason)| json!({ "group": group, "reason": reason }))
                .collect::<Vec<_>>(),
            "failed_step": failure.map(|failure| failure.reason.as_str()),
            "failed_step_index": failure.map(|failure| failure.index),
            "failed_step_name": failure.map(|failure| failure.name.as_str()),
            "failed_step_elapsed_ms": failure.map(|failure| failure.step_elapsed_ms),
            "failure_reason": failure.map(|failure| failure.reason.as_str()),
            "previous_completed_step": failure
                .and_then(|failure| failure.previous_completed_step.as_deref()),
            "failure_context": failure.and_then(|failure| failure.context.as_deref()),
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

include!("automation_semantic_actions.rs");

include!("automation_fixtures.rs");

#[cfg(test)]
mod tests {
    use super::*;
    include!("automation_tests.rs");
}
