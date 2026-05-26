use crate::app::api_client::{
    API_BODY_TEXT_SCALE, API_MOCK_TY_POPUP_BYTE, ApiFocus, ApiParam, ApiResponseView, ApiSchema,
    ApiSchemaKind, ApiSecuritySchemeKind, api_array_edit_parts, api_array_value_parts,
    api_auth_related_route_count, api_auth_route_rank, api_auth_scheme_row_height,
    api_body_text_area_height, api_response_text, api_response_text_area_height,
    api_route_auth_missing, api_route_auth_scheme_indices, api_schema_allowed_values,
    api_generated_response_for_route, api_mock_body_editor_text, api_mock_lan_url,
    api_schema_is_array_input,
    api_schema_is_file_input, api_schema_is_multi_file_input, api_text_area_line_height,
    api_text_area_max_scroll_x, json_body_is_valid, write_api_path_display,
};
use crate::app::api_mock::ty_check::ApiMockSourcePart;
use crate::renderer::Renderer;
use crate::widgets::{Button, IconButton, IconType};
use glow::HasContext;

const API_SECTION_TITLE_SCALE: f32 = 0.92;
const API_FIELD_NAME_SCALE: f32 = 0.94;
const API_FIELD_TYPE_SCALE: f32 = 0.84;
const API_FIELD_VALUE_SCALE: f32 = 0.88;
const API_FIELD_META_SCALE: f32 = 0.78;
#[derive(Clone, Copy)]
struct ApiFieldRowLayout {
    row_h: f32,
    input_x: f32,
    input_w: f32,
    input_h: f32,
    right_x: f32,
    right_w: f32,
}

fn api_mock_signature_lines(path: &str) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push("def handler(".to_string());
    lines.push("    req: Request,".to_string());
    for name in path_param_names(path) {
        lines.push(format!("    {}: str,", sanitize_python_param(&name)));
    }
    lines.push("    query: Query,".to_string());
    lines.push("    body: Body | None,".to_string());
    lines.push("    fields: Fields,".to_string());
    lines.push(") -> dict[str, Any]:".to_string());
    lines
}

fn api_mock_signature_text(path: &str) -> String {
    api_mock_signature_lines(path).join("\n")
}

fn api_mock_path_param_count(path: &str) -> usize {
    let mut count = 0usize;
    let mut rest = path;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            break;
        };
        if !after[..end].trim().is_empty() {
            count += 1;
        }
        rest = &after[end + 1..];
    }
    count
}

fn api_mock_signature_block_height(path: &str, s: f32) -> f32 {
    let line_h = api_text_area_line_height(s);
    let line_count = 6 + api_mock_path_param_count(path);
    line_count as f32 * line_h + 12.0 * s
}

