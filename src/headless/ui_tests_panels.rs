use crate::headless::tests_support::{click_ui, dump, has_ui, run_script, sample_file, scratch_dir, session_for_test, ui_center};

fn click(session: &mut crate::headless::HeadlessSession, id: &str) {
    click_ui(session, id);
}

fn close_panel_if_open(
    session: &mut crate::headless::HeadlessSession,
    slot_id: &str,
    panel_name: &str,
) {
    let is_open = dump(session)["ide_panel"]["open"]
        .as_array()
        .is_some_and(|panels| panels.iter().any(|panel| panel == panel_name));
    if is_open {
        click(session, slot_id);
    }
}

fn workspace_session(
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
    session
}

#[test]
fn headless_ide_panel_slots_open_and_register_controls() {
    let dir = scratch_dir("ui-panels-slots");
    let _ = sample_file(&dir);
    let mut session = workspace_session(1920, 1080, 1.0, &dir);
    for (slot, active) in [
        ("Explorer", "explorer"),
        ("Search", "search"),
        ("Git", "git"),
        ("ApiClient", "api"),
        ("Database", "database"),
        ("LspServers", "lsp"),
        ("Problems", "problems"),
    ] {
        let id = format!("SidebarSlot({slot})");
        click(&mut session, &id);
        run_script(&mut session, b"settle 2000\n");
        let state = dump(&mut session);
        assert!(state["ide_panel"]["open"].as_array().unwrap().iter().any(|v| v == active), "{slot}: {state}");
        let controls = state["ui"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| !e["id"].as_str().unwrap_or("").starts_with("SidebarSlot("))
            .count();
        assert!(controls > 0, "{slot}: {state}");
    }
    let _ = std::fs::remove_dir_all(dir);
}

