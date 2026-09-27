//! Headless API Mock route, contract, and server request regressions.

use crate::app::api_client::ApiMethod;
use crate::app::api_mock::types::{ApiMockMode, ApiMockResponse, ApiMockServerStatus};
use crate::headless::tests_support::{
    click_ui, dump, ensure_test_profile_root, has_ui, reset_api_test_state, run_script,
    scratch_dir, wait_until, wheel_until_visible, workspace_with_explorer,
};
use crate::headless::HeadlessSession;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::{mpsc, Mutex};
use std::thread;
use std::time::Duration;

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;
const PANEL_POINT: (f64, f64) = (200.0, 500.0);
const TAB_POINT: (f64, f64) = (800.0, 500.0);
const MIN_HITBOX: f64 = 24.0;

static API_MOCK_TEST_LOCK: Mutex<()> = Mutex::new(());

struct MockServerCleanup;

impl Drop for MockServerCleanup {
    fn drop(&mut self) {
        crate::app::api_mock::server::stop_api_mock_server();
    }
}

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

fn loopback_addr_from_panel_url(url: &str) -> Result<SocketAddr, String> {
    let authority = url
        .strip_prefix("http://")
        .ok_or_else(|| format!("unexpected Mock server URL: {url}"))?;
    let (_, port) = authority
        .rsplit_once(':')
        .ok_or_else(|| format!("Mock server URL has no port: {url}"))?;
    let port = port
        .parse::<u16>()
        .map_err(|error| format!("invalid Mock server port in {url}: {error}"))?;
    Ok(SocketAddr::from(([127, 0, 0, 1], port)))
}

fn start_mock_server(session: &mut HeadlessSession) -> (String, SocketAddr, MockServerCleanup) {
    // Port zero asks the OS for an unused port; the panel reports the actual bound URL.
    session.app.ide_panel.api.mock.port = 0;
    click_ui(session, "ApiMockServerToggle");
    let cleanup = MockServerCleanup;
    let mut url = None;
    wait_until(session, 5000, "API Mock server URL", |session| {
        if let Some(running_url) = session.app.ide_panel.api.mock.server_status.running_url() {
            url = Some(running_url.to_string());
            true
        } else {
            false
        }
    });
    let url = url.expect("running API Mock URL");
    let address = loopback_addr_from_panel_url(&url).expect("loopback address from panel URL");
    (url, address, cleanup)
}

fn request_mock_server(url: String, path: String) -> mpsc::Receiver<Result<(u16, String), String>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let response = (|| {
            let address = loopback_addr_from_panel_url(&url)?;
            let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))
                .map_err(|error| error.to_string())?;
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .map_err(|error| error.to_string())?;
            stream
                .set_write_timeout(Some(Duration::from_secs(2)))
                .map_err(|error| error.to_string())?;
            write!(
                stream,
                "GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
            )
            .map_err(|error| error.to_string())?;
            let mut bytes = Vec::new();
            stream
                .read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            let response = String::from_utf8_lossy(&bytes);
            let status = response
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .and_then(|status| status.parse::<u16>().ok())
                .ok_or_else(|| format!("invalid HTTP response: {response}"))?;
            let body = response
                .split_once("\r\n\r\n")
                .map(|(_, body)| body.to_string())
                .ok_or_else(|| format!("HTTP response has no body separator: {response}"))?;
            Ok((status, body))
        })();
        let _ = sender.send(response);
    });
    receiver
}

fn wait_for_mock_response(
    session: &mut HeadlessSession,
    receiver: mpsc::Receiver<Result<(u16, String), String>>,
    what: &str,
) -> (u16, String) {
    let mut response = None;
    wait_until(session, 5000, what, |_| {
        if response.is_none() {
            response = receiver.try_recv().ok();
        }
        response.is_some()
    });
    response
        .expect("HTTP worker completed")
        .unwrap_or_else(|error| panic!("Mock server request failed: {error}"))
}

fn stop_mock_server_from_ui(session: &mut HeadlessSession) {
    click_ui(session, "ApiMockServerToggle");
    wait_until(session, 5000, "API Mock server stop", |session| {
        matches!(
            session.app.ide_panel.api.mock.server_status,
            ApiMockServerStatus::Stopped
        )
    });
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

    let (url, _, _cleanup) = start_mock_server(&mut session);
    let response = wait_for_mock_response(
        &mut session,
        request_mock_server(url, "/mock-1".to_string()),
        "manual mock HTTP response",
    );
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
    let (url, _, _cleanup) = start_mock_server(&mut session);
    let response = wait_for_mock_response(
        &mut session,
        request_mock_server(url, "/no-such-route".to_string()),
        "unmatched mock HTTP response",
    );
    assert_eq!(response.0, 404);
    assert!(response.1.contains("mock route not found"), "{response:?}");
    stop_mock_server_from_ui(&mut session);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_api_mock_stop_closes_listener() {
    let _test_guard = API_MOCK_TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let (dir, mut session) = mock_session("api-mock-stop");
    let (_, address, _cleanup) = start_mock_server(&mut session);
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
