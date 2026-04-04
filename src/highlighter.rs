use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use tree_sitter::StreamingIterator;

#[derive(Clone, Debug)]
pub struct ColorSpan {
    pub start: usize,
    pub end: usize,
    pub color: [f32; 4],
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
    },
}

pub struct Highlighter {
    tx: Sender<HighlighterMessage>,
    rx: Receiver<(u64, Vec<ColorSpan>, Vec<CompletionItem>)>,
    pub spans: Vec<ColorSpan>,
    pub completions: Vec<CompletionItem>,
    pub current_version: u64,
}

const DRACULA_FG: [f32; 4] = [0.972, 0.972, 0.949, 1.0];
const DRACULA_COMMENT: [f32; 4] = [0.384, 0.447, 0.643, 1.0];
const DRACULA_CYAN: [f32; 4] = [0.545, 0.913, 0.992, 1.0];
const DRACULA_DARK_CYAN: [f32; 4] = [0.45, 0.85, 0.90, 1.0];
const DRACULA_GREEN: [f32; 4] = [0.313, 0.980, 0.482, 1.0];
const DRACULA_ORANGE: [f32; 4] = [0.973, 0.584, 0.502, 1.0];
const DRACULA_PINK: [f32; 4] = [1.0, 0.474, 0.776, 1.0];
const DRACULA_PURPLE: [f32; 4] = [0.741, 0.576, 0.976, 1.0];
const DRACULA_YELLOW: [f32; 4] = [0.945, 0.980, 0.549, 1.0];

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

