use super::*;

fn autocomplete_trace_enabled() -> bool {
    false
}

const AUTOCOMPLETE_CACHE_MAX_ITEMS: usize = 256;

fn autocomplete_detail_cache_item(
    item: &crate::lsp::LspCompletionItem,
) -> AutocompleteDetailCacheItem {
    AutocompleteDetailCacheItem {
        kind: item.kind,
        detail: item.detail.clone(),
        module: item.module.clone(),
        module_path: item.module.clone(),
    }
}

fn apply_autocomplete_detail_cache_item(
    item: &mut AutocompleteItem,
    cached: &AutocompleteDetailCacheItem,
    member_dot_context: bool,
) {
    let incoming_kind = completion_detail_kind(cached.kind, cached.detail.as_deref());
    let effective_kind = if incoming_kind == SymbolKind::Unknown {
        item.kind
    } else {
        incoming_kind
    };
    if matches!(item.word.as_str(), "self" | "cls") && item.kind == SymbolKind::Parameter {
        if item.detail.is_none() && cached.detail.is_some() {
            item.detail = cached.detail.clone();
        }
        return;
    }
    item.kind = effective_kind;
    if item.detail.is_none() && cached.detail.is_some() {
        item.detail = cached.detail.clone();
    }
    if !member_dot_context
        && (matches!(
            effective_kind,
            SymbolKind::Variable | SymbolKind::Parameter | SymbolKind::Property
        ) || completion_is_lowercase_type_source(
            &item.word,
            cached.module.as_deref(),
            cached.detail.as_deref(),
        ))
    {
        if completion_is_lowercase_type_source(
            &item.word,
            cached.module.as_deref(),
            cached.detail.as_deref(),
        ) {
            item.kind = SymbolKind::Variable;
        }
        item.module = None;
        item.module_path = None;
        return;
    }
    if item.module_path.is_none() && cached.module_path.is_some() {
        item.module_path = cached.module_path.clone();
    }
    if let Some(module) = cached
        .module
        .as_deref()
        .filter(|module| should_replace_completion_module(item.module.as_deref(), module))
    {
        item.module = Some(module.to_string());
    }
    assign_builtin_completion_module(item);
}

fn autocomplete_source_attr_class_detail(
    item: &AutocompleteItem,
    detail: &str,
    _owner: &str,
) -> Option<String> {
    if !matches!(item.kind, SymbolKind::Variable | SymbolKind::Property) {
        return None;
    }
    let class_name = autocomplete_detail_type_name(detail)?;
    let class_label = class_name.rsplit('.').next().unwrap_or(class_name);
    Some(format!("class {class_label}"))
}

fn autocomplete_detail_class_name(detail: &str) -> Option<&str> {
    detail.split('|').find_map(|part| {
        let part = part.trim();
        part.strip_prefix("<class '")
            .and_then(|rest| rest.strip_suffix("'>"))
            .map(str::trim)
            .filter(|name| !name.is_empty())
    })
}

fn autocomplete_detail_type_name(detail: &str) -> Option<&str> {
    autocomplete_detail_class_name(detail).or_else(|| {
        let detail = detail.trim();
        (is_class_like_type_name(detail) && !detail.contains('[')).then_some(detail)
    })
}

fn autocomplete_detail_type_module_path(
    detail: Option<&str>,
    imports: Option<&FxHashMap<String, String>>,
    fallback_module: Option<&str>,
) -> Option<String> {
    let class_name = autocomplete_detail_type_name(detail?)?;
    if completion_source_is_module_path(class_name) {
        return Some(class_name.to_string());
    }
    let class_label = class_name.rsplit('.').next().unwrap_or(class_name);
    imports
        .and_then(|imports| imports.get(class_label))
        .map(|module| format!("{module}.{class_label}"))
        .or_else(|| {
            fallback_module
                .map(normalized_completion_source)
                .filter(|module| completion_source_is_module_path(module))
                .map(|module| {
                    if module.ends_with(&format!(".{class_label}")) {
                        module.to_string()
                    } else {
                        format!("{module}.{class_label}")
                    }
                })
        })
}

