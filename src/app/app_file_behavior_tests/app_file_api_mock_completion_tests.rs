#[test]
fn api_mock_autocomplete_applies_lsp_text_edit_and_imports_to_prelude() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);
    app.toggle_api_route_python(0);
    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    app.ide_panel.api.input_editor.set_text_clean("    Res");
    app.ide_panel.api.input_editor.cursor = app.ide_panel.api.input_editor.len();

    let (method, path, route, model) = app.api_mock_route_context(0).unwrap();
    let script = app.api_mock_script_for_tools(0).unwrap();
    let virtual_source = crate::app::api_mock::ty_check::build_api_mock_virtual_source(
        method, &path, &route, &model, &script,
    );
    let start = virtual_source.source.rfind("Res").unwrap();
    let edit = text_change_for_source_span(
        &virtual_source.source,
        start,
        start + "Res".len(),
        "Response",
    );
    app.autocomplete_active = true;
    app.autocomplete_mode = crate::app::AutocompleteMode::TyContext;
    app.autocomplete_options = vec![(
        crate::app::AutocompleteItem {
            word: "Response".to_string(),
            kind: crate::highlighter::SymbolKind::Class,
            scope_start: 0,
            scope_end: usize::MAX,
            module: None,
            module_path: None,
            detail: None,
            insert_text: None,
            text_edit: Some(edit),
            additional_text_edits: vec![crate::lsp::TextChange {
                start_line: 0,
                start_col: 0,
                end_line: 0,
                end_col: 0,
                new_text: "from typing import Any\n".to_string(),
            }],
        },
        Vec::new(),
    )];

    assert!(app.apply_api_mock_autocomplete());

    let script = app.api_mock_script_for_tools(0).unwrap();
    assert_eq!(script.body, "    Response");
    assert!(script.prelude.contains("from typing import Any"));
}

#[test]
fn api_mock_signature_parameters_feed_autocomplete_like_python_editor() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);
    app.toggle_api_route_python(0);
    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    app.ide_panel
        .api
        .input_editor
        .set_text_clean("    result = make_item(");
    app.ide_panel.api.input_editor.cursor = app.ide_panel.api.input_editor.len();
    app.autocomplete_mode = crate::app::AutocompleteMode::TyContext;

    app.update_api_mock_ty_signature_help_autocomplete(vec!["value".to_string()]);

    assert!(app.autocomplete_active);
    assert_eq!(app.autocomplete_options[0].0.word, "value");
    assert_eq!(
        app.autocomplete_options[0].0.insert_text.as_deref(),
        Some("value=")
    );
    assert!(app.apply_api_mock_autocomplete());
    assert_eq!(
        app.ide_panel.api.input_editor.get_full_text(),
        "    result = make_item(value="
    );
}

#[test]
fn api_mock_tree_sitter_completions_show_before_ty_merge() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);
    app.toggle_api_route_python(0);
    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    app.ide_panel.api.input_editor.set_text_clean("    r");
    app.ide_panel.api.input_editor.cursor = app.ide_panel.api.input_editor.len();
    app.ide_panel.api.mock_highlighter.completions = vec![crate::highlighter::CompletionItem {
        word: "Response".to_string(),
        kind: crate::highlighter::SymbolKind::Class,
        scope_start: 0,
        scope_end: usize::MAX,
    }];

    app.update_api_mock_tree_sitter_autocomplete();

    assert!(app.autocomplete_active);
    assert_eq!(
        app.autocomplete_mode,
        crate::app::AutocompleteMode::TreeSitter
    );
    assert_eq!(app.autocomplete_options[0].0.word, "Response");
}

#[test]
fn api_mock_contract_completion_suggests_all_constraint_markers() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);
    app.toggle_api_route_python(0);
    app.focus_api_input(crate::app::api_client::ApiFocus::MockContract { route_idx: 0 });
    app.ide_panel
        .api
        .input_editor
        .set_text_clean("class Query:\n    name: Annotated[str, ");
    app.ide_panel.api.input_editor.cursor = app.ide_panel.api.input_editor.len();

    app.update_api_mock_tree_sitter_autocomplete();

    assert!(app.autocomplete_active);
    for marker in [
        "MinLen", "MaxLen", "Pattern", "Ge", "Gt", "Le", "Lt", "MinItems", "MaxItems",
    ] {
        assert!(
            app.autocomplete_options
                .iter()
                .any(|(item, _)| item.word == marker),
            "{marker} missing"
        );
    }
    let max_len = app
        .autocomplete_options
        .iter()
        .find(|(item, _)| item.word == "MaxLen")
        .map(|(item, _)| item)
        .unwrap();
    assert_eq!(max_len.insert_text.as_deref(), Some("MaxLen(255)"));
    assert!(
        max_len
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("max string length"))
    );
}

