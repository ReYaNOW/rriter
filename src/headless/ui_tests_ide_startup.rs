//! `--ide` startup: the active tab is highlighted in the first content frame, the other tabs
//! are restored as placeholders and filled after that frame, and a highlight request made
//! before the window exists (`App::preload_ide_startup`) is reused instead of repeated.
//!
//! The saved tab list is seeded through `set_test_open_tabs` (tests never read the user's
//! `tabs_ide.txt`); `run_ide_on_startup` plus one forced step is the native first
//! `about_to_wait` followed by the first content frame.

use crate::OpenTabSnapshot;
use crate::headless::HeadlessSession;
use crate::headless::tests_support::{
    click_ui, dump, has_ui, scratch_dir, session_for_test, wait_until,
};
use std::path::{Path, PathBuf};

fn rust_source(name: &str) -> String {
    format!(
        "// {name}\nuse std::fmt;\n\nstruct {name} {{\n    value: i32,\n}}\n\nimpl {name} {{\n    \
         fn get(&self) -> i32 {{\n        let doubled = self.value * 2;\n        doubled + 1\n    }}\n}}\n"
    )
}

fn write_sources(dir: &Path, names: &[&str]) -> Vec<PathBuf> {
    names
        .iter()
        .map(|name| {
            let path = dir.join(format!("{name}.rs"));
            std::fs::write(&path, rust_source(&name.to_uppercase()))
                .unwrap_or_else(|err| panic!("write {}: {err}", path.display()));
            path.canonicalize().unwrap_or(path)
        })
        .collect()
}

/// Startup of a session with `tabs` saved (`active` is the saved active index), stopped right
/// after the first content frame: the deferred restore work has not run yet. Also returns the
/// editor version of the early highlight request, when one was sent.
fn startup_session(tabs: Vec<OpenTabSnapshot>, active: usize) -> (HeadlessSession, Option<u64>) {
    let mut session = session_for_test(1280, 800);
    crate::set_test_open_tabs(tabs, active);
    session.app.run_ide_on_startup = true;
    session.app.preload_ide_startup();
    let early_version = session.app.ide_preload.as_ref().and_then(|p| p.highlight_version);
    assert!(session.step(true), "the first step draws the content frame");
    (session, early_version)
}

