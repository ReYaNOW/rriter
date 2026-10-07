// Independent stage-2 integration regressions. Linux-only test harness uses
// a surfaceless EGL pbuffer: real Renderer/layout, no window or screenshot.
// No production geometry, mapping, transition, or input logic is reimplemented.
#[cfg(all(test, target_os = "linux"))]
pub(crate) mod reviewer_stage2_integration {
    use super::*;
    use crate::app::{App, MarkdownMode};
    use crate::render_view::markdown_read::MarkdownSourceAnchor;
    use crate::platform::offscreen_gl::OffscreenContext;

    pub(crate) fn fixture(source: &str, width: f32, scale: f32) -> (OffscreenContext, App) {
        let context = OffscreenContext::new(1000, 800).expect("offscreen EGL context");
        let mut app = crate::app::reviewer_stage2_test_app().expect("headless App");
        app.show_welcome = false;
        app.file_path = Some(std::path::PathBuf::from("/tmp/reviewer-stage2.md"));
        app.file_extension = "md".to_string();
        app.editor = crate::app::reviewer_stage2_editor_with(source);
        app.editor.cursor = 0;
        let mut renderer = Renderer::new(
            context.glow(),
            scale,
            app.theme.clone(),
            context.requested_context(),
            &mut crate::startup_trace::StartupTrace::disabled(),
        )
        .expect("production Renderer");
        renderer.width = width;
        renderer.height = 180.0;
        app.renderer = Some(renderer);
        (context, app)
    }

    fn document() -> String {
        let mut source = String::new();
        for i in 0..100 {
            source.push_str(&format!("paragraph{i:03} "));
            source.push_str(&"alpha beta gamma delta epsilon zeta ".repeat(9));
            source.push_str("\n\n");
        }
        source
    }

    pub(crate) fn read_frame(app: &mut App) {
        app.ui_registry.clear();
        let renderer = app.renderer.as_mut().unwrap();
        renderer.draw_markdown_read(
            &mut app.markdown,
            &app.markdown_media,
            &app.editor,
            &mut app.scroll_y,
            &[],
            &[],
            None,
            0.0,
            0.0,
            renderer.width,
            renderer.height,
            &mut app.ui_registry,
        );
        renderer.flush();
    }

    pub(crate) fn edit_transition(app: &mut App) -> bool {
        let renderer = app.renderer.as_mut().unwrap();
        renderer.resolve_markdown_edit_scroll_transition(
            &mut app.markdown,
            &app.markdown_media,
            &app.editor,
            &mut app.scroll_y,
            &app.current_sticky_lines,
            renderer.height,
        )
    }

    fn start_read_at(app: &mut App, needle: &str, offset: f32) -> f32 {
        app.set_markdown_mode(MarkdownMode::Read);
        read_frame(app);
        let byte = app.editor.get_full_text().find(needle).expect("needle");
        let line_y = app
            .markdown
            .read_layout
            .source_anchor_y(&(byte..byte + needle.len()))
            .unwrap();
        app.scroll_y.jump_to(line_y + offset);
        read_frame(app);
        line_y + offset
    }

    fn set_motion(app: &mut App, remaining: f32, velocity: f32) {
        app.scroll_y.target = app.scroll_y.current + remaining;
        app.scroll_y.velocity = velocity;
        app.scroll_y.anim_speed = 7.0;
    }

    fn edit_expected(app: &mut App, anchor: &MarkdownSourceAnchor) -> f32 {
        let r = app.renderer.as_mut().unwrap();
        anchor.projected_scroll_y(
            r.markdown_edit_source_y(&app.editor, &anchor.source_range)
                .unwrap(),
        )
    }

    #[test]
    fn reviewer_stage2_real_renderer_fifty_wrapped_round_trips_preserve_motion() {
        let source = document();
        let (_context, mut app) = fixture(&source, 320.0, 1.25);
        let line_byte = source.find("paragraph030").unwrap() + 150;
        app.set_markdown_mode(MarkdownMode::Read);
        read_frame(&mut app);
        let y = app
            .markdown
            .read_layout
            .source_anchor_y(&(line_byte..line_byte + 1))
            .unwrap()
            + 3.25;
        app.scroll_y.jump_to(y);
        set_motion(&mut app, 73.0, -24.0);
        let before = app.editor.get_full_text();
        for _ in 0..50 {
            app.set_markdown_mode(MarkdownMode::Edit);
            assert!(edit_transition(&mut app));
            app.set_markdown_mode(MarkdownMode::Read);
            read_frame(&mut app);
            assert!(
                (app.scroll_y.current - y).abs() <= 1.0,
                "roundtrip current={} expected={y}",
                app.scroll_y.current
            );
            assert!((app.scroll_y.target - app.scroll_y.current - 73.0).abs() < 0.01);
            assert_eq!(app.scroll_y.velocity, -24.0);
            assert_eq!(app.scroll_y.anim_speed, 7.0);
        }
        println!(
            "WARM_50 start={y} final={} cursor={}",
            app.scroll_y.current, app.editor.cursor
        );
        assert_eq!(app.editor.get_full_text(), before);
        assert_eq!(app.editor.cursor, 0);
    }

    #[test]
    fn reviewer_stage2_pending_edit_to_read_relative_input_keeps_source_rebase() {
        let source = document();
        let (_context, mut app) = fixture(&source, 320.0, 1.0);
        let byte = source.find("paragraph030").unwrap();
        let edit_y = app
            .renderer
            .as_mut()
            .unwrap()
            .markdown_edit_source_y(&app.editor, &(byte..byte + 1))
            .unwrap()
            + 3.25;
        app.scroll_y.jump_to(edit_y);
        set_motion(&mut app, 80.0, 32.0);
        app.set_markdown_mode(MarkdownMode::Read);
        let anchor = app
            .markdown
            .scroll_transition
            .as_ref()
            .unwrap()
            .anchor
            .clone()
            .unwrap();
        // The same production relative-input helper is called by Reader keys/wheel.
        crate::app::reviewer_stage2_scroll_read(&mut app.scroll_y, None, 36.0);
        read_frame(&mut app);
        let expected = anchor.projected_scroll_y(
            app.markdown
                .read_layout
                .source_anchor_y(&anchor.source_range)
                .unwrap(),
        );
        println!(
            "EDIT_READ_RELATIVE edit={edit_y} expected={expected} actual={} remaining={}",
            app.scroll_y.current,
            app.scroll_y.target - app.scroll_y.current
        );
        assert!(
            (app.scroll_y.current - expected).abs() <= 1.0,
            "relative input must not cancel coordinate conversion"
        );
        assert!((app.scroll_y.target - app.scroll_y.current - 116.0).abs() < 0.01);
    }

    #[test]
    fn reviewer_stage2_pending_read_to_edit_relative_input_keeps_source_rebase() {
        let source = document();
        let (_context, mut app) = fixture(&source, 320.0, 1.0);
        let read_y = start_read_at(&mut app, "paragraph030", 3.25);
        set_motion(&mut app, -80.0, -32.0);
        app.set_markdown_mode(MarkdownMode::Edit);
        let anchor = app
            .markdown
            .scroll_transition
            .as_ref()
            .unwrap()
            .anchor
            .clone()
            .unwrap();
        let expected = edit_expected(&mut app, &anchor);
        app.scroll_y.scroll_by(-36.0);
        let applied = edit_transition(&mut app);
        println!(
            "READ_EDIT_RELATIVE read={read_y} expected={expected} actual={} applied={applied}",
            app.scroll_y.current
        );
        assert!(
            applied,
            "relative input cannot discard the pending source rebase"
        );
        assert!((app.scroll_y.current - expected).abs() <= 1.0);
    }

