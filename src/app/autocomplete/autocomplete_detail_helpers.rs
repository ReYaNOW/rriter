use super::*;

fn autocomplete_trace_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("RRITER_TRACE_AUTOCOMPLETE").is_some())
}

const AUTOCOMPLETE_CACHE_MAX_ITEMS: usize = 256;
const AUTOCOMPLETE_DETAIL_DECL_MAX_CHARS: usize = 112;

fn python_completion_context(file_extension: &str, text: &str) -> bool {
    matches!(file_extension, "py" | "pyi")
        || text.lines().take(200).any(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("from ") || trimmed.starts_with("import ")
        })
}

fn autocomplete_detail_cache_item(
    item: &crate::lsp::LspCompletionItem,
) -> AutocompleteDetailCacheItem {
    AutocompleteDetailCacheItem {
        kind: item.kind,
        detail: item.detail.clone(),
        module: item.module.clone(),
        module_path: item.module.clone(),
    }
}

fn apply_autocomplete_detail_cache_item(
    item: &mut AutocompleteItem,
    cached: &AutocompleteDetailCacheItem,
    member_dot_context: bool,
) {
    let incoming_kind = completion_detail_kind(cached.kind, cached.detail.as_deref());
    let effective_kind = if incoming_kind == SymbolKind::Unknown {
        item.kind
    } else {
        incoming_kind
    };
    if item.kind == SymbolKind::Parameter {
        if item.detail.is_none() && cached.detail.is_some() {
            item.detail = cached.detail.clone();
        }
        return;
    }
    item.kind = effective_kind;
    if item.detail.is_none() && cached.detail.is_some() {
        item.detail = cached.detail.clone();
    }
    if !member_dot_context
        && (matches!(
            effective_kind,
            SymbolKind::Variable | SymbolKind::Parameter | SymbolKind::Property
        ) || completion_is_lowercase_type_source(
            &item.word,
            cached.module.as_deref(),
            cached.detail.as_deref(),
        ))
    {
        if completion_is_lowercase_type_source(
            &item.word,
            cached.module.as_deref(),
            cached.detail.as_deref(),
        ) {
            item.kind = SymbolKind::Variable;
        }
        if item
            .module_path
            .as_deref()
            .is_some_and(completion_source_is_module_path)
            || item
                .module
                .as_deref()
                .is_some_and(completion_source_is_module_path)
        {
            return;
        }
        item.module = None;
        item.module_path = None;
        return;
    }
    if item.module_path.is_none() && cached.module_path.is_some() {
        item.module_path = cached.module_path.clone();
    }
    if let Some(module) = cached
        .module
        .as_deref()
        .filter(|module| should_replace_completion_module(item.module.as_deref(), module))
    {
        item.module = Some(module.to_string());
    }
    assign_builtin_completion_module(item);
}

fn autocomplete_source_attr_class_detail(
    item: &AutocompleteItem,
    detail: &str,
    owner: &str,
) -> Option<String> {
    if !matches!(item.kind, SymbolKind::Variable | SymbolKind::Property) {
        return None;
    }
    let class_name = autocomplete_detail_type_name(detail)?;
    let class_label = class_name.rsplit('.').next().unwrap_or(class_name);
    if completion_source_is_module_path(owner) {
        return Some(format!("(variable) {}: {class_label}", item.word));
    }
    Some(format!("class {class_label}"))
}

fn autocomplete_detail_type_label(detail: Option<&str>) -> Option<String> {
    let detail = detail?.trim();
    if detail.is_empty() || detail.contains('\n') {
        return None;
    }
    let detail = detail
        .strip_prefix("(variable) ")
        .or_else(|| detail.strip_prefix("(parameter) "))
        .or_else(|| detail.strip_prefix("(property) "))
        .unwrap_or(detail);
    let ty = detail
        .rsplit_once(':')
        .map(|(_, ty)| ty.trim())
        .unwrap_or(detail);
    (!ty.is_empty()).then(|| ty.to_string())
}

fn autocomplete_middle_ellipsis(text: &str, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars || max_chars <= 3 {
        return text.to_string();
    }
    let keep = max_chars - 3;
    let head = keep / 2;
    let tail = keep - head;
    let mut out = String::with_capacity(text.len().min(max_chars + 8));
    out.extend(text.chars().take(head));
    out.push_str("...");
    out.extend(text.chars().skip(char_count.saturating_sub(tail)));
    out
}

fn autocomplete_truncate_source_detail(text: String) -> String {
    let mut out = String::with_capacity(text.len());
    for (idx, line) in text.lines().enumerate() {
        if idx > 0 {
            out.push('\n');
        }
        if idx == 0 {
            out.push_str(line);
        } else if line.chars().count() > AUTOCOMPLETE_DETAIL_DECL_MAX_CHARS {
            out.push_str(&autocomplete_middle_ellipsis(
                line,
                AUTOCOMPLETE_DETAIL_DECL_MAX_CHARS,
            ));
        } else {
            out.push_str(line);
        }
    }
    out
}

