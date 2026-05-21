fn python_imported_symbol_is_known_function(word: &str, module: &str) -> bool {
    let module = normalized_completion_source(module);
    matches!(
        (module, word),
        ("contextlib", "asynccontextmanager") | ("typing", "cast")
    )
}

pub(crate) fn class_header_bases(line: &str, class_name: &str) -> Option<Vec<String>> {
    let trimmed = line.trim_start();
    let class_label = class_name.rsplit('.').next().unwrap_or(class_name);
    let prefix = format!("class {class_label}");
    let mut rest = trimmed.strip_prefix(&prefix)?;
    if rest
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }
    rest = rest.trim_start();
    if rest.starts_with('[') {
        let mut depth = 0usize;
        let mut generic_end = None;
        for (idx, ch) in rest.char_indices() {
            match ch {
                '[' => depth += 1,
                ']' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        generic_end = Some(idx + ch.len_utf8());
                        break;
                    }
                }
                _ => {}
            }
        }
        rest = rest.get(generic_end?..).unwrap_or("").trim_start();
    }
    if rest.starts_with(':') {
        return Some(Vec::new());
    }
    let open = rest.find('(')?;
    let close = rest.rfind(')')?;
    if close <= open {
        return Some(Vec::new());
    }
    Some(
        rest[open + 1..close]
            .split(',')
            .filter_map(|base| {
                let name = base
                    .trim()
                    .split(|c: char| c == '[' || c == '(' || c.is_whitespace())
                    .next()
                    .unwrap_or("")
                    .trim();
                let valid = name
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                    && name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
                valid.then(|| name.to_string())
            })
            .collect(),
    )
}

pub(crate) fn class_direct_attr(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("def ")
        || trimmed.starts_with("async def ")
        || trimmed.starts_with('@')
        || trimmed.starts_with("class ")
    {
        return None;
    }
    let end = trimmed
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(trimmed.len());
    if end == 0 {
        return None;
    }
    let rest = trimmed[end..].trim_start();
    (rest.starts_with(':') || rest.starts_with('=')).then_some(&trimmed[..end])
}

pub(crate) fn instance_attr_assignment_name(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let rest = trimmed
        .strip_prefix("self.")
        .or_else(|| trimmed.strip_prefix("cls."))?;
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    if end == 0 {
        return None;
    }
    let after = rest[end..].trim_start();
    (after.starts_with(':') || is_plain_assignment_after_token(after)).then_some(&rest[..end])
}

pub(crate) fn class_header_name(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("class ")?;
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    (end > 0).then_some(&rest[..end])
}

fn python_header_depth_delta(line: &str) -> i32 {
    let code = line.split('#').next().unwrap_or("");
    code.chars().fold(0, |depth, ch| match ch {
        '(' | '[' | '{' => depth + 1,
        ')' | ']' | '}' => depth - 1,
        _ => depth,
    })
}

fn python_class_header_complete(line: &str, depth: i32) -> bool {
    depth <= 0 && line.trim_end().ends_with(':')
}

fn python_class_bases_at(lines: &[&str], idx: usize, class_name: &str) -> Option<Vec<String>> {
    let class_label = class_name.rsplit('.').next().unwrap_or(class_name);
    let first = *lines.get(idx)?;
    if class_header_name(first) != Some(class_label) {
        return None;
    }
    let mut header = first.split('#').next().unwrap_or("").trim_end().to_string();
    let mut depth = python_header_depth_delta(first);
    let mut line_idx = idx;
    while !python_class_header_complete(&header, depth) && line_idx + 1 < lines.len() {
        line_idx += 1;
        let segment = lines[line_idx].split('#').next().unwrap_or("").trim();
        if !segment.is_empty() {
            header.push(' ');
            header.push_str(segment);
        }
        depth += python_header_depth_delta(lines[line_idx]);
    }
    class_header_bases(&header, class_label).or_else(|| Some(Vec::new()))
}

pub(crate) fn source_contains_python_class(source: &str, class_name: &str) -> bool {
    let class_label = class_name.rsplit('.').next().unwrap_or(class_name);
    source
        .lines()
        .any(|line| class_header_name(line) == Some(class_label))
}

fn python_def_name(trimmed: &str) -> Option<&str> {
    let rest = trimmed
        .strip_prefix("async def ")
        .or_else(|| trimmed.strip_prefix("def "))?;
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    (end > 0).then_some(&rest[..end])
}

