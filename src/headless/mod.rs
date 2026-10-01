//! Linux-only headless mode: the real `App` and `Renderer` on an offscreen EGL pbuffer,
//! driven by line commands from stdin or `--script`.
#![cfg(target_os = "linux")]

pub(crate) mod bench;
pub(crate) mod dump;
pub(crate) mod frame;
pub(crate) mod profile;
pub(crate) mod protocol;
#[cfg(all(test, target_os = "linux"))]
mod tests;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_api_client_import;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_pdf;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_pgo;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_api_client_multipart;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_api_client_navigation;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_api_client_request;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_api_client_response_views;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_api_client_spec;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_api_mock;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_api_mock_contract;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_api_mock_python;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_database_connect;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_database_edit;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_database_query;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_editor;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_multi_cursor;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_editor_completion;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_editor_folding_minimap;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_editor_search;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_editor_search_center;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_editor_selection;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_editor_shortcuts;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_editor_sticky;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_deleted_tab;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_ide_startup;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_git_commit;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_git_diff;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_git_graph;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_goto_definition;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_keyboard_panels;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_layout;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_lsp_servers;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_markdown;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_markdown_media;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_markdown_toc;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_panels;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_pickers;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_problem_url;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_problems;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_problems_groups;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_project_search;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_protected_save;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_scrollbars_panels;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_settings_appearance;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_settings_database;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_settings_general;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_settings_help;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_settings_ide;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_settings_tools;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_splitters;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_tabs_dirty;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_tabs_tree;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_terminal;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_terminal_scrollbar;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_tree_ops;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_tree_trash;
#[cfg(all(test, target_os = "linux"))]
mod ui_tests_welcome;
#[cfg(all(test, target_os = "linux"))]
pub(crate) use tests::tests_support;

use crate::app::automation::{AutomationOptions, PgoScenario};
use crate::app::events::host_loop::{HeadlessLoopState, HostLoop};
use crate::app::{App, AppInitOptions};
use crate::platform::offscreen_gl::OffscreenContext;
use crate::platform::{self, HeadlessPolicy, HeadlessWindow, WindowHost};
use crate::renderer::Renderer;
use crate::startup_trace::StartupTrace;
use frame::{NativeWake, StepState, WakeCause};
use profile::{BudgetChoice, HeadlessOptions, Profile};
use protocol::{ClickPhase, Command, DialogAnswer, MouseButtonArg, Response, WheelUnit};
use std::ffi::OsString;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::os::fd::{FromRawFd, OwnedFd, RawFd};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, MouseScrollDelta};
use winit::event_loop::ControlFlow;

/// Settle budget after startup and the positional path, before the first command is read.
const STARTUP_SETTLE: Duration = Duration::from_millis(500);
/// Pause of `wait` after a step that drew nothing, so it does not spin a core.
const WAIT_IDLE_SLEEP: Duration = Duration::from_millis(2);
/// `settle` pause while a background result is outstanding and no `UiWaker` event came yet.
const AWAIT_BACKGROUND_SLEEP: Duration = Duration::from_millis(2);
/// Sleep slice of `native_wake` while it models a blocked native loop.
const NATIVE_WAKE_SLICE: Duration = Duration::from_millis(1);
/// Frame period of `--pgo-train`: 60 Hz, the pace of the windowed training run.
const PGO_FRAME_PACE: Duration = Duration::from_micros(16_667);

/// Headless entry point (arguments without the program name). Returns the process exit code:
/// 0 all `ok`, 1 at least one `err`, 2 arguments, 3 GL context or `Renderer`.
/// With `--pgo-train`: 0 scenario finished, 1 a step failed or timed out, 2 arguments,
/// workspace, GL context or `Renderer`.
pub(crate) fn run(args: &[OsString], trace: StartupTrace) -> u8 {
    trace.mark("headless-run");
    let options = match profile::parse_args(args) {
        Ok(options) => options,
        Err(reason) => {
            eprintln!("headless: {reason}");
            return 2;
        }
    };
    if let Some(automation) = &options.automation
        && !automation.workspace.is_dir()
    {
        eprintln!("headless: --pgo-workspace {}: not a directory", automation.workspace.display());
        return 2;
    }
    let profile = match Profile::prepare(&options) {
        Ok(profile) => profile,
        Err(reason) => {
            eprintln!("headless: {reason}");
            return 2;
        }
    };
    trace.mark("profile");
    // `Profile` has no Drop: every path after `prepare` must reach `finish`.
    let code = run_prepared(&options, profile.root(), trace);
    profile.finish();
    code
}

