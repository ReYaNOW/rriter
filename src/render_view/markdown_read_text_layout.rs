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
    let top_pad = (INLINE_CODE_EXTRA_PAD_Y * scale_factor).round().max(1.0);
    let bottom_pad = top_pad
        + (INLINE_CODE_EXTRA_BOTTOM_PAD_Y * scale_factor)
            .round()
            .max(1.0);
    let top = baseline_y.round() - (17.0 * scale_factor * text_scale).round() - top_pad;
    let height = (20.0 * scale_factor * text_scale).round().max(1.0) + top_pad + bottom_pad;
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
        MARKDOWN_GOLD
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

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct VisualCharMetrics {
    leading: f32,
    advance: f32,
    trailing: f32,
}

impl VisualCharMetrics {
    #[inline]
    fn glyph(advance: f32) -> Self {
        Self {
            advance,
            ..Self::default()
        }
    }

    #[inline]
    fn width(self) -> f32 {
        self.leading + self.advance + self.trailing
    }
}

fn styled_char_metrics<F: FnMut(char, bool) -> f32>(
    styled: &StyledText,
    offset: usize,
    ch: char,
    text_scale: f32,
    layout_scale: f32,
    mono: bool,
    advance: &mut F,
) -> VisualCharMetrics {
    let run_idx = styled.runs.partition_point(|run| run.range.end <= offset);
    let run = styled
        .runs
        .get(run_idx)
        .filter(|run| run.range.start <= offset && offset < run.range.end);
    let inline_mono = run.is_some_and(|run| run.style.contains(TextStyle::CODE));
    let raw_advance = if matches!(ch, '\n' | '\r') || text_char_is_non_rendering_control(ch) {
        0.0
    } else {
        advance(ch, mono || inline_mono)
    };
    let mut metrics =
        VisualCharMetrics::glyph(Renderer::snapped_text_advance(raw_advance, text_scale));
    if inline_mono && let Some(run) = run {
        let pad = inline_code_padding_x(layout_scale);
        if offset == run.range.start {
            metrics.leading = pad;
        }
        if offset.saturating_add(ch.len_utf8()) >= run.range.end {
            metrics.trailing = pad;
        }
    }
    metrics
}

