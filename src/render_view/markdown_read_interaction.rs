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
                ReadBlockKind::Rule { .. } => {}
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
            ReadBlockKind::Rule { .. } => Some(block.source_range.start),
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
        let layout_scale = self.scale_factor;
        let advance_scale = if final_size_scale.is_some() {
            1.0
        } else {
            text_scale
        };
        visual_byte_at_x(text, range.start, target_x, |byte, ch| {
            let mut advance = |c: char, use_mono: bool| {
                self.markdown_read_char_advance(c, use_mono, final_size_scale)
            };
            styled_char_metrics(
                styled,
                byte,
                ch,
                advance_scale,
                layout_scale,
                mono,
                &mut advance,
            )
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
    let document = crate::languages::markdown::MarkdownParseState::default()
        .parse(source)
        .expect("markdown parse");
    let mut builder = LayoutBuilder::new(source, width, 1.0, test_layout_text_metrics(1.0), |_, _, _| 8.0);
    builder.append_blocks(&document.blocks, 0.0, 0, None);
    let (blocks, content_height) = builder.finish();
    let mut cache = MarkdownReadLayoutCache::default();
    cache.replace_layout(
        LayoutKey::new(1, width, 1.0, 16.0),
        blocks,
        content_height,
        source.len(),
    );
    cache
}

#[cfg(test)]
mod interaction_tests {
    use super::*;

    fn layout(source: &str, width: f32) -> MarkdownReadLayoutCache {
        build_test_markdown_read_layout(source, width)
    }

    fn mapped_styled_source_byte(
        styled: &StyledText,
        range: &Range<usize>,
        target_x: f32,
        text_scale: f32,
        layout_scale: f32,
        raw_advance: f32,
    ) -> usize {
        let text = styled.text.get(range.clone()).expect("styled range");
        let visual = visual_byte_at_x(text, range.start, target_x, |byte, ch| {
            let mut advance = |_ch: char, _mono: bool| raw_advance;
            styled_char_metrics(
                styled,
                byte,
                ch,
                text_scale,
                layout_scale,
                false,
                &mut advance,
            )
        });
        styled_source_boundary(styled, visual).expect("source boundary")
    }

    fn paint_layers(style: TextStyle, highlights: ReadHighlights<'_>) -> Vec<StyledRunPaintLayer> {
        STYLED_RUN_PAINT_ORDER
            .into_iter()
            .filter(|&layer| styled_run_layer_enabled(layer, style, !highlights.is_empty()))
            .collect()
    }

    fn paragraph_inline_code_style(cache: &MarkdownReadLayoutCache) -> TextStyle {
        cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Text(text) => text
                    .styled
                    .runs
                    .iter()
                    .find(|run| run.style.contains(TextStyle::CODE))
                    .map(|run| run.style),
                _ => None,
            })
            .expect("paragraph inline-code run")
    }

    #[test]
    fn inline_code_selection_paints_background_then_highlight_then_glyphs() {
        let source = "text `inline-code` tail\n";
        let cache = layout(source, 420.0);
        let start = source.find("inline-code").expect("inline source");
        let selection = start..start + "inline-code".len();
        let highlights = ReadHighlights {
            selection: Some(&selection),
            search_results: &[],
            search_current_idx: None,
        };

        assert_eq!(
            paint_layers(paragraph_inline_code_style(&cache), highlights),
            vec![
                StyledRunPaintLayer::InlineCodeBackground,
                StyledRunPaintLayer::SourceHighlights,
                StyledRunPaintLayer::Glyphs,
            ]
        );
    }

    #[test]
    fn inline_code_search_paints_above_background_and_keeps_active_color() {
        let source = "text `inline-code` tail\n";
        let cache = layout(source, 420.0);
        let start = source.find("inline-code").expect("inline source");
        let search = [(start, start + "inline-code".len())];
        let highlights = ReadHighlights {
            selection: None,
            search_results: &search,
            search_current_idx: Some(0),
        };

        assert_eq!(
            paint_layers(paragraph_inline_code_style(&cache), highlights),
            vec![
                StyledRunPaintLayer::InlineCodeBackground,
                StyledRunPaintLayer::SourceHighlights,
                StyledRunPaintLayer::Glyphs,
            ]
        );
        assert_eq!(
            search_highlight_color(highlights, 0),
            crate::render_view::SEARCH_ACTIVE_HIGHLIGHT_COLOR
        );
    }

    #[test]
    fn table_inline_code_uses_same_background_highlight_glyph_order() {
        let source = "| cell |\n| --- |\n| `inline-code` |\n";
        let cache = layout(source, 420.0);
        let style = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Table(table) => table
                    .rows
                    .iter()
                    .flat_map(|row| row.cells.iter())
                    .flat_map(|cell| cell.styled.runs.iter())
                    .find(|run| run.style.contains(TextStyle::CODE))
                    .map(|run| run.style),
                _ => None,
            })
            .expect("table inline-code run");
        let start = source.find("inline-code").expect("inline source");
        let search = [(start, start + "inline-code".len())];
        let highlights = ReadHighlights {
            selection: None,
            search_results: &search,
            search_current_idx: None,
        };

        assert_eq!(
            paint_layers(style, highlights),
            vec![
                StyledRunPaintLayer::InlineCodeBackground,
                StyledRunPaintLayer::SourceHighlights,
                StyledRunPaintLayer::Glyphs,
            ]
        );
    }

    #[test]
    fn normal_text_and_no_highlight_fast_path_skip_unneeded_layers() {
        let selection = 0..1;
        let selection_highlights = ReadHighlights {
            selection: Some(&selection),
            search_results: &[],
            search_current_idx: None,
        };
        let none = ReadHighlights {
            selection: None,
            search_results: &[],
            search_current_idx: None,
        };
        let code = TextStyle::default().with(TextStyle::CODE);

        assert_eq!(
            paint_layers(TextStyle::default(), selection_highlights),
            vec![
                StyledRunPaintLayer::SourceHighlights,
                StyledRunPaintLayer::Glyphs,
            ]
        );
        assert_eq!(
            paint_layers(code, none),
            vec![
                StyledRunPaintLayer::InlineCodeBackground,
                StyledRunPaintLayer::Glyphs,
            ]
        );
    }

    #[test]
    fn reader_selection_mapping_copies_visible_text_without_markdown_punctuation() {
        let source = "# Заголовок 😀\n\nТекст **strong** и *emphasis* с `code λ`.\n\n```rust\nlet x = 1;\nprintln!(\"λ\");\n```\n";
        let cache = layout(source, 520.0);
        let copied = cache.copy_source_selection(source, &(0..source.len()));

        assert!(copied.contains("Заголовок 😀"));
        assert!(copied.contains("Текст strong и emphasis с code λ."));
        assert!(copied.contains("let x = 1;\nprintln!(\"λ\");"));
        assert!(!copied.contains("# "));
        assert!(!copied.contains("**"));
        assert!(!copied.contains('`'));
    }

    #[test]
    fn reader_visual_mapping_skips_hidden_markdown_syntax_and_link_destination() {
        let source = "**bold** **жир** _курсив_ [label](https://example.com) and `code`\n";
        let cache = layout(source, 520.0);
        let styled = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Text(text) => Some(&text.styled),
                _ => None,
            })
            .expect("paragraph");

        assert!(!styled.text.contains("https://"));
        assert!(!styled.text.contains("**"));
        assert!(!styled.text.contains('`'));
        for needle in ["bold", "жир", "курсив", "label", "code"] {
            let visual = styled.text.find(needle).expect("visual needle");
            let source_byte = source.find(needle).expect("source needle");
            for (relative, _) in needle.char_indices() {
                assert_eq!(
                    styled_source_boundary(styled, visual + relative),
                    Some(source_byte + relative)
                );
            }
        }
    }

    #[test]
    fn inline_code_hit_test_uses_drawn_glyph_midpoint_not_padded_cell_midpoint() {
        let mut styled = StyledText::default();
        styled.push(
            "ab",
            TextStyle::default().with(TextStyle::CODE),
            Some(40..42),
        );
        let range = 0..styled.text.len();
        let mut advance = |_ch: char, _mono: bool| 8.0;
        let first = styled_char_metrics(&styled, 0, 'a', 1.0, 1.0, false, &mut advance);
        let second = styled_char_metrics(&styled, 1, 'b', 1.0, 1.0, false, &mut advance);

        assert_eq!(first, VisualCharMetrics { leading: 4.0, advance: 8.0, trailing: 0.0 });
        assert_eq!(second, VisualCharMetrics { leading: 0.0, advance: 8.0, trailing: 4.0 });
        let first_midpoint = first.leading + first.advance * 0.5;
        assert_eq!(first_midpoint, 8.0);
        assert_eq!(mapped_styled_source_byte(&styled, &range, 7.0, 1.0, 1.0, 8.0), 40);
        assert_eq!(mapped_styled_source_byte(&styled, &range, 9.0, 1.0, 1.0, 8.0), 41);

        let second_left = first.width() + second.leading;
        let second_midpoint = second_left + second.advance * 0.5;
        assert_eq!(second_midpoint, 16.0);
        assert_eq!(mapped_styled_source_byte(&styled, &range, 15.0, 1.0, 1.0, 8.0), 41);
        assert_eq!(mapped_styled_source_byte(&styled, &range, 17.0, 1.0, 1.0, 8.0), 42);
        assert_eq!(mapped_styled_source_byte(&styled, &range, -5.0, 1.0, 1.0, 8.0), 40);
        assert_eq!(mapped_styled_source_byte(&styled, &range, 30.0, 1.0, 1.0, 8.0), 42);
    }

    #[test]
    fn inline_code_hit_test_matches_wrapped_run_geometry_at_fractional_scales() {
        let mut styled = StyledText::default();
        styled.push(
            "abcd",
            TextStyle::default().with(TextStyle::CODE),
            Some(100..104),
        );
        let first_line = 0..2;
        let second_line = 2..4;

        for layout_scale in [1.25, 1.5, 1.75] {
            let raw_advance = 8.0 * layout_scale;
            let mut advance = |_ch: char, _mono: bool| raw_advance;
            let b = styled_char_metrics(
                &styled,
                1,
                'b',
                BODY_SCALE,
                layout_scale,
                false,
                &mut advance,
            );
            let c = styled_char_metrics(
                &styled,
                2,
                'c',
                BODY_SCALE,
                layout_scale,
                false,
                &mut advance,
            );
            let d = styled_char_metrics(
                &styled,
                3,
                'd',
                BODY_SCALE,
                layout_scale,
                false,
                &mut advance,
            );
            let pad = inline_code_padding_x(layout_scale);
            assert_eq!(b.trailing, 0.0, "soft wrap must not invent right padding");
            assert_eq!(c.leading, 0.0, "continuation line must not invent left padding");
            assert_eq!(d.trailing, pad);

            let c_midpoint = c.advance * 0.5;
            assert_eq!(
                mapped_styled_source_byte(
                    &styled,
                    &second_line,
                    c_midpoint - 0.25,
                    BODY_SCALE,
                    layout_scale,
                    raw_advance,
                ),
                102,
            );
            assert_eq!(
                mapped_styled_source_byte(
                    &styled,
                    &second_line,
                    c_midpoint + 0.25,
                    BODY_SCALE,
                    layout_scale,
                    raw_advance,
                ),
                103,
            );

            let first_a = styled_char_metrics(
                &styled,
                0,
                'a',
                BODY_SCALE,
                layout_scale,
                false,
                &mut advance,
            );
            assert_eq!(first_a.leading, pad);
            let a_midpoint = pad + first_a.advance * 0.5;
            assert_eq!(
                mapped_styled_source_byte(
                    &styled,
                    &first_line,
                    a_midpoint - 0.25,
                    BODY_SCALE,
                    layout_scale,
                    raw_advance,
                ),
                100,
            );
            assert_eq!(
                mapped_styled_source_byte(
                    &styled,
                    &first_line,
                    a_midpoint + 0.25,
                    BODY_SCALE,
                    layout_scale,
                    raw_advance,
                ),
                101,
            );
        }
    }

    #[test]
    fn table_inline_code_hit_test_keeps_midpoint_after_alignment_translation() {
        let mut styled = StyledText::default();
        styled.push(
            "ab",
            TextStyle::default().with(TextStyle::CODE),
            Some(200..202),
        );
        let range = 0..2;
        let text_scale = 0.82;

        for layout_scale in [1.25, 1.5, 1.75] {
            let raw_advance = 8.0 * layout_scale;
            let mut advance = |_ch: char, _mono: bool| raw_advance;
            let first = styled_char_metrics(
                &styled,
                0,
                'a',
                text_scale,
                layout_scale,
                false,
                &mut advance,
            );
            let second = styled_char_metrics(
                &styled,
                1,
                'b',
                text_scale,
                layout_scale,
                false,
                &mut advance,
            );
            let measured = first.width() + second.width();
            let cell_x = 100.0;
            let cell_w = 90.0 * layout_scale;
            let cell_pad = (6.0 * layout_scale).round();
            let centers = [
                cell_x + (cell_w - measured) * 0.5,
                cell_x + cell_w - cell_pad - measured,
            ];

            for tx in centers {
                let midpoint_world = tx + first.leading + first.advance * 0.5;
                for (world_x, expected) in [
                    (midpoint_world - 0.25, 200usize),
                    (midpoint_world + 0.25, 201usize),
                ] {
                    assert_eq!(
                        mapped_styled_source_byte(
                            &styled,
                            &range,
                            world_x - tx,
                            text_scale,
                            layout_scale,
                            raw_advance,
                        ),
                        expected,
                    );
                }
            }
        }
    }

    #[test]
    fn reader_visual_byte_mapping_skips_zero_width_unicode_and_keeps_tabs() {
        let text = "a\u{200D}\u{FE0F}\u{0301}\tb";
        let tab_byte = text.find('\t').expect("tab");
        let b_byte = text.find('b').expect("b");
        let mapped = |target_x: f32| {
            visual_byte_at_x(text, 100, target_x, |byte, ch| {
                let local = byte - 100;
                let advance = if text_char_is_non_rendering_control(ch) || ch == '\u{0301}' {
                    0.0
                } else if local == tab_byte {
                    16.0
                } else {
                    8.0
                };
                VisualCharMetrics::glyph(advance)
            })
        };

        assert_eq!(mapped(3.0), 100);
        assert_eq!(mapped(9.0), 100 + tab_byte);
        assert_eq!(mapped(17.0), 100 + b_byte);
        assert_eq!(mapped(40.0), 100 + text.len());
        for byte in [mapped(3.0), mapped(9.0), mapped(17.0), mapped(40.0)] {
            assert!(text.is_char_boundary(byte - 100));
        }
    }

    #[test]
    fn code_hit_test_padding_matches_draw_at_fractional_scales() {
        for scale in [1.0, 1.25, 1.3, 1.5, 1.75, 2.0] {
            assert_eq!(code_block_padding(scale), (CODE_PAD * scale).round());
        }
    }

    #[test]
    fn reader_selection_across_wrapped_blocks_does_not_copy_soft_wraps() {
        let source = "alpha beta gamma delta epsilon zeta eta theta\n\nsecond block\n";
        let cache = layout(source, 90.0);
        let first = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Text(text) => Some(text),
                _ => None,
            })
            .expect("first paragraph");
        assert!(first.lines.len() > 1, "fixture must wrap visually");

        let copied = cache.copy_source_selection(source, &(0..source.len()));
        assert_eq!(
            copied,
            "alpha beta gamma delta epsilon zeta eta theta\nsecond block"
        );
    }
    #[test]
    fn reader_table_selection_serializes_cells_without_pipe_syntax() {
        let source = "| left | right |\n| --- | --- |\n| one | `two` |\n";
        let cache = layout(source, 420.0);
        let copied = cache.copy_source_selection(source, &(0..source.len()));

        assert!(copied.contains("left\tright"));
        assert!(copied.contains("one\ttwo"));
        assert!(!copied.contains('|'));
        assert!(!copied.contains('`'));
    }
    #[test]
    fn reader_search_source_targets_cover_paragraph_inline_code_and_fenced_code() {
        let source = "# title\n\nparagraph needle\n\ninline `code-needle`\n\n```rust\nlet fenced_needle = 1;\n```\n";
        let cache = layout(source, 500.0);
        let paragraph = source.find("needle").expect("paragraph match");
        let inline = source.find("code-needle").expect("inline match");
        let fenced = source.find("fenced_needle").expect("fenced match");

        let paragraph_y = cache
            .source_target_y(&(paragraph..paragraph + "needle".len()))
            .expect("paragraph target");
        let inline_y = cache
            .source_target_y(&(inline..inline + "code-needle".len()))
            .expect("inline target");
        let fenced_y = cache
            .source_target_y(&(fenced..fenced + "fenced_needle".len()))
            .expect("fenced target");

        assert!(paragraph_y < inline_y);
        assert!(inline_y < fenced_y);
    }

    #[test]
    fn code_block_copy_payload_uses_cached_content_ranges_without_fence_or_language() {
        let source = "```rust\nlet a = 1;\nlet b = 2;\n```\n\n```\nplain\n```\n";
        let cache = layout(source, 500.0);
        let ids = cache
            .blocks
            .iter()
            .filter_map(|block| match &block.kind {
                ReadBlockKind::Code(_) => Some(block.source_range.start),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(ids.len(), 2);
        assert_eq!(
            cache.code_block_copy_text(source, ids[0]).as_deref(),
            Some("let a = 1;\nlet b = 2;\n")
        );
        assert_eq!(
            cache.code_block_copy_text(source, ids[1]).as_deref(),
            Some("plain\n")
        );
    }

    #[test]
    fn code_block_header_reserves_fixed_space_and_language_stays_left() {
        let source = "```rust\nlet a = 1;\nlet b = 2;\n```\n";
        let cache = layout(source, 520.0);
        let (block, code) = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Code(code) => Some((block, code)),
                _ => None,
            })
            .expect("code block");
        let pad = code_block_padding(1.0);
        let header_h = code_header_height(1.0);
        let baseline_offset = (code.line_height * 0.82).round();
        let first_content_y = code.lines.first().expect("code line").y - baseline_offset;
        assert_eq!(first_content_y, block.top + pad + header_h);
        assert_eq!(
            block.bottom - block.top,
            pad * 2.0 + header_h + code.lines.len() as f32 * code.line_height
        );

        let left = code.x;
        let right = 520.0 - CONTENT_PAD;
        let header = code_header_geometry(left, right, block.top, 1.0);
        assert_eq!(header.language_x, left + pad);
        assert_eq!(header.button_size, CODE_ACTION_SIZE);
        assert_eq!(header.button_icon_size, CODE_ACTION_ICON_SIZE);
        assert!(header.button_x > header.language_x);
        assert!(header.language_max_w > 0.0);
        assert!(
            header.language_x + header.language_max_w
                <= header.button_x - CODE_LANGUAGE_COPY_GAP
        );
        assert!(header.button_y >= block.top + pad);
        assert!(header.button_y + header.button_size <= block.top + pad + header_h);
    }

    #[test]
    fn code_header_language_and_copy_geometry_never_overlap_at_fractional_dpi() {
        for scale in [1.0_f32, 1.25, 1.5, 1.75, 2.0] {
            let left = (28.0 * scale).round();
            let top = (40.0 * scale).round();
            let right = (260.0 * scale).round();
            let header = code_header_geometry(left, right, top, scale);
            let pad = code_block_padding(scale);
            let header_h = code_header_height(scale);
            let gap = (CODE_LANGUAGE_COPY_GAP * scale).round().max(1.0);

            assert_eq!(header.button_size, (CODE_ACTION_SIZE * scale).round());
            assert_eq!(header.button_icon_size, (CODE_ACTION_ICON_SIZE * scale).round());
            assert!(header.language_max_w >= 0.0);
            assert!(header.language_x + header.language_max_w + gap <= header.button_x);
            assert!(header.button_y >= top + pad);
            assert!(header.button_y + header.button_size <= top + pad + header_h);
            let old_text_y = (top + pad + 24.0 * scale * 0.68).round();
            assert!(header.text_y <= old_text_y - (2.0 * scale).round());
            assert_eq!(header.text_y.fract(), 0.0);
        }
    }

    #[test]
    fn code_header_compacts_copy_inside_narrow_and_partially_visible_blocks() {
        let cases = [
            (100.0, 130.0, 1.0),
            (118.0, 142.0, 1.25),
            (-18.0, 20.0, 1.0),
            (164.0, 174.0, 1.5),
        ];
        for (left, right, scale) in cases {
            let header = code_header_geometry(left, right, 40.0, scale);
            let rounded_left = left.round();
            let rounded_right = right.round().max(rounded_left);

            assert!(header.button_x >= rounded_left, "case {left}..{right}");
            assert!(
                header.button_x + header.button_size <= rounded_right,
                "case {left}..{right}"
            );
            assert!(header.button_icon_size <= header.button_size);
            assert!(header.language_x <= header.button_x);
            assert_eq!(header.language_max_w, 0.0);
        }

        let narrow = code_header_geometry(100.0, 130.0, 0.0, 1.0);
        assert_eq!(narrow.button_x, 100.0);
        assert_eq!(narrow.button_size, 30.0);
        assert_eq!(narrow.button_icon_size, 16.0);
        assert_eq!(narrow.button_x + narrow.button_size, 130.0);
    }

    #[test]
    fn nested_quote_list_code_header_gives_copy_priority_at_narrow_width() {
        let source = "> - nested\n>\n>   ```rust\n>   code\n>   ```\n";
        let cache = layout(source, 180.0);
        let (block, code) = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Code(code) => Some((block, code)),
                _ => None,
            })
            .expect("nested quoted code block");
        assert!(code.quote_depth > 0);
        assert!(code.x > CONTENT_PAD);

        let header = code_header_geometry(code.x, code.x + 30.0, block.top, 1.0);
        assert_eq!(header.language_max_w, 0.0);
        assert_eq!(header.button_x, code.x.round());
        assert_eq!(header.button_x + header.button_size, (code.x + 30.0).round());
    }

    #[test]
    fn quoted_code_copy_preserves_newlines_without_quote_or_fence_markup() {
        let source = "> ```bash\n> echo one\n> echo two\n> ```\n";
        let cache = layout(source, 500.0);
        let id = cache
            .blocks
            .iter()
            .find_map(|block| matches!(block.kind, ReadBlockKind::Code(_)).then_some(block.source_range.start))
            .expect("code block");
        let copied = cache.code_block_copy_text(source, id).expect("copy payload");
        assert_eq!(copied, "echo one\necho two\n");
        assert!(!copied.contains('>'));
        assert!(!copied.contains("```"));
        assert!(!copied.contains("bash"));
    }

    #[test]
    fn code_block_hover_uses_cached_block_geometry_and_distinguishes_blocks() {
        let source = "```rust\none\n```\n\ntext\n\n```\ntwo\n```\n";
        let cache = layout(source, 500.0);
        let code_blocks = cache
            .blocks
            .iter()
            .filter_map(|block| match &block.kind {
                ReadBlockKind::Code(code) => Some((block, code)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(code_blocks.len(), 2);
        let frame = (10.0, 20.0, 500.0, 600.0);
        for (block, code) in code_blocks {
            let x = frame.0 + code.x + 8.0;
            let y = frame.1 + (block.top + block.bottom) * 0.5;
            assert_eq!(
                markdown_read_code_block_at_if_hover_valid(
                    true, &cache, frame, 0.0, 1.0, x, y,
                ),
                Some(block.source_range.start)
            );
            assert_eq!(
                markdown_read_code_block_at_if_hover_valid(
                    false, &cache, frame, 0.0, 1.0, x, y,
                ),
                None,
                "stale in-block mouse coordinates must not reactivate code-copy after leave"
            );
        }
        assert_eq!(
            markdown_read_code_block_at(&cache, frame, 0.0, 1.0, frame.0 - 1.0, frame.1 + 20.0),
            None
        );
    }
    #[test]
    fn reader_hidden_punctuation_search_target_falls_back_to_block_geometry() {
        let source = "# heading\n";
        let cache = layout(source, 500.0);
        assert!(cache.source_target_y(&(0..1)).is_some());
    }
    #[test]
    fn visible_selection_is_bounded_for_long_documents() {
        let source = (0..4000).map(|i| format!("paragraph {i}\n\n")).collect::<String>();
        let cache = layout(&source, 600.0);
        let visible = visible_block_range(&cache.blocks, 20_000.0, 20_800.0);
        assert!(visible.len() < cache.blocks.len() / 8);
    }
    #[test]
    fn huge_code_block_inner_selection_is_bounded() {
        let mut source = String::from("```rust\n");
        for i in 0..10_000 {
            source.push_str("let value_");
            source.push_str(&i.to_string());
            source.push_str(" = 42;\n");
        }
        source.push_str("```\n");
        let cache = layout(&source, 620.0);
        let code = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Code(code) => Some(code),
                _ => None,
            })
            .expect("code block");
        assert!(code.lines.len() >= 10_000);
        let center = code.lines[5_000].y;
        let visible = visible_code_line_range(&code.lines, center - 500.0, center + 500.0);
        assert!(visible.len() < 64, "selected {} of {} code lines", visible.len(), code.lines.len());
    }
    #[test]
    fn huge_pipe_table_inner_selection_is_bounded() {
        let mut source = String::from("| left | right |\n| --- | --- |\n");
        for i in 0..3_000 {
            source.push('|');
            source.push_str(&i.to_string());
            source.push_str(" | value |\n");
        }
        let cache = layout(&source, 620.0);
        let table = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Table(table) => Some(table),
                _ => None,
            })
            .expect("table block");
        assert!(table.rows.len() >= 3_000);
        assert!(table.cell_width > 0.0);
        assert_eq!(table.cell_width, table.width / 2.0);
        let center = table.rows[1_500].y;
        let visible = visible_table_row_range(&table.rows, center - 500.0, center + 500.0);
        assert!(visible.len() < 64, "selected {} of {} rows", visible.len(), table.rows.len());
    }
    #[test]
    fn huge_wrapped_table_cell_line_selection_is_bounded() {
        let mut source = String::from("| content |\n| --- |\n| ");
        for _ in 0..1_200 {
            source.push_str("слово😀слово😀 ");
        }
        source.push_str("|\n");

        let cache = layout(&source, 72.0);
        let table = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Table(table) => Some(table),
                _ => None,
            })
            .expect("table block");
        assert!(table.rows.len() <= 3, "unexpected row count: {}", table.rows.len());

        let (row, cell) = table
            .rows
            .iter()
            .flat_map(|row| row.cells.iter().map(move |cell| (row, cell)))
            .max_by_key(|(_, cell)| cell.lines.len())
            .expect("table cell");
        assert!(
            cell.lines.len() >= 3_000,
            "expected thousands of wrapped cell lines, got {}",
            cell.lines.len()
        );

        let viewport_h = table.line_height * 40.0;
        let middle_idx = cell.lines.len() / 2;
        let middle_top = row.y
            + table.cell_padding
            + middle_idx as f32 * table.line_height;
        let middle = visible_table_cell_line_range(
            cell.lines.len(),
            row.y,
            table.cell_padding,
            table.line_height,
            middle_top,
            middle_top + viewport_h,
        );
        assert!(middle.len() < 64, "selected {} of {} cell lines", middle.len(), cell.lines.len());
        assert!(middle.len() * 20 < cell.lines.len());
        for idx in middle.clone() {
            let range = &cell.lines[idx];
            assert!(cell.styled.text.is_char_boundary(range.start));
            assert!(cell.styled.text.is_char_boundary(range.end));
        }

        let end_top = row.y
            + table.cell_padding
            + cell.lines.len().saturating_sub(24) as f32 * table.line_height;
        let end = visible_table_cell_line_range(
            cell.lines.len(),
            row.y,
            table.cell_padding,
            table.line_height,
            end_top,
            end_top + viewport_h,
        );
        assert!(end.len() < 64, "selected {} end cell lines", end.len());
        assert_eq!(end.end, cell.lines.len());
        for idx in end {
            let range = &cell.lines[idx];
            assert!(cell.styled.text.is_char_boundary(range.start));
            assert!(cell.styled.text.is_char_boundary(range.end));
        }
    }
    #[test]
    fn styled_run_selection_is_bounded_near_end_of_huge_unicode_paragraph() {
        let mut styled = StyledText::default();
        let plain = TextStyle::default();
        let strong = plain.with(TextStyle::STRONG);
        for i in 0..6_000 {
            let style = if i % 2 == 0 { plain } else { strong };
            styled.push(
                if i % 3 == 0 { "слово😀" } else { "text" },
                style,
                None,
            );
        }
        assert!(styled.runs.len() >= 5_000);

        let first = styled.runs.len() - 24;
        let last = styled.runs.len() - 8;
        let text_range = styled.runs[first].range.start..styled.runs[last].range.end;
        assert!(styled.text.is_char_boundary(text_range.start));
        assert!(styled.text.is_char_boundary(text_range.end));

        let visible = visible_styled_run_range(&styled.runs, &text_range);
        assert!(visible.start >= first);
        assert!(visible.len() <= last - first + 1);
        assert!(visible.len() < 32, "selected {} of {} styled runs", visible.len(), styled.runs.len());
        for run in &styled.runs[visible] {
            assert!(run.range.end > text_range.start);
            assert!(run.range.start < text_range.end);
            assert!(styled.text.is_char_boundary(run.range.start));
            assert!(styled.text.is_char_boundary(run.range.end));
        }
    }
    #[test]
    fn huge_wrapped_paragraph_inner_selection_is_bounded() {
        let mut source = String::new();
        for i in 0..12_000 {
            if i > 0 {
                source.push(' ');
            }
            source.push_str("слово😀");
        }
        source.push('\n');
        let cache = layout(&source, 140.0);
        let text = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Text(text) => Some(text),
                _ => None,
            })
            .expect("paragraph block");
        assert!(text.lines.len() > 1_000);
        let center = text.lines[text.lines.len() / 2].y;
        let visible = visible_text_line_range(&text.lines, center - 500.0, center + 500.0);
        assert!(visible.len() < 64, "selected {} of {} text lines", visible.len(), text.lines.len());
        for idx in visible {
            let range = &text.lines[idx].range;
            assert!(text.styled.text.is_char_boundary(range.start));
            assert!(text.styled.text.is_char_boundary(range.end));
        }
    }

    #[test]
    fn reader_hit_test_helpers_choose_nearest_cached_geometry_in_large_layout() {
        let blocks = (0..20_000usize)
            .map(|idx| {
                let top = idx as f32 * 12.0;
                ReadBlock {
                    source_range: idx..idx + 1,
                    parent_source_ranges: Vec::new(),
                    top,
                    bottom: top + 8.0,
                    kind: ReadBlockKind::Rule {
                        x: 0.0,
                        width: 1.0,
                        quote_depth: 0,
                    },
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(nearest_block_index(&blocks, 0.0), Some(0));
        assert_eq!(nearest_block_index(&blocks, blocks[15_000].top + 1.0), Some(15_000));
        let gap_y = (blocks[999].bottom + blocks[1_000].top) * 0.5;
        assert_eq!(nearest_block_index(&blocks, gap_y), Some(999));
        assert_eq!(nearest_block_index(&blocks, blocks[19_999].bottom + 100.0), Some(19_999));

        let lines = (0..50_000usize)
            .map(|idx| {
                let top = idx as f32 * 18.0;
                CodeLine {
                    source_range: idx..idx + 1,
                    top,
                    y: top + 15.0,
                    bottom: top + 18.0,
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            line_box_index(&lines, lines[42_000].top + 1.0, |line| line.top, |line| line.bottom),
            Some(42_000)
        );
        assert_eq!(
            line_box_index(&lines, -100.0, |line| line.top, |line| line.bottom),
            Some(0)
        );
        assert_eq!(
            line_box_index(&lines, 1_000_000.0, |line| line.top, |line| line.bottom),
            Some(lines.len() - 1)
        );
    }

    #[test]
    fn reader_line_box_hit_test_uses_the_visible_line_not_nearest_baseline() {
        let lines = [
            CodeLine { source_range: 0..1, top: 10.0, y: 26.0, bottom: 30.0 },
            CodeLine { source_range: 1..2, top: 30.0, y: 46.0, bottom: 50.0 },
        ];
        for y in [10.0, 18.0, 29.99] {
            assert_eq!(line_box_index(&lines, y, |line| line.top, |line| line.bottom), Some(0), "y={y}");
        }
        for y in [30.0, 31.0, 40.0, 49.99] {
            assert_eq!(line_box_index(&lines, y, |line| line.top, |line| line.bottom), Some(1), "y={y}");
        }

        let gapped = [
            CodeLine { source_range: 0..1, top: 10.0, y: 26.0, bottom: 30.0 },
            CodeLine { source_range: 1..2, top: 34.0, y: 50.0, bottom: 54.0 },
        ];
        assert_eq!(line_box_index(&gapped, 31.0, |line| line.top, |line| line.bottom), Some(0));
        assert_eq!(line_box_index(&gapped, 33.0, |line| line.top, |line| line.bottom), Some(1));
    }

    #[test]
    fn table_line_hit_test_uses_uniform_line_boxes_at_boundaries() {
        assert_eq!(uniform_line_box_index(3, 20.0, 18.0, 19.0), Some(0));
        assert_eq!(uniform_line_box_index(3, 20.0, 18.0, 20.0), Some(0));
        assert_eq!(uniform_line_box_index(3, 20.0, 18.0, 37.99), Some(0));
        assert_eq!(uniform_line_box_index(3, 20.0, 18.0, 38.0), Some(1));
        assert_eq!(uniform_line_box_index(3, 20.0, 18.0, 56.0), Some(2));
        assert_eq!(uniform_line_box_index(3, 20.0, 18.0, 100.0), Some(2));
    }

}
