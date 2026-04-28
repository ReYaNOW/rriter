use crate::lsp::HoverLineKindPublic;
use std::collections::HashMap;
use tree_sitter::StreamingIterator;

pub const DOCSTRING_TEXT: [f32; 4] = crate::highlighter::DRACULA_COMMENT;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImportBlock {
    pub start: usize,
    pub end: usize,
    pub keyword_start: usize,
    pub keyword_end: usize,
    pub line_count: usize,
}

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

pub fn import_blocks(text: &str) -> Vec<ImportBlock> {
    let mut blocks = Vec::new();
    let mut current: Option<ImportBlock> = None;
    let mut pending_blank_lines = 0usize;
    let mut offset = 0usize;
    let mut continuing = false;
    let mut paren_depth = 0i32;

    for raw_line in text.split_inclusive('\n') {
        let line_start = offset;
        offset += raw_line.len();
        let line = raw_line.trim_end_matches('\n').trim_end_matches('\r');
        let line_end = line_start + line.len();
        let leading = line.len().saturating_sub(line.trim_start().len());
        let trimmed = line.trim_start();

        if trimmed.is_empty() && current.is_some() {
            pending_blank_lines += 1;
            continue;
        }

        if let Some(keyword_len) = python_import_keyword_len(trimmed) {
            let keyword_start = line_start + leading;
            if let Some(block) = &mut current {
                block.end = line_end;
                block.line_count += pending_blank_lines + 1;
            } else {
                current = Some(ImportBlock {
                    start: line_start,
                    end: line_end,
                    keyword_start,
                    keyword_end: keyword_start + keyword_len,
                    line_count: 1,
                });
            }
            pending_blank_lines = 0;
            update_python_import_continuation(trimmed, &mut paren_depth, &mut continuing);
            if !continuing {
                paren_depth = 0;
            }
            continue;
        }

        if continuing && !trimmed.is_empty() {
            if let Some(block) = &mut current {
                block.end = line_end;
                block.line_count += pending_blank_lines + 1;
            }
            pending_blank_lines = 0;
            update_python_import_continuation(trimmed, &mut paren_depth, &mut continuing);
            continue;
        }

        pending_blank_lines = 0;
        continuing = false;
        paren_depth = 0;
        finish_import_block(&mut current, &mut blocks);
    }

    finish_import_block(&mut current, &mut blocks);
    blocks
}

pub fn push_docstring_highlight_spans(
    source: &str,
    start: usize,
    end: usize,
    spans: &mut Vec<crate::highlighter::ColorSpan>,
) {
    if start >= end || end > source.len() {
        return;
    }
    spans.push(crate::highlighter::ColorSpan {
        start,
        end,
        color: DOCSTRING_TEXT,
    });

    let bytes = source.as_bytes();
    let mut quote_start = start;
    while quote_start < end && bytes[quote_start].is_ascii_alphabetic() {
        quote_start += 1;
    }
    if quote_start >= end {
        return;
    }

    let quote = bytes[quote_start];
    if quote != b'\'' && quote != b'"' {
        return;
    }
    let triple =
        quote_start + 2 < end && bytes[quote_start + 1] == quote && bytes[quote_start + 2] == quote;
    let quote_len = if triple { 3 } else { 1 };
    let content_start = quote_start + quote_len;
    let content_end = end.saturating_sub(quote_len);
    if content_start >= content_end {
        return;
    }

    spans.push(crate::highlighter::ColorSpan {
        start: quote_start,
        end: content_start,
        color: crate::highlighter::DRACULA_COMMENT,
    });
    if content_end < end {
        spans.push(crate::highlighter::ColorSpan {
            start: content_end,
            end,
            color: crate::highlighter::DRACULA_COMMENT,
        });
    }

    let content = &source[content_start..content_end];
    let mut line_offset = content_start;
    for raw_line in content.split_inclusive('\n') {
        let line = raw_line.trim_end_matches('\n').trim_end_matches('\r');
        push_docstring_line_spans(line, line_offset, spans);
        line_offset += raw_line.len();
    }
}

fn python_import_keyword_len(trimmed: &str) -> Option<usize> {
    if trimmed.starts_with("from ") {
        Some("from".len())
    } else if trimmed.starts_with("import ") {
        Some("import".len())
    } else {
        None
    }
}