    #[test]
    fn reviewer_stage2_block_gap_round_trip_preserves_position_without_navigation() {
        let source = document();
        let (_context, mut app) = fixture(&source, 320.0, 1.0);
        let y = start_read_at(&mut app, "paragraph030", -5.0);
        let anchor = app.markdown.read_layout.viewport_source_anchor(y).unwrap();
        assert!(
            anchor.viewport_offset_y > 0.0,
            "gap fixture needs a downstream anchor"
        );
        app.set_markdown_mode(MarkdownMode::Edit);
        assert!(edit_transition(&mut app));
        let edit_y = app.scroll_y.current;
        app.set_markdown_mode(MarkdownMode::Read);
        read_frame(&mut app);
        println!(
            "GAP_ROUNDTRIP read={y} anchor={anchor:?} edit={edit_y} returned={}",
            app.scroll_y.current
        );
        assert!(
            (app.scroll_y.current - y).abs() <= 1.0,
            "projection in the preceding source line must not invalidate its own carry"
        );
    }

    #[test]
    fn reviewer_stage2_reverse_pending_keeps_bounds_of_last_intended_mode() {
        let source = document();
        let (_context, mut app) = fixture(&source, 320.0, 1.0);
        let y = start_read_at(&mut app, "paragraph030", 3.25);
        set_motion(&mut app, 80.0, 32.0);
        let before_bounds = app.markdown.read_scroll_bounds();
        app.set_markdown_mode(MarkdownMode::Edit);
        app.set_markdown_mode(MarkdownMode::Read);
        assert_eq!(app.markdown.read_scroll_bounds(), before_bounds);
        assert!(app.markdown.scroll_transition.is_none());
        read_frame(&mut app);
        assert_eq!(app.scroll_y.current, y);
        assert_eq!(app.scroll_y.velocity, 32.0);
    }

    #[test]
    fn reviewer_stage2_elapsed_animation_tick_matches_control_after_rebase() {
        let source = document();
        let (_context, mut app) = fixture(&source, 320.0, 1.0);
        start_read_at(&mut app, "paragraph030", 3.25);
        set_motion(&mut app, 80.0, 32.0);
        let old = app.scroll_y.current;
        app.set_markdown_mode(MarkdownMode::Edit);
        let anchor = app
            .markdown
            .scroll_transition
            .as_ref()
            .unwrap()
            .anchor
            .clone()
            .unwrap();
        let dest = edit_expected(&mut app, &anchor);
        let mut control = app.scroll_y.clone();
        control.update(1.0 / 120.0);
        app.scroll_y.update(1.0 / 120.0);
        assert!(edit_transition(&mut app));
        println!(
            "ELAPSED old={old} destination={dest} control={} actual={}",
            control.current, app.scroll_y.current
        );
        assert!((app.scroll_y.current - (control.current + dest - old)).abs() <= 0.01);
        assert!((app.scroll_y.target - (control.target + dest - old)).abs() <= 0.01);
        assert_eq!(app.scroll_y.velocity, control.velocity);
    }

    #[test]
    fn reviewer_stage2_cold_first_frame_projects_viewport_not_cursor() {
        let source = document();
        let (_context, mut app) = fixture(&source, 320.0, 1.5);
        let byte = source.find("paragraph030").unwrap();
        let edit_y = app
            .renderer
            .as_mut()
            .unwrap()
            .markdown_edit_source_y(&app.editor, &(byte..byte + 1))
            .unwrap()
            + 3.5;
        app.scroll_y.jump_to(edit_y);
        set_motion(&mut app, 93.0, 0.0);
        app.set_markdown_mode(MarkdownMode::Read);
        let anchor = app
            .markdown
            .scroll_transition
            .as_ref()
            .unwrap()
            .anchor
            .clone()
            .unwrap();
        read_frame(&mut app);
        let expected = anchor.projected_scroll_y(
            app.markdown
                .read_layout
                .source_anchor_y(&anchor.source_range)
                .unwrap(),
        );
        println!(
            "COLD_FIRST_FRAME edit={edit_y} expected_read={expected} actual={}",
            app.scroll_y.current
        );
        assert!((app.scroll_y.current - expected).abs() < 0.01);
        assert_eq!(app.scroll_y.velocity, 0.0);
        assert!((app.scroll_y.target - app.scroll_y.current - 93.0).abs() < 0.01);
        assert_eq!(app.editor.cursor, 0);
        assert!(app.markdown.scroll_transition.is_none());
        read_frame(&mut app);
        assert!((app.scroll_y.current - expected).abs() < 0.01);
    }

    #[test]
    fn reviewer_stage2_raw_blank_run_round_trip_preserves_position() {
        let mut source = "before\n\n<pre>\n".to_string();
        source.push_str(&"value\n".repeat(80));
        source.push_str(&"\n".repeat(12));
        source.push_str("last\n</pre>\n\n");
        source.push_str(&document());
        let (_context, mut app) = fixture(&source, 360.0, 1.25);
        app.set_markdown_mode(MarkdownMode::Read);
        read_frame(&mut app);
        let byte = source.find("last\n</pre>").unwrap();
        let last_y = app
            .markdown
            .read_layout
            .source_anchor_y(&(byte..byte + 4))
            .unwrap();
        let y = last_y - 130.5;
        app.scroll_y.jump_to(y);
        let anchor = app.markdown.read_layout.viewport_source_anchor(y).unwrap();
        app.set_markdown_mode(MarkdownMode::Edit);
        assert!(edit_transition(&mut app));
        let edit_y = app.scroll_y.current;
        app.set_markdown_mode(MarkdownMode::Read);
        read_frame(&mut app);
        println!(
            "RAW_BLANK_ROUNDTRIP read={y} anchor={anchor:?} edit={edit_y} returned={}",
            app.scroll_y.current
        );
        assert!(
            (app.scroll_y.current - y).abs() <= 1.0,
            "blank fallback carry must survive its own projection"
        );
    }
    #[test]
    fn reviewer_stage2_v2_actual_wheel_pending_edit_preserves_residual_before_rebase() {
        let source = document();
        let (_context, mut app) = fixture(&source, 320.0, 1.0);
        let read_y = start_read_at(&mut app, "paragraph030", 3.25);
        set_motion(&mut app, 80.0, 31.0);
        app.set_markdown_mode(MarkdownMode::Edit);
        let anchor = app
            .markdown
            .scroll_transition
            .as_ref()
            .unwrap()
            .anchor
            .clone()
            .unwrap();
        let expected_edit = edit_expected(&mut app, &anchor);
        let r = app.renderer.as_mut().unwrap();
        r.last_mouse_x = 100.0;
        r.last_mouse_y = 80.0;
        let edit_max = r.get_max_scroll(&app.editor, r.height);
        assert!(read_y > edit_max);
        assert!(expected_edit > 300.0 && expected_edit + 44.0 < edit_max);
        app.handle_main_mouse_wheel(winit::event::MouseScrollDelta::PixelDelta(
            winit::dpi::PhysicalPosition::new(0.0, 36.0),
        ));
        let residual_before_resolve = app.scroll_y.target - app.scroll_y.current;
        assert!(edit_transition(&mut app));
        println!(
            "ACTUAL_WHEEL_PENDING read={read_y} edit_max={edit_max} residual_before={residual_before_resolve} expected_current={expected_edit} actual_current={} expected_residual=44 actual_residual={} target={}",
            app.scroll_y.current,
            app.scroll_y.target - app.scroll_y.current,
            app.scroll_y.target
        );
        assert!((app.scroll_y.current - expected_edit).abs() <= 1.0);
        assert!(
            (app.scroll_y.target - app.scroll_y.current - 44.0).abs() <= 1.0,
            "the real wheel handler must not clamp Reader coordinates to Editor bounds before rebase"
        );
        assert_eq!(app.scroll_y.velocity, 31.0);
    }

