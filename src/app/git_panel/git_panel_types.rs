use crate::app::App;
use crate::editor::Editor;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitFileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    TypeChange,
    Untracked,
}

impl GitFileStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Added => "A",
            Self::Modified => "M",
            Self::Deleted => "D",
            Self::Renamed => "R",
            Self::TypeChange => "T",
            Self::Untracked => "U",
        }
    }

    pub fn color(self) -> [f32; 4] {
        match self {
            Self::Added | Self::Untracked => [0.48, 0.82, 0.52, 1.0],
            Self::Modified | Self::TypeChange => [0.97, 0.76, 0.38, 1.0],
            Self::Deleted => [0.95, 0.42, 0.46, 1.0],
            Self::Renamed => [0.48, 0.74, 1.0, 1.0],
        }
    }
}

#[derive(Clone, Debug)]
pub struct GitFileEntry {
    pub workspace_idx: usize,
    pub rel_path: Box<str>,
    pub old_rel_path: Option<Box<str>>,
    pub display_path: Box<str>,
    pub depth: u16,
    pub staged: bool,
    pub status: GitFileStatus,
}

#[derive(Clone, Debug)]
pub struct GitTreeRow {
    pub name: Box<str>,
    pub path: Box<str>,
    pub depth: u16,
    pub file_idx: Option<usize>,
    pub icon_key: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitFolderStageState {
    Empty,
    Partial,
    All,
}

#[derive(Clone, Debug)]
pub struct GitWorkspaceStatus {
    pub workspace_idx: usize,
    pub root: PathBuf,
    pub repo_root: Option<PathBuf>,
    pub branch_name: Option<String>,
    pub files: Vec<GitFileEntry>,
    pub tree: Vec<GitTreeRow>,
    pub ahead: usize,
    pub error: Option<String>,
}

impl GitWorkspaceStatus {
    pub fn staged_count(&self) -> usize {
        self.files.iter().filter(|file| file.staged).count()
    }

    pub(crate) fn has_collapsible_rows(&self) -> bool {
        self.error.is_some() || !self.files.is_empty() || !self.tree.is_empty()
    }
}

#[derive(Clone, Debug, Default)]
pub struct GitStatusSnapshot {
    pub workspaces: Vec<GitWorkspaceStatus>,
}

impl GitStatusSnapshot {
    pub fn active_staged_workspace_idx(&self) -> Option<usize> {
        self.workspaces
            .iter()
            .find(|workspace| workspace.files.iter().any(|file| file.staged))
            .map(|workspace| workspace.workspace_idx)
    }

    pub fn has_staged_repo_files(&self) -> bool {
        self.workspaces.iter().any(|workspace| {
            workspace.repo_root.is_some() && workspace.files.iter().any(|file| file.staged)
        })
    }

