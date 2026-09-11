// Independent stage-2 integration regressions. Linux-only test harness uses
// a surfaceless EGL pbuffer: real Renderer/layout, no window or screenshot.
// No production geometry, mapping, transition, or input logic is reimplemented.
#[cfg(all(test, target_os = "linux"))]
pub(crate) mod reviewer_stage2_integration {
    use super::*;
    use crate::app::{App, MarkdownMode};
    use crate::render_view::markdown_read::MarkdownSourceAnchor;
    use std::ffi::{CString, c_char, c_void};

    type EglObject = *mut c_void;
    type GetProc = unsafe extern "C" fn(*const c_char) -> *const c_void;
    type MakeCurrent = unsafe extern "C" fn(EglObject, EglObject, EglObject, EglObject) -> u32;
    type Destroy = unsafe extern "C" fn(EglObject, EglObject) -> u32;
    type Terminate = unsafe extern "C" fn(EglObject) -> u32;

    pub(crate) struct OffscreenContext {
        library: EglObject,
        display: EglObject,
        surface: EglObject,
        context: EglObject,
        get_proc: GetProc,
        make_current: MakeCurrent,
        destroy_surface: Destroy,
        destroy_context: Destroy,
        terminate: Terminate,
    }

    impl OffscreenContext {
        fn new() -> Self {
            unsafe {
                let library =
                    libc::dlopen(c"libEGL.so.1".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
                assert!(
                    !library.is_null(),
                    "review integration tests require libEGL.so.1"
                );
                macro_rules! symbol {
                    ($name:literal, $ty:ty) => {{
                        let name = CString::new($name).unwrap();
                        let ptr = libc::dlsym(library, name.as_ptr());
                        assert!(!ptr.is_null(), "missing EGL entry {}", $name);
                        std::mem::transmute::<*mut c_void, $ty>(ptr)
                    }};
                }
                let get_display = symbol!(
                    "eglGetPlatformDisplay",
                    unsafe extern "C" fn(u32, EglObject, *const isize) -> EglObject
                );
                let initialize = symbol!(
                    "eglInitialize",
                    unsafe extern "C" fn(EglObject, *mut i32, *mut i32) -> u32
                );
                let bind_api = symbol!("eglBindAPI", unsafe extern "C" fn(u32) -> u32);
                let choose_config = symbol!(
                    "eglChooseConfig",
                    unsafe extern "C" fn(
                        EglObject,
                        *const i32,
                        *mut EglObject,
                        i32,
                        *mut i32,
                    ) -> u32
                );
                let create_surface = symbol!(
                    "eglCreatePbufferSurface",
                    unsafe extern "C" fn(EglObject, EglObject, *const i32) -> EglObject
                );
                let create_context = symbol!(
                    "eglCreateContext",
                    unsafe extern "C" fn(EglObject, EglObject, EglObject, *const i32) -> EglObject
                );
                let make_current = symbol!("eglMakeCurrent", MakeCurrent);
                let get_proc = symbol!("eglGetProcAddress", GetProc);
                let display = get_display(0x31DD, std::ptr::null_mut(), std::ptr::null());
                assert!(!display.is_null(), "EGL_MESA_platform_surfaceless display");
                let (mut major, mut minor) = (0, 0);
                assert_eq!(
                    initialize(display, &mut major, &mut minor),
                    1,
                    "EGL initialization"
                );
                assert_eq!(bind_api(0x30A2), 1, "EGL_OPENGL_API");
                let attrs = [
                    0x3033, 1, 0x3040, 8, 0x3024, 8, 0x3023, 8, 0x3022, 8, 0x3038,
                ];
                let mut config = std::ptr::null_mut();
                let mut count = 0;
                assert_eq!(
                    choose_config(display, attrs.as_ptr(), &mut config, 1, &mut count),
                    1
                );
                assert_eq!(count, 1, "OpenGL pbuffer config");
                let surface_attrs = [0x3057, 1000, 0x3056, 800, 0x3038];
                let surface = create_surface(display, config, surface_attrs.as_ptr());
                assert!(!surface.is_null(), "offscreen pbuffer");
                let context_attrs = [0x3098, 3, 0x30FB, 3, 0x30FD, 1, 0x3038];
                let context = create_context(
                    display,
                    config,
                    std::ptr::null_mut(),
                    context_attrs.as_ptr(),
                );
                assert!(!context.is_null(), "OpenGL 3.3 context");
                assert_eq!(make_current(display, surface, surface, context), 1);
                Self {
                    library,
                    display,
                    surface,
                    context,
                    get_proc,
                    make_current,
                    destroy_surface: symbol!("eglDestroySurface", Destroy),
                    destroy_context: symbol!("eglDestroyContext", Destroy),
                    terminate: symbol!("eglTerminate", Terminate),
                }
            }
        }

        fn glow(&self) -> glow::Context {
            unsafe {
                glow::Context::from_loader_function(|name| {
                    let name = CString::new(name).unwrap();
                    (self.get_proc)(name.as_ptr())
                })
            }
        }
    }

    impl Drop for OffscreenContext {
        fn drop(&mut self) {
            unsafe {
                (self.make_current)(
                    self.display,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                );
                (self.destroy_context)(self.display, self.context);
                (self.destroy_surface)(self.display, self.surface);
                (self.terminate)(self.display);
                libc::dlclose(self.library);
            }
        }
    }

    pub(crate) fn fixture(source: &str, width: f32, scale: f32) -> (OffscreenContext, App) {
        let context = OffscreenContext::new();
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
            "review surfaceless".to_string(),
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

    fn edit_transition(app: &mut App) -> bool {
        let renderer = app.renderer.as_mut().unwrap();
        renderer.resolve_markdown_edit_scroll_transition(
            &mut app.markdown,
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
            app.tab_scroll.current,
            &[],
            None,
            &[],
            &[],
            &app.ide_workspaces,
            false,
            None,
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
        app.search_results = vec![(result, result + 12)];
        app.search_current_idx = Some(0);
        app.jump_to_search_result();
        let destination_target = app.scroll_y.target;
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

    include!("markdown_scroll_transition_review_v6_tests.rs");
    include!("markdown_scroll_transition_review_v7_tests.rs");
    include!("markdown_scroll_transition_review_v9_tests.rs");
    include!("markdown_scroll_transition_stage3_tests.rs");
}