    #[test]
    fn reviewer_stage2_v2_actual_wheel_then_inverse_pending_keeps_read_motion() {
        let source = document();
        let (_context, mut app) = fixture(&source, 320.0, 1.0);
        let read_y = start_read_at(&mut app, "paragraph030", 3.25);
        set_motion(&mut app, 80.0, 31.0);
        app.set_markdown_mode(MarkdownMode::Edit);
        let r = app.renderer.as_mut().unwrap();
        r.last_mouse_x = 100.0;
        r.last_mouse_y = 80.0;
        app.handle_main_mouse_wheel(winit::event::MouseScrollDelta::PixelDelta(
            winit::dpi::PhysicalPosition::new(0.0, 36.0),
        ));
        app.set_markdown_mode(MarkdownMode::Read);
        read_frame(&mut app);
        println!(
            "WHEEL_INVERSE_PENDING read={read_y} current={} expected_target={} actual_target={}",
            app.scroll_y.current,
            read_y + 44.0,
            app.scroll_y.target
        );
        assert!((app.scroll_y.current - read_y).abs() <= 1.0);
        assert!((app.scroll_y.target - read_y - 44.0).abs() <= 1.0);
        assert!(app.markdown.scroll_transition.is_none());
    }

    #[test]
    fn reviewer_stage2_v2_actual_edit_wheel_retains_ordinary_boundary_clamp() {
        let source = document();
        let (_context, mut app) = fixture(&source, 320.0, 1.0);
        app.scroll_y.jump_to(1600.0);
        app.renderer.as_mut().unwrap().last_mouse_x = 100.0;
        app.renderer.as_mut().unwrap().last_mouse_y = 80.0;
        app.handle_main_mouse_wheel(winit::event::MouseScrollDelta::PixelDelta(
            winit::dpi::PhysicalPosition::new(0.0, -100_000.0),
        ));
        let r = app.renderer.as_mut().unwrap();
        let s = r.scale_factor;
        let inset = crate::render_view::editor_content_top_inset(false, false, false, s);
        let height = crate::render_view::editor_view_height(r.height, inset, 0.0, false, s);
        let max_scroll = r.get_max_scroll(&app.editor, height).round();
        println!(
            "ORDINARY_EDIT_WHEEL current={} target={} expected_max={max_scroll}",
            app.scroll_y.current, app.scroll_y.target
        );
        assert_eq!(app.scroll_y.current, 1600.0);
        assert_eq!(app.scroll_y.target, max_scroll);
    }

    #[test]
    fn reviewer_stage2_v2_carry_after_inertia_crosses_source_rows_uses_visible_content() {
        let source = document();
        let (_context, mut app) = fixture(&source, 320.0, 1.0);
        let read_y = start_read_at(&mut app, "paragraph030", 3.25);
        set_motion(&mut app, 208.0, 60.0);
        app.set_markdown_mode(MarkdownMode::Edit);
        assert!(edit_transition(&mut app));
        let original_edit_y = app.scroll_y.current;
        let original_anchor = app.markdown.scroll_carry.as_ref().unwrap().anchor.clone();
        for _ in 0..360 {
            app.scroll_y.update(1.0 / 120.0);
            if app.scroll_y.is_settled() {
                break;
            }
        }
        assert!(app.scroll_y.is_settled());
        assert!((app.scroll_y.current - original_edit_y - 208.0).abs() < 0.01);
        let new_edit_y = app.scroll_y.current;
        let visible_anchor = app
            .renderer
            .as_mut()
            .unwrap()
            .markdown_edit_viewport_anchor(&app.editor, new_edit_y)
            .unwrap();
        assert_ne!(visible_anchor.source_range, original_anchor.source_range);
        let source_text = app.editor.get_full_text();
        assert!(source_text[visible_anchor.source_range.clone()].starts_with("paragraph034"));
        let expected_read = visible_anchor.projected_scroll_y(
            app.markdown
                .read_layout
                .source_anchor_y(&visible_anchor.source_range)
                .unwrap(),
        );
        app.set_markdown_mode(MarkdownMode::Read);
        read_frame(&mut app);
        println!(
            "CARRY_CROSSED_ROWS read_start={read_y} edit_start={original_edit_y} edit_after_tick={new_edit_y} old_source={:?} visible_source={:?} expected_read={expected_read} actual_read={}",
            original_anchor.source_range, visible_anchor.source_range, app.scroll_y.current
        );
        assert!(
            (app.scroll_y.current - expected_read).abs() <= 1.0,
            "a saved fragment from paragraph030 must not override the now visible paragraph034"
        );
    }

    #[test]
    fn reviewer_stage2_v2_small_elapsed_motion_retains_wrapped_fragment_carry() {
        let source = document();
        let (_context, mut app) = fixture(&source, 320.0, 1.25);
        let byte = source.find("paragraph030").unwrap() + 150;
        app.set_markdown_mode(MarkdownMode::Read);
        read_frame(&mut app);
        let read_y = app
            .markdown
            .read_layout
            .source_anchor_y(&(byte..byte + 1))
            .unwrap()
            + 3.25;
        app.scroll_y.jump_to(read_y);
        set_motion(&mut app, 5.0, 0.0);
        app.set_markdown_mode(MarkdownMode::Edit);
        assert!(edit_transition(&mut app));
        let edit_y = app.scroll_y.current;
        app.scroll_y.update(1.0 / 120.0);
        let progress = app.scroll_y.current - edit_y;
        assert!(progress > 0.0 && progress < 1.0);
        app.set_markdown_mode(MarkdownMode::Read);
        read_frame(&mut app);
        println!(
            "SMALL_MOTION_CARRY start={read_y} progress={progress} actual={} expected={}",
            app.scroll_y.current,
            read_y + progress
        );
        assert!((app.scroll_y.current - read_y - progress).abs() <= 1.0);
    }

    #[test]
    fn reviewer_stage2_v2_explicit_absolute_navigation_replaces_pending_destination() {
        let source = document();
        let (_context, mut app) = fixture(&source, 320.0, 1.0);
        start_read_at(&mut app, "paragraph030", 3.25);
        app.set_markdown_mode(MarkdownMode::Edit);
        app.markdown.mark_absolute_scroll_navigation();
        app.scroll_y.jump_to(500.0);
        assert!(!edit_transition(&mut app));
        assert_eq!(app.scroll_y.current, 500.0);
        assert_eq!(app.scroll_y.target, 500.0);
        assert!(app.markdown.scroll_transition.is_none());
        assert!(app.markdown.scroll_carry.is_none());
    }

    // These tests go through the complete root draw, including real chrome and
    // registry replacement. Existing tests above intentionally remain unchanged.
    pub(crate) fn review_v3_root_frame(app: &mut App) {
        app.ui_registry.clear();
        let renderer = app.renderer.as_mut().unwrap();
        let (_, sticky) = renderer.draw(
            &mut app.editor,
            &app.base_title,
            app.file_path.as_ref(),
            &app.tabs,
            app.active_tab,
            app.scroll_x.current,
            &mut app.scroll_y,
            &mut app.markdown,
            1.0,
            false,
            &app.highlighter.spans,
            false,
            false,
            &app.search_results,
            app.search_current_idx,
            app.show_search,
            app.search_anim_y,
            &app.search_editor,
            app.search_focused,
            app.search_case_sensitive,
            app.show_welcome,
            &app.recent_files,
            &app.current_sticky_lines,
            app.sticky_anim_progress,
            app.sticky_anim_is_adding,
            app.is_ide_mode,
            &app.ide_panel,
            app.show_settings,
            None,
            &mut app.ui_registry,
            &mut app.hover,
            app.tab_scroll.current,
            &[],
            None,
            &[],
            &[],
            &app.ide_workspaces,
            false,
            "Файл открыт в режиме только чтение",
            None,
            &crate::app::git_blame::InlineBlameDwell::default(),
            &app.pdf_engine,
            app.pdf_dark_pages,
            &app.markdown_media,
            &app.empty_ide_open_label,
        );
        renderer.flush();
        app.target_sticky_lines = sticky;
    }

