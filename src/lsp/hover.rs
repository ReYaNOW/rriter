use tree_sitter::StreamingIterator;

pub fn highlight_hover_text(
    msg: &str,
) -> (
    String,
    Vec<crate::highlighter::ColorSpan>,
    Vec<HoverLineKindPublic>,
    Vec<(usize, usize)>,
) {
    let preprocessed = preprocess_hover_text(msg);
    if preprocessed.contains(":param ") || looks_like_python_hover(&preprocessed) {
        let (clean_msg, mut spans, line_kinds, inline_code_ranges) =
            crate::languages::python::highlight_python_hover_doc(&preprocessed);
        add_doc_arg_name_spans(&clean_msg, &mut spans);
        spans.sort_unstable_by(|a, b| a.start.cmp(&b.start).then_with(|| b.end.cmp(&a.end)));
        let spans = crate::highlighter::flatten_color_spans_prefer_specific(spans, clean_msg.len());
        return (clean_msg, spans, line_kinds, inline_code_ranges);
    }
    let (clean_msg, inline_code_ranges) = normalize_hover_text(&preprocessed);
    let mut spans = Vec::new();

    crate::languages::python::TS_DIAG_PARSER.with(|p_cell| {
        crate::languages::python::TS_DIAG_QUERY.with(|q_cell| {
            crate::languages::python::TS_DIAG_CURSOR.with(|c_cell| {
                let mut parser = p_cell.borrow_mut();
                let query_opt = q_cell.borrow();
                let mut cursor = c_cell.borrow_mut();

                if let Some(query) = query_opt.as_ref() {
                    let mut offset = 0usize;
                    let lines: Vec<&str> = clean_msg.lines().collect();
                    for (line_idx, line) in lines.iter().enumerate() {
                        if add_bound_method_signature_spans(line, offset, &mut spans) {
                            offset += line.len() + 1;
                            continue;
                        }
                        if looks_like_python_code_line(line) {
                            let mut parse_line_owned = String::new();
                            let parse_line = if (line.trim_start().starts_with("def ")
                                || line.trim_start().starts_with("async def ")
                                || line.trim_start().starts_with("class "))
                                && !line.trim_end().ends_with(':')
                            {
                                parse_line_owned.push_str(line);
                                parse_line_owned.push(':');
                                parse_line_owned.as_str()
                            } else {
                                line
                            };
                            if let Some(tree) = parser.parse(parse_line, None) {
                                let mut matches =
                                    cursor.matches(query, tree.root_node(), parse_line.as_bytes());
                                while let Some(m) = matches.next() {
                                    for cap in m.captures {
                                        let name = query.capture_names()[cap.index as usize];
                                        let color = match name {
                                            "property" | "variable" => [0.972, 0.972, 0.949, 1.0],
                                            "string" => [0.945, 0.980, 0.549, 1.0],
                                            "type" | "class_name" => [0.545, 0.913, 0.992, 1.0],
                                            "keyword.control" | "keyword" | "operator" => {
                                                [1.0, 0.474, 0.776, 1.0]
                                            }
                                            "function" | "py_function" | "py_builtin_or_func" => {
                                                [0.313, 0.980, 0.482, 1.0]
                                            }
                                            "number" => [0.741, 0.576, 0.976, 1.0],
                                            "comment" => [0.384, 0.447, 0.643, 1.0],
                                            _ => continue,
                                        };
                                        spans.push(crate::highlighter::ColorSpan {
                                            start: offset + cap.node.start_byte(),
                                            end: offset + cap.node.end_byte(),
                                            color,
                                        });
                                    }
                                }
                            }
                            add_param_name_spans_for_signature(line, offset, &mut spans);
                            add_self_param_span_for_signature(line, offset, &mut spans);
                            add_type_bracket_neutral_spans_for_signature(line, offset, &mut spans);
                        } else if looks_like_type_expr_line(line) {
                            add_type_expr_spans_for_line(line, offset, &mut spans);
                        } else if looks_like_simple_type_name_line(
                            line,
                            lines.get(line_idx + 1).copied(),
                        ) {
                            add_type_expr_spans_for_line(line, offset, &mut spans);
                        }
                        add_class_keyword_spans_for_signature(line, offset, &mut spans);
                        offset += line.len() + 1;
                    }
                }
            })
        })
    });

    for &(start, end) in &inline_code_ranges {
        if end > start && end <= clean_msg.len() {
            let code_chunk = &clean_msg[start..end];
            crate::languages::python::push_python_ts_spans(code_chunk, start, &mut spans);
        }
    }

    add_doc_arg_name_spans(&clean_msg, &mut spans);
    spans.sort_unstable_by(|a, b| a.start.cmp(&b.start).then_with(|| b.end.cmp(&a.end)));
    let spans = crate::highlighter::flatten_color_spans_prefer_specific(spans, clean_msg.len());
    let line_kinds = clean_msg
        .split('\n')
        .map(|line| {
            if line.starts_with("## ") {
                HoverLineKindPublic::Header2
            } else if line.starts_with("# ") {
                HoverLineKindPublic::Header1
            } else {
                HoverLineKindPublic::Text
            }
        })
        .collect();
    (clean_msg, spans, line_kinds, inline_code_ranges)
}

