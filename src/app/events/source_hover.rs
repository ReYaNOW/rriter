pub(crate) fn module_path_from_definition_path(
    path: &std::path::Path,
    workspaces: &[std::path::PathBuf],
) -> Option<String> {
    module_path_from_definition_path_for_platform(
        path,
        workspaces,
        crate::platform::CURRENT_PLATFORM,
    )
}

fn module_path_from_definition_path_for_platform(
    path: &std::path::Path,
    workspaces: &[std::path::PathBuf],
    platform: crate::platform::PlatformKind,
) -> Option<String> {
    fn sanitize_module_path(rel: &str) -> Option<String> {
        let rel = rel
            .strip_suffix(".pyi")
            .or_else(|| rel.strip_suffix(".py"))
            .unwrap_or(rel);
        let mut parts = rel
            .split('/')
            .map(str::trim)
            .filter(|part| !part.is_empty() && *part != ".")
            .collect::<Vec<_>>();
        if parts.last() == Some(&"__init__") {
            parts.pop();
        }
        if let Some(site_idx) = parts.iter().position(|part| *part == "site-packages") {
            parts.drain(..=site_idx);
        }
        (!parts.is_empty()).then(|| parts.join("."))
    }

    fn normalized(path: &std::path::Path) -> String {
        let mut text = path.to_string_lossy().replace('\\', "/");
        if let Some(rest) = text.strip_prefix("//?/UNC/") {
            text = format!("//{rest}");
        } else if let Some(rest) = text.strip_prefix("//?/") {
            text = rest.to_string();
        }
        text
    }

    fn component_eq(left: &str, right: &str, platform: crate::platform::PlatformKind) -> bool {
        if platform == crate::platform::PlatformKind::Windows {
            left.to_lowercase() == right.to_lowercase()
        } else {
            left == right
        }
    }

    fn relative_components(
        path: &str,
        root: &str,
        platform: crate::platform::PlatformKind,
    ) -> Option<String> {
        let path_parts = path
            .split('/')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        let root_parts = root
            .split('/')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        if root_parts.len() > path_parts.len()
            || !root_parts
                .iter()
                .zip(&path_parts)
                .all(|(root, path)| component_eq(root, path, platform))
        {
            return None;
        }
        Some(path_parts[root_parts.len()..].join("/"))
    }

    fn suffix_after_marker<'a>(path: &'a str, marker: &str) -> Option<&'a str> {
        let lower = path.to_ascii_lowercase();
        let index = lower.rfind(&marker.to_ascii_lowercase())?;
        path.get(index + marker.len()..)
    }

    let path_text = normalized(path);
    let workspace_rel = workspaces
        .iter()
        .filter_map(|workspace| {
            let workspace_text = normalized(workspace);
            relative_components(&path_text, &workspace_text, platform)
                .map(|relative| (workspace_text.split('/').count(), relative))
        })
        .max_by_key(|(depth, _)| *depth)
        .map(|(_, relative)| relative);

    if let Some(relative) = suffix_after_marker(&path_text, "/site-packages/") {
        return sanitize_module_path(relative);
    }

    if let Some(after_python) = suffix_after_marker(&path_text, "/lib/python") {
        let relative = after_python
            .split_once('/')
            .map(|(_, tail)| tail)
            .unwrap_or(after_python);
        if !relative.is_empty() {
            return sanitize_module_path(relative);
        }
    }

    if workspace_rel.is_none() {
        if let Some(relative) = suffix_after_marker(&path_text, "/stdlib/") {
            return sanitize_module_path(relative);
        }
        if let Some(relative) = suffix_after_marker(&path_text, "/stubs/") {
            let after_package = relative
                .split_once('/')
                .map(|(_, tail)| tail)
                .unwrap_or(relative);
            return sanitize_module_path(after_package);
        }
        if platform == crate::platform::PlatformKind::Windows {
            if let Some(relative) = suffix_after_marker(&path_text, "/lib/") {
                return sanitize_module_path(relative);
            }
        }
    }

    workspace_rel.and_then(|relative| sanitize_module_path(&relative))
}

pub(super) const HOVER_MODULE_PREFIX: &str = "[[MODULE]] ";
static HOVER_FOLDER_ICON_PREWARM: std::sync::Once = std::sync::Once::new();

