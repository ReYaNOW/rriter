use crate::headless::tests_support::{
    click_ui, dump, has_ui, install_fake_ty, scratch_dir, wait_until, workspace_with_explorer,
};
use crate::platform::{self, ToolPaths};
use crate::lsp::LspServerStatus;

struct ToolPathsReset(ToolPaths);

impl Drop for ToolPathsReset {
    fn drop(&mut self) {
        platform::configure_tool_paths(self.0.clone());
    }
}

fn ty_index(session: &crate::headless::HeadlessSession) -> usize {
    session
        .app
        .ide_panel
        .lsp_servers
        .iter()
        .position(|server| server.name == "ty")
        .unwrap_or_else(|| panic!("Ty server is listed in LSP panel"))
}

fn open_lsp_panel(session: &mut crate::headless::HeadlessSession) {
    click_ui(session, "SidebarSlot(LspServers)");
}

#[test]
fn headless_lsp_servers_panel_lists_configured_ty_and_starts_it() {
    let dir = scratch_dir("ui-lsp-available");
    let mut session = workspace_with_explorer(1920, 1080, 1.0, &dir);
    let reset = ToolPathsReset(session.app.tool_paths.clone());
    install_fake_ty(&mut session, &dir, "fake_lsp_server.py");
    open_lsp_panel(&mut session);

    let index = ty_index(&session);
    assert!(has_ui(&dump(&mut session), &format!("LspServerToggle({index})")));
    assert_eq!(session.app.ide_panel.lsp_servers[index].status, LspServerStatus::Disabled);
    click_ui(&mut session, &format!("LspServerToggle({index})"));
    wait_until(&mut session, 5000, "fake Ty server to start", |session| {
        session.app.ide_panel.lsp_servers.get(index).is_some_and(|server| {
            server.status == LspServerStatus::Running
        })
    });
    assert!(has_ui(&dump(&mut session), &format!("LspServerStop({index})")));
    click_ui(&mut session, &format!("LspServerStop({index})"));
    wait_until(&mut session, 3000, "fake Ty server cleanup", |session| {
        session.app.ide_panel.lsp_servers.get(index).is_some_and(|server| {
            server.status == LspServerStatus::Disabled
        })
    });

    let _ = std::fs::remove_dir_all(&dir);
    drop(reset);
}

#[test]
fn headless_lsp_servers_panel_stop_returns_ty_to_disabled() {
    let dir = scratch_dir("ui-lsp-stop");
    let mut session = workspace_with_explorer(1920, 1080, 1.0, &dir);
    let reset = ToolPathsReset(session.app.tool_paths.clone());
    install_fake_ty(&mut session, &dir, "fake_lsp_server.py");
    open_lsp_panel(&mut session);

    let index = ty_index(&session);
    assert!(has_ui(&dump(&mut session), &format!("LspServerToggle({index})")));
    click_ui(&mut session, &format!("LspServerToggle({index})"));
    wait_until(&mut session, 5000, "fake Ty server to start before stopping", |session| {
        session.app.ide_panel.lsp_servers.get(index).is_some_and(|server| {
            server.status == LspServerStatus::Running
        })
    });
    click_ui(&mut session, &format!("LspServerStop({index})"));
    wait_until(&mut session, 3000, "fake Ty server to stop", |session| {
        session.app.ide_panel.lsp_servers.get(index).is_some_and(|server| {
            server.status == LspServerStatus::Disabled
        })
    });

    let _ = std::fs::remove_dir_all(&dir);
    drop(reset);
}

#[test]
fn headless_lsp_servers_panel_reports_fake_ty_crash_without_restart_loop() {
    let dir = scratch_dir("ui-lsp-crash");
    let mut session = workspace_with_explorer(1920, 1080, 1.0, &dir);
    let reset = ToolPathsReset(session.app.tool_paths.clone());
    let executable = install_fake_ty(&mut session, &dir, "fake_lsp_server_crash.py");
    open_lsp_panel(&mut session);

    let index = ty_index(&session);
    assert!(has_ui(&dump(&mut session), &format!("LspServerToggle({index})")));
    click_ui(&mut session, &format!("LspServerToggle({index})"));
    wait_until(&mut session, 12_000, "fake Ty crash retries to finish", |session| {
        session.app.ide_panel.lsp_servers.get(index).is_some_and(|server| {
            server.status == LspServerStatus::Disabled
                && server.logs.iter().any(|entry| entry.text.contains("disabled after 4"))
        })
    });
    let starts_path = executable.with_extension("starts");
    let starts = std::fs::read_to_string(starts_path)
        .expect("fake server startup count")
        .parse::<u8>()
        .expect("numeric fake server startup count");
    assert_eq!(starts, 4, "crash retries must stop at the configured attempt limit");

    let _ = std::fs::remove_dir_all(&dir);
    drop(reset);
}
