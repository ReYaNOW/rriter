//! Headless frame-cost measurement: frame budget, per-frame timings, summary, CSV.

use super::HeadlessSession;
use super::frame;
use super::profile::{BudgetChoice, runtime_base};
use super::protocol::{BenchAction, Response};
use crate::app::App;
use crate::app::events::about;
use crate::app::events::host_loop::{HeadlessLoopState, HostLoop};
use crate::platform;
use crate::render_view::{FrameTelemetry, TELEMETRY_ENABLED, take_frame_telemetry};
use glow::HasContext;
use serde_json::{Value, json};
use std::ffi::OsStr;
use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use winit::event::MouseScrollDelta;

const DEFAULT_HZ: f64 = 240.0;
const WORST_FRAMES: usize = 5;
const GPU_UTIL_TIMEOUT: Duration = Duration::from_secs(2);
const CSV_HEADER: &str = "frame,update_ms,draw_cpu_ms,gpu_ms,total_ms,flush_calls,vertices,\
root_prep_ms,root_cache_ms,root_pre_editor_ms,root_overlays_ms,root_chrome_ms,\
chrome_0_ms,chrome_1_ms,chrome_2_ms,chrome_3_ms,chrome_4_ms,chrome_5_ms,scroll_y";
/// Extra `record` columns: animation state passed to `Renderer::draw`.
const RECORD_COLUMNS: &str = ",sticky_anim_progress,search_anim_y,tab_scroll";

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Budget {
    pub hz: f64,
    pub budget_ms: f64,
    pub source: &'static str,
}

/// `--budget-ms`/`--hz` win; otherwise the monitor refresh rate, else 240 Hz.
pub(crate) fn resolve_budget(choice: BudgetChoice, probe: impl FnOnce() -> Option<f64>) -> Budget {
    let (hz, source) = match choice {
        BudgetChoice::Ms(ms) => return Budget { hz: 1000.0 / ms, budget_ms: ms, source: "arg" },
        BudgetChoice::Hz(hz) => (hz, "arg"),
        BudgetChoice::Auto => match probe() {
            Some(hz) => (hz, "monitor"),
            None => (DEFAULT_HZ, "default"),
        },
    };
    Budget { hz, budget_ms: 1000.0 / hz, source }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Percentiles {
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub max: f64,
}

impl Percentiles {
    fn json(&self) -> Value {
        json!({"p50": self.p50, "p95": self.p95, "p99": self.p99, "max": self.max})
    }
}

/// Nearest-rank percentiles; sorts `samples` in place. Empty input gives zeros.
pub(crate) fn percentiles(samples: &mut [f64]) -> Percentiles {
    let n = samples.len();
    if n == 0 {
        return Percentiles::default();
    }
    samples.sort_by(f64::total_cmp);
    // Integer rank: float `p / 100 * n` can land just above an integer and skip a rank.
    let rank = |p: usize| samples[(p * n).div_ceil(100).clamp(1, n) - 1];
    Percentiles { p50: rank(50), p95: rank(95), p99: rank(99), max: samples[n - 1] }
}

/// utime + stime of a `/proc/<pid>/stat` line in ms. Fields are counted after the last `)`,
/// because the command name may contain spaces and parentheses.
pub(crate) fn parse_proc_stat_cpu_ms(stat: &str, clk_tck: f64) -> Option<f64> {
    if !(clk_tck > 0.0) {
        return None;
    }
    let mut fields = stat[stat.rfind(')')? + 1..].split_whitespace().skip(11);
    let utime: u64 = fields.next()?.parse().ok()?;
    let stime: u64 = fields.next()?.parse().ok()?;
    Some((utime + stime) as f64 * 1000.0 / clk_tck)
}

pub(crate) fn parse_loadavg(text: &str) -> Option<[f64; 3]> {
    let mut fields = text.split_whitespace().map(str::parse::<f64>);
    Some([fields.next()?.ok()?, fields.next()?.ok()?, fields.next()?.ok()?])
}

fn process_cpu_ms() -> Option<f64> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let clk_tck = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as f64;
    parse_proc_stat_cpu_ms(&stat, clk_tck)
}

fn loadavg() -> Option<[f64; 3]> {
    parse_loadavg(&std::fs::read_to_string("/proc/loadavg").ok()?)
}

