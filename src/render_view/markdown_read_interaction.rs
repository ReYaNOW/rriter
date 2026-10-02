// Markdown Reader interaction/source mapping responsibility chunk.
// Included by markdown_read.rs so cached layout structs stay private to the parent module.

use crate::widgets::{IconButton, IconType};

const CODE_PAD: f32 = 12.0;
const CODE_HEADER_H: f32 = 30.0;
const CODE_ACTION_SIZE: f32 = 30.0;
const CODE_ACTION_ICON_SIZE: f32 = 16.0;
const CODE_LANGUAGE_SCALE: f32 = 0.80;
const CODE_LANGUAGE_COPY_GAP: f32 = 6.0;
const CODE_LANGUAGE_BASELINE_OFFSET: f32 = 13.5;

#[derive(Clone, Copy, Debug, PartialEq)]
struct CodeHeaderGeometry {
    language_x: f32,
    language_max_w: f32,
    text_y: f32,
    button_x: f32,
    button_y: f32,
    button_size: f32,
    button_icon_size: f32,
}

fn code_block_padding(scale: f32) -> f32 {
    (CODE_PAD * scale).round()
}

fn code_header_height(scale: f32) -> f32 {
    (CODE_HEADER_H * scale).round().max(1.0)
}

fn code_header_geometry(left: f32, right: f32, top: f32, scale: f32) -> CodeHeaderGeometry {
    let pad = code_block_padding(scale);
    let header_h = code_header_height(scale);
    let left = left.round();
    let right = right.round().max(left);
    let available_w = right - left;
    let button_size = (CODE_ACTION_SIZE * scale).round().max(1.0).min(available_w);
    let button_x = (right - pad - button_size)
        .round()
        .clamp(left, (right - button_size).max(left));
    let button_icon_size = (CODE_ACTION_ICON_SIZE * scale)
        .round()
        .max(1.0)
        .min(button_size);
    let language_x = (left + pad).round().min(button_x);
    let language_gap = (CODE_LANGUAGE_COPY_GAP * scale).round().max(1.0);
    CodeHeaderGeometry {
        language_x,
        language_max_w: (button_x - language_gap - language_x).max(0.0),
        text_y: (top + pad + CODE_LANGUAGE_BASELINE_OFFSET * scale).round(),
        button_x,
        button_y: (top + pad + (header_h - button_size) * 0.5).round(),
        button_size,
        button_icon_size,
    }
}

impl MarkdownReadLayoutCache {
    pub(crate) fn copy_source_selection(&self, source: &str, selection: &Range<usize>) -> String {
        let mut out = String::new();
        for block in &self.blocks {
            if !ranges_overlap(&block.source_range, selection) {
                continue;
            }
            let mut block_text = String::new();
            match &block.kind {
                ReadBlockKind::Text(text) => {
                    append_selected_styled_text(
                        &mut block_text,
                        &text.styled,
                        source,
                        selection,
                    );
                }
                ReadBlockKind::Code(code) => {
                    append_selected_code_text(
                        &mut block_text,
                        &code.lines,
                        source,
                        selection,
                    );
                }
                ReadBlockKind::Table(table) => {
                    append_selected_table_text(
                        &mut block_text,
                        table,
                        source,
                        selection,
                    );
                }
                ReadBlockKind::Rule { .. } | ReadBlockKind::Media { .. } => {}
            }
            if block_text.is_empty() {
                continue;
            }
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&block_text);
        }
        out
    }

    pub(crate) fn source_target_y(&self, source_range: &Range<usize>) -> Option<f32> {
        self.source_anchor_y(source_range)
    }

    /// Targets of the layout's links; `StyledRun::link` and `PlacedMedia::link` index them.
    pub(crate) fn links(&self) -> &[LinkTarget] {
        &self.links
    }

    /// The layout was built from the text of `version` (its geometry may be any).
    pub(crate) fn is_for_version(&self, version: u64) -> bool {
        self.key.is_some_and(|key| key.version == version)
    }

    /// Folder of the document the links were resolved against.
    pub(crate) fn doc_dir(&self) -> &Path {
        &self.media_dir
    }

    fn replace_layout_with_links(
        &mut self,
        key: LayoutKey,
        blocks: Vec<ReadBlock>,
        content_height: f32,
        source_len: usize,
        links: Vec<LinkTarget>,
    ) {
        self.replace_layout(key, blocks, content_height, source_len);
        self.links = links;
    }

    /// Link under a point given in content coordinates: `x` from the left edge of the Reader
    /// frame, `y` from the top of the document. Only a character (or an image) of the link
    /// counts: the empty space of a line, and the gap between two links, do not.
    pub(crate) fn link_at<F>(&self, x: f32, y: f32, scale: f32, advance: &mut F) -> Option<u32>
    where
        F: FnMut(char, bool, Option<f32>) -> f32,
    {
        let block = self.blocks.get(self.blocks.partition_point(|block| block.bottom < y))?;
        if y < block.top {
            return None;
        }
        match &block.kind {
            ReadBlockKind::Text(text) => {
                let line = text.lines.get(text.lines.partition_point(|line| line.bottom <= y))?;
                let face = TextFace {
                    text_scale: text.scale,
                    layout_scale: scale,
                    mono: text.mono,
                    final_size_scale: text.heading_level.map(|_| text.scale),
                };
                (y >= line.top)
                    .then(|| styled_link_at_x(&text.styled, &line.range, x - text.x, face, advance))
                    .flatten()
            }
            ReadBlockKind::Table(table) => table_link_at(table, x, y, scale, advance),
            ReadBlockKind::Media { items } => items
                .iter()
                .find(|item| {
                    x >= item.x
                        && x < item.x + item.w
                        && y >= block.top + item.y
                        && y < block.top + item.y + item.h
                })
                .and_then(|item| item.link),
            ReadBlockKind::Code(_) | ReadBlockKind::Rule { .. } => None,
        }
    }

    pub(crate) fn code_block_copy_text(&self, source: &str, block_id: usize) -> Option<String> {
        let start = self
            .blocks
            .partition_point(|block| block.source_range.start < block_id);
        let code = self.blocks[start..]
            .iter()
            .take_while(|block| block.source_range.start == block_id)
            .find_map(|block| match &block.kind {
                ReadBlockKind::Code(code) => Some(code),
                _ => None,
            })?;

        let capacity = code
            .content_ranges
            .iter()
            .map(|range| range.end.saturating_sub(range.start))
            .sum();
        let mut out = String::with_capacity(capacity);
        for range in &code.content_ranges {
            out.push_str(source.get(range.clone())?);
        }
        Some(out)
    }
}

