#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn persist_test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

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
        let a = crate::app::App::api_mock_virtual_path_for(ApiSpecId(1), 0);
        let b = crate::app::App::api_mock_virtual_path_for(ApiSpecId(2), 0);
        let c = crate::app::App::api_mock_virtual_path_for(ApiSpecId(1), 1);

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
    fn api_text_area_top_matches_render_baseline_offset() {
        assert_eq!(api_text_area_baseline_offset(1.0), 20.0);
        assert_eq!(api_text_area_top_from_baseline(29.0, 1.0), 9.0);
    }

    #[test]
    fn api_mock_autocomplete_anchor_uses_cursor_baseline_and_scroll_x() {
        let rect = (100.0, 200.0, 300.0, 120.0);
        let text = "seed\n    Response";
        let (x, y) = crate::app::App::api_mock_autocomplete_anchor_for_text(
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
        let (x, y) = crate::app::App::api_mock_autocomplete_anchor_for_text(
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

    #[test]
    fn url_validation_rejects_bad_parts() {
        assert!(validate_api_url("https://example.com/openapi.json").is_ok());
        assert_eq!(
            validate_api_url("ftp://example.com/openapi.json")
                .unwrap_err()
                .kind,
            ApiLoadErrorKind::InvalidUrl
        );
        assert_eq!(
            validate_api_url("http://[:::1]").unwrap_err().kind,
            ApiLoadErrorKind::InvalidUrl
        );
        assert_eq!(
            validate_api_url("https://-bad.example/openapi.json")
                .unwrap_err()
                .kind,
            ApiLoadErrorKind::InvalidDomain
        );
        assert_eq!(
            validate_api_url("https://api.example.com/docs#post-/items")
                .unwrap_err()
                .kind,
            ApiLoadErrorKind::InvalidUrl
        );
    }

    #[test]
    fn parse_openapi_extracts_compact_routes_servers_and_schema() {
        let model = parse_openapi_model(ApiSpecId(7), &sample_spec()).expect("parse");
        assert_eq!(model.title, "Demo API");
        assert_eq!(model.version, "1.2.3");
        assert_eq!(model.openapi_version, "3.1.0");
        assert_eq!(model.servers.len(), 1);
        assert_eq!(model.routes.len(), 2);
        assert_eq!(model.routes[0].tag, "pets");
        assert_eq!(model.routes[0].method, ApiMethod::Get);
        assert_eq!(model.routes[1].method, ApiMethod::Post);
        assert_eq!(model.routes[0].path_params[0].name, "id");
        assert_eq!(model.routes[0].query_params[0].name, "verbose");
        assert!(!model.schema_arena.is_empty());
    }

    #[test]
    fn parse_openapi_parameter_array_item_ref_keeps_enum() {
        let spec = serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "Bookings", "version": "1.0.0"},
            "components": {
                "schemas": {
                    "StateEnum": {
                        "type": "string",
                        "enum": ["CREATED", "ACCEPTED"],
                        "default": "CREATED"
                    }
                }
            },
            "paths": {
                "/car_washes/bookings": {
                    "get": {
                        "parameters": [
                            {
                                "name": "state_in",
                                "in": "query",
                                "schema": {
                                    "type": "array",
                                    "items": {"$ref": "#/components/schemas/StateEnum"}
                                }
                            }
                        ],
                        "responses": {"200": {"description": "ok"}}
                    }
                }
            }
        });
        let model = parse_openapi_model(ApiSpecId(12), &spec).expect("parse");
        let param = &model.routes[0].query_params[0];
        assert_eq!(param.name, "state_in");
        assert_eq!(param.primitive_type, ApiPrimitiveType::Array);
        assert_eq!(param.item_type, Some(ApiPrimitiveType::String));
        assert_eq!(param.default_value.as_deref(), Some("CREATED"));
        assert_eq!(param.enum_values, vec!["CREATED", "ACCEPTED"]);
    }

    #[test]
    fn parse_openapi_date_datetime_time_and_bytes_types_keep_examples() {
        let spec = serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "Dates", "version": "1.0.0"},
            "paths": {
                "/events": {
                    "get": {
                        "parameters": [
                            {
                                "name": "day",
                                "in": "query",
                                "schema": {"type": "string", "format": "date", "example": "2026-05-25"}
                            },
                            {
                                "name": "at",
                                "in": "query",
                                "schema": {"type": "string", "format": "time", "example": "12:30:00"}
                            }
                        ],
                        "responses": {"200": {"description": "ok"}}
                    },
                    "post": {
                        "requestBody": {
                            "content": {
                                "application/x-www-form-urlencoded": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "starts_at": {
                                                "type": "string",
                                                "format": "date-time",
                                                "examples": ["2026-05-25T12:30:00Z"]
                                            },
                                            "opens_at": {
                                                "type": "string",
                                                "format": "time"
                                            },
                                            "avatar": {
                                                "type": "string",
                                                "format": "binary"
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        "responses": {"200": {"description": "ok"}}
                    }
                }
            }
        });
        let model = parse_openapi_model(ApiSpecId(30), &spec).expect("parse");
        let param = model.routes[0]
            .query_params
            .iter()
            .find(|param| param.name == "day")
            .expect("day param");
        assert_eq!(param.primitive_type, ApiPrimitiveType::Date);
        assert_eq!(param.examples, vec!["2026-05-25"]);
        let time_param = model.routes[0]
            .query_params
            .iter()
            .find(|param| param.name == "at")
            .expect("at param");
        assert_eq!(time_param.primitive_type, ApiPrimitiveType::Time);
        assert_eq!(time_param.examples, vec!["12:30:00"]);

        let body = model.routes[1].request_body.as_ref().expect("body");
        let root = body.schema.expect("schema");
        let root_schema = &model.schema_arena[root.0];
        let prop = root_schema
            .properties
            .iter()
            .find(|prop| prop.name == "starts_at")
            .expect("starts_at")
            .schema;
        let schema = &model.schema_arena[prop.0];
        assert_eq!(schema.kind, ApiSchemaKind::DateTime);
        assert_eq!(schema.examples, vec!["2026-05-25T12:30:00Z"]);
        let opens_at = root_schema
            .properties
            .iter()
            .find(|prop| prop.name == "opens_at")
            .expect("opens_at")
            .schema;
        assert_eq!(model.schema_arena[opens_at.0].kind, ApiSchemaKind::Time);
        let avatar = root_schema
            .properties
            .iter()
            .find(|prop| prop.name == "avatar")
            .expect("avatar")
            .schema;
        assert_eq!(model.schema_arena[avatar.0].kind, ApiSchemaKind::Bytes);
        let generated = schema_example_json(root, &model, 0);
        assert!(generated.contains("\"starts_at\": \"2026-05-25T12:30:00Z\""));
        assert!(generated.contains("\"opens_at\": \"12:00:00\""));
        assert!(generated.contains("\"avatar\": \"🖼\""));
    }

    #[test]
    fn parse_openapi_security_schemes_and_operation_security() {
        let model = parse_openapi_model(ApiSpecId(11), &auth_spec()).expect("parse");
        assert_eq!(model.security_schemes.len(), 8);
        assert_eq!(model.root_security.len(), 2);
        let names = model
            .security_schemes
            .iter()
            .map(|scheme| scheme.name.as_str())
            .collect::<Vec<_>>();
        assert!(names.contains(&"HeaderKey"));
        assert!(names.contains(&"QueryKey"));
        assert!(names.contains(&"CookieKey"));
        assert!(names.contains(&"BasicAuth"));
        assert!(names.contains(&"BearerJwt"));
        assert!(names.contains(&"DigestAuth"));
        assert!(names.contains(&"OAuthAll"));
        assert!(names.contains(&"Oidc"));
        assert!(model.security_schemes.iter().any(|scheme| matches!(
            scheme.kind,
            ApiSecuritySchemeKind::Http { ref scheme, ref bearer_format }
                if scheme == "bearer" && bearer_format == "JWT"
        )));
        assert!(model.security_schemes.iter().any(|scheme| matches!(
            scheme.kind,
            ApiSecuritySchemeKind::OAuth2 { ref flows }
                if flows == &vec![
                    ApiOAuthFlow::Implicit,
                    ApiOAuthFlow::Password,
                    ApiOAuthFlow::ClientCredentials,
                    ApiOAuthFlow::AuthorizationCode,
                ]
        )));
        let public = model
            .routes
            .iter()
            .find(|route| route.path == "/public")
            .expect("public route");
        assert_eq!(public.security, Some(Vec::new()));
    }

    #[test]
    fn auth_selection_respects_or_and_and_security_empty() {
        let model = parse_openapi_model(ApiSpecId(12), &auth_spec()).expect("parse");
        let items = model
            .routes
            .iter()
            .find(|route| route.path == "/items")
            .expect("items route");
        let public = model
            .routes
            .iter()
            .find(|route| route.path == "/public")
            .expect("public route");
        let mut auth = ApiAuthStore::default();
        assert_eq!(
            api_route_auth_scheme_indices(&model, items)
                .iter()
                .filter_map(|idx| model.security_schemes.get(*idx))
                .map(|scheme| scheme.name.as_str())
                .collect::<Vec<_>>(),
            vec!["BearerJwt", "HeaderKey", "QueryKey"]
        );
        assert!(api_route_auth_scheme_indices(&model, public).is_empty());
        assert!(api_route_auth_missing(&model, items, &auth));
        assert!(!api_route_auth_missing(&model, public, &auth));

        auth.entry_mut(model.id, "HeaderKey").value = "header-secret".to_string();
        auth.entry_mut(model.id, "QueryKey").value = "query-secret".to_string();
        assert!(!api_route_auth_missing(&model, items, &auth));

        let parts = prepared_auth_for_route(&model, items, &auth);
        assert_eq!(
            parts,
            vec![ApiPreparedAuthPart::Query {
                name: "api_key".to_string(),
                value: "query-secret".to_string(),
            }]
        );

        auth.entry_mut(model.id, "BearerJwt").access_token = "jwt".to_string();
        let parts = prepared_auth_for_route(&model, items, &auth);
        assert_eq!(parts.len(), 2);
        assert!(parts.contains(&ApiPreparedAuthPart::Header {
            name: "X-API-Key".to_string(),
            value: "header-secret".to_string(),
        }));
        assert!(parts.contains(&ApiPreparedAuthPart::Bearer {
            token: "jwt".to_string(),
        }));

        auth.entry_mut(model.id, "BearerJwt").value = "refresh".to_string();
        let parts = prepared_auth_for_route(&model, items, &auth);
        assert!(parts.contains(&ApiPreparedAuthPart::Bearer {
            token: "refresh".to_string(),
        }));

        assert!(prepared_auth_for_route(&model, public, &auth).is_empty());
    }

    #[test]
    fn auth_request_assembly_sets_headers_cookies_query_and_basic() {
        let mut url = "https://api.example.com/items".to_string();
        append_auth_query(
            &mut url,
            &[ApiPreparedAuthPart::Query {
                name: "api_key".to_string(),
                value: "q v".to_string(),
            }],
        );
        assert_eq!(url, "https://api.example.com/items?api_key=q+v");

        let client = reqwest::blocking::Client::new();
        let request = apply_auth_to_builder(
            client.get("https://api.example.com/items"),
            &[
                ApiPreparedAuthPart::Header {
                    name: "X-API-Key".to_string(),
                    value: "secret".to_string(),
                },
                ApiPreparedAuthPart::Cookie {
                    name: "session".to_string(),
                    value: "abc".to_string(),
                },
                ApiPreparedAuthPart::Bearer {
                    token: "jwt".to_string(),
                },
            ],
        )
        .build()
        .expect("request");
        assert_eq!(request.headers()["X-API-Key"], "secret");
        assert_eq!(request.headers()["Cookie"], "session=abc");
        assert_eq!(request.headers()["Authorization"], "Bearer jwt");

        let basic = apply_auth_to_builder(
            client.get("https://api.example.com/basic"),
            &[ApiPreparedAuthPart::Basic {
                username: "user".to_string(),
                password: "pass".to_string(),
            }],
        )
        .build()
        .expect("request");
        assert_eq!(basic.headers()["Authorization"], "Basic dXNlcjpwYXNz");
    }

    #[test]
    fn api_curl_command_includes_auth_and_json_body() {
        let job = ApiJobRequest {
            request_id: 9,
            spec_id: ApiSpecId(120),
            route_idx: 2,
            method: ApiMethod::Post,
            url: "https://api.example.test/pets?debug=true".to_string(),
            mock_target: ApiJobMockTarget::None,
            auth_parts: vec![
                ApiPreparedAuthPart::Header {
                    name: "X-Trace".to_string(),
                    value: "abc".to_string(),
                },
                ApiPreparedAuthPart::Bearer {
                    token: "token-1-abcdefghijklmnopqrstuvwxyz0123456789abcdefghijklmnopqrstuvwxyz0123456789".to_string(),
                },
                ApiPreparedAuthPart::Cookie {
                    name: "session".to_string(),
                    value: "cookie-1".to_string(),
                },
            ],
            body_json: Some(r#"{"name":"O'Reilly"}"#.to_string()),
            body_form: None,
            body_multipart: None,
            resolved_host: None,
        };

        let curl = format_api_curl_command(&job);

        assert!(curl.contains("curl \\\n  -X POST"));
        assert!(curl.contains("  'https://api.example.test/pets?debug=true'"));
        assert!(curl.contains("-H 'accept: application/json'"));
        assert!(curl.contains("-H 'X-Trace: abc'"));
        assert!(curl.contains("-H 'Authorization: Bearer token-1-"));
        assert!(curl.contains("'\\\n'"));
        assert!(curl.contains("-H 'Cookie: session=cookie-1'"));
        assert!(curl.contains("-H 'Content-Type: application/json'"));
        assert!(curl.contains(r#"--data-binary '{"name":"O'\''Reilly"}'"#));
    }

    #[test]
    fn auth_capture_saves_tokens_refresh_and_cookie_keys() {
        let model = parse_openapi_model(ApiSpecId(13), &auth_spec()).expect("parse");
        let mut auth = ApiAuthStore::default();
        let response = ApiJobResponse {
            request_id: 1,
            spec_id: model.id,
            route_idx: 0,
            status: Some(200),
            elapsed_ms: 1,
            server_reach_ms: None,
            timing_text: String::new(),
            headers: vec![(
                "set-cookie".to_string(),
                "session=cookie-secret; HttpOnly; Path=/".to_string(),
            )],
            headers_text: String::new(),
            curl_text: String::new(),
            body: serde_json::json!({
                "access_token": "access",
                "refresh_token": "refresh",
                "token_type": "Bearer",
                "expires_in": 60,
                "scope": "read write"
            })
            .to_string(),
            truncated: false,
            error: None,
            resolved_host: None,
        };

        assert!(capture_response_auth(
            &mut auth,
            model.id,
            &model.security_schemes,
            &response
        ));
        let bearer = auth.entry(model.id, "BearerJwt").expect("bearer auth");
        assert_eq!(bearer.access_token, "access");
        assert_eq!(bearer.refresh_token, "refresh");
        assert_eq!(bearer.value, "access");
        assert_eq!(bearer.scopes, vec!["read".to_string(), "write".to_string()]);
        assert!(bearer.expires_at.is_some());
        assert_eq!(
            auth.entry(model.id, "CookieKey")
                .expect("cookie auth")
                .value,
            "cookie-secret"
        );
    }

    #[test]
    fn api_auth_persist_roundtrip_uses_separate_file() {
        let _guard = persist_test_lock().lock().expect("lock");
        let _ = std::fs::remove_dir_all(api_config_dir());

        let mut auth = ApiAuthStore::default();
        auth.entry_mut(ApiSpecId(7), "BearerJwt").access_token = "access".to_string();
        auth.entry_mut(ApiSpecId(7), "BearerJwt").refresh_token = "refresh".to_string();
        auth.entry_mut(ApiSpecId(7), "BasicAuth").username = "user".to_string();
        auth.entry_mut(ApiSpecId(7), "BasicAuth").password = "pass".to_string();
        save_api_auth(&auth);

        let loaded = load_api_auth();
        assert_eq!(
            loaded
                .entry(ApiSpecId(7), "BearerJwt")
                .map(|entry| (entry.access_token.as_str(), entry.refresh_token.as_str())),
            Some(("access", "refresh"))
        );
        assert_eq!(
            loaded
                .entry(ApiSpecId(7), "BasicAuth")
                .map(|entry| (entry.username.as_str(), entry.password.as_str())),
            Some(("user", "pass"))
        );

        let _ = std::fs::remove_dir_all(api_config_dir());
    }

    #[test]
    fn api_method_display_and_sort_order_match_client_rows() {
        assert_eq!(ApiMethod::Get.chip_str(), "GET");
        assert_eq!(ApiMethod::Post.chip_str(), "POS");
        assert_eq!(ApiMethod::Patch.chip_str(), "PAT");
        assert_eq!(ApiMethod::Put.chip_str(), "PUT");
        assert_eq!(ApiMethod::Delete.chip_str(), "DEL");
        assert_eq!(ApiMethod::Head.chip_str(), "HEA");
        assert_eq!(ApiMethod::Options.chip_str(), "OPT");
        assert_eq!(ApiMethod::Trace.chip_str(), "TRA");

        let mut methods = [
            ApiMethod::Trace,
            ApiMethod::Put,
            ApiMethod::Get,
            ApiMethod::Delete,
            ApiMethod::Patch,
            ApiMethod::Options,
            ApiMethod::Post,
            ApiMethod::Head,
        ];
        methods.sort_unstable_by_key(|method| (*method).sort_rank());
        assert_eq!(
            methods,
            [
                ApiMethod::Get,
                ApiMethod::Post,
                ApiMethod::Patch,
                ApiMethod::Put,
                ApiMethod::Delete,
                ApiMethod::Head,
                ApiMethod::Options,
                ApiMethod::Trace,
            ]
        );
    }

    #[test]
    fn api_path_display_spaces_path_params_without_changing_path() {
        assert_eq!(
            format_api_path_display("/sites/{id}/complete"),
            "/sites/ {id} /complete"
        );
        assert_eq!(
            format_api_path_display("/orgs/{org_id}/sites/{site_id}"),
            "/orgs/ {org_id} /sites/ {site_id}"
        );
    }

    #[test]
    fn api_path_display_append_keeps_existing_prefix() {
        let mut out = String::from("GET ");
        append_api_path_display("/sites/{id}/complete", &mut out);
        assert_eq!(out, "GET /sites/ {id} /complete");
    }

    #[test]
    fn api_path_display_writer_clears_existing_buffer() {
        let mut out = String::from("stale");
        write_api_path_display("/sites/{id}/complete", &mut out);
        assert_eq!(out, "/sites/ {id} /complete");
    }

    #[test]
    fn route_grouping_uses_sorted_tag_ranges() {
        let model = parse_openapi_model(ApiSpecId(1), &sample_spec()).expect("parse");
        let groups: Vec<_> = model
            .route_groups
            .iter()
            .map(|group| (model.routes[group.start].tag.as_str(), group.start, group.len))
            .collect();
        assert_eq!(groups, vec![("pets", 0, 2)]);
        assert_eq!(model.route_display_paths.len(), model.routes.len());
        for (route, display_path) in model.routes.iter().zip(model.route_display_paths.iter()) {
            assert_eq!(display_path, &format_api_path_display(&route.path));
        }
    }

    #[test]
    fn route_filter_matches_route_metadata_case_insensitively() {
        let model = parse_openapi_model(ApiSpecId(1), &sample_spec()).expect("parse");
        let route = &model.routes[0];
        let display_path = &model.route_display_paths[0];

        assert!(api_route_matches_filter(route, display_path, "PETS"));
        assert!(api_route_matches_filter(route, display_path, "get"));
        assert!(api_route_matches_filter(route, display_path, "{id}"));
        assert!(!api_route_matches_filter(route, display_path, "missing-route"));
    }

    #[test]
    fn json_validator_catches_trailing_comma() {
        assert!(json_body_is_valid(r#"{"a": 1}"#));
        assert!(!json_body_is_valid(r#"{"a": 1,}"#));
    }

    #[test]
    fn request_url_builder_applies_server_vars_path_and_query() {
        let server = ApiServer {
            url: "https://api.example.com/{version}".to_string(),
            description: String::new(),
            variables: vec![ApiServerVariable {
                name: "version".to_string(),
                default_value: "v1".to_string(),
            }],
        };
        let url = build_request_url(
            &server,
            "/pets/{id}",
            &[ApiInputValue {
                name: "id".to_string(),
                value: "a b".to_string(),
            }],
            &[ApiInputValue {
                name: "verbose".to_string(),
                value: "true".to_string(),
            }],
        )
        .expect("url");
        assert_eq!(url, "https://api.example.com/v1/pets/a%20b?verbose=true");
    }

    #[test]
    fn form_urlencoded_body_prefers_fields_over_json() {
        let model = parse_openapi_model(ApiSpecId(21), &form_spec()).expect("parse");
        let route = &model.routes[0];
        let body = route.request_body.as_ref().expect("body");
        assert_eq!(body.content_type, "application/x-www-form-urlencoded");
        assert!(body.is_form_urlencoded);
        assert!(!body.is_multipart);

        let mut state = ApiClientTabState::default();
        fill_api_tab_inputs(&mut state, route, &model);
        assert_eq!(state.body_json, "");
        assert_eq!(
            state.body_values,
            vec![
                ApiInputValue {
                    name: "username".to_string(),
                    value: String::new(),
                },
                ApiInputValue {
                    name: "password".to_string(),
                    value: String::new(),
                },
            ]
        );

        let fields = [
            ApiInputValue {
                name: "username".to_string(),
                value: "alice".to_string(),
            },
            ApiInputValue {
                name: "password".to_string(),
                value: String::new(),
            },
        ];
        let pairs = api_form_pairs(&fields);
        assert_eq!(pairs, vec![("username", "alice")]);
    }

    #[test]
    fn json_body_uses_first_schema_example() {
        let spec = serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "Body Example", "version": "1.0.0"},
            "paths": {
                "/users": {
                    "post": {
                        "requestBody": {
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "examples": [
                                            {"name": "Ada", "age": 37},
                                            {"name": "Grace", "age": 85}
                                        ],
                                        "properties": {
                                            "name": {"type": "string"},
                                            "age": {"type": "integer"}
                                        }
                                    }
                                }
                            }
                        },
                        "responses": {"200": {"description": "ok"}}
                    }
                }
            }
        });
        let model = parse_openapi_model(ApiSpecId(44), &spec).expect("parse");
        let route = &model.routes[0];
        assert_eq!(
            default_body_for_route(route, &model),
            "{\"name\":\"Ada\",\"age\":37}"
        );
    }

    #[test]
    fn form_urlencoded_ref_body_uses_schema_property_order() {
        let spec = serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "Auth", "version": "1.0.0"},
            "components": {
                "schemas": {
                    "Login": {
                        "type": "object",
                        "required": ["username", "password"],
                        "properties": {
                            "password": {"type": "string"},
                            "username": {"type": "string"}
                        }
                    }
                }
            },
            "paths": {
                "/jwt/login": {
                    "post": {
                        "requestBody": {
                            "content": {
                                "application/x-www-form-urlencoded": {
                                    "schema": {"$ref": "#/components/schemas/Login"}
                                }
                            }
                        },
                        "responses": {"200": {"description": "ok"}}
                    }
                }
            }
        });
        let model = parse_openapi_model(ApiSpecId(22), &spec).expect("parse");
        let mut state = ApiClientTabState::default();
        fill_api_tab_inputs(&mut state, &model.routes[0], &model);

        assert_eq!(state.body_values[0].name, "password");
        assert_eq!(state.body_values[1].name, "username");
    }

    #[test]
    fn openapi_schema_refs_are_reused_for_large_specs_and_request_body_refs() {
        let mut paths = serde_json::Map::new();
        for idx in 0..(API_SCHEMA_MAX_COUNT + 25) {
            paths.insert(
                format!("/bulk/{idx:04}"),
                serde_json::json!({
                    "post": {
                        "requestBody": {"$ref": "#/components/requestBodies/SharedBody"},
                        "responses": {
                            "200": {"$ref": "#/components/responses/Ok"}
                        }
                    }
                }),
            );
        }
        let spec = serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "Large", "version": "1.0.0"},
            "components": {
                "schemas": {
                    "BaseBody": {
                        "type": "object",
                        "required": ["id"],
                        "properties": {
                            "id": {"type": "string"}
                        }
                    },
                    "HugeBody": {
                        "allOf": [
                            {"$ref": "#/components/schemas/BaseBody"},
                            {
                                "type": "object",
                                "required": ["payload"],
                                "properties": {
                                    "payload": {
                                        "type": "object",
                                        "properties": {
                                            "name": {"type": "string", "minLength": 2}
                                        }
                                    }
                                }
                            }
                        ]
                    }
                },
                "requestBodies": {
                    "SharedBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {"$ref": "#/components/schemas/HugeBody"}
                            }
                        }
                    }
                },
                "responses": {
                    "Ok": {
                        "description": "ok",
                        "content": {
                            "application/json": {
                                "schema": {"$ref": "#/components/schemas/HugeBody"}
                            }
                        }
                    }
                }
            },
            "paths": paths
        });
        let model = parse_openapi_model(ApiSpecId(31), &spec).expect("parse");
        let last_route = model
            .routes
            .iter()
            .find(|route| route.path == "/bulk/0792")
            .expect("last route");

        assert!(
            last_route
                .request_body
                .as_ref()
                .and_then(|body| body.schema)
                .is_some()
        );
        assert!(last_route.responses[0].schema.is_some());
        assert!(model.schema_arena.len() < 16);

        let schema_text = api_route_input_schema_text(last_route, &model, 0, &FxHashSet::default());
        assert!(!schema_text.contains("\"body\"*"));
        assert!(schema_text.contains("\"id\"*"));
        assert!(schema_text.contains("\"payload\"*"));
        assert!(schema_text.contains("minLength=2"));
    }

    #[test]
    fn late_response_schema_with_nested_ref_is_not_dropped_in_large_spec() {
        let mut paths = serde_json::Map::new();
        for idx in 0..400 {
            paths.insert(
                format!("/before/{idx:04}"),
                serde_json::json!({
                    "get": {
                        "responses": {
                            "200": {
                                "description": "ok",
                                "content": {
                                    "application/json": {
                                        "schema": {
                                            "type": "object",
                                            "properties": {
                                                "value": {"type": "string"}
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }),
            );
        }
        paths.insert(
            "/cars/show_car_models_by_bt".to_string(),
            serde_json::json!({
                "get": {
                    "responses": {
                        "200": {
                            "description": "Request fulfilled, document follows",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/CarModelsResponse"
                                    }
                                }
                            }
                        }
                    }
                }
            }),
        );
        let spec = serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "Cars", "version": "1"},
            "components": {
                "schemas": {
                    "CarModelsResponse": {
                        "type": "object",
                        "required": [
                            "body_type",
                            "body_type_id",
                            "current",
                            "data",
                            "total"
                        ],
                        "properties": {
                            "data": {
                                "type": "array",
                                "items": {"type": "string"}
                            },
                            "total": {"type": "integer"},
                            "current": {"type": "integer"},
                            "previous": {"type": "integer"},
                            "next": {"type": "integer"},
                            "body_type_id": {"type": "integer"},
                            "body_type": {
                                "$ref": "#/components/schemas/BodyTypeReadResponse"
                            }
                        }
                    },
                    "BodyTypeReadResponse": {
                        "type": "object",
                        "required": ["id", "name"],
                        "properties": {
                            "id": {"type": "integer"},
                            "name": {"type": "string"}
                        }
                    }
                }
            },
            "paths": paths
        });
        let model = parse_openapi_model(ApiSpecId(42), &spec).expect("parse");
        let route = model
            .routes
            .iter()
            .find(|route| route.path == "/cars/show_car_models_by_bt")
            .expect("cars route");
        let response = route.responses.first().expect("200 response");
        let media = response.media.first().expect("application/json media");
        assert!(media.schema.is_some(), "late response schema was dropped");

        let schema_text =
            api_route_output_schema_text_for(route, &model, 0, 0, &FxHashSet::default());
        assert!(!schema_text.contains("not described"), "{schema_text}");
        assert!(schema_text.contains("\"body_type\"*"));
        assert!(schema_text.contains("\"id\"*"));
        assert!(schema_text.contains("\"name\"*"));
        assert!(schema_text.contains("\"data\"*"));
    }

    #[test]
    fn nested_schema_ref_chains_are_resolved_in_response_output() {
        let spec = serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "Cars", "version": "1"},
            "components": {
                "schemas": {
                    "CarModelsResponse": {
                        "$ref": "#/components/schemas/CarModelsResponseAlias"
                    },
                    "CarModelsResponseAlias": {
                        "type": "object",
                        "required": ["body_type", "data"],
                        "properties": {
                            "data": {
                                "type": "array",
                                "items": {"type": "string"}
                            },
                            "body_type": {
                                "$ref": "#/components/schemas/BodyTypeAlias"
                            }
                        }
                    },
                    "BodyTypeAlias": {
                        "$ref": "#/components/schemas/BodyTypeReadResponse"
                    },
                    "BodyTypeReadResponse": {
                        "type": "object",
                        "required": ["id", "name"],
                        "properties": {
                            "id": {"type": "integer"},
                            "name": {"type": "string"}
                        }
                    }
                },
                "responses": {
                    "CarModelsOk": {
                        "$ref": "#/components/responses/CarModelsOkAlias"
                    },
                    "CarModelsOkAlias": {
                        "description": "ok",
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/CarModelsResponse"
                                }
                            }
                        }
                    }
                }
            },
            "paths": {
                "/cars/show_car_models_by_bt": {
                    "get": {
                        "responses": {
                            "200": {
                                "$ref": "#/components/responses/CarModelsOk"
                            }
                        }
                    }
                }
            }
        });
        let model = parse_openapi_model(ApiSpecId(41), &spec).expect("parse");
        let route = &model.routes[0];
        let schema_text =
            api_route_output_schema_text_for(route, &model, 0, 0, &FxHashSet::default());

        assert!(!schema_text.contains("not described"), "{schema_text}");
        assert!(schema_text.contains("\"body_type\"*"));
        assert!(schema_text.contains("\"id\"*"));
        assert!(schema_text.contains("\"name\"*"));
        assert!(schema_text.contains("\"data\"*"));
    }

    #[test]
    fn primitive_arrays_render_inline_without_items_row() {
        let spec = serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "Demo", "version": "1"},
            "paths": {
                "/ids": {
                    "post": {
                        "requestBody": {
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "addition_ids": {
                                                "type": "array",
                                                "items": {"type": "integer"}
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        "responses": {"200": {"description": "ok"}}
                    }
                }
            }
        });
        let model = parse_openapi_model(ApiSpecId(32), &spec).expect("parse");
        let route = &model.routes[0];
        let schema_text = api_route_input_schema_text(route, &model, 0, &FxHashSet::default());

        assert!(schema_text.contains("\"addition_ids\": [],  · array<integer>"));
        assert!(!schema_text.contains("\"items\""));
    }

    #[test]
    fn input_schema_lists_path_and_query_params_without_body_schema_warning() {
        let spec = serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "Demo", "version": "1"},
            "paths": {
                "/users/{user_id}": {
                    "get": {
                        "parameters": [
                            {
                                "name": "user_id",
                                "in": "path",
                                "required": true,
                                "schema": {"type": "integer"}
                            },
                            {
                                "name": "include",
                                "in": "query",
                                "schema": {"type": "array", "items": {"type": "string"}}
                            }
                        ],
                        "responses": {"200": {"description": "ok"}}
                    }
                }
            }
        });
        let model = parse_openapi_model(ApiSpecId(33), &spec).expect("parse");
        let route = &model.routes[0];
        let schema_text = api_route_input_schema_text(route, &model, 0, &FxHashSet::default());

        assert!(schema_text.contains("\"path\": {"));
        assert!(schema_text.contains("\"user_id\"*: 0  · integer"));
        assert!(schema_text.contains("\"query\": {"));
        assert!(schema_text.contains("\"include\": []  · array"));
        assert!(!schema_text.contains("Input body schema not described"));
    }

    #[test]
    fn output_example_omits_response_header_and_reports_missing_schema_per_status() {
        let spec = serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "Demo", "version": "1"},
            "components": {
                "schemas": {
                    "RefreshResponse": {
                        "type": "object",
                        "properties": {"token": {"type": "string"}}
                    }
                }
            },
            "paths": {
                "/login": {
                    "post": {
                        "responses": {
                            "200": {
                                "description": "ok",
                                "content": {
                                    "application/json": {
                                        "schema": {"$ref": "#/components/schemas/RefreshResponse"}
                                    }
                                }
                            },
                            "400": {"description": "bad request"},
                            "422": {
                                "description": "validation",
                                "content": {
                                    "application/json": {
                                        "examples": {
                                            "invalid_email": {
                                                "summary": "InvalidEmail",
                                                "value": {"error": "invalid email"}
                                            },
                                            "weak_password": {
                                                "summary": "WeakPassword",
                                                "value": {"error": "weak password"}
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
        let model = parse_openapi_model(ApiSpecId(34), &spec).expect("parse");
        let route = &model.routes[0];
        let ok_example = api_route_output_example_text_for(route, &model, 0, 0);
        let ok_schema =
            api_route_output_schema_text_for(route, &model, 0, 0, &FxHashSet::default());
        let bad_example = api_route_output_example_text_for(route, &model, 1, 0);
        let bad_schema =
            api_route_output_schema_text_for(route, &model, 1, 0, &FxHashSet::default());
        let validation_idx = route
            .responses
            .iter()
            .position(|response| response.status == "422")
            .expect("422 response");

        assert!(!ok_example.starts_with("Response 200"));
        assert!(ok_example.contains("\"token\""));
        assert!(ok_example.contains("{\n  \"token\""));
        assert!(ok_schema.contains("\"token\""));
        assert!(!ok_schema.contains("name=RefreshResponse"));
        assert_eq!(bad_example, "schema/example not described\n");
        assert_eq!(bad_schema, "null  · not described\n");
        assert_eq!(api_route_output_example_count(route, validation_idx), 2);
        assert_eq!(
            api_route_output_example_label(route, validation_idx, 0),
            "InvalidEmail"
        );
        assert_eq!(
            api_route_output_example_label(route, validation_idx, 1),
            "WeakPassword"
        );
        assert!(
            api_route_output_example_text_for(route, &model, validation_idx, 1)
                .contains("weak password")
        );
        let (status, content_type, generated) = api_generated_response_for_route(route, &model);
        assert_eq!(status, 200);
        assert_eq!(content_type, "application/json");
        assert!(generated.contains("\"token\""));
        let bad_route = ApiRouteRow {
            responses: vec![route.responses[1].clone()],
            ..route.clone()
        };
        let (status, content_type, generated) =
            api_generated_response_for_route(&bad_route, &model);
        assert_eq!(status, 400);
        assert_eq!(content_type, "text/plain; charset=utf-8");
        assert_eq!(
            generated,
            "Response 400 schema/example not described in OpenAPI."
        );
    }

    #[test]
    fn form_and_multipart_field_rows_stay_compact() {
        let model = parse_openapi_model(ApiSpecId(24), &form_spec()).expect("parse");
        let route = &model.routes[0];
        let schema = route
            .request_body
            .as_ref()
            .and_then(|body| body.schema)
            .and_then(|schema_ref| model.schema_arena.get(schema_ref.0))
            .expect("schema");
        let username = schema
            .properties
            .iter()
            .find(|prop| prop.name == "username")
            .and_then(|prop| model.schema_arena.get(prop.schema.0))
            .expect("username");
        assert_eq!(api_body_prop_row_height(username, &model, 1.0), 46.0);

        let spec = serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "Upload", "version": "1.0.0"},
            "paths": {
                "/upload": {
                    "post": {
                        "requestBody": {
                            "content": {
                                "multipart/form-data": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "kind": {
                                                "type": "string",
                                                "enum": ["avatar", "cover", "doc"]
                                            },
                                            "file": {
                                                "type": "string",
                                                "format": "binary"
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        "responses": {"200": {"description": "ok"}}
                    }
                }
            }
        });
        let model = parse_openapi_model(ApiSpecId(25), &spec).expect("parse");
        let route = &model.routes[0];
        let schema = route
            .request_body
            .as_ref()
            .and_then(|body| body.schema)
            .and_then(|schema_ref| model.schema_arena.get(schema_ref.0))
            .expect("schema");
        for prop in &schema.properties {
            let prop_schema = model.schema_arena.get(prop.schema.0).expect("prop");
            let expected = 46.0;
            assert_eq!(api_body_prop_row_height(prop_schema, &model, 1.0), expected);
        }
    }

    #[test]
    fn auth_view_focus_uses_single_token_field_and_routes_include_refresh_flow() {
        let model = parse_openapi_model(ApiSpecId(26), &auth_spec()).expect("parse");
        let state = ApiClientTabState {
            auth_view: true,
            ..Default::default()
        };
        let order = api_focus_order_for_view(model.id, &model, &state);
        assert!(order.contains(&ApiFocus::AuthValue {
            spec_id: model.id,
            scheme: "BearerJwt".to_string(),
        }));
        assert!(!order.contains(&ApiFocus::AuthRefreshToken {
            spec_id: model.id,
            scheme: "BearerJwt".to_string(),
        }));

        let spec = serde_json::json!({
            "openapi": "3.1.0",
            "info": {"title": "JWT", "version": "1.0.0"},
            "paths": {
                "/jwt/login": {"post": {"responses": {"200": {"description": "ok"}}}},
                "/jwt/refresh": {"post": {"responses": {"200": {"description": "ok"}}}},
                "/users": {"get": {"responses": {"200": {"description": "ok"}}}}
            }
        });
        let model = parse_openapi_model(ApiSpecId(27), &spec).expect("parse");
        assert_eq!(api_auth_related_route_count(&model), 2);
        assert_eq!(api_auth_route_rank(&model.routes[0]), Some(0));
        assert_eq!(api_auth_route_rank(&model.routes[1]), Some(1));
        assert_eq!(api_auth_route_rank(&model.routes[2]), None);
    }

    #[test]
    fn api_response_auth_token_detection_handles_access_or_refresh() {
        let response = ApiJobResponse {
            request_id: 1,
            spec_id: ApiSpecId(28),
            route_idx: 0,
            status: Some(200),
            elapsed_ms: 1,
            server_reach_ms: None,
            timing_text: String::new(),
            headers: Vec::new(),
            headers_text: String::new(),
            curl_text: String::new(),
            body: r#"{"access_token":"a"}"#.to_string(),
            truncated: false,
            resolved_host: None,
            error: None,
        };
        assert!(api_response_has_auth_tokens(&response));

        let response = ApiJobResponse {
            body: r#"{"refresh_token":"r"}"#.to_string(),
            ..response
        };
        assert!(api_response_has_auth_tokens(&response));
    }

    #[test]
    fn api_tab_keeps_response_when_switching_routes() {
        let mut state = ApiClientTabState {
            route_idx: Some(0),
            path_values: vec![ApiInputValue {
                name: "id".to_string(),
                value: "first".to_string(),
            }],
            response: Some(ApiJobResponse {
                request_id: 7,
                spec_id: ApiSpecId(29),
                route_idx: 0,
                status: Some(200),
                elapsed_ms: 3,
                server_reach_ms: None,
                timing_text: "3ms".to_string(),
                headers: Vec::new(),
                headers_text: String::new(),
                curl_text: String::new(),
                body: "{\"ok\":true}".to_string(),
                truncated: false,
                error: None,
                resolved_host: None,
            }),
            ..Default::default()
        };
        state.remember_route_state();
        state.route_idx = Some(1);
        state.path_values.clear();
        state.response = None;

        assert!(state.restore_route_state(0));
        assert_eq!(state.path_values[0].value, "first");
        assert_eq!(
            state
                .response
                .as_ref()
                .map(|response| response.body.as_str()),
            Some("{\"ok\":true}")
        );
    }

    #[test]
    fn api_focus_order_tabs_through_form_fields() {
        let model = parse_openapi_model(ApiSpecId(23), &form_spec()).expect("parse");
        let state = ApiClientTabState {
            route_idx: Some(0),
            ..Default::default()
        };
        let order = api_focus_order_for_view(model.id, &model, &state);

        assert_eq!(
            order,
            vec![
                ApiFocus::BodyField {
                    spec_id: model.id,
                    route_idx: 0,
                    name: "username".to_string(),
                },
                ApiFocus::BodyField {
                    spec_id: model.id,
                    route_idx: 0,
                    name: "password".to_string(),
                },
            ]
        );
    }

    #[test]
    fn parse_openapi_rejects_missing_or_old_version() {
        assert_eq!(
            parse_openapi_model(ApiSpecId(1), &serde_json::json!({}))
                .unwrap_err()
                .kind,
            ApiLoadErrorKind::UnsupportedOpenApi
        );
        assert_eq!(
            parse_openapi_model(
                ApiSpecId(1),
                &serde_json::json!({"openapi": "2.0", "paths": {}})
            )
            .unwrap_err()
            .message,
            "поддерживается OpenAPI 3.x"
        );
    }

    #[test]
    fn last_loaded_text_uses_now_then_minutes_without_seconds() {
        let now = now_epoch_secs();
        assert_eq!(
            format_last_loaded_at(Some(now.saturating_sub(30)), now),
            "только что"
        );
        assert_eq!(
            format_last_loaded_at(Some(now.saturating_sub(60)), now),
            "1 мин назад"
        );
        assert_eq!(format_last_loaded_at(None, now), "не загружено");
        assert!(api_timing_visible_at(Some(now.saturating_sub(9)), now));
        assert!(!api_timing_visible_at(Some(now.saturating_sub(10)), now));
        assert!(!api_timing_visible_at(None, now));
    }

    #[test]
    fn api_state_remove_spec_clears_model_loading_collapsed_and_selection() {
        let first = ApiSpecId(1);
        let second = ApiSpecId(2);
        let mut state = ApiClientState::default();
        state.specs.push(ApiSpecEntry {
            id: first,
            title: "One".to_string(),
            version: String::new(),
            openapi_version: "3.1.0".to_string(),
            source: ApiSpecSource::Url("https://example.com/one.json".to_string()),
            last_loaded: Some(1),
            last_fetch_secs: None,
            last_parse_secs: None,
            last_url_status: None,
            selected: true,
            error: None,
        });
        state.specs.push(ApiSpecEntry {
            id: second,
            title: "Two".to_string(),
            version: String::new(),
            openapi_version: "3.1.0".to_string(),
            source: ApiSpecSource::Url("https://example.com/two.json".to_string()),
            last_loaded: Some(2),
            last_fetch_secs: None,
            last_parse_secs: None,
            last_url_status: None,
            selected: false,
            error: None,
        });
        state.selected_spec = Some(first);
        state.models.insert(first, ApiSpecModel::default());
        state.loading.insert(first);
        state
            .collapsed_tags
            .entry(first)
            .or_default()
            .insert("pets".to_string());

        assert_eq!(state.remove_spec(0), Some(first));
        assert_eq!(state.selected_spec, Some(second));
        assert!(!state.models.contains_key(&first));
        assert!(!state.loading.contains(&first));
        assert!(state.collapsed_tags.is_empty());
        assert!(state.specs[0].selected);
        assert_eq!(state.remove_spec(99), None);
    }

    #[test]
    fn api_specs_persist_roundtrip_keeps_imported_sources_and_selection() {
        let _guard = persist_test_lock().lock().expect("lock");
        let _ = std::fs::remove_dir_all(api_config_dir());

        let mut state = ApiClientState::default();
        state.next_id = 8;
        state.selected_spec = Some(ApiSpecId(7));
        state.specs.push(ApiSpecEntry {
            id: ApiSpecId(7),
            title: "Persisted".to_string(),
            version: "1.0".to_string(),
            openapi_version: "3.1.0".to_string(),
            source: ApiSpecSource::Url("https://example.com/openapi.json".to_string()),
            last_loaded: Some(123),
            last_fetch_secs: Some(0.1234),
            last_parse_secs: Some(0.0456),
            last_url_status: Some(ApiUrlStatus::Ok(200)),
            selected: true,
            error: None,
        });
        save_url_cache(ApiSpecId(7), &sample_spec().to_string());
        state.persist();

        let loaded = ApiClientState::load_persisted();
        assert_eq!(loaded.next_id, 8);
        assert_eq!(loaded.selected_spec, Some(ApiSpecId(7)));
        assert_eq!(loaded.specs.len(), 1);
        assert_eq!(loaded.specs[0].title, "Persisted");
        assert_eq!(
            loaded.specs[0].source,
            ApiSpecSource::Url("https://example.com/openapi.json".to_string())
        );
        assert_eq!(loaded.specs[0].last_loaded, Some(123));
        assert_eq!(loaded.specs[0].last_fetch_secs, Some(0.1234));
        assert_eq!(loaded.specs[0].last_parse_secs, Some(0.0456));
        assert!(loaded.specs[0].selected);
        assert!(loaded.models.contains_key(&ApiSpecId(7)));
        assert!(loaded.loading.is_empty());

        let _ = std::fs::remove_dir_all(api_config_dir());
    }

    #[test]
    fn api_scroll_limits_are_finite_and_shrink_when_routes_collapsed() {
        let mut state = ApiClientState::default();
        let model = parse_openapi_model(ApiSpecId(5), &sample_spec()).expect("parse");
        state.specs.push(ApiSpecEntry {
            id: model.id,
            title: model.title.clone(),
            version: model.version.clone(),
            openapi_version: model.openapi_version.clone(),
            source: ApiSpecSource::Url("https://example.com/openapi.json".to_string()),
            last_loaded: Some(1),
            last_fetch_secs: None,
            last_parse_secs: None,
            last_url_status: Some(ApiUrlStatus::Ok(200)),
            selected: true,
            error: None,
        });
        state.selected_spec = Some(model.id);
        state.models.insert(model.id, model.clone());

        let expanded = api_panel_max_scroll(&state, 120.0, 1.0);
        state.route_filter = "missing-route".to_string();
        let filtered = api_panel_max_scroll(&state, 120.0, 1.0);
        state.route_filter.clear();
        state
            .collapsed_tags
            .entry(model.id)
            .or_default()
            .insert("pets".to_string());
        let collapsed = api_panel_max_scroll(&state, 120.0, 1.0);
        assert!(expanded.is_finite());
        assert!(filtered.is_finite());
        assert!(collapsed.is_finite());
        assert!(filtered < expanded);
        assert!(collapsed < expanded);

        let tab_state = ApiClientTabState {
            route_idx: Some(0),
            response: Some(ApiJobResponse {
                request_id: 0,
                spec_id: model.id,
                route_idx: 0,
                status: Some(200),
                elapsed_ms: 1,
                server_reach_ms: Some(1),
                timing_text: "1 ms (~1 ms до сервера)".to_string(),
                headers: Vec::new(),
                headers_text: String::new(),
                curl_text: String::new(),
                body: "{}".to_string(),
                truncated: false,
                error: None,
                resolved_host: None,
            }),
            ..Default::default()
        };
        let tab_max = api_tab_max_scroll(Some(&model), &tab_state, None, 180.0, 1.0);
        assert!(tab_max.is_finite());
        assert!(tab_max > 0.0);
        assert_eq!(api_tab_max_scroll(None, &tab_state, None, 180.0, 1.0), 0.0);
    }

    #[test]
    fn api_timing_text_never_mixes_mock_and_server_reach_labels() {
        assert_eq!(
            format_api_timing_text(7, Some(3), ApiJobMockTarget::Mock),
            "7 ms (мок-сервер)"
        );
        assert_eq!(
            format_api_timing_text(7, Some(3), ApiJobMockTarget::Proxy),
            "3 ms до сервера"
        );
    }
}
