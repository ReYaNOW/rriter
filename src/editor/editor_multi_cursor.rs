impl Editor {
    pub fn toggle_extra_cursor(&mut self, pos: usize) {
        let pos = self.valid_cursor(pos);
        if pos == self.cursor {
            return;
        }
        match self.extra_cursors.binary_search(&pos) {
            Ok(index) => {
                self.extra_cursors.remove(index);
            }
            Err(index) => self.extra_cursors.insert(index, pos),
        }
    }

    pub fn clear_extra_cursors(&mut self) {
        self.extra_cursors.clear();
    }

    pub fn has_extra_cursors(&self) -> bool {
        !self.extra_cursors.is_empty()
    }

    pub fn extra_cursors(&self) -> &[usize] {
        &self.extra_cursors
    }

    pub fn apply_at_all_cursors(&mut self, mut op: impl FnMut(&mut Editor)) {
        self.apply_at_all_cursors_indexed(|editor, _| op(editor));
    }

    fn apply_at_all_cursors_indexed(&mut self, mut op: impl FnMut(&mut Editor, usize)) {
        if self.extra_cursors.is_empty() {
            op(self, 0);
            return;
        }

        let before = self.caret_snapshot();
        let mut ordered: Vec<(usize, bool)> = self
            .extra_cursors
            .iter()
            .map(|&pos| (pos, false))
            .chain(std::iter::once((self.cursor, true)))
            .collect();
        ordered.sort_by_key(|caret| caret.0);
        let mut carets: Vec<(usize, bool, usize)> = ordered
            .into_iter()
            .enumerate()
            .map(|(index, (pos, primary))| (pos, primary, index))
            .collect();
        carets.sort_by_key(|caret| std::cmp::Reverse(caret.0));

        let history_start = self.history.len();
        let group_id = self.next_history_group_id;
        self.next_history_group_id = self.next_history_group_id.wrapping_add(1).max(1);
        self.active_history_group_id = Some(group_id);
        let mut processed: Vec<(usize, bool)> = Vec::with_capacity(carets.len());
        for (pos, is_primary, order) in carets {
            let pos = self.valid_cursor(pos);
            let before_len = self.len();
            self.cursor = pos;
            self.selection_anchor = None;
            op(self, order);
            let after_len = self.len();
            let delta = after_len as isize - before_len as isize;
            for (processed_pos, _) in &mut processed {
                *processed_pos = if delta >= 0 {
                    processed_pos.saturating_add(delta as usize)
                } else {
                    processed_pos.saturating_sub(delta.unsigned_abs())
                };
                *processed_pos = self.valid_cursor(*processed_pos);
            }
            processed.push((self.valid_cursor(self.cursor), is_primary));
        }
        self.active_history_group_id = None;

        if let Some((primary, _)) = processed.iter().find(|(_, is_primary)| *is_primary) {
            self.cursor = *primary;
        }
        self.selection_anchor = None;
        self.extra_cursors.clear();
        for (pos, is_primary) in processed {
            if is_primary || pos == self.cursor || self.extra_cursors.binary_search(&pos).is_ok() {
                continue;
            }
            let index = self.extra_cursors.binary_search(&pos).unwrap_or_else(|index| index);
            self.extra_cursors.insert(index, pos);
        }

        let after = self.caret_snapshot();
        let group_steps: Vec<usize> = self
            .history
            .iter()
            .enumerate()
            .skip(history_start.min(self.history.len()))
            .filter_map(|(index, step)| (step.group_id == Some(group_id)).then_some(index))
            .collect();
        if let (Some(&first), Some(&last)) = (group_steps.first(), group_steps.last()) {
            if let Some(step) = self.history.get_mut(first) {
                step.group_before = Some(before);
            }
            if let Some(step) = self.history.get_mut(last) {
                step.group_after = Some(after);
            }
        }
    }

    pub fn paste_at_all_cursors(&mut self, text: &str) {
        let caret_count = self.extra_cursors.len() + 1;
        let lines: Vec<&str> = text.split('\n').collect();
        if self.extra_cursors.is_empty() || lines.len() != caret_count {
            self.apply_at_all_cursors(|editor| {
                editor.insert_str(text);
            });
            return;
        }
        self.apply_at_all_cursors_indexed(|editor, order| {
            if let Some(line) = lines.get(order) {
                editor.insert_str(line);
            }
        });
    }

    pub fn move_all_cursors(&mut self, mut op: impl FnMut(&mut Editor)) {
        if self.extra_cursors.is_empty() {
            op(self);
            return;
        }
        let mut carets: Vec<(usize, bool)> = self
            .extra_cursors
            .iter()
            .copied()
            .map(|pos| (pos, false))
            .chain(std::iter::once((self.cursor, true)))
            .collect();
        for (pos, is_primary) in &mut carets {
            self.cursor = *pos;
            self.selection_anchor = None;
            op(self);
            *pos = self.valid_cursor(self.cursor);
            if *is_primary {
                self.cursor = *pos;
            }
        }
        if let Some((primary, _)) = carets.iter().find(|(_, is_primary)| *is_primary) {
            self.cursor = *primary;
        }
        self.extra_cursors = carets
            .into_iter()
            .filter_map(|(pos, primary)| (!primary && pos != self.cursor).then_some(pos))
            .collect();
        self.extra_cursors.sort_unstable();
        self.extra_cursors.dedup();
        self.selection_anchor = None;
    }

    fn caret_snapshot(&self) -> Vec<usize> {
        std::iter::once(self.cursor)
            .chain(self.extra_cursors.iter().copied())
            .collect()
    }

    fn restore_caret_snapshot(&mut self, snapshot: &[usize]) {
        if let Some(&primary) = snapshot.first() {
            self.cursor = self.valid_cursor(primary);
            self.extra_cursors = snapshot
                .iter()
                .skip(1)
                .map(|&pos| self.valid_cursor(pos))
                .filter(|&pos| pos != self.cursor)
                .collect();
            self.extra_cursors.sort_unstable();
            self.extra_cursors.dedup();
        } else {
            self.extra_cursors.clear();
        }
        self.selection_anchor = None;
    }

    fn undo_with_groups(&mut self) -> Option<UndoRedoDelta> {
        let first = self.history.pop_back()?;
        let group_id = first.group_id;
        let mut pending = Some(first);
        let mut result = None;
        let mut snapshot = None;
        self.is_working_history = true;
        loop {
            let mut step = if let Some(step) = pending.take() {
                step
            } else if group_id.is_some()
                && self.history.back().is_some_and(|step| step.group_id == group_id)
            {
                if let Some(step) = self.history.pop_back() {
                    step
                } else {
                    break;
                }
            } else {
                break;
            };
            self.history_size = self.history_size.saturating_sub(edit_op_size(&step.op));
            let delta = match &mut step.op {
                EditOp::Insert { offset, text } => {
                    let len = text.len();
                    self.shift_folds_delete(*offset, len);
                    self.move_gap(*offset);
                    self.sync_edits.push(SyncEdit::Delete { offset: *offset, len });
                    self.gap_end += len;
                    self.cursor = *offset;
                    UndoRedoDelta::Delete(*offset, len)
                }
                EditOp::Delete { offset, text } => {
                    self.cursor = *offset;
                    self.insert_str_internal(text);
                    UndoRedoDelta::Insert(*offset, text.len(), text.clone())
                }
                EditOp::Replace { offset, old_text, new_text } => {
                    let len = new_text.len();
                    self.shift_folds_delete(*offset, len);
                    self.move_gap(*offset);
                    self.gap_end += len;
                    self.sync_edits.push(SyncEdit::Delete { offset: *offset, len });
                    self.cursor = *offset;
                    self.insert_str_internal(old_text);
                    UndoRedoDelta::Replace(*offset, len, old_text.clone(), new_text.clone())
                }
            };
            if result.is_none() {
                result = Some(delta);
            }
            if let Some(before) = step.group_before.take() {
                snapshot = Some(before);
            }
            self.redo_stack.push_back(step);
        }
        if let Some(snapshot) = snapshot {
            self.restore_caret_snapshot(&snapshot);
        } else if let Some(step) = self.redo_stack.back() {
            self.cursor = self.valid_cursor(step.cursor_before);
            self.selection_anchor = None;
        }
        self.is_working_history = false;
        self.version = next_editor_version(self.version);
        self.update_modifications();
        result
    }

    fn redo_with_groups(&mut self) -> Option<UndoRedoDelta> {
        let first = self.redo_stack.pop_back()?;
        let group_id = first.group_id;
        let mut pending = Some(first);
        let mut result = None;
        let mut snapshot = None;
        self.is_working_history = true;
        loop {
            let step = if let Some(step) = pending.take() {
                step
            } else if group_id.is_some()
                && self.redo_stack.back().is_some_and(|step| step.group_id == group_id)
            {
                if let Some(step) = self.redo_stack.pop_back() {
                    step
                } else {
                    break;
                }
            } else {
                break;
            };
            let delta = match &step.op {
                EditOp::Insert { offset, text } => {
                    self.cursor = *offset;
                    self.insert_str_internal(text);
                    UndoRedoDelta::Insert(*offset, text.len(), text.clone())
                }
                EditOp::Delete { offset, text } => {
                    let len = text.len();
                    self.shift_folds_delete(*offset, len);
                    self.move_gap(*offset);
                    self.sync_edits.push(SyncEdit::Delete { offset: *offset, len });
                    self.gap_end += len;
                    self.cursor = *offset;
                    UndoRedoDelta::Delete(*offset, len)
                }
                EditOp::Replace { offset, old_text, new_text } => {
                    let len = old_text.len();
                    self.shift_folds_delete(*offset, len);
                    self.move_gap(*offset);
                    self.gap_end += len;
                    self.sync_edits.push(SyncEdit::Delete { offset: *offset, len });
                    self.cursor = *offset;
                    self.insert_str_internal(new_text);
                    UndoRedoDelta::Replace(*offset, len, new_text.clone(), old_text.clone())
                }
            };
            if result.is_none() {
                result = Some(delta);
            }
            if let Some(after) = step.group_after.as_ref() {
                snapshot = Some(after.clone());
            }
            self.history_size = self.history_size.saturating_add(edit_op_size(&step.op));
            self.history.push_back(step);
        }
        if let Some(snapshot) = snapshot {
            self.restore_caret_snapshot(&snapshot);
        } else if let Some(step) = self.history.back() {
            self.cursor = self.valid_cursor(step.cursor_after);
            self.selection_anchor = None;
        }
        self.is_working_history = false;
        self.version = next_editor_version(self.version);
        self.update_modifications();
        result
    }
}

