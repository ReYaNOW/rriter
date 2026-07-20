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

pub(crate) fn lsp_signature_parameter_items(
    help: &crate::lsp::LspSignatureHelp,
    file_extension: &str,
    text: &str,
    cursor: usize,
) -> Vec<crate::lsp::LspCompletionItem> {
    let Some(signature) = help
        .signatures
        .get(help.active_signature)
        .or_else(|| help.signatures.first())
    else {
        return Vec::new();
    };
    if matches!(file_extension, "py" | "pyi") {
        return ty_signature_parameter_items(
            signature
                .parameters
                .iter()
                .map(|parameter| parameter.label.clone())
                .collect(),
            text,
            cursor,
        );
    }
    let separator = if file_extension == "dart" { ": " } else { "" };
    signature
        .parameters
        .iter()
        .map(|parameter| crate::lsp::LspCompletionItem {
            label: parameter.label.clone(),
            kind: SymbolKind::Argument,
            module: None,
            detail: parameter
                .documentation
                .clone()
                .or_else(|| Some(signature.label.clone())),
            insert_text: Some(format!("{}{separator}", parameter.label)),
            text_edit: None,
            additional_text_edits: Vec::new(),
        })
        .collect()
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

pub(crate) fn lsp_autocomplete_context_key(
    snapshot: &AutocompleteSourceSnapshot,
    mode: AutocompleteMode,
    trigger: Option<&str>,
    workspaces: &[PathBuf],
) -> String {
    let prefix = snapshot.current_word_prefix();
    let anchor = snapshot
        .analysis_cursor
        .saturating_sub(prefix.len())
        .min(snapshot.analysis_text.len());
    let line = snapshot
        .line_offsets
        .partition_point(|&offset| offset <= anchor)
        .saturating_sub(1);
    let line_start = snapshot
        .line_offsets
        .get(line)
        .copied()
        .unwrap_or(0)
        .min(anchor);
    let context = snapshot
        .analysis_text
        .get(line_start..anchor)
        .unwrap_or("")
        .trim_end();
    let workspace = snapshot.path.as_deref().and_then(|path| {
        workspaces
            .iter()
            .filter(|workspace| crate::platform::path_is_within(path, workspace))
            .max_by_key(|workspace| workspace.components().count())
    });
    format!(
        "{mode:?}|lang={}|v{}|cursor={}|trigger={}|workspace={}|{context}",
        snapshot.file_extension,
        snapshot.version,
        snapshot.analysis_cursor,
        trigger.unwrap_or("manual"),
        workspace
            .map(|path| path.to_string_lossy())
            .unwrap_or_default()
    )
}

pub(crate) fn build_lsp_autocomplete_options(
    snapshot: &AutocompleteSourceSnapshot,
    items: Vec<crate::lsp::LspCompletionItem>,
) -> Vec<(AutocompleteItem, Vec<usize>)> {
    let prefix = snapshot.current_word_prefix();
    let prefix_lower = prefix.to_lowercase();
    let mut seen = FxHashSet::default();
    let mut matches = Vec::new();
    for (server_order, item) in items.into_iter().enumerate() {
        let filter = item.insert_text.as_deref().unwrap_or(&item.label);
        let filter_lower = filter.to_lowercase();
        let indices = if prefix.is_empty() {
            Vec::new()
        } else if let Some(indices) = fuzzy_match(&prefix_lower, &filter_lower) {
            indices
        } else {
            continue;
        };
        let dedup_key = (
            item.label.clone(),
            item.text_edit
                .as_ref()
                .map(|edit| {
                    (
                        edit.start_line,
                        edit.start_col,
                        edit.end_line,
                        edit.end_col,
                        edit.new_text.clone(),
                    )
                }),
            item.additional_text_edits.clone(),
        );
        if !seen.insert(dedup_key) {
            continue;
        }
        let is_prefix = prefix.is_empty() || filter_lower.starts_with(&prefix_lower);
        let label_len = item.label.len();
        matches.push((
            !is_prefix,
            server_order,
            label_len,
            AutocompleteItem::from(item),
            indices,
        ));
    }
    matches.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
            .then_with(|| left.3.word.cmp(&right.3.word))
    });
    matches
        .into_iter()
        .take(80)
        .map(|(_, _, _, item, indices)| (item, indices))
        .collect()
}

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

fn strict_lsp_position_to_offset(text: &str, line: u32, col: u32) -> Option<usize> {
    let mut current_line = 0u32;
    let mut current_col = 0u32;
    for (offset, ch) in text.char_indices() {
        if current_line == line && current_col == col {
            return Some(offset);
        }
        if ch == '\n' {
            if current_line == line {
                return None;
            }
            current_line = current_line.checked_add(1)?;
            current_col = 0;
            continue;
        }
        let next_col = current_col.checked_add(ch.len_utf16() as u32)?;
        if current_line == line && col > current_col && col < next_col {
            return None;
        }
        current_col = next_col;
    }
    (current_line == line && current_col == col).then_some(text.len())
}

