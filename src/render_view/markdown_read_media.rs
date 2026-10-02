// `markdown_read.rs` include chunk: media blocks of the Reader (image-only paragraphs and Mermaid
// blocks). The layout is a pure function over `(natural size, state)` of every element and the
// column width; `MarkdownMedia` is read only while a layout is built and while a frame is drawn,
// and drawing neither loads, decodes nor allocates beyond the reused scratch buffer.

use std::collections::HashMap;
use std::path::Path;

use crate::app::{MediaItem, document_media};
use crate::markdown_media::{MediaEntryView, MediaKey};

const MEDIA_GAP: f32 = 8.0;
const MEDIA_FRAME_PAD: f32 = 6.0;
const MEDIA_ALT_SCALE: f32 = 0.86;
const MEDIA_FRAME_RADIUS: f32 = 5.0;
const MEDIA_FRAME_BG: [f32; 4] = [0.11, 0.12, 0.15, 0.96];
const MEDIA_ERROR_COLOR: [f32; 4] = [0.92, 0.45, 0.45, 1.0];

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

    /// Lays `block` out as a media block when it is one; false leaves it to the text layout.
    fn try_append_media(&mut self, block: &MarkdownBlock, indent: f32) -> bool {
        let Some(input) = self.media.as_ref() else {
            return false;
        };
        let Some(items) = input.items.get(&block.source_range.start) else {
            return false;
        };
        if input.mermaid_error(block.source_range.start).is_some() {
            return false;
        }
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
        self.push_read_block(block.source_range.clone(), top, bottom, ReadBlockKind::Media { items: placed });
        self.y = bottom + (BLOCK_GAP * self.scale).round();
        true
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
                let border = faded(MEDIA_ERROR_COLOR, 0.7);
                self.push_rounded_rect_border(x, y, item.w, item.h, radius, 1.0, border, MEDIA_FRAME_BG);
            } else {
                self.push_rounded_rect(x, y, item.w, item.h, radius, MEDIA_FRAME_BG);
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
            MEDIA_ERROR_COLOR,
            MEDIA_ALT_SCALE,
            &mut scratch,
        );
        self.scratch_buffer = scratch;
    }
}

include!("markdown_read_media_tests.rs");
