//! Text layer of a PDF tab: char geometry helpers, the page-text cache,
//! search state and selection. Everything here is pure state; the `App`
//! wrappers (clipboard, links, worker sends) live in `input.rs`.
use super::{PdfPhase, PdfTabState};
use crate::pdf::{PageChar, PageLink, PageText, PdfRequest, PtRect};
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub struct PdfSearch {
    pub r#gen: u32,
    pub query: String,
    pub matches: Vec<PdfMatch>,
    pub current: Option<usize>,
    pub done: bool,
    pub pending_jump: Option<(u32, usize)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PdfMatch {
    pub page: usize,
    pub start: u32,
    pub end: u32,
}

/// `(page, char index)` endpoints; `head` is the moving end, both are inclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PdfSelection {
    pub anchor: (usize, usize),
    pub head: (usize, usize),
}

/// Left press on the page area: where it started, the link under it and the drag progress.
/// Only `PdfTabState::{begin_press, drag_to, end_press, cancel_press}` change it.
#[derive(Clone, Copy, Debug)]
pub struct PdfPress {
    pub x: f32,
    pub y: f32,
    pub link: Option<(usize, usize)>,
    pub dragging: bool,
    anchor: Option<(usize, usize)>,
}

/// Edge autoscroll speed in physical px per second at scale 1.0 (about 12 px per 60 Hz frame).
/// The caller passes the app's frame dt, which `animation_dt` already caps for stalled frames.
const AUTOSCROLL_SPEED_PX_S: f32 = 720.0;

impl PdfSelection {
    pub fn ordered(&self) -> ((usize, usize), (usize, usize)) {
        if self.anchor <= self.head { (self.anchor, self.head) } else { (self.head, self.anchor) }
    }
}

fn rect_is_empty(rect: &PtRect) -> bool {
    !(rect.w > 0.0 && rect.h > 0.0)
}

fn y_overlaps(a: &PtRect, b: &PtRect) -> bool {
    a.y < b.y + b.h && b.y < a.y + a.h
}

/// One rectangle per text line of `chars[start..end]`: neighbours whose Y ranges
/// intersect are merged, empty rectangles (line breaks) are skipped.
pub fn line_rects(chars: &[PageChar], start: usize, end: usize, out: &mut Vec<PtRect>) {
    let end = end.min(chars.len());
    let start = start.min(end);
    let mut current: Option<PtRect> = None;
    for item in &chars[start..end] {
        let rect = item.rect;
        if rect_is_empty(&rect) { continue; }
        if let Some(cur) = current.as_mut() && y_overlaps(cur, &rect) {
            let x0 = cur.x.min(rect.x);
            let y0 = cur.y.min(rect.y);
            let x1 = (cur.x + cur.w).max(rect.x + rect.w);
            let y1 = (cur.y + cur.h).max(rect.y + rect.h);
            *cur = PtRect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
            continue;
        }
        if let Some(done) = current.take() { out.push(done); }
        current = Some(rect);
    }
    if let Some(done) = current { out.push(done); }
}

/// Char under the point; otherwise the nearest char of the nearest line
/// (min |dy| to the char centre, then min |dx|). Empty text gives `None`.
pub fn hit_test_char(chars: &[PageChar], x_pt: f32, y_pt: f32) -> Option<usize> {
    let mut nearest: Option<(usize, f32)> = None;
    for (index, item) in chars.iter().enumerate() {
        let rect = item.rect;
        if rect_is_empty(&rect) { continue; }
        if x_pt >= rect.x && x_pt <= rect.x + rect.w && y_pt >= rect.y && y_pt <= rect.y + rect.h {
            return Some(index);
        }
        let dy = (y_pt - (rect.y + rect.h * 0.5)).abs();
        if nearest.is_none_or(|(_, best)| dy < best) { nearest = Some((index, dy)); }
    }
    let line = chars[nearest?.0].rect;
    let mut pick: Option<(usize, f32)> = None;
    for (index, item) in chars.iter().enumerate() {
        let rect = item.rect;
        if rect_is_empty(&rect) || !y_overlaps(&line, &rect) { continue; }
        let dx = if x_pt < rect.x { rect.x - x_pt } else if x_pt > rect.x + rect.w { x_pt - (rect.x + rect.w) } else { 0.0 };
        if pick.is_none_or(|(_, best)| dx < best) { pick = Some((index, dx)); }
    }
    pick.map(|(index, _)| index)
}

