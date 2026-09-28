use crate::app::api_client::{ApiInputDocView, ApiMethod, ApiResponseView};
use crate::headless::tests_support::{
    api_client_session, click_ui, dump, has_ui, serve_api_spec, serve_http_responses, send_request,
    wait_until, wheel_until_visible,
};
use crate::headless::HeadlessSession;
use std::path::PathBuf;
use std::time::Duration;

const PANEL_POINT: (f64, f64) = (200.0, 500.0);
const TAB_POINT: (f64, f64) = (800.0, 400.0);
const MIN_HITBOX: f64 = 24.0;

fn response_server(body: &'static str) -> String {
    serve_http_responses(
        "",
        vec![(200, "OK", "X-Fixture: response-views\r\n", body.to_string(), Duration::ZERO)],
    )
    .0
}

fn api_session(name: &str, base: &str, paths: serde_json::Value) -> (PathBuf, HeadlessSession) {
    let spec_url = serve_api_spec(base, paths);
    api_client_session(name, &spec_url)
}

fn open_route(session: &mut HeadlessSession, method: ApiMethod, path: &str) -> usize {
    let route_idx = session
        .app
        .ide_panel
        .api
        .selected_model()
        .and_then(|model| {
            model
                .routes
                .iter()
                .position(|route| route.method == method && route.path == path)
        })
        .unwrap_or_else(|| panic!("missing {} {path}", method.as_str()));
    let route_id = format!("ApiRouteRow({route_idx})");
    assert!(
        wheel_until_visible(session, PANEL_POINT, &route_id, MIN_HITBOX, 20),
        "{route_id} did not enter the API panel viewport: {}",
        dump(session)
    );
    click_ui(session, &route_id);
    wait_until(session, 5000, "API endpoint tab", |session| {
        session
            .app
            .active_api_tab()
            .is_some_and(|(_, state)| state.route_idx == Some(route_idx))
    });
    route_idx
}

fn reveal_tab_ui(session: &mut HeadlessSession, id: &str) {
    assert!(
        wheel_until_visible(session, TAB_POINT, id, MIN_HITBOX, 30),
        "{id} did not enter the endpoint tab viewport: {}",
        dump(session)
    );
}

#[test]
fn headless_api_client_response_headers_view_switches_back_to_body() {
    let base = response_server(r#"{"message":"hello"}"#);
    let (dir, mut session) = api_session(
        "api-response-headers-view",
        &base,
        serde_json::json!({
            "/get": {"get": {"responses": {"200": {"description": "ok"}}}}
        }),
    );
    let route = open_route(&mut session, ApiMethod::Get, "/get");
    send_request(&mut session, route);

    let headers_tab = format!("ApiResponseHeadersTab({route})");
    reveal_tab_ui(&mut session, &headers_tab);
    click_ui(&mut session, &headers_tab);
    let response = session
        .app
        .active_api_tab()
        .and_then(|(_, state)| state.response.as_ref())
        .unwrap_or_else(|| panic!("API response missing after request"));
    assert!(response
        .headers_text
        .to_ascii_lowercase()
        .contains("x-fixture: response-views"));
    assert_eq!(
        session.app.active_api_tab().map(|(_, state)| state.response_view),
        Some(ApiResponseView::Headers)
    );

    let body_tab = format!("ApiResponseBodyTab({route})");
    reveal_tab_ui(&mut session, &body_tab);
    click_ui(&mut session, &body_tab);
    assert_eq!(
        session.app.active_api_tab().map(|(_, state)| state.response_view),
        Some(ApiResponseView::Body)
    );
    assert!(session.app.active_api_tab().is_some_and(|(_, state)| {
        state.response.as_ref().is_some_and(|response| response.body.contains("hello"))
    }));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_api_client_output_status_tab_selects_documented_variant() {
    let (dir, mut session) = api_session(
        "api-output-status-tab",
        "http://127.0.0.1:1",
        serde_json::json!({
            "/get": {"get": {"responses": {
                "200": {"description": "ok"},
                "404": {"description": "missing"}
            }}}
        }),
    );
    let route = open_route(&mut session, ApiMethod::Get, "/get");
    let model = session
        .app
        .ide_panel
        .api
        .selected_model()
        .unwrap_or_else(|| panic!("selected OpenAPI model missing"));
    assert_eq!(model.routes[route].responses.len(), 2);
    let status_tab = format!("ApiOutputStatusTab({route}, 1)");
    reveal_tab_ui(&mut session, &format!("ApiOutputSchemaBody({route})"));
    for _ in 0..8 {
        if has_ui(&dump(&mut session), &status_tab) {
            break;
        }
        let lines = crate::headless::tests_support::run_script(
            &mut session,
            format!("mouse_move {} {}\nwheel 0 1\nsettle 2000\n", TAB_POINT.0, TAB_POINT.1)
                .as_bytes(),
        );
        assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    }
    assert!(
        has_ui(&dump(&mut session), &status_tab),
        "{status_tab} did not enter the endpoint tab viewport: {}",
        dump(&mut session)
    );
    click_ui(&mut session, &status_tab);
    assert_eq!(
        session.app.active_api_tab().map(|(_, state)| state.output_status_idx),
        Some(1)
    );
    let model = session
        .app
        .ide_panel
        .api
        .selected_model()
        .unwrap_or_else(|| panic!("selected OpenAPI model missing"));
    assert_eq!(model.routes[route].responses.len(), 2);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_api_client_input_schema_tab_expands_schema_field() {
    let (dir, mut session) = api_session(
        "api-input-schema-tab",
        "http://127.0.0.1:1",
        serde_json::json!({
            "/submit": {"post": {
                "requestBody": {"required": true, "content": {
                    "application/json": {"schema": {"type": "object", "properties": {
                        "profile": {"type": "object", "properties": {
                            "name": {"type": "string"}
                        }}
                    }}}
                }},
                "responses": {"200": {"description": "ok"}}
            }}
        }),
    );
    let route = open_route(&mut session, ApiMethod::Post, "/submit");
    let example_tab = format!("ApiInputExampleTab({route})");
    reveal_tab_ui(&mut session, &example_tab);
    click_ui(&mut session, &example_tab);
    assert_eq!(
        session.app.active_api_tab().map(|(_, state)| state.input_doc_view),
        Some(ApiInputDocView::Input)
    );

    let schema_tab = format!("ApiInputSchemaTab({route})");
    reveal_tab_ui(&mut session, &schema_tab);
    click_ui(&mut session, &schema_tab);
    assert_eq!(
        session.app.active_api_tab().map(|(_, state)| state.input_doc_view),
        Some(ApiInputDocView::Schema)
    );
    let schema_dump = dump(&mut session);
    let fold_id = schema_dump["ui"]
        .as_array()
        .and_then(|elements| {
            elements.iter().find_map(|element| {
                let id = element["id"].as_str()?;
                id.starts_with(&format!("ApiInputSchemaFold({route},")).then(|| id.to_string())
            })
        })
        .unwrap_or_else(|| panic!("schema field fold control missing: {schema_dump}"));
    click_ui(&mut session, &fold_id);
    assert!(session.app.active_api_tab().is_some_and(|(_, state)| {
        !state.input_schema_collapsed.is_empty()
    }));
    click_ui(&mut session, &fold_id);
    assert!(session.app.active_api_tab().is_some_and(|(_, state)| {
        state.input_schema_collapsed.is_empty()
    }));
    let _ = std::fs::remove_dir_all(dir);
}
