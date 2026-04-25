pub(super) fn module_path_from_definition_path(
    path: &std::path::Path,
    workspaces: &[std::path::PathBuf],
) -> Option<String> {
    fn sanitize_module_path_parts<'a>(parts: impl IntoIterator<Item = &'a str>) -> Option<String> {
        let mut out: Vec<&str> = parts
            .into_iter()
            .map(str::trim)
            .filter(|p| !p.is_empty() && *p != "__init__" && *p != "/")
            .collect();
        if let Some(site_idx) = out.iter().position(|p| *p == "site-packages") {
            out = out.into_iter().skip(site_idx + 1).collect();
        }
        if out.is_empty() {
            None
        } else {
            Some(out.join("."))
        }
    }

    let path_str = path.to_string_lossy();
    if let Some(std_idx) = path_str.rfind("/lib/python") {
        let stdlib_rel = &path_str[std_idx + "/lib/python".len()..];
        let after_version = stdlib_rel
            .split_once('/')
            .map(|(_, tail)| tail)
            .unwrap_or(stdlib_rel);
        let trimmed = after_version
            .strip_suffix(".pyi")
            .or_else(|| after_version.strip_suffix(".py"))
            .unwrap_or(after_version);
        if !trimmed.is_empty() && trimmed != "__init__" {
            return sanitize_module_path_parts(trimmed.trim_start_matches('/').split('/'));
        }
    }
    if !workspaces.iter().any(|ws| path.starts_with(ws)) {
        if let Some(ts_idx) = path_str.find("/stdlib/") {
            let rel = &path_str[ts_idx + "/stdlib/".len()..];
            let trimmed = rel.strip_suffix(".pyi").unwrap_or(rel);
            return sanitize_module_path_parts(trimmed.split('/'));
        }
        if let Some(ts_idx) = path_str.find("/stubs/") {
            let rel = &path_str[ts_idx + "/stubs/".len()..];
            let after_pkg = rel.split_once('/').map(|(_, tail)| tail).unwrap_or(rel);
            let trimmed = after_pkg.strip_suffix(".pyi").unwrap_or(after_pkg);
            return sanitize_module_path_parts(trimmed.split('/'));
        }
    }
    let rel = workspaces.iter().find_map(|ws| path.strip_prefix(ws).ok());
    if let Some(rel_path) = rel {
        let mut no_ext = rel_path.to_path_buf();
        no_ext.set_extension("");
        return sanitize_module_path_parts(
            no_ext.iter().filter_map(|c| c.to_str()).collect::<Vec<_>>(),
        );
    }
    None
}

pub(super) const HOVER_MODULE_PREFIX: &str = "[[MODULE]] ";
static HOVER_FOLDER_ICON_PREWARM: std::sync::Once = std::sync::Once::new();

pub(super) fn prepend_hover_module_path(
    popup: &mut crate::app::mouse::HoverPopup,
    module_path: &str,
) {
    HOVER_FOLDER_ICON_PREWARM.call_once(|| {
        crate::app::file_tree::pre_rasterize_icon("folder", true);
    });
    let legacy_header = format!("{}{}", HOVER_MODULE_PREFIX, module_path);
    if popup.text.starts_with(module_path) || popup.text.starts_with(&legacy_header) {
        return;
    }

    let prefix = format!("{legacy_header}\n");
    let shift = prefix.len();

    popup.text.insert_str(0, &prefix);

    for span in &mut popup.spans {
        span.start += shift;
        span.end += shift;
    }
    for (start, end) in &mut popup.inline_code_ranges {
        *start += shift;
        *end += shift;
    }

    let mut new_line_kinds = Vec::with_capacity(popup.line_kinds.len() + 1);
    new_line_kinds.push(crate::lsp::HoverLineKindPublic::Text);
    new_line_kinds.extend(popup.line_kinds.iter().copied());
    popup.line_kinds = new_line_kinds;
}

