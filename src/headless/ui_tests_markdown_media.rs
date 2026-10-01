//! Markdown Read mode media: images, SVG, Mermaid, revalidation on tab activation.
//!
//! Everything goes through the UI (the `open` command, the Read toggle, tab clicks, the
//! `wait` command that drives `about_to_wait` and the frames). Nothing is seeded into the
//! media cache; fixtures are plain files written into a per-process scratch directory.

use crate::headless::HeadlessSession;
use crate::headless::tests_support::{
    click_ui, dump, has_ui, open_file_session, run_script, scratch_dir, wait_until,
    workspace_with_explorer,
};
use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;
const WAIT_MS: u64 = 15_000;

const BADGE_SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"20\">\
<rect width=\"100\" height=\"20\" fill=\"#4c1\"/></svg>";

fn write_png(path: &Path, width: u32, height: u32) {
    let image = image::RgbaImage::from_pixel(width, height, image::Rgba([200, 30, 30, 255]));
    image.save_with_format(path, image::ImageFormat::Png)
        .unwrap_or_else(|error| panic!("write png fixture: {error}"));
}

/// A workspace with `preview.md` (the given source), `pic.png` (64x32), `badge.svg` and a
/// text file `other.txt` used to switch away from the Markdown tab.
fn fixture(name: &str, markdown: &str) -> (PathBuf, PathBuf) {
    let dir = scratch_dir(name);
    write_png(&dir.join("pic.png"), 64, 32);
    std::fs::write(dir.join("badge.svg"), BADGE_SVG)
        .unwrap_or_else(|error| panic!("write svg fixture: {error}"));
    std::fs::write(dir.join("other.txt"), "plain text\n")
        .unwrap_or_else(|error| panic!("write text fixture: {error}"));
    let path = dir.join("preview.md");
    std::fs::write(&path, markdown).unwrap_or_else(|error| panic!("write markdown fixture: {error}"));
    (dir, path)
}

fn open_file(session: &mut HeadlessSession, path: &Path) {
    let lines = run_script(session, format!("open {}\nsettle 2000\n", path.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
}

fn open_markdown(dir: &Path, path: &Path) -> HeadlessSession {
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, dir);
    open_file(&mut session, path);
    wait_until(&mut session, 5000, "Markdown mode toggle", |session| {
        has_ui(&dump(session), "MarkdownModeToggle")
    });
    session
}

fn open_markdown_read(dir: &Path, path: &Path) -> HeadlessSession {
    let mut session = open_markdown(dir, path);
    click_ui(&mut session, "MarkdownModeToggle");
    wait_until(&mut session, 5000, "Markdown read layout", |session| {
        session.app.markdown.read_layout.content_height() > 0.0
    });
    session
}

fn tab_index(state: &Value, file_name: &str) -> usize {
    state["tabs"]
        .as_array()
        .and_then(|tabs| {
            tabs.iter().position(|tab| {
                tab["path"].as_str().is_some_and(|path| Path::new(path).ends_with(file_name))
            })
        })
        .unwrap_or_else(|| panic!("no tab for {file_name}: {state}"))
}

/// The dump entry of the media element whose key ends with `source_suffix`.
fn media_entry(state: &Value, file_name: &str, source_suffix: &str) -> Option<Value> {
    let tab = &state["tabs"][tab_index(state, file_name)];
    tab["markdown_media"]
        .as_array()?
        .iter()
        .find(|item| item["key"].as_str().is_some_and(|key| key.ends_with(source_suffix)))
        .cloned()
}

fn media_state(state: &Value, source_suffix: &str) -> Option<String> {
    media_entry(state, "preview.md", source_suffix)?["state"].as_str().map(str::to_owned)
}

fn size_close(item: &Value, width: f64, height: f64) -> bool {
    let close = |key: &str, want: f64| item[key].as_f64().is_some_and(|got| (got - want).abs() < 0.6);
    close("w", width) && close("h", height)
}

struct MediaHttpFixture {
    base_url: String,
    repaired: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl MediaHttpFixture {
    fn start(png: Vec<u8>) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .unwrap_or_else(|error| panic!("bind media HTTP fixture: {error}"));
        let address = listener.local_addr()
            .unwrap_or_else(|error| panic!("media HTTP fixture address: {error}"));
        listener.set_nonblocking(true)
            .unwrap_or_else(|error| panic!("set fixture nonblocking: {error}"));
        let repaired = Arc::new(AtomicBool::new(false));
        let worker_repaired = Arc::clone(&repaired);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker = std::thread::spawn(move || {
            while !worker_stop.load(Ordering::Relaxed) {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(_) => break,
                };
                let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
                let mut request = Vec::new();
                let mut buffer = [0_u8; 1024];
                while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                    match stream.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(count) => request.extend_from_slice(&buffer[..count]),
                    }
                    if request.len() > 16 * 1024 {
                        break;
                    }
                }
                let request_line = String::from_utf8_lossy(&request);
                let path = request_line.split_whitespace().nth(1).unwrap_or("");
                let (status, mime, body): (u16, &str, &[u8]) = match path {
                    "/remote.png" => (200, "image/png", &png),
                    "/badge.svg" => (200, "image/svg+xml", BADGE_SVG.as_bytes()),
                    "/repair.svg" if worker_repaired.load(Ordering::Relaxed) => {
                        (200, "image/svg+xml", BADGE_SVG.as_bytes())
                    }
                    "/repair.svg" => (404, "text/plain", b"missing"),
                    _ => (404, "text/plain", b"missing"),
                };
                let reason = if status == 200 { "OK" } else { "Not Found" };
                let headers = format!(
                    "HTTP/1.1 {status} {reason}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(headers.as_bytes());
                let _ = stream.write_all(body);
            }
        });
        Self {
            base_url: format!("http://{address}"),
            repaired,
            stop,
            worker: Some(worker),
        }
    }

    fn repair(&self) {
        self.repaired.store(true, Ordering::Relaxed);
    }
}

