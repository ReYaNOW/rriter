// Review v7: geometry ownership after an applied deferred tick must survive
// another absolute navigation that prepares a newer destination layout before
// the first destination frame. These tests reuse the existing offscreen EGL
// harness and production search/Home/End/Renderer paths from the parent module.

fn review_v7_applied_read_anchor_before_new_geometry(
    app: &mut App,
    source: &str,
) -> MarkdownSourceAnchor {
    let byte = source.find("paragraph030").unwrap();
    review_v4_edit_at(app, byte, 3.25);
    app.set_markdown_mode(MarkdownMode::Read);
    review_v4_search(app, source, "paragraph034");
    assert!(
        app.scroll_y.update(1.0 / 120.0),
        "the single physics tick must consume the deferred current rebase"
    );
    assert_eq!(app.scroll_y.deferred_current_rebase_applied(), Some(true));
    let geometry = app
        .markdown
        .deferred_read_current_geometry()
        .expect("applied current owns prepared Read geometry");
    assert!(app.markdown.read_layout.is_valid_for_geometry(
        geometry.version,
        geometry.width,
        geometry.scale,
        geometry.font_size,
    ));
    app.markdown
        .read_layout
        .viewport_source_anchor(app.scroll_y.current)
        .expect("live source anchor in geometry that owns current")
}

fn review_v7_expected_read_current(app: &mut App, anchor: &MarkdownSourceAnchor) -> f32 {
    let line_y = app
        .markdown
        .read_layout
        .source_anchor_y(&anchor.source_range)
        .expect("anchor in current Read layout");
    anchor.projected_scroll_y(line_y)
}

#[test]
fn reviewer_stage2_v7_second_search_after_tick_and_resize_reprojects_live_current() {
    let source = document();
    let (_context, mut app) = fixture(&source, 720.0, 1.0);
    app.renderer.as_mut().unwrap().height = 420.0;

    let live_anchor = review_v7_applied_read_anchor_before_new_geometry(&mut app, &source);
    let before_resize_current = app.scroll_y.current;
    let old_geometry = app.markdown.deferred_read_current_geometry().unwrap();
    assert_eq!(old_geometry.width.round(), 720.0);

    // Resize after tick, then issue a *new* absolute search before any Read
    // frame. jump_to_search_result calls the production preparation path,
    // which is allowed to rebuild the one Read layout for width=450.
    app.renderer.as_mut().unwrap().width = 450.0;
    review_v4_search(&mut app, &source, "paragraph038");
    assert!(app.markdown.read_layout.is_valid_for_geometry(
        app.editor.version,
        450.0,
        app.renderer.as_ref().unwrap().scale_factor,
        app.renderer.as_ref().unwrap().font_size,
    ));
    let expected_current = review_v7_expected_read_current(&mut app, &live_anchor);
    let expected_target = app.scroll_y.target;

    review_v3_root_frame(&mut app);
    println!(
        "V7_RESIZE_SECOND_SEARCH before={before_resize_current} expected_current={expected_current} actual_current={} expected_target={expected_target} actual_target={}",
        app.scroll_y.current, app.scroll_y.target
    );
    assert!(
        (app.scroll_y.current - expected_current).abs() <= 1.0,
        "new search preparation must not destroy the source ownership of an already-applied current"
    );
    assert!((app.scroll_y.target - expected_target).abs() <= 1.0);
}

#[test]
fn reviewer_stage2_v7_second_search_after_tick_and_dpi_change_reprojects_live_current() {
    let source = document();
    let (_context, mut app) = fixture(&source, 720.0, 1.0);
    app.renderer.as_mut().unwrap().height = 420.0;

    let live_anchor = review_v7_applied_read_anchor_before_new_geometry(&mut app, &source);
    let before_scale_current = app.scroll_y.current;
    let old_geometry = app.markdown.deferred_read_current_geometry().unwrap();
    assert_eq!(old_geometry.scale, 1.0);

    // Same ownership hazard as resize, but through the real renderer DPI path.
    app.renderer.as_mut().unwrap().update_scale_factor(1.25);
    review_v4_search(&mut app, &source, "paragraph038");
    let current_geometry = app.renderer.as_ref().unwrap();
    assert!(app.markdown.read_layout.is_valid_for_geometry(
        app.editor.version,
        current_geometry.width,
        current_geometry.scale_factor,
        current_geometry.font_size,
    ));
    let expected_current = review_v7_expected_read_current(&mut app, &live_anchor);
    let expected_target = app.scroll_y.target;

    review_v3_root_frame(&mut app);
    println!(
        "V7_DPI_SECOND_SEARCH before={before_scale_current} expected_current={expected_current} actual_current={} expected_target={expected_target} actual_target={}",
        app.scroll_y.current, app.scroll_y.target
    );
    assert!((app.scroll_y.current - expected_current).abs() <= 1.0);
    assert!((app.scroll_y.target - expected_target).abs() <= 1.0);
}

