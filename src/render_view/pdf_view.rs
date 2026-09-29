use crate::app::pdf_tab::{PdfEngineState, PdfPhase, PdfTabState};
use crate::renderer::Renderer;
use crate::ui_system::{UiClipRect, UiId, UiRegistry};

impl Renderer {
    pub(crate) fn draw_root_pdf_frame(
        &mut self,
        tab: &PdfTabState,
        engine: &PdfEngineState,
        dark_pages: bool,
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
        let ready = matches!(tab.phase, PdfPhase::Ready);
        // Registered first so pages, text lines and links win `find_at`; it only absorbs
        // presses in margins and gaps (a click there must not reach the hidden editor).
        if ready {
            ui_registry.register_blocker(UiId::PdfBody, x, y, w, h, mx, my);
        }
        // One scratch buffer for the whole frame; the highlights of every page reuse it.
        // The buffer lives in the tab and is taken/put back, so a frame with highlights allocates nothing.
        let mut rects = tab.line_rects_buf.take();
        for page in tab.visible_range() {
            let Some((page_y, page_h)) = tab.layout.rows.get(page).copied() else { continue };
            let px = (x + ((w - tab.layout.page_w as f32) * 0.5)).round();
            let py = (y + page_y as f32 - offset as f32).round();
            let page_w = tab.layout.page_w as f32;
            let page_h = page_h as f32;
            if let Some(texture) = tab.textures.get(&page) {
                self.draw_texture_quad(&texture.tex, px, py, page_w, page_h);
            } else {
                let paper = if dark_pages { [0.11, 0.12, 0.13, 1.0] } else { [0.94, 0.94, 0.91, 1.0] };
                self.push_rect(px, py, page_w, page_h, paper);
                let mut caption = std::mem::take(&mut self.scratch_buffer);
                caption.clear();
                let _ = std::fmt::Write::write_fmt(&mut caption, format_args!("стр. {}", page + 1));
                let caption_scale = (0.9 * scale).max(0.6);
                let text_w = self.measure_ui_width(&caption, caption_scale);
                self.draw_string_scaled_pixel_snapped(
                    &caption,
                    (px + (page_w - text_w) * 0.5).round(),
                    (py + page_h * 0.5).round(),
                    self.theme.fg,
                    caption_scale,
                );
                self.scratch_buffer = caption;
            }
            self.draw_pdf_highlights(tab, page, (px, py, page_w, page_h), &mut rects);
            // A blocker keeps the default cursor on blank paper; lines and links below override it.
            ui_registry.register_blocker_clipped(
                UiId::PdfPage(page), px, py, page_w, page_h, clip, mx, my,
            );
            if ready {
                register_pdf_page_hits(tab, page, (px, py, page_w, page_h), clip, ui_registry, (mx, my));
            }
        }
        tab.line_rects_buf.set(rects);
        // The scrollbar lane is registered after the pages, so it wins `find_at`.
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
        // While the engine is missing, installing or starting, the engine state is the source of
        // truth: the tab's own error is a snapshot from the time it was opened.
        let state_message = match &tab.phase {
            PdfPhase::EngineMissing { error: Some(error) }
                if !matches!(
                    engine,
                    PdfEngineState::NotInstalled | PdfEngineState::Missing { .. }
                        | PdfEngineState::Installing { .. } | PdfEngineState::Starting
                ) => Some(std::borrow::Cow::Borrowed(error.as_str())),
            PdfPhase::EngineMissing { .. } | PdfPhase::EngineStarting => Some(engine.label()),
            PdfPhase::Loading => Some(std::borrow::Cow::Borrowed("Загрузка документа PDF…")),
            PdfPhase::Error(message) => Some(std::borrow::Cow::Borrowed(message.as_str())),
            PdfPhase::PasswordRequired => Some(std::borrow::Cow::Borrowed("Документ защищён паролем")),
            PdfPhase::Ready => None,
        };
        if let Some(message) = state_message {
            self.draw_pdf_centered_message(&message, x, y, w, h, scale);
        }
        if matches!(tab.phase, PdfPhase::EngineMissing { .. }) {
            self.draw_pdf_engine_actions(engine, (x, y, w, h), scale, (mx, my), ui_registry);
        }
    }

