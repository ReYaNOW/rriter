pub mod library;
pub mod worker;
mod pdfium_backend;
#[cfg(test)] pub mod fixture;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DocId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageGeom {
    pub width_pt: f32,
    pub height_pt: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PtRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageChar {
    pub ch: char,
    pub rect: PtRect,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageText {
    pub chars: Vec<PageChar>,
}

impl PageText {
    pub fn text(&self) -> String {
        self.chars.iter().map(|item| item.ch).collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum LinkTarget {
    Uri(String),
    Page { page: usize, y_pt: Option<f32> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct PageLink {
    pub rect: PtRect,
    pub target: LinkTarget,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PdfError {
    PasswordRequired,
    Invalid(String),
    Engine(String),
}

impl PdfError {
    pub fn message(&self) -> String {
        match self {
            Self::PasswordRequired => "документ защищён паролем".to_owned(),
            Self::Invalid(reason) => format!("не удалось открыть: {reason}"),
            Self::Engine(reason) => format!("ошибка движка: {reason}"),
        }
    }
}

#[derive(Debug)]
pub struct DocGens {
    pub render: AtomicU32,
    pub search: AtomicU32,
    pub closed: AtomicBool,
    pub wanted: AtomicU64,
}

impl DocGens {
    pub fn new() -> Self {
        Self {
            render: AtomicU32::new(0),
            search: AtomicU32::new(0),
            closed: AtomicBool::new(false),
            wanted: AtomicU64::new(u64::MAX),
        }
    }

    pub fn set_wanted(&self, first: usize, last: usize) {
        let encoded = ((first as u64) << 32) | (last as u64 & u64::from(u32::MAX));
        self.wanted.store(encoded, Ordering::Relaxed);
    }

    pub fn clear_wanted(&self) {
        self.wanted.store(u64::MAX, Ordering::Relaxed);
    }

    pub fn wanted(&self) -> Option<(usize, usize)> {
        let encoded = self.wanted.load(Ordering::Relaxed);
        (encoded != u64::MAX).then(|| {
            (
                (encoded >> 32) as usize,
                (encoded & u64::from(u32::MAX)) as usize,
            )
        })
    }

    pub fn contains(&self, page: usize) -> bool {
        self.wanted()
            .is_some_and(|(first, last)| (first..=last).contains(&page))
    }
}

impl Default for DocGens {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub enum PdfRequest {
    Open { id: DocId, path: PathBuf, gens: Arc<DocGens> },
    Close { id: DocId },
    Render { id: DocId, page: usize, width_px: u32, r#gen: u32, dark: bool },
    PageText { id: DocId, page: usize },
    Search { id: DocId, query: String, r#gen: u32 },
}

#[derive(Debug)]
pub enum PdfEvent {
    EngineReady,
    EngineFailed(String),
    LoadStarted,
    EngineUnavailable(String),
    Opened { id: DocId, pages: Vec<PageGeom> },
    OpenFailed { id: DocId, error: PdfError },
    Page { id: DocId, page: usize, r#gen: u32, width_px: u32, height_px: u32, rgba: Vec<u8> },
    RenderSkipped { id: DocId, page: usize, r#gen: u32, error: Option<String> },
    Text { id: DocId, page: usize, text: Arc<PageText>, links: Vec<PageLink> },
    TextFailed { id: DocId, page: usize, error: String },
    SearchPage { id: DocId, r#gen: u32, page: usize, matches: Vec<(u32, u32)> },
    SearchDone { id: DocId, r#gen: u32 },
}

pub struct PdfWorkerHandle {
    pub tx: Sender<PdfRequest>,
    pub rx: Receiver<PdfEvent>,
}

#[cfg(test)]
mod tests {
    use super::{DocGens, PdfError};

    #[test]
    fn wanted_window_encodes_empty_and_page_zero() {
        let gens = DocGens::new();
        assert_eq!(gens.wanted(), None);

        gens.set_wanted(0, 0);
        assert_eq!(gens.wanted(), Some((0, 0)));
        assert!(gens.contains(0));
        assert!(!gens.contains(1));

        gens.clear_wanted();
        assert_eq!(gens.wanted(), None);
    }

    #[test]
    fn password_error_is_localized() {
        assert!(PdfError::PasswordRequired.message().contains("паролем"));
    }
}