fn editor_line_number_text<'a>(line_no: usize, buf: &'a mut [u8; 20]) -> Option<&'a str> {
    let mut n = line_no;
    let mut idx = 20;
    if n == 0 {
        idx -= 1;
        buf[idx] = b'0';
    } else {
        while n > 0 {
            idx -= 1;
            buf[idx] = b'0' + (n % 10) as u8;
            n /= 10;
        }
    }
    std::str::from_utf8(&buf[idx..]).ok()
}

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

    #[allow(clippy::too_many_arguments)]
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

    fn draw_api_table_row_separator(&mut self, x: f32, y: f32, w: f32, row_h: f32, _s: f32) {
        let line_h = 1.0;
        let x = x.round();
        let y = (y + row_h - line_h).round();
        let w = w.round().max(line_h * 2.0);
        self.push_rect(
            x + line_h,
            y,
            (w - line_h * 2.0).max(0.0),
            line_h,
            [1.0, 1.0, 1.0, 0.13],
        );
    }

    fn draw_api_meta_inline(&mut self, label: &str, value: &str, x: f32, y: f32, s: f32) {
        self.draw_string_scaled_stable(label, x, y, [0.68, 0.70, 0.78, 1.0], API_FIELD_META_SCALE);
        let label_w = self.measure_ui_width(label, API_FIELD_META_SCALE);
        self.draw_string_scaled_stable(
            value,
            x + label_w + 4.0 * s,
            y,
            [0.82, 0.83, 0.88, 1.0],
            API_FIELD_META_SCALE,
        );
    }

    fn api_body_prop_row_layout(
        &mut self,
        w: f32,
        s: f32,
        schema: &ApiSchema,
        model: &crate::app::api_client::ApiSpecModel,
        value: &str,
    ) -> ApiFieldRowLayout {
        let allowed = api_schema_allowed_values(schema, model);
        let (choice_label, choices) = if !allowed.is_empty() {
            ("Допустимо:", allowed)
        } else if !schema.examples.is_empty() {
            ("Примеры:", schema.examples.as_slice())
        } else {
            ("", &[][..])
        };
        self.api_field_row_layout(
            w,
            s,
            value,
            api_schema_is_array_input(schema),
            if api_schema_is_file_input(schema, model) {
                64.0 * s
            } else {
                0.0
            },
            choice_label,
            choices,
            usize::from(schema.max_chars.is_some()) + usize::from(schema.default_value.is_some()),
            if allowed.is_empty() {
                0
            } else {
                schema.examples.len().min(3)
            },
        )
    }

    fn api_param_row_layout(
        &mut self,
        w: f32,
        s: f32,
        param: &ApiParam,
        value: &str,
    ) -> ApiFieldRowLayout {
        let (choice_label, choices) = if !param.enum_values.is_empty() {
            ("Допустимо:", param.enum_values.as_slice())
        } else if !param.examples.is_empty() {
            ("Примеры:", param.examples.as_slice())
        } else {
            ("", &[][..])
        };
        self.api_field_row_layout(
            w,
            s,
            value,
            matches!(
                param.primitive_type,
                crate::app::api_client::ApiPrimitiveType::Array
            ),
            0.0,
            choice_label,
            choices,
            usize::from(param.default_value.is_some()),
            0,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn api_field_row_layout(
        &mut self,
        w: f32,
        s: f32,
        value: &str,
        is_array: bool,
        pick_w: f32,
        choice_label: &str,
        choices: &[String],
        pre_choice_lines: usize,
        bottom_lines: usize,
    ) -> ApiFieldRowLayout {
        let left_w = (w * 0.30).clamp(126.0 * s, 230.0 * s);
        let gap = 20.0 * s;
        let min_input_w = 118.0 * s;
        let base_input_w = (w * 0.60).max(120.0 * s);
        let array_content_input_w = self.api_array_content_width(value, s) + 24.0 * s + pick_w;
        let desired_input_w = if is_array {
            array_content_input_w.max(base_input_w)
        } else {
            base_input_w
        };
        let has_choices = !choices.is_empty();
        let choice_full_w = if has_choices {
            self.api_choice_one_line_width(choice_label, choices, s)
        } else {
            0.0
        };
        let max_right_w = (w - left_w - min_input_w - gap).max(0.0);
        let compact_right_w = if has_choices || pre_choice_lines > 0 || bottom_lines > 0 {
            (w * 0.28).clamp(120.0 * s, 260.0 * s).min(max_right_w)
        } else {
            0.0
        };
        let one_line_right_w = if has_choices {
            choice_full_w.max(compact_right_w).min(max_right_w)
        } else {
            compact_right_w
        };
        let mut right_w = one_line_right_w;
        let mut choice_rows = if has_choices {
            self.api_choice_rows_for_width(choice_label, choices, right_w, s)
        } else {
            0
        };
        let max_input_one_line = (w - left_w - right_w - gap).max(min_input_w);
        if has_choices && is_array && array_content_input_w > max_input_one_line + 1.0 {
            right_w =
                (w - left_w - gap - array_content_input_w).clamp(compact_right_w, one_line_right_w);
            choice_rows = self.api_choice_rows_for_width(choice_label, choices, right_w, s);
        }
        let max_input_w = (w - left_w - right_w - gap).max(min_input_w);
        let input_w = desired_input_w.min(max_input_w).max(min_input_w);
        let field_w = (input_w - 16.0 * s - pick_w).max(24.0 * s);
        let array_rows = if is_array {
            self.api_array_rows_for_width(value, field_w, s)
        } else {
            1
        };
        let input_h = array_rows as f32 * 32.0 * s;
        let meta_lines = pre_choice_lines
            + choice_rows
            + bottom_lines
            + usize::from(
                !has_choices && pre_choice_lines == 0 && bottom_lines == 0 && right_w > 0.0,
            );
        let meta_h = if meta_lines == 0 {
            0.0
        } else {
            (32.0 + meta_lines.saturating_sub(1) as f32 * 20.0) * s
        };
        let row_h = (input_h + 14.0 * s).max(meta_h + 14.0 * s);
        let input_x = left_w;
        let right_x = input_x + input_w + 12.0 * s;
        ApiFieldRowLayout {
            row_h,
            input_x,
            input_w,
            input_h,
            right_x,
            right_w: (w - right_x - 8.0 * s).max(24.0 * s),
        }
    }

    fn api_choice_one_line_width(&mut self, label: &str, values: &[String], s: f32) -> f32 {
        if values.is_empty() {
            return 0.0;
        }
        let sep_w = self.measure_ui_width("┃", API_FIELD_META_SCALE) + 6.0 * s;
        let values_w = values
            .iter()
            .map(|value| self.measure_ui_width(value, API_FIELD_META_SCALE))
            .sum::<f32>();
        self.measure_ui_width(label, API_FIELD_META_SCALE)
            + 5.0 * s
            + values_w
            + values.len().saturating_sub(1) as f32 * sep_w
            + values.len() as f32 * 6.0 * s
    }

    fn api_choice_rows_for_width(
        &mut self,
        label: &str,
        values: &[String],
        w: f32,
        s: f32,
    ) -> usize {
        if values.is_empty() {
            return 0;
        }
        let label_w = self.measure_ui_width(label, API_FIELD_META_SCALE) + 5.0 * s;
        let sep_w = self.measure_ui_width("┃", API_FIELD_META_SCALE) + 6.0 * s;
        let max_x = w.max(24.0 * s);
        let mut rows = 1usize;
        let mut cx = label_w;
        for (idx, value) in values.iter().enumerate() {
            let value_w = self.measure_ui_width(value, API_FIELD_META_SCALE);
            let needs_sep = idx > 0 && cx > 1.0;
            let full_w = value_w + if needs_sep { sep_w } else { 0.0 };
            if cx + full_w > max_x {
                rows += 1;
                cx = 0.0;
                cx += value_w + 6.0 * s;
            } else {
                cx += full_w + 6.0 * s;
            }
        }
        rows
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_api_allowed_values<F>(
        &mut self,
        label: &str,
        values: &[String],
        x: f32,
        y: f32,
        w: f32,
        s: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        id_for: F,
    ) where
        F: Fn(usize) -> crate::ui_system::UiId,
    {
        self.draw_string_scaled_stable(label, x, y, [0.68, 0.70, 0.78, 1.0], API_FIELD_META_SCALE);
        let label_w = self.measure_ui_width(label, API_FIELD_META_SCALE) + 5.0 * s;
        let sep = "┃";
        let sep_w = self.measure_ui_width(sep, API_FIELD_META_SCALE) + 6.0 * s;
        let max_x = x + w;
        let line_h = 20.0 * s;
        let mut row = 0usize;
        let mut cx = x + label_w;
        for (idx, value) in values.iter().enumerate() {
            let value_w = self.measure_ui_width(value, API_FIELD_META_SCALE);
            let mut needs_sep = idx > 0 && cx > x + 1.0;
            let full_w = value_w + if needs_sep { sep_w } else { 0.0 };
            if cx + full_w > max_x {
                row += 1;
                cx = x;
                needs_sep = false;
            }
            let draw_y = y + row as f32 * line_h;
            if needs_sep {
                self.draw_string_scaled_stable(
                    sep,
                    cx,
                    draw_y,
                    [0.50, 0.54, 0.62, 1.0],
                    API_FIELD_META_SCALE,
                );
                cx += sep_w;
            }
            let hit_w = value_w.max(16.0 * s);
            if ui_registry.register_rect(
                id_for(idx),
                cx - 2.0 * s,
                draw_y - 12.0 * s,
                hit_w + 4.0 * s,
                18.0 * s,
                mx,
                my,
            ) {
                self.push_rect(
                    cx - 2.0 * s,
                    draw_y - 12.0 * s,
                    hit_w + 4.0 * s,
                    18.0 * s,
                    [1.0, 1.0, 1.0, 0.08],
                );
            }
            self.draw_string_scaled_stable(
                value,
                cx,
                draw_y,
                [0.35, 0.75, 1.0, 1.0],
                API_FIELD_META_SCALE,
            );
            cx += value_w + 6.0 * s;
        }
    }

    fn draw_api_array_value_chips(
        &mut self,
        value: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        focused: bool,
    ) {
        let mut cx = x;
        let max_x = x + w;
        let line_h = 32.0 * s;
        let focused_parts;
        let (items, draft) = if focused {
            focused_parts = api_array_edit_parts(value);
            (focused_parts.0.as_slice(), focused_parts.1)
        } else {
            focused_parts = (api_array_value_parts(value).collect::<Vec<_>>(), "");
            (focused_parts.0.as_slice(), "")
        };
        let mut row = 0usize;
        for item in items {
            let text_w = self.measure_ui_width(item, API_FIELD_META_SCALE);
            let chip_w = (text_w + 16.0 * s).max(24.0 * s);
            if cx > x && cx + chip_w > max_x {
                row += 1;
                cx = x;
            }
            let chip_y = y + row as f32 * line_h;
            if chip_y >= y + h {
                break;
            }
            let chip_h = (y + h - chip_y).min(line_h);
            self.push_rounded_rect_border(
                cx,
                chip_y,
                chip_w.min(w),
                chip_h,
                4.0 * s,
                1.0,
                [0.35, 0.75, 1.0, 0.42],
                [0.16, 0.22, 0.28, 1.0],
            );
            self.draw_string_scaled_stable(
                item,
                cx + 8.0 * s,
                api_centered_text_y(chip_y, chip_h, s),
                [0.70, 0.88, 1.0, 1.0],
                API_FIELD_META_SCALE,
            );
            cx += chip_w + 5.0 * s;
        }
        if focused && !draft.is_empty() {
            let draft_w = self.measure_ui_width(draft, API_FIELD_VALUE_SCALE);
            if cx > x && cx + draft_w > max_x {
                row += 1;
                cx = x;
            }
            let draft_y = y + row as f32 * line_h;
            if draft_y >= y + h {
                return;
            }
            self.draw_string_scaled_stable(
                draft,
                cx,
                api_centered_text_y(draft_y, line_h.min(y + h - draft_y), s),
                self.theme.fg,
                API_FIELD_VALUE_SCALE,
            );
        }
    }

    fn api_array_visual_cursor(&mut self, value: &str, max_w: f32, s: f32) -> (f32, usize) {
        let (items, draft) = api_array_edit_parts(value);
        let mut width = 0.0;
        let mut row = 0usize;
        for item in items {
            let text_w = self.measure_ui_width(item, API_FIELD_META_SCALE);
            let chip_w = (text_w + 16.0 * s).max(24.0 * s);
            if width > 0.0 && width + chip_w > max_w {
                row += 1;
                width = 0.0;
            }
            width += chip_w + 5.0 * s;
        }
        if !draft.is_empty() {
            let draft_w = self.measure_ui_width(draft, API_FIELD_VALUE_SCALE);
            if width > 0.0 && width + draft_w > max_w {
                row += 1;
                width = 0.0;
            }
            width += draft_w;
        }
        (width.min(max_w), row)
    }

    fn api_array_rows_for_width(&mut self, value: &str, max_w: f32, s: f32) -> usize {
        let (items, draft) = api_array_edit_parts(value);
        let mut rows = 1usize;
        let mut width = 0.0;
        for item in items {
            let text_w = self.measure_ui_width(item, API_FIELD_META_SCALE);
            let chip_w = (text_w + 16.0 * s).max(24.0 * s);
            if width > 0.0 && width + chip_w > max_w {
                rows += 1;
                width = 0.0;
            }
            width += chip_w + 5.0 * s;
        }
        if !draft.is_empty() {
            let draft_w = self.measure_ui_width(draft, API_FIELD_VALUE_SCALE);
            if width > 0.0 && width + draft_w > max_w {
                rows += 1;
            }
        }
        rows
    }

    fn api_array_content_width(&mut self, value: &str, s: f32) -> f32 {
        let (items, draft) = api_array_edit_parts(value);
        let mut width = 0.0;
        for item in items {
            width += (self.measure_ui_width(item, API_FIELD_META_SCALE) + 16.0 * s).max(24.0 * s)
                + 5.0 * s;
        }
        if !draft.is_empty() {
            width += self.measure_ui_width(draft, API_FIELD_VALUE_SCALE);
        }
        width
    }

    pub(crate) fn draw_api_editor_selection_one_line(
        &mut self,
        editor: &crate::editor::Editor,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        text_scale: f32,
        scroll_x: f32,
    ) {
        let Some(anchor) = editor.selection_anchor else {
            return;
        };
        if anchor == editor.cursor {
            return;
        }
        let text = editor.get_full_text();
        let start = anchor.min(editor.cursor).min(text.len());
        let end = anchor.max(editor.cursor).min(text.len());
        let sel_x = self.measure_ui_width(&text[..start], text_scale) - scroll_x;
        let sel_w = self.measure_ui_width(&text[start..end], text_scale);
        let x1 = (x + sel_x).max(x);
        let x2 = (x + sel_x + sel_w).min(x + w);
        if x2 > x1 {
            self.push_rect(x1, y, x2 - x1, h, [0.55, 0.36, 0.90, 0.36]);
        }
    }

    fn draw_api_one_line_clipped(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        w: f32,
        scroll_x: f32,
        color: [f32; 4],
        scale: f32,
    ) {
        let mut draw_x = x - scroll_x;
        let max_x = x + w;
        for ch in text.chars() {
            let adv = self
                .get_ui_glyph(ch)
                .map(|g| Self::snapped_text_advance(g.advance, scale))
                .unwrap_or(8.0);
            if draw_x >= x && draw_x + adv <= max_x {
                self.draw_string_scaled_stable(&ch.to_string(), draw_x, y, color, scale);
            }
            draw_x += adv;
            if draw_x > max_x {
                break;
            }
        }
    }

    pub(crate) fn api_editor_cursor_x_one_line(
        &mut self,
        editor: &crate::editor::Editor,
        text_scale: f32,
    ) -> f32 {
        let text = editor.get_full_text();
        let cursor = editor.cursor.min(text.len());
        self.measure_ui_width(&text[..cursor], text_scale)
    }

    fn begin_api_text_clip(
        &mut self,
        rect: (f32, f32, f32, f32),
        parent: (f32, f32, f32, f32),
    ) -> bool {
        let Some((x, y, w, h)) = api_rect_intersection(rect, parent) else {
            return false;
        };
        if w <= 0.0 || h <= 0.0 {
            return false;
        }
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
        true
    }

    fn restore_api_tab_clip(&mut self, rect: (f32, f32, f32, f32)) {
        let (x, y, w, h) = rect;
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
    }

    fn draw_api_editor_selection_multiline(
        &mut self,
        editor: &crate::editor::Editor,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        scroll_y: f32,
        scroll_x: f32,
    ) {
        let Some(anchor) = editor.selection_anchor else {
            return;
        };
        if anchor == editor.cursor {
            return;
        }
        let text = editor.get_full_text();
        let start = anchor.min(editor.cursor).min(text.len());
        let end = anchor.max(editor.cursor).min(text.len());
        let line_h = api_text_area_line_height(s);
        let scroll_y = scroll_y.clamp(
            0.0,
            crate::app::api_client::api_text_area_max_scroll(&text, h, s),
        );
        let first_line = (scroll_y / line_h).floor() as usize;
        let line_offset = scroll_y - first_line as f32 * line_h;
        let max_lines = (h / line_h).ceil().max(1.0) as usize + 1;
        let mut line_start = 0usize;
        for (line_idx, line) in text.split('\n').enumerate() {
            let line_end = line_start + line.len();
            if line_idx < first_line {
                line_start = line_end.saturating_add(1);
                continue;
            }
            let visible_idx = line_idx - first_line;
            if visible_idx >= max_lines {
                break;
            }
            let sel_start = start.max(line_start).min(line_end);
            let sel_end = end.max(line_start).min(line_end);
            let newline_selected = line_end < text.len() && start <= line_end && end > line_end;
            if sel_start < sel_end || newline_selected {
                let prefix = self.api_mono_width(&text[line_start..sel_start]) - scroll_x;
                let text_w = if sel_start < sel_end {
                    self.api_mono_width(&text[sel_start..sel_end])
                } else {
                    0.0
                };
                let raw_w = text_w + if newline_selected { 10.0 * s } else { 0.0 };
                let x1 = (x + prefix).max(x);
                let x2 = (x + prefix + raw_w).min(x + w);
                if x2 > x1 {
                    let sel_y = y - line_offset + visible_idx as f32 * line_h;
                    self.push_rect(x1, sel_y, x2 - x1, line_h, self.theme.sel);
                }
            }
            if end <= line_end {
                break;
            }
            line_start = line_end + 1;
        }
    }

    fn draw_api_editor_cursor_multiline(
        &mut self,
        editor: &crate::editor::Editor,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        scroll_y: f32,
        scroll_x: f32,
    ) {
        let text = editor.get_full_text();
        let cursor = editor.cursor.min(text.len());
        let line_h = api_text_area_line_height(s);
        let scroll_y = scroll_y.clamp(
            0.0,
            crate::app::api_client::api_text_area_max_scroll(&text, h, s),
        );
        let first_line = (scroll_y / line_h).floor() as usize;
        let line_offset = scroll_y - first_line as f32 * line_h;
        let max_lines = (h / line_h).ceil().max(1.0) as usize + 1;
        let mut line_start = 0usize;
        for (line_idx, line) in text.split('\n').enumerate() {
            let line_end = line_start + line.len();
            if cursor <= line_end {
                if line_idx < first_line {
                    return;
                }
                let visible_idx = line_idx - first_line;
                if visible_idx >= max_lines {
                    return;
                }
                let cursor_x = self.api_mono_width(&text[line_start..cursor]) - scroll_x;
                if cursor_x < -2.0 * s || cursor_x > w + 2.0 * s {
                    return;
                }
                self.push_rect(
                    x + cursor_x,
                    y - line_offset + visible_idx as f32 * line_h,
                    1.5 * s,
                    line_h,
                    self.theme.fg,
                );
                return;
            }
            line_start = line_end + 1;
        }
        if max_lines > 0 {
            self.push_rect(x, y, 1.5 * s, line_h, self.theme.fg);
        }
    }

    fn draw_json_text_area(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        scroll_y: f32,
        scroll_x: f32,
        headers: bool,
    ) {
        let line_h = api_text_area_line_height(s);
        let scroll_y = scroll_y.clamp(
            0.0,
            crate::app::api_client::api_text_area_max_scroll(text, h, s),
        );
        let first_line = (scroll_y / line_h).floor() as usize;
        let line_offset = scroll_y - first_line as f32 * line_h;
        let max_lines = (h / line_h).ceil().max(1.0) as usize + 1;
        let mut byte_idx = 0usize;
        for (line_idx, line) in text.split('\n').enumerate() {
            let line_start = byte_idx;
            let line_end = line_start + line.len();
            if line_idx < first_line {
                byte_idx = line_end.saturating_add(1);
                continue;
            }
            let visible_idx = line_idx - first_line;
            if visible_idx >= max_lines {
                break;
            }
            if headers {
                self.draw_header_lexed_line(
                    line,
                    x - scroll_x,
                    y - line_offset + visible_idx as f32 * line_h,
                    w + scroll_x,
                );
            } else {
                self.draw_json_lexed_line(
                    line,
                    x - scroll_x,
                    y - line_offset + visible_idx as f32 * line_h,
                    w + scroll_x,
                );
            }
            byte_idx = line_end.saturating_add(1);
        }
    }

    fn draw_api_text_scrollbar(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        h: f32,
        s: f32,
        scroll_y: f32,
    ) {
        let max_scroll = crate::app::api_client::api_text_area_max_scroll(text, h, s);
        if max_scroll <= 0.5 {
            return;
        }
        let track_w = (3.0 * s).max(2.0);
        self.push_rect(x, y, track_w, h, [0.52, 0.54, 0.60, 0.22]);
        let content_h = h + max_scroll;
        let thumb_h = (h / content_h * h).max(22.0 * s).min(h);
        let thumb_y = y + (scroll_y.clamp(0.0, max_scroll) / max_scroll) * (h - thumb_h);
        self.push_rect(x, thumb_y, track_w, thumb_h, [0.64, 0.66, 0.72, 0.70]);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_api_text_scrollbar_x(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        track_w: f32,
        visible_w: f32,
        scroll_x: f32,
        id: crate::ui_system::UiId,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
    ) {
        let max_scroll = api_text_area_max_scroll_x(text, visible_w, |line| {
            self.measure_ui_width(line, API_BODY_TEXT_SCALE)
        });
        if max_scroll <= 0.5 {
            return;
        }
        let track_h = 3.0_f32.max(2.0);
        self.push_rect(x, y, track_w, track_h, [0.52, 0.54, 0.60, 0.22]);
        let content_w = track_w + max_scroll;
        let thumb_w = (track_w / content_w * track_w).max(28.0).min(track_w);
        let thumb_x = x + (scroll_x.clamp(0.0, max_scroll) / max_scroll) * (track_w - thumb_w);
        self.push_rect(thumb_x, y, thumb_w, track_h, [0.64, 0.66, 0.72, 0.70]);
        ui_registry.register_rect(id, x, y - 5.0, track_w, 13.0, mx, my);
    }

    fn draw_api_mock_locked_signature_line(
        &mut self,
        path: &str,
        spans: &[crate::highlighter::ColorSpan],
        x: f32,
        y: f32,
        w: f32,
        s: f32,
    ) {
        let line_h = api_text_area_line_height(s);
        let signature = api_mock_signature_text(path);
        self.draw_python_text_area(
            &signature,
            spans,
            x,
            y + (line_h * 0.75).round(),
            w,
            api_mock_signature_block_height(path, s),
            s,
            0.0,
            0.0,
        );
        let sep_y = y + (6 + api_mock_path_param_count(path)) as f32 * line_h + 2.0 * s;
        self.push_rect(x, sep_y.round(), w, 1.0, [1.0, 1.0, 1.0, 0.08]);
    }

    fn draw_api_line_number_gutter(&mut self, x: f32, y: f32, w: f32, h: f32, s: f32) {
        let bg = [
            (self.theme.bg[0] + 0.018).min(1.0),
            (self.theme.bg[1] + 0.018).min(1.0),
            (self.theme.bg[2] + 0.022).min(1.0),
            1.0,
        ];
        self.push_rect(x, y, w, h, bg);
        self.push_rect(
            (x + w).round(),
            y,
            1.0_f32.max(s.round()),
            h.max(0.0),
            [1.0, 1.0, 1.0, 0.10],
        );
    }

    fn draw_api_editor_line_numbers(
        &mut self,
        text: &str,
        x: f32,
        w: f32,
        y: f32,
        h: f32,
        s: f32,
        scroll_y: f32,
        first_line_no: usize,
    ) {
        let line_h = api_text_area_line_height(s);
        let first_line = (scroll_y / line_h).floor() as usize;
        let line_offset = scroll_y - first_line as f32 * line_h;
        let max_lines = (h / line_h).ceil().max(1.0) as usize + 1;
        let line_count = text.split('\n').count();
        for visible_idx in 0..max_lines {
            let line_idx = first_line + visible_idx;
            if line_idx >= line_count {
                break;
            }
            let text_y = y - line_offset + visible_idx as f32 * line_h;
            self.draw_editor_line_number_centered(first_line_no + line_idx, x, w, text_y, 1.0);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_api_mock_ty_squiggles(
        &mut self,
        text: &str,
        diagnostics: &[crate::app::api_mock::ty_check::ApiMockTyDiagnostic],
        part: ApiMockSourcePart,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        scroll_y: f32,
        scroll_x: f32,
        mx: f32,
        my: f32,
    ) -> Option<(String, (f32, f32, f32, f32))> {
        let line_h = api_text_area_line_height(s);
        let first_line = (scroll_y / line_h).floor() as usize;
        let line_offset = scroll_y - first_line as f32 * line_h;
        let max_lines = (h / line_h).ceil().max(1.0) as usize + 1;
        let mut hovered = None;
        for diag in diagnostics {
            if diag.part != part || diag.line < first_line {
                continue;
            }
            let visible_idx = diag.line - first_line;
            if visible_idx >= max_lines {
                continue;
            }
            let Some(line) = text.split('\n').nth(diag.line) else {
                continue;
            };
            let start_byte = byte_offset_for_char_col(line, diag.start_col);
            let end_byte = byte_offset_for_char_col(line, diag.end_col);
            let x_start = x + self.api_mono_width(&line[..start_byte]) - scroll_x;
            let x_end = x + self.api_mono_width(&line[..end_byte]) - scroll_x;
            let base_y = y - line_offset + visible_idx as f32 * line_h;
            let line_top = base_y - 19.0 * s;
            let squiggle_y = base_y + 3.0 * s;
            let squiggle_w = (x_end - x_start).max(8.0 * s).min(w);
            self.push_squiggle(
                x_start.round(),
                squiggle_y.round(),
                squiggle_w,
                [1.0, 0.36, 0.36, 1.0],
            );
            let hit_top = base_y - 14.0 * s;
            if mx >= x_start
                && mx <= x_start + squiggle_w
                && my >= hit_top
                && my <= hit_top + line_h
            {
                hovered = Some((
                    diag.message.clone(),
                    (x_start.round(), line_top.round(), squiggle_w, line_h),
                ));
            }
        }
        hovered
    }

    fn draw_api_mock_ty_popup(
        &mut self,
        message: &str,
        rect: (f32, f32, f32, f32),
        editor: &crate::editor::Editor,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
    ) {
        let text = message.lines().next().unwrap_or(message);
        let (text, spans, line_kinds, inline_code_ranges) = crate::lsp::highlight_hover_text(text);
        let source_anchor_x = rect.0 + rect.2 * 0.5;
        let source_anchor_y = rect.1 + rect.3 * 0.5;
        let scroll = crate::app::mouse::HOVER_STATE.with(|state| {
            state
                .borrow()
                .popup
                .as_ref()
                .filter(|popup| {
                    popup.byte_offset == API_MOCK_TY_POPUP_BYTE && popup.text == text
                })
                .map(|popup| popup.scroll.clone())
                .unwrap_or_else(|| crate::scroll::ScrollState::new(15.0))
        });
        let mut popup = crate::app::mouse::HoverPopup {
            text,
            spans,
            line_kinds,
            inline_code_ranges,
            byte_offset: API_MOCK_TY_POPUP_BYTE,
            anchor_x: source_anchor_x,
            anchor_y: source_anchor_y,
            offset_x: None,
            offset_y: None,
            anim_progress: 1.0,
            scroll,
            layout_cache: None,
        };
        let render_scroll_y =
            self.api_mock_ty_hover_render_scroll_y(editor, popup.byte_offset, rect.1);
        let mut wants_pointer = false;
        let (bx, by, bw, bh, max_scroll) = self.draw_hover_popup(
            &mut popup,
            None,
            None,
            editor,
            ui_registry,
            mx,
            my,
            render_scroll_y,
            &mut wants_pointer,
            1.0,
            None,
        );
        crate::app::mouse::HOVER_STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.byte_offset = Some(API_MOCK_TY_POPUP_BYTE);
            state.put_type_popup_after_draw(Some(popup), Some((bx, by, bw, bh)), max_scroll);
        });
    }

    fn api_mock_ty_hover_render_scroll_y(
        &self,
        editor: &crate::editor::Editor,
        byte_offset: usize,
        source_line_top: f32,
    ) -> f32 {
        let phys_line = editor
            .line_offsets
            .partition_point(|&offset| offset <= byte_offset)
            .saturating_sub(1);
        let vis_line_idx = self.phys_to_visual.get(phys_line).copied().unwrap_or(0) as f32;
        vis_line_idx * self.line_height - source_line_top
    }

    fn draw_existing_api_mock_ty_popup(
        &mut self,
        editor: &crate::editor::Editor,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
    ) -> bool {
        let should_draw = crate::app::mouse::HOVER_STATE.with(|state| {
            let state = state.borrow();
            state
                .popup
                .as_ref()
                .is_some_and(|popup| popup.byte_offset == API_MOCK_TY_POPUP_BYTE)
                && state
                    .popup_or_bridge_contains(mx, my, self.width, self.scale_factor)
                    .0
        });
        if !should_draw {
            crate::app::mouse::HOVER_STATE.with(|state| {
                let mut state = state.borrow_mut();
                if state
                    .popup
                    .as_ref()
                    .is_some_and(|popup| popup.byte_offset == API_MOCK_TY_POPUP_BYTE)
                {
                    state.put_type_popup_after_draw(None, None, 0.0);
                    state.byte_offset = None;
                }
            });
            return false;
        }
        let Some(mut popup) = crate::app::mouse::HOVER_STATE.with(|state| {
            let mut state = state.borrow_mut();
            let (popup, _, _) = state.take_type_popup_for_draw(false);
            popup
        }) else {
            return false;
        };
        let render_scroll_y = self.api_mock_ty_hover_render_scroll_y(
            editor,
            popup.byte_offset,
            popup.anchor_y - api_text_area_line_height(self.scale_factor) * 0.5,
        );
        let mut wants_pointer = false;
        let (bx, by, bw, bh, max_scroll) = self.draw_hover_popup(
            &mut popup,
            None,
            None,
            editor,
            ui_registry,
            mx,
            my,
            render_scroll_y,
            &mut wants_pointer,
            1.0,
            None,
        );
        crate::app::mouse::HOVER_STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.byte_offset = Some(API_MOCK_TY_POPUP_BYTE);
            state.put_type_popup_after_draw(Some(popup), Some((bx, by, bw, bh)), max_scroll);
        });
        true
    }

    fn draw_python_text_area(
        &mut self,
        text: &str,
        spans: &[crate::highlighter::ColorSpan],
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        scroll_y: f32,
        scroll_x: f32,
    ) {
        let line_h = api_text_area_line_height(s);
        let scroll_y = scroll_y.clamp(
            0.0,
            crate::app::api_client::api_text_area_max_scroll(text, h, s),
        );
        let first_line = (scroll_y / line_h).floor() as usize;
        let line_offset = scroll_y - first_line as f32 * line_h;
        let max_lines = (h / line_h).ceil().max(1.0) as usize + 1;
        let mut byte_idx = 0usize;
        for (line_idx, line) in text.split('\n').enumerate() {
            let line_start = byte_idx;
            let line_end = line_start + line.len();
            if line_idx < first_line {
                byte_idx = line_end.saturating_add(1);
                continue;
            }
            let visible_idx = line_idx - first_line;
            if visible_idx >= max_lines {
                break;
            }
            let draw_y = y - line_offset + visible_idx as f32 * line_h;
            self.draw_spanned_api_line(
                line,
                spans,
                line_start,
                x - scroll_x,
                draw_y,
                w + scroll_x,
            );
            byte_idx = line_end.saturating_add(1);
        }
    }

    fn draw_spanned_api_line(
        &mut self,
        line: &str,
        spans: &[crate::highlighter::ColorSpan],
        base_offset: usize,
        x: f32,
        y: f32,
        w: f32,
    ) {
        let mut draw_x = x;
        let mut offset = base_offset;
        let mut span_idx = match spans.binary_search_by_key(&base_offset, |s| s.start) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };
        for ch in line.chars() {
            if draw_x > x + w {
                break;
            }
            let adv = self.char_advance(ch);
            if ch != ' ' && ch != '\t'
                && let Some(g) = self.get_glyph(ch)
            {
                while span_idx < spans.len() && spans[span_idx].end <= offset {
                    span_idx += 1;
                }
                let color = if span_idx < spans.len() && spans[span_idx].start <= offset {
                    spans[span_idx].color
                } else {
                    self.theme.fg
                };
                self.push_quad(
                    draw_x + g.offset_x,
                    y - g.offset_y,
                    g.width,
                    g.height,
                    g.u,
                    g.v,
                    g.uw,
                    g.vh,
                    color,
                    g.is_emoji,
                );
            }
            draw_x += adv;
            offset = offset.saturating_add(ch.len_utf8());
        }
    }

    fn draw_json_lexed_line(&mut self, line: &str, x: f32, y: f32, w: f32) {
        let mut draw_x = x;
        let bytes = line.as_bytes();
        let mut idx = 0usize;
        while idx < line.len() {
            if draw_x > x + w {
                break;
            }
            let b = bytes[idx];
            if b == b'"' {
                let end = json_string_end(line, idx);
                let color = if json_string_is_property(line, end) {
                    crate::highlighter::DRACULA_CYAN
                } else {
                    crate::highlighter::DRACULA_YELLOW
                };
                self.draw_json_colored_segment(&line[idx..end], color, x, y, w, &mut draw_x);
                idx = end;
                continue;
            }
            if b == b'-' || b.is_ascii_digit() {
                let end = json_number_end(line, idx);
                self.draw_json_colored_segment(
                    &line[idx..end],
                    crate::highlighter::DRACULA_PURPLE,
                    x,
                    y,
                    w,
                    &mut draw_x,
                );
                idx = end;
                continue;
            }
            if let Some(end) = json_keyword_end(line, idx) {
                self.draw_json_colored_segment(
                    &line[idx..end],
                    crate::highlighter::DRACULA_PINK,
                    x,
                    y,
                    w,
                    &mut draw_x,
                );
                idx = end;
                continue;
            }
            let ch = line[idx..].chars().next().unwrap_or(' ');
            let end = idx + ch.len_utf8();
            self.draw_json_colored_segment(&line[idx..end], self.theme.fg, x, y, w, &mut draw_x);
            idx = end;
        }
    }

    fn draw_header_lexed_line(&mut self, line: &str, x: f32, y: f32, w: f32) {
        let Some(colon_idx) = line.find(':') else {
            let mut draw_x = x;
            self.draw_json_colored_segment(line, [0.70, 0.72, 0.78, 1.0], x, y, w, &mut draw_x);
            return;
        };
        let (key, rest) = line.split_at(colon_idx);
        let value_start = rest
            .as_bytes()
            .iter()
            .position(|b| !matches!(*b, b':' | b' ' | b'\t'))
            .unwrap_or(rest.len());
        let mut draw_x = x;
        self.draw_json_colored_segment(key, [1.0, 0.68, 0.26, 1.0], x, y, w, &mut draw_x);
        self.draw_json_colored_segment(
            &rest[..value_start],
            [0.86, 0.87, 0.91, 1.0],
            x,
            y,
            w,
            &mut draw_x,
        );
        let value = &rest[value_start..];
        let value_color = if header_value_is_number(value) {
            crate::highlighter::DRACULA_PURPLE
        } else {
            [0.70, 0.72, 0.78, 1.0]
        };
        self.draw_json_colored_segment(value, value_color, x, y, w, &mut draw_x);
    }

    fn draw_json_colored_segment(
        &mut self,
        segment: &str,
        color: [f32; 4],
        x: f32,
        y: f32,
        w: f32,
        draw_x: &mut f32,
    ) {
        for ch in segment.chars() {
            if *draw_x > x + w {
                break;
            }
            let mut buf = [0u8; 4];
            self.draw_string_scaled_stable(
                ch.encode_utf8(&mut buf),
                *draw_x,
                y,
                color,
                API_BODY_TEXT_SCALE,
            );
            *draw_x += self
                .get_ui_glyph(ch)
                .map(|g| Self::snapped_text_advance(g.advance, API_BODY_TEXT_SCALE))
                .unwrap_or(8.0);
        }
    }
}

