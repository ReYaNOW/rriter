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
    std::fs::create_dir_all(dir.join("src"))
        .unwrap_or_else(|error| panic!("create Rust fixture source directory: {error}"));
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap_or_else(|error| panic!("write Rust fixture Cargo.toml: {error}"));
    let file = dir.join("src/main.rs");
    std::fs::write(&file, "fn main() {\n    missing_name();\n}\n")
        .unwrap_or_else(|error| panic!("write Rust fixture main.rs: {error}"));
    file
}

pub(super) fn install_rust_fake(session: &mut HeadlessSession, name: &str, mode: &str) -> PathBuf {
    let tools = scratch_dir(&format!("{name}-tools"));
    install_fake_lsp(session, ToolKind::RustAnalyzer, &tools, mode)
}

pub(super) fn open_file(session: &mut HeadlessSession, file: &Path) {
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
    let ui = state["ui"]
        .as_array()
        .unwrap_or_else(|| panic!("UI array"));
    let rect = ui
        .iter()
        .find(|item| item["id"] == element)
        .unwrap_or_else(|| panic!("missing {element}: {state}"));
    let x = rect["rect"][0]
        .as_f64()
        .unwrap_or_else(|| panic!("tab close x"))
        + rect["rect"][2]
            .as_f64()
            .unwrap_or_else(|| panic!("tab close width"))
            / 2.0;
    let y = rect["rect"][1]
        .as_f64()
        .unwrap_or_else(|| panic!("tab close y"))
        + rect["rect"][3]
            .as_f64()
            .unwrap_or_else(|| panic!("tab close height"))
            / 2.0;
    run_script(session, format!("mouse_move {x} {y}\nwait 50\n").as_bytes());
    click_ui(session, &element);
}

fn rust_status(session: &HeadlessSession) -> Option<LspServerStatus> {
    session.app.lsp.as_ref().map(|lsp| lsp.rust_row_info().status)
}

