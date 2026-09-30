use std::ops::Range;

use super::core_text::text_char_is_non_rendering_control;
use crate::app::{MarkdownMode, MarkdownTabState};
use crate::highlighter::{ColorSpan, MARKDOWN_GOLD};
use crate::markdown_media::MarkdownMedia;
use crate::languages::markdown::{
    MarkdownBlock, MarkdownBlockKind, MarkdownInlineSpan, MarkdownInlineStyle, MarkdownListKind,
    MarkdownTableAlignment,
};
use crate::renderer::{EDITOR_SURFACE_BG, Renderer};
use crate::ui_system::UiRegistry;

const BODY_SCALE: f32 = 0.96;
const INLINE_CODE_PAD_X: f32 = 4.0;
const INLINE_CODE_EXTRA_PAD_Y: f32 = 0.75;
const INLINE_CODE_EXTRA_BOTTOM_PAD_Y: f32 = 2.0;
const INLINE_CODE_BG_MIX: f32 = 0.10;
const BODY_LINE_H: f32 = 24.0;
const BLOCK_GAP: f32 = 12.0;
const CONTENT_PAD: f32 = 28.0;
const QUOTE_INDENT: f32 = 18.0;
const LIST_INDENT: f32 = 26.0;
const OVERSCAN: f32 = 96.0;
const READ_SCROLLBAR_W: f32 = 9.0;