pub(super) fn source_line<'a>(
    text: &'a str,
    line_offsets: &[usize],
    line_idx: usize,
) -> Option<&'a str> {
    let start = *line_offsets.get(line_idx)?;
    let end = line_offsets
        .get(line_idx + 1)
        .copied()
        .map(|v| v.saturating_sub(1))
        .unwrap_or(text.len());
    text.get(start..end)
}

pub(super) fn source_signature_for_hover(
    editor: &crate::editor::Editor,
    byte_offset: usize,
    allow_nearby_fallback: bool,
    lsp_type: Option<&str>,
    module_path: Option<&str>,
) -> Option<String> {
    fn format_attribute_assignment_line(line: &str) -> String {
        let mut lines_iter = line.lines();
        let first = lines_iter.next().unwrap_or("");
        let base_indent = first.len() - first.trim_start().len();
        let mut out = vec![first.trim_start().to_string()];
        for l in lines_iter {
            let current_indent = l.len() - l.trim_start().len();
            if current_indent >= base_indent {
                out.push(l[base_indent..].to_string());
            } else {
                out.push(l.trim_start().to_string());
            }
        }
        let joined = out.join("\n");
        let trimmed = joined.trim_end();
        if trimmed.contains('\n') {
            return trimmed.to_string();
        }
        let Some(eq_idx) = trimmed.find('=') else {
            return trimmed.to_string();
        };
        let lhs = trimmed[..eq_idx].trim_end();
        let rhs = trimmed[eq_idx + 1..].trim_start();
        if rhs.len() < 56 {
            return format!("{lhs} = {rhs}");
        }
        let Some(open_idx) = rhs.find('(') else {
            return format!("{lhs} = {rhs}");
        };
        if !rhs.ends_with(')') {
            return format!("{lhs} = {rhs}");
        }
        let head = &rhs[..=open_idx];
        let inner = rhs[open_idx + 1..rhs.len().saturating_sub(1)].trim();
        if inner.is_empty() {
            return format!("{lhs} = {rhs}");
        }
        format!("{lhs} = {head}\n    {inner}\n    )")
    }

    fn source_attribute_hover_for_symbol(
        text: &str,
        line_offsets: &[usize],
        symbol: &str,
        lsp_type: Option<&str>,
        original_line_idx: usize,
        module_path: Option<&str>,
    ) -> Option<String> {
        for idx in 0..line_offsets.len() {
            let line = source_line(text, line_offsets, idx)?;
            let trimmed = line.trim_start();
            if trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }
            let mut matched = false;
            if let Some(rest) = trimmed.strip_prefix(symbol) {
                matched = rest.starts_with(':') || rest.starts_with(" =");
            }
            if !matched {
                continue;
            }

            if idx == original_line_idx {
                let ty_str = lsp_type.unwrap_or("Unknown");
                if let Some(m) = module_path {
                    return Some(format!("## Variable {symbol} of {m}\n{symbol}: {ty_str}"));
                } else {
                    return Some(format!("## Variable {symbol}\n{symbol}: {ty_str}"));
                }
            }

            let mut paren_depth = 0;
            let mut bracket_depth = 0;
            let mut brace_depth = 0;
            let mut in_string = false;
            let mut string_char = ' ';
            let mut statement_lines = vec![line.trim_end().to_string()];

            for c in line.chars() {
                if in_string {
                    if c == '\\' {
                        continue;
                    }
                    if c == string_char {
                        in_string = false;
                    }
                } else {
                    match c {
                        '"' | '\'' => {
                            in_string = true;
                            string_char = c;
                        }
                        '(' => paren_depth += 1,
                        ')' => paren_depth -= 1,
                        '[' => bracket_depth += 1,
                        ']' => bracket_depth -= 1,
                        '{' => brace_depth += 1,
                        '}' => brace_depth -= 1,
                        _ => {}
                    }
                }
            }

            let mut curr_idx = idx;
            while (paren_depth > 0 || bracket_depth > 0 || brace_depth > 0)
                && curr_idx + 1 < line_offsets.len()
            {
                curr_idx += 1;
                if let Some(next_line) = source_line(text, line_offsets, curr_idx) {
                    let trim_next = next_line.trim_end();
                    statement_lines.push(trim_next.to_string());
                    for c in trim_next.chars() {
                        if in_string {
                            if c == '\\' {
                                continue;
                            }
                            if c == string_char {
                                in_string = false;
                            }
                        } else {
                            match c {
                                '"' | '\'' => {
                                    in_string = true;
                                    string_char = c;
                                }
                                '(' => paren_depth += 1,
                                ')' => paren_depth -= 1,
                                '[' => bracket_depth += 1,
                                ']' => bracket_depth -= 1,
                                '{' => brace_depth += 1,
                                '}' => brace_depth -= 1,
                                _ => {}
                            }
                        }
                    }
                } else {
                    break;
                }
            }

            let full_statement = statement_lines.join("\n");

            let mut assignment = format_attribute_assignment_line(&full_statement);
            if let Some(raw_ty) = lsp_type {
                let mut ty = String::with_capacity(raw_ty.len());
                let mut s = raw_ty;
                while let Some(pos) = s.find("<class '") {
                    ty.push_str(&s[..pos]);
                    let rest = &s[pos + "<class '".len()..];
                    if let Some(end) = rest.find("'>") {
                        ty.push_str(&rest[..end]);
                        s = &rest[end + 2..];
                    } else {
                        ty.push_str(&s[pos..]);
                        s = "";
                    }
                }
                ty.push_str(s);
                let ty = ty
                    .replace("... omitted 3 union elements", "OmittedUnionElements")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");

                if assignment.starts_with(&format!("{symbol} =")) {
                    let replacement = format!("{symbol}: {ty} =");
                    assignment = assignment.replacen(&format!("{symbol} ="), &replacement, 1);
                } else if assignment.starts_with(&format!("{symbol}=")) {
                    let replacement = format!("{symbol}: {ty} =");
                    assignment = assignment.replacen(&format!("{symbol}="), &replacement, 1);
                }
            }

            let mut class_name = None;
            for up in (0..idx).rev() {
                let class_line = source_line(text, line_offsets, up)?.trim_start();
                if let Some(rest) = class_line.strip_prefix("class ") {
                    class_name = rest
                        .split(|c: char| c == '(' || c == ':' || c.is_whitespace())
                        .next()
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty());
                    break;
                }
            }

            if let Some(class_name) = class_name {
                let header = format!("## Class attribute {symbol} of {class_name}");
                return Some(format!("{header}\n{assignment}"));
            } else {
                if let Some(m) = module_path {
                    return Some(format!("## Variable {symbol} of {m}\n{assignment}"));
                } else {
                    return Some(format!("## Variable {symbol}\n{assignment}"));
                }
            }
        }
        None
    }
    fn def_name(line: &str) -> Option<&str> {
        let trimmed = line.trim_start();
        let rest = trimmed
            .strip_prefix("async def ")
            .or_else(|| trimmed.strip_prefix("def "))?;
        let open = rest.find('(')?;
        let name = rest[..open].trim();
        if name.is_empty() {
            None
        } else {
            Some(name)
        }
    }

    let text = editor.get_full_text();
    let line_idx = editor
        .line_offsets
        .partition_point(|&o| o <= byte_offset)
        .saturating_sub(1);

    let hovered_symbol = symbol_at_offset(editor, byte_offset);
    let mut def_line_idx = hovered_symbol.as_deref().and_then(|symbol| {
        for idx in 0..editor.line_offsets.len() {
            let line = source_line(&text, &editor.line_offsets, idx)?.trim_start();
            if def_name(line) == Some(symbol) {
                return Some(idx);
            }
        }
        None
    });
    if def_line_idx.is_none() {
        if let Some(symbol) = hovered_symbol.as_deref() {
            if let Some(attr_hover) = source_attribute_hover_for_symbol(
                &text,
                &editor.line_offsets,
                symbol,
                lsp_type,
                line_idx,
                module_path,
            ) {
                return Some(attr_hover);
            }
        }
    }
    if allow_nearby_fallback && def_line_idx.is_none() {
        for up in 0..=24usize {
            let idx = line_idx.saturating_sub(up);
            let line = source_line(&text, &editor.line_offsets, idx)?.trim_start();
            if line.starts_with("async def ") || line.starts_with("def ") {
                def_line_idx = Some(idx);
                break;
            }
        }
    }
    let def_line_idx = def_line_idx?;

    let mut decorators = Vec::new();
    let mut deco_idx = def_line_idx;
    while deco_idx > 0 {
        let prev_idx = deco_idx - 1;
        let prev_line = source_line(&text, &editor.line_offsets, prev_idx)?.trim();
        if prev_line.starts_with('@') {
            decorators.push(prev_line.to_string());
            deco_idx = prev_idx;
            continue;
        }
        break;
    }
    decorators.reverse();

    let mut sig_lines = Vec::new();
    for idx in def_line_idx..(def_line_idx + 16).min(editor.line_offsets.len()) {
        let line = source_line(&text, &editor.line_offsets, idx)?
            .trim_end()
            .to_string();
        if line.is_empty() {
            break;
        }
        sig_lines.push(line.clone());
        if line.contains(':') {
            break;
        }
    }
    if sig_lines.is_empty() {
        return None;
    }
    let mut signature = sig_lines.join("\n");
    if signature.contains("_AsyncGeneratorContextManager[None, None]") {
        signature = signature.replace(
            "_AsyncGeneratorContextManager[None, None]",
            "AsyncGenerator[None, Any]",
        );
    }
    signature = signature.trim_end_matches(':').trim_start().to_string();
    if decorators.len() == 1 && decorators[0].trim() == "@asynccontextmanager" {
        let mut def_signature = signature.trim_start().to_string();
        def_signature = wrap_signature_after_first_param(&def_signature, "", "async def ");
        signature = format!("@asynccontextmanager\n{}", def_signature.trim_start());
    } else if !decorators.is_empty() {
        signature = format!("{}\n{}", decorators.join("\n"), signature);
    }
    Some(signature)
}