#[test]
fn reviewer_stage2_v7_end_after_tick_and_resize_keeps_live_source_before_destination() {
    let source = document();
    let (_context, mut app) = fixture(&source, 720.0, 1.0);
    app.renderer.as_mut().unwrap().height = 420.0;

    let live_anchor = review_v7_applied_read_anchor_before_new_geometry(&mut app, &source);
    app.renderer.as_mut().unwrap().width = 450.0;

    // This is the same production preparation used by the Read End key. The
    // test intentionally does not call a private alternative mapping helper.
    assert!(app.prepare_markdown_absolute_scroll_target_navigation());
    let max_scroll = app
        .markdown
        .read_scroll_bounds()
        .expect("prepared current Read bounds");
    app.markdown.mark_absolute_scroll_end_navigation();
    app.scroll_y.animate_to(max_scroll);
    app.markdown
        .remember_pending_absolute_scroll_target_y(max_scroll);
    let expected_current = review_v7_expected_read_current(&mut app, &live_anchor);

    review_v3_root_frame(&mut app);
    println!(
        "V7_RESIZE_END expected_current={expected_current} actual_current={} max_scroll={max_scroll} target={}",
        app.scroll_y.current, app.scroll_y.target
    );
    assert!(
        (app.scroll_y.current - expected_current).abs() <= 1.0,
        "Home/End preparation must preserve the same applied-current ownership as search"
    );
    assert!((app.scroll_y.target - max_scroll).abs() <= 1.0);
}

#[test]
fn reviewer_stage2_v7_inverse_after_tick_resize_and_second_search_uses_pre_resize_current_anchor() {
    let source = document();
    let (_context, mut app) = fixture(&source, 720.0, 1.0);
    app.renderer.as_mut().unwrap().height = 420.0;

    let live_anchor = review_v7_applied_read_anchor_before_new_geometry(&mut app, &source);
    let expected_edit_current = edit_expected(&mut app, &live_anchor);

    app.renderer.as_mut().unwrap().width = 450.0;
    review_v4_search(&mut app, &source, "paragraph038");
    let residual_before_inverse = app.scroll_y.target - app.scroll_y.current;

    // Return before a Read frame. The mode API must decode `current` using the
    // geometry that actually owns it, even though search has already prepared
    // the one cache for a newer width.
    app.set_markdown_mode(MarkdownMode::Edit);
    review_v3_root_frame(&mut app);
    let residual_after_inverse = app.scroll_y.target - app.scroll_y.current;
    println!(
        "V7_RESIZE_SECOND_SEARCH_INVERSE expected_current={expected_edit_current} actual_current={} residual_before={residual_before_inverse} residual_after={residual_after_inverse}",
        app.scroll_y.current
    );
    assert!((app.scroll_y.current - expected_edit_current).abs() <= 1.0);
    assert!((residual_after_inverse - residual_before_inverse).abs() <= 0.01);
}

#[test]
fn reviewer_stage2_v7_second_search_same_geometry_control_keeps_current_and_new_target() {
    let source = document();
    let (_context, mut app) = fixture(&source, 720.0, 1.0);
    app.renderer.as_mut().unwrap().height = 420.0;

    review_v7_applied_read_anchor_before_new_geometry(&mut app, &source);
    let current_before = app.scroll_y.current;
    review_v4_search(&mut app, &source, "paragraph038");
    let target_before = app.scroll_y.target;
    review_v3_root_frame(&mut app);
    println!(
        "V7_SAME_GEOMETRY_SEARCH current_before={current_before} current_after={} target_before={target_before} target_after={}",
        app.scroll_y.current, app.scroll_y.target
    );
    assert!((app.scroll_y.current - current_before).abs() <= 1.0);
    assert!((app.scroll_y.target - target_before).abs() <= 1.0);
}

