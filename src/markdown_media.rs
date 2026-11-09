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
    BudgetItem, QueueEntryState, QueueFacts, INVISIBLE_TEXTURE_BUDGET, NEAR_TEXTURE_BUDGET,
    UploadRank, enqueue_indices, eviction_plan, far_load_may_start, far_upload_fits,
    native_raster_suffices, near_fits, needs_rerender, next_to_start, should_enqueue,
    should_trim_disk_cache, texture_totals, upload_order,
};
pub(crate) use texture_budget::prefetch_rank;

pub(crate) use render_helper::run_media_helper_if_requested;

/// Loads one local image using the same bounded decoder/helper as Markdown Reader.
pub(crate) fn load_image_path(path: PathBuf) -> Result<MediaPixels, MediaError> {
    let env = FetchEnv {
        cache_dir: crate::platform::cache_dir().join("markdown-images"),
        http: HttpSource::lazy(|| None),
        max_bytes: MAX_FETCH_BYTES,
        render: RenderCommand::for_current_process(),
    };
    let req = MediaRequest {
        key: MediaKey::File(PathKey::new(&path)),
        source: MediaSource::File(path),
        max_raster_w: 4096,
        scale: 1.0,
    };
    load_media(&req, &env)
}


/// Identity of one media item inside the cache.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) enum MediaKey {
    File(PathKey),
    Url(String),
    /// Hash of the Mermaid source.
    Mermaid(u64),
}

impl MediaKey {
    /// Stable text form of the key for the headless dump.
    pub(crate) fn dump_name(&self) -> String {
        match self {
            MediaKey::File(path) => path.to_string_lossy(),
            MediaKey::Url(url) => url.clone(),
            MediaKey::Mermaid(hash) => format!("mermaid:{hash:016x}"),
        }
    }
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

/// The http client, built on the first `get` (in a worker thread, on the first Url fetch) so
/// that starting the application and opening documents without remote images pay nothing.
pub(crate) struct HttpSource {
    build: Box<dyn Fn() -> Option<reqwest::blocking::Client> + Send + Sync>,
    client: std::sync::Mutex<Option<reqwest::blocking::Client>>,
}

impl HttpSource {
    pub(crate) fn lazy(build: impl Fn() -> Option<reqwest::blocking::Client> + Send + Sync + 'static) -> Arc<Self> {
        Arc::new(Self { build: Box::new(build), client: std::sync::Mutex::new(None) })
    }

    /// An already built client.
    #[cfg(test)]
    pub(crate) fn ready(client: reqwest::blocking::Client) -> Arc<Self> {
        Arc::new(Self {
            build: Box::new(|| None),
            client: std::sync::Mutex::new(Some(client)),
        })
    }

    /// The shared client; `None` when it cannot be built. Blocks on the first call only.
    pub(crate) fn get(&self) -> Option<reqwest::blocking::Client> {
        let mut slot = self.client.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if slot.is_none() {
            *slot = (self.build)();
        }
        slot.clone()
    }
}

/// Everything `fetch_bytes` needs from its owner.
#[derive(Clone)]
pub(crate) struct FetchEnv {
    pub cache_dir: PathBuf,
    pub http: Arc<HttpSource>,
    pub max_bytes: u64,
    pub render: Option<RenderCommand>,
}

/// At most this many `load_media` tasks run at once (= helper processes). A slot stays
/// taken until the result is uploaded to the GPU or discarded.
const MAX_SLOTS: usize = 3;
/// One upload job per frame (`MarkdownMedia::upload_pending`); prefetch keeps uploads off the
/// critical path, so this is no visible delay.
const MAX_UPLOADS_PER_FRAME: usize = 1;
/// Rows sent to the GPU per frame. Measured on the scroll bench at 2560x1440: 6 MiB strips
/// cost ~3 ms of draw CPU (p99 3.95–4.04 ms against the 4.17 ms budget at 240 Hz), so a
/// strip is 2 MiB (~1 ms); a 1440 x 900 screenshot takes three frames, a tall one ~13, both
/// well inside the prefetch lead.
const UPLOAD_BYTES_PER_FRAME: usize = 2 * 1024 * 1024;
/// Decoded pixels kept in RAM after their upload (and for far results without a texture), on
/// top of those of the visible keys; least recently wanted go first.
const RAM_CACHE_BUDGET: u64 = 512 * 1024 * 1024;
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
    /// Decoded pixels kept in RAM (`RAM_CACHE_BUDGET`), so an evicted texture comes back
    /// with an upload only.
    pub ram_bytes: u64,
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
    /// Last frame the key was within the prefetch margin, and its distance to the viewport
    /// then (document px); the nearest queued key loads first.
    last_near_frame: u64,
    near_distance: f32,
    /// The texture is a bitmap at its own pixels, scaled by the GPU (`native_raster_ok`).
    native_raster: bool,
    /// A task is running or its pixels wait for the upload; the slot is held meanwhile.
    in_flight: bool,
    /// Unique per entry; a task result carrying another value is stale and dropped.
    generation: u64,
    stamp: Option<FileStamp>,
    /// Last request, reused when the texture is evicted or has to be re-rendered.
    request: MediaRequest,
    queued: bool,
    /// Decoded pixels between the end of the task (or a RAM restore) and the end of the upload.
    pending: Option<MediaPixels>,
    /// Decoded pixels of the last upload (or of a far result left without a texture), kept
    /// within `RAM_CACHE_BUDGET`: an evicted texture is restored without a new decode.
    ram: Option<MediaPixels>,
    /// Display width of the last re-render, so one size is never re-rendered twice in a row.
    rerendered_for: Option<u32>,
}

