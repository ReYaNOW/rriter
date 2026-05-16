use crate::app::App;
use crate::editor::Editor;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

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
    pub repo_root: PathBuf,
    pub rel_path: String,
    pub old_rel_path: Option<String>,
    pub display_path: String,
    pub depth: usize,
    pub staged: bool,
    pub status: GitFileStatus,
}

#[derive(Clone, Debug)]
pub struct GitTreeRow {
    pub name: String,
    pub path: String,
    pub depth: usize,
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
            for file in workspace.files.iter().filter(|file| file.staged) {
                if seen.insert(file.repo_root.clone()) {
                    roots.push(file.repo_root.clone());
                }
            }
        }
        roots
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitGraphLaneKind {
    Vertical,
    Parent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitGraphLane {
    pub column: usize,
    pub color_idx: usize,
    pub kind: GitGraphLaneKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitGraphRef {
    pub name: String,
    pub is_remote: bool,
}

#[derive(Clone, Debug)]
pub struct GitGraphCommit {
    pub oid: String,
    pub short_oid: String,
    pub summary: String,
    pub branch_name: Option<String>,
    pub author_name: String,
    pub author_email: String,
    pub time_secs: i64,
    pub time_offset: i32,
    pub relative_time: String,
    pub absolute_time: String,
    pub files_changed: usize,
    pub insertions: usize,
    pub deletions: usize,
    pub local_refs: Vec<GitGraphRef>,
    pub remote_refs: Vec<GitGraphRef>,
    pub lanes: Vec<GitGraphLane>,
    pub column: usize,
    pub color_idx: usize,
    pub is_head: bool,
    pub github_url: Option<String>,
    parent_oids: Vec<String>,
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
    pending_started_at: Option<std::time::Instant>,
    pending_label_until: Option<std::time::Instant>,
    pub next_request_id: u64,
    pub latest_request_id: u64,
    rx: Vec<GitPanelReceiver>,
    stage_tx: Option<mpsc::Sender<GitStageCommand>>,
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
    rx: mpsc::Receiver<GitPanelEvent>,
    blocking: bool,
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
            pending_started_at: None,
            pending_label_until: None,
            next_request_id: 1,
            latest_request_id: 0,
            rx: Vec::new(),
            stage_tx: None,
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
    tx: mpsc::Sender<GitPanelEvent>,
}

enum GitAction {
    Refresh,
    LoadGraph {
        workspace_idx: usize,
        repo_root: PathBuf,
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

impl App {
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn refresh_git_panel(&mut self) {
        if self.ide_workspaces.is_empty() {
            self.ide_panel.git.snapshot = GitStatusSnapshot::default();
            self.ide_panel.git.pending = false;
            self.ide_panel.git.pending_label = None;
            self.ide_panel.git.pending_started_at = None;
            self.ide_panel.git.pending_label_until = None;
            return;
        }
        self.ide_panel.git.graph_refresh_after_status = true;
        self.spawn_git_task(GitAction::Refresh);
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn refresh_git_panel_window(&mut self) {
        self.ide_panel.git.commit_menu_open = false;
        self.ide_panel.git.graph_cache.clear();
        self.ide_panel.git.graph_latest_request_by_root.clear();
        self.ide_panel.git.graph_pending_roots.clear();
        self.ide_panel.git.graph_pending = false;
        self.ide_panel.git.graph_snapshot.clear();
        self.ide_panel.git.graph_lane_count = 1;
        self.ide_panel.git.graph_commit_limit = GIT_GRAPH_LIMIT_STEP;
        self.ide_panel.git.graph_has_more = false;
        self.ide_panel.git.graph_copied_commit = None;
        self.ide_panel.git.graph_notice = None;
        self.ide_panel.git.graph_refresh_after_status = !self.ide_workspaces.is_empty();
        self.refresh_git_panel();
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn poll_git_panel(&mut self) -> bool {
        let now = std::time::Instant::now();
        let mut updated = false;
        let mut stale_seen = false;
        let mut reload_graph_cache = false;
        let mut prefetch_graph_after_status = false;
        let mut next_rx = Vec::with_capacity(self.ide_panel.git.rx.len());
        let receivers = std::mem::take(&mut self.ide_panel.git.rx);
        for receiver in receivers {
            let mut keep = true;
            loop {
                match receiver.rx.try_recv() {
                    Ok(event) => {
                        if event.request_id >= self.ide_panel.git.latest_request_id {
                            let reload_graph = event
                                .notice
                                .as_deref()
                                .is_some_and(|notice| notice.starts_with("Committed "));
                            self.ide_panel.git.apply_event(event);
                            self.ide_panel.git.pending = false;
                            if reload_graph {
                                reload_graph_cache = true;
                                prefetch_graph_after_status = true;
                            } else if self.ide_panel.git.graph_refresh_after_status {
                                prefetch_graph_after_status = true;
                            }
                            updated = true;
                        } else {
                            stale_seen = true;
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        keep = false;
                        break;
                    }
                }
            }
            if keep {
                next_rx.push(receiver);
            }
        }
        self.ide_panel.git.rx = next_rx;
        self.ide_panel.git.pending = self.ide_panel.git.rx.iter().any(|rx| rx.blocking);
        if !self.ide_panel.git.pending {
            self.ide_panel.git.stage_pending_workspace_idx = None;
            self.ide_panel.git.pending_started_at = None;
            if self
                .ide_panel
                .git
                .pending_label_until
                .is_none_or(|until| now >= until)
            {
                if self.ide_panel.git.pending_label.take().is_some() {
                    updated = true;
                }
                self.ide_panel.git.pending_label_until = None;
            } else {
                updated = true;
            }
        }
        if stale_seen && self.ide_panel.git.rx.is_empty() {
            self.spawn_git_task(GitAction::Refresh);
        }
        if reload_graph_cache {
            self.ide_panel.git.graph_cache.clear();
            self.ide_panel.git.graph_latest_request_by_root.clear();
            self.ide_panel.git.graph_pending_roots.clear();
            self.ide_panel.git.graph_snapshot.clear();
            self.ide_panel.git.graph_lane_count = 1;
            self.ide_panel.git.graph_has_more = false;
            self.ide_panel.git.graph_pending = false;
        }
        if prefetch_graph_after_status {
            self.ide_panel.git.graph_refresh_after_status = false;
            self.prefetch_git_graph_for_known_workspaces(true);
        }
        let mut next_graph_rx = Vec::with_capacity(self.ide_panel.git.graph_rx.len());
        let graph_receivers = std::mem::take(&mut self.ide_panel.git.graph_rx);
        for rx in graph_receivers {
            let mut keep = true;
            loop {
                match rx.try_recv() {
                    Ok(event) => {
                        let latest_for_root = self
                            .ide_panel
                            .git
                            .graph_latest_request_by_root
                            .get(&event.repo_root)
                            .copied();
                        if latest_for_root == Some(event.request_id) {
                            self.ide_panel.git.graph_latest_request_id = self
                                .ide_panel
                                .git
                                .graph_latest_request_id
                                .max(event.request_id);
                            self.ide_panel
                                .git
                                .graph_latest_request_by_root
                                .remove(&event.repo_root);
                            self.ide_panel
                                .git
                                .graph_pending_roots
                                .remove(&event.repo_root);
                            let cache_entry = GitGraphCacheEntry {
                                commits: event.commits,
                                lane_count: event.lane_count.max(1),
                                notice: event.notice,
                                limit: event.limit,
                                has_more: event.has_more,
                            };
                            self.ide_panel
                                .git
                                .graph_cache
                                .insert(event.repo_root.clone(), cache_entry.clone());
                            let same_workspace =
                                self.ide_panel.git.graph_workspace_idx == Some(event.workspace_idx);
                            let same_root = self
                                .ide_panel
                                .git
                                .graph_repo_root
                                .as_ref()
                                .is_some_and(|root| root == &event.repo_root);
                            if same_workspace && same_root {
                                self.apply_git_graph_cache_entry(cache_entry, event.reset_scroll);
                            }
                            updated = true;
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        keep = false;
                        break;
                    }
                }
            }
            if keep {
                next_graph_rx.push(rx);
            }
        }
        self.ide_panel.git.graph_rx = next_graph_rx;
        self.ide_panel.git.graph_pending = self
            .ide_panel
            .git
            .graph_repo_root
            .as_ref()
            .is_some_and(|root| self.ide_panel.git.graph_pending_roots.contains(root));
        updated
    }

    pub fn toggle_git_graph(&mut self) {
        self.ide_panel.git.commit_menu_open = false;
        self.ide_panel.git.graph_open = !self.ide_panel.git.graph_open;
        if self.ide_panel.git.graph_open {
            self.ensure_git_graph_loaded();
        }
    }

    pub fn select_git_graph_workspace(&mut self, workspace_idx: usize) {
        self.ide_panel.git.commit_menu_open = false;
        if self.ide_panel.git.graph_workspace_idx == Some(workspace_idx) {
            return;
        }
        let Some(repo_root) = self
            .ide_panel
            .git
            .snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_idx == workspace_idx)
            .and_then(|workspace| workspace.repo_root.clone())
        else {
            self.ide_panel.git.graph_notice = Some("No Git repo".to_string());
            return;
        };
        self.ide_panel.git.graph_workspace_idx = Some(workspace_idx);
        self.ide_panel.git.graph_repo_root = Some(repo_root);
        self.ide_panel.git.graph_snapshot.clear();
        self.ide_panel.git.graph_lane_count = 1;
        self.ide_panel.git.graph_commit_limit = GIT_GRAPH_LIMIT_STEP;
        self.ide_panel.git.graph_has_more = false;
        self.ide_panel.git.graph_copied_commit = None;
        self.ide_panel.git.graph_scroll.current = 0.0;
        self.ide_panel.git.graph_scroll.target = 0.0;
        if !self.apply_cached_git_graph_for_selected(true) {
            self.load_git_graph_for_selected_workspace();
        }
    }

    pub fn copy_git_graph_commit(&mut self, workspace_idx: usize, commit_idx: usize) {
        if self.ide_panel.git.graph_workspace_idx != Some(workspace_idx) {
            return;
        }
        let Some(oid) = self
            .ide_panel
            .git
            .graph_snapshot
            .get(commit_idx)
            .map(|commit| commit.oid.clone())
        else {
            return;
        };
        self.set_clipboard_text(oid);
        self.ide_panel.git.graph_copied_commit = Some((workspace_idx, commit_idx));
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.git_graph_tooltip_seen_copied = Some((workspace_idx, commit_idx));
            renderer.git_graph_tooltip_visible_copied = Some((workspace_idx, commit_idx));
        }
    }

    pub fn open_git_graph_commit(&mut self, workspace_idx: usize, commit_idx: usize) {
        if self.ide_panel.git.graph_workspace_idx != Some(workspace_idx) {
            return;
        }
        let Some(url) = self
            .ide_panel
            .git
            .graph_snapshot
            .get(commit_idx)
            .and_then(|commit| commit.github_url.clone())
        else {
            self.ide_panel.git.graph_notice = Some("No GitHub remote".to_string());
            return;
        };
        match open_url_async(&url) {
            Ok(()) => {
                self.ide_panel.git.graph_notice = Some("Opening GitHub".to_string());
            }
            Err(err) => {
                self.ide_panel.git.graph_notice = Some(err);
            }
        }
    }

    fn apply_git_graph_cache_entry(&mut self, cache_entry: GitGraphCacheEntry, reset_scroll: bool) {
        self.ide_panel.git.graph_snapshot = cache_entry.commits;
        self.ide_panel.git.graph_lane_count = cache_entry.lane_count.max(1);
        self.ide_panel.git.graph_notice = cache_entry.notice;
        self.ide_panel.git.graph_commit_limit = cache_entry.limit;
        self.ide_panel.git.graph_has_more = cache_entry.has_more;
        self.ide_panel.git.graph_pending = false;
        if reset_scroll {
            self.ide_panel.git.graph_scroll.set_target(0.0);
            self.ide_panel.git.graph_scroll.current = 0.0;
            self.ide_panel.git.graph_scroll.velocity = 0.0;
        }
    }

    fn apply_cached_git_graph_for_selected(&mut self, reset_scroll: bool) -> bool {
        let Some(repo_root) = self.ide_panel.git.graph_repo_root.clone() else {
            return false;
        };
        let Some(cache_entry) = self.ide_panel.git.graph_cache.get(&repo_root).cloned() else {
            return false;
        };
        self.apply_git_graph_cache_entry(cache_entry, reset_scroll);
        true
    }

    fn ensure_git_graph_loaded(&mut self) {
        if self.ide_panel.git.graph_workspace_idx.is_none()
            || self.ide_panel.git.graph_workspace_idx.is_some_and(|idx| {
                !self
                    .ide_panel
                    .git
                    .snapshot
                    .workspaces
                    .iter()
                    .any(|workspace| {
                        workspace.workspace_idx == idx && workspace.repo_root.is_some()
                    })
            })
        {
            if let Some((workspace_idx, repo_root)) = self
                .ide_panel
                .git
                .snapshot
                .workspaces
                .iter()
                .find_map(|workspace| {
                    workspace
                        .repo_root
                        .as_ref()
                        .map(|repo_root| (workspace.workspace_idx, repo_root.clone()))
                })
            {
                self.ide_panel.git.graph_workspace_idx = Some(workspace_idx);
                self.ide_panel.git.graph_repo_root = Some(repo_root);
                self.ide_panel.git.graph_snapshot.clear();
                self.ide_panel.git.graph_lane_count = 1;
                self.ide_panel.git.graph_commit_limit = GIT_GRAPH_LIMIT_STEP;
                self.ide_panel.git.graph_has_more = false;
            }
        }
        if self.ide_panel.git.graph_snapshot.is_empty()
            && self.apply_cached_git_graph_for_selected(true)
        {
            return;
        }
        if self.ide_panel.git.graph_snapshot.is_empty() && !self.ide_panel.git.graph_pending {
            self.load_git_graph_for_selected_workspace();
        }
    }

    fn load_git_graph_for_selected_workspace(&mut self) {
        let Some(workspace_idx) = self.ide_panel.git.graph_workspace_idx else {
            self.ide_panel.git.graph_notice = Some("No Git workspace".to_string());
            return;
        };
        let Some(repo_root) = self.ide_panel.git.graph_repo_root.clone() else {
            self.ide_panel.git.graph_notice = Some("No Git repo".to_string());
            return;
        };
        if self.ide_panel.git.graph_pending_roots.contains(&repo_root) {
            self.ide_panel.git.graph_pending = true;
            return;
        }
        self.spawn_git_task(GitAction::LoadGraph {
            workspace_idx,
            repo_root,
            limit: self.ide_panel.git.graph_commit_limit,
            reset_scroll: self.ide_panel.git.graph_snapshot.is_empty(),
            activate: true,
        });
    }

    pub fn load_more_git_graph_commits(&mut self) {
        if self.ide_panel.git.graph_pending || !self.ide_panel.git.graph_has_more {
            return;
        }
        let Some(workspace_idx) = self.ide_panel.git.graph_workspace_idx else {
            return;
        };
        let Some(repo_root) = self.ide_panel.git.graph_repo_root.clone() else {
            return;
        };
        self.ide_panel.git.graph_commit_limit = self
            .ide_panel
            .git
            .graph_commit_limit
            .saturating_add(GIT_GRAPH_LIMIT_STEP);
        self.spawn_git_task(GitAction::LoadGraph {
            workspace_idx,
            repo_root,
            limit: self.ide_panel.git.graph_commit_limit,
            reset_scroll: false,
            activate: true,
        });
    }

    fn prefetch_git_graph_for_repo(
        &mut self,
        workspace_idx: usize,
        repo_root: PathBuf,
        limit: usize,
        force_reload: bool,
    ) {
        if self
            .ide_panel
            .git
            .graph_latest_request_by_root
            .contains_key(&repo_root)
        {
            return;
        }
        if !force_reload
            && self
                .ide_panel
                .git
                .graph_cache
                .get(&repo_root)
                .is_some_and(|cache| cache.limit >= limit)
        {
            return;
        }
        self.spawn_git_task(GitAction::LoadGraph {
            workspace_idx,
            repo_root,
            limit,
            reset_scroll: false,
            activate: false,
        });
    }

    fn prefetch_git_graph_for_known_workspaces(&mut self, force_reload: bool) {
        let mut seen = FxHashSet::default();
        let workspaces = self
            .ide_panel
            .git
            .snapshot
            .workspaces
            .iter()
            .filter_map(|workspace| {
                let repo_root = workspace.repo_root.clone()?;
                seen.insert(repo_root.clone())
                    .then_some((workspace.workspace_idx, repo_root))
            })
            .collect::<Vec<_>>();
        for (workspace_idx, repo_root) in workspaces {
            let limit = if self.ide_panel.git.graph_repo_root.as_ref() == Some(&repo_root) {
                self.ide_panel.git.graph_commit_limit
            } else {
                GIT_GRAPH_LIMIT_STEP
            };
            self.prefetch_git_graph_for_repo(workspace_idx, repo_root, limit, force_reload);
        }
    }

    pub(crate) fn prefetch_active_tab_git_graph(&mut self) {
        if !self.is_ide_mode || !self.is_ready || !self.ide_panel.is_open(crate::app::PanelId::Git)
        {
            return;
        }
        let Some(file_path) = self.file_path.as_ref() else {
            return;
        };
        let abs_path = git_abs_path_for_workspaces(file_path, &self.ide_workspaces);
        let Some((workspace_idx, repo_root)) =
            git_graph_workspace_for_path(&self.ide_panel.git.snapshot, &abs_path)
        else {
            return;
        };
        self.prefetch_git_graph_for_repo(workspace_idx, repo_root, GIT_GRAPH_LIMIT_STEP, false);
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn toggle_git_file_stage(&mut self, workspace_idx: usize, file_idx: usize) {
        if git_stage_click_locked(&self.ide_panel.git, workspace_idx) {
            return;
        }
        if self
            .ide_panel
            .git
            .snapshot
            .active_staged_workspace_idx()
            .is_some_and(|idx| idx != workspace_idx)
        {
            return;
        }
        let Some(file) = self
            .ide_panel
            .git
            .snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_idx == workspace_idx)
            .and_then(|workspace| workspace.files.get(file_idx))
            .cloned()
        else {
            return;
        };
        if let Some(file_mut) = self
            .ide_panel
            .git
            .snapshot
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.workspace_idx == workspace_idx)
            .and_then(|workspace| workspace.files.get_mut(file_idx))
        {
            file_mut.staged = !file.staged;
        }
        self.ide_panel.git.stage_pending_workspace_idx = Some(workspace_idx);
        self.spawn_git_task(GitAction::ToggleStageMany {
            files: vec![GitStageFileCommand {
                repo_root: file.repo_root,
                rel_path: file.rel_path,
                old_rel_path: file.old_rel_path,
                staged: file.staged,
            }],
        });
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn toggle_git_folder_stage(&mut self, workspace_idx: usize, row_idx: usize) {
        if git_stage_click_locked(&self.ide_panel.git, workspace_idx) {
            return;
        }
        if self
            .ide_panel
            .git
            .snapshot
            .active_staged_workspace_idx()
            .is_some_and(|idx| idx != workspace_idx)
        {
            return;
        }

        let Some((file_indices, target_staged, files)) = self
            .ide_panel
            .git
            .snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_idx == workspace_idx)
            .map(|workspace| {
                let file_indices = git_folder_file_indices(workspace, row_idx);
                let all_staged = !file_indices.is_empty()
                    && file_indices
                        .iter()
                        .all(|idx| workspace.files.get(*idx).is_some_and(|file| file.staged));
                let target_staged = !all_staged;
                let files = file_indices
                    .iter()
                    .filter_map(|idx| workspace.files.get(*idx))
                    .filter(|file| file.staged != target_staged)
                    .map(|file| GitStageFileCommand {
                        repo_root: file.repo_root.clone(),
                        rel_path: file.rel_path.clone(),
                        old_rel_path: file.old_rel_path.clone(),
                        staged: file.staged,
                    })
                    .collect::<Vec<_>>();
                (file_indices, target_staged, files)
            })
        else {
            return;
        };
        if files.is_empty() {
            return;
        }

        if let Some(workspace) = self
            .ide_panel
            .git
            .snapshot
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.workspace_idx == workspace_idx)
        {
            for idx in file_indices {
                if let Some(file) = workspace.files.get_mut(idx) {
                    file.staged = target_staged;
                }
            }
        }

        self.ide_panel.git.stage_pending_workspace_idx = Some(workspace_idx);
        self.spawn_git_task(GitAction::ToggleStageMany { files });
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn commit_git_panel(&mut self) {
        self.commit_git_panel_with(false, false);
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn commit_git_panel_option(&mut self, option_idx: usize) {
        match option_idx {
            0 => self.commit_git_panel_with(false, false),
            1 => self.commit_git_panel_with(true, false),
            2 => self.commit_git_panel_with(false, true),
            _ => {}
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn commit_git_panel_with(&mut self, amend: bool, push_after: bool) {
        if self.ide_panel.git.pending {
            return;
        }
        self.ide_panel.git.commit_menu_open = false;
        self.ide_panel.git.message_focused = false;
        let message = self.ide_panel.git.message_editor.get_full_text();
        let trimmed = message.trim();
        if trimmed.is_empty() {
            self.ide_panel.git.notice = Some("Commit message empty".to_string());
            return;
        }
        let repo_roots = self.ide_panel.git.snapshot.staged_repo_roots();
        if repo_roots.is_empty() {
            self.ide_panel.git.notice = Some("No staged files".to_string());
            return;
        }
        self.spawn_git_task(GitAction::Commit {
            repo_roots,
            message: trimmed.to_string(),
            amend,
            push_after,
        });
        self.ide_panel.git.message_editor = Editor::new(512);
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn push_git_workspace(&mut self, workspace_idx: usize) {
        if self.ide_panel.git.pending {
            return;
        }
        let Some(repo_root) = self
            .ide_panel
            .git
            .snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_idx == workspace_idx)
            .and_then(|workspace| workspace.repo_root.clone())
        else {
            return;
        };
        self.spawn_git_task(GitAction::Push { repo_root });
    }

    pub fn open_git_rollback_staged_dialog(&mut self, workspace_idx: usize) {
        self.open_git_confirm_dialog(workspace_idx, GitConfirmAction::RollbackStaged);
    }

    pub fn open_git_unstage_all_dialog(&mut self, workspace_idx: usize) {
        self.unstage_all_git_workspace(workspace_idx);
    }

    pub fn stage_all_git_workspace(&mut self, workspace_idx: usize) {
        if git_stage_click_locked(&self.ide_panel.git, workspace_idx) {
            return;
        }
        if self
            .ide_panel
            .git
            .snapshot
            .active_staged_workspace_idx()
            .is_some_and(|idx| idx != workspace_idx)
        {
            return;
        }

        let Some((file_indices, files)) = self
            .ide_panel
            .git
            .snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_idx == workspace_idx)
            .map(|workspace| {
                let file_indices = workspace
                    .files
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, file)| (!file.staged).then_some(idx))
                    .collect::<Vec<_>>();
                let files = file_indices
                    .iter()
                    .filter_map(|idx| workspace.files.get(*idx))
                    .map(|file| GitStageFileCommand {
                        repo_root: file.repo_root.clone(),
                        rel_path: file.rel_path.clone(),
                        old_rel_path: file.old_rel_path.clone(),
                        staged: false,
                    })
                    .collect::<Vec<_>>();
                (file_indices, files)
            })
        else {
            return;
        };
        if files.is_empty() {
            self.ide_panel.git.notice = Some("No unstaged files".to_string());
            return;
        }

        if let Some(workspace) = self
            .ide_panel
            .git
            .snapshot
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.workspace_idx == workspace_idx)
        {
            for idx in file_indices {
                if let Some(file) = workspace.files.get_mut(idx) {
                    file.staged = true;
                }
            }
        }
        self.ide_panel.git.stage_pending_workspace_idx = Some(workspace_idx);
        self.spawn_git_task(GitAction::ToggleStageMany { files });
    }

    pub fn unstage_all_git_workspace(&mut self, workspace_idx: usize) {
        if git_stage_click_locked(&self.ide_panel.git, workspace_idx) {
            return;
        }
        let files = git_staged_confirm_files(&self.ide_panel.git.snapshot, workspace_idx)
            .into_iter()
            .map(|file| GitStageFileCommand {
                repo_root: file.repo_root,
                rel_path: file.rel_path,
                old_rel_path: file.old_rel_path,
                staged: true,
            })
            .collect::<Vec<_>>();
        if files.is_empty() {
            self.ide_panel.git.notice = Some("No staged files".to_string());
            return;
        }

        if let Some(workspace) = self
            .ide_panel
            .git
            .snapshot
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.workspace_idx == workspace_idx)
        {
            for file in &mut workspace.files {
                if file.staged {
                    file.staged = false;
                }
            }
        }
        self.ide_panel.git.stage_pending_workspace_idx = Some(workspace_idx);
        self.spawn_git_task(GitAction::ToggleStageMany { files });
    }

    fn open_git_confirm_dialog(&mut self, workspace_idx: usize, action: GitConfirmAction) {
        if self.ide_panel.git.pending {
            return;
        }
        let files = git_staged_confirm_files(&self.ide_panel.git.snapshot, workspace_idx);
        if files.is_empty() {
            self.ide_panel.git.notice = Some("No staged files".to_string());
            return;
        }
        self.ide_panel.git.commit_menu_open = false;
        self.ide_panel.git.message_focused = false;
        self.ide_panel.git.confirm_dialog = Some(GitConfirmDialog {
            action,
            workspace_idx,
            files,
        });
    }

    pub fn confirm_git_dialog(&mut self) {
        if self.ide_panel.git.pending {
            return;
        }
        let Some(dialog) = self.ide_panel.git.confirm_dialog.take() else {
            return;
        };
        let files = dialog
            .files
            .into_iter()
            .map(|file| GitStageFileCommand {
                repo_root: file.repo_root,
                rel_path: file.rel_path,
                old_rel_path: file.old_rel_path,
                staged: true,
            })
            .collect::<Vec<_>>();
        if files.is_empty() {
            self.ide_panel.git.notice = Some("No staged files".to_string());
            return;
        }

        match dialog.action {
            GitConfirmAction::RollbackStaged => {
                self.spawn_git_task(GitAction::RollbackStaged { files });
            }
        }
    }

    pub fn toggle_git_workspace(&mut self, workspace_idx: usize) {
        if !self
            .ide_panel
            .git
            .snapshot
            .workspaces
            .iter()
            .any(|workspace| {
                workspace.workspace_idx == workspace_idx && workspace.has_collapsible_rows()
            })
        {
            return;
        }
        if !self
            .ide_panel
            .git
            .collapsed_workspaces
            .remove(&workspace_idx)
        {
            self.ide_panel
                .git
                .collapsed_workspaces
                .insert(workspace_idx);
        }
    }

    pub fn toggle_git_tree_folder(&mut self, workspace_idx: usize, row_idx: usize) {
        let Some(row) = self
            .ide_panel
            .git
            .snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_idx == workspace_idx)
            .and_then(|workspace| workspace.tree.get(row_idx))
        else {
            return;
        };
        if row.file_idx.is_some() {
            return;
        }
        let dirs = self
            .ide_panel
            .git
            .collapsed_dirs
            .entry(workspace_idx)
            .or_default();
        if !dirs.remove(row.path.as_str()) {
            dirs.insert(row.path.clone());
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn spawn_git_task(&mut self, action: GitAction) {
        if let GitAction::LoadGraph {
            workspace_idx,
            repo_root,
            limit,
            reset_scroll,
            activate,
        } = action
        {
            let request_id = self.ide_panel.git.graph_next_request_id;
            self.ide_panel.git.graph_next_request_id =
                self.ide_panel.git.graph_next_request_id.saturating_add(1);
            self.ide_panel.git.graph_latest_request_id =
                self.ide_panel.git.graph_latest_request_id.max(request_id);
            self.ide_panel
                .git
                .graph_latest_request_by_root
                .insert(repo_root.clone(), request_id);
            self.ide_panel
                .git
                .graph_pending_roots
                .insert(repo_root.clone());
            if activate {
                self.ide_panel.git.graph_pending = true;
                self.ide_panel.git.graph_notice = None;
                self.ide_panel.git.graph_repo_root = Some(repo_root.clone());
                self.ide_panel.git.graph_workspace_idx = Some(workspace_idx);
                self.ide_panel.git.graph_commit_limit = limit;
            } else if self.ide_panel.git.graph_repo_root.as_ref() == Some(&repo_root) {
                self.ide_panel.git.graph_pending = true;
            }

            let (tx, rx) = mpsc::channel();
            self.ide_panel.git.graph_rx.push(rx);
            std::thread::spawn(move || {
                let (commits, lane_count, has_more, notice) =
                    match collect_git_graph(workspace_idx, &repo_root, limit) {
                        Ok((commits, lane_count, has_more)) => {
                            (commits, lane_count, has_more, None)
                        }
                        Err(err) => (Vec::new(), 1, false, Some(err)),
                    };
                let _ = tx.send(GitGraphEvent {
                    request_id,
                    workspace_idx,
                    repo_root,
                    commits,
                    lane_count,
                    notice,
                    limit,
                    has_more,
                    reset_scroll,
                });
            });
            return;
        }

        let request_id = self.ide_panel.git.next_request_id;
        self.ide_panel.git.next_request_id = self.ide_panel.git.next_request_id.saturating_add(1);
        self.ide_panel.git.latest_request_id = self.ide_panel.git.latest_request_id.max(request_id);
        let blocking = !matches!(&action, GitAction::Refresh);
        if blocking {
            let now = std::time::Instant::now();
            self.ide_panel.git.pending = true;
            self.ide_panel.git.pending_label = match &action {
                GitAction::Commit {
                    push_after: true, ..
                } => Some("Commit & Push"),
                GitAction::Commit { .. } => Some("Commit"),
                GitAction::Push { .. } => Some("Push"),
                _ => None,
            };
            self.ide_panel.git.pending_started_at = Some(now);
            self.ide_panel.git.pending_label_until = self
                .ide_panel
                .git
                .pending_label
                .map(|_| now + std::time::Duration::from_secs(1));
        }
        self.ide_panel.git.notice = None;

        let workspaces = self.ide_workspaces.clone();
        let (tx, rx) = mpsc::channel();
        self.ide_panel
            .git
            .rx
            .push(GitPanelReceiver { rx, blocking });

        if let GitAction::ToggleStageMany { files } = action {
            let mut command = Some(GitStageCommand {
                request_id,
                files,
                workspaces,
                tx,
            });
            if let Some(stage_tx) = &self.ide_panel.git.stage_tx {
                match stage_tx.send(command.take().unwrap()) {
                    Ok(()) => return,
                    Err(err) => command = Some(err.0),
                }
            }

            let (stage_tx, stage_rx) = mpsc::channel();
            self.ide_panel.git.stage_tx = Some(stage_tx.clone());
            std::thread::spawn(move || {
                for command in stage_rx {
                    let notice = run_stage_files(&command.files);
                    let snapshot = collect_git_status(&command.workspaces);
                    let _ = command.tx.send(GitPanelEvent {
                        request_id: command.request_id,
                        snapshot,
                        notice,
                        preserve_snapshot_on_empty: true,
                        clear_message: false,
                    });
                }
            });
            if let Some(command) = command {
                let _ = stage_tx.send(command);
            }
            return;
        }

        std::thread::spawn(move || {
            let outcome = run_git_action(action);
            let snapshot = collect_git_status(&workspaces);
            let _ = tx.send(GitPanelEvent {
                request_id,
                snapshot,
                notice: outcome.notice,
                preserve_snapshot_on_empty: false,
                clear_message: outcome.clear_message,
            });
        });
    }
}

fn git_stage_click_locked(state: &GitPanelState, workspace_idx: usize) -> bool {
    state.pending
        || state
            .staged_workspace_lock()
            .is_some_and(|idx| idx != workspace_idx)
}

fn run_git_action(action: GitAction) -> GitActionOutcome {
    match action {
        GitAction::Refresh | GitAction::LoadGraph { .. } => GitActionOutcome {
            notice: None,
            clear_message: false,
        },
        GitAction::ToggleStageMany { files } => GitActionOutcome {
            notice: run_stage_files(&files),
            clear_message: false,
        },
        GitAction::Commit {
            repo_roots,
            message,
            amend,
            push_after,
        } => {
            let mut ok = 0usize;
            let mut errors = Vec::new();
            for repo_root in repo_roots {
                match commit_repo(&repo_root, &message, amend) {
                    Ok(()) => {
                        ok += 1;
                        if push_after && let Err(err) = push_repo(&repo_root) {
                            errors.push(err);
                        }
                    }
                    Err(err) => errors.push(err),
                }
            }
            if errors.is_empty() {
                GitActionOutcome {
                    notice: Some(format!("Committed {ok} repo(s)")),
                    clear_message: ok > 0,
                }
            } else if ok > 0 {
                GitActionOutcome {
                    notice: Some(format!("Committed {ok} repo(s); {}", errors.join(" | "))),
                    clear_message: true,
                }
            } else {
                GitActionOutcome {
                    notice: Some(errors.join(" | ")),
                    clear_message: false,
                }
            }
        }
        GitAction::RollbackStaged { files } => GitActionOutcome {
            notice: rollback_staged_files(&files),
            clear_message: false,
        },
        GitAction::Push { repo_root } => GitActionOutcome {
            notice: match push_repo(&repo_root) {
                Ok(()) => Some("Push done".to_string()),
                Err(err) => Some(err),
            },
            clear_message: false,
        },
    }
}

fn open_url_async(url: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn();

    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(url).spawn();

    #[cfg(all(unix, not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(url).spawn();

    result.map(|_| ()).map_err(|err| err.to_string())
}

fn git_snapshot_has_visible_rows(snapshot: &GitStatusSnapshot) -> bool {
    !snapshot.workspaces.is_empty()
}

fn git_abs_path_for_workspaces(path: &Path, workspaces: &[PathBuf]) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else if let Some(workspace) = workspaces.first() {
        workspace.join(path)
    } else {
        path.to_path_buf()
    }
}

fn git_graph_workspace_for_path(
    snapshot: &GitStatusSnapshot,
    abs_path: &Path,
) -> Option<(usize, PathBuf)> {
    snapshot.workspaces.iter().find_map(|workspace| {
        let repo_root = workspace.repo_root.as_ref()?;
        (abs_path.starts_with(&workspace.root) || abs_path.starts_with(repo_root))
            .then(|| (workspace.workspace_idx, repo_root.clone()))
    })
}

fn merge_stage_snapshot(current: &mut GitStatusSnapshot, next: GitStatusSnapshot) {
    for current_workspace in &mut current.workspaces {
        let Some(next_workspace) = next
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_idx == current_workspace.workspace_idx)
        else {
            continue;
        };
        current_workspace.ahead = next_workspace.ahead;
        current_workspace.error = next_workspace.error.clone();
        current_workspace.repo_root = next_workspace.repo_root.clone();
        current_workspace.branch_name = next_workspace.branch_name.clone();

        let mut next_files = FxHashMap::default();
        for file in &next_workspace.files {
            next_files.insert(file.display_path.as_str(), file);
        }
        current_workspace
            .files
            .retain(|file| next_files.contains_key(file.display_path.as_str()));
        for file in &mut current_workspace.files {
            if let Some(next_file) = next_files.get(file.display_path.as_str()) {
                file.repo_root.clone_from(&next_file.repo_root);
                file.rel_path.clone_from(&next_file.rel_path);
                file.old_rel_path.clone_from(&next_file.old_rel_path);
                file.staged = next_file.staged;
                file.status = next_file.status;
            }
        }
        current_workspace.tree = build_git_tree(&current_workspace.files);
    }
}

fn git_staged_confirm_files(
    snapshot: &GitStatusSnapshot,
    workspace_idx: usize,
) -> Vec<GitConfirmFile> {
    snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_idx == workspace_idx)
        .map(|workspace| {
            workspace
                .files
                .iter()
                .filter(|file| file.staged)
                .map(|file| GitConfirmFile {
                    repo_root: file.repo_root.clone(),
                    rel_path: file.rel_path.clone(),
                    old_rel_path: file.old_rel_path.clone(),
                    display_path: file.display_path.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn run_stage_files(files: &[GitStageFileCommand]) -> Option<String> {
    let mut errors = Vec::new();
    for file in files {
        if let Err(err) = toggle_stage(
            &file.repo_root,
            &file.rel_path,
            file.old_rel_path.as_deref(),
            file.staged,
        ) {
            errors.push(err);
        }
    }
    if errors.is_empty() {
        None
    } else {
        Some(errors.join(" | "))
    }
}

fn rollback_staged_files(files: &[GitStageFileCommand]) -> Option<String> {
    let mut errors = Vec::new();
    for file in files {
        if let Err(err) = rollback_staged_file(
            &file.repo_root,
            &file.rel_path,
            file.old_rel_path.as_deref(),
        ) {
            errors.push(err);
        }
    }
    if errors.is_empty() {
        Some(format!("Rolled back {} staged file(s)", files.len()))
    } else {
        Some(errors.join(" | "))
    }
}

const GIT_GRAPH_LIMIT_STEP: usize = 200;

fn collect_git_graph(
    _workspace_idx: usize,
    repo_root: &Path,
    limit: usize,
) -> Result<(Vec<GitGraphCommit>, usize, bool), String> {
    let repo = git2::Repository::open(repo_root).map_err(short_git_error)?;
    let refs_by_oid = collect_git_graph_refs(&repo);
    let trace_labels_by_oid = collect_git_graph_trace_labels(&repo);
    let head_oid = repo.head().ok().and_then(|head| head.target());
    let github_base_url = repo
        .find_remote("origin")
        .ok()
        .and_then(|remote| remote.url().and_then(github_base_url_from_remote_url));
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);

    let mut revwalk = repo.revwalk().map_err(short_git_error)?;
    revwalk
        .set_sorting(git2::Sort::TIME | git2::Sort::TOPOLOGICAL)
        .map_err(short_git_error)?;
    revwalk.push_head().map_err(short_git_error)?;

    let mut commits = Vec::with_capacity(limit.min(GIT_GRAPH_LIMIT_STEP));
    let mut has_more = false;
    for (idx, oid_result) in revwalk.take(limit.saturating_add(1)).enumerate() {
        if idx >= limit {
            has_more = true;
            break;
        }
        let oid = oid_result.map_err(short_git_error)?;
        let commit = repo.find_commit(oid).map_err(short_git_error)?;
        let author = commit.author();
        let time = commit.time();
        let oid_text = oid.to_string();
        let parent_oids = commit
            .parents()
            .map(|parent| parent.id().to_string())
            .collect::<Vec<_>>();
        let mut local_refs = Vec::new();
        let mut remote_refs = Vec::new();
        if let Some(refs) = refs_by_oid.get(oid_text.as_str()) {
            for git_ref in refs {
                if git_ref.is_remote {
                    remote_refs.push(git_ref.clone());
                } else {
                    local_refs.push(git_ref.clone());
                }
            }
        }
        local_refs.sort_by(|a, b| a.name.cmp(&b.name));
        remote_refs.sort_by(|a, b| a.name.cmp(&b.name));

        let raw_summary = commit.summary().unwrap_or("(no message)");
        let summary = clean_git_summary(raw_summary);
        let message = commit.message().unwrap_or(raw_summary);
        let (files_changed, insertions, deletions) = git_commit_stats(&repo, &commit);
        let branch_name = git_graph_branch_label(&local_refs, &remote_refs)
            .or_else(|| git_graph_change_request_label(message))
            .or_else(|| trace_labels_by_oid.get(oid_text.as_str()).cloned());
        commits.push(GitGraphCommit {
            oid: oid_text.clone(),
            short_oid: oid_text.chars().take(7).collect(),
            summary,
            branch_name,
            author_name: author.name().unwrap_or("Unknown").to_string(),
            author_email: author.email().unwrap_or("").to_string(),
            time_secs: time.seconds(),
            time_offset: time.offset_minutes(),
            relative_time: format_git_relative_time(time.seconds(), now_secs),
            absolute_time: format_git_absolute_time(time.seconds(), time.offset_minutes()),
            files_changed,
            insertions,
            deletions,
            local_refs,
            remote_refs,
            lanes: Vec::new(),
            column: 0,
            color_idx: 0,
            is_head: head_oid == Some(oid),
            github_url: github_base_url
                .as_ref()
                .map(|base_url| format!("{base_url}/commit/{oid_text}")),
            parent_oids,
        });
    }

    let lane_count = apply_git_graph_lanes(&mut commits);
    Ok((commits, lane_count, has_more))
}

fn git_graph_branch_label(
    local_refs: &[GitGraphRef],
    remote_refs: &[GitGraphRef],
) -> Option<String> {
    local_refs
        .first()
        .map(|git_ref| git_ref.name.clone())
        .or_else(|| {
            remote_refs.first().map(|git_ref| {
                git_ref
                    .name
                    .split_once('/')
                    .map(|(_, branch)| branch)
                    .unwrap_or(git_ref.name.as_str())
                    .to_string()
            })
        })
}

fn clean_git_summary(summary: &str) -> String {
    let cleaned = summary
        .trim_matches(|ch: char| {
            ch.is_whitespace()
                || matches!(
                    ch,
                    '\u{feff}' | '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{2060}'
                )
        })
        .to_string();
    if cleaned.is_empty() {
        "(no message)".to_string()
    } else {
        cleaned
    }
}

fn git_graph_merge_source_label(summary: &str) -> Option<String> {
    let source = git_graph_merge_source(summary)?;
    (!source.is_empty()).then(|| format!("merged from {source}"))
}

fn git_graph_merge_side_parent_label(summary: &str) -> String {
    git_graph_merge_source_label(summary)
        .or_else(|| {
            git_graph_change_request_label(summary).map(|label| format!("merged via {label}"))
        })
        .unwrap_or_else(|| "merged side branch".to_string())
}

fn git_graph_merge_source(summary: &str) -> Option<String> {
    let source = if let Some((_, source)) = summary.rsplit_once(" from ") {
        source
    } else if let Some(rest) = summary.strip_prefix("Merge branch '") {
        rest.split_once('\'')?.0
    } else if let Some(rest) = summary.strip_prefix("Merge remote-tracking branch '") {
        rest.split_once('\'')?.0
    } else if let Some(rest) = summary.strip_prefix("Merged in ") {
        rest.split_once(" (pull request")
            .map(|(source, _)| source)
            .unwrap_or(rest)
    } else if let Some(rest) = summary.strip_prefix("Merge ") {
        rest.split_once(" into ")?.0
    } else {
        return None;
    };
    let source = source
        .trim()
        .trim_matches(|ch: char| ch == '\'' || ch == '"' || ch == '.' || ch == ':' || ch == ',');
    (!source.is_empty()).then(|| source.to_string())
}

fn git_graph_change_request_label(message: &str) -> Option<String> {
    let lower = message.to_ascii_lowercase();
    if let Some(idx) = lower.find("pull request #")
        && let Some(id) = git_graph_digits_after(&message[idx + "pull request ".len()..], '#')
    {
        return Some(format!("PR #{id}"));
    }
    if let Some(idx) = lower.find("(#")
        && let Some(id) = git_graph_digits_after(&message[idx + 1..], '#')
    {
        return Some(format!("PR #{id}"));
    }
    if let Some(idx) = lower.find("merge request !")
        && let Some(id) = git_graph_digits_after(&message[idx + "merge request ".len()..], '!')
    {
        return Some(format!("MR !{id}"));
    }
    if let Some(idx) = lower.find("see merge request")
        && let Some(id) = git_graph_digits_after(&message[idx..], '!')
    {
        return Some(format!("MR !{id}"));
    }
    if let Some(idx) = lower.find("mr !")
        && let Some(id) = git_graph_digits_after(&message[idx + "mr ".len()..], '!')
    {
        return Some(format!("MR !{id}"));
    }
    None
}

fn git_graph_digits_after(text: &str, marker: char) -> Option<&str> {
    let start = text.find(marker)? + marker.len_utf8();
    let digits_len = text[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .map(char::len_utf8)
        .sum::<usize>();
    (digits_len > 0).then_some(&text[start..start + digits_len])
}

fn git_graph_branch_label_propagates(label: &str) -> bool {
    label.starts_with("merged ")
}

fn collect_git_graph_trace_labels(repo: &git2::Repository) -> FxHashMap<String, String> {
    let mut out: FxHashMap<String, String> = FxHashMap::default();
    collect_git_graph_tag_labels(repo, &mut out);
    collect_git_graph_note_labels(repo, &mut out);
    collect_git_graph_reflog_labels(repo, &mut out);
    out
}

fn collect_git_graph_tag_labels(repo: &git2::Repository, out: &mut FxHashMap<String, String>) {
    let Ok(refs) = repo.references() else {
        return;
    };
    for reference_result in refs {
        let Ok(reference) = reference_result else {
            continue;
        };
        let Some(name) = reference.name() else {
            continue;
        };
        let Some(tag_name) = name.strip_prefix("refs/tags/") else {
            continue;
        };
        if tag_name.is_empty() {
            continue;
        }
        let Some(target) = reference
            .peel_to_commit()
            .ok()
            .map(|commit| commit.id())
            .or_else(|| reference.target())
        else {
            continue;
        };
        out.entry(target.to_string())
            .or_insert_with(|| format!("tag {tag_name}"));
    }
}

fn collect_git_graph_note_labels(repo: &git2::Repository, out: &mut FxHashMap<String, String>) {
    let Ok(notes) = repo.notes(None) else {
        return;
    };
    for note_result in notes.take(256) {
        let Ok((_, annotated_id)) = note_result else {
            continue;
        };
        let Ok(note) = repo.find_note(None, annotated_id) else {
            continue;
        };
        let Some(message) = note.message() else {
            continue;
        };
        if let Some(label) = git_graph_note_label(message) {
            out.entry(annotated_id.to_string()).or_insert(label);
        }
    }
}

fn collect_git_graph_reflog_labels(repo: &git2::Repository, out: &mut FxHashMap<String, String>) {
    collect_git_graph_reflog(repo, "HEAD", out);
    let Ok(refs) = repo.references() else {
        return;
    };
    for reference_result in refs.take(64) {
        let Ok(reference) = reference_result else {
            continue;
        };
        let Some(name) = reference.name() else {
            continue;
        };
        if name.starts_with("refs/heads/") {
            collect_git_graph_reflog(repo, name, out);
        }
    }
}

fn collect_git_graph_reflog(
    repo: &git2::Repository,
    name: &str,
    out: &mut FxHashMap<String, String>,
) {
    let Ok(reflog) = repo.reflog(name) else {
        return;
    };
    for entry in reflog.iter().take(128) {
        let Some(message) = entry.message() else {
            continue;
        };
        if let Some(label) = git_graph_reflog_label(message) {
            out.entry(entry.id_new().to_string()).or_insert(label);
        }
    }
}

fn git_graph_note_label(message: &str) -> Option<String> {
    git_graph_merge_source_label(message)
        .or_else(|| git_graph_change_request_label(message))
        .or_else(|| git_graph_first_line_label(message, "note"))
}

fn git_graph_reflog_label(message: &str) -> Option<String> {
    git_graph_merge_source_label(message)
        .or_else(|| git_graph_change_request_label(message))
        .or_else(|| {
            message
                .strip_prefix("merge ")
                .and_then(|rest| rest.split_once(':').map(|(source, _)| source.trim()))
                .filter(|source| !source.is_empty())
                .map(|source| format!("reflog merge {source}"))
        })
        .or_else(|| {
            message
                .strip_prefix("pull ")
                .and_then(|rest| rest.split_once(':').map(|(source, _)| source.trim()))
                .filter(|source| !source.is_empty())
                .map(|source| format!("reflog pull {source}"))
        })
}

fn git_graph_first_line_label(message: &str, prefix: &str) -> Option<String> {
    let line = message.lines().find(|line| !line.trim().is_empty())?.trim();
    if line.is_empty() {
        return None;
    }
    let end = line
        .char_indices()
        .take_while(|(idx, _)| *idx <= 48)
        .map(|(idx, ch)| idx + ch.len_utf8())
        .last()
        .unwrap_or(0)
        .min(line.len());
    Some(format!("{prefix}: {}", &line[..end]))
}

fn collect_git_graph_refs(repo: &git2::Repository) -> FxHashMap<String, Vec<GitGraphRef>> {
    let mut out: FxHashMap<String, Vec<GitGraphRef>> = FxHashMap::default();
    let Ok(refs) = repo.references() else {
        return out;
    };
    for reference_result in refs {
        let Ok(reference) = reference_result else {
            continue;
        };
        let Some(name) = reference.name() else {
            continue;
        };
        let Some(git_ref) = normalize_git_ref_name(name) else {
            continue;
        };
        let Some(target) = reference
            .target()
            .or_else(|| reference.peel_to_commit().ok().map(|commit| commit.id()))
        else {
            continue;
        };
        out.entry(target.to_string()).or_default().push(git_ref);
    }
    out
}

pub(crate) fn normalize_git_ref_name(name: &str) -> Option<GitGraphRef> {
    if let Some(short) = name.strip_prefix("refs/heads/") {
        if short.is_empty() {
            return None;
        }
        return Some(GitGraphRef {
            name: short.to_string(),
            is_remote: false,
        });
    }
    if let Some(short) = name.strip_prefix("refs/remotes/") {
        if short.is_empty() || short.ends_with("/HEAD") {
            return None;
        }
        return Some(GitGraphRef {
            name: short.to_string(),
            is_remote: true,
        });
    }
    None
}

pub(crate) fn github_base_url_from_remote_url(url: &str) -> Option<String> {
    let trimmed = url.trim().trim_end_matches('/');
    let path = trimmed
        .strip_prefix("https://github.com/")
        .or_else(|| trimmed.strip_prefix("http://github.com/"))
        .or_else(|| trimmed.strip_prefix("git@github.com:"))
        .or_else(|| trimmed.strip_prefix("ssh://git@github.com/"))?;
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut parts = path.split('/');
    let owner = parts.next()?.trim();
    let repo = parts.next()?.trim();
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some(format!("https://github.com/{owner}/{repo}"))
}

fn git_commit_stats(repo: &git2::Repository, commit: &git2::Commit<'_>) -> (usize, usize, usize) {
    let Ok(tree) = commit.tree() else {
        return (0, 0, 0);
    };
    let parent_tree = commit.parent(0).ok().and_then(|parent| parent.tree().ok());
    let Ok(diff) = repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), None) else {
        return (0, 0, 0);
    };
    let Ok(stats) = diff.stats() else {
        return (0, 0, 0);
    };
    (stats.files_changed(), stats.insertions(), stats.deletions())
}

#[derive(Clone, Debug)]
struct ActiveGraphLane {
    oid: String,
    column: usize,
    color_idx: usize,
    branch_name: Option<String>,
}

fn first_free_graph_column(active: &[ActiveGraphLane]) -> usize {
    let mut column = 0usize;
    while active.iter().any(|lane| lane.column == column) {
        column += 1;
    }
    column
}

fn push_unique_graph_lane(lanes: &mut Vec<GitGraphLane>, lane: GitGraphLane) {
    if !lanes
        .iter()
        .any(|existing| existing.column == lane.column && existing.kind == lane.kind)
    {
        lanes.push(lane);
    }
}

fn apply_git_graph_lanes(commits: &mut [GitGraphCommit]) -> usize {
    let mut branch_by_oid: FxHashMap<String, String> = FxHashMap::default();
    let mut merge_source_by_oid: FxHashMap<String, String> = FxHashMap::default();
    for commit in commits.iter() {
        if let Some(branch_name) = &commit.branch_name {
            branch_by_oid.insert(commit.oid.clone(), branch_name.clone());
        }
        if commit.parent_oids.len() > 1 {
            let source_label = git_graph_merge_side_parent_label(&commit.summary);
            for parent in commit.parent_oids.iter().skip(1) {
                merge_source_by_oid.insert(parent.clone(), source_label.clone());
            }
        }
    }

    let mut active: Vec<ActiveGraphLane> = Vec::new();
    let mut next_color = 0usize;
    let mut max_column = 0usize;

    for commit in commits {
        let lane_idx = if let Some(idx) = active.iter().position(|lane| lane.oid == commit.oid) {
            idx
        } else {
            let column = first_free_graph_column(&active);
            let color_idx = next_color;
            next_color = next_color.saturating_add(1);
            active.push(ActiveGraphLane {
                oid: commit.oid.clone(),
                column,
                color_idx,
                branch_name: merge_source_by_oid
                    .get(&commit.oid)
                    .cloned()
                    .or_else(|| commit.branch_name.clone()),
            });
            active.len() - 1
        };

        let commit_column = active[lane_idx].column;
        let commit_color = active[lane_idx].color_idx;
        if commit.branch_name.is_none() {
            commit.branch_name = merge_source_by_oid
                .get(&commit.oid)
                .cloned()
                .or_else(|| active[lane_idx].branch_name.clone());
        }
        let commit_branch_name = commit.branch_name.clone();
        let propagating_branch_name = commit_branch_name
            .as_ref()
            .filter(|label| git_graph_branch_label_propagates(label))
            .cloned();
        max_column = max_column.max(commit_column);
        let mut lanes = Vec::with_capacity(active.len() + commit.parent_oids.len() + 1);
        for lane in &active {
            max_column = max_column.max(lane.column);
            push_unique_graph_lane(
                &mut lanes,
                GitGraphLane {
                    column: lane.column,
                    color_idx: lane.color_idx,
                    kind: GitGraphLaneKind::Vertical,
                },
            );
        }

        let parents = commit.parent_oids.clone();
        if parents.is_empty() {
            active.remove(lane_idx);
        } else {
            let first_parent = &parents[0];
            if let Some(existing_idx) = active
                .iter()
                .position(|lane| lane.oid == *first_parent && lane.column != commit_column)
            {
                let parent_lane = active[existing_idx].clone();
                push_unique_graph_lane(
                    &mut lanes,
                    GitGraphLane {
                        column: parent_lane.column,
                        color_idx: commit_color,
                        kind: GitGraphLaneKind::Parent,
                    },
                );
                active.remove(lane_idx);
            } else if let Some(lane) = active.get_mut(lane_idx) {
                lane.oid.clone_from(first_parent);
                lane.branch_name = propagating_branch_name.clone();
            }

            for parent in parents.iter().skip(1) {
                let merge_parent_label = merge_source_by_oid.get(parent).cloned();
                let parent_branch_name = if merge_parent_label
                    .as_deref()
                    .is_some_and(|label| label != "merged side branch")
                {
                    merge_parent_label
                } else {
                    branch_by_oid
                        .get(parent)
                        .cloned()
                        .or(merge_parent_label)
                        .or_else(|| propagating_branch_name.clone())
                };
                let parent_lane = if let Some(existing_idx) =
                    active.iter().position(|lane| lane.oid == *parent)
                {
                    if active[existing_idx].branch_name.is_none() {
                        active[existing_idx].branch_name = parent_branch_name.clone();
                    }
                    active[existing_idx].clone()
                } else {
                    let column = first_free_graph_column(&active);
                    let color_idx = next_color;
                    next_color = next_color.saturating_add(1);
                    let lane = ActiveGraphLane {
                        oid: parent.clone(),
                        column,
                        color_idx,
                        branch_name: parent_branch_name,
                    };
                    active.push(lane.clone());
                    lane
                };
                max_column = max_column.max(parent_lane.column);
                push_unique_graph_lane(
                    &mut lanes,
                    GitGraphLane {
                        column: parent_lane.column,
                        color_idx: parent_lane.color_idx,
                        kind: GitGraphLaneKind::Vertical,
                    },
                );
                push_unique_graph_lane(
                    &mut lanes,
                    GitGraphLane {
                        column: parent_lane.column,
                        color_idx: parent_lane.color_idx,
                        kind: GitGraphLaneKind::Parent,
                    },
                );
            }
        }

        push_unique_graph_lane(
            &mut lanes,
            GitGraphLane {
                column: commit_column,
                color_idx: commit_color,
                kind: GitGraphLaneKind::Vertical,
            },
        );
        lanes.sort_by_key(|lane| {
            (
                lane.column,
                match lane.kind {
                    GitGraphLaneKind::Vertical => 0u8,
                    GitGraphLaneKind::Parent => 1u8,
                },
            )
        });
        commit.column = commit_column;
        commit.color_idx = commit_color;
        commit.lanes = lanes;
        active.sort_by_key(|lane| lane.column);
    }

    max_column.saturating_add(1).max(1)
}

pub(crate) fn format_git_relative_time(time_secs: i64, now_secs: i64) -> String {
    let delta = now_secs.saturating_sub(time_secs).max(0);
    if delta < 60 {
        return "только что".to_string();
    }
    let minutes = delta / 60;
    if minutes < 60 {
        return format!(
            "{minutes} {} назад",
            plural_ru(minutes, "минута", "минуты", "минут")
        );
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours} {} назад", plural_ru(hours, "час", "часа", "часов"));
    }
    let days = hours / 24;
    if days < 30 {
        return format!("{days} {} назад", plural_ru(days, "день", "дня", "дней"));
    }
    let months = days / 30;
    if months < 12 {
        return format!(
            "{months} {} назад",
            plural_ru(months, "месяц", "месяца", "месяцев")
        );
    }
    let years = days / 365;
    format!("{years} {} назад", plural_ru(years, "год", "года", "лет"))
}

fn plural_ru(value: i64, one: &'static str, few: &'static str, many: &'static str) -> &'static str {
    let mod100 = value % 100;
    if (11..=14).contains(&mod100) {
        return many;
    }
    match value % 10 {
        1 => one,
        2..=4 => few,
        _ => many,
    }
}

pub(crate) fn format_git_absolute_time(time_secs: i64, offset_minutes: i32) -> String {
    let shifted = time_secs.saturating_add(offset_minutes as i64 * 60);
    let days = div_floor(shifted, 86_400);
    let seconds_of_day = shifted - days * 86_400;
    let hour = seconds_of_day / 3600;
    let minute = (seconds_of_day % 3600) / 60;
    let (year, month, day) = unix_days_to_ymd(days);
    let month_name = match month {
        1 => "января",
        2 => "февраля",
        3 => "марта",
        4 => "апреля",
        5 => "мая",
        6 => "июня",
        7 => "июля",
        8 => "августа",
        9 => "сентября",
        10 => "октября",
        11 => "ноября",
        _ => "декабря",
    };
    format!("{day} {month_name} {year} г. в {hour:02}:{minute:02}")
}

fn div_floor(value: i64, divisor: i64) -> i64 {
    let quotient = value / divisor;
    let remainder = value % divisor;
    if remainder != 0 && ((remainder > 0) != (divisor > 0)) {
        quotient - 1
    } else {
        quotient
    }
}

fn unix_days_to_ymd(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + i64::from(month <= 2);
    (year, month, day)
}

fn collect_git_status(workspaces: &[PathBuf]) -> GitStatusSnapshot {
    let mut out = GitStatusSnapshot {
        workspaces: Vec::with_capacity(workspaces.len()),
    };
    for (workspace_idx, root) in workspaces.iter().enumerate() {
        out.workspaces
            .push(collect_workspace_status(workspace_idx, root));
    }
    out
}

fn collect_workspace_status(workspace_idx: usize, root: &Path) -> GitWorkspaceStatus {
    let repo = match git2::Repository::discover(root) {
        Ok(repo) => repo,
        Err(err) => {
            return GitWorkspaceStatus {
                workspace_idx,
                root: root.to_path_buf(),
                repo_root: None,
                branch_name: None,
                files: Vec::new(),
                tree: Vec::new(),
                ahead: 0,
                error: Some(short_git_error(err)),
            };
        }
    };

    let repo_root = repo
        .workdir()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.to_path_buf());

    let mut status_opts = git2::StatusOptions::new();
    status_opts
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true);

    let statuses = match repo.statuses(Some(&mut status_opts)) {
        Ok(statuses) => statuses,
        Err(err) => {
            return GitWorkspaceStatus {
                workspace_idx,
                root: root.to_path_buf(),
                repo_root: Some(repo_root),
                branch_name: None,
                files: Vec::new(),
                tree: Vec::new(),
                ahead: 0,
                error: Some(short_git_error(err)),
            };
        }
    };

    let mut files = Vec::new();
    let mut file_by_display_path: FxHashMap<String, usize> = FxHashMap::default();
    for entry in statuses.iter() {
        let status = entry.status();
        if status.is_ignored() || status.is_empty() {
            continue;
        }
        let Some((rel_path, old_rel_path)) = status_entry_paths(&entry) else {
            continue;
        };
        let abs_path = repo_root.join(&rel_path);
        if !abs_path.starts_with(root) {
            continue;
        }
        let display_path = abs_path
            .strip_prefix(root)
            .ok()
            .and_then(|path| path.to_str())
            .map(|path| path.trim_start_matches('/').to_string())
            .filter(|path| !path.is_empty())
            .or_else(|| rel_path.to_str().map(str::to_string))
            .unwrap_or_else(|| "?".to_string());
        let depth = display_path
            .split('/')
            .filter(|part| !part.is_empty())
            .count()
            .saturating_sub(1);
        let staged = status_intersects_index(status);
        let file_status = git_file_status(status, staged);
        if let Some(existing_idx) = file_by_display_path.get(display_path.as_str()).copied() {
            let existing: &mut GitFileEntry = &mut files[existing_idx];
            existing.staged |= staged;
            if staged {
                existing.status = file_status;
            }
            if existing.old_rel_path.is_none() {
                existing.old_rel_path = old_rel_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned());
            }
            continue;
        }
        file_by_display_path.insert(display_path.clone(), files.len());
        files.push(GitFileEntry {
            workspace_idx,
            repo_root: repo_root.clone(),
            rel_path: rel_path.to_string_lossy().into_owned(),
            old_rel_path: old_rel_path.map(|path| path.to_string_lossy().into_owned()),
            display_path,
            depth,
            staged,
            status: file_status,
        });
    }

    files.sort_by(|a, b| a.display_path.cmp(&b.display_path));
    let tree = build_git_tree(&files);
    let branch_name = current_branch_name(&repo);
    let ahead = branch_ahead(&repo).unwrap_or(0);

    GitWorkspaceStatus {
        workspace_idx,
        root: root.to_path_buf(),
        repo_root: Some(repo_root),
        branch_name,
        files,
        tree,
        ahead,
        error: None,
    }
}

#[derive(Default)]
struct GitTreeBuildNode {
    file_idx: Option<usize>,
    children: BTreeMap<String, GitTreeBuildNode>,
}

fn build_git_tree(files: &[GitFileEntry]) -> Vec<GitTreeRow> {
    let mut root = GitTreeBuildNode::default();
    for (file_idx, file) in files.iter().enumerate() {
        let mut node = &mut root;
        let mut parts = file
            .display_path
            .split('/')
            .filter(|part| !part.is_empty())
            .peekable();
        while let Some(part) = parts.next() {
            node = node.children.entry(part.to_string()).or_default();
            if parts.peek().is_none() {
                node.file_idx = Some(file_idx);
            }
        }
    }

    let mut rows = Vec::new();
    flatten_git_tree(&root, "", 0, &mut rows);
    rows
}

fn flatten_git_tree(
    node: &GitTreeBuildNode,
    parent_path: &str,
    depth: usize,
    rows: &mut Vec<GitTreeRow>,
) {
    for files_first in [false, true] {
        for (name, child) in &node.children {
            if child.file_idx.is_some() != files_first {
                continue;
            }
            let path = if parent_path.is_empty() {
                name.clone()
            } else {
                let mut path = String::with_capacity(parent_path.len() + 1 + name.len());
                path.push_str(parent_path);
                path.push('/');
                path.push_str(name);
                path
            };
            let icon_key = if child.file_idx.is_some() {
                crate::app::file_icons::file_icon_key(&name.to_ascii_lowercase())
            } else {
                crate::app::file_icons::folder_icon_key(&name.to_ascii_lowercase())
            };
            rows.push(GitTreeRow {
                name: name.clone(),
                path: path.clone(),
                depth,
                file_idx: child.file_idx,
                icon_key,
            });
            if !child.children.is_empty() {
                flatten_git_tree(child, &path, depth + 1, rows);
            }
        }
    }
}

pub(crate) fn git_visible_tree_row_count(
    workspace_idx: usize,
    rows: &[GitTreeRow],
    collapsed_dirs: &FxHashMap<usize, FxHashSet<String>>,
) -> usize {
    let mut count = 0usize;
    let mut collapsed_depth = None;
    let workspace_collapsed = collapsed_dirs.get(&workspace_idx);
    for row in rows {
        if let Some(depth) = collapsed_depth {
            if row.depth > depth {
                continue;
            }
            collapsed_depth = None;
        }
        count += 1;
        if row.file_idx.is_none()
            && workspace_collapsed.is_some_and(|dirs| dirs.contains(row.path.as_str()))
        {
            collapsed_depth = Some(row.depth);
        }
    }
    count
}

fn git_path_is_descendant(path: &str, folder: &str) -> bool {
    path.len() > folder.len()
        && path.starts_with(folder)
        && path
            .as_bytes()
            .get(folder.len())
            .is_some_and(|byte| *byte == b'/')
}

pub(crate) fn git_folder_file_indices(
    workspace: &GitWorkspaceStatus,
    row_idx: usize,
) -> Vec<usize> {
    let Some(row) = workspace.tree.get(row_idx) else {
        return Vec::new();
    };
    if row.file_idx.is_some() {
        return Vec::new();
    }
    let folder = row.path.as_str();
    workspace
        .files
        .iter()
        .enumerate()
        .filter_map(|(file_idx, file)| {
            git_path_is_descendant(file.display_path.as_str(), folder).then_some(file_idx)
        })
        .collect()
}

pub(crate) fn git_folder_stage_state(
    workspace: &GitWorkspaceStatus,
    row_idx: usize,
) -> Option<GitFolderStageState> {
    let Some(row) = workspace.tree.get(row_idx) else {
        return None;
    };
    if row.file_idx.is_some() {
        return None;
    }

    let folder = row.path.as_str();
    let mut total = 0usize;
    let mut staged = 0usize;
    for file in &workspace.files {
        if git_path_is_descendant(file.display_path.as_str(), folder) {
            total += 1;
            if file.staged {
                staged += 1;
            }
        }
    }
    match (total, staged) {
        (0, _) => None,
        (_, 0) => Some(GitFolderStageState::Empty),
        (total, staged) if total == staged => Some(GitFolderStageState::All),
        _ => Some(GitFolderStageState::Partial),
    }
}

fn status_entry_paths(entry: &git2::StatusEntry<'_>) -> Option<(PathBuf, Option<PathBuf>)> {
    let delta = entry.index_to_workdir().or_else(|| entry.head_to_index())?;
    let new_path = delta.new_file().path()?.to_path_buf();
    let old_path = delta
        .old_file()
        .path()
        .filter(|path| *path != new_path.as_path())
        .map(Path::to_path_buf);
    Some((new_path, old_path))
}

fn status_intersects_index(status: git2::Status) -> bool {
    status.intersects(
        git2::Status::INDEX_NEW
            | git2::Status::INDEX_MODIFIED
            | git2::Status::INDEX_DELETED
            | git2::Status::INDEX_RENAMED
            | git2::Status::INDEX_TYPECHANGE,
    )
}

fn git_file_status(status: git2::Status, staged: bool) -> GitFileStatus {
    let mask = if staged {
        git2::Status::INDEX_NEW
            | git2::Status::INDEX_MODIFIED
            | git2::Status::INDEX_DELETED
            | git2::Status::INDEX_RENAMED
            | git2::Status::INDEX_TYPECHANGE
    } else {
        git2::Status::WT_NEW
            | git2::Status::WT_MODIFIED
            | git2::Status::WT_DELETED
            | git2::Status::WT_RENAMED
            | git2::Status::WT_TYPECHANGE
    };
    let s = status & mask;
    if !staged && status.is_wt_new() {
        GitFileStatus::Untracked
    } else if s.intersects(git2::Status::INDEX_NEW | git2::Status::WT_NEW) {
        GitFileStatus::Added
    } else if s.intersects(git2::Status::INDEX_DELETED | git2::Status::WT_DELETED) {
        GitFileStatus::Deleted
    } else if s.intersects(git2::Status::INDEX_RENAMED | git2::Status::WT_RENAMED) {
        GitFileStatus::Renamed
    } else if s.intersects(git2::Status::INDEX_TYPECHANGE | git2::Status::WT_TYPECHANGE) {
        GitFileStatus::TypeChange
    } else {
        GitFileStatus::Modified
    }
}

fn current_branch_name(repo: &git2::Repository) -> Option<String> {
    repo.head()
        .ok()
        .and_then(|head| {
            head.shorthand()
                .map(str::to_string)
                .or_else(|| head.target().map(|oid| oid.to_string()))
        })
        .map(|name| name.chars().take(12).collect())
}

fn branch_ahead(repo: &git2::Repository) -> Result<usize, git2::Error> {
    let head = repo.head()?;
    let local_oid = head
        .target()
        .ok_or_else(|| git2::Error::from_str("No HEAD"))?;
    let name = head
        .shorthand()
        .ok_or_else(|| git2::Error::from_str("No branch"))?;
    let branch = repo.find_branch(name, git2::BranchType::Local)?;
    let upstream = branch.upstream()?;
    let upstream_oid = upstream
        .get()
        .target()
        .ok_or_else(|| git2::Error::from_str("No upstream target"))?;
    let (ahead, _) = repo.graph_ahead_behind(local_oid, upstream_oid)?;
    Ok(ahead)
}

fn toggle_stage(
    repo_root: &Path,
    rel_path: &str,
    old_rel_path: Option<&str>,
    staged: bool,
) -> Result<(), String> {
    let repo = git2::Repository::open(repo_root).map_err(short_git_error)?;
    let path = Path::new(rel_path);
    if staged {
        unstage_path(&repo, path, old_rel_path.map(Path::new)).map_err(short_git_error)
    } else {
        let mut index = repo.index().map_err(short_git_error)?;
        if let Some(old_path) = old_rel_path.map(Path::new)
            && old_path != path
        {
            index.remove_path(old_path).map_err(short_git_error)?;
        }
        if repo_root.join(path).exists() {
            index.add_path(path).map_err(short_git_error)?;
        } else {
            index.remove_path(path).map_err(short_git_error)?;
        }
        index.write().map_err(short_git_error)
    }
}

fn unstage_path(
    repo: &git2::Repository,
    path: &Path,
    old_path: Option<&Path>,
) -> Result<(), git2::Error> {
    let target = repo
        .head()
        .ok()
        .and_then(|head| head.peel(git2::ObjectType::Commit).ok());
    if let Some(target) = target.as_ref() {
        repo.reset_default(Some(target), [path])?;
        if let Some(old_path) = old_path
            && old_path != path
        {
            repo.reset_default(Some(target), [old_path])?;
        }
        Ok(())
    } else {
        let mut index = repo.index()?;
        if let Some(old_path) = old_path
            && old_path != path
        {
            index.remove_path(old_path)?;
        }
        index.remove_path(path)?;
        index.write()
    }
}

fn rollback_staged_file(
    repo_root: &Path,
    rel_path: &str,
    old_rel_path: Option<&str>,
) -> Result<(), String> {
    let repo = git2::Repository::open(repo_root).map_err(short_git_error)?;
    let path = Path::new(rel_path);
    let old_path = old_rel_path.map(Path::new);
    unstage_path(&repo, path, old_path).map_err(short_git_error)?;

    let _head = repo
        .head()
        .and_then(|head| head.peel(git2::ObjectType::Commit))
        .map_err(short_git_error)?;
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout
        .force()
        .remove_untracked(true)
        .recreate_missing(true);
    checkout.path(path);
    if let Some(old_path) = old_path
        && old_path != path
    {
        checkout.path(old_path);
    }
    repo.checkout_head(Some(&mut checkout))
        .map_err(short_git_error)
}

fn commit_repo(repo_root: &Path, message: &str, amend: bool) -> Result<(), String> {
    let repo = git2::Repository::open(repo_root).map_err(short_git_error)?;
    let sig = signature(&repo).map_err(short_git_error)?;
    let mut index = repo.index().map_err(short_git_error)?;
    let tree_id = index.write_tree().map_err(short_git_error)?;
    index.write().map_err(short_git_error)?;
    let tree = repo.find_tree(tree_id).map_err(short_git_error)?;

    if amend {
        let head_commit = repo
            .head()
            .and_then(|head| head.peel_to_commit())
            .map_err(short_git_error)?;
        head_commit
            .amend(
                Some("HEAD"),
                Some(&sig),
                Some(&sig),
                None,
                Some(message),
                Some(&tree),
            )
            .map_err(short_git_error)?;
    } else {
        let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
        let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .map_err(short_git_error)?;
    }
    Ok(())
}

fn signature(repo: &git2::Repository) -> Result<git2::Signature<'_>, git2::Error> {
    match repo.signature() {
        Ok(sig) => Ok(sig),
        Err(_) => git2::Signature::now("RRiter", "rriter@example.invalid"),
    }
}

