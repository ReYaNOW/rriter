use crate::app::{App, EditorTabKind};
use crate::editor::GitHeadSnapshot;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HeadCheckReason {
    GitAction,
    WindowFocus,
    TabActivation,
    GitMetadataChanged,
}

pub(crate) fn load_head_snapshot(path: &Path) -> Option<(GitHeadSnapshot, String)> {
    if !path.is_file() {
        return None;
    }
    let repo = git2::Repository::discover(path).ok()?;
    let repo_root = repo.workdir()?.to_path_buf();
    let rel_path = crate::platform::relative_to(path, &repo_root)?;
    let head_oid = repo.head().ok().and_then(|head| head.peel_to_commit().ok().map(|c| c.id()));
    let mut path_in_head = false;
    let mut text = String::new();
    if let Some(oid) = head_oid
        && let Ok(commit) = repo.find_commit(oid)
        && let Ok(entry) = commit.tree().and_then(|tree| tree.get_path(&rel_path))
        && let Ok(blob) = repo.find_blob(entry.id())
    {
        path_in_head = true;
        text = crate::platform::decode_text_bytes(blob.content()).ok()?.text;
    }
    let snapshot = GitHeadSnapshot {
        repo_key: crate::platform::PathKey::new(&repo_root),
        repo_root,
        rel_path,
        head_oid,
        path_in_head,
    };
    Some((snapshot, text))
}

pub(crate) fn load_workspace_head_snapshot(
    path: &Path,
    workspaces: &[PathBuf],
) -> Option<(GitHeadSnapshot, String)> {
    workspaces
        .iter()
        .any(|workspace| crate::platform::path_is_within(path, workspace))
        .then(|| load_head_snapshot(path))
        .flatten()
}

pub(crate) fn read_head_blob(
    repo: &git2::Repository,
    rel_path: &Path,
) -> Result<String, String> {
    let head = match repo.head() {
        Ok(head) => head,
        Err(_) => return Ok(String::new()),
    };
    let tree = head
        .peel_to_tree()
        .map_err(|error| format!("HEAD tree: {}", error.message()))?;
    let entry = match tree.get_path(rel_path) {
        Ok(entry) => entry,
        Err(_) => return Ok(String::new()),
    };
    let blob = repo
        .find_blob(entry.id())
        .map_err(|error| format!("HEAD blob: {}", error.message()))?;
    crate::platform::decode_text_bytes(blob.content())
        .map(|decoded| decoded.text)
        .map_err(|error| format!("HEAD blob: {error}"))
}

impl App {
    pub(crate) fn on_window_focus_gained(&mut self) {
        self.check_git_heads(HeadCheckReason::WindowFocus);
    }

    pub(crate) fn refresh_current_editor_git_base(&mut self) {
        let snapshot = self
            .file_path
            .as_deref()
            .filter(|_| self.is_ide_mode && !self.active_tab_is_git_diff())
            .map(|path| self.abs_path_for_workspace(path))
            .and_then(|path| load_workspace_head_snapshot(&path, &self.ide_workspaces));
        let (git_head, base_text) = match snapshot {
            Some((snapshot, text)) => (Some(snapshot), Some(text)),
            None => (None, None),
        };
        self.editor.set_git_head_snapshot(git_head, base_text);
        self.inline_git_popup = None;
    }

