//! Markdown reader media: images (file, http), SVG and Mermaid diagrams.
//!
//! This module owns the shared media types and `MarkdownMedia`, the texture cache. Byte
//! acquisition lives in `fetch`, decoding in `decode` and the helper process, the pure
//! queue/budget decisions in `texture_budget`.

use std::borrow::Cow;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use crate::platform::PathKey;
use crate::renderer::Renderer;
use crate::ui_waker::{OneShot, OneShotState, UiWaker};

mod decode;
mod fetch;
mod render_helper;
mod texture_budget;
#[cfg(test)]
mod tests;

use fetch::{load_media, trim_disk_cache};
use texture_budget::{
    BudgetItem, QueueFacts, INVISIBLE_TEXTURE_BUDGET, enqueue_indices, eviction_plan,
    needs_rerender, should_trim_disk_cache, texture_totals, upload_order,
};

pub(crate) use render_helper::run_media_helper_if_requested;

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

/// At most this many `load_media` tasks run at once (= helper processes). A slot stays
/// taken until the result is uploaded to the GPU or discarded.
const MAX_SLOTS: usize = 3;
const MAX_UPLOADS_PER_FRAME: usize = 2;
const DISK_CACHE_LIMIT_BYTES: u64 = 200 * 1024 * 1024;
const MAX_FETCH_BYTES: u64 = 20 * 1024 * 1024;
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);

type LoadResult = Result<MediaPixels, MediaError>;
type Loader = Arc<dyn Fn(&MediaRequest) -> LoadResult + Send + Sync>;

