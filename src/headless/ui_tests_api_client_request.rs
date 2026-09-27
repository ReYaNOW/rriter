//! Headless API Client request and response regressions.

use crate::app::api_client::ApiClientRouteIdentity;
use crate::headless::tests_support::{
    click_ui, dump, ensure_test_profile_root, has_ui, reset_api_test_state, run_script,
    scratch_dir, session_for_test, wait_until, wheel_until_visible,
};
use crate::headless::HeadlessSession;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;
/// Cursor over the API side panel (x 64..384 at this size and scale).
const PANEL_POINT: (f64, f64) = (200.0, 500.0);
/// Cursor over the API endpoint tab body.
const TAB_POINT: (f64, f64) = (800.0, 400.0);
/// Smallest hitbox accepted as "in the viewport" for rows, inputs and buttons.
const MIN_HITBOX: f64 = 24.0;

fn serve_spec(server: &str) -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind request spec server");
    let address = listener.local_addr().expect("request spec server address");
    let server = server.to_string();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept request spec import");
        let spec = serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "Headless request flow", "version": "1.0.0"},
            "servers": [{"url": server}],
            "paths": {
                "/get": {"get": {"responses": {"200": {"description": "ok"}}}},
                "/post": {"post": {
                    "requestBody": {"required": true, "content": {
                        "application/json": {"schema": {"type": "object"}}
                    }},
                    "responses": {"201": {"description": "created"}}
                }},
                "/fail": {"get": {"responses": {"500": {"description": "failure"}}}}
            }
        }).to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            spec.len(), spec
        );
        stream.write_all(response.as_bytes()).expect("write request spec");
    });
    format!("http://{address}/openapi.json")
}

fn request_server(status: u16, body: &'static str) -> (String, Receiver<String>) {
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

/// Reads one HTTP request (headers plus `Content-Length` body); empty when the peer
/// closed the connection without sending anything.
fn read_http_request(stream: &mut std::net::TcpStream) -> Vec<u8> {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 2048];
    loop {
        let read = match stream.read(&mut buffer) {
            Ok(read) => read,
            Err(_) if request.is_empty() => return request,
            Err(error) => panic!("read API request: {error}"),
        };
        if read == 0 {
            return request;
        }
        request.extend_from_slice(&buffer[..read]);
        if let Some(header_end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let content_length = headers.lines().find_map(|line| {
                let (key, value) = line.split_once(':')?;
                key.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            }).unwrap_or(0);
            if request.len() >= header_end + 4 + content_length {
                return request;
            }
        }
    }
}

fn refused_address() -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("reserve closed port");
    let address = listener.local_addr().expect("closed port address");
    drop(listener);
    format!("http://{address}")
}

fn workspace_session(dir: &Path) -> HeadlessSession {
    let mut session = session_for_test(TEST_WIDTH, TEST_HEIGHT);
    let lines = run_script(
        &mut session,
        format!("scale {TEST_SCALE}\nworkspace {}\nsettle 2000\n", dir.display()).as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    // The slot toggles the panel, so click it only when API Client is not already shown.
    if dump(&mut session)["ide_panel"]["active"] != "api" {
        click_ui(&mut session, "SidebarSlot(ApiClient)");
    }
    wait_until(&mut session, 5000, "API Client panel", |session| {
        has_ui(&dump(session), "ApiImportAdd")
    });
    session
}

/// Scrolls the endpoint tab down until `id` is on screen; the form grows downwards
/// (parameters, body, schemas, "Try request", response).
fn reveal_in_tab(session: &mut HeadlessSession, id: &str) {
    let visible = wheel_until_visible(session, TAB_POINT, id, MIN_HITBOX, 30);
    assert!(visible, "{id} did not enter the endpoint tab viewport: {}", dump(session));
}

fn install_spec(session: &mut HeadlessSession, server: &str) {
    let spec_url = serve_spec(server);
    click_ui(session, "ApiImportAdd");
    click_ui(session, "ApiImportUrl");
    click_ui(session, "ApiImportUrlInput");
    let lines = run_script(session, format!("key ctrl+a\ntype {spec_url}\n").as_bytes());
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    click_ui(session, "ApiImportUrlConfirm");
    wait_until(session, 5000, "local request-flow spec", |session| {
        let api = &session.app.ide_panel.api;
        api.loading.is_empty()
            && api.selected_spec.is_some_and(|id| api.models.contains_key(&id))
    });
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

fn send_request(session: &mut HeadlessSession, route_idx: usize) {
    reveal_in_tab(session, "ApiTryRequest");
    click_ui(session, "ApiTryRequest");
    wait_until(session, 15000, "API response", |session| {
        session
            .app
            .active_api_tab()
            .is_some_and(|(_, state)| state.route_idx == Some(route_idx) && state.response.is_some())
    });
}

fn scratch_request(name: &str, server: &str) -> (PathBuf, HeadlessSession) {
    ensure_test_profile_root();
    reset_api_test_state();
    let dir = scratch_dir(name);
    let mut session = workspace_session(&dir);
    install_spec(&mut session, server);
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
