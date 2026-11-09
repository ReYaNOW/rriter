//! Pure decisions of the media cache: what to queue, what to re-render, what to evict and
//! which ready pixels to upload first. They work on plain facts so that tests run without GL
//! and `MarkdownMedia::prepare_gpu` only applies their result.

use super::decode::{native_raster_ok, raster_target_size};

/// Invisible textures may hold this many bytes (w * h * 4) on top of the visible ones.
pub(super) const INVISIBLE_TEXTURE_BUDGET: u64 = 128 * 1024 * 1024;

/// Share of the invisible budget the prefetch zone may protect; the rest keeps recently shown
/// textures. Bounding by bytes, not by screens, holds the budget on any monitor size.
pub(super) const NEAR_TEXTURE_BUDGET: u64 = INVISIBLE_TEXTURE_BUDGET / 2;

/// Whether the next prefetch key, nearest first, still joins the zone: its texture (held or
/// expected) of `bytes` fits `budget` next to the `held` bytes of the nearer ones. The nearest
/// key always joins, so a single huge image is still prefetched.
pub(super) fn near_fits(held: u64, bytes: u64, budget: u64) -> bool {
    held == 0 || held.saturating_add(bytes) <= budget
}

/// Prefetch order of a key `distance` px from the viewport: keys ahead in the scroll
/// direction (`direction` > 0 below, < 0 above, 0 when still) count at their distance, keys
/// behind at twice it. `above` tells on which side of the viewport the key is.
pub(crate) fn prefetch_rank(distance: f32, above: bool, direction: f32) -> f32 {
    let behind = (direction > 0.0 && above) || (direction < 0.0 && !above);
    if behind { distance * 2.0 } else { distance }
}

/// A visible raster must differ from its target by more than this share to be re-rendered.
const RERENDER_TOLERANCE: f32 = 0.25;

/// What the queue decision needs to know about one entry.
#[derive(Clone, Copy, Debug)]
pub(super) struct QueueFacts {
    pub visible: bool,
    /// Within the prefetch margin around the viewport (see `MarkdownMedia::prepare_gpu`).
    pub near: bool,
    pub state: QueueEntryState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum QueueEntryState {
    Textured,
    InFlight,
    Failed,
    Queued,
    Idle,
}

/// A visible or near key without a texture that is neither loading, failed nor queued
/// already. Near keys are prefetched so they are drawn on the frame they scroll into view.
pub(super) fn should_enqueue(facts: &QueueFacts) -> bool {
    (facts.visible || facts.near) && facts.state == QueueEntryState::Idle
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

/// Whether a bitmap uploaded at its own pixels (`native_raster_ok`) still serves an element
/// drawn `display_w` wide; then a size change never re-renders it, the GPU scales it.
pub(super) fn native_raster_suffices(natural: (f32, f32), scale: f32, display_w: u32) -> bool {
    let (target_w, _) = raster_target_size(natural.0, natural.1, scale, display_w);
    native_raster_ok(natural, target_w)
}

/// One live texture (or a slot without one, `bytes == 0`) as the budget sees it.
#[derive(Clone, Copy, Debug)]
pub(super) struct BudgetItem {
    pub bytes: u64,
    pub visible: bool,
    /// Within the prefetch margin this frame: kept like a visible texture, but its bytes
    /// count against the invisible budget.
    pub near: bool,
    /// Last frame the key was visible or near; 0 for a key never shown nor prefetched.
    pub last_wanted_frame: u64,
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

/// Indices of textures to evict: invisible ones that are not near, least recently wanted
/// first, until the invisible textures fit `budget`. Visible and near textures are never
/// evicted, even when they alone exceed the budget, so eviction cannot feed a re-render loop
/// and a prefetched texture is not dropped before it scrolls into view.
/// `not_evictable` are invisible bytes held outside `items` (pixels waiting for an upload, a
/// partly uploaded texture): they count against the budget but cannot be evicted.
pub(super) fn eviction_plan(items: &[BudgetItem], not_evictable: u64, budget: u64) -> Vec<usize> {
    let mut held: u64 = items
        .iter()
        .filter(|item| !item.visible)
        .map(|item| item.bytes)
        .fold(not_evictable, u64::saturating_add);
    if held <= budget {
        return Vec::new();
    }
    let mut candidates: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| !item.visible && !item.near && item.bytes > 0)
        .map(|(index, _)| index)
        .collect();
    candidates.sort_by_key(|&index| items[index].last_wanted_frame);
    let mut evict = Vec::new();
    for index in candidates {
        if held <= budget {
            break;
        }
        held -= items[index].bytes;
        evict.push(index);
    }
    evict
}

/// Upload priority of one waiting result; lower goes first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum UploadRank {
    Visible,
    Near,
    Far,
}

/// Index of the waiting upload to start next: visible, then near by prefetch rank (nearest
/// first), then the rest; arrival order on ties. The i-th item describes the i-th waiting
/// upload as `(rank, prefetch rank)`; the prefetch rank matters among near keys only.
pub(super) fn next_upload(ranks: impl Iterator<Item = (UploadRank, f32)>) -> Option<usize> {
    let mut best: Option<(usize, UploadRank, f32)> = None;
    for (index, (rank, distance)) in ranks.enumerate() {
        let distance = if rank == UploadRank::Near { distance } else { 0.0 };
        if best.is_none_or(|(_, best_rank, best_distance)| {
            rank < best_rank || (rank == best_rank && distance < best_distance)
        }) {
            best = Some((index, rank, distance));
        }
    }
    best.map(|(index, _, _)| index)
}

/// Whether a result of `bytes` for a key that is neither visible nor near is worth an upload:
/// only while it fits the invisible budget next to the `invisible_bytes` already held. Such a
/// texture is the first to be evicted, so uploading it over the budget only costs frame time.
pub(super) fn far_upload_fits(invisible_bytes: u64, bytes: u64, budget: u64) -> bool {
    invisible_bytes.saturating_add(bytes) <= budget
}

/// Index of the queued key to start next: the first visible one in queue order, else the
/// near one closest to the viewport; `None` when every queued key is far (the caller then
/// takes the oldest, if a slot may go to a far key). `ranks[i]` is `None` for a far key,
/// `Some(-1.0)` for a visible one and `Some(distance)` for a near one.
pub(super) fn next_to_start(ranks: impl Iterator<Item = Option<f32>>) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (index, rank) in ranks.enumerate() {
        if let Some(rank) = rank
            && best.is_none_or(|(_, held)| rank < held)
        {
            best = Some((index, rank));
        }
    }
    best.map(|(index, _)| index)
}

/// Far loads (natural sizes of the whole document, keys far from the viewport) may take all
/// slots but one, so a key scrolling near never waits behind a full set of far loads.
pub(super) fn far_load_may_start(slots_used: usize, max_slots: usize) -> bool {
    slots_used + 1 < max_slots
}

/// The disk cache is trimmed before the first url load of a session and then before every
/// 10th one, which keeps the growth between trims within 10 files of at most 20 MB.
pub(super) fn should_trim_disk_cache(url_loads_started: u64) -> bool {
    url_loads_started.is_multiple_of(10)
}
