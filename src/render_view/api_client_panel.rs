use crate::app::api_client::{
    ApiMethod, ApiSpecSource, api_mock_guide_max_scroll, api_mock_server_log_max_scroll,
    api_timing_visible_at, format_api_secs, format_last_loaded_at, grouped_route_ranges,
    now_epoch_secs, write_api_path_display,
};
use crate::app::api_mock::types::{ApiMockMode, ApiMockServerStatus, ApiPythonRuntimeMode};
use crate::render_view::tree_ui::{TREE_INDENT_W, TREE_ROW_H, TREE_TEXT_SCALE};
use crate::renderer::Renderer;
use crate::widgets::{Button, IconButton, IconType};
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
        let icon_size = 30.0 * s;
        let toolbar_h = 44.0 * s;
        let mut cy = (y + pad - api.panel_scroll.current.round()).round();
        let hover_settled = (api.panel_scroll.current - api.panel_scroll.target).abs() < 0.5;

        let add = IconButton {
            x: x + pad,
            y: cy,
            size: icon_size,
            icon: Some(IconType::Plus),
            is_active: api.import_menu_open,
            icon_size: Some(22.0 * s),
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
            x: x + pad + 38.0 * s,
            y: cy,
            size: icon_size,
            icon: Some(IconType::GitMinus),
            is_active: false,
            icon_size: Some(22.0 * s),
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
            x: x + pad + 76.0 * s,
            y: cy,
            size: icon_size,
            icon: Some(IconType::Reload),
            is_active: false,
            icon_size: Some(21.0 * s),
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
                if matches!(
                    api.focused,
                    Some(crate::app::api_client::ApiFocus::ImportUrl)
                ) {
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
            if matches!(
                api.focused,
                Some(crate::app::api_client::ApiFocus::ImportUrl)
            ) {
                self.draw_api_editor_selection_one_line(
                    &api.input_editor,
                    input_x + 8.0 * s,
                    cy + 6.0 * s,
                    input_w - 16.0 * s,
                    input_h - 12.0 * s,
                    0.76,
                    0.0,
                );
            }
            self.draw_string_scaled_stable(shown, input_x + 8.0 * s, cy + 21.0 * s, color, 0.76);
            if matches!(
                api.focused,
                Some(crate::app::api_client::ApiFocus::ImportUrl)
            ) && blink_alpha > 0.5
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

        cy += 10.0 * s;
        self.draw_string_scaled_stable("Мок-сервер", x + pad, cy + 18.0 * s, self.theme.fg, 0.96);
        let help = Button {
            x: x + w - pad - 30.0 * s,
            y: cy - 2.0 * s,
            w: 30.0 * s,
            h: 24.0 * s,
            text: "?".to_string(),
            icon: None,
            text_scale: 0.84,
            icon_size: 0.0,
        };
        ui_registry.register_button(
            crate::ui_system::UiId::ApiMockGuideOpen,
            &help,
            self,
            mx,
            my,
            s,
            false,
        );
        cy += 28.0 * s;
        let btn_h = 32.0 * s;
        let server_running = matches!(
            api.mock.server_status,
            ApiMockServerStatus::Running { .. } | ApiMockServerStatus::Starting
        );
        let toggle = Button {
            x: x + pad,
            y: cy,
            w: (w - pad * 2.0).max(98.0 * s),
            h: btn_h,
            text: if server_running {
                "Остановить"
            } else {
                "Запустить"
            }
            .to_string(),
            icon: Some(if server_running {
                IconType::Close
            } else {
                IconType::Api
            }),
            text_scale: 0.86,
            icon_size: 18.0 * s,
        };
        ui_registry.register_button(
            crate::ui_system::UiId::ApiMockServerToggle,
            &toggle,
            self,
            mx,
            my,
            s,
            false,
        );
        cy += btn_h + 8.0 * s;
        self.draw_string_scaled_stable(
            "Доступен устройствам в сети",
            x + pad,
            api_panel_row_text_y(cy, 22.0 * s, s),
            [0.58, 0.61, 0.70, 1.0],
            0.70,
        );
        cy += 22.0 * s;
        let status = match &api.mock.server_status {
            ApiMockServerStatus::Stopped => "остановлен".to_string(),
            ApiMockServerStatus::Starting => "запускается".to_string(),
            ApiMockServerStatus::Stopping => "останавливается".to_string(),
            ApiMockServerStatus::Running { url } => url.clone(),
            ApiMockServerStatus::Failed(err) => format!("ошибка: {}", err),
        };
        self.draw_string_scaled_stable(
            &status,
            x + pad,
            api_panel_row_text_y(cy, 26.0 * s, s),
            [0.62, 0.66, 0.74, 1.0],
            0.82,
        );
        cy += 26.0 * s;
        let details = Button {
            x: x + pad,
            y: cy,
            w: (w - pad * 2.0).max(80.0 * s),
            h: btn_h,
            text: "Статус и логи".to_string(),
            icon: Some(IconType::Time),
            text_scale: 0.84,
            icon_size: 18.0 * s,
        };
        ui_registry.register_button(
            crate::ui_system::UiId::ApiMockServerDetails,
            &details,
            self,
            mx,
            my,
            s,
            false,
        );
        cy += btn_h + 10.0 * s;
        let mode_label = match api.mock.mode {
            ApiMockMode::MockAll => "Мокать все",
            ApiMockMode::MockSelectedOnly => "Только выбранные",
            ApiMockMode::MockSelectedProxyRest => "Выбранные + прокси",
        };
        let mode = Button {
            x: x + pad,
            y: cy,
            w: (w - pad * 2.0).max(80.0 * s),
            h: btn_h,
            text: mode_label.to_string(),
            icon: Some(IconType::Reload),
            text_scale: 0.84,
            icon_size: 18.0 * s,
        };
        ui_registry.register_button(
            crate::ui_system::UiId::ApiMockModeSelect,
            &mode,
            self,
            mx,
            my,
            s,
            false,
        );
        cy += btn_h + 8.0 * s;
        self.draw_string_scaled_stable(
            "Базовый URL прокси для немокнутых запросов",
            x + pad,
            api_panel_row_text_y(cy, 20.0 * s, s),
            [0.58, 0.61, 0.70, 1.0],
            0.74,
        );
        cy += 20.0 * s;
        let proxy_x = x + pad;
        let proxy_w = (w - pad * 2.0).max(80.0 * s);
        let proxy_h = 30.0 * s;
        let proxy_focused = matches!(
            api.focused,
            Some(crate::app::api_client::ApiFocus::MockProxyBase)
        );
        self.push_rounded_rect_border(
            proxy_x,
            cy,
            proxy_w,
            proxy_h,
            5.0 * s,
            (1.0 * s).max(1.0),
            if proxy_focused {
                [0.60, 0.35, 0.85, 1.0]
            } else {
                [1.0, 1.0, 1.0, 0.12]
            },
            [0.16, 0.17, 0.21, 1.0],
        );
        ui_registry.register_text_input(
            crate::ui_system::UiId::ApiMockProxyBaseInput,
            proxy_x,
            cy,
            proxy_w,
            proxy_h,
            mx,
            my,
        );
        let proxy_text = if proxy_focused {
            api.input_editor.get_full_text()
        } else {
            api.mock.proxy_base_url.clone()
        };
        let shown = if proxy_text.is_empty() {
            "https://backend.local"
        } else {
            proxy_text.as_str()
        };
        self.draw_string_scaled_stable(
            shown,
            proxy_x + 8.0 * s,
            cy + 20.0 * s,
            if proxy_text.is_empty() {
                [0.55, 0.57, 0.64, 1.0]
            } else {
                self.theme.fg
            },
            0.82,
        );
        cy += proxy_h + 18.0 * s;
        let runtime_status_label = match api.mock.uv.status {
            crate::app::api_mock::types::ApiPythonRuntimeStatus::Unknown => "не проверено",
            crate::app::api_mock::types::ApiPythonRuntimeStatus::Missing => "не найдено",
            crate::app::api_mock::types::ApiPythonRuntimeStatus::Ready => "готово",
            crate::app::api_mock::types::ApiPythonRuntimeStatus::Invalid => "ошибка",
        };
        let runtime_status = match api.mock.uv.mode {
            ApiPythonRuntimeMode::UvManaged => format!(
                "Python через uv {} · {}",
                api.mock.uv.python_version, runtime_status_label
            ),
            ApiPythonRuntimeMode::CustomPython => {
                format!("Свой Python · {}", runtime_status_label)
            }
        };
        self.draw_string_scaled_stable(
            &runtime_status,
            x + pad,
            api_panel_row_text_y(cy, 24.0 * s, s),
            [0.62, 0.66, 0.74, 1.0],
            0.74,
        );
        cy += 24.0 * s;
        if !api.mock.uv.last_error.is_empty() {
            self.draw_string_scaled_stable(
                &api.mock.uv.last_error,
                x + pad,
                cy + 16.0 * s,
                [1.0, 0.70, 0.42, 1.0],
                0.66,
            );
            cy += 22.0 * s;
        }
        let python_manage = Button {
            x: x + pad,
            y: cy,
            w: (w - pad * 2.0).max(80.0 * s),
            h: btn_h,
            text: "Python мок-сервера".to_string(),
            icon: Some(IconType::Api),
            text_scale: 0.82,
            icon_size: 18.0 * s,
        };
        ui_registry.register_button(
            crate::ui_system::UiId::ApiMockPythonManage,
            &python_manage,
            self,
            mx,
            my,
            s,
            false,
        );
        cy += btn_h + 14.0 * s;
        let manual = Button {
            x: x + pad,
            y: cy,
            w: (w - pad * 2.0).max(80.0 * s),
            h: btn_h,
            text: "Добавить ручной route".to_string(),
            icon: Some(IconType::Plus),
            text_scale: 0.82,
            icon_size: 18.0 * s,
        };
        ui_registry.register_button(
            crate::ui_system::UiId::ApiMockAddManualRoute,
            &manual,
            self,
            mx,
            my,
            s,
            false,
        );
        cy += btn_h + 14.0 * s;
        for (manual_idx, route) in api.mock.manual_routes.iter().enumerate().take(8) {
            let method_x = x + pad;
            let method_y = cy + 3.0 * s;
            let method_w = 44.0 * s;
            let method_h = 22.0 * s;
            self.draw_api_method_chip(
                route.method,
                method_x,
                method_y,
                method_w,
                method_h,
                s,
                0.58,
            );
            ui_registry.register_rect(
                crate::ui_system::UiId::ApiMockManualRouteMethod(manual_idx),
                method_x,
                method_y,
                method_w,
                method_h,
                mx,
                my,
            );
            let open_size = 22.0 * s;
            let remove_size = 22.0 * s;
            let path_x = x + pad + 54.0 * s;
            let path_y = cy;
            let path_w =
                (w - pad * 2.0 - 54.0 * s - open_size - remove_size - 14.0 * s).max(48.0 * s);
            let path_h = 28.0 * s;
            let path_focused = matches!(
                api.focused,
                Some(crate::app::api_client::ApiFocus::MockManualPath { manual_idx: f_idx })
                    if f_idx == manual_idx
            );
            self.push_rounded_rect_border(
                path_x,
                path_y,
                path_w,
                path_h,
                5.0 * s,
                (1.0 * s).max(1.0),
                if path_focused {
                    [0.60, 0.35, 0.85, 1.0]
                } else {
                    [1.0, 1.0, 1.0, 0.12]
                },
                [0.16, 0.17, 0.21, 1.0],
            );
            ui_registry.register_text_input(
                crate::ui_system::UiId::ApiMockManualRoutePath(manual_idx),
                path_x,
                path_y,
                path_w,
                path_h,
                mx,
                my,
            );
            let path_text = if path_focused {
                api.input_editor.get_full_text()
            } else {
                route.path.clone()
            };
            if path_focused {
                self.draw_api_editor_selection_one_line(
                    &api.input_editor,
                    path_x + 8.0 * s,
                    path_y + 5.0 * s,
                    path_w - 16.0 * s,
                    path_h - 10.0 * s,
                    0.72,
                    0.0,
                );
                if blink_alpha > 0.5 {
                    let text_w = self
                        .api_editor_cursor_x_one_line(&api.input_editor, 0.72)
                        .min(path_w - 16.0 * s);
                    self.push_rect(
                        path_x + 8.0 * s + text_w,
                        path_y + 6.0 * s,
                        1.5 * s,
                        path_h - 12.0 * s,
                        self.theme.fg,
                    );
                }
            }
            self.draw_string_scaled_stable(
                if path_text.is_empty() {
                    "/mock"
                } else {
                    path_text.as_str()
                },
                path_x + 8.0 * s,
                path_y + 19.0 * s,
                if path_text.is_empty() {
                    [0.55, 0.57, 0.64, 1.0]
                } else {
                    self.theme.fg
                },
                0.72,
            );
            let open = IconButton {
                x: x + w - pad - remove_size - open_size - 6.0 * s,
                y: cy + 3.0 * s,
                size: open_size,
                icon: Some(IconType::Api),
                is_active: false,
                icon_size: Some(17.0 * s),
                active_square_width: None,
                custom_color: Some([0.50, 0.82, 1.0, 1.0]),
            };
            ui_registry.register_icon_button(
                crate::ui_system::UiId::ApiMockManualRouteOpen(manual_idx),
                &open,
                self,
                mx,
                my,
                s,
                false,
            );
            let remove = IconButton {
                x: x + w - pad - remove_size,
                y: cy + 3.0 * s,
                size: remove_size,
                icon: Some(IconType::Close),
                is_active: false,
                icon_size: Some(17.0 * s),
                active_square_width: None,
                custom_color: Some([1.0, 0.48, 0.48, 1.0]),
            };
            ui_registry.register_icon_button(
                crate::ui_system::UiId::ApiMockManualRouteRemove(manual_idx),
                &remove,
                self,
                mx,
                my,
                s,
                false,
            );
            cy += 34.0 * s;
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
            let card_y = cy.round();
            let card_w = (w - pad * 2.0).max(40.0 * s);
            let selected = api.selected_spec == Some(spec.id);
            let bg = if selected {
                [0.20, 0.18, 0.27, 1.0]
            } else {
                [0.16, 0.17, 0.21, 1.0]
            };
            self.push_rounded_rect(card_x, card_y, card_w, card_h, 6.0 * s, bg);
            self.push_rounded_rect_border(
                card_x,
                card_y,
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
                card_y,
                card_w,
                card_h,
                mx,
                my,
            );
            self.draw_string_scaled_stable(
                &spec.title,
                card_x + 10.0 * s,
                (card_y + 22.0 * s).round(),
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
                (card_y + 42.0 * s).round(),
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
                (card_y + 62.0 * s).round(),
                [0.58, 0.61, 0.70, 1.0],
                0.74,
            );
            let loaded = format_last_loaded_at(spec.last_loaded, now);
            self.draw_string_scaled_stable(
                &loaded,
                card_x + 10.0 * s,
                api_panel_row_text_y(card_y + 66.0 * s, 18.0 * s, s),
                [0.58, 0.61, 0.70, 1.0],
                0.74,
            );
            if api_timing_visible_at(spec.last_loaded, now) {
                let fetch = format_api_secs(spec.last_fetch_secs);
                let parse = format_api_secs(spec.last_parse_secs);
                self.draw_string_scaled_stable(
                    "Запрос ",
                    card_x + 10.0 * s,
                    (card_y + 96.0 * s).round(),
                    self.theme.fg,
                    0.78,
                );
                self.draw_string_scaled_stable(
                    &fetch,
                    card_x + 68.0 * s,
                    (card_y + 96.0 * s).round(),
                    self.theme.fg,
                    0.78,
                );
                self.draw_string_scaled_stable(
                    "Парсинг ",
                    card_x + 132.0 * s,
                    (card_y + 96.0 * s).round(),
                    self.theme.fg,
                    0.78,
                );
                self.draw_string_scaled_stable(
                    &parse,
                    card_x + 202.0 * s,
                    (card_y + 96.0 * s).round(),
                    self.theme.fg,
                    0.78,
                );
            }
            if matches!(spec.source, ApiSpecSource::Url(_)) {
                let refresh = IconButton {
                    x: card_x + card_w - 34.0 * s,
                    y: card_y + 8.0 * s,
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
            let auth_hovered = hover_settled
                && ui_registry.register_rect(
                    crate::ui_system::UiId::ApiAuthRoot,
                    x,
                    cy,
                    w,
                    TREE_ROW_H * s,
                    mx,
                    my,
                );
            if auth_hovered {
                self.push_rect(x, cy, w, TREE_ROW_H * s, [1.0, 1.0, 1.0, 0.055]);
            }
            self.draw_string_scaled_stable(
                "Auth",
                x + pad + 18.0 * s,
                tree_text_y(cy),
                self.theme.fg,
                TREE_TEXT_SCALE,
            );
            cy += TREE_ROW_H * s;
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
                self.draw_tree_disclosure_icon(!collapsed, tag_x, cy, tag_h, self.theme.line_num);
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
                        let hovered = hover_settled
                            && ui_registry.hovered()
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
                        let route_mock_enabled = api.mock.route_overrides.iter().any(|item| {
                            item.enabled && item.method == route.method && item.path == route.path
                        });
                        if route_mock_enabled {
                            let icon_size = 17.0 * s;
                            self.draw_file_icon(
                                "mock",
                                false,
                                (x + w - pad - icon_size).round(),
                                (cy + (row_h - icon_size) * 0.5).round(),
                                icon_size,
                            );
                        }
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

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_api_mock_guide_overlay(
        &mut self,
        _x: f32,
        _y: f32,
        _w: f32,
        _h: f32,
        s: f32,
        api: &crate::app::api_client::ApiClientState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
    ) {
        ui_registry.mark_overlay_start();
        ui_registry.reset_cursor_state();
        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }
        self.push_rect(0.0, 0.0, self.width, self.height, [0.02, 0.02, 0.03, 0.82]);
        ui_registry.register_blocker(crate::ui_system::UiId::ApiTabBody, 0.0, 0.0, self.width, self.height, mx, my);
        let pad = 24.0 * s;
        let box_w = (860.0 * s).min(self.width - 32.0 * s).max(320.0 * s);
        let box_h = (700.0 * s).min(self.height - 32.0 * s).max(360.0 * s);
        let box_x = ((self.width - box_w) / 2.0).round();
        let box_y = ((self.height - box_h) / 2.0).round();
        self.push_rounded_rect_border(
            box_x,
            box_y,
            box_w,
            box_h,
            6.0 * s,
            (1.0 * s).max(1.0),
            [0.60, 0.35, 0.85, 0.90],
            [0.12, 0.13, 0.17, 1.0],
        );
        let close = Button {
            x: box_x + box_w - 34.0 * s,
            y: box_y + 8.0 * s,
            w: 26.0 * s,
            h: 24.0 * s,
            text: "x".to_string(),
            icon: None,
            text_scale: 0.76,
            icon_size: 0.0,
        };
        ui_registry.register_button(
            crate::ui_system::UiId::ApiMockGuideClose,
            &close,
            self,
            mx,
            my,
            s,
            false,
        );
        self.draw_string_scaled_stable(
            "Подробный гайд по мокам",
            box_x + pad,
            (box_y + 40.0 * s).round(),
            self.theme.fg,
            1.12,
        );
        let content_x = box_x + pad;
        let content_y = (box_y + 72.0 * s).round();
        let content_w = box_w - pad * 2.0;
        let content_h = (box_h - 90.0 * s).max(80.0 * s);
        ui_registry.register_blocker(
            crate::ui_system::UiId::ApiMockGuideBody,
            content_x,
            content_y,
            content_w,
            content_h,
            mx,
            my,
        );
        let max_scroll = api_mock_guide_max_scroll(content_h, s);
        let scroll_y = api.mock_guide_scroll.current.min(max_scroll).max(0.0);
        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            self.gl.scissor(
                content_x.round() as i32,
                (self.height - (content_y + content_h)).round() as i32,
                content_w.round() as i32,
                content_h.round() as i32,
            );
        }
        let mut cy = (content_y + 20.0 * s - scroll_y).round();
        self.draw_wrapped_api_panel_text(
            "1. Статичный ответ: открой route, нажми `Настроить мок`, включи `Мок вкл`, оставь `Python выкл`, затем впиши готовый ответ в `Ответ мока`.",
            content_x,
            cy,
            content_w,
            s,
            0.90,
        );
        cy += 44.0 * s;
        cy = self.draw_api_mock_guide_code_block(
            &[
                "{",
                "  \"items\": [",
                "    {\"id\": 1, \"name\": \"Alice\", \"role\": \"admin\"},",
                "    {\"id\": 2, \"name\": \"Bob\", \"role\": \"user\"}",
                "  ],",
                "  \"page\": 1,",
                "  \"total\": 2,",
                "  \"meta\": {\"source\": \"rriter\", \"cached\": false}",
                "}",
            ],
            content_x,
            cy,
            content_w,
            s,
        );
        cy += 16.0 * s;
        self.draw_wrapped_api_panel_text(
            "2. Прокси-режим: `Выбранные + прокси` отдаёт включённые моки локально, а остальные запросы отправляет в backend из `Базовый URL прокси`, например `https://api.dev.local`.",
            content_x,
            cy,
            content_w,
            s,
            0.90,
        );
        cy += 58.0 * s;
        self.draw_wrapped_api_panel_text(
            "3. Python-обработчик: включи `Python вкл`. Код получает `req`, `query`, `body`, `fields` и path params. Ty проверяет типы, подсветка и completion работают прямо в редакторе.",
            content_x,
            cy,
            content_w,
            s,
            0.90,
        );
        cy += 58.0 * s;
        cy = self.draw_api_mock_guide_code_block(
            &[
                "def handler(req: Request, query: Query, body: Body, fields: Fields, user_id: str):",
                "    if query.page < 1:",
                "        return error_response(\"page must be positive\", 400)",
                "    profile = {",
                "        \"id\": user_id,",
                "        \"name\": body.name,",
                "        \"active\": True,",
                "        \"roles\": [\"reader\", \"writer\"],",
                "    }",
                "    return json_response({",
                "        \"profile\": profile,",
                "        \"headers\": req.headers,",
                "        \"page\": query.page,",
                "    })",
            ],
            content_x,
            cy,
            content_w,
            s,
        );
        cy += 16.0 * s;
        self.draw_wrapped_api_panel_text(
            "4. Возвраты: `json_response(data)` для JSON, `text_response(text)` для текста, `error_response(message, status)` для ошибок. `Статус и логи` показывает startup axum и access log входящих запросов.",
            content_x,
            cy,
            content_w,
            s,
            0.90,
        );
        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }
        if max_scroll > 0.0 {
            let ratio = (scroll_y / max_scroll).clamp(0.0, 1.0);
            let track_h = content_h - 14.0 * s;
            let thumb_h = (content_h / (content_h + max_scroll) * track_h).max(28.0 * s);
            let thumb_y = content_y + 7.0 * s + ratio * (track_h - thumb_h);
            self.push_rounded_rect(
                box_x + box_w - 12.0 * s,
                thumb_y,
                4.0 * s,
                thumb_h,
                2.0 * s,
                [1.0, 1.0, 1.0, 0.28],
            );
            ui_registry.register_rect(
                crate::ui_system::UiId::ApiMockGuideScrollY,
                box_x + box_w - 18.0 * s,
                content_y,
                16.0 * s,
                content_h,
                mx,
                my,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_api_mock_server_detail_overlay(
        &mut self,
        _x: f32,
        _y: f32,
        _w: f32,
        _h: f32,
        s: f32,
        api: &crate::app::api_client::ApiClientState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
    ) {
        ui_registry.mark_overlay_start();
        ui_registry.reset_cursor_state();
        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }
        self.push_rect(0.0, 0.0, self.width, self.height, [0.02, 0.02, 0.03, 0.82]);
        ui_registry.register_blocker(crate::ui_system::UiId::ApiTabBody, 0.0, 0.0, self.width, self.height, mx, my);
        let pad = 22.0 * s;
        let box_w = (720.0 * s).min(self.width - 32.0 * s).max(320.0 * s);
        let box_h = (560.0 * s).min(self.height - 32.0 * s).max(340.0 * s);
        let box_x = ((self.width - box_w) / 2.0).round();
        let box_y = ((self.height - box_h) / 2.0).round();
        self.push_rounded_rect_border(
            box_x,
            box_y,
            box_w,
            box_h,
            6.0 * s,
            (1.0 * s).max(1.0),
            [0.60, 0.35, 0.85, 0.90],
            [0.12, 0.13, 0.17, 1.0],
        );
        let close = Button {
            x: box_x + box_w - 34.0 * s,
            y: box_y + 8.0 * s,
            w: 26.0 * s,
            h: 24.0 * s,
            text: "x".to_string(),
            icon: None,
            text_scale: 0.76,
            icon_size: 0.0,
        };
        ui_registry.register_button(
            crate::ui_system::UiId::ApiMockServerDetailsClose,
            &close,
            self,
            mx,
            my,
            s,
            false,
        );
        let mut cy = (box_y + 38.0 * s).round();
        self.draw_string_scaled_stable("Статус и логи мок-сервера", box_x + pad, cy, self.theme.fg, 1.02);
        cy += 34.0 * s;
        let status = match &api.mock.server_status {
            ApiMockServerStatus::Stopped => "остановлен".to_string(),
            ApiMockServerStatus::Starting => "запускается: 55%".to_string(),
            ApiMockServerStatus::Stopping => "останавливается".to_string(),
            ApiMockServerStatus::Running { url } => format!("готов: {url}"),
            ApiMockServerStatus::Failed(err) => format!("ошибка: {err}"),
        };
        self.draw_wrapped_api_panel_text(&status, box_x + pad, cy, box_w - pad * 2.0, s, 0.88);
        cy += 46.0 * s;
        self.draw_string_scaled_stable("axum / access log", box_x + pad, cy, self.theme.fg, 0.92);
        cy += 28.0 * s;
        let log_x = box_x + pad;
        let log_y = cy.round();
        let log_w = box_w - pad * 2.0;
        let log_h = (box_y + box_h - log_y - 18.0 * s).max(64.0 * s);
        self.push_rounded_rect_border(
            log_x,
            log_y,
            log_w,
            log_h,
            5.0 * s,
            (1.0 * s).max(1.0),
            [1.0, 1.0, 1.0, 0.12],
            [0.08, 0.08, 0.10, 1.0],
        );
        ui_registry.register_blocker(
            crate::ui_system::UiId::ApiMockServerLogArea,
            log_x,
            log_y,
            log_w,
            log_h,
            mx,
            my,
        );
        let max_scroll = api_mock_server_log_max_scroll(api.mock_server_logs.len(), log_h, s);
        let scroll_y = api.mock_server_log_scroll.current.min(max_scroll).max(0.0);
        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            self.gl.scissor(
                log_x.round() as i32,
                (self.height - (log_y + log_h)).round() as i32,
                log_w.round() as i32,
                log_h.round() as i32,
            );
        }
        if api.mock_server_logs.is_empty() {
            self.draw_string_scaled_stable(
                "No log events yet",
                log_x + 12.0 * s,
                log_y + 24.0 * s,
                [0.58, 0.61, 0.70, 1.0],
                0.84,
            );
        } else {
            let line_h = 20.0 * s;
            let first = (scroll_y / line_h).floor().max(0.0) as usize;
            let mut line_y = log_y + 22.0 * s - scroll_y + first as f32 * line_h;
            for line in api.mock_server_logs.iter().skip(first) {
                if line_y > log_y + log_h + line_h {
                    break;
                }
                if line_y + line_h >= log_y {
                    self.draw_string_mono_scaled(
                        &line.text,
                        log_x + 12.0 * s,
                        line_y,
                        [0.76, 0.79, 0.86, 1.0],
                        0.72,
                    );
                }
                line_y += line_h;
            }
        }
        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }
        if max_scroll > 0.0 {
            let ratio = (scroll_y / max_scroll).clamp(0.0, 1.0);
            let track_h = log_h - 14.0 * s;
            let total_h = api.mock_server_logs.len() as f32 * 20.0 * s + 12.0 * s;
            let thumb_h = (log_h / total_h * track_h).max(24.0 * s);
            let thumb_y = log_y + 7.0 * s + ratio * (track_h - thumb_h);
            self.push_rounded_rect(
                log_x + log_w - 8.0 * s,
                thumb_y,
                4.0 * s,
                thumb_h,
                2.0 * s,
                [1.0, 1.0, 1.0, 0.24],
            );
            ui_registry.register_rect(
                crate::ui_system::UiId::ApiMockServerLogScrollY,
                log_x + log_w - 14.0 * s,
                log_y,
                14.0 * s,
                log_h,
                mx,
                my,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_api_mock_python_overlay(
        &mut self,
        s: f32,
        api: &crate::app::api_client::ApiClientState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        blink_alpha: f32,
    ) {
        ui_registry.mark_overlay_start();
        ui_registry.reset_cursor_state();
        self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.42]);
        let layout = crate::app::api_client::api_python_runtime_dialog_layout(
            self.width,
            self.height,
            s,
        );
        let pad = layout.pad;
        let box_x = layout.box_x;
        let box_y = layout.box_y;
        let box_w = layout.box_w;
        let box_h = layout.box_h;
        self.push_rounded_rect_border(
            box_x,
            box_y,
            box_w,
            box_h,
            10.0 * s,
            2.0 * s,
            self.theme.sel,
            [0.15, 0.16, 0.20, 1.0],
        );
        self.draw_string_scaled(
            "Python мок-сервера",
            box_x + pad,
            box_y + 38.0 * s,
            self.theme.fg,
            1.0,
        );
        let mode_label = match api.mock.uv.mode {
            ApiPythonRuntimeMode::UvManaged => "Режим: uv управляет Python",
            ApiPythonRuntimeMode::CustomPython => "Режим: свой Python",
        };
        self.draw_api_python_dialog_button(
            ui_registry,
            crate::ui_system::UiId::ApiMockPythonModeToggle,
            mode_label,
            box_x + pad,
            box_y + 58.0 * s,
            box_w - pad * 2.0,
            32.0 * s,
            0.86,
            mx,
            my,
            s,
        );
        let content_w = layout.content_w;
        match api.mock.uv.mode {
            ApiPythonRuntimeMode::UvManaged => {
                let pick_w = 82.0 * s;
                let uv_path = api
                    .mock
                    .uv
                    .configured_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_else(|| {
                        api.mock
                            .uv
                            .detected_path
                            .as_ref()
                            .map(|path| path.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "авто поиск".to_string())
                    });
                self.draw_api_runtime_input(
                    "Путь к uv",
                    api,
                    crate::app::api_client::ApiFocus::MockPythonUvPath,
                    crate::ui_system::UiId::ApiMockPythonUvPathInput,
                    uv_path,
                    box_x + pad,
                    box_y + 104.0 * s,
                    content_w - pick_w - 10.0 * s,
                    s,
                    ui_registry,
                    mx,
                    my,
                    blink_alpha,
                );
                self.draw_api_python_dialog_button(
                    ui_registry,
                    crate::ui_system::UiId::ApiMockPythonPickUvPath,
                    "Выбрать",
                    box_x + pad + content_w - pick_w,
                    box_y + 122.0 * s,
                    pick_w,
                    34.0 * s,
                    0.70,
                    mx,
                    my,
                    s,
                );
                self.draw_api_python_version_selector(
                    api,
                    ui_registry,
                    box_x + pad,
                    box_y + 154.0 * s,
                    content_w,
                    s,
                    mx,
                    my,
                    false,
                );
            }
            ApiPythonRuntimeMode::CustomPython => {
                let pick_w = 82.0 * s;
                let python_path = api
                    .mock
                    .uv
                    .custom_python_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.draw_api_runtime_input(
                    "Путь к Python",
                    api,
                    crate::app::api_client::ApiFocus::MockPythonCustomPath,
                    crate::ui_system::UiId::ApiMockPythonCustomPathInput,
                    python_path,
                    box_x + pad,
                    box_y + 104.0 * s,
                    content_w - pick_w - 10.0 * s,
                    s,
                    ui_registry,
                    mx,
                    my,
                    blink_alpha,
                );
                self.draw_api_python_dialog_button(
                    ui_registry,
                    crate::ui_system::UiId::ApiMockPythonPickCustomPath,
                    "Выбрать",
                    box_x + pad + content_w - pick_w,
                    box_y + 122.0 * s,
                    pick_w,
                    34.0 * s,
                    0.70,
                    mx,
                    my,
                    s,
                );
            }
        }
        let status_label = match api.mock.uv.status {
            crate::app::api_mock::types::ApiPythonRuntimeStatus::Unknown => "не проверено",
            crate::app::api_mock::types::ApiPythonRuntimeStatus::Missing => "не найдено",
            crate::app::api_mock::types::ApiPythonRuntimeStatus::Ready => "готово",
            crate::app::api_mock::types::ApiPythonRuntimeStatus::Invalid => "ошибка",
        };
        let status = if api.mock.uv.last_error.is_empty() {
            format!("Состояние: {status_label}")
        } else {
            format!("Состояние: {status_label}. {}", api.mock.uv.last_error)
        };
        self.draw_wrapped_api_panel_text(&status, box_x + pad, box_y + 250.0 * s, content_w, s, 0.76);
        if crate::app::api_client::api_python_install_log_visible(api) {
            let log_rect = crate::app::api_client::api_python_install_log_rect(layout, s);
            self.draw_api_python_install_log(api, log_rect.0, log_rect.1, log_rect.2, log_rect.3, s);
        }
        let btn_gap = 10.0 * s;
        let btn_y = box_y + box_h - 64.0 * s;
        if matches!(api.mock.uv.mode, ApiPythonRuntimeMode::UvManaged) {
            let btn_w = ((box_w - pad * 2.0 - btn_gap * 2.0) / 3.0).floor();
            self.draw_api_python_dialog_button(
                ui_registry,
                crate::ui_system::UiId::ApiMockPythonCheckRuntime,
                "Проверить",
                box_x + pad,
                btn_y,
                btn_w,
                32.0 * s,
                0.76,
                mx,
                my,
                s,
            );
            self.draw_api_python_dialog_button(
                ui_registry,
                crate::ui_system::UiId::ApiMockPythonPrepareVersion,
                if api.mock_python_install_running {
                    "Скачивание..."
                } else {
                    "Скачать"
                },
                box_x + pad + btn_w + btn_gap,
                btn_y,
                btn_w,
                32.0 * s,
                0.70,
                mx,
                my,
                s,
            );
            self.draw_api_python_dialog_button(
                ui_registry,
                crate::ui_system::UiId::ApiMockPythonManageClose,
                "Закрыть",
                box_x + pad + (btn_w + btn_gap) * 2.0,
                btn_y,
                btn_w,
                32.0 * s,
                0.76,
                mx,
                my,
                s,
            );
        } else {
            let btn_w = ((box_w - pad * 2.0 - btn_gap) / 2.0).floor();
            self.draw_api_python_dialog_button(
                ui_registry,
                crate::ui_system::UiId::ApiMockPythonCheckRuntime,
                "Проверить",
                box_x + pad,
                btn_y,
                btn_w,
                32.0 * s,
                0.76,
                mx,
                my,
                s,
            );
            self.draw_api_python_dialog_button(
                ui_registry,
                crate::ui_system::UiId::ApiMockPythonManageClose,
                "Закрыть",
                box_x + pad + btn_w + btn_gap,
                btn_y,
                btn_w,
                32.0 * s,
                0.76,
                mx,
                my,
                s,
            );
        }
        if matches!(api.mock.uv.mode, ApiPythonRuntimeMode::UvManaged)
            && api.mock_python_version_picker_open
        {
            self.draw_api_python_version_selector(
                api,
                ui_registry,
                box_x + pad,
                box_y + 154.0 * s,
                content_w,
                s,
                mx,
                my,
                true,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_api_runtime_input(
        &mut self,
        label: &str,
        api: &crate::app::api_client::ApiClientState,
        focus: crate::app::api_client::ApiFocus,
        id: crate::ui_system::UiId,
        value: String,
        x: f32,
        y: f32,
        w: f32,
        s: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        blink_alpha: f32,
    ) {
        self.draw_string_scaled(label, x, y + 14.0 * s, [0.55, 0.57, 0.64, 1.0], 0.74);
        let focused = api.focused.as_ref() == Some(&focus);
        let input_y = y + 18.0 * s;
        let input_h = 34.0 * s;
        self.push_rounded_rect(
            x,
            input_y,
            w,
            input_h,
            5.0 * s,
            [0.08, 0.09, 0.12, 1.0],
        );
        ui_registry.register_text_input(id, x, input_y, w, input_h, mx, my);
        let (text, cursor) = if focused {
            (
                api.input_editor.get_full_text(),
                Some(api.input_editor.cursor),
            )
        } else {
            (value, None)
        };
        let text = if text.is_empty() { "не задано".to_string() } else { text };
        self.draw_api_python_dialog_input_text(
            &text,
            cursor,
            x,
            input_y,
            w,
            input_h,
            s,
            blink_alpha,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_api_python_version_selector(
        &mut self,
        api: &crate::app::api_client::ApiClientState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        x: f32,
        y: f32,
        w: f32,
        s: f32,
        mx: f32,
        my: f32,
        draw_popup: bool,
    ) {
        self.draw_string_scaled("Версия Python", x, y + 14.0 * s, [0.55, 0.57, 0.64, 1.0], 0.74);
        let input_y = y + 18.0 * s;
        let input_h = 34.0 * s;
        let hovered = ui_registry.register_rect(
            crate::ui_system::UiId::ApiMockPythonVersionInput,
            x,
            input_y,
            w,
            input_h,
            mx,
            my,
        );
        self.push_rounded_rect(
            x,
            input_y,
            w,
            input_h,
            5.0 * s,
            if hovered {
                [0.12, 0.13, 0.17, 1.0]
            } else {
                [0.08, 0.09, 0.12, 1.0]
            },
        );
        let value = if api.mock.uv.python_version.trim().is_empty() {
            "выбрать версию".to_string()
        } else {
            api.mock.uv.python_version.clone()
        };
        self.draw_string_scaled(&value, x + 8.0 * s, input_y + 23.0 * s, self.theme.fg, 0.92);
        self.draw_string_scaled(
            "v",
            x + w - 18.0 * s,
            input_y + 22.0 * s,
            [0.55, 0.57, 0.64, 1.0],
            0.82,
        );
        if draw_popup && api.mock_python_version_picker_open {
            let row_h = crate::app::api_client::api_python_version_row_height(s);
            let (list_x, list_y, list_w, list_h) = crate::app::api_client::api_python_version_list_rect(
                crate::app::api_client::api_python_runtime_dialog_layout(self.width, self.height, s),
                s,
            );
            let max_scroll = crate::app::api_client::api_python_version_list_max_scroll(
                api.mock_python_versions.len(),
                s,
            );
            let scroll_y = api.mock_python_versions_scroll.current.min(max_scroll).max(0.0);
            let render_scroll_y = scroll_y.round();
            let list_scrolling = api.mock_python_versions_scroll.is_dragging
                || (api.mock_python_versions_scroll.target - api.mock_python_versions_scroll.current).abs()
                    > 0.01
                || api.mock_python_versions_scroll.velocity.abs() > 0.01;
            let hover_mx = if list_scrolling { -1.0 } else { mx };
            let hover_my = if list_scrolling { -1.0 } else { my };
            self.push_rounded_rect_border(
                list_x,
                list_y,
                list_w,
                list_h,
                6.0 * s,
                1.0_f32.max(s),
                self.theme.sel,
                [0.09, 0.10, 0.14, 1.0],
            );
            self.flush();
            unsafe {
                self.gl.enable(glow::SCISSOR_TEST);
                self.gl.scissor(
                    list_x.round() as i32,
                    (self.height - (list_y + list_h)).round() as i32,
                    list_w.round() as i32,
                    list_h.round() as i32,
                );
            }
            if api.mock_python_versions_loading {
                self.draw_string_scaled(
                    "запрашиваю версии через uv...",
                    list_x + 10.0 * s,
                    (list_y + 25.0 * s).round(),
                    self.theme.fg,
                    0.84,
                );
            } else if api.mock_python_versions.is_empty() {
                self.draw_string_scaled(
                    "версии не найдены",
                    list_x + 10.0 * s,
                    (list_y + 25.0 * s).round(),
                    [0.82, 0.62, 0.42, 1.0],
                    0.84,
                );
            } else {
                for (idx, row) in api.mock_python_versions.iter().enumerate() {
                    let row_y = (list_y + 4.0 * s + idx as f32 * row_h - render_scroll_y).round();
                    if row_y + row_h < list_y || row_y > list_y + list_h {
                        continue;
                    }
                    let hovered = ui_registry.register_rect(
                        crate::ui_system::UiId::ApiMockPythonVersionOption(idx),
                        list_x,
                        row_y,
                        list_w,
                        row_h,
                        hover_mx,
                        hover_my,
                    );
                    if hovered {
                        self.push_rect(list_x + 2.0 * s, row_y, list_w - 4.0 * s, row_h, [1.0, 1.0, 1.0, 0.10]);
                    }
                    let mark = if row.installed { "установлена" } else { "доступна" };
                    let line = format!("{}  ·  {}", row.version, mark);
                    self.draw_string_scaled(
                        &line,
                        list_x + 10.0 * s,
                        (row_y + 20.0 * s).round(),
                        if row.installed {
                            [0.62, 0.86, 0.62, 1.0]
                        } else {
                            self.theme.fg
                        },
                        0.84,
                    );
                }
            }
            self.flush();
            unsafe {
                self.gl.disable(glow::SCISSOR_TEST);
            }
            self.draw_api_python_vertical_scrollbar(
                list_x,
                list_y,
                list_w,
                list_h,
                scroll_y,
                max_scroll,
                s,
            );
        }
    }

    fn draw_api_python_install_log(
        &mut self,
        api: &crate::app::api_client::ApiClientState,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        s: f32,
    ) {
        self.push_rounded_rect(x, y, w, h, 5.0 * s, [0.08, 0.09, 0.12, 1.0]);
        let max_scroll = crate::app::api_client::api_python_install_log_max_scroll(
            api.mock_python_install_log.len(),
            h,
            s,
        );
        let scroll_y = api
            .mock_python_install_log_scroll
            .current
            .min(max_scroll)
            .max(0.0);
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
        let line_h = crate::app::api_client::api_python_install_log_line_height(s);
        let render_scroll_y = scroll_y.round();
        for (idx, line) in api.mock_python_install_log.iter().enumerate() {
            let line_y = (y + 18.0 * s + idx as f32 * line_h - render_scroll_y).round();
            if line_y < y || line_y > y + h + line_h {
                continue;
            }
            let color = match line.kind {
                crate::app::api_client::ApiPythonInstallLogKind::Info => [0.70, 0.73, 0.80, 1.0],
                crate::app::api_client::ApiPythonInstallLogKind::Ok => [0.62, 0.86, 0.62, 1.0],
                crate::app::api_client::ApiPythonInstallLogKind::Error => [1.0, 0.45, 0.42, 1.0],
            };
            self.draw_string_scaled(&line.text, x + 8.0 * s, line_y, color, 0.74);
        }
        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }
        self.draw_api_python_vertical_scrollbar(x, y, w, h, scroll_y, max_scroll, s);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_api_python_vertical_scrollbar(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        scroll_y: f32,
        max_scroll: f32,
        s: f32,
    ) {
        if max_scroll <= 0.0 {
            return;
        }
        let track_w = 4.0 * s;
        let track_x = x + w - track_w - 4.0 * s;
        let track_y = y + 6.0 * s;
        let track_h = h - 12.0 * s;
        let thumb_h = (track_h * (track_h / (track_h + max_scroll))).clamp(18.0 * s, track_h);
        let thumb_y = track_y + (track_h - thumb_h) * (scroll_y / max_scroll).clamp(0.0, 1.0);
        self.push_rounded_rect(track_x, track_y, track_w, track_h, track_w * 0.5, [1.0, 1.0, 1.0, 0.08]);
        self.push_rounded_rect(track_x, thumb_y, track_w, thumb_h, track_w * 0.5, [1.0, 1.0, 1.0, 0.36]);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_api_python_dialog_input_text(
        &mut self,
        text: &str,
        cursor: Option<usize>,
        input_x: f32,
        input_y: f32,
        input_w: f32,
        input_h: f32,
        s: f32,
        blink_alpha: f32,
    ) {
        let text_scale = crate::app::file_tree::FILE_TREE_DIALOG_INPUT_TEXT_SCALE;
        let pad_x = 8.0 * s;
        let input_x = input_x.round();
        let input_y = input_y.round();
        let input_w = input_w.round();
        let input_h = input_h.round();
        let text_y = (input_y + 23.0 * s).round();
        let visible_width = (input_w - pad_x * 2.0).max(0.0);
        let scroll_x = cursor
            .map(|cursor| {
                crate::app::file_tree::file_tree_name_input_scroll_x(
                    text,
                    cursor,
                    visible_width,
                    |c| {
                        let char_to_render = if c == '\n' { '↵' } else { c };
                        self.get_ui_glyph(char_to_render)
                            .map(|g| g.advance * text_scale)
                            .unwrap_or(10.0 * text_scale)
                    },
                )
            })
            .unwrap_or(0.0);
        let text_color = if text == "не задано" {
            [0.55, 0.57, 0.64, 1.0]
        } else {
            self.theme.fg
        };
        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            let scissor_y = self.height - (input_y + input_h);
            self.gl.scissor(input_x as i32, scissor_y as i32, input_w as i32, input_h as i32);
        }
        self.draw_string_scaled(
            text,
            input_x + pad_x - scroll_x,
            text_y,
            text_color,
            text_scale,
        );
        if let Some(cursor) = cursor.filter(|_| blink_alpha > 0.5) {
            let prefix = text.get(..cursor).unwrap_or(text);
            let cursor_x = input_x + pad_x + self.measure_ui_width(prefix, text_scale) - scroll_x;
            self.push_rect(
                cursor_x.min(input_x + input_w - pad_x),
                input_y + 7.0 * s,
                2.0 * s,
                input_h - 14.0 * s,
                self.theme.fg,
            );
        }
        self.flush();
        unsafe {
            self.gl.disable(glow::SCISSOR_TEST);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_api_python_dialog_button(
        &mut self,
        ui_registry: &mut crate::ui_system::UiRegistry,
        id: crate::ui_system::UiId,
        label: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        text_scale: f32,
        mx: f32,
        my: f32,
        s: f32,
    ) {
        let x = x.round();
        let y = y.round();
        let w = w.round();
        let h = h.round();
        let hovered = ui_registry.register_rect(id, x, y, w, h, mx, my);
        let bg = if hovered {
            [0.30, 0.32, 0.38, 1.0]
        } else {
            [0.22, 0.23, 0.28, 1.0]
        };
        self.push_rounded_rect(x, y, w, h, 5.0 * s, bg);
        let tw = self.measure_ui_width(label, text_scale);
        self.draw_string_scaled(
            label,
            x + (w - tw) / 2.0,
            (y + 21.0 * s).round(),
            self.theme.fg,
            text_scale,
        );
    }

    fn draw_wrapped_api_panel_text(&mut self, text: &str, x: f32, y: f32, w: f32, s: f32, scale: f32) {
        let mut line = String::new();
        let mut cy = y;
        for word in text.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{} {}", line, word)
            };
            if self.measure_ui_width(&candidate, scale) > w && !line.is_empty() {
                self.draw_string_scaled_stable(&line, x, cy, self.theme.fg, scale);
                cy += 16.0 * s;
                line.clear();
                line.push_str(word);
            } else {
                line = candidate;
            }
        }
        if !line.is_empty() {
            self.draw_string_scaled_stable(&line, x, cy, self.theme.fg, scale);
        }
    }

    fn draw_api_mock_guide_code_block(
        &mut self,
        lines: &[&str],
        x: f32,
        y: f32,
        w: f32,
        s: f32,
    ) -> f32 {
        let line_h = 18.0 * s;
        let pad = 10.0 * s;
        let h = lines.len() as f32 * line_h + pad * 2.0;
        self.push_rounded_rect_border(
            x,
            y,
            w,
            h,
            5.0 * s,
            (1.0 * s).max(1.0),
            [1.0, 1.0, 1.0, 0.10],
            [0.08, 0.08, 0.11, 1.0],
        );
        let mut cy = y + pad + 13.0 * s;
        for line in lines {
            self.draw_string_mono_scaled(line, x + pad, cy, [0.82, 0.84, 0.90, 1.0], 0.72);
            cy += line_h;
        }
        y + h
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

fn api_panel_row_text_y(row_y: f32, row_h: f32, scale: f32) -> f32 {
    row_y.round() + row_h.round() * 0.5 + (4.5 * scale).round()
}