pub(super) fn source_attribute_hover_from_definition_file(
    path: &std::path::Path,
    symbol: &str,
    module_path: &str,
    lsp_type: Option<&str>,
) -> Option<String> {
    fn format_attribute_assignment_line(line: &str) -> String {
        let mut lines_iter = line.lines();
        let first = lines_iter.next().unwrap_or("");
        let base_indent = first.len() - first.trim_start().len();
        let mut out = vec![first.trim_start().to_string()];
        for l in lines_iter {
            let current_indent = l.len() - l.trim_start().len();
            if current_indent >= base_indent {
                out.push(l[base_indent..].to_string());
            } else {
                out.push(l.trim_start().to_string());
            }
        }
        let joined = out.join("\n");
        let trimmed = joined.trim_end();
        if trimmed.contains('\n') {
            return trimmed.to_string();
        }
        let Some(eq_idx) = trimmed.find('=') else {
            return trimmed.to_string();
        };
        let lhs = trimmed[..eq_idx].trim_end();
        let rhs = trimmed[eq_idx + 1..].trim_start();
        if rhs.len() < 56 {
            return format!("{lhs} = {rhs}");
        }
        let Some(open_idx) = rhs.find('(') else {
            return format!("{lhs} = {rhs}");
        };
        if !rhs.ends_with(')') {
            return format!("{lhs} = {rhs}");
        }
        let head = &rhs[..=open_idx];
        let inner = rhs[open_idx + 1..rhs.len().saturating_sub(1)].trim();
        if inner.is_empty() {
            return format!("{lhs} = {rhs}");
        }
        format!("{lhs} = {head}\n    {inner}\n    )")
    }
    let text = std::fs::read_to_string(path).ok()?;
    let lines: Vec<&str> = text.lines().collect();
    for idx in 0..lines.len() {
        let trimmed = lines[idx].trim_start();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        let mut matched = false;
        if let Some(rest) = trimmed.strip_prefix(symbol) {
            matched = rest.starts_with(':') || rest.starts_with(" =");
        }
        if !matched {
            continue;
        }

        let mut paren_depth = 0;
        let mut bracket_depth = 0;
        let mut brace_depth = 0;
        let mut in_string = false;
        let mut string_char = ' ';
        let mut statement_lines = vec![lines[idx].trim_end().to_string()];

        for c in lines[idx].chars() {
            if in_string {
                if c == '\\' {
                    continue;
                }
                if c == string_char {
                    in_string = false;
                }
            } else {
                match c {
                    '"' | '\'' => {
                        in_string = true;
                        string_char = c;
                    }
                    '(' => paren_depth += 1,
                    ')' => paren_depth -= 1,
                    '[' => bracket_depth += 1,
                    ']' => bracket_depth -= 1,
                    '{' => brace_depth += 1,
                    '}' => brace_depth -= 1,
                    _ => {}
                }
            }
        }

        let mut curr_idx = idx;
        while (paren_depth > 0 || bracket_depth > 0 || brace_depth > 0)
            && curr_idx + 1 < lines.len()
        {
            curr_idx += 1;
            let trim_next = lines[curr_idx].trim_end();
            statement_lines.push(trim_next.to_string());
            for c in trim_next.chars() {
                if in_string {
                    if c == '\\' {
                        continue;
                    }
                    if c == string_char {
                        in_string = false;
                    }
                } else {
                    match c {
                        '"' | '\'' => {
                            in_string = true;
                            string_char = c;
                        }
                        '(' => paren_depth += 1,
                        ')' => paren_depth -= 1,
                        '[' => bracket_depth += 1,
                        ']' => bracket_depth -= 1,
                        '{' => brace_depth += 1,
                        '}' => brace_depth -= 1,
                        _ => {}
                    }
                }
            }
        }

        let full_statement = statement_lines.join("\n");

        let mut assignment = format_attribute_assignment_line(&full_statement);
        if let Some(raw_ty) = lsp_type {
            let mut ty = String::with_capacity(raw_ty.len());
            let mut s = raw_ty;
            while let Some(pos) = s.find("<class '") {
                ty.push_str(&s[..pos]);
                let rest = &s[pos + "<class '".len()..];
                if let Some(end) = rest.find("'>") {
                    ty.push_str(&rest[..end]);
                    s = &rest[end + 2..];
                } else {
                    ty.push_str(&s[pos..]);
                    s = "";
                }
            }
            ty.push_str(s);
            let ty = ty
                .replace("... omitted 3 union elements", "OmittedUnionElements")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");

            if assignment.starts_with(&format!("{symbol} =")) {
                let replacement = format!("{symbol}: {ty} =");
                assignment = assignment.replacen(&format!("{symbol} ="), &replacement, 1);
            } else if assignment.starts_with(&format!("{symbol}=")) {
                let replacement = format!("{symbol}: {ty} =");
                assignment = assignment.replacen(&format!("{symbol}="), &replacement, 1);
            }
        }

        let mut class_name = None;
        for up in (0..idx).rev() {
            let class_line = lines[up].trim_start();
            if let Some(rest) = class_line.strip_prefix("class ") {
                class_name = rest
                    .split(|c: char| c == '(' || c == ':' || c.is_whitespace())
                    .next()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                break;
            }
        }
        let (header_prefix, fq_owner) = if let Some(class_name) = class_name {
            let owner = if module_path.is_empty() {
                class_name
            } else {
                format!("{module_path}.{class_name}")
            };
            ("Class attribute", owner)
        } else {
            ("Variable", module_path.to_string())
        };

        if fq_owner.is_empty() {
            return Some(format!("## {header_prefix} {symbol}\n{}", assignment));
        }

        return Some(format!(
            "## {header_prefix} {symbol} of {fq_owner}\n{}",
            assignment
        ));
    }
    None
}

