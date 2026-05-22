// highlighter.rs

#[path = "../highlighter_runtime.rs"]
mod runtime;
use runtime::flatten_spans;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use tree_sitter::StreamingIterator;

use crate::queries::{get_folding_query, get_injection_query, get_params_query, get_ts_config};

#[derive(Clone, Debug)]
pub struct ColorSpan {
    pub start: usize,
    pub end: usize,
    pub color: [f32; 4],
}

pub fn flatten_color_spans_prefer_specific(
    mut spans: Vec<ColorSpan>,
    len: usize,
) -> Vec<ColorSpan> {
    if spans.is_empty() || len == 0 {
        return Vec::new();
    }

    spans.sort_by_key(|s| std::cmp::Reverse(s.end.saturating_sub(s.start)));

    let mut byte_colors = vec![None; len];
    for span in spans {
        let start = span.start.min(len);
        let end = span.end.min(len);
        if start >= end {
            continue;
        }
        for color in &mut byte_colors[start..end] {
            *color = Some(span.color);
        }
    }

    let mut out = Vec::new();
    let mut current = byte_colors[0];
    let mut start = 0usize;
    for (i, color) in byte_colors.iter().copied().enumerate().skip(1) {
        if color != current {
            if let Some(color) = current {
                out.push(ColorSpan {
                    start,
                    end: i,
                    color,
                });
            }
            start = i;
            current = color;
        }
    }
    if let Some(color) = current {
        out.push(ColorSpan {
            start,
            end: len,
            color,
        });
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    Variable,
    Function,
    Class,
    Parameter,
    Argument,
    Property,
    Module,
    Builtin,
    Keyword,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct CompletionItem {
    pub word: String,
    pub kind: SymbolKind,
    pub scope_start: usize,
    pub scope_end: usize,
}

#[derive(Clone, Debug)]
pub enum SyncEdit {
    Insert { offset: usize, text: String },
    Delete { offset: usize, len: usize },
}

pub enum HighlighterMessage {
    Reset {
        request_id: u64,
        version: u64,
        text: String,
        ext: String,
        priority_anchor: usize,
    },
    Edits {
        request_id: u64,
        version: u64,
        edits: Vec<SyncEdit>,
        edit_start_byte: Option<usize>,
        edit_end_byte: Option<usize>,
        invalidate_start_byte: Option<usize>,
        invalidate_end_byte: Option<usize>,
    },
}

pub struct Highlighter {
    tx: Sender<HighlighterMessage>,
    pub rx: Receiver<(
        u64,
        u64,
        Vec<ColorSpan>,
        Vec<CompletionItem>,
        Vec<(usize, usize, bool, bool)>, // (start, end, is_autofold, is_sticky)
        Vec<(usize, usize)>,             // syntax errors
        Option<tree_sitter::Tree>,
    )>,
    pub spans: Vec<ColorSpan>,
    pub completions: Vec<CompletionItem>,
    pub foldable_ranges: Vec<(usize, usize, bool, bool)>,
    pub syntax_errors: Vec<(usize, usize)>,
    pub current_version: u64,
    current_request_id: u64,
    sync_text: String,
    sync_ext: String,
    sync_parser: tree_sitter::Parser,
    sync_tree: Option<tree_sitter::Tree>,
    sync_query_cache: HashMap<(&'static str, &'static str), tree_sitter::Query>,
    sync_byte_colors_buf: Vec<[f32; 4]>,
}

pub(crate) const DRACULA_FG: [f32; 4] = [0.972, 0.972, 0.949, 1.0];
pub(crate) const DRACULA_COMMENT: [f32; 4] = [0.384, 0.447, 0.643, 1.0];
pub(crate) const DRACULA_CYAN: [f32; 4] = [0.545, 0.913, 0.992, 1.0];
pub(crate) const DRACULA_DARK_CYAN: [f32; 4] = [0.45, 0.85, 0.90, 1.0];
pub(crate) const DRACULA_GREEN: [f32; 4] = [0.313, 0.980, 0.482, 1.0];
pub(crate) const DRACULA_ORANGE: [f32; 4] = [0.973, 0.584, 0.502, 1.0];
pub(crate) const DRACULA_PINK: [f32; 4] = [1.0, 0.474, 0.776, 1.0];
pub(crate) const DRACULA_PURPLE: [f32; 4] = [0.741, 0.576, 0.976, 1.0];
pub(crate) const DRACULA_YELLOW: [f32; 4] = [0.945, 0.980, 0.549, 1.0];

const MARKER_INTERPOLATION: [f32; 4] = [-1.0, 0.0, 0.0, 1.0];
pub(crate) const TREE_SITTER_HIGHLIGHT_MAX_BYTES: usize = 64 * 1024;
pub(crate) const TREE_SITTER_HIGHLIGHT_MAX_LINES: usize = 800;
const PRIORITY_HIGHLIGHT_HEAD_LINES: usize = 80;
const PRIORITY_HIGHLIGHT_HEAD_MIN_BYTES: usize = 12 * 1024;
const PRIORITY_HIGHLIGHT_TAIL_LINES: usize = 240;
const PRIORITY_HIGHLIGHT_TAIL_MIN_BYTES: usize = 32 * 1024;

#[derive(Debug)]
struct Scope {
    start: usize,
    end: usize,
    params: HashSet<String>,
}

fn get_point(text: &str, byte_offset: usize) -> tree_sitter::Point {
    let byte_offset = byte_offset.min(text.len());
    let prefix = &text.as_bytes()[..byte_offset];
    let row = prefix.iter().filter(|&&b| b == b'\n').count();
    let last_nl = prefix.iter().rposition(|&b| b == b'\n');
    let column = if let Some(nl) = last_nl {
        byte_offset - (nl + 1)
    } else {
        byte_offset
    };
    tree_sitter::Point::new(row, column)
}

fn resolve_color(
    name: &str,
    node_text: &str,
    start_byte: usize,
    param_scopes: &[Scope],
) -> [f32; 4] {
    let mut color = match name {
        "fg" | "property" | "py_assign" => DRACULA_FG,
        "interpolation" => MARKER_INTERPOLATION,
        "string" => DRACULA_YELLOW,
        "comment" => DRACULA_COMMENT,
        "function" | "py_function" => DRACULA_GREEN,
        "keyword.control" | "operator" | "boolean" => DRACULA_PINK,
        "keyword" | "subst" | "type" | "function.builtin" => DRACULA_CYAN,
        "class_name" => DRACULA_DARK_CYAN,
        "constant" => DRACULA_PURPLE,
        "parameter" => match node_text {
            "self" | "cls" => DRACULA_PURPLE,
            _ => DRACULA_ORANGE,
        },
        "py_builtin_or_func" => match node_text {
            "print" | "input" | "id" | "dict" | "str" | "int" | "float" | "list" | "set"
            | "tuple" | "bool" | "super" | "len" | "type" | "dir" | "vars" | "hasattr"
            | "getattr" | "setattr" | "delattr" | "isinstance" | "issubclass" | "enumerate"
            | "zip" | "map" | "filter" | "sum" | "any" | "all" | "min" | "max" | "abs"
            | "round" | "open" => DRACULA_CYAN,
            _ => DRACULA_GREEN,
        },
        "py_ident" => match node_text {
            "Exception" | "ValueError" | "TypeError" | "KeyError" | "IndexError"
            | "AttributeError" | "RuntimeError" | "KeyboardInterrupt" | "int" | "float" | "str"
            | "bool" | "list" | "dict" | "set" | "tuple" | "bytes" | "Any" | "Optional"
            | "Union" | "Callable" | "Type" | "Dict" | "List" | "Set" | "Tuple" | "print"
            | "len" | "range" | "enumerate" | "sum" | "min" | "max" => DRACULA_CYAN,
            "self" | "cls" => DRACULA_PURPLE,
            _ => DRACULA_FG,
        },
        "command_word" => match node_text {
            "sudo" | "sleep" | "ps" | "date" | "grep" | "awk" | "sed" | "cat" | "renice"
            | "ionice" | "systemctl" | "tee" | "tr" | "head" | "taskset" => DRACULA_GREEN,
            _ => DRACULA_CYAN,
        },
        "any_word" => {
            if node_text.starts_with('-') && node_text.len() > 1 {
                DRACULA_PINK
            } else {
                DRACULA_FG
            }
        }
        "variable" => DRACULA_FG,
        "number" => DRACULA_PURPLE,
        _ => DRACULA_FG,
    };

    if node_text == "None" {
        color = DRACULA_PINK;
    }

    if node_text != "self" && node_text != "cls" {
        if matches!(
            name,
            "py_ident" | "py_builtin_or_func" | "py_assign" | "parameter" | "variable" | "fg"
        ) {
            let mut is_param = false;
            for scope in param_scopes {
                if start_byte >= scope.start && start_byte < scope.end {
                    if scope.params.contains(node_text) {
                        is_param = true;
                        break;
                    }
                }
            }
            if is_param {
                color = DRACULA_ORANGE;
            }
        }
    }
    color
}

fn is_python_attribute_property(node: tree_sitter::Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if parent.kind() != "attribute" {
        return false;
    }
    parent.child_by_field_name("attribute").is_some_and(|attr| {
        attr.start_byte() == node.start_byte() && attr.end_byte() == node.end_byte()
    })
}

fn collect_param_scopes(
    lang: &tree_sitter::Language,
    lang_name: &'static str,
    tree: &tree_sitter::Tree,
    text: &str,
) -> Vec<Scope> {
    let mut param_scopes = Vec::new();
    let Some(q_str) = get_params_query(lang_name) else {
        return param_scopes;
    };
    let Ok(func_query) = tree_sitter::Query::new(lang, q_str) else {
        return param_scopes;
    };

    let mut cursor = tree_sitter::QueryCursor::new();
    let mut matches = cursor.matches(&func_query, tree.root_node(), text.as_bytes());
    while let Some(m) = matches.next() {
        let mut p_node = None;
        let mut b_node = None;
        for cap in m.captures {
            let cname = func_query.capture_names()[cap.index as usize];
            if cname == "params" {
                p_node = Some(cap.node);
            }
            if cname == "body" {
                b_node = Some(cap.node);
            }
        }

        let Some(params_node) = p_node else {
            continue;
        };
        let scope_start = b_node
            .map(|n| n.start_byte())
            .unwrap_or(params_node.end_byte());
        let scope_end = b_node.map(|n| n.end_byte()).unwrap_or(
            params_node
                .parent()
                .map(|p| p.end_byte())
                .unwrap_or(usize::MAX),
        );

        let mut params_set = HashSet::new();
        let mut t_cursor = params_node.walk();
        let mut exploring = true;
        while exploring {
            let c_node = t_cursor.node();
            if c_node.kind() == "identifier" {
                if let Ok(s) =
                    std::str::from_utf8(&text.as_bytes()[c_node.start_byte()..c_node.end_byte()])
                {
                    params_set.insert(s.to_string());
                }
            }
            if t_cursor.goto_first_child() {
                continue;
            }
            while !t_cursor.goto_next_sibling() {
                if !t_cursor.goto_parent() || t_cursor.node() == params_node {
                    exploring = false;
                    break;
                }
            }
        }

        if !params_set.is_empty() {
            param_scopes.push(Scope {
                start: scope_start,
                end: scope_end,
                params: params_set,
            });
        }
    }

    param_scopes
}

fn collect_query_highlight_spans(
    lang: &tree_sitter::Language,
    lang_name: &'static str,
    queries: &[&'static str],
    tree: &tree_sitter::Tree,
    text: &str,
    query_cache: &mut HashMap<(&'static str, &'static str), tree_sitter::Query>,
    byte_range: Option<Range<usize>>,
    spans: &mut Vec<ColorSpan>,
) {
    let param_scopes = collect_param_scopes(lang, lang_name, tree, text);

    for q_str in queries {
        let cache_key = (lang_name, *q_str);
        if !query_cache.contains_key(&cache_key) {
            if let Ok(query) = tree_sitter::Query::new(lang, q_str) {
                query_cache.insert(cache_key, query);
            }
        }

        let Some(query) = query_cache.get(&cache_key) else {
            continue;
        };
        let mut cursor = tree_sitter::QueryCursor::new();
        if let Some(range) = &byte_range {
            cursor.set_byte_range(range.clone());
        }
        let mut matches = cursor.matches(query, tree.root_node(), text.as_bytes());
        while let Some(m) = matches.next() {
            for cap in m.captures {
                let name = query.capture_names()[cap.index as usize];
                let node_text = std::str::from_utf8(
                    &text.as_bytes()[cap.node.start_byte()..cap.node.end_byte()],
                )
                .unwrap_or("");
                if lang_name == "py" && name == "py_ident" && is_python_attribute_property(cap.node)
                {
                    continue;
                }

                let color = resolve_color(name, node_text, cap.node.start_byte(), &param_scopes);
                if lang_name == "py" && name == "docstring" {
                    crate::languages::python::push_docstring_highlight_spans(
                        text,
                        cap.node.start_byte(),
                        cap.node.end_byte(),
                        spans,
                    );
                    continue;
                }

                if color != DRACULA_FG {
                    spans.push(ColorSpan {
                        start: cap.node.start_byte(),
                        end: cap.node.end_byte(),
                        color,
                    });
                }
            }
        }
    }
}

fn cut_spans_by_ranges(base: &mut Vec<ColorSpan>, ranges: &mut Vec<(usize, usize)>) {
    if ranges.is_empty() || base.is_empty() {
        return;
    }
    ranges.sort_unstable_by_key(|(start, _)| *start);

    let mut merged_ranges: Vec<(usize, usize)> = Vec::with_capacity(ranges.len());
    for &(start, end) in ranges.iter() {
        if let Some((_, last_end)) = merged_ranges.last_mut() {
            if start <= *last_end {
                *last_end = (*last_end).max(end);
                continue;
            }
        }
        merged_ranges.push((start, end));
    }

    let mut retained = Vec::with_capacity(base.len());
    for span in base.iter() {
        let mut pos = span.start;
        for &(cut_start, cut_end) in &merged_ranges {
            if cut_end <= pos {
                continue;
            }
            if cut_start >= span.end {
                break;
            }
            if cut_start > pos {
                retained.push(ColorSpan {
                    start: pos,
                    end: cut_start.min(span.end),
                    color: span.color,
                });
            }
            pos = pos.max(cut_end);
            if pos >= span.end {
                break;
            }
        }
        if pos < span.end {
            retained.push(ColorSpan {
                start: pos,
                end: span.end,
                color: span.color,
            });
        }
    }

    *base = retained;
}

fn overlay_spans_preserving_gaps(base: &mut Vec<ColorSpan>, overlays: &[ColorSpan]) {
    let mut ranges: Vec<(usize, usize)> = overlays
        .iter()
        .filter_map(|span| (span.start < span.end).then_some((span.start, span.end)))
        .collect();
    cut_spans_by_ranges(base, &mut ranges);
}

fn is_highlight_ident_byte(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

pub(crate) fn should_prioritize_front_highlight(ext: &str, text: &str) -> bool {
    let lang_name = lang_name_for_ext_and_text(ext, text);
    if !matches!(lang_name, "py" | "rs") {
        return false;
    }
    if text.len() > TREE_SITTER_HIGHLIGHT_MAX_BYTES {
        return true;
    }

    let mut lines = 1usize;
    for &b in text.as_bytes() {
        if b == b'\n' {
            lines += 1;
            if lines > TREE_SITTER_HIGHLIGHT_MAX_LINES {
                return true;
            }
        }
    }
    false
}

fn priority_highlight_range(lang_name: &'static str, text: &str, anchor: usize) -> Range<usize> {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return 0..0;
    }

    let mut anchor = anchor.min(bytes.len());
    while anchor > 0 && !text.is_char_boundary(anchor) {
        anchor -= 1;
    }

    let mut start = anchor.saturating_sub(PRIORITY_HIGHLIGHT_HEAD_MIN_BYTES);
    let mut seen = 0usize;
    let mut i = anchor;
    while i > 0 && seen < PRIORITY_HIGHLIGHT_HEAD_LINES {
        i -= 1;
        if bytes[i] == b'\n' {
            seen += 1;
            start = start.min(i + 1);
        }
    }
    while start > 0 && bytes[start - 1] != b'\n' {
        start -= 1;
    }
    while start > 0 && !text.is_char_boundary(start) {
        start -= 1;
    }

    let mut end = (anchor + PRIORITY_HIGHLIGHT_TAIL_MIN_BYTES).min(bytes.len());
    let mut seen = 0usize;
    let mut i = anchor;
    while i < bytes.len() && seen < PRIORITY_HIGHLIGHT_TAIL_LINES {
        if bytes[i] == b'\n' {
            seen += 1;
            end = i + 1;
        }
        i += 1;
    }
    end = end.max(anchor).min(bytes.len());
    while end < bytes.len() && !text.is_char_boundary(end) {
        end += 1;
    }

    if lang_name == "py" {
        python_priority_highlight_range(text, anchor, start, end)
    } else {
        start..end
    }
}

fn line_start_at_or_before(text: &str, byte: usize) -> usize {
    let byte = byte.min(text.len());
    text.as_bytes()[..byte]
        .iter()
        .rposition(|&b| b == b'\n')
        .map(|pos| pos + 1)
        .unwrap_or(0)
}

fn next_line_start(text: &str, line_start: usize) -> Option<usize> {
    text.as_bytes()
        .get(line_start..)?
        .iter()
        .position(|&b| b == b'\n')
        .map(|pos| line_start + pos + 1)
}

fn line_end(text: &str, line_start: usize) -> usize {
    text.as_bytes()
        .get(line_start..)
        .and_then(|tail| tail.iter().position(|&b| b == b'\n'))
        .map(|pos| line_start + pos)
        .unwrap_or(text.len())
}

fn line_indent(line: &str) -> usize {
    line.bytes()
        .take_while(|&b| b == b' ' || b == b'\t')
        .count()
}

fn python_priority_highlight_range(
    text: &str,
    anchor: usize,
    fallback_start: usize,
    fallback_end: usize,
) -> Range<usize> {
    let window_start_line = line_start_at_or_before(text, fallback_start);
    let mut scan = window_start_line;
    let mut start = fallback_start;

    loop {
        let end = line_end(text, scan);
        if let Some(line) = text.get(scan..end) {
            let trimmed = line.trim_start();
            if !trimmed.is_empty() && !trimmed.starts_with('#') && line_indent(line) == 0 {
                start = scan;
                break;
            }
        }
        if scan == 0 {
            break;
        }
        scan = line_start_at_or_before(text, scan.saturating_sub(1));
    }

    while start > 0 {
        let prev = line_start_at_or_before(text, start.saturating_sub(1));
        let end = line_end(text, prev);
        let Some(line) = text.get(prev..end) else {
            break;
        };
        let trimmed = line.trim_start();
        if line_indent(line) == 0 && (trimmed.starts_with('@') || trimmed.is_empty()) {
            start = prev;
        } else {
            break;
        }
    }

    let mut end = fallback_end;
    let window_end_line = line_start_at_or_before(text, fallback_end);
    let mut scan = next_line_start(text, window_end_line).unwrap_or(text.len());
    while scan < text.len() {
        let line_end = line_end(text, scan);
        if let Some(line) = text.get(scan..line_end) {
            let trimmed = line.trim_start();
            if !trimmed.is_empty() && !trimmed.starts_with('#') && line_indent(line) == 0 {
                end = scan;
                break;
            }
        }
        scan = next_line_start(text, scan).unwrap_or(text.len());
    }

    start..end.max(anchor).min(text.len())
}

fn push_language_import_foldable_ranges(
    lang_name: &'static str,
    text: &str,
    foldable_ranges: &mut Vec<(usize, usize, bool, bool)>,
) {
    match lang_name {
        "py" => {
            for block in crate::languages::python::import_blocks(text) {
                foldable_ranges.push((block.start, block.end, true, false));
            }
        }
        "rs" => {
            for block in crate::languages::rust::import_blocks(text) {
                foldable_ranges.push((block.start, block.end, true, false));
            }
        }
        "dart" => {
            for block in crate::languages::dart::import_blocks(text) {
                foldable_ranges.push((block.start, block.end, true, false));
            }
        }
        _ => {}
    }
}

fn priority_highlight_spans_from_slice(
    parser: &mut tree_sitter::Parser,
    lang: &tree_sitter::Language,
    lang_name: &'static str,
    queries: &[&'static str],
    text: &str,
    range: Range<usize>,
    query_cache: &mut HashMap<(&'static str, &'static str), tree_sitter::Query>,
    byte_colors_buf: &mut Vec<[f32; 4]>,
) -> Vec<ColorSpan> {
    if range.start >= range.end || range.end > text.len() {
        return Vec::new();
    }

    let Some(priority_text) = text.get(range.clone()) else {
        return Vec::new();
    };
    let Some(tree) = parser.parse(priority_text, None) else {
        return Vec::new();
    };

    let mut spans = Vec::new();
    collect_query_highlight_spans(
        lang,
        lang_name,
        queries,
        &tree,
        priority_text,
        query_cache,
        None,
        &mut spans,
    );
    let mut shifted = Vec::with_capacity(spans.len());
    for span in &mut spans {
        let start = span.start + range.start;
        let end = span.end + range.start;
        if start < end && start < range.end && end > range.start {
            shifted.push(ColorSpan {
                start: start.max(range.start),
                end: end.min(range.end),
                color: span.color,
            });
        }
    }

    let spans = merge_highlight_spans(Vec::new(), shifted, lang_name, text, true, None);
    flatten_spans_for_range(
        spans,
        range,
        text,
        byte_colors_buf,
        !lang_name.is_empty() && lang_name != "bash",
    )
}

fn expand_highlight_invalidation_range(
    text: &str,
    start: Option<usize>,
    end: Option<usize>,
) -> Option<Range<usize>> {
    let bytes = text.as_bytes();
    let mut start = start?.min(bytes.len());
    let mut end = end?.min(bytes.len());
    if start > end {
        std::mem::swap(&mut start, &mut end);
    }

    while start > 0 && is_highlight_ident_byte(bytes[start - 1]) {
        start -= 1;
    }
    while end < bytes.len() && is_highlight_ident_byte(bytes[end]) {
        end += 1;
    }

    (start < end).then_some(start..end)
}

pub(crate) fn sync_edit_invalidation_byte_range(
    edits: &[SyncEdit],
) -> (Option<usize>, Option<usize>) {
    let mut start = None::<usize>;
    let mut end = None::<usize>;
    for edit in edits {
        let (edit_start, edit_end) = match edit {
            SyncEdit::Insert { offset, text } => (*offset, offset + text.len()),
            SyncEdit::Delete { offset, .. } => (*offset, *offset),
        };
        start = Some(start.map_or(edit_start, |current| current.min(edit_start)));
        end = Some(end.map_or(edit_end, |current| current.max(edit_end)));
    }
    (start, end)
}

fn merge_highlight_spans(
    mut base: Vec<ColorSpan>,
    spans: Vec<ColorSpan>,
    lang_name: &'static str,
    text: &str,
    replace_all: bool,
    invalidation_range: Option<Range<usize>>,
) -> Vec<ColorSpan> {
    if replace_all {
        base.clear();
    } else {
        if let Some(range) = invalidation_range {
            let mut ranges = vec![(range.start, range.end)];
            cut_spans_by_ranges(&mut base, &mut ranges);
        }
        overlay_spans_preserving_gaps(&mut base, &spans);
    }
    base.extend(spans);

    if lang_name == "py" {
        for (start, end) in crate::languages::python::python_class_attr_name_ranges(text) {
            base.retain(|span| span.start != start || span.end != end);
        }
    }

    base
}

fn flatten_spans_for_range(
    mut spans: Vec<ColorSpan>,
    range: Range<usize>,
    text: &str,
    byte_colors: &mut Vec<[f32; 4]>,
    apply_rainbow_brackets: bool,
) -> Vec<ColorSpan> {
    if range.start >= range.end {
        return Vec::new();
    }

    let len = range.end - range.start;
    spans.sort_by_key(|s| std::cmp::Reverse(s.end.saturating_sub(s.start)));

    byte_colors.clear();
    byte_colors.resize(len, DRACULA_FG);

    for span in spans {
        let start = span.start.max(range.start);
        let end = span.end.min(range.end);
        if start >= end {
            continue;
        }
        for color in &mut byte_colors[start - range.start..end - range.start] {
            *color = span.color;
        }
    }

    let text_bytes = text.as_bytes();
    for (local_idx, byte_idx) in (range.start..range.end).enumerate() {
        let b = text_bytes[byte_idx];
        if byte_colors[local_idx] == MARKER_INTERPOLATION {
            if b == b'{' || b == b'}' {
                byte_colors[local_idx] = DRACULA_ORANGE;
            } else {
                byte_colors[local_idx] = DRACULA_FG;
            }
        }
    }

    if apply_rainbow_brackets {
        let mut depth_round = 0usize;
        let mut depth_square = 0usize;
        let mut depth_curly = 0usize;

        for byte_idx in 0..range.start {
            match text_bytes[byte_idx] {
                b'(' => depth_round += 1,
                b')' => depth_round = depth_round.saturating_sub(1),
                b'[' => depth_square += 1,
                b']' => depth_square = depth_square.saturating_sub(1),
                b'{' => depth_curly += 1,
                b'}' => depth_curly = depth_curly.saturating_sub(1),
                _ => {}
            }
        }

        for (local_idx, byte_idx) in (range.start..range.end).enumerate() {
            if byte_colors[local_idx] != DRACULA_COMMENT
                && (byte_colors[local_idx] == DRACULA_FG
                    || byte_colors[local_idx] == DRACULA_GREEN
                    || byte_colors[local_idx] == DRACULA_CYAN
                    || byte_colors[local_idx] == DRACULA_ORANGE
                    || byte_colors[local_idx] == DRACULA_YELLOW
                    || byte_colors[local_idx] == DRACULA_PURPLE)
            {
                match text_bytes[byte_idx] {
                    b'(' => {
                        byte_colors[local_idx] = runtime::get_bracket_color(depth_round);
                        depth_round += 1;
                    }
                    b')' => {
                        depth_round = depth_round.saturating_sub(1);
                        byte_colors[local_idx] = runtime::get_bracket_color(depth_round);
                    }
                    b'[' => {
                        byte_colors[local_idx] = runtime::get_bracket_color(depth_square);
                        depth_square += 1;
                    }
                    b']' => {
                        depth_square = depth_square.saturating_sub(1);
                        byte_colors[local_idx] = runtime::get_bracket_color(depth_square);
                    }
                    b'{' => {
                        byte_colors[local_idx] = runtime::get_bracket_color(depth_curly);
                        depth_curly += 1;
                    }
                    b'}' => {
                        depth_curly = depth_curly.saturating_sub(1);
                        byte_colors[local_idx] = runtime::get_bracket_color(depth_curly);
                    }
                    _ => {}
                }
            }
        }
    }

    let mut flat = Vec::new();
    let mut current_color = byte_colors[0];
    let mut start = range.start;
    for (local_idx, byte_idx) in (range.start + 1..range.end).enumerate() {
        let color = byte_colors[local_idx + 1];
        if color != current_color {
            flat.push(ColorSpan {
                start,
                end: byte_idx,
                color: current_color,
            });
            start = byte_idx;
            current_color = color;
        }
    }
    flat.push(ColorSpan {
        start,
        end: range.end,
        color: current_color,
    });
    flat
}

pub fn tree_sitter_lang_name_for_ext(ext: &str) -> &'static str {
    match ext {
        "sh" | "bash" => "bash",
        "rs" => "rs",
        "py" | "pyi" => "py",
        "toml" => "toml",
        "go" => "go",
        "js" | "jsx" | "mjs" | "cjs" => "js",
        "ts" => "ts",
        "tsx" => "tsx",
        "regex" => "regex",
        "java" => "java",
        "cs" => "cs",
        "dart" => "dart",
        "html" | "htm" => "html",
        "css" => "css",
        "json" => "json",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" => "cpp",
        "make" | "mk" | "mak" | "makefile" | "Makefile" | "GNUmakefile" => "make",
        _ => "",
    }
}

fn lang_name_for_ext_and_text(ext: &str, text: &str) -> &'static str {
    let actual_ext = if ext.is_empty() && text.starts_with("#!") {
        if text.contains("bash") {
            "bash"
        } else if text.contains("sh") {
            "sh"
        } else if text.contains("python") {
            "py"
        } else {
            ""
        }
    } else {
        ext
    };

    tree_sitter_lang_name_for_ext(actual_ext)
}

fn prev_char_start(text: &str, mut offset: usize) -> usize {
    offset = offset.min(text.len());
    if offset > 0 {
        offset -= 1;
    }
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn push_ast_select_range(ranges: &mut Vec<(usize, usize)>, text: &str, start: usize, end: usize) {
    if start >= end
        || end > text.len()
        || !text.is_char_boundary(start)
        || !text.is_char_boundary(end)
    {
        return;
    }

    let bytes = text.as_bytes();
    let inner = match (bytes[start], bytes[end - 1]) {
        (b'(', b')') | (b'[', b']') | (b'{', b'}') | (b'"', b'"') | (b'\'', b'\'')
            if start + 1 < end - 1 =>
        {
            Some((start + 1, end - 1))
        }
        _ => None,
    };
    if let Some((l, r)) = inner {
        ranges.push((l, r));
    }

    let mut trimmed_start = start;
    let mut trimmed_end = end;
    while trimmed_start < trimmed_end && bytes[trimmed_start].is_ascii_whitespace() {
        trimmed_start += 1;
    }
    while trimmed_end > trimmed_start && bytes[trimmed_end - 1].is_ascii_whitespace() {
        trimmed_end -= 1;
    }
    if trimmed_start < trimmed_end && (trimmed_start != start || trimmed_end != end) {
        ranges.push((trimmed_start, trimmed_end));
    }

    ranges.push((start, end));
}

fn push_ast_select_line_range(
    ranges: &mut Vec<(usize, usize)>,
    text: &str,
    sel_start: usize,
    sel_end: usize,
) {
    let bytes = text.as_bytes();
    let mut start = sel_start.min(text.len());
    while start > 0 && bytes[start - 1] != b'\n' {
        start -= 1;
    }

    let mut end = sel_end.min(text.len());
    while end < text.len() && bytes[end] != b'\n' {
        end += 1;
    }

    if start < end {
        ranges.push((start, end));
    }
}

pub fn ast_select_expand_range(
    text: &str,
    ext: &str,
    cursor: usize,
    selection_anchor: Option<usize>,
) -> Option<(usize, usize)> {
    if text.is_empty() || ext == "log" || text.len() > 500_000 {
        return None;
    }

    let lang_name = lang_name_for_ext_and_text(ext, text);
    let Some((lang, _)) = get_ts_config(lang_name) else {
        return None;
    };

    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&lang).is_err() {
        return None;
    }
    let tree = parser.parse(text, None)?;

    let cursor = cursor.min(text.len());
    let (sel_start, sel_end) = if let Some(anchor) = selection_anchor {
        (
            anchor.min(cursor).min(text.len()),
            anchor.max(cursor).min(text.len()),
        )
    } else {
        (cursor, cursor)
    };

    let probe = if sel_start == sel_end {
        let bytes = text.as_bytes();
        if cursor < text.len() && !bytes[cursor].is_ascii_whitespace() {
            cursor
        } else {
            prev_char_start(text, cursor)
        }
    } else {
        sel_start
    };

    let root = tree.root_node();
    let mut node = root.descendant_for_byte_range(probe, probe)?;
    let mut ranges = Vec::new();
    push_ast_select_line_range(&mut ranges, text, sel_start, sel_end);
    loop {
        if node.parent().is_some() {
            push_ast_select_range(&mut ranges, text, node.start_byte(), node.end_byte());
        }
        let Some(parent) = node.parent() else {
            break;
        };
        node = parent;
    }

    let mut best = None;
    let mut best_len = usize::MAX;
    for (start, end) in ranges {
        if start <= sel_start && end >= sel_end && (start < sel_start || end > sel_end) {
            let len = end - start;
            if len < best_len {
                best_len = len;
                best = Some((start, end));
            }
        }
    }
    best
}
