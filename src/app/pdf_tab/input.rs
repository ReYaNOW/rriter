use crate::app::{App, keyboard::KeyInput};
use winit::event::ElementState;
use winit::keyboard::{KeyCode, PhysicalKey};

impl App {
    pub(crate) fn handle_pdf_key(&mut self, input: &KeyInput) -> bool {
        if !self.tabs.get(self.active_tab).is_some_and(|tab| tab.kind.is_pdf()) { return false; }
        // The search panel, terminal and other focused inputs own the keyboard while focused; their handlers run later.
        if !self.editor_has_input_focus() { return false; }
        let pressed = input.state == ElementState::Pressed;
        if self.modifiers.control_key() || self.modifiers.alt_key() || self.modifiers.super_key() {
            // The hidden editor of a PDF tab must never see edit/save shortcuts (Ctrl+S would
            // overwrite the .pdf with its empty text); other combos stay global (Ctrl+F, Ctrl+W, tab switching).
            let primary = crate::platform::primary_shortcut_modifier(self.modifiers);
            let edit_key = matches!(input.physical_key, PhysicalKey::Code(
                KeyCode::KeyC | KeyCode::KeyX | KeyCode::KeyV | KeyCode::KeyA | KeyCode::KeyZ | KeyCode::KeyY
                | KeyCode::KeyS | KeyCode::Slash | KeyCode::Backspace | KeyCode::Delete));
            if !edit_key { return false; }
            if pressed && primary && input.physical_key == PhysicalKey::Code(KeyCode::KeyC) { self.pdf_copy_selection(); }
            return true;
        }
        if pressed && input.physical_key == PhysicalKey::Code(KeyCode::Escape) {
            // Escape first drops the selection; without one it falls through so the search panel closes.
            if self.active_pdf_tab_mut().is_some_and(|pdf| pdf.selection.is_some() || pdf.pending_copy.is_some()) {
                if let Some(pdf) = self.active_pdf_tab_mut() { pdf.clear_selection(); }
                if let Some(window) = self.window.as_ref() { window.request_redraw(); }
                return true;
            }
            return false;
        }
        if pressed {
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

    /// Search panel text or case flag changed: start a fresh worker search for the active PDF tab.
    pub(crate) fn pdf_update_search(&mut self) {
        let query = self.search_editor.get_full_text();
        self.pdf_start_search(&query);
    }

    fn pdf_start_search(&mut self, query: &str) {
        let case_sensitive = self.search_case_sensitive;
        let request = self.active_pdf_tab_mut().and_then(|pdf| pdf.begin_search(query, case_sensitive));
        if let (Some(request), Some(worker)) = (request, self.pdf_worker.as_ref()) {
            let _ = worker.tx.send(request);
        }
    }

    /// Brings the active PDF tab's search in line with the panel: the panel query when it is
    /// open, no search otherwise. Skips the restart when the tab already holds that query.
    pub(crate) fn pdf_restart_search_if_open(&mut self) {
        let query = if self.show_search { self.search_editor.get_full_text() } else { String::new() };
        let Some(pdf) = self.active_pdf_tab() else { return };
        let up_to_date = pdf.search.query == query
            && (query.is_empty() || !pdf.search.matches.is_empty() || !pdf.search.done);
        if !up_to_date { self.pdf_start_search(&query); }
    }

    pub(crate) fn pdf_jump_search(&mut self, forward: bool) {
        if let Some(pdf) = self.active_pdf_tab_mut() { pdf.jump_match(forward); }
        if let Some(window) = self.window.as_ref() { window.request_redraw(); }
    }

    /// Copies the selection; when a page's text is not cached yet the copy completes on arrival.
    pub(crate) fn pdf_copy_selection(&mut self) {
        let text = {
            let Some(pdf) = self.active_pdf_tab_mut() else { return };
            let Some(selection) = pdf.selection else { return };
            pdf.pending_copy = Some(selection);
            pdf.take_copy_text()
        };
        if let Some(text) = text { self.set_clipboard_text(text); }
        if let Some(window) = self.window.as_ref() { window.request_redraw(); }
    }

    pub(crate) fn pdf_link_at(&self, x: f32, y: f32) -> Option<(usize, usize)> {
        self.active_pdf_tab()?.link_at(x, y)
    }

    /// Left press on the page area: remembers the start; it becomes a selection drag or a click on release.
    pub(crate) fn press_pdf_body(&mut self, x: f32, y: f32) {
        self.focus_document_text_surface();
        let Some(pdf) = self.active_pdf_tab_mut() else { return };
        pdf.press = Some((x, y));
        pdf.dragging = false;
        pdf.drag_anchor = None;
    }

    /// Pointer move while the left button is down on the page area; `true` = a PDF press is active.
    pub(crate) fn update_pdf_drag(&mut self, x: f32, y: f32) -> bool {
        let threshold = 4.0 * self.renderer.as_ref().map_or(1.0, |renderer| renderer.scale_factor);
        let Some(pdf) = self.active_pdf_tab_mut() else { return false };
        let Some((press_x, press_y)) = pdf.press else { return false };
        if !pdf.dragging {
            if (x - press_x).hypot(y - press_y) < threshold { return true; }
            pdf.dragging = true;
            pdf.drag_anchor = pdf.hit_char_at(press_x, press_y);
        }
        if let Some(head) = pdf.hit_char_at(x, y) {
            let anchor = *pdf.drag_anchor.get_or_insert(head);
            pdf.pending_copy = None;
            pdf.selection = Some(crate::app::pdf_tab::PdfSelection { anchor, head });
        }
        if let Some(window) = self.window.as_ref() { window.request_redraw(); }
        true
    }

    /// Left release: ends a selection drag, or treats the press as a click (clears the
    /// selection, follows a link). `true` = the release belonged to a PDF press.
    pub(crate) fn finish_pdf_drag(&mut self, x: f32, y: f32) -> bool {
        let Some(pdf) = self.active_pdf_tab_mut() else { return false };
        if pdf.press.take().is_none() { return false; }
        let dragged = std::mem::take(&mut pdf.dragging);
        pdf.drag_anchor = None;
        if !dragged {
            pdf.clear_selection();
            if let Some((page, idx)) = pdf.link_at(x, y) { self.pdf_follow_link(page, idx); }
        }
        if let Some(window) = self.window.as_ref() { window.request_redraw(); }
        true
    }

    fn pdf_follow_link(&mut self, page: usize, idx: usize) {
        use crate::pdf::LinkTarget;
        let Some(target) = self.active_pdf_tab().and_then(|pdf| pdf.links.get(page)?.get(idx)).map(|link| link.target.clone()) else { return };
        match target {
            LinkTarget::Uri(url) => {
                // Only plain http(s) reaches the OS opener; anything else from a hostile PDF is ignored.
                if crate::app::pdf_tab::text::link_url_allowed(&url) {
                    let _ = crate::platform::open_url(self.external_requests.sink(), &url);
                }
            }
            LinkTarget::Page { page, y_pt } => {
                if let Some(pdf) = self.active_pdf_tab_mut() { pdf.scroll_to_page(page, y_pt); }
            }
        }
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
