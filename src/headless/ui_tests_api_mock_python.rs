//! Headless API Mock regressions for routes answered by the user's Python handler.
//!
//! The handler runs through the uv-managed runtime detected by the "Python мок-сервера"
//! dialog (`ApiMockPythonManage`), so these tests need `uv` with an installed CPython 3.13.

use crate::headless::tests_support::{
    click_ui, dump, ensure_test_profile_root, get_from_mock, has_ui, reset_api_test_state,
    run_script, scratch_dir, start_mock_server, stop_mock_server_from_ui, ui_rect, wait_until,
    workspace_with_explorer, API_MOCK_TEST_LOCK,
};
use crate::headless::HeadlessSession;

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;
/// Top of the API tab: wheel here scrolls the tab, not the combined Python editor.
const TAB_TOP_POINT: (f64, f64) = (800.0, 120.0);
const MIN_HITBOX: f64 = 24.0;

/// API panel with one manual route whose Python handler is enabled and a detected uv runtime.
fn python_route_session(name: &str) -> (std::path::PathBuf, HeadlessSession, usize) {
    ensure_test_profile_root();
    reset_api_test_state();
    let dir = scratch_dir(name);
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    if dump(&mut session)["ide_panel"]["active"] != "api" {
        click_ui(&mut session, "SidebarSlot(ApiClient)");
    }
    wait_until(&mut session, 5000, "API Mock controls", |session| {
        has_ui(&dump(session), "ApiMockAddManualRoute")
    });
    click_ui(&mut session, "ApiMockAddManualRoute");
    let route_idx = session.app.ide_panel.api.mock.manual_routes.len() - 1;
    click_ui(&mut session, &format!("ApiMockRoutePythonToggle({route_idx})"));
    // Opening the runtime dialog detects uv when no path is configured yet.
    click_ui(&mut session, "ApiMockPythonManage");
    wait_until(&mut session, 2000, "Python runtime dialog", |session| {
        has_ui(&dump(session), "ApiMockPythonManageClose")
    });
    click_ui(&mut session, "ApiMockPythonManageClose");
    assert!(
        session.app.ide_panel.api.mock.uv.selected_uv_path().is_some(),
        "uv was not detected: {:?}",
        session.app.ide_panel.api.mock.uv
    );
    (dir, session, route_idx)
}

/// Scrolls the tab until the handler section sits in the upper part of the combined
/// editor, hovers it (its inputs register only under the cursor) and clicks the body.
fn focus_handler_body(session: &mut HeadlessSession, route_idx: usize) {
    let reset_id = format!("ApiMockBodyReset({route_idx})");
    let body_id = format!("ApiMockBodyInput({route_idx})");
    let mut reset_rect = None;
    for _ in 0..20 {
        let state = dump(session);
        if has_ui(&state, &reset_id) {
            let rect = ui_rect(&state, &reset_id);
            if rect[3] >= MIN_HITBOX && rect[1] < TEST_HEIGHT as f64 * 0.7 {
                reset_rect = Some(rect);
                break;
            }
        }
        let (x, y) = TAB_TOP_POINT;
        let lines = run_script(session, format!("mouse_move {x} {y}\nwheel 0 -1\nsettle 200\n").as_bytes());
        assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    }
    let [_, reset_y, _, _] = reset_rect
        .unwrap_or_else(|| panic!("{reset_id} did not reach the upper tab area: {}", dump(session)));
    let lines = run_script(session, format!("mouse_move {} {}\n", TAB_TOP_POINT.0, reset_y + 60.0).as_bytes());
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    let state = dump(session);
    assert!(has_ui(&state, &body_id), "{body_id} not registered under the cursor: {state}");
    click_ui(session, &body_id);
}

/// Replaces the handler body through the editor and commits it with Escape.
fn set_handler_body(session: &mut HeadlessSession, route_idx: usize, body: &str) {
    focus_handler_body(session, route_idx);
    let lines = run_script(session, format!("key ctrl+a\ntype {body}\nkey escape\n").as_bytes());
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    let script = session.app.ide_panel.api.mock.manual_routes[route_idx]
        .python
        .as_ref()
        .expect("Python handler");
    assert_eq!(script.body.trim(), body);
}

