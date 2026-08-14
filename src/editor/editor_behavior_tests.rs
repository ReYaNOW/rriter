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
    fn replace_undo_redo_shifts_fold_offsets_once() {
        let mut editor = Editor::new(64);
        editor.insert_str("aaaa\nbbbb\ncccc\ndddd\n");
        editor.clear_history();
        editor.foldable_ranges_bytes.push((10, 20, false));
        editor.folded_start_bytes.insert(10);

        editor.replace_range(0, 2, "x");
        assert_eq!(editor.foldable_ranges_bytes, vec![(9, 19, false)]);
        assert!(editor.folded_start_bytes.contains(&9));

        editor.undo();
        assert_eq!(editor.foldable_ranges_bytes, vec![(10, 20, false)]);
        assert!(editor.folded_start_bytes.contains(&10));

        editor.redo();
        assert_eq!(editor.foldable_ranges_bytes, vec![(9, 19, false)]);
        assert!(editor.folded_start_bytes.contains(&9));
    }

    #[test]
    fn history_size_tracks_steps_moved_between_undo_and_redo() {
        let mut editor = Editor::new(16);
        editor.insert_str("abc");
        assert_eq!(editor.history_size, 3);

        editor.undo();
        assert_eq!(editor.history_size, 0);

        editor.redo();
        assert_eq!(editor.history_size, 3);

        editor.undo();
        editor.insert_str("x");
        assert_eq!(editor.history_size, 1);
        assert!(editor.redo_stack.is_empty());
    }

    #[test]
    fn preserving_history_snaps_cursor_to_new_text_utf8_boundary() {
        let mut editor = Editor::new(8);
        editor.insert_str("a");
        editor.clear_history();
        editor.cursor = 1;

        editor.set_text_preserve_history("é");

        assert_eq!(editor.cursor, 0);
        assert!(editor.get_full_text().is_char_boundary(editor.cursor));
        assert_eq!(editor.backspace(), None);
        assert_eq!(editor.get_full_text(), "é");
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

        let mut tab_indented = Editor::new(32);
        tab_indented.set_clean_text("\tif ready:");
        tab_indented.cursor = tab_indented.len();
        assert_eq!(tab_indented.get_auto_indent(), "\t    ");
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

        assert!(matches!(
            words.undo(),
            Some(UndoRedoDelta::Replace(0, 3, old_text, new_text))
                if old_text == "foo" && new_text == "BAR"
        ));
        assert_eq!(words.get_full_text(), "foobaz");
        assert!(matches!(
            words.redo(),
            Some(UndoRedoDelta::Replace(0, 3, old_text, new_text))
                if old_text == "BAR" && new_text == "foo"
        ));
        assert_eq!(words.get_full_text(), "BARbaz");

        let mut punctuation = Editor::new(16);
        punctuation.set_clean_text("foo,bar");
        punctuation.cursor = 5;
        punctuation.select_word();
        assert_eq!(punctuation.get_selection().as_deref(), Some("bar"));
        punctuation.selection_anchor = None;
        punctuation.cursor = punctuation.len();
        assert_eq!(punctuation.delete_word_backward(), Some((4, 3)));
        assert_eq!(punctuation.get_full_text(), "foo,");
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

        editor.set_git_base_text(Some("root\n    child\n\n\tgrand\n".to_string()));
        let kid_start = editor.get_full_text().find("kid").unwrap();
        editor.replace_range(kid_start, kid_start + "kid".len(), "child");
        assert!(editor.get_line_modification_state(1).is_none());
        assert!(editor.is_dirty());

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

    fn editor_with_git_base(base: &str, current: &str) -> Editor {
        let mut editor = Editor::new(current.len().saturating_add(16));
        editor.set_clean_text(current);
        editor.set_git_base_text(Some(base.to_string()));
        editor
    }

    #[test]
    fn git_line_change_kinds_follow_hunk_semantics() {
        let inserted = editor_with_git_base("a\nb\n", "a\nnew\nb\n");
        assert_eq!(
            inserted.get_git_line_change_kind(1),
            Some(GitChangeKind::Added)
        );
        assert!(inserted.git_hunks.iter().any(|hunk| {
            hunk.before_start == hunk.before_end
                && hunk.after_start <= 1
                && hunk.after_end > 1
        }));

        let inserted_into_empty = editor_with_git_base("", "first\n");
        assert_eq!(
            inserted_into_empty.get_git_line_change_kind(0),
            Some(GitChangeKind::Added)
        );

        let replaced = editor_with_git_base("a\nold\nb\n", "a\nchanged\nb\n");
        assert_eq!(
            replaced.get_git_line_change_kind(1),
            Some(GitChangeKind::Modified)
        );
        assert!(replaced.git_hunks.iter().any(|hunk| {
            hunk.before_start < hunk.before_end
                && hunk.after_start <= 1
                && hunk.after_end > 1
        }));

        let unequal_replacement = editor_with_git_base(
            "a\nold1\nold2\nb\n",
            "a\nnew1\nnew2\nnew3\nb\n",
        );
        for line in 1..=3 {
            assert_eq!(
                unequal_replacement.get_git_line_change_kind(line),
                Some(GitChangeKind::Modified)
            );
        }

        let deleted = editor_with_git_base("a\ngone\nb\n", "a\nb\n");
        let deletion_hunk = deleted
            .git_hunks
            .iter()
            .find(|hunk| {
                hunk.before_start < hunk.before_end && hunk.after_start == hunk.after_end
            })
            .unwrap();
        assert!(matches!(
            deleted.deleted_gaps.get(deletion_hunk.after_start),
            Some(Some(LineModState::ModifiedSaved))
        ));
        assert!(deleted
            .line_states
            .iter()
            .enumerate()
            .all(|(line, _)| deleted.get_git_line_change_kind(line).is_none()));
    }

    #[test]
    fn git_line_change_kinds_survive_separate_hunks_without_semantic_merging() {
        let editor = editor_with_git_base(
            "keep0\nold_mod\nkeep1\ngone\nkeep2\nkeep3\n",
            "keep0\nnew_mod\nkeep1\nkeep2\nadded\nkeep3\n",
        );

        assert_eq!(
            editor.get_git_line_change_kind(1),
            Some(GitChangeKind::Modified)
        );
        assert_eq!(
            editor.get_git_line_change_kind(4),
            Some(GitChangeKind::Added)
        );
        assert!(editor.get_git_line_change_kind(0).is_none());
        assert!(editor.get_git_line_change_kind(2).is_none());
        assert!(editor.get_git_line_change_kind(3).is_none());
        assert!(editor.get_git_line_change_kind(5).is_none());

        let mut saw_added = false;
        let mut saw_modified = false;
        let mut deletion_gap = None;
        for hunk in &editor.git_hunks {
            match (
                hunk.before_start == hunk.before_end,
                hunk.after_start == hunk.after_end,
            ) {
                (true, false) => saw_added = true,
                (false, true) => deletion_gap = Some(hunk.after_start),
                (false, false) => saw_modified = true,
                (true, true) => {}
            }
        }

        assert!(saw_added);
        assert!(saw_modified);
        let deletion_gap = deletion_gap.unwrap();
        assert!(matches!(
            editor.deleted_gaps.get(deletion_gap),
            Some(Some(LineModState::ModifiedSaved))
        ));
    }

    #[test]
    fn non_git_dirty_markers_keep_saved_unsaved_semantics() {
        let mut editor = editor_with_git_base("a\nb\n", "a\nnew\nb\n");
        assert_eq!(
            editor.get_git_line_change_kind(1),
            Some(GitChangeKind::Added)
        );

        editor.set_clean_text("a\nb\n");
        let b_start = editor.get_full_text().find('b').unwrap();
        editor.replace_range(b_start, b_start + 1, "changed");
        assert!(matches!(
            editor.get_line_modification_state(1),
            Some(LineModState::ModifiedUnsaved)
        ));
        assert!(editor.get_git_line_change_kind(1).is_none());

        editor.mark_saved();
        assert!(matches!(
            editor.get_line_modification_state(1),
            Some(LineModState::ModifiedSaved)
        ));
        assert!(editor.get_git_line_change_kind(1).is_none());
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

        editor.set_clean_text("alpha");
        editor.selection_anchor = Some(0);
        editor.cursor = editor.len();
        let version = editor.version;
        assert_eq!(editor.insert_str(""), (None, 0));
        assert_eq!(editor.get_full_text(), "alpha");
        assert_eq!(editor.get_selection().as_deref(), Some("alpha"));
        assert_eq!(editor.version, version);
        assert!(editor.history.is_empty());
        assert!(editor.sync_edits.is_empty());

        editor.selection_anchor = None;
        editor.cursor = editor.len();
        editor.insert_str(" beta\nlast");
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

#[cfg(test)]
mod round3_editor_regressions {
    use super::*;

    #[test]
    fn r3_097_editor_version_wraps_to_new_nonzero_generation() {
        assert_eq!(next_editor_version(u64::MAX), 1);
        assert_eq!(next_editor_version(0), 1);
        assert_eq!(next_editor_version(41), 42);
    }

    #[test]
    fn r3_098_lsp_document_version_stays_positive_after_editor_counter_wraps() {
        assert_eq!(lsp_document_version(0), 0);
        assert_eq!(lsp_document_version(i32::MAX as u64), i32::MAX);
        assert_eq!(lsp_document_version(i32::MAX as u64 + 1), 1);
        assert!((1..=i32::MAX).contains(&lsp_document_version(u64::MAX)));
    }

    #[test]
    fn r3_099_api_editor_generation_changes_after_u64_max_edit() {
        let mut editor = Editor::new(32);
        editor.insert_str("a");
        editor.version = u64::MAX;
        editor.replace_range(0, 1, "b");
        assert_eq!(editor.version, 1);
        assert_eq!(editor.get_full_text(), "b");
    }

    #[test]
    fn r3_111_replace_range_rejects_reversed_out_of_bounds_and_split_utf8() {
        let mut editor = Editor::new(32);
        editor.insert_str("aé😀z");
        let before = editor.get_full_text();
        let version = editor.version;
        assert_eq!(editor.replace_range(4, 2, "x").2, "");
        assert_eq!(editor.replace_range(0, 999, "x").2, "");
        assert_eq!(editor.replace_range(2, 3, "x").2, "");
        assert_eq!(editor.get_full_text(), before);
        assert_eq!(editor.version, version);
    }
}