fn run_prepared(options: &HeadlessOptions, root: &Path, trace: StartupTrace) -> u8 {
    if let Err(existing) = platform::set_app_root_override(root.to_path_buf()) {
        eprintln!("headless: profile root is already set to {}", existing.display());
        return 2;
    }
    platform::set_headless(HeadlessPolicy { allow_writes: options.allow_writes });
    // Env-only (honours RRITER_EGL_VENDOR) and still single-threaded here, as in `main`.
    crate::prefer_egl_vendor();
    trace.mark("egl-vendor");
    crate::init_rayon_global_pool();
    trace.mark("rayon");
    if let Some(automation) = options.automation.as_ref() {
        return run_pgo(options, automation, root, trace);
    }
    let protocol = match split_protocol_fd(libc::STDOUT_FILENO, libc::STDERR_FILENO) {
        Ok(file) => file,
        Err(error) => {
            eprintln!("headless: cannot separate protocol stdout: {error}");
            return 2;
        }
    };
    let input: Box<dyn BufRead> = match options.script.as_deref() {
        Some(script) if script != Path::new("-") => match File::open(script) {
            Ok(file) => Box::new(BufReader::new(file)),
            Err(error) => {
                eprintln!("headless: {}: {error}", script.display());
                return 2;
            }
        },
        _ => Box::new(io::stdin().lock()),
    };
    let mut session = match HeadlessSession::with_trace(options, root.to_path_buf(), trace) {
        Ok(session) => session,
        Err((code, message)) => {
            eprintln!("{message}");
            return code;
        }
    };
    session.hz_probe = platform::probe_display_refresh_hz;
    if let Some(path) = options.path.clone() {
        session.open_startup_path(path);
    }
    session.settle(STARTUP_SETTLE);
    session.finish_startup();
    session.app.startup_trace.mark("settled");
    session.run_loop(input, io::LineWriter::new(protocol));
    session.app.shutdown_background_services();
    let code = session.exit_code();
    drop(session);
    code
}

/// `--pgo-train`: one automation scenario, paced like the window, no protocol commands.
fn run_pgo(
    options: &HeadlessOptions,
    automation: &AutomationOptions,
    root: &Path,
    trace: StartupTrace,
) -> u8 {
    let mut session = match HeadlessSession::with_trace(options, root.to_path_buf(), trace) {
        Ok(session) => session,
        Err((_, message)) => {
            eprintln!("{message}");
            return 2;
        }
    };
    if let Err(reason) = session.enter_pgo_scenario(automation) {
        eprintln!("headless: {reason}");
        session.app.shutdown_background_services();
        return 2;
    }
    let outcome = session.run_automation(Some(PGO_FRAME_PACE));
    drop(session);
    match outcome.failed_step {
        None => {
            eprintln!(
                "headless: PGO scenario {} finished after {} frames",
                automation.scenario.as_str(),
                outcome.frames
            );
            0
        }
        Some(step) => {
            eprintln!(
                "headless: PGO scenario {} failed at step {step} after {} frames (report {})",
                automation.scenario.as_str(),
                outcome.frames,
                automation.report_path.display()
            );
            1
        }
    }
}

