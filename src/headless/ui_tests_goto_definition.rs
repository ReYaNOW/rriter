//! Ctrl-hover highlighting and Ctrl-click navigation through Python definitions.

use crate::headless::tests_support::{dump, run_script, scratch_dir, session_for_test, wait_until};
use crate::headless::HeadlessSession;
use std::path::{Path, PathBuf};
use winit::keyboard::ModifiersState;

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;

fn definition_workspace(name: &str, files: &[(&str, &str)], active: &str) -> (HeadlessSession, PathBuf) {
    let dir = scratch_dir(name);
    for (name, source) in files {
        std::fs::write(dir.join(name), source).expect("write Python definition fixture");
    }
    let mut session = session_for_test(TEST_WIDTH, TEST_HEIGHT);
    let lines = run_script(
        &mut session,
        format!("scale {TEST_SCALE}\nworkspace {}\nopen {}\nsettle 2000\n", dir.display(), dir.join(active).display())
            .as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    (session, dir)
}

fn ty_available(session: &mut HeadlessSession) -> bool {
    let Some(index) = session
        .app
        .ide_panel
        .lsp_servers
        .iter()
        .position(|server| server.name == "ty")
    else {
        return false;
    };
    wait_until(session, 8000, "ty LSP startup", |session| {
        session.app.ide_panel.lsp_servers.get(index).is_some_and(|server| {
            server.status != crate::lsp::LspServerStatus::Starting
        })
    });
    session
        .app
        .ide_panel
        .lsp_servers
        .get(index)
        .is_some_and(|server| server.status == crate::lsp::LspServerStatus::Running)
}

fn move_to_source_offset(session: &mut HeadlessSession, byte_offset: usize) -> (f32, f32) {
    let state = dump(session);
    let body = state["ui"]
        .as_array()
        .unwrap()
        .iter()
        .find(|element| element["id"] == "EditorTextBody")
        .unwrap_or_else(|| panic!("editor text body missing: {state}"));
    let body_rect = body["rect"].as_array().unwrap();
    let body_x = body_rect[0].as_f64().unwrap() as f32;
    let body_y = body_rect[1].as_f64().unwrap() as f32;
    let (x, y) = {
        let app = &mut session.app;
        let scroll_x = app.scroll_x.current.round();
        let scroll_y = app.scroll_y.current.round();
        let editor = &app.editor;
        let line = editor
            .line_offsets
            .partition_point(|&offset| offset <= byte_offset)
            .saturating_sub(1);
        let line_start = editor.line_offsets.get(line).copied().unwrap_or(0);
        let renderer = app.renderer.as_mut().expect("headless renderer");
        let visual_line = renderer.phys_to_visual.get(line).copied().unwrap_or(line) as f32;
        let text_x = renderer.visual_x_for_byte_offset(editor, line_start, byte_offset, true);
        let char_advance = renderer.ascii_advances['a' as usize];
        (
            (body_x + text_x + char_advance * 0.5 - scroll_x).round(),
            (body_y + visual_line * renderer.line_height + renderer.line_height * 0.5 - scroll_y)
                .round(),
        )
    };
    let lines = run_script(session, format!("mouse_move {x} {y}\n").as_bytes());
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    (x, y)
}

fn set_ctrl(session: &mut HeadlessSession, down: bool) {
    // The headless key command presses and releases modifiers together, so Ctrl must span mouse events directly.
    session.app.modifiers = if down {
        ModifiersState::CONTROL
    } else {
        ModifiersState::empty()
    };
    if !down {
        session.app.clear_ctrl_definition();
    }
}

fn wait_for_definition(session: &mut HeadlessSession) {
    wait_until(session, 10_000, "ty definition response", |session| {
        session.app.ctrl_definition.target.is_some()
    });
}

fn begin_ctrl_hover(session: &mut HeadlessSession, byte_offset: usize) -> (f32, f32) {
    set_ctrl(session, true);
    let point = move_to_source_offset(session, byte_offset);
    wait_for_definition(session);
    point
}

fn skip_without_ty(session: &mut HeadlessSession, dir: &Path) -> bool {
    if ty_available(session) {
        return false;
    }
    eprintln!("skip: no available ty LSP server");
    let _ = std::fs::remove_dir_all(dir);
    true
}

#[test]
fn headless_goto_definition_ctrl_hover_highlights_exact_symbol_and_release_clears_it() {
    let source = "def goto_target():\n    return 1\n\nresult = goto_target()\n";
    let (mut session, dir) = definition_workspace(
        "ui-goto-definition-hover",
        &[("main.py", source)],
        "main.py",
    );
    if skip_without_ty(&mut session, &dir) {
        return;
    }

    let use_offset = source.rfind("goto_target").unwrap();
    let expected_range = (use_offset, use_offset + "goto_target".len());
    let _ = begin_ctrl_hover(&mut session, use_offset);
    assert_eq!(session.app.ctrl_definition.source_range, Some(expected_range));
    assert_eq!(session.app.ctrl_definition_highlight_range(), Some(expected_range));

    let blank_line = source.find("\n\n").unwrap() + 1;
    let _ = move_to_source_offset(&mut session, blank_line);
    assert!(session.app.ctrl_definition.target.is_none());
    assert_eq!(session.app.ctrl_definition_highlight_range(), None);

    let _ = move_to_source_offset(&mut session, use_offset);
    wait_for_definition(&mut session);
    set_ctrl(&mut session, false);
    assert_eq!(session.app.ctrl_definition_highlight_range(), None);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_goto_definition_ctrl_click_jumps_to_same_file_definition_and_scrolls() {
    let mut source = String::from("def same_file_target():\n    return 9\n\n");
    for line in 0..45 {
        source.push_str(&format!("padding_{line} = {line}\n"));
    }
    source.push_str("\nsame_file_target()\n");
    let (mut session, dir) = definition_workspace(
        "ui-goto-definition-same-file",
        &[("main.py", &source)],
        "main.py",
    );
    if skip_without_ty(&mut session, &dir) {
        return;
    }

    let lines = run_script(&mut session, b"key ctrl+end\nsettle 500\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let before = dump(&mut session);
    let before_scroll = before["tabs"][0]["scroll_y"].as_f64().unwrap();
    let use_offset = source.rfind("same_file_target").unwrap();
    let (x, y) = begin_ctrl_hover(&mut session, use_offset);
    let lines = run_script(
        &mut session,
        format!("mouse_move {x} {y}\nclick left down\nclick left up\nsettle 500\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    wait_until(&mut session, 5000, "same-file definition jump", |session| {
        dump(session)["tabs"][0]["cursor"]["line"] == 1
    });
    let after = dump(&mut session);
    assert_eq!(after["tabs"][0]["cursor"]["line"], 1, "{after}");
    assert!(after["tabs"][0]["scroll_y"].as_f64().unwrap() < before_scroll, "{after}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_goto_definition_ctrl_click_opens_definition_in_another_file() {
    let source = "from definitions import imported_target\n\nimported_target()\n";
    let (mut session, dir) = definition_workspace(
        "ui-goto-definition-cross-file",
        &[
            ("definitions.py", "def imported_target():\n    return 23\n"),
            ("main.py", source),
        ],
        "main.py",
    );
    if skip_without_ty(&mut session, &dir) {
        return;
    }

    let use_offset = source.rfind("imported_target").unwrap();
    let (x, y) = begin_ctrl_hover(&mut session, use_offset);
    let lines = run_script(
        &mut session,
        format!("mouse_move {x} {y}\nclick left down\nclick left up\nsettle 500\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let target_path = dir.join("definitions.py").display().to_string();
    wait_until(&mut session, 5000, "cross-file definition tab", |session| {
        let state = dump(session);
        state["tabs"].as_array().is_some_and(|tabs| {
            tabs.len() == 2
                && tabs.iter().any(|tab| tab["active"] == true && tab["path"] == target_path)
        })
    });
    let after = dump(&mut session);
    assert_eq!(after["tabs"].as_array().unwrap().len(), 2, "{after}");
    assert_eq!(after["tabs"][1]["path"], target_path, "{after}");
    assert!(after["tabs"][1]["active"].as_bool().unwrap(), "{after}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_goto_definition_moving_to_non_symbol_clears_target_and_click_does_not_navigate() {
    let source = "def untouched_target():\n    return 3\n\nuntouched_target()\n\n";
    let (mut session, dir) = definition_workspace(
        "ui-goto-definition-non-symbol",
        &[("main.py", source)],
        "main.py",
    );
    if skip_without_ty(&mut session, &dir) {
        return;
    }

    let use_offset = source.rfind("untouched_target").unwrap();
    let _ = begin_ctrl_hover(&mut session, use_offset);
    let blank_line = source.rfind("\n\n").unwrap() + 1;
    let (x, y) = move_to_source_offset(&mut session, blank_line);
    assert!(session.app.ctrl_definition.target.is_none());
    let before_tabs = session.app.tabs.len();
    let lines = run_script(
        &mut session,
        format!("mouse_move {x} {y}\nclick left down\nclick left up\nsettle 200\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    assert_eq!(session.app.tabs.len(), before_tabs);
    assert_eq!(session.app.file_path.as_deref(), Some(dir.join("main.py").as_path()));
    let _ = std::fs::remove_dir_all(dir);
}
