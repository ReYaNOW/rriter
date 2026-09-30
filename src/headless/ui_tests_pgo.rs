//! Headless `--pgo-train` runner: scenario runs, CLI parsing and exit codes.

use crate::app::automation::{AutomationOptions, PgoScenario};
use crate::headless::frame::{PgoRunOutcome, next_frame_deadline};
use crate::headless::profile::{HeadlessOptions, parse_args};
use crate::headless::tests_support::{ensure_test_profile_root, reset_api_test_state, scratch_dir};
use crate::headless::{HeadlessSession, run};
use crate::startup_trace::StartupTrace;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

const TEST_SIZE: (u32, u32) = (1280, 720);
const TEST_SCALE: f64 = 4.0 / 3.0;

static RUN_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Runs `scenario` on a fresh per-process workspace with a git fixture repository; returns
/// the outcome, the session and the report path (for state checks after the run).
fn run_pgo_session(scenario: &str, timeout_ms: u64) -> (PgoRunOutcome, HeadlessSession, PathBuf) {
    run_pgo_session_seeded(scenario, timeout_ms, |_| Vec::new())
}

/// `run_pgo_session` with a saved session: `seed` gets the workspace and returns the saved tabs.
/// Tests never read the user's session file, so they hand the tab list to `preload_ide_session`
/// (as the native `resume` does); an empty list leaves the session unseeded.
fn run_pgo_session_seeded(
    scenario: &str,
    timeout_ms: u64,
    seed: impl FnOnce(&std::path::Path) -> Vec<crate::OpenTabSnapshot>,
) -> (PgoRunOutcome, HeadlessSession, PathBuf) {
    let root = ensure_test_profile_root();
    reset_api_test_state();
    let n = RUN_COUNTER.fetch_add(1, Ordering::Relaxed);
    let workspace = scratch_dir(&format!("pgo-{}-{n}", scenario.replace(':', "-")));
    crate::app::automation::ensure_fixture_repository(&workspace).expect("fixture repository");
    let automation = AutomationOptions {
        workspace: workspace.clone(),
        report_path: workspace.join("report.json"),
        timeout: Duration::from_millis(timeout_ms),
        scenario: PgoScenario::parse(scenario).expect("known scenario"),
    };
    let options = HeadlessOptions {
        size: TEST_SIZE,
        scale: TEST_SCALE,
        allow_writes: true,
        automation: Some(automation.clone()),
        ..HeadlessOptions::default()
    };
    let mut session = match HeadlessSession::new(&options, root) {
        Ok(session) => session,
        Err((code, message)) => panic!("headless session (code {code}): {message}"),
    };
    session.hz_probe = || None;
    let saved = seed(&workspace);
    if !saved.is_empty() {
        session.app.preload_ide_session(saved, 0);
    }
    session.enter_pgo_scenario(&automation).expect("enter scenario start state");
    let outcome = session.run_automation(None);
    (outcome, session, automation.report_path)
}

/// Shared entry of the scenario tests (Tasks 3-11): runs `scenario` to its end.
pub(crate) fn run_pgo_scenario(scenario: &str, timeout_ms: u64) -> PgoRunOutcome {
    run_pgo_session(scenario, timeout_ms).0
}

