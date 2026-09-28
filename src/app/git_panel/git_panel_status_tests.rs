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

fn collect_git_status_with_cache(
    workspaces: &[PathBuf],
    branch_ahead_cache: &mut BranchAheadCache,
) -> GitStatusSnapshot {
    let mut out = GitStatusSnapshot {
        workspaces: Vec::with_capacity(workspaces.len()),
    };
    for (workspace_idx, root) in workspaces.iter().enumerate() {
        out.workspaces
            .push(collect_workspace_status_with_cache(
                workspace_idx,
                root,
                branch_ahead_cache,
            ));
    }
    out
}

fn collect_workspace_status(workspace_idx: usize, root: &Path) -> GitWorkspaceStatus {
    let mut branch_ahead_cache = BranchAheadCache::default();
    collect_workspace_status_with_cache(workspace_idx, root, &mut branch_ahead_cache)
}

fn collect_workspace_status_with_cache(
    workspace_idx: usize,
    root: &Path,
    branch_ahead_cache: &mut BranchAheadCache,
) -> GitWorkspaceStatus {
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

    let rel_root = crate::platform::relative_to(root, &repo_root)
        .filter(|rel_root| !rel_root.as_os_str().is_empty());
    let mut status_opts = git2::StatusOptions::new();
    status_opts
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true);
    if let Some(rel_root) = rel_root.as_deref() {
        status_opts.pathspec(rel_root);
    }

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
    for entry in statuses.iter() {
        let status = entry.status();
        if status.is_ignored() || status.is_empty() {
            continue;
        }
        let Some((rel_path, old_rel_path)) = status_entry_paths(&entry) else {
            continue;
        };
        let Some(display_path) =
            git_status_display_path(rel_path, rel_root.as_deref(), root, &repo_root)
        else {
            continue;
        };
        let depth = display_path
            .split('/')
            .filter(|part| !part.is_empty())
            .count()
            .saturating_sub(1);
        let staged = status_intersects_index(status);
        let file_status = git_file_status(status, staged);
        let old_rel_path = if file_status == GitFileStatus::Renamed {
            old_rel_path.map(|path| path.to_string_lossy().into_owned().into_boxed_str())
        } else {
            None
        };
        files.push(GitFileEntry {
            workspace_idx,
            rel_path: rel_path.to_string_lossy().into_owned().into_boxed_str(),
            old_rel_path,
            display_path: display_path.into_boxed_str(),
            depth: depth.min(u16::MAX as usize) as u16,
            staged,
            status: file_status,
        });
    }

    #[cfg(any(windows, target_os = "macos"))]
    append_case_only_renames(
        &repo,
        &repo_root,
        rel_root.as_deref(),
        workspace_idx,
        root,
        &mut files,
    );

    files.sort_by(|a, b| a.display_path.cmp(&b.display_path));
    merge_git_status_files(&mut files);
    let tree = build_git_tree(&files);
    let branch_name = current_branch_name(&repo);
    let ahead = branch_ahead_cached(&repo, &repo_root, branch_ahead_cache).unwrap_or(0);

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
    if !git_tree_files_are_sorted(files) {
        return build_git_tree_from_unsorted_files(files);
    }
    let mut rows = Vec::with_capacity(files.len());
    push_git_tree_rows(files, 0, files.len(), "", 0, &mut rows);
    rows
}

fn git_tree_files_are_sorted(files: &[GitFileEntry]) -> bool {
    files
        .windows(2)
        .all(|pair| pair[0].display_path <= pair[1].display_path)
}