impl Highlighter {
    pub fn new() -> Self {
        let (tx_in, rx_in) = mpsc::channel::<HighlighterMessage>();
        let (tx_out, rx_out) = mpsc::channel::<(u64, Vec<ColorSpan>, Vec<CompletionItem>)>();

        thread::spawn(move || {
            let mut syntect_assets: Option<(
                syntect::parsing::SyntaxSet,
                syntect::highlighting::ThemeSet,
            )> = None;

            let mut parser = tree_sitter::Parser::new();
            let mut query_cache: HashMap<(&'static str, &'static str), tree_sitter::Query> =
                HashMap::new();
            let mut byte_colors_buf = Vec::new();

            let mut replica_text = String::new();
            let mut current_tree: Option<tree_sitter::Tree> = None;
            let mut current_ext = String::new();
            let mut old_spans = Vec::new();

            while let Ok(msg) = rx_in.recv() {
                let mut msgs = vec![msg];
                while let Ok(m) = rx_in.try_recv() {
                    msgs.push(m);
                }

                let mut final_version = 0;
                let mut do_highlight = false;

                for m in msgs {
                    match m {
                        HighlighterMessage::Reset { version, text, ext } => {
                            final_version = version;
                            replica_text = text;
                            current_ext = ext;
                            current_tree = None;
                            do_highlight = true;
                        }
                        HighlighterMessage::Edits { version, edits } => {
                            final_version = version;
                            for edit in edits {
                                match edit {
                                    SyncEdit::Insert { offset, text } => {
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
                    "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" => "js",
                    "java" => "java",
                    "cs" => "cs",
                    "dart" => "dart",
                    _ => "",
                };

                let mut spans = Vec::new();
                let mut completions_map: HashMap<(String, usize, usize), SymbolKind> =
                    HashMap::new();
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
                            "(expansion[\":\" \"-\" \"=\" \"+\" \"?\" \":-\"] @fg)", "(expansion (_) @fg)",
                            "(command_substitution \"$(\" @subst \")\" @subst)", "(command_substitution \"`\" @subst)",
                            "(simple_expansion \"$\" @subst (variable_name) @variable)",
                            "[\"if\" \"then\" \"elif\" \"else\" \"fi\" \"for\" \"while\" \"do\" \"done\" \"case\" \"esac\" \"in\"] @keyword.control",
                            "[\"export\" \"declare\" \"return\" \"function\" \"local\" \"readonly\"] @keyword",
                        ])),
                        "rs" => Some((tree_sitter_rust::LANGUAGE.into(), vec![
                            "(string_literal) @string", "(line_comment) @comment", "(block_comment) @comment",
                            "(function_item name: (identifier) @function)", "(call_expression function: (identifier) @function)",
                            "(type_identifier) @type", "(number_literal) @number",
                            "[\"true\" \"false\"] @boolean", "[\"fn\" \"let\" \"mut\" \"pub\" \"struct\" \"enum\" \"trait\" \"impl\" \"for\" \"while\" \"loop\" \"match\" \"if\" \"else\" \"return\" \"use\" \"mod\" \"break\" \"continue\" \"await\" \"async\" \"unsafe\" \"crate\" \"super\"] @keyword"
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
                        "toml" => Some((tree_sitter_toml_ng::LANGUAGE.into(), vec![
                            "(bare_key) @property",
                            "(string) @string",
                            "(integer) @number",
                            "(float) @number",
                            "(boolean) @boolean",
                            "(comment) @comment",
                            "[\"=\" \"[\" \"]\" \"[[\" \"]]\"] @operator",
                        ])),
                        "go" => Some((tree_sitter_go::LANGUAGE.into(), vec![
                            "(identifier) @variable",
                            "(type_identifier) @type",
                            "(function_declaration name: (identifier) @function)",
                            "(method_declaration name: (identifier) @function)",
                            "(call_expression function: (identifier) @function)",
                            "(string_literal) @string",
                            "(int_literal) @number",
                            "(float_literal) @number",
                            "(comment) @comment",
                            "[\"func\" \"var\" \"const\" \"type\" \"struct\" \"interface\" \"package\" \"import\" \"return\" \"if\" \"else\" \"for\" \"range\" \"switch\" \"case\" \"default\" \"go\" \"defer\" \"map\" \"chan\"] @keyword.control",
                            "[\"true\" \"false\" \"nil\"] @boolean",
                            "[\"=\" \":=\" \"==\" \"!=\" \"<\" \">\" \"<=\" \">=\" \"+\" \"-\" \"*\" \"/\" \"%\" \"&&\" \"||\" \"!\" \"<-\"] @operator",
                        ])),
                        "js" => Some((tree_sitter_javascript::LANGUAGE.into(), vec![
                            "(identifier) @variable",
                            "(string) @string",
                            "(number) @number",
                            "(comment) @comment",
                            "(function_declaration name: (identifier) @function)",
                            "(method_definition name: (property_identifier) @function)",
                            "(call_expression function: (identifier) @function)",
                            "(call_expression function: (member_expression property: (property_identifier) @function))",
                            "(property_identifier) @property",
                            "[\"function\" \"const\" \"let\" \"var\" \"return\" \"if\" \"else\" \"for\" \"while\" \"do\" \"switch\" \"case\" \"default\" \"break\" \"continue\" \"import\" \"export\" \"from\" \"class\" \"extends\" \"new\" \"try\" \"catch\" \"finally\" \"throw\" \"async\" \"await\" \"yield\" \"typeof\" \"instanceof\"] @keyword.control",
                            "[\"true\" \"false\" \"null\" \"undefined\"] @boolean",
                            "[\"=\" \"==\" \"===\" \"!=\" \"!==\" \"<\" \">\" \"<=\" \">=\" \"+\" \"-\" \"*\" \"/\" \"%\" \"**\" \"+=\" \"-=\" \"*=\" \"/=\" \"%=\" \"**=\" \"&&\" \"||\" \"!\" \"?\" \":\"] @operator",
                        ])),
                        "java" => Some((tree_sitter_java::LANGUAGE.into(), vec![
                            "(identifier) @variable",
                            "(type_identifier) @type",
                            "(string_literal) @string",
                            "(decimal_integer_literal) @number",
                            "(decimal_floating_point_literal) @number",
                            "(comment) @comment",
                            "(method_declaration name: (identifier) @function)",
                            "(method_invocation name: (identifier) @function)",
                            "[\"class\" \"interface\" \"enum\" \"public\" \"private\" \"protected\" \"static\" \"final\" \"void\" \"return\" \"if\" \"else\" \"for\" \"while\" \"do\" \"switch\" \"case\" \"default\" \"break\" \"continue\" \"import\" \"package\" \"new\" \"try\" \"catch\" \"finally\" \"throw\" \"throws\" \"extends\" \"implements\" \"this\" \"super\"] @keyword.control",
                            "[\"true\" \"false\" \"null\"] @boolean",
                            "[\"=\" \"==\" \"!=\" \"<\" \">\" \"<=\" \">=\" \"+\" \"-\" \"*\" \"/\" \"%\" \"+=\" \"-=\" \"*=\" \"/=\" \"%=\" \"&&\" \"||\" \"!\" \"?\" \":\"] @operator",
                            "(annotation name: (identifier) @keyword.control)",
                        ])),
                        "cs" => Some((tree_sitter_c_sharp::LANGUAGE.into(), vec![
                            "(identifier) @variable",
                            "(string_literal) @string",
                            "(integer_literal) @number",
                            "(real_literal) @number",
                            "(comment) @comment",
                            "(method_declaration name: (identifier) @function)",
                            "(invocation_expression function: (identifier) @function)",
                            "(invocation_expression function: (member_access_expression name: (identifier) @function))",
                            "[\"class\" \"interface\" \"enum\" \"struct\" \"public\" \"private\" \"protected\" \"internal\" \"static\" \"readonly\" \"void\" \"return\" \"if\" \"else\" \"for\" \"foreach\" \"in\" \"while\" \"do\" \"switch\" \"case\" \"default\" \"break\" \"continue\" \"using\" \"namespace\" \"new\" \"try\" \"catch\" \"finally\" \"throw\" \"async\" \"await\" \"yield\" \"this\" \"base\" \"var\"] @keyword.control",
                            "[\"true\" \"false\" \"null\"] @boolean",
                            "[\"=\" \"==\" \"!=\" \"<\" \">\" \"<=\" \">=\" \"+\" \"-\" \"*\" \"/\" \"%\" \"+=\" \"-=\" \"*=\" \"/=\" \"%=\" \"&&\" \"||\" \"!\" \"?\" \":\"] @operator",
                        ])),
                        "dart" => Some((tree_sitter_dart_orchard::LANGUAGE.into(), vec![
                            "(identifier) @variable",
                            "(string_literal) @string",
                            "(decimal_integer_literal) @number",
                            "(decimal_floating_point_literal) @number",
                            "(comment) @comment",
                            "(function_signature name: (identifier) @function)",
                            "(method_signature name: (identifier) @function)",
                            "(function_expression_body (identifier) @function)",
                            "(call_expression function: (identifier) @function)",
                            "(call_expression function: (selector (identifier) @function))",
                            "[\"class\" \"enum\" \"mixin\" \"extension\" \"void\" \"return\" \"if\" \"else\" \"for\" \"in\" \"while\" \"do\" \"switch\" \"case\" \"default\" \"break\" \"continue\" \"import\" \"export\" \"as\" \"show\" \"hide\" \"new\" \"try\" \"catch\" \"finally\" \"throw\" \"rethrow\" \"async\" \"await\" \"yield\" \"final\" \"const\" \"var\" \"late\" \"factory\" \"get\" \"set\" \"static\" \"this\" \"super\"] @keyword.control",
                            "[\"true\" \"false\" \"null\"] @boolean",
                            "[\"=\" \"==\" \"!=\" \"<\" \">\" \"<=\" \">=\" \"+\" \"-\" \"*\" \"/\" \"%\" \"~/=\" \"&&\" \"||\" \"!\" \"?\" \":\"] @operator",
                        ])),
                        _ => None,
                    };

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

                                let mut py_scopes = Vec::new();
                                if lang_name == "py" {
                                    for q_str in [
                                        "(function_definition parameters: (parameters) @params body: (_) @body)",
                                        "(lambda parameters: (lambda_parameters) @params body: (_) @body)"
                                    ] {
                                        if let Ok(func_query) = tree_sitter::Query::new(&lang, q_str) {
                                            let mut cursor = tree_sitter::QueryCursor::new();
                                            let mut matches = cursor.matches(&func_query, tree.root_node(), text.as_bytes());

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
                                                        | "enumerate" | "sum" | "min" | "max" => {
                                                            DRACULA_CYAN
                                                        }
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

                                                if lang_name == "py"
                                                    && node_text != "self"
                                                    && node_text != "cls"
                                                {
                                                    if matches!(
                                                        name,
                                                        "py_ident"
                                                            | "py_builtin_or_func"
                                                            | "py_assign"
                                                            | "parameter"
                                                    ) {
                                                        let mut is_param = false;
                                                        for scope in &py_scopes {
                                                            if cap.node.start_byte() >= scope.start
                                                                && cap.node.start_byte() < scope.end
                                                            {
                                                                if scope.params.contains(node_text)
                                                                {
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
                        if syntect_assets.is_none() {
                            let ps = syntect::parsing::SyntaxSet::load_defaults_newlines();
                            let ts = syntect::highlighting::ThemeSet::load_defaults();
                            syntect_assets = Some((ps, ts));
                        }

                        if let Some((ps, ts)) = syntect_assets.as_ref() {
                            let fallback_theme = &ts.themes["base16-ocean.dark"];
                            let syntax = ps.find_syntax_by_extension(&actual_ext).or_else(|| {
                                if actual_ext == "toml" {
                                    ps.find_syntax_by_extension("ini")
                                } else {
                                    None
                                }
                            });

                            if let Some(syntax) = syntax {
                                let mut h =
                                    syntect::easy::HighlightLines::new(syntax, fallback_theme);
                                let mut byte_offset = 0;
                                for line in syntect::util::LinesWithEndings::from(text.as_str()) {
                                    if let Ok(ranges) = h.highlight_line(line, &ps) {
                                        for (style, s) in ranges {
                                            let start = byte_offset;
                                            let end = start + s.len();
                                            spans.push(ColorSpan {
                                                start,
                                                end,
                                                color: [
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

                let flat_spans = flatten_spans(
                    spans,
                    text.len(),
                    text,
                    &mut byte_colors_buf,
                    error_ranges,
                    old_spans.clone(),
                    apply_rainbow_brackets,
                    is_log_or_huge,
                );

                old_spans = flat_spans.clone();

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

                let _ = tx_out.send((final_version, flat_spans, completions));
            }
        });
        Self {
            tx: tx_in,
            rx: rx_out,
            spans: vec![],
            completions: vec![],
            current_version: 0,
        }
    }

    pub fn reset(&self, version: u64, text: String, ext: String) {
        let _ = self
            .tx
            .send(HighlighterMessage::Reset { version, text, ext });
    }

    pub fn apply_edits(&self, version: u64, edits: Vec<SyncEdit>) {
        if !edits.is_empty() {
            let _ = self.tx.send(HighlighterMessage::Edits { version, edits });
        }
    }

    pub fn poll(&mut self, current_editor_version: u64) -> bool {
        let mut updated = false;
        while let Ok((ver, spans, completions)) = self.rx.try_recv() {
            if ver >= self.current_version {
                self.current_version = ver;
                if ver == current_editor_version {
                    self.spans = spans;
                    self.completions = completions;
                    updated = true;
                }
            }
        }
        updated
    }

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
                    if next.start <= current.end && next.color == current.color {
                        current.end = current.end.max(next.end);
                    } else if next.start >= current.end {
                        merged.push(current);
                        current = next.clone();
                    }
                }
                merged.push(current);
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
    is_log_or_huge: bool,
) -> Vec<ColorSpan> {
    if spans.is_empty()
        && old_spans.is_empty()
        && error_ranges.is_empty()
        && (is_log_or_huge || !apply_rainbow_brackets)
    {
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