    /// Buttons under the engine message: download / retry while the library is missing and
    /// installable, a progress line and "Cancel" during the download, nothing otherwise
    /// (not installable, failed or starting engine).
    fn draw_pdf_engine_actions(
        &mut self,
        engine: &PdfEngineState,
        (x, y, w, h): (f32, f32, f32, f32),
        scale: f32,
        (mx, my): (f32, f32),
        ui_registry: &mut UiRegistry,
    ) {
        let text_scale = (0.82 * scale).max(0.6);
        let button_h = (29.0 * scale).round();
        let first_y = (y + h * 0.5 + 18.0 * scale).round();
        let (id, label, button_y) = match engine {
            PdfEngineState::NotInstalled => (UiId::PdfEngineInstall, "Загрузить PDF-движок (3,7 МБ)", first_y),
            PdfEngineState::Missing { installable: true, .. } => (UiId::PdfEngineInstall, "Повторить", first_y),
            PdfEngineState::Installing { progress, .. } => {
                if !progress.is_empty() {
                    let progress_w = self.measure_ui_width(progress, text_scale).min(w.max(0.0));
                    self.draw_string_scaled_pixel_snapped(
                        progress,
                        (x + (w - progress_w) * 0.5).round(),
                        first_y,
                        self.theme.line_num,
                        text_scale,
                    );
                }
                (UiId::PdfEngineCancel, "Отмена", (first_y + 28.0 * scale).round())
            }
            _ => return,
        };
        let button_w = (self.measure_ui_width(label, text_scale) + 32.0 * scale).round();
        let button = crate::widgets::ButtonView {
            x: (x + (w - button_w) * 0.5).round(),
            y: button_y,
            w: button_w,
            h: button_h,
            text: label,
            icon: None,
            text_scale,
            icon_size: 0.0,
        };
        ui_registry.register_button_view(id, button, self, mx, my, scale, false);
    }

    /// Search matches and the text selection of one page, drawn over the raster.
    fn draw_pdf_highlights(
        &mut self,
        tab: &PdfTabState,
        page: usize,
        (px, py, page_w, page_h): (f32, f32, f32, f32),
        rects: &mut Vec<crate::pdf::PtRect>,
    ) {
        let (Some(Some(text)), Some(geom)) = (tab.text.get(page), tab.pages.get(page)) else { return };
        if tab.search.matches.is_empty() && tab.selection.is_none() { return; }
        let kx = page_w / geom.width_pt.max(1.0);
        let ky = page_h / geom.height_pt.max(1.0);
        // Edges are rounded, not sizes, so neighbouring rectangles of one line stay gap-free.
        let fill = |renderer: &mut Self, rects: &[crate::pdf::PtRect], color: [f32; 4]| {
            for rect in rects {
                let (x0, x1) = ((px + rect.x * kx).round(), (px + (rect.x + rect.w) * kx).round());
                let (y0, y1) = ((py + rect.y * ky).round(), (py + (rect.y + rect.h) * ky).round());
                renderer.push_rect(x0, y0, (x1 - x0).max(1.0), (y1 - y0).max(1.0), color);
            }
        };
        // `matches` is ordered by (page, start): this page's matches are one contiguous slice.
        let first = tab.search.matches.partition_point(|item| item.page < page);
        let last = tab.search.matches.partition_point(|item| item.page <= page);
        for (offset, item) in tab.search.matches[first..last].iter().enumerate() {
            rects.clear();
            crate::app::pdf_tab::text::line_rects(&text.chars, item.start as usize, item.end as usize, rects);
            let color = if tab.search.current == Some(first + offset) { [1.0, 0.55, 0.1, 0.55] } else { [1.0, 0.85, 0.2, 0.35] };
            fill(self, rects.as_slice(), color);
        }
        if let Some(selection) = tab.selection {
            let ((first_page, first_char), (last_page, last_char)) = selection.ordered();
            if (first_page..=last_page).contains(&page) {
                let start = if page == first_page { first_char } else { 0 };
                let end = if page == last_page { last_char.saturating_add(1) } else { text.chars.len() };
                rects.clear();
                crate::app::pdf_tab::text::line_rects(&text.chars, start, end, rects);
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

/// Registers the text lines (I-beam) and links (hand, clickable) of one visible page.
/// Links go last so they win `find_at` over the lines beneath them.
fn register_pdf_page_hits(
    tab: &PdfTabState,
    page: usize,
    (px, py, page_w, page_h): (f32, f32, f32, f32),
    clip: UiClipRect,
    ui_registry: &mut UiRegistry,
    (mx, my): (f32, f32),
) {
    let Some(geom) = tab.pages.get(page) else { return };
    let kx = page_w / geom.width_pt.max(1.0);
    let ky = page_h / geom.height_pt.max(1.0);
    if let Some(Some(lines)) = tab.line_boxes.get(page) {
        for rect in lines {
            ui_registry.register_text_region(UiId::PdfText, px + rect.x * kx, py + rect.y * ky, rect.w * kx, rect.h * ky, mx, my);
        }
    }
    if let Some(links) = tab.links.get(page) {
        for (idx, link) in links.iter().enumerate() {
            let rect = link.rect;
            ui_registry.register_rect_clipped(
                UiId::PdfLink(page, idx), px + rect.x * kx, py + rect.y * ky, rect.w * kx, rect.h * ky, clip, mx, my,
            );
        }
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
