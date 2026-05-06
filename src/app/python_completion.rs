use super::*;
pub(crate) fn initial_python_bracket_folds(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut folds = Vec::new();
    let mut line_start = 0usize;
    for raw_line in text.split_inclusive('\n') {
        let line_end = line_start + raw_line.len();
        let line = raw_line.trim_end_matches('\n').trim_end_matches('\r');
        let code = line.split('#').next().unwrap_or("").trim_end();
        let Some(opener) = code.as_bytes().last().copied() else {
            line_start = line_end;
            continue;
        };
        let closer = match opener {
            b'{' => b'}',
            b'[' => b']',
            _ => {
                line_start = line_end;
                continue;
            }
        };
        let opener_byte = line_start + code.len().saturating_sub(1);
        let mut depth = 0usize;
        let mut p = opener_byte;
        let mut close_byte = None;
        while p < bytes.len() {
            let b = bytes[p];
            if b == opener {
                depth += 1;
            } else if b == closer {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    close_byte = Some(p);
                    break;
                }
            }
            p += 1;
        }
        if let Some(close_byte) = close_byte {
            if text[line_start..close_byte].contains('\n') {
                let end = text[close_byte..]
                    .find('\n')
                    .map(|rel| close_byte + rel)
                    .unwrap_or(text.len());
                folds.push((opener_byte, end));
            }
        }
        line_start = line_end;
    }
    folds
}

pub(crate) fn is_python_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80
}

pub(crate) fn is_plain_assignment_after_token(after_token: &str) -> bool {
    let bytes = after_token.as_bytes();
    for (idx, &b) in bytes.iter().enumerate() {
        if b != b'=' {
            continue;
        }
        let prev = idx.checked_sub(1).and_then(|i| bytes.get(i)).copied();
        let next = bytes.get(idx + 1).copied();
        if matches!(prev, Some(b'=' | b'!' | b'<' | b'>' | b':')) || next == Some(b'=') {
            continue;
        }
        return true;
    }
    false
}

pub(crate) fn plain_assignment_index(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    for (idx, &b) in bytes.iter().enumerate() {
        if b != b'=' {
            continue;
        }
        let prev = idx.checked_sub(1).and_then(|i| bytes.get(i)).copied();
        let next = bytes.get(idx + 1).copied();
        if matches!(prev, Some(b'=' | b'!' | b'<' | b'>' | b':')) || next == Some(b'=') {
            continue;
        }
        return Some(idx);
    }
    None
}

pub(crate) fn token_occurrence_at_word_boundary(
    text: &str,
    token: &str,
    search_start: usize,
) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut cursor = search_start.min(text.len());
    while cursor < text.len() {
        let rel = text.get(cursor..)?.find(token)?;
        let start = cursor + rel;
        let end = start + token.len();
        let left_ok = start == 0 || !is_python_ident_byte(bytes[start - 1]);
        let right_ok = end >= bytes.len() || !is_python_ident_byte(bytes[end]);
        if left_ok && right_ok {
            return Some(start);
        }
        cursor = end;
    }
    None
}

pub(crate) fn previous_token_occurrence_at_word_boundary(
    text: &str,
    token: &str,
    search_end: usize,
) -> Option<usize> {
    let mut best = None;
    let mut cursor = 0;
    while let Some(pos) = token_occurrence_at_word_boundary(text, token, cursor) {
        if pos >= search_end {
            break;
        }
        best = Some(pos);
        cursor = pos + token.len();
    }
    best
}

