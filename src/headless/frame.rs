//! Headless frame stepping, `settle`, and pbuffer readback into PNG files.

use crate::app::App;
use crate::app::events::about;
use crate::app::events::host_loop::{HeadlessLoopState, HostLoop};
use crate::app::events::main_frame::FrameOutcome;
use glow::HasContext;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};
use winit::event_loop::ControlFlow;

/// Pause after an idle `Wait` step, so background threads (highlighter, LSP, watcher) can
/// deliver into the mpsc receivers that the next `about_to_wait` drains.
const IDLE_WAIT_SLEEP: Duration = Duration::from_millis(5);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum StepState {
    Redrawn,
    Idle { flow: ControlFlow },
}

/// One iteration of the headless event loop: the same `about_to_wait` the window runs, then
/// a frame if the app asked for one (or `force`). Never sleeps. Returns true when a frame was drawn.
pub(crate) fn step_frame(app: &mut App, loop_state: &HeadlessLoopState, force: bool) -> bool {
    about::about_to_wait(app, &HostLoop::headless(loop_state));
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
    if app.headless_dialog_open
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
