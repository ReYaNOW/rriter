use crate::headless::tests_support::{
    click_ui, dump, ensure_test_profile_root, has_ui, install_spec, reset_api_test_state,
    run_script, scratch_dir, serve_api_spec, serve_http_responses, wait_until, wheel_until_visible,
    workspace_session,
};
use crate::headless::HeadlessSession;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

const PANEL_POINT: (f64, f64) = (200.0, 500.0);
const MIN_HITBOX: f64 = 24.0;

fn api_navigation_session(name: &str) -> (PathBuf, HeadlessSession) {
    ensure_test_profile_root();
    reset_api_test_state();
    let dir = scratch_dir(name);
    let session = workspace_session(&dir);
    (dir, session)
}

fn navigation_paths() -> Value {
    json!({
        "/alpha": {"get": {"tags": ["Group A"], "responses": {"200": {"description": "ok"}}}},
        "/beta": {"post": {"tags": ["Group A"], "responses": {"200": {"description": "ok"}}}},
        "/gamma": {"get": {"tags": ["Group B"], "responses": {"200": {"description": "ok"}}}}
    })
}

fn serve_refresh_fixture() -> (String, std::sync::mpsc::Receiver<String>) {
    let fixtures = [
        json!({"/alpha": {"get": {"responses": {"200": {"description": "ok"}}}}}),
        json!({"/refreshed": {"get": {"responses": {"200": {"description": "ok"}}}}}),
    ];
    let responses = ["1", "2"]
        .into_iter()
        .zip(fixtures)
        .map(|(version, paths)| {
            let body = serde_json::to_string(&json!({
                "openapi": "3.1.0",
                "info": {"title": "Navigation Refresh", "version": version},
                "paths": paths
            }))
            .unwrap_or_else(|err| panic!("serialize refresh fixture: {err}"));
            (200, "OK", "", body, Duration::ZERO)
        })
        .collect();
    serve_http_responses("/openapi.json", responses)
}

fn route_index(session: &HeadlessSession, path: &str) -> usize {
    let Some(model) = session.app.ide_panel.api.selected_model() else {
        panic!("imported API model is not selected");
    };
    model
        .routes
        .iter()
        .position(|route| route.path == path)
        .unwrap_or_else(|| panic!("no route {path}"))
}

