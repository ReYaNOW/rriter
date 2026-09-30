//! Headless frame stepping, `settle`, and pbuffer readback into PNG files.

use crate::app::App;
use crate::app::events::about;
use crate::app::events::host_loop::{HeadlessLoopState, HostLoop};
use crate::app::events::main_frame::FrameOutcome;
use crate::headless::HeadlessSession;
use crate::platform::offscreen_gl::OffscreenContext;
use glow::HasContext;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};
use winit::dpi::PhysicalSize;
use winit::event_loop::ControlFlow;

/// Pause after an idle `Wait` step, so background threads (highlighter, LSP, watcher) can
/// deliver into the mpsc receivers that the next `about_to_wait` drains.
const IDLE_WAIT_SLEEP: Duration = Duration::from_millis(5);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum StepState {
    Redrawn,
    Idle { flow: ControlFlow },
}

/// Why the modelled native loop ran its next `about_to_wait` (strict `wake`/`idle`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WakeCause {
    /// The previous pass drew a frame: winit runs `about_to_wait` after `RedrawRequested`.
    Redraw,
    Poll,
    UiWaker,
    /// The `WaitUntil` instant was reached.
    Deadline,
    /// The budget ran out while the loop would still sleep; nothing ran.
    Timeout,
}

impl WakeCause {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Redraw => "redraw",
            Self::Poll => "poll",
            Self::UiWaker => "ui_waker",
            Self::Deadline => "deadline",
            Self::Timeout => "timeout",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct NativeWake {
    pub(crate) cause: WakeCause,
    /// The pass drew a frame (the app requested one).
    pub(crate) frame: bool,
    /// Start of the `about_to_wait` pass; `None` on `Timeout`. The frame is drawn after it.
    pub(crate) stepped_at: Option<Instant>,
}

/// `ControlFlow` as `dump`/`wake` report it: name and whole ms left until a `WaitUntil`.
pub(crate) fn control_flow_label(flow: ControlFlow, now: Instant) -> (&'static str, Option<u64>) {
    match flow {
        ControlFlow::Poll => ("poll", None),
        ControlFlow::Wait => ("wait", None),
        ControlFlow::WaitUntil(at) => {
            let left = at.saturating_duration_since(now).as_millis();
            ("wait_until", Some(u64::try_from(left).unwrap_or(u64::MAX)))
        }
    }
}

/// Resizes the pbuffer, the window model and the renderer to `size`; the one path of the
/// `resize` command and of an automation-requested window size.
pub(crate) fn resize_surface(
    app: &mut App,
    gl: &mut OffscreenContext,
    size: PhysicalSize<u32>,
) -> Result<(), String> {
    gl.resize(size.width, size.height)?;
    if let Some(window) = app.window.as_ref().and_then(|window| window.headless()) {
        window.set_size(size);
    }
    // `handle_main_resized` also resizes the renderer, as the window branch does.
    app.handle_main_resized(size);
    Ok(())
}

/// The window model changed size (`request_inner_size` from an automation step): brings the
/// pbuffer along before the frame is drawn. On failure the window goes back to the pbuffer
/// size, so the next frame does not retry the same resize.
fn sync_surface_to_window(app: &mut App, gl: &mut OffscreenContext) {
    let Some(size) = app.window.as_ref().map(|window| window.inner_size()) else {
        return;
    };
    let (w, h) = gl.size();
    if (size.width, size.height) == (w, h) {
        return;
    }
    if let Err(error) = resize_surface(app, gl, size) {
        eprintln!("headless: window resize {}x{}: {error}", size.width, size.height);
        if let Some(window) = app.window.as_ref().and_then(|window| window.headless()) {
            window.set_size(PhysicalSize::new(w, h));
        }
    }
}

/// One iteration of the headless event loop: the same `about_to_wait` the window runs, then
/// a frame if the app asked for one (or `force`). Never sleeps. Returns true when a frame was drawn.
pub(crate) fn step_frame(
    app: &mut App,
    gl: &mut OffscreenContext,
    loop_state: &HeadlessLoopState,
    force: bool,
) -> bool {
    about::about_to_wait(app, &HostLoop::headless(loop_state));
    // An automation tick may have asked for another window size; the frame is drawn at it.
    sync_surface_to_window(app, gl);
    // Always consume the request, also on forced frames, so it does not trigger a second frame.
    if !take_redraw_request(app) && !force {
        return false;
    }
    let outcome = render_frame(app);
    finish_gl(app);
    app.finish_main_frame(outcome);
    true
}

pub(crate) fn take_redraw_request(app: &App) -> bool {
    app.window
        .as_ref()
        .and_then(|window| window.headless())
        .is_some_and(|window| window.take_redraw_request())
}

/// The frame's draw calls: the main frame, then the confirmation dialog over it.
pub(crate) fn render_frame(app: &mut App) -> FrameOutcome {
    let outcome = app.render_main_frame();
    if app.confirm_dialog.drawn_in_frame()
        && let (Some(window), Some(renderer)) = (app.window.as_ref(), app.renderer.as_mut())
    {
        let size = window.inner_size();
        super::dump::draw_dialog(renderer, &app.base_title, size.width, size.height);
    }
    outcome
}

/// No swap on a pbuffer: finishing makes the frame's pixels complete for readback.
pub(crate) fn finish_gl(app: &App) {
    if let Some(renderer) = app.renderer.as_ref() {
        unsafe { renderer.gl.finish() };
    }
}

/// Extra time the runner gives the controller past its own global timeout before it gives up.
const RUN_TIMEOUT_GRACE: Duration = Duration::from_secs(10);

/// Result of `HeadlessSession::run_automation`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PgoRunOutcome {
    pub(crate) success: bool,
    /// Controller ticks (one per frame).
    pub(crate) frames: u64,
    /// Name of the failed step; `None` on success.
    pub(crate) failed_step: Option<String>,
}

