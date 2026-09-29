use crate::pdf::{DocGens, DocId, PageGeom, PageLink, PageText, PdfEvent, PdfRequest, PtRect};
use crate::scroll::ScrollState;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

mod engine;
mod input;
pub mod text;

pub use text::{PdfMatch, PdfPress, PdfSearch, PdfSelection};
#[cfg(test)]
pub(crate) use engine::engine_after_install;

#[derive(Clone, Debug)]
pub enum PdfEngineState {
    NotStarted,
    Starting,
    Ready,
    Missing { message: String, installable: bool },
    Failed(String),
    /// Engine download in progress. `prev` is the `Missing` message restored on cancel;
    /// `progress` is the last installer log line shown under the headline.
    Installing { prev: String, progress: String },
}

impl PdfEngineState {
    /// Status-screen text for the engine; only `Failed` allocates.
    pub fn label(&self) -> std::borrow::Cow<'_, str> {
        match self {
            Self::Missing { message, .. } => std::borrow::Cow::Borrowed(message.as_str()),
            Self::Failed(message) => std::borrow::Cow::Owned(format!("движок PDF остановлен: {message}. Перезапустите RRiter")),
            Self::Installing { .. } => std::borrow::Cow::Borrowed("Установка движка PDF…"),
            Self::NotStarted | Self::Starting | Self::Ready => std::borrow::Cow::Borrowed("Загрузка документа PDF…"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PdfPhase {
    EngineMissing { error: Option<String> },
    EngineStarting,
    Loading,
    Ready,
    Error(String),
    PasswordRequired,
}

#[derive(Clone, Copy, Debug)]
pub struct PageTexture {
    pub tex: glow::Texture,
    pub width_px: u32,
    pub height_px: u32,
    pub r#gen: u32,
}

#[derive(Debug)]
pub struct PendingBitmap {
    pub page: usize,
    pub r#gen: u32,
    pub width_px: u32,
    pub height_px: u32,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PdfLayout {
    pub margin: i32,
    pub gap: i32,
    pub page_w: i32,
    pub rows: Vec<(i32, i32)>,
    pub total_h: i32,
}

impl PdfLayout {
    pub fn compute(pages: &[PageGeom], viewport_w: u32, s: f32) -> Self {
        let margin = (16.0 * s).round().max(0.0) as i32;
        let gap = (8.0 * s).round().max(0.0) as i32;
        let page_w = (viewport_w as i32 - 2 * margin).max(64);
        let mut rows = Vec::with_capacity(pages.len());
        let mut y = margin;
        for page in pages {
            let ratio = if page.width_pt > 0.0 && page.height_pt > 0.0 {
                page.height_pt / page.width_pt
            } else {
                1.0
            };
            let h = (page_w as f32 * ratio).round().clamp(1.0, i32::MAX as f32) as i32;
            rows.push((y, h));
            y = y.saturating_add(h).saturating_add(gap);
        }
        let total_h = if let Some((last_y, last_h)) = rows.last() {
            last_y.saturating_add(*last_h).saturating_add(margin)
        } else {
            margin.saturating_mul(2)
        };
        Self { margin, gap, page_w, rows, total_h }
    }

    pub fn page_at_y(&self, y: i32) -> usize {
        let mut low = 0;
        let mut high = self.rows.len();
        while low < high {
            let mid = low + (high - low) / 2;
            if self.rows[mid].0 <= y { low = mid + 1; } else { high = mid; }
        }
        low.saturating_sub(1).min(self.rows.len().saturating_sub(1))
    }

    pub fn raster_size(page_w: i32, geom: &PageGeom) -> (u32, u32) {
        if geom.width_pt <= 0.0 || geom.height_pt <= 0.0 { return (64, 64); }
        let max_by_height = (4096.0 * geom.width_pt / geom.height_pt).floor().max(1.0) as u32;
        let width = (page_w.max(1) as u32).min(4096).min(max_by_height).max(1);
        let height = (width as f32 * geom.height_pt / geom.width_pt).round().clamp(1.0, 4096.0) as u32;
        (width, height)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PdfEventOutcome { Redraw, Ignore, Bitmap }

pub struct PdfTabState {
    pub path: PathBuf,
    pub doc: Option<DocId>,
    pub gens: Arc<DocGens>,
    pub phase: PdfPhase,
    pub pages: Vec<PageGeom>,
    pub layout: PdfLayout,
    pub scroll: ScrollState,
    pub textures: HashMap<usize, PageTexture>,
    pub pending_bitmaps: Vec<PendingBitmap>,
    pub requested: HashSet<(usize, u32)>,
    pub text: Vec<Option<Arc<PageText>>>,
    pub links: Vec<Vec<PageLink>>,
    pub text_requested: Vec<bool>,
    pub search: PdfSearch,
    pub selection: Option<PdfSelection>,
    pub pending_copy: Option<PdfSelection>,
    pub restore: Option<(usize, f32)>,
    pub hover_link: Option<(usize, usize)>,
    pub dark: bool,
    pub viewport: (u32, u32),
    pub line_rects_buf: Vec<PtRect>,
    pub line_boxes: Vec<Option<Vec<PtRect>>>,
    /// Tab body rectangle of the last prepared frame; maps window points to pages.
    pub body: (f32, f32, f32, f32),
    /// Left press on the page area (see `PdfPress`); changed only through the tab's press methods.
    pub press: Option<PdfPress>,
    pub(crate) layout_scale: f32,
    pub(crate) layout_dirty: bool,
    pub(crate) status_label: String,
}

impl PdfTabState {
    pub fn new(path: PathBuf, gens: Arc<DocGens>, phase: PdfPhase) -> Self {
        Self { path, doc: None, gens, phase, pages: Vec::new(), layout: PdfLayout::default(),
            scroll: ScrollState::new(15.0), textures: HashMap::new(), pending_bitmaps: Vec::new(),
            requested: HashSet::new(), text: Vec::new(), links: Vec::new(), text_requested: Vec::new(),
            search: PdfSearch::default(), selection: None, pending_copy: None, restore: None,
            hover_link: None, dark: false, viewport: (0, 0), line_rects_buf: Vec::new(), line_boxes: Vec::new(), body: (0.0, 0.0, 0.0, 0.0), press: None, layout_scale: 1.0, layout_dirty: true, status_label: String::new() }
    }

    /// Applies an owned event; a `Page` bitmap is moved into `pending_bitmaps` without a copy.
    pub fn apply_event_owned(&mut self, ev: PdfEvent) -> PdfEventOutcome {
        match ev {
            PdfEvent::Page { page, r#gen, width_px, height_px, rgba, .. } => {
                self.requested.remove(&(page, r#gen));
                if r#gen != self.gens.render.load(std::sync::atomic::Ordering::Relaxed)
                    || !self.gens.contains(page) || page >= self.pages.len()
                    || (width_px as usize).checked_mul(height_px as usize).and_then(|size| size.checked_mul(4)) != Some(rgba.len()) { return PdfEventOutcome::Ignore; }
                self.pending_bitmaps.push(PendingBitmap { page, r#gen, width_px, height_px, rgba });
                PdfEventOutcome::Bitmap
            }
            other => self.apply_event(&other),
        }
    }

    /// Borrowed variant for every event except `Page`: those carry the bitmap and must go through
    /// `apply_event_owned`, so this ignores them.
    pub fn apply_event(&mut self, ev: &PdfEvent) -> PdfEventOutcome {
        match ev {
            PdfEvent::LoadStarted => { self.phase = PdfPhase::Loading; PdfEventOutcome::Redraw }
            PdfEvent::EngineUnavailable(error) => { self.phase = PdfPhase::EngineMissing { error: Some(error.clone()) }; PdfEventOutcome::Redraw }
            PdfEvent::Opened { pages, .. } => {
                self.pages.clone_from(pages);
                self.phase = if pages.is_empty() { PdfPhase::Error("в документе нет страниц".to_owned()) } else { PdfPhase::Ready };
                self.text = vec![None; pages.len()];
                self.links = vec![Vec::new(); pages.len()];
                self.text_requested = vec![false; pages.len()];
                self.line_boxes = vec![None; pages.len()];
                self.layout_dirty = true;
                PdfEventOutcome::Redraw
            }
            PdfEvent::OpenFailed { error, .. } => {
                self.phase = if matches!(error, crate::pdf::PdfError::PasswordRequired) { PdfPhase::PasswordRequired } else { PdfPhase::Error(error.message()) };
                PdfEventOutcome::Redraw
            }
            PdfEvent::RenderSkipped { page, r#gen, .. } => { self.requested.remove(&(*page, *r#gen)); PdfEventOutcome::Redraw }
            PdfEvent::Text { page, text, links, .. } if *page < self.pages.len() && *page < self.text.len() => {
                self.text_requested[*page] = false;
                if self.text[*page].is_some() { return PdfEventOutcome::Ignore; }
                let mut boxes = Vec::new();
                text::line_rects(&text.chars, 0, text.chars.len(), &mut boxes);
                self.line_boxes[*page] = Some(boxes);
                self.text[*page] = Some(Arc::clone(text));
                self.links[*page] = links.clone();
                self.resume_pending_jump();
                PdfEventOutcome::Redraw
            }
            PdfEvent::TextFailed { page, .. } if *page < self.pages.len() && *page < self.text.len() => {
                self.text_requested[*page] = false;
                if self.text[*page].is_none() {
                    self.text[*page] = Some(Arc::new(PageText::default()));
                    self.line_boxes[*page] = Some(Vec::new());
                }
                self.resume_pending_jump();
                PdfEventOutcome::Redraw
            }
            PdfEvent::SearchPage { r#gen, page, matches, .. } if *r#gen == self.search.r#gen => {
                self.search.matches.extend(matches.iter().map(|(start, end)| PdfMatch { page: *page, start: *start, end: *end }));
                if self.search.current.is_none() && !self.search.matches.is_empty() {
                    self.search.current = Some(0);
                    self.goto_current_match();
                }
                PdfEventOutcome::Redraw
            }
            PdfEvent::SearchDone { r#gen, .. } if *r#gen == self.search.r#gen => { self.search.done = true; PdfEventOutcome::Redraw }
            _ => PdfEventOutcome::Ignore,
        }
    }

    pub fn set_viewport(&mut self, w: u32, h: u32, s: f32) {
        let layout_changed = self.viewport.0 != w || self.layout_scale != s || self.layout_dirty;
        self.viewport = (w, h);
        if layout_changed {
            self.layout_scale = s;
            self.layout = PdfLayout::compute(&self.pages, w, s);
            self.bump_render_gen();
            self.layout_dirty = false;
        }
        // The restore waits until the document is opened and the body has a size;
        // frames drawn while `Loading` must not consume it.
        if !self.pages.is_empty() && w > 0 && h > 0 && let Some((page, frac)) = self.restore.take() {
            let page = page.min(self.pages.len() - 1);
            let (y, ph) = self.layout.rows[page];
            self.scroll.jump_to(y as f32 + frac.clamp(0.0, 1.0) * ph as f32);
        }
        self.clamp_scroll();
        let wanted = self.wanted_range();
        if wanted.is_empty() { self.gens.clear_wanted(); } else { self.gens.set_wanted(wanted.start, wanted.end - 1); }
        if matches!(self.phase, PdfPhase::Ready) {
            let (page, _) = self.anchor();
            self.status_label = format!("стр. {} / {}", page + 1, self.page_count());
        } else {
            self.status_label.clear();
        }
    }

    pub fn bump_render_gen(&self) {
        self.gens.render.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn clamp_scroll(&mut self) { self.scroll.clamp_target(0.0, self.layout.total_h.saturating_sub(self.viewport.1 as i32).max(0) as f32); }
    pub fn anchor(&self) -> (usize, f32) {
        if self.layout.rows.is_empty() { return (0, 0.0); }
        let offset = self.scroll.current.round() as i32;
        let mut low = 0usize;
        let mut high = self.layout.rows.len();
        while low < high {
            let mid = low + (high - low) / 2;
            let (y, h) = self.layout.rows[mid];
            if y.saturating_add(h) <= offset { low = mid + 1; } else { high = mid; }
        }
        let page = low.min(self.layout.rows.len() - 1);
        let (y, h) = self.layout.rows[page];
        (page, ((offset - y) as f32 / h.max(1) as f32).clamp(0.0, 1.0))
    }
    /// Position written to the session: the pending restore until `set_viewport`
    /// applies it (engine missing, still loading), then the live anchor.
    pub fn session_position(&self) -> (usize, f32) { self.restore.unwrap_or_else(|| self.anchor()) }
    pub fn visible_range(&self) -> Range<usize> {
        if self.layout.rows.is_empty() { return 0..0; }
        let top = self.scroll.current.round() as i32;
        let bottom = top.saturating_add(self.viewport.1 as i32);
        let first = self.layout.page_at_y(top);
        let mut last = self.layout.page_at_y(bottom).saturating_add(1);
        if self.layout.rows[first].0 > bottom { last = first; }
        first..last.min(self.pages.len())
    }
    pub fn wanted_range(&self) -> Range<usize> {
        let visible = self.visible_range();
        visible.start.saturating_sub(2)..visible.end.saturating_add(2).min(self.pages.len())
    }
    pub fn take_render_requests(&mut self, dark: bool) -> Vec<PdfRequest> {
        let Some(id) = self.doc else { return Vec::new() };
        self.dark = dark;
        let r#gen = self.gens.render.load(std::sync::atomic::Ordering::Relaxed);
        let wanted = self.wanted_range();
        let mut requests = Vec::new();
        for page in wanted {
            if self.textures.get(&page).is_some_and(|texture| texture.r#gen == r#gen)
                || !self.requested.insert((page, r#gen)) { continue; }
            let Some(geom) = self.pages.get(page) else { continue };
            let (width_px, _) = PdfLayout::raster_size(self.layout.page_w, geom);
            requests.push(PdfRequest::Render { id, page, width_px, r#gen, dark });
        }
        requests
    }
    pub fn scroll_to_page(&mut self, page: usize, y_pt: Option<f32>) {
        if self.pages.is_empty() { return; }
        let page = page.min(self.pages.len() - 1);
        let (y, h) = self.layout.rows[page];
        let frac = y_pt.map(|value| (value / self.pages[page].height_pt.max(1.0)).clamp(0.0, 1.0)).unwrap_or(0.0);
        self.scroll.animate_to(y as f32 + frac * h as f32); self.clamp_scroll();
    }
    pub fn scroll_by(&mut self, dy: f32) {
        self.scroll.anim_speed = 7.0;
        self.scroll.scroll_by(dy);
        self.clamp_scroll();
    }
    pub fn page_up(&mut self) { self.scroll_by(-(self.viewport.1 as f32)); }
    pub fn page_down(&mut self) { self.scroll_by(self.viewport.1 as f32); }
    pub fn home(&mut self) { self.scroll.jump_to(0.0); }
    pub fn end(&mut self) { self.scroll.jump_to(self.layout.total_h.saturating_sub(self.viewport.1 as i32).max(0) as f32); }
    pub fn page_count(&self) -> usize { self.pages.len() }
}

#[cfg(test)]
mod tests {
    use super::{PageGeom, PdfEventOutcome, PdfLayout, PdfPhase, PdfTabState};
    use crate::pdf::{DocGens, PdfEvent, PdfRequest};
    use std::path::PathBuf;
    use std::sync::Arc;
    #[test]
    fn letter_layout_and_raster_limits_are_stable() {
        let letter = PageGeom { width_pt: 612.0, height_pt: 792.0 };
        let layout = PdfLayout::compute(&[letter; 3], 1000, 1.0);
        assert_eq!((layout.margin, layout.gap, layout.page_w), (16, 8, 968));
        assert_eq!(layout.rows, vec![(16, 1253), (1277, 1253), (2538, 1253)]);
        assert_eq!(layout.total_h, 3807);
        assert_eq!(layout.page_at_y(1276), 0); assert_eq!(layout.page_at_y(1277), 1);
        assert_eq!(PdfLayout::raster_size(968, &letter), (968, 1253));
        assert_eq!(PdfLayout::raster_size(5000, &letter), (3165, 4096));
        assert_eq!(PdfLayout::raster_size(968, &PageGeom { width_pt: 0.0, height_pt: 0.0 }), (64, 64));
        assert_eq!(PdfLayout::compute(&[letter], 10, 1.0).page_w, 64);
        assert_eq!(PdfLayout::compute(&[], 1000, 1.0).total_h, 32);
    }

    #[test]
    fn render_requests_are_deduplicated_and_rejected_by_generation_and_size() {
        let geom = PageGeom { width_pt: 612.0, height_pt: 792.0 };
        let pages = vec![geom; 3];
        let mut tab = PdfTabState::new(PathBuf::from("a.pdf"), Arc::new(DocGens::new()), PdfPhase::Loading);
        tab.doc = Some(crate::pdf::DocId(1));
        tab.apply_event(&PdfEvent::Opened { id: crate::pdf::DocId(1), pages });
        tab.set_viewport(1000, 800, 1.0);
        let requests = tab.take_render_requests(false);
        assert_eq!(requests.len(), 3);
        assert!(requests.iter().all(|request| matches!(request, PdfRequest::Render { width_px: 968, r#gen: 1, .. })));
        assert!(tab.take_render_requests(false).is_empty());
        let malformed = PdfEvent::Page { id: crate::pdf::DocId(1), page: 0, r#gen: 1, width_px: 2, height_px: 2, rgba: vec![0; 3] };
        assert_eq!(tab.apply_event_owned(malformed), PdfEventOutcome::Ignore);
        assert!(tab.pending_bitmaps.is_empty());
        tab.set_viewport(900, 800, 1.0);
        assert_eq!(tab.gens.render.load(std::sync::atomic::Ordering::Relaxed), 2);
        let stale = PdfEvent::Page { id: crate::pdf::DocId(1), page: 0, r#gen: 1, width_px: 2, height_px: 2, rgba: vec![0; 16] };
        assert_eq!(tab.apply_event_owned(stale), PdfEventOutcome::Ignore);
        let bitmap = vec![7; 16];
        let bitmap_ptr = bitmap.as_ptr();
        let current = PdfEvent::Page { id: crate::pdf::DocId(1), page: 0, r#gen: 2, width_px: 2, height_px: 2, rgba: bitmap };
        assert_eq!(tab.apply_event_owned(current), PdfEventOutcome::Bitmap);
        assert_eq!(tab.pending_bitmaps.len(), 1);
        assert_eq!(tab.pending_bitmaps[0].rgba.as_ptr(), bitmap_ptr, "bitmap must be moved, not copied");
    }

    #[test]
    fn scroll_navigation_clamps_to_document_bounds() {
        let geom = PageGeom { width_pt: 612.0, height_pt: 792.0 };
        let mut tab = PdfTabState::new(PathBuf::from("a.pdf"), Arc::new(DocGens::new()), PdfPhase::Loading);
        tab.apply_event(&PdfEvent::Opened { id: crate::pdf::DocId(1), pages: vec![geom; 3] });
        tab.set_viewport(1000, 800, 1.0);
        tab.scroll_by(f32::MAX);
        assert_eq!(tab.scroll.target, tab.layout.total_h.saturating_sub(800) as f32);
        tab.end();
        assert_eq!(tab.scroll.current, tab.scroll.target);
        tab.home();
        assert_eq!(tab.scroll.current, 0.0);
    }

    #[test]
    fn anchor_and_restored_page_clamp_are_stable() {
        let letter = PageGeom { width_pt: 612.0, height_pt: 792.0 };
        let pages = vec![letter; 3];
        let mut tab = PdfTabState::new(PathBuf::from("a.pdf"), Arc::new(DocGens::new()), PdfPhase::Loading);
        tab.apply_event(&PdfEvent::Opened { id: crate::pdf::DocId(1), pages: pages.clone() });
        tab.set_viewport(1000, 500, 1.0);
        tab.scroll.current = 1300.0;
        assert_eq!(tab.anchor(), (1, 23.0 / 1253.0));

        tab.restore = Some((usize::MAX, 0.5));
        tab.set_viewport(1000, 500, 1.0);
        assert_eq!(tab.anchor().0, 2);
    }

    #[test]
    fn restore_survives_loading_frames_and_applies_after_opened() {
        let letter = PageGeom { width_pt: 612.0, height_pt: 792.0 };
        let mut tab = PdfTabState::new(PathBuf::from("a.pdf"), Arc::new(DocGens::new()), PdfPhase::Loading);
        tab.restore = Some((1, 0.25));
        tab.set_viewport(1000, 500, 1.0);
        tab.set_viewport(1000, 500, 1.0);
        assert_eq!(tab.restore, Some((1, 0.25)));
        assert_eq!(tab.session_position(), (1, 0.25));

        tab.apply_event(&PdfEvent::Opened { id: crate::pdf::DocId(1), pages: vec![letter; 3] });
        tab.set_viewport(1000, 500, 1.0);
        assert_eq!(tab.restore, None);
        let (page, frac) = tab.anchor();
        assert_eq!(page, 1);
        assert!((frac - 0.25).abs() <= 1.0 / 1253.0, "frac {frac}");
        assert_eq!(tab.session_position(), (page, frac));
    }
}
