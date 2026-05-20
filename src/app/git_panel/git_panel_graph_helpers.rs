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

fn git_graph_prefetch_needed(
    force_reload: bool,
    is_active_graph: bool,
    active_loaded_len: usize,
    cached_limit: Option<usize>,
    requested_limit: usize,
) -> bool {
    force_reload
        || !(is_active_graph && active_loaded_len >= requested_limit
            || cached_limit.is_some_and(|limit| limit >= requested_limit))
}

fn collect_git_graph(
    _workspace_idx: usize,
    repo_root: &Path,
    offset: usize,
    limit: usize,
) -> Result<(Vec<GitGraphCommit>, usize, bool), String> {
    if offset >= limit {
        return Ok((Vec::new(), 1, false));
    }
    let repo = git2::Repository::open(repo_root).map_err(short_git_error)?;
    let refs_by_oid = collect_git_graph_refs(&repo);
    let trace_labels_by_oid = collect_git_graph_trace_labels(&repo);
    let head_oid = repo
        .head()
        .ok()
        .and_then(|head| head.target())
        .map(|oid| oid.to_string());
    let github_base_url = repo
        .find_remote("origin")
        .ok()
        .and_then(|remote| remote.url().and_then(github_base_url_from_remote_url));
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);

    let page_len = limit.saturating_sub(offset).max(1);
    let mut commits = Vec::with_capacity(page_len.min(GIT_GRAPH_LIMIT_STEP));
    let total_commit_count = git_graph_total_commit_count(repo_root);
    let records = git_graph_log_records(repo_root, offset, page_len.saturating_add(1))?;
    let has_more = records.len() > page_len;
    for record in records.into_iter().take(page_len) {
        let mut local_refs = Vec::new();
        let mut remote_refs = Vec::new();
        if let Some(refs) = refs_by_oid.get(record.oid.as_str()) {
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

        let branch_name = git_graph_branch_label(&local_refs, &remote_refs)
            .or_else(|| git_graph_change_request_label(&record.summary))
            .or_else(|| trace_labels_by_oid.get(record.oid.as_str()).cloned());
        commits.push(GitGraphCommit {
            oid: record.oid.clone(),
            short_oid: record.oid.chars().take(7).collect(),
            summary: clean_git_summary(&record.summary),
            branch_name,
            author_name: if record.author_name.is_empty() {
                "Unknown".to_string()
            } else {
                record.author_name
            },
            author_email: record.author_email,
            time_secs: record.time_secs,
            time_offset: record.time_offset,
            relative_time: format_git_relative_time(record.time_secs, now_secs),
            absolute_time: format_git_absolute_time(record.time_secs, record.time_offset),
            local_refs,
            remote_refs,
            lanes: Vec::new(),
            column: 0,
            color_idx: 0,
            branch_total_count: total_commit_count,
            is_head: head_oid.as_deref() == Some(record.oid.as_str()),
            github_url: github_base_url
                .as_ref()
                .map(|base_url| format!("{base_url}/commit/{}", record.oid)),
            stats: Some(record.stats),
            parent_oids: record.parent_oids,
        });
    }

    let lane_count = if offset == 0 {
        apply_git_graph_lanes(&mut commits)
    } else {
        1
    };
    Ok((commits, lane_count, has_more))
}

fn git_graph_total_commit_count(repo_root: &Path) -> Option<usize> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("rev-list")
        .arg("--count")
        .arg("HEAD")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.trim().parse::<usize>().ok()
}

struct GitGraphLogRecord {
    oid: String,
    parent_oids: Vec<String>,
    author_name: String,
    author_email: String,
    time_secs: i64,
    time_offset: i32,
    summary: String,
    stats: GitGraphStats,
}