pub(crate) fn python_enclosing_class_before_cursor(text: &str, cursor: usize) -> Option<String> {
    let cursor = cursor.min(text.len());
    let mut classes: Vec<(usize, String)> = Vec::new();
    let mut pending_class: Option<(usize, String, i32)> = None;
    let mut byte = 0usize;
    for line in text.lines() {
        if byte > cursor {
            break;
        }
        let trimmed = line.trim_start();
        let indent = line.len().saturating_sub(trimmed.len());
        if let Some((class_indent, name, depth)) = pending_class.take() {
            let depth = depth + python_header_depth_delta(line);
            if python_class_header_complete(line, depth) {
                classes.push((class_indent, name));
            } else {
                pending_class = Some((class_indent, name, depth));
            }
            byte = byte.saturating_add(line.len()).saturating_add(1);
            continue;
        }
        if !trimmed.is_empty() {
            while classes
                .last()
                .is_some_and(|(class_indent, _)| indent <= *class_indent)
            {
                classes.pop();
            }
            if let Some(name) = class_header_name(line) {
                let depth = python_header_depth_delta(line);
                if python_class_header_complete(line, depth) {
                    classes.push((indent, name.to_string()));
                } else {
                    pending_class = Some((indent, name.to_string(), depth));
                }
            }
        }
        byte = byte.saturating_add(line.len()).saturating_add(1);
    }
    classes.last().map(|(_, name)| name.clone())
}

fn trim_python_block_indent(line: &str, indent: usize) -> &str {
    line.get(indent..).unwrap_or_else(|| line.trim_start())
}

fn python_signature_block(lines: &[&str], def_idx: usize, direct_indent: usize) -> String {
    let mut out = String::new();
    let mut paren_depth = 0i32;
    for line in lines.iter().skip(def_idx) {
        let trimmed = line.trim_start();
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(trim_python_block_indent(line, direct_indent).trim_end());
        for ch in trimmed.chars() {
            match ch {
                '(' | '[' | '{' => paren_depth += 1,
                ')' | ']' | '}' => paren_depth -= 1,
                _ => {}
            }
        }
        if trimmed.ends_with(": ...") || trimmed.ends_with(':') && paren_depth <= 0 {
            break;
        }
    }
    out
}

pub(crate) fn python_class_method_overload_detail(
    source: &str,
    class_name: &str,
    method: &str,
) -> Option<String> {
    let class_label = class_name.rsplit('.').next().unwrap_or(class_name);
    let lines: Vec<&str> = source.lines().collect();
    let mut blocks = Vec::new();
    for (class_idx, line) in lines.iter().enumerate() {
        let class_indent = line.len().saturating_sub(line.trim_start().len());
        if python_class_bases_at(&lines, class_idx, class_label).is_none() {
            continue;
        }
        let direct_indent = class_indent + 4;
        let mut idx = class_idx + 1;
        while idx < lines.len() {
            let line = lines[idx];
            if line.trim().is_empty() {
                idx += 1;
                continue;
            }
            let indent = line.len().saturating_sub(line.trim_start().len());
            if indent <= class_indent {
                break;
            }
            if indent != direct_indent {
                idx += 1;
                continue;
            }
            let mut decorators = Vec::new();
            while idx < lines.len() {
                let trimmed = lines[idx].trim_start();
                let indent = lines[idx].len().saturating_sub(trimmed.len());
                if indent == direct_indent && trimmed.starts_with('@') {
                    decorators.push(trimmed);
                    idx += 1;
                    continue;
                }
                break;
            }
            if idx >= lines.len() {
                break;
            }
            let trimmed = lines[idx].trim_start();
            if python_def_name(trimmed) == Some(method)
                && decorators.iter().any(|line| line.contains("overload"))
            {
                let mut block = String::new();
                for decorator in decorators {
                    if !block.is_empty() {
                        block.push('\n');
                    }
                    let normalized = if decorator == "@typing.overload"
                        || decorator == "@typing_extensions.overload"
                        || decorator.ends_with(".overload")
                    {
                        std::borrow::Cow::Borrowed("@overload")
                    } else {
                        std::borrow::Cow::Borrowed(decorator)
                    };
                    block.push_str(normalized.as_ref());
                }
                if !block.is_empty() {
                    block.push('\n');
                }
                block.push_str(&python_signature_block(&lines, idx, direct_indent));
                blocks.push(block);
            }
            idx += 1;
        }
        break;
    }
    (!blocks.is_empty()).then(|| blocks.join("\n\n"))
}

