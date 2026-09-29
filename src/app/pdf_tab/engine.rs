use super::{PdfEngineState, PdfEventOutcome, PdfPhase, PdfTabState};
use crate::app::{App, EditorTab, EditorTabKind};
use crate::pdf::{DocGens, DocId, PdfEvent, PdfRequest};
use std::path::PathBuf;
use std::sync::Arc;

impl App {
    pub fn pdf_tab_mut(&mut self, idx: usize) -> Option<&mut PdfTabState> {
        self.tabs.get_mut(idx)?.pdf.as_deref_mut()
    }

    pub fn active_pdf_tab_mut(&mut self) -> Option<&mut PdfTabState> {
        self.pdf_tab_mut(self.active_tab)
    }

    pub fn active_pdf_tab(&self) -> Option<&PdfTabState> {
        self.tabs.get(self.active_tab)?.pdf.as_deref()
    }

    pub fn ensure_pdf_engine(&mut self) {
        if !matches!(self.pdf_engine, PdfEngineState::NotStarted) {
            return;
        }
        match crate::pdf::library::locate() {
            crate::pdf::library::LocateResult::Found(path) => {
                self.pdf_worker = Some(crate::pdf::worker::start(path, &self.ui_waker));
                self.pdf_engine = PdfEngineState::Starting;
            }
            crate::pdf::library::LocateResult::Missing { message, installable } => {
                self.pdf_engine = PdfEngineState::Missing { message, installable };
            }
        }
    }

    pub fn open_pdf_tab(&mut self, path: PathBuf) {
        self.open_pdf_tab_at(path, None);
    }

    /// Session restore: the position is in the tab before the switch saves the session.
    pub fn open_pdf_tab_restored(&mut self, path: PathBuf, page: usize, frac: f32) {
        self.open_pdf_tab_at(path, Some((page, frac)));
    }

