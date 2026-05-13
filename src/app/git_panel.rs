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
            Self::Untracked => "?",
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

#[derive(Clone, Debug)]
pub struct GitWorkspaceStatus {
    pub workspace_idx: usize,
    pub root: PathBuf,
    pub repo_root: Option<PathBuf>,
    pub files: Vec<GitFileEntry>,
    pub tree: Vec<GitTreeRow>,
    pub ahead: usize,
    pub error: Option<String>,
}

impl GitWorkspaceStatus {
    pub fn staged_count(&self) -> usize {
        self.files.iter().filter(|file| file.staged).count()
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

pub struct GitPanelState {
    pub snapshot: GitStatusSnapshot,
    pub message_editor: Editor,
    pub message_focused: bool,
    pub amend: bool,
    pub commit_menu_open: bool,
    pub collapsed_dirs: FxHashMap<usize, FxHashSet<String>>,
    pub scroll: crate::scroll::ScrollState,
    pub pending: bool,
    pub next_request_id: u64,
    pub latest_request_id: u64,
    pub rx: Vec<mpsc::Receiver<GitPanelEvent>>,
    stage_tx: Option<mpsc::Sender<GitStageCommand>>,
    pub notice: Option<String>,
}

impl Default for GitPanelState {
    fn default() -> Self {
        Self {
            snapshot: GitStatusSnapshot::default(),
            message_editor: Editor::new(512),
            message_focused: false,
            amend: false,
            commit_menu_open: false,
            collapsed_dirs: FxHashMap::default(),
            scroll: crate::scroll::ScrollState::new(15.0),
            pending: false,
            next_request_id: 1,
            latest_request_id: 0,
            rx: Vec::new(),
            stage_tx: None,
            notice: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct GitPanelEvent {
    request_id: u64,
    snapshot: GitStatusSnapshot,
    notice: Option<String>,
}

struct GitStageCommand {
    request_id: u64,
    repo_root: PathBuf,
    rel_path: String,
    old_rel_path: Option<String>,
    staged: bool,
    workspaces: Vec<PathBuf>,
    tx: mpsc::Sender<GitPanelEvent>,
}

enum GitAction {
    Refresh,
    ToggleStage {
        repo_root: PathBuf,
        rel_path: String,
        old_rel_path: Option<String>,
        staged: bool,
    },
    Commit {
        repo_roots: Vec<PathBuf>,
        message: String,
        amend: bool,
        push_after: bool,
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
            return;
        }
        self.spawn_git_task(GitAction::Refresh);
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn poll_git_panel(&mut self) -> bool {
        let mut updated = false;
        let mut stale_seen = false;
        let mut next_rx = Vec::with_capacity(self.ide_panel.git.rx.len());
        let receivers = std::mem::take(&mut self.ide_panel.git.rx);
        for rx in receivers {
            let mut keep = true;
            loop {
                match rx.try_recv() {
                    Ok(event) => {
                        if event.request_id >= self.ide_panel.git.latest_request_id {
                            self.ide_panel.git.latest_request_id = event.request_id;
                            self.ide_panel.git.snapshot = event.snapshot;
                            self.ide_panel.git.notice = event.notice;
                            self.ide_panel.git.pending = false;
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
                next_rx.push(rx);
            }
        }
        self.ide_panel.git.rx = next_rx;
        self.ide_panel.git.pending = !self.ide_panel.git.rx.is_empty();
        if stale_seen && self.ide_panel.git.rx.is_empty() {
            self.spawn_git_task(GitAction::Refresh);
        }
        updated
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn toggle_git_file_stage(&mut self, workspace_idx: usize, file_idx: usize) {
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
        self.spawn_git_task(GitAction::ToggleStage {
            repo_root: file.repo_root,
            rel_path: file.rel_path,
            old_rel_path: file.old_rel_path,
            staged: file.staged,
        });
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
        let request_id = self.ide_panel.git.next_request_id;
        self.ide_panel.git.next_request_id = self.ide_panel.git.next_request_id.saturating_add(1);
        self.ide_panel.git.latest_request_id = self.ide_panel.git.latest_request_id.max(request_id);
        self.ide_panel.git.pending = true;
        self.ide_panel.git.notice = None;

        let workspaces = self.ide_workspaces.clone();
        let (tx, rx) = mpsc::channel();
        self.ide_panel.git.rx.push(rx);

        if let GitAction::ToggleStage {
            repo_root,
            rel_path,
            old_rel_path,
            staged,
        } = action
        {
            let mut command = Some(GitStageCommand {
                request_id,
                repo_root,
                rel_path,
                old_rel_path,
                staged,
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
                    let notice = match toggle_stage(
                        &command.repo_root,
                        &command.rel_path,
                        command.old_rel_path.as_deref(),
                        command.staged,
                    ) {
                        Ok(()) => None,
                        Err(err) => Some(err),
                    };
                    let snapshot = collect_git_status(&command.workspaces);
                    let _ = command.tx.send(GitPanelEvent {
                        request_id: command.request_id,
                        snapshot,
                        notice,
                    });
                }
            });
            if let Some(command) = command {
                let _ = stage_tx.send(command);
            }
            return;
        }

        std::thread::spawn(move || {
            let notice = run_git_action(action);
            let snapshot = collect_git_status(&workspaces);
            let _ = tx.send(GitPanelEvent {
                request_id,
                snapshot,
                notice,
            });
        });
    }
}

fn run_git_action(action: GitAction) -> Option<String> {
    match action {
        GitAction::Refresh => None,
        GitAction::ToggleStage {
            repo_root,
            rel_path,
            old_rel_path,
            staged,
        } => match toggle_stage(&repo_root, &rel_path, old_rel_path.as_deref(), staged) {
            Ok(()) => None,
            Err(err) => Some(err),
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
                Some(format!("Committed {ok} repo(s)"))
            } else {
                Some(errors.join(" | "))
            }
        }
        GitAction::Push { repo_root } => match push_repo(&repo_root) {
            Ok(()) => Some("Push done".to_string()),
            Err(err) => Some(err),
        },
    }
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
    let ahead = branch_ahead(&repo).unwrap_or(0);

    GitWorkspaceStatus {
        workspace_idx,
        root: root.to_path_buf(),
        repo_root: Some(repo_root),
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
    flatten_git_tree(&root, 0, &mut rows);
    rows
}

fn flatten_git_tree(node: &GitTreeBuildNode, depth: usize, rows: &mut Vec<GitTreeRow>) {
    for (name, child) in &node.children {
        let path = if let Some(parent) = rows
            .iter()
            .rev()
            .find(|row| row.depth + 1 == depth && row.file_idx.is_none())
        {
            let mut path = parent.path.clone();
            path.push('/');
            path.push_str(name);
            path
        } else {
            name.clone()
        };
        let icon_key = if child.file_idx.is_some() {
            crate::app::file_icons::file_icon_key(&name.to_ascii_lowercase())
        } else {
            crate::app::file_icons::folder_icon_key(&name.to_ascii_lowercase())
        };
        rows.push(GitTreeRow {
            name: name.clone(),
            path,
            depth,
            file_idx: child.file_idx,
            icon_key,
        });
        if !child.children.is_empty() {
            flatten_git_tree(child, depth + 1, rows);
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

fn status_entry_paths(entry: &git2::StatusEntry<'_>) -> Option<(PathBuf, Option<PathBuf>)> {
    let delta = entry
        .index_to_workdir()
        .or_else(|| entry.head_to_index())?;
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
    let mut remote = repo.find_remote("origin").map_err(short_git_error)?;

    let mut callbacks = git2::RemoteCallbacks::new();
    callbacks.credentials(|_, username, allowed| {
        if allowed.contains(git2::CredentialType::SSH_KEY) {
            if let Some(user) = username {
                return git2::Cred::ssh_key_from_agent(user);
            }
        }
        git2::Cred::default()
    });
    let mut options = git2::PushOptions::new();
    options.remote_callbacks(callbacks);

    let branch = head_name
        .strip_prefix("refs/heads/")
        .ok_or_else(|| "Detached HEAD cannot push".to_string())?;
    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
    remote
        .push(&[refspec.as_str()], Some(&mut options))
        .map_err(short_git_error)
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
    fn staged_repo_roots_use_active_workspace_and_dedupe_roots() {
        let repo_a = PathBuf::from("/repo/a");
        let repo_b = PathBuf::from("/repo/b");
        let snapshot = GitStatusSnapshot {
            workspaces: vec![
                GitWorkspaceStatus {
                    workspace_idx: 0,
                    root: PathBuf::from("/ws/a"),
                    repo_root: Some(repo_a.clone()),
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
    fn git_tree_builds_folder_rows_icons_and_collapse_counts() {
        let files = vec![
            git_file("README.md", false, GitFileStatus::Modified),
            git_file("src/lib.rs", false, GitFileStatus::Modified),
            git_file("src/main.rs", true, GitFileStatus::Added),
        ];

        let rows = build_git_tree(&files);

        assert_eq!(
            rows.iter()
                .map(|row| (row.name.as_str(), row.path.as_str(), row.depth, row.file_idx))
                .collect::<Vec<_>>(),
            vec![
                ("README.md", "README.md", 0, Some(0)),
                ("src", "src", 0, None),
                ("lib.rs", "src/lib.rs", 1, Some(1)),
                ("main.rs", "src/main.rs", 1, Some(2)),
            ]
        );
        assert_ne!(rows[0].icon_key, "default_file");
        assert_ne!(rows[1].icon_key, "default");

        let mut collapsed = FxHashMap::default();
        assert_eq!(git_visible_tree_row_count(0, &rows, &collapsed), 4);
        collapsed.insert(0, FxHashSet::from_iter(["src".to_string()]));
        assert_eq!(git_visible_tree_row_count(0, &rows, &collapsed), 2);
        assert_eq!(git_visible_tree_row_count(1, &rows, &collapsed), 4);
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

        toggle_stage(&root, "src/main.rs", false).unwrap();
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
