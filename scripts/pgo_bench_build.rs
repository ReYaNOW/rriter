use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const DEFAULT_PROFILE: &str = "target/pgo-profiles/merged.profdata";
const DEFAULT_OUTPUT: &str = "target/pgo-compare";
const COMPARE_SOURCE: &str = "scripts/pgo_bench_compare.rs";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("pgo benchmark build failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--self-test") {
        self_test()?;
        println!("pgo benchmark build self-test passed");
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }

    let root = env::current_dir().map_err(|error| format!("cannot read current dir: {error}"))?;
    require_file(&root.join("Cargo.toml"), "Cargo.toml")?;
    require_file(&root.join(COMPARE_SOURCE), "comparison script")?;
    let config = Config::parse(&root, &args)?;
    require_file(&config.profile, "merged PGO profile")?;

    fs::create_dir_all(&config.output_dir).map_err(|error| {
        format!(
            "cannot create output directory {}: {error}",
            config.output_dir.display()
        )
    })?;

    let baseline = config.output_dir.join("baseline").join(executable_name());
    let pgo = config.output_dir.join("pgo").join(executable_name());
    build_variant(&root, &config, Variant::Baseline, &baseline)?;
    build_variant(&root, &config, Variant::Pgo, &pgo)?;

    let compare_binary = compile_compare_tool(&root, &config)?;
    println!("[pgo-build] baseline: {}", baseline.display());
    println!("[pgo-build] pgo: {}", pgo.display());
    println!("[pgo-build] compare tool: {}", compare_binary.display());

    let compare_args = compare_arguments(&config, &baseline, &pgo);
    if config.run_compare {
        run_command(
            command_with_args(&compare_binary, compare_args.iter().map(OsString::from)),
            "PGO comparison",
        )?;
    } else {
        println!(
            "[pgo-build] next: {} {}",
            display_path(&compare_binary),
            compare_args
                .iter()
                .map(|value| shellish_quote(value))
                .collect::<Vec<_>>()
                .join(" ")
        );
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Variant {
    Baseline,
    Pgo,
}

impl Variant {
    fn label(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Pgo => "pgo",
        }
    }
}

#[derive(Debug)]
struct Config {
    profile: PathBuf,
    output_dir: PathBuf,
    target: String,
    cargo: PathBuf,
    rustc: PathBuf,
    toolchain: Option<String>,
    rustflags: Vec<String>,
    build_std: bool,
    run_compare: bool,
    workspace: PathBuf,
    fixture: PathBuf,
    compare_runs: usize,
    compare_warmup: usize,
    scroll_runs: usize,
    scroll_warmup: usize,
    scroll_seconds: f32,
    skip_scroll: bool,
    extra_compare_args: Vec<String>,
}

impl Config {
    fn parse(root: &Path, args: &[OsString]) -> Result<Self, String> {
        let mut profile = root.join(DEFAULT_PROFILE);
        let mut output_dir = root.join(DEFAULT_OUTPUT);
        let mut target = None;
        let mut cargo = PathBuf::from("cargo");
        let mut rustc = PathBuf::from("rustc");
        let mut toolchain = Some("+nightly".to_string());
        let mut rustflags = default_rustflags();
        let mut custom_rustflags = false;
        let mut build_std = false;
        let mut run_compare = false;
        let mut workspace = root.to_path_buf();
        let mut fixture = root.join("tests/perf_large_realistic_15000.py");
        let mut compare_runs = 7usize;
        let mut compare_warmup = 2usize;
        let mut scroll_runs = 2usize;
        let mut scroll_warmup = 1usize;
        let mut scroll_seconds = 12.0f32;
        let mut skip_scroll = false;
        let mut extra_compare_args = Vec::new();
        let mut index = 0usize;

        while index < args.len() {
            let flag = args[index].to_string_lossy();
            if let Some(value) = flag.strip_prefix("--rustflag=") {
                if value.is_empty() {
                    return Err("--rustflag= requires a value".to_string());
                }
                if !custom_rustflags {
                    rustflags.clear();
                    custom_rustflags = true;
                }
                rustflags.push(value.to_string());
                index += 1;
                continue;
            }
            match flag.as_ref() {
                "--profile" => profile = root_relative(root, take_path(args, &mut index, "--profile")?),
                "--out-dir" => {
                    output_dir = root_relative(root, take_path(args, &mut index, "--out-dir")?)
                }
                "--target" => target = Some(take_string(args, &mut index, "--target")?),
                "--cargo" => cargo = take_path(args, &mut index, "--cargo")?,
                "--rustc" => rustc = take_path(args, &mut index, "--rustc")?,
                "--toolchain" => {
                    let value = take_string(args, &mut index, "--toolchain")?;
                    toolchain = if value == "none" { None } else { Some(value) };
                }
                "--rustflag" => {
                    let value = take_string(args, &mut index, "--rustflag")?;
                    if !custom_rustflags {
                        rustflags.clear();
                        custom_rustflags = true;
                    }
                    rustflags.push(value);
                }
                "--build-std" => build_std = true,
                "--run" => run_compare = true,
                "--workspace" => {
                    workspace = root_relative(root, take_path(args, &mut index, "--workspace")?)
                }
                "--fixture" => {
                    fixture = root_relative(root, take_path(args, &mut index, "--fixture")?)
                }
                "--runs" => compare_runs = take_usize(args, &mut index, "--runs")?,
                "--warmup" => compare_warmup = take_usize(args, &mut index, "--warmup")?,
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
                "--skip-scroll" => skip_scroll = true,
                "--compare-arg" => {
                    extra_compare_args.push(take_string(args, &mut index, "--compare-arg")?)
                }
                unknown => return Err(format!("unknown argument: {unknown}")),
            }
            index += 1;
        }

        if compare_runs == 0 || scroll_runs == 0 {
            return Err("measured run counts must be greater than zero".to_string());
        }
        let target = match target {
            Some(target) => target,
            None => host_target(&rustc)?,
        };
        Ok(Self {
            profile,
            output_dir,
            target,
            cargo,
            rustc,
            toolchain,
            rustflags,
            build_std,
            run_compare,
            workspace,
            fixture,
            compare_runs,
            compare_warmup,
            scroll_runs,
            scroll_warmup,
            scroll_seconds,
            skip_scroll,
            extra_compare_args,
        })
    }
}

