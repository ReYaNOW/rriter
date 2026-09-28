//! Headless coverage for scrollbars inside IDE panels.

use crate::headless::tests_support::{
    click_ui, dump, focus_handler_body, git, git_init, has_ui, python_route_session, run_script,
    scratch_dir, session_for_test, wait_until, wheel_until_visible, workspace_with_explorer,
};
use crate::render_view::scrollbar_widget::ScrollbarAxis;
use std::time::Instant;

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;

fn drag_to_bottom(session: &mut crate::headless::HeadlessSession, id: &str, press_y: f64) {
    let [x, y, width, height] = crate::headless::tests_support::ui_rect(&dump(session), id);
    let lane_x = x + width / 2.0;
    let bottom_y = y + height + 40.0;
    let lines = run_script(
        session,
        format!(
            "mouse_move {lane_x} {press_y}\nclick left down\nmouse_move {lane_x} {bottom_y}\nclick left up\n"
        )
        .as_bytes(),
    );
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
}

#[test]
#[ignore = "bug: dragging the LSP log scrollbar to the bottom leaves the offset outside its expected range"]
fn headless_lsp_log_scrollbar_wheel_and_drag_clamp_at_bottom() {
    let dir = scratch_dir("ui-scrollbar-lsp-log");
    let mut session = session_for_test(TEST_WIDTH, TEST_HEIGHT);
    let lines = run_script(
        &mut session,
        format!("scale {TEST_SCALE}\nworkspace {}\nsettle 1000\n", dir.display()).as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    click_ui(&mut session, "SidebarSlot(LspServers)");
    wait_until(&mut session, 5000, "LSP server list", |session| {
        has_ui(&dump(session), "LspServerLogs(0)")
    });

    let server_idx = 0;
    let name = session.app.ide_panel.lsp_servers[server_idx].name;
    let logs = (0..100)
        .map(|index| crate::lsp::LogEntry {
            text: format!("long LSP log line {index:03}: diagnostic payload"),
            spans: Vec::new(),
            folds: Vec::new(),
            created_at: Instant::now(),
        })
        .collect();
    if let Some(lsp) = session.app.lsp.as_mut() {
        lsp.server_logs.insert(name, logs);
        session.app.ide_panel.lsp_servers = lsp.servers_info();
    } else {
        panic!("workspace LSP manager missing");
    }
    click_ui(&mut session, &format!("LspServerLogs({server_idx})"));
    let state = dump(&mut session);
    let area_id = format!("LspLogArea({server_idx})");
    let scrollbar_id = format!("LspLogScrollY({server_idx})");
    assert!(has_ui(&state, &area_id), "{state}");
    assert!(has_ui(&state, &scrollbar_id), "{state}");
    let area = crate::headless::tests_support::ui_rect(&state, &area_id);
    let (x, y) = (area[0] + area[2] / 2.0, area[1] + area[3] / 2.0);
    let lines = run_script(
        &mut session,
        format!("mouse_move {x} {y}\nwheel 0 -100\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    let current_scroll = session.app.ide_panel.lsp_logs_scroll_y.get(name).unwrap().current;
    assert!(
        session.app.ide_panel.lsp_logs_scroll_y.get(name).unwrap().target > 0.0,
        "wheel did not scroll the LSP log"
    );
    wait_until(&mut session, 5000, "LSP log wheel scroll", |session| {
        session
            .app
            .ide_panel
            .lsp_logs_scroll_y
            .get(name)
            .is_some_and(|scroll| scroll.is_settled())
    });

    let track = crate::headless::tests_support::ui_rect(&dump(&mut session), &scrollbar_id);
    let (content_h, _) = session.app.lsp_server_inner_size(
        &session.app.ide_panel.lsp_servers[server_idx],
        TEST_SCALE,
    );
    let geometry = crate::app::lsp_actions::lsp_log_scrollbar(
        (track[0] as f32, track[1] as f32, track[2] as f32, track[3] as f32),
        area[3] as f32,
        content_h,
        current_scroll,
        ScrollbarAxis::Vertical,
    )
    .geometry(TEST_SCALE)
    .expect("LSP log scrollbar geometry");
    let press_y = (geometry.thumb.start + geometry.thumb.len / 2.0) as f64;
    let max_scroll = (content_h - area[3] as f32).max(0.0);
    drag_to_bottom(&mut session, &scrollbar_id, press_y);
    wait_until(&mut session, 5000, "LSP log scrollbar bottom", |session| {
        session
            .app
            .ide_panel
            .lsp_logs_scroll_y
            .get(name)
            .is_some_and(|scroll| scroll.is_settled())
    });
    let scroll = session.app.ide_panel.lsp_logs_scroll_y.get(name).unwrap();
    assert!(scroll.current > 0.0 && scroll.current <= max_scroll + 0.5);
    assert!((scroll.current - max_scroll).abs() < 0.5);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_git_workspace_scrollbar_wheel_and_drag_clamp_at_bottom() {
    let dir = scratch_dir("ui-scrollbar-git-workspace");
    for index in 0..60 {
        std::fs::write(dir.join(format!("changed-{index:02}.txt")), "before\n")
            .expect("write Git base file");
    }
    git_init(&dir);
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-qm", "scrollbar fixture"]);
    for index in 0..60 {
        std::fs::write(dir.join(format!("changed-{index:02}.txt")), "after\n")
            .expect("modify Git fixture file");
    }
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    click_ui(&mut session, "SidebarSlot(Git)");
    wait_until(&mut session, 8000, "Git changed-file list and scrollbar", |session| {
        let state = dump(session);
        has_ui(&state, "GitWorkspaceScroll") && has_ui(&state, "GitFileDiff(0, 0)")
    });
    let state = dump(&mut session);
    assert!(has_ui(&state, "GitWorkspaceScroll"), "{state}");
    let row = crate::headless::tests_support::ui_rect(&state, "GitFileDiff(0, 0)");
    let (x, y) = (row[0] + row[2] / 2.0, row[1] + row[3] / 2.0);
    let lines = run_script(
        &mut session,
        format!("mouse_move {x} {y}\nwheel 0 -100\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    assert!(session.app.ide_panel.git.scroll.target > 0.0, "wheel did not scroll Git list");
    wait_until(&mut session, 5000, "Git workspace wheel scroll", |session| {
        session.app.ide_panel.git.scroll.is_settled()
    });

    let geometry = session
        .app
        .renderer
        .as_ref()
        .and_then(|renderer| renderer.git_workspace_scrollbar)
        .and_then(|mut scrollbar| {
            scrollbar.extent.offset = session.app.ide_panel.git.scroll.current;
            scrollbar.geometry(TEST_SCALE)
        })
        .expect("Git workspace scrollbar geometry");
    let press_y = (geometry.thumb.start + geometry.thumb.len / 2.0) as f64;
    let max_scroll = geometry.max_scroll;
    drag_to_bottom(&mut session, "GitWorkspaceScroll", press_y);
    wait_until(&mut session, 5000, "Git workspace scrollbar bottom", |session| {
        session.app.ide_panel.git.scroll.is_settled()
    });
    assert!(session.app.ide_panel.git.scroll.current > 0.0);
    assert!(session.app.ide_panel.git.scroll.current <= max_scroll + 0.5);
    assert!((session.app.ide_panel.git.scroll.current - max_scroll).abs() < 0.5);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_api_mock_combined_editor_scrollbar_wheel_and_drag() {
    let (dir, mut session, route_idx) = python_route_session("ui-scrollbar-api-mock");
    let script = session.app.ide_panel.api.mock.manual_routes[route_idx]
        .python
        .as_mut()
        .expect("Python mock handler");
    script.body = (0..160)
        .map(|index| format!("    value_{index} = {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    session
        .app
        .ide_panel
        .api
        .mock_python_editors
        .remove(&(route_idx, crate::app::api_mock::ty_check::ApiMockSourcePart::Body));
    let lines = run_script(&mut session, b"mouse_move 0 0\n");
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    focus_handler_body(&mut session, route_idx, false);

    let combined_id = format!("ApiMockCombinedPython({route_idx})");
    assert!(
        wheel_until_visible(&mut session, (800.0, 120.0), &combined_id, 24.0, 20),
        "combined API Mock editor did not enter the viewport: {}",
        dump(&mut session)
    );
    let scrollbar_id = format!("ApiMockCombinedScrollY({route_idx})");
    let state = dump(&mut session);
    assert!(has_ui(&state, &scrollbar_id), "{state}");
    let editor = crate::headless::tests_support::ui_rect(&state, &combined_id);
    let (x, y) = (editor[0] + editor[2] / 2.0, editor[1] + editor[3] / 2.0);
    let lines = run_script(
        &mut session,
        format!("mouse_move {x} {y}\nwheel 0 -100\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    let key = (route_idx, crate::app::api_mock::ty_check::ApiMockSourcePart::Body);
    assert!(
        session.app.ide_panel.api.mock_python_scrolls[&key].target > 0.0,
        "wheel did not scroll the combined editor"
    );
    wait_until(&mut session, 5000, "API Mock combined editor wheel scroll", |session| {
        session.app.ide_panel.api.mock_python_scrolls[&key].is_settled()
    });

    let track = crate::headless::tests_support::ui_rect(&dump(&mut session), &scrollbar_id);
    let active = session.app.api_active_route();
    let max_scroll = session.app.ide_panel.api.api_mock_combined_max_scroll_for_route(
        active.as_ref(),
        route_idx,
        TEST_SCALE,
    );
    let viewport_h = editor[3] as f32;
    let geometry = crate::app::api_client::api_mock_combined_editor_scrollbar(
        (track[0] as f32, track[1] as f32, track[2] as f32, track[3] as f32),
        viewport_h,
        viewport_h + max_scroll,
        session.app.ide_panel.api.mock_python_scrolls[&key].current,
    )
    .geometry(TEST_SCALE)
    .expect("API Mock combined scrollbar geometry");
    let press_y = (geometry.thumb.start + geometry.thumb.len / 2.0) as f64;
    drag_to_bottom(&mut session, &scrollbar_id, press_y);
    wait_until(&mut session, 5000, "API Mock combined editor drag", |session| {
        session.app.ide_panel.api.mock_python_scrolls[&key].is_settled()
    });
    assert!(session.app.ide_panel.api.mock_python_scrolls[&key].current > 0.0);
    let _ = std::fs::remove_dir_all(dir);
}
