use super::*;
use std::time::Duration;

fn wait(highlighter: &mut Highlighter, version: u64) {
    assert!(
        highlighter.wait_for_first_result(version, Duration::from_secs(2)),
        "highlighter did not produce version {version}"
    );
    assert_eq!(highlighter.current_version, version);
}

fn color_at(highlighter: &Highlighter, offset: usize) -> [f32; 4] {
    highlighter
        .spans
        .iter()
        .find(|span| span.start <= offset && offset < span.end)
        .map(|span| span.color)
        .unwrap_or(DRACULA_FG)
}

#[test]
fn highlighter_drop_cancels_worker_without_leaving_background_work() {
    let baseline = active_highlighter_worker_count();
    let text = "fn pending() { let value = 1; }\n".repeat(4_000);

    for version in 1..=16 {
        let mut highlighter = Highlighter::new();
        let start_deadline = std::time::Instant::now() + Duration::from_secs(1);
        while active_highlighter_worker_count() == baseline
            && std::time::Instant::now() < start_deadline
        {
            std::thread::yield_now();
        }
        assert_eq!(active_highlighter_worker_count(), baseline + 1);

        highlighter.reset(version, text.clone(), "rs".to_string(), 0);
        drop(highlighter);

        let stop_deadline = std::time::Instant::now() + Duration::from_secs(1);
        while active_highlighter_worker_count() != baseline
            && std::time::Instant::now() < stop_deadline
        {
            std::thread::yield_now();
        }
        assert_eq!(active_highlighter_worker_count(), baseline);
    }

    let mut highlighter = Highlighter::new();
    highlighter.reset(100, "fn ready() {}\n".to_string(), "rs".to_string(), 0);
    wait(&mut highlighter, 100);
}

#[test]
fn tree_sitter_language_labels_follow_highlighter_extensions() {
    assert_eq!(tree_sitter_lang_name_for_ext("py"), "py");
    assert_eq!(tree_sitter_lang_name_for_ext("pyi"), "py");
    assert_eq!(tree_sitter_lang_name_for_ext("rs"), "rs");
    assert_eq!(tree_sitter_lang_name_for_ext("jsx"), "js");
    assert_eq!(tree_sitter_lang_name_for_ext("hpp"), "cpp");
    assert_eq!(tree_sitter_lang_name_for_ext("makefile"), "make");
    assert_eq!(tree_sitter_lang_name_for_ext("sql"), "sql");
    assert_eq!(tree_sitter_lang_name_for_ext("md"), "md");
    assert_eq!(tree_sitter_lang_name_for_ext("markdown"), "md");
    assert_eq!(tree_sitter_lang_name_for_ext("txt"), "");
}

#[test]
fn markdown_injection_aliases_reuse_existing_rriter_languages() {
    let cases = [
        ("rust", "rs"),
        ("rs", "rs"),
        ("python", "py"),
        ("py", "py"),
        ("shell", "bash"),
        ("sh", "bash"),
        ("bash", "bash"),
        ("javascript", "js"),
        ("js", "js"),
        ("typescript", "ts"),
        ("ts", "ts"),
        ("tsx", "tsx"),
        ("html", "html"),
        ("css", "css"),
        ("json", "json"),
        ("toml", "toml"),
        ("go", "go"),
        ("java", "java"),
        ("csharp", "cs"),
        ("cs", "cs"),
        ("dart", "dart"),
        ("c", "c"),
        ("cpp", "cpp"),
        ("c++", "cpp"),
        ("sql", "sql"),
        ("make", "make"),
        ("makefile", "make"),
        ("regex", "regex"),
        ("markdown", "md"),
        ("md", "md"),
        ("markdown_inline", "markdown_inline"),
    ];
    for (input, expected) in cases {
        assert_eq!(normalize_injection_language(input), Some(expected));
    }
    assert_eq!(normalize_injection_language("yaml"), None);
    assert_eq!(normalize_injection_language("latex"), None);
}

