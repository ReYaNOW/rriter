/// Обработчики UI событий - централизованная логика для кликов по кнопкам
/// Устраняет дублирование кода между input.rs и ui.rs

use crate::app::App;
use crate::ui_system::UiId;
use crate::editor::Editor;

impl App {
    /// Обрабатывает клик по UI элементу
    pub fn handle_ui_click(&mut self, id: UiId) {
        match id {
            // Welcome screen
            UiId::WelcomeNewFile => {
                self.show_welcome = false;
                self.is_ide_mode = false;
                self.file_path = None;
                self.base_title = "Безымянный".to_string();
                let old_version = self.editor.version;
                self.editor = Editor::new(8192);
                self.editor.version = old_version + 1;
                self.editor.set_original_text();
                self.editor.sync_edits.clear();
                self.highlighter.reset(self.editor.version, "".to_string(), "".to_string());
                App::update_window_title(self.window.as_ref().unwrap(), &self.base_title, false);
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::WelcomeOpenFile => {
                self.is_ide_mode = false;
                self.trigger_file_picker();
            }
            UiId::WelcomeIdeMode => {
                self.show_welcome = false;
                self.is_ide_mode = true;
                self.file_path = None;
                self.base_title = "Режим IDE".to_string();
                let old_version = self.editor.version;
                self.editor = Editor::new(8192);
                self.editor.version = old_version + 1;
                self.editor.set_original_text();
                self.editor.sync_edits.clear();
                self.highlighter.reset(self.editor.version, "".to_string(), "".to_string());

                if !self.ide_workspaces.is_empty() {
                    self.ide_panel.toggle(crate::app::PanelId::Explorer);
                    self.refresh_file_tree();
                    self.start_file_watcher();
                    if let Some(first_ws) = self.ide_workspaces.first().cloned() {
                        if self.lsp.is_none() {
                            self.lsp = Some(crate::lsp::LspManager::new(Some(first_ws)));
                        }
                    }
                } else {
                    self.trigger_folder_picker();
                }

                App::update_window_title(self.window.as_ref().unwrap(), &self.base_title, false);
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::WelcomeRecentFile(idx) => {
                if idx < self.recent_files.len() {
                    let path = self.recent_files[idx].clone();
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        self.show_welcome = false;
                        self.file_path = Some(path.clone());
                        self.base_title = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                        let ext = path.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
                        self.file_extension = ext.clone();

                        let old_version = self.editor.version;
                        self.editor = Editor::new(content.len() + 8192);
                        let _ = self.editor.insert_str(&content);
                        self.editor.cursor = 0;
                        self.editor.version = old_version + 1;
                        self.editor.set_original_text();
                        self.editor.sync_edits.clear();
                        self.highlighter.reset(self.editor.version, content, ext);

                        App::update_window_title(self.window.as_ref().unwrap(), &self.base_title, false);
                        self.window.as_ref().unwrap().request_redraw();
                    }
                }
            }

            // Dialog
            UiId::DialogSave => {
                if let Some(path) = self.file_path.clone() {
                    let _ = std::fs::write(&path, self.editor.get_full_text());
                    self.editor.set_original_text();
                } else {
                    self.trigger_save_as_picker();
                }
                self.dialog_window = None;
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::DialogDiscard => {
                self.dialog_window = None;
                match self.pending_action {
                    crate::app::PendingAction::Quit => {
                        if let Some(w) = &self.window {
                            w.set_visible(false);
                        }
                    }
                    crate::app::PendingAction::OpenFile => {
                        self.trigger_file_picker();
                    }
                    crate::app::PendingAction::CloseFile => {
                        self.show_welcome = true;
                        self.base_title = "Добро пожаловать".to_string();
                        App::update_window_title(self.window.as_ref().unwrap(), &self.base_title, false);
                    }
                }
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::DialogCancel => {
                self.dialog_window = None;
                self.window.as_ref().unwrap().request_redraw();
            }

            // Settings tabs
            UiId::SettingsTab(idx) => {
                self.settings_tab = idx;
                self.window.as_ref().unwrap().request_redraw();
            }

            // Settings IDE
            UiId::SettingsIdeAddWorkspace => {
                self.trigger_folder_picker();
            }
            UiId::SettingsIdeRemoveWorkspace(idx) => {
                if idx < self.ide_workspaces.len() {
                    self.ide_workspaces.remove(idx);
                    let config = crate::Config {
                        window_width: self.window_width,
                        window_height: self.window_height,
                        maximized: self.window.as_ref().map(|w| w.is_maximized()).unwrap_or(false),
                        ide_workspaces: self.ide_workspaces.clone(),
                        ide_ignore_patterns: self.ide_ignore_patterns.clone(),
                    };
                    crate::save_config(&config);
                    self.refresh_file_tree();
                    self.window.as_ref().unwrap().request_redraw();
                }
            }
            UiId::SettingsIdeAddIgnore => {
                let pattern = self.settings_ignore_editor.get_full_text().trim().to_string();
                if !pattern.is_empty() && !self.ide_ignore_patterns.contains(&pattern) {
                    self.ide_ignore_patterns.push(pattern);
                    // Очищаем редактор
                    let old_version = self.settings_ignore_editor.version;
                    self.settings_ignore_editor = Editor::new(128);
                    self.settings_ignore_editor.version = old_version + 1;
                    self.settings_ignore_editor.cursor = 0;
                    self.settings_ignore_editor.selection_anchor = None;
                    let config = crate::Config {
                        window_width: self.window_width,
                        window_height: self.window_height,
                        maximized: self.window.as_ref().map(|w| w.is_maximized()).unwrap_or(false),
                        ide_workspaces: self.ide_workspaces.clone(),
                        ide_ignore_patterns: self.ide_ignore_patterns.clone(),
                    };
                    crate::save_config(&config);
                    self.refresh_file_tree();
                    self.window.as_ref().unwrap().request_redraw();
                }
            }
            UiId::SettingsIdeRemoveIgnore(idx) => {
                if idx < self.ide_ignore_patterns.len() {
                    self.ide_ignore_patterns.remove(idx);
                    let config = crate::Config {
                        window_width: self.window_width,
                        window_height: self.window_height,
                        maximized: self.window.as_ref().map(|w| w.is_maximized()).unwrap_or(false),
                        ide_workspaces: self.ide_workspaces.clone(),
                        ide_ignore_patterns: self.ide_ignore_patterns.clone(),
                    };
                    crate::save_config(&config);
                    self.refresh_file_tree();
                    self.window.as_ref().unwrap().request_redraw();
                }
            }
            UiId::SettingsIdeIgnoreInput => {
                self.settings_ignore_focused = true;
                self.window.as_ref().unwrap().request_redraw();
            }

                        // LSP panel
            UiId::LspServerRestart(_idx) => {
                if let Some(lsp) = &mut self.lsp {
                    lsp.restart_python();
                    self.ide_panel.lsp_servers = lsp.servers_info();
                }
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::LspServerToggle(idx) => {
                if idx < self.ide_panel.lsp_servers.len() {
                    let is_disabled = matches!(self.ide_panel.lsp_servers[idx].status, crate::lsp::LspServerStatus::Disabled);
                    if let Some(lsp) = &mut self.lsp {
                        if is_disabled {
                            lsp.enable_python();
                        } else {
                            lsp.disable_python();
                        }
                        self.ide_panel.lsp_servers = lsp.servers_info();
                    }
                }
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::LspServerStop(_idx) => {
                if let Some(lsp) = &mut self.lsp {
                    lsp.disable_python();
                    self.ide_panel.lsp_servers = lsp.servers_info();
                }
                self.window.as_ref().unwrap().request_redraw();
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
            UiId::LspServerFixAll(idx) => {
                if let Some(lsp) = &mut self.lsp {
                    if idx < self.ide_panel.lsp_servers.len() {
                        if let Some(request_id) = lsp.request_fix_all(&self.file_extension) {
                            self.pending_fix_all_id = Some(request_id);
                        }
                    }
                }
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::LspLogFoldToggle(server_idx, line_idx) => {
                if server_idx < self.ide_panel.lsp_servers.len() {
                    let name = self.ide_panel.lsp_servers[server_idx].name;
                    if let Some(ed) = self.ide_panel.lsp_log_editors.get_mut(name) {
                        if ed.folded_lines.contains(&line_idx) {
                            ed.folded_lines.remove(&line_idx);
                        } else if ed.foldable_lines.contains_key(&line_idx) {
                            ed.folded_lines.insert(line_idx);
                        }
                    }
                }
                self.window.as_ref().unwrap().request_redraw();
            }

            // Sidebar
            UiId::SidebarSlot(panel_id) => {
                self.ide_panel.toggle(panel_id);
                if panel_id == crate::app::PanelId::Explorer && self.ide_panel.is_open(panel_id) {
                    if self.ide_panel.file_tree_nodes.is_empty() {
                        self.refresh_file_tree();
                        self.start_file_watcher();
                    }
                }
                crate::save_panel_state(&self.ide_panel);
                self.window.as_ref().unwrap().request_redraw();
            }

                                    // File tree
            UiId::FileTreeNode(idx) => {
                self.handle_file_tree_click(idx);
                self.window.as_ref().unwrap().request_redraw();
            }

            // Search panel
            UiId::SearchClose => {
                self.show_search = false;
                self.search_focused = false;
                self.search_results.clear();
                self.search_current_idx = None;
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::SearchNext => {
                if !self.search_results.is_empty() {
                    if let Some(idx) = self.search_current_idx {
                        self.search_current_idx = Some((idx + 1) % self.search_results.len());
                    }
                    self.jump_to_search_result();
                    self.window.as_ref().unwrap().request_redraw();
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
                    self.window.as_ref().unwrap().request_redraw();
                }
            }
            UiId::SearchCaseToggle => {
                self.search_case_sensitive = !self.search_case_sensitive;
                self.update_search();
                self.jump_to_search_result();
                self.window.as_ref().unwrap().request_redraw();
            }
        }
    }
}
