use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use tree_sitter::StreamingIterator; // <-- Обязательный импорт для новых версий tree-sitter

#[derive(Clone, Debug)]
pub struct ColorSpan {
    pub start: usize,
    pub end: usize,
    pub color:[f32; 4],
}

pub struct Highlighter {
    tx: Sender<(u64, String, String, Vec<ColorSpan>)>,
    rx: Receiver<(u64, Vec<ColorSpan>)>,
    pub spans: Vec<ColorSpan>,
    pub current_version: u64,
}

const DRACULA_FG: [f32; 4] =[0.972, 0.972, 0.949, 1.0];
const DRACULA_COMMENT:[f32; 4] =[0.384, 0.447, 0.643, 1.0];
const DRACULA_CYAN:[f32; 4] =[0.545, 0.913, 0.992, 1.0];
const DRACULA_DARK_CYAN: [f32; 4] = [0.45, 0.85, 0.90, 1.0];
const DRACULA_GREEN: [f32; 4] =[0.313, 0.980, 0.482, 1.0];
const DRACULA_ORANGE: [f32; 4] =[0.973, 0.584, 0.502, 1.0];
const DRACULA_PINK:[f32; 4] =[1.0, 0.474, 0.776, 1.0];
const DRACULA_PURPLE:[f32; 4] =[0.741, 0.576, 0.976, 1.0];
const DRACULA_YELLOW: [f32; 4] =[0.945, 0.980, 0.549, 1.0];

// Специальный маркер для захвата интерполяции до этапа плоских спанов
const MARKER_INTERPOLATION: [f32; 4] =[-1.0, 0.0, 0.0, 1.0];

#[derive(Debug)]
struct Scope {
    start: usize,
    end: usize,
    params: HashSet<String>,
}

