use super::*;

const HIGHLIGHT_RUNTIME_TRACE_SLOW_MS: f64 = 4.0;

fn highlighter_runtime_trace_elapsed_ms(start: std::time::Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn highlighter_runtime_trace_line_count(text: &str) -> usize {
    text.as_bytes().iter().filter(|&&b| b == b'\n').count() + 1
}

fn highlighter_runtime_trace_should_log(text_len: usize, priority: bool, elapsed_ms: f64) -> bool {
    !cfg!(test)
        && (text_len >= TREE_SITTER_HIGHLIGHT_MAX_BYTES
            || priority
            || elapsed_ms >= HIGHLIGHT_RUNTIME_TRACE_SLOW_MS)
}

impl Highlighter {
    fn shrink_sync_byte_colors_for_small_text(&mut self) {
        if self.sync_text.len() >= 512 * 1024 {
            return;
        }
        let cap_limit = self.sync_text.len().saturating_mul(2).max(64 * 1024);
        if self.sync_byte_colors_buf.capacity() > cap_limit {
            self.sync_byte_colors_buf.shrink_to_fit();
        }
    }

    pub fn restore_cached_view(&mut self, version: u64, text: String, ext: String) {
        if highlighter_runtime_trace_should_log(text.len(), false, 0.0) {
            eprintln!(
                "[HL TRACE runtime:restore] ver={} bytes={} lines={} ext={} spans={}",
                version,
                text.len(),
                highlighter_runtime_trace_line_count(&text),
                ext,
                self.spans.len(),
            );
        }
        self.current_request_id = self.current_request_id.wrapping_add(1).max(1);
        self.sync_text = text.clone();
        self.sync_ext = ext.clone();
        self.sync_tree = None;
        self.current_version = version;
        self.is_complete = true;
        self.pending_priority_anchor = None;
        let _ = self.tx.send(HighlighterMessage::Restore {
            text,
            ext,
            spans: self.spans.clone(),
        });
    }

    pub fn restart_cached_view(&mut self, version: u64, text: String, ext: String, anchor: usize) {
        let priority = should_prioritize_front_highlight(&ext, &text);
        if highlighter_runtime_trace_should_log(text.len(), priority, 0.0) {
            eprintln!(
                "[HL TRACE runtime:restart] req_next={} ver={} bytes={} lines={} ext={} priority={} anchor={}",
                self.current_request_id.wrapping_add(1).max(1),
                version,
                text.len(),
                highlighter_runtime_trace_line_count(&text),
                ext,
                priority,
                anchor,
            );
        }
        self.current_request_id = self.current_request_id.wrapping_add(1).max(1);
        let request_id = self.current_request_id;
        self.sync_text = text.clone();
        self.sync_ext = ext.clone();
        self.sync_tree = None;
        self.current_version = version;
        self.is_complete = false;
        self.pending_priority_anchor = None;
        let _ = self.tx.send(HighlighterMessage::Reset {
            request_id,
            version,
            text,
            ext,
            priority_anchor: anchor,
        });
    }

    pub fn reset(&mut self, version: u64, text: String, ext: String, priority_anchor: usize) {
        let priority = should_prioritize_front_highlight(&ext, &text);
        if highlighter_runtime_trace_should_log(text.len(), priority, 0.0) {
            eprintln!(
                "[HL TRACE runtime:reset] req_next={} ver={} bytes={} lines={} ext={} priority={} anchor={} old_spans={} old_complete={}",
                self.current_request_id.wrapping_add(1).max(1),
                version,
                text.len(),
                highlighter_runtime_trace_line_count(&text),
                ext,
                priority,
                priority_anchor,
                self.spans.len(),
                self.is_complete,
            );
        }
        self.current_request_id = self.current_request_id.wrapping_add(1).max(1);
        let request_id = self.current_request_id;
        self.sync_text = text.clone();
        self.sync_ext = ext.clone();
        self.sync_tree = None;
        self.is_complete = false;
        self.pending_priority_anchor = None;
        let _ = self.tx.send(HighlighterMessage::Reset {
            request_id,
            version,
            text,
            ext,
            priority_anchor,
        });
    }

    pub fn apply_edits(
        &mut self,
        version: u64,
        edits: Vec<SyncEdit>,
        edit_start_byte: Option<usize>,
        edit_end_byte: Option<usize>,
    ) {
        if !edits.is_empty() {
            let (invalidate_start_byte, invalidate_end_byte) =
                sync_edit_invalidation_byte_range(&edits);
            let worker_edit_start_byte = edit_start_byte.or(invalidate_start_byte);
            let worker_edit_end_byte = edit_end_byte.or(invalidate_end_byte);
            let trace = highlighter_runtime_trace_should_log(
                self.sync_text.len(),
                edit_start_byte.is_none() || edit_end_byte.is_none(),
                0.0,
            );
            if trace {
                let mut insert_bytes = 0usize;
                let mut delete_bytes = 0usize;
                for edit in &edits {
                    match edit {
                        SyncEdit::Insert { text, .. } => insert_bytes += text.len(),
                        SyncEdit::Delete { len, .. } => delete_bytes += *len,
                    }
                }
                eprintln!(
                    "[HL TRACE runtime:apply_edits] req={} ver={} current_ver={} bytes={} lines={} edits={} insert_bytes={} delete_bytes={} edit_range={:?} sent_range={:?} provided_range_missing={}",
                    self.current_request_id,
                    version,
                    self.current_version,
                    self.sync_text.len(),
                    highlighter_runtime_trace_line_count(&self.sync_text),
                    edits.len(),
                    insert_bytes,
                    delete_bytes,
                    edit_start_byte.zip(edit_end_byte),
                    worker_edit_start_byte.zip(worker_edit_end_byte),
                    edit_start_byte.is_none() || edit_end_byte.is_none(),
                );
            }
            self.pending_priority_anchor = None;
            let _ = self.tx.send(HighlighterMessage::Edits {
                request_id: self.current_request_id,
                version,
                edits,
                edit_start_byte: worker_edit_start_byte,
                edit_end_byte: worker_edit_end_byte,
                invalidate_start_byte,
                invalidate_end_byte,
            });
        }
    }

    pub fn request_priority_highlight(&mut self, version: u64, anchor: usize) -> bool {
        if self.current_request_id == 0 || self.sync_text.is_empty() {
            return false;
        }
        let lang_name = lang_name_for_ext_and_text(&self.sync_ext, &self.sync_text);
        if !should_skip_full_highlight(lang_name, &self.sync_text) {
            return false;
        }
        let anchor = anchor.min(self.sync_text.len());
        if self
            .spans
            .iter()
            .any(|span| span.start <= anchor && anchor < span.end)
        {
            return false;
        }
        if self
            .pending_priority_anchor
            .is_some_and(|pending| pending.abs_diff(anchor) < PRIORITY_HIGHLIGHT_HEAD_MIN_BYTES / 2)
        {
            return false;
        }
        self.pending_priority_anchor = Some(anchor);
        let _ = self.tx.send(HighlighterMessage::Priority {
            request_id: self.current_request_id,
            version,
            priority_anchor: anchor,
        });
        true
    }

    pub fn has_pending_priority_highlight(&self) -> bool {
        self.pending_priority_anchor.is_some()
    }

    pub fn poll(&mut self, current_editor_version: u64) -> bool {
        let mut updated = false;
        while let Ok((
            request_id,
            ver,
            spans,
            completions,
            foldable_ranges,
            syntax_errors,
            tree,
            is_complete,
        )) = self.rx.try_recv()
        {
            updated |= self.apply_poll_result(
                request_id,
                current_editor_version,
                ver,
                spans,
                completions,
                foldable_ranges,
                syntax_errors,
                tree,
                is_complete,
            );
        }
        updated
    }

    fn apply_poll_result(
        &mut self,
        request_id: u64,
        current_editor_version: u64,
        ver: u64,
        spans: Vec<ColorSpan>,
        completions: Vec<CompletionItem>,
        foldable_ranges: Vec<(usize, usize, bool, bool)>,
        syntax_errors: Vec<(usize, usize)>,
        tree: Option<tree_sitter::Tree>,
        is_complete: bool,
    ) -> bool {
        let trace = highlighter_runtime_trace_should_log(self.sync_text.len(), !is_complete, 0.0);
        if request_id != self.current_request_id
            || ver != current_editor_version
            || ver < self.current_version
        {
            if trace {
                eprintln!(
                    "[HL TRACE runtime:poll_drop] req={} current_req={} ver={} editor_ver={} current_ver={} spans={} completions={} complete={}",
                    request_id,
                    self.current_request_id,
                    ver,
                    current_editor_version,
                    self.current_version,
                    spans.len(),
                    completions.len(),
                    is_complete,
                );
            }
            return false;
        }

        self.current_version = ver;
        self.spans = spans;
        self.completions = completions;
        self.foldable_ranges = foldable_ranges;
        self.syntax_errors = syntax_errors;
        self.sync_tree = tree;
        self.is_complete = is_complete;
        self.pending_priority_anchor = None;
        if is_complete {
            self.shrink_sync_byte_colors_for_small_text();
        }
        if trace {
            eprintln!(
                "[HL TRACE runtime:poll_apply] req={} ver={} spans={} completions={} folds={} errors={} complete={} tree={} bytes={}",
                request_id,
                ver,
                self.spans.len(),
                self.completions.len(),
                self.foldable_ranges.len(),
                self.syntax_errors.len(),
                self.is_complete,
                self.sync_tree.is_some(),
                self.sync_text.len(),
            );
        }
        true
    }

    /// Блокирует текущий поток (до `timeout`) ожидая первый результат для `version`.
    /// Возвращает `true` если результат получен и применён до таймаута.
    /// Используется при открытии файла, чтобы первый кадр уже содержал подсветку.
    pub fn wait_for_first_result(&mut self, version: u64, timeout: std::time::Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let now = std::time::Instant::now();
            if now >= deadline {
                return false;
            }
            let remaining = deadline - now;
            match self.rx.recv_timeout(remaining) {
                Ok((
                    request_id,
                    ver,
                    spans,
                    completions,
                    foldable_ranges,
                    syntax_errors,
                    tree,
                    is_complete,
                )) => {
                    if self.apply_poll_result(
                        request_id,
                        version,
                        ver,
                        spans,
                        completions,
                        foldable_ranges,
                        syntax_errors,
                        tree,
                        is_complete,
                    ) {
                        // Дренируем оставшиеся ожидающие результаты
                        self.poll(version);
                        return true;
                    }
                    // Устаревший результат — ждём дальше
                }
                Err(_) => return false,
            }
        }
    }

    pub fn shift_insert(&mut self, offset: usize, len: usize, text_opt: Option<&str>) {
        if let Some(text) = text_opt {
            let start_byte = offset;
            let old_end_byte = offset;
            let new_end_byte = offset + text.len();
            let start_position = get_point(&self.sync_text, start_byte);
            let old_end_position = start_position;
            if offset <= self.sync_text.len() && self.sync_text.is_char_boundary(offset) {
                self.sync_text.insert_str(offset, text);
                let new_end_position = get_point(&self.sync_text, new_end_byte);
                if let Some(tree) = &mut self.sync_tree {
                    tree.edit(&tree_sitter::InputEdit {
                        start_byte,
                        old_end_byte,
                        new_end_byte,
                        start_position,
                        old_end_position,
                        new_end_position,
                    });
                }
            } else {
                self.sync_tree = None;
            }
        } else {
            self.sync_tree = None;
        }

        let prev_offset = offset.saturating_sub(1);
        let mut predicted_color = DRACULA_FG;

        for span in &self.spans {
            if span.start <= prev_offset && span.end > prev_offset {
                predicted_color = span.color;
                break;
            }
        }

        if let Some(t) = text_opt {
            match t.trim() {
                "+" | "-" | "*" | "/" | "%" | "=" | "==" | "!=" | "<" | ">" | "<=" | ">=" | "&"
                | "|" | "^" | "~" | ":" => predicted_color = DRACULA_PINK,
                "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => {
                    predicted_color = DRACULA_PURPLE
                }
                "." | "," | "(" | ")" | "[" | "]" | "{" | "}" => predicted_color = DRACULA_FG,
                "import" | "from" | "if" | "else" | "elif" | "for" | "while" | "return" | "def"
                | "class" | "let" | "const" | "fn" | "mut" | "pub" | "struct" | "impl"
                | "match" | "break" | "continue" | "in" | "as" | "await" | "async" | "yield"
                | "try" | "except" | "finally" | "raise" | "with" => predicted_color = DRACULA_PINK,
                "True" | "False" | "None" | "true" | "false" | "null" => {
                    predicted_color = DRACULA_PINK
                }
                "int" | "float" | "str" | "bool" | "String" => predicted_color = DRACULA_CYAN,
                "self" | "cls" => predicted_color = DRACULA_PURPLE,
                _ => {}
            }
        }

        let mut new_spans = Vec::new();
        for span in &mut self.spans {
            if span.start >= offset {
                span.start += len;
                span.end += len;
            } else if span.end > offset {
                let old_end = span.end;
                span.end = offset;

                new_spans.push(ColorSpan {
                    start: offset,
                    end: offset + len,
                    color: predicted_color,
                });

                new_spans.push(ColorSpan {
                    start: offset + len,
                    end: old_end + len,
                    color: span.color,
                });
            } else if span.end == offset {
                new_spans.push(ColorSpan {
                    start: offset,
                    end: offset + len,
                    color: predicted_color,
                });
            }
        }

        if !new_spans.is_empty() {
            self.spans.extend(new_spans);
            self.spans.sort_by_key(|s| s.start);
            let mut merged = Vec::new();
            if !self.spans.is_empty() {
                let mut current = self.spans[0].clone();
                for i in 1..self.spans.len() {
                    let next = &self.spans[i];
                    if next.start <= current.end {
                        if next.color == current.color {
                            current.end = current.end.max(next.end);
                        } else if next.end > current.end {
                            merged.push(current.clone());
                            current = next.clone();
                            current.start = current.start.max(merged.last().unwrap().end);
                        }
                    } else {
                        merged.push(current);
                        current = next.clone();
                    }
                }
                if current.start < current.end {
                    merged.push(current);
                }
            }
            self.spans = merged;
            self.spans.retain(|s| s.start < s.end);
        } else {
            self.spans.push(ColorSpan {
                start: offset,
                end: offset + len,
                color: predicted_color,
            });
        }
    }

    pub fn shift_delete(&mut self, offset: usize, len: usize) {
        let start_byte = offset;
        let old_end_byte = offset + len;
        let new_end_byte = offset;
        let start_position = get_point(&self.sync_text, start_byte);
        let old_end_position = get_point(&self.sync_text, old_end_byte);
        if offset + len <= self.sync_text.len()
            && self.sync_text.is_char_boundary(offset)
            && self.sync_text.is_char_boundary(offset + len)
        {
            self.sync_text.replace_range(offset..offset + len, "");
            if let Some(tree) = &mut self.sync_tree {
                tree.edit(&tree_sitter::InputEdit {
                    start_byte,
                    old_end_byte,
                    new_end_byte,
                    start_position,
                    old_end_position,
                    new_end_position: start_position,
                });
            }
        } else {
            self.sync_tree = None;
        }

        let end_del = offset + len;
        for span in &mut self.spans {
            if span.start >= end_del {
                span.start -= len;
            } else if span.start > offset {
                span.start = offset;
            }
            if span.end >= end_del {
                span.end -= len;
            } else if span.end > offset {
                span.end = offset;
            }
        }
        self.spans.retain(|s| s.start < s.end);
    }

    pub fn sync_highlight_after_edit(
        &mut self,
        version: u64,
        edit_start_byte: Option<usize>,
        edit_end_byte: Option<usize>,
        invalidate_start_byte: Option<usize>,
        invalidate_end_byte: Option<usize>,
        timeout: std::time::Duration,
    ) -> bool {
        let total_start = std::time::Instant::now();
        let text = self.sync_text.as_str();
        let priority = should_prioritize_front_highlight(&self.sync_ext, text);
        let trace = highlighter_runtime_trace_should_log(text.len(), priority, 0.0);
        if text.is_empty() || priority {
            if trace {
                eprintln!(
                    "[HL TRACE runtime:sync_edit_skip] ver={} bytes={} lines={} ext={} reason={} edit_range={:?} invalidate={:?}",
                    version,
                    text.len(),
                    highlighter_runtime_trace_line_count(text),
                    self.sync_ext,
                    if text.is_empty() { "empty" } else { "priority_or_large" },
                    edit_start_byte.zip(edit_end_byte),
                    invalidate_start_byte.zip(invalidate_end_byte),
                );
            }
            return false;
        }

        let lang_name = lang_name_for_ext_and_text(&self.sync_ext, text);
        let Some((lang, queries)) = get_ts_config(lang_name) else {
            if trace {
                eprintln!(
                    "[HL TRACE runtime:sync_edit_skip] ver={} bytes={} ext={} reason=no_ts_config",
                    version,
                    text.len(),
                    self.sync_ext,
                );
            }
            return false;
        };
        if self.sync_parser.set_language(&lang).is_err() {
            if trace {
                eprintln!(
                    "[HL TRACE runtime:sync_edit_skip] ver={} bytes={} ext={} lang={} reason=set_language_failed",
                    version,
                    text.len(),
                    self.sync_ext,
                    lang_name,
                );
            }
            return false;
        }

        let deadline = std::time::Instant::now() + timeout;
        let mut progress = |_state: &tree_sitter::ParseState| {
            if std::time::Instant::now() >= deadline {
                std::ops::ControlFlow::Break(())
            } else {
                std::ops::ControlFlow::Continue(())
            }
        };
        let options = tree_sitter::ParseOptions::new().progress_callback(&mut progress);
        let bytes = text.as_bytes();
        let len = bytes.len();
        let parse_start = std::time::Instant::now();
        let parsed_tree = self.sync_parser.parse_with_options(
            &mut |i, _| (i < len).then(|| &bytes[i..]).unwrap_or_default(),
            self.sync_tree.as_ref(),
            Some(options),
        );
        let parse_ms = highlighter_runtime_trace_elapsed_ms(parse_start);
        let Some(tree) = parsed_tree else {
            let total_ms = highlighter_runtime_trace_elapsed_ms(total_start);
            if trace || highlighter_runtime_trace_should_log(text.len(), false, total_ms) {
                eprintln!(
                    "[HL TRACE runtime:sync_edit_fail] ver={} bytes={} lines={} ext={} lang={} reason=parse_timeout total_ms={:.2} parse_ms={:.2} timeout_ms={}",
                    version,
                    text.len(),
                    highlighter_runtime_trace_line_count(text),
                    self.sync_ext,
                    lang_name,
                    total_ms,
                    parse_ms,
                    timeout.as_millis(),
                );
            }
            return false;
        };

        let range = if let (Some(sb), Some(eb)) = (edit_start_byte, edit_end_byte) {
            sb.saturating_sub(1000)..(eb + 1000).min(text.len())
        } else {
            0..text.len()
        };
        let mut spans = Vec::new();
        let query_start = std::time::Instant::now();
        collect_query_highlight_spans(
            &lang,
            lang_name,
            &queries,
            &tree,
            text,
            &mut self.sync_query_cache,
            Some(range),
            &mut spans,
        );
        let query_ms = highlighter_runtime_trace_elapsed_ms(query_start);
        let raw_span_count = spans.len();

        let merge_start = std::time::Instant::now();
        let merged_spans = merge_highlight_spans(
            self.spans.clone(),
            spans,
            lang_name,
            text,
            false,
            expand_highlight_invalidation_range(text, invalidate_start_byte, invalidate_end_byte),
        );
        let merge_ms = highlighter_runtime_trace_elapsed_ms(merge_start);
        let merged_span_count = merged_spans.len();
        let flatten_start = std::time::Instant::now();
        let flat_spans = flatten_spans(
            merged_spans,
            text.len(),
            text,
            &mut self.sync_byte_colors_buf,
            &[],
            !lang_name.is_empty() && lang_name != "bash",
            false,
        );
        let flatten_ms = highlighter_runtime_trace_elapsed_ms(flatten_start);
        let pre_mutation_total_ms = highlighter_runtime_trace_elapsed_ms(total_start);
        let should_log_done =
            trace || highlighter_runtime_trace_should_log(text.len(), false, pre_mutation_total_ms);
        let text_len = text.len();
        let line_count = if should_log_done {
            highlighter_runtime_trace_line_count(text)
        } else {
            0
        };
        let ext_label = self.sync_ext.clone();

        self.sync_tree = Some(tree);
        self.current_version = version;
        self.is_complete = true;
        self.spans = flat_spans;
        let shrink_start = std::time::Instant::now();
        self.shrink_sync_byte_colors_for_small_text();
        let shrink_ms = highlighter_runtime_trace_elapsed_ms(shrink_start);
        let total_ms = highlighter_runtime_trace_elapsed_ms(total_start);
        if should_log_done || highlighter_runtime_trace_should_log(text_len, false, total_ms) {
            eprintln!(
                "[HL TRACE runtime:sync_edit_done] ver={} bytes={} lines={} ext={} lang={} raw_spans={} merged_spans={} flat_spans={} range={:?} invalidate={:?} total_ms={:.2} parse_ms={:.2} query_ms={:.2} merge_ms={:.2} flatten_ms={:.2} shrink_ms={:.2} timeout_ms={}",
                version,
                text_len,
                line_count,
                ext_label,
                lang_name,
                raw_span_count,
                merged_span_count,
                self.spans.len(),
                edit_start_byte.zip(edit_end_byte),
                invalidate_start_byte.zip(invalidate_end_byte),
                total_ms,
                parse_ms,
                query_ms,
                merge_ms,
                flatten_ms,
                shrink_ms,
                timeout.as_millis(),
            );
        }
        true
    }
}
pub(super) fn get_bracket_color(depth: usize) -> [f32; 4] {
    if depth == 0 {
        return DRACULA_FG;
    }
    match 1 + (depth - 1) % 5 {
        1 => DRACULA_GREEN,
        2 => DRACULA_CYAN,
        3 => DRACULA_ORANGE,
        4 => DRACULA_YELLOW,
        5 => DRACULA_PURPLE,
        _ => DRACULA_FG,
    }
}

