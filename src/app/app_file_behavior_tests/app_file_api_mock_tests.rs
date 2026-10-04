fn api_mock_test_route() -> crate::app::api_client::ApiRouteRow {
    crate::app::api_client::ApiRouteRow {
        tag: String::new(),
        method: crate::app::api_client::ApiMethod::Get,
        path: "/users".to_string(),
        summary: String::new(),
        description: String::new(),
        operation_id: String::new(),
        security: None,
        path_params: Vec::new(),
        query_params: Vec::new(),
        request_body: None,
        responses: Vec::new(),
    }
}

fn open_api_mock_test_route(app: &mut App) -> crate::app::api_client::ApiSpecId {
    let spec_id = crate::app::api_client::ApiSpecId(777);
    let entry = crate::app::api_client::ApiSpecEntry {
        id: spec_id,
        title: "Mock API".to_string(),
        version: "1".to_string(),
        openapi_version: "3.1.0".to_string(),
        source: crate::app::api_client::ApiSpecSource::Url(
            "https://example.test/openapi.json".to_string(),
        ),
        last_loaded: None,
        last_fetch_secs: None,
        last_parse_secs: None,
        last_url_status: None,
        selected: true,
        error: None,
    };
    let mut model = crate::app::api_client::ApiSpecModel {
        id: spec_id,
        title: "Mock API".to_string(),
        version: "1".to_string(),
        openapi_version: "3.1.0".to_string(),
        servers: Vec::new(),
        routes: vec![api_mock_test_route()],
        route_groups: Vec::new(),
        route_display_paths: Vec::new(),
        security_schemes: Vec::new(),
        root_security: Vec::new(),
        schema_arena: Vec::new(),
    };
    model.rebuild_route_layout_cache();
    app.ide_panel.api.specs.push(entry);
    app.ide_panel.api.models.insert(spec_id, model);
    app.open_api_route(spec_id, 0);
    spec_id
}

fn open_api_mock_query_test_route(app: &mut App) -> crate::app::api_client::ApiSpecId {
    let spec_id = open_api_mock_test_route(app);
    let model = app.ide_panel.api.models.get_mut(&spec_id).unwrap();
    model.routes[0].query_params = vec![crate::app::api_client::ApiParam {
        name: "name".to_string(),
        location: crate::app::api_client::ApiParamLocation::Query,
        required: true,
        primitive_type: crate::app::api_client::ApiPrimitiveType::String,
        item_type: None,
        enum_values: Vec::new(),
        default_value: None,
        example: None,
        examples: Vec::new(),
        description: String::new(),
        constraints: crate::app::api_mock::types::ApiMockFieldConstraints::default(),
    }];
    spec_id
}

fn add_second_api_mock_test_route(app: &mut App, spec_id: crate::app::api_client::ApiSpecId) {
    let model = app.ide_panel.api.models.get_mut(&spec_id).unwrap();
    let mut route = api_mock_test_route();
    route.path = "/orders".to_string();
    model.routes.push(route);
    model.rebuild_route_layout_cache();
}

fn api_client_tab_route_idx(tab: &crate::app::EditorTab) -> Option<usize> {
    match &tab.kind {
        crate::app::EditorTabKind::ApiClient(_, state) => state.route_idx,
        _ => None,
    }
}

#[test]
fn manual_mock_schema_focus_uses_visible_schema_text_for_selection() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.ide_panel
        .api
        .mock
        .manual_routes
        .push(crate::app::api_mock::types::ApiManualRoute {
            stable_id: "manual-users".to_string(),
            method: crate::app::api_client::ApiMethod::Get,
            path: "/users/{user_id}".to_string(),
            enabled: true,
            response: crate::app::api_mock::types::ApiMockResponse::Generated,
            python: Some(crate::app::api_mock::types::default_api_mock_python_script()),
            input_fields: Vec::new(),
            output_fields: Vec::new(),
        });

    app.open_api_manual_route(0);
    app.focus_api_input(crate::app::api_client::ApiFocus::InputSchema {
        spec_id: crate::app::api_client::API_MANUAL_MOCK_SPEC_ID,
        route_idx: 0,
    });

    let text = app.ide_panel.api.input_editor.get_full_text();
    assert!(text.contains("user_id"));
    app.ide_panel.api.input_editor.select_all();
    assert_eq!(
        app.ide_panel.api.input_editor.get_selection().as_deref(),
        Some(text.as_str())
    );
}