fn preprocess_hover_text(msg: &str) -> String {
    let mut out = String::new();
    let cleaned = msg
        .replace('\r', "")
        .replace('\u{a0}', " ")
        .replace('\u{200b}', "");
    for line in cleaned.lines() {
        if let Some(normalized) = normalize_asynccontextmanager_signature_line(line.trim()) {
            out.push_str(&normalized);
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out.trim_end().to_string()
}

fn normalize_special_form_repr(line: &str) -> Option<String> {
    let unquoted = line.trim().trim_matches('`').trim();
    let body = unquoted
        .strip_prefix("<special-form '")?
        .strip_suffix("'>")?
        .trim();
    if body.is_empty() {
        return None;
    }
    let body = body.strip_prefix("typing.").unwrap_or(body);
    Some(body.replace("<metadata>", "..."))
}

fn looks_like_python_hover(msg: &str) -> bool {
    if msg.contains("```python") || msg.contains("```py\n") {
        return true;
    }
    let mut non_empty = msg
        .lines()
        .map(str::trim_start)
        .filter(|line| !line.is_empty());
    let Some(first_non_empty) = non_empty.next() else {
        return false;
    };
    if first_non_empty.starts_with("def ")
        || first_non_empty.starts_with("async def ")
        || first_non_empty.starts_with("class ")
        || first_non_empty.starts_with("## ")
    {
        return true;
    }
    if first_non_empty.starts_with('@')
        && (first_non_empty.contains(" def ") || first_non_empty.contains(" async def "))
    {
        return true;
    }
    if first_non_empty.starts_with('@') {
        if let Some(next_non_empty) = non_empty.next() {
            return next_non_empty.starts_with("def ") || next_non_empty.starts_with("async def ");
        }
    }
    false
}

fn looks_like_python_code_line(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() {
        return false;
    }
    let assignment_like = t.contains('=')
        && !t.starts_with("## ")
        && t.chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    (t.starts_with("def ")
        || t.starts_with("class ")
        || t.starts_with("async def ")
        || t.starts_with("for ")
        || t.starts_with("if ")
        || t.starts_with("while ")
        || t.starts_with("try:")
        || t.starts_with("except ")
        || t.starts_with("return ")
        || t.starts_with("await ")
        || t.starts_with("import ")
        || t.starts_with("from ")
        || assignment_like)
        && (t.contains('(') || t.contains(':') || t.contains('='))
}

fn looks_like_type_expr_line(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() || t.starts_with('#') {
        return false;
    }
    if t.starts_with("bound method ") {
        return false;
    }
    if t.contains(':') {
        return false;
    }
    let chars_allowed = t.chars().all(|c| {
        c.is_alphanumeric()
            || matches!(
                c,
                '_' | '.'
                    | '['
                    | ']'
                    | '('
                    | ')'
                    | '-'
                    | '>'
                    | ','
                    | '|'
                    | '&'
                    | '?'
                    | ' '
                    | '"'
                    | '\''
            )
    });
    if !chars_allowed {
        return false;
    }
    if t.contains('[') && t.contains(']') {
        return true;
    }
    let has_type_operator = t.contains('|') || t.contains('&') || t.contains("->");
    if has_type_operator {
        return t
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .any(|token| is_simple_type_token(token) || is_simple_class_type_token(token));
    }
    is_simple_type_token(t) || is_simple_class_type_token(t)
}

fn is_simple_type_token(token: &str) -> bool {
    matches!(
        token,
        "Any"
            | "Literal"
            | "None"
            | "Unknown"
            | "bool"
            | "bytes"
            | "datetime"
            | "dict"
            | "float"
            | "int"
            | "list"
            | "set"
            | "str"
            | "tuple"
            | "type"
    )
}

fn is_simple_class_type_token(token: &str) -> bool {
    !token.is_empty()
        && token.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && token.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && token.chars().any(|c| c.is_ascii_lowercase())
}

fn looks_like_simple_type_name_line(line: &str, next_line: Option<&str>) -> bool {
    let t = line.trim();
    if t.is_empty() || t.contains(' ') {
        return false;
    }
    if !t
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '_' | '.'))
    {
        return false;
    }
    if !t
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
    {
        return false;
    }
    matches!(next_line.map(str::trim), Some("---"))
}