#[inline]
pub(crate) fn markdown_read_frame_x_for_editor_text(editor_text_x: f32, scale: f32) -> f32 {
    let content_inset = (CONTENT_PAD * scale).round();
    (editor_text_x.round() - content_inset).max(0.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LayoutKey {
    version: u64,
    width_bits: u32,
    scale_bits: u32,
    font_size_bits: u32,
}

impl LayoutKey {
    fn new(version: u64, width: f32, scale: f32, font_size: f32) -> Self {
        Self {
            version,
            width_bits: width.max(1.0).round().to_bits(),
            scale_bits: scale.to_bits(),
            font_size_bits: font_size.to_bits(),
        }
    }
}

#[derive(Default)]
pub(crate) struct MarkdownReadLayoutCache {
    key: Option<LayoutKey>,
    blocks: Vec<ReadBlock>,
    content_height: f32,
    rebuild_count: u64,
    source_len: usize,
    anchor_lines: Vec<ReadAnchorLine>,
    source_lines: Vec<ReadSourceLine>,
    source_prefix_max_end: Vec<usize>,
    source_scopes: Vec<ReadSourceScope>,
    media_gen: Option<u64>,
    media_dir: std::path::PathBuf,
}

impl MarkdownReadLayoutCache {
    pub(crate) fn invalidate(&mut self) {
        self.key = None;
        self.source_len = 0;
        self.anchor_lines.clear();
        self.source_lines.clear();
        self.source_prefix_max_end.clear();
        self.source_scopes.clear();
    }

    fn is_valid_for(&self, key: LayoutKey) -> bool {
        self.key == Some(key)
    }

    pub(crate) fn is_valid_for_geometry(
        &self,
        version: u64,
        width: f32,
        scale: f32,
        font_size: f32,
    ) -> bool {
        self.is_valid_for(LayoutKey::new(version, width, scale, font_size))
    }

    fn replace_layout(
        &mut self,
        key: LayoutKey,
        blocks: Vec<ReadBlock>,
        content_height: f32,
        source_len: usize,
    ) {
        let (anchor_lines, source_lines, source_prefix_max_end, source_scopes) =
            build_read_source_indices(&blocks);
        self.blocks = blocks;
        self.content_height = content_height;
        self.source_len = source_len;
        self.anchor_lines = anchor_lines;
        self.source_lines = source_lines;
        self.source_prefix_max_end = source_prefix_max_end;
        self.source_scopes = source_scopes;
        self.key = Some(key);
        self.rebuild_count = self.rebuild_count.saturating_add(1);
    }

    pub(crate) fn content_height(&self) -> f32 {
        self.content_height
    }

    #[cfg(test)]
    pub(crate) fn rebuild_count(&self) -> u64 {
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
            debug_assert_eq!(
                source_range.end.saturating_sub(source_range.start),
                text.len()
            );
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
    top: f32,
    y: f32,
    bottom: f32,
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
    top: f32,
    y: f32,
    bottom: f32,
}

#[derive(Clone, Debug)]
struct CodeBlock {
    lines: Vec<CodeLine>,
    content_ranges: Vec<Range<usize>>,
    x: f32,
    quote_depth: usize,
    language: Option<String>,
    line_height: f32,
    content_width: f32,
    error: Option<String>,
}

#[derive(Clone, Debug)]
struct TableCell {
    source_range: Range<usize>,
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
    baseline_offset: f32,
    quote_depth: usize,
}

#[derive(Clone, Debug)]
enum ReadBlockKind {
    Text(TextBlock),
    Code(CodeBlock),
    Table(TableBlock),
    Rule {
        x: f32,
        width: f32,
        quote_depth: usize,
    },
    Media {
        items: Vec<PlacedMedia>,
    },
}

#[derive(Clone, Debug)]
struct ReadBlock {
    source_range: Range<usize>,
    parent_source_ranges: Vec<Range<usize>>,
    top: f32,
    bottom: f32,
    kind: ReadBlockKind,
}

#[derive(Clone, Copy)]
struct LayoutTextMetrics {
    font_size: f32,
    line_height: f32,
    baseline_offset: f32,
}

impl LayoutTextMetrics {
    fn heading_baseline_offset(self, text_scale: f32, line_height: f32) -> f32 {
        if self.font_size <= 0.0 {
            return (line_height * 0.82).round();
        }
        let pixel_size = crate::renderer::final_text_pixel_size(self.font_size, text_scale);
        let ratio = pixel_size / self.font_size;
        let scaled_editor_line_height = self.line_height * ratio;
        let extra_leading = (line_height - scaled_editor_line_height) * 0.5;
        (self.baseline_offset * ratio + extra_leading).round()
    }
}

#[cfg(test)]
fn test_layout_text_metrics(scale: f32) -> LayoutTextMetrics {
    LayoutTextMetrics {
        font_size: 18.0 * scale,
        line_height: (26.0 * scale).round(),
        baseline_offset: (19.0 * scale).round(),
    }
}

struct LayoutBuilder<'a, F: FnMut(char, bool, Option<f32>) -> f32> {
    source: &'a str,
    width: f32,
    scale: f32,
    text_metrics: LayoutTextMetrics,
    y: f32,
    blocks: Vec<ReadBlock>,
    source_scope_stack: Vec<Range<usize>>,
    advance: F,
    media: Option<MediaInput<'a>>,
}

impl<'a, F: FnMut(char, bool, Option<f32>) -> f32> LayoutBuilder<'a, F> {
    fn new(
        source: &'a str,
        width: f32,
        scale: f32,
        text_metrics: LayoutTextMetrics,
        advance: F,
    ) -> Self {
        Self {
            source,
            width: width.max(1.0),
            scale,
            text_metrics,
            y: (18.0 * scale).round(),
            blocks: Vec::new(),
            source_scope_stack: Vec::new(),
            advance,
            media: None,
        }
    }

    fn push_read_block(
        &mut self,
        source_range: Range<usize>,
        top: f32,
        bottom: f32,
        kind: ReadBlockKind,
    ) {
        self.blocks.push(ReadBlock {
            source_range,
            parent_source_ranges: self.source_scope_stack.iter().rev().cloned().collect(),
            top,
            bottom,
            kind,
        });
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
        if self.try_append_media(block, indent) {
            return;
        }
        match &block.kind {
            MarkdownBlockKind::Heading {
                level,
                content_ranges,
                inlines,
            } => {
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
            MarkdownBlockKind::Paragraph {
                content_ranges,
                inlines,
            } => {
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
                self.source_scope_stack.push(block.source_range.clone());
                self.append_blocks(blocks, indent + QUOTE_INDENT * self.scale, depth, prefix);
                self.source_scope_stack.pop();
            }
            MarkdownBlockKind::List(list) => {
                self.source_scope_stack.push(block.source_range.clone());
                for item in &list.items {
                    let item_prefix = if let Some(checked) = item.task_checked {
                        ReadPrefix::Task(checked)
                    } else if list.kind == MarkdownListKind::Ordered {
                        ordered_prefix(item.ordered_index.unwrap_or(1))
                    } else {
                        ReadPrefix::Bullet
                    };
                    self.source_scope_stack.push(item.source_range.clone());
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
                    self.source_scope_stack.pop();
                }
                self.source_scope_stack.pop();
            }
            MarkdownBlockKind::Code(code) => {
                self.append_code(
                    &code.content_ranges,
                    code.language.clone(),
                    indent,
                    quote_depth,
                    prefix,
                    block.source_range.clone(),
                    self.media_code_error(block.source_range.start),
                );
            }
            MarkdownBlockKind::Table(table) => {
                self.append_table(
                    table,
                    indent,
                    quote_depth,
                    prefix,
                    block.source_range.clone(),
                );
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
                self.push_read_block(
                    block.source_range.clone(),
                    top,
                    top + h,
                    ReadBlockKind::Rule {
                        x,
                        width,
                        quote_depth,
                    },
                );
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
                    push_source_range(
                        &mut styled,
                        self.source,
                        range,
                        TextStyle::default().with(TextStyle::LINK),
                    );
                }
                if let Some(range) = reference.destination_range.as_ref() {
                    styled.push("  ", TextStyle::default(), None);
                    push_source_range(
                        &mut styled,
                        self.source,
                        range,
                        TextStyle::default().with(TextStyle::LINK),
                    );
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
            MarkdownBlockKind::HtmlRaw
            | MarkdownBlockKind::MetadataRaw
            | MarkdownBlockKind::Raw => {
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
        let line_h = ((if heading_level.is_some() {
            28.0
        } else {
            BODY_LINE_H
        }) * self.scale
            * text_scale.max(0.75))
        .round()
        .max(1.0);
        let final_size_scale = heading_level.map(|_| text_scale);
        let advance_scale = if final_size_scale.is_some() {
            1.0
        } else {
            text_scale
        };
        let mut advance = |offset: usize, ch: char| {
            let mut glyph_advance =
                |c: char, use_mono: bool| (self.advance)(c, use_mono, final_size_scale);
            styled_char_advance(
                &styled,
                offset,
                ch,
                advance_scale,
                self.scale,
                mono,
                &mut glyph_advance,
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
        let baseline_offset = if heading_level.is_some() {
            self.text_metrics
                .heading_baseline_offset(text_scale, line_h)
        } else {
            (line_h * 0.82).round()
        };
        let mut lines = Vec::with_capacity(ranges.len());
        for (idx, (start, end)) in ranges.into_iter().enumerate() {
            let line_top = (self.y + line_h * idx as f32).round();
            lines.push(TextLine {
                range: start..end,
                top: line_top,
                y: (line_top + baseline_offset).round(),
                bottom: (line_top + line_h).round(),
            });
        }
        let line_count = lines.len().max(1) as f32;
        let bottom = (self.y + line_count * line_h).round();
        self.push_read_block(
            source_range,
            top,
            bottom,
            ReadBlockKind::Text(TextBlock {
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
        );
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
        error: Option<String>,
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
        let error_h = if error.is_some() { line_h } else { 0.0 };
        let mut y = top + pad + header_h + error_h;
        let mut lines = Vec::new();
        let mut content_width = 0.0f32;
        for range in ranges {
            let Some(text) = self.source.get(range.clone()) else {
                continue;
            };
            let mut local = 0usize;
            for part in text.split_inclusive('\n') {
                let visible = part.trim_end_matches(['\r', '\n']);
                content_width =
                    content_width.max(code_line_pixel_width(visible, &mut self.advance));
                let start = range.start + local;
                let end = start + visible.len();
                lines.push(CodeLine {
                    source_range: start..end,
                    top: y.round(),
                    y: (y + line_h * 0.82).round(),
                    bottom: (y + line_h).round(),
                });
                y += line_h;
                local += part.len();
            }
            if text.is_empty() {
                lines.push(CodeLine {
                    source_range: range.start..range.start,
                    top: y.round(),
                    y: (y + line_h * 0.82).round(),
                    bottom: (y + line_h).round(),
                });
                y += line_h;
            }
        }
        if lines.is_empty() {
            lines.push(CodeLine {
                source_range: source_range.start..source_range.start,
                top: y.round(),
                y: (y + line_h * 0.82).round(),
                bottom: (y + line_h).round(),
            });
            y += line_h;
        }
        let reserve = code_block_overflow_reserve(content_width, self.width, x, self.scale);
        let bottom = (y + pad + reserve).round();
        self.push_read_block(
            source_range,
            top,
            bottom,
            ReadBlockKind::Code(CodeBlock {
                lines,
                content_ranges: ranges.to_vec(),
                x,
                quote_depth,
                language,
                line_height: line_h,
                content_width,
                error,
            }),
        );
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
        let width = (self.width - x - CONTENT_PAD * self.scale)
            .max(40.0 * self.scale)
            .round();
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
        let text_scale = 0.82;
        let baseline_offset = self
            .text_metrics
            .heading_baseline_offset(text_scale, line_h);
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
                    styled_from_inlines(
                        self.source,
                        &cell.inlines,
                        std::slice::from_ref(&cell.source_range),
                    )
                });
                let max_text_w = (cell_w - pad * 2.0).max(8.0);
                let mut advance = |offset: usize, ch: char| {
                    let mut glyph_advance =
                        |c: char, use_mono: bool| (self.advance)(c, use_mono, None);
                    styled_char_advance(
                        &styled,
                        offset,
                        ch,
                        text_scale,
                        self.scale,
                        false,
                        &mut glyph_advance,
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
                    source_range: row
                        .cells
                        .get(col)
                        .map(|cell| cell.source_range.clone())
                        .unwrap_or_else(|| row.source_range.end..row.source_range.end),
                    styled,
                    lines,
                    alignment: table
                        .alignments
                        .get(col)
                        .copied()
                        .unwrap_or(MarkdownTableAlignment::None),
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
        self.push_read_block(
            source_range,
            top,
            bottom,
            ReadBlockKind::Table(TableBlock {
                rows,
                x,
                width,
                cell_width: cell_w,
                cell_padding: pad,
                line_height: line_h,
                baseline_offset,
                quote_depth,
            }),
        );
        self.y = bottom + (BLOCK_GAP * self.scale).round();
    }
}

include!("markdown_read_text_layout.rs");
include!("markdown_read_media.rs");
impl Renderer {
    #[inline]
    fn markdown_read_char_advance(
        &mut self,
        ch: char,
        mono: bool,
        final_size_scale: Option<f32>,
    ) -> f32 {
        if let Some(scale) = final_size_scale {
            let pixel_size = self.final_text_pixel_size(scale);
            if mono {
                self.char_advance_at_size(ch, pixel_size)
            } else {
                self.get_ui_glyph_at_size(ch, pixel_size)
                    .map_or(0.0, |glyph| glyph.advance)
            }
        } else if mono {
            self.char_advance(ch)
        } else {
            self.get_ui_glyph(ch).map_or(0.0, |glyph| glyph.advance)
        }
    }

    pub(crate) fn prepare_markdown_read_layout(
        &mut self,
        markdown: &mut MarkdownTabState,
        media: Option<&MarkdownMedia>,
        editor_version: u64,
        content_width: f32,
    ) -> bool {
        let content_width = content_width.max(1.0);
        let key = LayoutKey::new(
            editor_version,
            content_width,
            self.scale_factor,
            self.font_size,
        );
        let media_gen = media.map(MarkdownMedia::media_gen);
        if markdown.read_layout.is_current(key, media_gen) {
            return true;
        }
        let (blocks, content_height, source_len) = {
            let Some(document) = markdown.read_document(editor_version) else {
                return false;
            };
            let source = markdown.read_source.as_str();
            let scale = self.scale_factor;
            let text_metrics = LayoutTextMetrics {
                font_size: self.font_size,
                line_height: self.line_height,
                baseline_offset: self.baseline_offset,
            };
            let mut advance = |ch: char, mono: bool, final_size_scale: Option<f32>| {
                self.markdown_read_char_advance(ch, mono, final_size_scale)
            };
            let input = media
                .map(|m| MediaInput::new(document, source, &markdown.read_layout.media_dir, m));
            let mut builder =
                LayoutBuilder::new(source, content_width, scale, text_metrics, &mut advance)
                    .with_media(input);
            builder.append_blocks(&document.blocks, 0.0, 0, None);
            let (blocks, content_height) = builder.finish();
            (blocks, content_height, source.len())
        };
        markdown
            .read_layout
            .replace_layout(key, blocks, content_height, source_len);
        markdown.read_layout.media_gen = media_gen;
        true
    }

    pub(crate) fn draw_markdown_read(
        &mut self,
        markdown: &mut MarkdownTabState,
        media: Option<&MarkdownMedia>,
        editor: &crate::editor::Editor,
        scroll: &mut crate::scroll::ScrollState,
        spans: &[ColorSpan],
        search_results: &[(usize, usize)],
        search_current_idx: Option<usize>,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        ui_registry: &mut UiRegistry,
    ) {
        self.push_rect(x, y, w, h, EDITOR_SURFACE_BG);
        let editor_version = editor.version;

        if markdown.read_document(editor_version).is_none() {
            markdown.finish_read_selection_gesture(scroll);
            scroll.end_drag();
            ui_registry.register_blocker(
                crate::ui_system::UiId::MarkdownReadBody,
                x,
                y,
                w,
                h,
                self.last_mouse_x,
                self.last_mouse_y,
            );
            self.draw_string_scaled_pixel_snapped(
                "Markdown preview is preparing…",
                x + 28.0 * self.scale_factor,
                y + 42.0 * self.scale_factor,
                self.theme.line_num,
                0.88,
            );
            markdown.invalidate_read_scroll_bounds();
            return;
        }

        let content_w = w.max(1.0);
        let layout_was_valid = markdown.read_layout.is_valid_for_geometry(
            editor_version,
            content_w,
            self.scale_factor,
            self.font_size,
        );
        if !layout_was_valid {
            markdown.finish_read_selection_gesture(scroll);
            scroll.end_drag();
        }
        if !self.prepare_markdown_read_layout_preserving_current_ownership(
            markdown,
            media,
            scroll,
            editor_version,
            content_w,
        ) {
            markdown.finish_read_selection_gesture(scroll);
            scroll.end_drag();
            markdown.cancel_stale_scroll_transition();
            markdown.invalidate_read_scroll_bounds();
            return;
        }

        let max_scroll = (markdown.read_layout.content_height() - h).max(0.0);
        markdown.set_read_scroll_bounds(max_scroll);
        if max_scroll <= 0.0 {
            scroll.end_drag();
        }
        if let Some(transition) = markdown.pending_transition_for(MarkdownMode::Read) {
            if !markdown.pending_transition_is_valid(&transition, editor_version) {
                markdown.cancel_stale_scroll_transition();
            } else if !markdown.refresh_pending_read_absolute_target(scroll, h, max_scroll) {
                markdown.cancel_stale_scroll_transition();
            } else {
                let anchor = transition.anchor.clone().or_else(|| {
                    (transition.from == MarkdownMode::Edit).then(|| {
                        let origin_line_height = markdown
                            .displayed_edit_line_height(transition.version)
                            .unwrap_or(self.line_height);
                        let viewport_y = transition.origin_scroll_y
                            + transition.origin_sticky_lines as f32 * origin_line_height;
                        self.markdown_edit_viewport_anchor_with_line_height(
                            editor,
                            viewport_y,
                            origin_line_height,
                        )
                    })?
                });
                if let Some(anchor) = anchor
                    && let Some(line_y) = markdown.read_layout.source_anchor_y(&anchor.source_range)
                {
                    markdown.apply_scroll_transition(
                        scroll,
                        editor_version,
                        anchor,
                        line_y,
                        0.0,
                        max_scroll,
                    );
                } else {
                    markdown.cancel_stale_scroll_transition();
                }
            }
        }
        scroll.clamp_target(0.0, max_scroll);
        scroll.clamp_current(0.0, max_scroll);
        markdown.remember_displayed_read_geometry(
            editor_version,
            content_w,
            self.scale_factor,
            self.font_size,
        );
        let scrollbar_w = markdown_read_scrollbar_width(max_scroll, self.scale_factor);
        register_markdown_read_text_surface(
            ui_registry,
            x,
            y,
            w,
            h,
            scrollbar_w,
            self.last_mouse_x,
            self.last_mouse_y,
        );
        let scroll_y = scroll.current.round();
        let visible = visible_block_range(
            &markdown.read_layout.blocks,
            (scroll_y - OVERSCAN * self.scale_factor).max(0.0),
            scroll_y + h + OVERSCAN * self.scale_factor,
        );

        self.flush();
        unsafe {
            use glow::HasContext;
            self.gl.enable(glow::SCISSOR_TEST);
        }
        self.set_markdown_read_scissor(x, y, w, h);
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
                markdown.code_scroll_x(block.source_range.start),
                (x, y, w, h),
                media,
            );
            self.register_markdown_code_scrollbar(block, x, y - scroll_y, content_w, ui_registry);
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

        if scrollbar_w > 0.0 {
            let bar = markdown_read_scrollbar(
                (x, y, w, h),
                markdown.read_layout.content_height(),
                scroll_y,
                self.scale_factor,
                faded(self.theme.fg, 0.45),
            );
            let _ = self.draw_scrollbar(&bar, self.scale_factor, 1.0, None);
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
        code_scroll_x: f32,
        reader_clip: (f32, f32, f32, f32),
        media: Option<&MarkdownMedia>,
    ) {
        let offset_y = frame_y - scroll_y;
        match &block.kind {
            ReadBlockKind::Text(text) => {
                self.draw_quote_guides(
                    text.quote_depth,
                    frame_x,
                    block.top + offset_y,
                    block.bottom + offset_y,
                );
                if let Some(prefix) = text.prefix.as_ref() {
                    self.draw_markdown_prefix(
                        prefix,
                        frame_x + text.x - prefix_width(Some(prefix), self.scale_factor),
                        text.lines
                            .first()
                            .map_or(block.top + offset_y, |line| line.y + offset_y),
                    );
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
                        line.top + offset_y,
                        highlights,
                    );
                }
            }
            ReadBlockKind::Code(code) => {
                self.draw_quote_guides(
                    code.quote_depth,
                    frame_x,
                    block.top + offset_y,
                    block.bottom + offset_y,
                );
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
                    let header =
                        code_header_geometry(left, right, block.top + offset_y, self.scale_factor);
                    let mut scratch = std::mem::take(&mut self.scratch_buffer);
                    self.draw_tree_label_clipped(
                        language,
                        header.language_x,
                        header.text_y,
                        header.language_max_w,
                        faded(self.theme.line_num, 0.9),
                        CODE_LANGUAGE_SCALE,
                        &mut scratch,
                    );
                    self.scratch_buffer = scratch;
                }
                if let Some(error) = code.error.as_deref() {
                    self.draw_markdown_code_error(code, error, left, right, block.top + offset_y);
                }
                self.draw_markdown_code_lines(
                    code,
                    source,
                    spans,
                    frame_x,
                    content_w,
                    block.bottom + offset_y,
                    offset_y,
                    visible_top,
                    visible_bottom,
                    highlights,
                    code_scroll_x,
                    reader_clip,
                );
            }
            ReadBlockKind::Table(table) => {
                self.draw_quote_guides(
                    table.quote_depth,
                    frame_x,
                    block.top + offset_y,
                    block.bottom + offset_y,
                );
                let cell_w = table.cell_width;
                let cell_padding = table.cell_padding;
                let line_height = table.line_height;
                let baseline_offset = table.baseline_offset;
                for idx in visible_table_row_range(&table.rows, visible_top, visible_bottom) {
                    let row = &table.rows[idx];
                    let row_y = row.y + offset_y;
                    if row.header {
                        self.push_rect(
                            frame_x + table.x,
                            row_y,
                            table.width,
                            row.h,
                            faded(self.theme.fg, 0.07),
                        );
                    }
                    for (col, cell) in row.cells.iter().enumerate() {
                        let cell_x = frame_x + table.x + col as f32 * cell_w;
                        self.push_rect(
                            cell_x,
                            row_y + row.h - 1.0,
                            cell_w,
                            1.0,
                            faded(self.theme.fg, 0.12),
                        );
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
                                MarkdownTableAlignment::Center => {
                                    cell_x + (cell_w - measured) * 0.5
                                }
                                MarkdownTableAlignment::Right => {
                                    cell_x + cell_w - cell_padding - measured
                                }
                                _ => cell_x + cell_padding,
                            };
                            let line_top =
                                (row_y + cell_padding + line_idx as f32 * line_height).round();
                            let baseline = (line_top + baseline_offset).round();
                            self.draw_styled_fragment(
                                &cell.styled,
                                range,
                                tx,
                                baseline,
                                line_top,
                                0.82,
                                false,
                                line_height,
                                highlights,
                                None,
                            );
                        }
                    }
                }
            }
            ReadBlockKind::Rule {
                x,
                width,
                quote_depth,
            } => {
                self.draw_quote_guides(
                    *quote_depth,
                    frame_x,
                    block.top + offset_y - 5.0,
                    block.bottom + offset_y + 5.0,
                );
                self.push_rect(
                    frame_x + *x,
                    block.top + offset_y,
                    *width,
                    1.0,
                    faded(self.theme.fg, 0.22),
                );
            }
            ReadBlockKind::Media { items } => {
                self.draw_markdown_media(items, media, frame_x, block.top + offset_y);
            }
        }
    }

    fn draw_quote_guides(&mut self, depth: usize, frame_x: f32, top: f32, bottom: f32) {
        for level in 0..depth {
            self.push_rect(
                frame_x
                    + CONTENT_PAD * self.scale_factor
                    + level as f32 * QUOTE_INDENT * self.scale_factor,
                top,
                (2.0 * self.scale_factor).round().max(1.0),
                (bottom - top).max(1.0),
                [0.52, 0.46, 0.72, 0.72],
            );
        }
    }

    fn draw_markdown_prefix(&mut self, prefix: &ReadPrefix, x: f32, baseline: f32) {
        match prefix {
            ReadPrefix::Bullet => self.draw_string_scaled_pixel_snapped(
                "•",
                x + 7.0 * self.scale_factor,
                baseline,
                self.theme.fg,
                BODY_SCALE,
            ),
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
                    self.draw_string_scaled_pixel_snapped(
                        "✓",
                        x + 5.0 * self.scale_factor,
                        baseline - 1.0,
                        [0.45, 0.86, 0.60, 1.0],
                        0.72,
                    );
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
        line_top: f32,
        highlights: ReadHighlights<'_>,
    ) {
        let force_bold = block.heading_level.is_some();
        if block.mono {
            self.draw_styled_source_highlights(
                &block.styled,
                range,
                x,
                line_top,
                block.scale,
                true,
                block.line_height,
                highlights,
                None,
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
            line_top,
            block.scale,
            force_bold,
            block.line_height,
            highlights,
            block.heading_level.map(|_| block.scale),
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

include!("markdown_scroll.rs");
include!("markdown_read_interaction.rs");
include!("markdown_code_scroll.rs");

#[cfg(test)]
mod tests {
    include!("markdown_read_tests.rs");
}