#[cfg(test)]
mod multi_cursor_tests {
    use super::*;

    fn make_editor(text: &str, primary: usize, extras: &[usize]) -> Editor {
        let mut editor = Editor::new(text.len() + 32);
        editor.set_text_clean(text);
        editor.cursor = primary;
        for &pos in extras {
            editor.toggle_extra_cursor(pos);
        }
        editor
    }

    #[test]
    fn toggle_cursors_add_remove_clamp_and_respect_utf8_boundaries() {
        let mut editor = make_editor("aéz", 0, &[]);
        editor.toggle_extra_cursor(2);
        assert_eq!(editor.extra_cursors(), &[1]);
        editor.toggle_extra_cursor(1);
        assert!(editor.extra_cursors().is_empty());
        editor.toggle_extra_cursor(3);
        assert_eq!(editor.extra_cursors(), &[3]);
        editor.toggle_extra_cursor(3);
        assert!(editor.extra_cursors().is_empty());
        editor.toggle_extra_cursor(0);
        assert!(editor.extra_cursors().is_empty());
        editor.toggle_extra_cursor(100);
        assert_eq!(editor.extra_cursors(), &[4]);
    }

    #[test]
    fn applies_insertions_at_all_cursors_and_sync_edits_replay() {
        let mut editor = make_editor("a\nb\nc", 0, &[2, 4]);
        let original = editor.get_full_text();
        editor.sync_edits.clear();
        editor.apply_at_all_cursors(|editor| {
            editor.insert_str("x");
        });
        assert_eq!(editor.get_full_text(), "xa\nxb\nxc");
        let mut replay = original;
        for edit in &editor.sync_edits {
            match edit {
                SyncEdit::Insert { offset, text } => replay.insert_str(*offset, text),
                SyncEdit::Delete { offset, len } => replay.replace_range(*offset..offset + len, ""),
            }
        }
        assert_eq!(replay, editor.get_full_text());
        assert_eq!(editor.extra_cursors(), &[4, 7]);
    }

