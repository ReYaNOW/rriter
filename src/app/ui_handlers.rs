/// Обработчики UI событий - централизованная логика для кликов по кнопкам
/// Устраняет дублирование кода между input.rs и ui.rs
use crate::app::App;
use crate::editor::Editor;
use crate::render_view::{editor_bottom_blank_lines, editor_scroll_content_height};
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
    let thumb_w = (track_w / (max_scroll + track_w).max(1.0) * track_w).max(40.0 * scale);
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
}

impl App {
    /// Обрабатывает клик по UI элементу
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn handle_ui_click(&mut self, id: UiId) {
        match id {
            UiId::HoverPopupScroll => {}
            UiId::TerminalBody => {
                self.is_dragging = true;
                self.search_focused = false;
                self.ide_panel.term_search_focused = false;
                self.ide_panel.file_tree_focused = false;
                let active = self.ide_panel.active_terminal;
                if let Some(term) = self.ide_panel.terminals.get_mut(active) {
                    term.grid.lock().unwrap().selection = None;
                }
            }
            UiId::TerminalScrollY => {
                let active = self.ide_panel.active_terminal;
                if let Some(term) = self.ide_panel.terminals.get_mut(active) {
                    term.scroll_y.is_dragging = true;
                }
            }
            UiId::TerminalTab(idx) => {
                if idx < self.ide_panel.terminals.len() {
                    self.ide_panel.active_terminal = idx;
                }
            }
            UiId::TerminalTabClose(idx) => {
                if idx < self.ide_panel.terminals.len() {
                    self.ide_panel.terminals.remove(idx);
                    if self.ide_panel.terminals.is_empty() {
                        self.ide_panel
                            .terminals
                            .push(crate::app::terminal::Terminal::spawn(self.window.clone()));
                        self.ide_panel.active_terminal = 0;
                    } else if self.ide_panel.active_terminal >= self.ide_panel.terminals.len() {
                        self.ide_panel.active_terminal =
                            self.ide_panel.terminals.len().saturating_sub(1);
                    }
                }
            }
            UiId::TerminalAdd => {
                self.ide_panel
                    .terminals
                    .push(crate::app::terminal::Terminal::spawn(self.window.clone()));
                self.ide_panel.active_terminal = self.ide_panel.terminals.len() - 1;
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
                    term.grid.lock().unwrap().selection = None;
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
                self.is_dragging_search = true;
                if let Some(r) = self.renderer.as_mut() {
                    let mx = r.last_mouse_x;
                    let s = r.scale_factor;
                    let panel_w =
                        self.window.as_ref().unwrap().inner_size().width as f32 - 48.0 * s;
                    let search_w = 480.0 * s;
                    let search_x = 48.0 * s + panel_w - search_w - 20.0 * s;
                    let input_x = search_x + 10.0 * s;

                    let text = self.ide_panel.term_search_editor.get_full_text();
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
                    self.base_title = "Безымянный".to_string();
                    let old_version = self.editor.version;
                    self.editor = Editor::new(8192);
                    self.editor.version = old_version + 1;
                    self.editor.set_original_text();
                    self.editor.sync_edits.clear();
                    while let Ok(_) = self.highlighter.rx.try_recv() {}
                    self.highlighter
                        .reset(self.editor.version, "".to_string(), "".to_string());
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
                        enable_telemetry: crate::render_view::TELEMETRY_ENABLED
                            .load(std::sync::atomic::Ordering::Relaxed),
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
                        enable_telemetry: crate::render_view::TELEMETRY_ENABLED
                            .load(std::sync::atomic::Ordering::Relaxed),
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
                        enable_telemetry: crate::render_view::TELEMETRY_ENABLED
                            .load(std::sync::atomic::Ordering::Relaxed),
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
                        if self.ide_panel.lsp_servers.iter().all(|info| {
                            matches!(info.status, crate::lsp::LspServerStatus::Disabled)
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
                }
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::LspServerStop(_idx) => {
                if let Some(lsp) = &mut self.lsp {
                    lsp.disable_python();
                    self.ide_panel.lsp_servers = lsp.servers_info();
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

            // Sidebar
            UiId::SidebarSlot(panel_id) => {
                self.ide_panel.toggle(panel_id);
                if panel_id == crate::app::PanelId::Terminal && self.ide_panel.is_open(panel_id) {
                    self.ide_panel.terminal_focused = true;
                    self.ide_panel.term_search_focused = false;
                    if self.ide_panel.terminals.is_empty() {
                        self.ide_panel
                            .terminals
                            .push(crate::app::terminal::Terminal::spawn(self.window.clone()));
                        self.ide_panel.active_terminal = 0;
                    }
                }
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
                self.handle_file_tree_left_click(idx, false);
                self.window.as_ref().unwrap().request_redraw();
            }
            UiId::FileTreeArrow(idx) => {
                self.handle_file_tree_left_click(idx, true);
                self.window.as_ref().unwrap().request_redraw();
            }
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
                self.ide_panel.term_search_focused = false;
                self.ide_panel.file_tree_focused = false;
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
            UiId::EditorScrollbarY => {
                if let Some(r) = self.renderer.as_mut() {
                    self.scroll_y.is_dragging = true;
                    let mx = r.last_mouse_x;
                    let my = r.last_mouse_y;
                    self.last_click_pos = (mx, my);

                    let s = r.scale_factor;
                    let tab_bar_h = if self.show_welcome || !self.is_ide_mode {
                        0.0
                    } else {
                        38.0 * s
                    };
                    let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                    let editor_height = (wh - tab_bar_h).max(0.0);
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
                if let Some(r) = self.renderer.as_mut() {
                    let mx = r.last_mouse_x;
                    let my = r.last_mouse_y;
                    self.last_click_pos = (mx, my);
                    self.last_click_time =
                        std::time::Instant::now() - std::time::Duration::from_millis(200);

                    let s = r.scale_factor;
                    let tab_bar_h = if self.show_welcome || !self.is_ide_mode {
                        0.0
                    } else {
                        38.0 * s
                    };
                    let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                    let editor_height = (wh - tab_bar_h).max(0.0);
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
                if let Some(r) = self.renderer.as_mut() {
                    let mx = r.last_mouse_x;
                    let s = r.scale_factor;
                    let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                    let tab_bar_h = if self.show_welcome || !self.is_ide_mode {
                        0.0
                    } else {
                        38.0 * s
                    };
                    let max_y = r.get_max_scroll(&self.editor, wh - tab_bar_h);
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
            UiId::EditorTextBody => {
                self.is_dragging = true;
                self.ide_panel.file_tree_focused = false;
                crate::app::mouse::clear_hover_popup(self.renderer.as_mut());
                if let Some(r) = self.renderer.as_mut() {
                    r.suppress_popups_until_next_mouse_move();
                }
                self.scroll_y.anim_speed = 15.0;
                self.scroll_y.stop_anim();
                self.ide_panel.lsp_logs_focused = None;
                self.search_focused = false;
                self.ide_panel.term_search_focused = false;
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

                    let tab_bar_h = if self.show_welcome || !self.is_ide_mode {
                        0.0
                    } else {
                        38.0 * r.scale_factor
                    };
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
                self.ide_panel.lsp_scroll_y.is_dragging = true;
            }
            UiId::LspScrollX => {
                self.ide_panel.lsp_scroll_x.is_dragging = true;
            }
            UiId::LspLogScrollY(server_idx) => {
                if server_idx < self.ide_panel.lsp_servers.len() {
                    let name = self.ide_panel.lsp_servers[server_idx].name.to_string();
                    let scroll = self
                        .ide_panel
                        .lsp_logs_scroll_y
                        .entry(name)
                        .or_insert_with(|| crate::scroll::ScrollState::new(15.0));
                    scroll.is_dragging = true;
                }
            }
            UiId::LspLogScrollX(server_idx) => {
                if server_idx < self.ide_panel.lsp_servers.len() {
                    let name = self.ide_panel.lsp_servers[server_idx].name.to_string();
                    let scroll = self
                        .ide_panel
                        .lsp_logs_scroll_x
                        .entry(name)
                        .or_insert_with(|| crate::scroll::ScrollState::new(15.0));
                    scroll.is_dragging = true;
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
                    #[cfg(target_os = "windows")]
                    let _ = std::process::Command::new("cmd")
                        .args(["/c", "start", "", &href])
                        .spawn();
                    #[cfg(target_os = "macos")]
                    let _ = std::process::Command::new("open").arg(&href).spawn();
                    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
                    let _ = std::process::Command::new("xdg-open").arg(&href).spawn();
                }
            }
            UiId::ProblemUrl(idx) => {
                if let Some((path, diag_idx)) = self.ide_panel.flat_diags.get(idx) {
                    if let Some(lsp) = &self.lsp {
                        let diags = lsp.get_diagnostics(path);
                        if let Some(diag) = diags.get(*diag_idx) {
                            if let Some(href) = &diag.code_href {
                                #[cfg(target_os = "windows")]
                                let _ = std::process::Command::new("cmd")
                                    .args(["/c", "start", "", href])
                                    .spawn();
                                #[cfg(target_os = "macos")]
                                let _ = std::process::Command::new("open").arg(href).spawn();
                                #[cfg(not(any(target_os = "windows", target_os = "macos")))]
                                let _ = std::process::Command::new("xdg-open").arg(href).spawn();
                            }
                        }
                    }
                }
            }
            UiId::ProblemJump(idx) => {
                if let Some((path, diag_idx)) = self.ide_panel.flat_diags.get(idx).cloned() {
                    if diag_idx == usize::MAX {
                        return;
                    }
                    if self.file_path.as_ref() != Some(&path) {
                        self.open_file_in_tab(path.clone(), true);
                    }
                    if let Some(lsp) = &self.lsp {
                        let diags = lsp.get_diagnostics(&path);
                        if let Some(diag) = diags.get(diag_idx) {
                            let start_line = diag.start_line as usize;
                            let end_line = diag.end_line as usize;
                            if start_line < self.editor.line_offsets.len()
                                && end_line < self.editor.line_offsets.len()
                            {
                                let full_text = self.editor.get_full_text();

                                let end_byte = crate::lsp::lsp_pos_to_offset(
                                    &full_text,
                                    diag.end_line,
                                    diag.end_col,
                                );

                                self.editor.cursor = end_byte.min(self.editor.len());
                                self.editor.selection_anchor = None;

                                let tab_h = if self.is_ide_mode {
                                    32.0 * self.renderer.as_ref().unwrap().scale_factor
                                } else {
                                    38.0 * self.renderer.as_ref().unwrap().scale_factor
                                };

                                crate::app::App::ensure_cursor_visible(
                                    &mut self.scroll_y.target,
                                    &mut self.scroll_x.target,
                                    &self.editor,
                                    self.renderer.as_mut().unwrap(),
                                    self.window_height as f32,
                                    self.window_width as f32,
                                    tab_h,
                                );

                                let renderer = self.renderer.as_mut().unwrap();
                                let (_, cy) = renderer.get_cursor_xy(&self.editor);
                                let visible_h =
                                    (self.window_height as f32 - tab_h).max(renderer.line_height);
                                let max_scroll = renderer.get_max_scroll(&self.editor, visible_h);
                                self.scroll_y.target =
                                    (cy - renderer.baseline_offset - visible_h * 0.45)
                                        .max(0.0)
                                        .min(max_scroll)
                                        .round();

                                self.scroll_y.anim_speed = 7.0;
                                self.scroll_x.anim_speed = 7.0;

                                self.window.as_ref().unwrap().request_redraw();
                            }
                        }
                    }
                }
            }
            UiId::CopyDiagnostic(idx) => {
                if let Some((path, diag_idx)) = self.ide_panel.flat_diags.get(idx) {
                    if let Some(diag) = self
                        .lsp
                        .as_ref()
                        .and_then(|l| l.diagnostics.get(path))
                        .and_then(|diags| diags.get(*diag_idx))
                    {
                        let message = diag.message.clone();
                        self.set_clipboard_text(message);
                        self.ide_panel.diag_copied_idx = Some(idx);
                    }
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::PopupCopyDiagnostic(idx) => {
                if let Some(path) = &self.file_path {
                    if let Some(diag) = self
                        .lsp
                        .as_ref()
                        .and_then(|l| l.diagnostics.get(path))
                        .and_then(|diags| diags.get(idx))
                    {
                        let message = diag.message.clone();
                        self.set_clipboard_text(message);
                        self.ide_panel.diag_copied_idx = Some(idx);
                    }
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            UiId::LspLogArea(server_idx) => {
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
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
        }
    }
}
