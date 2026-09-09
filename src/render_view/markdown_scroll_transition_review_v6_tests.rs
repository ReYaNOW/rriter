// Deferred-navigation lifecycle regressions, included inside the existing
// reviewer_stage2_integration module. Reuses its offscreen EGL context, real
// Renderer::draw, mode/search helpers, and accepted source geometry verbatim.
// No alternative physics, UI registry, source locator, or layout builder.

fn review_v6_point_at_body(app: &mut App, id: crate::ui_system::UiId) {
    let (x, y, width, height) = app.ui_registry.rect_for(id).expect("drawn body");
    let (mx, my) = (x + width * 0.5, y + height * 0.5);
    assert_eq!(app.ui_registry.find_at(mx, my), Some(id));
    let renderer = app.renderer.as_mut().unwrap();
    renderer.last_mouse_x = mx;
    renderer.last_mouse_y = my;
}

fn review_v6_wheel_up(app: &mut App) {
    app.handle_main_mouse_wheel(winit::event::MouseScrollDelta::PixelDelta(
        winit::dpi::PhysicalPosition::new(0.0, 36.0),
    ));
}

#[test]
fn reviewer_stage2_v6_click_stop_after_tick_before_edit_frame_keeps_position() {
    let source = document();
    let (_context, mut app) = fixture(&source, 720.0, 1.0);
    app.renderer.as_mut().unwrap().height = 420.0;
    let byte = source.find("paragraph030").unwrap();
    review_v3_root_read_at(&mut app, byte, 3.25);
    app.set_markdown_mode(MarkdownMode::Edit);
    review_v4_search(&mut app, &source, "paragraph034");
    assert!(app.scroll_y.update(1.0 / 120.0));
    assert_eq!(app.scroll_y.deferred_current_rebase_applied(), Some(true));
    let before_stop = app.scroll_y.current;
    crate::app::mouse::stop_click_scroll_anims(&mut app, false);
    let stopped = app.scroll_y.current;
    assert!((stopped - before_stop).abs() <= 0.5);
    assert_eq!(app.scroll_y.target, stopped);
    assert_eq!(app.scroll_y.velocity, 0.0);
    review_v3_root_frame(&mut app);
    let frame_current = app.scroll_y.current;
    let frame_target = app.scroll_y.target;
    app.scroll_y.update(1.0 / 120.0);
    println!(
        "V6_EDIT_STOP_AFTER_TICK before_stop={before_stop} expected={stopped} first_current={frame_current} first_target={frame_target} next_current={} next_velocity={}",
        app.scroll_y.current, app.scroll_y.velocity
    );
    assert!(
        (frame_current - stopped).abs() <= 1.0,
        "ordinary stop must not rebase an already destination-coordinate current a second time"
    );
    assert_eq!(frame_target, frame_current);
    assert_eq!(app.scroll_y.current, frame_current);
    assert_eq!(app.scroll_y.velocity, 0.0);
}

#[test]
fn reviewer_stage2_v6_click_stop_after_tick_before_read_frame_keeps_position() {
    let source = document();
    let (_context, mut app) = fixture(&source, 720.0, 1.0);
    app.renderer.as_mut().unwrap().height = 420.0;
    let byte = source.find("paragraph030").unwrap();
    review_v4_edit_at(&mut app, byte, 3.25);
    app.set_markdown_mode(MarkdownMode::Read);
    review_v4_search(&mut app, &source, "paragraph034");
    assert!(app.scroll_y.update(1.0 / 120.0));
    assert_eq!(app.scroll_y.deferred_current_rebase_applied(), Some(true));
    let before_stop = app.scroll_y.current;
    crate::app::mouse::stop_click_scroll_anims(&mut app, false);
    let stopped = app.scroll_y.current;
    assert!((stopped - before_stop).abs() <= 0.5);
    assert_eq!(app.scroll_y.target, stopped);
    assert_eq!(app.scroll_y.velocity, 0.0);
    review_v3_root_frame(&mut app);
    let frame_current = app.scroll_y.current;
    let frame_target = app.scroll_y.target;
    app.scroll_y.update(1.0 / 120.0);
    println!(
        "V6_READ_STOP_AFTER_TICK before_stop={before_stop} expected={stopped} first_current={frame_current} first_target={frame_target} next_current={} next_velocity={}",
        app.scroll_y.current, app.scroll_y.velocity
    );
    assert!(
        (frame_current - stopped).abs() <= 1.0,
        "stop cancels motion, not the ownership of the current after an applied rebase"
    );
    assert_eq!(frame_target, frame_current);
    assert_eq!(app.scroll_y.current, frame_current);
    assert_eq!(app.scroll_y.velocity, 0.0);
}