fn push_repo(repo_root: &Path) -> Result<(), String> {
    let repo = git2::Repository::open(repo_root).map_err(short_git_error)?;
    let head = repo.head().map_err(short_git_error)?;
    let head_name = head
        .name()
        .ok_or_else(|| "No branch ref".to_string())?
        .to_string();
    let branch = head_name
        .strip_prefix("refs/heads/")
        .ok_or_else(|| "Detached HEAD cannot push".to_string())?;
    let local_branch = repo
        .find_branch(branch, git2::BranchType::Local)
        .map_err(short_git_error)?;
    let (remote_name, remote_ref) = local_branch
        .upstream()
        .ok()
        .and_then(|upstream| {
            upstream
                .get()
                .name()
                .and_then(|name| name.strip_prefix("refs/remotes/"))
                .and_then(|name| name.split_once('/'))
                .map(|(remote, remote_branch)| {
                    (remote.to_string(), format!("refs/heads/{remote_branch}"))
                })
        })
        .unwrap_or_else(|| ("origin".to_string(), format!("refs/heads/{branch}")));
    let remote = repo.find_remote(&remote_name).map_err(|err| {
        format!(
            "Push remote `{}` not found: {}",
            remote_name,
            short_git_error(err)
        )
    })?;
    let remote_url = remote.url().unwrap_or("<no-url>");
    let refspec = format!("refs/heads/{branch}:{remote_ref}");
    println!(
        "[GIT PUSH] repo={} remote={} url={} refspec={} backend=git",
        repo_root.display(),
        remote_name,
        remote_url,
        refspec
    );
    push_repo_with_git_cli(repo_root, &remote_name, branch, &remote_ref)
}

