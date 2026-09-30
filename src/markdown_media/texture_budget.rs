//! Pure decisions of the media cache: what to queue, what to re-render, what to evict and
//! which ready pixels to upload first. They work on plain facts so that tests run without GL
//! and `MarkdownMedia::prepare_gpu` only applies their result.

use super::decode::raster_target_size;

/// Invisible textures may hold this many bytes (w * h * 4) on top of the visible ones.
pub(super) const INVISIBLE_TEXTURE_BUDGET: u64 = 128 * 1024 * 1024;

/// A visible raster must differ from its target by more than this share to be re-rendered.
const RERENDER_TOLERANCE: f32 = 0.25;

/// What the queue decision needs to know about one entry.
#[derive(Clone, Copy, Debug)]
pub(super) struct QueueFacts {
    pub visible: bool,
    pub has_texture: bool,
    pub in_flight: bool,
    pub failed: bool,
    pub queued: bool,
}

/// A visible key without a texture that is neither loading, failed nor queued already.
pub(super) fn should_enqueue(facts: &QueueFacts) -> bool {
    facts.visible && !facts.has_texture && !facts.in_flight && !facts.failed && !facts.queued
}

/// Indices of the entries that must be put into the queue now.
pub(super) fn enqueue_indices(facts: &[QueueFacts]) -> Vec<usize> {
    facts
        .iter()
        .enumerate()
        .filter(|(_, facts)| should_enqueue(facts))
        .map(|(index, _)| index)
        .collect()
}

/// Whether a texture of `raster_w` pixels is too far from what a re-render for `display_w`
/// would produce. The comparison is against the CLAMPED target (`raster_target_size`: UI scale,
/// column width and the 4096 px long-side limit), not against `display_w` itself: a tall image
/// can never reach its display width, and comparing with it would re-render forever.
pub(super) fn needs_rerender(natural: (f32, f32), scale: f32, raster_w: u32, display_w: u32) -> bool {
    if display_w == 0 || raster_w == 0 {
        return false;
    }
    let (target_w, _) = raster_target_size(natural.0, natural.1, scale, display_w);
    let target = target_w as f32;
    (raster_w as f32 - target).abs() / target > RERENDER_TOLERANCE
}

/// One live texture (or a slot without one, `bytes == 0`) as the budget sees it.
#[derive(Clone, Copy, Debug)]
pub(super) struct BudgetItem {
    pub bytes: u64,
    pub visible: bool,
    pub last_visible_frame: u64,
}

/// `(all texture bytes, bytes of visible textures)`.
pub(super) fn texture_totals(items: &[BudgetItem]) -> (u64, u64) {
    let mut total = 0u64;
    let mut visible = 0u64;
    for item in items {
        total = total.saturating_add(item.bytes);
        if item.visible {
            visible = visible.saturating_add(item.bytes);
        }
    }
    (total, visible)
}

/// Indices of textures to evict: invisible ones, least recently shown first, until the
/// invisible textures fit `budget`. Visible textures are never evicted, even when they alone
/// exceed the budget, so eviction cannot feed a re-render loop.
pub(super) fn eviction_plan(items: &[BudgetItem], budget: u64) -> Vec<usize> {
    let mut invisible: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| !item.visible && item.bytes > 0)
        .map(|(index, _)| index)
        .collect();
    let mut held: u64 = invisible.iter().map(|&index| items[index].bytes).sum();
    if held <= budget {
        return Vec::new();
    }
    invisible.sort_by_key(|&index| items[index].last_visible_frame);
    let mut evict = Vec::new();
    for index in invisible {
        if held <= budget {
            break;
        }
        held -= items[index].bytes;
        evict.push(index);
    }
    evict
}

/// Order in which ready pixels are uploaded: visible first, arrival order inside each group.
/// `visible[i]` describes the i-th waiting upload; at most `limit` indices come back.
pub(super) fn upload_order(visible: &[bool], limit: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..visible.len()).filter(|&index| visible[index]).collect();
    order.extend((0..visible.len()).filter(|&index| !visible[index]));
    order.truncate(limit);
    order
}

/// The disk cache is trimmed before the first url load of a session and then before every
/// 10th one, which keeps the growth between trims within 10 files of at most 20 MB.
pub(super) fn should_trim_disk_cache(url_loads_started: u64) -> bool {
    url_loads_started % 10 == 0
}