fn titles(state: &serde_json::Value) -> Vec<String> {
    state["tabs"]
        .as_array()
        .expect("tabs array")
        .iter()
        .map(|tab| tab["title"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[test]
fn headless_ide_startup_first_frame_has_active_tab_highlighted() {
    let dir = scratch_dir("ui-ide-startup-first");
    let files = write_sources(&dir, &["alpha", "beta", "gamma"]);
    let saved = files.iter().cloned().map(OpenTabSnapshot::File).collect();
    let (mut session, early_version) = startup_session(saved, 1);
    let early_version = early_version.expect("the active file was read before the window");

    // The first content frame: active tab `beta` with spans, all three tabs in the bar.
    let app = &session.app;
    assert_eq!(app.active_tab, 1);
    assert_eq!(app.file_path.as_deref(), Some(files[1].as_path()));
    assert_eq!(app.editor.get_full_text(), rust_source("BETA"));
    assert!(!app.highlighter.spans.is_empty(), "first frame shows code without highlighting");
    assert_eq!(app.highlighter.current_version, app.editor.version);
    assert_eq!(app.editor.version, early_version, "the early highlight request was reused");
    let preload = app.ide_preload.as_ref().expect("preload is kept until the deferred work");
    assert!(preload.file.is_none(), "the early read was used by the tab");
    let state = dump(&mut session);
    assert_eq!(titles(&state), ["alpha.rs", "beta.rs", "gamma.rs"], "{state}");
    assert!(has_ui(&state, "EditorTab(0)") && has_ui(&state, "EditorTab(2)"), "{state}");
    assert!(!state["editor"]["highlight_spans"].as_array().unwrap().is_empty());

    // Inactive tabs are placeholders until the work after the first frame.
    assert_eq!(session.app.pending_tab_loads.len(), 2);
    assert_eq!(session.app.tabs[0].editor.len(), 0);
    assert!(session.app.lsp.is_none());
    wait_until(&mut session, 5000, "deferred IDE restore", |session| {
        session.app.pending_tab_loads.is_empty() && session.app.lsp.is_some()
    });
    assert_eq!(session.app.tabs[0].editor.get_full_text(), rust_source("ALPHA"));
    assert_eq!(session.app.tabs[2].editor.get_full_text(), rust_source("GAMMA"));

    // An inactive tab is highlighted when it becomes active.
    click_ui(&mut session, "EditorTab(0)");
    wait_until(&mut session, 5000, "highlight of the tab switched to", |session| {
        let state = dump(session);
        state["tabs"][0]["active"] == true
            && !state["editor"]["highlight_spans"].as_array().unwrap().is_empty()
    });
    assert_eq!(session.app.editor.get_full_text(), rust_source("ALPHA"));
    assert_eq!(session.app.highlighter.current_version, session.app.editor.version);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn headless_ide_startup_switch_before_deferred_work_reads_the_tab() {
    let dir = scratch_dir("ui-ide-startup-early-switch");
    let files = write_sources(&dir, &["alpha", "beta"]);
    let saved = files.iter().cloned().map(OpenTabSnapshot::File).collect();
    let (mut session, _) = startup_session(saved, 1);

    // A tab switched to before the deferred work is read by `switch_to_tab` itself.
    session.app.switch_to_tab(0);
    assert_eq!(session.app.editor.get_full_text(), rust_source("ALPHA"));
    assert!(!session.app.highlighter.spans.is_empty());
    wait_until(&mut session, 5000, "deferred IDE restore", |session| {
        session.app.pending_tab_loads.is_empty()
    });
    assert_eq!(session.app.tabs[1].editor.get_full_text(), rust_source("BETA"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn headless_ide_startup_missing_active_file_falls_back() {
    let dir = scratch_dir("ui-ide-startup-missing");
    let files = write_sources(&dir, &["alpha"]);
    let missing = dir.join("gone.rs");
    let saved = vec![OpenTabSnapshot::File(files[0].clone()), OpenTabSnapshot::File(missing)];
    let (session, early_version) = startup_session(saved, 1);
    assert_eq!(early_version, None, "the active file cannot be read early");

    // The saved active tab is gone: the session opens the remaining tab, highlighted.
    assert_eq!(session.app.tabs.len(), 1);
    assert_eq!(session.app.file_path.as_deref(), Some(files[0].as_path()));
    assert_eq!(session.app.editor.get_full_text(), rust_source("ALPHA"));
    assert!(!session.app.highlighter.spans.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn headless_ide_startup_shifted_active_index_ignores_early_read() {
    let dir = scratch_dir("ui-ide-startup-shifted");
    let files = write_sources(&dir, &["alpha", "beta"]);
    // The early read targets snapshot 1 (`alpha`), but the missing first entry shifts the
    // restored tabs, so index 1 is `beta`: the early highlight request must not be used.
    let saved = vec![
        OpenTabSnapshot::File(dir.join("gone.rs")),
        OpenTabSnapshot::File(files[0].clone()),
        OpenTabSnapshot::File(files[1].clone()),
    ];
    let (session, early_version) = startup_session(saved, 1);
    let early_version = early_version.expect("snapshot 1 was read early");

    assert_eq!(session.app.tabs.len(), 2);
    assert_eq!(session.app.file_path.as_deref(), Some(files[1].as_path()));
    assert_eq!(session.app.editor.get_full_text(), rust_source("BETA"));
    assert!(!session.app.highlighter.spans.is_empty());
    assert_eq!(session.app.highlighter.current_version, session.app.editor.version);
    assert_ne!(session.app.editor.version, early_version);
    let preload = session.app.ide_preload.as_ref().expect("preload is kept");
    assert!(preload.file.is_some(), "the early read belongs to the inactive tab");
    let _ = std::fs::remove_dir_all(&dir);
}
