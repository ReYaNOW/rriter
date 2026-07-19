use super::*;

fn span_fragments<'a>(text: &'a str, ranges: &[(usize, usize)]) -> Vec<&'a str> {
    ranges.iter().map(|&(s, e)| &text[s..e]).collect()
}

fn has_color_span(
    spans: &[crate::highlighter::ColorSpan],
    start: usize,
    end: usize,
    color: [f32; 4],
) -> bool {
    spans
        .iter()
        .any(|span| span.start <= start && span.end >= end && span.color == color)
}

#[test]
fn rst_inline_roles_params_and_wrapped_text_cover_edge_cases() {
    let (line, ranges) = normalize_inline_rst_code("Use ``path`` and ``broken");
    assert_eq!(line, "Use path and ``broken");
    assert_eq!(span_fragments(&line, &ranges), vec!["path"]);

    let (role_line, role_ranges) = normalize_rst_roles(
        "See :class:`~pkg.Type`, :meth:`display <pkg.func>`, and :data:`value`.",
    );
    assert_eq!(role_line, "See pkg.Type, display, and value.");
    assert_eq!(
        span_fragments(&role_line, &role_ranges),
        vec!["pkg.Type", "display", "value"]
    );

    let flat =
        flatten_rst_roles_and_code(":class:`pkg.\n    Thing` and ``multi\n    code`` \\\n next");
    assert_eq!(flat, ":class:`pkg. Thing` and ``multi code`` next");

    assert_eq!(
        parse_param_line(":param list\\* items: desc"),
        Some(("items".to_string(), "list*".to_string(), "desc".to_string()))
    );
    assert_eq!(parse_param_line(":param : missing head"), None);
    assert_eq!(parse_param_line("param value: missing marker"), None);
}

#[test]
fn normalize_python_hover_doc_covers_rst_blocks_headers_and_inline_code() {
    let raw = "Intro with :class:`~pkg.Type` and ``code``.\n\
\n\
--------\n\
.. warning::\n\
Arguments:\n\
:param int count: number of :obj:`items`\n\
:return: :class:`Result` value\n\
.. versionchanged:: 2.0 added :func:`go`\n\
Example::\n\
\n\
    for i in range(2):\n\
        print(i)\n\
\n\
.. code-block:: python\n\
\n\
    await call(name=\"x\")\n\
\n\
Note: return make(x=1)\n";

    let (out, kinds, inline_ranges) = normalize_python_hover_doc(raw);

    assert!(out.contains("Intro with pkg.Type and code."));
    assert!(out.contains("---\nWarning\nParameters"));
    assert!(out.contains("count: int\n    number of items"));
    assert!(out.contains("Returns\nResult value"));
    assert!(out.contains("versionchanged\n2.0 added go"));
    assert!(out.contains("Example:"));
    assert!(out.contains("for i in range(2):"));
    assert!(out.contains("print(i)"));
    assert!(out.contains("await call(name=\"x\")"));
    assert!(out.contains("Note:\n    return make(x=1)"));

    assert!(kinds.iter().any(|k| *k == HoverLineKind::Separator));
    assert!(kinds.iter().any(|k| *k == HoverLineKind::Header1));
    assert!(kinds.iter().any(|k| *k == HoverLineKind::Header2));
    assert!(kinds.iter().any(|k| *k == HoverLineKind::Code));

    let fragments = span_fragments(&out, &inline_ranges);
    assert!(fragments.contains(&"pkg.Type"));
    assert!(fragments.contains(&"code"));
    assert!(fragments.contains(&"items"));
    assert!(fragments.contains(&"Result"));
    assert!(fragments.contains(&"go"));
}

#[test]
fn parameter_header_splits_module_and_owner_like_attributes() {
    let raw = "## Parameter self of car_wash.core.fcm.service.FcmSenderService.__init__\nself: FcmSenderService";
    let (out, kinds, _) = normalize_python_hover_doc(raw);
    assert_eq!(
        out,
        "[[MODULE]] car_wash.core.fcm.service\nParameter self of FcmSenderService.__init__\n---\nself: FcmSenderService"
    );
    assert!(matches!(kinds[1], HoverLineKind::Text));
    assert!(matches!(kinds[2], HoverLineKind::Separator));
}

