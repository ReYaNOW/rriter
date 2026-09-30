#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#![cfg_attr(windows, allow(linker_messages))]

mod app;
mod editor;
// mod generated;
#[cfg(target_os = "linux")]
mod headless;
mod highlighter;
mod languages;
mod lsp;
mod markdown_media;
mod platform;
mod pdf;
mod queries;
mod render_view;
mod renderer;
#[cfg(test)]
mod round2_regression_tests;
#[cfg(test)]
mod round3_regression_tests;
mod scroll;
mod startup_environment;
mod startup_trace;
mod state_persistence;
mod headless_ty_mem_probe;
mod ui_system;
mod ui_waker;
mod widgets;

use crate::app::{App, AppInitOptions};
use std::time::Duration;
use winit::event_loop::{ControlFlow, EventLoop};

pub(crate) use startup_environment::*;
pub(crate) use state_persistence::*;
pub(crate) use headless_ty_mem_probe::*;

fn initial_file_argument(
    args: &[std::ffi::OsString],
    scroll_bench_idx: Option<usize>,
) -> Option<&std::ffi::OsStr> {
    if let Some(index) = scroll_bench_idx {
        return args.get(index + 1).map(std::ffi::OsString::as_os_str);
    }

    let mut index = 1;
    while let Some(argument) = args.get(index) {
        let value = argument.as_os_str();
        if value == std::ffi::OsStr::new("--ide")
            || value == std::ffi::OsStr::new("ide")
            || value == std::ffi::OsStr::new("--pgo-train")
        {
            index += 1;
            continue;
        }
        if value == std::ffi::OsStr::new("--pgo-workspace")
            || value == std::ffi::OsStr::new("--pgo-report")
            || value == std::ffi::OsStr::new("--pgo-timeout-seconds")
        {
            index += 2;
            continue;
        }
        if value.to_string_lossy().starts_with("--") {
            index += 1;
            continue;
        }
        return Some(value);
    }
    None
}

fn argument_value<'a>(args: &'a [String], flag: &str) -> Result<Option<&'a str>, String> {
    let Some(index) = args.iter().position(|argument| argument == flag) else {
        return Ok(None);
    };
    let Some(value) = args.get(index + 1) else {
        return Err(format!("{flag} requires a value"));
    };
    if value.starts_with("--") {
        return Err(format!("{flag} requires a value"));
    }
    Ok(Some(value.as_str()))
}

fn automation_options(
    args: &[String],
    initial_file: Option<&std::ffi::OsStr>,
) -> Result<Option<crate::app::automation::AutomationOptions>, String> {
    if !args.iter().any(|argument| argument == "--pgo-train") {
        return Ok(None);
    }

    let workspace = argument_value(args, "--pgo-workspace")?
        .map(std::path::PathBuf::from)
        .or_else(|| {
            initial_file
                .map(std::path::PathBuf::from)
                .and_then(|path| path.parent().map(std::path::Path::to_path_buf))
        })
        .or_else(|| std::env::current_dir().ok())
        .ok_or_else(|| "unable to determine PGO automation workspace".to_string())?;
    let report_path = argument_value(args, "--pgo-report")?
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| workspace.join("rriter-pgo-automation-report.json"));
    let timeout_seconds = argument_value(args, "--pgo-timeout-seconds")?
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| format!("invalid --pgo-timeout-seconds value: {value}"))
        })
        .transpose()?
        .unwrap_or(240);
    if timeout_seconds < 30 {
        return Err("--pgo-timeout-seconds must be at least 30".to_string());
    }

    Ok(Some(crate::app::automation::AutomationOptions {
        workspace,
        report_path,
        timeout: Duration::from_secs(timeout_seconds),
        scenario: crate::app::automation::PgoScenario::Full,
    }))
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn event_loop_error_message(stage: &str, error: &impl std::fmt::Display) -> String {
    format!("RRiter: {stage}: {error}")
}