pub(crate) fn prepend_hover_module_path(
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

fn format_attribute_assignment_line(line: &str) -> String {
    let mut lines_iter = line.lines();
    let first = lines_iter.next().unwrap_or("");
    let base_indent = first.len() - first.trim_start().len();
    let mut out = vec![first.trim_start().to_string()];
    for line in lines_iter {
        let current_indent = line.len() - line.trim_start().len();
        if current_indent >= base_indent {
            out.push(line[base_indent..].to_string());
        } else {
            out.push(line.trim_start().to_string());
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

fn source_attribute_class_owner(lines: &[&str], line_idx: usize) -> Option<String> {
    let line = *lines.get(line_idx)?;
    let target_indent = line.len().saturating_sub(line.trim_start().len());
    if target_indent == 0 {
        return None;
    }

    for line in lines[..line_idx].iter().rev().copied() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len().saturating_sub(trimmed.len());
        if indent >= target_indent {
            continue;
        }
        if let Some(name) = crate::app::class_header_name(line) {
            return Some(name.to_string());
        }
        if trimmed.starts_with("def ")
            || trimmed.starts_with("async def ")
            || trimmed.ends_with(':')
        {
            return None;
        }
    }
    None
}

pub(super) fn source_signature_for_hover(
    editor: &crate::editor::Editor,
    byte_offset: usize,
    allow_nearby_fallback: bool,
    lsp_type: Option<&str>,
    module_path: Option<&str>,
) -> Option<String> {
    fn source_attribute_hover_for_symbol(
        text: &str,
        line_offsets: &[usize],
        symbol: &str,
        lsp_type: Option<&str>,
        original_line_idx: usize,
        module_path: Option<&str>,
    ) -> Option<String> {
        let lines = text.lines().collect::<Vec<_>>();
        for idx in 0..line_offsets.len() {
            let line = source_line(text, line_offsets, idx)?;
            let trimmed = line.trim_start();
            if trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }
            if crate::app::class_direct_attr(trimmed) != Some(symbol) {
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

            let mut delimiters = crate::languages::PythonDelimiterState::default();
            let mut statement_lines = vec![line.trim_end().to_string()];

            delimiters.scan_line(line);

            let mut curr_idx = idx;
            while delimiters.has_open_delimiter() && curr_idx + 1 < line_offsets.len() {
                curr_idx += 1;
                if let Some(next_line) = source_line(text, line_offsets, curr_idx) {
                    let trim_next = next_line.trim_end();
                    statement_lines.push(trim_next.to_string());
                    delimiters.scan_line(trim_next);
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

            if let Some(class_name) = source_attribute_class_owner(&lines, idx) {
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
        let open_paren = rest.find('(')?;
        let open_bracket = rest.find('[').unwrap_or(open_paren);
        let name_end = open_paren.min(open_bracket);
        let name = rest[..name_end].trim();
        if name.is_empty() { None } else { Some(name) }
    }
    fn enclosing_python_owner(
        text: &str,
        line_offsets: &[usize],
        line_idx: usize,
    ) -> (Option<String>, Option<String>) {
        let mut method_name = None;
        let mut class_name = None;
        let mut def_indent = usize::MAX;
        for up in (0..=line_idx).rev() {
            let Some(line) = source_line(text, line_offsets, up) else {
                continue;
            };
            let trimmed = line.trim_start();
            let indent = line.len() - trimmed.len();
            if method_name.is_none() {
                if let Some(name) = def_name(trimmed) {
                    method_name = Some(name.to_string());
                    def_indent = indent;
                    continue;
                }
            }
            if indent < def_indent {
                if let Some(rest) = trimmed.strip_prefix("class ") {
                    class_name = rest
                        .split(|c: char| c == '(' || c == ':' || c.is_whitespace() || c == '[')
                        .next()
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty());
                    break;
                }
            }
        }
        (class_name, method_name)
    }
    fn self_parameter_hover(
        symbol: &str,
        text: &str,
        line_offsets: &[usize],
        line_idx: usize,
        lsp_type: Option<&str>,
        module_path: Option<&str>,
    ) -> Option<String> {
        if symbol != "self" && symbol != "cls" {
            return None;
        }
        let raw_ty = lsp_type?;
        if !raw_ty.contains("Self@") {
            return None;
        }
        let (class_name, method_name) = enclosing_python_owner(text, line_offsets, line_idx);
        let class_name = class_name?;
        let method_name = method_name.unwrap_or_else(|| "__init__".to_string());
        let owner = if let Some(m) = module_path {
            format!("{m}.{class_name}.{method_name}")
        } else {
            format!("{class_name}.{method_name}")
        };
        let ty = if symbol == "cls" {
            format!("type[{class_name}]")
        } else {
            class_name
        };
        Some(format!("## Parameter {symbol} of {owner}\n{symbol}: {ty}"))
    }

    let text = editor.get_full_text();
    let line_idx = editor
        .line_offsets
        .partition_point(|&o| o <= byte_offset)
        .saturating_sub(1);

    let hovered_symbol = symbol_at_offset(editor, byte_offset);
    if let Some(symbol) = hovered_symbol.as_deref() {
        if let Some(param_hover) = self_parameter_hover(
            symbol,
            &text,
            &editor.line_offsets,
            line_idx,
            lsp_type,
            module_path,
        ) {
            return Some(param_hover);
        }
    }
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
    if def_line_idx.is_none()
        && let Some(symbol) = hovered_symbol.as_deref()
        && let Some(class_hover) = source_class_signature_from_text(&text, symbol)
    {
        return Some(class_hover);
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
    let mut paren_depth = 0;
    let mut bracket_depth = 0;
    let mut in_string = false;
    let mut string_char = ' ';
    let mut found_end = false;

    for idx in def_line_idx..(def_line_idx + 16).min(editor.line_offsets.len()) {
        let line = source_line(&text, &editor.line_offsets, idx)?
            .trim_end()
            .to_string();
        if line.is_empty() {
            break;
        }
        sig_lines.push(line.clone());

        let mut prev_char = ' ';
        for c in line.chars() {
            if in_string {
                if c == string_char && prev_char != '\\' {
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
                    ':' if paren_depth <= 0 && bracket_depth <= 0 => {
                        found_end = true;
                    }
                    _ => {}
                }
            }
            if c == '\\' && prev_char == '\\' {
                prev_char = ' ';
            } else {
                prev_char = c;
            }
        }

        if found_end {
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
    let text = std::fs::read_to_string(path).ok()?;
    let lines: Vec<&str> = text.lines().collect();
    for idx in 0..lines.len() {
        let trimmed = lines[idx].trim_start();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        if crate::app::class_direct_attr(trimmed) != Some(symbol) {
            continue;
        }

        let mut delimiters = crate::languages::PythonDelimiterState::default();
        let mut statement_lines = vec![lines[idx].trim_end().to_string()];

        delimiters.scan_line(lines[idx]);

        let mut curr_idx = idx;
        while delimiters.has_open_delimiter() && curr_idx + 1 < lines.len() {
            curr_idx += 1;
            let trim_next = lines[curr_idx].trim_end();
            statement_lines.push(trim_next.to_string());
            delimiters.scan_line(trim_next);
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

        let (header_prefix, fq_owner) =
            if let Some(class_name) = source_attribute_class_owner(&lines, idx) {
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

fn source_function_name_offset(text: &str, symbol: &str) -> Option<usize> {
    if symbol.is_empty() {
        return None;
    }
    let mut byte = 0usize;
    for raw_line in text.split_inclusive('\n') {
        let line = raw_line.trim_end_matches(['\r', '\n']);
        let trimmed = line.trim_start();
        let rest = trimmed
            .strip_prefix("async def ")
            .or_else(|| trimmed.strip_prefix("def "));
        if let Some(rest) = rest {
            let Some(open_paren) = rest.find('(') else {
                byte = byte.saturating_add(raw_line.len());
                continue;
            };
            let open_bracket = rest.find('[').unwrap_or(open_paren);
            let name_end = open_paren.min(open_bracket);
            if rest[..name_end].trim() == symbol {
                let indent = line.len().saturating_sub(trimmed.len());
                let def_prefix = if trimmed.starts_with("async def ") {
                    "async def "
                } else {
                    "def "
                };
                return Some(byte + indent + def_prefix.len());
            }
        }
        byte = byte.saturating_add(raw_line.len());
    }
    None
}

pub(crate) fn source_function_signature_from_text(
    text: &str,
    symbol: &str,
    module_path: Option<&str>,
) -> Option<String> {
    let offset = source_function_name_offset(text, symbol)?;
    let mut editor = crate::editor::Editor::new(text.len().saturating_add(1));
    editor.insert_str(text);
    source_signature_for_hover(&editor, offset, false, None, module_path)
}

fn source_class_signature_from_text(text: &str, symbol: &str) -> Option<String> {
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
                let mut paren_depth = 0;
                let mut bracket_depth = 0;
                let mut in_string = false;
                let mut string_char = ' ';

                for l in lines.iter().skip(idx) {
                    let l = l.trim_end();
                    let mut prev_char = ' ';
                    let mut colon_idx = None;

                    for (c_idx, c) in l.char_indices() {
                        if in_string {
                            if c == string_char && prev_char != '\\' {
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
                                ':' if paren_depth <= 0 && bracket_depth <= 0 => {
                                    colon_idx = Some(c_idx);
                                    break;
                                }
                                _ => {}
                            }
                        }
                        if c == '\\' && prev_char == '\\' {
                            prev_char = ' ';
                        } else {
                            prev_char = c;
                        }
                    }

                    if let Some(c_idx) = colon_idx {
                        sig_lines.push(l[..c_idx].to_string());
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

pub(crate) fn source_class_signature_from_definition_file(
    path: &std::path::Path,
    symbol: &str,
) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    source_class_signature_from_text(&text, symbol)
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
        HOVER_MODULE_PREFIX, module_path_from_definition_path,
        module_path_from_definition_path_for_platform, prepend_hover_module_path,
        should_replace_hover_with_source_signature, source_attribute_hover_from_definition_file,
        source_class_signature_from_definition_file, source_line, source_signature_for_hover,
        symbol_at_offset, wrap_signature_after_first_param,
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
        assert_eq!(
            signature,
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
        assert_eq!(
            signature,
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
    fn module_path_covers_stdlib_stubs_workspace_and_empty_edges() {
        let stdlib = std::path::Path::new("/opt/pyright/stdlib/pathlib.pyi");
        assert_eq!(
            module_path_from_definition_path(stdlib, &[]).as_deref(),
            Some("pathlib")
        );

        let stub = std::path::Path::new("/opt/pyright/stubs/redis/client/core.pyi");
        assert_eq!(
            module_path_from_definition_path(stub, &[]).as_deref(),
            Some("client.core")
        );

        let ws = std::path::PathBuf::from("/work/app");
        let local = std::path::Path::new("/work/app/pkg/service/__init__.py");
        assert_eq!(
            module_path_from_definition_path(local, &[ws]).as_deref(),
            Some("pkg.service")
        );

        let empty = std::path::Path::new("/work/app/__init__.py");
        assert_eq!(
            module_path_from_definition_path(empty, &[std::path::PathBuf::from("/work/app")]),
            None
        );
    }

    #[test]
    fn module_path_supports_windows_venv_stdlib_and_workspace_paths() {
        use crate::platform::PlatformKind;

        let package = std::path::Path::new(
            r"C:\work\app\.venv\Lib\site-packages\litestar\routing\__init__.py",
        );
        assert_eq!(
            module_path_from_definition_path_for_platform(package, &[], PlatformKind::Windows)
                .as_deref(),
            Some("litestar.routing")
        );

        let stdlib = std::path::Path::new(r"C:\Python313\Lib\pathlib.py");
        assert_eq!(
            module_path_from_definition_path_for_platform(stdlib, &[], PlatformKind::Windows)
                .as_deref(),
            Some("pathlib")
        );

        let workspace = std::path::PathBuf::from(r"C:\WORK\App");
        let local = std::path::Path::new(r"c:\work\app\pkg\service.py");
        assert_eq!(
            module_path_from_definition_path_for_platform(
                local,
                &[workspace],
                PlatformKind::Windows,
            )
            .as_deref(),
            Some("pkg.service")
        );
    }

    #[test]
    fn prepend_hover_module_path_shifts_text_spans_and_inline_ranges_once() {
        let mut popup = crate::app::mouse::HoverPopup {
            text: "Thing\nvalue".to_string(),
            spans: vec![crate::highlighter::ColorSpan {
                start: 0,
                end: 5,
                color: [1.0, 0.0, 0.0, 1.0],
            }],
            line_kinds: vec![
                crate::lsp::HoverLineKindPublic::Code,
                crate::lsp::HoverLineKindPublic::Text,
            ],
            inline_code_ranges: vec![(6, 11)],
            byte_offset: 0,
            anchor_x: 0.0,
            anchor_y: 0.0,
            offset_x: None,
            offset_y: None,
            anim_progress: 1.0,
            scroll: crate::scroll::ScrollState::new(15.0),
            layout_cache: None,
        };

        prepend_hover_module_path(&mut popup, "pkg.mod");
        let prefix = format!("{HOVER_MODULE_PREFIX}pkg.mod\n");
        assert!(popup.text.starts_with(&prefix));
        assert_eq!(popup.spans[0].start, prefix.len());
        assert_eq!(popup.spans[0].end, prefix.len() + 5);
        assert_eq!(
            popup.inline_code_ranges[0],
            (prefix.len() + 6, prefix.len() + 11)
        );
        assert_eq!(popup.line_kinds[0], crate::lsp::HoverLineKindPublic::Text);

        let text_once = popup.text.clone();
        prepend_hover_module_path(&mut popup, "pkg.mod");
        assert_eq!(popup.text, text_once);
    }

    #[test]
    fn source_line_handles_middle_last_and_missing_lines() {
        let text = "alpha\nbeta\ngamma";
        let offsets = vec![0, 6, 11];
        assert_eq!(source_line(text, &offsets, 0), Some("alpha"));
        assert_eq!(source_line(text, &offsets, 1), Some("beta"));
        assert_eq!(source_line(text, &offsets, 2), Some("gamma"));
        assert_eq!(source_line(text, &offsets, 3), None);
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

    #[test]
    fn source_signature_handles_python_generics_in_def() {
        let mut editor = crate::editor::Editor::new(512);
        editor.insert_str(
            "def process_items[T: BaseModel, R: Struct](\n    items: list[T],\n) -> list[R]:\n    pass\n",
        );
        let hover_offset = editor.get_full_text().find("process_items").unwrap();
        let sig = source_signature_for_hover(&editor, hover_offset, true, None, None).unwrap();
        assert_eq!(
            sig,
            "def process_items[T: BaseModel, R: Struct](\n    items: list[T],\n) -> list[R]"
        );
    }

    #[test]
    fn definition_file_class_signature_handles_python_generics() {
        let mut tmp = std::env::temp_dir();
        tmp.push(format!(
            "rriter_class_sig_{}_{}.py",
            std::process::id(),
            3usize
        ));
        let src = "class RepoBase[TModel: Base, TReadStruct: BasedStruct]:\n    model: ClassVar[type[Base]]\n";
        std::fs::write(&tmp, src).expect("expected temp file write");
        let sig = source_class_signature_from_definition_file(&tmp, "RepoBase")
            .expect("expected class signature");
        assert_eq!(
            sig,
            "class RepoBase[TModel: Base, TReadStruct: BasedStruct]"
        );
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn source_attribute_hover_covers_class_object_type_cleanup_and_short_assignments() {
        let mut editor = crate::editor::Editor::new(512);
        editor.insert_str(
            "class Service:\n    client = make_client()\n\nasync def use():\n    return Service.client\n",
        );
        let hover_offset = editor.get_full_text().rfind("client").unwrap();
        let sig = source_signature_for_hover(
            &editor,
            hover_offset,
            false,
            Some("<class 'pkg.Client'> | ... omitted 3 union elements"),
            None,
        )
        .expect("expected class attribute hover");

        assert_eq!(
            sig,
            "## Class attribute client of Service\nclient: pkg.Client | OmittedUnionElements = make_client()"
        );
    }

    #[test]
    fn source_attribute_hover_accepts_whitespace_before_plain_assignment() {
        let mut editor = crate::editor::Editor::new(512);
        editor.insert_str(
            "class Service:\n    client  = make_client()\n\ndef use():\n    return Service.client\n",
        );
        let hover_offset = editor.get_full_text().rfind("client").unwrap();
        let hover = source_signature_for_hover(&editor, hover_offset, false, None, None)
            .expect("expected class attribute hover");

        assert_eq!(
            hover,
            "## Class attribute client of Service\nclient = make_client()"
        );
    }

    #[test]
    fn source_attribute_hover_does_not_inherit_class_after_dedent() {
        let mut editor = crate::editor::Editor::new(512);
        editor.insert_str(
            "class Service:\n    client = make_client()\n\nmodule_value  = make_value()\nprint(module_value)\n",
        );
        let hover_offset = editor.get_full_text().rfind("module_value").unwrap();
        let hover =
            source_signature_for_hover(&editor, hover_offset, false, None, Some("pkg.module"))
                .expect("expected module variable hover");

        assert_eq!(
            hover,
            "## Variable module_value of pkg.module\nmodule_value = make_value()"
        );
    }

    #[test]
    fn definition_attribute_hover_accepts_spaced_annotation() {
        let mut tmp = std::env::temp_dir();
        tmp.push(format!(
            "rriter_attr_hover_{}_spaced.py",
            std::process::id()
        ));
        std::fs::write(
            &tmp,
            "class Service:\n    client : Client = make_client()\n",
        )
        .expect("expected temp file write");

        let hover = source_attribute_hover_from_definition_file(&tmp, "client", "pkg.module", None)
            .expect("expected definition attribute hover");

        assert_eq!(
            hover,
            "## Class attribute client of pkg.module.Service\nclient : Client = make_client()"
        );
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn definition_attribute_hover_does_not_inherit_class_after_dedent() {
        let mut tmp = std::env::temp_dir();
        tmp.push(format!(
            "rriter_attr_hover_{}_dedent.py",
            std::process::id()
        ));
        std::fs::write(
            &tmp,
            "class Service:\n    client = make_client()\n\nmodule_value = make_value()\n",
        )
        .expect("expected temp file write");

        let hover =
            source_attribute_hover_from_definition_file(&tmp, "module_value", "pkg.module", None)
                .expect("expected definition variable hover");

        assert_eq!(
            hover,
            "## Variable module_value of pkg.module\nmodule_value = make_value()"
        );
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn self_hover_uses_parameter_header_and_class_type() {
        let mut editor = crate::editor::Editor::new(512);
        editor.insert_str(
            "class FcmSenderService:\n    def __init__(self, saq_session: AnnSaqDBSession):\n        self.fcm_token_repo = FcmTokenRepository(saq_session)\n",
        );
        let hover_offset = editor.get_full_text().find("self").unwrap();
        let sig = source_signature_for_hover(
            &editor,
            hover_offset,
            false,
            Some("Self@__init__ (invariant)"),
            Some("car_wash.core.fcm.service"),
        )
        .unwrap();
        assert_eq!(
            sig,
            "## Parameter self of car_wash.core.fcm.service.FcmSenderService.__init__\nself: FcmSenderService"
        );
    }

    #[test]
    fn bound_method_hover_triggers_source_replacement() {
        assert!(should_replace_hover_with_source_signature(
            "bound method Self@send_notification_by_booking.get_notif_and_data_by_state(\n    booking: BookingRead,\n    state: StateEnum\n) -> tuple[Notification, dict[str, str]]"
        ));
    }
}

pub(super) fn should_replace_hover_with_source_signature(clean_msg: &str) -> bool {
    should_replace_simple_type_hover(clean_msg) || {
        let trimmed = clean_msg.trim_start();
        trimmed.starts_with("bound method ")
            || ((trimmed.starts_with('(')
                || trimmed.starts_with(") ->")
                || trimmed.starts_with("(_:"))
                && clean_msg.contains("_AsyncGeneratorContextManager"))
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

pub(crate) fn source_hover_parts_for_editor(
    editor: &crate::editor::Editor,
    byte_offset: usize,
    text: String,
    module_path: Option<&str>,
) -> (
    String,
    Vec<crate::highlighter::ColorSpan>,
    Vec<crate::lsp::HoverLineKindPublic>,
    Vec<(usize, usize)>,
) {
    let (clean_msg, spans, line_kinds, inline_code_ranges) =
        crate::lsp::highlight_hover_text(&text);
    let is_simple_type = should_replace_simple_type_hover(&clean_msg);
    if !should_replace_hover_with_source_signature(&clean_msg) {
        return (clean_msg, spans, line_kinds, inline_code_ranges);
    }

    let lsp_ty = is_simple_type.then_some(clean_msg.as_str());
    if let Some(sig) =
        source_signature_for_hover(editor, byte_offset, !is_simple_type, lsp_ty, module_path)
    {
        crate::lsp::highlight_hover_text(&sig)
    } else {
        (clean_msg, spans, line_kinds, inline_code_ranges)
    }
}

pub(crate) fn source_hover_popup_for_editor(
    editor: &crate::editor::Editor,
    byte_offset: usize,
    text: String,
    module_path: Option<&str>,
    anchor: (f32, f32),
) -> crate::app::mouse::HoverPopup {
    let (text, spans, line_kinds, inline_code_ranges) =
        source_hover_parts_for_editor(editor, byte_offset, text, module_path);
    crate::app::mouse::HoverPopup {
        text,
        spans,
        line_kinds,
        inline_code_ranges,
        byte_offset,
        anchor_x: anchor.0,
        anchor_y: anchor.1,
        offset_x: None,
        offset_y: None,
        anim_progress: 0.0,
        scroll: crate::scroll::ScrollState::new(15.0),
        layout_cache: None,
    }
}

pub(crate) fn source_hover_popup_from_response_for_editor(
    editor: &crate::editor::Editor,
    byte_offset: usize,
    text: String,
    module_path: Option<&str>,
    anchor: (f32, f32),
) -> Option<crate::app::mouse::HoverPopup> {
    let (clean_msg, _, _, _) = crate::lsp::highlight_hover_text(&text);
    let hovered_symbol = symbol_at_offset(editor, byte_offset);
    if clean_msg.trim() == "None" && hovered_symbol.as_deref() == Some("await") {
        None
    } else {
        Some(source_hover_popup_for_editor(
            editor,
            byte_offset,
            text,
            module_path,
            anchor,
        ))
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_source_hover_response_to_state<F>(
    state: &mut crate::app::mouse::HoverState,
    request_id: i32,
    editor: &crate::editor::Editor,
    state_byte_offset: usize,
    editor_byte_offset: usize,
    text: Option<String>,
    module_path: Option<&str>,
    anchor: (f32, f32),
    request_definition: F,
) -> bool
where
    F: FnOnce() -> Option<i32>,
{
    if state.request_id != Some(request_id) || state.byte_offset != Some(state_byte_offset) {
        return false;
    }
    state.request_id = None;
    let Some(text) = text else {
        state.popup = None;
        state.pending_popup = None;
        state.rect = None;
        return true;
    };
    let Some(mut popup) = source_hover_popup_from_response_for_editor(
        editor,
        editor_byte_offset,
        text,
        module_path,
        anchor,
    ) else {
        state.popup = None;
        state.pending_popup = None;
        state.rect = None;
        return true;
    };
    popup.byte_offset = state_byte_offset;
    state.pending_popup = None;
    state.definition_request_id = None;
    state.hide_diagnostic_popup_until_ready();
    state.definition_request_id = request_definition();
    if state.definition_request_id.is_some() {
        state.pending_popup = Some(popup);
    } else {
        state.finish_stale_combined_transition();
        state.popup = Some(popup);
    }
    state.selection_anchor = None;
    state.selection_cursor = None;
    state.selecting = false;
    true
}

#[cfg(test)]
mod agent3_regression_tests {
    use super::*;

    #[test]
    fn source_function_offset_counts_crlf_bytes_exactly() {
        let text = "# first\r\n# second\r\ndef target(value: int) -> int:\r\n    return value\r\n";
        assert_eq!(
            source_function_name_offset(text, "target"),
            text.find("target")
        );
        assert!(source_function_signature_from_text(text, "target", Some("sample")).is_some());
    }
}
