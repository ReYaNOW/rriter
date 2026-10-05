use crate::renderer::Renderer;
use crate::theme::UiRole;

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
    } else if kind == crate::platform::ToolKind::RustAnalyzer {
        if stacked_actions { 142.0 } else { 102.0 }
    } else if stacked_actions {
        82.0
    } else {
        47.0
    }
}

/// Largest share of a wide tool row the action buttons may take, so the
/// name/status column on the left always keeps readable room.
const TOOL_ROW_ACTIONS_MAX_SHARE: f32 = 0.6;

/// Action buttons of a tool row as `(first_x, gap, widths)`; `natural` holds
/// `[install, pick, clear]` widths, `0.0` for an absent button, `count` is the
/// number of present buttons. Stacked rows sit under the text and split the
/// full width evenly; wide rows share the line with the text, so buttons keep
/// their natural widths, right-aligned and capped by `TOOL_ROW_ACTIONS_MAX_SHARE`.
fn tool_row_action_layout(
    content_x: f32,
    content_w: f32,
    scale: f32,
    stacked: bool,
    natural: [f32; 3],
    count: usize,
) -> (f32, f32, [f32; 3]) {
    let left = content_x + 8.0 * scale;
    let right = content_x + content_w - 8.0 * scale;
    let gap = (6.0 * scale).min((right - left).max(0.0) * 0.08);
    let gaps = gap * count.saturating_sub(1) as f32;
    if stacked {
        let each = ((right - left - gaps) / count.max(1) as f32).max(0.0);
        return (left, gap, natural.map(|w| if w > 0.0 { each } else { 0.0 }));
    }
    let natural_total = natural.iter().sum::<f32>();
    let max_total = (content_w * TOOL_ROW_ACTIONS_MAX_SHARE - gaps).max(0.0);
    let fit = if natural_total > max_total && natural_total > 0.0 {
        max_total / natural_total
    } else {
        1.0
    };
    let widths = natural.map(|w| (w * fit).round());
    let total = widths.iter().sum::<f32>() + gaps;
    ((right - total).max(left), gap, widths)
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
    let lsp = lsp_status_label(lsp_status);
    format!("{} · {source}: {detail} · LSP: {lsp}", state.status().label())
}

fn lsp_status_label(status: Option<crate::lsp::LspServerStatus>) -> &'static str {
    match status {
        Some(crate::lsp::LspServerStatus::Starting) => "запуск",
        Some(crate::lsp::LspServerStatus::Running) => "работает",
        Some(crate::lsp::LspServerStatus::Crashed) => "ошибка",
        Some(crate::lsp::LspServerStatus::Missing) => "не найден",
        Some(crate::lsp::LspServerStatus::Disabled) => "выключен",
        None => "не зарегистрирован",
    }
}