fn push_repo_with_git_cli(
    repo_root: &Path,
    remote_name: &str,
    branch: &str,
    remote_ref: &str,
) -> Result<(), String> {
    let refspec = format!("refs/heads/{branch}:{remote_ref}");
    println!(
        "[GIT PUSH] git -C {} push {} {}",
        repo_root.display(),
        remote_name,
        refspec
    );
    let mut command = std::process::Command::new("git");
    command
        .arg("-C")
        .arg(repo_root)
        .arg("push")
        .arg(remote_name)
        .arg(refspec)
        .env("GIT_TERMINAL_PROMPT", "0");
    if std::env::var_os("GIT_SSH_COMMAND").is_none() {
        command.env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes");
    }
    let output = command.output().map_err(|err| err.to_string())?;
    if output.status.success() {
        println!("[GIT PUSH] ok");
        Ok(())
    } else {
        let stderr = short_command_output(&output.stderr);
        let stdout = short_command_output(&output.stdout);
        println!(
            "[GIT PUSH] failed status={:?} stderr={} stdout={}",
            output.status.code(),
            stderr,
            stdout
        );
        if stderr.is_empty() {
            Err(stdout)
        } else {
            Err(stderr)
        }
    }
}

fn short_command_output(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes).trim().to_string();
    if text.len() > 180 {
        let end = text
            .char_indices()
            .take_while(|(idx, _)| *idx <= 180)
            .map(|(idx, ch)| idx + ch.len_utf8())
            .last()
            .unwrap_or(0)
            .min(text.len());
        format!("{}...", &text[..end])
    } else {
        text
    }
}

