use super::*;

fn sync_edit_line_range(
    edits: &[crate::highlighter::SyncEdit],
    line_offsets: &[usize],
    text_len: usize,
) -> (Option<usize>, Option<usize>) {
    let edit_start_byte = edits.first().map(|edit| match edit {
        crate::highlighter::SyncEdit::Insert { offset, .. } => *offset,
        crate::highlighter::SyncEdit::Delete { offset, .. } => *offset,
    });

    let edit_end_byte = edits.last().map(|edit| match edit {
        crate::highlighter::SyncEdit::Insert { offset, text } => offset + text.len(),
        crate::highlighter::SyncEdit::Delete { offset, .. } => *offset,
    });

    let (Some(sb), Some(eb)) = (edit_start_byte, edit_end_byte) else {
        return (None, None);
    };

    if line_offsets.is_empty() {
        return (Some(0), Some(text_len));
    }

    let sl = line_offsets.partition_point(|&x| x <= sb).saturating_sub(1);
    let el = line_offsets.partition_point(|&x| x <= eb).saturating_sub(1);

    let line_start_byte = Some(line_offsets[sl.min(line_offsets.len() - 1)]);
    let line_end_byte = if el + 1 < line_offsets.len() {
        Some(line_offsets[el + 1])
    } else {
        Some(text_len)
    };

    (line_start_byte, line_end_byte)
}

fn bounded_repeat_scroll_delta(delta_y: f32, line_height: f32) -> Option<f32> {
    (delta_y.abs() <= line_height * 2.0).then_some(delta_y)
}

fn line_for_offset(line_offsets: &[usize], offset: usize) -> usize {
    line_offsets
        .partition_point(|&line| line <= offset)
        .saturating_sub(1)
}

fn backspace_crossed_line(
    before_lines: &[usize],
    before_cursor: usize,
    after_lines: &[usize],
    after_cursor: usize,
) -> bool {
    line_for_offset(before_lines, before_cursor) != line_for_offset(after_lines, after_cursor)
}

fn key_text_for_editor_insert<'a>(
    physical_key: winit::keyboard::PhysicalKey,
    event_text: Option<&'a str>,
    logical_text: Option<&'a str>,
    shift: bool,
) -> Option<&'a str> {
    if let Some(text) = event_text {
        return Some(text);
    }
    if let Some(text) = logical_text {
        return Some(text);
    }
    match physical_key {
        PhysicalKey::Code(KeyCode::Period) if !shift => Some("."),
        PhysicalKey::Code(KeyCode::NumpadDecimal) if !shift => Some("."),
        _ => None,
    }
}

pub(crate) fn paired_editor_insert_text(text: &str) -> (&str, bool) {
    match text {
        "(" => ("()", true),
        "[" => ("[]", true),
        "{" => ("{}", true),
        "'" => ("''", true),
        "\"" => ("\"\"", true),
        "`" => ("``", true),
        _ => (text, false),
    }
}