pub fn hit_test_link(links: &[PageLink], x_pt: f32, y_pt: f32) -> Option<usize> {
    links.iter().position(|link| {
        x_pt >= link.rect.x && x_pt <= link.rect.x + link.rect.w
            && y_pt >= link.rect.y && y_pt <= link.rect.y + link.rect.h
    })
}

/// Text of the selection with pages joined by `\n`; `None` while any page of the range has no text yet.
pub fn selection_text(text: &[Option<Arc<PageText>>], sel: &PdfSelection) -> Option<String> {
    let ((first_page, first_char), (last_page, last_char)) = sel.ordered();
    let mut out = String::new();
    for page in first_page..=last_page {
        let chars = &text.get(page)?.as_ref()?.chars;
        let start = if page == first_page { first_char } else { 0 };
        let end = if page == last_page { last_char.saturating_add(1) } else { chars.len() };
        if page != first_page { out.push('\n'); }
        let end = end.min(chars.len());
        out.extend(chars[start.min(end)..end].iter().map(|item| item.ch));
    }
    Some(out)
}

/// Same semantics as the editor search: literal query, case flag.
pub fn search_pattern(query: &str, case_sensitive: bool) -> String {
    if case_sensitive { regex::escape(query) } else { format!("(?i){}", regex::escape(query)) }
}

/// Only plain web links are handed to the OS; anything else in a document is ignored.
pub fn link_url_allowed(url: &str) -> bool {
    url.len() <= 2048
        && !url.chars().any(|ch| ch.is_control() || ch.is_whitespace())
        && url::Url::parse(url).is_ok_and(|parsed| matches!(parsed.scheme(), "http" | "https"))
}

impl PdfTabState {
    /// Char under a window point on an already cached page (clamped into the nearest page).
    pub fn hit_char_at(&self, x: f32, y: f32) -> Option<(usize, usize)> {
        let (page, x_pt, y_pt) = self.point_to_page_pt(x, y)?;
        let text = self.text.get(page)?.as_ref()?;
        hit_test_char(&text.chars, x_pt, y_pt).map(|idx| (page, idx))
    }

    /// Starts a left press on the page area; it becomes a selection drag or a click on release.
    pub fn begin_press(&mut self, x: f32, y: f32, link: Option<(usize, usize)>) {
        self.press = Some(PdfPress { x, y, link, dragging: false, anchor: None });
    }

    /// Drops a press whose release will never arrive (focus loss, tab switch).
    pub fn cancel_press(&mut self) {
        self.press = None;
    }

    /// Pointer move with the button held. `false` = no PDF press is active. Past `threshold`
    /// pixels the press turns into a drag and the selection follows the pointer.
    pub fn drag_to(&mut self, x: f32, y: f32, threshold: f32) -> bool {
        let Some(mut press) = self.press else { return false };
        if !press.dragging {
            if (x - press.x).hypot(y - press.y) < threshold { return true; }
            press.dragging = true;
            press.anchor = self.hit_char_at(press.x, press.y);
        }
        if let Some(head) = self.hit_char_at(x, y) {
            let anchor = *press.anchor.get_or_insert(head);
            self.pending_copy = None;
            self.selection = Some(PdfSelection { anchor, head });
        }
        self.press = Some(press);
        true
    }

    /// Left release. Returns the finished press (`None` = it was not ours); a press
    /// that never became a drag also clears the selection, as a click does.
    pub fn end_press(&mut self) -> Option<PdfPress> {
        let press = self.press.take()?;
        if !press.dragging { self.clear_selection(); }
        Some(press)
    }

    /// Edge autoscroll by `dt` seconds while a drag is held near the top or bottom of the
    /// body; the selection head keeps following the (stationary) pointer. `true` = scrolled.
    pub fn autoscroll_step(&mut self, x: f32, y: f32, scale: f32, dt: f32) -> bool {
        if !self.press.is_some_and(|press| press.dragging) { return false; }
        let (_, body_y, _, body_h) = self.body;
        let edge = (24.0 * scale).round();
        if body_h <= 2.0 * edge { return false; }
        if !dt.is_finite() || dt <= 0.0 { return false; }
        let step = AUTOSCROLL_SPEED_PX_S * scale * dt;
        let delta = if y <= body_y + edge { -step } else if y >= body_y + body_h - edge { step } else { return false };
        let before = self.scroll.target;
        self.scroll_by(delta);
        if self.scroll.target == before { return false; }
        if let Some(head) = self.hit_char_at(x, y) && let Some(sel) = self.selection.as_mut() { sel.head = head; }
        true
    }