    fn open_pdf_tab_at(&mut self, path: PathBuf, restore: Option<(usize, f32)>) {
        if !self.is_ide_mode {
            return;
        }
        self.ensure_pdf_engine();
        let gens = Arc::new(DocGens::new());
        let doc = DocId(self.next_doc_id);
        self.next_doc_id = self.next_doc_id.saturating_add(1);
        let (phase, open_now) = match &self.pdf_engine {
            PdfEngineState::Ready => (PdfPhase::Loading, true),
            PdfEngineState::Starting => (PdfPhase::EngineStarting, false),
            PdfEngineState::Missing { message, .. } => (
                PdfPhase::EngineMissing { error: Some(message.clone()) },
                false,
            ),
            PdfEngineState::Failed(message) => (
                PdfPhase::EngineMissing { error: Some(format!("движок PDF остановлен: {message}. Перезапустите RRiter")) },
                false,
            ),
            PdfEngineState::NotStarted | PdfEngineState::Installing { .. } => (
                PdfPhase::EngineMissing { error: None },
                false,
            ),
        };
        let title = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| "PDF".to_owned());
        let extension = path.extension().map(|ext| ext.to_string_lossy().into_owned()).unwrap_or_default();
        let icon_key = crate::app::file_icons::file_icon_key_for_name(&title);
        let editor = crate::editor::Editor::new(128);
        let tab = EditorTab {
            editor,
            file_key: Some(crate::platform::PathKey::new(&path)),
            file_path: Some(path.clone()),
            text_file_format: crate::platform::TextFileFormat::default(),
            base_title: title,
            file_extension: extension,
            markdown: Default::default(),
            pdf: Some(Box::new(PdfTabState::new(path.clone(), Arc::clone(&gens), phase))),
            scroll_y: crate::scroll::ScrollState::new(15.0),
            scroll_x: crate::scroll::ScrollState::new(15.0),
            spans: Vec::new(), completions: Vec::new(), foldable_ranges: Vec::new(),
            syntax_errors: Vec::new(), last_sent_version: u64::MAX,
            search_results: Vec::new(), search_current_idx: None,
            is_highlighted_once: false, is_highlight_complete: false,
            icon_key, closing_hints: Default::default(), kind: EditorTabKind::Pdf, deleted: false,
        };
        let index = self.tabs.len();
        self.tabs.push(tab);
        if let Some(state) = self.pdf_tab_mut(index) { state.doc = Some(doc); state.restore = restore; }
        if index == self.active_tab {
            self.sync_active_tab();
            self.pdf_tab_activated(index);
            self.save_tabs_state();
        } else {
            self.switch_to_tab(index);
        }
        if open_now { self.send_pdf_open(doc); }
        self.show_welcome = false;
    }

    fn send_pdf_open(&mut self, id: DocId) {
        let Some(index) = self.tabs.iter().position(|tab| tab.pdf.as_ref().is_some_and(|pdf| pdf.doc == Some(id))) else { return };
        let Some(pdf) = self.tabs[index].pdf.as_ref() else { return };
        if let Some(worker) = self.pdf_worker.as_ref() {
            let request = PdfRequest::Open { id, path: pdf.path.clone(), gens: Arc::clone(&pdf.gens) };
            if worker.tx.send(request).is_ok() {
                let _ = self.tabs[index].pdf.as_mut().map(|state| state.apply_event(&PdfEvent::LoadStarted));
            }
        }
    }

    pub fn poll_pdf_worker(&mut self) -> bool {
        let mut events = Vec::new();
        let mut disconnected = false;
        if let Some(worker) = self.pdf_worker.as_ref() {
            loop {
                match worker.rx.try_recv() {
                    Ok(event) => events.push(event),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => { disconnected = true; break; }
                }
            }
        }
        if disconnected {
            self.pdf_worker = None;
            events.push(PdfEvent::EngineFailed("поток движка завершился".to_owned()));
        }
        let mut redraw = false;
        for event in events {
            match event {
                PdfEvent::EngineReady => {
                    self.pdf_engine = PdfEngineState::Ready;
                    let docs = self.tabs.iter().filter_map(|tab| {
                        let pdf = tab.pdf.as_ref()?;
                        matches!(pdf.phase, PdfPhase::EngineStarting | PdfPhase::EngineMissing { .. }).then_some(pdf.doc).flatten()
                    }).collect::<Vec<_>>();
                    for id in docs { self.send_pdf_open(id); }
                    redraw = true;
                }
                PdfEvent::EngineFailed(message) => {
                    self.pdf_engine = PdfEngineState::Failed(message.clone());
                    for tab in &mut self.tabs {
                        if let Some(pdf) = tab.pdf.as_mut() {
                            let error = format!("движок PDF остановлен: {message}. Перезапустите RRiter");
                            let _ = pdf.apply_event(&PdfEvent::EngineUnavailable(error));
                        }
                    }
                    redraw = true;
                }
                other => {
                    let Some(id) = pdf_event_doc_id(&other) else { continue };
                    let active_doc = self.tabs.get(self.active_tab).and_then(|tab| tab.pdf.as_ref()).and_then(|pdf| pdf.doc);
                    if matches!(other, PdfEvent::Page { .. }) && active_doc != Some(id) { continue; }
                    let Some(tab) = self.tabs.iter_mut().find(|tab| tab.pdf.as_ref().is_some_and(|pdf| pdf.doc == Some(id))) else { continue };
                    let mut copied = None;
                    if let Some(pdf) = tab.pdf.as_mut() {
                        redraw |= matches!(pdf.apply_event(&other), PdfEventOutcome::Redraw | PdfEventOutcome::Bitmap);
                        if matches!(other, PdfEvent::Text { .. } | PdfEvent::TextFailed { .. }) { copied = pdf.take_copy_text(); }
                    }
                    if let Some(text) = copied { self.set_clipboard_text(text); }
                    if matches!(other, PdfEvent::Opened { .. }) && active_doc == Some(id) { self.pdf_restart_search_if_open(); }
                }
            }
        }
        redraw
    }

    pub fn pdf_prepare_frame(&mut self, renderer: &mut crate::renderer::Renderer) {
        for texture in self.pdf_textures_to_free.drain(..) { renderer.delete_texture(texture); }
        if !self.tabs.get(self.active_tab).is_some_and(|tab| tab.kind.is_pdf()) { return; }
        let s = renderer.scale_factor;
        let panel_left = self.ide_panel.visible_left_width(s);
        let top = crate::render_view::editor_content_top_inset(self.show_welcome, self.is_ide_mode, false, s);
        let bottom = self.ide_panel.editor_reserved_bottom_height(s);
        let h = crate::render_view::editor_view_height(renderer.height, top, bottom, self.is_ide_mode, s);
        let (x, y, w, h) = renderer.tab_body_rect(s, panel_left, top, h);
        let dark_pages = self.pdf_dark_pages;
        let window = self.window.as_ref();
        let worker = self.pdf_worker.as_ref();
        let (tabs, textures_to_free) = (&mut self.tabs, &mut self.pdf_textures_to_free);
        if let Some(tab) = tabs.get_mut(self.active_tab).and_then(|tab| tab.pdf.as_deref_mut()) {
            tab.set_viewport(w.max(0.0) as u32, h.max(0.0) as u32, s);
            tab.body = (x, y, w, h);
            let wanted = tab.wanted_range();
            tab.textures.retain(|page, texture| {
                if wanted.contains(page) { true } else { textures_to_free.push(texture.tex); false }
            });
            let count = tab.pending_bitmaps.len().min(2);
            for bitmap in tab.pending_bitmaps.drain(..count) {
                if !wanted.contains(&bitmap.page)
                    || bitmap.r#gen != tab.gens.render.load(std::sync::atomic::Ordering::Relaxed)
                    || (bitmap.width_px as usize).checked_mul(bitmap.height_px as usize).and_then(|size| size.checked_mul(4)) != Some(bitmap.rgba.len()) { continue; }
                if let Some(tex) = renderer.upload_rgba(bitmap.width_px, bitmap.height_px, &bitmap.rgba) {
                    if let Some(old) = tab.textures.insert(bitmap.page, super::PageTexture { tex, width_px: bitmap.width_px, height_px: bitmap.height_px, r#gen: bitmap.r#gen }) {
                        textures_to_free.push(old.tex);
                    }
                } else {
                    tab.requested.insert((bitmap.page, bitmap.r#gen));
                }
            }
            if !tab.pending_bitmaps.is_empty() {
                if let Some(window) = window { window.request_redraw(); }
            }
            let mut requests = tab.take_render_requests(dark_pages);
            requests.extend(tab.take_text_requests());
            if let Some(worker) = worker {
                for request in requests { let _ = worker.tx.send(request); }
            }
        }
    }

    pub fn prepare_pdf_tab_close(&mut self, idx: usize) {
        {
            let (tabs, textures_to_free) = (&mut self.tabs, &mut self.pdf_textures_to_free);
            if let Some(pdf) = tabs.get_mut(idx).and_then(|tab| tab.pdf.as_deref_mut()) {
                textures_to_free.extend(pdf.textures.drain().map(|(_, texture)| texture.tex));
            }
        }
        let id = {
            let Some(pdf) = self.pdf_tab_mut(idx) else { return };
            pdf.gens.closed.store(true, std::sync::atomic::Ordering::Relaxed);
            pdf.pending_bitmaps.clear();
            pdf.doc
        };
        if let Some(id) = id && let Some(worker) = self.pdf_worker.as_ref() { let _ = worker.tx.send(PdfRequest::Close { id }); }
    }

    pub fn pdf_tab_deactivated(&mut self, idx: usize) {
        let (tabs, textures_to_free) = (&mut self.tabs, &mut self.pdf_textures_to_free);
        if let Some(pdf) = tabs.get_mut(idx).and_then(|tab| tab.pdf.as_deref_mut()) {
            pdf.pending_bitmaps.clear();
            pdf.requested.clear();
            pdf.gens.clear_wanted();
            textures_to_free.extend(pdf.textures.drain().map(|(_, texture)| texture.tex));
        }
    }

    pub fn pdf_tab_activated(&mut self, idx: usize) {
        if let Some(pdf) = self.pdf_tab_mut(idx) { pdf.layout_dirty = true; }
        self.pdf_restart_search_if_open();
    }

    pub fn toggle_pdf_dark_pages(&mut self) {
        self.pdf_dark_pages = !self.pdf_dark_pages;
        self.save_current_config();
        for tab in &self.tabs {
            if let Some(pdf) = tab.pdf.as_ref() { pdf.bump_render_gen(); }
        }
        if let Some(window) = self.window.as_ref() { window.request_redraw(); }
    }
}

fn pdf_event_doc_id(event: &PdfEvent) -> Option<DocId> {
    match event {
        PdfEvent::Opened { id, .. } | PdfEvent::OpenFailed { id, .. }
        | PdfEvent::Page { id, .. } | PdfEvent::RenderSkipped { id, .. }
        | PdfEvent::Text { id, .. } | PdfEvent::TextFailed { id, .. }
        | PdfEvent::SearchPage { id, .. } | PdfEvent::SearchDone { id, .. } => Some(*id),
        PdfEvent::EngineReady | PdfEvent::EngineFailed(_) | PdfEvent::LoadStarted | PdfEvent::EngineUnavailable(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::pdf_tab::PageTexture;

    #[test]
    fn bulk_close_queues_pdf_textures_and_sends_worker_close() {
        let (_context, mut app) =
            crate::platform::offscreen_gl::test_support::offscreen_test_app(64, 64, 1.0);
        app.is_ide_mode = true;
        app.pdf_engine = PdfEngineState::Ready;
        let (tx, rx) = std::sync::mpsc::channel();
        let (_event_tx, event_rx) = std::sync::mpsc::channel();
        app.pdf_worker = Some(crate::pdf::PdfWorkerHandle { tx, rx: event_rx });
        app.open_pdf_tab(PathBuf::from("bulk-close.pdf"));
        let texture = app.renderer.as_mut().unwrap().upload_rgba(1, 1, &[0, 0, 0, 255]).unwrap();
        let pdf = app.pdf_tab_mut(0).unwrap();
        pdf.textures.insert(0, PageTexture { tex: texture, width_px: 1, height_px: 1, r#gen: 1 });
        let doc = pdf.doc.unwrap();
        while rx.try_recv().is_ok() {}

        app.prepare_all_tabs_close();

        assert_eq!(app.pdf_textures_to_free.as_slice(), &[texture]);
        assert!(matches!(rx.try_recv(), Ok(PdfRequest::Close { id }) if id == doc));
    }
}
