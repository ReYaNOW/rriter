fn python_current_call_named_args(text: &str, cursor: usize) -> FxHashSet<String> {
    let bytes = text.as_bytes();
    let cursor = cursor.min(bytes.len());
    let mut depth = 0usize;
    let mut open = None;
    for idx in (0..cursor).rev() {
        match bytes[idx] {
            b')' | b']' | b'}' => depth += 1,
            b'(' => {
                if depth == 0 {
                    open = Some(idx + 1);
                    break;
                }
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }
    let Some(open) = open else {
        return FxHashSet::default();
    };
    let mut out = FxHashSet::default();
    let mut p = open;
    while p < cursor {
        while p < cursor && !is_python_ident_byte(bytes[p]) {
            p += 1;
        }
        let start = p;
        while p < cursor && is_python_ident_byte(bytes[p]) {
            p += 1;
        }
        if start == p {
            continue;
        }
        let mut q = p;
        while q < cursor && matches!(bytes[q], b' ' | b'\t') {
            q += 1;
        }
        if q < cursor
            && bytes[q] == b'='
            && start > open
            && bytes[start - 1] != b'.'
            && let Some(name) = text.get(start..p)
        {
            out.insert(name.to_string());
        }
    }
    out
}

pub(crate) fn ty_signature_parameter_items(
    names: Vec<String>,
    text: &str,
    cursor: usize,
) -> Vec<crate::lsp::LspCompletionItem> {
    let used = python_current_call_named_args(text, cursor);
    let mut seen = FxHashSet::default();
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        if used.contains(&name) || !seen.insert(name.clone()) {
            continue;
        }
        out.push(crate::lsp::LspCompletionItem {
            label: name.clone(),
            kind: SymbolKind::Argument,
            module: None,
            detail: Some(format!("(parameter) {name}")),
            insert_text: Some(format!("{name}=")),
            text_edit: None,
            additional_text_edits: Vec::new(),
        });
    }
    out
}

fn ty_auto_import_completion(item: &AutocompleteItem) -> bool {
    !item.additional_text_edits.is_empty()
        || item
            .detail
            .as_deref()
            .is_some_and(|detail| detail.trim_start().starts_with("(import "))
}

impl App {
    pub fn request_ty_autocomplete(&mut self, mode: AutocompleteMode, trigger: Option<&str>) {
        if autocomplete_trace_enabled() {
            println!(
                "Autocomplete request_ty: mode={:?} trigger={:?} active={} opts={} pending={:?}",
                mode,
                trigger,
                self.autocomplete_active,
                self.autocomplete_options.len(),
                self.autocomplete_pending_request_id
            );
        }
        self.trace_autocomplete_state("request_ty:begin");
        if !self.is_ide_mode || self.show_welcome {
            self.trace_autocomplete_state("request_ty:not_ide");
            return;
        }
        if mode == AutocompleteMode::TyContext
            && self.python_member_dot_receiver_is_unavailable_self()
        {
            self.close_autocomplete();
            self.trace_autocomplete_state("request_ty:unscoped_self_member");
            return;
        }
        if mode == AutocompleteMode::TyContext
            && (python_member_chain_too_deep(&self.editor)
                || trigger.is_none()
                    && self.get_current_word_prefix().is_empty()
                    && !cursor_after_python_member_dot(&self.editor)
                    && !cursor_inside_python_call_parens(&self.editor))
        {
            self.close_autocomplete();
            self.trace_autocomplete_state("request_ty:blocked_context");
            return;
        }
        if mode == AutocompleteMode::TyImports && self.get_current_word_prefix().is_empty() {
            self.autocomplete_mode = mode;
            self.autocomplete_active = true;
            self.autocomplete_options.clear();
            self.autocomplete_selected_idx = 0;
            self.autocomplete_pending_request_id = None;
            self.autocomplete_pending_request_mode = None;
            self.autocomplete_pending_request_path = None;
            self.autocomplete_pending_context_key = None;
            self.autocomplete_anim_progress = 0.0;
            self.autocomplete_scroll.current = 0.0;
            self.autocomplete_scroll.target = 0.0;
            self.autocomplete_anchor = None;
            self.trace_autocomplete_state("request_ty:imports_empty_prefix");
            return;
        }
        let Some(path) = self.file_path.clone() else {
            self.trace_autocomplete_state("request_ty:no_path");
            return;
        };
        let hide_exact_match =
            mode == AutocompleteMode::TyContext && self.autocomplete_has_only_current_text_match();
        let text = self.editor.get_full_text();
        let prefix = self.get_current_word_prefix();
        let context_key = ty_autocomplete_context_key(
            &text,
            &self.editor.line_offsets,
            self.editor.cursor,
            &prefix,
            mode,
        );
        let cacheable_response = prefix.is_empty();
        if let Some(items) = self
            .autocomplete_cache
            .as_ref()
            .filter(|cache| {
                cache.mode == mode && cache.path == path && cache.context_key == context_key
            })
            .map(|cache| cache.items.clone())
        {
            self.autocomplete_mode = mode;
            self.autocomplete_pending_request_id = None;
            self.autocomplete_pending_request_mode = None;
            self.autocomplete_pending_request_path = None;
            self.autocomplete_pending_context_key = None;
            self.autocomplete_apply_pending_response = false;
            self.update_ty_autocomplete(items);
            self.trace_autocomplete_state("request_ty:cached_end");
            return;
        }
        let Some(lsp) = self.lsp.as_mut() else {
            self.trace_autocomplete_state("request_ty:no_lsp");
            return;
        };
        lsp.notify_change(
            &path,
            &self.file_extension,
            &text,
            self.editor.version as i32,
        );
        let (line, col) =
            crate::lsp::offset_to_lsp_pos(&text, self.editor.cursor, &self.editor.line_offsets);
        let request_signature_help = mode == AutocompleteMode::TyContext
            && cursor_inside_python_call_parens(&self.editor)
            && !cursor_after_python_member_dot(&self.editor);
        let completion_id = if request_signature_help {
            None
        } else {
            lsp.request_ty_completion(&path, &self.file_extension, line, col, trigger)
        };
        let signature_id = if request_signature_help {
            lsp.request_ty_signature_help(&path, &self.file_extension, line, col, None)
        } else {
            None
        };
        if request_signature_help {
            self.autocomplete_signature_items.clear();
        } else {
            self.autocomplete_signature_request_id = None;
            self.autocomplete_signature_items.clear();
        }
        if let Some(id) = completion_id {
            if autocomplete_trace_enabled() {
                println!(
                    "Autocomplete request_ty_sent: id={} cached={} hide_exact={} context_key_len={} line={} col={}",
                    id,
                    0,
                    hide_exact_match,
                    context_key.len(),
                    line,
                    col
                );
            }
            self.autocomplete_mode = mode;
            self.autocomplete_pending_request_id = Some(id);
            if cacheable_response {
                self.autocomplete_pending_request_mode = Some(mode);
                self.autocomplete_pending_request_path = Some(path);
                self.autocomplete_pending_context_key = Some(context_key);
            } else {
                self.autocomplete_pending_request_mode = None;
                self.autocomplete_pending_request_path = None;
                self.autocomplete_pending_context_key = None;
            }
            self.autocomplete_apply_pending_response = false;
            if hide_exact_match {
                self.hide_autocomplete_popup_keep_request();
            } else {
                self.autocomplete_detail_popup = None;
                self.autocomplete_detail_rect = None;
            }
            self.trace_autocomplete_state("request_ty:end");
        }
        if let Some(id) = signature_id {
            self.autocomplete_mode = mode;
            self.autocomplete_signature_request_id = Some(id);
            if completion_id.is_none() {
                self.autocomplete_pending_request_id = None;
                self.autocomplete_pending_request_mode = None;
                self.autocomplete_pending_request_path = None;
                self.autocomplete_pending_context_key = None;
                self.autocomplete_apply_pending_response = false;
                self.autocomplete_active = false;
                self.autocomplete_options.clear();
                self.autocomplete_detail_popup = None;
                self.autocomplete_detail_rect = None;
                self.trace_autocomplete_state("request_ty:signature_end");
            }
        }
    }

    pub fn remember_ty_autocomplete_cache(
        &mut self,
        mut items: Vec<crate::lsp::LspCompletionItem>,
    ) {
        let (Some(mode), Some(path), Some(context_key)) = (
            self.autocomplete_pending_request_mode,
            self.autocomplete_pending_request_path.clone(),
            self.autocomplete_pending_context_key.clone(),
        ) else {
            return;
        };
        if items.len() > AUTOCOMPLETE_CACHE_MAX_ITEMS {
            items.truncate(AUTOCOMPLETE_CACHE_MAX_ITEMS);
        }
        self.autocomplete_cache = Some(AutocompleteCacheEntry {
            mode,
            path,
            context_key,
            items,
        });
        self.autocomplete_pending_request_mode = None;
        self.autocomplete_pending_request_path = None;
        self.autocomplete_pending_context_key = None;
    }

    pub fn update_ty_autocomplete(&mut self, items: Vec<crate::lsp::LspCompletionItem>) {
        if self.api_mock_completion_focus().is_some() {
            self.update_api_mock_ty_autocomplete(items);
            return;
        }
        if autocomplete_trace_enabled() {
            println!(
                "Autocomplete update_ty: incoming={} mode={:?} active={} opts_before={} pending={:?}",
                items.len(),
                self.autocomplete_mode,
                self.autocomplete_active,
                self.autocomplete_options.len(),
                self.autocomplete_pending_request_id
            );
        }
        self.trace_autocomplete_state("update_ty:begin");
        let prefix = self.get_current_word_prefix();
        if self.autocomplete_mode == AutocompleteMode::TyContext
            && (python_member_chain_too_deep(&self.editor)
                || prefix.is_empty()
                    && !cursor_after_python_member_dot(&self.editor)
                    && !cursor_inside_python_call_parens(&self.editor))
        {
            self.close_autocomplete();
            self.trace_autocomplete_state("update_ty:blocked_context");
            return;
        }
        if self.autocomplete_mode == AutocompleteMode::TyImports && prefix.is_empty() {
            self.autocomplete_options.clear();
            self.autocomplete_active = true;
            self.trace_autocomplete_state("update_ty:imports_empty_prefix");
            return;
        }

        let mut items: Vec<AutocompleteItem> =
            items.into_iter().map(AutocompleteItem::from).collect();
        let current_text = self.editor.get_full_text();
        let python_context = python_completion_context(&self.file_extension, &current_text);
        let current_module_path = python_context
            .then(|| {
                self.file_path.as_deref().and_then(|path| {
                    crate::app::events::module_path_from_definition_path(
                        path,
                        &self.ide_workspaces,
                    )
                })
            })
            .flatten();
        let local_self_owner = python_context
            .then(|| python_enclosing_class_before_cursor(&current_text, self.editor.cursor))
            .flatten();
        let imported_modules = python_context.then(|| imported_python_symbols(&current_text));
        let member_dot_context = cursor_after_python_member_dot(&self.editor);
        let local_function_names = if current_module_path.is_some() && !member_dot_context {
            python_source_function_names(&current_text)
        } else {
            FxHashSet::default()
        };
        let call_argument_context = self.autocomplete_mode == AutocompleteMode::TyContext
            && cursor_inside_python_call_parens(&self.editor)
            && !member_dot_context;
        if call_argument_context && !self.autocomplete_signature_items.is_empty() {
            let mut merged = Vec::with_capacity(self.autocomplete_signature_items.len() + items.len());
            merged.extend(
                self.autocomplete_signature_items
                    .iter()
                    .cloned()
                    .map(AutocompleteItem::from),
            );
            merged.extend(items);
            items = merged;
        }
        if self.autocomplete_mode == AutocompleteMode::TyContext {
            items.retain(|item| !ty_auto_import_completion(item));
        }
        let common_owner = (self.autocomplete_mode == AutocompleteMode::TyContext
            && member_dot_context)
            .then(|| common_completion_owner(&items))
            .flatten();
        let receiver_owner = (self.autocomplete_mode == AutocompleteMode::TyContext
            && member_dot_context)
            .then(|| {
                python_member_dot_receiver(&current_text, self.editor.cursor).and_then(|receiver| {
                    if matches!(receiver, "self" | "cls") {
                        return local_self_owner.clone();
                    }
                    let imported = imported_modules
                        .as_ref()
                        .is_some_and(|imports| imports.contains_key(receiver));
                    (imported || is_class_like_type_name(receiver))
                        .then(|| receiver.rsplit('.').next().unwrap_or(receiver).to_string())
                })
            })
            .flatten();
        let fallback_owner = receiver_owner.as_deref().or(common_owner.as_deref());
        let member_owner = if receiver_owner.is_some() {
            receiver_owner.clone()
        } else if self.autocomplete_mode == AutocompleteMode::TyContext && member_dot_context {
            infer_python_member_owner(
                &current_text,
                imported_modules.as_ref(),
                &self.ide_workspaces,
                self.file_path.as_deref(),
                &items,
                fallback_owner,
            )
        } else {
            fallback_owner.map(str::to_string)
        };
        if autocomplete_trace_enabled() {
            println!(
                "Autocomplete update_ty_context: prefix={:?} member_dot={} common_owner={:?} member_owner={:?} current_text_len={}",
                prefix,
                member_dot_context,
                common_owner,
                member_owner,
                current_text.len()
            );
        }
        let common_owner_module = member_owner.as_ref().and_then(|owner| {
            imported_modules
                .as_ref()
                .and_then(|imports| imports.get(owner))
                .map(String::as_str)
        });
        let source_attr_words: FxHashSet<String> = if member_owner.is_some() {
            items
                .iter()
                .filter(|item| completion_needs_python_source_attr_owner(item))
                .map(|item| item.word.clone())
                .collect()
        } else {
            FxHashSet::default()
        };
        let member_owner_source = if member_dot_context {
            member_owner.as_ref().and_then(|owner| {
                imported_python_class_source(
                    &current_text,
                    &self.ide_workspaces,
                    self.file_path.as_deref(),
                    owner,
                )
                .or_else(|| {
                    let owner_label = owner.rsplit('.').next().unwrap_or(owner);
                    source_contains_python_class(&current_text, owner_label)
                        .then(|| current_text.clone())
                })
            })
        } else {
            None
        };
        let source_attr_owners = if let (Some(owner), Some(source)) =
            (member_owner.as_deref(), member_owner_source.as_deref())
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
        let source_member_words: FxHashSet<String> = if member_dot_context && member_owner.is_some()
        {
            items
                .iter()
                .filter(|item| item.kind == SymbolKind::Function)
                .map(|item| item.word.clone())
                .collect()
        } else {
            FxHashSet::default()
        };
        let source_member_owners = if let (Some(owner), Some(source)) =
            (member_owner.as_deref(), member_owner_source.as_deref())
        {
            python_class_member_owners_with_imports(
                source,
                &self.ide_workspaces,
                self.file_path.as_deref(),
                owner,
                &source_member_words,
            )
        } else {
            FxHashMap::default()
        };
        let member_owner_depths = if let (Some(owner), Some(source)) =
            (member_owner.as_deref(), member_owner_source.as_deref())
        {
            python_class_owner_depths_with_imports(
                source,
                &self.ide_workspaces,
                self.file_path.as_deref(),
                owner,
            )
        } else {
            let mut depths = FxHashMap::default();
            if member_dot_context && let Some(owner) = member_owner.as_ref() {
                depths.insert(owner.clone(), 0);
            }
            depths
        };
        for item in &mut items {
            if member_dot_context
                && item
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.trim_start().starts_with("Overload["))
                && let (Some(owner), Some(source)) =
                    (member_owner.as_deref(), member_owner_source.as_deref())
                && let Some(detail) = python_class_method_overload_detail(source, owner, &item.word)
            {
                item.detail = Some(detail);
            }
            item.kind = completion_detail_kind(item.kind, item.detail.as_deref());
            if python_known_function_completion(item) {
                item.kind = SymbolKind::Function;
            }
            if python_context
                && item.kind == SymbolKind::Unknown
                && python_keyword_completion(&item.word)
            {
                item.kind = SymbolKind::Keyword;
            }
            if self.autocomplete_mode == AutocompleteMode::TyImports {
                normalize_ty_import_kind(item);
            }
            if self.autocomplete_mode == AutocompleteMode::TyContext {
                if !member_dot_context
                    && matches!(item.kind, SymbolKind::Function | SymbolKind::Builtin)
                    && item.module.is_none()
                    && item.module_path.is_none()
                    && local_function_names.contains(&item.word)
                    && let Some(module) = current_module_path.as_ref()
                {
                    item.module = Some(module.clone());
                    item.module_path = Some(module.clone());
                }
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
                        if item.kind == SymbolKind::Unknown
                            && completion_word_starts_lower(&item.word)
                        {
                            item.kind = SymbolKind::Variable;
                        }
                    } else {
                        assign_builtin_completion_module(item);
                    }
                }
                if completion_item_is_argument_like(item) {
                    item.kind = if call_argument_context {
                        SymbolKind::Argument
                    } else {
                        SymbolKind::Parameter
                    };
                }
                if !member_dot_context
                    && matches!(item.word.as_str(), "self" | "cls")
                    && let Some(owner) = local_self_owner.as_ref()
                {
                    item.kind = SymbolKind::Parameter;
                    item.module = Some(owner.clone());
                    item.module_path = None;
                    continue;
                }
                let source_attr_owner = if member_dot_context {
                    source_attr_owners.get(&item.word).cloned()
                } else {
                    None
                };
                let top_level_import_module = (!member_dot_context)
                    .then(|| {
                        imported_modules
                            .as_ref()
                            .and_then(|imports| imports.get(&item.word))
                    })
                    .flatten();
                if completion_item_is_field_like(item) || source_attr_owner.is_some() {
                    if !member_dot_context {
                        if item.kind == SymbolKind::Argument {
                            item.module = None;
                            item.module_path = None;
                        } else {
                            item.kind = SymbolKind::Variable;
                            if let Some(module) = top_level_import_module {
                                item.module = Some(module.clone());
                                item.module_path.get_or_insert_with(|| module.clone());
                            } else {
                                item.module = None;
                                item.module_path = None;
                            }
                        }
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
                        } else if let Some(owner) = member_owner.as_ref() {
                            set_completion_owner_source(
                                item,
                                owner.clone(),
                                imported_modules.as_ref(),
                                common_owner_module,
                            );
                        } else {
                            item.module = None;
                            item.module_path = None;
                        }
                    }
                    let module_is_known_owner = item
                        .module
                        .as_deref()
                        .is_some_and(|module| member_owner_depths.contains_key(module));
                    if item
                        .module
                        .as_deref()
                        .is_some_and(is_type_like_completion_source)
                        || !module_is_known_owner && completion_item_source_is_field_type(item)
                    {
                        item.module = None;
                    }
                    if let Some(type_path) = autocomplete_detail_type_module_path(
                        item.detail.as_deref(),
                        imported_modules.as_ref(),
                        item.module_path.as_deref().or(item.module.as_deref()),
                    ) {
                        item.module_path = Some(type_path);
                    }
                } else {
                    if !member_dot_context {
                        if let Some(module) = completion_parent_module_label(item) {
                            item.module = Some(module);
                        }
                    } else if let Some(owner) = source_member_owners.get(&item.word).cloned() {
                        set_completion_owner_source(
                            item,
                            owner,
                            imported_modules.as_ref(),
                            common_owner_module,
                        );
                    } else if let Some(owner) = completion_item_owner_label(item).filter(|owner| {
                        receiver_owner.is_none() || {
                            let owner_label = owner.rsplit('.').next().unwrap_or(owner);
                            member_owner_depths.contains_key(owner_label)
                        }
                    }) {
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
                        || receiver_owner.is_some() && completion_item_owner_label(item).is_some()
                    {
                        if let Some(owner) = member_owner.clone() {
                            set_completion_owner_source(
                                item,
                                owner,
                                imported_modules.as_ref(),
                                common_owner_module,
                            );
                        }
                    }
                }
            }
        }
        if python_context
            && !member_dot_context
            && let Some(imports) = imported_modules.as_ref()
        {
            for item in &mut items {
                if let Some(module) = imports.get(&item.word) {
                    apply_import_module_to_autocomplete_item(item, module);
                }
            }
        }
        if autocomplete_trace_enabled() {
            let sample = items
                .iter()
                .take(12)
                .map(|item| {
                    format!(
                        "{}:{:?}:m={:?}:mp={:?}:d={:?}",
                        item.word, item.kind, item.module, item.module_path, item.detail
                    )
                })
                .collect::<Vec<_>>()
                .join(" | ");
            println!(
                "Autocomplete update_ty normalized: prefix={:?} python_context={} imports={} items={}",
                prefix,
                python_context,
                imported_modules
                    .as_ref()
                    .map(|imports| imports.len())
                    .unwrap_or(0),
                sample
            );
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

        let prefer_call_arguments = call_argument_context;
        matches.sort_unstable_by_key(|(is_prefix, len, item, _)| {
            let arg_priority = if prefer_call_arguments
                && (item.kind == SymbolKind::Argument || completion_item_is_argument_like(item))
            {
                0
            } else {
                1
            };
            let owner_depth = if member_dot_context {
                completion_item_owner_label(item)
                    .as_deref()
                    .and_then(|owner| member_owner_depths.get(owner))
                    .copied()
                    .unwrap_or(u8::MAX)
            } else {
                u8::MAX
            };
            let private_priority = member_dot_context && item.word.starts_with('_');
            let low_priority_member =
                member_dot_context && python_low_priority_member_name(&item.word);
            let type_priority = match item.kind {
                SymbolKind::Argument => 0,
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
                low_priority_member,
                owner_depth,
                private_priority,
                is_magic_python_name(&item.word),
                type_priority,
                *len,
            )
        });

        let was_inactive = !self.autocomplete_active;

        self.autocomplete_options = matches
            .into_iter()
            .take(80)
            .map(|(_, _, item, indices)| (item, indices))
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
                "Autocomplete update_ty_matches: opts={} first=[{}] was_inactive={}",
                self.autocomplete_options.len(),
                first,
                was_inactive
            );
        }
        if self.autocomplete_options.len() == 1
            && !prefix.is_empty()
            && self.autocomplete_options[0].0.word == prefix
        {
            self.autocomplete_options.clear();
            self.close_autocomplete();
            self.trace_autocomplete_state("update_ty:single_exact_close");
            return;
        }

        if was_inactive && !self.autocomplete_options.is_empty() {
            self.autocomplete_anim_progress = 0.0;
            self.autocomplete_anchor = None;
        }

        self.autocomplete_active = !self.autocomplete_options.is_empty()
            || self.autocomplete_mode == AutocompleteMode::TyImports;
        if !self.autocomplete_active {
            self.autocomplete_detail_popup = None;
            self.autocomplete_detail_rect = None;
            self.autocomplete_detail_placement = None;
            self.autocomplete_detail_max_scroll = 0.0;
            self.reset_autocomplete_detail_size();
            crate::app::events::reset_autocomplete_frame_stats();
            self.trace_autocomplete_state("update_ty:inactive_end");
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
            self.trace_autocomplete_state("update_ty:applied_pending");
            return;
        }
        if self.autocomplete_mode == AutocompleteMode::TreeSitter {
            self.request_autocomplete_detail_for_index(0);
        }
        self.trace_autocomplete_state("update_ty:end");
    }

    pub fn update_ty_signature_help_autocomplete(&mut self, parameters: Vec<String>) {
        if self.api_mock_completion_focus().is_some() {
            self.update_api_mock_ty_signature_help_autocomplete(parameters);
            return;
        }
        if self.autocomplete_mode != AutocompleteMode::TyContext
            || !cursor_inside_python_call_parens(&self.editor)
            || cursor_after_python_member_dot(&self.editor)
        {
            return;
        }
        let text = self.editor.get_full_text();
        self.autocomplete_signature_items =
            ty_signature_parameter_items(parameters, &text, self.editor.cursor);
        if !self.autocomplete_signature_items.is_empty() {
            self.update_ty_autocomplete(Vec::new());
        }
    }
}