#[test]
fn markdown_edit_highlighting_covers_blocks_inline_unicode_and_fenced_injections() {
    let source = concat!(
        "# H1\n## H2\n### H3\n\n",
        "Setext\n------\n\n",
        "> цитата 👋\n\n",
        "- item\n- [ ] todo\n- [x] done\n1. ordered\n\n",
        "---\n\n",
        "| a | b |\n| :- | -: |\n| 1 | 2 |\n\n",
        "Текст *em* **strong** `inline` [link](https://example.invalid) [titled](https://example.invalid \"title\") ![alt](img).\n\n",
        "```rust\nfn main() { let value = \"rust\"; }\n```\n\n",
        "```python\ndef answer():\n    return 42\n```\n\n",
        "```unknown-lang\nplain_code()\n```\n",
    );
    let mut highlighter = Highlighter::new();
    highlighter.reset(1, source.to_string(), "md".to_string(), 0);
    wait(&mut highlighter, 1);

    let h1 = source.find("H1").unwrap();
    let em = source.find("*em*").unwrap() + 1;
    let strong = source.find("strong").unwrap();
    let inline = source.find("inline").unwrap();
    let link = source.find("link").unwrap();
    let uri = source.find("https://").unwrap();
    let link_title = source.find("\"title\"").unwrap() + 1;
    let rust_fn = source.find("fn main").unwrap();
    let rust_name = source.find("main()").unwrap();
    let rust_string = source.find("\"rust\"").unwrap() + 1;
    let python_def = source.find("def answer").unwrap();
    let python_name = source.find("answer()").unwrap();
    let unknown = source.find("plain_code").unwrap();

    assert_eq!(color_at(&highlighter, h1), DRACULA_PURPLE);
    assert_eq!(color_at(&highlighter, em), DRACULA_ORANGE);
    assert_eq!(color_at(&highlighter, strong), DRACULA_PINK);
    assert_eq!(color_at(&highlighter, inline), DRACULA_GREEN);
    assert_eq!(color_at(&highlighter, link), DRACULA_GREEN);
    assert_eq!(color_at(&highlighter, uri), DRACULA_CYAN);
    assert_eq!(color_at(&highlighter, link_title), DRACULA_YELLOW);
    assert_eq!(color_at(&highlighter, rust_fn), DRACULA_PINK);
    assert_eq!(color_at(&highlighter, rust_name), DRACULA_GREEN);
    assert_eq!(color_at(&highlighter, rust_string), DRACULA_YELLOW);
    assert_eq!(color_at(&highlighter, python_def), DRACULA_PINK);
    assert_eq!(color_at(&highlighter, python_name), DRACULA_GREEN);
    assert_eq!(color_at(&highlighter, unknown), DRACULA_FG);
}

#[test]
fn markdown_edit_highlighting_colors_inline_code_and_fenced_bash_by_context() {
    let source = concat!(
        "* `handle_main_mouse_input`\n",
        "* `src/render_view/editor_text_layer.rs`\n",
        "* `start_active_api_request`\n",
        "* `query_graph_tool`\n\n",
        "```bash\n",
        "code-review-graph build --skip-postprocess\n",
        "```\n",
    );
    let mut highlighter = Highlighter::new();
    highlighter.reset(1, source.to_string(), "md".to_string(), 0);
    wait(&mut highlighter, 1);

    for inline in [
        "handle_main_mouse_input",
        "src/render_view/editor_text_layer.rs",
        "start_active_api_request",
        "query_graph_tool",
    ] {
        assert_eq!(
            color_at(&highlighter, source.find(inline).unwrap()),
            DRACULA_GREEN,
            "inline code should be green: {inline}"
        );
    }

    assert_eq!(
        color_at(&highlighter, source.find('`').unwrap()),
        DRACULA_COMMENT
    );
    assert_eq!(
        color_at(&highlighter, source.find("code-review-graph").unwrap()),
        DRACULA_GREEN
    );
    assert_eq!(
        color_at(&highlighter, source.find("build --").unwrap()),
        DRACULA_YELLOW
    );
    assert_eq!(
        color_at(&highlighter, source.find("--skip-postprocess").unwrap()),
        DRACULA_PURPLE
    );
}

#[test]
fn standalone_bash_keeps_existing_palette_outside_markdown_injection() {
    let source = "code-review-graph build --skip-postprocess\n";
    let mut highlighter = Highlighter::new();
    highlighter.reset(1, source.to_string(), "sh".to_string(), 0);
    wait(&mut highlighter, 1);

    assert_eq!(
        color_at(&highlighter, source.find("code-review-graph").unwrap()),
        DRACULA_CYAN
    );
    assert_eq!(
        color_at(&highlighter, source.find("build").unwrap()),
        DRACULA_FG
    );
    assert_eq!(
        color_at(&highlighter, source.find("--skip-postprocess").unwrap()),
        DRACULA_PURPLE
    );
}

#[test]
fn markdown_incremental_edits_refresh_backtick_and_fence_injection_colors() {
    let inline_source = "`handle_main_mouse_input`\n";
    let mut inline_highlighter = Highlighter::new();
    inline_highlighter.reset(1, inline_source.to_string(), "md".to_string(), 0);
    wait(&mut inline_highlighter, 1);
    assert_eq!(color_at(&inline_highlighter, 1), DRACULA_GREEN);

    inline_highlighter.apply_edits(
        2,
        vec![SyncEdit::Delete { offset: 0, len: 1 }],
        Some(0),
        Some(0),
    );
    wait(&mut inline_highlighter, 2);
    assert_ne!(color_at(&inline_highlighter, 0), DRACULA_GREEN);

    let fenced_source = "```bash\ncode-review-graph build --skip-postprocess\n```\n";
    let mut fenced_highlighter = Highlighter::new();
    fenced_highlighter.reset(1, fenced_source.to_string(), "md".to_string(), 0);
    wait(&mut fenced_highlighter, 1);
    let command = fenced_source.find("code-review-graph").unwrap();
    assert_eq!(color_at(&fenced_highlighter, command), DRACULA_GREEN);

    let language = fenced_source.find("bash").unwrap();
    fenced_highlighter.apply_edits(
        2,
        vec![SyncEdit::Delete {
            offset: language,
            len: "bash".len(),
        }],
        Some(language),
        Some(language),
    );
    wait(&mut fenced_highlighter, 2);
    assert_ne!(
        color_at(&fenced_highlighter, command - "bash".len()),
        DRACULA_GREEN
    );
}

