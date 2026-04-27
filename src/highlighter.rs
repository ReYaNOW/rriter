// highlighter.rs

#[path = "highlighter_runtime.rs"]
mod runtime;
use runtime::flatten_spans;
use std::collections::{HashMap, HashSet};
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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    Variable,
    Function,
    Class,
    Parameter,
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
        version: u64,
        text: String,
        ext: String,
    },
    Edits {
        version: u64,
        edits: Vec<SyncEdit>,
        edit_start_byte: Option<usize>,
        edit_end_byte: Option<usize>,
    },
}

pub struct Highlighter {
    tx: Sender<HighlighterMessage>,
    pub rx: Receiver<(
        u64,
        Vec<ColorSpan>,
        Vec<CompletionItem>,
        Vec<(usize, usize, bool, bool)>, // (start, end, is_autofold, is_sticky)
        Vec<(usize, usize)>,             // syntax errors
    )>,
    pub spans: Vec<ColorSpan>,
    pub completions: Vec<CompletionItem>,
    pub foldable_ranges: Vec<(usize, usize, bool, bool)>,
    pub syntax_errors: Vec<(usize, usize)>,
    pub current_version: u64,
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

impl Highlighter {
    pub fn new() -> Self {
        let (tx_in, rx_in) = mpsc::channel::<HighlighterMessage>();
        let (tx_out, rx_out) = mpsc::channel::<(
            u64,
            Vec<ColorSpan>,
            Vec<CompletionItem>,
            Vec<(usize, usize, bool, bool)>,
            Vec<(usize, usize)>,
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
                let mut do_highlight = false;
                let mut final_edit_start_byte: Option<usize> = None;
                let mut final_edit_end_byte: Option<usize> = None;

                for m in msgs {
                    match m {
                        HighlighterMessage::Reset { version, text, ext } => {
                            final_version = version;
                            replica_text = text;
                            current_ext = ext;
                            current_tree = None;
                            do_highlight = true;
                            last_full_spans.clear();
                        }
                        HighlighterMessage::Edits {
                            version,
                            edits,
                            edit_start_byte,
                            edit_end_byte,
                        } => {
                            final_version = version;
                            final_edit_start_byte = edit_start_byte;
                            final_edit_end_byte = edit_end_byte;
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

                let is_log_or_huge = ext == "log" || text.len() > 500_000;

                let actual_ext = if ext.is_empty() && text.starts_with("#!") {
                    if text.contains("bash") {
                        "bash".to_string()
                    } else if text.contains("sh") {
                        "sh".to_string()
                    } else if text.contains("python") {
                        "py".to_string()
                    } else {
                        ext.clone()
                    }
                } else {
                    ext.clone()
                };

                let lang_name = match actual_ext.as_str() {
                    "sh" | "bash" => "bash",
                    "rs" => "rs",
                    "py" => "py",
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
                };

                let mut spans = Vec::new();
                let mut completions_map: HashMap<(String, usize, usize), SymbolKind> =
                    HashMap::new();
                let mut foldable_ranges = Vec::new();
                let mut error_ranges = Vec::new();

                if !is_log_or_huge {
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
                                        if let (Some(sb), Some(eb)) =
                                            (final_edit_start_byte, final_edit_end_byte)
                                        {
                                            // Expand bounds to ensure we capture whole nodes/statements
                                            let exp_sb = sb.saturating_sub(1000);
                                            let exp_eb = (eb + 1000).min(text.len());
                                            cursor.set_byte_range(exp_sb..exp_eb);
                                        }
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

                                let mut param_scopes = Vec::new();
                                if let Some(q_str) = get_params_query(lang_name) {
                                    if let Ok(func_query) = tree_sitter::Query::new(&lang, q_str) {
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
                                            &func_query,
                                            tree.root_node(),
                                            text.as_bytes(),
                                        );

                                        while let Some(m) = matches.next() {
                                            let mut p_node = None;
                                            let mut b_node = None;
                                            for cap in m.captures {
                                                let cname =
                                                    func_query.capture_names()[cap.index as usize];
                                                if cname == "params" {
                                                    p_node = Some(cap.node);
                                                }
                                                if cname == "body" {
                                                    b_node = Some(cap.node);
                                                }
                                            }

                                            if let Some(params_node) = p_node {
                                                let scope_start = b_node
                                                    .map(|n| n.start_byte())
                                                    .unwrap_or(params_node.end_byte());
                                                let scope_end =
                                                    b_node.map(|n| n.end_byte()).unwrap_or(
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
                                                        if let Ok(s) = std::str::from_utf8(
                                                            &text.as_bytes()[c_node.start_byte()
                                                                ..c_node.end_byte()],
                                                        ) {
                                                            params_set.insert(s.to_string());
                                                        }
                                                    }
                                                    if t_cursor.goto_first_child() {
                                                        continue;
                                                    }
                                                    while !t_cursor.goto_next_sibling() {
                                                        if !t_cursor.goto_parent()
                                                            || t_cursor.node() == params_node
                                                        {
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
                                        }
                                    }
                                }

                                for q_str in queries {
                                    let cache_key = (lang_name, q_str);
                                    if !query_cache.contains_key(&cache_key) {
                                        if let Ok(query) = tree_sitter::Query::new(&lang, q_str) {
                                            query_cache.insert(cache_key, query);
                                        }
                                    }

                                    if let Some(query) = query_cache.get(&cache_key) {
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
                                            query,
                                            tree.root_node(),
                                            text.as_bytes(),
                                        );

                                        while let Some(m) = matches.next() {
                                            for cap in m.captures {
                                                let name =
                                                    query.capture_names()[cap.index as usize];
                                                let node_text = std::str::from_utf8(
                                                    &text.as_bytes()[cap.node.start_byte()
                                                        ..cap.node.end_byte()],
                                                )
                                                .unwrap_or("");

                                                let color = resolve_color(
                                                    name,
                                                    node_text,
                                                    cap.node.start_byte(),
                                                    &param_scopes,
                                                );

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

                let apply_rainbow_brackets = !lang_name.is_empty() && lang_name != "bash";

                let mut merged_spans = last_full_spans.clone();
                if let (Some(sb), Some(eb)) = (final_edit_start_byte, final_edit_end_byte) {
                    let exp_sb = sb.saturating_sub(1000);
                    let exp_eb = (eb + 1000).min(text.len());
                    merged_spans.retain(|s| s.end <= exp_sb || s.start >= exp_eb);
                } else {
                    merged_spans.clear();
                }
                merged_spans.extend(spans);

                let flat_spans = flatten_spans(
                    merged_spans,
                    text.len(),
                    text,
                    &mut byte_colors_buf,
                    &error_ranges,
                    apply_rainbow_brackets,
                    is_log_or_huge,
                );

                last_full_spans = flat_spans.clone();

                // Очистка памяти от гигантских буферов после парсинга больших файлов
                if byte_colors_buf.capacity() > 1024 * 1024 && text.len() < 1024 * 512 {
                    byte_colors_buf.shrink_to_fit();
                }

                let mut inject_builtins = |items: &[(&str, SymbolKind)]| {
                    for &(word, ref kind) in items {
                        completions_map
                            .entry((word.to_string(), 0, usize::MAX))
                            .or_insert_with(|| kind.clone());
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
                            ("self", SymbolKind::Variable),
                            ("cls", SymbolKind::Variable),
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
                    final_version,
                    flat_spans,
                    completions,
                    foldable_ranges,
                    error_ranges,
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn wait(highlighter: &mut Highlighter, version: u64) {
        assert!(
            highlighter.wait_for_first_result(version, Duration::from_secs(2)),
            "highlighter did not produce version {version}"
        );
        assert_eq!(highlighter.current_version, version);
    }

    #[test]
    fn highlighter_thread_resets_parses_edits_and_injects_language_builtins() {
        let mut highlighter = Highlighter::new();

        highlighter.reset(
            1,
            "def greet(name):\n    return f'hi {name}'\n".to_string(),
            "py".to_string(),
        );
        wait(&mut highlighter, 1);
        assert!(highlighter.spans.iter().any(|span| span.color == DRACULA_PINK));
        assert!(highlighter.completions.iter().any(|item| item.word == "print"));
        assert!(highlighter.completions.iter().any(|item| item.word == "name"));

        highlighter.apply_edits(
            2,
            vec![SyncEdit::Insert {
                offset: 0,
                text: "# comment\n".to_string(),
            }],
            Some(0),
            Some(10),
        );
        wait(&mut highlighter, 2);
        assert!(highlighter.spans.iter().any(|span| span.color == DRACULA_COMMENT));

        let cases = [
            (
                "rs",
                "fn main() { let value = Some(1); println!(\"{}\", value); }\n",
                "println!",
            ),
            ("dart", "void main() { print('hi'); }\n", "Widget"),
            ("js", "function run(value) { console.log(value); }\n", "Promise"),
            ("cpp", "int main() { printf(\"hi\"); return 0; }\n", "printf"),
            ("makefile", "all:\n\tcc main.c\n", "if"),
        ];

        for (idx, (ext, text, builtin)) in cases.iter().enumerate() {
            let version = 10 + idx as u64;
            highlighter.reset(version, (*text).to_string(), (*ext).to_string());
            wait(&mut highlighter, version);
            assert!(
                highlighter
                    .completions
                    .iter()
                    .any(|item| item.word == *builtin),
                "missing builtin {builtin} for {ext}"
            );
        }
    }

    #[test]
    fn highlighter_thread_handles_shebang_log_and_invalid_incremental_edit() {
        let mut highlighter = Highlighter::new();

        highlighter.reset(
            1,
            "#!/usr/bin/env python\nprint(None)\n".to_string(),
            String::new(),
        );
        wait(&mut highlighter, 1);
        assert!(highlighter.completions.iter().any(|item| item.word == "print"));

        highlighter.reset(2, "plain log text\n".to_string(), "log".to_string());
        wait(&mut highlighter, 2);
        assert_eq!(highlighter.spans.len(), 1);
        assert_eq!(highlighter.spans[0].color, DRACULA_FG);

        highlighter.apply_edits(
            3,
            vec![SyncEdit::Delete {
                offset: 100,
                len: 4,
            }],
            Some(100),
            Some(104),
        );
        wait(&mut highlighter, 3);
        assert!(highlighter.syntax_errors.is_empty());
    }
}
