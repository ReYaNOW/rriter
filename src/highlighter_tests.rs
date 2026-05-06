
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
fn tree_sitter_language_labels_follow_highlighter_extensions() {
    assert_eq!(tree_sitter_lang_name_for_ext("py"), "py");
    assert_eq!(tree_sitter_lang_name_for_ext("pyi"), "py");
    assert_eq!(tree_sitter_lang_name_for_ext("rs"), "rs");
    assert_eq!(tree_sitter_lang_name_for_ext("jsx"), "js");
    assert_eq!(tree_sitter_lang_name_for_ext("hpp"), "cpp");
    assert_eq!(tree_sitter_lang_name_for_ext("makefile"), "make");
    assert_eq!(tree_sitter_lang_name_for_ext("txt"), "");
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
    assert!(
        highlighter
            .spans
            .iter()
            .any(|span| span.color == DRACULA_PINK)
    );
    assert!(
        highlighter
            .completions
            .iter()
            .any(|item| item.word == "print")
    );
    assert_eq!(
        highlighter
            .completions
            .iter()
            .find(|item| item.word == "print")
            .map(|item| item.kind),
        Some(SymbolKind::Builtin)
    );
    assert!(
        highlighter
            .completions
            .iter()
            .any(|item| item.word == "name")
    );

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
    assert!(
        highlighter
            .spans
            .iter()
            .any(|span| span.color == DRACULA_COMMENT)
    );

    let cases = [
        (
            "rs",
            "fn main() { let value = Some(1); println!(\"{}\", value); }\n",
            "println!",
        ),
        ("dart", "void main() { print('hi'); }\n", "Widget"),
        (
            "js",
            "function run(value) { console.log(value); }\n",
            "Promise",
        ),
        (
            "cpp",
            "int main() { printf(\"hi\"); return 0; }\n",
            "printf",
        ),
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
fn highlighter_keeps_python_class_fields_plain_fg() {
    let mut highlighter = Highlighter::new();
    let source = "class BoxReadPublic(BasedStruct, kw_only=True):\n    id: int\n    active: bool = True\n    created_at: dt.datetime\n";

    highlighter.reset(1, source.to_string(), "py".to_string());
    wait(&mut highlighter, 1);

    for name in ["id", "active", "created_at"] {
        let start = source.find(name).unwrap();
        let end = start + name.len();
        assert!(
            !highlighter.spans.iter().any(|span| {
                span.start < end && span.end > start && span.color == DRACULA_ORANGE
            }),
            "{name} must not be parameter-orange in class body"
        );
    }
}

#[test]
fn ast_select_expand_uses_tree_sitter_then_grows_to_parent() {
    let source = "fn main() {\n    let value = call(1);\n}\n";
    let cursor = source.find("value").unwrap() + 2;

    let (start, end) = ast_select_expand_range(source, "rs", cursor, None).unwrap();
    assert_eq!(&source[start..end], "value");

    let (start, end) = ast_select_expand_range(source, "rs", end, Some(start)).unwrap();
    assert!(source[start..end].starts_with("let value = call(1)"));

    assert!(ast_select_expand_range(source, "txt", cursor, None).is_none());
}

#[test]
fn ctrl_w_ast_expand_keeps_python_keyword_argument_line_before_argument_list() {
    let source = "app.state.pool = await asyncpg.create_pool(\n        config.database_url,\n        max_size=40,\n        command_timeout=60,\n        init=init_connection,\n    )\n";
    let cursor = source.find("command_timeout").unwrap() + "command".len();

    let (start, end) = ast_select_expand_range(source, "py", cursor, None).unwrap();
    assert_eq!(&source[start..end], "command_timeout");

    let (start, end) = ast_select_expand_range(source, "py", end, Some(start)).unwrap();
    assert_eq!(&source[start..end], "command_timeout=60");

    let (start, end) = ast_select_expand_range(source, "py", end, Some(start)).unwrap();
    assert_eq!(&source[start..end], "        command_timeout=60,");

    let (start, end) = ast_select_expand_range(source, "py", end, Some(start)).unwrap();
    assert!(source[start..end].contains("config.database_url"));
    assert!(source[start..end].contains("init=init_connection"));
}

#[test]
fn highlighter_keeps_fold_map_after_far_incremental_edit() {
    let mut highlighter = Highlighter::new();
    let filler = "# pad\n".repeat(260);
    let source = format!("items = [\n    1,\n    2,\n]\n{filler}tail = 1\n");

    highlighter.reset(1, source.clone(), "py".to_string());
    wait(&mut highlighter, 1);

    let list_start = source.find('[').unwrap();
    assert!(
        highlighter
            .foldable_ranges
            .iter()
            .any(|(start, end, is_autofold, _)| {
                *is_autofold && *start == list_start && *end > list_start
            }),
        "initial list fold missing"
    );

    let delete_offset = source.rfind("tail").unwrap();
    highlighter.apply_edits(
        2,
        vec![SyncEdit::Delete {
            offset: delete_offset,
            len: 1,
        }],
        Some(delete_offset),
        Some(delete_offset),
    );
    wait(&mut highlighter, 2);

    assert!(
        highlighter
            .foldable_ranges
            .iter()
            .any(|(start, end, is_autofold, _)| {
                *is_autofold && *start == list_start && *end > list_start
            }),
        "far edit must not drop existing fold ranges"
    );
}

#[test]
fn highlighter_keeps_self_attribute_plain_but_parameter_orange() {
    let mut highlighter = Highlighter::new();
    let source = "class KnownDBError(DefaultHttpException):\n    def __init__(self, msg: str | None = None):\n        self.msg = msg\n";

    highlighter.reset(1, source.to_string(), "py".to_string());
    wait(&mut highlighter, 1);

    let attr_start = source.find("self.msg").unwrap() + "self.".len();
    let attr_end = attr_start + "msg".len();
    assert!(
        !highlighter.spans.iter().any(|span| {
            span.start < attr_end && span.end > attr_start && span.color == DRACULA_ORANGE
        }),
        "self.msg attribute name must stay plain"
    );

    let param_start = source.find("msg: str").unwrap();
    let param_end = param_start + "msg".len();
    assert!(
        highlighter.spans.iter().any(|span| {
            span.start <= param_start && span.end >= param_end && span.color == DRACULA_ORANGE
        }),
        "bare parameter msg must stay orange"
    );
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
    assert!(
        highlighter
            .completions
            .iter()
            .any(|item| item.word == "print")
    );

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