fn autocomplete_push_dedent_line(out: &mut Vec<String>, line: &str, base_indent: usize) {
    let current_indent = line.len() - line.trim_start().len();
    if current_indent >= base_indent {
        out.push(line[base_indent..].to_string());
    } else {
        out.push(line.trim_start().to_string());
    }
}

fn autocomplete_scan_brackets(
    line: &str,
    paren_depth: &mut i32,
    bracket_depth: &mut i32,
    brace_depth: &mut i32,
    in_string: &mut bool,
    string_char: &mut char,
) {
    for c in line.chars() {
        if *in_string {
            if c == '\\' {
                continue;
            }
            if c == *string_char {
                *in_string = false;
            }
        } else {
            match c {
                '"' | '\'' => {
                    *in_string = true;
                    *string_char = c;
                }
                '(' => *paren_depth += 1,
                ')' => *paren_depth -= 1,
                '[' => *bracket_depth += 1,
                ']' => *bracket_depth -= 1,
                '{' => *brace_depth += 1,
                '}' => *brace_depth -= 1,
                _ => {}
            }
        }
    }
}

fn autocomplete_collect_assignment(lines: &[&str], idx: usize) -> String {
    let mut paren_depth = 0;
    let mut bracket_depth = 0;
    let mut brace_depth = 0;
    let mut in_string = false;
    let mut string_char = ' ';
    let mut statement_lines = vec![lines[idx].trim_end().to_string()];

    autocomplete_scan_brackets(
        lines[idx],
        &mut paren_depth,
        &mut bracket_depth,
        &mut brace_depth,
        &mut in_string,
        &mut string_char,
    );
    let mut curr_idx = idx;
    while (paren_depth > 0 || bracket_depth > 0 || brace_depth > 0) && curr_idx + 1 < lines.len() {
        curr_idx += 1;
        let line = lines[curr_idx].trim_end();
        statement_lines.push(line.to_string());
        autocomplete_scan_brackets(
            line,
            &mut paren_depth,
            &mut bracket_depth,
            &mut brace_depth,
            &mut in_string,
            &mut string_char,
        );
    }
    statement_lines.join("\n")
}

fn autocomplete_format_assignment(statement: &str, symbol: &str, lsp_type: Option<&str>) -> String {
    let mut lines_iter = statement.lines();
    let first = lines_iter.next().unwrap_or("");
    let base_indent = first.len() - first.trim_start().len();
    let mut out = vec![first.trim_start().to_string()];
    for line in lines_iter {
        autocomplete_push_dedent_line(&mut out, line, base_indent);
    }
    let mut assignment = out.join("\n").trim_end().to_string();
    if let Some(ty) = lsp_type {
        if assignment.starts_with(&format!("{symbol} =")) {
            let replacement = format!("{symbol}: {ty} =");
            assignment = assignment.replacen(&format!("{symbol} ="), &replacement, 1);
        } else if assignment.starts_with(&format!("{symbol}=")) {
            let replacement = format!("{symbol}: {ty} =");
            assignment = assignment.replacen(&format!("{symbol}="), &replacement, 1);
        }
    }
    assignment
}

fn autocomplete_source_variable_detail_from_text(
    text: &str,
    symbol: &str,
    module_path: Option<&str>,
    lsp_type: Option<&str>,
) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    for idx in 0..lines.len() {
        let trimmed = lines[idx].trim_start();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        let matched = trimmed.strip_prefix(symbol).is_some_and(|rest| {
            rest.starts_with(':') || rest.starts_with(" =") || rest.starts_with('=')
        });
        if !matched {
            continue;
        }

        let statement = autocomplete_collect_assignment(&lines, idx);
        let assignment = autocomplete_format_assignment(&statement, symbol, lsp_type);
        let mut class_name = None;
        for up in (0..idx).rev() {
            let class_line = lines[up].trim_start();
            if let Some(rest) = class_line.strip_prefix("class ") {
                class_name = rest
                    .split(|c: char| c == '(' || c == ':' || c.is_whitespace() || c == '[')
                    .next()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                break;
            }
        }

        let detail = if let Some(class_name) = class_name {
            let owner = module_path
                .filter(|module| !module.is_empty())
                .map(|module| format!("{module}.{class_name}"))
                .unwrap_or(class_name);
            format!("## Class attribute {symbol} of {owner}\n{assignment}")
        } else if let Some(module) = module_path.filter(|module| !module.is_empty()) {
            format!("## Variable {symbol} of {module}\n{assignment}")
        } else {
            format!("## Variable {symbol}\n{assignment}")
        };
        return Some(autocomplete_truncate_source_detail(detail));
    }
    None
}