fn add_doc_arg_name_spans(text: &str, spans: &mut Vec<crate::highlighter::ColorSpan>) {
    let mut in_args = false;
    let mut offset = 0usize;
    for line in text.split('\n') {
        let trimmed = line.trim();
        if matches!(
            trimmed,
            "Args:" | "Arguments:" | "Keyword Args:" | "Parameters"
        ) {
            in_args = true;
            offset += line.len() + 1;
            continue;
        }
        if in_args {
            if trimmed.is_empty() {
                offset += line.len() + 1;
                continue;
            }
            let indent = line.len() - line.trim_start().len();
            if indent == 0 && !line.starts_with('*') {
                in_args = false;
                offset += line.len() + 1;
                continue;
            }
            if let Some(colon) = line.find(':') {
                let name = line[..colon].trim();
                let name_start_in_line = line[..colon].find(name).unwrap_or(0);
                if !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '*')
                {
                    spans.push(crate::highlighter::ColorSpan {
                        start: offset + name_start_in_line,
                        end: offset + name_start_in_line + name.len(),
                        color: crate::highlighter::DRACULA_ORANGE,
                    });
                }
            }
        }
        offset += line.len() + 1;
    }
}

fn sanitize_hover_type_expr(mut s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    while let Some(pos) = s.find("<class '") {
        out.push_str(&s[..pos]);
        let rest = &s[pos + "<class '".len()..];
        if let Some(end) = rest.find("'>") {
            out.push_str(&rest[..end]);
            s = &rest[end + 2..];
        } else {
            out.push_str(&s[pos..]);
            s = "";
        }
    }
    out.push_str(s);
    out = out.replace("... omitted 3 union elements", "OmittedUnionElements");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn sanitize_module_path(path: &str) -> String {
    let mut parts: Vec<&str> = path
        .split('.')
        .map(str::trim)
        .filter(|p| !p.is_empty() && *p != "__init__")
        .collect();
    if let Some(site_idx) = parts.iter().position(|p| *p == "site-packages") {
        parts = parts.into_iter().skip(site_idx + 1).collect();
    }
    parts.join(".")
}

fn normalize_class_object_repr(line: &str) -> Option<(Option<String>, String)> {
    let trimmed = line.trim();
    let unquoted = trimmed.trim_matches('`').trim();
    let start = unquoted.find("<class '")?;
    let rest = &unquoted[start + "<class '".len()..];
    let end = rest.find("'>")?;
    if !unquoted[..start].trim().is_empty() || !rest[end + 2..].trim().is_empty() {
        return None;
    }
    let type_name = rest[..end].trim();
    if type_name.is_empty() {
        return None;
    }

    if let Some(dot) = type_name.rfind('.') {
        let module_path = sanitize_module_path(type_name[..dot].trim());
        let class_name = type_name[dot + 1..].trim();
        if !module_path.is_empty() && !class_name.is_empty() {
            return Some((Some(module_path), class_name.to_string()));
        }
    }

    Some((None, type_name.to_string()))
}

fn normalize_module_object_repr(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let unquoted = trimmed.trim_matches('`').trim();
    let start = unquoted.find("<module '")?;
    let rest = &unquoted[start + "<module '".len()..];
    let end = rest.find("'>")?;
    if !unquoted[..start].trim().is_empty() || !rest[end + 2..].trim().is_empty() {
        return None;
    }
    let module_path = sanitize_module_path(rest[..end].trim());
    if module_path.is_empty() {
        return None;
    }
    Some(module_path)
}

fn normalize_bound_method_signature(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let (prefix_async, body) = if let Some(rest) = trimmed.strip_prefix("bound async method ") {
        (true, rest)
    } else if let Some(rest) = trimmed.strip_prefix("bound method ") {
        (false, rest)
    } else {
        return None;
    };

    let open = body.find('(')?;
    let close = body.rfind(')')?;
    if close <= open {
        return None;
    }
    let target = body[..open].trim();
    let method = target.rsplit('.').next()?.trim();
    if method.is_empty() {
        return None;
    }

    let mut params = vec!["self".to_string()];
    for raw in body[open + 1..close].split(',') {
        let part = raw.trim();
        if part.is_empty() || part == "/" || part == "*" {
            continue;
        }
        if let Some(colon) = part.find(':') {
            let name = part[..colon].trim();
            let ty = sanitize_hover_type_expr(part[colon + 1..].trim());
            if !name.is_empty() && !ty.is_empty() {
                params.push(format!("{name}: {ty}"));
            } else if !name.is_empty() {
                params.push(name.to_string());
            }
        } else {
            params.push(part.to_string());
        }
    }

    let ret_ty = if let Some(arrow) = body.rfind("->") {
        sanitize_hover_type_expr(body[arrow + 2..].trim())
    } else {
        "Any".to_string()
    };
    let is_async = prefix_async
        || ret_ty.contains("Coroutine")
        || ret_ty.contains("CoroutineType")
        || ret_ty.contains("Awaitable");
    let def_kw = if is_async { "async def" } else { "def" };
    Some(format!(
        "{def_kw} {method}({}) -> {ret_ty}",
        params.join(", ")
    ))
}

fn add_bound_method_signature_spans(
    line: &str,
    line_offset: usize,
    spans: &mut Vec<crate::highlighter::ColorSpan>,
) -> bool {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("bound method ") || !line.contains("->") {
        return false;
    }

    let function_color = [0.313, 0.980, 0.482, 1.0];
    let type_color = [0.545, 0.913, 0.992, 1.0];
    let param_color = [0.973, 0.584, 0.502, 1.0];

    if let Some(open_paren) = line.find('(') {
        if let Some(dot_pos) = line[..open_paren].rfind('.') {
            let name_start = dot_pos + 1;
            if name_start < open_paren {
                spans.push(crate::highlighter::ColorSpan {
                    start: line_offset + name_start,
                    end: line_offset + open_paren,
                    color: function_color,
                });
            }
        }
    }

    if let Some(arrow) = line.rfind("->") {
        let after_arrow = arrow + 2;
        let return_ty = line[after_arrow..].trim_start();
        if !return_ty.is_empty() {
            let ws = line[after_arrow..].len() - return_ty.len();
            let ty_start = after_arrow + ws;
            let ty_len = return_ty
                .chars()
                .take_while(|c| !c.is_whitespace())
                .map(|c| c.len_utf8())
                .sum::<usize>();
            if ty_len > 0 {
                spans.push(crate::highlighter::ColorSpan {
                    start: line_offset + ty_start,
                    end: line_offset + ty_start + ty_len,
                    color: type_color,
                });
            }
        }
    }

    if let (Some(open), Some(close)) = (line.find('('), line.rfind(')')) {
        if close > open {
            let args = &line[open + 1..close];
            if let Some(colon_rel) = args.find(':') {
                let lhs = args[..colon_rel].trim();
                if !lhs.is_empty() && lhs.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    if let Some(lhs_pos) = line[open + 1..close].find(lhs) {
                        let start = open + 1 + lhs_pos;
                        spans.push(crate::highlighter::ColorSpan {
                            start: line_offset + start,
                            end: line_offset + start + lhs.len(),
                            color: param_color,
                        });
                    }
                }
            }
        }
    }

    true
}

