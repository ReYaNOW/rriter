use crate::renderer::Renderer;

#[derive(Clone, Copy, Debug, PartialEq)]
struct EditorCtrlWheelLayout {
    label_w: f32,
    decrement_x: f32,
    value_x: f32,
    value_w: f32,
    increment_x: f32,
    button_size: f32,
}

fn editor_ctrl_wheel_layout(content_x: f32, content_w: f32, scale: f32) -> EditorCtrlWheelLayout {
    let content_w = content_w.max(1.0);
    let gap = (8.0 * scale).min(content_w * 0.04).max(0.0);
    let available = (content_w - gap * 2.0).max(0.0);
    let button_size = (30.0 * scale).min(available * 0.22).max(0.0);
    let value_w = (76.0 * scale)
        .min((available - button_size * 2.0).max(0.0));
    let controls_w = button_size * 2.0 + value_w + gap * 2.0;
    let decrement_x = content_x + (content_w - controls_w).max(0.0);
    let value_x = decrement_x + button_size + gap;
    let increment_x = value_x + value_w + gap;
    EditorCtrlWheelLayout {
        label_w: (decrement_x - content_x - 12.0 * scale).max(0.0),
        decrement_x,
        value_x,
        value_w,
        increment_x,
        button_size,
    }
}

fn ctrl_wheel_multiplier_label(value: f32) -> String {
    let quarters = (crate::normalize_ctrl_wheel_multiplier(value) * 4.0).round() as u8;
    format!("{}.{:02}x", quarters / 4, (quarters % 4) * 25)
}

fn tool_row_units(kind: crate::platform::ToolKind, stacked_actions: bool) -> f32 {
    if kind == crate::platform::ToolKind::Dart {
        if stacked_actions { 183.0 } else { 148.0 }
    } else if stacked_actions {
        82.0
    } else {
        47.0
    }
}

fn dart_status_text(
    state: &crate::app::tool_installer::DartToolState,
    lsp_status: Option<crate::lsp::LspServerStatus>,
) -> String {
    let source = state.source().unwrap_or("auto");
    let detail = state
        .version()
        .or_else(|| state.error())
        .unwrap_or("SDK не найден");
    let lsp = match lsp_status {
        Some(crate::lsp::LspServerStatus::Starting) => "запуск",
        Some(crate::lsp::LspServerStatus::Running) => "работает",
        Some(crate::lsp::LspServerStatus::Crashed) => "ошибка",
        Some(crate::lsp::LspServerStatus::Missing) => "не найден",
        Some(crate::lsp::LspServerStatus::Disabled) => "выключен",
        None => "не зарегистрирован",
    };
    format!("{} · {source}: {detail} · LSP: {lsp}", state.status().label())
}

fn tool_status_text(
    kind: crate::platform::ToolKind,
    resolution: &crate::platform::ToolResolution,
    dart_state: &crate::app::tool_installer::DartToolState,
    dart_lsp_status: Option<crate::lsp::LspServerStatus>,
    compact_path_chars: usize,
) -> String {
    if kind == crate::platform::ToolKind::Dart {
        return dart_status_text(dart_state, dart_lsp_status);
    }
    if resolution.is_ready() {
        let path = resolution
            .path
            .as_deref()
            .unwrap_or(std::path::Path::new(""));
        let source = resolution.source_label(kind).unwrap_or("авто");
        return format!(
            "{source}: {}",
            super::settings_ui::compact_settings_path(path, compact_path_chars)
        );
    }
    if resolution.is_invalid_override() {
        let path = resolution
            .configured_path
            .as_deref()
            .unwrap_or(std::path::Path::new(""));
        return format!(
            "Не найден: {}",
            super::settings_ui::compact_settings_path(path, compact_path_chars)
        );
    }
    "Не найден".to_string()
}