impl Drop for MediaHttpFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn settle_wheel(session: &mut HeadlessSession, x: f64, y: f64, delta: i32) -> Duration {
    let script = format!("mouse_move {x} {y}\nwheel 0 {delta}\nsettle 2000\n");
    let started = Instant::now();
    let lines = run_script(session, script.as_bytes());
    let elapsed = started.elapsed();
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    elapsed
}

fn visible_media_are_ready(state: &Value, scroll_y: f64, viewport_h: f64) -> bool {
    let items = state["tabs"][tab_index(state, "preview.md")]["markdown_media"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    items.iter().all(|item| {
        let Some(y) = item["y"].as_f64() else { return true };
        let h = item["h"].as_f64().unwrap_or(0.0);
        let visible = y + h > scroll_y && y < scroll_y + viewport_h;
        !visible || item["state"] == "ready"
    })
}

#[test]
fn http_media_and_local_media_load_through_the_reader_and_http_failure_retries_on_reopen() {
    let (dir, local_path) = fixture("md-media-http", "# Media\n");
    let remote_png = std::fs::read(dir.join("pic.png")).expect("read png fixture");
    let server = MediaHttpFixture::start(remote_png);
    let markdown = format!(
        "# Media\n\n![local](pic.png)\n\n![remote]({}/remote.png)\n\n![badge]({}/badge.svg)\n\n![repair]({}/repair.svg)\n\n```mermaid\ngraph TD; A-->B\n```\n\n[external](https://example.invalid) and [text](other.txt)\n",
        server.base_url, server.base_url, server.base_url
    );
    std::fs::write(&local_path, markdown).expect("write HTTP markdown fixture");
    let mut session = open_markdown_read(&dir, &local_path);
    wait_until(&mut session, WAIT_MS, "HTTP, local, and Mermaid media to settle", |session| {
        let state = dump(session);
        ["pic.png", "/remote.png", "/badge.svg", "/repair.svg"]
            .iter()
            .all(|key| media_state(&state, key).is_some())
            && media_state(&state, "/repair.svg").as_deref() == Some("failed:Http")
            && state["tabs"][tab_index(&state, "preview.md")]["markdown_media"]
                .as_array()
                .is_some_and(|items| items.iter().any(|item| item["key"].as_str().is_some_and(|key| key.starts_with("mermaid:")) && item["state"] == "ready"))
    });
    let state = dump(&mut session);
    for suffix in ["pic.png", "/remote.png", "/badge.svg"] {
        assert_eq!(media_state(&state, suffix).as_deref(), Some("ready"), "{state}");
    }
    assert_eq!(media_state(&state, "/repair.svg").as_deref(), Some("failed:Http"), "{state}");

    server.repair();
    open_file(&mut session, &dir.join("other.txt"));
    switch_to(&mut session, "preview.md");
    let preview_index = tab_index(&dump(&mut session), "preview.md");
    click_ui(&mut session, &format!("EditorTabClose({preview_index})"));
    open_file(&mut session, &local_path);
    if session.app.markdown_mode() != crate::app::MarkdownMode::Read {
        click_ui(&mut session, "MarkdownModeToggle");
    }
    wait_until(&mut session, WAIT_MS, "reopened HTTP media to retry", |session| {
        media_state(&dump(session), "/repair.svg").as_deref() == Some("ready")
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn thirty_large_images_survive_repeated_end_to_end_scrolling_within_the_texture_budget() {
    let dir = scratch_dir("md-media-load");
    let source_png = dir.join("source.png");
    write_png(&source_png, 1600, 1200);
    let mut markdown = String::from("# Large media\n\n");
    for index in 0..30 {
        let name = format!("large-{index:02}.png");
        std::fs::copy(&source_png, dir.join(&name)).expect("copy large PNG fixture");
        markdown.push_str(&format!("![image {index}]({name})\n\n"));
    }
    let path = dir.join("preview.md");
    std::fs::write(&path, markdown).expect("write load markdown fixture");
    let mut session = open_markdown_read(&dir, &path);
    wait_until(&mut session, WAIT_MS, "large image Reader layout", |session| {
        session.app.markdown.read_layout.content_height() > 0.0
    });

    let (x, y, _, viewport_h) = session
        .app
        .ui_registry
        .rect_for(crate::ui_system::UiId::MarkdownReadBody)
        .expect("Markdown Reader body rect");
    let (x, y, viewport_h) = (f64::from(x), f64::from(y), f64::from(viewport_h));
    let mut max_settle = Duration::ZERO;
    for direction in ["down", "up", "down", "up"] {
        let delta = if direction == "down" { -12 } else { 12 };
        let mut reached_edge = false;
        for _ in 0..100 {
            let max_scroll = (f64::from(session.app.markdown.read_layout.content_height()) - viewport_h).max(0.0);
            let current = f64::from(session.app.scroll_y.current);
            if (direction == "down" && current >= max_scroll - 2.0)
                || (direction == "up" && current <= 2.0)
            {
                reached_edge = true;
                break;
            }
            max_settle = max_settle.max(settle_wheel(&mut session, x + 20.0, y + 20.0, delta));
            wait_until(&mut session, WAIT_MS, "visible media after wheel scrolling", |session| {
                visible_media_are_ready(
                    &dump(session),
                    f64::from(session.app.scroll_y.current),
                    viewport_h,
                )
            });
        }
        assert!(reached_edge, "scroll did not reach {direction} edge: {}", session.app.scroll_y.current);
    }

    let state = dump(&mut session);
    let stats = &state["markdown_media_stats"];
    let texture_bytes = stats["texture_bytes"].as_u64().unwrap_or(u64::MAX);
    let visible_texture_bytes = stats["visible_texture_bytes"].as_u64().unwrap_or(0);
    let loads_started = stats["loads_started"].as_u64().unwrap_or(u64::MAX);
    assert!(texture_bytes <= 128 * 1024 * 1024 + visible_texture_bytes, "{stats}");
    assert!(loads_started <= 60, "more than one reload per image: {stats}");
    assert!(visible_media_are_ready(
        &state,
        f64::from(session.app.scroll_y.current),
        viewport_h,
    ), "visible images are not ready: {state}");
    // Measured ~1.2 s under the parallel suite; the bound only catches a blocked frame loop.
    assert!(max_settle <= Duration::from_secs(3), "slowest wheel settle took {max_settle:?}");
    eprintln!(
        "markdown media load: max settle {:?}, loads_started {}, texture_bytes {}, visible_texture_bytes {}",
        max_settle, loads_started, texture_bytes, visible_texture_bytes
    );
    let _ = std::fs::remove_dir_all(dir);
}

fn switch_to(session: &mut HeadlessSession, file_name: &str) {
    let index = tab_index(&dump(session), file_name);
    click_ui(session, &format!("EditorTab({index})"));
    let lines = run_script(session, b"settle 2000\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
}

#[test]
fn png_svg_and_a_missing_image_reach_their_states() {
    let markdown = "# Media\n\n![pic](pic.png)\n\n![badge](badge.svg)\n\n![gone](missing.png)\n";
    let (dir, path) = fixture("md-media-states", markdown);
    let mut session = open_markdown_read(&dir, &path);
    wait_until(&mut session, WAIT_MS, "all three media settled", |session| {
        let state = dump(session);
        media_state(&state, "pic.png").as_deref() == Some("ready")
            && media_state(&state, "badge.svg").as_deref() == Some("ready")
            && media_state(&state, "missing.png").as_deref() == Some("failed:NotFound")
    });
    // The layout sizes are the natural sizes times the scale.
    wait_until(&mut session, WAIT_MS, "laid out at natural size", |session| {
        let state = dump(session);
        let scale = f64::from(TEST_SCALE);
        media_entry(&state, "preview.md", "pic.png").is_some_and(|item| size_close(&item, 64.0 * scale, 32.0 * scale))
            && media_entry(&state, "preview.md", "badge.svg")
                .is_some_and(|item| size_close(&item, 100.0 * scale, 20.0 * scale))
    });
    let state = dump(&mut session);
    assert!(state["markdown_media_stats"]["loads_started"].as_u64().unwrap_or(0) >= 3, "{state}");
    // Every element carries its document rectangle: `x`/`y` are numbers and the elements
    // are stacked top to bottom.
    let items = state["tabs"][tab_index(&state, "preview.md")]["markdown_media"].as_array().cloned().unwrap_or_default();
    assert_eq!(items.len(), 3, "{state}");
    for item in &items {
        assert!(item["x"].is_number() && item["y"].is_number(), "{item}");
    }
    assert!(items[0]["y"].as_f64() < items[1]["y"].as_f64(), "{state}");
    assert!(items[1]["y"].as_f64() < items[2]["y"].as_f64(), "{state}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_changed_and_then_a_deleted_png_follow_the_disk_after_tab_activation() {
    // The image lives outside the workspace, so the file tree watcher never sees it change:
    // only the tab activation can notice.
    let assets = scratch_dir("md-media-revalidate-assets");
    write_png(&assets.join("pic.png"), 64, 32);
    let assets_name = assets.file_name().and_then(|name| name.to_str()).expect("assets dir name").to_owned();
    let (dir, path) = fixture("md-media-revalidate", &format!("# Media\n\n![pic](../{assets_name}/pic.png)\n"));
    let mut session = open_markdown_read(&dir, &path);
    let scale = f64::from(TEST_SCALE);
    wait_until(&mut session, WAIT_MS, "the png at 64x32", |session| {
        media_entry(&dump(session), "preview.md", "pic.png")
            .is_some_and(|item| item["state"] == "ready" && size_close(&item, 64.0 * scale, 32.0 * scale))
    });

    write_png(&assets.join("pic.png"), 32, 32);
    open_file(&mut session, &dir.join("other.txt"));
    switch_to(&mut session, "preview.md");
    wait_until(&mut session, WAIT_MS, "the png at 32x32", |session| {
        media_entry(&dump(session), "preview.md", "pic.png")
            .is_some_and(|item| item["state"] == "ready" && size_close(&item, 32.0 * scale, 32.0 * scale))
    });

    std::fs::remove_file(assets.join("pic.png")).expect("delete png");
    switch_to(&mut session, "other.txt");
    switch_to(&mut session, "preview.md");
    wait_until(&mut session, WAIT_MS, "the deleted png fails", |session| {
        media_state(&dump(session), "pic.png").as_deref() == Some("failed:NotFound")
    });
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(assets);
}

#[test]
fn a_mermaid_block_renders_and_garbage_fails() {
    let markdown = "# Diagrams\n\n```mermaid\ngraph TD; A-->B\n```\n\n```mermaid\n@@@ not a diagram ((((\n```\n";
    let (dir, path) = fixture("md-media-mermaid", markdown);
    let mut session = open_markdown_read(&dir, &path);
    wait_until(&mut session, WAIT_MS, "both mermaid blocks settled", |session| {
        let state = dump(session);
        let items = state["tabs"][tab_index(&state, "preview.md")]["markdown_media"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let states: Vec<&str> = items.iter().filter_map(|item| item["state"].as_str()).collect();
        states == ["ready", "failed:Mermaid"]
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn edit_mode_and_text_tabs_have_no_media() {
    let (dir, path) = fixture("md-media-unaffected", "# Media\n\n![pic](pic.png)\n");
    let mut session = open_markdown(&dir, &path);
    open_file(&mut session, &dir.join("other.txt"));
    let _ = run_script(&mut session, b"wait 300\n");
    let state = dump(&mut session);
    for file_name in ["preview.md", "other.txt"] {
        assert_eq!(state["tabs"][tab_index(&state, file_name)]["markdown_media"], serde_json::json!([]), "{file_name}: {state}");
    }
    assert_eq!(state["markdown_media_stats"]["loads_started"], 0, "{state}");
    assert_eq!(state["markdown_media_stats"]["texture_bytes"], 0, "{state}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_read_layout_is_built_once_per_geometry_with_the_ide_panel_open() {
    // The frame step (media requests, anchoring) and the draw both lay the document out; if
    // their widths differed in one frame the cache would be rebuilt twice per frame.
    let (dir, path) = fixture("md-media-width", "# Media\n\n![pic](pic.png)\n\ntext\n");
    let mut session = open_markdown_read(&dir, &path);
    wait_until(&mut session, WAIT_MS, "the png ready", |session| {
        media_state(&dump(session), "pic.png").as_deref() == Some("ready")
    });
    let lines = run_script(&mut session, b"settle 2000\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert!(session.app.is_ide_mode && session.app.ide_panel.visible_left_width(1.0) > 0.0);
    // Whole and fractional panel widths, and steps below the half pixel the draw ignores.
    for width in [240.0f32, 263.4, 301.7, 301.9, 302.1, 302.3, 287.0] {
        session.app.ide_panel.left_width = width;
        let before = session.app.markdown.read_layout.rebuild_count();
        let lines = run_script(&mut session, b"mouse_move 700 300\nmouse_move 701 300\nmouse_move 700 300\n");
        assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
        let rebuilds = session.app.markdown.read_layout.rebuild_count() - before;
        assert!(rebuilds <= 1, "panel width {width}: {rebuilds} rebuilds in three frames");
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// Screen point over the first link of the Reader whose target satisfies `wanted`.
fn link_point(
    session: &mut HeadlessSession,
    wanted: impl Fn(&crate::app::LinkTarget) -> bool,
) -> Option<(f32, f32)> {
    let (x, y, w, h) = session.app.ui_registry.rect_for(crate::ui_system::UiId::MarkdownReadBody)?;
    let mut py = y + 2.0;
    while py < y + h {
        let mut px = x + 2.0;
        while px < x + w {
            if let Some(index) = session.app.markdown_read_link_at(px, py)
                && session.app.markdown.read_layout.links().get(index as usize).is_some_and(&wanted)
            {
                return Some((px, py));
            }
            px += 3.0;
        }
        py += 3.0;
    }
    None
}

fn click_at(session: &mut HeadlessSession, (x, y): (f32, f32)) {
    let script = format!("mouse_move {x} {y}\nclick left down\nclick left up\nsettle 300\n");
    let lines = run_script(session, script.as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
}

#[test]
fn clicking_reader_links_follows_them_and_a_drag_only_selects() {
    use crate::app::LinkTarget;
    use crate::platform::ExternalRequest;

    let filler = "filler line\n\n".repeat(120);
    let source = format!(
        "# Top\n\n[to b](b.md#Раздел-2)\n\n[web](https://example.com/x)\n\n[top](#заголовок)\n\n[image](./pic.png)\n\n[missing](nope.md)\n\n[bad anchor](#нет-такого)\n\n{filler}## Заголовок\n\n{filler}"
    );
    let (dir, path) = fixture("md-links", &source);
    std::fs::write(dir.join("b.md"), format!("# B\n\n{filler}## Раздел 2\n\nend\n")).expect("write b.md");
    let mut session = open_markdown_read(&dir, &path);
    let to_b = link_point(&mut session, |t| matches!(t, LinkTarget::File { path, .. } if path.ends_with("b.md")))
        .expect("link to b.md is on screen");
    let missing = link_point(&mut session, |t| matches!(t, LinkTarget::File { path, .. } if path.ends_with("nope.md")))
        .expect("link to nope.md is on screen");
    let bad = link_point(&mut session, |t| matches!(t, LinkTarget::Anchor(anchor) if anchor == "нет-такого"))
        .expect("bad anchor link");
    let tabs_before = session.app.tabs.len();

    // Garbage destinations: an anchor without a heading and a file that does not exist.
    for point in [bad, missing] {
        click_at(&mut session, point);
        assert_eq!(session.app.tabs.len(), tabs_before);
        assert!(session.app.file_path.as_deref().is_some_and(|p| p.ends_with("preview.md")));
        assert!(session.app.markdown.read_selection_range().is_none());
    }
    let web = link_point(&mut session, |target| matches!(target, LinkTarget::External(_)))
        .expect("web link remains visible after the ignored links");
    session.app.external_requests.take();
    click_at(&mut session, web);
    assert_eq!(
        session.app.external_requests.take(),
        Some(ExternalRequest::OpenUrl("https://example.com/x".to_string()))
    );

    // A drag that starts on a link selects text and follows nothing.
    let (x, y) = to_b;
    let script = format!("mouse_move {x} {y}\nclick left down\nmouse_move {} {y}\nclick left up\nsettle 300\n", x + 40.0);
    let lines = run_script(&mut session, script.as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert_eq!(session.app.tabs.len(), tabs_before);
    assert!(session.app.file_path.as_deref().is_some_and(|p| p.ends_with("preview.md")));
    assert!(session.app.markdown.read_selection_range().is_some_and(|range| !range.is_empty()));

    // A click opens b.md in Read mode and scrolls to its heading.
    click_at(&mut session, to_b);
    wait_until(&mut session, WAIT_MS, "b.md scrolled to the anchor", |session| {
        session.app.file_path.as_deref().is_some_and(|p| p.ends_with("b.md"))
            && session.app.markdown.pending_anchor.is_none()
            && session.app.scroll_y.current > 100.0
    });
    assert_eq!(session.app.markdown_mode(), crate::app::MarkdownMode::Read);

    // An in-document anchor in a long document animates to the actual heading.
    open_file(&mut session, &path);
    if session.app.markdown_mode() != crate::app::MarkdownMode::Read {
        click_ui(&mut session, "MarkdownModeToggle");
    }
    wait_until(&mut session, WAIT_MS, "a.md Reader layout", |session| {
        session.app.markdown.read_layout.content_height() > 0.0
    });
    let image = link_point(&mut session, |target| matches!(target, LinkTarget::File { path, .. } if path.ends_with("pic.png")))
        .expect("image link");
    click_at(&mut session, image);
    assert_eq!(session.app.tabs.len(), tabs_before + 2);
    wait_until(&mut session, WAIT_MS, "image tab opened from markdown link", |session| {
        dump(session)["tabs"].as_array().and_then(|tabs| tabs.last()).is_some_and(|tab| tab["kind"] == "image" && tab["image"]["phase"] == "ready")
    });

    open_file(&mut session, &path);
    if session.app.markdown_mode() != crate::app::MarkdownMode::Read {
        click_ui(&mut session, "MarkdownModeToggle");
    }
    wait_until(&mut session, WAIT_MS, "a.md Reader layout for its anchor", |session| {
        session.app.markdown.read_layout.content_height() > 0.0
    });
    let top = link_point(&mut session, |target| matches!(target, LinkTarget::Anchor(anchor) if anchor == "заголовок"))
        .expect("in-document anchor link");
    let headings = session.app.markdown.read_document(session.app.editor.version)
        .expect("Markdown document")
        .headings(&session.app.markdown.read_source);
    let heading = headings.iter().find(|heading| heading.text == "Заголовок").expect("target heading");
    let target_y = session.app.markdown.read_layout.source_target_y(&heading.source_range).expect("heading layout position");
    click_at(&mut session, top);
    wait_until(&mut session, WAIT_MS, "scroll reaches the in-document heading", |session| {
        (session.app.scroll_y.current - target_y).abs() < 1.0
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn dirty_standalone_reader_confirms_before_following_file_link() {
    use crate::app::{LinkTarget, PendingAction};

    let (dir, path) = fixture("md-links-dirty", "[to b](b.md)\n");
    std::fs::write(dir.join("b.md"), "# B\n").expect("write b.md");
    let mut session = open_file_session(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &path);
    assert!(!session.app.is_ide_mode);
    session.app.set_markdown_mode(crate::app::MarkdownMode::Read);
    wait_until(&mut session, WAIT_MS, "standalone Reader layout", |session| {
        session.app.markdown.read_layout.content_height() > 0.0
    });
    session.app.editor.insert_str("local edit");
    wait_until(&mut session, WAIT_MS, "updated dirty Reader layout", |session| {
        session.app.markdown.read_layout.is_for_version(session.app.editor.version)
    });
    let link = link_point(&mut session, |target| matches!(target, LinkTarget::File { path, .. } if path.ends_with("b.md")))
        .expect("link to b.md");
    click_at(&mut session, link);
    assert!(session.app.editor.is_dirty());
    assert!(session.app.file_path.as_deref().is_some_and(|open| open.ends_with("preview.md")));
    assert_eq!(session.app.confirm_dialog.action(), PendingAction::OpenLinkedFile);
    let _ = std::fs::remove_dir_all(dir);
}