#[test]
fn api_mock_contract_field_editor_commits_constraints_and_flags() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_query_test_route(&mut app);
    app.toggle_api_route_python(0);

    use crate::app::api_client::ApiFocus;
    use crate::ui_system::{ApiMockContractFieldGroup, ApiMockContractFieldProp};

    app.handle_ui_click(crate::ui_system::UiId::ApiMockContractFieldAddConstraint(
        0,
        ApiMockContractFieldGroup::Query,
        0,
    ));
    assert_eq!(
        app.ide_panel.api.mock_contract_constraint_menu,
        Some(crate::app::api_client::ApiMockContractConstraintMenu {
            route_idx: 0,
            group: ApiMockContractFieldGroup::Query,
            field_idx: 0,
        })
    );
    app.handle_ui_click(
        crate::ui_system::UiId::ApiMockContractFieldAddConstraintOption(
            0,
            ApiMockContractFieldGroup::Query,
            0,
            ApiMockContractFieldProp::Default,
        ),
    );
    assert!(matches!(
        app.ide_panel.api.focused,
        Some(ApiFocus::MockContractField {
            prop: ApiMockContractFieldProp::Default,
            ..
        })
    ));
    app.edit_api_mock_contract(0, |api, active| api
        .toggle_api_mock_contract_field_required(active, 0, ApiMockContractFieldGroup::Query, 0));
    app.edit_api_mock_contract(0, |api, active| api
        .toggle_api_mock_contract_field_nullable(active, 0, ApiMockContractFieldGroup::Query, 0));
    for (prop, text) in [
        (ApiMockContractFieldProp::Default, "guest"),
        (ApiMockContractFieldProp::Enum, "guest, admin"),
        (ApiMockContractFieldProp::MinLength, "2"),
        (ApiMockContractFieldProp::MaxLength, "16"),
        (ApiMockContractFieldProp::Pattern, "^[a-z]+$"),
        (ApiMockContractFieldProp::Minimum, "1"),
        (ApiMockContractFieldProp::Maximum, "99"),
        (ApiMockContractFieldProp::MinItems, "1"),
        (ApiMockContractFieldProp::MaxItems, "3"),
    ] {
        app.focus_api_input(ApiFocus::MockContractField {
            route_idx: 0,
            group: ApiMockContractFieldGroup::Query,
            field_idx: 0,
            prop,
        });
        app.ide_panel.api.input_editor.set_text_clean(text);
        app.commit_api_focus();
    }

    let script = app.api_mock_script_for_tools(0).unwrap();
    let field = &script.contract.query.fields[0];
    assert!(!field.required);
    assert!(field.nullable);
    assert_eq!(field.default_value.as_deref(), Some("guest"));
    assert_eq!(field.enum_values, ["guest", "admin"]);
    assert_eq!(field.constraints.min_length, Some(2));
    assert_eq!(field.constraints.max_length, Some(16));
    assert_eq!(field.constraints.pattern.as_deref(), Some("^[a-z]+$"));
    assert_eq!(field.constraints.minimum.as_deref(), Some("1"));
    assert_eq!(field.constraints.maximum.as_deref(), Some("99"));
    assert_eq!(field.constraints.min_items, Some(1));
    assert_eq!(field.constraints.max_items, Some(3));
    assert!(field.constraints.nullable);
    assert!(script.contract_source.contains("class NameEnum(StrEnum):"));
    assert!(script.contract_source.contains("name: Annotated[NameEnum"));
    assert!(script.contract_source.contains("MaxLen(16)"));
    assert!(script.contract_source.contains("Pattern("));
}

