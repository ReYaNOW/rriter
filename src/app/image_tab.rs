use crate::markdown_media::{MediaError, MediaPixels};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};

#[derive(Clone, Debug)]
pub(crate) enum ImagePhase {
    Loading,
    Ready,
    Failed(String),
}

/// State owned by one image tab; decode work is off-thread and GL work stays on the UI thread.
pub(crate) struct ImageTabState {
    pub path: PathBuf,
    pub key: crate::platform::PathKey,
    pub phase: ImagePhase,
    pub natural: (f32, f32),
    pub texture_size: (u32, u32),
    pub texture: Option<glow::Texture>,
    pub zoom: f32,
    pub offset: (f32, f32),
    pub stamp: Option<crate::markdown_media::FileStamp>,
    rx: Option<Receiver<Result<MediaPixels, MediaError>>>,
    pending: Option<MediaPixels>,
    pub body: (f32, f32, f32, f32),
    drag: Option<(f32, f32)>,
    last_click: Option<std::time::Instant>,
}

impl ImageTabState {
    pub(crate) fn new(path: PathBuf) -> Self {
        let key = crate::platform::PathKey::new(&path);
        let mut state = Self {
            path,
            key,
            phase: ImagePhase::Loading,
            natural: (0.0, 0.0),
            texture_size: (0, 0),
            texture: None,
            zoom: 0.0,
            offset: (0.0, 0.0),
            stamp: None,
            rx: None,
            pending: None,
            body: (0.0, 0.0, 0.0, 0.0),
            drag: None,
            last_click: None,
        };
        state.start_load();
        state
    }

    pub(crate) fn start_load(&mut self) {
        if self.stamp.is_none() { self.phase = ImagePhase::Loading; }
        let path = self.path.clone();
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        let _ = crate::platform::spawn_named("rriter-image-load", move || {
            let _ = tx.send(crate::markdown_media::load_image_path(path));
        });
    }

    pub(crate) fn poll(&mut self, _renderer: &mut crate::renderer::Renderer) -> bool {
        let Some(rx) = self.rx.as_ref() else { return false };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => Err(MediaError::Crashed),
        };
        self.rx = None;
        match result {
            Ok(pixels) => {
                if self.texture.is_some() && pixels.stamp == self.stamp { return false; }
                self.natural = (pixels.natural_w, pixels.natural_h);
                self.texture_size = (pixels.raster_w, pixels.raster_h);
                self.stamp = pixels.stamp;
                self.pending = Some(pixels);
                true
            }
            Err(error) => {
                self.phase = ImagePhase::Failed(error.label().into_owned());
                true
            }
        }
    }

    pub(crate) fn upload_pending(&mut self, renderer: &mut crate::renderer::Renderer) -> bool {
        let Some(pixels) = self.pending.take() else { return false };
        let valid = (pixels.raster_w as usize)
            .checked_mul(pixels.raster_h as usize)
            .and_then(|n| n.checked_mul(4)) == Some(pixels.rgba.len());
        if !valid {
            self.phase = ImagePhase::Failed("Изображение повреждено".to_owned());
            return true;
        }
        if let Some(texture) = renderer.upload_rgba(pixels.raster_w, pixels.raster_h, &pixels.rgba) {
            if let Some(old) = self.texture.replace(texture) { renderer.delete_texture(old); }
            self.phase = ImagePhase::Ready;
        } else {
            self.phase = ImagePhase::Failed("Не удалось загрузить текстуру".to_owned());
        }
        true
    }

    pub(crate) fn release_texture(&mut self, renderer: &mut crate::renderer::Renderer) {
        if let Some(texture) = self.texture.take() { renderer.delete_texture(texture); }
    }

    pub(crate) fn fit_scale(&self, width: f32, height: f32) -> f32 {
        if self.natural.0 <= 0.0 || self.natural.1 <= 0.0 { return 1.0; }
        (width / self.natural.0).min(height / self.natural.1).min(1.0).max(0.01)
    }

    pub(crate) fn reset_fit(&mut self, width: f32, height: f32) {
        self.zoom = self.fit_scale(width, height);
        self.offset = (0.0, 0.0);
    }

    pub(crate) fn zoom_at(&mut self, factor: f32, x: f32, y: f32) {
        let (bx, by, bw, bh) = self.body;
        let old = if self.zoom <= 0.0 { self.fit_scale(self.body.2, self.body.3) } else { self.zoom.max(0.01) };
        let next = (old * factor).clamp(0.05, 16.0);
        let old_size = (self.natural.0 * old, self.natural.1 * old);
        let next_size = (self.natural.0 * next, self.natural.1 * next);
        let old_origin = image_origin((bw, bh), old_size);
        let next_origin = image_origin((bw, bh), next_size);
        let anchor = (x - bx - old_origin.0 - self.offset.0, y - by - old_origin.1 - self.offset.1);
        let ratio = next / old;
        self.offset = (
            x - bx - next_origin.0 - anchor.0 * ratio,
            y - by - next_origin.1 - anchor.1 * ratio,
        );
        self.zoom = next;
        self.clamp_offset(bw, bh);
    }

    pub(crate) fn scroll(&mut self, delta: f32) {
        self.offset.1 += delta;
        self.clamp_offset(self.body.2, self.body.3);
    }

    pub(crate) fn begin_drag(&mut self, x: f32, y: f32) { self.drag = Some((x, y)); }

    pub(crate) fn drag_to(&mut self, x: f32, y: f32) -> bool {
        let Some((px, py)) = self.drag.replace((x, y)) else { return false };
        self.offset.0 += x - px;
        self.offset.1 += y - py;
        self.clamp_offset(self.body.2, self.body.3);
        true
    }

    pub(crate) fn end_drag(&mut self) { self.drag = None; }

    pub(crate) fn double_click(&mut self, now: std::time::Instant, width: f32, height: f32) -> bool {
        let double = self.last_click.is_some_and(|last| now.duration_since(last).as_millis() < 500);
        self.last_click = Some(now);
        if double { self.reset_fit(width, height); }
        double
    }

    fn clamp_offset(&mut self, width: f32, height: f32) {
        let iw = self.natural.0 * self.zoom;
        let ih = self.natural.1 * self.zoom;
        self.offset.0 = self.offset.0.clamp((width - iw).min(0.0), 0.0);
        self.offset.1 = self.offset.1.clamp((height - ih).min(0.0), 0.0);
    }

}