/// Deadline of the frame after the one due at `previous`. A frame that ran late does not queue a
/// burst of catch-up frames: the schedule restarts from `now`.
pub(crate) fn next_frame_deadline(previous: Instant, pace: Duration, now: Instant) -> Instant {
    previous.checked_add(pace).map_or(now, |next| next.max(now))
}

impl HeadlessSession {
    /// Steps frames until the automation controller asks to exit or the run outlives the
    /// controller's timeout plus a grace period. `pace` sets the frame period (`None`: back to
    /// back). Service shutdown is `about_to_wait`'s job on the exit tick.
    pub(crate) fn run_automation(&mut self, pace: Option<Duration>) -> PgoRunOutcome {
        let start = Instant::now();
        let limit = self.app.automation.as_ref().map(|automation| automation.timeout());
        let hard_deadline = limit.and_then(|limit| {
            limit.checked_add(RUN_TIMEOUT_GRACE).and_then(|total| start.checked_add(total))
        });
        let mut due = start;
        let mut runner_timed_out = false;
        while !self.loop_state.exit_requested.load(Ordering::Relaxed) {
            if hard_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                runner_timed_out = true;
                break;
            }
            let drew = self.step(false);
            match pace {
                Some(pace) => {
                    due = next_frame_deadline(due, pace, Instant::now());
                    sleep_until(due, due);
                }
                None if !drew => thread::yield_now(),
                None => {}
            }
        }
        if runner_timed_out {
            // The exit tick never came, so `about_to_wait` did not stop the services.
            self.app.shutdown_background_services();
        }
        let Some(automation) = self.app.automation.as_ref() else {
            return PgoRunOutcome {
                success: false,
                frames: 0,
                failed_step: Some("no-automation".to_string()),
            };
        };
        let frames = automation.frames();
        match automation.outcome() {
            Some(Ok(())) => PgoRunOutcome { success: true, frames, failed_step: None },
            Some(Err(step)) => PgoRunOutcome { success: false, frames, failed_step: Some(step) },
            None => {
                let reason = if runner_timed_out { "runner-timeout" } else { "exit-before-finish" };
                PgoRunOutcome { success: false, frames, failed_step: Some(reason.to_string()) }
            }
        }
    }
}