#[test]
fn api_mock_detail_popup_uses_api_editor_cursor_context() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);
    app.toggle_api_route_python(0);
    app.editor = editor_with("main_cursor_should_not_win");
    app.editor.cursor = 0;
    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    app.ide_panel
        .api
        .input_editor
        .set_text_clean("    Response");
    app.ide_panel.api.input_editor.cursor = app.ide_panel.api.input_editor.len();
    app.autocomplete_active = true;
    app.autocomplete_selected_idx = 0;
    app.autocomplete_options = vec![(
        crate::app::AutocompleteItem {
            word: "Response".to_string(),
            kind: crate::highlighter::SymbolKind::Class,
            scope_start: 0,
            scope_end: usize::MAX,
            module: None,
            module_path: None,
            detail: Some("class Response".to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        },
        Vec::new(),
    )];

    app.refresh_autocomplete_detail_popup();

    let popup = app.autocomplete_detail_popup.as_ref().unwrap();
    assert_eq!(popup.byte_offset, app.ide_panel.api.input_editor.cursor);
    assert_ne!(popup.byte_offset, app.editor.cursor);
}

#[test]
fn api_mock_popup_keys_move_selection_and_refresh_detail_like_editor() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);
    app.toggle_api_route_python(0);
    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    app.autocomplete_active = true;
    app.autocomplete_mode = crate::app::AutocompleteMode::TreeSitter;
    app.autocomplete_options = vec![
        (
            crate::app::AutocompleteItem {
                word: "alpha".to_string(),
                kind: crate::highlighter::SymbolKind::Function,
                scope_start: 0,
                scope_end: usize::MAX,
                module: None,
                module_path: None,
                detail: Some("alpha detail".to_string()),
                insert_text: None,
                text_edit: None,
                additional_text_edits: Vec::new(),
            },
            Vec::new(),
        ),
        (
            crate::app::AutocompleteItem {
                word: "beta".to_string(),
                kind: crate::highlighter::SymbolKind::Function,
                scope_start: 0,
                scope_end: usize::MAX,
                module: None,
                module_path: None,
                detail: Some("beta detail".to_string()),
                insert_text: None,
                text_edit: None,
                additional_text_edits: Vec::new(),
            },
            Vec::new(),
        ),
    ];

    let result = app.handle_active_autocomplete_key(
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowDown),
        false,
    );

    assert_eq!(result, crate::app::AutocompletePopupKeyResult::Consumed);
    assert_eq!(app.autocomplete_selected_idx, 1);
    assert!(
        app.autocomplete_detail_popup
            .as_ref()
            .is_some_and(|popup| popup.text.contains("beta detail"))
    );
}

#[test]
fn api_mock_enter_completion_keeps_body_focus() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);
    app.toggle_api_route_python(0);
    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    app.ide_panel.api.input_editor.set_text_clean("    Res");
    app.ide_panel.api.input_editor.cursor = app.ide_panel.api.input_editor.len();
    app.autocomplete_active = true;
    app.autocomplete_options = vec![(
        api_lsp_item(
            "Response",
            crate::highlighter::SymbolKind::Class,
            None,
            None,
        )
        .into(),
        Vec::new(),
    )];

    let result = app.handle_active_autocomplete_key(
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Enter),
        false,
    );

    assert_eq!(result, crate::app::AutocompletePopupKeyResult::Consumed);
    assert_eq!(
        app.ide_panel.api.focused,
        Some(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 })
    );
}

#[test]
fn api_mock_pending_enter_and_tab_share_main_autocomplete_gate() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);
    app.toggle_api_route_python(0);
    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    app.ide_panel
        .api
        .input_editor
        .set_text_clean("    response.");
    app.ide_panel.api.input_editor.cursor = app.ide_panel.api.input_editor.len();
    app.autocomplete_mode = crate::app::AutocompleteMode::TyContext;
    app.autocomplete_pending_request_id = Some(77);

    assert!(
        app.mark_pending_autocomplete_apply_for_key(winit::keyboard::PhysicalKey::Code(
            winit::keyboard::KeyCode::Enter
        ))
    );
    assert!(app.autocomplete_apply_pending_response);

    app.autocomplete_apply_pending_response = false;
    app.autocomplete_pending_request_id = Some(78);
    assert!(
        app.mark_pending_autocomplete_apply_for_key(winit::keyboard::PhysicalKey::Code(
            winit::keyboard::KeyCode::Tab
        ))
    );
    assert!(app.autocomplete_apply_pending_response);
}

#[test]
fn api_mock_ty_exact_single_match_closes_like_main_editor() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);
    app.toggle_api_route_python(0);
    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    app.ide_panel
        .api
        .input_editor
        .set_text_clean("    model_dump");
    app.ide_panel.api.input_editor.cursor = app.ide_panel.api.input_editor.len();
    app.autocomplete_active = true;
    app.autocomplete_mode = crate::app::AutocompleteMode::TyContext;

    app.update_api_mock_ty_autocomplete(vec![api_lsp_item(
        "model_dump",
        crate::highlighter::SymbolKind::Function,
        Some("Response"),
        Some("def Response.model_dump(self) -> dict"),
    )]);

    assert!(!app.autocomplete_active);
    assert!(app.autocomplete_options.is_empty());
}

