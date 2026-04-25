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
        spans.sort_unstable_by(|a, b| a.start.cmp(&b.start).then_with(|| b.end.cmp(&a.end)));
        return (clean_msg, spans, line_kinds, inline_code_ranges);
    }
    let clean_msg = normalize_hover_text(&preprocessed);
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
                                || line.trim_start().starts_with("async def "))
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

    spans.sort_unstable_by(|a, b| a.start.cmp(&b.start).then_with(|| b.end.cmp(&a.end)));
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
    (clean_msg, spans, line_kinds, Vec::new())
}

fn preprocess_hover_text(msg: &str) -> String {
    let mut out = String::new();
    for line in msg.replace('\r', "").lines() {
        if let Some(normalized) = normalize_asynccontextmanager_signature_line(line.trim()) {
            out.push_str(&normalized);
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out.trim_end().to_string()
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
    if !(t.contains('[') && t.contains(']')) {
        return false;
    }
    if t.contains("->") || t.contains('(') || t.contains(')') || t.contains(':') {
        return false;
    }
    t.chars().all(|c| {
        c.is_alphanumeric()
            || matches!(
                c,
                '_' | '.' | '[' | ']' | ',' | '|' | '?' | ' ' | '"' | '\''
            )
    })
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
    let type_color = [0.545, 0.913, 0.992, 1.0];
    let neutral_color = [0.972, 0.972, 0.949, 1.0];
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
            spans.push(crate::highlighter::ColorSpan {
                start: line_offset + start,
                end: line_offset + idx,
                color: type_color,
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
        spans.push(crate::highlighter::ColorSpan {
            start: line_offset + start,
            end: line_offset + line.len(),
            color: type_color,
        });
    }
}

fn normalize_hover_text(msg: &str) -> String {
    let mut out = String::new();
    let mut in_fence = false;
    for raw in msg.replace('\r', "").lines() {
        let trimmed = raw.trim();
        if let Some(normalized) = normalize_bound_method_signature(trimmed) {
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
        out.push_str(raw);
        out.push('\n');
    }
    out.trim_end().to_string()
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
}

#[cfg(test)]
mod tests {
    use super::{highlight_hover_text, HoverLineKindPublic};

    #[test]
    fn variable_header_highlights_name_pink() {
        let raw = "## Variable handlers of main\nhandlers: list[Router]";
        let (text, spans, kinds, _inline) = highlight_hover_text(raw);
        let handlers_idx = text.find("handlers").unwrap();
        assert!(
            spans.iter().any(|s| s.start == handlers_idx
                && s.end == handlers_idx + 8
                && s.color == [1.0, 0.474, 0.776, 1.0]),
            "Header variable name should be pink"
        );
        assert_eq!(
            kinds[2],
            HoverLineKindPublic::Code,
            "Assignment line should become code"
        );
    }

    #[test]
    fn uses_python_hover_pipeline_for_builtin_signatures() {
        let raw = "def update(m: SupportsKeysAndGetItem[str, Provide], /) -> None\n\
D.update([E, ]**F) -> None.  Update D from mapping/iterable E and F.\n\
If E present and has a .keys() method, does:     for k in E.keys(): D[k] = E[k]\n\
If E present and lacks .keys() method, does:     for (k, v) in E: D[k] = v\n\
In either case, this is followed by: for k, v in F.items(): D[k] = v";

        let (text, _spans, kinds, _inline) = highlight_hover_text(raw);
        assert!(text.starts_with("def update("));
        assert!(
            kinds.iter().any(|kind| *kind == HoverLineKindPublic::Text),
            "built-in docs must preserve prose lines",
        );
        assert!(
            kinds.iter().any(|kind| *kind == HoverLineKindPublic::Code),
            "inline python fragments should be promoted to code lines",
        );
    }

    #[test]
    fn plain_bound_method_prose_stays_uncolored() {
        let raw = "bound method dict[str, Provide].copy() -> dict[str, Provide]\n\
Return a shallow copy of the dict.";
        let (text, spans, _kinds, _inline) = highlight_hover_text(raw);
        assert!(text.starts_with("def copy(self"));
        assert!(!text.contains("bound method"));
        let first_line = text.lines().next().unwrap_or("");
        let def_start = first_line.find("def").unwrap_or(0);
        let self_start = first_line.find("self").unwrap_or(0);
        let bracket_start = first_line.find('[').unwrap_or(0);
        assert!(
            spans.iter().any(|s| s.start <= def_start
                && s.end >= def_start + 3
                && s.color == [1.0, 0.474, 0.776, 1.0]),
            "`def` should be highlighted as keyword (pink)",
        );
        assert!(
            spans.iter().any(|s| s.start <= self_start
                && s.end >= self_start + 4
                && s.color == [0.741, 0.576, 0.976, 1.0]),
            "`self` should be highlighted in violet",
        );
        assert!(
            spans.iter().any(|s| s.start <= bracket_start
                && s.end >= bracket_start + 1
                && s.color == [0.972, 0.972, 0.949, 1.0]),
            "type brackets should be white",
        );
        let first_line_len = text.lines().next().unwrap_or("").len();
        let prose_offset = first_line_len + 1;
        assert!(
            spans.iter().all(|s| s.end <= prose_offset),
            "prose line must stay uncolored",
        );
    }

    #[test]
    fn inline_for_fragment_becomes_separate_code_line() {
        let raw = "def update(m: SupportsKeysAndGetItem[str, Provide], /) -> None\n\
In either case, this is followed by: for k, v in F.items(): D[k] = v";
        let (text, _spans, kinds, _inline) = highlight_hover_text(raw);
        assert!(text
            .contains("In either case, this is followed by:\n    for k, v in F.items(): D[k] = v"));
        assert!(kinds.iter().any(|kind| *kind == HoverLineKindPublic::Code));
    }

    #[test]
    fn rich_rst_docstring_path_is_preserved() {
        let raw = "def create_pool() -> Unknown\n\
Can be used either with an ``async with`` block:\n\
\n\
.. code-block:: python\n\
\n\
    async with asyncpg.create_pool(user='postgres') as pool:\n\
        await pool.fetch('SELECT 1')\n\
\n\
:param str dsn:\n\
    Connection string.";
        let (text, _spans, kinds, _inline) = highlight_hover_text(raw);
        assert!(text.contains("Parameters"));
        assert!(kinds
            .iter()
            .any(|kind| *kind == HoverLineKindPublic::Header1));
    }

    #[test]
    fn bound_method_signature_line_is_highlighted_but_prose_stays_plain() {
        let raw = "bound method list[<class 'AuthController'> | Router].append(object: <class 'AuthController'> | Router, /) -> None\n\
Append object to the end of the list.";
        let (text, spans, _kinds, _inline) = highlight_hover_text(raw);
        assert!(text.starts_with("def append(self, object: AuthController | Router) -> None"));
        assert!(!text.contains("bound method"));
        assert!(
            !spans.is_empty(),
            "signature line should receive highlighting"
        );

        let first_line_len = text.lines().next().unwrap_or("").len();
        let prose_offset = first_line_len + 1;
        assert!(
            spans.iter().all(|s| s.end <= prose_offset),
            "prose description line must stay uncolored",
        );
    }

    #[test]
    fn coroutine_return_bound_method_becomes_async_def() {
        let raw = "bound method LokiBatcher.shutdown() -> CoroutineType[Any, Any, None]";
        let (text, _spans, _kinds, _inline) = highlight_hover_text(raw);
        assert!(text.starts_with("async def shutdown(self) -> CoroutineType[Any, Any, None]"));
    }

    #[test]
    fn coroutine_return_signature_becomes_async_def() {
        let raw = "def add_default_users_and_roles_session() -> CoroutineType[Any, Any, Unknown]";
        let (text, spans, _kinds, _inline) = highlight_hover_text(raw);
        assert!(text.starts_with(
            "async def add_default_users_and_roles_session() -> CoroutineType[Any, Any, Unknown]"
        ));
        let async_start = text.find("async").unwrap_or(0);
        assert!(
            spans.iter().any(|s| s.start <= async_start
                && s.end >= async_start + 5
                && s.color == [1.0, 0.474, 0.776, 1.0]),
            "`async` keyword should be pink in normalized coroutine signatures",
        );
    }

    #[test]
    fn long_bound_method_signature_is_normalized_and_highlighted() {
        let raw = "bound method list[<class 'AuthController'> | Router | <class 'DiscountsController'> | ... omitted 3 union elements].append(object: <class 'AuthController'> | Router | <class 'DiscountsController'> | ... omitted 3 union elements, /) -> None";
        let (text, spans, _kinds, _inline) = highlight_hover_text(raw);
        assert!(text.starts_with("def append(self, object: AuthController | Router | DiscountsController | OmittedUnionElements) -> None"));
        assert!(!text.contains("bound method"));
        assert!(
            !spans.is_empty(),
            "normalized signature should get syntax spans"
        );
        let first_line = text.lines().next().unwrap_or("");
        let self_start = first_line.find("self").unwrap_or(0);
        let object_start = first_line.find("object").unwrap_or(0);
        assert!(
            spans.iter().any(|s| s.start <= self_start
                && s.end >= self_start + 4
                && s.color == [0.741, 0.576, 0.976, 1.0]),
            "`self` should stay violet in long normalized signatures",
        );
        assert!(
            spans.iter().any(|s| s.start <= object_start
                && s.end >= object_start + 6
                && s.color == [0.973, 0.584, 0.502, 1.0]),
            "argument names should be orange",
        );
    }

    #[test]
    fn class_object_repr_is_normalized_and_highlighted() {
        let raw = "<class 'CoreMiddleware'>";
        let (text, spans, _kinds, _inline) = highlight_hover_text(raw);
        assert_eq!(text, "class CoreMiddleware");

        let class_kw = text.find("class").unwrap_or(0);
        let class_name = text.find("CoreMiddleware").unwrap_or(0);
        assert!(
            spans.iter().any(|s| s.start <= class_kw
                && s.end >= class_kw + 5
                && s.color == [1.0, 0.474, 0.776, 1.0]),
            "`class` keyword should be pink",
        );
        assert!(
            spans.iter().any(|s| s.start <= class_name
                && s.end >= class_name + "CoreMiddleware".len()
                && s.color == [0.545, 0.913, 0.992, 1.0]),
            "class name should be cyan",
        );
    }

    #[test]
    fn qualified_class_object_repr_prepends_module_path() {
        let raw = "<class 'car_wash.utils.middlewares.CoreMiddleware'>";
        let (text, spans, _kinds, _inline) = highlight_hover_text(raw);
        assert_eq!(
            text,
            "[[MODULE]] car_wash.utils.middlewares\nclass CoreMiddleware"
        );

        let class_line_offset = text.find("class ").unwrap_or(0);
        assert!(
            spans.iter().any(|s| s.start <= class_line_offset
                && s.end >= class_line_offset + 5
                && s.color == [1.0, 0.474, 0.776, 1.0]),
            "class keyword should remain highlighted on normalized second line",
        );
    }

    #[test]
    fn backticked_qualified_class_repr_prepends_module_path() {
        let raw = "`<class 'car_wash.utils.middlewares.CoreMiddleware'>`";
        let (text, _spans, _kinds, _inline) = highlight_hover_text(raw);
        assert_eq!(
            text,
            "[[MODULE]] car_wash.utils.middlewares\nclass CoreMiddleware"
        );
    }

    #[test]
    fn generic_type_line_gets_cyan_types_and_white_brackets() {
        let raw = "dict[str, Provide]";
        let (text, spans, _kinds, _inline) = highlight_hover_text(raw);
        assert_eq!(text, "dict[str, Provide]");

        let dict_start = text.find("dict").unwrap_or(0);
        let str_start = text.find("str").unwrap_or(0);
        let provide_start = text.find("Provide").unwrap_or(0);
        let l_bracket = text.find('[').unwrap_or(0);
        let r_bracket = text.find(']').unwrap_or(0);
        let cyan = [0.545, 0.913, 0.992, 1.0];
        let white = [0.972, 0.972, 0.949, 1.0];

        assert!(spans
            .iter()
            .any(|s| s.start <= dict_start && s.end >= dict_start + 4 && s.color == cyan));
        assert!(spans
            .iter()
            .any(|s| s.start <= str_start && s.end >= str_start + 3 && s.color == cyan));
        assert!(spans.iter().any(|s| s.start <= provide_start
            && s.end >= provide_start + "Provide".len()
            && s.color == cyan));
        assert!(spans
            .iter()
            .any(|s| s.start <= l_bracket && s.end >= l_bracket + 1 && s.color == white));
        assert!(spans
            .iter()
            .any(|s| s.start <= r_bracket && s.end >= r_bracket + 1 && s.color == white));
    }

    #[test]
    fn literal_type_line_is_highlighted() {
        let raw = "Literal[\"513\"]";
        let (text, spans, _kinds, _inline) = highlight_hover_text(raw);
        assert_eq!(text, "Literal[\"513\"]");
        let literal_start = text.find("Literal").unwrap_or(0);
        let l_bracket = text.find('[').unwrap_or(0);
        let r_bracket = text.rfind(']').unwrap_or(0);
        let cyan = [0.545, 0.913, 0.992, 1.0];
        let white = [0.972, 0.972, 0.949, 1.0];

        assert!(spans.iter().any(|s| s.start <= literal_start
            && s.end >= literal_start + "Literal".len()
            && s.color == cyan));
        assert!(spans
            .iter()
            .any(|s| s.start <= l_bracket && s.end >= l_bracket + 1 && s.color == white));
        assert!(spans
            .iter()
            .any(|s| s.start <= r_bracket && s.end >= r_bracket + 1 && s.color == white));
    }

    #[test]
    fn assignment_line_is_highlighted_in_hover_text() {
        let raw = "## Атрибут класса client в car_wash.core.fcm.service.FcmSenderService\n\
client = AsyncFirebaseClient(\n\
    request_timeout=RequestTimeout(timeout=50)\n\
    )";
        let (_text, spans, _kinds, _inline) = highlight_hover_text(raw);
        assert!(
            spans.iter().any(|s| s.color != [0.972, 0.972, 0.949, 1.0]),
            "assignment hover should not stay fully white",
        );
    }

    #[test]
    fn module_object_repr_is_normalized_to_module_header() {
        let raw = "<module 'car_wash.domains.policies.controller'>";
        let (text, _spans, _kinds, _inline) = highlight_hover_text(raw);
        assert_eq!(text, "[[MODULE]] car_wash.domains.policies.controller");
    }

    #[test]
    fn site_packages_and_init_are_removed_from_module_and_class_repr() {
        let module_raw = "<module 'site-packages.msgspec.__init__'>";
        let (module_text, _spans, _kinds, _inline) = highlight_hover_text(module_raw);
        assert_eq!(module_text, "[[MODULE]] msgspec");

        let class_raw = "<class 'site-packages.msgspec.__init__.UnsetType'>";
        let (class_text, _spans, _kinds, _inline) = highlight_hover_text(class_raw);
        assert_eq!(class_text, "[[MODULE]] msgspec\nclass UnsetType");
    }

    #[test]
    fn builtin_dict_class_hover_is_normalized_without_angle_brackets() {
        let raw = "<class 'dict'>\n\
dict() -> new empty dictionary";
        let (text, spans, _kinds, _inline) = highlight_hover_text(raw);
        assert!(text.starts_with("class dict"));
        assert!(!text.contains("<class"));
        assert!(
            spans.iter().any(|s| s.color == [1.0, 0.474, 0.776, 1.0]),
            "keyword highlight should exist for normalized class heading",
        );
    }

    #[test]
    fn builtin_str_heading_line_is_highlighted_as_type() {
        let raw = "str\n\
---------------------------------------------\n\
str(object='') -> str";
        let (text, spans, _kinds, _inline) = highlight_hover_text(raw);
        let str_start = text.find("str").unwrap_or(0);
        assert!(
            spans.iter().any(|s| s.start <= str_start
                && s.end >= str_start + 3
                && s.color == [0.545, 0.913, 0.992, 1.0]),
            "standalone builtin type heading must be cyan",
        );
    }

    #[test]
    fn builtin_any_heading_line_is_highlighted_as_type() {
        let raw = "Any\n\
---------------------------------------------\n\
Special type indicating an unconstrained type.";
        let (text, spans, _kinds, _inline) = highlight_hover_text(raw);
        let any_start = text.find("Any").unwrap_or(0);
        assert!(
            spans.iter().any(|s| s.start <= any_start
                && s.end >= any_start + 3
                && s.color == [0.545, 0.913, 0.992, 1.0]),
            "standalone typing.Any heading must be cyan",
        );
    }

    #[test]
    fn header_with_inline_code_preserves_correct_spans_without_byte_shift_panic() {
        let raw = "## ``client`` in FcmSenderService";
        let (text, _spans, _kinds, _inline) = highlight_hover_text(raw);
        assert!(text.contains("client"));
    }

    #[test]
    fn class_attribute_hover_is_translated_to_english_with_pink_name_and_orange_args() {
        let raw = "## Атрибут класса client в car_wash.core.fcm.service.FcmSenderService\n\
client = AsyncFirebaseClient(\n\
    request_timeout=RequestTimeout(timeout=50)\n\
    )";
        let (text, spans, kinds, _inline) = highlight_hover_text(raw);
        assert!(
            text.contains("car_wash.core.fcm.service\nClass attribute client of FcmSenderService")
        );
        assert!(text.contains("\n---\n"));
        assert!(kinds.iter().any(|k| *k == HoverLineKindPublic::Separator));

        let client_offset = text.find("Class attribute ").unwrap() + 16;
        assert!(spans.iter().any(|s| s.start == client_offset
            && s.end == client_offset + 6
            && s.color == [1.0, 0.474, 0.776, 1.0]));

        let fcm_offset = text.find("FcmSenderService").unwrap();
        assert!(spans.iter().any(|s| s.start == fcm_offset
            && s.end == fcm_offset + "FcmSenderService".len()
            && s.color == [0.545, 0.913, 0.992, 1.0]));

        let req_timeout_offset = text.find("request_timeout").unwrap();
        assert!(spans.iter().any(|s| s.start == req_timeout_offset
            && s.end == req_timeout_offset + "request_timeout".len()
            && s.color == [0.973, 0.584, 0.502, 1.0]));

        let timeout_offset = text.find("(timeout=").unwrap() + 1;
        assert!(spans.iter().any(|s| s.start == timeout_offset
            && s.end == timeout_offset + "timeout".len()
            && s.color == [0.973, 0.584, 0.502, 1.0]));
    }

    #[test]
    fn class_signature_is_highlighted() {
        let raw = "class ValueError(...)\n\
        ---\n\
        Inappropriate argument value (of correct type).";
        let (text, spans, _kinds, _inline) = highlight_hover_text(raw);
        assert!(text.starts_with("class ValueError"));
        let class_kw = text.find("class").unwrap();
        assert!(
            spans.iter().any(|s| s.start <= class_kw
                && s.end >= class_kw + 5
                && (s.color == [0.545, 0.913, 0.992, 1.0] || s.color == [1.0, 0.474, 0.776, 1.0])),
            "`class` keyword should be highlighted in signatures",
        );
    }

    #[test]
    fn litestar_giant_class_hover_signature_and_args_doc_are_parsed() {
        let raw = r#"class Litestar(
    route_handlers: Sequence[type[Controller] | HTTPRouteHandler | WebsocketRouteHandler | ... omitted 3 union elements] | None = None,
    *,
    after_exception: Sequence[(ExceptionT, HTTPScope | WebSocketScope, /) -> None | Awaitable[None]] | None = None,
    after_request: (((HTTPScope | WebSocketScope, (...) -> Awaitable[HTTPRequestEvent | HTTPDisconnectEvent | WebSocketConnectEvent | WebSocketReceiveEvent | WebSocketDisconnectEvent], (HTTPResponseStartEvent | HTTPResponseBodyEvent | HTTPServerPushEvent | ... omitted 6 union elements, /) -> Awaitable[None], /) -> Awaitable[None], /) -> ((HTTPScope | WebSocketScope, (...) -> Awaitable[HTTPRequestEvent | HTTPDisconnectEvent | WebSocketConnectEvent | WebSocketReceiveEvent | WebSocketDisconnectEvent], (HTTPResponseStartEvent | HTTPResponseBodyEvent | HTTPServerPushEvent | ... omitted 6 union elements, /) -> Awaitable[None], /) -> Awaitable[None]) | Awaitable[(HTTPScope | WebSocketScope, (...) -> Awaitable[HTTPRequestEvent | HTTPDisconnectEvent | WebSocketConnectEvent | WebSocketReceiveEvent | WebSocketDisconnectEvent], (HTTPResponseStartEvent | HTTPResponseBodyEvent | HTTPServerPushEvent | ... omitted 6 union elements, /) -> Awaitable[None], /) -> Awaitable[None]]) | ((Response[Unknown], /) -> Response[Unknown] | Awaitable[Response[Unknown]]) | None = None,
    after_response: ((Request[Unknown, Unknown, Unknown], /) -> None | Awaitable[None]) | None = None,
    allowed_hosts: Sequence[str] | AllowedHostsConfig | None = None,
    before_request: ((Request[Unknown, Unknown, Unknown], /) -> Any | Awaitable[Any]) | None = None,
    before_send: Sequence[(HTTPResponseStartEvent | HTTPResponseBodyEvent | HTTPServerPushEvent | ... omitted 6 union elements, HTTPScope | WebSocketScope, /) -> None | Awaitable[None]] | None = None,
    cache_control: CacheControlHeader | None = None,
    compression_config: CompressionConfig | None = None,
    cors_config: CORSConfig | None = None,
    csrf_config: CSRFConfig | None = None,
    dto: type[AbstractDTO[Unknown]] | None | _EmptyEnum = ...,
    debug: bool | None = None,
    dependencies: Mapping[str, Provide | ((...) -> Any)] | None = None,
    etag: ETag | None = None,
    event_emitter_backend: type[BaseEventEmitterBackend] = ...,
    exception_handlers: MutableMapping[int | type[Exception], (Request[Unknown, Unknown, Unknown], ExceptionT, /) -> Response[Unknown]] | None = None,
    guards: Sequence[(ASGIConnection[Unknown, Unknown, Unknown, Unknown], BaseRouteHandler, /) -> None | Awaitable[None]] | None = None,
    include_in_schema: bool | _EmptyEnum = ...,
    listeners: Sequence[EventListener] | None = None,
    logging_config: BaseLoggingConfig | _EmptyEnum | None = ...,
    middleware: Sequence[((...) -> ((HTTPScope | WebSocketScope, (...) -> Awaitable[HTTPRequestEvent | HTTPDisconnectEvent | WebSocketConnectEvent | WebSocketReceiveEvent | WebSocketDisconnectEvent], (HTTPResponseStartEvent | HTTPResponseBodyEvent | HTTPServerPushEvent | ... omitted 6 union elements, /) -> Awaitable[None], /) -> Awaitable[None])) | DefineMiddleware | Iterator[tuple[(HTTPScope | WebSocketScope, (...) -> Awaitable[HTTPRequestEvent | HTTPDisconnectEvent | WebSocketConnectEvent | WebSocketReceiveEvent | WebSocketDisconnectEvent], (HTTPResponseStartEvent | HTTPResponseBodyEvent | HTTPServerPushEvent | ... omitted 6 union elements, /) -> Awaitable[None], /) -> Awaitable[None], dict[str, Any]]] | type[@Todo]] | None = None,
    multipart_form_part_limit: int = 1000,
    on_app_init: Sequence[(AppConfig, /) -> AppConfig] | None = None,
    on_shutdown: Sequence[((Litestar, /) -> Any | Awaitable[Any]) | (() -> Any | Awaitable[Any])] | None = None,
    on_startup: Sequence[((Litestar, /) -> Any | Awaitable[Any]) | (() -> Any | Awaitable[Any])] | None = None,
    openapi_config: OpenAPIConfig | None = ...,
    opt: Mapping[str, Any] | None = None,
    parameters: Mapping[str, ParameterKwarg] | None = None,
    path: str | None = None,
    plugins: Sequence[CLIPluginProtocol | InitPluginProtocol | OpenAPISchemaPluginProtocol | ... omitted 3 union elements] | None = None,
    request_class: type[Request[Unknown, Unknown, Unknown]] | None = None,
    request_max_body_size: int | None = 10000000,
    response_cache_config: ResponseCacheConfig | None = None,
    response_class: type[Response[Unknown]] | None = None,
    response_cookies: Sequence[Cookie] | Mapping[str, str] | None = None,
    response_headers: Sequence[ResponseHeader] | Mapping[str, str] | None = None,
    return_dto: type[AbstractDTO[Unknown]] | None | _EmptyEnum = ...,
    security: Sequence[dict[str, list[str]]] | None = None,
    signature_namespace: Mapping[str, Any] | None = None,
    signature_types: Sequence[Any] | None = None,
    state: State | None = None,
    static_files_config: Sequence[StaticFilesConfig] | None = None,
    stores: StoreRegistry | dict[str, Store] | None = None,
    tags: Sequence[str] | None = None,
    template_config: TemplateConfig[EngineType] | None = None,
    type_decoders: Sequence[tuple[(Any, /) -> bool, (Any, Any, /) -> Any]] | None = None,
    type_encoders: Mapping[Any, (Any, /) -> Any] | None = None,
    websocket_class: type[WebSocket[Unknown, Unknown, Unknown]] | None = None,
    lifespan: Sequence[((Litestar, /) -> AbstractAsyncContextManager[Unknown, bool | None]) | AbstractAsyncContextManager[Unknown, bool | None]] | None = None,
    pdb_on_exception: bool | None = None,
    debugger_module: PDBProtocol = ...,
    experimental_features: Iterable[ExperimentalFeatures] | None = None
)
---------------------------------------------
Initialize a ``Litestar`` application.

Args:
    after_exception: A sequence of :class:`exception hook handlers <.types.AfterExceptionHookHandler>`. This
        hook is called after an exception occurs. In difference to exception handlers, it is not meant to
        return a response - only to process the exception (e.g. log it, send it to Sentry etc.).
    after_request: A sync or async function executed after the route handler function returned and the response
        object has been resolved. Receives the response object.
    after_response: A sync or async function called after the response has been awaited. It receives the
        :class:`Request <.connection.Request>` object and should not return any values.
    allowed_hosts: A sequence of allowed hosts, or an
        :class:`AllowedHostsConfig <.config.allowed_hosts.AllowedHostsConfig>` instance. Enables the builtin
        allowed hosts middleware.
    before_request: A sync or async function called immediately before calling the route handler. Receives the
        :class:`Request <.connection.Request>` instance and any non-``None`` return value is used for the
        response, bypassing the route handler.
    before_send: A sequence of :class:`before send hook handlers <.types.BeforeMessageSendHookHandler>`. Called
        when the ASGI send function is called.
    cache_control: A ``cache-control`` header of type
        :class:`CacheControlHeader <litestar.datastructures.CacheControlHeader>` to add to route handlers of
        this app. Can be overridden by route handlers.
    compression_config: Configures compression behaviour of the application, this enabled a builtin or user
        defined Compression middleware.
    cors_config: If set, configures CORS handling of the application.
    csrf_config: If set, configures :class:`CSRFMiddleware <.middleware.csrf.CSRFMiddleware>`.
    debug: If ``True``, app errors rendered as HTML with a stack trace.
    dependencies: A string keyed mapping of dependency :class:`Providers <.di.Provide>`.
    dto: :class:`AbstractDTO <.dto.base_dto.AbstractDTO>` to use for (de)serializing and
        validation of request data.
    etag: An ``etag`` header of type :class:`ETag <.datastructures.ETag>` to add to route handlers of this app.
        Can be overridden by route handlers.
    event_emitter_backend: A subclass of
        :class:`BaseEventEmitterBackend <.events.emitter.BaseEventEmitterBackend>`.
    exception_handlers: A mapping of status codes and/or exception types to handler functions.
    guards: A sequence of :class:`Guard <.types.Guard>` callables.
    include_in_schema: A boolean flag dictating whether  the route handler should be documented in the OpenAPI schema.
    lifespan: A list of callables returning async context managers, wrapping the lifespan of the ASGI application
    listeners: A sequence of :class:`EventListener <.events.listener.EventListener>`.
    logging_config: A subclass of :class:`BaseLoggingConfig <.logging.config.BaseLoggingConfig>`.
    middleware: A sequence of :class:`Middleware <.types.Middleware>`.
    multipart_form_part_limit: The maximal number of allowed parts in a multipart/formdata request. This limit
        is intended to protect from DoS attacks.
    on_app_init: A sequence of :class:`OnAppInitHandler <.types.OnAppInitHandler>` instances. Handlers receive
        an instance of :class:`AppConfig <.config.app.AppConfig>` that will have been initially populated with
        the parameters passed to :class:`Litestar <litestar.app.Litestar>`, and must return an instance of same.
        If more than one handler is registered they are called in the order they are provided.
    on_shutdown: A sequence of :class:`LifespanHook <.types.LifespanHook>` called during application
        shutdown.
    on_startup: A sequence of :class:`LifespanHook <litestar.types.LifespanHook>` called during
        application startup.
    openapi_config: Defaults to :attr:`DEFAULT_OPENAPI_CONFIG`
    opt: A string keyed mapping of arbitrary values that can be accessed in :class:`Guards <.types.Guard>` or
        wherever you have access to :class:`Request <litestar.connection.request.Request>` or
        :class:`ASGI Scope <.types.Scope>`.
    parameters: A mapping of :class:`Parameter <.params.Parameter>` definitions available to all application
        paths.
    path: A path fragment that is prefixed to all route handlers, controllers and routers associated
        with the application instance.

        .. versionadded:: 2.8.0
    pdb_on_exception: Drop into the PDB when an exception occurs.
    debugger_module: A `pdb`-like debugger module that supports the `post_mortem()` protocol.
        This module will be used when `pdb_on_exception` is set to True.
    plugins: Sequence of plugins.
    request_class: An optional subclass of :class:`Request <.connection.Request>` to use for http connections.
    request_max_body_size: Maximum allowed size of the request body in bytes. If this size is exceeded, a
        '413 - Request Entity Too Large' error response is returned.
    response_class: A custom subclass of :class:`Response <.response.Response>` to be used as the app's default
        response.
    response_cookies: A sequence of :class:`Cookie <.datastructures.Cookie>`.
    response_headers: A string keyed mapping of :class:`ResponseHeader <.datastructures.ResponseHeader>`
    response_cache_config: Configures caching behavior of the application.
    return_dto: :class:`AbstractDTO <.dto.base_dto.AbstractDTO>` to use for serializing
        outbound response data.
    route_handlers: A sequence of route handlers, which can include instances of
        :class:`Router <.router.Router>`, subclasses of :class:`Controller <.controller.Controller>` or any
        callable decorated by the route handler decorators.
    security: A sequence of dicts that will be added to the schema of all route handlers in the application.
        See
        :data:`SecurityRequirement <.openapi.spec.SecurityRequirement>` for details.
    signature_namespace: A mapping of names to types for use in forward reference resolution during signature modelling.
    signature_types: A sequence of types for use in forward reference resolution during signature modelling.
        These types will be added to the signature namespace using their ``__name__`` attribute.
    state: An optional :class:`State <.datastructures.State>` for application state.
    static_files_config: A sequence of :class:`StaticFilesConfig <.static_files.StaticFilesConfig>`
    stores: Central registry of :class:`Store <.stores.base.Store>` that will be available throughout the
        application. If this is a dictionary to it will be passed to a
        :class:`StoreRegistry <.stores.registry.StoreRegistry>`. If it is a
        :class:`StoreRegistry <.stores.registry.StoreRegistry>`, this instance will be used directly.
    tags: A sequence of string tags that will be appended to the schema of all route handlers under the
        application.
    template_config: An instance of :class:`TemplateConfig <.template.TemplateConfig>`
    type_decoders: A sequence of tuples, each composed of a predicate testing for type identity and a msgspec
        hook for deserialization.
    type_encoders: A mapping of types to callables that transform them into types supported for serialization.
    websocket_class: An optional subclass of :class:`WebSocket <.connection.WebSocket>` to use for websocket
        connections.
    experimental_features: An iterable of experimental features to enable"#;

        let (text, spans, kinds, _inline) = highlight_hover_text(raw);

        assert!(text.starts_with("class Litestar("));
        assert!(
            text.contains("experimental_features: Iterable[ExperimentalFeatures] | None = None\n)")
        );
        assert!(text.contains("Initialize a Litestar application."));
        assert!(text.contains("Args:"));
        assert!(kinds
            .iter()
            .any(|kind| *kind == HoverLineKindPublic::Separator));

        let class_idx = text.find("class").unwrap();
        assert!(spans.iter().any(|s| {
            s.start == class_idx
                && s.end == class_idx + "class".len()
                && s.color == [1.0, 0.474, 0.776, 1.0]
        }));

        let litestar_idx = text.find("Litestar").unwrap();
        assert!(spans.iter().any(|s| {
            s.start == litestar_idx
                && s.end == litestar_idx + "Litestar".len()
                && s.color == [0.545, 0.913, 0.992, 1.0]
        }));

        let args_idx = text.find("Args:").unwrap();
        for name in [
            "after_exception",
            "after_request",
            "allowed_hosts",
            "pdb_on_exception",
            "experimental_features",
        ] {
            let idx = args_idx + text[args_idx..].find(name).unwrap();
            assert!(
                spans.iter().any(|s| {
                    s.start == idx
                        && s.end == idx + name.len()
                        && s.color == [0.973, 0.584, 0.502, 1.0]
                }),
                "doc argument `{name}` should be highlighted orange"
            );
        }
    }

    #[test]
    fn asynccontextmanager_one_line_signature_is_wrapped_and_highlighted() {
        let raw =
            "@asynccontextmanager async def lifespan(_: Litestar, arg: str) -> AsyncGenerator[None, Any]:";
        let (text, spans, _kinds, _inline) = highlight_hover_text(raw);
        assert_eq!(
            text,
            "@asynccontextmanager\nasync def lifespan(_: Litestar,\n                   arg: str) -> AsyncGenerator[None, Any]"
        );
        let litestar = text.find("Litestar").unwrap_or(0);
        assert!(
            spans.iter().any(|s| s.start <= litestar
                && s.end >= litestar + "Litestar".len()
                && s.color == [0.545, 0.913, 0.992, 1.0]),
            "type names in signature should be cyan",
        );
        let decorator_at = text.find('@').unwrap_or(0);
        assert!(
            spans.iter().any(|s| s.start <= decorator_at
                && s.end >= decorator_at + 1
                && s.color == [1.0, 0.474, 0.776, 1.0]),
            "decorator @ should be pink",
        );
        let decorator_name = text.find("asynccontextmanager").unwrap_or(0);
        assert!(
            spans.iter().any(|s| s.start <= decorator_name
                && s.end >= decorator_name + "asynccontextmanager".len()
                && s.color == [0.313, 0.980, 0.482, 1.0]),
            "decorator name should be green",
        );
        let l_bracket = text.find('[').unwrap_or(0);
        let r_bracket = text.rfind(']').unwrap_or(0);
        assert!(
            spans.iter().any(|s| s.start <= l_bracket
                && s.end >= l_bracket + 1
                && s.color == [0.972, 0.972, 0.949, 1.0]),
            "left bracket in return type should be neutral white",
        );
        assert!(
            spans.iter().any(|s| s.start <= r_bracket
                && s.end >= r_bracket + 1
                && s.color == [0.972, 0.972, 0.949, 1.0]),
            "right bracket in return type should be neutral white",
        );
    }
}