    #[test]
    fn backspace_merges_adjacent_line_start_carets_and_undo_redo_restores_all() {
        let mut editor = make_editor("\na\nb", 0, &[1, 3]);
        editor.apply_at_all_cursors(|editor| {
            editor.backspace();
        });
        assert_eq!(editor.get_full_text(), "ab");
        assert_eq!(editor.extra_cursors(), &[1]);
        assert!(editor.undo().is_some());
        assert_eq!(editor.get_full_text(), "\na\nb");
        assert_eq!(editor.cursor, 0);
        assert_eq!(editor.extra_cursors(), &[1, 3]);
        assert!(editor.redo().is_some());
        assert_eq!(editor.get_full_text(), "ab");
        assert_eq!(editor.extra_cursors(), &[1]);
    }

    #[test]
    fn each_multi_cursor_action_is_one_undo_step_and_paste_distributes_lines() {
        let mut editor = make_editor("a\nb\nc", 0, &[2, 4]);
        editor.paste_at_all_cursors("1\n2\n3");
        assert_eq!(editor.get_full_text(), "1a\n2b\n3c");
        editor.apply_at_all_cursors(|editor| {
            editor.insert_str("!");
        });
        assert!(editor.undo().is_some());
        assert_eq!(editor.get_full_text(), "1a\n2b\n3c");
        assert!(editor.undo().is_some());
        assert_eq!(editor.get_full_text(), "a\nb\nc");
        assert!(editor.redo().is_some());
        assert_eq!(editor.get_full_text(), "1a\n2b\n3c");
        let mut mismatch = make_editor("a\nb", 0, &[2]);
        mismatch.paste_at_all_cursors("whole");
        assert_eq!(mismatch.get_full_text(), "wholea\nwholeb");
    }

    #[test]
    fn whole_text_replacement_clears_extra_cursors() {
        let mut editor = make_editor("abc", 0, &[2]);
        editor.set_text_preserve_history("xyz");
        assert!(!editor.has_extra_cursors());
        editor.toggle_extra_cursor(1);
        editor.set_clean_text("done");
        assert!(!editor.has_extra_cursors());
    }
}