#[test]
fn reviewer_stage2_v7_relative_wheel_after_second_search_same_geometry_control_survives() {
    let source = document();
    let (_context, mut app) = fixture(&source, 720.0, 1.0);
    app.renderer.as_mut().unwrap().height = 420.0;

    review_v7_applied_read_anchor_before_new_geometry(&mut app, &source);
    review_v4_search(&mut app, &source, "paragraph038");
    let base_target = app.scroll_y.target;

    // Point at the stale Edit body from the last displayed frame. Pending Read
    // explicitly accepts that document surface; the full wheel handler must
    // apply the relative delta and the later resolve must preserve it.
    review_v6_point_at_body(&mut app, crate::ui_system::UiId::EditorTextBody);
    review_v6_wheel_up(&mut app);
    let wheel_target = app.scroll_y.target;
    assert!((wheel_target - (base_target - 36.0)).abs() <= 0.01);

    review_v3_root_frame(&mut app);
    println!(
        "V7_SECOND_SEARCH_WHEEL base_target={base_target} wheel_target={wheel_target} resolved_target={}",
        app.scroll_y.target
    );
    assert!((app.scroll_y.target - wheel_target).abs() <= 0.01);
}

// Review v8: symmetric Edit geometry ownership. A deferred Read->Edit current
// can be consumed by the single physics tick before the first Edit frame; later
// absolute navigation must reproject that live current before assigning a target
// in newer Edit metrics.
fn review_v8_applied_edit_anchor_before_new_geometry(
    app: &mut App,
    source: &str,
) -> MarkdownSourceAnchor {
    let byte = source.find("paragraph030").unwrap();
    review_v3_root_read_at(app, byte, 3.25);
    app.set_markdown_mode(MarkdownMode::Edit);
    review_v4_search(app, source, "paragraph034");
    assert!(app.scroll_y.update(1.0 / 120.0));
    assert_eq!(app.scroll_y.deferred_current_rebase_applied(), Some(true));
    let (geometry, viewport_inset) = app
        .markdown
        .deferred_edit_current_geometry()
        .expect("applied current owns prepared Edit geometry");
    let renderer = app.renderer.as_mut().unwrap();
    renderer
        .markdown_edit_viewport_anchor_with_line_height(
            &app.editor,
            app.scroll_y.current + viewport_inset,
            geometry.line_height,
        )
        .expect("live source anchor in geometry that owns Edit current")
}

fn review_v8_expected_edit_current(app: &mut App, anchor: &MarkdownSourceAnchor) -> f32 {
    let renderer = app.renderer.as_mut().unwrap();
    let sticky_inset = app.current_sticky_lines.len() as f32 * renderer.line_height;
    let line_y = renderer
        .markdown_edit_source_y_with_line_height(
            &app.editor,
            &anchor.source_range,
            renderer.line_height,
        )
        .expect("anchor in current Edit geometry");
    anchor.projected_scroll_y(line_y) - sticky_inset
}

fn review_v8_edit_max_scroll(app: &mut App) -> f32 {
    let show_welcome = app.show_welcome;
    let is_ide_mode = app.is_ide_mode;
    let database_query = app.active_tab_is_database_query();
    let renderer = app.renderer.as_mut().unwrap();
    let tab_bar_h = crate::render_view::editor_content_top_inset(
        show_welcome,
        is_ide_mode,
        database_query,
        renderer.scale_factor,
    );
    let panel_bottom_h = if is_ide_mode {
        app.ide_panel
            .editor_reserved_bottom_height(renderer.scale_factor)
    } else {
        0.0
    };
    let visible_h = crate::render_view::editor_view_height(
        renderer.height,
        tab_bar_h,
        panel_bottom_h,
        is_ide_mode,
        renderer.scale_factor,
    );
    renderer.get_max_scroll(&app.editor, visible_h)
}

#[test]
fn reviewer_stage2_v8_edit_search_after_applied_tick_and_dpi_is_coherent_before_second_tick() {
    let source = document();
    let (_context, mut app) = fixture(&source, 720.0, 1.0);
    app.renderer.as_mut().unwrap().height = 420.0;

    let live_anchor = review_v8_applied_edit_anchor_before_new_geometry(&mut app, &source);
    app.renderer.as_mut().unwrap().update_scale_factor(1.25);
    let expected_at_prepare = review_v8_expected_edit_current(&mut app, &live_anchor);

    review_v4_search(&mut app, &source, "paragraph038");
    assert!((app.scroll_y.current - expected_at_prepare).abs() <= 1.0);
    let target = app.scroll_y.target;

    let mut control = app.scroll_y.clone();
    control.current = expected_at_prepare;
    assert!(control.update(1.0 / 120.0));
    assert!(app.scroll_y.update(1.0 / 120.0));
    println!(
        "V8_EDIT_SEARCH_TICK expected_current={} actual_current={} expected_velocity={} actual_velocity={} target={target}",
        control.current, app.scroll_y.current, control.velocity, app.scroll_y.velocity
    );
    assert!((app.scroll_y.current - control.current).abs() <= 1.0);
    assert!((app.scroll_y.velocity - control.velocity).abs() <= 0.01);
    assert_eq!(app.scroll_y.target, target);
}