pub(crate) fn nearest_python_assignment_usage(
    editor: &Editor,
    source_range: (usize, usize),
) -> Option<usize> {
    let text = editor.get_full_text();
    let (start, end) = source_range;
    let token = text.get(start..end)?;
    if token.is_empty() {
        return None;
    }

    let bytes = text.as_bytes();
    let line_start = bytes[..start.min(bytes.len())]
        .iter()
        .rposition(|&b| b == b'\n')
        .map(|pos| pos + 1)
        .unwrap_or(0);
    let line_end = bytes[end.min(bytes.len())..]
        .iter()
        .position(|&b| b == b'\n')
        .map(|pos| end + pos)
        .unwrap_or(bytes.len());

    let before = text.get(line_start..start)?.trim_start();
    if before.starts_with("def ") || before.starts_with("class ") {
        return None;
    }
    let line = text.get(line_start..line_end)?;
    let Some(eq_idx) = plain_assignment_index(line) else {
        return None;
    };
    let target_end = line[..eq_idx].find(':').unwrap_or(eq_idx);
    let source_start_in_line = start.saturating_sub(line_start);
    let source_end_in_line = end.saturating_sub(line_start);
    if source_start_in_line >= target_end || source_end_in_line > target_end {
        return None;
    }
    if !text
        .get(end..line_end)
        .is_some_and(is_plain_assignment_after_token)
    {
        return None;
    }

    token_occurrence_at_word_boundary(&text, token, line_end)
        .or_else(|| previous_token_occurrence_at_word_boundary(&text, token, line_start))
}

pub(crate) fn cursor_line_bounds(text: &str, cursor: usize) -> (usize, usize) {
    let cursor = cursor.min(text.len());
    let start = text.as_bytes()[..cursor]
        .iter()
        .rposition(|&b| b == b'\n')
        .map(|pos| pos + 1)
        .unwrap_or(0);
    let end = text.as_bytes()[cursor..]
        .iter()
        .position(|&b| b == b'\n')
        .map(|pos| cursor + pos)
        .unwrap_or(text.len());
    (start, end)
}

pub(crate) fn cursor_in_python_string_or_comment(line_prefix: &str) -> bool {
    let mut single = false;
    let mut double = false;
    let mut escaped = false;
    for b in line_prefix.bytes() {
        if escaped {
            escaped = false;
            continue;
        }
        if b == b'\\' {
            escaped = true;
            continue;
        }
        if !single && !double && b == b'#' {
            return true;
        }
        if !double && b == b'\'' {
            single = !single;
        } else if !single && b == b'"' {
            double = !double;
        }
    }
    single || double
}

pub(crate) fn python_import_completion_allowed(editor: &Editor) -> bool {
    let text = editor.get_full_text();
    let cursor = editor.cursor.min(text.len());
    let (line_start, line_end) = cursor_line_bounds(&text, cursor);
    let line = text.get(line_start..line_end).unwrap_or("");
    let prefix = text.get(line_start..cursor).unwrap_or("");
    if cursor_in_python_string_or_comment(prefix) {
        return false;
    }
    let trimmed = line.trim_start();
    if trimmed.starts_with("def ")
        || trimmed.starts_with("async ")
        || trimmed.starts_with("class ")
        || trimmed.starts_with("from ")
        || trimmed.starts_with("import ")
    {
        return false;
    }
    let bytes = text.as_bytes();
    let prev_ident = cursor
        .checked_sub(1)
        .and_then(|idx| bytes.get(idx))
        .is_some_and(|&b| is_python_ident_byte(b));
    let next_ident = bytes.get(cursor).is_some_and(|&b| is_python_ident_byte(b));
    !prev_ident && !next_ident
}

pub(crate) fn is_magic_python_name(name: &str) -> bool {
    name.len() > 4 && name.starts_with("__") && name.ends_with("__")
}

pub(crate) fn normalized_completion_source(source: &str) -> &str {
    let source = source.trim();
    source
        .strip_prefix("<class '")
        .and_then(|s| s.strip_suffix("'>"))
        .or_else(|| {
            source
                .strip_prefix("<module '")
                .and_then(|s| s.strip_suffix("'>"))
        })
        .unwrap_or(source)
        .trim()
}

