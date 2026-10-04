//! Headless coverage for Rust analyzer settings and status reporting.

use super::ui_tests_rust_lsp::rust_crate_session;
use crate::headless::tests_support::{
    click_ui, dump, has_ui, install_fake_lsp, open_settings_tab, run_script, scratch_dir,
    ui_rect, wait_until,
};
use crate::lsp::LspServerStatus;
use crate::platform::{self, ToolKind};
use std::path::{Path, PathBuf};

fn install_rust_fake(
    session: &mut crate::headless::HeadlessSession,
    name: &str,
    mode: &str,
) -> PathBuf {
    let tools = scratch_dir(&format!("{name}-tools"));
    install_fake_lsp(session, ToolKind::RustAnalyzer, &tools, mode)
}

fn open_file(session: &mut crate::headless::HeadlessSession, file: &Path) {
    let lines = run_script(session, format!("open {}\n", file.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(session, 5000, "Rust fixture tab", |session| {
        session.app.file_path.as_deref() == Some(file)
    });
}

fn rust_status(session: &crate::headless::HeadlessSession) -> Option<LspServerStatus> {
    session.app.lsp.as_ref().map(|lsp| lsp.rust_row_info().status)
}

fn problems_empty(session: &crate::headless::HeadlessSession) -> bool {
    session.app.ide_panel.flat_diags.is_empty()
}

fn open_all_problems(session: &mut crate::headless::HeadlessSession) {
    click_ui(session, "SidebarSlot(Problems)");
    wait_until(session, 5000, "Problems tabs", |session| {
        has_ui(&dump(session), "ProblemsTab(1)")
    });
    click_ui(session, "ProblemsTab(1)");
}

fn scroll_to_rust_settings(session: &mut crate::headless::HeadlessSession) {
    let lines = run_script(
        session,
        b"scale 1\n",
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    for _ in 0..20 {
        let state = dump(session);
        if has_ui(&state, "SettingsRustToggleEnabled")
            && ui_rect(&state, "SettingsRustToggleEnabled")[3] >= 29.0
        {
            return;
        }
        let lines = run_script(session, b"mouse_move 900 400\nwheel 0 -8\n");
        assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
        wait_until(session, 5000, "Rust settings scroll", |session| {
            session.app.settings_general_scroll.is_settled()
        });
    }
    let state = dump(session);
    panic!("Rust settings controls did not become visible: {state}");
}

fn init_log(executable: &Path) -> Vec<serde_json::Value> {
    let name = executable
        .file_name()
        .unwrap_or_else(|| panic!("fake executable basename"))
        .to_string_lossy();
    std::fs::read_to_string(executable.with_file_name(format!("{name}.init.jsonl")))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

struct ToolPathsReset;

impl Drop for ToolPathsReset {
    fn drop(&mut self) {
        platform::configure_tool_paths(platform::ToolPaths::default());
    }
}

#[test]
fn headless_rust_settings_toggle_and_check_command_restart() {
    let (dir, file, mut session) = rust_crate_session("rust-settings-toggle");
    let _reset = ToolPathsReset;
    let executable = install_rust_fake(
        &mut session,
        "rust-settings-toggle",
        "rust_analyzer_diagnostics_initlog",
    );
    open_file(&mut session, &file);
    open_all_problems(&mut session);
    wait_until(&mut session, 8000, "Rust diagnostics in Problems", |session| {
        !problems_empty(session) && rust_status(session) == Some(LspServerStatus::Running)
    });
    open_settings_tab(&mut session, 1);
    scroll_to_rust_settings(&mut session);

    click_ui(&mut session, "SettingsRustToggleEnabled");
    wait_until(&mut session, 8000, "Rust disabled and Problems cleared", |session| {
        rust_status(session) == Some(LspServerStatus::Disabled) && problems_empty(session)
    });
    click_ui(&mut session, "SettingsRustToggleEnabled");
    wait_until(&mut session, 8000, "Rust enabled with diagnostics", |session| {
        rust_status(session) == Some(LspServerStatus::Running) && !problems_empty(session)
    });
    click_ui(&mut session, "SettingsRustToggleCheckCommand");
    wait_until(&mut session, 8000, "Rust server restart after check command change", |session| {
        init_log(&executable).len() >= 3
    });
    let log = init_log(&executable);
    assert_eq!(log.last().and_then(|options| options["check"]["command"].as_str()), Some("check"));
    let starts = std::fs::read_to_string(executable.with_extension("starts"))
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    assert_eq!(starts, 3, "expected initial enable, re-enable, and check-command restart");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_rust_settings_row_shows_missing_tool() {
    let (dir, file, mut session) = rust_crate_session("rust-settings-missing");
    let _reset = ToolPathsReset;
    let missing = dir.join("missing-rust-analyzer");
    session.app.tool_paths.set(ToolKind::RustAnalyzer, Some(missing));
    platform::configure_tool_paths(session.app.tool_paths.clone());
    open_file(&mut session, &file);
    wait_until(&mut session, 8000, "missing Rust analyzer", |session| {
        rust_status(session) == Some(LspServerStatus::Missing)
    });
    open_settings_tab(&mut session, 1);
    let rust_tool = format!("SettingsToolPick({})", ToolKind::RustAnalyzer.index());
    let lines = run_script(
        &mut session,
        b"scale 1\nmouse_move 900 550\nwheel 0 -8\nwheel 0 -8\nwheel 0 -8\nwheel 0 -8\nwheel 0 -8\nwheel 0 -8\nwheel 0 -8\nwheel 0 -8\nwheel 0 -8\nwheel 0 -8\nwheel 0 -8\nwheel 0 -8\nwheel 0 -8\nwheel 0 -8\nwheel 0 -8\nwheel 0 -8\nwheel 0 -8\nwheel 0 -8\nwheel 0 -8\nwheel 0 -8\nwheel 0 -8\nwheel 0 -8\nwheel 0 -8\nwheel 0 -8\nsettle 2000\n",
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(&mut session, 5000, "Rust analyzer settings row", |session| {
        has_ui(&dump(session), &rust_tool)
    });
    let state = dump(&mut session);
    assert!(has_ui(&state, &rust_tool), "Rust analyzer row absent: {state}");
    assert!(
        has_ui(&state, &format!("SettingsToolInstall({})", ToolKind::RustAnalyzer.index())),
        "missing analyzer install control absent: {state}"
    );
    assert_eq!(rust_status(&session), Some(LspServerStatus::Missing));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_rust_server_status_busy_flag() {
    let (dir, file, mut session) = rust_crate_session("rust-server-status");
    let _reset = ToolPathsReset;
    install_rust_fake(&mut session, "rust-server-status", "rust_analyzer_serverstatus");
    open_file(&mut session, &file);
    wait_until(&mut session, 8000, "Rust analyzer busy status", |session| {
        session.app.lsp.as_ref().is_some_and(|lsp| {
            let info = lsp.rust_row_info();
            info.status == LspServerStatus::Running && info.busy
        })
    });
    wait_until(&mut session, 5000, "Rust analyzer quiescent status", |session| {
        session.app.lsp.as_ref().is_some_and(|lsp| !lsp.rust_row_info().busy)
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_rust_settings_picks_analyzer_override_and_starts_it() {
    let (dir, file, mut session) = rust_crate_session("rust-settings-picked-path");
    let _reset = ToolPathsReset;
    let executable = install_rust_fake(&mut session, "rust-settings-picked-path", "rust_analyzer_initlog");
    open_settings_tab(&mut session, 1);
    scroll_to_rust_settings(&mut session);
    let pick_id = format!("SettingsToolPick({})", ToolKind::RustAnalyzer.index());
    let clear_id = format!("SettingsToolClear({})", ToolKind::RustAnalyzer.index());
    assert!(has_ui(&dump(&mut session), &clear_id), "configured analyzer should expose Clear before picking: {}", dump(&mut session));
    click_ui(&mut session, &clear_id);

    session.app.external_requests.queue_picker_answer(vec![executable.clone()]);
    click_ui(&mut session, &pick_id);
    wait_until(&mut session, 5000, "Rust analyzer path picked in Settings", |session| {
        session.app.tool_paths.get(ToolKind::RustAnalyzer) == Some(executable.as_path())
    });
    let resolution = platform::resolve_tool_kind(ToolKind::RustAnalyzer);
    assert_eq!(resolution.configured_path.as_deref(), Some(executable.as_path()), "configured Rust analyzer path: {resolution:?}");
    assert_eq!(resolution.source_label(ToolKind::RustAnalyzer), Some("настройки"), "Rust analyzer source: {resolution:?}");
    assert!(has_ui(&dump(&mut session), &pick_id), "Rust analyzer Settings row missing after picker");

    open_file(&mut session, &file);
    wait_until(&mut session, 8000, "picked Rust analyzer server", |session| {
        rust_status(session) == Some(LspServerStatus::Running)
    });
    assert_eq!(std::fs::read_to_string(executable.with_extension("starts")).ok().as_deref(), Some("1"));
    assert!(!init_log(&executable).is_empty(), "picked fake server did not receive initialization");
    let _ = std::fs::remove_dir_all(dir);
}