struct Task {
    key: MediaKey,
    generation: u64,
    job: OneShot<LoadResult>,
}

/// A texture filled `UPLOAD_BYTES_PER_FRAME` of rows per frame from `Entry::pending`; the
/// entry keeps drawing its old texture (or its frame) until the last strip is in.
struct StripUpload {
    key: MediaKey,
    generation: u64,
    texture: glow::Texture,
    w: u32,
    h: u32,
    next_row: u32,
    /// Restored from RAM: holds no load slot.
    from_ram: bool,
}

/// The GPU side of `prepare_gpu`; `Renderer` in the app, a fake in tests.
trait TextureHost {
    /// An allocated, unfilled RGBA8 texture.
    fn create_rgba_texture(&mut self, w: u32, h: u32) -> Option<glow::Texture>;
    /// Writes `rows` full rows from row `y`; `rgba` holds exactly those rows.
    fn upload_rgba_rows(&mut self, texture: glow::Texture, w: u32, y: u32, rows: u32, rgba: &[u8]);
    fn delete_texture(&mut self, texture: glow::Texture);
}

impl TextureHost for Renderer {
    fn create_rgba_texture(&mut self, w: u32, h: u32) -> Option<glow::Texture> {
        Renderer::create_rgba_texture(self, w, h)
    }

    fn upload_rgba_rows(&mut self, texture: glow::Texture, w: u32, y: u32, rows: u32, rgba: &[u8]) {
        Renderer::upload_rgba_rows(self, texture, w, y, rows, rgba)
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
    /// Keys whose RAM pixels wait for the upload (`Entry::ram` moved to `pending`); no slot.
    restores: Vec<MediaKey>,
    uploading: Option<StripUpload>,
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

    /// The application cache: one http client (15 s timeout) built on the first Url fetch,
    /// the on-disk cache under the platform cache directory and the current executable as
    /// the helper. Nothing here touches the network stack or the certificate store.
    pub(crate) fn from_platform() -> Self {
        Self::new(FetchEnv {
            cache_dir: crate::platform::cache_dir().join("markdown-images"),
            http: HttpSource::lazy(|| {
                crate::platform::blocking_http_client_builder()
                    .timeout(HTTP_TIMEOUT)
                    .build()
                    .ok()
            }),
            max_bytes: MAX_FETCH_BYTES,
            render: RenderCommand::for_current_process(),
        })
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
            restores: Vec::new(),
            uploading: None,
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
        let ram_bytes = self.entries.values().map(ram_bytes).sum();
        MediaStats { loads_started: self.loads_started, texture_bytes, visible_texture_bytes, ram_bytes }
    }

    /// Pixels still waiting for (or in the middle of) an upload, so another frame is due.
    fn uploads_waiting(&self) -> bool {
        !self.pending_uploads.is_empty() || !self.restores.is_empty() || self.uploading.is_some()
    }

    /// Asks for a key. Unknown keys become `Pending` and join the queue. A known key only
    /// stores the latest request (scale, column width), so the next render and the re-render
    /// check use the current target; it is queued again by `prepare_gpu` when it is visible
    /// or near, never from here (an evicted far image must not be rendered again).
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
        // A same-size re-render or reload does not bump media_gen but still needs a frame to upload.
        self.media_gen != before || self.uploads_waiting()
    }

