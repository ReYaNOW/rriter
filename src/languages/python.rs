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
        let role_prefix =[":meth:`", ":func:`", ":class:`", ":exc:`", ":attr:`", ":obj:`", ":mod:`"]
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
        if chars[i] == '\\' && i + 1 < chars.len() && chars[i+1] == '\n' {
            i += 2;
            while i < chars.len() && chars[i].is_whitespace() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if chars[i] == '`' {
            if i + 1 < chars.len() && chars[i+1] == '`' {
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

pub fn normalize_python_hover_doc(msg: &str) -> (String, Vec<HoverLineKind>, Vec<(usize, usize)>) {
    let mut out = String::new();
    let mut kinds = Vec::new();
    let mut inline_code_ranges = Vec::new();
    let mut parameters_header_added = false;
    let flat_msg = flatten_rst_roles_and_code(&msg.replace('\r', ""));
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

        if trimmed.starts_with(".. code-block:: python") || trimmed.starts_with(".. code:: python") {
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
            if i < lines.len() && lines[i].trim().is_empty() { i += 1; }
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
                let (roles_line, mut role_ranges) = normalize_rst_roles(&trimmed_rest.replace("\\*", "*"));
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
                let (roles_line, mut role_ranges) = normalize_rst_roles(&trimmed_rest.replace("\\*", "*"));
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

        let line_start = out.len();
        let (roles_line, mut role_ranges) = normalize_rst_roles(&line.replace("\\*", "*"));
        let (normalized_line, mut ranges) = normalize_inline_rst_code(&roles_line);
        let trimmed_norm = normalized_line.trim_end();
        if let Some(s) = trimmed_norm.strip_prefix("## ") {
            out.push_str(s);
            out.push('\n');
            kinds.push(HoverLineKind::Header2);
        } else if let Some(s) = trimmed_norm.strip_prefix("# ") {
            out.push_str(s);
            out.push('\n');
            kinds.push(HoverLineKind::Header1);
        } else {
            out.push_str(trimmed_norm);
            out.push('\n');
            kinds.push(HoverLineKind::Text);
        }
        ranges.append(&mut role_ranges);
        for (start, end) in ranges {
            inline_code_ranges.push((line_start + start, line_start + end));
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
    let mut best_spans: HashMap<(usize, usize), (u8,[f32; 4])> = HashMap::new();

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
    })})});

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
    let text_light =[0.86, 0.87, 0.90, 1.0];
    let ty =[0.545, 0.913, 0.992, 1.0];
    let param =[0.973, 0.584, 0.502, 1.0];

    let (msg, line_kinds, inline_code_ranges) = normalize_python_hover_doc(raw_msg);
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
        let def_shift = lines[start_line].find("def ").unwrap_or(0);
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
                push_python_ts_spans(&msg[start..end], start, &mut spans);
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
        let kind = line_kinds.get(line_no).copied().unwrap_or(HoverLineKind::Text);

        if kind == HoverLineKind::Separator || kind == HoverLineKind::Header1 || kind == HoverLineKind::Header2 {
            saw_separator = true;
        }

        if saw_separator && kind == HoverLineKind::Text && !is_blank {
            spans.push(crate::highlighter::ColorSpan {
                start: line_start,
                end: line_end,
                color: text_light,
            });
        }

        if kind == HoverLineKind::Text {
            if let Some(colon_pos) = line.find(':') {
                let lhs = line[..colon_pos].trim();
                let is_indented = line.starts_with(' ') || line.starts_with('\t');
                if !lhs.is_empty()
                    && !is_indented
                    && lhs.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '*')
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
                        let rhs_start = line_start + colon_pos + 1 + (line[colon_pos + 1..].len() - rhs.len());
                        let rhs_end = rhs_start + rhs
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
            push_python_ts_spans(&msg[start..end], start, &mut spans);
        }
    }

    let public_kinds = line_kinds.into_iter().map(|k| match k {
        HoverLineKind::Text => HoverLineKindPublic::Text,
        HoverLineKind::Code => HoverLineKindPublic::Code,
        HoverLineKind::Separator => HoverLineKindPublic::Separator,
        HoverLineKind::Header1 => HoverLineKindPublic::Header1,
        HoverLineKind::Header2 => HoverLineKindPublic::Header2,
    }).collect();

    (msg, spans, public_kinds, inline_code_ranges)
}
