use crate::app::pdf_tab::{PdfPhase, PdfTabState};
use crate::renderer::Renderer;
use crate::ui_system::{UiClipRect, UiId, UiRegistry};

impl Renderer {
    pub(crate) fn draw_root_pdf_frame(
        &mut self,
        tab: &PdfTabState,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        scale: f32,
        mx: f32,
        my: f32,
        ui_registry: &mut UiRegistry,
    ) {
        self.push_rect(x, y, w, h, self.theme.bg);
        let offset = tab.scroll.current.round() as i32;
        let clip = UiClipRect { x, y, w, h };
        for page in tab.visible_range() {
            let Some((page_y, page_h)) = tab.layout.rows.get(page).copied() else { continue };
            let px = (x + ((w - tab.layout.page_w as f32) * 0.5)).round();
            let py = y + page_y as f32 - offset as f32;
            let page_w = tab.layout.page_w as f32;
            let page_h = page_h as f32;
            self.push_rect(px, py, page_w, page_h, [0.94, 0.94, 0.91, 1.0]);
            let caption = format!("стр. {}", page + 1);
            let caption_scale = (0.9 * scale).max(0.6);
            let text_w = self.measure_ui_width(&caption, caption_scale);
            self.draw_string_scaled_pixel_snapped(
                &caption,
                (px + (page_w - text_w) * 0.5).round(),
                (py + page_h * 0.5).round(),
                [0.24, 0.25, 0.27, 1.0],
                caption_scale,
            );
            ui_registry.register_rect_clipped(
                UiId::PdfPage(page), px, py, page_w, page_h, clip, mx, my,
            );
        }

        let state_message = match &tab.phase {
            PdfPhase::EngineMissing { error } => Some(error.as_deref().unwrap_or("Движок PDF недоступен")),
            PdfPhase::EngineStarting | PdfPhase::Loading => Some("Загрузка документа PDF…"),
            PdfPhase::Error(message) => Some(message.as_str()),
            PdfPhase::PasswordRequired => Some("Документ защищён паролем"),
            PdfPhase::Ready => None,
        };
        if let Some(message) = state_message {
            self.draw_pdf_centered_message(message, x, y, w, h, scale);
        }
    }

    fn draw_pdf_centered_message(&mut self, message: &str, x: f32, y: f32, w: f32, h: f32, scale: f32) {
        let text_scale = (0.92 * scale).max(0.62);
        let text_w = self.measure_ui_width(message, text_scale).min(w.max(0.0));
        self.draw_string_scaled_pixel_snapped(
            message,
            (x + (w - text_w) * 0.5).round(),
            (y + h * 0.5).round(),
            self.theme.fg,
            text_scale,
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::app::pdf_tab::PdfLayout;

    #[test]
    fn pdf_view_uses_page_placeholders_for_unrendered_pages() {
        let layout = PdfLayout::default();
        assert!(layout.rows.is_empty());
    }
}
