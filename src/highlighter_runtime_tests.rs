use super::*;

fn stale_comment_toggle_texts() -> (String, String) {
    // Commented block (old tree) vs the same block after uncomment removed "# " (new text).
    let commented: String = (0..400)
        .map(|i| format!("# value_{i} = compute({i}, 'x')\n"))
        .collect();
    let uncommented = commented.replace("# ", "");
    (commented, uncommented)
}

fn parse_python(text: &str) -> tree_sitter::Tree {
    let (lang, _) = get_ts_config("py").expect("python config");
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&lang).expect("python language");
    parser.parse(text, None).expect("python parse")
}

#[test]
fn highlighter_tree_root_end_matches_text_len_with_trailing_whitespace() {
    for text in ["x = 1\n", "x = 1\n\n\n", "x = 1   \n  \n   ", "# c\n", "", "\n\n"] {
        let tree = parse_python(text);
        assert_eq!(tree.root_node().end_byte(), text.len(), "text={text:?}");
    }
}

#[test]
fn highlighter_query_spans_skip_captures_past_text_end_for_stale_tree() {
    let (commented, uncommented) = stale_comment_toggle_texts();
    let stale_tree = parse_python(&commented);
    let (lang, queries) = get_ts_config("py").expect("python config");
    let mut cache = HashMap::new();
    let mut spans = Vec::new();
    collect_query_highlight_spans(
        &lang,
        "py",
        &queries,
        &stale_tree,
        &uncommented,
        &mut cache,
        None,
        &mut spans,
    );
    assert!(spans.iter().all(|span| span.end <= uncommented.len()));
}