pub(super) fn source_class_signature_from_definition_file(
    path: &std::path::Path,
    symbol: &str,
) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let lines: Vec<&str> = text.lines().collect();
    let class_prefix = format!("class {symbol}");

    for idx in 0..lines.len() {
        let trimmed = lines[idx].trim_start();
        if trimmed.starts_with(&class_prefix) {
            let next_char = trimmed[class_prefix.len()..].chars().next();
            if next_char.is_none()
                || matches!(next_char, Some('(') | Some(':') | Some(' ') | Some('['))
            {
                let mut sig_lines = vec![];
                for i in idx..lines.len() {
                    let l = lines[i].trim_end();
                    if let Some(colon_idx) = l.find(':') {
                        sig_lines.push(l[..colon_idx].to_string());
                        break;
                    } else {
                        sig_lines.push(l.to_string());
                    }
                }
                if sig_lines.is_empty() {
                    return None;
                }
                let base_indent = lines[idx].len() - lines[idx].trim_start().len();
                let mut out = vec![sig_lines[0].trim_start().to_string()];
                for l in sig_lines.into_iter().skip(1) {
                    let current_indent = l.len() - l.trim_start().len();
                    if current_indent >= base_indent {
                        out.push(l[base_indent..].to_string());
                    } else {
                        out.push(l.trim_start().to_string());
                    }
                }
                return Some(out.join("\n"));
            }
        }
    }
    None
}

