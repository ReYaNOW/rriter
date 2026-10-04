use super::{PdfEngineState, PdfEventOutcome, PdfLibrarySource, PdfPhase, PdfTabState};
use crate::app::tool_installer::ToolInstallFinish;
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
        let located = match &self.pdf_library_source {
            PdfLibrarySource::EnvValue(explicit) => crate::pdf::library::locate_with(explicit.clone()),
            PdfLibrarySource::ProcessEnv => crate::pdf::library::locate(),
        };
        match located {
            crate::pdf::library::LocateResult::Found(path) => {
                self.pdf_worker = Some(crate::pdf::worker::start(path, &self.ui_waker));
                self.pdf_engine = PdfEngineState::Starting;
            }
            crate::pdf::library::LocateResult::NotInstalled => {
                self.pdf_engine = PdfEngineState::NotInstalled;
            }
            crate::pdf::library::LocateResult::Missing { message, installable } => {
                self.pdf_engine = PdfEngineState::Missing { message, installable };
            }
        }
    }

    /// Handler of the "download engine" button. Only a `NotInstalled` or `Missing { installable: true }`
    /// engine starts a download, so a second click while `Installing` is ignored. No automatic retry.
    pub fn install_pdf_engine(&mut self) {
        let prev = match &self.pdf_engine {
            PdfEngineState::NotInstalled => String::new(),
            PdfEngineState::Missing { message, installable: true } => message.clone(),
            _ => return,
        };
        self.pdf_engine = if self.tool_installer.is_running() {
            PdfEngineState::Missing { message: "идёт установка другого инструмента".to_owned(), installable: true }
        } else {
            match self.tool_installer.start_pdfium_install(&self.ui_waker) {
                Ok(()) => PdfEngineState::Installing { prev, progress: String::new() },
                Err(message) => PdfEngineState::Missing { message, installable: true },
            }
        };
        if let Some(window) = self.window.as_ref() { window.request_redraw(); }
    }

    /// Handler of the "cancel" button; the state returns to `Missing` when the worker reports it.
    pub fn cancel_pdf_engine_install(&mut self) {
        if matches!(self.pdf_engine, PdfEngineState::Installing { .. })
            && self.tool_installer.is_running_for(crate::platform::ToolKind::Pdfium)
        {
            self.tool_installer.cancel();
        }
    }

    /// Applies one installer poll to `pdf_engine` while it is `Installing`; true when it changed.
    pub(crate) fn sync_pdf_engine_install(&mut self, finish: Option<ToolInstallFinish>) -> bool {
        if !matches!(self.pdf_engine, PdfEngineState::Installing { .. }) { return false; }
        match finish {
            None => {
                let last = self.tool_installer.logs().last().map(|line| line.text.as_str()).unwrap_or_default();
                match &mut self.pdf_engine {
                    PdfEngineState::Installing { progress, .. } if progress.as_str() != last => {
                        *progress = last.to_owned();
                        true
                    }
                    _ => false,
                }
            }
            Some(ToolInstallFinish::Cancelled) => {
                // The previous state moves out of `Installing`; nothing is cloned per poll.
                let prev = match std::mem::replace(&mut self.pdf_engine, PdfEngineState::NotStarted) {
                    PdfEngineState::Installing { prev, .. } => prev,
                    _ => String::new(),
                };
                self.pdf_engine = if prev.is_empty() {
                    PdfEngineState::NotInstalled
                } else {
                    PdfEngineState::Missing { message: prev, installable: true }
                };
                true
            }
            Some(ToolInstallFinish::Failed(message)) => {
                self.pdf_engine = PdfEngineState::Missing { message, installable: true };
                true
            }
            Some(ToolInstallFinish::Installed(outcome)) => {
                let installed = outcome.paths.first().map(|(_, path)| path.clone()).unwrap_or_default();
                self.pdf_engine = PdfEngineState::NotStarted;
                self.ensure_pdf_engine();
                let located = std::mem::replace(&mut self.pdf_engine, PdfEngineState::NotStarted);
                self.pdf_engine = engine_after_install(located, &installed);
                true
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
            PdfEngineState::NotInstalled => (
                PdfPhase::EngineMissing { error: Some(crate::pdf::library::NOT_FOUND_MESSAGE.to_owned()) },
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
            image: None,
            scroll_y: crate::scroll::ScrollState::new(15.0),
            scroll_x: crate::scroll::ScrollState::new(15.0),
            spans: Vec::new(), completions: Vec::new(), foldable_ranges: Vec::new(),
            syntax_errors: Vec::new(), last_sent_version: u64::MAX,
            search_results: Vec::new(), search_current_idx: None,
            is_highlighted_once: false, is_highlight_complete: false,
            icon_key, closing_hints: Default::default(), kind: EditorTabKind::Pdf, deleted: false,
            load: crate::app::TabLoad::Loaded,
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
            // A bind failure ends the worker right after reporting it; keep that reason instead of overwriting it.
            let already_failed = matches!(self.pdf_engine, PdfEngineState::Failed(_))
                || events.iter().any(|event| matches!(event, PdfEvent::EngineFailed(_)));
            if !already_failed {
                events.push(PdfEvent::EngineFailed("поток движка завершился".to_owned()));
            }
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
                    if let PdfEvent::Page { page, r#gen, .. } = &other && active_doc != Some(id) {
                        // The bitmap of an inactive tab is dropped, but its in-flight slot must not
                        // stay set: the page would never be requested again after the tab returns.
                        let slot = (*page, *r#gen);
                        if let Some(pdf) = self.tabs.iter_mut().find_map(|tab| tab.pdf.as_deref_mut().filter(|pdf| pdf.doc == Some(id))) {
                            pdf.requested.remove(&slot);
                        }
                        continue;
                    }
                    let Some(tab) = self.tabs.iter_mut().find(|tab| tab.pdf.as_ref().is_some_and(|pdf| pdf.doc == Some(id))) else { continue };
                    let is_text = matches!(other, PdfEvent::Text { .. } | PdfEvent::TextFailed { .. });
                    let is_opened = matches!(other, PdfEvent::Opened { .. });
                    let mut copied = None;
                    if let Some(pdf) = tab.pdf.as_mut() {
                        // Owned: a `Page` bitmap (up to 64 MB) is moved, not copied.
                        redraw |= matches!(pdf.apply_event_owned(other), PdfEventOutcome::Redraw | PdfEventOutcome::Bitmap);
                        if is_text { copied = pdf.take_copy_text(); }
                    }
                    if let Some(text) = copied { self.set_clipboard_text(text); }
                    if is_opened && active_doc == Some(id) { self.pdf_restart_search_if_open(); }
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
            if pump_page_uploads(tab, renderer, &wanted, textures_to_free, PDF_UPLOAD_BYTES_PER_FRAME) {
                crate::platform::trim_allocator();
            }
            if (!tab.pending_bitmaps.is_empty() || tab.upload.is_some()) && let Some(window) = window {
                window.request_redraw();
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
                textures_to_free.extend(pdf.upload.take().map(|upload| upload.tex));
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
            textures_to_free.extend(pdf.upload.take().map(|upload| upload.tex));
        }
    }

    pub fn pdf_tab_activated(&mut self, idx: usize) {
        if let Some(pdf) = self.pdf_tab_mut(idx) { pdf.layout_dirty = true; }
        self.pdf_restart_search_if_open();
    }

    pub fn toggle_pdf_dark_pages(&mut self) {
        self.set_pdf_dark_pages(!self.pdf_dark_pages);
        self.save_current_config();
        if let Some(window) = self.window.as_ref() { window.request_redraw(); }
    }

    pub(crate) fn set_pdf_dark_pages(&mut self, on: bool) {
        if self.pdf_dark_pages == on {
            return;
        }
        self.pdf_dark_pages = on;
        for tab in &self.tabs {
            if let Some(pdf) = tab.pdf.as_ref() { pdf.bump_render_gen(); }
        }
    }
}

/// Bytes of page pixels copied to the GPU per frame. A whole page in one `tex_image_2d`
/// (5–16 MB) cost 5–12 ms of a 4.17 ms frame at 240 Hz; strips keep a frame near 1.5 ms
/// and a typical page still lands within 2–3 frames.
const PDF_UPLOAD_BYTES_PER_FRAME: usize = 3 * 1024 * 1024;

/// Copies pending page bitmaps into textures, at most `budget` bytes of rows per call.
/// A texture is published to `tab.textures` only when all its rows are written, so a page
/// never shows half-uploaded; until then the old texture (previous generation) stays drawn.
/// Returns whether a large bitmap finished (its memory is released, worth trimming the heap).
fn pump_page_uploads(
    tab: &mut PdfTabState,
    renderer: &mut crate::renderer::Renderer,
    wanted: &std::ops::Range<usize>,
    textures_to_free: &mut Vec<glow::Texture>,
    mut budget: usize,
) -> bool {
    let render_gen = tab.gens.render.load(std::sync::atomic::Ordering::Relaxed);
    if tab.upload.as_ref().is_some_and(|upload| !wanted.contains(&upload.bitmap.page) || upload.bitmap.r#gen != render_gen) {
        textures_to_free.extend(tab.upload.take().map(|upload| upload.tex));
    }
    let mut finished_large = false;
    while budget > 0 {
        if tab.upload.is_none() {
            if tab.pending_bitmaps.is_empty() { break; }
            let bitmap = tab.pending_bitmaps.remove(0);
            if !wanted.contains(&bitmap.page) || bitmap.r#gen != render_gen || bitmap.width_px == 0 || bitmap.height_px == 0
                || (bitmap.width_px as usize).checked_mul(bitmap.height_px as usize).and_then(|size| size.checked_mul(4)) != Some(bitmap.rgba.len()) { continue; }
            let Some(tex) = renderer.create_rgba_texture(bitmap.width_px, bitmap.height_px) else {
                tab.requested.insert((bitmap.page, bitmap.r#gen));
                continue;
            };
            tab.upload = Some(super::PageUpload { bitmap, tex, next_row: 0 });
        }
        let Some(upload) = tab.upload.as_mut() else { break };
        let (width, height) = (upload.bitmap.width_px, upload.bitmap.height_px);
        let row_bytes = width as usize * 4;
        let rows = (budget / row_bytes).min((height - upload.next_row) as usize) as u32;
        if rows == 0 { break; }
        let start = upload.next_row as usize * row_bytes;
        let Some(strip) = upload.bitmap.rgba.get(start..start + rows as usize * row_bytes) else {
            textures_to_free.extend(tab.upload.take().map(|upload| upload.tex));
            continue;
        };
        renderer.upload_rgba_rows(upload.tex, width, upload.next_row, rows, strip);
        upload.next_row += rows;
        budget -= rows as usize * row_bytes;
        if upload.next_row >= height && let Some(done) = tab.upload.take() {
            finished_large |= done.bitmap.rgba.len() >= 1024 * 1024;
            let texture = super::PageTexture { tex: done.tex, width_px: width, height_px: height, r#gen: done.bitmap.r#gen };
            if let Some(old) = tab.textures.insert(done.bitmap.page, texture) {
                textures_to_free.push(old.tex);
            }
        }
    }
    finished_large
}

/// A finished install whose library `locate()` still cannot find must not loop back into a
/// download: it becomes an explicit `Missing` the user can retry by hand.
pub(crate) fn engine_after_install(located: PdfEngineState, installed: &std::path::Path) -> PdfEngineState {
    match located {
        PdfEngineState::Missing { .. } | PdfEngineState::NotInstalled => PdfEngineState::Missing {
            message: format!("библиотека установлена, но не найдена: {}", installed.display()),
            installable: true,
        },
        other => other,
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

    #[test]
    fn page_upload_is_split_across_frames_and_published_only_when_complete() {
        let (_context, mut app) =
            crate::platform::offscreen_gl::test_support::offscreen_test_app(64, 64, 1.0);
        let mut renderer = app.renderer.take().unwrap();
        let mut tab = PdfTabState::new(PathBuf::from("strips.pdf"), Arc::new(DocGens::new()), PdfPhase::Ready);
        let r#gen = tab.gens.render.load(std::sync::atomic::Ordering::Relaxed);
        let bitmap = |page| crate::app::pdf_tab::PendingBitmap { page, r#gen, width_px: 4, height_px: 8, rgba: vec![9; 4 * 8 * 4] };
        tab.pending_bitmaps.push(bitmap(0));
        tab.pending_bitmaps.push(bitmap(1));
        let mut freed = Vec::new();
        // 3 rows of 16 bytes per frame: 8 rows need 3 frames.
        for _ in 0..2 {
            assert!(!pump_page_uploads(&mut tab, &mut renderer, &(0..2), &mut freed, 48));
            assert!(tab.textures.is_empty(), "a half-written page must not be drawn");
            assert_eq!(tab.upload.as_ref().map(|upload| upload.bitmap.page), Some(0));
        }
        pump_page_uploads(&mut tab, &mut renderer, &(0..2), &mut freed, 48);
        assert_eq!(tab.textures.get(&0).map(|texture| (texture.width_px, texture.height_px)), Some((4, 8)));
        // The leftover budget of the finishing frame already starts page 1.
        assert_eq!(tab.upload.as_ref().map(|upload| (upload.bitmap.page, upload.next_row)), Some((1, 1)));
        // Page 1 scrolled out of the wanted range: its partial texture is freed, nothing is published.
        pump_page_uploads(&mut tab, &mut renderer, &(0..1), &mut freed, 48);
        assert!(tab.upload.is_none());
        assert_eq!(freed.len(), 1);
        assert!(!tab.textures.contains_key(&1));
        for texture in freed.into_iter().chain(tab.textures.drain().map(|(_, texture)| texture.tex)) {
            renderer.delete_texture(texture);
        }
    }

    #[test]
    fn page_bitmap_for_an_inactive_tab_frees_its_slot_so_the_page_is_requested_again_on_return() {
        let (_context, mut app) =
            crate::platform::offscreen_gl::test_support::offscreen_test_app(64, 64, 1.0);
        app.is_ide_mode = true;
        app.pdf_engine = PdfEngineState::Ready;
        let (tx, _rx) = std::sync::mpsc::channel();
        let (event_tx, event_rx) = std::sync::mpsc::channel();
        app.pdf_worker = Some(crate::pdf::PdfWorkerHandle { tx, rx: event_rx });
        app.open_pdf_tab(PathBuf::from("inactive-a.pdf"));
        let doc = app.pdf_tab_mut(0).unwrap().doc.unwrap();
        let geom = crate::pdf::PageGeom { width_pt: 612.0, height_pt: 792.0 };
        event_tx.send(PdfEvent::Opened { id: doc, pages: vec![geom; 2] }).unwrap();
        assert!(app.poll_pdf_worker());
        let requests = {
            let pdf = app.pdf_tab_mut(0).unwrap();
            pdf.set_viewport(400, 300, 1.0);
            pdf.take_render_requests(false)
        };
        let Some(PdfRequest::Render { page, r#gen, .. }) = requests.first() else { panic!("no render request: {}", requests.len()) };
        let slot = (*page, *r#gen);

        app.open_pdf_tab(PathBuf::from("inactive-b.pdf"));
        assert_eq!(app.active_tab, 1);
        // The render finished after the switch; model the request still being recorded as in flight.
        app.pdf_tab_mut(0).unwrap().requested.insert(slot);
        event_tx.send(PdfEvent::Page { id: doc, page: slot.0, r#gen: slot.1, width_px: 1, height_px: 1, rgba: vec![0; 4] }).unwrap();
        app.poll_pdf_worker();
        assert!(app.pdf_tab_mut(0).unwrap().requested.is_empty(), "the dropped bitmap must free its in-flight slot");
        assert!(app.pdf_tab_mut(0).unwrap().pending_bitmaps.is_empty());

        app.switch_to_tab(0);
        let again = {
            let pdf = app.pdf_tab_mut(0).unwrap();
            pdf.set_viewport(400, 300, 1.0);
            pdf.take_render_requests(false)
        };
        assert!(again.iter().any(|request| matches!(request, PdfRequest::Render { page, .. } if *page == slot.0)), "page must be re-requested after the tab returns");
    }

    #[test]
    fn bind_failure_reason_survives_worker_disconnect() {
        let (_context, mut app) =
            crate::platform::offscreen_gl::test_support::offscreen_test_app(64, 64, 1.0);
        app.is_ide_mode = true;
        app.pdf_engine = PdfEngineState::Starting;
        let (tx, _rx) = std::sync::mpsc::channel();
        let (event_tx, event_rx) = std::sync::mpsc::channel();
        app.pdf_worker = Some(crate::pdf::PdfWorkerHandle { tx, rx: event_rx });
        app.open_pdf_tab(PathBuf::from("bind-failure.pdf"));
        // The worker reports the bind error and exits: the sender is gone before the UI polls.
        event_tx.send(PdfEvent::EngineFailed("bind: library validation failed".to_owned())).unwrap();
        drop(event_tx);

        assert!(app.poll_pdf_worker());
        assert!(app.pdf_worker.is_none());
        assert!(matches!(&app.pdf_engine, PdfEngineState::Failed(message) if message == "bind: library validation failed"));
        let phase = app.pdf_tab_mut(0).unwrap().phase.clone();
        assert!(matches!(&phase, PdfPhase::EngineMissing { error: Some(error) } if error.contains("bind: library validation failed")), "{phase:?}");
        // Later polls without a worker must not invent a second failure either.
        app.poll_pdf_worker();
        assert!(matches!(&app.pdf_engine, PdfEngineState::Failed(message) if message == "bind: library validation failed"));
    }
}
