use crate::app::App;
use crate::theme::UiRole;
use crate::editor::Editor;
use crate::ui_system::UiId;
use super::UiClickFlow;

impl App {
    pub(super) fn handle_settings_ui_click(&mut self, id: UiId, _same_click_target: bool) -> UiClickFlow {
        if self.keymap_settings.recording.is_some()
            && !matches!(id, UiId::SettingsKeymapAdd(_) | UiId::SettingsKeymapConflictAccept | UiId::SettingsKeymapConflictCancel)
        {
            self.keymap_settings.cancel_recording();
        }
        let scrollbar = if id == UiId::SettingsKeymapScrollY {
            self.ui_registry.rect_for(UiId::SettingsKeymapScrollY).map(|rect| {
                let scale = self.renderer.as_ref().map(|renderer| renderer.scale_factor).unwrap_or(1.0);
                let pointer = self.renderer.as_ref().map(|renderer| renderer.last_mouse_y).unwrap_or(rect.1);
                (rect, scale, pointer)
            })
        } else { None };
        if let Some(keymap_click) = self.keymap_settings.handle_settings_click(
            id, &self.keymap, &self.keymap_overrides, scrollbar,
        ) {
            if let Some(overrides) = keymap_click.overrides { self.set_keymap_overrides(overrides); }
            if keymap_click.reset_all && self.confirm_dialog.request(crate::app::PendingAction::ResetKeymap) {
                self.request_main_redraw();
            }
            if keymap_click.redraw
                && let Some(window) = self.window.as_ref()
            {
                window.request_redraw();
            }
            return UiClickFlow::Handled;
        }
        match id {

            // Settings tabs
            UiId::SettingsTab(idx) => {
                self.keymap_settings.cancel_recording();
                self.settings_tab = idx;
                if let Some(window) = self.window.as_ref() { window.request_redraw(); }
            }
            UiId::SettingsEditorCtrlWheelAdjust(delta) => {
                let next = crate::normalize_ctrl_wheel_multiplier(
                    self.ctrl_wheel_multiplier + delta as f32 * crate::CTRL_WHEEL_MULTIPLIER_STEP,
                );
                if next != self.ctrl_wheel_multiplier {
                    self.ctrl_wheel_multiplier = next;
                    self.save_current_config();
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::SettingsThemePick(target, theme) => match target {
                crate::ui_system::ThemeTarget::Both => self.apply_themes(theme, theme),
                crate::ui_system::ThemeTarget::Editor => {
                    self.apply_themes(theme, self.ui_theme_id)
                }
                crate::ui_system::ThemeTarget::Ui => {
                    self.apply_themes(self.editor_theme_id, theme)
                }
            },
            UiId::SettingsThemeLinked => self.set_theme_linked(!self.theme_linked),
            UiId::SettingsDatabaseAdjust(setting, delta) => {
                self.adjust_database_setting(setting, delta as i32);
                self.window.as_ref().unwrap().request_redraw();
            }

            // Settings IDE
            UiId::SettingsIdeAddWorkspace => {
                self.trigger_folder_picker();
            }
            UiId::SettingsIdeRemoveWorkspace(idx) => {
                if idx < self.ide_workspaces.len() {
                    self.ide_workspaces.remove(idx);
                    self.refresh_dart_tool_state();
                    if let Some(lsp) = &mut self.lsp {
                        lsp.set_workspaces(self.ide_workspaces.clone());
                    }
                    self.clear_all_closing_hints();
                    self.refresh_dart_closing_hints();
                    self.save_current_config();
                    self.refresh_file_tree();
                    self.start_file_watcher();
                    self.window.as_ref().unwrap().request_redraw();
                }
            }
            UiId::SettingsIdeAddIgnore => {
                let pattern = self
                    .settings_ignore_editor
                    .get_full_text()
                    .trim()
                    .to_string();
                if !pattern.is_empty() && !self.ide_ignore_patterns.contains(&pattern) {
                    self.ide_ignore_patterns.push(pattern);
                    // Очищаем редактор
                    let old_version = self.settings_ignore_editor.version;
                    self.settings_ignore_editor = Editor::new(128);
                    self.settings_ignore_editor.version = old_version + 1;
                    self.settings_ignore_editor.cursor = 0;
                    self.settings_ignore_editor.selection_anchor = None;
                    self.save_current_config();
                    self.refresh_file_tree();
                    self.window.as_ref().unwrap().request_redraw();
                }
            }
            UiId::SettingsIdeRemoveIgnore(idx) => {
                if idx < self.ide_ignore_patterns.len() {
                    self.ide_ignore_patterns.remove(idx);
                    self.save_current_config();
                    self.refresh_file_tree();
                    self.window.as_ref().unwrap().request_redraw();
                }
            }
            UiId::SettingsIdeIgnoreInput => {
                self.settings_ignore_focused = true;
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::SettingsIdeScrollY => {
                if let Some(rect) = self.ui_registry.rect_for(UiId::SettingsIdeScrollY) {
                    let s = self
                        .renderer
                        .as_ref()
                        .map(|renderer| renderer.scale_factor)
                        .unwrap_or(1.0);
                    let pointer = self
                        .renderer
                        .as_ref()
                        .map(|renderer| renderer.last_mouse_y)
                        .unwrap_or(rect.1);
                    let window_size = self.window.as_ref().map(|window| window.inner_size());
                    if let Some(window_size) = window_size {
                        let layout =
                            crate::render_view::settings_ui::animated_settings_modal_layout(
                                window_size.width as f32,
                                window_size.height as f32,
                                s,
                                self.settings_anim_progress,
                            );
                        let pad_x = 12.0 * s;
                        let max_scroll = crate::render_view::settings_ui::settings_ide_max_scroll(
                            layout,
                            self.ide_workspaces.len(),
                            self.ide_ignore_patterns.iter().map(|pattern| {
                                self.renderer
                                    .as_mut()
                                    .unwrap()
                                    .measure_ui_width(pattern, 0.88)
                                    + pad_x * 2.0
                                    + 22.0 * s
                            }),
                            s,
                        );
                        let bar = crate::render_view::settings_ui::settings_scrollbar(
                            rect, rect.3, max_scroll, self.settings_ide_scroll.current,
                            6.0, 40.0, self.renderer.as_ref()
                                .map(|renderer| renderer.ui.pick(UiRole::ScrollbarThumb, [0.7, 0.33, 0.54, 1.0]))
                                .unwrap_or_default(),
                        );
                        let geometry = bar.geometry(s);
                        crate::app::mouse::press_scrollbar(
                            &mut self.settings_ide_scroll, geometry, 0.0, pointer,
                        );
                    }
                }
            }
            UiId::SettingsFaqScrollY => {
                if let Some(rect) = self.ui_registry.rect_for(UiId::SettingsFaqScrollY) {
                    let s = self
                        .renderer
                        .as_ref()
                        .map(|renderer| renderer.scale_factor)
                        .unwrap_or(1.0);
                    let pointer = self
                        .renderer
                        .as_ref()
                        .map(|renderer| renderer.last_mouse_y)
                        .unwrap_or(rect.1);
                    let max_scroll = self
                        .renderer
                        .as_mut()
                        .map(|renderer| renderer.get_faq_max_scroll(&self.faq_editor, rect.3))
                        .unwrap_or(0.0);
                    let bar = crate::render_view::settings_ui::settings_scrollbar(
                        rect, rect.3, max_scroll, self.settings_scroll.current,
                        6.0, 40.0, self.renderer.as_ref()
                            .map(|renderer| renderer.ui.pick(UiRole::ScrollbarThumb, [0.7, 0.33, 0.54, 1.0]))
                            .unwrap_or_default(),
                    );
                    let geometry = bar.geometry(s);
                    crate::app::mouse::press_scrollbar(
                        &mut self.settings_scroll, geometry, 0.0, pointer,
                    );
                }
            }
            UiId::SettingsGeneralScrollY => {
                if let Some(rect) = self.ui_registry.rect_for(UiId::SettingsGeneralScrollY) {
                    let s = self.renderer.as_ref().map(|renderer| renderer.scale_factor).unwrap_or(1.0);
                    let pointer = self.renderer.as_ref().map(|renderer| renderer.last_mouse_y).unwrap_or(rect.1);
                    let bar = crate::render_view::settings_ui::settings_scrollbar(
                        rect, rect.3, self.settings_general_max_scroll,
                        self.settings_general_scroll.current, 6.0, 40.0,
                        self.renderer.as_ref()
                            .map(|renderer| renderer.ui.pick(UiRole::ScrollbarThumb, [0.7, 0.33, 0.54, 1.0]))
                            .unwrap_or_default(),
                    );
                    let geometry = bar.geometry(s);
                    crate::app::mouse::press_scrollbar(
                        &mut self.settings_general_scroll, geometry, 0.0, pointer,
                    );
                }
            }
            UiId::SettingsDatabaseScrollY => {
                if let Some(rect) = self.ui_registry.rect_for(UiId::SettingsDatabaseScrollY) {
                    let s = self.renderer.as_ref().map(|renderer| renderer.scale_factor).unwrap_or(1.0);
                    let pointer = self.renderer.as_ref().map(|renderer| renderer.last_mouse_y).unwrap_or(rect.1);
                    let bar = crate::render_view::settings_ui::settings_scrollbar(
                        rect, rect.3, self.settings_database_max_scroll,
                        self.settings_database_scroll.current, 6.0, 40.0,
                        self.renderer.as_ref()
                            .map(|renderer| renderer.ui.pick(UiRole::ScrollbarThumb, [0.7, 0.33, 0.54, 1.0]))
                            .unwrap_or_default(),
                    );
                    let geometry = bar.geometry(s);
                    crate::app::mouse::press_scrollbar(
                        &mut self.settings_database_scroll, geometry, 0.0, pointer,
                    );
                }
            }
            UiId::SettingsToolPick(idx) => {
                if !self.tool_installer.is_running()
                    && let Some(kind) = crate::platform::ToolKind::from_index(idx)
                {
                    self.trigger_settings_tool_picker(kind);
                }
            }
            UiId::SettingsToolClear(idx) => {
                if !self.tool_installer.is_running()
                    && let Some(kind) = crate::platform::ToolKind::from_index(idx)
                {
                    self.apply_tool_path_selection(kind, None);
                }
            }
            UiId::SettingsToolInstall(idx) => {
                if let Some(kind) = crate::platform::ToolKind::from_index(idx) {
                    self.trigger_tool_install(kind);
                }
            }
            UiId::SettingsOpenToolInstallLog => {
                self.tool_installer.open_log();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::SettingsCloseToolInstallLog => {
                self.tool_installer.close_log();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::SettingsCancelToolInstall => {
                self.tool_installer.cancel();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::SettingsCopyToolInstallLog => {
                let log = self.tool_installer.full_log();
                if !log.is_empty() {
                    self.set_clipboard_text(log);
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::SettingsToolInstallLogBackdrop => {
                self.tool_installer.close_log();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::SettingsToolInstallLogBody => {}
            UiId::SettingsToolInstallLogScrollY => {
                if let Some(rect) = self
                    .ui_registry
                    .rect_for(UiId::SettingsToolInstallLogScrollY)
                {
                    let s = self
                        .renderer
                        .as_ref()
                        .map(|renderer| renderer.scale_factor)
                        .unwrap_or(1.0);
                    let pointer = self
                        .renderer
                        .as_ref()
                        .map(|renderer| renderer.last_mouse_y)
                        .unwrap_or(rect.1);
                    let line_h = crate::app::tool_installer::log_line_height(s);
                    let content_h = (self.tool_installer.logs().len().max(1) as f32 * line_h
                        + (12.0 * s).round())
                    .round();
                    self.tool_installer.begin_log_scroll_drag(
                        pointer,
                        rect.1 + 6.0 * s,
                        rect.3 - 12.0 * s,
                        rect.3,
                        content_h,
                        28.0 * s,
                    );
                }
            }
            UiId::SettingsOpenDirectory(idx) => {
                let paths = crate::platform::app_paths();
                let path = match idx {
                    0 => paths.config,
                    1 => paths.data,
                    2 => paths.cache,
                    3 => paths.state,
                    _ => return UiClickFlow::Return,
                };
                if let Err(error) = std::fs::create_dir_all(&path)
                    .and_then(|_| crate::platform::reveal_path(self.external_requests.sink(), &path))
                {
                    eprintln!(
                        "Failed to open RRiter directory {}: {error}",
                        path.display()
                    );
                }
            }
            UiId::SettingsCopyGraphicsDiagnostics => {
                if let Some(report) = self
                    .renderer
                    .as_ref()
                    .map(|renderer| renderer.graphics_diagnostics.report())
                {
                    self.set_clipboard_text(report);
                }
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::SettingsRefreshTools => {
                crate::platform::refresh_tool_resolutions();
                self.refresh_dart_tool_state();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::SettingsDartToggleSupport => {
                self.dart_settings.enabled = !self.dart_settings.enabled;
                if let Some(lsp) = &mut self.lsp {
                    lsp.set_server_enabled("dart", self.dart_settings.enabled);
                    self.ide_panel.lsp_servers = lsp.servers_info();
                }
                if !self.dart_settings.enabled {
                    self.clear_all_closing_hints();
                    self.refresh_dart_closing_hints();
                }
                self.save_current_config();
            }
            UiId::SettingsDartToggleWorkspaceAnalysis => {
                self.dart_settings.workspace_analysis = !self.dart_settings.workspace_analysis;
                if let Some(lsp) = &mut self.lsp {
                    lsp.set_dart_workspace_analysis_enabled(self.dart_settings.workspace_analysis);
                }
                self.save_current_config();
            }
            UiId::SettingsRustToggleEnabled => {
                self.rust_settings.enabled = !self.rust_settings.enabled;
                if let Some(lsp) = &mut self.lsp {
                    lsp.set_rust_enabled(self.rust_settings.enabled);
                }
                self.save_current_config();
            }
            UiId::SettingsRustToggleCheckCommand => {
                self.rust_settings.check_command = match self.rust_settings.check_command {
                    crate::app::RustCheckCommand::Check => crate::app::RustCheckCommand::Clippy,
                    crate::app::RustCheckCommand::Clippy => crate::app::RustCheckCommand::Check,
                };
                if let Some(lsp) = &mut self.lsp {
                    lsp.set_rust_init_options(crate::lsp::rust_initialization_options(
                        self.rust_settings.check_command.config_value(),
                    ));
                }
                self.save_current_config();
            }
            UiId::SettingsRustRestart => {
                if let Some(lsp) = &mut self.lsp {
                    lsp.refresh_rust_resolution();
                }
            }
            UiId::SettingsDartCycleClosingLabels => {
                self.dart_settings.closing_labels = self.dart_settings.closing_labels.next();
                self.sync_dart_closing_hint_settings();
                self.save_current_config();
            }
            UiId::SettingsDartAdjustNesting(delta) => {
                self.dart_settings.adjust_minimum_nesting_depth(delta);
                self.sync_dart_closing_hint_settings();
                self.save_current_config();
            }
            UiId::SettingsDartAdjustBlockLines(delta) => {
                self.dart_settings
                    .adjust_minimum_block_lines(i16::from(delta));
                self.sync_dart_closing_hint_settings();
                self.save_current_config();
            }
            UiId::SettingsDartRestart => {
                self.restart_dart_server();
            }
            UiId::SettingsDartOpenLog => {
                // The LSP panel is drawn only in IDE mode.
                if !self.is_ide_mode {
                    self.enter_ide_mode();
                }
                self.ide_panel.open(crate::app::PanelId::LspServers);
                if let Some(info) = self
                    .ide_panel
                    .lsp_servers
                    .iter()
                    .find(|info| info.name == "dart")
                {
                    self.ide_panel
                        .lsp_logs_expanded
                        .insert(info.name.to_string());
                }
                crate::save_panel_state(&self.ide_panel);
                self.show_settings = false;
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            _ => return UiClickFlow::NotMine,
        }
        UiClickFlow::Handled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_ctrl_wheel_adjust_clicks_use_quarter_steps_and_clamp() {
        let Some(mut app) = crate::app::reviewer_stage2_test_app() else {
            return;
        };
        assert_eq!(app.ctrl_wheel_multiplier, 2.0);
        app.handle_ui_click(UiId::SettingsEditorCtrlWheelAdjust(1));
        assert_eq!(app.ctrl_wheel_multiplier, 2.25);
        app.handle_ui_click(UiId::SettingsEditorCtrlWheelAdjust(-1));
        assert_eq!(app.ctrl_wheel_multiplier, 2.0);

        app.ctrl_wheel_multiplier = crate::CTRL_WHEEL_MULTIPLIER_MIN;
        app.handle_ui_click(UiId::SettingsEditorCtrlWheelAdjust(-1));
        assert_eq!(app.ctrl_wheel_multiplier, crate::CTRL_WHEEL_MULTIPLIER_MIN);
        app.ctrl_wheel_multiplier = crate::CTRL_WHEEL_MULTIPLIER_MAX;
        app.handle_ui_click(UiId::SettingsEditorCtrlWheelAdjust(1));
        assert_eq!(app.ctrl_wheel_multiplier, crate::CTRL_WHEEL_MULTIPLIER_MAX);
    }
}
