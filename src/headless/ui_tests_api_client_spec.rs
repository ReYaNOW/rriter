use crate::app::api_client::{ApiMethod, ApiSecurityApiKeyLocation, ApiSecuritySchemeKind};
use crate::headless::tests_support::{
    click_ui, dump, ensure_test_profile_root, has_ui, read_request_before_reply, reset_api_test_state, run_script,
    scratch_dir, wait_until, wheel_until_visible, workspace_with_explorer,
};
use crate::headless::HeadlessSession;
use std::io::Write;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
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

const OPENAPI_SPEC: &str = r#"{
  "openapi": "3.0.3",
  "info": {"title": "Headless API", "version": "1.0.0"},
  "servers": [{"url": "http://127.0.0.1:8123/api"}, {"url": "http://127.0.0.1:8124/api"}],
  "tags": [{"name": "Widgets"}],
  "paths": {
    "/widgets/{widgetId}": {
      "get": {
        "tags": ["Widgets"],
        "summary": "Get widget",
        "parameters": [
          {"name": "widgetId", "in": "path", "required": true, "schema": {"type": "string"}},
          {"name": "limit", "in": "query", "schema": {"type": "integer", "default": 5}}
        ],
        "responses": {"200": {"description": "ok"}}
      }
    },
    "/widgets": {
      "post": {
        "tags": ["Widgets"],
        "summary": "Create widget",
        "responses": {"201": {"description": "created"}}
      }
    }
  },
  "components": {
    "securitySchemes": {
      "Basic": {"type": "http", "scheme": "basic"},
      "Bearer": {"type": "http", "scheme": "bearer", "bearerFormat": "JWT"},
      "HeaderKey": {"type": "apiKey", "in": "header", "name": "X-Workspace-Key"}
    }
  }
}"#;

fn api_session(name: &str) -> (PathBuf, String, HeadlessSession) {
    ensure_test_profile_root();
    reset_api_test_state();
    let dir = scratch_dir(name);
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind OpenAPI fixture server");
    let url = format!("http://{}/openapi.json", listener.local_addr().expect("fixture address"));
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept OpenAPI import");
        read_request_before_reply(&mut stream);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            OPENAPI_SPEC.len(),
            OPENAPI_SPEC
        )
        .expect("serve OpenAPI fixture");
    });

    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    if dump(&mut session)["ide_panel"]["active"] != "api" {
        click_ui(&mut session, "SidebarSlot(ApiClient)");
    }
    wait_until(&mut session, 5000, "API Client panel", |session| {
        has_ui(&dump(session), "ApiImportAdd")
    });
    click_ui(&mut session, "ApiImportAdd");
    click_ui(&mut session, "ApiImportUrl");
    click_ui(&mut session, "ApiImportUrlInput");
    let lines = run_script(&mut session, format!("key ctrl+a\ntype {url}\n").as_bytes());
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    click_ui(&mut session, "ApiImportUrlConfirm");
    wait_until(&mut session, 5000, "local OpenAPI import", |session| {
        let api = &session.app.ide_panel.api;
        api.loading.is_empty()
            && api
                .selected_spec
                .is_some_and(|id| api.models.contains_key(&id))
    });

    // The API Mock block sits above the spec card, so the spec tree starts below the
    // viewport. A negative wheel delta scrolls down (`wheel_delta` negates winit lines).
    let last_route = session.app.ide_panel.api.selected_model().expect("selected API model").routes.len() - 1;
    let rows_visible = wheel_until_visible(&mut session, PANEL_POINT, &format!("ApiRouteRow({last_route})"), MIN_HITBOX, 20);
    assert!(rows_visible, "API endpoint rows did not enter the viewport: {}", dump(&mut session));
    (dir, url, session)
}

fn route_index(session: &HeadlessSession, method: ApiMethod, path: &str) -> usize {
    let api = &session.app.ide_panel.api;
    let model = api.selected_model().expect("selected API model");
    model
        .routes
        .iter()
        .position(|route| route.method == method && route.path == path)
        .expect("fixture route")
}

fn open_route(session: &mut HeadlessSession, route_idx: usize) {
    let row_id = format!("ApiRouteRow({route_idx})");
    assert!(has_ui(&dump(session), &row_id), "endpoint row missing: {row_id}");
    click_ui(session, &row_id);
    wait_until(session, 5000, "API endpoint form", |session| {
        session
            .app
            .active_api_tab()
            .is_some_and(|(_, state)| {
                !state.auth_view && state.route_idx == Some(route_idx) && state.tab_scroll.is_settled()
            })
    });
}

fn open_auth_form(session: &mut HeadlessSession) {
    assert!(has_ui(&dump(session), "ApiAuthRoot"), "Auth root missing from spec tree");
    click_ui(session, "ApiAuthRoot");
    wait_until(session, 5000, "API authentication form", |session| {
        session
            .app
            .active_api_tab()
            .is_some_and(|(_, state)| state.auth_view)
    });
}

fn scheme_index(session: &HeadlessSession, name: &str) -> usize {
    let api = &session.app.ide_panel.api;
    api.selected_model()
        .expect("selected API model")
        .security_schemes
        .iter()
        .position(|scheme| scheme.name == name)
        .expect("fixture security scheme")
}

/// Scrolls the endpoint tab down until `id` is on screen; the form grows downwards
/// (parameters, body, schemas, servers, "Try request", response).
fn reveal_in_tab(session: &mut HeadlessSession, id: &str) {
    let visible = wheel_until_visible(session, TAB_POINT, id, MIN_HITBOX, 30);
    assert!(visible, "{id} did not enter the endpoint tab viewport: {}", dump(session));
}

