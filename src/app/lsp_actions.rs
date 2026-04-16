use crate::app::App;

impl App {
    /// Возвращает (x, y, w, h) области LSP-панели или None если не открыта
    pub(crate) fn lsp_panel_bounds(&self) -> Option<(f32, f32, f32, f32)> {
        let s = self.renderer.as_ref()?.scale_factor;
        let is_top = self.ide_panel.slots.iter().any(|sl| {
            sl.id == crate::app::PanelId::LspServers && sl.group == crate::app::PanelGroup::Top
        });
        if is_top {
            let wh = self.window.as_ref()?.inner_size().height as f32;
            let panel_bottom_h = if self.ide_panel.any_bottom_open() {
                self.ide_panel.bottom_height * s
            } else {
                0.0
            };
            Some((
                48.0 * s,
                32.0 * s,
                self.ide_panel.left_width * s,
                wh - 32.0 * s - panel_bottom_h,
            ))
        } else {
            let first = self
                .ide_panel
                .slots
                .iter()
                .find(|sl| sl.group == crate::app::PanelGroup::Bottom && sl.open)?;
            if first.id != crate::app::PanelId::LspServers {
                return None;
            }
            let tab_h = 32.0 * s;
            let panel_bottom_h = self.ide_panel.bottom_height * s;
            let wh = self.window.as_ref()?.inner_size().height as f32;
            let ww = self.window.as_ref()?.inner_size().width as f32;
            Some((
                48.0 * s,
                wh - panel_bottom_h + 1.0 + tab_h,
                ww - 48.0 * s,
                panel_bottom_h - 1.0 - tab_h,
            ))
        }
    }

    /// Подсчитывает суммарную высоту LSP-панели с учётом свёрнутых блоков
    pub(crate) fn lsp_panel_total_h(&self, s: f32) -> f32 {
        let mut total = 8.0 * s;
        for info in &self.ide_panel.lsp_servers {
            total += 136.0 * s + self.lsp_server_logs_h(info, s) + 16.0 * s;
        }
        total
    }

    /// Высота блока логов одного LSP-сервера (0 если не развёрнут)
    pub(crate) fn lsp_server_logs_h(&self, info: &crate::lsp::LspServerInfo, s: f32) -> f32 {
        if !self.ide_panel.lsp_logs_expanded.contains(info.name) {
            return 0.0;
        }
        let (inner_total_h, _) = self.lsp_server_inner_size(info, s);
        (inner_total_h + 20.0 * s).clamp(50.0 * s, 800.0 * s)
    }

    pub(crate) fn lsp_server_inner_size(
        &self,
        info: &crate::lsp::LspServerInfo,
        s: f32,
    ) -> (f32, f32) {
        if let Some(log_ed) = self.ide_panel.lsp_log_editors.get(info.name) {
            let mut lines = 0;
            let mut max_w = 0.0f32;
            let mut phys_line = 0;
            while phys_line < log_ed.line_offsets.len() {
                let start = log_ed.line_offsets[phys_line];
                let end = if phys_line + 1 < log_ed.line_offsets.len() {
                    log_ed.line_offsets[phys_line + 1].saturating_sub(1)
                } else {
                    log_ed.len()
                };
                let w = (end.saturating_sub(start)) as f32 * 7.5 * s;
                if w > max_w {
                    max_w = w;
                }
                lines += 1;
                if log_ed.folded_lines.contains(&phys_line) {
                    if let Some(&fold_end) = log_ed.foldable_lines.get(&phys_line) {
                        phys_line = fold_end;
                    }
                }
                phys_line += 1;
            }
            (lines as f32 * 16.0 * s, max_w)
        } else {
            (0.0, 0.0)
        }
    }

