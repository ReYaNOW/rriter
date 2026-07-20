use crate::app::App;

pub(crate) fn lsp_actions_menu_screen_y(
    cursor_y: f32,
    scroll_y: f32,
    line_height: f32,
    editor_top_inset: f32,
) -> f32 {
    cursor_y - scroll_y + line_height + editor_top_inset
}

pub(crate) fn lsp_server_logs_h_for_content(inner_total_h: f32, content_h: f32, s: f32) -> f32 {
    let max_fit = (content_h - 152.0 * s).max(0.0);
    if max_fit < 68.0 * s {
        return 0.0;
    }
    let max_h = (800.0 * s).min(max_fit);
    let min_h = (84.0 * s).min(max_h);
    (inner_total_h + 54.0 * s).clamp(min_h, max_h)
}

pub(crate) fn lsp_log_scrollbar_thumb(
    track_start: f32,
    track_len: f32,
    viewport_len: f32,
    content_len: f32,
    current_scroll: f32,
    scale: f32,
) -> Option<crate::scroll::ScrollbarThumb> {
    crate::scroll::scrollbar_thumb(
        track_start,
        track_len,
        viewport_len,
        content_len,
        current_scroll,
        20.0 * scale,
    )
}

pub(crate) fn lsp_log_scrollbar_drag_target(
    pointer: f32,
    track_start: f32,
    track_len: f32,
    viewport_len: f32,
    content_len: f32,
    current_scroll: f32,
    scale: f32,
    drag_offset: Option<f32>,
) -> Option<(f32, f32)> {
    let thumb = lsp_log_scrollbar_thumb(
        track_start,
        track_len,
        viewport_len,
        content_len,
        current_scroll,
        scale,
    )?;
    crate::scroll::scrollbar_drag_target(
        pointer,
        track_start,
        track_len,
        thumb,
        (content_len - viewport_len).max(0.0),
        drag_offset,
    )
}

pub(crate) fn lsp_log_inner_size_by<F>(
    log_editor: &crate::editor::Editor,
    scale: f32,
    mut measure_line: F,
) -> (f32, f32)
where
    F: FnMut(&str, &str, usize, usize) -> f32,
{
    let mut visible_lines = 0usize;
    let mut max_width = 0.0f32;
    let mut physical_line = 0usize;
    let (first, second) = log_editor.text_parts();
    while physical_line < log_editor.line_offsets.len() {
        let start = log_editor.line_offsets[physical_line];
        let end = if physical_line + 1 < log_editor.line_offsets.len() {
            log_editor.line_offsets[physical_line + 1].saturating_sub(1)
        } else {
            log_editor.len()
        };
        max_width = max_width.max(measure_line(first, second, start, end));
        visible_lines += 1;
        if log_editor.folded_lines.contains(&physical_line)
            && let Some(&fold_end) = log_editor.foldable_lines.get(&physical_line)
        {
            physical_line = fold_end;
        }
        physical_line += 1;
    }
    (visible_lines as f32 * 16.0 * scale, max_width)
}

fn lsp_log_monospace_line_width(
    first: &str,
    second: &str,
    start: usize,
    end: usize,
    scale: f32,
) -> f32 {
    let first_len = first.len();
    let mut chars = 0usize;
    if start < first_len {
        chars += first[start..end.min(first_len)].chars().count();
    }
    if end > first_len {
        chars += second[start.saturating_sub(first_len)..end - first_len]
            .chars()
            .count();
    }
    chars as f32 * 7.5 * scale
}

pub(crate) fn lsp_server_logs_h_for_row(
    inner_total_h: f32,
    content_y: f32,
    content_h: f32,
    row_y: f32,
    s: f32,
) -> f32 {
    let panel_max = lsp_server_logs_h_for_content(inner_total_h, content_h, s);
    let max_fit = (content_y + content_h - row_y - 140.0 * s)
        .min(panel_max)
        .max(0.0);
    if max_fit < 68.0 * s {
        return 0.0;
    }
    let min_h = (84.0 * s).min(max_fit);
    (inner_total_h + 54.0 * s).clamp(min_h, max_fit)
}

