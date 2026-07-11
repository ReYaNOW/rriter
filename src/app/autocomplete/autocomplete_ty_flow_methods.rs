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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ActiveAutocompleteSource {
    MainEditor,
    ApiMock {
        spec_id: crate::app::api_client::ApiSpecId,
        route_idx: usize,
        part: crate::app::api_mock::ty_check::ApiMockSourcePart,
    },
}

impl ActiveAutocompleteSource {
    fn trace_label(self) -> &'static str {
        match self {
            Self::MainEditor => "main",
            Self::ApiMock { .. } => "api_mock",
        }
    }

    fn is_api_mock(self) -> bool {
        matches!(self, Self::ApiMock { .. })
    }

    fn cacheable(self) -> bool {
        matches!(self, Self::MainEditor)
    }
}

pub(crate) struct AutocompleteSourceSnapshot {
    pub(crate) source: ActiveAutocompleteSource,
    pub(crate) file_extension: String,
    pub(crate) visible_text: String,
    pub(crate) analysis_text: String,
    pub(crate) visible_cursor: usize,
    pub(crate) analysis_cursor: usize,
    pub(crate) path: Option<PathBuf>,
    pub(crate) line_offsets: Vec<usize>,
    pub(crate) version: i32,
}

impl AutocompleteSourceSnapshot {
    pub(crate) fn current_word_prefix(&self) -> String {
        autocomplete_word_prefix(&self.visible_text, self.visible_cursor)
    }

    fn editor_context<'a>(&'a self, editor: &'a Editor) -> AutocompleteEditorContext<'a> {
        AutocompleteEditorContext {
            editor,
            file_extension: &self.file_extension,
            visible_text: &self.visible_text,
            analysis_text: &self.analysis_text,
            cursor: self.visible_cursor,
            analysis_cursor: self.analysis_cursor,
            path: self.path.as_deref(),
            line_offsets: &self.line_offsets,
            source: self.source,
        }
    }
}

pub(crate) struct AutocompleteEditorContext<'a> {
    pub(crate) editor: &'a Editor,
    pub(crate) file_extension: &'a str,
    pub(crate) visible_text: &'a str,
    pub(crate) analysis_text: &'a str,
    pub(crate) cursor: usize,
    pub(crate) analysis_cursor: usize,
    pub(crate) path: Option<&'a Path>,
    pub(crate) line_offsets: &'a [usize],
    pub(crate) source: ActiveAutocompleteSource,
}

impl AutocompleteEditorContext<'_> {
    fn current_word_prefix(&self) -> String {
        autocomplete_word_prefix(self.visible_text, self.cursor)
    }

    fn lookup_path(&self) -> Option<&Path> {
        match self.source {
            ActiveAutocompleteSource::MainEditor => self.path,
            ActiveAutocompleteSource::ApiMock { .. } => None,
        }
    }
}

pub(crate) fn autocomplete_word_prefix(text: &str, cursor: usize) -> String {
    let cursor = cursor.min(text.len());
    let mut p = cursor;
    let bytes = text.as_bytes();
    while p > 0 {
        let b = bytes[p - 1];
        if !(b.is_ascii_alphanumeric() || b == b'_') {
            break;
        }
        p -= 1;
    }
    if p == cursor {
        return String::new();
    }
    text.get(p..cursor).unwrap_or("").to_string()
}