pub(crate) fn is_plain_python_type_name(text: &str) -> bool {
    matches!(
        text,
        "Any"
            | "None"
            | "bool"
            | "bytes"
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

pub(crate) fn is_builtin_class_completion_detail(word: &str, detail: &str) -> bool {
    let source = normalized_completion_source(detail);
    source == word && is_plain_python_type_name(source)
}

pub(crate) fn completion_source_missing_or_plain_builtin_type(
    word: &str,
    source: Option<&str>,
) -> bool {
    source.is_none_or(|source| {
        let source = normalized_completion_source(source);
        source.is_empty()
            || completion_source_is_builtin(source)
            || source == word && is_plain_python_type_name(source)
    })
}

pub(crate) fn should_assign_builtin_completion_module(item: &AutocompleteItem) -> bool {
    if matches!(
        item.kind,
        SymbolKind::Variable | SymbolKind::Parameter | SymbolKind::Property
    ) {
        return false;
    }
    if !item.detail.as_deref().is_none_or(|detail| {
        detail.starts_with("type[")
            || is_builtin_class_completion_detail(&item.word, detail)
            || normalized_completion_source(detail) == item.word
                && is_plain_python_type_name(&item.word)
    }) {
        return false;
    }
    if item.module.is_none() && item.module_path.is_none() {
        return true;
    }
    is_plain_python_type_name(&item.word)
        && completion_source_missing_or_plain_builtin_type(&item.word, item.module.as_deref())
        && completion_source_missing_or_plain_builtin_type(&item.word, item.module_path.as_deref())
}

pub(crate) fn assign_builtin_completion_module(item: &mut AutocompleteItem) {
    if should_assign_builtin_completion_module(item)
        && let Some(module) = python_builtin_completion_module(&item.word)
    {
        item.module = Some(module.to_string());
        item.module_path = Some(format!("{module}.{}", item.word));
    }
}

pub(crate) fn python_builtin_completion_kind(word: &str) -> Option<SymbolKind> {
    match word {
        "int" | "str" | "list" | "dict" | "set" | "tuple" | "bool" | "float" | "type"
        | "map" | "filter" | "range" | "reversed" | "super" | "Exception" | "ValueError"
        | "TypeError" | "KeyError" | "IndexError" | "AttributeError" | "RuntimeError"
        | "KeyboardInterrupt" => Some(SymbolKind::Class),
        "print" | "len" | "sum" | "min" | "max" | "abs" | "isinstance" | "issubclass"
        | "hasattr" | "getattr" | "setattr" | "delattr" | "dir" | "enumerate" | "zip"
        | "open" => Some(SymbolKind::Function),
        _ => None,
    }
}

pub(crate) fn is_type_like_completion_source(text: &str) -> bool {
    let s = normalized_completion_source(text);
    if s.is_empty() {
        return false;
    }
    if s.contains("module") || s.contains('/') || s.contains('\\') {
        return false;
    }
    s.contains('|')
        || s.contains('[')
        || s.contains(']')
        || s.contains("->")
        || s.starts_with("def ")
        || s.starts_with("async def ")
        || s.starts_with("overload[")
        || s.starts_with('(')
        || is_plain_python_type_name(s)
}

pub(crate) fn is_class_like_type_name(text: &str) -> bool {
    let name = text.rsplit('.').next().unwrap_or(text).trim();
    !name.is_empty()
        && name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && name.chars().any(|c| c.is_ascii_lowercase())
}

pub(crate) fn completion_item_has_explicit_owner(item: &AutocompleteItem) -> bool {
    let Some(module) = item.module.as_deref() else {
        return false;
    };
    let Some(owner) = item
        .detail
        .as_deref()
        .and_then(|detail| completion_owner_from_detail(&item.word, detail))
    else {
        return false;
    };
    completion_owner_label_from_source(module).as_deref()
        == Some(
            completion_owner_label_from_source(owner)
                .as_deref()
                .unwrap_or(owner),
        )
}

pub(crate) fn completion_item_is_field_like(item: &AutocompleteItem) -> bool {
    if matches!(
        item.kind,
        SymbolKind::Variable | SymbolKind::Parameter | SymbolKind::Property
    ) {
        return true;
    }
    if item.detail.as_deref().is_some_and(|detail| {
        detail.starts_with("(variable)")
            || detail.starts_with("(parameter)")
            || detail.starts_with("(property)")
            || detail.starts_with("(field)")
    }) {
        return true;
    }
    let lower_attr = item
        .word
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c == '_');
    lower_attr
        && item.module.as_deref().is_some_and(is_class_like_type_name)
        && item.detail.as_deref().is_none_or(|detail| {
            detail.contains(&format!(": {}", item.module.as_deref().unwrap_or("")))
        })
}

pub(crate) fn completion_item_is_argument_like(item: &AutocompleteItem) -> bool {
    matches!(item.kind, SymbolKind::Parameter)
        || item
            .detail
            .as_deref()
            .is_some_and(|detail| detail.starts_with("(parameter)") || detail.contains("parameter"))
        || item.word.ends_with('=')
        || item
            .insert_text
            .as_deref()
            .is_some_and(|text| text.contains('='))
}

pub(crate) fn completion_detail_is_type_label(detail: &str) -> bool {
    let detail = detail.trim();
    !detail.is_empty()
        && !detail.starts_with("(variable)")
        && !detail.starts_with("(parameter)")
        && !detail.starts_with("(property)")
        && !detail.starts_with("(field)")
        && !detail.starts_with("bound ")
        && !detail.starts_with("def ")
        && !detail.starts_with("async def ")
        && !detail.contains('\n')
}

pub(crate) fn completion_needs_python_source_attr_owner(item: &AutocompleteItem) -> bool {
    if !completion_word_starts_lower(&item.word)
        || item.word.starts_with("__")
        || completion_item_has_explicit_owner(item)
    {
        return false;
    }
    completion_item_is_field_like(item)
        || item
            .detail
            .as_deref()
            .is_some_and(completion_detail_is_type_label)
        || item.detail.is_none() && item.module.is_some()
}

pub(crate) fn completion_owner_from_detail<'a>(word: &str, detail: &'a str) -> Option<&'a str> {
    if word.is_empty() {
        return None;
    }
    let needle = format!(".{word}");
    let idx = detail.find(&needle)?;
    let before = &detail[..idx];
    let owner_start = before
        .char_indices()
        .rev()
        .find(|(_, ch)| !(ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '.'))
        .map(|(idx, ch)| idx + ch.len_utf8())
        .unwrap_or(0);
    let owner = before[owner_start..].trim().trim_matches('`');
    (!owner.is_empty()).then_some(owner)
}

