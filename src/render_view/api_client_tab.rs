use crate::app::api_client::{json_body_is_valid, ApiFocus, ApiPrimitiveType};
use crate::renderer::Renderer;
use crate::widgets::{Button, IconType};
use glow::HasContext;

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_api_client_tab(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        ide_panel: &crate::app::IdePanelState,
        tab_meta: &crate::app::api_client::ApiClientTabMeta,
        tab_state: &crate::app::api_client::ApiClientTabState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        blink_alpha: f32,
    ) {
        self.push_rect(x, y, w, h, self.theme.bg);
        ui_registry.register_rect(crate::ui_system::UiId::ApiTabBody, x, y, w, h, mx, my);
        let Some(model) = ide_panel.api.models.get(&tab_meta.spec_id) else {
            self.draw_string_scaled(
                "Спецификация загружается или cache пустой",
                x + 28.0 * s,
                y + 46.0 * s,
                [0.72, 0.74, 0.82, 1.0],
                0.95,
            );
            return;
        };
        let Some(route_idx) = tab_state.route_idx.or_else(|| (!model.routes.is_empty()).then_some(0))
        else {
            self.draw_string_scaled(
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

        let pad = 28.0 * s;
        let content_w = (w - pad * 2.0).max(1.0);
        let scroll = tab_state.tab_scroll.current.round();
        let mut cy = y + pad - scroll;

        let method_w = 64.0 * s;
        self.push_rounded_rect(
            x + pad,
            cy,
            method_w,
            30.0 * s,
            5.0 * s,
            crate::render_view::api_client_panel::method_color(route.method),
        );
        self.draw_string_scaled(
            route.method.as_str(),
            x + pad + 12.0 * s,
            cy + 21.0 * s,
            [0.04, 0.05, 0.07, 1.0],
            0.78,
        );
        self.draw_string_scaled(
            &route.path,
            x + pad + method_w + 12.0 * s,
            cy + 23.0 * s,
            self.theme.fg,
            1.06,
        );
        cy += 38.0 * s;
        if !route.summary.is_empty() {
            self.draw_string_scaled(
                &route.summary,
                x + pad,
                cy + 18.0 * s,
                [0.68, 0.70, 0.78, 1.0],
                0.78,
            );
            cy += 28.0 * s;
        }

        self.draw_api_section_title("Server", x + pad, cy + 18.0 * s, s);
        cy += 28.0 * s;
        let mut sx = x + pad;
        for (idx, server) in model.servers.iter().enumerate() {
            let label = server.url.as_str();
            let chip_w = (self.measure_ui_width(label, 0.72) + 20.0 * s)
                .max(72.0 * s)
                .min(content_w);
            if sx + chip_w > x + pad + content_w {
                sx = x + pad;
                cy += 30.0 * s;
            }
            let active = idx == tab_state.server_idx;
            self.push_rounded_rect(
                sx,
                cy,
                chip_w,
                24.0 * s,
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
                24.0 * s,
                mx,
                my,
            );
            self.draw_string_scaled(label, sx + 10.0 * s, cy + 17.0 * s, self.theme.fg, 0.72);
            sx += chip_w + 8.0 * s;
        }
        cy += 42.0 * s;

        if !route.path_params.is_empty() {
            self.draw_api_section_title("Path params", x + pad, cy + 18.0 * s, s);
            cy += 28.0 * s;
            for (param_idx, param) in route.path_params.iter().enumerate() {
                cy = self.draw_api_param_input(
                    x + pad,
                    cy,
                    content_w,
                    s,
                    route_idx,
                    param_idx,
                    &param.name,
                    param.required,
                    param.primitive_type,
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
            self.draw_api_section_title("Query params", x + pad, cy + 18.0 * s, s);
            cy += 28.0 * s;
            for (param_idx, param) in route.query_params.iter().enumerate() {
                cy = self.draw_api_param_input(
                    x + pad,
                    cy,
                    content_w,
                    s,
                    route_idx,
                    param_idx,
                    &param.name,
                    param.required,
                    param.primitive_type,
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
            let valid = body.is_multipart || json_body_is_valid(&tab_state.body_json);
            self.draw_string_scaled(
                if valid { "valid" } else { "invalid JSON" },
                x + pad + 58.0 * s,
                cy + 18.0 * s,
                if valid {
                    [0.48, 0.86, 0.52, 1.0]
                } else {
                    [1.0, 0.42, 0.42, 1.0]
                },
                0.70,
            );
            cy += 28.0 * s;
            if body.is_multipart {
                self.draw_string_scaled(
                    "multipart/form-data fields",
                    x + pad,
                    cy + 18.0 * s,
                    [0.68, 0.70, 0.78, 1.0],
                    0.74,
                );
                cy += 28.0 * s;
                if let Some(schema_ref) = body.schema
                    && let Some(schema) = model.schema_arena.get(schema_ref.0)
                {
                    for prop in &schema.properties {
                        self.draw_string_scaled(
                            &prop.name,
                            x + pad + 10.0 * s,
                            cy + 19.0 * s,
                            self.theme.fg,
                            0.74,
                        );
                        cy += 26.0 * s;
                    }
                }
            } else {
                let body_h = 220.0 * s;
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
                let body_text = if matches!(
                    ide_panel.api.focused,
                    Some(ApiFocus::Body { spec_id, route_idx: f_route })
                        if spec_id == tab_meta.spec_id && f_route == route_idx
                ) {
                    ide_panel.api.input_editor.get_full_text()
                } else {
                    tab_state.body_json.clone()
                };
                self.draw_json_text_area(&body_text, x + pad + 10.0 * s, cy + 22.0 * s, content_w - 20.0 * s, body_h - 16.0 * s, s);
                if matches!(
                    ide_panel.api.focused,
                    Some(ApiFocus::Body { spec_id, route_idx: f_route })
                        if spec_id == tab_meta.spec_id && f_route == route_idx
                ) && blink_alpha > 0.15
                {
                    self.push_rect(
                        x + pad + 12.0 * s,
                        cy + body_h - 20.0 * s,
                        1.5 * s,
                        14.0 * s,
                        self.theme.fg,
                    );
                }
                cy += body_h + 16.0 * s;
            }
        }

        let try_btn = Button {
            x: x + pad,
            y: cy,
            w: 112.0 * s,
            h: 34.0 * s,
            text: if tab_state.pending {
                "Pending".to_string()
            } else {
                "Try".to_string()
            },
            icon: Some(IconType::Reload),
            text_scale: 0.82,
            icon_size: 18.0 * s,
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

        self.draw_api_section_title("Response", x + pad, cy + 18.0 * s, s);
        cy += 30.0 * s;
        if let Some(response) = &tab_state.response {
            if let Some(err) = &response.error {
                self.draw_string_scaled(
                    &err.message,
                    x + pad,
                    cy + 18.0 * s,
                    [1.0, 0.42, 0.42, 1.0],
                    0.78,
                );
            } else {
                let status_text = response
                    .status
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "-".to_string());
                self.draw_string_scaled(
                    &status_text,
                    x + pad,
                    cy + 18.0 * s,
                    [0.48, 0.86, 0.52, 1.0],
                    0.82,
                );
                self.draw_string_scaled(
                    "ms",
                    x + pad + 62.0 * s,
                    cy + 18.0 * s,
                    [0.68, 0.70, 0.78, 1.0],
                    0.70,
                );
                let elapsed = response.elapsed_ms.to_string();
                self.draw_string_scaled(
                    &elapsed,
                    x + pad + 84.0 * s,
                    cy + 18.0 * s,
                    self.theme.fg,
                    0.78,
                );
                cy += 28.0 * s;
                let resp_h = 180.0 * s;
                self.push_rounded_rect(
                    x + pad,
                    cy,
                    content_w,
                    resp_h,
                    6.0 * s,
                    [0.12, 0.13, 0.17, 1.0],
                );
                self.draw_json_text_area(
                    &response.body,
                    x + pad + 10.0 * s,
                    cy + 22.0 * s,
                    content_w - 20.0 * s,
                    resp_h - 16.0 * s,
                    s,
                );
                if response.truncated {
                    self.draw_string_scaled(
                        "truncated",
                        x + pad + content_w - 86.0 * s,
                        cy + 18.0 * s,
                        [1.0, 0.76, 0.32, 1.0],
                        0.68,
                    );
                }
            }
        } else if tab_state.pending {
            self.draw_string_scaled(
                "Запрос выполняется",
                x + pad,
                cy + 18.0 * s,
                [0.68, 0.70, 0.78, 1.0],
                0.78,
            );
        }

        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }
    }

    fn draw_api_section_title(&mut self, text: &str, x: f32, y: f32, s: f32) {
        self.draw_string_scaled(text, x, y, [0.74, 0.76, 0.84, 1.0], 0.80);
        self.push_rect(x, y + 6.0 * s, 120.0 * s, 1.0, [1.0, 1.0, 1.0, 0.10]);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_api_param_input(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        s: f32,
        _route_idx: usize,
        _param_idx: usize,
        name: &str,
        required: bool,
        ty: ApiPrimitiveType,
        value: &str,
        focused: bool,
        editor: &crate::editor::Editor,
        blink_alpha: f32,
        id: crate::ui_system::UiId,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
    ) -> f32 {
        let row_h = 40.0 * s;
        let name_w = (w * 0.32).clamp(80.0 * s, 180.0 * s);
        self.draw_string_scaled(name, x, y + 24.0 * s, self.theme.fg, 0.76);
        if required {
            self.draw_string_scaled("*", x + name_w - 18.0 * s, y + 22.0 * s, [1.0, 0.42, 0.42, 1.0], 0.78);
        }
        let type_text = match ty {
            ApiPrimitiveType::String => "string",
            ApiPrimitiveType::Integer => "int",
            ApiPrimitiveType::Number => "number",
            ApiPrimitiveType::Boolean => "bool",
            ApiPrimitiveType::Array => "array",
            ApiPrimitiveType::Object => "object",
            ApiPrimitiveType::Unknown => "any",
        };
        self.draw_string_scaled(type_text, x + name_w - 58.0 * s, y + 24.0 * s, [0.58, 0.61, 0.70, 1.0], 0.64);
        let input_x = x + name_w;
        let input_w = (w - name_w).max(80.0 * s);
        self.push_rounded_rect_border(
            input_x,
            y + 5.0 * s,
            input_w,
            30.0 * s,
            5.0 * s,
            (1.0 * s).max(1.0),
            if focused {
                [0.60, 0.35, 0.85, 1.0]
            } else {
                [1.0, 1.0, 1.0, 0.12]
            },
            [0.13, 0.14, 0.18, 1.0],
        );
        ui_registry.register_text_input(id, input_x, y + 5.0 * s, input_w, 30.0 * s, mx, my);
        let shown = if focused {
            editor.get_full_text()
        } else {
            value.to_string()
        };
        self.draw_string_scaled(&shown, input_x + 8.0 * s, y + 25.0 * s, self.theme.fg, 0.76);
        if focused && blink_alpha > 0.15 {
            let text_w = self.measure_ui_width(&shown, 0.76).min(input_w - 16.0 * s);
            self.push_rect(
                input_x + 8.0 * s + text_w,
                y + 10.0 * s,
                1.5 * s,
                20.0 * s,
                self.theme.fg,
            );
        }
        y + row_h
    }

    fn draw_json_text_area(&mut self, text: &str, x: f32, y: f32, w: f32, h: f32, s: f32) {
        let line_h = 18.0 * s;
        let max_lines = (h / line_h).floor().max(1.0) as usize;
        for (line_idx, line) in text.lines().take(max_lines).enumerate() {
            self.draw_json_colored_line(line, x, y + line_idx as f32 * line_h, w, s);
        }
    }

    fn draw_json_colored_line(&mut self, line: &str, x: f32, y: f32, w: f32, _s: f32) {
        let mut draw_x = x;
        let mut in_string = false;
        let mut escaped = false;
        for ch in line.chars() {
            if draw_x > x + w {
                break;
            }
            let color = if in_string {
                [0.62, 0.86, 0.64, 1.0]
            } else if matches!(ch, '{' | '}' | '[' | ']' | ':' | ',') {
                [0.78, 0.58, 1.0, 1.0]
            } else if ch.is_ascii_digit() || ch == '-' {
                [1.0, 0.76, 0.32, 1.0]
            } else {
                self.theme.fg
            };
            let mut buf = [0u8; 4];
            let s_ch = ch.encode_utf8(&mut buf);
            self.draw_string_scaled(s_ch, draw_x, y, color, 0.76);
            draw_x += self
                .get_ui_glyph(ch)
                .map(|g| g.advance * 0.76)
                .unwrap_or(8.0);
            if ch == '"' && !escaped {
                in_string = !in_string;
            }
            escaped = ch == '\\' && !escaped;
            if ch != '\\' {
                escaped = false;
            }
        }
    }
}
