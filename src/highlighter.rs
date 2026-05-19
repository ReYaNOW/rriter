// highlighter.rs

#[path = "highlighter_runtime.rs"]
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

fn priority_highlight_range(text: &str, anchor: usize) -> Range<usize> {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return 0..0;
    }

    let mut anchor = anchor.min(bytes.len());
    while anchor > 0 && !text.is_char_boundary(anchor) {
        anchor -= 1;
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

    0..end
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
                                                        | SymbolKind::Parameter => {
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
#[path = "highlighter_tests.rs"]
mod highlighter_tests;