#[test]
fn reviewer_stage2_v6_resize_after_consumed_search_rebase_projects_live_source() {
    let source = document();
    let (_context, mut app) = fixture(&source, 720.0, 1.0);
    app.renderer.as_mut().unwrap().height = 420.0;
    let byte = source.find("paragraph030").unwrap();
    review_v4_edit_at(&mut app, byte, 3.25);
    app.set_markdown_mode(MarkdownMode::Read);
    review_v4_search(&mut app, &source, "paragraph034");
    assert!(app.scroll_y.update(1.0 / 120.0));
    assert_eq!(app.scroll_y.deferred_current_rebase_applied(), Some(true));
    // Read was not displayed, but current already belongs to its prepared 720px layout.
    let old_current = app.scroll_y.current;
    let live_anchor = app
        .markdown
        .read_layout
        .viewport_source_anchor(old_current)
        .unwrap();
    let velocity = app.scroll_y.velocity;
    app.renderer.as_mut().unwrap().width = 450.0;
    review_v3_root_frame(&mut app);
    let first_current = app.scroll_y.current;
    let first_target = app.scroll_y.target;
    let first_velocity = app.scroll_y.velocity;
    let expected_current = live_anchor.projected_scroll_y(
        app.markdown
            .read_layout
            .source_anchor_y(&live_anchor.source_range)
            .unwrap(),
    );
    // Independent target oracle: repeat the real search on the now-current layout.
    review_v4_search(&mut app, &source, "paragraph034");
    let expected_target = app.scroll_y.target;
    println!(
        "V6_RESIZE_AFTER_TICK old_current={old_current} live={live_anchor:?} expected_current={expected_current} actual_current={first_current} expected_target={expected_target} actual_target={first_target} expected_velocity={velocity} actual_velocity={first_velocity}"
    );
    assert!(
        (first_current - expected_current).abs() <= 1.0,
        "an applied deferred current must be reprojected from the geometry that owns it, not treated as fresh-width pixels"
    );
    assert!((first_target - expected_target).abs() <= 1.0);
    assert_eq!(first_velocity, velocity);
}

#[test]
fn reviewer_stage2_v6_resize_after_tick_then_inverse_keeps_known_read_geometry() {
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
    let residual = app.scroll_y.target - app.scroll_y.current;
    let velocity = app.scroll_y.velocity;
    app.renderer.as_mut().unwrap().width = 450.0;
    app.set_markdown_mode(MarkdownMode::Edit);
    let captured = app
        .markdown
        .scroll_transition
        .as_ref()
        .and_then(|transition| transition.anchor.clone());
    review_v3_root_frame(&mut app);
    let actual_residual = app.scroll_y.target - app.scroll_y.current;
    println!(
        "V6_RESIZE_TICK_INVERSE read_current={read_current} live={live_anchor:?} captured={captured:?} expected_edit={expected} actual_edit={} expected_residual={residual} actual_residual={actual_residual}",
        app.scroll_y.current
    );
    assert!(
        (app.scroll_y.current - expected).abs() <= 1.0,
        "renderer.width is not the key of the geometry in which the previous tick integrated current"
    );
    assert!((actual_residual - residual).abs() <= 0.01);
    assert_eq!(app.scroll_y.velocity, velocity);
}

#[test]
fn reviewer_stage2_v6_read_search_then_actual_wheel_keeps_new_relative_impulse() {
    let source = document();
    let (_context, mut app) = fixture(&source, 720.0, 1.0);
    app.renderer.as_mut().unwrap().height = 420.0;
    let byte = source.find("paragraph030").unwrap();
    review_v4_edit_at(&mut app, byte, 3.25);
    review_v6_point_at_body(&mut app, crate::ui_system::UiId::EditorTextBody);
    app.set_markdown_mode(MarkdownMode::Read);
    let origin_anchor = app
        .markdown
        .scroll_transition
        .as_ref()
        .unwrap()
        .anchor
        .clone()
        .unwrap();
    review_v4_search(&mut app, &source, "paragraph034");
    let expected_current = origin_anchor.projected_scroll_y(
        app.markdown
            .read_layout
            .source_anchor_y(&origin_anchor.source_range)
            .unwrap(),
    );
    let search_target = app.scroll_y.target;
    review_v6_wheel_up(&mut app);
    let after_wheel = app.scroll_y.target;
    assert!((after_wheel - (search_target - 36.0)).abs() <= 0.01);
    let velocity = app.scroll_y.velocity;
    review_v3_root_frame(&mut app);
    println!(
        "V6_READ_SEARCH_WHEEL search_target={search_target} expected_target={after_wheel} actual_target={} expected_current={expected_current} actual_current={}",
        app.scroll_y.target, app.scroll_y.current
    );
    assert!((app.scroll_y.current - expected_current).abs() <= 1.0);
    assert!(
        (app.scroll_y.target - after_wheel).abs() <= 0.01,
        "source-backed search resolve must not overwrite a newer real wheel impulse"
    );
    assert_eq!(app.scroll_y.velocity, velocity);
}

