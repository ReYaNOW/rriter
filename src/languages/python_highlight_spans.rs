pub fn ts_capture_role(name: &str) -> Option<crate::theme::SyntaxRole> {
    match name {
        "fg" | "property" | "py_assign" | "variable" => Some(crate::theme::SyntaxRole::Fg),
        "string" => Some(crate::theme::SyntaxRole::String),
        "comment" => Some(crate::theme::SyntaxRole::Comment),
        "function" | "py_function" | "py_builtin_or_func" => Some(crate::theme::SyntaxRole::Function),
        "keyword.control" | "operator" | "boolean" => Some(crate::theme::SyntaxRole::KeywordControl),
        "keyword" | "subst" | "type" | "function.builtin" => Some(crate::theme::SyntaxRole::Keyword),
        "class_name" => Some(crate::theme::SyntaxRole::Class),
        "constant" | "number" => Some(crate::theme::SyntaxRole::Constant),
        "parameter" => Some(crate::theme::SyntaxRole::Parameter),
        _ => None,
    }
}

pub fn push_python_ts_spans(
    code: &str,
    global_start: usize,
    spans: &mut Vec<crate::highlighter::ColorSpan>,
) {
    let mut best_spans: HashMap<(usize, usize), (u8, crate::theme::SyntaxRole)> = HashMap::new();

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
                                let Some(role) = ts_capture_role(name) else {
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
                                let entry = best_spans.entry(key).or_insert((prio, role));
                                if prio >= entry.0 {
                                    *entry = (prio, role);
                                }
                            }
                        }
                    }
                }
            })
        })
    });

    let class_attr_ranges = python_class_attr_name_ranges(code);
    for (start, end) in &class_attr_ranges {
        best_spans.remove(&(global_start + *start, global_start + *end));
    }

    for ((start, end), (_, role)) in best_spans {
        spans.push(crate::highlighter::ColorSpan { start, end, role });
    }
    for (start, end) in class_attr_ranges {
        spans.push(crate::highlighter::ColorSpan {
            start: global_start + start,
            end: global_start + end,
            role: crate::theme::SyntaxRole::Fg,
        });
    }
}

pub(crate) fn python_class_attr_name_ranges(code: &str) -> Vec<(usize, usize)> {
    #[derive(Clone, Copy)]
    struct ClassScope {
        indent: usize,
        body_indent: Option<usize>,
        header_delimiters: super::PythonDelimiterState,
        header_complete: bool,
    }

    let mut ranges = Vec::new();
    let mut offset = 0usize;
    let mut classes = Vec::<ClassScope>::new();
    for line in code.lines() {
        let trimmed = line.trim_start();
        let indent = line.len().saturating_sub(trimmed.len());

        while classes.last().is_some_and(|scope| {
            scope.header_complete && !trimmed.is_empty() && indent <= scope.indent
        }) {
            classes.pop();
        }

        if let Some(scope) = classes.last_mut()
            && !scope.header_complete
        {
            scope.header_delimiters.scan_line(line);
            scope.header_complete =
                !scope.header_delimiters.has_open_delimiter() && trimmed.trim_end().ends_with(':');
            offset = offset.saturating_add(line.len()).saturating_add(1);
            continue;
        }

        if let Some(scope) = classes.last_mut()
            && !trimmed.is_empty()
            && !trimmed.starts_with('#')
            && indent > scope.indent
        {
            let direct_indent = *scope.body_indent.get_or_insert(indent);
            if indent == direct_indent
                && let Some(name) = python_class_direct_attr(trimmed)
            {
                let start = offset + indent;
                ranges.push((start, start + name.len()));
            }
        }

        if python_class_header_name(line).is_some() {
            let mut header_delimiters = super::PythonDelimiterState::default();
            header_delimiters.scan_line(line);
            classes.push(ClassScope {
                indent,
                body_indent: None,
                header_complete: !header_delimiters.has_open_delimiter()
                    && trimmed.trim_end().ends_with(':'),
                header_delimiters,
            });
        }

        offset = offset.saturating_add(line.len()).saturating_add(1);
    }
    ranges
}

