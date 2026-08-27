use std::ops::Range;

use crate::app::{MarkdownMode, MarkdownTabState};
use crate::highlighter::{ColorSpan, DRACULA_YELLOW};
use crate::languages::markdown::{
    MarkdownBlock, MarkdownBlockKind, MarkdownDocument, MarkdownInlineSpan, MarkdownInlineStyle,
    MarkdownListKind, MarkdownTableAlignment,
};
use crate::renderer::Renderer;
use crate::ui_system::UiRegistry;

const BODY_SCALE: f32 = 0.96;
const INLINE_CODE_PAD_X: f32 = 4.0;
const INLINE_CODE_EXTRA_PAD_Y: f32 = 0.75;
const INLINE_CODE_BG_MIX: f32 = 0.10;
const BODY_LINE_H: f32 = 24.0;
const BLOCK_GAP: f32 = 12.0;
const CONTENT_PAD: f32 = 28.0;
const QUOTE_INDENT: f32 = 18.0;
const LIST_INDENT: f32 = 26.0;
const OVERSCAN: f32 = 96.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LayoutKey {
    version: u64,
    width_bits: u32,
    scale_bits: u32,
    font_size_bits: u32,
}

#[derive(Default)]
pub(crate) struct MarkdownReadLayoutCache {
    key: Option<LayoutKey>,
    blocks: Vec<ReadBlock>,
    content_height: f32,
    rebuild_count: u64,
}

impl MarkdownReadLayoutCache {
    pub(crate) fn invalidate(&mut self) {
        self.key = None;
    }

    fn is_valid_for(&self, key: LayoutKey) -> bool {
        self.key == Some(key)
    }

    pub(crate) fn content_height(&self) -> f32 {
        self.content_height
    }

    #[cfg(test)]
    fn rebuild_count(&self) -> u64 {
        self.rebuild_count
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TextStyle(u8);

impl TextStyle {
    const EMPHASIS: u8 = 1 << 0;
    const STRONG: u8 = 1 << 1;
    const CODE: u8 = 1 << 2;
    const LINK: u8 = 1 << 3;
    const IMAGE: u8 = 1 << 4;
    const RAW: u8 = 1 << 5;

    fn with(mut self, flag: u8) -> Self {
        self.0 |= flag;
        self
    }

    fn contains(self, flag: u8) -> bool {
        self.0 & flag != 0
    }
}

#[derive(Clone, Debug)]
struct StyledRun {
    range: Range<usize>,
    source_range: Option<Range<usize>>,
    style: TextStyle,
}

#[derive(Clone, Debug, Default)]
struct StyledText {
    text: String,
    runs: Vec<StyledRun>,
}

impl StyledText {
    fn push(&mut self, text: &str, style: TextStyle, source_range: Option<Range<usize>>) {
        if text.is_empty() {
            return;
        }
        if let Some(source_range) = source_range.as_ref() {
            debug_assert_eq!(source_range.end.saturating_sub(source_range.start), text.len());
        }
        let start = self.text.len();
        self.text.push_str(text);
        let end = self.text.len();
        if let Some(last) = self.runs.last_mut() {
            let source_contiguous = match (&last.source_range, &source_range) {
                (Some(last_source), Some(source)) => last_source.end == source.start,
                (None, None) => true,
                _ => false,
            };
            if last.style == style && last.range.end == start && source_contiguous {
                last.range.end = end;
                if let (Some(last_source), Some(source)) =
                    (last.source_range.as_mut(), source_range.as_ref())
                {
                    last_source.end = source.end;
                }
                return;
            }
        }
        self.runs.push(StyledRun {
            range: start..end,
            source_range,
            style,
        });
    }
}

#[derive(Clone, Debug)]
struct TextLine {
    range: Range<usize>,
    y: f32,
}

#[derive(Clone, Debug)]
enum ReadPrefix {
    Bullet,
    Ordered(String),
    Task(bool),
}

#[derive(Clone, Debug)]
struct TextBlock {
    styled: StyledText,
    lines: Vec<TextLine>,
    scale: f32,
    x: f32,
    quote_depth: usize,
    prefix: Option<ReadPrefix>,
    heading_level: Option<u8>,
    mono: bool,
    line_height: f32,
}

#[derive(Clone, Debug)]
struct CodeLine {
    source_range: Range<usize>,
    y: f32,
}

#[derive(Clone, Debug)]
struct CodeBlock {
    lines: Vec<CodeLine>,
    content_ranges: Vec<Range<usize>>,
    x: f32,
    quote_depth: usize,
    language: Option<String>,
    line_height: f32,
}

#[derive(Clone, Debug)]
struct TableCell {
    styled: StyledText,
    lines: Vec<Range<usize>>,
    alignment: MarkdownTableAlignment,
}

#[derive(Clone, Debug)]
struct TableRow {
    source_range: Range<usize>,
    cells: Vec<TableCell>,
    y: f32,
    h: f32,
    header: bool,
}

#[derive(Clone, Debug)]
struct TableBlock {
    rows: Vec<TableRow>,
    x: f32,
    width: f32,
    cell_width: f32,
    cell_padding: f32,
    line_height: f32,
    quote_depth: usize,
}

#[derive(Clone, Debug)]
enum ReadBlockKind {
    Text(TextBlock),
    Code(CodeBlock),
    Table(TableBlock),
    Rule { x: f32, width: f32, quote_depth: usize },
}

#[derive(Clone, Debug)]
struct ReadBlock {
    source_range: Range<usize>,
    top: f32,
    bottom: f32,
    kind: ReadBlockKind,
}

struct LayoutBuilder<'a, F: FnMut(char, bool) -> f32> {
    source: &'a str,
    width: f32,
    scale: f32,
    y: f32,
    blocks: Vec<ReadBlock>,
    advance: F,
}

impl<'a, F: FnMut(char, bool) -> f32> LayoutBuilder<'a, F> {
    fn new(source: &'a str, width: f32, scale: f32, advance: F) -> Self {
        Self {
            source,
            width: width.max(1.0),
            scale,
            y: (18.0 * scale).round(),
            blocks: Vec::new(),
            advance,
        }
    }

    fn finish(mut self) -> (Vec<ReadBlock>, f32) {
        self.y += (20.0 * self.scale).round();
        (self.blocks, self.y.max(0.0))
    }

