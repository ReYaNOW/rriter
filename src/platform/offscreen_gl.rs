//! Linux surfaceless EGL context backed by a pbuffer.
#![cfg(target_os = "linux")]

use std::ffi::{CStr, CString, c_char, c_void};
use std::ptr;

type Object = *mut c_void;
type GetProc = unsafe extern "C" fn(*const c_char) -> *const c_void;
type MakeCurrent = unsafe extern "C" fn(Object, Object, Object, Object) -> u32;
type Destroy = unsafe extern "C" fn(Object, Object) -> u32;
type Terminate = unsafe extern "C" fn(Object) -> u32;

struct Library(Object);

impl Drop for Library {
    fn drop(&mut self) {
        unsafe { libc::dlclose(self.0); }
    }
}

unsafe fn symbol<T: Copy>(library: &Library, name: &CStr) -> Result<T, String> {
    let ptr = unsafe { libc::dlsym(library.0, name.as_ptr()) };
    if ptr.is_null() {
        return Err(format!("dlopen: missing {}", name.to_string_lossy()));
    }
    Ok(unsafe { std::mem::transmute_copy(&ptr) })
}

#[derive(Debug)]
pub struct GlStrings {
    pub renderer: String,
    pub version: String,
    pub vendor: String,
}

pub struct OffscreenContext {
    _library: Library,
    display: Object,
    config: Object,
    surface: Object,
    context: Object,
    width: u32,
    height: u32,
    strings: GlStrings,
    get_proc: GetProc,
    get_error: unsafe extern "C" fn() -> u32,
    create_surface: unsafe extern "C" fn(Object, Object, *const i32) -> Object,
    make_current: MakeCurrent,
    destroy_surface: Destroy,
    destroy_context: Destroy,
    terminate: Terminate,
}

