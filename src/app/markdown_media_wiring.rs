//! Glue between the Markdown read state and the shared media cache (`MarkdownMedia`).
//!
//! `App` only routes here: the per-frame step (poll, request, layout, visible set, GPU
//! upload) and the revalidation triggers. The cache itself lives in `markdown_media`, the
//! media blocks and their keys come from `markdown_nav::document_media`, and the layout
//! (with the scroll anchoring on a `media_gen` change) lives in the render view.

use std::path::{Path, PathBuf};

use super::markdown_nav::document_media;
use super::{App, MarkdownMode, MarkdownTabState};
use crate::markdown_media::{MarkdownMedia, MediaKey, MediaRequest, VisibleMedia};
use crate::ui_waker::UiWaker;

/// Inputs of the last request pass of one tab. While they stay the same the pass is skipped;
/// `media_gen` is part of them so entries dropped by a revalidation or a reset are requested
/// again. `None` in the tab state means "first pass of this tab state" (a load or a reopen
/// builds a fresh state), which also forgets old failures and revalidates loaded files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MediaRequestMarker {
    version: u64,
    width_bits: u32,
    scale_bits: u32,
    media_gen: u64,
}

impl MarkdownTabState {
    /// Keys of every media element of the current read model (empty without a model).
    fn media_keys(&self, version: u64, doc_dir: &Path) -> Vec<MediaKey> {
        self.media_items_for_requests(version, doc_dir)
            .into_iter()
            .map(|request| request.key)
            .collect()
    }

    /// One request per media element, with `max_raster_w` and `scale` still unset (0).
    fn media_items_for_requests(&self, version: u64, doc_dir: &Path) -> Vec<MediaRequest> {
        let Some(document) = self.read_document(version) else {
            return Vec::new();
        };
        let source = self.read_source.as_str();
        let defs = document.link_definitions(source);
        document_media(document, source, doc_dir, &defs)
            .into_iter()
            .flat_map(|(_, items)| items)
            .map(|item| MediaRequest {
                key: item.key,
                source: item.source,
                max_raster_w: 0,
                scale: 0.0,
            })
            .collect()
    }

    /// Asks the cache for every media element of the document. Cheap when nothing changed
    /// since the last pass (see `MediaRequestMarker`). The first pass of a tab state also
    /// forgets failures and starts a revalidation of the already loaded files.
    pub(crate) fn request_media(
        &mut self,
        media: &mut MarkdownMedia,
        doc_dir: &Path,
        version: u64,
        width: f32,
        scale: f32,
        waker: &UiWaker,
    ) {
        let marker = MediaRequestMarker {
            version,
            width_bits: width.to_bits(),
            scale_bits: scale.to_bits(),
            media_gen: media.media_gen(),
        };
        if self.media_request == Some(marker) {
            return;
        }
        let requests = self.media_items_for_requests(version, doc_dir);
        let first = self.media_request.is_none();
        let keys: Vec<MediaKey> = requests.iter().map(|request| request.key.clone()).collect();
        if first {
            media.reset_failed(&keys);
        }
        let max_raster_w = width.max(1.0) as u32;
        for request in requests {
            media.request(MediaRequest { max_raster_w, scale, ..request }, waker);
        }
        if first {
            media.revalidate_files(&keys, waker);
        }
        self.media_request = Some(MediaRequestMarker {
            media_gen: media.media_gen(),
            ..marker
        });
    }

    /// Starts a stat pass over the loaded files of this document; with `reset_failed` the
    /// failed elements are forgotten too, so the next request pass asks for them again.
    pub(crate) fn revalidate_media(
        &self,
        media: &mut MarkdownMedia,
        doc_dir: &Path,
        version: u64,
        reset_failed: bool,
        waker: &UiWaker,
    ) {
        let keys = self.media_keys(version, doc_dir);
        if keys.is_empty() {
            return;
        }
        if reset_failed {
            media.reset_failed(&keys);
        }
        media.revalidate_files(&keys, waker);
    }

