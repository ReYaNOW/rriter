/// Обработчики UI событий - централизованная логика для кликов по кнопкам
/// Устраняет дублирование кода между input.rs и ui.rs
use crate::app::App;
use crate::editor::Editor;
use crate::ui_system::UiId;

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
                self.highlighter
                    .reset(self.editor.version, "".to_string(), "".to_string());
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
                self.highlighter
                    .reset(self.editor.version, "".to_string(), "".to_string());

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
                        self.base_title = path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        let ext = path
                            .extension()
                            .map(|e| e.to_string_lossy().to_string())
                            .unwrap_or_default();
                        self.file_extension = ext.clone();

                        let old_version = self.editor.version;
                        self.editor = Editor::new(content.len() + 8192);
                        let _ = self.editor.insert_str(&content);
                        self.editor.cursor = 0;
                        self.editor.version = old_version + 1;
                        self.editor.set_original_text();
                        self.editor.sync_edits.clear();
                        self.highlighter.reset(self.editor.version, content, ext);

                        App::update_window_title(
                            self.window.as_ref().unwrap(),
                            &self.base_title,
                            false,
                        );
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
                        App::update_window_title(
                            self.window.as_ref().unwrap(),
                            &self.base_title,
                            false,
                        );
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
                        maximized: self
                            .window
                            .as_ref()
                            .map(|w| w.is_maximized())
                            .unwrap_or(false),
                        ide_workspaces: self.ide_workspaces.clone(),
                        ide_ignore_patterns: self.ide_ignore_patterns.clone(),
                    };
                    crate::save_config(&config);
                    self.refresh_file_tree();
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
                    let config = crate::Config {
                        window_width: self.window_width,
                        window_height: self.window_height,
                        maximized: self
                            .window
                            .as_ref()
                            .map(|w| w.is_maximized())
                            .unwrap_or(false),
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
                        maximized: self
                            .window
                            .as_ref()
                            .map(|w| w.is_maximized())
                            .unwrap_or(false),
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
                    let is_disabled = matches!(
                        self.ide_panel.lsp_servers[idx].status,
                        crate::lsp::LspServerStatus::Disabled
                    );
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
                if let Some(r) = self.renderer.as_mut() {
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
                    let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                    let max_scroll = r.get_max_scroll(&self.editor, wh);
                    let ry = slot_index as f32 * r.line_height;
                    let padding = r.line_height * 3.0;
                    self.scroll_y.target = (line_y - ry - padding)
                        .max(0.0)
                        .clamp(0.0, max_scroll)
                        .round();
                    self.scroll_y.anim_speed = 15.0;
                }
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::SearchInput => {
                self.search_focused = true;
                self.is_dragging_search = true;
                if let Some(r) = self.renderer.as_mut() {
                    let mx = r.last_mouse_x;
                    let s = r.scale_factor;
                    let window_width = self.window.as_ref().unwrap().inner_size().width as f32;
                    let minimap_w = r.minimap_width;
                    let scrollbar_w = if r.max_scroll_x > 0.0 { 10.0 * s } else { 0.0 };
                    let scrollbar_x = window_width - minimap_w - scrollbar_w;
                    let search_w = 480.0 * s;
                    let search_x = scrollbar_x - search_w - 20.0 * s;
                    let input_x = search_x + 10.0 * s;

                    let text = self.search_editor.get_full_text();
                    let x_offset = (mx - (input_x + 5.0 * s)).max(0.0);
                    let mut current_x = 0.0;
                    let mut target_idx = text.len();
                    let mut byte_idx = 0;
                    for c in text.chars() {
                        let adv = r.get_ui_glyph(c).map(|g| g.advance).unwrap_or(10.0);
                        if x_offset <= current_x + adv / 2.0 {
                            target_idx = byte_idx;
                            break;
                        }
                        current_x += adv;
                        byte_idx += c.len_utf8();
                    }
                    self.search_editor.cursor = target_idx;
                    self.search_editor.selection_anchor = Some(target_idx);
                }
                self.window.as_ref().unwrap().request_redraw();
            }
                        UiId::EditorScrollbarY | UiId::EditorMinimap => {
                if let Some(r) = self.renderer.as_ref() {
                    self.scroll_y.is_dragging = true;
                    let mx = r.last_mouse_x;
                    let my = r.last_mouse_y;
                    self.last_click_pos = (mx, my);
                    self.last_click_time = std::time::Instant::now();
                }
            }
            UiId::EditorScrollbarX => {
                self.scroll_x.is_dragging = true;
            }
            UiId::EditorTextBody => {
                self.is_dragging = true;
                self.scroll_y.anim_speed = 15.0;
                self.scroll_y.stop_anim();
                self.ide_panel.lsp_logs_focused = None;
                self.search_focused = false;
                self.settings_ignore_focused = false;

                if let Some(r) = self.renderer.as_mut() {
                    let mx = r.last_mouse_x;
                    let my = r.last_mouse_y;
                    let now = std::time::Instant::now();
                    let dx = mx - self.last_click_pos.0;
                    let dy = my - self.last_click_pos.1;
                    let dist_sq = dx * dx + dy * dy;

                    if now.duration_since(self.last_click_time).as_millis() < 400 && dist_sq < 25.0
                    {
                        self.click_count += 1;
                    } else {
                        self.click_count = 1;
                    }

                    self.last_click_time = now;
                    self.last_click_pos = (mx, my);

                    self.editor
                        .set_cursor_at_pos(mx, my + self.scroll_y.current, r, true);
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
                self.ide_panel.is_resizing_left = true;
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::ResizeBottom => {
                self.ide_panel.is_resizing_bottom = true;
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::LspScrollY => {
                self.ide_panel.lsp_scroll_y.is_dragging = true;
            }
            UiId::LspScrollX => {
                self.ide_panel.lsp_scroll_x.is_dragging = true;
            }
            UiId::LspLogArea(server_idx) => {
                if server_idx < self.ide_panel.lsp_servers.len() {
                    self.ide_panel.lsp_logs_focused =
                        Some(self.ide_panel.lsp_servers[server_idx].name.to_string());
                }
                self.is_dragging_lsp_log = true;
                if let Some(r) = self.renderer.as_ref() {
                    let mx = r.last_mouse_x;
                    let my = r.last_mouse_y;
                    let now = std::time::Instant::now();
                    let dx = mx - self.last_click_pos.0;
                    let dy = my - self.last_click_pos.1;
                    let dist_sq = dx * dx + dy * dy;

                    if now.duration_since(self.last_click_time).as_millis() < 400 && dist_sq < 25.0
                    {
                        self.click_count += 1;
                    } else {
                        self.click_count = 1;
                    }
                    self.last_click_time = now;
                    self.last_click_pos = (mx, my);
                }
                self.window.as_ref().unwrap().request_redraw();
            }
        }
    }
}
