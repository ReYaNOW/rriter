use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::texture_budget::{
    BudgetItem, QueueEntryState, QueueFacts, UploadRank, enqueue_indices, eviction_plan,
    far_load_may_start, far_upload_fits, native_raster_suffices, near_fits, needs_rerender,
    next_to_start, prefetch_rank, should_trim_disk_cache, texture_totals,
    upload_order,
};
use super::*;

const TIMEOUT: Duration = Duration::from_secs(2);

fn mermaid_req(n: u64, max_raster_w: u32) -> MediaRequest {
    MediaRequest {
        key: MediaKey::Mermaid(n),
        source: MediaSource::Mermaid(format!("graph {n}")),
        max_raster_w,
        scale: 1.0,
    }
}

fn file_req(path: &Path) -> MediaRequest {
    MediaRequest {
        key: MediaKey::File(PathKey::new(path)),
        source: MediaSource::File(path.to_path_buf()),
        max_raster_w: 800,
        scale: 1.0,
    }
}

fn pixels(natural: (f32, f32), raster: (u32, u32)) -> MediaPixels {
    MediaPixels {
        natural_w: natural.0,
        natural_h: natural.1,
        raster_w: raster.0,
        raster_h: raster.1,
        rgba: vec![0u8; raster.0 as usize * raster.1 as usize * 4],
        stamp: None,
    }
}

fn small_pixels() -> MediaPixels {
    pixels((100.0, 50.0), (100, 50))
}

fn visible(key: &MediaKey, display_w: u32) -> VisibleMedia {
    VisibleMedia { key: key.clone(), display_w, display_h: display_w }
}

/// Loader that blocks until the gate opens.
#[derive(Default)]
struct Gate(AtomicBool);

impl Gate {
    fn open(&self) {
        self.0.store(true, Ordering::Release);
    }