fn api_rect_intersection(
    a: (f32, f32, f32, f32),
    b: (f32, f32, f32, f32),
) -> Option<(f32, f32, f32, f32)> {
    let x1 = a.0.max(b.0);
    let y1 = a.1.max(b.1);
    let x2 = (a.0 + a.2).min(b.0 + b.2);
    let y2 = (a.1 + a.3).min(b.1 + b.3);
    (x2 > x1 && y2 > y1).then_some((x1, y1, x2 - x1, y2 - y1))
}

fn api_centered_text_y(y: f32, h: f32, scale: f32) -> f32 {
    y + h * 0.5 + 4.5 * scale
}

fn api_split_label_text_y(y: f32, h: f32, scale: f32, bottom: bool) -> f32 {
    y + h * if bottom { 0.74 } else { 0.30 } + 4.5 * scale
}

fn response_auth_token_flags(response: &crate::app::api_client::ApiJobResponse) -> (bool, bool) {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&response.body) else {
        return (false, false);
    };
    (
        json.get("access_token")
            .and_then(serde_json::Value::as_str)
            .is_some(),
        json.get("refresh_token")
            .and_then(serde_json::Value::as_str)
            .is_some(),
    )
}

fn byte_offset_for_char_col(line: &str, col: usize) -> usize {
    line.char_indices()
        .nth(col)
        .map(|(idx, _)| idx)
        .unwrap_or(line.len())
}