    /// Page under a window point plus the point in page points (Y down); the point is
    /// clamped into the nearest page so a drag through gaps and margins keeps working.
    pub fn point_to_page_pt(&self, x: f32, y: f32) -> Option<(usize, f32, f32)> {
        if !(x.is_finite() && y.is_finite()) || self.layout.rows.is_empty() || self.pages.len() != self.layout.rows.len() {
            return None;
        }
        let (body_x, body_y, body_w, _) = self.body;
        let content_y = y - body_y + self.scroll.current.round();
        let page = self.layout.page_at_y(content_y.floor() as i32);
        let (row_y, row_h) = *self.layout.rows.get(page)?;
        let geom = self.pages.get(page)?;
        let page_w = self.layout.page_w.max(1) as f32;
        let left = (body_x + (body_w - page_w) * 0.5).round();
        let fx = ((x - left) / page_w).clamp(0.0, 1.0);
        let fy = ((content_y - row_y as f32) / row_h.max(1) as f32).clamp(0.0, 1.0);
        Some((page, fx * geom.width_pt, fy * geom.height_pt))
    }

    fn push_text_request(&mut self, id: crate::pdf::DocId, page: usize, out: &mut Vec<PdfRequest>) {
        if page < self.text.len() && page < self.text_requested.len() && self.text[page].is_none() && !self.text_requested[page] {
            self.text_requested[page] = true;
            out.push(PdfRequest::PageText { id, page });
        }
    }

    /// Text for the visible pages +-1 plus whatever a pending jump or copy waits for.
    pub fn take_text_requests(&mut self) -> Vec<PdfRequest> {
        let Some(id) = self.doc else { return Vec::new() };
        let mut out = Vec::new();
        let visible = self.visible_range();
        for page in visible.start.saturating_sub(1)..visible.end.saturating_add(1).min(self.pages.len()) {
            self.push_text_request(id, page, &mut out);
        }
        if let Some((_, idx)) = self.search.pending_jump && let Some(item) = self.search.matches.get(idx) {
            let page = item.page;
            self.push_text_request(id, page, &mut out);
        }
        if let Some(sel) = self.pending_copy {
            let ((first, _), (last, _)) = sel.ordered();
            for page in first..=last.min(self.pages.len().saturating_sub(1)) {
                self.push_text_request(id, page, &mut out);
            }
        }
        out
    }