fn update_python_import_continuation(trimmed: &str, paren_depth: &mut i32, continuing: &mut bool) {
    for b in trimmed.bytes() {
        match b {
            b'(' | b'[' | b'{' => *paren_depth += 1,
            b')' | b']' | b'}' => *paren_depth -= 1,
            _ => {}
        }
    }
    *continuing = *paren_depth > 0 || trimmed.ends_with('\\');
}

fn finish_import_block(current: &mut Option<ImportBlock>, blocks: &mut Vec<ImportBlock>) {
    if let Some(block) = current.take() {
        if block.line_count >= 2 && block.end > block.start {
            blocks.push(block);
        }
    }
}

fn push_docstring_line_spans(
    line: &str,
    line_start: usize,
    spans: &mut Vec<crate::highlighter::ColorSpan>,
) {
    let trimmed = line.trim_start();
    let leading = line.len().saturating_sub(trimmed.len());
    let header = matches!(
        trimmed,
        "Args:"
            | "Arguments:"
            | "Parameters:"
            | "Returns:"
            | "Raises:"
            | "Yields:"
            | "Examples:"
            | "Notes:"
            | "Note:"
    );
    if header {
        spans.push(crate::highlighter::ColorSpan {
            start: line_start + leading,
            end: line_start + leading + trimmed.len(),
            color: crate::highlighter::DRACULA_CYAN,
        });
    }

    if let Some(rest) = trimmed.strip_prefix(":param ") {
        let role_start = line_start + leading;
        let role_end = role_start + ":param".len();
        spans.push(crate::highlighter::ColorSpan {
            start: role_start,
            end: role_end,
            color: crate::highlighter::DRACULA_CYAN,
        });
        if let Some(colon) = rest.find(':') {
            let name_start = role_start + ":param ".len();
            spans.push(crate::highlighter::ColorSpan {
                start: name_start,
                end: name_start + colon,
                color: crate::highlighter::DRACULA_ORANGE,
            });
        }
    } else if trimmed.starts_with(":return") || trimmed.starts_with(":raises ") {
        let role_len = trimmed
            .find(':')
            .unwrap_or(trimmed.len())
            .max(":return".len().min(trimmed.len()));
        spans.push(crate::highlighter::ColorSpan {
            start: line_start + leading,
            end: line_start + leading + role_len,
            color: crate::highlighter::DRACULA_CYAN,
        });
    }

    let mut search_from = 0usize;
    while let Some(open_rel) = line[search_from..].find("``") {
        let open = search_from + open_rel;
        let body_start = open + 2;
        let Some(close_rel) = line[body_start..].find("``") else {
            break;
        };
        let close = body_start + close_rel;
        if close > body_start {
            spans.push(crate::highlighter::ColorSpan {
                start: line_start + body_start,
                end: line_start + close,
                color: crate::highlighter::DRACULA_CYAN,
            });
        }
        search_from = close + 2;
    }
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
            ":meth:`", ":func:`", ":class:`", ":exc:`", ":attr:`", ":obj:`", ":mod:`", ":data:`",
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
            if i + 2 < chars.len() && chars[i + 1] == '`' && chars[i + 2] == '`' {
                out.push('`');
                out.push('`');
                out.push('`');
                i += 3;
                continue;
            } else if i + 1 < chars.len() && chars[i + 1] == '`' {
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
    let flat_msg = flatten_rst_roles_and_code(
        &msg.replace('\r', "")
            .replace('\u{a0}', " ")
            .replace('\u{200b}', ""),
    );
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

        if trimmed == "Args:" || trimmed == "Arguments:" || trimmed == "Keyword Args:" {
            out.push_str("Parameters");
            out.push('\n');
            kinds.push(HoverLineKind::Header1);
            parameters_header_added = true;
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
        let is_ru_attr = s.starts_with("Атрибут класса ");
        let is_en_attr = s.starts_with("Class attribute ");
        let is_en_param = s.starts_with("Parameter ");
        let is_ru_var = s.starts_with("Переменная ");
        let is_en_var = s.starts_with("Variable ");

        let is_ru = is_ru_attr || is_ru_var;
        let is_en = is_en_attr || is_en_var || is_en_param;

        if is_ru || is_en {
            let prefix_len = if is_ru_attr {
                "Атрибут класса ".len()
            } else if is_en_attr {
                "Class attribute ".len()
            } else if is_en_param {
                "Parameter ".len()
            } else if is_ru_var {
                "Переменная ".len()
            } else {
                "Variable ".len()
            };
            let separator = if is_ru { " в " } else { " of " };

            if let Some(v_idx) = s.rfind(separator) {
                if v_idx > prefix_len {
                    let clean_name = s[prefix_len..v_idx].trim_matches('`').trim();
                    let clean_path = s[v_idx + separator.len()..].trim_matches('`').trim();

                    if is_en_param {
                        if let Some((owner_prefix, method)) = clean_path.rsplit_once('.') {
                            if let Some((module, cls)) = owner_prefix.rsplit_once('.') {
                                extra_module_line = Some(module.to_string());
                                header_text =
                                    format!("Parameter {} of {}.{}", clean_name, cls, method);
                            } else {
                                header_text = format!("Parameter {} of {}", clean_name, clean_path);
                            }
                        } else {
                            header_text = format!("Parameter {} of {}", clean_name, clean_path);
                        }
                    } else if let Some(dot_idx) = clean_path.rfind('.') {
                        let module = &clean_path[..dot_idx];
                        let cls = &clean_path[dot_idx + 1..];
                        extra_module_line = Some(module.to_string());
                        let kind = if is_ru_attr || is_en_attr {
                            "Class attribute"
                        } else if is_en_param {
                            "Parameter"
                        } else {
                            "Variable"
                        };
                        header_text = format!("{} {} of {}", kind, clean_name, cls);
                    } else {
                        let kind = if is_ru_attr || is_en_attr {
                            "Class attribute"
                        } else if is_en_param {
                            "Parameter"
                        } else {
                            "Variable"
                        };
                        header_text = format!("{} {} of {}", kind, clean_name, clean_path);
                    }
                    replaced_entirely = true;
                    is_header2 = false;
                    is_header1 = false;
                }
            } else {
                let clean_name = s[prefix_len..].trim_matches('`').trim();
                let kind = if is_ru_attr || is_en_attr {
                    "Class attribute"
                } else if is_en_param {
                    "Parameter"
                } else {
                    "Variable"
                };
                header_text = format!("{} {}", kind, clean_name);
                replaced_entirely = true;
                is_header2 = false;
                is_header1 = false;
            }
        }

        if let Some(mod_line) = extra_module_line {
            out.push_str("[[MODULE]] ");
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
            if replaced_entirely {
                out.push_str("---\n");
                kinds.push(HoverLineKind::Separator);
            }
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
            || trimmed.starts_with("class ")
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
        let mut paren_depth = 0i32;
        let mut bracket_depth = 0i32;
        let mut started = false;
        for i in start_line..lines.len() {
            end_line = i;
            for c in lines[i].chars() {
                if c == '(' {
                    paren_depth += 1;
                    started = true;
                } else if c == ')' {
                    paren_depth -= 1;
                } else if c == '[' {
                    bracket_depth += 1;
                    started = true;
                } else if c == ']' {
                    bracket_depth -= 1;
                }
            }
            if started && paren_depth <= 0 && bracket_depth <= 0 {
                break;
            }
            if !started && lines[i].contains(':') {
                break;
            }
        }
        let def_shift = lines[start_line]
            .find("async def ")
            .or_else(|| lines[start_line].find("def "))
            .or_else(|| lines[start_line].find("class "))
            .unwrap_or(0);
        let start = line_starts[start_line] + def_shift;
        let end = if end_line + 1 < line_starts.len() {
            line_starts[end_line + 1] - 1
        } else {
            msg.len()
        };
        if start < end && end <= msg.len() {
            let sig_code = msg[start..end].to_string();
            let is_class_sig = sig_code.starts_with("class ");
            let mut ts_code = sig_code.clone();
            if !ts_code.trim_end().ends_with(':') {
                ts_code.push(':');
            }
            push_python_ts_spans(&ts_code, start, &mut spans);
            color_keyword_args_orange(&ts_code, start, &mut spans);

            if is_class_sig {
                let open_paren = sig_code.find('(').unwrap_or(sig_code.len());
                let open_bracket = sig_code.find('[').unwrap_or(sig_code.len());
                let name_end = open_paren.min(open_bracket);
                spans.retain(|s| s.start >= start + name_end || s.end <= start);
                spans.push(crate::highlighter::ColorSpan {
                    start,
                    end: start + 5,
                    color: crate::highlighter::DRACULA_PINK,
                });
                let name_range = 6..open_paren;
                let class_name = sig_code[name_range.clone()].trim();
                if !class_name.is_empty() {
                    let name_start = start
                        + name_range.start
                        + sig_code[name_range].find(class_name).unwrap_or(0);
                    spans.push(crate::highlighter::ColorSpan {
                        start: name_start,
                        end: name_start + class_name.len(),
                        color: crate::highlighter::DRACULA_CYAN,
                    });
                }
            }

            for k in line_kinds.iter_mut().take(end_line + 1).skip(start_line) {
                *k = HoverLineKind::Code;
            }
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
            let is_assignment = (trimmed.contains('=') || trimmed.contains(':'))
                && trimmed
                    .chars()
                    .next()
                    .map_or(false, |c| c.is_ascii_alphabetic() || c == '_');
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

        if kind == HoverLineKind::Header2 || kind == HoverLineKind::Text {
            let is_en_attr = line.starts_with("Class attribute ");
            let is_ru_attr = line.starts_with("Атрибут класса ");
            let is_en_param = line.starts_with("Parameter ");
            let is_en_var = line.starts_with("Variable ");
            let is_ru_var = line.starts_with("Переменная ");

            if is_en_attr || is_ru_attr || is_en_param || is_en_var || is_ru_var {
                let separator = if is_en_attr || is_en_var {
                    " of "
                } else if is_en_param {
                    " of "
                } else {
                    " в "
                };
                let prefix_len = if is_en_attr {
                    16
                } else if is_ru_attr {
                    15
                } else if is_en_param {
                    10
                } else if is_en_var {
                    9
                } else {
                    11
                };

                if let Some(of_idx) = line.find(separator) {
                    spans.push(crate::highlighter::ColorSpan {
                        start: line_start + prefix_len,
                        end: line_start + of_idx,
                        color: crate::highlighter::DRACULA_PINK,
                    });
                    spans.push(crate::highlighter::ColorSpan {
                        start: line_start + of_idx + separator.len(),
                        end: line_end,
                        color: ty,
                    });
                } else {
                    spans.push(crate::highlighter::ColorSpan {
                        start: line_start + prefix_len,
                        end: line_end,
                        color: crate::highlighter::DRACULA_PINK,
                    });
                }
            }
        }

        if kind == HoverLineKind::Text {
            if let Some(colon_pos) = line.find(':') {
                let lhs = line[..colon_pos].trim();
                let pre_lhs_len = line.find(lhs).unwrap_or(0);
                let pre_lhs = &line[..pre_lhs_len];
                let is_start_of_line = pre_lhs.chars().all(|c| c.is_whitespace());
                if !lhs.is_empty()
                    && is_start_of_line
                    && lhs
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '*')
                    && saw_separator
                {
                    let lhs_start = line_start + pre_lhs_len;
                    let lhs_end = lhs_start + lhs.len();
                    spans.push(crate::highlighter::ColorSpan {
                        start: lhs_start,
                        end: lhs_end,
                        color: param,
                    });
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

fn color_keyword_args_orange(
    code: &str,
    global_start: usize,
    spans: &mut Vec<crate::highlighter::ColorSpan>,
) {
    let mut paren_depth = 0;
    let mut in_string = false;
    let mut string_char = ' ';
    let mut i = 0;
    let chars: Vec<char> = code.chars().collect();
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            if c == '\\' {
                i += 1;
            } else if c == string_char {
                in_string = false;
            }
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
                } else if i > 0
                    && (chars[i - 1] == '='
                        || chars[i - 1] == '!'
                        || chars[i - 1] == '<'
                        || chars[i - 1] == '>')
                {
                    // Do nothing
                } else {
                    let mut id_end = i;
                    while id_end > 0 && chars[id_end - 1].is_whitespace() {
                        id_end -= 1;
                    }
                    let mut id_start = id_end;
                    while id_start > 0
                        && (chars[id_start - 1].is_alphanumeric() || chars[id_start - 1] == '_')
                    {
                        id_start -= 1;
                    }
                    if id_start < id_end && !chars[id_start].is_ascii_digit() {
                        let byte_start = chars[..id_start]
                            .iter()
                            .map(|c| c.len_utf8())
                            .sum::<usize>();
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

#[cfg(test)]
mod tests {
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

        let flat = flatten_rst_roles_and_code(
            ":class:`pkg.\n    Thing` and ``multi\n    code`` \\\n next",
        );
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
}
