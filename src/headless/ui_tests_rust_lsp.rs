//! Headless coverage for Rust analyzer lifecycle and diagnostics.

use crate::headless::HeadlessSession;
use crate::headless::tests_support::{
    click_ui, dump, has_ui, install_fake_lsp, run_script, scratch_dir, wait_until,
    workspace_with_explorer,
};
use crate::lsp::LspServerStatus;
use crate::platform::ToolKind;
use std::path::{Path, PathBuf};

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;

/// Creates a Cargo crate and a headless session rooted at its directory.
pub(super) fn rust_crate_session(name: &str) -> (PathBuf, PathBuf, HeadlessSession) {
    let dir = scratch_dir(name);
    let file = rust_crate(&dir);
    let session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    (dir, file, session)
}

fn rust_crate(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir.join("src")).expect("create Rust fixture source directory");
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("write Rust fixture Cargo.toml");
    let file = dir.join("src/main.rs");
    std::fs::write(&file, "fn main() {\n    missing_name();\n}\n")
        .expect("write Rust fixture main.rs");
    file
}

fn install_rust_fake(session: &mut HeadlessSession, name: &str, mode: &str) -> PathBuf {
    let tools = scratch_dir(&format!("{name}-tools"));
    install_fake_lsp(session, ToolKind::RustAnalyzer, &tools, mode)
}

fn open_file(session: &mut HeadlessSession, file: &Path) {
    let lines = run_script(session, format!("open {}\n", file.display()).as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(session, 5000, "Rust fixture tab", |session| {
        session.app.file_path.as_deref() == Some(file)
    });
}

fn close_tab(session: &mut HeadlessSession, index: usize) {
    click_ui(session, &format!("EditorTab({index})"));
    let state = dump(session);
    let element = format!("EditorTabClose({index})");
    let ui = state["ui"].as_array().expect("UI array");
    let rect = ui
        .iter()
        .find(|item| item["id"] == element)
        .unwrap_or_else(|| panic!("missing {element}: {state}"));
    let x = rect["rect"][0].as_f64().expect("tab close x")
        + rect["rect"][2].as_f64().expect("tab close width") / 2.0;
    let y = rect["rect"][1].as_f64().expect("tab close y")
        + rect["rect"][3].as_f64().expect("tab close height") / 2.0;
    run_script(session, format!("mouse_move {x} {y}\nwait 50\n").as_bytes());
    click_ui(session, &element);
}

fn rust_status(session: &HeadlessSession) -> Option<LspServerStatus> {
    session.app.lsp.as_ref().map(|lsp| lsp.rust_row_info().status)
}

fn fake_starts(executable: &Path) -> usize {
    std::fs::read_to_string(executable.with_extension("starts"))
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

fn problem_paths(session: &HeadlessSession) -> Vec<PathBuf> {
    session.app.ide_panel.flat_diags.iter().map(|row| row.path.to_path_buf()).collect()
}

fn open_all_problems(session: &mut HeadlessSession) {
    click_ui(session, "SidebarSlot(Problems)");
    wait_until(session, 5000, "Problems tabs", |session| {
        crate::headless::tests_support::has_ui(&dump(session), "ProblemsTab(1)")
    });
    click_ui(session, "ProblemsTab(1)");
}

#[test]
fn headless_rust_starts_on_open_and_stops_on_last_close() {
    let (dir, file, mut session) = rust_crate_session("rust-lifecycle");
    install_rust_fake(&mut session, "rust-lifecycle", "rust_analyzer");

    open_file(&mut session, &file);
    wait_until(&mut session, 8000, "Rust server Running", |session| {
        rust_status(session) == Some(LspServerStatus::Running)
    });
    close_tab(&mut session, 0);
    wait_until(&mut session, 5000, "Rust server Disabled after last close", |session| {
        rust_status(session) == Some(LspServerStatus::Disabled)
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_rust_diagnostics_reach_problems_including_unopened_file() {
    let (dir, file, mut session) = rust_crate_session("rust-diagnostics");
    let sibling = file.with_file_name("sibling.rs");
    std::fs::write(&sibling, "pub fn sibling() {}\n").expect("write sibling Rust file");
    install_rust_fake(
        &mut session,
        "rust-diagnostics",
        "rust_analyzer_diagnostics_unopened",
    );

    open_file(&mut session, &file);
    open_all_problems(&mut session);
    wait_until(&mut session, 8000, "diagnostics for open and unopened Rust files", |session| {
        let paths = problem_paths(session);
        paths.contains(&file) && paths.contains(&sibling)
    });
    close_tab(&mut session, 0);
    wait_until(&mut session, 5000, "Rust diagnostics cleared on close", |session| {
        problem_paths(session).is_empty()
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_rust_save_notifies_server_and_refreshes_diagnostics() {
    let (dir, file, mut session) = rust_crate_session("rust-did-save");
    install_rust_fake(&mut session, "rust-did-save", "rust_analyzer_did_save");

    open_file(&mut session, &file);
    open_all_problems(&mut session);
    run_script(&mut session, b"key ctrl+s\n");
    wait_until(&mut session, 8000, "Rust didSave diagnostics", |session| {
        session.app.lsp.as_ref().is_some_and(|lsp| {
            lsp.rust.live_diagnostics().values().any(|set| {
                set.items.iter().any(|diagnostic| diagnostic.message.as_ref() == "fake: saved")
            })
        })
    });
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_rust_outside_cargo_root_has_no_lsp() {
    let dir = scratch_dir("rust-no-cargo");
    let file = dir.join("main.rs");
    std::fs::write(&file, "fn main() {}\n").expect("write standalone Rust file");
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    let executable = install_rust_fake(&mut session, "rust-no-cargo", "rust_analyzer");

    open_file(&mut session, &file);
    run_script(&mut session, b"wait 300\n");
    assert_eq!(rust_status(&session), Some(LspServerStatus::Disabled));
    assert_eq!(session.app.lsp.as_ref().map(|lsp| lsp.rust_row_info().roots), Some(0));
    assert_eq!(fake_starts(&executable), 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_rust_two_roots_are_independent() {
    let parent = scratch_dir("rust-two-roots");
    let first_root = parent.join("first");
    let second_root = parent.join("second");
    let first = rust_crate(&first_root);
    let second = rust_crate(&second_root);
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &parent);
    let executable = install_rust_fake(&mut session, "rust-two-roots", "rust_analyzer_diagnostics_unopened");

    open_file(&mut session, &first);
    open_file(&mut session, &second);
    open_all_problems(&mut session);
    wait_until(&mut session, 10000, "two Rust roots and diagnostics", |session| {
        session.app.lsp.as_ref().is_some_and(|lsp| {
            let info = lsp.rust_row_info();
            info.roots == 2 && info.status == LspServerStatus::Running
        }) && problem_paths(session).contains(&second)
    });
    close_tab(&mut session, 0);
    wait_until(&mut session, 8000, "one Rust root remains", |session| {
        session.app.lsp.as_ref().is_some_and(|lsp| {
            let info = lsp.rust_row_info();
            info.roots == 1 && info.status == LspServerStatus::Running
        }) && !problem_paths(session).contains(&first)
    });
    assert!(problem_paths(&session).contains(&second));
    assert_eq!(fake_starts(&executable), 2, "one rust-analyzer process per Cargo root");
    let _ = std::fs::remove_dir_all(parent);
}
