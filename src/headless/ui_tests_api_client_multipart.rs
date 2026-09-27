//! Headless API Client multipart request regressions.

use crate::app::api_client::{ApiMethod, ApiSpecModel};
use crate::app::EditorTabKind;
use crate::headless::tests_support::{
    click_ui, dump, ensure_test_profile_root, has_ui, reset_api_test_state, run_script,
    scratch_dir, send_request, read_http_request, workspace_session, install_spec,
    wait_until, wheel_until_visible,
};
use crate::headless::HeadlessSession;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const PANEL_POINT: (f64, f64) = (200.0, 500.0);
const TAB_POINT: (f64, f64) = (800.0, 400.0);
const MIN_HITBOX: f64 = 24.0;

struct MultipartServerCleanup {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for MultipartServerCleanup {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn multipart_server() -> (String, Receiver<Vec<u8>>, MultipartServerCleanup) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind multipart fixture server");
    let address = listener.local_addr().expect("multipart fixture address");
    let base = format!("http://{address}");
    let spec = serde_json::json!({
        "openapi": "3.1.0",
        "info": {"title": "Headless multipart", "version": "1.0.0"},
        "servers": [{"url": base}],
        "paths": {
            "/upload": {"post": {
                "requestBody": {"required": true, "content": {
                    "multipart/form-data": {"schema": {
                        "type": "object",
                        "required": ["title"],
                        "properties": {
                            "title": {"type": "string"},
                            "note": {"type": "string"},
                            "attachment": {"type": "string", "format": "binary"}
                        }
                    }}
                }},
                "responses": {"200": {"description": "ok"}}
            }}
        }
    })
    .to_string();
    let (tx, rx) = mpsc::channel();
    listener.set_nonblocking(true).expect("make multipart server nonblocking");
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = stop.clone();
    let worker = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(60);
        let (mut stream, request) = accept_request(&listener, deadline, &worker_stop)
            .expect("accept OpenAPI import and multipart request");
        assert!(request.starts_with(b"GET /openapi.json "), "spec request: {}", String::from_utf8_lossy(&request));
        write_response(&mut stream, &spec);

        let Some((mut stream, request)) = accept_request(&listener, deadline, &worker_stop) else {
            return;
        };
        let _ = tx.send(request);
        write_response(&mut stream, "{}");
    });
    (format!("{base}/openapi.json"), rx, MultipartServerCleanup { stop, thread: Some(worker) })
}