fn default_rustflags() -> Vec<String> {
    vec![
        "-Ctarget-cpu=native".to_string(),
        "-Cllvm-args=-fp-contract=fast".to_string(),
        "-Csymbol-mangling-version=v0".to_string(),
    ]
}

fn build_variant(
    root: &Path,
    config: &Config,
    variant: Variant,
    stable_output: &Path,
) -> Result<(), String> {
    let cargo_target_dir = config
        .output_dir
        .join(format!("cargo-{}", variant.label()));
    let mut flags = config.rustflags.clone();
    if variant == Variant::Pgo {
        flags.push(format!("-Cprofile-use={}", config.profile.display()));
        flags.push("-Cllvm-args=-pgo-warn-missing-function".to_string());
    }
    let encoded_flags = flags.join("\u{1f}");
    let mut command = Command::new(&config.cargo);
    if let Some(toolchain) = &config.toolchain {
        command.arg(toolchain);
    }
    command.arg("build");
    if config.build_std {
        command.arg("-Z").arg("build-std=core,alloc,std,panic_abort,test");
    }
    command
        .arg("--locked")
        .arg("--release")
        .arg("--target")
        .arg(&config.target)
        .arg("--bin")
        .arg("rriter")
        .current_dir(root)
        .env("CARGO_TARGET_DIR", &cargo_target_dir)
        .env("CARGO_PROFILE_RELEASE_CODEGEN_UNITS", "1")
        .env("CARGO_PROFILE_RELEASE_LTO", "fat")
        .env("CARGO_PROFILE_RELEASE_PANIC", "immediate-abort")
        .env("CARGO_PROFILE_RELEASE_INCREMENTAL", "false")
        .env("CARGO_ENCODED_RUSTFLAGS", encoded_flags)
        .env_remove("RUSTFLAGS");
    println!(
        "[pgo-build] building {} with target {}",
        variant.label(),
        config.target
    );
    run_command(command, &format!("{} build", variant.label()))?;

    let built = cargo_target_dir
        .join(&config.target)
        .join("release")
        .join(executable_name());
    require_file(&built, &format!("{} executable", variant.label()))?;
    if let Some(parent) = stable_output.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!("cannot create {}: {error}", parent.display())
        })?;
    }
    fs::copy(&built, stable_output).map_err(|error| {
        format!(
            "cannot copy {} to {}: {error}",
            built.display(),
            stable_output.display()
        )
    })?;
    Ok(())
}

fn compile_compare_tool(root: &Path, config: &Config) -> Result<PathBuf, String> {
    let output = config.output_dir.join(tool_name("pgo-bench-compare"));
    let mut command = Command::new(&config.rustc);
    command
        .arg("--edition=2021")
        .arg("-O")
        .arg(root.join(COMPARE_SOURCE))
        .arg("-o")
        .arg(&output)
        .current_dir(root);
    run_command(command, "comparison tool build")?;
    require_file(&output, "comparison tool")?;
    Ok(output)
}

fn compare_arguments(config: &Config, baseline: &Path, pgo: &Path) -> Vec<String> {
    let mut args = vec![
        "--baseline".to_string(),
        baseline.display().to_string(),
        "--pgo".to_string(),
        pgo.display().to_string(),
        "--workspace".to_string(),
        config.workspace.display().to_string(),
        "--fixture".to_string(),
        config.fixture.display().to_string(),
        "--runs".to_string(),
        config.compare_runs.to_string(),
        "--warmup".to_string(),
        config.compare_warmup.to_string(),
        "--scroll-runs".to_string(),
        config.scroll_runs.to_string(),
        "--scroll-warmup".to_string(),
        config.scroll_warmup.to_string(),
        "--scroll-seconds".to_string(),
        config.scroll_seconds.to_string(),
        "--csv".to_string(),
        config.output_dir.join("report.csv").display().to_string(),
    ];
    if config.skip_scroll {
        args.push("--skip-scroll".to_string());
    }
    args.extend(config.extra_compare_args.iter().cloned());
    args
}

