/// Обработчики UI событий - централизованная логика для кликов по кнопкам
/// Устраняет дублирование кода между input.rs и ui.rs
use crate::app::App;
use crate::editor::Editor;
use crate::render_view::{editor_bottom_blank_lines, editor_scroll_content_height};
use crate::renderer::VisualLine;
use crate::ui_system::UiId;

fn scrollbar_x_click_target(
    mouse_x: f32,
    track_x: f32,
    track_w: f32,
    current_scroll: f32,
    max_scroll: f32,
    scale: f32,
) -> Option<(f32, f32)> {
    if track_w <= 0.0 || max_scroll <= 0.0 {
        return None;
    }
    let thumb_w = (track_w / (max_scroll + track_w).max(1.0) * track_w)
        .max(40.0 * scale)
        .min(track_w.max(0.0));
    let scroll_ratio = (current_scroll / max_scroll).clamp(0.0, 1.0);
    let thumb_x = track_x + scroll_ratio * (track_w - thumb_w);
    if mouse_x >= thumb_x && mouse_x <= thumb_x + thumb_w {
        Some((mouse_x - thumb_x, current_scroll))
    } else {
        let drag_offset = thumb_w / 2.0;
        let ratio = (mouse_x - track_x - drag_offset) / (track_w - thumb_w).max(0.0001);
        Some((drag_offset, (ratio * max_scroll).clamp(0.0, max_scroll)))
    }
}

pub(crate) fn repeated_ui_click(
    same_target: bool,
    elapsed: std::time::Duration,
    dx: f32,
    dy: f32,
) -> bool {
    same_target && elapsed < std::time::Duration::from_millis(400) && dx * dx + dy * dy < 25.0
}

fn content_y_hits_visual_text_row(
    content_y: f32,
    line_height: f32,
    visual_lines: &[VisualLine],
) -> bool {
    visual_lines
        .iter()
        .any(|line| content_y >= line.y_offset && content_y < line.y_offset + line_height)
}

