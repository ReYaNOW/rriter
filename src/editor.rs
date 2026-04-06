use crate::highlighter::SyncEdit;
use crate::renderer::Renderer;
use rustc_hash::FxHasher;
use std::collections::VecDeque;
use std::hash::Hasher;

fn get_diff_info(old: &[u64], new: &[u64]) -> (Vec<bool>, Vec<bool>) {
    let n = old.len();
    let m = new.len();
    let mut modified = vec![true; m];
    let mut deleted_gaps = vec![false; m + 1];

    let mut prefix = 0;
    while prefix < n && prefix < m && old[prefix] == new[prefix] {
        modified[prefix] = false;
        prefix += 1;
    }

    let mut suffix = 0;
    while suffix < n - prefix && suffix < m - prefix && old[n - 1 - suffix] == new[m - 1 - suffix] {
        modified[m - 1 - suffix] = false;
        suffix += 1;
    }

    let n_mid = n - prefix - suffix;
    let m_mid = m - prefix - suffix;

    let mut matches = Vec::new();

    if n_mid > 0 && m_mid > 0 && n_mid * m_mid <= 4_000_000 {
        let mut dp = vec![vec![0; m_mid + 1]; n_mid + 1];
        for i in 0..n_mid {
            for j in 0..m_mid {
                if old[prefix + i] == new[prefix + j] {
                    dp[i + 1][j + 1] = dp[i][j] + 1;
                } else {
                    dp[i + 1][j + 1] = dp[i + 1][j].max(dp[i][j + 1]);
                }
            }
        }

        let mut i = n_mid;
        let mut j = m_mid;
        while i > 0 && j > 0 {
            if old[prefix + i - 1] == new[prefix + j - 1] {
                matches.push((prefix + i - 1, prefix + j - 1));
                modified[prefix + j - 1] = false;
                i -= 1;
                j -= 1;
            } else if dp[i - 1][j] >= dp[i][j - 1] {
                i -= 1;
            } else {
                j -= 1;
            }
        }
    }

    for k in 0..prefix {
        matches.push((k, k));
    }
    for k in 0..suffix {
        matches.push((n - 1 - k, m - 1 - k));
    }
    matches.sort_unstable_by_key(|&(o, _)| o);

    let mut old_to_new = vec![None; n];
    for (o, n_idx) in matches {
        old_to_new[o] = Some(n_idx);
    }

    let mut last_mapped_new = 0;
    for o in 0..n {
        if let Some(n_idx) = old_to_new[o] {
            last_mapped_new = n_idx + 1;
        } else {
            deleted_gaps[last_mapped_new] = true;
        }
    }

    for i in 1..m {
        let mut curr = i;
        while curr > 0 && modified[curr] && !modified[curr - 1] && new[curr] == new[curr - 1] {
            modified.swap(curr, curr - 1);
            curr -= 1;
        }
    }

    for i in 1..=m {
        let mut curr = i;
        while curr > 1
            && deleted_gaps[curr]
            && !deleted_gaps[curr - 1]
            && new[curr - 1] == new[curr - 2]
        {
            deleted_gaps.swap(curr, curr - 1);
            curr -= 1;
        }
    }

    (modified, deleted_gaps)
}

fn is_delimiter(b: u8) -> bool {
    matches!(
        b,
        b' ' | b'\n' | b'\r' | b'\t' | b'.' | b'(' | b')' | b'[' | b']' | b'{' | b'}'
    )
}

