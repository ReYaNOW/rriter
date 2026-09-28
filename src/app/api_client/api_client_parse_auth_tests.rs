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
        assert_eq!(model.routes[0].summary, "Read pet");
        assert_eq!(
            model.routes[0].description,
            "Returns one pet.\nRequires the pets:read role."
        );
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
    fn auth_store_set_value_creates_a_bearer_entry() {
        let mut auth = ApiAuthStore::default();
        let spec_id = ApiSpecId(42);

        auth.set_value(spec_id, "BearerAuth", "rriter-pgo-token".to_string());

        let entry = auth.entry(spec_id, "BearerAuth").expect("saved auth entry");
        assert_eq!(entry.value, "rriter-pgo-token");
        assert_eq!(entry.token_type, "Bearer");
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
            body_content_type: Some("application/json".to_string()),
            body_json: Some(r#"{"name":"O'Reilly"}"#.to_string()),
            body_form: None,
            body_multipart: None,
            resolved_host: None,
        };

        let curl = format_api_curl_command_for_platform(
            &job,
            crate::platform::PlatformKind::Linux,
        );

        assert!(curl.contains("curl \\\n  -X POST"));
        assert!(curl.contains("  'https://api.example.test/pets?debug=true'"));
        assert!(curl.contains("-H 'accept: application/json'"));
        assert!(curl.contains("-H 'X-Trace: abc'"));
        assert!(curl.contains("-H 'Authorization: Bearer token-1-"));
        assert!(curl.contains("'\\\n'"));
        assert!(curl.contains("-H 'Cookie: session=cookie-1'"));
        assert!(curl.contains("-H 'Content-Type: application/json'"));
        assert!(curl.contains(r#"--data-binary '{"name":"O'\''Reilly"}'"#));

        let windows = format_api_curl_command_for_platform(
            &job,
            crate::platform::PlatformKind::Windows,
        );
        assert!(windows.starts_with("curl.exe `\n  -X POST `\n"));
        assert!(windows.contains("-H 'Authorization: Bearer token-1-"));
        assert!(windows.contains(r#"--data-binary '{"name":"O''Reilly"}'"#));
        assert!(!windows.contains(" \\\n"));
        assert!(!windows.contains(r#"'\''"#));
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
        let _ = std::fs::remove_dir_all(api_config_dir());

        let mut auth = ApiAuthStore::default();
        auth.entry_mut(ApiSpecId(7), "BearerJwt").access_token = "access".to_string();
        auth.entry_mut(ApiSpecId(7), "BearerJwt").refresh_token = "refresh".to_string();
        auth.entry_mut(ApiSpecId(7), "BasicAuth").username = "user".to_string();
        auth.entry_mut(ApiSpecId(7), "BasicAuth").password = "pass".to_string();
        let _ = save_api_auth(&auth);

        let record = std::fs::read(api_auth_path()).expect("auth record");
        let plain = crate::platform::open_user_secret(&record, API_AUTH_SECRET_PURPOSE)
            .expect("open auth record");
        assert!(plain.windows(b"access".len()).any(|part| part == b"access"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(api_auth_path())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }

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
    fn legacy_plaintext_api_auth_is_loaded_and_rewritten_safely() {
        let _ = std::fs::remove_dir_all(api_config_dir());
        let mut auth = ApiAuthStore::default();
        auth.entry_mut(ApiSpecId(8), "BearerJwt").access_token = "legacy".to_string();
        std::fs::create_dir_all(api_config_dir()).unwrap();
        std::fs::write(api_auth_path(), serde_json::to_vec_pretty(&auth).unwrap()).unwrap();

        let loaded = load_api_auth();
        assert_eq!(
            loaded
                .entry(ApiSpecId(8), "BearerJwt")
                .map(|entry| entry.access_token.as_str()),
            Some("legacy")
        );
        let _ = save_api_auth(&loaded);
        let record = std::fs::read(api_auth_path()).unwrap();
        assert_eq!(
            crate::platform::open_user_secret(&record, API_AUTH_SECRET_PURPOSE).unwrap(),
            serde_json::to_vec_pretty(&loaded).unwrap()
        );

        let _ = std::fs::remove_dir_all(api_config_dir());
    }