/// Protocol replies keep the original `stdout` fd (returned, close-on-exec); `stdout` itself
/// becomes a copy of `stray`. The app's own `println!` from any thread then never mixes into
/// the protocol, never waits on a locked `Stdout`, and never hits EPIPE when the reader closes.
pub(crate) fn split_protocol_fd(stdout: RawFd, stray: RawFd) -> io::Result<File> {
    let fd = unsafe { libc::fcntl(stdout, libc::F_DUPFD_CLOEXEC, 3) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let protocol = File::from(unsafe { OwnedFd::from_raw_fd(fd) });
    if unsafe { libc::dup2(stray, stdout) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(protocol)
}

fn no_display_probe() -> Option<f64> {
    None
}

fn egl_failure(detail: impl std::fmt::Display) -> (u8, String) {
    (3, format!("headless: EGL setup failed at {detail}. Try RRITER_EGL_VENDOR=mesa"))
}

pub(crate) struct HeadlessSession {
    /// Declared before `gl`: the renderer's GL objects are dropped while the context is alive.
    pub(crate) app: App,
    pub(crate) gl: OffscreenContext,
    pub(crate) loop_state: HeadlessLoopState,
    pub(crate) had_error: bool,
    pub(crate) hz_probe: fn() -> Option<f64>,
    pub(crate) budget: BudgetChoice,
    pub(crate) profile_root: PathBuf,
    /// Readback buffer reused by every `screenshot`.
    frame_buf: Vec<u8>,
    /// The last step drew a frame, so the native loop runs `about_to_wait` again at once
    /// (after `RedrawRequested`); read by `native_wake`.
    drew_last_step: bool,
}

impl HeadlessSession {
    #[cfg(test)]
    pub(crate) fn new(options: &HeadlessOptions, profile_root: PathBuf) -> Result<Self, (u8, String)> {
        Self::with_trace(options, profile_root, StartupTrace::disabled())
    }

    fn with_trace(
        options: &HeadlessOptions,
        profile_root: PathBuf,
        trace: StartupTrace,
    ) -> Result<Self, (u8, String)> {
        let (w, h) = options.size;
        let gl = OffscreenContext::new_traced(w, h, &trace).map_err(egl_failure)?;
        trace.mark("offscreen-gl");
        let config = crate::load_config();
        trace.mark("config");
        platform::configure_tool_paths(config.tool_paths.clone());
        let mut app = App::new_from_config(
            config,
            AppInitOptions {
                startup_trace: trace,
                ..AppInitOptions::headless(options.automation.clone())
            },
        );
        app.startup_trace.mark("app");
        let mut renderer = Renderer::new(
            gl.glow(),
            options.scale as f32,
            app.theme.clone(),
            gl.requested_context(),
            &mut app.startup_trace,
        )
        .map_err(|error| egl_failure(format!("Renderer: {error}")))?;
        app.startup_trace.mark("renderer");
        renderer.resize(w, h);
        app.window = Some(Arc::new(WindowHost::Headless(HeadlessWindow::new(
            PhysicalSize::new(w, h),
            options.scale,
        ))));
        app.renderer = Some(renderer);
        // No first-frame clear/present or maximize pass: those are window-only.
        app.is_ready = true;
        app.is_focused = true;
        Ok(Self {
            app,
            gl,
            loop_state: HeadlessLoopState::default(),
            had_error: false,
            hz_probe: no_display_probe,
            budget: options.budget,
            profile_root,
            frame_buf: Vec::new(),
            drew_last_step: false,
        })
    }

    pub(crate) fn exit_code(&self) -> u8 {
        u8::from(self.had_error)
    }

    /// Reads commands until `quit`, EOF, an app exit request, or a failed write
    /// (closed stdout included). Returns false only when reading the input failed.
    pub(crate) fn run_loop(&mut self, mut input: impl BufRead, mut out: impl Write) -> bool {
        let mut line = Vec::new();
        loop {
            line.clear();
            match input.read_until(b'\n', &mut line) {
                Ok(0) => return true,
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    eprintln!("headless: cannot read commands: {error}");
                    self.had_error = true;
                    return false;
                }
            }
            let (response, quit) = match protocol::parse_line(&line) {
                Ok(None) => continue,
                Ok(Some(command)) => {
                    let quit = matches!(command, Command::Quit);
                    (self.execute(command), quit)
                }
                Err(reason) => (Response::Err(reason), false),
            };
            if matches!(response, Response::Err(_)) {
                self.had_error = true;
            }
            if writeln!(out, "{}", response.line()).and_then(|()| out.flush()).is_err() {
                return true;
            }
            if quit || self.loop_state.exit_requested.load(Ordering::Relaxed) {
                return true;
            }
        }
    }

    pub(crate) fn execute(&mut self, command: Command) -> Response {
        match command {
            Command::Open(path) => self.open(path),
            Command::Workspace(dir) => self.workspace(dir),
            Command::Resize { w, h } => self.resize(w, h),
            Command::Scale(scale) => {
                if let Some(window) = self.headless_window() {
                    window.set_scale_factor(scale);
                }
                self.app.handle_main_scale_factor_changed(scale);
                self.frame_ok()
            }
            Command::MouseMove { x, y } => {
                self.app.handle_main_cursor_moved(PhysicalPosition::new(x, y));
                self.frame_ok()
            }
            Command::Click { button, phase, alt } => {
                let button = mouse_button(button);
                let saved_modifiers = self.app.modifiers;
                // Held modifiers (e.g. Ctrl for go-to-definition) stay; `alt` only adds Alt.
                if alt {
                    self.app.modifiers |= winit::keyboard::ModifiersState::ALT;
                }
                match phase {
                    ClickPhase::Both => self.click(button),
                    ClickPhase::Down => self.mouse_event(ElementState::Pressed, button),
                    ClickPhase::Up => self.mouse_event(ElementState::Released, button),
                }
                self.app.modifiers = saved_modifiers;
                Response::Ok(None)
            }
            Command::DblClick { button } => {
                let button = mouse_button(button);
                self.click(button);
                self.click(button);
                Response::Ok(None)
            }
            Command::Wheel { dx, dy, unit } => {
                self.app.handle_main_mouse_wheel(match unit {
                    WheelUnit::Lines => MouseScrollDelta::LineDelta(dx as f32, dy as f32),
                    WheelUnit::Px => MouseScrollDelta::PixelDelta(PhysicalPosition::new(dx, dy)),
                });
                self.frame_ok()
            }
            Command::Key { input, mods, .. } => {
                let hold = self.app.press_key_combo(&HostLoop::headless(&self.loop_state), input, mods);
                self.step(true);
                self.app.release_key_combo(&HostLoop::headless(&self.loop_state), &hold);
                self.step(true);
                self.app.end_key_combo(hold);
                Response::Ok(None)
            }
            Command::Type(text) => {
                self.app.handle_main_ime_commit(&text);
                self.frame_ok()
            }
            Command::Settle { ms } => {
                let (frames, settled) = self.settle(Duration::from_millis(ms));
                Response::Ok(Some(format!("frames={frames} settled={settled}")))
            }
            Command::Wait { ms } => {
                let frames = self.wait(Duration::from_millis(ms));
                Response::Ok(Some(format!("frames={frames}")))
            }
            Command::Wake { ms } => {
                let wake = self.native_wake(Duration::from_millis(ms));
                Response::Ok(Some(format!(
                    "cause={} frame={} {}",
                    wake.cause.name(),
                    wake.frame,
                    self.control_flow_text()
                )))
            }
            Command::Idle { ms } => Response::Ok(Some(self.idle(Duration::from_millis(ms)))),
            Command::Screenshot(path) => self.screenshot(&path),
            Command::Quit => Response::Ok(None),
            Command::Dump(path) => self.dump(path.as_deref()),
            Command::PickerAnswer(paths) => {
                self.app.external_requests.queue_picker_answer(paths);
                Response::Ok(None)
            }
            Command::Dialog(answer) => {
                if !self.app.confirm_dialog.drawn_in_frame() {
                    return Response::Err("no dialog".to_string());
                }
                // The same methods as the window's dialog buttons; they clear the flag.
                match answer {
                    DialogAnswer::Save => self.app.begin_pending_action_save(),
                    DialogAnswer::Discard => self.app.discard_pending_action_changes(),
                    DialogAnswer::Cancel => self.app.cancel_pending_action(),
                }
                self.frame_ok()
            }
            Command::Info => Response::Ok(Some(bench::info_json(self).to_string())),
            Command::Bench { frames, csv, action } => bench::bench(self, frames, csv, &action),
            Command::Record { frames, dir, action } => bench::record(self, frames, dir, &action),
        }
    }

    /// Positional `FILE_OR_DIR`: like `open`/`workspace`, but errors go to stderr, not stdout.
    fn open_startup_path(&mut self, path: PathBuf) {
        let response = if path.is_dir() { self.workspace(path) } else { self.open(path) };
        if let Response::Err(reason) = response {
            eprintln!("headless: {reason}");
            self.had_error = true;
        }
    }

    fn open(&mut self, path: PathBuf) -> Response {
        let path = std::path::absolute(&path).unwrap_or(path);
        match std::fs::metadata(&path) {
            Err(error) => return Response::Err(format!("{}: {error}", path.display())),
            Ok(metadata) if metadata.is_dir() => {
                return Response::Err(format!("{}: is a directory", path.display()));
            }
            Ok(_) => {}
        }
        if let Err(error) = File::open(&path) {
            return Response::Err(format!("{}: {error}", path.display()));
        }
        // Same call as a dropped file in `WindowEvent::DroppedFile`.
        self.app.open_file_in_tab(path, true);
        self.step(true);
        Response::Ok(Some(format!("tabs={} active={}", self.app.tabs.len(), self.app.active_tab)))
    }

    fn workspace(&mut self, dir: PathBuf) -> Response {
        let dir = std::path::absolute(&dir).unwrap_or(dir);
        if !dir.is_dir() {
            return Response::Err(format!("{}: not a directory", dir.display()));
        }
        // A dropped directory only adds a workspace; the spec's `workspace` also enters IDE mode.
        if !self.app.is_ide_mode {
            self.app.enter_ide_mode_deferred();
        }
        // Same call as a dropped directory in `WindowEvent::DroppedFile`.
        self.app.apply_selected_workspace_folder(dir);
        let response = self.frame_ok();
        if self.drew_last_step {
            self.app.startup_trace.first_frame();
        }
        // The window runs this in the `about_to_wait` after the first content frame; the
        // command must end in the complete state.
        self.finish_startup();
        self.frame_ok();
        response
    }

    /// Puts the app where the scenario starts: IDE mode with the workspace (as the `workspace`
    /// command), or the welcome screen for `welcome`.
    fn enter_pgo_scenario(&mut self, automation: &AutomationOptions) -> Result<(), String> {
        if automation.scenario == PgoScenario::Welcome {
            return Ok(());
        }
        match self.workspace(automation.workspace.clone()) {
            Response::Err(reason) => Err(reason),
            _ => Ok(()),
        }
    }

    /// Ends a startup the way the window does: waits for the blank editor area to clear
    /// (highlight ready or the sync fallback deadline), then runs the deferred work.
    pub(crate) fn finish_startup(&mut self) {
        let limit = Instant::now()
            + crate::app::FILE_OPEN_LARGE_PRIORITY_HIGHLIGHT_TIMEOUT
            + Duration::from_millis(500);
        while self.app.startup_editor_pending.is_some() && Instant::now() < limit {
            if !self.step(false) {
                std::thread::sleep(WAIT_IDLE_SLEEP);
            }
        }
        self.app.finish_ide_deferred();
    }

    fn resize(&mut self, w: u32, h: u32) -> Response {
        let size = PhysicalSize::new(w, h);
        if let Err(error) = frame::resize_surface(&mut self.app, &mut self.gl, size) {
            return Response::Err(format!("resize {w}x{h}: {error}"));
        }
        self.step(true);
        Response::Ok(Some(format!("{w}x{h}")))
    }

    fn screenshot(&mut self, path: &Path) -> Response {
        self.step(true);
        let (w, h) = self.gl.size();
        match self.save_frame(path) {
            Ok(written) => Response::Ok(Some(format!("{} {w}x{h}", written.display()))),
            Err(error) => Response::Err(error),
        }
    }

    /// Reads the last drawn frame back into the reused buffer and writes it as a PNG.
    fn save_frame(&mut self, path: &Path) -> Result<PathBuf, String> {
        let (w, h) = self.gl.size();
        let Some(renderer) = self.app.renderer.as_ref() else {
            return Err("renderer is unavailable".to_string());
        };
        frame::read_frame_rgba(&renderer.gl, w, h, &mut self.frame_buf);
        frame::flip_rows_in_place(&mut self.frame_buf, w as usize, h as usize);
        frame::force_opaque(&mut self.frame_buf);
        frame::write_png(path, &self.frame_buf, w, h).map_err(|error| format!("io: {error}"))
    }

    fn dump(&mut self, path: Option<&Path>) -> Response {
        let json = dump::dump_json(&mut self.app, &self.loop_state).to_string();
        let Some(path) = path else {
            return Response::Ok(Some(json));
        };
        let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&path, json));
        match written {
            Ok(()) => Response::Ok(Some(path.display().to_string())),
            Err(error) => Response::Err(format!("io: {error}")),
        }
    }

    fn settle(&mut self, budget: Duration) -> (u32, bool) {
        let Self { app, gl, loop_state, drew_last_step, .. } = self;
        frame::settle_loop(budget, || {
            *drew_last_step = frame::step_frame(app, gl, loop_state, false);
            if *drew_last_step {
                app.startup_trace.first_frame();
                StepState::Redrawn
            } else {
                let flow = loop_state.last_control_flow.get();
                // A delivered `UiWaker` event is the native loop's next wake-up: step at once.
                if app.ui_waker.take_events() > 0 {
                    return StepState::Idle { flow: ControlFlow::Poll };
                }
                // The native loop sleeps until the job's wake; settle must not call that idle.
                if flow == ControlFlow::Wait && loop_state.awaiting_background.get() {
                    return StepState::Idle {
                        flow: ControlFlow::WaitUntil(Instant::now() + AWAIT_BACKGROUND_SLEEP),
                    };
                }
                StepState::Idle { flow }
            }
        })
    }

    fn wait(&mut self, duration: Duration) -> u32 {
        let started = Instant::now();
        let mut frames = 0u32;
        while started.elapsed() < duration {
            if self.step(false) {
                frames = frames.saturating_add(1);
            } else {
                std::thread::sleep(WAIT_IDLE_SLEEP);
            }
        }
        frames
    }

    /// Strict loop model (`wake`): blocks until the native event loop would run its next
    /// `about_to_wait` (see `WakeCause`), then runs that one pass and draws only if the app
    /// requested a frame. Unlike `settle`/`wait` it never steps on its own schedule, so a
    /// frame the app forgot to ask for (or a wake-up it forgot to arm) stays missing.
    /// `budget` bounds the block in real time; on `Timeout` nothing runs.
    pub(crate) fn native_wake(&mut self, budget: Duration) -> NativeWake {
        let end = Instant::now().checked_add(budget).unwrap_or_else(Instant::now);
        let cause = loop {
            if self.drew_last_step {
                break WakeCause::Redraw;
            }
            let wake_at = match self.loop_state.last_control_flow.get() {
                ControlFlow::Poll => break WakeCause::Poll,
                ControlFlow::Wait => None,
                ControlFlow::WaitUntil(at) => Some(at),
            };
            if self.app.ui_waker.take_events() > 0 {
                break WakeCause::UiWaker;
            }
            let now = Instant::now();
            if wake_at.is_some_and(|at| now >= at) {
                break WakeCause::Deadline;
            }
            if now >= end {
                return NativeWake { cause: WakeCause::Timeout, frame: false, stepped_at: None };
            }
            // Short slices: a `UiWaker` event may arrive before the deadline.
            let slice = wake_at.map_or(end, |at| at.min(end)).min(now + NATIVE_WAKE_SLICE);
            std::thread::sleep(slice.saturating_duration_since(now));
        };
        let stepped_at = Instant::now();
        let frame = self.step(false);
        NativeWake { cause, frame, stepped_at: Some(stepped_at) }
    }

    /// Strict loop model (`idle`): `native_wake` until `budget` runs out, with no input.
    fn idle(&mut self, budget: Duration) -> String {
        let end = Instant::now().checked_add(budget).unwrap_or_else(Instant::now);
        let (mut frames, mut redraws, mut polls, mut wakes, mut deadlines) = (0u32, 0u32, 0u32, 0u32, 0u32);
        loop {
            let wake = self.native_wake(end.saturating_duration_since(Instant::now()));
            let counter = match wake.cause {
                WakeCause::Timeout => break,
                WakeCause::Redraw => &mut redraws,
                WakeCause::Poll => &mut polls,
                WakeCause::UiWaker => &mut wakes,
                WakeCause::Deadline => &mut deadlines,
            };
            *counter = counter.saturating_add(1);
            frames = frames.saturating_add(u32::from(wake.frame));
            if Instant::now() >= end {
                break;
            }
        }
        format!(
            "frames={frames} redraws={redraws} polls={polls} wakes={wakes} deadlines={deadlines} {}",
            self.control_flow_text()
        )
    }

    /// `flow=<wait|poll|wait_until> deadline_ms=<n|none>` of the last `about_to_wait`.
    fn control_flow_text(&self) -> String {
        let flow = self.loop_state.last_control_flow.get();
        let (name, deadline_ms) = frame::control_flow_label(flow, Instant::now());
        match deadline_ms {
            Some(ms) => format!("flow={name} deadline_ms={ms}"),
            None => format!("flow={name} deadline_ms=none"),
        }
    }

    fn step(&mut self, force: bool) -> bool {
        self.drew_last_step =
            frame::step_frame(&mut self.app, &mut self.gl, &self.loop_state, force);
        self.drew_last_step
    }

    fn frame_ok(&mut self) -> Response {
        self.step(true);
        Response::Ok(None)
    }

    fn click(&mut self, button: MouseButton) {
        self.mouse_event(ElementState::Pressed, button);
        self.mouse_event(ElementState::Released, button);
    }

    fn mouse_event(&mut self, state: ElementState, button: MouseButton) {
        self.app.handle_main_mouse_input(&HostLoop::headless(&self.loop_state), state, button);
        self.step(true);
    }

    fn headless_window(&self) -> Option<&HeadlessWindow> {
        self.app.window.as_ref().and_then(|window| window.headless())
    }
}

fn mouse_button(button: MouseButtonArg) -> MouseButton {
    match button {
        MouseButtonArg::Left => MouseButton::Left,
        MouseButtonArg::Right => MouseButton::Right,
        MouseButtonArg::Middle => MouseButton::Middle,
    }
}