#[test]
fn api_mock_ty_member_order_reuses_python_owner_and_private_ranking() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);
    app.toggle_api_route_python(0);
    app.ide_panel
        .api
        .mock
        .route_overrides
        .iter_mut()
        .find_map(|override_route| override_route.python.as_mut())
        .unwrap()
        .prelude =
        "class GrandBase:\n    grand_public: int\n\nclass BoxReadPublic(GrandBase):\n    base_public: int\n    _base_hidden: int\n\nclass BoxRead(BoxReadPublic):\n    current_public: int\n    _current_hidden: int\n"
            .to_string();
    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });
    app.ide_panel.api.input_editor.set_text_clean("    box.");
    app.ide_panel.api.input_editor.cursor = app.ide_panel.api.input_editor.len();
    app.autocomplete_mode = crate::app::AutocompleteMode::TyContext;

    app.update_api_mock_ty_autocomplete(vec![
        api_lsp_item(
            "base_public",
            crate::highlighter::SymbolKind::Class,
            Some("int"),
            Some("int"),
        ),
        api_lsp_item(
            "_base_hidden",
            crate::highlighter::SymbolKind::Class,
            Some("int"),
            Some("int"),
        ),
        api_lsp_item(
            "grand_public",
            crate::highlighter::SymbolKind::Class,
            Some("int"),
            Some("int"),
        ),
        api_lsp_item(
            "current_public",
            crate::highlighter::SymbolKind::Class,
            Some("int"),
            Some("int"),
        ),
        api_lsp_item(
            "_current_hidden",
            crate::highlighter::SymbolKind::Class,
            Some("int"),
            Some("int"),
        ),
        api_lsp_item(
            "model_dump",
            crate::highlighter::SymbolKind::Function,
            Some("BoxRead"),
            Some("def BoxRead.model_dump(self) -> dict"),
        ),
        api_lsp_item(
            "base_method",
            crate::highlighter::SymbolKind::Function,
            Some("BoxReadPublic"),
            Some("def BoxReadPublic.base_method(self) -> dict"),
        ),
        api_lsp_item(
            "mro",
            crate::highlighter::SymbolKind::Function,
            Some("BoxRead"),
            Some("def BoxRead.mro(self) -> list[type]"),
        ),
    ]);

    let words = app
        .autocomplete_options
        .iter()
        .map(|(item, _)| item.word.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        words,
        vec![
            "current_public",
            "model_dump",
            "_current_hidden",
            "base_public",
            "base_method",
            "_base_hidden",
            "grand_public",
            "mro",
        ]
    );
}

#[test]
fn api_mock_python_hover_uses_virtual_source_fallback() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);
    app.toggle_api_route_python(0);
    app.focus_api_input(crate::app::api_client::ApiFocus::MockBody { route_idx: 0 });

    let body = app.ide_panel.api.input_editor.get_full_text();
    let edit_byte = body.find("Response").unwrap();
    let target = crate::app::api_client::ApiMockHoverTarget {
        route_idx: 0,
        part: crate::app::api_mock::ty_check::ApiMockSourcePart::Body,
        edit_byte,
        version: app.ide_panel.api.input_editor.version,
    };
    let source = "class Response:\n    ok: bool = True\n\nreturn Response(ok=True)".to_string();
    let source_cursor = source.rfind("Response").unwrap();
    app.ide_panel.api.mock_hover_target = Some(target.clone());
    app.ide_panel.api.mock_hover_request = Some(crate::app::api_client::ApiMockHoverRequest {
        request_id: 77,
        target,
        source,
        source_cursor,
        anchor: (0.0, 0.0),
    });
    crate::app::mouse::HOVER_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.byte_offset = Some(edit_byte);
        state.request_id = Some(77);
        state.popup = None;
        state.pending_popup = None;
    });

    assert!(app.apply_api_mock_hover_response(77, Some("None".to_string())));
    crate::app::mouse::HOVER_STATE.with(|state| {
        let mut state = state.borrow_mut();
        let popup_text = state.popup.as_ref().map(|popup| popup.text.as_str());
        assert!(popup_text.is_some_and(|text| {
            text.starts_with("[[MODULE]] api_mock.mock_api.get_users\n")
                && text.contains("class Response")
        }));
        state.popup = None;
        state.byte_offset = None;
    });
}

