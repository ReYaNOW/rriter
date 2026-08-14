use crate::app::App;
use crate::renderer::Renderer;
use glutin::config::{Config, ConfigTemplateBuilder, GlConfig};
use glutin::context::{
    ContextApi, ContextAttributesBuilder, GlProfile, NotCurrentContext, NotCurrentGlContext,
    PossiblyCurrentContext, Version,
};
use glutin::display::{GetGlDisplay, GlDisplay};
use glutin::surface::{GlSurface, Surface, WindowSurface};
use glutin_winit::DisplayBuilder;
use std::cmp::Reverse;
use std::num::NonZeroU32;
use std::sync::Arc;
use winit::event_loop::ActiveEventLoop;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::{Window, WindowAttributes};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GlContextPlan {
    Desktop { major: u8, minor: u8 },
    Gles { major: u8, minor: u8 },
}

impl GlContextPlan {
    fn label(self) -> String {
        match self {
            Self::Desktop { major, minor } => format!("OpenGL {major}.{minor} Core"),
            Self::Gles { major, minor } => format!("OpenGL ES {major}.{minor}"),
        }
    }

    fn attributes(self, raw_window_handle: RawWindowHandle) -> glutin::context::ContextAttributes {
        let version = match self {
            Self::Desktop { major, minor } | Self::Gles { major, minor } => {
                Version::new(major, minor)
            }
        };
        let builder = match self {
            Self::Desktop { .. } => ContextAttributesBuilder::new()
                .with_profile(GlProfile::Core)
                .with_context_api(ContextApi::OpenGl(Some(version))),
            Self::Gles { .. } => {
                ContextAttributesBuilder::new().with_context_api(ContextApi::Gles(Some(version)))
            }
        };
        builder.build(Some(raw_window_handle))
    }
}

