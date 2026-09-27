//! Headless API Mock route, contract, and server request regressions.

use crate::app::api_client::ApiMethod;
use crate::app::api_mock::types::{ApiMockMode, ApiMockResponse, ApiMockServerStatus};
use crate::headless::tests_support::{
    click_ui, dump, ensure_test_profile_root, get_from_mock, has_ui, loopback_addr_from_panel_url,
    reset_api_test_state, run_script, scratch_dir, start_mock_server, stop_mock_server_from_ui,
    wait_until, wheel_until_visible, workspace_with_explorer, API_MOCK_TEST_LOCK,
};
use crate::headless::HeadlessSession;
use std::net::TcpStream;
use std::time::Duration;

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;
const PANEL_POINT: (f64, f64) = (200.0, 500.0);
const TAB_POINT: (f64, f64) = (800.0, 500.0);
const MIN_HITBOX: f64 = 24.0;

fn mock_session(name: &str) -> (std::path::PathBuf, HeadlessSession) {
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
    (dir, session)
}

fn add_manual_route(session: &mut HeadlessSession) -> usize {
    click_ui(session, "ApiMockAddManualRoute");
    session.app.ide_panel.api.mock.manual_routes.len() - 1
}

fn reveal_manual_route(session: &mut HeadlessSession, route_idx: usize) {
    let path_id = format!("ApiMockManualRoutePath({route_idx})");
    assert!(
        wheel_until_visible(session, PANEL_POINT, &path_id, MIN_HITBOX, 30),
        "{path_id} did not enter the API Mock list: {}",
        dump(session)
    );
}

fn edit_manual_route_path(session: &mut HeadlessSession, route_idx: usize, path: &str) {
    reveal_manual_route(session, route_idx);
    let path_id = format!("ApiMockManualRoutePath({route_idx})");
    click_ui(session, &path_id);
    let lines = run_script(session, format!("key ctrl+a\ntype {path}\n").as_bytes());
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    click_ui(session, &format!("ApiMockManualRouteOpen({route_idx})"));
}

#[test]
fn headless_api_mock_manual_route_can_be_edited_in_list() {
    let _test_guard = API_MOCK_TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let (dir, mut session) = mock_session("api-mock-route-edit");
    let route_idx = add_manual_route(&mut session);
    edit_manual_route_path(&mut session, route_idx, "/products/42");
    click_ui(&mut session, &format!("ApiMockManualRouteMethod({route_idx})"));

    let route = &session.app.ide_panel.api.mock.manual_routes[route_idx];
    assert_eq!(route.path, "/products/42");
    assert_eq!(route.method, ApiMethod::Post);
    let state = dump(&mut session);
    assert!(has_ui(&state, &format!("ApiMockManualRoutePath({route_idx})")), "{state}");
    assert!(state["tabs"][0]["title"].as_str().unwrap_or_default().contains("/products/42"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_api_mock_server_returns_manual_route_response() {
    let _test_guard = API_MOCK_TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let (dir, mut session) = mock_session("api-mock-request");
    let route_idx = add_manual_route(&mut session);
    let body = r#"{"source":"rriter-headless"}"#;
    click_ui(&mut session, &format!("ApiMockStaticResponseInput({route_idx})"));
    let lines = run_script(&mut session, format!("key ctrl+a\ntype {body}\n").as_bytes());
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");

    let url = start_mock_server(&mut session);
    let response = get_from_mock(&mut session, &url, "/mock-1");
    assert_eq!(response.0, 200);
    assert!(response.1.contains("rriter-headless"), "{response:?}");
    assert!(matches!(
        &session.app.ide_panel.api.mock.manual_routes[route_idx].response,
        ApiMockResponse::Json(text) if text == body
    ));
    stop_mock_server_from_ui(&mut session);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_api_mock_unknown_path_returns_404() {
    let _test_guard = API_MOCK_TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let (dir, mut session) = mock_session("api-mock-not-found");
    // The default "mock selected, proxy the rest" mode proxies unmatched paths (502 without
    // a proxy URL); only "mock all" answers them with 404.
    assert!(
        wheel_until_visible(&mut session, PANEL_POINT, "ApiMockModeSelect", MIN_HITBOX, 20),
        "ApiMockModeSelect did not enter the API Mock panel: {}",
        dump(&mut session)
    );
    if session.app.ide_panel.api.mock.mode.canonical() != ApiMockMode::MockAll {
        click_ui(&mut session, "ApiMockModeSelect");
    }
    assert_eq!(session.app.ide_panel.api.mock.mode, ApiMockMode::MockAll);
    let url = start_mock_server(&mut session);
    let response = get_from_mock(&mut session, &url, "/no-such-route");
    assert_eq!(response.0, 404);
    assert!(response.1.contains("mock route not found"), "{response:?}");
    stop_mock_server_from_ui(&mut session);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_api_mock_stop_closes_listener() {
    let _test_guard = API_MOCK_TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let (dir, mut session) = mock_session("api-mock-stop");
    let url = start_mock_server(&mut session);
    let address = loopback_addr_from_panel_url(&url).expect("loopback address from panel URL");
    assert!(TcpStream::connect_timeout(&address, Duration::from_secs(1)).is_ok());

    click_ui(&mut session, "ApiMockServerToggle");
    wait_until(&mut session, 5000, "API Mock listener to close", |session| {
        matches!(
            session.app.ide_panel.api.mock.server_status,
            ApiMockServerStatus::Stopped
        ) && TcpStream::connect_timeout(&address, Duration::from_millis(50)).is_err()
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_api_mock_python_contract_controls_follow_route_path() {
    let _test_guard = API_MOCK_TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let (dir, mut session) = mock_session("api-mock-python-contract");
    let route_idx = add_manual_route(&mut session);
    click_ui(&mut session, &format!("ApiMockRoutePythonToggle({route_idx})"));
    edit_manual_route_path(&mut session, route_idx, "/users/{id}");

    let query_id = format!("ApiMockContractQueryToggle({route_idx})");
    assert!(
        wheel_until_visible(&mut session, TAB_POINT, &query_id, MIN_HITBOX, 20),
        "{query_id} did not enter the API tab viewport: {}",
        dump(&mut session)
    );
    click_ui(&mut session, &query_id);
    click_ui(&mut session, &format!("ApiMockContractBodyToggle({route_idx})"));

    let route = &session.app.ide_panel.api.mock.manual_routes[route_idx];
    let script = route.python.as_ref().expect("Python handler controls");
    assert!(script.enabled);
    assert!(script.contract.path_params.enabled);
    assert_eq!(script.contract.path_params.fields[0].name, "id");
    assert!(script.contract.query.enabled);
    assert!(script.contract.body.enabled);
    let state = dump(&mut session);
    assert!(has_ui(&state, &format!("ApiMockContractPathFieldToggle({route_idx}, 0)")), "{state}");
    // The enabled query/body groups push the combined handler editor below the viewport.
    let combined_id = format!("ApiMockCombinedPython({route_idx})");
    assert!(
        wheel_until_visible(&mut session, TAB_POINT, &combined_id, MIN_HITBOX, 20),
        "{combined_id} did not enter the API tab viewport: {}",
        dump(&mut session)
    );
    let _ = std::fs::remove_dir_all(dir);
}
