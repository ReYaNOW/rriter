// `markdown_read.rs` include chunk: media blocks of the Reader (image-only paragraphs and Mermaid
// blocks). The layout is a pure function over `(natural size, state)` of every element and the
// column width; `MarkdownMedia` is read only while a layout is built and while a frame is drawn,
// and drawing neither loads, decodes nor allocates beyond the reused scratch buffer.

use std::collections::HashMap;
use std::path::Path;

use crate::app::{MediaItem, document_media};
use crate::markdown_media::{MediaEntryView, MediaKey};
use crate::theme::UiRole;

const MEDIA_GAP: f32 = 8.0;
const MEDIA_FRAME_PAD: f32 = 6.0;
const MEDIA_ALT_SCALE: f32 = 0.86;
const MEDIA_FRAME_RADIUS: f32 = 5.0;

/// One element of a media block. `x` is measured from the left edge of the content frame, `y`
/// from the top of the block; both, like `w` and `h`, are whole pixels. For a failed element
/// `alt` already reads `alt — reason`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlacedMedia {
    pub(crate) key: MediaKey,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) w: f32,
    pub(crate) h: f32,
    pub(crate) alt: String,
    /// Index into `MarkdownReadLayoutCache::links` of the link wrapped around the image.
    pub(crate) link: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MediaSlotState {
    Unknown,
    Pending,
    Ready,
    Failed,
}

/// What the layout needs to know about an element; carries no reference to `MarkdownMedia`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct MediaSlot {
    natural: Option<(f32, f32)>,
    state: MediaSlotState,
}

fn media_slot(entry: MediaEntryView<'_>) -> MediaSlot {
    match entry {
        MediaEntryView::Unknown => MediaSlot { natural: None, state: MediaSlotState::Unknown },
        MediaEntryView::Pending { natural } => MediaSlot { natural, state: MediaSlotState::Pending },
        MediaEntryView::Ready { natural_w, natural_h, .. } => MediaSlot {
            natural: Some((natural_w, natural_h)),
            state: MediaSlotState::Ready,
        },
        MediaEntryView::Failed(_) => MediaSlot { natural: None, state: MediaSlotState::Failed },
    }
}

fn media_line_height(scale: f32) -> f32 {
    (BODY_LINE_H * scale * BODY_SCALE.max(0.75)).round().max(1.0)
}

/// Size of one element: natural size times the UI scale, shrunk proportionally to the column
/// width when wider, never enlarged. Without a usable natural size (unknown, still loading
/// without a size, zero or not finite) or when failed it is a frame of one text line.
fn media_slot_size(slot: &MediaSlot, col_w: f32, scale: f32, line_h: f32) -> (f32, f32) {
    let natural = match slot.state {
        MediaSlotState::Failed => None,
        _ => slot
            .natural
            .filter(|(w, h)| w.is_finite() && h.is_finite() && *w > 0.0 && *h > 0.0),
    };
    let Some((natural_w, natural_h)) = natural else {
        return (col_w, line_h);
    };
    let (mut w, mut h) = (natural_w * scale, natural_h * scale);
    if w > col_w {
        h *= col_w / w;
        w = col_w;
    }
    // `max`/`min` rather than `clamp`: a NaN scale must end up at 1 px, not stay NaN.
    (w.round().max(1.0).min(col_w), h.round().max(1.0))
}

/// Places elements left to right with a gap, wrapping to a new row when the next one does not
/// fit; a row is as high as its highest element. Returns `[x, y, w, h]` of every element, `x`
/// from the column start and `y` from the block top, and the height of the block.
fn layout_media_rects(slots: &[MediaSlot], col_w: f32, scale: f32, line_h: f32) -> (Vec<[f32; 4]>, f32) {
    let col_w = col_w.floor().max(1.0);
    let line_h = line_h.max(1.0);
    let gap = (MEDIA_GAP * scale).round().max(0.0);
    let mut rects = Vec::with_capacity(slots.len());
    let (mut x, mut y, mut row_h) = (0.0f32, 0.0f32, 0.0f32);
    for slot in slots {
        let (w, h) = media_slot_size(slot, col_w, scale, line_h);
        if x > 0.0 && x + w > col_w {
            y += row_h + gap;
            x = 0.0;
            row_h = 0.0;
        }
        rects.push([x, y, w, h]);
        x += w + gap;
        row_h = row_h.max(h);
    }
    (rects, y + row_h)
}