#[inline]
fn mono_char_pixel_advance<F: FnOnce() -> f32>(ch: char, scale: f32, advance: F) -> f32 {
    if matches!(ch, '\n' | '\r') || text_char_is_non_rendering_control(ch) {
        0.0
    } else {
        Renderer::snapped_text_advance(advance(), scale)
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
    styled_char_metrics(styled, offset, ch, text_scale, layout_scale, mono, advance).width()
}

#[inline]
pub(crate) fn markdown_read_scrollbar_width(max_scroll: f32, scale: f32) -> f32 {
    if max_scroll > 0.0 {
        (READ_SCROLLBAR_W * scale).round().max(4.0)
    } else {
        0.0
    }
}

/// Reader vertical scrollbar shared by the renderer and the press/drag handlers: a
/// `markdown_read_scrollbar_width` lane at the right edge of the Reader `frame`, whose
/// height is both the track and the viewport. `thumb_color` only matters for drawing.
pub(crate) fn markdown_read_scrollbar(
    frame: (f32, f32, f32, f32),
    content_height: f32,
    displayed_scroll_y: f32,
    scale: f32,
    thumb_color: [f32; 4],
) -> crate::render_view::scrollbar_widget::Scrollbar {
    use crate::render_view::scrollbar_widget::{
        Scrollbar, ScrollbarAxis, ScrollbarExtent, ScrollbarStyle,
    };
    let (x, y, w, h) = frame;
    let extent = ScrollbarExtent::new(h, content_height, displayed_scroll_y);
    let bar_w = markdown_read_scrollbar_width(extent.max_scroll, scale);
    Scrollbar {
        style: ScrollbarStyle {
            thumb_color,
            ..ScrollbarStyle::MARKDOWN_READ
        },
        axis: ScrollbarAxis::Vertical,
        lane: (x + w - bar_w, y, bar_w, h),
        extent,
    }
}

/// Exact (unrounded) Reader thumb along the axis, as press/drag use it.
pub(crate) fn markdown_read_scrollbar_thumb(
    track_y: f32,
    viewport_height: f32,
    content_height: f32,
    displayed_scroll_y: f32,
    scale: f32,
) -> Option<crate::scroll::ScrollbarThumb> {
    markdown_read_scrollbar(
        (0.0, track_y, 0.0, viewport_height),
        content_height,
        displayed_scroll_y,
        scale,
        [0.0; 4],
    )
    .geometry(scale)
    .map(|geometry| geometry.thumb)
}

fn register_markdown_read_text_surface(
    ui_registry: &mut UiRegistry,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    scrollbar_w: f32,
    mouse_x: f32,
    mouse_y: f32,
) {
    ui_registry.register_text_region(
        crate::ui_system::UiId::MarkdownReadBody,
        x,
        y,
        w,
        h,
        mouse_x,
        mouse_y,
    );
    if scrollbar_w > 0.0 {
        ui_registry.register_blocker(
            crate::ui_system::UiId::MarkdownReadScrollbar,
            x + (w - scrollbar_w).max(0.0),
            y,
            scrollbar_w,
            h,
            mouse_x,
            mouse_y,
        );
    }
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

fn push_source_range(
    styled: &mut StyledText,
    source: &str,
    range: &Range<usize>,
    style: TextStyle,
) {
    if let Some(text) = source.get(range.clone()) {
        styled.push(text, style, Some(range.clone()));
    }
}

/// Link targets collected while a layout is built, plus what resolving a destination needs:
/// the folder of the document and its link reference definitions.
#[derive(Default)]
struct LinkTable {
    targets: Vec<LinkTarget>,
    dir: std::path::PathBuf,
    defs: Vec<(String, String)>,
}

impl LinkTable {
    /// Index of `target` in the table. `Unsupported` is no link: it neither reacts to the
    /// pointer nor shows a hand.
    fn add(&mut self, target: LinkTarget) -> Option<u32> {
        if target == LinkTarget::Unsupported {
            return None;
        }
        let index = u32::try_from(self.targets.len()).ok()?;
        self.targets.push(target);
        Some(index)
    }
}

impl<'a, F: FnMut(char, bool, Option<f32>) -> f32> LayoutBuilder<'a, F> {
    /// Resolves relative destinations against `dir` and `[label]` ones against the definitions.
    fn with_links(mut self, dir: &Path, defs: Vec<(String, String)>) -> Self {
        self.links.dir = dir.to_path_buf();
        self.links.defs = defs;
        self
    }

    fn finish_with_links(mut self) -> (Vec<ReadBlock>, f32, Vec<LinkTarget>) {
        let links = std::mem::take(&mut self.links.targets);
        let (blocks, height) = self.finish();
        (blocks, height, links)
    }
}

fn styled_from_inlines(
    source: &str,
    inlines: &[MarkdownInlineSpan],
    fallback_ranges: &[Range<usize>],
    links: &mut LinkTable,
) -> StyledText {
    let mut styled = StyledText::default();
    if inlines.is_empty() {
        for range in fallback_ranges {
            push_source_range(&mut styled, source, range, TextStyle::default());
        }
        return styled;
    }
    for span in inlines {
        append_inline(&mut styled, source, span, TextStyle::default(), links);
    }
    styled
}

fn append_inline(
    styled: &mut StyledText,
    source: &str,
    span: &MarkdownInlineSpan,
    inherited: TextStyle,
    links: &mut LinkTable,
) {
    let mut style = inherited;
    match &span.style {
        MarkdownInlineStyle::Emphasis => style = style.with(TextStyle::EMPHASIS),
        MarkdownInlineStyle::Strong => style = style.with(TextStyle::STRONG),
        MarkdownInlineStyle::Code => style = style.with(TextStyle::CODE),
        MarkdownInlineStyle::Link { .. } | MarkdownInlineStyle::Uri => {
            style = style.with(TextStyle::LINK)
        }
        MarkdownInlineStyle::Image { .. } => {
            style = style.with(TextStyle::IMAGE);
            styled.push("Image: ", style, None);
        }
        MarkdownInlineStyle::HtmlRaw | MarkdownInlineStyle::Raw => {
            style = style.with(TextStyle::RAW)
        }
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
    // Every run of the link's text, nested styles and wrapped lines included, carries its index.
    let outer_link = styled.link;
    if let Some(target) = inline_link_target(source, span, &links.dir, &links.defs) {
        styled.link = links.add(target);
    }
    if !span.children.is_empty() {
        for child in &span.children {
            append_inline(styled, source, child, style, links);
        }
    } else {
        for range in &span.text_ranges {
            push_source_range(styled, source, range, style);
        }
    }
    styled.link = outer_link;
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
    let end = (((visible_bottom - content_top) / line_height)
        .ceil()
        .max(0.0) as usize)
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

