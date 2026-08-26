use std::ops::Range;

use tree_sitter::{InputEdit, Node};
use tree_sitter_md::{MarkdownParser, MarkdownTree};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownDocument {
    pub source_len: usize,
    pub blocks: Vec<MarkdownBlock>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownBlock {
    pub source_range: Range<usize>,
    pub kind: MarkdownBlockKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkdownBlockKind {
    Heading {
        level: u8,
        content_ranges: Vec<Range<usize>>,
        inlines: Vec<MarkdownInlineSpan>,
    },
    Paragraph {
        content_ranges: Vec<Range<usize>>,
        inlines: Vec<MarkdownInlineSpan>,
    },
    BlockQuote {
        depth: usize,
        blocks: Vec<MarkdownBlock>,
    },
    List(MarkdownList),
    Code(MarkdownCodeBlock),
    Table(MarkdownTable),
    ThematicBreak,
    LinkReference(MarkdownLinkReference),
    HtmlRaw,
    MetadataRaw,
    Raw,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkdownListKind {
    Ordered,
    Unordered,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownList {
    pub kind: MarkdownListKind,
    pub depth: usize,
    pub items: Vec<MarkdownListItem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownListItem {
    pub source_range: Range<usize>,
    pub marker_range: Option<Range<usize>>,
    pub ordered_index: Option<u64>,
    pub task_checked: Option<bool>,
    pub blocks: Vec<MarkdownBlock>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownCodeBlock {
    pub source_range: Range<usize>,
    pub content_ranges: Vec<Range<usize>>,
    pub fenced: bool,
    pub info_range: Option<Range<usize>>,
    pub language_range: Option<Range<usize>>,
    pub language: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkdownTableAlignment {
    None,
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownTable {
    pub header: Option<MarkdownTableRow>,
    pub alignments: Vec<MarkdownTableAlignment>,
    pub rows: Vec<MarkdownTableRow>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownTableRow {
    pub source_range: Range<usize>,
    pub cells: Vec<MarkdownTableCell>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownTableCell {
    pub source_range: Range<usize>,
    pub inlines: Vec<MarkdownInlineSpan>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownLinkReference {
    pub label_range: Option<Range<usize>>,
    pub destination_range: Option<Range<usize>>,
    pub title_range: Option<Range<usize>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownInlineSpan {
    pub source_range: Range<usize>,
    pub text_ranges: Vec<Range<usize>>,
    pub style: MarkdownInlineStyle,
    pub children: Vec<MarkdownInlineSpan>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkdownInlineStyle {
    Text,
    Emphasis,
    Strong,
    Code,
    Link {
        destination_range: Option<Range<usize>>,
        reference_range: Option<Range<usize>>,
    },
    Image {
        destination_range: Option<Range<usize>>,
        reference_range: Option<Range<usize>>,
    },
    Uri,
    Escape,
    HardBreak,
    HtmlRaw,
    Raw,
}

/// Stateful Markdown parser for cached document models.
///
/// Callers edit the retained `MarkdownTree` before reparsing changed source. This keeps the
/// block and inline trees incremental and avoids making semantic-model construction a render-frame
/// operation. The highlighter has its own asynchronous tree-sitter pipeline; Read mode can keep
/// one of these states beside its version-bound cache without introducing another parser per frame.
pub struct MarkdownParseState {
    parser: MarkdownParser,
    tree: Option<MarkdownTree>,
}

impl Default for MarkdownParseState {
    fn default() -> Self {
        Self {
            parser: MarkdownParser::default(),
            tree: None,
        }
    }
}

impl MarkdownParseState {
    pub fn apply_edit(&mut self, edit: &InputEdit) {
        if let Some(tree) = self.tree.as_mut() {
            tree.edit(edit);
        }
    }

    pub fn parse(&mut self, source: &str) -> Option<MarkdownDocument> {
        let tree = self.parser.parse(source.as_bytes(), self.tree.as_ref())?;
        let document = MarkdownDocument::from_tree(source, &tree);
        self.tree = Some(tree);
        Some(document)
    }
}

impl MarkdownDocument {
    pub fn from_tree(source: &str, tree: &MarkdownTree) -> Self {
        let mut blocks = Vec::new();
        append_block_children(
            source,
            tree,
            tree.block_tree().root_node(),
            0,
            0,
            &mut blocks,
        );
        Self {
            source_len: source.len(),
            blocks,
        }
    }
}

fn append_block_children(
    source: &str,
    tree: &MarkdownTree,
    parent: Node<'_>,
    quote_depth: usize,
    list_depth: usize,
    blocks: &mut Vec<MarkdownBlock>,
) {
    let mut cursor = parent.walk();
    for child in parent.named_children(&mut cursor) {
        if is_non_renderable_block_syntax(child.kind()) {
            continue;
        }
        if matches!(child.kind(), "document" | "section") {
            append_block_children(source, tree, child, quote_depth, list_depth, blocks);
        } else if let Some(block) = build_block(source, tree, child, quote_depth, list_depth) {
            blocks.push(block);
        }
    }
}

fn build_block(
    source: &str,
    tree: &MarkdownTree,
    node: Node<'_>,
    quote_depth: usize,
    list_depth: usize,
) -> Option<MarkdownBlock> {
    let source_range = node_range(source, node);
    let kind = match node.kind() {
        "atx_heading" => build_atx_heading(source, tree, node),
        "setext_heading" => build_setext_heading(source, tree, node),
        "paragraph" => build_paragraph(source, tree, node),
        "block_quote" => {
            let depth = quote_depth + 1;
            let mut blocks = Vec::new();
            append_block_children(source, tree, node, depth, list_depth, &mut blocks);
            MarkdownBlockKind::BlockQuote { depth, blocks }
        }
        "list" => MarkdownBlockKind::List(build_list(
            source,
            tree,
            node,
            quote_depth,
            list_depth + 1,
        )),
        "fenced_code_block" => {
            MarkdownBlockKind::Code(build_fenced_code_block(source, node))
        }
        "indented_code_block" => MarkdownBlockKind::Code(MarkdownCodeBlock {
            source_range: source_range.clone(),
            content_ranges: indented_code_content_ranges(source, node),
            fenced: false,
            info_range: None,
            language_range: None,
            language: None,
        }),
        "pipe_table" => MarkdownBlockKind::Table(build_table(source, tree, node)),
        "thematic_break" => MarkdownBlockKind::ThematicBreak,
        "link_reference_definition" => {
            MarkdownBlockKind::LinkReference(build_link_reference(source, node))
        }
        "html_block" => MarkdownBlockKind::HtmlRaw,
        "minus_metadata" | "plus_metadata" => MarkdownBlockKind::MetadataRaw,
        "document" | "section" | "block_continuation" | "block_quote_marker" => return None,
        _ => MarkdownBlockKind::Raw,
    };
    Some(MarkdownBlock { source_range, kind })
}

fn build_atx_heading(source: &str, tree: &MarkdownTree, node: Node<'_>) -> MarkdownBlockKind {
    let level = child_kind_prefix_level(node, "atx_h", "_marker").unwrap_or(1);
    let inline = node.child_by_field_name("heading_content");
    let content_ranges = inline
        .map(|inline| visible_source_ranges(source, inline))
        .unwrap_or_default();
    let inlines = inline
        .map(|inline| inline_spans(source, tree, inline))
        .unwrap_or_default();
    MarkdownBlockKind::Heading {
        level,
        content_ranges,
        inlines,
    }
}

fn build_setext_heading(source: &str, tree: &MarkdownTree, node: Node<'_>) -> MarkdownBlockKind {
    let level = if has_named_child_kind(node, "setext_h2_underline") {
        2
    } else {
        1
    };
    let paragraph = node.child_by_field_name("heading_content");
    let inline = paragraph.and_then(|paragraph| first_named_child_kind(paragraph, "inline"));
    let content_ranges = inline
        .or(paragraph)
        .map(|content| visible_source_ranges(source, content))
        .unwrap_or_default();
    let inlines = inline
        .map(|inline| inline_spans(source, tree, inline))
        .unwrap_or_default();
    MarkdownBlockKind::Heading {
        level,
        content_ranges,
        inlines,
    }
}

fn build_paragraph(source: &str, tree: &MarkdownTree, node: Node<'_>) -> MarkdownBlockKind {
    let inline = first_named_child_kind(node, "inline");
    let content_ranges = inline
        .map(|inline| visible_source_ranges(source, inline))
        .unwrap_or_else(|| visible_source_ranges(source, node));
    let inlines = inline
        .map(|inline| inline_spans(source, tree, inline))
        .unwrap_or_default();
    MarkdownBlockKind::Paragraph {
        content_ranges,
        inlines,
    }
}

fn build_list(
    source: &str,
    tree: &MarkdownTree,
    node: Node<'_>,
    quote_depth: usize,
    depth: usize,
) -> MarkdownList {
    let mut items = Vec::new();
    let mut kind = MarkdownListKind::Unordered;
    let mut cursor = node.walk();
    for item in node.named_children(&mut cursor) {
        if item.kind() != "list_item" {
            continue;
        }
        let marker = list_marker(item);
        let ordered_index = marker.and_then(|marker| ordered_list_index(source, marker));
        if ordered_index.is_some() {
            kind = MarkdownListKind::Ordered;
        }
        let task_checked = if has_named_child_kind(item, "task_list_marker_checked") {
            Some(true)
        } else if has_named_child_kind(item, "task_list_marker_unchecked") {
            Some(false)
        } else {
            None
        };
        let mut blocks = Vec::new();
        let mut item_cursor = item.walk();
        for child in item.named_children(&mut item_cursor) {
            if is_list_marker(child.kind())
                || matches!(
                    child.kind(),
                    "task_list_marker_checked" | "task_list_marker_unchecked" | "block_continuation"
                )
            {
                continue;
            }
            if child.kind() == "section" {
                append_block_children(source, tree, child, quote_depth, depth, &mut blocks);
            } else if let Some(block) = build_block(source, tree, child, quote_depth, depth) {
                blocks.push(block);
            }
        }
        items.push(MarkdownListItem {
            source_range: node_range(source, item),
            marker_range: marker.map(|marker| node_range(source, marker)),
            ordered_index,
            task_checked,
            blocks,
        });
    }
    MarkdownList { kind, depth, items }
}

fn build_fenced_code_block(source: &str, node: Node<'_>) -> MarkdownCodeBlock {
    let content = first_named_child_kind(node, "code_fence_content");
    let info = first_named_child_kind(node, "info_string");
    let language_node = info.and_then(|info| first_named_child_kind(info, "language"));
    let language = language_node
        .and_then(|language| source.get(node_range(source, language)))
        .map(str::trim)
        .filter(|language| !language.is_empty())
        .map(str::to_string);
    MarkdownCodeBlock {
        source_range: node_range(source, node),
        content_ranges: content
            .map(|content| visible_source_ranges(source, content))
            .unwrap_or_default(),
        fenced: true,
        info_range: info.map(|info| node_range(source, info)),
        language_range: language_node.map(|language| node_range(source, language)),
        language,
    }
}

fn build_table(source: &str, tree: &MarkdownTree, node: Node<'_>) -> MarkdownTable {
    let mut header = None;
    let mut alignments = Vec::new();
    let mut rows = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "pipe_table_header" => header = Some(build_table_row(source, tree, child)),
            "pipe_table_delimiter_row" => {
                let mut delimiter_cursor = child.walk();
                for cell in child.named_children(&mut delimiter_cursor) {
                    if cell.kind() != "pipe_table_delimiter_cell" {
                        continue;
                    }
                    let left = has_named_child_kind(cell, "pipe_table_align_left");
                    let right = has_named_child_kind(cell, "pipe_table_align_right");
                    alignments.push(match (left, right) {
                        (true, true) => MarkdownTableAlignment::Center,
                        (true, false) => MarkdownTableAlignment::Left,
                        (false, true) => MarkdownTableAlignment::Right,
                        (false, false) => MarkdownTableAlignment::None,
                    });
                }
            }
            "pipe_table_row" => rows.push(build_table_row(source, tree, child)),
            _ => {}
        }
    }
    MarkdownTable {
        header,
        alignments,
        rows,
    }
}

fn build_table_row(source: &str, tree: &MarkdownTree, node: Node<'_>) -> MarkdownTableRow {
    let mut cells = Vec::new();
    let mut cursor = node.walk();
    for cell in node.named_children(&mut cursor) {
        if cell.kind() != "pipe_table_cell" {
            continue;
        }
        cells.push(MarkdownTableCell {
            source_range: node_range(source, cell),
            inlines: inline_spans(source, tree, cell),
        });
    }
    MarkdownTableRow {
        source_range: node_range(source, node),
        cells,
    }
}

fn build_link_reference(source: &str, node: Node<'_>) -> MarkdownLinkReference {
    MarkdownLinkReference {
        label_range: first_named_child_kind(node, "link_label")
            .map(|child| node_range(source, child)),
        destination_range: first_named_child_kind(node, "link_destination")
            .map(|child| node_range(source, child)),
        title_range: first_named_child_kind(node, "link_title")
            .map(|child| node_range(source, child)),
    }
}

fn inline_spans(source: &str, tree: &MarkdownTree, parent: Node<'_>) -> Vec<MarkdownInlineSpan> {
    let visible_ranges = visible_source_ranges(source, parent);
    let Some(inline_tree) = tree.inline_tree(&parent) else {
        return plain_text_spans(source, &visible_ranges);
    };
    parse_inline_children(source, inline_tree.root_node(), &visible_ranges)
}

fn parse_inline_children(
    source: &str,
    parent: Node<'_>,
    visible_ranges: &[Range<usize>],
) -> Vec<MarkdownInlineSpan> {
    let Some(first_visible) = visible_ranges.first() else {
        return Vec::new();
    };
    let Some(last_visible) = visible_ranges.last() else {
        return Vec::new();
    };
    let visible_start = first_visible.start;
    let visible_end = last_visible.end;
    let mut spans = Vec::new();
    let mut cursor_at = visible_start;
    let mut tree_cursor = parent.walk();
    for child in parent.named_children(&mut tree_cursor) {
        let child_range = node_range(source, child);
        if child_range.end <= visible_start || child_range.start >= visible_end {
            continue;
        }
        let start = child_range.start.max(visible_start);
        append_plain_text_spans(
            source,
            visible_ranges,
            safe_range(source, cursor_at, start),
            &mut spans,
        );
        if let Some(span) = build_inline_span(source, child, visible_ranges) {
            cursor_at = cursor_at.max(span.source_range.end.min(visible_end));
            spans.push(span);
        } else {
            cursor_at = cursor_at.max(child_range.end.min(visible_end));
        }
    }
    append_plain_text_spans(
        source,
        visible_ranges,
        safe_range(source, cursor_at, visible_end),
        &mut spans,
    );
    spans
}

fn build_inline_span(
    source: &str,
    node: Node<'_>,
    visible_ranges: &[Range<usize>],
) -> Option<MarkdownInlineSpan> {
    let source_range = node_range(source, node);
    let (style, raw_text_range) = match node.kind() {
        "emphasis" => (
            MarkdownInlineStyle::Emphasis,
            range_inside_delimiters(source, node, "emphasis_delimiter"),
        ),
        "strong_emphasis" => (
            MarkdownInlineStyle::Strong,
            range_inside_delimiters(source, node, "emphasis_delimiter"),
        ),
        "code_span" => (
            MarkdownInlineStyle::Code,
            range_inside_delimiters(source, node, "code_span_delimiter"),
        ),
        "inline_link" | "full_reference_link" | "collapsed_reference_link" | "shortcut_link" => {
            let label = first_named_child_kind(node, "link_text")
                .or_else(|| first_named_child_kind(node, "link_label"));
            let destination = first_named_child_kind(node, "link_destination")
                .map(|child| node_range(source, child));
            let reference = first_named_child_kind(node, "link_label")
                .map(|child| node_range(source, child));
            (
                MarkdownInlineStyle::Link {
                    destination_range: destination,
                    reference_range: reference,
                },
                label
                    .map(|label| node_range(source, label))
                    .unwrap_or_else(|| source_range.clone()),
            )
        }
        "image" => {
            let description = first_named_child_kind(node, "image_description");
            let destination = first_named_child_kind(node, "link_destination")
                .map(|child| node_range(source, child));
            let reference = first_named_child_kind(node, "link_label")
                .map(|child| node_range(source, child));
            (
                MarkdownInlineStyle::Image {
                    destination_range: destination,
                    reference_range: reference,
                },
                description
                    .map(|description| node_range(source, description))
                    .unwrap_or_else(|| source_range.clone()),
            )
        }
        "uri_autolink" | "email_autolink" => (
            MarkdownInlineStyle::Uri,
            strip_ascii_wrappers(source, source_range.clone(), b'<', b'>'),
        ),
        "backslash_escape" => (
            MarkdownInlineStyle::Escape,
            strip_ascii_prefix(source, source_range.clone(), b'\\'),
        ),
        "hard_line_break" => (MarkdownInlineStyle::HardBreak, source_range.clone()),
        "html_tag" => (MarkdownInlineStyle::HtmlRaw, source_range.clone()),
        "emphasis_delimiter" | "code_span_delimiter" | "latex_span_delimiter" => return None,
        "link_text" | "image_description" | "entity_reference" | "numeric_character_reference" => {
            (MarkdownInlineStyle::Text, source_range.clone())
        }
        _ => (MarkdownInlineStyle::Raw, source_range.clone()),
    };
    let text_ranges = intersect_source_ranges(source, visible_ranges, raw_text_range);
    let children = match node.kind() {
        "emphasis" | "strong_emphasis" | "inline_link" | "full_reference_link"
        | "collapsed_reference_link" | "shortcut_link" | "image" | "link_text"
        | "image_description" => parse_inline_children(source, node, &text_ranges),
        _ => Vec::new(),
    };
    Some(MarkdownInlineSpan {
        source_range,
        text_ranges,
        style,
        children,
    })
}

fn plain_text_spans(source: &str, ranges: &[Range<usize>]) -> Vec<MarkdownInlineSpan> {
    ranges
        .iter()
        .filter_map(|range| plain_text_span(source, range.clone()))
        .collect()
}

fn append_plain_text_spans(
    source: &str,
    visible_ranges: &[Range<usize>],
    range: Range<usize>,
    spans: &mut Vec<MarkdownInlineSpan>,
) {
    for visible in intersect_source_ranges(source, visible_ranges, range) {
        if let Some(text) = plain_text_span(source, visible) {
            spans.push(text);
        }
    }
}

fn plain_text_span(source: &str, range: Range<usize>) -> Option<MarkdownInlineSpan> {
    if range.start >= range.end || source.get(range.clone()).is_none() {
        return None;
    }
    Some(MarkdownInlineSpan {
        source_range: range.clone(),
        text_ranges: vec![range],
        style: MarkdownInlineStyle::Text,
        children: Vec::new(),
    })
}

fn visible_source_ranges(source: &str, node: Node<'_>) -> Vec<Range<usize>> {
    let whole = node_range(source, node);
    let mut omitted = Vec::new();
    collect_omitted_source_ranges(source, node, &mut omitted);
    subtract_source_ranges(source, whole, &mut omitted)
}

fn indented_code_content_ranges(source: &str, node: Node<'_>) -> Vec<Range<usize>> {
    let mut ranges = visible_source_ranges(source, node);
    let Some(first) = ranges.first_mut() else {
        return ranges;
    };

    // tree-sitter-md recognizes indented code with its hidden
    // `_indented_chunk_start` external token. Hidden grammar tokens do not
    // survive as Node children, so reconstruct the four-column content boundary
    // only after tree-sitter has already classified this node as indented code.
    // Extra indentation remains source-backed content.
    if let Some(content_start) = indented_chunk_content_start(source, first.start, first.end) {
        first.start = content_start;
    }
    ranges.retain(|range| range.start < range.end);
    ranges
}

fn indented_chunk_content_start(source: &str, start: usize, end: usize) -> Option<usize> {
    let prefix = source.get(..start)?;
    let line_start = prefix.rfind(['\n', '\r']).map_or(0, |index| index + 1);
    let mut column = 0usize;
    for ch in source.get(line_start..start)?.chars() {
        column = scanner_column_after(column, ch);
    }

    let mut indentation = 0usize;
    for (offset, ch) in source.get(start..end)?.char_indices() {
        let width = match ch {
            ' ' => 1,
            '\t' => 4 - column,
            _ => break,
        };
        indentation += width;
        column = scanner_column_after(column, ch);
        if indentation >= 4 {
            return Some(start + offset + ch.len_utf8());
        }
    }
    None
}

fn scanner_column_after(column: usize, ch: char) -> usize {
    if ch == '\t' {
        0
    } else {
        (column + 1) % 4
    }
}

fn collect_omitted_source_ranges(
    source: &str,
    node: Node<'_>,
    omitted: &mut Vec<Range<usize>>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if is_omitted_source_syntax(child.kind()) {
            let range = node_range(source, child);
            if range.start < range.end {
                omitted.push(range);
            }
        } else {
            collect_omitted_source_ranges(source, child, omitted);
        }
    }
}

fn is_omitted_source_syntax(kind: &str) -> bool {
    matches!(
        kind,
        "block_continuation" | "block_quote_marker" | "_indented_chunk_start"
    )
}

fn is_non_renderable_block_syntax(kind: &str) -> bool {
    matches!(kind, "block_continuation" | "block_quote_marker")
}

fn subtract_source_ranges(
    source: &str,
    whole: Range<usize>,
    omitted: &mut Vec<Range<usize>>,
) -> Vec<Range<usize>> {
    omitted.sort_unstable_by_key(|range| (range.start, range.end));
    let mut visible = Vec::new();
    let mut cursor = whole.start;
    for omitted_range in omitted.iter() {
        let start = omitted_range.start.max(whole.start).min(whole.end);
        let end = omitted_range.end.max(start).min(whole.end);
        if end <= cursor {
            continue;
        }
        if cursor < start {
            let range = safe_range(source, cursor, start);
            if range.start < range.end {
                visible.push(range);
            }
        }
        cursor = cursor.max(end);
    }
    if cursor < whole.end {
        let range = safe_range(source, cursor, whole.end);
        if range.start < range.end {
            visible.push(range);
        }
    }
    visible
}

fn intersect_source_ranges(
    source: &str,
    visible_ranges: &[Range<usize>],
    clip: Range<usize>,
) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    for visible in visible_ranges {
        let start = visible.start.max(clip.start);
        let end = visible.end.min(clip.end);
        if start < end {
            ranges.push(safe_range(source, start, end));
        }
    }
    ranges
}

fn range_inside_delimiters(source: &str, node: Node<'_>, delimiter_kind: &str) -> Range<usize> {
    let mut first_end = None;
    let mut last_start = None;
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == delimiter_kind {
            first_end.get_or_insert(child.end_byte());
            last_start = Some(child.start_byte());
        }
    }
    let whole = node_range(source, node);
    let start = first_end.unwrap_or(whole.start).max(whole.start);
    let end = last_start.unwrap_or(whole.end).min(whole.end).max(start);
    safe_range(source, start, end)
}

fn list_marker(node: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| is_list_marker(child.kind()))
}

fn is_list_marker(kind: &str) -> bool {
    matches!(
        kind,
        "list_marker_dot"
            | "list_marker_parenthesis"
            | "list_marker_minus"
            | "list_marker_plus"
            | "list_marker_star"
    )
}

fn ordered_list_index(source: &str, marker: Node<'_>) -> Option<u64> {
    if !matches!(marker.kind(), "list_marker_dot" | "list_marker_parenthesis") {
        return None;
    }
    source
        .get(node_range(source, marker))?
        .bytes()
        .take_while(u8::is_ascii_digit)
        .fold(None, |value, digit| {
            let digit = u64::from(digit - b'0');
            Some(value.unwrap_or(0u64).saturating_mul(10).saturating_add(digit))
        })
}

fn child_kind_prefix_level(node: Node<'_>, prefix: &str, suffix: &str) -> Option<u8> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        let kind = child.kind();
        if let Some(level) = kind
            .strip_prefix(prefix)
            .and_then(|level| level.strip_suffix(suffix))
            .and_then(|level| level.parse::<u8>().ok())
        {
            return Some(level);
        }
    }
    None
}

fn has_named_child_kind(node: Node<'_>, kind: &str) -> bool {
    first_named_child_kind(node, kind).is_some()
}

fn first_named_child_kind<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| child.kind() == kind)
}

fn node_range(source: &str, node: Node<'_>) -> Range<usize> {
    safe_range(source, node.start_byte(), node.end_byte())
}

fn safe_range(source: &str, start: usize, end: usize) -> Range<usize> {
    let mut start = start.min(source.len());
    let mut end = end.min(source.len()).max(start);
    while start > 0 && !source.is_char_boundary(start) {
        start -= 1;
    }
    while end > start && !source.is_char_boundary(end) {
        end -= 1;
    }
    start..end
}

fn strip_ascii_prefix(source: &str, range: Range<usize>, prefix: u8) -> Range<usize> {
    if source.as_bytes().get(range.start) == Some(&prefix) {
        safe_range(source, range.start + 1, range.end)
    } else {
        range
    }
}

fn strip_ascii_wrappers(
    source: &str,
    range: Range<usize>,
    prefix: u8,
    suffix: u8,
) -> Range<usize> {
    if range.end > range.start + 1
        && source.as_bytes().get(range.start) == Some(&prefix)
        && source.as_bytes().get(range.end - 1) == Some(&suffix)
    {
        safe_range(source, range.start + 1, range.end - 1)
    } else {
        range
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Point;

    fn parse(source: &str) -> MarkdownDocument {
        let mut state = MarkdownParseState::default();
        state.parse(source).expect("markdown should parse")
    }

    fn walk_inlines<'a>(
        spans: &'a [MarkdownInlineSpan],
        out: &mut Vec<&'a MarkdownInlineSpan>,
    ) {
        for span in spans {
            out.push(span);
            walk_inlines(&span.children, out);
        }
    }


    fn text_from_ranges(source: &str, ranges: &[Range<usize>]) -> String {
        let mut text = String::new();
        for range in ranges {
            text.push_str(source.get(range.clone()).expect("semantic range must be source-backed"));
        }
        text
    }

    fn visible_inline_text(source: &str, spans: &[MarkdownInlineSpan]) -> String {
        let mut text = String::new();
        for span in spans {
            text.push_str(&text_from_ranges(source, &span.text_ranges));
        }
        text
    }

    fn visible_block_text(source: &str, blocks: &[MarkdownBlock]) -> String {
        let mut text = String::new();
        for block in blocks {
            match &block.kind {
                MarkdownBlockKind::Heading { inlines, .. }
                | MarkdownBlockKind::Paragraph { inlines, .. } => {
                    text.push_str(&visible_inline_text(source, inlines));
                }
                MarkdownBlockKind::BlockQuote { blocks: nested, .. } => {
                    text.push_str(&visible_block_text(source, nested));
                }
                MarkdownBlockKind::List(list) => {
                    for item in &list.items {
                        text.push_str(&visible_block_text(source, &item.blocks));
                    }
                }
                MarkdownBlockKind::Code(code) => {
                    text.push_str(&text_from_ranges(source, &code.content_ranges));
                }
                MarkdownBlockKind::Table(table) => {
                    for row in table.header.iter().chain(&table.rows) {
                        for cell in &row.cells {
                            text.push_str(&visible_inline_text(source, &cell.inlines));
                        }
                    }
                }
                _ => {}
            }
        }
        text
    }

    fn assert_no_raw_blocks(blocks: &[MarkdownBlock]) {
        for block in blocks {
            match &block.kind {
                MarkdownBlockKind::Raw => panic!("known structural syntax became Raw: {block:?}"),
                MarkdownBlockKind::BlockQuote { blocks: nested, .. } => {
                    assert_no_raw_blocks(nested);
                }
                MarkdownBlockKind::List(list) => {
                    for item in &list.items {
                        assert_no_raw_blocks(&item.blocks);
                    }
                }
                _ => {}
            }
        }
    }

    fn collect_quote_depths(blocks: &[MarkdownBlock], depths: &mut Vec<usize>) {
        for block in blocks {
            match &block.kind {
                MarkdownBlockKind::BlockQuote { depth, blocks: nested } => {
                    depths.push(*depth);
                    collect_quote_depths(nested, depths);
                }
                MarkdownBlockKind::List(list) => {
                    for item in &list.items {
                        collect_quote_depths(&item.blocks, depths);
                    }
                }
                _ => {}
            }
        }
    }

    fn collect_code_blocks<'a>(blocks: &'a [MarkdownBlock], out: &mut Vec<&'a MarkdownCodeBlock>) {
        for block in blocks {
            match &block.kind {
                MarkdownBlockKind::Code(code) => out.push(code),
                MarkdownBlockKind::BlockQuote { blocks: nested, .. } => {
                    collect_code_blocks(nested, out);
                }
                MarkdownBlockKind::List(list) => {
                    for item in &list.items {
                        collect_code_blocks(&item.blocks, out);
                    }
                }
                _ => {}
            }
        }
    }

    fn assert_valid_ranges(source: &str, ranges: &[Range<usize>]) {
        for range in ranges {
            assert!(range.start <= range.end);
            assert!(source.get(range.clone()).is_some());
            assert!(source.is_char_boundary(range.start));
            assert!(source.is_char_boundary(range.end));
        }
    }
    fn collect_document_inlines<'a>(document: &'a MarkdownDocument) -> Vec<&'a MarkdownInlineSpan> {
        fn collect_blocks<'a>(blocks: &'a [MarkdownBlock], out: &mut Vec<&'a MarkdownInlineSpan>) {
            for block in blocks {
                match &block.kind {
                    MarkdownBlockKind::Heading { inlines, .. }
                    | MarkdownBlockKind::Paragraph { inlines, .. } => walk_inlines(inlines, out),
                    MarkdownBlockKind::BlockQuote { blocks: nested, .. } => collect_blocks(nested, out),
                    MarkdownBlockKind::List(list) => {
                        for item in &list.items {
                            collect_blocks(&item.blocks, out);
                        }
                    }
                    MarkdownBlockKind::Table(table) => {
                        for row in table.header.iter().chain(&table.rows) {
                            for cell in &row.cells {
                                walk_inlines(&cell.inlines, out);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        let mut out = Vec::new();
        collect_blocks(&document.blocks, &mut out);
        out
    }

    #[test]
    fn semantic_model_covers_markdown_blocks_and_inline_semantics() {
        let source = concat!(
            "# H1\n## H2\n### H3\n\n",
            "Setext\n------\n\n",
            "> quote\n\n",
            "- item\n  - nested\n- [x] done\n1. ordered\n\n",
            "---\n\n",
            "| left | center | right |\n",
            "| :--- | :----: | ----: |\n",
            "| a | b | c |\n\n",
            "Paragraph *em* **strong** `code` [link](https://example.invalid) ",
            "![alt](image.invalid/a.png).\n\n",
            "```rust\nfn main() {}\n```\n",
        );
        let document = parse(source);

        let heading_levels: Vec<u8> = document
            .blocks
            .iter()
            .filter_map(|block| match block.kind {
                MarkdownBlockKind::Heading { level, .. } => Some(level),
                _ => None,
            })
            .collect();
        assert!(heading_levels.starts_with(&[1, 2, 3]));
        assert!(document.blocks.iter().any(|block| matches!(
            &block.kind,
            MarkdownBlockKind::Heading { level: 2, content_ranges, .. }
                if text_from_ranges(source, content_ranges).contains("Setext")
        )));
        assert!(document.blocks.iter().any(|block| matches!(
            block.kind,
            MarkdownBlockKind::BlockQuote { depth: 1, .. }
        )));
        assert!(document.blocks.iter().any(|block| matches!(
            block.kind,
            MarkdownBlockKind::ThematicBreak
        )));
        assert!(document.blocks.iter().any(|block| matches!(
            &block.kind,
            MarkdownBlockKind::Table(table)
                if table.alignments == [
                    MarkdownTableAlignment::Left,
                    MarkdownTableAlignment::Center,
                    MarkdownTableAlignment::Right,
                ]
        )));
        assert!(document.blocks.iter().any(|block| matches!(
            &block.kind,
            MarkdownBlockKind::Code(code)
                if code.fenced && code.language.as_deref() == Some("rust")
        )));
        let indented = parse("    indented code\n");
        assert!(indented.blocks.iter().any(|block| matches!(
            &block.kind,
            MarkdownBlockKind::Code(code) if !code.fenced
        )));
        assert!(document.blocks.iter().any(|block| matches!(
            &block.kind,
            MarkdownBlockKind::List(list)
                if list.items.iter().any(|item| item.task_checked == Some(true))
        )));
        assert!(document.blocks.iter().any(|block| matches!(
            &block.kind,
            MarkdownBlockKind::List(list)
                if list.kind == MarkdownListKind::Ordered
                    && list.items.iter().any(|item| item.ordered_index == Some(1))
        )));
        assert!(document.blocks.iter().any(|block| matches!(
            &block.kind,
            MarkdownBlockKind::List(list)
                if list.items.iter().any(|item| item.blocks.iter().any(|nested| matches!(
                    &nested.kind,
                    MarkdownBlockKind::List(nested_list) if nested_list.depth == 2
                )))
        )));

        let inlines = collect_document_inlines(&document);
        assert!(inlines
            .iter()
            .any(|span| matches!(span.style, MarkdownInlineStyle::Emphasis)));
        assert!(inlines
            .iter()
            .any(|span| matches!(span.style, MarkdownInlineStyle::Strong)));
        assert!(inlines
            .iter()
            .any(|span| matches!(span.style, MarkdownInlineStyle::Code)));
        assert!(inlines.iter().any(|span| matches!(
            &span.style,
            MarkdownInlineStyle::Link { destination_range: Some(range), .. }
                if source.get(range.clone()) == Some("https://example.invalid")
        )));
        assert!(inlines.iter().any(|span| matches!(
            &span.style,
            MarkdownInlineStyle::Image { destination_range: Some(range), .. }
                if source.get(range.clone()) == Some("image.invalid/a.png")
                    && text_from_ranges(source, &span.text_ranges) == "alt"
        )));
    }

    #[test]
    fn semantic_ranges_remain_utf8_boundaries() {
        let source = "# Привет 👋\n\nТекст **жирный 😀** и [ссылка](https://example.invalid/юникод).\n";
        let document = parse(source);
        assert_eq!(document.source_len, source.len());
        for block in &document.blocks {
            assert!(source.is_char_boundary(block.source_range.start));
            assert!(source.is_char_boundary(block.source_range.end));
        }
        for span in collect_document_inlines(&document) {
            assert!(source.is_char_boundary(span.source_range.start));
            assert!(source.is_char_boundary(span.source_range.end));
            assert_valid_ranges(source, &span.text_ranges);
        }
    }

    #[test]
    fn block_quote_markers_are_structural_not_renderable_raw_blocks() {
        let source = "> quote\n";
        let document = parse(source);
        assert!(matches!(
            document.blocks.first().map(|block| &block.kind),
            Some(MarkdownBlockKind::BlockQuote { depth: 1, .. })
        ));
        assert_no_raw_blocks(&document.blocks);
        assert_eq!(visible_block_text(source, &document.blocks).trim_end(), "quote");
    }

    #[test]
    fn block_quote_continuations_stay_out_of_visible_inline_ranges() {
        let source = "> first\n> second\n";
        let document = parse(source);
        assert_no_raw_blocks(&document.blocks);
        let visible = visible_block_text(source, &document.blocks);
        assert!(visible.contains("first"));
        assert!(visible.contains("second"));
        assert!(!visible.contains('>'));
        for span in collect_document_inlines(&document) {
            assert_valid_ranges(source, &span.text_ranges);
            assert!(!text_from_ranges(source, &span.text_ranges).contains("> "));
        }
    }

    #[test]
    fn nested_block_quote_depth_and_visible_text_are_tree_derived() {
        let source = "> outer\n>> inner\n";
        let document = parse(source);
        let mut depths = Vec::new();
        collect_quote_depths(&document.blocks, &mut depths);
        assert!(depths.contains(&1));
        assert!(depths.contains(&2));
        assert_no_raw_blocks(&document.blocks);
        let visible = visible_block_text(source, &document.blocks);
        assert!(visible.contains("outer"));
        assert!(visible.contains("inner"));
        assert!(!visible.contains('>'));
    }

    #[test]
    fn multiline_list_continuation_keeps_marker_metadata_and_clean_text() {
        let source = "- first line\n  second line\n";
        let document = parse(source);
        let list = document
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                MarkdownBlockKind::List(list) => Some(list),
                _ => None,
            })
            .expect("list block");
        let item = list.items.first().expect("list item");
        let marker = item.marker_range.clone().expect("list marker range");
        assert_eq!(source.get(marker), Some("- "));
        let visible = visible_block_text(source, &item.blocks);
        assert!(visible.contains("first line"));
        assert!(visible.contains("second line"));
        assert!(!visible.contains("  second line"));
    }

    #[test]
    fn fenced_code_inside_block_quote_exposes_clean_discontiguous_content() {
        let source = "> ```rust\n> fn main() {}\n> ```\n";
        let document = parse(source);
        let mut codes = Vec::new();
        collect_code_blocks(&document.blocks, &mut codes);
        let code = codes.first().expect("quoted fenced code");
        assert!(code.fenced);
        assert_eq!(code.language.as_deref(), Some("rust"));
        assert_eq!(
            text_from_ranges(source, &code.content_ranges).trim_end(),
            "fn main() {}"
        );
        assert!(!text_from_ranges(source, &code.content_ranges).contains('>'));
        assert_valid_ranges(source, &code.content_ranges);
    }

    #[test]
    fn indented_code_ranges_exclude_structural_indentation_and_continuations() {
        let source = "    alpha\n    beta\n";
        let document = parse(source);
        let mut codes = Vec::new();
        collect_code_blocks(&document.blocks, &mut codes);
        let code = codes.first().expect("indented code");
        assert!(!code.fenced);
        assert_eq!(
            text_from_ranges(source, &code.content_ranges).trim_end(),
            "alpha\nbeta"
        );
        assert_valid_ranges(source, &code.content_ranges);

        let extra_indent = "      alpha\n";
        let document = parse(extra_indent);
        let mut codes = Vec::new();
        collect_code_blocks(&document.blocks, &mut codes);
        let code = codes.first().expect("extra-indented code");
        assert_eq!(
            text_from_ranges(extra_indent, &code.content_ranges).trim_end(),
            "  alpha"
        );

        let nested = ">     quoted alpha\n>     quoted beta\n";
        let document = parse(nested);
        let mut codes = Vec::new();
        collect_code_blocks(&document.blocks, &mut codes);
        let code = codes.first().expect("quoted indented code");
        let visible = text_from_ranges(nested, &code.content_ranges);
        assert_eq!(visible.trim_end(), "quoted alpha\nquoted beta");
        assert!(!visible.contains('>'));
        assert_valid_ranges(nested, &code.content_ranges);
    }

    #[test]
    fn unicode_continuation_ranges_stay_source_backed_and_marker_free() {
        let source = "> Привет 👋\n> мир 😀\n\n- пункт 🚀\n  продолжение 🌍\n";
        let document = parse(source);
        let visible = visible_block_text(source, &document.blocks);
        assert!(visible.contains("Привет 👋"));
        assert!(visible.contains("мир 😀"));
        assert!(visible.contains("пункт 🚀"));
        assert!(visible.contains("продолжение 🌍"));
        assert!(!visible.contains('>'));
        assert!(!visible.contains("  продолжение"));
        for span in collect_document_inlines(&document) {
            assert_valid_ranges(source, &span.text_ranges);
        }
    }

    #[test]
    fn semantic_parser_reuses_edited_markdown_tree() {
        let old = "# Title\n\nplain\n";
        let new = "## Title\n\nplain\n";
        let mut state = MarkdownParseState::default();
        let first = state.parse(old).expect("first parse");
        assert!(matches!(
            first.blocks[0].kind,
            MarkdownBlockKind::Heading { level: 1, .. }
        ));

        state.apply_edit(&InputEdit {
            start_byte: 0,
            old_end_byte: 1,
            new_end_byte: 2,
            start_position: Point::new(0, 0),
            old_end_position: Point::new(0, 1),
            new_end_position: Point::new(0, 2),
        });
        let second = state.parse(new).expect("incremental parse");
        assert!(matches!(
            second.blocks[0].kind,
            MarkdownBlockKind::Heading { level: 2, .. }
        ));
    }

    #[test]
    fn unknown_and_html_blocks_keep_source_fallback_ranges() {
        let source = "<custom-tag>raw</custom-tag>\n\n~~unsupported~~\n";
        let document = parse(source);
        assert!(document.blocks.iter().any(|block| matches!(
            block.kind,
            MarkdownBlockKind::HtmlRaw | MarkdownBlockKind::Paragraph { .. }
        )));
        assert!(document
            .blocks
            .iter()
            .all(|block| source.get(block.source_range.clone()).is_some()));
    }
}
