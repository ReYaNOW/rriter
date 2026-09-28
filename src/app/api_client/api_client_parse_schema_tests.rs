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