#[test]
fn markdown_highlighter_incremental_edit_reparses_new_structure() {
    let mut highlighter = Highlighter::new();
    highlighter.reset(1, "plain\n".to_string(), "md".to_string(), 0);
    wait(&mut highlighter, 1);
    assert_eq!(color_at(&highlighter, 0), DRACULA_FG);

    highlighter.apply_edits(
        2,
        vec![SyncEdit::Insert {
            offset: 0,
            text: "# ".to_string(),
        }],
        Some(0),
        Some(2),
    );
    wait(&mut highlighter, 2);
    assert_eq!(color_at(&highlighter, 2), DRACULA_PURPLE);
}

#[test]
fn markdown_link_punctuation_is_not_recolored_by_rainbow_brackets() {
    assert!(!should_apply_rainbow_brackets("md"));
    assert!(!should_apply_rainbow_brackets("markdown_inline"));
    assert!(should_apply_rainbow_brackets("rs"));

    let source = "[link](https://example.invalid)\n";
    let mut highlighter = Highlighter::new();
    highlighter.reset(1, source.to_string(), "md".to_string(), 0);
    wait(&mut highlighter, 1);
    assert_eq!(color_at(&highlighter, source.find('[').unwrap()), DRACULA_COMMENT);
    assert_eq!(color_at(&highlighter, source.find('(').unwrap()), DRACULA_COMMENT);
}

#[test]
fn sql_highlighter_colors_postgresql_and_injects_core_completions() {
    let mut highlighter = Highlighter::new();
    highlighter.reset(
        1,
        "SELECT jsonb_build_object('id', id) FROM users WHERE active = TRUE;".to_string(),
        "sql".to_string(),
        0,
    );
    wait(&mut highlighter, 1);

    assert!(
        highlighter
            .spans
            .iter()
            .any(|span| span.color == DRACULA_CYAN || span.color == DRACULA_PINK)
    );
    assert!(
        highlighter
            .completions
            .iter()
            .any(|item| item.word == "SELECT" && item.kind == SymbolKind::Keyword)
    );
    assert!(
        highlighter
            .completions
            .iter()
            .any(|item| item.word == "jsonb_build_object" && item.kind == SymbolKind::Builtin)
    );
}

#[test]
fn highlighter_thread_resets_parses_edits_and_injects_language_builtins() {
    let mut highlighter = Highlighter::new();

    highlighter.reset(
        1,
        "def greet(name):\n    return f'hi {name}'\n".to_string(),
        "py".to_string(),
        0,
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
        highlighter.reset(version, (*text).to_string(), (*ext).to_string(), 0);
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

    highlighter.reset(1, source.to_string(), "py".to_string(), 0);
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

    highlighter.reset(1, source.clone(), "py".to_string(), 0);
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

    highlighter.reset(1, source.to_string(), "py".to_string(), 0);
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
    assert!(
        !highlighter
            .completions
            .iter()
            .any(|item| item.word == "str" && item.kind == SymbolKind::Variable),
        "annotation type str must not shadow builtin as variable"
    );
    assert!(
        highlighter.completions.iter().any(|item| item.word == "str"
            && matches!(item.kind, SymbolKind::Class | SymbolKind::Builtin)),
        "annotation type str must stay class/builtin completion"
    );
}

#[test]
fn highlighter_keeps_typed_python_parameters_as_parameters() {
    let mut highlighter = Highlighter::new();
    let source = "class BookingService:\n    def __init__(\n        self,\n        session: AnnDBSession,\n    ):\n        super().__init__(session)\n        self.session = session\n";

    highlighter.reset(1, source.to_string(), "py".to_string(), 0);
    wait(&mut highlighter, 1);

    assert!(
        highlighter
            .completions
            .iter()
            .any(|item| item.word == "self" && item.kind == SymbolKind::Parameter),
        "self must stay a parameter completion"
    );
    assert!(
        highlighter
            .completions
            .iter()
            .any(|item| item.word == "session" && item.kind == SymbolKind::Parameter),
        "typed session must stay a parameter completion"
    );
}

#[test]
fn highlighter_thread_handles_shebang_log_and_invalid_incremental_edit() {
    let mut highlighter = Highlighter::new();

    highlighter.reset(
        1,
        "#!/usr/bin/env python\nprint(None)\n".to_string(),
        String::new(),
        0,
    );
    wait(&mut highlighter, 1);
    assert!(
        highlighter
            .completions
            .iter()
            .any(|item| item.word == "print")
    );

    highlighter.reset(2, "plain log text\n".to_string(), "log".to_string(), 0);
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