pub(crate) fn build_ty_autocomplete_options(
    ctx: &AutocompleteEditorContext<'_>,
    mode: AutocompleteMode,
    items: Vec<crate::lsp::LspCompletionItem>,
    signature_items: &[crate::lsp::LspCompletionItem],
    workspaces: &[PathBuf],
) -> Vec<(AutocompleteItem, Vec<usize>)> {
    debug_assert!(ctx.line_offsets.first().copied().unwrap_or(0) == 0);
    let prefix = ctx.current_word_prefix();
    let mut items: Vec<AutocompleteItem> = items.into_iter().map(AutocompleteItem::from).collect();
    let current_text = ctx.analysis_text;
    let current_cursor = ctx.analysis_cursor.min(current_text.len());
    let python_context = python_completion_context(ctx.file_extension, current_text);
    let current_path = ctx.lookup_path();
    let current_module_path = python_context
        .then(|| {
            current_path.and_then(|path| {
                crate::app::events::module_path_from_definition_path(path, workspaces)
            })
        })
        .flatten();
    let local_self_owner = python_context
        .then(|| python_enclosing_class_before_cursor(current_text, current_cursor))
        .flatten();
    let imported_modules = python_context.then(|| imported_python_symbols(current_text));
    let member_dot_context = cursor_after_python_member_dot(ctx.editor);
    let local_function_names = if current_module_path.is_some() && !member_dot_context {
        python_source_function_names(current_text)
    } else {
        FxHashSet::default()
    };
    let call_argument_context = mode == AutocompleteMode::TyContext
        && cursor_inside_python_call_parens(ctx.editor)
        && !member_dot_context;
    if call_argument_context && !signature_items.is_empty() {
        let mut merged = Vec::with_capacity(signature_items.len() + items.len());
        merged.extend(signature_items.iter().cloned().map(AutocompleteItem::from));
        merged.extend(items);
        items = merged;
    }
    if mode == AutocompleteMode::TyContext {
        items.retain(|item| !ty_auto_import_completion(item));
    }
    let common_owner = (mode == AutocompleteMode::TyContext && member_dot_context)
        .then(|| common_completion_owner(&items))
        .flatten();
    let receiver_owner = (mode == AutocompleteMode::TyContext && member_dot_context)
        .then(|| {
            python_member_dot_receiver(current_text, current_cursor).and_then(|receiver| {
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
    } else if mode == AutocompleteMode::TyContext && member_dot_context {
        infer_python_member_owner(
            current_text,
            imported_modules.as_ref(),
            workspaces,
            current_path,
            &items,
            fallback_owner,
        )
    } else {
        fallback_owner.map(str::to_string)
    };
    if autocomplete_trace_enabled() {
        println!(
            "Autocomplete build_ty_context: source={} prefix={:?} member_dot={} common_owner={:?} member_owner={:?} current_text_len={}",
            ctx.source.trace_label(),
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
            imported_python_class_source(current_text, workspaces, current_path, owner).or_else(
                || {
                    let owner_label = owner.rsplit('.').next().unwrap_or(owner);
                    source_contains_python_class(current_text, owner_label)
                        .then(|| current_text.to_string())
                },
            )
        })
    } else {
        None
    };
    let source_attr_owners = if let (Some(owner), Some(source)) =
        (member_owner.as_deref(), member_owner_source.as_deref())
    {
        python_class_attr_owners_with_imports(
            source,
            workspaces,
            current_path,
            owner,
            &source_attr_words,
        )
    } else {
        FxHashMap::default()
    };
    let source_member_words: FxHashSet<String> = if member_dot_context && member_owner.is_some() {
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
            workspaces,
            current_path,
            owner,
            &source_member_words,
        )
    } else {
        FxHashMap::default()
    };
    let member_owner_depths = if let (Some(owner), Some(source)) =
        (member_owner.as_deref(), member_owner_source.as_deref())
    {
        python_class_owner_depths_with_imports(source, workspaces, current_path, owner)
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
        if mode == AutocompleteMode::TyImports {
            normalize_ty_import_kind(item);
        }
        if mode == AutocompleteMode::TyContext {
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
                    if item.kind == SymbolKind::Unknown && completion_word_starts_lower(&item.word)
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
            "Autocomplete build_ty_normalized: source={} prefix={:?} python_context={} imports={} items={}",
            ctx.source.trace_label(),
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
        if mode == AutocompleteMode::TyImports && item.module.is_none() {
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
        let low_priority_member = member_dot_context && python_low_priority_member_name(&item.word);
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

    matches
        .into_iter()
        .take(80)
        .map(|(_, _, item, indices)| (item, indices))
        .collect()
}

fn api_mock_contract_constraint_options(
    ctx: &AutocompleteEditorContext<'_>,
) -> Vec<(AutocompleteItem, Vec<usize>)> {
    match ctx.source {
        ActiveAutocompleteSource::ApiMock {
            part: crate::app::api_mock::ty_check::ApiMockSourcePart::Contract,
            ..
        } => {}
        _ => return Vec::new(),
    }
    const MARKERS: &[(&str, &str, &str)] = &[
        ("MinLen", "MinLen(1)", "min string length"),
        ("MaxLen", "MaxLen(255)", "max string length"),
        ("Pattern", "Pattern(\"^[a-z0-9_]+$\")", "string regex pattern"),
        ("Ge", "Ge(0)", "number >= value"),
        ("Gt", "Gt(0)", "number > value"),
        ("Le", "Le(100)", "number <= value"),
        ("Lt", "Lt(100)", "number < value"),
        ("MinItems", "MinItems(1)", "min array items"),
        ("MaxItems", "MaxItems(10)", "max array items"),
    ];
    let cursor = ctx.cursor.min(ctx.visible_text.len());
    let line_start = ctx.visible_text[..cursor]
        .rfind('\n')
        .map(|idx| idx + 1)
        .unwrap_or(0);
    let line_prefix = &ctx.visible_text[line_start..cursor];
    let Some((_, type_part)) = line_prefix.split_once(':') else {
        return Vec::new();
    };
    let prefix = ctx.current_word_prefix();
    let prefix_lower = prefix.to_ascii_lowercase();
    let in_annotated = type_part.contains("Annotated[");
    let marker_prefix = !prefix.is_empty()
        && MARKERS
            .iter()
            .any(|(name, _, _)| name.to_ascii_lowercase().starts_with(&prefix_lower));
    if !in_annotated && !marker_prefix {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(MARKERS.len());
    for (name, insert, detail) in MARKERS {
        let name_lower = name.to_ascii_lowercase();
        let indices = if prefix.is_empty() {
            Vec::new()
        } else if let Some(indices) = fuzzy_match(&prefix_lower, &name_lower) {
            indices
        } else {
            continue;
        };
        out.push((
            AutocompleteItem {
                word: (*name).to_string(),
                kind: SymbolKind::Class,
                scope_start: 0,
                scope_end: usize::MAX,
                module: Some("RRiter constraint".to_string()),
                module_path: None,
                detail: Some(format!("(constraint) {name}: {detail}")),
                insert_text: Some((*insert).to_string()),
                text_edit: None,
                additional_text_edits: Vec::new(),
            },
            indices,
        ));
    }
    out
}

impl App {
    pub(crate) fn active_api_mock_autocomplete_source(&self) -> Option<ActiveAutocompleteSource> {
        let (route_idx, part) = self.api_mock_completion_focus()?;
        let (meta, _) = self.active_api_tab()?;
        Some(ActiveAutocompleteSource::ApiMock {
            spec_id: meta.spec_id,
            route_idx,
            part,
        })
    }

    fn active_autocomplete_source(&self) -> Option<ActiveAutocompleteSource> {
        self.active_api_mock_autocomplete_source()
            .or(Some(ActiveAutocompleteSource::MainEditor))
    }

    pub(crate) fn active_autocomplete_source_snapshot(
        &self,
        source: ActiveAutocompleteSource,
    ) -> Option<AutocompleteSourceSnapshot> {
        match source {
            ActiveAutocompleteSource::MainEditor => {
                let text = self.editor.get_full_text();
                Some(AutocompleteSourceSnapshot {
                    source,
                    file_extension: self.file_extension.clone(),
                    visible_cursor: self.editor.cursor,
                    analysis_cursor: self.editor.cursor,
                    visible_text: text.clone(),
                    analysis_text: text,
                    path: self.file_path.clone(),
                    line_offsets: self.editor.line_offsets.clone(),
                    version: self.editor.version.min(i32::MAX as u64) as i32,
                })
            }
            ActiveAutocompleteSource::ApiMock {
                spec_id,
                route_idx,
                part,
            } => {
                let (method, path, route, model) = self.api_mock_route_context(route_idx)?;
                let script = self.api_mock_script_for_tools(route_idx)?;
                let virtual_source = crate::app::api_mock::ty_check::build_api_mock_virtual_source(
                    method, &path, &route, &model, &script,
                );
                let edit_text = self.ide_panel.api.input_editor.get_full_text();
                let cursor = self.ide_panel.api.input_editor.cursor;
                let source_cursor = virtual_source.edit_offset_to_source(part, &edit_text, cursor);
                let analysis_text = virtual_source.source;
                let line_offsets = line_offsets_for_text(&analysis_text);
                Some(AutocompleteSourceSnapshot {
                    source,
                    file_extension: "py".to_string(),
                    visible_text: edit_text,
                    analysis_text,
                    visible_cursor: cursor,
                    analysis_cursor: source_cursor,
                    path: Some(Self::api_mock_virtual_path_for(spec_id, route_idx)),
                    line_offsets,
                    version: self.ide_panel.api.input_editor.version.min(i32::MAX as u64) as i32,
                })
            }
        }
    }

    fn autocomplete_editor_for_source(&self, source: ActiveAutocompleteSource) -> &Editor {
        match source {
            ActiveAutocompleteSource::MainEditor => &self.editor,
            ActiveAutocompleteSource::ApiMock { .. } => &self.ide_panel.api.input_editor,
        }
    }

    fn autocomplete_tree_sitter_completions_for_source(
        &self,
        source: ActiveAutocompleteSource,
    ) -> &[CompletionItem] {
        match source {
            ActiveAutocompleteSource::MainEditor => &self.highlighter.completions,
            ActiveAutocompleteSource::ApiMock { .. } => {
                &self.ide_panel.api.mock_highlighter.completions
            }
        }
    }

    pub(crate) fn source_after_python_member_dot(&self, source: ActiveAutocompleteSource) -> bool {
        cursor_after_python_member_dot(self.autocomplete_editor_for_source(source))
    }

    pub(crate) fn source_inside_python_call_parens(
        &self,
        source: ActiveAutocompleteSource,
    ) -> bool {
        cursor_inside_python_call_parens(self.autocomplete_editor_for_source(source))
    }

    fn source_member_chain_too_deep(&self, source: ActiveAutocompleteSource) -> bool {
        python_member_chain_too_deep(self.autocomplete_editor_for_source(source))
    }

    fn source_member_dot_receiver_is_unavailable_self(
        &self,
        source: ActiveAutocompleteSource,
        snapshot: &AutocompleteSourceSnapshot,
    ) -> bool {
        if snapshot.file_extension != "py" {
            return false;
        }
        let editor = self.autocomplete_editor_for_source(source);
        let Some(receiver) = python_member_receiver_before_cursor(editor) else {
            return false;
        };
        if !matches!(receiver.as_str(), "self" | "cls") {
            return false;
        }
        let cursor = editor.cursor.min(editor.len());
        let lookup_cursor = if cursor > 0 && editor.byte_at(cursor - 1) == b'.' {
            cursor - 1
        } else {
            cursor
        };
        !self
            .autocomplete_tree_sitter_completions_for_source(source)
            .iter()
            .any(|item| {
                item.word == receiver
                    && item.kind == SymbolKind::Parameter
                    && lookup_cursor >= item.scope_start
                    && lookup_cursor <= item.scope_end
            })
    }

    fn autocomplete_anchor_for_source(
        &mut self,
        source: ActiveAutocompleteSource,
    ) -> Option<(f32, f32)> {
        match source {
            ActiveAutocompleteSource::MainEditor => None,
            ActiveAutocompleteSource::ApiMock { .. } => self.api_mock_autocomplete_anchor(),
        }
    }

    fn reset_autocomplete_for_empty_prefix(&mut self) {
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
    }

    pub fn request_ty_autocomplete(&mut self, mode: AutocompleteMode, trigger: Option<&str>) {
        let Some(source) = self.active_autocomplete_source() else {
            return;
        };
        self.request_ty_autocomplete_for_source(source, mode, trigger);
    }

    pub(crate) fn request_ty_autocomplete_for_source(
        &mut self,
        source: ActiveAutocompleteSource,
        mode: AutocompleteMode,
        trigger: Option<&str>,
    ) {
        if autocomplete_trace_enabled() {
            println!(
                "Autocomplete request_ty: source={} mode={:?} trigger={:?} active={} opts={} pending={:?}",
                source.trace_label(),
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
        let Some(snapshot) = self.active_autocomplete_source_snapshot(source) else {
            self.trace_autocomplete_state("request_ty:no_source");
            return;
        };
        if python_completion_context(&snapshot.file_extension, &snapshot.analysis_text)
            && !python_completion_allowed_at_cursor(self.autocomplete_editor_for_source(source))
        {
            self.close_autocomplete();
            self.trace_autocomplete_state("request_ty:python_string");
            return;
        }
        let prefix = snapshot.current_word_prefix();
        if mode == AutocompleteMode::TyContext
            && self.source_member_dot_receiver_is_unavailable_self(source, &snapshot)
        {
            self.close_autocomplete();
            self.trace_autocomplete_state("request_ty:unscoped_self_member");
            return;
        }
        if mode == AutocompleteMode::TyContext
            && (self.source_member_chain_too_deep(source)
                || trigger.is_none()
                    && prefix.is_empty()
                    && !self.source_after_python_member_dot(source)
                    && !self.source_inside_python_call_parens(source))
        {
            self.close_autocomplete();
            self.trace_autocomplete_state("request_ty:blocked_context");
            return;
        }
        if mode == AutocompleteMode::TyImports && prefix.is_empty() {
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
            self.autocomplete_anchor = self.autocomplete_anchor_for_source(source);
            self.trace_autocomplete_state("request_ty:imports_empty_prefix");
            return;
        }
        let Some(path) = snapshot.path.clone() else {
            self.trace_autocomplete_state("request_ty:no_path");
            return;
        };
        let hide_exact_match = matches!(source, ActiveAutocompleteSource::MainEditor)
            && mode == AutocompleteMode::TyContext
            && self.autocomplete_has_only_current_text_match();
        let context_key = ty_autocomplete_context_key(
            &snapshot.analysis_text,
            &snapshot.line_offsets,
            snapshot.analysis_cursor,
            &prefix,
            mode,
        );
        let cacheable_response = source.cacheable() && prefix.is_empty();
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
            self.update_ty_autocomplete_for_source(source, items);
            self.trace_autocomplete_state("request_ty:cached_end");
            return;
        }
        let notified = match source {
            ActiveAutocompleteSource::MainEditor => {
                let Some(lsp) = self.lsp.as_mut() else {
                    self.trace_autocomplete_state("request_ty:no_lsp");
                    return;
                };
                lsp.notify_change(
                    &path,
                    &snapshot.file_extension,
                    &snapshot.analysis_text,
                    snapshot.version,
                );
                true
            }
            ActiveAutocompleteSource::ApiMock { .. } => {
                self.notify_api_mock_lsp_source(&path, &snapshot.analysis_text, snapshot.version)
            }
        };
        if !notified {
            self.trace_autocomplete_state("request_ty:no_lsp");
            return;
        }
        let (line, col) = crate::lsp::offset_to_lsp_pos(
            &snapshot.analysis_text,
            snapshot.analysis_cursor,
            &snapshot.line_offsets,
        );
        let request_signature_help = mode == AutocompleteMode::TyContext
            && self.source_inside_python_call_parens(source)
            && !self.source_after_python_member_dot(source);
        let Some(lsp) = self.lsp.as_mut() else {
            self.trace_autocomplete_state("request_ty:no_lsp");
            return;
        };
        let completion_id =
            lsp.request_ty_completion(&path, &snapshot.file_extension, line, col, trigger);
        let signature_id = if request_signature_help {
            lsp.request_ty_signature_help(&path, &snapshot.file_extension, line, col, None)
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
                if source.is_api_mock() {
                    self.autocomplete_anchor = self.autocomplete_anchor_for_source(source);
                }
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
                if source.is_api_mock() {
                    self.autocomplete_anchor = self.autocomplete_anchor_for_source(source);
                }
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
        let Some(source) = self.active_autocomplete_source() else {
            return;
        };
        self.update_ty_autocomplete_for_source(source, items);
    }

    pub(crate) fn update_ty_autocomplete_for_source(
        &mut self,
        source: ActiveAutocompleteSource,
        items: Vec<crate::lsp::LspCompletionItem>,
    ) {
        if autocomplete_trace_enabled() {
            println!(
                "Autocomplete update_ty: source={} incoming={} mode={:?} active={} opts_before={} pending={:?}",
                source.trace_label(),
                items.len(),
                self.autocomplete_mode,
                self.autocomplete_active,
                self.autocomplete_options.len(),
                self.autocomplete_pending_request_id
            );
        }
        self.trace_autocomplete_state("update_ty:begin");
        let Some(snapshot) = self.active_autocomplete_source_snapshot(source) else {
            self.trace_autocomplete_state("update_ty:no_source");
            return;
        };
        if python_completion_context(&snapshot.file_extension, &snapshot.analysis_text)
            && !python_completion_allowed_at_cursor(self.autocomplete_editor_for_source(source))
        {
            self.close_autocomplete();
            self.trace_autocomplete_state("update_ty:python_string");
            return;
        }
        let prefix = snapshot.current_word_prefix();
        if self.autocomplete_mode == AutocompleteMode::TyContext
            && (self.source_member_chain_too_deep(source)
                || prefix.is_empty()
                    && !self.source_after_python_member_dot(source)
                    && !self.source_inside_python_call_parens(source))
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

        let was_inactive = !self.autocomplete_active;
        let options = {
            let editor = self.autocomplete_editor_for_source(source);
            let ctx = snapshot.editor_context(editor);
            let mut options = build_ty_autocomplete_options(
                &ctx,
                self.autocomplete_mode,
                items,
                &self.autocomplete_signature_items,
                &self.ide_workspaces,
            );
            options.extend(api_mock_contract_constraint_options(&ctx));
            options
        };
        self.autocomplete_options = options;
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
            self.autocomplete_anchor = self.autocomplete_anchor_for_source(source);
        } else if source.is_api_mock() {
            self.autocomplete_anchor = self.autocomplete_anchor_for_source(source);
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
            self.request_active_autocomplete_detail_for_index(0);
        }
        self.trace_autocomplete_state("update_ty:end");
    }

    pub fn update_ty_signature_help_autocomplete(&mut self, parameters: Vec<String>) {
        if self.api_mock_completion_focus().is_some() {
            self.update_api_mock_ty_signature_help_autocomplete(parameters);
            return;
        }
        let Some(source) = self.active_autocomplete_source() else {
            return;
        };
        self.update_ty_signature_help_autocomplete_for_source(source, parameters);
    }

    pub(crate) fn update_ty_signature_help_autocomplete_for_source(
        &mut self,
        source: ActiveAutocompleteSource,
        parameters: Vec<String>,
    ) {
        if self.autocomplete_mode != AutocompleteMode::TyContext
            || !self.source_inside_python_call_parens(source)
            || self.source_after_python_member_dot(source)
        {
            return;
        }
        let Some(snapshot) = self.active_autocomplete_source_snapshot(source) else {
            return;
        };
        self.autocomplete_signature_items = ty_signature_parameter_items(
            parameters,
            &snapshot.visible_text,
            snapshot.visible_cursor,
        );
        if !self.autocomplete_signature_items.is_empty() {
            self.update_ty_autocomplete_for_source(source, Vec::new());
        }
    }
}