#[cfg(test)]
pub(crate) fn class_body_declares_attr(
    lines: &[&str],
    class_idx: usize,
    class_indent: usize,
    attr: &str,
) -> bool {
    let direct_indent = class_indent + 4;
    let mut type_checking_attr_indent = None;
    for body in lines.iter().skip(class_idx + 1) {
        if body.trim().is_empty() {
            continue;
        }
        let indent = body.len().saturating_sub(body.trim_start().len());
        if indent <= class_indent {
            break;
        }
        if let Some(instance_attr) = instance_attr_assignment_name(body)
            && instance_attr == attr
        {
            return true;
        }
        if indent == direct_indent {
            type_checking_attr_indent = None;
            if class_direct_attr(body) == Some(attr) {
                return true;
            }
            let trimmed = body.trim();
            if trimmed == "if TYPE_CHECKING:" {
                type_checking_attr_indent = Some(indent + 4);
            }
            continue;
        }
        if type_checking_attr_indent == Some(indent) && class_direct_attr(body) == Some(attr) {
            return true;
        }
    }
    false
}

pub(crate) fn class_body_declared_attrs(
    lines: &[&str],
    class_idx: usize,
    class_indent: usize,
    attrs: &FxHashSet<String>,
    owner: &str,
    out: &mut FxHashMap<String, String>,
) {
    let direct_indent = class_indent + 4;
    let mut type_checking_attr_indent = None;
    for body in lines.iter().skip(class_idx + 1) {
        if body.trim().is_empty() {
            continue;
        }
        let indent = body.len().saturating_sub(body.trim_start().len());
        if indent <= class_indent {
            break;
        }
        if let Some(attr) = instance_attr_assignment_name(body)
            && attrs.contains(attr)
        {
            out.entry(attr.to_string())
                .or_insert_with(|| owner.to_string());
        }
        if indent == direct_indent {
            type_checking_attr_indent = None;
            if let Some(attr) = class_direct_attr(body)
                && attrs.contains(attr)
            {
                out.entry(attr.to_string())
                    .or_insert_with(|| owner.to_string());
            }
            let trimmed = body.trim();
            if trimmed == "if TYPE_CHECKING:" {
                type_checking_attr_indent = Some(indent + 4);
            }
            continue;
        }
        if type_checking_attr_indent == Some(indent)
            && let Some(attr) = class_direct_attr(body)
            && attrs.contains(attr)
        {
            out.entry(attr.to_string())
                .or_insert_with(|| owner.to_string());
        }
    }
}

fn class_body_declared_methods(
    lines: &[&str],
    class_idx: usize,
    class_indent: usize,
    names: &FxHashSet<String>,
    owner: &str,
    out: &mut FxHashMap<String, String>,
) {
    let direct_indent = class_indent + 4;
    for body in lines.iter().skip(class_idx + 1) {
        if body.trim().is_empty() {
            continue;
        }
        let indent = body.len().saturating_sub(body.trim_start().len());
        if indent <= class_indent {
            break;
        }
        if indent == direct_indent
            && let Some(method) = python_def_name(body.trim_start())
            && names.contains(method)
        {
            out.entry(method.to_string())
                .or_insert_with(|| owner.to_string());
        }
    }
}

#[cfg(test)]
pub(crate) fn python_class_attr_owner_in_source(
    source: &str,
    class_name: &str,
    attr: &str,
) -> Option<String> {
    fn find(source: &str, class_name: &str, attr: &str, depth: usize) -> Option<String> {
        if depth > 8 {
            return None;
        }
        let lines: Vec<&str> = source.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            let class_indent = line.len().saturating_sub(line.trim_start().len());
            let Some(bases) = python_class_bases_at(&lines, idx, class_name) else {
                continue;
            };
            if class_body_declares_attr(&lines, idx, class_indent, attr) {
                return Some(class_name.to_string());
            }
            for base in bases {
                if let Some(owner) = find(source, &base, attr, depth + 1) {
                    return Some(owner);
                }
            }
        }
        None
    }
    find(source, class_name, attr, 0)
}