    /// Frame step on the GPU side. `near` are the keys outside the viewport but within the
    /// prefetch margin, with their distance to it: they load (nearest first) and keep their
    /// textures, so they are drawn on the frame they scroll into view. True while another
    /// frame is needed for pending uploads.
    pub(crate) fn prepare_gpu<'k>(
        &mut self,
        renderer: &mut Renderer,
        visible: &[VisibleMedia],
        near: impl Iterator<Item = (&'k MediaKey, f32)>,
        waker: &UiWaker,
    ) -> bool {
        self.prepare_frame(renderer, visible, near, waker)
    }

    #[cfg(test)]
    fn prepare_with(&mut self, host: &mut dyn TextureHost, visible: &[VisibleMedia], waker: &UiWaker) -> bool {
        self.prepare_frame(host, visible, std::iter::empty(), waker)
    }

    fn prepare_frame<'k>(
        &mut self,
        host: &mut dyn TextureHost,
        visible: &[VisibleMedia],
        near: impl Iterator<Item = (&'k MediaKey, f32)>,
        waker: &UiWaker,
    ) -> bool {
        let media_gen = self.media_gen;
        for texture in self.to_free.drain(..) {
            host.delete_texture(texture);
        }
        self.frame += 1;
        for item in visible {
            if let Some(entry) = self.entries.get_mut(&item.key) {
                entry.last_visible_frame = self.frame;
            }
        }
        self.mark_near(near);
        self.upload_pending(host);
        self.queue_visible(visible);
        self.evict_over_budget();
        self.evict_ram_over_budget();
        self.start_tasks(waker);
        self.uploads_waiting() || self.media_gen != media_gen
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
        self.restores.retain(|pending| *pending != key);
        // A strip upload of the removed entry is dropped by `continue_upload`.
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
            last_near_frame: 0,
            near_distance: 0.0,
            native_raster: false,
            in_flight: false,
            generation: self.next_generation,
            stamp: None,
            request,
            queued: false,
            pending: None,
            ram: None,
            rerendered_for: None,
        }
    }

    fn queue_facts(entry: &Entry, visible: bool, near: bool) -> QueueFacts {
        QueueFacts {
            visible,
            near,
            state: if entry.texture.is_some() {
                QueueEntryState::Textured
            } else if entry.in_flight {
                QueueEntryState::InFlight
            } else if matches!(entry.state, EntryState::Failed(_)) {
                QueueEntryState::Failed
            } else if entry.queued {
                QueueEntryState::Queued
            } else {
                QueueEntryState::Idle
            },
        }
    }

    fn is_visible(&self, entry: &Entry) -> bool {
        self.frame > 0 && entry.last_visible_frame == self.frame
    }

    fn is_near(&self, entry: &Entry) -> bool {
        self.frame > 0 && entry.last_near_frame == self.frame
    }

    fn budget_item(&self, entry: &Entry) -> BudgetItem {
        BudgetItem {
            bytes: texture_bytes(entry),
            visible: self.is_visible(entry),
            near: self.is_near(entry),
            last_wanted_frame: entry.last_visible_frame.max(entry.last_near_frame),
        }
    }

    /// Marks the keys of the prefetch zone of this frame, nearest first (`near` comes sorted by
    /// `prefetch_rank`), while their textures fit `NEAR_TEXTURE_BUDGET` (`near_fits`), and
    /// queues those without a texture. Allocates only for a key that joins the queue.
    fn mark_near<'k>(&mut self, near: impl Iterator<Item = (&'k MediaKey, f32)>) {
        let mut held = 0u64;
        for (key, distance) in near {
            let Some(entry) = self.entries.get_mut(key) else {
                continue;
            };
            let bytes = expected_texture_bytes(entry);
            if !near_fits(held, bytes, NEAR_TEXTURE_BUDGET) {
                break;
            }
            held = held.saturating_add(bytes).max(1);
            entry.last_near_frame = self.frame;
            entry.near_distance = distance;
            if should_enqueue(&Self::queue_facts(entry, false, true)) {
                enqueue_or_restore(entry, key, &mut self.queue, &mut self.restores);
            }
        }
    }

    fn slots_used(&self) -> usize {
        let uploading = self.uploading.as_ref().is_some_and(|upload| !upload.from_ram);
        self.tasks.len() + self.pending_uploads.len() + usize::from(uploading)
    }

    /// Next key to load: the first visible one in queue order, else the nearest prefetch key
    /// (`next_to_start`), else the oldest while a slot may go to a far key.
    fn pop_next_to_start(&mut self) -> Option<MediaKey> {
        let allow_far = far_load_may_start(self.slots_used(), MAX_SLOTS);
        let entries = &self.entries;
        self.queue
            .retain(|key| entries.get(key).is_some_and(|entry| entry.queued && !entry.in_flight));
        let position = next_to_start(self.queue.iter().map(|key| {
            entries.get(key).and_then(|entry| {
                if self.is_visible(entry) {
                    Some(-1.0)
                } else if self.is_near(entry) {
                    Some(entry.near_distance)
                } else {
                    None
                }
            })
        }));
        match position {
            Some(position) => self.queue.remove(position),
            None if allow_far => self.queue.pop_front(),
            None => None,
        }
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
                // Only a bitmap decoder returns the natural pixels when the target differs
                // (SVG and Mermaid always render at the target size).
                let raster = (pixels.raster_w, pixels.raster_h);
                let target = decode::raster_target_size(
                    pixels.natural_w,
                    pixels.natural_h,
                    entry.request.scale,
                    entry.request.max_raster_w,
                );
                entry.native_raster = raster
                    == (pixels.natural_w.round() as u32, pixels.natural_h.round() as u32)
                    && raster != target;
                entry.pending = Some(pixels);
                self.pending_uploads.push(key.clone());
            }
            Err(error) => {
                if entry.texture.is_some() {
                    entry.state = EntryState::Ready;
                    entry.pending = None;
                    entry.in_flight = false;
                } else {
                    fail_entry(entry, error, &mut self.to_free);
                    self.media_gen += 1;
                }
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

    /// One upload job per frame: continue the strip upload in progress, or start the next
    /// waiting result (visible, then near, then far; `upload_order`) and send its first
    /// strip. A texture of up to `UPLOAD_BYTES_PER_FRAME` is complete on the frame it starts.
    fn upload_pending(&mut self, host: &mut dyn TextureHost) {
        if self.uploading.is_none() {
            self.start_upload(host);
        }
        self.continue_upload(host);
    }

    fn start_upload(&mut self, host: &mut dyn TextureHost) {
        while !self.pending_uploads.is_empty() || !self.restores.is_empty() {
            let rank_of = |key: &MediaKey| match self.entries.get(key) {
                Some(entry) if self.is_visible(entry) => UploadRank::Visible,
                Some(entry) if self.is_near(entry) => UploadRank::Near,
                _ => UploadRank::Far,
            };
            let ranks: Vec<UploadRank> = self.pending_uploads.iter().chain(&self.restores).map(rank_of).collect();
            let Some(&index) = upload_order(&ranks, MAX_UPLOADS_PER_FRAME).first() else {
                return;
            };
            let rank = ranks[index];
            let from_ram = index >= self.pending_uploads.len();
            let key = if from_ram {
                self.restores.remove(index - self.pending_uploads.len())
            } else {
                self.pending_uploads.remove(index)
            };
            let invisible_bytes: u64 = if rank == UploadRank::Far {
                self.entries
                    .values()
                    .filter(|entry| !self.is_visible(entry))
                    .map(texture_bytes)
                    .sum()
            } else {
                0
            };
            let Some(entry) = self.entries.get_mut(&key) else {
                continue;
            };
            let Some(pixels) = entry.pending.take() else {
                entry.in_flight = false;
                continue;
            };
            let (w, h) = (pixels.raster_w, pixels.raster_h);
            // A far key stays `Ready` without a texture (its natural size is known), its
            // pixels go to RAM; it is restored once it comes near.
            if rank == UploadRank::Far
                && !far_upload_fits(invisible_bytes, u64::from(w) * u64::from(h) * 4, INVISIBLE_TEXTURE_BUDGET)
            {
                entry.in_flight = false;
                entry.ram = Some(pixels);
                continue;
            }
            let size_ok = w > 0
                && h > 0
                && (w as usize).checked_mul(h as usize).and_then(|n| n.checked_mul(4))
                    == Some(pixels.rgba.len());
            let texture = if size_ok { host.create_rgba_texture(w, h) } else { None };
            let Some(texture) = texture else {
                entry.in_flight = false;
                if entry.texture.is_some() {
                    entry.state = EntryState::Ready;
                } else {
                    let error = if size_ok { MediaError::TooManyPixels } else { MediaError::Decode };
                    fail_entry(entry, error, &mut self.to_free);
                    self.media_gen += 1;
                }
                continue;
            };
            entry.pending = Some(pixels);
            self.uploading = Some(StripUpload {
                key,
                generation: entry.generation,
                texture,
                w,
                h,
                next_row: 0,
                from_ram,
            });
            return;
        }
    }

    /// Sends the next `UPLOAD_BYTES_PER_FRAME` of rows; the last strip swaps the texture in
    /// and moves the pixels to RAM. An entry dropped or reloaded meanwhile loses the upload.
    fn continue_upload(&mut self, host: &mut dyn TextureHost) {
        let Some(upload) = self.uploading.as_mut() else {
            return;
        };
        let pixels = self
            .entries
            .get(&upload.key)
            .filter(|entry| entry.generation == upload.generation && entry.in_flight)
            .and_then(|entry| entry.pending.as_ref());
        let Some(pixels) = pixels else {
            host.delete_texture(upload.texture);
            self.uploading = None;
            return;
        };
        let row_bytes = upload.w as usize * 4;
        let rows = (UPLOAD_BYTES_PER_FRAME / row_bytes).max(1).min((upload.h - upload.next_row) as usize);
        let start = upload.next_row as usize * row_bytes;
        host.upload_rgba_rows(
            upload.texture,
            upload.w,
            upload.next_row,
            rows as u32,
            &pixels.rgba[start..start + rows * row_bytes],
        );
        upload.next_row += rows as u32;
        if upload.next_row < upload.h {
            return;
        }
        let StripUpload { key, texture, w, h, .. } = self.uploading.take().expect("upload in progress");
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.in_flight = false;
            entry.ram = entry.pending.take();
            if let Some((old, _, _)) = entry.texture.replace((texture, w, h)) {
                host.delete_texture(old);
            }
        }
    }

    /// Drops RAM pixels over `RAM_CACHE_BUDGET`, least recently wanted first; pixels of
    /// visible and near keys stay. Allocates only when over the bound.
    fn evict_ram_over_budget(&mut self) {
        let held: u64 = self
            .entries
            .values()
            .filter(|entry| !self.is_visible(entry))
            .map(ram_bytes)
            .sum();
        if held <= RAM_CACHE_BUDGET {
            return;
        }
        let mut keys: Vec<MediaKey> = Vec::new();
        let mut items: Vec<BudgetItem> = Vec::new();
        for (key, entry) in &self.entries {
            if entry.ram.is_some() {
                keys.push(key.clone());
                items.push(BudgetItem { bytes: ram_bytes(entry), ..self.budget_item(entry) });
            }
        }
        for index in eviction_plan(&items, RAM_CACHE_BUDGET) {
            if let Some(entry) = self.entries.get_mut(&keys[index]) {
                entry.ram = None;
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
                facts.push(Self::queue_facts(entry, true, false));
            }
        }
        for index in enqueue_indices(&facts) {
            if let Some(entry) = self.entries.get_mut(keys[index])
                && !entry.queued
            {
                enqueue_or_restore(entry, keys[index], &mut self.queue, &mut self.restores);
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
                || (entry.native_raster && native_raster_suffices(natural, entry.request.scale, item.display_w))
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

/// Queues a key that needs a texture; with pixels in RAM it only waits for an upload.
fn enqueue_or_restore(
    entry: &mut Entry,
    key: &MediaKey,
    queue: &mut VecDeque<MediaKey>,
    restores: &mut Vec<MediaKey>,
) {
    if let Some(pixels) = entry.ram.take() {
        entry.pending = Some(pixels);
        entry.in_flight = true;
        restores.push(key.clone());
    } else {
        entry.queued = true;
        queue.push_back(key.clone());
    }
}

/// Bytes of the RAM pixels of an entry.
fn ram_bytes(entry: &Entry) -> u64 {
    entry.ram.as_ref().map_or(0, |pixels| pixels.rgba.len() as u64)
}

/// `w * h * 4` of the entry's texture, 0 without one.
fn texture_bytes(entry: &Entry) -> u64 {
    entry.texture.map_or(0, |(_, w, h)| u64::from(w) * u64::from(h) * 4)
}

/// Bytes of the entry's texture, or of the one its load will produce once the natural size
/// is known (the same raster rules as `decode_raster`); 0 while nothing is known.
fn expected_texture_bytes(entry: &Entry) -> u64 {
    if entry.texture.is_some() {
        return texture_bytes(entry);
    }
    let Some((natural_w, natural_h)) = entry.natural else {
        return 0;
    };
    let (w, h) = decode::raster_target_size(natural_w, natural_h, entry.request.scale, entry.request.max_raster_w);
    let pixels = if decode::native_raster_ok((natural_w, natural_h), w) {
        u64::from(natural_w as u32) * u64::from(natural_h as u32)
    } else {
        u64::from(w) * u64::from(h)
    };
    pixels * 4
}

/// Marks an entry failed and releases whatever it held; its texture goes to the free list.
fn fail_entry(entry: &mut Entry, error: MediaError, to_free: &mut Vec<glow::Texture>) {
    entry.state = EntryState::Failed(error);
    entry.in_flight = false;
    entry.pending = None;
    entry.ram = None;
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
