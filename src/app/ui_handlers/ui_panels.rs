use crate::app::App;
use crate::editor::Editor;
use crate::ui_system::UiId;
use super::UiClickFlow;

impl App {
    pub(super) fn handle_panels_ui_click(&mut self, id: UiId, same_click_target: bool) -> UiClickFlow {
        match id {
            UiId::MarkdownModeToggle => {
                self.toggle_markdown_mode();
            }
            UiId::PdfDarkToggle => self.toggle_pdf_dark_pages(),
            UiId::PdfScrollY => {}
            UiId::MarkdownCodeCopy(block_id) => {
                let _ = self.copy_markdown_read_code_block(block_id);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
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
                let mouse = self
                    .renderer
                    .as_ref()
                    .map(|renderer| (renderer.last_mouse_x, renderer.last_mouse_y));
                let cell = mouse.and_then(|(mx, my)| self.terminal_selection_cell(mx, my));
                if let Some(term) = self.ide_panel.terminals.get_mut(active) {
                    let mut grid = crate::app::terminal::lock_terminal_grid(&term.grid);
                    grid.selection = cell.map(|(x, y)| (x, y, x, y));
                }
            }
            // Terminal scrollbar drag state is initialized in mouse/input.rs, where
            // the inverted 0=bottom geometry is available.
            UiId::TerminalScrollY => {}
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
            UiId::MarkdownReadBody
            | UiId::MarkdownReadScrollbar
            | UiId::MarkdownCodeScrollbarX(_) => {
                self.is_dragging = false;
                self.is_editor_drag_pending = false;
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            _ => return UiClickFlow::NotMine,
        }
        UiClickFlow::Handled
    }
}
