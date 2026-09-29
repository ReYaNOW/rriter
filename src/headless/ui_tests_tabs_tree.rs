use crate::headless::tests_support::{
    assert_rect_inside_window, assert_ui_y_integral, click_ui, dump, has_ui, ok_json, run_script,
    scratch_dir, session_for_test, ui_center, ui_rect, wait_until, workspace_with_explorer,
};

#[test]
fn headless_tabs_open_counts_and_active_close_switches_to_previous() {
    let dir = scratch_dir("ui-tabs-counts");
    for i in 0..20 {
        std::fs::write(dir.join(format!("tab-{i:02}.txt")), format!("file {i}\n")).unwrap();
    }
    for count in [1, 5, 20] {
        let mut session = session_for_test(1280, 800);
        let lines = run_script(
            &mut session,
            format!("workspace {}\nsettle 2000\n", dir.display()).as_bytes(),
        );
        assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
        let mut script = String::new();
        for i in 0..count {
            script.push_str(&format!(
                "open {}\nsettle 2000\n",
                dir.join(format!("tab-{i:02}.txt")).display()
            ));
        }
        run_script(&mut session, script.as_bytes());
        let state = dump(&mut session);
        assert_eq!(state["tabs"].as_array().unwrap().len(), count);
        if count > 1 {
            let old_active = state["tabs"][count - 1]["path"].clone();
            let tab_id = format!("EditorTab({})", count - 1);
            let (x, y) = ui_center(&state, &tab_id);
            run_script(&mut session, format!("mouse_move {x} {y}\nsettle 2000\n").as_bytes());
            click_ui(&mut session, &format!("EditorTabClose({})", count - 1));
            run_script(&mut session, b"settle 2000\n");
            let after = dump(&mut session);
            assert_eq!(after["tabs"].as_array().unwrap().len(), count - 1);
            assert_eq!(after["tabs"][count - 2]["active"], true);
            assert_ne!(after["tabs"][count - 2]["path"], old_active);
        }
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_tabs_closing_inactive_tab_preserves_active_tab() {
    let dir = scratch_dir("ui-tabs-inactive");
    let first = dir.join("first.txt");
    let second = dir.join("second.txt");
    std::fs::write(&first, "first\n").unwrap();
    std::fs::write(&second, "second\n").unwrap();
    let mut session = session_for_test(1280, 800);
    let lines = run_script(
        &mut session,
        format!(
            "workspace {}\nsettle 2000\nopen {}\nsettle 2000\nopen {}\nsettle 2000\n",
            dir.display(),
            first.display(),
            second.display()
        )
        .as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let before = dump(&mut session);
    assert_eq!(before["tabs"][1]["active"], true);
    let (tab_x, tab_y) = ui_center(&before, "EditorTab(0)");
    run_script(&mut session, format!("mouse_move {tab_x} {tab_y}\nsettle 2000\n").as_bytes());
    click_ui(&mut session, "EditorTabClose(0)");
    run_script(&mut session, b"settle 2000\n");
    let after = dump(&mut session);
    assert_eq!(after["tabs"].as_array().unwrap().len(), 1);
    assert_eq!(after["tabs"][0]["active"], true);
    assert_eq!(after["tabs"][0]["path"], second.display().to_string());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_tabs_dirty_close_dialog_discard_closes_tab() {
    let dir = scratch_dir("ui-tabs-dirty");
    let file = dir.join("dirty.txt");
    std::fs::write(&file, "original\n").unwrap();
    let mut session = session_for_test(1280, 800);
    let lines = run_script(
        &mut session,
        format!(
            "workspace {}\nsettle 2000\nopen {}\nsettle 2000\ntype changed\nsettle 2000\n",
            dir.display(),
            file.display()
        )
        .as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    run_script(&mut session, b"key ctrl+4\nsettle 2000\n");
    assert_eq!(dump(&mut session)["dialog"]["action"], "CloseTab");
    run_script(&mut session, b"dialog discard\n");
    run_script(&mut session, b"settle 2000\n");
    assert!(dump(&mut session)["tabs"].as_array().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_tree_250_files_expands_and_scrolls_within_budget() {
    let dir = scratch_dir("ui-tree-many");
    let folder = dir.join("folder");
    std::fs::create_dir_all(&folder).unwrap();
    for i in 0..250 {
        std::fs::write(folder.join(format!("item-{i:03}.txt")), "x\n").unwrap();
    }
    let mut session = workspace_with_explorer(1280, 800, 1.0, &dir);
    let state = dump(&mut session);
    let arrow = state["ui"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"].as_str().unwrap_or("").starts_with("FileTreeArrow("))
        .unwrap();
    let id = arrow["id"].as_str().unwrap().to_string();
    click_ui(&mut session, &id);
    run_script(&mut session, b"settle 2000\n");
    let expanded = dump(&mut session);
    assert!(expanded["ui"].as_array().unwrap().iter().any(|e| {
        e["id"].as_str().unwrap_or("").starts_with("FileTreeNode(")
    }));
    let out = dir.join("tree-scroll");
    let lines = run_script(&mut session, format!("record 5 {} wheel 0 -3\n", out.display()).as_bytes());
    let summary = ok_json(&lines[0]);
    assert_cpu_frames_within_budget(&summary);
    let _ = std::fs::remove_dir_all(dir);
}

/// Frame budget on the app's own work: `update_ms + draw_cpu_ms` of every recorded frame.
/// `total_ms` (and so `over_budget`) also holds the `glFinish` wait, which grows with every
/// other GL context on the GPU (parallel test processes, a game): on the same frames it goes
/// from ~0.2 ms alone to 4-5 ms next to 8 other contexts while the CPU part stays ~0.1 ms, so
/// it cannot be asserted in a parallel suite. In-frame work (e.g. a synchronous svg
/// rasterization) lands in `draw_cpu_ms` and still fails this check.
fn assert_cpu_frames_within_budget(summary: &serde_json::Value) {
    let budget = summary["budget_ms"].as_f64().unwrap_or(0.0);
    assert!(budget > 0.0, "{summary}");
    let Ok(csv) = std::fs::read_to_string(summary["csv"].as_str().unwrap_or_default()) else {
        panic!("record csv missing: {summary}");
    };
    let rows: Vec<&str> = csv.lines().skip(1).collect();
    assert_eq!(Some(rows.len() as u64), summary["frames"].as_u64(), "{csv}");
    for row in rows {
        // A malformed cell parses to NaN and fails the comparison below.
        let cells: Vec<f64> = row.split(',').take(3).map(|cell| cell.parse().unwrap_or(f64::NAN)).collect();
        assert!(cells.len() == 3 && cells[1] + cells[2] <= budget, "frame over budget on CPU: {row}\n{summary}");
    }
}

#[test]
fn headless_tree_long_filename_single_and_double_click() {
    let dir = scratch_dir("ui-tree-long-name");
    let name = format!("{}.txt", "long-name".repeat(14));
    let file = dir.join(&name);
    std::fs::write(&file, "line one\nline two\n").unwrap();
    let mut session = workspace_with_explorer(1280, 800, 1.0, &dir);
    let state = dump(&mut session);
    let node = state["ui"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| {
            let Some(id) = e["id"].as_str() else {
                return false;
            };
            let Some(index) = id
                .strip_prefix("FileTreeNode(")
                .and_then(|id| id.strip_suffix(')'))
            else {
                return false;
            };
            !has_ui(&state, &format!("FileTreeArrow({index})"))
        })
        .unwrap();
    let id = node["id"].as_str().unwrap().to_string();
    click_ui(&mut session, &id);
    run_script(&mut session, b"settle 2000\n");
    let clicked = dump(&mut session);
    assert!(clicked["hover"]["ui"].is_string() || has_ui(&clicked, &id));
    let (x, y) = ui_center(&clicked, &id);
    run_script(
        &mut session,
        format!("mouse_move {x} {y}\nsettle 2000\ndblclick\nsettle 2000\n").as_bytes(),
    );
    let opened = dump(&mut session);
    assert!(
        opened["tabs"].as_array().unwrap().iter().any(|tab| {
            tab["path"] == file.display().to_string()
        }),
        "{opened}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_tree_search_and_file_open_fixture_paths_are_temporary() {
    let dir = scratch_dir("ui-tree-isolation");
    let nested = dir.join("src");
    std::fs::create_dir_all(&nested).unwrap();
    let file = nested.join("entry.txt");
    std::fs::write(&file, "temporary fixture\n").unwrap();
    let mut session = workspace_with_explorer(900, 600, 1.0, &dir);
    let state = dump(&mut session);
    assert_eq!(state["mode"], "ide");
    assert!(state["tabs"].as_array().unwrap().iter().all(|tab| tab["path"].is_null()), "{state}");
    assert!(state["ui"].as_array().unwrap().iter().any(|e| {
        e["id"].as_str().unwrap_or("").starts_with("FileTreeNode(")
    }));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_tree_expand_and_collapse_folder_restores_child_nodes() {
    let dir = scratch_dir("ui-tree-collapse");
    let folder = dir.join("group");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("child.txt"), "child\n").unwrap();
    let mut session = workspace_with_explorer(1280, 800, 1.0, &dir);
    let state = dump(&mut session);
    let arrow = state["ui"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["id"].as_str().unwrap_or("").starts_with("FileTreeArrow("))
        .last()
        .unwrap();
    let id = arrow["id"].as_str().unwrap().to_string();
    click_ui(&mut session, &id);
    run_script(&mut session, b"settle 2000\n");
    let expanded = dump(&mut session);
    let child_count = expanded["ui"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["id"].as_str().unwrap_or("").starts_with("FileTreeNode("))
        .count();
    click_ui(&mut session, &id);
    run_script(&mut session, b"settle 2000\n");
    let collapsed = dump(&mut session);
    let collapsed_count = collapsed["ui"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["id"].as_str().unwrap_or("").starts_with("FileTreeNode("))
        .count();
    assert!(child_count > collapsed_count);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_tree_record_wheel_keeps_frame_budget_at_common_scales() {
    let dir = scratch_dir("ui-tree-scroll-scales");
    for i in 0..80 {
        std::fs::write(dir.join(format!("tree-{i:03}.txt")), "item\n").unwrap();
    }
    for scale in [1.0, 1.5] {
        let mut session = workspace_with_explorer(1280, 800, scale, &dir);
        let output = dir.join(format!("scroll-{scale}"));
        let lines = run_script(&mut session, format!("record 4 {} wheel 0 -3\n", output.display()).as_bytes());
        let summary = ok_json(&lines[0]);
        assert_eq!(summary["frames"], 4);
        assert_cpu_frames_within_budget(&summary);
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_tabs_open_many_unique_fixture_files_without_path_leaks() {
    let dir = scratch_dir("ui-tabs-many-unique");
    let mut script = format!("workspace {}\nsettle 2000\n", dir.display());
    for i in 0..20 {
        let path = dir.join(format!("unique-{i:02}.txt"));
        std::fs::write(&path, format!("unique fixture {i}\n")).unwrap();
        script.push_str(&format!(
            "open {}\nsettle 2000\n",
            path.display()
        ));
    }
    let mut session = session_for_test(1920, 1080);
    run_script(&mut session, script.as_bytes());
    let state = dump(&mut session);
    assert_eq!(state["tabs"].as_array().unwrap().len(), 20);
    assert_eq!(state["tabs"][19]["active"], true);
    assert_eq!(state["tabs"][0]["active"], false);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_tree_fixture_directory_can_be_opened_at_fractional_scale() {
    let dir = scratch_dir("ui-tree-fractional-scale");
    std::fs::write(dir.join("root.txt"), "root\n").unwrap();
    let mut session = workspace_with_explorer(1280, 800, 1.5, &dir);
    let state = dump(&mut session);
    assert_eq!(state["mode"], "ide");
    assert_eq!(state["scale"], 1.5);
    assert!(state["ui"].as_array().unwrap().iter().any(|e| {
        e["id"].as_str().unwrap_or("").starts_with("FileTreeNode(")
    }));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_tree_dump_includes_scroll_track_for_large_fixture() {
    let dir = scratch_dir("ui-tree-scroll-track");
    for i in 0..100 {
        std::fs::write(dir.join(format!("scroll-node-{i:03}.txt")), "x\n").unwrap();
    }
    let mut session = workspace_with_explorer(800, 600, 1.0, &dir);
    let state = dump(&mut session);
    assert!(state["ui"].as_array().unwrap().iter().any(|e| e["id"] == "FileTreeScrollY"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_tree_and_tabs_dump_use_only_scratch_fixture_paths() {
    let dir = scratch_dir("ui-path-isolation");
    let file = dir.join("fixture-only.txt");
    std::fs::write(&file, "only local test fixture\n").unwrap();
    let mut session = session_for_test(1024, 768);
    run_script(&mut session, format!("workspace {}\nopen {}\nsettle 1000\n", dir.display(), file.display()).as_bytes());
    let state = dump(&mut session);
    assert_eq!(state["tabs"][0]["path"], file.display().to_string());
    assert!(state["tabs"][0]["path"].as_str().unwrap().contains("rriter-headless-ui-path-isolation-"));
    let _ = std::fs::remove_dir_all(dir);
}

fn tab_fixture_content(index: usize) -> String {
    format!("tab {index}\ncontent {index}\n")
}

fn write_tab_fixture(dir: &std::path::Path, index: usize) -> std::path::PathBuf {
    let path = dir.join(format!("tab-{index:02}.txt"));
    std::fs::write(&path, tab_fixture_content(index)).unwrap();
    path
}

fn open_tab_fixtures(
    session: &mut crate::headless::HeadlessSession,
    dir: &std::path::Path,
    count: usize,
) -> Vec<std::path::PathBuf> {
    let mut files = Vec::with_capacity(count);
    let mut script = format!("scale {}\nworkspace {}\nsettle 2000\n", 4.0f64 / 3.0, dir.display());
    for index in 0..count {
        let path = write_tab_fixture(dir, index);
        script.push_str(&format!("open {}\n", path.display()));
        files.push(path);
    }
    script.push_str("settle 2000\n");
    let lines = run_script(session, script.as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    files
}

fn tab_paths(state: &serde_json::Value) -> Vec<String> {
    state["tabs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tab| tab["path"].as_str().unwrap().to_string())
        .collect()
}

fn active_tab_index(state: &serde_json::Value) -> usize {
    state["tabs"]
        .as_array()
        .unwrap()
        .iter()
        .position(|tab| tab["active"] == true)
        .unwrap()
}

fn visible_tab_indexes(state: &serde_json::Value) -> Vec<usize> {
    state["ui"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|element| {
            element["id"]
                .as_str()?
                .strip_prefix("EditorTab(")?
                .strip_suffix(')')?
                .parse()
                .ok()
        })
        .collect()
}

fn assert_tab_hitboxes_fit(state: &serde_json::Value, width: u32, height: u32) {
    let mut visible_count = 0;
    for element in state["ui"].as_array().unwrap() {
        let Some(id) = element["id"].as_str() else {
            continue;
        };
        if !id.starts_with("EditorTab(") {
            continue;
        }
        visible_count += 1;
        let rect = ui_rect(state, id);
        assert_rect_inside_window(rect, width as f64, height as f64, 0.0, 0.01, id);
        assert_ui_y_integral(rect[1], 0.0, id);
    }
    assert!(visible_count > 0, "no visible editor tab hitboxes: {state}");
}

fn scroll_tab_strip(session: &mut crate::headless::HeadlessSession, width: u32, dy: i32) {
    let lines = run_script(
        session,
        format!("mouse_move {} 20\nwheel 0 {dy}\nsettle 2000\n", width / 2).as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
}

#[test]
fn headless_tabs_drag_active_reorders_and_preserves_content() {
    let dir = scratch_dir("ui-tabs-drag-active");
    let mut session = session_for_test(1280, 720);
    let files = open_tab_fixtures(&mut session, &dir, 4);
    let before = dump(&mut session);
    let (start_x, start_y) = ui_center(&before, "EditorTab(3)");
    let (_, target_y) = ui_center(&before, "EditorTab(0)");
    let target_x = ui_rect(&before, "EditorTab(0)")[0] + 4.0;
    let lines = run_script(
        &mut session,
        format!(
            "mouse_move {start_x} {start_y}\nclick left down\nmouse_move {target_x} {target_y}\nclick left up\nsettle 1000\n"
        )
        .as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");

    let after = dump(&mut session);
    assert_eq!(
        tab_paths(&after),
        vec![
            files[3].display().to_string(),
            files[0].display().to_string(),
            files[1].display().to_string(),
            files[2].display().to_string(),
        ]
    );
    assert_eq!(active_tab_index(&after), 0);
    assert_eq!(session.app.editor.get_full_text(), tab_fixture_content(3));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_tabs_drag_clamps_at_both_ends_and_click_does_not_reorder() {
    let dir = scratch_dir("ui-tabs-drag-edges");
    let mut session = session_for_test(1280, 720);
    let files = open_tab_fixtures(&mut session, &dir, 4);
    let before = dump(&mut session);
    let (start_x, start_y) = ui_center(&before, "EditorTab(3)");
    let lines = run_script(
        &mut session,
        format!(
            "mouse_move {start_x} {start_y}\nclick left down\nmouse_move 64 {start_y}\nclick left up\nsettle 1000\n"
        )
        .as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let first = dump(&mut session);
    assert_eq!(tab_paths(&first)[0], files[3].display().to_string());
    assert_eq!(active_tab_index(&first), 0);

    let (start_x, start_y) = ui_center(&first, "EditorTab(0)");
    let lines = run_script(
        &mut session,
        format!(
            "mouse_move {start_x} {start_y}\nclick left down\nmouse_move 1279 {start_y}\nclick left up\nsettle 1000\n"
        )
        .as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let last = dump(&mut session);
    assert_eq!(tab_paths(&last)[3], files[3].display().to_string());
    assert_eq!(active_tab_index(&last), 3);
    let order_before_click = tab_paths(&last);

    let (click_x, click_y) = ui_center(&last, "EditorTab(1)");
    let lines = run_script(
        &mut session,
        format!("mouse_move {click_x} {click_y}\nclick\nsettle 500\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let clicked = dump(&mut session);
    assert_eq!(tab_paths(&clicked), order_before_click);
    assert_eq!(active_tab_index(&clicked), 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_tabs_overflow_wheel_clamps_reveals_new_file_and_keeps_hitboxes_inside_1280x720() {
    const TAB_COUNT: usize = 32;
    let dir = scratch_dir("ui-tabs-overflow-1280");
    let mut session = session_for_test(1280, 720);
    let files = open_tab_fixtures(&mut session, &dir, TAB_COUNT);
    let opened = dump(&mut session);
    assert_eq!(opened["tabs"].as_array().unwrap().len(), TAB_COUNT);
    assert_eq!(active_tab_index(&opened), TAB_COUNT - 1);
    let at_end = visible_tab_indexes(&opened);
    assert!(at_end.len() < TAB_COUNT, "fixture tabs did not overflow: {at_end:?}");
    assert!(at_end.contains(&(TAB_COUNT - 1)), "new active tab is not visible: {at_end:?}");
    assert_tab_hitboxes_fit(&opened, 1280, 720);

    scroll_tab_strip(&mut session, 1280, 10_000);
    let at_start = dump(&mut session);
    let start_indexes = visible_tab_indexes(&at_start);
    assert_eq!(start_indexes.first(), Some(&0));
    assert!(!start_indexes.contains(&(TAB_COUNT - 1)));
    scroll_tab_strip(&mut session, 1280, 10_000);
    assert_eq!(visible_tab_indexes(&dump(&mut session)), start_indexes);

    scroll_tab_strip(&mut session, 1280, -10_000);
    let at_end_again = dump(&mut session);
    let end_indexes = visible_tab_indexes(&at_end_again);
    assert_eq!(end_indexes.last(), Some(&(TAB_COUNT - 1)));
    scroll_tab_strip(&mut session, 1280, -10_000);
    assert_eq!(visible_tab_indexes(&dump(&mut session)), end_indexes);

    scroll_tab_strip(&mut session, 1280, 10_000);
    let new_file = write_tab_fixture(&dir, TAB_COUNT);
    let lines = run_script(
        &mut session,
        format!("open {}\nsettle 1000\n", new_file.display()).as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let after_open = dump(&mut session);
    assert_eq!(after_open["tabs"].as_array().unwrap().len(), TAB_COUNT + 1);
    assert_eq!(active_tab_index(&after_open), TAB_COUNT);
    assert!(visible_tab_indexes(&after_open).contains(&TAB_COUNT));
    assert_eq!(after_open["tabs"][TAB_COUNT]["path"], new_file.display().to_string());
    assert_eq!(session.app.editor.get_full_text(), tab_fixture_content(TAB_COUNT));
    assert_tab_hitboxes_fit(&after_open, 1280, 720);
    assert_eq!(files.len(), TAB_COUNT);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_tabs_overflow_hitboxes_fit_2560x1440_at_fractional_scale() {
    const TAB_COUNT: usize = 32;
    let dir = scratch_dir("ui-tabs-overflow-2560");
    let mut session = session_for_test(2560, 1440);
    let files = open_tab_fixtures(&mut session, &dir, TAB_COUNT);
    let state = dump(&mut session);
    assert_eq!(state["size"][0], 2560);
    assert!((state["scale"].as_f64().unwrap() - 4.0 / 3.0).abs() < 0.00001);
    assert_eq!(state["tabs"].as_array().unwrap().len(), files.len());
    assert!(visible_tab_indexes(&state).len() < files.len(), "fixture tabs did not overflow");
    assert!(visible_tab_indexes(&state).contains(&(files.len() - 1)));
    assert_tab_hitboxes_fit(&state, 2560, 1440);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_tabs_drag_autoscrolls_and_preserves_active_content() {
    const TAB_COUNT: usize = 32;
    let dir = scratch_dir("ui-tabs-drag-autoscroll");
    let mut session = session_for_test(1280, 720);
    let files = open_tab_fixtures(&mut session, &dir, TAB_COUNT);
    scroll_tab_strip(&mut session, 1280, 10_000);
    let at_start = dump(&mut session);
    assert_eq!(visible_tab_indexes(&at_start).first(), Some(&0));

    click_ui(&mut session, "EditorTab(1)");
    let selected = dump(&mut session);
    assert_eq!(active_tab_index(&selected), 1);
    let (start_x, start_y) = ui_center(&selected, "EditorTab(1)");
    let lines = run_script(
        &mut session,
        format!(
            "mouse_move {start_x} {start_y}\nclick left down\nmouse_move 1278 {start_y}\nwait 1200\n"
        )
        .as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let while_dragging = dump(&mut session);
    assert!(visible_tab_indexes(&while_dragging).first().unwrap() > &0);
    assert_tab_hitboxes_fit(&while_dragging, 1280, 720);

    let lines = run_script(&mut session, b"click left up\nsettle 1500\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let after = dump(&mut session);
    assert_ne!(tab_paths(&after), files.iter().map(|path| path.display().to_string()).collect::<Vec<_>>());
    let active = active_tab_index(&after);
    assert!(active > 1);
    assert_eq!(after["tabs"][active]["path"], files[1].display().to_string());
    assert_eq!(session.app.editor.get_full_text(), tab_fixture_content(1));
    assert!(visible_tab_indexes(&after).contains(&active));
    assert_tab_hitboxes_fit(&after, 1280, 720);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_bug_dragging_inactive_editor_tab_reorders_and_activates_it() {
    let dir = scratch_dir("ui-tabs-drag-inactive");
    let mut session = session_for_test(1280, 720);
    let files = open_tab_fixtures(&mut session, &dir, 4);
    let before = dump(&mut session);
    let (start_x, start_y) = ui_center(&before, "EditorTab(0)");
    let lines = run_script(
        &mut session,
        format!(
            "mouse_move {start_x} {start_y}\nclick left down\nmouse_move 1279 {start_y}\nclick left up\nsettle 1000\n"
        )
        .as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let after = dump(&mut session);
    assert_eq!(
        tab_paths(&after),
        vec![
            files[1].display().to_string(),
            files[2].display().to_string(),
            files[3].display().to_string(),
            files[0].display().to_string(),
        ]
    );
    assert_eq!(active_tab_index(&after), 3);
    assert_eq!(session.app.editor.get_full_text(), tab_fixture_content(0));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_keyboard_activation_reveals_offscreen_tab() {
    const TAB_COUNT: usize = 32;
    let dir = scratch_dir("ui-tabs-keyboard-switch");
    let mut session = session_for_test(1280, 720);
    let files = open_tab_fixtures(&mut session, &dir, TAB_COUNT);
    scroll_tab_strip(&mut session, 1280, 10_000);
    click_ui(&mut session, "EditorTab(0)");
    let at_start = dump(&mut session);
    let initially_visible = visible_tab_indexes(&at_start);
    let mut selected_offscreen = None;

    for step in 0..TAB_COUNT {
        let lines = run_script(&mut session, b"key ctrl+pagedown\n");
        assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
        let state = dump(&mut session);
        let active = active_tab_index(&state);
        assert_eq!(active, (step + 1) % TAB_COUNT);
        if !initially_visible.contains(&active) {
            selected_offscreen = Some((active, state));
            break;
        }
    }

    let (active, state) = selected_offscreen.expect("Ctrl+PageDown did not select an offscreen tab");
    assert!(visible_tab_indexes(&state).contains(&active));
    assert_eq!(state["tabs"][active]["path"], files[active].display().to_string());
    assert_eq!(session.app.editor.get_full_text(), tab_fixture_content(active));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_keyboard_tab_switch_wraps_and_reveals_ends() {
    const TAB_COUNT: usize = 32;
    let dir = scratch_dir("ui-tabs-keyboard-wrap");
    let mut session = session_for_test(1280, 720);
    open_tab_fixtures(&mut session, &dir, TAB_COUNT);
    assert_eq!(active_tab_index(&dump(&mut session)), TAB_COUNT - 1);

    run_script(&mut session, b"key ctrl+pagedown\n");
    let first = dump(&mut session);
    assert_eq!(active_tab_index(&first), 0);
    assert!(visible_tab_indexes(&first).contains(&0));

    run_script(&mut session, b"key ctrl+pageup\n");
    let last = dump(&mut session);
    assert_eq!(active_tab_index(&last), TAB_COUNT - 1);
    assert!(visible_tab_indexes(&last).contains(&(TAB_COUNT - 1)));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_terminal_plain_page_keys_do_not_switch_editor_tabs() {
    let dir = scratch_dir("ui-tabs-terminal-page-keys");
    let mut session = session_for_test(1280, 720);
    open_tab_fixtures(&mut session, &dir, 2);
    let terminal_was_open = session
        .app
        .ide_panel
        .is_open(crate::app::PanelId::Terminal);
    let terminal_was_focused = session.app.ide_panel.terminal_focused;
    if !terminal_was_open {
        click_ui(&mut session, "SidebarSlot(Terminal)");
    }
    wait_until(&mut session, 3000, "terminal body", |session| {
        has_ui(&dump(session), "TerminalBody")
    });
    let state = dump(&mut session);
    let (x, y) = ui_center(&state, "TerminalBody");
    run_script(&mut session, format!("mouse_move {x} {y}\nclick\n").as_bytes());
    assert!(session.app.ide_panel.terminal_focused);
    let active = active_tab_index(&dump(&mut session));
    run_script(&mut session, b"key pagedown\nkey pageup\n");
    assert_eq!(active_tab_index(&dump(&mut session)), active);
    run_script(&mut session, b"key ctrl+pagedown\n");
    assert_eq!(active_tab_index(&dump(&mut session)), (active + 1) % 2);
    if terminal_was_open {
        session.app.ide_panel.terminal_focused = terminal_was_focused;
    } else {
        click_ui(&mut session, "SidebarSlot(Terminal)");
    }
    let _ = std::fs::remove_dir_all(dir);
}
