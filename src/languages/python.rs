pub use super::ImportBlock;
use super::finish_import_block;
use crate::lsp::HoverLineKindPublic;
use std::collections::HashMap;
use tree_sitter::StreamingIterator;

pub const DOCSTRING_TEXT: [f32; 4] = crate::highlighter::DRACULA_COMMENT;

pub fn fence_tag(line: &str) -> Option<&str> {
    line.trim().strip_prefix("```").map(str::trim)
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
    let mut delimiter_state = super::PythonDelimiterState::default();

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
            update_python_import_continuation(trimmed, &mut delimiter_state, &mut continuing);
            if !continuing {
                delimiter_state = super::PythonDelimiterState::default();
            }
            continue;
        }

        if continuing && !trimmed.is_empty() {
            if let Some(block) = &mut current {
                block.end = line_end;
                block.line_count += pending_blank_lines + 1;
            }
            pending_blank_lines = 0;
            update_python_import_continuation(trimmed, &mut delimiter_state, &mut continuing);
            continue;
        }

        pending_blank_lines = 0;
        continuing = false;
        delimiter_state = super::PythonDelimiterState::default();
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

pub(crate) fn python_plain_assignment_after_token(after_token: &str) -> bool {
    let bytes = after_token.as_bytes();
    for (idx, &byte) in bytes.iter().enumerate() {
        if byte != b'=' {
            continue;
        }
        let previous = idx.checked_sub(1).and_then(|at| bytes.get(at)).copied();
        let next = bytes.get(idx + 1).copied();
        if matches!(previous, Some(b'=' | b'!' | b'<' | b'>' | b':')) || next == Some(b'=') {
            continue;
        }
        return true;
    }
    false
}

pub(crate) fn python_class_direct_attr(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("def ")
        || trimmed.starts_with("async def ")
        || trimmed.starts_with('@')
        || trimmed.starts_with("class ")
    {
        return None;
    }
    let end = trimmed
        .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .unwrap_or(trimmed.len());
    if end == 0 {
        return None;
    }
    let rest = trimmed[end..].trim_start();
    (rest.starts_with(':') || python_plain_assignment_after_token(rest)).then_some(&trimmed[..end])
}

pub(crate) fn python_class_header_name(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("class ")?;
    let end = rest
        .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .unwrap_or(rest.len());
    (end > 0).then_some(&rest[..end])
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

fn python_import_line_has_explicit_continuation(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut quote = None;
    let mut triple = false;
    let mut escaped = false;
    let mut last_code = None;
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
                index += 1;
                continue;
            }
            if byte == b'\\' {
                escaped = true;
                index += 1;
                continue;
            }
            let closes = byte == active_quote
                && (!triple
                    || bytes.get(index + 1) == Some(&active_quote)
                        && bytes.get(index + 2) == Some(&active_quote));
            if closes {
                last_code = Some(active_quote);
                quote = None;
                index += if triple { 3 } else { 1 };
            } else {
                index += 1;
            }
            continue;
        }
        match byte {
            b'#' => break,
            active_quote @ (b'\'' | b'"') => {
                quote = Some(active_quote);
                triple = bytes.get(index + 1) == Some(&active_quote)
                    && bytes.get(index + 2) == Some(&active_quote);
                last_code = Some(active_quote);
                index += if triple { 3 } else { 1 };
            }
            byte if byte.is_ascii_whitespace() => index += 1,
            _ => {
                last_code = Some(byte);
                index += 1;
            }
        }
    }
    last_code == Some(b'\\')
}

fn update_python_import_continuation(
    trimmed: &str,
    delimiters: &mut super::PythonDelimiterState,
    continuing: &mut bool,
) {
    delimiters.scan_line(trimmed);
    *continuing =
        delimiters.has_open_delimiter() || python_import_line_has_explicit_continuation(trimmed);
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

        if let Some(lang) = fence_tag(trimmed) {
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

include!("python_highlight_spans.rs");
#[cfg(test)]
#[path = "python_tests.rs"]
mod python_tests;