pub fn highlight_python_hover_doc(
    raw_msg: &str,
) -> (
    String,
    Vec<crate::highlighter::ColorSpan>,
    Vec<HoverLineKindPublic>,
    Vec<(usize, usize)>,
) {
    let text_light = crate::theme::SyntaxRole::Fg;
    let ty = crate::theme::SyntaxRole::Keyword;
    let neutral = crate::theme::SyntaxRole::Fg;
    let param = crate::theme::SyntaxRole::Parameter;

    let (msg, mut line_kinds, inline_code_ranges) = normalize_python_hover_doc(raw_msg);
    let lines: Vec<&str> = msg.split('\n').collect();
    let mut line_starts = Vec::with_capacity(lines.len());
    let mut at = 0usize;
    for line in &lines {
        line_starts.push(at);
        at += line.len() + 1;
    }
    let mut spans = Vec::new();
    let mut signature_brackets = Vec::new();
    let mut signature_type_tokens = Vec::new();

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
                role: crate::theme::SyntaxRole::KeywordControl,
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
                    role: crate::theme::SyntaxRole::Function,
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
                    role: crate::theme::SyntaxRole::KeywordControl,
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
                        role: crate::theme::SyntaxRole::Keyword,
                    });
                }
            }

            for k in line_kinds.iter_mut().take(end_line + 1).skip(start_line) {
                *k = HoverLineKind::Code;
            }
        }

        for line_no in start_line..=end_line {
            for (idx, ch) in lines[line_no].char_indices() {
                if ch == '[' || ch == ']' {
                    let abs = line_starts[line_no] + idx;
                    signature_brackets.push((abs, abs + 1));
                }
            }
        }
        signature_type_tokens =
            signature_type_token_ranges(&lines, &line_starts, start_line, end_line);
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

    if !signature_brackets.is_empty() {
        force_role_on_ranges(&mut spans, &signature_brackets, neutral);
    }
    if !signature_type_tokens.is_empty() {
        force_role_on_ranges(&mut spans, &signature_type_tokens, ty);
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
                role: text_light,
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
                        role: crate::theme::SyntaxRole::KeywordControl,
                    });
                    spans.push(crate::highlighter::ColorSpan {
                        start: line_start + of_idx + separator.len(),
                        end: line_end,
                        role: ty,
                    });
                } else {
                    spans.push(crate::highlighter::ColorSpan {
                        start: line_start + prefix_len,
                        end: line_end,
                        role: crate::theme::SyntaxRole::KeywordControl,
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
                        role: param,
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

fn force_role_on_ranges(
    spans: &mut Vec<crate::highlighter::ColorSpan>,
    ranges: &[(usize, usize)],
    role: crate::theme::SyntaxRole,
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
                        role: piece.role,
                    });
                }
                if piece.end > force_end {
                    next.push(crate::highlighter::ColorSpan {
                        start: force_end,
                        end: piece.end,
                        role: piece.role,
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
            .map(|&(start, end)| crate::highlighter::ColorSpan { start, end, role }),
    );
    *spans = out;
}

fn signature_type_token_ranges(
    lines: &[&str],
    line_starts: &[usize],
    start_line: usize,
    end_line: usize,
) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    for line_no in start_line..=end_line {
        let Some(line) = lines.get(line_no).copied() else {
            continue;
        };
        let line_start = line_starts.get(line_no).copied().unwrap_or(0);
        let mut scan_from = 0usize;
        while let Some(colon_rel) = line[scan_from..].find(':') {
            let start = scan_from + colon_rel + 1;
            let end = type_slice_end(line, start);
            push_type_tokens(line, line_start, start, end, &mut ranges);
            scan_from = end.saturating_add(1).min(line.len());
        }
        scan_from = 0;
        while let Some(arrow_rel) = line[scan_from..].find("->") {
            let start = scan_from + arrow_rel + 2;
            push_type_tokens(line, line_start, start, line.len(), &mut ranges);
            scan_from = start;
        }
    }
    ranges
}

fn type_slice_end(line: &str, start: usize) -> usize {
    let mut paren_depth = 0i32;
    let mut bracket_depth = 0i32;
    let mut in_string = None;
    for (idx, ch) in line[start..].char_indices() {
        let abs = start + idx;
        if let Some(quote) = in_string {
            if ch == quote {
                in_string = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            in_string = Some(ch);
            continue;
        }
        match ch {
            '(' => paren_depth += 1,
            ')' if paren_depth > 0 => paren_depth -= 1,
            '[' => bracket_depth += 1,
            ']' if bracket_depth > 0 => bracket_depth -= 1,
            ',' if paren_depth == 0 && bracket_depth == 0 => return abs,
            _ => {}
        }
    }
    line.len()
}

fn push_type_tokens(
    line: &str,
    line_start: usize,
    start: usize,
    end: usize,
    ranges: &mut Vec<(usize, usize)>,
) {
    let mut token_start = None;
    let mut in_string = None;
    for (idx, ch) in line[start..end].char_indices() {
        let abs = start + idx;
        if let Some(quote) = in_string {
            if ch == quote {
                in_string = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            if let Some(token_start) = token_start.take() {
                ranges.push((line_start + token_start, line_start + abs));
            }
            in_string = Some(ch);
            continue;
        }
        if ch.is_alphanumeric() || ch == '_' || ch == '.' {
            token_start.get_or_insert(abs);
        } else if let Some(token_start) = token_start.take() {
            ranges.push((line_start + token_start, line_start + abs));
        }
    }
    if let Some(token_start) = token_start {
        ranges.push((line_start + token_start, line_start + end));
    }
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
                            role: crate::theme::SyntaxRole::Parameter,
                        });
                    }
                }
            }
        }
        i += 1;
    }
}
