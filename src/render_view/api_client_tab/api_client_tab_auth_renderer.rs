
#[derive(Debug, Clone, Copy, PartialEq)]
struct ApiAuthRowLayout {
    input_offset: f32,
    input_w: f32,
    save_w: f32,
    clear_w: f32,
    input_action_gap: f32,
    action_gap: f32,
    compact_actions: bool,
}

fn api_auth_row_layout(width: f32, scale: f32) -> ApiAuthRowLayout {
    let width = width.max(0.0);
    let input_offset = (width * 0.28)
        .clamp(52.0 * scale, 124.0 * scale)
        .min((width * 0.42).max(0.0));
    let after_label = (width - input_offset).max(0.0);
    let natural_action_w = 84.0 * scale;
    let gap_total = (14.0 * scale).min(after_label * 0.15);
    let preferred_input_w = (72.0 * scale).min(after_label * 0.45);
    let action_w = ((after_label - gap_total - preferred_input_w).max(0.0) * 0.5)
        .min(natural_action_w);
    let input_w = (after_label - gap_total - action_w * 2.0).max(0.0);
    ApiAuthRowLayout {
        input_offset,
        input_w,
        save_w: action_w,
        clear_w: action_w,
        input_action_gap: gap_total * (8.0 / 14.0),
        action_gap: gap_total * (6.0 / 14.0),
        compact_actions: action_w < 62.0 * scale,
    }
}
impl Renderer {
    fn draw_api_section_title(&mut self, text: &str, x: f32, y: f32, _s: f32) {
        self.draw_string_scaled_stable(
            text,
            x,
            y.round(),
            [0.74, 0.76, 0.84, 1.0],
            API_SECTION_TITLE_SCALE,
        );
    }

    fn draw_api_schema_summary(&mut self, text: &str, x: f32, y: f32) {
        let mut draw_x = x;
        let y = y.round();
        for (idx, part) in text.split(" · ").enumerate() {
            if idx > 0 {
                self.draw_string_scaled_stable(
                    " · ",
                    draw_x,
                    y,
                    [0.50, 0.52, 0.60, 1.0],
                    API_SECTION_TITLE_SCALE,
                );
                draw_x += self.measure_ui_width(" · ", API_SECTION_TITLE_SCALE);
            }
            let color = if part.contains('/') {
                [0.35, 0.75, 1.0, 1.0]
            } else if part.contains("required") {
                crate::highlighter::DRACULA_PINK
            } else if idx == 0 {
                [0.74, 0.76, 0.84, 1.0]
            } else {
                [0.62, 0.64, 0.70, 1.0]
            };
            self.draw_string_scaled_stable(part, draw_x, y, color, API_SECTION_TITLE_SCALE);
            draw_x += self.measure_ui_width(part, API_SECTION_TITLE_SCALE);
        }
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
        let label_w = self.measure_ui_width(label, 0.86);
        self.draw_string_scaled_stable(
            label,
            x + ((w - label_w) * 0.5).round(),
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
        let layout = api_auth_row_layout(w, s);
        let mut label_scratch = String::new();
        let label_max_w = (layout.input_offset - 24.0 * s).max(1.0);
        self.draw_tree_label_clipped(
            &scheme.name,
            label_x,
            text_y,
            label_max_w,
            self.theme.fg,
            0.90,
            &mut label_scratch,
        );
        self.draw_tree_label_clipped(
            &scheme.summary(),
            label_x,
            text_y + 18.0 * s,
            label_max_w,
            [0.35, 0.75, 1.0, 1.0],
            API_FIELD_META_SCALE,
            &mut label_scratch,
        );

        let save_w = layout.save_w;
        let clear_w = layout.clear_w;
        let input_x = x + layout.input_offset;
        let input_w = layout.input_w;
        let save_label = if layout.compact_actions { "✓" } else { "Сохранить" };
        let clear_label = if layout.compact_actions { "×" } else { "Очистить" };
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
                x: input_x + input_w + layout.input_action_gap,
                y: y + 14.0 * s,
                w: save_w,
                h: 30.0 * s,
                text: save_label.to_string(),
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
                x: save.x + save.w + layout.action_gap,
                y: save.y,
                w: clear_w,
                h: 30.0 * s,
                text: clear_label.to_string(),
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
            x: input_x + input_w + layout.input_action_gap,
            y: btn_y,
            w: save_w,
            h: 30.0 * s,
            text: save_label.to_string(),
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
            x: save.x + save.w + layout.action_gap,
            y: btn_y,
            w: clear_w,
            h: 30.0 * s,
            text: clear_label.to_string(),
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
        let shown = if mask && !focused && !value.is_empty() {
            "••••••••".to_string()
        } else {
            value.to_string()
        };
        self.draw_api_one_line_input(
            x,
            y,
            w,
            h,
            s,
            &shown,
            self.theme.fg,
            focused,
            input_scroll_x,
            editor,
            blink_alpha,
            id,
            ui_registry,
            mx,
            my,
            API_FIELD_VALUE_SCALE,
        );
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
            input_scroll_x.round()
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
            input_scroll_x.round()
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
