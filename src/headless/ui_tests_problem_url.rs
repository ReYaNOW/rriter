//! Headless UI coverage for diagnostic documentation links.

use crate::headless::tests_support::{
    click_ui, dump, has_ui, install_fake_ty, run_script, scratch_dir, session_for_test, wait_until,
};
use crate::headless::HeadlessSession;
use crate::platform::ExternalRequest;
use std::path::Path;

const DIAGNOSTIC_URL: &str = "https://docs.astral.sh/ty/rules/unresolved-reference";

fn open_diagnostic_workspace(session: &mut HeadlessSession, dir: &Path, file: &Path) {
    let lines = run_script(
        session,
        format!("scale 1.3333333\nworkspace {}\nsettle 2000\n", dir.display()).as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");

    install_fake_ty(session, dir, "fake_lsp_server_diagnostics.py");
    let lines = run_script(
        session,
        format!("open {}\nsettle 2000\n", file.display()).as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(session, 5000, "fake Ty documentation diagnostic", |session| {
        session.app.lsp.as_ref().is_some_and(|lsp| {
            lsp.instant_merged_diagnostics(file)
                .1
                .iter()
                .any(|diagnostic| diagnostic.code_href.as_deref() == Some(DIAGNOSTIC_URL))
        })
    });
}

#[test]
fn headless_problems_click_opens_diagnostic_url_in_external_request_log() {
    let dir = scratch_dir("ui-problem-url");
    let file = dir.join("main.py");
    std::fs::write(
        &file,
        "def broken_value() -> int:\n    return missing_hover_name\n",
    )
    .expect("write Python diagnostics fixture");

    let mut session = session_for_test(1280, 720);
    open_diagnostic_workspace(&mut session, &dir, &file);

    click_ui(&mut session, "SidebarSlot(Problems)");
    wait_until(&mut session, 5000, "Problems tabs", |session| {
        has_ui(&dump(session), "ProblemsTab(1)")
    });
    click_ui(&mut session, "ProblemsTab(1)");

    wait_until(&mut session, 5000, "diagnostic URL row", |session| {
        session
            .app
            .ide_panel
            .flat_diags
            .iter()
            .any(|(path, diag_idx)| {
                session
                    .app
                    .ide_panel
                    .problem_diagnostic(session.app.lsp.as_ref(), path, *diag_idx)
                    .is_some_and(|diagnostic| {
                        diagnostic.code_href.as_deref() == Some(DIAGNOSTIC_URL)
                    })
            })
    });
    let problem_url_id = session
        .app
        .ide_panel
        .flat_diags
        .iter()
        .enumerate()
        .find_map(|(ui_idx, (path, diag_idx))| {
            session
                .app
                .ide_panel
                .problem_diagnostic(session.app.lsp.as_ref(), path, *diag_idx)
                .is_some_and(|diagnostic| diagnostic.code_href.as_deref() == Some(DIAGNOSTIC_URL))
                .then(|| format!("ProblemUrl({ui_idx})"))
        })
        .expect("diagnostic URL row");
    assert!(has_ui(&dump(&mut session), &problem_url_id));

    let [x, y, width, height] = crate::headless::tests_support::ui_rect(
        &dump(&mut session),
        &problem_url_id,
    );
    let lines = run_script(
        &mut session,
        format!("mouse_move {} {}\n", x + width / 2.0, y + height / 2.0).as_bytes(),
    );
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    wait_until(&mut session, 3000, "hovered diagnostic URL", |session| {
        dump(session)["hover"]["ui"] == problem_url_id
    });
    let lines = run_script(&mut session, b"click\n");
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    assert_eq!(
        session.app.external_requests.take(),
        Some(ExternalRequest::OpenUrl(DIAGNOSTIC_URL.to_string()))
    );

    let _ = std::fs::remove_dir_all(&dir);
}