/// What the cache knows about a key, as the layout sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum MediaEntryView<'a> {
    Unknown,
    Pending { natural: Option<(f32, f32)> },
    Ready { natural_w: f32, natural_h: f32, texture: Option<&'a glow::Texture> },
    Failed(&'a MediaError),
}

/// A key drawn in the current frame and the size it is drawn at, in pixels.
#[derive(Clone, Debug)]
pub(crate) struct VisibleMedia {
    pub key: MediaKey,
    pub display_w: u32,
    pub display_h: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct MediaStats {
    /// `load_media` tasks started during the life of the cache.
    pub loads_started: u64,
    /// Sum of `w * h * 4` over live textures.
    pub texture_bytes: u64,
    /// The same over the keys visible in the last `prepare_gpu`.
    pub visible_texture_bytes: u64,
}

enum EntryState {
    Pending,
    Ready,
    Failed(MediaError),
}

struct Entry {
    natural: Option<(f32, f32)>,
    state: EntryState,
    texture: Option<(glow::Texture, u32, u32)>,
    last_visible_frame: u64,
    /// A task is running or its pixels wait for the upload; the slot is held meanwhile.
    in_flight: bool,
    /// Unique per entry; a task result carrying another value is stale and dropped.
    generation: u64,
    stamp: Option<FileStamp>,
    /// Last request, reused when the texture is evicted or has to be re-rendered.
    request: MediaRequest,
    queued: bool,
    /// Decoded pixels between the end of the task and the upload.
    pending: Option<MediaPixels>,
    /// Display width of the last re-render, so one size is never re-rendered twice in a row.
    rerendered_for: Option<u32>,
}

struct Task {
    key: MediaKey,
    generation: u64,
    job: OneShot<LoadResult>,
}

/// The GPU side of `prepare_gpu`; `Renderer` in the app, a fake in tests.
trait TextureHost {
    fn upload_rgba(&mut self, w: u32, h: u32, rgba: &[u8]) -> Option<glow::Texture>;
    fn delete_texture(&mut self, texture: glow::Texture);
}

impl TextureHost for Renderer {
    fn upload_rgba(&mut self, w: u32, h: u32, rgba: &[u8]) -> Option<glow::Texture> {
        Renderer::upload_rgba(self, w, h, rgba)
    }

    fn delete_texture(&mut self, texture: glow::Texture) {
        Renderer::delete_texture(self, texture)
    }
}

/// Media cache shared by all tabs: natural sizes, states, GPU textures and the background
/// loads behind them. Frames only read it; all I/O and decoding run in tasks.
pub(crate) struct MarkdownMedia {
    loader: Loader,
    /// False when no helper can run: every request fails at once with `Unsupported`.
    supported: bool,
    disk_cache_dir: Option<PathBuf>,
    entries: HashMap<MediaKey, Entry>,
    queue: VecDeque<MediaKey>,
    tasks: Vec<Task>,
    /// Keys whose pixels wait for the upload, in arrival order; each holds a slot.
    pending_uploads: Vec<MediaKey>,
    to_free: Vec<glow::Texture>,
    revalidation: Option<OneShot<Vec<(PathKey, u64)>>>,
    trim_task: Option<OneShot<()>>,
    media_gen: u64,
    frame: u64,
    next_generation: u64,
    loads_started: u64,
    url_loads_started: u64,
}

impl MarkdownMedia {
    pub(crate) fn new(env: FetchEnv) -> Self {
        let supported = env.render.is_some();
        let cache_dir = env.cache_dir.clone();
        let mut media = Self::with_loader(Arc::new(move |req| load_media(req, &env)));
        media.supported = supported;
        media.disk_cache_dir = Some(cache_dir);
        media
    }

    /// The application cache: one http client (15 s timeout), the on-disk cache under the
    /// platform cache directory and the current executable as the helper.
    pub(crate) fn from_platform() -> Self {
        let client = crate::platform::blocking_http_client_builder()
            .timeout(HTTP_TIMEOUT)
            .build();
        match client {
            Ok(http) => Self::new(FetchEnv {
                cache_dir: crate::platform::cache_dir().join("markdown-images"),
                http,
                max_bytes: MAX_FETCH_BYTES,
                render: RenderCommand::for_current_process(),
            }),
            Err(_) => {
                let mut media = Self::with_loader(Arc::new(|_| Err(MediaError::Unsupported)));
                media.supported = false;
                media
            }
        }
    }

    pub(crate) fn with_loader(loader: Loader) -> Self {
        Self {
            loader,
            supported: true,
            disk_cache_dir: None,
            entries: HashMap::new(),
            queue: VecDeque::new(),
            tasks: Vec::new(),
            pending_uploads: Vec::new(),
            to_free: Vec::new(),
            revalidation: None,
            trim_task: None,
            media_gen: 0,
            frame: 0,
            next_generation: 0,
            loads_started: 0,
            url_loads_started: 0,
        }
    }

    pub(crate) fn media_gen(&self) -> u64 {
        self.media_gen
    }

    pub(crate) fn entry(&self, key: &MediaKey) -> MediaEntryView<'_> {
        let Some(entry) = self.entries.get(key) else {
            return MediaEntryView::Unknown;
        };
        match (&entry.state, entry.natural) {
            (EntryState::Failed(error), _) => MediaEntryView::Failed(error),
            (EntryState::Ready, Some((natural_w, natural_h))) => MediaEntryView::Ready {
                natural_w,
                natural_h,
                texture: entry.texture.as_ref().map(|(texture, _, _)| texture),
            },
            (_, natural) => MediaEntryView::Pending { natural },
        }
    }

    pub(crate) fn stats(&self) -> MediaStats {
        let items: Vec<BudgetItem> = self
            .entries
            .values()
            .map(|entry| self.budget_item(entry))
            .collect();
        let (texture_bytes, visible_texture_bytes) = texture_totals(&items);
        MediaStats { loads_started: self.loads_started, texture_bytes, visible_texture_bytes }
    }

    /// Asks for a key. Unknown keys become `Pending` and join the queue. A known key only
    /// stores the latest request (scale, column width), so the next render and the re-render
    /// check use the current target; it is queued again by `prepare_gpu` when it is visible,
    /// never from here (an evicted invisible image must not be rendered again).
    pub(crate) fn request(&mut self, req: MediaRequest, waker: &UiWaker) {
        if let Some(entry) = self.entries.get_mut(&req.key) {
            // A queued re-render already carries the width it was queued for.
            let rerender_width = (entry.queued && entry.texture.is_some()).then_some(entry.request.max_raster_w);
            entry.request = req;
            if let Some(width) = rerender_width {
                entry.request.max_raster_w = width;
            }
        } else if self.supported {
            let key = req.key.clone();
            let mut entry = self.new_entry(req);
            entry.queued = true;
            self.entries.insert(key.clone(), entry);
            self.queue.push_back(key);
        } else {
            let key = req.key.clone();
            let mut entry = self.new_entry(req);
            entry.state = EntryState::Failed(MediaError::Unsupported);
            self.entries.insert(key, entry);
            self.media_gen += 1;
        }
        self.start_tasks(waker);
    }

    /// Collects finished tasks, then starts queued ones. True when `media_gen` changed.
    pub(crate) fn poll(&mut self, waker: &UiWaker) -> bool {
        let before = self.media_gen;
        for mut task in std::mem::take(&mut self.tasks) {
            match task.job.poll() {
                OneShotState::Pending => self.tasks.push(task),
                OneShotState::Ready(result) => self.finish_task(&task.key, task.generation, result),
                OneShotState::Closed => {
                    self.finish_task(&task.key, task.generation, Err(MediaError::Crashed))
                }
            }
        }
        self.poll_revalidation();
        if let Some(job) = self.trim_task.as_mut()
            && !matches!(job.poll(), OneShotState::Pending)
        {
            self.trim_task = None;
        }
        self.start_tasks(waker);
        self.media_gen != before
    }

    /// Frame step on the GPU side. True while another frame is needed: pixels wait for the
    /// upload or a visible key is still loading.
    pub(crate) fn prepare_gpu(
        &mut self,
        renderer: &mut Renderer,
        visible: &[VisibleMedia],
        waker: &UiWaker,
    ) -> bool {
        self.prepare_with(renderer, visible, waker)
    }

    fn prepare_with(&mut self, host: &mut dyn TextureHost, visible: &[VisibleMedia], waker: &UiWaker) -> bool {
        for texture in self.to_free.drain(..) {
            host.delete_texture(texture);
        }
        self.frame += 1;
        for item in visible {
            if let Some(entry) = self.entries.get_mut(&item.key) {
                entry.last_visible_frame = self.frame;
            }
        }
        self.upload_pending(host);
        self.queue_visible(visible);
        self.evict_over_budget();
        self.start_tasks(waker);
        !self.pending_uploads.is_empty()
            || visible
                .iter()
                .any(|item| self.entries.get(&item.key).is_some_and(|entry| entry.in_flight))
    }

    /// Drops the entry of a changed or vanished file. A task still running for it is
    /// discarded when it reports back, so the entry does not come back.
    pub(crate) fn invalidate_path(&mut self, path: &PathKey) {
        let key = MediaKey::File(path.clone());
        let Some(entry) = self.entries.remove(&key) else {
            return;
        };
        if let Some((texture, _, _)) = entry.texture {
            self.to_free.push(texture);
        }
        self.queue.retain(|queued| *queued != key);
        self.pending_uploads.retain(|pending| *pending != key);
        self.media_gen += 1;
    }

    /// Starts one background `stat` pass over the loaded `File` keys among `keys`; changed
    /// and vanished files are invalidated by `poll`. Does nothing while a pass is running.
    pub(crate) fn revalidate_files(&mut self, keys: &[MediaKey], waker: &UiWaker) {
        if self.revalidation.as_ref().is_some_and(OneShot::is_pending) {
            return;
        }
        let mut checks: Vec<(PathKey, PathBuf, FileStamp, u64)> = Vec::new();
        for key in keys {
            let MediaKey::File(path_key) = key else {
                continue;
            };
            let Some(entry) = self.entries.get(key) else {
                continue;
            };
            if let (Some(stamp), MediaSource::File(path)) = (entry.stamp, &entry.request.source) {
                checks.push((path_key.clone(), path.clone(), stamp, entry.generation));
            }
        }
        if checks.is_empty() {
            return;
        }
        self.revalidation = waker
            .spawn_one_shot("markdown-media-stat", move || {
                checks
                    .into_iter()
                    .filter(|(_, path, stamp, _)| file_changed(path, stamp))
                    .map(|(path_key, _, _, generation)| (path_key, generation))
                    .collect()
            })
            .ok();
    }

    /// Forgets failures of `keys` so the next layout asks for them again.
    pub(crate) fn reset_failed(&mut self, keys: &[MediaKey]) {
        for key in keys {
            if self
                .entries
                .get(key)
                .is_some_and(|entry| matches!(entry.state, EntryState::Failed(_)))
            {
                self.entries.remove(key);
                self.media_gen += 1;
            }
        }
    }

    fn new_entry(&mut self, request: MediaRequest) -> Entry {
        self.next_generation += 1;
        Entry {
            natural: None,
            state: EntryState::Pending,
            texture: None,
            last_visible_frame: 0,
            in_flight: false,
            generation: self.next_generation,
            stamp: None,
            request,
            queued: false,
            pending: None,
            rerendered_for: None,
        }
    }

    fn queue_facts(entry: &Entry, visible: bool) -> QueueFacts {
        QueueFacts {
            visible,
            has_texture: entry.texture.is_some(),
            in_flight: entry.in_flight,
            failed: matches!(entry.state, EntryState::Failed(_)),
            queued: entry.queued,
        }
    }

    fn is_visible(&self, entry: &Entry) -> bool {
        self.frame > 0 && entry.last_visible_frame == self.frame
    }

    fn budget_item(&self, entry: &Entry) -> BudgetItem {
        BudgetItem {
            bytes: entry.texture.map_or(0, |(_, w, h)| u64::from(w) * u64::from(h) * 4),
            visible: self.is_visible(entry),
            last_visible_frame: entry.last_visible_frame,
        }
    }

    fn slots_used(&self) -> usize {
        self.tasks.len() + self.pending_uploads.len()
    }

    /// Next key to load: the first visible one in queue order, else the oldest.
    fn pop_next_to_start(&mut self) -> Option<MediaKey> {
        let entries = &self.entries;
        self.queue
            .retain(|key| entries.get(key).is_some_and(|entry| entry.queued && !entry.in_flight));
        let position = self
            .queue
            .iter()
            .position(|key| entries.get(key).is_some_and(|entry| self.is_visible(entry)))
            .unwrap_or(0);
        self.queue.remove(position)
    }

    fn start_tasks(&mut self, waker: &UiWaker) {
        while self.slots_used() < MAX_SLOTS {
            let Some(key) = self.pop_next_to_start() else {
                break;
            };
            let Some(entry) = self.entries.get_mut(&key) else {
                continue;
            };
            entry.queued = false;
            entry.in_flight = true;
            let request = entry.request.clone();
            let generation = entry.generation;
            if matches!(request.source, MediaSource::Url(_)) {
                self.maybe_trim_disk_cache(waker);
            }
            let loader = Arc::clone(&self.loader);
            match waker.spawn_one_shot("markdown-media-load", move || loader(&request)) {
                Ok(job) => {
                    self.loads_started += 1;
                    self.tasks.push(Task { key, generation, job });
                }
                Err(_) => {
                    if let Some(entry) = self.entries.get_mut(&key) {
                        fail_entry(entry, MediaError::Crashed, &mut self.to_free);
                        self.media_gen += 1;
                    }
                }
            }
        }
    }

    fn maybe_trim_disk_cache(&mut self, waker: &UiWaker) {
        let Some(dir) = self.disk_cache_dir.clone() else {
            return;
        };
        let started = self.url_loads_started;
        self.url_loads_started += 1;
        if !should_trim_disk_cache(started)
            || self.trim_task.as_ref().is_some_and(OneShot::is_pending)
        {
            return;
        }
        self.trim_task = waker
            .spawn_one_shot("markdown-media-trim", move || {
                trim_disk_cache(&dir, DISK_CACHE_LIMIT_BYTES)
            })
            .ok();
    }

    fn finish_task(&mut self, key: &MediaKey, generation: u64, result: LoadResult) {
        let Some(entry) = self.entries.get_mut(key) else {
            return;
        };
        if entry.generation != generation || !entry.in_flight {
            return;
        }
        match result {
            Ok(pixels) => {
                let natural = Some((pixels.natural_w, pixels.natural_h));
                if entry.natural != natural {
                    entry.natural = natural;
                    self.media_gen += 1;
                }
                entry.state = EntryState::Ready;
                entry.stamp = pixels.stamp;
                entry.pending = Some(pixels);
                self.pending_uploads.push(key.clone());
            }
            Err(error) => {
                fail_entry(entry, error, &mut self.to_free);
                self.media_gen += 1;
            }
        }
    }

    fn poll_revalidation(&mut self) {
        let Some(job) = self.revalidation.as_mut() else {
            return;
        };
        let changed = match job.poll() {
            OneShotState::Pending => return,
            OneShotState::Ready(changed) => changed,
            OneShotState::Closed => Vec::new(),
        };
        self.revalidation = None;
        for (path_key, generation) in changed {
            let current = self
                .entries
                .get(&MediaKey::File(path_key.clone()))
                .is_some_and(|entry| entry.generation == generation);
            if current {
                self.invalidate_path(&path_key);
            }
        }
    }

    fn upload_pending(&mut self, host: &mut dyn TextureHost) {
        if self.pending_uploads.is_empty() {
            return;
        }
        let visible: Vec<bool> = self
            .pending_uploads
            .iter()
            .map(|key| self.entries.get(key).is_some_and(|entry| self.is_visible(entry)))
            .collect();
        let picked: Vec<MediaKey> = upload_order(&visible, MAX_UPLOADS_PER_FRAME)
            .into_iter()
            .map(|index| self.pending_uploads[index].clone())
            .collect();
        self.pending_uploads.retain(|key| !picked.contains(key));
        for key in picked {
            let Some(entry) = self.entries.get_mut(&key) else {
                continue;
            };
            let Some(pixels) = entry.pending.take() else {
                continue;
            };
            entry.in_flight = false;
            let (w, h) = (pixels.raster_w, pixels.raster_h);
            let size_ok = w > 0
                && h > 0
                && (w as usize).checked_mul(h as usize).and_then(|n| n.checked_mul(4))
                    == Some(pixels.rgba.len());
            if !size_ok {
                fail_entry(entry, MediaError::Decode, &mut self.to_free);
                self.media_gen += 1;
                continue;
            }
            // `pixels` (and its buffer) is dropped at the end of this iteration.
            match host.upload_rgba(w, h, &pixels.rgba) {
                Some(texture) => {
                    if let Some((old, _, _)) = entry.texture.replace((texture, w, h)) {
                        host.delete_texture(old);
                    }
                }
                None => {
                    fail_entry(entry, MediaError::TooManyPixels, &mut self.to_free);
                    self.media_gen += 1;
                }
            }
        }
    }

    /// Queues visible keys that lost their texture and re-renders visible textures whose
    /// raster is too far from the size they are drawn at.
    fn queue_visible(&mut self, visible: &[VisibleMedia]) {
        let mut keys: Vec<&MediaKey> = Vec::new();
        let mut facts: Vec<QueueFacts> = Vec::new();
        for item in visible {
            if let Some(entry) = self.entries.get(&item.key) {
                keys.push(&item.key);
                facts.push(Self::queue_facts(entry, true));
            }
        }
        for index in enqueue_indices(&facts) {
            if let Some(entry) = self.entries.get_mut(keys[index])
                && !entry.queued
            {
                entry.queued = true;
                self.queue.push_back(keys[index].clone());
            }
        }
        for item in visible {
            let Some(entry) = self.entries.get_mut(&item.key) else {
                continue;
            };
            let (Some(natural), Some((_, raster_w, _))) = (entry.natural, entry.texture) else {
                continue;
            };
            if entry.in_flight
                || entry.queued
                || !matches!(entry.state, EntryState::Ready)
                || entry.rerendered_for == Some(item.display_w)
                || !needs_rerender(natural, entry.request.scale, raster_w, item.display_w)
            {
                continue;
            }
            entry.request.max_raster_w = item.display_w;
            entry.rerendered_for = Some(item.display_w);
            entry.queued = true;
            self.queue.push_back(item.key.clone());
        }
    }

    fn evict_over_budget(&mut self) {
        let invisible: u64 = self
            .entries
            .values()
            .map(|entry| self.budget_item(entry))
            .filter(|item| !item.visible)
            .map(|item| item.bytes)
            .sum();
        if invisible <= INVISIBLE_TEXTURE_BUDGET {
            return;
        }
        let mut keys: Vec<MediaKey> = Vec::new();
        let mut items: Vec<BudgetItem> = Vec::new();
        for (key, entry) in &self.entries {
            if entry.texture.is_some() {
                keys.push(key.clone());
                items.push(self.budget_item(entry));
            }
        }
        for index in eviction_plan(&items, INVISIBLE_TEXTURE_BUDGET) {
            if let Some((texture, _, _)) = self.entries.get_mut(&keys[index]).and_then(|entry| entry.texture.take()) {
                self.to_free.push(texture);
            }
        }
    }
}

/// Marks an entry failed and releases whatever it held; its texture goes to the free list.
fn fail_entry(entry: &mut Entry, error: MediaError, to_free: &mut Vec<glow::Texture>) {
    entry.state = EntryState::Failed(error);
    entry.in_flight = false;
    entry.pending = None;
    if let Some((texture, _, _)) = entry.texture.take() {
        to_free.push(texture);
    }
}

/// True when the file is gone, unreadable or differs from the stamp taken at load time.
fn file_changed(path: &std::path::Path, stamp: &FileStamp) -> bool {
    match std::fs::metadata(path) {
        Ok(meta) => {
            meta.len() != stamp.len
                || meta.modified().unwrap_or(SystemTime::UNIX_EPOCH) != stamp.mtime
        }
        Err(_) => true,
    }
}