pub(crate) fn image_origin(body: (f32, f32), image: (f32, f32)) -> (f32, f32) {
    ((body.0 - image.0).max(0.0) * 0.5, (body.1 - image.1).max(0.0) * 0.5)
}

impl crate::app::App {
    pub(crate) fn handle_image_key(&mut self, input: &crate::app::keyboard::KeyInput) -> bool {
        if !self.tabs.get(self.active_tab).is_some_and(|tab| tab.kind.is_image()) { return false; }
        if !self.editor_has_input_focus()
            || self.modifiers.control_key()
            || self.modifiers.alt_key()
            || self.modifiers.super_key()
            || self.modifiers.shift_key()
            || matches!(input.physical_key, winit::keyboard::PhysicalKey::Code(
                winit::keyboard::KeyCode::F1 | winit::keyboard::KeyCode::F2 | winit::keyboard::KeyCode::F3
                | winit::keyboard::KeyCode::F4 | winit::keyboard::KeyCode::F5 | winit::keyboard::KeyCode::F6
                | winit::keyboard::KeyCode::F7 | winit::keyboard::KeyCode::F8 | winit::keyboard::KeyCode::F9
                | winit::keyboard::KeyCode::F10 | winit::keyboard::KeyCode::F11 | winit::keyboard::KeyCode::F12
                | winit::keyboard::KeyCode::F13 | winit::keyboard::KeyCode::F14 | winit::keyboard::KeyCode::F15
                | winit::keyboard::KeyCode::F16 | winit::keyboard::KeyCode::F17 | winit::keyboard::KeyCode::F18
                | winit::keyboard::KeyCode::F19 | winit::keyboard::KeyCode::F20 | winit::keyboard::KeyCode::F21
                | winit::keyboard::KeyCode::F22 | winit::keyboard::KeyCode::F23 | winit::keyboard::KeyCode::F24
            ))
        {
            return false;
        }
        if input.state == winit::event::ElementState::Pressed
            && input.physical_key == winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Digit0)
        {
            let body = self.ui_registry.rect_for(crate::ui_system::UiId::PdfBody);
            if let Some(image) = self.tabs.get_mut(self.active_tab).and_then(|tab| tab.image.as_deref_mut()) {
                if let Some(rect) = body { image.body = rect; }
                image.reset_fit(image.body.2, image.body.3);
            }
            return true;
        }
        false
    }

    pub(crate) fn handle_image_mouse(&mut self, state: winit::event::ElementState, button: winit::event::MouseButton, x: f32, y: f32) -> bool {
        if !self.tabs.get(self.active_tab).is_some_and(|tab| tab.kind.is_image()) { return false; }
        if button != winit::event::MouseButton::Left { return false; }
        let body = self.ui_registry.rect_for(crate::ui_system::UiId::PdfBody);
        let in_body = body.is_some_and(|(bx, by, bw, bh)| crate::ui_system::point_in_rect(x, y, (bx, by, bw, bh)));
        let dragging = self.tabs.get(self.active_tab).and_then(|tab| tab.image.as_deref()).is_some_and(|image| image.drag.is_some());
        if state == winit::event::ElementState::Pressed && !in_body { return false; }
        if state == winit::event::ElementState::Released && !dragging { return false; }
        let Some(image) = self.tabs.get_mut(self.active_tab).and_then(|tab| tab.image.as_deref_mut()) else { return true };
        if state == winit::event::ElementState::Pressed {
            if let Some(rect) = body {
                image.body = rect;
            }
            if image.double_click(std::time::Instant::now(), image.body.2, image.body.3) { return true; }
            image.begin_drag(x, y);
        } else {
            image.end_drag();
        }
        if let Some(window) = self.window.as_ref() { window.request_redraw(); }
        true
    }

    pub(crate) fn open_image_tab(&mut self, path: PathBuf) {
        if !self.is_ide_mode { return; }
        let path = crate::platform::canonicalize_or_absolutize(&path);
        let key = crate::platform::PathKey::new(&path);
        if let Some(index) = self.tabs.iter().position(|tab| tab.image.as_ref().is_some_and(|image| image.key == key)) {
            if index != self.active_tab { self.switch_to_tab(index); }
            return;
        }
        let title = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| "Изображение".to_owned());
        let extension = path.extension().map(|ext| ext.to_string_lossy().into_owned()).unwrap_or_default();
        let tab = crate::app::EditorTab {
            editor: crate::editor::Editor::new(128),
            file_path: Some(path.clone()), file_key: Some(key),
            text_file_format: crate::platform::TextFileFormat::default(), base_title: title.clone(),
            file_extension: extension, markdown: Default::default(), pdf: None,
            image: Some(Box::new(ImageTabState::new(path))),
            scroll_y: crate::scroll::ScrollState::new(15.0), scroll_x: crate::scroll::ScrollState::new(15.0),
            spans: Vec::new(), completions: Vec::new(), foldable_ranges: Vec::new(), syntax_errors: Vec::new(),
            last_sent_version: u64::MAX, search_results: Vec::new(), search_current_idx: None,
            is_highlighted_once: false, is_highlight_complete: false,
            icon_key: crate::app::file_icons::file_icon_key_for_name(&title),
            closing_hints: Default::default(), kind: crate::app::EditorTabKind::Image,
            deleted: false, load: crate::app::TabLoad::Loaded,
        };
        self.tabs.push(tab);
        let index = self.tabs.len() - 1;
        if index == self.active_tab {
            self.sync_active_tab();
            self.image_tab_activated(index);
            self.save_tabs_state();
        } else {
            self.switch_to_tab(index);
        }
        self.show_welcome = false;
    }

    pub(crate) fn poll_image_tabs(&mut self) -> bool {
        let Some(renderer) = self.renderer.as_mut() else { return false };
        let Some(image) = self.tabs.get_mut(self.active_tab).and_then(|tab| tab.image.as_deref_mut()) else { return false };
        image.poll(renderer) | image.upload_pending(renderer)
    }

    pub(crate) fn image_tab_deactivated(&mut self, index: usize) {
        if let (Some(renderer), Some(image)) = (self.renderer.as_mut(), self.tabs.get_mut(index).and_then(|tab| tab.image.as_deref_mut())) {
            image.release_texture(renderer);
        }
    }

    pub(crate) fn image_tab_activated(&mut self, index: usize) {
        if let Some(image) = self.tabs.get_mut(index).and_then(|tab| tab.image.as_deref_mut())
            && image.texture.is_none() && image.rx.is_none() && image.pending.is_none()
        {
            image.start_load();
        }
    }

    pub(crate) fn revalidate_image_tabs(&mut self) {
        if let Some(image) = self.tabs.get_mut(self.active_tab).and_then(|tab| tab.image.as_deref_mut()) { image.start_load(); }
    }
}