fn path_param_names(path: &str) -> Vec<String> {
    path.split('/')
        .filter_map(|part| {
            part.strip_prefix('{')
                .and_then(|part| part.strip_suffix('}'))
                .map(str::to_string)
        })
        .collect()
}

fn sanitize_python_param(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() || out.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        out.insert(0, '_');
    }
    if matches!(out.as_str(), "req" | "query" | "body" | "fields") {
        out.push_str("_param");
    }
    out
}

fn json_string_end(line: &str, start: usize) -> usize {
    let bytes = line.as_bytes();
    let mut idx = start.saturating_add(1);
    let mut escaped = false;
    while idx < bytes.len() {
        let b = bytes[idx];
        if escaped {
            escaped = false;
        } else if b == b'\\' {
            escaped = true;
        } else if b == b'"' {
            return idx + 1;
        }
        idx += 1;
    }
    line.len()
}

fn json_string_is_property(line: &str, string_end: usize) -> bool {
    let bytes = line.as_bytes();
    let mut idx = string_end;
    while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
        idx += 1;
    }
    bytes.get(idx).is_some_and(|b| *b == b':')
}

fn json_number_end(line: &str, start: usize) -> usize {
    let bytes = line.as_bytes();
    let mut idx = start;
    while idx < bytes.len()
        && (bytes[idx].is_ascii_digit() || matches!(bytes[idx], b'-' | b'+' | b'.' | b'e' | b'E'))
    {
        idx += 1;
    }
    idx.max(start + 1)
}

