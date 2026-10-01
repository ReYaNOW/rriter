use crate::app::image_tab::{ImagePhase, ImageTabState};
use crate::renderer::Renderer;
use crate::ui_system::{UiClipRect, UiId, UiRegistry};

impl Renderer {
    pub(crate) fn draw_root_image_frame(
        &mut self, image: &ImageTabState, x: f32, y: f32, w: f32, h: f32,
        scale: f32, mx: f32, my: f32, ui: &mut UiRegistry,
    ) {
        self.push_rect(x, y, w, h, self.theme.bg);
        ui.push_clip(UiClipRect { x, y, w, h });
        ui.register_blocker(UiId::PdfBody, x, y, w, h, mx, my);
        match &image.phase {
            ImagePhase::Loading => self.draw_string_scaled_stable("Загрузка изображения…", x.round() + 24.0 * scale, (y + h * 0.5).round(), self.theme.fg, scale),
            ImagePhase::Failed(message) => self.draw_string_scaled_stable(message, x.round() + 24.0 * scale, (y + h * 0.5).round(), [1.0, 0.35, 0.35, 1.0], scale),
            ImagePhase::Ready => {
                let zoom = if image.zoom <= 0.0 { image.fit_scale(w, h) } else { image.zoom };
                let iw = (image.natural.0 * zoom).round();
                let ih = (image.natural.1 * zoom).round();
                let ix = (x + (w - iw) * 0.5 + image.offset.0).round();
                let iy = (y + (h - ih) * 0.5 + image.offset.1).round();
                if let Some(texture) = image.texture.as_ref() { self.draw_texture_quad(texture, ix, iy, iw, ih); }
            }
        }
        ui.pop_clip();
    }
}