pub(crate) fn completion_owner_label_from_source(source: &str) -> Option<String> {
    let class_repr = source.trim().starts_with("<class '");
    let source = normalized_completion_source(source);
    if source.is_empty()
        || !class_repr && is_type_like_completion_source(source)
        || source.contains('/')
        || source.contains('\\')
        || !source
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
    {
        return None;
    }
    let owner = source.rsplit('.').next().unwrap_or(source).trim();
    (!owner.is_empty()).then(|| owner.to_string())
}

pub(crate) fn completion_source_label_is_clean(source: &str) -> bool {
    let source = normalized_completion_source(source);
    !source.is_empty()
        && !is_type_like_completion_source(source)
        && !source.contains('/')
        && !source.contains('\\')
        && source
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

pub(crate) fn completion_source_is_module_path(source: &str) -> bool {
    let source = normalized_completion_source(source);
    completion_source_label_is_clean(source)
        && !is_plain_python_type_name(source)
        && (source.contains('.')
            || source
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_lowercase()))
}

pub(crate) fn completion_item_source_is_field_type(item: &AutocompleteItem) -> bool {
    let Some(source) = item
        .module
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return false;
    };
    let Some(detail) = item.detail.as_deref() else {
        return false;
    };
    detail
        .rsplit_once(':')
        .map(|(_, ty)| ty.trim())
        .is_some_and(|ty| ty == source)
}

pub(crate) fn completion_item_owner_label(item: &AutocompleteItem) -> Option<String> {
    if let Some(owner) = item
        .detail
        .as_deref()
        .and_then(|detail| completion_owner_from_detail(&item.word, detail))
    {
        return completion_owner_label_from_source(owner).or_else(|| Some(owner.to_string()));
    }
    if completion_item_source_is_field_type(item) {
        return None;
    }
    item.module
        .as_deref()
        .and_then(completion_owner_label_from_source)
}

pub(crate) fn completion_parent_module_label(item: &AutocompleteItem) -> Option<String> {
    let source = item
        .module_path
        .as_deref()
        .or(item.module.as_deref())
        .map(normalized_completion_source)?;
    if !completion_source_is_module_path(source) {
        return None;
    }
    let suffix = format!(".{}", item.word);
    Some(
        source
            .strip_suffix(&suffix)
            .filter(|parent| !parent.is_empty())
            .unwrap_or(source)
            .to_string(),
    )
}