fn build_git_tree_from_unsorted_files(files: &[GitFileEntry]) -> Vec<GitTreeRow> {
    let mut order = (0..files.len()).collect::<Vec<_>>();
    order.sort_by(|&left, &right| files[left].display_path.cmp(&files[right].display_path));
    let mut rows = Vec::with_capacity(files.len());
    push_git_tree_rows_ordered(files, &order, 0, order.len(), "", 0, &mut rows);
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
    start: usize,
    end: usize,
    parent_path: &str,
    depth: u16,
    rows: &mut Vec<GitTreeRow>,
) {
    let mut idx = start;
    while idx < end {
        let Some((name, true)) =
            git_tree_next_part(files[idx].display_path.as_ref(), parent_path)
        else {
            idx += 1;
            continue;
        };
        let folder_start = idx;
        idx += 1;
        while idx < end
            && git_tree_next_part(files[idx].display_path.as_ref(), parent_path)
                .is_some_and(|(next_name, has_child)| has_child && next_name == name)
        {
            idx += 1;
        }
        let path = git_tree_join_path(parent_path, name);
        rows.push(GitTreeRow {
            name: name.into(),
            path: path.clone().into_boxed_str(),
            depth,
            file_idx: None,
            icon_key: crate::app::file_icons::folder_icon_key_for_name(name),
        });
        push_git_tree_rows(
            files,
            folder_start,
            idx,
            &path,
            depth.saturating_add(1),
            rows,
        );
    }

    for file_idx in start..end {
        let file = &files[file_idx];
        if let Some((name, false)) = git_tree_next_part(file.display_path.as_ref(), parent_path) {
            rows.push(GitTreeRow {
                name: name.into(),
                path: file.display_path.clone(),
                depth,
                file_idx: Some(file_idx),
                icon_key: crate::app::file_icons::file_icon_key_for_name(name),
            });
        }
    }
}

fn push_git_tree_rows_ordered(
    files: &[GitFileEntry],
    order: &[usize],
    start: usize,
    end: usize,
    parent_path: &str,
    depth: u16,
    rows: &mut Vec<GitTreeRow>,
) {
    let mut idx = start;
    while idx < end {
        let Some((name, true)) =
            git_tree_next_part(files[order[idx]].display_path.as_ref(), parent_path)
        else {
            idx += 1;
            continue;
        };
        let folder_start = idx;
        idx += 1;
        while idx < end
            && git_tree_next_part(files[order[idx]].display_path.as_ref(), parent_path)
                .is_some_and(|(next_name, has_child)| has_child && next_name == name)
        {
            idx += 1;
        }
        let path = git_tree_join_path(parent_path, name);
        rows.push(GitTreeRow {
            name: name.into(),
            path: path.clone().into_boxed_str(),
            depth,
            file_idx: None,
            icon_key: crate::app::file_icons::folder_icon_key_for_name(name),
        });
        push_git_tree_rows_ordered(
            files,
            order,
            folder_start,
            idx,
            &path,
            depth.saturating_add(1),
            rows,
        );
    }

    for &file_idx in order.iter().take(end).skip(start) {
        let file = &files[file_idx];
        if let Some((name, false)) = git_tree_next_part(file.display_path.as_ref(), parent_path) {
            rows.push(GitTreeRow {
                name: name.into(),
                path: file.display_path.clone(),
                depth,
                file_idx: Some(file_idx),
                icon_key: crate::app::file_icons::file_icon_key_for_name(name),
            });
        }
    }
}

fn git_status_path_string(path: &Path) -> Option<String> {
    path.to_str()
        .map(|path| path.trim_start_matches('/').to_string())
        .filter(|path| !path.is_empty())
}

fn git_status_display_path(
    rel_path: &Path,
    rel_root: Option<&Path>,
    root: &Path,
    repo_root: &Path,
) -> Option<String> {
    if let Some(rel_root) = rel_root {
        let display_path = rel_path.strip_prefix(rel_root).ok()?;
        return git_status_path_string(display_path).or_else(|| git_status_path_string(rel_path));
    }
    if crate::platform::paths_equal(root, repo_root) {
        return git_status_path_string(rel_path);
    }

    let abs_path = repo_root.join(rel_path);
    if !crate::platform::path_is_within(&abs_path, root) {
        return None;
    }
    crate::platform::relative_to(&abs_path, root)
        .as_deref()
        .and_then(git_status_path_string)
        .or_else(|| git_status_path_string(rel_path))
}