#[test]
fn reviewer_stage2_v8_edit_end_after_applied_tick_and_dpi_is_coherent_before_second_tick() {
    let source = document();
    let (_context, mut app) = fixture(&source, 720.0, 1.0);
    app.renderer.as_mut().unwrap().height = 420.0;

    let live_anchor = review_v8_applied_edit_anchor_before_new_geometry(&mut app, &source);
    app.renderer.as_mut().unwrap().update_scale_factor(1.25);
    let expected_at_prepare = review_v8_expected_edit_current(&mut app, &live_anchor);

    assert!(app.prepare_markdown_absolute_scroll_target_navigation());
    assert!((app.scroll_y.current - expected_at_prepare).abs() <= 1.0);
    let max_scroll = review_v8_edit_max_scroll(&mut app);
    app.markdown.mark_absolute_scroll_end_navigation();
    app.scroll_y.animate_to(max_scroll);
    app.markdown
        .remember_pending_absolute_scroll_target_y(max_scroll);

    let mut control = app.scroll_y.clone();
    control.current = expected_at_prepare;
    assert!(control.update(1.0 / 120.0));
    assert!(app.scroll_y.update(1.0 / 120.0));
    println!(
        "V8_EDIT_END_TICK expected_current={} actual_current={} expected_velocity={} actual_velocity={} target={max_scroll}",
        control.current, app.scroll_y.current, control.velocity, app.scroll_y.velocity
    );
    assert!((app.scroll_y.current - control.current).abs() <= 1.0);
    assert!((app.scroll_y.velocity - control.velocity).abs() <= 0.01);
    assert!((app.scroll_y.target - max_scroll).abs() <= 0.01);
}

#[test]
fn reviewer_stage2_v8_edit_search_after_dpi_reprojects_before_any_second_tick() {
    let source = document();
    let (_context, mut app) = fixture(&source, 720.0, 1.0);
    app.renderer.as_mut().unwrap().height = 420.0;

    let live_anchor = review_v8_applied_edit_anchor_before_new_geometry(&mut app, &source);
    app.renderer.as_mut().unwrap().update_scale_factor(1.25);
    let expected_current = review_v8_expected_edit_current(&mut app, &live_anchor);
    review_v4_search(&mut app, &source, "paragraph038");
    println!(
        "V8_EDIT_SEARCH_NO_SECOND_TICK expected_current={expected_current} actual_current={} target={}",
        app.scroll_y.current, app.scroll_y.target
    );
    assert!((app.scroll_y.current - expected_current).abs() <= 1.0);
}

#[test]
fn reviewer_stage2_v8_same_geometry_edit_absolute_navigation_keeps_current() {
    let source = document();
    let (_context, mut app) = fixture(&source, 720.0, 1.0);
    app.renderer.as_mut().unwrap().height = 420.0;

    review_v8_applied_edit_anchor_before_new_geometry(&mut app, &source);
    let before = app.scroll_y.current;
    review_v4_search(&mut app, &source, "paragraph038");
    println!(
        "V8_EDIT_SAME_GEOMETRY current_before={before} current_after={} target={}",
        app.scroll_y.current, app.scroll_y.target
    );
    assert!((app.scroll_y.current - before).abs() <= 0.01);
}

#[test]
fn reviewer_stage2_v8_relative_wheel_after_new_edit_search_survives_resolve() {
    let source = document();
    let (_context, mut app) = fixture(&source, 720.0, 1.0);
    app.renderer.as_mut().unwrap().height = 420.0;

    review_v8_applied_edit_anchor_before_new_geometry(&mut app, &source);
    app.renderer.as_mut().unwrap().update_scale_factor(1.25);
    review_v4_search(&mut app, &source, "paragraph038");
    let base_target = app.scroll_y.target;
    review_v6_point_at_body(&mut app, crate::ui_system::UiId::MarkdownReadBody);
    review_v6_wheel_up(&mut app);
    let wheel_target = app.scroll_y.target;
    assert!((wheel_target - (base_target - 36.0)).abs() <= 0.01);
    review_v3_root_frame(&mut app);
    println!(
        "V8_EDIT_SEARCH_WHEEL base_target={base_target} wheel_target={wheel_target} resolved_target={}",
        app.scroll_y.target
    );
    assert!((app.scroll_y.target - wheel_target).abs() <= 0.01);
}

