impl App {
    pub(crate) fn reset_autocomplete_detail_size(&mut self) {
        self.autocomplete_detail_min_width = 0.0;
        self.autocomplete_detail_min_height = 0.0;
    }

    pub(crate) fn stable_autocomplete_detail_size(
        &mut self,
        natural_w: f32,
        natural_h: f32,
        max_h: f32,
    ) -> (f32, f32) {
        self.autocomplete_detail_min_width = self.autocomplete_detail_min_width.max(natural_w);
        let capped_h = natural_h.min(max_h);
        self.autocomplete_detail_min_height =
            self.autocomplete_detail_min_height.min(max_h).max(capped_h);
        (
            self.autocomplete_detail_min_width,
            self.autocomplete_detail_min_height,
        )
    }

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
        self.autocomplete_signature_request_id = None;
        self.autocomplete_signature_items.clear();
        self.autocomplete_detail_request_id = None;
        self.autocomplete_detail_word = None;
        self.autocomplete_detail_request_path = None;
        self.autocomplete_detail_context_key = None;
        self.autocomplete_detail_popup = None;
        self.autocomplete_detail_rect = None;
        self.autocomplete_detail_placement = None;
        self.autocomplete_detail_max_scroll = 0.0;
        self.autocomplete_min_width = 0.0;
        self.reset_autocomplete_detail_size();
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

    fn autocomplete_source_function_detail(
        &self,
        item: &AutocompleteItem,
    ) -> Option<(String, Option<String>)> {
        if self.file_extension != "py"
            || !matches!(item.kind, SymbolKind::Function | SymbolKind::Builtin)
        {
            return None;
        }

        let current_module = self.file_path.as_deref().and_then(|path| {
            crate::app::events::module_path_from_definition_path(path, &self.ide_workspaces)
        });
        let text = self.editor.get_full_text();
        if let Some(detail) = crate::app::events::source_function_signature_from_text(
            &text,
            &item.word,
            current_module.as_deref(),
        ) {
            return Some((detail, current_module));
        }

        let module_path = autocomplete_detail_module_path(item)?;
        let path = autocomplete_module_source_path(module_path, &self.ide_workspaces)?;
        let source = std::fs::read_to_string(&path).ok()?;
        let source_module = crate::app::events::module_path_from_definition_path(
            &path,
            &self.ide_workspaces,
        )
        .or_else(|| Some(module_path.to_string()));
        let detail = crate::app::events::source_function_signature_from_text(
            &source,
            &item.word,
            source_module.as_deref(),
        )?;
        Some((detail, source_module))
    }

    fn autocomplete_source_variable_detail(&self, item: &AutocompleteItem) -> Option<String> {
        if self.file_extension != "py"
            || !matches!(
                item.kind,
                SymbolKind::Variable
                    | SymbolKind::Parameter
                    | SymbolKind::Argument
                    | SymbolKind::Property
            )
        {
            return None;
        }

        let lsp_type = autocomplete_detail_type_label(item.detail.as_deref());
        let module_path = item
            .module_path
            .as_deref()
            .filter(|module| completion_source_is_module_path(module));
        let text = self.editor.get_full_text();
        if let Some(detail) = autocomplete_source_variable_detail_from_text(
            &text,
            &item.word,
            module_path,
            lsp_type.as_deref(),
        ) {
            return Some(detail);
        }

        let path = module_path
            .and_then(|module| autocomplete_module_source_path(module, &self.ide_workspaces))?;
        let source = std::fs::read_to_string(path).ok()?;
        autocomplete_source_variable_detail_from_text(
            &source,
            &item.word,
            module_path,
            lsp_type.as_deref(),
        )
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
        self.reset_autocomplete_detail_size();
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
        let source_function_detail = self.autocomplete_source_function_detail(item);
        let source_detail_module_path = source_function_detail
            .as_ref()
            .and_then(|(_, module_path)| module_path.clone());
        let source_detail = source_function_detail
            .map(|(detail, _)| detail)
            .or_else(|| self.autocomplete_source_variable_detail(item));
        let has_source_detail = source_detail.is_some();
        let detail = item.detail.as_ref().filter(|s| !s.trim().is_empty());
        if source_detail.is_none() && detail.is_none() {
            self.autocomplete_detail_popup = None;
            self.autocomplete_detail_rect = None;
            self.trace_autocomplete_state("detail_refresh:no_detail");
            return;
        }
        let detail_result = if let Some(source_detail) = source_detail {
            AutocompleteDetailText {
                text: std::borrow::Cow::Owned(source_detail),
                module_path: source_detail_module_path,
            }
        } else {
            autocomplete_detail_text_for_item(item, detail.unwrap(), &self.ide_workspaces)
        };
        let module_path = detail_result.module_path.or_else(|| {
            (!has_source_detail)
                .then(|| autocomplete_detail_module_path(item).map(str::to_string))
                .flatten()
        });
        let mut detail_text = detail_result.text;
        if let Some(module) = module_path.as_deref()
            && detail_text.lines().map(str::trim).find(|line| !line.is_empty())
                == Some(module)
        {
            let stripped = detail_text
                .as_ref()
                .split_once('\n')
                .map(|(_, rest)| rest.trim_start_matches('\n'))
                .unwrap_or("");
            detail_text = std::borrow::Cow::Owned(stripped.to_string());
        }
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

    fn set_autocomplete_detail_placeholder(&mut self, text: &'static str) {
        self.autocomplete_detail_popup = Some(crate::app::mouse::HoverPopup {
            text: text.to_string(),
            spans: Vec::new(),
            line_kinds: vec![crate::lsp::HoverLineKindPublic::Text],
            inline_code_ranges: Vec::new(),
            byte_offset: self.editor.cursor,
            anchor_x: 0.0,
            anchor_y: 0.0,
            offset_x: Some(0.0),
            offset_y: Some(0.0),
            anim_progress: self.autocomplete_anim_progress,
            scroll: crate::scroll::ScrollState::new(15.0),
            layout_cache: None,
        });
        self.autocomplete_detail_rect = None;
        self.autocomplete_detail_placement = None;
        self.autocomplete_detail_max_scroll = 0.0;
        self.autocomplete_detail_selection_anchor = None;
        self.autocomplete_detail_selection_cursor = None;
        self.autocomplete_detail_selecting = false;
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

    pub fn remember_autocomplete_detail_cache(&mut self, items: &[crate::lsp::LspCompletionItem]) {
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
            self.set_autocomplete_detail_placeholder("Unknown");
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
        self.set_autocomplete_detail_placeholder("Loading...");
        if self.autocomplete_detail_word.as_deref() == Some(target_word.as_str())
            && self.autocomplete_detail_request_id.is_some()
            && self.autocomplete_detail_request_path.as_ref() == Some(&path)
            && self.autocomplete_detail_context_key.as_deref() == Some(context_key.as_str())
        {
            self.trace_autocomplete_state("detail_request:dedupe");
            return;
        }
        let Some(lsp) = self.lsp.as_mut() else {
            self.set_autocomplete_detail_placeholder("Unknown");
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

}
