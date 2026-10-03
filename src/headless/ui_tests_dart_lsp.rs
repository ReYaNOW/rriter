//! Headless characterization of the Dart LSP lifecycle (start on open, stop on last close,
//! per-root independence, live diagnostics, close/reopen during an analyzer job). These pin
//! today's behaviour so moving Dart onto a shared per-root layer cannot change it silently.
//!
//! Workspace analysis is switched off in every scenario: the real `dart analyze` job would run
//! the fake server binary as an analyzer at a moment the test cannot control. Job-dependent
//! scenarios install their own job through `LspManager::dart_hold_job_for_test`.

use crate::headless::HeadlessSession;
use crate::headless::tests_support::{
    click_ui, dump, has_ui, install_fake_lsp, run_script, scratch_dir, session_for_test, ui_center,
    wait_until,
};
use crate::lsp::LspServerStatus;
use crate::platform::ToolKind;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Writes `root/pubspec.yaml` and `root/lib/main.dart`; returns the Dart file.
fn dart_package(root: &Path) -> PathBuf {
    std::fs::create_dir_all(root.join("lib")).expect("create Dart package dirs");
    std::fs::write(root.join("pubspec.yaml"), "name: fixture\n").expect("write pubspec");
    let file = root.join("lib").join("main.dart");
    std::fs::write(&file, "void main() {\n  print('hi');\n}\n").expect("write Dart file");
    file
}