/// NVIDIA GPU utilization in percent; `None` without `nvidia-smi` or on any failure.
fn gpu_util() -> Option<f64> {
    let program = platform::resolve_executable(OsStr::new("nvidia-smi"))?;
    let mut command = Command::new(program);
    command.args(["--query-gpu=utilization.gpu", "--format=csv,noheader,nounits"]);
    let output = platform::run_command_output(&mut command, GPU_UTIL_TIMEOUT).ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout).lines().next()?.trim().parse().ok()
}

/// One measured frame: a row of the CSV.
#[derive(Clone, Debug, Default)]
pub(crate) struct FrameRow {
    pub frame: u32,
    pub update_ms: f64,
    pub draw_cpu_ms: f64,
    pub gpu_ms: Option<f64>,
    pub total_ms: f64,
    pub telemetry: FrameTelemetry,
    pub scroll_y: f64,
    /// `record` only: `sticky_anim_progress`, `search_anim_y`, `tab_scroll`.
    pub anim: Option<[f32; 3]>,
}

impl FrameRow {
    fn write_csv(&self, line: &mut String) {
        let t = &self.telemetry;
        let _ = write!(
            line,
            "{},{:.4},{:.4},",
            self.frame, self.update_ms, self.draw_cpu_ms
        );
        if let Some(gpu_ms) = self.gpu_ms {
            let _ = write!(line, "{gpu_ms:.4}");
        }
        let _ = write!(line, ",{:.4},{},{}", self.total_ms, t.flush_calls, t.vertices);
        for ms in t.root_phase_ms.iter().chain(&t.chrome_ms) {
            let _ = write!(line, ",{ms:.4}");
        }
        let _ = write!(line, ",{}", self.scroll_y);
        if let Some([sticky, search, tab]) = self.anim {
            let _ = write!(line, ",{sticky},{search},{tab}");
        }
        line.push('\n');
    }

    fn worst_json(&self) -> Value {
        json!({
            "frame": self.frame, "total_ms": self.total_ms, "update_ms": self.update_ms,
            "draw_cpu_ms": self.draw_cpu_ms, "gpu_ms": self.gpu_ms,
            "root_phase_ms": self.telemetry.root_phase_ms,
        })
    }
}

/// `TELEMETRY_ENABLED` for the duration of a run; the previous value comes back on drop.
struct TelemetryOn(bool);

impl TelemetryOn {
    fn enable() -> Self {
        Self(TELEMETRY_ENABLED.swap(true, Ordering::Relaxed))
    }
}

impl Drop for TelemetryOn {
    fn drop(&mut self) {
        TELEMETRY_ENABLED.store(self.0, Ordering::Relaxed);
    }
}

/// `GL_TIME_ELAPSED` around draw + finish. `create_query` does not check support, so the
/// first `begin_query` is checked with `get_error`; a failure disables it for the whole run.
struct GpuTimer {
    query: Option<glow::Query>,
    checked: bool,
}

impl GpuTimer {
    fn new(gl: &glow::Context) -> Self {
        Self { query: unsafe { gl.create_query() }.ok(), checked: false }
    }

    fn begin(&mut self, gl: &glow::Context) {
        let Some(query) = self.query else { return };
        unsafe {
            if !self.checked {
                // Drop stale errors so the check below sees only `begin_query`'s.
                for _ in 0..16 {
                    if gl.get_error() == glow::NO_ERROR {
                        break;
                    }
                }
            }
            gl.begin_query(glow::TIME_ELAPSED, query);
            if !self.checked {
                self.checked = true;
                if gl.get_error() != glow::NO_ERROR {
                    gl.delete_query(query);
                    self.query = None;
                }
            }
        }
    }

    fn end(&self, gl: &glow::Context) -> Option<f64> {
        let query = self.query?;
        unsafe {
            gl.end_query(glow::TIME_ELAPSED);
            Some(gl.get_query_parameter_u64(query, glow::QUERY_RESULT) as f64 / 1_000_000.0)
        }
    }

    fn delete(self, gl: &glow::Context) {
        if let Some(query) = self.query {
            unsafe { gl.delete_query(query) };
        }
    }
}

