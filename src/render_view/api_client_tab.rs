use crate::app::api_client::{
    API_BODY_TEXT_SCALE, ApiFocus, ApiParam, ApiResponseView, ApiSchema, ApiSchemaKind,
    ApiSecuritySchemeKind, api_auth_scheme_row_height, api_body_prop_row_height,
    api_param_row_height, api_response_text, api_route_auth_missing,
    api_route_auth_scheme_indices, api_schema_is_file_input,
    api_schema_is_multi_file_input, api_text_area_line_height, json_body_is_valid,
    write_api_path_display,
};
use crate::renderer::Renderer;
use crate::widgets::{Button, IconType};
use glow::HasContext;

const API_SECTION_TITLE_SCALE: f32 = 0.92;
const API_FIELD_NAME_SCALE: f32 = 0.94;
const API_FIELD_TYPE_SCALE: f32 = 0.84;
const API_FIELD_VALUE_SCALE: f32 = 0.88;
const API_FIELD_META_SCALE: f32 = 0.78;

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
        ui_registry.register_blocker(crate::ui_system::UiId::ApiTabBody, x, y, w, h, mx, my);
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

            self.draw_string_scaled_stable("Auth", x + pad, cy + 24.0 * s, self.theme.fg, 1.18);
            cy += 38.0 * s;
            if model.security_schemes.is_empty() {
                self.draw_string_scaled_stable(
                    "No auth schemes",
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

        self.draw_api_section_title("Server", x + pad, cy + 18.0 * s, s);
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
            self.draw_api_section_title("Auth", x + pad, cy + 18.0 * s, s);
            if api_route_auth_missing(model, route, &ide_panel.api.auth) {
                self.draw_string_scaled_stable(
                    "missing auth",
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
            self.draw_api_section_title("Path params", x + pad, cy + 18.0 * s, s);
            cy += 28.0 * s;
            let table_h = route
                .path_params
                .iter()
                .map(|param| api_param_row_height(param, s))
                .sum::<f32>();
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
            let table_h = route
                .query_params
                .iter()
                .map(|param| api_param_row_height(param, s))
                .sum::<f32>();
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
            let valid = body.is_multipart
                || body.is_form_urlencoded
                || if body_focused {
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
                if valid { "valid" } else { "invalid JSON" },
                x + pad + 52.0 * s + body_type_w + 12.0 * s,
                cy + 18.0 * s,
                if valid {
                    [0.48, 0.86, 0.52, 1.0]
                } else {
                    [1.0, 0.42, 0.42, 1.0]
                },
                0.92,
            );
            cy += 28.0 * s;
            if body.is_multipart || body.is_form_urlencoded {
                if let Some(schema_ref) = body.schema
                    && let Some(schema) = model.schema_arena.get(schema_ref.0)
                {
                    let table_h = schema
                        .properties
                        .iter()
                        .filter_map(|prop| model.schema_arena.get(prop.schema.0))
                        .map(|schema| api_body_prop_row_height(schema, s))
                        .sum::<f32>();
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
                                &ide_panel.api.input_editor,
                                blink_alpha,
                                ui_registry,
                                mx,
                                my,
                            );
                            cy += row_h;
                        }
                    }
                }
            } else {
                let body_h = 300.0 * s;
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
                let body_text = if body_focused {
                    ide_panel.api.input_editor.get_full_text()
                } else {
                    tab_state.body_json.clone()
                };
                let body_clip = (
                    x + pad + 2.0 * s,
                    cy + 2.0 * s,
                    content_w - 4.0 * s,
                    body_h - 4.0 * s,
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
                        self.draw_api_editor_cursor_multiline(
                            &ide_panel.api.input_editor,
                            x + pad + 10.0 * s,
                            cy + 10.0 * s,
                            content_w - 20.0 * s,
                            body_h - 16.0 * s,
                            s,
                            tab_state.body_scroll.current,
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
            w: 126.0 * s,
            h: 38.0 * s,
            text: if tab_state.pending {
                "Pending".to_string()
            } else {
                "Try".to_string()
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

        self.draw_api_section_title("Response", x + pad, cy + 18.0 * s, s);
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
                let resp_h = 180.0 * s;
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
                let resp_clip = (
                    x + pad + 2.0 * s,
                    cy + 2.0 * s,
                    content_w - 4.0 * s,
                    resp_h - 4.0 * s,
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
                        self.draw_api_editor_cursor_multiline(
                            &ide_panel.api.input_editor,
                            x + pad + 10.0 * s,
                            cy + 10.0 * s,
                            content_w - 20.0 * s,
                            resp_h - 16.0 * s,
                            s,
                            tab_state.response_scroll.current,
                        );
                    }
                    self.restore_api_tab_clip(tab_clip);
                }
                if response.truncated {
                    self.draw_string_scaled_stable(
                        "truncated",
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
            y,
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

    fn draw_api_dynamic_table_frame(&mut self, x: f32, y: f32, w: f32, h: f32, s: f32) {
        if h <= 0.0 {
            return;
        }
        let line = [1.0, 1.0, 1.0, 0.13];
        self.push_rect(x, y, w, h, [0.12, 0.13, 0.17, 1.0]);
        self.push_rect(x, y, w, (1.0 * s).max(1.0), line);
        self.push_rect(x, y + h, w, (1.0 * s).max(1.0), line);
        self.push_rect(x, y, (1.0 * s).max(1.0), h, line);
        self.push_rect(x + w, y, (1.0 * s).max(1.0), h, line);
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
                true,
                &ide_panel.api.input_editor,
                blink_alpha,
                crate::ui_system::UiId::ApiAuthPassword(scheme_idx),
                ui_registry,
                mx,
                my,
            );
        } else {
            let focused = matches!(
                ide_panel.api.focused,
                Some(ApiFocus::AuthValue { spec_id: f_spec, scheme: ref focused_scheme })
                    if f_spec == spec_id && focused_scheme == &scheme.name
            );
            let value = entry
                .map(|entry| {
                    if !entry.access_token.is_empty() {
                        entry.access_token.as_str()
                    } else {
                        entry.value.as_str()
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
            self.draw_api_auth_input(
                input_x,
                y + (row_h - 34.0 * s) * 0.5,
                input_w,
                34.0 * s,
                s,
                shown_value,
                focused,
                false,
                &ide_panel.api.input_editor,
                blink_alpha,
                crate::ui_system::UiId::ApiAuthValue(scheme_idx),
                ui_registry,
                mx,
                my,
            );
            if entry.is_some_and(|entry| !entry.refresh_token.is_empty()) {
                self.draw_string_scaled_stable(
                    "refresh saved",
                    input_x,
                    y + row_h - 8.0 * s,
                    [0.48, 0.86, 0.52, 1.0],
                    API_FIELD_META_SCALE,
                );
            }
        }

        let btn_y = y + (row_h - 30.0 * s) * 0.5;
        let save = Button {
            x: input_x + input_w + 8.0 * s,
            y: btn_y,
            w: save_w,
            h: 30.0 * s,
            text: "Save".to_string(),
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
            text: "Clear".to_string(),
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
        self.push_rect(x, y + row_h, w, 1.0, [1.0, 1.0, 1.0, 0.08]);
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
        if focused {
            self.draw_api_editor_selection_one_line(
                editor,
                x + 8.0 * s,
                y + 5.0 * s,
                w - 16.0 * s,
                22.0 * s,
                API_FIELD_VALUE_SCALE,
            );
        }
        let shown = if mask && !focused && !value.is_empty() {
            "••••••••".to_string()
        } else {
            value.to_string()
        };
        self.draw_string_scaled_stable(
            &shown,
            x + 8.0 * s,
            y + 23.0 * s,
            self.theme.fg,
            API_FIELD_VALUE_SCALE,
        );
        if focused && blink_alpha > 0.5 {
            let cursor_w = self
                .api_editor_cursor_x_one_line(editor, API_FIELD_VALUE_SCALE)
                .min(w - 16.0 * s);
            self.push_rect(
                x + 8.0 * s + cursor_w,
                y + 7.0 * s,
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
        editor: &crate::editor::Editor,
        blink_alpha: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
    ) -> f32 {
        let row_h = api_body_prop_row_height(schema, s);
        let input_w = (w * 0.60).max(120.0 * s);
        let input_x = x + (w - input_w) * 0.5;
        let input_h = 36.0 * s;
        let input_y = y + (row_h - input_h) * 0.5 - schema.examples.len().min(3) as f32 * 8.0 * s;
        let label_right = input_x - 12.0 * s;
        let name_w = self.measure_ui_width(name, API_FIELD_NAME_SCALE);
        let name_x = (label_right - name_w).max(x + 12.0 * s);
        let name_y = api_split_label_text_y(input_y, input_h, s, false);
        self.draw_string_scaled_stable(name, name_x, name_y, self.theme.fg, API_FIELD_NAME_SCALE);
        if required {
            self.draw_string_scaled_stable(
                "*",
                label_right + 4.0 * s,
                name_y,
                [1.0, 0.42, 0.42, 1.0],
                API_FIELD_NAME_SCALE,
            );
        }
        let type_text = api_body_schema_type_text(schema, model);
        let type_w = self.measure_ui_width(type_text, API_FIELD_TYPE_SCALE);
        self.draw_string_scaled_stable(
            type_text,
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
        let is_file = api_schema_is_file_input(schema, model);
        let pick_w = if is_file { 64.0 * s } else { 0.0 };
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
        let shown = if focused {
            editor.get_full_text()
        } else {
            value.to_string()
        };
        if focused {
            self.draw_api_editor_selection_one_line(
                editor,
                input_x + 8.0 * s,
                input_y + 6.0 * s,
                text_w,
                24.0 * s,
                API_FIELD_VALUE_SCALE,
            );
        }
        self.draw_string_scaled_stable(
            &shown,
            input_x + 8.0 * s,
            input_y + 25.0 * s,
            self.theme.fg,
            API_FIELD_VALUE_SCALE,
        );
        if focused && blink_alpha > 0.5 {
            let cursor_w = self
                .api_editor_cursor_x_one_line(editor, API_FIELD_VALUE_SCALE)
                .min(text_w);
            self.push_rect(
                input_x + 8.0 * s + cursor_w,
                input_y + 8.0 * s,
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
                    "Files".to_string()
                } else {
                    "File".to_string()
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
        let right_x = input_x + input_w + 12.0 * s;
        if let Some(max) = schema.max_chars {
            let text = format!("Max {} chars", max);
            self.draw_string_scaled_stable(
                &text,
                right_x,
                right_y,
                [0.68, 0.70, 0.78, 1.0],
                API_FIELD_META_SCALE,
            );
            right_y += 20.0 * s;
        }
        if let Some(default) = &schema.default_value {
            self.draw_string_scaled_stable(
                "default",
                right_x,
                right_y,
                [0.68, 0.70, 0.78, 1.0],
                API_FIELD_META_SCALE,
            );
            right_y += 17.0 * s;
            self.draw_string_scaled_stable(
                default,
                right_x,
                right_y,
                [0.82, 0.83, 0.88, 1.0],
                API_FIELD_META_SCALE,
            );
            right_y += 20.0 * s;
        }
        for (idx, allowed) in schema.enum_values.iter().take(5).enumerate() {
            let line_h = 18.0 * s;
            if ui_registry.register_rect(
                crate::ui_system::UiId::ApiBodyAllowedValue(route_idx, prop_idx, idx),
                right_x - 2.0 * s,
                right_y - 12.0 * s,
                (x + w - right_x - 8.0 * s).max(24.0 * s),
                line_h,
                mx,
                my,
            ) {
                self.push_rect(
                    right_x - 2.0 * s,
                    right_y - 12.0 * s,
                    (x + w - right_x - 8.0 * s).max(24.0 * s),
                    line_h,
                    [1.0, 1.0, 1.0, 0.08],
                );
            }
            self.draw_string_scaled_stable(
                allowed,
                right_x,
                right_y,
                [0.50, 0.80, 1.0, 1.0],
                API_FIELD_META_SCALE,
            );
            right_y += 20.0 * s;
        }
        let mut example_y = input_y + input_h + 19.0 * s;
        for example in schema.examples.iter().take(3) {
            self.draw_string_scaled_stable(
                example,
                input_x + 8.0 * s,
                example_y,
                [0.62, 0.64, 0.72, 1.0],
                API_FIELD_META_SCALE,
            );
            example_y += 20.0 * s;
        }
        self.push_rect(x, y + row_h, w, 1.0, [1.0, 1.0, 1.0, 0.08]);
        row_h
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
        param: &ApiParam,
        value: &str,
        focused: bool,
        editor: &crate::editor::Editor,
        blink_alpha: f32,
        id: crate::ui_system::UiId,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
    ) -> f32 {
        let row_h = api_param_row_height(param, s);
        let input_w = (w * 0.60).max(120.0 * s);
        let input_x = x + (w - input_w) * 0.5;
        let input_h = 36.0 * s;
        let input_y = y + (row_h - input_h) * 0.5;
        let label_right = input_x - 12.0 * s;
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
                label_right + 4.0 * s,
                name_y,
                [1.0, 0.42, 0.42, 1.0],
                API_FIELD_NAME_SCALE,
            );
        }
        let type_text = api_param_type_text(param);
        let type_w = self.measure_ui_width(type_text, API_FIELD_TYPE_SCALE);
        self.draw_string_scaled_stable(
            type_text,
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
        let shown = if focused {
            editor.get_full_text()
        } else {
            value.to_string()
        };
        if focused {
            self.draw_api_editor_selection_one_line(
                editor,
                input_x + 8.0 * s,
                input_y + 6.0 * s,
                input_w - 16.0 * s,
                24.0 * s,
                API_FIELD_VALUE_SCALE,
            );
        }
        self.draw_string_scaled_stable(
            &shown,
            input_x + 8.0 * s,
            input_y + 25.0 * s,
            self.theme.fg,
            API_FIELD_VALUE_SCALE,
        );
        if focused && blink_alpha > 0.5 {
            let cursor_w = self
                .api_editor_cursor_x_one_line(editor, API_FIELD_VALUE_SCALE)
                .min(input_w - 16.0 * s);
            self.push_rect(
                input_x + 8.0 * s + cursor_w,
                input_y + 8.0 * s,
                1.5 * s,
                22.0 * s,
                self.theme.fg,
            );
        }
        let right_x = input_x + input_w + 10.0 * s;
        let mut right_y = input_y + 15.0 * s;
        if let Some(default) = &param.default_value {
            self.draw_string_scaled_stable(
                "default",
                right_x,
                right_y,
                [0.68, 0.70, 0.78, 1.0],
                API_FIELD_META_SCALE,
            );
            right_y += 17.0 * s;
            self.draw_string_scaled_stable(
                default,
                right_x,
                right_y,
                [0.82, 0.83, 0.88, 1.0],
                API_FIELD_META_SCALE,
            );
            right_y += 20.0 * s;
        }
        if let Some(example) = &param.example {
            self.draw_string_scaled_stable(
                "example",
                right_x,
                right_y,
                [0.68, 0.70, 0.78, 1.0],
                API_FIELD_META_SCALE,
            );
            right_y += 17.0 * s;
            self.draw_string_scaled_stable(
                example,
                right_x,
                right_y,
                [0.82, 0.83, 0.88, 1.0],
                API_FIELD_META_SCALE,
            );
        }
        y + row_h
    }

    pub(crate) fn draw_api_editor_selection_one_line(
        &mut self,
        editor: &crate::editor::Editor,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        text_scale: f32,
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
        let sel_x = self.measure_ui_width(&text[..start], text_scale).min(w);
        let sel_w = (self.measure_ui_width(&text[start..end], text_scale)).min(w - sel_x);
        if sel_w > 0.0 {
            self.push_rect(x + sel_x, y, sel_w, h, [0.55, 0.36, 0.90, 0.36]);
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
                let prefix = self
                    .measure_ui_width(&text[line_start..sel_start], API_BODY_TEXT_SCALE)
                    .min(w);
                let text_w = if sel_start < sel_end {
                    self.measure_ui_width(&text[sel_start..sel_end], API_BODY_TEXT_SCALE)
                } else {
                    0.0
                };
                let sel_w =
                    (text_w + if newline_selected { 10.0 * s } else { 0.0 }).min(w - prefix);
                if sel_w > 0.0 {
                    let sel_y = y - line_offset + visible_idx as f32 * line_h;
                    self.push_rect(x + prefix, sel_y, sel_w, line_h, self.theme.sel);
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
                let cursor_x = self
                    .measure_ui_width(&text[line_start..cursor], API_BODY_TEXT_SCALE)
                    .min(w);
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
                    x,
                    y - line_offset + visible_idx as f32 * line_h,
                    w,
                );
            } else {
                self.draw_json_lexed_line(
                    line,
                    x,
                    y - line_offset + visible_idx as f32 * line_h,
                    w,
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

fn api_param_type_text(param: &ApiParam) -> &'static str {
    api_schema_type_text(match param.primitive_type {
        crate::app::api_client::ApiPrimitiveType::String => ApiSchemaKind::String,
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
) -> &'static str {
    if api_schema_is_multi_file_input(schema, model) {
        "files"
    } else if matches!(schema.kind, ApiSchemaKind::Bytes) {
        "file"
    } else if !schema.enum_values.is_empty() {
        "enum"
    } else {
        api_schema_type_text(schema.kind)
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
