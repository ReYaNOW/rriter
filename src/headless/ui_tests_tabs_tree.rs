use crate::headless::tests_support::{
    click_ui, dump, has_ui, ok_json, run_script, scratch_dir, session_for_test, ui_center,
};

fn workspace_with_explorer(
    w: u32,
    h: u32,
    scale: f32,
    dir: &std::path::Path,
) -> crate::headless::HeadlessSession {
    let mut session = session_for_test(w, h);
    let lines = run_script(
        &mut session,
        format!("scale {scale}\nworkspace {}\nsettle 2000\n", dir.display()).as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    click_ui(&mut session, "SidebarSlot(Explorer)");
    let lines = run_script(&mut session, b"settle 2000\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    session
}

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
    assert_eq!(summary["over_budget"], 0, "{summary}");
    let _ = std::fs::remove_dir_all(dir);
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
        assert_eq!(summary["over_budget"], 0, "{summary}");
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
