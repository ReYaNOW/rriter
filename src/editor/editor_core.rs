use crate::highlighter::SyncEdit;
use crate::renderer::Renderer;
use imara_diff::{Algorithm, Diff, InternedInput, TokenSource};
use rustc_hash::FxHasher;
use std::collections::VecDeque;
use std::hash::Hasher;

#[path = "../editor_navigation.rs"]
mod navigation;
struct HashSource<'a>(&'a [u64]);

impl<'a> TokenSource for HashSource<'a> {
    type Token = u64;
    type Tokenizer = std::iter::Copied<std::slice::Iter<'a, u64>>;

    fn tokenize(&self) -> Self::Tokenizer {
        self.0.iter().copied()
    }

    fn estimate_tokens(&self) -> u32 {
        self.0.len() as u32
    }
}

#[derive(Clone)]
struct DiffInfo {
    modified: Vec<bool>,
    deleted_gaps: Vec<bool>,
    hunks: Vec<LineDiffHunk>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineDiffHunk {
    pub before_start: usize,
    pub before_end: usize,
    pub after_start: usize,
    pub after_end: usize,
}

fn get_diff_info(old: &[u64], new: &[u64]) -> DiffInfo {
    let m = new.len();
    let mut modified = vec![false; m];
    let mut deleted_gaps = vec![false; m + 1];
    let mut hunks = Vec::new();

    let input = InternedInput::new(HashSource(old), HashSource(new));
    let diff = Diff::compute(Algorithm::Histogram, &input);

    for hunk in diff.hunks() {
        for i in hunk.after.start..hunk.after.end {
            modified[i as usize] = true;
        }
        if !hunk.before.is_empty() {
            deleted_gaps[hunk.after.start as usize] = true;
        }
        hunks.push(LineDiffHunk {
            before_start: hunk.before.start as usize,
            before_end: hunk.before.end as usize,
            after_start: hunk.after.start as usize,
            after_end: hunk.after.end as usize,
        });
    }

    DiffInfo {
        modified,
        deleted_gaps,
        hunks,
    }
}

fn line_hashes_from_text(text: &str) -> Vec<u64> {
    let mut hashes = Vec::with_capacity(1024);
    let mut hasher = FxHasher::default();
    for &b in text.as_bytes() {
        if b == b'\n' {
            hashes.push(hasher.finish());
            hasher = FxHasher::default();
        } else {
            hasher.write_u8(b);
        }
    }
    hashes.push(hasher.finish());
    hashes
}

fn line_hashes_from_slices(first: &str, second: &str) -> Vec<u64> {
    let mut hashes = Vec::with_capacity(1024);
    let mut hasher = FxHasher::default();

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
    Insert {
        offset: usize,
        text: String,
    },
    Delete {
        offset: usize,
        text: String,
    },
    Replace {
        offset: usize,
        old_text: String,
        new_text: String,
    },
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
    Replace(usize, usize, String, String),
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
    pub git_base_text: Option<String>,
    pub git_hunks: Vec<LineDiffHunk>,
    pub line_states: Vec<Option<LineModState>>,
    pub deleted_gaps: Vec<Option<LineModState>>,
    pub is_dirty: bool,

    indent_cache: Vec<u8>,
    last_indent_version: u64,

    pub sync_edits: Vec<SyncEdit>,
    pub foldable_lines: std::collections::HashMap<usize, usize>,
    pub folded_lines: std::collections::HashSet<usize>,
    pub folded_start_bytes: std::collections::HashSet<usize>,
    pub foldable_ranges_bytes: Vec<(usize, usize, bool)>,
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
            git_base_text: None,
            git_hunks: Vec::new(),
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
        for &(start_b, end_b, _) in &self.foldable_ranges_bytes {
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
        let (first, second) = self.text_parts();
        line_hashes_from_slices(first, second)
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
            let clamped_d = raw_d.min(255) as u8;
            if !is_blank {
                self.indent_cache[i] = clamped_d;
            } else {
                if clamped_d > 0 {
                    self.indent_cache[i] = clamped_d;
                } else {
                    self.indent_cache[i] = prev_non_blank[i].min(next_non_blank[i]).min(255) as u8;
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
        self.git_base_text = None;
        self.git_hunks.clear();
        self.update_modifications();
    }

    pub fn set_git_base_text(&mut self, text: Option<String>) {
        if let Some(text) = text {
            self.original_hashes = line_hashes_from_text(&text);
            self.git_base_text = Some(text);
        } else {
            self.original_hashes = self.saved_hashes.clone();
            self.git_base_text = None;
        }
        self.git_hunks.clear();
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

        // Хеш пустой строки — чтобы определить, был ли файл изначально пустым.
        // Если saved-состояние == пустой файл, LCS матчит пустые строки в новом тексте
        // к единственной оригинальной пустой строке, и они ложно выглядят немодифицированными.
        let empty_hash = FxHasher::default().finish();
        let curr_is_empty = curr_hashes.len() == 1 && curr_hashes[0] == empty_hash;

        let saved_was_empty = self.saved_hashes.len() == 1 && self.saved_hashes[0] == empty_hash;
        let saved_info = if saved_was_empty && !curr_is_empty {
            // Весь текущий контент — новый, всё помечаем как unsaved.
            DiffInfo {
                modified: vec![true; curr_hashes.len()],
                deleted_gaps: vec![false; curr_hashes.len() + 1],
                hunks: vec![LineDiffHunk {
                    before_start: 0,
                    before_end: self.saved_hashes.len(),
                    after_start: 0,
                    after_end: curr_hashes.len(),
                }],
            }
        } else {
            get_diff_info(&self.saved_hashes, &curr_hashes)
        };
        let mod_saved = saved_info.modified;
        let mut del_saved = saved_info.deleted_gaps;

        let orig_was_empty =
            self.original_hashes.len() == 1 && self.original_hashes[0] == empty_hash;
        let orig_info = if orig_was_empty && !curr_is_empty {
            DiffInfo {
                modified: vec![true; curr_hashes.len()],
                deleted_gaps: vec![false; curr_hashes.len() + 1],
                hunks: vec![LineDiffHunk {
                    before_start: 0,
                    before_end: self.original_hashes.len(),
                    after_start: 0,
                    after_end: curr_hashes.len(),
                }],
            }
        } else {
            get_diff_info(&self.original_hashes, &curr_hashes)
        };
        let mod_orig = orig_info.modified;
        let mut del_orig = orig_info.deleted_gaps;
        self.git_hunks = if self.git_base_text.is_some() {
            orig_info.hunks
        } else {
            Vec::new()
        };

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

    pub fn git_hunk_index_at_line(&self, line: usize) -> Option<usize> {
        self.git_hunks.iter().position(|hunk| {
            if hunk.after_start == hunk.after_end {
                line == hunk.after_start || line.saturating_add(1) == hunk.after_start
            } else {
                line >= hunk.after_start && line < hunk.after_end
            }
        })
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
                EditOp::Replace {
                    old_text, new_text, ..
                } => old_text.len().max(new_text.len()),
            };
            if size > 0 {
                self.history.push_back(step);
                self.history_size += size;
            }
        }
        // Урезаем лимит памяти на историю: 5 МБ на вкладку (вместо 50 МБ)
        while self.history_size > 5 * 1024 * 1024 {
            if let Some(old) = self.history.pop_front() {
                let old_size = match &old.op {
                    EditOp::Insert { text, .. } => text.len(),
                    EditOp::Delete { text, .. } => text.len(),
                    EditOp::Replace {
                        old_text, new_text, ..
                    } => old_text.len().max(new_text.len()),
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
                EditOp::Replace {
                    offset,
                    old_text,
                    new_text,
                } => {
                    let len = new_text.len();
                    self.shift_folds_delete(*offset, len);
                    self.move_gap(*offset);
                    self.gap_end += len;
                    self.sync_edits.push(SyncEdit::Delete {
                        offset: *offset,
                        len,
                    });

                    let ins_len = old_text.len();
                    self.shift_folds_insert(*offset, ins_len);
                    self.cursor = *offset;
                    self.insert_str_internal(old_text);

                    UndoRedoDelta::Replace(*offset, len, old_text.clone(), new_text.clone())
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
                    UndoRedoDelta::Delete(*offset, len)
                }
                EditOp::Replace {
                    offset,
                    old_text,
                    new_text,
                } => {
                    let len = old_text.len();
                    self.shift_folds_delete(*offset, len);
                    self.move_gap(*offset);
                    self.gap_end += len;
                    self.sync_edits.push(SyncEdit::Delete {
                        offset: *offset,
                        len,
                    });

                    let ins_len = new_text.len();
                    self.shift_folds_insert(*offset, ins_len);
                    self.cursor = *offset;
                    self.insert_str_internal(new_text);

                    UndoRedoDelta::Replace(*offset, len, new_text.clone(), old_text.clone())
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

    pub fn line_text_owned(&self, line: usize) -> String {
        let Some(&start) = self.line_offsets.get(line) else {
            return String::new();
        };
        let end = self
            .line_offsets
            .get(line + 1)
            .copied()
            .unwrap_or_else(|| self.len());
        let mut bytes = Vec::with_capacity(end.saturating_sub(start));
        for i in start..end.min(self.len()) {
            bytes.push(self.byte_at(i));
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }

    pub fn set_text_preserve_history(&mut self, text: &str) {
        let history = std::mem::take(&mut self.history);
        let redo_stack = std::mem::take(&mut self.redo_stack);
        let history_size = self.history_size;
        let original_hashes = self.original_hashes.clone();
        let saved_hashes = self.saved_hashes.clone();
        let git_base_text = self.git_base_text.clone();
        let version = self.version.saturating_add(1);
        let cursor = self.cursor.min(text.len());

        let capacity = text.len() + 8192;
        self.data = vec![0; capacity];
        self.data[..text.len()].copy_from_slice(text.as_bytes());
        self.gap_start = text.len();
        self.gap_end = capacity;
        self.cursor = cursor;
        self.selection_anchor = None;
        self.version = version;
        self.history = history;
        self.redo_stack = redo_stack;
        self.history_size = history_size;
        self.is_working_history = false;
        self.original_hashes = original_hashes;
        self.saved_hashes = saved_hashes;
        self.git_base_text = git_base_text;
        self.sync_edits.clear();
        self.foldable_lines.clear();
        self.folded_lines.clear();
        self.folded_start_bytes.clear();
        self.foldable_ranges_bytes.clear();
        self.indent_cache.clear();
        self.last_indent_version = u64::MAX;
        self.rebuild_line_offsets();
        self.update_modifications();
    }

    pub fn set_clean_text(&mut self, text: &str) {
        let capacity = text.len() + 8192;
        self.data = vec![0; capacity];
        self.data[..text.len()].copy_from_slice(text.as_bytes());
        self.gap_start = text.len();
        self.gap_end = capacity;
        self.cursor = 0;
        self.selection_anchor = None;
        self.history.clear();
        self.redo_stack.clear();
        self.history_size = 0;
        self.is_working_history = false;
        self.sync_edits.clear();
        self.foldable_lines.clear();
        self.folded_lines.clear();
        self.folded_start_bytes.clear();
        self.foldable_ranges_bytes.clear();
        self.indent_cache.clear();
        self.last_indent_version = u64::MAX;
        self.rebuild_line_offsets();
        self.original_hashes = self.get_line_hashes();
        self.saved_hashes = self.original_hashes.clone();
        self.git_base_text = None;
        self.git_hunks.clear();
        let line_count = self.original_hashes.len();
        self.line_states = vec![None; line_count];
        self.deleted_gaps = vec![None; line_count + 1];
        self.is_dirty = false;
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

    pub fn replace_range(
        &mut self,
        start: usize,
        end: usize,
        new_text: &str,
    ) -> (usize, usize, String) {
        self.version += 1;
        let cursor_before_op = self.cursor;

        let len = end - start;
        let mut res = Vec::with_capacity(len);
        for i in start..end {
            res.push(self.byte_at(i));
        }
        let old_text = String::from_utf8_lossy(&res).into_owned();

        if len > 0 {
            self.shift_folds_delete(start, len);
            self.move_gap(start);
            self.gap_end += len;
            self.sync_edits
                .push(SyncEdit::Delete { offset: start, len });
        }

        let ins_len = new_text.len();
        if ins_len > 0 {
            self.cursor = start;
            self.insert_str_internal(new_text);
        }

        self.cursor = start + ins_len;
        self.selection_anchor = None;

        self.push_history(HistoryStep {
            op: EditOp::Replace {
                offset: start,
                old_text: old_text.clone(),
                new_text: new_text.to_string(),
            },
            cursor_before: cursor_before_op,
            cursor_after: self.cursor,
        });

        self.update_modifications();
        (start, len, old_text)
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
        if self.cursor < self.len() {
            let char_before = self.byte_at(self.cursor - 1);
            let char_after = self.byte_at(self.cursor);
            let is_pair = (char_before == b'(' && char_after == b')')
                || (char_before == b'[' && char_after == b']')
                || (char_before == b'{' && char_after == b'}');
            if is_pair {
                return self.backspace();
            }
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
}
