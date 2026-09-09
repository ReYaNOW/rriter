// Source-backed Markdown Read/Edit scroll geometry.
// Included by markdown_read.rs so Reader layout internals stay private while the
// transition API remains reusable by app state without duplicating layout logic.

use crate::editor::Editor;

/// Source-backed visual-line anchor used to preserve a viewport across Read/Edit.
///
/// `source_range` identifies the rendered source fragment. `viewport_offset_y`
/// is visual-line-top minus document Y at the top of the effective text viewport
/// (after caller-owned chrome/sticky insets). It may be negative for a partially
/// clipped line and is intentionally kept as f32 so repeated mode switches do not
/// accumulate draw-time rounding. Reader gaps use downstream affinity when a next
/// source-backed visual line exists, otherwise the last line is retained.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MarkdownSourceAnchor {
    pub(crate) source_range: Range<usize>,
    pub(crate) viewport_offset_y: f32,
}

impl MarkdownSourceAnchor {
    #[inline]
    pub(crate) fn projected_scroll_y(&self, line_y: f32) -> f32 {
        line_y - self.viewport_offset_y
    }
}

#[derive(Clone, Debug)]
struct ReadAnchorLine {
    source_range: Range<usize>,
    top: f32,
    bottom: f32,
    projection_y: f32,
}

type ReadSourceScope = Vec<Range<usize>>;

#[derive(Clone, Debug)]
struct ReadSourceLine {
    source_range: Range<usize>,
    scope_index: usize,
    y: f32,
}

fn text_line_top(text: &TextBlock, line: &TextLine) -> f32 {
    (line.y - text.line_height * 0.82).round()
}

fn code_line_top(code: &CodeBlock, line: &CodeLine) -> f32 {
    (line.y - code.line_height * 0.82).round()
}

fn append_styled_line_source_entries(
    styled: &StyledText,
    visual_range: &Range<usize>,
    y: f32,
    fallback_source: &Range<usize>,
    scope_index: usize,
    out: &mut Vec<ReadSourceLine>,
) -> Range<usize> {
    let mut first = None;
    let visible_runs = visible_styled_run_range(&styled.runs, visual_range);
    for run in &styled.runs[visible_runs] {
        let Some(source) = run.source_range.as_ref() else {
            continue;
        };
        let start = run.range.start.max(visual_range.start);
        let end = run.range.end.min(visual_range.end);
        if start >= end {
            continue;
        }
        let source_start = source.start + start.saturating_sub(run.range.start);
        let source_end = source.start + end.saturating_sub(run.range.start);
        let mapped = source_start.min(source.end)..source_end.min(source.end);
        if mapped.start >= mapped.end {
            continue;
        }
        if first.is_none() {
            first = Some(mapped.clone());
        }
        out.push(ReadSourceLine {
            source_range: mapped,
            scope_index,
            y,
        });
    }
    if let Some(first) = first {
        return first;
    }

    // Empty rendered lines and fully synthetic lines still need a source-local
    // position. Prefer the exact visual/source boundary when one exists (raw
    // blocks, empty physical lines); otherwise use this block/cell's own start.
    let byte = styled_source_boundary(styled, visual_range.start)
        .filter(|&byte| fallback_source.start <= byte && byte <= fallback_source.end)
        .unwrap_or(fallback_source.start);
    let fallback = byte..byte;
    out.push(ReadSourceLine {
        source_range: fallback.clone(),
        scope_index,
        y,
    });
    fallback
}

fn push_source_scope(
    scopes: &mut Vec<ReadSourceScope>,
    ranges: impl IntoIterator<Item = Range<usize>>,
) -> usize {
    let mut scope = Vec::new();
    for range in ranges {
        if scope.last() != Some(&range) {
            scope.push(range);
        }
    }
    let index = scopes.len();
    scopes.push(scope);
    index
}

fn push_anchor_line(
    anchors: &mut Vec<ReadAnchorLine>,
    source_range: Range<usize>,
    top: f32,
    bottom: f32,
) {
    let top = top.max(0.0);
    let bottom = bottom.max(top);
    if let Some(last) = anchors.last_mut()
        && (last.top - top).abs() < 0.01
    {
        last.bottom = last.bottom.max(bottom);
        if last.source_range.is_empty() && !source_range.is_empty() {
            last.source_range = source_range;
        }
        return;
    }
    anchors.push(ReadAnchorLine {
        source_range,
        top,
        bottom,
        projection_y: top,
    });
}