    pub fn staged_repo_roots(&self) -> Vec<PathBuf> {
        let mut seen = FxHashSet::default();
        let mut roots = Vec::new();
        let active_workspace = self.active_staged_workspace_idx();
        for workspace in &self.workspaces {
            if active_workspace.is_some_and(|idx| idx != workspace.workspace_idx) {
                continue;
            }
            if !workspace.files.iter().any(|file| file.staged) {
                continue;
            }
            let Some(repo_root) = &workspace.repo_root else {
                continue;
            };
            if seen.insert(crate::platform::PathKey::new(repo_root)) {
                roots.push(repo_root.clone());
            }
        }
        roots
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct BranchAheadKey {
    repo_root: crate::platform::PathKey,
    head_oid: git2::Oid,
    upstream_oid: git2::Oid,
}

type BranchAheadCache = FxHashMap<BranchAheadKey, usize>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitGraphLaneKind {
    Vertical,
    VerticalTop,
    VerticalBottom,
    Shift,
    ShiftToCommit,
    Parent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitGraphLane {
    pub column: u16,
    pub target_column: u16,
    pub color_idx: u16,
    pub kind: GitGraphLaneKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GitGraphStats {
    pub files_changed: usize,
    pub insertions: usize,
    pub deletions: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitGraphRef {
    pub name: String,
    pub is_remote: bool,
}

#[derive(Clone, Debug)]
pub struct GitGraphCommit {
    pub oid: Arc<str>,
    pub short_oid: String,
    pub summary: String,
    pub branch_name: Option<Arc<str>>,
    pub author_name: String,
    pub author_email: String,
    pub time_secs: i64,
    pub time_offset: i32,
    pub relative_time: String,
    pub absolute_time: String,
    pub local_refs: Vec<GitGraphRef>,
    pub remote_refs: Vec<GitGraphRef>,
    pub lanes: Vec<GitGraphLane>,
    pub column: usize,
    pub color_idx: usize,
    pub branch_total_count: Option<usize>,
    pub is_head: bool,
    pub github_url: Option<String>,
    pub stats: Option<GitGraphStats>,
    parent_oids: Vec<Arc<str>>,
}

#[derive(Clone, Debug)]
struct GitGraphEvent {
    request_id: u64,
    workspace_idx: usize,
    repo_root: PathBuf,
    commits: Vec<GitGraphCommit>,
    lane_count: usize,
    notice: Option<String>,
    limit: usize,
    offset: usize,
    has_more: bool,
    reset_scroll: bool,
}

#[derive(Clone, Debug)]
struct GitGraphCacheEntry {
    commits: Vec<GitGraphCommit>,
    lane_count: usize,
    notice: Option<String>,
    limit: usize,
    has_more: bool,
}

pub(crate) const GIT_GRAPH_CONTROLS_H: f32 = 102.0;
pub(crate) const GIT_GRAPH_ROW_H: f32 = 34.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GitBottomPane {
    #[default]
    Closed,
    Graph,
    Logs,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GitCommitOptions {
    pub skip_hooks: bool,
}

impl GitCommitOptions {
    pub fn any_enabled(self) -> bool {
        self.skip_hooks
    }
}

pub(crate) fn git_graph_divider_h(scale: f32) -> f32 {
    scale.max(1.0)
}

pub(crate) fn git_graph_split_heights(list_h: f32, ratio: f32, scale: f32) -> (f32, f32, f32) {
    let divider_h = git_graph_divider_h(scale);
    if list_h <= divider_h {
        return (0.0, divider_h, 0.0);
    }
    let usable_h = list_h - divider_h;
    let min_graph_h = (160.0 * scale).min(usable_h);
    let min_changes_h = (72.0 * scale).min(usable_h);
    let max_graph_h = (usable_h - min_changes_h).max(min_graph_h);
    let graph_h = (usable_h * ratio.clamp(0.25, 0.78)).clamp(min_graph_h, max_graph_h);
    let changes_h = (usable_h - graph_h).max(0.0);
    (changes_h, divider_h, graph_h)
}

pub(crate) fn git_graph_max_scroll(commit_count: usize, view_h: f32, scale: f32) -> f32 {
    let total_h = commit_count as f32 * GIT_GRAPH_ROW_H * scale;
    (total_h - view_h).max(0.0)
}

pub(crate) fn git_graph_scroll_thumb_h(commit_count: usize, rows_h: f32, scale: f32) -> f32 {
    if commit_count == 0 || rows_h <= 0.0 {
        return 0.0;
    }
    let total_h = commit_count as f32 * GIT_GRAPH_ROW_H * scale;
    let track_h = (rows_h - 8.0 * scale).max(1.0);
    let min_thumb_h = 10.0 * scale;
    if track_h <= min_thumb_h {
        track_h
    } else {
        (rows_h / total_h * track_h).clamp(min_thumb_h, track_h)
    }
}

pub(crate) fn git_graph_near_load_more(scroll_target: f32, max_scroll: f32, scale: f32) -> bool {
    scroll_target >= (max_scroll - GIT_GRAPH_ROW_H * scale * 14.0).max(0.0)
}

pub(crate) fn apply_git_graph_scroll_drag(
    scroll: &mut crate::scroll::ScrollState,
    target: f32,
    drag_offset: f32,
) {
    let _ = crate::app::mouse::apply_scrollbar_drag_target(scroll, target, drag_offset);
}

pub(crate) fn git_graph_scroll_drag_target(
    pointer_y: f32,
    rows_y: f32,
    rows_h: f32,
    commit_count: usize,
    current_scroll: f32,
    drag_offset: Option<f32>,
    scale: f32,
) -> Option<(f32, f32)> {
    let max_scroll = git_graph_max_scroll(commit_count, rows_h, scale);
    if max_scroll <= 0.0 || rows_h <= 1.0 {
        return None;
    }
    let track_h = (rows_h - 8.0 * scale).max(1.0);
    let thumb_h = git_graph_scroll_thumb_h(commit_count, rows_h, scale);
    let thumb_y =
        rows_y + 4.0 * scale + (current_scroll / max_scroll).clamp(0.0, 1.0) * (track_h - thumb_h);
    let offset = drag_offset.unwrap_or_else(|| {
        if pointer_y >= thumb_y && pointer_y <= thumb_y + thumb_h {
            pointer_y - thumb_y
        } else {
            thumb_h / 2.0
        }
    });
    let ratio = (pointer_y - rows_y - 4.0 * scale - offset) / (track_h - thumb_h).max(1.0);
    Some((offset, (ratio * max_scroll).clamp(0.0, max_scroll)))
}

pub(crate) fn git_logs_max_scroll_from_content_height(
    content_h: f32,
    view_h: f32,
    scale: f32,
) -> f32 {
    let rows_h = (view_h - GIT_LOG_TOOLBAR_H * scale).max(0.0);
    (content_h - rows_h).max(0.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct GitLogLineId {
    pub(crate) epoch: u64,
    pub(crate) sequence: u64,
}

impl GitLogLineId {
    fn ordinal(self) -> u128 {
        (u128::from(self.epoch) << 64) | u128::from(self.sequence)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum GitLogDisplayLineId {
    TruncationMarker,
    Line(GitLogLineId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GitLogTextPoint {
    pub(crate) line: GitLogDisplayLineId,
    pub(crate) byte: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GitLogSelection {
    pub(crate) anchor: GitLogTextPoint,
    pub(crate) cursor: GitLogTextPoint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GitLogSelectionRange {
    pub(crate) start: GitLogTextPoint,
    pub(crate) end: GitLogTextPoint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GitLogBufferSnapshot {
    pub(crate) revision: u64,
    pub(crate) first_stored_line: Option<GitLogLineId>,
    pub(crate) last_stored_line: Option<GitLogLineId>,
    pub(crate) stored_line_count: usize,
    pub(crate) display_line_count: usize,
    pub(crate) truncated: bool,
}

#[derive(Clone, Debug)]
struct GitLogEntry {
    id: GitLogLineId,
    line: GitLogLine,
}

#[derive(Clone, Copy)]
pub(crate) struct GitLogDisplayLineRef<'a> {
    id: GitLogDisplayLineId,
    line: GitLogLineRef<'a>,
}

impl GitLogKind {
    pub(crate) fn display_prefix(self) -> &'static str {
        match self {
            Self::Stdout => "> ",
            Self::Stderr => "! ",
            _ => "",
        }
    }
}

impl<'a> GitLogDisplayLineRef<'a> {
    pub(crate) fn id(self) -> GitLogDisplayLineId {
        self.id
    }

    pub(crate) fn line(self) -> GitLogLineRef<'a> {
        self.line
    }

    pub(crate) fn byte_len(self) -> usize {
        let mut len = 0usize;
        self.visit_text_pieces(|piece| len = len.saturating_add(piece.len()));
        len
    }

    pub(crate) fn is_char_boundary(self, byte: usize) -> bool {
        let mut offset = 0usize;
        let mut valid = false;
        self.visit_text_pieces(|piece| {
            let end = offset.saturating_add(piece.len());
            if byte >= offset && byte <= end && piece.is_char_boundary(byte.saturating_sub(offset))
            {
                valid = true;
            }
            offset = end;
        });
        valid && byte <= offset
    }

    fn visit_text_pieces(self, mut visit: impl FnMut(&str)) {
        match self.line {
            GitLogLineRef::TruncationMarker => visit(GIT_LOG_TRUNCATION_MARKER),
            GitLogLineRef::Line(line) => {
                let prefix = line.kind.display_prefix();
                if !prefix.is_empty() {
                    visit(prefix);
                }
                for span in &line.spans {
                    visit(&span.text);
                }
            }
        }
    }

    fn push_range(self, start: usize, end: usize, out: &mut String) -> bool {
        if start > end
            || end > self.byte_len()
            || !self.is_char_boundary(start)
            || !self.is_char_boundary(end)
        {
            return false;
        }
        let mut offset = 0usize;
        let mut valid = true;
        self.visit_text_pieces(|piece| {
            let piece_start = offset;
            let piece_end = piece_start.saturating_add(piece.len());
            if start < piece_end && end > piece_start {
                let local_start = start.saturating_sub(piece_start);
                let local_end = end.min(piece_end).saturating_sub(piece_start);
                if let Some(slice) = piece.get(local_start..local_end) {
                    out.push_str(slice);
                } else {
                    valid = false;
                }
            }
            offset = piece_end;
        });
        valid
    }
}

impl GitLogBuffer {
    fn allocate_line_id(&mut self) -> GitLogLineId {
        let id = GitLogLineId {
            epoch: self.next_line_epoch,
            sequence: self.next_line_sequence,
        };
        if self.next_line_sequence == u64::MAX {
            self.next_line_epoch = self.next_line_epoch.wrapping_add(1);
            self.next_line_sequence = 0;
        } else {
            self.next_line_sequence += 1;
        }
        id
    }

    fn bump_revision(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn snapshot(&self) -> GitLogBufferSnapshot {
        GitLogBufferSnapshot {
            revision: self.revision,
            first_stored_line: self.lines.front().map(|entry| entry.id),
            last_stored_line: self.lines.back().map(|entry| entry.id),
            stored_line_count: self.lines.len(),
            display_line_count: self.line_count(),
            truncated: self.truncated,
        }
    }

    pub(crate) fn display_line_at(&self, index: usize) -> Option<GitLogDisplayLineRef<'_>> {
        if self.truncated && index == 0 {
            return Some(GitLogDisplayLineRef {
                id: GitLogDisplayLineId::TruncationMarker,
                line: GitLogLineRef::TruncationMarker,
            });
        }
        let stored_index = index.checked_sub(usize::from(self.truncated))?;
        let entry = self.lines.get(stored_index)?;
        Some(GitLogDisplayLineRef {
            id: GitLogDisplayLineId::Line(entry.id),
            line: GitLogLineRef::Line(&entry.line),
        })
    }

    pub(crate) fn display_line_index(&self, id: GitLogDisplayLineId) -> Option<usize> {
        match id {
            GitLogDisplayLineId::TruncationMarker => self.truncated.then_some(0),
            GitLogDisplayLineId::Line(id) => {
                let first = self.lines.front()?.id;
                let distance = id.ordinal().checked_sub(first.ordinal())?;
                let stored_index = usize::try_from(distance).ok()?;
                self.lines
                    .get(stored_index)
                    .filter(|entry| entry.id == id)
                    .map(|_| stored_index + usize::from(self.truncated))
            }
        }
    }

    pub(crate) fn point_is_valid(&self, point: GitLogTextPoint) -> bool {
        let Some(index) = self.display_line_index(point.line) else {
            return false;
        };
        self.display_line_at(index)
            .is_some_and(|line| line.is_char_boundary(point.byte))
    }

    pub(crate) fn normalize_selection(
        &self,
        selection: GitLogSelection,
    ) -> Option<GitLogSelectionRange> {
        if !self.point_is_valid(selection.anchor) || !self.point_is_valid(selection.cursor) {
            return None;
        }
        let anchor_index = self.display_line_index(selection.anchor.line)?;
        let cursor_index = self.display_line_index(selection.cursor.line)?;
        let forward = anchor_index < cursor_index
            || (anchor_index == cursor_index && selection.anchor.byte <= selection.cursor.byte);
        let (start, end) = if forward {
            (selection.anchor, selection.cursor)
        } else {
            (selection.cursor, selection.anchor)
        };
        Some(GitLogSelectionRange { start, end })
    }

    pub(crate) fn copy_selection(&self, selection: GitLogSelection) -> Option<String> {
        let range = self.normalize_selection(selection)?;
        if range.start == range.end {
            return None;
        }
        let start_index = self.display_line_index(range.start.line)?;
        let end_index = self.display_line_index(range.end.line)?;
        let mut text = String::new();
        for index in start_index..=end_index {
            let line = self.display_line_at(index)?;
            let start = if index == start_index {
                range.start.byte
            } else {
                0
            };
            let end = if index == end_index {
                range.end.byte
            } else {
                line.byte_len()
            };
            if !line.push_range(start, end, &mut text) {
                return None;
            }
            if index != end_index {
                text.push('\n');
            }
        }
        Some(text)
    }

    pub(crate) fn set_selection(
        &mut self,
        anchor: GitLogTextPoint,
        cursor: GitLogTextPoint,
    ) -> bool {
        let selection = GitLogSelection { anchor, cursor };
        if self.normalize_selection(selection).is_none() {
            self.selection = None;
            return false;
        }
        self.selection = Some(selection);
        true
    }

    pub(crate) fn selection(&self) -> Option<GitLogSelection> {
        self.selection
    }

    fn prune_selection(&mut self) {
        if self
            .selection
            .is_some_and(|selection| self.normalize_selection(selection).is_none())
        {
            self.selection = None;
        }
    }
}

pub struct GitPanelState {
    pub snapshot: GitStatusSnapshot,
    pub message_editor: Editor,
    pub message_focused: bool,
    pub amend: bool,
    pub commit_menu_opened_at: Option<std::time::Instant>,
    pub commit_options_menu_opened_at: Option<std::time::Instant>,
    pub commit_options: GitCommitOptions,
    pub repo_action_menu_workspace_idx: Option<usize>,
    pub repo_action_menu_opened_at: Option<std::time::Instant>,
    pub collapsed_workspaces: FxHashSet<usize>,
    pub collapsed_dirs: FxHashMap<usize, FxHashSet<String>>,
    pub scroll: crate::scroll::ScrollState,
    pub pending: bool,
    pub pending_label: Option<String>,
    active_git_hooks: Vec<(String, u64, String)>,
    pub selected_file: Option<(usize, usize)>,
    pending_started_at: Option<std::time::Instant>,
    pending_label_until: Option<std::time::Instant>,
    pub next_request_id: u64,
    pub latest_request_id: u64,
    applied_request_id: u64,
    rx: Vec<GitPanelReceiver>,
    stage_tx: Option<mpsc::Sender<GitStageCommand>>,
    stage_reconcile_candidates: FxHashSet<GitStageOwnershipKey>,
    status_refresh_pending: bool,
    status_refresh_dirty: bool,
    branch_ahead_cache: BranchAheadCache,
    pub stage_pending_workspace_idx: Option<usize>,
    pub notice: Option<String>,
    pub confirm_dialog: Option<GitConfirmDialog>,
    pub bottom_pane: GitBottomPane,
    pub graph_scroll: crate::scroll::ScrollState,
    pub logs_scroll: crate::scroll::ScrollState,
    pub logs_follow_tail: bool,
    logs_copy_owner: bool,
    pub(crate) git_logs: GitLogBuffer,
    pub graph_pending: bool,
    pub graph_snapshot: Vec<GitGraphCommit>,
    pub graph_workspace_idx: Option<usize>,
    pub graph_repo_root: Option<PathBuf>,
    pub graph_notice: Option<String>,
    pub graph_height_ratio: f32,
    pub graph_resizing: bool,
    pub graph_lane_count: usize,
    pub graph_workspace_scroll_x: f32,
    pub graph_commit_limit: usize,
    pub graph_has_more: bool,
    pub graph_copied_commit: Option<(usize, usize)>,
    graph_rx: Vec<GitGraphReceiver>,
    graph_next_request_id: u64,
    graph_latest_request_id: u64,
    graph_latest_request_by_root: FxHashMap<crate::platform::PathKey, u64>,
    graph_pending_roots: FxHashSet<crate::platform::PathKey>,
    graph_cache: FxHashMap<crate::platform::PathKey, GitGraphCacheEntry>,
    graph_refresh_after_status: bool,
}

struct GitPanelReceiver {
    rx: mpsc::Receiver<GitPanelTaskResult>,
    runtime_rx: Option<mpsc::Receiver<GitRuntimeEvent>>,
    request_id: u64,
    blocking: bool,
    refresh: bool,
    status_mutation: bool,
}

struct GitGraphReceiver {
    rx: mpsc::Receiver<GitGraphEvent>,
    request_id: u64,
    repo_root: PathBuf,
}

struct GitPanelTaskResult {
    event: GitPanelEvent,
    branch_ahead_cache: BranchAheadCache,
}

enum OneShotReceiverPoll<T> {
    Pending,
    Ready(T),
    Disconnected,
}

fn poll_one_shot_receiver<T>(rx: &mpsc::Receiver<T>) -> OneShotReceiverPoll<T> {
    match rx.try_recv() {
        Ok(value) => OneShotReceiverPoll::Ready(value),
        Err(mpsc::TryRecvError::Empty) => OneShotReceiverPoll::Pending,
        Err(mpsc::TryRecvError::Disconnected) => OneShotReceiverPoll::Disconnected,
    }
}

struct GitActionOutcome {
    notice: Option<String>,
    clear_message: bool,
    refresh_graph: bool,
    transaction_failed: bool,
}

impl Default for GitPanelState {
    fn default() -> Self {
        Self {
            snapshot: GitStatusSnapshot::default(),
            message_editor: Editor::new(512),
            message_focused: false,
            amend: false,
            commit_menu_opened_at: None,
            commit_options_menu_opened_at: None,
            commit_options: GitCommitOptions::default(),
            repo_action_menu_workspace_idx: None,
            repo_action_menu_opened_at: None,
            collapsed_workspaces: FxHashSet::default(),
            collapsed_dirs: FxHashMap::default(),
            scroll: crate::scroll::ScrollState::new(15.0),
            pending: false,
            pending_label: None,
            active_git_hooks: Vec::new(),
            selected_file: None,
            pending_started_at: None,
            pending_label_until: None,
            next_request_id: 1,
            latest_request_id: 0,
            applied_request_id: 0,
            rx: Vec::new(),
            stage_tx: None,
            stage_reconcile_candidates: FxHashSet::default(),
            status_refresh_pending: false,
            status_refresh_dirty: false,
            branch_ahead_cache: BranchAheadCache::default(),
            stage_pending_workspace_idx: None,
            notice: None,
            confirm_dialog: None,
            bottom_pane: GitBottomPane::Closed,
            graph_scroll: crate::scroll::ScrollState::new(15.0),
            logs_scroll: crate::scroll::ScrollState::new(15.0),
            logs_follow_tail: true,
            logs_copy_owner: false,
            git_logs: GitLogBuffer::default(),
            graph_pending: false,
            graph_snapshot: Vec::new(),
            graph_workspace_idx: None,
            graph_repo_root: None,
            graph_notice: None,
            graph_height_ratio: 0.45,
            graph_resizing: false,
            graph_lane_count: 1,
            graph_workspace_scroll_x: 0.0,
            graph_commit_limit: GIT_GRAPH_LIMIT_STEP,
            graph_has_more: false,
            graph_copied_commit: None,
            graph_rx: Vec::new(),
            graph_next_request_id: 1,
            graph_latest_request_id: 0,
            graph_latest_request_by_root: FxHashMap::default(),
            graph_pending_roots: FxHashSet::default(),
            graph_cache: FxHashMap::default(),
            graph_refresh_after_status: false,
        }
    }
}

impl GitPanelState {
    pub fn commit_menu_open(&self) -> bool {
        self.commit_menu_opened_at.is_some()
    }

    pub fn commit_options_menu_open(&self) -> bool {
        self.commit_options_menu_opened_at.is_some()
    }

    pub fn close_commit_menus(&mut self) -> bool {
        self.commit_menu_opened_at.take().is_some()
            | self.commit_options_menu_opened_at.take().is_some()
    }

    pub fn toggle_commit_menu(&mut self, now: std::time::Instant) {
        self.commit_options_menu_opened_at = None;
        self.commit_menu_opened_at = if self.commit_menu_opened_at.is_some() {
            None
        } else {
            Some(now)
        };
    }

    pub fn toggle_commit_options_menu(&mut self, now: std::time::Instant) {
        self.commit_menu_opened_at = None;
        self.commit_options_menu_opened_at = if self.commit_options_menu_opened_at.is_some() {
            None
        } else {
            Some(now)
        };
    }

    pub fn close_repo_action_menu(&mut self) -> bool {
        let was_open = self.repo_action_menu_workspace_idx.take().is_some();
        self.repo_action_menu_opened_at = None;
        was_open
    }

    pub fn toggle_repo_action_menu(&mut self, workspace_idx: usize, now: std::time::Instant) {
        if self.repo_action_menu_workspace_idx == Some(workspace_idx) {
            self.close_repo_action_menu();
        } else {
            self.repo_action_menu_workspace_idx = Some(workspace_idx);
            self.repo_action_menu_opened_at = Some(now);
        }
    }

    pub fn active_repo_action_menu_opened_at(&self) -> Option<std::time::Instant> {
        self.repo_action_menu_workspace_idx
            .and(self.repo_action_menu_opened_at)
    }

    pub fn graph_open(&self) -> bool {
        self.bottom_pane == GitBottomPane::Graph
    }

    pub fn logs_open(&self) -> bool {
        self.bottom_pane == GitBottomPane::Logs
    }

    pub fn toggle_graph_pane(&mut self) {
        self.bottom_pane = if self.graph_open() {
            GitBottomPane::Closed
        } else {
            GitBottomPane::Graph
        };
        self.revoke_git_logs_copy_owner();
    }

    pub fn toggle_logs_pane(&mut self) {
        self.bottom_pane = if self.logs_open() {
            GitBottomPane::Closed
        } else {
            GitBottomPane::Logs
        };
        self.revoke_git_logs_copy_owner();
    }

    pub fn open_logs_for_failure(&mut self) {
        let was_logs_open = self.logs_open();
        self.bottom_pane = GitBottomPane::Logs;
        if !was_logs_open {
            self.logs_follow_tail = true;
            self.revoke_git_logs_copy_owner();
        }
        if self.graph_height_ratio < 0.50 {
            self.graph_height_ratio = 0.50;
        }
    }

    pub fn clear_git_logs(&mut self) {
        self.git_logs.clear();
        self.logs_scroll.reset();
        self.logs_follow_tail = true;
        self.revoke_git_logs_copy_owner();
    }

    pub(crate) fn claim_git_logs_copy_owner(&mut self) {
        self.logs_copy_owner = true;
    }

    pub(crate) fn revoke_git_logs_copy_owner(&mut self) {
        self.logs_copy_owner = false;
    }

    pub(crate) fn owns_git_logs_copy(&self) -> bool {
        self.logs_copy_owner
    }

    pub(crate) fn copy_owned_git_logs_selection(&self) -> Option<String> {
        if !self.owns_git_logs_copy() || !self.logs_open() {
            return None;
        }
        self.git_logs
            .selection()
            .and_then(|selection| self.git_logs.copy_selection(selection))
    }

    #[cfg(test)]
    pub(crate) fn seed_git_log_for_test(&mut self, text: &str) {
        self.git_logs
            .append(GitLogLine::plain(GitLogKind::Info, text.to_string()));
    }

    pub(crate) fn refresh_git_logs_follow_tail(&mut self, max_scroll: f32) {
        self.logs_follow_tail = self.logs_scroll.target >= (max_scroll - 1.0).max(0.0);
    }

    pub(crate) fn scroll_git_logs_by(&mut self, delta: f32, max_scroll: f32) {
        self.logs_scroll.anim_speed = 7.0;
        self.logs_scroll.scroll_by(delta);
        self.logs_scroll.clamp_target(0.0, max_scroll);
        self.refresh_git_logs_follow_tail(max_scroll);
    }

    pub(crate) fn update_git_logs_scroll(&mut self, dt: f32, max_scroll: f32) -> bool {
        let before_current = self.logs_scroll.current;
        let before_target = self.logs_scroll.target;
        if self.logs_follow_tail {
            self.logs_scroll.set_target(max_scroll);
        } else {
            self.logs_scroll.clamp_target(0.0, max_scroll);
        }
        self.logs_scroll.clamp_current(0.0, max_scroll);
        let animated = self.logs_scroll.update(dt);
        animated
            || self.logs_scroll.current != before_current
            || self.logs_scroll.target != before_target
    }

    pub(crate) fn allocate_status_request_id(&mut self) -> u64 {
        let request_id = self.next_request_id.max(1);
        self.next_request_id = request_id.wrapping_add(1).max(1);
        self.latest_request_id = request_id;
        request_id
    }

    pub(crate) fn allocate_graph_request_id(&mut self) -> u64 {
        let request_id = self.graph_next_request_id.max(1);
        self.graph_next_request_id = request_id.wrapping_add(1).max(1);
        self.graph_latest_request_id = request_id;
        request_id
    }

    pub(crate) fn reset_async_state(&mut self) {
        self.rx.clear();
        self.graph_rx.clear();
        self.pending = false;
        self.pending_label = None;
        self.active_git_hooks.clear();
        self.pending_started_at = None;
        self.status_refresh_pending = false;
        self.status_refresh_dirty = false;
        self.stage_pending_workspace_idx = None;
        self.stage_reconcile_candidates.clear();
        self.graph_pending = false;
        self.graph_pending_roots.clear();
        self.graph_latest_request_by_root.clear();
    }

    pub(crate) fn handle_status_disconnect(&mut self, request_id: u64) {
        if request_id == self.latest_request_id {
            self.notice = Some("Git-операция неожиданно завершилась".to_string());
        }
    }

    pub(crate) fn handle_graph_disconnect(&mut self, repo_root: &std::path::Path, request_id: u64) {
        let key = crate::platform::PathKey::new(repo_root);
        if self.graph_latest_request_by_root.get(&key).copied() == Some(request_id) {
            self.graph_pending_roots.remove(&key);
            self.graph_latest_request_by_root.remove(&key);
            self.graph_notice = Some("Загрузка Git Graph неожиданно завершилась".to_string());
            if self.graph_repo_root.as_ref().is_some_and(|root| crate::platform::paths_equal(root, repo_root)) {
                self.graph_pending = false;
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn seed_graph_request_for_test(
        &mut self,
        repo_root: std::path::PathBuf,
        request_id: u64,
        active: bool,
    ) {
        let key = crate::platform::PathKey::new(&repo_root);
        self.graph_latest_request_by_root.insert(key.clone(), request_id);
        self.graph_pending_roots.insert(key);
        if active {
            self.graph_repo_root = Some(repo_root);
            self.graph_pending = true;
        }
    }

    #[cfg(test)]
    pub(crate) fn graph_request_pending_for_test(
        &self,
        repo_root: &std::path::Path,
        request_id: u64,
    ) -> bool {
        let key = crate::platform::PathKey::new(repo_root);
        self.graph_latest_request_by_root.get(&key).copied() == Some(request_id)
            && self.graph_pending_roots.contains(&key)
    }

    #[cfg(test)]
    pub(crate) fn set_graph_next_request_id_for_test(&mut self, value: u64) {
        self.graph_next_request_id = value;
    }

    #[cfg(test)]
    pub(crate) fn async_state_is_empty_for_test(&self) -> bool {
        self.rx.is_empty()
            && self.graph_rx.is_empty()
            && self.graph_pending_roots.is_empty()
            && self.graph_latest_request_by_root.is_empty()
            && !self.pending
            && !self.graph_pending
    }

    #[cfg(test)]
    pub(crate) fn applied_status_request_id_for_test(&self) -> u64 {
        self.applied_request_id
    }

    #[cfg(test)]
    pub(crate) fn status_mutation_in_flight_for_test(&self) -> bool {
        self.rx.iter().any(|receiver| receiver.status_mutation)
    }

    fn begin_status_refresh(&mut self) -> bool {
        if self.status_refresh_pending {
            self.status_refresh_dirty = true;
            false
        } else {
            self.status_refresh_pending = true;
            self.status_refresh_dirty = false;
            true
        }
    }

    fn finish_status_refresh(&mut self) -> bool {
        self.status_refresh_pending = false;
        if self.status_refresh_dirty {
            self.status_refresh_dirty = false;
            true
        } else {
            false
        }
    }

    pub(crate) fn pending_elapsed_secs(&self, now: std::time::Instant) -> Option<f32> {
        self.pending
            .then_some(self.pending_started_at?)
            .map(|started_at| now.saturating_duration_since(started_at).as_secs_f32())
    }

    pub fn staged_workspace_lock(&self) -> Option<usize> {
        self.stage_pending_workspace_idx
            .or_else(|| self.snapshot.active_staged_workspace_idx())
    }

    pub fn status_loading(&self) -> bool {
        self.status_refresh_pending
    }

    pub fn commit_enabled(&self) -> bool {
        self.snapshot.has_staged_repo_files()
    }

    fn update_stage_reconcile_candidates(&mut self, files: &[GitStageFileCommand]) {
        for file in files {
            let key = GitStageOwnershipKey::new(&file.repo_root, &file.rel_path);
            if file.staged {
                self.stage_reconcile_candidates.remove(&key);
                continue;
            }
            let is_modified = self.snapshot.workspaces.iter().any(|workspace| {
                workspace.repo_root.as_ref().is_some_and(|repo_root| {
                    crate::platform::PathKey::new(repo_root) == key.repo_root
                }) && workspace.files.iter().any(|entry| {
                    entry.rel_path.as_ref() == key.rel_path
                        && entry.status == GitFileStatus::Modified
                })
            });
            if is_modified {
                self.stage_reconcile_candidates.insert(key);
            } else {
                self.stage_reconcile_candidates.remove(&key);
            }
        }
    }

    fn take_stage_reconcile_candidate(&mut self, repo_root: &Path, rel_path: &str) -> bool {
        self.stage_reconcile_candidates
            .remove(&GitStageOwnershipKey::new(repo_root, rel_path))
    }

    fn clear_stage_reconcile_candidates_for_operation(&mut self, operation: &GitStageOperation) {
        if let GitStageOperation::ToggleMany(files) = operation {
            for file in files {
                self.stage_reconcile_candidates
                    .remove(&GitStageOwnershipKey::new(&file.repo_root, &file.rel_path));
            }
        }
    }

    fn retain_stage_reconcile_candidates(&mut self) {
        let snapshot = &self.snapshot;
        self.stage_reconcile_candidates.retain(|key| {
            snapshot.workspaces.iter().any(|workspace| {
                workspace.repo_root.as_ref().is_some_and(|repo_root| {
                    crate::platform::PathKey::new(repo_root) == key.repo_root
                }) && workspace.files.iter().any(|file| {
                    file.rel_path.as_ref() == key.rel_path
                        && file.staged
                        && file.status == GitFileStatus::Modified
                })
            })
        });
    }

    #[cfg(test)]
    pub(crate) fn has_stage_reconcile_candidate_for_test(
        &self,
        repo_root: &Path,
        rel_path: &str,
    ) -> bool {
        self.stage_reconcile_candidates
            .contains(&GitStageOwnershipKey::new(repo_root, rel_path))
    }

    fn apply_event(&mut self, event: GitPanelEvent) {
        self.latest_request_id = event.request_id;
        self.applied_request_id = event.request_id;
        self.notice = event.notice;
        if event.transaction_failed {
            self.open_logs_for_failure();
        }
        if event.clear_message {
            self.message_editor = Editor::new(512);
            self.message_focused = false;
        }
        if self.stage_pending_workspace_idx.is_some() && !event.preserve_snapshot_on_empty {
            return;
        }
        if event.preserve_snapshot_on_empty && git_snapshot_has_visible_rows(&self.snapshot) {
            merge_stage_snapshot(&mut self.snapshot, event.snapshot);
            self.stage_pending_workspace_idx = None;
            self.retain_stage_reconcile_candidates();
            return;
        }
        self.snapshot = event.snapshot;
        if event.preserve_snapshot_on_empty {
            self.stage_pending_workspace_idx = None;
        }
        self.retain_stage_reconcile_candidates();
    }
}

#[derive(Clone, Debug)]
pub struct GitPanelEvent {
    request_id: u64,
    snapshot: GitStatusSnapshot,
    notice: Option<String>,
    preserve_snapshot_on_empty: bool,
    clear_message: bool,
    refresh_graph: bool,
    transaction_failed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitConfirmAction {
    RollbackStaged,
}

#[derive(Clone, Debug)]
pub struct GitConfirmFile {
    pub repo_root: PathBuf,
    pub rel_path: String,
    pub old_rel_path: Option<String>,
    pub display_path: String,
}

#[derive(Clone, Debug)]
pub struct GitConfirmDialog {
    pub action: GitConfirmAction,
    pub workspace_idx: usize,
    pub files: Vec<GitConfirmFile>,
}

#[derive(Clone, Debug)]
struct GitStageFileCommand {
    repo_root: PathBuf,
    rel_path: String,
    old_rel_path: Option<String>,
    staged: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct GitStageOwnershipKey {
    repo_root: crate::platform::PathKey,
    rel_path: String,
}

impl GitStageOwnershipKey {
    fn new(repo_root: &Path, rel_path: &str) -> Self {
        Self {
            repo_root: crate::platform::PathKey::new(repo_root),
            rel_path: rel_path.to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GitIndexEntryIdentity {
    id: git2::Oid,
    mode: u32,
}

#[derive(Clone, Debug)]
struct GitReconcileFileCommand {
    repo_root: PathBuf,
    rel_path: String,
}

enum GitStageOperation {
    ToggleMany(Vec<GitStageFileCommand>),
    ReconcileModified(GitReconcileFileCommand),
}

struct GitStageCommand {
    request_id: u64,
    operation: GitStageOperation,
    workspaces: Vec<PathBuf>,
    branch_ahead_cache: BranchAheadCache,
    tx: mpsc::Sender<GitPanelTaskResult>,
}

enum GitAction {
    Refresh,
    LoadGraph {
        workspace_idx: usize,
        repo_root: PathBuf,
        offset: usize,
        limit: usize,
        reset_scroll: bool,
        activate: bool,
    },
    ToggleStageMany {
        files: Vec<GitStageFileCommand>,
    },
    ReconcileStagedModified {
        file: GitReconcileFileCommand,
    },
    Commit {
        repo_roots: Vec<PathBuf>,
        message: String,
        amend: bool,
        push_after: bool,
        skip_hooks: bool,
    },
    RollbackStaged {
        files: Vec<GitStageFileCommand>,
    },
    Push {
        repo_root: PathBuf,
    },
    Fetch {
        repo_root: PathBuf,
    },
    Pull {
        repo_root: PathBuf,
    },
}

#[cfg(test)]
mod git_log_document_tests {
    use super::*;

    fn point_at(logs: &GitLogBuffer, line_index: usize, byte: usize) -> GitLogTextPoint {
        GitLogTextPoint {
            line: logs.display_line_at(line_index).unwrap().id(),
            byte,
        }
    }

    fn info_line(text: impl Into<String>) -> GitLogLine {
        GitLogLine::plain(GitLogKind::Info, text)
    }

    #[test]
    fn git_log_selection_normalizes_forward_and_reverse_identically() {
        let mut logs = GitLogBuffer::default();
        logs.append(info_line("abcdef"));
        let start = point_at(&logs, 0, 1);
        let end = point_at(&logs, 0, 5);
        let forward = logs
            .normalize_selection(GitLogSelection {
                anchor: start,
                cursor: end,
            })
            .unwrap();
        let reverse = logs
            .normalize_selection(GitLogSelection {
                anchor: end,
                cursor: start,
            })
            .unwrap();
        assert_eq!(forward, reverse);
        assert_eq!(
            logs.copy_selection(GitLogSelection {
                anchor: start,
                cursor: end
            })
            .as_deref(),
            Some("bcde")
        );
        assert_eq!(
            logs.copy_selection(GitLogSelection {
                anchor: end,
                cursor: start
            })
            .as_deref(),
            Some("bcde")
        );
    }

    #[test]
    fn git_log_copy_preserves_semantic_span_text_without_ansi_metadata() {
        let mut logs = GitLogBuffer::default();
        logs.append(GitLogLine {
            kind: GitLogKind::Stdout,
            spans: vec![
                GitLogSpan {
                    text: "red".to_string(),
                    ansi_fg: Some(1),
                },
                GitLogSpan {
                    text: "blue".to_string(),
                    ansi_fg: Some(4),
                },
            ],
        });
        let line = logs.display_line_at(0).unwrap();
        let start = GitLogTextPoint {
            line: line.id(),
            byte: 0,
        };
        let end = GitLogTextPoint {
            line: line.id(),
            byte: line.byte_len(),
        };
        assert_eq!(
            logs.copy_selection(GitLogSelection {
                anchor: start,
                cursor: end
            })
            .as_deref(),
            Some("> redblue")
        );
    }

    #[test]
    fn git_log_copy_uses_only_logical_newlines() {
        let mut logs = GitLogBuffer::default();
        logs.append(info_line("abcdef"));
        logs.append(info_line("ghijkl"));

        let one_line = GitLogSelection {
            anchor: point_at(&logs, 0, 1),
            cursor: point_at(&logs, 0, 5),
        };
        assert_eq!(logs.copy_selection(one_line).as_deref(), Some("bcde"));
        assert!(!logs.copy_selection(one_line).unwrap().contains('\n'));

        let two_lines = GitLogSelection {
            anchor: point_at(&logs, 0, 2),
            cursor: point_at(&logs, 1, 2),
        };
        assert_eq!(logs.copy_selection(two_lines).as_deref(), Some("cdef\ngh"));
    }

    #[test]
    fn git_log_points_are_utf8_safe_for_cyrillic_and_emoji() {
        let mut logs = GitLogBuffer::default();
        let text = "Привет🙂мир";
        logs.append(info_line(text));
        let line_id = logs.display_line_at(0).unwrap().id();
        let emoji_start = text.find('🙂').unwrap();
        let after_emoji = emoji_start + '🙂'.len_utf8();
        let start = GitLogTextPoint {
            line: line_id,
            byte: "Пр".len(),
        };
        let end = GitLogTextPoint {
            line: line_id,
            byte: after_emoji,
        };
        assert!(logs.point_is_valid(start));
        assert!(logs.point_is_valid(end));
        assert!(!logs.point_is_valid(GitLogTextPoint {
            line: line_id,
            byte: emoji_start + 1,
        }));
        assert_eq!(
            logs.copy_selection(GitLogSelection {
                anchor: start,
                cursor: end
            })
            .as_deref(),
            Some("ивет🙂")
        );
    }

    #[test]
    fn git_log_display_prefix_policy_is_explicit_and_selectable() {
        assert_eq!(GitLogKind::Stdout.display_prefix(), "> ");
        assert_eq!(GitLogKind::Stderr.display_prefix(), "! ");
        assert_eq!(GitLogKind::Header.display_prefix(), "");

        let mut logs = GitLogBuffer::default();
        logs.append(GitLogLine::plain(GitLogKind::Stderr, "failed"));
        let line = logs.display_line_at(0).unwrap();
        assert_eq!(line.byte_len(), "! failed".len());
        let selection = GitLogSelection {
            anchor: GitLogTextPoint {
                line: line.id(),
                byte: 0,
            },
            cursor: GitLogTextPoint {
                line: line.id(),
                byte: line.byte_len(),
            },
        };
        assert_eq!(logs.copy_selection(selection).as_deref(), Some("! failed"));
    }

    #[test]
    fn git_log_clear_and_front_eviction_invalidate_selection() {
        let mut logs = GitLogBuffer::default();
        logs.append(info_line("selected"));
        let anchor = point_at(&logs, 0, 0);
        let cursor = point_at(&logs, 0, 4);
        assert!(logs.set_selection(anchor, cursor));
        logs.clear();
        assert_eq!(logs.selection(), None);

        logs.append(info_line("old"));
        let anchor = point_at(&logs, 0, 0);
        let cursor = point_at(&logs, 0, 1);
        assert!(logs.set_selection(anchor, cursor));
        let chunk = "x".repeat(900 * 1024);
        logs.append(info_line(chunk.clone()));
        logs.append(info_line(chunk.clone()));
        logs.append(info_line(chunk));
        assert!(logs.snapshot().truncated);
        assert_eq!(logs.selection(), None);
    }

    #[test]
    fn git_log_snapshot_distinguishes_append_eviction_clear_and_truncation_marker() {
        let mut logs = GitLogBuffer::default();
        let empty = logs.snapshot();
        logs.append(info_line("first"));
        let first = logs.snapshot();
        logs.append(info_line("second"));
        let appended = logs.snapshot();
        assert_ne!(empty.revision, first.revision);
        assert_ne!(first.revision, appended.revision);
        assert_eq!(first.first_stored_line, appended.first_stored_line);
        assert_ne!(first.last_stored_line, appended.last_stored_line);
        assert!(!appended.truncated);

        let chunk = "x".repeat(900 * 1024);
        logs.append(info_line(chunk.clone()));
        logs.append(info_line(chunk.clone()));
        logs.append(info_line(chunk));
        let evicted = logs.snapshot();
        assert!(evicted.truncated);
        assert_ne!(appended.first_stored_line, evicted.first_stored_line);
        assert_eq!(evicted.display_line_count, evicted.stored_line_count + 1);
        assert!(matches!(
            logs.display_line_at(0).map(|line| line.id()),
            Some(GitLogDisplayLineId::TruncationMarker)
        ));

        logs.clear();
        let cleared = logs.snapshot();
        assert_ne!(evicted.revision, cleared.revision);
        assert_eq!(cleared.stored_line_count, 0);
        assert_eq!(cleared.display_line_count, 0);
        assert!(!cleared.truncated);
    }

    #[test]
    fn git_log_line_identity_sequence_wrap_advances_epoch() {
        let mut logs = GitLogBuffer::default();
        logs.next_line_epoch = 7;
        logs.next_line_sequence = u64::MAX;
        logs.append(info_line("last-in-epoch"));
        logs.append(info_line("first-in-next"));
        assert_eq!(
            logs.display_line_at(0).unwrap().id(),
            GitLogDisplayLineId::Line(GitLogLineId {
                epoch: 7,
                sequence: u64::MAX,
            })
        );
        assert_eq!(
            logs.display_line_at(1).unwrap().id(),
            GitLogDisplayLineId::Line(GitLogLineId {
                epoch: 8,
                sequence: 0,
            })
        );
    }

    #[test]
    fn git_log_copy_owner_gates_selection_and_does_not_resurrect_after_logs_reopen() {
        let mut state = GitPanelState::default();
        state.toggle_logs_pane();
        state.seed_git_log_for_test("selected");
        let line = state.git_logs.display_line_at(0).unwrap();
        assert!(state.git_logs.set_selection(
            GitLogTextPoint {
                line: line.id(),
                byte: 0,
            },
            GitLogTextPoint {
                line: line.id(),
                byte: line.byte_len(),
            },
        ));

        assert!(!state.owns_git_logs_copy());
        assert_eq!(state.copy_owned_git_logs_selection(), None);

        state.claim_git_logs_copy_owner();
        assert_eq!(
            state.copy_owned_git_logs_selection().as_deref(),
            Some("selected")
        );

        state.toggle_logs_pane();
        assert!(!state.owns_git_logs_copy());
        state.toggle_logs_pane();
        assert!(!state.owns_git_logs_copy());
        assert_eq!(state.copy_owned_git_logs_selection(), None);
    }

    #[test]
    fn git_log_follow_tail_accepts_measured_visual_max_scroll() {
        let measured_content_h = 420.0;
        let max_scroll = git_logs_max_scroll_from_content_height(measured_content_h, 180.0, 1.0);
        assert_eq!(max_scroll, 270.0);

        let mut state = GitPanelState::default();
        state.logs_follow_tail = true;
        assert!(state.update_git_logs_scroll(0.016, max_scroll));
        assert_eq!(state.logs_scroll.target, max_scroll);
        state.scroll_git_logs_by(-80.0, max_scroll);
        assert!(!state.logs_follow_tail);
        let held_target = state.logs_scroll.target;
        state.update_git_logs_scroll(0.016, max_scroll + 200.0);
        assert_eq!(state.logs_scroll.target, held_target);
    }

    #[test]
    fn git_log_truncation_marker_is_safe_selectable_display_text() {
        let mut logs = GitLogBuffer::default();
        let chunk = "x".repeat(1100 * 1024);
        logs.append(info_line(chunk.clone()));
        logs.append(info_line(chunk));
        assert!(logs.snapshot().truncated);
        let marker = logs.display_line_at(0).unwrap();
        let selection = GitLogSelection {
            anchor: GitLogTextPoint {
                line: marker.id(),
                byte: 0,
            },
            cursor: GitLogTextPoint {
                line: marker.id(),
                byte: marker.byte_len(),
            },
        };
        assert_eq!(
            logs.copy_selection(selection).as_deref(),
            Some(GIT_LOG_TRUNCATION_MARKER)
        );
    }
}