pub(super) fn gl_context_plans(
    platform: crate::platform::PlatformKind,
) -> &'static [GlContextPlan] {
    const MACOS: &[GlContextPlan] = &[GlContextPlan::Desktop { major: 4, minor: 1 }];
    const WINDOWS: &[GlContextPlan] = &[
        GlContextPlan::Desktop { major: 4, minor: 1 },
        GlContextPlan::Desktop { major: 3, minor: 3 },
    ];
    const LINUX: &[GlContextPlan] = &[
        GlContextPlan::Desktop { major: 4, minor: 1 },
        GlContextPlan::Desktop { major: 3, minor: 3 },
        GlContextPlan::Gles { major: 3, minor: 0 },
    ];
    match platform {
        crate::platform::PlatformKind::Macos => MACOS,
        crate::platform::PlatformKind::Windows => WINDOWS,
        crate::platform::PlatformKind::Linux | crate::platform::PlatformKind::Other => LINUX,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DroppedPathKind {
    File,
    Directory,
}

pub(super) fn dropped_path_kind(path: &std::path::Path) -> Option<DroppedPathKind> {
    let metadata = std::fs::metadata(path).ok()?;
    if metadata.is_file() {
        Some(DroppedPathKind::File)
    } else if metadata.is_dir() {
        Some(DroppedPathKind::Directory)
    } else {
        None
    }
}

struct BootstrappedWindow {
    window: Arc<Window>,
    config: Config,
    context: PossiblyCurrentContext,
    surface: Surface<WindowSurface>,
    renderer: Renderer,
}

fn framebuffer_config_rank(hardware_accelerated: bool, num_samples: u8) -> (bool, Reverse<u8>) {
    (hardware_accelerated, Reverse(num_samples))
}

fn window_attributes(app: &App) -> WindowAttributes {
    let icon_bytes = include_bytes!("../../icons/icon.png");
    let window_icon = image::load_from_memory(icon_bytes)
        .ok()
        .map(|image| image.into_rgba8())
        .and_then(|image| {
            let (width, height) = image.dimensions();
            winit::window::Icon::from_rgba(image.into_raw(), width, height).ok()
        });
    crate::platform::apply_window_attributes(
        Window::default_attributes()
            .with_title(format!("{} — RRiter", app.base_title))
            .with_inner_size(winit::dpi::LogicalSize::new(
                app.window_width,
                app.window_height,
            ))
            .with_window_icon(window_icon)
            .with_transparent(false),
    )
}

fn create_not_current_context(
    gl_config: &Config,
    raw_window_handle: RawWindowHandle,
) -> Result<(NotCurrentContext, String), String> {
    let display = gl_config.display();
    let mut errors = Vec::new();
    for plan in gl_context_plans(crate::platform::CURRENT_PLATFORM) {
        let attributes = plan.attributes(raw_window_handle);
        match unsafe { display.create_context(gl_config, &attributes) } {
            Ok(context) => return Ok((context, plan.label())),
            Err(error) => errors.push(format!("{}: {error}", plan.label())),
        }
    }
    Err(format!(
        "RRiter failed to create a supported graphics context:\n{}",
        errors.join("\n")
    ))
}

fn create_surface_and_context(
    gl_config: &Config,
    window: &Window,
    not_current_context: NotCurrentContext,
) -> Result<(Surface<WindowSurface>, PossiblyCurrentContext), String> {
    let raw_window_handle = window
        .window_handle()
        .map_err(|error| format!("window handle is unavailable: {error}"))?
        .as_raw();
    let size = window.inner_size();
    let attributes = glutin::surface::SurfaceAttributesBuilder::<WindowSurface>::new().build(
        raw_window_handle,
        NonZeroU32::new(size.width.max(1)).unwrap_or(NonZeroU32::MIN),
        NonZeroU32::new(size.height.max(1)).unwrap_or(NonZeroU32::MIN),
    );
    let display = gl_config.display();
    let surface = unsafe { display.create_window_surface(gl_config, &attributes) }
        .map_err(|error| format!("window surface creation failed: {error}"))?;
    let context = not_current_context
        .make_current(&surface)
        .map_err(|error| format!("making OpenGL context current failed: {error}"))?;
    let _ = surface.set_swap_interval(
        &context,
        glutin::surface::SwapInterval::Wait(NonZeroU32::MIN),
    );
    Ok((surface, context))
}

fn create_glow_context(gl_config: &Config) -> glow::Context {
    let display = gl_config.display();
    unsafe {
        glow::Context::from_loader_function(|symbol| {
            std::ffi::CString::new(symbol)
                .map(|symbol| display.get_proc_address(symbol.as_c_str()) as *const _)
                .unwrap_or(std::ptr::null())
        })
    }
}

fn bootstrap(app: &App, event_loop: &ActiveEventLoop) -> Result<BootstrappedWindow, String> {
    let template = ConfigTemplateBuilder::new()
        .with_transparency(false)
        .with_depth_size(0)
        .with_stencil_size(0);
    let display_builder =
        DisplayBuilder::new().with_window_attributes(Some(window_attributes(app)));
    let (window, gl_config) = display_builder
        .build(event_loop, template, |configs| {
            configs
                .max_by_key(|config| {
                    framebuffer_config_rank(config.hardware_accelerated(), config.num_samples())
                })
                .unwrap_or_else(|| panic!("no OpenGL framebuffer configuration is available"))
        })
        .map_err(|error| format!("window/display creation failed: {error}"))?;
    let window = window.ok_or_else(|| "window backend did not create a window".to_string())?;
    window.set_ime_allowed(true);
    let raw_window_handle = window
        .window_handle()
        .map_err(|error| format!("window handle is unavailable: {error}"))?
        .as_raw();
    let (not_current_context, requested_context) =
        create_not_current_context(&gl_config, raw_window_handle)?;
    let (surface, context) = create_surface_and_context(&gl_config, &window, not_current_context)?;
    let renderer = Renderer::new(
        create_glow_context(&gl_config),
        window.scale_factor() as f32,
        app.theme.clone(),
        requested_context,
    )
    .map_err(|error| format!("RRiter renderer initialization failed: {error}"))?;
    Ok(BootstrappedWindow {
        window: Arc::new(window),
        config: gl_config,
        context,
        surface,
        renderer,
    })
}

#[cfg(target_os = "linux")]
fn trim_allocator_after_gl_bootstrap() {
    unsafe extern "C" {
        fn malloc_trim(pad: usize) -> i32;
    }
    unsafe {
        malloc_trim(0);
    }
}

#[cfg(not(target_os = "linux"))]
fn trim_allocator_after_gl_bootstrap() {}

#[cfg_attr(coverage_nightly, coverage(off))]
pub(super) fn resume(app: &mut App, event_loop: &ActiveEventLoop) {
    if app.window.is_some() {
        return;
    }
    let runtime = match bootstrap(app, event_loop) {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("{error}");
            event_loop.exit();
            return;
        }
    };
    eprintln!(
        "RRiter graphics:\n{}",
        runtime.renderer.graphics_diagnostics.report()
    );
    app.renderer = Some(runtime.renderer);
    app.gl_config = Some(runtime.config);
    app.window = Some(runtime.window);
    app.gl_context = Some(runtime.context);
    app.gl_surface = Some(runtime.surface);
    if let Some(window) = app.window.as_ref() {
        App::update_window_title(window, &app.base_title, app.editor.is_dirty());
    }
    trim_allocator_after_gl_bootstrap();
}