/// Media elements of the document, found once per layout build, keyed by the start of the
/// block they replace, plus the cache that supplies their sizes and states.
struct MediaInput<'m> {
    items: HashMap<usize, Vec<MediaItem>>,
    media: &'m MarkdownMedia,
}

impl<'m> MediaInput<'m> {
    fn new(
        document: &crate::languages::markdown::MarkdownDocument,
        source: &str,
        doc_dir: &Path,
        media: &'m MarkdownMedia,
    ) -> Self {
        let defs = document.link_definitions(source);
        let items = document_media(document, source, doc_dir, &defs)
            .into_iter()
            .filter_map(|(index, found)| {
                Some((document.blocks.get(index)?.source_range.start, found))
            })
            .collect();
        Self { items, media }
    }

    /// First line of the error of a Mermaid block that failed to render, whatever the failure
    /// (spec: the block stays a code block and shows one error line under its header).
    fn mermaid_error(&self, block_start: usize) -> Option<String> {
        let item = self.items.get(&block_start)?.first()?;
        match (&item.key, self.media.entry(&item.key)) {
            (MediaKey::Mermaid(_), MediaEntryView::Failed(error)) => {
                Some(error.label().lines().next().unwrap_or("").to_string())
            }
            _ => None,
        }
    }
}

impl<'a, F: FnMut(char, bool, Option<f32>) -> f32> LayoutBuilder<'a, F> {
    fn with_media(mut self, media: Option<MediaInput<'a>>) -> Self {
        self.media = media;
        self
    }

    fn media_code_error(&self, block_start: usize) -> Option<String> {
        self.media.as_ref()?.mermaid_error(block_start)
    }

    /// Lays `block` out as media when it has media elements: a paragraph becomes media blocks
    /// for its images and text blocks for the text around them, in source order (see
    /// `split_media_paragraph`). False leaves the block to the text layout.
    fn try_append_media(&mut self, block: &MarkdownBlock, indent: f32) -> bool {
        // Taken out for the duration so that the text parts can borrow `self` mutably.
        let Some(input) = self.media.take() else {
            return false;
        };
        let handled = self.append_media_parts(&input, block, indent);
        self.media = Some(input);
        handled
    }

    fn append_media_parts(&mut self, input: &MediaInput<'_>, block: &MarkdownBlock, indent: f32) -> bool {
        let Some(items) = input.items.get(&block.source_range.start) else {
            return false;
        };
        if input.mermaid_error(block.source_range.start).is_some() {
            return false;
        }
        let inlines = match &block.kind {
            MarkdownBlockKind::Paragraph { inlines, .. } => inlines.as_slice(),
            _ => &[],
        };
        let parts = split_media_paragraph(self.source, inlines, items);
        if parts.is_empty() {
            return false;
        }
        let whole = parts.len() == 1;
        for part in parts {
            match part {
                ParagraphPart::Media(range) => {
                    let source_range = if whole {
                        block.source_range.clone()
                    } else {
                        items[range.start].source_range.start..items[range.end - 1].source_range.end
                    };
                    self.append_media_run(input, &items[range], source_range, indent);
                }
                ParagraphPart::Text(range) => {
                    let spans = &inlines[range];
                    let source_range = spans[0].source_range.start..spans[spans.len() - 1].source_range.end;
                    let mut styled = styled_from_inlines(self.source, spans, &[], &mut self.links);
                    styled.trim_edge_whitespace();
                    self.append_text(styled, BODY_SCALE, indent, 0, None, None, false, source_range);
                }
            }
        }
        true
    }