fn short_git_error(err: git2::Error) -> String {
    let msg = err.message();
    if msg.len() > 140 {
        let end = msg
            .char_indices()
            .take_while(|(idx, _)| *idx <= 140)
            .map(|(idx, ch)| idx + ch.len_utf8())
            .last()
            .unwrap_or(0)
            .min(msg.len());
        format!("{}...", &msg[..end])
    } else {
        msg.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_git_root(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("rriter_git_panel_{name}_{stamp}"))
    }

    fn git_file(display_path: &str, staged: bool, status: GitFileStatus) -> GitFileEntry {
        GitFileEntry {
            workspace_idx: 0,
            repo_root: PathBuf::from("/repo"),
            rel_path: display_path.to_string(),
            old_rel_path: None,
            display_path: display_path.to_string(),
            depth: display_path.matches('/').count(),
            staged,
            status,
        }
    }

    fn git_workspace(files: Vec<GitFileEntry>, error: Option<String>) -> GitWorkspaceStatus {
        GitWorkspaceStatus {
            workspace_idx: 0,
            root: PathBuf::from("/workspace"),
            repo_root: Some(PathBuf::from("/workspace")),
            branch_name: None,
            tree: build_git_tree(&files),
            files,
            ahead: 0,
            error,
        }
    }

    fn graph_commit(oid: &str, parents: &[&str]) -> GitGraphCommit {
        GitGraphCommit {
            oid: oid.to_string(),
            short_oid: oid.chars().take(7).collect(),
            summary: oid.to_string(),
            branch_name: None,
            author_name: "A".to_string(),
            author_email: "a@example.invalid".to_string(),
            time_secs: 0,
            time_offset: 0,
            relative_time: String::new(),
            absolute_time: String::new(),
            files_changed: 0,
            insertions: 0,
            deletions: 0,
            local_refs: Vec::new(),
            remote_refs: Vec::new(),
            lanes: Vec::new(),
            column: 0,
            color_idx: 0,
            is_head: false,
            github_url: None,
            parent_oids: parents.iter().map(|parent| (*parent).to_string()).collect(),
        }
    }

    #[test]
    fn git_graph_remote_url_parse_and_ref_normalize() {
        assert_eq!(
            github_base_url_from_remote_url("https://github.com/org/repo.git"),
            Some("https://github.com/org/repo".to_string())
        );
        assert_eq!(
            github_base_url_from_remote_url("git@github.com:org/repo.git"),
            Some("https://github.com/org/repo".to_string())
        );
        assert_eq!(
            github_base_url_from_remote_url("ssh://git@github.com/org/repo"),
            Some("https://github.com/org/repo".to_string())
        );
        assert_eq!(
            github_base_url_from_remote_url("https://example.com/x/y"),
            None
        );

        assert_eq!(
            normalize_git_ref_name("refs/heads/master"),
            Some(GitGraphRef {
                name: "master".to_string(),
                is_remote: false,
            })
        );
        assert_eq!(
            normalize_git_ref_name("refs/remotes/origin/master"),
            Some(GitGraphRef {
                name: "origin/master".to_string(),
                is_remote: true,
            })
        );
        assert_eq!(normalize_git_ref_name("refs/remotes/origin/HEAD"), None);
        assert_eq!(normalize_git_ref_name("refs/tags/v1"), None);
        assert_eq!(
            git_graph_merge_source_label("Merge pull request #2 from stormasm/update_api"),
            Some("merged from stormasm/update_api".to_string())
        );
        assert_eq!(
            git_graph_merge_source_label("Merge branch 'feature/ui'"),
            Some("merged from feature/ui".to_string())
        );
        assert_eq!(
            git_graph_merge_source_label("Merge branch 'feature/ui' into 'main'"),
            Some("merged from feature/ui".to_string())
        );
        assert_eq!(
            git_graph_merge_source_label("Merge remote-tracking branch 'origin/feature/ui'"),
            Some("merged from origin/feature/ui".to_string())
        );
        assert_eq!(
            git_graph_merge_source_label("Merged in feature/ui (pull request #7)"),
            Some("merged from feature/ui".to_string())
        );
        assert_eq!(git_graph_merge_source_label("feat: normal commit"), None);
        assert_eq!(
            git_graph_change_request_label("fix calculator (#12)"),
            Some("PR #12".to_string())
        );
        assert_eq!(
            git_graph_change_request_label("See merge request group/repo!34"),
            Some("MR !34".to_string())
        );
        assert_eq!(
            git_graph_merge_side_parent_label("Merge something custom"),
            "merged side branch"
        );
        assert_eq!(
            git_graph_note_label("See merge request group/repo!34"),
            Some("MR !34".to_string())
        );
        assert_eq!(
            git_graph_note_label("reviewed by ops"),
            Some("note: reviewed by ops".to_string())
        );
        assert_eq!(
            git_graph_reflog_label("merge feature/api: Merge made by the 'ort' strategy."),
            Some("reflog merge feature/api".to_string())
        );
        assert_eq!(
            git_graph_reflog_label("pull origin main: Fast-forward"),
            Some("reflog pull origin main".to_string())
        );
    }

    #[test]
    fn git_graph_summary_trims_hidden_prefixes() {
        assert_eq!(
            clean_git_summary("\u{feff}\u{200b}  fix check_item in audits;"),
            "fix check_item in audits;"
        );
        assert_eq!(clean_git_summary("\u{200b}"), "(no message)");
    }

    #[test]
    fn git_graph_time_format_is_cached_friendly() {
        assert_eq!(format_git_relative_time(100, 130), "только что");
        assert_eq!(format_git_relative_time(0, 60), "1 минута назад");
        assert_eq!(format_git_relative_time(0, 120), "2 минуты назад");
        assert_eq!(format_git_relative_time(0, 300), "5 минут назад");
        assert_eq!(format_git_relative_time(0, 3 * 3600), "3 часа назад");
        assert_eq!(format_git_absolute_time(0, 0), "1 января 1970 г. в 00:00");
        assert_eq!(format_git_absolute_time(0, 180), "1 января 1970 г. в 03:00");
    }

    #[test]
    fn git_graph_lane_layout_handles_branch_and_merge() {
        let mut commits = vec![
            graph_commit("merge", &["main", "branch"]),
            graph_commit("main", &["root"]),
            graph_commit("branch", &["root"]),
            graph_commit("root", &[]),
        ];

        let lane_count = apply_git_graph_lanes(&mut commits);

        assert_eq!(lane_count, 2);
        assert_eq!(commits[0].column, 0);
        assert_eq!(commits[2].column, 1);
        assert!(commits[0].lanes.iter().any(|lane| {
            lane.kind == GitGraphLaneKind::Parent && lane.column == commits[2].column
        }));
        assert!(commits[2].lanes.iter().any(|lane| {
            lane.kind == GitGraphLaneKind::Parent && lane.column == commits[1].column
        }));
        assert!(commits[2].lanes.iter().any(|lane| {
            lane.kind == GitGraphLaneKind::Parent
                && lane.column == commits[1].column
                && lane.color_idx == commits[2].color_idx
        }));
    }

    #[test]
    fn git_file_status_maps_index_and_worktree_flags() {
        assert_eq!(
            git_file_status(git2::Status::WT_NEW, false),
            GitFileStatus::Untracked
        );
        assert_eq!(
            git_file_status(git2::Status::INDEX_NEW, true),
            GitFileStatus::Added
        );
        assert_eq!(
            git_file_status(git2::Status::WT_DELETED, false),
            GitFileStatus::Deleted
        );
        assert_eq!(
            git_file_status(git2::Status::INDEX_RENAMED, true),
            GitFileStatus::Renamed
        );
        assert_eq!(
            git_file_status(git2::Status::INDEX_TYPECHANGE, true),
            GitFileStatus::TypeChange
        );
        assert_eq!(
            git_file_status(git2::Status::WT_MODIFIED, false),
            GitFileStatus::Modified
        );
    }

    #[test]
    fn git_file_status_labels_match_editor_badges() {
        assert_eq!(GitFileStatus::Added.label(), "A");
        assert_eq!(GitFileStatus::Modified.label(), "M");
        assert_eq!(GitFileStatus::Deleted.label(), "D");
        assert_eq!(GitFileStatus::Renamed.label(), "R");
        assert_eq!(GitFileStatus::TypeChange.label(), "T");
        assert_eq!(GitFileStatus::Untracked.label(), "U");
    }

    #[test]
    fn git_workspace_collapse_button_only_when_rows_exist() {
        assert!(!git_workspace(Vec::new(), None).has_collapsible_rows());
        assert!(
            git_workspace(Vec::new(), Some("git status failed".to_string()))
                .has_collapsible_rows()
        );
        assert!(git_workspace(
            vec![git_file("src/main.rs", false, GitFileStatus::Modified)],
            None
        )
        .has_collapsible_rows());
    }

    #[test]
    fn staged_repo_roots_use_active_workspace_and_dedupe_roots() {
        let repo_a = PathBuf::from("/repo/a");
        let repo_b = PathBuf::from("/repo/b");
        let snapshot = GitStatusSnapshot {
            workspaces: vec![
                GitWorkspaceStatus {
                    workspace_idx: 0,
                    root: PathBuf::from("/ws/a"),
                    repo_root: Some(repo_a.clone()),
                    branch_name: None,
                    files: vec![
                        GitFileEntry {
                            repo_root: repo_a.clone(),
                            staged: true,
                            ..git_file("src/main.rs", true, GitFileStatus::Added)
                        },
                        GitFileEntry {
                            repo_root: repo_a.clone(),
                            staged: true,
                            ..git_file("src/lib.rs", true, GitFileStatus::Modified)
                        },
                    ],
                    tree: Vec::new(),
                    ahead: 0,
                    error: None,
                },
                GitWorkspaceStatus {
                    workspace_idx: 1,
                    root: PathBuf::from("/ws/b"),
                    repo_root: Some(repo_b.clone()),
                    branch_name: None,
                    files: vec![GitFileEntry {
                        workspace_idx: 1,
                        repo_root: repo_b,
                        rel_path: "other.rs".to_string(),
                        old_rel_path: None,
                        display_path: "other.rs".to_string(),
                        depth: 0,
                        staged: true,
                        status: GitFileStatus::Added,
                    }],
                    tree: Vec::new(),
                    ahead: 0,
                    error: None,
                },
            ],
        };

        assert_eq!(snapshot.active_staged_workspace_idx(), Some(0));
        assert_eq!(snapshot.staged_repo_roots(), vec![repo_a]);
    }

    #[test]
    fn staged_workspace_lock_keeps_pending_workspace_stable() {
        let mut state = GitPanelState::default();
        state.stage_pending_workspace_idx = Some(0);
        state.snapshot = GitStatusSnapshot {
            workspaces: vec![GitWorkspaceStatus {
                workspace_idx: 1,
                root: PathBuf::from("/ws/b"),
                repo_root: Some(PathBuf::from("/repo/b")),
                branch_name: None,
                files: vec![GitFileEntry {
                    workspace_idx: 1,
                    repo_root: PathBuf::from("/repo/b"),
                    rel_path: "other.rs".to_string(),
                    old_rel_path: None,
                    display_path: "other.rs".to_string(),
                    depth: 0,
                    staged: true,
                    status: GitFileStatus::Added,
                }],
                tree: Vec::new(),
                ahead: 0,
                error: None,
            }],
        };

        assert_eq!(state.staged_workspace_lock(), Some(0));

        state.snapshot.workspaces[0].files[0].staged = false;
        assert_eq!(state.staged_workspace_lock(), Some(0));
    }

    #[test]
    fn commit_event_clears_message_editor() {
        let mut state = GitPanelState::default();
        let _ = state.message_editor.insert_str("ready");
        state.message_focused = true;

        state.apply_event(GitPanelEvent {
            request_id: 3,
            snapshot: GitStatusSnapshot::default(),
            notice: Some("Committed 1 repo(s)".to_string()),
            preserve_snapshot_on_empty: false,
            clear_message: true,
        });

        assert_eq!(state.message_editor.get_full_text(), "");
        assert!(!state.message_focused);
    }

    #[test]
    fn git_stage_click_locked_blocks_pending_and_other_workspace() {
        let mut state = GitPanelState::default();
        state.pending = true;
        state.stage_pending_workspace_idx = Some(0);

        assert!(git_stage_click_locked(&state, 0));
        assert!(git_stage_click_locked(&state, 1));

        state.pending = false;
        assert!(!git_stage_click_locked(&state, 0));
        assert!(git_stage_click_locked(&state, 1));
    }

    #[test]
    fn stage_event_preserves_visible_topology_and_merges_existing_files() {
        let mut state = GitPanelState::default();
        state.snapshot = GitStatusSnapshot {
            workspaces: vec![GitWorkspaceStatus {
                workspace_idx: 0,
                root: PathBuf::from("/workspace"),
                repo_root: Some(PathBuf::from("/workspace")),
                branch_name: None,
                files: vec![git_file("tests/test_api.py", true, GitFileStatus::Modified)],
                tree: Vec::new(),
                ahead: 0,
                error: None,
            }],
        };

        state.apply_event(GitPanelEvent {
            request_id: 7,
            snapshot: GitStatusSnapshot {
                workspaces: vec![GitWorkspaceStatus {
                    workspace_idx: 0,
                    root: PathBuf::from("/workspace"),
                    repo_root: Some(PathBuf::from("/workspace")),
                    branch_name: None,
                    files: vec![
                        git_file("tests/test_api.py", false, GitFileStatus::Modified),
                        git_file(".dockerignore", true, GitFileStatus::Renamed),
                    ],
                    tree: Vec::new(),
                    ahead: 0,
                    error: None,
                }],
            },
            notice: None,
            preserve_snapshot_on_empty: true,
            clear_message: false,
        });

        assert_eq!(state.latest_request_id, 7);
        assert_eq!(state.snapshot.workspaces[0].files.len(), 1);
        assert!(!state.snapshot.workspaces[0].files[0].staged);
        assert_eq!(
            state.snapshot.workspaces[0].files[0].display_path,
            "tests/test_api.py"
        );

        state.apply_event(GitPanelEvent {
            request_id: 8,
            snapshot: GitStatusSnapshot {
                workspaces: vec![GitWorkspaceStatus {
                    workspace_idx: 0,
                    root: PathBuf::from("/workspace"),
                    repo_root: Some(PathBuf::from("/workspace")),
                    branch_name: None,
                    files: Vec::new(),
                    tree: Vec::new(),
                    ahead: 0,
                    error: None,
                }],
            },
            notice: None,
            preserve_snapshot_on_empty: false,
            clear_message: false,
        });

        assert!(state.snapshot.workspaces[0].files.is_empty());
    }

    #[test]
    fn stage_event_removes_clean_files_and_clears_pending_workspace() {
        let mut state = GitPanelState::default();
        state.stage_pending_workspace_idx = Some(0);
        state.snapshot = GitStatusSnapshot {
            workspaces: vec![GitWorkspaceStatus {
                workspace_idx: 0,
                root: PathBuf::from("/workspace"),
                repo_root: Some(PathBuf::from("/workspace")),
                branch_name: None,
                files: vec![git_file("tests/test_api.py", true, GitFileStatus::Modified)],
                tree: build_git_tree(&[git_file(
                    "tests/test_api.py",
                    true,
                    GitFileStatus::Modified,
                )]),
                ahead: 0,
                error: None,
            }],
        };

        state.apply_event(GitPanelEvent {
            request_id: 10,
            snapshot: GitStatusSnapshot {
                workspaces: vec![GitWorkspaceStatus {
                    workspace_idx: 0,
                    root: PathBuf::from("/workspace"),
                    repo_root: Some(PathBuf::from("/workspace")),
                    branch_name: Some("main".to_string()),
                    files: Vec::new(),
                    tree: Vec::new(),
                    ahead: 0,
                    error: None,
                }],
            },
            notice: None,
            preserve_snapshot_on_empty: true,
            clear_message: false,
        });

        assert!(state.snapshot.workspaces[0].files.is_empty());
        assert!(state.snapshot.workspaces[0].tree.is_empty());
        assert_eq!(
            state.snapshot.workspaces[0].branch_name.as_deref(),
            Some("main")
        );
        assert_eq!(state.stage_pending_workspace_idx, None);
    }

    #[test]
    fn stage_workspace_lock_preserves_topology_for_refresh_events_too() {
        let mut state = GitPanelState::default();
        state.stage_pending_workspace_idx = Some(0);
        state.snapshot = GitStatusSnapshot {
            workspaces: vec![GitWorkspaceStatus {
                workspace_idx: 0,
                root: PathBuf::from("/workspace"),
                repo_root: Some(PathBuf::from("/workspace")),
                branch_name: None,
                files: vec![git_file("tests/test_api.py", true, GitFileStatus::Modified)],
                tree: Vec::new(),
                ahead: 0,
                error: None,
            }],
        };

        state.apply_event(GitPanelEvent {
            request_id: 9,
            snapshot: GitStatusSnapshot {
                workspaces: vec![GitWorkspaceStatus {
                    workspace_idx: 0,
                    root: PathBuf::from("/workspace"),
                    repo_root: Some(PathBuf::from("/workspace")),
                    branch_name: None,
                    files: vec![
                        git_file("tests/test_api.py", false, GitFileStatus::Modified),
                        git_file(".dockerignore", true, GitFileStatus::Renamed),
                    ],
                    tree: Vec::new(),
                    ahead: 0,
                    error: None,
                }],
            },
            notice: None,
            preserve_snapshot_on_empty: false,
            clear_message: false,
        });

        assert_eq!(state.snapshot.workspaces[0].files.len(), 1);
        assert_eq!(
            state.snapshot.workspaces[0].files[0].display_path,
            "tests/test_api.py"
        );
        assert!(state.snapshot.workspaces[0].files[0].staged);
    }

    #[test]
    fn git_tree_builds_folder_rows_icons_and_collapse_counts() {
        let files = vec![
            git_file("README.md", false, GitFileStatus::Modified),
            git_file(".dockerignore", false, GitFileStatus::Modified),
            git_file("src/lib.rs", false, GitFileStatus::Modified),
            git_file("src/main.rs", true, GitFileStatus::Added),
        ];

        let rows = build_git_tree(&files);

        assert_eq!(
            rows.iter()
                .map(|row| (
                    row.name.as_str(),
                    row.path.as_str(),
                    row.depth,
                    row.file_idx
                ))
                .collect::<Vec<_>>(),
            vec![
                ("src", "src", 0, None),
                ("lib.rs", "src/lib.rs", 1, Some(2)),
                ("main.rs", "src/main.rs", 1, Some(3)),
                (".dockerignore", ".dockerignore", 0, Some(1)),
                ("README.md", "README.md", 0, Some(0)),
            ]
        );
        assert_ne!(rows[0].icon_key, "default");
        assert_ne!(rows[3].icon_key, "default_file");

        let mut collapsed = FxHashMap::default();
        assert_eq!(git_visible_tree_row_count(0, &rows, &collapsed), 5);
        collapsed.insert(0, FxHashSet::from_iter(["src".to_string()]));
        assert_eq!(git_visible_tree_row_count(0, &rows, &collapsed), 3);
        assert_eq!(git_visible_tree_row_count(1, &rows, &collapsed), 5);
    }

    #[test]
    fn git_folder_stage_state_uses_descendant_files_only() {
        let files = vec![
            git_file("src/lib.rs", false, GitFileStatus::Modified),
            git_file("src/main.rs", true, GitFileStatus::Added),
            git_file("src-extra/mod.rs", true, GitFileStatus::Added),
        ];
        let workspace = GitWorkspaceStatus {
            workspace_idx: 0,
            root: PathBuf::from("/workspace"),
            repo_root: Some(PathBuf::from("/workspace")),
            branch_name: None,
            tree: build_git_tree(&files),
            files,
            ahead: 0,
            error: None,
        };

        let src_idx = workspace
            .tree
            .iter()
            .position(|row| row.path == "src" && row.file_idx.is_none())
            .unwrap();
        let src_extra_idx = workspace
            .tree
            .iter()
            .position(|row| row.path == "src-extra" && row.file_idx.is_none())
            .unwrap();

        assert_eq!(
            git_folder_file_indices(&workspace, src_idx),
            vec![0usize, 1usize]
        );
        assert_eq!(
            git_folder_stage_state(&workspace, src_idx),
            Some(GitFolderStageState::Partial)
        );
        assert_eq!(
            git_folder_stage_state(&workspace, src_extra_idx),
            Some(GitFolderStageState::All)
        );
    }

    #[test]
    fn git_status_stage_and_commit_round_trip_uses_libgit2_only() {
        let root = temp_git_root("round_trip");
        std::fs::create_dir_all(root.join("src")).unwrap();
        let repo = git2::Repository::init(&root).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();

        let workspace = collect_workspace_status(7, &root);
        assert_eq!(workspace.workspace_idx, 7);
        assert_eq!(workspace.files.len(), 1);
        assert_eq!(workspace.files[0].display_path, "src/main.rs");
        assert!(!workspace.files[0].staged);
        assert_eq!(workspace.files[0].status, GitFileStatus::Untracked);
        assert_eq!(
            workspace
                .tree
                .iter()
                .map(|row| row.name.as_str())
                .collect::<Vec<_>>(),
            vec!["src", "main.rs"]
        );

        toggle_stage(&root, "src/main.rs", None, false).unwrap();
        let workspace = collect_workspace_status(7, &root);
        assert!(workspace.files[0].staged);
        assert_eq!(workspace.files[0].status, GitFileStatus::Added);

        commit_repo(&root, "initial commit", false).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.message().unwrap(), "initial commit");
        assert!(collect_workspace_status(7, &root).files.is_empty());

        std::fs::remove_dir_all(&root).unwrap();
    }
}
