//! `--ide` startup: the first frame is the full IDE chrome with a blank editor area
//! (`startup_editor_pending`), the active tab is highlighted by the early worker request,
//! the other tabs are placeholders (`TabLoad`) filled by the deferred work, and input that
//! would reach the editor is ignored while the area is blank.
//!
//! The saved tab list is handed to `preload_ide_session` (tests never read the user's
//! `tabs_ide.txt`). Headless has no `--ide` flag: `run_ide_on_startup` + `preload_ide_session`
//! + `enter_ide_on_startup` is the native `resume` sequence.

use crate::OpenTabSnapshot;
use crate::app::TabLoad;
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

/// Native `resume` sequence for `--ide` with `tabs` saved (`active` is the saved active
/// index), stopped right after the IDE entry: no frame was drawn and nothing was polled, so
/// the blank-editor wait and the placeholders are in their initial state. Also returns the
/// editor version of the early highlight request, when one was sent.
fn entered_session(tabs: Vec<OpenTabSnapshot>, active: usize) -> (HeadlessSession, Option<u64>) {
    let mut session = session_for_test(1280, 800);
    session.app.run_ide_on_startup = true;
    session.app.preload_ide_session(tabs, active);
    let early_version = session.app.ide_preload.as_ref().and_then(|p| p.highlight_version);
    session.app.enter_ide_on_startup();
    (session, early_version)
}

fn titles(state: &serde_json::Value) -> Vec<String> {
    state["tabs"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|tab| tab["title"].as_str().unwrap_or_default().to_string())
        .collect()
}

fn loads(session: &HeadlessSession) -> Vec<TabLoad> {
    session.app.tabs.iter().map(|tab| tab.load).collect()
}

