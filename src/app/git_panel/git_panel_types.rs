use crate::app::App;
use crate::editor::Editor;
use rustc_hash::{FxHashMap, FxHashSet};
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
            if seen.insert(repo_root.clone()) {
                roots.push(repo_root.clone());
            }
        }
        roots
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct BranchAheadKey {
    repo_root: PathBuf,
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

pub struct GitPanelState {
    pub snapshot: GitStatusSnapshot,
    pub message_editor: Editor,
    pub message_focused: bool,
    pub amend: bool,
    pub commit_menu_open: bool,
    pub collapsed_workspaces: FxHashSet<usize>,
    pub collapsed_dirs: FxHashMap<usize, FxHashSet<String>>,
    pub scroll: crate::scroll::ScrollState,
    pub pending: bool,
    pub pending_label: Option<&'static str>,
    pub selected_file: Option<(usize, usize)>,
    pending_started_at: Option<std::time::Instant>,
    pending_label_until: Option<std::time::Instant>,
    pub next_request_id: u64,
    pub latest_request_id: u64,
    applied_request_id: u64,
    rx: Vec<GitPanelReceiver>,
    stage_tx: Option<mpsc::Sender<GitStageCommand>>,
    status_refresh_pending: bool,
    status_refresh_dirty: bool,
    branch_ahead_cache: BranchAheadCache,
    pub stage_pending_workspace_idx: Option<usize>,
    pub notice: Option<String>,
    pub confirm_dialog: Option<GitConfirmDialog>,
    pub graph_open: bool,
    pub graph_scroll: crate::scroll::ScrollState,
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
    graph_rx: Vec<mpsc::Receiver<GitGraphEvent>>,
    graph_next_request_id: u64,
    graph_latest_request_id: u64,
    graph_latest_request_by_root: FxHashMap<PathBuf, u64>,
    graph_pending_roots: FxHashSet<PathBuf>,
    graph_cache: FxHashMap<PathBuf, GitGraphCacheEntry>,
    graph_refresh_after_status: bool,
}

struct GitPanelReceiver {
    rx: mpsc::Receiver<GitPanelTaskResult>,
    blocking: bool,
    refresh: bool,
}

struct GitPanelTaskResult {
    event: GitPanelEvent,
    branch_ahead_cache: BranchAheadCache,
}

struct GitActionOutcome {
    notice: Option<String>,
    clear_message: bool,
}

impl Default for GitPanelState {
    fn default() -> Self {
        Self {
            snapshot: GitStatusSnapshot::default(),
            message_editor: Editor::new(512),
            message_focused: false,
            amend: false,
            commit_menu_open: false,
            collapsed_workspaces: FxHashSet::default(),
            collapsed_dirs: FxHashMap::default(),
            scroll: crate::scroll::ScrollState::new(15.0),
            pending: false,
            pending_label: None,
            selected_file: None,
            pending_started_at: None,
            pending_label_until: None,
            next_request_id: 1,
            latest_request_id: 0,
            applied_request_id: 0,
            rx: Vec::new(),
            stage_tx: None,
            status_refresh_pending: false,
            status_refresh_dirty: false,
            branch_ahead_cache: BranchAheadCache::default(),
            stage_pending_workspace_idx: None,
            notice: None,
            confirm_dialog: None,
            graph_open: false,
            graph_scroll: crate::scroll::ScrollState::new(15.0),
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

    fn apply_event(&mut self, event: GitPanelEvent) {
        self.latest_request_id = event.request_id;
        self.applied_request_id = event.request_id;
        self.notice = event.notice;
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
            return;
        }
        self.snapshot = event.snapshot;
        if event.preserve_snapshot_on_empty {
            self.stage_pending_workspace_idx = None;
        }
    }
}

#[derive(Clone, Debug)]
pub struct GitPanelEvent {
    request_id: u64,
    snapshot: GitStatusSnapshot,
    notice: Option<String>,
    preserve_snapshot_on_empty: bool,
    clear_message: bool,
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

struct GitStageCommand {
    request_id: u64,
    files: Vec<GitStageFileCommand>,
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
    Commit {
        repo_roots: Vec<PathBuf>,
        message: String,
        amend: bool,
        push_after: bool,
    },
    RollbackStaged {
        files: Vec<GitStageFileCommand>,
    },
    Push {
        repo_root: PathBuf,
    },
}