#[cfg_attr(coverage_nightly, coverage(off))]
pub(super) fn persist_state_and_shutdown(app: &mut App) {
    if app.is_automation_mode() {
        app.write_interrupted_automation_report("application exited before automation completed");
        app.shutdown_background_services();
        return;
    }
    let (width, height, maximized) = if let Some(window) = app.window.as_ref() {
        let maximized = window.is_maximized();
        let (width, height) = if maximized {
            (app.window_width, app.window_height)
        } else {
            let size = window.inner_size().to_logical::<f64>(window.scale_factor());
            (size.width, size.height)
        };
        (width, height, maximized)
    } else {
        (app.window_width, app.window_height, app.should_maximize)
    };
    crate::save_config(&crate::Config {
        window_width: width,
        window_height: height,
        maximized,
        ide_workspaces: app.ide_workspaces.clone(),
        ide_ignore_patterns: app.ide_ignore_patterns.clone(),
        enable_telemetry: crate::render_view::TELEMETRY_ENABLED
            .load(std::sync::atomic::Ordering::Relaxed),
        tool_paths: app.tool_paths.clone(),
        dart_settings: app.dart_settings.clone(),
    });
    if app.is_ide_mode {
        app.ide_panel.api.persist();
        crate::save_panel_state(&app.ide_panel);
    }
    app.shutdown_background_services();
}

#[cfg_attr(coverage_nightly, coverage(off))]
pub(super) fn save_state_and_exit(app: &mut App, event_loop: &ActiveEventLoop) {
    persist_state_and_shutdown(app);
    event_loop.exit();
}

#[cfg(test)]
mod tests {
    use super::{GlContextPlan, framebuffer_config_rank, gl_context_plans};

    #[test]
    fn framebuffer_config_rank_prefers_hardware_then_minimum_samples() {
        assert!(framebuffer_config_rank(true, 0) > framebuffer_config_rank(true, 8));
        assert!(framebuffer_config_rank(true, 8) > framebuffer_config_rank(false, 0));
        assert!(framebuffer_config_rank(false, 0) > framebuffer_config_rank(false, 8));
    }

    #[test]
    fn graphics_context_plan_is_platform_specific() {
        assert_eq!(
            gl_context_plans(crate::platform::PlatformKind::Macos),
            &[GlContextPlan::Desktop { major: 4, minor: 1 }]
        );
        assert_eq!(
            gl_context_plans(crate::platform::PlatformKind::Windows),
            &[
                GlContextPlan::Desktop { major: 4, minor: 1 },
                GlContextPlan::Desktop { major: 3, minor: 3 },
            ]
        );
        assert!(
            gl_context_plans(crate::platform::PlatformKind::Linux)
                .iter()
                .any(|plan| matches!(plan, GlContextPlan::Gles { major: 3, minor: 0 }))
        );
    }

    #[test]
    fn macos_never_falls_back_to_gles_or_legacy_opengl() {
        let plans = gl_context_plans(crate::platform::PlatformKind::Macos);
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].label(), "OpenGL 4.1 Core");
    }

    #[test]
    fn dropped_path_classifier_accepts_files_and_directories_only() {
        let root = std::env::temp_dir().join(format!(
            "rriter-drop-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("sample.rs");
        std::fs::write(&file, b"fn main() {}\n").unwrap();
        assert_eq!(
            super::dropped_path_kind(&file),
            Some(super::DroppedPathKind::File)
        );
        assert_eq!(
            super::dropped_path_kind(&root),
            Some(super::DroppedPathKind::Directory)
        );
        assert_eq!(super::dropped_path_kind(&root.join("missing")), None);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn windows_fallback_stays_within_supported_desktop_gl() {
        let labels = gl_context_plans(crate::platform::PlatformKind::Windows)
            .iter()
            .copied()
            .map(GlContextPlan::label)
            .collect::<Vec<_>>();
        assert_eq!(labels, ["OpenGL 4.1 Core", "OpenGL 3.3 Core"]);
    }
}