pub(super) fn is_ident_byte(b: u8) -> bool {
    matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_')
}

pub(super) fn symbol_at_offset(
    editor: &crate::editor::Editor,
    byte_offset: usize,
) -> Option<String> {
    if byte_offset >= editor.len() {
        return None;
    }
    let mut pos = byte_offset;
    if !is_ident_byte(editor.byte_at(pos)) {
        if pos > 0 && is_ident_byte(editor.byte_at(pos - 1)) {
            pos -= 1;
        } else {
            return None;
        }
    }
    let mut start = pos;
    while start > 0 && is_ident_byte(editor.byte_at(start - 1)) {
        start -= 1;
    }
    let mut end = pos + 1;
    while end < editor.len() && is_ident_byte(editor.byte_at(end)) {
        end += 1;
    }
    editor
        .get_full_text()
        .get(start..end)
        .map(|s| s.to_string())
}

pub(super) fn wrap_signature_after_first_param(
    signature: &str,
    line_prefix: &str,
    def_prefix: &str,
) -> String {
    if !signature.starts_with(line_prefix) {
        return signature.to_string();
    }
    let def_part = &signature[line_prefix.len()..];
    if !def_part.starts_with(def_prefix) || def_part.contains('\n') {
        return signature.to_string();
    }
    let Some(open_rel) = def_part.find('(') else {
        return signature.to_string();
    };
    let Some(close_rel) = def_part.rfind(')') else {
        return signature.to_string();
    };
    if close_rel <= open_rel {
        return signature.to_string();
    }
    let params = &def_part[open_rel + 1..close_rel];
    let Some(first_comma_rel) = params.find(',') else {
        return signature.to_string();
    };
    let comma_abs = line_prefix.len() + open_rel + 1 + first_comma_rel;
    let head = &signature[..=comma_abs];
    let tail = signature[comma_abs + 1..].trim_start();
    let indent = " ".repeat(open_rel + 1);
    format!("{head}\n{indent}{tail}")
}