fn add_self_param_span_for_signature(
    line: &str,
    line_offset: usize,
    spans: &mut Vec<crate::highlighter::ColorSpan>,
) {
    let trimmed = line.trim_start();
    if !(trimmed.starts_with("def ") || trimmed.starts_with("async def ")) {
        return;
    }
    if let Some(open) = line.find('(') {
        let after = &line[open + 1..];
        if let Some(rest) = after.strip_prefix("self") {
            let next = rest.chars().next();
            if next.is_none() || next == Some(',') || next == Some(')') || next == Some(':') {
                spans.push(crate::highlighter::ColorSpan {
                    start: line_offset + open + 1,
                    end: line_offset + open + 1 + 4,
                    color: [0.741, 0.576, 0.976, 1.0],
                });
            }
        }
    }
}

fn add_param_name_spans_for_signature(
    line: &str,
    line_offset: usize,
    spans: &mut Vec<crate::highlighter::ColorSpan>,
) {
    let trimmed = line.trim_start();
    if !(trimmed.starts_with("def ") || trimmed.starts_with("async def ")) {
        return;
    }
    let (Some(open), Some(close)) = (line.find('('), line.rfind(')')) else {
        return;
    };
    if close <= open {
        return;
    }
    let params_slice = &line[open + 1..close];
    let mut local = 0usize;
    for raw in params_slice.split(',') {
        let p = raw.trim();
        if p.is_empty() || p == "/" || p == "*" {
            local += raw.len() + 1;
            continue;
        }
        let name = p.split(':').next().unwrap_or("").trim();
        if name.is_empty() || name == "self" {
            local += raw.len() + 1;
            continue;
        }
        if let Some(name_pos_rel) = raw.find(name) {
            let start = line_offset + open + 1 + local + name_pos_rel;
            spans.push(crate::highlighter::ColorSpan {
                start,
                end: start + name.len(),
                color: [0.973, 0.584, 0.502, 1.0],
            });
        }
        local += raw.len() + 1;
    }
}

