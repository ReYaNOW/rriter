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
    fn api_route_memories_follow_route_identity_after_model_reorder() {
        let mut old_model = parse_openapi_model(ApiSpecId(29), &form_spec()).expect("parse");
        let mut first = old_model.routes[0].clone();
        first.method = ApiMethod::Get;
        first.path = "/first".to_string();
        let mut second = old_model.routes[0].clone();
        second.method = ApiMethod::Post;
        second.path = "/second".to_string();
        old_model.routes = vec![first, second];
        let previous_routes = old_model
            .routes
            .iter()
            .map(|route| (route.method, route.path.clone()))
            .collect::<Vec<_>>();
        let mut reordered = old_model.clone();
        reordered.routes.reverse();

        let mut state = ApiClientTabState {
            route_idx: Some(0),
            body_json: "first".to_string(),
            ..Default::default()
        };
        state.remember_route_state();
        state.tab_scroll.current = 10.0;
        state.tab_scroll.target = 12.0;
        state.remember_view_scroll();
        state.route_idx = Some(1);
        state.body_json = "second".to_string();
        state.remember_route_state();
        state.tab_scroll.current = 20.0;
        state.tab_scroll.target = 22.0;
        state.remember_view_scroll();

        state.remap_route_memories(&previous_routes, &reordered);

        assert_eq!(
            state
                .route_states
                .iter()
                .find(|saved| saved.body_json == "first")
                .map(|saved| saved.route_idx),
            Some(1)
        );
        assert_eq!(
            state
                .route_states
                .iter()
                .find(|saved| saved.body_json == "second")
                .map(|saved| saved.route_idx),
            Some(0)
        );
        assert_eq!(
            state
                .view_scrolls
                .iter()
                .find(|saved| saved.current == 10.0)
                .and_then(|saved| saved.route_idx),
            Some(1)
        );
        assert_eq!(
            state
                .view_scrolls
                .iter()
                .find(|saved| saved.current == 20.0)
                .and_then(|saved| saved.route_idx),
            Some(0)
        );
    }

    #[test]
    fn resetting_api_route_content_clears_pending_and_all_inner_scrolls() {
        let mut state = ApiClientTabState {
            route_idx: Some(7),
            pending: true,
            pending_request_id: Some(99),
            response: Some(ApiJobResponse {
                request_id: 99,
                spec_id: ApiSpecId(29),
                route_idx: 7,
                status: Some(200),
                elapsed_ms: 1,
                server_reach_ms: None,
                timing_text: String::new(),
                headers: Vec::new(),
                headers_text: String::new(),
                curl_text: String::new(),
                body: "stale".to_string(),
                truncated: false,
                error: None,
                resolved_host: None,
            }),
            ..Default::default()
        };
        for scroll in [
            &mut state.body_scroll,
            &mut state.body_scroll_x,
            &mut state.output_scroll,
            &mut state.output_scroll_x,
            &mut state.mock_static_response_scroll,
            &mut state.mock_static_response_scroll_x,
            &mut state.response_scroll,
            &mut state.response_scroll_x,
        ] {
            scroll.current = 20.0;
            scroll.target = 30.0;
            scroll.is_dragging = true;
        }

        state.reset_route_content(Some(2));

        assert_eq!(state.route_idx, Some(2));
        assert!(!state.pending);
        assert_eq!(state.pending_request_id, None);
        assert!(state.response.is_none());
        for scroll in [
            &state.body_scroll,
            &state.body_scroll_x,
            &state.output_scroll,
            &state.output_scroll_x,
            &state.mock_static_response_scroll,
            &state.mock_static_response_scroll_x,
            &state.response_scroll,
            &state.response_scroll_x,
        ] {
            assert_eq!(scroll.current, 0.0);
            assert_eq!(scroll.target, 0.0);
            assert!(!scroll.is_dragging);
        }
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
        save_url_cache(ApiSpecId(7), &sample_spec().to_string()).unwrap();
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
        let mut without_description = model.clone();
        without_description.routes[0].description.clear();
        let without_description_max =
            api_tab_max_scroll(Some(&without_description), &tab_state, None, 180.0, 1.0);
        assert!(tab_max > without_description_max);
        assert_eq!(api_tab_max_scroll(None, &tab_state, None, 180.0, 1.0), 0.0);
    }

    #[test]
    fn api_panel_scroll_height_matches_selected_model_header_rows() {
        let mut state = ApiClientState::default();
        let model = parse_openapi_model(ApiSpecId(5), &sample_spec()).expect("parse");
        state.models.insert(model.id, model.clone());
        state.collapsed_route_roots.insert(model.id);
        let base = api_panel_max_scroll(&state, 0.0, 1.0);
        state.selected_spec = Some(model.id);
        assert!(state.selected_model().is_some());
        // Renderer: routes gap, auth row, routes root row; root collapsed stops there.
        assert_eq!(
            api_panel_max_scroll(&state, 0.0, 2.0) - 2.0 * base
                - 2.0 * (API_PANEL_ROUTES_GAP_ADVANCE + 2.0 * API_PANEL_TREE_ROW_ADVANCE),
            0.0
        );
        assert_eq!(
            api_panel_max_scroll(&state, 0.0, 1.0) - base,
            API_PANEL_ROUTES_GAP_ADVANCE + 2.0 * API_PANEL_TREE_ROW_ADVANCE
        );
    }

    #[test]
    fn api_panel_scroll_height_matches_optional_rows_and_manual_route_stride() {
        let mut state = ApiClientState::default();
        let base = api_panel_max_scroll(&state, 0.0, 1.0);

        state.persistence_error = Some("persist failed".to_string());
        assert_eq!(
            api_panel_max_scroll(&state, 0.0, 1.0) - base,
            API_PANEL_PERSISTENCE_ERROR_ADVANCE
        );

        state.persistence_error = None;
        state.mock.uv.last_error = "python failed".to_string();
        assert_eq!(
            api_panel_max_scroll(&state, 0.0, 1.0) - base,
            API_PANEL_UV_ERROR_ADVANCE
        );

        state.mock.uv.last_error.clear();
        state
            .mock
            .manual_routes
            .push(crate::app::api_mock::types::ApiManualRoute {
                stable_id: "manual-test".to_string(),
                method: ApiMethod::Get,
                path: "/test".to_string(),
                enabled: true,
                response: crate::app::api_mock::types::ApiMockResponse::Generated,
                python: None,
                input_fields: Vec::new(),
                output_fields: Vec::new(),
            });
        for idx in 1..9 {
            state
                .mock
                .manual_routes
                .push(crate::app::api_mock::types::ApiManualRoute {
                    stable_id: format!("manual-test-{idx}"),
                    method: ApiMethod::Get,
                    path: format!("/test/{idx}"),
                    enabled: true,
                    response: crate::app::api_mock::types::ApiMockResponse::Generated,
                    python: None,
                    input_fields: Vec::new(),
                    output_fields: Vec::new(),
                });
        }
        assert_eq!(
            api_panel_max_scroll(&state, 0.0, 1.0) - base,
            9.0 * API_PANEL_MANUAL_ROUTE_ADVANCE
        );
    }

    #[test]
    fn api_panel_import_error_visibility_uses_same_timeout_as_renderer() {
        let mut state = ApiClientState::default();
        state.import_error = Some("failed".to_string());
        state.import_error_at = Some(100);
        assert!(api_panel_import_error_visible(&state, 104));
        assert!(!api_panel_import_error_visible(&state, 105));
        state.import_error_at = None;
        assert!(api_panel_import_error_visible(&state, u64::MAX));
    }

    #[test]
    fn api_description_markdown_lines_expose_heading_and_list_content() {
        let (kind, offset, content) = api_description_line_parts("### ⚙️ Настройки");
        assert_eq!(kind, ApiDescriptionLineKind::Heading);
        assert_eq!(content, "⚙️ Настройки");
        assert_eq!(&"### ⚙️ Настройки"[offset..], content);

        let (kind, offset, content) = api_description_line_parts("  - первый пункт");
        assert_eq!(kind, ApiDescriptionLineKind::ListItem);
        assert_eq!(content, "первый пункт");
        assert_eq!(&"  - первый пункт"[offset..], content);

        let (kind, offset, content) = api_description_line_parts("обычный текст");
        assert_eq!(kind, ApiDescriptionLineKind::Text);
        assert_eq!(offset, 0);
        assert_eq!(content, "обычный текст");
    }

    #[test]
    fn api_description_inline_markdown_exposes_bold_and_code_without_delimiters() {
        let text = "🪪  **Roles**: Requires `Admin` and ```Manager```";
        let spans = api_description_inline_spans(text).collect::<Vec<_>>();
        assert_eq!(
            spans.iter().map(|span| (span.kind, span.text)).collect::<Vec<_>>(),
            vec![
                (ApiDescriptionInlineKind::Text, "🪪  "),
                (ApiDescriptionInlineKind::Bold, "Roles"),
                (ApiDescriptionInlineKind::Text, ": Requires "),
                (ApiDescriptionInlineKind::Code, "Admin"),
                (ApiDescriptionInlineKind::Text, " and "),
                (ApiDescriptionInlineKind::Code, "Manager"),
            ]
        );
        for span in spans {
            assert_eq!(&text[span.source_start..span.source_end], span.text);
        }
    }

    #[test]
    fn api_description_inline_markdown_keeps_code_inside_bold_context() {
        let text = "**own `car wash` only**";
        assert_eq!(
            api_description_inline_spans(text)
                .map(|span| (span.kind, span.text))
                .collect::<Vec<_>>(),
            vec![
                (ApiDescriptionInlineKind::Bold, "own "),
                (ApiDescriptionInlineKind::Code, "car wash"),
                (ApiDescriptionInlineKind::Bold, " only"),
            ]
        );
    }

    #[test]
    fn api_description_visual_style_uses_primary_text_and_indented_centered_marker() {
        let primary = [0.91, 0.92, 0.93, 1.0];
        for kind in [
            ApiDescriptionLineKind::Heading,
            ApiDescriptionLineKind::Text,
            ApiDescriptionLineKind::ListItem,
        ] {
            assert_eq!(api_description_line_color(kind, primary), primary);
        }
        assert_eq!(API_DESCRIPTION_LIST_MARKER, "•");
        assert!(API_DESCRIPTION_LIST_MARKER_INDENT > 0.0);
        assert!(API_DESCRIPTION_LIST_CONTENT_INDENT > API_DESCRIPTION_LIST_MARKER_INDENT);
    }

    #[test]
    fn api_route_text_selection_copies_path_summary_and_description() {
        let cases = [
            (ApiRouteTextField::Path, "/cars/{id}", 0, 5, "/cars"),
            (ApiRouteTextField::Summary, "ListCars", 4, 8, "Cars"),
            (
                ApiRouteTextField::Description,
                "⚙️ route description",
                0,
                "⚙️".len(),
                "⚙️",
            ),
        ];
        for (field, text, anchor, cursor, expected) in cases {
            let selection = ApiRouteTextSelection {
                field,
                anchor,
                cursor,
                selecting: false,
            };
            assert_eq!(api_route_selected_text(selection, text), Some(expected));
        }

        let invalid = ApiRouteTextSelection {
            field: ApiRouteTextField::Description,
            anchor: 0,
            cursor: 1,
            selecting: false,
        };
        assert_eq!(api_route_selected_text(invalid, "⚙️"), None);
    }

    #[test]
    fn api_route_emoji_presentation_is_forced_only_by_variation_selector() {
        assert!(api_route_force_emoji_presentation(Some('\u{FE0F}')));
        assert!(!api_route_force_emoji_presentation(Some('a')));
        assert!(!api_route_force_emoji_presentation(None));
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

    #[test]
    fn multipart_file_picker_paths_remain_native_pathbuf_values() {
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
                                            "file": {"type": "string", "format": "binary"},
                                            "title": {"type": "string"}
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
        let model = parse_openapi_model(ApiSpecId(88), &spec).expect("parse");
        let route = &model.routes[0];
        let selected = PathBuf::from(r"\\server\share\folder with spaces\файл.bin");
        let values = vec![
            ApiInputValue {
                name: "file".to_string(),
                value: selected.to_string_lossy().into_owned(),
            },
            ApiInputValue {
                name: "title".to_string(),
                value: "avatar".to_string(),
            },
        ];
        let mut file_paths = FxHashMap::default();
        file_paths.insert("file".to_string(), vec![selected.clone()]);

        let parts = api_multipart_parts_for_route(route, &model, &values, &file_paths);
        assert_eq!(
            parts,
            vec![
                ApiMultipartPart::File {
                    name: "file".to_string(),
                    path: selected,
                },
                ApiMultipartPart::Text {
                    name: "title".to_string(),
                    value: "avatar".to_string(),
                },
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn multipart_file_picker_preserves_non_utf8_unix_paths() {
        use std::os::unix::ffi::OsStringExt;

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
                                            "file": {"type": "string", "format": "binary"}
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
        let model = parse_openapi_model(ApiSpecId(89), &spec).expect("parse");
        let selected = PathBuf::from(std::ffi::OsString::from_vec(
            b"/tmp/file-\xff.bin".to_vec(),
        ));
        let values = vec![ApiInputValue {
            name: "file".to_string(),
            value: selected.to_string_lossy().into_owned(),
        }];
        let mut file_paths = FxHashMap::default();
        file_paths.insert("file".to_string(), vec![selected.clone()]);

        let parts = api_multipart_parts_for_route(
            &model.routes[0],
            &model,
            &values,
            &file_paths,
        );
        assert_eq!(
            parts,
            vec![ApiMultipartPart::File {
                name: "file".to_string(),
                path: selected,
            }]
        );
    }

    #[test]
    fn route_memory_keeps_native_multipart_file_paths() {
        let mut state = ApiClientTabState {
            route_idx: Some(3),
            ..Default::default()
        };
        let path = PathBuf::from(r"C:\Users\Reyan\upload.bin");
        state
            .body_file_paths
            .insert("file".to_string(), vec![path.clone()]);
        state.remember_route_state();
        state.body_file_paths.clear();

        assert!(state.restore_route_state(3));
        assert_eq!(state.body_file_paths.get("file"), Some(&vec![path]));
    }

    #[test]
    fn portable_server_reach_uses_tcp_connect_instead_of_platform_ping() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let resolved = ApiResolvedHost {
            host: "localhost".to_string(),
            ip: addr.ip(),
            port: addr.port(),
        };
        assert!(measure_direct_server_reach_ms(Some(&resolved)).is_some());
    }

    #[test]
    fn api_python_shutdown_cancels_version_and_install_workers() {
        let mut state = ApiClientState::default();
        let list_cancel = Arc::new(AtomicBool::new(false));
        let install_cancel = Arc::new(AtomicBool::new(false));
        let (list_tx, list_rx) = mpsc::channel();
        let (install_tx, install_rx) = mpsc::channel();
        let list_worker_cancel = Arc::clone(&list_cancel);
        let install_worker_cancel = Arc::clone(&install_cancel);
        let list_worker = std::thread::spawn(move || {
            while !list_worker_cancel.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(2));
            }
            let _ = list_tx.send(ApiPythonVersionListResult {
                rows: Vec::new(),
                error: Some("cancelled".to_string()),
            });
        });
        let install_worker = std::thread::spawn(move || {
            while !install_worker_cancel.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(2));
            }
            let _ = install_tx.send(ApiPythonInstallEvent::Done(Err(
                "cancelled".to_string(),
            )));
        });
        state.python_version_list_cancel = Some(list_cancel);
        state.python_version_list_rx = Some(list_rx);
        state.python_install_cancel = Some(install_cancel);
        state.python_install_rx = Some(install_rx);
        state.mock_python_versions_loading = true;
        state.mock_python_install_running = true;

        state.shutdown_background_tasks();
        list_worker.join().unwrap();
        install_worker.join().unwrap();

        assert!(state.python_version_list_rx.is_none());
        assert!(state.python_install_rx.is_none());
        assert!(!state.mock_python_versions_loading);
        assert!(!state.mock_python_install_running);
    }