    /// Открывает меню быстрых действий LSP для текущей строки
    pub(crate) fn open_lsp_actions_menu(&mut self) {
        if !self.is_ide_mode || self.show_welcome {
            return;
        }
        let cursor = self.editor.cursor;
        let cursor_line = self
            .editor
            .line_offsets
            .partition_point(|&o| o <= cursor)
            .saturating_sub(1) as u32;

        // Собираем диагностики текущей строки
        let diags: Vec<crate::lsp::Diagnostic> = if let Some(lsp) = &self.lsp {
            lsp.diagnostics_for_line(cursor_line)
                .into_iter()
                .cloned()
                .collect()
        } else {
            Vec::new()
        };

        // Вычисляем позицию меню (под курсором)
        let (cx, cy) = self.renderer.as_mut().unwrap().get_cursor_xy(&self.editor);
        let _s = self.renderer.as_ref().unwrap().scale_factor;
        let menu_x = cx.max(self.renderer.as_ref().unwrap().left_padding);
        let menu_y = cy - self.scroll_y.current + self.renderer.as_ref().unwrap().line_height;

                // Начальные элементы: noqa варианты
        let mut items: Vec<crate::app::LspActionItem> = Vec::new();

        if !diags.is_empty() {
            // Добавляем быстрые фиксы (quickfixes) из диагностики, которые ruff прислал заранее
            for d in &diags {
                for qf in &d.quickfixes {
                    let mut changes = std::collections::HashMap::new();
                    if let Some(path) = self.file_path.clone() {
                        changes.insert(path, qf.edits.clone());
                    }
                    items.push(crate::app::LspActionItem::CodeAction(crate::lsp::CodeAction {
                        title: qf.title.clone(),
                        kind: Some("quickfix".to_string()),
                        edit: Some(crate::lsp::WorkspaceEdit { changes }),
                        code: d.code.clone(),
                    }));
                }
            }

            // Сначала "Добавить # noqa: CODES" для конкретных кодов
            let codes: Vec<String> = diags.iter().filter_map(|d| d.code.clone()).collect();
            if !codes.is_empty() {
                items.push(crate::app::LspActionItem::AddNoqa {
                    codes: codes.clone(),
                });
            }
            // Затем "Добавить # noqa" (всё отключить)
            items.push(crate::app::LspActionItem::AddNoqaAll);
        }

        // Запрашиваем code actions от LSP
                        let pending_id = if let Some(lsp) = &mut self.lsp {
            let ext = self.file_extension.clone();
            let (sl, sc) = crate::lsp::offset_to_lsp_pos(&self.editor.get_full_text(), self.editor.cursor, &self.editor.line_offsets);
            lsp.request_code_actions(&ext, sl, sc, sl, sc, &diags, None)
                } else {
            None
        };

        items.push(crate::app::LspActionItem::OrganizeImports);
        items.push(crate::app::LspActionItem::FixAll);

        self.lsp_actions_menu = Some(crate::app::LspActionsMenu {
            cursor_line,
            items,
            selected: 0,
            menu_x,
            menu_y,
            pending_request_id: pending_id,
        });

        self.window.as_ref().unwrap().request_redraw();
    }

    /// Применяет выбранный элемент меню LSP
    pub fn apply_selected_lsp_action(&mut self) {
        let menu = match self.lsp_actions_menu.take() {
            Some(m) => m,
            None => return,
        };
        if menu.items.is_empty() {
            return;
        }
        let item = menu.items[menu.selected.min(menu.items.len() - 1)].clone();

                match item {
                        crate::app::LspActionItem::CodeAction(action) => {
                if let Some(edit) = action.edit {
                    self.apply_workspace_edit(&edit);
                }
            }
            crate::app::LspActionItem::AddNoqa { codes } => {
                self.insert_noqa_comment(menu.cursor_line, &codes);
            }
            crate::app::LspActionItem::AddNoqaAll => {
                self.insert_noqa_comment(menu.cursor_line, &[]);
            }
            crate::app::LspActionItem::FixAll => {
                if let Some(lsp) = &mut self.lsp {
                    if let Some(id) = lsp.request_fix_all(&self.file_extension) {
                        self.pending_fix_all_id = Some(id);
                    }
                }
            }
            crate::app::LspActionItem::OrganizeImports => {
                if let Some(lsp) = &mut self.lsp {
                    if let Some(id) = lsp.request_organize_imports(&self.file_extension) {
                        self.pending_fix_all_id = Some(id);
                    }
                }
            }
        }

        self.window.as_ref().unwrap().request_redraw();
    }