fn python_known_function_completion(item: &AutocompleteItem) -> bool {
    matches!(item.word.as_str(), "cast")
        && item
            .module
            .as_deref()
            .or(item.module_path.as_deref())
            .is_some_and(|module| {
                let module = normalized_completion_source(module);
                module == "typing" || module.starts_with("typing.")
            })
        && item
            .detail
            .as_deref()
            .is_some_and(|detail| detail.trim_start().starts_with("Overload["))
}

fn python_keyword_completion(word: &str) -> bool {
    matches!(
        word,
        "False"
            | "None"
            | "True"
            | "and"
            | "as"
            | "assert"
            | "async"
            | "await"
            | "break"
            | "case"
            | "class"
            | "continue"
            | "def"
            | "del"
            | "elif"
            | "else"
            | "except"
            | "finally"
            | "for"
            | "from"
            | "global"
            | "if"
            | "import"
            | "in"
            | "is"
            | "lambda"
            | "match"
            | "nonlocal"
            | "not"
            | "or"
            | "pass"
            | "raise"
            | "return"
            | "try"
            | "while"
            | "with"
            | "yield"
    )
}

fn python_scoped_self_priority(word: &str) -> u8 {
    match word {
        "self" => 0,
        "cls" => 1,
        _ => 2,
    }
}

fn python_low_priority_member_name(word: &str) -> bool {
    matches!(word, "mro")
}

fn infer_python_member_owner(
    current_text: &str,
    imported_modules: Option<&FxHashMap<String, String>>,
    workspaces: &[PathBuf],
    current_path: Option<&Path>,
    items: &[AutocompleteItem],
    fallback: Option<&str>,
) -> Option<String> {
    let mut owner_candidates = FxHashSet::default();
    if let Some(owner) = fallback {
        owner_candidates.insert(owner.to_string());
    }
    let mut item_owners = Vec::new();
    for item in items {
        let Some(owner) = completion_item_owner_label(item) else {
            continue;
        };
        if imported_modules.is_some_and(|imports| imports.contains_key(&owner))
            || fallback == Some(owner.as_str())
        {
            owner_candidates.insert(owner.clone());
        }
        item_owners.push(owner);
    }

    let mut best = fallback.map(str::to_string);
    let mut best_score = 0usize;
    for owner in owner_candidates {
        let Some(source) =
            imported_python_class_source(current_text, workspaces, current_path, &owner)
        else {
            continue;
        };
        let depths =
            python_class_owner_depths_with_imports(&source, workspaces, current_path, &owner);
        let score = item_owners
            .iter()
            .filter(|item_owner| depths.contains_key(item_owner.as_str()))
            .count();
        if score > best_score {
            best_score = score;
            best = Some(owner);
        }
    }
    best
}