#[test]
fn api_mock_contract_focus_switch_keeps_python_highlight_cache() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);
    app.toggle_api_route_python(0);
    app.focus_api_input(crate::app::api_client::ApiFocus::MockContract { route_idx: 0 });
    app.ide_panel.api.input_editor.cursor = 0;
    app.ide_panel.api.input_editor.selection_anchor = None;
    let version = app.ide_panel.api.input_editor.version;
    let cache_key = (
        0,
        crate::app::api_mock::ty_check::ApiMockSourcePart::Contract,
    );
    let target = Some((cache_key.0, cache_key.1, version));
    app.ide_panel.api.mock_highlight_cache.insert(
        cache_key,
        vec![crate::highlighter::ColorSpan {
            start: 0,
            end: 5,
            role: crate::theme::SyntaxRole::Keyword,
        }],
    );
    app.ide_panel.api.mock_highlight_target = target;

    app.focus_api_input(crate::app::api_client::ApiFocus::MockPrelude { route_idx: 0 });

    assert!(matches!(
        app.ide_panel.api.focused,
        Some(crate::app::api_client::ApiFocus::MockPrelude { route_idx: 0 })
    ));
    assert_eq!(app.ide_panel.api.mock_highlight_target, target);
    assert!(
        app.ide_panel
            .api
            .mock_highlight_cache
            .contains_key(&cache_key)
    );
}

#[test]
fn api_route_open_reuses_last_api_tab_without_ctrl() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.is_ide_mode = true;
    let spec_id = open_api_mock_test_route(&mut app);
    add_second_api_mock_test_route(&mut app, spec_id);

    app.open_api_route_with_new_tab(spec_id, 0, true);
    assert_eq!(app.tabs.len(), 2);
    app.switch_to_tab(0);

    app.open_api_route(spec_id, 1);

    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.active_tab, 1);
    assert_eq!(api_client_tab_route_idx(&app.tabs[0]), Some(0));
    assert_eq!(api_client_tab_route_idx(&app.tabs[1]), Some(1));
}

#[test]
fn api_route_open_with_ctrl_adds_same_api_tab() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.is_ide_mode = true;
    let spec_id = open_api_mock_test_route(&mut app);
    add_second_api_mock_test_route(&mut app, spec_id);

    app.open_api_route_with_new_tab(spec_id, 1, true);

    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.active_tab, 1);
    assert_eq!(api_client_tab_route_idx(&app.tabs[0]), Some(0));
    assert_eq!(api_client_tab_route_idx(&app.tabs[1]), Some(1));
}

#[test]
fn api_client_stays_visible_when_last_remaining_tab() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.is_ide_mode = true;
    open_api_mock_test_route(&mut app);
    app.open_new_tab();
    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.active_tab, 1);

    app.close_tab_at(1);

    assert_eq!(app.tabs.len(), 1);
    assert_eq!(app.active_tab, 0);
    assert!(app.active_tab_is_api_client());
    assert!(!app.show_welcome);
}

fn text_change_for_source_span(
    source: &str,
    start: usize,
    end: usize,
    new_text: &str,
) -> crate::lsp::TextChange {
    let mut line_offsets = vec![0usize];
    for (idx, b) in source.bytes().enumerate() {
        if b == b'\n' {
            line_offsets.push(idx + 1);
        }
    }
    let (start_line, start_col) = crate::lsp::offset_to_lsp_pos(source, start, &line_offsets);
    let (end_line, end_col) = crate::lsp::offset_to_lsp_pos(source, end, &line_offsets);
    crate::lsp::TextChange {
        start_line,
        start_col,
        end_line,
        end_col,
        new_text: new_text.to_string(),
    }
}

fn api_lsp_item(
    label: &str,
    kind: crate::highlighter::SymbolKind,
    module: Option<&str>,
    detail: Option<&str>,
) -> crate::lsp::LspCompletionItem {
    crate::lsp::LspCompletionItem {
        label: label.to_string(),
        kind,
        module: module.map(str::to_string),
        detail: detail.map(str::to_string),
        insert_text: None,
        text_edit: None,
        additional_text_edits: Vec::new(),
    }
}

