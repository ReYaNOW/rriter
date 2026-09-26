use crate::headless::tests_support::{
    assert_rect_inside_window, assert_ui_y_integral, click_ui, dump, has_ui, ok_json, run_script,
    sample_file, scratch_dir, session_for_test,
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

const LAYOUT_HITBOX_SIZE_SCALE_MATRIX: [(u32, u32, f32); 4] = [
    (2560, 1440, 4.0 / 3.0),
    (1280, 1440, 4.0 / 3.0),
    (1280, 720, 4.0 / 3.0),
    (640, 480, 1.5),
];

const LAYOUT_PANEL_CASES: [(&str, &str); 8] = [
    ("Explorer", "explorer"),
    ("Search", "search"),
    ("Git", "git"),
    ("ApiClient", "api"),
    ("Database", "database"),
    ("LspServers", "lsp"),
    ("Problems", "problems"),
    ("Terminal", "terminal"),
];

fn layout_hitbox_session(w: u32, h: u32, scale: f32) -> crate::headless::HeadlessSession {
    let mut session = session_for_test(w, h);
    let lines = run_script(&mut session, format!("scale {scale}\nsettle 800\n").as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{w}x{h}@{scale} scale setup: {lines:?}");
    session
}

fn layout_hitbox_workspace_session(
    w: u32,
    h: u32,
    scale: f32,
    dir: &std::path::Path,
) -> crate::headless::HeadlessSession {
    let mut session = layout_hitbox_session(w, h, scale);
    let lines = run_script(
        &mut session,
        format!("workspace {}\nsettle 800\n", dir.display()).as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{w}x{h}@{scale} workspace setup: {lines:?}");
    session
}

fn layout_hitbox_fixture(name: &str) -> std::path::PathBuf {
    let dir = scratch_dir(name);
    let _ = sample_file(&dir);
    let folder = dir.join("folder");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("child.txt"), "child\n").unwrap();
    dir
}

fn assert_layout_hitboxes(
    state: &serde_json::Value,
    w: u32,
    h: u32,
    scale: f32,
    state_name: &str,
) {
    let label = format!("{w}x{h}@{scale} {state_name}");
    let ui = state["ui"].as_array().unwrap_or_else(|| panic!("{label}: UI dump missing"));
    assert!(!ui.is_empty(), "{label}: no registered UiId hitboxes");
    for element in ui {
        let id = element["id"].as_str().unwrap_or("<missing>");
        let rect = element["rect"].as_array().unwrap_or_else(|| panic!("{label} UiId={id}: rect missing"));
        assert_eq!(rect.len(), 4, "{label} UiId={id}: rect={rect:?}");
        let x = rect[0].as_f64().unwrap_or_else(|| panic!("{label} UiId={id}: rect={rect:?}"));
        let y = rect[1].as_f64().unwrap_or_else(|| panic!("{label} UiId={id}: rect={rect:?}"));
        let width = rect[2].as_f64().unwrap_or_else(|| panic!("{label} UiId={id}: rect={rect:?}"));
        let height = rect[3].as_f64().unwrap_or_else(|| panic!("{label} UiId={id}: rect={rect:?}"));
        assert_rect_inside_window(
            [x, y, width, height],
            w as f64,
            h as f64,
            0.01,
            0.01,
            &format!("{label} UiId={id}"),
        );
        assert_ui_y_integral(y, 0.01, &format!("{label} UiId={id}"));
    }
}

fn close_layout_panels(session: &mut crate::headless::HeadlessSession) {
    for (slot, panel) in LAYOUT_PANEL_CASES {
        let state = dump(session);
        let is_open = state["ide_panel"]["open"]
            .as_array()
            .is_some_and(|panels| panels.iter().any(|value| value == panel));
        let id = format!("SidebarSlot({slot})");
        if is_open && has_ui(&state, &id) {
            click_ui(session, &id);
        }
    }
}

fn open_layout_panel(
    session: &mut crate::headless::HeadlessSession,
    w: u32,
    h: u32,
    scale: f32,
    slot: &str,
    panel: &str,
) -> Option<serde_json::Value> {
    close_layout_panels(session);
    let id = format!("SidebarSlot({slot})");
    let before = dump(session);
    if !has_ui(&before, &id) {
        assert_eq!((w, h), (640, 480), "{w}x{h}@{scale} panel {slot} missing UiId={id}");
        return None;
    }
    click_ui(session, &id);
    run_script(session, b"settle 100\n");
    let state = dump(session);
    assert!(
        state["ide_panel"]["open"]
            .as_array()
            .is_some_and(|panels| panels.iter().any(|value| value == panel)),
        "{w}x{h}@{scale} panel {slot} did not open UiId={id}: {state}"
    );
    Some(state)
}

#[test]
fn headless_layout_registered_hitboxes_welcome_size_matrix() {
    for (w, h, scale) in LAYOUT_HITBOX_SIZE_SCALE_MATRIX {
        let mut session = layout_hitbox_session(w, h, scale);
        let state = dump(&mut session);
        assert_eq!(state["mode"], "welcome", "{w}x{h}@{scale} welcome mode");
        assert_layout_hitboxes(&state, w, h, scale, "welcome");
    }
}

#[test]
fn headless_layout_registered_hitboxes_workspace_and_api_matrix() {
    let dir = layout_hitbox_fixture("ui-layout-hitbox-api-matrix");
    for (w, h, scale) in LAYOUT_HITBOX_SIZE_SCALE_MATRIX {
        let mut session = layout_hitbox_workspace_session(w, h, scale, &dir);
        let workspace = dump(&mut session);
        assert_eq!(workspace["mode"], "ide", "{w}x{h}@{scale} workspace mode");
        assert_layout_hitboxes(&workspace, w, h, scale, "workspace");
        let api = open_layout_panel(&mut session, w, h, scale, "ApiClient", "api")
            .unwrap_or_else(|| panic!("{w}x{h}@{scale} API client missing UiId=SidebarSlot(ApiClient)"));
        assert_layout_hitboxes(&api, w, h, scale, "panel ApiClient");
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_layout_registered_hitboxes_smoke_file_explorer_and_lsp() {
    let dir = layout_hitbox_fixture("ui-layout-hitbox-smoke");
    let file = dir.join("sample.txt");
    let (w, h, scale) = (640, 480, 1.5);

    let mut file_session = layout_hitbox_session(w, h, scale);
    open_file(&mut file_session, &file);
    let file_state = dump(&mut file_session);
    assert_eq!(file_state["mode"], "editor", "{w}x{h}@{scale} file editor mode");
    assert_layout_hitboxes(&file_state, w, h, scale, "file open");

    let mut workspace = layout_hitbox_workspace_session(w, h, scale, &dir);
    for (slot, panel) in [("Explorer", "explorer"), ("LspServers", "lsp")] {
        let state = open_layout_panel(&mut workspace, w, h, scale, slot, panel)
            .unwrap_or_else(|| panic!("{w}x{h}@{scale} panel {slot} missing UiId=SidebarSlot({slot})"));
        assert_layout_hitboxes(&state, w, h, scale, &format!("panel {slot}"));
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_layout_registered_hitboxes_settings_tabs_that_fit_matrix() {
    for (w, h, last_tab) in [(2560, 1440, 5), (1280, 1440, 5), (1280, 720, 3)] {
        let scale = 4.0 / 3.0;
        let mut session = layout_hitbox_session(w, h, scale);
        let lines = run_script(&mut session, b"key f1\nsettle 100\n");
        assert!(lines.iter().all(|line| line.starts_with("ok")), "{w}x{h}@{scale} settings open: {lines:?}");
        for tab in 2..=last_tab {
            let before = dump(&mut session);
            let id = format!("SettingsTab({tab})");
            assert!(has_ui(&before, &id), "{w}x{h}@{scale} settings tab {tab} missing UiId={id}");
            click_ui(&mut session, &id);
            run_script(&mut session, b"settle 100\n");
            let state = dump(&mut session);
            assert!(state["overlays"]["settings"].as_bool().unwrap_or(false), "{w}x{h}@{scale} settings tab {tab} overlay");
            assert_layout_hitboxes(&state, w, h, scale, &format!("settings tab {tab}"));
        }
    }
}

#[test]
#[ignore = "kanri jbn6bb6q5whxo5ro4h9k5k46: editor scrollbar hitbox y is fractional"]
fn headless_bug_editor_hitbox_y_integral_size_matrix() {
    let dir = layout_hitbox_fixture("ui-layout-bug-editor-hitbox-y");
    let file = dir.join("wide.txt");
    std::fs::write(&file, format!("{}\n", "x".repeat(600))).unwrap();
    for (w, h, scale) in LAYOUT_HITBOX_SIZE_SCALE_MATRIX {
        let mut session = layout_hitbox_session(w, h, scale);
        open_file(&mut session, &file);
        let state = dump(&mut session);
        assert_layout_hitboxes(&state, w, h, scale, "file open");
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
#[ignore = "kanri h158154p1igj3c7ovzi56i2j: IDE sidebar hitbox y is fractional at scale 4/3"]
fn headless_bug_sidebar_panel_hitbox_y_integral_size_matrix() {
    let dir = layout_hitbox_fixture("ui-layout-bug-sidebar-hitbox-y");
    for (w, h, scale) in LAYOUT_HITBOX_SIZE_SCALE_MATRIX {
        let mut session = layout_hitbox_workspace_session(w, h, scale, &dir);
        for (slot, panel) in LAYOUT_PANEL_CASES {
            if let Some(state) = open_layout_panel(&mut session, w, h, scale, slot, panel) {
                assert_layout_hitboxes(&state, w, h, scale, &format!("panel {slot}"));
            }
        }
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
#[ignore = "kanri tdxq0x7q0f0glr5iyl54nr6n: API Client spec row hitbox y is fractional"]
fn headless_bug_api_client_spec_refresh_hitbox_y_integral_size_matrix() {
    let dir = layout_hitbox_fixture("ui-layout-bug-api-spec-hitbox-y");
    for (w, h, scale) in LAYOUT_HITBOX_SIZE_SCALE_MATRIX {
        let mut session = layout_hitbox_workspace_session(w, h, scale, &dir);
        // A URL spec card registers `ApiSpecRefresh(0)`; the default state has no spec.
        session.app.ide_panel.api.specs.push(crate::app::api_client::ApiSpecEntry {
            id: crate::app::api_client::ApiSpecId(1),
            title: "Spec API".to_string(),
            version: "1".to_string(),
            openapi_version: "3.1.0".to_string(),
            source: crate::app::api_client::ApiSpecSource::Url(
                "https://example.test/openapi.json".to_string(),
            ),
            last_loaded: None,
            last_fetch_secs: None,
            last_parse_secs: None,
            last_url_status: None,
            selected: true,
            error: None,
        });
        if let Some(state) = open_layout_panel(&mut session, w, h, scale, "ApiClient", "api") {
            assert!(has_ui(&state, "ApiSpecRefresh(0)"), "{w}x{h}@{scale} spec row missing");
            assert_layout_hitboxes(&state, w, h, scale, "panel ApiClient with spec");
        }
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
#[ignore = "kanri pgwvbzz1pvi8fjcccipu0ar2: settings tab hitbox leaves the window"]
fn headless_bug_settings_hitboxes_inside_window_size_matrix() {
    for (w, h, scale) in LAYOUT_HITBOX_SIZE_SCALE_MATRIX {
        let mut session = layout_hitbox_session(w, h, scale);
        let lines = run_script(&mut session, b"key f1\nsettle 100\n");
        assert!(lines.iter().all(|line| line.starts_with("ok")), "{w}x{h}@{scale} settings open: {lines:?}");
        for tab in 0..6 {
            if tab > 0 {
                let before = dump(&mut session);
                let id = format!("SettingsTab({tab})");
                assert!(has_ui(&before, &id), "{w}x{h}@{scale} settings tab {tab} missing UiId={id}");
                click_ui(&mut session, &id);
                run_script(&mut session, b"settle 100\n");
            }
            let state = dump(&mut session);
            assert!(state["overlays"]["settings"].as_bool().unwrap_or(false), "{w}x{h}@{scale} settings tab {tab} overlay");
            assert_layout_hitboxes(&state, w, h, scale, &format!("settings tab {tab}"));
        }
    }
}
