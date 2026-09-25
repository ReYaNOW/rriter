//! Linux-only headless mode: the real `App` and `Renderer` on an offscreen EGL pbuffer,
//! driven by line commands from stdin or `--script`.
#![cfg(target_os = "linux")]

pub(crate) mod frame;
pub(crate) mod profile;
pub(crate) mod protocol;
#[cfg(all(test, target_os = "linux"))]
mod tests;
#[cfg(all(test, target_os = "linux"))]
pub(crate) use tests::tests_support;

use crate::app::events::host_loop::{HeadlessLoopState, HostLoop};
use crate::app::{App, AppInitOptions};
use crate::platform::offscreen_gl::OffscreenContext;
use crate::platform::{self, HeadlessPolicy, HeadlessWindow, WindowHost};
use crate::renderer::Renderer;
use frame::StepState;
use profile::{BudgetChoice, HeadlessOptions, Profile};
use protocol::{ClickPhase, Command, MouseButtonArg, Response, WheelUnit};
use std::ffi::OsString;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, MouseScrollDelta};

/// Settle budget after startup and the positional path, before the first command is read.
const STARTUP_SETTLE: Duration = Duration::from_millis(500);
/// Pause of `wait` after a step that drew nothing, so it does not spin a core.
const WAIT_IDLE_SLEEP: Duration = Duration::from_millis(2);

/// Headless entry point (arguments without the program name). Returns the process exit code:
/// 0 all `ok`, 1 at least one `err`, 2 arguments, 3 GL context or `Renderer`.
pub(crate) fn run(args: &[OsString]) -> u8 {
    let options = match profile::parse_args(args) {
        Ok(options) => options,
        Err(reason) => {
            eprintln!("headless: {reason}");
            return 2;
        }
    };
    let profile = match Profile::prepare(&options) {
        Ok(profile) => profile,
        Err(reason) => {
            eprintln!("headless: {reason}");
            return 2;
        }
    };
    // `Profile` has no Drop: every path after `prepare` must reach `finish`.
    let code = run_prepared(&options, profile.root());
    profile.finish();
    code
}

fn run_prepared(options: &HeadlessOptions, root: &Path) -> u8 {
    if let Err(existing) = platform::set_app_root_override(root.to_path_buf()) {
        eprintln!("headless: profile root is already set to {}", existing.display());
        return 2;
    }
    platform::set_headless(HeadlessPolicy { allow_writes: options.allow_writes });
    // Env-only (honours RRITER_EGL_VENDOR) and still single-threaded here, as in `main`.
    crate::prefer_egl_vendor();
    crate::init_rayon_global_pool();
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
    let mut session = match HeadlessSession::new(options, root.to_path_buf()) {
        Ok(session) => session,
        Err((code, message)) => {
            eprintln!("{message}");
            return code;
        }
    };
    if let Some(path) = options.path.clone() {
        session.open_startup_path(path);
    }
    session.settle(STARTUP_SETTLE);
    session.run_loop(input, io::stdout().lock());
    session.app.shutdown_background_services();
    let code = session.exit_code();
    drop(session);
    code
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
}

impl HeadlessSession {
    pub(crate) fn new(options: &HeadlessOptions, profile_root: PathBuf) -> Result<Self, (u8, String)> {
        let (w, h) = options.size;
        let gl = OffscreenContext::new(w, h).map_err(egl_failure)?;
        let config = crate::load_config();
        platform::configure_tool_paths(config.tool_paths.clone());
        let mut app = App::new_from_config(config, AppInitOptions::headless());
        let mut renderer = Renderer::new(
            gl.glow(),
            options.scale as f32,
            app.theme.clone(),
            gl.requested_context(),
        )
        .map_err(|error| egl_failure(format!("Renderer: {error}")))?;
        renderer.resize(w, h);
        app.window = Some(Arc::new(WindowHost::Headless(HeadlessWindow::new(
            PhysicalSize::new(w, h),
            options.scale,
        ))));
        app.renderer = Some(renderer);
        // No first-frame clear/present or maximize pass: those are window-only.
        app.is_ready = true;
        app.tried_maximize = true;
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
            Command::Click { button, phase } => {
                let button = mouse_button(button);
                match phase {
                    ClickPhase::Both => self.click(button),
                    ClickPhase::Down => self.mouse_event(ElementState::Pressed, button),
                    ClickPhase::Up => self.mouse_event(ElementState::Released, button),
                }
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
                let saved = self.app.modifiers;
                self.app.modifiers = mods;
                let release = input.released();
                self.app.handle_main_key_input(&HostLoop::headless(&self.loop_state), input);
                self.step(true);
                self.app.handle_main_key_input(&HostLoop::headless(&self.loop_state), release);
                self.step(true);
                self.app.modifiers = saved;
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
            Command::Screenshot(path) => self.screenshot(&path),
            Command::Quit => Response::Ok(None),
            Command::Dump(_) => not_available("dump"),
            Command::Dialog(_) => not_available("dialog"),
            Command::Info => not_available("info"),
            Command::Bench { .. } => not_available("bench"),
            Command::Record { .. } => not_available("record"),
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
        // Same call as a dropped directory in `WindowEvent::DroppedFile`.
        self.app.apply_selected_workspace_folder(dir);
        self.frame_ok()
    }

    fn resize(&mut self, w: u32, h: u32) -> Response {
        if let Err(error) = self.gl.resize(w, h) {
            return Response::Err(format!("resize {w}x{h}: {error}"));
        }
        let size = PhysicalSize::new(w, h);
        if let Some(window) = self.headless_window() {
            window.set_size(size);
        }
        // `handle_main_resized` also resizes the renderer, as the window branch does.
        self.app.handle_main_resized(size);
        self.step(true);
        Response::Ok(Some(format!("{w}x{h}")))
    }

    fn screenshot(&mut self, path: &Path) -> Response {
        self.step(true);
        let (w, h) = self.gl.size();
        let Some(renderer) = self.app.renderer.as_ref() else {
            return Response::Err("renderer is unavailable".to_string());
        };
        frame::read_frame_rgba(&renderer.gl, w, h, &mut self.frame_buf);
        frame::flip_rows_in_place(&mut self.frame_buf, w as usize, h as usize);
        frame::force_opaque(&mut self.frame_buf);
        match frame::write_png(path, &self.frame_buf, w, h) {
            Ok(written) => Response::Ok(Some(format!("{} {w}x{h}", written.display()))),
            Err(error) => Response::Err(format!("io: {error}")),
        }
    }

    fn settle(&mut self, budget: Duration) -> (u32, bool) {
        let Self { app, loop_state, .. } = self;
        frame::settle_loop(budget, || {
            if frame::step_frame(app, loop_state, false) {
                StepState::Redrawn
            } else {
                StepState::Idle { flow: loop_state.last_control_flow.get() }
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

    fn step(&mut self, force: bool) -> bool {
        frame::step_frame(&mut self.app, &self.loop_state, force)
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

fn not_available(name: &str) -> Response {
    Response::Err(format!("'{name}' is not available yet"))
}