    /// Headless dump of the media of a tab in Read mode: `{key, state, x, y, w, h}` per media
    /// element of the document, the rectangle in document pixels. Empty outside Read mode
    /// and without a layout. `state` is `pending`, `ready` or `failed:<MediaError variant>`;
    /// a key the cache has not been asked for yet counts as `pending`. An element without a
    /// media rectangle (a failed Mermaid block is drawn as a code block) has no x/y/w/h
    /// (`null`).
    pub(crate) fn media_dump(
        &self,
        media: &MarkdownMedia,
        version: u64,
        doc_dir: Option<&Path>,
    ) -> Vec<serde_json::Value> {
        use crate::markdown_media::MediaEntryView;

        if self.mode != MarkdownMode::Read || self.read_layout.content_height() <= 0.0 {
            return Vec::new();
        }
        let Some(document) = self.read_document(version) else {
            return Vec::new();
        };
        let source = self.read_source.as_str();
        let defs = document.link_definitions(source);
        let items = document_media(document, source, doc_dir.unwrap_or(Path::new("")), &defs);
        let rects: Vec<_> = self.read_layout.media_blocks().collect();
        items
            .into_iter()
            .flat_map(|(_, items)| items)
            .map(|item| {
                let key = &item.key;
                let rect = rects.iter().find(|(rect_key, _)| *rect_key == key).map(|(_, rect)| *rect);
                let state = match media.entry(key) {
                    MediaEntryView::Unknown | MediaEntryView::Pending { .. } => "pending".to_string(),
                    MediaEntryView::Ready { .. } => "ready".to_string(),
                    MediaEntryView::Failed(error) => {
                        let name = format!("{error:?}");
                        let variant = name.split('(').next().unwrap_or_default();
                        format!("failed:{variant}")
                    }
                };
                serde_json::json!({
                    "key": key.dump_name(),
                    "state": state,
                    "x": rect.map(|rect| rect[0]),
                    "y": rect.map(|rect| rect[1]),
                    "w": rect.map(|rect| rect[2]),
                    "h": rect.map(|rect| rect[3]),
                })
            })
            .collect()
    }

    /// Media elements whose rectangle overlaps `[top, bottom]` of the document, with the
    /// size they are drawn at.
    fn visible_media(&self, top: f32, bottom: f32) -> Vec<VisibleMedia> {
        self.read_layout
            .media_blocks()
            .filter(|(_, [_, y, _, h])| *y < bottom && *y + *h > top)
            .map(|(key, [_, _, w, h])| VisibleMedia {
                key: key.clone(),
                display_w: w.round().max(0.0) as u32,
                display_h: h.round().max(0.0) as u32,
            })
            .collect()
    }
}

impl App {
    fn markdown_read_active(&self) -> bool {
        self.active_document_is_markdown() && self.markdown.mode == MarkdownMode::Read
    }

    fn markdown_doc_dir(&self) -> Option<PathBuf> {
        self.file_path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
    }

    /// `about_to_wait` step: collects finished media tasks. True when the layout must be
    /// rebuilt, so the frame is drawn (a task result only wakes `about_to_wait`).
    pub(crate) fn poll_markdown_media(&mut self) -> bool {
        let waker = self.ui_waker.clone();
        self.markdown_media.poll(&waker)
    }

    /// Frame step, before the draw: poll, request, layout with the current `media_gen`
    /// (anchored), the visible set, then the GPU upload. Asks for another frame while
    /// pixels wait for the upload or a visible element is still loading.
    pub(crate) fn markdown_media_prepare_frame(&mut self) {
        let waker = self.ui_waker.clone();
        self.markdown_media.poll(&waker);
        let mut visible = Vec::new();
        if self.markdown_read_active()
            && let Some((width, height, scale)) = self
                .renderer
                .as_ref()
                .map(|renderer| (renderer.width, renderer.height, renderer.scale_factor))
        {
            let content_width = self.markdown_read_content_width_for(width, scale);
            visible = self.markdown_media_layout_visible(content_width, scale, height, &waker);
        }
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        if self.markdown_media.prepare_gpu(renderer, &visible, &waker)
            && let Some(window) = self.window.as_ref()
        {
            window.request_redraw();
        }
    }

    fn markdown_media_layout_visible(
        &mut self,
        content_width: f32,
        scale: f32,
        view_height: f32,
        waker: &UiWaker,
    ) -> Vec<VisibleMedia> {
        let version = self.editor.version;
        let Some(doc_dir) = self.markdown_doc_dir() else {
            return Vec::new();
        };
        if self.markdown.read_document(version).is_none() {
            return Vec::new();
        }
        self.markdown.read_layout.set_media_dir(&doc_dir);
        self.markdown.request_media(
            &mut self.markdown_media,
            &doc_dir,
            version,
            content_width,
            scale,
            waker,
        );
        let Some(renderer) = self.renderer.as_mut() else {
            return Vec::new();
        };
        let content_width = content_width.max(1.0);
        // A geometry change ends the gestures that belong to the old layout; a change of
        // the media alone (`media_gen`) must not.
        if !self.markdown.read_layout.is_valid_for_geometry(
            version,
            content_width,
            renderer.scale_factor,
            renderer.font_size,
        ) {
            self.markdown.finish_read_selection_gesture(&mut self.scroll_y);
            self.scroll_y.end_drag();
        }
        if !renderer.prepare_markdown_read_layout_preserving_current_ownership(
            &mut self.markdown,
            &self.markdown_media,
            &mut self.scroll_y,
            version,
            content_width,
        ) {
            return Vec::new();
        }
        let top = self.scroll_y.current;
        self.markdown.visible_media(top, top + view_height)
    }

