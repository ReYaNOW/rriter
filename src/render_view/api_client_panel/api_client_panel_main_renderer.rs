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

}