impl App {
    fn trace_autocomplete_state(&self, event: &str) {
        if !autocomplete_trace_enabled() {
            return;
        }
        let detail_len = self
            .autocomplete_detail_popup
            .as_ref()
            .map(|popup| popup.text.len())
            .unwrap_or(0);
        let detail_lines = self
            .autocomplete_detail_popup
            .as_ref()
            .map(|popup| popup.text.lines().count())
            .unwrap_or(0);
        let prefix = self.get_current_word_prefix();
        println!(
            "Autocomplete state: event={} active={} mode={:?} opts={} selected={} hovered={:?} anim={:.3} scroll={:.1}/{:.1} pending={:?} detail_req={:?} detail_word={:?} detail={}B/{}l rect={:?} detail_rect={:?} anchor={:?} min_w={:.1} prefix={:?} cursor={} version={}",
            event,
            self.autocomplete_active,
            self.autocomplete_mode,
            self.autocomplete_options.len(),
            self.autocomplete_selected_idx,
            self.autocomplete_hovered_idx,
            self.autocomplete_anim_progress,
            self.autocomplete_scroll.current,
            self.autocomplete_scroll.target,
            self.autocomplete_pending_request_id,
            self.autocomplete_detail_request_id,
            self.autocomplete_detail_word,
            detail_len,
            detail_lines,
            self.autocomplete_rect,
            self.autocomplete_detail_rect,
            self.autocomplete_anchor,
            self.autocomplete_min_width,
            prefix,
            self.editor.cursor,
            self.editor.version
        );
    }

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
        self.trace_autocomplete_state("close:before");
        self.autocomplete_active = false;
        self.autocomplete_selected_idx = 0;
        self.autocomplete_hovered_idx = None;
        self.autocomplete_scroll.current = 0.0;
        self.autocomplete_scroll.target = 0.0;
        self.autocomplete_rect = None;
        self.autocomplete_anchor = None;
        self.autocomplete_pending_request_id = None;
        self.autocomplete_pending_request_mode = None;
        self.autocomplete_pending_request_path = None;
        self.autocomplete_pending_context_key = None;
        self.autocomplete_detail_request_id = None;
        self.autocomplete_detail_word = None;
        self.autocomplete_detail_request_path = None;
        self.autocomplete_detail_context_key = None;
        self.autocomplete_detail_popup = None;
        self.autocomplete_detail_rect = None;
        self.autocomplete_detail_placement = None;
        self.autocomplete_detail_max_scroll = 0.0;
        self.autocomplete_min_width = 0.0;
        self.autocomplete_detail_selection_anchor = None;
        self.autocomplete_detail_selection_cursor = None;
        self.autocomplete_detail_selecting = false;
        self.autocomplete_apply_pending_response = false;
        crate::app::events::reset_autocomplete_frame_stats();
        self.trace_autocomplete_state("close:after");
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
        if let Some(detail) = python_stdlib_completion_detail(item, detail) {
            return std::borrow::Cow::Borrowed(detail);
        }
        let Some(owner) = item
            .module
            .as_deref()
            .filter(|module| !is_type_like_completion_source(module))
        else {
            return std::borrow::Cow::Borrowed(detail);
        };
        if let Some(detail) = autocomplete_source_attr_class_detail(item, detail, owner) {
            return std::borrow::Cow::Owned(detail);
        }
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
        self.trace_autocomplete_state("hide_keep_request:before");
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
        self.autocomplete_detail_request_path = None;
        self.autocomplete_detail_context_key = None;
        self.autocomplete_detail_popup = None;
        self.autocomplete_detail_rect = None;
        self.autocomplete_detail_placement = None;
        self.autocomplete_detail_max_scroll = 0.0;
        self.autocomplete_min_width = 0.0;
        self.autocomplete_detail_selection_anchor = None;
        self.autocomplete_detail_selection_cursor = None;
        self.autocomplete_detail_selecting = false;
        self.autocomplete_apply_pending_response = false;
        crate::app::events::reset_autocomplete_frame_stats();
        self.trace_autocomplete_state("hide_keep_request:after");
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
        self.trace_autocomplete_state("detail_refresh:begin");
        let idx = self.autocomplete_selected_idx;
        let Some((item, _)) = self.autocomplete_options.get(idx) else {
            self.autocomplete_detail_popup = None;
            self.autocomplete_detail_rect = None;
            self.trace_autocomplete_state("detail_refresh:no_item");
            return;
        };
        let Some(detail) = item.detail.as_ref().filter(|s| !s.trim().is_empty()) else {
            self.autocomplete_detail_popup = None;
            self.autocomplete_detail_rect = None;
            self.trace_autocomplete_state("detail_refresh:no_detail");
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
                self.trace_autocomplete_state("detail_refresh:unchanged");
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
        self.trace_autocomplete_state("detail_refresh:rebuilt");
    }

    fn current_autocomplete_detail_context_key(&self, text: &str, prefix: &str) -> String {
        ty_autocomplete_context_key(
            text,
            &self.editor.line_offsets,
            self.editor.cursor,
            prefix,
            AutocompleteMode::TreeSitter,
        )
    }

    fn autocomplete_detail_wanted_words(&self) -> FxHashSet<String> {
        let mut words = FxHashSet::default();
        words.reserve(self.autocomplete_options.len() + 1);
        for (item, _) in &self.autocomplete_options {
            words.insert(item.word.clone());
        }
        if let Some(word) = &self.autocomplete_detail_word {
            words.insert(word.clone());
        }
        words
    }

