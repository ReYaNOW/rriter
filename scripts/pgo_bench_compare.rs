use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("pgo comparison failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--self-test") {
        self_test()?;
        println!("pgo comparison self-test passed");
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }
    let config = Config::parse(&args)?;
    require_file(&config.baseline, "baseline executable")?;
    require_file(&config.pgo, "PGO executable")?;
    require_dir(&config.workspace, "workspace")?;
    if !config.skip_scroll {
        require_file(&config.fixture, "scroll fixture")?;
    }

    let isolated = IsolatedEnvironment::new(&config.workspace)?;
    let mut reports = Vec::new();

    reports.push(binary_size_report(&config)?);
    if !config.skip_project_search {
        println!("[pgo-compare] project search: warmup={} runs={}", config.warmup, config.runs);
        let (baseline, pgo) = paired_runs_for_binaries(
            &config.baseline,
            &config.pgo,
            config.warmup,
            config.runs,
            |binary| run_project_search(binary, &config, &isolated),
        )?;
        reports.extend(reports_from_records(
            "project-search",
            &baseline,
            &pgo,
            &["worker_ms", "wall_ms", "process_ms"],
        )?);
    }

    if !config.skip_git {
        let git_repo = config.git_repo.as_deref().unwrap_or(&config.workspace);
        if git_repo.join(".git").exists() {
            println!("[pgo-compare] Git graph: warmup={} runs={}", config.warmup, config.runs);
            let (baseline, pgo) = paired_runs_for_binaries(
                &config.baseline,
                &config.pgo,
                config.warmup,
                config.runs,
                |binary| run_git_graph(binary, git_repo, &config, &isolated),
            )?;
            reports.extend(reports_from_records(
                "git-graph",
                &baseline,
                &pgo,
                &["process_ms"],
            )?);
        } else {
            println!(
                "[pgo-compare] Git graph skipped: no .git at {}",
                git_repo.display()
            );
        }
    }

    if !config.skip_scroll {
        println!(
            "[pgo-compare] scroll render: warmup={} runs={} seconds={:.1}",
            config.scroll_warmup, config.scroll_runs, config.scroll_seconds
        );
        let (baseline, pgo) = paired_runs_for_binaries(
            &config.baseline,
            &config.pgo,
            config.scroll_warmup,
            config.scroll_runs,
            |binary| run_scroll(binary, &config, &isolated),
        )?;
        reports.extend(reports_from_records(
            "scroll",
            &baseline,
            &pgo,
            &[
                "fps",
                "avg_gap_ms",
                "max_gap_ms",
                "flush_avg_ms",
                "root_prep_ms",
                "root_cache_ms",
                "root_pre_editor_ms",
                "root_overlays_ms",
                "root_chrome_ms",
                "frame_editor_ms",
                "frame_minimap_ms",
                "frame_side_ms",
                "frame_swap_ms",
                "process_ms",
            ],
        )?);
    }

    print_reports(&reports);
    if let Some(csv) = &config.csv {
        write_csv(csv, &reports)?;
        println!("[pgo-compare] CSV: {}", csv.display());
    }
    if let Some(limit) = config.fail_regression_percent {
        let regressions = reports
            .iter()
            .filter(|report| report.gain_percent < -limit)
            .collect::<Vec<_>>();
        if !regressions.is_empty() {
            return Err(format!(
                "{} metric(s) regressed beyond {:.2}%: {}",
                regressions.len(),
                limit,
                regressions
                    .iter()
                    .map(|report| report.aspect.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    Ok(())
}

#[derive(Debug)]
struct Config {
    baseline: PathBuf,
    pgo: PathBuf,
    workspace: PathBuf,
    fixture: PathBuf,
    git_repo: Option<PathBuf>,
    query: String,
    runs: usize,
    warmup: usize,
    scroll_runs: usize,
    scroll_warmup: usize,
    scroll_seconds: f32,
    skip_project_search: bool,
    skip_git: bool,
    skip_scroll: bool,
    csv: Option<PathBuf>,
    fail_regression_percent: Option<f64>,
}

impl Config {
    fn parse(args: &[OsString]) -> Result<Self, String> {
        let mut baseline = None;
        let mut pgo = None;
        let mut workspace = env::current_dir().map_err(|error| error.to_string())?;
        let mut fixture = workspace.join("tests/perf_large_realistic_15000.py");
        let mut git_repo = None;
        let mut query = "fn".to_string();
        let mut runs = 7usize;
        let mut warmup = 2usize;
        let mut scroll_runs = 2usize;
        let mut scroll_warmup = 1usize;
        let mut scroll_seconds = 12.0f32;
        let mut skip_project_search = false;
        let mut skip_git = false;
        let mut skip_scroll = false;
        let mut csv = None;
        let mut fail_regression_percent = None;
        let mut index = 0usize;

        while index < args.len() {
            let flag = args[index].to_string_lossy();
            match flag.as_ref() {
                "--baseline" => baseline = Some(take_path(args, &mut index, "--baseline")?),
                "--pgo" => pgo = Some(take_path(args, &mut index, "--pgo")?),
                "--workspace" => workspace = take_path(args, &mut index, "--workspace")?,
                "--fixture" => fixture = take_path(args, &mut index, "--fixture")?,
                "--git-repo" => git_repo = Some(take_path(args, &mut index, "--git-repo")?),
                "--query" => query = take_string(args, &mut index, "--query")?,
                "--runs" => runs = take_usize(args, &mut index, "--runs")?,
                "--warmup" => warmup = take_usize(args, &mut index, "--warmup")?,
                "--scroll-runs" => scroll_runs = take_usize(args, &mut index, "--scroll-runs")?,
                "--scroll-warmup" => {
                    scroll_warmup = take_usize(args, &mut index, "--scroll-warmup")?
                }
                "--scroll-seconds" => {
                    scroll_seconds = take_f32(args, &mut index, "--scroll-seconds")?;
                    if scroll_seconds < 10.5 {
                        return Err("--scroll-seconds must be at least 10.5 for telemetry".to_string());
                    }
                }
                "--skip-project-search" => skip_project_search = true,
                "--skip-git" => skip_git = true,
                "--skip-scroll" => skip_scroll = true,
                "--csv" => csv = Some(take_path(args, &mut index, "--csv")?),
                "--fail-regression-percent" => {
                    let value = take_f64(args, &mut index, "--fail-regression-percent")?;
                    if value < 0.0 {
                        return Err("--fail-regression-percent must be non-negative".to_string());
                    }
                    fail_regression_percent = Some(value);
                }
                unknown => return Err(format!("unknown argument: {unknown}")),
            }
            index += 1;
        }

        if runs == 0 || scroll_runs == 0 {
            return Err("measured run counts must be greater than zero".to_string());
        }
        if query.is_empty() {
            return Err("--query must not be empty".to_string());
        }
        Ok(Self {
            baseline: baseline.ok_or_else(|| "--baseline is required".to_string())?,
            pgo: pgo.ok_or_else(|| "--pgo is required".to_string())?,
            workspace,
            fixture,
            git_repo,
            query,
            runs,
            warmup,
            scroll_runs,
            scroll_warmup,
            scroll_seconds,
            skip_project_search,
            skip_git,
            skip_scroll,
            csv,
            fail_regression_percent,
        })
    }
}

struct IsolatedEnvironment {
    root: PathBuf,
    workspace_config: String,
    next_run: AtomicU64,
}

impl IsolatedEnvironment {
    fn new(workspace: &Path) -> Result<Self, String> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = env::temp_dir().join(format!(
            "rriter-pgo-bench-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).map_err(|error| {
            format!("cannot create isolated root {}: {error}", root.display())
        })?;
        let workspace_text = workspace
            .to_str()
            .ok_or_else(|| "workspace path must be valid Unicode for benchmark config".to_string())?;
        let workspace_config = format!(
            "{{\n  \"schema_version\": 3,\n  \"window_width\": 1280.0,\n  \"window_height\": 800.0,\n  \"maximized\": false,\n  \"ide_workspaces\": [\"{}\"],\n  \"ide_ignore_patterns\": [\"target\", \"target-chatgpt-test\", \"vendor\", \".git\", \".code-review-graph\"],\n  \"enable_telemetry\": false,\n  \"tool_paths\": {{}}\n}}\n",
            json_escape(workspace_text)
        );
        Ok(Self {
            root,
            workspace_config,
            next_run: AtomicU64::new(0),
        })
    }

    fn command(&self, binary: &Path, workspace: &Path) -> Result<Command, String> {
        let run_id = self.next_run.fetch_add(1, Ordering::Relaxed);
        let run_root = self.root.join(format!("run-{run_id}"));
        let xdg_config = run_root.join("xdg-config");
        let xdg_data = run_root.join("xdg-data");
        let xdg_cache = run_root.join("xdg-cache");
        let xdg_state = run_root.join("xdg-state");
        let appdata = run_root.join("AppData").join("Roaming");
        let local_appdata = run_root.join("AppData").join("Local");
        for directory in [
            &xdg_config,
            &xdg_data,
            &xdg_cache,
            &xdg_state,
            &appdata,
            &local_appdata,
        ] {
            fs::create_dir_all(directory).map_err(|error| {
                format!("cannot create isolated directory {}: {error}", directory.display())
            })?;
        }
        let config_dir = if cfg!(windows) {
            appdata.join("RRiter")
        } else if cfg!(target_os = "macos") {
            run_root
                .join("Library")
                .join("Application Support")
                .join("RRiter")
        } else {
            xdg_config.join("RRiter")
        };
        fs::create_dir_all(&config_dir).map_err(|error| {
            format!("cannot create isolated config {}: {error}", config_dir.display())
        })?;
        fs::write(config_dir.join("config.json"), &self.workspace_config)
            .map_err(|error| format!("cannot write isolated RRiter config: {error}"))?;

        let mut command = Command::new(binary);
        command
            .current_dir(workspace)
            .env("HOME", &run_root)
            .env("USERPROFILE", &run_root)
            .env("APPDATA", &appdata)
            .env("LOCALAPPDATA", &local_appdata)
            .env("XDG_CONFIG_HOME", &xdg_config)
            .env("XDG_DATA_HOME", &xdg_data)
            .env("XDG_CACHE_HOME", &xdg_cache)
            .env("XDG_STATE_HOME", &xdg_state);
        Ok(command)
    }
}

impl Drop for IsolatedEnvironment {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[derive(Clone, Debug)]
struct RunRecord {
    metrics: BTreeMap<String, f64>,
    signature: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    Lower,
    Higher,
}

#[derive(Debug)]
struct MetricReport {
    aspect: String,
    direction: Direction,
    baseline: Stats,
    pgo: Stats,
    gain_percent: f64,
}

#[derive(Clone, Copy, Debug)]
struct Stats {
    count: usize,
    median: f64,
    mean: f64,
    p95: f64,
    min: f64,
    max: f64,
}

fn paired_runs_for_binaries<F>(
    baseline_binary: &Path,
    pgo_binary: &Path,
    warmup: usize,
    measured: usize,
    mut run_one: F,
) -> Result<(Vec<RunRecord>, Vec<RunRecord>), String>
where
    F: FnMut(&Path) -> Result<RunRecord, String>,
{
    for pair in 0..warmup {
        let (first, second) = if pair % 2 == 0 {
            (baseline_binary, pgo_binary)
        } else {
            (pgo_binary, baseline_binary)
        };
        run_one(first)?;
        run_one(second)?;
    }

    let mut baseline = Vec::with_capacity(measured);
    let mut pgo = Vec::with_capacity(measured);
    for pair in 0..measured {
        let baseline_first = pair % 2 == 0;
        let first_binary = if baseline_first { baseline_binary } else { pgo_binary };
        let second_binary = if baseline_first { pgo_binary } else { baseline_binary };
        let first = run_one(first_binary)?;
        let second = run_one(second_binary)?;
        let (baseline_record, pgo_record) = if baseline_first {
            (first, second)
        } else {
            (second, first)
        };
        if baseline_record.signature != pgo_record.signature {
            return Err(format!(
                "result mismatch in pair {}: baseline={} pgo={}",
                pair + 1,
                baseline_record.signature,
                pgo_record.signature
            ));
        }
        baseline.push(baseline_record);
        pgo.push(pgo_record);
    }
    Ok((baseline, pgo))
}

fn run_project_search(
    binary: &Path,
    config: &Config,
    isolated: &IsolatedEnvironment,
) -> Result<RunRecord, String> {
    let mut command = isolated.command(binary, &config.workspace)?;
    command
        .arg("--probe-project-search")
        .arg(&config.query)
        .arg("1");
    let (output, process_ms) = capture(command, "project search")?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .find(|line| line.contains("[PROJECT SEARCH PROBE]"))
        .ok_or_else(|| format!("project search marker missing in output:\n{stdout}"))?;
    let worker_ms = key_number(line, "worker")?;
    let wall_ms = key_number(line, "wall")?;
    let result_files = key_token(line, "result_files")?;
    let matches = key_token(line, "matches")?;
    let capped = key_token(line, "capped")?;
    let error = line.split_once(" error=").map(|(_, value)| value).unwrap_or("");
    let mut metrics = BTreeMap::new();
    metrics.insert("worker_ms".to_string(), worker_ms);
    metrics.insert("wall_ms".to_string(), wall_ms);
    metrics.insert("process_ms".to_string(), process_ms);
    Ok(RunRecord {
        metrics,
        signature: format!("files={result_files};matches={matches};capped={capped};error={error}"),
    })
}

fn run_git_graph(
    binary: &Path,
    git_repo: &Path,
    config: &Config,
    isolated: &IsolatedEnvironment,
) -> Result<RunRecord, String> {
    let mut command = isolated.command(binary, &config.workspace)?;
    command.arg("--probe-git-graph").arg(git_repo).arg("1");
    let (output, process_ms) = capture(command, "Git graph")?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .find(|line| line.starts_with("iter=1 "))
        .ok_or_else(|| format!("Git graph iteration marker missing in output:\n{stdout}"))?;
    let commits = key_token(line, "commits")?;
    let lanes = key_token(line, "lanes")?;
    let has_more = key_token(line, "has_more")?;
    let mut metrics = BTreeMap::new();
    metrics.insert("process_ms".to_string(), process_ms);
    Ok(RunRecord {
        metrics,
        signature: format!("commits={commits};lanes={lanes};has_more={has_more}"),
    })
}

fn run_scroll(
    binary: &Path,
    config: &Config,
    isolated: &IsolatedEnvironment,
) -> Result<RunRecord, String> {
    let mut command = isolated.command(binary, &config.workspace)?;
    command
        .arg("--bench-scroll-render")
        .arg(&config.fixture)
        .arg(config.scroll_seconds.to_string());
    let (output, process_ms) = capture(command, "scroll render")?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.contains("SCROLL_BENCH_START") || !stdout.contains("SCROLL_BENCH_DONE") {
        return Err(format!("scroll benchmark markers missing in output:\n{stdout}"));
    }
    let mut metric_samples: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for line in stdout.lines() {
        if line.contains("Scroll present:") {
            push_metric(&mut metric_samples, "fps", metric_between(line, "Scroll present:", "FPS")?);
            push_metric(&mut metric_samples, "avg_gap_ms", named_metric(line, "Avg gap")?);
            push_metric(&mut metric_samples, "max_gap_ms", named_metric(line, "Max gap")?);
        } else if line.contains("Flush:") {
            push_metric(&mut metric_samples, "flush_avg_ms", named_metric(line, "Avg")?);
        } else if line.contains("Root phases:") {
            push_metric(&mut metric_samples, "root_prep_ms", named_metric(line, "Prep")?);
            push_metric(&mut metric_samples, "root_cache_ms", named_metric(line, "Cache")?);
            push_metric(
                &mut metric_samples,
                "root_pre_editor_ms",
                named_metric(line, "Pre-editor")?,
            );
            push_metric(
                &mut metric_samples,
                "root_overlays_ms",
                named_metric(line, "Overlays")?,
            );
            push_metric(&mut metric_samples, "root_chrome_ms", named_metric(line, "Chrome")?);
        } else if line.contains("Frame split:") {
            push_metric(&mut metric_samples, "frame_editor_ms", named_metric(line, "Editor")?);
            push_metric(
                &mut metric_samples,
                "frame_minimap_ms",
                named_metric(line, "Minimap")?,
            );
            push_metric(&mut metric_samples, "frame_side_ms", named_metric(line, "Side")?);
            push_metric(&mut metric_samples, "frame_swap_ms", named_metric(line, "Swap")?);
        }
    }
    if !metric_samples.contains_key("fps") {
        return Err(format!("scroll telemetry missing in output:\n{stdout}"));
    }
    let mut metrics = BTreeMap::new();
    for (name, values) in metric_samples {
        metrics.insert(name, median(&values)?);
    }
    metrics.insert("process_ms".to_string(), process_ms);
    Ok(RunRecord {
        metrics,
        signature: "scroll-render".to_string(),
    })
}

fn capture(mut command: Command, label: &str) -> Result<(Output, f64), String> {
    let started = Instant::now();
    let output = command
        .output()
        .map_err(|error| format!("cannot start {label}: {error}"))?;
    let elapsed = duration_ms(started.elapsed());
    if output.status.success() {
        Ok((output, elapsed))
    } else {
        Err(format!(
            "{label} exited with {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn reports_from_records(
    prefix: &str,
    baseline: &[RunRecord],
    pgo: &[RunRecord],
    metric_order: &[&str],
) -> Result<Vec<MetricReport>, String> {
    let mut reports = Vec::new();
    for metric in metric_order {
        let baseline_values = values_for_metric(baseline, metric)?;
        let pgo_values = values_for_metric(pgo, metric)?;
        let direction = metric_direction(metric);
        reports.push(make_report(
            format!("{prefix}.{metric}"),
            direction,
            &baseline_values,
            &pgo_values,
        )?);
    }
    Ok(reports)
}

fn values_for_metric(records: &[RunRecord], metric: &str) -> Result<Vec<f64>, String> {
    records
        .iter()
        .map(|record| {
            record
                .metrics
                .get(metric)
                .copied()
                .ok_or_else(|| format!("metric {metric} missing from run record"))
        })
        .collect()
}

fn metric_direction(metric: &str) -> Direction {
    if metric == "fps" {
        Direction::Higher
    } else {
        Direction::Lower
    }
}

fn binary_size_report(config: &Config) -> Result<MetricReport, String> {
    let baseline = fs::metadata(&config.baseline)
        .map_err(|error| format!("cannot stat {}: {error}", config.baseline.display()))?
        .len() as f64;
    let pgo = fs::metadata(&config.pgo)
        .map_err(|error| format!("cannot stat {}: {error}", config.pgo.display()))?
        .len() as f64;
    make_report("binary.size_bytes".to_string(), Direction::Lower, &[baseline], &[pgo])
}

fn make_report(
    aspect: String,
    direction: Direction,
    baseline_values: &[f64],
    pgo_values: &[f64],
) -> Result<MetricReport, String> {
    let baseline = stats(baseline_values)?;
    let pgo = stats(pgo_values)?;
    let gain_percent = gain_percent(baseline.median, pgo.median, direction);
    Ok(MetricReport {
        aspect,
        direction,
        baseline,
        pgo,
        gain_percent,
    })
}

fn stats(values: &[f64]) -> Result<Stats, String> {
    if values.is_empty() {
        return Err("cannot summarize empty sample set".to_string());
    }
    if values.iter().any(|value| !value.is_finite()) {
        return Err("sample set contains non-finite value".to_string());
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
    let p95_index = ((sorted.len() as f64 * 0.95).ceil() as usize)
        .saturating_sub(1)
        .min(sorted.len() - 1);
    Ok(Stats {
        count: sorted.len(),
        median: median_sorted(&sorted),
        mean,
        p95: sorted[p95_index],
        min: sorted[0],
        max: sorted[sorted.len() - 1],
    })
}

fn median(values: &[f64]) -> Result<f64, String> {
    if values.is_empty() {
        return Err("cannot calculate median of empty values".to_string());
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    Ok(median_sorted(&sorted))
}

fn median_sorted(sorted: &[f64]) -> f64 {
    let middle = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        (sorted[middle - 1] + sorted[middle]) * 0.5
    } else {
        sorted[middle]
    }
}

fn gain_percent(baseline: f64, pgo: f64, direction: Direction) -> f64 {
    if baseline == 0.0 {
        return if pgo == 0.0 { 0.0 } else { f64::NEG_INFINITY };
    }
    match direction {
        Direction::Lower => (baseline - pgo) / baseline * 100.0,
        Direction::Higher => (pgo - baseline) / baseline * 100.0,
    }
}

fn print_reports(reports: &[MetricReport]) {
    println!();
    println!(
        "{:<34} {:>13} {:>13} {:>10} {:>8} {:>8}",
        "aspect", "baseline med", "PGO med", "gain", "n", "goal"
    );
    println!("{}", "-".repeat(92));
    for report in reports {
        println!(
            "{:<34} {:>13.3} {:>13.3} {:>+9.2}% {:>8} {:>8}",
            report.aspect,
            report.baseline.median,
            report.pgo.median,
            report.gain_percent,
            report.baseline.count.min(report.pgo.count),
            match report.direction {
                Direction::Lower => "lower",
                Direction::Higher => "higher",
            }
        );
    }
    println!("\nPositive gain = PGO better. Median drives gain; CSV also contains mean/p95/min/max.");
}

fn write_csv(path: &Path, reports: &[MetricReport]) -> Result<(), String> {
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let mut file = fs::File::create(path)
        .map_err(|error| format!("cannot create CSV {}: {error}", path.display()))?;
    writeln!(
        file,
        "aspect,direction,samples,baseline_median,pgo_median,gain_percent,baseline_mean,pgo_mean,baseline_p95,pgo_p95,baseline_min,pgo_min,baseline_max,pgo_max"
    )
    .map_err(|error| error.to_string())?;
    for report in reports {
        writeln!(
            file,
            "{},{},{},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6}",
            report.aspect,
            match report.direction {
                Direction::Lower => "lower",
                Direction::Higher => "higher",
            },
            report.baseline.count.min(report.pgo.count),
            report.baseline.median,
            report.pgo.median,
            report.gain_percent,
            report.baseline.mean,
            report.pgo.mean,
            report.baseline.p95,
            report.pgo.p95,
            report.baseline.min,
            report.pgo.min,
            report.baseline.max,
            report.pgo.max,
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn key_number(line: &str, key: &str) -> Result<f64, String> {
    let token = key_token(line, key)?;
    let trimmed = token.trim_end_matches("ms");
    trimmed
        .parse::<f64>()
        .map_err(|error| format!("invalid {key} value {token:?}: {error}"))
}

fn key_token<'a>(line: &'a str, key: &str) -> Result<&'a str, String> {
    let prefix = format!("{key}=");
    line.split_whitespace()
        .find_map(|token| token.strip_prefix(&prefix))
        .ok_or_else(|| format!("missing {key}= in line: {line}"))
}

fn metric_between(line: &str, prefix: &str, suffix: &str) -> Result<f64, String> {
    let start = line
        .find(prefix)
        .map(|index| index + prefix.len())
        .ok_or_else(|| format!("missing metric prefix {prefix:?}: {line}"))?;
    let tail = &line[start..];
    let end = tail
        .find(suffix)
        .ok_or_else(|| format!("missing metric suffix {suffix:?}: {line}"))?;
    tail[..end]
        .split_whitespace()
        .last()
        .ok_or_else(|| format!("missing metric value: {line}"))?
        .parse::<f64>()
        .map_err(|error| format!("invalid metric in line {line:?}: {error}"))
}

fn named_metric(line: &str, name: &str) -> Result<f64, String> {
    let prefix = format!("{name} ");
    let start = line
        .find(&prefix)
        .map(|index| index + prefix.len())
        .ok_or_else(|| format!("missing named metric {name:?}: {line}"))?;
    let value = line[start..]
        .split_whitespace()
        .next()
        .ok_or_else(|| format!("missing value for {name:?}: {line}"))?
        .trim_end_matches("ms");
    value
        .parse::<f64>()
        .map_err(|error| format!("invalid {name:?} metric {value:?}: {error}"))
}

fn push_metric(samples: &mut BTreeMap<String, Vec<f64>>, name: &str, value: f64) {
    samples.entry(name.to_string()).or_default().push(value);
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character < '\u{20}' => {
                escaped.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped
}

fn take_string(args: &[OsString], index: &mut usize, flag: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .map(|value| value.to_string_lossy().into_owned())
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn take_path(args: &[OsString], index: &mut usize, flag: &str) -> Result<PathBuf, String> {
    *index += 1;
    args.get(*index)
        .map(PathBuf::from)
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn take_usize(args: &[OsString], index: &mut usize, flag: &str) -> Result<usize, String> {
    let value = take_string(args, index, flag)?;
    value
        .parse::<usize>()
        .map_err(|error| format!("invalid {flag} value {value:?}: {error}"))
}

fn take_f32(args: &[OsString], index: &mut usize, flag: &str) -> Result<f32, String> {
    let value = take_string(args, index, flag)?;
    value
        .parse::<f32>()
        .map_err(|error| format!("invalid {flag} value {value:?}: {error}"))
}

fn take_f64(args: &[OsString], index: &mut usize, flag: &str) -> Result<f64, String> {
    let value = take_string(args, index, flag)?;
    value
        .parse::<f64>()
        .map_err(|error| format!("invalid {flag} value {value:?}: {error}"))
}

fn require_file(path: &Path, label: &str) -> Result<(), String> {
    if path.is_file() {
        Ok(())
    } else {
        Err(format!("{label} not found: {}", path.display()))
    }
}

fn require_dir(path: &Path, label: &str) -> Result<(), String> {
    if path.is_dir() {
        Ok(())
    } else {
        Err(format!("{label} not found: {}", path.display()))
    }
}

fn self_test() -> Result<(), String> {
    let project = "[PROJECT SEARCH PROBE] run=1 wall=42ms worker=39ms result_files=12 matches=88 capped=false error=";
    if key_number(project, "worker")? != 39.0
        || key_token(project, "matches")? != "88"
        || key_token(project, "capped")? != "false"
    {
        return Err("project-search parser self-test failed".to_string());
    }
    let scroll = "📊 Scroll present: 244 FPS | Avg gap 4.10ms | Max gap 9.20ms | Frames 100";
    if metric_between(scroll, "Scroll present:", "FPS")? != 244.0
        || named_metric(scroll, "Avg gap")? != 4.10
        || named_metric(scroll, "Max gap")? != 9.20
    {
        return Err("scroll parser self-test failed".to_string());
    }
    let root = "📊 Root phases: Prep 0.10ms | Cache 0.20ms | Pre-editor 0.30ms | Overlays 0.40ms | Chrome 0.50ms";
    if named_metric(root, "Pre-editor")? != 0.30 || named_metric(root, "Chrome")? != 0.50 {
        return Err("root-phase parser self-test failed".to_string());
    }
    let summarized = stats(&[5.0, 1.0, 3.0, 2.0, 4.0])?;
    if summarized.median != 3.0 || summarized.mean != 3.0 || summarized.p95 != 5.0 {
        return Err("statistics self-test failed".to_string());
    }
    if (gain_percent(10.0, 8.0, Direction::Lower) - 20.0).abs() > f64::EPSILON
        || (gain_percent(100.0, 110.0, Direction::Higher) - 10.0).abs() > f64::EPSILON
    {
        return Err("gain calculation self-test failed".to_string());
    }
    if json_escape("C:\\A\"B\n") != "C:\\\\A\\\"B\\n" {
        return Err("JSON escaping self-test failed".to_string());
    }
    let baseline = Path::new("baseline");
    let pgo = Path::new("pgo");
    let (base, optimized) = paired_runs_for_binaries(baseline, pgo, 1, 2, |binary| {
        let value = if binary == baseline { 10.0 } else { 8.0 };
        Ok(RunRecord {
            metrics: BTreeMap::from([("process_ms".to_string(), value)]),
            signature: "same".to_string(),
        })
    })?;
    if base.len() != 2 || optimized.len() != 2 {
        return Err("paired ordering self-test failed".to_string());
    }
    Ok(())
}

fn print_help() {
    println!(
        "RRiter baseline/PGO comparison tool\n\
Usage: pgo-bench-compare --baseline PATH --pgo PATH [options]\n\n\
Options:\n\
  --workspace PATH              controlled project-search workspace\n\
  --fixture PATH                real scroll-render source fixture\n\
  --git-repo PATH               repository for Git graph probe\n\
  --query TEXT                  project-search query (default: fn)\n\
  --runs N                      measured headless pairs (default: 7)\n\
  --warmup N                    headless warmup pairs (default: 2)\n\
  --scroll-runs N               measured real-window pairs (default: 2)\n\
  --scroll-warmup N             scroll warmup pairs (default: 1)\n\
  --scroll-seconds N            seconds per scroll process, >=10.5\n\
  --skip-project-search         omit project-search metrics\n\
  --skip-git                    omit Git graph metric\n\
  --skip-scroll                 omit window/render metrics\n\
  --csv PATH                    write machine-readable report\n\
  --fail-regression-percent N   fail if any metric is worse by more than N\n\
  --self-test                   validate parsers/stats without RRiter binaries\n\n\
Positive gain always means PGO is better. Pair order alternates AB/BA.\n"
    );
}
