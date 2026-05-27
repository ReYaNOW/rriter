impl Renderer {
    fn draw_api_manual_client_tab(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        ide_panel: &crate::app::IdePanelState,
        tab_meta: &crate::app::api_client::ApiClientTabMeta,
        tab_state: &crate::app::api_client::ApiClientTabState,
        stable_id: &str,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        blink_alpha: f32,
    ) {
        let Some((manual_idx, route)) = ide_panel
            .api
            .mock
            .manual_routes
            .iter()
            .enumerate()
            .find(|(_, route)| route.stable_id == stable_id)
        else {
            self.draw_string_scaled_stable(
                "Ручной route удалён",
                x + 28.0 * s,
                y + 46.0 * s,
                [0.72, 0.74, 0.82, 1.0],
                0.95,
            );
            return;
        };

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
        let route_idx = tab_state.route_idx.unwrap_or(manual_idx);

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
        cy += 50.0 * s;

        self.draw_api_section_title("LAN-сервер", x + pad, cy + 18.0 * s, s);
        cy += 28.0 * s;
        let lan_url = api_mock_lan_url(&ide_panel.api.mock);
        self.draw_string_scaled_stable(
            &lan_url,
            x + pad,
            cy + 20.0 * s,
            [0.68, 0.70, 0.78, 1.0],
            0.88,
        );
        cy += 42.0 * s;

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

    fn draw_api_section_title(&mut self, text: &str, x: f32, y: f32, _s: f32) {
        self.draw_string_scaled_stable(
            text,
            x,
            y.round(),
            [0.74, 0.76, 0.84, 1.0],
            API_SECTION_TITLE_SCALE,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_api_response_tab(
        &mut self,
        label: &str,
        active: bool,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        id: crate::ui_system::UiId,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
    ) {
        let hovered = ui_registry.register_rect(id, x, y, w, h, mx, my);
        if active || hovered {
            self.push_rounded_rect(
                x,
                y,
                w,
                h,
                5.0 * s,
                if active {
                    [1.0, 1.0, 1.0, 0.12]
                } else {
                    [1.0, 1.0, 1.0, 0.06]
                },
            );
        }
        self.draw_string_scaled_stable(
            label,
            x + 11.0 * s,
            api_centered_text_y(y, h, s),
            if active {
                self.theme.fg
            } else {
                [0.64, 0.66, 0.74, 1.0]
            },
            0.86,
        );
    }

    fn draw_api_dynamic_table_frame(&mut self, x: f32, y: f32, w: f32, h: f32, _s: f32) {
        if h <= 0.0 {
            return;
        }
        let line = [1.0, 1.0, 1.0, 0.13];
        let line_h = 1.0;
        let x = x.round();
        let y = y.round();
        let w = w.round().max(line_h * 2.0);
        let h = h.round().max(line_h);
        self.push_rect(x, y, w, h, [0.12, 0.13, 0.17, 1.0]);
        self.push_rect(x, y, w, line_h, line);
        self.push_rect(x, y, line_h, h, line);
        self.push_rect(x + w - line_h, y, line_h, h, line);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_api_auth_scheme_row(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        s: f32,
        spec_id: crate::app::api_client::ApiSpecId,
        scheme_idx: usize,
        scheme: &crate::app::api_client::ApiSecurityScheme,
        ide_panel: &crate::app::IdePanelState,
        blink_alpha: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
    ) -> f32 {
        let row_h = api_auth_scheme_row_height(scheme, s);
        let label_x = x + 12.0 * s;
        let text_y = y + 24.0 * s;
        self.draw_string_scaled_stable(&scheme.name, label_x, text_y, self.theme.fg, 0.90);
        self.draw_string_scaled_stable(
            &scheme.summary(),
            label_x,
            text_y + 18.0 * s,
            [0.35, 0.75, 1.0, 1.0],
            API_FIELD_META_SCALE,
        );

        let save_w = 58.0 * s;
        let clear_w = 58.0 * s;
        let input_x = x + (w * 0.34).max(145.0 * s);
        let input_w = (w - (input_x - x) - save_w - clear_w - 28.0 * s).max(120.0 * s);
        let entry = ide_panel.api.auth.entry(spec_id, &scheme.name);
        if matches!(
            scheme.kind,
            ApiSecuritySchemeKind::Http { ref scheme, .. } if scheme.eq_ignore_ascii_case("basic")
        ) {
            let user_focused = matches!(
                ide_panel.api.focused,
                Some(ApiFocus::AuthUsername { spec_id: f_spec, scheme: ref focused_scheme })
                    if f_spec == spec_id && focused_scheme == &scheme.name
            );
            let pass_focused = matches!(
                ide_panel.api.focused,
                Some(ApiFocus::AuthPassword { spec_id: f_spec, scheme: ref focused_scheme })
                    if f_spec == spec_id && focused_scheme == &scheme.name
            );
            let username = entry.map(|entry| entry.username.as_str()).unwrap_or("");
            let password = entry.map(|entry| entry.password.as_str()).unwrap_or("");
            let user_edit_text;
            let shown_username = if user_focused {
                user_edit_text = ide_panel.api.input_editor.get_full_text();
                user_edit_text.as_str()
            } else {
                username
            };
            self.draw_api_auth_input(
                input_x,
                y + 10.0 * s,
                input_w,
                32.0 * s,
                s,
                shown_username,
                user_focused,
                ide_panel.api.input_scroll_x.current,
                false,
                &ide_panel.api.input_editor,
                blink_alpha,
                crate::ui_system::UiId::ApiAuthUsername(scheme_idx),
                ui_registry,
                mx,
                my,
            );
            let pass_edit_text;
            let shown_password = if pass_focused {
                pass_edit_text = ide_panel.api.input_editor.get_full_text();
                pass_edit_text.as_str()
            } else {
                password
            };
            self.draw_api_auth_input(
                input_x,
                y + 48.0 * s,
                input_w,
                32.0 * s,
                s,
                shown_password,
                pass_focused,
                ide_panel.api.input_scroll_x.current,
                true,
                &ide_panel.api.input_editor,
                blink_alpha,
                crate::ui_system::UiId::ApiAuthPassword(scheme_idx),
                ui_registry,
                mx,
                my,
            );
        } else if scheme.token_capable() {
            let focused = matches!(
                ide_panel.api.focused,
                Some(ApiFocus::AuthValue { spec_id: f_spec, scheme: ref focused_scheme })
                    if f_spec == spec_id && focused_scheme == &scheme.name
            );
            let value = entry
                .map(|entry| {
                    if !entry.value.is_empty() {
                        entry.value.as_str()
                    } else {
                        entry.access_token.as_str()
                    }
                })
                .unwrap_or("");
            let edit_text;
            let shown_value = if focused {
                edit_text = ide_panel.api.input_editor.get_full_text();
                edit_text.as_str()
            } else {
                value
            };
            let token_label_w = self.measure_ui_width("token", API_FIELD_META_SCALE);
            self.draw_string_scaled_stable(
                "токен",
                input_x - token_label_w - 10.0 * s,
                api_centered_text_y(y + 14.0 * s, 30.0 * s, s),
                [0.68, 0.70, 0.78, 1.0],
                API_FIELD_META_SCALE,
            );
            self.draw_api_auth_input(
                input_x,
                y + 14.0 * s,
                input_w,
                30.0 * s,
                s,
                shown_value,
                focused,
                ide_panel.api.input_scroll_x.current,
                false,
                &ide_panel.api.input_editor,
                blink_alpha,
                crate::ui_system::UiId::ApiAuthValue(scheme_idx),
                ui_registry,
                mx,
                my,
            );
            let save = Button {
                x: input_x + input_w + 8.0 * s,
                y: y + 14.0 * s,
                w: save_w,
                h: 30.0 * s,
                text: "Сохранить".to_string(),
                icon: None,
                text_scale: API_FIELD_META_SCALE,
                icon_size: 0.0,
            };
            ui_registry.register_button(
                crate::ui_system::UiId::ApiAuthAccessSave(scheme_idx),
                &save,
                self,
                mx,
                my,
                s,
                false,
            );
            let clear = Button {
                x: save.x + save.w + 6.0 * s,
                y: save.y,
                w: clear_w,
                h: 30.0 * s,
                text: "Очистить".to_string(),
                icon: None,
                text_scale: API_FIELD_META_SCALE,
                icon_size: 0.0,
            };
            ui_registry.register_button(
                crate::ui_system::UiId::ApiAuthAccessClear(scheme_idx),
                &clear,
                self,
                mx,
                my,
                s,
                false,
            );
            let status = entry.map(|entry| entry.token_type.as_str()).unwrap_or("");
            if !status.is_empty() {
                self.draw_string_scaled_stable(
                    status,
                    input_x,
                    y + row_h - 8.0 * s,
                    [0.48, 0.86, 0.52, 1.0],
                    API_FIELD_META_SCALE,
                );
            }
            self.push_rect(x, y + row_h, w, (1.0 * s).max(1.0), [1.0, 1.0, 1.0, 0.12]);
            return y + row_h;
        } else {
            let focused = matches!(
                ide_panel.api.focused,
                Some(ApiFocus::AuthValue { spec_id: f_spec, scheme: ref focused_scheme })
                    if f_spec == spec_id && focused_scheme == &scheme.name
            );
            let value = entry.map(|entry| entry.value.as_str()).unwrap_or("");
            let edit_text;
            let shown_value = if focused {
                edit_text = ide_panel.api.input_editor.get_full_text();
                edit_text.as_str()
            } else {
                value
            };
            self.draw_api_auth_input(
                input_x,
                y + (row_h - 34.0 * s) * 0.5,
                input_w,
                34.0 * s,
                s,
                shown_value,
                focused,
                ide_panel.api.input_scroll_x.current,
                false,
                &ide_panel.api.input_editor,
                blink_alpha,
                crate::ui_system::UiId::ApiAuthValue(scheme_idx),
                ui_registry,
                mx,
                my,
            );
        }

        let btn_y = y + (row_h - 30.0 * s) * 0.5;
        let save = Button {
            x: input_x + input_w + 8.0 * s,
            y: btn_y,
            w: save_w,
            h: 30.0 * s,
            text: "Сохранить".to_string(),
            icon: None,
            text_scale: API_FIELD_META_SCALE,
            icon_size: 0.0,
        };
        ui_registry.register_button(
            crate::ui_system::UiId::ApiAuthSave(scheme_idx),
            &save,
            self,
            mx,
            my,
            s,
            false,
        );
        let clear = Button {
            x: save.x + save.w + 6.0 * s,
            y: btn_y,
            w: clear_w,
            h: 30.0 * s,
            text: "Очистить".to_string(),
            icon: None,
            text_scale: API_FIELD_META_SCALE,
            icon_size: 0.0,
        };
        ui_registry.register_button(
            crate::ui_system::UiId::ApiAuthClear(scheme_idx),
            &clear,
            self,
            mx,
            my,
            s,
            false,
        );
        self.push_rect(x, y + row_h, w, (1.0 * s).max(1.0), [1.0, 1.0, 1.0, 0.12]);
        y + row_h
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_api_auth_input(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        value: &str,
        focused: bool,
        input_scroll_x: f32,
        mask: bool,
        editor: &crate::editor::Editor,
        blink_alpha: f32,
        id: crate::ui_system::UiId,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
    ) {
        self.push_rounded_rect_border(
            x,
            y,
            w,
            h,
            5.0 * s,
            (1.0 * s).max(1.0),
            if focused {
                [0.60, 0.35, 0.85, 1.0]
            } else {
                [1.0, 1.0, 1.0, 0.12]
            },
            [0.13, 0.14, 0.18, 1.0],
        );
        ui_registry.register_text_input(id, x, y, w, h, mx, my);
        let text_w = w - 16.0 * s;
        let scroll_x = if focused { input_scroll_x } else { 0.0 };
        if focused {
            let sel_y = y + (h - 22.0 * s) * 0.5;
            self.draw_api_editor_selection_one_line(
                editor,
                x + 8.0 * s,
                sel_y,
                text_w,
                22.0 * s,
                API_FIELD_VALUE_SCALE,
                scroll_x,
            );
        }
        let shown = if mask && !focused && !value.is_empty() {
            "••••••••".to_string()
        } else {
            value.to_string()
        };
        self.draw_api_one_line_clipped(
            &shown,
            x + 8.0 * s,
            api_centered_text_y(y, h, s),
            text_w,
            scroll_x,
            self.theme.fg,
            API_FIELD_VALUE_SCALE,
        );
        if focused && blink_alpha > 0.5 {
            let cursor_w =
                self.api_editor_cursor_x_one_line(editor, API_FIELD_VALUE_SCALE) - scroll_x;
            self.push_rect(
                x + 8.0 * s + cursor_w.clamp(0.0, text_w),
                y + (h - 20.0 * s) * 0.5,
                1.5 * s,
                20.0 * s,
                self.theme.fg,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_api_body_prop_row(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        s: f32,
        route_idx: usize,
        prop_idx: usize,
        name: &str,
        required: bool,
        schema: &ApiSchema,
        model: &crate::app::api_client::ApiSpecModel,
        value: &str,
        focused: bool,
        input_scroll_x: f32,
        editor: &crate::editor::Editor,
        blink_alpha: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
    ) -> f32 {
        let allowed = api_schema_allowed_values(schema, model);
        let shown = if focused {
            editor.get_full_text()
        } else {
            value.to_string()
        };
        let layout = self.api_body_prop_row_layout(w, s, schema, model, &shown);
        let row_h = layout.row_h;
        let is_array = api_schema_is_array_input(schema);
        let is_file = api_schema_is_file_input(schema, model);
        let pick_w = if is_file { 64.0 * s } else { 0.0 };
        let input_x = x + layout.input_x;
        let input_w = layout.input_w;
        let input_h = layout.input_h;
        let input_y = y + (row_h - input_h) * 0.5;
        let label_right = input_x - 18.0 * s;
        let name_w = self.measure_ui_width(name, API_FIELD_NAME_SCALE);
        let name_x = (label_right - name_w).max(x + 12.0 * s);
        let name_y = api_split_label_text_y(input_y, input_h, s, false);
        self.draw_string_scaled_stable(name, name_x, name_y, self.theme.fg, API_FIELD_NAME_SCALE);
        if required {
            self.draw_string_scaled_stable(
                "*",
                name_x + name_w + 3.0 * s,
                name_y,
                [1.0, 0.42, 0.42, 1.0],
                API_FIELD_NAME_SCALE,
            );
        }
        let type_text = api_body_schema_type_text(schema, model);
        let type_w = self.measure_ui_width(&type_text, API_FIELD_TYPE_SCALE);
        self.draw_string_scaled_stable(
            &type_text,
            (label_right - type_w).max(x + 12.0 * s),
            api_split_label_text_y(input_y, input_h, s, true),
            [0.35, 0.75, 1.0, 1.0],
            API_FIELD_TYPE_SCALE,
        );
        self.push_rounded_rect_border(
            input_x,
            input_y,
            input_w,
            input_h,
            5.0 * s,
            (1.0 * s).max(1.0),
            if focused {
                [0.60, 0.35, 0.85, 1.0]
            } else {
                [1.0, 1.0, 1.0, 0.12]
            },
            [0.13, 0.14, 0.18, 1.0],
        );
        let text_w = (input_w - 16.0 * s - pick_w).max(24.0 * s);
        ui_registry.register_text_input(
            crate::ui_system::UiId::ApiBodyFieldInput(route_idx, prop_idx),
            input_x,
            input_y,
            input_w - pick_w,
            input_h,
            mx,
            my,
        );
        let field_scroll_x = if focused && !is_array {
            input_scroll_x
        } else {
            0.0
        };
        if focused && !is_array {
            self.draw_api_editor_selection_one_line(
                editor,
                input_x + 8.0 * s,
                input_y + (input_h - 24.0 * s) * 0.5,
                text_w,
                24.0 * s,
                API_FIELD_VALUE_SCALE,
                field_scroll_x,
            );
        }
        if is_array {
            self.draw_api_array_value_chips(
                &shown,
                input_x + 8.0 * s,
                input_y,
                text_w,
                input_h,
                s,
                focused,
            );
        } else {
            self.draw_api_one_line_clipped(
                &shown,
                input_x + 8.0 * s,
                api_centered_text_y(input_y, input_h, s),
                text_w,
                field_scroll_x,
                self.theme.fg,
                API_FIELD_VALUE_SCALE,
            );
        }
        if focused && blink_alpha > 0.5 {
            let (cursor_w, cursor_y) = if is_array {
                let (cursor_w, cursor_row) = self.api_array_visual_cursor(&shown, text_w, s);
                (
                    cursor_w,
                    input_y + cursor_row as f32 * 32.0 * s + (32.0 * s - 22.0 * s) * 0.5,
                )
            } else {
                (
                    self.api_editor_cursor_x_one_line(editor, API_FIELD_VALUE_SCALE)
                        - field_scroll_x,
                    input_y + (input_h - 22.0 * s) * 0.5,
                )
            };
            self.push_rect(
                input_x + 8.0 * s + cursor_w.clamp(0.0, text_w),
                cursor_y,
                1.5 * s,
                22.0 * s,
                self.theme.fg,
            );
        }
        if is_file {
            let btn = Button {
                x: input_x + input_w - pick_w,
                y: input_y,
                w: pick_w,
                h: input_h,
                text: if api_schema_is_multi_file_input(schema, model) {
                    "Файлы".to_string()
                } else {
                    "Файл".to_string()
                },
                icon: None,
                text_scale: API_FIELD_META_SCALE,
                icon_size: 0.0,
            };
            ui_registry.register_button(
                crate::ui_system::UiId::ApiBodyFilePick(route_idx, prop_idx),
                &btn,
                self,
                mx,
                my,
                s,
                false,
            );
        }
        let mut right_y = input_y + 13.0 * s;
        let right_x = x + layout.right_x;
        if let Some(max) = schema.max_chars {
            let text = format!("До {} символов", max);
            self.draw_string_scaled_stable(
                &text,
                right_x,
                right_y,
                [0.68, 0.70, 0.78, 1.0],
                API_FIELD_META_SCALE,
            );
            right_y += 20.0 * s;
        }
        let right_w = layout.right_w;
        if let Some(default) = &schema.default_value {
            self.draw_api_meta_inline("По умолчанию:", default, right_x, right_y, s);
            right_y += 20.0 * s;
        }
        if !allowed.is_empty() {
            self.draw_api_allowed_values(
                "Допустимо:",
                allowed,
                right_x,
                right_y,
                right_w,
                s,
                ui_registry,
                mx,
                my,
                |idx| crate::ui_system::UiId::ApiBodyAllowedValue(route_idx, prop_idx, idx),
            );
        } else if !schema.examples.is_empty() {
            self.draw_api_allowed_values(
                "Примеры:",
                &schema.examples,
                right_x,
                right_y,
                right_w,
                s,
                ui_registry,
                mx,
                my,
                |idx| crate::ui_system::UiId::ApiBodyAllowedValue(route_idx, prop_idx, idx),
            );
        }
        let mut example_y = input_y + input_h + 19.0 * s;
        for example in schema
            .examples
            .iter()
            .take(3)
            .filter(|_| !allowed.is_empty())
        {
            self.draw_string_scaled_stable(
                example,
                input_x + 8.0 * s,
                example_y,
                [0.62, 0.64, 0.72, 1.0],
                API_FIELD_META_SCALE,
            );
            example_y += 20.0 * s;
        }
        self.draw_api_table_row_separator(x, y, w, row_h, s);
        row_h
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_api_param_input(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        s: f32,
        route_idx: usize,
        param_idx: usize,
        param: &ApiParam,
        value: &str,
        focused: bool,
        input_scroll_x: f32,
        editor: &crate::editor::Editor,
        blink_alpha: f32,
        id: crate::ui_system::UiId,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
    ) -> f32 {
        let shown = if focused {
            editor.get_full_text()
        } else {
            value.to_string()
        };
        let layout = self.api_param_row_layout(w, s, param, &shown);
        let row_h = layout.row_h;
        let is_array = matches!(
            param.primitive_type,
            crate::app::api_client::ApiPrimitiveType::Array
        );
        let input_x = x + layout.input_x;
        let input_w = layout.input_w;
        let input_h = layout.input_h;
        let input_y = y + (row_h - input_h) * 0.5;
        let label_right = input_x - 18.0 * s;
        let name_w = self.measure_ui_width(&param.name, API_FIELD_NAME_SCALE);
        let name_x = (label_right - name_w).max(x + 12.0 * s);
        let name_y = api_split_label_text_y(input_y, input_h, s, false);
        self.draw_string_scaled_stable(
            &param.name,
            name_x,
            name_y,
            self.theme.fg,
            API_FIELD_NAME_SCALE,
        );
        if param.required {
            self.draw_string_scaled_stable(
                "*",
                name_x + name_w + 3.0 * s,
                name_y,
                [1.0, 0.42, 0.42, 1.0],
                API_FIELD_NAME_SCALE,
            );
        }
        let type_text = api_param_type_text(param);
        let type_w = self.measure_ui_width(&type_text, API_FIELD_TYPE_SCALE);
        self.draw_string_scaled_stable(
            &type_text,
            (label_right - type_w).max(x + 12.0 * s),
            api_split_label_text_y(input_y, input_h, s, true),
            [0.35, 0.75, 1.0, 1.0],
            API_FIELD_TYPE_SCALE,
        );
        self.push_rounded_rect_border(
            input_x,
            input_y,
            input_w,
            input_h,
            5.0 * s,
            (1.0 * s).max(1.0),
            if focused {
                [0.60, 0.35, 0.85, 1.0]
            } else {
                [1.0, 1.0, 1.0, 0.12]
            },
            [0.13, 0.14, 0.18, 1.0],
        );
        ui_registry.register_text_input(id, input_x, input_y, input_w, input_h, mx, my);
        let field_w = input_w - 16.0 * s;
        let field_scroll_x = if focused && !is_array {
            input_scroll_x
        } else {
            0.0
        };
        if focused && !is_array {
            self.draw_api_editor_selection_one_line(
                editor,
                input_x + 8.0 * s,
                input_y + (input_h - 24.0 * s) * 0.5,
                field_w,
                24.0 * s,
                API_FIELD_VALUE_SCALE,
                field_scroll_x,
            );
        }
        if is_array {
            self.draw_api_array_value_chips(
                &shown,
                input_x + 8.0 * s,
                input_y,
                field_w,
                input_h,
                s,
                focused,
            );
        } else {
            self.draw_api_one_line_clipped(
                &shown,
                input_x + 8.0 * s,
                api_centered_text_y(input_y, input_h, s),
                field_w,
                field_scroll_x,
                self.theme.fg,
                API_FIELD_VALUE_SCALE,
            );
        }
        if focused && blink_alpha > 0.5 {
            let (cursor_w, cursor_y) = if is_array {
                let (cursor_w, cursor_row) = self.api_array_visual_cursor(&shown, field_w, s);
                (
                    cursor_w,
                    input_y + cursor_row as f32 * 32.0 * s + (32.0 * s - 22.0 * s) * 0.5,
                )
            } else {
                (
                    self.api_editor_cursor_x_one_line(editor, API_FIELD_VALUE_SCALE)
                        - field_scroll_x,
                    input_y + (input_h - 22.0 * s) * 0.5,
                )
            };
            self.push_rect(
                input_x + 8.0 * s + cursor_w.clamp(0.0, field_w),
                cursor_y,
                1.5 * s,
                22.0 * s,
                self.theme.fg,
            );
        }
        let right_x = x + layout.right_x;
        let mut right_y = input_y + 15.0 * s;
        let right_w = layout.right_w;
        if let Some(default) = &param.default_value {
            self.draw_api_meta_inline("По умолчанию:", default, right_x, right_y, s);
            right_y += 20.0 * s;
        }
        if !param.enum_values.is_empty() || !param.examples.is_empty() {
            let base_id = match id {
                crate::ui_system::UiId::ApiPathParamInput(_, _) => {
                    crate::ui_system::UiId::ApiPathParamAllowedValue(route_idx, param_idx, 0)
                }
                crate::ui_system::UiId::ApiQueryParamInput(_, _) => {
                    crate::ui_system::UiId::ApiQueryParamAllowedValue(route_idx, param_idx, 0)
                }
                _ => id,
            };
            let (label, values) = if param.enum_values.is_empty() {
                ("Примеры:", &param.examples)
            } else {
                ("Допустимо:", &param.enum_values)
            };
            self.draw_api_allowed_values(
                label,
                values,
                right_x,
                right_y,
                right_w,
                s,
                ui_registry,
                mx,
                my,
                |idx| match base_id {
                    crate::ui_system::UiId::ApiPathParamAllowedValue(route, param, _) => {
                        crate::ui_system::UiId::ApiPathParamAllowedValue(route, param, idx)
                    }
                    crate::ui_system::UiId::ApiQueryParamAllowedValue(route, param, _) => {
                        crate::ui_system::UiId::ApiQueryParamAllowedValue(route, param, idx)
                    }
                    _ => id,
                },
            );
        } else if let Some(example) = &param.example {
            self.draw_api_meta_inline("Пример:", example, right_x, right_y, s);
        }
        self.draw_api_table_row_separator(x, y, w, row_h, s);
        y + row_h
    }
}