fn cleanup(session: HeadlessSession, dir: &Path) {
    drop(session);
    reset_api_test_state();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_api_client_refresh_reloads_routes_from_url_fixture() {
    let (url, requests) = serve_refresh_fixture();
    let (dir, mut session) = api_navigation_session("ui-api-navigation-refresh");
    install_spec(&mut session, &url);
    assert!(
        session
            .app
            .ide_panel
            .api
            .selected_model()
            .is_some_and(|model| model.routes.iter().any(|route| route.path == "/alpha"))
    );
    assert!(wheel_until_visible(
        &mut session,
        PANEL_POINT,
        "ApiSpecRefresh(0)",
        MIN_HITBOX,
        20
    ));
    click_ui(&mut session, "ApiSpecRefresh(0)");
    wait_until(&mut session, 5000, "refreshed API routes", |session| {
        session.app.ide_panel.api.loading.is_empty()
            && session
                .app
                .ide_panel
                .api
                .selected_model()
                .is_some_and(|model| model.routes.iter().any(|route| route.path == "/refreshed"))
    });

    let model = session.app.ide_panel.api.selected_model();
    assert!(model.is_some_and(|model| model.routes.iter().all(|route| route.path != "/alpha")));
    assert!(model.is_some_and(|model| model.routes.iter().any(|route| route.path == "/refreshed")));
    for _ in 0..2 {
        let request = requests
            .recv_timeout(Duration::from_secs(2))
            .unwrap_or_else(|err| panic!("expected import and refresh requests: {err}"));
        assert!(request.starts_with("GET /openapi.json "), "{request}");
    }
    cleanup(session, &dir);
}

#[test]
fn headless_api_client_remove_cancel_keeps_imported_spec() {
    let url = serve_api_spec("http://127.0.0.1:8123/api", navigation_paths());
    let (dir, mut session) = api_navigation_session("ui-api-navigation-cancel");
    install_spec(&mut session, &url);

    click_ui(&mut session, "ApiSpecRemove(0)");
    assert!(has_ui(&dump(&mut session), "ApiSpecRemoveCancel"));
    click_ui(&mut session, "ApiSpecRemoveCancel");

    assert_eq!(session.app.ide_panel.api.specs.len(), 1);
    assert!(has_ui(&dump(&mut session), "ApiSpecRemove(0)"));
    cleanup(session, &dir);
}

#[test]
fn headless_api_client_remove_confirm_removes_spec_from_list() {
    let url = serve_api_spec("http://127.0.0.1:8123/api", navigation_paths());
    let (dir, mut session) = api_navigation_session("ui-api-navigation-remove");
    install_spec(&mut session, &url);

    click_ui(&mut session, "ApiSpecRemove(0)");
    assert!(has_ui(&dump(&mut session), "ApiSpecRemoveConfirm"));
    click_ui(&mut session, "ApiSpecRemoveConfirm");

    assert!(session.app.ide_panel.api.specs.is_empty());
    assert!(!has_ui(&dump(&mut session), "ApiSpecRemove(0)"));
    cleanup(session, &dir);
}

#[test]
fn headless_api_client_route_filter_and_tag_collapse_update_route_rows() {
    let url = serve_api_spec("http://127.0.0.1:8123/api", navigation_paths());
    let (dir, mut session) = api_navigation_session("ui-api-navigation-routes");
    install_spec(&mut session, &url);
    let alpha = route_index(&session, "/alpha");
    let beta = route_index(&session, "/beta");
    let gamma = route_index(&session, "/gamma");

    assert!(wheel_until_visible(
        &mut session,
        PANEL_POINT,
        "ApiRouteFilterInput",
        MIN_HITBOX,
        24
    ));
    click_ui(&mut session, "ApiRouteFilterInput");
    let Some(model) = session.app.ide_panel.api.selected_model() else {
        panic!("imported API model is not selected");
    };
    let group_idx = model
        .route_groups
        .iter()
        .position(|group| model.routes.get(group.start).is_some_and(|route| route.tag == "Group A"))
        .unwrap_or_else(|| panic!("Group A route group is missing"));
    let tag_id = format!("ApiRouteTag({group_idx})");
    for route_idx in [alpha, beta] {
        assert!(wheel_until_visible(
            &mut session,
            PANEL_POINT,
            &format!("ApiRouteRow({route_idx})"),
            MIN_HITBOX,
            20
        ));
    }
    assert!(has_ui(&dump(&mut session), &tag_id));
    click_ui(&mut session, &tag_id);
    let collapsed = dump(&mut session);
    assert!(!has_ui(&collapsed, &format!("ApiRouteRow({alpha})")), "{collapsed}");
    assert!(!has_ui(&collapsed, &format!("ApiRouteRow({beta})")), "{collapsed}");
    click_ui(&mut session, &tag_id);
    let expanded = dump(&mut session);
    assert!(has_ui(&expanded, &format!("ApiRouteRow({alpha})")), "{expanded}");
    assert!(has_ui(&expanded, &format!("ApiRouteRow({beta})")), "{expanded}");

    let lines = run_script(&mut session, b"type beta\n");
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    assert!(wheel_until_visible(
        &mut session,
        PANEL_POINT,
        &format!("ApiRouteRow({beta})"),
        MIN_HITBOX,
        20
    ));
    let filtered = dump(&mut session);
    assert!(has_ui(&filtered, &format!("ApiRouteRow({beta})")), "{filtered}");
    assert!(!has_ui(&filtered, &format!("ApiRouteRow({alpha})")), "{filtered}");
    assert!(!has_ui(&filtered, &format!("ApiRouteRow({gamma})")), "{filtered}");
    cleanup(session, &dir);
}
