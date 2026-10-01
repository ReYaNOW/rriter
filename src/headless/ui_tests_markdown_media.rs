//! Markdown Read mode media: images, SVG, Mermaid, revalidation on tab activation.
//!
//! Everything goes through the UI (the `open` command, the Read toggle, tab clicks, the
//! `wait` command that drives `about_to_wait` and the frames). Nothing is seeded into the
//! media cache; fixtures are plain files written into a per-process scratch directory.

use crate::headless::HeadlessSession;
use crate::headless::tests_support::{
    click_ui, dump, has_ui, run_script, scratch_dir, wait_until, workspace_with_explorer,
};
use serde_json::Value;
use std::path::{Path, PathBuf};

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;
const WAIT_MS: u64 = 15_000;

const BADGE_SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"20\">\
<rect width=\"100\" height=\"20\" fill=\"#4c1\"/></svg>";

fn write_png(path: &Path, width: u32, height: u32) {
    let image = image::RgbaImage::from_pixel(width, height, image::Rgba([200, 30, 30, 255]));
    image.save_with_format(path, image::ImageFormat::Png).expect("write png fixture");
}

/// A workspace with `preview.md` (the given source), `pic.png` (64x32), `badge.svg` and a
/// text file `other.txt` used to switch away from the Markdown tab.
fn fixture(name: &str, markdown: &str) -> (PathBuf, PathBuf) {
    let dir = scratch_dir(name);
    write_png(&dir.join("pic.png"), 64, 32);
    std::fs::write(dir.join("badge.svg"), BADGE_SVG).expect("write svg fixture");
    std::fs::write(dir.join("other.txt"), "plain text\n").expect("write text fixture");
    let path = dir.join("preview.md");
    std::fs::write(&path, markdown).expect("write markdown fixture");
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
        "# Top\n\n[to b](b.md#Раздел-2)\n\n[missing](nope.md)\n\n[web](https://example.com/x)\n\n[bad anchor](#нет-такого)\n\n{filler}"
    );
    let (dir, path) = fixture("md-links", &source);
    std::fs::write(dir.join("b.md"), format!("# B\n\n{filler}## Раздел 2\n\nend\n")).expect("write b.md");
    let mut session = open_markdown_read(&dir, &path);
    let to_b = link_point(&mut session, |t| matches!(t, LinkTarget::File { path, .. } if path.ends_with("b.md")))
        .expect("link to b.md is on screen");
    let missing = link_point(&mut session, |t| matches!(t, LinkTarget::File { path, .. } if path.ends_with("nope.md")))
        .expect("link to nope.md is on screen");
    let web = link_point(&mut session, |t| matches!(t, LinkTarget::External(_))).expect("web link");
    let bad = link_point(&mut session, |t| matches!(t, LinkTarget::Anchor(_))).expect("anchor link");
    let tabs_before = session.app.tabs.len();

    // Garbage destinations: an anchor without a heading and a file that does not exist.
    for point in [bad, missing] {
        click_at(&mut session, point);
        assert_eq!(session.app.tabs.len(), tabs_before);
        assert!(session.app.file_path.as_deref().is_some_and(|p| p.ends_with("preview.md")));
        assert!(session.app.markdown.read_selection_range().is_none());
    }
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

    // A click opens b.md in Read mode and scrolls to its heading.
    click_at(&mut session, to_b);
    wait_until(&mut session, WAIT_MS, "b.md scrolled to the anchor", |session| {
        session.app.file_path.as_deref().is_some_and(|p| p.ends_with("b.md"))
            && session.app.markdown.pending_anchor.is_none()
            && session.app.scroll_y.current > 100.0
    });
    assert_eq!(session.app.markdown_mode(), crate::app::MarkdownMode::Read);
    let _ = std::fs::remove_dir_all(dir);
}