/// Feeds one frame's action without drawing: `key` presses and releases with no frame between.
fn apply_action(app: &mut App, loop_state: &HeadlessLoopState, action: &BenchAction) {
    match action {
        BenchAction::None => {}
        BenchAction::Wheel { dx, dy } => {
            app.handle_main_mouse_wheel(MouseScrollDelta::LineDelta(*dx as f32, *dy as f32));
        }
        BenchAction::Key { input, mods } => {
            let saved = app.modifiers;
            app.modifiers = *mods;
            app.handle_main_key_input(&HostLoop::headless(loop_state), input.clone());
            app.handle_main_key_input(&HostLoop::headless(loop_state), input.released());
            app.modifiers = saved;
        }
        BenchAction::Type(text) => app.handle_main_ime_commit(text),
    }
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// The measured frame loop shared by `bench` and `record`: action + `about_to_wait`
/// (`update_ms`), draw calls (`draw_cpu_ms`), GPU time, and the whole step (`total_ms`).
/// With `record_dir`, every frame is also saved as `frame-%04d.png` outside the timings.
pub(crate) fn run_frames(
    session: &mut HeadlessSession,
    frames: u32,
    action: &BenchAction,
    record_dir: Option<&Path>,
) -> Result<Vec<FrameRow>, String> {
    let gl = session.gl.glow();
    let mut timer = GpuTimer::new(&gl);
    let rows = measured_frames(session, &gl, &mut timer, frames, action, record_dir);
    timer.delete(&gl);
    rows
}

fn measured_frames(
    session: &mut HeadlessSession,
    gl: &glow::Context,
    timer: &mut GpuTimer,
    frames: u32,
    action: &BenchAction,
    record_dir: Option<&Path>,
) -> Result<Vec<FrameRow>, String> {
    let _telemetry = TelemetryOn::enable();
    take_frame_telemetry();
    let mut rows = Vec::with_capacity(frames as usize);
    for frame in 0..frames {
        let start = Instant::now();
        apply_action(&mut session.app, &session.loop_state, action);
        about::about_to_wait(&mut session.app, &HostLoop::headless(&session.loop_state));
        // Every bench frame is forced; drop the request so it cannot leak into a later step.
        frame::take_redraw_request(&session.app);
        let draw_start = Instant::now();
        timer.begin(gl);
        let outcome = frame::render_frame(&mut session.app);
        let draw_end = Instant::now();
        frame::finish_gl(&session.app);
        let gpu_ms = timer.end(gl);
        session.app.finish_main_frame(outcome);
        let total_ms = ms(start.elapsed());
        let app = &session.app;
        rows.push(FrameRow {
            frame,
            update_ms: ms(draw_start - start),
            draw_cpu_ms: ms(draw_end - draw_start),
            gpu_ms,
            total_ms,
            telemetry: take_frame_telemetry(),
            scroll_y: f64::from(app.scroll_y.current),
            anim: record_dir
                .map(|_| [app.sticky_anim_progress, app.search_anim_y, app.tab_scroll.current]),
        });
        if let Some(dir) = record_dir {
            session.save_frame(&dir.join(format!("frame-{frame:04}.png")))?;
        }
    }
    Ok(rows)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Motion {
    pub delta_min: f64,
    pub delta_max: f64,
    pub nonmonotonic_frames: u32,
}

/// Frame-to-frame `scroll_y` deltas; a frame is non-monotonic when its delta is nonzero and
/// against the direction of the first nonzero delta (the action's direction does not change).
pub(crate) fn motion_stats(scroll_y: &[f64]) -> Motion {
    let mut motion = Motion::default();
    let mut direction = 0.0;
    for (index, pair) in scroll_y.windows(2).enumerate() {
        let delta = pair[1] - pair[0];
        if index == 0 {
            (motion.delta_min, motion.delta_max) = (delta, delta);
        }
        motion.delta_min = motion.delta_min.min(delta);
        motion.delta_max = motion.delta_max.max(delta);
        if delta == 0.0 {
            continue;
        }
        if direction == 0.0 {
            direction = delta.signum();
        } else if delta.signum() != direction {
            motion.nonmonotonic_frames += 1;
        }
    }
    motion
}

struct SystemSample {
    loadavg: Option<[f64; 3]>,
    gpu_util: Option<f64>,
}

impl SystemSample {
    fn take() -> Self {
        Self { loadavg: loadavg(), gpu_util: gpu_util() }
    }
}

fn default_csv_path() -> PathBuf {
    let unix_ms = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis());
    runtime_base().join("rriter-headless").join(format!("bench-{unix_ms}.csv"))
}

/// Creates the CSV (and its directory) before the run, so a bad path fails without measuring.
fn create_file(path: &Path) -> io::Result<File> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    File::create(path)
}

