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
        self.sync_version = version;
        self.sync_ext = ext.clone();
        self.sync_tree = None;
        self.current_version = version;
        self.is_complete = true;
        self.pending_priority_anchor = None;
        if self
            .tx
            .send(HighlighterMessage::Restore {
                version,
                text,
                ext,
                spans: self.spans.clone(),
            })
            .is_err()
        {
            self.pending_priority_anchor = None;
            self.is_complete = true;
        }
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
        self.sync_version = version;
        self.sync_ext = ext.clone();
        self.sync_tree = None;
        self.current_version = version;
        self.is_complete = false;
        self.pending_priority_anchor = None;
        if self
            .tx
            .send(HighlighterMessage::Reset {
                request_id,
                version,
                text,
                ext,
                priority_anchor: anchor,
            })
            .is_err()
        {
            self.pending_priority_anchor = None;
            self.is_complete = self.sync_highlight_after_edit(
                version,
                None,
                None,
                None,
                None,
                std::time::Duration::from_millis(20),
            );
            if !self.is_complete {
                self.spans.clear();
                self.is_complete = true;
            }
        }
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
        self.sync_version = version;
        self.sync_ext = ext.clone();
        self.sync_tree = None;
        self.is_complete = false;
        self.pending_priority_anchor = None;
        if self
            .tx
            .send(HighlighterMessage::Reset {
                request_id,
                version,
                text,
                ext,
                priority_anchor,
            })
            .is_err()
        {
            self.pending_priority_anchor = None;
            self.is_complete = self.sync_highlight_after_edit(
                version,
                None,
                None,
                None,
                None,
                std::time::Duration::from_millis(20),
            );
            if !self.is_complete {
                self.spans.clear();
                self.is_complete = true;
            }
        }
    }

    /// Test shorthand: the document is `sync_text` with `edits` applied (edits that do not
    /// fit leave it as is, like an editor that rejected them).
    #[cfg(test)]
    pub(crate) fn apply_edits(
        &mut self,
        version: u64,
        edits: Vec<SyncEdit>,
        edit_start_byte: Option<usize>,
        edit_end_byte: Option<usize>,
    ) {
        let mut document = self.sync_text.clone();
        for edit in &edits {
            let _ = apply_sync_edit_to_replica(&mut document, None, edit);
        }
        let document_len = document.len();
        self.apply_document_edits(
            version,
            edits,
            edit_start_byte,
            edit_end_byte,
            document_len,
            move || document,
        );
    }

    /// The single commit point for editor edits. Both replicas (`sync_text` here and the
    /// worker copy) apply the same ordered `edits` with `apply_sync_edit_to_replica`, and the
    /// worker only on top of the same `sync_version`, so they cannot diverge from each other.
    /// If the edits do not fit `sync_text` or the result is not `document_len` long, the
    /// replica had drifted from the editor: both are reset from `document_text()` (O(n) only
    /// then; the regular path is O(edit)).
    pub fn apply_document_edits(
        &mut self,
        version: u64,
        edits: Vec<SyncEdit>,
        edit_start_byte: Option<usize>,
        edit_end_byte: Option<usize>,
        document_len: usize,
        document_text: impl FnOnce() -> String,
    ) {
        if edits.is_empty() {
            return;
        }
        let worker_desynced = self
            ._worker
            .control
            .replica_desynced
            .swap(false, Ordering::AcqRel);
        let applied = edits.iter().all(|edit| {
            apply_sync_edit_to_replica(&mut self.sync_text, self.sync_tree.as_mut(), edit)
        });
        if !applied || worker_desynced || self.sync_text.len() != document_len {
            if !cfg!(test) {
                eprintln!(
                    "[HL TRACE runtime:replica_resync] ver={} sync_ver={} applied={} worker_desynced={} sync_bytes={} doc_bytes={}",
                    version,
                    self.sync_version,
                    applied,
                    worker_desynced,
                    self.sync_text.len(),
                    document_len,
                );
            }
            let ext = self.sync_ext.clone();
            self.reset(version, document_text(), ext, edit_start_byte.unwrap_or(0));
            return;
        }
        let base_version = self.sync_version;
        self.sync_version = version;
        {
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
            if self
                .tx
                .send(HighlighterMessage::Edits {
                    request_id: self.current_request_id,
                    base_version,
                    version,
                    edits,
                    edit_start_byte: worker_edit_start_byte,
                    edit_end_byte: worker_edit_end_byte,
                    invalidate_start_byte,
                    invalidate_end_byte,
                })
                .is_err()
            {
                self.pending_priority_anchor = None;
                self.is_complete = self.sync_highlight_after_edit(
                    version,
                    worker_edit_start_byte,
                    worker_edit_end_byte,
                    invalidate_start_byte,
                    invalidate_end_byte,
                    std::time::Duration::from_millis(12),
                );
                if !self.is_complete {
                    self.spans.clear();
                    self.is_complete = true;
                }
            }
        }
    }

    pub(crate) fn is_byte_highlighted(&self, byte: usize) -> bool {
        if self.spans.is_empty() || self.sync_text.is_empty() {
            return false;
        }
        let byte = byte.min(self.sync_text.len().saturating_sub(1));
        let idx = self.spans.partition_point(|span| span.end <= byte);
        self.spans
            .get(idx)
            .is_some_and(|span| span.start <= byte && byte < span.end)
    }

    pub(crate) fn unhighlighted_anchor_in_range(
        &self,
        start: usize,
        end: usize,
        prefer_end: bool,
    ) -> Option<usize> {
        let text_len = self.sync_text.len();
        let start = start.min(text_len);
        let end = end.min(text_len);
        if start >= end {
            return None;
        }
        if self.spans.is_empty() {
            return Some(if prefer_end { end - 1 } else { start });
        }

        if prefer_end {
            let mut cursor = end;
            let mut idx = self.spans.partition_point(|span| span.start < cursor);
            while cursor > start {
                let Some(span) = idx.checked_sub(1).and_then(|idx| self.spans.get(idx)) else {
                    return Some(cursor - 1);
                };
                if span.end < cursor {
                    return Some(cursor - 1);
                }
                cursor = cursor.min(span.start);
                idx -= 1;
            }
            None
        } else {
            let mut cursor = start;
            let mut idx = self.spans.partition_point(|span| span.end <= cursor);
            while cursor < end {
                let Some(span) = self.spans.get(idx) else {
                    return Some(cursor);
                };
                if span.start > cursor {
                    return Some(cursor);
                }
                cursor = cursor.max(span.end);
                idx += 1;
            }
            None
        }
    }

    /// `_version`: the reply carries the version of the worker replica it was built from
    /// (`HighlighterMessage::Priority`), so the caller's version is not sent.
    pub fn request_priority_highlight(&mut self, _version: u64, anchor: usize) -> bool {
        if self.current_request_id == 0 || self.sync_text.is_empty() {
            return false;
        }
        let lang_name = lang_name_for_ext_and_text(&self.sync_ext, &self.sync_text);
        if !should_skip_full_highlight(lang_name, &self.sync_text) {
            return false;
        }
        let anchor = anchor.min(self.sync_text.len().saturating_sub(1));
        if self.is_byte_highlighted(anchor) {
            return false;
        }
        if self
            .pending_priority_anchor
            .is_some_and(|pending| pending.abs_diff(anchor) < PRIORITY_HIGHLIGHT_HEAD_MIN_BYTES / 2)
        {
            return false;
        }
        if self
            .tx
            .send(HighlighterMessage::Priority {
                request_id: self.current_request_id,
                priority_anchor: anchor,
            })
            .is_err()
        {
            self.pending_priority_anchor = None;
            return false;
        }
        self.pending_priority_anchor = Some(anchor);
        true
    }

    pub fn has_pending_priority_highlight(&self) -> bool {
        self.pending_priority_anchor.is_some()
    }

    /// The worker dropped its replica (see `HighlighterWorkerControl::replica_desynced`):
    /// re-seed it from `sync_text`, which `apply_document_edits` checked against the editor.
    fn resync_worker_if_desynced(&mut self) {
        if self
            ._worker
            .control
            .replica_desynced
            .swap(false, Ordering::AcqRel)
        {
            let anchor = self.pending_priority_anchor.unwrap_or(0);
            self.reset(self.sync_version, self.sync_text.clone(), self.sync_ext.clone(), anchor);
        }
    }

    pub fn poll(&mut self, current_editor_version: u64) -> bool {
        self.resync_worker_if_desynced();
        let mut updated = false;
        loop {
            match self.rx.try_recv() {
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
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.pending_priority_anchor = None;
                    if !self.is_complete {
                        self.spans.clear();
                        self.completions.clear();
                        self.foldable_ranges.clear();
                        self.syntax_errors.clear();
                        self.sync_tree = None;
                        self.current_version = current_editor_version;
                        self.is_complete = true;
                        updated = true;
                    }
                    break;
                }
            }
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
        // The worker tree belongs to the worker replica of `ver`; it may be paired with
        // `sync_text` only when that holds the same version. The length check stays as a
        // last line of defence against a tree of another text.
        self.sync_tree = tree.filter(|tree| {
            let matches = ver == self.sync_version && tree_matches_text_len(tree, &self.sync_text);
            if !matches && !cfg!(test) {
                eprintln!(
                    "[HL TRACE runtime:poll_tree_mismatch] req={} ver={} tree_bytes={} sync_bytes={}",
                    request_id,
                    ver,
                    tree.root_node().end_byte(),
                    self.sync_text.len(),
                );
            }
            matches
        });
        self.is_complete = is_complete;
        if self
            .pending_priority_anchor
            .is_some_and(|anchor| self.is_byte_highlighted(anchor))
        {
            self.pending_priority_anchor = None;
        }
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

    /// Shifts the displayed spans for an edit the editor just made (with a predicted color
    /// for the inserted text). The text replicas are not touched here: they follow the
    /// editor's `SyncEdit`s in `apply_document_edits` only.
    pub fn shift_insert(&mut self, offset: usize, len: usize, text_opt: Option<&str>) {
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

    /// Span-only counterpart of `shift_insert`.
    pub fn shift_delete(&mut self, offset: usize, len: usize) {
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
        // An old tree of another text version would be reused as-is and keep its old length;
        // parse from scratch instead (see `tree_matches_text_len`).
        let old_tree = self
            .sync_tree
            .as_ref()
            .filter(|tree| tree_matches_text_len(tree, text));
        let parsed_tree = self
            .sync_parser
            .parse_with_options(
                &mut |i, _| (i < len).then(|| &bytes[i..]).unwrap_or_default(),
                old_tree,
                Some(options),
            )
            .filter(|tree| tree_matches_text_len(tree, text));
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

/// Applies one editor `SyncEdit` to a document replica and keeps its tree in step. Both
/// replicas (`Highlighter::sync_text` and the worker copy) go through this one function, in
/// the order the editor produced the edits, so any valid ordered list (including many
/// back-to-front edits of one multi-cursor action) yields the same text in both. Returns
/// `false` and leaves the replica untouched when the edit does not fit it: the replica is
/// out of sync with the editor and has to be reset, never patched by a guess.
pub(super) fn apply_sync_edit_to_replica(
    text: &mut String,
    tree: Option<&mut tree_sitter::Tree>,
    edit: &SyncEdit,
) -> bool {
    let (start_byte, old_end_byte, inserted) = match edit {
        SyncEdit::Insert { offset, text } => (*offset, *offset, text.as_str()),
        SyncEdit::Delete { offset, len } => (*offset, offset.saturating_add(*len), ""),
    };
    if old_end_byte > text.len()
        || !text.is_char_boundary(start_byte)
        || !text.is_char_boundary(old_end_byte)
    {
        return false;
    }
    let start_position = get_point(text, start_byte);
    let old_end_position = get_point(text, old_end_byte);
    text.replace_range(start_byte..old_end_byte, inserted);
    if let Some(tree) = tree {
        let new_end_byte = start_byte + inserted.len();
        tree.edit(&tree_sitter::InputEdit {
            start_byte,
            old_end_byte,
            new_end_byte,
            start_position,
            old_end_position,
            new_end_position: get_point(text, new_end_byte),
        });
    }
    true
}

#[cfg(test)]
#[path = "highlighter_runtime_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "highlighter_replica_sync_tests.rs"]
mod replica_sync_tests;
