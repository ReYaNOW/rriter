//! Headless UI coverage for editor folding and minimap navigation.

use crate::headless::tests_support::{
    click_ui, click_ui_fraction, dump, has_ui, open_file_session, run_script, scratch_dir,
    ui_rect, wait_until,
};
use crate::headless::HeadlessSession;
use std::ops::Range;

const TEST_SCALE: f32 = 4.0 / 3.0;
/// Per-channel tolerance for matching the minimap viewport border colour.
const BORDER_COLOR_TOLERANCE: i16 = 12;
/// Columns scanned from the minimap's left edge for the 2 px viewport border.
const BORDER_SCAN_COLUMNS: u32 = 4;

/// Drags the minimap from `from_y` to `to_y` at `x` and waits for the editor scroll to
/// stop (drag released and smooth scroll reached its target).
fn drag_minimap(session: &mut HeadlessSession, x: f64, from_y: f64, to_y: f64) {
    let lines = run_script(
        session,
        format!("mouse_move {x} {from_y}\nclick down\nmouse_move {x} {to_y}\nclick up\n")
            .as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(session, 1800, "minimap drag scroll settles", |session| {
        session.app.scroll_y.is_settled()
    });
}

fn long_minimap_file(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("long-minimap.txt");
    let text: String = (0..1200)
        .map(|line| format!("line {line:04} minimap fixture content\n"))
        .collect();
    std::fs::write(&path, text).expect("write long minimap fixture");
    path
}

fn scroll_y(state: &serde_json::Value) -> f64 {
    state["tabs"][0]["scroll_y"]
        .as_f64()
        .expect("active tab scroll position")
}

fn border_color_matches(pixel: [u8; 4], color: [f32; 4]) -> bool {
    (0..3).all(|channel| {
        let expected = (color[channel] * 255.0).round() as i16;
        (pixel[channel] as i16 - expected).abs() <= BORDER_COLOR_TOLERANCE
    })
}

fn minimap_thumb_bounds(
    image: &image::RgbaImage,
    minimap_rect: [f64; 4],
    body_rect: [f64; 4],
    selection: [f32; 4],
) -> Range<u32> {
    let left = minimap_rect[0].floor().max(0.0) as u32;
    let right = (left + BORDER_SCAN_COLUMNS).min(image.width());
    let start_y = body_rect[1].floor().max(0.0) as u32;
    let end_y = ((body_rect[1] + body_rect[3]).ceil() as u32).min(image.height());
    let mut first = None;
    let mut last = None;

    for y in start_y..end_y {
        if (left..right).any(|x| border_color_matches(image.get_pixel(x, y).0, selection)) {
            first.get_or_insert(y);
            last = Some(y);
        }
    }

    let start = first.expect("minimap viewport border pixel");
    let end = last.expect("minimap viewport border pixel") + 1;
    start..end
}

#[test]
fn headless_editor_folding_hides_rows_and_skips_cursor() {
    let dir = scratch_dir("ui-fold-hidden-rows");
    let file = dir.join("rows.rs");
    std::fs::write(
        &file,
        "fn alpha() {\n    let first = 1;\n    let second = 2;\n    let third = 3;\n}\nfn beta() {\n    let after = 4;\n}\n",
    )
    .expect("write fold fixture");
    let mut session = open_file_session(1280, 720, TEST_SCALE, &file);
    wait_until(&mut session, 5000, "fold arrows", |session| {
        let state = dump(session);
        has_ui(&state, "EditorFoldArrow(0)") && has_ui(&state, "EditorFoldArrow(5)")
    });

    let before = dump(&mut session);
    let beta_y_before = ui_rect(&before, "EditorFoldArrow(5)")[1];
    let line_height = session.app.renderer.as_ref().unwrap().line_height as f64;
    click_ui(&mut session, "EditorFoldArrow(0)");
    let folded = dump(&mut session);

    assert!(session.app.editor.folded_lines.contains(&0));
    let beta_y_after = ui_rect(&folded, "EditorFoldArrow(5)")[1];
    assert!(
        ((beta_y_before - beta_y_after) - 4.0 * line_height).abs() <= 2.0,
        "next visible block should move up by four rows: before={beta_y_before}, after={beta_y_after}, line_height={line_height}"
    );

    let lines = run_script(&mut session, b"key down\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let moved = dump(&mut session);
    assert_eq!(
        moved["tabs"][0]["cursor"]["line"].as_u64(),
        Some(6),
        "cursor should skip the four hidden rows: {moved}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_folding_sibling_blocks_toggle_independently() {
    let dir = scratch_dir("ui-fold-siblings");
    let file = dir.join("siblings.rs");
    std::fs::write(
        &file,
        "fn alpha() {\n    fn inner_alpha() {\n        let alpha = 1;\n    }\n}\nfn beta() {\n    fn inner_beta() {\n        let beta = 2;\n    }\n}\n",
    )
    .expect("write sibling fold fixture");
    let mut session = open_file_session(1280, 720, TEST_SCALE, &file);
    wait_until(&mut session, 5000, "sibling fold arrows", |session| {
        let state = dump(session);
        has_ui(&state, "EditorFoldArrow(0)")
            && has_ui(&state, "EditorFoldArrow(1)")
            && has_ui(&state, "EditorFoldArrow(5)")
            && has_ui(&state, "EditorFoldArrow(6)")
    });

    click_ui(&mut session, "EditorFoldArrow(0)");
    let alpha_folded = dump(&mut session);
    assert!(session.app.editor.folded_lines.contains(&0));
    assert!(!session.app.editor.folded_lines.contains(&5));
    assert!(!has_ui(&alpha_folded, "EditorFoldArrow(1)"));
    assert!(has_ui(&alpha_folded, "EditorFoldArrow(6)"));

    click_ui(&mut session, "EditorFoldArrow(5)");
    let both_folded = dump(&mut session);
    assert!(session.app.editor.folded_lines.contains(&0));
    assert!(session.app.editor.folded_lines.contains(&5));
    assert!(!has_ui(&both_folded, "EditorFoldArrow(1)"));
    assert!(!has_ui(&both_folded, "EditorFoldArrow(6)"));

    click_ui(&mut session, "EditorFoldArrow(0)");
    let alpha_unfolded = dump(&mut session);
    assert!(!session.app.editor.folded_lines.contains(&0));
    assert!(session.app.editor.folded_lines.contains(&5));
    assert!(has_ui(&alpha_unfolded, "EditorFoldArrow(1)"));
    assert!(!has_ui(&alpha_unfolded, "EditorFoldArrow(6)"));

    click_ui(&mut session, "EditorFoldArrow(5)");
    assert!(session.app.editor.folded_lines.is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_minimap_drag_thumb_is_proportional_and_clamped() {
    let dir = scratch_dir("ui-minimap-drag");
    let file = long_minimap_file(&dir);
    let mut session = open_file_session(1280, 720, TEST_SCALE, &file);
    let initial = dump(&mut session);
    let minimap = ui_rect(&initial, "EditorMinimap");
    let max_scroll = session
        .app
        .renderer
        .as_mut()
        .unwrap()
        .get_max_scroll(&session.app.editor, minimap[3] as f32)
        as f64;
    let start_x = minimap[0] + minimap[2] / 2.0;
    let start_y = minimap[1] + 2.0;
    let middle_y = minimap[1] + minimap[3] * 0.4;

    drag_minimap(&mut session, start_x, start_y, middle_y);
    let middle_scroll = scroll_y(&dump(&mut session));
    assert!(
        (middle_scroll / max_scroll - 0.4).abs() < 0.08,
        "thumb drag should track the pointer proportionally: scroll={middle_scroll}, max={max_scroll}"
    );

    let bottom_y = minimap[1] + minimap[3] - 2.0;
    drag_minimap(&mut session, start_x, middle_y, bottom_y);
    let bottom_scroll = scroll_y(&dump(&mut session));
    let line_height = session.app.renderer.as_ref().unwrap().line_height as f64;
    assert!(
        (max_scroll - bottom_scroll).abs() <= line_height,
        "thumb should stop at the bottom: scroll={bottom_scroll}, max={max_scroll}"
    );

    drag_minimap(&mut session, start_x, bottom_y, start_y);
    assert!(scroll_y(&dump(&mut session)) <= line_height);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_minimap_click_tracks_lower_file_position() {
    let dir = scratch_dir("ui-minimap-lower-click");
    let file = long_minimap_file(&dir);
    let mut session = open_file_session(1280, 720, TEST_SCALE, &file);
    let minimap = ui_rect(&dump(&mut session), "EditorMinimap");
    // Line drawn under the pointer, from the same metrics `draw_minimap` uses.
    let (line_height, target_line) = {
        let app = &mut session.app;
        let renderer = app.renderer.as_mut().unwrap();
        let editor_height = minimap[3] as f32;
        let max_scroll = renderer.get_max_scroll(&app.editor, editor_height);
        let metrics = crate::render_view::minimap_ui::minimap_view_metrics(
            renderer.minimap_total_visual_lines(&app.editor),
            editor_height,
            renderer.line_height,
            app.scroll_y.current.round().min(max_scroll),
            max_scroll,
        );
        let click_y = minimap[3] * 0.85;
        (
            renderer.line_height as f64,
            (click_y + metrics.scroll as f64) / metrics.line_height as f64,
        )
    };
    let (x, y) = click_ui_fraction(&mut session, "EditorMinimap", 0.5, 0.85);
    wait_until(&mut session, 1800, "minimap click scroll settles", |session| {
        session.app.scroll_y.is_settled()
    });

    let center_line = (scroll_y(&dump(&mut session)) + minimap[3] / 2.0) / line_height;
    assert!(
        target_line > 200.0,
        "85% of the minimap should point into the lower file: line={target_line}"
    );
    assert!(
        (center_line - target_line).abs() <= 2.0,
        "click at ({x}, {y}) should center the line drawn under it: center={center_line}, target={target_line}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_minimap_thumb_geometry_at_two_viewports() {
    let dir = scratch_dir("ui-minimap-geometry");
    let file = long_minimap_file(&dir);
    let mut thumb_heights = Vec::new();

    for (width, height) in [(1280, 720), (2560, 1440)] {
        let mut session = open_file_session(width, height, TEST_SCALE, &file);
        let initial = dump(&mut session);
        let body = ui_rect(&initial, "EditorTextBody");
        let minimap = ui_rect(&initial, "EditorMinimap");
        let selection = session.app.renderer.as_ref().unwrap().theme.sel;
        let screenshot = dir.join(format!("minimap-{width}.png"));
        let lines = run_script(
            &mut session,
            format!("mouse_move 0 0\nscreenshot {}\n", screenshot.display()).as_bytes(),
        );
        assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");

        let image = image::open(&screenshot).expect("read minimap screenshot").to_rgba8();
        let bounds = minimap_thumb_bounds(&image, minimap, body, selection);
        let thumb_height = bounds.end - bounds.start;
        assert!(minimap[2] > 0.0 && minimap[3] > 0.0);
        let minimap_top = minimap[1].round() as u32;
        assert!(
            bounds.start + 2 >= minimap_top && bounds.start <= minimap_top + 2,
            "thumb should start at the minimap top: bounds={bounds:?}, minimap={minimap:?}"
        );
        assert!(bounds.end <= (minimap[1] + minimap[3]).round() as u32 + 2);
        assert!(
            thumb_height as f64 >= body[3] * 0.03
                && thumb_height as f64 <= body[3] * 0.09,
            "thumb height should reflect the visible viewport: {width}x{height}, bounds={bounds:?}, body={body:?}"
        );
        thumb_heights.push(thumb_height as f64);
    }

    let scale_ratio = thumb_heights[1] / thumb_heights[0];
    assert!(
        (scale_ratio - 2.0).abs() < 0.15,
        "doubling viewport height should double the thumb height: {thumb_heights:?}"
    );
    let _ = std::fs::remove_dir_all(dir);
}
