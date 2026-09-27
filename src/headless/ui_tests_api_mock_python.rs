//! Headless API Mock regressions for routes answered by the user's Python handler.
//!
//! The handler runs through the uv-managed runtime detected by the "Python мок-сервера"
//! dialog (`ApiMockPythonManage`), so these tests need `uv` with an installed CPython 3.13.

use crate::headless::tests_support::{
    click_ui, dump, get_from_mock, has_ui, python_route_session, reset_api_test_state,
    set_handler_body, start_mock_server, stop_mock_server_from_ui, wait_for_request_log, wait_until,
    API_MOCK_TEST_LOCK,
};
use crate::headless::HeadlessSession;

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