fn build_read_source_indices(
    blocks: &[ReadBlock],
) -> (
    Vec<ReadAnchorLine>,
    Vec<ReadSourceLine>,
    Vec<usize>,
    Vec<ReadSourceScope>,
) {
    let mut anchors = Vec::new();
    let mut source_lines = Vec::new();
    let mut source_scopes = Vec::new();

    for block in blocks {
        let source_before = source_lines.len();
        let block_scope = push_source_scope(
            &mut source_scopes,
            std::iter::once(block.source_range.clone())
                .chain(block.parent_source_ranges.iter().cloned()),
        );
        match &block.kind {
            ReadBlockKind::Text(text) => {
                for line in &text.lines {
                    let top = text_line_top(text, line);
                    let preferred = append_styled_line_source_entries(
                        &text.styled,
                        &line.range,
                        top,
                        &block.source_range,
                        block_scope,
                        &mut source_lines,
                    );
                    push_anchor_line(&mut anchors, preferred, top, top + text.line_height);
                }
            }
            ReadBlockKind::Code(code) => {
                for line in &code.lines {
                    let top = code_line_top(code, line);
                    push_anchor_line(
                        &mut anchors,
                        line.source_range.clone(),
                        top,
                        top + code.line_height,
                    );
                    source_lines.push(ReadSourceLine {
                        source_range: line.source_range.clone(),
                        scope_index: block_scope,
                        y: top,
                    });
                }
            }
            ReadBlockKind::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        let cell_scope = push_source_scope(
                            &mut source_scopes,
                            std::iter::once(cell.source_range.clone())
                                .chain(std::iter::once(row.source_range.clone()))
                                .chain(std::iter::once(block.source_range.clone()))
                                .chain(block.parent_source_ranges.iter().cloned()),
                        );
                        for (line_idx, range) in cell.lines.iter().enumerate() {
                            let top =
                                (row.y + table.cell_padding + line_idx as f32 * table.line_height)
                                    .round();
                            let preferred = append_styled_line_source_entries(
                                &cell.styled,
                                range,
                                top,
                                &cell.source_range,
                                cell_scope,
                                &mut source_lines,
                            );
                            push_anchor_line(&mut anchors, preferred, top, top + table.line_height);
                        }
                    }
                }
            }
            ReadBlockKind::Rule { .. } => {
                push_anchor_line(
                    &mut anchors,
                    block.source_range.clone(),
                    block.top,
                    block.bottom,
                );
                source_lines.push(ReadSourceLine {
                    source_range: block.source_range.clone(),
                    scope_index: block_scope,
                    y: block.top,
                });
            }
        }

        // A block with no rendered source text still gets a source-local empty
        // position. Never use file byte 0 as a synthetic sentinel for a later block.
        if source_lines.len() == source_before {
            source_lines.push(ReadSourceLine {
                source_range: block.source_range.start..block.source_range.start,
                scope_index: block_scope,
                y: block.top,
            });
        }
    }

    anchors.sort_by(|a, b| a.top.total_cmp(&b.top));
    source_lines.sort_by(|a, b| {
        a.source_range
            .start
            .cmp(&b.source_range.start)
            .then_with(|| a.source_range.end.cmp(&b.source_range.end))
            .then_with(|| a.y.total_cmp(&b.y))
    });

    let mut source_prefix_max_end = Vec::with_capacity(source_lines.len());
    let mut max_end = 0usize;
    for line in &source_lines {
        max_end = max_end.max(line.source_range.end);
        source_prefix_max_end.push(max_end);
    }

    // Some visual lines are necessarily synthetic or ambiguous in source space.
    // Resolve each viewport anchor through the same locator used by reverse
    // projection, then carry that resolved Y in the offset. This guarantees that
    // the source fragment and viewport offset describe one resolvable geometry.
    for anchor in &mut anchors {
        if let Some(line) = read_source_line(
            &source_lines,
            &source_prefix_max_end,
            &source_scopes,
            &anchor.source_range,
        ) {
            anchor.source_range = line.source_range.clone();
            anchor.projection_y = line.y;
        }
    }

    (anchors, source_lines, source_prefix_max_end, source_scopes)
}

#[inline]
fn source_range_is_valid(source_range: &Range<usize>, source_len: usize) -> bool {
    source_range.start <= source_range.end && source_range.end <= source_len
}

#[inline]
fn source_fallback_distance(a: &Range<usize>, b: &Range<usize>) -> usize {
    if a.end <= b.start {
        b.start - a.end
    } else if b.end <= a.start {
        a.start - b.end
    } else {
        0
    }
}

fn exact_read_source_line<'a>(
    source_lines: &'a [ReadSourceLine],
    source_prefix_max_end: &[usize],
    source_range: &Range<usize>,
) -> Option<&'a ReadSourceLine> {
    if source_lines.is_empty() || source_prefix_max_end.len() != source_lines.len() {
        return None;
    }

    if source_range.is_empty() {
        let byte = source_range.start;

        // Empty positions have downstream affinity at a wrap/source boundary.
        // An entry beginning exactly at the byte wins over the preceding entry
        // ending there. Among equal starts the sorted order picks the narrowest,
        // then the first visual occurrence.
        let downstream = source_lines.partition_point(|entry| entry.source_range.start < byte);
        if let Some(entry) = source_lines.get(downstream)
            && entry.source_range.start == byte
        {
            return Some(entry);
        }

        // Otherwise use a real containing half-open interval. Strict `byte < end`
        // intentionally excludes a range that merely ends at this boundary.
        let first_possible = source_prefix_max_end.partition_point(|&max_end| max_end <= byte);
        let end = source_lines.partition_point(|entry| entry.source_range.start < byte);
        return source_lines[first_possible.min(end)..end]
            .iter()
            .rev()
            .find(|entry| entry.source_range.start < byte && byte < entry.source_range.end);
    }

    // Non-empty source ranges use strict half-open intersection. A touching range
    // cannot compete with an actual hit. If a query spans several visual lines,
    // the first source-order line with a real intersection is the target.
    let first_possible =
        source_prefix_max_end.partition_point(|&max_end| max_end <= source_range.start);
    let end = source_lines.partition_point(|entry| entry.source_range.start < source_range.end);
    source_lines[first_possible.min(end)..end]
        .iter()
        .find(|entry| source_intersection(&entry.source_range, source_range).is_some())
}

#[inline]
fn source_scope_owns(scope: &Range<usize>, source_range: &Range<usize>) -> bool {
    if source_range.is_empty() {
        scope.start <= source_range.start && source_range.start <= scope.end
    } else {
        source_intersection(scope, source_range).is_some()
            || (scope.start <= source_range.start && source_range.end <= scope.end)
    }
}

fn source_scope_match<'a>(
    source_scopes: &'a [ReadSourceScope],
    entry: &ReadSourceLine,
    source_range: &Range<usize>,
) -> Option<&'a Range<usize>> {
    source_scopes
        .get(entry.scope_index)?
        .iter()
        .find(|scope| source_scope_owns(scope, source_range))
}

#[inline]
fn source_scope_is_more_specific(a: &Range<usize>, b: &Range<usize>) -> bool {
    let a_inside_b = b.start <= a.start && a.end <= b.end;
    let b_inside_a = a.start <= b.start && b.end <= a.end;
    if a_inside_b != b_inside_a {
        return a_inside_b;
    }
    a.end.saturating_sub(a.start) < b.end.saturating_sub(b.start)
}

fn choose_fallback_candidate<'a>(
    source_scopes: &[ReadSourceScope],
    source_range: &Range<usize>,
    upstream: Option<&'a ReadSourceLine>,
    downstream: Option<&'a ReadSourceLine>,
) -> Option<&'a ReadSourceLine> {
    let upstream_scope =
        upstream.and_then(|entry| source_scope_match(source_scopes, entry, source_range));
    let downstream_scope =
        downstream.and_then(|entry| source_scope_match(source_scopes, entry, source_range));

    match (upstream_scope, downstream_scope) {
        (Some(upstream_scope), Some(downstream_scope))
            if source_scope_is_more_specific(upstream_scope, downstream_scope) =>
        {
            return upstream;
        }
        (Some(upstream_scope), Some(downstream_scope))
            if source_scope_is_more_specific(downstream_scope, upstream_scope) =>
        {
            return downstream;
        }
        (Some(_), None) => return upstream,
        (None, Some(_)) => return downstream,
        _ => {}
    }

    match (upstream, downstream) {
        (Some(upstream), Some(downstream)) => {
            let upstream_distance = source_fallback_distance(source_range, &upstream.source_range);
            let downstream_distance =
                source_fallback_distance(source_range, &downstream.source_range);
            if downstream_distance <= upstream_distance {
                Some(downstream)
            } else {
                Some(upstream)
            }
        }
        (Some(upstream), None) => Some(upstream),
        (None, Some(downstream)) => Some(downstream),
        (None, None) => None,
    }
}