#[test]
fn reviewer_stage2_v6_edit_search_then_actual_wheel_keeps_new_relative_impulse() {
    let source = document();
    let (_context, mut app) = fixture(&source, 720.0, 1.0);
    app.renderer.as_mut().unwrap().height = 420.0;
    let byte = source.find("paragraph030").unwrap();
    review_v3_root_read_at(&mut app, byte, 3.25);
    review_v6_point_at_body(&mut app, crate::ui_system::UiId::MarkdownReadBody);
    app.set_markdown_mode(MarkdownMode::Edit);
    let origin_anchor = app
        .markdown
        .scroll_transition
        .as_ref()
        .unwrap()
        .anchor
        .clone()
        .unwrap();
    let expected_current = edit_expected(&mut app, &origin_anchor);
    review_v4_search(&mut app, &source, "paragraph034");
    let search_target = app.scroll_y.target;
    review_v6_wheel_up(&mut app);
    let after_wheel = app.scroll_y.target;
    assert!((after_wheel - (search_target - 36.0)).abs() <= 0.01);
    let velocity = app.scroll_y.velocity;
    review_v3_root_frame(&mut app);
    println!(
        "V6_EDIT_SEARCH_WHEEL search_target={search_target} expected_target={after_wheel} actual_target={} expected_current={expected_current} actual_current={}",
        app.scroll_y.target, app.scroll_y.current
    );
    assert!((app.scroll_y.current - expected_current).abs() <= 1.0);
    assert!(
        (app.scroll_y.target - after_wheel).abs() <= 0.01,
        "pending Edit source target must preserve relative input received after the search"
    );
    assert_eq!(app.scroll_y.velocity, velocity);
}

#[test]
fn reviewer_stage2_v6_resolved_read_search_wheel_stays_effective_control() {
    let source = document();
    let (_context, mut app) = fixture(&source, 720.0, 1.0);
    app.renderer.as_mut().unwrap().height = 420.0;
    let byte = source.find("paragraph030").unwrap();
    review_v4_edit_at(&mut app, byte, 3.25);
    app.set_markdown_mode(MarkdownMode::Read);
    review_v4_search(&mut app, &source, "paragraph034");
    review_v3_root_frame(&mut app);
    assert!(app.markdown.scroll_transition.is_none());
    review_v6_point_at_body(&mut app, crate::ui_system::UiId::MarkdownReadBody);
    let current = app.scroll_y.current;
    let before_wheel = app.scroll_y.target;
    review_v6_wheel_up(&mut app);
    let expected = before_wheel - 36.0;
    assert!((app.scroll_y.target - expected).abs() <= 0.01);
    review_v3_root_frame(&mut app);
    println!(
        "V6_RESOLVED_WHEEL_CONTROL expected={expected} actual={} current={current} actual_current={}",
        app.scroll_y.target, app.scroll_y.current
    );
    assert!((app.scroll_y.target - expected).abs() <= 0.01);
    assert_eq!(app.scroll_y.current, current);
}

#[test]
fn reviewer_stage2_v6_later_search_replaces_earlier_wheel_control() {
    let source = document();
    let (_context, mut app) = fixture(&source, 720.0, 1.0);
    app.renderer.as_mut().unwrap().height = 420.0;
    let byte = source.find("paragraph030").unwrap();
    review_v4_edit_at(&mut app, byte, 3.25);
    review_v6_point_at_body(&mut app, crate::ui_system::UiId::EditorTextBody);
    app.set_markdown_mode(MarkdownMode::Read);
    review_v4_search(&mut app, &source, "paragraph034");
    let first_target = app.scroll_y.target;
    review_v6_wheel_up(&mut app);
    assert!((app.scroll_y.target - (first_target - 36.0)).abs() <= 0.01);
    review_v4_search(&mut app, &source, "paragraph038");
    let expected = app.scroll_y.target;
    let mut control = app.scroll_y.clone();
    control.update(1.0 / 120.0);
    app.scroll_y.update(1.0 / 120.0);
    review_v3_root_frame(&mut app);
    println!(
        "V6_LAST_SEARCH_CONTROL expected_target={expected} actual_target={} expected_current={} actual_current={} expected_velocity={} actual_velocity={}",
        app.scroll_y.target,
        control.current,
        app.scroll_y.current,
        control.velocity,
        app.scroll_y.velocity
    );
    assert!((app.scroll_y.target - expected).abs() <= 0.01);
    assert_eq!(app.scroll_y.current, control.current);
    assert_eq!(app.scroll_y.velocity, control.velocity);
}
