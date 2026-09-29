use super::*;
use crate::editor::Editor;

fn python_block_document() -> String {
    let mut text = String::from("import os\n\n");
    for i in 0..300 {
        text.push_str(&format!("def f_{i}(x):\n    # λ note {i}\n    return x + {i}\n\n"));
    }
    text.push_str("class Tail:\n    # value = 1\n    # other = 'ü'\n    pass");
    text
}

fn parse_python(text: &str, old_tree: Option<&tree_sitter::Tree>) -> tree_sitter::Tree {
    let (lang, _) = get_ts_config("py").expect("python config");
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&lang).expect("python language");
    parser.parse(text, old_tree).expect("python parse")
}

fn editor_with(text: &str) -> Editor {
    let mut editor = Editor::new(text.len() + 64);
    editor.set_clean_text(text);
    editor
}

/// Replays what the app does after one editor action: the `SyncEdit`s the editor produced
/// are applied, in order, to a replica (text + incrementally edited tree) of the text before
/// the action. The replica must equal the editor text, and reparsing with the edited tree
/// must give a tree spanning exactly that text.
fn assert_replica_follows(editor: &mut Editor, what: &str, action: impl FnOnce(&mut Editor)) {
    editor.sync_edits.clear();
    let mut replica = editor.get_full_text();
    let mut tree = Some(parse_python(&replica, None));
    action(editor);
    for edit in &editor.sync_edits {
        assert!(
            apply_sync_edit_to_replica(&mut replica, tree.as_mut(), edit),
            "{what}: edit {edit:?} does not fit the replica"
        );
    }
    assert_eq!(replica, editor.get_full_text(), "{what}: replica drifted");
    let reparsed = parse_python(&replica, tree.as_ref());
    assert_eq!(reparsed.root_node().end_byte(), replica.len(), "{what}: tree length");
    editor.sync_edits.clear();
}

fn select_lines(editor: &mut Editor, first: usize, last_exclusive: usize) {
    editor.selection_anchor = Some(editor.line_offsets[first]);
    editor.cursor = editor.line_offsets[last_exclusive];
}

#[test]
fn replica_follows_comment_toggle_and_its_undo_redo() {
    for document in [python_block_document(), python_block_document().replace('\n', "\r\n")] {
        let mut editor = editor_with(&document);
        let block = document.find("def f_150").map_or(0, |offset| {
            document[..offset].bytes().filter(|&b| b == b'\n').count()
        });
        select_lines(&mut editor, block, block + 3);
        assert_replica_follows(&mut editor, "comment", |e| assert!(e.toggle_line_comment("# ")));
        assert_replica_follows(&mut editor, "uncomment", |e| assert!(e.toggle_line_comment("# ")));
        assert_replica_follows(&mut editor, "undo uncomment", |e| assert!(e.undo().is_some()));
        assert_replica_follows(&mut editor, "undo comment", |e| assert!(e.undo().is_some()));
        assert_replica_follows(&mut editor, "redo comment", |e| assert!(e.redo().is_some()));
        assert_replica_follows(&mut editor, "redo uncomment", |e| assert!(e.redo().is_some()));
        // Uncommenting the two commented tail lines, then commenting up to the text end
        // (last line without a newline).
        let lines = editor.line_offsets.len();
        select_lines(&mut editor, lines - 3, lines - 1);
        assert_replica_follows(&mut editor, "uncomment tail", |e| assert!(e.toggle_line_comment("# ")));
        editor.selection_anchor = Some(editor.line_offsets[lines - 3]);
        editor.cursor = editor.len();
        assert_replica_follows(&mut editor, "comment to end", |e| assert!(e.toggle_line_comment("# ")));
        assert_replica_follows(&mut editor, "undo end", |e| assert!(e.undo().is_some()));
        assert_replica_follows(&mut editor, "undo tail", |e| assert!(e.undo().is_some()));
    }
}

#[test]
fn replica_follows_tab_paste_auto_pairs_and_their_undo_redo() {
    let document = python_block_document();
    let mut editor = editor_with(&document);
    select_lines(&mut editor, 20, 26);
    assert_replica_follows(&mut editor, "tab over selection", |e| {
        let _ = e.insert_str("    ");
    });
    select_lines(&mut editor, 40, 43);
    assert_replica_follows(&mut editor, "paste over selection", |e| {
        let _ = e.insert_str("pasted = 'λ'\nsecond()\n");
    });
    editor.cursor = editor.line_offsets[60] + 4;
    editor.selection_anchor = None;
    assert_replica_follows(&mut editor, "auto pair", |e| {
        let _ = e.insert_str("()");
        e.cursor -= 1;
    });
    assert_replica_follows(&mut editor, "pair backspace", |e| assert!(e.backspace().is_some()));
    for step in 0..4 {
        assert_replica_follows(&mut editor, &format!("undo {step}"), |e| {
            assert!(e.undo().is_some())
        });
    }
    for step in 0..4 {
        assert_replica_follows(&mut editor, &format!("redo {step}"), |e| {
            assert!(e.redo().is_some())
        });
    }
}

