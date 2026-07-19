#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ApiOverlayLayout {
    pub box_x: f32,
    pub box_y: f32,
    pub box_w: f32,
    pub box_h: f32,
    pub pad: f32,
    pub close_x: f32,
    pub close_y: f32,
    pub close_size: f32,
}

pub(crate) fn api_overlay_layout(
    width: f32,
    height: f32,
    scale: f32,
    desired_w: f32,
    desired_h: f32,
    desired_pad: f32,
) -> ApiOverlayLayout {
    let width = width.max(0.0);
    let height = height.max(0.0);
    let box_w = (desired_w * scale).min((width - 32.0 * scale).max(0.0));
    let box_h = (desired_h * scale).min((height - 32.0 * scale).max(0.0));
    let pad = (desired_pad * scale).min(box_w * 0.25).min(box_h * 0.25);
    let box_x = ((width - box_w) * 0.5).max(0.0).round();
    let box_y = ((height - box_h) * 0.5).max(0.0).round();
    let close_size = (32.0 * scale).min(box_w).min(box_h).max(0.0);
    let close_x = (box_x + box_w - close_size - 6.0 * scale).max(box_x);
    let close_y = (box_y + 6.0 * scale).min((box_y + box_h - close_size).max(box_y));
    ApiOverlayLayout {
        box_x, box_y, box_w, box_h, pad, close_x, close_y, close_size,
    }
}

impl Renderer {
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
        self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.42]);
        ui_registry.register_blocker(crate::ui_system::UiId::ApiTabBody, 0.0, 0.0, self.width, self.height, mx, my);
        let layout = api_overlay_layout(self.width, self.height, s, 860.0, 700.0, 24.0);
        let box_w = layout.box_w;
        let box_h = layout.box_h;
        let pad = layout.pad;
        let box_x = layout.box_x;
        let box_y = layout.box_y;
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
        let close_size = layout.close_size;
        let close = IconButton {
            x: layout.close_x,
            y: layout.close_y,
            size: close_size,
            icon: Some(IconType::Cancel),
            is_active: false,
            icon_size: Some(26.0 * s),
            active_square_width: None,
            custom_color: None,
        };
        if close_size > 0.0 {
        ui_registry.register_icon_button(
            crate::ui_system::UiId::ApiMockGuideClose,
            &close,
            self,
            mx,
            my,
            s,
            false,
        );
        }
        self.draw_string_scaled_stable(
            "Подробный гайд по мокам",
            box_x + pad,
            (box_y + 40.0 * s).round(),
            self.theme.fg,
            1.12,
        );
        let content_x = box_x + pad;
        let content_y = (box_y + 72.0 * s).round();
        let content_w = (box_w - pad * 2.0).max(0.0);
        let content_h = (box_h - 90.0 * s).max(0.0);
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
        if content_w > 0.0 && content_h > 0.0 {
            unsafe {
                self.gl.enable(glow::SCISSOR_TEST);
                self.gl.scissor(
                    content_x.round() as i32,
                    (self.height - (content_y + content_h)).round() as i32,
                    content_w.round() as i32,
                    content_h.round() as i32,
                );
            }
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
        if let Some(thumb) = crate::scroll::scrollbar_thumb(
            content_y + 7.0 * s,
            (content_h - 14.0 * s).max(0.0),
            content_h,
            content_h + max_scroll,
            scroll_y,
            28.0 * s,
        ) {
            self.push_rounded_rect(
                box_x + box_w - 12.0 * s,
                thumb.start,
                4.0 * s,
                thumb.len,
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
        self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.42]);
        ui_registry.register_blocker(crate::ui_system::UiId::ApiTabBody, 0.0, 0.0, self.width, self.height, mx, my);
        let layout = api_overlay_layout(self.width, self.height, s, 720.0, 560.0, 22.0);
        let box_w = layout.box_w;
        let box_h = layout.box_h;
        let pad = layout.pad;
        let box_x = layout.box_x;
        let box_y = layout.box_y;
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
        let close_size = layout.close_size;
        let close = IconButton {
            x: layout.close_x,
            y: layout.close_y,
            size: close_size,
            icon: Some(IconType::Cancel),
            is_active: false,
            icon_size: Some(26.0 * s),
            active_square_width: None,
            custom_color: None,
        };
        if close_size > 0.0 {
            ui_registry.register_icon_button(
                crate::ui_system::UiId::ApiMockServerDetailsClose,
                &close,
                self,
                mx,
                my,
                s,
                false,
            );
        }
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
        let log_w = (box_w - pad * 2.0).max(0.0);
        let log_h = (box_y + box_h - log_y - 18.0 * s).max(0.0);
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
        if let Some(thumb) = crate::app::api_client::api_mock_server_log_scrollbar_thumb(
            (log_x, log_y, log_w, log_h),
            api.mock_server_logs.len(),
            scroll_y,
            s,
        ) {
            self.push_rounded_rect(
                log_x + log_w - 8.0 * s,
                thumb.start,
                4.0 * s,
                thumb.len,
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
        let (text, cursor, selection_anchor) = if focused {
            (
                api.input_editor.get_full_text(),
                Some(api.input_editor.cursor),
                api.input_editor.selection_anchor,
            )
        } else {
            (value, None, None)
        };
        let text = if text.is_empty() { "не задано".to_string() } else { text };
        self.draw_api_python_dialog_input_text(
            &text,
            cursor,
            selection_anchor,
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
                list_h,
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
        let Some((track_h, thumb_h)) = crate::app::api_client::api_python_scrollbar_metrics(
            h,
            max_scroll,
            s,
        ) else {
            return;
        };
        let track_w = 4.0 * s;
        let track_x = x + w - track_w - 4.0 * s;
        let track_y = y + 6.0 * s;
        let thumb_y = track_y + (track_h - thumb_h) * (scroll_y / max_scroll).clamp(0.0, 1.0);
        self.push_rounded_rect(track_x, track_y, track_w, track_h, track_w * 0.5, [1.0, 1.0, 1.0, 0.08]);
        self.push_rounded_rect(track_x, thumb_y, track_w, thumb_h, track_w * 0.5, [1.0, 1.0, 1.0, 0.36]);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_api_python_dialog_input_text(
        &mut self,
        text: &str,
        cursor: Option<usize>,
        selection_anchor: Option<usize>,
        input_x: f32,
        input_y: f32,
        input_w: f32,
        input_h: f32,
        s: f32,
        blink_alpha: f32,
    ) {
        let text_scale = crate::app::file_tree::FILE_TREE_DIALOG_INPUT_TEXT_SCALE;
        let pad_x = 8.0 * s;
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
        self.draw_one_line_selectable_text(
            text,
            cursor.unwrap_or(0),
            selection_anchor,
            false,
            cursor.is_some(),
            input_x,
            input_y,
            input_w,
            input_h,
            scroll_x,
            blink_alpha,
            text_scale,
            text_color,
            0.0,
            pad_x,
        );
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
