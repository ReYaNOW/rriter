use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use winit::dpi::{PhysicalSize, Size};
use winit::raw_window_handle::{HandleError, HasWindowHandle, WindowHandle};
use winit::window::{Cursor, CursorIcon, Window, WindowId};

pub enum WindowHost {
    Native(Arc<Window>),
    Headless(HeadlessWindow),
}

pub struct HeadlessWindow {
    size: Mutex<PhysicalSize<u32>>,
    scale_factor: Mutex<f64>,
    title: Mutex<String>,
    maximized: AtomicBool,
    ime_allowed: AtomicBool,
    cursor_icon: Mutex<CursorIcon>,
    redraw_requested: AtomicBool,
    id: WindowId,
}

impl HeadlessWindow {
    pub fn new(size: PhysicalSize<u32>, scale_factor: f64) -> Self {
        Self {
            size: Mutex::new(size),
            scale_factor: Mutex::new(scale_factor),
            title: Mutex::new(String::new()),
            maximized: AtomicBool::new(false),
            ime_allowed: AtomicBool::new(false),
            cursor_icon: Mutex::new(CursorIcon::Default),
            redraw_requested: AtomicBool::new(false),
            id: WindowId::dummy(),
        }
    }

    pub fn take_redraw_request(&self) -> bool {
        self.redraw_requested.swap(false, Ordering::AcqRel)
    }

    /// Headless `dump`: `take_redraw_request` without consuming.
    pub fn redraw_requested(&self) -> bool {
        self.redraw_requested.load(Ordering::Acquire)
    }

    pub fn set_size(&self, size: PhysicalSize<u32>) {
        *self.size.lock().unwrap_or_else(PoisonError::into_inner) = size;
    }

    pub fn set_scale_factor(&self, scale_factor: f64) {
        *self.scale_factor.lock().unwrap_or_else(PoisonError::into_inner) = scale_factor;
    }

    pub fn title(&self) -> String {
        self.title.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    pub fn cursor_icon(&self) -> CursorIcon {
        *self.cursor_icon.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn ime_allowed(&self) -> bool {
        self.ime_allowed.load(Ordering::Relaxed)
    }
}

impl WindowHost {
    pub fn native(&self) -> Option<&Arc<Window>> {
        match self {
            Self::Native(window) => Some(window),
            Self::Headless(_) => None,
        }
    }

    pub fn headless(&self) -> Option<&HeadlessWindow> {
        match self {
            Self::Native(_) => None,
            Self::Headless(window) => Some(window),
        }
    }

    pub fn request_redraw(&self) {
        match self {
            Self::Native(window) => window.request_redraw(),
            Self::Headless(window) => window.redraw_requested.store(true, Ordering::Release),
        }
    }

    pub fn inner_size(&self) -> PhysicalSize<u32> {
        match self {
            Self::Native(window) => window.inner_size(),
            Self::Headless(window) => *window.size.lock().unwrap_or_else(PoisonError::into_inner),
        }
    }

    pub fn scale_factor(&self) -> f64 {
        match self {
            Self::Native(window) => window.scale_factor(),
            Self::Headless(window) => *window.scale_factor.lock().unwrap_or_else(PoisonError::into_inner),
        }
    }

    pub fn set_title(&self, title: &str) {
        match self {
            Self::Native(window) => window.set_title(title),
            Self::Headless(window) => *window.title.lock().unwrap_or_else(PoisonError::into_inner) = title.to_string(),
        }
    }

    pub fn set_maximized(&self, maximized: bool) {
        match self {
            Self::Native(window) => window.set_maximized(maximized),
            Self::Headless(window) => window.maximized.store(maximized, Ordering::Relaxed),
        }
    }

    pub fn is_maximized(&self) -> bool {
        match self {
            Self::Native(window) => window.is_maximized(),
            Self::Headless(window) => window.maximized.load(Ordering::Relaxed),
        }
    }

    pub fn focus_window(&self) {
        if let Self::Native(window) = self {
            window.focus_window();
        }
    }

    pub fn request_inner_size<S: Into<Size>>(&self, size: S) -> Option<PhysicalSize<u32>> {
        match self {
            Self::Native(window) => window.request_inner_size(size),
            Self::Headless(window) => {
                let size = size.into().to_physical(self.scale_factor());
                window.set_size(size);
                Some(size)
            }
        }
    }

    pub fn set_ime_allowed(&self, allowed: bool) {
        match self {
            Self::Native(window) => window.set_ime_allowed(allowed),
            Self::Headless(window) => window.ime_allowed.store(allowed, Ordering::Relaxed),
        }
    }

    pub fn set_cursor(&self, cursor: impl Into<Cursor>) {
        match self {
            Self::Native(window) => window.set_cursor(cursor),
            Self::Headless(window) => {
                let icon = match cursor.into() {
                    Cursor::Icon(icon) => icon,
                    Cursor::Custom(_) => CursorIcon::Default,
                };
                *window.cursor_icon.lock().unwrap_or_else(PoisonError::into_inner) = icon;
            }
        }
    }

    pub fn id(&self) -> WindowId {
        match self {
            Self::Native(window) => window.id(),
            Self::Headless(window) => window.id,
        }
    }

    pub fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        match self {
            Self::Native(window) => window.window_handle(),
            Self::Headless(_) => Err(HandleError::NotSupported),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{HeadlessWindow, WindowHost};
    use winit::dpi::PhysicalSize;
    use winit::window::CursorIcon;

    #[test]
    fn window_host_initial_size_and_scale() {
        let host = WindowHost::Headless(HeadlessWindow::new(PhysicalSize::new(640, 480), 1.5));
        assert_eq!(host.inner_size(), PhysicalSize::new(640, 480));
        assert_eq!(host.scale_factor(), 1.5);
        assert!(host.headless().is_some());
        assert!(host.native().is_none());
    }

    #[test]
    fn window_host_redraw_request_is_consumed_once() {
        let headless = HeadlessWindow::new(PhysicalSize::new(640, 480), 1.0);
        let host = WindowHost::Headless(headless);
        host.request_redraw();
        let headless = host.headless().unwrap();
        assert!(headless.take_redraw_request());
        assert!(!headless.take_redraw_request());
    }

    #[test]
    fn window_host_request_inner_size_updates_headless_size() {
        let host = WindowHost::Headless(HeadlessWindow::new(PhysicalSize::new(640, 480), 1.0));
        assert_eq!(
            host.request_inner_size(PhysicalSize::new(800, 600)),
            Some(PhysicalSize::new(800, 600))
        );
        assert_eq!(host.inner_size(), PhysicalSize::new(800, 600));
    }

    #[test]
    fn window_host_headless_state_changes() {
        let host = WindowHost::Headless(HeadlessWindow::new(PhysicalSize::new(640, 480), 1.0));
        let headless = host.headless().unwrap();
        headless.set_size(PhysicalSize::new(300, 200));
        headless.set_scale_factor(2.0);
        assert_eq!(host.inner_size(), PhysicalSize::new(300, 200));
        assert_eq!(host.scale_factor(), 2.0);
        host.set_cursor(CursorIcon::Text);
        assert_eq!(headless.cursor_icon(), CursorIcon::Text);
        host.set_title("x");
        assert_eq!(headless.title(), "x");
        host.set_maximized(true);
        assert!(host.is_maximized());
        host.set_ime_allowed(true);
        assert!(headless.ime_allowed());
        host.focus_window();
        let _ = host.id();
    }
}