fn wait_for_request_log(session: &mut HeadlessSession, line: &str) {
    wait_until(session, 5000, line, |session| {
        session.app.ide_panel.api.mock_server_logs.iter().any(|log| log.text.ends_with(line))
    });
}

#[test]
fn headless_api_mock_python_handler_result_is_served() {
    let _test_guard = API_MOCK_TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let (dir, mut session, route_idx) = python_route_session("api-mock-python-result");
    let path = session.app.ide_panel.api.mock.manual_routes[route_idx].path.clone();
    set_handler_body(&mut session, route_idx, r#"return text_response("from-python", status=201)"#);

    let (url, _cleanup) = start_mock_server(&mut session);
    let response = get_from_mock(&mut session, &url, &path);
    assert_eq!(response, (201, "from-python".to_string()));
    wait_for_request_log(&mut session, &format!("GET {path} -> 201 · python"));
    stop_mock_server_from_ui(&mut session);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_api_mock_python_exception_returns_500_and_shows_in_log() {
    let _test_guard = API_MOCK_TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let (dir, mut session, route_idx) = python_route_session("api-mock-python-raise");
    let path = session.app.ide_panel.api.mock.manual_routes[route_idx].path.clone();
    set_handler_body(&mut session, route_idx, r#"raise ValueError("boom-from-handler")"#);

    let (url, _cleanup) = start_mock_server(&mut session);
    let response = get_from_mock(&mut session, &url, &path);
    assert_eq!(response.0, 500);
    assert!(response.1.contains("boom-from-handler"), "{response:?}");
    wait_for_request_log(&mut session, &format!("GET {path} -> 500 · python"));
    // The failed request is visible to the user in the server status and log dialog.
    click_ui(&mut session, "ApiMockServerDetails");
    let state = dump(&mut session);
    assert!(has_ui(&state, "ApiMockServerLogArea"), "{state}");
    click_ui(&mut session, "ApiMockServerDetailsClose");
    stop_mock_server_from_ui(&mut session);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_api_mock_python_syntax_error_returns_500() {
    let _test_guard = API_MOCK_TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let (dir, mut session, route_idx) = python_route_session("api-mock-python-syntax");
    let path = session.app.ide_panel.api.mock.manual_routes[route_idx].path.clone();
    set_handler_body(&mut session, route_idx, "return text_response(");

    let (url, _cleanup) = start_mock_server(&mut session);
    let response = get_from_mock(&mut session, &url, &path);
    assert_eq!(response.0, 500);
    assert!(response.1.contains("was never closed"), "{response:?}");
    wait_for_request_log(&mut session, &format!("GET {path} -> 500 · python"));
    stop_mock_server_from_ui(&mut session);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_api_mock_python_edit_changes_response_after_restart() {
    let _test_guard = API_MOCK_TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let (dir, mut session, route_idx) = python_route_session("api-mock-python-edit-restart");
    let path = session.app.ide_panel.api.mock.manual_routes[route_idx].path.clone();
    set_handler_body(&mut session, route_idx, r#"return text_response("first", status=201)"#);
    let (url, _cleanup) = start_mock_server(&mut session);
    assert_eq!(get_from_mock(&mut session, &url, &path), (201, "first".to_string()));
    stop_mock_server_from_ui(&mut session);

    set_handler_body(&mut session, route_idx, r#"return json_response({"n": 1}, status=202)"#);
    let (url, _cleanup) = start_mock_server(&mut session);
    assert_eq!(get_from_mock(&mut session, &url, &path), (202, r#"{"n":1}"#.to_string()));
    stop_mock_server_from_ui(&mut session);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_api_mock_python_edit_hot_updates_running_server() {
    let _test_guard = API_MOCK_TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let (dir, mut session, route_idx) = python_route_session("api-mock-python-hot-edit");
    let path = session.app.ide_panel.api.mock.manual_routes[route_idx].path.clone();
    let (url, _cleanup) = start_mock_server(&mut session);
    assert_eq!(get_from_mock(&mut session, &url, &path), (200, r#"{"ok":true}"#.to_string()));

    set_handler_body(&mut session, route_idx, r#"return text_response("edited", status=201)"#);
    assert_eq!(get_from_mock(&mut session, &url, &path), (201, "edited".to_string()));
    stop_mock_server_from_ui(&mut session);
    let _ = std::fs::remove_dir_all(dir);
}
