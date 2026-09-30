const NO_VERSION_IN_FLIGHT: u64 = u64::MAX;

impl HighlighterWorkerControl {
    /// Worker side: a job for `version` starts now. The runtime uses this to tell a job
    /// that already runs from one still queued behind a stale job.
    fn begin_job(&self, version: u64) -> InFlightGuard<'_> {
        self.in_flight_version.store(version, Ordering::Release);
        InFlightGuard(self)
    }

    fn is_in_flight(&self, version: u64) -> bool {
        self.in_flight_version.load(Ordering::Acquire) == version
    }
}

/// Clears `HighlighterWorkerControl::in_flight_version` on every exit of a worker job
/// (`continue`, `return`, end of the iteration).
struct InFlightGuard<'a>(&'a HighlighterWorkerControl);

impl Drop for InFlightGuard<'_> {
    fn drop(&mut self) {
        self.0
            .in_flight_version
            .store(NO_VERSION_IN_FLIGHT, Ordering::Release);
    }
}

type QueryCacheKey = (&'static str, &'static str);
/// One key's compile-once cell; `None` caches a query that failed to compile.
type QuerySlot = Arc<OnceLock<Option<Arc<tree_sitter::Query>>>>;

/// Compiled tree-sitter queries shared by the worker thread, the synchronous fallback on
/// the main thread and prewarm threads. Owned by `Highlighter` behind an `Arc`.
/// Every key is compiled exactly once: a thread that needs a query another thread is
/// compiling blocks on that key's `OnceLock` until it is ready. `QueryCursor`s stay per
/// call; `tree_sitter::Query` itself is `Send + Sync`.
pub(crate) struct QueryCache {
    entries: Mutex<HashMap<QueryCacheKey, QuerySlot>>,
    compile_count: AtomicUsize,
}

impl QueryCache {
    pub(crate) fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            compile_count: AtomicUsize::new(0),
        }
    }

    fn slot(&self, key: QueryCacheKey) -> QuerySlot {
        // The lock guards only the map; compiling happens outside it, on the key's slot.
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        Arc::clone(entries.entry(key).or_default())
    }

    /// The compiled query for `(lang_name, q_str)`, compiling it on first use.
    /// `None` when the query does not compile (that result is cached too).
    pub(crate) fn get_or_compile(
        &self,
        lang_name: &'static str,
        q_str: &'static str,
        lang: &tree_sitter::Language,
    ) -> Option<Arc<tree_sitter::Query>> {
        let slot = self.slot((lang_name, q_str));
        slot.get_or_init(|| {
            self.compile_count.fetch_add(1, Ordering::AcqRel);
            tree_sitter::Query::new(lang, q_str).ok().map(Arc::new)
        })
        .clone()
    }

    /// True once `(lang_name, q_str)` finished compiling (successfully or not).
    pub(crate) fn is_compiled(&self, lang_name: &'static str, q_str: &'static str) -> bool {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries
            .get(&(lang_name, q_str))
            .is_some_and(|slot| slot.get().is_some())
    }

    /// Number of compilations started so far.
    pub(crate) fn compile_count(&self) -> usize {
        self.compile_count.load(Ordering::Acquire)
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.lock().unwrap_or_else(|e| e.into_inner()).len()
    }
}

/// Every query a full highlight of `lang_name` runs: highlights, params, folding,
/// injections.
fn all_query_sources(
    lang_name: &'static str,
    highlight_queries: &[&'static str],
) -> Vec<&'static str> {
    let mut all: Vec<&'static str> = highlight_queries.to_vec();
    all.extend(get_params_query(lang_name));
    all.extend(get_folding_query(lang_name));
    all.extend(get_injection_query(lang_name));
    all
}

const HIGHLIGHT_TRACE_MIN_BYTES: usize = TREE_SITTER_HIGHLIGHT_MAX_BYTES;
const HIGHLIGHT_TRACE_SLOW_MS: f64 = 8.0;