    fn review_v3_root_read_at(app: &mut App, source_byte: usize, offset: f32) -> f32 {
        app.set_markdown_mode(MarkdownMode::Read);
        review_v3_root_frame(app);
        let y = app
            .markdown
            .read_layout
            .source_anchor_y(&(source_byte..source_byte + 1))
            .unwrap()
            + offset;
        app.scroll_y.jump_to(y);
        review_v3_root_frame(app);
        y
    }

    #[test]
    fn reviewer_stage2_status_toggle_is_registered_only_for_markdown_across_width_and_dpi() {
        let source = "alpha\nbeta\ngamma\n";
        for width in [320.0, 480.0, 800.0, 1280.0, 1920.0] {
            for scale in [1.0, 1.25, 1.5, 2.0] {
                let (_context, mut app) = fixture(source, width * scale, scale);
                let renderer = app.renderer.as_mut().unwrap();
                renderer.height = 420.0 * scale;
                let markdown_path = std::path::PathBuf::from("/tmp/status-test.markdown");
                app.ui_registry.clear();
                renderer.draw_status_bar(
                    &app.editor,
                    Some((&markdown_path, crate::platform::TextEncoding::Utf16Le)),
                    MarkdownMode::Read,
                    None,
                    None,
                    &mut app.ui_registry,
                    scale,
                    -1.0,
                    -1.0,
                    0.0,
                    Some("Git operation with a deliberately long label"),
                    Some(125.5),
                    Some(0.42),
                );
                let toggle = app
                    .ui_registry
                    .rect_for(crate::ui_system::UiId::MarkdownModeToggle)
                    .expect("320px and wider Markdown status bar keeps compact toggle accessible");
                let status = app
                    .ui_registry
                    .rect_for(crate::ui_system::UiId::StatusBar)
                    .expect("status bar blocker");
                assert!(toggle.0 >= status.0 - 0.5);
                assert!(toggle.0 + toggle.2 <= status.0 + status.2 + 0.5);
                assert!(toggle.1 >= status.1 - 0.5);
                assert!(toggle.1 + toggle.3 <= status.1 + status.3 + 0.5);

                let source_path = std::path::PathBuf::from("/tmp/status-test.py");
                app.ui_registry.clear();
                renderer.draw_status_bar(
                    &app.editor,
                    Some((&source_path, crate::platform::TextEncoding::Utf16Le)),
                    MarkdownMode::Read,
                    None,
                    None,
                    &mut app.ui_registry,
                    scale,
                    -1.0,
                    -1.0,
                    0.0,
                    None,
                    None,
                    None,
                );
                assert_eq!(
                    app.ui_registry
                        .rect_for(crate::ui_system::UiId::MarkdownModeToggle),
                    None,
                    "ordinary source tabs must not acquire a Markdown toggle"
                );

                app.ui_registry.clear();
                renderer.draw_status_bar(
                    &app.editor,
                    None,
                    MarkdownMode::Edit,
                    Some(crate::app::pdf_tab::PdfStatus { dark: true, page: None }),
                    None,
                    &mut app.ui_registry,
                    scale,
                    -1.0,
                    -1.0,
                    0.0,
                    None,
                    None,
                    None,
                );
                assert!(app.ui_registry.rect_for(crate::ui_system::UiId::PdfDarkToggle).is_some());
            }
        }
    }

