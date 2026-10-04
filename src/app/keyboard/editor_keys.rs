use super::*;

fn sync_edit_line_range(
    edits: &[crate::highlighter::SyncEdit],
    line_offsets: &[usize],
    text_len: usize,
) -> (Option<usize>, Option<usize>) {
    let (edit_start_byte, edit_end_byte) = edits.iter().fold(
        (None, None),
        |(start, end), edit| {
            let (edit_start, edit_end) = match edit {
                crate::highlighter::SyncEdit::Insert { offset, text } => {
                    (*offset, offset + text.len())
                }
                crate::highlighter::SyncEdit::Delete { offset, .. } => (*offset, *offset),
            };
            (
                Some(start.map_or(edit_start, |current: usize| current.min(edit_start))),
                Some(end.map_or(edit_end, |current: usize| current.max(edit_end))),
            )
        },
    );

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
        return (!text.bytes().any(|byte| (byte < 0x20 && byte != b'\t') || byte == 0x7f)).then_some(text);
    }
    if let Some(text) = logical_text {
        return (!text.bytes().any(|byte| (byte < 0x20 && byte != b'\t') || byte == 0x7f)).then_some(text);
    }
    match physical_key {
        PhysicalKey::Code(KeyCode::Period) if !shift => Some("."),
        PhysicalKey::Code(KeyCode::NumpadDecimal) if !shift => Some("."),
        _ => None,
    }
}

/// Keys a PDF tab still lets into the editor key match: exactly its application-level arms
/// (settings F1, close-all Ctrl+Q, find Ctrl+F, open file Ctrl+O, close tab Ctrl+4, Escape
/// closing the search panel). Every other arm there edits or moves the hidden text.
fn pdf_tab_key_reaches_editor(chord: Option<crate::keymap::Chord>, keymap: &crate::keymap::Keymap) -> bool {
    let Some(chord) = chord else {
        return false;
    };
    let allowed = [
        crate::keymap::Command::SettingsToggle,
        crate::keymap::Command::TabsCloseAll,
        crate::keymap::Command::SearchEditorOpen,
        crate::keymap::Command::FileOpen,
        crate::keymap::Command::TabsClose,
    ];
    let reserved_escape = chord.key == KeyCode::Escape
        && crate::keymap::is_reserved(crate::platform::CURRENT_PLATFORM, chord);
    (reserved_escape || allowed.iter().any(|command| keymap.hit(*command, chord)))
        && !crate::keymap::COMMANDS.iter().any(|info| {
            info.context == crate::keymap::KeyContext::Editor
                && !allowed.contains(&info.command)
                && keymap.hit(info.command, chord)
        })
}

fn editor_line_comment_marker(
    file_extension: &str,
    physical_key: PhysicalKey,
    primary: bool,
) -> Option<&'static str> {
    if !primary || physical_key != PhysicalKey::Code(KeyCode::Slash) {
        return None;
    }
    editor_line_comment_marker_for_binding(file_extension, true)
}

fn editor_line_comment_marker_for_binding(file_extension: &str, bound: bool) -> Option<&'static str> {
    if !bound {
        return None;
    }
    let lang_id = crate::highlighter::tree_sitter_lang_name_for_ext(file_extension);
    crate::languages::line_comment_marker(lang_id)
}