#[cfg(test)]
mod tests {
    use super::{
        module_path_from_definition_path, should_replace_hover_with_source_signature,
        source_attribute_hover_from_definition_file, source_signature_for_hover, symbol_at_offset,
        wrap_signature_after_first_param,
    };

    #[test]
    fn wraps_asynccontextmanager_signature_after_first_param() {
        let raw = "async def lifespan(_: Litestar, arg: str) -> AsyncGenerator[None, Any]";
        let wrapped = wrap_signature_after_first_param(raw, "", "async def ");
        assert_eq!(
            wrapped,
            "async def lifespan(_: Litestar,\n                   arg: str) -> AsyncGenerator[None, Any]"
        );
    }

    #[test]
    fn asynccontextmanager_source_signature_is_split_into_lines() {
        let mut editor = crate::editor::Editor::new(256);
        editor.insert_str(
            "@asynccontextmanager\nasync def lifespan(_: Litestar, arg: str) -> AsyncGenerator[None, Any]:\n    yield\n",
        );
        let hover_offset = editor
            .get_full_text()
            .find("lifespan")
            .expect("expected test function name");
        let signature = source_signature_for_hover(&editor, hover_offset, true, None, None)
            .expect("expected signature");
        assert_eq!(signature,
            "@asynccontextmanager\nasync def lifespan(_: Litestar,\n                   arg: str) -> AsyncGenerator[None, Any]"
        );
    }