fn main() {
    let mut startup_trace = startup_trace::StartupTrace::new();
    let startup_args = std::env::args_os().collect::<Vec<_>>();
    if let Some(exit_code) = crate::platform::handle_startup_helper(&startup_args) {
        std::process::exit(exit_code);
    }
    startup_trace.start_logo_decode(startup_trace::LOGO_PNG);
    if startup_args.iter().skip(1).any(|arg| arg == "--headless") {
        // Same as the GUI branch below: a PGO run records the frame telemetry it trains on.
        if startup_args.iter().skip(1).any(|arg| arg == "--pgo-train") {
            crate::render_view::TELEMETRY_ENABLED
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        #[cfg(target_os = "linux")]
        std::process::exit(i32::from(headless::run(&startup_args[1..], startup_trace)));
        #[cfg(not(target_os = "linux"))]
        {
            eprintln!("headless mode is supported on Linux only");
            std::process::exit(2);
        }
    }
    crate::platform::initialize_gui_application();
    prefer_egl_vendor();

    #[cfg(target_os = "linux")]
    unsafe {
        // Константа M_ARENA_MAX = -8. Настраиваем glibc напрямую,
        // так как переменные окружения читать уже поздно.
        unsafe extern "C" {
            fn mallopt(param: i32, val: i32) -> i32;
        }
        mallopt(-8, 2);
    }
    init_rayon_global_pool();

    let args = startup_args
        .iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    if let Some(idx) = args.iter().position(|arg| arg == "--probe-git-graph") {
        let Some(repo) = args.get(idx + 1) else {
            eprintln!("usage: rriter --probe-git-graph <repo-path> [iterations]");
            return;
        };
        let iterations = args
            .get(idx + 2)
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(3);
        match crate::app::git_panel::run_git_graph_probe(std::path::Path::new(repo), iterations) {
            Ok(report) => print!("{report}"),
            Err(err) => eprintln!("{err}"),
        }
        return;
    }
    if let Some(idx) = args.iter().position(|arg| arg == "--probe-project-search") {
        run_project_search_probe(&args, idx);
        return;
    }
    if let Some(idx) = args
        .iter()
        .position(|arg| arg == "--headless-ty-mem" || arg == "--probe-ty-mem")
    {
        run_headless_ty_mem_probe(&args, idx);
        return;
    }
    let scroll_bench_idx = args.iter().position(|arg| arg == "--bench-scroll-render");
    let scroll_bench_seconds = scroll_bench_idx
        .and_then(|idx| args.get(idx + 2))
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(22.0);
    let pgo_train = args.iter().any(|argument| argument == "--pgo-train");
    let run_ide_on_startup =
        scroll_bench_idx.is_some() || pgo_train || args.iter().any(|a| a == "--ide" || a == "ide");
    let initial_file_arg = initial_file_argument(&startup_args, scroll_bench_idx);
    let automation_options = match automation_options(&args, initial_file_arg) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("PGO_AUTOMATION_ARGUMENT_ERROR {error}");
            return;
        }
    };
    let has_file_arg = initial_file_arg.is_some();
    let mut initial_text = String::new();
    let mut title = "Безымянный".to_string();
    let mut ext = String::new();
    let mut file_path = None;
    let mut text_file_format = crate::platform::TextFileFormat::default();

    let mut recent_files = load_recent_files();

    if has_file_arg {
        let path = initial_file_arg.expect("has_file_arg is derived from initial_file_arg");
        if let Ok(decoded) = crate::platform::read_text_file(std::path::Path::new(path)) {
            initial_text = decoded.text;
            text_file_format = decoded.format;
            let f_path = std::path::Path::new(path);

            let abs_path = std::fs::canonicalize(f_path).unwrap_or_else(|_| f_path.to_path_buf());

            file_path = Some(abs_path.clone());
            let file_name = abs_path.file_name().unwrap_or_default().to_string_lossy();
            title = file_name.into_owned();

            if let Some(e) = abs_path.extension() {
                ext = e.to_string_lossy().to_string();
            }

            if scroll_bench_idx.is_none() {
                recent_files.retain(|path| !crate::platform::paths_equal(path, &abs_path));
                recent_files.insert(0, abs_path);
                recent_files.truncate(10);
                save_recent_files(&recent_files);
            }
        }
    }

    let editor = App::initial_editor(&initial_text);
    let mut event_loop_builder = EventLoop::<ui_waker::AppWake>::with_user_event();
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        event_loop_builder
            .with_activation_policy(ActivationPolicy::Regular)
            .with_default_menu(true)
            .with_activate_ignoring_other_apps(true);
    }
    let event_loop = match event_loop_builder.build() {
        Ok(event_loop) => event_loop,
        Err(error) => {
            eprintln!(
                "{}",
                event_loop_error_message("не удалось создать event loop", &error)
            );
            return;
        }
    };
    startup_trace.mark("event-loop");
    event_loop.set_control_flow(ControlFlow::Wait);

    let config = load_config();
    startup_trace.mark("config");
    crate::platform::configure_tool_paths(config.tool_paths.clone());
    crate::render_view::TELEMETRY_ENABLED.store(
        config.enable_telemetry || scroll_bench_idx.is_some() || pgo_train,
        std::sync::atomic::Ordering::Relaxed,
    );
    let options = AppInitOptions {
        editor: Some(editor),
        title: Some(title),
        ext: Some(ext),
        file_path,
        text_file_format: Some(text_file_format),
        recent_files: Some(recent_files),
        has_file_arg,
        run_ide_on_startup,
        automation_options,
        scroll_bench_idx,
        scroll_bench_seconds: Some(scroll_bench_seconds),
        headless: false,
        ui_waker: ui_waker::UiWaker::native(event_loop.create_proxy()),
        startup_trace,
    };
    let mut app = App::new_from_config(config, options);
    app.startup_trace.mark("app");

    if let Err(error) = event_loop.run_app(&mut app) {
        eprintln!(
            "{}",
            event_loop_error_message("event loop завершился с ошибкой", &error)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_file_argument_skips_ide_mode_and_honors_benchmark_position() {
        let normal = vec![
            std::ffi::OsString::from("rriter"),
            std::ffi::OsString::from("--ide"),
            std::ffi::OsString::from(r"C:\Work Tree\пример.py"),
        ];
        assert_eq!(
            initial_file_argument(&normal, None),
            Some(std::ffi::OsStr::new(r"C:\Work Tree\пример.py"))
        );

        let benchmark = vec![
            std::ffi::OsString::from("rriter"),
            std::ffi::OsString::from("--bench-scroll-render"),
            std::ffi::OsString::from("sample.py"),
            std::ffi::OsString::from("2"),
        ];
        assert_eq!(
            initial_file_argument(&benchmark, Some(1)),
            Some(std::ffi::OsStr::new("sample.py"))
        );

        let automation = vec![
            std::ffi::OsString::from("rriter"),
            std::ffi::OsString::from("--pgo-train"),
            std::ffi::OsString::from("--pgo-workspace"),
            std::ffi::OsString::from("fixture"),
            std::ffi::OsString::from("--pgo-report"),
            std::ffi::OsString::from("report.json"),
            std::ffi::OsString::from("fixture/src/main.rs"),
        ];
        assert_eq!(
            initial_file_argument(&automation, None),
            Some(std::ffi::OsStr::new("fixture/src/main.rs"))
        );
    }

    #[test]
    fn pgo_automation_arguments_are_validated_without_becoming_file_paths() {
        let args = vec![
            "rriter".to_string(),
            "--pgo-train".to_string(),
            "--pgo-workspace".to_string(),
            "fixture".to_string(),
            "--pgo-report".to_string(),
            "report.json".to_string(),
            "--pgo-timeout-seconds".to_string(),
            "90".to_string(),
        ];
        let options = automation_options(&args, None).unwrap().unwrap();
        assert_eq!(options.workspace, std::path::PathBuf::from("fixture"));
        assert_eq!(options.report_path, std::path::PathBuf::from("report.json"));
        assert_eq!(options.timeout, Duration::from_secs(90));

        let missing = vec![
            "rriter".to_string(),
            "--pgo-train".to_string(),
            "--pgo-workspace".to_string(),
        ];
        assert!(automation_options(&missing, None).is_err());

        let too_short = vec![
            "rriter".to_string(),
            "--pgo-train".to_string(),
            "--pgo-timeout-seconds".to_string(),
            "5".to_string(),
        ];
        assert!(automation_options(&too_short, None).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn initial_file_argument_preserves_non_utf8_native_paths() {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};

        let path = std::ffi::OsString::from_vec(b"source-\xff.py".to_vec());
        let arguments = vec![std::ffi::OsString::from("rriter"), path.clone()];
        assert_eq!(
            initial_file_argument(&arguments, None)
                .expect("native path")
                .as_bytes(),
            path.as_os_str().as_bytes()
        );
    }
}