pub(crate) fn python_class_attr_owners_with_imports(
    source: &str,
    workspaces: &[PathBuf],
    current_path: Option<&Path>,
    class_name: &str,
    attrs: &FxHashSet<String>,
) -> FxHashMap<String, String> {
    fn find(
        source: &str,
        workspaces: &[PathBuf],
        current_path: Option<&Path>,
        class_name: &str,
        attrs: &FxHashSet<String>,
        depth: usize,
        seen: &mut FxHashSet<String>,
        out: &mut FxHashMap<String, String>,
    ) {
        if depth > 8 || out.len() >= attrs.len() {
            return;
        }
        let seen_key = format!("{:p}:{}:{class_name}", source.as_ptr(), source.len());
        if !seen.insert(seen_key) {
            return;
        }
        let lines: Vec<&str> = source.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            let class_indent = line.len().saturating_sub(line.trim_start().len());
            let Some(bases) = python_class_bases_at(&lines, idx, class_name) else {
                continue;
            };
            class_body_declared_attrs(&lines, idx, class_indent, attrs, class_name, out);
            if out.len() >= attrs.len() {
                return;
            }
            for base in bases {
                find(
                    source,
                    workspaces,
                    current_path,
                    &base,
                    attrs,
                    depth + 1,
                    seen,
                    out,
                );
                if out.len() >= attrs.len() {
                    return;
                }
                if let Some(base_source) =
                    imported_python_class_source(source, workspaces, current_path, &base)
                {
                    find(
                        &base_source,
                        workspaces,
                        current_path,
                        &base,
                        attrs,
                        depth + 1,
                        seen,
                        out,
                    );
                    if out.len() >= attrs.len() {
                        return;
                    }
                }
            }
        }
    }

    let mut out = FxHashMap::default();
    if !attrs.is_empty() {
        find(
            source,
            workspaces,
            current_path,
            class_name,
            attrs,
            0,
            &mut FxHashSet::default(),
            &mut out,
        );
    }
    out
}

pub(crate) fn python_class_member_owners_with_imports(
    source: &str,
    workspaces: &[PathBuf],
    current_path: Option<&Path>,
    class_name: &str,
    names: &FxHashSet<String>,
) -> FxHashMap<String, String> {
    fn find(
        source: &str,
        workspaces: &[PathBuf],
        current_path: Option<&Path>,
        class_name: &str,
        names: &FxHashSet<String>,
        depth: usize,
        seen: &mut FxHashSet<String>,
        out: &mut FxHashMap<String, String>,
    ) {
        if depth > 8 || out.len() >= names.len() {
            return;
        }
        let class_label = class_name.rsplit('.').next().unwrap_or(class_name);
        let seen_key = format!("{:p}:{}:{class_label}", source.as_ptr(), source.len());
        if !seen.insert(seen_key) {
            return;
        }
        let lines: Vec<&str> = source.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            let class_indent = line.len().saturating_sub(line.trim_start().len());
            let Some(bases) = python_class_bases_at(&lines, idx, class_label) else {
                continue;
            };
            class_body_declared_methods(&lines, idx, class_indent, names, class_label, out);
            if out.len() >= names.len() {
                return;
            }
            for base in bases {
                let base_label = base.rsplit('.').next().unwrap_or(&base);
                find(
                    source,
                    workspaces,
                    current_path,
                    base_label,
                    names,
                    depth + 1,
                    seen,
                    out,
                );
                if out.len() >= names.len() {
                    return;
                }
                if let Some(base_source) =
                    imported_python_class_source(source, workspaces, current_path, base_label)
                {
                    find(
                        &base_source,
                        workspaces,
                        current_path,
                        base_label,
                        names,
                        depth + 1,
                        seen,
                        out,
                    );
                    if out.len() >= names.len() {
                        return;
                    }
                }
            }
        }
    }

    let mut out = FxHashMap::default();
    if !names.is_empty() {
        find(
            source,
            workspaces,
            current_path,
            class_name,
            names,
            0,
            &mut FxHashSet::default(),
            &mut out,
        );
    }
    out
}

pub(crate) fn python_class_owner_depths_with_imports(
    source: &str,
    workspaces: &[PathBuf],
    current_path: Option<&Path>,
    class_name: &str,
) -> FxHashMap<String, u8> {
    fn find(
        source: &str,
        workspaces: &[PathBuf],
        current_path: Option<&Path>,
        class_name: &str,
        depth: usize,
        seen: &mut FxHashSet<String>,
        out: &mut FxHashMap<String, u8>,
    ) {
        if depth > 8 {
            return;
        }
        let class_label = class_name.rsplit('.').next().unwrap_or(class_name);
        out.entry(class_label.to_string())
            .or_insert(depth.min(u8::MAX as usize) as u8);
        let seen_key = format!("{:p}:{}:{class_label}", source.as_ptr(), source.len());
        if !seen.insert(seen_key) {
            return;
        }
        let lines: Vec<&str> = source.lines().collect();
        for (idx, _line) in lines.iter().enumerate() {
            let Some(bases) = python_class_bases_at(&lines, idx, class_label) else {
                continue;
            };
            for base in bases {
                let base_label = base.rsplit('.').next().unwrap_or(&base);
                find(
                    source,
                    workspaces,
                    current_path,
                    base_label,
                    depth + 1,
                    seen,
                    out,
                );
                if let Some(base_source) =
                    imported_python_class_source(source, workspaces, current_path, base_label)
                {
                    find(
                        &base_source,
                        workspaces,
                        current_path,
                        base_label,
                        depth + 1,
                        seen,
                        out,
                    );
                }
            }
        }
    }

    let mut out = FxHashMap::default();
    find(
        source,
        workspaces,
        current_path,
        class_name,
        0,
        &mut FxHashSet::default(),
        &mut out,
    );
    out
}