#[test]
fn highlight_python_hover_doc_colors_attrs_params_inline_code_and_keyword_args() {
    let raw = "@decorator(mode=\"fast\")\n\
class Box[T](Base):\n\
---\n\
Class attribute field of pkg.Mod\n\
field: int\n\
Inline ``call(name=1)`` text\n";

    let (msg, spans, kinds, inline_ranges) = highlight_python_hover_doc(raw);

    assert!(msg.contains("@decorator"));
    assert!(msg.contains("class Box[T](Base):"));
    assert!(msg.contains("[[MODULE]] pkg"));
    assert!(msg.contains("Class attribute field of Mod"));
    assert!(kinds.iter().any(|k| *k == HoverLineKindPublic::Separator));
    assert_eq!(span_fragments(&msg, &inline_ranges), vec!["call(name=1)"]);

    let class_start = msg.find("class").unwrap();
    assert!(has_color_span(
        &spans,
        class_start,
        class_start + "class".len(),
        crate::highlighter::DRACULA_PINK,
    ));

    let box_start = msg.find("Box").unwrap();
    assert!(has_color_span(
        &spans,
        box_start,
        box_start + "Box".len(),
        crate::highlighter::DRACULA_CYAN,
    ));

    let attr_name_start = msg.find("field of").unwrap();
    assert!(has_color_span(
        &spans,
        attr_name_start,
        attr_name_start + "field".len(),
        crate::highlighter::DRACULA_PINK,
    ));

    let attr_type_start = msg.find("of Mod").unwrap() + "of ".len();
    assert!(has_color_span(
        &spans,
        attr_type_start,
        attr_type_start + "Mod".len(),
        crate::highlighter::DRACULA_CYAN,
    ));

    let param_start = msg.find("field: int").unwrap();
    assert!(has_color_span(
        &spans,
        param_start,
        param_start + "field".len(),
        crate::highlighter::DRACULA_ORANGE,
    ));

    let inline_arg_start = msg.find("name=1").unwrap();
    assert!(has_color_span(
        &spans,
        inline_arg_start,
        inline_arg_start + "name".len(),
        crate::highlighter::DRACULA_ORANGE,
    ));
}

#[test]
fn color_helpers_cover_capture_map_span_forcing_and_keyword_arg_edges() {
    assert_eq!(
        ts_capture_color("keyword.control"),
        Some([1.0, 0.474, 0.776, 1.0])
    );
    assert_eq!(
        ts_capture_color("py_builtin_or_func"),
        Some([0.313, 0.980, 0.482, 1.0])
    );
    assert_eq!(ts_capture_color("missing.capture"), None);

    let mut forced = vec![crate::highlighter::ColorSpan {
        start: 0,
        end: 10,
        color: [1.0, 0.0, 0.0, 1.0],
    }];
    force_color_on_ranges(&mut forced, &[(3, 6)], [0.0, 1.0, 0.0, 1.0]);
    assert!(has_color_span(&forced, 0, 3, [1.0, 0.0, 0.0, 1.0]));
    assert!(has_color_span(&forced, 6, 10, [1.0, 0.0, 0.0, 1.0]));
    assert!(has_color_span(&forced, 3, 6, [0.0, 1.0, 0.0, 1.0]));

    let code = "call(alpha=1, beta == 2, gamma!=3, delta='x=y', ε=3)";
    let mut spans = Vec::new();
    color_keyword_args_orange(code, 100, &mut spans);

    for name in ["alpha", "delta", "ε"] {
        let start = 100 + code.find(name).unwrap();
        assert!(has_color_span(
            &spans,
            start,
            start + name.len(),
            crate::highlighter::DRACULA_ORANGE,
        ));
    }

    for name in ["beta", "gamma"] {
        let start = 100 + code.find(name).unwrap();
        assert!(!has_color_span(
            &spans,
            start,
            start + name.len(),
            crate::highlighter::DRACULA_ORANGE,
        ));
    }

    let mut ts_spans = Vec::new();
    push_python_ts_spans(
        "class C:\n    def f(self, x=1):\n        return x\n",
        7,
        &mut ts_spans,
    );
    assert!(!ts_spans.is_empty());
    assert!(
        ts_spans
            .iter()
            .all(|span| span.start >= 7 && span.end > span.start)
    );

    let mut class_attr_spans = Vec::new();
    let class_code = "class BoxReadPublic(BasedStruct, kw_only=True):\n    id: int\n    created_at: dt.datetime\n";
    push_python_ts_spans(class_code, 0, &mut class_attr_spans);
    for name in ["id", "created_at"] {
        let start = class_code.find(name).unwrap();
        assert!(has_color_span(
            &class_attr_spans,
            start,
            start + name.len(),
            crate::highlighter::DRACULA_FG,
        ));
        assert!(!has_color_span(
            &class_attr_spans,
            start,
            start + name.len(),
            crate::highlighter::DRACULA_ORANGE,
        ));
    }
}