fn tool_status_text(
    kind: crate::platform::ToolKind,
    resolution: &crate::platform::ToolResolution,
    dart_state: &crate::app::tool_installer::DartToolState,
    dart_lsp_status: Option<crate::lsp::LspServerStatus>,
    rust_row: Option<&crate::lsp::RustRowInfo>,
    compact_path_chars: usize,
) -> String {
    if kind == crate::platform::ToolKind::Dart {
        return dart_status_text(dart_state, dart_lsp_status);
    }
    if kind == crate::platform::ToolKind::RustAnalyzer {
        let source = resolution.source_label(kind).unwrap_or("авто");
        let detail = rust_row.and_then(|info| info.version.as_deref()).unwrap_or("не найден");
        let lsp = lsp_status_label(rust_row.map(|info| info.status));
        let mut status = format!("rust-analyzer · {source}: {detail} · LSP: {lsp}");
        let Some(info) = rust_row else { return status };
        if info.busy { status.push_str(" · занят"); }
        if let Some(message) = info.health_message.as_deref() { status.push_str(&format!(" · {message}")); }
        if info.cargo_missing { status.push_str(" · cargo не найден: установите rustup"); }
        if info.component_missing { status.push_str(" · rustup component add rust-analyzer"); }
        return status;
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
    ui: &crate::theme::UiPalette,
    kind: crate::platform::ToolKind,
    resolution: &crate::platform::ToolResolution,
    dart_state: &crate::app::tool_installer::DartToolState,
) -> [f32; 4] {
    if kind == crate::platform::ToolKind::Dart {
        return match dart_state.status() {
            crate::app::tool_installer::DartToolStatus::Ready => ui.pick(UiRole::Success, [0.46, 0.82, 0.58, 1.0]),
            crate::app::tool_installer::DartToolStatus::Checking
            | crate::app::tool_installer::DartToolStatus::Installing
            | crate::app::tool_installer::DartToolStatus::Updating
            | crate::app::tool_installer::DartToolStatus::Cancelling => {
                ui.pick(UiRole::TextSecondary, [0.72, 0.72, 0.82, 1.0])
            }
            crate::app::tool_installer::DartToolStatus::NotFound
            | crate::app::tool_installer::DartToolStatus::Error => ui.pick(UiRole::Error, [0.90, 0.52, 0.52, 1.0]),
        };
    }
    if resolution.is_ready() {
        ui.pick(UiRole::Success, [0.46, 0.82, 0.58, 1.0])
    } else {
        ui.pick(UiRole::Error, [0.90, 0.52, 0.52, 1.0])
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_settings_general_tab(
        &mut self,
        content_x: f32,
        content_available_w: f32,
        mut content_y: f32,
        inner: crate::ui_system::UiClipRect,
        settings_content_clip: crate::ui_system::UiClipRect,
        general_scroll_y: f32,
        general_max_scroll: &mut f32,
        tool_paths: &crate::platform::ToolPaths,
        tool_installer: &crate::app::tool_installer::ToolInstaller,
        dart_settings: &crate::app::DartSettings,
        rust_settings: &crate::app::RustSettings,
        dart_tool_state: &crate::app::tool_installer::DartToolState,
        dart_lsp_status: Option<crate::lsp::LspServerStatus>,
        rust_row: Option<&crate::lsp::RustRowInfo>,
        ui_registry: &mut crate::ui_system::UiRegistry,
    ) {
        let s = self.scale_factor;
        self.begin_settings_content_clip(ui_registry, settings_content_clip);
        content_y = (content_y - general_scroll_y.round()).round();
        self.draw_string_scaled_stable(
            "Внешние инструменты",
            content_x.round(),
            content_y,
            self.ui.pick(UiRole::TextSecondary, [0.82, 0.82, 0.86, 1.0]),
            1.0,
        );
        let refresh_w = (102.0 * s).min(content_available_w);
        let refresh_x = (content_x + content_available_w - refresh_w).round();
        let refresh_y = (content_y - 18.0 * s).round();
        ui_registry.register_rect(
            crate::ui_system::UiId::SettingsRefreshTools,
            refresh_x,
            refresh_y,
            refresh_w,
            29.0 * s,
            self.last_mouse_x,
            self.last_mouse_y,
        );
        crate::widgets::ButtonView {
            x: refresh_x,
            y: refresh_y,
            w: refresh_w,
            h: 29.0 * s,
            text: "Обновить",
            icon: Some(crate::widgets::IconType::Reload),
            text_scale: 0.72,
            icon_size: 14.0 * s,
        }
        .render(self, self.last_mouse_x, self.last_mouse_y, s, false);
        content_y = (content_y + (24.0 * s).round()).round();
        self.draw_string_scaled_stable(
            "Явный путь имеет приоритет над PATH. Переменные RRITER_*_PATH — выше настроек.",
            content_x.round(),
            content_y,
            self.ui.pick(UiRole::TextMuted, [0.44, 0.46, 0.54, 1.0]),
            0.76,
        );
        content_y = (content_y + (18.0 * s).round()).round();
        self.draw_string_scaled_stable(
            "uv, Ruff и Ty ставятся управляемо; Dart выбирается из custom, Flutter, managed или PATH.",
            content_x.round(),
            content_y,
            self.ui.pick(UiRole::TextMuted, [0.44, 0.46, 0.54, 1.0]),
            0.72,
        );
        content_y = (content_y + (18.0 * s).round()).round();

        for kind in crate::platform::ToolKind::ALL {
            let row_y = content_y.round();
            let row_h = self.draw_settings_tool_row(
                kind,
                content_x,
                row_y,
                content_available_w,
                s,
                tool_paths,
                tool_installer,
                dart_settings,
                rust_settings,
                dart_tool_state,
                dart_lsp_status,
                rust_row,
                ui_registry,
            );
            content_y = (row_y + row_h).round();
        }

        if let Some(target) = tool_installer.target() {
            content_y = (content_y + (3.0 * s).round()).round();
            let panel_h = (102.0 * s).round();
            self.push_rounded_rect(
                content_x,
                content_y,
                content_available_w.round(),
                panel_h,
                5.0 * s,
                self.ui.pick(UiRole::BgPanelAlt, [0.10, 0.11, 0.15, 1.0]),
            );
            let heading = format!("{} · {}", target.label(), tool_installer.phase().label());
            self.draw_string_scaled_stable(
                &heading,
                (content_x + 10.0 * s).round(),
                (content_y + (18.0 * s).round()).round(),
                self.ui.pick(UiRole::TextSecondary, [0.84, 0.84, 0.90, 1.0]),
                0.80,
            );
            self.draw_string_scaled_stable(
                &super::settings_ui::compact_settings_text(tool_installer.detail(), 58),
                (content_x + 10.0 * s).round(),
                (content_y + (36.0 * s).round()).round(),
                self.ui.pick(UiRole::TextMuted, [0.56, 0.58, 0.68, 1.0]),
                0.70,
            );
            let logs = tool_installer.logs();
            let start = logs.len().saturating_sub(3);
            let preview_step = (14.0 * s).round().max(1.0);
            let preview_y = (content_y + (55.0 * s).round()).round();
            for (line_idx, line) in logs[start..].iter().enumerate() {
                let color = match line.kind {
                    crate::app::tool_installer::ToolInstallLogKind::Error => {
                        self.ui.pick(UiRole::Error, [0.92, 0.50, 0.50, 1.0])
                    }
                    crate::app::tool_installer::ToolInstallLogKind::Success => {
                        self.ui.pick(UiRole::Success, [0.46, 0.82, 0.58, 1.0])
                    }
                    crate::app::tool_installer::ToolInstallLogKind::Info => {
                        self.ui.pick(UiRole::Info, [0.62, 0.64, 0.72, 1.0])
                    }
                    crate::app::tool_installer::ToolInstallLogKind::Output => {
                        self.ui.pick(UiRole::TextSecondary, [0.74, 0.74, 0.78, 1.0])
                    }
                };
                self.draw_string_scaled_stable(
                    &super::settings_ui::compact_settings_text(&line.text, 58),
                    (content_x + 10.0 * s).round(),
                    (preview_y + line_idx as f32 * preview_step).round(),
                    color,
                    0.65,
                );
            }
            if !logs.is_empty() {
                let button_y = (content_y + 7.0 * s).round();
                let copy_log_w = (104.0 * s).min(content_available_w * 0.48);
                let open_log_w = (100.0 * s).min(content_available_w * 0.48);
                let copy_log_x = (content_x + content_available_w - copy_log_w).round();
                let open_log_x = (copy_log_x - 6.0 * s - open_log_w)
                    .max(content_x)
                    .round();
                ui_registry.register_rect(
                    crate::ui_system::UiId::SettingsOpenToolInstallLog,
                    open_log_x,
                    button_y,
                    open_log_w,
                    29.0 * s,
                    self.last_mouse_x,
                    self.last_mouse_y,
                );
                crate::widgets::ButtonView {
                    x: open_log_x,
                    y: button_y,
                    w: open_log_w,
                    h: 29.0 * s,
                    text: "Открыть лог",
                    icon: None,
                    text_scale: 0.66,
                    icon_size: 0.0,
                }
                .render(self, self.last_mouse_x, self.last_mouse_y, s, false);

                ui_registry.register_rect(
                    crate::ui_system::UiId::SettingsCopyToolInstallLog,
                    copy_log_x,
                    button_y,
                    copy_log_w,
                    29.0 * s,
                    self.last_mouse_x,
                    self.last_mouse_y,
                );
                crate::widgets::ButtonView {
                    x: copy_log_x,
                    y: button_y,
                    w: copy_log_w,
                    h: 29.0 * s,
                    text: "Копировать",
                    icon: None,
                    text_scale: 0.66,
                    icon_size: 0.0,
                }
                .render(self, self.last_mouse_x, self.last_mouse_y, s, false);
            }
            content_y = (content_y + panel_h + (3.0 * s).round()).round();
        }

        content_y = (content_y + (5.0 * s).round()).round();
        self.draw_string_scaled_stable(
            "Каталоги RRiter",
            content_x,
            content_y,
            self.ui.pick(UiRole::TextSecondary, [0.82, 0.82, 0.86, 1.0]),
            0.92,
        );
        content_y = (content_y + (13.0 * s).round()).round();
        let directory_labels = ["Config", "Data", "Cache", "State"];
        let dir_gap = 8.0 * s;
        let dir_button_w = 102.0 * s;
        let dir_columns = (((content_available_w + dir_gap) / (dir_button_w + dir_gap))
            .floor() as usize)
            .clamp(1, directory_labels.len());
        for (idx, label) in directory_labels.iter().enumerate() {
            let col = idx % dir_columns;
            let row = idx / dir_columns;
            let button_x = content_x + col as f32 * (dir_button_w + dir_gap);
            let button_y = (content_y + row as f32 * (37.0 * s).round()).round();
            ui_registry.register_rect(
                crate::ui_system::UiId::SettingsOpenDirectory(idx),
                button_x,
                button_y,
                dir_button_w,
                29.0 * s,
                self.last_mouse_x,
                self.last_mouse_y,
            );
            crate::widgets::ButtonView {
                x: button_x,
                y: button_y,
                w: dir_button_w,
                h: 29.0 * s,
                text: label,
                icon: None,
                text_scale: 0.76,
                icon_size: 0.0,
            }
            .render(self, self.last_mouse_x, self.last_mouse_y, s, false);
        }
        let directory_rows = (directory_labels.len() + dir_columns - 1) / dir_columns;
        content_y = (content_y
            + directory_rows as f32 * (37.0 * s).round()
            + (3.0 * s).round())
        .round();

        self.draw_string_scaled(
            "Графика",
            content_x,
            content_y,
            self.ui.pick(UiRole::TextSecondary, [0.82, 0.82, 0.86, 1.0]),
            0.92,
        );
        content_y = (content_y + (18.0 * s).round()).round();
        let graphics_summary = format!(
            "{} · {} · scale {:.2}",
            self.graphics_diagnostics.renderer,
            self.graphics_diagnostics.version,
            self.graphics_diagnostics.scale_factor
        );
        self.draw_string_scaled(
            &super::settings_ui::compact_settings_text(&graphics_summary, 66),
            content_x,
            content_y,
            self.ui.pick(UiRole::TextMuted, [0.56, 0.58, 0.66, 1.0]),
            0.74,
        );
        let copy_w = (114.0 * s).min(content_available_w);
        let copy_x = content_x + content_available_w - copy_w;
        let copy_y = content_y - 17.0 * s;
        ui_registry.register_rect(
            crate::ui_system::UiId::SettingsCopyGraphicsDiagnostics,
            copy_x,
            copy_y,
            copy_w,
            29.0 * s,
            self.last_mouse_x,
            self.last_mouse_y,
        );
        crate::widgets::ButtonView {
            x: copy_x,
            y: copy_y,
            w: copy_w,
            h: 29.0 * s,
            text: "Скопировать",
            icon: Some(crate::widgets::IconType::Copy),
            text_scale: 0.72,
            icon_size: 14.0 * s,
        }
        .render(self, self.last_mouse_x, self.last_mouse_y, s, false);
        *general_max_scroll = (content_y + general_scroll_y.round() + (12.0 * s).round()
            - (inner.y + inner.h))
            .max(0.0);
        if *general_max_scroll > 0.0 {
            let sb_x = (inner.x + inner.w - 14.0 * s).round();
            let bar = super::settings_ui::settings_scrollbar(
                (sb_x - 5.0 * s, settings_content_clip.y, 16.0 * s, settings_content_clip.h),
                settings_content_clip.h, *general_max_scroll, general_scroll_y,
                6.0, 40.0, self.ui.pick(UiRole::ScrollbarThumb, [0.7, 0.33, 0.54, 1.0]),
            );
            self.draw_scrollbar(&bar, s, 1.0, Some(crate::render_view::scrollbar_widget::ScrollbarHit {
                ui: &mut *ui_registry,
                id: crate::ui_system::UiId::SettingsGeneralScrollY,
                mx: self.last_mouse_x,
                my: self.last_mouse_y,
                blocker: false,
            }));
        }
        self.end_settings_content_clip(ui_registry);
    }

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
            self.ui.pick(UiRole::TextSecondary, [0.82, 0.82, 0.86, 1.0]),
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
            self.ui.pick(UiRole::BgInput, [0.20, 0.21, 0.26, 1.0]),
        );
        let value = ctrl_wheel_multiplier_label(multiplier);
        let text_w = self.measure_ui_width(&value, 0.78);
        self.draw_string_scaled_pixel_snapped(
            &value,
            (value_x + (value_w - text_w) * 0.5).round(),
            Self::tree_row_text_y(row_y, row_h, scale),
            self.ui.pick(UiRole::TextPrimary, [0.92, 0.92, 0.95, 1.0]),
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
        rust_settings: &crate::app::RustSettings,
        dart_tool_state: &crate::app::tool_installer::DartToolState,
        dart_lsp_status: Option<crate::lsp::LspServerStatus>,
        rust_row: Option<&crate::lsp::RustRowInfo>,
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
            rust_row,
            compact_path_chars,
        );
        let status_color = tool_status_color(&self.ui, kind, &resolution, dart_tool_state);

        self.push_rounded_rect(
            content_x,
            row_y,
            content_available_w.round(),
            (row_h - (4.0 * scale).round()).max(1.0),
            5.0 * scale,
            self.ui.pick(UiRole::BgPanelAlt, [0.12, 0.13, 0.17, 1.0]),
        );
        let rust_archive_supported = kind != crate::platform::ToolKind::RustAnalyzer
            || crate::lsp::rust_analyzer_archive_for_platform().is_some();
        let managed = kind.supports_managed_install() && rust_archive_supported;
        let install_text = if tool_installer.is_running_for(kind) {
            "Отмена"
        } else if resolution.is_ready() {
            "Обновить"
        } else {
            "Установить"
        };
        let action_h = 29.0 * scale;
        let action_count =
            usize::from(managed) + 1 + usize::from(configured.is_some());
        let natural = if stacked_actions {
            // Stacked rows ignore natural widths; only presence (> 0) matters.
            [f32::from(u8::from(managed)), 1.0, f32::from(u8::from(configured.is_some()))]
        } else {
            let pad = 12.0 * scale;
            let install_w = if managed {
                self.measure_ui_width(install_text, 0.68) + pad * 2.0
            } else {
                0.0
            };
            let clear_w = if configured.is_some() {
                (self.measure_ui_width("×", 0.92) + pad).max(action_h)
            } else {
                0.0
            };
            [install_w, self.measure_ui_width("Выбрать", 0.72) + pad * 2.0, clear_w]
        };
        let (action_left, action_gap, [install_w, choose_w, clear_w]) = tool_row_action_layout(
            content_x,
            content_available_w,
            scale,
            stacked_actions,
            natural,
            action_count,
        );

        // Text column: full row width when actions sit below the text, else
        // everything left of the right-aligned buttons.
        let text_x = (content_x + 10.0 * scale).round();
        let row_text_w = (content_available_w - 20.0 * scale).max(0.0);
        let text_w = if stacked_actions {
            row_text_w
        } else {
            (action_left - 12.0 * scale - text_x).max(0.0)
        };
        let mut clip_scratch = String::new();
        self.draw_tree_label_clipped(
            kind.label(),
            text_x,
            (row_y + (17.0 * scale).round()).round(),
            text_w,
            self.ui.pick(UiRole::TextSecondary, [0.88, 0.88, 0.92, 1.0]),
            0.88,
            &mut clip_scratch,
        );
        self.draw_tree_label_clipped(
            &status,
            text_x,
            (row_y + (35.0 * scale).round()).round(),
            text_w,
            status_color,
            0.70,
            &mut clip_scratch,
        );
        if kind == crate::platform::ToolKind::Dart {
            let path = dart_tool_state
                .sdk_root()
                .or_else(|| dart_tool_state.path())
                .map(|path| super::settings_ui::compact_settings_path(path, 68))
                .unwrap_or_else(|| "—".to_string());
            // The path line sits below the action buttons in both layouts.
            self.draw_tree_label_clipped(
                &format!("Путь: {path}"),
                text_x,
                (row_y
                    + if stacked_actions {
                        91.0 * scale
                    } else {
                        52.0 * scale
                    })
                .round(),
                row_text_w,
                self.ui.pick(UiRole::TextMuted, [0.50, 0.52, 0.60, 1.0]),
                0.64,
                &mut clip_scratch,
            );
        }
        if kind == crate::platform::ToolKind::RustAnalyzer {
            self.draw_rust_settings_controls(
                content_x,
                row_y,
                content_available_w,
                scale,
                stacked_actions,
                rust_settings,
                ui_registry,
            );
        }

        let action_y = (row_y
            + if stacked_actions {
                44.0 * scale
            } else {
                7.0 * scale
            })
        .round();
        let mut action_x = action_left;

        if managed {
            let install_x = action_x.round();
            action_x += install_w + action_gap;
            let install_disabled = tool_installer.is_running()
                && !tool_installer.is_running_for(kind);
            if !install_disabled {
                ui_registry.register_rect(
                    crate::ui_system::UiId::SettingsToolInstall(kind.index()),
                    install_x,
                    action_y,
                    install_w,
                    action_h,
                    self.last_mouse_x,
                    self.last_mouse_y,
                );
            }
            crate::widgets::ButtonView {
                x: install_x,
                y: action_y,
                w: install_w,
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
        action_x += choose_w + action_gap;
        let path_controls_disabled = tool_installer.is_running();
        if !path_controls_disabled {
            ui_registry.register_rect(
                crate::ui_system::UiId::SettingsToolPick(kind.index()),
                choose_x,
                action_y,
                choose_w,
                action_h,
                self.last_mouse_x,
                self.last_mouse_y,
            );
        }
        crate::widgets::ButtonView {
            x: choose_x,
            y: action_y,
            w: choose_w,
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
                    clear_w,
                    action_h,
                    self.last_mouse_x,
                    self.last_mouse_y,
                );
            }
            crate::widgets::ButtonView {
                x: clear_x,
                y: action_y,
                w: clear_w,
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

    fn draw_rust_settings_controls(
        &mut self,
        content_x: f32,
        row_y: f32,
        content_available_w: f32,
        scale: f32,
        stacked_actions: bool,
        settings: &crate::app::RustSettings,
        ui_registry: &mut crate::ui_system::UiRegistry,
    ) {
        let controls_y = (row_y + if stacked_actions { 108.0 * scale } else { 68.0 * scale }).round();
        let controls_x = (content_x + 8.0 * scale).round();
        let controls_w = (content_available_w - 16.0 * scale).max(0.0);
        let gap = (6.0 * scale).round();
        let button_w = ((controls_w - gap * 2.0) / 3.0).max(0.0);
        let buttons = [
            (
                crate::ui_system::UiId::SettingsRustToggleEnabled,
                if settings.enabled { "Rust: вкл" } else { "Rust: выкл" },
            ),
            (
                crate::ui_system::UiId::SettingsRustToggleCheckCommand,
                match settings.check_command {
                    crate::app::RustCheckCommand::Check => "Проверка: cargo check",
                    crate::app::RustCheckCommand::Clippy => "Проверка: clippy",
                },
            ),
            (crate::ui_system::UiId::SettingsRustRestart, "Restart"),
        ];
        for (index, (id, text)) in buttons.into_iter().enumerate() {
            let x = (controls_x + index as f32 * (button_w + gap)).round();
            ui_registry.register_rect(
                id,
                x,
                controls_y,
                button_w,
                29.0 * scale,
                self.last_mouse_x,
                self.last_mouse_y,
            );
            crate::widgets::ButtonView {
                x,
                y: controls_y,
                w: button_w,
                h: 29.0 * scale,
                text,
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
    use super::{
        ctrl_wheel_multiplier_label, editor_ctrl_wheel_layout, tool_row_action_layout,
        tool_row_units, TOOL_ROW_ACTIONS_MAX_SHARE,
    };

    #[test]
    fn wide_tool_row_actions_right_align_at_natural_width() {
        let (x, gap, widths) =
            tool_row_action_layout(100.0, 600.0, 1.0, false, [0.0, 80.0, 29.0], 2);
        assert_eq!(widths, [0.0, 80.0, 29.0]);
        assert!((x + 80.0 + gap + 29.0 - 692.0).abs() < 0.001);

        let (x, gap, widths) =
            tool_row_action_layout(100.0, 200.0, 1.0, false, [300.0, 300.0, 0.0], 2);
        let total = widths.iter().sum::<f32>() + gap;
        assert!(total <= 200.0 * TOOL_ROW_ACTIONS_MAX_SHARE + 1.0);
        assert!(x + total <= 292.0 + 0.001);
    }

    #[test]
    fn stacked_tool_row_actions_split_full_width() {
        let (x, gap, widths) =
            tool_row_action_layout(0.0, 316.0, 1.0, true, [1.0, 1.0, 0.0], 2);
        assert_eq!(x, 8.0);
        assert_eq!(widths[2], 0.0);
        assert!((widths[0] + gap + widths[1] - 300.0).abs() < 0.001);
    }

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