    #[test]
    fn reviewer_stage2_cold_read_width_uses_same_editor_gutter_geometry() {
        let mut source = String::new();
        for i in 0..1000 {
            source.push_str(&format!("line{i:04} alpha beta gamma delta epsilon\n"));
        }
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let (_context, mut app) = fixture(&source, 980.0 * scale, scale);
            app.set_markdown_mode(MarkdownMode::Read);
            assert!(app.prepare_markdown_absolute_scroll_target_navigation());

            let editor_text_x = crate::render_view::editor_left_padding_for(
                app.editor.line_offsets.len(),
                false,
                false,
                0.0,
                scale,
            );
            let frame_x = crate::render_view::markdown_read::markdown_read_frame_x_for_editor_text(
                editor_text_x,
                scale,
            );
            let expected_width = (app.renderer.as_ref().unwrap().width - frame_x).max(1.0);
            let renderer = app.renderer.as_ref().unwrap();
            assert!(app.markdown.read_layout.is_valid_for_geometry(
                app.editor.version,
                expected_width,
                renderer.scale_factor,
                renderer.font_size,
            ));

            app.is_ide_mode = true;
            app.ide_panel.left_width = 210.0;
            app.ide_panel.slots[0].open = true;
            let panel_left_w = app.ide_panel.visible_left_width(scale);
            let ide_text_x = crate::render_view::editor_left_padding_for(
                app.editor.line_offsets.len(),
                false,
                true,
                panel_left_w,
                scale,
            );
            let ide_frame_x =
                crate::render_view::markdown_read::markdown_read_frame_x_for_editor_text(
                    ide_text_x,
                    scale,
                );
            assert_eq!(
                app.markdown_read_content_width_for(
                    app.renderer.as_ref().unwrap().width,
                    scale,
                ),
                (app.renderer.as_ref().unwrap().width - ide_frame_x).max(1.0),
                "cold width calculation must include the same visible IDE panel gutter"
            );
        }
    }

    #[test]
    fn reviewer_stage2_v3_root_frames_fifty_round_trips_retain_real_chrome_geometry() {
        let source = document();
        let (_context, mut app) = fixture(&source, 720.0, 1.25);
        app.renderer.as_mut().unwrap().height = 420.0;
        let byte = source.find("paragraph030").unwrap() + 150;
        let y = review_v3_root_read_at(&mut app, byte, 3.25);
        set_motion(&mut app, 73.0, -24.0);
        let rebuilds = app.markdown.read_layout.rebuild_count();
        for _ in 0..50 {
            app.set_markdown_mode(MarkdownMode::Edit);
            review_v3_root_frame(&mut app);
            assert!(app.markdown.scroll_transition.is_none());
            assert!(
                app.ui_registry
                    .rect_for(crate::ui_system::UiId::EditorTextBody)
                    .is_some()
            );
            app.set_markdown_mode(MarkdownMode::Read);
            review_v3_root_frame(&mut app);
            assert!((app.scroll_y.current - y).abs() <= 1.0);
            assert!((app.scroll_y.target - app.scroll_y.current - 73.0).abs() < 0.01);
            assert_eq!(app.scroll_y.velocity, -24.0);
            assert_eq!(app.scroll_y.anim_speed, 7.0);
        }
        println!(
            "ROOT_WARM_50 start={y} returned={} rebuilds_before={rebuilds} rebuilds_after={}",
            app.scroll_y.current,
            app.markdown.read_layout.rebuild_count()
        );
        assert_eq!(app.markdown.read_layout.rebuild_count(), rebuilds);
        assert_eq!(app.editor.get_full_text(), source);
        assert_eq!(app.editor.cursor, 0);
    }

    #[test]
    fn reviewer_stage2_v3_real_edit_to_read_wheel_before_first_frame_is_not_lost() {
        let source = document();
        let (_context, mut app) = fixture(&source, 720.0, 1.0);
        app.renderer.as_mut().unwrap().height = 420.0;
        let byte = source.find("paragraph030").unwrap();
        let edit_y = app
            .renderer
            .as_mut()
            .unwrap()
            .markdown_edit_source_y(&app.editor, &(byte..byte + 1))
            .unwrap()
            + 3.25;
        app.scroll_y.jump_to(edit_y);
        review_v3_root_frame(&mut app);
        let (x, y, w, h) = app
            .ui_registry
            .rect_for(crate::ui_system::UiId::EditorTextBody)
            .unwrap();
        let (mx, my) = (x + w * 0.5, y + h * 0.5);
        assert_eq!(
            app.ui_registry.find_at(mx, my),
            Some(crate::ui_system::UiId::EditorTextBody)
        );
        app.renderer.as_mut().unwrap().last_mouse_x = mx;
        app.renderer.as_mut().unwrap().last_mouse_y = my;
        set_motion(&mut app, 80.0, 31.0);
        app.set_markdown_mode(MarkdownMode::Read);
        let anchor = app
            .markdown
            .scroll_transition
            .as_ref()
            .unwrap()
            .anchor
            .clone()
            .unwrap();
        app.handle_main_mouse_wheel(winit::event::MouseScrollDelta::PixelDelta(
            winit::dpi::PhysicalPosition::new(0.0, 36.0),
        ));
        let residual_before = app.scroll_y.target - app.scroll_y.current;
        review_v3_root_frame(&mut app);
        let expected = anchor.projected_scroll_y(
            app.markdown
                .read_layout
                .source_anchor_y(&anchor.source_range)
                .unwrap(),
        );
        println!(
            "EDIT_READ_REAL_WHEEL residual_before={residual_before} expected_current={expected} actual_current={} expected_residual=44 actual_residual={}",
            app.scroll_y.current,
            app.scroll_y.target - app.scroll_y.current
        );
        assert!((app.scroll_y.current - expected).abs() <= 1.0);
        assert!(
            (app.scroll_y.target - app.scroll_y.current - 44.0).abs() < 0.01,
            "a wheel event over the document between toggle and first Read frame must not disappear because the registry is still Edit"
        );
    }

    #[test]
    fn reviewer_stage2_v3_real_read_to_edit_wheel_full_root_preserves_fractional_residual() {
        let source = document();
        let (_context, mut app) = fixture(&source, 720.0, 1.25);
        app.renderer.as_mut().unwrap().height = 420.0;
        let byte = source.find("paragraph030").unwrap() + 150;
        let read_y = review_v3_root_read_at(&mut app, byte, 3.25);
        set_motion(&mut app, 80.125, 31.0);
        let (x, y, w, h) = app
            .ui_registry
            .rect_for(crate::ui_system::UiId::MarkdownReadBody)
            .unwrap();
        app.renderer.as_mut().unwrap().last_mouse_x = x + w * 0.5;
        app.renderer.as_mut().unwrap().last_mouse_y = y + h * 0.5;
        app.set_markdown_mode(MarkdownMode::Edit);
        let anchor = app
            .markdown
            .scroll_transition
            .as_ref()
            .unwrap()
            .anchor
            .clone()
            .unwrap();
        let expected = edit_expected(&mut app, &anchor);
        app.handle_main_mouse_wheel(winit::event::MouseScrollDelta::PixelDelta(
            winit::dpi::PhysicalPosition::new(0.0, 36.375),
        ));
        review_v3_root_frame(&mut app);
        println!(
            "READ_EDIT_ROOT_WHEEL read={read_y} expected={expected} current={} residual={}",
            app.scroll_y.current,
            app.scroll_y.target - app.scroll_y.current
        );
        assert!((app.scroll_y.current - expected).abs() <= 1.0);
        assert!((app.scroll_y.target - app.scroll_y.current - 43.75).abs() < 0.01);
        assert_eq!(app.scroll_y.velocity, 31.0);
    }

    #[test]
    fn reviewer_stage2_v3_search_during_pending_edit_keeps_current_in_destination_coordinates() {
        let source = document();
        let (_context, mut app) = fixture(&source, 720.0, 1.0);
        app.renderer.as_mut().unwrap().height = 420.0;
        let byte = source.find("paragraph030").unwrap();
        let read_y = review_v3_root_read_at(&mut app, byte, 3.25);
        app.set_markdown_mode(MarkdownMode::Edit);
        let anchor = app
            .markdown
            .scroll_transition
            .as_ref()
            .unwrap()
            .anchor
            .clone()
            .unwrap();
        let expected_current = edit_expected(&mut app, &anchor);
        let result = source.find("paragraph034").unwrap();
        let renderer = app.renderer.as_ref().unwrap();
        let scale = renderer.scale_factor;
        let visible_h = crate::render_view::editor_view_height(
            renderer.height,
            crate::render_view::editor_content_top_inset(false, false, false, scale),
            0.0,
            false,
            scale,
        );
        let line_y = app
            .renderer
            .as_mut()
            .unwrap()
            .markdown_edit_source_y(&app.editor, &(result..result + 12))
            .unwrap();
        let max_scroll = app
            .renderer
            .as_mut()
            .unwrap()
            .get_max_scroll(&app.editor, visible_h);
        app.scroll_y.set_target(line_y - visible_h * 0.20);
        app.search_results = vec![(result, result + 12)];
        app.search_current_idx = Some(0);
        app.jump_to_search_result();
        let destination_target = app.scroll_y.target;
        let expected_target = (line_y - visible_h * 0.35)
            .clamp(0.0, max_scroll)
            .round();
        assert_eq!(destination_target, expected_target);
        review_v3_root_frame(&mut app);
        println!(
            "SEARCH_DURING_PENDING read_current={read_y} expected_current={expected_current} actual_current={} expected_target={destination_target} actual_target={}",
            app.scroll_y.current, app.scroll_y.target
        );
        assert_eq!(
            app.scroll_y.target, destination_target,
            "the explicit search destination takes priority over old relative target"
        );
        assert!(
            (app.scroll_y.current - expected_current).abs() <= 1.0,
            "animated search changes destination target, not the coordinate system of current; first Edit frame must not retain Read pixels"
        );
    }

    #[test]
    fn reviewer_stage2_pending_edit_search_uses_destination_band_without_preconverted_target() {
        for (needle, expected_ratio) in [("paragraph034", None), ("paragraph045", Some(0.65))] {
            let source = document();
            let (_context, mut app) = fixture(&source, 720.0, 1.0);
            app.renderer.as_mut().unwrap().height = 420.0;
            let origin = source.find("paragraph030").unwrap();
            let _ = review_v3_root_read_at(&mut app, origin, 3.25);
            app.set_markdown_mode(MarkdownMode::Edit);
            let origin_target = app.scroll_y.target;
            assert!(app.prepare_markdown_absolute_scroll_target_navigation());
            assert!(app.scroll_y.update(f32::MIN_POSITIVE));
            assert_eq!(app.scroll_y.target, origin_target);
            let destination_current = app.scroll_y.current;

            let result = source.find(needle).unwrap();
            let line_y = app
                .renderer
                .as_mut()
                .unwrap()
                .markdown_edit_source_y(&app.editor, &(result..result + needle.len()))
                .unwrap();
            let renderer = app.renderer.as_ref().unwrap();
            let scale = renderer.scale_factor;
            let visible_h = crate::render_view::editor_view_height(
                renderer.height,
                crate::render_view::editor_content_top_inset(false, false, false, scale),
                0.0,
                false,
                scale,
            );
            let max_scroll = app
                .renderer
                .as_mut()
                .unwrap()
                .get_max_scroll(&app.editor, visible_h);
            let destination_ratio = (line_y - destination_current) / visible_h;
            if expected_ratio.is_none() {
                assert!((0.35..=0.65).contains(&destination_ratio));
            } else {
                assert!(destination_ratio > 0.65);
            }
            app.search_results = vec![(result, result + needle.len())];
            app.search_current_idx = Some(0);

            app.jump_to_search_result();

            let expected_target = expected_ratio.map_or(destination_current.round(), |ratio| {
                (line_y - visible_h * ratio)
                    .clamp(0.0, max_scroll)
                    .round()
            });
            assert_eq!(app.scroll_y.target, expected_target, "needle={needle}");
        }
    }

    #[test]
    fn reviewer_stage2_search_navigation_uses_nearest_edit_central_band_edge() {
        let source = document();
        let (_context, mut app) = fixture(&source, 720.0, 1.0);
        app.renderer.as_mut().unwrap().height = 420.0;
        let result = source.find("paragraph030").unwrap();
        let line_y = app
            .renderer
            .as_mut()
            .unwrap()
            .markdown_edit_source_y(&app.editor, &(result..result + 12))
            .unwrap();
        let renderer = app.renderer.as_ref().unwrap();
        let scale = renderer.scale_factor;
        let visible_h = crate::render_view::editor_view_height(
            renderer.height,
            crate::render_view::editor_content_top_inset(false, false, false, scale),
            0.0,
            false,
            scale,
        );
        let max_scroll = app
            .renderer
            .as_mut()
            .unwrap()
            .get_max_scroll(&app.editor, visible_h);

        app.scroll_y.jump_to(line_y - visible_h * 0.20);
        app.search_results = vec![(result, result + 12)];
        app.search_current_idx = Some(0);

        app.jump_to_search_result();

        let expected = (line_y - visible_h * 0.35)
            .clamp(0.0, max_scroll)
            .round();
        assert_eq!(app.scroll_y.target, expected);
    }

    #[test]
    fn reviewer_stage2_search_navigation_preserves_an_edit_target_inside_central_band() {
        let source = document();
        let (_context, mut app) = fixture(&source, 720.0, 1.0);
        app.renderer.as_mut().unwrap().height = 420.0;
        let result = source.find("paragraph030").unwrap();
        let line_y = app
            .renderer
            .as_mut()
            .unwrap()
            .markdown_edit_source_y(&app.editor, &(result..result + 12))
            .unwrap();
        let renderer = app.renderer.as_ref().unwrap();
        let scale = renderer.scale_factor;
        let visible_h = crate::render_view::editor_view_height(
            renderer.height,
            crate::render_view::editor_content_top_inset(false, false, false, scale),
            0.0,
            false,
            scale,
        );
        let previous_target = (line_y - visible_h * 0.42).round();
        app.scroll_y.jump_to(previous_target);
        app.search_results = vec![(result, result + 12)];
        app.search_current_idx = Some(0);

        app.jump_to_search_result();

        assert_eq!(app.scroll_y.target, previous_target);
    }

    #[test]
    fn reviewer_stage2_folded_edit_search_uses_visual_anchor_for_central_band() {
        let source = document();
        let (_context, mut app) = fixture(&source, 720.0, 1.0);
        app.renderer.as_mut().unwrap().height = 420.0;
        app.editor.foldable_lines.insert(10, 50);
        app.editor.folded_lines.insert(10);
        let result = source.find("paragraph030").unwrap();
        let line_y = app
            .renderer
            .as_mut()
            .unwrap()
            .markdown_edit_source_y(&app.editor, &(result..result + 12))
            .unwrap();
        let renderer = app.renderer.as_ref().unwrap();
        let scale = renderer.scale_factor;
        let visible_h = crate::render_view::editor_view_height(
            renderer.height,
            crate::render_view::editor_content_top_inset(false, false, false, scale),
            0.0,
            false,
            scale,
        );
        let previous_target = (line_y - visible_h * 0.42).round();
        app.scroll_y.jump_to(previous_target);
        app.search_results = vec![(result, result + 12)];
        app.search_current_idx = Some(0);

        app.jump_to_search_result();

        assert_eq!(app.scroll_y.target, previous_target);
        let viewport_ratio = (line_y - app.scroll_y.target) / visible_h;
        assert!((0.35..=0.65).contains(&viewport_ratio));
    }

    #[test]
    fn reviewer_stage2_in_band_read_search_replaces_the_recorded_navigation_range() {
        let source = document();
        let (_context, mut app) = fixture(&source, 720.0, 1.0);
        app.renderer.as_mut().unwrap().height = 420.0;
        app.set_markdown_mode(MarkdownMode::Read);
        read_frame(&mut app);
        let renderer = app.renderer.as_ref().unwrap();
        let scale = renderer.scale_factor;
        let visible_h = crate::render_view::editor_view_height(
            renderer.height,
            crate::render_view::editor_content_top_inset(false, false, false, scale),
            0.0,
            false,
            scale,
        );
        let max_scroll = app.markdown.read_scroll_bounds().unwrap_or(f32::MAX);
        let previous = source.find("paragraph020").unwrap();
        let current = source.find("paragraph030").unwrap();
        let previous_y = app
            .markdown
            .read_layout
            .source_target_y(&(previous..previous + 12))
            .unwrap();
        let current_y = app
            .markdown
            .read_layout
            .source_target_y(&(current..current + 12))
            .unwrap();
        let recorded_range = |app: &App| {
            app.markdown
                .pending_absolute_scroll_target(MarkdownMode::Read, app.scroll_y.target)
                .and_then(|(target, _)| match target {
                    crate::app::MarkdownAbsoluteScrollTarget::Source { source_range, .. } => {
                        Some(source_range)
                    }
                    _ => None,
                })
        };

        // The first jump lands outside the band and records `previous`.
        app.scroll_y.jump_to(previous_y - visible_h * 0.9);
        app.search_results = vec![(previous, previous + 12)];
        app.search_current_idx = Some(0);
        app.jump_to_search_result();
        assert_eq!(recorded_range(&app), Some(previous..previous + 12));

        // The second jump is already inside the band: the scroll target is preserved,
        // but the navigation record must follow the match the user jumped to.
        let in_band_target = (current_y - visible_h * 0.5).round();
        app.scroll_y.jump_to(in_band_target);
        app.search_results = vec![(current, current + 12)];
        app.search_current_idx = Some(0);
        app.jump_to_search_result();
        assert_eq!(app.scroll_y.target, in_band_target.clamp(0.0, max_scroll));
        read_frame(&mut app);

        let (target, relative_delta) = app
            .markdown
            .pending_absolute_scroll_target(MarkdownMode::Read, app.scroll_y.target)
            .expect("an in-band search jump keeps its absolute navigation record");
        let crate::app::MarkdownAbsoluteScrollTarget::Source {
            source_range,
            viewport_ratio,
        } = target
        else {
            panic!("an in-band search jump must record its source range");
        };
        assert_eq!(source_range, current..current + 12);
        assert!((0.35..=0.65).contains(&viewport_ratio));
        assert_eq!(
            relative_delta, 0.0,
            "keeping the in-band target must stay a zero-delta navigation"
        );
    }

    #[test]
    fn reviewer_stage2_search_navigation_uses_nearest_reader_central_band_edge() {
        let source = document();
        let (_context, mut app) = fixture(&source, 720.0, 1.0);
        app.renderer.as_mut().unwrap().height = 420.0;
        app.set_markdown_mode(MarkdownMode::Read);
        read_frame(&mut app);
        let result = source.find("paragraph030").unwrap();
        let target_y = app
            .markdown
            .read_layout
            .source_target_y(&(result..result + 12))
            .unwrap();
        let renderer = app.renderer.as_ref().unwrap();
        let scale = renderer.scale_factor;
        let visible_h = crate::render_view::editor_view_height(
            renderer.height,
            crate::render_view::editor_content_top_inset(false, false, false, scale),
            0.0,
            false,
            scale,
        );
        let max_scroll = app.markdown.read_scroll_bounds().unwrap();

        app.scroll_y.jump_to(target_y - visible_h * 0.80);
        app.search_results = vec![(result, result + 12)];
        app.search_current_idx = Some(0);

        app.jump_to_search_result();

        let expected = (target_y - visible_h * 0.65)
            .clamp(0.0, max_scroll)
            .round();
        assert_eq!(app.scroll_y.target, expected);
    }

    #[test]
    fn reviewer_stage2_v3_folded_fragment_retains_carry_without_navigation() {
        let source = document();
        let (_context, mut app) = fixture(&source, 720.0, 1.25);
        app.renderer.as_mut().unwrap().height = 420.0;
        // The existing editor fold map is used; no geometry fixture substitutes it.
        app.editor.foldable_lines.insert(50, 70);
        app.editor.folded_lines.insert(50);
        let byte = source.find("paragraph030").unwrap() + 150;
        let read_y = review_v3_root_read_at(&mut app, byte, 3.25);
        for _ in 0..10 {
            app.set_markdown_mode(MarkdownMode::Edit);
            review_v3_root_frame(&mut app);
            app.set_markdown_mode(MarkdownMode::Read);
            review_v3_root_frame(&mut app);
            assert!((app.scroll_y.current - read_y).abs() <= 1.0);
        }
        println!(
            "ROOT_FOLDED_ROUNDTRIP start={read_y} final={}",
            app.scroll_y.current
        );
        assert!(app.editor.folded_lines.contains(&50));
    }

    #[test]
    fn reviewer_stage2_v3_dpi_change_before_toggle_keeps_last_visible_read_fragment() {
        let source = document();
        let (_context, mut app) = fixture(&source, 720.0, 1.0);
        app.renderer.as_mut().unwrap().height = 420.0;
        let byte = source.find("paragraph030").unwrap() + 150;
        let read_y = review_v3_root_read_at(&mut app, byte, 3.25);
        let visible = app
            .markdown
            .read_layout
            .viewport_source_anchor(read_y)
            .unwrap();
        app.renderer.as_mut().unwrap().update_scale_factor(1.25);
        // The scale event can precede the queued redraw. The last visible Read
        // frame and source anchor remain the old geometry at this instant.
        let expected = edit_expected(&mut app, &visible);
        app.set_markdown_mode(MarkdownMode::Edit);
        review_v3_root_frame(&mut app);
        println!(
            "DPI_BEFORE_TOGGLE source={:?} expected_edit={expected} actual_edit={}",
            visible.source_range, app.scroll_y.current
        );
        assert!(
            (app.scroll_y.current - expected).abs() <= 1.0,
            "pending source capture must not interpret old Read Y using freshly rebuilt DPI geometry"
        );
    }

    #[test]
    fn reviewer_stage2_v3_stable_read_root_wheel_and_pending_blocker_control() {
        let source = document();
        let (_context, mut app) = fixture(&source, 720.0, 1.0);
        app.renderer.as_mut().unwrap().height = 420.0;
        let byte = source.find("paragraph030").unwrap();
        review_v3_root_read_at(&mut app, byte, 3.25);
        let (x, y, w, h) = app
            .ui_registry
            .rect_for(crate::ui_system::UiId::MarkdownReadBody)
            .unwrap();
        let (mx, my) = (x + w * 0.5, y + h * 0.5);
        app.renderer.as_mut().unwrap().last_mouse_x = mx;
        app.renderer.as_mut().unwrap().last_mouse_y = my;
        set_motion(&mut app, 80.0, 31.0);
        app.handle_main_mouse_wheel(winit::event::MouseScrollDelta::PixelDelta(
            winit::dpi::PhysicalPosition::new(0.0, 36.0),
        ));
        assert!((app.scroll_y.target - app.scroll_y.current - 44.0).abs() < 0.01);
        app.set_markdown_mode(MarkdownMode::Edit);
        review_v3_root_frame(&mut app);
        app.set_markdown_mode(MarkdownMode::Read);
        // Registered blocker models an unrelated surface that must not become
        // scroll-through when stale EditorTextBody is accepted for pending input.
        app.ui_registry
            .register_blocker(crate::ui_system::UiId::StatusBar, x, y, w, h, mx, my);
        let before = app.scroll_y.target;
        app.handle_main_mouse_wheel(winit::event::MouseScrollDelta::PixelDelta(
            winit::dpi::PhysicalPosition::new(0.0, 36.0),
        ));
        assert_eq!(app.scroll_y.target, before);
        println!("ROOT_WHEEL_CONTROLS stable_read_residual=44 pending_blocker_target={before}");
    }

    fn review_v4_edit_at(app: &mut App, source_byte: usize, offset: f32) -> MarkdownSourceAnchor {
        let line_y = app
            .renderer
            .as_mut()
            .unwrap()
            .markdown_edit_source_y(&app.editor, &(source_byte..source_byte + 1))
            .unwrap();
        app.scroll_y.jump_to(line_y + offset);
        review_v3_root_frame(app);
        let r = app.renderer.as_mut().unwrap();
        r.markdown_edit_viewport_anchor(&app.editor, app.scroll_y.current)
            .unwrap()
    }

    fn review_v4_search(app: &mut App, source: &str, needle: &str) {
        let byte = source.find(needle).unwrap();
        app.search_results = vec![(byte, byte + needle.len())];
        app.search_current_idx = Some(0);
        app.jump_to_search_result();
    }

    #[test]
    fn reviewer_stage2_v4_edit_dpi_before_toggle_preserves_displayed_source() {
        let source = document();
        let (_context, mut app) = fixture(&source, 720.0, 1.0);
        app.renderer.as_mut().unwrap().height = 420.0;
        let byte = source.find("paragraph030").unwrap();
        let visible = review_v4_edit_at(&mut app, byte, 3.25);
        let original_y = app.scroll_y.current;
        set_motion(&mut app, 73.0, -24.0);
        app.renderer.as_mut().unwrap().update_scale_factor(1.25);
        app.set_markdown_mode(MarkdownMode::Read);
        let captured = app
            .markdown
            .scroll_transition
            .as_ref()
            .unwrap()
            .anchor
            .clone()
            .unwrap();
        review_v3_root_frame(&mut app);
        let expected = visible.projected_scroll_y(
            app.markdown
                .read_layout
                .source_anchor_y(&visible.source_range)
                .unwrap(),
        );
        println!(
            "EDIT_DPI_BEFORE_TOGGLE origin_y={original_y} visible={visible:?} captured={captured:?} expected={expected} actual={} residual={}",
            app.scroll_y.current,
            app.scroll_y.target - app.scroll_y.current
        );
        assert!(
            (app.scroll_y.current - expected).abs() <= 1.0,
            "Edit origin must use last displayed metrics too, not new line_height to decode old current"
        );
        assert!((app.scroll_y.target - app.scroll_y.current - 73.0).abs() < 0.01);
        assert_eq!(app.scroll_y.velocity, -24.0);
    }

    #[test]
    fn reviewer_stage2_v4_read_width_before_toggle_preserves_displayed_source_control() {
        let source = document();
        let (_context, mut app) = fixture(&source, 720.0, 1.25);
        app.renderer.as_mut().unwrap().height = 420.0;
        let byte = source.find("paragraph030").unwrap() + 150;
        let start = review_v3_root_read_at(&mut app, byte, 3.25);
        let visible = app
            .markdown
            .read_layout
            .viewport_source_anchor(start)
            .unwrap();
        app.renderer.as_mut().unwrap().width = 450.0;
        let expected = edit_expected(&mut app, &visible);
        app.set_markdown_mode(MarkdownMode::Edit);
        review_v3_root_frame(&mut app);
        println!(
            "READ_WIDTH_CONTROL expected={expected} actual={}",
            app.scroll_y.current
        );
        assert!((app.scroll_y.current - expected).abs() <= 1.0);
    }

    #[test]
    fn reviewer_stage2_v4_read_search_then_inverse_pending_keeps_origin_current() {
        let source = document();
        let (_context, mut app) = fixture(&source, 720.0, 1.0);
        app.renderer.as_mut().unwrap().height = 420.0;
        let byte = source.find("paragraph030").unwrap();
        let start = review_v3_root_read_at(&mut app, byte, 3.25);
        app.set_markdown_mode(MarkdownMode::Edit);
        review_v4_search(&mut app, &source, "paragraph034");
        let edit_target = app.scroll_y.target;
        assert_eq!(app.scroll_y.current, start);
        app.set_markdown_mode(MarkdownMode::Read);
        let captured = app
            .markdown
            .scroll_transition
            .as_ref()
            .and_then(|t| t.anchor.clone());
        review_v3_root_frame(&mut app);
        println!(
            "READ_SEARCH_INVERSE expected_current={start} actual_current={} edit_target={edit_target} returned_target={} captured={captured:?}",
            app.scroll_y.current, app.scroll_y.target
        );
        assert!(
            (app.scroll_y.current - start).abs() <= 1.0,
            "without animation, current is still origin-Read; inverse pending must not reinterpret it as Edit"
        );
    }

    #[test]
    fn reviewer_stage2_v4_edit_search_then_inverse_pending_keeps_origin_current() {
        let source = document();
        let (_context, mut app) = fixture(&source, 720.0, 1.0);
        app.renderer.as_mut().unwrap().height = 420.0;
        let byte = source.find("paragraph030").unwrap();
        review_v3_root_read_at(&mut app, byte, 3.25);
        app.set_markdown_mode(MarkdownMode::Edit);
        review_v3_root_frame(&mut app);
        let start = app.scroll_y.current;
        app.set_markdown_mode(MarkdownMode::Read);
        review_v4_search(&mut app, &source, "paragraph034");
        let read_target = app.scroll_y.target;
        assert_eq!(app.scroll_y.current, start);
        app.set_markdown_mode(MarkdownMode::Edit);
        let captured = app
            .markdown
            .scroll_transition
            .as_ref()
            .and_then(|t| t.anchor.clone());
        review_v3_root_frame(&mut app);
        println!(
            "EDIT_SEARCH_INVERSE expected_current={start} actual_current={} read_target={read_target} returned_target={} captured={captured:?}",
            app.scroll_y.current, app.scroll_y.target
        );
        assert!(
            (app.scroll_y.current - start).abs() <= 1.0,
            "without animation, current is still origin-Edit; inverse pending must not reinterpret it as Read"
        );
    }

    #[test]
    fn reviewer_stage2_v4_search_before_pending_tick_matches_rebased_animation_control() {
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
        let destination_target = app.scroll_y.target;
        let mut control = app.scroll_y.clone();
        assert!(control.rebase_current_preserving_target(destination_current));
        assert!(control.update(1.0 / 120.0));
        assert!(app.scroll_y.update(1.0 / 120.0));
        review_v3_root_frame(&mut app);
        println!(
            "SEARCH_PENDING_TICK origin_read={start} destination_current={destination_current} target={destination_target} expected_current={} actual_current={} expected_velocity={} actual_velocity={}",
            control.current, app.scroll_y.current, control.velocity, app.scroll_y.velocity
        );
        assert!(
            (app.scroll_y.current - control.current).abs() <= 1.0,
            "the single animation tick must not integrate a Read current against an Edit target"
        );
        assert!((app.scroll_y.velocity - control.velocity).abs() < 0.01);
        assert_eq!(app.scroll_y.target, destination_target);
    }

    #[test]
    fn reviewer_stage2_v4_warm_read_search_during_pending_keeps_destination_target_control() {
        let source = document();
        let (_context, mut app) = fixture(&source, 720.0, 1.0);
        app.renderer.as_mut().unwrap().height = 420.0;
        let byte = source.find("paragraph030").unwrap();
        review_v3_root_read_at(&mut app, byte, 3.25);
        app.set_markdown_mode(MarkdownMode::Edit);
        review_v3_root_frame(&mut app);
        app.set_markdown_mode(MarkdownMode::Read);
        let anchor = app
            .markdown
            .scroll_transition
            .as_ref()
            .unwrap()
            .anchor
            .clone()
            .unwrap();
        let expected_current = anchor.projected_scroll_y(
            app.markdown
                .read_layout
                .source_anchor_y(&anchor.source_range)
                .unwrap(),
        );
        review_v4_search(&mut app, &source, "paragraph034");
        let destination_target = app.scroll_y.target;
        review_v3_root_frame(&mut app);
        println!(
            "READ_SEARCH_CONTROL expected_current={expected_current} actual_current={} expected_target={destination_target} actual_target={}",
            app.scroll_y.current, app.scroll_y.target
        );
        assert!((app.scroll_y.current - expected_current).abs() <= 1.0);
        assert_eq!(app.scroll_y.target, destination_target);
    }

    #[test]
    fn reviewer_stage2_v4_resize_read_search_target_uses_destination_layout() {
        let source = document();
        let (_context, mut app) = fixture(&source, 720.0, 1.0);
        app.renderer.as_mut().unwrap().height = 420.0;
        let byte = source.find("paragraph030").unwrap();
        review_v3_root_read_at(&mut app, byte, 3.25);
        app.set_markdown_mode(MarkdownMode::Edit);
        review_v3_root_frame(&mut app);
        // The cached Read source geometry belongs to the earlier wide frame.
        app.renderer.as_mut().unwrap().width = 450.0;
        review_v3_root_frame(&mut app);
        app.set_markdown_mode(MarkdownMode::Read);
        review_v4_search(&mut app, &source, "paragraph034");
        let stale_target = app.scroll_y.target;
        review_v3_root_frame(&mut app);
        let first_target = app.scroll_y.target;
        // The same production navigation, now against the actual destination
        // frame, is the reference. No alternative search/locator is implemented.
        review_v4_search(&mut app, &source, "paragraph034");
        let expected_target = app.scroll_y.target;
        println!(
            "READ_RESIZE_SEARCH stale_target={stale_target} first_frame_target={first_target} expected_target={expected_target}"
        );
        assert!(
            (first_target - expected_target).abs() <= 1.0,
            "pending absolute target must belong to destination layout, not a retained previous-width layout"
        );
    }

    #[test]
    fn reviewer_stage2_v4_cold_read_search_keeps_explicit_destination() {
        let source = document();
        let (_context, mut app) = fixture(&source, 720.0, 1.0);
        app.renderer.as_mut().unwrap().height = 420.0;
        let byte = source.find("paragraph030").unwrap();
        review_v4_edit_at(&mut app, byte, 3.25);
        app.set_markdown_mode(MarkdownMode::Read);
        review_v4_search(&mut app, &source, "paragraph034");
        review_v3_root_frame(&mut app);
        let first_target = app.scroll_y.target;
        review_v4_search(&mut app, &source, "paragraph034");
        let expected_target = app.scroll_y.target;
        println!(
            "READ_COLD_SEARCH first_frame_target={first_target} expected_target={expected_target}"
        );
        assert!(
            (first_target - expected_target).abs() <= 1.0,
            "explicit search while pending cold Read must resolve once geometry is ready, not disappear"
        );
    }

    include!("markdown_scroll_transition_late_tests.rs");

    include!("markdown_scroll_transition_review_v6_tests.rs");
    include!("markdown_scroll_transition_review_v7_tests.rs");
    include!("markdown_scroll_transition_review_v9_tests.rs");
    include!("markdown_scroll_transition_stage3_tests.rs");
}
