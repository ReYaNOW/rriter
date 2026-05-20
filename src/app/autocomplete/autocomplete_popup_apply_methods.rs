impl App {
    pub fn update_autocomplete(&mut self) {
        self.trace_autocomplete_state("update_ts:begin");
        if self.autocomplete_active && self.autocomplete_mode != AutocompleteMode::TreeSitter {
            let after_member_dot = cursor_after_python_member_dot(&self.editor);
            let inside_call = cursor_inside_python_call_parens(&self.editor);
            let empty_prefix = self.get_current_word_prefix().is_empty();
            if self.autocomplete_mode == AutocompleteMode::TyContext
                && (python_member_chain_too_deep(&self.editor)
                    || (!after_member_dot && !inside_call)
                    || (empty_prefix && !after_member_dot))
            {
                self.close_autocomplete();
                self.trace_autocomplete_state("update_ts:closed_non_ts_context");
            }
            self.trace_autocomplete_state("update_ts:skip_non_ts_active");
            return;
        }
        let prefix = self.get_current_word_prefix();
        if prefix.is_empty() {
            self.autocomplete_active = false;
            self.autocomplete_options.clear();
            self.autocomplete_mode = AutocompleteMode::TreeSitter;
            self.autocomplete_rect = None;
            self.autocomplete_anchor = None;
            self.autocomplete_detail_popup = None;
            self.autocomplete_detail_rect = None;
            self.autocomplete_detail_placement = None;
            self.autocomplete_detail_max_scroll = 0.0;
            self.reset_autocomplete_detail_size();
            crate::app::events::reset_autocomplete_frame_stats();
            self.trace_autocomplete_state("update_ts:empty_prefix");
            return;
        }

        let prefix_lower = prefix.to_lowercase();
        let cursor = self.editor.cursor;
        let prefix_start = cursor.saturating_sub(prefix.len());

        let mut best_scopes: FxHashMap<String, CompletionItem> = FxHashMap::default();

        for comp in &self.highlighter.completions {
            if comp.scope_start == prefix_start
                && matches!(
                    comp.kind,
                    SymbolKind::Variable | SymbolKind::Parameter | SymbolKind::Unknown
                )
                && comp.word.to_lowercase().starts_with(&prefix_lower)
            {
                continue;
            }
            if cursor >= comp.scope_start && cursor <= comp.scope_end {
                let current_size = comp.scope_end.saturating_sub(comp.scope_start);
                if let Some(existing) = best_scopes.get(&comp.word) {
                    let ex_size = existing.scope_end.saturating_sub(existing.scope_start);
                    let prefer_parameter = comp.kind == SymbolKind::Parameter
                        && existing.kind != SymbolKind::Parameter;
                    let keep_parameter = existing.kind == SymbolKind::Parameter
                        && comp.kind != SymbolKind::Parameter;
                    if prefer_parameter || (!keep_parameter && current_size < ex_size) {
                        best_scopes.insert(comp.word.clone(), comp.clone());
                    }
                } else {
                    best_scopes.insert(comp.word.clone(), comp.clone());
                }
            }
        }

        let mut matches = Vec::with_capacity(best_scopes.len());
        if autocomplete_trace_enabled() {
            println!(
                "Autocomplete update_ts_scope: prefix={:?} completions={} best_scopes={}",
                prefix,
                self.highlighter.completions.len(),
                best_scopes.len()
            );
        }

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
            let scoped_self_priority =
                if self.file_extension == "py" && comp.kind == SymbolKind::Parameter {
                    python_scoped_self_priority(&comp.word)
                } else {
                    2
                };
            let type_priority = match comp.kind {
                SymbolKind::Variable | SymbolKind::Parameter | SymbolKind::Property => 0,
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

        self.autocomplete_options = matches
            .into_iter()
            .take(60)
            .map(|m| (m.2.into(), m.3))
            .collect();
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
        if self.autocomplete_options.len() == 1 && self.autocomplete_options[0].0.word == prefix {
            self.autocomplete_options.clear();
            self.trace_autocomplete_state("update_ts:single_exact_clear");
        }
        if self.file_extension == "py" {
            for (item, _) in &mut self.autocomplete_options {
                assign_builtin_completion_module(item);
                if item.kind == SymbolKind::Builtin {
                    item.kind =
                        python_builtin_completion_kind(&item.word).unwrap_or(SymbolKind::Function);
                }
            }
        }
        if self.file_extension == "py" && !self.autocomplete_options.is_empty() {
            let imports = imported_python_symbols(&self.editor.get_full_text());
            apply_import_modules_to_autocomplete_items(&mut self.autocomplete_options, &imports);
        }
        if self.file_extension == "py"
            && !self.autocomplete_options.is_empty()
            && let Some(owner) = python_enclosing_class_before_cursor(
                &self.editor.get_full_text(),
                self.editor.cursor,
            )
        {
            for (item, _) in &mut self.autocomplete_options {
                if item.kind == SymbolKind::Parameter
                    && matches!(item.word.as_str(), "self" | "cls")
                {
                    item.module = Some(owner.clone());
                    item.module_path = None;
                }
            }
        }

        if !self.autocomplete_options.is_empty() {
            if !self.autocomplete_active {
                self.autocomplete_anim_progress = 0.0;
                self.autocomplete_anchor = None;
            }
            self.autocomplete_scroll.current = 0.0;
            self.autocomplete_scroll.target = 0.0;
            self.autocomplete_mode = AutocompleteMode::TreeSitter;
            self.autocomplete_active = true;
            self.autocomplete_selected_idx = 0;
            self.request_autocomplete_detail_for_index(0);
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

        for _ in 0..prefix_len {
            if let Some((offset, len)) = self.editor.backspace() {
                self.highlighter.shift_delete(offset, len);
            }
        }

        let (del_info, ins_len) = self.editor.insert_str(&selected);
        if let Some((offset, len)) = del_info {
            self.highlighter.shift_delete(offset, len);
        }
        self.highlighter
            .shift_insert(self.editor.cursor - ins_len, ins_len, Some(&selected));

        self.close_autocomplete();
        self.sync_after_autocomplete();

        if let Some(w) = self.window.as_ref() {
            App::update_window_title(w, &self.base_title, self.editor.is_dirty());
            w.request_redraw();
        }
    }

    pub(crate) fn apply_lsp_completion_item(&mut self, item: &AutocompleteItem) {
        let Some(main_edit) = item.text_edit.clone() else {
            if !item.additional_text_edits.is_empty() {
                if let Some(path) = self.file_path.clone() {
                    let mut changes = std::collections::HashMap::new();
                    changes.insert(path, item.additional_text_edits.clone());
                    self.apply_workspace_edit(&crate::lsp::WorkspaceEdit { changes }, true);
                }
            }
            return;
        };

        let text = self.editor.get_full_text();
        let main_start =
            crate::lsp::lsp_pos_to_offset(&text, main_edit.start_line, main_edit.start_col);
        let mut target_cursor = main_start + main_edit.new_text.len();
        let mut changes = item.additional_text_edits.clone();
        changes.push(main_edit);

        let mut ops = Vec::with_capacity(changes.len());
        for change in &changes {
            let start = crate::lsp::lsp_pos_to_offset(&text, change.start_line, change.start_col);
            let end = crate::lsp::lsp_pos_to_offset(&text, change.end_line, change.end_col);
            ops.push((start, end, change.new_text.clone()));
        }
        ops.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));

        for (start, end, new_text) in &ops {
            if *start <= *end {
                let (off, len, _) = self.editor.replace_range(*start, *end, new_text);
                self.highlighter.shift_delete(off, len);
                self.highlighter
                    .shift_insert(off, new_text.len(), Some(new_text));
            }
        }

        for (start, end, new_text) in &ops {
            if *end <= main_start {
                let delta = new_text.len() as isize - (*end - *start) as isize;
                target_cursor = ((target_cursor as isize) + delta).max(0) as usize;
            }
        }
        self.editor.cursor = target_cursor.min(self.editor.len());
        self.editor.selection_anchor = None;
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

    pub(crate) fn python_member_dot_receiver_is_unavailable_self(&self) -> bool {
        if self.file_extension != "py" {
            return false;
        }
        let Some(receiver) = python_member_receiver_before_cursor(&self.editor) else {
            return false;
        };
        if !matches!(receiver.as_str(), "self" | "cls") {
            return false;
        }
        let cursor = self.editor.cursor.min(self.editor.len());
        let lookup_cursor = if cursor > 0 && self.editor.byte_at(cursor - 1) == b'.' {
            cursor - 1
        } else {
            cursor
        };
        !self.highlighter.completions.iter().any(|item| {
            item.word == receiver
                && item.kind == SymbolKind::Parameter
                && lookup_cursor >= item.scope_start
                && lookup_cursor <= item.scope_end
        })
    }
}