fn tool_status_color(
    kind: crate::platform::ToolKind,
    resolution: &crate::platform::ToolResolution,
    dart_state: &crate::app::tool_installer::DartToolState,
) -> [f32; 4] {
    if kind == crate::platform::ToolKind::Dart {
        return match dart_state.status() {
            crate::app::tool_installer::DartToolStatus::Ready => [0.46, 0.82, 0.58, 1.0],
            crate::app::tool_installer::DartToolStatus::Checking
            | crate::app::tool_installer::DartToolStatus::Installing
            | crate::app::tool_installer::DartToolStatus::Updating
            | crate::app::tool_installer::DartToolStatus::Cancelling => {
                [0.72, 0.72, 0.82, 1.0]
            }
            crate::app::tool_installer::DartToolStatus::NotFound
            | crate::app::tool_installer::DartToolStatus::Error => [0.90, 0.52, 0.52, 1.0],
        };
    }
    if resolution.is_ready() {
        [0.46, 0.82, 0.58, 1.0]
    } else {
        [0.90, 0.52, 0.52, 1.0]
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    pub(crate) fn draw_editor_ctrl_wheel_setting(
        &mut self,
        content_x: f32,
        content_w: f32,
        row_y: f32,
        multiplier: f32,
        ui_registry: &mut crate::ui_system::UiRegistry,
    ) {
        let scale = self.scale_factor;
        let row_y = row_y.round();
        let row_h = (30.0 * scale).round().max(1.0);
        let layout = editor_ctrl_wheel_layout(content_x, content_w, scale);
        let mut label_scratch = String::new();
        self.draw_tree_label_clipped(
            "Ускорение Ctrl + колесо",
            content_x.round(),
            Self::tree_row_text_y(row_y, row_h, scale),
            layout.label_w,
            [0.82, 0.82, 0.86, 1.0],
            0.86,
            &mut label_scratch,
        );

        let button_size = layout.button_size.round().max(1.0);
        let button_y = (row_y + (row_h - button_size) * 0.5).round();
        let icon_size = (18.0 * scale).min(button_size * 0.72).max(1.0);
        let decrement = crate::widgets::IconButton {
            x: layout.decrement_x.round(),
            y: button_y,
            size: button_size,
            icon: Some(crate::widgets::IconType::Down),
            is_active: false,
            icon_size: Some(icon_size),
            active_square_width: None,
            custom_color: None,
        };
        let increment = crate::widgets::IconButton {
            x: layout.increment_x.round(),
            y: button_y,
            size: button_size,
            icon: Some(crate::widgets::IconType::Up),
            is_active: false,
            icon_size: Some(icon_size),
            active_square_width: None,
            custom_color: None,
        };
        let value_x = layout.value_x.round();
        let value_w = layout.value_w.round().max(1.0);
        self.push_rounded_rect(
            value_x,
            row_y,
            value_w,
            row_h,
            5.0 * scale,
            [0.20, 0.21, 0.26, 1.0],
        );
        let value = ctrl_wheel_multiplier_label(multiplier);
        let text_w = self.measure_ui_width(&value, 0.78);
        self.draw_string_scaled_pixel_snapped(
            &value,
            (value_x + (value_w - text_w) * 0.5).round(),
            Self::tree_row_text_y(row_y, row_h, scale),
            [0.92, 0.92, 0.95, 1.0],
            0.78,
        );
        let mx = self.last_mouse_x;
        let my = self.last_mouse_y;
        ui_registry.register_icon_button(
            crate::ui_system::UiId::SettingsEditorCtrlWheelAdjust(-1),
            &decrement,
            self,
            mx,
            my,
            scale,
            false,
        );
        ui_registry.register_icon_button(
            crate::ui_system::UiId::SettingsEditorCtrlWheelAdjust(1),
            &increment,
            self,
            mx,
            my,
            scale,
            false,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_settings_tool_row(
        &mut self,
        kind: crate::platform::ToolKind,
        content_x: f32,
        row_y: f32,
        content_available_w: f32,
        scale: f32,
        tool_paths: &crate::platform::ToolPaths,
        tool_installer: &crate::app::tool_installer::ToolInstaller,
        dart_settings: &crate::app::DartSettings,
        dart_tool_state: &crate::app::tool_installer::DartToolState,
        dart_lsp_status: Option<crate::lsp::LspServerStatus>,
        ui_registry: &mut crate::ui_system::UiRegistry,
    ) -> f32 {
        let stacked_actions = content_available_w < 430.0 * scale;
        let row_h = (tool_row_units(kind, stacked_actions) * scale)
            .round()
            .max(1.0);
        let resolution = crate::platform::resolve_tool_kind(kind);
        let configured = tool_paths.get(kind);
        let compact_path_chars = if kind.supports_managed_install() {
            24
        } else {
            47
        };
        let status = tool_status_text(
            kind,
            &resolution,
            dart_tool_state,
            dart_lsp_status,
            compact_path_chars,
        );
        let status_color = tool_status_color(kind, &resolution, dart_tool_state);

        self.push_rounded_rect(
            content_x,
            row_y,
            content_available_w.round(),
            (row_h - (4.0 * scale).round()).max(1.0),
            5.0 * scale,
            [0.12, 0.13, 0.17, 1.0],
        );
        self.draw_string_scaled_stable(
            kind.label(),
            (content_x + 10.0 * scale).round(),
            (row_y + (17.0 * scale).round()).round(),
            [0.88, 0.88, 0.92, 1.0],
            0.88,
        );
        self.draw_string_scaled_stable(
            &status,
            (content_x + 10.0 * scale).round(),
            (row_y + (35.0 * scale).round()).round(),
            status_color,
            0.70,
        );
        if kind == crate::platform::ToolKind::Dart {
            let path = dart_tool_state
                .sdk_root()
                .or_else(|| dart_tool_state.path())
                .map(|path| super::settings_ui::compact_settings_path(path, 68))
                .unwrap_or_else(|| "—".to_string());
            self.draw_string_scaled_stable(
                &format!("Путь: {path}"),
                (content_x + 10.0 * scale).round(),
                (row_y
                    + if stacked_actions {
                        91.0 * scale
                    } else {
                        52.0 * scale
                    })
                .round(),
                [0.50, 0.52, 0.60, 1.0],
                0.64,
            );
        }

        let action_y = (row_y
            + if stacked_actions {
                44.0 * scale
            } else {
                7.0 * scale
            })
        .round();
        let action_left = content_x + 8.0 * scale;
        let action_right = content_x + content_available_w - 8.0 * scale;
        let action_gap = (6.0 * scale).min((action_right - action_left).max(0.0) * 0.08);
        let action_count = usize::from(kind.supports_managed_install())
            + 1
            + usize::from(configured.is_some());
        let action_w = ((action_right
            - action_left
            - action_gap * action_count.saturating_sub(1) as f32)
            / action_count.max(1) as f32)
            .max(0.0);
        let mut action_x = action_left;

        if kind.supports_managed_install() {
            let install_x = action_x.round();
            action_x += action_w + action_gap;
            let install_disabled = tool_installer.is_running()
                && !tool_installer.is_running_for(kind);
            let install_text = if tool_installer.is_running_for(kind) {
                "Отмена"
            } else if resolution.is_ready() {
                "Обновить"
            } else {
                "Установить"
            };
            if !install_disabled {
                ui_registry.register_rect(
                    crate::ui_system::UiId::SettingsToolInstall(kind.index()),
                    install_x,
                    action_y,
                    action_w,
                    29.0 * scale,
                    self.last_mouse_x,
                    self.last_mouse_y,
                );
            }
            crate::widgets::ButtonView {
                x: install_x,
                y: action_y,
                w: action_w,
                h: 29.0 * scale,
                text: install_text,
                icon: None,
                text_scale: 0.68,
                icon_size: 0.0,
            }
            .render(
                self,
                self.last_mouse_x,
                self.last_mouse_y,
                scale,
                install_disabled,
            );
        }

        let choose_x = action_x.round();
        action_x += action_w + action_gap;
        let path_controls_disabled = tool_installer.is_running();
        if !path_controls_disabled {
            ui_registry.register_rect(
                crate::ui_system::UiId::SettingsToolPick(kind.index()),
                choose_x,
                action_y,
                action_w,
                29.0 * scale,
                self.last_mouse_x,
                self.last_mouse_y,
            );
        }
        crate::widgets::ButtonView {
            x: choose_x,
            y: action_y,
            w: action_w,
            h: 29.0 * scale,
            text: "Выбрать",
            icon: None,
            text_scale: 0.72,
            icon_size: 0.0,
        }
        .render(
            self,
            self.last_mouse_x,
            self.last_mouse_y,
            scale,
            path_controls_disabled,
        );

        if configured.is_some() {
            let clear_x = action_x.round();
            if !path_controls_disabled {
                ui_registry.register_rect(
                    crate::ui_system::UiId::SettingsToolClear(kind.index()),
                    clear_x,
                    action_y,
                    action_w,
                    29.0 * scale,
                    self.last_mouse_x,
                    self.last_mouse_y,
                );
            }
            crate::widgets::ButtonView {
                x: clear_x,
                y: action_y,
                w: action_w,
                h: 29.0 * scale,
                text: "×",
                icon: None,
                text_scale: 0.92,
                icon_size: 0.0,
            }
            .render(
                self,
                self.last_mouse_x,
                self.last_mouse_y,
                scale,
                path_controls_disabled,
            );
        }

        if kind == crate::platform::ToolKind::Dart {
            self.draw_dart_settings_controls(
                content_x,
                row_y,
                content_available_w,
                scale,
                stacked_actions,
                dart_settings,
                ui_registry,
            );
        }
        row_h
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_dart_settings_controls(
        &mut self,
        content_x: f32,
        row_y: f32,
        content_available_w: f32,
        scale: f32,
        stacked_actions: bool,
        settings: &crate::app::DartSettings,
        ui_registry: &mut crate::ui_system::UiRegistry,
    ) {
        let controls_y = (row_y
            + if stacked_actions {
                108.0 * scale
            } else {
                68.0 * scale
            })
        .round();
        let controls_x = (content_x + 8.0 * scale).round();
        let controls_w = (content_available_w - 16.0 * scale).max(0.0);
        let gap = (6.0 * scale).round();
        let first_w = ((controls_w - gap * 2.0) / 3.0).max(0.0);
        let first = [
            (
                crate::ui_system::UiId::SettingsDartToggleSupport,
                if settings.enabled { "Dart: вкл" } else { "Dart: выкл" }.to_string(),
            ),
            (
                crate::ui_system::UiId::SettingsDartToggleWorkspaceAnalysis,
                if settings.workspace_analysis {
                    "Анализ: вкл"
                } else {
                    "Анализ: выкл"
                }
                .to_string(),
            ),
            (
                crate::ui_system::UiId::SettingsDartCycleClosingLabels,
                settings.closing_labels.label().to_string(),
            ),
        ];
        for (index, (id, text)) in first.into_iter().enumerate() {
            let x = (controls_x + index as f32 * (first_w + gap)).round();
            ui_registry.register_rect(
                id,
                x,
                controls_y,
                first_w,
                29.0 * scale,
                self.last_mouse_x,
                self.last_mouse_y,
            );
            crate::widgets::ButtonView {
                x,
                y: controls_y,
                w: first_w,
                h: 29.0 * scale,
                text: &text,
                icon: None,
                text_scale: 0.62,
                icon_size: 0.0,
            }
            .render(self, self.last_mouse_x, self.last_mouse_y, scale, false);
        }

        let second_y = (controls_y + 35.0 * scale).round();
        let second_w = ((controls_w - gap * 5.0) / 6.0).max(0.0);
        let second = [
            (
                crate::ui_system::UiId::SettingsDartAdjustNesting(-1),
                "Влож. −".to_string(),
            ),
            (
                crate::ui_system::UiId::SettingsDartAdjustNesting(1),
                format!("Влож. {} +", settings.minimum_nesting_depth),
            ),
            (
                crate::ui_system::UiId::SettingsDartAdjustBlockLines(-1),
                "Строк −".to_string(),
            ),
            (
                crate::ui_system::UiId::SettingsDartAdjustBlockLines(1),
                format!("Строк {} +", settings.minimum_block_lines),
            ),
            (
                crate::ui_system::UiId::SettingsDartRestart,
                "Restart".to_string(),
            ),
            (
                crate::ui_system::UiId::SettingsDartOpenLog,
                "LSP log".to_string(),
            ),
        ];
        for (index, (id, text)) in second.into_iter().enumerate() {
            let x = (controls_x + index as f32 * (second_w + gap)).round();
            ui_registry.register_rect(
                id,
                x,
                second_y,
                second_w,
                29.0 * scale,
                self.last_mouse_x,
                self.last_mouse_y,
            );
            crate::widgets::ButtonView {
                x,
                y: second_y,
                w: second_w,
                h: 29.0 * scale,
                text: &text,
                icon: None,
                text_scale: 0.58,
                icon_size: 0.0,
            }
            .render(self, self.last_mouse_x, self.last_mouse_y, scale, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ctrl_wheel_multiplier_label, editor_ctrl_wheel_layout, tool_row_units};

    #[test]
    fn editor_ctrl_wheel_controls_fit_normal_and_narrow_widths() {
        for (width, scale) in [(620.0, 1.0), (180.0, 1.25), (120.0, 1.75)] {
            let layout = editor_ctrl_wheel_layout(50.0, width, scale);
            assert!(layout.label_w >= 0.0);
            assert!(layout.decrement_x >= 50.0);
            assert!(layout.value_x >= layout.decrement_x + layout.button_size);
            assert!(layout.increment_x >= layout.value_x + layout.value_w);
            assert!(layout.increment_x + layout.button_size <= 50.0 + width + 0.001);
        }
    }

    #[test]
    fn ctrl_wheel_multiplier_labels_are_exact_for_every_quarter_step() {
        let expected = [
            "1.25x", "1.50x", "1.75x", "2.00x", "2.25x", "2.50x", "2.75x", "3.00x",
            "3.25x", "3.50x", "3.75x", "4.00x", "4.25x", "4.50x", "4.75x", "5.00x",
        ];
        for (index, label) in expected.into_iter().enumerate() {
            let value = crate::CTRL_WHEEL_MULTIPLIER_MIN
                + index as f32 * crate::CTRL_WHEEL_MULTIPLIER_STEP;
            assert_eq!(ctrl_wheel_multiplier_label(value), label);
        }
    }

    #[test]
    fn dart_row_reserves_two_control_lines() {
        assert_eq!(tool_row_units(crate::platform::ToolKind::Dart, false), 148.0);
        assert_eq!(tool_row_units(crate::platform::ToolKind::Dart, true), 183.0);
    }

    #[test]
    fn ordinary_tool_rows_keep_existing_height() {
        assert_eq!(tool_row_units(crate::platform::ToolKind::Git, false), 47.0);
        assert_eq!(tool_row_units(crate::platform::ToolKind::Ty, true), 82.0);
    }
}