fn json_keyword_end(line: &str, start: usize) -> Option<usize> {
    for kw in ["true", "false", "null"] {
        let end = start + kw.len();
        if line.get(start..end) == Some(kw) && json_token_boundary(line, end) {
            return Some(end);
        }
    }
    None
}

fn json_token_boundary(line: &str, idx: usize) -> bool {
    line.as_bytes()
        .get(idx)
        .is_none_or(|b| !b.is_ascii_alphanumeric() && *b != b'_')
}

fn header_value_is_number(value: &str) -> bool {
    let value = value.trim();
    value.bytes().any(|b| b.is_ascii_digit()) && value.parse::<f64>().is_ok()
}

fn api_param_type_text(param: &ApiParam) -> String {
    if matches!(
        param.primitive_type,
        crate::app::api_client::ApiPrimitiveType::Array
    ) {
        let item = param
            .item_type
            .map(api_primitive_type_text)
            .unwrap_or("any");
        return format!("array<{item}>");
    }
    if !param.enum_values.is_empty() {
        "enum".to_string()
    } else {
        api_primitive_type_text(param.primitive_type).to_string()
    }
}

fn api_primitive_type_text(kind: crate::app::api_client::ApiPrimitiveType) -> &'static str {
    api_schema_type_text(match kind {
        crate::app::api_client::ApiPrimitiveType::String => ApiSchemaKind::String,
        crate::app::api_client::ApiPrimitiveType::Date => ApiSchemaKind::Date,
        crate::app::api_client::ApiPrimitiveType::DateTime => ApiSchemaKind::DateTime,
        crate::app::api_client::ApiPrimitiveType::Integer => ApiSchemaKind::Integer,
        crate::app::api_client::ApiPrimitiveType::Number => ApiSchemaKind::Number,
        crate::app::api_client::ApiPrimitiveType::Boolean => ApiSchemaKind::Boolean,
        crate::app::api_client::ApiPrimitiveType::Array => ApiSchemaKind::Array,
        crate::app::api_client::ApiPrimitiveType::Object => ApiSchemaKind::Object,
        crate::app::api_client::ApiPrimitiveType::Bytes => ApiSchemaKind::Bytes,
        crate::app::api_client::ApiPrimitiveType::Unknown => ApiSchemaKind::Unknown,
    })
}