    fn apply_cached_autocomplete_detail_for_index(
        &mut self,
        idx: usize,
        path: &PathBuf,
        context_key: &str,
    ) -> bool {
        let Some((item, _)) = self.autocomplete_options.get(idx) else {
            return false;
        };
        let target = item.word.clone();
        let Some(cache) = self.autocomplete_detail_cache.as_ref().filter(|cache| {
            cache.path.as_path() == path.as_path() && cache.context_key == context_key
        }) else {
            return false;
        };
        let Some(cached) = cache.items.get(&target).cloned() else {
            return false;
        };
        let member_dot_context = cursor_after_python_member_dot(&self.editor);
        if let Some((item, _)) = self.autocomplete_options.get_mut(idx) {
            apply_autocomplete_detail_cache_item(item, &cached, member_dot_context);
        }
        self.refresh_autocomplete_detail_popup();
        true
    }

    pub fn remember_autocomplete_detail_cache(
        &mut self,
        items: &[crate::lsp::LspCompletionItem],
    ) {
        let (Some(path), Some(context_key)) = (
            self.autocomplete_detail_request_path.clone(),
            self.autocomplete_detail_context_key.clone(),
        ) else {
            return;
        };
        let wanted = self.autocomplete_detail_wanted_words();
        let mut details = FxHashMap::default();
        for item in items {
            if !wanted.contains(item.label.as_str()) {
                continue;
            }
            details
                .entry(item.label.clone())
                .or_insert_with(|| autocomplete_detail_cache_item(item));
        }
        if details.is_empty() {
            self.autocomplete_detail_cache = None;
            return;
        }
        self.autocomplete_detail_cache = Some(AutocompleteDetailCacheEntry {
            path,
            context_key,
            items: details,
        });
    }

    pub fn finish_autocomplete_detail_request(&mut self) {
        self.autocomplete_detail_request_id = None;
        self.autocomplete_detail_word = None;
        self.autocomplete_detail_request_path = None;
        self.autocomplete_detail_context_key = None;
    }

    pub fn request_autocomplete_detail_for_index(&mut self, idx: usize) {
        self.trace_autocomplete_state("detail_request:begin");
        if self.autocomplete_mode != AutocompleteMode::TreeSitter {
            self.refresh_autocomplete_detail_popup();
            self.trace_autocomplete_state("detail_request:non_treesitter_refresh");
            return;
        }
        let Some((item, _)) = self.autocomplete_options.get(idx) else {
            self.trace_autocomplete_state("detail_request:no_item");
            return;
        };
        let target_word = item.word.clone();
        if item.detail.is_some() {
            self.refresh_autocomplete_detail_popup();
            self.trace_autocomplete_state("detail_request:local_detail");
            return;
        }
        let Some(path) = self.file_path.clone() else {
            self.trace_autocomplete_state("detail_request:no_path");
            return;
        };
        let text = self.editor.get_full_text();
        let prefix = self.get_current_word_prefix();
        let context_key = self.current_autocomplete_detail_context_key(&text, &prefix);
        if self.apply_cached_autocomplete_detail_for_index(idx, &path, &context_key) {
            self.trace_autocomplete_state("detail_request:cache_hit");
            return;
        }
        if self.autocomplete_detail_word.as_deref() == Some(target_word.as_str())
            && self.autocomplete_detail_request_id.is_some()
            && self.autocomplete_detail_request_path.as_ref() == Some(&path)
            && self.autocomplete_detail_context_key.as_deref() == Some(context_key.as_str())
        {
            self.trace_autocomplete_state("detail_request:dedupe");
            return;
        }
        let Some(lsp) = self.lsp.as_mut() else {
            self.trace_autocomplete_state("detail_request:no_lsp");
            return;
        };
        let (line, col) =
            crate::lsp::offset_to_lsp_pos(&text, self.editor.cursor, &self.editor.line_offsets);
        if let Some(id) = lsp.request_ty_completion(&path, &self.file_extension, line, col, None) {
            self.autocomplete_detail_request_id = Some(id);
            self.autocomplete_detail_word = Some(target_word);
            self.autocomplete_detail_request_path = Some(path);
            self.autocomplete_detail_context_key = Some(context_key);
            self.trace_autocomplete_state("detail_request:sent");
        }
    }

