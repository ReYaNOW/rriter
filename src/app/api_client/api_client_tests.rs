#[cfg(test)]
mod tests {
    use super::*;

    fn mock_route_override(
        enabled: bool,
        proxy_when_disabled: bool,
        response: crate::app::api_mock::types::ApiMockResponse,
        python: Option<crate::app::api_mock::types::ApiMockPythonScript>,
    ) -> crate::app::api_mock::types::ApiMockRouteOverride {
        crate::app::api_mock::types::ApiMockRouteOverride {
            source_key: "test".to_string(),
            method: ApiMethod::Get,
            path: "/users".to_string(),
            enabled,
            proxy_when_disabled,
            response,
            python,
            extra_input_fields: Vec::new(),
            extra_output_fields: Vec::new(),
        }
    }

    #[test]
    fn stopped_mock_all_without_route_override_requires_mock_server() {
        assert!(api_mock_request_requires_stopped_server(
            crate::app::api_mock::types::ApiMockMode::MockAll,
            None,
        ));
    }

    #[test]
    fn stopped_selected_proxy_requires_server_only_for_mocked_route() {
        let enabled = mock_route_override(
            true,
            false,
            crate::app::api_mock::types::ApiMockResponse::Generated,
            None,
        );
        let disabled_proxy = mock_route_override(
            false,
            true,
            crate::app::api_mock::types::ApiMockResponse::Generated,
            None,
        );
        let python = mock_route_override(
            false,
            false,
            crate::app::api_mock::types::ApiMockResponse::Generated,
            Some(crate::app::api_mock::types::default_api_mock_python_script()),
        );

        assert!(!api_mock_request_requires_stopped_server(
            crate::app::api_mock::types::ApiMockMode::MockSelectedProxyRest,
            None,
        ));
        assert!(api_mock_request_requires_stopped_server(
            crate::app::api_mock::types::ApiMockMode::MockSelectedProxyRest,
            Some(&enabled),
        ));
        assert!(!api_mock_request_requires_stopped_server(
            crate::app::api_mock::types::ApiMockMode::MockSelectedProxyRest,
            Some(&disabled_proxy),
        ));
        assert!(api_mock_request_requires_stopped_server(
            crate::app::api_mock::types::ApiMockMode::MockAll,
            Some(&python),
        ));
    }

    #[test]
    fn selected_proxy_send_uses_mock_server_only_for_mocked_route() {
        let enabled = mock_route_override(
            true,
            false,
            crate::app::api_mock::types::ApiMockResponse::Generated,
            None,
        );
        let disabled_proxy = mock_route_override(
            false,
            true,
            crate::app::api_mock::types::ApiMockResponse::Generated,
            None,
        );

        assert!(!api_mock_route_wants_server(
            crate::app::api_mock::types::ApiMockMode::MockSelectedProxyRest,
            None,
        ));
        assert!(api_mock_route_wants_server(
            crate::app::api_mock::types::ApiMockMode::MockSelectedProxyRest,
            Some(&enabled),
        ));
        assert!(!api_mock_route_wants_server(
            crate::app::api_mock::types::ApiMockMode::MockSelectedProxyRest,
            Some(&disabled_proxy),
        ));
        assert!(api_mock_route_wants_server(
            crate::app::api_mock::types::ApiMockMode::MockAll,
            None,
        ));
    }

    #[test]
    fn api_mock_virtual_path_is_unique_per_spec_and_route() {
        let a = ApiClientState::api_mock_virtual_path_for(ApiSpecId(1), 0);
        let b = ApiClientState::api_mock_virtual_path_for(ApiSpecId(2), 0);
        let c = ApiClientState::api_mock_virtual_path_for(ApiSpecId(1), 1);

        assert_ne!(a, b);
        assert_ne!(a, c);
    }

