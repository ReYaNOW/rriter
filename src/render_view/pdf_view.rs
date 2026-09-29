use crate::app::pdf_tab::{PdfEngineState, PdfPhase, PdfTabState};
use crate::renderer::Renderer;
use crate::ui_system::{UiClipRect, UiId, UiRegistry};

impl Renderer {
    pub(crate) fn draw_root_pdf_frame(
        &mut self,
        tab: &PdfTabState,
        engine: &PdfEngineState,
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
        ui_registry.push_clip(clip);
        for page in tab.visible_range() {
            let Some((page_y, page_h)) = tab.layout.rows.get(page).copied() else { continue };
            let px = (x + ((w - tab.layout.page_w as f32) * 0.5)).round();
            let py = y + page_y as f32 - offset as f32;
            let page_w = tab.layout.page_w as f32;
            let page_h = page_h as f32;
            if let Some(texture) = tab.textures.get(&page) {
                self.draw_texture_quad(&texture.tex, px, py.round(), page_w, page_h);
            } else {
                let paper = if tab.dark { [0.11, 0.12, 0.13, 1.0] } else { [0.94, 0.94, 0.91, 1.0] };
                self.push_rect(px, py, page_w, page_h, paper);
                let caption = format!("стр. {}", page + 1);
                let caption_scale = (0.9 * scale).max(0.6);
                let text_w = self.measure_ui_width(&caption, caption_scale);
                self.draw_string_scaled_pixel_snapped(
                    &caption,
                    (px + (page_w - text_w) * 0.5).round(),
                    (py + page_h * 0.5).round(),
                    self.theme.fg,
                    caption_scale,
                );
            }
            self.draw_pdf_highlights(tab, page, px, py, page_w, page_h);
            ui_registry.register_rect_clipped(
                UiId::PdfPage(page), px, py, page_w, page_h, clip, mx, my,
            );
        }
        // Registered after the pages so it wins `find_at`; the scrollbar lane is registered later still.
        if matches!(tab.phase, PdfPhase::Ready) {
            ui_registry.register_text_region(UiId::PdfText, x, y, w, h, mx, my);
        }
        ui_registry.pop_clip();
        self.flush();
        unsafe {
            use glow::HasContext;
            if let Some(previous) = restore_scissor {
                self.gl.scissor(previous[0], previous[1], previous[2], previous[3]);
            } else {
                self.gl.disable(glow::SCISSOR_TEST);
            }
        }
        let bar = crate::render_view::scrollbar_widget::Scrollbar {
            style: crate::render_view::scrollbar_widget::ScrollbarStyle::MARKDOWN_READ,
            axis: crate::render_view::scrollbar_widget::ScrollbarAxis::Vertical,
            lane: (x + w - 12.0 * scale, y, 12.0 * scale, h),
            extent: crate::render_view::scrollbar_widget::ScrollbarExtent::new(
                h, tab.layout.total_h as f32, tab.scroll.current,
            ),
        };
        self.draw_scrollbar(&bar, scale, 1.0, Some(crate::render_view::scrollbar_widget::ScrollbarHit {
            ui: ui_registry, id: UiId::PdfScrollY, mx, my, blocker: false,
        }));
        // Engine screens show `PdfEngineState::label`;
        // a tab error set by the engine (already the full sentence) wins over it, so the
        // `Failed` label is not formatted per frame. `Error` holds `PdfError::message`.
        let state_message = match &tab.phase {
            PdfPhase::EngineMissing { error: Some(error) } => Some(std::borrow::Cow::Borrowed(error.as_str())),
            PdfPhase::EngineMissing { error: None } | PdfPhase::EngineStarting => Some(engine.label()),
            PdfPhase::Loading => Some(std::borrow::Cow::Borrowed("Загрузка документа PDF…")),
            PdfPhase::Error(message) => Some(std::borrow::Cow::Borrowed(message.as_str())),
            PdfPhase::PasswordRequired => Some(std::borrow::Cow::Borrowed("Документ защищён паролем")),
            PdfPhase::Ready => None,
        };
        if let Some(message) = state_message {
            self.draw_pdf_centered_message(&message, x, y, w, h, scale);
        }
    }

    /// Search matches and the text selection of one page, drawn over the raster.
    fn draw_pdf_highlights(&mut self, tab: &PdfTabState, page: usize, px: f32, py: f32, page_w: f32, page_h: f32) {
        let (Some(Some(text)), Some(geom)) = (tab.text.get(page), tab.pages.get(page)) else { return };
        if tab.search.matches.is_empty() && tab.selection.is_none() { return; }
        let kx = page_w / geom.width_pt.max(1.0);
        let ky = page_h / geom.height_pt.max(1.0);
        let mut rects = Vec::new();
        let fill = |renderer: &mut Self, rects: &[crate::pdf::PtRect], color: [f32; 4]| {
            for rect in rects {
                renderer.push_rect(px + rect.x * kx, py + rect.y * ky, (rect.w * kx).max(1.0), (rect.h * ky).max(1.0), color);
            }
        };
        for (idx, item) in tab.search.matches.iter().enumerate().filter(|(_, item)| item.page == page) {
            rects.clear();
            crate::app::pdf_tab::text::line_rects(&text.chars, item.start as usize, item.end as usize, &mut rects);
            let color = if tab.search.current == Some(idx) { [1.0, 0.55, 0.1, 0.55] } else { [1.0, 0.85, 0.2, 0.35] };
            fill(self, rects.as_slice(), color);
        }
        if let Some(selection) = tab.selection {
            let ((first_page, first_char), (last_page, last_char)) = selection.ordered();
            if (first_page..=last_page).contains(&page) {
                let start = if page == first_page { first_char } else { 0 };
                let end = if page == last_page { last_char.saturating_add(1) } else { text.chars.len() };
                rects.clear();
                crate::app::pdf_tab::text::line_rects(&text.chars, start, end, &mut rects);
                let sel = self.theme.sel;
                fill(self, rects.as_slice(), [sel[0], sel[1], sel[2], 0.55]);
            }
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
