//! Headless UI coverage for native picker answers.

use crate::headless::profile::HeadlessOptions;
use crate::headless::tests_support::{
    click_ui, dump, ensure_test_profile_root, has_ui, open_settings_tab, run_script, scratch_dir,
    reset_api_test_state, session_for_test, wait_until,
};
use crate::headless::HeadlessSession;

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;

const OPENAPI_SPEC: &str = r#"{
  "openapi": "3.0.3",
  "info": {"title": "Picker fixture", "version": "1.0.0"},
  "paths": {
    "/widgets": {
      "get": {"responses": {"200": {"description": "ok"}}},
      "post": {"responses": {"201": {"description": "created"}}}
    }
  }
}"#;

#[test]
#[ignore = "bug: queued OpenAPI path does not import a spec; src/app/api_client/api_client_app_request_methods.rs:36"]
fn headless_api_client_imports_openapi_from_picker() {
    ensure_test_profile_root();
    reset_api_test_state();
    let dir = scratch_dir("ui-picker-api-spec");
    let spec_path = dir.join("openapi.json");
    std::fs::write(&spec_path, OPENAPI_SPEC).expect("write OpenAPI fixture");

    let mut session = session_for_test(TEST_WIDTH, TEST_HEIGHT);
    let lines = run_script(
        &mut session,
        format!("scale {TEST_SCALE}\nsettle 2000\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    click_ui(&mut session, "WelcomeIdeMode");
    wait_until(&mut session, 3000, "API Client sidebar", |session| {
        has_ui(&dump(session), "SidebarSlot(ApiClient)")
    });
    click_ui(&mut session, "SidebarSlot(ApiClient)");
    wait_until(&mut session, 3000, "API import control", |session| {
        has_ui(&dump(session), "ApiImportAdd")
    });
    click_ui(&mut session, "ApiImportAdd");
    wait_until(&mut session, 1500, "API file import control", |session| {
        has_ui(&dump(session), "ApiImportFile")
    });
    session
        .app
        .external_requests
        .queue_picker_answer(vec![spec_path.clone()]);
    click_ui(&mut session, "ApiImportFile");

    wait_until(&mut session, 5000, "imported OpenAPI endpoints", |session| {
        let api = &session.app.ide_panel.api;
        api.loading.is_empty()
            && api.selected_model().is_some_and(|model| {
                model.routes.iter().any(|route| route.path == "/widgets")
                    && model.routes.iter().filter(|route| route.path == "/widgets").count() == 2
            })
    });
    let model = session.app.ide_panel.api.selected_model().expect("imported API model");
    assert_eq!(model.title, "Picker fixture");
    assert_eq!(model.routes.iter().filter(|route| route.path == "/widgets").count(), 2);

    drop(session);
    reset_api_test_state();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
#[ignore = "bug: queued welcome-open path leaves the welcome tab open; src/app/events/about/about_tick_input_sections.rs:348"]
fn headless_welcome_open_file_uses_picker_answer() {
    let dir = scratch_dir("ui-picker-open-file");
    let path = dir.join("picked.txt");
    std::fs::write(&path, "opened from picker\n").expect("write open-file fixture");

    let mut session = session_for_test(TEST_WIDTH, TEST_HEIGHT);
    let lines = run_script(
        &mut session,
        format!("scale {TEST_SCALE}\nsettle 2000\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    session
        .app
        .external_requests
        .queue_picker_answer(vec![path.clone()]);
    click_ui(&mut session, "WelcomeOpenFile");
    wait_until(&mut session, 5000, "picked file tab", |session| {
        session.app.tabs.iter().any(|tab| tab.file_path.as_ref() == Some(&path))
    });

    let state = dump(&mut session);
    assert_eq!(state["tabs"][0]["path"], path.display().to_string(), "{state}");
    assert_eq!(session.app.editor.get_full_text(), "opened from picker\n");
    drop(session);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
#[ignore = "bug: queued folder path is not applied to workspaces; src/app/events/about/about_tick_input_sections.rs:297"]
fn headless_settings_ide_add_workspace_uses_picker_answer() {
    let dir = scratch_dir("ui-picker-workspace");
    let mut session = session_for_test(TEST_WIDTH, TEST_HEIGHT);
    let lines = run_script(
        &mut session,
        format!("scale {TEST_SCALE}\nsettle 2000\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    session.app.ide_workspaces.clear();
    click_ui(&mut session, "WelcomeIdeMode");
    open_settings_tab(&mut session, 0);
    session
        .app
        .external_requests
        .queue_picker_answer(vec![dir.clone()]);
    click_ui(&mut session, "SettingsIdeAddWorkspace");

    wait_until(&mut session, 5000, "selected IDE workspace", |session| {
        session.app.ide_workspaces.iter().any(|workspace| {
            crate::platform::paths_equal(workspace, &dir)
        })
    });
    let state = dump(&mut session);
    assert!(has_ui(&state, "SettingsIdeRemoveWorkspace(0)"), "{state}");
    drop(session);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
#[ignore = "bug: queued Save As path does not save the untitled tab; src/app/events/about/about_tick_input_sections.rs:365"]
fn headless_untitled_tab_save_as_writes_picker_answer() {
    let dir = scratch_dir("ui-picker-save-as");
    let path = dir.join("saved-from-picker.txt");
    let options = HeadlessOptions {
        size: (TEST_WIDTH, TEST_HEIGHT),
        allow_writes: true,
        ..HeadlessOptions::default()
    };
    let mut session = match HeadlessSession::new(&options, ensure_test_profile_root()) {
        Ok(session) => session,
        Err((code, message)) => panic!("headless session (code {code}): {message}"),
    };
    session.hz_probe = || None;
    let lines = run_script(
        &mut session,
        format!("scale {TEST_SCALE}\nsettle 2000\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    click_ui(&mut session, "WelcomeNewFile");
    let lines = run_script(&mut session, b"type saved by picker\n");
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    session
        .app
        .external_requests
        .queue_picker_answer(vec![path.clone()]);
    let lines = run_script(&mut session, b"key ctrl+s\n");
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");

    wait_until(&mut session, 5000, "Save As picker write", |session| {
        path.is_file()
            && session.app.tabs.iter().any(|tab| tab.file_path.as_ref() == Some(&path))
    });
    assert_eq!(std::fs::read_to_string(&path).expect("read saved file"), "saved by picker");
    let state = dump(&mut session);
    assert_eq!(state["tabs"][0]["path"], path.display().to_string(), "{state}");
    assert_eq!(state["tabs"][0]["title"], "saved-from-picker.txt", "{state}");
    drop(session);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_welcome_open_file_picker_cancel_keeps_tabs_unchanged() {
    let mut session = session_for_test(TEST_WIDTH, TEST_HEIGHT);
    let lines = run_script(
        &mut session,
        format!("scale {TEST_SCALE}\nsettle 2000\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    session.app.external_requests.queue_picker_answer(vec![]);
    click_ui(&mut session, "WelcomeOpenFile");

    wait_until(&mut session, 5000, "cancelled open-file picker", |session| {
        session.app.open_file_rx.is_none()
    });
    let state = dump(&mut session);
    assert_eq!(state["tabs"].as_array().map(Vec::len), Some(1), "{state}");
    assert_eq!(state["tabs"][0]["path"], serde_json::Value::Null, "{state}");
    assert!(session.app.ide_panel.file_tree_error.is_none(), "{state}");
}