pub(crate) fn imported_python_class_source(
    current_text: &str,
    workspaces: &[PathBuf],
    current_path: Option<&Path>,
    class_name: &str,
) -> Option<String> {
    let module = imported_python_module_for_symbol(current_text, class_name)?;
    let rel = module.replace('.', "/");
    let candidates = [rel.clone() + ".py", rel + ".pyi"];
    let first_segment = module.split('.').next().unwrap_or("");
    for ws in workspaces {
        for rel in &candidates {
            let path = ws.join(rel);
            if let Ok(source) = std::fs::read_to_string(path) {
                return Some(source);
            }
            if ws.file_name().and_then(|name| name.to_str()) == Some(first_segment) {
                if let Some(parent) = ws.parent() {
                    if let Ok(source) = std::fs::read_to_string(parent.join(rel)) {
                        return Some(source);
                    }
                }
            }
        }
    }
    if let Some(path) = current_path.and_then(Path::parent) {
        for root in path.ancestors() {
            for rel in &candidates {
                let candidate = root.join(rel);
                if let Ok(source) = std::fs::read_to_string(candidate) {
                    return Some(source);
                }
            }
        }
    }
    None
}

pub(crate) fn python_member_chain_too_deep(editor: &Editor) -> bool {
    let text = editor.get_full_text();
    let cursor = editor.cursor.min(text.len());
    let (line_start, _) = cursor_line_bounds(&text, cursor);
    let prefix = text.get(line_start..cursor).unwrap_or("");
    if cursor_in_python_string_or_comment(prefix) {
        return false;
    }
    let chain = prefix
        .rsplit(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
        .next()
        .unwrap_or("");
    chain.bytes().filter(|&b| b == b'.').count() > 4
}

pub(crate) fn cursor_after_python_member_dot(editor: &Editor) -> bool {
    let text = editor.get_full_text();
    let cursor = editor.cursor.min(text.len());
    let (line_start, _) = cursor_line_bounds(&text, cursor);
    let prefix = text.get(line_start..cursor).unwrap_or("");
    if cursor_in_python_string_or_comment(prefix) {
        return false;
    }
    let bytes = prefix.as_bytes();
    let mut idx = bytes.len();
    while idx > 0 {
        let b = bytes[idx - 1];
        if is_python_ident_byte(b) {
            idx -= 1;
        } else {
            break;
        }
    }
    if idx == 0 || bytes[idx - 1] != b'.' {
        return false;
    }
    idx >= 2 && is_python_ident_byte(bytes[idx - 2])
}

pub(crate) fn python_member_receiver_before_cursor(editor: &Editor) -> Option<String> {
    let text = editor.get_full_text();
    let cursor = editor.cursor.min(text.len());
    let (line_start, _) = cursor_line_bounds(&text, cursor);
    let prefix = text.get(line_start..cursor)?;
    if cursor_in_python_string_or_comment(prefix) {
        return None;
    }
    let bytes = prefix.as_bytes();
    let mut idx = bytes.len();
    while idx > 0 && is_python_ident_byte(bytes[idx - 1]) {
        idx -= 1;
    }
    if idx == 0 || bytes[idx - 1] != b'.' {
        return None;
    }
    let end = idx - 1;
    let mut start = end;
    while start > 0 && is_python_ident_byte(bytes[start - 1]) {
        start -= 1;
    }
    (start < end).then(|| prefix[start..end].to_string())
}

pub(crate) fn cursor_inside_python_call_parens(editor: &Editor) -> bool {
    let text = editor.get_full_text();
    let cursor = editor.cursor.min(text.len());
    let (line_start, _) = cursor_line_bounds(&text, cursor);
    let prefix = text.get(line_start..cursor).unwrap_or("");
    if cursor_in_python_string_or_comment(prefix) {
        return false;
    }
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    for idx in (0..cursor).rev() {
        match bytes[idx] {
            b')' | b']' | b'}' => depth += 1,
            b'(' | b'[' | b'{' => {
                if depth > 0 {
                    depth -= 1;
                } else {
                    return bytes[idx] == b'(';
                }
            }
            _ => {}
        }
    }
    false
}
