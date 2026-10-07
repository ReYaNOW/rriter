use crate::app::App;
use crate::editor::{Editor, GitBlame, GitBlameState};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InlineDwellKey {
    pub(crate) tab: usize,
    pub(crate) line: usize,
    pub(crate) version: u64,
    pub(crate) generation: u64,
}

#[derive(Default)]
pub(crate) struct InlineBlameDwell {
    pub(crate) key: Option<InlineDwellKey>,
    pub(crate) since: Option<std::time::Instant>,
    pub(crate) text: String,
    pub(crate) line: Option<usize>,
}

pub(crate) fn inline_blame_matches_current(
    editor: &Editor,
    active_tab: usize,
    dwell: &InlineBlameDwell,
) -> bool {
    let line = editor.line_offsets.partition_point(|&offset| offset <= editor.cursor).saturating_sub(1);
    dwell.key.is_some_and(|key| {
        key.tab == active_tab
            && key.line == line
            && key.version == editor.version
            && key.generation == editor.git_blame.generation
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BlameKey {
    repo_root: PathBuf,
    repo_key: crate::platform::PathKey,
    rel_path: PathBuf,
    oid: git2::Oid,
}

pub(crate) struct GitBlameReceiver {
    key: BlameKey,
    generation: u64,
    rx: crate::ui_waker::OneShot<GitBlameEvent>,
}

struct GitBlameEvent {
    key: BlameKey,
    generation: u64,
    result: Result<GitBlame, String>,
}

pub(crate) struct CommitMessageReceiver {
    key: BlameKey,
    generation: u64,
    oid: git2::Oid,
    rx: crate::ui_waker::OneShot<Result<String, String>>,
    pub(crate) failed: bool,
}

impl App {
    pub(crate) fn toggle_git_blame_inline(&mut self) {
        self.git_blame_inline = !self.git_blame_inline;
        self.save_current_config();
        self.ensure_blame_for_active();
        if !self.git_blame_inline {
            clear_inline_blame(&mut self.inline_blame_dwell);
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub(crate) fn tick_git_blame_inline(&mut self, now: std::time::Instant) -> bool {
        if !self.is_ide_mode || !self.git_blame_inline {
            return clear_inline_blame(&mut self.inline_blame_dwell);
        }
        let line = self.editor.line_offsets.partition_point(|&offset| offset <= self.editor.cursor).saturating_sub(1);
        let generation = self.editor.git_blame.generation;
        let key = InlineDwellKey { tab: self.active_tab, line, version: self.editor.version, generation };
        if self.inline_blame_dwell.key != Some(key) {
            self.inline_blame_dwell.key = Some(key);
            self.inline_blame_dwell.since = Some(now);
            self.inline_blame_dwell.text.clear();
            self.inline_blame_dwell.line = None;
            return true;
        }
        if !self.inline_blame_dwell.text.is_empty() {
            return false;
        }
        let Some(since) = self.inline_blame_dwell.since else { return false };
        let delay = std::time::Duration::from_millis(u64::from(self.git_blame_delay_ms));
        if now.saturating_duration_since(since) < delay {
            return false;
        }
        let Some(snapshot) = self.editor.git_head.as_ref() else { return false };
        let Some((repo, path, oid)) = self.editor.git_blame.key.as_ref() else { return false };
        if *repo != snapshot.repo_key || *path != snapshot.rel_path || Some(*oid) != snapshot.head_oid || !snapshot.path_in_head {
            return false;
        }
        let Some(blame) = self.editor.git_blame.blame.as_ref() else { return false };
        let Some(head_line) = crate::editor::head_line_for(&self.editor.git_hunks, line) else { return false };
        let Some(commit_index) = blame.line_commit.get(head_line).copied() else { return false };
        let Some(commit) = blame.commits.get(commit_index as usize) else { return false };
        if commit.uncommitted {
            return false;
        }
        let author = commit.author.clone();
        let summary = commit.summary.clone();
        let author_time = commit.author_time;
        let now_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs().min(i64::MAX as u64) as i64);
        let relative = crate::app::git_panel::format_git_relative_time(author_time, now_secs);
        let dwell = &mut self.inline_blame_dwell;
        dwell.text.clear();
        dwell.text.push_str(&author);
        dwell.text.push_str(", ");
        dwell.text.push_str(&relative);
        dwell.text.push_str(" • ");
        dwell.text.push_str(&summary);
        dwell.line = Some(line);
        true
    }

    pub(crate) fn git_blame_inline_wake_at(&self, now: std::time::Instant) -> Option<std::time::Instant> {
        if !self.git_blame_inline || !self.inline_blame_dwell.text.is_empty() {
            return None;
        }
        self.inline_blame_dwell.since
            .map(|since| since + std::time::Duration::from_millis(u64::from(self.git_blame_delay_ms)))
            .filter(|deadline| *deadline > now)
    }

    pub(crate) fn blame_needed(&self) -> bool {
        self.git_blame_inline || self.editor.git_blame.column_open
    }

    pub(crate) fn ensure_blame_for_active(&mut self) {
        if !self.is_ide_mode || !self.blame_needed() {
            return;
        }
        let Some(snapshot) = self.editor.git_head.clone() else {
            return;
        };
        let Some(oid) = snapshot.head_oid.filter(|_| snapshot.path_in_head) else {
            return;
        };
        let key = BlameKey { repo_root: snapshot.repo_root, repo_key: snapshot.repo_key, rel_path: snapshot.rel_path, oid };
        let state = &mut self.editor.git_blame;
        if state.key.as_ref().is_some_and(|(repo, path, head)| {
            repo == &key.repo_key && path == &key.rel_path && head == &key.oid
        }) {
            if state.blame.is_some() || state.pending || state.failed_key.as_ref() == state.key.as_ref() {
                return;
            }
        } else {
            state.generation = state.generation.wrapping_add(1);
            state.key = Some((key.repo_key.clone(), key.rel_path.clone(), key.oid));
            state.blame = None;
            state.messages.clear();
            state.failed_key = None;
            state.pending = false;
        }
        state.pending = true;
        let generation = state.generation;
        let worker_key = key.clone();
        let worker = self.ui_waker.spawn_one_shot("rriter-git-blame", move || {
            let result = (|| -> Result<GitBlame, String> {
                let out = crate::app::git_panel::git_blame_porcelain(
                    &worker_key.repo_root,
                    &worker_key.oid.to_string(),
                    &worker_key.rel_path,
                )?;
                let blame = crate::editor::parse_porcelain(&out)
                    .or_else(|| out.is_empty().then(|| GitBlame {
                        commits: Vec::new(), line_commit: Vec::new(), age_ranks: Vec::new(), column_labels: Vec::new(),
                    }))
                    .ok_or_else(|| "Не удалось разобрать результат Git blame".to_string())?;
                let repo = git2::Repository::open(&worker_key.repo_root)
                    .map_err(|error| error.message().to_string())?;
                let commit = repo.find_commit(worker_key.oid)
                    .map_err(|error| error.message().to_string())?;
                let tree = commit.tree().map_err(|error| error.message().to_string())?;
                let entry = tree.get_path(&worker_key.rel_path)
                    .map_err(|error| error.message().to_string())?;
                let blob = repo.find_blob(entry.id()).map_err(|error| error.message().to_string())?;
                let text = crate::platform::decode_text_bytes(blob.content())
                    .map_err(|error| error.to_string())?.text;
                let baseline_lines = git_style_line_count(&text);
                consistent_blame(blame, baseline_lines)
                    .ok_or_else(|| "Количество строк Git blame не совпадает со снимком HEAD".to_string())
            })();
            GitBlameEvent { key: worker_key, generation, result }
        });
        match worker {
            Ok(rx) => self.git_blame_rx.push(GitBlameReceiver { key, generation, rx }),
            Err(error) => {
                self.editor.git_blame.pending = false;
                self.editor.git_blame.failed_key = self.editor.git_blame.key.clone();
                self.show_notice(format!("Не удалось запустить Git blame: {error}"));
            }
        }
    }

    pub(crate) fn poll_git_blame(&mut self) -> bool {
        self.sync_active_tab();
        let mut changed = false;
        let mut pending = Vec::with_capacity(self.git_blame_rx.len());
        for mut receiver in std::mem::take(&mut self.git_blame_rx) {
            match receiver.rx.poll() {
                crate::ui_waker::OneShotState::Pending => pending.push(receiver),
                crate::ui_waker::OneShotState::Closed => {
                    self.fail_blame_request(&receiver.key, receiver.generation);
                    changed = true;
                }
                crate::ui_waker::OneShotState::Ready(event) => {
                    let Some(state) = self.matching_blame_state_mut(&event.key, event.generation) else { continue };
                    state.pending = false;
                    match event.result {
                        Ok(blame) => {
                            state.blame = Some(blame);
                            state.generation = state.generation.wrapping_add(1);
                        },
                        Err(error) => {
                            state.blame = None;
                            state.failed_key = state.key.clone();
                            println!("[GIT blame] {error}");
                        }
                    }
                    changed = true;
                }
            }
        }
        self.git_blame_rx = pending;
        self.sync_active_tab();
        if changed && let Some(window) = self.window.as_ref() { window.request_redraw(); }
        changed
    }

    fn fail_blame_request(&mut self, key: &BlameKey, generation: u64) {
        if let Some(state) = self.matching_blame_state_mut(key, generation) {
            state.pending = false;
            state.failed_key = state.key.clone();
        }
    }

    fn matching_blame_state_mut(&mut self, key: &BlameKey, generation: u64) -> Option<&mut GitBlameState> {
        if blame_state_matches(&self.editor.git_blame, key, generation) {
            return Some(&mut self.editor.git_blame);
        }
        let index = matching_blame_state_index(self.tabs.iter().map(|tab| &tab.editor.git_blame), key, generation)?;
        Some(&mut self.tabs[index].editor.git_blame)
    }

    pub(crate) fn request_commit_message(&mut self, oid: git2::Oid) {
        let Some((repo_key, rel_path, head_oid)) = self.editor.git_blame.key.clone() else { return };
        let Some(repo_root) = self.editor.git_head.as_ref().map(|head| head.repo_root.clone()) else { return };
        let key = BlameKey { repo_root, repo_key, rel_path, oid: head_oid };
        if self.editor.git_blame.messages.iter().any(|(cached, _)| *cached == oid) { return; }
        let generation = self.editor.git_blame.generation;
        if self.git_blame_message_rx.iter().any(|receiver| {
            receiver.generation == generation && receiver.oid == oid
                && blame_keys_match(&receiver.key, &key)
        }) { return; }
        let worker_key = key.clone();
        if let Ok(rx) = self.ui_waker.spawn_one_shot("rriter-git-blame-message", move || {
            (|| {
                let repo = git2::Repository::open(&worker_key.repo_root)
                    .map_err(|error| error.message().to_string())?;
                repo.find_commit(oid)
                    .map(|commit| commit.message().unwrap_or_default().to_owned())
                    .map_err(|error| error.message().to_string())
            })()
        }) {
            self.git_blame_message_rx.push(CommitMessageReceiver { key, generation, oid, rx, failed: false });
        }
    }

    pub(crate) fn poll_git_blame_messages(&mut self) -> bool {
        self.sync_active_tab();
        let mut changed = false;
        let mut pending = Vec::with_capacity(self.git_blame_message_rx.len());
        for mut receiver in std::mem::take(&mut self.git_blame_message_rx) {
            if receiver.failed {
                if self.has_matching_blame_state(&receiver.key, receiver.generation) {
                    pending.push(receiver);
                }
                continue;
            }
            match receiver.rx.poll() {
                crate::ui_waker::OneShotState::Pending => pending.push(receiver),
                crate::ui_waker::OneShotState::Ready(result) => {
                    if let Some(state) = self.matching_blame_state_mut(&receiver.key, receiver.generation) {
                        match result {
                            Ok(message) => state.messages.push((receiver.oid, message)),
                            Err(_) => {
                                receiver.failed = true;
                                pending.push(receiver);
                            }
                        }
                        changed = true;
                    }
                }
                crate::ui_waker::OneShotState::Closed => {
                    if self.has_matching_blame_state(&receiver.key, receiver.generation) {
                        receiver.failed = true;
                        pending.push(receiver);
                    }
                }
            }
        }
        self.git_blame_message_rx = pending;
        self.sync_active_tab();
        changed
    }

    fn has_matching_blame_state(&self, key: &BlameKey, generation: u64) -> bool {
        blame_state_matches(&self.editor.git_blame, key, generation)
            || self.tabs.iter().any(|tab| blame_state_matches(&tab.editor.git_blame, key, generation))
    }
}

fn clear_inline_blame(state: &mut InlineBlameDwell) -> bool {
    let changed = !state.text.is_empty() || state.line.is_some() || state.key.is_some();
    state.text.clear();
    state.line = None;
    state.key = None;
    state.since = None;
    changed
}

fn blame_state_matches(state: &GitBlameState, key: &BlameKey, generation: u64) -> bool {
    state.generation == generation && state.key.as_ref().is_some_and(|(repo, path, oid)| {
        repo == &key.repo_key && path == &key.rel_path && oid == &key.oid
    })
}

fn blame_keys_match(left: &BlameKey, right: &BlameKey) -> bool {
    left.repo_key == right.repo_key && left.rel_path == right.rel_path && left.oid == right.oid
}

fn matching_blame_state_index<'a>(
    mut states: impl Iterator<Item = &'a GitBlameState>,
    key: &BlameKey,
    generation: u64,
) -> Option<usize> {
    states.position(|state| blame_state_matches(state, key, generation))
}

fn git_style_line_count(text: &str) -> usize {
    if text.is_empty() { 0 } else { text.bytes().filter(|byte| *byte == b'\n').count() + usize::from(!text.ends_with('\n')) }
}

fn consistent_blame(blame: GitBlame, baseline_lines: usize) -> Option<GitBlame> {
    (blame.line_commit.len() == baseline_lines).then_some(blame)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> BlameKey {
        BlameKey {
            repo_root: PathBuf::from("/repo"),
            repo_key: crate::platform::PathKey::new(std::path::Path::new("/repo")),
            rel_path: PathBuf::from("file.txt"),
            oid: git2::Oid::from_str("0000000000000000000000000000000000000001").unwrap(),
        }
    }

    #[test]
    fn stale_blame_generation_is_rejected() {
        let key = key();
        let mut state = GitBlameState::default();
        state.key = Some((key.repo_key.clone(), key.rel_path.clone(), key.oid));
        state.generation = 7;
        assert!(blame_state_matches(&state, &key, 7));
        assert!(!blame_state_matches(&state, &key, 6));
    }

    #[test]
    fn blame_result_routes_by_state_after_tab_reorder() {
        let key = key();
        let mut original = GitBlameState::default();
        original.key = Some((key.repo_key.clone(), key.rel_path.clone(), key.oid));
        original.generation = 7;
        let mut other = GitBlameState::default();
        other.key = Some((
            crate::platform::PathKey::new(std::path::Path::new("/other")),
            PathBuf::from("other.txt"),
            key.oid,
        ));
        other.generation = 7;
        let reordered = [other, original];

        assert_eq!(matching_blame_state_index(reordered.iter(), &key, 7), Some(1));
    }

    #[test]
    fn mismatched_blame_line_count_is_rejected() {
        let blame = GitBlame {
            commits: Vec::new(),
            line_commit: vec![0, 0],
            age_ranks: Vec::new(),
            column_labels: Vec::new(),
        };
        assert!(consistent_blame(blame.clone(), 1).is_none());
        assert!(consistent_blame(blame, 2).is_some());
    }

    #[test]
    fn git_line_count_ignores_trailing_empty_editor_line() {
        assert_eq!(git_style_line_count("one\r\ntwo\r\n"), 2);
        assert_eq!(git_style_line_count("one"), 1);
        assert_eq!(git_style_line_count(""), 0);
    }
}
