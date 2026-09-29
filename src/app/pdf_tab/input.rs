use crate::app::{App, keyboard::KeyInput};
use winit::event::ElementState;
use winit::keyboard::{KeyCode, PhysicalKey};

impl App {
    pub(crate) fn handle_pdf_key(&mut self, input: &KeyInput) -> bool {
        if !self.tabs.get(self.active_tab).is_some_and(|tab| tab.kind.is_pdf()) { return false; }
        if self.modifiers.control_key() || self.modifiers.alt_key() || self.modifiers.super_key() { return false; }
        if input.state == ElementState::Pressed {
            if let Some(pdf) = self.active_pdf_tab_mut() {
                match input.physical_key {
                    PhysicalKey::Code(KeyCode::PageUp) => pdf.page_up(),
                    PhysicalKey::Code(KeyCode::PageDown) => pdf.page_down(),
                    PhysicalKey::Code(KeyCode::Home) => pdf.home(),
                    PhysicalKey::Code(KeyCode::End) => pdf.end(),
                    PhysicalKey::Code(KeyCode::ArrowUp) => pdf.scroll_by(-(48.0 * pdf.layout_scale).round()),
                    PhysicalKey::Code(KeyCode::ArrowDown) => pdf.scroll_by((48.0 * pdf.layout_scale).round()),
                    _ => {}
                }
            }
            if let Some(window) = self.window.as_ref() { window.request_redraw(); }
        }
        true
    }

    pub(crate) fn handle_pdf_wheel(&mut self, dy: f32) {
        let Some(pdf) = self.active_pdf_tab_mut() else { return };
        pdf.scroll_by(dy);
        if let Some(window) = self.window.as_ref() { window.request_redraw(); }
    }

    pub(crate) fn update_pdf_scrollbar_drag(&mut self, x: f32, y: f32) -> bool {
        let geometry = self.pdf_scrollbar_geometry();
        let Some(pdf) = self.active_pdf_tab_mut() else { return false };
        crate::app::mouse::drag_scrollbar(&mut pdf.scroll, geometry, x, y).is_some()
    }

    pub(crate) fn press_pdf_scrollbar(&mut self, x: f32, y: f32) {
        let geometry = self.pdf_scrollbar_geometry();
        let Some(pdf) = self.active_pdf_tab_mut() else { return };
        let _ = crate::app::mouse::press_scrollbar(&mut pdf.scroll, geometry, x, y);
    }

    fn pdf_scrollbar_geometry(&self) -> Option<crate::render_view::scrollbar_widget::ScrollbarGeometry> {
        let scale = self.renderer.as_ref()?.scale_factor;
        let lane = self.ui_registry.rect_for(crate::ui_system::UiId::PdfScrollY)?;
        let pdf = self.active_pdf_tab()?;
        let geometry = crate::render_view::scrollbar_widget::Scrollbar {
            style: crate::render_view::scrollbar_widget::ScrollbarStyle::MARKDOWN_READ,
            axis: crate::render_view::scrollbar_widget::ScrollbarAxis::Vertical,
            lane,
            extent: crate::render_view::scrollbar_widget::ScrollbarExtent::new(pdf.viewport.1 as f32, pdf.layout.total_h as f32, pdf.scroll.current),
        }.geometry(scale);
        geometry
    }
}