fn add_class_keyword_spans_for_signature(
    line: &str,
    line_offset: usize,
    spans: &mut Vec<crate::highlighter::ColorSpan>,
) {
    let trimmed = line.trim_start();
    let Some(after_class) = trimmed.strip_prefix("class ") else {
        return;
    };

    let class_kw_start = line.len() - trimmed.len();
    spans.push(crate::highlighter::ColorSpan {
        start: line_offset + class_kw_start,
        end: line_offset + class_kw_start + "class".len(),
        color: [1.0, 0.474, 0.776, 1.0],
    });

    let class_name = after_class
        .split(|c: char| c == '(' || c == ':' || c.is_whitespace())
        .next()
        .unwrap_or("")
        .trim();
    if class_name.is_empty() {
        return;
    }

    let relative_name_start = trimmed.find(class_name).unwrap_or("class ".len());
    let class_name_start = class_kw_start + relative_name_start;
    spans.push(crate::highlighter::ColorSpan {
        start: line_offset + class_name_start,
        end: line_offset + class_name_start + class_name.len(),
        color: [0.545, 0.913, 0.992, 1.0],
    });
}

fn add_type_bracket_neutral_spans_for_signature(
    line: &str,
    line_offset: usize,
    spans: &mut Vec<crate::highlighter::ColorSpan>,
) {
    let trimmed = line.trim_start();
    if !(trimmed.starts_with("def ") || trimmed.starts_with("async def ")) {
        return;
    }
    for (idx, ch) in line.char_indices() {
        if ch == '[' || ch == ']' {
            spans.push(crate::highlighter::ColorSpan {
                start: line_offset + idx,
                end: line_offset + idx + ch.len_utf8(),
                color: [0.972, 0.972, 0.949, 1.0],
            });
        }
    }
}

fn add_type_expr_spans_for_line(
    line: &str,
    line_offset: usize,
    spans: &mut Vec<crate::highlighter::ColorSpan>,
) {
    let neutral_color = crate::highlighter::DRACULA_FG;
    let mut run_start: Option<usize> = None;
    for (idx, ch) in line.char_indices() {
        let is_type_char = ch.is_alphanumeric() || ch == '_' || ch == '.';
        if is_type_char {
            if run_start.is_none() {
                run_start = Some(idx);
            }
            continue;
        }
        if let Some(start) = run_start.take() {
            let token = &line[start..idx];
            spans.push(crate::highlighter::ColorSpan {
                start: line_offset + start,
                end: line_offset + idx,
                color: type_expr_token_color(token),
            });
        }
        if matches!(ch, '[' | ']') {
            spans.push(crate::highlighter::ColorSpan {
                start: line_offset + idx,
                end: line_offset + idx + ch.len_utf8(),
                color: neutral_color,
            });
        }
    }
    if let Some(start) = run_start {
        let token = &line[start..];
        spans.push(crate::highlighter::ColorSpan {
            start: line_offset + start,
            end: line_offset + line.len(),
            color: type_expr_token_color(token),
        });
    }
}