impl App {
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn handle_editor_keyboard_input(
        &mut self,
        event_loop: &ActiveEventLoop,
        key_event: KeyEvent,
    ) {
        let ctrl = self.modifiers.control_key() || self.modifiers.super_key();
        let shift = self.modifiers.shift_key();
        let physical_key = key_event.physical_key;

        if self.show_welcome {
            match physical_key {
                PhysicalKey::Code(KeyCode::KeyO) if ctrl => {
                    self.trigger_file_picker();
                }
                PhysicalKey::Code(KeyCode::KeyQ) if ctrl => {
                    let w = self.window.as_ref().unwrap();
                    let maximized = w.is_maximized();
                    let (width, height) = if maximized {
                        (self.window_width, self.window_height)
                    } else {
                        let scale = w.scale_factor();
                        let size = w.inner_size().to_logical::<f64>(scale);
                        (size.width, size.height)
                    };
                    crate::save_config(&crate::Config {
                        window_width: width,
                        window_height: height,
                        maximized,
                        ide_workspaces: self.ide_workspaces.clone(),
                        ide_ignore_patterns: self.ide_ignore_patterns.clone(),
                        enable_telemetry: crate::render_view::TELEMETRY_ENABLED
                            .load(std::sync::atomic::Ordering::Relaxed),
                    });
                    if self.is_ide_mode {
                        crate::save_panel_state(&self.ide_panel);
                    }
                    event_loop.exit();
                }
                _ => {}
            }
            return;
        }

        if self.autocomplete_active {
            match self.handle_active_autocomplete_key(physical_key, ctrl) {
                AutocompletePopupKeyResult::Consumed => return,
                AutocompletePopupKeyResult::Continue | AutocompletePopupKeyResult::NotHandled => {}
            }
        }
        if self.mark_pending_autocomplete_apply_for_key(physical_key) {
            return;
        }

        // Alt+Enter — меню быстрых действий LSP
        if self.modifiers.alt_key() {
            if let PhysicalKey::Code(KeyCode::Enter) = physical_key {
                self.open_lsp_actions_menu();
                return;
            }
        }

        // Навигация в открытом меню LSP
        if self.lsp_actions_menu.is_some() {
            match physical_key {
                PhysicalKey::Code(KeyCode::Escape) => {
                    self.lsp_actions_menu = None;
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
                PhysicalKey::Code(KeyCode::ArrowUp) => {
                    if let Some(menu) = &mut self.lsp_actions_menu {
                        if menu.selected > 0 {
                            menu.selected -= 1;
                        } else {
                            menu.selected = menu.items.len().saturating_sub(1);
                        }
                    }
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
                PhysicalKey::Code(KeyCode::ArrowDown) => {
                    if let Some(menu) = &mut self.lsp_actions_menu {
                        if !menu.items.is_empty() {
                            menu.selected = (menu.selected + 1) % menu.items.len();
                        }
                    }
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
                PhysicalKey::Code(KeyCode::Enter) | PhysicalKey::Code(KeyCode::NumpadEnter) => {
                    self.apply_selected_lsp_action();
                    return;
                }
                _ => {}
            }
        }

        let mut cursor_moved = false;
        let mut is_edit = false;
        let mut should_trigger_autocomplete = false;
        let mut should_notify_lsp = true;
        let mut ty_completion_trigger: Option<&'static str> = None;
        let mut force_close_autocomplete = false;
        let is_git_diff_tab = self.active_tab_is_git_diff();

        if is_git_diff_tab {
            let text_insert = !ctrl
                && !self.modifiers.alt_key()
                && !self.modifiers.super_key()
                && key_text_for_editor_insert(
                    physical_key,
                    key_event.text.as_deref(),
                    key_event.logical_key.to_text(),
                    shift,
                )
                .is_some();
            let edit_key = matches!(
                physical_key,
                PhysicalKey::Code(
                    KeyCode::Enter
                        | KeyCode::NumpadEnter
                        | KeyCode::Tab
                        | KeyCode::Space
                        | KeyCode::Backspace
                        | KeyCode::Delete
                )
            ) || (ctrl
                && matches!(
                    physical_key,
                    PhysicalKey::Code(KeyCode::KeyX | KeyCode::KeyV)
                ));
            if text_insert || edit_key {
                self.show_readonly_diff_notice();
                return;
            }
        }

        let old_cursor_y = self
            .renderer
            .as_mut()
            .unwrap()
            .get_cursor_xy(&self.editor)
            .1;

        match physical_key {
            PhysicalKey::Code(KeyCode::KeyQ) if ctrl => {
                if self.is_ide_mode {
                    // TODO: Спросить о сохранении несохраненных вкладок
                    // Закрываем все вкладки и возвращаемся на Welcome Screen

                    // 1. Уведомить LSP о закрытии всех файлов
                    self.sync_active_tab(); // Синхронизируем последнюю активную вкладку
                    if let Some(lsp) = &mut self.lsp {
                        for tab in &self.tabs {
                            if let Some(p) = &tab.file_path {
                                lsp.notify_close(p, &tab.file_extension);
                            }
                        }
                    }

                    // 2. Очистить все вкладки и сбросить состояние редактора до "пустого"
                    self.tabs.clear();
                    self.active_tab = 0;
                    self.file_path = None;
                    self.file_key = None;
                    self.text_file_format = crate::platform::TextFileFormat::default();
                    self.base_title = "Добро пожаловать".to_string();
                    self.editor = Editor::new(8192);
                    self.editor.set_original_text();
                    self.highlighter
                        .reset(self.editor.version, "".to_string(), "".to_string(), 0);
                    self.show_welcome = true;
                    self.autocomplete_active = false;
                    self.scroll_y.stop_anim();
                    self.scroll_x.stop_anim();
                    self.save_tabs_state();
                } else {
                    if self.editor.is_dirty() {
                        self.show_action_dialog(event_loop, PendingAction::CloseFile);
                    } else {
                        self.close_current_file();
                    }
                }
                return;
            }
            PhysicalKey::Code(KeyCode::F1) => {
                self.show_settings = !self.show_settings;
                self.is_dragging = false;
                return;
            }
            PhysicalKey::Code(KeyCode::KeyF) if ctrl => {
                self.show_search = true;
                self.search_focused = true;
                self.search_editor.select_all();
                self.search_current_idx = None;
                self.update_search();
                self.jump_to_search_result();

                self.window.as_ref().unwrap().request_redraw();
                return;
            }
            PhysicalKey::Code(KeyCode::KeyW) if ctrl => {
                let text = self.editor.get_full_text();
                if let Some((start, end)) = crate::highlighter::ast_select_expand_range(
                    &text,
                    &self.file_extension,
                    self.editor.cursor,
                    self.editor.selection_anchor,
                ) {
                    self.editor.selection_anchor = Some(start);
                    self.editor.cursor = end;
                } else {
                    self.editor.select_expand();
                }
                self.close_autocomplete();
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::Escape) => {
                if self.show_search {
                    self.show_search = false;
                    self.search_focused = false;
                    self.search_results.clear();
                    self.search_current_idx = None;
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
            }
            PhysicalKey::Code(KeyCode::KeyS) if ctrl => {
                if self.save_current_file() {
                    App::update_window_title(
                        self.window.as_ref().unwrap(),
                        &self.base_title,
                        self.editor.is_dirty(),
                    );
                }
            }
            PhysicalKey::Code(KeyCode::KeyO) if ctrl => {
                if self.editor.is_dirty() {
                    self.show_action_dialog(event_loop, PendingAction::OpenFile);
                } else {
                    self.trigger_file_picker();
                }
            }
            PhysicalKey::Code(KeyCode::KeyZ) if ctrl => {
                if let Some(delta) = self.editor.undo() {
                    if !is_git_diff_tab {
                        match delta {
                            crate::editor::UndoRedoDelta::Insert(offset, len, text) => {
                                self.highlighter.shift_insert(offset, len, Some(&text));
                            }
                            crate::editor::UndoRedoDelta::Delete(offset, len) => {
                                self.highlighter.shift_delete(offset, len);
                            }
                            crate::editor::UndoRedoDelta::Replace(
                                offset,
                                del_len,
                                old_text,
                                _new_text,
                            ) => {
                                self.highlighter.shift_delete(offset, del_len);
                                self.highlighter.shift_insert(
                                    offset,
                                    old_text.len(),
                                    Some(&old_text),
                                );
                            }
                        }
                    }
                    cursor_moved = true;
                    is_edit = true;
                }
            }
            PhysicalKey::Code(KeyCode::KeyY) if ctrl => {
                if let Some(delta) = self.editor.redo() {
                    if !is_git_diff_tab {
                        match delta {
                            crate::editor::UndoRedoDelta::Insert(offset, len, text) => {
                                self.highlighter.shift_insert(offset, len, Some(&text));
                            }
                            crate::editor::UndoRedoDelta::Delete(offset, len) => {
                                self.highlighter.shift_delete(offset, len);
                            }
                            crate::editor::UndoRedoDelta::Replace(
                                offset,
                                del_len,
                                new_text,
                                _old_text,
                            ) => {
                                self.highlighter.shift_delete(offset, del_len);
                                self.highlighter.shift_insert(
                                    offset,
                                    new_text.len(),
                                    Some(&new_text),
                                );
                            }
                        }
                    }
                    cursor_moved = true;
                    is_edit = true;
                }
            }
            PhysicalKey::Code(KeyCode::ArrowLeft) => {
                if ctrl {
                    self.editor.move_word_left(shift);
                } else {
                    self.editor.move_left(shift);
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::ArrowRight) => {
                if ctrl {
                    self.editor.move_word_right(shift);
                } else {
                    self.editor.move_right(shift);
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::ArrowUp) => {
                self.editor.move_up(self.renderer.as_mut().unwrap(), shift);
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::ArrowDown) => {
                self.editor
                    .move_down(self.renderer.as_mut().unwrap(), shift);
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::Home) => {
                if ctrl {
                    self.editor.move_start_of_file(shift);
                } else {
                    self.editor.move_home(shift);
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::End) => {
                if ctrl {
                    self.editor.move_end_of_file(shift);
                } else {
                    self.editor.move_end(shift);
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::PageUp) => {
                let step = self.window.as_ref().unwrap().inner_size().height as f32 * 0.8;
                self.scroll_y.scroll_by(-step);
                self.editor
                    .move_page_up(self.renderer.as_mut().unwrap(), shift, step);
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::PageDown) => {
                let step = self.window.as_ref().unwrap().inner_size().height as f32 * 0.8;
                self.scroll_y.scroll_by(step);
                self.editor
                    .move_page_down(self.renderer.as_mut().unwrap(), shift, step);
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::Backspace) if ctrl => {
                let before_cursor = self.editor.cursor;
                let before_lines = self.editor.line_offsets.clone();
                if let Some((offset, len)) = self.editor.delete_word_backward() {
                    self.highlighter.shift_delete(offset, len);
                    is_edit = true;
                    force_close_autocomplete = backspace_crossed_line(
                        &before_lines,
                        before_cursor,
                        &self.editor.line_offsets,
                        self.editor.cursor,
                    );
                    if self.autocomplete_active && !force_close_autocomplete {
                        should_trigger_autocomplete = true;
                        if self.autocomplete_mode != AutocompleteMode::TreeSitter {
                            ty_completion_trigger = None;
                        }
                    }
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::Delete) if ctrl => {
                if let Some((offset, len)) = self.editor.delete_word_forward() {
                    self.highlighter.shift_delete(offset, len);
                    is_edit = true;
                    if self.autocomplete_active {
                        should_trigger_autocomplete = true;
                        if self.autocomplete_mode != AutocompleteMode::TreeSitter {
                            ty_completion_trigger = None;
                        }
                    }
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::Backspace) => {
                let before_cursor = self.editor.cursor;
                let before_lines = self.editor.line_offsets.clone();
                if let Some((offset, len)) = self.editor.backspace() {
                    self.highlighter.shift_delete(offset, len);
                    is_edit = true;
                    force_close_autocomplete = backspace_crossed_line(
                        &before_lines,
                        before_cursor,
                        &self.editor.line_offsets,
                        self.editor.cursor,
                    );
                    if self.autocomplete_active && !force_close_autocomplete {
                        should_trigger_autocomplete = true;
                        if self.autocomplete_mode != AutocompleteMode::TreeSitter {
                            ty_completion_trigger = None;
                        }
                    }
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::Delete) => {
                if let Some((offset, len)) = self.editor.delete_forward() {
                    self.highlighter.shift_delete(offset, len);
                    is_edit = true;
                    if self.autocomplete_active {
                        should_trigger_autocomplete = true;
                        if self.autocomplete_mode != AutocompleteMode::TreeSitter {
                            ty_completion_trigger = None;
                        }
                    }
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::Enter) => {
                let indent = self.editor.get_auto_indent();
                let insert_text = format!("\n{}", indent);
                let (del_info, ins_len) = self.editor.insert_str(&insert_text);
                if let Some((offset, len)) = del_info {
                    self.highlighter.shift_delete(offset, len);
                }
                self.highlighter.shift_insert(
                    self.editor.cursor - ins_len,
                    ins_len,
                    Some(&insert_text),
                );
                cursor_moved = true;
                is_edit = true;
            }
            PhysicalKey::Code(KeyCode::Tab) => {
                let (del_info, ins_len) = self.editor.insert_str("    ");
                if let Some((offset, len)) = del_info {
                    self.highlighter.shift_delete(offset, len);
                }
                self.highlighter
                    .shift_insert(self.editor.cursor - ins_len, ins_len, Some("    "));
                cursor_moved = true;
                is_edit = true;
            }
            PhysicalKey::Code(KeyCode::Space) => {
                let (del_info, ins_len) = self.editor.insert_str(" ");
                if let Some((offset, len)) = del_info {
                    self.highlighter.shift_delete(offset, len);
                }
                self.highlighter
                    .shift_insert(self.editor.cursor - ins_len, ins_len, Some(" "));
                cursor_moved = true;
                is_edit = true;
                should_notify_lsp = false;
            }
            PhysicalKey::Code(KeyCode::Digit4) if ctrl => {
                self.close_tab_at(self.active_tab);
                return;
            }
            PhysicalKey::Code(KeyCode::KeyC) if ctrl => {
                let mut copied = false;
                if let Some(text) = self.selected_autocomplete_detail_text() {
                    self.set_clipboard_text(text);
                    self.autocomplete_detail_selection_anchor = None;
                    self.autocomplete_detail_selection_cursor = None;
                    self.autocomplete_detail_selecting = false;
                    copied = true;
                }
                if !copied && self.copy_hover_popup_selection_or_diagnostic() {
                    copied = true;
                }
                if !copied {
                    let graph_copy = self
                        .renderer
                        .as_ref()
                        .and_then(|renderer| renderer.selected_git_graph_tooltip_text());
                    if let Some(text) = graph_copy {
                        self.set_clipboard_text(text);
                        if let Some(renderer) = self.renderer.as_mut() {
                            renderer.git_graph_tooltip_selection_anchor = None;
                            renderer.git_graph_tooltip_selection_cursor = None;
                            renderer.git_graph_tooltip_selecting = false;
                        }
                        copied = true;
                    }
                }
                if !copied {
                    if let Some(text) = self.editor.get_selection() {
                        self.set_clipboard_text(text);
                    }
                }
                if copied {
                    self.window.as_ref().unwrap().request_redraw();
                }
            }
            PhysicalKey::Code(KeyCode::KeyX) if ctrl => {
                if let Some(text) = self.editor.get_selection() {
                    self.set_clipboard_text(text);
                    if let Some((offset, len)) = self.editor.delete_selection() {
                        self.highlighter.shift_delete(offset, len);
                        is_edit = true;
                    }
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::KeyV) if ctrl => {
                if let Some(text) = self.get_clipboard_text() {
                    let (del_info, ins_len) = self.editor.insert_str(&text);
                    if let Some((offset, len)) = del_info {
                        self.highlighter.shift_delete(offset, len);
                    }
                    self.highlighter.shift_insert(
                        self.editor.cursor - ins_len,
                        ins_len,
                        Some(&text),
                    );
                    is_edit = true;
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::KeyA) if ctrl => {
                self.editor.select_all();
                self.close_autocomplete();
            }
            _ => {
                if !ctrl && !self.modifiers.alt_key() && !self.modifiers.super_key() {
                    if let Some(txt) = key_text_for_editor_insert(
                        physical_key,
                        key_event.text.as_deref(),
                        key_event.logical_key.to_text(),
                        shift,
                    ) {
                        if txt == "."
                            && self.autocomplete_active
                            && !self.autocomplete_options.is_empty()
                        {
                            self.apply_autocomplete();
                        }

                        let (insert_txt, move_inside_pair) = paired_editor_insert_text(txt);

                        // We only log if it's a simple character insert, not an autofold/autoclose or space/enter, although the prompt said "что печатаются в редакторе".
                        // Let's log any printable text that is typed.
                        if crate::render_view::TELEMETRY_ENABLED
                            .load(std::sync::atomic::Ordering::Relaxed)
                        {
                            self.pending_key_log = Some(crate::app::KeyLog {
                                key: txt.to_string(),
                                t0: std::time::Instant::now(),
                                t_highlight: None,
                                t_render: None,
                            });
                        }

                        let (del_info, ins_len) = self.editor.insert_str(insert_txt);
                        if let Some((offset, len)) = del_info {
                            self.highlighter.shift_delete(offset, len);
                        }
                        self.highlighter.shift_insert(
                            self.editor.cursor - ins_len,
                            ins_len,
                            Some(insert_txt),
                        );
                        if move_inside_pair {
                            self.editor.move_left(false);
                        }
                        cursor_moved = true;
                        is_edit = true;

                        if txt == "." {
                            should_trigger_autocomplete = true;
                            ty_completion_trigger = Some(".");
                        } else if txt.chars().all(|c| c.is_alphanumeric() || c == '_') {
                            should_trigger_autocomplete = true;
                        }
                        if txt == "=" {
                            should_notify_lsp = false;
                        }
                    }
                }
            }
        }

        if cursor_moved && !is_edit {
            self.close_autocomplete();
            self.lsp_actions_menu = None;
        }

        if is_edit {
            if is_git_diff_tab {
                let is_undo = matches!(physical_key, PhysicalKey::Code(KeyCode::KeyZ)) && ctrl;
                self.rebuild_active_git_diff_from_editor_after_history(is_undo);
                self.editor.sync_edits.clear();
                if self.show_search && !self.search_editor.get_full_text().is_empty() {
                    self.update_search();
                } else {
                    self.search_results.clear();
                }
                App::update_window_title(
                    self.window.as_ref().unwrap(),
                    &self.base_title,
                    self.editor.is_dirty(),
                );
                self.window.as_ref().unwrap().request_redraw();
                return;
            }
            self.lsp_actions_menu = None;
            self.is_highlighted_once = true;
            self.is_highlight_complete = false;
            if force_close_autocomplete {
                self.close_autocomplete();
            } else if should_trigger_autocomplete {
                if let Some(trigger) = ty_completion_trigger {
                    self.request_ty_autocomplete(AutocompleteMode::TyContext, Some(trigger));
                } else if self.autocomplete_active
                    && self.autocomplete_mode == AutocompleteMode::TyImports
                {
                    self.request_ty_autocomplete(AutocompleteMode::TyImports, None);
                } else if cursor_after_python_member_dot(&self.editor)
                    || cursor_inside_python_call_parens(&self.editor)
                {
                    self.request_ty_autocomplete(AutocompleteMode::TyContext, None);
                } else {
                    self.update_autocomplete();
                }
            } else {
                self.close_autocomplete();
            }

            App::update_window_title(
                self.window.as_ref().unwrap(),
                &self.base_title,
                self.editor.is_dirty(),
            );
            if self.show_search && !self.search_editor.get_full_text().is_empty() {
                self.update_search();
            } else {
                self.search_results.clear();
            }

            if !self.editor.sync_edits.is_empty() {
                let edits = std::mem::take(&mut self.editor.sync_edits);
                self.shift_current_python_inlay_hints_for_edits(&edits);
                // LSP can skip low-value keystrokes; highlighter cannot, its replica must stay exact.
                if should_notify_lsp && self.is_ide_mode {
                    if let (Some(lsp), Some(path)) = (&mut self.lsp, &self.file_path) {
                        let text = self.editor.get_full_text();
                        let ext = self.file_extension.clone();
                        let path = path.clone();
                        lsp.notify_change(&path, &ext, &text, self.editor.version as i32);
                    }
                }
                let (line_start_byte, line_end_byte) =
                    sync_edit_line_range(&edits, &self.editor.line_offsets, self.editor.len());
                let (invalidate_start_byte, invalidate_end_byte) =
                    crate::highlighter::sync_edit_invalidation_byte_range(&edits);

                self.highlighter.apply_edits(
                    self.editor.version,
                    edits,
                    line_start_byte,
                    line_end_byte,
                );
                self.highlighter.sync_highlight_after_edit(
                    self.editor.version,
                    line_start_byte,
                    line_end_byte,
                    invalidate_start_byte,
                    invalidate_end_byte,
                    std::time::Duration::from_millis(1),
                );
            }
            if should_notify_lsp {
                self.last_sent_version = self.editor.version;
            }

            let highlight_updated = self.highlighter.poll(self.editor.version);

            if highlight_updated {
                let autofold_threshold = match self.file_extension.as_str() {
                    "py" | "pyi" | "rs" | "dart" => 1,
                    _ => 2,
                };
                let should_autofold_initial = false;
                self.editor.foldable_lines.clear();
                self.editor.foldable_ranges_bytes.clear();
                for &(start_b, end_b, is_autofold, is_sticky) in &self.highlighter.foldable_ranges {
                    self.editor
                        .foldable_ranges_bytes
                        .push((start_b, end_b, is_sticky));
                    let sl = self
                        .editor
                        .line_offsets
                        .partition_point(|&x| x <= start_b)
                        .saturating_sub(1);
                    let el = self
                        .editor
                        .line_offsets
                        .partition_point(|&x| x <= end_b)
                        .saturating_sub(1);
                    if el > sl {
                        self.editor.foldable_lines.insert(sl, el);
                        if is_autofold && el - sl >= autofold_threshold && should_autofold_initial {
                            self.editor.folded_lines.insert(sl);
                            self.editor
                                .folded_start_bytes
                                .insert(self.editor.line_offsets[sl]);
                        }
                    }
                }

                self.is_highlighted_once = true;
                self.is_highlight_complete = self.highlighter.is_complete;
                if self.autocomplete_active {
                    self.update_autocomplete();
                }
            }

            if let Some(log) = &mut self.pending_key_log {
                log.t_highlight = Some(std::time::Instant::now());
            }
        }

        if cursor_moved {
            let is_arrow = matches!(
                physical_key,
                PhysicalKey::Code(
                    KeyCode::ArrowUp
                        | KeyCode::ArrowDown
                        | KeyCode::ArrowLeft
                        | KeyCode::ArrowRight
                )
            );
            let is_page = matches!(
                physical_key,
                PhysicalKey::Code(KeyCode::PageUp | KeyCode::PageDown)
            );

            if is_arrow {
                self.scroll_y.anim_speed = 10.0;
                self.scroll_x.anim_speed = 10.0;
            } else if is_page {
                self.scroll_y.anim_speed = 7.0;
                self.scroll_x.anim_speed = 7.0;
            } else {
                self.scroll_y.anim_speed = 25.0;
                self.scroll_x.anim_speed = 25.0;
            }

            let wh_width = self.window.as_ref().unwrap().inner_size().width as f32;
            let wh_height = self.window.as_ref().unwrap().inner_size().height as f32;

            let is_enter_or_backspace = matches!(
                physical_key,
                PhysicalKey::Code(KeyCode::Enter | KeyCode::Backspace | KeyCode::Delete)
            );

            if is_enter_or_backspace && key_event.repeat {
                let new_cursor_y = self
                    .renderer
                    .as_mut()
                    .unwrap()
                    .get_cursor_xy(&self.editor)
                    .1;
                let delta_y = new_cursor_y - old_cursor_y;
                if let Some(delta_y) = bounded_repeat_scroll_delta(
                    delta_y,
                    self.renderer.as_ref().unwrap().line_height,
                ) {
                    self.scroll_y.target += delta_y;
                    self.scroll_y.current += delta_y;
                }
                let max_scroll = self
                    .renderer
                    .as_mut()
                    .unwrap()
                    .get_max_scroll(&self.editor, wh_height);
                self.scroll_y.clamp_target(0.0, max_scroll);
                self.scroll_y.target = self.scroll_y.target.round();
                self.scroll_y.clamp_current(0.0, max_scroll);
            } else {
                let old_target_y = self.scroll_y.target;
                let old_target_x = self.scroll_x.target;

                let tab_bar_h = if self.show_welcome || !self.is_ide_mode {
                    0.0
                } else {
                    38.0 * self.renderer.as_ref().unwrap().scale_factor
                };
                App::ensure_cursor_visible(
                    &mut self.scroll_y.target,
                    &mut self.scroll_x.target,
                    &self.editor,
                    self.renderer.as_mut().unwrap(),
                    wh_width,
                    wh_height,
                    tab_bar_h,
                );

                if key_event.repeat && !is_arrow && !is_page {
                    self.scroll_y.current += self.scroll_y.target - old_target_y;
                    self.scroll_x.current += self.scroll_x.target - old_target_x;
                }
            }
        }

        self.last_action = Instant::now();
        self.window.as_ref().unwrap().request_redraw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AutocompleteKeyAction, autocomplete_key_action, autocomplete_next_index};

    #[test]
    fn autocomplete_key_action_maps_navigation_and_apply_keys() {
        assert_eq!(
            autocomplete_key_action(PhysicalKey::Code(KeyCode::Escape)),
            AutocompleteKeyAction::DismissAndConsume
        );
        assert_eq!(
            autocomplete_key_action(PhysicalKey::Code(KeyCode::ArrowLeft)),
            AutocompleteKeyAction::DismissAndContinue
        );
        assert_eq!(
            autocomplete_key_action(PhysicalKey::Code(KeyCode::ArrowRight)),
            AutocompleteKeyAction::DismissAndContinue
        );
        assert_eq!(
            autocomplete_key_action(PhysicalKey::Code(KeyCode::ArrowDown)),
            AutocompleteKeyAction::MoveDown
        );
        assert_eq!(
            autocomplete_key_action(PhysicalKey::Code(KeyCode::ArrowUp)),
            AutocompleteKeyAction::MoveUp
        );
        assert_eq!(
            autocomplete_key_action(PhysicalKey::Code(KeyCode::Enter)),
            AutocompleteKeyAction::Apply
        );
        assert_eq!(
            autocomplete_key_action(PhysicalKey::Code(KeyCode::Tab)),
            AutocompleteKeyAction::Apply
        );
        assert_eq!(
            autocomplete_key_action(PhysicalKey::Code(KeyCode::KeyA)),
            AutocompleteKeyAction::None
        );
    }

    #[test]
    fn autocomplete_ctrl_navigation_jumps_five_items() {
        assert_eq!(autocomplete_next_index(0, 10, false, false), 1);
        assert_eq!(autocomplete_next_index(0, 10, false, true), 5);
        assert_eq!(autocomplete_next_index(2, 10, true, false), 1);
        assert_eq!(autocomplete_next_index(7, 10, true, true), 2);
        assert_eq!(autocomplete_next_index(0, 0, false, true), 0);
    }

    #[test]
    fn autocomplete_ctrl_navigation_stops_at_edge_before_wrapping() {
        assert_eq!(autocomplete_next_index(2, 10, true, true), 0);
        assert_eq!(autocomplete_next_index(3, 10, true, true), 0);
        assert_eq!(autocomplete_next_index(4, 10, true, true), 0);
        assert_eq!(autocomplete_next_index(0, 10, true, true), 9);

        assert_eq!(autocomplete_next_index(5, 10, false, true), 9);
        assert_eq!(autocomplete_next_index(6, 10, false, true), 9);
        assert_eq!(autocomplete_next_index(7, 10, false, true), 9);
        assert_eq!(autocomplete_next_index(9, 10, false, true), 0);
    }

    #[test]
    fn backspace_cross_line_detects_only_line_changes() {
        assert!(!backspace_crossed_line(&[0, 4, 8], 5, &[0, 4, 7], 4));
        assert!(backspace_crossed_line(&[0, 4, 8], 4, &[0, 7], 3));
    }

    #[test]
    fn sync_edit_line_range_covers_insert_delete_empty_and_last_line() {
        let lines = vec![0, 6, 12, 20];

        assert_eq!(sync_edit_line_range(&[], &lines, 24), (None, None));

        let edits = vec![crate::highlighter::SyncEdit::Insert {
            offset: 7,
            text: "abc".to_string(),
        }];
        assert_eq!(
            sync_edit_line_range(&edits, &lines, 24),
            (Some(6), Some(12))
        );

        let edits = vec![crate::highlighter::SyncEdit::Delete { offset: 18, len: 3 }];
        assert_eq!(
            sync_edit_line_range(&edits, &lines, 24),
            (Some(12), Some(20))
        );

        let edits = vec![
            crate::highlighter::SyncEdit::Insert {
                offset: 2,
                text: "x".to_string(),
            },
            crate::highlighter::SyncEdit::Delete { offset: 18, len: 2 },
        ];
        assert_eq!(
            sync_edit_line_range(&edits, &lines, 24),
            (Some(0), Some(20))
        );

        let edits = vec![
            crate::highlighter::SyncEdit::Insert {
                offset: 7,
                text: " ".to_string(),
            },
            crate::highlighter::SyncEdit::Delete { offset: 7, len: 1 },
        ];
        assert_eq!(
            sync_edit_line_range(&edits, &lines, 24),
            (Some(6), Some(12))
        );

        let edits = vec![crate::highlighter::SyncEdit::Insert {
            offset: 3,
            text: "x".to_string(),
        }];
        assert_eq!(sync_edit_line_range(&edits, &[], 9), (Some(0), Some(9)));
    }

    #[test]
    fn bounded_repeat_scroll_delta_ignores_stale_large_focus_jump() {
        assert_eq!(bounded_repeat_scroll_delta(22.0, 12.0), Some(22.0));
        assert_eq!(bounded_repeat_scroll_delta(-25.0, 12.0), None);
    }

    #[test]
    fn period_key_falls_back_to_dot_text_on_press() {
        assert_eq!(
            key_text_for_editor_insert(PhysicalKey::Code(KeyCode::Period), None, None, false),
            Some(".")
        );
        assert_eq!(
            key_text_for_editor_insert(PhysicalKey::Code(KeyCode::Period), Some("."), None, false),
            Some(".")
        );
        assert_eq!(
            key_text_for_editor_insert(
                PhysicalKey::Code(KeyCode::NumpadDecimal),
                None,
                None,
                false
            ),
            Some(".")
        );
        assert_eq!(
            key_text_for_editor_insert(PhysicalKey::Code(KeyCode::Period), None, Some(">"), true),
            Some(">")
        );
        assert_eq!(
            key_text_for_editor_insert(PhysicalKey::Code(KeyCode::Period), None, None, true),
            None
        );
    }

    #[test]
    fn paired_editor_insert_text_reuses_file_editor_pairs() {
        assert_eq!(paired_editor_insert_text("("), ("()", true));
        assert_eq!(paired_editor_insert_text("["), ("[]", true));
        assert_eq!(paired_editor_insert_text("{"), ("{}", true));
        assert_eq!(paired_editor_insert_text("'"), ("''", true));
        assert_eq!(paired_editor_insert_text("\""), ("\"\"", true));
        assert_eq!(paired_editor_insert_text("x"), ("x", false));
    }
}
