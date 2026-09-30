//! Markdown reader media: images (file, http), SVG and Mermaid diagrams.
//!
//! This module owns the shared media types. Byte acquisition lives in `fetch`; later
//! submodules add decoding, the helper process and the texture cache.

use std::borrow::Cow;
use std::path::PathBuf;
use std::time::SystemTime;

use crate::platform::PathKey;

mod decode;
mod fetch;

/// Identity of one media item inside the cache.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) enum MediaKey {
    File(PathKey),
    Url(String),
    /// Hash of the Mermaid source.
    Mermaid(u64),
}

/// Where the bytes of a media item come from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MediaSource {
    File(PathBuf),
    Url(String),
    /// Mermaid diagram source text.
    Mermaid(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MediaKind {
    Raster,
    Svg,
    Mermaid,
}

#[derive(Clone, Debug)]
pub(crate) struct MediaRequest {
    pub key: MediaKey,
    pub source: MediaSource,
    /// Width of the text column in pixels; the raster result is shrunk to fit it.
    pub max_raster_w: u32,
    pub scale: f32,
}

/// Fingerprint of a local file taken at read time, used for revalidation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FileStamp {
    pub mtime: SystemTime,
    pub len: u64,
}

/// Decoded media: RGBA8, straight alpha. `stamp` is set for `File` sources.
#[derive(Debug)]
pub(crate) struct MediaPixels {
    pub natural_w: f32,
    pub natural_h: f32,
    pub raster_w: u32,
    pub raster_h: u32,
    pub rgba: Vec<u8>,
    pub stamp: Option<FileStamp>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MediaError {
    NotFound,
    Http(u16),
    Timeout,
    TooLarge,
    TooManyPixels,
    Decode,
    Unsupported,
    Mermaid(String),
    Crashed,
}

impl MediaError {
    /// Short Russian text for the placeholder frame.
    pub(crate) fn label(&self) -> Cow<'static, str> {
        match self {
            MediaError::NotFound => Cow::Borrowed("не найдено"),
            MediaError::Http(code) => Cow::Owned(format!("HTTP {code}")),
            MediaError::Timeout => Cow::Borrowed("таймаут"),
            MediaError::TooLarge => Cow::Borrowed("больше 20 МБ"),
            MediaError::TooManyPixels => Cow::Borrowed("слишком большая"),
            MediaError::Decode => Cow::Borrowed("не удалось декодировать"),
            MediaError::Unsupported => Cow::Borrowed("не поддерживается"),
            MediaError::Mermaid(text) => Cow::Owned(text.clone()),
            MediaError::Crashed => Cow::Borrowed("рендер упал"),
        }
    }
}

/// How to reach the decoding helper.
#[derive(Clone, Debug)]
pub(crate) enum RenderCommand {
    /// Spawn `exe` with `prefix_args` followed by the helper arguments.
    Process {
        exe: PathBuf,
        prefix_args: Vec<String>,
    },
    /// Decode in the calling thread (tests and headless fallbacks).
    InProcess,
}

/// Everything `fetch_bytes` needs from its owner.
#[derive(Clone)]
pub(crate) struct FetchEnv {
    pub cache_dir: PathBuf,
    pub http: reqwest::blocking::Client,
    pub max_bytes: u64,
    pub render: Option<RenderCommand>,
}