fn accept_request(listener: &TcpListener, deadline: Instant, stop: &AtomicBool) -> Option<(TcpStream, Vec<u8>)> {
    loop {
        if stop.load(Ordering::Acquire) {
            return None;
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .expect("set fixture stream timeout");
                let request = read_http_request(&mut stream);
                if !request.is_empty() {
                    return Some((stream, request));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return None;
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("accept multipart request: {error}"),
        }
    }
}

fn write_response(stream: &mut TcpStream, body: &str) {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .expect("write fixture response");
}

fn scratch_request(name: &str) -> (PathBuf, HeadlessSession, Receiver<Vec<u8>>, MultipartServerCleanup) {
    ensure_test_profile_root();
    reset_api_test_state();
    let dir = scratch_dir(name);
    let (spec_url, request_rx, fixture_cleanup) = multipart_server();
    let mut session = workspace_session(&dir);
    install_spec(&mut session, &spec_url);
    (dir, session, request_rx, fixture_cleanup)
}

fn route_index(session: &HeadlessSession) -> usize {
    session
        .app
        .ide_panel
        .api
        .selected_model()
        .expect("selected multipart spec")
        .routes
        .iter()
        .position(|route| route.method == ApiMethod::Post && route.path == "/upload")
        .expect("multipart upload route")
}

fn open_route(session: &mut HeadlessSession) -> usize {
    let route_idx = route_index(session);
    let route_id = format!("ApiRouteRow({route_idx})");
    let visible = wheel_until_visible(session, PANEL_POINT, &route_id, MIN_HITBOX, 20);
    assert!(visible, "{route_id} did not enter the API panel: {}", dump(session));
    click_ui(session, &route_id);
    wait_until(session, 5000, "multipart endpoint tab", |session| {
        session
            .app
            .active_api_tab()
            .is_some_and(|(_, state)| state.route_idx == Some(route_idx))
    });
    route_idx
}

fn body_field_index(model: &ApiSpecModel, route_idx: usize, name: &str) -> usize {
    let route = model.routes.get(route_idx).expect("multipart route");
    let schema = route
        .request_body
        .as_ref()
        .and_then(|body| body.schema)
        .and_then(|schema| model.schema_arena.get(schema.0))
        .expect("multipart body schema");
    schema
        .properties
        .iter()
        .position(|prop| prop.name == name)
        .unwrap_or_else(|| panic!("multipart field {name}"))
}

fn replace_body_field(session: &mut HeadlessSession, route_idx: usize, name: &str, value: &str) {
    let model = session
        .app
        .ide_panel
        .api
        .selected_model()
        .expect("selected multipart spec");
    let field_idx = body_field_index(model, route_idx, name);
    let input = format!("ApiBodyFieldInput({route_idx}, {field_idx})");
    let visible = wheel_until_visible(session, TAB_POINT, &input, MIN_HITBOX, 30);
    assert!(visible, "{input} did not enter the endpoint tab: {}", dump(session));
    click_ui(session, &input);
    let lines = run_script(
        session,
        format!("key ctrl+a\ntype {value}\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
}

fn set_file_path(session: &mut HeadlessSession, route_idx: usize, path: PathBuf) {
    let active_tab = session.app.active_tab;
    let EditorTabKind::ApiClient(_, state) = &mut session.app.tabs[active_tab].kind else {
        panic!("active tab is not API Client");
    };
    state
        .body_file_paths
        .insert("attachment".to_string(), vec![path]);
    assert_eq!(state.route_idx, Some(route_idx));
    let lines = run_script(session, b"mouse_move 0 0\n");
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
}

fn captured_request(receiver: &Receiver<Vec<u8>>) -> Vec<u8> {
    receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("multipart request did not reach fixture server")
}

fn assert_multipart_header_and_boundary(request: &[u8]) -> (&str, &[u8]) {
    let header_end = request
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("HTTP headers ended");
    let headers = std::str::from_utf8(&request[..header_end]).expect("UTF-8 HTTP headers");
    assert!(headers.starts_with("POST /upload "), "{headers}");
    let content_type = headers
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case("content-type").then_some(value.trim())
        })
        .expect("multipart Content-Type");
    let boundary = content_type
        .strip_prefix("multipart/form-data; boundary=")
        .expect("multipart boundary parameter");
    assert!(!boundary.is_empty(), "empty multipart boundary");
    let body = &request[header_end + 4..];
    assert!(body.starts_with(format!("--{boundary}\r\n").as_bytes()));
    assert!(body.ends_with(format!("--{boundary}--\r\n").as_bytes()));
    (boundary, body)
}

fn assert_text_part(body: &[u8], name: &str, value: &str) {
    let header = format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n");
    let expected = format!("{header}{value}\r\n");
    assert!(
        body.windows(expected.len()).any(|window| window == expected.as_bytes()),
        "missing multipart text part {name}={value:?}: {}",
        String::from_utf8_lossy(body)
    );
}

fn cleanup(session: HeadlessSession, dir: &Path, fixture_cleanup: MultipartServerCleanup) {
    drop(session);
    drop(fixture_cleanup);
    reset_api_test_state();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_api_client_multipart_sends_fields_and_file() {
    let (dir, mut session, request_rx, fixture_cleanup) = scratch_request("api-client-multipart-file");
    let route_idx = open_route(&mut session);
    replace_body_field(&mut session, route_idx, "title", "Ada");
    replace_body_field(&mut session, route_idx, "note", "avatar upload");
    let file_path = dir.join("avatar.bin");
    std::fs::write(&file_path, b"file\0payload").expect("write multipart fixture file");
    set_file_path(&mut session, route_idx, file_path);

    send_request(&mut session, route_idx);
    let response = session.app.active_api_tab().unwrap().1.response.as_ref().unwrap();
    assert_eq!(response.status, Some(200));
    let request = captured_request(&request_rx);
    let (_, body) = assert_multipart_header_and_boundary(&request);
    assert_text_part(body, "title", "Ada");
    assert_text_part(body, "note", "avatar upload");
    assert!(body.windows(b"name=\"attachment\"; filename=\"avatar.bin\"".len()).any(
        |window| window == b"name=\"attachment\"; filename=\"avatar.bin\""
    ));
    assert!(body.windows(b"file\0payload".len()).any(|window| window == b"file\0payload"));
    cleanup(session, &dir, fixture_cleanup);
}

#[test]
fn headless_api_client_multipart_preserves_special_and_non_ascii_text() {
    let (dir, mut session, request_rx, fixture_cleanup) = scratch_request("api-client-multipart-unicode");
    let route_idx = open_route(&mut session);
    let value = "café & snowman ☃ / ?=%";
    replace_body_field(&mut session, route_idx, "title", value);

    send_request(&mut session, route_idx);
    let request = captured_request(&request_rx);
    let (_, body) = assert_multipart_header_and_boundary(&request);
    assert_text_part(body, "title", value);
    cleanup(session, &dir, fixture_cleanup);
}

#[test]
fn headless_api_client_multipart_sends_empty_required_field() {
    let (dir, mut session, request_rx, fixture_cleanup) = scratch_request("api-client-multipart-required");
    let route_idx = open_route(&mut session);

    send_request(&mut session, route_idx);
    let response = session.app.active_api_tab().unwrap().1.response.as_ref().unwrap();
    assert_eq!(response.status, Some(200), "empty required field response: {response:?}");
    let request = captured_request(&request_rx);
    let (_, body) = assert_multipart_header_and_boundary(&request);
    assert_text_part(body, "title", "");
    cleanup(session, &dir, fixture_cleanup);
}

#[test]
fn headless_api_client_multipart_missing_file_path_shows_request_error() {
    let (dir, mut session, request_rx, fixture_cleanup) = scratch_request("api-client-multipart-missing-file");
    let route_idx = open_route(&mut session);
    replace_body_field(&mut session, route_idx, "title", "document");
    set_file_path(&mut session, route_idx, dir.join("missing.bin"));

    send_request(&mut session, route_idx);
    let response = session.app.active_api_tab().unwrap().1.response.as_ref().unwrap();
    assert!(response.error.is_some(), "missing multipart file did not show an error: {response:?}");
    assert!(request_rx.try_recv().is_err(), "request with missing file reached the server");
    cleanup(session, &dir, fixture_cleanup);
}
