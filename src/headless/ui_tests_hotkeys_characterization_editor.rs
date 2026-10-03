//! Characterization of editor-owned hotkeys before keymap changes.

use crate::headless::tests_support::{
    click_ui, dump, git, git_init, has_ui, keyboard_session, run_script, scratch_dir,
    session_for_test, ui_center, wait_until, workspace_with_explorer,
};
use crate::headless::ui_tests_hotkeys_characterization::assert_chord_effect;
use crate::headless::HeadlessSession;

pub(super) const EXCLUDED_EDITOR_ROWS: &[(&str, &str)] = &[
    (
        "app.quit (Ctrl+Q on Welcome)",
        "the shortcut exits the headless event loop before a post-key state can be inspected",
    ),
];

#[test]
fn headless_hotkeys_editor_exclusions_have_reasons() {
    for (row, reason) in EXCLUDED_EDITOR_ROWS {
        assert!(!row.is_empty() && !reason.is_empty());
    }
}

fn run_ok(session: &mut HeadlessSession, script: &str) {
    let lines = run_script(session, script.as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
}

fn open_writable_file(path: &std::path::Path) -> HeadlessSession {
    let options = crate::headless::profile::HeadlessOptions {
        size: (1280, 720),
        allow_writes: true,
        ..Default::default()
    };
    let root = crate::headless::tests_support::ensure_test_profile_root();
    let mut session = match crate::headless::HeadlessSession::new(&options, root) {
        Ok(session) => session,
        Err((code, message)) => panic!("headless session (code {code}): {message}"),
    };
    session.hz_probe = || None;
    run_ok(
        &mut session,
        &format!("scale 1.3333334\nopen {}\nsettle 2000\n", path.display()),
    );
    session
}

#[test]
fn headless_hotkeys_editor_save_and_history() {
    let dir = scratch_dir("ui-hotkeys-editor-history");
    let file = dir.join("history.txt");
    let original = "alpha\n";
    std::fs::write(&file, original).expect("write editor fixture");
    let mut session = open_writable_file(&file);

    run_ok(&mut session, "type x\n");
    let edited = "xalpha\n";
    assert_eq!(session.app.editor.get_full_text(), edited);
    assert_eq!(dump(&mut session)["tabs"][0]["modified"], true);

    run_ok(&mut session, "key ctrl+shift+z\n");
    assert_eq!(session.app.editor.get_full_text(), edited, "Ctrl+Shift+Z currently performs redo");
    run_ok(&mut session, "key ctrl+y\n");
    assert_eq!(session.app.editor.get_full_text(), edited);
    run_ok(&mut session, "key ctrl+s\n");
    assert_eq!(std::fs::read_to_string(&file).expect("read saved fixture"), edited);
    assert_eq!(dump(&mut session)["tabs"][0]["modified"], false);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_ctrl_o_on_welcome_requests_file_picker() {
    let mut session = session_for_test(1280, 720);
    assert_eq!(dump(&mut session)["mode"], "welcome");

    run_ok(&mut session, "key ctrl+o\n");
    assert_eq!(dump(&mut session)["external_request"]["kind"], "pick_file");
}

#[test]
fn headless_hotkeys_editor_search_completion_and_open() {
    let dir = scratch_dir("ui-hotkeys-editor-search-open");
    let search_file = dir.join("search.txt");
    let next_file = dir.join("next.txt");
    let completion_file = dir.join("completion.rs");
    let markdown_file = dir.join("notes.md");
    std::fs::write(&search_file, "needle one\nother\nneedle two\n").expect("write search fixture");
    std::fs::write(&next_file, "next file\n").expect("write open fixture");
    std::fs::write(&completion_file, "fn primary() {}\nfn example() {\n    pri")
        .expect("write completion fixture");
    std::fs::write(&markdown_file, "# Notes\n\n## Details\n").expect("write Markdown fixture");
    let mut session = session_for_test(1280, 720);
    run_ok(
        &mut session,
        &format!("workspace {}\nopen {}\nsettle 2000\n", dir.display(), search_file.display()),
    );

    assert_chord_effect(
        &mut session,
        "ctrl+f",
        |_| {},
        |session| assert_eq!(dump(session)["overlays"]["search"], true),
    );
    run_ok(&mut session, "type needle\n");
    run_ok(&mut session, "key ctrl+f\n");
    assert_eq!(session.app.search_editor.get_selection().as_deref(), Some("needle"));
    assert_eq!(dump(&mut session)["overlays"]["search"], true);
    let first_match_line = dump(&mut session)["tabs"][0]["cursor"]["line"]
        .as_i64()
        .unwrap();
    run_ok(&mut session, "key enter\n");
    assert_ne!(
        dump(&mut session)["tabs"][0]["cursor"]["line"]
            .as_i64()
            .unwrap(),
        first_match_line
    );
    run_ok(&mut session, "key escape\n");
    assert_eq!(dump(&mut session)["overlays"]["search"], false);

    click_ui(&mut session, "EditorTextBody");
    session.app.external_requests.queue_picker_answer(vec![next_file.clone()]);
    run_ok(&mut session, "key ctrl+o\nsettle 500\n");
    wait_until(&mut session, 5000, "Ctrl+O open file", |session| {
        dump(session)["tabs"].as_array().is_some_and(|tabs| tabs.len() == 2)
    });
    assert_eq!(
        dump(&mut session)["tabs"][1]["path"],
        next_file.display().to_string()
    );

    run_ok(
        &mut session,
        &format!(
            "open {}\nsettle 2000\nkey ctrl+end\nkey ctrl+space\n",
            completion_file.display()
        ),
    );
    wait_until(&mut session, 8000, "Ctrl+Space completion", |session| {
        session.app.autocomplete_active && !session.app.autocomplete_options.is_empty()
    });
    run_ok(&mut session, "key ctrl+4\n");
    wait_until(&mut session, 5000, "Ctrl+4 closes active editor tab", |session| {
        dump(session)["tabs"].as_array().is_some_and(|tabs| tabs.len() == 2)
    });
    run_ok(
        &mut session,
        &format!(
            "open {}\nsettle 1000\nkey ctrl+shift+o\n",
            markdown_file.display()
        ),
    );
    assert_eq!(dump(&mut session)["markdown_toc"]["open"], true);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_alt_enter_opens_editor_lsp_actions() {
    let (dir, mut session) = keyboard_session("ui-hotkeys-editor-code-actions");
    assert_chord_effect(
        &mut session,
        "alt+enter",
        |_| {},
        |session| assert_eq!(dump(session)["overlays"]["lsp_actions_menu"], true),
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_git_diff_blocks_cut_paste_and_comment() {
    const TEST_WIDTH: u32 = 1280;
    const TEST_HEIGHT: u32 = 720;
    const TEST_SCALE: f32 = 4.0 / 3.0;

    let dir = scratch_dir("ui-hotkeys-editor-git-diff-readonly");
    let file = dir.join("changed.py");
    std::fs::write(&file, "before\n").expect("write Git diff baseline");
    git_init(&dir);
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-qm", "fixture"]);
    std::fs::write(&file, "after\n").expect("write Git diff change");

    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    click_ui(&mut session, "SidebarSlot(Git)");
    wait_until(&mut session, 8000, "changed Git file", |session| {
        has_ui(&dump(session), "GitFileDiff(0, 0)")
    });
    let (x, y) = ui_center(&dump(&mut session), "GitFileDiff(0, 0)");
    run_ok(&mut session, &format!("mouse_move {x} {y}\ndblclick\n"));
    wait_until(&mut session, 5000, "Git diff tab", |session| {
        dump(session)["tabs"].as_array().is_some_and(|tabs| {
            tabs.iter().any(|tab| tab["kind"] == "git_diff")
        })
    });

    let original = session.app.editor.get_full_text();
    run_ok(&mut session, "key ctrl+a\nkey ctrl+x\n");
    assert_eq!(session.app.editor.get_full_text(), original);
    assert_eq!(dump(&mut session)["overlays"]["readonly_notice"], true);

    run_ok(&mut session, "key ctrl+v\n");
    assert_eq!(session.app.editor.get_full_text(), original);
    assert_eq!(dump(&mut session)["overlays"]["readonly_notice"], true);

    run_ok(&mut session, "key ctrl+/\n");
    assert_eq!(session.app.editor.get_full_text(), original);
    assert_eq!(dump(&mut session)["overlays"]["readonly_notice"], true);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_editor_selection_comment_and_settings() {
    let dir = scratch_dir("ui-hotkeys-editor-selection");
    let file = dir.join("main.py");
    std::fs::write(&file, "alpha beta\n").expect("write selection fixture");
    let mut session = session_for_test(1280, 720);
    run_ok(&mut session, &format!("open {}\nsettle 2000\n", file.display()));

    run_ok(&mut session, "key ctrl+a\n");
    assert!(session.app.editor.selection_anchor.is_some());
    run_ok(&mut session, "key ctrl+c\n");
    assert_eq!(dump(&mut session)["clipboard"]["text"], "alpha beta\n");
    run_ok(&mut session, "key ctrl+x\n");
    assert_eq!(session.app.editor.get_full_text(), "");
    run_ok(&mut session, "key ctrl+v\n");
    assert_eq!(session.app.editor.get_full_text(), "alpha beta\n");

    run_ok(&mut session, "key ctrl+home\nkey ctrl+w\n");
    assert!(session.app.editor.selection_anchor.is_some());

    run_ok(&mut session, "key ctrl+home\nkey ctrl+/\n");
    assert_eq!(session.app.editor.get_full_text(), "#alpha beta\n");
    run_ok(&mut session, "key ctrl+z\n");
    assert_eq!(session.app.editor.get_full_text(), "alpha beta\n");

    run_ok(&mut session, "key f1\n");
    assert_eq!(dump(&mut session)["overlays"]["settings"], true);
    run_ok(&mut session, "key f1\n");
    assert_eq!(dump(&mut session)["overlays"]["settings"], false);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_editor_ctrl_q_closes_tabs_in_both_modes() {
    let (ide_dir, mut ide) = keyboard_session("ui-hotkeys-editor-close-ide");
    run_ok(&mut ide, "key ctrl+q\n");
    wait_until(&mut ide, 5000, "IDE Ctrl+Q close all tabs", |session| {
        dump(session)["tabs"].as_array().is_some_and(|tabs| tabs.is_empty())
    });
    // today: closing the last IDE tab returns to Welcome.
    assert_eq!(dump(&mut ide)["mode"], "welcome");
    let _ = std::fs::remove_dir_all(ide_dir);

    let dir = scratch_dir("ui-hotkeys-editor-close-file");
    let file = dir.join("single.txt");
    std::fs::write(&file, "only file\n").expect("write single-file fixture");
    let mut editor = session_for_test(1280, 720);
    run_ok(
        &mut editor,
        &format!("open {}\nsettle 1000\nkey ctrl+q\n", file.display()),
    );
    assert!(dump(&mut editor)["tabs"]
        .as_array()
        .is_some_and(|tabs| tabs.is_empty()));
    assert_eq!(dump(&mut editor)["mode"], "welcome");
    let _ = std::fs::remove_dir_all(dir);
}