#[test]
fn api_mock_python_toggle_preserves_code_and_enables_openapi_mock() {
    let Some(mut app) = test_app() else {
        return;
    };
    let spec_id = open_api_mock_test_route(&mut app);
    app.ide_panel.api.mock.mode = crate::app::api_mock::types::ApiMockMode::MockAll;

    app.toggle_api_route_python(0);
    assert_eq!(
        app.ide_panel.api.mock.mode,
        crate::app::api_mock::types::ApiMockMode::MockSelectedProxyRest
    );
    let override_route = app.ide_panel.api.mock.route_overrides.first().unwrap();
    assert!(override_route.enabled);
    assert!(!override_route.proxy_when_disabled);
    assert!(
        override_route
            .python
            .as_ref()
            .is_some_and(|script| script.enabled)
    );

    let entry = app.ide_panel.api.specs.first().unwrap();
    let model = app.ide_panel.api.models.get(&spec_id).unwrap();
    let routes = crate::app::api_mock::merge::build_api_mock_routes(
        [(entry, model)],
        &app.ide_panel.api.mock,
    );
    assert!(routes.first().unwrap().enabled);

    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    let _ = app
        .ide_panel
        .api
        .input_editor
        .insert_str("\n    value = 42");
    app.toggle_api_route_python(0);
    assert!(
        !app.ide_panel.api.mock.route_overrides[0]
            .python
            .as_ref()
            .unwrap()
            .enabled
    );
    app.toggle_api_route_python(0);
    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    assert!(
        app.ide_panel
            .api
            .input_editor
            .get_full_text()
            .contains("value = 42")
    );
}

#[test]
fn api_route_specific_mock_switches_from_mock_all_to_selected_mode() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);
    app.ide_panel.api.mock.mode = crate::app::api_mock::types::ApiMockMode::MockAll;

    app.toggle_api_route_mock(0);

    assert_eq!(
        app.ide_panel.api.mock.mode,
        crate::app::api_mock::types::ApiMockMode::MockSelectedProxyRest
    );
}

#[test]
fn api_manual_mock_route_keeps_other_routes_in_selected_mode() {
    let Some(mut app) = test_app() else {
        return;
    };
    app.ide_panel.api.mock.mode = crate::app::api_mock::types::ApiMockMode::MockAll;

    app.add_api_manual_route();

    assert_eq!(
        app.ide_panel.api.mock.mode,
        crate::app::api_mock::types::ApiMockMode::MockSelectedProxyRest
    );
}

#[test]
fn api_mock_disable_turns_off_python() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);

    app.toggle_api_route_python(0);
    app.toggle_api_route_mock(0);

    let override_route = app.ide_panel.api.mock.route_overrides.first().unwrap();
    assert!(!override_route.enabled);
    assert!(override_route.proxy_when_disabled);
    assert!(
        !override_route
            .python
            .as_ref()
            .is_some_and(|script| script.enabled)
    );
}

#[test]
fn api_mock_route_reset_removes_override_and_cached_python_editors() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);

    app.toggle_api_route_mock(0);
    app.toggle_api_route_python(0);
    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    let _ = app
        .ide_panel
        .api
        .input_editor
        .insert_str("\n    reset_marker = 1");

    app.handle_ui_click(crate::ui_system::UiId::ApiMockRouteReset(0));

    assert!(app.ide_panel.api.mock_route_reset_dialog.is_some());
    assert_eq!(app.ide_panel.api.mock.route_overrides.len(), 1);
    app.handle_ui_click(crate::ui_system::UiId::ApiMockRouteResetConfirm);

    assert!(app.ide_panel.api.mock.route_overrides.is_empty());
    assert!(app.ide_panel.api.focused.is_none());
    assert!(
        app.ide_panel
            .api
            .mock_python_editors
            .keys()
            .all(|(route_idx, _)| *route_idx != 0)
    );
    assert!(app.ide_panel.api.mock_ty_diagnostics.is_empty());
}

