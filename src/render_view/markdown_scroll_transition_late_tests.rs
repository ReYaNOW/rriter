    // Review v5: exercise events between capture, deferred-current consumption,
    // and the first destination frame. All mapping/search/physics is production.
    #[test]
    fn reviewer_stage2_v5_read_inverse_preserves_search_motion_residual_control() {
        let source = document();
        let (_context, mut app) = fixture(&source, 720.0, 1.0);
        app.renderer.as_mut().unwrap().height = 420.0;
        let byte = source.find("paragraph030").unwrap();
        let start = review_v3_root_read_at(&mut app, byte, 3.25);
        app.set_markdown_mode(MarkdownMode::Edit);
        let anchor = app
            .markdown
            .scroll_transition
            .as_ref()
            .unwrap()
            .anchor
            .clone()
            .unwrap();
        let destination_current = edit_expected(&mut app, &anchor);
        review_v4_search(&mut app, &source, "paragraph034");
        let expected_residual = app.scroll_y.target - destination_current;
        let expected_velocity = app.scroll_y.velocity;
        app.set_markdown_mode(MarkdownMode::Read);
        review_v3_root_frame(&mut app);
        let actual_residual = app.scroll_y.target - app.scroll_y.current;
        println!(
            "V5_READ_INVERSE_RESIDUAL start={start} current={} expected_residual={expected_residual} actual_residual={actual_residual}",
            app.scroll_y.current
        );
        // Section 3.2 promises constant coordinate translation, NOT the same
        // future source byte after switching an already animated search.
        assert!((app.scroll_y.current - start).abs() <= 1.0);
        assert!((actual_residual - expected_residual).abs() <= 0.01);
        assert_eq!(app.scroll_y.velocity, expected_velocity);
    }

    #[test]
    fn reviewer_stage2_v5_edit_inverse_preserves_search_motion_residual_control() {
        let source = document();
        let (_context, mut app) = fixture(&source, 720.0, 1.0);
        app.renderer.as_mut().unwrap().height = 420.0;
        let byte = source.find("paragraph030").unwrap();
        review_v4_edit_at(&mut app, byte, 3.25);
        let start = app.scroll_y.current;
        app.set_markdown_mode(MarkdownMode::Read);
        let anchor = app
            .markdown
            .scroll_transition
            .as_ref()
            .unwrap()
            .anchor
            .clone()
            .unwrap();
        review_v4_search(&mut app, &source, "paragraph034");
        let destination_current = anchor.projected_scroll_y(
            app.markdown
                .read_layout
                .source_anchor_y(&anchor.source_range)
                .unwrap(),
        );
        let expected_residual = app.scroll_y.target - destination_current;
        let expected_velocity = app.scroll_y.velocity;
        app.set_markdown_mode(MarkdownMode::Edit);
        review_v3_root_frame(&mut app);
        let actual_residual = app.scroll_y.target - app.scroll_y.current;
        println!(
            "V5_EDIT_INVERSE_RESIDUAL start={start} current={} expected_residual={expected_residual} actual_residual={actual_residual}",
            app.scroll_y.current
        );
        assert!((app.scroll_y.current - start).abs() <= 1.0);
        assert!((actual_residual - expected_residual).abs() <= 0.01);
        assert_eq!(app.scroll_y.velocity, expected_velocity);
    }

    #[test]
    fn reviewer_stage2_v5_cold_read_search_tick_inverse_has_known_current_geometry() {
        let source = document();
        let (_context, mut app) = fixture(&source, 720.0, 1.0);
        app.renderer.as_mut().unwrap().height = 420.0;
        let byte = source.find("paragraph030").unwrap();
        review_v4_edit_at(&mut app, byte, 3.25);
        assert!(app.markdown.last_read_geometry.is_none());
        app.set_markdown_mode(MarkdownMode::Read);
        review_v4_search(&mut app, &source, "paragraph034");
        assert!(app.scroll_y.update(1.0 / 120.0));
        let read_current = app.scroll_y.current;
        let live_anchor = app
            .markdown
            .read_layout
            .viewport_source_anchor(read_current)
            .unwrap();
        let expected = edit_expected(&mut app, &live_anchor);
        app.set_markdown_mode(MarkdownMode::Edit);
        let captured = app
            .markdown
            .scroll_transition
            .as_ref()
            .and_then(|t| t.anchor.clone());
        review_v3_root_frame(&mut app);
        println!(
            "V5_COLD_TICK_INVERSE read_current={read_current} source={:?} expected_edit={expected} actual_edit={} captured={captured:?}",
            live_anchor.source_range, app.scroll_y.current
        );
        assert!(
            (app.scroll_y.current - expected).abs() <= 1.0,
            "a consumed rebase belongs to prepared Read geometry even before it was displayed"
        );
    }

    #[test]
    fn reviewer_stage2_v5_dpi_search_tick_inverse_uses_consumed_edit_geometry() {
        let source = document();
        let (_context, mut app) = fixture(&source, 720.0, 1.0);
        app.renderer.as_mut().unwrap().height = 420.0;
        let byte = source.find("paragraph030").unwrap();
        review_v4_edit_at(&mut app, byte, 3.25);
        review_v3_root_read_at(&mut app, byte, 3.25);
        app.renderer.as_mut().unwrap().update_scale_factor(1.25);
        app.set_markdown_mode(MarkdownMode::Edit);
        review_v4_search(&mut app, &source, "paragraph034");
        assert!(app.scroll_y.update(1.0 / 120.0));
        let edit_current = app.scroll_y.current;
        let live_anchor = app
            .renderer
            .as_mut()
            .unwrap()
            .markdown_edit_viewport_anchor(&app.editor, edit_current)
            .unwrap();
        app.set_markdown_mode(MarkdownMode::Read);
        let captured = app
            .markdown
            .scroll_transition
            .as_ref()
            .and_then(|t| t.anchor.clone());
        review_v3_root_frame(&mut app);
        let expected = live_anchor.projected_scroll_y(
            app.markdown
                .read_layout
                .source_anchor_y(&live_anchor.source_range)
                .unwrap(),
        );
        println!(
            "V5_DPI_TICK_INVERSE edit_current={edit_current} live={live_anchor:?} captured={captured:?} expected_read={expected} actual_read={}",
            app.scroll_y.current
        );
        assert!(
            (app.scroll_y.current - expected).abs() <= 1.0,
            "after deferred rebase, current is not in last-displayed Edit metrics any more"
        );
    }

    #[test]
    fn reviewer_stage2_v5_click_stop_before_search_rebase_stays_stopped() {
        let source = document();
        let (_context, mut app) = fixture(&source, 720.0, 1.0);
        app.renderer.as_mut().unwrap().height = 420.0;
        let byte = source.find("paragraph030").unwrap();
        review_v3_root_read_at(&mut app, byte, 3.25);
        app.set_markdown_mode(MarkdownMode::Edit);
        review_v4_search(&mut app, &source, "paragraph034");
        crate::app::mouse::stop_click_scroll_anims(&mut app, false);
        let stopped_current = app.scroll_y.current;
        assert!(app.scroll_y.is_settled());
        review_v3_root_frame(&mut app);
        let first_current = app.scroll_y.current;
        let first_target = app.scroll_y.target;
        app.scroll_y.update(1.0 / 120.0);
        println!(
            "V5_CLICK_STOP origin_stop={stopped_current} first_current={first_current} first_target={first_target} next_current={} next_velocity={}",
            app.scroll_y.current, app.scroll_y.velocity
        );
        assert_eq!(
            first_target, first_current,
            "a normal click-stop must remain stopped across deferred mode resolution"
        );
        assert_eq!(app.scroll_y.current, first_current);
        assert_eq!(app.scroll_y.velocity, 0.0);
    }

    #[test]
    fn reviewer_stage2_v5_two_searches_before_tick_keep_current_rebase_control() {
        let source = document();
        let (_context, mut app) = fixture(&source, 720.0, 1.0);
        app.renderer.as_mut().unwrap().height = 420.0;
        let byte = source.find("paragraph030").unwrap();
        review_v3_root_read_at(&mut app, byte, 3.25);
        app.set_markdown_mode(MarkdownMode::Edit);
        let anchor = app
            .markdown
            .scroll_transition
            .as_ref()
            .unwrap()
            .anchor
            .clone()
            .unwrap();
        let expected_at_capture = edit_expected(&mut app, &anchor);
        review_v4_search(&mut app, &source, "paragraph034");
        review_v4_search(&mut app, &source, "paragraph038");
        let mut control = app.scroll_y.clone();
        assert!(control.rebase_current_preserving_target(expected_at_capture));
        control.update(1.0 / 120.0);
        app.scroll_y.update(1.0 / 120.0);
        review_v3_root_frame(&mut app);
        println!(
            "V5_TWO_SEARCH_CONTROL expected_current={} actual_current={} expected_velocity={} actual_velocity={}",
            control.current, app.scroll_y.current, control.velocity, app.scroll_y.velocity
        );
        assert!((app.scroll_y.current - control.current).abs() <= 1.0);
        assert!((app.scroll_y.velocity - control.velocity).abs() <= 0.01);
        assert_eq!(app.scroll_y.target, control.target);
    }

    #[test]
    fn reviewer_stage2_v5_resize_after_search_preparation_revalidates_destination() {
        let source = document();
        let (_context, mut app) = fixture(&source, 720.0, 1.0);
        app.renderer.as_mut().unwrap().height = 420.0;
        let byte = source.find("paragraph030").unwrap();
        review_v4_edit_at(&mut app, byte, 3.25);
        app.set_markdown_mode(MarkdownMode::Read);
        let anchor = app
            .markdown
            .scroll_transition
            .as_ref()
            .unwrap()
            .anchor
            .clone()
            .unwrap();
        review_v4_search(&mut app, &source, "paragraph034");
        // A real window resize updates width before the requested redraw.
        app.renderer.as_mut().unwrap().width = 450.0;
        review_v3_root_frame(&mut app);
        let first_current = app.scroll_y.current;
        let first_target = app.scroll_y.target;
        let expected_current = anchor.projected_scroll_y(
            app.markdown
                .read_layout
                .source_anchor_y(&anchor.source_range)
                .unwrap(),
        );
        review_v4_search(&mut app, &source, "paragraph034");
        println!(
            "V5_RESIZE_AFTER_PREPARE expected_current={expected_current} actual_current={first_current} expected_target={} actual_target={first_target}",
            app.scroll_y.target
        );
        assert!(
            (first_current - expected_current).abs() <= 1.0,
            "pending numeric current must not outlive its destination geometry key"
        );
        assert!((first_target - app.scroll_y.target).abs() <= 1.0);
    }

    #[test]
    fn reviewer_stage2_v5_click_stop_after_resolved_search_remains_stopped_control() {
        let source = document();
        let (_context, mut app) = fixture(&source, 720.0, 1.0);
        app.renderer.as_mut().unwrap().height = 420.0;
        let byte = source.find("paragraph030").unwrap();
        review_v3_root_read_at(&mut app, byte, 3.25);
        app.set_markdown_mode(MarkdownMode::Edit);
        review_v4_search(&mut app, &source, "paragraph034");
        review_v3_root_frame(&mut app);
        crate::app::mouse::stop_click_scroll_anims(&mut app, false);
        let stopped = app.scroll_y.current;
        review_v3_root_frame(&mut app);
        assert!(!app.scroll_y.update(1.0 / 120.0));
        println!(
            "V5_CLICK_STOP_CONTROL stopped={stopped} current={} target={}",
            app.scroll_y.current, app.scroll_y.target
        );
        assert_eq!(app.scroll_y.current, stopped);
        assert_eq!(app.scroll_y.target, stopped);
        assert_eq!(app.scroll_y.velocity, 0.0);
    }

    #[test]
    fn reviewer_stage2_v1_full_root_reader_body_tracks_editor_origin_at_digit_boundaries() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for line_count in [1usize, 999, 1000, 10_000] {
                let mut source = String::new();
                for line in 0..line_count {
                    if line > 0 {
                        source.push('\n');
                    }
                    source.push_str("reader gutter geometry");
                }
                let (_context, mut app) = fixture(&source, 1280.0 * scale, scale);
                app.renderer.as_mut().unwrap().height = 420.0 * scale;
                app.set_markdown_mode(MarkdownMode::Read);
                review_v3_root_frame(&mut app);

                let renderer = app.renderer.as_ref().unwrap();
                let (body_x, _, body_w, _) = app
                    .ui_registry
                    .rect_for(crate::ui_system::UiId::MarkdownReadBody)
                    .expect("full root draw must register Reader body");
                let content_inset = (28.0 * scale).round();
                assert_eq!(
                    (body_x + content_inset).round(),
                    renderer.left_padding.round(),
                    "line_count={line_count} scale={scale}"
                );
                assert_eq!(
                    body_w.round(),
                    (renderer.width - body_x).max(0.0).round(),
                    "Reader keeps the original right edge"
                );
                assert!(app.markdown.read_layout.is_valid_for_geometry(
                    app.editor.version,
                    body_w.max(1.0),
                    renderer.scale_factor,
                    renderer.font_size,
                ));
            }
        }
    }
