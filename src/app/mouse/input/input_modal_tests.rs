use super::*;

#[test]
fn reviewer_stage2_modal_dismiss_click_on_dimmed_toggle_does_not_get_exception() {
    let (_context, mut app) = crate::render_view::reviewer_stage2_integration::fixture(
        "# doc\n\ntext\n",
        1000.0,
        1.0,
    );
    app.scroll_y.current = 64.0;
    app.scroll_y.target = 144.0;
    app.scroll_y.velocity = 35.0;
    app.is_ide_mode = true;
    app.ide_panel.project_search.help_open = true;
    let renderer = app.renderer.as_mut().unwrap();
    renderer.height = 800.0;
    renderer.draw_status_bar(
        &app.editor,
        Some((
            app.file_path.as_ref().unwrap(),
            crate::platform::TextEncoding::Utf8,
            )),
            crate::app::MarkdownMode::Edit,
            None,
            None,
        &mut app.ui_registry,
        1.0,
        -1.0,
        -1.0,
        0.0,
        None,
        None,
        None,
        false,
    );
    let (x, y, w, h) = app
        .ui_registry
        .rect_for(crate::ui_system::UiId::MarkdownModeToggle)
        .expect("real status-bar toggle");
    let (mx, my) = (x + w * 0.5, y + h * 0.5);
    let _close_button_hovered = renderer.draw_project_search_help_overlay(
        &app.ide_panel,
        &mut app.ui_registry,
        mx,
        my,
        1.0,
    );
    renderer.flush();
    // The native input router consumes this click in the help_open branch,
    // dismisses the modal, and returns without dispatching the toggle.
    assert_eq!(app.ui_registry.find_overlay_at(mx, my), None);
    let preserve = preserve_main_vertical_scroll_for_click(&app, mx, my);
    stop_click_scroll_anims(&mut app, preserve);
    println!(
        "MODAL_DISMISS top={:?} overlay={:?} preserve={preserve} current={} target={} velocity={}",
        app.ui_registry.find_at(mx, my),
        app.ui_registry.find_overlay_at(mx, my),
        app.scroll_y.current,
        app.scroll_y.target,
        app.scroll_y.velocity
    );
    assert!(
        !preserve,
        "a modal-dismiss click is not a mode-toggle action"
    );
    assert_eq!(app.scroll_y.target, app.scroll_y.current);
    assert_eq!(app.scroll_y.velocity, 0.0);
}
#[test]
fn reviewer_stage2_v2_ddl_dismiss_real_pressed_route_does_not_preserve_toggle_motion() {
    let (_context, mut app) = crate::render_view::reviewer_stage2_integration::fixture(
        "# doc\n\ntext\n",
        1000.0,
        1.0,
    );
    app.is_ide_mode = true;
    app.scroll_y.current = 64.0;
    app.scroll_y.target = 144.0;
    app.scroll_y.velocity = 35.0;
    *app.ide_panel.database.ddl_hover.borrow_mut() =
        Some(crate::app::database::DatabaseDdlHoverState {
            connection_id: crate::app::database::DatabaseConnectionId(1),
            database_name: "test".to_string(),
            table_name: "example".to_string(),
            popup: HoverPopup {
                text: "CREATE TABLE example (id integer);".to_string(),
                spans: Vec::new(),
                line_kinds: vec![crate::lsp::HoverLineKindPublic::Code],
                inline_code_ranges: Vec::new(),
                byte_offset: 0,
                anchor_x: 400.0,
                anchor_y: 90.0,
                offset_x: None,
                offset_y: None,
                anim_progress: 1.0,
                scroll: crate::scroll::ScrollState::new(15.0),
                layout_cache: None,
            },
            rect: None,
            max_scroll: 0.0,
            selection_anchor: None,
            selection_cursor: None,
            selecting: false,
        });
    let renderer = app.renderer.as_mut().unwrap();
    renderer.height = 800.0;
    renderer.draw_status_bar(
        &app.editor,
        Some((
            app.file_path.as_ref().unwrap(),
            crate::platform::TextEncoding::Utf8,
        )),
        crate::app::MarkdownMode::Edit,
        None,
        None,
        &mut app.ui_registry,
        1.0,
        -1.0,
        -1.0,
        0.0,
        None,
        None,
        None,
        false,
    );
    let (x, y, w, h) = app
        .ui_registry
        .rect_for(crate::ui_system::UiId::MarkdownModeToggle)
        .unwrap();
    let (mx, my) = (x + w * 0.5, y + h * 0.5);
    assert!(renderer.draw_database_overlays(
        1.0,
        &app.ide_panel,
        &app.editor,
        &mut app.ui_registry,
        mx,
        my,
        1.0
    ));
    renderer.flush();
    let popup_rect = app
        .ide_panel
        .database
        .ddl_hover
        .borrow()
        .as_ref()
        .unwrap()
        .rect
        .unwrap();
    assert!(!crate::ui_system::point_in_rect(mx, my, popup_rect));
    assert_eq!(
        app.ui_registry.find_at(mx, my),
        Some(crate::ui_system::UiId::MarkdownModeToggle)
    );
    renderer.last_mouse_x = mx;
    renderer.last_mouse_y = my;
    let preserve = preserve_main_vertical_scroll_for_click(&app, mx, my);
    // The complete production input body and actual popup dismissal run.
    app.handle_main_mouse_input_inner(
        &HostLoop::headless(&crate::app::events::host_loop::HeadlessLoopState::default()),
        ElementState::Pressed,
        winit::event::MouseButton::Left,
    );
    assert!(app.ide_panel.database.ddl_hover.borrow().is_none());
    assert_eq!(app.markdown.mode, crate::app::MarkdownMode::Edit);
    assert!(app.markdown.scroll_transition.is_none());
    println!(
        "DDL_REAL_PRESS preserve={preserve} popup={popup_rect:?} mode={:?} current={} target={} velocity={}",
        app.markdown.mode, app.scroll_y.current, app.scroll_y.target, app.scroll_y.velocity
    );
    assert_eq!(
        app.scroll_y.target, 64.0,
        "the consumed popup-dismiss click never toggled mode and must use ordinary click-stop"
    );
    assert_eq!(app.scroll_y.velocity, 0.0);
}