    /// One media block of `items`, laid out in rows (`layout_media_rects`).
    fn append_media_run(
        &mut self,
        input: &MediaInput<'_>,
        items: &[MediaItem],
        source_range: std::ops::Range<usize>,
        indent: f32,
    ) {
        let slots: Vec<MediaSlot> = items
            .iter()
            .map(|item| media_slot(input.media.entry(&item.key)))
            .collect();
        let x = (CONTENT_PAD * self.scale + indent).round();
        let right_pad = (CONTENT_PAD * self.scale).round();
        let col_w = (self.width - x - right_pad).max(20.0 * self.scale);
        let (rects, height) = layout_media_rects(&slots, col_w, self.scale, media_line_height(self.scale));
        let links = &mut self.links;
        let placed = items
            .iter()
            .zip(&rects)
            .map(|(item, rect)| {
                let alt = match input.media.entry(&item.key) {
                    MediaEntryView::Failed(error) if item.alt.is_empty() => error.label().into_owned(),
                    MediaEntryView::Failed(error) => format!("{} — {}", item.alt, error.label()),
                    _ => item.alt.clone(),
                };
                let link = item.link.clone().and_then(|target| links.add(target));
                PlacedMedia { key: item.key.clone(), x: x + rect[0], y: rect[1], w: rect[2], h: rect[3], alt, link }
            })
            .collect();
        let top = self.y;
        let bottom = top + height;
        self.push_read_block(source_range, top, bottom, ReadBlockKind::Media { items: placed });
        self.y = bottom + (BLOCK_GAP * self.scale).round();
    }
}

/// A run of a media paragraph: inline indices of text, or item indices of media elements.
#[derive(Clone, Debug, PartialEq, Eq)]
enum ParagraphPart {
    Text(std::ops::Range<usize>),
    Media(std::ops::Range<usize>),
}

/// Splits a paragraph into text runs and media runs in source order. An inline is an element
/// when its source range is that of the next item (`media_paragraph` emits the items in inline
/// order). Elements separated only by whitespace and line breaks share one media run (badge
/// rows); text runs made only of whitespace are dropped. A block without inlines (Mermaid) is a
/// single media run. Empty when the items do not match the inlines, so the caller keeps text.
fn split_media_paragraph(
    source: &str,
    inlines: &[MarkdownInlineSpan],
    items: &[MediaItem],
) -> Vec<ParagraphPart> {
    if inlines.is_empty() {
        return if items.is_empty() { Vec::new() } else { vec![ParagraphPart::Media(0..items.len())] };
    }
    let blank = |spans: &[MarkdownInlineSpan]| spans.iter().all(|span| is_blank_inline(source, span));
    let mut parts = Vec::new();
    let (mut next, mut text_start) = (0usize, 0usize);
    for (index, span) in inlines.iter().enumerate() {
        if items.get(next).is_none_or(|item| item.source_range != span.source_range) {
            continue;
        }
        if !blank(&inlines[text_start..index]) {
            parts.push(ParagraphPart::Text(text_start..index));
        }
        match parts.last_mut() {
            Some(ParagraphPart::Media(run)) => run.end = next + 1,
            _ => parts.push(ParagraphPart::Media(next..next + 1)),
        }
        next += 1;
        text_start = index + 1;
    }
    if next != items.len() {
        return Vec::new();
    }
    if !blank(&inlines[text_start..]) {
        parts.push(ParagraphPart::Text(text_start..inlines.len()));
    }
    parts
}

fn is_blank_inline(source: &str, span: &MarkdownInlineSpan) -> bool {
    match span.style {
        MarkdownInlineStyle::HardBreak => true,
        MarkdownInlineStyle::Text if span.children.is_empty() => span
            .text_ranges
            .iter()
            .all(|range| source.get(range.clone()).is_some_and(|text| text.trim().is_empty())),
        _ => false,
    }
}

impl StyledText {
    /// Drops whitespace at both ends, so the soft break between an image and its caption does
    /// not open the caption with an empty line.
    fn trim_edge_whitespace(&mut self) {
        let end = self.text.trim_end().len();
        let start = end - self.text[..end].trim_start().len();
        if start == 0 && end == self.text.len() {
            return;
        }
        self.runs.retain_mut(|run| {
            let (from, to) = (run.range.start.max(start), run.range.end.min(end));
            if from >= to {
                return false;
            }
            if let Some(source) = run.source_range.as_mut() {
                source.start += from - run.range.start;
                source.end -= run.range.end - to;
            }
            run.range = from - start..to - start;
            true
        });
        self.text.truncate(end);
        self.text.drain(..start);
    }
}

impl MarkdownReadLayoutCache {
    /// `media_gen` of the `MarkdownMedia` the layout was built with.
    pub(crate) fn media_gen(&self) -> u64 {
        self.media_gen
    }