fn toggle_editor_line_comment_and_sync_highlighter(
    editor: &mut crate::editor::Editor,
    highlighter: &mut crate::highlighter::Highlighter,
    marker: &str,
) -> bool {
    let sync_edits_start = editor.sync_edits.len();
    if !editor.toggle_line_comment(marker) {
        return false;
    }

    for edit in &editor.sync_edits[sync_edits_start..] {
        match edit {
            crate::highlighter::SyncEdit::Insert { offset, text } => {
                highlighter.shift_insert(*offset, text.len(), Some(text));
            }
            crate::highlighter::SyncEdit::Delete { offset, len } => {
                highlighter.shift_delete(*offset, *len);
            }
        }
    }
    true
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

fn markdown_editor_key_action(
    markdown_document: bool,
    read_mode: bool,
    physical_key: PhysicalKey,
    primary: bool,
    alt: bool,
    has_text_insert: bool,
) -> Option<MarkdownEditorKeyAction> {
    let primary_copy = primary && physical_key == PhysicalKey::Code(KeyCode::KeyC);
    let primary_cut = primary && physical_key == PhysicalKey::Code(KeyCode::KeyX);
    let primary_paste = primary && physical_key == PhysicalKey::Code(KeyCode::KeyV);
    let primary_undo = primary && physical_key == PhysicalKey::Code(KeyCode::KeyZ);
    let primary_redo = primary && physical_key == PhysicalKey::Code(KeyCode::KeyY);
    markdown_editor_key_action_with_commands(
        markdown_document,
        read_mode,
        physical_key,
        primary,
        alt,
        has_text_insert,
        primary_copy,
        primary_cut,
        primary_paste,
        primary_undo,
        primary_redo,
        primary && physical_key == PhysicalKey::Code(KeyCode::KeyW),
    )
}

fn multicursor_keeps_bound_edit_command(
    keymap: &crate::keymap::Keymap,
    chord: crate::keymap::Chord,
) -> bool {
    keymap.hit(crate::keymap::Command::EditPaste, chord)
        || keymap.hit(crate::keymap::Command::EditorUndo, chord)
        || keymap.hit(crate::keymap::Command::EditorRedo, chord)
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum MarkdownEditorKeyAction {
    CopySelection,
    ReadonlyNotice,
    ScrollLines(i8),
    ScrollPages(i8),
    ScrollStart,
    ScrollEnd,
    Consume,
}

fn markdown_editor_key_action_with_commands(
    markdown_document: bool,
    read_mode: bool,
    physical_key: PhysicalKey,
    primary: bool,
    alt: bool,
    has_text_insert: bool,
    copy_bound: bool,
    cut_bound: bool,
    paste_bound: bool,
    undo_bound: bool,
    redo_bound: bool,
    expand_selection_bound: bool,
) -> Option<MarkdownEditorKeyAction> {
    if !markdown_document || !read_mode {
        return None;
    }

    if has_text_insert
        || matches!(
            physical_key,
            PhysicalKey::Code(
                KeyCode::Enter
                    | KeyCode::NumpadEnter
                    | KeyCode::Tab
                    | KeyCode::Space
                    | KeyCode::Backspace
                    | KeyCode::Delete
            )
        )
        || cut_bound || paste_bound || undo_bound || redo_bound
        || (alt && matches!(physical_key, PhysicalKey::Code(KeyCode::Enter)))
    {
        return Some(MarkdownEditorKeyAction::ReadonlyNotice);
    }

    match physical_key {
        _ if copy_bound => Some(MarkdownEditorKeyAction::CopySelection),
        PhysicalKey::Code(KeyCode::ArrowUp) => Some(MarkdownEditorKeyAction::ScrollLines(-1)),
        PhysicalKey::Code(KeyCode::ArrowDown) => Some(MarkdownEditorKeyAction::ScrollLines(1)),
        PhysicalKey::Code(KeyCode::PageUp) => Some(MarkdownEditorKeyAction::ScrollPages(-1)),
        PhysicalKey::Code(KeyCode::PageDown) => Some(MarkdownEditorKeyAction::ScrollPages(1)),
        PhysicalKey::Code(KeyCode::Home) => Some(MarkdownEditorKeyAction::ScrollStart),
        PhysicalKey::Code(KeyCode::End) => Some(MarkdownEditorKeyAction::ScrollEnd),
        PhysicalKey::Code(KeyCode::ArrowLeft | KeyCode::ArrowRight)
        | PhysicalKey::Code(KeyCode::KeyA)
            if primary =>
        {
            Some(MarkdownEditorKeyAction::Consume)
        }
        PhysicalKey::Code(KeyCode::ArrowLeft | KeyCode::ArrowRight) => {
            Some(MarkdownEditorKeyAction::Consume)
        }
        _ if expand_selection_bound => Some(MarkdownEditorKeyAction::Consume),
        _ => None,
    }
}

impl App {
    fn finish_editor_edit_after_input(
        &mut self,
        is_git_diff_tab: bool,
        git_diff_undo: bool,
        force_close_autocomplete: bool,
        should_trigger_autocomplete: bool,
        ty_completion_trigger: Option<&'static str>,
        should_notify_lsp: bool,
    ) -> bool {
        if is_git_diff_tab {
            self.rebuild_active_git_diff_from_editor_after_history(git_diff_undo);
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
            return true;
        }

        self.lsp_actions_menu = None;
        self.is_highlighted_once = true;
        self.is_highlight_complete = false;
        if self.active_tab_is_database_query() {
            self.refresh_active_database_query_analysis();
            if force_close_autocomplete {
                self.close_autocomplete();
            } else {
                self.update_active_database_query_completion(false);
            }
        } else if force_close_autocomplete {
            self.close_autocomplete();
        } else if should_trigger_autocomplete && self.file_extension != "dart" {
            if let Some(trigger) = ty_completion_trigger {
                self.request_ide_autocomplete(AutocompleteMode::TyContext, Some(trigger));
            } else if self.autocomplete_active
                && self.autocomplete_mode == AutocompleteMode::TyImports
            {
                self.request_ide_autocomplete(AutocompleteMode::TyImports, None);
            } else if cursor_after_python_member_dot(&self.editor)
                || cursor_inside_python_call_parens(&self.editor)
            {
                self.request_ide_autocomplete(AutocompleteMode::TyContext, None);
            } else {
                self.update_autocomplete();
            }
        } else if !should_trigger_autocomplete {
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
                    lsp.notify_change(
                        &path,
                        &ext,
                        &text,
                        crate::editor::lsp_document_version(self.editor.version),
                    );
                }
            }
            let (line_start_byte, line_end_byte) =
                sync_edit_line_range(&edits, &self.editor.line_offsets, self.editor.len());
            let (invalidate_start_byte, invalidate_end_byte) =
                crate::highlighter::sync_edit_invalidation_byte_range(&edits);

            self.highlighter.apply_document_edits(
                self.editor.version,
                edits,
                line_start_byte,
                line_end_byte,
                self.editor.len(),
                || self.editor.get_full_text(),
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
        if should_trigger_autocomplete
            && crate::lsp::has_server_for_extension(&self.file_extension)
            && !matches!(self.file_extension.as_str(), "py" | "pyi")
        {
            self.request_lsp_autocomplete(ty_completion_trigger);
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
            self.refresh_dart_closing_hints();
            if self.autocomplete_active {
                self.update_autocomplete();
            }
        }

        if let Some(log) = &mut self.pending_key_log {
            log.t_highlight = Some(std::time::Instant::now());
        }
        false
    }

    pub fn handle_editor_ime_commit(&mut self, text: &str) {
        // The hidden editor under a PDF tab is not a text field: only the inputs handled before
        // this call (search panel, terminal, dialogs) take IME text there.
        if text.is_empty() || self.show_welcome || self.tabs.get(self.active_tab).is_some_and(|tab| tab.kind.is_pdf() || tab.kind.is_image()) {
            return;
        }
        if self.active_tab_is_git_diff() || self.markdown_mode() == crate::app::MarkdownMode::Read {
            self.show_readonly_notice();
            return;
        }

        crate::app::mouse::suppress_hover_popup_until_mouse_move(&mut self.hover, self.renderer.as_mut());
        let multi_cursor_active = self.editor.has_extra_cursors();
        if multi_cursor_active {
            self.editor.apply_at_all_cursors(|editor| {
                editor.insert_str(text);
            });
        } else {
            let (deleted, inserted_len) = self.editor.insert_str(text);
            if let Some((offset, len)) = deleted {
                self.highlighter.shift_delete(offset, len);
            }
            self.highlighter
                .shift_insert(self.editor.cursor - inserted_len, inserted_len, Some(text));
        }
        let trigger = (text == ".").then_some(".");
        let wants_completion =
            text == "." || text.chars().all(|ch| ch.is_alphanumeric() || ch == '_');
        self.finish_editor_edit_after_input(
            false,
            false,
            multi_cursor_active,
            wants_completion && !multi_cursor_active,
            if multi_cursor_active { None } else { trigger },
            true,
        );

        let database_query_tab = self.active_tab_is_database_query();
        if let (Some(window), Some(renderer)) = (self.window.as_ref(), self.renderer.as_mut()) {
            let size = window.inner_size();
            let tab_bar_h = crate::render_view::editor_content_top_inset(
                self.show_welcome,
                self.is_ide_mode,
                database_query_tab,
                renderer.scale_factor,
            );
            App::ensure_cursor_visible(
                &mut self.scroll_y.target,
                &mut self.scroll_x.target,
                &self.editor,
                renderer,
                size.width as f32,
                size.height as f32,
                tab_bar_h,
            );
            window.request_redraw();
        }
        self.last_action = Instant::now();
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn handle_editor_keyboard_input(
        &mut self,
        event_loop: &HostLoop,
        key_event: KeyInput,
        chord: Option<crate::keymap::Chord>,
    ) {
        let ctrl = crate::platform::primary_shortcut_modifier(self.modifiers);
        let word = crate::platform::word_navigation_modifier(self.modifiers);
        let shift = self.modifiers.shift_key();
        let physical_key = key_event.physical_key;

        if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::MarkdownToggleToc, chord))
            && self.active_document_is_markdown()
        {
            self.toggle_markdown_toc();
            return;
        }

        if self.show_welcome {
            match physical_key {
                _ if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::FileOpen, chord)) => {
                    self.trigger_file_picker();
                }
                _ if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::AppQuit, chord)) => {
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
                        pdf_dark_pages: self.pdf_dark_pages,
                        ctrl_wheel_multiplier: self.ctrl_wheel_multiplier,
                        tool_paths: self.tool_paths.clone(),
                        dart_settings: self.dart_settings.clone(),
                        rust_settings: self.rust_settings.clone(),
                        keymap_overrides: self.keymap_overrides.clone(),
                    });
                    if self.is_ide_mode {
                        crate::save_panel_state(&self.ide_panel);
                    }
                    self.shutdown_background_services();
                    event_loop.exit();
                }
                _ => {}
            }
            return;
        }

        // The hidden editor under a PDF tab must not run any text or cursor command (edit, move,
        // select, clipboard, undo, save: Ctrl+S would overwrite the .pdf with the empty text).
        // Global shortcuts (tab switching, panels, project search, F8) are handled before this point.
        if self.tabs.get(self.active_tab).is_some_and(|tab| tab.kind.is_pdf() || tab.kind.is_image())
            && !pdf_tab_key_reaches_editor(chord, &self.keymap)
        {
            return;
        }

        let has_text_insert = crate::platform::text_input_modifiers_allowed(self.modifiers)
            && key_text_for_editor_insert(
                physical_key,
                key_event.text.as_deref(),
                key_event.logical_text.as_deref(),
                shift,
            )
            .is_some();
        if self.editor.has_extra_cursors() {
            let plain_navigation = !shift && (!self.modifiers.alt_key() || word);
            let multi_cursor_action = has_text_insert
                || (physical_key == PhysicalKey::Code(KeyCode::Space)
                    && crate::platform::text_input_modifiers_allowed(self.modifiers))
                || (!ctrl && !self.modifiers.alt_key()
                    && matches!(physical_key, PhysicalKey::Code(KeyCode::Backspace | KeyCode::Delete | KeyCode::Enter)))
                || chord.is_some_and(|chord| {
                    multicursor_keeps_bound_edit_command(&self.keymap, chord)
                })
                || (plain_navigation
                    && matches!(physical_key, PhysicalKey::Code(KeyCode::ArrowLeft | KeyCode::ArrowRight))
                    && (!ctrl || word))
                || (plain_navigation
                    && !ctrl
                    && matches!(physical_key, PhysicalKey::Code(KeyCode::ArrowUp | KeyCode::ArrowDown | KeyCode::Home | KeyCode::End)));
            if physical_key == PhysicalKey::Code(KeyCode::Escape)
                && chord.is_some_and(|chord| crate::keymap::is_reserved(crate::platform::CURRENT_PLATFORM, chord))
            {
                self.editor.clear_extra_cursors();
                self.close_autocomplete();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
                return;
            }
            if !multi_cursor_action {
                self.editor.clear_extra_cursors();
            }
        }
        let multi_cursor_active = self.editor.has_extra_cursors();
        if multi_cursor_active {
            self.close_autocomplete();
        }
        let markdown_action = markdown_editor_key_action_with_commands(
            self.active_document_is_markdown(),
            self.markdown_mode() == crate::app::MarkdownMode::Read,
            physical_key,
            ctrl,
            self.modifiers.alt_key(),
            has_text_insert,
            chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::EditCopy, chord)),
            chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::EditCut, chord)),
            chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::EditPaste, chord)),
            chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::EditorUndo, chord)),
            chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::EditorRedo, chord)),
            chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::EditorExpandSelection, chord)),
        );
        if let Some(action) = markdown_action {
            match action {
                MarkdownEditorKeyAction::CopySelection => {
                    if self.copy_markdown_read_selection()
                        && let Some(window) = self.window.as_ref()
                    {
                        window.request_redraw();
                    }
                }
                MarkdownEditorKeyAction::ReadonlyNotice => self.show_readonly_notice(),
                MarkdownEditorKeyAction::ScrollLines(direction) => {
                    let line_step = self
                        .renderer
                        .as_ref()
                        .map(|renderer| renderer.line_height.round().max(1.0))
                        .unwrap_or(24.0);
                    crate::app::markdown::scroll_markdown_read(
                        &mut self.scroll_y,
                        self.markdown.read_scroll_bounds(),
                        line_step * f32::from(direction),
                    );
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                }
                MarkdownEditorKeyAction::ScrollPages(direction) => {
                    let page = self
                        .window
                        .as_ref()
                        .map(|window| window.inner_size().height as f32)
                        .or_else(|| self.renderer.as_ref().map(|renderer| renderer.height))
                        .unwrap_or(600.0)
                        * 0.8;
                    crate::app::markdown::scroll_markdown_read(
                        &mut self.scroll_y,
                        self.markdown.read_scroll_bounds(),
                        page * f32::from(direction),
                    );
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                }
                MarkdownEditorKeyAction::ScrollStart => {
                    if self.prepare_markdown_absolute_scroll_target_navigation() {
                        self.markdown.mark_absolute_scroll_start_navigation();
                        self.scroll_y.animate_to(0.0);
                        self.markdown.remember_pending_absolute_scroll_target_y(0.0);
                    }
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                }
                MarkdownEditorKeyAction::ScrollEnd => {
                    if self.prepare_markdown_absolute_scroll_target_navigation()
                        && let Some(max_scroll) = self.markdown.read_scroll_bounds()
                    {
                        self.markdown.mark_absolute_scroll_end_navigation();
                        self.scroll_y.animate_to(max_scroll);
                        self.markdown
                            .remember_pending_absolute_scroll_target_y(max_scroll);
                    }
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                }
                MarkdownEditorKeyAction::Consume => {}
            }
            return;
        }

        if self.autocomplete_active && !multi_cursor_active {
            match self.handle_active_autocomplete_key(physical_key, ctrl) {
                AutocompletePopupKeyResult::Consumed => return,
                AutocompletePopupKeyResult::Continue | AutocompletePopupKeyResult::NotHandled => {}
            }
        }
        if self.mark_pending_autocomplete_apply_for_key(physical_key) {
            return;
        }

        if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::LspCodeActions, chord)) {
            self.open_lsp_actions_menu();
            return;
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
        let line_comment_marker = chord
            .is_some_and(|chord| self.keymap.hit(crate::keymap::Command::EditorToggleLineComment, chord))
            .then(|| editor_line_comment_marker_for_binding(&self.file_extension, true))
            .flatten();

        if is_git_diff_tab {
            let text_insert = crate::platform::text_input_modifiers_allowed(self.modifiers)
                && key_text_for_editor_insert(
                    physical_key,
                    key_event.text.as_deref(),
                    key_event.logical_text.as_deref(),
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
            ) || chord.is_some_and(|chord| {
                self.keymap.hit(crate::keymap::Command::EditCut, chord)
                    || self.keymap.hit(crate::keymap::Command::EditPaste, chord)
            })
                || line_comment_marker.is_some();
            if text_insert || edit_key {
                self.show_readonly_notice();
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
            _ if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::TabsCloseAll, chord)) => {
                if self.is_ide_mode {
                    if self.has_unsaved_changes() {
                        self.show_action_dialog(event_loop, PendingAction::CloseAllTabs);
                    } else {
                        self.close_all_tabs_unchecked();
                    }
                } else {
                    if self.has_unsaved_changes() {
                        self.show_action_dialog(event_loop, PendingAction::CloseFile);
                    } else {
                        self.close_current_file();
                    }
                }
                return;
            }
            _ if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::SettingsToggle, chord)) => {
                self.set_settings_visible(!self.show_settings);
                self.is_dragging = false;
                return;
            }
            _ if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::SearchEditorOpen, chord)) => {
                self.show_search = true;
                self.search_focused = true;
                self.search_editor.select_all();
                self.search_current_idx = None;
                self.update_search();
                self.jump_to_search_result();

                self.window.as_ref().unwrap().request_redraw();
                return;
            }
            _ if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::EditorExpandSelection, chord)) => {
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
            PhysicalKey::Code(KeyCode::Escape)
                if chord.is_some_and(|chord| crate::keymap::is_reserved(crate::platform::CURRENT_PLATFORM, chord)) =>
            {
                if self.show_search {
                    self.show_search = false;
                    self.search_focused = false;
                    self.search_results.clear();
                    self.search_current_idx = None;
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }
            }
            _ if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::FileSave, chord)) => {
                if self.save_current_file() {
                    App::update_window_title(
                        self.window.as_ref().unwrap(),
                        &self.base_title,
                        self.editor.is_dirty(),
                    );
                }
            }
            _ if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::FileOpen, chord)) =>
            {
                if self.editor.is_dirty() {
                    self.show_action_dialog(event_loop, PendingAction::OpenFile);
                } else {
                    self.trigger_file_picker();
                }
            }
            _ if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::EditorUndo, chord)) => {
                if let Some(delta) = self.editor.undo() {
                    if !is_git_diff_tab && !multi_cursor_active {
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
            _ if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::EditorRedo, chord)) => {
                if let Some(delta) = self.editor.redo() {
                    if !is_git_diff_tab && !multi_cursor_active {
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
            PhysicalKey::Code(KeyCode::ArrowLeft)
                if chord.is_some_and(|chord| crate::keymap::is_reserved(crate::platform::CURRENT_PLATFORM, chord)) =>
            {
                if word {
                    if multi_cursor_active {
                        self.editor.move_all_cursors(|editor| editor.move_word_left(false));
                    } else {
                        self.editor.move_word_left(shift);
                    }
                } else {
                    if multi_cursor_active {
                        self.editor.move_all_cursors(|editor| editor.move_left(false));
                    } else {
                        self.editor.move_left(shift);
                    }
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::ArrowRight)
                if chord.is_some_and(|chord| crate::keymap::is_reserved(crate::platform::CURRENT_PLATFORM, chord)) =>
            {
                if word {
                    if multi_cursor_active {
                        self.editor.move_all_cursors(|editor| editor.move_word_right(false));
                    } else {
                        self.editor.move_word_right(shift);
                    }
                } else {
                    if multi_cursor_active {
                        self.editor.move_all_cursors(|editor| editor.move_right(false));
                    } else {
                        self.editor.move_right(shift);
                    }
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::ArrowUp)
                if chord.is_some_and(|chord| crate::keymap::is_reserved(crate::platform::CURRENT_PLATFORM, chord)) =>
            {
                let Some(renderer) = self.renderer.as_mut() else {
                    return;
                };
                if multi_cursor_active {
                    self.editor.move_all_cursors(|editor| editor.move_up(renderer, false));
                } else {
                    self.editor.move_up(renderer, shift);
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::ArrowDown)
                if chord.is_some_and(|chord| crate::keymap::is_reserved(crate::platform::CURRENT_PLATFORM, chord)) =>
            {
                let Some(renderer) = self.renderer.as_mut() else {
                    return;
                };
                if multi_cursor_active {
                    self.editor.move_all_cursors(|editor| editor.move_down(renderer, false));
                } else {
                    self.editor.move_down(renderer, shift);
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::Home)
                if chord.is_some_and(|chord| crate::keymap::is_reserved(crate::platform::CURRENT_PLATFORM, chord)) =>
            {
                if multi_cursor_active {
                    self.editor.move_all_cursors(|editor| editor.move_home(false));
                } else if ctrl {
                    self.editor.move_start_of_file(shift);
                } else {
                    self.editor.move_home(shift);
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::End)
                if chord.is_some_and(|chord| crate::keymap::is_reserved(crate::platform::CURRENT_PLATFORM, chord)) =>
            {
                if multi_cursor_active {
                    self.editor.move_all_cursors(|editor| editor.move_end(false));
                } else if ctrl {
                    self.editor.move_end_of_file(shift);
                } else {
                    self.editor.move_end(shift);
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::PageUp)
                if chord.is_some_and(|chord| crate::keymap::is_reserved(crate::platform::CURRENT_PLATFORM, chord)) =>
            {
                let step = self.window.as_ref().unwrap().inner_size().height as f32 * 0.8;
                self.scroll_y.scroll_by(-step);
                self.editor
                    .move_page_up(self.renderer.as_mut().unwrap(), shift, step);
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::PageDown)
                if chord.is_some_and(|chord| crate::keymap::is_reserved(crate::platform::CURRENT_PLATFORM, chord)) =>
            {
                let step = self.window.as_ref().unwrap().inner_size().height as f32 * 0.8;
                self.scroll_y.scroll_by(step);
                self.editor
                    .move_page_down(self.renderer.as_mut().unwrap(), shift, step);
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::Backspace)
                if word && chord.is_some_and(|chord| crate::keymap::is_reserved(crate::platform::CURRENT_PLATFORM, chord)) =>
            {
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
            PhysicalKey::Code(KeyCode::Delete)
                if word && chord.is_some_and(|chord| crate::keymap::is_reserved(crate::platform::CURRENT_PLATFORM, chord)) =>
            {
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
            PhysicalKey::Code(KeyCode::Backspace)
                if chord.is_some_and(|chord| crate::keymap::is_reserved(crate::platform::CURRENT_PLATFORM, chord)) =>
            {
                let before_sync_edits = self.editor.sync_edits.len();
                let before_cursor = self.editor.cursor;
                let before_lines = self.editor.line_offsets.clone();
                if multi_cursor_active {
                    self.editor.apply_at_all_cursors(|editor| {
                        editor.backspace();
                    });
                } else if let Some((offset, len)) = self.editor.backspace() {
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
                if multi_cursor_active && self.editor.sync_edits.len() > before_sync_edits {
                    is_edit = true;
                    force_close_autocomplete = true;
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::Delete)
                if chord.is_some_and(|chord| crate::keymap::is_reserved(crate::platform::CURRENT_PLATFORM, chord)) =>
            {
                if multi_cursor_active {
                    let before_sync_edits = self.editor.sync_edits.len();
                    self.editor.apply_at_all_cursors(|editor| {
                        editor.delete_forward();
                    });
                    is_edit = self.editor.sync_edits.len() > before_sync_edits;
                    force_close_autocomplete = is_edit;
                } else if let Some((offset, len)) = self.editor.delete_forward() {
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
            PhysicalKey::Code(KeyCode::Enter)
                if chord.is_some_and(|chord| crate::keymap::is_reserved(crate::platform::CURRENT_PLATFORM, chord)) =>
            {
                if multi_cursor_active {
                    self.editor.apply_at_all_cursors(|editor| {
                        let insert_text = format!("\n{}", editor.get_auto_indent());
                        editor.insert_str(&insert_text);
                    });
                    is_edit = true;
                    force_close_autocomplete = true;
                } else {
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
                    is_edit = true;
                }
                cursor_moved = true;
            }
            PhysicalKey::Code(KeyCode::Tab)
                if chord.is_some_and(|chord| crate::keymap::is_reserved(crate::platform::CURRENT_PLATFORM, chord)) =>
            {
                let (del_info, ins_len) = self.editor.insert_str("    ");
                if let Some((offset, len)) = del_info {
                    self.highlighter.shift_delete(offset, len);
                }
                self.highlighter
                    .shift_insert(self.editor.cursor - ins_len, ins_len, Some("    "));
                cursor_moved = true;
                is_edit = true;
            }
            _ if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::EditorToggleLineComment, chord)) => {
                if let Some(marker) = line_comment_marker
                    && toggle_editor_line_comment_and_sync_highlighter(
                        &mut self.editor,
                        &mut self.highlighter,
                        marker,
                    )
                {
                    cursor_moved = true;
                    is_edit = true;
                    force_close_autocomplete = true;
                }
            }
            _ if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::EditorComplete, chord)) => {
                if crate::lsp::has_server_for_extension(&self.file_extension)
                    && !matches!(self.file_extension.as_str(), "py" | "pyi")
                {
                    self.request_lsp_autocomplete(None);
                } else {
                    self.update_autocomplete();
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
                return;
            }
            PhysicalKey::Code(KeyCode::Space)
                if chord.is_some_and(|chord| crate::keymap::is_reserved(crate::platform::CURRENT_PLATFORM, chord)) =>
            {
                if multi_cursor_active {
                    self.editor.apply_at_all_cursors(|editor| {
                        editor.insert_str(" ");
                    });
                    is_edit = true;
                    force_close_autocomplete = true;
                } else {
                    let (del_info, ins_len) = self.editor.insert_str(" ");
                    if let Some((offset, len)) = del_info {
                        self.highlighter.shift_delete(offset, len);
                    }
                    self.highlighter
                        .shift_insert(self.editor.cursor - ins_len, ins_len, Some(" "));
                    is_edit = true;
                }
                cursor_moved = true;
                should_notify_lsp = false;
            }
            _ if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::TabsClose, chord)) => {
                self.close_tab_at(self.active_tab);
                return;
            }
            _ if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::EditCopy, chord)) => {
                let mut copied = false;
                if !copied && let Some(text) = self.selected_autocomplete_detail_text() {
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
            _ if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::EditCut, chord)) => {
                if let Some(text) = self.editor.get_selection() {
                    self.set_clipboard_text(text);
                    if let Some((offset, len)) = self.editor.delete_selection() {
                        self.highlighter.shift_delete(offset, len);
                        is_edit = true;
                    }
                }
                cursor_moved = true;
            }
            _ if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::EditPaste, chord)) => {
                if let Some(text) = self.get_clipboard_text() {
                    if multi_cursor_active {
                        let before_sync_edits = self.editor.sync_edits.len();
                        self.editor.paste_at_all_cursors(&text);
                        is_edit = self.editor.sync_edits.len() > before_sync_edits;
                        force_close_autocomplete = is_edit;
                        cursor_moved = true;
                    } else {
                        let (del_info, ins_len) = self.editor.insert_str(&text);
                        if del_info.is_some() || ins_len > 0 {
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
                }
            }
            _ if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::EditSelectAll, chord)) => {
                self.editor.select_all();
                self.close_autocomplete();
            }
            _ => {
                if crate::platform::text_input_modifiers_allowed(self.modifiers) {
                    if let Some(txt) = key_text_for_editor_insert(
                        physical_key,
                        key_event.text.as_deref(),
                        key_event.logical_text.as_deref(),
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

                        if multi_cursor_active {
                            self.editor.apply_at_all_cursors(|editor| {
                                editor.insert_str(insert_txt);
                                if move_inside_pair {
                                    editor.move_left(false);
                                }
                            });
                            force_close_autocomplete = true;
                        } else {
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
                        }
                        cursor_moved = true;
                        is_edit = true;

                        if txt == "." && !multi_cursor_active {
                            should_trigger_autocomplete = true;
                            ty_completion_trigger = Some(".");
                        } else if !multi_cursor_active
                            && crate::lsp::has_server_for_extension(&self.file_extension)
                            && !matches!(self.file_extension.as_str(), "py" | "pyi")
                            && matches!(txt, "(" | ",")
                        {
                            should_trigger_autocomplete = true;
                            ty_completion_trigger = Some(if txt == "(" { "(" } else { "," });
                        } else if !multi_cursor_active && txt.chars().all(|c| c.is_alphanumeric() || c == '_') {
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
            if self.active_document_is_markdown() {
                self.markdown
                    .mark_absolute_scroll_navigation_with_scroll(&mut self.scroll_y);
            }
            self.close_autocomplete();
            self.lsp_actions_menu = None;
        }

        if is_edit {
            crate::app::mouse::suppress_hover_popup_until_mouse_move(&mut self.hover, self.renderer.as_mut());
            let git_diff_undo = chord
                .is_some_and(|chord| self.keymap.hit(crate::keymap::Command::EditorUndo, chord));
            if self.finish_editor_edit_after_input(
                is_git_diff_tab,
                git_diff_undo,
                force_close_autocomplete,
                should_trigger_autocomplete,
                ty_completion_trigger,
                should_notify_lsp,
            ) {
                return;
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

                let tab_bar_h = crate::render_view::editor_content_top_inset(
                    self.show_welcome,
                    self.is_ide_mode,
                    self.active_tab_is_database_query(),
                    self.renderer.as_ref().unwrap().scale_factor,
                );
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

include!("editor_keys_tests.rs");

#[cfg(test)]
mod markdown_read_keymap_tests {
    use super::*;

    #[test]
    fn markdown_read_mode_does_not_swallow_search_rebound_to_ctrl_w() {
        let action = markdown_editor_key_action_with_commands(
            true, true, PhysicalKey::Code(KeyCode::KeyW), true, false, false,
            false, false, false, false, false, false,
        );
        assert_eq!(action, None, "SearchEditorOpen on Ctrl+W with expand_selection unbound must reach search: {action:?}");
    }

    #[test]
    fn markdown_read_mode_consumes_bound_expand_selection() {
        let action = markdown_editor_key_action_with_commands(
            true, true, PhysicalKey::Code(KeyCode::KeyW), true, false, false,
            false, false, false, false, false, true,
        );
        assert_eq!(action, Some(MarkdownEditorKeyAction::Consume), "bound expand_selection remains unavailable in read mode: {action:?}");
    }
}

#[cfg(test)]
mod multicursor_keymap_tests {
    use super::*;

    #[test]
    fn rebound_alt_paste_preserves_multicursor_action() {
        let chord = crate::keymap::Chord::parse(crate::platform::CURRENT_PLATFORM, "alt+v")
            .expect("test chord parses");
        let mut overrides = crate::keymap::KeymapOverrides::default();
        overrides.add_chord(crate::platform::CURRENT_PLATFORM, crate::keymap::Command::EditPaste, chord);
        let keymap = crate::keymap::Keymap::build(&overrides);
        let keeps_cursors = multicursor_keeps_bound_edit_command(&keymap, chord);
        assert!(keeps_cursors, "Alt+V is EditPaste and must preserve extra cursors: chord={chord:?}, keeps={keeps_cursors}");
    }

    #[test]
    fn rebound_alt_undo_and_redo_preserve_multicursor_actions() {
        for (command, key) in [
            (crate::keymap::Command::EditorUndo, "alt+u"),
            (crate::keymap::Command::EditorRedo, "alt+r"),
        ] {
            let chord = crate::keymap::Chord::parse(crate::platform::CURRENT_PLATFORM, key)
                .expect("test chord parses");
            let mut overrides = crate::keymap::KeymapOverrides::default();
            overrides.add_chord(crate::platform::CURRENT_PLATFORM, command, chord);
            let keymap = crate::keymap::Keymap::build(&overrides);
            let keeps_cursors = multicursor_keeps_bound_edit_command(&keymap, chord);
            assert!(keeps_cursors, "Alt-bound {command:?} must preserve extra cursors: chord={chord:?}, keeps={keeps_cursors}");
        }
    }
}