#[test]
fn headless_ide_startup_first_frame_is_chrome_then_editor() {
    let dir = scratch_dir("ui-ide-startup-first");
    let files = write_sources(&dir, &["alpha", "beta", "gamma"]);
    let saved = files.iter().cloned().map(OpenTabSnapshot::File).collect();
    let (mut session, early_version) = entered_session(saved, 1);
    let early_version = early_version.expect("the active file was read before the window");

    // At the entry: IDE chrome, the editor area is held back, inactive tabs are placeholders.
    let app = &session.app;
    assert!(app.is_ide_mode && app.startup_editor_pending.is_some());
    assert_eq!(app.active_tab, 1);
    assert_eq!(app.editor.version, early_version, "the early highlight request was reused");
    assert_eq!(loads(&session)[0], TabLoad::Pending);
    assert_eq!(loads(&session)[2], TabLoad::Pending);
    assert_eq!(session.app.tabs[0].editor.len(), 0);
    assert!(session.app.lsp.is_none());
    session.step(true);
    if session.app.startup_editor_pending.is_some() {
        let state = dump(&mut session);
        assert!(!has_ui(&state, "EditorTab(0)"), "no tab bar while the editor is held: {state}");
    }

    // Then the editor shows with spans, and the deferred work fills the other tabs.
    wait_until(&mut session, 5000, "editor shown and deferred restore", |s| {
        s.app.startup_editor_pending.is_none() && s.app.lsp.is_some()
    });
    session.step(true);
    let state = dump(&mut session);
    assert_eq!(titles(&state), ["alpha.rs", "beta.rs", "gamma.rs"], "{state}");
    assert!(has_ui(&state, "EditorTab(0)") && has_ui(&state, "EditorTab(2)"), "{state}");
    assert!(!state["editor"]["highlight_spans"].as_array().unwrap().is_empty(), "{state}");
    assert!(loads(&session).iter().all(|load| *load == TabLoad::Loaded));
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
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn headless_ide_startup_ignores_editor_input_while_blank() {
    let dir = scratch_dir("ui-ide-startup-input");
    let files = write_sources(&dir, &["alpha", "beta"]);
    let saved = files.iter().cloned().map(OpenTabSnapshot::File).collect();
    let (mut session, _) = entered_session(saved, 1);
    assert!(session.app.startup_editor_pending.is_some());

    let before = session.app.editor.get_full_text();
    session.app.handle_main_ime_commit("x");
    assert_eq!(session.app.editor.get_full_text(), before, "IME text must not reach the editor");
    assert_eq!(session.app.active_tab, 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn headless_ide_startup_duplicate_saved_path_opens_one_tab() {
    let dir = scratch_dir("ui-ide-startup-dup");
    let files = write_sources(&dir, &["alpha"]);
    let saved = vec![OpenTabSnapshot::File(files[0].clone()), OpenTabSnapshot::File(files[0].clone())];
    let (session, _) = entered_session(saved, 0);
    assert_eq!(session.app.tabs.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn headless_ide_startup_close_active_before_deferred_loads_neighbour() {
    let dir = scratch_dir("ui-ide-startup-close");
    let files = write_sources(&dir, &["alpha", "beta"]);
    let saved = files.iter().cloned().map(OpenTabSnapshot::File).collect();
    let (mut session, _) = entered_session(saved, 1);

    session.app.close_tab_at(1);
    assert_eq!(session.app.tabs.len(), 1);
    assert_eq!(session.app.editor.get_full_text(), rust_source("ALPHA"), "neighbour was read");
    assert_eq!(session.app.tabs[0].load, TabLoad::Loaded);
    wait_until(&mut session, 5000, "highlight of the neighbour", |s| {
        s.app.is_highlighted_once && s.app.startup_editor_pending.is_none()
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn headless_ide_startup_saving_a_pending_tab_keeps_the_file() {
    let dir = scratch_dir("ui-ide-startup-save");
    let files = write_sources(&dir, &["alpha", "beta"]);
    let saved = files.iter().cloned().map(OpenTabSnapshot::File).collect();
    let (mut session, _) = entered_session(saved, 1);
    assert_eq!(session.app.tabs[0].load, TabLoad::Pending);

    session.app.switch_to_tab(0);
    session.app.save_current_file();
    assert_eq!(std::fs::read_to_string(&files[0]).unwrap(), rust_source("ALPHA"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn headless_ide_startup_unreadable_pending_file_leaves_unnamed_tab() {
    let dir = scratch_dir("ui-ide-startup-unreadable");
    let files = write_sources(&dir, &["alpha"]);
    let binary = dir.join("blob.rs");
    std::fs::write(&binary, [0u8, 159, 146, 150, 0, 1, 2]).unwrap();
    let saved = vec![OpenTabSnapshot::File(binary.clone()), OpenTabSnapshot::File(files[0].clone())];
    let (mut session, _) = entered_session(saved, 1);

    session.app.finish_ide_deferred();
    assert!(session.app.tabs[0].file_path.is_none(), "no path on the empty buffer");
    assert_eq!(std::fs::read(&binary).unwrap(), [0u8, 159, 146, 150, 0, 1, 2]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn headless_ide_startup_poll_before_entry_keeps_the_early_highlight() {
    let dir = scratch_dir("ui-ide-startup-poll");
    let files = write_sources(&dir, &["alpha"]);
    let saved = vec![OpenTabSnapshot::File(files[0].clone())];
    let mut session = session_for_test(1280, 800);
    session.app.run_ide_on_startup = true;
    session.app.preload_ide_session(saved, 0);
    assert!(session.app.preloaded_highlight_awaits_ide_entry());
    // The worker answers while the window is still being created.
    std::thread::sleep(std::time::Duration::from_millis(300));
    session.app.enter_ide_on_startup();
    wait_until(&mut session, 5000, "early highlight applied", |s| {
        s.app.startup_editor_pending.is_none()
    });
    assert!(!session.app.highlighter.spans.is_empty());
    assert_eq!(session.app.highlighter.current_version, session.app.editor.version);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn headless_ide_startup_missing_active_file_falls_back() {
    let dir = scratch_dir("ui-ide-startup-missing");
    let files = write_sources(&dir, &["alpha"]);
    let saved = vec![
        OpenTabSnapshot::File(files[0].clone()),
        OpenTabSnapshot::File(dir.join("gone.rs")),
    ];
    let (mut session, early_version) = entered_session(saved, 1);
    assert_eq!(early_version, None, "the active file cannot be read early");
    session.app.finish_ide_deferred();
    assert!(session.app.tabs.iter().all(|tab| tab.load == TabLoad::Loaded));
    let _ = std::fs::remove_dir_all(&dir);
}

fn write_python(dir: &Path, names: &[&str]) -> Vec<PathBuf> {
    names
        .iter()
        .map(|name| {
            let path = dir.join(format!("{name}.py"));
            std::fs::write(&path, format!("def {name}():\n    return 1\n"))
                .unwrap_or_else(|err| panic!("write {}: {err}", path.display()));
            path.canonicalize().unwrap_or(path)
        })
        .collect()
}

#[test]
fn headless_ide_startup_deferred_work_reads_one_tab_per_step() {
    let dir = scratch_dir("ui-ide-startup-step");
    let files = write_sources(&dir, &["alpha", "beta", "gamma"]);
    let saved = files.iter().cloned().map(OpenTabSnapshot::File).collect();
    let (mut session, _) = entered_session(saved, 1);
    let unfinished =
        |session: &HeadlessSession| loads(session).iter().filter(|l| **l != TabLoad::Loaded).count();
    // The two inactive placeholders, and the active tab's git base.
    assert_eq!(unfinished(&session), 3, "{:?}", loads(&session));

    for expected in [2, 1, 0] {
        session.app.run_ide_deferred();
        assert_eq!(unfinished(&session), expected, "one tab per step: {:?}", loads(&session));
        assert!(session.app.lsp.is_none(), "the LSP is the last step");
    }
    session.app.run_ide_deferred();
    assert!(session.app.lsp.is_some());
    assert_eq!(session.app.ide_deferred, crate::app::IdeDeferred::None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn headless_ide_startup_tab_loaded_by_deferred_work_is_opened_on_a_running_lsp() {
    let dir = scratch_dir("ui-ide-startup-lsp");
    let files = write_python(&dir, &["alpha", "beta"]);
    let saved = files.iter().cloned().map(OpenTabSnapshot::File).collect();
    let (mut session, _) = entered_session(saved, 1);
    crate::headless::tests_support::disable_python_lsp(&mut session, vec![dir.clone()]);
    let opened = |session: &HeadlessSession| {
        session.app.lsp.as_ref().unwrap().open_python_paths_for_test()
    };
    assert!(!opened(&session).contains(&files[0]), "placeholder is not sent to the LSP");

    session.app.run_ide_deferred();
    assert_eq!(session.app.tabs[0].load, TabLoad::Loaded);
    assert!(opened(&session).contains(&files[0]), "{:?}", opened(&session));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn headless_ide_startup_key_gate_blocks_editor_input_but_not_global_chords() {
    use crate::app::keyboard::KeyInput;

    let dir = scratch_dir("ui-ide-startup-keys");
    let files = write_sources(&dir, &["alpha", "beta"]);
    let saved = files.iter().cloned().map(OpenTabSnapshot::File).collect();
    let (mut session, _) = entered_session(saved, 1);
    assert!(session.app.startup_editor_pending.is_some());
    let gate = |session: &mut HeadlessSession, combo: &str| {
        let (input, mods) = KeyInput::parse_combo(combo).unwrap();
        session.app.modifiers = mods;
        let blocked = session.app.startup_blocks_key_input(&input);
        let release_blocked = session.app.startup_blocks_key_input(&input.released());
        assert!(!release_blocked, "a release always passes: {combo}");
        blocked
    };

    // The editor has the input focus: typing and tab/document chords are held back.
    assert!(gate(&mut session, "a"));
    assert!(gate(&mut session, "ctrl+s"));
    assert!(gate(&mut session, "ctrl+z"));
    assert!(gate(&mut session, "ctrl+tab"));
    // App-global chords pass.
    assert!(!gate(&mut session, "f1"));
    assert!(!gate(&mut session, "alt+q"));
    assert!(!gate(&mut session, "alt+w"));

    // Another surface has the focus: its typing passes, tab chords are still held back.
    session.app.search_focused = true;
    assert!(!gate(&mut session, "a"));
    assert!(gate(&mut session, "ctrl+s"));
    assert!(gate(&mut session, "ctrl+z"));
    assert!(!gate(&mut session, "ctrl+f"));

    // Nothing is held back once the wait is over.
    session.app.startup_editor_pending = None;
    assert!(!gate(&mut session, "ctrl+s"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn headless_ide_startup_pointer_gate_yields_to_overlays() {
    let dir = scratch_dir("ui-ide-startup-pointer");
    let files = write_sources(&dir, &["alpha", "beta"]);
    let saved = files.iter().cloned().map(OpenTabSnapshot::File).collect();
    let (mut session, _) = entered_session(saved, 1);
    {
        let renderer = session.app.renderer.as_mut().expect("renderer");
        renderer.last_mouse_x = renderer.width - 40.0;
        renderer.last_mouse_y = 200.0;
    }
    assert!(session.app.startup_blocks_pointer_input(), "editor area press is held back");

    session.app.show_settings = true;
    assert!(!session.app.startup_blocks_pointer_input(), "the settings page is not blank");
    session.app.show_settings = false;

    session.app.renderer.as_mut().unwrap().last_mouse_x = 4.0;
    assert!(!session.app.startup_blocks_pointer_input(), "the activity bar is not blank");

    session.app.startup_editor_pending = None;
    session.app.renderer.as_mut().unwrap().last_mouse_x = 1240.0;
    assert!(!session.app.startup_blocks_pointer_input(), "nothing is held back after the wait");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn headless_ide_startup_plain_text_file_does_not_wait_for_the_deadline() {
    let dir = scratch_dir("ui-ide-startup-txt");
    let notes = dir.join("notes.txt");
    std::fs::write(&notes, "plain text, no grammar\nsecond line\n").unwrap();
    let notes = notes.canonicalize().unwrap_or(notes);
    let (mut session, early_version) = entered_session(vec![OpenTabSnapshot::File(notes)], 0);
    assert!(early_version.is_some(), "the active file was read before the window");

    // The worker answers every `Reset` (spans may be empty), well before the 1.2 s deadline.
    let started = std::time::Instant::now();
    wait_until(&mut session, 1000, "editor shown for a plain text file", |s| {
        s.app.startup_editor_pending.is_none()
    });
    assert!(started.elapsed() < crate::app::FILE_OPEN_LARGE_PRIORITY_HIGHLIGHT_TIMEOUT);
    assert!(session.app.is_highlighted_once);
    let _ = std::fs::remove_dir_all(&dir);
}