    fn sample_spec() -> Value {
        serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "Demo API", "version": "1.2.3"},
            "servers": [
                {"url": "https://api.example.com/{version}", "variables": {"version": {"default": "v1"}}}
            ],
            "components": {
                "schemas": {
                    "Pet": {
                        "type": "object",
                        "required": ["name"],
                        "properties": {
                            "name": {"type": "string"},
                            "age": {"type": "integer"}
                        }
                    }
                }
            },
            "paths": {
                "/pets/{id}": {
                    "get": {
                        "tags": ["pets"],
                        "summary": "Read pet",
                        "description": "Returns one pet.\nRequires the pets:read role.",
                        "parameters": [
                            {"name": "id", "in": "path", "required": true, "schema": {"type": "string"}},
                            {"name": "verbose", "in": "query", "schema": {"type": "boolean"}}
                        ],
                        "responses": {"200": {"description": "ok"}}
                    },
                    "post": {
                        "tags": ["pets"],
                        "requestBody": {
                            "content": {
                                "application/json": {"schema": {"$ref": "#/components/schemas/Pet"}}
                            }
                        },
                        "responses": {"201": {"description": "created"}}
                    }
                }
            }
        })
    }

    fn form_spec() -> Value {
        serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "Form API", "version": "1.0.0"},
            "paths": {
                "/token": {
                    "post": {
                        "requestBody": {
                            "content": {
                                "application/x-www-form-urlencoded": {
                                    "schema": {
                                        "type": "object",
                                        "required": ["username"],
                                        "properties": {
                                            "username": {"type": "string", "maxLength": 500},
                                            "password": {"type": "string"}
                                        }
                                    }
                                },
                                "application/json": {
                                    "schema": {"type": "object"}
                                }
                            }
                        },
                        "responses": {"200": {"description": "ok"}}
                    }
                }
            }
        })
    }

    fn auth_spec() -> Value {
        serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "Auth API", "version": "1.0.0"},
            "components": {
                "securitySchemes": {
                    "HeaderKey": {"type": "apiKey", "in": "header", "name": "X-API-Key"},
                    "QueryKey": {"type": "apiKey", "in": "query", "name": "api_key"},
                    "CookieKey": {"type": "apiKey", "in": "cookie", "name": "session"},
                    "BasicAuth": {"type": "http", "scheme": "basic"},
                    "BearerJwt": {"type": "http", "scheme": "bearer", "bearerFormat": "JWT"},
                    "DigestAuth": {"type": "http", "scheme": "digest"},
                    "OAuthAll": {
                        "type": "oauth2",
                        "flows": {
                            "implicit": {"authorizationUrl": "/oauth/authorize", "scopes": {}},
                            "password": {"tokenUrl": "/oauth/token", "scopes": {}},
                            "clientCredentials": {"tokenUrl": "/oauth/token", "scopes": {}},
                            "authorizationCode": {
                                "authorizationUrl": "/oauth/authorize",
                                "tokenUrl": "/oauth/token",
                                "scopes": {}
                            }
                        }
                    },
                    "Oidc": {
                        "type": "openIdConnect",
                        "openIdConnectUrl": "/.well-known/openid-configuration"
                    }
                }
            },
            "security": [
                {"HeaderKey": [], "BearerJwt": []},
                {"QueryKey": []}
            ],
            "paths": {
                "/items": {
                    "get": {
                        "responses": {"200": {"description": "ok"}}
                    }
                },
                "/basic": {
                    "get": {
                        "security": [{"BasicAuth": []}],
                        "responses": {"200": {"description": "ok"}}
                    }
                },
                "/public": {
                    "get": {
                        "security": [],
                        "responses": {"200": {"description": "ok"}}
                    }
                }
            }
        })
    }

    #[test]
    fn stale_api_focus_clears_so_editor_ctrl_shortcuts_are_not_swallowed() {
        let mut state = ApiClientState::default();
        state.focused = Some(ApiFocus::Body {
            spec_id: ApiSpecId(1),
            route_idx: 0,
        });

        assert!(!state.clear_stale_keyboard_focus(Some((ApiSpecId(2), Some(0)))));
        assert_eq!(state.focused, None);

        state.focused = Some(ApiFocus::Response {
            spec_id: ApiSpecId(1),
            route_idx: 2,
        });
        assert!(state.clear_stale_keyboard_focus(Some((ApiSpecId(1), Some(2)))));
        assert!(state.focused.is_some());

        state.focused = Some(ApiFocus::ImportUrl);
        assert!(state.clear_stale_keyboard_focus(None));
    }

    #[test]
    fn api_input_vertical_arrows_move_cursor_and_shift_selects() {
        let mut editor = Editor::new(64);
        editor.insert_str("abc\ndefg\nhi");
        editor.cursor = 1;

        move_api_input_vertical(&mut editor, true, false);
        assert_eq!(editor.cursor, 5);
        assert_eq!(editor.selection_anchor, None);

        move_api_input_vertical(&mut editor, true, true);
        assert_eq!(editor.cursor, 10);
        assert_eq!(editor.selection_anchor, Some(5));

        move_api_input_vertical(&mut editor, false, false);
        assert_eq!(editor.cursor, 5);
        assert_eq!(editor.selection_anchor, None);
    }

    #[test]
    fn api_mock_python_vertical_edges_jump_between_editable_blocks() {
        let mut editor = Editor::new(64);
        editor.set_text_clean("one\ntwo");
        editor.cursor = editor.len();

        assert!(api_editor_at_vertical_edge(&editor, true));
        assert_eq!(
            api_mock_adjacent_python_part(ApiMockSourcePart::Contract, true),
            Some(ApiMockSourcePart::Body)
        );
        assert_eq!(
            api_mock_adjacent_python_part(ApiMockSourcePart::Body, false),
            Some(ApiMockSourcePart::Contract)
        );
        assert_eq!(
            api_mock_focus_for_part(3, ApiMockSourcePart::Prelude),
            Some(ApiFocus::MockPrelude { route_idx: 3 })
        );

        editor.cursor = 1;
        assert!(!api_editor_at_vertical_edge(&editor, true));
        assert!(api_editor_at_vertical_edge(&editor, false));
    }

    #[test]
    fn api_mock_tools_queue_only_after_same_part_edit() {
        assert_eq!(
            api_mock_tools_queue_route_after_key(
                Some((3, ApiMockSourcePart::Contract)),
                Some((3, ApiMockSourcePart::Contract)),
                10,
                11,
            ),
            Some(3)
        );
        assert_eq!(
            api_mock_tools_queue_route_after_key(
                Some((3, ApiMockSourcePart::Contract)),
                Some((3, ApiMockSourcePart::Prelude)),
                10,
                11,
            ),
            None
        );
        assert_eq!(
            api_mock_tools_queue_route_after_key(
                Some((3, ApiMockSourcePart::Contract)),
                Some((3, ApiMockSourcePart::Contract)),
                10,
                10,
            ),
            None
        );
    }

    #[test]
    fn api_mock_alt_enter_runs_tools_only_inside_python_blocks() {
        assert_eq!(
            api_mock_alt_enter_route_target(Some((7, ApiMockSourcePart::Body)), true, true),
            Some(7)
        );
        assert_eq!(
            api_mock_alt_enter_route_target(Some((7, ApiMockSourcePart::Contract)), false, true),
            None
        );
        assert_eq!(
            api_mock_alt_enter_route_target(Some((7, ApiMockSourcePart::Prelude)), true, false),
            None
        );
        assert_eq!(api_mock_alt_enter_route_target(None, true, true), None);
    }

    #[test]
    fn mock_input_schema_uses_enabled_contract_fields_and_constraints() {
        let mut contract = crate::app::api_mock::types::ApiMockPythonContract::default();
        contract.query.enabled = true;
        let mut query = crate::app::api_mock::types::ApiMockContractField::new(
            "role",
            crate::app::api_mock::types::ApiMockContractFieldKind::String,
            true,
        );
        query.enum_values = vec!["admin".to_string(), "guest".to_string()];
        query.default_value = Some("guest".to_string());
        contract.query.fields.push(query);
        contract.body.enabled = true;
        let mut body = crate::app::api_mock::types::ApiMockContractField::new(
            "age",
            crate::app::api_mock::types::ApiMockContractFieldKind::Integer,
            false,
        );
        body.constraints.minimum = Some("1".to_string());
        body.constraints.maximum = Some("120".to_string());
        contract.body.fields.push(body);

        let text = api_mock_input_schema_text(&contract);

        assert!(!text.contains("\"query\": {"));
        assert!(text.contains("\"role\"*: \"string\""));
        assert!(text.contains("default=guest"));
        assert!(text.contains("enum=[admin|guest]"));
        assert!(!text.contains("\"body\": {"));
        assert!(text.contains("\"age\": 0"));
        assert!(text.contains("minimum=1"));
        assert!(text.contains("maximum=120"));
        assert_eq!(
            api_mock_input_schema_summary(&contract),
            "Mock contract · path 0 · query 1 · body 1"
        );
    }

    #[test]
    fn api_array_editor_uses_blocks_plus_draft() {
        assert_eq!(api_array_editor_text("alpha\nbeta"), "alpha\nbeta\n");
        assert_eq!(
            api_array_edit_parts("alpha\nbeta\ngam"),
            (vec!["alpha", "beta"], "gam")
        );

        let mut editor = Editor::new(64);
        editor.set_text_clean(&api_array_editor_text("alpha\nbeta"));
        editor.cursor = editor.len();
        editor.selection_anchor = Some(editor.cursor);
        editor.insert_str("gam");
        finish_api_array_editor_draft(&mut editor);
        assert_eq!(editor.get_full_text(), "alpha\nbeta\ngam\n");

        backspace_api_array_editor(&mut editor);
        assert_eq!(editor.get_full_text(), "alpha\nbeta\n");
        editor.insert_str("x");
        backspace_api_array_editor(&mut editor);
        assert_eq!(editor.get_full_text(), "alpha\nbeta\n");
    }

    #[test]
    fn api_mock_body_backspace_removes_leading_empty_default_line() {
        let mut editor = Editor::new(64);
        editor.set_text_clean("\n    return Response(ok=True)");
        editor.cursor = 0;
        editor.selection_anchor = Some(0);

        assert_eq!(backspace_api_mock_body_editor(&mut editor), Some((0, 1)));
        assert_eq!(editor.get_full_text(), "    return Response(ok=True)");
        assert_eq!(editor.cursor, editor.len());
    }

    #[test]
    fn api_mock_body_backspace_removes_inner_empty_line_normally() {
        let mut editor = Editor::new(64);
        editor.set_text_clean("    return Response(ok=True)\n\n    status = 200");
        editor.cursor = "    return Response(ok=True)\n".len();
        editor.selection_anchor = Some(editor.cursor);

        assert_eq!(backspace_api_mock_body_editor(&mut editor), Some((28, 1)));
        assert_eq!(
            editor.get_full_text(),
            "    return Response(ok=True)\n    status = 200"
        );
    }

    #[test]
    fn api_mock_server_log_scrollbar_drag_preserves_thumb_offset() {
        let rect = (100.0, 200.0, 14.0, 120.0);
        let line_count = 30;
        let current = 180.0;
        let thumb = api_mock_server_log_scrollbar(rect, line_count, current, 1.0)
            .geometry(1.0)
            .expect("scrollbar")
            .thumb;
        let pointer = thumb.start + 5.0;
        let (offset, target) = api_mock_server_log_scrollbar_drag_target(
            rect, line_count, current, pointer, 1.0, None,
        )
        .expect("drag starts");
        assert_eq!(offset, 5.0);
        assert_eq!(target, current);

        let (_, moved) = api_mock_server_log_scrollbar_drag_target(
            rect,
            line_count,
            target,
            pointer + 20.0,
            1.0,
            Some(offset),
        )
        .expect("drag continues");
        assert!(moved > target);

        let mut scroll = crate::scroll::ScrollState::new(7.0);
        scroll.jump_to(current);
        assert!(crate::app::mouse::apply_scrollbar_drag_target(
            &mut scroll,
            moved,
            offset
        ));
        assert_eq!(scroll.current, current);
        assert_eq!(scroll.target, moved);
        assert!(scroll.is_dragging);
        scroll.update(1.0 / 60.0);
        assert!(scroll.current > current);
        assert!(scroll.current < moved);
    }

    #[test]
    fn api_text_area_horizontal_scroll_uses_longest_line() {
        let max = api_text_area_max_scroll_x("short\nvery-long-line", 40.0, |line| {
            line.len() as f32 * 10.0
        });
        assert_eq!(max, 120.0);
        assert_eq!(
            api_text_area_max_scroll_x("tiny", 100.0, |line| line.len() as f32 * 10.0),
            0.0
        );
    }

    #[test]
    fn api_text_scrollbar_drag_preserves_pointer_offset_inside_thumb() {
        let rect = (100.0, 0.0, 200.0, 10.0);
        let max_scroll = 600.0;
        let current = 300.0;
        let thumb = crate::scroll::scrollbar_thumb(
            rect.0,
            rect.2,
            rect.2,
            rect.2 + max_scroll,
            current,
            22.0,
        )
        .expect("scrollbar thumb");
        let pointer = thumb.start + 7.0;
        let (offset, initial_target) =
            api_text_scrollbar_x_drag_target(rect, current, max_scroll, pointer, 1.0, None)
                .expect("drag starts");
        assert_eq!(offset, 7.0);
        assert_eq!(initial_target, current);

        let (_, moved_target) = api_text_scrollbar_x_drag_target(
            rect,
            initial_target,
            max_scroll,
            pointer + 20.0,
            1.0,
            Some(offset),
        )
        .expect("drag continues");
        assert!(moved_target > initial_target);

        let mut scroll = crate::scroll::ScrollState::new(7.0);
        scroll.jump_to(current);
        assert!(crate::app::mouse::apply_scrollbar_drag_target(
            &mut scroll,
            moved_target,
            offset
        ));
        assert_eq!(scroll.current, current);
        assert_eq!(scroll.target, moved_target);
    }

    #[test]
    fn api_text_vertical_scrollbar_drag_preserves_pointer_offset_inside_thumb() {
        let rect = (0.0, 100.0, 10.0, 200.0);
        let max_scroll = 600.0;
        let current = 300.0;
        let thumb = crate::scroll::scrollbar_thumb(
            rect.1,
            rect.3,
            rect.3,
            rect.3 + max_scroll,
            current,
            22.0,
        )
        .expect("scrollbar thumb");
        let pointer = thumb.start + 7.0;
        let (offset, initial_target) =
            api_text_scrollbar_y_drag_target(rect, current, max_scroll, pointer, 1.0, None)
                .expect("drag starts");
        assert_eq!(offset, 7.0);
        assert_eq!(initial_target, current);

        let (_, moved_target) = api_text_scrollbar_y_drag_target(
            rect,
            initial_target,
            max_scroll,
            pointer + 20.0,
            1.0,
            Some(offset),
        )
        .expect("drag continues");
        assert!(moved_target > initial_target);

        for target in [moved_target, moved_target + 25.0] {
            let mut scroll = crate::scroll::ScrollState::new(7.0);
            scroll.jump_to(current);
            assert!(crate::app::mouse::apply_scrollbar_drag_target(
                &mut scroll,
                target,
                offset
            ));
            assert_eq!(scroll.current, current);
            assert_eq!(scroll.target, target);
            assert_eq!(scroll.drag_offset, offset);
        }
    }

    #[test]
    fn api_output_menu_scrollbar_drag_is_target_only_and_preserves_offset() {
        let scale = 1.0;
        let example_count = 10;
        let (visible_h, max_scroll) = api_output_schema_menu_scroll_metrics(example_count, scale);
        assert_eq!(max_scroll, 120.0);
        let rect = (400.0, 200.0, 12.0, 180.0);
        let current = max_scroll * 0.5;
        let thumb = crate::scroll::scrollbar_thumb(
            rect.1,
            rect.3,
            visible_h,
            visible_h + max_scroll,
            current,
            22.0 * scale,
        )
        .expect("menu thumb");
        let pointer = thumb.start + 6.0;
        let (offset, initial_target) = api_output_schema_menu_scrollbar_drag_target(
            rect,
            example_count,
            current,
            pointer,
            scale,
            None,
        )
        .expect("menu drag starts");
        assert!((initial_target - current).abs() < 0.0001);

        let (_, moved_target) = api_output_schema_menu_scrollbar_drag_target(
            rect,
            example_count,
            current,
            pointer + 24.0,
            scale,
            Some(offset),
        )
        .expect("menu drag moves");
        let mut scroll = crate::scroll::ScrollState::new(7.0);
        scroll.jump_to(current);
        assert!(crate::app::mouse::apply_scrollbar_drag_target(
            &mut scroll,
            moved_target,
            offset
        ));
        assert_eq!(scroll.current, current);
        assert!(scroll.target > current);
        assert_eq!(scroll.drag_offset, offset);
    }

    #[test]
    fn api_python_scrollbar_drag_matches_render_geometry_and_is_target_only() {
        let rect = (100.0, 50.0, 320.0, 158.0);
        let max_scroll = api_python_version_list_max_scroll(20, rect.3, 1.0);
        let current = max_scroll * 0.5;
        let thumb = api_python_scrollbar(rect, current, max_scroll, 1.0)
            .geometry(1.0)
            .expect("python scrollbar")
            .thumb;
        let pointer = thumb.start + 5.0;
        let (offset, initial_target) =
            api_python_scrollbar_drag_target(rect, current, max_scroll, pointer, 1.0, None)
                .expect("python drag starts");
        assert_eq!(initial_target, current);
        let (_, moved_target) = api_python_scrollbar_drag_target(
            rect,
            current,
            max_scroll,
            pointer + 20.0,
            1.0,
            Some(offset),
        )
        .expect("python drag moves");

        let mut scroll = crate::scroll::ScrollState::new(7.0);
        scroll.jump_to(current);
        assert!(crate::app::mouse::apply_scrollbar_drag_target(
            &mut scroll,
            moved_target,
            offset
        ));
        assert_eq!(scroll.current, current);
        assert!(scroll.target > current);
        assert!(api_python_scrollbar_drag_target(rect, 0.0, 0.0, pointer, 1.0, None).is_none());
    }

    #[test]
    fn api_text_area_top_matches_render_baseline_offset() {
        assert_eq!(api_text_area_baseline_offset(1.0), 20.0);
        assert_eq!(api_text_area_top_from_baseline(29.0, 1.0), 9.0);
    }

    #[test]
    fn api_mock_autocomplete_anchor_uses_cursor_baseline_and_scroll_x() {
        let rect = (100.0, 200.0, 300.0, 120.0);
        let text = "seed\n    Response";
        let (x, y) = ApiClientState::api_mock_autocomplete_anchor_for_text(
            crate::ui_system::UiId::ApiMockBodyInput(0),
            rect,
            1.0,
            text,
            text.len(),
            12.0,
            |prefix| prefix.len() as f32 * 7.0,
        );

        assert_eq!(x, 100.0 + 10.0 + "    Response".len() as f32 * 7.0 - 12.0);
        assert_eq!(y, 200.0 + 9.0 + 26.0 + 20.0);
    }

    #[test]
    fn api_mock_signature_autocomplete_anchor_uses_registered_left_edge() {
        let rect = (140.0, 80.0, 360.0, 32.0);
        let text = "def handler";
        let (x, y) = ApiClientState::api_mock_autocomplete_anchor_for_text(
            crate::ui_system::UiId::ApiMockSignatureInput(0),
            rect,
            1.0,
            text,
            text.len(),
            0.0,
            |prefix| prefix.len() as f32 * 5.0,
        );

        assert_eq!(x, 140.0 + "def handler".len() as f32 * 5.0);
        assert_eq!(y, 80.0 + 20.0);
    }

    #[test]
    fn api_mock_hover_uses_editor_line_hitbox_for_vertical_mouse_range() {
        let line_h = api_text_area_line_height(1.0);
        let top_y = 100.0;

        assert!(
            api_mock_hover_content_y_at_point(top_y + line_h * 0.25 - 0.1, top_y, 0.0, line_h)
                .is_none()
        );
        assert!(
            api_mock_hover_content_y_at_point(top_y + line_h * 0.25, top_y, 0.0, line_h).is_some()
        );
        assert!(
            api_mock_hover_content_y_at_point(top_y + line_h * 0.75 - 0.1, top_y, 0.0, line_h)
                .is_some()
        );
        assert!(
            api_mock_hover_content_y_at_point(top_y + line_h * 0.75, top_y, 0.0, line_h).is_none()
        );
    }

    #[test]
    fn api_tab_prefill_uses_selected_restored_route() {
        let model = parse_openapi_model(ApiSpecId(9), &sample_spec()).expect("parse");
        let post_idx = model
            .routes
            .iter()
            .position(|route| route.method == ApiMethod::Post)
            .expect("post route");
        let mut state = ApiClientTabState {
            route_idx: Some(post_idx),
            ..Default::default()
        };

        fill_api_tab_inputs(&mut state, &model.routes[post_idx], &model);

        assert!(state.path_values.is_empty());
        assert!(state.query_values.is_empty());
        assert!(state.body_json.contains("\"name\": \"\""));
        assert!(state.body_json.contains("\"age\": 0"));
    }

    include!("api_client_parse_auth_tests.rs");
    include!("api_client_parse_schema_tests.rs");
    include!("api_client_request_state_tests.rs");

}