#[test]
fn python_import_blocks_cover_from_and_import_groups() {
    let text = "from os import path\nimport sys\n\n\ndef f():\n    pass\n";
    let blocks = import_blocks(text);
    assert_eq!(blocks.len(), 1);
    assert_eq!(
        &text[blocks[0].keyword_start..blocks[0].keyword_end],
        "from"
    );
    assert_eq!(blocks[0].line_count, 2);
}

#[test]
fn python_import_blocks_keep_blank_lines_between_groups_only() {
    let text = "import time\nimport typing\n\nimport msgspec\nfrom sqlalchemy import inspect\n\n\ndef f():\n    pass\n";
    let blocks = import_blocks(text);
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].line_count, 5);
    assert_eq!(
        &text[blocks[0].start..blocks[0].end],
        "import time\nimport typing\n\nimport msgspec\nfrom sqlalchemy import inspect"
    );
}

#[test]
fn python_import_blocks_ignore_comment_delimiters_and_continuations() {
    let comment = "from pkg import value  # (\ndef f():\n    pass\n";
    assert!(import_blocks(comment).is_empty());

    let explicit = r#"from pkg import value  # keep \
def f():
    pass
"#;
    assert!(import_blocks(explicit).is_empty());
}

#[test]
fn python_class_attribute_highlighting_uses_actual_body_indent() {
    let code = "class TwoSpace:\n  value: int\n\nclass Tabbed:\n\tother = 1\n";
    let ranges = python_class_attr_name_ranges(code);

    for name in ["value", "other"] {
        let start = code.find(name).expect("expected class attribute");
        assert!(ranges.contains(&(start, start + name.len())));
    }
}

#[test]
fn python_docstring_spans_color_text_header_and_inline_code() {
    let text = "def f():\n    \"\"\"Args:\n    value: use ``int``.\n    \"\"\"\n";
    let start = text.find("\"\"\"").unwrap();
    let end = text.rfind("\"\"\"").unwrap() + 3;
    let mut spans = Vec::new();
    push_docstring_highlight_spans(text, start, end, &mut spans);
    let doc_text = text.find("value: use").unwrap();
    assert!(
        spans
            .iter()
            .any(|s| s.color == DOCSTRING_TEXT && s.start <= doc_text && s.end > doc_text)
    );
    assert!(
        spans
            .iter()
            .any(|s| s.color == crate::highlighter::DRACULA_CYAN)
    );
    let inline_code = text.find("int").unwrap();
    assert!(spans.iter().any(|s| {
        s.color == crate::highlighter::DRACULA_CYAN
            && s.start <= inline_code
            && s.end >= inline_code + "int".len()
    }));
}
