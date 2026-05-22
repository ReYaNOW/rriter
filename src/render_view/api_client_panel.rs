use crate::app::api_client::{
    format_last_loaded, grouped_route_ranges, ApiSpecSource, ApiUrlStatus,
};
use crate::renderer::Renderer;
use crate::widgets::{Button, IconButton, IconType};
use glow::HasContext;

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_api_client_panel(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        ide_panel: &crate::app::IdePanelState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        blink_alpha: f32,
    ) {
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

        let api = &ide_panel.api;
        let pad = 10.0 * s;
        let icon_size = 26.0 * s;
        let toolbar_h = 40.0 * s;
        let mut cy = y + pad - api.panel_scroll.current.round();

        let add = IconButton {
            x: x + pad,
            y: cy,
            size: icon_size,
            icon: Some(IconType::Plus),
            is_active: api.import_menu_open,
            icon_size: Some(20.0 * s),
            active_square_width: None,
            custom_color: None,
        };
        ui_registry.register_icon_button(
            crate::ui_system::UiId::ApiImportAdd,
            &add,
            self,
            mx,
            my,
            s,
            false,
        );

        let remove = IconButton {
            x: x + pad + 34.0 * s,
            y: cy,
            size: icon_size,
            icon: Some(IconType::GitMinus),
            is_active: false,
            icon_size: Some(20.0 * s),
            active_square_width: None,
            custom_color: None,
        };
        if let Some(selected_idx) = api
            .selected_spec
            .and_then(|id| api.specs.iter().position(|entry| entry.id == id))
        {
            ui_registry.register_icon_button(
                crate::ui_system::UiId::ApiSpecRemove(selected_idx),
                &remove,
                self,
                mx,
                my,
                s,
                false,
            );
        } else {
            remove.render(self, mx, my, s, false);
        }

        let edit = IconButton {
            x: x + pad + 68.0 * s,
            y: cy,
            size: icon_size,
            icon: Some(IconType::Reload),
            is_active: false,
            icon_size: Some(18.0 * s),
            active_square_width: None,
            custom_color: None,
        };
        edit.render(self, mx, my, s, false);

        cy += toolbar_h;

        if api.import_menu_open {
            let item_w = (w - pad * 2.0).max(40.0 * s);
            let item_h = 28.0 * s;
            self.push_rounded_rect(
                x + pad,
                cy,
                item_w,
                item_h * 2.0 + 6.0 * s,
                6.0 * s,
                [0.10, 0.11, 0.14, 0.98],
            );
            ui_registry.register_rect(
                crate::ui_system::UiId::ApiImportFile,
                x + pad,
                cy,
                item_w,
                item_h,
                mx,
                my,
            );
            self.draw_string_scaled(
                "Файл openapi.json",
                x + pad + 10.0 * s,
                cy + 19.0 * s,
                self.theme.fg,
                0.82,
            );
            ui_registry.register_rect(
                crate::ui_system::UiId::ApiImportUrl,
                x + pad,
                cy + item_h,
                item_w,
                item_h,
                mx,
                my,
            );
            self.draw_string_scaled(
                "URL openapi.json",
                x + pad + 10.0 * s,
                cy + item_h + 19.0 * s,
                self.theme.fg,
                0.82,
            );
            cy += item_h * 2.0 + 12.0 * s;
        }

        if api.import_url_open {
            let input_h = 32.0 * s;
            let input_x = x + pad;
            let input_w = (w - pad * 3.0 - 34.0 * s).max(40.0 * s);
            self.push_rounded_rect_border(
                input_x,
                cy,
                input_w,
                input_h,
                5.0 * s,
                (1.0 * s).max(1.0),
                if matches!(api.focused, Some(crate::app::api_client::ApiFocus::ImportUrl)) {
                    [0.60, 0.35, 0.85, 1.0]
                } else {
                    [1.0, 1.0, 1.0, 0.12]
                },
                [0.16, 0.17, 0.21, 1.0],
            );
            ui_registry.register_text_input(
                crate::ui_system::UiId::ApiImportUrlInput,
                input_x,
                cy,
                input_w,
                input_h,
                mx,
                my,
            );
            let text = api.input_editor.get_full_text();
            let shown = if text.is_empty() {
                "https://example.com/openapi.json"
            } else {
                text.as_str()
            };
            let color = if text.is_empty() {
                [0.55, 0.57, 0.64, 1.0]
            } else {
                self.theme.fg
            };
            self.draw_string_scaled(shown, input_x + 8.0 * s, cy + 21.0 * s, color, 0.76);
            if matches!(api.focused, Some(crate::app::api_client::ApiFocus::ImportUrl))
                && blink_alpha > 0.15
            {
                let text_w = self.measure_ui_width(&text, 0.76).min(input_w - 16.0 * s);
                self.push_rect(
                    input_x + 8.0 * s + text_w,
                    cy + 7.0 * s,
                    1.5 * s,
                    input_h - 14.0 * s,
                    self.theme.fg,
                );
            }
            let go = IconButton {
                x: input_x + input_w + pad,
                y: cy + 3.0 * s,
                size: 26.0 * s,
                icon: Some(IconType::Check),
                is_active: false,
                icon_size: Some(18.0 * s),
                active_square_width: None,
                custom_color: Some([0.50, 0.90, 0.55, 1.0]),
            };
            ui_registry.register_icon_button(
                crate::ui_system::UiId::ApiImportUrlConfirm,
                &go,
                self,
                mx,
                my,
                s,
                false,
            );
            cy += input_h + 10.0 * s;
        }

        if let Some(err) = &api.import_error {
            self.draw_string_scaled(err, x + pad, cy + 18.0 * s, [1.0, 0.38, 0.38, 1.0], 0.72);
            cy += 24.0 * s;
        }

        if api.specs.is_empty() {
            self.draw_string_scaled(
                "Нет импортированных API",
                x + pad,
                cy + 24.0 * s,
                [0.55, 0.57, 0.64, 1.0],
                0.85,
            );
        }

        let card_h = 112.0 * s;
        for (idx, spec) in api.specs.iter().enumerate() {
            let card_x = x + pad;
            let card_w = (w - pad * 2.0).max(40.0 * s);
            let selected = api.selected_spec == Some(spec.id);
            let bg = if selected {
                [0.20, 0.18, 0.27, 1.0]
            } else {
                [0.16, 0.17, 0.21, 1.0]
            };
            self.push_rounded_rect(card_x, cy, card_w, card_h, 6.0 * s, bg);
            self.push_rounded_rect_border(
                card_x,
                cy,
                card_w,
                card_h,
                6.0 * s,
                (1.0 * s).max(1.0),
                if selected {
                    [0.60, 0.35, 0.85, 0.80]
                } else {
                    [1.0, 1.0, 1.0, 0.10]
                },
                bg,
            );
            ui_registry.register_rect(
                crate::ui_system::UiId::ApiSpecSelect(idx),
                card_x,
                cy,
                card_w,
                card_h,
                mx,
                my,
            );
            self.draw_string_scaled(
                &spec.title,
                card_x + 10.0 * s,
                cy + 22.0 * s,
                self.theme.fg,
                0.86,
            );
            let version = if spec.version.is_empty() {
                spec.openapi_version.as_str()
            } else {
                spec.version.as_str()
            };
            self.draw_string_scaled(
                version,
                card_x + 10.0 * s,
                cy + 42.0 * s,
                [0.68, 0.70, 0.78, 1.0],
                0.72,
            );
            let source = match &spec.source {
                ApiSpecSource::Local(path) => path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("local"),
                ApiSpecSource::Url(url) => url.as_str(),
            };
            self.draw_string_scaled(
                source,
                card_x + 10.0 * s,
                cy + 62.0 * s,
                [0.58, 0.61, 0.70, 1.0],
                0.66,
            );
            let loaded = format_last_loaded(spec.last_loaded);
            self.draw_string_scaled(
                &loaded,
                card_x + 10.0 * s,
                cy + 82.0 * s,
                [0.58, 0.61, 0.70, 1.0],
                0.66,
            );
            if matches!(spec.source, ApiSpecSource::Url(_)) {
                let (icon, color) = match spec.last_url_status {
                    Some(ApiUrlStatus::Ok(_)) => (IconType::Check, [0.48, 0.86, 0.52, 1.0]),
                    _ => (IconType::Error, [1.0, 0.36, 0.36, 1.0]),
                };
                self.draw_atlas_icon(icon, card_x + card_w - 32.0 * s, cy + 8.0 * s, 20.0 * s, color);
                let refresh = IconButton {
                    x: card_x + card_w - 34.0 * s,
                    y: cy + 40.0 * s,
                    size: 26.0 * s,
                    icon: Some(IconType::Reload),
                    is_active: api.loading.contains(&spec.id),
                    icon_size: Some(18.0 * s),
                    active_square_width: None,
                    custom_color: None,
                };
                ui_registry.register_icon_button(
                    crate::ui_system::UiId::ApiSpecRefresh(idx),
                    &refresh,
                    self,
                    mx,
                    my,
                    s,
                    false,
                );
            }
            let open = Button {
                x: card_x + card_w - 96.0 * s,
                y: cy + card_h - 34.0 * s,
                w: 84.0 * s,
                h: 26.0 * s,
                text: "Открыть".to_string(),
                icon: None,
                text_scale: 0.72,
                icon_size: 0.0,
            };
            open.render(self, mx, my, s, false);
            ui_registry.register_rect(
                crate::ui_system::UiId::ApiSpecOpen(idx),
                open.x,
                open.y,
                open.w,
                open.h,
                mx,
                my,
            );
            cy += card_h + 10.0 * s;
        }

        if let Some(model) = api.selected_model() {
            cy += 6.0 * s;
            self.draw_string_scaled(
                "Routes",
                x + pad,
                cy + 18.0 * s,
                [0.74, 0.76, 0.84, 1.0],
                0.76,
            );
            cy += 28.0 * s;
            let row_h = 30.0 * s;
            let tag_h = 28.0 * s;
            let groups = grouped_route_ranges(&model.routes, &api.collapsed_tags, model.id);
            let mut group_idx = 0usize;
            for (tag, start, len, collapsed) in groups {
                ui_registry.register_rect(
                    crate::ui_system::UiId::ApiRouteTag(group_idx),
                    x,
                    cy,
                    w,
                    tag_h,
                    mx,
                    my,
                );
                let arrow = if collapsed { "›" } else { "⌄" };
                self.draw_string_scaled(arrow, x + pad, cy + 19.0 * s, self.theme.line_num, 0.84);
                self.draw_string_scaled(
                    &tag,
                    x + pad + 18.0 * s,
                    cy + 19.0 * s,
                    self.theme.fg,
                    0.78,
                );
                cy += tag_h;
                if !collapsed {
                    for route_idx in start..start + len {
                        let route = &model.routes[route_idx];
                        ui_registry.register_rect(
                            crate::ui_system::UiId::ApiRouteRow(route_idx),
                            x,
                            cy,
                            w,
                            row_h,
                            mx,
                            my,
                        );
                        let hovered = ui_registry.hovered()
                            == Some(crate::ui_system::UiId::ApiRouteRow(route_idx));
                        if hovered {
                            self.push_rect(x, cy, w, row_h, [1.0, 1.0, 1.0, 0.06]);
                        }
                        let method_color = method_color(route.method);
                        self.draw_string_scaled(
                            route.method.as_str(),
                            x + pad + 8.0 * s,
                            cy + 20.0 * s,
                            method_color,
                            0.66,
                        );
                        self.draw_string_scaled(
                            &route.path,
                            x + pad + 58.0 * s,
                            cy + 20.0 * s,
                            self.theme.fg,
                            0.70,
                        );
                        cy += row_h;
                    }
                }
                group_idx += 1;
            }
        }

        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }
    }
}

pub(crate) fn method_color(method: crate::app::api_client::ApiMethod) -> [f32; 4] {
    match method {
        crate::app::api_client::ApiMethod::Get => [0.35, 0.75, 1.0, 1.0],
        crate::app::api_client::ApiMethod::Post => [0.48, 0.86, 0.52, 1.0],
        crate::app::api_client::ApiMethod::Put => [1.0, 0.76, 0.32, 1.0],
        crate::app::api_client::ApiMethod::Patch => [0.78, 0.58, 1.0, 1.0],
        crate::app::api_client::ApiMethod::Delete => [1.0, 0.42, 0.42, 1.0],
        _ => [0.72, 0.76, 0.84, 1.0],
    }
}
