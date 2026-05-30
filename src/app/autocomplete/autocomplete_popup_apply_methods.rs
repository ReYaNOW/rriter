fn python_import_completion_text(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("from ") || trimmed.starts_with("import ")
}

fn update_python_import_completion_depth(trimmed: &str, paren_depth: &mut i32) -> bool {
    for b in trimmed.bytes() {
        match b {
            b'(' | b'[' | b'{' => *paren_depth += 1,
            b')' | b']' | b'}' => *paren_depth -= 1,
            _ => {}
        }
    }
    *paren_depth > 0 || trimmed.ends_with('\\')
}

fn python_top_import_block_end(text: &str) -> Option<usize> {
    let mut offset = 0usize;
    let mut end = None;
    let mut continuing = false;
    let mut paren_depth = 0i32;
    for raw_line in text.split_inclusive('\n') {
        let line_start = offset;
        offset += raw_line.len();
        let line = raw_line.trim_end_matches('\n').trim_end_matches('\r');
        let trimmed = line.trim_start();
        let leading = line.len().saturating_sub(trimmed.len());

        if continuing {
            end = Some(line_start + line.len());
            continuing = update_python_import_completion_depth(trimmed, &mut paren_depth);
            if !continuing {
                paren_depth = 0;
            }
            continue;
        }

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if leading == 0 && python_import_completion_text(trimmed) {
            end = Some(line_start + line.len());
            continuing = update_python_import_completion_depth(trimmed, &mut paren_depth);
            if !continuing {
                paren_depth = 0;
            }
            continue;
        }
        break;
    }
    end
}