fn fallback_read_source_line<'a>(
    source_lines: &'a [ReadSourceLine],
    source_scopes: &[ReadSourceScope],
    source_range: &Range<usize>,
) -> Option<&'a ReadSourceLine> {
    if source_lines.is_empty() {
        return None;
    }

    let pivot = source_lines.partition_point(|entry| entry.source_range.start < source_range.start);
    let upstream = pivot.checked_sub(1).and_then(|idx| source_lines.get(idx));
    let downstream = source_lines.get(pivot);
    choose_fallback_candidate(source_scopes, source_range, upstream, downstream)
}

fn read_source_line<'a>(
    source_lines: &'a [ReadSourceLine],
    source_prefix_max_end: &[usize],
    source_scopes: &[ReadSourceScope],
    source_range: &Range<usize>,
) -> Option<&'a ReadSourceLine> {
    exact_read_source_line(source_lines, source_prefix_max_end, source_range)
        .or_else(|| fallback_read_source_line(source_lines, source_scopes, source_range))
}

fn read_source_line_y(
    source_lines: &[ReadSourceLine],
    source_prefix_max_end: &[usize],
    source_scopes: &[ReadSourceScope],
    source_range: &Range<usize>,
) -> Option<f32> {
    read_source_line(
        source_lines,
        source_prefix_max_end,
        source_scopes,
        source_range,
    )
    .map(|line| line.y)
}

fn read_viewport_anchor(
    anchors: &[ReadAnchorLine],
    viewport_y: f32,
) -> Option<MarkdownSourceAnchor> {
    if anchors.is_empty() || !viewport_y.is_finite() {
        return None;
    }
    let viewport_y = viewport_y.max(0.0);
    let next = anchors.partition_point(|line| line.top <= viewport_y);
    let idx = if next == 0 {
        0
    } else {
        let previous = next - 1;
        if viewport_y < anchors[previous].bottom || next >= anchors.len() {
            previous
        } else {
            next
        }
    };
    let line = &anchors[idx];
    Some(MarkdownSourceAnchor {
        source_range: line.source_range.clone(),
        viewport_offset_y: line.projection_y - viewport_y,
    })
}

impl MarkdownReadLayoutCache {
    pub(crate) fn viewport_source_anchor(&self, viewport_y: f32) -> Option<MarkdownSourceAnchor> {
        read_viewport_anchor(&self.anchor_lines, viewport_y)
    }

    pub(crate) fn source_anchor_y(&self, source_range: &Range<usize>) -> Option<f32> {
        if !source_range_is_valid(source_range, self.source_len) {
            return None;
        }
        read_source_line_y(
            &self.source_lines,
            &self.source_prefix_max_end,
            &self.source_scopes,
            source_range,
        )
    }
}

fn editor_line_source_range(editor: &Editor, line: usize) -> Option<Range<usize>> {
    let start = *editor.line_offsets.get(line)?;
    let mut end = editor
        .line_offsets
        .get(line + 1)
        .copied()
        .unwrap_or_else(|| editor.len())
        .min(editor.len());
    if end > start && editor.byte_at(end - 1) == b'\n' {
        end -= 1;
        if end > start && editor.byte_at(end - 1) == b'\r' {
            end -= 1;
        }
    }
    Some(start..end)
}

fn editor_source_line_index(editor: &Editor, source_range: &Range<usize>) -> Option<usize> {
    if editor.line_offsets.is_empty() || !source_range_is_valid(source_range, editor.len()) {
        return None;
    }
    let byte = source_range.start;
    Some(
        editor
            .line_offsets
            .partition_point(|&offset| offset <= byte)
            .saturating_sub(1)
            .min(editor.line_offsets.len().saturating_sub(1)),
    )
}

fn editor_viewport_anchor_from_map(
    editor: &Editor,
    phys_to_visual: &[usize],
    line_height: f32,
    viewport_y: f32,
) -> Option<MarkdownSourceAnchor> {
    if editor.line_offsets.is_empty()
        || phys_to_visual.len() != editor.line_offsets.len()
        || !line_height.is_finite()
        || line_height <= 0.0
        || !viewport_y.is_finite()
    {
        return None;
    }
    let viewport_y = viewport_y.max(0.0);
    let wanted_visual = (viewport_y / line_height).floor().max(0.0) as usize;
    let phys = phys_to_visual
        .partition_point(|&visual| visual < wanted_visual)
        .min(editor.line_offsets.len().saturating_sub(1));
    let visual = phys_to_visual.get(phys).copied().unwrap_or(0);
    let line_y = visual as f32 * line_height;
    Some(MarkdownSourceAnchor {
        source_range: editor_line_source_range(editor, phys)?,
        viewport_offset_y: line_y - viewport_y,
    })
}

fn editor_source_y_from_map(
    editor: &Editor,
    phys_to_visual: &[usize],
    line_height: f32,
    source_range: &Range<usize>,
) -> Option<f32> {
    if phys_to_visual.len() != editor.line_offsets.len()
        || !line_height.is_finite()
        || line_height <= 0.0
    {
        return None;
    }
    let phys = editor_source_line_index(editor, source_range)?;
    Some(phys_to_visual.get(phys).copied()? as f32 * line_height)
}