fn char_class(b: u8) -> u8 {
    if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
        0
    } else if b.is_ascii_punctuation() && b != b'_' {
        2
    } else {
        1
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LineModState {
    ModifiedUnsaved,
    ModifiedSaved,
}

#[derive(Clone)]
pub enum EditOp {
    Insert { offset: usize, text: String },
    Delete { offset: usize, text: String },
}

#[derive(Clone)]
pub struct HistoryStep {
    pub op: EditOp,
    pub cursor_before: usize,
    pub cursor_after: usize,
}

pub enum UndoRedoDelta {
    Insert(usize, usize, String),
    Delete(usize, usize),
}

pub struct Editor {
    data: Vec<u8>,
    gap_start: usize,
    gap_end: usize,
    pub cursor: usize,
    pub selection_anchor: Option<usize>,
    pub version: u64,
    pub line_offsets: Vec<usize>,
    pub longest_line_idx: usize,

    pub history: VecDeque<HistoryStep>,
    pub redo_stack: VecDeque<HistoryStep>,
    pub history_size: usize,
    pub is_working_history: bool,

    pub original_hashes: Vec<u64>,
    pub saved_hashes: Vec<u64>,
    pub line_states: Vec<Option<LineModState>>,
    pub deleted_gaps: Vec<Option<LineModState>>,
    pub is_dirty: bool,

    indent_cache: Vec<u8>,
    last_indent_version: u64,

    pub sync_edits: Vec<SyncEdit>,
    pub foldable_lines: std::collections::HashMap<usize, usize>,
    pub folded_lines: std::collections::HashSet<usize>,
    pub folded_start_bytes: std::collections::HashSet<usize>,
    pub foldable_ranges_bytes: Vec<(usize, usize)>,
}

impl Editor {
    pub fn new(capacity: usize) -> Self {
        Self {
            data: vec![0; capacity],
            gap_start: 0,
            gap_end: capacity,
            cursor: 0,
            selection_anchor: None,
            version: 0,
            line_offsets: vec![0],
            longest_line_idx: 0,
            history: VecDeque::new(),
            redo_stack: VecDeque::new(),
            history_size: 0,
            is_working_history: false,
            original_hashes: vec![],
            saved_hashes: vec![],
            line_states: vec![],
            deleted_gaps: vec![],
            is_dirty: false,
            indent_cache: Vec::new(),
            last_indent_version: u64::MAX,
            sync_edits: Vec::new(),
            foldable_lines: std::collections::HashMap::new(),
            folded_lines: std::collections::HashSet::new(),
            folded_start_bytes: std::collections::HashSet::new(),
            foldable_ranges_bytes: Vec::new(),
        }
    }

    pub fn shift_folds_insert(&mut self, offset: usize, len: usize) {
        let mut new_folded = std::collections::HashSet::new();
        for &b in &self.folded_start_bytes {
            if b >= offset {
                new_folded.insert(b + len);
            } else {
                new_folded.insert(b);
            }
        }
        self.folded_start_bytes = new_folded;

        for range in &mut self.foldable_ranges_bytes {
            if range.0 >= offset {
                range.0 += len;
            }
            // ИСПРАВЛЕНИЕ: строго больше (>), а не (>=).
            // Это гарантирует, что вставка после закрывающей скобки фолда не захватится им.
            if range.1 > offset {
                range.1 += len;
            }
        }
    }

    pub fn shift_folds_delete(&mut self, offset: usize, len: usize) {
        let mut new_folded = std::collections::HashSet::new();
        let end = offset + len;
        for &b in &self.folded_start_bytes {
            if b >= end {
                new_folded.insert(b - len);
            } else if b >= offset {
                // block start was deleted, let it unfold
            } else {
                new_folded.insert(b);
            }
        }
        self.folded_start_bytes = new_folded;

        for range in &mut self.foldable_ranges_bytes {
            if range.0 >= end {
                range.0 -= len;
            } else if range.0 >= offset {
                range.0 = offset;
            }

            if range.1 >= end {
                range.1 -= len;
            } else if range.1 >= offset {
                range.1 = offset;
            }
        }
    }

    pub fn rebuild_line_offsets(&mut self) {
        let mut new_offsets = Vec::with_capacity(1024);
        new_offsets.push(0);

        let (first, second) = self.text_parts();
        let mut offset = 0;
        let mut max_len = 0;
        let mut current_longest_idx = 0;
        let mut current_line_start = 0;
        let mut current_line_idx = 0;

        let mut process = |bytes: &[u8], mut_offset: &mut usize| {
            for &b in bytes {
                *mut_offset += 1;
                if b == b'\n' {
                    let len = *mut_offset - current_line_start;
                    if len > max_len {
                        max_len = len;
                        current_longest_idx = current_line_idx;
                    }
                    new_offsets.push(*mut_offset);
                    current_line_start = *mut_offset;
                    current_line_idx += 1;
                }
            }
        };

        process(first.as_bytes(), &mut offset);
        process(second.as_bytes(), &mut offset);

        let len = offset - current_line_start;
        if len > max_len {
            current_longest_idx = current_line_idx;
        }

        self.line_offsets = new_offsets;
        self.longest_line_idx = current_longest_idx;

        self.folded_lines.clear();
        for &b in &self.folded_start_bytes {
            let new_line = self
                .line_offsets
                .partition_point(|&o| o <= b)
                .saturating_sub(1);
            self.folded_lines.insert(new_line);
        }

        self.folded_start_bytes.clear();
        for &l in &self.folded_lines {
            if l < self.line_offsets.len() {
                self.folded_start_bytes.insert(self.line_offsets[l]);
            }
        }

        self.foldable_lines.clear();
        for &(start_b, end_b) in &self.foldable_ranges_bytes {
            let sl = self
                .line_offsets
                .partition_point(|&o| o <= start_b)
                .saturating_sub(1);
            let el = self
                .line_offsets
                .partition_point(|&o| o <= end_b)
                .saturating_sub(1);
            if el > sl {
                self.foldable_lines.insert(sl, el);
            }
        }
    }

    fn get_line_hashes(&self) -> Vec<u64> {
        let mut hashes = Vec::with_capacity(1024);
        let mut hasher = FxHasher::default();
        let (first, second) = self.text_parts();

        let mut process_slice = |bytes: &[u8]| {
            for &b in bytes {
                if b == b'\n' {
                    hashes.push(hasher.finish());
                    hasher = FxHasher::default();
                } else {
                    hasher.write_u8(b);
                }
            }
        };

        process_slice(first.as_bytes());
        process_slice(second.as_bytes());
        hashes.push(hasher.finish());

        hashes
    }

    pub fn ensure_indent_cache_updated(&mut self) {
        if self.version == self.last_indent_version {
            return;
        }

        self.indent_cache.clear();
        let mut count = 0;
        let mut is_blank = true;
        let mut raw_depths = Vec::with_capacity(1024);

        let (first, second) = self.text_parts();

        let mut process = |bytes: &[u8]| {
            for &b in bytes {
                if b == b'\n' {
                    raw_depths.push((count / 4, is_blank));
                    count = 0;
                    is_blank = true;
                } else if is_blank {
                    if b == b' ' {
                        count += 1;
                    } else if b == b'\t' {
                        count = (count / 4 + 1) * 4;
                    } else if b != b'\r' {
                        is_blank = false;
                    }
                }
            }
        };

        process(first.as_bytes());
        process(second.as_bytes());
        raw_depths.push((count / 4, is_blank));

        let num_lines = raw_depths.len();
        self.indent_cache.resize(num_lines, 0);

        let mut prev_non_blank = vec![0; num_lines];
        let mut curr_prev = 0;
        for i in 0..num_lines {
            if !raw_depths[i].1 {
                curr_prev = raw_depths[i].0;
            }
            prev_non_blank[i] = curr_prev;
        }

        let mut next_non_blank = vec![0; num_lines];
        let mut curr_next = 0;
        for i in (0..num_lines).rev() {
            if !raw_depths[i].1 {
                curr_next = raw_depths[i].0;
            }
            next_non_blank[i] = curr_next;
        }

        for i in 0..num_lines {
            let (raw_d, is_blank) = raw_depths[i];
            if !is_blank {
                self.indent_cache[i] = raw_d as u8;
            } else {
                if raw_d > 0 {
                    self.indent_cache[i] = raw_d as u8;
                } else {
                    self.indent_cache[i] = prev_non_blank[i].min(next_non_blank[i]) as u8;
                }
            }
        }

        self.last_indent_version = self.version;
    }

    pub fn get_cached_indent_levels(&self) -> &[u8] {
        &self.indent_cache
    }

    pub fn backspace(&mut self) -> Option<(usize, usize)> {
        if let Some(del_info) = self.delete_selection() {
            self.update_modifications();
            return Some(del_info);
        }

        if self.cursor > 0 && self.cursor < self.len() {
            let char_before = self.byte_at(self.cursor - 1);
            let char_after = self.byte_at(self.cursor);

            let is_pair = (char_before == b'(' && char_after == b')')
                || (char_before == b'[' && char_after == b']')
                || (char_before == b'{' && char_after == b'}');

            if is_pair {
                self.version += 1;
                let start = self.cursor - 1;
                let len = 2;
                let text_to_save = format!("{}{}", char_before as char, char_after as char);

                let cursor_before = self.cursor;
                self.shift_folds_delete(start, len);
                self.move_gap(start);
                self.sync_edits
                    .push(SyncEdit::Delete { offset: start, len });
                self.gap_end += len;
                self.cursor = start;
                self.selection_anchor = None;

                self.push_history(HistoryStep {
                    op: EditOp::Delete {
                        offset: start,
                        text: text_to_save,
                    },
                    cursor_before,
                    cursor_after: self.cursor,
                });
                self.update_modifications();
                return Some((start, len));
            }
        }

        if self.cursor > 0 {
            self.version += 1;
            let cursor_before = self.cursor;
            let mut prev = self.cursor - 1;
            while prev > 0 && !self.is_char_boundary(prev) {
                prev -= 1;
            }
            let len = self.cursor - prev;
            let mut res = Vec::with_capacity(len);
            for i in prev..self.cursor {
                res.push(self.byte_at(i));
            }
            let text = String::from_utf8_lossy(&res).into_owned();

            self.shift_folds_delete(prev, len);
            self.move_gap(self.cursor);
            self.sync_edits.push(SyncEdit::Delete { offset: prev, len });
            self.gap_start -= len;
            self.cursor = prev;

            self.push_history(HistoryStep {
                op: EditOp::Delete { offset: prev, text },
                cursor_before,
                cursor_after: self.cursor,
            });
            self.update_modifications();
            return Some((prev, len));
        }
        None
    }

    pub fn set_original_text(&mut self) {
        self.original_hashes = self.get_line_hashes();
        self.saved_hashes = self.original_hashes.clone();
        self.update_modifications();
    }

    pub fn mark_saved(&mut self) {
        self.saved_hashes = self.get_line_hashes();
        self.update_modifications();
    }

    pub fn is_dirty(&self) -> bool {
        self.is_dirty
    }

    pub fn get_line_modification_state(&self, line: usize) -> Option<LineModState> {
        self.line_states.get(line).copied().flatten()
    }

    pub fn update_modifications(&mut self) {
        self.rebuild_line_offsets();

        let curr_hashes = self.get_line_hashes();

        let (mod_saved, mut del_saved) = get_diff_info(&self.saved_hashes, &curr_hashes);
        let (mod_orig, mut del_orig) = get_diff_info(&self.original_hashes, &curr_hashes);

        // Treat a line replacement (delete + insert) as just a modification.
        // This prevents showing a deletion gap marker above a modified line.
        for i in 0..curr_hashes.len() {
            if mod_saved[i] && i < del_saved.len() {
                del_saved[i] = false;
            }
            if mod_orig[i] && i < del_orig.len() {
                del_orig[i] = false;
            }
        }

        let mut states = vec![None; curr_hashes.len()];
        let mut gaps = vec![None; curr_hashes.len() + 1];
        let mut dirty = false;

        for i in 0..curr_hashes.len() {
            if mod_saved[i] {
                states[i] = Some(LineModState::ModifiedUnsaved);
                dirty = true;
            } else if mod_orig[i] {
                states[i] = Some(LineModState::ModifiedSaved);
            }
        }

        for i in 0..=curr_hashes.len() {
            if del_saved[i] {
                gaps[i] = Some(LineModState::ModifiedUnsaved);
                dirty = true;
            } else if del_orig[i] {
                gaps[i] = Some(LineModState::ModifiedSaved);
            }
        }

        self.line_states = states;
        self.deleted_gaps = gaps;
        self.is_dirty = dirty;
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
        self.redo_stack.clear();
        self.history_size = 0;
        self.update_modifications();
        self.sync_edits.clear();
    }

    fn push_history(&mut self, step: HistoryStep) {
        if self.is_working_history {
            return;
        }
        self.redo_stack.clear();
        let mut merge = false;
        if let Some(last) = self.history.back_mut() {
            if let (
                EditOp::Insert {
                    offset: last_off,
                    text: last_txt,
                },
                EditOp::Insert {
                    offset: new_off,
                    text: new_txt,
                },
            ) = (&mut last.op, &step.op)
            {
                let is_newline = new_txt.contains('\n');
                if *last_off + last_txt.len() == *new_off
                    && !is_newline
                    && new_txt.len() < 100
                    && last_txt.len() < 1000
                {
                    last_txt.push_str(new_txt);
                    last.cursor_after = step.cursor_after;
                    self.history_size += new_txt.len();
                    merge = true;
                }
            } else if let (
                EditOp::Delete {
                    offset: last_off,
                    text: last_txt,
                },
                EditOp::Delete {
                    offset: new_off,
                    text: new_txt,
                },
            ) = (&mut last.op, &step.op)
            {
                if *new_off + new_txt.len() == *last_off && new_txt.len() < 100 {
                    let mut merged = new_txt.clone();
                    merged.push_str(last_txt);
                    *last_txt = merged;
                    *last_off = *new_off;
                    last.cursor_after = step.cursor_after;
                    self.history_size += new_txt.len();
                    merge = true;
                } else if *new_off == *last_off && new_txt.len() < 100 {
                    last_txt.push_str(new_txt);
                    last.cursor_after = step.cursor_after;
                    self.history_size += new_txt.len();
                    merge = true;
                }
            }
        }
        if !merge {
            let size = match &step.op {
                EditOp::Insert { text, .. } => text.len(),
                EditOp::Delete { text, .. } => text.len(),
            };
            if size > 0 {
                self.history.push_back(step);
                self.history_size += size;
            }
        }
        while self.history_size > 50 * 1024 * 1024 {
            if let Some(old) = self.history.pop_front() {
                let old_size = match &old.op {
                    EditOp::Insert { text, .. } => text.len(),
                    EditOp::Delete { text, .. } => text.len(),
                };
                self.history_size -= old_size;
            }
        }
    }

    pub fn undo(&mut self) -> Option<UndoRedoDelta> {
        if let Some(mut step) = self.history.pop_back() {
            self.is_working_history = true;
            let delta = match &mut step.op {
                EditOp::Insert { offset, text } => {
                    self.selection_anchor = Some(*offset);
                    self.cursor = *offset + text.len();
                    let len = text.len();
                    let start = *offset;
                    self.shift_folds_delete(start, len);
                    self.move_gap(start);
                    self.sync_edits
                        .push(SyncEdit::Delete { offset: start, len });
                    self.gap_end += len;
                    self.cursor = start;
                    self.selection_anchor = None;
                    UndoRedoDelta::Delete(*offset, text.len())
                }
                EditOp::Delete { offset, text } => {
                    self.cursor = *offset;
                    self.selection_anchor = None;
                    self.insert_str_internal(text);
                    UndoRedoDelta::Insert(*offset, text.len(), text.clone())
                }
            };
            self.cursor = step.cursor_before;
            self.selection_anchor = None;
            self.redo_stack.push_back(step);
            self.is_working_history = false;
            self.version += 1;

            self.update_modifications();
            return Some(delta);
        }
        None
    }

    pub fn redo(&mut self) -> Option<UndoRedoDelta> {
        if let Some(step) = self.redo_stack.pop_back() {
            self.is_working_history = true;
            let delta = match &step.op {
                EditOp::Insert { offset, text } => {
                    self.cursor = *offset;
                    self.selection_anchor = None;
                    self.insert_str_internal(text);
                    UndoRedoDelta::Insert(*offset, text.len(), text.clone())
                }
                EditOp::Delete { offset, text, .. } => {
                    self.selection_anchor = Some(*offset);
                    self.cursor = step.cursor_before;
                    let len = text.len();
                    let start = *offset;
                    self.shift_folds_delete(start, len);
                    self.move_gap(start);
                    self.sync_edits
                        .push(SyncEdit::Delete { offset: start, len });
                    self.gap_end += len;
                    self.cursor = start;
                    self.selection_anchor = None;
                    UndoRedoDelta::Delete(*offset, step.cursor_before - *offset)
                }
            };
            self.cursor = step.cursor_after;
            self.selection_anchor = None;
            self.history.push_back(step);
            self.is_working_history = false;
            self.version += 1;

            self.update_modifications();
            return Some(delta);
        }
        None
    }

    pub fn text_parts(&self) -> (&str, &str) {
        unsafe {
            let first = std::str::from_utf8_unchecked(&self.data[..self.gap_start]);
            let second = std::str::from_utf8_unchecked(&self.data[self.gap_end..]);
            (first, second)
        }
    }

    pub fn get_full_text(&self) -> String {
        let (first, second) = self.text_parts();
        let mut s = String::with_capacity(first.len() + second.len());
        s.push_str(first);
        s.push_str(second);
        s
    }

    fn move_gap(&mut self, target: usize) {
        if target == self.gap_start {
            return;
        }
        if target < self.gap_start {
            let shift = self.gap_start - target;
            self.data
                .copy_within(target..self.gap_start, self.gap_end - shift);
            self.gap_start -= shift;
            self.gap_end -= shift;
        } else {
            let shift = target - self.gap_start;
            self.data
                .copy_within(self.gap_end..self.gap_end + shift, self.gap_start);
            self.gap_start += shift;
            self.gap_end += shift;
        }
    }

    fn insert_str_internal(&mut self, s: &str) -> usize {
        let bytes = s.as_bytes();
        let len = bytes.len();
        self.shift_folds_insert(self.cursor, len);
        self.sync_edits.push(SyncEdit::Insert {
            offset: self.cursor,
            text: s.to_string(),
        });
        self.move_gap(self.cursor);
        if self.gap_start + len > self.gap_end {
            let mut new_data = vec![0; self.data.len() * 2 + len];
            new_data[..self.gap_start].copy_from_slice(&self.data[..self.gap_start]);
            let tail_len = self.data.len() - self.gap_end;
            let new_len = new_data.len();
            new_data[new_len - tail_len..].copy_from_slice(&self.data[self.gap_end..]);
            self.gap_end = new_len - tail_len;
            self.data = new_data;
        }
        self.data[self.gap_start..self.gap_start + len].copy_from_slice(bytes);
        self.gap_start += len;
        self.cursor += len;
        len
    }

    pub fn insert_str(&mut self, s: &str) -> (Option<(usize, usize)>, usize) {
        let cursor_before = self.cursor;
        let del_info = self.delete_selection();

        if s.is_empty() {
            self.update_modifications();
            return (del_info, 0);
        }

        self.version += 1;
        let insert_offset = self.cursor;
        let len = self.insert_str_internal(s);

        self.selection_anchor = None;
        self.push_history(HistoryStep {
            op: EditOp::Insert {
                offset: insert_offset,
                text: s.to_string(),
            },
            cursor_before,
            cursor_after: self.cursor,
        });

        self.update_modifications();
        (del_info, len)
    }

    pub fn delete_selection(&mut self) -> Option<(usize, usize)> {
        if let Some(anchor) = self.selection_anchor {
            if anchor != self.cursor {
                self.version += 1;
                let start = anchor.min(self.cursor);
                let end = anchor.max(self.cursor);
                let len = end - start;
                let mut res = Vec::with_capacity(len);
                for i in start..end {
                    res.push(self.byte_at(i));
                }
                let text = String::from_utf8_lossy(&res).into_owned();

                let cursor_before = self.cursor;
                self.shift_folds_delete(start, len);
                self.move_gap(start);
                self.sync_edits
                    .push(SyncEdit::Delete { offset: start, len });
                self.gap_end += len;
                self.cursor = start;
                self.selection_anchor = None;

                self.push_history(HistoryStep {
                    op: EditOp::Delete {
                        offset: start,
                        text,
                    },
                    cursor_before,
                    cursor_after: self.cursor,
                });
                self.update_modifications();
                return Some((start, len));
            }
        }
        self.selection_anchor = None;
        None
    }

    pub fn delete_forward(&mut self) -> Option<(usize, usize)> {
        if let Some(del_info) = self.delete_selection() {
            self.update_modifications();
            return Some(del_info);
        }
        if self.cursor < self.len() {
            self.version += 1;
            let cursor_before = self.cursor;
            let mut next = self.cursor + 1;
            while next < self.len() && !self.is_char_boundary(next) {
                next += 1;
            }
            let len = next - self.cursor;
            let mut res = Vec::with_capacity(len);
            for i in self.cursor..next {
                res.push(self.byte_at(i));
            }
            let text = String::from_utf8_lossy(&res).into_owned();

            let offset = self.cursor;
            self.shift_folds_delete(offset, len);
            self.move_gap(self.cursor);
            self.sync_edits.push(SyncEdit::Delete { offset, len });
            self.gap_end += len;

            self.push_history(HistoryStep {
                op: EditOp::Delete { offset, text },
                cursor_before,
                cursor_after: self.cursor,
            });
            self.update_modifications();
            return Some((offset, len));
        }
        None
    }

    pub fn delete_word_backward(&mut self) -> Option<(usize, usize)> {
        if self.selection_anchor.is_some() && self.selection_anchor != Some(self.cursor) {
            return self.delete_selection();
        }
        if self.cursor == 0 {
            return None;
        }

        let mut p = self.cursor;
        let is_space = self.byte_at(p - 1) == b' ';

        while p > 0 {
            let b = self.byte_at(p - 1);
            if is_space {
                if b != b' ' {
                    break;
                }
            } else {
                if is_delimiter(b) {
                    break;
                }
            }
            p -= 1;
        }

        if p == self.cursor {
            p -= 1;
        }

        self.selection_anchor = Some(p);
        self.delete_selection()
    }

    pub fn delete_word_forward(&mut self) -> Option<(usize, usize)> {
        if self.selection_anchor.is_some() && self.selection_anchor != Some(self.cursor) {
            return self.delete_selection();
        }
        if self.cursor == self.len() {
            return None;
        }

        let mut p = self.cursor;
        let is_space = self.byte_at(p) == b' ';

        while p < self.len() {
            let b = self.byte_at(p);
            if is_space {
                if b != b' ' {
                    break;
                }
            } else {
                if is_delimiter(b) {
                    break;
                }
            }
            p += 1;
        }

        if p == self.cursor {
            p += 1;
        }

        self.selection_anchor = Some(self.cursor);
        self.cursor = p;
        self.delete_selection()
    }

    pub fn get_auto_indent(&self) -> String {
        let mut start = self.cursor;
        while start > 0 && self.byte_at(start - 1) != b'\n' {
            start -= 1;
        }

        let mut space_count = 0;
        let mut curr = start;
        while curr < self.cursor && self.byte_at(curr) == b' ' {
            space_count += 1;
            curr += 1;
        }

        let mut indent = " ".repeat(space_count);

        let mut p = self.cursor;
        while p > start {
            let b = self.byte_at(p - 1);
            if b != b' ' && b != b'\t' && b != b'\r' {
                if b == b':' || b == b'{' || b == b'[' || b == b'(' {
                    indent.push_str("    ");
                }
                break;
            }
            p -= 1;
        }

        indent
    }

    pub fn select_expand(&mut self) {
        let (start, end) = if let Some(anchor) = self.selection_anchor {
            (anchor.min(self.cursor), anchor.max(self.cursor))
        } else {
            (self.cursor, self.cursor)
        };

        let mut candidates = Vec::new();

        let mut word_l = start;
        while word_l > 0 && char_class(self.byte_at(word_l - 1)) == 1 {
            word_l -= 1;
        }
        let mut word_r = end;
        while word_r < self.len() && char_class(self.byte_at(word_r)) == 1 {
            word_r += 1;
        }
        if word_l < word_r {
            candidates.push((word_l, word_r));
        }

        let mut line_l = start;
        while line_l > 0 && self.byte_at(line_l - 1) != b'\n' {
            line_l -= 1;
        }
        let mut line_r = end;
        while line_r < self.len() && self.byte_at(line_r) != b'\n' {
            line_r += 1;
        }
        candidates.push((line_l, line_r));

        let mut t_line_l = line_l;
        while t_line_l < line_r
            && (self.byte_at(t_line_l) == b' ' || self.byte_at(t_line_l) == b'\t')
        {
            t_line_l += 1;
        }
        let mut t_line_r = line_r;
        while t_line_r > t_line_l
            && (self.byte_at(t_line_r - 1) == b' ' || self.byte_at(t_line_r - 1) == b'\t')
        {
            t_line_r -= 1;
        }
        if t_line_l < t_line_r {
            candidates.push((t_line_l, t_line_r));
        }

        let scan_start = start.saturating_sub(50000);
        let scan_end = (end + 50000).min(self.len());
        let mut round_stack = Vec::new();
        let mut curly_stack = Vec::new();
        let mut square_stack = Vec::new();

        for i in scan_start..scan_end {
            let b = self.byte_at(i);
            match b {
                b'(' => round_stack.push(i),
                b'{' => curly_stack.push(i),
                b'[' => square_stack.push(i),
                b')' => {
                    if let Some(open) = round_stack.pop() {
                        candidates.push((open + 1, i));
                        candidates.push((open, i + 1));
                    }
                }
                b'}' => {
                    if let Some(open) = curly_stack.pop() {
                        candidates.push((open + 1, i));
                        candidates.push((open, i + 1));
                    }
                }
                b']' => {
                    if let Some(open) = square_stack.pop() {
                        candidates.push((open + 1, i));
                        candidates.push((open, i + 1));
                    }
                }
                _ => {}
            }
        }

        let mut q_l = line_l;
        let mut last_double = None;
        let mut last_single = None;
        while q_l < line_r {
            if self.byte_at(q_l) == b'"' {
                if let Some(open) = last_double {
                    candidates.push((open + 1, q_l));
                    candidates.push((open, q_l + 1));
                    last_double = None;
                } else {
                    last_double = Some(q_l);
                }
            } else if self.byte_at(q_l) == b'\'' {
                if let Some(open) = last_single {
                    candidates.push((open + 1, q_l));
                    candidates.push((open, q_l + 1));
                    last_single = None;
                } else {
                    last_single = Some(q_l);
                }
            }
            q_l += 1;
        }

        candidates.push((0, self.len()));

        let mut best = None;
        let mut min_len = usize::MAX;

        for (l, r) in candidates {
            if l <= start && r >= end && (l < start || r > end) {
                let len = r - l;
                if len < min_len {
                    min_len = len;
                    best = Some((l, r));
                }
            }
        }

        if let Some((l, r)) = best {
            self.selection_anchor = Some(l);
            self.cursor = r;
        }
    }

    pub fn select_word(&mut self) {
        let mut start = self.cursor;
        let mut end = self.cursor;

        while start > 0 && !is_delimiter(self.byte_at(start - 1)) {
            start -= 1;
        }
        while end < self.len() && !is_delimiter(self.byte_at(end)) {
            end += 1;
        }

        if start != end {
            self.selection_anchor = Some(start);
            self.cursor = end;
        }
    }

    pub fn select_line(&mut self) {
        let mut start = self.cursor;
        let mut end = self.cursor;

        while start > 0 && self.byte_at(start - 1) != b'\n' {
            start -= 1;
        }
        while end < self.len() && self.byte_at(end) != b'\n' {
            end += 1;
        }

        self.selection_anchor = Some(start);
        self.cursor = end;
    }

    fn is_char_boundary(&self, index: usize) -> bool {
        if index == 0 || index == self.len() {
            return true;
        }
        let b = self.byte_at(index);
        b < 128 || b >= 192
    }

    pub fn byte_at(&self, idx: usize) -> u8 {
        if idx < self.gap_start {
            self.data[idx]
        } else {
            self.data[self.gap_end + (idx - self.gap_start)]
        }
    }

    pub fn len(&self) -> usize {
        self.data.len() - (self.gap_end - self.gap_start)
    }

    pub fn get_selection(&self) -> Option<String> {
        let anchor = self.selection_anchor?;
        if anchor == self.cursor {
            return None;
        }
        let start = anchor.min(self.cursor);
        let end = anchor.max(self.cursor);
        let mut res = Vec::with_capacity(end - start);
        for i in start..end {
            res.push(self.byte_at(i));
        }
        Some(String::from_utf8_lossy(&res).into_owned())
    }

    pub fn select_all(&mut self) {
        self.selection_anchor = Some(0);
        self.cursor = self.len();
    }

    fn handle_selection(&mut self, shift: bool) {
        if !shift {
            self.selection_anchor = None;
        } else if self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.cursor);
        }
    }

    /// ИСПРАВЛЕНИЕ: Новая универсальная логика обработки курсора, попадающего внутрь фолда.
    pub fn snap_cursor_out_of_fold(&mut self, old_cursor: usize) {
        let mut current_line = 0;
        while current_line < self.line_offsets.len() {
            if self.folded_lines.contains(&current_line)
                && self.foldable_lines.contains_key(&current_line)
            {
                let fold_end = self.foldable_lines[&current_line];
                let first_line_end = if current_line + 1 < self.line_offsets.len() {
                    self.line_offsets[current_line + 1].saturating_sub(1)
                } else {
                    self.len()
                };
                let block_end = if fold_end + 1 < self.line_offsets.len() {
                    self.line_offsets[fold_end + 1].saturating_sub(1)
                } else {
                    self.len()
                };

                // Если курсор оказался внутри свернутого кода (включая саму невидимую \n)
                if self.cursor >= first_line_end && self.cursor < block_end {
                    if old_cursor >= block_end {
                        // Идем влево (Left) - перепрыгиваем до видимой части первой строки
                        self.cursor = first_line_end.saturating_sub(1);
                    } else if old_cursor < first_line_end {
                        // Идем вправо (Right) - перепрыгиваем в конец блока
                        self.cursor = block_end;
                    } else {
                        // Fallback
                        if self.cursor > old_cursor {
                            self.cursor = block_end;
                        } else {
                            self.cursor = first_line_end.saturating_sub(1);
                        }
                    }
                    return;
                }
                current_line = fold_end;
            }
            current_line += 1;
        }
    }

    pub fn move_left(&mut self, shift: bool) {
        self.handle_selection(shift);
        if self.cursor > 0 {
            let old_cursor = self.cursor;
            let mut prev = self.cursor - 1;
            while prev > 0 && !self.is_char_boundary(prev) {
                prev -= 1;
            }
            self.cursor = prev;
            self.snap_cursor_out_of_fold(old_cursor);
        }
    }

    pub fn move_right(&mut self, shift: bool) {
        self.handle_selection(shift);
        if self.cursor < self.len() {
            let old_cursor = self.cursor;
            let mut next = self.cursor + 1;
            while next < self.len() && !self.is_char_boundary(next) {
                next += 1;
            }
            self.cursor = next;
            self.snap_cursor_out_of_fold(old_cursor);
        }
    }

    pub fn move_word_left(&mut self, shift: bool) {
        self.handle_selection(shift);
        if self.cursor == 0 {
            return;
        }

        let old_cursor = self.cursor;
        let mut p = self.cursor;

        while p > 0 && char_class(self.byte_at(p - 1)) == 0 {
            p -= 1;
        }

        if p > 0 {
            let cls = char_class(self.byte_at(p - 1));
            while p > 0 && char_class(self.byte_at(p - 1)) == cls {
                p -= 1;
            }
        }
        self.cursor = p;
        self.snap_cursor_out_of_fold(old_cursor);
    }

    pub fn move_word_right(&mut self, shift: bool) {
        self.handle_selection(shift);
        if self.cursor == self.len() {
            return;
        }

        let old_cursor = self.cursor;
        let mut p = self.cursor;

        while p < self.len() && char_class(self.byte_at(p)) == 0 {
            p += 1;
        }

        if p < self.len() {
            let cls = char_class(self.byte_at(p));
            while p < self.len() && char_class(self.byte_at(p)) == cls {
                p += 1;
            }
        }
        self.cursor = p;
        self.snap_cursor_out_of_fold(old_cursor);
    }

    pub fn move_home(&mut self, shift: bool) {
        self.handle_selection(shift);

        let mut current_line = 0;
        let mut snapped = false;
        while current_line < self.line_offsets.len() {
            if self.folded_lines.contains(&current_line)
                && self.foldable_lines.contains_key(&current_line)
            {
                let fold_end = self.foldable_lines[&current_line];
                let start_byte = self.line_offsets[current_line];
                let block_end = if fold_end + 1 < self.line_offsets.len() {
                    self.line_offsets[fold_end + 1].saturating_sub(1)
                } else {
                    self.len()
                };

                if self.cursor > start_byte && self.cursor <= block_end {
                    self.cursor = start_byte;
                    snapped = true;
                    break;
                }
                current_line = fold_end;
            }
            current_line += 1;
        }

        if !snapped {
            let mut curr = self.cursor;
            while curr > 0 {
                let b = self.byte_at(curr - 1);
                if b == b'\n' {
                    break;
                }
                curr -= 1;
            }
            self.cursor = curr;
        }
    }

    pub fn move_end(&mut self, shift: bool) {
        self.handle_selection(shift);
        let mut curr = self.cursor;
        while curr < self.len() {
            let b = self.byte_at(curr);
            if b == b'\n' {
                break;
            }
            curr += 1;
        }
        self.cursor = curr;

        let mut current_line = 0;
        while current_line < self.line_offsets.len() {
            if self.folded_lines.contains(&current_line)
                && self.foldable_lines.contains_key(&current_line)
            {
                let fold_end = self.foldable_lines[&current_line];
                let first_line_end = if current_line + 1 < self.line_offsets.len() {
                    self.line_offsets[current_line + 1].saturating_sub(1)
                } else {
                    self.len()
                };
                let block_end = if fold_end + 1 < self.line_offsets.len() {
                    self.line_offsets[fold_end + 1].saturating_sub(1)
                } else {
                    self.len()
                };

                // При нажатии End на первой строке фолда — перепрыгиваем в самый конец свернутого блока
                if self.cursor == first_line_end {
                    self.cursor = block_end;
                    break;
                }
                current_line = fold_end;
            }
            current_line += 1;
        }
    }

    pub fn move_start_of_file(&mut self, shift: bool) {
        self.handle_selection(shift);
        self.cursor = 0;
    }

    pub fn move_end_of_file(&mut self, shift: bool) {
        self.handle_selection(shift);
        self.cursor = self.len();
    }

    pub fn move_up(&mut self, renderer: &mut Renderer, shift: bool) {
        self.handle_selection(shift);
        let (x, y) = renderer.get_cursor_xy(self);
        self.cursor = renderer.get_byte_at_xy(self, x, y - renderer.line_height * 1.5);
    }

    pub fn move_down(&mut self, renderer: &mut Renderer, shift: bool) {
        self.handle_selection(shift);
        let (x, y) = renderer.get_cursor_xy(self);
        self.cursor = renderer.get_byte_at_xy(self, x, y + renderer.line_height * 0.5);
    }

    pub fn set_cursor_at_pos(
        &mut self,
        target_x: f32,
        target_y: f32,
        renderer: &mut Renderer,
        is_click: bool,
    ) {
        let idx = renderer.get_byte_at_xy(self, target_x, target_y);
        if is_click {
            self.selection_anchor = Some(idx);
        }
        self.cursor = idx;
    }

    pub fn move_page_up(&mut self, renderer: &mut Renderer, shift: bool, step: f32) {
        self.handle_selection(shift);
        let (x, y) = renderer.get_cursor_xy(self);
        self.cursor = renderer.get_byte_at_xy(self, x, y - step);
    }

    pub fn move_page_down(&mut self, renderer: &mut Renderer, shift: bool, step: f32) {
        self.handle_selection(shift);
        let (x, y) = renderer.get_cursor_xy(self);
        self.cursor = renderer.get_byte_at_xy(self, x, y + step);
    }
}
