use crate::app::{App, LspActionItem};

impl App {
    /// Возвращает (x, y, w, h) области LSP-панели или None если не открыта
    pub(crate) fn lsp_panel_bounds(&self) -> Option<(f32, f32, f32, f32)> {
        let s = self.renderer.as_ref()?.scale_factor;
        let is_top = self.ide_panel.slots.iter().any(|sl| {
            sl.id == crate::app::PanelId::LspServers && sl.group == crate::app::PanelGroup::Top
        });
        if is_top {
            let wh = self.window.as_ref()?.inner_size().height as f32;
            Some((48.0 * s, 32.0 * s, self.ide_panel.left_width * s, wh - 32.0 * s))
        } else {
            let first = self.ide_panel.slots.iter()
                .find(|sl| sl.group == crate::app::PanelGroup::Bottom && sl.open)?;
            if first.id != crate::app::PanelId::LspServers { return None; }
            let tab_h = 32.0 * s;
            let panel_bottom_h = self.ide_panel.bottom_height * s;
            let wh = self.window.as_ref()?.inner_size().height as f32;
            let ww = self.window.as_ref()?.inner_size().width as f32;
            Some((48.0 * s, wh - panel_bottom_h + 1.0 + tab_h, ww - 48.0 * s, panel_bottom_h - 1.0 - tab_h))
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
        if !self.ide_panel.lsp_logs_expanded.contains(info.name) { return 0.0; }
        let lines: usize = if let Some(ed) = self.ide_panel.lsp_log_editors.get(info.name) {
            let mut count = 0usize;
            let mut skip_until: Option<usize> = None;
            for i in 0..ed.line_offsets.len() {
                if let Some(tgt) = skip_until { if i < tgt { continue; } skip_until = None; }
                count += 1;
                if ed.folded_lines.contains(&i) { skip_until = Some(ed.foldable_lines[&i]); }
            }
            count
        } else {
            info.logs.iter().map(|e| e.text.split('\n').count()).sum()
        };
        (lines as f32 * 16.0 * s).max(50.0 * s) + 20.0 * s
    }

    /// Максимальная ширина строк в логах (для горизонтального скролла)
    pub(crate) fn lsp_max_log_width(&mut self, _s: f32) -> f32 {
        let mut max_w = 0.0f32;
        for info in &self.ide_panel.lsp_servers {
            if !self.ide_panel.lsp_logs_expanded.contains(info.name) { continue; }
            for entry in &info.logs {
                for line in entry.text.split('\n') {
                    let s = if line.len() > 250 { &line[..250] } else { line };
                    let lw = self.renderer.as_mut().unwrap().measure_mono_width(s, 0.7);
                    if lw > max_w { max_w = lw; }
                }
            }
        }
        max_w
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
        let pending_id = if !diags.is_empty() {
            if let Some(lsp) = &mut self.lsp {
                let ext = self.file_extension.clone();
                let sl = cursor_line;
                let el = cursor_line;
                let sc = diags.iter().map(|d| d.start_col).min().unwrap_or(0);
                let ec = diags.iter().map(|d| d.end_col).max().unwrap_or(0);
                lsp.request_code_actions(&ext, sl, sc, el, ec, &diags)
            } else {
                None
            }
        } else {
            None
        };

        if items.is_empty() && pending_id.is_none() {
            return; // нечего показывать
        }

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
                if let (Some(edit), Some(path)) = (action.edit, self.file_path.clone()) {
                    let new_text = crate::lsp::apply_workspace_edit_to_text(
                        &self.editor.get_full_text(),
                        &edit,
                        &path,
                    );
                    if new_text != self.editor.get_full_text() {
                        self.apply_full_text_replacement(new_text);
                    }
                }
            }
            crate::app::LspActionItem::AddNoqa { codes } => {
                self.insert_noqa_comment(menu.cursor_line, &codes);
            }
            crate::app::LspActionItem::AddNoqaAll => {
                self.insert_noqa_comment(menu.cursor_line, &[]);
            }
        }

        self.window.as_ref().unwrap().request_redraw();
    }

    /// Вставляет/обновляет # noqa комментарий на указанной строке
    pub(crate) fn insert_noqa_comment(&mut self, line: u32, codes: &[String]) {
        let line = line as usize;
        let line_end = if line + 1 < self.editor.line_offsets.len() {
            self.editor.line_offsets[line + 1] - 1 // позиция перед \n
        } else {
            self.editor.len()
        };

        // Читаем текущую строку
        let line_start = self.editor.line_offsets.get(line).copied().unwrap_or(0);
        let mut line_bytes = Vec::with_capacity(line_end - line_start);
        for i in line_start..line_end {
            line_bytes.push(self.editor.byte_at(i));
        }
        let line_text = String::from_utf8_lossy(&line_bytes);

        // Вычисляем куда вставить
        if let Some(noqa_pos_in_line) = line_text.find("# noqa") {
            // Уже есть noqa — добавляем коды если нужно
            if codes.is_empty() {
                return; // Уже есть # noqa, всё ок
            }
            // Парсим существующие коды
            let noqa_byte_start = line_start + noqa_pos_in_line;
            let noqa_text = &line_text[noqa_pos_in_line..];
            let existing = if let Some(colon) = noqa_text.find(": ") {
                noqa_text[colon + 2..]
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

            // Заменяем noqa блок
            let new_noqa = format!("# noqa: {}", merged.join(", "));
            let old_noqa_len = line_text.len() - noqa_pos_in_line;

            // Удаляем старый noqa
            self.editor.cursor = noqa_byte_start;
            for _ in 0..old_noqa_len {
                let _ = self.editor.delete_forward();
            }
            // Вставляем новый
            let ins_start = self.editor.cursor;
            let (del_info, ins_len) = self.editor.insert_str(&new_noqa);
            if let Some((off, len)) = del_info {
                self.highlighter.shift_delete(off, len);
            }
            self.highlighter
                .shift_insert(ins_start, ins_len, Some(&new_noqa));
        } else {
            // Нет noqa — добавляем в конец строки
            self.editor.cursor = line_end;
            let noqa = if codes.is_empty() {
                "  # noqa".to_string()
            } else {
                format!("  # noqa: {}", codes.join(", "))
            };
            let ins_start = self.editor.cursor;
            let (del_info, ins_len) = self.editor.insert_str(&noqa);
            if let Some((off, len)) = del_info {
                self.highlighter.shift_delete(off, len);
            }
            self.highlighter
                .shift_insert(ins_start, ins_len, Some(&noqa));
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

    /// Заменяет весь текст редактора новым (для workspace edit)
    pub(crate) fn apply_full_text_replacement(&mut self, new_text: String) {
        let version = self.editor.version + 1;
        self.editor = crate::editor::Editor::new(new_text.len() + 8192);
        self.editor.version = version;
        let _ = self.editor.insert_str(&new_text);
        self.editor.cursor = 0;
        self.editor.set_original_text();
        self.highlighter
            .reset(version, new_text.clone(), self.file_extension.clone());
        if let (Some(lsp), Some(path)) = (&mut self.lsp, &self.file_path) {
            lsp.notify_change(
                path,
                &self.file_extension.clone(),
                &new_text,
                version as i32,
            );
        }
        App::update_window_title(self.window.as_ref().unwrap(), &self.base_title, true);
    }
}