    #[test]
    fn classmethod_asynccontextmanager_source_signature_keeps_both_decorators() {
        let mut editor = crate::editor::Editor::new(512);
        editor.insert_str(
            "class LitestarCache:\n    @classmethod\n    @asynccontextmanager\n    async def setup(cls) -> AsyncGenerator[None, Any]:\n        yield\n\nasync def use_cache():\n    async with LitestarCache.setup():\n        pass\n",
        );
        let hover_offset = editor
            .get_full_text()
            .rfind("setup")
            .expect("expected setup call");
        let signature = source_signature_for_hover(&editor, hover_offset, true, None, None)
            .expect("expected signature");
        assert_eq!(
            signature,
            "@classmethod\n@asynccontextmanager\nasync def setup(cls) -> AsyncGenerator[None, Any]"
        );
    }

    #[test]
    fn attribute_hover_uses_source_attribute_line() {
        let mut editor = crate::editor::Editor::new(512);
        editor.insert_str(
            "class FcmSenderService:\n    client: AsyncFirebaseClient = AsyncFirebaseClient(request_timeout=RequestTimeout(timeout=50))\n\nasync def close_client():\n    if FcmSenderService.client:\n        pass\n",
        );
        let hover_offset = editor
            .get_full_text()
            .rfind("client")
            .expect("expected client usage");
        let signature = source_signature_for_hover(&editor, hover_offset, false, None, None)
            .expect("expected attribute hover");
        assert_eq!(signature,
            "## Class attribute client of FcmSenderService\nclient: AsyncFirebaseClient = AsyncFirebaseClient(\n    request_timeout=RequestTimeout(timeout=50)\n    )"
        );
    }

    #[test]
    fn simple_type_hover_triggers_source_replacement() {
        assert!(should_replace_hover_with_source_signature(
            "AsyncFirebaseClient"
        ));
    }

    #[test]
    fn symbol_at_offset_reads_identifier_from_nearby_position() {
        let mut editor = crate::editor::Editor::new(128);
        editor.insert_str("await service.client\n");
        let client_offset = editor
            .get_full_text()
            .find("client")
            .expect("expected client token");
        assert_eq!(
            symbol_at_offset(&editor, client_offset + 1).as_deref(),
            Some("client")
        );
    }

    #[test]
    fn strict_source_lookup_does_not_fallback_to_nearby_def() {
        let mut editor = crate::editor::Editor::new(256);
        editor.insert_str(
            "@asynccontextmanager\nasync def lifespan(_: Litestar, arg: str) -> AsyncGenerator[None, Any]:\n    yield\n\nawait FcmSenderService.client._http_client.aclose()\n",
        );
        let client_offset = editor
            .get_full_text()
            .rfind("client")
            .expect("expected client token");
        let sig = source_signature_for_hover(&editor, client_offset, false, None, None);
        assert!(sig.is_none(), "strict mode must not return unrelated def");
    }

