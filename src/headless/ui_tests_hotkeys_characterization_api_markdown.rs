//! Characterization of API Client and Markdown keyboard ownership before keymap changes.

use crate::app::keyboard::KeyInput;
use crate::headless::tests_support::{
    api_client_session, click_ui, dump, ensure_test_profile_root, has_ui, reset_api_test_state,
    run_script, scratch_dir, send_request, serve_api_spec, serve_http_responses, wait_until,
    wheel_until_visible, workspace_with_explorer,
};
use crate::headless::HeadlessSession;
use std::path::Path;
use winit::event::ElementState;
use winit::keyboard::{KeyCode, PhysicalKey};

pub(super) const EXCLUDED_API_MARKDOWN_ROWS: &[(&str, &str)] = &[
    (
        "api.mock.run_route_tools",
        "Alt+Enter was sent with the Python editor focused on the prebuilt binary, but the resulting type-check has no dump-visible status to assert",
    ),
    (
        "api.edit.copy_route_or_hover_selection",
        "the positive Ctrl+C path requires a drag selection over API route or hover text; this probe did not establish a stable selection target",
    ),
];

#[test]
fn headless_hotkeys_api_markdown_excluded_rows_have_reasons() {
    for (row, reason) in EXCLUDED_API_MARKDOWN_ROWS {
        assert!(!row.is_empty() && !reason.is_empty());
    }
}

