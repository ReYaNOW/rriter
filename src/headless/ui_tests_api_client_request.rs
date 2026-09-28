//! Headless API Client request and response regressions.

use crate::app::api_client::ApiClientRouteIdentity;
use crate::headless::tests_support::{
    click_ui, dump, ensure_test_profile_root, reset_api_test_state, run_script,
    scratch_dir, send_request, serve_api_spec, read_http_request, workspace_session,
    install_spec, wait_until, wheel_until_visible,
};
use crate::headless::HeadlessSession;
use std::io::Write;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::thread;

/// Cursor over the API side panel (x 64..384 at this size and scale).
const PANEL_POINT: (f64, f64) = (200.0, 500.0);
/// Cursor over the API endpoint tab body.
const TAB_POINT: (f64, f64) = (800.0, 400.0);
/// Smallest hitbox accepted as "in the viewport" for rows, inputs and buttons.
const MIN_HITBOX: f64 = 24.0;

fn serve_spec(server: &str) -> String {
    serve_api_spec(server, serde_json::json!({
        "/get": {"get": {"responses": {"200": {"description": "ok"}}}},
        "/post": {"post": {
            "requestBody": {"required": true, "content": {
                "application/json": {"schema": {"type": "object"}}
            }},
            "responses": {"201": {"description": "created"}}
        }},
        "/fail": {"get": {"responses": {"500": {"description": "failure"}}}}
    }))
}

fn request_server(status: u16, body: &'static str) -> (String, Receiver<String>) {
    request_server_delayed(status, body, std::time::Duration::ZERO)
}

/// `request_server` that holds the reply for `delay` after reading the request.
fn request_server_delayed(
    status: u16,
    body: &'static str,
    delay: std::time::Duration,
) -> (String, Receiver<String>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind request test server");
    let address = listener.local_addr().expect("request server address");
    let (tx, rx) = mpsc::channel();
    listener.set_nonblocking(true).expect("set accept nonblocking");
    thread::spawn(move || {
        // The server starts before the session (workspace, spec import, scrolling to the
        // endpoint), which under a parallel test run can take well over ten seconds.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let (mut stream, request) = loop {
            let mut stream = match listener.accept() {
                Ok((stream, _)) => stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if std::time::Instant::now() >= deadline {
                        return;
                    }
                    thread::sleep(std::time::Duration::from_millis(10));
                    continue;
                }
                Err(error) => panic!("accept API request: {error}"),
            };
            stream.set_nonblocking(false).expect("set request stream blocking");
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .expect("set server read timeout");
            let request = read_http_request(&mut stream);
            // `measure_direct_server_reach_ms` opens and closes a bare TCP connection before
            // the real request; skip such empty connections.
            if !request.is_empty() {
                break (stream, request);
            }
        };
        let _ = tx.send(String::from_utf8_lossy(&request).into_owned());
        thread::sleep(delay);
        let reason = match status {
            200 => "OK",
            201 => "Created",
            404 => "Not Found",
            500 => "Internal Server Error",
            _ => "Response",
        };
        write!(
            stream,
            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .expect("write canned response");
    });
    (format!("http://{address}"), rx)
}

fn refused_address() -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("reserve closed port");
    let address = listener.local_addr().expect("closed port address");
    drop(listener);
    format!("http://{address}")
}

/// Scrolls the endpoint tab down until `id` is on screen; the form grows downwards
/// (parameters, body, schemas, "Try request", response).
fn reveal_in_tab(session: &mut HeadlessSession, id: &str) {
    let visible = wheel_until_visible(session, TAB_POINT, id, MIN_HITBOX, 30);
    assert!(visible, "{id} did not enter the endpoint tab viewport: {}", dump(session));
}

fn open_route(session: &mut HeadlessSession, method: &str, path: &str) -> usize {
    let route_idx = session
        .app
        .ide_panel
        .api
        .selected_model()
        .expect("selected request-flow model")
        .routes
        .iter()
        .position(|route| route.method.as_str() == method && route.path == path)
        .unwrap_or_else(|| panic!("missing {method} {path}"));
    let route_id = format!("ApiRouteRow({route_idx})");
    // The API Mock block sits above the spec card, so the endpoint rows start below the
    // viewport. A negative wheel delta scrolls down (`wheel_delta` negates winit lines).
    let visible = wheel_until_visible(session, PANEL_POINT, &route_id, MIN_HITBOX, 20);
    assert!(visible, "{route_id} did not enter the API panel viewport: {}", dump(session));
    click_ui(session, &route_id);
    wait_until(session, 5000, "API endpoint tab", |session| {
        session
            .app
            .active_api_tab()
            .is_some_and(|(_, state)| state.route_idx == Some(route_idx))
    });
    route_idx
}

fn scratch_request(name: &str, server: &str) -> (PathBuf, HeadlessSession) {
    ensure_test_profile_root();
    reset_api_test_state();
    let dir = scratch_dir(name);
    let mut session = workspace_session(&dir);
    install_spec(&mut session, &serve_spec(server));
    (dir, session)
}

