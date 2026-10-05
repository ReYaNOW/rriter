#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    /// Route header of a route tab: method chip, path, summary and description.
    fn draw_api_client_tab_route_header(
        &mut self,
        ctx: ApiTabRouteCtx<'_>,
        mut cy: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
    ) -> f32 {
        let ApiTabRouteCtx {
            x, pad, content_w, s, mx, my, route_idx, route, tab_state,
            ..
        } = ctx;
        let method_w = 58.0 * s;
        self.draw_api_method_chip(route.method, x + pad, cy, method_w, 34.0 * s, s, 0.88);
        let mut display_path = String::new();
        write_api_path_display(&route.path, &mut display_path);
        let path_x = x + pad + method_w + 12.0 * s;
        let path_selection = tab_state
            .route_text_selection
            .filter(|selection| selection.field == ApiRouteTextField::Path)
            .and_then(|selection| selection.range(&display_path));
        self.draw_api_route_text_run(
            &display_path,
            path_x,
            cy + 23.0 * s,
            cy,
            34.0 * s,
            self.ui.pick(UiRole::TextPrimary, self.ui_theme.fg),
            1.14,
            false,
            0,
            path_selection,
        );
        ui_registry.register_text_region(
            crate::ui_system::UiId::ApiRoutePathText(route_idx),
            path_x,
            cy,
            (x + pad + content_w - path_x).max(1.0),
            34.0 * s,
            mx,
            my,
        );
        cy += API_ROUTE_PATH_ROW_ADVANCE * s;
        if !route.summary.is_empty() {
            let summary_selection = tab_state
                .route_text_selection
                .filter(|selection| selection.field == ApiRouteTextField::Summary)
                .and_then(|selection| selection.range(&route.summary));
            self.draw_api_route_text_run(
                &route.summary,
                x + pad,
                cy + 18.0 * s,
                cy,
                28.0 * s,
                self.ui.pick(UiRole::TextSecondary, [0.68, 0.70, 0.78, 1.0]),
                0.92,
                false,
                0,
                summary_selection,
            );
            ui_registry.register_text_region(
                crate::ui_system::UiId::ApiRouteSummaryText(route_idx),
                x + pad,
                cy,
                content_w,
                28.0 * s,
                mx,
                my,
            );
            cy += API_ROUTE_SUMMARY_ROW_ADVANCE * s;
        }
        if !route.description.trim().is_empty() {
            let description_y = cy;
            let description_h = self.draw_api_route_description(
                &route.description,
                x + pad,
                description_y,
                content_w,
                s,
                tab_state.route_text_selection,
            );
            ui_registry.register_text_region(
                crate::ui_system::UiId::ApiRouteDescriptionText(route_idx),
                x + pad,
                description_y,
                content_w,
                description_h.max(1.0),
                mx,
                my,
            );
            cy += description_h + API_ROUTE_BLOCK_GAP_ADVANCE * s;
        }
        cy
    }

    /// Authorization schemes required by the route.
    fn draw_api_client_tab_route_auth(
        &mut self,
        ctx: ApiTabRouteCtx<'_>,
        mut cy: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
    ) -> f32 {
        let ApiTabRouteCtx {
            x, pad, content_w, s, mx, my, blink_alpha, route, model, tab_meta, ide_panel,
            ..
        } = ctx;
        let auth_scheme_indices = api_route_auth_scheme_indices(model, route);
        if !auth_scheme_indices.is_empty() {
            self.draw_api_section_title("Авторизация", x + pad, cy + 18.0 * s, s);
            if api_route_auth_missing(model, route, &ide_panel.api.auth) {
                let title_w = self.measure_ui_width("Авторизация", API_SECTION_TITLE_SCALE);
                self.draw_string_scaled_stable(
                    "Не авторизовано",
                    x + pad + title_w + 16.0 * s,
                    cy + 18.0 * s,
                    self.ui.pick(UiRole::Error, [1.0, 0.42, 0.42, 1.0]),
                    0.86,
                );
            }
            cy += API_ROUTE_SECTION_TITLE_ADVANCE * s;
            self.draw_api_dynamic_table_frame(
                x + pad,
                cy,
                content_w,
                auth_scheme_indices
                    .iter()
                    .filter_map(|idx| model.security_schemes.get(*idx))
                    .map(|scheme| api_auth_scheme_row_height(scheme, s))
                    .sum::<f32>(),
                s,
            );
            for scheme_idx in auth_scheme_indices {
                if let Some(scheme) = model.security_schemes.get(scheme_idx) {
                    cy = self.draw_api_auth_scheme_row(
                        x + pad,
                        cy,
                        content_w,
                        s,
                        tab_meta.spec_id,
                        scheme_idx,
                        scheme,
                        ide_panel,
                        blink_alpha,
                        ui_registry,
                        mx,
                        my,
                    );
                }
            }
            cy += API_ROUTE_BLOCK_GAP_ADVANCE * s;
        }
        cy
    }

    /// Input / Schema switch above the request inputs.
    fn draw_api_client_tab_input_tabs(
        &mut self,
        ctx: ApiTabRouteCtx<'_>,
        mut cy: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
    ) -> f32 {
        let ApiTabRouteCtx {
            x, pad, s, mx, my, route_idx, tab_state,
            ..
        } = ctx;
        let input_tab_y = cy;
        let input_tab_h = 28.0 * s;
        let input_w = self.measure_ui_width("Input", 0.86) + 22.0 * s;
        let input_schema_w = self.measure_ui_width("Schema", 0.86) + 22.0 * s;
        self.draw_api_response_tab(
            "Input",
            tab_state.input_doc_view == ApiInputDocView::Input,
            x + pad,
            input_tab_y,
            input_w,
            input_tab_h,
            s,
            crate::ui_system::UiId::ApiInputExampleTab(route_idx),
            ui_registry,
            mx,
            my,
        );
        self.draw_api_response_tab(
            "Schema",
            tab_state.input_doc_view == ApiInputDocView::Schema,
            x + pad + input_w + 8.0 * s,
            input_tab_y,
            input_schema_w,
            input_tab_h,
            s,
            crate::ui_system::UiId::ApiInputSchemaTab(route_idx),
            ui_registry,
            mx,
            my,
        );
        cy += API_ROUTE_INPUT_TABS_ADVANCE * s;
        cy
    }

    /// Input schema view: summary, media menu and the schema text pane.
    fn draw_api_client_tab_input_schema(
        &mut self,
        ctx: ApiTabRouteCtx<'_>,
        mut cy: f32,
        manual_mock: Option<&crate::app::api_mock::types::ApiManualRoute>,
        mock_override: Option<&crate::app::api_mock::types::ApiMockRouteOverride>,
        ui_registry: &mut crate::ui_system::UiRegistry,
    ) -> f32 {
        let ApiTabRouteCtx {
            x, pad, content_w, s, mx, my, blink_alpha, tab_clip, route_idx, route, model, tab_meta,
            tab_state, ide_panel,
            ..
        } = ctx;
        let mock_input_contract = manual_mock
            .and_then(|route| route.python.as_ref())
            .or_else(|| mock_override.and_then(|item| item.python.as_ref()))
            .filter(|script| script.enabled)
            .map(|script| {
                let mut contract = api_mock_effective_contract(script, route, model);
                let contract_focused = matches!(
                    ide_panel.api.focused,
                    Some(ApiFocus::MockContract { route_idx: f_route })
                        if f_route == route_idx
                );
                if contract_focused {
                    let text = ide_panel.api.input_editor.get_full_text();
                    contract = api_mock_contract_from_state_text(&contract, &text);
                }
                contract
            });
        let selected_media_count =
            api_route_selected_media_count(mock_input_contract.is_some(), route);
        let selected_schema_idx = tab_state
            .input_schema_idx
            .min(selected_media_count.saturating_sub(1));
        let input_schema_text = if let Some(contract) = mock_input_contract.as_ref() {
            api_mock_input_schema_text(contract)
        } else {
            api_route_input_schema_text(
                route,
                model,
                selected_schema_idx,
                &tab_state.input_schema_collapsed,
            )
        };
        let input_target_h = api_route_input_view_height(route, model, tab_state, s);
        let schema_h = input_target_h;
        let summary = if let Some(contract) = mock_input_contract.as_ref() {
            api_mock_input_schema_summary(contract)
        } else {
            api_route_input_schema_summary(route, model, selected_schema_idx)
        };
        let menu_label = api_route_input_media_label(route, selected_schema_idx);
        let menu_w = (self.measure_ui_width(&menu_label, 0.82) + 34.0 * s)
            .clamp(146.0 * s, (content_w * 0.46).max(146.0 * s));
        let menu_x = (x + pad + content_w - menu_w).round();
        self.draw_api_schema_summary(&summary, x + pad, cy + 18.0 * s);
        if selected_media_count > 1 {
            self.push_rounded_rect(
                menu_x,
                cy,
                menu_w,
                28.0 * s,
                5.0 * s,
                self.ui.pick(UiRole::BgPanelAlt, [0.18, 0.19, 0.23, 1.0]),
            );
            ui_registry.register_rect(
                crate::ui_system::UiId::ApiInputSchemaMenu(route_idx),
                menu_x,
                cy,
                menu_w,
                28.0 * s,
                mx,
                my,
            );
            self.draw_string_scaled_stable(
                &menu_label,
                menu_x + 10.0 * s,
                api_centered_text_y(cy, 28.0 * s, s),
                self.ui.pick(UiRole::TextSecondary, [0.78, 0.80, 0.88, 1.0]),
                0.82,
            );
            self.draw_string_scaled_stable(
                if tab_state.input_schema_menu_open {
                    "▼"
                } else {
                    "▶"
                },
                menu_x + menu_w - 18.0 * s,
                api_centered_text_y(cy, 28.0 * s, s),
                self.ui.pick(UiRole::TextSecondary, [0.78, 0.80, 0.88, 1.0]),
                0.82,
            );
        }
        cy += API_ROUTE_SECTION_TITLE_ADVANCE * s;
        if selected_media_count > 1 && tab_state.input_schema_menu_open {
            for media_idx in 0..selected_media_count {
                let item_y = cy + media_idx as f32 * 30.0 * s;
                let label = api_route_input_media_label(route, media_idx);
                self.push_rounded_rect(
                    menu_x,
                    item_y,
                    menu_w,
                    28.0 * s,
                    4.0 * s,
                    if media_idx == selected_schema_idx {
                        self.ui.ink(0.12)
                    } else {
                        self.ui.pick(UiRole::BgPanelAlt, [0.15, 0.16, 0.20, 1.0])
                    },
                );
                ui_registry.register_rect(
                    crate::ui_system::UiId::ApiInputSchemaMenuItem(route_idx, media_idx),
                    menu_x,
                    item_y,
                    menu_w,
                    28.0 * s,
                    mx,
                    my,
                );
                self.draw_string_scaled_stable(
                    &label,
                    menu_x + 10.0 * s,
                    api_centered_text_y(item_y, 28.0 * s, s),
                    self.ui.pick(UiRole::TextPrimary, self.ui_theme.fg),
                    0.80,
                );
            }
            cy += selected_media_count as f32 * API_ROUTE_INPUT_MEDIA_MENU_ITEM_ADVANCE * s
                + API_ROUTE_INPUT_MEDIA_MENU_TAIL_ADVANCE * s;
        }
        let input_schema_focused = tab_state.focused_schema_pane
            == Some(crate::app::api_client::ApiSchemaPaneFocus::Input);
        self.push_rounded_rect_border(
            x + pad,
            cy,
            content_w,
            schema_h,
            0.0,
            (1.0 * s).max(1.0),
            if input_schema_focused {
                self.ui.pick(UiRole::Accent, [0.60, 0.35, 0.85, 1.0])
            } else {
                self.ui.ink(0.12)
            },
            self.ui.pick(UiRole::BgInput, [0.12, 0.13, 0.17, 1.0]),
        );
        ui_registry.register_text_input(
            crate::ui_system::UiId::ApiInputSchemaBody(route_idx),
            x + pad,
            cy,
            content_w,
            schema_h,
            mx,
            my,
        );
        let schema_clip = (
            x + pad + 10.0 * s,
            cy + 8.0 * s,
            content_w - 20.0 * s,
            schema_h - 16.0 * s,
        );
        if self.begin_api_text_clip(schema_clip, tab_clip) {
            let input_schema_text_focused = matches!(
                ide_panel.api.focused,
                Some(ApiFocus::InputSchema { spec_id, route_idx: f_route })
                    if spec_id == tab_meta.spec_id && f_route == route_idx
            );
            if input_schema_text_focused {
                let schema_text_top = api_text_area_top_from_baseline(cy + 29.0 * s, s);
                self.draw_api_editor_selection_multiline_ui(
                    &ide_panel.api.input_editor,
                    x + pad + 10.0 * s,
                    schema_text_top,
                    content_w - 20.0 * s,
                    schema_h - 16.0 * s,
                    s,
                    tab_state.body_scroll.current.round(),
                    0.0,
                );
            }
            self.draw_api_schema_text_area(
                &input_schema_text,
                x + pad + 10.0 * s,
                cy + 29.0 * s,
                content_w - 20.0 * s,
                schema_h - 16.0 * s,
                s,
                tab_state.body_scroll.current.round(),
                0.0,
                false,
                true,
                route_idx,
                ui_registry,
                mx,
                my,
            );
            if input_schema_text_focused && blink_alpha > 0.5 {
                let schema_text_top = api_text_area_top_from_baseline(cy + 29.0 * s, s);
                self.draw_api_editor_cursor_multiline_ui(
                    &ide_panel.api.input_editor,
                    x + pad + 10.0 * s,
                    schema_text_top,
                    content_w - 20.0 * s,
                    schema_h - 16.0 * s,
                    s,
                    tab_state.body_scroll.current.round(),
                    0.0,
                );
            }
            self.draw_api_schema_scrollbar(
                &input_schema_text,
                x + pad + content_w - 15.0 * s,
                cy + 8.0 * s,
                content_w - 20.0 * s,
                schema_h - 16.0 * s,
                s,
                tab_state.body_scroll.current.round(),
                crate::ui_system::UiId::ApiBodyScrollY(route_idx),
                ui_registry,
                mx,
                my,
            );
            self.restore_api_tab_clip(tab_clip);
        }
        cy += schema_h + API_ROUTE_INPUT_SCHEMA_GAP_ADVANCE * s;
        cy
    }

    /// Path and query parameter tables of the Input view.
    fn draw_api_client_tab_path_query_params(
        &mut self,
        ctx: ApiTabRouteCtx<'_>,
        mut cy: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
    ) -> f32 {
        let ApiTabRouteCtx {
            x, pad, content_w, s, mx, my, blink_alpha, route_idx, route, tab_meta, tab_state,
            ide_panel,
            ..
        } = ctx;
        if !route.path_params.is_empty() {
            self.draw_api_section_title("Параметры пути", x + pad, cy + 18.0 * s, s);
            cy += API_ROUTE_SECTION_TITLE_ADVANCE * s;
            let mut table_h = 0.0;
            for param in &route.path_params {
                let value = tab_state
                    .path_values
                    .iter()
                    .find(|v| v.name == param.name)
                    .map(|v| v.value.as_str())
                    .unwrap_or("");
                table_h += self.api_param_row_layout(content_w, s, param, value).row_h;
            }
            self.draw_api_dynamic_table_frame(x + pad, cy, content_w, table_h, s);
            for (param_idx, param) in route.path_params.iter().enumerate() {
                cy = self.draw_api_param_input(
                x + pad,
                cy,
                content_w,
                s,
                route_idx,
                param_idx,
                param,
                tab_state
                    .path_values
                    .iter()
                    .find(|v| v.name == param.name)
                    .map(|v| v.value.as_str())
                    .unwrap_or(""),
                matches!(
                    ide_panel.api.focused,
                    Some(ApiFocus::PathParam { spec_id, route_idx: f_route, ref name })
                        if spec_id == tab_meta.spec_id && f_route == route_idx && name == &param.name
                ),
                ide_panel.api.input_scroll_x.current,
                &ide_panel.api.input_editor,
                blink_alpha,
                crate::ui_system::UiId::ApiPathParamInput(route_idx, param_idx),
                ui_registry,
                mx,
                my,
            );
            }
            cy += API_ROUTE_BLOCK_GAP_ADVANCE * s;
        }

        if !route.query_params.is_empty() {
            self.draw_api_section_title("Параметры query", x + pad, cy + 18.0 * s, s);
            cy += API_ROUTE_SECTION_TITLE_ADVANCE * s;
            let mut table_h = 0.0;
            for param in &route.query_params {
                let value = tab_state
                    .query_values
                    .iter()
                    .find(|v| v.name == param.name)
                    .map(|v| v.value.as_str())
                    .unwrap_or("");
                table_h += self.api_param_row_layout(content_w, s, param, value).row_h;
            }
            self.draw_api_dynamic_table_frame(x + pad, cy, content_w, table_h, s);
            for (param_idx, param) in route.query_params.iter().enumerate() {
                cy = self.draw_api_param_input(
                x + pad,
                cy,
                content_w,
                s,
                route_idx,
                param_idx,
                param,
                tab_state
                    .query_values
                    .iter()
                    .find(|v| v.name == param.name)
                    .map(|v| v.value.as_str())
                    .unwrap_or(""),
                matches!(
                    ide_panel.api.focused,
                    Some(ApiFocus::QueryParam { spec_id, route_idx: f_route, ref name })
                        if spec_id == tab_meta.spec_id && f_route == route_idx && name == &param.name
                ),
                ide_panel.api.input_scroll_x.current,
                &ide_panel.api.input_editor,
                blink_alpha,
                crate::ui_system::UiId::ApiQueryParamInput(route_idx, param_idx),
                ui_registry,
                mx,
                my,
            );
            }
            cy += API_ROUTE_BLOCK_GAP_ADVANCE * s;
        }
        cy
    }

    /// Request body of the Input view: form fields or the JSON/text editor.
    fn draw_api_client_tab_request_body(
        &mut self,
        ctx: ApiTabRouteCtx<'_>,
        mut cy: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
    ) -> f32 {
        let ApiTabRouteCtx {
            x, pad, content_w, s, mx, my, blink_alpha, tab_clip, route_idx, route, model, tab_meta,
            tab_state, ide_panel,
            ..
        } = ctx;
        if let Some(body) = &route.request_body {
            self.draw_api_section_title("Body", x + pad, cy + 18.0 * s, s);
            self.draw_string_scaled_stable(
                &body.content_type,
                x + pad + 52.0 * s,
                cy + 18.0 * s,
                self.ui.pick(UiRole::Info, [0.35, 0.75, 1.0, 1.0]),
                0.84,
            );
            let body_focused = matches!(
                ide_panel.api.focused,
                Some(ApiFocus::Body { spec_id, route_idx: f_route })
                    if spec_id == tab_meta.spec_id && f_route == route_idx
            );
            let validates_json = !body.is_multipart
                && !body.is_form_urlencoded
                && api_content_type_is_json(&body.content_type);
            if validates_json {
                let valid = if body_focused {
                    ide_panel
                        .api
                        .body_json_valid_for(
                            tab_meta.spec_id,
                            route_idx,
                            ide_panel.api.input_editor.version,
                        )
                        .unwrap_or_else(|| json_body_is_valid(&tab_state.body_json))
                } else {
                    json_body_is_valid(&tab_state.body_json)
                };
                let body_type_w = self.measure_ui_width(&body.content_type, 0.84);
                self.draw_string_scaled_stable(
                    if valid {
                        "JSON корректен"
                    } else {
                        "JSON с ошибкой"
                    },
                    x + pad + 52.0 * s + body_type_w + 12.0 * s,
                    cy + 18.0 * s,
                    if valid {
                        self.ui.pick(UiRole::Success, [0.48, 0.86, 0.52, 1.0])
                    } else {
                        self.ui.pick(UiRole::Error, [1.0, 0.42, 0.42, 1.0])
                    },
                    0.92,
                );
            }
            cy += API_ROUTE_SECTION_TITLE_ADVANCE * s;
            if body.is_multipart || body.is_form_urlencoded {
                if let Some(schema_ref) = body.schema
                    && let Some(schema) = model.schema_arena.get(schema_ref.0)
                {
                    let mut table_h = 0.0;
                    for prop in &schema.properties {
                        if let Some(prop_schema) = model.schema_arena.get(prop.schema.0) {
                            let value = tab_state
                                .body_values
                                .iter()
                                .find(|item| item.name == prop.name)
                                .map(|item| item.value.as_str())
                                .unwrap_or("");
                            table_h += self
                                .api_body_prop_row_layout(
                                    content_w,
                                    s,
                                    prop_schema,
                                    model,
                                    value,
                                )
                                .row_h;
                        }
                    }
                    self.draw_api_dynamic_table_frame(x + pad, cy, content_w, table_h, s);
                    for (prop_idx, prop) in schema.properties.iter().enumerate() {
                        if let Some(prop_schema) = model.schema_arena.get(prop.schema.0) {
                            let focused = matches!(
                                ide_panel.api.focused,
                                Some(ApiFocus::BodyField { spec_id, route_idx: f_route, ref name })
                                    if spec_id == tab_meta.spec_id
                                        && f_route == route_idx
                                        && name == &prop.name
                            );
                            let value = tab_state
                                .body_values
                                .iter()
                                .find(|item| item.name == prop.name)
                                .map(|item| item.value.as_str())
                                .unwrap_or("");
                            let row_h = self.draw_api_body_prop_row(
                                x + pad,
                                cy,
                                content_w,
                                s,
                                route_idx,
                                prop_idx,
                                &prop.name,
                                prop.required,
                                prop_schema,
                                model,
                                value,
                                focused,
                                ide_panel.api.input_scroll_x.current,
                                &ide_panel.api.input_editor,
                                blink_alpha,
                                ui_registry,
                                mx,
                                my,
                            );
                            cy += row_h;
                        }
                    }
                    cy += API_ROUTE_INPUT_SCHEMA_GAP_ADVANCE * s;
                }
            } else {
                let body_text = if body_focused {
                    ide_panel.api.input_editor.get_full_text()
                } else {
                    tab_state.body_json.clone()
                };
                let body_h = api_body_text_area_height(&body_text, s);
                self.push_rounded_rect_border(
                    x + pad,
                    cy,
                    content_w,
                    body_h,
                    0.0,
                    (1.0 * s).max(1.0),
                    if matches!(
                        ide_panel.api.focused,
                        Some(ApiFocus::Body { spec_id, route_idx: f_route })
                            if spec_id == tab_meta.spec_id && f_route == route_idx
                    ) {
                        self.ui.pick(UiRole::Accent, [0.60, 0.35, 0.85, 1.0])
                    } else {
                        self.ui.ink(0.12)
                    },
                    self.ui.pick(UiRole::BgInput, [0.13, 0.14, 0.18, 1.0]),
                );
                ui_registry.register_text_input(
                    crate::ui_system::UiId::ApiBodyInput(route_idx),
                    x + pad,
                    cy,
                    content_w,
                    body_h,
                    mx,
                    my,
                );
                let body_clip = (
                    x + pad + 10.0 * s,
                    cy + 8.0 * s,
                    content_w - 20.0 * s,
                    body_h - 16.0 * s,
                );
                if self.begin_api_text_clip(body_clip, tab_clip) {
                    if body_focused {
                        let text_top = api_text_area_top_from_baseline(cy + 29.0 * s, s);
                        self.draw_api_editor_selection_multiline_ui(
                            &ide_panel.api.input_editor,
                            x + pad + 10.0 * s,
                            text_top,
                            content_w - 20.0 * s,
                            body_h - 16.0 * s,
                            s,
                            tab_state.body_scroll.current,
                            tab_state.body_scroll_x.current,
                        );
                    }
                    self.draw_json_text_area(
                        &body_text,
                        x + pad + 10.0 * s,
                        cy + 29.0 * s,
                        content_w - 20.0 * s,
                        body_h - 16.0 * s,
                        s,
                        tab_state.body_scroll.current,
                        tab_state.body_scroll_x.current,
                        false,
                    );
                    self.draw_api_text_scrollbar(
                        &body_text,
                        x + pad + content_w - 8.0 * s,
                        cy + 8.0 * s,
                        body_h - 16.0 * s,
                        s,
                        tab_state.body_scroll.current,
                        crate::ui_system::UiId::ApiBodyScrollY(route_idx),
                        ui_registry,
                        mx,
                        my,
                    );
                    if body_focused && blink_alpha > 0.5 {
                        let text_top = api_text_area_top_from_baseline(cy + 29.0 * s, s);
                        self.draw_api_editor_cursor_multiline_ui(
                            &ide_panel.api.input_editor,
                            x + pad + 10.0 * s,
                            text_top,
                            content_w - 20.0 * s,
                            body_h - 16.0 * s,
                            s,
                            tab_state.body_scroll.current,
                            tab_state.body_scroll_x.current,
                        );
                    }
                    self.restore_api_tab_clip(tab_clip);
                    self.draw_api_text_scrollbar_x(
                        &body_text,
                        x + pad + 8.0 * s,
                        cy + body_h - 12.0 * s,
                        content_w - 16.0 * s,
                        content_w - 20.0 * s,
                        tab_state.body_scroll_x.current,
                        crate::ui_system::UiId::ApiBodyScrollX(route_idx),
                        ui_registry,
                        mx,
                        my,
                    );
                }
                cy += body_h + API_ROUTE_INPUT_SCHEMA_GAP_ADVANCE * s;
            }
        }
        cy
    }

    /// Server chips and the send button.
    fn draw_api_client_tab_servers_and_send(
        &mut self,
        ctx: ApiTabRouteCtx<'_>,
        mut cy: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
    ) -> f32 {
        let ApiTabRouteCtx {
            x, pad, content_w, s, mx, my, model, tab_state,
            ..
        } = ctx;
        if model.servers.len() > 1 {
            self.draw_api_section_title("Сервер", x + pad, cy + 18.0 * s, s);
            cy += API_ROUTE_SECTION_TITLE_ADVANCE * s;
            let mut sx = x + pad;
            for (idx, server) in model.servers.iter().enumerate() {
                let label = server.url.as_str();
                let server_text_scale = 0.92;
                let chip_w = (self.measure_ui_width(label, server_text_scale) + 20.0 * s)
                    .max(72.0 * s)
                    .min(content_w);
                if sx + chip_w > x + pad + content_w {
                    sx = x + pad;
                    cy += API_ROUTE_SERVER_CHIP_ROW_ADVANCE * s;
                }
                let active = idx == tab_state.server_idx;
                self.push_rounded_rect(
                    sx,
                    cy,
                    chip_w,
                    32.0 * s,
                    5.0 * s,
                    if active {
                        self.ui.pick(UiRole::Accent, [0.35, 0.26, 0.48, 1.0])
                    } else {
                        self.ui.pick(UiRole::BgPanelAlt, [0.18, 0.19, 0.23, 1.0])
                    },
                );
                ui_registry.register_rect(
                    crate::ui_system::UiId::ApiServerSelect(idx),
                    sx,
                    cy,
                    chip_w,
                    32.0 * s,
                    mx,
                    my,
                );
                self.draw_string_scaled_stable(
                    label,
                    sx + 10.0 * s,
                    api_centered_text_y(cy, 32.0 * s, s),
                    self.ui.pick(UiRole::TextPrimary, self.ui_theme.fg),
                    server_text_scale,
                );
                sx += chip_w + 8.0 * s;
            }
            cy += API_ROUTE_SERVER_TAIL_ADVANCE * s;
        }

        let try_btn = Button {
            x: x + pad,
            y: cy,
            w: 148.0 * s,
            h: 38.0 * s,
            text: if tab_state.pending {
                "Жду ответ".to_string()
            } else {
                "Отправить".to_string()
            },
            icon: Some(IconType::Reload),
            text_scale: 0.96,
            icon_size: 22.0 * s,
        };
        try_btn.render(self, mx, my, s, false);
        if !tab_state.pending {
            ui_registry.register_rect(
                crate::ui_system::UiId::ApiTryRequest,
                try_btn.x,
                try_btn.y,
                try_btn.w,
                try_btn.h,
                mx,
                my,
            );
        }
        cy += API_ROUTE_TRY_BUTTON_ADVANCE * s;
        cy
    }
}
