#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    /// Mock block of a route tab: title, settings toggle, static response editor and frame.
    fn draw_api_client_tab_mock(
        &mut self,
        ctx: ApiTabRouteCtx<'_>,
        mut cy: f32,
        manual_mock: Option<&crate::app::api_mock::types::ApiManualRoute>,
        mock_override: Option<&crate::app::api_mock::types::ApiMockRouteOverride>,
        ui_registry: &mut crate::ui_system::UiRegistry,
        hover: &mut crate::app::mouse::HoverState,
    ) -> f32 {
        let ApiTabRouteCtx {
            x, pad, content_w, s, mx, my, blink_alpha, tab_clip, route_idx, route, model, tab_meta,
            tab_state, ide_panel,
            ..
        } = ctx;
        let mock_enabled = manual_mock.is_some() || mock_override.is_some_and(|item| item.enabled);
        let is_manual_mock = manual_mock.is_some();
        let python_enabled = manual_mock
            .and_then(|route| route.python.as_ref())
            .or_else(|| mock_override.and_then(|item| item.python.as_ref()))
            .is_some_and(|script| script.enabled);

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
                self.ui.pick(UiRole::Success, [0.48, 0.86, 0.52, 1.0]),
                0.86,
            );
        }
        cy += API_ROUTE_MOCK_TITLE_ADVANCE * s;
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
        cy += API_ROUTE_MOCK_TOGGLE_ADVANCE * s;
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
                        self.ui.pick(UiRole::ApiMockRoute, [0.50, 0.90, 0.55, 1.0]),
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
                    self.ui.pick(UiRole::TextSecondary, [0.68, 0.70, 0.78, 1.0]),
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
                        self.ui.pick(UiRole::Accent, [0.60, 0.35, 0.85, 1.0])
                    } else {
                        self.ui.ink(0.12)
                    },
                    self.ui.pick(UiRole::BgInput, [0.13, 0.14, 0.18, 1.0]),
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
                let static_scroll_y = tab_state.mock_static_response_scroll.current.round();
                let static_scroll_x = tab_state.mock_static_response_scroll_x.current.round();
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
                            static_scroll_x,
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
                        static_scroll_x,
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
                            static_scroll_x,
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
                    crate::ui_system::UiId::ApiMockStaticResponseScrollY(route_idx),
                    ui_registry,
                    mx,
                    my,
                );
                self.draw_api_text_scrollbar_x(
                    &static_text,
                    x + pad + 8.0 * s,
                    cy + editor_h - 12.0 * s,
                    content_w - 16.0 * s,
                    content_w - 20.0 * s,
                    static_scroll_x,
                    crate::ui_system::UiId::ApiMockStaticResponseScrollX(route_idx),
                    ui_registry,
                    mx,
                    my,
                );
                cy += editor_h + 14.0 * s;
            } else {
                cy = self.draw_api_client_tab_mock_python(
                    ctx,
                    cy,
                    manual_mock,
                    mock_override,
                    ui_registry,
                    hover,
                );
            }
        }
        if mock_expanded {
            let line_w = (1.0 * s).round().max(1.0);
            let frame_x = (x + pad - 10.0 * s).round();
            let frame_y = mock_frame_y.round();
            let frame_w = (content_w + 20.0 * s).round().max(line_w * 2.0);
            let frame_h = (cy - mock_frame_y - 8.0 * s).round().max(line_w * 2.0);
            let frame_color = self.ui.pick(
                UiRole::Selection,
                [self.theme.sel[0], self.theme.sel[1], self.theme.sel[2], 0.55],
            );
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
        cy
    }

    /// Python mock editor of a route tab: contract controls and the combined prelude/contract/body editor.
    fn draw_api_client_tab_mock_python(
        &mut self,
        ctx: ApiTabRouteCtx<'_>,
        mut cy: f32,
        manual_mock: Option<&crate::app::api_mock::types::ApiManualRoute>,
        mock_override: Option<&crate::app::api_mock::types::ApiMockRouteOverride>,
        ui_registry: &mut crate::ui_system::UiRegistry,
        hover: &mut crate::app::mouse::HoverState,
    ) -> f32 {
        let ApiTabRouteCtx {
            x, pad, content_w, s, mx, my, blink_alpha, tab_clip, route_idx, route, model, ide_panel,
            ..
        } = ctx;
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
                    self.ui.pick(UiRole::Accent, [0.60, 0.35, 0.85, 1.0])
                } else {
                    self.ui.ink(0.12)
                },
                self.ui.pick(UiRole::BgInput, [0.13, 0.14, 0.18, 1.0]),
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
                        self.ui.ink(0.030),
                    );
                    self.draw_string_scaled_stable(
                        label,
                        x + pad + line_gutter_w + 10.0 * s,
                        api_mock_contract_row_text_y(section_y, header_h, s),
                        self.ui.pick(UiRole::TextSecondary, [0.68, 0.70, 0.78, 1.0]),
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
                            custom_color: Some(self.ui.pick(UiRole::ApiMockRoute, [0.76, 0.79, 0.88, 1.0])),
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
                        self.ui.ink(0.08),
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
                                    hover,
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
                                hover,
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
                            self.ui.ink(0.10),
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
                let scrollbar = crate::app::api_client::api_mock_combined_editor_scrollbar(
                    (track_x, track_y, track_w, track_h),
                    viewport_h,
                    combined_h,
                    combined_scroll_y,
                );
                let _ = self.draw_scrollbar(
                    &scrollbar,
                    s,
                    1.0,
                    Some(crate::render_view::scrollbar_widget::ScrollbarHit {
                        ui: ui_registry,
                        id: crate::ui_system::UiId::ApiMockCombinedScrollY(route_idx),
                        mx,
                        my,
                        blocker: false,
                    }),
                );
            }
            cy += viewport_h + 14.0 * s;
        }
        cy
    }
}
