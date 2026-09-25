#[cfg(all(test, target_os = "linux"))]
mod host_cases {
    use crate::app::{App, MarkdownMode};
    use crate::app::events::host_loop::{HeadlessLoopState, HostLoop};
    use crate::platform::offscreen_gl::test_support::offscreen_test_app;
    use crate::ui_system::UiId;
    use winit::dpi::PhysicalPosition;
    use winit::event::{ElementState, MouseButton, MouseScrollDelta};
    use std::collections::HashSet;

    fn document_app() -> (crate::platform::offscreen_gl::OffscreenContext, App) {
        let (context, mut app) = offscreen_test_app(1280, 800, 1.0);
        let mut source = String::new();
        for i in 0..400 {
            source.push_str(&format!("line {i:03} alpha beta gamma delta\n"));
        }
        app.editor = crate::app::reviewer_stage2_editor_with(&source);
        app.editor.cursor = 0;
        app.file_path = Some(std::path::PathBuf::from("/tmp/host-characterization.md"));
        app.file_extension = "md".to_string();
        app.is_ide_mode = true;
        app.tabs.push(super::tab_with("host-characterization.md", Some("/tmp/host-characterization.md"), &source));
        app.show_welcome = false;
        crate::render_view::reviewer_stage2_integration::review_v3_root_frame(&mut app);
        (context, app)
    }

    fn toggle_center(app: &App) -> (f32, f32) {
        let (x, y, w, h) = app.ui_registry.rect_for(UiId::MarkdownModeToggle)
            .expect("rendered Markdown mode toggle");
        (x + w * 0.5, y + h * 0.5)
    }

    #[test]
    fn host_frame_render_main_frame_registers_same_ui() {
        let (_context, mut app) = document_app();
        let manual: HashSet<_> = app.ui_registry.element_ids()
            .map(|id| format!("{id:?}"))
            .collect();
        assert!(!manual.is_empty());
        assert!(manual.contains(&format!("{:?}", UiId::EditorTab(0))));
        assert!(manual.contains(&format!("{:?}", UiId::StatusBar)));

        let outcome = app.render_main_frame();
        let rendered: HashSet<_> = app.ui_registry.element_ids()
            .map(|id| format!("{id:?}"))
            .collect();
        assert!(!rendered.is_empty());
        assert!(manual.is_subset(&rendered), "manual UI ids missing: {:?}", manual.difference(&rendered).collect::<Vec<_>>());
        app.finish_main_frame(outcome);
        let outcome = app.render_main_frame();
        app.finish_main_frame(outcome);
    }

    #[test]
    fn host_characterization_cursor_moved_sets_hover() {
        let (_context, mut app) = document_app();
        let (x, y) = toggle_center(&app);
        app.handle_main_cursor_moved(PhysicalPosition::new(x as f64, y as f64));
        crate::render_view::reviewer_stage2_integration::review_v3_root_frame(&mut app);
        assert_eq!(app.ui_registry.hovered(), Some(UiId::MarkdownModeToggle));
    }

    #[test]
    fn host_characterization_wheel_scrolls_editor() {
        let (_context, mut app) = document_app();
        app.handle_main_cursor_moved(PhysicalPosition::new(640.0, 400.0));
        let before = app.scroll_y.target;
        app.handle_main_mouse_wheel(MouseScrollDelta::LineDelta(0.0, -3.0));
        assert!(app.scroll_y.target > before, "editor scroll target should increase");
    }

    #[test]
    fn host_characterization_click_toggles_markdown_mode() {
        let (_context, mut app) = document_app();
        let (x, y) = toggle_center(&app);
        assert_eq!(app.markdown_mode(), MarkdownMode::Edit);
        app.handle_main_cursor_moved(PhysicalPosition::new(x as f64, y as f64));
        let loop_state = HeadlessLoopState::default();
        let host = HostLoop::headless(&loop_state);
        app.handle_main_mouse_input(&host, ElementState::Pressed, MouseButton::Left);
        app.handle_main_mouse_input(&host, ElementState::Released, MouseButton::Left);
        assert_ne!(app.markdown_mode(), MarkdownMode::Edit);
        assert_eq!(app.markdown_mode(), MarkdownMode::Read);
    }
}
