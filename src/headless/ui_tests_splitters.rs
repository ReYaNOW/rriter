//! Headless UI coverage for IDE splitters and the status bar.

use crate::headless::tests_support::{
    click_ui, dump, has_ui, run_script, sample_file, scratch_dir, ui_rect,
    workspace_with_explorer,
};

const TEST_SCALE: f32 = 4.0 / 3.0;

fn ide_session(
    width: u32,
    height: u32,
    name: &str,
) -> (std::path::PathBuf, crate::headless::HeadlessSession) {
    let dir = scratch_dir(name);
    let file = sample_file(&dir);
    let mut session = workspace_with_explorer(width, height, TEST_SCALE, &dir);
    let lines = run_script(&mut session, format!("open {}\n", file.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    (dir, session)
}

fn drag_splitter(
    session: &mut crate::headless::HeadlessSession,
    id: &str,
    target_x: f64,
    target_y: f64,
) {
    let [x, y, width, height] = ui_rect(&dump(session), id);
    let start_x = x + width / 2.0;
    let start_y = y + height / 2.0;
    let lines = run_script(
        session,
        format!(
            "mouse_move {start_x} {start_y}\nclick down\nmouse_move {target_x} {target_y}\nclick up\n"
        )
        .as_bytes(),
    );
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
}

fn assert_rect_inside(inner: [f64; 4], outer: [f64; 4], label: &str) {
    let [x, y, width, height] = inner;
    let [outer_x, outer_y, outer_width, outer_height] = outer;
    assert!(
        x >= outer_x - 0.5
            && y >= outer_y - 0.5
            && x + width <= outer_x + outer_width + 0.5
            && y + height <= outer_y + outer_height + 0.5,
        "{label} {inner:?} outside {outer:?}"
    );
}

#[test]
fn headless_splitters_bottom_drag_changes_height_and_survives_toggle() {
    let (dir, mut session) = ide_session(1280, 720, "ui-splitter-bottom-height");
    click_ui(&mut session, "SidebarSlot(Problems)");

    let before = ui_rect(&dump(&mut session), "BottomPanelBody");
    let resize = ui_rect(&dump(&mut session), "ResizeBottom");
    drag_splitter(
        &mut session,
        "ResizeBottom",
        resize[0] + resize[2] / 2.0,
        resize[1] - 90.0,
    );
    let resized = ui_rect(&dump(&mut session), "BottomPanelBody");
    assert_ne!(resized[3], before[3], "bottom panel height did not change");

    click_ui(&mut session, "SidebarSlot(Problems)");
    assert!(!has_ui(&dump(&mut session), "BottomPanelBody"));
    click_ui(&mut session, "SidebarSlot(Problems)");
    let reopened = ui_rect(&dump(&mut session), "BottomPanelBody");
    assert!(
        (reopened[3] - resized[3]).abs() <= 1.0,
        "height changed after toggle: {resized:?} -> {reopened:?}"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_splitters_bottom_panel_stays_bounded_at_window_edges() {
    let (dir, mut session) = ide_session(1280, 720, "ui-splitter-bottom-bounds");
    click_ui(&mut session, "SidebarSlot(Problems)");

    drag_splitter(&mut session, "ResizeBottom", 640.0, 719.0);
    let at_bottom = dump(&mut session);
    let panel = ui_rect(&at_bottom, "BottomPanelBody");
    let window = [0.0, 0.0, 1280.0, 720.0];
    assert_rect_inside(panel, window, "bottom panel at lower edge");
    assert!(panel[3] >= 70.0, "bottom panel collapsed: {panel:?}");
    assert!(has_ui(&at_bottom, "ResizeBottom"));

    drag_splitter(&mut session, "ResizeBottom", 640.0, 0.0);
    let at_top = dump(&mut session);
    let panel = ui_rect(&at_top, "BottomPanelBody");
    assert_rect_inside(panel, window, "bottom panel at upper edge");
    assert!(
        panel[1] >= 48.0 && panel[3] > 0.0,
        "bottom panel escaped its upper bound: {panel:?}"
    );
    assert!(has_ui(&at_top, "ResizeBottom"));

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_splitters_side_panel_stays_bounded_at_window_edges() {
    let (dir, mut session) = ide_session(1280, 720, "ui-splitter-side-bounds");

    drag_splitter(&mut session, "ResizeLeft", 0.0, 120.0);
    let at_left = dump(&mut session);
    let min_width = at_left["ide_panel"]["width"].as_f64().unwrap();
    assert!(min_width >= 64.0, "side panel collapsed: {min_width}");
    assert!(has_ui(&at_left, "ResizeLeft"));

    drag_splitter(&mut session, "ResizeLeft", 1279.0, 120.0);
    let at_right = dump(&mut session);
    let max_width = at_right["ide_panel"]["width"].as_f64().unwrap();
    assert!(max_width > min_width, "side splitter did not move: {min_width} -> {max_width}");
    assert!(
        max_width < 1280.0 / f64::from(TEST_SCALE),
        "side panel extends past the window: {max_width}"
    );
    let splitter = ui_rect(&at_right, "ResizeLeft");
    assert_rect_inside(splitter, [0.0, 0.0, 1280.0, 720.0], "side splitter at right edge");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_status_bar_stays_at_window_bottom_at_two_sizes() {
    for (width, height, name) in [
        (1280, 720, "ui-status-bar-1280"),
        (2560, 1440, "ui-status-bar-2560"),
    ] {
        let (dir, mut session) = ide_session(width, height, name);
        click_ui(&mut session, "SidebarSlot(Problems)");
        let state = dump(&mut session);
        let status_bar = ui_rect(&state, "StatusBar");
        let window = [0.0, 0.0, f64::from(width), f64::from(height)];

        assert_rect_inside(status_bar, window, "status bar");
        assert!((status_bar[1] + status_bar[3] - f64::from(height)).abs() <= 0.5);
        let status_items = state["ui"].as_array().unwrap().iter().filter(|element| {
            element["id"].as_str().is_some_and(|id| {
                id.starts_with("Status") && id != "StatusBar"
            })
        });
        let mut item_count = 0;
        for item in status_items {
            item_count += 1;
            let id = item["id"].as_str().unwrap();
            assert_rect_inside(ui_rect(&state, id), status_bar, id);
        }
        assert!(item_count > 0, "status bar has no registered items: {state}");

        let _ = std::fs::remove_dir_all(dir);
    }
}