pub(super) fn flatten_spans(
    mut spans: Vec<ColorSpan>,
    len: usize,
    text: &str,
    byte_colors: &mut Vec<[f32; 4]>,
    error_ranges: &[(usize, usize)],
    apply_rainbow_brackets: bool,
    is_log_or_huge: bool,
) -> Vec<ColorSpan> {
    if spans.is_empty() && error_ranges.is_empty() && (is_log_or_huge || !apply_rainbow_brackets) {
        return vec![ColorSpan {
            start: 0,
            end: len,
            color: DRACULA_FG,
        }];
    }

    spans.sort_by_key(|s| std::cmp::Reverse(s.end - s.start));

    byte_colors.clear();
    byte_colors.resize(len, DRACULA_FG);

    for span in spans {
        for i in span.start..span.end.min(len) {
            byte_colors[i] = span.color;
        }
    }

    let text_bytes = text.as_bytes();

    for i in 0..len {
        let b = text_bytes[i];
        if byte_colors[i] == MARKER_INTERPOLATION {
            if b == b'{' || b == b'}' {
                byte_colors[i] = DRACULA_ORANGE;
            } else {
                byte_colors[i] = DRACULA_FG;
            }
        }
    }

    if apply_rainbow_brackets {
        let mut depth_round = 0usize;
        let mut depth_square = 0usize;
        let mut depth_curly = 0usize;

        for i in 0..len {
            if byte_colors[i] != DRACULA_COMMENT
                && (byte_colors[i] == DRACULA_FG
                    || byte_colors[i] == DRACULA_GREEN
                    || byte_colors[i] == DRACULA_CYAN
                    || byte_colors[i] == DRACULA_ORANGE
                    || byte_colors[i] == DRACULA_YELLOW
                    || byte_colors[i] == DRACULA_PURPLE)
            {
                match text_bytes[i] {
                    b'(' => {
                        byte_colors[i] = get_bracket_color(depth_round);
                        depth_round += 1;
                    }
                    b')' => {
                        if depth_round > 0 {
                            depth_round -= 1;
                        }
                        byte_colors[i] = get_bracket_color(depth_round);
                    }
                    b'[' => {
                        byte_colors[i] = get_bracket_color(depth_square);
                        depth_square += 1;
                    }
                    b']' => {
                        if depth_square > 0 {
                            depth_square -= 1;
                        }
                        byte_colors[i] = get_bracket_color(depth_square);
                    }
                    b'{' => {
                        byte_colors[i] = get_bracket_color(depth_curly);
                        depth_curly += 1;
                    }
                    b'}' => {
                        if depth_curly > 0 {
                            depth_curly -= 1;
                        }
                        byte_colors[i] = get_bracket_color(depth_curly);
                    }
                    _ => {}
                }
            }
        }
    }

    // The logic to restore colors for ranges with syntax errors was removed.
    // It was using stale byte offsets from before the edit, causing highlighting to shift.
    // Now, text with syntax errors will just use the default color until the syntax is valid again.

    let mut flat = Vec::new();
    if len == 0 {
        return flat;
    }

    let mut current_color = byte_colors[0];
    let mut start = 0;
    for i in 1..len {
        if byte_colors[i] != current_color {
            flat.push(ColorSpan {
                start,
                end: i,
                color: current_color,
            });
            start = i;
            current_color = byte_colors[i];
        }
    }
    flat.push(ColorSpan {
        start,
        end: len,
        color: current_color,
    });
    flat
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlighter_flatten_spans_overlays_colors_and_brackets_end_to_end() {
        let text = "fn call((x))";
        let mut byte_colors = Vec::new();
        let spans = vec![ColorSpan {
            start: 0,
            end: 2,
            color: DRACULA_PINK,
        }];

        let flat = flatten_spans(spans, text.len(), text, &mut byte_colors, &[], true, false);

        assert_eq!(flat.first().map(|span| span.color), Some(DRACULA_PINK));
        assert_eq!(byte_colors[0], DRACULA_PINK);
        let nested_open = text.find("((").unwrap() + 1;
        let nested_close = text.find("))").unwrap();
        assert_ne!(byte_colors[nested_open], DRACULA_FG);
        assert_ne!(byte_colors[nested_close], DRACULA_FG);
    }

    #[test]
    fn highlighter_flatten_spans_returns_plain_span_for_logs_without_input_spans() {
        let mut byte_colors = Vec::new();
        let flat = flatten_spans(Vec::new(), 4, "text", &mut byte_colors, &[], false, true);

        assert_eq!(flat.len(), 1);
        assert_eq!(flat[0].start, 0);
        assert_eq!(flat[0].end, 4);
        assert_eq!(flat[0].color, DRACULA_FG);
    }

    #[test]
    fn highlighter_poll_ignores_non_current_versions_without_advancing_watermark() {
        let mut highlighter = Highlighter::new();
        let future_span = ColorSpan {
            start: 0,
            end: 6,
            color: DRACULA_GREEN,
        };
        let current_span = ColorSpan {
            start: 0,
            end: 6,
            color: DRACULA_PINK,
        };
        highlighter.current_request_id = 1;

        let future_applied = highlighter.apply_poll_result(
            1,
            2,
            3,
            vec![future_span],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            None,
            true,
        );
        assert!(!future_applied);
        assert_eq!(highlighter.current_version, 0);
        assert!(highlighter.spans.is_empty());

        let current_applied = highlighter.apply_poll_result(
            1,
            2,
            2,
            vec![current_span],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            None,
            true,
        );
        assert!(current_applied);
        assert_eq!(highlighter.current_version, 2);
        assert_eq!(highlighter.spans[0].color, DRACULA_PINK);
    }

    #[test]
    fn highlighter_sync_parse_after_seed_colors_python_constant_immediately() {
        let mut highlighter = Highlighter::new();
        highlighter.reset(1, "S\n".to_string(), "py".to_string(), 0);
        assert!(highlighter.wait_for_first_result(1, std::time::Duration::from_secs(2)));
        assert!(highlighter.sync_tree.is_some());

        highlighter.shift_insert(1, 1, Some("S"));
        assert!(highlighter.sync_highlight_after_edit(
            2,
            Some(0),
            Some(2),
            Some(1),
            Some(2),
            std::time::Duration::from_millis(10),
        ));
        assert!(
            highlighter
                .spans
                .iter()
                .any(|span| span.start <= 0 && span.end >= 2 && span.color == DRACULA_PURPLE)
        );
    }

    #[test]
    fn highlighter_sync_parse_keeps_python_parameters_colored() {
        let mut highlighter = Highlighter::new();
        let source = "def f(session):\n    BoxRepository(session)\n";
        highlighter.reset(1, source.to_string(), "py".to_string(), 0);
        assert!(highlighter.wait_for_first_result(1, std::time::Duration::from_secs(2)));

        let insert_at = source.find("    BoxRepository").unwrap();
        highlighter.shift_insert(insert_at, 7, Some("    if\n"));
        assert!(highlighter.sync_highlight_after_edit(
            2,
            Some(insert_at),
            Some(insert_at + 7),
            Some(insert_at),
            Some(insert_at + 7),
            std::time::Duration::from_millis(10),
        ));

        let text = &highlighter.sync_text;
        let session_start = text.rfind("session").unwrap();
        let session_end = session_start + "session".len();
        assert!(highlighter.spans.iter().any(|span| {
            span.start <= session_start && span.end >= session_end && span.color == DRACULA_ORANGE
        }));
    }

    #[test]
    fn highlighter_sync_parse_clears_stale_python_self_color_after_delete() {
        let mut highlighter = Highlighter::new();
        highlighter.reset(1, "self\n".to_string(), "py".to_string(), 0);
        assert!(highlighter.wait_for_first_result(1, std::time::Duration::from_secs(2)));

        highlighter.shift_delete(3, 1);
        assert!(highlighter.sync_highlight_after_edit(
            2,
            Some(0),
            Some(3),
            Some(3),
            Some(3),
            std::time::Duration::from_millis(10),
        ));
        assert!(
            !highlighter
                .spans
                .iter()
                .any(|span| span.start <= 0 && span.end >= 3 && span.color == DRACULA_PURPLE)
        );
    }

    #[test]
    fn highlighter_shift_insert_predicts_colors_splits_and_merges_spans() {
        let mut highlighter = Highlighter::new();
        highlighter.spans = vec![
            ColorSpan {
                start: 0,
                end: 4,
                color: DRACULA_GREEN,
            },
            ColorSpan {
                start: 8,
                end: 12,
                color: DRACULA_CYAN,
            },
        ];

        highlighter.shift_insert(4, 2, Some("return"));
        assert!(
            highlighter
                .spans
                .iter()
                .any(|span| span.start == 4 && span.end == 6 && span.color == DRACULA_PINK)
        );
        assert!(
            highlighter
                .spans
                .iter()
                .any(|span| span.start == 10 && span.end == 14 && span.color == DRACULA_CYAN)
        );

        highlighter.spans = vec![ColorSpan {
            start: 0,
            end: 6,
            color: DRACULA_GREEN,
        }];
        highlighter.shift_insert(3, 1, Some("9"));
        assert!(
            highlighter
                .spans
                .iter()
                .any(|span| span.start == 3 && span.end == 4 && span.color == DRACULA_PURPLE)
        );
        assert!(
            highlighter
                .spans
                .iter()
                .any(|span| span.start == 4 && span.end == 7 && span.color == DRACULA_GREEN)
        );

        highlighter.spans.clear();
        highlighter.shift_insert(0, 3, Some("str"));
        assert_eq!(highlighter.spans.len(), 1);
        assert_eq!(highlighter.spans[0].color, DRACULA_CYAN);
    }

    #[test]
    fn highlighter_shift_delete_clamps_overlapping_spans() {
        let mut highlighter = Highlighter::new();
        highlighter.spans = vec![
            ColorSpan {
                start: 0,
                end: 4,
                color: DRACULA_GREEN,
            },
            ColorSpan {
                start: 5,
                end: 10,
                color: DRACULA_ORANGE,
            },
            ColorSpan {
                start: 12,
                end: 15,
                color: DRACULA_CYAN,
            },
        ];

        highlighter.shift_delete(3, 6);

        assert!(
            highlighter
                .spans
                .iter()
                .any(|span| span.start == 0 && span.end == 3 && span.color == DRACULA_GREEN)
        );
        assert!(
            highlighter
                .spans
                .iter()
                .any(|span| span.start == 3 && span.end == 4 && span.color == DRACULA_ORANGE)
        );
        assert!(
            highlighter
                .spans
                .iter()
                .any(|span| span.start == 6 && span.end == 9 && span.color == DRACULA_CYAN)
        );
        assert!(highlighter.spans.iter().all(|span| span.start < span.end));
    }

    #[test]
    fn highlighter_flatten_spans_handles_empty_and_interpolation_markers() {
        let mut byte_colors = Vec::new();
        let empty = flatten_spans(Vec::new(), 0, "", &mut byte_colors, &[], true, false);
        assert!(empty.is_empty());

        let text = "{x}";
        let flat = flatten_spans(
            vec![ColorSpan {
                start: 0,
                end: text.len(),
                color: MARKER_INTERPOLATION,
            }],
            text.len(),
            text,
            &mut byte_colors,
            &[],
            false,
            false,
        );

        assert_eq!(byte_colors[0], DRACULA_ORANGE);
        assert_eq!(byte_colors[1], DRACULA_FG);
        assert_eq!(byte_colors[2], DRACULA_ORANGE);
        assert_eq!(flat.len(), 3);
    }

    #[test]
    fn highlighter_sync_parse_skips_files_above_tree_sitter_budget() {
        let mut highlighter = Highlighter::new();
        highlighter.sync_ext = "py".to_string();
        highlighter.sync_text = "value = 1\n".repeat(TREE_SITTER_HIGHLIGHT_MAX_BYTES / 10 + 2);

        assert!(!highlighter.sync_highlight_after_edit(
            1,
            None,
            None,
            None,
            None,
            std::time::Duration::from_millis(10),
        ));
    }

    #[test]
    fn highlighter_priority_result_uses_anchor_not_file_start() {
        let mut highlighter = Highlighter::new();
        let imports = "import os\nimport sys\n\n";
        let prefix = "# pad\n".repeat(TREE_SITTER_HIGHLIGHT_MAX_LINES + 20);
        let text = format!("{imports}{prefix}def target():\n    return 'x'\n");
        let anchor = text.find("target").unwrap();

        highlighter.reset(11, text.clone(), "py".to_string(), anchor);
        assert!(highlighter.wait_for_first_result(11, std::time::Duration::from_secs(2)));

        assert!(highlighter.spans.iter().any(|span| {
            span.start <= anchor
                && span.end >= anchor + "target".len()
                && span.color == DRACULA_GREEN
        }));
        assert!(
            highlighter
                .foldable_ranges
                .iter()
                .any(|&(start, end, is_autofold, _)| start == 0 && end <= anchor && is_autofold)
        );
    }

    #[test]
    fn python_priority_range_starts_at_real_top_level_statement() {
        let text = "class Example:\n    \"\"\"doc\n    still doc\n    \"\"\"\n    def target(self) -> str:\n        return \"ok\"\n\nclass Next:\n    pass\n";
        let anchor = text.find("target").unwrap();
        let range = priority_highlight_range("py", text, anchor);

        assert_eq!(range.start, 0);
        assert!(range.end >= text.find("class Next").unwrap());
        assert!(range.end <= text.len());
    }

    #[test]
    fn python_priority_range_covers_visible_window_before_anchor() {
        let text = "class Prev:\n    @overload\n    def __new__(self) -> Self: ...\n\n@overload\ndef max(arg: int) -> int:\n    \"\"\"doc\"\"\"\n";
        let anchor = text.find("max").unwrap();
        let range = priority_highlight_range("py", text, anchor);

        assert_eq!(range.start, 0);
        assert_eq!(&text[range.clone()], text);
    }

    #[test]
    fn priority_range_from_viewport_top_covers_following_minimap_window() {
        let mut text = String::new();
        let mut line_starts = Vec::new();
        for idx in 0..5000 {
            line_starts.push(text.len());
            text.push_str(&format!("value_{idx} = {idx}\n"));
        }
        let anchor = line_starts[1200];
        let lower_visible_line = line_starts[1500];
        let range = priority_highlight_range("py", &text, anchor);

        assert!(range.start <= anchor);
        assert!(range.end > lower_visible_line);
    }

    #[test]
    fn highlighter_full_result_follows_priority_result() {
        let mut highlighter = Highlighter::new();
        let text = "def f():\n    return 'x'\n".repeat(TREE_SITTER_HIGHLIGHT_MAX_LINES + 2);
        assert!(text.len() < TREE_SITTER_HIGHLIGHT_MAX_BYTES);
        assert!(should_prioritize_front_highlight("py", &text));

        highlighter.reset(12, text.clone(), "py".to_string(), 0);
        assert!(highlighter.wait_for_first_result(12, std::time::Duration::from_secs(2)));
        let first_span_end = highlighter
            .spans
            .iter()
            .map(|span| span.end)
            .max()
            .unwrap_or(0);
        assert!(first_span_end < text.len());

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while highlighter
            .spans
            .iter()
            .map(|span| span.end)
            .max()
            .unwrap_or(0)
            < text.len()
            && std::time::Instant::now() < deadline
        {
            highlighter.poll(12);
        }
        assert!(highlighter.spans.iter().any(|span| span.end == text.len()));
    }

    #[test]
    fn highlighter_huge_file_stops_after_priority_result() {
        let mut highlighter = Highlighter::new();
        let text = "value = 1\n".repeat(TREE_SITTER_FULL_HIGHLIGHT_MAX_LINES + 10);
        assert!(should_skip_full_highlight("py", &text));

        highlighter.reset(21, text.clone(), "py".to_string(), 0);
        assert!(highlighter.wait_for_first_result(21, std::time::Duration::from_secs(2)));

        let max_span_end = highlighter
            .spans
            .iter()
            .map(|span| span.end)
            .max()
            .unwrap_or(0);
        assert!(max_span_end < text.len());
        assert!(highlighter.is_complete);
        assert!(highlighter.completions.iter().any(|item| item.word == "print"));
        assert!(!highlighter.poll(21));
    }

    #[test]
    fn highlighter_huge_file_loads_visible_slice_on_priority_request() {
        let mut highlighter = Highlighter::new();
        let text = "value = 1\n".repeat(TREE_SITTER_FULL_HIGHLIGHT_MAX_LINES + 10_000);
        let anchor = text[..text.len() * 3 / 4]
            .rfind('\n')
            .map(|pos| pos + 1)
            .unwrap_or(0);

        highlighter.reset(22, text.clone(), "py".to_string(), 0);
        assert!(highlighter.wait_for_first_result(22, std::time::Duration::from_secs(2)));
        assert!(!highlighter
            .spans
            .iter()
            .any(|span| span.start <= anchor && anchor < span.end));

        assert!(highlighter.request_priority_highlight(22, anchor));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !highlighter
            .spans
            .iter()
            .any(|span| span.start <= anchor && anchor < span.end)
            && std::time::Instant::now() < deadline
        {
            highlighter.poll(22);
            std::thread::yield_now();
        }

        assert!(highlighter
            .spans
            .iter()
            .any(|span| span.start <= anchor && anchor < span.end));
        assert!(highlighter.is_complete);
        assert!(highlighter.completions.iter().any(|item| item.word == "print"));
    }

    #[test]
    fn highlighter_huge_edit_without_explicit_range_highlights_edit_slice() {
        let mut highlighter = Highlighter::new();
        let text = "value = 1\n".repeat(TREE_SITTER_FULL_HIGHLIGHT_MAX_LINES + 10);
        let edit_at = text[..text.len() / 2].rfind('\n').map(|pos| pos + 1).unwrap_or(0);

        highlighter.reset(31, text.clone(), "py".to_string(), 0);
        assert!(highlighter.wait_for_first_result(31, std::time::Duration::from_secs(2)));
        assert!(highlighter.is_complete);

        highlighter.shift_insert(edit_at, 1, Some("x"));
        highlighter.apply_edits(
            32,
            vec![SyncEdit::Insert {
                offset: edit_at,
                text: "x".to_string(),
            }],
            None,
            None,
        );

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while highlighter.current_version != 32 && std::time::Instant::now() < deadline {
            highlighter.poll(32);
            std::thread::yield_now();
        }

        assert_eq!(highlighter.current_version, 32);
        assert!(highlighter.is_complete);
        assert!(highlighter.completions.iter().any(|item| item.word == "print"));
        assert!(highlighter
            .spans
            .iter()
            .any(|span| span.start <= edit_at && span.end > edit_at));
    }
}
