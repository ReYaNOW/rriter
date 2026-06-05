use crate::app::git_panel::{GitFileEntry, GitFileStatus};
use crate::app::{App, EditorTab, EditorTabKind, InlineGitPopup, InlineGitPopupLine};
use crate::editor::{Editor, LineDiffHunk};
use crate::highlighter::{ColorSpan, Highlighter};
use imara_diff::{Algorithm, Diff, InternedInput, TokenSource};
use rustc_hash::FxHasher;
use std::hash::Hasher;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

pub(crate) const GIT_DIFF_FOCUS_RATIO: f32 = 0.38;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitDiffTabMeta {
    pub repo_root: PathBuf,
    pub rel_path: String,
    pub old_rel_path: Option<String>,
    pub status: GitFileStatus,
    pub workspace_idx: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffLineKind {
    Context,
    Added,
    Deleted,
    ModifiedOld,
    ModifiedNew,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffHunk {
    pub display_start: usize,
    pub display_end: usize,
    pub display_start_line: usize,
    pub before_line_start: usize,
    pub before_line_end: usize,
    pub after_line_start: usize,
    pub after_line_end: usize,
    pub old_text: String,
    pub new_text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitDiffState {
    pub base_text: String,
    pub worktree_text: String,
    pub displayed_text: String,
    pub line_kinds: Vec<DiffLineKind>,
    pub hunks: Vec<DiffHunk>,
    pub loading: bool,
    pub error: Option<String>,
    pub version: u64,
    pub current_hunk_idx: Option<usize>,
    pub undo_extract_line_kinds: Option<Vec<DiffLineKind>>,
    pub redo_extract_line_kinds: Option<Vec<DiffLineKind>>,
}

impl GitDiffState {
    pub fn loading(version: u64) -> Self {
        Self {
            base_text: String::new(),
            worktree_text: String::new(),
            displayed_text: "Loading diff...\n".to_string(),
            line_kinds: vec![DiffLineKind::Context],
            hunks: Vec::new(),
            loading: true,
            error: None,
            version,
            current_hunk_idx: None,
            undo_extract_line_kinds: None,
            redo_extract_line_kinds: None,
        }
    }

    pub fn error(message: String, version: u64) -> Self {
        let displayed_text = format!("Diff error: {message}\n");
        Self {
            base_text: String::new(),
            worktree_text: String::new(),
            displayed_text,
            line_kinds: vec![DiffLineKind::Context],
            hunks: Vec::new(),
            loading: false,
            error: Some(message),
            version,
            current_hunk_idx: None,
            undo_extract_line_kinds: None,
            redo_extract_line_kinds: None,
        }
    }

    pub fn rollback_hunk_index_at_line(&self, phys_line: usize) -> Option<usize> {
        self.hunks
            .iter()
            .position(|hunk| hunk.display_start_line == phys_line)
    }

    pub fn first_changed_line(&self) -> Option<usize> {
        self.hunks.first().map(|hunk| hunk.display_start_line)
    }
}

#[derive(Clone, Debug)]
pub struct GitDiffPayload {
    pub base_text: String,
    pub worktree_text: String,
}

#[derive(Clone, Debug)]
pub struct GitDiffEvent {
    pub meta: GitDiffTabMeta,
    pub result: Result<GitDiffPayload, String>,
    pub version: u64,
}

pub struct InlineGitDiffEvent {
    pub hunk_idx: usize,
    pub target_hunk: LineDiffHunk,
    pub anchor_line: usize,
    pub editor_version: u64,
    pub result: Result<InlineGitDiffPayload, String>,
}

pub struct InlineGitDiffPayload {
    pub state: GitDiffState,
    pub spans: Vec<ColorSpan>,
}

#[derive(Clone, Copy)]
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

#[derive(Clone, Debug)]
struct LineSpan {
    text: String,
    start: usize,
    end: usize,
}

fn split_lines_preserve(text: &str) -> Vec<LineSpan> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mut start = 0usize;
    for (idx, ch) in text.char_indices() {
        if ch == '\n' {
            let end = idx + 1;
            lines.push(LineSpan {
                text: text[start..end].to_string(),
                start,
                end,
            });
            start = end;
        }
    }
    if start < text.len() {
        lines.push(LineSpan {
            text: text[start..].to_string(),
            start,
            end: text.len(),
        });
    }
    lines
}

fn hash_line(line: &str) -> u64 {
    let mut hasher = FxHasher::default();
    hasher.write(line.as_bytes());
    hasher.finish()
}

fn push_line(out: &mut String, kinds: &mut Vec<DiffLineKind>, line: &LineSpan, kind: DiffLineKind) {
    out.push_str(&line.text);
    kinds.push(kind);
}

fn range_text(lines: &[LineSpan], start: usize, end: usize) -> String {
    let mut text = String::new();
    for line in &lines[start.min(lines.len())..end.min(lines.len())] {
        text.push_str(&line.text);
    }
    text
}

fn line_byte_bounds(
    lines: &[LineSpan],
    start: usize,
    end: usize,
    text_len: usize,
) -> (usize, usize) {
    let start_byte = lines.get(start).map(|line| line.start).unwrap_or(text_len);
    let end_byte = lines
        .get(end)
        .map(|line| line.start)
        .or_else(|| lines.get(end.saturating_sub(1)).map(|line| line.end))
        .unwrap_or(start_byte);
    (start_byte, end_byte)
}

pub fn build_diff_view(base_text: String, worktree_text: String) -> GitDiffState {
    let before_lines = split_lines_preserve(&base_text);
    let after_lines = split_lines_preserve(&worktree_text);
    let before_hashes = before_lines
        .iter()
        .map(|line| hash_line(&line.text))
        .collect::<Vec<_>>();
    let after_hashes = after_lines
        .iter()
        .map(|line| hash_line(&line.text))
        .collect::<Vec<_>>();

    let input = InternedInput::new(HashSource(&before_hashes), HashSource(&after_hashes));
    let diff = Diff::compute(Algorithm::Histogram, &input);

    let mut displayed_text = String::with_capacity(base_text.len().max(worktree_text.len()));
    let mut line_kinds = Vec::with_capacity(before_lines.len().max(after_lines.len()));
    let mut hunks = Vec::new();
    let mut before_pos = 0usize;
    let mut after_pos = 0usize;

    for hunk in diff.hunks() {
        let before_start = hunk.before.start as usize;
        let before_end = hunk.before.end as usize;
        let after_start = hunk.after.start as usize;
        let after_end = hunk.after.end as usize;

        while before_pos < before_start && after_pos < after_start {
            push_line(
                &mut displayed_text,
                &mut line_kinds,
                &after_lines[after_pos],
                DiffLineKind::Context,
            );
            before_pos += 1;
            after_pos += 1;
        }

        let display_start = displayed_text.len();
        let display_start_line = line_kinds.len();
        let old_text = range_text(&before_lines, before_start, before_end);
        let new_text = range_text(&after_lines, after_start, after_end);
        let old_count = before_end.saturating_sub(before_start);
        let new_count = after_end.saturating_sub(after_start);
        let old_kind = if new_count > 0 {
            DiffLineKind::ModifiedOld
        } else {
            DiffLineKind::Deleted
        };
        let new_kind = if old_count > 0 {
            DiffLineKind::ModifiedNew
        } else {
            DiffLineKind::Added
        };

        for line in &before_lines[before_start..before_end] {
            push_line(&mut displayed_text, &mut line_kinds, line, old_kind);
        }
        for line in &after_lines[after_start..after_end] {
            push_line(&mut displayed_text, &mut line_kinds, line, new_kind);
        }

        hunks.push(DiffHunk {
            display_start,
            display_end: displayed_text.len(),
            display_start_line,
            before_line_start: before_start,
            before_line_end: before_end,
            after_line_start: after_start,
            after_line_end: after_end,
            old_text,
            new_text,
        });

        before_pos = before_end;
        after_pos = after_end;
    }

    while after_pos < after_lines.len() {
        push_line(
            &mut displayed_text,
            &mut line_kinds,
            &after_lines[after_pos],
            DiffLineKind::Context,
        );
        after_pos += 1;
    }

    GitDiffState {
        base_text,
        worktree_text,
        displayed_text,
        line_kinds,
        current_hunk_idx: (!hunks.is_empty()).then_some(0),
        hunks,
        loading: false,
        error: None,
        version: 0,
        undo_extract_line_kinds: None,
        redo_extract_line_kinds: None,
    }
}

pub fn extract_worktree_text(displayed_text: &str, line_kinds: &[DiffLineKind]) -> String {
    let lines = split_lines_preserve(displayed_text);
    let mut out = String::with_capacity(displayed_text.len());
    for (idx, line) in lines.iter().enumerate() {
        let kind = line_kinds
            .get(idx)
            .copied()
            .unwrap_or(DiffLineKind::Context);
        if !matches!(kind, DiffLineKind::Deleted | DiffLineKind::ModifiedOld) {
            out.push_str(&line.text);
        }
    }
    out
}

pub fn rollback_hunk_text(current_new_text: &str, hunk: &DiffHunk) -> String {
    let after_lines = split_lines_preserve(current_new_text);
    let start = hunk.after_line_start.min(after_lines.len());
    let end = hunk.after_line_end.min(after_lines.len());
    let start_byte = after_lines
        .get(start)
        .map(|line| line.start)
        .unwrap_or(current_new_text.len());
    let end_byte = after_lines
        .get(end)
        .map(|line| line.start)
        .or_else(|| after_lines.get(end.saturating_sub(1)).map(|line| line.end))
        .unwrap_or(start_byte);
    let mut next = String::with_capacity(
        current_new_text
            .len()
            .saturating_sub(end_byte.saturating_sub(start_byte))
            + hunk.old_text.len(),
    );
    next.push_str(&current_new_text[..start_byte]);
    next.push_str(&hunk.old_text);
    next.push_str(&current_new_text[end_byte..]);
    next
}

fn read_head_blob(repo: &git2::Repository, rel_path: &str) -> Result<String, String> {
    let head = match repo.head() {
        Ok(head) => head,
        Err(_) => return Ok(String::new()),
    };
    let tree = head
        .peel_to_tree()
        .map_err(|err| format!("HEAD tree: {}", err.message()))?;
    let entry = match tree.get_path(Path::new(rel_path)) {
        Ok(entry) => entry,
        Err(_) => return Ok(String::new()),
    };
    let object = entry
        .to_object(repo)
        .map_err(|err| format!("HEAD object: {}", err.message()))?;
    let blob = object
        .peel_to_blob()
        .map_err(|err| format!("HEAD blob: {}", err.message()))?;
    Ok(String::from_utf8_lossy(blob.content()).into_owned())
}

pub(crate) fn load_head_text_for_worktree_path(abs_path: &Path) -> Option<String> {
    let repo = git2::Repository::discover(abs_path).ok()?;
    let repo_root = repo.workdir()?.to_path_buf();
    let rel_path = abs_path.strip_prefix(&repo_root).ok()?;
    let rel_path = rel_path.to_string_lossy();
    read_head_blob(&repo, &rel_path).ok()
}

fn read_index_blob(repo: &git2::Repository, rel_path: &str) -> Result<Option<String>, String> {
    let index = repo.index().map_err(|err| err.message().to_string())?;
    let Some(entry) = index.get_path(Path::new(rel_path), 0) else {
        return Ok(None);
    };
    let blob = repo
        .find_blob(entry.id)
        .map_err(|err| format!("index blob: {}", err.message()))?;
    Ok(Some(String::from_utf8_lossy(blob.content()).into_owned()))
}

fn read_worktree_or_index(
    repo: &git2::Repository,
    repo_root: &Path,
    rel_path: &str,
) -> Result<String, String> {
    let path = repo_root.join(rel_path);
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(_) => Ok(read_index_blob(repo, rel_path)?.unwrap_or_default()),
    }
}

pub fn load_git_diff(
    repo_root: PathBuf,
    rel_path: String,
    old_rel_path: Option<String>,
    status: GitFileStatus,
) -> Result<GitDiffPayload, String> {
    load_git_diff_with_side(repo_root, rel_path, old_rel_path, status, false)
}

fn load_git_diff_with_side(
    repo_root: PathBuf,
    rel_path: String,
    old_rel_path: Option<String>,
    status: GitFileStatus,
    staged: bool,
) -> Result<GitDiffPayload, String> {
    let repo = git2::Repository::open(&repo_root).map_err(|err| err.message().to_string())?;
    let old_path = old_rel_path.as_deref().unwrap_or(rel_path.as_str());
    let base_text = if matches!(status, GitFileStatus::Added | GitFileStatus::Untracked) {
        String::new()
    } else {
        read_head_blob(&repo, old_path)?
    };
    let worktree_text = if matches!(status, GitFileStatus::Deleted) {
        String::new()
    } else if staged {
        read_index_blob(&repo, &rel_path)?.unwrap_or_default()
    } else {
        read_worktree_or_index(&repo, &repo_root, &rel_path)?
    };
    Ok(GitDiffPayload {
        base_text,
        worktree_text,
    })
}

fn build_inline_git_diff_payload(
    payload: GitDiffPayload,
    file_extension: String,
    priority_anchor: usize,
) -> InlineGitDiffPayload {
    let state = build_diff_view(payload.base_text, payload.worktree_text);
    let mut highlighter = Highlighter::new();
    let version = 1;
    highlighter.reset(
        version,
        state.displayed_text.clone(),
        file_extension,
        priority_anchor.min(state.displayed_text.len()),
    );
    let _ = highlighter.wait_for_first_result(version, std::time::Duration::from_millis(150));
    InlineGitDiffPayload {
        state,
        spans: highlighter.spans,
    }
}

fn file_name_for_diff_title(rel_path: &str) -> String {
    Path::new(rel_path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| rel_path.to_string())
}

fn new_editor_with_text(text: &str, version: u64) -> Editor {
    let mut editor = Editor::new(text.len() + 8192);
    editor.version = version;
    if !text.is_empty() {
        let _ = editor.insert_str(text);
        editor.cursor = 0;
        editor.clear_history();
    }
    editor.set_original_text();
    editor.sync_edits.clear();
    editor
}

impl App {
    fn current_git_file_entry_for_diff(&self) -> Option<(PathBuf, GitFileEntry)> {
        if !self.is_ide_mode || self.active_tab_is_git_diff() {
            return None;
        }
        let path = self.file_path.as_ref()?;
        let abs_path = self.abs_path_for_workspace(path);
        self.ide_panel
            .git
            .snapshot
            .workspaces
            .iter()
            .find_map(|workspace| {
                let repo_root = workspace.repo_root.as_ref()?;
                workspace
                    .files
                    .iter()
                    .find(|file| repo_root.join(file.rel_path.as_ref()) == abs_path)
                    .cloned()
                    .map(|file| (repo_root.clone(), file))
            })
    }

    pub(crate) fn current_file_git_base_text(&self) -> Option<String> {
        if !self.is_ide_mode || self.active_tab_is_git_diff() {
            return None;
        }
        let path = self.file_path.as_ref()?;
        let abs_path = self.abs_path_for_workspace(path);
        if !self
            .ide_workspaces
            .iter()
            .any(|workspace| abs_path.starts_with(workspace))
        {
            return None;
        }
        load_head_text_for_worktree_path(&abs_path)
    }

    pub(crate) fn refresh_current_editor_git_base(&mut self) {
        let base_text = self.current_file_git_base_text();
        self.editor.set_git_base_text(base_text);
        self.inline_git_popup = None;
    }

    pub fn active_tab_is_git_diff(&self) -> bool {
        self.tabs
            .get(self.active_tab)
            .is_some_and(|tab| tab.kind.is_git_diff())
    }

    pub fn active_git_diff_state(&self) -> Option<&GitDiffState> {
        match &self.tabs.get(self.active_tab)?.kind {
            EditorTabKind::GitDiff(_, state) => Some(state),
            EditorTabKind::Normal | EditorTabKind::ApiClient(_, _) => None,
        }
    }

    fn active_git_diff_state_mut(&mut self) -> Option<&mut GitDiffState> {
        match &mut self.tabs.get_mut(self.active_tab)?.kind {
            EditorTabKind::GitDiff(_, state) => Some(state),
            EditorTabKind::Normal | EditorTabKind::ApiClient(_, _) => None,
        }
    }

    fn git_file_entry(
        &self,
        workspace_idx: usize,
        file_idx: usize,
    ) -> Option<(PathBuf, GitFileEntry)> {
        self.ide_panel
            .git
            .snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_idx == workspace_idx)
            .and_then(|workspace| {
                let repo_root = workspace.repo_root.clone()?;
                workspace
                    .files
                    .get(file_idx)
                    .cloned()
                    .map(|file| (repo_root, file))
            })
    }

    pub fn open_git_diff_tab(&mut self, workspace_idx: usize, file_idx: usize) {
        let Some((repo_root, file)) = self.git_file_entry(workspace_idx, file_idx) else {
            return;
        };
        let meta = GitDiffTabMeta {
            repo_root,
            rel_path: file.rel_path.to_string(),
            old_rel_path: file.old_rel_path.as_ref().map(ToString::to_string),
            status: file.status,
            workspace_idx,
        };

        if let Some(idx) = self.tabs.iter().position(|tab| match &tab.kind {
            EditorTabKind::GitDiff(existing, _) => {
                existing.repo_root == meta.repo_root
                    && existing.rel_path == meta.rel_path
                    && existing.old_rel_path == meta.old_rel_path
            }
            EditorTabKind::Normal | EditorTabKind::ApiClient(_, _) => false,
        }) {
            if idx != self.active_tab {
                self.switch_to_tab(idx);
            }
            return;
        }

        if self.is_ide_mode && !self.tabs.is_empty() {
            self.sync_active_tab();
        }

        let version = self.next_tab_highlight_version();
        let state = GitDiffState::loading(version);
        let title = format!(
            "Diff: {}",
            file_name_for_diff_title(file.rel_path.as_ref())
        );
        let tab = EditorTab {
            editor: new_editor_with_text(&state.displayed_text, version),
            file_path: None,
            base_title: title,
            file_extension: Path::new(file.rel_path.as_ref())
                .extension()
                .map(|ext| ext.to_string_lossy().into_owned())
                .unwrap_or_default(),
            scroll_y: crate::scroll::ScrollState::new(15.0),
            scroll_x: crate::scroll::ScrollState::new(15.0),
            spans: Vec::new(),
            completions: Vec::new(),
            foldable_ranges: Vec::new(),
            syntax_errors: Vec::new(),
            last_sent_version: u64::MAX,
            search_results: Vec::new(),
            search_current_idx: None,
            is_highlighted_once: true,
            is_highlight_complete: true,
            icon_key: "default_file",
            kind: EditorTabKind::GitDiff(meta.clone(), state),
        };
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        self.sync_active_tab();
        self.show_welcome = false;
        self.highlighter
            .reset(self.editor.version, String::new(), String::new(), 0);
        self.spawn_git_diff_load(meta, version, file.staged);
        self.reveal_active_tab_now();
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    fn spawn_git_diff_load(&mut self, meta: GitDiffTabMeta, version: u64, staged: bool) {
        let (tx, rx) = mpsc::channel();
        self.git_diff_rx.push(rx);
        std::thread::spawn(move || {
            let result = if staged {
                load_git_diff_with_side(
                    meta.repo_root.clone(),
                    meta.rel_path.clone(),
                    meta.old_rel_path.clone(),
                    meta.status,
                    true,
                )
            } else {
                load_git_diff(
                    meta.repo_root.clone(),
                    meta.rel_path.clone(),
                    meta.old_rel_path.clone(),
                    meta.status,
                )
            };
            let _ = tx.send(GitDiffEvent {
                meta,
                result,
                version,
            });
        });
    }

    pub(crate) fn reload_git_diff_tab(&mut self, tab_idx: usize) {
        let Some((meta, current_version, staged)) = self.tabs.get(tab_idx).and_then(|tab| {
            if tab.editor.is_dirty() {
                return None;
            }
            let EditorTabKind::GitDiff(meta, state) = &tab.kind else {
                return None;
            };
            let staged = self
                .ide_panel
                .git
                .snapshot
                .workspaces
                .iter()
                .find(|workspace| workspace.workspace_idx == meta.workspace_idx)
                .and_then(|workspace| {
                    let repo_root = workspace.repo_root.as_ref()?;
                    if repo_root != &meta.repo_root {
                        return None;
                    }
                    workspace.files.iter().find(|file| {
                        file.rel_path.as_ref() == meta.rel_path.as_str()
                            && file.old_rel_path.as_deref() == meta.old_rel_path.as_deref()
                    })
                })
                .map(|file| file.staged)
                .unwrap_or(false);
            Some((meta.clone(), state.version, staged))
        }) else {
            return;
        };
        let version = current_version.saturating_add(1);
        if let Some(tab) = self.tabs.get_mut(tab_idx)
            && let EditorTabKind::GitDiff(existing, state) = &mut tab.kind
        {
            *existing = meta.clone();
            state.version = version;
            state.loading = true;
            state.error = None;
        }
        self.spawn_git_diff_load(meta, version, staged);
    }

    pub fn poll_git_diff_tabs(&mut self) -> bool {
        let mut updated = false;
        let mut next_rx = Vec::with_capacity(self.git_diff_rx.len());
        let receivers = std::mem::take(&mut self.git_diff_rx);
        for rx in receivers {
            match rx.try_recv() {
                Ok(event) => {
                    self.apply_git_diff_event(event);
                    updated = true;
                }
                Err(mpsc::TryRecvError::Empty) => next_rx.push(rx),
                Err(mpsc::TryRecvError::Disconnected) => {}
            }
        }
        self.git_diff_rx = next_rx;
        updated
    }

    pub fn poll_inline_git_diff_popup(&mut self) -> bool {
        let Some(rx) = self.inline_git_diff_rx.take() else {
            return false;
        };
        match rx.try_recv() {
            Ok(event) => {
                if event.editor_version == self.editor.version {
                    if let Ok(payload) = event.result {
                        self.set_inline_git_popup_from_diff_state(
                            event.hunk_idx,
                            event.target_hunk,
                            event.anchor_line,
                            &payload.state,
                            payload.spans,
                        );
                    } else {
                        self.inline_git_popup = None;
                    }
                }
                true
            }
            Err(mpsc::TryRecvError::Empty) => {
                self.inline_git_diff_rx = Some(rx);
                true
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.inline_git_popup = None;
                true
            }
        }
    }

    fn apply_git_diff_event(&mut self, event: GitDiffEvent) {
        let Some(tab_idx) = self.tabs.iter().position(|tab| match &tab.kind {
            EditorTabKind::GitDiff(meta, state) => {
                meta.repo_root == event.meta.repo_root
                    && meta.rel_path == event.meta.rel_path
                    && meta.old_rel_path == event.meta.old_rel_path
                    && state.version == event.version
            }
            EditorTabKind::Normal | EditorTabKind::ApiClient(_, _) => false,
        }) else {
            return;
        };
        let mut state = match event.result {
            Ok(payload) => build_diff_view(payload.base_text, payload.worktree_text),
            Err(err) => GitDiffState::error(err, event.version),
        };
        state.version = event.version;
        let text = state.displayed_text.clone();
        if let EditorTabKind::GitDiff(_, tab_state) = &mut self.tabs[tab_idx].kind {
            *tab_state = state;
        }
        if tab_idx == self.active_tab {
            self.editor.set_text_preserve_history(&text);
            self.editor.clear_history();
            self.editor.set_original_text();
            self.refresh_active_git_diff_highlight();
            self.scroll_active_git_diff_to_first_change();
            self.scroll_x.current = 0.0;
            self.scroll_x.target = 0.0;
        } else {
            self.tabs[tab_idx].editor = new_editor_with_text(&text, event.version);
            self.tabs[tab_idx].spans.clear();
            self.tabs[tab_idx].is_highlighted_once = false;
            self.tabs[tab_idx].is_highlight_complete = false;
        }
    }

    pub fn prepare_active_git_diff_highlight_after_switch(&mut self) {
        if !self.active_tab_is_git_diff() {
            return;
        }
        self.refresh_active_git_diff_highlight();
    }

    fn refresh_active_git_diff_highlight(&mut self) {
        self.editor.foldable_lines.clear();
        self.editor.folded_lines.clear();
        self.editor.folded_start_bytes.clear();
        self.editor.foldable_ranges_bytes.clear();
        self.highlighter.spans.clear();
        self.highlighter.completions.clear();
        self.highlighter.foldable_ranges.clear();
        self.highlighter.syntax_errors.clear();
        while let Ok(_) = self.highlighter.rx.try_recv() {}
        self.highlighter.reset(
            self.editor.version,
            self.editor.get_full_text(),
            self.file_extension.clone(),
            self.editor.cursor,
        );
        let _ = self
            .highlighter
            .wait_for_first_result(self.editor.version, std::time::Duration::from_millis(150));
        self.editor.foldable_lines.clear();
        self.editor.folded_lines.clear();
        self.editor.folded_start_bytes.clear();
        self.editor.foldable_ranges_bytes.clear();
        self.highlighter.foldable_ranges.clear();
        self.is_highlighted_once = true;
        self.is_highlight_complete = true;
    }

    pub fn scroll_active_git_diff_to_first_change(&mut self) {
        let Some(line) = self
            .active_git_diff_state()
            .and_then(GitDiffState::first_changed_line)
        else {
            self.scroll_y.current = 0.0;
            self.scroll_y.target = 0.0;
            return;
        };
        self.set_active_git_diff_current_hunk_for_line(line);
        self.scroll_active_git_diff_to_line(line);
    }

    pub fn scroll_active_git_diff_to_line(&mut self, line: usize) {
        let line_height = self
            .renderer
            .as_ref()
            .map(|r| r.line_height)
            .unwrap_or(20.0);
        let visible_h = self
            .renderer
            .as_ref()
            .map(|r| {
                let s = r.scale_factor;
                let tab_bar_h = if self.show_welcome || !self.is_ide_mode {
                    0.0
                } else {
                    38.0 * s
                };
                let panel_bottom_h = if self.is_ide_mode {
                    self.ide_panel.editor_reserved_bottom_height(s)
                } else {
                    0.0
                };
                crate::render_view::editor_view_height(
                    r.height,
                    tab_bar_h,
                    panel_bottom_h,
                    self.is_ide_mode,
                    s,
                )
            })
            .unwrap_or(line_height * 12.0);
        let line_y = self.editor_visual_y_for_line(line, line_height);
        let target = (line_y - visible_h * GIT_DIFF_FOCUS_RATIO).max(0.0).round();
        self.scroll_y.current = target;
        self.scroll_y.target = target;
    }

    fn animate_active_git_diff_to_line(&mut self, line: usize) {
        let line_height = self
            .renderer
            .as_ref()
            .map(|r| r.line_height)
            .unwrap_or(20.0);
        let Some((renderer_h, s)) = self.renderer.as_ref().map(|r| (r.height, r.scale_factor))
        else {
            self.scroll_active_git_diff_to_line(line);
            return;
        };
        let tab_bar_h = if self.show_welcome || !self.is_ide_mode {
            0.0
        } else {
            38.0 * s
        };
        let panel_bottom_h = if self.is_ide_mode {
            self.ide_panel.editor_reserved_bottom_height(s)
        } else {
            0.0
        };
        let visible_h = crate::render_view::editor_view_height(
            renderer_h,
            tab_bar_h,
            panel_bottom_h,
            self.is_ide_mode,
            s,
        );
        let line_y = self.editor_visual_y_for_line(line, line_height);
        let target = (line_y - visible_h * GIT_DIFF_FOCUS_RATIO).max(0.0).round();
        let max_s = self
            .renderer
            .as_mut()
            .map(|r| r.get_max_scroll(&self.editor, visible_h))
            .unwrap_or(target);
        let target = target.clamp(0.0, max_s).round();
        self.scroll_y.target = target;
        self.scroll_y.anim_speed = 10.0;
    }

    fn animate_editor_to_line(&mut self, line: usize) {
        let line_height = self
            .renderer
            .as_ref()
            .map(|r| r.line_height)
            .unwrap_or(20.0);
        let Some((renderer_h, s)) = self.renderer.as_ref().map(|r| (r.height, r.scale_factor))
        else {
            self.scroll_y.current = line as f32 * line_height;
            self.scroll_y.target = self.scroll_y.current;
            return;
        };
        let tab_bar_h = if self.show_welcome || !self.is_ide_mode {
            0.0
        } else {
            38.0 * s
        };
        let panel_bottom_h = if self.is_ide_mode {
            self.ide_panel.editor_reserved_bottom_height(s)
        } else {
            0.0
        };
        let visible_h = crate::render_view::editor_view_height(
            renderer_h,
            tab_bar_h,
            panel_bottom_h,
            self.is_ide_mode,
            s,
        );
        let line_y = self.editor_visual_y_for_line(line, line_height);
        let target = (line_y - visible_h * GIT_DIFF_FOCUS_RATIO).max(0.0).round();
        let max_s = self
            .renderer
            .as_mut()
            .map(|r| r.get_max_scroll(&self.editor, visible_h))
            .unwrap_or(target);
        let target = target.clamp(0.0, max_s).round();
        self.scroll_y.target = target;
        self.scroll_y.anim_speed = 10.0;
    }

    fn editor_visual_y_for_line(&self, target_line: usize, line_height: f32) -> f32 {
        let mut y = 0.0;
        let mut phys_line = 0usize;
        while phys_line < self.editor.line_offsets.len() {
            if phys_line >= target_line {
                return y;
            }
            let is_folded = self.editor.folded_lines.contains(&phys_line)
                && self.editor.foldable_lines.contains_key(&phys_line);
            if is_folded
                && let Some(&fold_end) = self.editor.foldable_lines.get(&phys_line)
                && target_line <= fold_end
            {
                return y;
            }
            y += line_height;
            if is_folded && let Some(&fold_end) = self.editor.foldable_lines.get(&phys_line) {
                phys_line = fold_end;
            }
            phys_line += 1;
        }
        y
    }

    fn inline_git_anchor_line(&self, line: usize) -> usize {
        let mut anchor = line.min(self.editor.line_offsets.len().saturating_sub(1));
        for &fold_start in &self.editor.folded_lines {
            let Some(&fold_end) = self.editor.foldable_lines.get(&fold_start) else {
                continue;
            };
            if anchor > fold_start && anchor <= fold_end {
                anchor = fold_start;
                break;
            }
        }
        anchor
    }

    fn inline_diff_hunk_index_for_target(
        state: &GitDiffState,
        target_hunk: LineDiffHunk,
        fallback_idx: usize,
    ) -> Option<usize> {
        state
            .hunks
            .iter()
            .position(|hunk| {
                hunk.before_line_start == target_hunk.before_start
                    && hunk.before_line_end == target_hunk.before_end
                    && hunk.after_line_start == target_hunk.after_start
                    && hunk.after_line_end == target_hunk.after_end
            })
            .or_else(|| {
                state.hunks.iter().position(|hunk| {
                    hunk.after_line_start == target_hunk.after_start
                        && hunk.after_line_end == target_hunk.after_end
                })
            })
            .or_else(|| (fallback_idx < state.hunks.len()).then_some(fallback_idx))
    }

    fn set_inline_git_popup_from_diff_state(
        &mut self,
        hunk_idx: usize,
        target_hunk: LineDiffHunk,
        anchor_line: usize,
        diff_state: &GitDiffState,
        spans: Vec<ColorSpan>,
    ) {
        let Some(diff_idx) =
            Self::inline_diff_hunk_index_for_target(diff_state, target_hunk, hunk_idx)
        else {
            self.inline_git_popup = None;
            return;
        };
        let Some(diff_hunk) = diff_state.hunks.get(diff_idx) else {
            self.inline_git_popup = None;
            return;
        };
        let displayed_lines = split_lines_preserve(&diff_state.displayed_text);
        let display_end_line = displayed_lines
            .partition_point(|line| line.start < diff_hunk.display_end)
            .max(diff_hunk.display_start_line);
        let mut lines =
            Vec::with_capacity(display_end_line.saturating_sub(diff_hunk.display_start_line));
        for display_line in diff_hunk.display_start_line..display_end_line {
            let Some(kind) = diff_state.line_kinds.get(display_line).copied() else {
                continue;
            };
            let text = displayed_lines
                .get(display_line)
                .map(|line| {
                    line.text
                        .trim_end_matches(|c| c == '\r' || c == '\n')
                        .to_string()
                })
                .unwrap_or_default();
            lines.push(InlineGitPopupLine {
                text,
                kind,
                display_start: displayed_lines
                    .get(display_line)
                    .map(|line| line.start)
                    .unwrap_or(0),
            });
        }
        if lines.is_empty() {
            self.inline_git_popup = None;
            return;
        }
        self.inline_git_popup = Some(InlineGitPopup {
            hunk_idx,
            anchor_line,
            lines,
            spans,
            diff_state: diff_state.clone(),
        });
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub fn show_inline_git_hunk_popup(&mut self, hunk_idx: usize, anchor_line: usize) {
        let Some(target_hunk) = self.editor.git_hunks.get(hunk_idx).copied() else {
            self.inline_git_diff_rx = None;
            self.inline_git_popup = None;
            return;
        };
        let anchor_line = self.inline_git_anchor_line(anchor_line);

        if let Some(base_text) = self.editor.git_base_text.clone() {
            let current_text = self.editor.get_full_text();
            let payload = build_inline_git_diff_payload(
                GitDiffPayload {
                    base_text,
                    worktree_text: current_text,
                },
                self.file_extension.clone(),
                target_hunk.after_start,
            );
            self.set_inline_git_popup_from_diff_state(
                hunk_idx,
                target_hunk,
                anchor_line,
                &payload.state,
                payload.spans,
            );
            return;
        }

        if let Some((repo_root, file)) = self.current_git_file_entry_for_diff() {
            let (tx, rx) = mpsc::channel();
            self.inline_git_diff_rx = Some(rx);
            let editor_version = self.editor.version;
            let file_extension = self.file_extension.clone();
            std::thread::spawn(move || {
                let result = load_git_diff_with_side(
                    repo_root,
                    file.rel_path.into(),
                    file.old_rel_path.map(Into::into),
                    file.status,
                    file.staged,
                )
                .map(|payload| {
                    build_inline_git_diff_payload(payload, file_extension, target_hunk.after_start)
                });
                let _ = tx.send(InlineGitDiffEvent {
                    hunk_idx,
                    target_hunk,
                    anchor_line,
                    editor_version,
                    result,
                });
            });
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            return;
        }

        self.inline_git_diff_rx = None;
        self.inline_git_popup = None;
    }

    pub fn jump_inline_git_hunk(&mut self, direction: isize) {
        if self.editor.git_hunks.is_empty() {
            self.inline_git_popup = None;
            return;
        }
        if self.editor.git_hunks.len() == 1 {
            let anchor_line = self.inline_git_anchor_line(self.editor.git_hunks[0].after_start + 1);
            self.show_inline_git_hunk_popup(0, anchor_line);
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            return;
        }
        let current = self
            .inline_git_popup
            .as_ref()
            .map(|popup| popup.hunk_idx)
            .unwrap_or(0);
        let target = if direction > 0 {
            (current + 1) % self.editor.git_hunks.len()
        } else if current == 0 {
            self.editor.git_hunks.len() - 1
        } else {
            current - 1
        };
        let target_line = self.editor.git_hunks[target].after_start;
        let anchor_line = self.inline_git_anchor_line(target_line + 1);
        if let (Some((diff_state, spans)), Some(target_hunk)) = (
            self.inline_git_popup
                .as_ref()
                .map(|popup| (popup.diff_state.clone(), popup.spans.clone())),
            self.editor.git_hunks.get(target).copied(),
        ) {
            self.set_inline_git_popup_from_diff_state(
                target,
                target_hunk,
                anchor_line,
                &diff_state,
                spans,
            );
        } else {
            self.show_inline_git_hunk_popup(target, anchor_line);
        }
        self.animate_editor_to_line(target_line);
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub fn rollback_inline_git_hunk(&mut self) {
        let Some(hunk_idx) = self.inline_git_popup.as_ref().map(|popup| popup.hunk_idx) else {
            return;
        };
        let Some(hunk) = self.editor.git_hunks.get(hunk_idx).copied() else {
            self.inline_git_popup = None;
            return;
        };
        let Some(base_text) = self.editor.git_base_text.clone() else {
            self.inline_git_popup = None;
            return;
        };
        let current_text = self.editor.get_full_text();
        let before_lines = split_lines_preserve(&base_text);
        let after_lines = split_lines_preserve(&current_text);
        let old_text = range_text(&before_lines, hunk.before_start, hunk.before_end);
        let (replace_start, replace_end) = line_byte_bounds(
            &after_lines,
            hunk.after_start,
            hunk.after_end,
            current_text.len(),
        );
        self.editor.cursor = replace_end;
        let (offset, len, _) = self
            .editor
            .replace_range(replace_start, replace_end, &old_text);
        self.highlighter.shift_delete(offset, len);
        self.highlighter
            .shift_insert(offset, old_text.len(), Some(&old_text));
        self.inline_git_popup = None;
        self.inline_git_diff_rx = None;
        if !self.editor.sync_edits.is_empty() {
            let edits = std::mem::take(&mut self.editor.sync_edits);
            self.shift_current_python_inlay_hints_for_edits(&edits);
            let (invalidate_start_byte, invalidate_end_byte) =
                crate::highlighter::sync_edit_invalidation_byte_range(&edits);
            self.highlighter
                .apply_edits(self.editor.version, edits, None, None);
            self.highlighter.sync_highlight_after_edit(
                self.editor.version,
                None,
                None,
                invalidate_start_byte,
                invalidate_end_byte,
                std::time::Duration::from_millis(1),
            );
        }
        if self.is_ide_mode
            && let (Some(lsp), Some(path)) = (&mut self.lsp, &self.file_path)
        {
            let text = self.editor.get_full_text();
            let ext = self.file_extension.clone();
            lsp.notify_change(path, &ext, &text, self.editor.version as i32);
            self.last_sent_version = self.editor.version;
        }
        if self.show_search && !self.search_editor.get_full_text().is_empty() {
            self.update_search();
        } else {
            self.search_results.clear();
        }
        if let Some(window) = self.window.as_ref() {
            App::update_window_title(window, &self.base_title, self.editor.is_dirty());
            window.request_redraw();
        }
    }

    fn active_git_diff_focus_line(&self) -> usize {
        let line_height = self
            .renderer
            .as_ref()
            .map(|r| r.line_height)
            .unwrap_or(20.0)
            .max(1.0);
        let visible_h = self
            .renderer
            .as_ref()
            .map(|r| {
                let s = r.scale_factor;
                let tab_bar_h = if self.show_welcome || !self.is_ide_mode {
                    0.0
                } else {
                    38.0 * s
                };
                let panel_bottom_h = if self.is_ide_mode {
                    self.ide_panel.editor_reserved_bottom_height(s)
                } else {
                    0.0
                };
                crate::render_view::editor_view_height(
                    r.height,
                    tab_bar_h,
                    panel_bottom_h,
                    self.is_ide_mode,
                    s,
                )
            })
            .unwrap_or(line_height * 12.0);
        ((self.scroll_y.target + visible_h * GIT_DIFF_FOCUS_RATIO) / line_height)
            .floor()
            .max(0.0) as usize
    }

    fn active_git_diff_hunk_index_for_line(&self, line: usize) -> Option<usize> {
        self.active_git_diff_state().and_then(|state| {
            state
                .hunks
                .iter()
                .rposition(|hunk| hunk.display_start_line <= line)
        })
    }

    fn set_active_git_diff_current_hunk_for_line(&mut self, line: usize) {
        let idx = self.active_git_diff_hunk_index_for_line(line);
        if let Some(state) = self.active_git_diff_state_mut() {
            state.current_hunk_idx = idx;
        }
    }

    pub fn jump_active_git_diff_hunk(&mut self, direction: isize) {
        let focus_line = self.active_git_diff_focus_line();
        let Some(state) = self.active_git_diff_state() else {
            return;
        };
        if state.hunks.is_empty() {
            return;
        }
        let current_idx = state.current_hunk_idx.or_else(|| {
            state
                .hunks
                .iter()
                .rposition(|hunk| hunk.display_start_line <= focus_line)
        });
        let target_idx = if direction > 0 {
            current_idx
                .map(|idx| (idx + 1) % state.hunks.len())
                .unwrap_or(0)
        } else {
            current_idx
                .map(|idx| {
                    if idx == 0 {
                        state.hunks.len() - 1
                    } else {
                        idx - 1
                    }
                })
                .unwrap_or_else(|| state.hunks.len() - 1)
        };
        let target_line = state
            .hunks
            .get(target_idx)
            .map(|hunk| hunk.display_start_line);
        if let Some(target_line) = target_line {
            if let Some(state) = self.active_git_diff_state_mut() {
                state.current_hunk_idx = Some(target_idx);
            }
            self.animate_active_git_diff_to_line(target_line);
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
        }
    }

    pub fn show_readonly_diff_notice(&mut self) {
        self.readonly_notice_until =
            Some(std::time::Instant::now() + std::time::Duration::from_secs(2));
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub(crate) fn active_git_diff_lsp_hover_target(
        &self,
        display_byte: usize,
    ) -> Option<(PathBuf, u32, u32)> {
        let EditorTabKind::GitDiff(meta, state) = &self.tabs.get(self.active_tab)?.kind else {
            return None;
        };
        let display_line = self
            .editor
            .line_offsets
            .partition_point(|&offset| offset <= display_byte)
            .saturating_sub(1);
        let kind = state.line_kinds.get(display_line).copied()?;
        if matches!(kind, DiffLineKind::Deleted | DiffLineKind::ModifiedOld) {
            return None;
        }
        let worktree_line = state
            .line_kinds
            .iter()
            .take(display_line)
            .filter(|kind| !matches!(kind, DiffLineKind::Deleted | DiffLineKind::ModifiedOld))
            .count();
        let line_start = self
            .editor
            .line_offsets
            .get(display_line)
            .copied()
            .unwrap_or(0);
        let line_end = display_byte.min(self.editor.len());
        let col = self
            .editor
            .get_full_text()
            .get(line_start..line_end)
            .map(|slice| slice.chars().map(|ch| ch.len_utf16() as u32).sum())
            .unwrap_or(0);
        Some((
            meta.repo_root.join(&meta.rel_path),
            worktree_line as u32,
            col,
        ))
    }

    pub fn rollback_active_git_diff_hunk(&mut self, hunk_idx: usize) {
        let Some(state) = self.active_git_diff_state().cloned() else {
            return;
        };
        let Some(hunk) = state.hunks.get(hunk_idx).cloned() else {
            return;
        };
        let next_new_text = rollback_hunk_text(&state.worktree_text, &hunk);
        let replace_end = hunk.display_end.min(self.editor.len());
        let replace_start = hunk.display_start.min(replace_end);
        let _ = self
            .editor
            .replace_range(replace_start, replace_end, &hunk.old_text);
        let mut next_state = build_diff_view(state.base_text.clone(), next_new_text);
        next_state.version = state.version;
        next_state.undo_extract_line_kinds = Some(state.line_kinds.clone());
        next_state.redo_extract_line_kinds = Some(next_state.line_kinds.clone());
        let text = next_state.displayed_text.clone();
        if let Some(active_state) = self.active_git_diff_state_mut() {
            *active_state = next_state;
        }
        self.editor.set_text_preserve_history(&text);
        self.refresh_active_git_diff_highlight();
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub fn rebuild_active_git_diff_from_editor_after_history(&mut self, is_undo: bool) {
        let Some(state) = self.active_git_diff_state().cloned() else {
            return;
        };
        let displayed = self.editor.get_full_text();
        let extract_kinds = if is_undo {
            state
                .undo_extract_line_kinds
                .as_deref()
                .unwrap_or(&state.line_kinds)
        } else {
            state
                .redo_extract_line_kinds
                .as_deref()
                .unwrap_or(&state.line_kinds)
        };
        let next_new_text = extract_worktree_text(&displayed, extract_kinds);
        let mut next_state = build_diff_view(state.base_text.clone(), next_new_text);
        next_state.version = state.version;
        if is_undo {
            next_state.redo_extract_line_kinds = state.redo_extract_line_kinds.clone();
        } else {
            next_state.undo_extract_line_kinds = state.undo_extract_line_kinds.clone();
        }
        let text = next_state.displayed_text.clone();
        if let Some(active_state) = self.active_git_diff_state_mut() {
            *active_state = next_state;
        }
        self.editor.set_text_preserve_history(&text);
        self.refresh_active_git_diff_highlight();
    }

    pub fn save_active_git_diff(&mut self) -> bool {
        let Some((repo_root, rel_path, line_kinds)) =
            self.active_git_diff_state()
                .map(|state| match &self.tabs[self.active_tab].kind {
                    EditorTabKind::GitDiff(meta, _) => (
                        meta.repo_root.clone(),
                        meta.rel_path.clone(),
                        state.line_kinds.clone(),
                    ),
                    EditorTabKind::Normal | EditorTabKind::ApiClient(_, _) => unreachable!(),
                })
        else {
            return false;
        };
        let text = extract_worktree_text(&self.editor.get_full_text(), &line_kinds);
        let path = repo_root.join(rel_path);
        match std::fs::write(path, text) {
            Ok(()) => {
                self.editor.mark_saved();
                self.refresh_git_panel();
                true
            }
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_diff_build_added_deleted_modified_order() {
        let state = build_diff_view(
            "same\nold\nremove\n".to_string(),
            "same\nnew\nadd\n".to_string(),
        );
        assert_eq!(state.displayed_text, "same\nold\nremove\nnew\nadd\n");
        assert_eq!(
            state.line_kinds,
            vec![
                DiffLineKind::Context,
                DiffLineKind::ModifiedOld,
                DiffLineKind::ModifiedOld,
                DiffLineKind::ModifiedNew,
                DiffLineKind::ModifiedNew,
            ]
        );
    }

    #[test]
    fn git_diff_rollback_added_deletes_new_lines() {
        let state = build_diff_view("a\n".to_string(), "a\nb\n".to_string());
        let hunk = state.hunks.first().unwrap();
        assert_eq!(rollback_hunk_text(&state.worktree_text, hunk), "a\n");
    }

    #[test]
    fn git_diff_rollback_deleted_restores_old_lines() {
        let state = build_diff_view("a\nb\n".to_string(), "a\n".to_string());
        let hunk = state.hunks.first().unwrap();
        assert_eq!(rollback_hunk_text(&state.worktree_text, hunk), "a\nb\n");
    }

    #[test]
    fn git_diff_rollback_modified_replaces_new_with_old() {
        let state = build_diff_view("a\nold\n".to_string(), "a\nnew\n".to_string());
        let hunk = state.hunks.first().unwrap();
        assert_eq!(rollback_hunk_text(&state.worktree_text, hunk), "a\nold\n");
    }

    #[test]
    fn inline_diff_hunk_match_prefers_line_ranges() {
        let state = build_diff_view(
            "a\nold\nb\nbefore\n".to_string(),
            "a\nnew\nb\nafter\n".to_string(),
        );
        let target = LineDiffHunk {
            before_start: 3,
            before_end: 4,
            after_start: 3,
            after_end: 4,
        };
        assert_eq!(
            App::inline_diff_hunk_index_for_target(&state, target, 0),
            Some(1)
        );
    }
}
