use crate::app::App;
use crate::editor::Editor;
use crate::ui_system::UiId;
use super::{UiClickFlow, repeated_ui_click};

impl App {
    pub(super) fn handle_lsp_ui_click(&mut self, id: UiId, same_click_target: bool) -> UiClickFlow {
        match id {

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
                    return UiClickFlow::Return;
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
            UiId::LspScrollY => {
                let geometry = self.ui_registry.rect_for(UiId::LspScrollY).and_then(|rect| {
                    let renderer = self.renderer.as_ref()?;
                    let s = renderer.scale_factor;
                    crate::app::lsp_actions::lsp_panel_scrollbar(
                        rect,
                        self.lsp_panel_total_h(s),
                        self.ide_panel.lsp_scroll_y.current,
                    )
                    .geometry(s)
                });
                let pointer_y = self.renderer.as_ref().map_or(f32::NAN, |r| r.last_mouse_y);
                let _ = crate::app::mouse::press_scrollbar(
                    &mut self.ide_panel.lsp_scroll_y,
                    geometry,
                    0.0,
                    pointer_y,
                );
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
                            crate::app::lsp_actions::lsp_log_scrollbar(
                                rect,
                                rect.3,
                                content_len,
                                self.ide_panel.lsp_logs_scroll_y
                                    .get(&name)
                                    .map_or(0.0, |scroll| scroll.current),
                                crate::render_view::scrollbar_widget::ScrollbarAxis::Vertical,
                            )
                            .geometry(s)
                        });
                    let scroll = self
                        .ide_panel
                        .lsp_logs_scroll_y
                        .entry(name)
                        .or_insert_with(|| crate::scroll::ScrollState::new(15.0));
                    let pointer_y = self.renderer.as_ref().map_or(f32::NAN, |r| r.last_mouse_y);
                    let _ = crate::app::mouse::press_scrollbar(scroll, geometry, 0.0, pointer_y);
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
                            crate::app::lsp_actions::lsp_log_scrollbar(
                                rect,
                                rect.2,
                                max_line_w + 20.0 * s,
                                self.ide_panel.lsp_logs_scroll_x
                                    .get(&name)
                                    .map_or(0.0, |scroll| scroll.current),
                                crate::render_view::scrollbar_widget::ScrollbarAxis::Horizontal,
                            )
                            .geometry(s)
                        });
                    let scroll = self
                        .ide_panel
                        .lsp_logs_scroll_x
                        .entry(name)
                        .or_insert_with(|| crate::scroll::ScrollState::new(15.0));
                    let pointer_x = self.renderer.as_ref().map_or(f32::NAN, |r| r.last_mouse_x);
                    let _ = crate::app::mouse::press_scrollbar(scroll, geometry, pointer_x, 0.0);
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
                if let Some(row) = self.ide_panel.flat_diags.get(idx) {
                    if row.is_group_header() {
                        let path = row.path.to_path_buf();
                        if !self.ide_panel.problems_collapsed.remove(&path) {
                            self.ide_panel.problems_collapsed.insert(path);
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
                if let Some(href) = self.hover.diag_href.as_deref() {
                    let _ = crate::platform::open_url(self.external_requests.sink(), href);
                }
            }
            UiId::ProblemUrl(idx) => {
                if let Some(row) = self.ide_panel.flat_diags.get(idx)
                    && let Some(diag) = row.diagnostic()
                    && let Some(href) = &diag.code_href
                {
                    let _ = crate::platform::open_url(self.external_requests.sink(), href.as_ref());
                }
            }
            UiId::ProblemJump(idx) => {
                if let Some((path, diag_idx, end_position)) = self
                    .ide_panel.flat_diags.get(idx).map(|row| {
                        (
                            row.path.clone(),
                            row.index,
                            row.diagnostic().map(|diagnostic| {
                                (diagnostic.end_line, diagnostic.end_col)
                            }),
                        )
                    })
                {
                    if diag_idx == usize::MAX {
                        return UiClickFlow::Return;
                    }
                    if self.ide_panel.is_query_problem_path(&path) {
                        self.jump_to_active_database_query_diagnostic(diag_idx);
                    } else if let Some((line, col)) = end_position {
                        self.jump_to_lsp_position_in_file(path.to_path_buf(), line, col, true, 0.45);
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
                    .and_then(|row| row.diagnostic())
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
                let mut message = self
                    .hover
                    .diag_copy_texts
                    .get(idx)
                    .filter(|text| !text.is_empty())
                    .cloned();
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
                    message = (!self.hover.diag_text.is_empty())
                        .then(|| self.hover.diag_text.clone());
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
            _ => return UiClickFlow::NotMine,
        }
        UiClickFlow::Handled
    }
}
