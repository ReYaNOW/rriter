//! Headless API Mock Python handler contract regressions.

use crate::app::api_mock::types::{ApiMockContractField, ApiMockContractFieldKind};
use crate::headless::tests_support::{
    click_ui, dump, get_from_mock, python_route_session, request_to_mock,
    reset_api_test_state, run_script, set_contract_handler_body, start_mock_server,
    stop_mock_server_from_ui, wait_for_request_log, wait_until, wheel_until_visible,
};
use crate::headless::HeadlessSession;
use serde_json::{Value, json};

const TAB_TOP_POINT: (f64, f64) = (800.0, 120.0);
const MIN_HITBOX: f64 = 24.0;

fn set_route_path(session: &mut HeadlessSession, route_idx: usize, path: &str) {
    let path_id = format!("ApiMockManualRoutePath({route_idx})");
    assert!(
        wheel_until_visible(session, (200.0, 500.0), &path_id, MIN_HITBOX, 30),
        "{path_id} did not enter the API Mock list: {}",
        dump(session)
    );
    click_ui(session, &path_id);
    let lines = run_script(session, format!("key ctrl+a\ntype {path}\n").as_bytes());
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    click_ui(session, &format!("ApiMockManualRouteOpen({route_idx})"));
}

fn enable_query_and_body(session: &mut HeadlessSession, route_idx: usize) {
    let query_id = format!("ApiMockContractQueryToggle({route_idx})");
    let body_id = format!("ApiMockContractBodyToggle({route_idx})");
    assert!(
        wheel_until_visible(session, TAB_TOP_POINT, &query_id, MIN_HITBOX, 30),
        "{query_id} not visible: {}",
        dump(session)
    );
    click_ui(session, &query_id);
    assert!(
        wheel_until_visible(session, TAB_TOP_POINT, &body_id, MIN_HITBOX, 30),
        "{body_id} not visible: {}",
        dump(session)
    );
    click_ui(session, &body_id);
}

fn post_to_mock(
    session: &mut HeadlessSession,
    url: &str,
    path: &str,
    body: &str,
) -> (u16, String) {
    request_to_mock(session, url, "POST", path, &[("Content-Type", "application/json")], body.as_bytes())
}

fn set_timeout_ms(session: &mut HeadlessSession, route_idx: usize, timeout_ms: u64) {
    session.app.ide_panel.api.mock.manual_routes[route_idx]
        .python
        .as_mut()
        .expect("Python handler")
        .timeout_ms = timeout_ms;
    let lines = run_script(session, b"mouse_move 0 0\n");
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
}

#[test]
fn headless_api_mock_python_handler_receives_path_query_and_json_body() {
    let (dir, mut session, route_idx) = python_route_session("api-mock-python-contract-data");
    set_route_path(&mut session, route_idx, "/items/{item_id}");
    click_ui(&mut session, &format!("ApiMockManualRouteMethod({route_idx})"));
    {
        let script = session.app.ide_panel.api.mock.manual_routes[route_idx]
            .python
            .as_mut()
            .expect("Python handler");
        script.contract.query.fields = vec![ApiMockContractField::new(
            "q",
            ApiMockContractFieldKind::String,
            false,
        )];
        script.contract.body.fields = vec![ApiMockContractField::new(
            "label",
            ApiMockContractFieldKind::String,
            false,
        )];
    }
    enable_query_and_body(&mut session, route_idx);
    set_contract_handler_body(
        &mut session,
        route_idx,
        "return json_response({\"item_id\": item_id, \"q\": query.q, \"label\": body.label})",
    );

    let url = start_mock_server(&mut session);
    let response = post_to_mock(
        &mut session,
        &url,
        "/items/42?q=blue",
        r#"{"label":"chair"}"#,
    );
    assert_eq!(response.0, 200, "{response:?}");
    let response_json: Value = serde_json::from_str(&response.1).expect("JSON handler response");
    assert_eq!(response_json, json!({"item_id":"42", "q":"blue", "label":"chair"}));
    wait_for_request_log(&mut session, "POST /items/42 -> 200 · python");
    stop_mock_server_from_ui(&mut session);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_api_mock_python_handler_receives_missing_optional_query_as_none() {
    let (dir, mut session, route_idx) = python_route_session("api-mock-python-missing-query");
    set_route_path(&mut session, route_idx, "/items/{item_id}");
    {
        let script = session.app.ide_panel.api.mock.manual_routes[route_idx]
            .python
            .as_mut()
            .expect("Python handler");
        script.contract.query.fields = vec![ApiMockContractField::new(
            "q",
            ApiMockContractFieldKind::String,
            false,
        )];
    }
    enable_query_and_body(&mut session, route_idx);
    set_contract_handler_body(
        &mut session,
        route_idx,
        "return json_response({\"item_id\": item_id, \"q\": query.q})",
    );

    let url = start_mock_server(&mut session);
    let response = get_from_mock(&mut session, &url, "/items/42");
    assert_eq!(response.0, 200, "{response:?}");
    let response_json: Value = serde_json::from_str(&response.1).expect("JSON handler response");
    assert_eq!(response_json, json!({"item_id":"42", "q":null}));
    wait_for_request_log(&mut session, "GET /items/42 -> 200 · python");
    stop_mock_server_from_ui(&mut session);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_api_mock_python_timeout_returns_500_logs_timeout_and_server_recovers() {
    let (dir, mut session, route_idx) = python_route_session("api-mock-python-timeout");
    let path = session.app.ide_panel.api.mock.manual_routes[route_idx].path.clone();
    set_timeout_ms(&mut session, route_idx, 100);
    set_contract_handler_body(&mut session, route_idx, "while True: pass");
    let url = start_mock_server(&mut session);

    let timed_out = get_from_mock(&mut session, &url, &path);
    assert_eq!(timed_out.0, 500, "{timed_out:?}");
    assert!(timed_out.1.contains("Python mock timeout"), "{timed_out:?}");
    wait_for_request_log(&mut session, &format!("GET {path} -> 500 · python"));

    set_contract_handler_body(
        &mut session,
        route_idx,
        "return text_response(\"recovered\")",
    );
    assert_eq!(get_from_mock(&mut session, &url, &path), (200, "recovered".to_string()));
    wait_for_request_log(&mut session, &format!("GET {path} -> 200 · python"));
    stop_mock_server_from_ui(&mut session);
    let _ = std::fs::remove_dir_all(dir);
}