pub(crate) fn set_completion_owner_source(
    item: &mut AutocompleteItem,
    owner: String,
    imports: Option<&FxHashMap<String, String>>,
    fallback_module: Option<&str>,
) {
    if item
        .module_path
        .as_deref()
        .is_none_or(|source| !completion_source_is_module_path(source))
        || completion_item_source_is_field_type(item)
    {
        item.module_path = imports
            .and_then(|imports| imports.get(&owner))
            .map(|module| format!("{module}.{owner}"))
            .or_else(|| fallback_module.map(|module| format!("{module}.{owner}")));
    }
    item.module = Some(owner);
}

pub(crate) fn autocomplete_detail_module_path(item: &AutocompleteItem) -> Option<&str> {
    item.module_path
        .as_deref()
        .filter(|source| completion_source_is_module_path(source))
        .or_else(|| {
            item.module
                .as_deref()
                .filter(|source| completion_source_is_module_path(source))
        })
}

pub(crate) fn prepend_autocomplete_detail_module_path(
    popup: &mut crate::app::mouse::HoverPopup,
    module_path: &str,
) {
    crate::app::file_tree::pre_rasterize_icon("folder", true);
    let prefix = format!("[[MODULE]] {module_path}\n");
    if popup.text.starts_with(&prefix) {
        return;
    }
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
    let mut line_kinds = Vec::with_capacity(popup.line_kinds.len() + 1);
    line_kinds.push(crate::lsp::HoverLineKindPublic::Text);
    line_kinds.extend(popup.line_kinds.iter().copied());
    popup.line_kinds = line_kinds;
}

pub(crate) fn normalize_ty_import_kind(item: &mut AutocompleteItem) {
    if item.kind != SymbolKind::Unknown {
        return;
    }
    let Some(module) = item.module.as_deref() else {
        return;
    };
    if module.is_empty() {
        return;
    }
    item.kind = SymbolKind::Module;
}

pub(crate) fn common_completion_owner(items: &[AutocompleteItem]) -> Option<String> {
    let mut counts: FxHashMap<String, usize> = FxHashMap::default();
    for item in items {
        let Some(owner) = completion_item_owner_label(item) else {
            continue;
        };
        if !is_class_like_type_name(&owner) {
            continue;
        }
        if completion_needs_python_source_attr_owner(item) {
            continue;
        }
        *counts.entry(owner).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(module, _)| module)
}

pub(crate) fn imported_python_module_for_symbol(text: &str, symbol: &str) -> Option<String> {
    imported_python_symbols(text).remove(symbol)
}

pub(crate) fn imported_python_symbols(text: &str) -> FxHashMap<String, String> {
    let mut symbols = FxHashMap::default();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("from ") {
            let Some((module, imports)) = rest.split_once(" import ") else {
                continue;
            };
            let module = module.trim();
            if module.is_empty() {
                continue;
            }
            let mut add_import = |item: &str| {
                let item = item.trim().trim_end_matches(',');
                if item.is_empty() || item == ")" {
                    return false;
                }
                let mut parts = item.split(" as ");
                let name = parts.next().unwrap_or("").trim();
                let visible = parts.next().unwrap_or(name).trim();
                if !visible.is_empty() && !name.is_empty() {
                    symbols.insert(visible.to_string(), module.to_string());
                }
                false
            };
            if imports.trim_start().starts_with('(') {
                for import_line in lines.by_ref() {
                    if add_import(import_line.trim()) {
                        break;
                    }
                    if import_line.trim() == ")" {
                        break;
                    }
                }
            } else {
                for item in imports.split(',') {
                    add_import(item);
                }
            }
            continue;
        }
        let Some(imports) = trimmed.strip_prefix("import ") else {
            continue;
        };
        for item in imports.split(',') {
            let mut parts = item.trim().split(" as ");
            let module = parts.next().unwrap_or("").trim();
            if module.is_empty() {
                continue;
            }
            let visible = parts
                .next()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| module.split('.').next().unwrap_or(module));
            symbols.insert(visible.to_string(), module.to_string());
        }
    }
    symbols
}