fn autocomplete_module_source_path(module_path: &str, workspaces: &[PathBuf]) -> Option<PathBuf> {
    if !completion_source_is_module_path(module_path) {
        return None;
    }
    let rel = module_path.replace('.', "/");
    for root in workspaces {
        let file = root.join(format!("{rel}.py"));
        if file.is_file() {
            return Some(file);
        }
        let init = root.join(&rel).join("__init__.py");
        if init.is_file() {
            return Some(init);
        }
    }
    None
}

fn autocomplete_detail_class_name(detail: &str) -> Option<&str> {
    detail.split('|').find_map(|part| {
        let part = part.trim();
        part.strip_prefix("<class '")
            .and_then(|rest| rest.strip_suffix("'>"))
            .map(str::trim)
            .filter(|name| !name.is_empty())
    })
}

fn autocomplete_detail_type_name(detail: &str) -> Option<&str> {
    autocomplete_detail_class_name(detail).or_else(|| {
        let detail = detail.trim();
        (is_class_like_type_name(detail) && !detail.contains('[')).then_some(detail)
    })
}

fn autocomplete_detail_type_module_path(
    detail: Option<&str>,
    imports: Option<&FxHashMap<String, String>>,
    fallback_module: Option<&str>,
) -> Option<String> {
    let class_name = autocomplete_detail_type_name(detail?)?;
    if completion_source_is_module_path(class_name) {
        return Some(class_name.to_string());
    }
    let class_label = class_name.rsplit('.').next().unwrap_or(class_name);
    imports
        .and_then(|imports| imports.get(class_label))
        .map(|module| format!("{module}.{class_label}"))
        .or_else(|| {
            fallback_module
                .map(normalized_completion_source)
                .filter(|module| completion_source_is_module_path(module))
                .map(|module| {
                    if module.ends_with(&format!(".{class_label}")) {
                        module.to_string()
                    } else {
                        format!("{module}.{class_label}")
                    }
                })
        })
}

fn python_known_function_completion(item: &AutocompleteItem) -> bool {
    matches!(item.word.as_str(), "cast")
        && item
            .module
            .as_deref()
            .or(item.module_path.as_deref())
            .is_some_and(|module| {
                let module = normalized_completion_source(module);
                module == "typing" || module.starts_with("typing.")
            })
        && item
            .detail
            .as_deref()
            .is_some_and(|detail| detail.trim_start().starts_with("Overload["))
}

fn python_keyword_completion(word: &str) -> bool {
    matches!(
        word,
        "False"
            | "None"
            | "True"
            | "and"
            | "as"
            | "assert"
            | "async"
            | "await"
            | "break"
            | "case"
            | "class"
            | "continue"
            | "def"
            | "del"
            | "elif"
            | "else"
            | "except"
            | "finally"
            | "for"
            | "from"
            | "global"
            | "if"
            | "import"
            | "in"
            | "is"
            | "lambda"
            | "match"
            | "nonlocal"
            | "not"
            | "or"
            | "pass"
            | "raise"
            | "return"
            | "try"
            | "while"
            | "with"
            | "yield"
    )
}

fn python_scoped_self_priority(word: &str) -> u8 {
    match word {
        "self" => 0,
        "cls" => 1,
        _ => 2,
    }
}

fn python_low_priority_member_name(word: &str) -> bool {
    matches!(word, "mro")
}

fn infer_python_member_owner(
    current_text: &str,
    imported_modules: Option<&FxHashMap<String, String>>,
    workspaces: &[PathBuf],
    current_path: Option<&Path>,
    items: &[AutocompleteItem],
    fallback: Option<&str>,
) -> Option<String> {
    let mut owner_candidates = FxHashSet::default();
    if let Some(owner) = fallback {
        owner_candidates.insert(owner.to_string());
    }
    let mut item_owners = Vec::new();
    for item in items {
        let Some(owner) = completion_item_owner_label(item) else {
            continue;
        };
        if imported_modules.is_some_and(|imports| imports.contains_key(&owner))
            || fallback == Some(owner.as_str())
        {
            owner_candidates.insert(owner.clone());
        }
        item_owners.push(owner);
    }

    let mut best = fallback.map(str::to_string);
    let mut best_score = 0usize;
    for owner in owner_candidates {
        let Some(source) =
            imported_python_class_source(current_text, workspaces, current_path, &owner)
        else {
            continue;
        };
        let depths =
            python_class_owner_depths_with_imports(&source, workspaces, current_path, &owner);
        let score = item_owners
            .iter()
            .filter(|item_owner| depths.contains_key(item_owner.as_str()))
            .count();
        if score > best_score {
            best_score = score;
            best = Some(owner);
        }
    }
    best
}