#[cfg(any(windows, target_os = "macos"))]
fn append_case_only_renames(
    repo: &git2::Repository,
    repo_root: &Path,
    rel_root: Option<&Path>,
    workspace_idx: usize,
    workspace_root: &Path,
    files: &mut Vec<GitFileEntry>,
) {
    let Ok(index) = repo.index() else {
        return;
    };
    let mut directory_entries: FxHashMap<PathBuf, Vec<std::ffi::OsString>> =
        FxHashMap::default();

    for entry in index.iter() {
        let Ok(index_path) = std::str::from_utf8(&entry.path) else {
            continue;
        };
        let index_rel = PathBuf::from(index_path.replace('/', std::path::MAIN_SEPARATOR_STR));
        if let Some(rel_root) = rel_root
            && !crate::platform::path_is_within(&index_rel, rel_root)
        {
            continue;
        }
        let Some(actual_rel) = actual_case_relative_path(
            repo_root,
            &index_rel,
            &mut directory_entries,
        ) else {
            continue;
        };
        if actual_rel == index_rel || !git_paths_equal_ignoring_case(&actual_rel, &index_rel) {
            continue;
        }

        files.retain(|file| {
            let current = Path::new(file.rel_path.as_ref());
            let old = file.old_rel_path.as_deref().map(Path::new);
            !git_paths_equal_ignoring_case(current, &index_rel)
                && !git_paths_equal_ignoring_case(current, &actual_rel)
                && !old.is_some_and(|old| {
                    git_paths_equal_ignoring_case(old, &index_rel)
                        || git_paths_equal_ignoring_case(old, &actual_rel)
                })
        });

        let Some(display_path) = git_status_display_path(
            &actual_rel,
            rel_root,
            workspace_root,
            repo_root,
        ) else {
            continue;
        };
        let rel_path = actual_rel.to_string_lossy().replace('\\', "/");
        let old_rel_path = index_rel.to_string_lossy().replace('\\', "/");
        let depth = display_path
            .split('/')
            .filter(|part| !part.is_empty())
            .count()
            .saturating_sub(1);
        files.push(GitFileEntry {
            workspace_idx,
            rel_path: rel_path.into_boxed_str(),
            old_rel_path: Some(old_rel_path.into_boxed_str()),
            display_path: display_path.into_boxed_str(),
            depth: depth.min(u16::MAX as usize) as u16,
            staged: false,
            status: GitFileStatus::Renamed,
        });
    }
}

#[cfg(any(windows, target_os = "macos"))]
fn git_paths_equal_ignoring_case(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        return crate::platform::paths_equal(left, right);
    }
    #[cfg(target_os = "macos")]
    {
        left.to_string_lossy().to_lowercase() == right.to_string_lossy().to_lowercase()
    }
}

#[cfg(any(windows, target_os = "macos"))]
fn actual_case_relative_path(
    repo_root: &Path,
    relative: &Path,
    directory_entries: &mut FxHashMap<PathBuf, Vec<std::ffi::OsString>>,
) -> Option<PathBuf> {
    let mut directory = repo_root.to_path_buf();
    let mut actual = PathBuf::new();
    for component in relative.components() {
        let std::path::Component::Normal(expected) = component else {
            return None;
        };
        let entries = directory_entries.entry(directory.clone()).or_insert_with(|| {
            std::fs::read_dir(&directory)
                .ok()
                .into_iter()
                .flatten()
                .filter_map(|entry| entry.ok().map(|entry| entry.file_name()))
                .collect()
        });
        let name = entries
            .iter()
            .find(|name| name.as_os_str() == expected)
            .or_else(|| {
                entries.iter().find(|name| {
                    git_paths_equal_ignoring_case(Path::new(name), Path::new(expected))
                })
            })?
            .clone();
        actual.push(&name);
        directory.push(name);
    }
    Some(actual)
}

