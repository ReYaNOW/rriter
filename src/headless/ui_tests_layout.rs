use crate::headless::tests_support::{
    click_ui, dump, ok_json, run_script, sample_file, scratch_dir, session_for_test,
};

fn open_file(session: &mut crate::headless::HeadlessSession, path: &std::path::Path) {
    let lines = run_script(session, format!("open {}\nsettle 2000\n", path.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
}

#[test]
fn headless_layout_welcome_file_and_tree_size_scale_matrix() {
    let dir = scratch_dir("ui-layout-matrix");
    let file = sample_file(&dir);
    let tree = dir.join("folder");
    std::fs::create_dir_all(&tree).unwrap();
    std::fs::write(tree.join("child.txt"), "child\n").unwrap();
    for mode in ["welcome", "file", "tree"] {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for (w, h) in [(640, 480), (2560, 1440)] {
                let mut session = session_for_test(w, h);
                let mut script = format!("scale {scale}\n");
                if mode == "file" { script.push_str(&format!("open {}\n", file.display())); }
                if mode == "tree" { script.push_str(&format!("workspace {}\n", dir.display())); }
                script.push_str("settle 2000\ndump\n");
                let lines = run_script(&mut session, script.as_bytes());
                let state = ok_json(lines.last().unwrap());
                assert_eq!(state["size"], serde_json::json!([w, h]));
                assert_eq!(state["scale"].as_f64().unwrap(), scale as f64);
                assert_eq!(state["mode"], if mode == "welcome" { "welcome" } else if mode == "tree" { "ide" } else { "editor" });
                let settle = lines.iter().find(|line| line.starts_with("ok frames=")).unwrap();
                assert!(settle.ends_with("settled=true"), "{mode} {w}x{h}@{scale}: {lines:?}");
            }
        }
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_layout_resize_and_scale_on_open_file() {
    let dir = scratch_dir("ui-layout-resize");
    let file = sample_file(&dir);
    let mut session = session_for_test(640, 480);
    open_file(&mut session, &file);
    let lines = run_script(&mut session, b"resize 400x300\nresize 2560x1440\nscale 2\nscale 1.25\nsettle 2000\ndump\n");
    assert_eq!(lines[0], "ok 400x300");
    assert_eq!(lines[1], "ok 2560x1440");
    assert!(lines[4].ends_with("settled=true"), "{lines:?}");
    let state = ok_json(lines.last().unwrap());
    assert_eq!(state["size"], serde_json::json!([2560, 1440]));
    assert_eq!(state["scale"], 1.25);
    assert_eq!(state["tabs"].as_array().unwrap().len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_layout_small_large_framebuffer_reports_exact_size_and_scale() {
    for (w, h, scale) in [(640, 480, 1.0), (640, 480, 1.25), (640, 480, 1.5), (640, 480, 2.0), (2560, 1440, 1.0), (2560, 1440, 1.25), (2560, 1440, 1.5), (2560, 1440, 2.0)] {
        let mut session = session_for_test(w, h);
        let lines = run_script(&mut session, format!("scale {scale}\nsettle 2000\ndump\n").as_bytes());
        let state = ok_json(lines.last().unwrap());
        assert_eq!(state["size"], serde_json::json!([w, h]));
        assert_eq!(state["scale"].as_f64().unwrap(), scale as f64);
        assert!(lines[1].ends_with("settled=true"), "{w}x{h}@{scale}: {lines:?}");
    }
}

#[test]
fn headless_layout_resize_preserves_open_tab_and_scale_order() {
    let dir = scratch_dir("ui-layout-resize-order");
    let file = sample_file(&dir);
    let mut session = session_for_test(900, 600);
    open_file(&mut session, &file);
    for command in ["resize 640x480", "scale 1.25", "resize 2560x1440", "scale 2", "scale 1"] {
        let lines = run_script(&mut session, format!("{command}\nsettle 2000\n").as_bytes());
        assert!(lines.iter().all(|line| line.starts_with("ok")), "{command}: {lines:?}");
        assert!(lines.last().unwrap().ends_with("settled=true"), "{command}: {lines:?}");
    }
    let state = dump(&mut session);
    assert_eq!(state["size"], serde_json::json!([2560, 1440]));
    assert_eq!(state["scale"], 1.0);
    assert_eq!(state["tabs"][0]["path"], file.display().to_string());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_layout_welcome_dump_has_no_file_tabs() {
    for size in [(640, 480), (2560, 1440)] {
        let mut session = session_for_test(size.0, size.1);
        run_script(&mut session, b"settle 2000\n");
        let state = dump(&mut session);
        assert_eq!(state["mode"], "welcome");
        assert!(state["tabs"].as_array().unwrap().is_empty());
        assert_eq!(state["size"], serde_json::json!([size.0, size.1]));
    }
}

#[test]
fn headless_layout_file_and_tree_keep_requested_scale_through_resize() {
    let dir = scratch_dir("ui-layout-file-tree-scale");
    let file = sample_file(&dir);
    for mode in ["file", "tree"] {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let mut session = session_for_test(640, 480);
            let mut script = format!("scale {scale}\n");
            if mode == "file" {
                script.push_str(&format!("open {}\nsettle 2000\n", file.display()));
            }
            if mode == "tree" {
                script.push_str(&format!("workspace {}\nsettle 2000\n", dir.display()));
            }
            let lines = run_script(&mut session, script.as_bytes());
            assert!(lines.iter().all(|line| line.starts_with("ok")), "{mode}: {lines:?}");
            if mode == "tree" {
                click_ui(&mut session, "SidebarSlot(Explorer)");
                run_script(&mut session, b"settle 2000\n");
            }
            let resize_lines = run_script(&mut session, b"resize 2560x1440\nsettle 2000\ndump\n");
            assert_eq!(resize_lines[0], "ok 2560x1440");
            let settled = resize_lines.iter().find(|line| line.starts_with("ok frames="));
            assert!(settled.is_some_and(|line| line.ends_with("settled=true")), "{resize_lines:?}");
            let state = ok_json(resize_lines.last().unwrap());
            assert_eq!(state["size"], serde_json::json!([2560, 1440]));
            assert_eq!(state["scale"].as_f64().unwrap(), scale as f64);
            assert_eq!(state["mode"], if mode == "file" { "editor" } else { "ide" });
        }
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_layout_mode_survives_repeated_resize_cycle() {
    let dir = scratch_dir("ui-layout-repeated-resize");
    let file = sample_file(&dir);
    let mut session = session_for_test(640, 480);
    open_file(&mut session, &file);
    for (w, h) in [(400, 300), (2560, 1440), (640, 480), (1920, 1080)] {
        let lines = run_script(&mut session, format!("resize {w}x{h}\nsettle 2000\ndump\n").as_bytes());
        assert!(lines[1].ends_with("settled=true"), "{w}x{h}: {lines:?}");
        let state = ok_json(lines.last().unwrap());
        assert_eq!(state["size"], serde_json::json!([w, h]));
        assert_eq!(state["mode"], "editor");
        assert_eq!(state["tabs"].as_array().unwrap().len(), 1);
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_layout_minimum_size_resize_returns_ok_and_updates_dump() {
    let dir = scratch_dir("ui-layout-min-resize");
    let file = sample_file(&dir);
    let mut session = session_for_test(640, 480);
    open_file(&mut session, &file);
    let lines = run_script(&mut session, b"resize 400x300\nsettle 2000\ndump\n");
    assert_eq!(lines[0], "ok 400x300");
    assert!(lines[1].ends_with("settled=true"), "{lines:?}");
    assert_eq!(ok_json(lines.last().unwrap())["size"], serde_json::json!([400, 300]));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_layout_large_scale_scroll_record_and_4k_bench_summary() {
    let dir = scratch_dir("ui-layout-bench");
    let path = dir.join("long.txt");
    let text: String = (0..1000).map(|i| format!("line {i:04} long scrolling content\n")).collect();
    std::fs::write(&path, text).unwrap();
    for scale in [1.25, 1.5, 2.0] {
        let mut session = session_for_test(1280, 800);
        run_script(&mut session, format!("scale {scale}\nopen {}\nsettle 2000\n", path.display()).as_bytes());
        let out = dir.join(format!("scale-{scale}"));
        let lines = run_script(&mut session, format!("record 5 {} wheel 0 -3\n", out.display()).as_bytes());
        let summary = ok_json(&lines[0]);
        assert_eq!(summary["motion"]["nonmonotonic_frames"], 0, "{summary}");
    }
    let mut session = session_for_test(3840, 2160);
    run_script(&mut session, b"scale 2\n");
    let lines = run_script(&mut session, b"bench 3\n");
    let summary = ok_json(&lines[0]);
    assert_eq!(summary["frames"], 3);
    assert!(summary["over_budget"].is_number());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
#[ignore = "kanri j38ars8wtrqhwaf9q0837wtp: welcome hitboxes leave compact window"]
fn headless_bug_welcome_rectangles_inside_compact_windows() {
    for (w, h, scale) in [(400, 300, 1.0), (640, 480, 2.0)] {
        let mut session = session_for_test(w, h);
        run_script(&mut session, format!("scale {scale}\n").as_bytes());
        let state = dump(&mut session);
        for element in state["ui"].as_array().unwrap().iter().filter(|e| e["id"].as_str().unwrap_or("").starts_with("Welcome")) {
            let r = element["rect"].as_array().unwrap();
            assert!(r[0].as_f64().unwrap() + r[2].as_f64().unwrap() <= w as f64, "{} {r:?}", element["id"]);
            assert!(r[1].as_f64().unwrap() + r[3].as_f64().unwrap() <= h as f64, "{} {r:?}", element["id"]);
        }
    }
}

#[test]
#[ignore = "kanri pdogejwchp8es8frldky27mg: status bar moves below framebuffer when bottom panel is open"]
fn headless_bug_status_bar_stays_inside_with_bottom_panel() {
    let dir = scratch_dir("ui-bug-status-bar");
    let mut session = session_for_test(1920, 1080);
    run_script(&mut session, format!("workspace {}\nscale 1.25\n", dir.display()).as_bytes());
    let state = dump(&mut session);
    let slot = state["ui"].as_array().unwrap().iter().find(|e| e["id"] == "SidebarSlot(Problems)").unwrap();
    let r = &slot["rect"];
    let x = r[0].as_f64().unwrap() + r[2].as_f64().unwrap()/2.0;
    let y = r[1].as_f64().unwrap() + r[3].as_f64().unwrap()/2.0;
    run_script(&mut session, format!("mouse_move {x} {y}\nclick\n").as_bytes());
    let state = dump(&mut session);
    let status = state["ui"].as_array().unwrap().iter().find(|e| e["id"] == "StatusBar").unwrap();
    let rect = &status["rect"];
    assert!(rect[1].as_f64().unwrap() + rect[3].as_f64().unwrap() <= 1080.0, "{rect}");
    let _ = std::fs::remove_dir_all(dir);
}
