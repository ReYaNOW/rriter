use crate::headless::tests_support::{click_ui, dump, has_ui, long_file, ok_json, open_file_session, run_script, sample_file, scratch_dir, session_for_test, ui_center};

#[test]
fn headless_editor_selection_and_cursor_at_multiple_scales() {
    let dir = scratch_dir("ui-editor-selection");
    let file = dir.join("select.txt");
    std::fs::write(&file, "alpha bravo charlie\nsecond line\n").unwrap();
    for scale in [1.0, 1.5] {
        let mut session = open_file_session(900, 600, scale, &file);
        let (x, y) = ui_center(&dump(&mut session), "EditorTextBody");
        let script = format!("mouse_move {} {}\ndblclick\nkey shift+right\n", x + 15.0, y + 15.0);
        run_script(&mut session, script.as_bytes());
        let selected = dump(&mut session);
        assert!(selected["editor"]["selection"].is_array() || selected["editor"]["selection"].is_object(), "{selected}");
        let before = selected["tabs"][0]["cursor"].clone();
        run_script(&mut session, b"key backspace\n");
        let after = dump(&mut session)["tabs"][0]["cursor"].clone();
        assert_ne!(before, after, "backspace should move the cursor after selection handling");
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_drag_selection_and_fold_unfold() {
    let dir = scratch_dir("ui-editor-fold");
    let file = dir.join("fold.rs");
    std::fs::write(
        &file,
        "\n\n\nfn outer() {\n    fn inner() {\n        let value = 1;\n    }\n}\nfn sibling() {}\n",
    )
    .unwrap();
    let mut session = open_file_session(1000, 700, 1.0, &file);
    let before = dump(&mut session);
    let body = before["ui"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "EditorTextBody")
        .unwrap();
    let rect = body["rect"].as_array().unwrap();
    let x = rect[0].as_f64().unwrap() + 20.0;
    let y = rect[1].as_f64().unwrap() + 78.0;
    run_script(
        &mut session,
        format!("mouse_move {x} {y}\nclick down\nmouse_move {} {}\nclick up\n", x + 100.0, y + 42.0)
            .as_bytes(),
    );
    let selected = dump(&mut session);
    assert!(selected["editor"]["selection"].is_object() || selected["editor"]["selection"].is_array(), "{selected}");
    let initial = dump(&mut session);
    assert!(
        has_ui(&initial, "EditorFoldArrow(3)"),
        "EditorFoldArrow(3) missing: {initial}"
    );
    click_ui(&mut session, "EditorFoldArrow(3)");
    let folded = dump(&mut session);
    assert!(
        !has_ui(&folded, "EditorFoldArrow(4)"),
        "nested fold arrow remains: {folded}"
    );
    click_ui(&mut session, "EditorFoldArrow(3)");
    assert!(has_ui(&dump(&mut session), "EditorFoldArrow(4)"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_minimap_and_long_scroll_record_converge() {
    let dir = scratch_dir("ui-editor-scroll");
    let path = long_file(&dir);
    for scale in [1.0, 1.5] {
        let mut session = open_file_session(900, 600, scale, &path);
        let before = dump(&mut session)["tabs"][0]["scroll_y"].as_f64().unwrap();
        click_ui(&mut session, "EditorMinimap");
        let moved = dump(&mut session)["tabs"][0]["scroll_y"].as_f64().unwrap();
        assert_ne!(before, moved);
        let rec = dir.join(format!("record-{scale}"));
        let lines = run_script(&mut session, format!("record 6 {} wheel 0 -3\nkey pagedown\nsettle 2000\n", rec.display()).as_bytes());
        let summary = ok_json(&lines[0]);
        assert_eq!(summary["motion"]["nonmonotonic_frames"], 0, "{summary}");
        assert!(dump(&mut session)["tabs"][0]["scroll_y"].as_f64().is_some());
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_search_overlay_and_completion() {
    let dir = scratch_dir("ui-editor-overlays");
    let file = dir.join("search.txt");
    std::fs::write(&file, "needle\nneedle two\n").unwrap();
    for scale in [1.0, 1.5] {
        let mut session = open_file_session(640, 400, scale, &file);
        run_script(&mut session, b"key ctrl+f\ntype needle\n");
        assert_eq!(dump(&mut session)["overlays"]["search"], true);
        run_script(&mut session, b"key enter\ntype this-query-does-not-exist\nkey escape\n");
        assert_eq!(dump(&mut session)["overlays"]["search"], false);
    }
    let completion_file = dir.join("completion.rs");
    std::fs::write(&completion_file, "fn primary() {}\nfn example() {\n    pri").unwrap();
    let mut session = open_file_session(640, 400, 1.0, &completion_file);
    run_script(&mut session, b"key ctrl+end\nkey ctrl+space\nsettle 1000\n");
    assert!(session.app.autocomplete_active, "completion should be active after Ctrl+Space");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_search_match_advances_and_escape_closes_at_both_scales() {
    let dir = scratch_dir("ui-editor-search-next");
    let file = dir.join("matches.txt");
    std::fs::write(&file, "match one\nother\nmatch two\nmatch three\n").unwrap();
    for scale in [1.0, 1.5] {
        let mut session = open_file_session(640, 400, scale, &file);
        run_script(&mut session, b"key ctrl+f\ntype match\n");
        assert_eq!(dump(&mut session)["overlays"]["search"], true);
        let before = dump(&mut session)["tabs"][0]["cursor"]["line"].as_i64().unwrap();
        run_script(&mut session, b"key enter\n");
        let after = dump(&mut session)["tabs"][0]["cursor"]["line"].as_i64().unwrap();
        assert_ne!(before, after);
        run_script(&mut session, b"key ctrl+a\ntype a-query-longer-than-any-line-in-this-fixture\nkey escape\n");
        assert_eq!(dump(&mut session)["overlays"]["search"], false);
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_settings_overlay_closes_on_outside_click() {
    let dir = scratch_dir("ui-editor-settings-outside");
    let file = sample_file(&dir);
    for scale in [1.0, 1.5] {
        let mut session = open_file_session(640, 400, scale, &file);
        run_script(&mut session, b"key f1\n");
        assert_eq!(dump(&mut session)["overlays"]["settings"], true);
        run_script(&mut session, b"mouse_move 2 2\nclick\n");
        assert_eq!(dump(&mut session)["overlays"]["settings"], false);
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_drag_selection_does_not_require_double_click() {
    let dir = scratch_dir("ui-editor-drag-selection");
    let file = dir.join("drag.txt");
    std::fs::write(&file, "first words on this line\nsecond line\n").unwrap();
    let mut session = open_file_session(900, 600, 1.0, &file);
    let state = dump(&mut session);
    let body = state["ui"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "EditorTextBody")
        .unwrap();
    let rect = body["rect"].as_array().unwrap();
    let x = rect[0].as_f64().unwrap() + 20.0;
    let y = rect[1].as_f64().unwrap() + 18.0;
    run_script(
        &mut session,
        format!("mouse_move {x} {y}\nclick down\nmouse_move {} {}\nclick up\n", x + 100.0, y + 24.0)
            .as_bytes(),
    );
    let selected = dump(&mut session);
    assert!(
        !selected["editor"]["selection"].is_null(),
        "drag should select a range: {selected}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_record_wheel_and_page_down_settle_without_reverse_frames() {
    let dir = scratch_dir("ui-editor-wheel-pagedown");
    let path = long_file(&dir);
    for scale in [1.0, 1.5] {
        let mut session = open_file_session(1280, 800, scale, &path);
        let out = dir.join(format!("pagedown-{scale}"));
        let lines = run_script(&mut session, format!("record 8 {} wheel 0 -3\nkey pagedown\nsettle 2000\n", out.display()).as_bytes());
        let summary = ok_json(&lines[0]);
        assert_eq!(summary["motion"]["nonmonotonic_frames"], 0, "{summary}");
        assert!(lines.last().unwrap().ends_with("settled=true"), "{lines:?}");
        assert!(dump(&mut session)["tabs"][0]["scroll_y"].as_f64().unwrap() > 0.0);
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_bug_fold_arrow_top_visible() {
    let dir = scratch_dir("ui-bug-fold-top");
    let file = dir.join("top.rs");
    let mut source = String::from("fn outer() {\n    fn inner() {}\n");
    for line in 0..350 {
        source.push_str(&format!("    let _line_{line} = {line};\n"));
    }
    source.push_str("}\n");
    std::fs::write(&file, source).unwrap();
    let mut session = open_file_session(1920, 1080, 1.0, &file);
    run_script(&mut session, b"settle 1500\n");
    assert!(!has_ui(&dump(&mut session), "StickyLine(0, 0)"));
    run_script(&mut session, b"mouse_move 53 10\nclick\nsettle 1000\n");
    assert!(!has_ui(&dump(&mut session), "EditorFoldArrow(1)"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_bug_editor_text_body_minimum_width() {
    let dir = scratch_dir("ui-bug-editor-width");
    let file = sample_file(&dir);
    let mut session = open_file_session(400, 300, 2.0, &file);
    let state = dump(&mut session);
    let rect = state["ui"].as_array().unwrap().iter().find(|e| e["id"] == "EditorTextBody").unwrap()["rect"].clone();
    assert!(rect[2].as_f64().unwrap() >= 150.0, "{rect}");
    assert!(!has_ui(&state, "EditorMinimap"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_bug_long_editor_tab_visible_close() {
    let dir = scratch_dir("ui-bug-long-tab");
    let file = dir.join(format!("{}.txt", "x".repeat(120)));
    std::fs::write(&file, "x\n").unwrap();
    let mut session = session_for_test(1280, 800);
    let lines = run_script(
        &mut session,
        format!(
            "scale 1.5\nworkspace {}\nopen {}\nsettle 2000\n",
            dir.display(),
            file.display()
        )
        .as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    let state = dump(&mut session);
    assert!(has_ui(&state, "EditorTab(0)"));
    assert!(has_ui(&state, "EditorTabClose(0)"));
    let _ = std::fs::remove_dir_all(dir);
}

fn open_writable_file(
    w: u32,
    h: u32,
    scale: f32,
    path: &std::path::Path,
) -> crate::headless::HeadlessSession {
    let options = crate::headless::profile::HeadlessOptions {
        size: (w, h),
        allow_writes: true,
        ..Default::default()
    };
    let root = crate::headless::tests_support::ensure_test_profile_root();
    let mut session = match crate::headless::HeadlessSession::new(&options, root) {
        Ok(session) => session,
        Err((code, message)) => panic!("headless session (code {code}): {message}"),
    };
    session.hz_probe = || None;
    let lines = run_script(
        &mut session,
        format!("scale {scale}\nopen {}\nsettle 2000\n", path.display()).as_bytes(),
    );
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    session
}

#[test]
fn headless_editor_save_undo_redo_shortcuts() {
    let dir = scratch_dir("ui-editor-shortcuts-save-history");
    let file = dir.join("history.txt");
    let original = "before\n";
    std::fs::write(&file, original).unwrap();
    let mut session = open_writable_file(1280, 720, 4.0 / 3.0, &file);
    assert_eq!(session.app.editor.get_full_text(), original);
    assert!(!dump(&mut session)["tabs"][0]["modified"].as_bool().unwrap());

    run_script(&mut session, b"key ctrl+z\n");
    let empty_undo = dump(&mut session);
    assert_eq!(session.app.editor.get_full_text(), original);
    assert!(!empty_undo["tabs"][0]["modified"].as_bool().unwrap());

    run_script(&mut session, b"type x\n");
    let edited = "xbefore\n";
    assert_eq!(session.app.editor.get_full_text(), edited);
    assert!(dump(&mut session)["tabs"][0]["modified"].as_bool().unwrap());
    run_script(&mut session, b"key ctrl+z\n");
    let undone = dump(&mut session);
    assert_eq!(session.app.editor.get_full_text(), original);
    assert!(!undone["tabs"][0]["modified"].as_bool().unwrap());

    run_script(&mut session, b"key ctrl+y\n");
    let redone = dump(&mut session);
    assert_eq!(session.app.editor.get_full_text(), edited);
    assert!(redone["tabs"][0]["modified"].as_bool().unwrap());
    run_script(&mut session, b"key ctrl+s\n");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), edited);
    assert!(!dump(&mut session)["tabs"][0]["modified"].as_bool().unwrap());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_copy_paste_round_trip() {
    let dir = scratch_dir("ui-editor-shortcuts-clipboard");
    let file = dir.join("clipboard.txt");
    let original = "selected text\n";
    std::fs::write(&file, original).unwrap();
    let mut session = open_file_session(1280, 720, 4.0 / 3.0, &file);
    assert_eq!(dump(&mut session)["clipboard"], serde_json::json!({"mode": "memory", "text": null}));

    run_script(&mut session, b"key ctrl+a\nkey ctrl+c\n");
    let copied = dump(&mut session);
    assert_eq!(session.app.editor.get_full_text(), original);
    assert!(!copied["editor"]["selection"].is_null());
    assert!(!copied["tabs"][0]["modified"].as_bool().unwrap());
    assert_eq!(copied["clipboard"]["text"], original);

    run_script(&mut session, b"key right\nkey ctrl+v\n");
    assert_eq!(session.app.editor.get_full_text(), format!("{original}{original}"));
    let pasted = dump(&mut session);
    assert!(pasted["editor"]["selection"].is_null());
    assert_eq!(pasted["clipboard"]["text"], original);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_cut_paste_round_trip() {
    let dir = scratch_dir("ui-editor-cut-paste");
    let file = dir.join("cut.txt");
    let original = "cut text\n";
    std::fs::write(&file, original).unwrap();
    let mut session = open_file_session(1280, 720, 4.0 / 3.0, &file);

    run_script(&mut session, b"key ctrl+a\nkey ctrl+x\n");
    let cut = dump(&mut session);
    assert_eq!(session.app.editor.get_full_text(), "");
    assert!(cut["tabs"][0]["modified"].as_bool().unwrap());
    assert_eq!(cut["clipboard"]["text"], original);
    run_script(&mut session, b"key ctrl+v\n");
    assert_eq!(session.app.editor.get_full_text(), original);
    assert_eq!(dump(&mut session)["clipboard"]["text"], original);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_paste_with_empty_clipboard_does_nothing() {
    let dir = scratch_dir("ui-editor-empty-clipboard");
    let file = dir.join("empty.txt");
    let original = "keep selected\n";
    std::fs::write(&file, original).unwrap();
    let mut session = open_file_session(1280, 720, 4.0 / 3.0, &file);

    run_script(&mut session, b"key ctrl+a\n");
    let before = dump(&mut session);
    assert_eq!(before["clipboard"], serde_json::json!({"mode": "memory", "text": null}));
    run_script(&mut session, b"key ctrl+v\n");
    let after = dump(&mut session);
    assert_eq!(session.app.editor.get_full_text(), original);
    assert_eq!(after["editor"]["selection"], before["editor"]["selection"]);
    assert_eq!(after["tabs"][0]["cursor"], before["tabs"][0]["cursor"]);
    assert_eq!(after["tabs"][0]["modified"], before["tabs"][0]["modified"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_multiline_copy_paste_round_trip() {
    let dir = scratch_dir("ui-editor-multiline-clipboard");
    let file = dir.join("multiline.txt");
    let original = "first line\nsecond line\nthird line\n";
    std::fs::write(&file, original).unwrap();
    let mut session = open_file_session(1280, 720, 4.0 / 3.0, &file);

    run_script(&mut session, b"key ctrl+a\nkey ctrl+c\nkey right\nkey ctrl+v\n");
    assert_eq!(session.app.editor.get_full_text(), format!("{original}{original}"));
    let state = dump(&mut session);
    assert_eq!(state["clipboard"]["text"], original);
    assert!(state["editor"]["selection"].is_null());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_editor_comment_toggle_single_and_multiple_lines() {
    let dir = scratch_dir("ui-editor-shortcuts-comments");
    let file = dir.join("comments.rs");
    let original = "\nfn main() {\n    let a = 1;\n    let b = 2;\n}\n";
    std::fs::write(&file, original).unwrap();
    let mut session = open_file_session(1280, 720, 4.0 / 3.0, &file);
    run_script(&mut session, b"key ctrl+home\nkey ctrl+/\n");
    let empty_line = dump(&mut session);
    assert_eq!(session.app.editor.get_full_text(), original);
    assert!(!empty_line["tabs"][0]["modified"].as_bool().unwrap());

    run_script(&mut session, b"key down\nkey shift+end\nkey ctrl+/\n");
    let single_line = "\n//fn main() {\n    let a = 1;\n    let b = 2;\n}\n";
    assert_eq!(session.app.editor.get_full_text(), single_line);
    assert!(dump(&mut session)["tabs"][0]["modified"].as_bool().unwrap());
    run_script(&mut session, b"key ctrl+/\n");
    assert_eq!(session.app.editor.get_full_text(), original);

    run_script(&mut session, b"key ctrl+a\nkey ctrl+/\n");
    let multiple_lines = "\n//fn main() {\n    //let a = 1;\n    //let b = 2;\n//}\n";
    assert_eq!(session.app.editor.get_full_text(), multiple_lines);
    assert!(dump(&mut session)["tabs"][0]["modified"].as_bool().unwrap());
    run_script(&mut session, b"key ctrl+/\n");
    let uncommented = dump(&mut session);
    assert_eq!(session.app.editor.get_full_text(), original);
    assert!(!uncommented["tabs"][0]["modified"].as_bool().unwrap());
    let _ = std::fs::remove_dir_all(dir);
}