#[test]
fn reviewer_stage2_v8_inverse_after_edit_geometry_reprojection_keeps_source_and_residual() {
    let source = document();
    let (_context, mut app) = fixture(&source, 720.0, 1.0);
    app.renderer.as_mut().unwrap().height = 420.0;

    review_v8_applied_edit_anchor_before_new_geometry(&mut app, &source);
    app.renderer.as_mut().unwrap().update_scale_factor(1.25);
    review_v4_search(&mut app, &source, "paragraph038");
    let residual_before = app.scroll_y.target - app.scroll_y.current;
    let renderer = app.renderer.as_mut().unwrap();
    let sticky_inset = app.current_sticky_lines.len() as f32 * renderer.line_height;
    let live_anchor = renderer
        .markdown_edit_viewport_anchor_with_line_height(
            &app.editor,
            app.scroll_y.current + sticky_inset,
            renderer.line_height,
        )
        .unwrap();

    app.set_markdown_mode(MarkdownMode::Read);
    review_v3_root_frame(&mut app);
    let expected_read = live_anchor.projected_scroll_y(
        app.markdown
            .read_layout
            .source_anchor_y(&live_anchor.source_range)
            .unwrap(),
    );
    let residual_after = app.scroll_y.target - app.scroll_y.current;
    println!(
        "V8_EDIT_REPROJECT_INVERSE expected_read={expected_read} actual_read={} residual_before={residual_before} residual_after={residual_after}",
        app.scroll_y.current
    );
    assert!((app.scroll_y.current - expected_read).abs() <= 1.0);
    assert!((residual_after - residual_before).abs() <= 0.01);
}

#[test]
fn reviewer_stage2_v8_stop_after_edit_geometry_reprojection_remains_settled() {
    let source = document();
    let (_context, mut app) = fixture(&source, 720.0, 1.0);
    app.renderer.as_mut().unwrap().height = 420.0;

    review_v8_applied_edit_anchor_before_new_geometry(&mut app, &source);
    app.renderer.as_mut().unwrap().update_scale_factor(1.25);
    review_v4_search(&mut app, &source, "paragraph038");
    crate::app::mouse::stop_click_scroll_anims(&mut app, false);
    let stopped = app.scroll_y.current;
    assert_eq!(app.scroll_y.target, stopped);
    assert_eq!(app.scroll_y.velocity, 0.0);
    review_v3_root_frame(&mut app);
    app.scroll_y.update(1.0 / 120.0);
    println!(
        "V8_EDIT_REPROJECT_STOP stopped={stopped} current={} target={} velocity={}",
        app.scroll_y.current, app.scroll_y.target, app.scroll_y.velocity
    );
    assert!((app.scroll_y.current - stopped).abs() <= 1.0);
    assert_eq!(app.scroll_y.target, app.scroll_y.current);
    assert_eq!(app.scroll_y.velocity, 0.0);
}

#[test]
fn reviewer_stage2_v8_read_resize_second_search_tick_control_stays_coherent() {
    let source = document();
    let (_context, mut app) = fixture(&source, 720.0, 1.0);
    app.renderer.as_mut().unwrap().height = 420.0;

    let live_anchor = review_v7_applied_read_anchor_before_new_geometry(&mut app, &source);
    app.renderer.as_mut().unwrap().width = 450.0;
    review_v4_search(&mut app, &source, "paragraph038");
    let expected_at_prepare = review_v7_expected_read_current(&mut app, &live_anchor);
    assert!((app.scroll_y.current - expected_at_prepare).abs() <= 1.0);
    let target = app.scroll_y.target;

    let mut control = app.scroll_y.clone();
    control.current = expected_at_prepare;
    assert!(control.update(1.0 / 120.0));
    assert!(app.scroll_y.update(1.0 / 120.0));
    println!(
        "V8_READ_RESIZE_SECOND_SEARCH_TICK expected_current={} actual_current={} expected_velocity={} actual_velocity={} target={target}",
        control.current, app.scroll_y.current, control.velocity, app.scroll_y.velocity
    );
    assert!((app.scroll_y.current - control.current).abs() <= 1.0);
    assert!((app.scroll_y.velocity - control.velocity).abs() <= 0.01);
    assert!((app.scroll_y.target - target).abs() <= 0.01);
}