fn build_noqa_comment(existing_noqa: Option<&str>, codes: &[String]) -> String {
    let prefix = if existing_noqa.is_some() {
        "# noqa"
    } else {
        "  # noqa"
    };

    if codes.is_empty() {
        return prefix.to_string();
    }

    let mut merged = if let Some(old_noqa) = existing_noqa {
        if let Some(colon) = old_noqa.find(": ") {
            old_noqa[colon + 2..]
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    for code in codes {
        if !merged.contains(code) {
            merged.push(code.clone());
        }
    }

    format!("{}: {}", prefix, merged.join(", "))
}

impl App {
    /// Возвращает (x, y, w, h) области LSP-панели или None если не открыта
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) fn lsp_panel_bounds(&self) -> Option<(f32, f32, f32, f32)> {
        if !self.ide_panel.is_open(crate::app::PanelId::LspServers) {
            return None;
        }
        let s = self.renderer.as_ref()?.scale_factor;
        let (x, y, w, h, _) =
            crate::app::mouse::app_panel_scroll_rect(self, crate::app::PanelId::LspServers, s);
        Some((x, y, w, h))
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
        let content_h = self
            .lsp_panel_bounds()
            .map(|(_, _, _, h)| h)
            .unwrap_or(952.0 * s);
        lsp_server_logs_h_for_content(inner_total_h, content_h, s)
    }

    pub(crate) fn lsp_server_inner_size(
        &self,
        info: &crate::lsp::LspServerInfo,
        s: f32,
    ) -> (f32, f32) {
        self.ide_panel
            .lsp_log_editors
            .get(info.name)
            .map_or((0.0, 0.0), |log_editor| {
                lsp_log_inner_size_by(log_editor, s, |first, second, start, end| {
                    lsp_log_monospace_line_width(first, second, start, end, s)
                })
            })
    }

    /// Открывает меню быстрых действий LSP для текущей строки
    #[cfg_attr(coverage_nightly, coverage(off))]
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
        let diags: Vec<crate::lsp::Diagnostic> =
            if let (Some(lsp), Some(path)) = (&self.lsp, &self.file_path) {
                lsp.diagnostics_for_line(path, cursor_line)
                    .into_iter()
                    .cloned()
                    .collect()
            } else {
                Vec::new()
            };

        // Вычисляем позицию меню (под курсором)
        let (cx, cy) = self.renderer.as_mut().unwrap().get_cursor_xy(&self.editor);
        let s = self.renderer.as_ref().unwrap().scale_factor;
        let menu_x = cx.max(self.renderer.as_ref().unwrap().left_padding);
        let editor_top_inset = crate::render_view::editor_content_top_inset(
            self.show_welcome,
            self.is_ide_mode,
            self.active_tab_is_database_query(),
            s,
        );
        let menu_y = lsp_actions_menu_screen_y(
            cy,
            self.scroll_y.current,
            self.renderer.as_ref().unwrap().line_height,
            editor_top_inset,
        );

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
                    items.push(crate::app::LspActionItem::CodeAction(
                        crate::lsp::CodeAction {
                            title: qf.title.clone(),
                            kind: Some("quickfix".to_string()),
                            edit: Some(crate::lsp::WorkspaceEdit { changes }),
                            code: d.code.as_ref().map(|code| code.to_string()),
                        },
                    ));
                }
            }

            // Сначала "Добавить # noqa: CODES" для конкретных кодов
            let codes: Vec<String> = diags
                .iter()
                .filter_map(|d| d.code.as_ref().map(|code| code.to_string()))
                .collect();
            if !codes.is_empty() {
                items.push(crate::app::LspActionItem::AddNoqa {
                    codes: codes.clone(),
                });
            }
            // Затем "Добавить # noqa" (всё отключить)
            items.push(crate::app::LspActionItem::AddNoqaAll);
        }

        if matches!(self.file_extension.as_str(), "py" | "pyi")
            && super::python_import_completion_allowed(&self.editor)
        {
            items.push(crate::app::LspActionItem::CompleteImports);
        }

        // Запрашиваем code actions от LSP
        let pending_id = if let (Some(lsp), Some(path)) = (&mut self.lsp, &self.file_path) {
            let ext = self.file_extension.clone();
            let (sl, sc) = crate::lsp::offset_to_lsp_pos(
                &self.editor.get_full_text(),
                self.editor.cursor,
                &self.editor.line_offsets,
            );
            lsp.request_code_actions(path, &ext, sl, sc, sl, sc, &diags, None)
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
                    self.apply_workspace_edit(&edit, false);
                }
            }
            crate::app::LspActionItem::AddNoqa { codes } => {
                self.insert_noqa_comment(menu.cursor_line, &codes);
            }
            crate::app::LspActionItem::AddNoqaAll => {
                self.insert_noqa_comment(menu.cursor_line, &[]);
            }
            crate::app::LspActionItem::FixAll => {
                if let (Some(lsp), Some(path)) = (&mut self.lsp, &self.file_path) {
                    if let Some(id) = lsp.request_fix_all(path, &self.file_extension) {
                        self.pending_fix_all_id = Some(id);
                    }
                }
            }
            crate::app::LspActionItem::OrganizeImports => {
                if let (Some(lsp), Some(path)) = (&mut self.lsp, &self.file_path) {
                    if let Some(id) = lsp.request_organize_imports(path, &self.file_extension) {
                        self.pending_fix_all_id = Some(id);
                    }
                }
            }
            crate::app::LspActionItem::CompleteImports => {
                self.request_ty_autocomplete(crate::app::AutocompleteMode::TyImports, None);
            }
        }

        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
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
            let new_noqa = build_noqa_comment(Some(old_noqa_str), codes);

            let (off, len, _) = self
                .editor
                .replace_range(noqa_byte_start, actual_end, &new_noqa);
            self.highlighter.shift_delete(off, len);
            self.highlighter
                .shift_insert(off, new_noqa.len(), Some(&new_noqa));
        } else {
            // Нет noqa — добавляем в конец строки
            let noqa = build_noqa_comment(None, codes);
            let (off, len, _) = self.editor.replace_range(actual_end, actual_end, &noqa);
            self.highlighter.shift_delete(off, len);
            self.highlighter.shift_insert(off, noqa.len(), Some(&noqa));
        }

        // Синхронизируем с LSP и подсветчиком
        if !self.editor.sync_edits.is_empty() {
            let edits = std::mem::take(&mut self.editor.sync_edits);
            self.shift_current_python_inlay_hints_for_edits(&edits);
            if self.is_ide_mode {
                if let (Some(lsp), Some(path)) = (&mut self.lsp, &self.file_path) {
                    let text = self.editor.get_full_text();
                    let ext = self.file_extension.clone();
                    let path = path.clone();
                    lsp.notify_change(
                        &path,
                        &ext,
                        &text,
                        crate::editor::lsp_document_version(self.editor.version),
                    );
                }
            }
            self.highlighter
                .apply_edits(self.editor.version, edits, None, None);
        }

        if let Some(window) = self.window.as_ref() {
            App::update_window_title(window, &self.base_title, self.editor.is_dirty());
        }
    }

    pub(crate) fn apply_workspace_edit(
        &mut self,
        edit: &crate::lsp::WorkspaceEdit,
        preserve_cursor: bool,
    ) {
        if let Some(path) = &self.file_path {
            if edit
                .changes
                .keys()
                .any(|changed_path| !crate::platform::paths_equal(changed_path, path))
            {
                // Никогда не применять только часть rename/quick-fix. Multi-file edits
                // будут подключены отдельным атомарным workspace transaction API.
                return;
            }
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
                    let start =
                        crate::lsp::lsp_pos_to_offset(&text, change.start_line, change.start_col);
                    let end = crate::lsp::lsp_pos_to_offset(&text, change.end_line, change.end_col);
                    ops.push((start, end, change.new_text.clone()));
                }

                let cursor_before = self.editor.cursor;
                let selection_before = self.editor.selection_anchor;

                for (start, end, new_text) in &ops {
                    if *start <= *end {
                        let (off, len, _) = self.editor.replace_range(*start, *end, new_text);
                        self.highlighter.shift_delete(off, len);
                        self.highlighter
                            .shift_insert(off, new_text.len(), Some(new_text));
                    }
                }

                if preserve_cursor {
                    let mut cursor_after = cursor_before;
                    let mut selection_after = selection_before;
                    for (start, end, new_text) in &ops {
                        let delta = new_text.len() as isize - (*end - *start) as isize;

                        if *start <= cursor_after {
                            if *end <= cursor_after {
                                cursor_after = ((cursor_after as isize) + delta).max(0) as usize;
                            } else {
                                // Курсор внутри измененного блока — пытаемся сохранить относительную позицию
                                let relative_offset = cursor_after - *start;
                                cursor_after =
                                    (*start + relative_offset).min(*start + new_text.len());
                            }
                        }

                        if let Some(mut sel_anchor) = selection_after {
                            if *start <= sel_anchor {
                                if *end <= sel_anchor {
                                    sel_anchor = ((sel_anchor as isize) + delta).max(0) as usize;
                                } else {
                                    let relative_offset = sel_anchor - *start;
                                    sel_anchor =
                                        (*start + relative_offset).min(*start + new_text.len());
                                }
                            }
                            selection_after = Some(sel_anchor);
                        }
                    }
                    self.editor.cursor = cursor_after;
                    self.editor.selection_anchor = selection_after;
                }

                if !self.editor.sync_edits.is_empty() {
                    let edits = std::mem::take(&mut self.editor.sync_edits);
                    self.shift_current_python_inlay_hints_for_edits(&edits);
                    if self.is_ide_mode {
                        if let (Some(lsp), Some(path)) = (&mut self.lsp, &self.file_path) {
                            let text = self.editor.get_full_text();
                            let ext = self.file_extension.clone();
                            let path = path.clone();
                            lsp.notify_change(
                                &path,
                                &ext,
                                &text,
                                crate::editor::lsp_document_version(self.editor.version),
                            );
                        }
                    }
                    self.highlighter
                        .apply_edits(self.editor.version, edits, None, None);
                }

                if let Some(window) = self.window.as_ref() {
                    App::update_window_title(window, &self.base_title, self.editor.is_dirty());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn build_noqa_comment_creates_plain_and_code_comments() {
        assert_eq!(build_noqa_comment(None, &[]), "  # noqa");
        assert_eq!(
            build_noqa_comment(None, &codes(&["F401"])),
            "  # noqa: F401"
        );
        assert_eq!(
            build_noqa_comment(None, &codes(&["F401", "E501"])),
            "  # noqa: F401, E501"
        );
    }

    #[test]
    fn build_noqa_comment_merges_existing_codes_without_duplicates() {
        assert_eq!(
            build_noqa_comment(Some("# noqa: F401, E501"), &codes(&["F401", "UP001"])),
            "# noqa: F401, E501, UP001"
        );
        assert_eq!(
            build_noqa_comment(Some("# noqa"), &codes(&["F401"])),
            "# noqa: F401"
        );
        assert_eq!(
            build_noqa_comment(Some("# noqa: "), &codes(&["F401"])),
            "# noqa: F401"
        );
        assert_eq!(build_noqa_comment(Some("# noqa: F401"), &[]), "# noqa");
    }
    #[test]
    fn lsp_log_scrollbar_drag_uses_the_rendered_track_origin() {
        let track_start = 107.0;
        let track_len = 186.0;
        let viewport_len = 200.0;
        let content_len = 600.0;
        let current = 150.0;
        let thumb = lsp_log_scrollbar_thumb(
            track_start,
            track_len,
            viewport_len,
            content_len,
            current,
            1.0,
        )
        .unwrap();
        let pointer_offset = thumb.len * 0.35;
        let pointer = thumb.start + pointer_offset;
        let (offset, target) = lsp_log_scrollbar_drag_target(
            pointer,
            track_start,
            track_len,
            viewport_len,
            content_len,
            current,
            1.0,
            None,
        )
        .unwrap();
        assert!((offset - pointer_offset).abs() < 0.001);
        assert!((target - current).abs() < 0.001);
    }

    #[test]
    fn lsp_log_inner_size_counts_unicode_characters_not_utf8_bytes() {
        let mut editor = crate::editor::Editor::new(32);
        editor.set_text_clean("λ中a\nsecond");
        let (height, width) = lsp_log_inner_size_by(&editor, 1.0, |first, second, start, end| {
            lsp_log_monospace_line_width(first, second, start, end, 1.0)
        });
        assert_eq!(height, 32.0);
        assert_eq!(width, 6.0 * 7.5);
    }

    #[test]
    fn lsp_action_menu_anchor_includes_editor_top_inset_once() {
        assert_eq!(lsp_actions_menu_screen_y(120.0, 30.0, 20.0, 44.0), 154.0);
    }
}