fn editor_interaction_view_height(
    window_height: f32,
    tab_bar_height: f32,
    panel_bottom_height: f32,
    query_results_height: f32,
    is_ide_mode: bool,
    scale: f32,
) -> f32 {
    let reserved_bottom_height = if is_ide_mode {
        panel_bottom_height + query_results_height
    } else {
        0.0
    };
    crate::render_view::editor_view_height(
        window_height,
        tab_bar_height,
        reserved_bottom_height,
        is_ide_mode,
        scale,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrollbar_x_click_target_drags_thumb_or_jumps_to_pointer() {
        let on_thumb = scrollbar_x_click_target(120.0, 100.0, 400.0, 0.0, 800.0, 1.0)
            .expect("x scrollbar visible");
        assert_eq!(on_thumb, (20.0, 0.0));

        let jump = scrollbar_x_click_target(420.0, 100.0, 400.0, 0.0, 800.0, 1.0)
            .expect("x scrollbar visible");
        assert!(jump.0 > 0.0);
        assert!(jump.1 > 0.0);
        assert!(jump.1 <= 800.0);

        assert!(scrollbar_x_click_target(120.0, 100.0, 400.0, 0.0, 0.0, 1.0).is_none());
    }

    #[test]
    fn editor_pointer_viewport_matches_rendered_bottom_reservations() {
        let viewport = editor_interaction_view_height(1000.0, 40.0, 300.0, 160.0, true, 1.0);
        assert_eq!(
            viewport,
            1000.0 - 40.0 - 300.0 - 160.0 - crate::render_view::ide_status_bar_height(1.0)
        );

        assert_eq!(
            editor_interaction_view_height(1000.0, 40.0, 300.0, 160.0, false, 1.0),
            960.0
        );
    }

    #[test]
    fn repeated_click_requires_the_same_ui_target() {
        let elapsed = std::time::Duration::from_millis(100);
        assert!(repeated_ui_click(true, elapsed, 2.0, 1.0));
        assert!(!repeated_ui_click(false, elapsed, 0.0, 0.0));
        assert!(!repeated_ui_click(
            true,
            std::time::Duration::from_millis(400),
            0.0,
            0.0,
        ));
        assert!(!repeated_ui_click(true, elapsed, 5.0, 0.0));
    }

    #[test]
    fn editor_text_row_hit_test_rejects_blank_viewport_edges() {
        let lines = [
            VisualLine {
                byte_idx: 0,
                physical_line: 1,
                is_soft_wrap: false,
                whitespace_px_width: 0.0,
                text_px_width: 10.0,
                y_offset: 24.0,
                is_folded: false,
                fold_suffix: ['\0'; 4],
                fold_suffix_len: 0,
            },
            VisualLine {
                byte_idx: 4,
                physical_line: 2,
                is_soft_wrap: false,
                whitespace_px_width: 0.0,
                text_px_width: 10.0,
                y_offset: 48.0,
                is_folded: false,
                fold_suffix: ['\0'; 4],
                fold_suffix_len: 0,
            },
        ];

        assert!(!content_y_hits_visual_text_row(12.0, 24.0, &lines));
        assert!(content_y_hits_visual_text_row(24.0, 24.0, &lines));
        assert!(content_y_hits_visual_text_row(71.9, 24.0, &lines));
        assert!(!content_y_hits_visual_text_row(72.0, 24.0, &lines));
    }

    #[test]
    fn calendar_popup_stays_open_for_its_controls_and_closes_elsewhere() {
        assert!(!database_table_click_closes_cell_popup(
            UiId::DatabaseTableDateToday
        ));
        assert!(!database_table_click_closes_cell_popup(
            UiId::DatabaseTableDateDay(10)
        ));
        assert!(database_table_click_closes_cell_popup(
            UiId::DatabaseTableGridBody
        ));
        assert!(database_table_click_closes_cell_popup(
            UiId::DatabaseTableWhereInput
        ));
    }
}

impl App {
    fn editor_interaction_bottom_heights(&self, window_height: f32, scale: f32) -> (f32, f32) {
        if !self.is_ide_mode {
            return (0.0, 0.0);
        }
        let panel_bottom_height = self.ide_panel.editor_reserved_bottom_height(scale);
        let open_panel_height = if self.ide_panel.any_bottom_open() {
            self.ide_panel.bottom_height * scale
        } else {
            0.0
        };
        let query_results_height =
            self.tabs
                .get(self.active_tab)
                .map_or(0.0, |tab| match &tab.kind {
                    crate::app::EditorTabKind::DatabaseQuery(_, state)
                        if crate::app::database::database_query_results_visible(state) =>
                    {
                        crate::app::database::database_query_results_height(
                            state.result_view.preferred_height,
                            window_height,
                            open_panel_height,
                            scale,
                        )
                    }
                    _ => 0.0,
                });
        (panel_bottom_height, query_results_height)
    }

    fn editor_interaction_metrics(&self) -> Option<(f32, f32, f32, f32)> {
        let renderer = self.renderer.as_ref()?;
        let window_height = self
            .window
            .as_ref()
            .map_or(renderer.height, |window| window.inner_size().height as f32);
        let scale = renderer.scale_factor;
        let (panel_bottom_height, query_results_height) =
            self.editor_interaction_bottom_heights(window_height, scale);
        Some((
            window_height,
            scale,
            panel_bottom_height,
            query_results_height,
        ))
    }

    /// Обрабатывает клик по UI элементу
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn handle_ui_click(&mut self, id: UiId) {
        let same_click_target = self.last_click_ui_id == Some(id);
        self.last_click_ui_id = Some(id);
        match id {
            UiId::DatabasePanelBody
            | UiId::DatabaseGlobalErrorCopy
            | UiId::DatabaseAdd
            | UiId::DatabaseDelete
            | UiId::DatabaseRefresh
            | UiId::DatabaseConnectionRow(_)
            | UiId::DatabaseConnectionArrow(_)
            | UiId::DatabaseRow(_, _)
            | UiId::DatabaseArrow(_, _)
            | UiId::DatabaseTableRow(_, _, _)
            | UiId::DatabaseContextItem(_)
            | UiId::DatabaseDialogBackdrop
            | UiId::DatabaseDialogBody
            | UiId::DatabaseDialogField(_)
            | UiId::DatabaseDialogSecretEye(_)
            | UiId::DatabaseDialogTls
            | UiId::DatabaseDialogColor
            | UiId::DatabaseDialogSshToggle
            | UiId::DatabaseDialogJumpToggle
            | UiId::DatabaseDialogRememberPostgres
            | UiId::DatabaseDialogRememberSshPassword
            | UiId::DatabaseDialogRememberSshPassphrase
            | UiId::DatabaseDialogRememberJumpPassword
            | UiId::DatabaseDialogRememberJumpPassphrase
            | UiId::DatabaseDialogTest
            | UiId::DatabaseDialogSave
            | UiId::DatabaseDialogCancel
            | UiId::DatabaseDeleteConfirm
            | UiId::DatabaseDeleteCancel
            | UiId::DatabaseHostKeyTrustOnce
            | UiId::DatabaseHostKeyTrustStore
            | UiId::DatabaseHostKeyCancel
            | UiId::DatabaseDdlBody
            | UiId::DatabaseDdlScroll
            | UiId::DatabaseTableBody
            | UiId::DatabaseTableUnavailableText
            | UiId::DatabaseTableAddRow
            | UiId::DatabaseTableDeleteRows
            | UiId::DatabaseTableUndo
            | UiId::DatabaseTableSave
            | UiId::DatabaseTablePreview
            | UiId::DatabaseTableRefresh
            | UiId::DatabaseTablePageFirst
            | UiId::DatabaseTablePagePrevious
            | UiId::DatabaseTablePageNext
            | UiId::DatabaseTablePageLast
            | UiId::DatabaseTableLimit
            | UiId::DatabaseTableWhereInput
            | UiId::DatabaseTableOrderInput
            | UiId::DatabaseTableHeader(_)
            | UiId::DatabaseTableColumnResize(_)
            | UiId::DatabaseGridRow(_)
            | UiId::DatabaseTableCell(_, _)
            | UiId::DatabaseTableCellEditor
            | UiId::DatabaseTableEnumOption(_)
            | UiId::DatabaseTableEnumPreviousPage
            | UiId::DatabaseTableEnumNextPage
            | UiId::DatabaseTableDatePreviousMonth
            | UiId::DatabaseTableDateNextMonth
            | UiId::DatabaseTableDateDay(_)
            | UiId::DatabaseTableDateToday
            | UiId::DatabaseTableDateNow
            | UiId::DatabaseTableGridBody
            | UiId::DatabaseTableScrollY
            | UiId::DatabaseTableScrollX
            | UiId::DatabaseQueryScrollY
            | UiId::DatabaseQueryScrollX
            | UiId::DatabaseTableModalBackdrop
            | UiId::DatabaseTableModalBody
            | UiId::DatabaseTableModalInput
            | UiId::DatabaseTableModalPrimary
            | UiId::DatabaseTableModalSecondary
            | UiId::DatabaseTableModalTertiary
            | UiId::DatabaseTableModalScroll
            | UiId::DatabaseTableModalScrollX
            | UiId::DatabaseQueryRun
            | UiId::DatabaseQueryCancel
            | UiId::DatabaseQueryExplain
            | UiId::DatabaseQueryExplainAnalyze
            | UiId::DatabaseQueryFormat
            | UiId::DatabaseQueryHistory
            | UiId::DatabaseQueryNextDiagnostic
            | UiId::DatabaseQueryResultTab(_)
            | UiId::DatabaseQueryHistoryEntry(_)
            | UiId::DatabaseQueryResultBody
            | UiId::DatabaseQueryResultResize
            | UiId::DatabaseQueryColumnResize(_)
            | UiId::DatabaseQueryReviewBackdrop
            | UiId::DatabaseQueryReviewBody
            | UiId::DatabaseQueryReviewMessagesBody
            | UiId::DatabaseQueryReviewMessagesScrollY
            | UiId::DatabaseQueryCommit
            | UiId::DatabaseQueryRollback => {
                self.handle_database_ui_click(id, same_click_target);
            }
            UiId::ApiImportAdd
            | UiId::ApiImportFile
            | UiId::ApiImportUrl
            | UiId::ApiImportUrlInput
            | UiId::ApiImportUrlConfirm
            | UiId::ApiSpecSelect(_)
            | UiId::ApiSpecOpen(_)
            | UiId::ApiSpecRefresh(_)
            | UiId::ApiSpecRemove(_)
            | UiId::ApiSpecRemoveConfirm
            | UiId::ApiSpecRemoveCancel
            | UiId::ApiAuthRoot
            | UiId::ApiRoutesRoot
            | UiId::ApiRouteFilterInput
            | UiId::ApiRouteFilterClear
            | UiId::ApiRouteTag(_)
            | UiId::ApiRouteRow(_)
            | UiId::ApiRoutePathText(_)
            | UiId::ApiRouteSummaryText(_)
            | UiId::ApiRouteDescriptionText(_)
            | UiId::ApiServerSelect(_)
            | UiId::ApiAuthValue(_)
            | UiId::ApiAuthRefreshToken(_)
            | UiId::ApiAuthUsername(_)
            | UiId::ApiAuthPassword(_)
            | UiId::ApiAuthAccessSave(_)
            | UiId::ApiAuthAccessClear(_)
            | UiId::ApiAuthRefreshSave(_)
            | UiId::ApiAuthRefreshClear(_)
            | UiId::ApiAuthSave(_)
            | UiId::ApiAuthClear(_)
            | UiId::ApiTryRequest
            | UiId::ApiPathParamInput(_, _)
            | UiId::ApiQueryParamInput(_, _)
            | UiId::ApiPathParamAllowedValue(_, _, _)
            | UiId::ApiQueryParamAllowedValue(_, _, _)
            | UiId::ApiBodyInput(_)
            | UiId::ApiInputExampleTab(_)
            | UiId::ApiInputSchemaTab(_)
            | UiId::ApiInputSchemaMenu(_)
            | UiId::ApiInputSchemaMenuItem(_, _)
            | UiId::ApiInputSchemaBody(_)
            | UiId::ApiInputSchemaFold(_, _)
            | UiId::ApiBodyScrollY(_)
            | UiId::ApiBodyScrollX(_)
            | UiId::ApiBodyFieldInput(_, _)
            | UiId::ApiBodyAllowedValue(_, _, _)
            | UiId::ApiBodyFilePick(_, _)
            | UiId::ApiOutputExampleTab(_)
            | UiId::ApiOutputSchemaTab(_)
            | UiId::ApiOutputStatusTab(_, _)
            | UiId::ApiOutputSchemaMenu(_)
            | UiId::ApiOutputSchemaMenuItem(_, _)
            | UiId::ApiOutputSchemaBody(_)
            | UiId::ApiOutputSchemaFold(_, _)
            | UiId::ApiOutputScrollY(_)
            | UiId::ApiOutputScrollX(_)
            | UiId::ApiResponseBodyTab(_)
            | UiId::ApiResponseHeadersTab(_)
            | UiId::ApiResponseCurlTab(_)
            | UiId::ApiResponseBody(_)
            | UiId::ApiResponseScrollY(_)
            | UiId::ApiResponseScrollX(_)
            | UiId::ApiResponseUseAccessToken(_, _)
            | UiId::ApiResponseSaveRefreshToken(_, _)
            | UiId::ApiMockServerToggle
            | UiId::ApiMockServerDetails
            | UiId::ApiMockServerCopyUrl
            | UiId::ApiMockServerDetailsClose
            | UiId::ApiMockServerLogArea
            | UiId::ApiMockServerLogScrollY
            | UiId::ApiMockModeSelect
            | UiId::ApiMockProxyBaseInput
            | UiId::ApiMockGuideOpen
            | UiId::ApiMockGuideClose
            | UiId::ApiMockGuideBody
            | UiId::ApiMockGuideScrollY
            | UiId::ApiMockPythonManage
            | UiId::ApiMockPythonManageClose
            | UiId::ApiMockPythonModeToggle
            | UiId::ApiMockPythonCheckRuntime
            | UiId::ApiMockPythonPrepareVersion
            | UiId::ApiMockPythonPickUvPath
            | UiId::ApiMockPythonPickCustomPath
            | UiId::ApiMockPythonVersionOption(_)
            | UiId::ApiMockPythonUvPathInput
            | UiId::ApiMockPythonVersionInput
            | UiId::ApiMockPythonCustomPathInput
            | UiId::ApiMockExportOpenApi
            | UiId::ApiMockRouteEnable(_)
            | UiId::ApiMockRouteDetailsToggle(_)
            | UiId::ApiMockRoutePythonToggle(_)
            | UiId::ApiMockRouteReset(_)
            | UiId::ApiMockRouteResetConfirm
            | UiId::ApiMockRouteResetCancel
            | UiId::ApiMockContractPathToggle(_)
            | UiId::ApiMockContractQueryToggle(_)
            | UiId::ApiMockContractBodyToggle(_)
            | UiId::ApiMockContractPathFieldToggle(_, _)
            | UiId::ApiMockContractQueryFieldToggle(_, _)
            | UiId::ApiMockContractBodyFieldToggle(_, _)
            | UiId::ApiMockContractFieldRequired(_, _, _)
            | UiId::ApiMockContractFieldNullable(_, _, _)
            | UiId::ApiMockContractFieldRemove(_, _, _)
            | UiId::ApiMockContractFieldRemoveConfirm
            | UiId::ApiMockContractFieldRemoveCancel
            | UiId::ApiMockContractFieldPropInput(_, _, _, _)
            | UiId::ApiMockContractFieldAddConstraint(_, _, _)
            | UiId::ApiMockContractFieldAddConstraintOption(_, _, _, _)
            | UiId::ApiMockStaticResponseInput(_)
            | UiId::ApiMockStaticResponseScrollY(_)
            | UiId::ApiMockStaticResponseScrollX(_)
            | UiId::ApiMockCombinedPython(_)
            | UiId::ApiMockContractInput(_)
            | UiId::ApiMockSignatureInput(_)
            | UiId::ApiMockPreludeInput(_)
            | UiId::ApiMockBodyInput(_)
            | UiId::ApiMockContractReset(_)
            | UiId::ApiMockPreludeReset(_)
            | UiId::ApiMockBodyReset(_)
            | UiId::ApiMockAddInputField(_)
            | UiId::ApiMockAddOutputField(_)
            | UiId::ApiMockAddManualRoute
            | UiId::ApiMockManualRouteOpen(_)
            | UiId::ApiMockManualRouteMethod(_)
            | UiId::ApiMockManualRoutePath(_)
            | UiId::ApiMockManualRouteRemove(_)
            | UiId::ApiTabBody => {
                self.handle_api_client_click(id, same_click_target);
            }
            UiId::HoverPopupScroll
            | UiId::StatusBar
            | UiId::SearchPanelBody
            | UiId::ProjectSearchPanelBody
            | UiId::ProjectSearchHelpPopup
            | UiId::InlineGitPanelBody
            | UiId::GitDiffPanelBody => {}
            UiId::MarkdownModeToggle => {
                self.toggle_markdown_mode();
            }
            UiId::StatusDiagnostics => {
                self.ide_panel.toggle(crate::app::PanelId::Problems);
                crate::save_panel_state(&self.ide_panel);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::TerminalBody => {
                self.is_dragging = true;
                self.search_focused = false;
                self.ide_panel.term_search_focused = false;
                self.ide_panel.file_tree_focused = false;
                let active = self.ide_panel.active_terminal;
                if let Some(term) = self.ide_panel.terminals.get_mut(active) {
                    crate::app::terminal::lock_terminal_grid(&term.grid).selection = None;
                }
            }
            UiId::TerminalScrollY => {
                let active = self.ide_panel.active_terminal;
                if let Some(term) = self.ide_panel.terminals.get_mut(active) {
                    term.scroll_y.is_dragging = true;
                }
            }
            UiId::TerminalTab(idx) => {
                self.select_terminal_tab_from_user(idx);
            }
            UiId::TerminalTabClose(idx) => {
                self.close_terminal_tab_at(idx);
            }
            UiId::TerminalAdd => {
                self.add_terminal();
            }
            UiId::TerminalSearchClose => {
                self.ide_panel.term_show_search = false;
                self.ide_panel.term_search_focused = false;
                self.ide_panel.term_search_results.clear();
                self.ide_panel.term_search_current_idx = None;
                if let Some(term) = self
                    .ide_panel
                    .terminals
                    .get_mut(self.ide_panel.active_terminal)
                {
                    crate::app::terminal::lock_terminal_grid(&term.grid).selection = None;
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::TerminalSearchNext => {
                if !self.ide_panel.term_search_results.is_empty() {
                    if let Some(idx) = self.ide_panel.term_search_current_idx {
                        self.ide_panel.term_search_current_idx =
                            Some((idx + 1) % self.ide_panel.term_search_results.len());
                    }
                    self.jump_to_terminal_search_result();
                    self.window.as_ref().unwrap().request_redraw();
                }
            }
            UiId::TerminalSearchPrev => {
                if !self.ide_panel.term_search_results.is_empty() {
                    if let Some(idx) = self.ide_panel.term_search_current_idx {
                        self.ide_panel.term_search_current_idx = Some(if idx == 0 {
                            self.ide_panel.term_search_results.len() - 1
                        } else {
                            idx - 1
                        });
                    }
                    self.jump_to_terminal_search_result();
                    self.window.as_ref().unwrap().request_redraw();
                }
            }
            UiId::TerminalSearchCaseToggle => {
                self.ide_panel.term_search_case_sensitive =
                    !self.ide_panel.term_search_case_sensitive;
                self.update_terminal_search();
                self.jump_to_terminal_search_result();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::TerminalSearchInput => {
                self.ide_panel.term_search_focused = true;
                self.search_focused = false;
                self.ide_panel.git.message_focused = false;
                self.is_dragging_search = true;
                let input_rect = self.ui_registry.rect_for(UiId::TerminalSearchInput);
                if let (Some(rect), Some(r)) = (input_rect, self.renderer.as_mut()) {
                    let text = self.ide_panel.term_search_editor.get_full_text();
                    let x_offset = (r.last_mouse_x - (rect.0 + 5.0 * r.scale_factor)
                        + r.terminal_search_scroll_x)
                        .max(0.0);
                    let target_idx = r.one_line_cursor_from_x(&text, x_offset, 1.0);
                    self.ide_panel.term_search_editor.cursor = target_idx;
                    self.ide_panel.term_search_editor.selection_anchor = Some(target_idx);
                }
                self.window.as_ref().unwrap().request_redraw();
            }
            // Welcome screen
            UiId::WelcomeNewFile => {
                self.show_welcome = false;
                self.is_ide_mode = false;
                if self.file_path.is_some() || self.editor.is_dirty() || self.editor.len() > 0 {
                    self.open_new_tab();
                } else {
                    self.file_path = None;
                    self.file_key = None;
                    self.text_file_format = crate::platform::TextFileFormat::default();
                    self.base_title = "Безымянный".to_string();
                    let old_version = self.editor.version;
                    self.editor = Editor::new(8192);
                    self.editor.version = old_version + 1;
                    self.markdown = Default::default();
                    self.editor.set_original_text();
                    self.editor.sync_edits.clear();
                    while let Ok(_) = self.highlighter.rx.try_recv() {}
                    self.highlighter
                        .reset(self.editor.version, "".to_string(), "".to_string(), 0);
                }
                App::update_window_title(self.window.as_ref().unwrap(), &self.base_title, false);
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::WelcomeOpenFile => {
                self.show_welcome = false;
                self.is_ide_mode = false;
                self.trigger_file_picker();
            }
            UiId::WelcomeIdeMode => {
                self.enter_ide_mode();
            }
            UiId::WelcomeRecentFile(idx) => {
                if idx < self.recent_files.len() {
                    let path = self.recent_files[idx].clone();
                    self.show_welcome = false;
                    self.is_ide_mode = false;
                    self.open_file_in_tab(path, true);
                    self.window.as_ref().unwrap().request_redraw();
                }
            }

            // Dialog
            UiId::DialogSave => {
                self.begin_pending_action_save();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::DialogDiscard => {
                self.discard_pending_action_changes();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::DialogCancel => {
                self.cancel_pending_action();
                self.window.as_ref().unwrap().request_redraw();
            }

            // Settings tabs
            UiId::SettingsTab(idx) => {
                self.settings_tab = idx;
                self.window.as_ref().unwrap().request_redraw();
            }
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
                        let widths = self
                            .ide_ignore_patterns
                            .iter()
                            .map(|pattern| {
                                self.renderer
                                    .as_mut()
                                    .unwrap()
                                    .measure_ui_width(pattern, 0.88)
                                    + pad_x * 2.0
                                    + 22.0 * s
                            })
                            .collect::<Vec<_>>();
                        let max_scroll = crate::render_view::settings_ui::settings_ide_max_scroll(
                            layout,
                            self.ide_workspaces.len(),
                            widths,
                            s,
                        );
                        crate::app::mouse::begin_scrollbar_drag(
                            &mut self.settings_ide_scroll,
                            pointer,
                            rect.1,
                            rect.3,
                            max_scroll,
                            40.0 * s,
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
                    crate::app::mouse::begin_scrollbar_drag(
                        &mut self.settings_scroll,
                        pointer,
                        rect.1,
                        rect.3,
                        max_scroll,
                        40.0 * s,
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
                    _ => return,
                };
                if let Err(error) =
                    std::fs::create_dir_all(&path).and_then(|_| crate::platform::reveal_path(&path))
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

            // LSP panel
            UiId::LspServerRestart(idx) => {
                if let Some(name) = self.ide_panel.lsp_servers.get(idx).map(|info| info.name)
                    && let Some(lsp) = &mut self.lsp
                {
                    lsp.restart_server(name);
                    self.ide_panel.lsp_servers = lsp.servers_info();
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::LspServerToggle(idx) => {
                if let Some(info) = self.ide_panel.lsp_servers.get(idx).cloned()
                    && let Some(lsp) = &mut self.lsp
                {
                    let enable = matches!(
                        info.status,
                        crate::lsp::LspServerStatus::Disabled
                            | crate::lsp::LspServerStatus::Missing
                    );
                    lsp.set_server_enabled(info.name, enable);
                    self.ide_panel.lsp_servers = lsp.servers_info();
                    if self.ide_panel.lsp_servers.iter().all(|server| {
                        matches!(server.status, crate::lsp::LspServerStatus::Disabled)
                    }) {
                        if let Some(slot) = self
                            .ide_panel
                            .slots
                            .iter_mut()
                            .find(|slot| slot.id == crate::app::PanelId::LspServers)
                        {
                            slot.open = false;
                        }
                        self.ide_panel.lsp_logs_focused = None;
                        crate::save_panel_state(&self.ide_panel);
                    }
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::LspServerStop(idx) => {
                if let Some(name) = self.ide_panel.lsp_servers.get(idx).map(|info| info.name)
                    && let Some(lsp) = &mut self.lsp
                {
                    lsp.stop_server(name);
                    self.ide_panel.lsp_servers = lsp.servers_info();
                    if self.ide_panel.lsp_servers.iter().all(|server| {
                        matches!(server.status, crate::lsp::LspServerStatus::Disabled)
                    }) {
                        if let Some(slot) = self
                            .ide_panel
                            .slots
                            .iter_mut()
                            .find(|slot| slot.id == crate::app::PanelId::LspServers)
                        {
                            slot.open = false;
                        }
                        self.ide_panel.lsp_logs_focused = None;
                        crate::save_panel_state(&self.ide_panel);
                    }
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::LspServerLogs(idx) => {
                if idx < self.ide_panel.lsp_servers.len() {
                    let name = self.ide_panel.lsp_servers[idx].name.to_string();
                    if self.ide_panel.lsp_logs_expanded.contains(&name) {
                        self.ide_panel.lsp_logs_expanded.remove(&name);
                    } else {
                        self.ide_panel.lsp_logs_expanded.insert(name);
                    }
                }
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::LspServerClearLogs(idx) => {
                if idx < self.ide_panel.lsp_servers.len() {
                    let name = self.ide_panel.lsp_servers[idx].name.to_string();
                    if let Some(lsp) = &mut self.lsp {
                        lsp.clear_server_logs(&name);
                        self.ide_panel.lsp_servers = lsp.servers_info();
                    }
                    self.ide_panel.lsp_log_editors.remove(&name);
                    self.ide_panel.lsp_log_source_counts.remove(&name);
                    self.ide_panel.lsp_logs_scroll_y.remove(&name);
                    self.ide_panel.lsp_logs_scroll_x.remove(&name);
                    if self.ide_panel.lsp_logs_focused.as_deref() == Some(name.as_str()) {
                        self.ide_panel.lsp_logs_focused = None;
                    }
                }
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::LspServerFixAll(idx) => {
                if self.markdown_mode() == crate::app::MarkdownMode::Read {
                    self.show_readonly_notice();
                    return;
                }
                if let Some(lsp) = &mut self.lsp {
                    if idx < self.ide_panel.lsp_servers.len() {
                        if let Some(path) = self.file_path.clone() {
                            if let Some(request_id) =
                                lsp.request_fix_all(&path, &self.file_extension)
                            {
                                self.pending_fix_all_id = Some(request_id);
                            }
                        }
                    }
                }
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::LspLogFoldToggle(server_idx, line_idx) => {
                if server_idx < self.ide_panel.lsp_servers.len() {
                    let name = self.ide_panel.lsp_servers[server_idx].name;
                    if let Some(ed) = self.ide_panel.lsp_log_editors.get_mut(name) {
                        let is_folded = ed.folded_lines.contains(&line_idx);
                        if let Some(&end_idx) = ed.foldable_lines.get(&line_idx) {
                            if is_folded {
                                for i in line_idx..=end_idx {
                                    ed.folded_lines.remove(&i);
                                    if i < ed.line_offsets.len() {
                                        ed.folded_start_bytes.remove(&ed.line_offsets[i]);
                                    }
                                }
                            } else {
                                for i in line_idx..=end_idx {
                                    if ed.foldable_lines.contains_key(&i) {
                                        ed.folded_lines.insert(i);
                                        if i < ed.line_offsets.len() {
                                            ed.folded_start_bytes.insert(ed.line_offsets[i]);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                self.window.as_ref().unwrap().request_redraw();
            }

            // Git panel
            UiId::GitWorkspaceToggle(workspace_idx) => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                self.toggle_git_workspace(workspace_idx);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitFile(workspace_idx, file_idx) => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                self.toggle_git_file_stage(workspace_idx, file_idx);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitFileDiff(workspace_idx, file_idx) => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                let (mx, my) = self
                    .renderer
                    .as_ref()
                    .map(|r| (r.last_mouse_x, r.last_mouse_y))
                    .unwrap_or((0.0, 0.0));
                let now = std::time::Instant::now();
                let dx = mx - self.last_click_pos.0;
                let dy = my - self.last_click_pos.1;
                let double_click = repeated_ui_click(
                    same_click_target,
                    now.duration_since(self.last_click_time),
                    dx,
                    dy,
                );
                self.ide_panel.git.selected_file = Some((workspace_idx, file_idx));
                self.last_click_time = now;
                self.last_click_pos = (mx, my);
                if double_click {
                    self.open_git_diff_tab(workspace_idx, file_idx);
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitFolderStage(workspace_idx, row_idx) => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                self.toggle_git_folder_stage(workspace_idx, row_idx);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitFolder(workspace_idx, row_idx) => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                self.toggle_git_tree_folder(workspace_idx, row_idx);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitCommit => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                self.commit_git_panel();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitCommitMenuToggle => {
                self.ide_panel.git.close_repo_action_menu();
                if self.ide_panel.git.commit_enabled() && !self.ide_panel.git.pending {
                    self.ide_panel
                        .git
                        .toggle_commit_menu(std::time::Instant::now());
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitCommitMenuItem(idx) => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                if self.ide_panel.git.commit_enabled() && !self.ide_panel.git.pending {
                    self.commit_git_panel_option(idx);
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitCommitOptionsToggle => {
                self.ide_panel.git.close_repo_action_menu();
                if self.ide_panel.git.commit_enabled() && !self.ide_panel.git.pending {
                    self.ide_panel
                        .git
                        .toggle_commit_options_menu(std::time::Instant::now());
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitCommitOptionsItem(0) => {
                if !self.ide_panel.git.pending {
                    self.ide_panel.git.commit_options.skip_hooks =
                        !self.ide_panel.git.commit_options.skip_hooks;
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitCommitOptionsItem(_) => {}
            UiId::GitPush(workspace_idx) => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                self.push_git_workspace(workspace_idx);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitRollbackStaged(workspace_idx) => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                self.open_git_rollback_staged_dialog(workspace_idx);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitStageAll(workspace_idx) => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                self.stage_all_git_workspace(workspace_idx);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitUnstageAll(workspace_idx) => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                self.open_git_unstage_all_dialog(workspace_idx);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitRepoActionMenu(workspace_idx) => {
                self.ide_panel.git.close_commit_menus();
                if !self.ide_panel.git.pending {
                    self.ide_panel
                        .git
                        .toggle_repo_action_menu(workspace_idx, std::time::Instant::now());
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitFetch(workspace_idx) => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                self.fetch_git_workspace(workspace_idx);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitPull(workspace_idx) => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                self.pull_git_workspace(workspace_idx);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitConfirmAction => {
                self.ide_panel.git.close_repo_action_menu();
                self.confirm_git_dialog();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitConfirmCancel => {
                self.ide_panel.git.confirm_dialog = None;
                self.ide_panel.git.close_repo_action_menu();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitRefresh => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                self.refresh_git_panel_window();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitGraphToggle => {
                self.ide_panel.git.close_repo_action_menu();
                self.toggle_git_graph();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitLogsToggle => {
                self.ide_panel.git.close_repo_action_menu();
                self.toggle_git_logs();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitLogsClear => {
                self.ide_panel.git.clear_git_logs();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitGraphWorkspace(workspace_idx) => {
                self.ide_panel.git.close_repo_action_menu();
                self.select_git_graph_workspace(workspace_idx);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitGraphResize
            | UiId::GitGraphScroll
            | UiId::GitGraphCommit(_, _)
            | UiId::GitLogsBody => {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitGraphCopyCommit(workspace_idx, commit_idx) => {
                self.copy_git_graph_commit(workspace_idx, commit_idx);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitGraphOpenCommit(workspace_idx, commit_idx) => {
                self.open_git_graph_commit(workspace_idx, commit_idx);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::GitMessageInput => {
                self.ide_panel.git.close_commit_menus();
                self.ide_panel.git.close_repo_action_menu();
                let was_focused = self.ide_panel.git.message_focused;
                self.ide_panel.git.message_focused = true;
                self.search_focused = false;
                self.ide_panel.term_search_focused = false;
                self.ide_panel.lsp_log_filter_focused = false;
                self.ide_panel.file_tree_focused = false;
                self.is_dragging_search = true;
                let input_rect = self.ui_registry.rect_for(UiId::GitMessageInput);
                if let (Some(rect), Some(r)) = (input_rect, self.renderer.as_mut()) {
                    if !was_focused {
                        r.git_commit_scroll_x = 0.0;
                    }
                    let x_offset = (r.last_mouse_x - (rect.0 + 5.0 * r.scale_factor)
                        + r.git_commit_scroll_x)
                        .max(0.0);
                    let text = self.ide_panel.git.message_editor.get_full_text();
                    let target_idx = r.one_line_cursor_from_x(&text, x_offset, 1.0);
                    self.ide_panel.git.message_editor.cursor = target_idx;
                    self.ide_panel.git.message_editor.selection_anchor = Some(target_idx);
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }

            // Sidebar
            UiId::SidebarSlot(panel_id) => {
                self.ide_panel.toggle(panel_id);
                if panel_id == crate::app::PanelId::Terminal && self.ide_panel.is_open(panel_id) {
                    self.ide_panel.terminal_focused = true;
                    self.ide_panel.term_search_focused = false;
                    if self.ide_panel.terminals.is_empty() {
                        self.add_terminal();
                    }
                    self.defer_terminal_panel_until_ready();
                }
                if panel_id == crate::app::PanelId::Explorer && self.ide_panel.is_open(panel_id) {
                    if self.ide_panel.file_tree_nodes.is_empty() {
                        self.refresh_file_tree();
                        self.start_file_watcher();
                    }
                }
                if panel_id == crate::app::PanelId::Git && self.ide_panel.is_open(panel_id) {
                    self.refresh_git_panel();
                }
                if panel_id == crate::app::PanelId::Search && self.ide_panel.is_open(panel_id) {
                    self.ide_panel.project_search.focused =
                        Some(crate::app::project_search::ProjectSearchField::Query);
                }
                if panel_id == crate::app::PanelId::Database && self.ide_panel.is_open(panel_id) {
                    self.reconcile_expanded_database_connections();
                }
                crate::save_panel_state(&self.ide_panel);
                self.window.as_ref().unwrap().request_redraw();
            }

            // File tree
            UiId::FileTreeNode(idx) => {
                self.handle_file_tree_left_click(idx, false, same_click_target);
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::FileTreeArrow(idx) => {
                self.handle_file_tree_left_click(idx, true, same_click_target);
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::FileTreeScrollY => {}
            UiId::FileTreeMenuItem(idx) => {
                self.handle_file_tree_context_item(idx);
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::FileTreeCreateInput => {
                let kind = crate::app::file_tree::FileTreeDialogInputKind::Create;
                if let Some(mx) = self.renderer.as_ref().map(|r| r.last_mouse_x) {
                    if let Some(target_idx) = self.file_tree_dialog_input_index_at(kind, mx) {
                        self.set_file_tree_dialog_input_cursor(kind, target_idx, true);
                        self.ide_panel.file_tree_dialog_input_drag = Some(kind);
                    }
                }
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::FileTreeCreateConfirm => {
                self.submit_file_tree_create_dialog();
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::FileTreeCreateCancel => {
                self.ide_panel.file_tree_create_dialog = None;
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::FileTreeRenameInput => {
                let kind = crate::app::file_tree::FileTreeDialogInputKind::Rename;
                if let Some(mx) = self.renderer.as_ref().map(|r| r.last_mouse_x) {
                    if let Some(target_idx) = self.file_tree_dialog_input_index_at(kind, mx) {
                        self.set_file_tree_dialog_input_cursor(kind, target_idx, true);
                        self.ide_panel.file_tree_dialog_input_drag = Some(kind);
                    }
                }
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::FileTreeRenameConfirm => {
                self.submit_file_tree_rename_dialog();
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::FileTreeRenameCancel => {
                self.ide_panel.file_tree_rename_dialog = None;
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::FileTreeMoveConfirm => {
                self.finish_file_tree_move();
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::FileTreeMoveCancel => {
                self.ide_panel.file_tree_move_dialog = None;
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::FileTreeDeleteConfirm => {
                let _ = self.confirm_file_tree_delete();
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::FileTreeDeleteCancel => {
                self.ide_panel.file_tree_delete_dialog = None;
                self.window.as_ref().unwrap().request_redraw();
            }

            // Project search
            UiId::ProjectSearchQueryInput => {
                self.focus_project_search_field(
                    crate::app::project_search::ProjectSearchField::Query,
                );
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::ProjectSearchIncludeInput => {
                self.focus_project_search_field(
                    crate::app::project_search::ProjectSearchField::Include,
                );
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::ProjectSearchExcludeInput => {
                self.focus_project_search_field(
                    crate::app::project_search::ProjectSearchField::Exclude,
                );
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::ProjectSearchFilterInput => {
                self.focus_project_search_field(
                    crate::app::project_search::ProjectSearchField::Filter,
                );
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::ProjectSearchRun => {
                self.start_project_search();
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::ProjectSearchCaseToggle => {
                self.ide_panel.project_search.case_sensitive =
                    !self.ide_panel.project_search.case_sensitive;
                self.ide_panel.project_search.dirty = true;
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::ProjectSearchHelp => {
                self.ide_panel.project_search.help_open = !self.ide_panel.project_search.help_open;
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::ProjectSearchFileToggle(file_idx) => {
                self.ide_panel.project_search.toggle_file(file_idx);
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::ProjectSearchMatchJump(file_idx, match_idx) => {
                self.handle_project_search_match_click(file_idx, match_idx);
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::ProjectSearchQueryScrollbarY
            | UiId::ProjectSearchQueryScrollbarX
            | UiId::ProjectSearchScrollbar => {}

            // Search panel
            UiId::SearchClose => {
                self.show_search = false;
                self.search_focused = false;
                self.search_results.clear();
                self.search_current_idx = None;
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::SearchNext => {
                if !self.search_results.is_empty() {
                    if let Some(idx) = self.search_current_idx {
                        self.search_current_idx = Some((idx + 1) % self.search_results.len());
                    }
                    self.jump_to_search_result();
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                }
            }
            UiId::SearchPrev => {
                if !self.search_results.is_empty() {
                    if let Some(idx) = self.search_current_idx {
                        self.search_current_idx = Some(if idx == 0 {
                            self.search_results.len() - 1
                        } else {
                            idx - 1
                        });
                    }
                    self.jump_to_search_result();
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                }
            }
            UiId::SearchCaseToggle => {
                self.search_case_sensitive = !self.search_case_sensitive;
                self.update_search();
                self.jump_to_search_result();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }

            // Tabs
            UiId::EditorTab(idx) => {
                self.switch_to_tab(idx);
            }
            UiId::EditorTabClose(idx) => {
                self.close_tab_at(idx);
            }

            // Editor
            UiId::EditorFoldArrow(phys_idx) => {
                if self.editor.folded_lines.contains(&phys_idx) {
                    self.editor.folded_lines.remove(&phys_idx);
                    self.editor
                        .folded_start_bytes
                        .remove(&self.editor.line_offsets[phys_idx]);
                } else if self.editor.foldable_lines.contains_key(&phys_idx) {
                    self.editor.folded_lines.insert(phys_idx);
                    self.editor
                        .folded_start_bytes
                        .insert(self.editor.line_offsets[phys_idx]);
                }
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::EditorGitHunk(hunk_idx, clicked_line) => {
                self.show_inline_git_hunk_popup(hunk_idx, clicked_line.saturating_add(1));
            }
            UiId::InlineGitPrevHunk => {
                self.jump_inline_git_hunk(-1);
            }
            UiId::InlineGitNextHunk => {
                self.jump_inline_git_hunk(1);
            }
            UiId::InlineGitRollbackHunk => {
                self.rollback_inline_git_hunk();
            }
            UiId::GitDiffRollbackHunk(tab_idx, hunk_idx) => {
                if tab_idx == self.active_tab {
                    self.rollback_active_git_diff_hunk(hunk_idx);
                }
            }
            UiId::GitDiffPrevHunk => {
                self.jump_active_git_diff_hunk(-1);
            }
            UiId::GitDiffNextHunk => {
                self.jump_active_git_diff_hunk(1);
            }
            UiId::EditorFoldDots(phys_idx) => {
                self.editor.folded_lines.remove(&phys_idx);
                self.editor
                    .folded_start_bytes
                    .remove(&self.editor.line_offsets[phys_idx]);
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::StickyLine(target_byte, slot_index) => {
                self.editor.cursor = target_byte;
                self.editor.selection_anchor = None;
                let database_query_tab = self.active_tab_is_database_query();
                let metrics = self.editor_interaction_metrics();
                if let (Some((wh, s, panel_bottom_h, query_results_h)), Some(r)) =
                    (metrics, self.renderer.as_mut())
                {
                    let phys_line = self
                        .editor
                        .line_offsets
                        .partition_point(|&o| o <= target_byte)
                        .saturating_sub(1);
                    let visual_line = r
                        .phys_to_visual
                        .get(phys_line)
                        .copied()
                        .unwrap_or(phys_line);
                    let line_y = visual_line as f32 * r.line_height;
                    let tab_bar_h = crate::render_view::editor_content_top_inset(
                        self.show_welcome,
                        self.is_ide_mode,
                        database_query_tab,
                        s,
                    );
                    let editor_height = editor_interaction_view_height(
                        wh,
                        tab_bar_h,
                        panel_bottom_h,
                        query_results_h,
                        self.is_ide_mode,
                        s,
                    );
                    let max_scroll = r.get_max_scroll(&self.editor, editor_height);
                    let ry = slot_index as f32 * r.line_height;
                    let padding = r.line_height * 3.0;
                    self.scroll_y.animate_to(
                        (line_y - ry - padding)
                            .max(0.0)
                            .clamp(0.0, max_scroll)
                            .round(),
                    );
                    self.scroll_y.anim_speed = 15.0;
                }
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::SearchInput => {
                self.search_focused = true;
                self.ide_panel.term_search_focused = false;
                self.ide_panel.git.message_focused = false;
                self.ide_panel.file_tree_focused = false;
                self.is_dragging_search = true;
                let input_rect = self.ui_registry.rect_for(UiId::SearchInput);
                if let (Some(rect), Some(r)) = (input_rect, self.renderer.as_mut()) {
                    let text = self.search_editor.get_full_text();
                    let x_offset = (r.last_mouse_x - (rect.0 + 5.0 * r.scale_factor)
                        + r.search_scroll_x)
                        .max(0.0);
                    let target_idx = r.one_line_cursor_from_x(&text, x_offset, 1.0);
                    self.search_editor.cursor = target_idx;
                    self.search_editor.selection_anchor = Some(target_idx);
                }
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::EditorScrollbarY => {
                let database_query_tab = self.active_tab_is_database_query();
                let metrics = self.editor_interaction_metrics();
                if let (Some((wh, s, panel_bottom_h, query_results_h)), Some(r)) =
                    (metrics, self.renderer.as_mut())
                {
                    self.scroll_y.is_dragging = true;
                    let mx = r.last_mouse_x;
                    let my = r.last_mouse_y;
                    self.last_click_pos = (mx, my);

                    let tab_bar_h = crate::render_view::editor_content_top_inset(
                        self.show_welcome,
                        self.is_ide_mode,
                        database_query_tab,
                        s,
                    );
                    let editor_height = editor_interaction_view_height(
                        wh,
                        tab_bar_h,
                        panel_bottom_h,
                        query_results_h,
                        self.is_ide_mode,
                        s,
                    );
                    let max_scroll = r.get_max_scroll(&self.editor, editor_height);

                    if max_scroll > 0.0 {
                        let total_content_height = editor_scroll_content_height(
                            self.editor.get_visible_lines_count(),
                            r.line_height,
                            editor_height,
                        );
                        let thumb_h = (editor_height / total_content_height.max(editor_height)
                            * editor_height)
                            .max(20.0 * s);
                        let track_h = editor_height;

                        let scroll_ratio = (self.scroll_y.current / max_scroll).clamp(0.0, 1.0);
                        let thumb_y = tab_bar_h + scroll_ratio * (track_h - thumb_h);

                        if my >= thumb_y && my <= thumb_y + thumb_h {
                            self.scroll_y.drag_offset = my - thumb_y;
                            self.last_click_time = std::time::Instant::now();
                        } else {
                            self.scroll_y.drag_offset = thumb_h / 2.0;
                            let new_ratio = (my - tab_bar_h - self.scroll_y.drag_offset)
                                / (track_h - thumb_h).max(0.0001);
                            self.scroll_y.target =
                                (new_ratio * max_scroll).clamp(0.0, max_scroll).round();
                            self.scroll_y.anim_speed = 15.0;
                            self.last_click_time =
                                std::time::Instant::now() - std::time::Duration::from_millis(200);
                        }
                    } else {
                        self.last_click_time = std::time::Instant::now();
                    }
                }
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::EditorMinimap => {
                self.scroll_y.is_dragging = true;
                let database_query_tab = self.active_tab_is_database_query();
                let metrics = self.editor_interaction_metrics();
                if let (Some((wh, s, panel_bottom_h, query_results_h)), Some(r)) =
                    (metrics, self.renderer.as_mut())
                {
                    let mx = r.last_mouse_x;
                    let my = r.last_mouse_y;
                    self.last_click_pos = (mx, my);
                    self.last_click_time =
                        std::time::Instant::now() - std::time::Duration::from_millis(200);

                    let tab_bar_h = crate::render_view::editor_content_top_inset(
                        self.show_welcome,
                        self.is_ide_mode,
                        database_query_tab,
                        s,
                    );
                    let editor_height = editor_interaction_view_height(
                        wh,
                        tab_bar_h,
                        panel_bottom_h,
                        query_results_h,
                        self.is_ide_mode,
                        s,
                    );
                    let max_scroll = r.get_max_scroll(&self.editor, editor_height);

                    if max_scroll > 0.0 {
                        let total_lines_f32 = self.editor.line_offsets.len() as f32;
                        let bottom_blank_lines =
                            editor_bottom_blank_lines(editor_height, r.line_height);
                        let visible_minimap_lines = total_lines_f32.min(900.0);
                        let minimap_line_h = (editor_height
                            / (visible_minimap_lines + bottom_blank_lines).max(1.0))
                        .max(1.5);
                        let max_minimap_scroll = ((total_lines_f32 + bottom_blank_lines)
                            * minimap_line_h
                            - editor_height)
                            .max(0.0);

                        let scroll_ratio_y = (self.scroll_y.current / max_scroll).clamp(0.0, 1.0);
                        let current_minimap_scroll = scroll_ratio_y * max_minimap_scroll;

                        let visible_lines = editor_height / r.line_height;
                        let thumb_h = (visible_lines * minimap_line_h).max(4.0);
                        let viewport_y = tab_bar_h + scroll_ratio_y * (editor_height - thumb_h);

                        if my >= viewport_y && my <= viewport_y + thumb_h {
                            self.scroll_y.drag_offset = my - viewport_y;
                        } else {
                            let minimap_y = my - tab_bar_h;
                            let abs_minimap_y = minimap_y + current_minimap_scroll;
                            let target_line = abs_minimap_y / minimap_line_h;

                            let target_scroll = target_line * r.line_height - editor_height / 2.0;
                            let clamped_scroll = target_scroll.clamp(0.0, max_scroll).round();

                            self.scroll_y.target = clamped_scroll;
                            self.scroll_y.anim_speed = 15.0;

                            let target_ratio = clamped_scroll / max_scroll;
                            let thumb_visual_y = target_ratio * (editor_height - thumb_h);
                            self.scroll_y.drag_offset = my - tab_bar_h - thumb_visual_y;
                        }
                    }
                }
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::EditorScrollbarX => {
                self.scroll_x.is_dragging = true;
                let database_query_tab = self.active_tab_is_database_query();
                let metrics = self.editor_interaction_metrics();
                if let (Some((wh, s, panel_bottom_h, query_results_h)), Some(r)) =
                    (metrics, self.renderer.as_mut())
                {
                    let mx = r.last_mouse_x;
                    let tab_bar_h = crate::render_view::editor_content_top_inset(
                        self.show_welcome,
                        self.is_ide_mode,
                        database_query_tab,
                        s,
                    );
                    let editor_height = editor_interaction_view_height(
                        wh,
                        tab_bar_h,
                        panel_bottom_h,
                        query_results_h,
                        self.is_ide_mode,
                        s,
                    );
                    let max_y = r.get_max_scroll(&self.editor, editor_height);
                    let scrollbar_w = if max_y > 0.0 { 10.0 * s } else { 0.0 };
                    let track_x = r.left_padding;
                    let track_w = r.width - r.minimap_width - scrollbar_w - track_x;
                    if let Some((drag_offset, target)) = scrollbar_x_click_target(
                        mx,
                        track_x,
                        track_w,
                        self.scroll_x.current,
                        r.max_scroll_x,
                        s,
                    ) {
                        self.scroll_x.drag_offset = drag_offset;
                        self.scroll_x.target = target.round();
                        self.scroll_x.current = self.scroll_x.target;
                    }
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::MarkdownReadBody => {
                self.is_dragging = false;
                self.is_editor_drag_pending = false;
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::EditorTextBody => {
                let database_query_tab = self.active_tab_is_database_query();
                if let Some(r) = self.renderer.as_mut() {
                    let tab_bar_h = crate::render_view::editor_content_top_inset(
                        self.show_welcome,
                        self.is_ide_mode,
                        database_query_tab,
                        r.scale_factor,
                    );
                    let content_y = r.last_mouse_y - tab_bar_h + self.scroll_y.current;
                    if !content_y_hits_visual_text_row(content_y, r.line_height, &r.visual_lines) {
                        self.is_dragging = false;
                        self.window.as_ref().unwrap().request_redraw();
                        return;
                    }

                    r.suppress_popups_until_next_mouse_move();
                }
                self.is_dragging = false;
                self.is_editor_drag_pending = true;
                self.ide_panel.file_tree_focused = false;
                crate::app::mouse::clear_hover_popup(self.renderer.as_mut());
                self.scroll_y.anim_speed = 15.0;
                self.scroll_y.stop_anim();
                self.ide_panel.lsp_logs_focused = None;
                self.search_focused = false;
                self.ide_panel.term_search_focused = false;
                self.ide_panel.git.message_focused = false;
                self.ide_panel.git.close_commit_menus();
                self.settings_ignore_focused = false;

                if let Some(r) = self.renderer.as_mut() {
                    let mx = r.last_mouse_x;
                    let my = r.last_mouse_y;
                    let now = std::time::Instant::now();
                    let dx = mx - self.last_click_pos.0;
                    let dy = my - self.last_click_pos.1;
                    if repeated_ui_click(
                        same_click_target,
                        now.duration_since(self.last_click_time),
                        dx,
                        dy,
                    ) {
                        self.click_count += 1;
                    } else {
                        self.click_count = 1;
                    }

                    self.last_click_time = now;
                    self.last_click_pos = (mx, my);

                    let tab_bar_h = crate::render_view::editor_content_top_inset(
                        self.show_welcome,
                        self.is_ide_mode,
                        database_query_tab,
                        r.scale_factor,
                    );
                    self.editor.set_cursor_at_pos(
                        mx,
                        my - tab_bar_h + self.scroll_y.current,
                        r,
                        true,
                    );
                }

                if self.click_count == 2 {
                    self.editor.select_word();
                } else if self.click_count >= 3 {
                    self.editor.select_line();
                    self.click_count = 3;
                }
                self.window.as_ref().unwrap().request_redraw();
            }

            // Panels
            UiId::BottomPanelBody => {
                // Поглощаем клик — непрозрачная панель блокирует взаимодействие с редактором под ней
            }
            UiId::ResizeLeft => {
                // Блокируем resize, когда терминал в фокусе
                if !self.ide_panel.terminal_focused {
                    self.ide_panel.is_resizing_left = true;
                    self.window.as_ref().unwrap().request_redraw();
                }
            }
            UiId::ResizeBottom => {
                // Блокируем resize, когда терминал в фокусе
                if !self.ide_panel.terminal_focused {
                    self.ide_panel.is_resizing_bottom = true;
                    self.window.as_ref().unwrap().request_redraw();
                }
            }
            UiId::LspScrollY => {
                let started = self
                    .ui_registry
                    .rect_for(UiId::LspScrollY)
                    .and_then(|rect| {
                        let renderer = self.renderer.as_ref()?;
                        let s = renderer.scale_factor;
                        let track_start = rect.1 + 5.0 * s;
                        let track_len = (rect.3 - 10.0 * s).max(0.0);
                        let content_len = self.lsp_panel_total_h(s);
                        let max_scroll = (content_len - rect.3).max(0.0);
                        let thumb = crate::scroll::scrollbar_thumb(
                            track_start,
                            track_len,
                            rect.3,
                            content_len,
                            self.ide_panel.lsp_scroll_y.current,
                            40.0 * s,
                        )?;
                        let (drag_offset, target) = crate::scroll::scrollbar_drag_target(
                            renderer.last_mouse_y,
                            track_start,
                            track_len,
                            thumb,
                            max_scroll,
                            None,
                        )?;
                        Some((drag_offset, target))
                    });
                if let Some((drag_offset, target)) = started {
                    let scroll = &mut self.ide_panel.lsp_scroll_y;
                    scroll.current = target;
                    scroll.target = target;
                    scroll.velocity = 0.0;
                    scroll.drag_offset = drag_offset;
                    scroll.is_dragging = true;
                } else {
                    self.ide_panel.lsp_scroll_y.is_dragging = true;
                }
            }
            UiId::LspScrollX => {
                self.ide_panel.lsp_scroll_x.is_dragging = true;
            }
            UiId::LspLogScrollY(server_idx) => {
                if server_idx < self.ide_panel.lsp_servers.len() {
                    let name = self.ide_panel.lsp_servers[server_idx].name.to_string();
                    let geometry = self
                        .ui_registry
                        .rect_for(UiId::LspLogScrollY(server_idx))
                        .and_then(|rect| {
                            let renderer = self.renderer.as_ref()?;
                            let s = renderer.scale_factor;
                            let (content_len, _) = self
                                .lsp_server_inner_size(&self.ide_panel.lsp_servers[server_idx], s);
                            Some((
                                renderer.last_mouse_y,
                                rect.1 + 7.0 * s,
                                (rect.3 - 14.0 * s).max(0.0),
                                rect.3,
                                content_len,
                                20.0 * s,
                            ))
                        });
                    let scroll = self
                        .ide_panel
                        .lsp_logs_scroll_y
                        .entry(name)
                        .or_insert_with(|| crate::scroll::ScrollState::new(15.0));
                    if let Some((pointer, track_start, track_len, viewport_len, content_len, min)) =
                        geometry
                    {
                        if let Some((drag_offset, target)) =
                            crate::app::lsp_actions::lsp_log_scrollbar_drag_target(
                                pointer,
                                track_start,
                                track_len,
                                viewport_len,
                                content_len,
                                scroll.current,
                                min / 20.0,
                                None,
                            )
                        {
                            scroll.current = target;
                            scroll.target = target;
                            scroll.velocity = 0.0;
                            scroll.drag_offset = drag_offset;
                        }
                    }
                    scroll.is_dragging = true;
                }
            }
            UiId::LspLogScrollX(server_idx) => {
                if server_idx < self.ide_panel.lsp_servers.len() {
                    let name = self.ide_panel.lsp_servers[server_idx].name.to_string();
                    let geometry = self
                        .ui_registry
                        .rect_for(UiId::LspLogScrollX(server_idx))
                        .and_then(|rect| {
                            let renderer = self.renderer.as_ref()?;
                            let s = renderer.scale_factor;
                            let (_, max_line_w) = self
                                .lsp_server_inner_size(&self.ide_panel.lsp_servers[server_idx], s);
                            Some((
                                renderer.last_mouse_x,
                                rect.0 + 7.0 * s,
                                (rect.2 - 14.0 * s).max(0.0),
                                rect.2,
                                max_line_w + 20.0 * s,
                                20.0 * s,
                            ))
                        });
                    let scroll = self
                        .ide_panel
                        .lsp_logs_scroll_x
                        .entry(name)
                        .or_insert_with(|| crate::scroll::ScrollState::new(15.0));
                    if let Some((pointer, track_start, track_len, viewport_len, content_len, min)) =
                        geometry
                    {
                        if let Some((drag_offset, target)) =
                            crate::app::lsp_actions::lsp_log_scrollbar_drag_target(
                                pointer,
                                track_start,
                                track_len,
                                viewport_len,
                                content_len,
                                scroll.current,
                                min / 20.0,
                                None,
                            )
                        {
                            scroll.current = target;
                            scroll.target = target;
                            scroll.velocity = 0.0;
                            scroll.drag_offset = drag_offset;
                        }
                    }
                    scroll.is_dragging = true;
                }
            }
            UiId::LspLogsFilterInput(server_idx) => {
                self.ide_panel.lsp_log_filter_focused = true;
                self.ide_panel.lsp_logs_focused = None;
                if let Some(rect) = self
                    .ui_registry
                    .rect_for(UiId::LspLogsFilterInput(server_idx))
                {
                    if let Some(r) = self.renderer.as_mut() {
                        let mx = r.last_mouse_x;
                        let s = r.scale_factor;
                        let text = self.ide_panel.lsp_log_filter_editor.get_full_text();
                        let scroll_x = r.one_line_scroll_for_cursor(
                            &text,
                            self.ide_panel.lsp_log_filter_editor.cursor,
                            0.78,
                            rect.2 - 16.0 * s,
                            0.0,
                        );
                        let x_offset = (mx - (rect.0 + 8.0 * s) + scroll_x).max(0.0);
                        let target_idx = r.one_line_cursor_from_x(&text, x_offset, 0.78);
                        self.ide_panel.lsp_log_filter_editor.cursor = target_idx;
                        self.ide_panel.lsp_log_filter_editor.selection_anchor = Some(target_idx);
                    }
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::LspLogsFilterClear(_) => {
                let old_version = self.ide_panel.lsp_log_filter_editor.version;
                self.ide_panel.lsp_log_filter_editor = Editor::new(256);
                self.ide_panel.lsp_log_filter_editor.version = old_version + 1;
                self.ide_panel.lsp_log_filter_dirty = true;
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::LspLogsFilterCase(_) => {
                self.ide_panel.lsp_log_filter_case_sensitive =
                    !self.ide_panel.lsp_log_filter_case_sensitive;
                self.ide_panel.lsp_log_filter_dirty = true;
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::LspLogsFilterSend(_) => {
                self.ide_panel.lsp_log_filter_show_send = !self.ide_panel.lsp_log_filter_show_send;
                self.ide_panel.lsp_log_filter_dirty = true;
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::LspLogsFilterRecv(_) => {
                self.ide_panel.lsp_log_filter_show_recv = !self.ide_panel.lsp_log_filter_show_recv;
                self.ide_panel.lsp_log_filter_dirty = true;
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::LspLogsFilterOther(_) => {
                self.ide_panel.lsp_log_filter_show_other =
                    !self.ide_panel.lsp_log_filter_show_other;
                self.ide_panel.lsp_log_filter_dirty = true;
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::ProblemFileToggle(idx) => {
                if let Some((path, diag_idx)) = self.ide_panel.flat_diags.get(idx) {
                    if *diag_idx == usize::MAX {
                        if self.ide_panel.problems_collapsed.contains(path) {
                            self.ide_panel.problems_collapsed.remove(path);
                        } else {
                            self.ide_panel.problems_collapsed.insert(path.clone());
                        }
                        if let Some(window) = self.window.as_ref() {
                            window.request_redraw();
                        }
                    }
                }
            }
            UiId::ProblemsTab(idx) => {
                self.ide_panel.problems_tab = idx;
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::PopupOpenDiagUrl(_idx) | UiId::OpenDiagUrl(_idx) => {
                if let Some(href) =
                    crate::app::mouse::HOVER_STATE.with(|s| s.borrow().diag_href.clone())
                {
                    let _ = crate::platform::open_url(&href);
                }
            }
            UiId::ProblemUrl(idx) => {
                if let Some((path, diag_idx)) = self.ide_panel.flat_diags.get(idx)
                    && let Some(diag) =
                        self.ide_panel
                            .problem_diagnostic(self.lsp.as_ref(), path, *diag_idx)
                    && let Some(href) = &diag.code_href
                {
                    let _ = crate::platform::open_url(href.as_ref());
                }
            }
            UiId::ProblemJump(idx) => {
                if let Some((path, diag_idx)) = self.ide_panel.flat_diags.get(idx).cloned() {
                    if diag_idx == usize::MAX {
                        return;
                    }
                    if self.ide_panel.is_query_problem_path(&path) {
                        self.jump_to_active_database_query_diagnostic(diag_idx);
                    } else {
                        let diagnostic = self
                            .ide_panel
                            .problem_diagnostic(self.lsp.as_ref(), &path, diag_idx)
                            .cloned();
                        if let Some(diagnostic) = diagnostic {
                            self.jump_to_lsp_position_in_file(
                                path,
                                diagnostic.end_line,
                                diagnostic.end_col,
                                true,
                                0.45,
                            );
                        }
                    }
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                }
            }
            UiId::CopyDiagnostic(idx) => {
                let message = self
                    .ide_panel
                    .flat_diags
                    .get(idx)
                    .and_then(|(path, diag_idx)| {
                        self.ide_panel
                            .problem_diagnostic(self.lsp.as_ref(), path, *diag_idx)
                    })
                    .map(|diagnostic| diagnostic.message.to_string());
                if let Some(message) = message {
                    self.set_clipboard_text(message);
                    self.ide_panel.diag_copied_idx = Some(idx);
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::PopupCopyDiagnostic(idx) => {
                let mut message = crate::app::mouse::HOVER_STATE.with(|state| {
                    state
                        .borrow()
                        .diag_copy_texts
                        .get(idx)
                        .filter(|text| !text.is_empty())
                        .cloned()
                });
                if let Some(path) = &self.file_path {
                    if message.is_none() {
                        message = self
                            .lsp
                            .as_ref()
                            .and_then(|l| l.diagnostic_at(path, idx))
                            .map(|diag| diag.message.to_string());
                    }
                }
                if message.is_none() {
                    message = crate::app::mouse::HOVER_STATE.with(|state| {
                        let state = state.borrow();
                        (!state.diag_text.is_empty()).then(|| state.diag_text.clone())
                    });
                }
                if let Some(message) = message {
                    self.set_clipboard_text(message);
                    self.ide_panel.diag_copied_idx = Some(idx);
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::LspLogArea(server_idx) => {
                self.ide_panel.lsp_log_filter_focused = false;
                if server_idx < self.ide_panel.lsp_servers.len() {
                    self.ide_panel.lsp_logs_focused =
                        Some(self.ide_panel.lsp_servers[server_idx].name.to_string());
                }
                if let Some(focused) = &self.ide_panel.lsp_logs_focused {
                    if let Some(ed) = self.ide_panel.lsp_log_editors.get_mut(focused) {
                        ed.selection_anchor = None;
                    }
                }
                self.is_dragging_lsp_log = true;
                if let Some(r) = self.renderer.as_ref() {
                    let mx = r.last_mouse_x;
                    let my = r.last_mouse_y;
                    let now = std::time::Instant::now();
                    let dx = mx - self.last_click_pos.0;
                    let dy = my - self.last_click_pos.1;
                    if repeated_ui_click(
                        same_click_target,
                        now.duration_since(self.last_click_time),
                        dx,
                        dy,
                    ) {
                        self.click_count += 1;
                    } else {
                        self.click_count = 1;
                    }
                    self.last_click_time = now;
                    self.last_click_pos = (mx, my);
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
        }
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl App {
    fn handle_database_ui_click(&mut self, id: UiId, same_click_target: bool) {
        use crate::app::database::{DatabaseConnectionColor, PostgresTlsMode, SshHostKeyPolicy};

        if database_table_click_closes_cell_popup(id)
            && let Some(tab_id) = self.active_database_table_tab_id()
            && let Some((_, state)) = self.database_table_meta_state_mut(tab_id)
            && state.grid.cell_editor.as_ref().is_some_and(|editor| {
                matches!(
                    editor.kind,
                    crate::app::database::DatabaseCellEditorKind::Enum
                        | crate::app::database::DatabaseCellEditorKind::DateTime
                )
            })
        {
            state.grid.cell_editor = None;
            if state.grid.focused_input
                == Some(crate::app::database::DatabaseTableInputTarget::Cell)
            {
                state.grid.focused_input = None;
            }
        }

        match id {
            UiId::DatabasePanelBody
            | UiId::DatabaseDialogBody
            | UiId::DatabaseDdlBody
            | UiId::DatabaseDdlScroll
            | UiId::DatabaseTableGridBody
            | UiId::DatabaseTableModalBody => {}
            UiId::DatabaseGlobalErrorCopy => {
                if let Some(error) = self.ide_panel.database.global_error.clone() {
                    self.set_clipboard_text(error);
                }
            }
            UiId::DatabaseTableBody => {
                if let Some(tab_id) = self.active_database_table_tab_id()
                    && let Some((_, state)) = self.database_table_meta_state_mut(tab_id)
                {
                    state.clear_unavailable_selection();
                }
            }
            UiId::DatabaseTableUnavailableText => {
                let mouse_x = self
                    .renderer
                    .as_ref()
                    .map_or(0.0, |renderer| renderer.last_mouse_x);
                let input_index = self.database_table_unavailable_text_index_at(mouse_x);
                if let Some(tab_id) = self.active_database_table_tab_id()
                    && let Some((_, state)) = self.database_table_meta_state_mut(tab_id)
                {
                    state.unavailable_text_focused = true;
                    state.unavailable_text_dragging = input_index.is_some();
                }
                if let Some(input_index) = input_index {
                    self.set_database_table_unavailable_text_cursor(input_index, false);
                }
            }
            UiId::DatabaseAdd => self.open_database_connection_dialog(),
            UiId::DatabaseDelete => {
                if let Some(id) = self.ide_panel.database.selected_connection {
                    self.request_delete_database_connection(id);
                }
            }
            UiId::DatabaseRefresh => self.refresh_selected_database(),
            UiId::DatabaseConnectionRow(index) => {
                if let Some(id) = self
                    .ide_panel
                    .database
                    .connections
                    .get(index)
                    .map(|node| node.config.id)
                {
                    self.select_database_connection(id);
                }
            }
            UiId::DatabaseConnectionArrow(index) => {
                if let Some(id) = self
                    .ide_panel
                    .database
                    .connections
                    .get(index)
                    .map(|node| node.config.id)
                {
                    self.toggle_database_connection(id);
                }
            }
            UiId::DatabaseRow(connection_index, database_index) => {
                let selected = self
                    .ide_panel
                    .database
                    .connections
                    .get(connection_index)
                    .and_then(|node| {
                        node.databases
                            .get(database_index)
                            .map(|database| (node.config.id, database.name.clone()))
                    });
                if let Some((id, database_name)) = selected {
                    self.ide_panel.database.selected_connection = Some(id);
                    self.ide_panel.database.selected_database = Some((id, database_name));
                }
            }
            UiId::DatabaseArrow(connection_index, database_index) => {
                if let Some(id) = self
                    .ide_panel
                    .database
                    .connections
                    .get(connection_index)
                    .map(|node| node.config.id)
                {
                    self.toggle_database_node(id, database_index);
                }
            }
            UiId::DatabaseTableRow(connection_index, database_index, table_index) => {
                let target = self
                    .ide_panel
                    .database
                    .connections
                    .get(connection_index)
                    .and_then(|node| {
                        node.databases.get(database_index).and_then(|database| {
                            database.tables.get(table_index).map(|table| {
                                (node.config.id, database.name.clone(), table.name.clone())
                            })
                        })
                    });
                if let Some((id, database, table)) = target {
                    let now = std::time::Instant::now();
                    let table_key = (id, database.clone(), table.clone());
                    let double_click = self
                        .ide_panel
                        .database
                        .last_table_click
                        .as_ref()
                        .is_some_and(|(previous, at)| {
                            previous == &table_key
                                && now.saturating_duration_since(*at)
                                    <= std::time::Duration::from_millis(500)
                        });
                    self.ide_panel.database.selected_connection = Some(id);
                    self.ide_panel.database.selected_database = Some((id, database.clone()));
                    self.ide_panel.database.selected_table = Some(table_key.clone());
                    self.ide_panel.database.last_table_click = Some((table_key, now));
                    self.ide_panel.database.notice =
                        Some(format!("Выбрана таблица public.{table}"));
                    if double_click {
                        self.open_database_table_tab(id, &database, &table);
                    }
                }
            }
            UiId::DatabaseContextItem(index) => self.activate_database_context_action(index),
            UiId::DatabaseDialogField(field) => {
                let mouse_x = self
                    .renderer
                    .as_ref()
                    .map_or(0.0, |renderer| renderer.last_mouse_x);
                let target = self.database_dialog_input_index_at(field, mouse_x);
                if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
                    dialog.focused = Some(field);
                    dialog.dragging_field = Some(field);
                    dialog.error = None;
                    dialog.test_status = None;
                }
                self.last_action = std::time::Instant::now();
                self.last_blink_state = true;
                if let Some(target) = target {
                    self.set_database_dialog_input_cursor(field, target, false);
                }
            }
            UiId::DatabaseDialogSecretEye(field) => {
                if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
                    dialog.toggle_secret_visibility(field);
                    dialog.focused = Some(field);
                    dialog.error = None;
                    dialog.test_status = None;
                }
                self.last_action = std::time::Instant::now();
                self.last_blink_state = true;
            }
            UiId::DatabaseDialogTls => {
                if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
                    dialog.tls_mode = match dialog.tls_mode {
                        PostgresTlsMode::Disable => PostgresTlsMode::Prefer,
                        PostgresTlsMode::Prefer => PostgresTlsMode::Require,
                        PostgresTlsMode::Require => PostgresTlsMode::Disable,
                    };
                    dialog.error = None;
                    dialog.test_status = None;
                }
            }
            UiId::DatabaseDialogColor => {
                if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
                    dialog.color = match dialog.color {
                        DatabaseConnectionColor::Blue => DatabaseConnectionColor::Green,
                        DatabaseConnectionColor::Green => DatabaseConnectionColor::Yellow,
                        DatabaseConnectionColor::Yellow => DatabaseConnectionColor::Orange,
                        DatabaseConnectionColor::Orange => DatabaseConnectionColor::Red,
                        DatabaseConnectionColor::Red => DatabaseConnectionColor::Purple,
                        DatabaseConnectionColor::Purple => DatabaseConnectionColor::Cyan,
                        DatabaseConnectionColor::Cyan => DatabaseConnectionColor::Gray,
                        DatabaseConnectionColor::Gray => DatabaseConnectionColor::Blue,
                    };
                    dialog.error = None;
                    dialog.test_status = None;
                }
            }
            UiId::DatabaseDialogSshToggle => {
                if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
                    dialog.ssh_enabled = !dialog.ssh_enabled;
                    if !dialog.ssh_enabled {
                        dialog.jump_enabled = false;
                        if dialog
                            .focused
                            .is_some_and(|field| dialog.visible_field_index(field).is_none())
                        {
                            dialog.focused =
                                Some(crate::app::database::DatabaseFormField::MaintenanceDatabase);
                        }
                    }
                    dialog.error = None;
                    dialog.test_status = None;
                }
                self.clamp_database_dialog_scroll_to_layout();
            }
            UiId::DatabaseDialogJumpToggle => {
                if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
                    dialog.toggle_jump_host();
                }
                self.clamp_database_dialog_scroll_to_layout();
                self.ensure_database_dialog_focus_visible();
            }
            UiId::DatabaseDialogRememberPostgres => {
                if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
                    dialog.remember_postgres_password = !dialog.remember_postgres_password;
                }
            }
            UiId::DatabaseDialogRememberSshPassword => {
                if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
                    dialog.remember_ssh_password = !dialog.remember_ssh_password;
                }
            }
            UiId::DatabaseDialogRememberSshPassphrase => {
                if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
                    dialog.remember_ssh_key_passphrase = !dialog.remember_ssh_key_passphrase;
                }
            }
            UiId::DatabaseDialogRememberJumpPassword => {
                if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
                    dialog.remember_jump_password = !dialog.remember_jump_password;
                }
            }
            UiId::DatabaseDialogRememberJumpPassphrase => {
                if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
                    dialog.remember_jump_key_passphrase = !dialog.remember_jump_key_passphrase;
                }
            }
            UiId::DatabaseDialogTest => self.test_database_dialog_connection(),
            UiId::DatabaseDialogSave => self.save_database_connection_dialog(),
            UiId::DatabaseDialogCancel => self.cancel_database_dialog(),
            UiId::DatabaseDialogBackdrop => {
                if self.ide_panel.database.dialog.is_some() {
                    self.cancel_database_dialog();
                } else if self.ide_panel.database.delete_prompt.is_some() {
                    self.cancel_delete_database_connection();
                } else if self.ide_panel.database.host_key_prompt.is_some() {
                    self.cancel_database_host_key_prompt();
                }
            }
            UiId::DatabaseDeleteConfirm => self.confirm_delete_database_connection(),
            UiId::DatabaseDeleteCancel => self.cancel_delete_database_connection(),
            UiId::DatabaseHostKeyTrustOnce => {
                self.resolve_database_host_key(SshHostKeyPolicy::TrustOnce)
            }
            UiId::DatabaseHostKeyTrustStore => {
                self.resolve_database_host_key(SshHostKeyPolicy::TrustAndStore)
            }
            UiId::DatabaseHostKeyCancel => self.cancel_database_host_key_prompt(),
            UiId::DatabaseTableAddRow => {
                if let Some(tab) = self.active_database_table_tab_id() {
                    self.add_database_table_row(tab);
                }
            }
            UiId::DatabaseTableDeleteRows => {
                if let Some(tab) = self.active_database_table_tab_id() {
                    self.delete_database_table_selection(tab);
                }
            }
            UiId::DatabaseTableUndo => {
                if let Some(tab) = self.active_database_table_tab_id() {
                    self.undo_database_table_selection(tab);
                }
            }
            UiId::DatabaseTableSave => {
                if let Some(tab) = self.active_database_table_tab_id() {
                    self.save_database_table_changes(tab, false);
                }
            }
            UiId::DatabaseTablePreview => {
                if let Some(tab) = self.active_database_table_tab_id() {
                    self.preview_database_table_changes(tab);
                }
            }
            UiId::DatabaseTableRefresh => {
                if let Some(tab) = self.active_database_table_tab_id() {
                    self.request_database_table_refresh(tab);
                }
            }
            UiId::DatabaseTablePageFirst => {
                if let Some(tab) = self.active_database_table_tab_id() {
                    self.database_table_page_first(tab);
                }
            }
            UiId::DatabaseTablePagePrevious => {
                if let Some(tab) = self.active_database_table_tab_id() {
                    self.database_table_page_previous(tab);
                }
            }
            UiId::DatabaseTablePageNext => {
                if let Some(tab) = self.active_database_table_tab_id() {
                    self.database_table_page_next(tab);
                }
            }
            UiId::DatabaseTablePageLast => {
                if let Some(tab) = self.active_database_table_tab_id() {
                    self.database_table_page_last(tab);
                }
            }
            UiId::DatabaseTableLimit => {
                if let Some(tab) = self.active_database_table_tab_id() {
                    self.open_database_table_limit_dialog(tab);
                }
            }
            UiId::DatabaseTableModalInput => {
                let mouse = self.renderer.as_ref().map_or((0.0, 0.0), |renderer| {
                    (renderer.last_mouse_x, renderer.last_mouse_y)
                });
                let input_index = self.database_table_modal_input_index_at(mouse.0, mouse.1);
                self.ide_panel.database.table_modal_input_dragging = input_index.is_some();
                self.last_action = std::time::Instant::now();
                self.last_blink_state = true;
                if let Some(input_index) = input_index {
                    self.set_database_table_modal_input_cursor(input_index, false);
                }
            }
            UiId::DatabaseTableWhereInput => {
                let target = crate::app::database::DatabaseTableInputTarget::Where;
                let mouse_x = self
                    .renderer
                    .as_ref()
                    .map_or(0.0, |renderer| renderer.last_mouse_x);
                let input_index = self.database_table_input_index_at(target, mouse_x);
                if let Some(tab) = self.active_database_table_tab_id()
                    && let Some((_, state)) = self.database_table_meta_state_mut(tab)
                {
                    state.grid.focused_input = Some(target);
                    state.grid.text_drag = Some(target);
                    state.grid.cell_editor = None;
                }
                self.last_action = std::time::Instant::now();
                self.last_blink_state = true;
                if let Some(input_index) = input_index {
                    self.set_database_table_input_cursor(target, input_index, false);
                }
                self.close_autocomplete();
            }
            UiId::DatabaseTableOrderInput => {
                let target = crate::app::database::DatabaseTableInputTarget::OrderBy;
                let mouse_x = self
                    .renderer
                    .as_ref()
                    .map_or(0.0, |renderer| renderer.last_mouse_x);
                let input_index = self.database_table_input_index_at(target, mouse_x);
                if let Some(tab) = self.active_database_table_tab_id()
                    && let Some((_, state)) = self.database_table_meta_state_mut(tab)
                {
                    state.grid.focused_input = Some(target);
                    state.grid.text_drag = Some(target);
                    state.grid.cell_editor = None;
                }
                self.last_action = std::time::Instant::now();
                self.last_blink_state = true;
                if let Some(input_index) = input_index {
                    self.set_database_table_input_cursor(target, input_index, false);
                }
                self.close_autocomplete();
            }
            UiId::DatabaseTableCellEditor => {
                let target = crate::app::database::DatabaseTableInputTarget::Cell;
                let mouse_x = self
                    .renderer
                    .as_ref()
                    .map_or(0.0, |renderer| renderer.last_mouse_x);
                let input_index = self.database_table_input_index_at(target, mouse_x);
                if let Some(tab) = self.active_database_table_tab_id()
                    && let Some((_, state)) = self.database_table_meta_state_mut(tab)
                {
                    state.grid.focused_input = Some(target);
                    state.grid.text_drag = Some(target);
                }
                self.last_action = std::time::Instant::now();
                self.last_blink_state = true;
                if let Some(input_index) = input_index {
                    self.set_database_table_input_cursor(target, input_index, false);
                }
            }
            UiId::DatabaseTableHeader(column) => {
                if let Some(tab) = self.active_database_table_tab_id() {
                    self.cycle_database_table_sort(tab, column);
                }
            }
            UiId::DatabaseTableColumnResize(column) => {
                if let Some(tab) = self.active_database_table_tab_id() {
                    let now = std::time::Instant::now();
                    let mouse = self.renderer.as_ref().map_or((0.0, 0.0), |renderer| {
                        (renderer.last_mouse_x, renderer.last_mouse_y)
                    });
                    let double = repeated_ui_click(
                        same_click_target,
                        now.duration_since(self.last_click_time),
                        mouse.0 - self.last_click_pos.0,
                        mouse.1 - self.last_click_pos.1,
                    );
                    self.last_click_time = now;
                    self.last_click_pos = mouse;
                    if double {
                        self.auto_size_database_table_column(tab, column);
                    } else {
                        self.start_database_table_column_resize(tab, column, mouse.0);
                    }
                }
            }
            UiId::DatabaseGridRow(row) => {
                let extend = self.modifiers.shift_key();
                let toggle = self.modifiers.control_key() || self.modifiers.super_key();
                if let Some(tab) = self.active_database_table_tab_id()
                    && let Some((_, state)) = self.database_table_meta_state_mut(tab)
                {
                    state.grid.select_row(row, extend, toggle);
                    state.grid.focused_input = None;
                }
            }
            UiId::DatabaseTableCell(row, column) => {
                let now = std::time::Instant::now();
                let mouse = self.renderer.as_ref().map_or((0.0, 0.0), |renderer| {
                    (renderer.last_mouse_x, renderer.last_mouse_y)
                });
                let double = repeated_ui_click(
                    same_click_target,
                    now.duration_since(self.last_click_time),
                    mouse.0 - self.last_click_pos.0,
                    mouse.1 - self.last_click_pos.1,
                );
                self.last_click_time = now;
                self.last_click_pos = mouse;
                self.handle_database_table_cell_click(row, column, double);
            }
            UiId::DatabaseTableEnumOption(option) => self.select_database_table_enum_option(option),
            UiId::DatabaseTableEnumPreviousPage => self.page_database_table_enum_options(false),
            UiId::DatabaseTableEnumNextPage => self.page_database_table_enum_options(true),
            UiId::DatabaseTableDatePreviousMonth => self.shift_database_table_calendar_month(-1),
            UiId::DatabaseTableDateNextMonth => self.shift_database_table_calendar_month(1),
            UiId::DatabaseTableDateDay(day) => self.select_database_table_calendar_day(day as u32),
            UiId::DatabaseTableDateToday => self.set_database_table_date_today(),
            UiId::DatabaseTableDateNow => self.set_database_table_time_now_utc(),
            UiId::DatabaseTableScrollY => self.start_database_table_scroll_drag(false),
            UiId::DatabaseTableScrollX => self.start_database_table_scroll_drag(true),
            UiId::DatabaseTableModalScroll => self.start_database_sql_preview_scroll_drag(false),
            UiId::DatabaseTableModalScrollX => self.start_database_sql_preview_scroll_drag(true),
            UiId::DatabaseTableModalPrimary => self.activate_database_table_modal_action(0),
            UiId::DatabaseTableModalSecondary | UiId::DatabaseTableModalBackdrop => {
                self.activate_database_table_modal_action(1)
            }
            UiId::DatabaseTableModalTertiary => self.activate_database_table_modal_action(2),
            UiId::DatabaseQueryRun => {
                self.run_active_database_query(crate::app::database::DatabaseQueryMode::Run)
            }
            UiId::DatabaseQueryCancel => self.cancel_active_database_query(),
            UiId::DatabaseQueryExplain => {
                self.run_active_database_query(crate::app::database::DatabaseQueryMode::Explain)
            }
            UiId::DatabaseQueryExplainAnalyze => self
                .run_active_database_query(crate::app::database::DatabaseQueryMode::ExplainAnalyze),
            UiId::DatabaseQueryFormat => self.format_active_database_query(),
            UiId::DatabaseQueryHistory => self.toggle_active_database_query_history(),
            UiId::DatabaseQueryNextDiagnostic => {
                self.jump_to_next_active_database_query_diagnostic();
            }
            UiId::DatabaseQueryResultTab(index) => self.select_active_database_query_result(index),
            UiId::DatabaseQueryHistoryEntry(index) => self.load_database_query_history_entry(index),
            UiId::DatabaseQueryResultResize => self.start_database_query_result_resize(),
            UiId::DatabaseQueryColumnResize(column) => {
                let now = std::time::Instant::now();
                let mouse = self.renderer.as_ref().map_or((0.0, 0.0), |renderer| {
                    (renderer.last_mouse_x, renderer.last_mouse_y)
                });
                let double = repeated_ui_click(
                    same_click_target,
                    now.duration_since(self.last_click_time),
                    mouse.0 - self.last_click_pos.0,
                    mouse.1 - self.last_click_pos.1,
                );
                self.last_click_time = now;
                self.last_click_pos = mouse;
                if double {
                    self.auto_size_active_database_query_column(column);
                } else {
                    self.start_database_query_column_resize(column, mouse.0);
                }
            }
            UiId::DatabaseQueryScrollY => self.start_database_query_scroll_drag(false),
            UiId::DatabaseQueryScrollX => self.start_database_query_scroll_drag(true),
            UiId::DatabaseQueryReviewMessagesScrollY => {
                self.scroll_active_database_query_review_messages_to_pointer();
            }
            UiId::DatabaseQueryCommit => self.commit_active_database_query(),
            UiId::DatabaseQueryRollback | UiId::DatabaseQueryReviewBackdrop => {
                self.rollback_active_database_query();
            }
            UiId::DatabaseQueryResultBody
            | UiId::DatabaseQueryReviewBody
            | UiId::DatabaseQueryReviewMessagesBody => {}
            _ => {}
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub(crate) fn open_database_context_menu_for_hit(
        &mut self,
        id: UiId,
        mx: f32,
        my: f32,
    ) -> bool {
        use crate::app::database::DatabaseContextTarget;
        let target = match id {
            UiId::DatabaseConnectionRow(connection_index)
            | UiId::DatabaseConnectionArrow(connection_index) => self
                .ide_panel
                .database
                .connections
                .get(connection_index)
                .map(|node| DatabaseContextTarget::Connection(node.config.id)),
            UiId::DatabaseRow(connection_index, database_index)
            | UiId::DatabaseArrow(connection_index, database_index) => self
                .ide_panel
                .database
                .connections
                .get(connection_index)
                .map(|node| DatabaseContextTarget::Database(node.config.id, database_index)),
            UiId::DatabaseTableRow(connection_index, database_index, table_index) => self
                .ide_panel
                .database
                .connections
                .get(connection_index)
                .map(|node| {
                    DatabaseContextTarget::Table(node.config.id, database_index, table_index)
                }),
            _ => None,
        };
        if let Some(target) = target {
            self.open_database_context_menu(target, mx, my);
            true
        } else {
            false
        }
    }
}

fn database_table_click_closes_cell_popup(id: UiId) -> bool {
    !matches!(
        id,
        UiId::DatabaseTableCellEditor
            | UiId::DatabaseTableEnumOption(_)
            | UiId::DatabaseTableEnumPreviousPage
            | UiId::DatabaseTableEnumNextPage
            | UiId::DatabaseTableDatePreviousMonth
            | UiId::DatabaseTableDateNextMonth
            | UiId::DatabaseTableDateDay(_)
            | UiId::DatabaseTableDateToday
            | UiId::DatabaseTableDateNow
    )
}