fn merge_git_status_files(files: &mut Vec<GitFileEntry>) {
    if files.len() < 2 {
        return;
    }
    let mut idx = 1usize;
    while idx < files.len() {
        if files[idx - 1].display_path != files[idx].display_path {
            idx += 1;
            continue;
        }
        let duplicate = files.remove(idx);
        let existing = &mut files[idx - 1];
        existing.staged |= duplicate.staged;
        if duplicate.staged {
            existing.status = duplicate.status;
        }
        if existing.old_rel_path.is_none() {
            existing.old_rel_path = duplicate.old_rel_path;
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
            && workspace_collapsed.is_some_and(|dirs| dirs.contains(row.path.as_ref()))
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
    let folder = row.path.as_ref();
    workspace
        .files
        .iter()
        .enumerate()
        .filter_map(|(file_idx, file)| {
            git_path_is_descendant(file.display_path.as_ref(), folder).then_some(file_idx)
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

    let folder = row.path.as_ref();
    let mut total = 0usize;
    let mut staged = 0usize;
    for file in &workspace.files {
        if git_path_is_descendant(file.display_path.as_ref(), folder) {
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

fn status_entry_paths<'a>(entry: &'a git2::StatusEntry<'_>) -> Option<(&'a Path, Option<&'a Path>)> {
    let delta = entry.index_to_workdir().or_else(|| entry.head_to_index())?;
    let new_path = delta.new_file().path()?;
    let old_path = delta
        .old_file()
        .path()
        .filter(|path| *path != new_path);
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

fn branch_ahead_key(
    repo: &git2::Repository,
    repo_root: &Path,
) -> Result<BranchAheadKey, git2::Error> {
    let head = repo.head()?;
    let head_oid = head
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
    Ok(BranchAheadKey {
        repo_root: crate::platform::PathKey::new(repo_root),
        head_oid,
        upstream_oid,
    })
}

fn branch_ahead_cached(
    repo: &git2::Repository,
    repo_root: &Path,
    cache: &mut BranchAheadCache,
) -> Result<usize, git2::Error> {
    let key = branch_ahead_key(repo, repo_root)?;
    if let Some(&ahead) = cache.get(&key) {
        return Ok(ahead);
    }
    let (ahead, _) = repo.graph_ahead_behind(key.head_oid, key.upstream_oid)?;
    cache.insert(key, ahead);
    Ok(ahead)
}

fn git_index_change_status(status: git2::Status) -> git2::Status {
    status
        & (git2::Status::INDEX_NEW
            | git2::Status::INDEX_MODIFIED
            | git2::Status::INDEX_DELETED
            | git2::Status::INDEX_RENAMED
            | git2::Status::INDEX_TYPECHANGE)
}

fn git_index_entry_identity(entry: git2::IndexEntry) -> GitIndexEntryIdentity {
    GitIndexEntryIdentity {
        id: entry.id,
        mode: entry.mode,
    }
}

fn toggle_stage_with_identity(
    repo_root: &Path,
    rel_path: &str,
    old_rel_path: Option<&str>,
    staged: bool,
) -> Result<Option<GitIndexEntryIdentity>, String> {
    let repo = git2::Repository::open(repo_root).map_err(short_git_error)?;
    let path = Path::new(rel_path);
    if staged {
        unstage_path(&repo, path, old_rel_path.map(Path::new)).map_err(short_git_error)?;
        return Ok(None);
    }

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
    index.write().map_err(short_git_error)?;

    let status = match repo.status_file(path) {
        Ok(status) => status,
        Err(err) if err.code() == git2::ErrorCode::NotFound => return Ok(None),
        Err(err) => return Err(short_git_error(err)),
    };
    if git_index_change_status(status) != git2::Status::INDEX_MODIFIED {
        return Ok(None);
    }
    let index = repo.index().map_err(short_git_error)?;
    Ok(index.get_path(path, 0).map(git_index_entry_identity))
}

#[cfg(test)]
fn toggle_stage(
    repo_root: &Path,
    rel_path: &str,
    old_rel_path: Option<&str>,
    staged: bool,
) -> Result<(), String> {
    toggle_stage_with_identity(repo_root, rel_path, old_rel_path, staged).map(|_| ())
}

fn reconcile_owned_staged_modified_if_worktree_matches_head(
    repo_root: &Path,
    rel_path: &str,
    owned_identity: GitIndexEntryIdentity,
) -> Result<bool, String> {
    let repo = git2::Repository::open(repo_root).map_err(short_git_error)?;
    let path = Path::new(rel_path);
    let status = match repo.status_file(path) {
        Ok(status) => status,
        Err(err) if err.code() == git2::ErrorCode::NotFound => return Ok(false),
        Err(err) => return Err(short_git_error(err)),
    };
    if git_index_change_status(status) != git2::Status::INDEX_MODIFIED {
        return Ok(false);
    }

    let current_identity = repo
        .index()
        .map_err(short_git_error)?
        .get_path(path, 0)
        .map(git_index_entry_identity);
    if current_identity != Some(owned_identity) {
        return Ok(false);
    }

    let head = match repo.head().and_then(|head| head.peel_to_commit()) {
        Ok(head) => head,
        Err(err)
            if matches!(
                err.code(),
                git2::ErrorCode::NotFound | git2::ErrorCode::UnbornBranch
            ) =>
        {
            return Ok(false);
        }
        Err(err) => return Err(short_git_error(err)),
    };
    let tree = head.tree().map_err(short_git_error)?;
    if tree.get_path(path).is_err() {
        return Ok(false);
    }
    let mut diff_options = git2::DiffOptions::new();
    diff_options
        .pathspec(rel_path)
        .disable_pathspec_match(true);
    let diff = repo
        .diff_tree_to_workdir(Some(&tree), Some(&mut diff_options))
        .map_err(short_git_error)?;
    if diff.deltas().next().is_some() {
        return Ok(false);
    }

    unstage_path(&repo, path, None).map_err(short_git_error)?;
    Ok(true)
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

fn git_push_target(repo_root: &Path) -> Result<(String, String, String), String> {
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
    let _remote = repo.find_remote(&remote_name).map_err(|err| {
        format!(
            "Push remote `{}` not found: {}",
            remote_name,
            short_git_error(err)
        )
    })?;
    Ok((remote_name, branch.to_string(), remote_ref))
}

fn push_repo(repo_root: &Path) -> Result<(), String> {
    let (remote_name, branch, remote_ref) = git_push_target(repo_root)?;
    println!(
        "[GIT PUSH] repo={} remote={} branch={} backend=git",
        repo_root.display(),
        remote_name,
        branch
    );
    push_repo_with_git_cli(repo_root, &remote_name, &branch, &remote_ref)
}

fn fetch_repo(repo_root: &Path) -> Result<(), String> {
    run_git_cli(repo_root, &["fetch"], "FETCH")
}

fn pull_repo(repo_root: &Path) -> Result<(), String> {
    run_git_cli(repo_root, &["pull"], "PULL")
}

fn run_git_cli(repo_root: &Path, args: &[&str], label: &str) -> Result<(), String> {
    run_git_checked(repo_root, args, label)
}

fn push_repo_with_git_cli(
    repo_root: &Path,
    remote_name: &str,
    branch: &str,
    remote_ref: &str,
) -> Result<(), String> {
    run_git_checked_owned(repo_root, git_push_args(remote_name, branch, remote_ref), "PUSH")
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
            rel_path: display_path.into(),
            old_rel_path: None,
            display_path: display_path.into(),
            depth: display_path.matches('/').count().min(u16::MAX as usize) as u16,
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

    fn stage_operation(repo_root: &Path, rel_path: &str, staged: bool) -> GitStageOperation {
        GitStageOperation::ToggleMany(vec![GitStageFileCommand {
            repo_root: repo_root.to_path_buf(),
            rel_path: rel_path.to_string(),
            old_rel_path: None,
            staged,
        }])
    }

    fn reconcile_operation(repo_root: &Path, rel_path: &str) -> GitStageOperation {
        GitStageOperation::ReconcileModified(GitReconcileFileCommand {
            repo_root: repo_root.to_path_buf(),
            rel_path: rel_path.to_string(),
        })
    }

    include!("git_panel_graph_tests.rs");
    include!("git_panel_stage_status_tests.rs");

}

#[test]
fn one_shot_git_receiver_delivers_success_before_normal_disconnect() {
    let (tx, rx) = mpsc::channel();
    tx.send(42usize).unwrap();
    drop(tx);

    assert!(matches!(
        poll_one_shot_receiver(&rx),
        OneShotReceiverPoll::Ready(42)
    ));
    assert!(matches!(
        poll_one_shot_receiver(&rx),
        OneShotReceiverPoll::Disconnected
    ));
}