    /// Starts a new search and returns the worker request. The single owner of
    /// search-generation bumps: state and worker cancellation see the same value.
    pub fn begin_search(&mut self, query: &str, case_sensitive: bool) -> Option<PdfRequest> {
        let r#gen = self.gens.search.fetch_add(1, std::sync::atomic::Ordering::AcqRel).wrapping_add(1);
        self.search.r#gen = r#gen;
        self.search.query.clear();
        self.search.query.push_str(query);
        self.search.matches.clear();
        self.search.current = None;
        self.search.pending_jump = None;
        self.search.done = false;
        let id = self.doc;
        if query.is_empty() || !matches!(self.phase, PdfPhase::Ready) || id.is_none() {
            self.search.done = true;
            return None;
        }
        Some(PdfRequest::Search { id: id?, query: search_pattern(query, case_sensitive), r#gen })
    }

    /// Moves `current` cyclically and scrolls to it; returns the match page.
    pub fn jump_match(&mut self, forward: bool) -> Option<usize> {
        let count = self.search.matches.len();
        if count == 0 { return None; }
        let next = match self.search.current {
            Some(current) if forward => (current + 1) % count,
            Some(current) => (current + count - 1) % count,
            None if forward => 0,
            None => count - 1,
        };
        self.search.current = Some(next);
        self.goto_current_match();
        self.search.matches.get(next).map(|item| item.page)
    }

    /// Scrolls to `current` without moving it; waits for the page text when it is not cached.
    pub fn goto_current_match(&mut self) {
        let Some(idx) = self.search.current else { return };
        if let Some((page, y_pt)) = self.match_rect_top(idx) {
            self.search.pending_jump = None;
            self.scroll_to_match(page, y_pt);
        } else {
            self.search.pending_jump = Some((self.search.r#gen, idx));
        }
    }

    /// Page and top Y (pt) of the first line of match `idx`; `None` until the page text is cached.
    pub fn match_rect_top(&self, idx: usize) -> Option<(usize, f32)> {
        let item = self.search.matches.get(idx)?;
        let text = self.text.get(item.page)?.as_ref()?;
        let mut lines = Vec::new();
        line_rects(&text.chars, item.start as usize, item.end as usize, &mut lines);
        Some((item.page, lines.first().map_or(0.0, |rect| rect.y)))
    }

    fn scroll_to_match(&mut self, page: usize, y_pt: f32) {
        let (Some(&(row_y, row_h)), Some(geom)) = (self.layout.rows.get(page), self.pages.get(page)) else { return };
        let k = row_h as f32 / geom.height_pt.max(1.0);
        // A match near the top of its page aligns to the page top instead of showing the previous page.
        let target = (row_y as f32 + (y_pt * k).round() - self.viewport.1 as f32 / 3.0).max(row_y as f32);
        self.scroll.animate_to(target);
        self.clamp_scroll();
    }

    /// Runs a jump that was waiting for page text; drops it when the search moved on.
    pub(super) fn resume_pending_jump(&mut self) {
        let Some((r#gen, idx)) = self.search.pending_jump else { return };
        if r#gen != self.search.r#gen || self.search.current != Some(idx) || idx >= self.search.matches.len() {
            self.search.pending_jump = None;
            return;
        }
        if self.match_rect_top(idx).is_some() { self.goto_current_match(); }
    }

    /// Ctrl+C: the selection text now, or `None` while some page still lacks text; the copy
    /// then stays pending and `take_copy_text` finishes it when the text arrives.
    pub fn copy_selection(&mut self) -> Option<String> {
        self.pending_copy = self.selection;
        self.take_copy_text()
    }

    /// Clipboard text once every page of the pending copy has text; the copy is dropped
    /// when the selection changed meanwhile.
    pub fn take_copy_text(&mut self) -> Option<String> {
        let sel = self.pending_copy?;
        if self.selection != Some(sel) {
            self.pending_copy = None;
            return None;
        }
        let text = selection_text(&self.text, &sel)?;
        self.pending_copy = None;
        Some(text)
    }

    pub fn selection_chars(&self) -> usize {
        let Some(sel) = self.selection else { return 0 };
        let ((first_page, first_char), (last_page, last_char)) = sel.ordered();
        let mut total = 0usize;
        for page in first_page..=last_page {
            let Some(Some(text)) = self.text.get(page) else { continue };
            let start = if page == first_page { first_char } else { 0 };
            let end = if page == last_page { last_char.saturating_add(1) } else { text.chars.len() }.min(text.chars.len());
            total += end.saturating_sub(start);
        }
        total
    }

    /// Selection state after a click without drag: drop it and any copy waiting for text.
    pub fn clear_selection(&mut self) -> bool {
        let had = self.selection.is_some() || self.pending_copy.is_some();
        self.selection = None;
        self.pending_copy = None;
        had
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::{DocGens, DocId, PageGeom, PdfEvent};
    use std::path::PathBuf;

    fn ch(c: char, x: f32, y: f32, w: f32, h: f32) -> PageChar {
        PageChar { ch: c, rect: PtRect { x, y, w, h } }
    }

    fn line(text: &str, x0: f32, y: f32) -> Vec<PageChar> {
        text.chars().enumerate().map(|(i, c)| ch(c, x0 + i as f32 * 10.0, y, 10.0, 12.0)).collect()
    }

    fn page_text(chars: Vec<PageChar>) -> Option<Arc<PageText>> { Some(Arc::new(PageText { chars })) }

    #[test]
    fn line_rects_merge_a_line_and_skip_empty_rects() {
        let mut out = Vec::new();
        line_rects(&line("Hello", 72.0, 74.0), 0, 5, &mut out);
        assert_eq!(out, vec![PtRect { x: 72.0, y: 74.0, w: 50.0, h: 12.0 }]);

        let mut chars = line("ab", 72.0, 74.0);
        chars.push(ch('\n', 0.0, 0.0, 0.0, 0.0));
        chars.extend(line("cd", 72.0, 100.0));
        out.clear();
        line_rects(&chars, 0, chars.len(), &mut out);
        assert_eq!(out.len(), 2);
        assert_eq!((out[0].y, out[1].y), (74.0, 100.0));

        out.clear();
        line_rects(&chars, 3, 99, &mut out);
        assert_eq!(out.len(), 1, "range past the end is clamped");
        out.clear();
        line_rects(&chars, 4, 2, &mut out);
        assert!(out.is_empty(), "inverted range is empty");
    }

    #[test]
    fn hit_test_char_prefers_containing_rect_then_nearest_line() {
        let mut chars = line("abc", 72.0, 74.0);
        chars.push(ch('\n', 0.0, 0.0, 0.0, 0.0));
        chars.extend(line("de", 72.0, 100.0));
        assert_eq!(hit_test_char(&chars, 86.0, 80.0), Some(1));
        assert_eq!(hit_test_char(&chars, 400.0, 80.0), Some(2), "right of the line -> its last char");
        assert_eq!(hit_test_char(&chars, 10.0, 80.0), Some(0));
        assert_eq!(hit_test_char(&chars, 80.0, 92.0), Some(0), "between lines -> nearest by Y");
        assert_eq!(hit_test_char(&chars, 80.0, 97.0), Some(4));
        assert_eq!(hit_test_char(&[], 1.0, 1.0), None);
        assert_eq!(hit_test_char(&[ch('\n', 0.0, 0.0, 0.0, 0.0)], 1.0, 1.0), None);
    }

    #[test]
    fn hit_test_link_handles_empty_and_outside() {
        let link = PageLink { rect: PtRect { x: 10.0, y: 10.0, w: 20.0, h: 10.0 }, target: crate::pdf::LinkTarget::Uri("https://a.b/".into()) };
        assert_eq!(hit_test_link(&[], 15.0, 15.0), None);
        assert_eq!(hit_test_link(std::slice::from_ref(&link), 15.0, 15.0), Some(0));
        assert_eq!(hit_test_link(std::slice::from_ref(&link), 50.0, 15.0), None);
    }

    #[test]
    fn selection_text_spans_pages_and_needs_all_of_them() {
        let text = vec![page_text(line("abc", 0.0, 0.0)), page_text(line("def", 0.0, 0.0))];
        let sel = PdfSelection { anchor: (0, 1), head: (1, 1) };
        assert_eq!(selection_text(&text, &sel).as_deref(), Some("bc\nde"));
        let reversed = PdfSelection { anchor: (1, 1), head: (0, 1) };
        assert_eq!(selection_text(&text, &reversed).as_deref(), Some("bc\nde"));
        let missing = vec![text[0].clone(), None, text[1].clone()];
        assert_eq!(selection_text(&missing, &PdfSelection { anchor: (0, 0), head: (2, 0) }), None);
        assert_eq!(selection_text(&text, &PdfSelection { anchor: (0, 0), head: (5, 0) }), None, "page out of range");
        assert_eq!(selection_text(&text, &PdfSelection { anchor: (0, 2), head: (0, 99) }).as_deref(), Some("c"));
    }

    #[test]
    fn search_pattern_matches_editor_semantics() {
        assert_eq!(search_pattern("a.b", false), "(?i)a\\.b");
        assert_eq!(search_pattern("a.b", true), "a\\.b");
    }

    #[test]
    fn link_urls_are_restricted_to_web_schemes() {
        assert!(link_url_allowed("https://example.com/"));
        assert!(link_url_allowed("http://example.com/a?b=c"));
        for bad in ["", "javascript:alert(1)", "file:///etc/passwd", "not a url", "mailto:x@example.com", "https://exa mple.com", "https://a.b/\u{7}"] {
            assert!(!link_url_allowed(bad), "{bad:?}");
        }
        assert!(!link_url_allowed(&format!("https://a.b/{}", "x".repeat(3000))));
    }

    fn ready_tab(pages: usize) -> PdfTabState {
        let geom = PageGeom { width_pt: 612.0, height_pt: 792.0 };
        let mut tab = PdfTabState::new(PathBuf::from("a.pdf"), Arc::new(DocGens::new()), PdfPhase::Loading);
        tab.doc = Some(DocId(1));
        tab.apply_event(&PdfEvent::Opened { id: DocId(1), pages: vec![geom; pages] });
        tab.set_viewport(1000, 800, 1.0);
        tab.body = (0.0, 0.0, 1000.0, 800.0);
        tab
    }

    #[test]
    fn search_events_respect_generation_and_jump_after_text_arrives() {
        let mut tab = ready_tab(3);
        assert!(tab.begin_search("", false).is_none());
        assert!(tab.search.done);
        let request = tab.begin_search("Second", false);
        assert!(matches!(request, Some(PdfRequest::Search { r#gen: 2, ref query, .. }) if query == "(?i)Second"));
        assert!(!tab.search.done);
        let stale = PdfEvent::SearchPage { id: DocId(1), r#gen: 1, page: 1, matches: vec![(0, 6)] };
        assert_eq!(tab.apply_event(&stale), super::super::PdfEventOutcome::Ignore);
        assert!(tab.search.matches.is_empty());
        tab.apply_event(&PdfEvent::SearchPage { id: DocId(1), r#gen: 2, page: 1, matches: vec![(0, 6)] });
        assert_eq!((tab.search.matches.len(), tab.search.current), (1, Some(0)));
        assert_eq!(tab.search.pending_jump, Some((2, 0)), "page text is missing, the jump waits");
        let requests = tab.take_text_requests();
        assert!(requests.iter().any(|request| matches!(request, PdfRequest::PageText { page: 1, .. })));
        assert!(tab.take_text_requests().is_empty(), "requested pages are not requested twice");
        let text = Arc::new(PageText { chars: line("Second page", 72.0, 74.0) });
        tab.apply_event(&PdfEvent::Text { id: DocId(1), page: 1, text: Arc::clone(&text), links: Vec::new() });
        assert_eq!(tab.search.pending_jump, None);
        assert!(tab.scroll.target >= tab.layout.rows[1].0 as f32);
        assert!(tab.line_boxes[1].as_ref().is_some_and(|boxes| boxes.len() == 1));
        // A second Text for the same page keeps the cached data.
        let other = Arc::new(PageText { chars: line("X", 0.0, 0.0) });
        assert_eq!(tab.apply_event(&PdfEvent::Text { id: DocId(1), page: 1, text: other, links: Vec::new() }), super::super::PdfEventOutcome::Ignore);
        assert_eq!(tab.text[1].as_ref().map(|item| item.chars.len()), Some(11));
        assert_eq!(tab.jump_match(true), Some(1), "one match cycles onto itself");
        assert_eq!(tab.jump_match(false), Some(1));
        tab.apply_event(&PdfEvent::SearchDone { id: DocId(1), r#gen: 1 });
        assert!(!tab.search.done);
        tab.apply_event(&PdfEvent::SearchDone { id: DocId(1), r#gen: 2 });
        assert!(tab.search.done);
    }

    #[test]
    fn pending_jump_from_an_old_search_is_dropped() {
        let mut tab = ready_tab(3);
        tab.begin_search("a", false);
        tab.apply_event(&PdfEvent::SearchPage { id: DocId(1), r#gen: 1, page: 2, matches: vec![(0, 1)] });
        assert_eq!(tab.search.pending_jump, Some((1, 0)));
        tab.begin_search("b", false);
        assert_eq!(tab.search.pending_jump, None);
        tab.search.pending_jump = Some((1, 0));
        tab.apply_event(&PdfEvent::TextFailed { id: DocId(1), page: 2, error: "x".into() });
        assert_eq!(tab.search.pending_jump, None, "stale generation is discarded");
        assert!(tab.text[2].as_ref().is_some_and(|text| text.chars.is_empty()), "failed text becomes empty text");
    }

    #[test]
    fn press_drag_click_cancel_and_autoscroll_are_owned_by_the_tab() {
        let mut tab = ready_tab(2);
        let text = Arc::new(PageText { chars: line("Hello world", 72.0, 74.0) });
        tab.apply_event(&PdfEvent::Text { id: DocId(1), page: 0, text, links: Vec::new() });
        let (row_y, _) = tab.layout.rows[0];
        let k = tab.layout.page_w as f32 / 612.0;
        let left = ((1000.0 - tab.layout.page_w as f32) * 0.5).round();
        let (x0, y0) = (left + 73.0 * k, row_y as f32 + 80.0 * k);
        let (x1, y1) = (left + 120.0 * k, row_y as f32 + 80.0 * k);
        assert!(!tab.drag_to(x1, y1, 4.0), "no press, no drag");
        tab.begin_press(x0, y0, None);
        assert!(tab.drag_to(x0 + 1.0, y0, 4.0));
        assert!(tab.selection.is_none(), "below the drag threshold");
        assert!(tab.drag_to(x1, y1, 4.0));
        assert_eq!(tab.selection, Some(PdfSelection { anchor: (0, 0), head: (0, 4) }));
        assert!(tab.end_press().is_some_and(|press| press.dragging));
        assert!(tab.selection.is_some(), "a drag keeps its selection");
        assert!(tab.end_press().is_none(), "release without a press is not ours");
        // A click clears the selection and reports the link under the press.
        tab.begin_press(x0, y0, Some((0, 1)));
        let click = tab.end_press().expect("press");
        assert_eq!((click.dragging, click.link), (false, Some((0, 1))));
        assert!(tab.selection.is_none());
        // Focus loss drops the press: the next moves are not swallowed.
        tab.begin_press(x0, y0, None);
        tab.cancel_press();
        assert!(!tab.drag_to(x1, y1, 4.0));
        // Autoscroll runs only during a drag and only near the body edges.
        tab.begin_press(x0, y0, None);
        let dt = 1.0 / 60.0;
        assert!(!tab.autoscroll_step(x1, 790.0, 1.0, dt), "not dragging yet");
        tab.drag_to(x1, y1, 4.0);
        assert!(!tab.autoscroll_step(x1, 400.0, 1.0, dt), "middle of the body");
        let before = tab.scroll.target;
        assert!(tab.autoscroll_step(x1, 790.0, 1.0, dt));
        assert!((tab.scroll.target - (before + 12.0)).abs() < 1e-3);
        // Speed is time-based: twice the elapsed time scrolls twice as far.
        let mid = tab.scroll.target;
        assert!(tab.autoscroll_step(x1, 790.0, 1.0, 2.0 * dt));
        assert!((tab.scroll.target - (mid + 24.0)).abs() < 1e-3);
        assert!(!tab.autoscroll_step(x1, 790.0, 1.0, 0.0), "no time, no scroll");
        for _ in 0..10 { tab.autoscroll_step(x1, 5.0, 1.0, 0.05); }
        assert_eq!(tab.scroll.target, 0.0);
        assert!(!tab.autoscroll_step(x1, 5.0, 1.0, dt), "clamped at the top");
    }

    #[test]
    fn point_hit_test_and_copy_wait_for_text() {
        let mut tab = ready_tab(2);
        let (row_y, row_h) = tab.layout.rows[0];
        let k = tab.layout.page_w as f32 / 612.0;
        let left = ((1000.0 - tab.layout.page_w as f32) * 0.5).round();
        let (page, x_pt, y_pt) = tab.point_to_page_pt(left + 72.0 * k, row_y as f32 + 80.0 * k).unwrap();
        assert_eq!(page, 0);
        assert!((x_pt - 72.0).abs() < 0.6 && (y_pt - 80.0).abs() < 0.6, "{x_pt} {y_pt}");
        assert_eq!(tab.point_to_page_pt(f32::NAN, 1.0), None);
        let clamped = tab.point_to_page_pt(-500.0, 1.0e6).unwrap();
        assert_eq!(clamped.0, 1);
        assert!(row_h > 0);

        tab.selection = Some(PdfSelection { anchor: (0, 0), head: (1, 2) });
        tab.pending_copy = tab.selection;
        assert_eq!(tab.take_copy_text(), None);
        assert!(tab.pending_copy.is_some(), "still waiting for text");
        let requests = tab.take_text_requests();
        assert_eq!(requests.iter().filter(|request| matches!(request, PdfRequest::PageText { .. })).count(), 2);
        for page in 0..2 {
            tab.apply_event(&PdfEvent::Text { id: DocId(1), page, text: Arc::new(PageText { chars: line("abc", 0.0, 0.0) }), links: Vec::new() });
        }
        assert_eq!(tab.selection_chars(), 6);
        assert_eq!(tab.take_copy_text().as_deref(), Some("abc\nabc"));
        assert!(tab.pending_copy.is_none());
        tab.pending_copy = tab.selection;
        tab.selection = None;
        assert_eq!(tab.take_copy_text(), None);
        assert!(tab.pending_copy.is_none(), "copy is dropped when the selection changed");
    }
}