#[test]
fn api_mock_hover_clears_old_popup_when_crossing_mock_editors() {
    let Some(mut app) = test_app() else {
        return;
    };
    open_api_mock_test_route(&mut app);
    app.toggle_api_route_python(0);
    app.ui_registry.register_text_input(
        crate::ui_system::UiId::ApiMockPreludeInput(0),
        10.0,
        10.0,
        220.0,
        80.0,
        20.0,
        20.0,
    );

    let old_target = crate::app::api_client::ApiMockHoverTarget {
        route_idx: 0,
        part: crate::app::api_mock::ty_check::ApiMockSourcePart::Body,
        edit_byte: 3,
        version: 1,
    };
    app.ide_panel.api.mock_hover_target = Some(old_target);
    crate::app::mouse::HOVER_STATE.with(|state| {
        let mut state = state.borrow_mut();
        *state = crate::app::mouse::HoverState::default();
        state.byte_offset = Some(3);
        state.request_id = Some(44);
        state.rect = Some((40.0, 40.0, 180.0, 90.0));
        state.popup = Some(crate::app::mouse::HoverPopup {
            text: "old".to_string(),
            spans: Vec::new(),
            line_kinds: Vec::new(),
            inline_code_ranges: Vec::new(),
            byte_offset: 3,
            anchor_x: 40.0,
            anchor_y: 40.0,
            offset_x: Some(6.0),
            offset_y: Some(8.0),
            anim_progress: 1.0,
            scroll: crate::scroll::ScrollState::new(15.0),
            layout_cache: None,
        });
    });

    assert!(app.update_api_mock_hover_from_cursor(20.0, 20.0, false, false));

    assert!(app.ide_panel.api.mock_hover_target.is_none());
    crate::app::mouse::HOVER_STATE.with(|state| {
        let mut state = state.borrow_mut();
        assert!(state.popup.is_none());
        assert!(state.byte_offset.is_none());
        assert!(state.request_id.is_none());
        assert!(state.rect.is_none());
        *state = crate::app::mouse::HoverState::default();
    });
}

#[test]
fn cancelling_pointer_interactions_ends_api_scroll_drags_across_shared_states() {
    let Some(mut app) = test_app() else {
        return;
    };
    let spec_id = open_api_mock_test_route(&mut app);
    let (_, state) = app.active_api_tab_mut_for(spec_id).expect("active API tab");
    for scroll in [
        &mut state.body_scroll,
        &mut state.body_scroll_x,
        &mut state.output_scroll,
        &mut state.output_scroll_x,
        &mut state.mock_static_response_scroll,
        &mut state.mock_static_response_scroll_x,
        &mut state.response_scroll,
        &mut state.response_scroll_x,
        &mut state.output_schema_menu_scroll,
    ] {
        scroll.is_dragging = true;
        scroll.drag_offset = 5.0;
    }
    let mut python_scroll = crate::scroll::ScrollState::new(7.0);
    python_scroll.is_dragging = true;
    python_scroll.drag_offset = 4.0;
    app.ide_panel.api.mock_python_scrolls.insert(
        (
            0,
            crate::app::api_mock::ty_check::ApiMockSourcePart::Body,
        ),
        python_scroll,
    );
    app.ide_panel.is_resizing_left = true;
    app.ide_panel.git.graph_resizing = true;

    app.cancel_pointer_interactions();

    let (_, state) = app.active_api_tab().expect("active API tab");
    for scroll in [
        &state.body_scroll,
        &state.body_scroll_x,
        &state.output_scroll,
        &state.output_scroll_x,
        &state.mock_static_response_scroll,
        &state.mock_static_response_scroll_x,
        &state.response_scroll,
        &state.response_scroll_x,
        &state.output_schema_menu_scroll,
    ] {
        assert!(!scroll.is_dragging);
        assert_eq!(scroll.drag_offset, 0.0);
    }
    let python_scroll = app
        .ide_panel
        .api
        .mock_python_scrolls
        .get(&(0, crate::app::api_mock::ty_check::ApiMockSourcePart::Body))
        .expect("python scroll");
    assert!(!python_scroll.is_dragging);
    assert_eq!(python_scroll.drag_offset, 0.0);
    assert!(!app.ide_panel.is_resizing_left);
    assert!(!app.ide_panel.git.graph_resizing);
}