    pub(crate) fn check_git_heads(&mut self, _reason: HeadCheckReason) {
        if !self.is_ide_mode || self.tabs.is_empty() {
            return;
        }
        self.sync_active_tab();
        let mut heads = std::collections::HashMap::new();
        let mut changed = false;
        for tab in &mut self.tabs {
            if !matches!(tab.kind, EditorTabKind::Normal) {
                continue;
            }
            if tab.load != crate::app::TabLoad::Loaded {
                continue;
            }
            let Some(path) = tab.file_path.as_deref() else {
                continue;
            };
            let in_workspace = self
                .ide_workspaces
                .iter()
                .any(|workspace| crate::platform::path_is_within(path, workspace));
            if !in_workspace {
                continue;
            }
            let repo_root = tab
                .editor
                .git_head
                .as_ref()
                .map(|snapshot| snapshot.repo_root.clone())
                .or_else(|| {
                    let repo = git2::Repository::discover(path).ok().or_else(|| {
                        self.ide_workspaces
                            .iter()
                            .find(|workspace| crate::platform::path_is_within(path, workspace))
                            .and_then(|workspace| git2::Repository::discover(workspace).ok())
                    })?;
                    repo.workdir().map(Path::to_path_buf)
                });
            let Some(repo_root) = repo_root else {
                continue;
            };
            let repo_key = crate::platform::PathKey::new(&repo_root);
            let head_oid = *heads.entry(repo_key).or_insert_with(|| {
                git2::Repository::open(&repo_root).ok().and_then(|repo| {
                    repo.head()
                        .ok()
                        .and_then(|head| head.peel_to_commit().ok().map(|commit| commit.id()))
                })
            });
            let differs = !path.is_file()
                || tab
                    .editor
                    .git_head
                    .as_ref()
                    .is_none_or(|snapshot| snapshot.head_oid != head_oid);
            if differs {
                let loaded = load_head_snapshot(path);
                let (snapshot, text) = loaded
                    .map(|(snapshot, text)| (Some(snapshot), Some(text)))
                    .unwrap_or((None, None));
                tab.editor.set_git_head_snapshot(snapshot, text);
                changed = true;
            }
        }
        self.sync_active_tab();
        if changed {
            self.inline_git_popup = None;
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};

    #[test]
    fn snapshot_for_committed_file_contains_head_and_path() {
        let (repo, path) = git_baseline_test_repo("committed");
        let (snapshot, text) = load_head_snapshot(&path).expect("snapshot");
        assert_eq!(snapshot.repo_root, repo.path().parent().unwrap());
        assert_eq!(snapshot.rel_path, PathBuf::from("file.txt"));
        assert_eq!(snapshot.head_oid, repo.head().unwrap().target());
        assert!(snapshot.path_in_head);
        assert_eq!(text, "baseline\n");
    }

    #[test]
    fn snapshot_for_untracked_file_marks_path_missing_from_head() {
        let (repo, committed_path) = git_baseline_test_repo("untracked");
        let path = committed_path.with_file_name("untracked.txt");
        fs::write(&path, "untracked\n").unwrap();
        let (snapshot, text) = load_head_snapshot(&path).expect("snapshot");
        assert_eq!(snapshot.head_oid, repo.head().unwrap().target());
        assert!(!snapshot.path_in_head);
        assert_eq!(text, "");
    }

    #[test]
    fn snapshot_for_unborn_repo_has_no_head_or_path() {
        let root = git_baseline_test_root("unborn");
        fs::create_dir_all(&root).unwrap();
        let repo = git2::Repository::init(&root).unwrap();
        let path = root.join("file.txt");
        fs::write(&path, "working\n").unwrap();
        let (snapshot, text) = load_head_snapshot(&path).expect("snapshot");
        assert!(snapshot.head_oid.is_none());
        assert!(!snapshot.path_in_head);
        assert_eq!(text, "");
        drop(repo);
    }

    #[test]
    fn path_outside_repo_has_no_snapshot() {
        let path = git_baseline_test_root("outside").join("file.txt");
        assert!(load_head_snapshot(&path).is_none());
    }

    #[test]
    fn directory_and_missing_path_have_no_snapshot() {
        let (repo, path) = git_baseline_test_repo("invalid-path");
        assert!(load_head_snapshot(repo.path().parent().unwrap()).is_none());
        assert!(load_head_snapshot(&path.with_file_name("missing.txt")).is_none());
    }

    fn git_baseline_test_repo(name: &str) -> (git2::Repository, PathBuf) {
        let root = git_baseline_test_root(name);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let repo = git2::Repository::init(&root).unwrap();
        let path = root.join("file.txt");
        fs::write(&path, "baseline\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("file.txt")).unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let signature = git2::Signature::now("Tester", "tester@example.com").unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
            .unwrap();
        drop(tree);
        (repo, path)
    }

    fn git_baseline_test_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("rriter_git_baseline_{name}_{}", std::process::id()))
    }
}
