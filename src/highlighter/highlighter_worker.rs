impl Highlighter {
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
        )>();

        thread::spawn(move || {
            let mut parser = tree_sitter::Parser::new();
            let mut query_cache: HashMap<(&'static str, &'static str), tree_sitter::Query> =
                HashMap::new();
            let mut byte_colors_buf = Vec::new();
            let mut last_full_spans: Vec<ColorSpan> = Vec::new();

            let mut replica_text = String::new();
            let mut current_tree: Option<tree_sitter::Tree> = None;
            let mut current_ext = String::new();

            while let Ok(msg) = rx_in.recv() {
                let mut msgs = vec![msg];
                while let Ok(m) = rx_in.try_recv() {
                    msgs.push(m);
                }

                let mut final_version = 0;
                let mut final_request_id = 0;
                let mut do_highlight = false;
                let mut final_edit_start_byte: Option<usize> = None;
                let mut final_edit_end_byte: Option<usize> = None;
                let mut final_invalidate_start_byte: Option<usize> = None;
                let mut final_invalidate_end_byte: Option<usize> = None;
                let mut final_priority_anchor = 0usize;

                for m in msgs {
                    match m {
                        HighlighterMessage::Reset {
                            request_id,
                            version,
                            text,
                            ext,
                            priority_anchor,
                        } => {
                            final_request_id = request_id;
                            final_version = version;
                            final_priority_anchor = priority_anchor;
                            replica_text = text;
                            current_ext = ext;
                            current_tree = None;
                            do_highlight = true;
                            last_full_spans.clear();
                        }
                        HighlighterMessage::Edits {
                            request_id,
                            version,
                            edits,
                            edit_start_byte,
                            edit_end_byte,
                            invalidate_start_byte,
                            invalidate_end_byte,
                        } => {
                            final_request_id = request_id;
                            final_version = version;
                            final_edit_start_byte = edit_start_byte;
                            final_edit_end_byte = edit_end_byte;
                            final_invalidate_start_byte = invalidate_start_byte;
                            final_invalidate_end_byte = invalidate_end_byte;
                            for edit in edits {
                                match edit {
                                    SyncEdit::Insert { offset, text } => {
                                        let len = text.len();
                                        for span in &mut last_full_spans {
                                            if span.start >= offset {
                                                span.start += len;
                                                span.end += len;
                                            } else if span.end > offset {
                                                span.end += len;
                                            }
                                        }

                                        let start_byte = offset;
                                        let old_end_byte = offset;
                                        let new_end_byte = offset + text.len();

                                        let start_position = get_point(&replica_text, start_byte);
                                        let old_end_position = start_position;

                                        if offset <= replica_text.len()
                                            && replica_text.is_char_boundary(offset)
                                        {
                                            replica_text.insert_str(offset, &text);
                                        } else {
                                            replica_text.push_str(&text);
                                            current_tree = None;
                                        }

                                        let new_end_position =
                                            get_point(&replica_text, new_end_byte);

                                        let input_edit = tree_sitter::InputEdit {
                                            start_byte,
                                            old_end_byte,
                                            new_end_byte,
                                            start_position,
                                            old_end_position,
                                            new_end_position,
                                        };
                                        if let Some(tree) = &mut current_tree {
                                            tree.edit(&input_edit);
                                        }
                                    }
                                    SyncEdit::Delete { offset, len } => {
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

                                        let start_byte = offset;
                                        let old_end_byte = offset + len;
                                        let new_end_byte = offset;

                                        let start_position = get_point(&replica_text, start_byte);
                                        let old_end_position =
                                            get_point(&replica_text, old_end_byte);

                                        if offset + len <= replica_text.len()
                                            && replica_text.is_char_boundary(offset)
                                            && replica_text.is_char_boundary(offset + len)
                                        {
                                            replica_text.replace_range(offset..offset + len, "");
                                        } else {
                                            current_tree = None;
                                        }

                                        let new_end_position = start_position;

                                        let input_edit = tree_sitter::InputEdit {
                                            start_byte,
                                            old_end_byte,
                                            new_end_byte,
                                            start_position,
                                            old_end_position,
                                            new_end_position,
                                        };
                                        if let Some(tree) = &mut current_tree {
                                            tree.edit(&input_edit);
                                        }
                                    }
                                }
                            }
                            do_highlight = true;
                        }
                    }
                }

                if !do_highlight {
                    continue;
                }

                let text = &replica_text;
                let ext = &current_ext;

                let is_log = ext == "log";

                let lang_name = lang_name_for_ext_and_text(ext, text);
                let should_prioritize_front = !is_log
                    && final_edit_start_byte.is_none()
                    && should_prioritize_front_highlight(ext, text);

                let mut spans = Vec::new();
                let mut completions_map: HashMap<(String, usize, usize), SymbolKind> =
                    HashMap::new();
                let mut foldable_ranges = Vec::new();
                let mut error_ranges = Vec::new();

                if !is_log {
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

                    if let Some((lang, queries)) = ts_config {
                        if parser.set_language(&lang).is_ok() {
                            let parsed_tree = parser.parse(&replica_text, current_tree.as_ref());
                            current_tree = parsed_tree.clone();

                            if let Some(tree) = parsed_tree {
                                if let Some(fold_query_str) = get_folding_query(lang_name) {
                                    if let Ok(fold_query) =
                                        tree_sitter::Query::new(&lang, fold_query_str)
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
                                                        let b = text.as_bytes()[start_byte];
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

                                match lang_name {
                                    "py" => {
                                        for block in crate::languages::python::import_blocks(text) {
                                            foldable_ranges.push((
                                                block.start,
                                                block.end,
                                                true,
                                                false,
                                            ));
                                        }
                                    }
                                    "rs" => {
                                        for block in crate::languages::rust::import_blocks(text) {
                                            foldable_ranges.push((
                                                block.start,
                                                block.end,
                                                true,
                                                false,
                                            ));
                                        }
                                    }
                                    "dart" => {
                                        for block in crate::languages::dart::import_blocks(text) {
                                            foldable_ranges.push((
                                                block.start,
                                                block.end,
                                                true,
                                                false,
                                            ));
                                        }
                                    }
                                    _ => {}
                                }

                                if should_prioritize_front {
                                    let priority_range =
                                        priority_highlight_range(text, final_priority_anchor);
                                    if priority_range.start < priority_range.end {
                                        let mut priority_spans = Vec::new();
                                        collect_query_highlight_spans(
                                            &lang,
                                            lang_name,
                                            &queries,
                                            &tree,
                                            &text,
                                            &mut query_cache,
                                            Some(priority_range.clone()),
                                            &mut priority_spans,
                                        );
                                        let priority_spans = merge_highlight_spans(
                                            Vec::new(),
                                            priority_spans,
                                            lang_name,
                                            &text,
                                            true,
                                            None,
                                        );
                                        let priority_spans = flatten_spans_for_range(
                                            priority_spans,
                                            priority_range.clone(),
                                            text,
                                            &mut byte_colors_buf,
                                            !lang_name.is_empty() && lang_name != "bash",
                                        );
                                        last_full_spans = priority_spans.clone();
                                        let priority_foldable_ranges = foldable_ranges
                                            .iter()
                                            .copied()
                                            .filter(|&(start, end, _, _)| {
                                                start < priority_range.end
                                                    && end <= priority_range.end
                                            })
                                            .collect::<Vec<_>>();
                                        let _ = tx_out.send((
                                            final_request_id,
                                            final_version,
                                            priority_spans,
                                            Vec::new(),
                                            priority_foldable_ranges,
                                            Vec::new(),
                                            current_tree.clone(),
                                        ));
                                    }
                                }

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
                                            &text.as_bytes()[node.start_byte()..node.end_byte()],
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

                                let byte_range = if let (Some(sb), Some(eb)) =
                                    (final_edit_start_byte, final_edit_end_byte)
                                {
                                    Some(sb.saturating_sub(1000)..(eb + 1000).min(text.len()))
                                } else {
                                    None
                                };
                                collect_query_highlight_spans(
                                    &lang,
                                    lang_name,
                                    &queries,
                                    &tree,
                                    &text,
                                    &mut query_cache,
                                    byte_range,
                                    &mut spans,
                                );

                                // ---------------------------------------------------------
                                // Обработка языковых инъекций (Language Injections)
                                // ---------------------------------------------------------
                                let mut injected_regions: HashMap<String, Vec<tree_sitter::Range>> =
                                    HashMap::new();
                                if let Some(inj_query_str) = get_injection_query(lang_name) {
                                    if let Ok(inj_query) =
                                        tree_sitter::Query::new(&lang, inj_query_str)
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
                                                        &text.as_bytes()[cap.node.start_byte()
                                                            ..cap.node.end_byte()],
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
                                                    let range = tree_sitter::Range {
                                                        start_byte: node.start_byte(),
                                                        end_byte: node.end_byte(),
                                                        start_point: node.start_position(),
                                                        end_point: node.end_position(),
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
                                    let mapped_lang = match inj_lang_name.as_str() {
                                        "js" | "javascript" => "js",
                                        "ts" | "typescript" => "ts",
                                        "tsx" => "tsx",
                                        "html" => "html",
                                        "css" => "css",
                                        "regex" => "regex",
                                        "json" => "json",
                                        "c" => "c",
                                        "cpp" | "c++" => "cpp",
                                        "make" | "makefile" => "make",
                                        _ => continue,
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
                                                        if let Ok(query) = tree_sitter::Query::new(
                                                            &inj_lang, q_str,
                                                        ) {
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
                                                                            &text.as_bytes()[cap
                                                                                .node
                                                                                .start_byte()
                                                                                ..cap
                                                                                    .node
                                                                                    .end_byte()],
                                                                        )
                                                                        .unwrap_or("");

                                                                    let color = resolve_color(
                                                                        name,
                                                                        node_text,
                                                                        cap.node.start_byte(),
                                                                        &[],
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
                            }
                        }
                    }
                }

                let flat_spans = if is_log {
                    vec![ColorSpan {
                        start: 0,
                        end: text.len(),
                        color: DRACULA_FG,
                    }]
                } else {
                    let apply_rainbow_brackets = !lang_name.is_empty() && lang_name != "bash";

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

                last_full_spans = flat_spans.clone();

                // Очистка памяти от гигантских буферов после парсинга больших файлов
                if byte_colors_buf.capacity() > 1024 * 1024 && text.len() < 1024 * 512 {
                    byte_colors_buf.shrink_to_fit();
                }

                let mut inject_builtins = |items: &[(&str, SymbolKind)]| {
                    for &(word, ref kind) in items {
                        let kind = if *kind == SymbolKind::Keyword {
                            SymbolKind::Keyword
                        } else {
                            SymbolKind::Builtin
                        };
                        completions_map
                            .entry((word.to_string(), 0, usize::MAX))
                            .or_insert(kind);
                    }
                };

                match lang_name {
                    "py" => {
                        inject_builtins(&[
                            ("print", SymbolKind::Function),
                            ("len", SymbolKind::Function),
                            ("int", SymbolKind::Class),
                            ("str", SymbolKind::Class),
                            ("list", SymbolKind::Class),
                            ("dict", SymbolKind::Class),
                            ("set", SymbolKind::Class),
                            ("tuple", SymbolKind::Class),
                            ("bool", SymbolKind::Class),
                            ("float", SymbolKind::Class),
                            ("sum", SymbolKind::Function),
                            ("min", SymbolKind::Function),
                            ("max", SymbolKind::Function),
                            ("abs", SymbolKind::Function),
                            ("isinstance", SymbolKind::Function),
                            ("issubclass", SymbolKind::Function),
                            ("hasattr", SymbolKind::Function),
                            ("getattr", SymbolKind::Function),
                            ("setattr", SymbolKind::Function),
                            ("delattr", SymbolKind::Function),
                            ("dir", SymbolKind::Function),
                            ("type", SymbolKind::Class),
                            ("enumerate", SymbolKind::Function),
                            ("zip", SymbolKind::Function),
                            ("map", SymbolKind::Class),
                            ("filter", SymbolKind::Class),
                            ("range", SymbolKind::Class),
                            ("reversed", SymbolKind::Class),
                            ("open", SymbolKind::Function),
                            ("super", SymbolKind::Function),
                            ("Exception", SymbolKind::Class),
                            ("ValueError", SymbolKind::Class),
                            ("TypeError", SymbolKind::Class),
                            ("KeyError", SymbolKind::Class),
                            ("IndexError", SymbolKind::Class),
                            ("AttributeError", SymbolKind::Class),
                            ("RuntimeError", SymbolKind::Class),
                            ("KeyboardInterrupt", SymbolKind::Class),
                            ("True", SymbolKind::Keyword),
                            ("False", SymbolKind::Keyword),
                            ("None", SymbolKind::Keyword),
                            ("__name__", SymbolKind::Variable),
                            ("__file__", SymbolKind::Variable),
                            ("__doc__", SymbolKind::Variable),
                            ("__dict__", SymbolKind::Variable),
                            ("__init__", SymbolKind::Function),
                            ("__call__", SymbolKind::Function),
                        ]);
                    }
                    "rs" => {
                        inject_builtins(&[
                            ("println!", SymbolKind::Function),
                            ("print!", SymbolKind::Function),
                            ("format!", SymbolKind::Function),
                            ("panic!", SymbolKind::Function),
                            ("vec!", SymbolKind::Function),
                            ("String", SymbolKind::Class),
                            ("Vec", SymbolKind::Class),
                            ("Option", SymbolKind::Class),
                            ("Result", SymbolKind::Class),
                            ("Some", SymbolKind::Variable),
                            ("None", SymbolKind::Variable),
                            ("Ok", SymbolKind::Variable),
                            ("Err", SymbolKind::Variable),
                            ("Box", SymbolKind::Class),
                            ("Rc", SymbolKind::Class),
                            ("Arc", SymbolKind::Class),
                            ("HashMap", SymbolKind::Class),
                            ("HashSet", SymbolKind::Class),
                            ("std", SymbolKind::Variable),
                            ("iter", SymbolKind::Function),
                            ("map", SymbolKind::Function),
                            ("collect", SymbolKind::Function),
                            ("unwrap", SymbolKind::Function),
                            ("expect", SymbolKind::Function),
                            ("clone", SymbolKind::Function),
                            ("as_ref", SymbolKind::Function),
                            ("into", SymbolKind::Function),
                            ("from", SymbolKind::Function),
                            ("mut", SymbolKind::Keyword),
                            ("let", SymbolKind::Keyword),
                            ("fn", SymbolKind::Keyword),
                            ("impl", SymbolKind::Keyword),
                            ("pub", SymbolKind::Keyword),
                            ("struct", SymbolKind::Keyword),
                        ]);
                    }
                    "dart" => {
                        inject_builtins(&[
                            ("print", SymbolKind::Function),
                            ("String", SymbolKind::Class),
                            ("int", SymbolKind::Class),
                            ("double", SymbolKind::Class),
                            ("bool", SymbolKind::Class),
                            ("List", SymbolKind::Class),
                            ("Map", SymbolKind::Class),
                            ("Set", SymbolKind::Class),
                            ("Future", SymbolKind::Class),
                            ("Stream", SymbolKind::Class),
                            ("Widget", SymbolKind::Class),
                            ("StatelessWidget", SymbolKind::Class),
                            ("StatefulWidget", SymbolKind::Class),
                            ("BuildContext", SymbolKind::Class),
                            ("Scaffold", SymbolKind::Class),
                            ("AppBar", SymbolKind::Class),
                            ("Text", SymbolKind::Class),
                            ("Container", SymbolKind::Class),
                            ("Column", SymbolKind::Class),
                            ("Row", SymbolKind::Class),
                            ("ListView", SymbolKind::Class),
                            ("Padding", SymbolKind::Class),
                            ("Center", SymbolKind::Class),
                            ("initState", SymbolKind::Function),
                            ("build", SymbolKind::Function),
                            ("dispose", SymbolKind::Function),
                            ("setState", SymbolKind::Function),
                            ("late", SymbolKind::Keyword),
                            ("final", SymbolKind::Keyword),
                            ("const", SymbolKind::Keyword),
                        ]);
                    }
                    "js" | "ts" | "tsx" => {
                        inject_builtins(&[
                            ("console", SymbolKind::Variable),
                            ("window", SymbolKind::Variable),
                            ("document", SymbolKind::Variable),
                            ("require", SymbolKind::Function),
                            ("setTimeout", SymbolKind::Function),
                            ("setInterval", SymbolKind::Function),
                            ("Promise", SymbolKind::Class),
                            ("Math", SymbolKind::Class),
                            ("Object", SymbolKind::Class),
                            ("Array", SymbolKind::Class),
                            ("String", SymbolKind::Class),
                            ("Number", SymbolKind::Class),
                            ("Boolean", SymbolKind::Class),
                            ("Error", SymbolKind::Class),
                            ("true", SymbolKind::Keyword),
                            ("false", SymbolKind::Keyword),
                            ("null", SymbolKind::Keyword),
                            ("undefined", SymbolKind::Keyword),
                        ]);
                    }
                    "c" | "cpp" => {
                        inject_builtins(&[
                            ("int", SymbolKind::Class),
                            ("float", SymbolKind::Class),
                            ("double", SymbolKind::Class),
                            ("char", SymbolKind::Class),
                            ("void", SymbolKind::Class),
                            ("struct", SymbolKind::Keyword),
                            ("class", SymbolKind::Keyword),
                            ("return", SymbolKind::Keyword),
                            ("if", SymbolKind::Keyword),
                            ("else", SymbolKind::Keyword),
                            ("for", SymbolKind::Keyword),
                            ("while", SymbolKind::Keyword),
                            ("sizeof", SymbolKind::Function),
                            ("printf", SymbolKind::Function),
                            ("malloc", SymbolKind::Function),
                            ("free", SymbolKind::Function),
                            ("true", SymbolKind::Keyword),
                            ("false", SymbolKind::Keyword),
                            ("nullptr", SymbolKind::Keyword),
                            ("this", SymbolKind::Keyword),
                        ]);
                    }
                    _ => {}
                }

                let mut completions: Vec<CompletionItem> = completions_map
                    .into_iter()
                    .map(|((word, scope_start, scope_end), kind)| CompletionItem {
                        word,
                        kind,
                        scope_start,
                        scope_end,
                    })
                    .collect();
                completions.sort_by(|a, b| a.word.cmp(&b.word));

                let _ = tx_out.send((
                    final_request_id,
                    final_version,
                    flat_spans,
                    completions,
                    foldable_ranges,
                    error_ranges,
                    current_tree.clone(),
                ));
            }
        });
        Self {
            tx: tx_in,
            rx: rx_out,
            spans: vec![],
            completions: vec![],
            foldable_ranges: vec![],
            syntax_errors: vec![],
            current_version: 0,
            current_request_id: 0,
            sync_text: String::new(),
            sync_ext: String::new(),
            sync_parser: tree_sitter::Parser::new(),
            sync_tree: None,
            sync_query_cache: HashMap::new(),
            sync_byte_colors_buf: Vec::new(),
        }
    }
}

#[cfg(test)]
#[path = "../highlighter_tests.rs"]
mod highlighter_tests;