#[test]
fn replica_applies_back_to_front_multi_edit_lists() {
    // One multi-cursor action: markers inserted/removed on several lines, back to front,
    // each offset valid for the text left by the previous edits.
    let text = "a = 1\nb = 2\nc = 3\n";
    let comment = [12, 6, 0].map(|offset| SyncEdit::Insert { offset, text: "# ".to_string() });
    let uncomment = [16, 8, 0].map(|offset| SyncEdit::Delete { offset, len: 2 });
    let mut replica = text.to_string();
    let mut tree = Some(parse_python(&replica, None));
    for edit in &comment {
        assert!(apply_sync_edit_to_replica(&mut replica, tree.as_mut(), edit));
    }
    assert_eq!(replica, "# a = 1\n# b = 2\n# c = 3\n");
    assert_eq!(parse_python(&replica, tree.as_ref()).root_node().end_byte(), replica.len());
    for edit in &uncomment {
        assert!(apply_sync_edit_to_replica(&mut replica, tree.as_mut(), edit));
    }
    assert_eq!(replica, text);
    assert_eq!(parse_python(&replica, tree.as_ref()).root_node().end_byte(), text.len());
}

#[test]
fn replica_rejects_edits_that_do_not_fit_and_stays_untouched() {
    let mut replica = "λx\n".to_string();
    for edit in [
        SyncEdit::Insert { offset: 1, text: "y".to_string() },
        SyncEdit::Insert { offset: 9, text: "y".to_string() },
        SyncEdit::Delete { offset: 2, len: 5 },
        SyncEdit::Delete { offset: usize::MAX, len: 2 },
    ] {
        assert!(!apply_sync_edit_to_replica(&mut replica, None, &edit), "{edit:?}");
    }
    assert_eq!(replica, "λx\n");
}

fn wait_for_version(highlighter: &mut Highlighter, version: u64) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while std::time::Instant::now() < deadline {
        highlighter.poll(version);
        if highlighter.current_version == version && highlighter.is_complete {
            return;
        }
        std::thread::yield_now();
    }
    panic!("highlighter did not produce version {version}");
}

/// The drift seen in the field: the highlighter is re-seeded with the full editor text
/// (`reset_highlighter_with_text`, e.g. `reprioritize_highlighter_around_cursor` or a
/// formatter) while the editor still holds the `SyncEdit`s of that same text. The next
/// drain sends them again, so the worker replica applied the comment toggle twice and
/// its tree no longer matched `sync_text` (maintained by `shift_*`).
#[test]
fn replicas_do_not_drift_when_reset_overtakes_pending_sync_edits() {
    let document = python_block_document();
    let mut editor = editor_with(&document);
    let mut highlighter = Highlighter::new();
    highlighter.reset(editor.version, document.clone(), "py".to_string(), 0);
    wait_for_version(&mut highlighter, editor.version);

    let lines = editor.line_offsets.len();
    select_lines(&mut editor, lines - 3, lines - 1);
    assert!(editor.toggle_line_comment("# "));
    assert!(!editor.get_full_text().contains("# value"));
    highlighter.reset(editor.version, editor.get_full_text(), "py".to_string(), 0);

    editor.cursor = 0;
    editor.selection_anchor = None;
    let _ = editor.insert_str("x");
    highlighter.shift_insert(0, 1, Some("x"));
    let edits = std::mem::take(&mut editor.sync_edits);
    highlighter.apply_document_edits(
        editor.version,
        edits,
        None,
        None,
        editor.len(),
        || editor.get_full_text(),
    );
    wait_for_version(&mut highlighter, editor.version);

    assert_eq!(highlighter.sync_text, editor.get_full_text());
    let tree = highlighter.sync_tree.as_ref().expect("worker tree of the current version");
    assert_eq!(tree.root_node().end_byte(), editor.len());
}

/// A worker reply is paired with `sync_text` by version, not by a length coincidence: a
/// tree of the same length but of another version is not taken.
#[test]
fn poll_rejects_worker_tree_of_other_replica_version() {
    let mut highlighter = Highlighter::new();
    highlighter.current_request_id = 1;
    highlighter.sync_ext = "py".to_string();
    highlighter.sync_text = "a = 1\n".to_string();
    highlighter.sync_version = 7;
    let same_len_other_text = parse_python("b = 2\n", None);
    assert!(highlighter.apply_poll_result(
        1,
        8,
        8,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Some(same_len_other_text),
        true,
    ));
    assert!(highlighter.sync_tree.is_none());
}