fn run_ok(session: &mut HeadlessSession, script: &str) {
    let lines = run_script(session, script.as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
}

fn open_markdown_read(dir: &Path, path: &Path) -> HeadlessSession {
    let mut session = workspace_with_explorer(1280, 720, 4.0 / 3.0, dir);
    run_ok(&mut session, &format!("open {}\n", path.display()));
    wait_until(&mut session, 5000, "Markdown mode button", |session| {
        has_ui(&dump(session), "MarkdownModeToggle")
    });
    run_ok(&mut session, "key ctrl+shift+v\n");
    assert_eq!(dump(&mut session)["tabs"][0]["markdown"], true);
    session
}

#[test]
fn headless_hotkeys_characterize_api_request_and_field_redo() {
    let (base, _request_rx) = serve_http_responses(
        "",
        vec![(200, "OK", "", "{\"ok\":true}".to_string(), std::time::Duration::ZERO)],
    );
    let spec = serve_api_spec(
        &base,
        serde_json::json!({
            "/get": {"get": {"responses": {"200": {"description": "ok"}}}}
        }),
    );
    let (dir, mut session) = api_client_session("ui-hotkeys-api-send", &spec);
    let f2 = KeyInput {
        physical_key: PhysicalKey::Code(KeyCode::F2),
        logical_text: None,
        text: None,
        state: ElementState::Pressed,
        repeat: false,
    };
    assert!(!session.app.handle_api_client_keyboard_input(&f2));
    let other_key = KeyInput {
        physical_key: PhysicalKey::Code(KeyCode::KeyA),
        ..f2.clone()
    };
    // today: API Client keys return false while its keyboard surface is hidden.
    assert!(!session.app.handle_api_client_keyboard_input(&other_key));
    let editor_file = dir.join("editor.txt");
    std::fs::write(&editor_file, "").expect("write editor marker fixture");
    run_ok(&mut session, &format!("open {}\n", editor_file.display()));
    wait_until(&mut session, 5000, "editor body", |session| {
        has_ui(&dump(session), "EditorTextBody")
    });
    click_ui(&mut session, "EditorTextBody");
    run_ok(&mut session, "type editor-marker\n");
    assert_eq!(session.app.editor.get_full_text(), "editor-marker");
    let route_idx = session
        .app
        .ide_panel
        .api
        .selected_model()
        .expect("selected API spec")
        .routes
        .iter()
        .position(|route| route.path == "/get")
        .expect("GET route");
    let row_id = format!("ApiRouteRow({route_idx})");
    assert!(wheel_until_visible(&mut session, (200.0, 500.0), &row_id, 24.0, 20));
    click_ui(&mut session, &row_id);
    wait_until(&mut session, 5000, "API request tab", |session| {
        session.app.active_api_tab().is_some_and(|(_, tab)| tab.route_idx == Some(route_idx))
    });
    click_ui(&mut session, "SidebarSlot(ApiClient)");
    assert!(session.app.active_tab_is_api_client());
    assert!(session.app.handle_api_client_keyboard_input(&other_key));
    run_ok(&mut session, "key ctrl+z\n");
    // today: Ctrl+Z on an API request tab undoes the editor buffer.
    assert_eq!(session.app.editor.get_full_text(), "");
    send_request(&mut session, route_idx);
    assert_eq!(
        session.app.active_api_tab().and_then(|(_, tab)| tab.response.as_ref()).and_then(|response| response.status),
        Some(200)
    );
    let tab_count = session.app.tabs.len();
    run_ok(&mut session, "key ctrl+4\n");
    assert_eq!(session.app.tabs.len(), tab_count - 1);
    assert!(!session.app.active_tab_is_api_client());
    let _ = std::fs::remove_dir_all(dir);

    ensure_test_profile_root();
    reset_api_test_state();
    let dir = scratch_dir("ui-hotkeys-api-field-redo");
    let mut session = workspace_with_explorer(1280, 720, 4.0 / 3.0, &dir);
    click_ui(&mut session, "SidebarSlot(ApiClient)");
    wait_until(&mut session, 5000, "API Mock proxy input", |session| {
        has_ui(&dump(session), "ApiMockProxyBaseInput")
    });
    click_ui(&mut session, "ApiMockProxyBaseInput");
    run_ok(&mut session, "key ctrl+a\ntype https://redo.test\nkey ctrl+z\n");
    let input = session.app.ide_panel.api.input_editor.get_full_text();
    run_ok(&mut session, "key ctrl+shift+z\n");
    assert_ne!(session.app.ide_panel.api.input_editor.get_full_text(), input);
    assert_eq!(session.app.ide_panel.api.input_editor.get_full_text(), "https://redo.test");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_characterize_markdown_read_and_toc_ownership() {
    let dir = scratch_dir("ui-hotkeys-markdown-read");
    let path = dir.join("note.md");
    std::fs::write(&path, "# Title\n\nParagraph.\n\n## More\n\nBody.\n")
        .expect("write Markdown fixture");
    let mut session = open_markdown_read(&dir, &path);

    run_ok(&mut session, "key ctrl+a\n");
    assert_eq!(session.app.editor.get_full_text(), "# Title\n\nParagraph.\n\n## More\n\nBody.\n");
    assert_eq!(session.app.editor.selection_anchor, None);
    assert_eq!(dump(&mut session)["overlays"]["readonly_notice"], false);
    run_ok(&mut session, "key ctrl+c\n");
    assert_eq!(dump(&mut session)["clipboard"]["text"], serde_json::Value::Null);

    for chord in ["ctrl+x", "ctrl+v", "ctrl+z", "ctrl+y"] {
        session.app.readonly_notice_until = None;
        run_ok(&mut session, &format!("key {chord}\n"));
        assert!(session.app.readonly_notice_until.is_some(), "{chord}");
    }
    run_ok(&mut session, "key alt+enter\n");
    assert!(session.app.readonly_notice_until.is_some());

    let selection_start = session.app.markdown.read_source.find("Paragraph.").unwrap();
    session.app.markdown.begin_read_selection(selection_start);
    session.app.markdown.update_read_selection(selection_start + "Paragraph.".len());
    session.app.markdown.finish_read_selection();
    run_ok(&mut session, "key ctrl+c\n");
    assert_eq!(dump(&mut session)["clipboard"]["text"], "Paragraph.");

    session.app.editor.cursor = 5;
    run_ok(&mut session, "key ctrl+w\nkey left\nkey right\n");
    assert_eq!(session.app.editor.cursor, 5);
    assert_eq!(session.app.editor.selection_anchor, None);
    run_ok(&mut session, "key escape\nkey ctrl+shift+o\n");
    assert_eq!(dump(&mut session)["markdown_toc"]["open"], true);
    run_ok(&mut session, "key f1\nkey alt+q\nkey ctrl+shift+o\n");
    let state = dump(&mut session);
    assert_eq!(state["markdown_toc"]["open"], true);
    assert_eq!(state["overlays"]["settings"], false);
    assert!(!state["ide_panel"]["open"].as_array().is_some_and(|panels| panels.iter().any(|p| p == "terminal")));
    run_ok(&mut session, "key escape\n");
    assert_eq!(dump(&mut session)["markdown_toc"]["open"], false);

    run_ok(&mut session, "key ctrl+shift+v\n");
    assert_eq!(dump(&mut session)["tabs"][0]["markdown"], false);
    let _ = std::fs::remove_dir_all(dir);
}
