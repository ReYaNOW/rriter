#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    pub(crate) fn draw_editor_line_number(
        &mut self,
        line_no: usize,
        right_x: f32,
        right_pad: f32,
        baseline_y: f32,
        scale: f32,
    ) {
        let mut buf = [0u8; 20];
        if let Some(num_str) = editor_line_number_text(line_no, &mut buf) {
            let num_w = self.measure_mono_width(num_str, scale);
            let draw_x = right_x - right_pad - num_w;
            self.draw_string_mono_scaled(num_str, draw_x, baseline_y, self.theme.line_num, scale);
        }
    }

    pub(crate) fn draw_editor_line_number_centered(
        &mut self,
        line_no: usize,
        x: f32,
        w: f32,
        baseline_y: f32,
        scale: f32,
    ) {
        let mut buf = [0u8; 20];
        if let Some(num_str) = editor_line_number_text(line_no, &mut buf) {
            let num_w = self.measure_mono_width(num_str, scale);
            let draw_x = x + ((w - num_w) * 0.5).round();
            self.draw_string_mono_scaled(num_str, draw_x, baseline_y, self.theme.line_num, scale);
        }
    }

    fn api_mono_width(&mut self, text: &str) -> f32 {
        text.chars().map(|ch| self.char_advance(ch)).sum()
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_api_client_tab(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        _editor: &crate::editor::Editor,
        ide_panel: &crate::app::IdePanelState,
        tab_meta: &crate::app::api_client::ApiClientTabMeta,
        tab_state: &crate::app::api_client::ApiClientTabState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        blink_alpha: f32,
    ) {
        self.push_rect(x, y, w, h, self.theme.bg);
        ui_registry.register_blocker(crate::ui_system::UiId::ApiTabBody, x, y, w, h, mx, my);
        let manual_route = match &tab_meta.route_identity {
            Some(crate::app::api_client::ApiClientRouteIdentity::Manual { stable_id }) => ide_panel
                .api
                .mock
                .manual_routes
                .iter()
                .enumerate()
                .find(|(_, route)| route.stable_id == *stable_id),
            _ => None,
        };
        let manual_model;
        let model = if let Some((_, route)) = manual_route {
            manual_model = api_manual_route_model(route);
            &manual_model
        } else {
            let Some(model) = ide_panel.api.models.get(&tab_meta.spec_id) else {
                self.draw_string_scaled_stable(
                    "Спецификация загружается или кэш пустой",
                    x + 28.0 * s,
                    y + 46.0 * s,
                    [0.72, 0.74, 0.82, 1.0],
                    0.95,
                );
                return;
            };
            model
        };
        if tab_state.auth_view {
            self.flush();
            unsafe {
                self.gl.enable(glow::SCISSOR_TEST);
                self.gl.scissor(
                    x.round() as i32,
                    (self.height - (y + h)).round() as i32,
                    w.round() as i32,
                    h.round() as i32,
                );
            }
            let tab_clip = (x, y, w, h);
            let pad = 28.0 * s;
            let content_w = (w - pad * 2.0).max(1.0);
            let scroll = tab_state.tab_scroll.current.round();
            let mut cy = y + pad - scroll;

            self.draw_string_scaled_stable(
                "Авторизация",
                x + pad,
                cy + 24.0 * s,
                self.theme.fg,
                1.18,
            );
            cy += 38.0 * s;
            if model.security_schemes.is_empty() {
                self.draw_string_scaled_stable(
                    "Схем авторизации нет",
                    x + pad,
                    cy + 20.0 * s,
                    [0.68, 0.70, 0.78, 1.0],
                    0.90,
                );
            } else {
                self.draw_api_dynamic_table_frame(
                    x + pad,
                    cy,
                    content_w,
                    model
                        .security_schemes
                        .iter()
                        .map(|scheme| api_auth_scheme_row_height(scheme, s))
                        .sum::<f32>(),
                    s,
                );
                for (scheme_idx, scheme) in model.security_schemes.iter().enumerate() {
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
            let auth_route_count = api_auth_related_route_count(model).min(12);
            if auth_route_count > 0 {
                cy += 28.0 * s;
                self.draw_api_section_title("Роуты авторизации", x + pad, cy + 18.0 * s, s);
                cy += 28.0 * s;
                self.draw_api_dynamic_table_frame(
                    x + pad,
                    cy,
                    content_w,
                    auth_route_count as f32 * 34.0 * s,
                    s,
                );
                let mut drawn = 0usize;
                for rank in 0..=2 {
                    for (route_idx, route) in model.routes.iter().enumerate() {
                        if drawn >= auth_route_count {
                            break;
                        }
                        if api_auth_route_rank(route) != Some(rank) {
                            continue;
                        }
                        let row_y = cy + drawn as f32 * 34.0 * s;
                        let method_w = 56.0 * s;
                        self.draw_api_method_chip(
                            route.method,
                            x + pad + 8.0 * s,
                            row_y + 5.0 * s,
                            method_w,
                            24.0 * s,
                            s,
                            0.72,
                        );
                        let mut display_path = String::new();
                        write_api_path_display(&route.path, &mut display_path);
                        self.draw_string_scaled_stable(
                            &display_path,
                            x + pad + method_w + 20.0 * s,
                            row_y + 22.0 * s,
                            self.theme.fg,
                            0.86,
                        );
                        if !route.summary.is_empty() {
                            let path_w = self.measure_ui_width(&display_path, 0.86);
                            self.draw_string_scaled_stable(
                                &route.summary,
                                x + pad + method_w + path_w + 32.0 * s,
                                row_y + 22.0 * s,
                                [0.62, 0.64, 0.72, 1.0],
                                0.78,
                            );
                        }
                        ui_registry.register_rect(
                            crate::ui_system::UiId::ApiRouteRow(route_idx),
                            x + pad,
                            row_y,
                            content_w,
                            34.0 * s,
                            mx,
                            my,
                        );
                        self.push_rect(
                            x + pad,
                            row_y + 34.0 * s,
                            content_w,
                            1.0,
                            [1.0, 1.0, 1.0, 0.08],
                        );
                        drawn += 1;
                    }
                }
            }
            self.restore_api_tab_clip(tab_clip);
            self.flush();
            unsafe {
                self.gl.disable(glow::SCISSOR_TEST);
            }
            return;
        }
        let Some(route_idx) = tab_state
            .route_idx
            .or_else(|| (!model.routes.is_empty()).then_some(0))
        else {
            self.draw_string_scaled_stable(
                "В спецификации нет routes",
                x + 28.0 * s,
                y + 46.0 * s,
                [0.72, 0.74, 0.82, 1.0],
                0.95,
            );
            return;
        };
        let Some(route) = (if manual_route.is_some() {
            model.routes.first()
        } else {
            model.routes.get(route_idx)
        }) else {
            return;
        };
        let manual_mock = manual_route.map(|(_, route)| route);
        let mock_override = if manual_mock.is_none() {
            ide_panel
                .api
                .mock
                .route_overrides
                .iter()
                .find(|item| item.method == route.method && item.path == route.path)
        } else {
            None
        };
        let mock_enabled = manual_mock.is_some() || mock_override.is_some_and(|item| item.enabled);
        let is_manual_mock = manual_mock.is_some();
        let python_enabled = manual_mock
            .and_then(|route| route.python.as_ref())
            .or_else(|| mock_override.and_then(|item| item.python.as_ref()))
            .is_some_and(|script| script.enabled);

        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            self.gl.scissor(
                x.round() as i32,
                (self.height - (y + h)).round() as i32,
                w.round() as i32,
                h.round() as i32,
            );
        }

        let tab_clip = (x, y, w, h);
        let pad = 28.0 * s;
        let content_w = (w - pad * 2.0).max(1.0);
        let scroll = tab_state.tab_scroll.current.round();
        let mut cy = y + pad - scroll;

        let method_w = 58.0 * s;
        self.draw_api_method_chip(route.method, x + pad, cy, method_w, 34.0 * s, s, 0.88);
        let mut display_path = String::new();
        write_api_path_display(&route.path, &mut display_path);
        self.draw_string_scaled_stable(
            &display_path,
            x + pad + method_w + 12.0 * s,
            cy + 23.0 * s,
            self.theme.fg,
            1.14,
        );
        cy += 42.0 * s;
        if !route.summary.is_empty() {
            self.draw_string_scaled_stable(
                &route.summary,
                x + pad,
                cy + 18.0 * s,
                [0.68, 0.70, 0.78, 1.0],
                0.92,
            );
            cy += 28.0 * s;
        }

        self.draw_api_section_title("Мок", x + pad, cy + 18.0 * s, s);
        if mock_enabled {
            self.draw_string_scaled_stable(
                if python_enabled {
                    if is_manual_mock {
                        "(Всегда вкл, python)"
                    } else {
                        "(Включен с python)"
                    }
                } else if is_manual_mock {
                    "(Всегда вкл)"
                } else {
                    "(Включен)"
                },
                x + pad + 48.0 * s,
                (cy + 18.0 * s).round(),
                [0.48, 0.86, 0.52, 1.0],
                0.86,
            );
        }
        cy += 30.0 * s;
        let mock_expanded = ide_panel
            .api
            .expanded_mock_routes
            .contains(&(tab_meta.spec_id, route_idx));
        let mock_toggle = Button {
            x: x + pad,
            y: cy,
            w: 232.0 * s,
            h: 34.0 * s,
            text: if mock_expanded {
                "Скрыть настройки мока"
            } else {
                "Настроить мок"
            }
            .to_string(),
            icon: Some(IconType::Api),
            text_scale: 0.90,
            icon_size: 19.0 * s,
        };
        ui_registry.register_button(
            crate::ui_system::UiId::ApiMockRouteDetailsToggle(route_idx),
            &mock_toggle,
            self,
            mx,
            my,
            s,
            false,
        );
        cy += 46.0 * s;
        let mock_frame_y = cy - 8.0 * s;
        if mock_expanded {
            let btn_h = 30.0 * s;
            let mut button_x = x + pad;
            if !is_manual_mock {
                let enable_btn = Button {
                    x: button_x,
                    y: cy,
                    w: 128.0 * s,
                    h: btn_h,
                    text: if mock_enabled {
                        "Мок вкл"
                    } else {
                        "Мок выкл"
                    }
                    .to_string(),
                    icon: Some(if mock_enabled {
                        IconType::Check
                    } else {
                        IconType::Close
                    }),
                    text_scale: 0.86,
                    icon_size: 18.0 * s,
                };
                ui_registry.register_button(
                    crate::ui_system::UiId::ApiMockRouteEnable(route_idx),
                    &enable_btn,
                    self,
                    mx,
                    my,
                    s,
                    false,
                );
                button_x += 140.0 * s;
            }
            let python_btn = Button {
                x: button_x,
                y: cy,
                w: 138.0 * s,
                h: btn_h,
                text: if python_enabled {
                    "Python вкл"
                } else {
                    "Python выкл"
                }
                .to_string(),
                icon: Some(if python_enabled {
                    IconType::Check
                } else {
                    IconType::Close
                }),
                text_scale: 0.86,
                icon_size: 18.0 * s,
            };
            ui_registry.register_button(
                crate::ui_system::UiId::ApiMockRoutePythonToggle(route_idx),
                &python_btn,
                self,
                mx,
                my,
                s,
                false,
            );
            button_x += 150.0 * s;
            let reset_btn = Button {
                x: button_x,
                y: cy,
                w: btn_h,
                h: btn_h,
                text: String::new(),
                icon: Some(IconType::Discard),
                text_scale: 0.86,
                icon_size: 17.0 * s,
            };
            ui_registry.register_button(
                crate::ui_system::UiId::ApiMockRouteReset(route_idx),
                &reset_btn,
                self,
                mx,
                my,
                s,
                false,
            );
            cy += btn_h + 18.0 * s;
            match &ide_panel.api.mock.check_status {
                crate::app::api_mock::types::ApiMockCheckStatus::Ok {
                    route_idx: checked,
                    message,
                    ..
                } if *checked == route_idx => {
                    self.draw_string_scaled_stable(
                        message.lines().next().unwrap_or("Ty проверка прошла"),
                        x + pad,
                        cy + 16.0 * s,
                        [0.50, 0.90, 0.55, 1.0],
                        0.76,
                    );
                    cy += 22.0 * s;
                }
                crate::app::api_mock::types::ApiMockCheckStatus::Failed { .. } => {}
                _ => {}
            }
            if !python_enabled {
                let static_focused = matches!(
                    ide_panel.api.focused,
                    Some(ApiFocus::MockStaticResponse { route_idx: f_route }) if f_route == route_idx
                );
                let static_text = if static_focused {
                    ide_panel.api.input_editor.get_full_text()
                } else {
                    let generated = api_generated_response_for_route(route, model).2;
                    let mock_response = manual_mock
                        .map(|route| &route.response)
                        .or_else(|| mock_override.map(|item| &item.response));
                    mock_response
                        .map(|response| match response {
                            crate::app::api_mock::types::ApiMockResponse::Generated => {
                                generated.clone()
                            }
                            crate::app::api_mock::types::ApiMockResponse::Json(text)
                            | crate::app::api_mock::types::ApiMockResponse::Text(text) => {
                                text.clone()
                            }
                        })
                        .unwrap_or(generated)
                };
                self.draw_string_scaled_stable(
                    "Ответ мока",
                    x + pad,
                    cy + 16.0 * s,
                    [0.68, 0.70, 0.78, 1.0],
                    0.82,
                );
                cy += 22.0 * s;
                let editor_h = 195.0 * s;
                self.push_rounded_rect_border(
                    x + pad,
                    cy,
                    content_w,
                    editor_h,
                    0.0,
                    (1.0 * s).max(1.0),
                    if static_focused {
                        [0.60, 0.35, 0.85, 1.0]
                    } else {
                        [1.0, 1.0, 1.0, 0.12]
                    },
                    [0.13, 0.14, 0.18, 1.0],
                );
                ui_registry.register_text_input(
                    crate::ui_system::UiId::ApiMockStaticResponseInput(route_idx),
                    x + pad,
                    cy,
                    content_w,
                    editor_h,
                    mx,
                    my,
                );
                let clip = (
                    x + pad + 10.0 * s,
                    cy + 8.0 * s,
                    content_w - 20.0 * s,
                    editor_h - 16.0 * s,
                );
                let static_scroll_y = tab_state.body_scroll.current.round();
                if self.begin_api_text_clip(clip, tab_clip) {
                    if static_focused {
                        let text_top = api_text_area_top_from_baseline(cy + 29.0 * s, s);
                        self.draw_api_editor_selection_multiline_ui(
                            &ide_panel.api.input_editor,
                            x + pad + 10.0 * s,
                            text_top,
                            content_w - 20.0 * s,
                            editor_h - 16.0 * s,
                            s,
                            static_scroll_y,
                            0.0,
                        );
                    }
                    self.draw_json_text_area(
                        &static_text,
                        x + pad + 10.0 * s,
                        cy + 29.0 * s,
                        content_w - 20.0 * s,
                        editor_h - 16.0 * s,
                        s,
                        static_scroll_y,
                        0.0,
                        false,
                    );
                    if static_focused && blink_alpha > 0.5 {
                        let text_top = api_text_area_top_from_baseline(cy + 29.0 * s, s);
                        self.draw_api_editor_cursor_multiline_ui(
                            &ide_panel.api.input_editor,
                            x + pad + 10.0 * s,
                            text_top,
                            content_w - 20.0 * s,
                            editor_h - 16.0 * s,
                            s,
                            static_scroll_y,
                            0.0,
                        );
                    }
                    self.restore_api_tab_clip(tab_clip);
                }
                self.draw_api_text_scrollbar(
                    &static_text,
                    x + pad + content_w - 8.0 * s,
                    cy + 8.0 * s,
                    editor_h - 16.0 * s,
                    s,
                    static_scroll_y,
                );
                cy += editor_h + 14.0 * s;
            } else {
                let active_script = manual_mock
                    .and_then(|route| route.python.as_ref())
                    .or_else(|| mock_override.and_then(|item| item.python.as_ref()))
                    .filter(|script| script.enabled);
                if let Some(script) = active_script {
                    let contract = api_mock_effective_contract(script, route, model);
                    cy = self.draw_api_mock_contract_controls(
                        x + pad,
                        cy,
                        content_w,
                        s,
                        route_idx,
                        &contract,
                        ide_panel.api.focused.as_ref(),
                        ide_panel.api.mock_contract_constraint_menu,
                        &ide_panel.api.input_editor,
                        ide_panel.api.input_scroll_x.current,
                        blink_alpha,
                        ui_registry,
                        mx,
                        my,
                    );
                    let prelude_focused = matches!(
                        ide_panel.api.focused,
                        Some(ApiFocus::MockPrelude { route_idx: f_route }) if f_route == route_idx
                    );
                    let contract_focused = matches!(
                        ide_panel.api.focused,
                        Some(ApiFocus::MockContract { route_idx: f_route }) if f_route == route_idx
                    );
                    let body_focused = matches!(
                        ide_panel.api.focused,
                        Some(ApiFocus::MockBody { route_idx: f_route }) if f_route == route_idx
                    );
                    let signature_focused = matches!(
                        ide_panel.api.focused,
                        Some(ApiFocus::MockSignature { route_idx: f_route }) if f_route == route_idx
                    );
                    let sections = [
                        (
                            "Подготовка: импорты и состояние",
                            crate::ui_system::UiId::ApiMockPreludeInput(route_idx),
                            crate::ui_system::UiId::ApiMockPreludeReset(route_idx),
                            ApiMockSourcePart::Prelude,
                            prelude_focused,
                            if prelude_focused {
                                ide_panel.api.input_editor.get_full_text()
                            } else {
                                script.prelude.clone()
                            },
                        ),
                        (
                            "Контракт: Query, Body, Response",
                            crate::ui_system::UiId::ApiMockContractInput(route_idx),
                            crate::ui_system::UiId::ApiMockContractReset(route_idx),
                            ApiMockSourcePart::Contract,
                            contract_focused,
                            if contract_focused {
                                ide_panel.api.input_editor.get_full_text()
                            } else {
                                crate::app::api_mock::contract::api_mock_contract_source_text(
                                    script, route, model,
                                )
                            },
                        ),
                        (
                            "Обработчик",
                            crate::ui_system::UiId::ApiMockBodyInput(route_idx),
                            crate::ui_system::UiId::ApiMockBodyReset(route_idx),
                            ApiMockSourcePart::Body,
                            body_focused,
                            if body_focused {
                                ide_panel.api.input_editor.get_full_text()
                            } else {
                                api_mock_body_editor_text(&script.body)
                            },
                        ),
                    ];
                    let header_h = 28.0 * s;
                    let line_gutter_w = 38.0 * s;
                    let body_signature = api_mock_handler_signature_text(&contract);
                    let combined_h =
                        crate::app::api_client::api_mock_combined_editor_content_height(
                            &sections[0].5,
                            &sections[1].5,
                            &body_signature,
                            &sections[2].5,
                            s,
                        );
                    let viewport_h =
                        crate::app::api_client::api_mock_combined_editor_viewport_height(
                            &body_signature,
                            s,
                        );
                    let combined_max_scroll = (combined_h - viewport_h).max(0.0);
                    let combined_scroll_key = (route_idx, ApiMockSourcePart::Body);
                    let combined_scroll_y = ide_panel
                        .api
                        .mock_python_scrolls
                        .get(&combined_scroll_key)
                        .map(|scroll| scroll.current.round())
                        .unwrap_or(0.0)
                        .clamp(0.0, combined_max_scroll);
                    let any_focused =
                        prelude_focused || contract_focused || body_focused || signature_focused;
                    self.push_rounded_rect_border(
                        x + pad,
                        cy,
                        content_w,
                        viewport_h,
                        0.0,
                        (1.0 * s).max(1.0),
                        if any_focused {
                            [0.60, 0.35, 0.85, 1.0]
                        } else {
                            [1.0, 1.0, 1.0, 0.12]
                        },
                        [0.13, 0.14, 0.18, 1.0],
                    );
                    self.draw_api_line_number_gutter(x + pad, cy, line_gutter_w, viewport_h, s);
                    let viewport_clip = (x + pad, cy, content_w, viewport_h);
                    let viewport_visible_clip = api_rect_intersection(viewport_clip, tab_clip);
                    if let Some((vx, vy, vw, vh)) = viewport_visible_clip {
                        ui_registry.register_blocker(
                            crate::ui_system::UiId::ApiMockCombinedPython(route_idx),
                            vx,
                            vy,
                            vw,
                            vh,
                            mx,
                            my,
                        );
                    }
                    let route_ty_diagnostics = if matches!(
                        ide_panel.api.mock.check_status,
                        crate::app::api_mock::types::ApiMockCheckStatus::Failed {
                            route_idx: checked,
                            ..
                        } if checked == route_idx
                    ) {
                        ide_panel.api.mock_ty_diagnostics.as_slice()
                    } else {
                        &[]
                    };
                    let mut mock_ty_popup_drawn = false;
                    if let Some(viewport_visible_clip) = viewport_visible_clip
                        && self.begin_api_text_clip(viewport_visible_clip, tab_clip)
                    {
                        let mouse_in_viewport = mx >= viewport_visible_clip.0
                            && mx <= viewport_visible_clip.0 + viewport_visible_clip.2
                            && my >= viewport_visible_clip.1
                            && my <= viewport_visible_clip.1 + viewport_visible_clip.3;
                        let mut section_y = (cy - combined_scroll_y).round();
                        let mut first_line_no = 1usize;
                        for (label, id, reset_id, part, focused, text) in sections {
                            let locked_text = if part == ApiMockSourcePart::Body {
                                body_signature.clone()
                            } else {
                                String::new()
                            };
                            let locked_line_count = api_mock_locked_text_line_count(&locked_text);
                            let locked_h = api_mock_locked_text_block_height(&locked_text, s);
                            let input_h = (text.split('\n').count().max(1) as f32
                                * api_text_area_line_height(s)
                                + 16.0 * s)
                                .max(112.0 * s);
                            self.push_rect(
                                x + pad + line_gutter_w,
                                section_y,
                                content_w - line_gutter_w,
                                header_h,
                                [1.0, 1.0, 1.0, 0.030],
                            );
                            self.draw_string_scaled_stable(
                                label,
                                x + pad + line_gutter_w + 10.0 * s,
                                api_mock_contract_row_text_y(section_y, header_h, s),
                                [0.68, 0.70, 0.78, 1.0],
                                0.78,
                            );
                            if section_y + header_h >= cy && section_y <= cy + viewport_h {
                                let reset_btn = IconButton {
                                    x: x + pad + content_w - 26.0 * s,
                                    y: section_y + ((header_h - 24.0 * s) * 0.5).round(),
                                    size: 24.0 * s,
                                    icon: Some(IconType::Reload),
                                    is_active: false,
                                    icon_size: Some(16.0 * s),
                                    active_square_width: None,
                                    custom_color: Some([0.76, 0.79, 0.88, 1.0]),
                                };
                                ui_registry.register_icon_button(
                                    reset_id, &reset_btn, self, mx, my, s, false,
                                );
                            }
                            self.push_rect(
                                x + pad,
                                (section_y + header_h).round(),
                                content_w,
                                1.0,
                                [1.0, 1.0, 1.0, 0.08],
                            );
                            let content_y = section_y + header_h;
                            if locked_h > 0.0 {
                                let locked_x = x + pad + line_gutter_w + 10.0 * s;
                                let locked_y = content_y + 8.0 * s;
                                let locked_w = content_w - line_gutter_w - 20.0 * s;
                                if self.begin_api_text_clip(
                                    (x + pad, content_y, line_gutter_w, locked_h),
                                    viewport_visible_clip,
                                ) {
                                    self.draw_api_editor_line_numbers(
                                        &locked_text,
                                        x + pad,
                                        line_gutter_w,
                                        locked_y + api_text_area_baseline_offset(s),
                                        locked_h,
                                        s,
                                        0.0,
                                        first_line_no,
                                    );
                                    self.restore_api_tab_clip(viewport_visible_clip);
                                }
                                if mouse_in_viewport
                                    && locked_y + locked_h >= cy
                                    && locked_y <= cy + viewport_h
                                {
                                    ui_registry.register_text_input(
                                        crate::ui_system::UiId::ApiMockSignatureInput(route_idx),
                                        locked_x,
                                        locked_y,
                                        locked_w,
                                        locked_h,
                                        mx,
                                        my,
                                    );
                                }
                                if signature_focused {
                                    self.draw_api_editor_selection_multiline(
                                        &ide_panel.api.input_editor,
                                        locked_x,
                                        locked_y,
                                        locked_w,
                                        locked_h,
                                        s,
                                        0.0,
                                        0.0,
                                    );
                                }
                                let signature_spans = ide_panel
                                    .api
                                    .mock_highlight_cache
                                    .get(&(route_idx, ApiMockSourcePart::Signature))
                                    .map(Vec::as_slice)
                                    .unwrap_or(&[]);
                                self.draw_api_mock_locked_signature_line(
                                    &locked_text,
                                    locked_h,
                                    signature_spans,
                                    locked_x,
                                    locked_y,
                                    locked_w,
                                    s,
                                );
                                if signature_focused && blink_alpha > 0.5 {
                                    self.draw_api_editor_cursor_multiline(
                                        &ide_panel.api.input_editor,
                                        locked_x,
                                        locked_y,
                                        locked_w,
                                        locked_h,
                                        s,
                                        0.0,
                                        0.0,
                                    );
                                }
                            }
                            let input_y = content_y + locked_h;
                            let input_rect_x = x + pad;
                            let input_rect_w = content_w;
                            if mouse_in_viewport
                                && input_y + input_h >= cy
                                && input_y <= cy + viewport_h
                            {
                                ui_registry.register_text_input(
                                    id,
                                    input_rect_x + line_gutter_w,
                                    input_y,
                                    input_rect_w - line_gutter_w,
                                    input_h,
                                    mx,
                                    my,
                                );
                            }
                            let scroll_key = (route_idx, part);
                            let input_scroll_y = 0.0;
                            let input_scroll_x = ide_panel
                                .api
                                .mock_python_scrolls_x
                                .get(&scroll_key)
                                .map(|scroll| scroll.current.round())
                                .unwrap_or(0.0);
                            if self.begin_api_text_clip(
                                (input_rect_x, input_y, line_gutter_w, input_h),
                                viewport_visible_clip,
                            ) {
                                self.draw_api_editor_line_numbers(
                                    &text,
                                    input_rect_x,
                                    line_gutter_w,
                                    input_y + 29.0 * s,
                                    input_h - 16.0 * s,
                                    s,
                                    input_scroll_y,
                                    first_line_no + locked_line_count,
                                );
                                self.restore_api_tab_clip(viewport_visible_clip);
                            }
                            let clip = (
                                input_rect_x + line_gutter_w + 10.0 * s,
                                input_y + 8.0 * s,
                                input_rect_w - line_gutter_w - 20.0 * s,
                                input_h - 16.0 * s,
                            );
                            if self.begin_api_text_clip(clip, viewport_visible_clip) {
                                let cached_spans =
                                    ide_panel.api.mock_highlight_cache.get(&(route_idx, part));
                                let spans = cached_spans.map(Vec::as_slice).unwrap_or_else(|| {
                                    if ide_panel.api.mock_highlight_target.is_some_and(
                                        |(highlight_route, highlight_part, _)| {
                                            highlight_route == route_idx && highlight_part == part
                                        },
                                    ) {
                                        ide_panel.api.mock_highlight_spans.as_slice()
                                    } else {
                                        &[]
                                    }
                                });
                                let source_editor = if focused {
                                    Some(&ide_panel.api.input_editor)
                                } else {
                                    ide_panel.api.mock_python_editors.get(&scroll_key)
                                };
                                if let Some(source_editor) = source_editor {
                                    self.draw_embedded_python_editor(
                                        source_editor,
                                        spans,
                                        input_rect_x + line_gutter_w + 10.0 * s,
                                        input_y + 29.0 * s,
                                        input_rect_w - line_gutter_w - 20.0 * s,
                                        input_scroll_y,
                                        input_scroll_x,
                                        focused,
                                        blink_alpha,
                                        ui_registry,
                                    );
                                } else {
                                    self.draw_python_text_area(
                                        &text,
                                        spans,
                                        input_rect_x + line_gutter_w + 10.0 * s,
                                        input_y + 29.0 * s,
                                        input_rect_w - line_gutter_w - 20.0 * s,
                                        input_h - 16.0 * s,
                                        s,
                                        input_scroll_y,
                                        input_scroll_x,
                                    );
                                }
                                self.draw_api_mock_ty_squiggles(
                                    &text,
                                    route_ty_diagnostics,
                                    part,
                                    input_rect_x + line_gutter_w + 10.0 * s,
                                    input_y + 29.0 * s,
                                    input_rect_w - line_gutter_w - 20.0 * s,
                                    input_h - 16.0 * s,
                                    s,
                                    input_scroll_y,
                                    input_scroll_x,
                                );
                                self.restore_api_tab_clip(viewport_visible_clip);
                                if part == ApiMockSourcePart::Body {
                                    let signature_source_editor = ide_panel
                                .api
                                .mock_hover_target
                                .as_ref()
                                .filter(|target| {
                                    target.route_idx == route_idx
                                        && target.part == ApiMockSourcePart::Signature
                                })
                                .and_then(|_| {
                                    if matches!(
                                        ide_panel.api.focused,
                                        Some(ApiFocus::MockSignature { route_idx: f_route }) if f_route == route_idx
                                    ) {
                                        Some(&ide_panel.api.input_editor)
                                    } else {
                                        ide_panel
                                            .api
                                            .mock_python_editors
                                            .get(&(route_idx, ApiMockSourcePart::Signature))
                                    }
                                });
                                    if !mock_ty_popup_drawn
                                        && self.draw_existing_api_mock_ty_popup(
                                            signature_source_editor,
                                            route_ty_diagnostics,
                                            input_rect_x + line_gutter_w + 10.0 * s,
                                            content_y + 8.0 * s,
                                            0.0,
                                            0.0,
                                            ide_panel,
                                            ui_registry,
                                            Some(viewport_visible_clip),
                                            mx,
                                            my,
                                        )
                                    {
                                        mock_ty_popup_drawn = true;
                                    }
                                }
                                let text_x = input_rect_x + line_gutter_w + 10.0 * s;
                                let source_top_y =
                                    api_text_area_top_from_baseline(input_y + 29.0 * s, s);
                                let source_editor = ide_panel
                                    .api
                                    .mock_hover_target
                                    .as_ref()
                                    .filter(|target| {
                                        target.route_idx == route_idx && target.part == part
                                    })
                                    .and_then(|_| {
                                        if focused {
                                            Some(&ide_panel.api.input_editor)
                                        } else {
                                            ide_panel.api.mock_python_editors.get(&scroll_key)
                                        }
                                    });
                                if !mock_ty_popup_drawn
                                    && self.draw_existing_api_mock_ty_popup(
                                        source_editor,
                                        route_ty_diagnostics,
                                        text_x,
                                        source_top_y,
                                        input_scroll_y,
                                        input_scroll_x,
                                        ide_panel,
                                        ui_registry,
                                        Some(viewport_visible_clip),
                                        mx,
                                        my,
                                    )
                                {
                                    mock_ty_popup_drawn = true;
                                }
                            }
                            let section_h = header_h + locked_h + input_h;
                            section_y += section_h;
                            first_line_no += locked_line_count + text.split('\n').count().max(1);
                            if section_y < cy + combined_h - combined_scroll_y {
                                self.push_rect(
                                    x + pad,
                                    section_y.round(),
                                    content_w,
                                    1.0,
                                    [1.0, 1.0, 1.0, 0.10],
                                );
                            }
                        }
                        self.restore_api_tab_clip(tab_clip);
                    }
                    if combined_max_scroll > 0.5 {
                        let track_x = x + pad + content_w - 8.0 * s;
                        let track_y = cy + 8.0 * s;
                        let track_h = (viewport_h - 16.0 * s).max(1.0);
                        let track_w = (3.0 * s).max(2.0);
                        self.push_rect(
                            track_x,
                            track_y,
                            track_w,
                            track_h,
                            [0.52, 0.54, 0.60, 0.22],
                        );
                        let thumb_h = (viewport_h / combined_h * track_h)
                            .max(22.0 * s)
                            .min(track_h);
                        let thumb_y = track_y
                            + (combined_scroll_y / combined_max_scroll) * (track_h - thumb_h);
                        self.push_rect(
                            track_x,
                            thumb_y,
                            track_w,
                            thumb_h,
                            [0.64, 0.66, 0.72, 0.70],
                        );
                    }
                    cy += viewport_h + 14.0 * s;
                }
            }
        }
        if mock_expanded {
            let line_w = (1.0 * s).round().max(1.0);
            let frame_x = (x + pad - 10.0 * s).round();
            let frame_y = mock_frame_y.round();
            let frame_w = (content_w + 20.0 * s).round().max(line_w * 2.0);
            let frame_h = (cy - mock_frame_y - 8.0 * s).round().max(line_w * 2.0);
            let frame_color = [
                self.theme.sel[0],
                self.theme.sel[1],
                self.theme.sel[2],
                0.55,
            ];
            self.push_rect(frame_x, frame_y, frame_w, line_w, frame_color);
            self.push_rect(frame_x, frame_y, line_w, frame_h, frame_color);
            self.push_rect(
                frame_x + frame_w - line_w,
                frame_y,
                line_w,
                frame_h,
                frame_color,
            );
            self.push_rect(
                frame_x,
                frame_y + frame_h - line_w,
                frame_w,
                line_w,
                frame_color,
            );
        }

        let auth_scheme_indices = api_route_auth_scheme_indices(model, route);
        if !auth_scheme_indices.is_empty() {
            self.draw_api_section_title("Авторизация", x + pad, cy + 18.0 * s, s);
            if api_route_auth_missing(model, route, &ide_panel.api.auth) {
                let title_w = self.measure_ui_width("Авторизация", API_SECTION_TITLE_SCALE);
                self.draw_string_scaled_stable(
                    "Не авторизовано",
                    x + pad + title_w + 16.0 * s,
                    cy + 18.0 * s,
                    [1.0, 0.42, 0.42, 1.0],
                    0.86,
                );
            }
            cy += 28.0 * s;
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
            cy += 8.0 * s;
        }

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
        cy += 40.0 * s;

        if tab_state.input_doc_view == ApiInputDocView::Schema {
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
            let selected_media_count = if mock_input_contract.is_some() {
                1
            } else {
                api_route_input_media_count(route).max(1)
            };
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
                    [0.18, 0.19, 0.23, 1.0],
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
                    [0.78, 0.80, 0.88, 1.0],
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
                    [0.78, 0.80, 0.88, 1.0],
                    0.82,
                );
            }
            cy += 28.0 * s;
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
                            [1.0, 1.0, 1.0, 0.12]
                        } else {
                            [0.15, 0.16, 0.20, 1.0]
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
                        self.theme.fg,
                        0.80,
                    );
                }
                cy += selected_media_count as f32 * 30.0 * s + 4.0 * s;
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
                    [0.60, 0.35, 0.85, 1.0]
                } else {
                    [1.0, 1.0, 1.0, 0.12]
                },
                [0.12, 0.13, 0.17, 1.0],
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
                );
                self.restore_api_tab_clip(tab_clip);
            }
            cy += schema_h + 16.0 * s;
        } else {
            if !route.path_params.is_empty() {
                self.draw_api_section_title("Параметры пути", x + pad, cy + 18.0 * s, s);
                cy += 28.0 * s;
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
                cy += 8.0 * s;
            }

            if !route.query_params.is_empty() {
                self.draw_api_section_title("Параметры query", x + pad, cy + 18.0 * s, s);
                cy += 28.0 * s;
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
                cy += 8.0 * s;
            }

            if let Some(body) = &route.request_body {
                self.draw_api_section_title("Body", x + pad, cy + 18.0 * s, s);
                self.draw_string_scaled_stable(
                    &body.content_type,
                    x + pad + 52.0 * s,
                    cy + 18.0 * s,
                    [0.35, 0.75, 1.0, 1.0],
                    0.84,
                );
                let body_focused = matches!(
                    ide_panel.api.focused,
                    Some(ApiFocus::Body { spec_id, route_idx: f_route })
                        if spec_id == tab_meta.spec_id && f_route == route_idx
                );
                let validates_json = !body.is_multipart
                    && !body.is_form_urlencoded
                    && body.content_type.to_ascii_lowercase().contains("json");
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
                            [0.48, 0.86, 0.52, 1.0]
                        } else {
                            [1.0, 0.42, 0.42, 1.0]
                        },
                        0.92,
                    );
                }
                cy += 28.0 * s;
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
                        cy += 16.0 * s;
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
                            [0.60, 0.35, 0.85, 1.0]
                        } else {
                            [1.0, 1.0, 1.0, 0.12]
                        },
                        [0.13, 0.14, 0.18, 1.0],
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
                    cy += body_h + 16.0 * s;
                }
            }
        }

        if !route.responses.is_empty() {
            self.draw_api_section_title("Output", x + pad, cy + 18.0 * s, s);
            cy += 28.0 * s;
            let mut status_x = x + pad;
            let selected_status_idx = tab_state
                .output_status_idx
                .min(route.responses.len().saturating_sub(1));
            for (response_idx, response) in route.responses.iter().enumerate() {
                let label = response.status.as_str();
                let chip_w = (self.measure_ui_width(label, 0.86) + 22.0 * s).max(54.0 * s);
                if status_x + chip_w > x + pad + content_w {
                    status_x = x + pad;
                    cy += 34.0 * s;
                }
                self.draw_api_response_tab(
                    label,
                    response_idx == selected_status_idx,
                    status_x,
                    cy,
                    chip_w,
                    28.0 * s,
                    s,
                    crate::ui_system::UiId::ApiOutputStatusTab(route_idx, response_idx),
                    ui_registry,
                    mx,
                    my,
                );
                status_x += chip_w + 8.0 * s;
            }
            cy += 38.0 * s;
            let output_tab_y = cy;
            let output_tab_h = 28.0 * s;
            let example_w = self.measure_ui_width("Example", 0.86) + 22.0 * s;
            let schema_w = self.measure_ui_width("Schema", 0.86) + 22.0 * s;
            let example_count = api_route_output_example_count(route, selected_status_idx).max(1);
            let schema_media_count =
                api_route_output_media_count(route, selected_status_idx).max(1);
            let selected_example_idx = tab_state
                .output_example_idx
                .min(example_count.saturating_sub(1));
            let selected_schema_media_idx = tab_state
                .output_schema_idx
                .min(schema_media_count.saturating_sub(1));
            let show_example_label = tab_state.output_doc_view == ApiOutputDocView::Example;
            let show_example_menu = show_example_label && example_count > 1;
            let menu_label = api_route_output_example_menu_label(
                route,
                selected_status_idx,
                selected_example_idx,
            );
            let mut menu_label_w = self.measure_ui_width(&menu_label, 0.82);
            if show_example_menu {
                for option_idx in 0..example_count {
                    let label =
                        api_route_output_example_menu_label(route, selected_status_idx, option_idx);
                    menu_label_w = menu_label_w.max(self.measure_ui_width(&label, 0.82));
                }
            }
            let menu_extra_w = if show_example_menu {
                48.0 * s
            } else {
                20.0 * s
            };
            let menu_w = (menu_label_w + menu_extra_w).clamp(132.0 * s, content_w);
            let tab_x = x + pad;
            self.draw_api_response_tab(
                "Example",
                tab_state.output_doc_view == ApiOutputDocView::Example,
                tab_x,
                output_tab_y,
                example_w,
                output_tab_h,
                s,
                crate::ui_system::UiId::ApiOutputExampleTab(route_idx),
                ui_registry,
                mx,
                my,
            );
            self.draw_api_response_tab(
                "Schema",
                tab_state.output_doc_view == ApiOutputDocView::Schema,
                tab_x + example_w + 8.0 * s,
                output_tab_y,
                schema_w,
                output_tab_h,
                s,
                crate::ui_system::UiId::ApiOutputSchemaTab(route_idx),
                ui_registry,
                mx,
                my,
            );
            cy += 34.0 * s;
            let output_menu_y = cy;
            if show_example_label {
                self.push_rounded_rect(
                    x + pad,
                    output_menu_y,
                    menu_w,
                    output_tab_h,
                    5.0 * s,
                    [0.18, 0.19, 0.23, 1.0],
                );
                if show_example_menu {
                    ui_registry.register_blocker(
                        crate::ui_system::UiId::ApiOutputSchemaMenu(route_idx),
                        x + pad,
                        output_menu_y,
                        menu_w,
                        output_tab_h,
                        mx,
                        my,
                    );
                }
                self.draw_string_scaled_stable(
                    &menu_label,
                    x + pad + 10.0 * s,
                    api_centered_text_y(output_menu_y, output_tab_h, s),
                    [0.78, 0.80, 0.88, 1.0],
                    0.82,
                );
                if show_example_menu {
                    self.draw_string_scaled_stable(
                        if tab_state.output_schema_menu_open {
                            "▼"
                        } else {
                            "▶"
                        },
                        x + pad + menu_w - 18.0 * s,
                        api_centered_text_y(output_menu_y, output_tab_h, s),
                        [0.78, 0.80, 0.88, 1.0],
                        0.82,
                    );
                }
            }
            let output_example_text = api_route_output_example_text_for(
                route,
                model,
                selected_status_idx,
                selected_example_idx,
            );
            let output_schema_text = api_route_output_schema_text_for(
                route,
                model,
                selected_status_idx,
                selected_schema_media_idx,
                &tab_state.output_schema_collapsed,
            );
            let output_h = api_response_text_area_height(&output_example_text, s)
                .max(api_response_text_area_height(&output_schema_text, s));
            let output_text = match tab_state.output_doc_view {
                ApiOutputDocView::Example => output_example_text,
                ApiOutputDocView::Schema => output_schema_text,
            };
            let output_summary = if tab_state.output_doc_view == ApiOutputDocView::Schema {
                api_route_output_schema_summary(
                    route,
                    model,
                    selected_status_idx,
                    selected_schema_media_idx,
                )
            } else {
                String::new()
            };
            if tab_state.output_doc_view == ApiOutputDocView::Schema && !output_summary.is_empty() {
                self.draw_api_schema_summary(&output_summary, x + pad, cy + 18.0 * s);
            }
            cy += 40.0 * s;
            let output_schema_focused = tab_state.focused_schema_pane
                == Some(crate::app::api_client::ApiSchemaPaneFocus::Output);
            self.push_rounded_rect_border(
                x + pad,
                cy,
                content_w,
                output_h,
                0.0,
                (1.0 * s).max(1.0),
                if output_schema_focused {
                    [0.60, 0.35, 0.85, 1.0]
                } else {
                    [1.0, 1.0, 1.0, 0.12]
                },
                [0.12, 0.13, 0.17, 1.0],
            );
            ui_registry.register_text_input(
                crate::ui_system::UiId::ApiOutputSchemaBody(route_idx),
                x + pad,
                cy,
                content_w,
                output_h,
                mx,
                my,
            );
            let output_clip = (
                x + pad + 10.0 * s,
                cy + 8.0 * s,
                content_w - 20.0 * s,
                output_h - 16.0 * s,
            );
            if self.begin_api_text_clip(output_clip, tab_clip) {
                let output_schema_text_focused = matches!(
                    ide_panel.api.focused,
                    Some(ApiFocus::OutputSchema { spec_id, route_idx: f_route })
                        if spec_id == tab_meta.spec_id && f_route == route_idx
                );
                if output_schema_text_focused {
                    let output_text_top = api_text_area_top_from_baseline(cy + 29.0 * s, s);
                    self.draw_api_editor_selection_multiline_ui(
                        &ide_panel.api.input_editor,
                        x + pad + 10.0 * s,
                        output_text_top,
                        content_w - 20.0 * s,
                        output_h - 16.0 * s,
                        s,
                        tab_state.response_scroll.current.round(),
                        0.0,
                    );
                }
                if tab_state.output_doc_view == ApiOutputDocView::Schema {
                    self.draw_api_schema_text_area(
                        &output_text,
                        x + pad + 10.0 * s,
                        cy + 29.0 * s,
                        content_w - 20.0 * s,
                        output_h - 16.0 * s,
                        s,
                        tab_state.response_scroll.current.round(),
                        0.0,
                        false,
                        false,
                        route_idx,
                        ui_registry,
                        mx,
                        my,
                    );
                } else {
                    self.draw_json_text_area(
                        &output_text,
                        x + pad + 10.0 * s,
                        cy + 29.0 * s,
                        content_w - 20.0 * s,
                        output_h - 16.0 * s,
                        s,
                        tab_state.response_scroll.current.round(),
                        0.0,
                        false,
                    );
                }
                if output_schema_text_focused && blink_alpha > 0.5 {
                    let output_text_top = api_text_area_top_from_baseline(cy + 29.0 * s, s);
                    self.draw_api_editor_cursor_multiline_ui(
                        &ide_panel.api.input_editor,
                        x + pad + 10.0 * s,
                        output_text_top,
                        content_w - 20.0 * s,
                        output_h - 16.0 * s,
                        s,
                        tab_state.response_scroll.current.round(),
                        0.0,
                    );
                }
                if tab_state.output_doc_view == ApiOutputDocView::Schema {
                    self.draw_api_schema_scrollbar(
                        &output_text,
                        x + pad + content_w - 15.0 * s,
                        cy + 8.0 * s,
                        content_w - 20.0 * s,
                        output_h - 16.0 * s,
                        s,
                        tab_state.response_scroll.current.round(),
                    );
                } else {
                    self.draw_api_text_scrollbar(
                        &output_text,
                        x + pad + content_w - 15.0 * s,
                        cy + 8.0 * s,
                        output_h - 16.0 * s,
                        s,
                        tab_state.response_scroll.current.round(),
                    );
                }
                self.restore_api_tab_clip(tab_clip);
            }
            if show_example_menu && tab_state.output_schema_menu_anim > 0.01 {
                let menu_x = x + pad;
                let row_h = 30.0 * s;
                let row_inset = 5.0 * s;
                let row_top_pad = 6.0 * s;
                let row_bottom_pad = 4.0 * s;
                let track_w = (4.0 * s).max(3.0);
                let track_gap = 8.0 * s;
                let max_menu_h = row_top_pad + row_h * 6.0 + row_bottom_pad;
                let menu_content_h = row_top_pad + example_count as f32 * row_h + row_bottom_pad;
                let menu_h = menu_content_h.min(max_menu_h);
                let anim_h = (menu_h * tab_state.output_schema_menu_anim)
                    .round()
                    .max(1.0);
                let popup_y = output_menu_y + output_tab_h + 6.0 * s;
                let max_scroll = (menu_content_h - menu_h).max(0.0);
                let scrollbar_visible = max_scroll > 0.5;
                let list_scrolling = tab_state.output_schema_menu_scroll.is_dragging
                    || (tab_state.output_schema_menu_scroll.current
                        - tab_state.output_schema_menu_scroll.target)
                        .abs()
                        >= 0.5;
                for i in 1..=5 {
                    let offset = i as f32 * s;
                    let alpha = (0.15 - (i as f32 * 0.03))
                        * tab_state.output_schema_menu_anim.clamp(0.0, 1.0);
                    self.push_rounded_rect(
                        menu_x - offset,
                        popup_y - offset,
                        menu_w + offset * 2.0,
                        anim_h + offset * 2.0,
                        6.0 * s,
                        [0.0, 0.0, 0.0, alpha],
                    );
                }
                self.push_rounded_rect_border(
                    menu_x - 2.0 * s,
                    popup_y - 2.0 * s,
                    menu_w + 4.0 * s,
                    anim_h + 4.0 * s,
                    6.0 * s,
                    (2.0 * s).max(1.0),
                    [self.theme.sel[0], self.theme.sel[1], self.theme.sel[2], 1.0],
                    [0.15, 0.16, 0.20, 1.0],
                );
                ui_registry.register_blocker(
                    crate::ui_system::UiId::ApiOutputSchemaMenu(route_idx),
                    menu_x,
                    popup_y,
                    menu_w,
                    anim_h,
                    mx,
                    my,
                );
                let clip_top_pad = (4.0 * s).max(3.0);
                let clip_bottom_pad = clip_top_pad;
                let menu_clip = (
                    menu_x,
                    popup_y + clip_top_pad,
                    menu_w,
                    (anim_h - clip_top_pad - clip_bottom_pad).max(1.0),
                );
                if self.begin_api_text_clip(menu_clip, tab_clip) {
                    let scroll_y = tab_state.output_schema_menu_scroll.current.round();
                    let first = ((scroll_y - row_top_pad).max(0.0) / row_h).floor() as usize;
                    let max_visible = (menu_h / row_h).ceil() as usize + 1;
                    for option_idx in first..example_count.min(first + max_visible) {
                        let item_y = popup_y + row_top_pad + option_idx as f32 * row_h - scroll_y;
                        if item_y >= popup_y + anim_h || item_y + 28.0 * s <= popup_y {
                            continue;
                        }
                        let label = api_route_output_example_menu_label(
                            route,
                            selected_status_idx,
                            option_idx,
                        );
                        let row_x = menu_x + row_inset;
                        let gutter_w = if scrollbar_visible {
                            track_w + track_gap
                        } else {
                            0.0
                        };
                        let row_w = (menu_w - 2.0 * row_inset - gutter_w).max(0.0);
                        let hovered = !list_scrolling
                            && mx >= row_x
                            && mx <= row_x + row_w
                            && my >= item_y
                            && my <= item_y + 28.0 * s;
                        self.push_rounded_rect(
                            row_x,
                            item_y,
                            row_w,
                            28.0 * s,
                            4.0 * s,
                            if option_idx == selected_example_idx {
                                [1.0, 1.0, 1.0, 0.12]
                            } else if hovered {
                                [0.20, 0.21, 0.28, 1.0]
                            } else {
                                [0.15, 0.16, 0.20, 1.0]
                            },
                        );
                        ui_registry.register_rect(
                            crate::ui_system::UiId::ApiOutputSchemaMenuItem(route_idx, option_idx),
                            row_x,
                            item_y,
                            row_w,
                            28.0 * s,
                            mx,
                            my,
                        );
                        self.draw_string_scaled_stable(
                            &label,
                            row_x + 10.0 * s,
                            api_centered_text_y(item_y, 28.0 * s, s),
                            self.theme.fg,
                            0.80,
                        );
                    }
                    if scrollbar_visible {
                        let track_x = menu_x + menu_w - track_w - 4.0 * s;
                        let track_y = popup_y + clip_top_pad;
                        let track_h = (anim_h - clip_top_pad - clip_bottom_pad).max(1.0);
                        self.push_rect(
                            track_x,
                            track_y,
                            track_w,
                            track_h,
                            [0.52, 0.54, 0.60, 0.36],
                        );
                        let content_h = menu_h + max_scroll;
                        let thumb_h = (menu_h / content_h * track_h).max(22.0 * s).min(track_h);
                        let thumb_y = track_y
                            + (scroll_y.clamp(0.0, max_scroll) / max_scroll) * (track_h - thumb_h);
                        self.push_rect(
                            track_x,
                            thumb_y,
                            track_w,
                            thumb_h,
                            [0.70, 0.72, 0.80, 0.88],
                        );
                    }
                    self.restore_api_tab_clip(tab_clip);
                }
            }
            cy += output_h + 18.0 * s;
        }

        if model.servers.len() > 1 {
            self.draw_api_section_title("Сервер", x + pad, cy + 18.0 * s, s);
            cy += 28.0 * s;
            let mut sx = x + pad;
            for (idx, server) in model.servers.iter().enumerate() {
                let label = server.url.as_str();
                let server_text_scale = 0.92;
                let chip_w = (self.measure_ui_width(label, server_text_scale) + 20.0 * s)
                    .max(72.0 * s)
                    .min(content_w);
                if sx + chip_w > x + pad + content_w {
                    sx = x + pad;
                    cy += 34.0 * s;
                }
                let active = idx == tab_state.server_idx;
                self.push_rounded_rect(
                    sx,
                    cy,
                    chip_w,
                    32.0 * s,
                    5.0 * s,
                    if active {
                        [0.35, 0.26, 0.48, 1.0]
                    } else {
                        [0.18, 0.19, 0.23, 1.0]
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
                    self.theme.fg,
                    server_text_scale,
                );
                sx += chip_w + 8.0 * s;
            }
            cy += 42.0 * s;
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
        cy += 54.0 * s;

        self.draw_api_section_title("Ответ", x + pad, cy + 18.0 * s, s);
        cy += 30.0 * s;
        if let Some(response) = &tab_state.response {
            if let Some(err) = &response.error {
                self.draw_string_scaled_stable(
                    &err.message,
                    x + pad,
                    cy + 18.0 * s,
                    [1.0, 0.42, 0.42, 1.0],
                    0.88,
                );
                cy += 28.0 * s;
            }
            if response.error.is_none() || !response.body.is_empty() {
                let status_text = response
                    .status
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "-".to_string());
                self.draw_string_scaled_stable(
                    &status_text,
                    x + pad,
                    cy + 18.0 * s,
                    api_status_color(response.status),
                    0.92,
                );
                self.draw_string_scaled_stable(
                    &response.timing_text,
                    x + pad + 62.0 * s,
                    cy + 18.0 * s,
                    [0.68, 0.70, 0.78, 1.0],
                    0.88,
                );
                cy += 28.0 * s;
                let tab_y = cy;
                let tab_h = 28.0 * s;
                let body_w = self.measure_ui_width("Body", 0.86) + 22.0 * s;
                let headers_w = self.measure_ui_width("Headers", 0.86) + 22.0 * s;
                self.draw_api_response_tab(
                    "Body",
                    tab_state.response_view == ApiResponseView::Body,
                    x + pad,
                    tab_y,
                    body_w,
                    tab_h,
                    s,
                    crate::ui_system::UiId::ApiResponseBodyTab(route_idx),
                    ui_registry,
                    mx,
                    my,
                );
                self.draw_api_response_tab(
                    "Headers",
                    tab_state.response_view == ApiResponseView::Headers,
                    x + pad + body_w + 8.0 * s,
                    tab_y,
                    headers_w,
                    tab_h,
                    s,
                    crate::ui_system::UiId::ApiResponseHeadersTab(route_idx),
                    ui_registry,
                    mx,
                    my,
                );
                cy += 34.0 * s;
                let (has_access, has_refresh) = response_auth_token_flags(response);
                if has_access || has_refresh {
                    let row_h = 30.0 * s;
                    let btn_h = 24.0 * s;
                    let access_w = self.measure_ui_width("Сохранить access", 0.78) + 18.0 * s;
                    let refresh_w = self.measure_ui_width("Сохранить refresh", 0.78) + 18.0 * s;
                    for (scheme_idx, scheme) in model
                        .security_schemes
                        .iter()
                        .enumerate()
                        .filter(|(_, scheme)| scheme.token_capable())
                    {
                        self.draw_string_scaled_stable(
                            &scheme.name,
                            x + pad,
                            cy + 20.0 * s,
                            [0.68, 0.70, 0.78, 1.0],
                            0.78,
                        );
                        let mut bx = x + pad + (content_w * 0.34).max(130.0 * s);
                        if has_access {
                            let use_btn = Button {
                                x: bx,
                                y: cy + 2.0 * s,
                                w: access_w,
                                h: btn_h,
                                text: "Сохранить access".to_string(),
                                icon: None,
                                text_scale: 0.84,
                                icon_size: 0.0,
                            };
                            ui_registry.register_button(
                                crate::ui_system::UiId::ApiResponseUseAccessToken(
                                    route_idx, scheme_idx,
                                ),
                                &use_btn,
                                self,
                                mx,
                                my,
                                s,
                                false,
                            );
                            bx += access_w + 8.0 * s;
                        }
                        if has_refresh {
                            let save_btn = Button {
                                x: bx,
                                y: cy + 2.0 * s,
                                w: refresh_w,
                                h: btn_h,
                                text: "Сохранить refresh".to_string(),
                                icon: None,
                                text_scale: 0.84,
                                icon_size: 0.0,
                            };
                            ui_registry.register_button(
                                crate::ui_system::UiId::ApiResponseSaveRefreshToken(
                                    route_idx, scheme_idx,
                                ),
                                &save_btn,
                                self,
                                mx,
                                my,
                                s,
                                false,
                            );
                        }
                        cy += row_h;
                    }
                }
                let response_focused = matches!(
                    ide_panel.api.focused,
                    Some(ApiFocus::Response { spec_id, route_idx: f_route })
                        if spec_id == tab_meta.spec_id && f_route == route_idx
                );
                let response_text = if response_focused {
                    ide_panel.api.input_editor.get_full_text()
                } else {
                    api_response_text(response, tab_state.response_view).to_string()
                };
                let resp_h = api_response_text_area_height(&response_text, s);
                self.push_rounded_rect_border(
                    x + pad,
                    cy,
                    content_w,
                    resp_h,
                    0.0,
                    (1.0 * s).max(1.0),
                    if response_focused {
                        [0.60, 0.35, 0.85, 1.0]
                    } else {
                        [1.0, 1.0, 1.0, 0.12]
                    },
                    [0.12, 0.13, 0.17, 1.0],
                );
                ui_registry.register_text_input(
                    crate::ui_system::UiId::ApiResponseBody(route_idx),
                    x + pad,
                    cy,
                    content_w,
                    resp_h,
                    mx,
                    my,
                );
                let resp_clip = (
                    x + pad + 10.0 * s,
                    cy + 8.0 * s,
                    content_w - 20.0 * s,
                    resp_h - 16.0 * s,
                );
                if self.begin_api_text_clip(resp_clip, tab_clip) {
                    if response_focused {
                        let text_top = api_text_area_top_from_baseline(cy + 29.0 * s, s);
                        self.draw_api_editor_selection_multiline_ui(
                            &ide_panel.api.input_editor,
                            x + pad + 10.0 * s,
                            text_top,
                            content_w - 20.0 * s,
                            resp_h - 16.0 * s,
                            s,
                            tab_state.response_scroll.current,
                            tab_state.response_scroll_x.current,
                        );
                    }
                    self.draw_json_text_area(
                        &response_text,
                        x + pad + 10.0 * s,
                        cy + 29.0 * s,
                        content_w - 20.0 * s,
                        resp_h - 16.0 * s,
                        s,
                        tab_state.response_scroll.current,
                        tab_state.response_scroll_x.current,
                        tab_state.response_view == ApiResponseView::Headers,
                    );
                    self.draw_api_text_scrollbar(
                        &response_text,
                        x + pad + content_w - 8.0 * s,
                        cy + 8.0 * s,
                        resp_h - 16.0 * s,
                        s,
                        tab_state.response_scroll.current,
                    );
                    if response_focused && blink_alpha > 0.5 {
                        let text_top = api_text_area_top_from_baseline(cy + 29.0 * s, s);
                        self.draw_api_editor_cursor_multiline_ui(
                            &ide_panel.api.input_editor,
                            x + pad + 10.0 * s,
                            text_top,
                            content_w - 20.0 * s,
                            resp_h - 16.0 * s,
                            s,
                            tab_state.response_scroll.current,
                            tab_state.response_scroll_x.current,
                        );
                    }
                    self.restore_api_tab_clip(tab_clip);
                    if response_focused {
                        self.draw_api_text_scrollbar_x(
                            &response_text,
                            x + pad + 8.0 * s,
                            cy + resp_h - 12.0 * s,
                            content_w - 16.0 * s,
                            content_w - 20.0 * s,
                            tab_state.response_scroll_x.current,
                            crate::ui_system::UiId::ApiResponseScrollX(route_idx),
                            ui_registry,
                            mx,
                            my,
                        );
                    }
                }
                if response.truncated {
                    self.draw_string_scaled_stable(
                        "обрезано",
                        x + pad + content_w - 86.0 * s,
                        cy + 18.0 * s,
                        [1.0, 0.76, 0.32, 1.0],
                        0.78,
                    );
                }
            }
        } else if tab_state.pending {
            self.draw_string_scaled_stable(
                "Запрос выполняется",
                x + pad,
                cy + 18.0 * s,
                [0.68, 0.70, 0.78, 1.0],
                0.88,
            );
        }

        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }
    }
}
