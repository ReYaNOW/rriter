use crate::lsp::HoverLineKindPublic;
use std::collections::HashMap;
use tree_sitter::StreamingIterator;

thread_local! {
    pub static TS_DIAG_PARSER: std::cell::RefCell<tree_sitter::Parser> = {
        let mut parser = tree_sitter::Parser::new();
        if let Some((lang, _)) = crate::queries::get_ts_config("py") {
            let _ = parser.set_language(&lang);
        }
        std::cell::RefCell::new(parser)
    };
    pub static TS_DIAG_QUERY: std::cell::RefCell<Option<tree_sitter::Query>> = std::cell::RefCell::new({
        if let Some((lang, queries)) = crate::queries::get_ts_config("py") {
            let full = queries.join("\n");
            tree_sitter::Query::new(&lang, &full).ok()
        } else {
            None
        }
    });
    pub static TS_DIAG_CURSOR: std::cell::RefCell<tree_sitter::QueryCursor> = std::cell::RefCell::new(tree_sitter::QueryCursor::new());
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HoverLineKind {
    Text,
    Code,
    Separator,
    Header1,
    Header2,
}

pub fn normalize_inline_rst_code(line: &str) -> (String, Vec<(usize, usize)>) {
    let mut out = String::with_capacity(line.len());
    let mut ranges = Vec::new();
    let mut from = 0usize;
    while let Some(open_rel) = line[from..].find("``") {
        let open = from + open_rel;
        out.push_str(&line[from..open]);
        let body_start = open + 2;
        let Some(close_rel) = line[body_start..].find("``") else {
            out.push_str(&line[open..]);
            return (out, ranges);
        };
        let close = body_start + close_rel;
        let start_in_out = out.len();
        out.push_str(&line[body_start..close]);
        let end_in_out = out.len();
        if end_in_out > start_in_out {
            ranges.push((start_in_out, end_in_out));
        }
        from = close + 2;
    }
    out.push_str(&line[from..]);
    (out, ranges)
}

pub fn normalize_rst_roles(line: &str) -> (String, Vec<(usize, usize)>) {
    let mut out = String::with_capacity(line.len());
    let mut ranges = Vec::new();
    let mut i = 0usize;
    while i < line.len() {
        let rest = &line[i..];
        let role_prefix = [
            ":meth:`", ":func:`", ":class:`", ":exc:`", ":attr:`", ":obj:`", ":mod:`",
        ]
        .iter()
        .find(|p| rest.starts_with(**p))
        .copied();
        if let Some(prefix) = role_prefix {
            i += prefix.len();
            if let Some(end_rel) = line[i..].find('`') {
                let raw = &line[i..i + end_rel];
                let mut display = raw;
                if let Some(lt_pos) = raw.find('<') {
                    display = raw[..lt_pos].trim();
                } else if let Some(stripped) = raw.strip_prefix('~') {
                    display = stripped;
                }
                let start = out.len();
                out.push_str(display);
                let end = out.len();
                if end > start {
                    ranges.push((start, end));
                }
                i += end_rel + 1;
                continue;
            }
            out.push_str(prefix);
            continue;
        }
        let ch = rest.chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    (out, ranges)
}

pub fn flatten_rst_roles_and_code(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_role = false;
    let mut in_code = false;
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() && chars[i + 1] == '\n' {
            i += 2;
            while i < chars.len() && chars[i].is_whitespace() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if chars[i] == '`' {
            if i + 1 < chars.len() && chars[i + 1] == '`' {
                in_code = !in_code;
                out.push('`');
                out.push('`');
                i += 2;
                continue;
            } else if !in_code {
                in_role = !in_role;
                out.push('`');
                i += 1;
                continue;
            }
        }
        if chars[i] == '\n' && (in_role || in_code) {
            out.push(' ');
            i += 1;
            while i < chars.len() && chars[i].is_whitespace() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

pub fn parse_param_line(trimmed: &str) -> Option<(String, String, String)> {
    if !trimmed.starts_with(":param ") {
        return None;
    }
    let rest = trimmed.trim_start_matches(":param ").replace("\\*", "*");
    let colon = rest.find(':')?;
    let head = rest[..colon].trim();
    let desc = rest[colon + 1..].trim().to_string();
    if head.is_empty() {
        return None;
    }
    let mut parts = head.split_whitespace().collect::<Vec<_>>();
    if parts.is_empty() {
        return None;
    }
    let name = parts.pop()?.trim().to_string();
    let ty = parts.join(" ").trim().to_string();
    Some((name, ty, desc))
}

fn split_inline_python_after_colon(line: &str) -> Option<(String, String)> {
    let colon = line.find(':')?;
    let head = line[..=colon].trim_end();
    let tail = line[colon + 1..].trim_start();
    if head.is_empty() || tail.is_empty() {
        return None;
    }
    let looks_like_inline_code = (tail.starts_with("for ")
        || tail.starts_with("if ")
        || tail.starts_with("while ")
        || tail.starts_with("try")
        || tail.starts_with("await ")
        || tail.starts_with("return "))
        && (tail.contains(':') || tail.contains('=') || tail.contains('('));
    if !looks_like_inline_code {
        return None;
    }
    Some((head.to_string(), tail.to_string()))
}

fn normalize_coroutine_signature_line(line: &str) -> String {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("def ") || trimmed.starts_with("async def ") {
        return line.to_string();
    }
    let Some(arrow) = trimmed.rfind("->") else {
        return line.to_string();
    };
    let ret = trimmed[arrow + 2..].trim();
    let is_async_ret =
        ret.contains("CoroutineType") || ret.contains("Coroutine") || ret.contains("Awaitable");
    if !is_async_ret {
        return line.to_string();
    }
    let leading = line.len() - trimmed.len();
    let mut out = String::with_capacity(line.len() + 6);
    out.push_str(&line[..leading]);
    out.push_str("async ");
    out.push_str(trimmed);
    out
}

pub fn normalize_python_hover_doc(msg: &str) -> (String, Vec<HoverLineKind>, Vec<(usize, usize)>) {
    let mut out = String::new();
    let mut kinds = Vec::new();
    let mut inline_code_ranges = Vec::new();
    let mut parameters_header_added = false;
        let flat_msg = flatten_rst_roles_and_code(&msg.replace('\r', "").replace('\u{a0}', " ").replace('\u{200b}', ""));
    let lines: Vec<&str> = flat_msg.lines().collect();
    let mut i = 0usize;
    let mut in_fence = false;
    let mut in_fence_is_code = false;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        if trimmed.starts_with("```") {
            if in_fence {
                while kinds.last() == Some(&HoverLineKind::Code) && out.ends_with("\n\n") {
                    out.pop();
                    kinds.pop();
                }
                if !out.ends_with("\n\n") && !out.is_empty() {
                    out.push('\n');
                    kinds.push(HoverLineKind::Text);
                }
                in_fence = false;
            } else {
                in_fence = true;
                let lang = trimmed.strip_prefix("```").unwrap_or("").trim();
                in_fence_is_code = lang.is_empty() || lang == "python" || lang == "py";
            }
            i += 1;
            continue;
        }

        if in_fence {
            out.push_str(line);
            out.push('\n');
            if in_fence_is_code {
                kinds.push(HoverLineKind::Code);
            } else {
                kinds.push(HoverLineKind::Text);
            }
            i += 1;
            continue;
        }

        if trimmed.starts_with(".. code-block:: python") || trimmed.starts_with(".. code:: python")
        {
            if !out.ends_with("\n\n") && !out.is_empty() {
                out.push('\n');
                kinds.push(HoverLineKind::Text);
            }
            let base_indent = line.len() - line.trim_start().len();
            i += 1;
            if i < lines.len() && lines[i].trim().is_empty() {
                i += 1;
            }
            while i < lines.len() {
                let code_line = lines[i];
                if code_line.trim().is_empty() {
                    out.push('\n');
                    kinds.push(HoverLineKind::Code);
                    i += 1;
                    continue;
                }
                let current_indent = code_line.len() - code_line.trim_start().len();
                if current_indent > base_indent {
                    let stripped = if current_indent >= base_indent + 4 {
                        &code_line[base_indent + 4..]
                    } else {
                        code_line.trim_start()
                    };
                    out.push_str(stripped);
                    out.push('\n');
                    kinds.push(HoverLineKind::Code);
                    i += 1;
                    continue;
                }
                break;
            }
            while kinds.last() == Some(&HoverLineKind::Code) && out.ends_with("\n\n") {
                out.pop();
                kinds.pop();
            }
            if !out.ends_with("\n\n") && !out.is_empty() {
                out.push('\n');
                kinds.push(HoverLineKind::Text);
            }
            continue;
        }

        if trimmed.ends_with("::") && !trimmed.starts_with(".. ") {
            let base_indent = line.len() - line.trim_start().len();
            let clean = trimmed.strip_suffix("::").unwrap_or(trimmed);
            let line_start = out.len();
            let (roles_line, mut role_ranges) = normalize_rst_roles(&clean.replace("\\*", "*"));
            let (normalized_line, mut ranges) = normalize_inline_rst_code(&roles_line);

            out.push_str(normalized_line.trim_end());
            out.push_str(":\n\n");
            kinds.push(HoverLineKind::Text);
            kinds.push(HoverLineKind::Text);

            ranges.append(&mut role_ranges);
            for (s, e) in ranges {
                inline_code_ranges.push((line_start + s, line_start + e));
            }

            i += 1;
            if i < lines.len() && lines[i].trim().is_empty() {
                i += 1;
            }
            while i < lines.len() {
                let code_line = lines[i];
                if code_line.trim().is_empty() {
                    out.push('\n');
                    kinds.push(HoverLineKind::Code);
                } else {
                    let current_indent = code_line.len() - code_line.trim_start().len();
                    if current_indent > base_indent {
                        let stripped = if current_indent >= base_indent + 4 {
                            &code_line[base_indent + 4..]
                        } else {
                            code_line.trim_start()
                        };
                        out.push_str(stripped);
                        out.push('\n');
                        kinds.push(HoverLineKind::Code);
                    } else {
                        break;
                    }
                }
                i += 1;
            }
            while kinds.last() == Some(&HoverLineKind::Code) && out.ends_with("\n\n") {
                out.pop();
                kinds.pop();
            }
            if !out.ends_with("\n\n") && !out.is_empty() {
                out.push('\n');
                kinds.push(HoverLineKind::Text);
            }
            continue;
        }

        if trimmed.chars().all(|c| c == '-') && trimmed.len() >= 5 {
            out.push_str("---");
            out.push('\n');
            kinds.push(HoverLineKind::Separator);
            i += 1;
            continue;
        }

        if trimmed == ".. warning::" {
            out.push_str("Warning");
            out.push('\n');
            kinds.push(HoverLineKind::Header2);
            i += 1;
            continue;
        }

        if let Some(stripped) = trimmed.strip_prefix(":return:") {
            out.push_str("Returns");
            out.push('\n');
            kinds.push(HoverLineKind::Header1);
            let trimmed_rest = stripped.trim();
            if !trimmed_rest.is_empty() {
                let _line_start = out.len();
                let (roles_line, mut role_ranges) =
                    normalize_rst_roles(&trimmed_rest.replace("\\*", "*"));
                let (normalized_line, mut ranges) = normalize_inline_rst_code(&roles_line);
                out.push_str(normalized_line.trim_end());
                out.push('\n');
                kinds.push(HoverLineKind::Text);
                ranges.append(&mut role_ranges);
                for (s, e) in ranges {
                    inline_code_ranges.push((_line_start + s, _line_start + e));
                }
            }
            i += 1;
            continue;
        }

        if let Some(stripped) = trimmed.strip_prefix(".. versionchanged::") {
            out.push_str("versionchanged");
            out.push('\n');
            kinds.push(HoverLineKind::Header2);
            let trimmed_rest = stripped.trim();
            if !trimmed_rest.is_empty() {
                let _line_start = out.len();
                let (roles_line, mut role_ranges) =
                    normalize_rst_roles(&trimmed_rest.replace("\\*", "*"));
                let (normalized_line, mut ranges) = normalize_inline_rst_code(&roles_line);
                out.push_str(normalized_line.trim_end());
                out.push('\n');
                kinds.push(HoverLineKind::Text);
                ranges.append(&mut role_ranges);
                for (s, e) in ranges {
                    inline_code_ranges.push((_line_start + s, _line_start + e));
                }
            }
            i += 1;
            continue;
        }

        if let Some((name, ty, desc)) = parse_param_line(trimmed) {
            if !parameters_header_added {
                out.push_str("Parameters");
                out.push('\n');
                kinds.push(HoverLineKind::Header1);
                parameters_header_added = true;
            }
            if ty.is_empty() {
                out.push_str(&format!("{}:", name));
            } else {
                out.push_str(&format!("{}: {}", name, ty));
            }
            out.push('\n');
            kinds.push(HoverLineKind::Text);

            if !desc.is_empty() {
                let (roles_line, mut role_ranges) = normalize_rst_roles(&desc);
                let (normalized_line, mut ranges) = normalize_inline_rst_code(&roles_line);
                out.push_str("    ");
                let desc_start = out.len();
                out.push_str(normalized_line.trim_end());
                out.push('\n');
                kinds.push(HoverLineKind::Text);
                ranges.append(&mut role_ranges);
                for (s, e) in ranges {
                    inline_code_ranges.push((desc_start + s, desc_start + e));
                }
            }
            i += 1;
            continue;
        }

        if let Some((head, code_tail)) = split_inline_python_after_colon(trimmed) {
            out.push_str(&head);
            out.push('\n');
            kinds.push(HoverLineKind::Text);
            out.push_str("    ");
            out.push_str(&code_tail);
            out.push('\n');
            kinds.push(HoverLineKind::Code);
            i += 1;
            continue;
        }

                let normalized_src_line = normalize_coroutine_signature_line(line);
        let line_start = out.len();
        let (roles_line, mut role_ranges) =
            normalize_rst_roles(&normalized_src_line.replace("\\*", "*"));
        let (normalized_line, mut ranges) = normalize_inline_rst_code(&roles_line);
        let trimmed_norm = normalized_line.trim_end();

                let mut shift = 0;
        let mut replaced_entirely = false;
        let mut extra_module_line = None;
        let mut is_header2 = false;
        let mut is_header1 = false;

        let mut s = trimmed_norm;
        if let Some(rem) = trimmed_norm.strip_prefix("## ") {
            shift = 3;
            s = rem;
            is_header2 = true;
        } else if let Some(rem) = trimmed_norm.strip_prefix("# ") {
            shift = 2;
            s = rem;
            is_header1 = true;
        }

        let mut header_text = s.to_string();
        let is_ru = s.starts_with("Атрибут класса ");
        let is_en = s.starts_with("Class attribute ");

        if is_ru || is_en {
            let prefix_len = if is_ru { "Атрибут класса ".len() } else { "Class attribute ".len() };
            let separator = if is_ru { " в " } else { " of " };

            if let Some(v_idx) = s.rfind(separator) {
                if v_idx > prefix_len {
                    let clean_name = s[prefix_len..v_idx].trim_matches('`').trim();
                    let clean_path = s[v_idx + separator.len()..].trim_matches('`').trim();

                                        if let Some(dot_idx) = clean_path.rfind('.') {
                        let module = &clean_path[..dot_idx];
                        let cls = &clean_path[dot_idx + 1..];
                        extra_module_line = Some(module.to_string());
                        header_text = format!("Class attribute {} of {}", clean_name, cls);
                    } else {
                        header_text = format!("Class attribute {} of {}", clean_name, clean_path);
                    }
                    replaced_entirely = true;
                    is_header2 = true;
                    is_header1 = false;
                }
            }
        }

        if let Some(mod_line) = extra_module_line {
            out.push_str(&mod_line);
            out.push('\n');
            kinds.push(HoverLineKind::Text);
        }

        if is_header2 {
            out.push_str(&header_text);
            out.push('\n');
            kinds.push(HoverLineKind::Header2);
            if replaced_entirely {
                out.push_str("---\n");
                kinds.push(HoverLineKind::Separator);
            }
        } else if is_header1 {
            out.push_str(&header_text);
            out.push('\n');
            kinds.push(HoverLineKind::Header1);
        } else {
            out.push_str(&header_text);
            out.push('\n');
            kinds.push(HoverLineKind::Text);
        }

        if replaced_entirely {
            ranges.clear();
            role_ranges.clear();
        } else if shift > 0 {
            for r in &mut ranges {
                r.0 = r.0.saturating_sub(shift);
                r.1 = r.1.saturating_sub(shift);
            }
            for r in &mut role_ranges {
                r.0 = r.0.saturating_sub(shift);
                r.1 = r.1.saturating_sub(shift);
            }
        }

        ranges.append(&mut role_ranges);
        for (start, end) in ranges {
            if start < end {
                inline_code_ranges.push((line_start + start, line_start + end));
            }
        }
        i += 1;
    }

    while out.ends_with('\n') {
        out.pop();
    }
    (out, kinds, inline_code_ranges)
}

pub fn ts_capture_color(name: &str) -> Option<[f32; 4]> {
    match name {
        "fg" | "property" | "py_assign" | "variable" => Some([0.972, 0.972, 0.949, 1.0]),
        "string" => Some([0.945, 0.980, 0.549, 1.0]),
        "comment" => Some([0.384, 0.447, 0.643, 1.0]),
        "function" | "py_function" => Some([0.313, 0.980, 0.482, 1.0]),
        "keyword.control" | "operator" | "boolean" => Some([1.0, 0.474, 0.776, 1.0]),
        "keyword" | "subst" | "type" | "function.builtin" => Some([0.545, 0.913, 0.992, 1.0]),
        "class_name" => Some([0.45, 0.85, 0.90, 1.0]),
        "constant" | "number" => Some([0.741, 0.576, 0.976, 1.0]),
        "parameter" => Some([0.973, 0.584, 0.502, 1.0]),
        "py_builtin_or_func" => Some([0.313, 0.980, 0.482, 1.0]),
        _ => None,
    }
}

pub fn push_python_ts_spans(
    code: &str,
    global_start: usize,
    spans: &mut Vec<crate::highlighter::ColorSpan>,
) {
    let mut best_spans: HashMap<(usize, usize), (u8, [f32; 4])> = HashMap::new();

    TS_DIAG_PARSER.with(|p_cell| {
        TS_DIAG_QUERY.with(|q_cell| {
            TS_DIAG_CURSOR.with(|c_cell| {
                let mut parser = p_cell.borrow_mut();
                let query_opt = q_cell.borrow();
                let mut cursor = c_cell.borrow_mut();

                if let Some(query) = query_opt.as_ref() {
                    if let Some(tree) = parser.parse(code, None) {
                        let mut matches = cursor.matches(query, tree.root_node(), code.as_bytes());
                        while let Some(m) = matches.next() {
                            for cap in m.captures {
                                let name = query.capture_names()[cap.index as usize];
                                let Some(color) = ts_capture_color(name) else {
                                    continue;
                                };
                                let prio = match name {
                                    "py_function" | "function" | "py_builtin_or_func" => 10,
                                    "keyword" | "keyword.control" | "operator" => 8,
                                    "string" | "number" => 8,
                                    "type" | "class_name" => 8,
                                    "parameter" => 5,
                                    "property" | "variable" | "py_assign" => 1,
                                    _ => 0,
                                };
                                let key = (
                                    global_start + cap.node.start_byte(),
                                    global_start + cap.node.end_byte(),
                                );
                                let entry = best_spans.entry(key).or_insert((prio, color));
                                if prio >= entry.0 {
                                    *entry = (prio, color);
                                }
                            }
                        }
                    }
                }
            })
        })
    });

    for ((start, end), (_, color)) in best_spans {
        spans.push(crate::highlighter::ColorSpan { start, end, color });
    }
}

pub fn highlight_python_hover_doc(
    raw_msg: &str,
) -> (
    String,
    Vec<crate::highlighter::ColorSpan>,
    Vec<HoverLineKindPublic>,
    Vec<(usize, usize)>,
) {
    let text_light = crate::highlighter::DRACULA_FG;
    let ty = crate::highlighter::DRACULA_CYAN;
    let neutral = crate::highlighter::DRACULA_FG;
    let param = crate::highlighter::DRACULA_ORANGE;

        let (msg, mut line_kinds, inline_code_ranges) = normalize_python_hover_doc(raw_msg);
    let lines: Vec<&str> = msg.split('\n').collect();
    let mut line_starts = Vec::with_capacity(lines.len());
    let mut at = 0usize;
    for line in &lines {
        line_starts.push(at);
        at += line.len() + 1;
    }
    let mut spans = Vec::new();

    // signature block -> tree-sitter python highlight
    let mut sig_start_line = None;
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("def ")
            || trimmed.starts_with("async def ")
            || trimmed.contains(" def ")
            || trimmed.contains(" async def ")
        {
            sig_start_line = Some(idx);
            break;
        }
        if !trimmed.is_empty() && line_kinds.get(idx) != Some(&HoverLineKind::Text) {
            break;
        }
    }
    if let Some(start_line) = sig_start_line {
        let mut decorator_start_line = start_line;
        while decorator_start_line > 0 {
            let prev = lines[decorator_start_line - 1].trim_start();
            if prev.starts_with('@') {
                decorator_start_line -= 1;
            } else {
                break;
            }
        }
        for line_no in decorator_start_line..start_line {
            let line = lines[line_no];
            let line_offset = line_starts[line_no];
            let leading_ws = line.len().saturating_sub(line.trim_start().len());
            let trimmed = line.trim_start();
            if !trimmed.starts_with('@') {
                continue;
            }
            let at_pos = line_offset + leading_ws;
            spans.push(crate::highlighter::ColorSpan {
                start: at_pos,
                end: at_pos + 1,
                color: crate::highlighter::DRACULA_PINK,
            });
            let name = trimmed[1..]
                .split(|c: char| c == '(' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .trim();
            if !name.is_empty() {
                let name_start = at_pos + 1;
                spans.push(crate::highlighter::ColorSpan {
                    start: name_start,
                    end: name_start + name.len(),
                    color: crate::highlighter::DRACULA_GREEN,
                });
            }
        }

        let mut end_line = start_line;
        for i in start_line..lines.len() {
            let trimmed = lines[i].trim_end();
            end_line = i;
            if trimmed.ends_with(") -> Unknown")
                || trimmed.ends_with(')')
                || lines[i].contains(") ->")
            {
                break;
            }
        }
        let def_shift = lines[start_line]
            .find("async def ")
            .or_else(|| lines[start_line].find("def "))
            .unwrap_or(0);
        let start = line_starts[start_line] + def_shift;
        let end = if end_line + 1 < line_starts.len() {
            line_starts[end_line + 1] - 1
        } else {
            msg.len()
        };
                if start < end && end <= msg.len() {
            let mut sig_code = msg[start..end].to_string();
            if !sig_code.trim_end().ends_with(':') {
                sig_code.push(':');
            }
            push_python_ts_spans(&sig_code, start, &mut spans);
            color_keyword_args_orange(&sig_code, start, &mut spans);
        }

        let mut signature_brackets = Vec::new();
        for line_no in start_line..=end_line {
            for (idx, ch) in lines[line_no].char_indices() {
                if ch == '[' || ch == ']' {
                    let abs = line_starts[line_no] + idx;
                    signature_brackets.push((abs, abs + 1));
                }
            }
        }
                if !signature_brackets.is_empty() {
            force_color_on_ranges(&mut spans, &signature_brackets, neutral);
        }
    }

    let mut assignment_start = None;
    let mut saw_sep_for_assignment = false;
    for (idx, kind) in line_kinds.iter().enumerate() {
        if *kind == HoverLineKind::Separator {
            saw_sep_for_assignment = true;
            continue;
        }
        if saw_sep_for_assignment && *kind == HoverLineKind::Text && !lines[idx].trim().is_empty() {
            let trimmed = lines[idx].trim();
            let is_assignment = trimmed.contains('=') 
                && trimmed.chars().next().map_or(false, |c| c.is_ascii_alphabetic() || c == '_');
            if is_assignment {
                assignment_start = Some(idx);
            }
            break;
        }
    }

    if let Some(start_idx) = assignment_start {
        for k in line_kinds.iter_mut().skip(start_idx) {
            *k = HoverLineKind::Code;
        }
    }

    // code blocks -> tree-sitter python highlight
    let mut i = 0usize;
    while i < lines.len() {
        if line_kinds.get(i) == Some(&HoverLineKind::Code) {
            let block_start = i;
            while i < lines.len() && line_kinds.get(i) == Some(&HoverLineKind::Code) {
                i += 1;
            }
            let block_end_line = i.saturating_sub(1);
            let start = line_starts[block_start];
                        let end = if block_end_line + 1 < line_starts.len() {
                line_starts[block_end_line + 1] - 1
            } else {
                msg.len()
            };
            if start < end && end <= msg.len() {
                let code_chunk = &msg[start..end];
                push_python_ts_spans(code_chunk, start, &mut spans);
                color_keyword_args_orange(code_chunk, start, &mut spans);
            }
            continue;
        }
        i += 1;
    }

    // light text lines after separator
    let mut saw_separator = false;
    for (line_no, line) in lines.iter().enumerate() {
        let line_start = line_starts[line_no];
        let line_end = line_start + line.len();
        let trimmed = line.trim_start();
        let is_blank = trimmed.is_empty();
        let kind = line_kinds
            .get(line_no)
            .copied()
            .unwrap_or(HoverLineKind::Text);

        if kind == HoverLineKind::Separator
            || kind == HoverLineKind::Header1
            || kind == HoverLineKind::Header2
        {
            saw_separator = true;
        }

                if saw_separator && kind == HoverLineKind::Text && !is_blank {
            spans.push(crate::highlighter::ColorSpan {
                start: line_start,
                end: line_end,
                color: text_light,
            });
        }

                if kind == HoverLineKind::Header2 && line.starts_with("Class attribute ") {
            if let Some(of_idx) = line.find(" of ") {
                spans.push(crate::highlighter::ColorSpan {
                    start: line_start + 16,
                    end: line_start + of_idx,
                    color: crate::highlighter::DRACULA_PINK,
                });
                spans.push(crate::highlighter::ColorSpan {
                    start: line_start + of_idx + 4,
                    end: line_end,
                    color: ty,
                });
            }
        }

        if kind == HoverLineKind::Text {
            if let Some(colon_pos) = line.find(':') {
                let lhs = line[..colon_pos].trim();
                let is_indented = line.starts_with(' ') || line.starts_with('\t');
                if !lhs.is_empty()
                    && !is_indented
                    && lhs
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '*')
                    && saw_separator
                {
                    let lhs_start = line_start + line.find(lhs).unwrap_or(0);
                    let lhs_end = lhs_start + lhs.len();
                    spans.push(crate::highlighter::ColorSpan {
                        start: lhs_start,
                        end: lhs_end,
                        color: param,
                    });
                    let rhs = line[colon_pos + 1..].trim_start();
                    if !rhs.is_empty() {
                        let rhs_start =
                            line_start + colon_pos + 1 + (line[colon_pos + 1..].len() - rhs.len());
                        let rhs_end = rhs_start
                            + rhs
                                .chars()
                                .take_while(|c| !c.is_whitespace())
                                .map(|c| c.len_utf8())
                                .sum::<usize>();
                        if rhs_end > rhs_start {
                            spans.push(crate::highlighter::ColorSpan {
                                start: rhs_start,
                                end: rhs_end,
                                color: ty,
                            });
                        }
                    }
                }
            }
        }
    }

        for &(start, end) in &inline_code_ranges {
        if end > start && end <= msg.len() {
            let code_chunk = &msg[start..end];
            push_python_ts_spans(code_chunk, start, &mut spans);
            color_keyword_args_orange(code_chunk, start, &mut spans);
        }
    }

    let public_kinds = line_kinds
        .into_iter()
        .map(|k| match k {
            HoverLineKind::Text => HoverLineKindPublic::Text,
            HoverLineKind::Code => HoverLineKindPublic::Code,
            HoverLineKind::Separator => HoverLineKindPublic::Separator,
            HoverLineKind::Header1 => HoverLineKindPublic::Header1,
            HoverLineKind::Header2 => HoverLineKindPublic::Header2,
        })
        .collect();

    (msg, spans, public_kinds, inline_code_ranges)
}

fn force_color_on_ranges(
    spans: &mut Vec<crate::highlighter::ColorSpan>,
    ranges: &[(usize, usize)],
    color: [f32; 4],
) {
    let mut out = Vec::with_capacity(spans.len() + ranges.len());
    for span in spans.drain(..) {
        let mut pieces = vec![span];
        for &(force_start, force_end) in ranges {
            let mut next = Vec::with_capacity(pieces.len() + 1);
            for piece in pieces {
                if piece.end <= force_start || piece.start >= force_end {
                    next.push(piece);
                    continue;
                }
                if piece.start < force_start {
                    next.push(crate::highlighter::ColorSpan {
                        start: piece.start,
                        end: force_start,
                        color: piece.color,
                    });
                }
                if piece.end > force_end {
                    next.push(crate::highlighter::ColorSpan {
                        start: force_end,
                        end: piece.end,
                        color: piece.color,
                    });
                }
            }
            pieces = next;
        }
        out.extend(pieces);
    }
    out.extend(
        ranges
            .iter()
            .map(|&(start, end)| crate::highlighter::ColorSpan { start, end, color }),
    );
        *spans = out;
}

fn color_keyword_args_orange(code: &str, global_start: usize, spans: &mut Vec<crate::highlighter::ColorSpan>) {
    let mut paren_depth = 0;
    let mut in_string = false;
    let mut string_char = ' ';
    let mut i = 0;
    let chars: Vec<char> = code.chars().collect();
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            if c == '\\' { i += 1; }
            else if c == string_char { in_string = false; }
        } else {
            if c == '"' || c == '\'' {
                in_string = true;
                string_char = c;
            } else if c == '(' {
                paren_depth += 1;
            } else if c == ')' {
                paren_depth -= 1;
            } else if c == '=' && paren_depth > 0 {
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    i += 1;
                } else if i > 0 && (chars[i-1] == '=' || chars[i-1] == '!' || chars[i-1] == '<' || chars[i-1] == '>') {
                    // Do nothing
                } else {
                    let mut id_end = i;
                    while id_end > 0 && chars[id_end - 1].is_whitespace() {
                        id_end -= 1;
                    }
                    let mut id_start = id_end;
                    while id_start > 0 && (chars[id_start - 1].is_alphanumeric() || chars[id_start - 1] == '_') {
                        id_start -= 1;
                    }
                                        if id_start < id_end && !chars[id_start].is_ascii_digit() {
                        let byte_start = chars[..id_start].iter().map(|c| c.len_utf8()).sum::<usize>();
                        let byte_end = chars[..id_end].iter().map(|c| c.len_utf8()).sum::<usize>();
                        let span_start = global_start + byte_start;
                        let span_end = global_start + byte_end;
                        spans.retain(|s| !(s.start < span_end && s.end > span_start));
                        spans.push(crate::highlighter::ColorSpan {
                            start: span_start,
                            end: span_end,
                            color: crate::highlighter::DRACULA_ORANGE,
                        });
                    }
                }
            }
        }
        i += 1;
    }
}
