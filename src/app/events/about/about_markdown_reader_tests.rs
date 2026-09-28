    use super::*;
    #[test]
    fn markdown_reader_edge_zone_is_dpi_scaled_small_viewport_safe_and_signed() {
        for (scale, viewport_h, expected) in [
            (1.0, 400.0, 24.0),
            (1.5, 400.0, 36.0),
            (2.0, 400.0, 48.0),
            (2.0, 80.0, 20.0),
        ] {
            let edge = markdown_read_selection_autoscroll_edge(viewport_h, scale);
            assert!((edge - expected).abs() < f32::EPSILON);
            assert!(
                markdown_read_selection_autoscroll_delta(edge * 0.5, 0.0, viewport_h, edge) < 0.0
            );
            assert_eq!(
                markdown_read_selection_autoscroll_delta(edge, 0.0, viewport_h, edge),
                0.0
            );
            assert!(
                markdown_read_selection_autoscroll_delta(
                    viewport_h - edge * 0.5,
                    0.0,
                    viewport_h,
                    edge,
                ) > 0.0
            );
            assert_eq!(
                markdown_read_selection_autoscroll_delta(
                    viewport_h - edge,
                    0.0,
                    viewport_h,
                    edge,
                ),
                0.0
            );

            let epsilon = 0.01;
            let top_inside = markdown_read_selection_autoscroll_delta(
                epsilon,
                0.0,
                viewport_h,
                edge,
            );
            let top_outside = markdown_read_selection_autoscroll_delta(
                -epsilon,
                0.0,
                viewport_h,
                edge,
            );
            let bottom_inside = markdown_read_selection_autoscroll_delta(
                viewport_h - epsilon,
                0.0,
                viewport_h,
                edge,
            );
            let bottom_outside = markdown_read_selection_autoscroll_delta(
                viewport_h + epsilon,
                0.0,
                viewport_h,
                edge,
            );
            assert!(top_inside < 0.0 && top_outside < 0.0);
            assert!(bottom_inside > 0.0 && bottom_outside > 0.0);
            assert!((top_inside - top_outside).abs() <= epsilon * 2.1);
            assert!((bottom_inside - bottom_outside).abs() <= epsilon * 2.1);
        }
        assert_eq!(markdown_read_selection_autoscroll_edge(0.0, 1.0), 0.0);
        assert_eq!(markdown_read_selection_autoscroll_edge(100.0, 0.0), 0.0);
        assert_eq!(markdown_read_selection_autoscroll_edge(f32::NAN, 1.0), 0.0);
    }

    #[test]
    fn markdown_reader_scrollbar_thumb_guards_short_zero_and_tiny_viewports() {
        use crate::render_view::markdown_read::markdown_read_scrollbar_thumb;

        assert!(markdown_read_scrollbar_thumb(10.0, 0.0, 500.0, 0.0, 1.0).is_none());
        assert!(markdown_read_scrollbar_thumb(10.0, 180.0, 180.0, 0.0, 1.0).is_none());
        assert!(markdown_read_scrollbar_thumb(10.0, 180.0, 120.0, 0.0, 1.0).is_none());
        assert!(markdown_read_scrollbar_thumb(10.0, 180.0, 500.0, 0.0, 0.0).is_none());

        let tiny = markdown_read_scrollbar_thumb(7.0, 12.0, 50_000.0, 49_988.0, 1.75)
            .expect("tiny viewport still has finite scrollbar geometry");
        assert!(tiny.start.is_finite());
        assert!(tiny.len.is_finite());
        assert!(tiny.len > 0.0 && tiny.len <= 12.0);
        assert!(tiny.start >= 7.0 && tiny.start + tiny.len <= 19.0 + 0.01);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn markdown_reader_scrollbar_real_press_drag_and_release_use_shared_geometry() {
        let mut source = String::new();
        for i in 0..120 {
            source.push_str(&format!("paragraph {i:03} alpha beta gamma delta epsilon\n\n"));
        }
        let (_context, mut app) =
            crate::render_view::reviewer_stage2_integration::fixture(&source, 900.0, 1.25);
        app.set_markdown_mode(crate::app::MarkdownMode::Read);
        crate::render_view::reviewer_stage2_integration::read_frame(&mut app);
        let max_scroll = app.markdown.read_scroll_bounds().expect("reader bounds");
        assert!(max_scroll > 0.0);
        app.scroll_y.jump_to((max_scroll * 0.31).round());
        app.scroll_y.anim_speed = 9.0;
        crate::render_view::reviewer_stage2_integration::read_frame(&mut app);

        let scrollbar = app
            .ui_registry
            .rect_for(crate::ui_system::UiId::MarkdownReadScrollbar)
            .expect("reader scrollbar registry");
        let body = app
            .ui_registry
            .rect_for(crate::ui_system::UiId::MarkdownReadBody)
            .expect("reader body registry");
        let thumb = crate::render_view::markdown_read::markdown_read_scrollbar_thumb(
            body.1,
            body.3,
            app.markdown.read_layout.content_height(),
            app.scroll_y.current.round(),
            1.25,
        )
        .expect("reader thumb");
        let original_scroll = app.scroll_y.current;
        let mut expected_revision = app.markdown.scroll_navigation_revision();
        for fraction in [0.1, 0.5, 0.9] {
            let press_y = thumb.start + thumb.len * fraction;
            app.renderer.as_mut().unwrap().last_mouse_x = scrollbar.0 + scrollbar.2 * 0.5;
            app.renderer.as_mut().unwrap().last_mouse_y = press_y;

            app.handle_main_mouse_input(&host_loop::HostLoop::headless(&host_loop::HeadlessLoopState::default()),
                winit::event::ElementState::Pressed,
                winit::event::MouseButton::Left,
            );
            expected_revision = expected_revision.wrapping_add(1);
            assert!(app.scroll_y.is_dragging);
            assert!((app.scroll_y.drag_offset - thumb.len * fraction).abs() < 0.01);
            assert!((app.scroll_y.current - original_scroll).abs() < 0.02);
            assert!((app.scroll_y.target - app.scroll_y.current).abs() < 0.02);
            assert_eq!(app.scroll_y.velocity, 0.0);
            assert_eq!(app.scroll_y.anim_speed, 15.0);
            assert_eq!(app.markdown.scroll_navigation_revision(), expected_revision);
            assert!(app.markdown.scroll_transition.is_none());
            assert!(app.markdown.scroll_carry.is_none());

            app.handle_main_mouse_input(&host_loop::HostLoop::headless(&host_loop::HeadlessLoopState::default()),
                winit::event::ElementState::Released,
                winit::event::MouseButton::Left,
            );
            assert!(!app.scroll_y.is_dragging);
        }

        let press_y = thumb.start + thumb.len * 0.3;
        app.renderer.as_mut().unwrap().last_mouse_x = scrollbar.0 + scrollbar.2 * 0.5;
        app.renderer.as_mut().unwrap().last_mouse_y = press_y;
        app.handle_main_mouse_input(&host_loop::HostLoop::headless(&host_loop::HeadlessLoopState::default()),
            winit::event::ElementState::Pressed,
            winit::event::MouseButton::Left,
        );
        assert!(app.scroll_y.is_dragging);

        let drag_y = body.1 + body.3 - 2.0;
        assert!(app.drag_markdown_read_scrollbar_to(drag_y));
        assert!(app.scroll_y.is_dragging);
        assert!((app.scroll_y.current - original_scroll).abs() < 0.02);
        assert!(app.scroll_y.target > app.scroll_y.current);
        assert_eq!(app.scroll_y.velocity, 0.0);
        assert_eq!(app.scroll_y.anim_speed, 15.0);

        // Release coordinates are intentionally outside the scrollbar track. The
        // captured Reader drag must end before ordinary UI release dispatch.
        app.renderer.as_mut().unwrap().last_mouse_x = body.0 + 30.0;
        app.renderer.as_mut().unwrap().last_mouse_y = body.1 + body.3 * 0.5;
        app.handle_main_mouse_input(&host_loop::HostLoop::headless(&host_loop::HeadlessLoopState::default()),
            winit::event::ElementState::Released,
            winit::event::MouseButton::Left,
        );
        assert!(!app.scroll_y.is_dragging);
        assert_eq!(app.scroll_y.drag_offset, 0.0);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn markdown_reader_wheel_is_not_swallowed_by_hidden_editor_hover_state() {
        let source = (0..120)
            .map(|i| format!("paragraph {i:03} alpha beta gamma delta\n\n"))
            .collect::<String>();
        let (_context, mut app) =
            crate::render_view::reviewer_stage2_integration::fixture(&source, 900.0, 1.25);
        app.set_markdown_mode(crate::app::MarkdownMode::Read);
        crate::render_view::reviewer_stage2_integration::read_frame(&mut app);
        let body = app
            .ui_registry
            .rect_for(crate::ui_system::UiId::MarkdownReadBody)
            .expect("reader body");
        {
            let renderer = app.renderer.as_mut().unwrap();
            renderer.last_mouse_x = body.0 + 40.0;
            renderer.last_mouse_y = body.1 + body.3 * 0.5;
        }
        app.hover.byte_offset = Some(0);
        let before = app.scroll_y.target;
        app.handle_main_mouse_wheel(winit::event::MouseScrollDelta::PixelDelta(
            winit::dpi::PhysicalPosition::new(0.0, -36.0),
        ));
        crate::app::mouse::clear_hover_popup(&mut app.hover);
        assert!(app.scroll_y.target > before, "first reader wheel notch must scroll");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn editor_wheel_continues_after_zero_progress_hover_is_cleared() {
        let source = (0..120)
            .map(|i| format!("line {i:03} alpha beta gamma delta\n"))
            .collect::<String>();
        let (_context, mut app) =
            crate::render_view::reviewer_stage2_integration::fixture(&source, 900.0, 1.0);
        app.set_markdown_mode(crate::app::MarkdownMode::Edit);
        {
            let renderer = app.renderer.as_mut().unwrap();
            renderer.last_mouse_x = 140.0;
            renderer.last_mouse_y = 100.0;
        }
        {
            let state = &mut app.hover;
            state.rect = Some((100.0, 60.0, 240.0, 120.0));
            state.popup = Some(crate::app::mouse::HoverPopup {
                text: "value: int".to_string(),
                spans: Vec::new(),
                line_kinds: Vec::new(),
                inline_code_ranges: Vec::new(),
                byte_offset: 0,
                anchor_x: 140.0,
                anchor_y: 100.0,
                offset_x: None,
                offset_y: None,
                anim_progress: 0.0,
                scroll: crate::scroll::ScrollState::new(15.0),
                layout_cache: None,
            });
        }

        let before = app.scroll_y.target;
        app.handle_main_mouse_wheel(winit::event::MouseScrollDelta::PixelDelta(
            winit::dpi::PhysicalPosition::new(0.0, -36.0),
        ));

        crate::app::mouse::clear_hover_popup(&mut app.hover);
        assert!(
            app.scroll_y.target > before,
            "clearing an invisible hover must not swallow the same editor wheel event"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn stale_hover_interaction_rect_without_target_does_not_swallow_editor_wheel() {
        let source = (0..120)
            .map(|i| format!("line {i:03} alpha beta gamma delta\n"))
            .collect::<String>();
        let (_context, mut app) =
            crate::render_view::reviewer_stage2_integration::fixture(&source, 900.0, 1.0);
        app.set_markdown_mode(crate::app::MarkdownMode::Edit);
        {
            let renderer = app.renderer.as_mut().unwrap();
            renderer.last_mouse_x = 140.0;
            renderer.last_mouse_y = 100.0;
        }
        app.hover.interaction_rect = Some((100.0, 60.0, 240.0, 120.0));

        let before = app.scroll_y.target;
        app.handle_main_mouse_wheel(winit::event::MouseScrollDelta::PixelDelta(
            winit::dpi::PhysicalPosition::new(0.0, -36.0),
        ));

        crate::app::mouse::clear_hover_popup(&mut app.hover);
        assert!(
            app.scroll_y.target > before,
            "a stale interaction rect without popup/diagnostic target must pass the wheel downstream"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn combined_hover_gap_inside_outer_frame_does_not_scroll_diagnostic() {
        let source = (0..120)
            .map(|i| format!("paragraph {i:03} alpha beta gamma delta\n\n"))
            .collect::<String>();
        let (_context, mut app) =
            crate::render_view::reviewer_stage2_integration::fixture(&source, 900.0, 1.0);
        app.set_markdown_mode(crate::app::MarkdownMode::Read);
        crate::render_view::reviewer_stage2_integration::read_frame(&mut app);
        {
            let renderer = app.renderer.as_mut().unwrap();
            renderer.last_mouse_x = 140.0;
            renderer.last_mouse_y = 145.0;
        }
        {
            let state = &mut app.hover;
            state.rect = Some((100.0, 160.0, 240.0, 40.0));
            state.diag_rect = Some((100.0, 100.0, 240.0, 30.0, 120.0, 180.0, 120.0));
            state.interaction_rect = Some((100.0, 100.0, 240.0, 100.0));
            state.diag_anim_progress = 1.0;
            state.popup = Some(crate::app::mouse::HoverPopup {
                text: "value: int".to_string(),
                spans: Vec::new(),
                line_kinds: Vec::new(),
                inline_code_ranges: Vec::new(),
                byte_offset: 0,
                anchor_x: 140.0,
                anchor_y: 120.0,
                offset_x: None,
                offset_y: None,
                anim_progress: 1.0,
                scroll: crate::scroll::ScrollState::new(15.0),
                layout_cache: None,
            });
        }

        let before_reader = app.scroll_y.target;
        let before_diag = app.hover.diag_scroll.target;
        app.handle_main_mouse_wheel(winit::event::MouseScrollDelta::PixelDelta(
            winit::dpi::PhysicalPosition::new(0.0, -36.0),
        ));

        let after_diag = app.hover.diag_scroll.target;
        crate::app::mouse::clear_hover_popup(&mut app.hover);
        assert_eq!(after_diag, before_diag, "outer-frame gap must not scroll diagnostics");
        assert!(
            app.scroll_y.target > before_reader,
            "outer-frame gap must pass the wheel to Reader"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn markdown_reader_wheel_continues_outside_partially_visible_combined_hover() {
        let source = (0..120)
            .map(|i| format!("paragraph {i:03} alpha beta gamma delta\n\n"))
            .collect::<String>();
        let (_context, mut app) =
            crate::render_view::reviewer_stage2_integration::fixture(&source, 900.0, 1.0);
        app.set_markdown_mode(crate::app::MarkdownMode::Read);
        crate::render_view::reviewer_stage2_integration::read_frame(&mut app);
        {
            let renderer = app.renderer.as_mut().unwrap();
            renderer.last_mouse_x = 180.0;
            renderer.last_mouse_y = 130.0;
        }
        {
            let state = &mut app.hover;
            state.rect = Some((100.0, 80.0, 240.0, 70.0));
            state.diag_rect = Some((100.0, 40.0, 240.0, 40.0, 120.0, 160.0, 60.0));
            state.interaction_rect = Some((100.0, 40.0, 240.0, 20.0));
            state.diag_anim_progress = 0.1;
            state.popup = Some(crate::app::mouse::HoverPopup {
                text: "value: int".to_string(),
                spans: Vec::new(),
                line_kinds: Vec::new(),
                inline_code_ranges: Vec::new(),
                byte_offset: 0,
                anchor_x: 140.0,
                anchor_y: 60.0,
                offset_x: None,
                offset_y: None,
                anim_progress: 1.0,
                scroll: crate::scroll::ScrollState::new(15.0),
                layout_cache: None,
            });
        }

        let before = app.scroll_y.target;
        app.handle_main_mouse_wheel(winit::event::MouseScrollDelta::PixelDelta(
            winit::dpi::PhysicalPosition::new(0.0, -36.0),
        ));

        crate::app::mouse::clear_hover_popup(&mut app.hover);
        assert!(
            app.scroll_y.target > before,
            "the hidden exterior of a combined hover must pass the wheel to Reader"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn wheel_inside_rendered_hover_surface_is_consumed_locally() {
        let source = (0..120)
            .map(|i| format!("line {i:03} alpha beta gamma delta\n"))
            .collect::<String>();
        let (_context, mut app) =
            crate::render_view::reviewer_stage2_integration::fixture(&source, 900.0, 1.0);
        {
            let renderer = app.renderer.as_mut().unwrap();
            renderer.last_mouse_x = 140.0;
            renderer.last_mouse_y = 100.0;
        }
        {
            let state = &mut app.hover;
            state.rect = Some((100.0, 80.0, 240.0, 120.0));
            state.interaction_rect = Some((100.0, 80.0, 120.0, 40.0));
            state.max_scroll = 200.0;
            state.popup = Some(crate::app::mouse::HoverPopup {
                text: "value: int".to_string(),
                spans: Vec::new(),
                line_kinds: Vec::new(),
                inline_code_ranges: Vec::new(),
                byte_offset: 0,
                anchor_x: 140.0,
                anchor_y: 100.0,
                offset_x: None,
                offset_y: None,
                anim_progress: 0.2,
                scroll: crate::scroll::ScrollState::new(15.0),
                layout_cache: None,
            });
        }

        let editor_before = app.scroll_y.target;
        app.handle_main_mouse_wheel(winit::event::MouseScrollDelta::PixelDelta(
            winit::dpi::PhysicalPosition::new(0.0, -36.0),
        ));
        let popup_target = app
            .hover
            .popup
            .as_ref()
            .map(|popup| popup.scroll.target)
            .unwrap_or_default();

        crate::app::mouse::clear_hover_popup(&mut app.hover);
        assert!(popup_target > 0.0, "visible hover must receive wheel delta");
        assert_eq!(app.scroll_y.target, editor_before);
    }

    // Bug 1 end-to-end: the wheel is owned only through the interaction rect the
    // production renderer writes for the popup frame it actually drew, and that
    // rect disappears again when the popup stops painting a real frame.
    #[cfg(target_os = "linux")]
    #[test]
    fn rendered_type_popup_owns_interaction_rect_for_wheel_and_releases_it_when_invisible() {
        let source = (0..120)
            .map(|i| format!("value_{i:03} = handler(alpha, beta, gamma)\n"))
            .collect::<String>();
        let (_context, mut app) =
            crate::render_view::reviewer_stage2_integration::fixture(&source, 900.0, 1.0);
        app.set_markdown_mode(crate::app::MarkdownMode::Edit);
        let byte = source.find("handler").expect("hover symbol") + 3;
        {
            let state = &mut app.hover;
            state.reset_diagnostic_popup();
            state.byte_offset = None;
            // Cleared up front so the assertions below can only come from the frame.
            state.interaction_rect = None;
            state.popup = Some(crate::app::mouse::HoverPopup {
                text: "def handler(alpha, beta, gamma) -> None".to_string(),
                spans: Vec::new(),
                line_kinds: Vec::new(),
                inline_code_ranges: Vec::new(),
                byte_offset: byte,
                anchor_x: 300.0,
                anchor_y: 60.0,
                offset_x: None,
                offset_y: None,
                anim_progress: 1.0,
                scroll: crate::scroll::ScrollState::new(15.0),
                layout_cache: None,
            });
        }
        {
            let renderer = app.renderer.as_mut().unwrap();
            renderer.last_mouse_x = 300.0;
            renderer.last_mouse_y = 60.0;
        }

        crate::render_view::reviewer_stage2_integration::review_v3_root_frame(&mut app);

        let (content_rect, interaction_rect) = (app.hover.rect, app.hover.interaction_rect);
        let content_rect = content_rect.expect("the drawn popup reports its content rect");
        let interaction_rect =
            interaction_rect.expect("the drawn popup writes its interaction rect");
        assert!(
            (interaction_rect.2 - content_rect.2).abs() <= 2.0
                && (interaction_rect.3 - content_rect.3).abs() <= 2.0,
            "interaction rect {interaction_rect:?} must frame the drawn popup {content_rect:?}"
        );
        assert!(
            interaction_rect.0 <= content_rect.0 + 1.0
                && interaction_rect.1 <= content_rect.1 + 1.0
                && interaction_rect.0 + interaction_rect.2 >= content_rect.0 + content_rect.2 - 1.0
                && interaction_rect.1 + interaction_rect.3
                    >= content_rect.1 + content_rect.3 - 1.0,
            "the drawn popup content must sit inside its interaction rect"
        );

        let inside_x = content_rect.0 + content_rect.2 * 0.5;
        let inside_y = content_rect.1 + content_rect.3 * 0.5;
        {
            let renderer = app.renderer.as_mut().unwrap();
            renderer.last_mouse_x = inside_x;
            renderer.last_mouse_y = inside_y;
        }
        let editor_before = app.scroll_y.target;
        app.handle_main_mouse_wheel(winit::event::MouseScrollDelta::PixelDelta(
            winit::dpi::PhysicalPosition::new(0.0, -36.0),
        ));
        assert_eq!(
            app.scroll_y.target, editor_before,
            "a wheel inside the drawn hover frame must stay with the popup"
        );

        // The popup keeps its state while its animation collapses the painted frame to
        // zero size; the renderer must then stop owning the wheel through that frame.
        if let Some(popup) = app.hover.popup.as_mut() {
            popup.anim_progress = 0.0;
        }
        crate::render_view::reviewer_stage2_integration::review_v3_root_frame(&mut app);

        let (popup_kept, collapsed_rect) = (app.hover.popup.is_some(), app.hover.interaction_rect);
        assert!(popup_kept, "the collapsed popup stays in hover state");
        assert!(
            collapsed_rect.is_none(),
            "a popup painting a zero-size frame must not keep an interaction rect"
        );

        app.handle_main_mouse_wheel(winit::event::MouseScrollDelta::PixelDelta(
            winit::dpi::PhysicalPosition::new(0.0, -36.0),
        ));
        crate::app::mouse::clear_hover_popup(&mut app.hover);
        assert!(
            app.scroll_y.target > editor_before,
            "the wheel must reach the editor once the hover frame is not drawn"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn reviewer_stage3_fractional_scroll_thumb_press_preserves_displayed_position() {
        let source = (0..140)
            .map(|i| format!("paragraph {i:03} alpha beta gamma delta epsilon\n\n"))
            .collect::<String>();
        let (_context, mut app) =
            crate::render_view::reviewer_stage2_integration::fixture(&source, 900.0, 1.25);
        app.set_markdown_mode(crate::app::MarkdownMode::Read);
        crate::render_view::reviewer_stage2_integration::read_frame(&mut app);
        let max_scroll = app.markdown.read_scroll_bounds().expect("reader bounds");
        let fractional_scroll = max_scroll * 0.3137 + 0.37;
        app.scroll_y.current = fractional_scroll;
        app.scroll_y.target = (fractional_scroll + 180.0).min(max_scroll);
        app.scroll_y.velocity = 75.0;
        app.scroll_y.anim_speed = 9.0;
        crate::render_view::reviewer_stage2_integration::read_frame(&mut app);

        let scrollbar = app
            .ui_registry
            .rect_for(crate::ui_system::UiId::MarkdownReadScrollbar)
            .expect("reader scrollbar registry");
        let body = app
            .ui_registry
            .rect_for(crate::ui_system::UiId::MarkdownReadBody)
            .expect("reader body registry");
        let displayed_scroll = app.scroll_y.current.round();
        let thumb = crate::render_view::markdown_read::markdown_read_scrollbar_thumb(
            body.1,
            body.3,
            app.markdown.read_layout.content_height(),
            displayed_scroll,
            1.25,
        )
        .expect("reader thumb");
        let fraction = 0.37;
        app.renderer.as_mut().unwrap().last_mouse_x = scrollbar.0 + scrollbar.2 * 0.5;
        app.renderer.as_mut().unwrap().last_mouse_y = thumb.start + thumb.len * fraction;

        app.handle_main_mouse_input(&host_loop::HostLoop::headless(&host_loop::HeadlessLoopState::default()),
            winit::event::ElementState::Pressed,
            winit::event::MouseButton::Left,
        );

        assert!(app.scroll_y.is_dragging);
        assert!((app.scroll_y.current - displayed_scroll).abs() < 0.001);
        assert!((app.scroll_y.target - app.scroll_y.current).abs() < 0.01);
        assert_eq!(app.scroll_y.velocity, 0.0);
        assert_eq!(app.scroll_y.anim_speed, 15.0);
        assert!((app.scroll_y.drag_offset - thumb.len * fraction).abs() < 0.01);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn markdown_reader_scrollbar_track_click_centers_thumb_and_stale_layout_releases_capture() {
        let source = (0..100)
            .map(|i| format!("line {i:03} lorem ipsum dolor sit amet\n\n"))
            .collect::<String>();
        let (_context, mut app) =
            crate::render_view::reviewer_stage2_integration::fixture(&source, 780.0, 1.0);
        app.set_markdown_mode(crate::app::MarkdownMode::Read);
        crate::render_view::reviewer_stage2_integration::read_frame(&mut app);
        let body = app
            .ui_registry
            .rect_for(crate::ui_system::UiId::MarkdownReadBody)
            .unwrap();
        let max_scroll = app.markdown.read_scroll_bounds().unwrap();
        let thumb = crate::render_view::markdown_read::markdown_read_scrollbar_thumb(
            body.1,
            body.3,
            app.markdown.read_layout.content_height(),
            app.scroll_y.current.round(),
            1.0,
        )
        .unwrap();
        let press_y = body.1 + body.3 - 1.0;
        let (expected_offset, expected_target) = crate::scroll::scrollbar_drag_target(
            press_y,
            body.1,
            body.3,
            thumb,
            max_scroll,
            None,
        )
        .unwrap();
        assert!((expected_offset - thumb.len * 0.5).abs() < 0.01);

        let current_before_press = app.scroll_y.current;
        assert!(app.begin_markdown_read_scrollbar_drag_at(press_y));
        assert!(app.scroll_y.is_dragging);
        assert!((app.scroll_y.drag_offset - expected_offset).abs() < 0.01);
        assert_eq!(app.scroll_y.current, current_before_press);
        assert!((app.scroll_y.target - expected_target).abs() < 0.02);

        app.markdown.read_layout.invalidate();
        assert!(app.drag_markdown_read_scrollbar_to(body.1 + 10.0));
        assert!(!app.scroll_y.is_dragging);
        assert_eq!(app.scroll_y.drag_offset, 0.0);
        assert!(app.scroll_y.current.is_finite());
        assert!(app.scroll_y.target.is_finite());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn markdown_reader_selection_edge_tick_scrolls_updates_endpoint_and_settles_on_exit() {
        let mut source = String::new();
        for i in 0..180 {
            source.push_str(&format!("paragraph {i:03} alpha beta gamma delta epsilon zeta\n\n"));
        }
        let (_context, mut app) =
            crate::render_view::reviewer_stage2_integration::fixture(&source, 860.0, 1.0);
        app.set_markdown_mode(crate::app::MarkdownMode::Read);
        crate::render_view::reviewer_stage2_integration::read_frame(&mut app);
        let max_scroll = app.markdown.read_scroll_bounds().unwrap();
        app.scroll_y.jump_to(max_scroll * 0.35);
        crate::render_view::reviewer_stage2_integration::read_frame(&mut app);
        let body = app
            .ui_registry
            .rect_for(crate::ui_system::UiId::MarkdownReadBody)
            .unwrap();
        let x = body.0 + 120.0;
        let mid_y = body.1 + body.3 * 0.5;
        assert!(app.begin_markdown_read_selection_at(x, mid_y));
        let anchor = app.markdown.read_selection_anchor.unwrap();
        let hidden_editor = (app.editor.cursor, app.editor.selection_anchor, app.editor.version);
        let start_scroll = app.scroll_y.current;

        app.renderer.as_mut().unwrap().last_mouse_x = x;
        app.renderer.as_mut().unwrap().last_mouse_y = body.1 + body.3 - 2.0;
        let _ = app.update_markdown_read_selection_at(x, body.1 + body.3 - 2.0);
        let cursor_before_ticks = app.markdown.read_selection_cursor.unwrap();
        assert!(update_markdown_read_selection_autoscroll(&mut app, 0.016, false));
        assert!(app.markdown.read_selection_autoscrolling);
        assert!(app.scroll_y.target > app.scroll_y.current);

        for _ in 0..24 {
            let moved = app.scroll_y.update(0.016);
            let _ = update_markdown_read_selection_autoscroll(&mut app, 0.016, moved);
        }
        assert!(app.scroll_y.current > start_scroll);
        assert_eq!(app.markdown.read_selection_anchor, Some(anchor));
        assert!(app.markdown.read_selection_cursor.unwrap() >= cursor_before_ticks);
        assert_eq!(
            (app.editor.cursor, app.editor.selection_anchor, app.editor.version),
            hidden_editor
        );

        app.renderer.as_mut().unwrap().last_mouse_y = mid_y;
        let moved = app.scroll_y.update(0.016);
        let _ = update_markdown_read_selection_autoscroll(&mut app, 0.016, moved);
        assert!(!app.markdown.read_selection_autoscrolling);
        assert_eq!(app.scroll_y.target, app.scroll_y.current);
        assert_eq!(app.scroll_y.velocity, 0.0);
        assert!(app.markdown.read_selecting);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn markdown_reader_selection_top_edge_reverses_and_short_document_never_scrolls() {
        let source = (0..160)
            .map(|i| format!("row {i:03} alpha beta gamma delta\n\n"))
            .collect::<String>();
        let (_context, mut app) =
            crate::render_view::reviewer_stage2_integration::fixture(&source, 820.0, 1.5);
        app.set_markdown_mode(crate::app::MarkdownMode::Read);
        crate::render_view::reviewer_stage2_integration::read_frame(&mut app);
        let max_scroll = app.markdown.read_scroll_bounds().unwrap();
        app.scroll_y.jump_to(max_scroll * 0.65);
        crate::render_view::reviewer_stage2_integration::read_frame(&mut app);
        let body = app
            .ui_registry
            .rect_for(crate::ui_system::UiId::MarkdownReadBody)
            .unwrap();
        let x = body.0 + 100.0;
        assert!(app.begin_markdown_read_selection_at(x, body.1 + body.3 * 0.6));
        let before = app.scroll_y.current;
        app.renderer.as_mut().unwrap().last_mouse_x = x;
        app.renderer.as_mut().unwrap().last_mouse_y = body.1 + 2.0;
        assert!(update_markdown_read_selection_autoscroll(&mut app, 0.016, false));
        assert!(app.scroll_y.target < before);
        assert!(app.markdown.read_selection_autoscrolling);
        assert!(app.finish_markdown_read_selection_gesture());
        assert!(!app.markdown.read_selecting);
        assert!(!app.markdown.read_selection_autoscrolling);
        assert_eq!(app.scroll_y.target, app.scroll_y.current);

        let (_context, mut short) = crate::render_view::reviewer_stage2_integration::fixture(
            "one short paragraph\n",
            820.0,
            1.0,
        );
        short.set_markdown_mode(crate::app::MarkdownMode::Read);
        crate::render_view::reviewer_stage2_integration::read_frame(&mut short);
        let body = short
            .ui_registry
            .rect_for(crate::ui_system::UiId::MarkdownReadBody)
            .unwrap();
        assert_eq!(short.markdown.read_scroll_bounds(), Some(0.0));
        assert!(short.begin_markdown_read_selection_at(body.0 + 80.0, body.1 + 20.0));
        short.renderer.as_mut().unwrap().last_mouse_x = body.0 + 80.0;
        short.renderer.as_mut().unwrap().last_mouse_y = body.1 + body.3 - 1.0;
        let _ = update_markdown_read_selection_autoscroll(&mut short, 0.016, false);
        assert_eq!(short.scroll_y.current, 0.0);
        assert_eq!(short.scroll_y.target, 0.0);
        assert!(!short.markdown.read_selection_autoscrolling);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn reviewer_stage3_autoscroll_settles_without_redraw_loop_at_bottom_bound() {
        let source = (0..160)
            .map(|i| format!("row {i:03} alpha beta gamma delta\n\n"))
            .collect::<String>();
        let (_context, mut app) =
            crate::render_view::reviewer_stage2_integration::fixture(&source, 820.0, 1.0);
        app.set_markdown_mode(crate::app::MarkdownMode::Read);
        crate::render_view::reviewer_stage2_integration::read_frame(&mut app);
        let max_scroll = app.markdown.read_scroll_bounds().expect("reader bounds");
        app.scroll_y.jump_to(max_scroll);
        crate::render_view::reviewer_stage2_integration::read_frame(&mut app);
        let body = app
            .ui_registry
            .rect_for(crate::ui_system::UiId::MarkdownReadBody)
            .expect("reader body registry");
        let x = body.0 + 100.0;
        let y = body.1 + body.3 - 1.0;
        assert!(app.begin_markdown_read_selection_at(x, y));
        app.renderer.as_mut().unwrap().last_mouse_x = x;
        app.renderer.as_mut().unwrap().last_mouse_y = y;
        app.markdown.read_selection_autoscrolling = true;

        assert!(!update_markdown_read_selection_autoscroll(
            &mut app, 0.016, false,
        ));
        assert!(!app.markdown.read_selection_autoscrolling);
        assert_eq!(app.scroll_y.current, max_scroll);
        assert_eq!(app.scroll_y.target, max_scroll);
        assert_eq!(app.scroll_y.velocity, 0.0);
        assert!(!update_markdown_read_selection_autoscroll(
            &mut app, 0.016, false,
        ));
    }