fn write_csv(file: File, rows: &[FrameRow], record: bool) -> io::Result<()> {
    let mut out = BufWriter::new(file);
    let extra = if record { RECORD_COLUMNS } else { "" };
    writeln!(out, "{CSV_HEADER}{extra}")?;
    let mut line = String::new();
    for row in rows {
        line.clear();
        row.write_csv(&mut line);
        out.write_all(line.as_bytes())?;
    }
    out.flush()
}

fn summary_json(
    rows: &[FrameRow],
    budget: Budget,
    system: [&SystemSample; 2],
    process_cpu_ms: Option<f64>,
    csv: &Path,
) -> Value {
    let mut total: Vec<f64> = rows.iter().map(|row| row.total_ms).collect();
    let mut draw: Vec<f64> = rows.iter().map(|row| row.draw_cpu_ms).collect();
    let gpu = rows.iter().map(|row| row.gpu_ms).collect::<Option<Vec<f64>>>();
    let mut worst: Vec<&FrameRow> = rows.iter().collect();
    worst.sort_by(|a, b| b.total_ms.total_cmp(&a.total_ms));
    let [before, after] = system;
    json!({
        "frames": rows.len(),
        "budget_ms": budget.budget_ms,
        "hz": budget.hz,
        "hz_source": budget.source,
        "total_ms": percentiles(&mut total).json(),
        "gpu_ms": gpu.map(|mut gpu| percentiles(&mut gpu).json()),
        "draw_cpu_ms": percentiles(&mut draw).json(),
        "over_budget": rows.iter().filter(|row| row.total_ms > budget.budget_ms).count(),
        "worst": worst.iter().take(WORST_FRAMES).map(|row| row.worst_json()).collect::<Vec<_>>(),
        "system": {
            "loadavg_before": before.loadavg,
            "loadavg_after": after.loadavg,
            "cpus": std::thread::available_parallelism().map_or(1, |n| n.get()),
            "gpu_util_before": before.gpu_util,
            "gpu_util_after": after.gpu_util,
            "process_cpu_ms": process_cpu_ms,
        },
        "csv": csv.display().to_string(),
    })
}

/// `bench <frames> [csv=<path>] [action]` → `ok <json summary>`.
pub(crate) fn bench(session: &mut HeadlessSession, frames: u32, csv: Option<PathBuf>, action: &BenchAction) -> Response {
    let csv = csv.unwrap_or_else(default_csv_path);
    let csv = std::path::absolute(&csv).unwrap_or(csv);
    measure(session, frames, action, &csv, None)
}

/// `record <frames> <dir> [action]`: `bench` plus a PNG per frame, animation columns in
/// `<dir>/frames.csv`, and the `motion` of `scroll_y` in the summary.
pub(crate) fn record(session: &mut HeadlessSession, frames: u32, dir: PathBuf, action: &BenchAction) -> Response {
    let dir = std::path::absolute(&dir).unwrap_or(dir);
    if let Err(error) = std::fs::create_dir_all(&dir) {
        return Response::Err(format!("io: {error}"));
    }
    measure(session, frames, action, &dir.join("frames.csv"), Some(&dir))
}

fn measure(
    session: &mut HeadlessSession,
    frames: u32,
    action: &BenchAction,
    csv: &Path,
    record_dir: Option<&Path>,
) -> Response {
    let file = match create_file(csv) {
        Ok(file) => file,
        Err(error) => return Response::Err(format!("io: {error}")),
    };
    let budget = resolve_budget(session.budget, session.hz_probe);
    let before = SystemSample::take();
    let cpu_before = process_cpu_ms();
    let rows = match run_frames(session, frames, action, record_dir) {
        Ok(rows) => rows,
        Err(error) => return Response::Err(error),
    };
    let cpu_after = process_cpu_ms();
    let after = SystemSample::take();
    if let Err(error) = write_csv(file, &rows, record_dir.is_some()) {
        return Response::Err(format!("io: {error}"));
    }
    let process_cpu_ms = cpu_before.zip(cpu_after).map(|(before, after)| after - before);
    let mut summary = summary_json(&rows, budget, [&before, &after], process_cpu_ms, csv);
    if record_dir.is_some() {
        let scroll: Vec<f64> = rows.iter().map(|row| row.scroll_y).collect();
        let motion = motion_stats(&scroll);
        summary["motion"] = json!({
            "scroll_y_delta_min": motion.delta_min,
            "scroll_y_delta_max": motion.delta_max,
            "nonmonotonic_frames": motion.nonmonotonic_frames,
        });
    }
    Response::Ok(Some(summary.to_string()))
}

