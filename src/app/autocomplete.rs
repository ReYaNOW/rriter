use super::*;

impl App {
    pub(crate) fn nearest_assignment_usage_target(
        &self,
        source_range: (usize, usize),
    ) -> Option<DefinitionJumpTarget> {
        let usage = nearest_python_assignment_usage(&self.editor, source_range)?;
        let (line, col) = crate::lsp::offset_to_lsp_pos(
            &self.editor.get_full_text(),
            usage,
            &self.editor.line_offsets,
        );
        Some(DefinitionJumpTarget {
            path: self.current_abs_path()?,
            line,
            col,
        })
    }

    pub fn get_current_word_prefix(&self) -> String {
        let mut p = self.editor.cursor;
        while p > 0 {
            let b = self.editor.byte_at(p - 1);
            if !(b.is_ascii_alphanumeric() || b == b'_') {
                break;
            }
            p -= 1;
        }
        if p == self.editor.cursor {
            return String::new();
        }
        let len = self.editor.cursor - p;
        let mut res = Vec::with_capacity(len);
        for i in p..self.editor.cursor {
            res.push(self.editor.byte_at(i));
        }
        String::from_utf8(res).unwrap_or_default()
    }

    pub fn close_autocomplete(&mut self) {
        self.autocomplete_active = false;
        self.autocomplete_selected_idx = 0;
        self.autocomplete_hovered_idx = None;
        self.autocomplete_scroll.current = 0.0;
        self.autocomplete_scroll.target = 0.0;
        self.autocomplete_rect = None;
        self.autocomplete_anchor = None;
        self.autocomplete_pending_request_id = None;
        self.autocomplete_detail_request_id = None;
        self.autocomplete_detail_word = None;
        self.autocomplete_detail_popup = None;
        self.autocomplete_detail_rect = None;
        self.autocomplete_detail_placement = None;
        self.autocomplete_detail_max_scroll = 0.0;
        self.autocomplete_min_width = 0.0;
        self.autocomplete_detail_selection_anchor = None;
        self.autocomplete_detail_selection_cursor = None;
        self.autocomplete_detail_selecting = false;
        self.autocomplete_apply_pending_response = false;
    }

    pub fn autocomplete_detail_selection(&self) -> Option<(usize, usize)> {
        let a = self.autocomplete_detail_selection_anchor?;
        let b = self.autocomplete_detail_selection_cursor?;
        (a != b).then_some((a.min(b), a.max(b)))
    }

    pub fn selected_autocomplete_detail_text(&self) -> Option<String> {
        let popup = self.autocomplete_detail_popup.as_ref()?;
        let (start, end) = self.autocomplete_detail_selection()?;
        if start < end
            && end <= popup.text.len()
            && popup.text.is_char_boundary(start)
            && popup.text.is_char_boundary(end)
        {
            Some(popup.text[start..end].to_string())
        } else {
            None
        }
    }