// Stage 1 exposes these production geometry entry points for the mode-transition
// integration in stage 2; this stage intentionally does not change toggle behavior.
#[allow(dead_code)]
impl Renderer {
    /// Prepares the single Reader layout without losing ownership of a current
    /// that was already rebased into an older Reader geometry by the physics
    /// tick. Absolute-navigation preparation and draw both use this wrapper,
    /// so resize/DPI/font changes cannot decode old pixels with a new layout.
    pub(crate) fn prepare_markdown_read_layout_preserving_current_ownership(
        &mut self,
        markdown: &mut MarkdownTabState,
        scroll: &mut crate::scroll::ScrollState,
        editor_version: u64,
        content_width: f32,
    ) -> bool {
        let content_width = content_width.max(1.0);
        let reproject_applied_current = scroll.deferred_current_rebase_applied() == Some(true)
            && !markdown.deferred_current_geometry_matches_read(
                editor_version,
                content_width,
                self.scale_factor,
                self.font_size,
            );
        let applied_anchor = if reproject_applied_current {
            let Some(anchor) = markdown.applied_read_current_anchor(scroll, editor_version) else {
                return false;
            };
            Some(anchor)
        } else {
            None
        };

        if !self.prepare_markdown_read_layout(markdown, editor_version, content_width) {
            return false;
        }
        let Some(anchor) = applied_anchor else {
            return true;
        };
        let Some(line_y) = markdown.read_layout.source_anchor_y(&anchor.source_range) else {
            return false;
        };
        markdown.reproject_applied_read_current(
            scroll,
            editor_version,
            &anchor,
            line_y,
            content_width,
            self.scale_factor,
            self.font_size,
        )
    }

    pub(crate) fn markdown_edit_viewport_anchor(
        &mut self,
        editor: &Editor,
        viewport_y: f32,
    ) -> Option<MarkdownSourceAnchor> {
        self.markdown_edit_viewport_anchor_with_line_height(editor, viewport_y, self.line_height)
    }

    pub(crate) fn markdown_edit_viewport_anchor_with_line_height(
        &mut self,
        editor: &Editor,
        viewport_y: f32,
        line_height: f32,
    ) -> Option<MarkdownSourceAnchor> {
        self.ensure_editor_visual_line_map(editor);
        editor_viewport_anchor_from_map(editor, &self.phys_to_visual, line_height, viewport_y)
    }

    pub(crate) fn markdown_edit_source_y(
        &mut self,
        editor: &Editor,
        source_range: &Range<usize>,
    ) -> Option<f32> {
        self.markdown_edit_source_y_with_line_height(editor, source_range, self.line_height)
    }

    pub(crate) fn markdown_edit_source_y_with_line_height(
        &mut self,
        editor: &Editor,
        source_range: &Range<usize>,
        line_height: f32,
    ) -> Option<f32> {
        self.ensure_editor_visual_line_map(editor);
        editor_source_y_from_map(editor, &self.phys_to_visual, line_height, source_range)
    }
}

#[cfg(test)]
mod markdown_scroll_tests {
    use super::*;
    use crate::render_view::rebuild_editor_visual_line_map;

    fn layout(source: &str, width: f32) -> MarkdownReadLayoutCache {
        build_test_markdown_read_layout(source, width)
    }

    fn editor(source: &str) -> Editor {
        let mut editor = Editor::new(source.len() + 32);
        let _ = editor.insert_str(source);
        editor
    }

    #[test]
    fn reader_anchor_tracks_third_paragraph_not_absolute_document_height() {
        let source = "# Large heading\n\n- list one\n- list two\n\n```rust\nlet a = 1;\nlet b = 2;\n```\n\nfirst paragraph\n\nsecond paragraph\n\nthird paragraph target\n";
        let cache = layout(source, 260.0);
        let byte = source.find("third paragraph target").expect("target");
        let y = cache.source_anchor_y(&(byte..byte + 5)).expect("target y");
        let anchor = cache
            .viewport_source_anchor(y + 3.0)
            .expect("viewport anchor");

        assert!(anchor.source_range.start <= byte);
        assert!(anchor.source_range.end >= byte);
        assert_eq!(anchor.projected_scroll_y(y), y + 3.0);
    }