    /// The layout is valid for this geometry but was built with another media generation, so
    /// the next prepare re-lays it out only because a size or state of an image changed.
    pub(crate) fn media_relayout_pending(
        &self,
        version: u64,
        width: f32,
        scale: f32,
        font_size: f32,
        media_gen: u64,
    ) -> bool {
        self.is_valid_for_geometry(version, width, scale, font_size) && self.media_gen != media_gen
    }

    /// Folder of the document, against which relative image paths resolve. A change rebuilds.
    pub(crate) fn set_media_dir(&mut self, dir: &Path) {
        if self.media_dir != dir {
            self.media_dir = dir.to_path_buf();
            self.invalidate();
        }
    }

    /// The layout answers for `key` and this media generation.
    fn is_current(&self, key: LayoutKey, media_gen: u64) -> bool {
        self.is_valid_for(key) && self.media_gen == media_gen
    }

    /// Every media element with its rectangle `[x, y, w, h]` in content coordinates (`y` from
    /// the top of the document), in document order.
    pub(crate) fn media_blocks(&self) -> impl Iterator<Item = (&MediaKey, [f32; 4])> + '_ {
        self.blocks.iter().flat_map(|block| {
            let (items, top) = match &block.kind {
                ReadBlockKind::Media { items } => (items.as_slice(), block.top),
                _ => (&[][..], 0.0),
            };
            items.iter().map(move |item| (&item.key, [item.x, top + item.y, item.w, item.h]))
        })
    }
}

impl Renderer {
    /// Draws the elements of one media block: the texture where there is one, otherwise a
    /// rounded frame with the alt text (a red border and `alt — reason` when failed).
    fn draw_markdown_media(
        &mut self,
        items: &[PlacedMedia],
        media: &MarkdownMedia,
        frame_x: f32,
        top: f32,
    ) {
        let s = self.scale_factor;
        let pad = (MEDIA_FRAME_PAD * s).round();
        let line_h = media_line_height(s);
        for item in items {
            let (x, y) = (frame_x + item.x, top + item.y);
            let entry = media.entry(&item.key);
            if let MediaEntryView::Ready { texture: Some(texture), .. } = entry {
                self.draw_texture_quad(texture, x, y, item.w, item.h);
                continue;
            }
            let radius = MEDIA_FRAME_RADIUS * s;
            if matches!(entry, MediaEntryView::Failed(_)) {
                let border = faded(self.editor_ui.pick(UiRole::Error, [0.92, 0.45, 0.45, 1.0]), 0.7);
                self.push_rounded_rect_border(
                    x,
                    y,
                    item.w,
                    item.h,
                    radius,
                    1.0,
                    border,
                    self.editor_ui.pick(UiRole::BgMedia, [0.11, 0.12, 0.15, 0.96]),
                );
            } else {
                self.push_rounded_rect(
                    x,
                    y,
                    item.w,
                    item.h,
                    radius,
                    self.editor_ui.pick(UiRole::BgMedia, [0.11, 0.12, 0.15, 0.96]),
                );
            }
            if item.h >= line_h {
                let mut scratch = std::mem::take(&mut self.scratch_buffer);
                self.draw_tree_label_clipped(
                    &item.alt,
                    (x + pad).round(),
                    (y + line_h * 0.82).round(),
                    (item.w - 2.0 * pad).max(0.0),
                    faded(self.theme.line_num, 0.95),
                    MEDIA_ALT_SCALE,
                    &mut scratch,
                );
                self.scratch_buffer = scratch;
            }
        }
    }

    /// The error line of a failed Mermaid block, in the row reserved under the code header.
    fn draw_markdown_code_error(&mut self, code: &CodeBlock, error: &str, left: f32, right: f32, top: f32) {
        let s = self.scale_factor;
        let pad = code_block_padding(s);
        let row_top = top + pad + code_header_height(s);
        let mut scratch = std::mem::take(&mut self.scratch_buffer);
        self.draw_tree_label_clipped(
            error,
            (left + pad).round(),
            (row_top + code.line_height * 0.82).round(),
            right - left - 2.0 * pad,
            self.editor_ui.pick(UiRole::Error, [0.92, 0.45, 0.45, 1.0]),
            MEDIA_ALT_SCALE,
            &mut scratch,
        );
        self.scratch_buffer = scratch;
    }
}

include!("markdown_read_media_tests.rs");