#[test]
fn highlighter_sync_parse_rejects_worker_tree_of_other_text_version() {
    let (commented, uncommented) = stale_comment_toggle_texts();
    let mut highlighter = Highlighter::new();
    highlighter.current_request_id = 1;
    highlighter.sync_ext = "py".to_string();
    highlighter.sync_text = uncommented.clone();
    // The worker answers for the same version with a tree of the pre-uncomment text.
    assert!(highlighter.apply_poll_result(
        1,
        5,
        5,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Some(parse_python(&commented)),
        true,
    ));
    assert!(highlighter.sync_highlight_after_edit(
        6,
        None,
        None,
        None,
        None,
        std::time::Duration::from_secs(2),
    ));
    let tree = highlighter.sync_tree.as_ref().expect("fresh sync tree");
    assert_eq!(tree.root_node().end_byte(), uncommented.len());
    assert!(highlighter.spans.iter().all(|span| span.end <= uncommented.len()));
}

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
    highlighter.apply_edits(2, vec![SyncEdit::Insert { offset: 1, text: "S".to_string() }], Some(0), Some(2));
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
            .any(|span| span.start == 0 && span.end >= 2 && span.color == DRACULA_PURPLE)
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
    let insert = SyncEdit::Insert { offset: insert_at, text: "    if\n".to_string() };
    highlighter.apply_edits(2, vec![insert], Some(insert_at), Some(insert_at + 7));
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
    highlighter.apply_edits(2, vec![SyncEdit::Delete { offset: 3, len: 1 }], Some(0), Some(3));
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
            .any(|span| span.start == 0 && span.end >= 3 && span.color == DRACULA_PURPLE)
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
fn priority_range_covers_minimap_window_in_both_directions() {
    let mut text = String::new();
    let mut line_starts = Vec::new();
    for idx in 0..5000 {
        line_starts.push(text.len());
        text.push_str(&format!("value_{idx} = {idx}\n"));
    }
    let anchor = line_starts[2500];
    let upper_minimap_line = line_starts[1550];
    let lower_minimap_line = line_starts[3450];
    let range = priority_highlight_range("py", &text, anchor);

    assert!(range.start <= upper_minimap_line);
    assert!(range.end > lower_minimap_line);
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
fn newer_pending_priority_anchor_survives_older_partial_result() {
    let mut highlighter = Highlighter::new();
    highlighter.current_request_id = 7;
    highlighter.current_version = 1;
    highlighter.sync_text = "x".repeat(200);
    highlighter.pending_priority_anchor = Some(150);

    assert!(highlighter.apply_poll_result(
        7,
        1,
        1,
        vec![ColorSpan {
            start: 0,
            end: 100,
            color: DRACULA_FG,
        }],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        None,
        true,
    ));
    assert_eq!(highlighter.pending_priority_anchor, Some(150));

    assert!(highlighter.apply_poll_result(
        7,
        1,
        1,
        vec![ColorSpan {
            start: 100,
            end: 200,
            color: DRACULA_FG,
        }],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        None,
        true,
    ));
    assert_eq!(highlighter.pending_priority_anchor, None);
}

#[test]
fn unhighlighted_range_anchor_finds_gaps_in_scroll_direction() {
    let mut highlighter = Highlighter::new();
    highlighter.sync_text = "x".repeat(400);
    highlighter.spans = vec![
        ColorSpan {
            start: 0,
            end: 100,
            color: DRACULA_FG,
        },
        ColorSpan {
            start: 200,
            end: 300,
            color: DRACULA_FG,
        },
    ];

    assert_eq!(
        highlighter.unhighlighted_anchor_in_range(50, 250, false),
        Some(100)
    );
    assert_eq!(
        highlighter.unhighlighted_anchor_in_range(50, 250, true),
        Some(199)
    );
    assert_eq!(
        highlighter.unhighlighted_anchor_in_range(0, 100, false),
        None
    );
}

#[test]
fn highlighter_huge_file_loads_visible_slice_on_priority_request() {
    let mut highlighter = Highlighter::new();
    let line = "value = 1\n";
    let line_count = TREE_SITTER_FULL_HIGHLIGHT_MAX_LINES + 10_000;
    let text = line.repeat(line_count);
    let anchor_line = line_count * 3 / 4;
    let anchor = anchor_line * line.len();
    let minimap_start = (anchor_line - 900) * line.len();
    let minimap_end = (anchor_line + 900) * line.len();

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

    assert!(highlighter.is_byte_highlighted(minimap_start));
    assert!(highlighter.is_byte_highlighted(anchor));
    assert!(highlighter.is_byte_highlighted(minimap_end));
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

#[test]
fn r3_100_reset_send_failure_finishes_with_synchronous_fallback() {
    let mut highlighter = Highlighter::new();
    let (tx, rx) = std::sync::mpsc::channel();
    drop(rx);
    highlighter.tx = tx;
    highlighter.reset(9, "let value = 1;\n".to_string(), "rs".to_string(), 0);
    assert!(highlighter.is_complete);
    assert_eq!(highlighter.current_version, 9);
    assert!(highlighter.pending_priority_anchor.is_none());
}

#[test]
fn r3_101_priority_send_failure_does_not_leave_pending_anchor() {
    let mut highlighter = Highlighter::new();
    let (tx, rx) = std::sync::mpsc::channel();
    drop(rx);
    highlighter.tx = tx;
    highlighter.current_request_id = 1;
    highlighter.current_version = 1;
    highlighter.sync_ext = "py".to_string();
    highlighter.sync_text = "x = 1\n".repeat(TREE_SITTER_FULL_HIGHLIGHT_MAX_LINES + 20);
    highlighter.spans.clear();
    assert!(!highlighter.request_priority_highlight(1, 100));
    assert!(highlighter.pending_priority_anchor.is_none());
}

#[test]
fn r3_102_worker_disconnect_clears_stale_pending_state() {
    let mut highlighter = Highlighter::new();
    let (tx, rx) = std::sync::mpsc::channel();
    drop(tx);
    highlighter.rx = rx;
    highlighter.spans = vec![ColorSpan { start: 0, end: 1, color: DRACULA_FG }];
    highlighter.completions = vec![CompletionItem { word: "stale".to_string(), kind: SymbolKind::Unknown, scope_start: 0, scope_end: 1 }];
    highlighter.foldable_ranges = vec![(0, 1, false, false)];
    highlighter.syntax_errors = vec![(0, 1)];
    highlighter.pending_priority_anchor = Some(0);
    highlighter.is_complete = false;
    assert!(highlighter.poll(77));
    assert!(highlighter.spans.is_empty());
    assert!(highlighter.completions.is_empty());
    assert!(highlighter.foldable_ranges.is_empty());
    assert!(highlighter.syntax_errors.is_empty());
    assert!(highlighter.pending_priority_anchor.is_none());
    assert!(highlighter.is_complete);
    assert_eq!(highlighter.current_version, 77);
}

#[test]
fn dart_lsp_completion_uses_syntax_tree_for_comments_strings_and_interpolation() {
    let mut highlighter = Highlighter::new();
    let text = "void main() {
  // Wid
  print('Wid');
  print('${Wid}');
}
";
    highlighter.reset(101, text.to_string(), "dart".to_string(), 0);
    assert!(highlighter.wait_for_first_result(101, std::time::Duration::from_secs(2)));

    let comment_cursor = text.find("// Wid").unwrap() + "// Wid".len();
    let string_cursor = text.find("'Wid'").unwrap() + "'Wid".len();
    let interpolation_cursor = text.find("${Wid}").unwrap() + "${Wid".len();
    assert!(!highlighter.lsp_completion_allowed_at_cursor("dart", comment_cursor));
    assert!(!highlighter.lsp_completion_allowed_at_cursor("dart", string_cursor));
    assert!(highlighter.lsp_completion_allowed_at_cursor("dart", interpolation_cursor));
}

#[test]
fn dart_signature_help_is_limited_to_argument_lists() {
    let mut highlighter = Highlighter::new();
    let text = "void main() {
  build(title: 'x', count: 2);
  final outside = 1;
}
";
    highlighter.reset(102, text.to_string(), "dart".to_string(), 0);
    assert!(highlighter.wait_for_first_result(102, std::time::Duration::from_secs(2)));

    let argument_cursor = text.find("count: 2").unwrap() + "count".len();
    let outside_cursor = text.find("outside").unwrap() + "outside".len();
    assert!(highlighter.lsp_signature_help_allowed_at_cursor("dart", argument_cursor));
    assert!(!highlighter.lsp_signature_help_allowed_at_cursor("dart", outside_cursor));
}