    #[test]
    fn reader_anchor_round_trips_inner_wrapped_paragraph_line() {
        let source = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau";
        let cache = layout(source, 120.0);
        let text = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Text(text) => Some(text),
                _ => None,
            })
            .expect("text block");
        assert!(text.lines.len() >= 5, "fixture must wrap deeply");
        let line = &text.lines[4];
        let y = text_line_top(text, line);
        let anchor = cache.viewport_source_anchor(y + 5.25).expect("anchor");
        let projected = cache
            .source_anchor_y(&anchor.source_range)
            .expect("projected y");

        assert_eq!(projected, y);
        assert!((anchor.projected_scroll_y(projected) - (y + 5.25)).abs() < f32::EPSILON);
        assert!(source.is_char_boundary(anchor.source_range.start));
        assert!(source.is_char_boundary(anchor.source_range.end));
    }

    #[test]
    fn reader_anchor_round_trips_inner_fenced_code_line() {
        let mut source = String::from("```rust\n");
        for line in 0..40 {
            source.push_str(&format!("let value_{line} = {line};\n"));
        }
        source.push_str("```\n");
        let cache = layout(&source, 360.0);
        let byte = source.find("value_31").expect("code target");
        let y = cache.source_anchor_y(&(byte..byte + 8)).expect("code y");
        let anchor = cache.viewport_source_anchor(y + 7.0).expect("anchor");

        assert!(anchor.source_range.start <= byte && byte <= anchor.source_range.end);
        assert_eq!(cache.source_anchor_y(&anchor.source_range), Some(y));
    }

    #[test]
    fn reader_anchor_distinguishes_deep_wrapped_table_cell_line() {
        let source = "| left | right |\n| --- | --- |\n| short | alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu |\n";
        let cache = layout(source, 180.0);
        let byte = source.find("lambda").expect("deep cell target");
        let deep_y = cache.source_anchor_y(&(byte..byte + 6)).expect("deep line");
        let row_y = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Table(table) => table.rows.last().map(|row| row.y),
                _ => None,
            })
            .expect("table row");

        assert!(deep_y > row_y, "wrapped cell must map below row top");
        let anchor = cache.viewport_source_anchor(deep_y + 2.0).expect("anchor");
        assert_eq!(cache.source_anchor_y(&anchor.source_range), Some(deep_y));
    }

    #[test]
    fn reader_source_locator_handles_hidden_markup_gaps_utf8_and_eof() {
        let source = "**жир** [label](https://example.com) `код` 😀\n\n---";
        let cache = layout(source, 220.0);
        for needle in ["**", "https://", "`", "😀"] {
            let byte = source.find(needle).expect("needle");
            let y = cache
                .source_anchor_y(&(byte..byte + needle.len()))
                .expect("fallback y");
            assert!(y.is_finite());
        }
        let eof = source.len()..source.len();
        assert!(cache.source_anchor_y(&eof).is_some());
        for line in &cache.anchor_lines {
            assert!(source.is_char_boundary(line.source_range.start.min(source.len())));
            assert!(source.is_char_boundary(line.source_range.end.min(source.len())));
        }
    }

    #[test]
    fn reader_anchor_keeps_partial_line_and_uses_downstream_gap_affinity() {
        let source =
            "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu\n\nsecond target\n";
        let cache = layout(source, 130.0);
        let text_blocks = cache
            .blocks
            .iter()
            .filter_map(|block| match &block.kind {
                ReadBlockKind::Text(text) => Some((block, text)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(text_blocks.len() >= 2);
        assert!(text_blocks[0].1.lines.len() >= 2);

        let wrapped_line = &text_blocks[0].1.lines[1];
        let wrapped_top = text_line_top(text_blocks[0].1, wrapped_line);
        let viewport_y = wrapped_top + 5.5;
        let anchor = cache
            .viewport_source_anchor(viewport_y)
            .expect("partial anchor");
        let projected = cache
            .source_anchor_y(&anchor.source_range)
            .expect("partial y");
        assert_eq!(projected, wrapped_top);
        assert!((anchor.viewport_offset_y + 5.5).abs() < f32::EPSILON);
        assert!((anchor.projected_scroll_y(projected) - viewport_y).abs() < f32::EPSILON);

        let first = text_blocks[0].0;
        let second = text_blocks[1].0;
        assert!(first.bottom < second.top);
        let gap_y = (first.bottom + second.top) * 0.5;
        let gap_anchor = cache.viewport_source_anchor(gap_y).expect("gap anchor");
        let second_byte = source.find("second target").expect("second byte");
        assert!(gap_anchor.source_range.start <= second_byte);
        assert!(gap_anchor.source_range.end >= second_byte);
        assert_eq!(
            cache.source_anchor_y(&gap_anchor.source_range),
            Some(second.top)
        );
    }

    #[test]
    fn reader_source_lookup_has_stable_wrap_boundary_eof_and_invalid_range_rules() {
        let source = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi";
        let cache = layout(source, 115.0);
        let pair = cache
            .source_lines
            .windows(2)
            .find(|pair| {
                pair[0].y < pair[1].y && pair[0].source_range.end <= pair[1].source_range.start
            })
            .expect("wrapped source entries");
        let previous = &pair[0];
        let next = &pair[1];
        let boundary = next.source_range.start;

        assert_eq!(cache.source_anchor_y(&(boundary..boundary)), Some(next.y));
        if previous.source_range.end < boundary {
            assert_eq!(
                cache.source_anchor_y(&(previous.source_range.end..boundary)),
                Some(next.y),
                "trimmed wrap whitespace uses downstream affinity"
            );
        }
        assert_eq!(
            cache.source_target_y(&next.source_range),
            cache.source_anchor_y(&next.source_range),
            "search targeting must use the shared locator"
        );
        assert!(
            cache
                .source_anchor_y(&(source.len()..source.len()))
                .is_some()
        );
        assert_eq!(cache.source_anchor_y(&(0..source.len() + 1)), None);
        assert_eq!(cache.source_anchor_y(&(source.len()..0)), None);
    }

    #[test]
    fn reader_fallbacks_keep_nested_hidden_markup_on_its_content_line() {
        let source = "> - [x] **nested bold** and [label](https://example.test)\n>\n> tail\n\n---\n\n<div>raw</div>\n";
        let cache = layout(source, 640.0);
        let nested = source.find("nested bold").expect("nested");
        let nested_y = cache
            .source_anchor_y(&(nested..nested + "nested".len()))
            .expect("nested y");
        for needle in ["> -", "[x]", "**", "https://example.test"] {
            let byte = source.find(needle).expect("hidden marker");
            assert_eq!(
                cache.source_anchor_y(&(byte..byte + needle.len())),
                Some(nested_y),
                "{needle} should fall back within the same visible nested line"
            );
        }

        let rule = source.find("---").expect("rule");
        let raw = source.find("<div>").expect("raw");
        let rule_y = cache.source_anchor_y(&(rule..rule + 3)).expect("rule y");
        let raw_y = cache.source_anchor_y(&(raw..raw + 5)).expect("raw y");
        assert!(rule_y > nested_y);
        assert!(raw_y > rule_y);
    }

    #[test]
    fn editor_source_projection_handles_partial_viewport_utf8_eof_and_invalid_ranges() {
        let source = "zero\none\nтри😀\nlast";
        let editor = editor(source);
        let mut map = Vec::new();
        assert_eq!(rebuild_editor_visual_line_map(&editor, &mut map), 4);

        let line_y = 48.0;
        let viewport_y = line_y + 5.25;
        let anchor =
            editor_viewport_anchor_from_map(&editor, &map, 24.0, viewport_y).expect("edit anchor");
        let utf8 = source.find("три😀").expect("utf8 line");
        assert_eq!(anchor.source_range.start, utf8);
        assert!(source.is_char_boundary(anchor.source_range.start));
        assert!(source.is_char_boundary(anchor.source_range.end));
        assert!((anchor.viewport_offset_y + 5.25).abs() < f32::EPSILON);
        let projected = editor_source_y_from_map(&editor, &map, 24.0, &anchor.source_range)
            .expect("edit projected y");
        assert_eq!(projected, line_y);
        assert!((anchor.projected_scroll_y(projected) - viewport_y).abs() < f32::EPSILON);

        let eof_y = editor_source_y_from_map(&editor, &map, 24.0, &(source.len()..source.len()))
            .expect("eof y");
        assert_eq!(eof_y, 72.0);
        assert_eq!(
            editor_source_y_from_map(&editor, &map, 24.0, &(0..source.len() + 1)),
            None
        );
        assert_eq!(
            editor_source_y_from_map(&editor, &map, 24.0, &(source.len()..0)),
            None
        );
    }

    #[test]
    fn editor_source_projection_uses_fold_header_without_cursor_mutation() {
        let source = "zero\nfold start\nhidden one\nhidden two\nafter\n";
        let mut editor = editor(source);
        editor.cursor = source.find("after").expect("cursor");
        editor.selection_anchor = Some(0);
        editor.foldable_lines.insert(1, 3);
        editor.folded_lines.insert(1);
        let cursor_before = editor.cursor;
        let selection_before = editor.selection_anchor;
        let mut map = Vec::new();
        let total = rebuild_editor_visual_line_map(&editor, &mut map);
        assert_eq!(map, vec![0, 1, 1, 1, 2, 3]);
        assert_eq!(total, 4);

        let hidden = source.find("hidden two").expect("hidden");
        let y =
            editor_source_y_from_map(&editor, &map, 24.0, &(hidden..hidden + 6)).expect("fold y");
        assert_eq!(y, 24.0);
        let anchor =
            editor_viewport_anchor_from_map(&editor, &map, 24.0, y + 4.0).expect("edit anchor");
        assert_eq!(
            anchor.source_range.start,
            source.find("fold start").unwrap()
        );
        assert_eq!(editor.cursor, cursor_before);
        assert_eq!(editor.selection_anchor, selection_before);
    }

    fn raw_text_layout(source: &str, width: f32, scale: f32) -> MarkdownReadLayoutCache {
        let mut styled = StyledText::default();
        styled.push(
            source,
            TextStyle::default().with(TextStyle::RAW),
            Some(0..source.len()),
        );
        let mut builder = LayoutBuilder::new(source, width, scale, |_, _| 8.0 * scale);
        builder.append_text(styled, 0.82, 0.0, 0, None, None, true, 0..source.len());
        let (blocks, content_height) = builder.finish();
        let mut cache = MarkdownReadLayoutCache::default();
        cache.replace_layout(
            LayoutKey::new(1, width, scale, 16.0),
            blocks,
            content_height,
            source.len(),
        );
        cache
    }

    pub(super) fn scaled_layout(source: &str, width: f32, scale: f32) -> MarkdownReadLayoutCache {
        let document = crate::languages::markdown::MarkdownParseState::default()
            .parse(source)
            .expect("markdown parse");
        let mut builder = LayoutBuilder::new(source, width, scale, |_, _| 8.0 * scale);
        builder.append_blocks(&document.blocks, 0.0, 0, None);
        let (blocks, content_height) = builder.finish();
        let mut cache = MarkdownReadLayoutCache::default();
        cache.replace_layout(
            LayoutKey::new(1, width, scale, 16.0),
            blocks,
            content_height,
            source.len(),
        );
        cache
    }

    #[test]
    fn reader_source_locator_uses_half_open_hits_before_boundary_fallback() {
        let mut cache = MarkdownReadLayoutCache::default();
        cache.source_len = 16;
        cache.source_lines = vec![
            ReadSourceLine {
                source_range: 7..8,
                scope_index: 0,
                y: 18.0,
            },
            ReadSourceLine {
                source_range: 8..16,
                scope_index: 0,
                y: 41.0,
            },
        ];
        cache.source_prefix_max_end = vec![8, 16];
        cache.source_scopes = vec![vec![7..16]];

        assert_eq!(cache.source_target_y(&(7..8)), Some(18.0));
        assert_eq!(cache.source_target_y(&(8..16)), Some(41.0));
        assert_eq!(cache.source_target_y(&(7..16)), Some(18.0));
        assert_eq!(
            cache.source_target_y(&(8..8)),
            Some(41.0),
            "an empty position at a shared boundary has downstream affinity"
        );
    }

    #[test]
    fn reviewer_half_open_nonempty_ranges_do_not_compete_at_touching_boundary() {
        let mut cache = MarkdownReadLayoutCache::default();
        cache.source_len = 16;
        cache.source_lines = vec![
            ReadSourceLine {
                source_range: 7..8,
                scope_index: 0,
                y: 18.0,
            },
            ReadSourceLine {
                source_range: 8..16,
                scope_index: 0,
                y: 41.0,
            },
        ];
        cache.source_prefix_max_end = vec![8, 16];
        cache.source_scopes = vec![vec![7..16]];

        assert_eq!(cache.source_target_y(&(7..8)), Some(18.0));
        assert_eq!(cache.source_target_y(&(8..16)), Some(41.0));
        assert_eq!(cache.source_target_y(&(7..16)), Some(18.0));
        assert_eq!(cache.source_target_y(&(8..8)), Some(41.0));
    }

    #[test]
    fn reviewer_deep_raw_empty_line_keeps_source_and_offset_on_same_geometry() {
        let mut source = String::new();
        for line in 0..90 {
            source.push_str(&format!("raw-{line:03}\n"));
        }
        let empty_byte = source.len();
        source.push('\n');
        source.push_str("tail\n");
        let cache = raw_text_layout(&source, 640.0, 1.0);
        let empty = cache
            .source_lines
            .iter()
            .find(|line| line.source_range == (empty_byte..empty_byte))
            .expect("deep empty source line");
        let viewport_y = empty.y + 6.5;
        let anchor = cache.viewport_source_anchor(viewport_y).expect("anchor");
        let projected_y = cache
            .source_anchor_y(&anchor.source_range)
            .expect("projection");

        assert!(empty.y > 1_600.0);
        assert_eq!(anchor.source_range, empty_byte..empty_byte);
        assert_eq!(projected_y, empty.y);
        assert!((anchor.projected_scroll_y(projected_y) - viewport_y).abs() < f32::EPSILON);
    }

    #[test]
    fn reviewer_empty_fenced_block_never_falls_back_to_file_zero() {
        let source = "before paragraph\n\n```text\n```\nafter\n";
        let cache = layout(source, 420.0);
        let (block, code) = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Code(code) => Some((block, code)),
                _ => None,
            })
            .expect("empty code block");
        let line = code.lines.first().expect("empty code line");
        let line_y = code_line_top(code, line);
        let viewport_y = line_y + 3.5;
        let anchor = cache.viewport_source_anchor(viewport_y).expect("anchor");
        let projected_y = cache
            .source_anchor_y(&anchor.source_range)
            .expect("projection");

        assert!(block.source_range.start > 0);
        assert_ne!(anchor.source_range, 0..0);
        assert!(block.source_range.start <= anchor.source_range.start);
        assert!(anchor.source_range.end <= block.source_range.end);
        assert_eq!(projected_y, line_y);
        assert!((anchor.projected_scroll_y(projected_y) - viewport_y).abs() < f32::EPSILON);
    }

    #[test]
    fn reviewer_synthetic_image_anchor_keeps_compensated_round_trip() {
        let source = "before\n\n![](target.png)\n\nafter\n";
        let cache = layout(source, 360.0);
        let image_byte = source.find("![]").expect("image");
        let (block, text) = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Text(text)
                    if block.source_range.start <= image_byte
                        && image_byte < block.source_range.end =>
                {
                    Some((block, text))
                }
                _ => None,
            })
            .expect("image block");
        let line_y = text_line_top(text, text.lines.first().expect("image line"));
        let viewport_y = line_y + 4.5;
        let anchor = cache.viewport_source_anchor(viewport_y).expect("anchor");
        let projected_y = cache
            .source_anchor_y(&anchor.source_range)
            .expect("projection");

        assert!(block.source_range.start <= anchor.source_range.start);
        assert!(anchor.source_range.end <= block.source_range.end);
        assert_ne!(anchor.source_range, 0..0);
        assert!((anchor.projected_scroll_y(projected_y) - viewport_y).abs() < f32::EPSILON);
    }

    #[test]
    fn reader_hidden_link_title_fallback_stays_in_own_text_block() {
        let source = "prefix paragraph\n\n[label](https://example.com/destination \"hidden title words\")\n\nsuffix paragraph\n";
        let cache = layout(source, 260.0);
        let label = source.find("label").expect("label");
        let title = source.find("hidden title words").expect("title");
        let label_y = cache
            .source_target_y(&(label..label + "label".len()))
            .expect("label y");
        let title_y = cache
            .source_target_y(&(title..title + "hidden title words".len()))
            .expect("title fallback y");

        assert_eq!(title_y, label_y);
    }

    #[test]
    fn reader_hidden_link_url_tail_fallback_stays_in_own_text_block() {
        let source =
            "prefix paragraph\n\n[label](https://example.com/destination)\n\nsuffix paragraph\n";
        let cache = layout(source, 260.0);
        let label = source.find("label").expect("label");
        let tail = source.find("destination").expect("url tail");
        let label_y = cache
            .source_target_y(&(label..label + "label".len()))
            .expect("label y");
        let tail_y = cache
            .source_target_y(&(tail..tail + "destination".len()))
            .expect("url fallback y");

        assert_eq!(tail_y, label_y);
    }

    #[test]
    fn reader_opening_code_fence_fallback_stays_in_own_code_block() {
        let source = "intro\n\n```rust\nfirst line\nsecond line\n```\nafter\n";
        let cache = layout(source, 300.0);
        let fence = source.find("```").expect("opening fence");
        let code = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Code(code) => Some((block, code)),
                _ => None,
            })
            .expect("code block");
        let expected_y = code_line_top(code.1, code.1.lines.first().expect("first code line"));
        let fence_y = cache
            .source_target_y(&(fence..fence + 3))
            .expect("fence fallback y");

        assert_eq!(fence_y, expected_y);
        assert!(fence_y >= code.0.top && fence_y < code.0.bottom);
    }

    #[test]
    fn reader_nonempty_exact_query_ignores_empty_synthetic_interval() {
        let source = "intro\n\n![alt](destination)\n\nsuffix";
        let cache = layout(source, 80.0);
        let exact_y = cache.source_target_y(&(9..10)).expect("real exact hit");
        let spanning_y = cache.source_target_y(&(7..10)).expect("spanning exact hit");

        assert_eq!(spanning_y, exact_y);
        assert_eq!(source.get(7..10), Some("![a"));
    }

    #[test]
    fn reader_hidden_utf8_link_fallback_is_local_at_fractional_scale() {
        let source = "до\n\n[метка](https://example.com/путь \"скрытый заголовок\")\n\nпосле";
        let cache = scaled_layout(source, 190.0, 1.25);
        let label = source.find("метка").expect("utf8 label");
        let title = source.find("скрытый заголовок").expect("utf8 title");
        let label_y = cache
            .source_target_y(&(label..label + "метка".len()))
            .expect("utf8 label y");
        let title_y = cache
            .source_target_y(&(title..title + "скрытый заголовок".len()))
            .expect("utf8 hidden title y");

        assert_eq!(title_y, label_y);
        assert!(cache.is_valid_for_geometry(1, 190.0, 1.25, 16.0));
    }

    #[test]
    fn reader_table_delimiter_fallback_stays_on_its_row_at_fractional_scale() {
        let source = "intro\n\n| α | β |\n| --- | --- |\n| one | two |\n\nafter";
        let cache = scaled_layout(source, 190.0, 1.25);
        let one = source.find("one").expect("one");
        let two = source.find("two").expect("two");
        let between = source[one..two].find('|').expect("cell delimiter") + one;
        let one_y = cache.source_target_y(&(one..one + 3)).expect("one y");
        let two_y = cache.source_target_y(&(two..two + 3)).expect("two y");
        let delimiter_y = cache
            .source_target_y(&(between..between + 1))
            .expect("delimiter fallback y");

        assert_eq!(one_y, two_y);
        assert_eq!(delimiter_y, one_y);
    }

    #[test]
    fn reader_raw_empty_line_anchor_keeps_local_source_and_viewport_offset() {
        let mut source = String::new();
        for line in 0..90 {
            source.push_str(&format!("raw-{line:03}\n"));
        }
        let empty_byte = source.len();
        source.push('\n');
        source.push_str("tail\n");
        let cache = raw_text_layout(&source, 640.0, 1.0);
        let empty = cache
            .source_lines
            .iter()
            .find(|line| line.source_range == (empty_byte..empty_byte))
            .expect("internal raw empty line source position");
        assert!(empty.y > 1_600.0, "fixture must exercise a deep raw line");

        let viewport_y = empty.y + 6.5;
        let anchor = cache
            .viewport_source_anchor(viewport_y)
            .expect("raw anchor");
        assert_eq!(anchor.source_range, empty_byte..empty_byte);
        let projected_y = cache
            .source_anchor_y(&anchor.source_range)
            .expect("raw reverse projection");
        assert_eq!(projected_y, empty.y);
        assert!((anchor.projected_scroll_y(projected_y) - viewport_y).abs() < f32::EPSILON);
    }

    #[test]
    fn reader_empty_fenced_block_anchor_never_uses_file_zero_sentinel() {
        let source = "before paragraph\n\n```text\n```\nafter\n";
        let cache = layout(source, 420.0);
        let block = cache
            .blocks
            .iter()
            .find(|block| matches!(block.kind, ReadBlockKind::Code(_)))
            .expect("empty fenced code block");
        let ReadBlockKind::Code(code) = &block.kind else {
            unreachable!();
        };
        let line = code.lines.first().expect("synthetic empty code line");
        assert!(block.source_range.start > 0);
        assert_eq!(
            line.source_range,
            block.source_range.start..block.source_range.start
        );

        let line_y = code_line_top(code, line);
        let viewport_y = line_y + 3.5;
        let anchor = cache
            .viewport_source_anchor(viewport_y)
            .expect("code anchor");
        assert_eq!(anchor.source_range, line.source_range);
        assert_ne!(anchor.source_range, 0..0);
        let projected_y = cache
            .source_anchor_y(&anchor.source_range)
            .expect("code reverse projection");
        assert_eq!(projected_y, line_y);
        assert!((anchor.projected_scroll_y(projected_y) - viewport_y).abs() < f32::EPSILON);
    }

    #[test]
    fn reader_synthetic_image_anchor_compensates_to_resolvable_geometry() {
        let source = "before\n\n![](target.png)\n\nafter\n";
        let cache = layout(source, 360.0);
        let image_byte = source.find("![]").expect("image byte");
        let block = cache
            .blocks
            .iter()
            .find(|block| {
                block.source_range.start <= image_byte
                    && image_byte < block.source_range.end
                    && matches!(block.kind, ReadBlockKind::Text(_))
            })
            .expect("image text block");
        let ReadBlockKind::Text(text) = &block.kind else {
            unreachable!();
        };
        let line = text.lines.first().expect("image line");
        let line_y = text_line_top(text, line);
        let viewport_y = line_y + 4.5;
        let anchor = cache
            .viewport_source_anchor(viewport_y)
            .expect("image anchor");
        assert!(block.source_range.contains(&anchor.source_range.start));
        assert_ne!(anchor.source_range, 0..0);
        let projected_y = cache
            .source_anchor_y(&anchor.source_range)
            .expect("image reverse projection");
        assert!((anchor.projected_scroll_y(projected_y) - viewport_y).abs() < f32::EPSILON);
    }

    #[test]
    fn reader_table_empty_cells_and_deep_wraps_use_local_fallback_geometry() {
        let source = "| left | middle | right |\n| --- | --- | --- |\n|  | alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu |  |\n";
        let cache = layout(source, 220.0);
        let table = cache
            .blocks
            .iter()
            .find_map(|block| match &block.kind {
                ReadBlockKind::Table(table) => Some(table),
                _ => None,
            })
            .expect("table block");
        let row = table.rows.last().expect("body row");
        assert_eq!(row.cells.len(), 3);

        for cell in [&row.cells[0], &row.cells[2]] {
            let y = cache
                .source_anchor_y(&(cell.source_range.start..cell.source_range.start))
                .expect("empty cell local fallback");
            assert_eq!(y, (row.y + table.cell_padding).round());
        }

        let lambda = source.find("lambda").expect("deep wrapped cell byte");
        let deep_y = cache
            .source_anchor_y(&(lambda..lambda + "lambda".len()))
            .expect("deep wrapped cell y");
        assert!(deep_y > row.y + table.cell_padding);
        let viewport_y = deep_y + 2.25;
        let anchor = cache
            .viewport_source_anchor(viewport_y)
            .expect("deep table anchor");
        let projected_y = cache
            .source_anchor_y(&anchor.source_range)
            .expect("table reverse projection");
        assert!((anchor.projected_scroll_y(projected_y) - viewport_y).abs() < f32::EPSILON);
    }

    #[test]
    fn reader_utf8_fractional_scale_anchor_has_exact_local_round_trip() {
        let source = "αβγδεζηθ ι κ λ μ ν ξ ο π ρ σ τ υ φ χ ψ ω\n\nконец 😀";
        let cache = scaled_layout(source, 170.0, 1.25);
        let byte = source.find("ζηθ").expect("utf8 target");
        let range = byte..byte + "ζηθ".len();
        assert!(source.is_char_boundary(range.start));
        assert!(source.is_char_boundary(range.end));
        let y = cache.source_target_y(&range).expect("utf8 y");
        let exact_line = cache
            .source_lines
            .iter()
            .find(|line| ranges_overlap(&line.source_range, &range))
            .expect("utf8 source line");
        assert_eq!(y, exact_line.y);

        let viewport_y = y + 3.25;
        let anchor = cache
            .viewport_source_anchor(viewport_y)
            .expect("utf8 anchor");
        assert!(source.is_char_boundary(anchor.source_range.start));
        assert!(source.is_char_boundary(anchor.source_range.end));
        let projected_y = cache
            .source_anchor_y(&anchor.source_range)
            .expect("utf8 reverse projection");
        assert!((anchor.projected_scroll_y(projected_y) - viewport_y).abs() < f32::EPSILON);
        assert!(cache.is_valid_for_geometry(1, 170.0, 1.25, 16.0));
        assert!(!cache.is_valid_for_geometry(1, 170.0, 1.5, 16.0));
    }

    #[test]
    fn repeated_source_lookups_reuse_layout_and_scale_to_large_code_index() {
        let mut source = String::from("```text\n");
        for line in 0..15_000 {
            source.push_str(&format!("line-{line:05}\n"));
        }
        source.push_str("```\n");
        let cache = layout(&source, 500.0);
        let rebuilds = cache.rebuild_count();
        assert!(cache.source_lines.len() >= 15_000);

        for line in [0, 7_500, 14_999] {
            let needle = format!("line-{line:05}");
            let byte = source.find(&needle).expect("line byte");
            assert!(
                cache
                    .source_anchor_y(&(byte..byte + needle.len()))
                    .is_some()
            );
        }
        let tail = source.find("line-14999").expect("tail byte");
        for _ in 0..100 {
            assert!(cache.source_anchor_y(&(tail..tail + 10)).is_some());
        }
        assert_eq!(cache.rebuild_count(), rebuilds);
    }
}

#[cfg(test)]
include!("markdown_scroll_review_tests.rs");
