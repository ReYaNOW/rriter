use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver};

use regex::Regex;

use crate::ui_waker::{UiWaker, WakeSender};

use super::pdfium_backend::{self, Doc};
use super::{DocGens, DocId, PageText, PdfEvent, PdfRequest, PdfWorkerHandle};

struct ActiveSearch {
    id: DocId,
    r#gen: u32,
    regex: Regex,
    next_page: usize,
}

pub fn start(lib_path: PathBuf, waker: &UiWaker) -> PdfWorkerHandle {
    let (tx, rx) = mpsc::channel();
    let (event_tx, event_rx) = waker.channel();
    let failed_tx = event_tx.clone();
    let result = std::thread::Builder::new()
        .name("pdf-worker".to_owned())
        .spawn(move || run(lib_path, rx, event_tx));
    if let Err(error) = result {
        let _ = failed_tx.send(PdfEvent::EngineFailed(error.to_string()));
    }
    PdfWorkerHandle { tx, rx: event_rx }
}

fn run(lib_path: PathBuf, rx: Receiver<PdfRequest>, event_tx: WakeSender<PdfEvent>) {
    let pdfium = match pdfium_backend::bind(&lib_path) {
        Ok(pdfium) => pdfium,
        Err(error) => {
            let _ = event_tx.send(PdfEvent::EngineFailed(error));
            return;
        }
    };
    let _ = event_tx.send(PdfEvent::EngineReady);
    let mut docs: HashMap<DocId, (Doc<'_>, Arc<DocGens>)> = HashMap::new();
    let mut text_cache: HashMap<(DocId, usize), Arc<PageText>> = HashMap::new();
    let mut active_search: Option<ActiveSearch> = None;
    loop {
        while let Ok(request) = rx.try_recv() {
            handle_request(
                &pdfium,
                request,
                &event_tx,
                &mut docs,
                &mut text_cache,
                &mut active_search,
            );
        }
        if active_search.is_some() {
            search_one_page(&event_tx, &docs, &mut text_cache, &mut active_search);
            continue;
        }
        match rx.recv() {
            Ok(request) => handle_request(
                &pdfium,
                request,
                &event_tx,
                &mut docs,
                &mut text_cache,
                &mut active_search,
            ),
            Err(_) => break,
        }
    }
}

fn handle_request<'a>(
    pdfium: &'a pdfium_render::prelude::Pdfium,
    request: PdfRequest,
    event_tx: &WakeSender<PdfEvent>,
    docs: &mut HashMap<DocId, (Doc<'a>, Arc<DocGens>)>,
    text_cache: &mut HashMap<(DocId, usize), Arc<PageText>>,
    active_search: &mut Option<ActiveSearch>,
) {
    match request {
        PdfRequest::Open { id, path, gens } => {
            match pdfium_backend::open(pdfium, &path) {
                Ok(doc) => {
                    text_cache.retain(|(cached_id, _), _| *cached_id != id);
                    let pages = doc.pages.clone();
                    docs.insert(id, (doc, gens));
                    let _ = event_tx.send(PdfEvent::Opened { id, pages });
                }
                Err(error) => {
                    let _ = event_tx.send(PdfEvent::OpenFailed { id, error });
                }
            }
        }
        PdfRequest::Close { id } => {
            docs.remove(&id);
            text_cache.retain(|(cached_id, _), _| *cached_id != id);
            if active_search.as_ref().is_some_and(|search| search.id == id) {
                *active_search = None;
            }
        }
        PdfRequest::Render { id, page, width_px, r#gen, dark } => {
            let Some((doc, gens)) = docs.get(&id) else {
                return;
            };
            if gens.closed.load(Ordering::Acquire) {
                return;
            }
            if r#gen != gens.render.load(Ordering::Acquire) || !gens.contains(page) {
                let _ = event_tx.send(PdfEvent::RenderSkipped { id, page, r#gen, error: None });
                return;
            }
            match pdfium_backend::render(doc, page, width_px, dark) {
                Ok((width_px, height_px, rgba)) => {
                    let _ = event_tx.send(PdfEvent::Page { id, page, r#gen, width_px, height_px, rgba });
                }
                Err(error) => {
                    let _ = event_tx.send(PdfEvent::RenderSkipped { id, page, r#gen, error: Some(error) });
                }
            }
        }
        PdfRequest::PageText { id, page } => {
            let Some((doc, gens)) = docs.get(&id) else {
                return;
            };
            if gens.closed.load(Ordering::Acquire) {
                return;
            }
            if let Some(text) = text_cache.get(&(id, page)) {
                let _ = event_tx.send(PdfEvent::Text { id, page, text: Arc::clone(text), links: Vec::new() });
                return;
            }
            match pdfium_backend::text(doc, page) {
                Ok((text, links)) => {
                    let text = Arc::new(text);
                    text_cache.insert((id, page), Arc::clone(&text));
                    let _ = event_tx.send(PdfEvent::Text { id, page, text, links });
                }
                Err(error) => {
                    text_cache.insert((id, page), Arc::new(PageText::default()));
                    let _ = event_tx.send(PdfEvent::TextFailed { id, page, error });
                }
            }
        }
        PdfRequest::Search { id, query, r#gen } => {
            *active_search = None;
            let Some((_, gens)) = docs.get(&id) else {
                let _ = event_tx.send(PdfEvent::SearchDone { id, r#gen });
                return;
            };
            if gens.closed.load(Ordering::Acquire) {
                return;
            }
            match Regex::new(&query) {
                Ok(regex) => {
                    *active_search = Some(ActiveSearch { id, r#gen, regex, next_page: 0 });
                }
                Err(_) => {
                    let _ = event_tx.send(PdfEvent::SearchDone { id, r#gen });
                }
            }
        }
    }
}

fn search_one_page(
    event_tx: &WakeSender<PdfEvent>,
    docs: &HashMap<DocId, (Doc<'_>, Arc<DocGens>)>,
    text_cache: &mut HashMap<(DocId, usize), Arc<PageText>>,
    active_search: &mut Option<ActiveSearch>,
) {
    let Some(search) = active_search.as_mut() else {
        return;
    };
    let Some((doc, gens)) = docs.get(&search.id) else {
        *active_search = None;
        return;
    };
    if gens.search.load(Ordering::Acquire) != search.r#gen || gens.closed.load(Ordering::Acquire) {
        *active_search = None;
        return;
    }
    if search.next_page >= doc.pages.len() {
        let _ = event_tx.send(PdfEvent::SearchDone { id: search.id, r#gen: search.r#gen });
        *active_search = None;
        return;
    }
    let page = search.next_page;
    search.next_page += 1;
    let text = text_cache.entry((search.id, page)).or_insert_with(|| {
        let text = pdfium_backend::text(doc, page)
            .map_or_else(|_| PageText::default(), |(text, _)| text);
        Arc::new(text)
    });
    let string = text.text();
    let char_starts = string.char_indices().map(|(byte, _)| byte).collect::<Vec<_>>();
    let matches = search
        .regex
        .find_iter(&string)
        .filter_map(|found| {
            if found.start() == found.end() {
                return None;
            }
            let start = char_starts.partition_point(|byte| *byte < found.start());
            let end = char_starts.partition_point(|byte| *byte < found.end());
            Some((start as u32, end as u32))
        })
        .collect::<Vec<_>>();
    if !matches.is_empty() {
        let _ = event_tx.send(PdfEvent::SearchPage { id: search.id, r#gen: search.r#gen, page, matches });
    }
    if search.next_page >= doc.pages.len() {
        let _ = event_tx.send(PdfEvent::SearchDone { id: search.id, r#gen: search.r#gen });
        *active_search = None;
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::mpsc::{Receiver, RecvTimeoutError};
    use std::time::Duration;

    use crate::ui_waker::UiWaker;

    use super::start;
    use crate::pdf::fixture::{write_empty, write_fixture_pdf, write_garbage};
    use crate::pdf::{DocGens, DocId, LinkTarget, PdfError, PdfEvent, PdfRequest};

    fn temp_dir() -> PathBuf {
        std::env::temp_dir().join(format!("rriter_pdf_{}", std::process::id()))
    }

    fn worker() -> (crate::pdf::PdfWorkerHandle, PathBuf) {
        let lib_path = std::env::var_os("RRITER_PDFIUM_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| panic!("RRITER_PDFIUM_PATH не задана: запускай через make test"));
        let waker = UiWaker::counting();
        let worker = start(lib_path, &waker);
        assert!(matches!(recv(&worker.rx), PdfEvent::EngineReady));
        (worker, temp_dir())
    }

    fn recv(rx: &Receiver<PdfEvent>) -> PdfEvent {
        rx.recv_timeout(Duration::from_secs(5)).unwrap()
    }

    fn open(worker: &crate::pdf::PdfWorkerHandle, id: DocId, path: &Path, gens: &Arc<DocGens>) -> Vec<crate::pdf::PageGeom> {
        worker.tx.send(PdfRequest::Open { id, path: path.to_path_buf(), gens: Arc::clone(gens) }).unwrap();
        match recv(&worker.rx) {
            PdfEvent::Opened { id: event_id, pages } => {
                assert_eq!(event_id, id);
                pages
            }
            event => panic!("expected Opened, got {event:?}"),
        }
    }

    #[test]
    fn opens_three_page_fixture_and_reads_text_and_links() {
        let (worker, dir) = worker();
        let path = write_fixture_pdf(&dir);
        let id = DocId(1);
        let gens = Arc::new(DocGens::new());
        let pages = open(&worker, id, &path, &gens);
        assert_eq!(pages.len(), 3);
        assert!(pages.iter().all(|page| page.width_pt == 612.0 && page.height_pt == 792.0));

        worker.tx.send(PdfRequest::PageText { id, page: 0 }).unwrap();
        match recv(&worker.rx) {
            PdfEvent::Text { text, links, .. } => {
                assert!(text.text().contains("Hello PDF viewer"));
                assert!((60.0..100.0).contains(&text.chars[0].rect.y));
                assert_eq!(links.len(), 3);
                assert!(links.iter().any(|link| link.target == LinkTarget::Uri("https://example.com/".to_owned())));
                assert!(links.iter().any(|link| link.target == LinkTarget::Page { page: 1, y_pt: Some(0.0) }));
                assert!(links.iter().any(|link| link.target == LinkTarget::Page { page: 2, y_pt: None }));
            }
            event => panic!("expected Text, got {event:?}"),
        }
        worker.tx.send(PdfRequest::PageText { id, page: 2 }).unwrap();
        assert!(matches!(recv(&worker.rx), PdfEvent::Text { text, .. } if text.chars.is_empty()));
    }

    #[test]
    fn renders_with_generation_window_and_dark_background() {
        let (worker, dir) = worker();
        let path = write_fixture_pdf(&dir);
        let id = DocId(2);
        let gens = Arc::new(DocGens::new());
        open(&worker, id, &path, &gens);
        gens.set_wanted(0, 2);
        for dark in [false, true] {
            worker.tx.send(PdfRequest::Render { id, page: 0, width_px: 306, r#gen: 0, dark }).unwrap();
            match recv(&worker.rx) {
                PdfEvent::Page { width_px, height_px, rgba, .. } => {
                    assert_eq!((width_px, height_px), (306, 396));
                    assert_eq!(rgba.len(), 306 * 396 * 4);
                    let expected = if dark { [30, 30, 30, 255] } else { [255, 255, 255, 255] };
                    assert_eq!(&rgba[..4], &expected);
                }
                event => panic!("expected Page, got {event:?}"),
            }
        }
        for (page, width_px, r#gen, has_error) in [(0, 306, 5, false), (3, 306, 0, false), (0, 0, 0, true)] {
            worker.tx.send(PdfRequest::Render { id, page, width_px, r#gen, dark: false }).unwrap();
            assert!(matches!(recv(&worker.rx), PdfEvent::RenderSkipped { error, .. } if error.is_some() == has_error));
        }
    }

    #[test]
    fn search_replies_and_invalid_documents_are_handled() {
        let (worker, dir) = worker();
        let path = write_fixture_pdf(&dir);
        let id = DocId(3);
        let gens = Arc::new(DocGens::new());
        open(&worker, id, &path, &gens);
        gens.search.store(1, std::sync::atomic::Ordering::Relaxed);
        worker.tx.send(PdfRequest::Search { id, query: "Second".to_owned(), r#gen: 1 }).unwrap();
        assert!(matches!(recv(&worker.rx), PdfEvent::SearchPage { page: 1, matches, .. } if matches.len() == 1));
        assert!(matches!(recv(&worker.rx), PdfEvent::SearchDone { .. }));
        worker.tx.send(PdfRequest::Search { id, query: "a*".to_owned(), r#gen: 1 }).unwrap();
        assert!(matches!(recv(&worker.rx), PdfEvent::SearchPage { page: 1, matches, .. } if !matches.is_empty() && matches.iter().all(|(start, end)| end > start)));
        assert!(matches!(recv(&worker.rx), PdfEvent::SearchDone { .. }));
        worker.tx.send(PdfRequest::Search { id, query: "(".to_owned(), r#gen: 1 }).unwrap();
        assert!(matches!(recv(&worker.rx), PdfEvent::SearchDone { .. }));

        for path in [write_garbage(&dir), write_empty(&dir)] {
            worker.tx.send(PdfRequest::Open { id: DocId(4), path, gens: Arc::new(DocGens::new()) }).unwrap();
            assert!(matches!(recv(&worker.rx), PdfEvent::OpenFailed { error: PdfError::Invalid(_), .. }));
        }
        worker.tx.send(PdfRequest::Open { id: DocId(4), path: dir.join("missing.pdf"), gens: Arc::new(DocGens::new()) }).unwrap();
        assert!(matches!(recv(&worker.rx), PdfEvent::OpenFailed { .. }));
    }

    #[test]
    fn closing_document_silences_requests_and_worker_stays_available() {
        let (worker, dir) = worker();
        let path = write_fixture_pdf(&dir);
        let id = DocId(5);
        let gens = Arc::new(DocGens::new());
        open(&worker, id, &path, &gens);
        gens.closed.store(true, std::sync::atomic::Ordering::Relaxed);
        worker.tx.send(PdfRequest::Close { id }).unwrap();
        worker.tx.send(PdfRequest::Render { id, page: 0, width_px: 100, r#gen: 0, dark: false }).unwrap();
        assert!(matches!(worker.rx.recv_timeout(Duration::from_millis(300)), Err(RecvTimeoutError::Timeout)));
        let reopened = write_fixture_pdf(&dir);
        assert_eq!(open(&worker, DocId(6), &reopened, &Arc::new(DocGens::new())).len(), 3);
    }
}