impl Highlighter {
    pub fn new() -> Self {
        let (tx_in, rx_in) = mpsc::channel::<(u64, String, String, Vec<ColorSpan>)>();
        let (tx_out, rx_out) = mpsc::channel();
        
        thread::spawn(move || {
            let mut syntect_assets: Option<(syntect::parsing::SyntaxSet, syntect::highlighting::ThemeSet)> = None;

            let mut parser = tree_sitter::Parser::new();
            let mut query_cache: HashMap<(&'static str, &'static str), tree_sitter::Query> =
                HashMap::new();
            let mut byte_colors_buf = Vec::new();

            while let Ok((mut version, mut text, mut ext, mut old_spans)) = rx_in.recv() {
                while let Ok((v, t, e, o)) = rx_in.try_recv() {
                    version = v;
                    text = t;
                    ext = e;
                    old_spans = o;
                }

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
                    _ => "",
                };

                let mut spans = Vec::new();
                let mut used_ts = false;
                let mut error_ranges = Vec::new(); 

                if !is_log_or_huge {
                    let ts_config = match lang_name {
                        "bash" => Some((tree_sitter_bash::LANGUAGE.into(), vec![
                            "(word) @any_word",
                            "(function_definition name: (word) @function)",
                            "(function_definition \"()\" @function)",      
                            "[\"(\" \")\" \"{\" \"}\"] @operator",
                            "[\"[\" \"]\"] @keyword",
                            "(string) @string", "(raw_string) @string", "(comment) @comment",
                            "(command_name (word) @command_word)", "(variable_assignment name: (variable_name) @variable)",
                            "\"|\" @operator", "\"||\" @operator", "\"&&\" @operator", "\"&\" @operator",
                            "\">\" @operator", "\">>\" @operator", "\"<\" @operator", "\">&\" @operator", "\"&>\" @operator", "\"&>>\" @operator",
                            "\"<(\" @operator", "\">(\" @operator", "(process_substitution \")\" @operator)",
                            "(file_descriptor) @number", "(file_redirect destination: (_) @number)",
                            "(expansion \"${\" @subst \"}\" @subst)", "(expansion (variable_name) @variable)",
                            "(expansion [\":\" \"-\" \"=\" \"+\" \"?\" \":-\"] @fg)", "(expansion (_) @fg)",
                            "(command_substitution \"$(\" @subst \")\" @subst)", "(command_substitution \"`\" @subst)",
                            "(simple_expansion \"$\" @subst (variable_name) @variable)",
                            "[\"if\" \"then\" \"elif\" \"else\" \"fi\" \"for\" \"while\" \"do\" \"done\" \"case\" \"esac\" \"in\"] @keyword.control",
                            "[\"export\" \"declare\"] @keyword",
                        ])),
                        "rs" => Some((tree_sitter_rust::LANGUAGE.into(), vec![
                            "(string_literal) @string", "(line_comment) @comment", "(block_comment) @comment",
                            "(function_item name: (identifier) @function)", "(call_expression function: (identifier) @function)",
                            "(type_identifier) @type", "(number_literal) @number",
                            "[\"true\" \"false\"] @boolean", "[\"fn\" \"let\" \"mut\" \"pub\" \"struct\" \"enum\" \"trait\" \"impl\" \"for\" \"while\" \"loop\" \"match\" \"if\" \"else\" \"return\" \"use\" \"mod\"] @keyword"
                        ])),
                        "py" => Some((tree_sitter_python::LANGUAGE.into(), vec![
                            "(identifier) @py_ident", 
                            "(attribute attribute: (identifier) @property)",
                            "(string) @string", 
                            "(interpolation) @interpolation", 
                            "(comment) @comment", "(integer) @number", "(float) @number",
                            "(true) @boolean", "(false) @boolean", "(none) @keyword.control",
                            "[\"def\" \"class\" \"return\" \"pass\" \"continue\" \"break\" \"if\" \"elif\" \"else\" \"for\" \"while\" \"import\" \"from\" \"as\" \"async\" \"await\" \"match\" \"case\" \"try\" \"except\" \"finally\" \"raise\" \"with\" \"global\" \"nonlocal\" \"assert\" \"yield\" \"del\" \"and\" \"or\" \"not\" \"is\" \"in\" \"lambda\"] @keyword.control",
                            "[\":\" \"=\"] @keyword.control",
                            "\"->\" @fg",
                            "[\"==\" \"!=\" \"<\" \">\" \"<=\" \">=\" \"+\" \"-\" \"*\" \"/\" \"//\" \"%\" \"**\" \"+=\" \"-=\" \"*=\" \"/=\" \"//=\" \"%=\" \"**=\" \"&\" \"|\" \"^\" \"~\" \"<<\" \">>\"] @operator",
                            "(function_definition name: (identifier) @py_function)", 
                            "(call function: (identifier) @py_builtin_or_func)",
                            "(call function: (attribute attribute: (identifier) @py_function))", 
                            "(class_definition name: (identifier) @class_name)",
                            "(parameters (identifier) @parameter)", 
                            "(parameters (typed_parameter (identifier) @parameter))",
                            "(parameters (default_parameter name: (identifier) @parameter))", 
                            "(parameters (typed_default_parameter name: (identifier) @parameter))",
                            "(lambda_parameters (identifier) @parameter)", 
                            "(keyword_argument name: (identifier) @parameter)",
                            "(assignment left: (identifier) @py_assign)",
                            "(assignment left: (pattern_list (identifier) @py_assign))",
                            "(assignment left: (tuple (identifier) @py_assign))",
                            "(for_statement left: (identifier) @py_assign)",
                            "(decorator \"@\" @keyword.control)", 
                            "(decorator (identifier) @py_function)",
                        ])),
                        // Идеальный нативный импорт из нового пакета!
                        "toml" => Some((tree_sitter_toml_ng::LANGUAGE.into(), vec![
                            "(bare_key) @property",
                            "(string) @string",
                            "(integer) @number",
                            "(float) @number",
                            "(boolean) @boolean",
                            "(comment) @comment",
                            "[\"=\" \"[\" \"]\" \"[[\" \"]]\"] @operator",
                        ])),
                        _ => None,
                    };

                    if let Some((lang, queries)) = ts_config {
                        if parser.set_language(&lang).is_ok() {
                            parser.reset();
                            if let Some(tree) = parser.parse(&text, None) {
                                
                                let mut cursor = tree.walk();
                                let mut visiting = true;
                                while visiting {
                                    let node = cursor.node();
                                    if node.is_error() {
                                        error_ranges.push((node.start_byte(), node.end_byte()));
                                    }
                                    if cursor.goto_first_child() { continue; }
                                    while !cursor.goto_next_sibling() {
                                        if !cursor.goto_parent() {
                                            visiting = false;
                                            break;
                                        }
                                    }
                                }

                                let mut py_scopes = Vec::new();
                                if lang_name == "py" {
                                    for q_str in[
                                        "(function_definition parameters: (parameters) @params body: (_) @body)",
                                        "(lambda parameters: (lambda_parameters) @params body: (_) @body)"
                                    ] {
                                        if let Ok(func_query) = tree_sitter::Query::new(&lang, q_str) {
                                            let mut cursor = tree_sitter::QueryCursor::new();
                                            let mut matches = cursor.matches(&func_query, tree.root_node(), text.as_bytes());
                                            
                                            // <-- Исправлено: используем while let и .next() вместо for
                                            while let Some(m) = matches.next() {
                                                let mut p_node = None;
                                                let mut b_node = None;
                                                for cap in m.captures {
                                                    let cname = func_query.capture_names()[cap.index as usize];
                                                    if cname == "params" { p_node = Some(cap.node); }
                                                    if cname == "body" { b_node = Some(cap.node); }
                                                }
                                                
                                                if let (Some(params_node), Some(body_node)) = (p_node, b_node) {
                                                    let mut params_set = HashSet::new();
                                                    let mut t_cursor = params_node.walk();
                                                    for child in params_node.children(&mut t_cursor) {
                                                        let kind = child.kind();
                                                        let mut name_node = None;
                                                        
                                                        if kind == "identifier" {
                                                            name_node = Some(child);
                                                        } else if kind == "typed_parameter" || kind == "default_parameter" || kind == "typed_default_parameter" {
                                                            let mut inner_cursor = child.walk();
                                                            for inner in child.children(&mut inner_cursor) {
                                                                if inner.kind() == "identifier" {
                                                                    name_node = Some(inner);
                                                                    break;
                                                                }
                                                            }
                                                        } else if kind == "list_splat_pattern" || kind == "dictionary_splat_pattern" {
                                                            let mut inner_cursor = child.walk();
                                                            for inner in child.children(&mut inner_cursor) {
                                                                if inner.kind() == "identifier" {
                                                                    name_node = Some(inner);
                                                                    break;
                                                                }
                                                            }
                                                        }
                                                        
                                                        if let Some(n) = name_node {
                                                            if let Ok(s) = std::str::from_utf8(&text.as_bytes()[n.start_byte()..n.end_byte()]) {
                                                                params_set.insert(s.to_string());
                                                            }
                                                        }
                                                    }
                                                    
                                                    py_scopes.push(Scope {
                                                        start: body_node.start_byte(),
                                                        end: body_node.end_byte(),
                                                        params: params_set,
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }

                                let mut success_count = 0;
                                for q_str in queries {
                                    let cache_key = (lang_name, q_str);
                                    if !query_cache.contains_key(&cache_key) {
                                        if let Ok(query) = tree_sitter::Query::new(&lang, q_str) {
                                            query_cache.insert(cache_key, query);
                                        }
                                    }

                                    if let Some(query) = query_cache.get(&cache_key) {
                                        success_count += 1;
                                        let mut cursor = tree_sitter::QueryCursor::new();
                                        let mut matches = cursor.matches(
                                            query,
                                            tree.root_node(),
                                            text.as_bytes(),
                                        );

                                        // <-- Исправлено: используем while let и .next() вместо for
                                        while let Some(m) = matches.next() {
                                            for cap in m.captures {
                                                let name =
                                                    query.capture_names()[cap.index as usize];
                                                let node_text = std::str::from_utf8(
                                                    &text.as_bytes()[cap.node.start_byte()
                                                        ..cap.node.end_byte()],
                                                )
                                                .unwrap_or("");

                                                let mut color = match name {
                                                    "fg" | "property" | "py_assign" => DRACULA_FG,
                                                    "interpolation" => MARKER_INTERPOLATION,
                                                    "string" => DRACULA_YELLOW,
                                                    "comment" => DRACULA_COMMENT,
                                                    "function" | "py_function" => DRACULA_GREEN,
                                                    "keyword.control" | "operator" | "boolean" => {
                                                        DRACULA_PINK
                                                    }
                                                    "keyword" | "subst" | "type" => DRACULA_CYAN,
                                                    "variable" | "number" => DRACULA_PURPLE,
                                                    "class_name" => DRACULA_DARK_CYAN,
                                                    "parameter" => match node_text {
                                                        "self" | "cls" => DRACULA_PURPLE,
                                                        _ => DRACULA_ORANGE,
                                                    },
                                                    "py_builtin_or_func" => match node_text {
                                                        "print" | "input" | "id" | "dict"
                                                        | "str" | "int" | "float" | "list"
                                                        | "set" | "tuple" | "bool" | "super"
                                                        | "len" | "type" | "dir" | "vars"
                                                        | "hasattr" | "getattr" | "setattr"
                                                        | "delattr" | "isinstance"
                                                        | "issubclass" | "enumerate" | "zip"
                                                        | "map" | "filter" | "sum" | "any"
                                                        | "all" | "min" | "max" | "abs"
                                                        | "round" | "open" => DRACULA_CYAN,
                                                        _ => DRACULA_GREEN,
                                                    },
                                                    "py_ident" => match node_text {
                                                        "Exception" | "ValueError"
                                                        | "TypeError" | "KeyError"
                                                        | "IndexError" | "AttributeError"
                                                        | "RuntimeError" | "KeyboardInterrupt"
                                                        | "int" | "float" | "str" | "bool"
                                                        | "list" | "dict" | "set" | "tuple"
                                                        | "bytes" | "Any" | "Optional"
                                                        | "Union" | "Callable" | "Type"
                                                        | "Dict" | "List" | "Set" | "Tuple"
                                                        | "id" | "print" | "len" | "range"
                                                        | "enumerate" | "sum" | "min" | "max" => DRACULA_CYAN,
                                                        "self" | "cls" => DRACULA_PURPLE,
                                                        _ => DRACULA_FG,
                                                    },
                                                    "command_word" => match node_text {
                                                        "sudo" | "sleep" | "ps" | "date"
                                                        | "grep" | "awk" | "sed" | "cat"
                                                        | "renice" | "ionice" | "systemctl"
                                                        | "tee" | "tr" | "head" | "taskset" => {
                                                            DRACULA_GREEN
                                                        }
                                                        _ => DRACULA_CYAN,
                                                    },
                                                    "any_word" => {
                                                        if node_text.starts_with('-')
                                                            && node_text.len() > 1
                                                        {
                                                            DRACULA_PINK
                                                        } else {
                                                            DRACULA_FG
                                                        }
                                                    }
                                                    _ => DRACULA_FG,
                                                };

                                                if lang_name == "py" && node_text != "self" && node_text != "cls" {
                                                    if matches!(name, "py_ident" | "py_builtin_or_func" | "py_assign" | "parameter") {
                                                        let mut is_param = false;
                                                        for scope in &py_scopes {
                                                            if cap.node.start_byte() >= scope.start && cap.node.start_byte() < scope.end {
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

                                                spans.push(ColorSpan {
                                                    start: cap.node.start_byte(),
                                                    end: cap.node.end_byte(),
                                                    color,
                                                });
                                            }
                                        }
                                    }
                                }
                                if success_count > 0 {
                                    used_ts = true;
                                }
                            }
                        }
                    }

                    if !used_ts {
                        // Отложенно загружаем syntect
                        if syntect_assets.is_none() {
                            let ps = syntect::parsing::SyntaxSet::load_defaults_newlines();
                            let ts = syntect::highlighting::ThemeSet::load_defaults();
                            syntect_assets = Some((ps, ts));
                        }
                        
                        if let Some((ps, ts)) = syntect_assets.as_ref() {
                            let fallback_theme = &ts.themes["base16-ocean.dark"];
                            
                            // Фолбэк для TOML если tree-sitter упадет/не загрузится
                            let syntax = ps.find_syntax_by_extension(&actual_ext)
                                .or_else(|| if actual_ext == "toml" { ps.find_syntax_by_extension("ini") } else { None });

                            if let Some(syntax) = syntax {
                                let mut h = syntect::easy::HighlightLines::new(syntax, fallback_theme);
                                let mut byte_offset = 0;
                                for line in syntect::util::LinesWithEndings::from(&text) {
                                    if let Ok(ranges) = h.highlight_line(line, &ps) {
                                        for (style, s) in ranges {
                                            let start = byte_offset;
                                            let end = start + s.len();
                                            spans.push(ColorSpan {
                                                start,
                                                end,
                                                color:[
                                                    style.foreground.r as f32 / 255.0,
                                                    style.foreground.g as f32 / 255.0,
                                                    style.foreground.b as f32 / 255.0,
                                                    1.0,
                                                ],
                                            });
                                            byte_offset = end;
                                        }
                                    } else {
                                        byte_offset += line.len();
                                    }
                                }
                            }
                        }
                    }
                }

                let apply_rainbow_brackets = lang_name != "bash";

                let flat_spans = flatten_spans(spans, text.len(), &text, &mut byte_colors_buf, error_ranges, old_spans, apply_rainbow_brackets);
                let _ = tx_out.send((version, flat_spans));
            }
        });
        Self {
            tx: tx_in,
            rx: rx_out,
            spans: vec![],
            current_version: 0,
        }
    }

    pub fn request_update(&self, version: u64, text: String, ext: String) {
        let _ = self.tx.send((version, text, ext, self.spans.clone()));
    }

    pub fn poll(&mut self, current_editor_version: u64) -> bool {
        let mut updated = false;
        while let Ok((ver, spans)) = self.rx.try_recv() {
            if ver >= self.current_version {
                self.current_version = ver;
                if ver == current_editor_version {
                    self.spans = spans;
                    updated = true;
                }
            }
        }
        updated
    }

    pub fn shift_insert(&mut self, offset: usize, len: usize, text_opt: Option<&str>, restored_spans: Option<Vec<ColorSpan>>) {
        let special_color = if restored_spans.is_none() {
            text_opt.and_then(|t| match t.trim() {
                "+" | "-" | "*" | "/" | "%" | "=" | "==" | "!=" | "<" | ">" | "<=" | ">=" | "&"
                | "|" | "^" | "~" | ":" => Some(DRACULA_PINK),
                "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" => Some(DRACULA_PURPLE),
                "." | "," | "(" | ")" | "[" | "]" | "{" | "}" => Some(DRACULA_FG),
                _ => None,
            })
        } else {
            None
        };

        let mut new_spans = Vec::new();
        for span in &mut self.spans {
            if span.start > offset {
                span.start += len;
                span.end += len;
            } else if span.start == offset {
                if offset == 0 {
                    if let Some(c) = special_color {
                        new_spans.push(ColorSpan { start: 0, end: len, color: c });
                        span.start += len;
                        span.end += len;
                    } else {
                        span.end += len;
                    }
                } else {
                    span.start += len;
                    span.end += len;
                }
            } else if span.end > offset {
                if let Some(c) = special_color {
                    let old_end = span.end;
                    span.end = offset;
                    new_spans.push(ColorSpan { start: offset, end: offset + len, color: c });
                    new_spans.push(ColorSpan { start: offset + len, end: old_end + len, color: span.color });
                } else {
                    span.end += len;
                }
            } else if span.end == offset {
                if let Some(c) = special_color {
                    new_spans.push(ColorSpan { start: offset, end: offset + len, color: c });
                } else {
                    span.end += len;
                }
            }
        }

        if let Some(mut r_spans) = restored_spans {
            for s in &mut r_spans {
                s.start += offset;
                s.end += offset;
            }
            new_spans.extend(r_spans);
        }

        if !new_spans.is_empty() {
            self.spans.extend(new_spans);
            self.spans.sort_by_key(|s| s.start);
            self.spans.retain(|s| s.start < s.end);
        }
    }

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
}

fn get_bracket_color(depth: usize) -> [f32; 4] {
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

fn flatten_spans(
    mut spans: Vec<ColorSpan>,
    len: usize,
    text: &str,
    byte_colors: &mut Vec<[f32; 4]>,
    error_ranges: Vec<(usize, usize)>,
    old_spans: Vec<ColorSpan>,
    apply_rainbow_brackets: bool,
) -> Vec<ColorSpan> {
    spans.sort_by_key(|s| std::cmp::Reverse(s.end - s.start));

    byte_colors.clear();
    // Базовый цвет для всего файла — всегда яркий DRACULA_FG
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

    if !error_ranges.is_empty() && !old_spans.is_empty() {
        let mut old_byte_colors = vec![DRACULA_FG; len];
        for span in old_spans {
            for i in span.start..span.end.min(len) {
                old_byte_colors[i] = span.color;
            }
        }

        for (e_start, e_end) in error_ranges {
            for i in e_start..e_end.min(len) {
                byte_colors[i] = old_byte_colors[i];
            }
        }
    }

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