pub(crate) fn should_replace_completion_module(current: Option<&str>, incoming: &str) -> bool {
    let incoming = incoming.trim();
    if incoming.is_empty() {
        return false;
    }
    let incoming_path = incoming.contains('.')
        && incoming
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
    match current {
        None => true,
        Some(current) => {
            let current_path = current.contains('.')
                && current
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
            incoming_path && !current_path
        }
    }
}

pub(crate) fn completion_detail_kind(kind: SymbolKind, detail: Option<&str>) -> SymbolKind {
    let Some(detail) = detail else {
        return kind;
    };
    if detail.starts_with("(parameter)") {
        SymbolKind::Parameter
    } else if detail.starts_with("(variable)") {
        SymbolKind::Variable
    } else if detail.starts_with("(property)") || detail.starts_with("(field)") {
        SymbolKind::Property
    } else {
        kind
    }
}

pub(crate) fn completion_source_is_builtin(source: &str) -> bool {
    let source = normalized_completion_source(source);
    source == "builtins" || source.starts_with("builtins.")
}

pub(crate) fn completion_word_starts_lower(word: &str) -> bool {
    word.chars()
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c == '_')
}

pub(crate) fn completion_is_lowercase_type_source(
    word: &str,
    module: Option<&str>,
    detail: Option<&str>,
) -> bool {
    let Some(module) = module else {
        return false;
    };
    completion_word_starts_lower(word)
        && is_class_like_type_name(module)
        && detail.is_none_or(|detail| {
            detail.contains(&format!(": {module}")) || !detail.starts_with("type[")
        })
}

pub(crate) fn python_builtin_completion_module(word: &str) -> Option<&'static str> {
    match word {
        "print" | "len" | "int" | "str" | "list" | "dict" | "set" | "tuple" | "bool" | "float"
        | "sum" | "min" | "max" | "abs" | "isinstance" | "issubclass" | "hasattr" | "getattr"
        | "setattr" | "delattr" | "dir" | "type" | "enumerate" | "zip" | "map" | "filter"
        | "range" | "reversed" | "open" | "super" | "Exception" | "ValueError" | "TypeError"
        | "KeyError" | "IndexError" | "AttributeError" | "RuntimeError" | "KeyboardInterrupt" => {
            Some("builtins")
        }
        _ => None,
    }
}

pub(crate) fn ty_autocomplete_context_key(
    text: &str,
    line_offsets: &[usize],
    cursor: usize,
    prefix: &str,
    mode: AutocompleteMode,
) -> String {
    let anchor = cursor.saturating_sub(prefix.len()).min(text.len());
    let line = line_offsets
        .partition_point(|&offset| offset <= anchor)
        .saturating_sub(1);
    let line_start = line_offsets.get(line).copied().unwrap_or(0).min(anchor);
    let context = text.get(line_start..anchor).unwrap_or("").trim_end();
    format!("{mode:?}|{context}")
}

pub(crate) fn apply_import_modules_to_autocomplete_items(
    items: &mut [(AutocompleteItem, Vec<usize>)],
    imports: &FxHashMap<String, String>,
) {
    for (item, _) in items {
        if let Some(module) = imports.get(&item.word) {
            if should_replace_completion_module(item.module.as_deref(), module) {
                item.module = Some(module.clone());
            }
            if item.module_path.is_none() {
                item.module_path = Some(module.clone());
            }
            if item
                .word
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_uppercase())
            {
                item.kind = SymbolKind::Class;
            }
        }
    }
}

pub(crate) fn class_header_bases(line: &str, class_name: &str) -> Option<Vec<String>> {
    let trimmed = line.trim_start();
    let prefix = format!("class {class_name}");
    let rest = trimmed.strip_prefix(&prefix)?;
    if rest
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }
    if rest.trim_start().starts_with(':') {
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
                (!name.is_empty()).then(|| name.to_string())
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
            let Some(bases) = class_header_bases(line, class_name) else {
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
            let Some(bases) = class_header_bases(line, class_name) else {
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
        for line in &lines {
            let Some(bases) = class_header_bases(line, class_label) else {
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
    let mut depth = 0usize;
    for b in prefix.bytes() {
        match b {
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth > 0
}