    fn append_blocks(
        &mut self,
        blocks: &[MarkdownBlock],
        indent: f32,
        quote_depth: usize,
        mut prefix: Option<ReadPrefix>,
    ) {
        for block in blocks {
            let use_prefix = prefix.take();
            self.append_block(block, indent, quote_depth, use_prefix);
        }
    }

    fn append_block(
        &mut self,
        block: &MarkdownBlock,
        indent: f32,
        quote_depth: usize,
        prefix: Option<ReadPrefix>,
    ) {
        match &block.kind {
            MarkdownBlockKind::Heading { level, content_ranges, inlines } => {
                let styled = styled_from_inlines(self.source, inlines, content_ranges);
                let scale = heading_scale(*level);
                self.append_text(
                    styled,
                    scale,
                    indent,
                    quote_depth,
                    prefix,
                    Some(*level),
                    false,
                    block.source_range.clone(),
                );
            }
            MarkdownBlockKind::Paragraph { content_ranges, inlines } => {
                let styled = styled_from_inlines(self.source, inlines, content_ranges);
                self.append_text(
                    styled,
                    BODY_SCALE,
                    indent,
                    quote_depth,
                    prefix,
                    None,
                    false,
                    block.source_range.clone(),
                );
            }
            MarkdownBlockKind::BlockQuote { depth, blocks } => {
                let depth = (*depth).max(quote_depth + 1);
                self.append_blocks(
                    blocks,
                    indent + QUOTE_INDENT * self.scale,
                    depth,
                    prefix,
                );
            }
            MarkdownBlockKind::List(list) => {
                for item in &list.items {
                    let item_prefix = if let Some(checked) = item.task_checked {
                        ReadPrefix::Task(checked)
                    } else if list.kind == MarkdownListKind::Ordered {
                        ordered_prefix(item.ordered_index.unwrap_or(1))
                    } else {
                        ReadPrefix::Bullet
                    };
                    let before = self.blocks.len();
                    self.append_blocks(
                        &item.blocks,
                        indent + LIST_INDENT * self.scale,
                        quote_depth,
                        Some(item_prefix.clone()),
                    );
                    if self.blocks.len() == before {
                        self.append_text(
                            StyledText::default(),
                            BODY_SCALE,
                            indent + LIST_INDENT * self.scale,
                            quote_depth,
                            Some(item_prefix),
                            None,
                            false,
                            item.source_range.clone(),
                        );
                    }
                }
            }
            MarkdownBlockKind::Code(code) => {
                self.append_code(
                    &code.content_ranges,
                    code.language.clone(),
                    indent,
                    quote_depth,
                    prefix,
                    block.source_range.clone(),
                );
            }
            MarkdownBlockKind::Table(table) => {
                self.append_table(table, indent, quote_depth, prefix, block.source_range.clone());
            }
            MarkdownBlockKind::ThematicBreak => {
                if prefix.is_some() {
                    self.append_text(
                        StyledText::default(),
                        BODY_SCALE,
                        indent,
                        quote_depth,
                        prefix,
                        None,
                        false,
                        block.source_range.clone(),
                    );
                }
                let top = self.y + (5.0 * self.scale).round();
                let h = (1.0 * self.scale).round().max(1.0);
                let x = indent + CONTENT_PAD * self.scale;
                let width = (self.width - x - CONTENT_PAD * self.scale).max(1.0);
                self.blocks.push(ReadBlock {
                    source_range: block.source_range.clone(),
                    top,
                    bottom: top + h,
                    kind: ReadBlockKind::Rule { x, width, quote_depth },
                });
                self.y = top + h + (BLOCK_GAP * self.scale).round();
            }
            MarkdownBlockKind::LinkReference(reference) => {
                let mut styled = StyledText::default();
                styled.push(
                    "Reference: ",
                    TextStyle::default().with(TextStyle::EMPHASIS),
                    None,
                );
                if let Some(range) = reference.label_range.as_ref() {
                    push_source_range(&mut styled, self.source, range, TextStyle::default().with(TextStyle::LINK));
                }
                if let Some(range) = reference.destination_range.as_ref() {
                    styled.push("  ", TextStyle::default(), None);
                    push_source_range(&mut styled, self.source, range, TextStyle::default().with(TextStyle::LINK));
                }
                self.append_text(
                    styled,
                    0.82,
                    indent,
                    quote_depth,
                    prefix,
                    None,
                    false,
                    block.source_range.clone(),
                );
            }
            MarkdownBlockKind::HtmlRaw | MarkdownBlockKind::MetadataRaw | MarkdownBlockKind::Raw => {
                let mut styled = StyledText::default();
                push_source_range(
                    &mut styled,
                    self.source,
                    &block.source_range,
                    TextStyle::default().with(TextStyle::RAW),
                );
                self.append_text(
                    styled,
                    0.82,
                    indent,
                    quote_depth,
                    prefix,
                    None,
                    true,
                    block.source_range.clone(),
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn append_text(
        &mut self,
        styled: StyledText,
        text_scale: f32,
        indent: f32,
        quote_depth: usize,
        prefix: Option<ReadPrefix>,
        heading_level: Option<u8>,
        mono: bool,
        source_range: Range<usize>,
    ) {
        let prefix_w = prefix_width(prefix.as_ref(), self.scale);
        let x = (CONTENT_PAD * self.scale + indent + prefix_w).round();
        let right_pad = (CONTENT_PAD * self.scale).round();
        let max_w = (self.width - x - right_pad).max(20.0 * self.scale);
        let line_h = ((if heading_level.is_some() { 28.0 } else { BODY_LINE_H })
            * self.scale
            * text_scale.max(0.75))
            .round()
            .max(1.0);
        let mut advance = |offset: usize, ch: char| {
            styled_char_advance(
                &styled,
                offset,
                ch,
                text_scale,
                self.scale,
                mono,
                &mut self.advance,
            )
        };
        let ranges = crate::render_view::core_text::wrapped_text_ranges_with_offsets(
            &styled.text,
            max_w,
            &mut advance,
        );
        let top_margin = if let Some(level) = heading_level {
            ((if level <= 2 { 18.0 } else { 10.0 }) * self.scale).round()
        } else {
            0.0
        };
        self.y += top_margin;
        let top = self.y;
        let mut lines = Vec::with_capacity(ranges.len());
        for (idx, (start, end)) in ranges.into_iter().enumerate() {
            lines.push(TextLine {
                range: start..end,
                y: (self.y + line_h * (idx as f32 + 0.82)).round(),
            });
        }
        let line_count = lines.len().max(1) as f32;
        let bottom = (self.y + line_count * line_h).round();
        self.blocks.push(ReadBlock {
            source_range,
            top,
            bottom,
            kind: ReadBlockKind::Text(TextBlock {
                styled,
                lines,
                scale: text_scale,
                x,
                quote_depth,
                prefix,
                heading_level,
                mono,
                line_height: line_h,
            }),
        });
        self.y = bottom + (BLOCK_GAP * self.scale).round();
    }

    fn append_code(
        &mut self,
        ranges: &[Range<usize>],
        language: Option<String>,
        indent: f32,
        quote_depth: usize,
        prefix: Option<ReadPrefix>,
        source_range: Range<usize>,
    ) {
        if prefix.is_some() {
            self.append_text(
                StyledText::default(),
                BODY_SCALE,
                indent,
                quote_depth,
                prefix,
                None,
                false,
                source_range.clone(),
            );
        }
        let pad = code_block_padding(self.scale);
        let header_h = code_header_height(self.scale);
        let x = (CONTENT_PAD * self.scale + indent).round();
        let line_h = (BODY_LINE_H * self.scale).round().max(1.0);
        let top = self.y;
        let mut y = top + pad + header_h;
        let mut lines = Vec::new();
        for range in ranges {
            let Some(text) = self.source.get(range.clone()) else { continue };
            let mut local = 0usize;
            for part in text.split_inclusive('\n') {
                let visible = part.trim_end_matches(['\r', '\n']);
                let start = range.start + local;
                let end = start + visible.len();
                lines.push(CodeLine { source_range: start..end, y: (y + line_h * 0.82).round() });
                y += line_h;
                local += part.len();
            }
            if text.is_empty() {
                lines.push(CodeLine { source_range: range.start..range.start, y: (y + line_h * 0.82).round() });
                y += line_h;
            }
        }
        if lines.is_empty() {
            lines.push(CodeLine { source_range: 0..0, y: (y + line_h * 0.82).round() });
            y += line_h;
        }
        let bottom = (y + pad).round();
        self.blocks.push(ReadBlock {
            source_range,
            top,
            bottom,
            kind: ReadBlockKind::Code(CodeBlock {
                lines,
                content_ranges: ranges.to_vec(),
                x,
                quote_depth,
                language,
                line_height: line_h,
            }),
        });
        self.y = bottom + (BLOCK_GAP * self.scale).round();
    }

    fn append_table(
        &mut self,
        table: &crate::languages::markdown::MarkdownTable,
        indent: f32,
        quote_depth: usize,
        prefix: Option<ReadPrefix>,
        source_range: Range<usize>,
    ) {
        if prefix.is_some() {
            self.append_text(
                StyledText::default(),
                BODY_SCALE,
                indent,
                quote_depth,
                prefix,
                None,
                false,
                source_range.clone(),
            );
        }
        let x = (CONTENT_PAD * self.scale + indent).round();
        let width = (self.width - x - CONTENT_PAD * self.scale).max(40.0 * self.scale).round();
        let col_count = table
            .header
            .iter()
            .chain(table.rows.iter())
            .map(|row| row.cells.len())
            .max()
            .unwrap_or(1)
            .max(1);
        let cell_w = width / col_count as f32;
        let pad = (8.0 * self.scale).round();
        let line_h = (22.0 * self.scale).round().max(1.0);
        let mut rows = Vec::new();
        let mut row_y = self.y;
        for (is_header, row) in table
            .header
            .iter()
            .map(|row| (true, row))
            .chain(table.rows.iter().map(|row| (false, row)))
        {
            let mut cells = Vec::with_capacity(col_count);
            let mut max_lines = 1usize;
            for col in 0..col_count {
                let styled = row.cells.get(col).map_or_else(StyledText::default, |cell| {
                    styled_from_inlines(self.source, &cell.inlines, std::slice::from_ref(&cell.source_range))
                });
                let max_text_w = (cell_w - pad * 2.0).max(8.0);
                let mut advance = |offset: usize, ch: char| {
                    styled_char_advance(
                        &styled,
                        offset,
                        ch,
                        0.82,
                        self.scale,
                        false,
                        &mut self.advance,
                    )
                };
                let lines = crate::render_view::core_text::wrapped_text_ranges_with_offsets(
                    &styled.text,
                    max_text_w,
                    &mut advance,
                )
                    .into_iter()
                    .map(|(start, end)| start..end)
                    .collect::<Vec<_>>();
                max_lines = max_lines.max(lines.len());
                cells.push(TableCell {
                    styled,
                    lines,
                    alignment: table.alignments.get(col).copied().unwrap_or(MarkdownTableAlignment::None),
                });
            }
            let h = (pad * 2.0 + max_lines as f32 * line_h).round();
            rows.push(TableRow {
                source_range: row.source_range.clone(),
                cells,
                y: row_y,
                h,
                header: is_header,
            });
            row_y += h;
        }
        let top = self.y;
        let bottom = row_y.max(top + line_h + pad * 2.0);
        self.blocks.push(ReadBlock {
            source_range,
            top,
            bottom,
            kind: ReadBlockKind::Table(TableBlock {
                rows,
                x,
                width,
                cell_width: cell_w,
                cell_padding: pad,
                line_height: line_h,
                quote_depth,
            }),
        });
        self.y = bottom + (BLOCK_GAP * self.scale).round();
    }
}

fn heading_scale(level: u8) -> f32 {
    match level {
        1 => 1.60,
        2 => 1.42,
        3 => 1.27,
        4 => 1.16,
        5 => 1.07,
        _ => 1.00,
    }
}

fn inline_code_padding_x(scale: f32) -> f32 {
    (INLINE_CODE_PAD_X * scale).round().max(1.0)
}

fn inline_code_vertical_bounds(baseline_y: f32, scale_factor: f32, text_scale: f32) -> (f32, f32) {
    let extra_pad = (INLINE_CODE_EXTRA_PAD_Y * scale_factor).round().max(1.0);
    let top = baseline_y.round() - (17.0 * scale_factor * text_scale).round() - extra_pad;
    let height = (20.0 * scale_factor * text_scale).round().max(1.0) + extra_pad * 2.0;
    (top.round(), height.round())
}

fn inline_code_background(bg: [f32; 4], fg: [f32; 4]) -> [f32; 4] {
    [
        bg[0] + (fg[0] - bg[0]) * INLINE_CODE_BG_MIX,
        bg[1] + (fg[1] - bg[1]) * INLINE_CODE_BG_MIX,
        bg[2] + (fg[2] - bg[2]) * INLINE_CODE_BG_MIX,
        bg[3],
    ]
}

fn markdown_text_color(style: TextStyle, theme_fg: [f32; 4]) -> [f32; 4] {
    if style.contains(TextStyle::CODE) {
        DRACULA_YELLOW
    } else if style.contains(TextStyle::LINK) {
        [0.47, 0.68, 0.96, 1.0]
    } else if style.contains(TextStyle::STRONG) {
        [0.95, 0.93, 0.98, 1.0]
    } else if style.contains(TextStyle::EMPHASIS) {
        [0.78, 0.75, 0.87, 1.0]
    } else if style.contains(TextStyle::IMAGE) {
        [0.73, 0.70, 0.86, 1.0]
    } else if style.contains(TextStyle::RAW) {
        faded(theme_fg, 0.80)
    } else {
        theme_fg
    }
}

fn styled_char_advance<F: FnMut(char, bool) -> f32>(
    styled: &StyledText,
    offset: usize,
    ch: char,
    text_scale: f32,
    layout_scale: f32,
    mono: bool,
    advance: &mut F,
) -> f32 {
    let run_idx = styled.runs.partition_point(|run| run.range.end <= offset);
    let run = styled
        .runs
        .get(run_idx)
        .filter(|run| run.range.start <= offset && offset < run.range.end);
    let inline_mono = run.is_some_and(|run| run.style.contains(TextStyle::CODE));
    let mut width = (advance(ch, mono || inline_mono) * text_scale)
        .round()
        .max(1.0);
    if inline_mono
        && let Some(run) = run
    {
        let pad = inline_code_padding_x(layout_scale);
        if offset == run.range.start {
            width += pad;
        }
        if offset.saturating_add(ch.len_utf8()) >= run.range.end {
            width += pad;
        }
    }
    width
}

fn ordered_prefix(index: u64) -> ReadPrefix {
    let mut label = index.to_string();
    label.push('.');
    ReadPrefix::Ordered(label)
}

fn prefix_width(prefix: Option<&ReadPrefix>, scale: f32) -> f32 {
    match prefix {
        Some(ReadPrefix::Ordered(index)) => (18.0 + index.len() as f32 * 7.0) * scale,
        Some(_) => 24.0 * scale,
        None => 0.0,
    }
}

fn push_source_range(styled: &mut StyledText, source: &str, range: &Range<usize>, style: TextStyle) {
    if let Some(text) = source.get(range.clone()) {
        styled.push(text, style, Some(range.clone()));
    }
}

fn styled_from_inlines(
    source: &str,
    inlines: &[MarkdownInlineSpan],
    fallback_ranges: &[Range<usize>],
) -> StyledText {
    let mut styled = StyledText::default();
    if inlines.is_empty() {
        for range in fallback_ranges {
            push_source_range(&mut styled, source, range, TextStyle::default());
        }
        return styled;
    }
    for span in inlines {
        append_inline(&mut styled, source, span, TextStyle::default());
    }
    styled
}

fn append_inline(styled: &mut StyledText, source: &str, span: &MarkdownInlineSpan, inherited: TextStyle) {
    let mut style = inherited;
    match &span.style {
        MarkdownInlineStyle::Emphasis => style = style.with(TextStyle::EMPHASIS),
        MarkdownInlineStyle::Strong => style = style.with(TextStyle::STRONG),
        MarkdownInlineStyle::Code => style = style.with(TextStyle::CODE),
        MarkdownInlineStyle::Link { .. } | MarkdownInlineStyle::Uri => style = style.with(TextStyle::LINK),
        MarkdownInlineStyle::Image { .. } => {
            style = style.with(TextStyle::IMAGE);
            styled.push("Image: ", style, None);
        }
        MarkdownInlineStyle::HtmlRaw | MarkdownInlineStyle::Raw => style = style.with(TextStyle::RAW),
        MarkdownInlineStyle::HardBreak => {
            let source_range = source
                .get(span.source_range.clone())
                .and_then(|text| text.rfind('\n'))
                .map(|offset| {
                    let start = span.source_range.start + offset;
                    start..start + 1
                });
            styled.push("\n", style, source_range);
            return;
        }
        MarkdownInlineStyle::Text | MarkdownInlineStyle::Escape => {}
    }
    if !span.children.is_empty() {
        for child in &span.children {
            append_inline(styled, source, child, style);
        }
    } else {
        for range in &span.text_ranges {
            push_source_range(styled, source, range, style);
        }
    }
}

fn visible_block_range(blocks: &[ReadBlock], top: f32, bottom: f32) -> Range<usize> {
    let start = blocks.partition_point(|block| block.bottom < top);
    let end = blocks.partition_point(|block| block.top <= bottom);
    start.min(end)..end
}

fn visible_baseline_range<T>(
    items: &[T],
    top: f32,
    bottom: f32,
    baseline: impl Fn(&T) -> f32,
) -> Range<usize> {
    let start = items
        .partition_point(|item| baseline(item) < top)
        .saturating_sub(1);
    let end = items
        .partition_point(|item| baseline(item) <= bottom)
        .saturating_add(1)
        .min(items.len());
    start.min(end)..end
}

fn visible_text_line_range(lines: &[TextLine], top: f32, bottom: f32) -> Range<usize> {
    visible_baseline_range(lines, top, bottom, |line| line.y)
}

fn visible_code_line_range(lines: &[CodeLine], top: f32, bottom: f32) -> Range<usize> {
    visible_baseline_range(lines, top, bottom, |line| line.y)
}

fn visible_table_row_range(rows: &[TableRow], top: f32, bottom: f32) -> Range<usize> {
    let start = rows.partition_point(|row| row.y + row.h < top);
    let end = rows.partition_point(|row| row.y <= bottom);
    start.min(end)..end
}

fn visible_table_cell_line_range(
    line_count: usize,
    row_y: f32,
    cell_padding: f32,
    line_height: f32,
    visible_top: f32,
    visible_bottom: f32,
) -> Range<usize> {
    if line_count == 0 || !line_height.is_finite() || line_height <= 0.0 {
        return 0..0;
    }

    let content_top = row_y + cell_padding;
    let content_bottom = content_top + line_count as f32 * line_height;
    if visible_bottom < content_top {
        return 0..0;
    }
    if visible_top > content_bottom {
        return line_count..line_count;
    }

    let start = (((visible_top - content_top) / line_height).floor().max(0.0) as usize)
        .saturating_sub(1)
        .min(line_count);
    let end = (((visible_bottom - content_top) / line_height).ceil().max(0.0) as usize)
        .saturating_add(1)
        .min(line_count);
    start.min(end)..end
}

fn visible_styled_run_range(runs: &[StyledRun], text_range: &Range<usize>) -> Range<usize> {
    let start = runs.partition_point(|run| run.range.end <= text_range.start);
    let end = runs.partition_point(|run| run.range.start < text_range.end);
    start.min(end)..end
}

fn faded(color: [f32; 4], alpha: f32) -> [f32; 4] {
    [color[0], color[1], color[2], alpha]
}

impl Renderer {
    pub(crate) fn draw_markdown_read(
        &mut self,
        markdown: &mut MarkdownTabState,
        editor_version: u64,
        spans: &[ColorSpan],
        search_results: &[(usize, usize)],
        search_current_idx: Option<usize>,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        ui_registry: &mut UiRegistry,
    ) {
        self.push_rect(x, y, w, h, self.theme.bg);
        ui_registry.register_blocker(
            crate::ui_system::UiId::MarkdownReadBody,
            x,
            y,
            w,
            h,
            self.last_mouse_x,
            self.last_mouse_y,
        );

        let Some(document) = markdown.read_document(editor_version) else {
            self.draw_string_scaled_pixel_snapped(
                "Markdown preview is preparing…",
                x + 28.0 * self.scale_factor,
                y + 42.0 * self.scale_factor,
                self.theme.line_num,
                0.88,
            );
            markdown.read_max_scroll = 0.0;
            markdown.read_scroll_y.clamp_target(0.0, 0.0);
            markdown.read_scroll_y.clamp_current(0.0, 0.0);
            return;
        };

        let content_w = w.max(1.0);
        let key = LayoutKey {
            version: editor_version,
            width_bits: content_w.round().to_bits(),
            scale_bits: self.scale_factor.to_bits(),
            font_size_bits: self.font_size.to_bits(),
        };
        if !markdown.read_layout.is_valid_for(key) {
            let source = markdown.read_source.as_str();
            let scale = self.scale_factor;
            let mut advance = |ch: char, mono: bool| {
                if mono {
                    self.char_advance(ch)
                } else {
                    self.get_ui_glyph(ch)
                        .map(|glyph| glyph.advance)
                        .unwrap_or(10.0 * scale)
                }
            };
            let builder = LayoutBuilder::new(source, content_w, scale, &mut advance);
            let mut builder = builder;
            builder.append_blocks(&document.blocks, 0.0, 0, None);
            let (blocks, content_height) = builder.finish();
            markdown.read_layout.blocks = blocks;
            markdown.read_layout.content_height = content_height;
            markdown.read_layout.key = Some(key);
            markdown.read_layout.rebuild_count = markdown.read_layout.rebuild_count.saturating_add(1);
        }

        let max_scroll = (markdown.read_layout.content_height() - h).max(0.0);
        markdown.read_max_scroll = max_scroll;
        markdown.read_scroll_y.clamp_target(0.0, max_scroll);
        markdown.read_scroll_y.clamp_current(0.0, max_scroll);
        let scroll_y = markdown.read_scroll_y.current.round();
        let visible = visible_block_range(
            &markdown.read_layout.blocks,
            (scroll_y - OVERSCAN * self.scale_factor).max(0.0),
            scroll_y + h + OVERSCAN * self.scale_factor,
        );

        self.flush();
        unsafe {
            use glow::HasContext;
            self.gl.enable(glow::SCISSOR_TEST);
            self.gl.scissor(
                x.round() as i32,
                (self.height - (y + h)).round() as i32,
                w.round().max(0.0) as i32,
                h.round().max(0.0) as i32,
            );
        }
        let visible_top = (scroll_y - OVERSCAN * self.scale_factor).max(0.0);
        let visible_bottom = scroll_y + h + OVERSCAN * self.scale_factor;
        let hovered_code_block = markdown_read_code_block_at_if_hover_valid(
            markdown.code_copy_hover_valid,
            &markdown.read_layout,
            (x, y, w, h),
            scroll_y,
            self.scale_factor,
            self.last_mouse_x,
            self.last_mouse_y,
        );
        let copied_code_block = markdown.copied_code_block;
        let selection = markdown.read_selection_range();
        let highlights = ReadHighlights {
            selection: selection.as_ref(),
            search_results,
            search_current_idx,
        };
        ui_registry.push_clip(crate::ui_system::UiClipRect::new(x, y, w, h));
        for idx in visible {
            let block = &markdown.read_layout.blocks[idx];
            self.draw_markdown_block(
                block,
                markdown.read_source.as_str(),
                spans,
                x,
                y,
                scroll_y,
                content_w,
                visible_top,
                visible_bottom,
                highlights,
            );
            if hovered_code_block == Some(block.source_range.start) {
                self.draw_markdown_code_copy_action(
                    block,
                    x,
                    y - scroll_y,
                    content_w,
                    copied_code_block == Some(block.source_range.start),
                    ui_registry,
                );
            }
        }
        ui_registry.pop_clip();
        self.flush();
        unsafe {
            use glow::HasContext;
            self.gl.disable(glow::SCISSOR_TEST);
        }

        if max_scroll > 0.0 {
            let bar_w = (9.0 * self.scale_factor).round().max(4.0);
            let track_x = (x + w - bar_w).round();
            let thumb_h = (h / markdown.read_layout.content_height.max(h) * h)
                .max(20.0 * self.scale_factor)
                .min(h);
            let ratio = (scroll_y / max_scroll).clamp(0.0, 1.0);
            let thumb_y = y + ratio * (h - thumb_h);
            self.push_rounded_rect(
                track_x + self.scale_factor,
                thumb_y,
                (bar_w - 2.0 * self.scale_factor).max(2.0),
                thumb_h,
                (bar_w * 0.4).max(2.0),
                faded(self.theme.fg, 0.45),
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_markdown_block(
        &mut self,
        block: &ReadBlock,
        source: &str,
        spans: &[ColorSpan],
        frame_x: f32,
        frame_y: f32,
        scroll_y: f32,
        content_w: f32,
        visible_top: f32,
        visible_bottom: f32,
        highlights: ReadHighlights<'_>,
    ) {
        let offset_y = frame_y - scroll_y;
        match &block.kind {
            ReadBlockKind::Text(text) => {
                self.draw_quote_guides(text.quote_depth, frame_x, block.top + offset_y, block.bottom + offset_y);
                if let Some(prefix) = text.prefix.as_ref() {
                    self.draw_markdown_prefix(prefix, frame_x + text.x - prefix_width(Some(prefix), self.scale_factor), text.lines.first().map_or(block.top + offset_y, |line| line.y + offset_y));
                }
                if text.heading_level.is_some_and(|level| level <= 2) {
                    self.push_rect(
                        frame_x + text.x,
                        block.bottom + offset_y + 2.0 * self.scale_factor,
                        (content_w - text.x - CONTENT_PAD * self.scale_factor).max(1.0),
                        1.0,
                        faded(self.theme.fg, 0.10),
                    );
                }
                for idx in visible_text_line_range(&text.lines, visible_top, visible_bottom) {
                    let line = &text.lines[idx];
                    self.draw_styled_text_line(
                        text,
                        &line.range,
                        frame_x + text.x,
                        line.y + offset_y,
                        highlights,
                    );
                }
            }
            ReadBlockKind::Code(code) => {
                self.draw_quote_guides(code.quote_depth, frame_x, block.top + offset_y, block.bottom + offset_y);
                let pad = code_block_padding(self.scale_factor);
                let left = frame_x + code.x;
                let right = frame_x + content_w - CONTENT_PAD * self.scale_factor;
                self.push_rounded_rect(
                    left,
                    block.top + offset_y,
                    (right - left).max(1.0),
                    (block.bottom - block.top).max(1.0),
                    5.0 * self.scale_factor,
                    [0.11, 0.12, 0.15, 0.96],
                );
                if let Some(language) = code.language.as_deref().filter(|lang| !lang.is_empty()) {
                    let header = code_header_geometry(
                        left,
                        right,
                        block.top + offset_y,
                        self.scale_factor,
                    );
                    self.draw_string_scaled_pixel_snapped(
                        language,
                        header.language_x,
                        header.text_y,
                        faded(self.theme.line_num, 0.9),
                        0.66,
                    );
                }
                for idx in visible_code_line_range(&code.lines, visible_top, visible_bottom) {
                    let line = &code.lines[idx];
                    let slice = source.get(line.source_range.clone()).unwrap_or("");
                    self.draw_mono_source_highlights(
                        slice,
                        line.source_range.start,
                        left + pad,
                        line.y + offset_y,
                        code.line_height,
                        1.0,
                        highlights,
                    );
                    self.draw_spanned_editor_line_pixel_snapped_alpha(
                        slice,
                        spans,
                        Some(line.source_range.start),
                        left + pad,
                        line.y + offset_y,
                        right - pad,
                        1.0,
                    );
                }
            }
            ReadBlockKind::Table(table) => {
                self.draw_quote_guides(table.quote_depth, frame_x, block.top + offset_y, block.bottom + offset_y);
                let cell_w = table.cell_width;
                let cell_padding = table.cell_padding;
                let line_height = table.line_height;
                let baseline_offset = (line_height * 0.82).round();
                for idx in visible_table_row_range(&table.rows, visible_top, visible_bottom) {
                    let row = &table.rows[idx];
                    let row_y = row.y + offset_y;
                    if row.header {
                        self.push_rect(frame_x + table.x, row_y, table.width, row.h, faded(self.theme.fg, 0.07));
                    }
                    for (col, cell) in row.cells.iter().enumerate() {
                        let cell_x = frame_x + table.x + col as f32 * cell_w;
                        self.push_rect(cell_x, row_y + row.h - 1.0, cell_w, 1.0, faded(self.theme.fg, 0.12));
                        if col > 0 {
                            self.push_rect(cell_x, row_y, 1.0, row.h, faded(self.theme.fg, 0.08));
                        }
                        let visible_lines = visible_table_cell_line_range(
                            cell.lines.len(),
                            row.y,
                            cell_padding,
                            line_height,
                            visible_top,
                            visible_bottom,
                        );
                        for line_idx in visible_lines {
                            let range = &cell.lines[line_idx];
                            let measured = self.measure_styled_fragment(&cell.styled, range, 0.82);
                            let tx = match cell.alignment {
                                MarkdownTableAlignment::Center => cell_x + (cell_w - measured) * 0.5,
                                MarkdownTableAlignment::Right => cell_x + cell_w - cell_padding - measured,
                                _ => cell_x + cell_padding,
                            };
                            let baseline = (row_y
                                + cell_padding
                                + line_idx as f32 * line_height
                                + baseline_offset)
                                .round();
                            self.draw_styled_fragment(
                                &cell.styled,
                                range,
                                tx,
                                baseline,
                                0.82,
                                false,
                                line_height,
                                highlights,
                            );
                        }
                    }
                }
            }
            ReadBlockKind::Rule { x, width, quote_depth } => {
                self.draw_quote_guides(*quote_depth, frame_x, block.top + offset_y - 5.0, block.bottom + offset_y + 5.0);
                self.push_rect(frame_x + *x, block.top + offset_y, *width, 1.0, faded(self.theme.fg, 0.22));
            }
        }
    }

    fn draw_quote_guides(&mut self, depth: usize, frame_x: f32, top: f32, bottom: f32) {
        for level in 0..depth {
            self.push_rect(
                frame_x + CONTENT_PAD * self.scale_factor + level as f32 * QUOTE_INDENT * self.scale_factor,
                top,
                (2.0 * self.scale_factor).round().max(1.0),
                (bottom - top).max(1.0),
                [0.52, 0.46, 0.72, 0.72],
            );
        }
    }

    fn draw_markdown_prefix(&mut self, prefix: &ReadPrefix, x: f32, baseline: f32) {
        match prefix {
            ReadPrefix::Bullet => self.draw_string_scaled_pixel_snapped("•", x + 7.0 * self.scale_factor, baseline, self.theme.fg, BODY_SCALE),
            ReadPrefix::Ordered(label) => {
                self.draw_string_scaled_pixel_snapped(label, x, baseline, self.theme.fg, 0.86);
            }
            ReadPrefix::Task(checked) => {
                let size = (13.0 * self.scale_factor).round();
                let top = baseline - size;
                self.push_rounded_rect_border(
                    x + 4.0 * self.scale_factor,
                    top,
                    size,
                    size,
                    2.0 * self.scale_factor,
                    1.0,
                    faded(self.theme.fg, 0.45),
                    faded(self.theme.bg, 0.96),
                );
                if *checked {
                    self.draw_string_scaled_pixel_snapped("✓", x + 5.0 * self.scale_factor, baseline - 1.0, [0.45, 0.86, 0.60, 1.0], 0.72);
                }
            }
        }
    }

    fn draw_styled_text_line(
        &mut self,
        block: &TextBlock,
        range: &Range<usize>,
        x: f32,
        y: f32,
        highlights: ReadHighlights<'_>,
    ) {
        let force_bold = block.heading_level.is_some();
        if block.mono {
            self.draw_styled_source_highlights(
                &block.styled,
                range,
                x,
                y,
                block.scale,
                true,
                block.line_height,
                highlights,
            );
            let text = block.styled.text.get(range.clone()).unwrap_or("");
            self.draw_string_mono_scaled_pixel_snapped(
                text,
                x,
                y,
                faded(self.theme.fg, 0.88),
                block.scale,
                force_bold,
            );
            return;
        }
        self.draw_styled_fragment(
            &block.styled,
            range,
            x,
            y,
            block.scale,
            force_bold,
            block.line_height,
            highlights,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn measure_styled_fragment(
        &mut self,
        styled: &StyledText,
        range: &Range<usize>,
        scale: f32,
    ) -> f32 {
        let pad = inline_code_padding_x(self.scale_factor);
        let visible_runs = visible_styled_run_range(&styled.runs, range);
        let mut width = 0.0;
        for run in &styled.runs[visible_runs] {
            let start = run.range.start.max(range.start);
            let end = run.range.end.min(range.end);
            if start >= end {
                continue;
            }
            let Some(text) = styled.text.get(start..end) else {
                continue;
            };
            if run.style.contains(TextStyle::CODE) {
                width += self.measure_mono_width_pixel_snapped(text, scale);
                if start == run.range.start {
                    width += pad;
                }
                if end == run.range.end {
                    width += pad;
                }
            } else {
                width += self.measure_ui_width(text, scale);
            }
        }
        width
    }


}

pub(crate) fn markdown_read_active(mode: MarkdownMode) -> bool {
    mode == MarkdownMode::Read
}

include!("markdown_read_interaction.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::markdown::MarkdownParseState;

    fn parse(source: &str) -> MarkdownDocument {
        MarkdownParseState::default().parse(source).expect("markdown parse")
    }

    fn layout_with_scale_and_advance(
        source: &str,
        width: f32,
        scale: f32,
        advance: impl FnMut(char, bool) -> f32,
    ) -> MarkdownReadLayoutCache {
        let doc = parse(source);
        let mut builder = LayoutBuilder::new(source, width, scale, advance);
        builder.append_blocks(&doc.blocks, 0.0, 0, None);
        let (blocks, content_height) = builder.finish();
        MarkdownReadLayoutCache {
            key: Some(LayoutKey {
                version: 1,
                width_bits: width.to_bits(),
                scale_bits: scale.to_bits(),
                font_size_bits: 16.0f32.to_bits(),
            }),
            blocks,
            content_height,
            rebuild_count: 1,
        }
    }

    fn layout_with_advance(
        source: &str,
        width: f32,
        advance: impl FnMut(char, bool) -> f32,
    ) -> MarkdownReadLayoutCache {
        layout_with_scale_and_advance(source, width, 1.0, advance)
    }

    fn layout(source: &str, width: f32) -> MarkdownReadLayoutCache {
        layout_with_advance(source, width, |_, _| 8.0)
    }

    fn visual_text(cache: &MarkdownReadLayoutCache) -> String {
        let mut out = String::new();
        for block in &cache.blocks {
            match &block.kind {
                ReadBlockKind::Text(text) => out.push_str(&text.styled.text),
                ReadBlockKind::Table(table) => {
                    for row in &table.rows { for cell in &row.cells { out.push_str(&cell.styled.text); } }
                }
                _ => {}
            }
        }
        out
    }

    #[test]
    fn headings_have_strict_visual_hierarchy() {
        let cache = layout(
            "# One\n\n## Two\n\n### Three\n\n#### Four\n\n##### Five\n\n###### Six\n\nBody\n",
            800.0,
        );
        let scales = cache
            .blocks
            .iter()
            .filter_map(|block| match &block.kind {
                ReadBlockKind::Text(text) if text.heading_level.is_some() => Some(text.scale),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(scales, (1..=6).map(heading_scale).collect::<Vec<_>>());
        assert!(scales.windows(2).all(|pair| pair[0] > pair[1]));
        assert!(scales[0] - scales[5] > 0.58, "hierarchy must exceed previous span");
        assert!(scales[5] >= BODY_SCALE);
        assert!(BODY_SCALE >= 0.95);
    }

    #[test]
    fn reader_layout_baselines_are_pixel_stable_at_fractional_scales() {
        let source = "# Heading\n\nParagraph with `inline code` and unicode 😀.\n\n```rust\nfn main() {}\n```\n\n| left | right |\n| --- | --- |\n| one | `two` |\n";
        for scale in [1.25, 1.32, 1.5, 1.75] {
            let cache = layout_with_scale_and_advance(source, 720.0 * scale, scale, |_, _| 8.0);
            for block in &cache.blocks {
                assert_eq!(block.top.fract(), 0.0, "scale {scale}: block top {}", block.top);
                assert_eq!(block.bottom.fract(), 0.0, "scale {scale}: block bottom {}", block.bottom);
                match &block.kind {
                    ReadBlockKind::Text(text) => {
                        for line in &text.lines {
                            assert_eq!(line.y.fract(), 0.0, "scale {scale}: text baseline {}", line.y);
                        }
                    }
                    ReadBlockKind::Code(code) => {
                        for line in &code.lines {
                            assert_eq!(line.y.fract(), 0.0, "scale {scale}: code baseline {}", line.y);
                        }
                    }
                    ReadBlockKind::Table(table) => {
                        let baseline_offset = (table.line_height * 0.82).round();
                        for row in &table.rows {
                            assert_eq!(row.y.fract(), 0.0, "scale {scale}: row y {}", row.y);
                            assert_eq!(row.h.fract(), 0.0, "scale {scale}: row h {}", row.h);
                            for cell in &row.cells {
                                for line_idx in 0..cell.lines.len() {
                                    let baseline = (row.y
                                        + table.cell_padding
                                        + line_idx as f32 * table.line_height
                                        + baseline_offset)
                                        .round();
                                    assert_eq!(baseline.fract(), 0.0);
                                }
                            }
                        }
                    }
                    ReadBlockKind::Rule { .. } => {}
                }
            }

            let line_h = (BODY_LINE_H * scale * BODY_SCALE).round().max(1.0);
            let baseline = (line_h * 0.82).round();
            let (pill_top, pill_h) = inline_code_vertical_bounds(baseline, scale, BODY_SCALE);
            assert!(pill_top >= 0.0, "scale {scale}: inline pill starts above line");
            assert!(pill_top + pill_h <= line_h, "scale {scale}: inline pill overlaps next line");
        }
    }

    #[test]
    fn inline_code_padding_and_reader_style_are_part_of_geometry() {
        let mut styled = StyledText::default();
        let code_style = TextStyle::default().with(TextStyle::CODE);
        styled.push("ab", code_style, Some(0..2));
        let mut advance = |_: char, mono: bool| if mono { 10.0 } else { 4.0 };
        let first = styled_char_advance(&styled, 0, 'a', BODY_SCALE, 1.0, false, &mut advance);
        let second = styled_char_advance(&styled, 1, 'b', BODY_SCALE, 1.0, false, &mut advance);
        assert_eq!(first + second, 28.0);
        assert_eq!(inline_code_padding_x(1.0), 4.0);
        assert_eq!(markdown_text_color(code_style, [0.0; 4]), DRACULA_YELLOW);

        let bg = [0.156, 0.164, 0.211, 1.0];
        let fg = [0.972, 0.972, 0.949, 1.0];
        let inline_bg = inline_code_background(bg, fg);
        assert!(inline_bg[0] > bg[0] && inline_bg[1] > bg[1] && inline_bg[2] > bg[2]);
        assert!(inline_bg[0] < fg[0] && inline_bg[1] < fg[1] && inline_bg[2] < fg[2]);
    }


    #[test]
    fn inline_code_uses_monospace_metrics_for_wrapping() {
        let source = "`abcdefgh` tail";
        let mono_aware = layout_with_advance(source, 150.0, |_, mono| if mono { 18.0 } else { 4.0 });
        let uniform = layout_with_advance(source, 150.0, |_, _| 4.0);
        let line_count = |cache: &MarkdownReadLayoutCache| {
            cache
                .blocks
                .iter()
                .find_map(|block| match &block.kind {
                    ReadBlockKind::Text(text) => Some(text.lines.len()),
                    _ => None,
                })
                .unwrap_or(0)
        };
        assert!(line_count(&mono_aware) > line_count(&uniform));
    }

    #[test]
    fn unicode_wrapping_keeps_utf8_boundaries() {
        let source = "Привет 😀 мир — длинный абзац с кириллицей и emoji.";
        let cache = layout(source, 150.0);
        let text = cache.blocks.iter().find_map(|block| match &block.kind { ReadBlockKind::Text(text) => Some(text), _ => None }).unwrap();
        for line in &text.lines {
            assert!(text.styled.text.is_char_boundary(line.range.start));
            assert!(text.styled.text.is_char_boundary(line.range.end));
        }
        assert!(text.lines.len() > 1);
    }

    #[test]
    fn quote_list_task_table_and_code_layouts_do_not_overlap() {
        let source = "> quote\n> continuation\n\n- [x] done\n- item\n\n| a | b |\n| --- | --- |\n| c | d |\n\n```rust\nfn main() {}\n```\n";
        let cache = layout(source, 520.0);
        assert!(cache.blocks.windows(2).all(|pair| pair[0].bottom <= pair[1].top));
        assert!(cache.blocks.iter().any(|b| matches!(b.kind, ReadBlockKind::Table(_))));
        assert!(cache.blocks.iter().any(|b| matches!(b.kind, ReadBlockKind::Code(_))));
    }

    #[test]
    fn semantic_continuation_markers_never_enter_visual_text() {
        let source = "> first\n> second\n> - nested\n>   - child\n";
        let cache = layout(source, 500.0);
        let text = visual_text(&cache);
        assert!(text.contains("first"));
        assert!(text.contains("second"));
        assert!(!text.contains("> "));
    }

    #[test]
    fn fenced_code_inside_quote_keeps_clean_source_ranges() {
        let source = "> ```rust\n> fn main() { println!(\"ok\"); }\n> ```\n";
        let cache = layout(source, 500.0);
        let code = cache.blocks.iter().find_map(|block| match &block.kind { ReadBlockKind::Code(code) => Some(code), _ => None }).unwrap();
        let visible = code.lines.iter().filter_map(|line| source.get(line.source_range.clone())).collect::<Vec<_>>().join("\n");
        assert!(visible.contains("fn main"));
        assert!(!visible.contains('>'));
        assert!(!visible.contains("```"));
    }

    #[test]
    fn table_draw_uses_cached_geometry_without_full_row_scan() {
        let source = include_str!("markdown_read.rs");
        let draw_start = source.find("    fn draw_markdown_block(").expect("draw function");
        let draw_end = source[draw_start..]
            .find("    fn draw_quote_guides(")
            .map(|offset| draw_start + offset)
            .expect("draw function end");
        let draw_path = &source[draw_start..draw_end];
        assert!(draw_path.contains("let cell_w = table.cell_width;"));
        assert!(draw_path.contains("let cell_padding = table.cell_padding;"));
        assert!(draw_path.contains("let line_height = table.line_height;"));
        assert!(draw_path.contains("visible_table_cell_line_range("));
        assert!(!draw_path.contains("table.rows.iter()"));
        assert!(!draw_path.contains("cell.lines.iter()"));
    }

    #[test]
    fn max_scroll_uses_preview_content_height() {
        let cache = layout("# H\n\nparagraph\n\nparagraph\n\nparagraph\n", 250.0);
        let viewport = 80.0;
        assert_eq!((cache.content_height() - viewport).max(0.0), cache.content_height() - viewport);
    }

    #[test]
    fn cache_invalidation_is_explicit_and_stable() {
        let mut cache = layout("text", 400.0);
        let key = cache.key.expect("layout key");
        assert_eq!(cache.rebuild_count(), 1);
        assert!(cache.is_valid_for(key));

        let changed_version = LayoutKey { version: key.version + 1, ..key };
        let changed_width = LayoutKey { width_bits: 420.0f32.to_bits(), ..key };
        assert!(!cache.is_valid_for(changed_version));
        assert!(!cache.is_valid_for(changed_width));

        cache.invalidate();
        assert!(!cache.is_valid_for(key));
        assert_eq!(cache.rebuild_count(), 1);
    }
}
