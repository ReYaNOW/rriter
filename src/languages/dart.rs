pub use super::ImportBlock;
use super::finish_import_block;
use std::sync::Arc;
use tree_sitter::StreamingIterator;

thread_local! {
    static TS_HOVER_PARSER: std::cell::RefCell<tree_sitter::Parser> = {
        let mut parser = tree_sitter::Parser::new();
        if let Some((lang, _)) = crate::queries::get_ts_config("dart") {
            let _ = parser.set_language(&lang);
        }
        std::cell::RefCell::new(parser)
    };
    static TS_HOVER_QUERIES: std::cell::RefCell<Vec<tree_sitter::Query>> =
        std::cell::RefCell::new(
            crate::queries::get_ts_config("dart")
                .map(|(lang, queries)| {
                    queries
                        .into_iter()
                        .filter_map(|query| tree_sitter::Query::new(&lang, query).ok())
                        .collect()
                })
                .unwrap_or_default(),
        );
    static TS_HOVER_CURSOR: std::cell::RefCell<tree_sitter::QueryCursor> =
        std::cell::RefCell::new(tree_sitter::QueryCursor::new());
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HoverParseMode {
    CompilationUnit,
    StatementFragment,
    TypeFragment,
}

pub(crate) fn push_hover_highlight_spans(
    source: &str,
    offset: usize,
    spans: &mut Vec<crate::highlighter::ColorSpan>,
    parse_mode: HoverParseMode,
) {
    if source.is_empty() {
        return;
    }
    TS_HOVER_PARSER.with(|parser_cell| {
        TS_HOVER_QUERIES.with(|queries_cell| {
            TS_HOVER_CURSOR.with(|cursor_cell| {
                let mut parser = parser_cell.borrow_mut();
                let queries = queries_cell.borrow();
                let mut cursor = cursor_cell.borrow_mut();
                let Some(mut tree) = parser.parse(source, None) else {
                    return;
                };

                // Dartdoc examples can be executable statements, while Type metadata contains
                // standalone type expressions. Parse each known hover context in matching Dart
                // syntax instead of applying statement recovery to every errored fragment.
                const BODY_PREFIX: &str = "void __rriter_hover__() {\n";
                const TYPE_PREFIX: &str = "void __rriter_hover__(";
                const TYPE_SUFFIX: &str = " __rriter_value__) {}";
                let mut wrapped_source = None;
                let mut source_start = 0usize;
                let wrapper = match parse_mode {
                    HoverParseMode::CompilationUnit => None,
                    HoverParseMode::StatementFragment if tree.root_node().has_error() => {
                        Some((BODY_PREFIX, "\n}"))
                    }
                    HoverParseMode::StatementFragment => None,
                    HoverParseMode::TypeFragment => Some((TYPE_PREFIX, TYPE_SUFFIX)),
                };
                if let Some((prefix, suffix)) = wrapper {
                    let mut wrapped = String::with_capacity(
                        prefix
                            .len()
                            .saturating_add(source.len())
                            .saturating_add(suffix.len()),
                    );
                    wrapped.push_str(prefix);
                    wrapped.push_str(source);
                    wrapped.push_str(suffix);
                    if let Some(wrapped_tree) = parser.parse(&wrapped, None)
                        && !wrapped_tree.root_node().has_error()
                    {
                        tree = wrapped_tree;
                        source_start = prefix.len();
                        wrapped_source = Some(wrapped);
                    }
                }

                let parse_source = wrapped_source.as_deref().unwrap_or(source);
                let source_end = source_start.saturating_add(source.len());
                for query in queries.iter() {
                    let mut matches =
                        cursor.matches(query, tree.root_node(), parse_source.as_bytes());
                    while let Some(query_match) = matches.next() {
                        for capture in query_match.captures {
                            let name = query.capture_names()[capture.index as usize];
                            let start = capture.node.start_byte();
                            let end = capture.node.end_byte();
                            if start < source_start || end > source_end {
                                continue;
                            }
                            let Some(node_text) = parse_source.get(start..end) else {
                                continue;
                            };
                            let color = crate::highlighter::hover_capture_color(name, node_text);
                            if color != crate::highlighter::DRACULA_FG {
                                spans.push(crate::highlighter::ColorSpan {
                                    start: offset + start - source_start,
                                    end: offset + end - source_start,
                                    color,
                                });
                            }
                        }
                    }
                }
            })
        })
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClosingHintMode {
    Off,
    DartServer,
    DartServerAndBlocks,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClosingHintSettings {
    pub mode: ClosingHintMode,
    pub minimum_nesting_depth: usize,
    pub minimum_block_lines: usize,
}

impl Default for ClosingHintSettings {
    fn default() -> Self {
        Self {
            mode: ClosingHintMode::DartServerAndBlocks,
            minimum_nesting_depth: 2,
            minimum_block_lines: 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClosingHintSource {
    DartServer,
    SyntaxTree,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosingHint {
    pub revision: u64,
    pub line: usize,
    pub anchor_byte: usize,
    pub label: Arc<str>,
    pub source: ClosingHintSource,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClosingHintState {
    revision: u64,
    server_hints: Vec<ClosingHint>,
    syntax_hints: Vec<ClosingHint>,
    merged_hints: Vec<ClosingHint>,
}

impl ClosingHintState {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn hints(&self) -> &[ClosingHint] {
        &self.merged_hints
    }

    pub fn invalidate(&mut self, revision: u64) {
        self.revision = revision;
        self.server_hints.clear();
        self.syntax_hints.clear();
        self.merged_hints.clear();
    }

    pub fn replace_server(
        &mut self,
        revision: u64,
        mut hints: Vec<ClosingHint>,
        settings: ClosingHintSettings,
    ) {
        self.prepare_revision(revision);
        if settings.mode == ClosingHintMode::Off {
            self.server_hints.clear();
            self.rebuild(settings);
            return;
        }
        sort_and_deduplicate_hints(&mut hints);
        self.server_hints = hints;
        self.rebuild(settings);
    }

    pub fn replace_syntax(
        &mut self,
        revision: u64,
        mut hints: Vec<ClosingHint>,
        settings: ClosingHintSettings,
    ) {
        self.prepare_revision(revision);
        if settings.mode == ClosingHintMode::Off {
            self.syntax_hints.clear();
            self.rebuild(settings);
            return;
        }
        sort_and_deduplicate_hints(&mut hints);
        self.syntax_hints = hints;
        self.rebuild(settings);
    }

    pub fn apply_settings(&mut self, settings: ClosingHintSettings) {
        if settings.mode == ClosingHintMode::Off {
            self.server_hints.clear();
            self.syntax_hints.clear();
            self.merged_hints.clear();
            return;
        }
        self.rebuild(settings);
    }

    fn prepare_revision(&mut self, revision: u64) {
        if self.revision != revision {
            self.invalidate(revision);
        }
    }

    fn rebuild(&mut self, settings: ClosingHintSettings) {
        self.merged_hints.clear();
        if settings.mode == ClosingHintMode::Off {
            return;
        }

        self.merged_hints.extend(self.server_hints.iter().cloned());
        if settings.mode == ClosingHintMode::DartServerAndBlocks {
            for hint in &self.syntax_hints {
                let server_owns_position = self
                    .server_hints
                    .binary_search_by_key(&(hint.line, hint.anchor_byte), |server| {
                        (server.line, server.anchor_byte)
                    })
                    .is_ok();
                if !server_owns_position {
                    self.merged_hints.push(hint.clone());
                }
            }
        }
        sort_and_deduplicate_hints(&mut self.merged_hints);
    }
}

fn sort_and_deduplicate_hints(hints: &mut Vec<ClosingHint>) {
    hints.sort_unstable_by(|left, right| {
        left.line
            .cmp(&right.line)
            .then(left.anchor_byte.cmp(&right.anchor_byte))
            .then_with(|| left.label.cmp(&right.label))
    });
    hints.dedup_by(|left, right| {
        left.line == right.line
            && left.anchor_byte == right.anchor_byte
            && left.label == right.label
    });
}

pub fn import_blocks(text: &str) -> Vec<ImportBlock> {
    let mut blocks = Vec::new();
    let mut current: Option<ImportBlock> = None;
    let mut pending_blank_lines = 0usize;
    let mut offset = 0usize;

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

        if let Some(keyword_len) = dart_import_keyword_len(trimmed) {
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
            continue;
        }

        pending_blank_lines = 0;
        finish_import_block(&mut current, &mut blocks);
    }

    finish_import_block(&mut current, &mut blocks);
    blocks
}

pub fn local_closing_hints(
    text: &str,
    tree: &tree_sitter::Tree,
    revision: u64,
    settings: ClosingHintSettings,
) -> Vec<ClosingHint> {
    if settings.mode != ClosingHintMode::DartServerAndBlocks || text.is_empty() {
        return Vec::new();
    }

    let minimum_depth = settings.minimum_nesting_depth.max(1);
    let minimum_lines = settings.minimum_block_lines.max(2);
    let mut hints = Vec::new();
    let mut cursor = tree.walk();

    loop {
        let node = cursor.node();
        if node.kind() == "}"
            && let Some((label, owner, opening_line)) = closing_label_for_brace(node, text)
        {
            let close_line = node.start_position().row;
            let block_lines = close_line.saturating_sub(opening_line).saturating_add(1);
            if close_line > opening_line
                && block_lines >= minimum_lines
                && braced_nesting_depth(node) >= minimum_depth
                && !owner.has_error()
                && let Some(anchor_byte) = valid_closing_line_anchor(text, node)
            {
                hints.push(ClosingHint {
                    revision,
                    line: close_line,
                    anchor_byte,
                    label: Arc::<str>::from(label),
                    source: ClosingHintSource::SyntaxTree,
                });
            }
        }

        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                sort_and_deduplicate_hints(&mut hints);
                return hints;
            }
        }
    }
}

pub fn server_closing_hints(
    text: &str,
    revision: u64,
    labels: &[crate::lsp::LspClosingLabel],
) -> Vec<ClosingHint> {
    let mut pending = Vec::with_capacity(labels.len());
    for (order, label) in labels.iter().enumerate() {
        let Some(start) = lsp_position_to_byte(text, label.start_line, label.start_col) else {
            continue;
        };
        let Some(end) = lsp_position_to_byte(text, label.end_line, label.end_col) else {
            continue;
        };
        if start > end || label.start_line != label.end_line {
            continue;
        }
        let close_byte = if start < end {
            let Some(relative) = text
                .get(start..end)
                .and_then(|slice| slice.bytes().position(is_closing_token))
            else {
                continue;
            };
            start.saturating_add(relative)
        } else if text
            .as_bytes()
            .get(start)
            .copied()
            .is_some_and(is_closing_token)
        {
            start
        } else {
            continue;
        };
        let Some((_, anchor_byte)) = line_content_bounds(text, close_byte) else {
            continue;
        };
        let trimmed = label.label.trim();
        if trimmed.is_empty() {
            continue;
        }
        pending.push((
            label.start_line as usize,
            anchor_byte,
            start,
            order,
            trimmed.to_string(),
        ));
    }

    pending.sort_unstable_by_key(|&(line, anchor, start, order, _)| (line, anchor, start, order));
    let mut hints: Vec<ClosingHint> = Vec::new();
    for (line, anchor_byte, _, _, label) in pending {
        if let Some(last) = hints.last_mut()
            && last.line == line
            && last.anchor_byte == anchor_byte
        {
            if !last.label.split(" · ").any(|existing| existing == label) {
                let mut combined = String::with_capacity(last.label.len() + label.len() + 3);
                combined.push_str(&last.label);
                combined.push_str(" · ");
                combined.push_str(&label);
                last.label = Arc::<str>::from(combined);
            }
            continue;
        }
        hints.push(ClosingHint {
            revision,
            line,
            anchor_byte,
            label: Arc::<str>::from(label),
            source: ClosingHintSource::DartServer,
        });
    }
    hints
}

fn dart_import_keyword_len(trimmed: &str) -> Option<usize> {
    if trimmed.starts_with("import ") {
        Some("import".len())
    } else {
        None
    }
}

fn closing_label_for_brace<'tree>(
    brace: tree_sitter::Node<'tree>,
    text: &str,
) -> Option<(String, tree_sitter::Node<'tree>, usize)> {
    let container = brace.parent()?;
    let opening_line = container.start_position().row;
    match container.kind() {
        "class_body" => declaration_label(container.parent()?, text, "class")
            .map(|label| (label, container.parent().unwrap_or(container), opening_line)),
        "enum_body" => declaration_label(container.parent()?, text, "enum")
            .map(|label| (label, container.parent().unwrap_or(container), opening_line)),
        "extension_body" => declaration_label(container.parent()?, text, "extension")
            .map(|label| (label, container.parent().unwrap_or(container), opening_line)),
        "block" => block_label(container, text).map(|(label, owner)| (label, owner, opening_line)),
        "switch_block" => container
            .parent()
            .filter(|parent| parent.kind() == "switch_statement")
            .map(|owner| ("switch".to_string(), owner, opening_line)),
        _ => None,
    }
}

fn declaration_label(node: tree_sitter::Node<'_>, text: &str, keyword: &str) -> Option<String> {
    let expected_kind = match keyword {
        "class" => "class_declaration",
        "enum" => "enum_declaration",
        "extension" => "extension_declaration",
        _ => return None,
    };
    let actual_kind = node.kind();
    if actual_kind != expected_kind
        && !(keyword == "class"
            && matches!(
                actual_kind,
                "mixin_declaration" | "extension_type_declaration"
            ))
    {
        return None;
    }
    let actual_keyword = match actual_kind {
        "mixin_declaration" => "mixin",
        "extension_type_declaration" => "extension type",
        _ => keyword,
    };
    let name = node
        .child_by_field_name("name")
        .and_then(|name| node_text(text, name));
    Some(match name {
        Some(name) if !name.is_empty() => format!("{actual_keyword} {name}"),
        _ => actual_keyword.to_string(),
    })
}

fn block_label<'tree>(
    block: tree_sitter::Node<'tree>,
    text: &str,
) -> Option<(String, tree_sitter::Node<'tree>)> {
    let parent = block.parent()?;
    match parent.kind() {
        "function_body" => {
            let owner = parent.parent()?;
            function_owner_name(owner, text).map(|name| (name, owner))
        }
        "if_statement" => {
            let label = if node_matches_field(parent, "alternative", block) {
                "else"
            } else {
                "if"
            };
            Some((label.to_string(), parent))
        }
        "for_statement" => Some(("for".to_string(), parent)),
        "while_statement" => Some(("while".to_string(), parent)),
        "do_statement" => Some(("do".to_string(), parent)),
        "try_statement" => {
            let label = if node_matches_field(parent, "body", block) {
                "try"
            } else {
                "catch"
            };
            Some((label.to_string(), parent))
        }
        "catch_clause" => Some(("catch".to_string(), parent)),
        "finally_clause" => Some(("finally".to_string(), parent)),
        _ => None,
    }
}

fn function_owner_name(owner: tree_sitter::Node<'_>, text: &str) -> Option<String> {
    if owner.kind() == "function_expression" {
        return None;
    }
    let mut pending = vec![owner];
    while let Some(node) = pending.pop() {
        if matches!(
            node.kind(),
            "function_signature"
                | "constructor_signature"
                | "factory_constructor_signature"
                | "getter_signature"
                | "setter_signature"
                | "operator_signature"
        ) && let Some(name) = node
            .child_by_field_name("name")
            .and_then(|name| node_text(text, name))
            .filter(|name| !name.is_empty())
        {
            return Some(name.to_string());
        }
        pending.extend((0..node.named_child_count()).rev().filter_map(|index| {
            u32::try_from(index)
                .ok()
                .and_then(|index| node.named_child(index))
        }));
    }
    None
}

fn node_matches_field(
    parent: tree_sitter::Node<'_>,
    field: &str,
    node: tree_sitter::Node<'_>,
) -> bool {
    parent.child_by_field_name(field).is_some_and(|candidate| {
        candidate.start_byte() == node.start_byte()
            && candidate.end_byte() == node.end_byte()
            && candidate.kind() == node.kind()
    })
}

fn node_text<'a>(text: &'a str, node: tree_sitter::Node<'_>) -> Option<&'a str> {
    text.get(node.start_byte()..node.end_byte()).map(str::trim)
}

fn braced_nesting_depth(mut node: tree_sitter::Node<'_>) -> usize {
    let mut depth = 1usize;
    while let Some(parent) = node.parent() {
        if matches!(
            parent.kind(),
            "block" | "class_body" | "enum_body" | "extension_body" | "switch_statement"
        ) {
            depth = depth.saturating_add(1);
        }
        node = parent;
    }
    depth
}

fn valid_closing_line_anchor(text: &str, brace: tree_sitter::Node<'_>) -> Option<usize> {
    let (line_start, line_end) = line_content_bounds(text, brace.start_byte())?;
    if !text.get(line_start..brace.start_byte())?.trim().is_empty() {
        return None;
    }
    let suffix = text.get(brace.end_byte()..line_end)?;
    if !closing_suffix_is_allowed(suffix) {
        return None;
    }
    Some(line_end)
}

fn line_content_bounds(text: &str, byte: usize) -> Option<(usize, usize)> {
    if byte > text.len() || !text.is_char_boundary(byte) {
        return None;
    }
    let line_start = text
        .get(..byte)?
        .rfind('\n')
        .map_or(0, |newline| newline.saturating_add(1));
    let mut line_end = text
        .get(byte..)?
        .find('\n')
        .map_or(text.len(), |relative| byte.saturating_add(relative));
    if text.as_bytes().get(line_end.saturating_sub(1)) == Some(&b'\r') {
        line_end = line_end.saturating_sub(1);
    }
    Some((line_start, line_end))
}

fn closing_suffix_is_allowed(mut suffix: &str) -> bool {
    suffix = suffix.trim_start();
    while let Some(first) = suffix.as_bytes().first().copied() {
        if matches!(first, b',' | b';' | b')' | b']') {
            suffix = suffix.get(1..).unwrap_or("").trim_start();
            continue;
        }
        break;
    }
    suffix.is_empty() || suffix.starts_with("//") || suffix.starts_with("/*")
}

fn is_closing_token(byte: u8) -> bool {
    matches!(byte, b'}' | b')' | b']')
}

fn lsp_position_to_byte(text: &str, line: u32, col: u32) -> Option<usize> {
    let line = usize::try_from(line).ok()?;
    let mut line_start = 0usize;
    for _ in 0..line {
        let relative = text.get(line_start..)?.find('\n')?;
        line_start = line_start.saturating_add(relative).saturating_add(1);
    }
    let line_end = text
        .get(line_start..)?
        .find('\n')
        .map_or(text.len(), |relative| line_start.saturating_add(relative));
    let line_text = text.get(line_start..line_end)?;
    let mut utf16_col = 0u32;
    for (relative, ch) in line_text.char_indices() {
        if utf16_col == col {
            return Some(line_start + relative);
        }
        utf16_col = utf16_col.saturating_add(ch.len_utf16() as u32);
        if utf16_col > col {
            return None;
        }
    }
    (utf16_col == col).then_some(line_end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> tree_sitter::Tree {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_dart::LANGUAGE.into())
            .expect("Dart grammar must load");
        parser.parse(text, None).expect("Dart source must parse")
    }

    fn labels(text: &str) -> Vec<String> {
        local_closing_hints(text, &parse(text), 7, ClosingHintSettings::default())
            .into_iter()
            .map(|hint| hint.label.to_string())
            .collect()
    }

    fn relaxed_labels(text: &str) -> Vec<String> {
        let settings = ClosingHintSettings {
            minimum_nesting_depth: 1,
            minimum_block_lines: 2,
            ..ClosingHintSettings::default()
        };
        local_closing_hints(text, &parse(text), 7, settings)
            .into_iter()
            .map(|hint| hint.label.to_string())
            .collect()
    }

    #[test]
    fn dart_import_blocks_cover_contiguous_imports() {
        let text = "import 'a.dart';\nimport 'b.dart';\n\nvoid main() {}\n";
        let blocks = import_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(
            &text[blocks[0].keyword_start..blocks[0].keyword_end],
            "import"
        );
        assert_eq!(
            &text[blocks[0].start..blocks[0].end],
            "import 'a.dart';\nimport 'b.dart';"
        );
    }

    #[test]
    fn dart_import_blocks_keep_blank_lines_between_import_groups_only() {
        let text = "import 'dart:async';\n\nimport 'package:a/a.dart';\n\nvoid main() {}\n";
        let blocks = import_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].line_count, 3);
        assert_eq!(
            &text[blocks[0].start..blocks[0].end],
            "import 'dart:async';\n\nimport 'package:a/a.dart';"
        );
    }

    #[test]
    fn local_labels_cover_named_declarations_and_methods() {
        let text = "class Application {\n  void initialize() {\n    run();\n  }\n}\n";
        let found = labels(text);
        assert!(found.iter().any(|label| label == "initialize"));
        assert!(found.iter().any(|label| label == "class Application"));
    }

    #[test]
    fn local_labels_cover_control_blocks() {
        let text = "void main() {\n  if (ready) {\n    run();\n  }\n  else {\n    stop();\n  }\n  for (final item in items) {\n    use(item);\n  }\n  while (active) {\n    tick();\n  }\n  switch (value) {\n    case 1:\n      break;\n  }\n}\n";
        let found = labels(text);
        for expected in ["if", "else", "for", "while", "switch"] {
            assert!(
                found.iter().any(|label| label == expected),
                "missing {expected}: {found:?}"
            );
        }
    }

    #[test]
    fn local_labels_cover_try_catch_finally() {
        let text = "void main() {\n  try {\n    run();\n  }\n  catch (error) {\n    report(error);\n  }\n  finally {\n    cleanup();\n  }\n}\n";
        let found = labels(text);
        for expected in ["try", "catch", "finally"] {
            assert!(
                found.iter().any(|label| label == expected),
                "missing {expected}: {found:?}"
            );
        }
    }

    #[test]
    fn one_line_and_shallow_blocks_are_filtered() {
        assert!(labels("void main() { if (ready) { run(); } }\n").is_empty());
        let text = "if (ready) {\n  run();\n}\n";
        let settings = ClosingHintSettings {
            minimum_nesting_depth: 3,
            minimum_block_lines: 3,
            ..ClosingHintSettings::default()
        };
        assert!(local_closing_hints(text, &parse(text), 1, settings).is_empty());
    }

    #[test]
    fn short_blocks_and_syntax_errors_are_filtered() {
        let short = "void main() {\n}\n";
        assert!(
            local_closing_hints(short, &parse(short), 1, ClosingHintSettings::default()).is_empty()
        );
        let broken = "void main() {\n  if (ready) {\n    run(\n  }\n}\n";
        assert!(!parse(broken).root_node().has_error() || labels(broken).is_empty());
        assert!(labels(broken).is_empty());
    }

    #[test]
    fn braces_in_strings_and_comments_do_not_create_labels() {
        let text = "void main() {\n  print('}');\n  // }\n}\n";
        assert_eq!(relaxed_labels(text), vec!["main"]);
    }

    #[test]
    fn closing_line_must_start_with_the_closing_brace() {
        let text = "void main() {\n  if (ready) {\n    run(); }\n}\n";
        assert!(!labels(text).iter().any(|label| label == "if"));
    }

    #[test]
    fn server_labels_validate_ranges_and_combine_same_line() {
        let text = "Widget build() {\n  return Column(\n    children: [],\n  );\n}\n";
        let labels = vec![
            crate::lsp::LspClosingLabel {
                label: "Column".into(),
                start_line: 3,
                start_col: 2,
                end_line: 3,
                end_col: 3,
            },
            crate::lsp::LspClosingLabel {
                label: "build".into(),
                start_line: 3,
                start_col: 2,
                end_line: 3,
                end_col: 3,
            },
        ];
        let found = server_closing_hints(text, 9, &labels);
        assert_eq!(found.len(), 1);
        assert_eq!(&*found[0].label, "Column · build");
        assert_eq!(found[0].revision, 9);
    }

    #[test]
    fn empty_or_stale_server_labels_clear_or_are_rejected() {
        let text = "void main() {\n}\n";
        assert!(server_closing_hints(text, 1, &[]).is_empty());
        let stale = crate::lsp::LspClosingLabel {
            label: "main".into(),
            start_line: 0,
            start_col: 0,
            end_line: 0,
            end_col: 1,
        };
        assert!(server_closing_hints(text, 1, &[stale]).is_empty());
    }

    #[test]
    fn empty_server_notification_replaces_the_previous_server_set() {
        let settings = ClosingHintSettings::default();
        let mut state = ClosingHintState::default();
        state.replace_server(
            3,
            vec![ClosingHint {
                revision: 3,
                line: 2,
                anchor_byte: 20,
                label: Arc::<str>::from("main"),
                source: ClosingHintSource::DartServer,
            }],
            settings,
        );
        assert_eq!(state.hints().len(), 1);
        state.replace_server(3, Vec::new(), settings);
        assert!(state.hints().is_empty());
    }

    #[test]
    fn off_mode_ignores_new_server_and_syntax_payloads() {
        let settings = ClosingHintSettings {
            mode: ClosingHintMode::Off,
            ..ClosingHintSettings::default()
        };
        let hint = ClosingHint {
            revision: 4,
            line: 2,
            anchor_byte: 20,
            label: Arc::<str>::from("hidden"),
            source: ClosingHintSource::DartServer,
        };
        let mut state = ClosingHintState::default();
        state.replace_server(4, vec![hint.clone()], settings);
        state.replace_syntax(
            4,
            vec![ClosingHint {
                source: ClosingHintSource::SyntaxTree,
                ..hint
            }],
            settings,
        );
        state.apply_settings(ClosingHintSettings::default());
        assert!(state.hints().is_empty());
    }

    #[test]
    fn server_labels_reject_out_of_document_and_accept_unicode_columns() {
        let text = "void main() {\n  print('😀');\n}\n";
        let out = crate::lsp::LspClosingLabel {
            label: "bad".into(),
            start_line: 99,
            start_col: 0,
            end_line: 99,
            end_col: 1,
        };
        assert!(server_closing_hints(text, 1, &[out]).is_empty());
        let valid = crate::lsp::LspClosingLabel {
            label: "main".into(),
            start_line: 2,
            start_col: 0,
            end_line: 2,
            end_col: 1,
        };
        let found = server_closing_hints(
            text,
            1,
            &[crate::lsp::LspClosingLabel {
                label: "метод 😀".into(),
                ..valid
            }],
        );
        assert_eq!(found.len(), 1);
        assert_eq!(&*found[0].label, "метод 😀");
    }

    #[test]
    fn server_hints_override_local_hints_at_same_anchor() {
        let text = "void main() {\n  run();\n}\n";
        let settings = ClosingHintSettings {
            minimum_nesting_depth: 1,
            minimum_block_lines: 2,
            ..ClosingHintSettings::default()
        };
        let syntax = local_closing_hints(text, &parse(text), 4, settings);
        let server = server_closing_hints(
            text,
            4,
            &[crate::lsp::LspClosingLabel {
                label: "server main".into(),
                start_line: 2,
                start_col: 0,
                end_line: 2,
                end_col: 1,
            }],
        );
        let mut state = ClosingHintState::default();
        state.replace_syntax(4, syntax, settings);
        state.replace_server(4, server, settings);
        assert_eq!(state.hints().len(), 1);
        assert_eq!(&*state.hints()[0].label, "server main");
        assert_eq!(state.hints()[0].source, ClosingHintSource::DartServer);
    }

    #[test]
    fn state_modes_and_revision_invalidation_are_safe() {
        let hint = ClosingHint {
            revision: 1,
            line: 2,
            anchor_byte: 7,
            label: Arc::<str>::from("main"),
            source: ClosingHintSource::SyntaxTree,
        };
        let mut state = ClosingHintState::default();
        state.replace_syntax(1, vec![hint], ClosingHintSettings::default());
        assert_eq!(state.hints().len(), 1);
        state.apply_settings(ClosingHintSettings {
            mode: ClosingHintMode::Off,
            ..ClosingHintSettings::default()
        });
        assert!(state.hints().is_empty());
        state.apply_settings(ClosingHintSettings::default());
        assert!(state.hints().is_empty());
        state.invalidate(2);
        assert_eq!(state.revision(), 2);
        assert!(state.hints().is_empty());
    }

    #[test]
    fn local_labels_cover_mixin_extension_enum_and_do() {
        let text = "mixin CacheMixin {\n  void load() {\n    do {\n      tick();\n    } while (active);\n  }\n}\n\nextension JsonHelpers on String {\n  String encode() {\n    return this;\n  }\n}\n\nenum State {\n  ready,\n  stopped;\n}\n";
        let found = labels(text);
        for expected in [
            "mixin CacheMixin",
            "load",
            "extension JsonHelpers",
            "encode",
            "enum State",
        ] {
            assert!(
                found.iter().any(|label| label == expected),
                "missing {expected}: {found:?}"
            );
        }
        assert!(
            !found.iter().any(|label| label == "do"),
            "a do block with trailing while is not a closing-only line: {found:?}"
        );
    }

    #[test]
    fn minimum_block_lines_is_enforced_independently() {
        let text = "void main() {\n  run();\n}\n";
        let settings = ClosingHintSettings {
            minimum_nesting_depth: 1,
            minimum_block_lines: 4,
            ..ClosingHintSettings::default()
        };
        assert!(local_closing_hints(text, &parse(text), 1, settings).is_empty());
    }

    #[test]
    fn closing_suffix_allows_comments_but_rejects_user_code() {
        let allowed = "void main() {\n  if (ready) {\n    run();\n  } // existing comment\n}\n";
        assert!(labels(allowed).iter().any(|label| label == "if"));

        let rejected = "void main() {\n  if (ready) {\n    run();\n  } print('after');\n}\n";
        assert!(!labels(rejected).iter().any(|label| label == "if"));
    }

    #[test]
    fn server_labels_handle_crlf_zero_length_ranges_and_deduplicate() {
        let text = "void main() {\r\n  run();\r\n}\r\n";
        let label = crate::lsp::LspClosingLabel {
            label: "main".into(),
            start_line: 2,
            start_col: 0,
            end_line: 2,
            end_col: 0,
        };
        let found = server_closing_hints(text, 3, &[label.clone(), label]);
        assert_eq!(found.len(), 1);
        assert_eq!(&*found[0].label, "main");
        assert_eq!(found[0].anchor_byte, text.len() - 2);
    }

    #[test]
    fn server_label_range_may_include_prefix_before_closing_token() {
        let text = "void main() {\n  run();\n}\n";
        let found = server_closing_hints(
            text,
            5,
            &[crate::lsp::LspClosingLabel {
                label: "main".into(),
                start_line: 2,
                start_col: 0,
                end_line: 2,
                end_col: 1,
            }],
        );
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn state_sorts_server_hints_before_applying_server_priority() {
        let settings = ClosingHintSettings::default();
        let mut state = ClosingHintState::default();
        state.replace_server(
            1,
            vec![
                ClosingHint {
                    revision: 1,
                    line: 8,
                    anchor_byte: 80,
                    label: Arc::<str>::from("later"),
                    source: ClosingHintSource::DartServer,
                },
                ClosingHint {
                    revision: 1,
                    line: 2,
                    anchor_byte: 20,
                    label: Arc::<str>::from("server"),
                    source: ClosingHintSource::DartServer,
                },
            ],
            settings,
        );
        state.replace_syntax(
            1,
            vec![ClosingHint {
                revision: 1,
                line: 2,
                anchor_byte: 20,
                label: Arc::<str>::from("syntax"),
                source: ClosingHintSource::SyntaxTree,
            }],
            settings,
        );
        assert_eq!(state.hints().len(), 2);
        assert_eq!(&*state.hints()[0].label, "server");
        assert_eq!(&*state.hints()[1].label, "later");
    }

    #[test]
    fn closing_hint_states_remain_isolated_between_documents() {
        let settings = ClosingHintSettings::default();
        let mut first = ClosingHintState::default();
        let mut second = ClosingHintState::default();
        first.replace_server(
            4,
            vec![ClosingHint {
                revision: 4,
                line: 1,
                anchor_byte: 10,
                label: Arc::<str>::from("first"),
                source: ClosingHintSource::DartServer,
            }],
            settings,
        );
        second.replace_server(
            7,
            vec![ClosingHint {
                revision: 7,
                line: 3,
                anchor_byte: 30,
                label: Arc::<str>::from("second"),
                source: ClosingHintSource::DartServer,
            }],
            settings,
        );
        first.invalidate(5);
        assert!(first.hints().is_empty());
        assert_eq!(second.revision(), 7);
        assert_eq!(&*second.hints()[0].label, "second");
    }

    #[test]
    fn dart_server_only_mode_hides_syntax_hints_without_dropping_server_hints() {
        let syntax = ClosingHint {
            revision: 1,
            line: 2,
            anchor_byte: 10,
            label: Arc::<str>::from("syntax"),
            source: ClosingHintSource::SyntaxTree,
        };
        let server = ClosingHint {
            label: Arc::<str>::from("server"),
            source: ClosingHintSource::DartServer,
            ..syntax.clone()
        };
        let settings = ClosingHintSettings {
            mode: ClosingHintMode::DartServer,
            ..ClosingHintSettings::default()
        };
        let mut state = ClosingHintState::default();
        state.replace_syntax(1, vec![syntax], settings);
        state.replace_server(1, vec![server], settings);
        assert_eq!(state.hints().len(), 1);
        assert_eq!(&*state.hints()[0].label, "server");
    }

    #[test]
    fn dart_lsp_hover_removes_fences_and_highlights_code_blocks() {
        let raw = "```dart\nabstract final class String implements Comparable<String>, Pattern\n```\nDeclared in _dart:core_.\n\n---\nA string example:\n```dart\nconst string = 'Dart is fun';\nprint(string.substring(0, 4));\n```\n## Other resources";
        let (text, spans, kinds, _inline) = crate::lsp::highlight_hover_text(raw);

        assert!(!text.contains("```"));
        assert!(
            text.starts_with("abstract final class String implements Comparable<String>, Pattern")
        );
        assert_eq!(kinds.first(), Some(&crate::lsp::HoverLineKindPublic::Code));
        let const_line = text
            .lines()
            .position(|line| line.starts_with("const "))
            .unwrap();
        assert_eq!(
            kinds.get(const_line),
            Some(&crate::lsp::HoverLineKindPublic::Code)
        );
        let declared_line = text
            .lines()
            .position(|line| line.starts_with("Declared in"))
            .unwrap();
        assert_eq!(
            kinds.get(declared_line),
            Some(&crate::lsp::HoverLineKindPublic::Text)
        );
        assert_eq!(text.lines().last(), Some("Other resources"));
        assert_eq!(
            kinds.last(),
            Some(&crate::lsp::HoverLineKindPublic::Header2)
        );

        for color in [
            crate::highlighter::DRACULA_PINK,
            crate::highlighter::DRACULA_CYAN,
            crate::highlighter::DRACULA_YELLOW,
        ] {
            assert!(
                spans.iter().any(|span| span.color == color),
                "missing Dart hover syntax color {color:?}: {spans:?}"
            );
        }
    }

    fn hover_color_at(
        spans: &[crate::highlighter::ColorSpan],
        byte: usize,
    ) -> [f32; 4] {
        spans
            .iter()
            .find(|span| span.start <= byte && byte < span.end)
            .map_or(crate::highlighter::DRACULA_FG, |span| span.color)
    }

    #[test]
    fn dart_lsp_hover_dedents_common_fenced_indent_and_preserves_relative_indent() {
        let raw = "```dart\n    class RestClient {\n\n      RestClient(Dio dio, {String? baseUrl})\n    }\n```";
        let (text, _spans, kinds, _inline) = crate::lsp::highlight_hover_text(raw);

        assert_eq!(
            text,
            "class RestClient {\n\n  RestClient(Dio dio, {String? baseUrl})\n}"
        );
        assert!(
            kinds
                .iter()
                .all(|kind| *kind == crate::lsp::HoverLineKindPublic::Code)
        );
    }

    #[test]
    fn dart_lsp_hover_does_not_dedent_non_dart_fences() {
        let raw = "```dart\nclass Dio {}\n```\n```json\n    {\"name\": \"dio\"}\n```";
        let (text, _spans, kinds, _inline) = crate::lsp::highlight_hover_text(raw);

        assert!(text.contains("\n    {\"name\": \"dio\"}"));
        assert_eq!(kinds[1], crate::lsp::HoverLineKindPublic::Text);
    }

    #[test]
    fn dart_lsp_hover_drops_only_immediate_redundant_type_metadata() {
        let raw = "```dart\nDio dio\n```\n\nType: `Dio`";
        let (text, spans, kinds, _inline) = crate::lsp::highlight_hover_text(raw);

        assert_eq!(text, "Dio dio");
        assert_eq!(kinds, vec![crate::lsp::HoverLineKindPublic::Code]);
        assert_eq!(hover_color_at(&spans, 0), crate::highlighter::DRACULA_CYAN);
    }

    #[test]
    fn dart_lsp_hover_keeps_non_redundant_type_metadata() {
        let raw = "```dart\nDio dio\n```\nType: `Dio?`";
        let (text, _spans, _kinds, _inline) = crate::lsp::highlight_hover_text(raw);

        assert_eq!(text, "Dio dio\nType: Dio?");
    }

    #[test]
    fn dart_lsp_hover_keeps_unrelated_late_type_metadata() {
        let raw = "```dart\nDio dio\n```\nParameter documentation.\n\nType: `Dio`";
        let (text, _spans, _kinds, _inline) = crate::lsp::highlight_hover_text(raw);

        assert_eq!(text, "Dio dio\nParameter documentation.\n\nType: Dio");
    }

    #[test]
    fn dart_lsp_hover_strips_only_paired_declared_in_emphasis() {
        let raw = "```dart\nclass Dio {}\n```\nDeclared in *package**:dio**/src/dio.dart*.\nDeclared in _dart:core_.";
        let (text, _spans, _kinds, _inline) = crate::lsp::highlight_hover_text(raw);

        assert!(text.contains("Declared in package:dio/src/dio.dart."));
        assert!(text.contains("Declared in dart:core."));
        assert!(!text.contains('*'));
        assert!(!text.contains("_dart:core_"));
    }

    #[test]
    fn dart_lsp_hover_preserves_identifier_underscores_and_unmatched_emphasis() {
        let raw = "```dart\nclass Dio {}\n```\nusers_employees_client.dart\nfoo_bar_\n_privateName\nsome__name\nunmatched * marker\nunmatched _ marker";
        let (text, _spans, _kinds, _inline) = crate::lsp::highlight_hover_text(raw);

        for expected in [
            "users_employees_client.dart",
            "foo_bar_",
            "_privateName",
            "some__name",
            "unmatched * marker",
            "unmatched _ marker",
        ] {
            assert!(text.lines().any(|line| line == expected), "missing {expected:?}: {text:?}");
        }
    }

    #[test]
    fn dart_lsp_hover_inline_type_loses_backticks_and_keeps_type_color() {
        let raw = "```dart\nvar value\n```\nType: `String`";
        let (text, spans, _kinds, inline) = crate::lsp::highlight_hover_text(raw);
        let string_start = text.find("String").expect("normalized type");

        assert_eq!(text, "var value\nType: String");
        assert_eq!(inline, vec![(string_start, string_start + "String".len())]);
        assert_eq!(
            hover_color_at(&spans, string_start),
            crate::highlighter::DRACULA_CYAN
        );
    }

    #[test]
    fn dart_hover_type_fragments_use_type_parse_context() {
        for source in ["String", "Dio", "List<String>"] {
            let mut spans = Vec::new();
            push_hover_highlight_spans(source, 0, &mut spans, HoverParseMode::TypeFragment);

            assert_eq!(
                hover_color_at(&spans, 0),
                crate::highlighter::DRACULA_CYAN,
                "type fragment must keep Dart type semantics: {source:?}"
            );
            if let Some(inner) = source.find("String").filter(|&offset| offset > 0) {
                assert_eq!(
                    hover_color_at(&spans, inner),
                    crate::highlighter::DRACULA_CYAN
                );
            }
        }
    }

    #[test]
    fn dart_hover_statement_fragment_uses_statement_parse_context() {
        let snippet = "dio.options.baseUrl = \"https://pub.dev\";\ndio.options.connectTimeout = const Duration(seconds: 5);\ndio.options.receiveTimeout = const Duration(seconds: 5);";
        assert!(parse(snippet).root_node().has_error());

        let mut spans = Vec::new();
        push_hover_highlight_spans(
            snippet,
            0,
            &mut spans,
            HoverParseMode::StatementFragment,
        );
        let offsets = |token: &str| {
            snippet
                .match_indices(token)
                .map(|(offset, _)| offset)
                .collect::<Vec<_>>()
        };

        for offset in offsets("dio") {
            assert_eq!(hover_color_at(&spans, offset), crate::highlighter::DRACULA_FG);
        }
        for member in ["baseUrl", "connectTimeout", "receiveTimeout"] {
            assert_eq!(
                hover_color_at(&spans, offsets(member)[0]),
                crate::highlighter::DRACULA_FG
            );
        }
        assert_eq!(
            hover_color_at(&spans, offsets("Duration")[0]),
            crate::highlighter::DRACULA_CYAN
        );
        assert_eq!(
            hover_color_at(&spans, offsets("https://pub.dev")[0]),
            crate::highlighter::DRACULA_YELLOW
        );
        assert_eq!(
            hover_color_at(&spans, offsets("const")[0]),
            crate::highlighter::DRACULA_PINK
        );
    }

    #[test]
    fn dart_lsp_hover_options_doc_snippet_uses_consistent_statement_highlighting() {
        let raw = "```dart\nabstract class Dio\n```\nDeclared in *package:dio/src/dio.dart*.\n\n---\nThe [Dio.options] can be updated in anytime:\n```dart\ndio.options.baseUrl = \"https://pub.dev\";\ndio.options.connectTimeout = const Duration(seconds: 5);\ndio.options.receiveTimeout = const Duration(seconds: 5);\n```";
        let (text, spans, _kinds, _inline) = crate::lsp::highlight_hover_text(raw);
        let snippet_start = text
            .find("dio.options.baseUrl")
            .expect("rendered hover must contain the Dio options example");
        let snippet = &text[snippet_start..];

        let token_offsets = |token: &str| {
            snippet
                .match_indices(token)
                .map(|(offset, _)| snippet_start + offset)
                .collect::<Vec<_>>()
        };
        let dio_colors = token_offsets("dio")
            .into_iter()
            .map(|byte| hover_color_at(&spans, byte))
            .collect::<Vec<_>>();
        let member_colors = ["baseUrl", "connectTimeout", "receiveTimeout"]
            .into_iter()
            .map(|token| hover_color_at(&spans, token_offsets(token)[0]))
            .collect::<Vec<_>>();

        assert_eq!(dio_colors.len(), 3);
        assert!(
            dio_colors.windows(2).all(|colors| colors[0] == colors[1]),
            "same-role dio identifiers must share one color: {dio_colors:?}"
        );
        assert!(
            member_colors.windows(2).all(|colors| colors[0] == colors[1]),
            "same-role option members must share one color: {member_colors:?}"
        );
        assert_eq!(
            hover_color_at(&spans, token_offsets("Duration")[0]),
            crate::highlighter::DRACULA_CYAN
        );
        assert_eq!(
            hover_color_at(&spans, token_offsets("https://pub.dev")[0]),
            crate::highlighter::DRACULA_YELLOW
        );
        assert_eq!(
            hover_color_at(&spans, token_offsets("const")[0]),
            crate::highlighter::DRACULA_PINK
        );
    }

    #[test]
    fn dart_lsp_hover_closing_fence_does_not_color_following_prose_as_code() {
        let raw = "```dart\nfinal value = StringBuffer();\n```\nFollowing prose with `inlineCode`.";
        let (text, _spans, kinds, inline) = crate::lsp::highlight_hover_text(raw);

        assert_eq!(
            kinds,
            vec![
                crate::lsp::HoverLineKindPublic::Code,
                crate::lsp::HoverLineKindPublic::Text,
            ]
        );
        assert_eq!(
            text,
            "final value = StringBuffer();\nFollowing prose with inlineCode."
        );
        assert_eq!(&text[inline[0].0..inline[0].1], "inlineCode");
    }
}
