#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_edit_history_dirty_state_end_to_end() {
        let mut editor = Editor::new(4);
        editor.set_original_text();

        editor.insert_str("hello");
        assert_eq!(editor.get_full_text(), "hello");
        assert!(editor.is_dirty());

        editor.mark_saved();
        assert!(!editor.is_dirty());

        editor.replace_range(0, 5, "hey");
        assert_eq!(editor.get_full_text(), "hey");
        assert!(editor.is_dirty());

        editor.undo();
        assert_eq!(editor.get_full_text(), "hello");
        assert!(!editor.is_dirty());

        editor.redo();
        assert_eq!(editor.get_full_text(), "hey");
        assert!(editor.is_dirty());
    }

    #[test]
    fn editor_navigation_selection_indent_and_utf8_end_to_end() {
        let mut editor = Editor::new(16);
        editor.insert_str("def main:\n    привет\n");
        let text = editor.get_full_text();
        editor.cursor = text.find("привет").unwrap() + "при".len();

        editor.select_word();
        assert_eq!(editor.get_selection().as_deref(), Some("привет"));

        editor.cursor = text.find("def main:").unwrap() + "def main:".len();
        assert_eq!(editor.get_auto_indent(), "    ");

        editor.move_end_of_file(false);
        editor.move_word_left(false);
        assert_eq!(editor.cursor, text.find("привет").unwrap());
    }

    #[test]
    fn editor_fold_visibility_and_offsets_end_to_end() {
        let mut editor = Editor::new(64);
        editor.insert_str("fn main() {\n    call();\n}\nlast\n");
        editor.foldable_ranges_bytes.push((0, 24, false));
        editor.folded_start_bytes.insert(0);
        editor.rebuild_line_offsets();

        assert_eq!(editor.line_offsets, vec![0, 12, 24, 26, 31]);
        assert_eq!(editor.get_visible_lines_count(), 3);

        editor.cursor = editor.line_offsets[1];
        editor.snap_cursor_out_of_fold(0);
        assert_eq!(editor.cursor, editor.line_offsets[3]);
    }

    #[test]
    fn editor_navigation_expand_home_end_folds_and_utf16_edges() {
        let mut editor = Editor::new(128);
        editor.insert_str("let value = call(\"hello\", [one, two]);\nnext 😀 line\n");
        let text = editor.get_full_text();

        editor.cursor = text.find("value").unwrap() + 2;
        editor.select_expand();
        assert_eq!(editor.get_selection().as_deref(), Some("value"));
        editor.select_expand();
        assert_eq!(
            editor.get_selection().as_deref(),
            Some("let value = call(\"hello\", [one, two]);")
        );

        editor.cursor = text.find("hello").unwrap() + 1;
        editor.selection_anchor = None;
        editor.select_expand();
        assert_eq!(editor.get_selection().as_deref(), Some("hello"));
        editor.select_expand();
        assert_eq!(editor.get_selection().as_deref(), Some("\"hello\""));

        editor.cursor = text.find("one").unwrap();
        editor.selection_anchor = Some(text.find("two").unwrap() + 3);
        editor.select_expand();
        assert_eq!(editor.get_selection().as_deref(), Some("[one, two]"));

        editor.cursor = text.find("next").unwrap() + 2;
        editor.selection_anchor = None;
        editor.move_home(true);
        assert_eq!(
            editor.selection_anchor,
            Some(text.find("next").unwrap() + 2)
        );
        assert_eq!(editor.cursor, text.find("next").unwrap());
        editor.move_end(false);
        assert!(editor.selection_anchor.is_none());
        assert_eq!(editor.cursor, text.trim_end().len());

        let mut seen = Vec::new();
        editor.utf16_col_to_byte_advance(1, |ch, col, byte| seen.push((ch, col, byte)));
        let emoji = seen.iter().find(|(ch, _, _)| *ch == '😀').unwrap();
        assert_eq!(emoji.1, 5);
        assert_eq!(seen.iter().find(|(ch, _, _)| *ch == 'l').unwrap().1, 8);

        editor.foldable_lines.insert(0, 1);
        editor.folded_lines.insert(0);
        editor.cursor = editor.line_offsets[1] + 4;
        editor.snap_cursor_out_of_fold(editor.len());
        assert_eq!(editor.cursor, editor.line_offsets[2].saturating_sub(1));

        editor.cursor = editor.line_offsets[1] + 4;
        editor.snap_cursor_out_of_fold(0);
        assert_eq!(editor.cursor, editor.line_offsets[2]);
    }

    #[test]
    fn editor_delete_paths_pair_unicode_words_and_selection_replacement() {
        let mut pair = Editor::new(4);
        pair.insert_str("([])");
        pair.cursor = 2;
        assert_eq!(pair.backspace(), Some((1, 2)));
        assert_eq!(pair.get_full_text(), "()");
        assert!(matches!(pair.undo(), Some(UndoRedoDelta::Insert(1, 2, text)) if text == "[]"));
        assert_eq!(pair.get_full_text(), "([])");
        assert!(matches!(pair.redo(), Some(UndoRedoDelta::Delete(1, 2))));
        assert_eq!(pair.get_full_text(), "()");

        let mut ctrl_pair = Editor::new(4);
        ctrl_pair.insert_str("([])");
        ctrl_pair.cursor = 2;
        assert_eq!(ctrl_pair.delete_word_backward(), Some((1, 2)));
        assert_eq!(ctrl_pair.get_full_text(), "()");

        let mut unicode = Editor::new(4);
        unicode.insert_str("a😀b");
        unicode.cursor = "a😀".len();
        assert_eq!(unicode.backspace(), Some((1, "😀".len())));
        assert_eq!(unicode.get_full_text(), "ab");
        unicode.cursor = 1;
        assert_eq!(unicode.delete_forward(), Some((1, 1)));
        assert_eq!(unicode.get_full_text(), "a");

        let mut words = Editor::new(32);
        words.insert_str("foo  bar.baz");
        words.cursor = "foo  bar".len();
        assert_eq!(words.delete_word_backward(), Some((5, 3)));
        assert_eq!(words.get_full_text(), "foo  .baz");
        words.cursor = 3;
        assert_eq!(words.delete_word_forward(), Some((3, 2)));
        assert_eq!(words.get_full_text(), "foo.baz");
        assert_eq!(words.delete_forward(), Some((3, 1)));
        assert_eq!(words.get_full_text(), "foobaz");

        words.selection_anchor = Some(0);
        words.cursor = 3;
        let (deleted, inserted) = words.insert_str("BAR");
        assert_eq!(deleted, Some((0, 3)));
        assert_eq!(inserted, 3);
        assert_eq!(words.get_full_text(), "BARbaz");
    }

    #[test]
    fn editor_indent_cache_line_states_and_fold_shift_edges() {
        let mut editor = Editor::new(64);
        editor.insert_str("root\n    child\n\n\tgrand\n");
        editor.ensure_indent_cache_updated();
        assert_eq!(editor.get_cached_indent_levels(), &[0, 1, 1, 1, 0]);

        let cached = editor.get_cached_indent_levels().as_ptr();
        editor.ensure_indent_cache_updated();
        assert_eq!(editor.get_cached_indent_levels().as_ptr(), cached);

        editor.set_original_text();
        let child_start = editor.get_full_text().find("child").unwrap();
        editor.replace_range(child_start, child_start + "child".len(), "kid");
        assert!(matches!(
            editor.get_line_modification_state(1),
            Some(LineModState::ModifiedUnsaved)
        ));
        assert!(editor.is_dirty());

        editor.mark_saved();
        assert!(matches!(
            editor.get_line_modification_state(1),
            Some(LineModState::ModifiedSaved)
        ));
        assert!(!editor.is_dirty());

        let mut folds = Editor::new(64);
        folds.insert_str("aaa\nbbb\nccc\n");
        folds.foldable_ranges_bytes.push((0, 8, false));
        folds.folded_start_bytes.insert(4);

        folds.shift_folds_insert(8, 2);
        assert_eq!(folds.foldable_ranges_bytes, vec![(0, 8, false)]);
        assert!(folds.folded_start_bytes.contains(&4));

        folds.shift_folds_insert(4, 2);
        assert_eq!(folds.foldable_ranges_bytes, vec![(0, 10, false)]);
        assert!(folds.folded_start_bytes.contains(&6));

        folds.shift_folds_delete(0, 2);
        assert_eq!(folds.foldable_ranges_bytes, vec![(0, 8, false)]);
        assert!(folds.folded_start_bytes.contains(&4));
    }

    #[test]
    fn editor_clear_history_empty_ops_and_navigation_edges_are_stable() {
        let mut editor = Editor::new(8);
        assert_eq!(editor.backspace(), None);
        assert_eq!(editor.delete_forward(), None);
        assert_eq!(editor.delete_word_backward(), None);
        assert_eq!(editor.delete_word_forward(), None);
        assert_eq!(editor.insert_str(""), (None, 0));
        assert_eq!(editor.get_visible_lines_count(), 1);

        editor.insert_str("alpha beta\nlast");
        editor.clear_history();
        assert!(editor.history.is_empty());
        assert!(editor.redo_stack.is_empty());
        assert_eq!(editor.history_size, 0);
        assert!(editor.sync_edits.is_empty());

        editor.select_all();
        assert_eq!(editor.get_selection().as_deref(), Some("alpha beta\nlast"));
        editor.move_start_of_file(false);
        assert!(editor.get_selection().is_none());
        editor.move_end_of_file(true);
        assert_eq!(editor.get_selection().as_deref(), Some("alpha beta\nlast"));

        editor.cursor = 0;
        editor.selection_anchor = None;
        editor.move_word_right(false);
        assert_eq!(editor.cursor, "alpha".len());
        editor.move_word_right(false);
        assert_eq!(editor.cursor, "alpha beta".len());
        editor.move_word_left(false);
        assert_eq!(editor.cursor, "alpha ".len());
        editor.move_left(false);
        assert_eq!(editor.cursor, "alpha".len());
        editor.move_right(false);
        assert_eq!(editor.cursor, "alpha ".len());
    }

    #[test]
    fn editor_set_clean_text_loads_without_dirty_history_or_sync_edits() {
        let mut editor = Editor::new(8);
        editor.insert_str("dirty");
        editor.set_clean_text("alpha\nbeta\n");

        assert_eq!(editor.get_full_text(), "alpha\nbeta\n");
        assert_eq!(editor.cursor, 0);
        assert!(!editor.is_dirty());
        assert!(editor.history.is_empty());
        assert!(editor.redo_stack.is_empty());
        assert_eq!(editor.history_size, 0);
        assert!(editor.sync_edits.is_empty());
        assert_eq!(editor.line_offsets, vec![0, 6, 11]);
        assert_eq!(editor.line_states.len(), 3);
        assert!(editor.line_states.iter().all(Option::is_none));
        assert_eq!(editor.deleted_gaps.len(), 4);
        assert!(editor.deleted_gaps.iter().all(Option::is_none));
    }

    #[test]
    fn editor_navigation_extra_boundaries_selection_and_fold_edges() {
        let mut editor = Editor::new(128);
        editor.insert_str("  alpha\ncall({one: [two]})\n\"qq\"\n");
        let text = editor.get_full_text();

        editor.cursor = text.find("alpha").unwrap() + 2;
        editor.select_line();
        assert_eq!(editor.get_selection().as_deref(), Some("  alpha"));

        editor.selection_anchor = None;
        editor.cursor = text.find("one").unwrap();
        editor.select_expand();
        assert_eq!(editor.get_selection().as_deref(), Some("one"));
        editor.select_expand();
        assert_eq!(editor.get_selection().as_deref(), Some("one: [two]"));
        editor.select_expand();
        assert_eq!(editor.get_selection().as_deref(), Some("{one: [two]}"));

        editor.selection_anchor = None;
        editor.cursor = text.find("qq").unwrap();
        editor.select_expand();
        assert_eq!(editor.get_selection().as_deref(), Some("qq"));
        editor.select_expand();
        assert_eq!(editor.get_selection().as_deref(), Some("\"qq\""));

        editor.select_all();
        assert_eq!(editor.get_selection().as_deref(), Some(text.as_str()));
        editor.selection_anchor = Some(editor.cursor);
        assert!(editor.get_selection().is_none());

        assert!(editor.is_char_boundary(0));
        assert!(editor.is_char_boundary(editor.len()));
        let emoji_start = editor.len();
        editor.move_end_of_file(false);
        editor.insert_str("é😀");
        assert!(!editor.is_char_boundary(emoji_start + 1));
        assert!(!editor.is_char_boundary(emoji_start + "é".len() + 1));

        let mut seen = Vec::new();
        editor.utf16_col_to_byte_advance(99, |ch, col, byte| seen.push((ch, col, byte)));
        assert_eq!(seen.first().copied(), Some((' ', 0, 0)));

        let mut short_utf8 = Editor::new(8);
        short_utf8.data = vec![b'a', 0xD1];
        short_utf8.gap_start = 2;
        short_utf8.gap_end = 2;
        short_utf8.line_offsets = vec![0];
        let mut short_seen = Vec::new();
        short_utf8.utf16_col_to_byte_advance(0, |ch, col, byte| {
            short_seen.push((ch, col, byte));
        });
        assert_eq!(short_seen, vec![('a', 0, 0)]);

        let mut folds = Editor::new(64);
        folds.insert_str("head\nchild\nlast");
        folds.foldable_lines.insert(0, 1);
        folds.folded_lines.insert(0);
        folds.cursor = folds.line_offsets[1] + 2;
        folds.move_home(false);
        assert_eq!(folds.cursor, 0);
        folds.cursor = 0;
        folds.move_end(false);
        assert_eq!(folds.cursor, folds.line_offsets[2].saturating_sub(1));
    }

    #[test]
    fn editor_navigation_extra_word_and_file_movement_edges() {
        let mut editor = Editor::new(64);
        editor.insert_str("one  two\né😀z\nlast");
        let text = editor.get_full_text();

        editor.cursor = 0;
        editor.move_left(false);
        assert_eq!(editor.cursor, 0);
        editor.move_word_left(false);
        assert_eq!(editor.cursor, 0);

        editor.cursor = editor.len();
        editor.move_right(false);
        assert_eq!(editor.cursor, editor.len());
        editor.move_word_right(false);
        assert_eq!(editor.cursor, editor.len());

        editor.cursor = text.find("two").unwrap();
        editor.move_word_right(true);
        assert_eq!(editor.selection_anchor, Some(text.find("two").unwrap()));
        assert_eq!(editor.get_selection().as_deref(), Some("two"));

        let unicode = text.find("é").unwrap();
        editor.selection_anchor = None;
        editor.cursor = unicode;
        editor.move_right(false);
        assert_eq!(editor.cursor, unicode + "é".len());
        editor.move_right(false);
        assert_eq!(editor.cursor, unicode + "é😀".len());
        editor.move_left(false);
        assert_eq!(editor.cursor, unicode + "é".len());

        editor.move_start_of_file(true);
        assert_eq!(editor.selection_anchor, Some(unicode + "é".len()));
        assert_eq!(editor.cursor, 0);
        editor.move_end_of_file(false);
        assert!(editor.selection_anchor.is_none());
        assert_eq!(editor.cursor, editor.len());
    }
}
