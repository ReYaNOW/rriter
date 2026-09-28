use crate::app::api_client::{ApiMethod, ApiSpecSource};
use crate::headless::tests_support::{
    click_ui, dump, ensure_test_profile_root, has_ui, reset_api_test_state, run_script, scratch_dir,
    serve_http_responses, wait_until, wheel_until_visible, workspace_with_explorer,
};
use crate::headless::HeadlessSession;
use std::path::{Path, PathBuf};
use std::time::Duration;

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;
/// Cursor over the API side panel (x 64..384 at this size and scale).
const PANEL_POINT: (f64, f64) = (200.0, 500.0);
/// Smallest hitbox accepted as "in the viewport" for rows and buttons.
const MIN_HITBOX: f64 = 24.0;

const IMPORT_SPEC: &str = r#"{
  "openapi": "3.0.3",
  "info": {"title": "Import Probe", "version": "1.0.0"},
  "paths": {
    "/items": {"get": {"summary": "List items", "responses": {"200": {"description": "ok"}}}},
    "/items/{id}": {"delete": {"summary": "Delete item", "responses": {"204": {"description": "gone"}}}}
  }
}"#;

fn fixture_server(responses: Vec<(u16, &'static str)>) -> (String, std::sync::mpsc::Receiver<String>) {
    serve_http_responses(
        "/openapi.json",
        responses
            .into_iter()
            .map(|(status, body)| (status, "X", "", body.to_string(), Duration::ZERO))
            .collect(),
    )
}

fn import_session(name: &str) -> (PathBuf, HeadlessSession) {
    ensure_test_profile_root();
    reset_api_test_state();
    let dir = scratch_dir(name);
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    if dump(&mut session)["ide_panel"]["active"] != "api" {
        click_ui(&mut session, "SidebarSlot(ApiClient)");
    }
    wait_until(&mut session, 5000, "API Client panel", |session| {
        has_ui(&dump(session), "ApiImportAdd")
    });
    assert!(session.app.ide_panel.api.specs.is_empty(), "stale API specs after reset");
    (dir, session)
}

/// "+" -> "URL" -> replace the input text with `url` -> confirm (check-mark button).
fn submit_import_url(session: &mut HeadlessSession, url: &str) {
    click_ui(session, "ApiImportAdd");
    click_ui(session, "ApiImportUrl");
    click_ui(session, "ApiImportUrlInput");
    let script = if url.is_empty() {
        "key ctrl+a\nkey backspace\n".to_string()
    } else {
        format!("key ctrl+a\ntype {url}\n")
    };
    let lines = run_script(session, script.as_bytes());
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    click_ui(session, "ApiImportUrlConfirm");
}

/// A started URL import puts its spec id into `loading` synchronously on confirm,
/// so an empty set means the load finished (success or error).
fn wait_import_done(session: &mut HeadlessSession) {
    wait_until(session, 5000, "OpenAPI URL import", |session| session.app.ide_panel.api.loading.is_empty());
}

fn cleanup(session: HeadlessSession, dir: &Path) {
    drop(session);
    reset_api_test_state();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_api_client_import_url_adds_spec_and_routes() {
    let (url, requests) = fixture_server(vec![(200, IMPORT_SPEC)]);
    let (dir, mut session) = import_session("ui-api-import-url");
    submit_import_url(&mut session, &url);
    wait_import_done(&mut session);

    let api = &session.app.ide_panel.api;
    assert_eq!(api.import_error, None);
    assert!(!api.import_url_open, "URL input should close after a successful import");
    assert_eq!(api.specs.len(), 1);
    assert_eq!(api.specs[0].title, "Import Probe");
    assert_eq!(api.specs[0].source, ApiSpecSource::Url(url.clone()));
    let routes: Vec<_> = api
        .selected_model()
        .expect("imported spec is selected")
        .routes
        .iter()
        .map(|route| (route.method, route.path.clone()))
        .collect();
    assert!(routes.contains(&(ApiMethod::Get, "/items".to_string())), "{routes:?}");
    assert!(routes.contains(&(ApiMethod::Delete, "/items/{id}".to_string())), "{routes:?}");
    let request = requests.recv_timeout(Duration::from_secs(2)).expect("import request");
    assert!(request.starts_with("GET /openapi.json "), "{request}");

    // The API Mock block sits above the spec card; a negative wheel delta scrolls down.
    let last_route = routes.len() - 1;
    let visible = wheel_until_visible(&mut session, PANEL_POINT, &format!("ApiRouteRow({last_route})"), MIN_HITBOX, 20);
    assert!(visible, "imported endpoint rows did not enter the viewport: {}", dump(&mut session));
    assert!(has_ui(&dump(&mut session), "ApiSpecSelect(0)"));
    cleanup(session, &dir);
}

#[test]
fn headless_api_client_import_rejects_empty_and_non_http_url() {
    let (dir, mut session) = import_session("ui-api-import-empty");
    submit_import_url(&mut session, "");
    let api = &session.app.ide_panel.api;
    assert_eq!(api.import_error.as_deref(), Some("URL пустой"));
    assert!(api.import_url_open, "URL input must stay open after a rejected URL");
    assert!(api.loading.is_empty() && api.specs.is_empty(), "rejected URL started a load");
    let panel = dump(&mut session);
    assert!(has_ui(&panel, "ApiImportUrlInput") && has_ui(&panel, "ApiImportUrlConfirm"), "{panel}");

    submit_import_url(&mut session, "ftp://127.0.0.1/openapi.json");
    let api = &session.app.ide_panel.api;
    assert_eq!(api.import_error.as_deref(), Some("URL должен быть http или https"));
    assert!(api.import_url_open);
    assert!(api.loading.is_empty() && api.specs.is_empty(), "non-http URL started a load");
    cleanup(session, &dir);
}

#[test]
fn headless_api_client_import_http_404_shows_error() {
    let (url, requests) = fixture_server(vec![(404, "missing")]);
    let (dir, mut session) = import_session("ui-api-import-404");
    submit_import_url(&mut session, &url);
    wait_import_done(&mut session);
    assert!(requests.recv_timeout(Duration::from_secs(2)).is_ok(), "404 fixture was not requested");

    let api = &session.app.ide_panel.api;
    assert_eq!(api.import_error.as_deref(), Some("HTTP 404"));
    assert!(api.specs.is_empty() && api.models.is_empty(), "failed import added a spec");
    // The panel keeps working: the toolbar is still drawn and can open the import menu again.
    click_ui(&mut session, "ApiImportAdd");
    assert!(has_ui(&dump(&mut session), "ApiImportUrl"));
    cleanup(session, &dir);
}

#[test]
fn headless_api_client_import_invalid_json_shows_error() {
    let (url, requests) = fixture_server(vec![(200, "{ not json")]);
    let (dir, mut session) = import_session("ui-api-import-bad-json");
    submit_import_url(&mut session, &url);
    wait_import_done(&mut session);
    assert!(requests.recv_timeout(Duration::from_secs(2)).is_ok(), "bad JSON fixture was not requested");

    let api = &session.app.ide_panel.api;
    assert_eq!(api.import_error.as_deref(), Some("URL не ведет на валидный openapi.json"));
    assert!(api.specs.is_empty() && api.models.is_empty(), "invalid spec was added");
    assert!(has_ui(&dump(&mut session), "ApiImportAdd"));
    cleanup(session, &dir);
}

#[test]
fn headless_api_client_import_same_url_twice_keeps_one_spec() {
    let (url, requests) = fixture_server(vec![(200, IMPORT_SPEC), (200, IMPORT_SPEC)]);
    let (dir, mut session) = import_session("ui-api-import-dup");
    submit_import_url(&mut session, &url);
    wait_import_done(&mut session);
    assert!(requests.recv_timeout(Duration::from_secs(2)).is_ok(), "first import did not reach the server");
    assert_eq!(session.app.ide_panel.api.specs.len(), 1);

    // The second import must really re-fetch, so one spec means deduplication, not a skipped request.
    submit_import_url(&mut session, &url);
    wait_import_done(&mut session);
    assert!(requests.recv_timeout(Duration::from_secs(2)).is_ok(), "second import did not reach the server");
    let api = &session.app.ide_panel.api;
    let same_url = api
        .specs
        .iter()
        .filter(|entry| entry.source == ApiSpecSource::Url(url.clone()))
        .count();
    assert_eq!(same_url, 1, "duplicate spec entries for {url}: {:?}", api.specs);
    cleanup(session, &dir);
}