pub(super) fn fake_starts(executable: &Path) -> usize {
    std::fs::read_to_string(executable.with_extension("starts"))
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

fn problem_paths(session: &HeadlessSession) -> Vec<PathBuf> {
    session.app.ide_panel.flat_diags.iter().map(|row| row.path.to_path_buf()).collect()
}

fn request_log(executable: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(executable.with_extension("requests.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn assert_request_for_root(executable: &Path, method: &str, root: &Path, file: &Path) {
    let root_uri = format!("file://{}", root.display());
    let file_uri = format!("file://{}", file.display());
    assert!(
        request_log(executable).iter().any(|request| {
            request["method"] == method
                && request["rootUri"] == root_uri
                && request["params"]["textDocument"]["uri"] == file_uri
        }),
        "missing {method} request for {file_uri} handled by {root_uri}: {:?}",
        request_log(executable)
    );
}

fn source_point(session: &mut HeadlessSession, line: usize, column: usize) -> (f32, f32) {
    let state = dump(session);
    let body = state["ui"]
        .as_array()
        .unwrap()
        .iter()
        .find(|element| element["id"] == "EditorTextBody")
        .unwrap_or_else(|| panic!("editor body missing: {state}"));
    let rect = body["rect"].as_array().unwrap();
    let body_x = rect[0].as_f64().unwrap() as f32;
    let body_y = rect[1].as_f64().unwrap() as f32;
    let renderer = session.app.renderer.as_ref().expect("headless renderer");
    let advance = renderer.ascii_advances['a' as usize];
    let line_height = renderer.line_height;
    (
        (body_x + column as f32 * advance + advance * 0.5).round(),
        (body_y + line as f32 * line_height + line_height * 0.5).round(),
    )
}

fn assert_ok(lines: Vec<String>) {
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
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
    std::fs::write(&sibling, "pub fn sibling() {}\n")
        .unwrap_or_else(|error| panic!("write sibling Rust file: {error}"));
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
    std::fs::write(&file, "fn main() {}\n")
        .unwrap_or_else(|error| panic!("write standalone Rust file: {error}"));
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

#[test]
fn headless_rust_hover_shows_rust_server_markdown_and_rust_highlighting() {
    let (dir, file, mut session) = rust_crate_session("rust-hover-request");
    let source = "fn main() {\n    hover_subject();\n}\n";
    std::fs::write(&file, source).expect("write Rust hover fixture");
    let executable = install_rust_fake(&mut session, "rust-hover-request", "rust_ide_requests");
    open_file(&mut session, &file);
    wait_until(&mut session, 8000, "Rust hover server", |session| {
        rust_status(session) == Some(LspServerStatus::Running)
    });

    let (x, y) = source_point(&mut session, 1, 5);
    assert_ok(run_script(&mut session, format!("mouse_move {x} {y}\n").as_bytes()));
    wait_until(&mut session, 5000, "Rust hover popup", |session| {
        session.app.hover.popup.is_some()
    });
    let popup = session.app.hover.popup.as_ref().expect("hover popup");
    assert!(popup.text.contains("rust_hover_value"), "unexpected hover: {}", popup.text);
    let struct_start = popup.text.find("struct").expect("Rust struct in hover block");
    assert!(
        popup.spans.iter().any(|span| span.start <= struct_start && struct_start < span.end),
        "Rust keyword was not highlighted: {:?}",
        popup.spans
    );
    assert_request_for_root(&executable, "textDocument/hover", &dir, &file);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_rust_completion_lists_and_inserts_rust_server_item() {
    let (dir, file, mut session) = rust_crate_session("rust-completion-request");
    let source = "fn main() {\n    rust_server_object";
    std::fs::write(&file, source).expect("write Rust completion fixture");
    let executable = install_rust_fake(&mut session, "rust-completion-request", "rust_ide_requests");
    open_file(&mut session, &file);
    wait_until(&mut session, 8000, "Rust completion server", |session| {
        rust_status(session) == Some(LspServerStatus::Running)
    });
    assert_ok(run_script(&mut session, b"key ctrl+end\ntype .\n"));
    wait_until(&mut session, 8000, "Rust server completion popup", |session| {
        session.app.autocomplete_active
            && session.app.autocomplete_options.iter().any(|(item, _)| item.word == "ra_completion_item")
    });
    assert_request_for_root(&executable, "textDocument/completion", &dir, &file);
    assert_ok(run_script(&mut session, b"key enter\n"));
    assert!(
        session.app.editor.get_full_text().contains("ra_completion_item"),
        "completion was not inserted: {}",
        session.app.editor.get_full_text()
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_rust_goto_definition_opens_server_target_in_same_root() {
    let (dir, file, mut session) = rust_crate_session("rust-definition-request");
    let source = "fn main() {\n    rust_definition_target();\n}\n";
    std::fs::write(&file, source).expect("write Rust definition fixture");
    let target = file.with_file_name("definitions.rs");
    std::fs::write(&target, "pub fn rust_definition_target() {}\n")
        .expect("write Rust definition target");
    let executable = install_rust_fake(&mut session, "rust-definition-request", "rust_ide_requests");
    open_file(&mut session, &file);
    wait_until(&mut session, 8000, "Rust definition server", |session| {
        rust_status(session) == Some(LspServerStatus::Running)
    });
    let (x, y) = source_point(&mut session, 1, 8);
    session.app.modifiers = winit::keyboard::ModifiersState::CONTROL;
    assert_ok(run_script(&mut session, format!("mouse_move {x} {y}\n").as_bytes()));
    wait_until(&mut session, 8000, "Rust definition response", |session| {
        session.app.ctrl_definition.target.is_some()
    });
    assert_request_for_root(&executable, "textDocument/definition", &dir, &file);
    assert_ok(run_script(
        &mut session,
        format!("click left down\nclick left up\n").as_bytes(),
    ));
    wait_until(&mut session, 5000, "Rust target tab", |session| {
        session.app.file_path.as_deref() == Some(target.as_path())
    });
    let state = dump(&mut session);
    assert_eq!(state["tabs"][1]["path"], target.display().to_string());
    assert_eq!(state["tabs"][1]["cursor"]["line"], 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_rust_signature_help_uses_rust_server_parameters() {
    let (dir, file, mut session) = rust_crate_session("rust-signature-help");
    // The closing `)` is pre-seeded: `type` commits text as IME input, which
    // does not auto-pair like the `(` key does, and signature help is only
    // requested inside a call's arguments. The `(` goes right after the
    // callee, not after the closing brace at EOF.
    let source = "fn main() {\n    rust_signature_target)\n}\n";
    std::fs::write(&file, source).expect("write Rust signature fixture");
    let executable = install_rust_fake(&mut session, "rust-signature-help", "rust_ide_requests");
    open_file(&mut session, &file);
    wait_until(&mut session, 8000, "Rust signature server", |session| {
        rust_status(session) == Some(LspServerStatus::Running)
    });
    assert_ok(run_script(&mut session, b"key ctrl+home\nkey down\nkey end\nkey left\ntype (\n"));
    wait_until(&mut session, 5000, "Rust signature parameter completion", |session| {
        session.app.autocomplete_options.iter().any(|(item, _)| item.word == "value")
    });
    assert_request_for_root(&executable, "textDocument/signatureHelp", &dir, &file);
    let _ = std::fs::remove_dir_all(dir);
}
