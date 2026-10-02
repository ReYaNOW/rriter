//! Headless UI coverage for the Problems panel.

use crate::headless::tests_support::{
    click_ui, disable_python_lsp, dump, has_ui, run_script, scratch_dir, seed_lsp_diagnostics,
    wait_until, workspace_with_explorer,
};
use crate::headless::HeadlessSession;
use crate::lsp::{DiagSeverity, Diagnostic};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;
const PROBLEM_MESSAGE: &str = "Undefined name `missing_problems_name`";

fn problem_diagnostic() -> Diagnostic {
    Diagnostic {
        start_line: 1,
        start_col: 11,
        end_line: 1,
        end_col: 32,
        severity: DiagSeverity::Error,
        code: Some(Arc::from("F821")),
        code_href: None,
        message: Arc::from(PROBLEM_MESSAGE),
        source: Some(Arc::from("ruff")),
        tags: crate::lsp::DiagTags::NONE,
        extra: None,
    }
}

fn python_problem_session(name: &str) -> (PathBuf, PathBuf, HeadlessSession) {
    let dir = scratch_dir(name);
    let file = dir.join("main.py");
    std::fs::write(&file, "def calculate():\n    return missing_problems_name\n")
        .expect("write Python diagnostic fixture");

    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    disable_python_lsp(&mut session, vec![dir.clone()]);

    let lines = run_script(&mut session, format!("open {}\n", file.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(&mut session, 5000, "Python fixture tab", |session| {
        session.app.file_path.as_deref() == Some(file.as_path())
    });

    seed_lsp_diagnostics(
        &mut session,
        vec![dir.clone()],
        vec![(file.clone(), vec![problem_diagnostic()])],
    );

    click_ui(&mut session, "SidebarSlot(Problems)");
    wait_until(&mut session, 5000, "Problems tabs", |session| {
        has_ui(&dump(session), "ProblemsTab(1)")
    });
    click_ui(&mut session, "ProblemsTab(1)");
    wait_until(&mut session, 5000, "Python problem row", |session| {
        let state = dump(session);
        has_ui(&state, "ProblemFileToggle(0)") && has_ui(&state, "ProblemJump(1)")
    });

    (dir, file, session)
}

fn diagnostic_for<'a>(session: &'a HeadlessSession, path: &Path) -> &'a Diagnostic {
    session
        .app
        .lsp
        .as_ref()
        .and_then(|lsp| lsp.diagnostic_at(path, 0))
        .expect("injected Python diagnostic")
}

#[test]
fn headless_problems_lists_python_file_line_and_message() {
    let (dir, file, mut session) = python_problem_session("ui-problems-list");
    let state = dump(&mut session);

    let shared = std::sync::Arc::<std::path::Path>::from(file.as_path());
    assert_eq!(
        session.app.ide_panel.flat_diags,
        vec![
            crate::app::ProblemRow::group_header(shared.clone()),
            crate::app::ProblemRow::with_diagnostic_source(
                shared,
                0,
                std::sync::Arc::from(vec![problem_diagnostic()]),
                0,
            ),
        ]
    );
    assert!(has_ui(&state, "ProblemFileToggle(0)"), "file group missing: {state}");
    assert!(has_ui(&state, "ProblemJump(1)"), "diagnostic row missing: {state}");
    assert_eq!(diagnostic_for(&session, &file).start_line + 1, 2);
    assert_eq!(diagnostic_for(&session, &file).message.as_ref(), PROBLEM_MESSAGE);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_problems_click_jumps_to_diagnostic_line() {
    let (dir, file, mut session) = python_problem_session("ui-problems-jump");

    click_ui(&mut session, "ProblemJump(1)");
    let state = dump(&mut session);
    assert_eq!(state["tabs"][0]["path"].as_str(), Some(file.to_str().unwrap()));
    assert_eq!(state["tabs"][0]["cursor"]["line"].as_u64(), Some(2), "{state}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_problems_clears_row_when_diagnostics_are_cleared() {
    let (dir, file, mut session) = python_problem_session("ui-problems-clear");
    assert!(has_ui(&dump(&mut session), "ProblemJump(1)"));

    let lsp = session.app.lsp.as_mut().expect("workspace LSP manager");
    lsp.diagnostics.remove(&file);
    lsp.dirty_diagnostics = true;
    wait_until(&mut session, 5000, "cleared Python problem row", |session| {
        session.app.ide_panel.flat_diags.is_empty()
    });

    let state = dump(&mut session);
    assert!(!has_ui(&state, "ProblemJump(1)"), "stale diagnostic row: {state}");
    assert!(!has_ui(&state, "ProblemFileToggle(0)"), "stale file group: {state}");
    let _ = std::fs::remove_dir_all(dir);
}