/// Steps until two consecutive idle steps report `ControlFlow::Wait`, or `budget` runs out.
/// Returns the number of drawn frames and whether the app settled.
pub(crate) fn settle_loop(budget: Duration, mut step: impl FnMut() -> StepState) -> (u32, bool) {
    let start = Instant::now();
    let deadline = start.checked_add(budget).unwrap_or(start);
    let mut frames = 0u32;
    let mut idle_waits = 0u8;
    while idle_waits < 2 {
        if Instant::now() >= deadline {
            return (frames, false);
        }
        match step() {
            StepState::Redrawn => {
                frames = frames.saturating_add(1);
                idle_waits = 0;
            }
            StepState::Idle { flow: ControlFlow::Wait } => {
                idle_waits += 1;
                if idle_waits < 2 {
                    sleep_until(Instant::now() + IDLE_WAIT_SLEEP, deadline);
                }
            }
            StepState::Idle { flow: ControlFlow::WaitUntil(wake_at) } => {
                // Nothing is due before the budget ends (e.g. a label expiring in
                // seconds): sleeping to the deadline would change no frame.
                if wake_at >= deadline {
                    return (frames, false);
                }
                idle_waits = 0;
                sleep_until(wake_at, deadline);
            }
            StepState::Idle { flow: ControlFlow::Poll } => {
                idle_waits = 0;
                thread::yield_now();
            }
        }
    }
    (frames, true)
}

fn sleep_until(wake_at: Instant, deadline: Instant) {
    let pause = wake_at.min(deadline).saturating_duration_since(Instant::now());
    if !pause.is_zero() {
        thread::sleep(pause);
    }
}

/// Reads the current default framebuffer (bottom-up rows) into `buf`, reusing its allocation.
pub(crate) fn read_frame_rgba(gl: &glow::Context, w: u32, h: u32, buf: &mut Vec<u8>) {
    buf.resize(w as usize * h as usize * 4, 0);
    unsafe {
        // The pbuffer, not an offscreen cache FBO the renderer may have left bound.
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
        gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
        gl.read_pixels(
            0,
            0,
            w as i32,
            h as i32,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelPackData::Slice(Some(&mut buf[..])),
        );
    }
}

/// Mirrors RGBA rows vertically: GL rows are bottom-up, PNG rows top-down.
pub(crate) fn flip_rows_in_place(buf: &mut [u8], w: usize, h: usize) {
    let row = w * 4;
    if row == 0 || buf.len() < row * h {
        return;
    }
    for top_row in 0..h / 2 {
        let bottom_start = (h - 1 - top_row) * row;
        let (head, tail) = buf.split_at_mut(bottom_start);
        head[top_row * row..(top_row + 1) * row].swap_with_slice(&mut tail[..row]);
    }
}

/// The pbuffer config may carry alpha 0 where nothing was drawn; screenshots are opaque.
pub(crate) fn force_opaque(buf: &mut [u8]) {
    for pixel in buf.chunks_exact_mut(4) {
        pixel[3] = 255;
    }
}