fn command_with_args(
    executable: &Path,
    args: impl IntoIterator<Item = OsString>,
) -> Command {
    let mut command = Command::new(executable);
    command.args(args);
    command
}

fn run_command(mut command: Command, label: &str) -> Result<(), String> {
    let status = command
        .status()
        .map_err(|error| format!("cannot start {label}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{label} exited with {status}"))
    }
}

fn host_target(rustc: &Path) -> Result<String, String> {
    let output = Command::new(rustc)
        .arg("-vV")
        .output()
        .map_err(|error| format!("cannot run {} -vV: {error}", rustc.display()))?;
    if !output.status.success() {
        return Err(format!("{} -vV exited with {}", rustc.display(), output.status));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(str::to_string)
        .ok_or_else(|| "rustc -vV did not report host target".to_string())
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

fn root_relative(root: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

fn require_file(path: &Path, label: &str) -> Result<(), String> {
    if path.is_file() {
        Ok(())
    } else {
        Err(format!("{label} not found: {}", path.display()))
    }
}

fn executable_name() -> &'static str {
    if cfg!(windows) { "rriter.exe" } else { "rriter" }
}

fn tool_name(stem: &str) -> String {
    if cfg!(windows) {
        format!("{stem}.exe")
    } else {
        stem.to_string()
    }
}

fn display_path(path: &Path) -> String {
    shellish_quote(&path.display().to_string())
}

fn shellish_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-._/:\\".contains(character))
    {
        value.to_string()
    } else {
        format!("{:?}", value)
    }
}

fn self_test() -> Result<(), String> {
    let flags = default_rustflags();
    if flags.len() != 3 || !flags.iter().any(|flag| flag == "-Ctarget-cpu=native") {
        return Err("default Rust flags self-test failed".to_string());
    }
    let root = Path::new("/tmp/rriter test");
    if root_relative(root, PathBuf::from("target/profile")) != root.join("target/profile") {
        return Err("relative path self-test failed".to_string());
    }
    let config = Config {
        profile: root.join(DEFAULT_PROFILE),
        output_dir: root.join(DEFAULT_OUTPUT),
        target: "x86_64-unknown-linux-gnu".to_string(),
        cargo: PathBuf::from("cargo"),
        rustc: PathBuf::from("rustc"),
        toolchain: Some("+nightly".to_string()),
        rustflags: flags,
        build_std: true,
        run_compare: false,
        workspace: root.to_path_buf(),
        fixture: root.join("fixture.py"),
        compare_runs: 5,
        compare_warmup: 1,
        scroll_runs: 2,
        scroll_warmup: 1,
        scroll_seconds: 12.0,
        skip_scroll: true,
        extra_compare_args: vec!["--skip-git".to_string()],
    };
    let args = compare_arguments(
        &config,
        &config.output_dir.join("baseline/rriter"),
        &config.output_dir.join("pgo/rriter"),
    );
    if !args.iter().any(|arg| arg == "--skip-scroll")
        || !args.iter().any(|arg| arg == "--skip-git")
        || !args.windows(2).any(|pair| pair == ["--runs", "5"])
    {
        return Err("comparison argument self-test failed".to_string());
    }
    if Variant::Pgo.label() != "pgo" || Variant::Baseline.label() != "baseline" {
        return Err("variant label self-test failed".to_string());
    }
    Ok(())
}

fn print_help() {
    println!(
        "RRiter baseline/PGO build helper\n\
Usage: rustc --edition=2021 -O scripts/pgo_bench_build.rs -o <tool>\n\
       <tool> [options]\n\n\
Options:\n\
  --profile PATH          merged.profdata path (default: {DEFAULT_PROFILE})\n\
  --out-dir PATH          isolated output root (default: {DEFAULT_OUTPUT})\n\
  --target TRIPLE         Rust target; default comes from rustc -vV\n\
  --cargo PATH            Cargo executable (default: cargo)\n\
  --rustc PATH            rustc executable (default: rustc)\n\
  --toolchain VALUE       Cargo toolchain token (default: +nightly; none disables)\n\
  --rustflag FLAG         replace defaults with repeated exact rustc flags\n\
  --build-std             build core/alloc/std/panic_abort/test from nightly source\n\
  --run                   run comparison after both builds\n\
  --workspace PATH        deterministic project-search/Git fixture root\n\
  --fixture PATH          scroll-render fixture\n\
  --runs N                measured headless pairs (default: 7)\n\
  --warmup N              headless warmup pairs (default: 2)\n\
  --scroll-runs N         measured scroll pairs (default: 2)\n\
  --scroll-warmup N       scroll warmup pairs (default: 1)\n\
  --scroll-seconds N      seconds per scroll process, >=10.5 (default: 12)\n\
  --skip-scroll           skip real-window render comparison\n\
  --compare-arg VALUE     append one raw argument to comparison tool\n\
  --self-test             validate script logic without building\n"
    );
}