#[cfg(target_os = "linux")]
#[test]
fn reviewer_stage2_v3_actual_toggle_pressed_route_preserves_both_directions() {
    let (_context, mut app) = crate::render_view::reviewer_stage2_integration::fixture(
        "# doc\n\ntext\n",
        1000.0,
        1.0,
    );
    app.scroll_y.current = 64.25;
    app.scroll_y.target = 144.25;
    app.scroll_y.velocity = 35.0;
    app.scroll_y.anim_speed = 7.0;
    let renderer = app.renderer.as_mut().unwrap();
    renderer.height = 800.0;
    renderer.draw_status_bar(
        &app.editor,
        Some((
            app.file_path.as_ref().unwrap(),
            crate::platform::TextEncoding::Utf8,
        )),
        crate::app::MarkdownMode::Edit,
        None,
        None,
        &mut app.ui_registry,
        1.0,
        -1.0,
        -1.0,
        0.0,
        None,
        None,
        None,
        false,
    );
    let (x, y, w, h) = app
        .ui_registry
        .rect_for(crate::ui_system::UiId::MarkdownModeToggle)
        .unwrap();
    renderer.last_mouse_x = x + w * 0.5;
    renderer.last_mouse_y = y + h * 0.5;
    renderer.flush();
    app.handle_main_mouse_input_inner(
        &HostLoop::headless(&crate::app::events::host_loop::HeadlessLoopState::default()),
        ElementState::Pressed,
        winit::event::MouseButton::Left,
    );
    assert_eq!(app.markdown.mode, crate::app::MarkdownMode::Read);
    assert!(app.markdown.scroll_transition.is_some());
    assert_eq!(app.scroll_y.current, 64.25);
    assert_eq!(app.scroll_y.target, 144.25);
    assert_eq!(app.scroll_y.velocity, 35.0);
    app.handle_main_mouse_input_inner(
        &HostLoop::headless(&crate::app::events::host_loop::HeadlessLoopState::default()),
        ElementState::Pressed,
        winit::event::MouseButton::Left,
    );
    assert_eq!(app.markdown.mode, crate::app::MarkdownMode::Edit);
    assert!(app.markdown.scroll_transition.is_none());
    assert_eq!(app.scroll_y.current, 64.25);
    assert_eq!(app.scroll_y.target, 144.25);
    assert_eq!(app.scroll_y.velocity, 35.0);
    assert_eq!(app.scroll_y.anim_speed, 7.0);
    println!("ACTUAL_TOGGLE_PRESSED_ROUTES current=64.25 target=144.25 velocity=35 speed=7");
}
