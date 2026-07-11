use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("scroll benchmark failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let root = env::current_dir().map_err(|error| error.to_string())?;
    let mut args = env::args().skip(1);
    let fixture = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("tests/perf_large_realistic_15000.py"));
    let seconds = args.next().unwrap_or_else(|| "22".to_string());
    let binary = root.join("target/x86_64-unknown-linux-gnu/release/rriter");

    require_file(&binary, "RRiter binary")?;
    require_file(&fixture, "fixture")?;

    let output = Command::new(&binary)
        .arg("--bench-scroll-render")
        .arg(&fixture)
        .arg(&seconds)
        .output()
        .map_err(|error| format!("cannot run {}: {error}", binary.display()))?;

    print!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!("{}", String::from_utf8_lossy(&output.stderr));

    if !output.status.success() {
        return Err(format!("editor exited with {}", output.status));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.contains("SCROLL_BENCH_START") || !stdout.contains("SCROLL_BENCH_DONE") {
        return Err("editor benchmark markers missing".to_string());
    }
    if !stdout.contains("Scroll present:") || !stdout.contains("Root phases:") {
        return Err("real frame telemetry missing".to_string());
    }
    if !stdout.contains("Flush:") {
        return Err("flush telemetry missing".to_string());
    }
    if !stdout.contains("Chrome detail:") {
        return Err("IDE chrome telemetry missing".to_string());
    }

    let fps_samples = metric_values(&stdout, "Scroll present:", "FPS")?;
    let status_samples = metric_values(&stdout, "Bottom-status", "ms")?;
    if fps_samples.len() < 2 || status_samples.len() < 2 {
        return Err("expected two scroll benchmark phases".to_string());
    }
    let min_fps = fps_samples.iter().copied().fold(f64::INFINITY, f64::min);
    let max_status_ms = status_samples.iter().copied().fold(0.0, f64::max);
    let required_fps = env_f64("RRITER_SCROLL_BENCH_MIN_FPS", 230.0);
    let allowed_status_ms = env_f64("RRITER_SCROLL_BENCH_MAX_STATUS_MS", 0.5);
    println!(
        "SCROLL_BENCH_ASSERT min_fps={min_fps:.0} required_fps={required_fps:.0} max_status_ms={max_status_ms:.2} allowed_status_ms={allowed_status_ms:.2}"
    );
    if min_fps < required_fps {
        return Err(format!("FPS regression: {min_fps:.0} < {required_fps:.0}"));
    }
    if max_status_ms > allowed_status_ms {
        return Err(format!(
            "status-bar regression: {max_status_ms:.2}ms > {allowed_status_ms:.2}ms"
        ));
    }

    Ok(())
}

fn metric_values(text: &str, prefix: &str, suffix: &str) -> Result<Vec<f64>, String> {
    let mut values = Vec::new();
    for line in text.lines().filter(|line| line.contains(prefix)) {
        let start = line
            .find(prefix)
            .map(|index| index + prefix.len())
            .ok_or_else(|| format!("missing metric prefix: {prefix}"))?;
        let tail = &line[start..];
        let end = tail
            .find(suffix)
            .ok_or_else(|| format!("missing metric suffix {suffix}: {line}"))?;
        let token = tail[..end]
            .split_whitespace()
            .last()
            .ok_or_else(|| format!("missing metric value: {line}"))?;
        values.push(
            token
                .parse::<f64>()
                .map_err(|error| format!("invalid metric value {token}: {error}"))?,
        );
    }
    Ok(values)
}

fn env_f64(name: &str, default: f64) -> f64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn require_file(path: &Path, label: &str) -> Result<(), String> {
    if path.is_file() {
        Ok(())
    } else {
        Err(format!("{label} not found: {}", path.display()))
    }
}