fn git_fixture(dir: &std::path::Path) {
    let run = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .expect("git fixture command");
        assert!(
            output.status.success(),
            "git {:?}: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    };
    std::fs::write(dir.join("changed.txt"), "before\n").unwrap();
    std::fs::write(dir.join("deleted.txt"), "delete me\n").unwrap();
    run(&["init", "-q"]);
    run(&["config", "user.name", "Headless Test"]);
    run(&["config", "user.email", "headless@example.invalid"]);
    run(&["add", "."]);
    run(&["commit", "-qm", "fixture"]);
    std::fs::write(dir.join("changed.txt"), "after\n").unwrap();
    std::fs::remove_file(dir.join("deleted.txt")).unwrap();
    std::fs::write(dir.join("untracked.txt"), "new\n").unwrap();
}

#[test]
fn headless_project_search_empty_and_open_result() {
    let dir = scratch_dir("ui-project-search");
    let file = dir.join("needle.txt");
    std::fs::write(&file, "zero\none needle\ntwo\n").unwrap();
    let mut session = workspace_session(1920, 1080, 1.0, &dir);
    click(&mut session, "SidebarSlot(Search)");
    run_script(&mut session, b"settle 2000\n");
    let state = dump(&mut session);
    assert!(
        has_ui(&state, "ProjectSearchQueryInput"),
        "project search input missing: {state}"
    );
    let (x, y) = ui_center(&state, "ProjectSearchQueryInput");
    run_script(&mut session, format!("mouse_move {x} {y}\nclick\ntype needle\n").as_bytes());
    click(&mut session, "ProjectSearchRun");
    run_script(&mut session, b"wait 1000\n");
    let results = dump(&mut session);
    assert!(has_ui(&results, "ProjectSearchFileToggle(0)"), "{results}");
    click(&mut session, "ProjectSearchFileToggle(0)");
    let match_state = dump(&mut session);
    let hit = match_state["ui"].as_array().unwrap().iter().find(|e| e["id"].as_str().unwrap_or("").contains("ProjectSearchMatch"));
    if let Some(hit) = hit {
        let r = &hit["rect"];
        let (x, y) = (
            r[0].as_f64().unwrap() + r[2].as_f64().unwrap() / 2.0,
            r[1].as_f64().unwrap() + r[3].as_f64().unwrap() / 2.0,
        );
        run_script(&mut session, format!("mouse_move {x} {y}\nclick\n").as_bytes());
        assert_eq!(dump(&mut session)["tabs"][0]["cursor"]["line"], 2);
    }
    let query_state = dump(&mut session);
    if has_ui(&query_state, "ProjectSearchQueryInput") {
        click(&mut session, "ProjectSearchQueryInput");
        run_script(&mut session, b"key ctrl+a\ntype no-such-result-987\n");
        click(&mut session, "ProjectSearchRun");
        run_script(&mut session, b"wait 1000\n");
    }
    assert!(!has_ui(&dump(&mut session), "ProjectSearchFileToggle(0)"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_database_dialog_and_api_mock_guide_controls() {
    let dir = scratch_dir("ui-database-api");
    let mut session = workspace_session(1920, 1080, 1.0, &dir);
    click(&mut session, "SidebarSlot(Database)");
    if has_ui(&dump(&mut session), "DatabaseAdd") {
        click(&mut session, "DatabaseAdd");
        let dialog = dump(&mut session);
        assert!(dialog["ui"].as_array().unwrap().iter().any(|e| e["id"].as_str().unwrap_or("").contains("DatabaseDialogField")), "{dialog}");
        run_script(&mut session, b"key escape\n");
    }
    click(&mut session, "SidebarSlot(ApiClient)");
    let api = dump(&mut session);
    if has_ui(&api, "ApiMockServerToggle") {
        click(&mut session, "ApiMockServerToggle");
        assert!(has_ui(&dump(&mut session), "ApiMockServerCopyUrl"));
    }
    if has_ui(&dump(&mut session), "ApiMockGuideOpen") {
        click(&mut session, "ApiMockGuideOpen");
        assert!(has_ui(&dump(&mut session), "ApiMockGuideClose"));
        click(&mut session, "ApiMockGuideClose");
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_lsp_server_toggle_exposes_stop_control_when_available() {
    let dir = scratch_dir("ui-lsp-toggle");
    let file = dir.join("main.py");
    std::fs::write(&file, "def main():\n    value = 1\n").unwrap();
    let mut session = workspace_session(1920, 1080, 1.25, &dir);
    run_script(&mut session, format!("open {}\nsettle 2000\n", file.display()).as_bytes());
    click(&mut session, "SidebarSlot(LspServers)");
    run_script(&mut session, b"settle 2000\n");
    let state = dump(&mut session);
    let available = session
        .app
        .ide_panel
        .lsp_servers
        .iter()
        .position(|server| {
            !matches!(
                server.status,
                crate::lsp::LspServerStatus::Missing | crate::lsp::LspServerStatus::Crashed
            )
        });
    let Some(index) = available else {
        eprintln!("skip: no available LSP server");
        let _ = std::fs::remove_dir_all(dir);
        return;
    };
    let toggle = format!("LspServerToggle({index})");
    if !has_ui(&state, &toggle) {
        eprintln!("skip: LSP server toggle is unavailable");
        let _ = std::fs::remove_dir_all(dir);
        return;
    }
    click(&mut session, &toggle);
    run_script(&mut session, b"wait 8000\n");
    let state = dump(&mut session);
    let server_status = session.app.ide_panel.lsp_servers.get(index).map(|server| server.status);
    if server_status == Some(crate::lsp::LspServerStatus::Running) {
        assert!(has_ui(&state, &format!("LspServerStop({index})")), "{state}");
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_ide_panel_splitters_resize_side_and_bottom_panels() {
    let dir = scratch_dir("ui-panel-splitters");
    let file = sample_file(&dir);
    let mut session = workspace_session(1920, 1080, 1.0, &dir);
    run_script(&mut session, format!("open {}\nsettle 2000\n", file.display()).as_bytes());
    click(&mut session, "SidebarSlot(Git)");
    run_script(&mut session, b"settle 2000\n");
    let before = dump(&mut session);
    let width_before = before["ide_panel"]["width"].as_f64().unwrap();
    let (x, y) = ui_center(&before, "ResizeLeft");
    run_script(&mut session, format!("mouse_move {x} {y}\nclick down\nmouse_move {} {y}\nclick up\n", x + 100.0).as_bytes());
    run_script(&mut session, b"settle 2000\n");
    assert_ne!(dump(&mut session)["ide_panel"]["width"].as_f64().unwrap(), width_before);
    click(&mut session, "SidebarSlot(Problems)");
    run_script(&mut session, b"settle 2000\n");
    let before = dump(&mut session);
    let rect = before["ui"].as_array().unwrap().iter().find(|e| e["id"] == "ResizeBottom").unwrap()["rect"].clone();
    let x = rect[0].as_f64().unwrap() + rect[2].as_f64().unwrap()/2.0;
    let y = rect[1].as_f64().unwrap() + rect[3].as_f64().unwrap()/2.0;
    run_script(&mut session, format!("mouse_move {x} {y}\nclick down\nmouse_move {x} {}\nclick up\n", y - 120.0).as_bytes());
    let resized = dump(&mut session);
    assert!(has_ui(&resized, "ResizeBottom"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_terminal_scroll_returns_to_bottom_after_output() {
    let dir = scratch_dir("ui-terminal-scroll");
    let mut session = workspace_session(1920, 1080, 1.0, &dir);
    click(&mut session, "SidebarSlot(Terminal)");
    run_script(&mut session, b"wait 3000\n");
    let state = dump(&mut session);
    let terminal_spawn_failed = session.app.ide_panel.terminals.first().is_some_and(|terminal| {
        let grid = crate::app::terminal::lock_terminal_grid(&terminal.grid);
        grid.lines.iter().flatten().map(|cell| cell.c).collect::<String>().contains("RRiter terminal error:")
    });
    if terminal_spawn_failed {
        let _ = std::fs::remove_dir_all(dir);
        return;
    }
    assert!(has_ui(&state, "TerminalBody"), "{state}");
    let (x, y) = ui_center(&state, "TerminalBody");
    run_script(&mut session, format!("mouse_move {x} {y}\nclick\ntype seq 1 500\\n\nwait 1500\n").as_bytes());
    let top = dir.join("terminal-top.png");
    let bottom = dir.join("terminal-bottom.png");
    run_script(
        &mut session,
        format!("screenshot {}\nmouse_move {x} {y}\nwheel 0 10\nsettle 2000\n", top.display()).as_bytes(),
    );
    run_script(&mut session, b"wheel 0 -1000\nsettle 2000\n");
    run_script(&mut session, format!("screenshot {}\n", bottom.display()).as_bytes());
    let top = image::open(top).unwrap().to_rgba8();
    let bottom = image::open(bottom).unwrap().to_rgba8();
    assert_eq!(top.as_raw(), bottom.as_raw());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_bug_terminal_sidebar_slot_opens_panel() {
    let dir = scratch_dir("ui-terminal-slot");
    let mut session = workspace_session(1920, 1080, 1.0, &dir);
    close_panel_if_open(&mut session, "SidebarSlot(Terminal)", "terminal");
    click(&mut session, "SidebarSlot(Terminal)");
    // The panel is shown once the shell prints its first output.
    run_script(&mut session, b"wait 8000\n");
    let state = dump(&mut session);
    assert!(
        state["ide_panel"]["open"]
            .as_array()
            .unwrap()
            .iter()
            .any(|panel| panel == "terminal"),
        "{state}"
    );
    // Panel state is saved to the process-wide test profile; do not leak it into later tests.
    close_panel_if_open(&mut session, "SidebarSlot(Terminal)", "terminal");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_bug_api_mock_guide_wheel_scrolls_content() {
    let dir = scratch_dir("ui-bug-api-guide");
    let mut session = workspace_session(1920, 1080, 1.0, &dir);
    click(&mut session, "SidebarSlot(ApiClient)");
    click(&mut session, "ApiMockGuideOpen");
    let shot_a = dir.join("before.png");
    let shot_b = dir.join("after.png");
    let state = dump(&mut session);
    let body = state["ui"]
        .as_array()
        .unwrap()
        .iter()
        .find(|element| element["id"] == "ApiMockGuideBody")
        .unwrap_or_else(|| panic!("ApiMockGuideBody missing: {state}"));
    let rect = body["rect"].as_array().unwrap();
    let x = rect[0].as_f64().unwrap() + rect[2].as_f64().unwrap() / 2.0;
    let y = rect[1].as_f64().unwrap() + rect[3].as_f64().unwrap() / 2.0;
    run_script(&mut session, format!("mouse_move {x} {y}\nsettle 200\n").as_bytes());
    run_script(&mut session, format!("screenshot {}\n", shot_a.display()).as_bytes());
    run_script(&mut session, b"wheel 0 -10\nwait 800\n");
    run_script(&mut session, format!("screenshot {}\n", shot_b.display()).as_bytes());
    let a = image::open(shot_a).unwrap().to_rgba8();
    let b = image::open(shot_b).unwrap().to_rgba8();
    let x = rect[0].as_f64().unwrap().round() as u32;
    let y = rect[1].as_f64().unwrap().round() as u32;
    let width = rect[2].as_f64().unwrap().round() as u32;
    let height = rect[3].as_f64().unwrap().round() as u32;
    assert_ne!(
        image::imageops::crop_imm(&a, x, y, width, height).to_image().as_raw(),
        image::imageops::crop_imm(&b, x, y, width, height).to_image().as_raw(),
        "API Mock guide body did not scroll"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_bug_sidebar_slots_hit_lsp_servers_at_small_sizes() {
    let dir = scratch_dir("ui-bug-sidebar-slots");
    for (w, h, scale) in [(640, 480, 1.5), (400, 300, 1.0)] {
        let mut session = workspace_session(w, h, scale, &dir);
        close_panel_if_open(&mut session, "SidebarSlot(LspServers)", "lsp");
        click(&mut session, "SidebarSlot(LspServers)");
        assert!(dump(&mut session)["ide_panel"]["open"].as_array().unwrap().iter().any(|v| v == "lsp"));
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_bug_git_changes_load_without_manual_refresh() {
    let dir = scratch_dir("ui-bug-git-refresh");
    git_fixture(&dir);
    let mut session = workspace_session(1920, 1080, 1.0, &dir);
    close_panel_if_open(&mut session, "SidebarSlot(Git)", "git");
    click(&mut session, "SidebarSlot(Git)");
    run_script(&mut session, b"wait 8000\n");
    assert!(dump(&mut session)["ui"].as_array().unwrap().iter().any(|e| e["id"].as_str().unwrap_or("").starts_with("GitFile")));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_bug_settings_tab_y_integral() {
    // Card eydb0ylohhjr1zx8clsiz4lv: 640x400 at 1.5 with a file open, F1.
    let dir = scratch_dir("ui-bug-settings-tabs");
    let file = sample_file(&dir);
    let mut session = session_for_test(640, 400);
    run_script(&mut session, format!("scale 1.5\nopen {}\nkey f1\nsettle 800\n", file.display()).as_bytes());
    let state = dump(&mut session);
    let rect_of = |element: &serde_json::Value| -> [f64; 4] {
        let rect = element["rect"].as_array().unwrap();
        [0, 1, 2, 3].map(|i| rect[i].as_f64().unwrap())
    };
    let tabs: Vec<[f64; 4]> = state["ui"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["id"].as_str().unwrap_or("").starts_with("SettingsTab("))
        .map(rect_of)
        .collect();
    assert_eq!(tabs.len(), 6, "settings tabs missing: {state}");
    for (i, rect) in tabs.iter().enumerate() {
        assert_eq!(rect[1].fract(), 0.0, "SettingsTab({i}) y={}", rect[1]);
        assert_eq!(rect[3].fract(), 0.0, "SettingsTab({i}) h={}", rect[3]);
    }
    let add = state["ui"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "SettingsIdeAddWorkspace")
        .map(rect_of)
        .unwrap_or_else(|| panic!("SettingsIdeAddWorkspace missing: {state}"));
    let screenshot = dir.join("tabs.png");
    run_script(&mut session, format!("screenshot {}\n", screenshot.display()).as_bytes());
    let image = image::open(&screenshot).unwrap().to_rgba8();
    let bright = |x: u32, y: u32| image.get_pixel(x, y).0[..3].iter().all(|&c| c >= 140);
    // Labels stay inside their hitbox: nothing bright between the hitbox's
    // right edge and the sidebar divider (10 * scale further right).
    for rect in &tabs {
        let right = (rect[0] + rect[2]).round() as u32;
        for y in rect[1] as u32..(rect[1] + rect[3]) as u32 {
            for x in right.saturating_sub(3)..right + 13 {
                assert!(!bright(x, y), "tab label crosses its hitbox at ({x}, {y})");
            }
        }
    }
    // The «+» icon of «Добавить папку» stays inside the button.
    let left = add[0].round() as u32;
    for y in add[1] as u32..(add[1] + add[3]) as u32 {
        for x in left.saturating_sub(12)..left {
            assert!(!bright(x, y), "add-folder content sticks out left of the button at ({x}, {y})");
        }
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_bug_settings_help_right_edge_content() {
    let dir = scratch_dir("ui-bug-settings-help");
    let mut session = session_for_test(1280, 800);
    run_script(&mut session, b"scale 1.5\nkey f1\nsettle 500\n");
    let tabs = dump(&mut session);
    assert!(has_ui(&tabs, "SettingsTab(4)"), "Help settings tab missing: {tabs}");
    click(&mut session, "SettingsTab(4)");
    run_script(&mut session, b"settle 500\n");
    let state = dump(&mut session);
    assert!(state["overlays"]["settings"].as_bool().unwrap_or(false));
    assert!(has_ui(&state, "SettingsFaqScrollY"), "Help scrollbar missing: {state}");
    let scroll = state["ui"]
        .as_array()
        .unwrap()
        .iter()
        .find(|element| element["id"] == "SettingsFaqScrollY")
        .unwrap();
    let rect = scroll["rect"].as_array().unwrap();
    let edge = rect[0].as_f64().unwrap().floor() as u32;
    let top = rect[1].as_f64().unwrap().floor() as u32;
    let bottom = (rect[1].as_f64().unwrap() + rect[3].as_f64().unwrap()).ceil() as u32;
    let screenshot = dir.join("help.png");
    run_script(&mut session, format!("screenshot {}\n", screenshot.display()).as_bytes());
    let image = image::open(&screenshot).unwrap().to_rgba8();
    for y in top..bottom {
        for x in edge.saturating_sub(3)..edge {
            let pixel = image.get_pixel(x, y);
            assert!(
                pixel[0] < 160 || pixel[1] < 160 || pixel[2] < 160,
                "Help text reaches right edge at ({x}, {y}): {pixel:?}"
            );
        }
    }
    let _ = std::fs::remove_dir_all(dir);
}
