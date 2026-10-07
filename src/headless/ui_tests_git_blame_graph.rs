use crate::headless::tests_support::{
    click_ui, dump, git, git_blame_fixture, run_script, wait_until, workspace_with_explorer,
};
use std::time::Instant;

const TEST_WIDTH: u32 = 1920;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;

#[test]
fn headless_git_blame_popup_reveals_commit_beyond_first_graph_page() {
    let (root, file) = git_blame_fixture(&format!("ui-git-blame-graph-{}", std::process::id()));
    let expected_oid = git(&root, &["rev-parse", "HEAD"]).trim().to_string();
    for index in 0..202 {
        std::fs::write(root.join("history.txt"), format!("{index}\n"))
            .unwrap_or_else(|error| panic!("write graph history: {error}"));
        git(&root, &["add", "history.txt"]);
        git(
            &root,
            &[
                "-c",
                "user.name=Graph Author",
                "-c",
                "user.email=graph@example.invalid",
                "commit",
                "-qm",
                &format!("graph page {index}"),
            ],
        );
    }

    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &root);
    session.app.git_blame_inline = true;
    session.app.git_blame_delay_ms = 0;
    let lines = run_script(&mut session, format!("open {}\n", file.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(&mut session, 8000, "Git blame graph fixture", |session| {
        session.app.editor.git_blame.blame.is_some()
    });
    session.app.editor.cursor = session.app.editor.line_offsets[2];
    let now = Instant::now();
    session.app.tick_git_blame_inline(now);
    session.app.tick_git_blame_inline(now);
    let _ = run_script(&mut session, b"mouse_move 0 0\ndump\n");
    let inline = dump(&mut session)["blame_inline"].clone();
    assert!(!inline.is_null(), "annotation must fit in the wide test window");
    let x = inline["x"].as_f64().unwrap_or_default() + inline["w"].as_f64().unwrap_or_default() * 0.5;
    let y = inline["y"].as_f64().unwrap_or_default() + inline["h"].as_f64().unwrap_or_default() * 0.5;
    let lines = run_script(&mut session, format!("mouse_move {x} {y}\n").as_bytes());
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    wait_until(&mut session, 5000, "blame commit popup", |session| {
        session.app.renderer.as_ref().is_some_and(|renderer| {
            renderer.git_blame_popup_hover.is_some()
        })
    });
    wait_until(&mut session, 5000, "blame popup reveal button", |session| {
        dump(session)["ui"].as_array().is_some_and(|elements| {
            elements.iter().any(|element| element["id"] == "GitBlameShowInGraph")
        })
    });
    click_ui(&mut session, "GitBlameShowInGraph");
    assert!(session.app.renderer.as_ref().is_some_and(|renderer| {
        renderer.git_blame_popup_hover.is_none()
    }));
    wait_until(&mut session, 30000, "commit beyond first graph page", |session| {
        session.app.ide_panel.git.graph_highlight_oid.as_deref() == Some(expected_oid.as_str())
    });
    let target_index = session
        .app
        .ide_panel
        .git
        .graph_snapshot
        .iter()
        .position(|commit| commit.oid.as_ref() == expected_oid)
        .unwrap_or_else(|| panic!("revealed commit is missing from graph snapshot"));
    assert!(target_index >= 200, "expected commit beyond first page, got row {target_index}");
    wait_until(&mut session, 10000, "revealed graph row visible", |session| {
        let scale = session.app.renderer.as_ref().map_or(1.0, |renderer| renderer.scale_factor);
        let expected_scroll = target_index as f32
            * crate::app::git_panel::GIT_GRAPH_ROW_H
            * scale;
        session.app.ide_panel.git.graph_scroll.is_settled()
            && (session.app.ide_panel.git.graph_scroll.current - expected_scroll).abs() < 1.0
    });
    let _ = run_script(&mut session, b"mouse_move 0 0\ndump\n");
    let workspace_idx = session.app.ide_panel.git.graph_workspace_idx.unwrap_or_default();
    let graph_row_id = format!("GitGraphCommit({workspace_idx}, {target_index})");
    assert!(
        dump(&mut session)["ui"]
            .as_array()
            .is_some_and(|elements| elements.iter().any(|element| element["id"] == graph_row_id)),
        "revealed graph row should be in the visible UI registry"
    );
    click_ui(
        &mut session,
        &graph_row_id,
    );
    assert!(session.app.ide_panel.git.graph_highlight_oid.is_none());
    session.app.reveal_commit_in_graph(&root, "missing-commit-oid");
    assert!(session.app.ide_panel.git.graph_reveal.is_none());
    assert!(session.app.ide_panel.git.graph_snapshot.len() < 250);
    session.app.reveal_commit_in_graph(&root, &expected_oid);
    assert_eq!(
        session.app.ide_panel.git.graph_highlight_oid.as_deref(),
        Some(expected_oid.as_str())
    );
    session.app.toggle_git_graph();
    assert!(session.app.ide_panel.git.graph_highlight_oid.is_none());
}
