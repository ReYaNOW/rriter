//! Headless UI coverage for Problems filtering and file groups.

use crate::headless::tests_support::{
    click_ui, disable_python_lsp, dump, has_ui, run_script, scratch_dir, seed_lsp_diagnostics,
    wait_until, workspace_with_explorer,
};
use crate::headless::HeadlessSession;
use crate::lsp::{DiagSeverity, Diagnostic};
use std::path::PathBuf;
use std::sync::Arc;

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;

fn diagnostic(message: &'static str, severity: DiagSeverity, line: u32) -> Diagnostic {
    Diagnostic {
        start_line: line,
        start_col: 4,
        end_line: line,
        end_col: 12,
        severity,
        code: None,
        code_href: None,
        message: Arc::from(message),
        source: Some(Arc::from("headless-test")),
        tags: crate::lsp::DiagTags::NONE,
        extra: None,
    }
}

fn seeded_problems_session(
    name: &str,
    fixtures: Vec<(&str, Vec<Diagnostic>)>,
) -> (PathBuf, Vec<PathBuf>, HeadlessSession) {
    let dir = scratch_dir(name);
    let files: Vec<PathBuf> = fixtures.iter().map(|(name, _)| dir.join(name)).collect();
    for file in &files {
        std::fs::write(file, "def fixture():\n    return value\n")
            .unwrap_or_else(|err| panic!("write diagnostic fixture: {err}"));
    }

    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    disable_python_lsp(&mut session, vec![dir.clone()]);
    let mut script = String::new();
    for file in &files {
        script.push_str(&format!("open {}\n", file.display()));
    }
    let replies = run_script(&mut session, script.as_bytes());
    assert!(replies.iter().all(|line| line.starts_with("ok")), "{replies:?}");
    if let Some(active_file) = files.last() {
        wait_until(&mut session, 5000, "active diagnostic fixture", |session| {
            session.app.file_path.as_deref() == Some(active_file.as_path())
        });
    }

    let diagnostic_fixtures = files
        .iter()
        .cloned()
        .zip(fixtures.into_iter().map(|(_, diagnostics)| diagnostics))
        .collect();
    seed_lsp_diagnostics(&mut session, vec![dir.clone()], diagnostic_fixtures);
    let replies = run_script(&mut session, b"mouse_move 0 0\n");
    assert!(replies.iter().all(|line| line == "ok"), "{replies:?}");

    click_ui(&mut session, "SidebarSlot(Problems)");
    wait_until(&mut session, 5000, "Problems tabs", |session| {
        has_ui(&dump(session), "ProblemsTab(0)")
    });
    (dir, files, session)
}

fn ui_count(state: &serde_json::Value, prefix: &str) -> usize {
    state["ui"].as_array().map_or(0, |ui| {
        ui.iter()
            .filter(|element| {
                element["id"]
                    .as_str()
                    .is_some_and(|id| id.starts_with(prefix))
            })
            .count()
    })
}

