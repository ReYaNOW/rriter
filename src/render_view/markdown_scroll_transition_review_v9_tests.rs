// Reviewer acceptance extras after v8. These exercise repeated geometry
// ownership changes plus stop/relative/inverse handling using the existing
// offscreen production Renderer/input/physics helpers from the parent module.

#[test]
fn reviewer_stage2_v9_edit_reprojects_across_two_dpi_changes_before_first_frame() {
    let source = document();
    let (_context, mut app) = fixture(&source, 720.0, 1.0);
    app.renderer.as_mut().unwrap().height = 420.0;

    review_v8_applied_edit_anchor_before_new_geometry(&mut app, &source);
    app.renderer.as_mut().unwrap().update_scale_factor(1.25);
    review_v4_search(&mut app, &source, "paragraph038");
    assert!(app.scroll_y.update(1.0 / 120.0));
    assert_eq!(app.scroll_y.deferred_current_rebase_applied(), Some(true));

    let (owned, inset) = app.markdown.deferred_edit_current_geometry().unwrap();
    let live_anchor = app
        .renderer
        .as_mut()
        .unwrap()
        .markdown_edit_viewport_anchor_with_line_height(
            &app.editor,
            app.scroll_y.current + inset,
            owned.line_height,
        )
        .unwrap();

    app.renderer.as_mut().unwrap().update_scale_factor(1.5);
    let expected_before_tick = review_v8_expected_edit_current(&mut app, &live_anchor);
    review_v4_search(&mut app, &source, "paragraph042");
    assert!((app.scroll_y.current - expected_before_tick).abs() <= 1.0);
    let target = app.scroll_y.target;

    let mut control = app.scroll_y.clone();
    control.current = expected_before_tick;
    assert!(control.update(1.0 / 120.0));
    assert!(app.scroll_y.update(1.0 / 120.0));
    println!(
        "V9_EDIT_TWO_DPI expected_current={} actual_current={} expected_velocity={} actual_velocity={} target={target}",
        control.current, app.scroll_y.current, control.velocity, app.scroll_y.velocity
    );
    assert!((app.scroll_y.current - control.current).abs() <= 1.0);
    assert!((app.scroll_y.velocity - control.velocity).abs() <= 0.01);
    assert!((app.scroll_y.target - target).abs() <= 0.01);
}

#[test]
fn reviewer_stage2_v9_read_reprojects_across_two_width_changes_before_first_frame() {
    let source = document();
    let (_context, mut app) = fixture(&source, 720.0, 1.0);
    app.renderer.as_mut().unwrap().height = 420.0;

    review_v7_applied_read_anchor_before_new_geometry(&mut app, &source);
    app.renderer.as_mut().unwrap().width = 600.0;
    review_v4_search(&mut app, &source, "paragraph038");
    assert!(app.scroll_y.update(1.0 / 120.0));
    assert_eq!(app.scroll_y.deferred_current_rebase_applied(), Some(true));
    let live_anchor = app
        .markdown
        .read_layout
        .viewport_source_anchor(app.scroll_y.current)
        .unwrap();

    app.renderer.as_mut().unwrap().width = 450.0;
    review_v4_search(&mut app, &source, "paragraph042");
    let expected_before_tick = review_v7_expected_read_current(&mut app, &live_anchor);
    assert!((app.scroll_y.current - expected_before_tick).abs() <= 1.0);
    let target = app.scroll_y.target;

    let mut control = app.scroll_y.clone();
    control.current = expected_before_tick;
    assert!(control.update(1.0 / 120.0));
    assert!(app.scroll_y.update(1.0 / 120.0));
    println!(
        "V9_READ_TWO_WIDTH expected_current={} actual_current={} expected_velocity={} actual_velocity={} target={target}",
        control.current, app.scroll_y.current, control.velocity, app.scroll_y.velocity
    );
    assert!((app.scroll_y.current - control.current).abs() <= 1.0);
    assert!((app.scroll_y.velocity - control.velocity).abs() <= 0.01);
    assert!((app.scroll_y.target - target).abs() <= 0.01);
}

#[test]
fn reviewer_stage2_v9_stop_after_edit_reproject_and_second_tick_stays_settled() {
    let source = document();
    let (_context, mut app) = fixture(&source, 720.0, 1.0);
    app.renderer.as_mut().unwrap().height = 420.0;

    review_v8_applied_edit_anchor_before_new_geometry(&mut app, &source);
    app.renderer.as_mut().unwrap().update_scale_factor(1.25);
    review_v4_search(&mut app, &source, "paragraph038");
    assert!(app.scroll_y.update(1.0 / 120.0));
    crate::app::mouse::stop_click_scroll_anims(&mut app, false);
    let stopped = app.scroll_y.current;
    assert_eq!(app.scroll_y.target, stopped);
    assert_eq!(app.scroll_y.velocity, 0.0);

    review_v3_root_frame(&mut app);
    assert!(!app.scroll_y.update(1.0 / 120.0));
    println!(
        "V9_EDIT_SECOND_TICK_STOP stopped={stopped} current={} target={} velocity={}",
        app.scroll_y.current, app.scroll_y.target, app.scroll_y.velocity
    );
    assert!((app.scroll_y.current - stopped).abs() <= 1.0);
    assert_eq!(app.scroll_y.target, app.scroll_y.current);
    assert_eq!(app.scroll_y.velocity, 0.0);
}

#[test]
fn reviewer_stage2_v9_edit_search_wheel_tick_then_inverse_preserves_live_source_and_residual() {
    let source = document();
    let (_context, mut app) = fixture(&source, 720.0, 1.0);
    app.renderer.as_mut().unwrap().height = 420.0;

    review_v8_applied_edit_anchor_before_new_geometry(&mut app, &source);
    app.renderer.as_mut().unwrap().update_scale_factor(1.25);
    review_v4_search(&mut app, &source, "paragraph038");
    review_v6_point_at_body(&mut app, crate::ui_system::UiId::MarkdownReadBody);
    review_v6_wheel_up(&mut app);
    assert!(app.scroll_y.update(1.0 / 120.0));

    let (owned, inset) = app.markdown.deferred_edit_current_geometry().unwrap();
    let live_anchor = app
        .renderer
        .as_mut()
        .unwrap()
        .markdown_edit_viewport_anchor_with_line_height(
            &app.editor,
            app.scroll_y.current + inset,
            owned.line_height,
        )
        .unwrap();
    let residual_before = app.scroll_y.target - app.scroll_y.current;

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
        "V9_EDIT_WHEEL_TICK_INVERSE expected_read={expected_read} actual_read={} residual_before={residual_before} residual_after={residual_after}",
        app.scroll_y.current
    );
    assert!((app.scroll_y.current - expected_read).abs() <= 1.0);
    assert!((residual_after - residual_before).abs() <= 0.01);
}