    #[test]
    fn definition_file_attribute_hover_includes_fq_class_name() {
        let mut tmp = std::env::temp_dir();
        tmp.push(format!(
            "rriter_attr_hover_{}_{}.py",
            std::process::id(),
            1usize
        ));
        let src = "class FcmSenderService:\n    client: AsyncFirebaseClient = AsyncFirebaseClient(request_timeout=RequestTimeout(timeout=50))\n";
        std::fs::write(&tmp, src).expect("expected temp file write");
        let hover = source_attribute_hover_from_definition_file(
            &tmp,
            "client",
            "car_wash.core.fcm.service",
            None,
        )
        .expect("expected attribute hover text");
        assert_eq!(
            hover,
            "## Class attribute client of car_wash.core.fcm.service.FcmSenderService\nclient: AsyncFirebaseClient = AsyncFirebaseClient(\n    request_timeout=RequestTimeout(timeout=50)\n    )"
        );
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn definition_file_attribute_hover_includes_lsp_type() {
        let mut tmp = std::env::temp_dir();
        tmp.push(format!(
            "rriter_attr_hover_{}_{}.py",
            std::process::id(),
            2usize
        ));
        let src = "cars_router = Router(\n    path='/cars'\n)\n";
        std::fs::write(&tmp, src).expect("expected temp file write");
        let hover = source_attribute_hover_from_definition_file(
            &tmp,
            "cars_router",
            "car_wash",
            Some("Router"),
        )
        .expect("expected attribute hover text");

        assert!(hover.contains("cars_router: Router = Router("));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn module_path_from_site_packages_strips_noise_segments() {
        let path = std::path::Path::new(
            "/usr/lib/python3.12/site-packages/litestar/exceptions/http_exceptions.py",
        );
        let module = module_path_from_definition_path(path, &[]);
        assert_eq!(
            module.as_deref(),
            Some("litestar.exceptions.http_exceptions")
        );
    }

    #[test]
    fn module_path_strips_trailing_init() {
        let path = std::path::Path::new("/usr/lib/python3.12/site-packages/msgspec/__init__.py");
        let module = module_path_from_definition_path(path, &[]);
        assert_eq!(module.as_deref(), Some("msgspec"));
    }

    #[test]
    fn attribute_hover_merges_lsp_type_and_ignores_when_on_declaration() {
        let mut editor = crate::editor::Editor::new(512);
        editor.insert_str(
            "handlers = [\n    AuthController,\n    users_router,\n]\n\nfor h in handlers:\n    pass\n",
        );

        let text = editor.get_full_text();

        let decl_offset = text.find("handlers").unwrap();
        let decl_sig = source_signature_for_hover(
            &editor,
            decl_offset,
            false,
            Some("list[Controller]"),
            Some("car_wash"),
        )
        .expect("should return specific declaration format");
        assert_eq!(
            decl_sig, "## Variable handlers of car_wash\nhandlers: list[Controller]",
            "should show only type on declaration"
        );

        let usage_offset = text.rfind("handlers").unwrap();
        let usage_sig = source_signature_for_hover(
            &editor,
            usage_offset,
            false,
            Some("list[Controller]"),
            Some("car_wash"),
        )
        .expect("expected signature on usage");

        assert_eq!(
            usage_sig,
            "## Variable handlers of car_wash\nhandlers: list[Controller] = [\n    AuthController,\n    users_router,\n]"
        );
    }
}

pub(super) fn should_replace_hover_with_source_signature(clean_msg: &str) -> bool {
    should_replace_simple_type_hover(clean_msg) || {
        let trimmed = clean_msg.trim_start();
        (trimmed.starts_with('(') || trimmed.starts_with(") ->") || trimmed.starts_with("(_:"))
            && clean_msg.contains("_AsyncGeneratorContextManager")
    }
}

pub(super) fn should_replace_simple_type_hover(clean_msg: &str) -> bool {
    let trimmed = clean_msg.trim_start();
    if trimmed.contains('\n') || trimmed.contains("::") {
        return false;
    }
    if trimmed.starts_with("def ")
        || trimmed.starts_with("class ")
        || trimmed.starts_with("async def ")
        || trimmed.starts_with("bound method ")
    {
        return false;
    }
    if !trimmed
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
    {
        return false;
    }
    true
}