fn replace_text(session: &mut HeadlessSession, input_id: &str, text: &str) {
    reveal_in_tab(session, input_id);
    click_ui(session, input_id);
    let lines = run_script(
        session,
        format!("key ctrl+a\ntype {text}\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
}

fn cleanup_api_session(session: HeadlessSession, dir: &Path) {
    drop(session);
    reset_api_test_state();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_api_client_spec_lists_and_opens_endpoint() {
    let (dir, url, mut session) = api_session("ui-api-spec-route");
    let route_idx = route_index(&session, ApiMethod::Get, "/widgets/{widgetId}");
    let panel = dump(&mut session);
    assert!(has_ui(&panel, "ApiSpecSelect(0)"), "{panel}");
    assert!(has_ui(&panel, &format!("ApiRouteRow({route_idx})")), "{panel}");
    assert_eq!(session.app.ide_panel.api.specs[0].source, crate::app::api_client::ApiSpecSource::Url(url));

    open_route(&mut session, route_idx);
    // Server chips render only for specs with more than one server, right above "Try".
    reveal_in_tab(&mut session, "ApiServerSelect(0)");
    reveal_in_tab(&mut session, "ApiTryRequest");
    let form = dump(&mut session);
    assert!(has_ui(&form, "ApiServerSelect(0)"), "{form}");
    let (meta, tab) = session.app.active_api_tab().expect("active API tab");
    assert_eq!(meta.route_method, Some(ApiMethod::Get));
    assert_eq!(meta.route_path, "/widgets/{widgetId}");
    assert_eq!(tab.server_idx, 0);
    let server = &session.app.ide_panel.api.models[&meta.spec_id].servers[tab.server_idx];
    assert_eq!(server.url, "http://127.0.0.1:8123/api");
    cleanup_api_session(session, &dir);
}

#[test]
fn headless_api_client_spec_edits_path_and_query_parameters() {
    let (dir, _, mut session) = api_session("ui-api-spec-params");
    let route_idx = route_index(&session, ApiMethod::Get, "/widgets/{widgetId}");
    open_route(&mut session, route_idx);
    let path_input = format!("ApiPathParamInput({route_idx}, 0)");
    let query_input = format!("ApiQueryParamInput({route_idx}, 0)");

    replace_text(&mut session, &path_input, "widget-42");
    replace_text(&mut session, &query_input, "12");
    session.app.commit_api_focus();
    let (_, tab) = session.app.active_api_tab().expect("active API tab");
    assert_eq!(tab.path_values[0].name, "widgetId");
    assert_eq!(tab.path_values[0].value, "widget-42");
    assert_eq!(tab.query_values[0].name, "limit");
    assert_eq!(tab.query_values[0].value, "12");
    cleanup_api_session(session, &dir);
}

#[test]
fn headless_api_client_spec_edits_bearer_and_basic_auth() {
    let (dir, _, mut session) = api_session("ui-api-spec-auth");
    open_auth_form(&mut session);
    let basic_idx = scheme_index(&session, "Basic");
    let bearer_idx = scheme_index(&session, "Bearer");
    for id in [
        format!("ApiAuthUsername({basic_idx})"),
        format!("ApiAuthPassword({basic_idx})"),
        format!("ApiAuthSave({basic_idx})"),
        format!("ApiAuthValue({bearer_idx})"),
        format!("ApiAuthAccessSave({bearer_idx})"),
    ] {
        assert!(has_ui(&dump(&mut session), &id), "auth control missing: {id}");
    }

    replace_text(&mut session, &format!("ApiAuthUsername({basic_idx})"), "headless-user");
    replace_text(&mut session, &format!("ApiAuthPassword({basic_idx})"), "headless-pass");
    click_ui(&mut session, &format!("ApiAuthSave({basic_idx})"));
    replace_text(&mut session, &format!("ApiAuthValue({bearer_idx})"), "local-test-token");
    click_ui(&mut session, &format!("ApiAuthAccessSave({bearer_idx})"));

    let auth = &session.app.ide_panel.api.auth.entries;
    let basic = auth.iter().find(|entry| entry.scheme == "Basic").expect("basic auth entry");
    assert_eq!(basic.username, "headless-user");
    assert_eq!(basic.password, "headless-pass");
    let bearer = auth.iter().find(|entry| entry.scheme == "Bearer").expect("bearer auth entry");
    assert_eq!(bearer.value, "local-test-token");
    cleanup_api_session(session, &dir);
}

#[test]
fn headless_api_client_spec_edits_api_key_header_auth() {
    let (dir, _, mut session) = api_session("ui-api-spec-header-auth");
    open_auth_form(&mut session);
    let header_idx = scheme_index(&session, "HeaderKey");
    let model = session.app.ide_panel.api.selected_model().expect("selected API model");
    assert!(matches!(
        model.security_schemes[header_idx].kind,
        ApiSecuritySchemeKind::ApiKey {
            ref name,
            location: ApiSecurityApiKeyLocation::Header
        } if name == "X-Workspace-Key"
    ));
    let value_id = format!("ApiAuthValue({header_idx})");
    let save_id = format!("ApiAuthSave({header_idx})");
    assert!(has_ui(&dump(&mut session), &value_id));
    assert!(has_ui(&dump(&mut session), &save_id));

    replace_text(&mut session, &value_id, "local-header-key");
    click_ui(&mut session, &save_id);
    let entry = session
        .app
        .ide_panel
        .api
        .auth
        .entries
        .iter()
        .find(|entry| entry.scheme == "HeaderKey")
        .expect("header auth entry");
    assert_eq!(entry.value, "local-header-key");
    cleanup_api_session(session, &dir);
}