impl OffscreenContext {
    pub fn new(width: u32, height: u32) -> Result<Self, String> {
        unsafe {
            let raw = libc::dlopen(c"libEGL.so.1".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
            if raw.is_null() {
                let detail = libc::dlerror();
                let message = if detail.is_null() { "libEGL.so.1 unavailable".into() }
                    else { CStr::from_ptr(detail).to_string_lossy().into_owned() };
                return Err(format!("dlopen: {message}"));
            }
            let library = Library(raw);
            macro_rules! load {
                ($name:literal, $ty:ty) => {
                    symbol::<$ty>(&library, CString::new($name).map_err(|_| format!("dlopen: invalid symbol {}", $name))?.as_c_str())?
                };
            }
            let get_error = load!("eglGetError", unsafe extern "C" fn() -> u32);
            let query_string = load!("eglQueryString", unsafe extern "C" fn(Object, i32) -> *const c_char);
            let get_display = load!("eglGetPlatformDisplay", unsafe extern "C" fn(u32, Object, *const isize) -> Object);
            let initialize = load!("eglInitialize", unsafe extern "C" fn(Object, *mut i32, *mut i32) -> u32);
            let bind_api = load!("eglBindAPI", unsafe extern "C" fn(u32) -> u32);
            let choose_config = load!("eglChooseConfig", unsafe extern "C" fn(Object, *const i32, *mut Object, i32, *mut i32) -> u32);
            let create_surface = load!("eglCreatePbufferSurface", unsafe extern "C" fn(Object, Object, *const i32) -> Object);
            let create_context = load!("eglCreateContext", unsafe extern "C" fn(Object, Object, Object, *const i32) -> Object);
            let make_current = load!("eglMakeCurrent", MakeCurrent);
            let get_proc = load!("eglGetProcAddress", GetProc);
            let destroy_surface = load!("eglDestroySurface", Destroy);
            let destroy_context = load!("eglDestroyContext", Destroy);
            let terminate = load!("eglTerminate", Terminate);
            let extensions_ptr = query_string(ptr::null_mut(), 0x3055);
            let extensions = if extensions_ptr.is_null() { String::new() }
                else { CStr::from_ptr(extensions_ptr).to_string_lossy().into_owned() };
            if !extensions.split_whitespace().any(|ext| ext == "EGL_MESA_platform_surfaceless") {
                return Err(format!("GetPlatformDisplay: EGL_MESA_platform_surfaceless missing; client extensions: {extensions}"));
            }
            let display = get_display(0x31DD, ptr::null_mut(), ptr::null());
            if display.is_null() { return Err(format!("GetPlatformDisplay: eglGetError=0x{:04X}", get_error())); }
            let mut result = Self {
                _library: library, display, config: ptr::null_mut(), surface: ptr::null_mut(),
                context: ptr::null_mut(), width, height,
                strings: GlStrings { renderer: String::new(), version: String::new(), vendor: String::new() },
                get_proc, get_error, create_surface, make_current, destroy_surface, destroy_context, terminate,
            };
            let (mut major, mut minor) = (0, 0);
            if initialize(display, &mut major, &mut minor) != 1 {
                return Err(result.error("Initialize"));
            }
            if bind_api(0x30A2) != 1 { return Err(result.error("ChooseConfig")); }
            let attrs = [0x3033, 1, 0x3040, 8, 0x3024, 8, 0x3023, 8, 0x3022, 8, 0x3038];
            let mut count = 0;
            if choose_config(display, attrs.as_ptr(), &mut result.config, 1, &mut count) != 1 || count != 1 {
                return Err(result.error("ChooseConfig"));
            }
            result.surface = result.create_pbuffer(width, height);
            if result.surface.is_null() { return Err(result.error("CreatePbuffer")); }
            let context_attrs = [0x3098, 3, 0x30FB, 3, 0x30FD, 1, 0x3038];
            result.context = create_context(display, result.config, ptr::null_mut(), context_attrs.as_ptr());
            if result.context.is_null() { return Err(result.error("CreateContext")); }
            if make_current(display, result.surface, result.surface, result.context) != 1 {
                return Err(result.error("MakeCurrent"));
            }
            use glow::HasContext;
            let gl = result.glow();
            result.strings = GlStrings {
                renderer: gl.get_parameter_string(glow::RENDERER),
                version: gl.get_parameter_string(glow::VERSION),
                vendor: gl.get_parameter_string(glow::VENDOR),
            };
            Ok(result)
        }
    }

    fn error(&self, stage: &str) -> String {
        format!("{stage}: eglGetError=0x{:04X}", unsafe { (self.get_error)() })
    }

    fn create_pbuffer(&self, width: u32, height: u32) -> Object {
        let attrs = [0x3057, width as i32, 0x3056, height as i32, 0x3038];
        unsafe { (self.create_surface)(self.display, self.config, attrs.as_ptr()) }
    }

    pub fn glow(&self) -> glow::Context {
        unsafe {
            glow::Context::from_loader_function(|name| {
                let Ok(name) = CString::new(name) else { return ptr::null(); };
                (self.get_proc)(name.as_ptr())
            })
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), String> {
        let next = self.create_pbuffer(width, height);
        if next.is_null() { return Err(self.error("CreatePbuffer")); }
        if unsafe { (self.make_current)(self.display, next, next, self.context) } != 1 {
            let error = self.error("MakeCurrent");
            unsafe { (self.destroy_surface)(self.display, next); }
            return Err(error);
        }
        unsafe { (self.destroy_surface)(self.display, self.surface); }
        self.surface = next;
        self.width = width;
        self.height = height;
        Ok(())
    }

    pub fn requested_context(&self) -> String { "EGL surfaceless OpenGL 3.3 Core".into() }
    pub fn gl_strings(&self) -> &GlStrings { &self.strings }
    pub fn size(&self) -> (u32, u32) { (self.width, self.height) }
}

impl Drop for OffscreenContext {
    fn drop(&mut self) {
        unsafe {
            (self.make_current)(self.display, ptr::null_mut(), ptr::null_mut(), ptr::null_mut());
            if !self.context.is_null() { (self.destroy_context)(self.display, self.context); }
            if !self.surface.is_null() { (self.destroy_surface)(self.display, self.surface); }
            (self.terminate)(self.display);
        }
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use std::sync::Arc;
    use winit::dpi::PhysicalSize;

    pub(crate) fn offscreen_test_app(width: u32, height: u32, scale: f32) -> (OffscreenContext, crate::app::App) {
        let ctx = OffscreenContext::new(width, height).expect("offscreen EGL context");
        let mut app = crate::app::reviewer_stage2_test_app().expect("headless App");
        let mut renderer = crate::renderer::Renderer::new(ctx.glow(), scale, app.theme.clone(), ctx.requested_context()).expect("production Renderer");
        renderer.resize(width, height);
        app.renderer = Some(renderer);
        app.window = Some(Arc::new(crate::platform::WindowHost::Headless(
            crate::platform::HeadlessWindow::new(PhysicalSize::new(width, height), scale as f64),
        )));
        (ctx, app)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glow::HasContext;

    #[test]
    fn offscreen_gl_creates_context_and_reports_gl_strings() {
        let ctx = OffscreenContext::new(64, 48).expect("EGL context");
        assert!(!ctx.gl_strings().version.is_empty());
        assert!(!ctx.gl_strings().renderer.is_empty());
        assert!(!ctx.gl_strings().vendor.is_empty());
    }

    #[test]
    fn offscreen_gl_resize_keeps_context() {
        let mut ctx = OffscreenContext::new(64, 48).expect("EGL context");
        ctx.resize(128, 96).expect("resize pbuffer");
        assert_eq!(ctx.size(), (128, 96));
        assert!(!unsafe { ctx.glow().get_parameter_string(glow::VERSION) }.is_empty());
        assert!(ctx.resize(u32::MAX, 96).is_err());
        assert_eq!(ctx.size(), (128, 96));
        assert!(!unsafe { ctx.glow().get_parameter_string(glow::VERSION) }.is_empty());
    }
}