fn git_graph_log_records(
    repo_root: &Path,
    offset: usize,
    count: usize,
) -> Result<Vec<GitGraphLogRecord>, String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("log")
        .arg("--topo-order")
        .arg("--decorate=no")
        .arg("--numstat")
        .arg(format!("--skip={offset}"))
        .arg(format!("--max-count={count}"))
        .arg("--format=%x1e%H%x1f%P%x1f%an%x1f%ae%x1f%ct%x1f%ai%x1f%s")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|err| err.to_string())?;
    if !output.status.success() {
        let stderr = short_command_output(&output.stderr);
        let stdout = short_command_output(&output.stdout);
        return Err(if stderr.is_empty() { stdout } else { stderr });
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut records = Vec::new();
    for raw_record in text.split('\x1e') {
        let raw_record = raw_record.trim_matches('\n');
        if raw_record.is_empty() {
            continue;
        }
        let mut lines = raw_record.lines();
        let Some(header) = lines.next() else {
            continue;
        };
        let mut fields = header.split('\x1f');
        let Some(oid) = fields.next() else { continue };
        let Some(parents) = fields.next() else {
            continue;
        };
        let Some(author_name) = fields.next() else {
            continue;
        };
        let Some(author_email) = fields.next() else {
            continue;
        };
        let Some(time_secs) = fields.next().and_then(|value| value.parse::<i64>().ok()) else {
            continue;
        };
        let Some(author_date) = fields.next() else {
            continue;
        };
        let summary = fields.next().unwrap_or("(no message)");
        let stats = git_graph_numstat(lines);
        records.push(GitGraphLogRecord {
            oid: oid.to_string(),
            parent_oids: parents
                .split_whitespace()
                .map(str::to_string)
                .collect::<Vec<_>>(),
            author_name: author_name.to_string(),
            author_email: author_email.to_string(),
            time_secs,
            time_offset: git_log_time_offset_minutes(author_date),
            summary: summary.to_string(),
            stats,
        });
    }
    Ok(records)
}

fn git_graph_numstat<'a>(lines: impl Iterator<Item = &'a str>) -> GitGraphStats {
    let mut stats = GitGraphStats {
        files_changed: 0,
        insertions: 0,
        deletions: 0,
    };
    for line in lines {
        let mut parts = line.split('\t');
        let (Some(insertions), Some(deletions), Some(_path)) =
            (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        stats.files_changed = stats.files_changed.saturating_add(1);
        if insertions != "-" {
            stats.insertions = stats
                .insertions
                .saturating_add(insertions.parse::<usize>().unwrap_or(0));
        }
        if deletions != "-" {
            stats.deletions = stats
                .deletions
                .saturating_add(deletions.parse::<usize>().unwrap_or(0));
        }
    }
    stats
}

fn git_log_time_offset_minutes(author_date: &str) -> i32 {
    let Some(offset) = author_date.split_whitespace().last() else {
        return 0;
    };
    let bytes = offset.as_bytes();
    if bytes.len() != 5 || !matches!(bytes[0], b'+' | b'-') {
        return 0;
    }
    let Ok(hours) = offset[1..3].parse::<i32>() else {
        return 0;
    };
    let Ok(minutes) = offset[3..5].parse::<i32>() else {
        return 0;
    };
    let total = hours.saturating_mul(60).saturating_add(minutes);
    if bytes[0] == b'-' { -total } else { total }
}

pub(crate) fn run_git_graph_probe(repo_root: &Path, iterations: usize) -> Result<String, String> {
    let iterations = iterations.max(1);
    let mut out = String::new();
    let status = collect_workspace_status(0, repo_root);
    let graph_repo_root = status
        .repo_root
        .clone()
        .unwrap_or_else(|| repo_root.to_path_buf());
    let base_rss = current_rss_kb();
    out.push_str(&format!(
        "probe repo={} iterations={} files={} tree={} base_rss_kb={}\n",
        repo_root.display(),
        iterations,
        status.files.len(),
        status.tree.len(),
        base_rss.unwrap_or(0)
    ));
    let mut max_delta = 0usize;
    for iteration in 0..iterations {
        let (commits, lane_count, has_more) = collect_git_graph(0, &graph_repo_root, 0, 200)?;
        let rss = current_rss_kb();
        let delta = rss
            .zip(base_rss)
            .map(|(rss, base)| rss.saturating_sub(base))
            .unwrap_or(0);
        max_delta = max_delta.max(delta);
        out.push_str(&format!(
            "iter={} commits={} lanes={} has_more={} rss_kb={} graph_delta_kb={}\n",
            iteration + 1,
            commits.len(),
            lane_count,
            has_more,
            rss.unwrap_or(0),
            delta
        ));
        drop(commits);
    }
    out.push_str(&format!("max_graph_delta_kb={max_delta}\n"));
    Ok(out)
}

fn current_rss_kb() -> Option<usize> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|line| {
        let rest = line.strip_prefix("VmRSS:")?;
        rest.split_whitespace().next()?.parse::<usize>().ok()
    })
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
        .split_once(" to ")
        .map(|(source, _)| source)
        .unwrap_or(source)
        .trim()
        .trim_matches(|ch: char| ch == '\'' || ch == '"' || ch == '.' || ch == ':' || ch == ',');
    (!source.is_empty() && !git_graph_source_is_oid(source)).then(|| source.to_string())
}