/// Writes an RGBA8 PNG regardless of the file extension; creates the parent directory.
/// Returns the absolute path that was written.
pub(crate) fn write_png(path: &Path, buf: &[u8], w: u32, h: u32) -> Result<PathBuf, String> {
    let path = std::path::absolute(path).map_err(|error| error.to_string())?;
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    image::save_buffer_with_format(
        &path,
        buf,
        w,
        h,
        image::ExtendedColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .map_err(|error| error.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    #[test]
    fn headless_frame_flip_rows_swaps_2x2() {
        let mut buf = vec![
            1, 1, 1, 1, 2, 2, 2, 2, //
            3, 3, 3, 3, 4, 4, 4, 4,
        ];
        flip_rows_in_place(&mut buf, 2, 2);
        assert_eq!(buf, vec![3, 3, 3, 3, 4, 4, 4, 4, 1, 1, 1, 1, 2, 2, 2, 2]);
    }

    #[test]
    fn headless_frame_flip_rows_keeps_middle_row_and_ignores_short_buffer() {
        let mut buf = vec![1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3];
        flip_rows_in_place(&mut buf, 1, 3);
        assert_eq!(buf, vec![3, 3, 3, 3, 2, 2, 2, 2, 1, 1, 1, 1]);
        let mut short = vec![9u8; 7];
        flip_rows_in_place(&mut short, 1, 2);
        assert_eq!(short, vec![9u8; 7]);
    }

    #[test]
    fn headless_frame_settle_times_out_when_always_redrawn() {
        let budget = Duration::from_millis(60);
        let started = Instant::now();
        let (frames, settled) = settle_loop(budget, || StepState::Redrawn);
        let elapsed = started.elapsed();
        assert!(!settled);
        assert!(frames > 0);
        assert!(elapsed >= budget, "returned early after {elapsed:?}");
        assert!(elapsed <= budget + Duration::from_millis(50), "overran budget: {elapsed:?}");
    }

    #[test]
    fn headless_frame_settle_two_idle_waits() {
        let mut steps = 0;
        let (frames, settled) = settle_loop(Duration::from_secs(5), || {
            steps += 1;
            StepState::Idle { flow: ControlFlow::Wait }
        });
        assert_eq!((frames, settled, steps), (0, true, 2));
    }

    #[test]
    fn headless_frame_settle_counts_redraws_between_idles() {
        let wait = StepState::Idle { flow: ControlFlow::Wait };
        let mut script = VecDeque::from([StepState::Redrawn, wait, StepState::Redrawn, wait, wait]);
        let (frames, settled) = settle_loop(Duration::from_secs(5), || {
            script.pop_front().expect("settle stepped past the scripted states")
        });
        assert_eq!((frames, settled), (2, true));
        assert!(script.is_empty());
    }

    #[test]
    fn headless_frame_settle_wait_until_does_not_settle_and_respects_budget() {
        let budget = Duration::from_millis(40);
        let started = Instant::now();
        let (frames, settled) = settle_loop(budget, || StepState::Idle {
            flow: ControlFlow::WaitUntil(Instant::now() + Duration::from_secs(10)),
        });
        assert_eq!((frames, settled), (0, false));
        assert!(started.elapsed() <= budget + Duration::from_millis(50));
    }

    #[test]
    fn headless_frame_settle_returns_at_once_when_the_timer_is_past_the_budget() {
        let started = Instant::now();
        let (frames, settled) = settle_loop(Duration::from_secs(5), || StepState::Idle {
            flow: ControlFlow::WaitUntil(Instant::now() + Duration::from_secs(10)),
        });
        assert_eq!((frames, settled), (0, false));
        assert!(started.elapsed() < Duration::from_secs(1), "slept {:?}", started.elapsed());
    }

    #[test]
    fn headless_frame_write_png_creates_parent_and_forces_rgba() {
        let dir = std::env::temp_dir()
            .join(format!("rriter-headless-frame-{}", std::process::id()))
            .join("nested");
        let path = dir.join("out.image");
        let mut buf = vec![10u8, 20, 30, 0, 40, 50, 60, 0];
        force_opaque(&mut buf);
        let written = write_png(&path, &buf, 2, 1).expect("png written");
        assert!(written.is_absolute());
        // The extension is not `.png`: decoding by content proves the format was forced.
        let bytes = std::fs::read(&written).expect("read png");
        let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
            .expect("decode png")
            .to_rgba8();
        assert_eq!(decoded.as_raw(), &vec![10u8, 20, 30, 255, 40, 50, 60, 255]);
        let _ = std::fs::remove_dir_all(dir.parent().expect("scratch root"));
        assert!(write_png(Path::new("/proc/rriter-nope/x.png"), &buf, 2, 1).is_err());
    }
}