    fn wait(&self) {
        while !self.0.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

fn wait_until(what: &str, mut cond: impl FnMut() -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while !cond() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[derive(Default)]
struct FakeHost {
    next: u32,
    uploaded: Vec<(u32, u32)>,
    deleted: Vec<glow::Texture>,
    refuse_upload: bool,
    /// Bytes of every strip, in order.
    strips: Vec<usize>,
}

impl TextureHost for FakeHost {
    fn create_rgba_texture(&mut self, w: u32, h: u32) -> Option<glow::Texture> {
        if self.refuse_upload {
            return None;
        }
        self.next += 1;
        self.uploaded.push((w, h));
        std::num::NonZeroU32::new(self.next).map(glow::NativeTexture)
    }

    fn upload_rgba_rows(&mut self, _texture: glow::Texture, w: u32, _y: u32, rows: u32, rgba: &[u8]) {
        assert_eq!(rgba.len(), w as usize * rows as usize * 4);
        self.strips.push(rgba.len());
    }

    fn delete_texture(&mut self, texture: glow::Texture) {
        self.deleted.push(texture);
    }
}

/// poll + prepare_gpu until `done`, like frames of the app.
fn pump(
    media: &mut MarkdownMedia,
    waker: &UiWaker,
    host: &mut FakeHost,
    visible: &[VisibleMedia],
    mut done: impl FnMut(&MarkdownMedia) -> bool,
) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        media.poll(waker);
        media.prepare_with(host, visible, waker);
        assert!(media.slots_used() <= MAX_SLOTS, "more than {MAX_SLOTS} slots in use");
        if done(media) {
            return;
        }
        assert!(Instant::now() < deadline, "timed out pumping the cache");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn poll_until(media: &mut MarkdownMedia, waker: &UiWaker, mut done: impl FnMut(&MarkdownMedia) -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        media.poll(waker);
        if done(media) {
            return;
        }
        assert!(Instant::now() < deadline, "timed out polling the cache");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn has_texture(media: &MarkdownMedia, key: &MediaKey) -> bool {
    matches!(media.entry(key), MediaEntryView::Ready { texture: Some(_), .. })
}

fn counting_loader(calls: &Arc<AtomicUsize>, result: fn() -> LoadResult) -> Loader {
    let calls = Arc::clone(calls);
    Arc::new(move |_| {
        calls.fetch_add(1, Ordering::SeqCst);
        result()
    })
}

fn test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rriter-md-cache-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test dir");
    dir
}

#[test]
fn duplicate_requests_load_once() {
    let waker = UiWaker::counting();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut media = MarkdownMedia::with_loader(counting_loader(&calls, || Ok(small_pixels())));
    let req = mermaid_req(1, 400);
    media.request(req.clone(), &waker);
    media.request(req.clone(), &waker);
    poll_until(&mut media, &waker, |m| matches!(m.entry(&req.key), MediaEntryView::Ready { .. }));
    media.request(req, &waker);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(media.stats().loads_started, 1);
}

#[test]
fn at_most_three_loads_run_and_slots_are_held_until_upload() {
    let waker = UiWaker::counting();
    let gate = Arc::new(Gate::default());
    let started = Arc::new(AtomicUsize::new(0));
    let loader: Loader = {
        let (gate, started) = (Arc::clone(&gate), Arc::clone(&started));
        Arc::new(move |_| {
            started.fetch_add(1, Ordering::SeqCst);
            gate.wait();
            Ok(small_pixels())
        })
    };
    let mut media = MarkdownMedia::with_loader(loader);
    let reqs: Vec<MediaRequest> = (0..5).map(|n| mermaid_req(n, 400)).collect();
    for req in &reqs {
        media.request(req.clone(), &waker);
    }
    // Before any frame every key is far: one slot stays free for visible and near keys.
    wait_until("two far loads to start", || started.load(Ordering::SeqCst) == 2);
    let mut host = FakeHost::default();
    let keys: Vec<VisibleMedia> = reqs.iter().map(|req| visible(&req.key, 400)).collect();
    media.prepare_with(&mut host, &keys, &waker);
    wait_until("three loads to start", || started.load(Ordering::SeqCst) == 3);
    std::thread::sleep(Duration::from_millis(30));
    assert_eq!(started.load(Ordering::SeqCst), 3, "a 4th load started while 3 slots were busy");
    assert_eq!(media.stats().loads_started, 3);

    gate.open();
    // Results are in, but nothing is uploaded: the slots stay taken.
    poll_until(&mut media, &waker, |m| m.pending_uploads.len() == 3);
    std::thread::sleep(Duration::from_millis(30));
    media.poll(&waker);
    assert_eq!(media.stats().loads_started, 3, "slots were freed before the upload");

    media.prepare_with(&mut host, &keys, &waker);
    assert_eq!(host.uploaded.len(), MAX_UPLOADS_PER_FRAME, "at most {MAX_UPLOADS_PER_FRAME} uploads per frame");
    pump(&mut media, &waker, &mut host, &keys, |m| reqs.iter().all(|r| has_texture(m, &r.key)));
    assert_eq!(media.stats().loads_started, 5);
    assert!(media.pending_uploads.is_empty() && media.tasks.is_empty());
}

#[test]
fn media_gen_follows_natural_size_and_failures() {
    let waker = UiWaker::counting();
    let results: Arc<Mutex<Vec<LoadResult>>> = Arc::new(Mutex::new(vec![
        Err(MediaError::Timeout),
        Ok(pixels((4000.0, 4000.0), (1000, 1000))),
    ]));
    let loader: Loader = {
        let results = Arc::clone(&results);
        Arc::new(move |_| results.lock().expect("results").remove(0))
    };
    let mut media = MarkdownMedia::with_loader(loader);
    assert_eq!(media.media_gen(), 0);

    let req = mermaid_req(1, 1000);
    media.request(req.clone(), &waker);
    let mut changed = false;
    let deadline = Instant::now() + TIMEOUT;
    while !matches!(media.entry(&req.key), MediaEntryView::Failed(_)) {
        changed |= media.poll(&waker);
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(changed, "poll must report the failure");
    assert_eq!(media.media_gen(), 1);

    // Retry after reset: a new natural size raises the generation once.
    media.reset_failed(std::slice::from_ref(&req.key));
    assert_eq!(media.media_gen(), 2);
    media.request(req.clone(), &waker);
    let mut host = FakeHost::default();
    let shown = [visible(&req.key, 1000)];
    pump(&mut media, &waker, &mut host, &shown, |m| has_texture(m, &req.key));
    assert_eq!(media.media_gen(), 3);

    // The same natural size again (a re-render) leaves the generation alone.
    results.lock().expect("results").push(Ok(pixels((4000.0, 4000.0), (600, 600))));
    media.request(mermaid_req(1, 600), &waker);
    let loads = media.stats().loads_started;
    pump(&mut media, &waker, &mut host, &[visible(&req.key, 600)], |m| m.stats().loads_started == loads + 1 && m.tasks.is_empty() && m.pending_uploads.is_empty());
    assert_eq!(media.media_gen(), 3, "a result with the same natural size must not raise media_gen");
    assert_eq!(host.uploaded, vec![(1000, 1000), (600, 600)]);
    assert_eq!(host.deleted.len(), 1, "the replaced texture is deleted");
}

#[test]
fn prepare_gpu_does_not_redraw_for_only_in_flight_visible_keys() {
    let gate = Arc::new(Gate::default());
    let loader: Loader = {
        let gate = Arc::clone(&gate);
        Arc::new(move |_| {
            gate.wait();
            Ok(small_pixels())
        })
    };
    let waker = UiWaker::counting();
    let mut media = MarkdownMedia::with_loader(loader);
    let req = mermaid_req(91, 100);
    media.request(req.clone(), &waker);
    let mut host = FakeHost::default();
    assert!(!media.prepare_with(&mut host, &[visible(&req.key, 100)], &waker));
    gate.open();
    poll_until(&mut media, &waker, |m| m.tasks.is_empty());
}

#[test]
fn failed_entry_is_not_retried_until_reset() {
    let waker = UiWaker::counting();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut media = MarkdownMedia::with_loader(counting_loader(&calls, || Err(MediaError::Decode)));
    let req = mermaid_req(1, 400);
    media.request(req.clone(), &waker);
    let mut host = FakeHost::default();
    let shown = [visible(&req.key, 400)];
    pump(&mut media, &waker, &mut host, &shown, |m| matches!(m.entry(&req.key), MediaEntryView::Failed(_)));
    for _ in 0..20 {
        media.request(req.clone(), &waker);
        media.poll(&waker);
        media.prepare_with(&mut host, &shown, &waker);
    }
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(calls.load(Ordering::SeqCst), 1, "a failed key must not be retried");
    assert_eq!(media.entry(&req.key), MediaEntryView::Failed(&MediaError::Decode));
    assert!(!media.prepare_with(&mut host, &shown, &waker), "nothing pending: no more frames wanted");

    media.reset_failed(std::slice::from_ref(&req.key));
    assert_eq!(media.entry(&req.key), MediaEntryView::Unknown);
    media.request(req, &waker);
    wait_until("the retry", || calls.load(Ordering::SeqCst) == 2);
}

#[test]
fn crashed_loader_becomes_failed_crashed() {
    let waker = UiWaker::counting();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut media = MarkdownMedia::with_loader(counting_loader(&calls, || Err(MediaError::Crashed)));
    let req = mermaid_req(1, 400);
    media.request(req.clone(), &waker);
    poll_until(&mut media, &waker, |m| matches!(m.entry(&req.key), MediaEntryView::Failed(_)));
    assert_eq!(media.entry(&req.key), MediaEntryView::Failed(&MediaError::Crashed));
    assert!(media.tasks.is_empty(), "the slot is free again");
}

#[test]
fn upload_refusal_fails_with_too_many_pixels() {
    let waker = UiWaker::counting();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut media = MarkdownMedia::with_loader(counting_loader(&calls, || Ok(small_pixels())));
    let req = mermaid_req(1, 400);
    media.request(req.clone(), &waker);
    let mut host = FakeHost { refuse_upload: true, ..FakeHost::default() };
    let shown = [visible(&req.key, 400)];
    pump(&mut media, &waker, &mut host, &shown, |m| matches!(m.entry(&req.key), MediaEntryView::Failed(_)));
    assert_eq!(media.entry(&req.key), MediaEntryView::Failed(&MediaError::TooManyPixels));
    assert!(media.pending_uploads.is_empty() && media.tasks.is_empty());
}

#[test]
fn failed_rerender_keeps_the_ready_texture() {
    let results: Arc<Mutex<Vec<LoadResult>>> = Arc::new(Mutex::new(vec![
        Ok(pixels((2000.0, 1000.0), (2000, 1000))),
        Err(MediaError::Timeout),
    ]));
    let loader: Loader = {
        let results = Arc::clone(&results);
        Arc::new(move |_| results.lock().expect("results").remove(0))
    };
    let waker = UiWaker::counting();
    let mut media = MarkdownMedia::with_loader(loader);
    let req = mermaid_req(92, 2000);
    media.request(req.clone(), &waker);
    let mut host = FakeHost::default();
    let wide = [visible(&req.key, 2000)];
    pump(&mut media, &waker, &mut host, &wide, |m| has_texture(m, &req.key));
    media.request(mermaid_req(92, 1000), &waker);
    let narrow = [visible(&req.key, 1000)];
    pump(&mut media, &waker, &mut host, &narrow, |m| m.stats().loads_started == 2 && m.tasks.is_empty());
    assert!(has_texture(&media, &req.key));
    assert!(matches!(media.entry(&req.key), MediaEntryView::Ready { texture: Some(_), .. }));
}

#[test]
fn invalidate_path_drops_entry_and_frees_texture() {
    let waker = UiWaker::counting();
    let dir = test_dir("invalidate");
    let path = dir.join("a.png");
    std::fs::write(&path, b"x").expect("write");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut media = MarkdownMedia::with_loader(counting_loader(&calls, || Ok(small_pixels())));
    let req = file_req(&path);
    media.request(req.clone(), &waker);
    let mut host = FakeHost::default();
    let shown = [visible(&req.key, 100)];
    pump(&mut media, &waker, &mut host, &shown, |m| has_texture(m, &req.key));
    let gen_before = media.media_gen();

    media.invalidate_path(&PathKey::new(&path));
    assert_eq!(media.entry(&req.key), MediaEntryView::Unknown);
    assert_eq!(media.media_gen(), gen_before + 1);
    assert_eq!(media.stats().texture_bytes, 0);
    media.prepare_with(&mut host, &[], &waker);
    assert_eq!(host.deleted.len(), 1, "the texture of the removed entry is freed at the next prepare_gpu");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn stale_result_after_invalidate_is_discarded() {
    let waker = UiWaker::counting();
    let dir = test_dir("stale");
    let path = dir.join("a.png");
    std::fs::write(&path, b"x").expect("write");
    let gate = Arc::new(Gate::default());
    let started = Arc::new(AtomicUsize::new(0));
    let loader: Loader = {
        let (gate, started) = (Arc::clone(&gate), Arc::clone(&started));
        Arc::new(move |_| {
            started.fetch_add(1, Ordering::SeqCst);
            gate.wait();
            Ok(small_pixels())
        })
    };
    let mut media = MarkdownMedia::with_loader(loader);
    let req = file_req(&path);
    media.request(req.clone(), &waker);
    wait_until("the load to start", || started.load(Ordering::SeqCst) == 1);
    media.invalidate_path(&PathKey::new(&path));
    let gen_after_invalidate = media.media_gen();
    gate.open();
    poll_until(&mut media, &waker, |m| m.tasks.is_empty());
    assert_eq!(media.entry(&req.key), MediaEntryView::Unknown, "the entry must not come back");
    assert_eq!(media.media_gen(), gen_after_invalidate, "a discarded result must not raise media_gen");
    assert!(media.pending_uploads.is_empty(), "the slot is free");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn stale_result_does_not_feed_a_recreated_entry() {
    let waker = UiWaker::counting();
    let dir = test_dir("recreated");
    let path = dir.join("a.png");
    std::fs::write(&path, b"x").expect("write");
    let gate = Arc::new(Gate::default());
    let calls = Arc::new(AtomicUsize::new(0));
    let loader: Loader = {
        let (gate, calls) = (Arc::clone(&gate), Arc::clone(&calls));
        Arc::new(move |_| {
            let n = calls.fetch_add(1, Ordering::SeqCst);
            gate.wait();
            // The first (stale) load reports another size than the second.
            Ok(if n == 0 { pixels((10.0, 10.0), (10, 10)) } else { small_pixels() })
        })
    };
    let mut media = MarkdownMedia::with_loader(loader);
    let req = file_req(&path);
    media.request(req.clone(), &waker);
    wait_until("the first load", || calls.load(Ordering::SeqCst) == 1);
    media.invalidate_path(&PathKey::new(&path));
    media.request(req.clone(), &waker);
    wait_until("the second load", || calls.load(Ordering::SeqCst) == 2);
    gate.open();
    let mut host = FakeHost::default();
    let shown = [visible(&req.key, 100)];
    pump(&mut media, &waker, &mut host, &shown, |m| has_texture(m, &req.key) && m.tasks.is_empty());
    assert_eq!(
        media.entry(&req.key),
        MediaEntryView::Ready { natural_w: 100.0, natural_h: 50.0, texture: media.entries[&req.key].texture.as_ref().map(|t| &t.0) }
    );
    assert_eq!(host.uploaded, vec![(100, 50)], "only the current generation is uploaded");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn revalidate_drops_changed_and_vanished_files_only() {
    let waker = UiWaker::counting();
    let dir = test_dir("revalidate");
    let (changed, gone, same) = (dir.join("changed.png"), dir.join("gone.png"), dir.join("same.png"));
    for path in [&changed, &gone, &same] {
        std::fs::write(path, b"one").expect("write");
    }
    let loader: Loader = Arc::new(|req| {
        let MediaSource::File(path) = &req.source else {
            return Err(MediaError::Unsupported);
        };
        let meta = std::fs::metadata(path).map_err(|_| MediaError::NotFound)?;
        let mut px = small_pixels();
        px.stamp = Some(FileStamp { mtime: meta.modified().map_err(|_| MediaError::Decode)?, len: meta.len() });
        Ok(px)
    });
    let mut media = MarkdownMedia::with_loader(loader);
    let reqs: Vec<MediaRequest> = [&changed, &gone, &same].into_iter().map(|p| file_req(p)).collect();
    let keys: Vec<MediaKey> = reqs.iter().map(|r| r.key.clone()).collect();
    for req in &reqs {
        media.request(req.clone(), &waker);
    }
    let mut host = FakeHost::default();
    let shown: Vec<VisibleMedia> = keys.iter().map(|k| visible(k, 100)).collect();
    pump(&mut media, &waker, &mut host, &shown, |m| keys.iter().all(|k| has_texture(m, k)));

    std::fs::write(&changed, b"a longer body").expect("rewrite");
    std::fs::remove_file(&gone).expect("remove");
    media.revalidate_files(&keys, &waker);
    media.revalidate_files(&keys, &waker);
    poll_until(&mut media, &waker, |m| m.revalidation.is_none());
    assert_eq!(media.entry(&keys[0]), MediaEntryView::Unknown);
    assert_eq!(media.entry(&keys[1]), MediaEntryView::Unknown);
    assert!(has_texture(&media, &keys[2]), "an unchanged file keeps its entry");

    // Nothing changed now: the pass drops nothing and needs no further frames.
    let gen_before = media.media_gen();
    media.revalidate_files(&keys, &waker);
    poll_until(&mut media, &waker, |m| m.revalidation.is_none());
    assert_eq!(media.media_gen(), gen_before);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn missing_render_command_fails_with_unsupported() {
    let waker = UiWaker::counting();
    let dir = test_dir("unsupported");
    let http = reqwest::blocking::Client::builder().no_proxy().build().expect("client");
    let env = FetchEnv { cache_dir: dir.clone(), http: HttpSource::ready(http), max_bytes: 1024, render: None };
    let mut media = MarkdownMedia::new(env);
    let req = mermaid_req(1, 400);
    media.request(req.clone(), &waker);
    assert_eq!(media.entry(&req.key), MediaEntryView::Failed(&MediaError::Unsupported));
    assert_eq!(media.media_gen(), 1);
    assert_eq!(media.stats().loads_started, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn many_large_images_stay_in_budget_with_three_loads() {
    const IMAGES: u64 = 12;
    let waker = UiWaker::counting();
    let running = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let loader: Loader = {
        let (running, peak) = (Arc::clone(&running), Arc::clone(&peak));
        Arc::new(move |_| {
            let now = running.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(2));
            running.fetch_sub(1, Ordering::SeqCst);
            // 2048 x 2048 x 4 = 16 MiB each, 192 MiB in total: over the 128 MiB budget.
            Ok(pixels((2048.0, 2048.0), (2048, 2048)))
        })
    };
    let mut media = MarkdownMedia::with_loader(loader);
    let reqs: Vec<MediaRequest> = (0..IMAGES).map(|n| mermaid_req(n, 2048)).collect();
    for req in &reqs {
        media.request(req.clone(), &waker);
    }
    let mut host = FakeHost::default();
    // Scroll down one image at a time.
    for req in &reqs {
        let shown = [visible(&req.key, 2048)];
        pump(&mut media, &waker, &mut host, &shown, |m| has_texture(m, &req.key));
    }
    let last = [visible(&reqs[IMAGES as usize - 1].key, 2048)];
    media.prepare_with(&mut host, &last, &waker);
    let stats = media.stats();
    assert!(
        stats.texture_bytes - stats.visible_texture_bytes <= INVISIBLE_TEXTURE_BUDGET,
        "invisible textures exceed the budget: {stats:?}"
    );
    assert!(!host.deleted.is_empty(), "evicted textures are released");

    // Scroll back to the first image: it was evicted and renders again, then all is quiet.
    let first = [visible(&reqs[0].key, 2048)];
    pump(&mut media, &waker, &mut host, &first, |m| has_texture(m, &reqs[0].key) && m.tasks.is_empty());
    let loads = media.stats().loads_started;
    for _ in 0..20 {
        media.poll(&waker);
        media.prepare_with(&mut host, &first, &waker);
    }
    std::thread::sleep(Duration::from_millis(20));
    media.poll(&waker);
    assert_eq!(media.stats().loads_started, loads, "a settled view must not re-render");
    assert!(peak.load(Ordering::SeqCst) <= MAX_SLOTS, "peak of concurrent loads: {}", peak.load(Ordering::SeqCst));
}

#[test]
fn tall_image_is_not_rerendered_forever() {
    let waker = UiWaker::counting();
    let calls = Arc::new(AtomicUsize::new(0));
    // 1000 x 20000 natural: the raster is clamped to 4096 on the long side, so 205 px wide.
    let mut media = MarkdownMedia::with_loader(counting_loader(&calls, || Ok(pixels((1000.0, 20000.0), (205, 4096)))));
    let req = mermaid_req(1, 1000);
    media.request(req.clone(), &waker);
    let mut host = FakeHost::default();
    let shown = [visible(&req.key, 1000)];
    pump(&mut media, &waker, &mut host, &shown, |m| has_texture(m, &req.key));
    for _ in 0..30 {
        media.poll(&waker);
        media.prepare_with(&mut host, &shown, &waker);
    }
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(calls.load(Ordering::SeqCst), 1, "a tall image must not be requested again");
}

#[test]
fn ui_scale_change_rerenders_a_known_key_once() {
    let waker = UiWaker::counting();
    let scales = Arc::new(Mutex::new(Vec::<f32>::new()));
    let loader: Loader = {
        let scales = Arc::clone(&scales);
        Arc::new(move |req| {
            scales.lock().expect("scales").push(req.scale);
            let (w, h) = super::decode::raster_target_size(400.0, 300.0, req.scale, req.max_raster_w);
            Ok(pixels((400.0, 300.0), (w, h)))
        })
    };
    let mut media = MarkdownMedia::with_loader(loader);
    let req = mermaid_req(1, 1000);
    media.request(req.clone(), &waker);
    let mut host = FakeHost::default();
    pump(&mut media, &waker, &mut host, &[visible(&req.key, 400)], |m| has_texture(m, &req.key));
    // The UI scale goes 1 -> 2: the layout asks again and draws the image 800 wide.
    media.request(MediaRequest { scale: 2.0, ..req.clone() }, &waker);
    let scaled = [visible(&req.key, 800)];
    pump(&mut media, &waker, &mut host, &scaled, |m| m.stats().loads_started == 2 && m.tasks.is_empty() && m.pending_uploads.is_empty());
    for _ in 0..30 {
        media.poll(&waker);
        media.request(MediaRequest { scale: 2.0, ..req.clone() }, &waker);
        media.prepare_with(&mut host, &scaled, &waker);
    }
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(media.stats().loads_started, 2, "exactly one re-render after the scale change");
    assert_eq!(*scales.lock().expect("scales"), vec![1.0, 2.0]);
    assert_eq!(host.uploaded, vec![(400, 300), (800, 600)]);
}

#[test]
fn request_does_not_requeue_an_evicted_invisible_key() {
    const IMAGES: u64 = 30;
    let waker = UiWaker::counting();
    // 2048 x 2048 x 4 = 16 MiB each, 480 MiB in total: most of them get evicted.
    let mut media = MarkdownMedia::with_loader(Arc::new(|_| Ok(pixels((2048.0, 2048.0), (2048, 2048)))));
    let reqs: Vec<MediaRequest> = (0..IMAGES).map(|n| mermaid_req(n, 2048)).collect();
    for req in &reqs {
        media.request(req.clone(), &waker);
    }
    let mut host = FakeHost::default();
    for req in &reqs {
        let shown = [visible(&req.key, 2048)];
        pump(&mut media, &waker, &mut host, &shown, |m| has_texture(m, &req.key));
    }
    let last = [visible(&reqs[IMAGES as usize - 1].key, 2048)];
    media.prepare_with(&mut host, &last, &waker);
    let evicted: Vec<&MediaRequest> = reqs.iter().filter(|req| !has_texture(&media, &req.key)).collect();
    assert!(evicted.len() >= 15, "a large part is evicted: {}", evicted.len());
    let loads = media.stats().loads_started;

    // The editor refreshes the read model and asks for every key again; only the last is visible.
    for _ in 0..5 {
        for req in &reqs {
            media.request(req.clone(), &waker);
        }
        media.poll(&waker);
        media.prepare_with(&mut host, &last, &waker);
    }
    std::thread::sleep(Duration::from_millis(20));
    media.poll(&waker);
    assert_eq!(media.stats().loads_started, loads, "evicted invisible keys are not loaded again");

    // Scrolling one of them into view queues it exactly once.
    let back = evicted[0].key.clone();
    let shown = [visible(&back, 2048)];
    for _ in 0..5 {
        media.request(evicted[0].clone(), &waker);
    }
    pump(&mut media, &waker, &mut host, &shown, |m| has_texture(m, &back));
    for _ in 0..20 {
        media.poll(&waker);
        media.request(evicted[0].clone(), &waker);
        media.prepare_with(&mut host, &shown, &waker);
    }
    std::thread::sleep(Duration::from_millis(20));
    media.poll(&waker);
    // 480 MiB of pixels fit the RAM cache: the texture comes back by an upload, no load.
    assert_eq!(media.stats().loads_started, loads, "the visible evicted key is restored from RAM");
    let uploads = host.uploaded.len();
    for _ in 0..10 {
        media.poll(&waker);
        media.prepare_with(&mut host, &shown, &waker);
    }
    assert_eq!(host.uploaded.len(), uploads, "restored exactly once");
}

#[test]
fn rerender_requests_display_width_once() {
    let waker = UiWaker::counting();
    let widths = Arc::new(Mutex::new(Vec::<u32>::new()));
    let loader: Loader = {
        let widths = Arc::clone(&widths);
        Arc::new(move |req| {
            widths.lock().expect("widths").push(req.max_raster_w);
            let w = req.max_raster_w.min(4000);
            Ok(pixels((4000.0, 4000.0), (w, w)))
        })
    };
    let mut media = MarkdownMedia::with_loader(loader);
    let req = mermaid_req(1, 2000);
    media.request(req.clone(), &waker);
    let mut host = FakeHost::default();
    pump(&mut media, &waker, &mut host, &[visible(&req.key, 2000)], |m| has_texture(m, &req.key));
    // The column got narrower: the 2000 px texture is 100% off the new size.
    let narrow = [visible(&req.key, 1000)];
    pump(&mut media, &waker, &mut host, &narrow, |m| m.stats().loads_started == 2 && m.tasks.is_empty() && m.pending_uploads.is_empty());
    for _ in 0..30 {
        media.poll(&waker);
        media.prepare_with(&mut host, &narrow, &waker);
    }
    std::thread::sleep(Duration::from_millis(20));
    assert_eq!(*widths.lock().expect("widths"), vec![2000, 1000]);
    assert_eq!(host.uploaded, vec![(2000, 2000), (1000, 1000)]);
}

fn facts(visible: bool, state: QueueEntryState) -> QueueFacts {
    QueueFacts { visible, near: false, state }
}

#[test]
fn near_idle_key_is_queued_like_a_visible_one() {
    let near = |state| QueueFacts { visible: false, near: true, state };
    let all = [
        near(QueueEntryState::Idle),
        near(QueueEntryState::Textured),
        near(QueueEntryState::InFlight),
        near(QueueEntryState::Failed),
        near(QueueEntryState::Queued),
        facts(false, QueueEntryState::Idle),
    ];
    assert_eq!(enqueue_indices(&all), vec![0], "only the idle near key; a far key stays out");
}

#[test]
fn next_load_is_visible_first_then_the_nearest_and_far_loads_leave_a_slot() {
    let pick = |ranks: &[Option<f32>]| next_to_start(ranks.iter().copied());
    assert_eq!(pick(&[None, Some(300.0), Some(-1.0), Some(-1.0)]), Some(2), "first visible in queue order");
    assert_eq!(pick(&[None, Some(300.0), Some(40.0), Some(900.0)]), Some(2), "nearest prefetch key");
    assert_eq!(pick(&[None, None]), None, "only far keys: the caller decides");
    assert_eq!(pick(&[]), None);
    assert!(far_load_may_start(0, MAX_SLOTS));
    assert!(far_load_may_start(MAX_SLOTS - 2, MAX_SLOTS));
    assert!(!far_load_may_start(MAX_SLOTS - 1, MAX_SLOTS), "the last slot is kept for near keys");
}

#[test]
fn far_result_is_uploaded_only_within_the_invisible_budget() {
    let mib = 1024 * 1024;
    assert!(far_upload_fits(100 * mib, 28 * mib, 128 * mib));
    assert!(!far_upload_fits(120 * mib, 9 * mib, 128 * mib));
    assert!(!far_upload_fits(u64::MAX, 1, 128 * mib), "no overflow");
}

/// Like `pump`, with keys inside the prefetch margin.
fn pump_near(
    media: &mut MarkdownMedia,
    waker: &UiWaker,
    host: &mut FakeHost,
    visible: &[VisibleMedia],
    near: &[(MediaKey, f32)],
    mut done: impl FnMut(&MarkdownMedia) -> bool,
) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        media.poll(waker);
        media.prepare_frame(host, visible, near.iter().map(|(key, distance)| (key, *distance)), waker);
        assert!(media.slots_used() <= MAX_SLOTS, "more than {MAX_SLOTS} slots in use");
        if done(media) {
            return;
        }
        assert!(Instant::now() < deadline, "timed out pumping the cache");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn near_keys_are_prefetched_kept_and_drawn_on_the_frame_they_become_visible() {
    const IMAGES: u64 = 30;
    let waker = UiWaker::counting();
    // 2048 x 2048 x 4 = 16 MiB each, 480 MiB in total: far over the 128 MiB budget.
    let mut media = MarkdownMedia::with_loader(Arc::new(|_| Ok(pixels((2048.0, 2048.0), (2048, 2048)))));
    let reqs: Vec<MediaRequest> = (0..IMAGES).map(|n| mermaid_req(n, 2048)).collect();
    for req in &reqs {
        media.request(req.clone(), &waker);
    }
    let key = |n: usize| reqs[n].key.clone();
    let mut host = FakeHost::default();
    // Viewport on image 10; 9 and 11 are within the margin, 12 further down.
    let shown = [visible(&key(10), 2048)];
    let near = [(key(11), 10.0), (key(9), 10.0), (key(12), 900.0)];
    pump_near(&mut media, &waker, &mut host, &shown, &near, |m| {
        m.tasks.is_empty() && !m.uploads_waiting() && m.queue.is_empty()
    });
    for n in 9..=12 {
        assert!(has_texture(&media, &key(n)), "image {n} is prefetched");
    }
    let stats = media.stats();
    assert!(stats.texture_bytes - stats.visible_texture_bytes <= INVISIBLE_TEXTURE_BUDGET, "{stats:?}");
    assert!(
        (host.uploaded.len() as u64) < IMAGES,
        "far results over the budget are not uploaded: {} uploads",
        host.uploaded.len()
    );

    // One step down: 11 is visible and drawn at once, 10 went up into the margin and stays.
    let loads = media.stats().loads_started;
    let shown = [visible(&key(11), 2048)];
    let near = [(key(10), 10.0), (key(12), 10.0), (key(13), 900.0)];
    media.prepare_frame(&mut host, &shown, near.iter().map(|(key, distance)| (key, *distance)), &waker);
    assert!(has_texture(&media, &key(11)), "the image entering the view is drawn on that frame");
    pump_near(&mut media, &waker, &mut host, &shown, &near, |m| has_texture(m, &key(13)) && m.tasks.is_empty());
    for n in 10..=13 {
        assert!(has_texture(&media, &key(n)), "image {n} kept or prefetched");
    }
    assert!(media.stats().loads_started <= loads + 1, "only the new near key loads");
}

#[test]
fn enqueue_decision_needs_visible_idle_textureless_key() {
    let all = [
        facts(true, QueueEntryState::Idle),
        facts(false, QueueEntryState::Idle),
        facts(true, QueueEntryState::Textured),
        facts(true, QueueEntryState::InFlight),
        facts(true, QueueEntryState::Failed),
        facts(true, QueueEntryState::Queued),
    ];
    assert_eq!(enqueue_indices(&all), vec![0]);
}

#[test]
fn evicted_and_visible_again_key_is_queued_exactly_once_in_five_cycles() {
    let mut entry = facts(true, QueueEntryState::Idle);
    let mut queued_times = 0;
    for _ in 0..5 {
        for index in enqueue_indices(&[entry]) {
            assert_eq!(index, 0);
            queued_times += 1;
            entry.state = QueueEntryState::Queued;
        }
    }
    assert_eq!(queued_times, 1);
}

fn item(mib: u64, visible: bool, last_wanted_frame: u64) -> BudgetItem {
    BudgetItem { bytes: mib * 1024 * 1024, visible, near: false, last_wanted_frame }
}

#[test]
fn prefetch_zone_is_bounded_by_bytes_and_prefers_the_scroll_direction() {
    let mib = 1024 * 1024;
    assert!(near_fits(0, 300 * mib, 64 * mib), "the nearest key always joins");
    assert!(near_fits(40 * mib, 24 * mib, 64 * mib));
    assert!(!near_fits(40 * mib, 25 * mib, 64 * mib));
    // Scrolling down: 100 px above is behind (200), 150 px below is ahead (150).
    assert_eq!(prefetch_rank(100.0, true, 1.0), 200.0);
    assert_eq!(prefetch_rank(150.0, false, 1.0), 150.0);
    assert_eq!(prefetch_rank(150.0, false, -1.0), 300.0);
    assert_eq!(prefetch_rank(100.0, true, 0.0), 100.0, "still: both sides alike");
}

#[test]
fn a_large_texture_is_uploaded_in_strips_and_shown_only_when_complete() {
    let waker = UiWaker::counting();
    // 2048 x 2048 x 4 = 16 MiB: several strips of at most UPLOAD_BYTES_PER_FRAME.
    let mut media = MarkdownMedia::with_loader(Arc::new(|_| Ok(pixels((2048.0, 2048.0), (2048, 2048)))));
    let req = mermaid_req(1, 2048);
    media.request(req.clone(), &waker);
    let shown = [visible(&req.key, 2048)];
    poll_until(&mut media, &waker, |m| m.pending_uploads.len() == 1);
    let mut host = FakeHost::default();
    let strips = 2048usize.div_ceil(UPLOAD_BYTES_PER_FRAME / (2048 * 4));
    assert!(strips > 1);
    let mut frames = 0;
    while !has_texture(&media, &req.key) {
        media.prepare_with(&mut host, &shown, &waker);
        frames += 1;
        assert!(frames <= strips, "one strip per frame, the texture only when complete");
    }
    assert_eq!(frames, strips);
    assert_eq!(host.strips.len(), strips);
    assert!(host.strips.iter().all(|&bytes| bytes <= UPLOAD_BYTES_PER_FRAME), "{:?}", host.strips);
    assert_eq!(host.strips.iter().sum::<usize>(), 2048 * 2048 * 4);
}

#[test]
fn ram_cache_restores_evicted_textures_within_its_bound() {
    const IMAGES: u64 = 40;
    let waker = UiWaker::counting();
    // 40 x 16 MiB = 640 MiB of pixels: over both the texture and the RAM bound.
    let mut media = MarkdownMedia::with_loader(Arc::new(|_| Ok(pixels((2048.0, 2048.0), (2048, 2048)))));
    let reqs: Vec<MediaRequest> = (0..IMAGES).map(|n| mermaid_req(n, 2048)).collect();
    for req in &reqs {
        media.request(req.clone(), &waker);
    }
    let mut host = FakeHost::default();
    for req in &reqs {
        let shown = [visible(&req.key, 2048)];
        pump(&mut media, &waker, &mut host, &shown, |m| has_texture(m, &req.key));
    }
    let stats = media.stats();
    assert!(stats.ram_bytes <= RAM_CACHE_BUDGET + stats.visible_texture_bytes, "{stats:?}");
    assert!(stats.texture_bytes - stats.visible_texture_bytes <= INVISIBLE_TEXTURE_BUDGET, "{stats:?}");
    // Back to an image evicted from the GPU but recent enough for RAM: no new load.
    let loads = stats.loads_started;
    let back = &reqs[IMAGES as usize - 15].key;
    assert!(!has_texture(&media, back), "evicted from the GPU");
    pump(&mut media, &waker, &mut host, &[visible(back, 2048)], |m| has_texture(m, back));
    assert_eq!(media.stats().loads_started, loads, "restored from RAM");
    // The oldest one left RAM too: it loads again.
    let first = &reqs[0].key;
    pump(&mut media, &waker, &mut host, &[visible(first, 2048)], |m| has_texture(m, first));
    assert_eq!(media.stats().loads_started, loads + 1, "beyond the RAM bound a key is decoded again");
}

#[test]
fn a_native_bitmap_is_scaled_by_the_gpu_instead_of_re_rendered() {
    // 1440 px wide shot drawn 1920 (x1.333) or 2880 (x2) wide: the native texture serves.
    assert!(native_raster_suffices((1440.0, 900.0), 1.333, 1920));
    assert!(native_raster_suffices((1440.0, 900.0), 2.0, 2880));
    // A narrow column shrinks it below `NATIVE_MIN_SHRINK`: a CPU-resized raster is due.
    assert!(!native_raster_suffices((1440.0, 900.0), 1.0, 600));
}

#[test]
fn near_keys_beyond_the_byte_bound_are_not_prefetched() {
    let waker = UiWaker::counting();
    // 2048 x 2048 x 4 = 16 MiB each: four fit the 64 MiB near bound.
    let mut media = MarkdownMedia::with_loader(Arc::new(|_| Ok(pixels((2048.0, 2048.0), (2048, 2048)))));
    let reqs: Vec<MediaRequest> = (0..8).map(|n| mermaid_req(n, 2048)).collect();
    for req in &reqs {
        media.request(req.clone(), &waker);
    }
    let mut host = FakeHost::default();
    // Natural sizes known for all; then textures and RAM pixels are dropped, as after a
    // long scroll elsewhere.
    pump_near(&mut media, &waker, &mut host, &[], &[], |m| {
        m.tasks.is_empty() && !m.uploads_waiting() && m.queue.is_empty()
    });
    for req in &reqs {
        if let Some(entry) = media.entries.get_mut(&req.key) {
            entry.ram = None;
            if let Some((texture, _, _)) = entry.texture.take() {
                media.to_free.push(texture);
            }
        }
    }
    let near: Vec<(MediaKey, f32)> = reqs.iter().enumerate().map(|(n, req)| (req.key.clone(), n as f32 * 100.0)).collect();
    pump_near(&mut media, &waker, &mut host, &[], &near, |m| {
        (0..4).all(|n| has_texture(m, &reqs[n].key)) && m.tasks.is_empty() && m.queue.is_empty() && !m.uploads_waiting()
    });
    for req in &reqs[4..] {
        assert!(!has_texture(&media, &req.key), "a key past the byte bound is not prefetched");
    }
}

#[test]
fn near_textures_are_kept_but_count_against_the_invisible_budget() {
    let near = |mib: u64, frame: u64| BudgetItem { near: true, ..item(mib, false, frame) };
    // 180 MiB invisible, of it 100 MiB near: only far ones go, never-wanted first.
    let items = [near(50, 9), item(40, false, 4), near(50, 9), item(30, false, 0), item(10, false, 7)];
    assert_eq!(eviction_plan(&items, 128 * 1024 * 1024), vec![3, 1]);
    // Near textures alone over the budget stay.
    assert!(eviction_plan(&[near(100, 9), near(100, 9)], 128 * 1024 * 1024).is_empty());
}

#[test]
fn visible_textures_are_never_evicted_even_over_budget() {
    let items = [item(100, true, 9), item(100, true, 9), item(50, false, 1)];
    let budget = 128 * 1024 * 1024;
    // Only the invisible texture can go, and only while invisible ones exceed the budget.
    assert!(eviction_plan(&items, budget).is_empty());
    assert_eq!(eviction_plan(&items, 10 * 1024 * 1024), vec![2]);
    let only_visible = [item(300, true, 9)];
    assert!(eviction_plan(&only_visible, 0).is_empty());
}

#[test]
fn invisible_textures_go_oldest_first_until_within_budget() {
    let items = [item(60, false, 5), item(60, false, 2), item(60, false, 8), item(60, false, 1), item(10, true, 9)];
    let evicted = eviction_plan(&items, 128 * 1024 * 1024);
    // 240 MiB invisible, budget 128: the two oldest (frames 1 and 2) leave 120 MiB.
    assert_eq!(evicted, vec![3, 1]);
}

#[test]
fn rerender_threshold_is_25_percent_of_the_clamped_target() {
    let natural = (4000.0, 4000.0);
    assert!(needs_rerender(natural, 1.0, 1300, 1000), "30% off must re-render");
    assert!(!needs_rerender(natural, 1.0, 1200, 1000), "20% off must not");
    assert!(needs_rerender(natural, 1.0, 600, 1000), "too small is also off");
    assert!(!needs_rerender(natural, 1.0, 1000, 0), "an unknown display size never re-renders");
}

#[test]
fn rerender_compares_with_the_clamped_raster_target() {
    // 1000 x 20000 is clamped to 4096 px on the long side: the raster is 205 px wide whatever
    // the display width is, and a re-render would give the same 205.
    assert!(!needs_rerender((1000.0, 20000.0), 1.0, 205, 1000));
    // An image smaller than its display width is not stretched by a re-render either.
    assert!(!needs_rerender((300.0, 200.0), 1.0, 300, 1000));
    // Scale counts: natural 500 at scale 2 wants 1000.
    assert!(!needs_rerender((500.0, 500.0), 2.0, 1000, 1000));
    assert!(needs_rerender((500.0, 500.0), 2.0, 500, 1000));
}

#[test]
fn totals_sum_texture_bytes_over_a_slice() {
    let items = [item(10, true, 3), item(20, false, 2), item(0, true, 1)];
    assert_eq!(texture_totals(&items), (30 * 1024 * 1024, 10 * 1024 * 1024));
    assert_eq!(texture_totals(&[]), (0, 0));
}

#[test]
fn uploads_take_visible_then_near_first_and_at_most_the_limit() {
    use UploadRank::{Far, Near, Visible};
    assert_eq!(upload_order(&[Far, Visible, Far, Visible], 2), vec![1, 3]);
    assert_eq!(upload_order(&[Far, Far, Visible], 2), vec![2, 0]);
    assert_eq!(upload_order(&[Far, Near, Visible, Near], 3), vec![2, 1, 3]);
    assert!(upload_order(&[], 2).is_empty());
}

#[test]
fn disk_cache_is_trimmed_first_and_then_every_tenth_url_load() {
    let trims: Vec<u64> = (0..25).filter(|n| should_trim_disk_cache(*n)).collect();
    assert_eq!(trims, vec![0, 10, 20]);
}

#[test]
fn the_http_client_is_built_on_the_first_get_and_only_once() {
    let builds = Arc::new(AtomicUsize::new(0));
    let counter = builds.clone();
    let source = HttpSource::lazy(move || {
        counter.fetch_add(1, Ordering::SeqCst);
        reqwest::blocking::Client::builder().no_proxy().build().ok()
    });
    assert_eq!(builds.load(Ordering::SeqCst), 0, "building the cache must not build the client");
    assert!(source.get().is_some());
    assert!(source.get().is_some());
    assert_eq!(builds.load(Ordering::SeqCst), 1);
}