fn append_python_import_edits_to_block(
    file_extension: &str,
    text: &str,
    line_offsets: &[usize],
    edits: &mut [crate::lsp::TextChange],
) {
    if !matches!(file_extension, "py" | "pyi") {
        return;
    }
    let block_end = python_top_import_block_end(text).or_else(|| {
        crate::languages::python::import_blocks(text)
            .first()
            .map(|block| block.end)
    });
    let Some(block_end) = block_end else { return };
    let (line, col) = crate::lsp::offset_to_lsp_pos(text, block_end, line_offsets);
    for edit in edits {
        if edit.start_line != edit.end_line
            || edit.start_col != edit.end_col
            || !python_import_completion_text(&edit.new_text)
        {
            continue;
        }
        let import_text = edit
            .new_text
            .trim_matches(|c| c == '\n' || c == '\r')
            .to_string();
        if import_text.is_empty() {
            continue;
        }
        edit.start_line = line;
        edit.start_col = col;
        edit.end_line = line;
        edit.end_col = col;
        edit.new_text = format!("\n{import_text}");
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CompletionTextEditOp {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) new_text: String,
}

#[derive(Clone, Debug)]
pub(crate) struct CompletionAppliedEdit {
    pub(crate) offset: usize,
    pub(crate) deleted_len: usize,
    pub(crate) inserted_text: String,
}

#[derive(Clone, Debug)]
pub(crate) struct CompletionApplyPlan {
    pub(crate) ops: Vec<CompletionTextEditOp>,
    pub(crate) primary_start: Option<usize>,
    pub(crate) target_cursor: Option<usize>,
    pub(crate) fallback_insert: String,
    pub(crate) fallback_prefix_len: usize,
}

pub(crate) fn apply_completion_plan_to_editor(
    editor: &mut Editor,
    mut plan: CompletionApplyPlan,
) -> Vec<CompletionAppliedEdit> {
    let mut applied = Vec::new();
    if plan.ops.is_empty() {
        for _ in 0..plan.fallback_prefix_len {
            if let Some((offset, len)) = editor.backspace() {
                applied.push(CompletionAppliedEdit {
                    offset,
                    deleted_len: len,
                    inserted_text: String::new(),
                });
            }
        }
        let (del_info, ins_len) = editor.insert_str(&plan.fallback_insert);
        if let Some((offset, len)) = del_info {
            applied.push(CompletionAppliedEdit {
                offset,
                deleted_len: len,
                inserted_text: String::new(),
            });
        }
        if ins_len > 0 {
            applied.push(CompletionAppliedEdit {
                offset: editor.cursor.saturating_sub(ins_len),
                deleted_len: 0,
                inserted_text: plan.fallback_insert,
            });
        }
        return applied;
    }

    plan.ops
        .sort_unstable_by(|a, b| b.start.cmp(&a.start).then(b.end.cmp(&a.end)));
    for op in &plan.ops {
        if op.start <= op.end {
            let (offset, deleted_len, _) = editor.replace_range(op.start, op.end, &op.new_text);
            applied.push(CompletionAppliedEdit {
                offset,
                deleted_len,
                inserted_text: op.new_text.clone(),
            });
        }
    }
    if let (Some(primary_start), Some(mut target_cursor)) =
        (plan.primary_start, plan.target_cursor)
    {
        for op in &plan.ops {
            if op.end <= primary_start {
                let old_len = op.end.saturating_sub(op.start);
                let delta = op.new_text.len() as isize - old_len as isize;
                target_cursor = ((target_cursor as isize) + delta).max(0) as usize;
            }
        }
        editor.cursor = target_cursor.min(editor.len());
        editor.selection_anchor = None;
    }
    applied
}

pub(crate) fn tree_sitter_completion_options(
    completions: &[CompletionItem],
    prefix: &str,
    cursor: usize,
    prefix_start: usize,
    file_extension: &str,
    trace_label: &str,
) -> Vec<(AutocompleteItem, Vec<usize>)> {
    let prefix_lower = prefix.to_lowercase();
    let mut best_scopes: FxHashMap<String, CompletionItem> = FxHashMap::default();
    for comp in completions {
        if comp.scope_start == prefix_start
            && matches!(
                comp.kind,
                SymbolKind::Variable
                    | SymbolKind::Parameter
                    | SymbolKind::Argument
                    | SymbolKind::Unknown
            )
            && comp.word.to_lowercase().starts_with(&prefix_lower)
        {
            continue;
        }
        if cursor >= comp.scope_start && cursor <= comp.scope_end {
            let current_size = comp.scope_end.saturating_sub(comp.scope_start);
            if let Some(existing) = best_scopes.get(&comp.word) {
                let ex_size = existing.scope_end.saturating_sub(existing.scope_start);
                let prefer_parameter =
                    comp.kind == SymbolKind::Parameter && existing.kind != SymbolKind::Parameter;
                let keep_parameter =
                    existing.kind == SymbolKind::Parameter && comp.kind != SymbolKind::Parameter;
                if prefer_parameter || (!keep_parameter && current_size < ex_size) {
                    best_scopes.insert(comp.word.clone(), comp.clone());
                }
            } else {
                best_scopes.insert(comp.word.clone(), comp.clone());
            }
        }
    }

    if autocomplete_trace_enabled() {
        println!(
            "Autocomplete {trace_label}_scope: prefix={:?} completions={} best_scopes={}",
            prefix,
            completions.len(),
            best_scopes.len()
        );
    }

    let mut matches = Vec::with_capacity(best_scopes.len());
    for (_, comp) in best_scopes {
        let comp_lower = comp.word.to_lowercase();
        if let Some(indices) = fuzzy_match(&prefix_lower, &comp_lower) {
            let is_prefix = comp_lower.starts_with(&prefix_lower);
            let mut score = 0i64;
            let scope_bonus = if comp.kind == SymbolKind::Keyword {
                0
            } else {
                let scope_size = comp.scope_end.saturating_sub(comp.scope_start);
                let sz = scope_size.min(i64::MAX as usize) as i64;
                10_000_000 / (sz + 1).max(1)
            };
            score += scope_bonus;
            score -= (comp.word.len() as i64) * 10;
            matches.push((is_prefix, score, comp, indices));
        }
    }

    matches.sort_unstable_by_key(|(is_prefix, score, comp, _)| {
        let scoped_self_priority = if file_extension == "py" && comp.kind == SymbolKind::Parameter {
            python_scoped_self_priority(&comp.word)
        } else {
            2
        };
        let type_priority = match comp.kind {
            SymbolKind::Variable
            | SymbolKind::Parameter
            | SymbolKind::Argument
            | SymbolKind::Property => 0,
            SymbolKind::Function => 1,
            SymbolKind::Class | SymbolKind::Module => 2,
            SymbolKind::Builtin => 3,
            SymbolKind::Keyword => 4,
            SymbolKind::Unknown => 5,
        };
        let match_priority = if *is_prefix { 0 } else { 1 };
        (
            match_priority,
            scoped_self_priority,
            is_magic_python_name(&comp.word),
            type_priority,
            std::cmp::Reverse(*score),
        )
    });

    matches
        .into_iter()
        .take(60)
        .map(|m| (m.2.into(), m.3))
        .collect()
}

pub(crate) fn enrich_python_tree_sitter_options(
    options: &mut Vec<(AutocompleteItem, Vec<usize>)>,
    file_extension: &str,
    text: &str,
    cursor: usize,
) {
    if file_extension != "py" {
        return;
    }
    for (item, _) in options.iter_mut() {
        assign_builtin_completion_module(item);
        if item.kind == SymbolKind::Builtin {
            item.kind = python_builtin_completion_kind(&item.word).unwrap_or(SymbolKind::Function);
        }
    }
    if !options.is_empty() {
        let imports = imported_python_symbols(text);
        apply_import_modules_to_autocomplete_items(options, &imports);
    }
    if !options.is_empty()
        && let Some(owner) = python_enclosing_class_before_cursor(text, cursor)
    {
        for (item, _) in options.iter_mut() {
            if item.kind == SymbolKind::Parameter && matches!(item.word.as_str(), "self" | "cls") {
                item.module = Some(owner.clone());
                item.module_path = None;
            }
        }
    }
}

pub(crate) fn build_tree_sitter_autocomplete_options(
    ctx: &AutocompleteEditorContext<'_>,
    completions: &[CompletionItem],
    trace_label: &str,
) -> Vec<(AutocompleteItem, Vec<usize>)> {
    let prefix = ctx.current_word_prefix();
    let prefix_start = ctx.analysis_cursor.saturating_sub(prefix.len());
    let mut options = tree_sitter_completion_options(
        completions,
        &prefix,
        ctx.analysis_cursor,
        prefix_start,
        ctx.file_extension,
        trace_label,
    );
    if options.len() == 1 && options[0].0.word == prefix {
        options.clear();
    }
    enrich_python_tree_sitter_options(
        &mut options,
        ctx.file_extension,
        ctx.analysis_text,
        ctx.analysis_cursor,
    );
    options
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutocompletePopupKeyResult {
    NotHandled,
    Continue,
    Consumed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutocompleteKeyAction {
    None,
    DismissAndContinue,
    DismissAndConsume,
    MoveDown,
    MoveUp,
    Apply,
}

pub(crate) fn autocomplete_key_action(
    physical_key: winit::keyboard::PhysicalKey,
) -> AutocompleteKeyAction {
    match physical_key {
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape) => {
            AutocompleteKeyAction::DismissAndConsume
        }
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowLeft)
        | winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowRight) => {
            AutocompleteKeyAction::DismissAndContinue
        }
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowDown) => {
            AutocompleteKeyAction::MoveDown
        }
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowUp) => {
            AutocompleteKeyAction::MoveUp
        }
        winit::keyboard::PhysicalKey::Code(
            winit::keyboard::KeyCode::Enter
            | winit::keyboard::KeyCode::NumpadEnter
            | winit::keyboard::KeyCode::Tab,
        ) => AutocompleteKeyAction::Apply,
        _ => AutocompleteKeyAction::None,
    }
}