fn highlight_trace_elapsed_ms(start: std::time::Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn highlight_trace_line_count(text: &str) -> usize {
    text.as_bytes().iter().filter(|&&b| b == b'\n').count() + 1
}

fn highlight_trace_should_log(text_len: usize, priority: bool, elapsed_ms: f64) -> bool {
    !cfg!(test)
        && (text_len >= HIGHLIGHT_TRACE_MIN_BYTES
            || priority
            || elapsed_ms >= HIGHLIGHT_TRACE_SLOW_MS)
}

fn resolve_injected_capture_color(
    parent_lang_name: &str,
    injected_lang_name: &str,
    name: &str,
    node: tree_sitter::Node<'_>,
    node_text: &str,
) -> [f32; 4] {
    if parent_lang_name == "md" && injected_lang_name == "bash" {
        if name == "command_word"
            || (name == "any_word"
                && node.parent().is_some_and(|parent| parent.kind() == "command_name"))
        {
            return DRACULA_GREEN;
        }
        if name == "any_word" {
            return if node_text.starts_with('-') && node_text.len() > 1 {
                DRACULA_PURPLE
            } else {
                DRACULA_FG
            };
        }
    }
    let color = capture_color_override(injected_lang_name, name, node)
        .unwrap_or_else(|| resolve_color(name, node_text, node.start_byte(), &[]));
    if parent_lang_name == "md" && color == DRACULA_YELLOW {
        MARKDOWN_GOLD
    } else {
        color
    }
}

impl Highlighter {
    /// Worker results wake the UI through `waker` from now on. The first binding wins;
    /// rebinding the same owner's waker is a cheap no-op.
    pub(crate) fn bind_ui_waker(&self, waker: &crate::ui_waker::UiWaker) {
        let control = &self._worker.control;
        if control.ui_waker.get().is_none() {
            let _ = control.ui_waker.set(waker.clone());
        }
    }

    /// True while the worker is highlighting exactly `version` (the job has started; it is
    /// not merely queued, possibly behind a stale job).
    pub(crate) fn is_worker_processing(&self, version: u64) -> bool {
        self._worker.control.is_in_flight(version)
    }

    /// Compiles the queries for files with extension `ext` into the shared cache on a
    /// short-lived thread, so the first highlight of such a file finds them ready.
    /// Never blocks; a no-op for unknown extensions and when everything is compiled already.
    pub fn prewarm_query(&self, ext: &str) {
        let lang_name = tree_sitter_lang_name_for_ext(ext);
        let Some((lang, queries)) = get_ts_config(lang_name) else {
            return;
        };
        let sources = all_query_sources(lang_name, &queries);
        if sources
            .iter()
            .all(|q_str| self.query_cache.is_compiled(lang_name, q_str))
        {
            return;
        }
        let cache = Arc::clone(&self.query_cache);
        let spawned = thread::Builder::new()
            .name("rriter-query-prewarm".to_string())
            .spawn(move || {
                for q_str in sources {
                    cache.get_or_compile(lang_name, q_str, &lang);
                }
            });
        if let Err(error) = spawned {
            eprintln!("failed to spawn RRiter query prewarm thread: {error}");
        }
    }

    pub fn new() -> Self {
        let (tx_in, rx_in) = mpsc::channel::<HighlighterMessage>();
        let (tx_out, rx_out) = mpsc::channel::<(
            u64,
            u64,
            Vec<ColorSpan>,
            Vec<CompletionItem>,
            Vec<(usize, usize, bool, bool)>,
            Vec<(usize, usize)>,
            Option<tree_sitter::Tree>,
            bool,
        )>();

        let worker_control = Arc::new(HighlighterWorkerControl::new());
        let worker_control_for_thread = Arc::clone(&worker_control);
        let query_cache = Arc::new(QueryCache::new());
        let query_cache_for_thread = Arc::clone(&query_cache);
        let spawn_result = thread::Builder::new()
            .name("rriter-highlighter".to_string())
            .spawn(move || {
                #[cfg(test)]
                let _active_worker_guard = ActiveHighlighterWorkerGuard::new();
                let mut parser = tree_sitter::Parser::new();
                let query_cache = query_cache_for_thread;
                let mut byte_colors_buf = Vec::new();
                let mut last_full_spans: Vec<ColorSpan> = Vec::new();

                let mut replica_text = String::new();
                // Document version `replica_text` holds; `None` once it lost sync.
                let mut replica_version: Option<u64> = Some(0);
                let mut current_tree: Option<tree_sitter::Tree> = None;
                let mut current_ext = String::new();

                while !worker_control_for_thread.is_cancelled() {
                    let Ok(msg) = rx_in.recv() else {
                        break;
                    };
                let worker_start = std::time::Instant::now();
                let mut msgs = vec![msg];
                while let Ok(m) = rx_in.try_recv() {
                    msgs.push(m);
                }
                let batch_count = msgs.len();

                let mut final_request_id = 0;
                let mut do_highlight = false;
                let mut final_edit_start_byte: Option<usize> = None;
                let mut final_edit_end_byte: Option<usize> = None;
                let mut final_invalidate_start_byte: Option<usize> = None;
                let mut final_invalidate_end_byte: Option<usize> = None;
                let mut final_priority_anchor = 0usize;
                let mut reset_msg_count = 0usize;
                let mut edit_msg_count = 0usize;
                let mut priority_msg_count = 0usize;
                let mut edit_op_count = 0usize;

                for m in msgs {
                    match m {
                        HighlighterMessage::Shutdown => return,
                        HighlighterMessage::Restore {
                            version,
                            text,
                            ext,
                            spans,
                        } => {
                            replica_text = text;
                            replica_version = Some(version);
                            current_ext = ext;
                            current_tree = None;
                            last_full_spans = spans;
                        }
                        HighlighterMessage::Reset {
                            request_id,
                            version,
                            text,
                            ext,
                            priority_anchor,
                        } => {
                            reset_msg_count += 1;
                            final_request_id = request_id;
                            final_priority_anchor = priority_anchor;
                            replica_text = text;
                            replica_version = Some(version);
                            current_ext = ext;
                            current_tree = None;
                            do_highlight = true;
                            last_full_spans.clear();
                        }
                        HighlighterMessage::Edits {
                            request_id,
                            base_version,
                            version,
                            edits,
                            edit_start_byte,
                            edit_end_byte,
                            invalidate_start_byte,
                            invalidate_end_byte,
                        } => {
                            edit_msg_count += 1;
                            edit_op_count += edits.len();
                            final_request_id = request_id;
                            final_edit_start_byte = edit_start_byte;
                            final_edit_end_byte = edit_end_byte;
                            final_invalidate_start_byte = invalidate_start_byte;
                            final_invalidate_end_byte = invalidate_end_byte;
                            // Edits made against another version would land on the wrong
                            // bytes: drop the replica until the runtime resets it.
                            if replica_version != Some(base_version) {
                                replica_version = None;
                            }
                            for edit in &edits {
                                if replica_version.is_none() {
                                    break;
                                }
                                match edit {
                                    SyncEdit::Insert { offset, text } => {
                                        let (offset, len) = (*offset, text.len());
                                        for span in &mut last_full_spans {
                                            if span.start >= offset {
                                                span.start += len;
                                                span.end += len;
                                            } else if span.end > offset {
                                                span.end += len;
                                            }
                                        }
                                    }
                                    SyncEdit::Delete { offset, len } => {
                                        let (offset, len) = (*offset, *len);
                                        for span in &mut last_full_spans {
                                            if span.start >= offset + len {
                                                span.start -= len;
                                                span.end -= len;
                                            } else if span.start >= offset {
                                                span.start = offset;
                                                span.end = span.end.saturating_sub(len).max(offset);
                                            } else if span.end > offset {
                                                span.end = span.end.saturating_sub(len).max(offset);
                                            }
                                        }
                                        last_full_spans.retain(|s| s.start < s.end);
                                    }
                                }
                                if !apply_sync_edit_to_replica(
                                    &mut replica_text,
                                    current_tree.as_mut(),
                                    edit,
                                ) {
                                    replica_version = None;
                                }
                            }
                            if replica_version.is_some() {
                                replica_version = Some(version);
                            }
                            do_highlight = true;
                        }
                        HighlighterMessage::Priority {
                            request_id,
                            priority_anchor,
                        } => {
                            priority_msg_count += 1;
                            final_request_id = request_id;
                            final_priority_anchor = priority_anchor;
                            do_highlight = true;
                        }
                    }
                }

                if worker_control_for_thread.is_cancelled() {
                    break;
                }

                let Some(highlighted_version) = replica_version else {
                    // Never highlight (or hand out a tree of) a text the editor does not hold.
                    current_tree = None;
                    worker_control_for_thread
                        .replica_desynced
                        .store(true, Ordering::Release);
                    worker_control_for_thread.wake_ui();
                    continue;
                };
                if !do_highlight {
                    continue;
                }
                // Replies carry the version of the text they were built from, whatever
                // version the triggering message named.
                let final_version = highlighted_version;
                let _in_flight = worker_control_for_thread.begin_job(final_version);

                let text = &replica_text;
                let ext = &current_ext;

                let is_log = ext == "log";

                let lang_name = lang_name_for_ext_and_text(ext, text);
                let should_prioritize_front = !is_log
                    && final_edit_start_byte.is_none()
                    && should_prioritize_front_highlight(ext, text);
                let skip_full_highlight = !is_log && should_skip_full_highlight(lang_name, text);
                let queue_ms = highlight_trace_elapsed_ms(worker_start);
                let trace_large = highlight_trace_should_log(
                    text.len(),
                    should_prioritize_front || skip_full_highlight,
                    0.0,
                );
                if trace_large {
                    let mode = if reset_msg_count > 0 {
                        "reset"
                    } else if edit_msg_count > 0 {
                        "edits"
                    } else if priority_msg_count > 0 {
                        "priority"
                    } else {
                        "unknown"
                    };
                    eprintln!(
                        "[HL TRACE worker:start] req={} ver={} mode={} batch={} resets={} edit_msgs={} edit_ops={} bytes={} lines={} ext={} lang={} priority={} skip_full={} anchor={} edit_range={:?} invalidate={:?} queued_ms={:.2}",
                        final_request_id,
                        final_version,
                        mode,
                        batch_count,
                        reset_msg_count,
                        edit_msg_count,
                        edit_op_count,
                        text.len(),
                        highlight_trace_line_count(text),
                        ext,
                        lang_name,
                        should_prioritize_front,
                        skip_full_highlight,
                        final_priority_anchor,
                        final_edit_start_byte.zip(final_edit_end_byte),
                        final_invalidate_start_byte.zip(final_invalidate_end_byte),
                        queue_ms,
                    );
                }

                let mut spans = Vec::new();
                let mut completions_map: HashMap<(String, usize, usize), SymbolKind> =
                    HashMap::new();
                let mut foldable_ranges = Vec::new();
                let mut error_ranges = Vec::new();
                let mut keyword_ms = 0.0;
                let mut priority_ms = 0.0;
                let mut priority_range_bytes = 0usize;
                let mut priority_span_count = 0usize;
                let mut priority_sent = false;
                let mut parse_ms = 0.0;
                let mut fold_ms = 0.0;
                let mut import_fold_ms = 0.0;
                let mut completion_walk_ms = 0.0;
                let mut query_ms = 0.0;
                let mut injection_ms = 0.0;

                if !is_log {
                    let keyword_start = std::time::Instant::now();
                    let ts_config = get_ts_config(lang_name);

                    if let Some((_, queries)) = &ts_config {
                        for q_str in queries {
                            let mut start_idx = None;
                            for (i, b) in q_str.bytes().enumerate() {
                                if b == b'"' {
                                    if let Some(start) = start_idx {
                                        let word = &q_str[start..i];
                                        if word.len() > 1
                                            && word
                                                .bytes()
                                                .all(|c| c.is_ascii_alphabetic() || c == b'_')
                                        {
                                            completions_map.insert(
                                                (word.to_string(), 0, usize::MAX),
                                                SymbolKind::Keyword,
                                            );
                                        }
                                        start_idx = None;
                                    } else {
                                        start_idx = Some(i + 1);
                                    }
                                }
                            }
                        }
                    }
                    keyword_ms = highlight_trace_elapsed_ms(keyword_start);

                    if let Some((lang, queries)) = ts_config {
                        if parser.set_language(&lang).is_ok() {
                            if should_prioritize_front || skip_full_highlight {
                                let priority_start = std::time::Instant::now();
                                let priority_anchor =
                                    final_edit_start_byte.unwrap_or(final_priority_anchor);
                                let priority_range =
                                    priority_highlight_range(lang_name, text, priority_anchor);
                                priority_range_bytes =
                                    priority_range.end.saturating_sub(priority_range.start);
                                let mut priority_spans = priority_highlight_spans_from_slice(
                                    &mut parser,
                                    &lang,
                                    lang_name,
                                    &queries,
                                    text,
                                    priority_range.clone(),
                                    &query_cache,
                                    &mut byte_colors_buf,
                                    &worker_control_for_thread,
                                );
                                if worker_control_for_thread.is_cancelled() {
                                    return;
                                }
                                priority_ms = highlight_trace_elapsed_ms(priority_start);
                                priority_span_count = priority_spans.len();
                                if priority_spans.is_empty()
                                    && priority_range.start < priority_range.end
                                {
                                    priority_spans.push(ColorSpan {
                                        start: priority_range.start,
                                        end: priority_range.end,
                                        color: DRACULA_FG,
                                    });
                                }
                                if !priority_spans.is_empty() {
                                    let mut priority_foldable_ranges = Vec::new();
                                    push_language_import_foldable_ranges(
                                        lang_name,
                                        text,
                                        &mut priority_foldable_ranges,
                                    );
                                    let priority_completions = if skip_full_highlight {
                                        inject_builtin_completions(
                                            lang_name,
                                            &mut completions_map,
                                        );
                                        completion_items_from_map(completions_map.clone())
                                    } else {
                                        Vec::new()
                                    };
                                    let priority_result_spans = if skip_full_highlight {
                                        current_tree = None;
                                        merge_partial_highlight_spans(
                                            last_full_spans.clone(),
                                            priority_spans,
                                            priority_range.clone(),
                                        )
                                    } else {
                                        priority_spans
                                    };
                                    priority_span_count = priority_result_spans.len();
                                    last_full_spans = priority_result_spans.clone();
                                    let _ = tx_out.send((
                                        final_request_id,
                                        final_version,
                                        priority_result_spans,
                                        priority_completions,
                                        priority_foldable_ranges,
                                        Vec::new(),
                                        if skip_full_highlight {
                                            None
                                        } else {
                                            current_tree.clone()
                                        },
                                        skip_full_highlight,
                                    ));
                                    worker_control_for_thread.wake_ui();
                                    priority_sent = true;
                                }
                                if skip_full_highlight {
                                    let total_ms = highlight_trace_elapsed_ms(worker_start);
                                    if trace_large
                                        || highlight_trace_should_log(text.len(), true, total_ms)
                                    {
                                        eprintln!(
                                            "[HL TRACE worker:skip_full] req={} ver={} bytes={} lines={} lang={} priority_sent={} range={} spans={} byte_buf_cap={} total_ms={:.2} queued_ms={:.2} keyword_ms={:.2} priority_ms={:.2}",
                                            final_request_id,
                                            final_version,
                                            text.len(),
                                            highlight_trace_line_count(text),
                                            lang_name,
                                            priority_sent,
                                            priority_range_bytes,
                                            last_full_spans.len(),
                                            byte_colors_buf.capacity(),
                                            total_ms,
                                            queue_ms,
                                            keyword_ms,
                                            priority_ms,
                                        );
                                    }
                                    continue;
                                }
                            }

                            let parse_start = std::time::Instant::now();
                            let parsed_tree = parse_with_worker_control(
                                &mut parser,
                                &replica_text,
                                current_tree.as_ref(),
                                &worker_control_for_thread,
                            );
                            parse_ms = highlight_trace_elapsed_ms(parse_start);
                            if worker_control_for_thread.is_cancelled() {
                                return;
                            }
                            current_tree = parsed_tree.clone();

                            if let Some(tree) = parsed_tree {
                                let fold_start = std::time::Instant::now();
                                if let Some(fold_query_str) = get_folding_query(lang_name) {
                                    if let Some(fold_query) =
                                        query_cache.get_or_compile(lang_name, fold_query_str, &lang)
                                    {
                                        let mut cursor = tree_sitter::QueryCursor::new();
                                        let mut matches = cursor.matches(
                                            &fold_query,
                                            tree.root_node(),
                                            text.as_bytes(),
                                        );
                                        while let Some(m) = matches.next() {
                                            for cap in m.captures {
                                                let name =
                                                    fold_query.capture_names()[cap.index as usize];
                                                let is_autofold = name == "autofold";
                                                let node = cap.node;
                                                let mut start_byte = node.start_byte();

                                                if node.kind() == "block" {
                                                    while start_byte > 0 {
                                                        start_byte -= 1;
                                                        let Some(&b) = text.as_bytes().get(start_byte)
                                                        else {
                                                            break;
                                                        };
                                                        if b != b' '
                                                            && b != b'\t'
                                                            && b != b'\n'
                                                            && b != b'\r'
                                                        {
                                                            break;
                                                        }
                                                    }
                                                }

                                                let is_sticky = name == "sticky";
                                                if node.end_byte() > start_byte {
                                                    if is_sticky
                                                        || node.end_position().row
                                                            > node.start_position().row
                                                    {
                                                        foldable_ranges.push((
                                                            start_byte,
                                                            node.end_byte(),
                                                            is_autofold,
                                                            is_sticky,
                                                        ));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                fold_ms = highlight_trace_elapsed_ms(fold_start);

                                let import_fold_start = std::time::Instant::now();
                                push_language_import_foldable_ranges(
                                    lang_name,
                                    text,
                                    &mut foldable_ranges,
                                );
                                import_fold_ms = highlight_trace_elapsed_ms(import_fold_start);

                                let completion_walk_start = std::time::Instant::now();
                                let is_same_node = |n1: Option<tree_sitter::Node>,
                                                    n2: tree_sitter::Node|
                                 -> bool {
                                    if let Some(n1) = n1 {
                                        n1.start_byte() == n2.start_byte()
                                            && n1.end_byte() == n2.end_byte()
                                    } else {
                                        false
                                    }
                                };
                                let contains_node = |n1: Option<tree_sitter::Node>,
                                                     n2: tree_sitter::Node|
                                 -> bool {
                                    n1.is_some_and(|n1| {
                                        n2.start_byte() >= n1.start_byte()
                                            && n2.end_byte() <= n1.end_byte()
                                    })
                                };

                                let mut c_cursor = tree.walk();
                                let mut visiting = true;
                                while visiting {
                                    let node = c_cursor.node();
                                    let kind = node.kind();

                                    if node.is_error() {
                                        error_ranges.push((node.start_byte(), node.end_byte()));
                                    }

                                    if kind.contains("identifier")
                                        || kind == "word"
                                        || kind == "property_identifier"
                                        || kind == "type_identifier"
                                    {
                                        if let Ok(s) = std::str::from_utf8(
                                            text.as_bytes()
                                                .get(node.start_byte()..node.end_byte())
                                                .unwrap_or_default(),
                                        ) {
                                            if s.len() > 2 && !s.contains('\n') && !s.contains(' ')
                                            {
                                                let mut sym_kind = SymbolKind::Variable;
                                                let mut scope_start = 0;
                                                let mut scope_end = usize::MAX;
                                                let mut skip = false;
                                                let mut scope_found = false;
                                                let mut in_type_annotation = false;

                                                if let Some(p) = node.parent() {
                                                    let p_kind = p.kind();

                                                    if p_kind == "keyword_argument"
                                                        && is_same_node(
                                                            p.child_by_field_name("name"),
                                                            node,
                                                        )
                                                    {
                                                        skip = true;
                                                    } else if p_kind == "attribute"
                                                        && is_same_node(
                                                            p.child_by_field_name("attribute"),
                                                            node,
                                                        )
                                                    {
                                                        skip = true;
                                                    } else if p_kind == "member_expression"
                                                        && is_same_node(
                                                            p.child_by_field_name("property"),
                                                            node,
                                                        )
                                                    {
                                                        skip = true;
                                                    } else if p_kind == "field_expression"
                                                        && is_same_node(
                                                            p.child_by_field_name("field"),
                                                            node,
                                                        )
                                                    {
                                                        skip = true;
                                                    } else if kind == "property_identifier" {
                                                        skip = true;
                                                    } else if (p_kind.contains("function")
                                                        || p_kind.contains("method"))
                                                        && is_same_node(
                                                            p.child_by_field_name("name"),
                                                            node,
                                                        )
                                                    {
                                                        sym_kind = SymbolKind::Function;
                                                    } else if (p_kind.contains("class")
                                                        || p_kind.contains("struct")
                                                        || p_kind.contains("enum")
                                                        || p_kind.contains("trait"))
                                                        && is_same_node(
                                                            p.child_by_field_name("name"),
                                                            node,
                                                        )
                                                    {
                                                        sym_kind = SymbolKind::Class;
                                                    } else if p_kind.contains("parameter")
                                                        || p_kind.contains("argument")
                                                    {
                                                        sym_kind = SymbolKind::Parameter;
                                                    }

                                                    let mut curr = node;
                                                    let mut curr_parent = Some(p);
                                                    while let Some(cp) = curr_parent {
                                                        let cp_kind = cp.kind();

                                                        if lang_name == "py"
                                                            && (cp_kind == "type"
                                                                || contains_node(
                                                                    cp.child_by_field_name("type"),
                                                                    curr,
                                                                )
                                                                || contains_node(
                                                                    cp.child_by_field_name(
                                                                        "return_type",
                                                                    ),
                                                                    curr,
                                                                ))
                                                        {
                                                            in_type_annotation = true;
                                                        }

                                                        if cp_kind == "import_from_statement"
                                                            || cp_kind == "import_statement"
                                                        {
                                                            if let Some(mod_name) = cp
                                                                .child_by_field_name("module_name")
                                                            {
                                                                if curr.start_byte()
                                                                    >= mod_name.start_byte()
                                                                    && curr.end_byte()
                                                                        <= mod_name.end_byte()
                                                                {
                                                                    skip = true;
                                                                }
                                                            }
                                                        }
                                                        if cp_kind == "aliased_import" {
                                                            if let Some(name_node) =
                                                                cp.child_by_field_name("name")
                                                            {
                                                                if curr.start_byte()
                                                                    >= name_node.start_byte()
                                                                    && curr.end_byte()
                                                                        <= name_node.end_byte()
                                                                {
                                                                    skip = true;
                                                                }
                                                            }
                                                        }

                                                        if !scope_found
                                                            && (cp_kind.contains("function")
                                                                || cp_kind.contains("method")
                                                                || cp_kind.contains("class")
                                                                || cp_kind.contains("block")
                                                                || cp_kind == "module"
                                                                || cp_kind == "source_file")
                                                        {
                                                            scope_start = cp.start_byte();
                                                            scope_end = cp.end_byte();
                                                            scope_found = true;
                                                        }
                                                        curr = cp;
                                                        curr_parent = cp.parent();
                                                    }
                                                    if in_type_annotation
                                                        && sym_kind != SymbolKind::Parameter
                                                    {
                                                        sym_kind = SymbolKind::Class;
                                                    }
                                                }

                                                if !skip {
                                                    if let Some(p) = node.parent() {
                                                        let p_kind = p.kind();
                                                        if p_kind.contains("import")
                                                            || p_kind == "dotted_name"
                                                            || p_kind == "aliased_import"
                                                        {
                                                            sym_kind = SymbolKind::Unknown;
                                                        }
                                                    }

                                                    let actual_scope_start = match sym_kind {
                                                        SymbolKind::Variable
                                                        | SymbolKind::Parameter
                                                        | SymbolKind::Argument => {
                                                            node.start_byte()
                                                        }
                                                        _ => scope_start,
                                                    };

                                                    completions_map.insert(
                                                        (
                                                            s.to_string(),
                                                            actual_scope_start,
                                                            scope_end,
                                                        ),
                                                        sym_kind,
                                                    );
                                                }
                                            }
                                        }
                                    }

                                    if c_cursor.goto_first_child() {
                                        continue;
                                    }
                                    while !c_cursor.goto_next_sibling() {
                                        if !c_cursor.goto_parent() {
                                            visiting = false;
                                            break;
                                        }
                                    }
                                }
                                completion_walk_ms =
                                    highlight_trace_elapsed_ms(completion_walk_start);

                                let byte_range = if let (Some(sb), Some(eb)) =
                                    (final_edit_start_byte, final_edit_end_byte)
                                {
                                    Some(sb.saturating_sub(1000)..(eb + 1000).min(text.len()))
                                } else {
                                    None
                                };
                                let query_start = std::time::Instant::now();
                                collect_query_highlight_spans(
                                    &lang,
                                    lang_name,
                                    &queries,
                                    &tree,
                                    &text,
                                    &query_cache,
                                    byte_range,
                                    &mut spans,
                                );
                                query_ms = highlight_trace_elapsed_ms(query_start);

                                let injection_start = std::time::Instant::now();
                                // ---------------------------------------------------------
                                // Обработка языковых инъекций (Language Injections)
                                // ---------------------------------------------------------
                                let mut injected_regions: HashMap<String, Vec<tree_sitter::Range>> =
                                    HashMap::new();
                                if let Some(inj_query_str) = get_injection_query(lang_name) {
                                    if let Some(inj_query) =
                                        query_cache.get_or_compile(lang_name, inj_query_str, &lang)
                                    {
                                        let mut cursor = tree_sitter::QueryCursor::new();
                                        if let (Some(sb), Some(eb)) =
                                            (final_edit_start_byte, final_edit_end_byte)
                                        {
                                            // Expand bounds to ensure we capture whole nodes/statements
                                            let exp_sb = sb.saturating_sub(1000);
                                            let exp_eb = (eb + 1000).min(text.len());
                                            cursor.set_byte_range(exp_sb..exp_eb);
                                        }
                                        let mut matches = cursor.matches(
                                            &inj_query,
                                            tree.root_node(),
                                            text.as_bytes(),
                                        );

                                        let lang_cap_idx =
                                            inj_query.capture_index_for_name("injection.language");
                                        let content_cap_idx =
                                            inj_query.capture_index_for_name("injection.content");

                                        while let Some(m) = matches.next() {
                                            let mut inj_lang = String::new();
                                            let mut content_node = None;

                                            for prop in inj_query.property_settings(m.pattern_index)
                                            {
                                                if prop.key.as_ref() == "injection.language" {
                                                    if let Some(v) = &prop.value {
                                                        inj_lang = v.to_string();
                                                    }
                                                }
                                            }

                                            for cap in m.captures {
                                                if Some(cap.index) == lang_cap_idx {
                                                    if let Ok(s) = std::str::from_utf8(
                                                        text.as_bytes()
                                                            .get(
                                                                cap.node.start_byte()
                                                                    ..cap.node.end_byte(),
                                                            )
                                                            .unwrap_or_default(),
                                                    ) {
                                                        inj_lang = s.to_string();
                                                    }
                                                }
                                                if Some(cap.index) == content_cap_idx {
                                                    content_node = Some(cap.node);
                                                }
                                            }

                                            if !inj_lang.is_empty() {
                                                if let Some(node) = content_node {
                                                    let mut end_byte = node.end_byte();
                                                    let mut end_point = node.end_position();
                                                    if inj_lang.trim() == "markdown_inline" {
                                                        let bytes = text.as_bytes();
                                                        if bytes.get(end_byte) == Some(&b'\r')
                                                            && bytes.get(end_byte + 1)
                                                                == Some(&b'\n')
                                                        {
                                                            end_byte += 2;
                                                            end_point = tree_sitter::Point::new(
                                                                end_point.row + 1,
                                                                0,
                                                            );
                                                        } else if bytes.get(end_byte)
                                                            == Some(&b'\n')
                                                        {
                                                            end_byte += 1;
                                                            end_point = tree_sitter::Point::new(
                                                                end_point.row + 1,
                                                                0,
                                                            );
                                                        }
                                                    }
                                                    let range = tree_sitter::Range {
                                                        start_byte: node.start_byte(),
                                                        end_byte,
                                                        start_point: node.start_position(),
                                                        end_point,
                                                    };
                                                    injected_regions
                                                        .entry(inj_lang)
                                                        .or_default()
                                                        .push(range);
                                                }
                                            }
                                        }
                                    }
                                }

                                for (inj_lang_name, ranges) in injected_regions {
                                    let Some(mapped_lang) =
                                        normalize_injection_language(&inj_lang_name)
                                    else {
                                        continue;
                                    };

                                    if let Some((inj_lang, inj_queries)) =
                                        get_ts_config(mapped_lang)
                                    {
                                        let mut inj_parser = tree_sitter::Parser::new();
                                        if inj_parser.set_language(&inj_lang).is_ok() {
                                            if inj_parser.set_included_ranges(&ranges).is_ok() {
                                                if let Some(inj_tree) = inj_parser.parse(text, None)
                                                {
                                                    for q_str in inj_queries {
                                                        if let Some(query) = query_cache
                                                            .get_or_compile(
                                                                mapped_lang,
                                                                q_str,
                                                                &inj_lang,
                                                            )
                                                        {
                                                            let mut cursor =
                                                                tree_sitter::QueryCursor::new();
                                                            if let (Some(sb), Some(eb)) = (
                                                                final_edit_start_byte,
                                                                final_edit_end_byte,
                                                            ) {
                                                                let exp_sb =
                                                                    sb.saturating_sub(1000);
                                                                let exp_eb =
                                                                    (eb + 1000).min(text.len());
                                                                cursor
                                                                    .set_byte_range(exp_sb..exp_eb);
                                                            }
                                                            let mut matches = cursor.matches(
                                                                &query,
                                                                inj_tree.root_node(),
                                                                text.as_bytes(),
                                                            );

                                                            while let Some(m) = matches.next() {
                                                                for cap in m.captures {
                                                                    let name = query
                                                                        .capture_names()
                                                                        [cap.index as usize];
                                                                    let node_text =
                                                                        std::str::from_utf8(
                                                                            text.as_bytes()
                                                                                .get(
                                                                                    cap.node
                                                                                        .start_byte()
                                                                                        ..cap
                                                                                            .node
                                                                                            .end_byte(),
                                                                                )
                                                                                .unwrap_or_default(),
                                                                        )
                                                                        .unwrap_or("");

                                                                    let color = resolve_injected_capture_color(
                                                                        lang_name,
                                                                        mapped_lang,
                                                                        name,
                                                                        cap.node,
                                                                        node_text,
                                                                    );
                                                                    if color != DRACULA_FG {
                                                                        spans.push(ColorSpan {
                                                                            start: cap
                                                                                .node
                                                                                .start_byte(),
                                                                            end: cap
                                                                                .node
                                                                                .end_byte(),
                                                                            color,
                                                                        });
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                // ---------------------------------------------------------
                                injection_ms = highlight_trace_elapsed_ms(injection_start);
                            }
                        }
                    }
                }

                let raw_span_count = spans.len();
                let flatten_start = std::time::Instant::now();
                let flat_spans = if is_log {
                    vec![ColorSpan {
                        start: 0,
                        end: text.len(),
                        color: DRACULA_FG,
                    }]
                } else {
                    let apply_rainbow_brackets = should_apply_rainbow_brackets(lang_name);

                    let merged_spans = merge_highlight_spans(
                        last_full_spans.clone(),
                        spans,
                        lang_name,
                        &text,
                        final_edit_start_byte.is_none() || final_edit_end_byte.is_none(),
                        expand_highlight_invalidation_range(
                            &text,
                            final_invalidate_start_byte,
                            final_invalidate_end_byte,
                        ),
                    );

                    flatten_spans(
                        merged_spans,
                        text.len(),
                        text,
                        &mut byte_colors_buf,
                        &error_ranges,
                        apply_rainbow_brackets,
                        false,
                    )
                };
                let flatten_ms = highlight_trace_elapsed_ms(flatten_start);

                last_full_spans = flat_spans.clone();

                let shrink_start = std::time::Instant::now();
                // Очистка памяти от гигантских буферов после парсинга больших файлов.
                let byte_color_cap_limit = text.len().saturating_mul(2).max(64 * 1024);
                if text.len() < 1024 * 512 && byte_colors_buf.capacity() > byte_color_cap_limit {
                    byte_colors_buf.shrink_to_fit();
                }
                let shrink_ms = highlight_trace_elapsed_ms(shrink_start);

                let completions_build_start = std::time::Instant::now();
                inject_builtin_completions(lang_name, &mut completions_map);
                let completions = completion_items_from_map(completions_map);
                let completions_build_ms = highlight_trace_elapsed_ms(completions_build_start);

                let flat_span_count = last_full_spans.len();
                let completion_count = completions.len();
                let fold_count = foldable_ranges.len();
                let error_count = error_ranges.len();
                let send_start = std::time::Instant::now();
                let send_result = tx_out.send((
                    final_request_id,
                    final_version,
                    flat_spans,
                    completions,
                    foldable_ranges,
                    error_ranges,
                    current_tree.clone(),
                    true,
                ));
                worker_control_for_thread.wake_ui();
                let send_ms = highlight_trace_elapsed_ms(send_start);
                let total_ms = highlight_trace_elapsed_ms(worker_start);
                if trace_large || highlight_trace_should_log(text.len(), false, total_ms) {
                    eprintln!(
                        "[HL TRACE worker:done] req={} ver={} bytes={} lines={} lang={} priority_sent={} priority_range={} priority_spans={} raw_spans={} flat_spans={} completions={} folds={} errors={} cache={} byte_buf_cap={} tree={} total_ms={:.2} queued_ms={:.2} keyword_ms={:.2} priority_ms={:.2} parse_ms={:.2} fold_ms={:.2} import_fold_ms={:.2} completion_walk_ms={:.2} query_ms={:.2} injection_ms={:.2} flatten_ms={:.2} shrink_ms={:.2} completions_build_ms={:.2} send_ms={:.2}",
                        final_request_id,
                        final_version,
                        text.len(),
                        highlight_trace_line_count(text),
                        lang_name,
                        priority_sent,
                        priority_range_bytes,
                        priority_span_count,
                        raw_span_count,
                        flat_span_count,
                        completion_count,
                        fold_count,
                        error_count,
                        query_cache.len(),
                        byte_colors_buf.capacity(),
                        current_tree.is_some(),
                        total_ms,
                        queue_ms,
                        keyword_ms,
                        priority_ms,
                        parse_ms,
                        fold_ms,
                        import_fold_ms,
                        completion_walk_ms,
                        query_ms,
                        injection_ms,
                        flatten_ms,
                        shrink_ms,
                        completions_build_ms,
                        send_ms,
                    );
                }
                let _ = send_result;
                }
            });
        if let Err(error) = spawn_result {
            // Sending to the disconnected worker channel will fail harmlessly and
            // the synchronous highlighter path remains available.
            eprintln!("failed to spawn RRiter highlighter worker; using synchronous fallback: {error}");
        }
        let worker = HighlighterWorker {
            control: worker_control,
            shutdown_tx: tx_in.clone(),
        };
        Self {
            tx: tx_in,
            _worker: worker,
            query_cache,
            rx: rx_out,
            spans: vec![],
            completions: vec![],
            foldable_ranges: vec![],
            syntax_errors: vec![],
            current_version: 0,
            is_complete: true,
            current_request_id: 0,
            sync_text: String::new(),
            sync_version: 0,
            sync_ext: String::new(),
            sync_parser: tree_sitter::Parser::new(),
            sync_tree: None,
            sync_byte_colors_buf: Vec::new(),
            pending_priority_anchor: None,
        }
    }
}

#[cfg(test)]
mod highlighter_spawn_regression_tests {
    #[test]
    fn bug_69_highlighter_thread_spawn_failure_keeps_synchronous_fallback() {
        let source = include_str!("highlighter_worker.rs");
        assert!(source.contains("let spawn_result = thread::Builder::new()"));
        assert!(source.contains("using synchronous fallback"));
        assert!(!source.contains(".expect(\"failed to spawn RRiter highlighter worker\")"));
    }
}

#[cfg(test)]
#[path = "../highlighter_tests.rs"]
mod highlighter_tests;

impl Highlighter {
    pub fn lsp_completion_allowed_at_cursor(&self, ext: &str, cursor: usize) -> bool {
        if ext != "dart" {
            return true;
        }
        if self.sync_ext != ext || self.sync_text.is_empty() {
            return false;
        }
        let Some(tree) = self.sync_tree.as_ref() else {
            return false;
        };
        let cursor = cursor.min(self.sync_text.len());
        let start = cursor.saturating_sub(1);
        let end = cursor.max(start.saturating_add(1)).min(self.sync_text.len());
        let Some(mut node) = tree.root_node().descendant_for_byte_range(start, end) else {
            return true;
        };
        loop {
            let kind = node.kind();
            if kind == "template_substitution" {
                return true;
            }
            if matches!(
                kind,
                "comment" | "block_comment" | "documentation_block_comment"
            ) {
                return false;
            }
            if kind.contains("string_literal") {
                return false;
            }
            let Some(parent) = node.parent() else {
                return true;
            };
            node = parent;
        }
    }

    pub fn lsp_signature_help_allowed_at_cursor(&self, ext: &str, cursor: usize) -> bool {
        if ext != "dart" || self.sync_ext != ext || self.sync_text.is_empty() {
            return false;
        }
        let Some(tree) = self.sync_tree.as_ref() else {
            return false;
        };
        let cursor = cursor.min(self.sync_text.len());
        let start = cursor.saturating_sub(1);
        let end = cursor.max(start.saturating_add(1)).min(self.sync_text.len());
        let Some(mut node) = tree.root_node().descendant_for_byte_range(start, end) else {
            return false;
        };
        loop {
            if matches!(node.kind(), "arguments" | "argument_part") {
                return true;
            }
            let Some(parent) = node.parent() else {
                return false;
            };
            node = parent;
        }
    }

    pub(crate) fn syntax_tree_for(
        &self,
        version: u64,
        extension: &str,
    ) -> Option<&tree_sitter::Tree> {
        if self.current_version == version && self.sync_ext == extension {
            self.sync_tree.as_ref()
        } else {
            None
        }
    }

    fn shrink_sync_byte_colors_for_small_text(&mut self) {
        if self.sync_text.len() >= 512 * 1024 {
            return;
        }
        let cap_limit = self.sync_text.len().saturating_mul(2).max(64 * 1024);
        if self.sync_byte_colors_buf.capacity() > cap_limit {
            self.sync_byte_colors_buf.shrink_to_fit();
        }
    }
}