fn completion_text_edit_op(
    text: &str,
    change: &crate::lsp::TextChange,
) -> Option<CompletionTextEditOp> {
    let start = strict_lsp_position_to_offset(text, change.start_line, change.start_col)?;
    let end = strict_lsp_position_to_offset(text, change.end_line, change.end_col)?;
    (start <= end).then(|| CompletionTextEditOp {
        start,
        end,
        new_text: change.new_text.clone(),
    })
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

    let text = editor.get_full_text();
    if plan.ops.iter().any(|op| {
        op.start > op.end
            || op.end > text.len()
            || !text.is_char_boundary(op.start)
            || !text.is_char_boundary(op.end)
    }) {
        return Vec::new();
    }
    plan.ops.sort_unstable_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then(a.end.cmp(&b.end))
            .then(a.new_text.cmp(&b.new_text))
    });
    plan.ops.dedup_by(|left, right| {
        left.start == right.start && left.end == right.end && left.new_text == right.new_text
    });
    if plan.ops.windows(2).any(|ops| {
        let left = &ops[0];
        let right = &ops[1];
        right.start < left.end
            || (left.start == left.end
                && right.start == right.end
                && left.start == right.start)
    }) {
        return Vec::new();
    }

    plan.ops
        .sort_unstable_by(|a, b| b.start.cmp(&a.start).then(b.end.cmp(&a.end)));
    for op in &plan.ops {
        let (offset, deleted_len, _) = editor.replace_range(op.start, op.end, &op.new_text);
        applied.push(CompletionAppliedEdit {
            offset,
            deleted_len,
            inserted_text: op.new_text.clone(),
        });
    }
    if let (Some(primary_start), Some(mut target_cursor)) =
        (plan.primary_start, plan.target_cursor)
    {
        for op in &plan.ops {
            let strictly_before_primary = op.end < primary_start
                || (op.end == primary_start && op.start < primary_start);
            if strictly_before_primary {
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
        if let Some((match_kind, indices)) = autocomplete_match_candidate(&prefix_lower, &comp_lower)
        {
            let is_prefix = match_kind.is_prefix();
            let mut score = 0i64;
            let scope_bonus = if comp.kind == SymbolKind::Keyword {
                0
            } else {
                let scope_size = comp.scope_end.saturating_sub(comp.scope_start);
                let sz = scope_size.min(i64::MAX as usize) as i64;
                10_000_000 / sz.saturating_add(1).max(1)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutocompleteMatchKind {
    Exact,
    Prefix,
    Fuzzy,
}

impl AutocompleteMatchKind {
    #[inline]
    pub(crate) fn is_prefix(self) -> bool {
        matches!(self, Self::Exact | Self::Prefix)
    }
}

pub(crate) fn autocomplete_match_candidate(
    prefix: &str,
    candidate: &str,
) -> Option<(AutocompleteMatchKind, Vec<usize>)> {
    let prefix_lower = prefix.to_lowercase();
    let candidate_lower = candidate.to_lowercase();
    let indices = fuzzy_match(&prefix_lower, &candidate_lower)?;
    let kind = if candidate_lower == prefix_lower {
        AutocompleteMatchKind::Exact
    } else if candidate_lower.starts_with(&prefix_lower) {
        AutocompleteMatchKind::Prefix
    } else {
        AutocompleteMatchKind::Fuzzy
    };
    Some((kind, indices))
}

fn autocomplete_item_identity_matches(left: &AutocompleteItem, right: &AutocompleteItem) -> bool {
    left.word == right.word
        && left.kind == right.kind
        && left.insert_text == right.insert_text
        && left.module == right.module
        && left.module_path == right.module_path
        && left.detail == right.detail
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
    options.extend(api_mock_contract_constraint_options(ctx));
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
    pub(crate) fn update_autocomplete_session(
        &mut self,
        mode: AutocompleteMode,
        context_key: Option<String>,
        options: Vec<(AutocompleteItem, Vec<usize>)>,
        anchor: Option<(f32, f32)>,
        refresh_anchor_while_open: bool,
    ) {
        let was_active = self.autocomplete_active;
        let same_session = was_active
            && self.autocomplete_mode == mode
            && self.autocomplete_pending_context_key.as_deref() == context_key.as_deref();
        let previous_index = self.autocomplete_selected_idx;
        let previous_option_count = self.autocomplete_options.len();
        let previous_item = same_session
            .then(|| {
                self.autocomplete_options
                    .get(previous_index)
                    .map(|(item, _)| item.clone())
            })
            .flatten();

        self.autocomplete_options = options;
        if self.autocomplete_options.is_empty() {
            self.close_autocomplete();
            return;
        }

        if !was_active {
            self.autocomplete_anim_progress = 0.0;
        }
        if !same_session {
            self.autocomplete_scroll.reset();
        }
        if !was_active || !same_session || refresh_anchor_while_open {
            self.autocomplete_anchor = anchor;
        }

        let preserved_selection = previous_item.as_ref().and_then(|previous_item| {
            self.autocomplete_options
                .iter()
                .position(|(item, _)| autocomplete_item_identity_matches(item, previous_item))
        });
        self.autocomplete_selected_idx = preserved_selection
            .unwrap_or_else(|| previous_index.min(self.autocomplete_options.len() - 1));
        if !same_session {
            self.autocomplete_selected_idx = 0;
        } else if previous_item.is_some() && preserved_selection.is_none() {
            self.autocomplete_scroll.reset();
        }
        self.autocomplete_hovered_idx = None;
        self.autocomplete_mode = mode;
        self.autocomplete_pending_context_key = context_key;
        self.autocomplete_active = true;
        self.autocomplete_detail_rect = None;
        self.autocomplete_detail_placement = None;
        self.autocomplete_detail_max_scroll = 0.0;
        self.reset_autocomplete_detail_size();
        self.refresh_autocomplete_detail_popup();

        if same_session && self.autocomplete_options.len() < previous_option_count {
            self.ensure_autocomplete_visible();
        }
    }

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
        if !matches!(
            self.autocomplete_mode,
            AutocompleteMode::TyContext | AutocompleteMode::LspContext
        ) || self.autocomplete_pending_request_id.is_none()
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
        if self.autocomplete_mode == AutocompleteMode::TyContext
            && !cursor_after_python_member_dot(editor)
            && !cursor_inside_python_call_parens(editor)
        {
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
        if python_completion_context(&snapshot.file_extension, &snapshot.analysis_text)
            && !python_completion_allowed_at_cursor(self.autocomplete_editor_for_source(source))
        {
            self.close_autocomplete();
            self.trace_autocomplete_state("update_ts:python_string");
            return;
        }
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
            let local_options = {
                let editor = self.autocomplete_editor_for_source(source);
                let ctx = snapshot.editor_context(editor);
                api_mock_contract_constraint_options(&ctx)
            };
            if !local_options.is_empty() {
                let anchor = self.autocomplete_anchor_for_source(source);
                self.update_autocomplete_session(
                    AutocompleteMode::TreeSitter,
                    None,
                    local_options,
                    anchor,
                    source.is_api_mock(),
                );
                self.trace_autocomplete_state("update_ts:local_empty_prefix");
                return;
            }
            self.reset_autocomplete_for_empty_prefix();
            self.trace_autocomplete_state("update_ts:empty_prefix");
            return;
        }

        let options = {
            let editor = self.autocomplete_editor_for_source(source);
            let completions = self.autocomplete_tree_sitter_completions_for_source(source);
            let ctx = snapshot.editor_context(editor);
            build_tree_sitter_autocomplete_options(&ctx, completions, "update_ts")
        };
        if autocomplete_trace_enabled() {
            let first = options
                .iter()
                .take(5)
                .map(|(item, _)| item.word.as_str())
                .collect::<Vec<_>>()
                .join(",");
            println!(
                "Autocomplete update_ts_matches: opts={} first=[{}]",
                options.len(),
                first
            );
        }
        let has_options = !options.is_empty();
        let anchor = has_options
            .then(|| self.autocomplete_anchor_for_source(source))
            .flatten();
        self.update_autocomplete_session(
            AutocompleteMode::TreeSitter,
            None,
            options,
            anchor,
            source.is_api_mock(),
        );
        if has_options {
            self.request_active_autocomplete_detail_for_index(self.autocomplete_selected_idx);
            self.trace_autocomplete_state("update_ts:active_end");
        } else {
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
        self.autocomplete_scroll.clamp_current(0.0, max_scroll);
    }

    pub fn apply_autocomplete(&mut self) {
        if self.apply_database_table_filter_autocomplete() {
            return;
        }
        if let Some((route_idx, part)) = self.api_mock_completion_focus()
            && self.apply_api_mock_autocomplete()
        {
            self.ide_panel.api.focused = Some(match part {
                crate::app::api_mock::ty_check::ApiMockSourcePart::Contract => {
                    crate::app::api_client::ApiFocus::MockContract { route_idx }
                }
                crate::app::api_mock::ty_check::ApiMockSourcePart::Prelude => {
                    crate::app::api_client::ApiFocus::MockPrelude { route_idx }
                }
                crate::app::api_mock::ty_check::ApiMockSourcePart::Body => {
                    crate::app::api_client::ApiFocus::MockBody { route_idx }
                }
                crate::app::api_mock::ty_check::ApiMockSourcePart::Signature => {
                    crate::app::api_client::ApiFocus::MockSignature { route_idx }
                }
            });
            return;
        }
        if !self.autocomplete_active || self.autocomplete_options.is_empty() {
            return;
        }
        if !self.lsp_completion_selection_is_current() {
            self.close_autocomplete();
            return;
        }
        let Some((selected_item, _)) = self
            .autocomplete_options
            .get(self.autocomplete_selected_idx)
            .or_else(|| self.autocomplete_options.first())
        else {
            self.close_autocomplete();
            return;
        };
        let selected_item = selected_item.clone();
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
        let text = self.editor.get_full_text();
        let mut changes = item.additional_text_edits.clone();
        if self.autocomplete_mode == AutocompleteMode::TyImports {
            append_python_import_edits_to_block(
                &self.file_extension,
                &text,
                &self.editor.line_offsets,
                &mut changes,
            );
        }

        let selected = item
            .insert_text
            .as_deref()
            .unwrap_or(&item.word)
            .to_string();
        let prefix_len = self.get_current_word_prefix().len();
        let (primary_start, target_cursor, fallback_prefix_len) =
            if let Some(main_edit) = item.text_edit.as_ref() {
                let Some(main_op) = completion_text_edit_op(&text, main_edit) else {
                    return;
                };
                let primary_start = main_op.start;
                let target_cursor = primary_start.saturating_add(main_edit.new_text.len());
                changes.push(main_edit.clone());
                (Some(primary_start), Some(target_cursor), prefix_len)
            } else if !changes.is_empty() {
                let primary_start = self.editor.cursor.saturating_sub(prefix_len);
                let primary = crate::lsp::TextChange {
                    start_line: crate::lsp::offset_to_lsp_pos(
                        &text,
                        primary_start,
                        &self.editor.line_offsets,
                    )
                    .0,
                    start_col: crate::lsp::offset_to_lsp_pos(
                        &text,
                        primary_start,
                        &self.editor.line_offsets,
                    )
                    .1,
                    end_line: crate::lsp::offset_to_lsp_pos(
                        &text,
                        self.editor.cursor,
                        &self.editor.line_offsets,
                    )
                    .0,
                    end_col: crate::lsp::offset_to_lsp_pos(
                        &text,
                        self.editor.cursor,
                        &self.editor.line_offsets,
                    )
                    .1,
                    new_text: selected.clone(),
                };
                changes.push(primary);
                (
                    Some(primary_start),
                    Some(primary_start.saturating_add(selected.len())),
                    prefix_len,
                )
            } else {
                self.apply_completion_plan_to_main_editor(CompletionApplyPlan {
                    ops: Vec::new(),
                    primary_start: None,
                    target_cursor: None,
                    fallback_insert: selected,
                    fallback_prefix_len: prefix_len,
                });
                self.finish_lsp_completion_apply();
                return;
            };

        let Some(ops) = changes
            .iter()
            .map(|change| completion_text_edit_op(&text, change))
            .collect::<Option<Vec<_>>>()
        else {
            return;
        };
        self.apply_completion_plan_to_main_editor(CompletionApplyPlan {
            ops,
            primary_start,
            target_cursor,
            fallback_insert: selected,
            fallback_prefix_len,
        });
        self.finish_lsp_completion_apply();
    }

    fn finish_lsp_completion_apply(&mut self) {
        self.sync_after_autocomplete();
        if let Some(window) = self.window.as_ref() {
            App::update_window_title(window, &self.base_title, self.editor.is_dirty());
            window.request_redraw();
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
                lsp.notify_change(&path, &ext, &text, crate::editor::lsp_document_version(self.editor.version));
            }
        }
        self.highlighter
            .apply_edits(self.editor.version, edits, None, None);
        self.last_sent_version = self.editor.version;
        if self.active_tab_is_database_query() {
            self.refresh_active_database_query_analysis();
        }
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

#[cfg(test)]
mod autocomplete_session_identity_tests {
    use super::*;

    fn item(word: &str, detail: &str) -> AutocompleteItem {
        AutocompleteItem {
            word: word.to_string(),
            kind: SymbolKind::Property,
            scope_start: 0,
            scope_end: 0,
            module: None,
            module_path: None,
            detail: Some(detail.to_string()),
            insert_text: None,
            text_edit: None,
            additional_text_edits: Vec::new(),
        }
    }

    #[test]
    fn autocomplete_identity_distinguishes_same_word_from_different_sql_sources() {
        let left = item("id", "booking · bigint");
        let right = item("id", "customer · bigint");
        assert!(!autocomplete_item_identity_matches(&left, &right));
        assert!(autocomplete_item_identity_matches(&left, &left));
    }
}