/// `info` payload: frame budget, GL strings, write policy, profile root.
pub(crate) fn info_json(session: &HeadlessSession) -> Value {
    let budget = resolve_budget(session.budget, session.hz_probe);
    let gl = session.gl.gl_strings();
    json!({
        "hz": budget.hz,
        "budget_ms": budget.budget_ms,
        "hz_source": budget.source,
        "gl_renderer": gl.renderer,
        "gl_version": gl.version,
        "gl_vendor": gl.vendor,
        "writes_allowed": platform::headless_writes_allowed(),
        "profile": session.profile_root.display().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headless_bench_resolve_budget_sources() {
        let budget = resolve_budget(BudgetChoice::Hz(144.0), || panic!("probe must not run"));
        assert!((budget.budget_ms - 6.944).abs() < 0.001, "{budget:?}");
        assert_eq!((budget.hz, budget.source), (144.0, "arg"));
        let budget = resolve_budget(BudgetChoice::Ms(5.0), || panic!("probe must not run"));
        assert_eq!((budget.hz, budget.budget_ms, budget.source), (200.0, 5.0, "arg"));
        let budget = resolve_budget(BudgetChoice::Auto, || Some(165.0));
        assert!((budget.budget_ms - 1000.0 / 165.0).abs() < 1e-9, "{budget:?}");
        assert_eq!((budget.hz, budget.source), (165.0, "monitor"));
        let budget = resolve_budget(BudgetChoice::Auto, || None);
        assert!((budget.budget_ms - 4.1667).abs() < 0.001, "{budget:?}");
        assert_eq!((budget.hz, budget.source), (240.0, "default"));
    }

    #[test]
    fn headless_bench_percentiles_nearest_rank() {
        // Nearest rank: the value at 1-based rank ceil(p/100 * n) of the sorted samples.
        let mut samples: Vec<f64> = (1..=100).rev().map(f64::from).collect();
        let p = percentiles(&mut samples);
        assert_eq!((p.p50, p.p95, p.p99, p.max), (50.0, 95.0, 99.0, 100.0));
        let mut three = vec![3.0, 1.0, 2.0];
        let p = percentiles(&mut three);
        assert_eq!((p.p50, p.p95, p.p99, p.max), (2.0, 3.0, 3.0, 3.0));
        let p = percentiles(&mut []);
        assert_eq!((p.p50, p.p95, p.p99, p.max), (0.0, 0.0, 0.0, 0.0));
    }

    #[test]
    fn headless_bench_parse_proc_stat_cpu_ms() {
        let stat = "4242 (rr iter) x) S 1 2 3 4 5 6 7 8 9 10 250 50 0 0 20 0 1 0";
        assert_eq!(parse_proc_stat_cpu_ms(stat, 100.0), Some(3000.0));
        for bad in ["", "nonsense", "1 (x) S 1 2 3", "1 (x) S 1 2 3 4 5 6 7 8 9 10 u 50", "1 (x S 1 2 3 4 5 6 7 8 9 10 250 50"] {
            assert_eq!(parse_proc_stat_cpu_ms(bad, 100.0), None, "{bad}");
        }
        assert_eq!(parse_proc_stat_cpu_ms(stat, 0.0), None);
    }

    #[test]
    fn headless_record_motion_stats() {
        let monotonic = motion_stats(&[0.0, 1.0, 3.0, 3.0, 6.0]);
        assert_eq!(monotonic, Motion { delta_min: 0.0, delta_max: 3.0, nonmonotonic_frames: 0 });
        // Deltas 1, 2, -1, 2: only frame 3 moves against the action's direction.
        let back = motion_stats(&[0.0, 1.0, 3.0, 2.0, 4.0]);
        assert_eq!(back, Motion { delta_min: -1.0, delta_max: 2.0, nonmonotonic_frames: 1 });
        let up = motion_stats(&[10.0, 8.0, 9.0, 5.0]);
        assert_eq!(up, Motion { delta_min: -4.0, delta_max: 1.0, nonmonotonic_frames: 1 });
        assert_eq!(motion_stats(&[]), Motion::default());
        assert_eq!(motion_stats(&[5.0]), Motion::default());
    }

    #[test]
    fn headless_bench_parse_loadavg() {
        assert_eq!(parse_loadavg("0.52 1.25 2.00 3/1024 4242\n"), Some([0.52, 1.25, 2.0]));
        assert_eq!(parse_loadavg("0.52 x"), None);
    }
}