#[test]
fn r3_103_url_cache_write_failure_is_reported() {
    let root = std::env::temp_dir().join(format!("rriter-r3-url-cache-{}", crate::platform::next_operation_id()));
    std::fs::write(&root, b"not a directory").unwrap();
    let target = root.join("spec.json");
    let error = save_url_cache_to(&target, "{}").unwrap_err();
    assert!(error.contains("OpenAPI URL cache"));
    let _ = std::fs::remove_file(root);
}

#[test]
fn r3_104_corrupt_api_auth_is_reported_and_backed_up() {
    let root = std::env::temp_dir().join(format!("rriter-r3-auth-{}", crate::platform::next_operation_id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("api_auth.json");
    std::fs::write(&path, b"definitely not a sealed secret").unwrap();
    let error = load_api_auth_from_checked(&path).unwrap_err();
    assert!(error.contains("API credentials"));
    assert!(std::fs::read_dir(&root).unwrap().flatten().any(|entry| {
        entry.file_name().to_string_lossy().starts_with("api_auth.corrupt-")
    }));
    let _ = std::fs::remove_dir_all(root);
}


#[test]
fn preproduction_corrupt_api_specs_are_reported_and_backed_up() {
    let root = std::env::temp_dir().join(format!(
        "rriter-preproduction-api-specs-{}",
        crate::platform::next_operation_id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("api_specs.json");
    std::fs::write(&path, b"{broken").unwrap();

    let error = load_api_specs_from_checked(&path).unwrap_err();

    assert!(error.contains("API specifications"));
    assert!(std::fs::read_dir(&root).unwrap().flatten().any(|entry| {
        entry
            .file_name()
            .to_string_lossy()
            .starts_with("api_specs.corrupt-")
    }));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn request_body_content_type_matches_openapi_media_type() {
    assert!(api_content_type_is_json("application/json"));
    assert!(api_content_type_is_json(
        "Application/Problem+Json; charset=utf-8"
    ));
    assert!(!api_content_type_is_json("text/plain"));
    assert!(!api_content_type_is_json("application/xml"));

    let client = reqwest::blocking::Client::new();
    let request = apply_api_request_body(
        client.post("https://api.example.test/text"),
        Some("text/plain; charset=utf-8"),
        Some("plain payload"),
        None,
        None,
    )
    .build()
    .expect("request");
    assert_eq!(
        request.headers()[reqwest::header::CONTENT_TYPE],
        "text/plain; charset=utf-8"
    );
    assert_eq!(
        request.body().and_then(reqwest::blocking::Body::as_bytes),
        Some(b"plain payload".as_slice())
    );

    let job = ApiJobRequest {
        request_id: 1,
        spec_id: ApiSpecId(1),
        route_idx: 0,
        method: ApiMethod::Post,
        url: "https://api.example.test/text".to_string(),
        mock_target: ApiJobMockTarget::None,
        auth_parts: Vec::new(),
        body_content_type: Some("text/plain; charset=utf-8".to_string()),
        body_json: Some("plain payload".to_string()),
        body_form: None,
        body_multipart: None,
        resolved_host: None,
    };
    let curl = format_api_curl_command_for_platform(&job, crate::platform::PlatformKind::Linux);
    assert!(curl.contains("Content-Type: text/plain; charset=utf-8"));
    assert!(curl.contains("--data-binary 'plain payload'"));
}

#[test]
fn bodyless_post_does_not_invent_json_content_type_or_body() {
    let client = reqwest::blocking::Client::new();
    let request = apply_api_request_body(
        client.post("https://api.example.test/ping"),
        None,
        None,
        None,
        None,
    )
    .build()
    .expect("request");
    assert!(!request.headers().contains_key(reqwest::header::CONTENT_TYPE));
    assert!(request.body().is_none());

    let job = ApiJobRequest {
        request_id: 2,
        spec_id: ApiSpecId(1),
        route_idx: 0,
        method: ApiMethod::Post,
        url: "https://api.example.test/ping".to_string(),
        mock_target: ApiJobMockTarget::None,
        auth_parts: Vec::new(),
        body_content_type: None,
        body_json: None,
        body_form: None,
        body_multipart: None,
        resolved_host: None,
    };
    let curl = format_api_curl_command_for_platform(&job, crate::platform::PlatformKind::Linux);
    assert!(!curl.contains("Content-Type:"));
    assert!(!curl.contains("--data-binary"));
}

#[test]
fn api_request_response_wakes_the_ui_loop() {
    // Nothing listens on a port that was just released: the worker answers with an error.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("free port")
        .port();
    let job = ApiJobRequest {
        request_id: 9,
        spec_id: ApiSpecId(1),
        route_idx: 0,
        method: ApiMethod::Get,
        url: format!("http://127.0.0.1:{port}/ping"),
        mock_target: ApiJobMockTarget::None,
        auth_parts: Vec::new(),
        body_content_type: None,
        body_json: None,
        body_form: None,
        body_multipart: None,
        resolved_host: None,
    };
    let ui_waker = crate::ui_waker::UiWaker::counting();
    let rx = spawn_api_request(job, &ui_waker);
    let response = rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("API worker response");
    assert_eq!(response.request_id, 9);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while ui_waker.take_events() == 0 {
        assert!(std::time::Instant::now() < deadline, "API response must wake the UI");
        std::thread::yield_now();
    }
}

#[test]
fn request_worker_disconnect_is_attached_to_the_pending_response() {
    let mut state = ApiClientTabState {
        route_idx: Some(4),
        pending: true,
        pending_request_id: Some(77),
        ..Default::default()
    };
    state.response_scroll.current = 20.0;
    state.response_scroll_x.current = 12.0;

    assert!(state.apply_request_disconnect(ApiSpecId(8), 77));
    assert!(!state.pending);
    assert_eq!(state.pending_request_id, None);
    assert_eq!(state.response_scroll.current, 0.0);
    assert_eq!(state.response_scroll_x.current, 0.0);
    let response = state.response.expect("disconnect response");
    assert_eq!(response.request_id, 77);
    assert_eq!(response.spec_id, ApiSpecId(8));
    assert_eq!(response.route_idx, 4);
    assert_eq!(
        response.error.expect("disconnect error").kind,
        ApiLoadErrorKind::Io
    );
}

#[test]
fn multipart_limit_counts_text_fields_headers_and_final_boundary() {
    let parts = vec![ApiMultipartPart::Text {
        name: "note".to_string(),
        value: "payload".to_string(),
    }];
    let (_, encoded) = build_multipart_body_with_limit(&parts, 31, usize::MAX)
        .expect("unlimited multipart");
    let error = build_multipart_body_with_limit(&parts, 31, encoded.len().saturating_sub(1))
        .expect_err("one byte below exact encoded size must fail");
    assert_eq!(error.kind, ApiLoadErrorKind::TooLarge);

    let (_, empty) = build_multipart_body_with_limit(&[], 32, usize::MAX)
        .expect("empty multipart");
    let error = build_multipart_body_with_limit(&[], 32, empty.len().saturating_sub(1))
        .expect_err("final boundary must count toward the limit");
    assert_eq!(error.kind, ApiLoadErrorKind::TooLarge);
}

#[test]
fn auth_view_scroll_includes_every_related_route() {
    let spec = serde_json::json!({
        "openapi": "3.1.0",
        "info": {"title": "Auth routes", "version": "1"},
        "paths": {
            "/template": {
                "get": {"responses": {"200": {"description": "ok"}}}
            }
        }
    });
    let mut model = parse_openapi_model(ApiSpecId(90), &spec).expect("parse");
    let template = model.routes.first().cloned().expect("sample route");
    model.routes = (0..13)
        .map(|idx| {
            let mut route = template.clone();
            route.path = format!("/login/{idx}");
            route
        })
        .collect();
    model.rebuild_route_layout_cache();
    let state = ApiClientTabState {
        auth_view: true,
        ..Default::default()
    };

    assert_eq!(api_auth_related_route_count(&model), 13);
    assert_eq!(api_tab_max_scroll(Some(&model), &state, None, 0.0, 1.0), 644.0);
}

#[test]
fn manual_request_url_uses_entered_path_values() {
    let route = crate::app::api_mock::types::ApiManualRoute {
        stable_id: "manual-user".to_string(),
        method: ApiMethod::Get,
        path: "/users/{id}".to_string(),
        enabled: true,
        response: crate::app::api_mock::types::ApiMockResponse::Generated,
        python: None,
        input_fields: Vec::new(),
        output_fields: Vec::new(),
    };
    let server = ApiServer {
        url: "http://127.0.0.1:4010".to_string(),
        description: String::new(),
        variables: Vec::new(),
    };
    let path_values = vec![ApiInputValue {
        name: "id".to_string(),
        value: "42".to_string(),
    }];

    let url = build_manual_api_request_url(&server, &route, &path_values, &[])
        .expect("manual URL");

    assert_eq!(url, "http://127.0.0.1:4010/users/42");
}

#[test]
fn request_url_without_query_values_has_no_trailing_question_mark() {
    let server = ApiServer {
        url: "https://api.example.test".to_string(),
        description: String::new(),
        variables: Vec::new(),
    };

    let url = build_request_url(&server, "/health", &[], &[]).expect("URL");

    assert_eq!(url, "https://api.example.test/health");
}

#[test]
fn operation_parameters_override_path_item_parameters() {
    let spec = serde_json::json!({
        "openapi": "3.1.0",
        "info": {"title": "Override", "version": "1"},
        "paths": {
            "/users/{id}": {
                "parameters": [{
                    "name": "id", "in": "path", "required": true,
                    "description": "path item", "schema": {"type": "string"}
                }],
                "get": {
                    "parameters": [{
                        "name": "id", "in": "path", "required": true,
                        "description": "operation", "schema": {"type": "integer"}
                    }],
                    "responses": {"200": {"description": "ok"}}
                }
            }
        }
    });

    let model = parse_openapi_model(ApiSpecId(91), &spec).expect("parse");
    let params = &model.routes[0].path_params;

    assert_eq!(params.len(), 1);
    assert_eq!(params[0].description, "operation");
    assert_eq!(params[0].primitive_type, ApiPrimitiveType::Integer);
}

#[test]
fn explicit_parameter_example_keeps_precedence_over_schema_examples() {
    let spec = serde_json::json!({
        "openapi": "3.1.0",
        "info": {"title": "Examples", "version": "1"},
        "paths": {
            "/search": {
                "get": {
                    "parameters": [{
                        "name": "q", "in": "query", "example": "z-explicit",
                        "schema": {"type": "string", "example": "a-schema"}
                    }],
                    "responses": {"200": {"description": "ok"}}
                }
            }
        }
    });

    let model = parse_openapi_model(ApiSpecId(92), &spec).expect("parse");
    let param = &model.routes[0].query_params[0];

    assert_eq!(param.example.as_deref(), Some("z-explicit"));
    assert_eq!(param.examples, vec!["z-explicit", "a-schema"]);
}

#[test]
fn python_version_scroll_uses_actual_compressed_viewport() {
    let full = api_python_version_list_max_scroll(10, 158.0, 1.0);
    let compressed = api_python_version_list_max_scroll(10, 46.0, 1.0);

    assert_eq!(full, 130.0);
    assert_eq!(compressed, 242.0);
    assert!(compressed > full);
}

#[test]
fn python_scrollbar_geometry_handles_tiny_viewports_without_invalid_bounds() {
    assert!(api_python_scrollbar((0.0, 0.0, 100.0, 8.0), 0.0, 100.0, 1.0)
        .geometry(1.0)
        .is_none());
    let geometry = api_python_scrollbar((0.0, 0.0, 100.0, 100.0), 0.0, 200.0, 1.0)
        .geometry(1.0)
        .expect("scrollbar");
    assert_eq!(geometry.track_len, 88.0);
    assert!(geometry.thumb.len >= 18.0 && geometry.thumb.len <= geometry.track_len);
}

#[test]
fn ty_worker_disconnect_does_not_create_a_synthetic_route_failure() {
    assert!(api_mock_ty_disconnect_status(None).is_none());
    let status = api_mock_ty_disconnect_status(Some((7, 11))).expect("current check");
    assert!(matches!(
        status,
        crate::app::api_mock::types::ApiMockCheckStatus::Failed {
            route_idx: 7,
            version: 11,
            ..
        }
    ));
}