#[test]
fn headless_api_client_get_shows_status_and_body() {
    let (base, request_rx) = request_server(200, r#"{"message":"hello"}"#);
    let (dir, mut session) = scratch_request("api-client-get", &base);
    let route = open_route(&mut session, "GET", "/get");
    send_request(&mut session, route);
    let (meta, state) = session.app.active_api_tab().expect("active API tab");
    assert_eq!(state.response.as_ref().and_then(|response| response.status), Some(200), "{:?}", state.response);
    assert!(state.response.as_ref().unwrap().body.contains("hello"));
    assert!(matches!(
        meta.route_identity.as_ref(),
        Some(ApiClientRouteIdentity::OpenApi { .. })
    ));
    reveal_in_tab(&mut session, &format!("ApiResponseBody({route})"));
    assert!(request_rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap().starts_with("GET /get "));
    let _ = std::fs::remove_dir_all(dir);
}

/// Strict loop model: a response that arrives while the UI sleeps reaches the screen
/// through its `UiWaker` wake-up alone, with no input and no timer.
#[test]
fn headless_api_client_response_arrives_through_ui_waker_without_input() {
    use crate::headless::frame::WakeCause;
    let (base, _) =
        request_server_delayed(200, r#"{"message":"woken"}"#, std::time::Duration::from_millis(200));
    let (dir, mut session) = scratch_request("api-client-ui-waker", &base);
    let route = open_route(&mut session, "GET", "/get");
    reveal_in_tab(&mut session, "ApiTryRequest");
    click_ui(&mut session, "ApiTryRequest");
    let has_response = |session: &HeadlessSession| {
        session.app.active_api_tab().is_some_and(|(_, state)| {
            state.route_idx == Some(route) && state.response.is_some()
        })
    };
    assert!(!has_response(&session), "the delayed response is already shown");

    let budget = std::time::Instant::now() + std::time::Duration::from_secs(15);
    let mut causes = Vec::new();
    let applied = loop {
        let wake = session.native_wake(budget.saturating_duration_since(std::time::Instant::now()));
        causes.push((wake.cause.name(), wake.frame));
        assert_ne!(wake.cause, WakeCause::Timeout, "no wake-up delivered the response: {causes:?}");
        if has_response(&session) {
            break wake;
        }
    };
    assert_eq!(applied.cause, WakeCause::UiWaker, "wake-ups: {causes:?}");
    assert!(applied.frame, "the response pass drew no frame: {causes:?}");
    let response = session.app.active_api_tab().unwrap().1.response.as_ref().unwrap();
    assert_eq!(response.status, Some(200));
    assert!(response.body.contains("woken"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_api_client_post_sends_json_body() {
    let (base, request_rx) = request_server(201, r#"{"saved":true}"#);
    let (dir, mut session) = scratch_request("api-client-post", &base);
    let route = open_route(&mut session, "POST", "/post");
    let body_input = format!("ApiBodyInput({route})");
    reveal_in_tab(&mut session, &body_input);
    click_ui(&mut session, &body_input);
    let lines = run_script(&mut session, b"key ctrl+a\ntype {\"name\":\"Ada\"}\n");
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    send_request(&mut session, route);
    let response = session.app.active_api_tab().unwrap().1.response.as_ref().unwrap();
    assert_eq!(response.status, Some(201));
    assert!(response.body.contains("saved"));
    let request = request_rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
    assert!(request.starts_with("POST /post "), "{request}");
    assert!(request.contains(r#"{"name":"Ada"}"#), "{request}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_api_client_http_error_shows_status_and_body() {
    let (base, _) = request_server(500, r#"{"error":"unavailable"}"#);
    let (dir, mut session) = scratch_request("api-client-http-error", &base);
    let route = open_route(&mut session, "GET", "/fail");
    send_request(&mut session, route);
    let response = session.app.active_api_tab().unwrap().1.response.as_ref().unwrap();
    assert_eq!(response.status, Some(500));
    assert!(response.body.contains("unavailable"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_api_client_refused_connection_shows_error() {
    let (dir, mut session) = scratch_request("api-client-refused", &refused_address());
    let route = open_route(&mut session, "GET", "/get");
    send_request(&mut session, route);
    let response = session.app.active_api_tab().unwrap().1.response.as_ref().unwrap();
    assert!(response.error.is_some(), "connection failure was not shown: {response:?}");
    assert!(!response.body.contains("timeout"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_api_client_curl_view_can_be_copied() {
    let (base, _) = request_server(200, r#"{"message":"hello"}"#);
    let (dir, mut session) = scratch_request("api-client-curl", &base);
    let route = open_route(&mut session, "GET", "/get");
    send_request(&mut session, route);
    let curl_tab = format!("ApiResponseCurlTab({route})");
    reveal_in_tab(&mut session, &curl_tab);
    click_ui(&mut session, &curl_tab);
    let body = format!("ApiResponseBody({route})");
    reveal_in_tab(&mut session, &body);
    click_ui(&mut session, &body);
    run_script(&mut session, b"key ctrl+a\nkey ctrl+c\n");
    let clipboard = dump(&mut session)["clipboard"]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(clipboard.contains("curl"), "clipboard did not contain the request: {clipboard}");
    assert!(clipboard.contains("/get"), "clipboard missing request path: {clipboard}");
    let _ = std::fs::remove_dir_all(dir);
}