    /// Вставляет/обновляет # noqa комментарий на указанной строке
        pub(crate) fn insert_noqa_comment(&mut self, line: u32, codes: &[String]) {
        let line = line as usize;
        let line_start = self.editor.line_offsets.get(line).copied().unwrap_or(0);
        let line_end_raw = if line + 1 < self.editor.line_offsets.len() {
            self.editor.line_offsets[line + 1]
        } else {
            self.editor.len()
        };

        let mut actual_end = line_end_raw;
        while actual_end > line_start {
            let b = self.editor.byte_at(actual_end - 1);
            if b == b'\n' || b == b'\r' {
                actual_end -= 1;
            } else {
                break;
            }
        }

        // Читаем текущую строку
        let mut line_bytes = Vec::with_capacity(actual_end - line_start);
        for i in line_start..actual_end {
            line_bytes.push(self.editor.byte_at(i));
        }
        let line_text = String::from_utf8_lossy(&line_bytes);

        // Вычисляем куда вставить
        if let Some(noqa_pos_in_line) = line_text.find("# noqa") {
            let noqa_byte_start = line_start + noqa_pos_in_line;
            let old_noqa_str = &line_text[noqa_pos_in_line..];
                        let new_noqa = if codes.is_empty() {
                "# noqa".to_string()
            } else {
                let existing = if let Some(colon) = old_noqa_str.find(": ") {
                    old_noqa_str[colon + 2..]
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                };

                let mut merged = existing.clone();
                for code in codes {
                    if !merged.contains(code) {
                        merged.push(code.clone());
                    }
                }
                format!("# noqa: {}", merged.join(", "))
            };

                        let (off, len, _) = self.editor.replace_range(noqa_byte_start, actual_end, &new_noqa);
            self.highlighter.shift_delete(off, len);
            self.highlighter.shift_insert(off, new_noqa.len(), Some(&new_noqa));
        } else {
            // Нет noqa — добавляем в конец строки
            let noqa = if codes.is_empty() {
                "  # noqa".to_string()
            } else {
                format!("  # noqa: {}", codes.join(", "))
            };
            let (off, len, _) = self.editor.replace_range(actual_end, actual_end, &noqa);
            self.highlighter.shift_delete(off, len);
            self.highlighter.shift_insert(off, noqa.len(), Some(&noqa));
        }

        // Синхронизируем с LSP и подсветчиком
        if !self.editor.sync_edits.is_empty() {
            let edits = std::mem::take(&mut self.editor.sync_edits);
            if self.is_ide_mode {
                if let (Some(lsp), Some(path)) = (&mut self.lsp, &self.file_path) {
                    let text = self.editor.get_full_text();
                    let ext = self.file_extension.clone();
                    let path = path.clone();
                    lsp.notify_change(&path, &ext, &text, self.editor.version as i32);
                }
            }
            self.highlighter.apply_edits(self.editor.version, edits);
        }

        App::update_window_title(
            self.window.as_ref().unwrap(),
            &self.base_title,
            self.editor.is_dirty(),
        );
    }

        pub(crate) fn apply_workspace_edit(&mut self, edit: &crate::lsp::WorkspaceEdit) {
        if let Some(path) = &self.file_path {
            if let Some(changes) = edit.changes.get(path) {
                let text = self.editor.get_full_text();
                let mut sorted = changes.clone();
                sorted.sort_unstable_by(|a, b| {
                    b.start_line
                        .cmp(&a.start_line)
                        .then(b.start_col.cmp(&a.start_col))
                });

                let mut ops = Vec::new();
                for change in &sorted {
                    let start = crate::lsp::lsp_pos_to_offset(&text, change.start_line, change.start_col);
                    let end = crate::lsp::lsp_pos_to_offset(&text, change.end_line, change.end_col);
                    ops.push((start, end, change.new_text.clone()));
                }

                                for (start, end, new_text) in ops {
                    if start <= end {
                        let (off, len, _) = self.editor.replace_range(start, end, &new_text);
                        self.highlighter.shift_delete(off, len);
                        self.highlighter.shift_insert(off, new_text.len(), Some(&new_text));
                    }
                }

                if !self.editor.sync_edits.is_empty() {
                    let edits = std::mem::take(&mut self.editor.sync_edits);
                    if self.is_ide_mode {
                        if let (Some(lsp), Some(path)) = (&mut self.lsp, &self.file_path) {
                            let text = self.editor.get_full_text();
                            let ext = self.file_extension.clone();
                            let path = path.clone();
                            lsp.notify_change(&path, &ext, &text, self.editor.version as i32);
                        }
                    }
                    self.highlighter.apply_edits(self.editor.version, edits);
                }

                App::update_window_title(
                    self.window.as_ref().unwrap(),
                    &self.base_title,
                    self.editor.is_dirty(),
                );
            }
        }
    }
}