fn os(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

fn automation_of(args: &[&str]) -> AutomationOptions {
    parse_args(&os(args)).expect("valid args").automation.expect("automation options")
}

#[test]
fn headless_pgo_smoke_runs_to_the_end_and_resizes_the_pbuffer() {
    let (outcome, session, _) = run_pgo_session("smoke", 20_000);
    assert!(outcome.success, "{outcome:?}");
    assert_eq!(outcome.failed_step, None);
    assert!(outcome.frames > 3, "{outcome:?}");
    // The `Resize 1600x900` step reaches the pbuffer, not only the window model.
    assert_eq!(session.gl.size(), (1600, 900));
    let window = session.app.window.as_ref().expect("window").inner_size();
    assert_eq!((window.width, window.height), (1600, 900));
}

#[test]
fn headless_pgo_report_names_the_scenario_and_frames() {
    let (outcome, _session, report_path) = run_pgo_session("smoke", 20_000);
    assert!(outcome.success, "{outcome:?}");
    let text = std::fs::read_to_string(&report_path).expect("report file");
    let report: serde_json::Value = serde_json::from_str(&text).expect("report json");
    assert_eq!(report["scenario"], "smoke");
    assert!(report["frames"].as_u64().unwrap_or(0) > 3, "{report}");
}

#[test]
fn headless_pgo_zero_timeout_fails_with_the_step_name() {
    let (outcome, _session, _) = run_pgo_session("smoke", 0);
    assert!(!outcome.success, "{outcome:?}");
    let step = outcome.failed_step.expect("failed step");
    assert!(!step.is_empty());
}

#[test]
fn headless_pgo_unknown_group_fails_and_names_itself() {
    let (outcome, _session, report_path) = run_pgo_session("group:does_not_exist", 20_000);
    assert!(!outcome.success, "{outcome:?}");
    assert_eq!(outcome.failed_step.as_deref(), Some("group:does_not_exist"));
    let report = read_report(&report_path);
    assert!(report["failure_reason"].as_str().unwrap_or("").contains("does_not_exist"), "{report}");
}

/// Three source files under the workspace, as a saved tab list.
fn three_saved_tabs(workspace: &std::path::Path) -> Vec<crate::OpenTabSnapshot> {
    let dir = workspace.join("session_seed");
    std::fs::create_dir_all(&dir).expect("seed dir");
    ["one", "two", "three"]
        .iter()
        .map(|name| {
            let path = dir.join(format!("{name}.rs"));
            std::fs::write(&path, format!("fn {name}() -> i32 {{\n    1\n}}\n")).expect("seed file");
            crate::OpenTabSnapshot::File(path.canonicalize().unwrap_or(path))
        })
        .collect()
}

#[test]
fn headless_pgo_startup_restores_the_saved_tabs_and_finishes() {
    let (outcome, session, report_path) =
        run_pgo_session_seeded("startup", 30_000, three_saved_tabs);
    assert!(outcome.success, "{outcome:?}");
    assert_eq!(outcome.failed_step, None);
    assert_eq!(session.app.tabs.len(), 3);
    assert!(!session.app.highlighter.spans.is_empty());
    let completed = read_report(&report_path)["completed_steps"].to_string();
    assert!(completed.contains("restored tabs"), "{completed}");
}

#[test]
fn headless_pgo_startup_without_a_saved_session_fails_on_restored_tabs() {
    let (outcome, _session, _) = run_pgo_session("startup", 30_000);
    assert!(!outcome.success, "{outcome:?}");
    assert_eq!(outcome.failed_step.as_deref(), Some("restored tabs"));
}

#[test]
fn headless_pgo_other_scenarios_ignore_a_saved_session() {
    let (outcome, session, _) = run_pgo_session_seeded("smoke", 20_000, three_saved_tabs);
    assert!(outcome.success, "{outcome:?}");
    assert!(session.app.tabs.len() < 3, "smoke restored {} tabs", session.app.tabs.len());
}

#[test]
fn headless_pgo_welcome_enters_the_ide_with_the_workspace() {
    let (outcome, session, _) = run_pgo_session("welcome", 30_000);
    assert!(outcome.success, "{outcome:?}");
    assert!(session.app.is_ide_mode && !session.app.show_welcome);
    assert!(!session.app.ide_panel.file_tree_nodes.is_empty());
}

fn read_report(path: &std::path::Path) -> serde_json::Value {
    let text = std::fs::read_to_string(path).expect("report file");
    serde_json::from_str(&text).expect("report json")
}

#[test]
fn headless_pgo_wait_until_never_satisfied_fails_within_its_timeout() {
    let started = Instant::now();
    let outcome = run_pgo_scenario("group:test_never", 20_000);
    assert!(!outcome.success, "{outcome:?}");
    assert_eq!(outcome.failed_step.as_deref(), Some("test-never-satisfied"));
    assert!(started.elapsed() < Duration::from_secs(5), "took {:?}", started.elapsed());
}

#[test]
fn headless_pgo_input_steps_edit_select_and_scroll_the_editor() {
    let (outcome, session, _) = run_pgo_session("group:test_input", 30_000);
    assert!(outcome.success, "{outcome:?}");
    assert!(session.app.scroll_y.target > 0.0);
    // The undo restored the file; the double click left a selection on it.
    assert!(session.app.editor.get_full_text().starts_with("line 00 = 0\n"));
    assert!(session.app.editor.selection_anchor.is_some());
}

#[test]
fn headless_pgo_group_with_failing_requires_is_skipped_and_reported() {
    let (outcome, _session, report_path) = run_pgo_session("group:test_skip", 20_000);
    assert!(outcome.success, "{outcome:?}");
    let report = read_report(&report_path);
    assert_eq!(report["status"], "success");
    assert_eq!(report["skipped_groups"], serde_json::json!([{ "group": "test_skip", "reason": "no tool" }]));
    let completed = report["completed_steps"].to_string();
    assert!(!completed.contains("test-skip-body"), "{completed}");
}

#[test]
fn headless_pgo_find_target_without_a_position_fails_the_step() {
    let (outcome, _session, report_path) = run_pgo_session("group:test_find_none", 20_000);
    assert!(!outcome.success, "{outcome:?}");
    assert_eq!(outcome.failed_step.as_deref(), Some("click"));
    let report = read_report(&report_path);
    assert!(report["failure_reason"].as_str().unwrap_or("").contains("no position"), "{report}");
}

#[test]
fn pgo_group_pdf() {
    let outcome = run_pgo_scenario("group:pdf", 60_000);
    assert!(outcome.success, "{outcome:?}");
}

#[test]
fn pgo_group_api_mock() {
    let outcome = run_pgo_scenario("group:api_mock", 60_000);
    assert!(outcome.success, "{outcome:?}");
}

#[test]
fn pgo_group_git_changes() {
    let outcome = run_pgo_scenario("group:git_changes", 90_000);
    assert!(outcome.success, "{outcome:?}");
}

#[test]
fn pgo_group_editor_ops() {
    let outcome = run_pgo_scenario("group:editor_ops", 60_000);
    assert!(outcome.success, "{outcome:?}");
}

/// With `ty` the group runs to the end; without it the group is skipped (like `skip_without_ty`
/// in the goto-definition tests) and the skip is reported.
#[test]
fn pgo_group_lsp_nav() {
    let ty_found = crate::platform::resolve_tool_executable(
        std::ffi::OsStr::new("ty"),
        "RRITER_TY_PATH",
    )
    .is_some();
    let (outcome, _session, report_path) = run_pgo_session("group:lsp_nav", 90_000);
    assert!(outcome.success, "{outcome:?}");
    let report = read_report(&report_path);
    let skipped = &report["skipped_groups"];
    if ty_found {
        assert_eq!(*skipped, serde_json::json!([]), "{report}");
        assert!(report["completed_steps"].to_string().contains("cursor at definition"), "{report}");
    } else {
        assert_eq!(*skipped, serde_json::json!([{ "group": "lsp_nav", "reason": "ty not found" }]));
    }
}

#[test]
fn headless_pgo_cli_parses_scenario_and_pgo_window_defaults() {
    let opts = parse_args(&os(&["--headless", "--pgo-train", "--pgo-scenario", "group:x"]))
        .expect("valid args");
    let automation = opts.automation.expect("automation");
    assert_eq!(automation.scenario, PgoScenario::Group("x".to_string()));
    assert_eq!(automation.timeout, Duration::from_secs(240));
    assert!(automation.report_path.ends_with("rriter-pgo-automation-report.json"));
    assert!(automation.workspace.is_absolute());
    assert!(opts.allow_writes, "--pgo-train implies --allow-writes");
    assert_eq!(opts.size, (2560, 1440));
    assert_eq!(opts.scale, 1.333);

    assert_eq!(automation_of(&["--pgo-train"]).scenario, PgoScenario::Full);
    let explicit = parse_args(&os(&[
        "--pgo-train",
        "--size",
        "800x600",
        "--scale",
        "1.5",
        "--pgo-workspace",
        "/tmp/ws",
        "--pgo-report",
        "/tmp/r.json",
        "--pgo-timeout-seconds",
        "31",
    ]))
    .expect("valid args");
    assert_eq!((explicit.size, explicit.scale), ((800, 600), 1.5));
    let automation = explicit.automation.expect("automation");
    assert_eq!(automation.workspace, PathBuf::from("/tmp/ws"));
    assert_eq!(automation.report_path, PathBuf::from("/tmp/r.json"));
    assert_eq!(automation.timeout, Duration::from_secs(31));
}

#[test]
fn headless_pgo_cli_without_pgo_train_keeps_the_interactive_defaults() {
    let opts = parse_args(&os(&["--headless"])).expect("valid args");
    assert_eq!(opts.automation, None);
    assert!(!opts.allow_writes);
    assert_eq!(opts.size, (1920, 1080));
}

#[test]
fn headless_pgo_cli_rejects_bad_arguments() {
    let cases: &[(&[&str], &str)] = &[
        (&["--pgo-train", "--pgo-scenario", "nope"], "nope"),
        (&["--pgo-train", "--pgo-scenario", "group:"], "no group name"),
        (&["--pgo-train", "--pgo-scenario"], "missing value for --pgo-scenario"),
        (&["--pgo-train", "--pgo-workspace"], "missing value for --pgo-workspace"),
        (&["--pgo-train", "--pgo-report", "--pgo-scenario", "smoke"], "missing value for --pgo-report"),
        (&["--pgo-train", "--pgo-timeout-seconds", "abc"], "--pgo-timeout-seconds"),
        (&["--pgo-train", "--pgo-timeout-seconds", "-1"], "--pgo-timeout-seconds"),
        (&["--pgo-train", "--pgo-timeout-seconds", "5"], "at least 30"),
        (&["--pgo-scenario", "smoke"], "--pgo-scenario requires --pgo-train"),
        (&["--pgo-workspace", "/tmp"], "--pgo-workspace requires --pgo-train"),
        (&["--pgo-timeout-seconds", "60"], "--pgo-timeout-seconds requires --pgo-train"),
        (&["--pgo-train", "--script", "/tmp/s.txt"], "neither --script nor"),
        (&["--pgo-train", "/tmp/project"], "neither --script nor"),
    ];
    for (args, needle) in cases {
        let err = parse_args(&os(args)).expect_err(&format!("{args:?} must be rejected"));
        assert!(err.contains(needle), "{args:?}: {err:?} does not contain {needle:?}");
    }
}

#[test]
fn headless_pgo_cli_exits_2_for_a_missing_workspace_and_bad_arguments() {
    let missing = std::env::temp_dir()
        .join(format!("rriter-pgo-no-such-workspace-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&missing);
    let missing = missing.to_string_lossy().into_owned();
    let bad_workspace = ["--headless", "--pgo-train", "--pgo-workspace", missing.as_str()];
    assert_eq!(run(&os(&bad_workspace), StartupTrace::disabled()), 2);
    let file = std::env::current_exe().expect("test binary path");
    let file = file.to_string_lossy().into_owned();
    let workspace_is_a_file = ["--headless", "--pgo-train", "--pgo-workspace", file.as_str()];
    assert_eq!(run(&os(&workspace_is_a_file), StartupTrace::disabled()), 2);
    let bad_scenario = ["--headless", "--pgo-train", "--pgo-scenario", "nope"];
    assert_eq!(run(&os(&bad_scenario), StartupTrace::disabled()), 2);
}

#[test]
fn headless_pgo_frame_deadline_keeps_the_pace_and_does_not_burst_after_a_late_frame() {
    let pace = Duration::from_millis(16);
    let start = Instant::now();
    // On time: the next frame is one pace after the previous deadline.
    assert_eq!(next_frame_deadline(start, pace, start), start + pace);
    // Late by five paces: the next frame is due now, not five frames in a row.
    let late = start + pace * 5;
    assert_eq!(next_frame_deadline(start, pace, late), late);
}
