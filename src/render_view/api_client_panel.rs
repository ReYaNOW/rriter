use crate::app::api_client::{
    api_timing_visible_at, format_api_secs, format_last_loaded_at, grouped_route_ranges,
    now_epoch_secs, write_api_path_display, ApiMethod, ApiSpecSource,
};
use crate::renderer::Renderer;
use crate::render_view::tree_ui::{TREE_INDENT_W, TREE_ROW_H, TREE_TEXT_SCALE};
use crate::widgets::{IconButton, IconType};
use glow::HasContext;

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    pub(crate) fn draw_api_method_chip(
        &mut self,
        method: ApiMethod,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
        text_scale: f32,
    ) {
        let color = method_color(method);
        let r = (h * 0.5).min(8.0 * s);
        self.push_rounded_rect(x, y, w, h, r, color);
        self.push_rect(x + w - r, y, r, h, color);
        let label = method.chip_str();
        let text_w = self.measure_ui_width(label, text_scale);
        self.draw_string_scaled_stable(
            label,
            x + (w - text_w) * 0.5,
            y + h * 0.5 + 4.5 * s,
            [0.04, 0.05, 0.07, 1.0],
            text_scale,
        );
    }

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
        active_api_route: Option<(crate::app::api_client::ApiSpecId, usize)>,
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
        let hover_settled = (api.panel_scroll.current - api.panel_scroll.target).abs() < 0.5;

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
            self.draw_string_scaled_stable(
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
            self.draw_string_scaled_stable(
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
            if matches!(api.focused, Some(crate::app::api_client::ApiFocus::ImportUrl)) {
                self.draw_api_editor_selection_one_line(
                    &api.input_editor,
                    input_x + 8.0 * s,
                    cy + 6.0 * s,
                    input_w - 16.0 * s,
                    input_h - 12.0 * s,
                    0.76,
                );
            }
            self.draw_string_scaled_stable(shown, input_x + 8.0 * s, cy + 21.0 * s, color, 0.76);
            if matches!(api.focused, Some(crate::app::api_client::ApiFocus::ImportUrl))
                && blink_alpha > 0.5
            {
                let text_w = self
                    .api_editor_cursor_x_one_line(&api.input_editor, 0.76)
                    .min(input_w - 16.0 * s);
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

        let now = now_epoch_secs();
        if let Some(err) = &api.import_error
            && api
                .import_error_at
                .map(|at| now.saturating_sub(at) < 5)
                .unwrap_or(true)
        {
            self.draw_string_scaled_stable(
                err,
                x + pad,
                cy + 18.0 * s,
                [1.0, 0.38, 0.38, 1.0],
                0.72,
            );
        }

        if api.specs.is_empty() {
            self.draw_string_scaled_stable(
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
            self.draw_string_scaled_stable(
                &spec.title,
                card_x + 10.0 * s,
                cy + 22.0 * s,
                self.theme.fg,
                0.90,
            );
            let version = if spec.version.is_empty() {
                spec.openapi_version.as_str()
            } else {
                spec.version.as_str()
            };
            self.draw_string_scaled_stable(
                version,
                card_x + 10.0 * s,
                cy + 42.0 * s,
                [0.68, 0.70, 0.78, 1.0],
                0.80,
            );
            let source = match &spec.source {
                ApiSpecSource::Local(path) => path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("local"),
                ApiSpecSource::Url(url) => url.as_str(),
            };
            self.draw_string_scaled_stable(
                source,
                card_x + 10.0 * s,
                cy + 62.0 * s,
                [0.58, 0.61, 0.70, 1.0],
                0.74,
            );
            let loaded = format_last_loaded_at(spec.last_loaded, now);
            self.draw_string_scaled_stable(
                &loaded,
                card_x + 10.0 * s,
                cy + 78.0 * s,
                [0.58, 0.61, 0.70, 1.0],
                0.74,
            );
            if api_timing_visible_at(spec.last_loaded, now) {
                let fetch = format_api_secs(spec.last_fetch_secs);
                let parse = format_api_secs(spec.last_parse_secs);
                self.draw_string_scaled_stable(
                    "Запрос ",
                    card_x + 10.0 * s,
                    cy + 96.0 * s,
                    self.theme.fg,
                    0.78,
                );
                self.draw_string_scaled_stable(
                    &fetch,
                    card_x + 68.0 * s,
                    cy + 96.0 * s,
                    self.theme.fg,
                    0.78,
                );
                self.draw_string_scaled_stable(
                    "Парсинг ",
                    card_x + 132.0 * s,
                    cy + 96.0 * s,
                    self.theme.fg,
                    0.78,
                );
                self.draw_string_scaled_stable(
                    &parse,
                    card_x + 202.0 * s,
                    cy + 96.0 * s,
                    self.theme.fg,
                    0.78,
                );
            }
            if matches!(spec.source, ApiSpecSource::Url(_)) {
                let refresh = IconButton {
                    x: card_x + card_w - 34.0 * s,
                    y: cy + 8.0 * s,
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
            cy += card_h + 10.0 * s;
        }

        if let Some(model) = api.selected_model() {
            cy += 6.0 * s;
            let tree_text_y = |row_y: f32| Renderer::tree_row_text_y(row_y, TREE_ROW_H * s, s);
            let active_route_idx = active_api_route
                .filter(|(spec_id, _)| *spec_id == model.id)
                .map(|(_, route_idx)| route_idx);
            let root_collapsed = api.collapsed_route_roots.contains(&model.id);
            ui_registry.register_rect(
                crate::ui_system::UiId::ApiRoutesRoot,
                x,
                cy,
                w,
                TREE_ROW_H * s,
                mx,
                my,
            );
            self.draw_tree_disclosure_icon(
                !root_collapsed,
                x + pad,
                cy,
                TREE_ROW_H * s,
                self.theme.line_num,
            );
            self.draw_string_scaled_stable(
                "Routes",
                x + pad + 18.0 * s,
                tree_text_y(cy),
                self.theme.fg,
                TREE_TEXT_SCALE,
            );
            cy += TREE_ROW_H * s;
            let row_h = TREE_ROW_H * s;
            let tag_h = TREE_ROW_H * s;
            let indent_w = TREE_INDENT_W * s;
            if root_collapsed {
                self.flush();
                unsafe {
                    self.gl.disable(glow::SCISSOR_TEST);
                }
                return;
            }
            let groups = grouped_route_ranges(&model.routes, &api.collapsed_tags, model.id);
            let mut group_idx = 0usize;
            let mut path_scratch = String::new();
            for (tag, start, len, collapsed) in groups {
                let tag_hovered = hover_settled
                    && ui_registry.register_rect(
                    crate::ui_system::UiId::ApiRouteTag(group_idx),
                    x,
                    cy,
                    w,
                    tag_h,
                    mx,
                    my,
                );
                if tag_hovered {
                    self.push_rect(x, cy, w, tag_h, [1.0, 1.0, 1.0, 0.055]);
                }
                let tag_x = x + pad + indent_w;
                self.draw_tree_disclosure_icon(
                    !collapsed,
                    tag_x,
                    cy,
                    tag_h,
                    self.theme.line_num,
                );
                self.draw_string_scaled_stable(
                    &tag,
                    tag_x + 18.0 * s,
                    tree_text_y(cy),
                    self.theme.fg,
                    TREE_TEXT_SCALE,
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
                        let hovered = hover_settled && ui_registry.hovered()
                            == Some(crate::ui_system::UiId::ApiRouteRow(route_idx));
                        let active = active_route_idx == Some(route_idx);
                        if active {
                            self.push_rect(x, cy, w, row_h, [0.60, 0.35, 0.85, 0.14]);
                            self.push_rect(x, cy, 3.0 * s, row_h, method_color(route.method));
                        }
                        if hovered {
                            self.push_rect(x, cy, w, row_h, [1.0, 1.0, 1.0, 0.06]);
                        }
                        let route_x = x + pad + indent_w * 2.0;
                        let chip_w = 34.0 * s;
                        self.draw_api_method_chip(
                            route.method,
                            route_x,
                            cy + 5.0 * s,
                            chip_w,
                            18.0 * s,
                            s,
                            0.62,
                        );
                        write_api_path_display(&route.path, &mut path_scratch);
                        self.draw_string_scaled_stable(
                            &path_scratch,
                            route_x + chip_w + 8.0 * s,
                            tree_text_y(cy),
                            self.theme.fg,
                            TREE_TEXT_SCALE,
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