fn type_expr_token_color(token: &str) -> [f32; 4] {
    match token {
        "None" | "True" | "False" => crate::highlighter::DRACULA_PINK,
        token if is_simple_type_token(token) => crate::highlighter::DRACULA_CYAN,
        token if is_simple_class_type_token(token) => crate::highlighter::DRACULA_DARK_CYAN,
        _ => crate::highlighter::DRACULA_CYAN,
    }
}

fn normalize_hover_text(msg: &str) -> (String, Vec<(usize, usize)>) {
    let mut out = String::new();
    let mut in_fence = false;
    let mut inline_ranges = Vec::new();

    for raw in msg.replace('\r', "").lines() {
        let trimmed = raw.trim();
        if let Some(normalized) = normalize_bound_method_signature(trimmed) {
            out.push_str(&normalized);
            out.push('\n');
            continue;
        }
        if let Some(normalized) = normalize_special_form_repr(trimmed) {
            out.push_str(&normalized);
            out.push('\n');
            continue;
        }
        if let Some((module_path, class_name)) = normalize_class_object_repr(trimmed) {
            if let Some(module_path) = module_path {
                out.push_str("[[MODULE]] ");
                out.push_str(&module_path);
                out.push('\n');
            }
            out.push_str("class ");
            out.push_str(&class_name);
            out.push('\n');
            continue;
        }
        if let Some(module_path) = normalize_module_object_repr(trimmed) {
            out.push_str("[[MODULE]] ");
            out.push_str(&module_path);
            out.push('\n');
            continue;
        }
        if let Some(normalized) = normalize_asynccontextmanager_signature_line(trimmed) {
            out.push_str(&normalized);
            out.push('\n');
            continue;
        }
        if trimmed == "```" || trimmed == "```python" {
            in_fence = !in_fence;
            continue;
        }
        if trimmed.starts_with(".. code-block::") {
            continue;
        }
        if trimmed.chars().all(|c| c == '-') && trimmed.len() >= 5 {
            out.push_str("---");
            out.push('\n');
            continue;
        }
        if in_fence {
            out.push_str(raw.trim_start_matches("    "));
            out.push('\n');
            continue;
        }
        let (normalized_line, line_ranges) =
            crate::languages::python::normalize_inline_rst_code(raw);
        for (s, e) in line_ranges {
            inline_ranges.push((out.len() + s, out.len() + e));
        }
        out.push_str(&normalized_line);
        out.push('\n');
    }
    (out.trim_end().to_string(), inline_ranges)
}

fn wrap_signature_after_first_param(signature: &str, def_prefix: &str) -> String {
    if !signature.starts_with(def_prefix) || signature.contains('\n') {
        return signature.to_string();
    }
    let Some(open_rel) = signature.find('(') else {
        return signature.to_string();
    };
    let Some(close_rel) = signature.rfind(')') else {
        return signature.to_string();
    };
    if close_rel <= open_rel {
        return signature.to_string();
    }
    let params = &signature[open_rel + 1..close_rel];
    let Some(first_comma_rel) = params.find(',') else {
        return signature.to_string();
    };
    let comma_abs = open_rel + 1 + first_comma_rel;
    let head = &signature[..=comma_abs];
    let tail = signature[comma_abs + 1..].trim_start();
    let indent = " ".repeat(open_rel + 1);
    format!("{head}\n{indent}{tail}")
}

fn normalize_asynccontextmanager_signature_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let def_part = trimmed.strip_prefix("@asynccontextmanager ")?;
    if !def_part.starts_with("async def ") {
        return None;
    }
    let def_part = def_part.trim_end_matches(':');
    let wrapped = wrap_signature_after_first_param(def_part, "async def ");
    Some(format!("@asynccontextmanager\n{wrapped}"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoverLineKindPublic {
    Text,
    Code,
    Separator,
    Header1,
    Header2,
}

#[derive(Clone, Copy)]
pub(crate) enum PendingRequestKind {
    Hover,
    CodeAction,
    Definition,
    Completion,
    InlayHint,
    WorkspaceDiagnostic,
}

#[cfg(test)]
#[path = "python_hover_tests.rs"]
mod python_hover_tests;