#[test]
fn headless_problems_current_file_and_all_tabs_filter_diagnostics() {
    let (dir, files, mut session) = seeded_problems_session(
        "ui-problems-tabs",
        vec![
            ("first.py", vec![diagnostic("first file issue", DiagSeverity::Error, 0)]),
            ("second.py", vec![diagnostic("second file issue", DiagSeverity::Warning, 1)]),
        ],
    );
    let active = files.last().unwrap_or_else(|| panic!("active fixture path"));

    wait_until(&mut session, 5000, "current-file diagnostic", |session| {
        session.app.ide_panel.flat_diags.len() == 1
            && session.app.ide_panel.flat_diags.first()
                .is_some_and(|(path, index)| **path == **active && *index == 0)
    });
    assert!(has_ui(&dump(&mut session), "ProblemJump(0)"));
    assert!(!has_ui(&dump(&mut session), "ProblemFileToggle(0)"));

    click_ui(&mut session, "ProblemsTab(1)");
    wait_until(&mut session, 5000, "all-file diagnostic groups", |session| {
        session.app.ide_panel.flat_diags.len() == 4
    });
    let state = dump(&mut session);
    assert_eq!(ui_count(&state, "ProblemFileToggle("), 2, "file groups missing: {state}");
    assert_eq!(ui_count(&state, "ProblemJump("), 2, "diagnostic rows missing: {state}");
    assert!(session.app.ide_panel.flat_diags.iter().any(|(path, idx)| **path == **active && *idx == 0));
    assert!(files.iter().all(|path| {
        session
            .app
            .ide_panel
            .flat_diags
            .iter()
            .any(|(listed, idx)| **listed == **path && *idx == usize::MAX)
    }));

    click_ui(&mut session, "ProblemsTab(0)");
    wait_until(&mut session, 5000, "current-file tab restored", |session| {
        session.app.ide_panel.flat_diags.len() == 1
            && session.app.ide_panel.flat_diags.first()
                .is_some_and(|(path, index)| **path == **active && *index == 0)
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_problems_file_group_collapses_and_expands() {
    let (dir, _, mut session) = seeded_problems_session(
        "ui-problems-collapse",
        vec![("collapsed.py", vec![diagnostic("group issue", DiagSeverity::Error, 0)])],
    );
    click_ui(&mut session, "ProblemsTab(1)");
    wait_until(&mut session, 5000, "expanded file group", |session| {
        session.app.ide_panel.flat_diags.len() == 2
    });
    assert!(has_ui(&dump(&mut session), "ProblemJump(1)"));

    click_ui(&mut session, "ProblemFileToggle(0)");
    wait_until(&mut session, 5000, "collapsed file group", |session| {
        session.app.ide_panel.flat_diags.len() == 1
    });
    let state = dump(&mut session);
    assert!(has_ui(&state, "ProblemFileToggle(0)"));
    assert!(!has_ui(&state, "ProblemJump(1)"));

    click_ui(&mut session, "ProblemFileToggle(0)");
    wait_until(&mut session, 5000, "expanded file group restored", |session| {
        session.app.ide_panel.flat_diags.len() == 2
    });
    assert!(has_ui(&dump(&mut session), "ProblemJump(1)"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_problems_shows_two_groups_with_their_counts_and_messages() {
    let (dir, files, mut session) = seeded_problems_session(
        "ui-problems-two-groups",
        vec![
            (
                "alpha.py",
                vec![
                    diagnostic("alpha error", DiagSeverity::Error, 0),
                    diagnostic("alpha warning", DiagSeverity::Warning, 1),
                ],
            ),
            ("beta.py", vec![diagnostic("beta error", DiagSeverity::Error, 0)]),
        ],
    );
    click_ui(&mut session, "ProblemsTab(1)");
    wait_until(&mut session, 5000, "two Problems file groups", |session| {
        session.app.ide_panel.flat_diags.len() == 5
    });

    let state = dump(&mut session);
    assert_eq!(ui_count(&state, "ProblemFileToggle("), 2, "file groups missing: {state}");
    assert_eq!(ui_count(&state, "ProblemJump("), 3, "diagnostic rows missing: {state}");
    let lsp = session.app.lsp.as_ref().unwrap_or_else(|| panic!("workspace LSP manager"));
    assert_eq!(session.app.ide_panel.problem_counts(Some(lsp), &files[0]), (1, 1));
    assert_eq!(session.app.ide_panel.problem_counts(Some(lsp), &files[1]), (1, 0));
    for (path, mut expected) in [
        (&files[0], vec!["alpha error", "alpha warning"]),
        (&files[1], vec!["beta error"]),
    ] {
        let mut messages: Vec<&str> = lsp
            .diagnostic_entries_for_path(path)
            .into_iter()
            .map(|(_, diagnostic)| diagnostic.message.as_ref())
            .collect();
        messages.sort_unstable();
        expected.sort_unstable();
        assert_eq!(messages, expected, "messages for {}", path.display());
    }
    assert_eq!(
        session
            .app
            .ide_panel
            .flat_diags
            .iter()
            .filter(|(_, index)| *index == usize::MAX)
            .map(|(path, _)| path)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        2,
        "file headers should represent separate paths"
    );
    let _ = std::fs::remove_dir_all(dir);
}