fn api_schema_type_text(kind: ApiSchemaKind) -> &'static str {
    match kind {
        ApiSchemaKind::Object => "object",
        ApiSchemaKind::Array => "array",
        ApiSchemaKind::String => "string",
        ApiSchemaKind::Date => "date",
        ApiSchemaKind::DateTime => "date-time",
        ApiSchemaKind::Integer => "int",
        ApiSchemaKind::Number => "number",
        ApiSchemaKind::Boolean => "bool",
        ApiSchemaKind::Bytes => "bytes",
        ApiSchemaKind::Unknown => "any",
    }
}

fn api_body_schema_type_text(
    schema: &ApiSchema,
    model: &crate::app::api_client::ApiSpecModel,
) -> String {
    if api_schema_is_multi_file_input(schema, model) {
        "files".to_string()
    } else if matches!(schema.kind, ApiSchemaKind::Bytes) {
        "file".to_string()
    } else if matches!(schema.kind, ApiSchemaKind::Array) {
        if let Some(item) = schema.item.and_then(|item| model.schema_arena.get(item.0)) {
            format!("array<{}>", api_schema_type_text(item.kind))
        } else {
            "array<any>".to_string()
        }
    } else if !api_schema_allowed_values(schema, model).is_empty() {
        "enum".to_string()
    } else {
        api_schema_type_text(schema.kind).to_string()
    }
}

fn api_status_color(status: Option<u16>) -> [f32; 4] {
    match status {
        Some(200..=399) => [0.48, 0.86, 0.52, 1.0],
        Some(400..=499) => [0.35, 0.75, 1.0, 1.0],
        Some(500..=599) => [1.0, 0.42, 0.42, 1.0],
        _ => [0.68, 0.70, 0.78, 1.0],
    }
}
