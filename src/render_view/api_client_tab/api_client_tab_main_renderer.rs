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
        editor: &crate::editor::Editor,
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
        if let Some(crate::app::api_client::ApiClientRouteIdentity::Manual { stable_id }) =
            &tab_meta.route_identity
        {
            self.draw_api_manual_client_tab(
                x,
                y,
                w,
                h,
                s,
                ide_panel,
                tab_meta,
                tab_state,
                stable_id,
                ui_registry,
                mx,
                my,
                blink_alpha,
            );
            return;
        }
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

            self.draw_string_scaled_stable("Авторизация", x + pad, cy + 24.0 * s, self.theme.fg, 1.18);
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
        let Some(route) = model.routes.get(route_idx) else {
            return;
        };
        let mock_override = ide_panel
            .api
            .mock
            .route_overrides
            .iter()
            .find(|item| item.method == route.method && item.path == route.path);
        let mock_enabled = mock_override.is_some_and(|item| item.enabled);

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
            cy += 30.0 * s;
        }

        self.draw_api_section_title("Мок", x + pad, cy + 18.0 * s, s);
        if mock_enabled {
            self.draw_string_scaled_stable(
                "(Включен)",
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
            w: 220.0 * s,
            h: 34.0 * s,
            text: if mock_expanded {
                "Скрыть настройки мока"
            } else {
                "Настроить мок"
            }
            .to_string(),
            icon: Some(IconType::Api),
            text_scale: 0.88,
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
        if mock_expanded {
        let python_enabled = mock_override
            .and_then(|item| item.python.as_ref())
            .is_some_and(|script| script.enabled);
        let btn_h = 30.0 * s;
        let enable_btn = Button {
            x: x + pad,
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
        let python_btn = Button {
            x: x + pad + 140.0 * s,
            y: cy,
            w: 138.0 * s,
            h: btn_h,
            text: if python_enabled {
                "Python вкл"
            } else {
                "Python выкл"
            }
            .to_string(),
            icon: Some(IconType::Api),
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
                mock_override
                    .map(|item| match &item.response {
                        crate::app::api_mock::types::ApiMockResponse::Generated => {
                            generated.clone()
                        }
                        crate::app::api_mock::types::ApiMockResponse::Json(text)
                        | crate::app::api_mock::types::ApiMockResponse::Text(text) => text.clone(),
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
            let editor_h = 150.0 * s;
            self.push_rounded_rect_border(
                x + pad,
                cy,
                content_w,
                editor_h,
                6.0 * s,
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
                    self.draw_api_editor_selection_multiline(
                        &ide_panel.api.input_editor,
                        x + pad + 10.0 * s,
                        cy + 10.0 * s,
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
                    self.draw_api_editor_cursor_multiline(
                        &ide_panel.api.input_editor,
                        x + pad + 10.0 * s,
                        cy + 10.0 * s,
                        content_w - 20.0 * s,
                        editor_h - 16.0 * s,
                        s,
                        static_scroll_y,
                        0.0,
                    );
                }
                self.restore_api_tab_clip(tab_clip);
            }
            cy += editor_h + 14.0 * s;
        } else {
            if let Some(script) = mock_override.and_then(|item| item.python.as_ref()).filter(|script| script.enabled) {
                let mut mock_ty_popup_drawn = false;
                for (label, id, part, focused, text) in [
                    (
                        "Подготовка: импорты и состояние",
                        crate::ui_system::UiId::ApiMockPreludeInput(route_idx),
                        ApiMockSourcePart::Prelude,
                        matches!(
                            ide_panel.api.focused,
                            Some(ApiFocus::MockPrelude { route_idx: f_route }) if f_route == route_idx
                        ),
                        if matches!(
                            ide_panel.api.focused,
                            Some(ApiFocus::MockPrelude { route_idx: f_route }) if f_route == route_idx
                        ) {
                            ide_panel.api.input_editor.get_full_text()
                        } else {
                            script.prelude.clone()
                        },
                    ),
                    (
                        "Обработчик",
                        crate::ui_system::UiId::ApiMockBodyInput(route_idx),
                        ApiMockSourcePart::Body,
                        matches!(
                            ide_panel.api.focused,
                            Some(ApiFocus::MockBody { route_idx: f_route }) if f_route == route_idx
                        ),
                        if matches!(
                            ide_panel.api.focused,
                            Some(ApiFocus::MockBody { route_idx: f_route }) if f_route == route_idx
                        ) {
                            ide_panel.api.input_editor.get_full_text()
                        } else {
                            api_mock_body_editor_text(&script.body)
                        },
                    ),
                ] {
                    self.draw_string_scaled_stable(
                        label,
                        x + pad,
                        cy + 16.0 * s,
                        [0.68, 0.70, 0.78, 1.0],
                        0.78,
                    );
                    let reset_id = match part {
                        ApiMockSourcePart::Prelude => {
                            crate::ui_system::UiId::ApiMockPreludeReset(route_idx)
                        }
                        ApiMockSourcePart::Body => {
                            crate::ui_system::UiId::ApiMockBodyReset(route_idx)
                        }
                        ApiMockSourcePart::Signature => {
                            crate::ui_system::UiId::ApiMockBodyReset(route_idx)
                        }
                    };
                    let reset_btn = IconButton {
                        x: x + pad + content_w - 26.0 * s,
                        y: cy - 2.0 * s,
                        size: 24.0 * s,
                        icon: Some(IconType::Reload),
                        is_active: false,
                        icon_size: Some(16.0 * s),
                        active_square_width: None,
                        custom_color: Some([0.76, 0.79, 0.88, 1.0]),
                    };
                    ui_registry.register_icon_button(
                        reset_id,
                        &reset_btn,
                        self,
                        mx,
                        my,
                        s,
                        false,
                    );
                    cy += 22.0 * s;
                    let locked_h = if part == ApiMockSourcePart::Body {
                        api_mock_signature_block_height(&route.path, s)
                    } else {
                        0.0
                    };
                    let input_h = if part == ApiMockSourcePart::Body {
                        112.0 * s + 3.0 * api_text_area_line_height(s)
                    } else {
                        112.0 * s
                    };
                    let editor_h = if part == ApiMockSourcePart::Body {
                        locked_h + input_h
                    } else {
                        input_h
                    };
                    let line_gutter_w = 38.0 * s;
                    self.push_rounded_rect_border(
                        x + pad,
                        cy,
                        content_w,
                        editor_h,
                        6.0 * s,
                        (1.0 * s).max(1.0),
                        if focused {
                            [0.60, 0.35, 0.85, 1.0]
                        } else {
                            [1.0, 1.0, 1.0, 0.12]
                        },
                        [0.13, 0.14, 0.18, 1.0],
                    );
                    if part == ApiMockSourcePart::Body {
                        let signature_focused = matches!(
                            ide_panel.api.focused,
                            Some(ApiFocus::MockSignature { route_idx: f_route }) if f_route == route_idx
                        );
                        let sig_x = x + pad + line_gutter_w + 10.0 * s;
                        let sig_y = cy + 8.0 * s;
                        let sig_w = content_w - line_gutter_w - 20.0 * s;
                        let sig_h = api_mock_signature_block_height(&route.path, s);
                        let signature_text = api_mock_signature_text(&route.path);
                        self.draw_api_line_number_gutter(
                            x + pad,
                            cy,
                            line_gutter_w,
                            locked_h,
                            s,
                        );
                        if self.begin_api_text_clip((x + pad, cy, line_gutter_w, locked_h), tab_clip)
                        {
                            self.draw_api_editor_line_numbers(
                                &signature_text,
                                x + pad,
                                line_gutter_w,
                                sig_y + (api_text_area_line_height(s) * 0.75).round(),
                                sig_h,
                                s,
                                0.0,
                                1,
                            );
                            self.restore_api_tab_clip(tab_clip);
                        }
                        ui_registry.register_text_input(
                            crate::ui_system::UiId::ApiMockSignatureInput(route_idx),
                            sig_x,
                            sig_y,
                            sig_w,
                            sig_h,
                            mx,
                            my,
                        );
                        if signature_focused {
                            self.draw_api_editor_selection_multiline(
                                &ide_panel.api.input_editor,
                                sig_x,
                                sig_y,
                                sig_w,
                                sig_h,
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
                            &route.path,
                            signature_spans,
                            sig_x,
                            sig_y,
                            sig_w,
                            s,
                        );
                        if signature_focused && blink_alpha > 0.5 {
                            self.draw_api_editor_cursor_multiline(
                                &ide_panel.api.input_editor,
                                sig_x,
                                sig_y,
                                sig_w,
                                sig_h,
                                s,
                                0.0,
                                0.0,
                            );
                        }
                    }
                    let input_y = cy + locked_h;
                    let input_rect_x = x + pad;
                    let input_rect_w = content_w;
                    ui_registry.register_text_input(
                        id,
                        input_rect_x + line_gutter_w,
                        input_y,
                        input_rect_w - line_gutter_w,
                        input_h,
                        mx,
                        my,
                    );
                    self.draw_api_line_number_gutter(
                        input_rect_x,
                        input_y,
                        line_gutter_w,
                        input_h,
                        s,
                    );
                    let scroll_key = (route_idx, part);
                    let input_scroll_y = ide_panel
                        .api
                        .mock_python_scrolls
                        .get(&scroll_key)
                        .map(|scroll| scroll.current.round())
                        .unwrap_or(0.0)
                        .clamp(
                            0.0,
                            crate::app::api_client::api_text_area_max_scroll(
                                &text,
                                input_h - 16.0 * s,
                                s,
                            ),
                        );
                    let input_scroll_x = ide_panel
                        .api
                        .mock_python_scrolls_x
                        .get(&scroll_key)
                        .map(|scroll| scroll.current.round())
                        .unwrap_or(0.0);
                    if self.begin_api_text_clip(
                        (input_rect_x, input_y, line_gutter_w, input_h),
                        tab_clip,
                    ) {
                        self.draw_api_editor_line_numbers(
                            &text,
                            input_rect_x,
                            line_gutter_w,
                            input_y + 29.0 * s,
                            input_h - 16.0 * s,
                            s,
                            input_scroll_y,
                            if part == ApiMockSourcePart::Body {
                                api_mock_signature_lines(&route.path).len() + 1
                            } else {
                                1
                            },
                        );
                        self.restore_api_tab_clip(tab_clip);
                    }
                    let clip = (
                        input_rect_x + line_gutter_w + 10.0 * s,
                        input_y + 8.0 * s,
                        input_rect_w - line_gutter_w - 20.0 * s,
                        input_h - 16.0 * s,
                    );
                    if self.begin_api_text_clip(clip, tab_clip) {
                        if focused {
                            self.draw_api_editor_selection_multiline(
                                &ide_panel.api.input_editor,
                                input_rect_x + line_gutter_w + 10.0 * s,
                                input_y + 10.0 * s,
                                input_rect_w - line_gutter_w - 20.0 * s,
                                input_h - 16.0 * s,
                                s,
                                input_scroll_y,
                                input_scroll_x,
                            );
                        }
                        let spans = if ide_panel.api.mock_highlight_target.is_some_and(
                            |(highlight_route, highlight_part, _)| {
                                highlight_route == route_idx && highlight_part == part
                            },
                        )
                        {
                            ide_panel.api.mock_highlight_spans.as_slice()
                        } else if let Some(spans) =
                            ide_panel.api.mock_highlight_cache.get(&(route_idx, part))
                        {
                            spans.as_slice()
                        } else {
                            &[]
                        };
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
                        self.draw_api_text_scrollbar(
                            &text,
                            input_rect_x + input_rect_w - 8.0 * s,
                            input_y + 8.0 * s,
                            input_h - 16.0 * s,
                            s,
                            input_scroll_y,
                        );
                        let ty_diagnostics = if matches!(
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
                        let hovered_ty = self.draw_api_mock_ty_squiggles(
                            &text,
                            ty_diagnostics,
                            part,
                            input_rect_x + line_gutter_w + 10.0 * s,
                            input_y + 29.0 * s,
                            input_rect_w - line_gutter_w - 20.0 * s,
                            input_h - 16.0 * s,
                            s,
                            input_scroll_y,
                            input_scroll_x,
                            mx,
                            my,
                        );
                        if focused && blink_alpha > 0.5 {
                            self.draw_api_editor_cursor_multiline(
                                &ide_panel.api.input_editor,
                                input_rect_x + line_gutter_w + 10.0 * s,
                                input_y + 10.0 * s,
                                input_rect_w - line_gutter_w - 20.0 * s,
                                input_h - 16.0 * s,
                                s,
                                input_scroll_y,
                                input_scroll_x,
                            );
                        }
                        self.restore_api_tab_clip(tab_clip);
                        if let Some((message, rect)) = hovered_ty {
                            self.draw_api_mock_ty_popup(
                                &message,
                                rect,
                                editor,
                                ui_registry,
                                mx,
                                my,
                            );
                            mock_ty_popup_drawn = true;
                        } else if !mock_ty_popup_drawn
                            && self.draw_existing_api_mock_ty_popup(editor, ui_registry, mx, my)
                        {
                            mock_ty_popup_drawn = true;
                        }
                    }
                    cy += editor_h + 14.0 * s;
                }
            }
        }
        }

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

        let auth_scheme_indices = api_route_auth_scheme_indices(model, route);
        if !auth_scheme_indices.is_empty() {
            self.draw_api_section_title("Авторизация", x + pad, cy + 18.0 * s, s);
            if api_route_auth_missing(model, route, &ide_panel.api.auth) {
                self.draw_string_scaled_stable(
                    "нет данных авторизации",
                    x + pad + 52.0 * s,
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
                    if valid { "JSON корректен" } else { "JSON с ошибкой" },
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
                                .api_body_prop_row_layout(content_w, s, prop_schema, model, value)
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
                    6.0 * s,
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
                        self.draw_api_editor_selection_multiline(
                            &ide_panel.api.input_editor,
                            x + pad + 10.0 * s,
                            cy + 10.0 * s,
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
                    self.draw_api_text_scrollbar_x(
                        &body_text,
                        x + pad + 8.0 * s,
                        cy + body_h - 8.0 * s,
                        content_w - 16.0 * s,
                        content_w - 20.0 * s,
                        tab_state.body_scroll_x.current,
                        crate::ui_system::UiId::ApiBodyScrollX(route_idx),
                        ui_registry,
                        mx,
                        my,
                    );
                    if body_focused && blink_alpha > 0.5 {
                        self.draw_api_editor_cursor_multiline(
                            &ide_panel.api.input_editor,
                            x + pad + 10.0 * s,
                            cy + 10.0 * s,
                            content_w - 20.0 * s,
                            body_h - 16.0 * s,
                            s,
                            tab_state.body_scroll.current,
                            tab_state.body_scroll_x.current,
                        );
                    }
                    self.restore_api_tab_clip(tab_clip);
                }
                cy += body_h + 16.0 * s;
            }
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
                self.push_rounded_rect(
                    x + pad,
                    cy,
                    content_w,
                    resp_h,
                    6.0 * s,
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
                        self.draw_api_editor_selection_multiline(
                            &ide_panel.api.input_editor,
                            x + pad + 10.0 * s,
                            cy + 10.0 * s,
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
                    self.draw_api_text_scrollbar_x(
                        &response_text,
                        x + pad + 8.0 * s,
                        cy + resp_h - 8.0 * s,
                        content_w - 16.0 * s,
                        content_w - 20.0 * s,
                        tab_state.response_scroll_x.current,
                        crate::ui_system::UiId::ApiResponseScrollX(route_idx),
                        ui_registry,
                        mx,
                        my,
                    );
                    if response_focused && blink_alpha > 0.5 {
                        self.draw_api_editor_cursor_multiline(
                            &ide_panel.api.input_editor,
                            x + pad + 10.0 * s,
                            cy + 10.0 * s,
                            content_w - 20.0 * s,
                            resp_h - 16.0 * s,
                            s,
                            tab_state.response_scroll.current,
                            tab_state.response_scroll_x.current,
                        );
                    }
                    self.restore_api_tab_clip(tab_clip);
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