#[test]
fn api_mock_contract_code_edit_updates_field_list_after_focus_leaves() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_query_test_route(&mut app);
    app.toggle_api_route_python(0);
    app.focus_api_input(crate::app::api_client::ApiFocus::MockContract { route_idx: 0 });

    let text = app.ide_panel.api.input_editor.get_full_text();
    let insert_at = text
        .find("class Query:\n")
        .map(|idx| idx + "class Query:\n".len())
        .expect("query contract class exists");
    app.ide_panel.api.input_editor.cursor = insert_at;
    let _ = app
        .ide_panel
        .api
        .input_editor
        .insert_str("    live_value: int\n");

    let script = app.ide_panel.api.mock.route_overrides[0]
        .python
        .as_ref()
        .expect("python mock script");
    assert!(
        !script
            .contract
            .query
            .fields
            .iter()
            .any(|field| field.name == "live_value" && field.enabled)
    );

    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    let script = app.ide_panel.api.mock.route_overrides[0]
        .python
        .as_ref()
        .expect("python mock script");
    assert!(
        script
            .contract
            .query
            .fields
            .iter()
            .any(|field| field.name == "live_value" && field.enabled)
    );
    assert!(
        app.api_mock_contract_source_for_route(0)
            .is_some_and(|source| source.contains("live_value: int"))
    );
}

#[test]
fn api_mock_contract_field_remove_deletes_variable_from_contract() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_query_test_route(&mut app);
    app.toggle_api_route_python(0);

    use crate::ui_system::ApiMockContractFieldGroup;

    let removed_name = app.ide_panel.api.mock.route_overrides[0]
        .python
        .as_ref()
        .expect("python mock script")
        .contract
        .query
        .fields
        .first()
        .expect("query field")
        .name
        .clone();
    app.handle_ui_click(crate::ui_system::UiId::ApiMockContractFieldRemove(
        0,
        ApiMockContractFieldGroup::Query,
        0,
    ));
    assert!(
        app.ide_panel
            .api
            .mock_contract_field_delete_dialog
            .is_some()
    );
    let script = app.ide_panel.api.mock.route_overrides[0]
        .python
        .as_ref()
        .expect("python mock script");
    assert!(
        script
            .contract
            .query
            .fields
            .iter()
            .any(|field| field.name == removed_name)
    );

    app.handle_ui_click(crate::ui_system::UiId::ApiMockContractFieldRemoveConfirm);
    let script = app.ide_panel.api.mock.route_overrides[0]
        .python
        .as_ref()
        .expect("python mock script");
    assert!(
        !script
            .contract
            .query
            .fields
            .iter()
            .any(|field| field.name == removed_name)
    );
    assert!(!script.contract_source.contains(&format!("{removed_name}:")));
}

#[test]
fn api_mock_python_editors_keep_independent_undo_and_reset_parts() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);
    app.toggle_api_route_python(0);

    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    let _ = app
        .ide_panel
        .api
        .input_editor
        .insert_str("\n    body_marker = 1");
    assert!(
        app.ide_panel
            .api
            .input_editor
            .get_full_text()
            .contains("body_marker")
    );

    app.focus_api_input(crate::app::api_client::ApiFocus::MockPrelude { route_idx: 0 });
    let _ = app.ide_panel.api.input_editor.insert_str("import os\n");
    assert!(
        app.ide_panel
            .api
            .input_editor
            .get_full_text()
            .contains("import os")
    );

    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    let _ = app.ide_panel.api.input_editor.undo();
    assert!(
        !app.ide_panel
            .api
            .input_editor
            .get_full_text()
            .contains("body_marker")
    );

    app.focus_api_input(crate::app::api_client::ApiFocus::MockPrelude { route_idx: 0 });
    assert!(
        app.ide_panel
            .api
            .input_editor
            .get_full_text()
            .contains("import os")
    );

    app.reset_api_route_python_part(
        0,
        crate::app::api_mock::ty_check::ApiMockSourcePart::Prelude,
    );
    assert_eq!(app.ide_panel.api.input_editor.get_full_text(), "");

    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    let _ = app
        .ide_panel
        .api
        .input_editor
        .insert_str("\n    reset_me = 1");
    app.reset_api_route_python_part(0, crate::app::api_mock::ty_check::ApiMockSourcePart::Body);
    let body = app.ide_panel.api.input_editor.get_full_text();
    assert_eq!(body, "\n    return Response(ok=True)");
    assert!(!body.contains("reset_me"));
    assert!(
        app.ide_panel
            .api
            .mock_highlight_target
            .is_some_and(|(route_idx, part, _)| {
                route_idx == 0 && part == crate::app::api_mock::ty_check::ApiMockSourcePart::Body
            })
    );
    assert!(
        app.ide_panel
            .api
            .mock_highlight_cache
            .contains_key(&(0, crate::app::api_mock::ty_check::ApiMockSourcePart::Body))
    );
    assert!(app.ide_panel.api.mock_ty_diagnostics.is_empty());
}