fn ranges_overlap(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

fn source_intersection(a: &Range<usize>, b: &Range<usize>) -> Option<Range<usize>> {
    let start = a.start.max(b.start);
    let end = a.end.min(b.end);
    (start < end).then_some(start..end)
}

fn append_selected_styled_text(
    out: &mut String,
    styled: &StyledText,
    source: &str,
    selection: &Range<usize>,
) {
    for run in &styled.runs {
        let Some(source_range) = run.source_range.as_ref() else {
            continue;
        };
        let Some(overlap) = source_intersection(source_range, selection) else {
            continue;
        };
        if let Some(text) = source.get(overlap) {
            out.push_str(text);
        }
    }
}

fn append_selected_code_text(
    out: &mut String,
    lines: &[CodeLine],
    source: &str,
    selection: &Range<usize>,
) {
    for (idx, line) in lines.iter().enumerate() {
        if let Some(overlap) = source_intersection(&line.source_range, selection)
            && let Some(text) = source.get(overlap)
        {
            out.push_str(text);
        }
        let Some(next) = lines.get(idx + 1) else {
            continue;
        };
        if selection.start < next.source_range.start
            && selection.end > line.source_range.end
            && line.source_range.end <= next.source_range.start
            && source
                .get(line.source_range.end..next.source_range.start)
                .is_some_and(|between| between.contains('\n'))
        {
            out.push('\n');
        }
    }
}

fn append_selected_table_text(
    out: &mut String,
    table: &TableBlock,
    source: &str,
    selection: &Range<usize>,
) {
    let mut wrote_row = false;
    for row in &table.rows {
        if !ranges_overlap(&row.source_range, selection) {
            continue;
        }
        let mut row_text = String::new();
        let mut wrote_cell = false;
        for cell in &row.cells {
            let mut cell_text = String::new();
            append_selected_styled_text(&mut cell_text, &cell.styled, source, selection);
            if cell_text.is_empty() {
                continue;
            }
            let mapped_extent = cell
                .styled
                .runs
                .iter()
                .filter_map(|run| run.source_range.as_ref())
                .fold(None, |extent: Option<Range<usize>>, range| {
                    Some(match extent {
                        Some(extent) => extent.start.min(range.start)..extent.end.max(range.end),
                        None => range.clone(),
                    })
                });
            let cell_text = if mapped_extent
                .as_ref()
                .is_some_and(|extent| selection.start <= extent.start && selection.end >= extent.end)
            {
                cell_text.trim_matches(|ch| ch == ' ' || ch == '\t')
            } else {
                cell_text.as_str()
            };
            if cell_text.is_empty() {
                continue;
            }
            if wrote_cell {
                row_text.push('\t');
            }
            row_text.push_str(cell_text);
            wrote_cell = true;
        }
        if wrote_cell {
            if wrote_row {
                out.push('\n');
            }
            out.push_str(&row_text);
            wrote_row = true;
        }
    }
}

/// How a laid-out piece of text is measured: the parameters `styled_char_metrics` needs.
#[derive(Clone, Copy)]
struct TextFace {
    text_scale: f32,
    layout_scale: f32,
    mono: bool,
    final_size_scale: Option<f32>,
}

fn styled_metrics_at<F>(
    styled: &StyledText,
    byte: usize,
    ch: char,
    face: TextFace,
    advance: &mut F,
) -> VisualCharMetrics
where
    F: FnMut(char, bool, Option<f32>) -> f32,
{
    let advance_scale = if face.final_size_scale.is_some() {
        1.0
    } else {
        face.text_scale
    };
    let mut glyph = |c: char, use_mono: bool| advance(c, use_mono, face.final_size_scale);
    styled_char_metrics(
        styled,
        byte,
        ch,
        advance_scale,
        face.layout_scale,
        face.mono,
        &mut glyph,
    )
}

fn styled_link_at_byte(styled: &StyledText, byte: usize) -> Option<u32> {
    let idx = styled.runs.partition_point(|run| run.range.end <= byte);
    styled
        .runs
        .get(idx)
        .filter(|run| run.range.start <= byte)
        .and_then(|run| run.link)
}

/// Link of the character of the line `range` whose box holds `target_x` (measured from the
/// start of the line); `None` right of the text and on characters that are not a link.
fn styled_link_at_x<F>(
    styled: &StyledText,
    range: &Range<usize>,
    target_x: f32,
    face: TextFace,
    advance: &mut F,
) -> Option<u32>
where
    F: FnMut(char, bool, Option<f32>) -> f32,
{
    if target_x < 0.0 {
        return None;
    }
    let text = styled.text.get(range.clone())?;
    let mut x = 0.0;
    let mut byte = range.start;
    for ch in text.chars() {
        x += styled_metrics_at(styled, byte, ch, face, advance).width();
        if target_x < x {
            return styled_link_at_byte(styled, byte);
        }
        byte += ch.len_utf8();
    }
    None
}

fn table_link_at<F>(table: &TableBlock, x: f32, y: f32, scale: f32, advance: &mut F) -> Option<u32>
where
    F: FnMut(char, bool, Option<f32>) -> f32,
{
    let row = table.rows.get(table.rows.partition_point(|row| row.y + row.h <= y))?;
    let col = ((x - table.x) / table.cell_width.max(1.0)).floor();
    let line = ((y - row.y - table.cell_padding) / table.line_height.max(1.0)).floor();
    if y < row.y || col < 0.0 || line < 0.0 {
        return None;
    }
    let cell = row.cells.get(col as usize)?;
    let range = cell.lines.get(line as usize)?;
    let face = TextFace {
        text_scale: 0.82,
        layout_scale: scale,
        mono: false,
        final_size_scale: None,
    };
    let cell_x = table.x + col * table.cell_width;
    let text_x = match cell.alignment {
        MarkdownTableAlignment::Left | MarkdownTableAlignment::None => cell_x + table.cell_padding,
        MarkdownTableAlignment::Center | MarkdownTableAlignment::Right => {
            let mut width = 0.0;
            let mut byte = range.start;
            for ch in cell.styled.text.get(range.clone())?.chars() {
                width += styled_metrics_at(&cell.styled, byte, ch, face, advance).width();
                byte += ch.len_utf8();
            }
            if cell.alignment == MarkdownTableAlignment::Center {
                cell_x + (table.cell_width - width) * 0.5
            } else {
                cell_x + table.cell_width - table.cell_padding - width
            }
        }
    };
    styled_link_at_x(&cell.styled, range, x - text_x, face, advance)
}

fn nearest_block_index(blocks: &[ReadBlock], y: f32) -> Option<usize> {
    if blocks.is_empty() {
        return None;
    }
    let idx = blocks
        .partition_point(|block| block.bottom < y)
        .min(blocks.len().saturating_sub(1));
    if idx > 0 && y < blocks[idx].top {
        let previous = &blocks[idx - 1];
        if y - previous.bottom <= blocks[idx].top - y {
            return Some(idx - 1);
        }
    }
    Some(idx)
}

fn markdown_read_code_block_at_if_hover_valid(
    hover_valid: bool,
    layout: &MarkdownReadLayoutCache,
    frame: (f32, f32, f32, f32),
    scroll_y: f32,
    scale: f32,
    mouse_x: f32,
    mouse_y: f32,
) -> Option<usize> {
    hover_valid
        .then(|| markdown_read_code_block_at(layout, frame, scroll_y, scale, mouse_x, mouse_y))
        .flatten()
}

fn markdown_read_code_block_at(
    layout: &MarkdownReadLayoutCache,
    frame: (f32, f32, f32, f32),
    scroll_y: f32,
    scale: f32,
    mouse_x: f32,
    mouse_y: f32,
) -> Option<usize> {
    let (frame_x, frame_y, frame_w, frame_h) = frame;
    if mouse_x < frame_x
        || mouse_x > frame_x + frame_w
        || mouse_y < frame_y
        || mouse_y > frame_y + frame_h
    {
        return None;
    }

    let doc_y = mouse_y - frame_y + scroll_y;
    let block_idx = layout.blocks.partition_point(|block| block.bottom < doc_y);
    let block = layout.blocks.get(block_idx)?;
    if doc_y < block.top || doc_y > block.bottom {
        return None;
    }
    let ReadBlockKind::Code(code) = &block.kind else {
        return None;
    };
    let left = frame_x + code.x;
    let right = frame_x + frame_w - CONTENT_PAD * scale;
    (mouse_x >= left && mouse_x <= right).then_some(block.source_range.start)
}

fn line_box_index<T>(
    items: &[T],
    y: f32,
    top: impl Fn(&T) -> f32,
    bottom: impl Fn(&T) -> f32,
) -> Option<usize> {
    if items.is_empty() {
        return None;
    }
    let idx = items
        .partition_point(|item| bottom(item) <= y)
        .min(items.len());
    if idx == 0 {
        return Some(0);
    }
    if idx == items.len() {
        return Some(items.len() - 1);
    }
    if y >= top(&items[idx]) {
        return Some(idx);
    }
    let before = idx - 1;
    if y - bottom(&items[before]) <= top(&items[idx]) - y {
        Some(before)
    } else {
        Some(idx)
    }
}

fn uniform_line_box_index(
    line_count: usize,
    first_top: f32,
    line_height: f32,
    y: f32,
) -> Option<usize> {
    if line_count == 0 || !line_height.is_finite() || line_height <= 0.0 {
        return None;
    }
    let idx = ((y - first_top) / line_height).floor() as isize;
    Some(idx.clamp(0, line_count.saturating_sub(1) as isize) as usize)
}

fn source_highlight_vertical_bounds(line_top: f32, line_height: f32) -> (f32, f32) {
    (
        (line_top.round() + 2.0).round(),
        line_height.round().max(1.0),
    )
}

fn styled_source_boundary(styled: &StyledText, visual: usize) -> Option<usize> {
    let next_idx = styled.runs.partition_point(|run| run.range.start < visual);
    if let Some(run) = styled.runs.get(next_idx)
        && run.range.start == visual
        && let Some(source_range) = run.source_range.as_ref()
    {
        return Some(source_range.start);
    }
    let idx = styled.runs.partition_point(|run| run.range.end < visual);
    if let Some(run) = styled.runs.get(idx)
        && run.range.start <= visual
        && visual <= run.range.end
        && let Some(source_range) = run.source_range.as_ref()
    {
        return Some(
            source_range.start + visual.saturating_sub(run.range.start).min(source_range.len()),
        );
    }
    styled.runs[..idx]
        .iter()
        .rev()
        .find_map(|run| run.source_range.as_ref().map(|range| range.end))
        .or_else(|| {
            styled.runs[idx..]
                .iter()
                .find_map(|run| run.source_range.as_ref().map(|range| range.start))
        })
}

fn visual_byte_at_x<F>(
    text: &str,
    source_start: usize,
    target_x: f32,
    mut metrics_for: F,
) -> usize
where
    F: FnMut(usize, char) -> VisualCharMetrics,
{
    if target_x <= 0.0 {
        return source_start;
    }
    let mut x = 0.0;
    let mut byte = source_start;
    for ch in text.chars() {
        let metrics = metrics_for(byte, ch);
        let leading = metrics.leading.max(0.0);
        let advance = metrics.advance.max(0.0);
        let trailing = metrics.trailing.max(0.0);
        let next_byte = byte + ch.len_utf8();

        x += leading;
        if advance > 0.0 {
            if target_x <= x + advance * 0.5 {
                return byte;
            }
            x += advance;
            if target_x <= x + trailing {
                return next_byte;
            }
        }
        x += trailing;
        byte = next_byte;
    }
    source_start + text.len()
}

#[derive(Clone, Copy)]
struct ReadHighlights<'a> {
    selection: Option<&'a Range<usize>>,
    search_results: &'a [(usize, usize)],
    search_current_idx: Option<usize>,
}