/// Workspace session with the fake Dart server (`basename` picks its mode) installed outside
/// the workspace and workspace analysis off.
fn dart_session(name: &str, workspace: &Path, basename: &str) -> (HeadlessSession, PathBuf) {
    let tools = scratch_dir(&format!("{name}-tools"));
    let mut session = session_for_test(1280, 720);
    session.app.dart_settings.workspace_analysis = false;
    let lines = run_script(
        &mut session,
        format!("workspace {}\nsettle 2000\n", workspace.display()).as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    if let Some(lsp) = session.app.lsp.as_mut() {
        lsp.set_dart_workspace_analysis_enabled(false);
    }
    let executable = install_fake_lsp(&mut session, ToolKind::Dart, &tools, basename);
    (session, executable)
}

fn open_file(session: &mut HeadlessSession, file: &Path) {
    let lines = run_script(session, format!("open {}\n", file.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(session, 5000, "Dart tab to become active", |session| {
        session.app.file_path.as_deref() == Some(file)
            && session.app.lsp.is_some()
    });
}

fn close_tab(session: &mut HeadlessSession, index: usize) {
    let (x, y) = ui_center(&dump(session), &format!("EditorTab({index})"));
    run_script(session, format!("mouse_move {x} {y}\nwait 50\n").as_bytes());
    click_ui(session, &format!("EditorTabClose({index})"));
}

fn dart_status(session: &HeadlessSession) -> Option<LspServerStatus> {
    session.app.lsp.as_ref().map(crate::lsp::LspManager::dart_status)
}

fn root_has_process(session: &HeadlessSession, root: &Path) -> Option<bool> {
    session
        .app
        .lsp
        .as_ref()
        .and_then(|lsp| lsp.dart_root_state_for_test(root))
        .map(|state| state.process)
}

fn root_has_job(session: &HeadlessSession, root: &Path) -> Option<bool> {
    session
        .app
        .lsp
        .as_ref()
        .and_then(|lsp| lsp.dart_root_state_for_test(root))
        .map(|state| state.job)
}

/// Number of `initialize` requests the fake server received (one per LSP server start; the
/// `.starts` counter would also count analyzer-style runs of the same binary).
fn lsp_initializations(executable: &Path) -> usize {
    let name = executable.file_name().expect("fake server name").to_string_lossy();
    std::fs::read_to_string(executable.with_file_name(format!("{name}.init.jsonl")))
        .map(|log| log.lines().count())
        .unwrap_or(0)
}

#[test]
fn headless_dart_lifecycle_starts_on_open_and_stops_on_last_close() {
    let root = scratch_dir("ui-dart-lifecycle");
    let file = dart_package(&root);
    let (mut session, _exe) = dart_session("ui-dart-lifecycle", &root, "dart");

    assert_ne!(dart_status(&session), Some(LspServerStatus::Running));
    open_file(&mut session, &file);
    wait_until(&mut session, 8000, "Dart server Running", |session| {
        dart_status(session) == Some(LspServerStatus::Running)
    });
    assert_eq!(root_has_process(&session, &root), Some(true));

    close_tab(&mut session, 0);
    wait_until(&mut session, 5000, "Dart server Disabled after last close", |session| {
        dart_status(session) == Some(LspServerStatus::Disabled)
    });
    assert_eq!(root_has_process(&session, &root), Some(false));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn headless_dart_process_survives_close_during_analyzer_job() {
    let root = scratch_dir("ui-dart-job-close");
    let file = dart_package(&root);
    let (mut session, _exe) = dart_session("ui-dart-job-close", &root, "dart");

    open_file(&mut session, &file);
    wait_until(&mut session, 8000, "Dart server Running", |session| {
        dart_status(session) == Some(LspServerStatus::Running)
    });
    // The real analyzer job finishes in milliseconds against the fake binary, so a held job
    // stands in for a slow `dart analyze`.
    let held = session
        .app
        .lsp
        .as_mut()
        .is_some_and(|lsp| lsp.dart_hold_job_for_test(&root, Duration::from_millis(2500)));
    assert!(held, "job was not installed");
    assert_eq!(root_has_job(&session, &root), Some(true));

    close_tab(&mut session, 0);
    wait_until(&mut session, 3000, "tab closed", |session| {
        session.app.tabs.is_empty()
    });
    assert_eq!(root_has_job(&session, &root), Some(true), "job must still be running");
    assert_eq!(
        root_has_process(&session, &root),
        Some(true),
        "closing the last tab during a job must keep the process"
    );

    wait_until(&mut session, 8000, "job to finish and stop the server", |session| {
        root_has_job(session, &root) == Some(false)
            && root_has_process(session, &root) == Some(false)
    });
    wait_until(&mut session, 5000, "Dart status Disabled after job", |session| {
        dart_status(session) == Some(LspServerStatus::Disabled)
    });

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn headless_dart_reopen_during_analyzer_job_keeps_the_same_process() {
    let root = scratch_dir("ui-dart-job-reopen");
    let file = dart_package(&root);
    let (mut session, exe) = dart_session("ui-dart-job-reopen", &root, "dart_initlog");

    open_file(&mut session, &file);
    wait_until(&mut session, 8000, "Dart server Running", |session| {
        dart_status(session) == Some(LspServerStatus::Running)
    });
    assert_eq!(lsp_initializations(&exe), 1);
    let held = session
        .app
        .lsp
        .as_mut()
        .is_some_and(|lsp| lsp.dart_hold_job_for_test(&root, Duration::from_millis(3000)));
    assert!(held, "job was not installed");

    close_tab(&mut session, 0);
    wait_until(&mut session, 3000, "tab closed", |session| {
        session.app.tabs.is_empty()
    });
    assert_eq!(root_has_process(&session, &root), Some(true));
    open_file(&mut session, &file);
    assert_eq!(root_has_process(&session, &root), Some(true));
    assert_eq!(root_has_job(&session, &root), Some(true), "job must outlive the reopen");

    wait_until(&mut session, 8000, "job to finish", |session| {
        root_has_job(session, &root) == Some(false)
    });
    // The root is open again, so the finished job must not stop the server.
    run_script(&mut session, b"wait 300\n");
    assert_eq!(root_has_process(&session, &root), Some(true));
    assert_eq!(dart_status(&session), Some(LspServerStatus::Running));
    assert_eq!(lsp_initializations(&exe), 1, "reopen during a job must not restart the server");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn headless_dart_two_roots_are_independent() {
    let parent = scratch_dir("ui-dart-two-roots");
    let first_root = parent.join("first");
    let second_root = parent.join("second");
    let first = dart_package(&first_root);
    let second = dart_package(&second_root);
    let (mut session, _exe) = dart_session("ui-dart-two-roots", &parent, "dart");

    open_file(&mut session, &first);
    open_file(&mut session, &second);
    wait_until(&mut session, 8000, "both Dart servers started", |session| {
        root_has_process(session, &first_root) == Some(true)
            && root_has_process(session, &second_root) == Some(true)
    });
    assert_eq!(
        session.app.lsp.as_ref().map(|lsp| lsp.dart_workspace_count_for_test()),
        Some(2)
    );
    wait_until(&mut session, 8000, "Dart status Running", |session| {
        dart_status(session) == Some(LspServerStatus::Running)
    });

    // Seeded: `App::notify_lsp_tab_close` forwards only Python tabs, so no UI path closes a
    // non-last Dart tab in the LSP (see the ignored bug test below).
    session
        .app
        .lsp
        .as_mut()
        .expect("workspace LSP manager")
        .notify_close(&first, "dart");
    assert_eq!(root_has_process(&session, &first_root), Some(false));
    assert_eq!(root_has_process(&session, &second_root), Some(true));
    assert_eq!(dart_status(&session), Some(LspServerStatus::Running));

    let _ = std::fs::remove_dir_all(&parent);
}

#[test]
fn headless_dart_closing_non_last_tab_stops_its_root() {
    let parent = scratch_dir("ui-dart-non-last-close");
    let first_root = parent.join("first");
    let second_root = parent.join("second");
    let first = dart_package(&first_root);
    let second = dart_package(&second_root);
    let (mut session, _exe) = dart_session("ui-dart-non-last-close", &parent, "dart");

    open_file(&mut session, &first);
    open_file(&mut session, &second);
    wait_until(&mut session, 8000, "both Dart servers started", |session| {
        root_has_process(session, &first_root) == Some(true)
            && root_has_process(session, &second_root) == Some(true)
    });
    close_tab(&mut session, 0);
    wait_until(&mut session, 5000, "first root stopped", |session| {
        root_has_process(session, &first_root) == Some(false)
    });
    assert_eq!(root_has_process(&session, &second_root), Some(true));

    let _ = std::fs::remove_dir_all(&parent);
}

#[test]
fn headless_dart_live_diagnostics_reach_problems() {
    let root = scratch_dir("ui-dart-diagnostics");
    let file = dart_package(&root);
    let (mut session, _exe) = dart_session("ui-dart-diagnostics", &root, "dart_diagnostics");

    open_file(&mut session, &file);
    click_ui(&mut session, "SidebarSlot(Problems)");
    wait_until(&mut session, 5000, "Problems tabs", |session| {
        has_ui(&dump(session), "ProblemsTab(1)")
    });
    click_ui(&mut session, "ProblemsTab(1)");
    wait_until(&mut session, 8000, "Dart live diagnostic row", |session| {
        has_ui(&dump(session), "ProblemJump(1)")
    });

    close_tab(&mut session, 0);
    wait_until(&mut session, 5000, "diagnostic row removed after close", |session| {
        !has_ui(&dump(session), "ProblemJump(1)")
    });

    let _ = std::fs::remove_dir_all(&root);
}
