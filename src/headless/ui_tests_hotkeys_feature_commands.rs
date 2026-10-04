//! Headless coverage for configurable feature commands without default chords.

use crate::app::api_mock::types::ApiMockServerStatus;
use crate::headless::tests_support::{
    api_client_session, click_ui, dump, git, git_init, connect_postgres_fixture_through_ui,
    postgres_fixture, run_script, scratch_dir, wheel_until_visible,
    serve_api_spec, serve_http_responses, wait_until, workspace_with_explorer,
};
use crate::headless::HeadlessSession;
use crate::lsp::LspServerStatus;
use serde_json::Value;
use std::path::PathBuf;

use super::ui_tests_hotkeys::install_config_keymap;

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;
const API_PANEL_POINT: (f64, f64) = (200.0, 500.0);
const API_MIN_HITBOX: f64 = 24.0;

fn run_ok(session: &mut HeadlessSession, script: &str) {
    let lines = run_script(session, script.as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
}

fn bind(session: &mut HeadlessSession, name: &str, keymap: Value) -> PathBuf {
    let dir = scratch_dir(name);
    install_config_keymap(session, &dir.join("config.json"), keymap);
    dir
}

fn git_hunks_fixture(name: &str) -> (PathBuf, PathBuf) {
    let dir = scratch_dir(name);
    let file = dir.join("hunks.txt");
    let original = (1..=30)
        .map(|line| format!("line {line:02}\n"))
        .collect::<String>();
    let changed = (1..=30)
        .map(|line| match line {
            3 => "changed first hunk\n".to_string(),
            15 => "changed second hunk\n".to_string(),
            27 => "changed third hunk\n".to_string(),
            _ => format!("line {line:02}\n"),
        })
        .collect::<String>();
    std::fs::write(&file, original).expect("write Git hunk baseline");
    git_init(&dir);
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-qm", "fixture"]);
    std::fs::write(&file, changed).expect("write changed Git hunk file");
    (dir, file)
}

#[test]
fn headless_hotkeys_git_diff_hunk_commands_follow_inline_hunks() {
    let (dir, file) = git_hunks_fixture("ui-hotkeys-feature-git-hunks");
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    run_ok(&mut session, &format!("open {}\n", file.display()));
    wait_until(&mut session, 5000, "inline Git hunks", |session| {
        !session.app.editor.git_hunks.is_empty()
    });
    let config_dir = bind(
        &mut session,
        "ui-hotkeys-feature-git-hunks-config",
        serde_json::json!({
            "editor.git_diff.prev_hunk": ["ctrl+alt+left"],
            "editor.git_diff.next_hunk": ["ctrl+alt+right"]
        }),
    );

    run_ok(&mut session, "key ctrl+alt+right\n");
    let second_hunk = session.app.inline_git_popup.as_ref().map(|popup| popup.hunk_idx);
    assert_eq!(second_hunk, Some(1), "next_hunk did not select the second inline hunk");
    run_ok(&mut session, "key ctrl+alt+right\n");
    let third_hunk = session.app.inline_git_popup.as_ref().map(|popup| popup.hunk_idx);
    assert_eq!(third_hunk, Some(2), "next_hunk did not select the third inline hunk");
    run_ok(&mut session, "key ctrl+alt+left\n");
    assert_eq!(
        session.app.inline_git_popup.as_ref().map(|popup| popup.hunk_idx),
        Some(1),
        "prev_hunk did not return to the preceding inline hunk"
    );

    let _ = std::fs::remove_dir_all(config_dir);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_lsp_restart_and_fix_all_target_active_document() {
    let (dir, file, mut session) = super::ui_tests_rust_lsp::rust_crate_session("hotkeys-lsp-commands");
    let executable = super::ui_tests_rust_lsp::install_rust_fake(
        &mut session,
        "hotkeys-lsp-commands-tools",
        "rust_analyzer_ide_requests",
    );
    super::ui_tests_rust_lsp::open_file(&mut session, &file);
    wait_until(&mut session, 8000, "active Rust language server", |session| {
        session.app.lsp.as_ref().is_some_and(|lsp| {
            lsp.rust_row_info().status == LspServerStatus::Running
        })
    });
    let config_dir = bind(
        &mut session,
        "ui-hotkeys-feature-lsp-config",
        serde_json::json!({
            "lsp.restart_server": ["ctrl+alt+r"],
            "lsp.fix_all": ["ctrl+alt+f"]
        }),
    );

    let starts_before = super::ui_tests_rust_lsp::fake_starts(&executable);
    run_ok(&mut session, "key ctrl+alt+r\n");
    wait_until(&mut session, 8000, "active Rust server restart", |session| {
        super::ui_tests_rust_lsp::fake_starts(&executable) > starts_before
            && session.app.lsp.as_ref().is_some_and(|lsp| {
                lsp.rust_row_info().status == LspServerStatus::Running
            })
    });

    run_ok(&mut session, "key ctrl+alt+f\n");
    wait_until(&mut session, 5000, "fix-all code action request", |_| {
        std::fs::read_to_string(executable.with_extension("requests.jsonl"))
            .is_ok_and(|requests| requests.contains("textDocument/codeAction"))
    });
    let requests = std::fs::read_to_string(executable.with_extension("requests.jsonl"))
        .expect("fake LSP request log");
    assert!(requests.contains("textDocument/codeAction"), "fix_all request missing: {requests}");

    let _ = std::fs::remove_dir_all(config_dir);
    drop(session);
    let _ = std::fs::remove_dir_all(dir);
    if let Some(tools_dir) = executable.parent() {
        let _ = std::fs::remove_dir_all(tools_dir);
    }
}

#[test]
fn headless_hotkeys_api_commands_import_send_toggle_and_export() {
    crate::headless::tests_support::ensure_test_profile_root();
    crate::headless::tests_support::reset_api_test_state();
    let server = serve_http_responses(
        "",
        vec![(200, "OK", "", "{\"ok\":true}".to_string(), std::time::Duration::ZERO)],
    )
    .0;
    let spec_url = serve_api_spec(
        &server,
        serde_json::json!({
            "/send": {"get": {"responses": {"200": {"description": "ok"}}}}
        }),
    );
    let (dir, mut session) = api_client_session("hotkeys-api-commands", &spec_url);
    let unavailable_config = bind(
        &mut session,
        "ui-hotkeys-feature-api-unavailable",
        serde_json::json!({"api.send_request": ["ctrl+alt+s"]}),
    );
    run_ok(&mut session, "key ctrl+alt+s\n");
    assert_eq!(session.app.readonly_notice_text, "Нет активного запроса");

    let export_path = dir.join("mock-export.json");
    let config_dir = bind(
        &mut session,
        "ui-hotkeys-feature-api-config",
        serde_json::json!({
            "api.import_openapi_file": ["ctrl+alt+i"],
            "api.send_request": ["ctrl+alt+s"],
            "api.mock.toggle_server": ["ctrl+alt+t"],
            "api.mock.export_openapi": ["ctrl+alt+e"]
        }),
    );

    let route_idx = session.app.ide_panel.api.selected_model().unwrap().routes
        .iter().position(|route| route.path == "/send").expect("GET /send route");
    let route_id = format!("ApiRouteRow({route_idx})");
    assert!(
        wheel_until_visible(&mut session, API_PANEL_POINT, &route_id, API_MIN_HITBOX, 20),
        "{route_id} did not enter the API panel viewport: {}",
        dump(&mut session)
    );
    click_ui(&mut session, &route_id);
    run_ok(&mut session, "key ctrl+alt+s\n");
    wait_until(&mut session, 8000, "hotkey API request response", |session| {
        session.app.active_api_tab().is_some_and(|(_, state)| {
            state.route_idx == Some(route_idx) && state.response.is_some()
        })
    });
    assert_eq!(session.app.active_api_tab().unwrap().1.response.as_ref().unwrap().status, Some(200));

    run_ok(&mut session, "key ctrl+alt+t\n");
    wait_until(&mut session, 8000, "hotkey API Mock server start", |session| {
        matches!(session.app.ide_panel.api.mock.server_status, ApiMockServerStatus::Running { .. })
    });
    session.app.external_requests.queue_picker_answer(vec![export_path.clone()]);
    run_ok(&mut session, "key ctrl+alt+e\n");
    wait_until(&mut session, 8000, "hotkey API Mock OpenAPI export", |_| export_path.is_file());
    let export = std::fs::read_to_string(&export_path).expect("read API Mock OpenAPI export");
    assert!(export.contains("openapi"), "export is not OpenAPI: {export}");
    run_ok(&mut session, "key ctrl+alt+t\n");
    wait_until(&mut session, 5000, "hotkey API Mock server stop", |session| {
        session.app.ide_panel.api.mock.server_status == ApiMockServerStatus::Stopped
    });

    let imported_path = dir.join("imported.json");
    std::fs::write(
        &imported_path,
        r#"{"openapi":"3.0.0","info":{"title":"Hotkey import","version":"1"},"paths":{"/imported":{"get":{"responses":{"200":{"description":"ok"}}}}}}"#,
    )
    .expect("write OpenAPI import fixture");
    session.app.external_requests.queue_picker_answer(vec![imported_path]);
    run_ok(&mut session, "key ctrl+alt+i\n");
    wait_until(&mut session, 8000, "hotkey imported OpenAPI document", |session| {
        session.app.ide_panel.api.selected_model().is_some_and(|model| {
            model.routes.iter().any(|route| route.path == "/imported")
        })
    });

    let _ = std::fs::remove_dir_all(unavailable_config);
    let _ = std::fs::remove_dir_all(config_dir);
    drop(session);
    crate::headless::tests_support::reset_api_test_state();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_database_refresh_selected_reloads_catalog() {
    let fixture = postgres_fixture();
    let dir = scratch_dir("ui-hotkeys-feature-database");
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    let connection = connect_postgres_fixture_through_ui(
        &mut session,
        &fixture,
        &format!("Database refresh hotkey {}", fixture.port),
    );
    let config_dir = bind(
        &mut session,
        "ui-hotkeys-feature-database-config",
        serde_json::json!({"database.refresh_selected": ["ctrl+alt+d"]}),
    );
    let previous_job_id = session.app.ide_panel.database.next_job_id;

    run_ok(&mut session, "key ctrl+alt+d\n");
    wait_until(&mut session, 8000, "selected Database catalog refresh", |session| {
        session.app.ide_panel.database.connections.get(connection)
            .is_some_and(|node| node.loading || node.databases_loaded)
            && session.app.ide_panel.database.next_job_id > previous_job_id
    });
    wait_until(&mut session, 10000, "refreshed Database catalog", |session| {
        session.app.ide_panel.database.pending_job.is_none()
            && session.app.ide_panel.database.connections.get(connection).is_some_and(|node| {
                node.databases.iter().any(|database| database.name == fixture.database)
            })
    });
    assert!(dump(&mut session)["ui"].as_array().is_some());

    let _ = std::fs::remove_dir_all(config_dir);
    let _ = std::fs::remove_dir_all(dir);
}
