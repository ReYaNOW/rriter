use crate::highlighter::ColorSpan;
use crate::renderer::Renderer;
use std::collections::hash_map::DefaultHasher;
use std::collections::VecDeque;
use std::hash::{Hash, Hasher};

fn hash_lines(s: &str) -> Vec<u64> {
    s.split('\n')
        .map(|line| {
            let mut hasher = DefaultHasher::new();
            line.hash(&mut hasher);
            hasher.finish()
        })
        .collect()
}

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

fn extract_spans_for_range(spans: &[ColorSpan], start: usize, end: usize) -> Vec<ColorSpan> {
    let mut res = Vec::new();
    for s in spans {
        if s.end > start && s.start < end {
            let mut new_s = s.clone();
            new_s.start = new_s.start.max(start) - start;
            new_s.end = new_s.end.min(end) - start;
            if new_s.start < new_s.end {
                res.push(new_s);
            }
        }
    }
    res
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LineModState {
    ModifiedUnsaved,
    ModifiedSaved,
}

#[derive(Clone)]
pub enum EditOp {
    Insert {
        offset: usize,
        text: String,
        spans: Option<Vec<ColorSpan>>,
    },
    Delete {
        offset: usize,
        text: String,
        spans: Vec<ColorSpan>,
    },
}

#[derive(Clone)]
pub struct HistoryStep {
    pub op: EditOp,
    pub cursor_before: usize,
    pub cursor_after: usize,
}

pub enum UndoRedoDelta {
    Insert(usize, usize, String, Option<Vec<ColorSpan>>),
    Delete(usize, usize),
}

pub struct Editor {
    data: Vec<u8>,
    gap_start: usize,
    gap_end: usize,
    pub cursor: usize,
    pub selection_anchor: Option<usize>,
    pub version: u64,

    pub history: VecDeque<HistoryStep>,
    pub redo_stack: VecDeque<HistoryStep>,
    pub history_size: usize,
    pub is_working_history: bool,

    pub original_hashes: Vec<u64>,
    pub saved_hashes: Vec<u64>,
    pub line_states: Vec<Option<LineModState>>,
    pub deleted_gaps: Vec<Option<LineModState>>,
    pub is_dirty: bool,
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
            history: VecDeque::new(),
            redo_stack: VecDeque::new(),
            history_size: 0,
            is_working_history: false,
            original_hashes: vec![],
            saved_hashes: vec![],
            line_states: vec![],
            deleted_gaps: vec![],
            is_dirty: false,
        }
    }

    pub fn set_original_text(&mut self) {
        let full = self.get_full_text();
        self.original_hashes = hash_lines(&full);
        self.saved_hashes = self.original_hashes.clone();
        self.update_modifications();
    }

    pub fn mark_saved(&mut self) {
        let full = self.get_full_text();
        self.saved_hashes = hash_lines(&full);
        self.update_modifications();
    }

    pub fn is_dirty(&self) -> bool {
        self.is_dirty
    }

    pub fn get_line_modification_state(&self, line: usize) -> Option<LineModState> {
        self.line_states.get(line).copied().flatten()
    }

    pub fn update_modifications(&mut self) {
        let full_text = self.get_full_text();
        let curr_hashes = hash_lines(&full_text);

        let (mod_saved, del_saved) = get_diff_info(&self.saved_hashes, &curr_hashes);
        let (mod_orig, del_orig) = get_diff_info(&self.original_hashes, &curr_hashes);

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
                    ..
                },
                EditOp::Insert {
                    offset: new_off,
                    text: new_txt,
                    ..
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
                    spans: last_spans,
                },
                EditOp::Delete {
                    offset: new_off,
                    text: new_txt,
                    spans: new_spans,
                },
            ) = (&mut last.op, &step.op)
            {
                if *new_off + new_txt.len() == *last_off && new_txt.len() < 100 {
                    for s in last_spans.iter_mut() {
                        s.start += new_txt.len();
                        s.end += new_txt.len();
                    }
                    let mut merged_spans = new_spans.clone();
                    merged_spans.append(last_spans);
                    *last_spans = merged_spans;

                    let mut merged = new_txt.clone();
                    merged.push_str(last_txt);
                    *last_txt = merged;
                    *last_off = *new_off;
                    last.cursor_after = step.cursor_after;
                    self.history_size += new_txt.len();
                    merge = true;
                } else if *new_off == *last_off && new_txt.len() < 100 {
                    let mut shifted_new = new_spans.clone();
                    for s in shifted_new.iter_mut() {
                        s.start += last_txt.len();
                        s.end += last_txt.len();
                    }
                    last_spans.append(&mut shifted_new);

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

    pub fn undo(&mut self, current_spans: &[ColorSpan]) -> Option<UndoRedoDelta> {
        if let Some(mut step) = self.history.pop_back() {
            self.is_working_history = true;
            let delta = match &mut step.op {
                EditOp::Insert {
                    offset,
                    text,
                    spans,
                } => {
                    *spans = Some(extract_spans_for_range(
                        current_spans,
                        *offset,
                        *offset + text.len(),
                    ));
                    self.selection_anchor = Some(*offset);
                    self.cursor = *offset + text.len();
                    let len = text.len();
                    let start = *offset;
                    self.move_gap(start);
                    self.gap_end += len;
                    self.cursor = start;
                    self.selection_anchor = None;
                    UndoRedoDelta::Delete(*offset, text.len())
                }
                EditOp::Delete {
                    offset,
                    text,
                    spans,
                } => {
                    self.cursor = *offset;
                    self.selection_anchor = None;
                    self.insert_str_internal(text);
                    UndoRedoDelta::Insert(*offset, text.len(), text.clone(), Some(spans.clone()))
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
                EditOp::Insert {
                    offset,
                    text,
                    spans,
                } => {
                    self.cursor = *offset;
                    self.selection_anchor = None;
                    self.insert_str_internal(text);
                    UndoRedoDelta::Insert(*offset, text.len(), text.clone(), spans.clone())
                }
                EditOp::Delete { offset, text, .. } => {
                    self.selection_anchor = Some(*offset);
                    self.cursor = step.cursor_before;
                    let len = text.len();
                    let start = *offset;
                    self.move_gap(start);
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

    pub fn insert_str(
        &mut self,
        s: &str,
        current_spans: &[ColorSpan],
    ) -> (Option<(usize, usize)>, usize) {
        let cursor_before = self.cursor;
        let del_info = self.delete_selection(current_spans);

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
                spans: None,
            },
            cursor_before,
            cursor_after: self.cursor,
        });

        self.update_modifications();
        (del_info, len)
    }

    pub fn delete_selection(&mut self, current_spans: &[ColorSpan]) -> Option<(usize, usize)> {
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
                let saved_spans = extract_spans_for_range(current_spans, start, end);

                let cursor_before = self.cursor;
                self.move_gap(start);
                self.gap_end += len;
                self.cursor = start;
                self.selection_anchor = None;

                self.push_history(HistoryStep {
                    op: EditOp::Delete {
                        offset: start,
                        text,
                        spans: saved_spans,
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

    pub fn backspace(&mut self, current_spans: &[ColorSpan]) -> Option<(usize, usize)> {
        if let Some(del_info) = self.delete_selection(current_spans) {
            self.update_modifications();
            return Some(del_info);
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
            let saved_spans = extract_spans_for_range(current_spans, prev, self.cursor);

            self.move_gap(self.cursor);
            self.gap_start -= len;
            self.cursor = prev;

            self.push_history(HistoryStep {
                op: EditOp::Delete {
                    offset: prev,
                    text,
                    spans: saved_spans,
                },
                cursor_before,
                cursor_after: self.cursor,
            });
            self.update_modifications();
            return Some((prev, len));
        }
        None
    }

    pub fn delete_forward(&mut self, current_spans: &[ColorSpan]) -> Option<(usize, usize)> {
        if let Some(del_info) = self.delete_selection(current_spans) {
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
            let saved_spans = extract_spans_for_range(current_spans, self.cursor, next);

            let offset = self.cursor;
            self.move_gap(self.cursor);
            self.gap_end += len;

            self.push_history(HistoryStep {
                op: EditOp::Delete {
                    offset,
                    text,
                    spans: saved_spans,
                },
                cursor_before,
                cursor_after: self.cursor,
            });
            self.update_modifications();
            return Some((offset, len));
        }
        None
    }

    pub fn delete_word_backward(&mut self, current_spans: &[ColorSpan]) -> Option<(usize, usize)> {
        if self.selection_anchor.is_some() && self.selection_anchor != Some(self.cursor) {
            return self.delete_selection(current_spans);
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
        self.delete_selection(current_spans)
    }

    pub fn delete_word_forward(&mut self, current_spans: &[ColorSpan]) -> Option<(usize, usize)> {
        if self.selection_anchor.is_some() && self.selection_anchor != Some(self.cursor) {
            return self.delete_selection(current_spans);
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
        self.delete_selection(current_spans)
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

    pub fn move_left(&mut self, shift: bool) {
        self.handle_selection(shift);
        if self.cursor > 0 {
            let mut prev = self.cursor - 1;
            while prev > 0 && !self.is_char_boundary(prev) {
                prev -= 1;
            }
            self.cursor = prev;
        }
    }

    pub fn move_right(&mut self, shift: bool) {
        self.handle_selection(shift);
        if self.cursor < self.len() {
            let mut next = self.cursor + 1;
            while next < self.len() && !self.is_char_boundary(next) {
                next += 1;
            }
            self.cursor = next;
        }
    }

    pub fn move_home(&mut self, shift: bool) {
        self.handle_selection(shift);
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