impl ReadHighlights<'_> {
    fn is_empty(self) -> bool {
        self.selection.is_none() && self.search_results.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StyledRunPaintLayer {
    InlineCodeBackground,
    SourceHighlights,
    Glyphs,
    LinkUnderline,
}

const STYLED_RUN_PAINT_ORDER: [StyledRunPaintLayer; 4] = [
    StyledRunPaintLayer::InlineCodeBackground,
    StyledRunPaintLayer::SourceHighlights,
    StyledRunPaintLayer::Glyphs,
    StyledRunPaintLayer::LinkUnderline,
];

fn styled_run_layer_enabled(
    layer: StyledRunPaintLayer,
    style: TextStyle,
    has_highlights: bool,
) -> bool {
    match layer {
        StyledRunPaintLayer::InlineCodeBackground => style.contains(TextStyle::CODE),
        StyledRunPaintLayer::SourceHighlights => has_highlights,
        StyledRunPaintLayer::Glyphs => true,
        StyledRunPaintLayer::LinkUnderline => style.contains(TextStyle::LINK),
    }
}

fn search_highlight_color(highlights: ReadHighlights<'_>, search_idx: usize) -> [f32; 4] {
    if highlights.search_current_idx == Some(search_idx) {
        crate::render_view::SEARCH_ACTIVE_HIGHLIGHT_COLOR
    } else {
        crate::render_view::SEARCH_HIGHLIGHT_COLOR
    }
}

impl Renderer {
    pub(crate) fn markdown_read_code_block_at(
        &self,
        markdown: &MarkdownTabState,
        editor_version: u64,
        frame: (f32, f32, f32, f32),
        scroll_y: f32,
        mouse_x: f32,
        mouse_y: f32,
    ) -> Option<usize> {
        if markdown.mode != MarkdownMode::Read
            || markdown.read_layout.key?.version != editor_version
        {
            return None;
        }
        markdown_read_code_block_at(
            &markdown.read_layout,
            frame,
            scroll_y.round(),
            self.scale_factor,
            mouse_x,
            mouse_y,
        )
    }

    /// Index of the link under the pointer in the Reader `frame`, if the layout is current.
    pub(crate) fn markdown_read_link_at(
        &mut self,
        markdown: &MarkdownTabState,
        editor_version: u64,
        frame: (f32, f32, f32, f32),
        scroll_y: f32,
        mouse_x: f32,
        mouse_y: f32,
    ) -> Option<u32> {
        let (frame_x, frame_y, frame_w, frame_h) = frame;
        if markdown.mode != MarkdownMode::Read
            || markdown.read_layout.key?.version != editor_version
            || mouse_x < frame_x
            || mouse_x > frame_x + frame_w
            || mouse_y < frame_y
            || mouse_y > frame_y + frame_h
        {
            return None;
        }
        let doc_y = mouse_y - frame_y + scroll_y.round();
        let scale = self.scale_factor;
        let mut advance = |c: char, mono: bool, final_size: Option<f32>| {
            self.markdown_read_char_advance(c, mono, final_size)
        };
        markdown
            .read_layout
            .link_at(mouse_x - frame_x, doc_y, scale, &mut advance)
    }

    /// Refreshes `markdown.hovered_link` for the pointer and draws the destination tooltip once
    /// the pointer has rested on the link. Called at the end of the Reader draw.
    fn update_markdown_read_link_hover(
        &mut self,
        markdown: &mut MarkdownTabState,
        editor_version: u64,
        frame: (f32, f32, f32, f32),
        scroll_y: f32,
        ui_registry: &UiRegistry,
    ) {
        const LINK_TOOLTIP_NAMESPACE: u64 = 1u64 << 58;
        const LINK_TOOLTIP_MAX_CHARS: usize = 160;
        let (mouse_x, mouse_y) = (self.last_mouse_x, self.last_mouse_y);
        let over_body = ui_registry.hovered() == Some(crate::ui_system::UiId::MarkdownReadBody)
            && !markdown.read_selecting;
        let link = over_body
            .then(|| self.markdown_read_link_at(markdown, editor_version, frame, scroll_y, mouse_x, mouse_y))
            .flatten();
        markdown.hovered_link = link;
        let text = link
            .and_then(|index| markdown.read_layout.links().get(index as usize))
            .and_then(|target| link_tooltip(target, markdown.read_layout.doc_dir()));
        let (Some(index), Some(text)) = (link, text) else {
            self.reset_delayed_tooltip_anchor_namespace(LINK_TOOLTIP_NAMESPACE);
            self.markdown_link_tooltip_waiting = false;
            return;
        };
        let anchor = self.delayed_tooltip_anchor(
            Some(LINK_TOOLTIP_NAMESPACE | u64::from(index)),
            mouse_x,
            mouse_y,
            std::time::Instant::now(),
        );
        self.markdown_link_tooltip_waiting = anchor.is_none();
        if let Some((anchor_x, anchor_y)) = anchor
            && !self.hide_popups_until_mouse_move
        {
            let text: String = if text.chars().count() > LINK_TOOLTIP_MAX_CHARS {
                text.chars().take(LINK_TOOLTIP_MAX_CHARS).chain(std::iter::once('…')).collect()
            } else {
                text
            };
            let scale = self.scale_factor;
            self.draw_tab_tooltip(&text, anchor_x, anchor_y + 12.0 * scale, scale);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_markdown_code_copy_action(
        &mut self,
        block: &ReadBlock,
        frame_x: f32,
        offset_y: f32,
        content_w: f32,
        copied: bool,
        ui_registry: &mut UiRegistry,
    ) {
        let ReadBlockKind::Code(code) = &block.kind else {
            return;
        };
        let left = frame_x + code.x;
        let right = frame_x + content_w - CONTENT_PAD * self.scale_factor;
        let header = code_header_geometry(
            left,
            right,
            block.top + offset_y,
            self.scale_factor,
        );
        let button = IconButton {
            x: header.button_x,
            y: header.button_y,
            size: header.button_size,
            icon: Some(if copied { IconType::Check } else { IconType::Copy }),
            is_active: false,
            icon_size: Some(header.button_icon_size),
            active_square_width: None,
            custom_color: copied.then_some([0.3, 0.9, 0.4, 1.0]),
        };
        ui_registry.register_icon_button(
            crate::ui_system::UiId::MarkdownCodeCopy(block.source_range.start),
            &button,
            self,
            self.last_mouse_x,
            self.last_mouse_y,
            self.scale_factor,
            false,
        );
    }

    pub(crate) fn markdown_read_source_byte_at(
        &mut self,
        markdown: &MarkdownTabState,
        editor_version: u64,
        frame: (f32, f32, f32, f32),
        scroll_y: f32,
        mouse_x: f32,
        mouse_y: f32,
    ) -> Option<usize> {
        if markdown.mode != MarkdownMode::Read
            || markdown.read_layout.key?.version != editor_version
            || markdown.read_layout.blocks.is_empty()
        {
            return None;
        }

        let (frame_x, frame_y, frame_w, frame_h) = frame;
        let scroll_y = scroll_y.round();
        let doc_y = (mouse_y.clamp(frame_y, frame_y + frame_h) - frame_y + scroll_y)
            .clamp(0.0, markdown.read_layout.content_height.max(0.0));
        let block_idx = nearest_block_index(&markdown.read_layout.blocks, doc_y)?;
        let block = &markdown.read_layout.blocks[block_idx];

        match &block.kind {
            ReadBlockKind::Text(text) => {
                let line_idx =
                    line_box_index(&text.lines, doc_y, |line| line.top, |line| line.bottom)?;
                let line = &text.lines[line_idx];
                let local_x = mouse_x - (frame_x + text.x);
                let visual = self.styled_visual_offset_at_x(
                    &text.styled,
                    &line.range,
                    local_x,
                    text.scale,
                    text.mono,
                    text.heading_level.map(|_| text.scale),
                );
                styled_source_boundary(&text.styled, visual)
                    .or_else(|| Some(block.source_range.start))
            }
            ReadBlockKind::Code(code) => {
                let line_idx =
                    line_box_index(&code.lines, doc_y, |line| line.top, |line| line.bottom)?;
                let line = &code.lines[line_idx];
                let pad = code_block_padding(self.scale_factor);
                let local_x = mouse_x - (frame_x + code.x + pad)
                    + code_block_scroll_offset(
                        markdown.code_scroll_x(block.source_range.start),
                        code,
                        frame_w.max(1.0),
                        self.scale_factor,
                    );
                let text = markdown
                    .read_source
                    .get(line.source_range.clone())
                    .unwrap_or("");
                Some(self.mono_source_byte_at_x(text, line.source_range.start, local_x, 1.0))
            }
            ReadBlockKind::Table(table) => {
                if table.rows.is_empty() {
                    return Some(block.source_range.start);
                }
                let row_idx = table
                    .rows
                    .partition_point(|row| row.y + row.h <= doc_y)
                    .min(table.rows.len().saturating_sub(1));
                let row = &table.rows[row_idx];
                if row.cells.is_empty() {
                    return Some(row.source_range.start);
                }
                let doc_x = mouse_x - frame_x;
                let col = ((doc_x - table.x) / table.cell_width.max(1.0))
                    .floor()
                    .max(0.0) as usize;
                let col = col.min(row.cells.len().saturating_sub(1));
                let cell = &row.cells[col];
                if cell.lines.is_empty() {
                    return Some(row.source_range.start);
                }
                let line_idx = uniform_line_box_index(
                    cell.lines.len(),
                    row.y + table.cell_padding,
                    table.line_height,
                    doc_y,
                )?;
                let range = &cell.lines[line_idx];
                let measured = self.measure_styled_fragment(&cell.styled, range, 0.82);
                let cell_x = frame_x + table.x + col as f32 * table.cell_width;
                let tx = match cell.alignment {
                    MarkdownTableAlignment::Center => {
                        cell_x + (table.cell_width - measured) * 0.5
                    }
                    MarkdownTableAlignment::Right => {
                        cell_x + table.cell_width - table.cell_padding - measured
                    }
                    _ => cell_x + table.cell_padding,
                };
                let visual = self.styled_visual_offset_at_x(
                    &cell.styled,
                    range,
                    mouse_x - tx,
                    0.82,
                    false,
                    None,
                );
                styled_source_boundary(&cell.styled, visual)
                    .or_else(|| Some(row.source_range.start))
            }
            ReadBlockKind::Rule { .. } | ReadBlockKind::Media { .. } => {
                Some(block.source_range.start)
            }
        }
    }

    fn styled_visual_offset_at_x(
        &mut self,
        styled: &StyledText,
        range: &Range<usize>,
        target_x: f32,
        text_scale: f32,
        mono: bool,
        final_size_scale: Option<f32>,
    ) -> usize {
        let Some(text) = styled.text.get(range.clone()) else {
            return range.start;
        };
        let face = TextFace {
            text_scale,
            layout_scale: self.scale_factor,
            mono,
            final_size_scale,
        };
        visual_byte_at_x(text, range.start, target_x, |byte, ch| {
            let mut advance = |c: char, use_mono: bool, final_size: Option<f32>| {
                self.markdown_read_char_advance(c, use_mono, final_size)
            };
            styled_metrics_at(styled, byte, ch, face, &mut advance)
        })
    }

    fn mono_source_byte_at_x(
        &mut self,
        text: &str,
        source_start: usize,
        target_x: f32,
        scale: f32,
    ) -> usize {
        visual_byte_at_x(text, source_start, target_x, |_, ch| {
            VisualCharMetrics::glyph(mono_char_pixel_advance(ch, scale, || self.char_advance(ch)))
        })
    }
}

impl Renderer {
    #[allow(clippy::too_many_arguments)]
    fn draw_styled_fragment(
        &mut self,
        styled: &StyledText,
        range: &Range<usize>,
        mut x: f32,
        y: f32,
        line_top: f32,
        scale: f32,
        bold: bool,
        line_height: f32,
        highlights: ReadHighlights<'_>,
        final_size_scale: Option<f32>,
    ) {
        let pad = inline_code_padding_x(self.scale_factor);
        let has_highlights = !highlights.is_empty();
        let pixel_size = final_size_scale.map(|scale| self.final_text_pixel_size(scale));
        let visible_runs = visible_styled_run_range(&styled.runs, range);
        for run in &styled.runs[visible_runs] {
            let start = run.range.start.max(range.start);
            let end = run.range.end.min(range.end);
            if start >= end {
                continue;
            }
            let Some(text) = styled.text.get(start..end) else {
                continue;
            };
            let style = run.style;
            let inline_code = style.contains(TextStyle::CODE);
            let left_pad = if inline_code && start == run.range.start {
                pad
            } else {
                0.0
            };
            let right_pad = if inline_code && end == run.range.end {
                pad
            } else {
                0.0
            };
            let text_width = if let Some(pixel_size) = pixel_size {
                if inline_code {
                    self.measure_mono_width_at_pixel_size(text, pixel_size)
                } else {
                    self.measure_ui_width_at_pixel_size(text, pixel_size)
                }
            } else if inline_code {
                self.measure_mono_width_pixel_snapped(text, scale)
            } else {
                self.measure_ui_width(text, scale)
            };
            let width = left_pad + text_width + right_pad;
            let text_x = x + left_pad;
            let color = markdown_text_color(style, self.theme.fg);
            for layer in STYLED_RUN_PAINT_ORDER {
                if !styled_run_layer_enabled(layer, style, has_highlights) {
                    continue;
                }
                match layer {
                    StyledRunPaintLayer::InlineCodeBackground => {
                        let (bg_y, bg_h) = inline_code_vertical_bounds(y, self.scale_factor, scale);
                        self.push_rounded_rect(
                            x,
                            bg_y,
                            width,
                            bg_h,
                            3.0 * self.scale_factor,
                            inline_code_background(self.theme.bg, self.theme.fg),
                        );
                    }
                    StyledRunPaintLayer::SourceHighlights => {
                        self.draw_styled_run_source_highlights(
                            styled,
                            run,
                            start,
                            end,
                            x,
                            line_top,
                            scale,
                            false,
                            line_height,
                            highlights,
                            final_size_scale,
                        );
                    }
                    StyledRunPaintLayer::Glyphs => {
                        if let Some(pixel_size) = pixel_size {
                            if inline_code {
                                self.draw_string_mono_at_pixel_size(
                                    text, text_x, y, color, pixel_size, bold,
                                );
                            } else {
                                self.draw_string_at_pixel_size_weighted(
                                    text, text_x, y, color, pixel_size, bold,
                                );
                            }
                        } else if inline_code {
                            self.draw_string_mono_scaled_pixel_snapped(
                                text,
                                text_x,
                                y,
                                color,
                                scale,
                                bold,
                            );
                        } else {
                            self.draw_string_scaled_pixel_snapped_weighted(
                                text,
                                text_x,
                                y,
                                color,
                                scale,
                                bold,
                            );
                        }
                    }
                    StyledRunPaintLayer::LinkUnderline => {
                        self.push_rect(
                            x,
                            y + 2.0 * self.scale_factor,
                            width,
                            1.0,
                            faded(color, 0.65),
                        );
                    }
                }
            }
            x += width;
        }
    }

    fn draw_styled_run_source_highlights(
        &mut self,
        styled: &StyledText,
        run: &StyledRun,
        start: usize,
        end: usize,
        mut x: f32,
        line_top: f32,
        text_scale: f32,
        mono: bool,
        line_height: f32,
        highlights: ReadHighlights<'_>,
        final_size_scale: Option<f32>,
    ) -> f32 {
        if highlights.is_empty() {
            return 0.0;
        }
        let Some(text) = styled.text.get(start..end) else {
            return 0.0;
        };
        let (top, highlight_height) = source_highlight_vertical_bounds(line_top, line_height);
        let start_x = x;
        let mut byte = start;
        let advance_scale = if final_size_scale.is_some() {
            1.0
        } else {
            text_scale
        };
        for ch in text.chars() {
            let width = {
                let scale = self.scale_factor;
                let mut advance = |c: char, use_mono: bool| {
                    self.markdown_read_char_advance(c, use_mono, final_size_scale)
                };
                styled_char_advance(
                    styled,
                    byte,
                    ch,
                    advance_scale,
                    scale,
                    mono,
                    &mut advance,
                )
            };
            if let Some(source_range) = run.source_range.as_ref() {
                let source_start = source_range.start + byte.saturating_sub(run.range.start);
                let source_end = source_start + ch.len_utf8();
                if width > 0.0 {
                    self.draw_source_highlight_rects(
                        source_start..source_end,
                        x,
                        top,
                        width,
                        highlight_height,
                        highlights,
                    );
                }
            }
            x += width;
            byte += ch.len_utf8();
        }
        x - start_x
    }

    fn draw_styled_source_highlights(
        &mut self,
        styled: &StyledText,
        range: &Range<usize>,
        mut x: f32,
        line_top: f32,
        text_scale: f32,
        mono: bool,
        line_height: f32,
        highlights: ReadHighlights<'_>,
        final_size_scale: Option<f32>,
    ) {
        if highlights.is_empty() {
            return;
        }
        let visible_runs = visible_styled_run_range(&styled.runs, range);
        for run in &styled.runs[visible_runs] {
            let start = run.range.start.max(range.start);
            let end = run.range.end.min(range.end);
            if start >= end {
                continue;
            }
            x += self.draw_styled_run_source_highlights(
                styled,
                run,
                start,
                end,
                x,
                line_top,
                text_scale,
                mono,
                line_height,
                highlights,
                final_size_scale,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_mono_source_highlights(
        &mut self,
        text: &str,
        source_start: usize,
        mut x: f32,
        line_top: f32,
        line_height: f32,
        scale: f32,
        highlights: ReadHighlights<'_>,
    ) {
        if highlights.is_empty() {
            return;
        }
        let (top, highlight_height) = source_highlight_vertical_bounds(line_top, line_height);
        let mut source_byte = source_start;
        for ch in text.chars() {
            let width = mono_char_pixel_advance(ch, scale, || self.char_advance(ch));
            let end = source_byte + ch.len_utf8();
            if width > 0.0 {
                self.draw_source_highlight_rects(
                    source_byte..end,
                    x,
                    top,
                    width,
                    highlight_height,
                    highlights,
                );
            }
            x += width;
            source_byte = end;
        }
    }

    fn draw_source_highlight_rects(
        &mut self,
        source_range: Range<usize>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        highlights: ReadHighlights<'_>,
    ) {
        let search_idx = highlights
            .search_results
            .partition_point(|&(_, end)| end <= source_range.start);
        if let Some(&(start, end)) = highlights.search_results.get(search_idx)
            && start < source_range.end
            && source_range.start < end
        {
            self.push_rect(
                x,
                y,
                width.max(1.0),
                height.max(1.0),
                search_highlight_color(highlights, search_idx),
            );
        }
        if highlights
            .selection
            .is_some_and(|selection| ranges_overlap(selection, &source_range))
        {
            self.push_rect(x, y, width.max(1.0), height.max(1.0), self.theme.sel);
        }
    }
}

#[cfg(test)]
pub(crate) fn build_test_markdown_read_layout(
    source: &str,
    width: f32,
) -> MarkdownReadLayoutCache {
    build_test_markdown_read_layout_with_media(source, width, 1.0, None)
}

/// Same, with media blocks: `media` is the cache to read and the folder of the document.
#[cfg(test)]
pub(crate) fn build_test_markdown_read_layout_with_media(
    source: &str,
    width: f32,
    scale: f32,
    media: Option<(&MarkdownMedia, &std::path::Path)>,
) -> MarkdownReadLayoutCache {
    let document = crate::languages::markdown::MarkdownParseState::default()
        .parse(source)
        .expect("markdown parse");
    let input = media.map(|(m, dir)| MediaInput::new(&document, source, dir, m));
    let dir = media.map_or_else(std::path::PathBuf::new, |(_, dir)| dir.to_path_buf());
    let mut builder =
        LayoutBuilder::new(source, width, scale, test_layout_text_metrics(scale), |_, _, _| 8.0 * scale)
            .with_media(input)
            .with_links(&dir, document.link_definitions(source));
    builder.append_blocks(&document.blocks, 0.0, 0, None);
    let (blocks, content_height, links) = builder.finish_with_links();
    let mut cache = MarkdownReadLayoutCache::default();
    cache.replace_layout_with_links(
        LayoutKey::new(1, width, scale, 16.0),
        blocks,
        content_height,
        source.len(),
        links,
    );
    cache.media_gen = media.map_or(0, |(m, _)| m.media_gen());
    cache
}

include!("markdown_read_interaction_tests.rs");