pub(crate) fn autocomplete_next_index(
    current: usize,
    len: usize,
    reverse: bool,
    jump: bool,
) -> usize {
    if len == 0 {
        return 0;
    }
    if jump && reverse {
        return if current == 0 {
            len - 1
        } else if current < 5 {
            0
        } else {
            current - 5
        };
    }
    if jump {
        return if current + 1 == len {
            0
        } else {
            (current + 5).min(len - 1)
        };
    }
    if reverse {
        if current == 0 { len - 1 } else { current - 1 }
    } else {
        (current + 1) % len
    }
}

impl App {
    pub(crate) fn handle_active_autocomplete_key(
        &mut self,
        physical_key: winit::keyboard::PhysicalKey,
        ctrl: bool,
    ) -> AutocompletePopupKeyResult {
        if !self.autocomplete_active {
            return AutocompletePopupKeyResult::NotHandled;
        }
        match autocomplete_key_action(physical_key) {
            AutocompleteKeyAction::DismissAndContinue => {
                self.close_autocomplete();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
                AutocompletePopupKeyResult::Continue
            }
            AutocompleteKeyAction::DismissAndConsume => {
                self.close_autocomplete();
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
                AutocompletePopupKeyResult::Consumed
            }
            AutocompleteKeyAction::MoveDown => {
                if !self.autocomplete_options.is_empty() {
                    self.autocomplete_selected_idx = autocomplete_next_index(
                        self.autocomplete_selected_idx,
                        self.autocomplete_options.len(),
                        false,
                        ctrl,
                    );
                    self.autocomplete_hovered_idx = None;
                    self.ensure_autocomplete_visible();
                    self.request_active_autocomplete_detail_for_index(
                        self.autocomplete_selected_idx,
                    );
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
                AutocompletePopupKeyResult::Consumed
            }
            AutocompleteKeyAction::MoveUp => {
                if !self.autocomplete_options.is_empty() {
                    self.autocomplete_selected_idx = autocomplete_next_index(
                        self.autocomplete_selected_idx,
                        self.autocomplete_options.len(),
                        true,
                        ctrl,
                    );
                    self.autocomplete_hovered_idx = None;
                    self.ensure_autocomplete_visible();
                    self.request_active_autocomplete_detail_for_index(
                        self.autocomplete_selected_idx,
                    );
                }
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
                AutocompletePopupKeyResult::Consumed
            }
            AutocompleteKeyAction::Apply => {
                if !self.autocomplete_options.is_empty() {
                    self.apply_autocomplete();
                }
                AutocompletePopupKeyResult::Consumed
            }
            AutocompleteKeyAction::None => AutocompletePopupKeyResult::NotHandled,
        }
    }

    pub(crate) fn mark_pending_autocomplete_apply_for_key(
        &mut self,
        physical_key: winit::keyboard::PhysicalKey,
    ) -> bool {
        if self.autocomplete_mode != AutocompleteMode::TyContext
            || self.autocomplete_pending_request_id.is_none()
                && self.autocomplete_signature_request_id.is_none()
            || !matches!(
                physical_key,
                winit::keyboard::PhysicalKey::Code(
                    winit::keyboard::KeyCode::Enter
                        | winit::keyboard::KeyCode::Tab
                        | winit::keyboard::KeyCode::NumpadEnter
                )
            )
        {
            return false;
        }
        let editor = if self.api_mock_completion_focus().is_some() {
            &self.ide_panel.api.input_editor
        } else {
            &self.editor
        };
        if !cursor_after_python_member_dot(editor) && !cursor_inside_python_call_parens(editor) {
            return false;
        }
        self.autocomplete_apply_pending_response = true;
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
        true
    }

    pub fn update_autocomplete(&mut self) {
        let Some(source) = self.active_autocomplete_source() else {
            return;
        };
        self.update_tree_sitter_autocomplete_for_source(source);
    }

    pub(crate) fn update_tree_sitter_autocomplete_for_source(
        &mut self,
        source: ActiveAutocompleteSource,
    ) {
        self.trace_autocomplete_state("update_ts:begin");
        let Some(snapshot) = self.active_autocomplete_source_snapshot(source) else {
            self.trace_autocomplete_state("update_ts:no_source");
            return;
        };
        let prefix = snapshot.current_word_prefix();
        if self.autocomplete_active && self.autocomplete_mode != AutocompleteMode::TreeSitter {
            let after_member_dot = self.source_after_python_member_dot(source);
            let inside_call = self.source_inside_python_call_parens(source);
            if self.autocomplete_mode == AutocompleteMode::TyContext
                && (self.source_member_chain_too_deep(source)
                    || (!after_member_dot && !inside_call)
                    || (prefix.is_empty() && !after_member_dot))
            {
                self.close_autocomplete();
                self.trace_autocomplete_state("update_ts:closed_non_ts_context");
            }
            self.trace_autocomplete_state("update_ts:skip_non_ts_active");
            return;
        }
        if prefix.is_empty() {
            self.reset_autocomplete_for_empty_prefix();
            self.trace_autocomplete_state("update_ts:empty_prefix");
            return;
        }

        self.autocomplete_options = {
            let editor = self.autocomplete_editor_for_source(source);
            let completions = self.autocomplete_tree_sitter_completions_for_source(source);
            let ctx = snapshot.editor_context(editor);
            build_tree_sitter_autocomplete_options(&ctx, completions, "update_ts")
        };
        if autocomplete_trace_enabled() {
            let first = self
                .autocomplete_options
                .iter()
                .take(5)
                .map(|(item, _)| item.word.as_str())
                .collect::<Vec<_>>()
                .join(",");
            println!(
                "Autocomplete update_ts_matches: opts={} first=[{}]",
                self.autocomplete_options.len(),
                first
            );
        }
        if !self.autocomplete_options.is_empty() {
            if !self.autocomplete_active {
                self.autocomplete_anim_progress = 0.0;
                self.autocomplete_anchor = self.autocomplete_anchor_for_source(source);
            } else if source.is_api_mock() {
                self.autocomplete_anchor = self.autocomplete_anchor_for_source(source);
            }
            self.autocomplete_scroll.current = 0.0;
            self.autocomplete_scroll.target = 0.0;
            self.autocomplete_mode = AutocompleteMode::TreeSitter;
            self.autocomplete_active = true;
            self.autocomplete_selected_idx = 0;
            if source.is_api_mock() {
                self.autocomplete_hovered_idx = None;
            }
            self.request_active_autocomplete_detail_for_index(0);
            self.trace_autocomplete_state("update_ts:active_end");
        } else {
            self.autocomplete_active = false;
            self.trace_autocomplete_state("update_ts:no_options");
        }
    }

    pub fn ensure_autocomplete_visible(&mut self) {
        let scale = self
            .renderer
            .as_ref()
            .map(|r| r.scale_factor)
            .unwrap_or(1.0);
        let step = 36.0 * scale;
        let visible_items = 7.0;

        self.autocomplete_scroll.anim_speed = 15.0;
        let top = self.autocomplete_scroll.target;
        let bottom = top + (visible_items * step);

        let item_top = self.autocomplete_selected_idx as f32 * step;
        let item_bottom = item_top + step;

        if item_top < top {
            self.autocomplete_scroll.set_target(item_top);
        } else if item_bottom > bottom {
            self.autocomplete_scroll
                .set_target(item_bottom - (visible_items * step));
        }

        let total_items = self.autocomplete_options.len() as f32;
        let visible_limit = total_items.min(visible_items);
        let max_scroll = ((total_items - visible_limit) * step).max(0.0);

        self.autocomplete_scroll.clamp_target(0.0, max_scroll);
    }

    pub fn apply_autocomplete(&mut self) {
        if self.api_mock_completion_focus().is_some() && self.apply_api_mock_autocomplete() {
            return;
        }
        if !self.autocomplete_active || self.autocomplete_options.is_empty() {
            return;
        }
        let selected_item = self.autocomplete_options[self.autocomplete_selected_idx]
            .0
            .clone();
        if selected_item.text_edit.is_some() || !selected_item.additional_text_edits.is_empty() {
            self.apply_lsp_completion_item(&selected_item);
            self.close_autocomplete();
            return;
        }
        let selected = selected_item
            .insert_text
            .clone()
            .unwrap_or(selected_item.word.clone());
        let prefix_len = self.get_current_word_prefix().len();
        self.apply_completion_plan_to_main_editor(CompletionApplyPlan {
            ops: Vec::new(),
            primary_start: None,
            target_cursor: None,
            fallback_insert: selected,
            fallback_prefix_len: prefix_len,
        });

        self.close_autocomplete();
        self.sync_after_autocomplete();

        if let Some(w) = self.window.as_ref() {
            App::update_window_title(w, &self.base_title, self.editor.is_dirty());
            w.request_redraw();
        }
    }

    fn apply_completion_plan_to_main_editor(&mut self, plan: CompletionApplyPlan) {
        let applied = apply_completion_plan_to_editor(&mut self.editor, plan);
        for edit in applied {
            if edit.deleted_len > 0 {
                self.highlighter.shift_delete(edit.offset, edit.deleted_len);
            }
            if !edit.inserted_text.is_empty() {
                self.highlighter.shift_insert(
                    edit.offset,
                    edit.inserted_text.len(),
                    Some(&edit.inserted_text),
                );
            }
        }
    }

    pub(crate) fn apply_lsp_completion_item(&mut self, item: &AutocompleteItem) {
        let Some(main_edit) = item.text_edit.clone() else {
            if !item.additional_text_edits.is_empty() {
                if let Some(path) = self.file_path.clone() {
                    let text = self.editor.get_full_text();
                    let mut edits = item.additional_text_edits.clone();
                    if self.autocomplete_mode == AutocompleteMode::TyImports {
                        append_python_import_edits_to_block(
                            &self.file_extension,
                            &text,
                            &self.editor.line_offsets,
                            &mut edits,
                        );
                    }
                    let mut changes = std::collections::HashMap::new();
                    changes.insert(path, edits);
                    self.apply_workspace_edit(&crate::lsp::WorkspaceEdit { changes }, true);
                }
            }
            let selected = item
                .insert_text
                .as_deref()
                .unwrap_or(&item.word)
                .to_string();
            let prefix_len = self.get_current_word_prefix().len();
            self.apply_completion_plan_to_main_editor(CompletionApplyPlan {
                ops: Vec::new(),
                primary_start: None,
                target_cursor: None,
                fallback_insert: selected,
                fallback_prefix_len: prefix_len,
            });

            self.sync_after_autocomplete();

            if let Some(w) = self.window.as_ref() {
                App::update_window_title(w, &self.base_title, self.editor.is_dirty());
                w.request_redraw();
            }
            return;
        };

        let text = self.editor.get_full_text();
        let main_start =
            crate::lsp::lsp_pos_to_offset(&text, main_edit.start_line, main_edit.start_col);
        let target_cursor = main_start + main_edit.new_text.len();
        let mut changes = item.additional_text_edits.clone();
        if self.autocomplete_mode == AutocompleteMode::TyImports {
            append_python_import_edits_to_block(
                &self.file_extension,
                &text,
                &self.editor.line_offsets,
                &mut changes,
            );
        }
        changes.push(main_edit);

        let mut ops = Vec::with_capacity(changes.len());
        for change in &changes {
            let start = crate::lsp::lsp_pos_to_offset(&text, change.start_line, change.start_col);
            let end = crate::lsp::lsp_pos_to_offset(&text, change.end_line, change.end_col);
            ops.push(CompletionTextEditOp {
                start,
                end,
                new_text: change.new_text.clone(),
            });
        }
        self.apply_completion_plan_to_main_editor(CompletionApplyPlan {
            ops,
            primary_start: Some(main_start),
            target_cursor: Some(target_cursor),
            fallback_insert: item.insert_text.as_deref().unwrap_or(&item.word).to_string(),
            fallback_prefix_len: self.get_current_word_prefix().len(),
        });
        self.sync_after_autocomplete();

        if let Some(w) = self.window.as_ref() {
            App::update_window_title(w, &self.base_title, self.editor.is_dirty());
            w.request_redraw();
        }
    }

    pub(crate) fn sync_after_autocomplete(&mut self) {
        if self.editor.sync_edits.is_empty() {
            return;
        }
        let edits = std::mem::take(&mut self.editor.sync_edits);
        self.shift_current_python_inlay_hints_for_edits(&edits);
        if self.is_ide_mode {
            if let (Some(lsp), Some(path)) = (&mut self.lsp, &self.file_path) {
                let text = self.editor.get_full_text();
                let ext = self.file_extension.clone();
                let path = path.clone();
                lsp.notify_change(&path, &ext, &text, self.editor.version as i32);
            }
        }
        self.highlighter
            .apply_edits(self.editor.version, edits, None, None);
        self.last_sent_version = self.editor.version;
    }

    #[cfg(test)]
    pub(crate) fn python_member_dot_receiver_is_unavailable_self(&self) -> bool {
        let source = ActiveAutocompleteSource::MainEditor;
        self.active_autocomplete_source_snapshot(source)
            .is_some_and(|snapshot| {
                self.source_member_dot_receiver_is_unavailable_self(source, &snapshot)
            })
    }
}
