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
                existing.old_rel_path = (file_status == GitFileStatus::Renamed)
                    .then(|| old_rel_path.as_ref())
                    .flatten()
                    .map(|path| path.to_string_lossy().into_owned());
            }
            continue;
        }
        file_by_display_path.insert(display_path.clone(), files.len());
        let old_rel_path = if file_status == GitFileStatus::Renamed {
            old_rel_path.map(|path| path.to_string_lossy().into_owned())
        } else {
            None
        };
        files.push(GitFileEntry {
            workspace_idx,
            rel_path: rel_path.to_string_lossy().into_owned(),
            old_rel_path,
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

fn build_git_tree(files: &[GitFileEntry]) -> Vec<GitTreeRow> {
    let mut order = (0..files.len()).collect::<Vec<_>>();
    order.sort_by(|&left, &right| files[left].display_path.cmp(&files[right].display_path));
    let mut rows = Vec::with_capacity(files.len());
    push_git_tree_rows(files, &order, 0, order.len(), "", 0, &mut rows);
    rows
}

fn git_tree_next_part<'a>(display_path: &'a str, parent_path: &str) -> Option<(&'a str, bool)> {
    let rest = if parent_path.is_empty() {
        display_path
    } else {
        display_path
            .strip_prefix(parent_path)?
            .strip_prefix('/')?
    };
    if rest.is_empty() {
        return None;
    }
    if let Some(slash_idx) = rest.find('/') {
        Some((&rest[..slash_idx], true))
    } else {
        Some((rest, false))
    }
}

fn git_tree_join_path(parent_path: &str, name: &str) -> String {
    if parent_path.is_empty() {
        name.to_string()
    } else {
        let mut path = String::with_capacity(parent_path.len() + 1 + name.len());
        path.push_str(parent_path);
        path.push('/');
        path.push_str(name);
        path
    }
}

fn push_git_tree_rows(
    files: &[GitFileEntry],
    order: &[usize],
    start: usize,
    end: usize,
    parent_path: &str,
    depth: usize,
    rows: &mut Vec<GitTreeRow>,
) {
    let mut idx = start;
    while idx < end {
        let Some((name, true)) =
            git_tree_next_part(&files[order[idx]].display_path, parent_path)
        else {
            idx += 1;
            continue;
        };
        let folder_start = idx;
        idx += 1;
        while idx < end
            && git_tree_next_part(&files[order[idx]].display_path, parent_path)
                .is_some_and(|(next_name, has_child)| has_child && next_name == name)
        {
            idx += 1;
        }
        let path = git_tree_join_path(parent_path, name);
        rows.push(GitTreeRow {
            name: name.to_string(),
            path: path.clone(),
            depth,
            file_idx: None,
            icon_key: crate::app::file_icons::folder_icon_key_for_name(name),
        });
        push_git_tree_rows(files, order, folder_start, idx, &path, depth + 1, rows);
    }

    for &file_idx in order.iter().take(end).skip(start) {
        let file = &files[file_idx];
        if let Some((name, false)) = git_tree_next_part(&file.display_path, parent_path) {
            rows.push(GitTreeRow {
                name: name.to_string(),
                path: file.display_path.clone(),
                depth,
                file_idx: Some(file_idx),
                icon_key: crate::app::file_icons::file_icon_key_for_name(name),
            });
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

    #[test]
    fn git_graph_prefetch_skips_loaded_active_and_cached_repos() {
        assert!(!git_graph_prefetch_needed(
            false,
            true,
            GIT_GRAPH_LIMIT_STEP,
            None,
            GIT_GRAPH_LIMIT_STEP
        ));
        assert!(!git_graph_prefetch_needed(
            false,
            false,
            0,
            Some(GIT_GRAPH_LIMIT_STEP),
            GIT_GRAPH_LIMIT_STEP
        ));
        assert!(git_graph_prefetch_needed(
            false,
            true,
            GIT_GRAPH_LIMIT_STEP - 1,
            None,
            GIT_GRAPH_LIMIT_STEP
        ));
        assert!(git_graph_prefetch_needed(
            true,
            true,
            GIT_GRAPH_LIMIT_STEP,
            Some(GIT_GRAPH_LIMIT_STEP),
            GIT_GRAPH_LIMIT_STEP
        ));
    }

    #[test]
    fn git_graph_scroll_thumb_shrinks_as_loaded_commits_grow() {
        let initial = git_graph_scroll_thumb_h(100, 500.0, 1.0);
        let loaded_more = git_graph_scroll_thumb_h(300, 500.0, 1.0);

        assert!(loaded_more < initial);
        assert!(loaded_more >= 10.0);
    }

    #[test]
    fn git_graph_scroll_thumb_handles_tiny_track() {
        let thumb = git_graph_scroll_thumb_h(100, 23.921906, 1.3333334);
        assert!(thumb > 0.0);
        assert!(thumb <= 23.921906);
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
            local_refs: Vec::new(),
            remote_refs: Vec::new(),
            lanes: Vec::new(),
            column: 0,
            color_idx: 0,
            branch_total_count: None,
            is_head: false,
            github_url: None,
            stats: None,
            parent_oids: parents.iter().map(|parent| (*parent).to_string()).collect(),
        }
    }

    fn has_graph_lane(commit: &GitGraphCommit, kind: GitGraphLaneKind, column: usize) -> bool {
        commit.lanes.iter().any(|lane| {
            lane.kind == kind
                && match kind {
                    GitGraphLaneKind::Parent
                    | GitGraphLaneKind::Shift
                    | GitGraphLaneKind::ShiftToCommit => usize::from(lane.target_column) == column,
                    _ => usize::from(lane.column) == column,
                }
        })
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
        assert_eq!(
            git_graph_merge_source_label(
                "Merge branch from ac6feca32d5424753f2664167f6b07f89e70cf11 to master"
            ),
            None
        );
        assert_eq!(
            git_graph_merge_source_label("Merge branch from feature/ui to master"),
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
            lane.kind == GitGraphLaneKind::Parent
                && usize::from(lane.target_column) == commits[2].column
        }));
        assert!(has_graph_lane(
            &commits[2],
            GitGraphLaneKind::VerticalTop,
            commits[2].column
        ));
        assert!(has_graph_lane(
            &commits[2],
            GitGraphLaneKind::VerticalBottom,
            commits[2].column
        ));
        assert!(has_graph_lane(
            &commits[3],
            GitGraphLaneKind::ShiftToCommit,
            0
        ));
    }

    #[test]
    fn git_graph_lane_layout_collapses_side_branch_into_commit_without_shift_tail() {
        let mut commits = vec![
            graph_commit("merge", &["main", "side"]),
            graph_commit("main", &["base"]),
            graph_commit("side", &["base"]),
            graph_commit("base", &[]),
        ];

        apply_git_graph_lanes(&mut commits);

        let side_column = commits[2].column;
        let base_column = commits[3].column;
        assert_eq!(side_column, 1);
        assert_eq!(base_column, 0);
        assert!(commits[3].lanes.iter().any(|lane| {
            lane.kind == GitGraphLaneKind::ShiftToCommit
                && usize::from(lane.column) == side_column
                && usize::from(lane.target_column) == base_column
        }));
        assert!(!commits[3].lanes.iter().any(|lane| {
            lane.kind == GitGraphLaneKind::Shift
                && usize::from(lane.column) == side_column
                && usize::from(lane.target_column) == base_column
        }));
        assert!(!has_graph_lane(
            &commits[3],
            GitGraphLaneKind::VerticalBottom,
            side_column
        ));
    }

    #[test]
    fn git_graph_lane_layout_keeps_long_side_chains_connected() {
        let mut commits = vec![
            graph_commit("merge", &["main", "branch1", "branch2", "branch3"]),
            graph_commit("main", &["root"]),
            graph_commit("branch1", &["branch1_mid"]),
            graph_commit("branch2", &["branch2_mid"]),
            graph_commit("branch3", &["branch3_mid"]),
            graph_commit("branch1_mid", &["root"]),
            graph_commit("branch2_mid", &["root"]),
            graph_commit("branch3_mid", &["root"]),
            graph_commit("root", &[]),
        ];

        let lane_count = apply_git_graph_lanes(&mut commits);

        assert_eq!(lane_count, 4);
        assert_eq!(commits[0].column, 0);
        assert_eq!(commits[1].column, 0);
        assert_eq!(commits[2].column, 1);
        assert_eq!(commits[3].column, 2);
        assert_eq!(commits[4].column, 3);
        assert_eq!(commits[5].column, 1);
        assert_eq!(commits[6].column, 2);
        assert_eq!(commits[7].column, 3);
        assert_eq!(commits[8].column, 0);

        assert!(!has_graph_lane(
            &commits[0],
            GitGraphLaneKind::VerticalTop,
            commits[0].column
        ));
        for column in 1..=3 {
            assert!(has_graph_lane(
                &commits[0],
                GitGraphLaneKind::Parent,
                column
            ));
        }

        for idx in 2..=4 {
            assert!(has_graph_lane(
                &commits[idx],
                GitGraphLaneKind::VerticalTop,
                commits[idx].column
            ));
            assert!(has_graph_lane(
                &commits[idx],
                GitGraphLaneKind::VerticalBottom,
                commits[idx].column
            ));
        }
        for idx in 5..=7 {
            assert!(has_graph_lane(
                &commits[idx],
                GitGraphLaneKind::VerticalTop,
                commits[idx].column
            ));
            assert!(has_graph_lane(
                &commits[idx],
                GitGraphLaneKind::VerticalBottom,
                commits[idx].column
            ));
        }
        assert!(has_graph_lane(
            &commits[8],
            GitGraphLaneKind::VerticalTop,
            commits[8].column
        ));
        for idx in 1..=3 {
            assert!(commits[8].lanes.iter().any(|lane| {
                lane.kind == GitGraphLaneKind::ShiftToCommit
                    && usize::from(lane.column) == idx
                    && usize::from(lane.target_column) == commits[8].column
            }));
        }
        assert!(!has_graph_lane(
            &commits[8],
            GitGraphLaneKind::VerticalBottom,
            commits[8].column
        ));
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
            git_workspace(Vec::new(), Some("git status failed".to_string())).has_collapsible_rows()
        );
        assert!(
            git_workspace(
                vec![git_file("src/main.rs", false, GitFileStatus::Modified)],
                None
            )
            .has_collapsible_rows()
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
                    branch_name: None,
                    files: vec![
                        GitFileEntry {
                            staged: true,
                            ..git_file("src/main.rs", true, GitFileStatus::Added)
                        },
                        GitFileEntry {
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
