use super::Scope;
use crate::theme::SyntaxRole;

pub(super) fn resolve_role(
    name: &str,
    node_text: &str,
    start_byte: usize,
    param_scopes: &[Scope],
) -> SyntaxRole {
    let mut role = match name {
        "fg" | "property" | "field" | "py_assign" => SyntaxRole::Fg,
        "interpolation" => SyntaxRole::Interpolation,
        "text.title" => SyntaxRole::Constant,
        "text.literal" => SyntaxRole::String,
        "text.uri" => SyntaxRole::Keyword,
        "text.reference" => SyntaxRole::Function,
        "text.emphasis" => SyntaxRole::Parameter,
        "text.strong" => SyntaxRole::KeywordControl,
        "punctuation.special" | "punctuation.delimiter" => SyntaxRole::Comment,
        "string.escape" => SyntaxRole::KeywordControl,
        "none" => SyntaxRole::Fg,
        "string" => SyntaxRole::String,
        "comment" => SyntaxRole::Comment,
        "function" | "function.call" | "py_function" => SyntaxRole::Function,
        "keyword.control" | "keyword.operator" | "operator" | "boolean" | "conditional" => {
            SyntaxRole::KeywordControl
        }
        "keyword"
        | "subst"
        | "type"
        | "type.builtin"
        | "type.qualifier"
        | "storageclass"
        | "attribute"
        | "function.builtin" => SyntaxRole::Keyword,
        "class_name" => SyntaxRole::Class,
        "constant" => SyntaxRole::Constant,
        "parameter" => match node_text {
            "self" | "cls" => SyntaxRole::Constant,
            _ => SyntaxRole::Parameter,
        },
        "py_builtin_or_func" => match node_text {
            "print" | "input" | "id" | "dict" | "str" | "int" | "float" | "list" | "set"
            | "tuple" | "bool" | "super" | "len" | "type" | "dir" | "vars" | "hasattr"
            | "getattr" | "setattr" | "delattr" | "isinstance" | "issubclass" | "enumerate"
            | "zip" | "map" | "filter" | "sum" | "any" | "all" | "min" | "max" | "abs"
            | "round" | "open" => SyntaxRole::Keyword,
            _ => SyntaxRole::Function,
        },
        "py_ident" => match node_text {
            "Exception" | "ValueError" | "TypeError" | "KeyError" | "IndexError"
            | "AttributeError" | "RuntimeError" | "KeyboardInterrupt" | "int" | "float" | "str"
            | "bool" | "list" | "dict" | "set" | "tuple" | "bytes" | "Any" | "Optional"
            | "Union" | "Callable" | "Type" | "Dict" | "List" | "Set" | "Tuple" | "print"
            | "len" | "range" | "enumerate" | "sum" | "min" | "max" => SyntaxRole::Keyword,
            "self" | "cls" => SyntaxRole::Constant,
            _ => SyntaxRole::Fg,
        },
        "command_word" => match node_text {
            "sudo" | "sleep" | "ps" | "date" | "grep" | "awk" | "sed" | "cat" | "renice"
            | "ionice" | "systemctl" | "tee" | "tr" | "head" | "taskset" => SyntaxRole::Function,
            _ => SyntaxRole::Keyword,
        },
        "any_word" => {
            if node_text.starts_with('-') && node_text.len() > 1 {
                SyntaxRole::KeywordControl
            } else {
                SyntaxRole::Fg
            }
        }
        "variable" => SyntaxRole::Fg,
        "number" | "float" => SyntaxRole::Constant,
        _ => SyntaxRole::Fg,
    };

    if node_text == "None" {
        role = SyntaxRole::KeywordControl;
    }

    if node_text != "self"
        && node_text != "cls"
        && matches!(
            name,
            "py_ident" | "py_builtin_or_func" | "py_assign" | "parameter" | "variable" | "fg"
        )
        && param_scopes.iter().any(|scope| {
                start_byte >= scope.start
                    && start_byte < scope.end
                    && scope.params.contains(node_text)
            })
    {
        role = SyntaxRole::Parameter;
    }
    role
}

pub(super) fn capture_color_override(
    lang_name: &str,
    name: &str,
    node: tree_sitter::Node<'_>,
) -> Option<SyntaxRole> {
    (lang_name == "markdown_inline" && name == "text.literal" && node.kind() == "code_span")
        .then_some(SyntaxRole::MdCode)
}

pub(crate) fn hover_capture_role(name: &str, node_text: &str) -> SyntaxRole {
    resolve_role(name, node_text, 0, &[])
}
