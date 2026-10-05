use crate::app::image_tab::{ImagePhase, ImageTabState};
use crate::renderer::Renderer;
use crate::theme::UiRole;
use crate::ui_system::{UiClipRect, UiId, UiRegistry};

impl Renderer {
    pub(crate) fn draw_root_image_frame(
        &mut self, image: &ImageTabState, x: f32, y: f32, w: f32, h: f32,
        scale: f32, mx: f32, my: f32, ui: &mut UiRegistry,
    ) {
        self.push_rect(x, y, w, h, self.ui.pick(UiRole::BgPanel, self.theme.bg));
        self.flush();
        let restore_scissor = unsafe {
            use glow::HasContext;
            let enabled = self.gl.is_enabled(glow::SCISSOR_TEST);
            let mut previous = [0i32; 4];
            if enabled {
                self.gl.get_parameter_i32_slice(glow::SCISSOR_BOX, &mut previous);
            }
            self.gl.enable(glow::SCISSOR_TEST);
            let requested = [
                x.round() as i32,
                (self.height - (y + h)).round() as i32,
                w.round().max(0.0) as i32,
                h.round().max(0.0) as i32,
            ];
            let active = if enabled {
                crate::render_view::intersect_scissor_boxes(previous, requested)
            } else {
                requested
            };
            self.gl.scissor(active[0], active[1], active[2], active[3]);
            enabled.then_some(previous)
        };
        ui.push_clip(UiClipRect { x, y, w, h });
        ui.register_blocker(UiId::PdfBody, x, y, w, h, mx, my);
        match &image.phase {
            ImagePhase::Loading => self.draw_string_scaled_stable("Загрузка изображения…", x.round() + 24.0 * scale, (y + h * 0.5).round(), self.ui.pick(UiRole::TextPrimary, self.theme.fg), scale),
            ImagePhase::Failed(message) => self.draw_string_scaled_stable(message, x.round() + 24.0 * scale, (y + h * 0.5).round(), self.ui.pick(UiRole::Error, [1.0, 0.35, 0.35, 1.0]), scale),
            ImagePhase::Ready => {
                let zoom = if image.zoom <= 0.0 { image.fit_scale(w, h) } else { image.zoom };
                let iw = (image.natural.0 * zoom).round();
                let ih = (image.natural.1 * zoom).round();
                let origin = crate::app::image_tab::image_origin((w, h), (iw, ih));
                let ix = (x + origin.0 + image.offset.0).round();
                let iy = (y + origin.1 + image.offset.1).round();
                if let Some(texture) = image.texture.as_ref() { self.draw_texture_quad(texture, ix, iy, iw, ih); }
            }
        }
        ui.pop_clip();
        self.flush();
        unsafe {
            use glow::HasContext;
            if let Some(previous) = restore_scissor {
                self.gl.scissor(previous[0], previous[1], previous[2], previous[3]);
            } else {
                self.gl.disable(glow::SCISSOR_TEST);
            }
        }
    }
}