    pub fn merge_autocomplete_details(&mut self, items: Vec<crate::lsp::LspCompletionItem>) {
        if autocomplete_trace_enabled() {
            println!(
                "Autocomplete merge: incoming={} target={:?}",
                items.len(),
                self.autocomplete_detail_word
            );
        }
        self.trace_autocomplete_state("merge_detail:begin");
        let Some(target) = self.autocomplete_detail_word.clone() else {
            self.trace_autocomplete_state("merge_detail:no_target");
            return;
        };
        let wanted = self.autocomplete_detail_wanted_words();
        let mut details = FxHashMap::default();
        for item in &items {
            if !wanted.contains(item.label.as_str()) {
                continue;
            }
            details
                .entry(item.label.clone())
                .or_insert_with(|| autocomplete_detail_cache_item(item));
        }
        let mut target_changed = false;
        let member_dot_context = cursor_after_python_member_dot(&self.editor);
        for (item, _) in &mut self.autocomplete_options {
            let Some(cached) = details.get(&item.word) else {
                continue;
            };
            apply_autocomplete_detail_cache_item(item, cached, member_dot_context);
            if item.word == target {
                target_changed = true;
            }
        }
        if target_changed {
            self.refresh_autocomplete_detail_popup();
        }
        self.finish_autocomplete_detail_request();
        self.trace_autocomplete_state("merge_detail:end");
    }

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
                    && !cursor_after_python_member_dot(&self.editor))
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
        if let Some(id) = lsp.request_ty_completion(&path, &self.file_extension, line, col, trigger)
        {
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
    }

    pub fn remember_ty_autocomplete_cache(&mut self, mut items: Vec<crate::lsp::LspCompletionItem>) {
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
                || prefix.is_empty() && !cursor_after_python_member_dot(&self.editor))
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
        let local_self_owner = (self.file_extension == "py")
            .then(|| python_enclosing_class_before_cursor(&current_text, self.editor.cursor))
            .flatten();
        let imported_modules =
            (self.file_extension == "py").then(|| imported_python_symbols(&current_text));
        let member_dot_context = cursor_after_python_member_dot(&self.editor);
        let common_owner = (self.autocomplete_mode == AutocompleteMode::TyContext
            && member_dot_context)
            .then(|| common_completion_owner(&items))
            .flatten();
        let receiver_owner = (self.autocomplete_mode == AutocompleteMode::TyContext
            && member_dot_context)
            .then(|| {
                python_member_dot_receiver(&current_text, self.editor.cursor).and_then(
                    |receiver| {
                        if matches!(receiver, "self" | "cls") {
                            return local_self_owner.clone();
                        }
                        let imported = imported_modules
                            .as_ref()
                            .is_some_and(|imports| imports.contains_key(receiver));
                        (imported || is_class_like_type_name(receiver))
                            .then(|| receiver.rsplit('.').next().unwrap_or(receiver).to_string())
                    },
                )
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
            if member_dot_context
                && let Some(owner) = member_owner.as_ref()
            {
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
                && let Some(detail) =
                    python_class_method_overload_detail(source, owner, &item.word)
            {
                item.detail = Some(detail);
            }
            item.kind = completion_detail_kind(item.kind, item.detail.as_deref());
            if python_known_function_completion(item) {
                item.kind = SymbolKind::Function;
            }
            if self.file_extension == "py"
                && item.kind == SymbolKind::Unknown
                && python_keyword_completion(&item.word)
            {
                item.kind = SymbolKind::Keyword;
            }
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
                    item.kind = SymbolKind::Parameter;
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
                        item.kind = SymbolKind::Variable;
                        if let Some(module) = top_level_import_module {
                            item.module = Some(module.clone());
                            item.module_path.get_or_insert_with(|| module.clone());
                        } else {
                            item.module = None;
                            item.module_path = None;
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
            let scoped_self_priority = if self.file_extension == "py"
                && comp.kind == SymbolKind::Parameter
            {
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