    /// Revalidation trigger (watcher tick, activation of a Markdown tab, reopen of a file):
    /// loaded local images are compared with the disk in the background. `reset_failed`
    /// (reopen) also forgets failures so they are asked for again.
    pub(crate) fn revalidate_markdown_media(&mut self, reset_failed: bool) {
        if !self.markdown_read_active() {
            return;
        }
        let Some(doc_dir) = self.markdown_doc_dir() else {
            return;
        };
        let waker = self.ui_waker.clone();
        self.markdown.revalidate_media(
            &mut self.markdown_media,
            &doc_dir,
            self.editor.version,
            reset_failed,
            &waker,
        );
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::markdown_media::{MediaEntryView, MediaError, MediaPixels};

    const SOURCE: &str = "# T\n\n![a](a.png)\n\ntext\n\n![b](https://example.com/b.png)\n";

    fn state() -> MarkdownTabState {
        let mut state = MarkdownTabState::default();
        assert!(state.refresh_read_model(1, SOURCE.to_string()));
        state
    }

    fn failing_media() -> MarkdownMedia {
        MarkdownMedia::with_loader(Arc::new(|_| Err(MediaError::NotFound)))
    }

    fn pixels() -> MediaPixels {
        MediaPixels {
            natural_w: 10.0,
            natural_h: 10.0,
            raster_w: 1,
            raster_h: 1,
            rgba: vec![0; 4],
            stamp: None,
        }
    }

    fn settle(media: &mut MarkdownMedia, keys: &[MediaKey], waker: &UiWaker) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while keys
            .iter()
            .any(|key| matches!(media.entry(key), MediaEntryView::Pending { .. }))
        {
            media.poll(waker);
            assert!(Instant::now() < deadline, "media tasks did not finish");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn media_keys_follow_the_document_and_are_empty_without_a_model() {
        let keys = state().media_keys(1, Path::new("/docs"));
        assert_eq!(keys.len(), 2);
        assert!(matches!(keys[0], MediaKey::File(_)));
        assert!(matches!(keys[1], MediaKey::Url(_)));
        assert!(MarkdownTabState::default().media_keys(1, Path::new("/docs")).is_empty());
        assert!(state().media_keys(2, Path::new("/docs")).is_empty());
    }

    #[test]
    fn first_pass_requests_every_element_and_sets_the_marker() {
        let mut state = state();
        let mut media = failing_media();
        let waker = UiWaker::counting();
        let keys = state.media_keys(1, Path::new("/docs"));
        assert!(state.media_request.is_none());
        state.request_media(&mut media, Path::new("/docs"), 1, 800.0, 1.0, &waker);
        assert!(state.media_request.is_some());
        for key in &keys {
            assert!(!matches!(media.entry(key), MediaEntryView::Unknown));
        }
        let marker = state.media_request;
        state.request_media(&mut media, Path::new("/docs"), 1, 800.0, 1.0, &waker);
        assert_eq!(state.media_request, marker);
    }

    #[test]
    fn a_dropped_entry_is_requested_again_after_media_gen_changes() {
        let mut state = state();
        let mut media = MarkdownMedia::with_loader(Arc::new(|_| Ok(pixels())));
        let waker = UiWaker::counting();
        let dir = Path::new("/docs");
        let keys = state.media_keys(1, dir);
        state.request_media(&mut media, dir, 1, 800.0, 1.0, &waker);
        settle(&mut media, &keys, &waker);
        let MediaKey::File(path) = keys[0].clone() else {
            panic!("the first element is a file");
        };
        media.invalidate_path(&path);
        assert!(matches!(media.entry(&keys[0]), MediaEntryView::Unknown));
        state.request_media(&mut media, dir, 1, 800.0, 1.0, &waker);
        assert!(!matches!(media.entry(&keys[0]), MediaEntryView::Unknown));
    }

    #[test]
    fn the_first_pass_of_a_fresh_state_forgets_failures() {
        let mut media = failing_media();
        let waker = UiWaker::counting();
        let dir = Path::new("/docs");
        let mut first = state();
        let keys = first.media_keys(1, dir);
        first.request_media(&mut media, dir, 1, 800.0, 1.0, &waker);
        settle(&mut media, &keys, &waker);
        assert!(matches!(media.entry(&keys[0]), MediaEntryView::Failed(_)));
        // A reopen builds a fresh tab state: its first pass resets the failure.
        let mut reopened = state();
        reopened.request_media(&mut media, dir, 1, 800.0, 1.0, &waker);
        assert!(matches!(media.entry(&keys[0]), MediaEntryView::Pending { .. }));
    }

    #[test]
    fn revalidation_without_media_does_nothing() {
        let mut media = failing_media();
        let waker = UiWaker::counting();
        let mut state = MarkdownTabState::default();
        assert!(state.refresh_read_model(1, "plain text\n".to_string()));
        state.revalidate_media(&mut media, Path::new("/docs"), 1, true, &waker);
        assert_eq!(media.media_gen(), 0);
    }

    #[test]
    fn visible_media_is_empty_without_a_layout() {
        let state = MarkdownTabState::default();
        assert!(state.visible_media(0.0, 100.0).is_empty());
    }
}