fn git_graph_source_is_oid(source: &str) -> bool {
    (7..=64).contains(&source.len()) && source.bytes().all(|byte| byte.is_ascii_hexdigit())
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

#[derive(Clone, Debug)]
struct ActiveGraphLane {
    oid: String,
    column: usize,
    color_idx: usize,
    branch_name: Option<String>,
}

fn git_graph_lane(
    column: usize,
    target_column: usize,
    color_idx: usize,
    kind: GitGraphLaneKind,
) -> GitGraphLane {
    GitGraphLane {
        column: column.min(u16::MAX as usize) as u16,
        target_column: target_column.min(u16::MAX as usize) as u16,
        color_idx: color_idx.min(u16::MAX as usize) as u16,
        kind,
    }
}

fn push_unique_graph_lane(lanes: &mut Vec<GitGraphLane>, lane: GitGraphLane) {
    if !lanes.iter().any(|existing| {
        existing.column == lane.column
            && existing.target_column == lane.target_column
            && existing.kind == lane.kind
    }) {
        lanes.push(lane);
    }
}

fn last_graph_lane_index(active: &[ActiveGraphLane], oid: &str) -> Option<usize> {
    active.iter().rposition(|lane| lane.oid == oid)
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
    let mut next_color = 1usize;
    let mut max_column = 0usize;

    for commit in commits {
        let input_lanes = active.clone();
        let input_idx = input_lanes.iter().position(|lane| lane.oid == commit.oid);
        let circle_idx = input_idx.unwrap_or(input_lanes.len());
        if commit.branch_name.is_none() {
            commit.branch_name = merge_source_by_oid
                .get(&commit.oid)
                .cloned()
                .or_else(|| input_idx.and_then(|idx| input_lanes[idx].branch_name.clone()));
        }
        let commit_branch_name = commit.branch_name.clone();
        let propagating_branch_name = commit_branch_name
            .as_ref()
            .filter(|label| git_graph_branch_label_propagates(label))
            .cloned();

        let parents = commit.parent_oids.clone();
        let mut output_lanes: Vec<ActiveGraphLane> =
            Vec::with_capacity(input_lanes.len() + parents.len());
        let mut first_parent_added = false;
        if !parents.is_empty() {
            for lane in &input_lanes {
                if lane.oid == commit.oid {
                    if !first_parent_added {
                        output_lanes.push(ActiveGraphLane {
                            oid: parents[0].clone(),
                            column: output_lanes.len(),
                            color_idx: lane.color_idx,
                            branch_name: propagating_branch_name.clone(),
                        });
                        first_parent_added = true;
                    }
                } else {
                    let mut lane = lane.clone();
                    lane.column = output_lanes.len();
                    output_lanes.push(lane);
                }
            }
        }

        let first_unprocessed_parent = if first_parent_added { 1 } else { 0 };
        for (parent_idx, parent) in parents.iter().enumerate().skip(first_unprocessed_parent) {
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
            let color_idx = if parent_idx == 0 {
                input_idx
                    .map(|idx| input_lanes[idx].color_idx)
                    .unwrap_or_else(|| {
                        let color_idx = next_color;
                        next_color = next_color.saturating_add(1);
                        color_idx
                    })
            } else {
                let color_idx = next_color;
                next_color = next_color.saturating_add(1);
                color_idx
            };
            output_lanes.push(ActiveGraphLane {
                oid: parent.clone(),
                column: output_lanes.len(),
                color_idx,
                branch_name: parent_branch_name,
            });
        }

        let commit_color = output_lanes
            .get(circle_idx)
            .map(|lane| lane.color_idx)
            .or_else(|| input_lanes.get(circle_idx).map(|lane| lane.color_idx))
            .unwrap_or_else(|| {
                let color_idx = next_color;
                next_color = next_color.saturating_add(1);
                color_idx
            });

        let mut lanes = Vec::with_capacity(input_lanes.len() + output_lanes.len() + parents.len());
        let mut output_idx = 0usize;
        for (index, lane) in input_lanes.iter().enumerate() {
            if lane.oid == commit.oid {
                if index != circle_idx {
                    push_unique_graph_lane(
                        &mut lanes,
                        git_graph_lane(
                            index,
                            circle_idx,
                            lane.color_idx,
                            GitGraphLaneKind::ShiftToCommit,
                        ),
                    );
                } else {
                    output_idx = output_idx.saturating_add(1);
                }
                continue;
            }

            if output_idx < output_lanes.len() && lane.oid == output_lanes[output_idx].oid {
                if index == output_idx {
                    push_unique_graph_lane(
                        &mut lanes,
                        git_graph_lane(index, index, lane.color_idx, GitGraphLaneKind::VerticalTop),
                    );
                    push_unique_graph_lane(
                        &mut lanes,
                        git_graph_lane(
                            index,
                            index,
                            lane.color_idx,
                            GitGraphLaneKind::VerticalBottom,
                        ),
                    );
                } else {
                    push_unique_graph_lane(
                        &mut lanes,
                        git_graph_lane(index, output_idx, lane.color_idx, GitGraphLaneKind::Shift),
                    );
                }
                output_idx = output_idx.saturating_add(1);
            }
        }

        for parent in parents.iter().skip(1) {
            if let Some(parent_output_idx) = last_graph_lane_index(&output_lanes, parent) {
                push_unique_graph_lane(
                    &mut lanes,
                    git_graph_lane(
                        circle_idx,
                        parent_output_idx,
                        output_lanes[parent_output_idx].color_idx,
                        GitGraphLaneKind::Parent,
                    ),
                );
            }
        }

        if let Some(idx) = input_idx {
            push_unique_graph_lane(
                &mut lanes,
                git_graph_lane(
                    circle_idx,
                    circle_idx,
                    input_lanes[idx].color_idx,
                    GitGraphLaneKind::VerticalTop,
                ),
            );
        }
        if !parents.is_empty() {
            push_unique_graph_lane(
                &mut lanes,
                git_graph_lane(
                    circle_idx,
                    circle_idx,
                    commit_color,
                    GitGraphLaneKind::VerticalBottom,
                ),
            );
        }

        lanes.sort_by_key(|lane| {
            (
                lane.column,
                lane.target_column,
                match lane.kind {
                    GitGraphLaneKind::Vertical => 0u8,
                    GitGraphLaneKind::VerticalTop => 0u8,
                    GitGraphLaneKind::VerticalBottom => 0u8,
                    GitGraphLaneKind::Shift => 1u8,
                    GitGraphLaneKind::ShiftToCommit => 1u8,
                    GitGraphLaneKind::Parent => 2u8,
                },
            )
        });
        commit.column = circle_idx;
        commit.color_idx = commit_color;
        commit.lanes = lanes;
        max_column = max_column.max(circle_idx);
        max_column = max_column.max(input_lanes.len().saturating_sub(1));
        max_column = max_column.max(output_lanes.len().saturating_sub(1));
        active = output_lanes;
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