    pub(crate) fn autocomplete_detail_text<'a>(
        item: &AutocompleteItem,
        detail: &'a str,
    ) -> std::borrow::Cow<'a, str> {
        let Some(owner) = item
            .module
            .as_deref()
            .filter(|module| !is_type_like_completion_source(module))
        else {
            return std::borrow::Cow::Borrowed(detail);
        };
        if detail.contains(owner) || detail.contains(&format!(".{}", item.word)) {
            return std::borrow::Cow::Borrowed(detail);
        }
        for prefix in ["(variable) ", "(parameter) "] {
            if let Some(rest) = detail.strip_prefix(prefix) {
                if rest.starts_with(&item.word) {
                    return std::borrow::Cow::Owned(format!("{prefix}{owner}.{rest}"));
                }
            }
        }
        std::borrow::Cow::Borrowed(detail)
    }

    pub(crate) fn autocomplete_has_only_current_text_match(&self) -> bool {
        let prefix = self.get_current_word_prefix();
        !prefix.is_empty()
            && self.autocomplete_options.len() == 1
            && self.autocomplete_options[0].0.word == prefix
    }

    pub(crate) fn hide_autocomplete_popup_keep_request(&mut self) {
        self.autocomplete_active = false;
        self.autocomplete_options.clear();
        self.autocomplete_selected_idx = 0;
        self.autocomplete_hovered_idx = None;
        self.autocomplete_scroll.current = 0.0;
        self.autocomplete_scroll.target = 0.0;
        self.autocomplete_rect = None;
        self.autocomplete_anchor = None;
        self.autocomplete_detail_request_id = None;
        self.autocomplete_detail_word = None;
        self.autocomplete_detail_popup = None;
        self.autocomplete_detail_rect = None;
        self.autocomplete_detail_placement = None;
        self.autocomplete_detail_max_scroll = 0.0;
        self.autocomplete_min_width = 0.0;
        self.autocomplete_detail_selection_anchor = None;
        self.autocomplete_detail_selection_cursor = None;
        self.autocomplete_detail_selecting = false;
        self.autocomplete_apply_pending_response = false;
    }

    pub fn autocomplete_window_contains(&self, x: f32, y: f32) -> bool {
        if !self.autocomplete_active {
            return false;
        }

        [self.autocomplete_rect, self.autocomplete_detail_rect]
            .into_iter()
            .flatten()
            .any(|(rx, ry, rw, rh)| {
                rw > 0.0 && rh > 0.0 && x >= rx && x <= rx + rw && y >= ry && y <= ry + rh
            })
    }

    pub fn popup_blocks_background_at(&self, x: f32, y: f32) -> bool {
        self.file_tree_overlay_active() || self.autocomplete_window_contains(x, y)
    }

    pub fn refresh_autocomplete_detail_popup(&mut self) {
        let idx = self.autocomplete_selected_idx;
        let Some((item, _)) = self.autocomplete_options.get(idx) else {
            self.autocomplete_detail_popup = None;
            self.autocomplete_detail_rect = None;
            return;
        };
        let Some(detail) = item.detail.as_ref().filter(|s| !s.trim().is_empty()) else {
            self.autocomplete_detail_popup = None;
            self.autocomplete_detail_rect = None;
            return;
        };
        let module_path = autocomplete_detail_module_path(item).map(str::to_string);
        let detail_text = Self::autocomplete_detail_text(item, detail);
        let expected_text = module_path
            .as_ref()
            .map(|module_path| format!("[[MODULE]] {module_path}\n{}", detail_text.as_ref()))
            .unwrap_or_else(|| detail_text.to_string());
        let (text, spans, line_kinds, inline_code_ranges) = {
            if self
                .autocomplete_detail_popup
                .as_ref()
                .is_some_and(|popup| popup.text == expected_text)
            {
                return;
            }
            crate::lsp::highlight_hover_text(detail_text.as_ref())
        };
        let mut popup = crate::app::mouse::HoverPopup {
            text,
            spans,
            line_kinds,
            inline_code_ranges,
            byte_offset: self.editor.cursor,
            anchor_x: 0.0,
            anchor_y: 0.0,
            offset_x: Some(0.0),
            offset_y: Some(0.0),
            anim_progress: self.autocomplete_anim_progress,
            scroll: crate::scroll::ScrollState::new(15.0),
            layout_cache: None,
        };
        if let Some(module_path) = module_path {
            prepend_autocomplete_detail_module_path(&mut popup, &module_path);
        }
        self.autocomplete_detail_popup = Some(popup);
        self.autocomplete_detail_rect = None;
        self.autocomplete_detail_placement = None;
        self.autocomplete_detail_max_scroll = 0.0;
        self.autocomplete_detail_selection_anchor = None;
        self.autocomplete_detail_selection_cursor = None;
        self.autocomplete_detail_selecting = false;
    }

    pub fn request_autocomplete_detail_for_index(&mut self, idx: usize) {
        if self.autocomplete_mode != AutocompleteMode::TreeSitter {
            self.refresh_autocomplete_detail_popup();
            return;
        }
        let Some((item, _)) = self.autocomplete_options.get(idx) else {
            return;
        };
        if item.detail.is_some() {
            self.refresh_autocomplete_detail_popup();
            return;
        }
        if self.autocomplete_detail_word.as_deref() == Some(item.word.as_str())
            && self.autocomplete_detail_request_id.is_some()
        {
            return;
        }
        let Some(path) = self.file_path.clone() else {
            return;
        };
        let Some(lsp) = self.lsp.as_mut() else {
            return;
        };
        let text = self.editor.get_full_text();
        let (line, col) =
            crate::lsp::offset_to_lsp_pos(&text, self.editor.cursor, &self.editor.line_offsets);
        if let Some(id) = lsp.request_ty_completion(&path, &self.file_extension, line, col, None) {
            self.autocomplete_detail_request_id = Some(id);
            self.autocomplete_detail_word = Some(item.word.clone());
        }
    }

    pub fn merge_autocomplete_details(&mut self, items: Vec<crate::lsp::LspCompletionItem>) {
        let Some(target) = self.autocomplete_detail_word.clone() else {
            return;
        };
        let mut details: FxHashMap<
            String,
            (SymbolKind, Option<String>, Option<String>, Option<String>),
        > = FxHashMap::default();
        for item in items {
            let module_path = item.module.clone();
            details
                .entry(item.label)
                .or_insert((item.kind, item.detail, item.module, module_path));
        }
        let mut target_changed = false;
        let member_dot_context = cursor_after_python_member_dot(&self.editor);
        for (item, _) in &mut self.autocomplete_options {
            let Some((kind, detail, module, module_path)) = details.get(&item.word) else {
                continue;
            };
            let incoming_kind = completion_detail_kind(*kind, detail.as_deref());
            item.kind = incoming_kind;
            if item.detail.is_none() && detail.is_some() {
                item.detail = detail.clone();
            }
            if !member_dot_context
                && (matches!(
                    incoming_kind,
                    SymbolKind::Variable | SymbolKind::Parameter | SymbolKind::Property
                ) || completion_is_lowercase_type_source(
                    &item.word,
                    module.as_deref(),
                    detail.as_deref(),
                ))
            {
                if completion_is_lowercase_type_source(
                    &item.word,
                    module.as_deref(),
                    detail.as_deref(),
                ) {
                    item.kind = SymbolKind::Variable;
                }
                item.module = None;
                item.module_path = None;
                if item.word == target {
                    target_changed = true;
                }
                continue;
            }
            if item.module_path.is_none() && module_path.is_some() {
                item.module_path = module_path.clone();
            }
            if let Some(module) = module
                .as_deref()
                .filter(|module| should_replace_completion_module(item.module.as_deref(), module))
            {
                item.module = Some(module.to_string());
            }
            assign_builtin_completion_module(item);
            if item.word == target {
                target_changed = true;
            }
        }
        if target_changed {
            self.refresh_autocomplete_detail_popup();
        }
        self.autocomplete_detail_request_id = None;
        self.autocomplete_detail_word = None;
    }

    pub fn request_ty_autocomplete(&mut self, mode: AutocompleteMode, trigger: Option<&str>) {
        if !self.is_ide_mode || self.show_welcome {
            return;
        }
        if mode == AutocompleteMode::TyContext
            && (python_member_chain_too_deep(&self.editor)
                || trigger.is_none()
                    && self.get_current_word_prefix().is_empty()
                    && !cursor_after_python_member_dot(&self.editor))
        {
            self.close_autocomplete();
            return;
        }
        if mode == AutocompleteMode::TyImports && self.get_current_word_prefix().is_empty() {
            self.autocomplete_mode = mode;
            self.autocomplete_active = true;
            self.autocomplete_options.clear();
            self.autocomplete_selected_idx = 0;
            self.autocomplete_pending_request_id = None;
            self.autocomplete_anim_progress = 0.0;
            self.autocomplete_scroll.current = 0.0;
            self.autocomplete_scroll.target = 0.0;
            self.autocomplete_anchor = None;
            return;
        }
        let Some(path) = self.file_path.clone() else {
            return;
        };
        let hide_exact_match =
            mode == AutocompleteMode::TyContext && self.autocomplete_has_only_current_text_match();
        let Some(lsp) = self.lsp.as_mut() else {
            return;
        };
        let text = self.editor.get_full_text();
        lsp.notify_change(
            &path,
            &self.file_extension,
            &text,
            self.editor.version as i32,
        );
        let (line, col) =
            crate::lsp::offset_to_lsp_pos(&text, self.editor.cursor, &self.editor.line_offsets);
        if let Some(id) = lsp.request_ty_completion(&path, &self.file_extension, line, col, trigger)
        {
            let prefix = self.get_current_word_prefix();
            let context_key = ty_autocomplete_context_key(
                &text,
                &self.editor.line_offsets,
                self.editor.cursor,
                &prefix,
                mode,
            );
            let cached_items = self
                .autocomplete_cache
                .as_ref()
                .filter(|cache| {
                    cache.mode == mode && cache.path == path && cache.context_key == context_key
                })
                .map(|cache| cache.items.clone());
            self.autocomplete_mode = mode;
            self.autocomplete_pending_request_id = Some(id);
            self.autocomplete_apply_pending_response = false;
            if let Some(items) = cached_items {
                self.update_ty_autocomplete(items);
                self.autocomplete_pending_request_id = Some(id);
            } else if hide_exact_match {
                self.hide_autocomplete_popup_keep_request();
            } else if !self.autocomplete_active {
                self.autocomplete_active = true;
                self.autocomplete_anim_progress = 0.0;
                self.autocomplete_scroll.current = 0.0;
                self.autocomplete_scroll.target = 0.0;
                self.autocomplete_anchor = None;
            } else {
                self.autocomplete_active = true;
            }
        }
    }

    pub fn remember_ty_autocomplete_cache(&mut self, items: Vec<crate::lsp::LspCompletionItem>) {
        let Some(path) = self.file_path.clone() else {
            return;
        };
        let text = self.editor.get_full_text();
        let prefix = self.get_current_word_prefix();
        self.autocomplete_cache = Some(AutocompleteCacheEntry {
            mode: self.autocomplete_mode,
            path,
            context_key: ty_autocomplete_context_key(
                &text,
                &self.editor.line_offsets,
                self.editor.cursor,
                &prefix,
                self.autocomplete_mode,
            ),
            items,
        });
    }

    pub fn update_ty_autocomplete(&mut self, items: Vec<crate::lsp::LspCompletionItem>) {
        let prefix = self.get_current_word_prefix();
        if self.autocomplete_mode == AutocompleteMode::TyContext
            && (python_member_chain_too_deep(&self.editor)
                || prefix.is_empty() && !cursor_after_python_member_dot(&self.editor))
        {
            self.close_autocomplete();
            return;
        }
        if self.autocomplete_mode == AutocompleteMode::TyImports && prefix.is_empty() {
            self.autocomplete_options.clear();
            self.autocomplete_active = true;
            return;
        }

        let mut items: Vec<AutocompleteItem> =
            items.into_iter().map(AutocompleteItem::from).collect();
        let current_text = self.editor.get_full_text();
        let imported_modules =
            (self.file_extension == "py").then(|| imported_python_symbols(&current_text));
        let member_dot_context = cursor_after_python_member_dot(&self.editor);
        let common_owner = (self.autocomplete_mode == AutocompleteMode::TyContext
            && member_dot_context)
            .then(|| common_completion_owner(&items))
            .flatten();
        let common_owner_module = common_owner.as_ref().and_then(|owner| {
            imported_modules
                .as_ref()
                .and_then(|imports| imports.get(owner))
                .map(String::as_str)
        });
        let source_attr_words: FxHashSet<String> = if common_owner.is_some() {
            items
                .iter()
                .filter(|item| completion_needs_python_source_attr_owner(item))
                .map(|item| item.word.clone())
                .collect()
        } else {
            FxHashSet::default()
        };
        let inherited_owner_source = if !source_attr_words.is_empty() {
            common_owner.as_ref().and_then(|owner| {
                imported_python_class_source(
                    &current_text,
                    &self.ide_workspaces,
                    self.file_path.as_deref(),
                    owner,
                )
            })
        } else {
            None
        };
        let source_attr_owners = if let (Some(owner), Some(source)) =
            (common_owner.as_deref(), inherited_owner_source.as_deref())
        {
            python_class_attr_owners_with_imports(
                source,
                &self.ide_workspaces,
                self.file_path.as_deref(),
                owner,
                &source_attr_words,
            )
        } else {
            FxHashMap::default()
        };
        for item in &mut items {
            item.kind = completion_detail_kind(item.kind, item.detail.as_deref());
            if self.autocomplete_mode == AutocompleteMode::TyImports {
                normalize_ty_import_kind(item);
            }
            if self.autocomplete_mode == AutocompleteMode::TyContext {
                if item
                    .module_path
                    .as_deref()
                    .is_some_and(|source| !completion_source_is_module_path(source))
                {
                    item.module_path = None;
                }
                if item
                    .module
                    .as_deref()
                    .is_some_and(|source| !completion_source_label_is_clean(source))
                {
                    item.module = None;
                }
                if !member_dot_context {
                    if let Some(module) = imported_modules
                        .as_ref()
                        .and_then(|imports| imports.get(&item.word))
                    {
                        item.module = Some(module.clone());
                        item.module_path.get_or_insert_with(|| module.clone());
                    } else {
                        assign_builtin_completion_module(item);
                    }
                }
                if completion_item_is_argument_like(item) {
                    item.kind = SymbolKind::Parameter;
                }
                let source_attr_owner = if member_dot_context {
                    source_attr_owners.get(&item.word).cloned()
                } else {
                    None
                };
                if completion_item_is_field_like(item) || source_attr_owner.is_some() {
                    if !member_dot_context {
                        item.kind = SymbolKind::Variable;
                        item.module = None;
                        item.module_path = None;
                    } else if let Some(owner) = source_attr_owner {
                        item.kind = SymbolKind::Variable;
                        set_completion_owner_source(
                            item,
                            owner,
                            imported_modules.as_ref(),
                            common_owner_module,
                        );
                    } else if let Some(owner) = item
                        .detail
                        .as_deref()
                        .and_then(|detail| completion_owner_from_detail(&item.word, detail))
                        .and_then(|owner| {
                            completion_owner_label_from_source(owner)
                                .or_else(|| Some(owner.to_string()))
                        })
                    {
                        set_completion_owner_source(
                            item,
                            owner,
                            imported_modules.as_ref(),
                            common_owner_module,
                        );
                    } else if !completion_item_has_explicit_owner(item) {
                        if let Some(owner) = source_attr_owners.get(&item.word).cloned() {
                            set_completion_owner_source(
                                item,
                                owner,
                                imported_modules.as_ref(),
                                common_owner_module,
                            );
                        } else if inherited_owner_source.is_none() {
                            if let Some(owner) = common_owner.as_ref() {
                                set_completion_owner_source(
                                    item,
                                    owner.clone(),
                                    imported_modules.as_ref(),
                                    common_owner_module,
                                );
                            }
                        } else {
                            item.module = None;
                            item.module_path = None;
                        }
                    }
                    if item
                        .module
                        .as_deref()
                        .is_some_and(is_type_like_completion_source)
                        || completion_item_source_is_field_type(item)
                    {
                        item.module = None;
                    }
                } else {
                    if !member_dot_context {
                        if let Some(module) = completion_parent_module_label(item) {
                            item.module = Some(module);
                        }
                    } else if let Some(owner) = completion_item_owner_label(item) {
                        set_completion_owner_source(
                            item,
                            owner,
                            imported_modules.as_ref(),
                            common_owner_module,
                        );
                    } else if item
                        .module
                        .as_deref()
                        .is_some_and(is_type_like_completion_source)
                        || item.module.is_none()
                    {
                        item.module = common_owner.clone();
                    }
                }
            }
        }

        let prefix_lower = prefix.to_lowercase();
        let mut seen = FxHashMap::default();
        let mut matches = Vec::new();

        for item in items {
            if self.autocomplete_mode == AutocompleteMode::TyImports && item.module.is_none() {
                continue;
            }
            let key = (item.word.clone(), item.module.clone().unwrap_or_default());
            if seen.insert(key, ()).is_some() {
                continue;
            }
            let word_lower = item.word.to_lowercase();
            let indices = if prefix.is_empty() {
                Vec::new()
            } else if let Some(indices) = fuzzy_match(&prefix_lower, &word_lower) {
                indices
            } else {
                continue;
            };
            let is_prefix = prefix.is_empty() || word_lower.starts_with(&prefix_lower);
            matches.push((is_prefix, item.word.len(), item, indices));
        }

        let prefer_call_arguments = self.autocomplete_mode == AutocompleteMode::TyContext
            && cursor_inside_python_call_parens(&self.editor)
            && !cursor_after_python_member_dot(&self.editor);
        matches.sort_unstable_by_key(|(is_prefix, len, item, _)| {
            let arg_priority = if prefer_call_arguments && completion_item_is_argument_like(item) {
                0
            } else {
                1
            };
            let type_priority = match item.kind {
                SymbolKind::Parameter => 0,
                SymbolKind::Property | SymbolKind::Variable => 1,
                SymbolKind::Function => 2,
                SymbolKind::Class | SymbolKind::Module => 3,
                SymbolKind::Builtin => 4,
                SymbolKind::Keyword => 5,
                SymbolKind::Unknown => 6,
            };
            (
                !*is_prefix,
                arg_priority,
                is_magic_python_name(&item.word),
                type_priority,
                *len,
            )
        });

        let was_empty_or_inactive =
            self.autocomplete_options.is_empty() || !self.autocomplete_active;

        self.autocomplete_options = matches
            .into_iter()
            .take(80)
            .map(|(_, _, item, indices)| (item, indices))
            .collect();
        if self.autocomplete_options.len() == 1
            && !prefix.is_empty()
            && self.autocomplete_options[0].0.word == prefix
        {
            self.autocomplete_options.clear();
            self.close_autocomplete();
            return;
        }

        if was_empty_or_inactive && !self.autocomplete_options.is_empty() {
            self.autocomplete_anim_progress = 0.0;
        }

        self.autocomplete_active = !self.autocomplete_options.is_empty()
            || self.autocomplete_mode == AutocompleteMode::TyImports;
        if !self.autocomplete_active {
            self.autocomplete_detail_popup = None;
            self.autocomplete_detail_rect = None;
            self.autocomplete_detail_placement = None;
            self.autocomplete_detail_max_scroll = 0.0;
            return;
        }
        self.autocomplete_selected_idx = 0;
        self.autocomplete_hovered_idx = None;
        self.autocomplete_scroll.current = 0.0;
        self.autocomplete_scroll.target = 0.0;
        self.refresh_autocomplete_detail_popup();
        if self.autocomplete_apply_pending_response {
            self.autocomplete_apply_pending_response = false;
            self.apply_autocomplete();
            return;
        }
        if self.autocomplete_mode == AutocompleteMode::TreeSitter {
            self.request_autocomplete_detail_for_index(0);
        }
    }

    pub fn update_autocomplete(&mut self) {
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
            }
            return;
        }
        let prefix = self.get_current_word_prefix();
        if prefix.is_empty() {
            self.autocomplete_active = false;
            self.autocomplete_options.clear();
            self.autocomplete_mode = AutocompleteMode::TreeSitter;
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
                    if current_size < ex_size {
                        best_scopes.insert(comp.word.clone(), comp.clone());
                    }
                } else {
                    best_scopes.insert(comp.word.clone(), comp.clone());
                }
            }
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
        if self.autocomplete_options.len() == 1 && self.autocomplete_options[0].0.word == prefix {
            self.autocomplete_options.clear();
        }
        if self.file_extension == "py" {
            for (item, _) in &mut self.autocomplete_options {
                assign_builtin_completion_module(item);
                if item.kind == SymbolKind::Builtin {
                    if item
                        .word
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_uppercase())
                    {
                        item.kind = SymbolKind::Class;
                    } else if matches!(
                        item.word.as_str(),
                        "bool"
                            | "int"
                            | "float"
                            | "str"
                            | "list"
                            | "dict"
                            | "set"
                            | "tuple"
                            | "bytes"
                            | "type"
                            | "object"
                            | "complex"
                    ) {
                        item.kind = SymbolKind::Class;
                    } else {
                        item.kind = SymbolKind::Function;
                    }
                }
            }
        }
        if self.file_extension == "py" && !self.autocomplete_options.is_empty() {
            let imports = imported_python_symbols(&self.editor.get_full_text());
            apply_import_modules_to_autocomplete_items(&mut self.autocomplete_options, &imports);
        }

        if !self.autocomplete_options.is_empty() {
            if !self.autocomplete_active {
                self.autocomplete_anim_progress = 0.0;
                self.autocomplete_scroll.current = 0.0;
                self.autocomplete_scroll.target = 0.0;
                self.autocomplete_anchor = None;
            }
            self.autocomplete_mode = AutocompleteMode::TreeSitter;
            self.autocomplete_active = true;
            self.autocomplete_selected_idx = 0;
            self.refresh_autocomplete_detail_popup();
            self.request_autocomplete_detail_for_index(0);
        } else {
            self.autocomplete_active = false;
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
}
