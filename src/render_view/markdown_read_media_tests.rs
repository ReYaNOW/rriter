#[cfg(test)]
mod markdown_read_media_tests {
    use super::*;
    use crate::markdown_media::{MarkdownMedia, MediaError, MediaPixels, MediaRequest};
    use crate::ui_waker::UiWaker;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    type TestLoader = Arc<dyn Fn(&MediaRequest) -> Result<MediaPixels, MediaError> + Send + Sync>;

    fn slot(w: f32, h: f32) -> MediaSlot {
        MediaSlot { natural: Some((w, h)), state: MediaSlotState::Ready }
    }

    fn state_slot(natural: Option<(f32, f32)>, state: MediaSlotState) -> MediaSlot {
        MediaSlot { natural, state }
    }

    fn pixels(w: f32, h: f32) -> MediaPixels {
        MediaPixels {
            natural_w: w,
            natural_h: h,
            raster_w: w as u32,
            raster_h: h as u32,
            rgba: vec![0; 4],
            stamp: None,
        }
    }

    fn ok_loader(w: f32, h: f32) -> TestLoader {
        Arc::new(move |_| Ok(pixels(w, h)))
    }

    fn parse(source: &str) -> crate::languages::markdown::MarkdownDocument {
        crate::languages::markdown::MarkdownParseState::default()
            .parse(source)
            .expect("markdown parse")
    }

    /// A cache in which every media element of `source` has been requested and has finished
    /// (ready without a texture, or failed).
    fn loaded_media(source: &str, loader: TestLoader) -> MarkdownMedia {
        let mut media = MarkdownMedia::with_loader(loader);
        let document = parse(source);
        let requests: Vec<MediaRequest> = MediaInput::new(&document, source, Path::new(""), &media)
            .items
            .values()
            .flatten()
            .map(|item| MediaRequest {
                key: item.key.clone(),
                source: item.source.clone(),
                max_raster_w: 800,
                scale: 1.0,
            })
            .collect();
        assert!(!requests.is_empty(), "the source has no media element");
        let waker = UiWaker::counting();
        for request in &requests {
            media.request(request.clone(), &waker);
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while requests
            .iter()
            .any(|request| matches!(media.entry(&request.key), MediaEntryView::Pending { .. }))
        {
            media.poll(&waker);
            assert!(Instant::now() < deadline, "timed out loading test media");
            std::thread::sleep(Duration::from_millis(1));
        }
        media
    }

    fn layout_with(source: &str, width: f32, scale: f32, media: &MarkdownMedia) -> MarkdownReadLayoutCache {
        build_test_markdown_read_layout_with_media(source, width, scale, Some((media, Path::new(""))))
    }

    fn media_items(cache: &MarkdownReadLayoutCache) -> Vec<&PlacedMedia> {
        cache
            .blocks
            .iter()
            .filter_map(|block| match &block.kind {
                ReadBlockKind::Media { items } => Some(items),
                _ => None,
            })
            .flatten()
            .collect()
    }

    #[test]
    fn image_is_natural_size_times_scale() {
        let (rects, height) = layout_media_rects(&[slot(400.0, 300.0)], 800.0, 1.5, 23.0);
        assert_eq!(rects, vec![[0.0, 0.0, 600.0, 450.0]]);
        assert_eq!(height, 450.0);
    }

    #[test]
    fn image_wider_than_column_shrinks_proportionally() {
        let (rects, height) = layout_media_rects(&[slot(400.0, 300.0)], 500.0, 1.5, 23.0);
        assert_eq!(rects, vec![[0.0, 0.0, 500.0, 375.0]]);
        assert_eq!(height, 375.0);
        let (rects, _) = layout_media_rects(&[slot(20_000.0, 10_000.0)], 500.0, 1.0, 23.0);
        assert_eq!(rects, vec![[0.0, 0.0, 500.0, 250.0]]);
    }

    #[test]
    fn small_image_is_never_enlarged() {
        let (rects, _) = layout_media_rects(&[slot(100.0, 50.0)], 800.0, 1.0, 23.0);
        assert_eq!(rects, vec![[0.0, 0.0, 100.0, 50.0]]);
    }

    #[test]
    fn three_badges_wrap_into_two_rows() {
        let slots = [slot(90.0, 20.0), slot(90.0, 20.0), slot(90.0, 20.0)];
        let (rects, height) = layout_media_rects(&slots, 200.0, 1.0, 23.0);
        assert_eq!(
            rects,
            vec![[0.0, 0.0, 90.0, 20.0], [98.0, 0.0, 90.0, 20.0], [0.0, 28.0, 90.0, 20.0]]
        );
        assert_eq!(height, 48.0);
    }

    #[test]
    fn row_is_as_high_as_its_highest_element() {
        let slots = [slot(50.0, 10.0), slot(50.0, 40.0), slot(150.0, 12.0)];
        let (rects, height) = layout_media_rects(&slots, 200.0, 1.0, 23.0);
        assert_eq!(rects[2], [0.0, 48.0, 150.0, 12.0]);
        assert_eq!(height, 60.0);
    }

    #[test]
    fn unknown_and_pending_without_size_are_one_line_placeholders() {
        let slots = [
            state_slot(None, MediaSlotState::Unknown),
            state_slot(None, MediaSlotState::Pending),
        ];
        let (rects, height) = layout_media_rects(&slots, 600.0, 1.0, 23.0);
        assert_eq!(rects, vec![[0.0, 0.0, 600.0, 23.0], [0.0, 31.0, 600.0, 23.0]]);
        assert_eq!(height, 54.0);
    }

    #[test]
    fn pending_with_known_size_keeps_that_size() {
        let (rects, _) =
            layout_media_rects(&[state_slot(Some((80.0, 40.0)), MediaSlotState::Pending)], 600.0, 1.0, 23.0);
        assert_eq!(rects, vec![[0.0, 0.0, 80.0, 40.0]]);
    }

    #[test]
    fn failed_is_a_one_line_frame_across_the_column() {
        let failed = state_slot(Some((80.0, 40.0)), MediaSlotState::Failed);
        let (rects, height) = layout_media_rects(&[failed], 600.0, 1.0, 23.0);
        assert_eq!(rects, vec![[0.0, 0.0, 600.0, 23.0]]);
        assert_eq!(height, 23.0);
    }

    #[test]
    fn zero_nan_and_infinite_sizes_fall_back_to_the_placeholder() {
        for natural in [
            (0.0, 0.0),
            (0.0, 10.0),
            (10.0, 0.0),
            (-5.0, 10.0),
            (f32::NAN, 10.0),
            (10.0, f32::NAN),
            (f32::INFINITY, 10.0),
            (10.0, f32::NEG_INFINITY),
        ] {
            let (rects, height) = layout_media_rects(&[slot(natural.0, natural.1)], 600.0, 1.0, 23.0);
            assert_eq!(rects, vec![[0.0, 0.0, 600.0, 23.0]], "natural {natural:?}");
            assert_eq!(height, 23.0);
        }
    }

    #[test]
    fn absurd_scale_and_column_never_produce_non_finite_geometry() {
        for (scale, col_w) in [(f32::NAN, 500.0), (1e30, 500.0), (1.0, f32::NAN), (1.0, 0.0), (0.0, 500.0)] {
            let (rects, height) = layout_media_rects(&[slot(400.0, 300.0), slot(10.0, 10.0)], col_w, scale, 23.0);
            assert!(height.is_finite(), "scale {scale} col {col_w}");
            for rect in rects {
                assert!(rect.iter().all(|v| v.is_finite()), "{rect:?} for scale {scale} col {col_w}");
                assert!(rect[2] >= 1.0 && rect[3] >= 1.0);
            }
        }
    }

    #[test]
    fn coordinates_are_whole_pixels_at_fractional_scale() {
        let slots = [slot(97.0, 23.0), slot(61.0, 19.0), slot(333.0, 71.0), slot(45.0, 17.0)];
        let (rects, height) = layout_media_rects(&slots, 417.0, 1.333, media_line_height(1.333));
        assert!(rects.len() == 4 && height.fract() == 0.0);
        for rect in rects {
            assert!(rect.iter().all(|v| v.fract() == 0.0), "{rect:?}");
        }
    }

    #[test]
    fn media_paragraph_becomes_a_media_block_with_paragraph_spacing() {
        let source = "![alt](https://example.com/a.png)\n\ntail\n";
        let media = loaded_media(source, ok_loader(400.0, 300.0));
        let cache = layout_with(source, 1000.0, 1.0, &media);
        assert_eq!(cache.blocks.len(), 2);
        let first = &cache.blocks[0];
        assert!(matches!(first.kind, ReadBlockKind::Media { .. }));
        let items = media_items(&cache);
        assert_eq!((items[0].x, items[0].y, items[0].w, items[0].h), (28.0, 0.0, 400.0, 300.0));
        assert_eq!(items[0].alt, "alt");
        assert_eq!(first.bottom - first.top, 300.0);
        assert_eq!(cache.blocks[1].top, first.bottom + BLOCK_GAP);
    }

    #[test]
    fn media_blocks_reports_keys_and_document_rectangles() {
        let source = "text\n\n![a](https://example.com/a.png) ![b](https://example.com/b.png)\n";
        let media = loaded_media(source, ok_loader(100.0, 50.0));
        let cache = layout_with(source, 1000.0, 1.0, &media);
        let top = cache.blocks[1].top;
        let blocks: Vec<_> = cache.media_blocks().collect();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].1, [28.0, top, 100.0, 50.0]);
        assert_eq!(blocks[1].1, [136.0, top, 100.0, 50.0]);
        assert!(matches!(blocks[0].0, MediaKey::Url(url) if url.ends_with("a.png")));
        assert!(matches!(blocks[1].0, MediaKey::Url(url) if url.ends_with("b.png")));
    }

    #[test]
    fn failed_image_shows_the_alt_and_reason_in_a_one_line_frame() {
        let source = "![logo](https://example.com/a.png)\n";
        let media = loaded_media(source, Arc::new(|_| Err(MediaError::NotFound)));
        let cache = layout_with(source, 1000.0, 1.0, &media);
        let items = media_items(&cache);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].alt, "logo — не найдено");
        assert_eq!((items[0].w, items[0].h), (944.0, media_line_height(1.0)));
    }

    #[test]
    fn unloaded_image_is_a_placeholder_until_the_size_is_known() {
        let source = "![logo](https://example.com/a.png)\n";
        let idle = MarkdownMedia::with_loader(ok_loader(400.0, 300.0));
        let before = layout_with(source, 1000.0, 1.0, &idle);
        assert_eq!(media_items(&before)[0].h, media_line_height(1.0));
        let loaded = loaded_media(source, ok_loader(400.0, 300.0));
        let after = layout_with(source, 1000.0, 1.0, &loaded);
        assert_eq!(media_items(&after)[0].h, 300.0);
        assert_ne!(before.media_gen(), after.media_gen());
    }

    #[test]
    fn without_a_media_cache_images_stay_text() {
        let source = "![logo](https://example.com/a.png)\n";
        let cache = build_test_markdown_read_layout(source, 1000.0);
        assert!(cache.media_blocks().next().is_none());
        assert!(matches!(cache.blocks[0].kind, ReadBlockKind::Text(_)));
        assert_eq!(cache.media_gen(), None);
    }

    #[test]
    fn layout_is_rebuilt_only_when_the_media_generation_changes() {
        let source = "![logo](https://example.com/a.png)\n";
        let media = loaded_media(source, ok_loader(400.0, 300.0));
        let mut cache = layout_with(source, 1000.0, 1.0, &media);
        let key = LayoutKey::new(1, 1000.0, 1.0, 16.0);
        cache.media_gen = Some(1);
        assert!(cache.is_current(key, Some(1)));
        assert!(!cache.is_current(key, Some(2)));
        assert!(!cache.is_current(key, None));
        cache.media_gen = None;
        assert!(cache.is_current(key, None));
        assert!(!cache.is_current(key, Some(0)));
        cache.media_gen = Some(1);
        assert!(!cache.is_current(LayoutKey::new(2, 1000.0, 1.0, 16.0), Some(1)));
    }

    #[test]
    fn media_dir_change_invalidates_the_layout() {
        let mut cache = build_test_markdown_read_layout("text\n", 400.0);
        let key = LayoutKey::new(1, 400.0, 1.0, 16.0);
        cache.set_media_dir(Path::new(""));
        assert!(cache.is_current(key, None));
        cache.set_media_dir(Path::new("/docs"));
        assert!(!cache.is_current(key, None));
    }

    #[test]
    fn media_block_is_not_selectable_and_is_skipped_by_copy() {
        let source = "a\n\n![logo](https://example.com/a.png)\n\nb\n";
        let media = loaded_media(source, ok_loader(40.0, 20.0));
        let cache = layout_with(source, 400.0, 1.0, &media);
        assert_eq!(cache.blocks.len(), 3);
        assert_eq!(cache.copy_source_selection(source, &(0..source.len())), "a\nb");
    }

    #[test]
    fn failed_mermaid_stays_a_code_block_with_an_error_row() {
        let source = "```mermaid\ngraph TD\n```\n";
        let plain = build_test_markdown_read_layout(source, 400.0);
        let media = loaded_media(source, Arc::new(|_| Err(MediaError::Mermaid("boom\nsecond line".into()))));
        let cache = layout_with(source, 400.0, 1.0, &media);
        let (ReadBlockKind::Code(plain_code), ReadBlockKind::Code(code)) =
            (&plain.blocks[0].kind, &cache.blocks[0].kind)
        else {
            panic!("a failed mermaid block must stay a code block");
        };
        assert_eq!(plain_code.error, None);
        assert_eq!(code.error.as_deref(), Some("boom"));
        assert_eq!(code.lines[0].top - plain_code.lines[0].top, code.line_height);
        assert_eq!(cache.blocks[0].bottom - plain.blocks[0].bottom, code.line_height);
    }

    #[test]
    fn ready_mermaid_becomes_a_media_block() {
        let source = "```mermaid\ngraph TD\n```\n";
        let media = loaded_media(source, ok_loader(200.0, 100.0));
        let cache = layout_with(source, 600.0, 1.0, &media);
        let items = media_items(&cache);
        assert_eq!(items.len(), 1);
        assert_eq!((items[0].w, items[0].h, items[0].alt.as_str()), (200.0, 100.0, "mermaid"));
    }

    #[test]
    fn every_failure_of_a_mermaid_block_keeps_the_code_with_an_error_line() {
        let source = "```mermaid\ngraph TD\n```\n";
        let cases = [
            (MediaError::Crashed, "рендер упал"),
            (MediaError::Timeout, "таймаут"),
            (MediaError::Unsupported, "не поддерживается"),
        ];
        for (error, label) in cases {
            let media = loaded_media(source, Arc::new(move |_| Err(error.clone())));
            let cache = layout_with(source, 600.0, 1.0, &media);
            let ReadBlockKind::Code(code) = &cache.blocks[0].kind else {
                panic!("a failed mermaid block must stay a code block ({label})");
            };
            assert_eq!(code.error.as_deref(), Some(label));
        }
    }
}
