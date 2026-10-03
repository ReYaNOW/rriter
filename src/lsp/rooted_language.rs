use super::{DART_SERVER, RUST_ANALYZER_SERVER, Diagnostic, LspEvent, LspProcess, LspServerStatus};
use crate::platform::{PathKey, ToolKind};
use crate::ui_waker::UiWaker;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RootedLanguage {
    Dart,
    Rust,
}

impl RootedLanguage {
    pub fn server_def(self) -> &'static super::LspServerDef {
        match self {
            Self::Dart => &DART_SERVER,
            Self::Rust => &RUST_ANALYZER_SERVER,
        }
    }

    pub fn extensions(self) -> &'static [&'static str] {
        self.server_def().extensions
    }

    pub fn root_for_path(
        self,
        path: &Path,
        workspaces: &[PathBuf],
        locate: &dyn Fn(&Path) -> Option<PathBuf>,
    ) -> Option<PathBuf> {
        match self {
            Self::Dart => {
                let file_dir = path.parent().unwrap_or(path);
                let configured = workspaces
                    .iter()
                    .filter(|workspace| crate::platform::path_is_within(path, workspace))
                    .max_by_key(|workspace| workspace.components().count());
                let search_stop = configured.map(PathBuf::as_path);
                let mut located = None;
                for ancestor in file_dir.ancestors() {
                    if let Some(root) = locate(ancestor) {
                        located = Some(root);
                        break;
                    }
                    if search_stop.is_some_and(|stop| crate::platform::paths_equal(ancestor, stop)) {
                        break;
                    }
                }
                located
                    .or_else(|| configured.cloned())
                    .or_else(|| Some(file_dir.to_path_buf()))
            }
            Self::Rust => super::rust_workspace::cargo_root_for_path(path, workspaces, locate),
        }
    }

    pub fn tool_kind(self) -> ToolKind {
        match self {
            Self::Dart => ToolKind::Dart,
            Self::Rust => ToolKind::RustAnalyzer,
        }
    }
}

#[derive(Clone)]
pub struct OpenRootedFile {
    pub path: PathBuf,
    pub root: PathBuf,
    pub text: Arc<str>,
    pub version: i32,
}

pub struct RootedWorkspaceState {
    pub root: PathBuf,
    pub process: Option<LspProcess>,
}

pub struct LiveDiagnostics {
    pub version: i32,
    pub root: PathKey,
    pub items: Arc<[Diagnostic]>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ServerHealth {
    Ok,
    Warning,
    Error,
}

pub struct RootedWorkspaces {
    pub lang: RootedLanguage,
    open_files: HashMap<PathKey, OpenRootedFile>,
    roots: HashMap<PathKey, RootedWorkspaceState>,
    closed_documents: Vec<PathBuf>,
    live_diagnostics: HashMap<PathBuf, LiveDiagnostics>,
    init_options: Option<serde_json::Value>,
    status: LspServerStatus,
    health: Option<ServerHealth>,
    busy: bool,
    enabled: bool,
    missing: bool,
    root_statuses: HashMap<PathKey, LspServerStatus>,
}

impl RootedWorkspaces {
    pub fn new(lang: RootedLanguage, enabled: bool) -> Self {
        let init_options = match lang {
            RootedLanguage::Dart => Some(super::dart_workspace::DART_INIT_OPTIONS.clone()),
            RootedLanguage::Rust => None,
        };
        Self {
            lang,
            open_files: HashMap::new(),
            roots: HashMap::new(),
            closed_documents: Vec::new(),
            live_diagnostics: HashMap::new(),
            init_options,
            status: LspServerStatus::Disabled,
            health: None,
            busy: false,
            enabled,
            missing: false,
            root_statuses: HashMap::new(),
        }
    }

    pub fn status(&self) -> LspServerStatus { self.status }
    pub fn status_ref(&self) -> &LspServerStatus { &self.status }
    pub fn health(&self) -> Option<ServerHealth> { self.health }
    pub fn busy(&self) -> bool { self.busy }
    pub fn enabled(&self) -> bool { self.enabled }
    pub fn missing(&self) -> bool { self.missing }
    pub fn init_options(&self) -> Option<&serde_json::Value> { self.init_options.as_ref() }
    pub fn open_files(&self) -> &HashMap<PathKey, OpenRootedFile> { &self.open_files }
    pub fn roots(&self) -> &HashMap<PathKey, RootedWorkspaceState> { &self.roots }
    pub fn live_diagnostics(&self) -> &HashMap<PathBuf, LiveDiagnostics> { &self.live_diagnostics }
    pub fn root_for_open_path(&self, path: &Path) -> Option<&Path> {
        self.open_files.get(&PathKey::new(path)).map(|file| file.root.as_path())
    }

    pub fn clear_missing(&mut self) {
        self.missing = false;
        self.recompute_status(false);
    }

    pub(super) fn set_pending_status(&mut self, pending: bool) {
        self.recompute_status(pending);
    }

    pub(super) fn apply_server_status(
        &mut self,
        busy: bool,
        health: ServerHealth,
    ) {
        self.busy = busy;
        self.health = Some(health);
    }

    pub fn open_document(
        &mut self,
        path: PathBuf,
        text: Arc<str>,
        version: i32,
        root: PathBuf,
        executable: Option<PathBuf>,
        ui_waker: &UiWaker,
    ) -> PathKey {
        let path_key = PathKey::new(&path);
        let root_key = PathKey::new(&root);
        self.open_files.insert(path_key.clone(), OpenRootedFile {
            path: path.clone(), root: root.clone(), text: text.clone(), version,
        });
        self.roots.entry(root_key.clone()).or_insert_with(|| RootedWorkspaceState {
            root: root.clone(), process: None,
        });
        self.ensure_process(&root_key, executable, ui_waker);
        if let Some(process) = self.roots.get_mut(&root_key).and_then(|state| state.process.as_mut()) {
            process.notify_open(&path, text, version, Some(&root));
        }
        path_key
    }

    pub fn change_document(&mut self, path: &Path, text: Arc<str>, version: i32) {
        let key = PathKey::new(path);
        let Some(open) = self.open_files.get_mut(&key) else { return; };
        if version <= open.version { return; }
        open.text = text.clone();
        open.version = version;
        let root_key = PathKey::new(&open.root);
        if let Some(process) = self.roots.get_mut(&root_key).and_then(|state| state.process.as_mut()) {
            process.notify_change(&open.path, text, version);
        }
    }

    pub fn close_document(&mut self, path: &Path) -> Option<PathKey> {
        let path_key = PathKey::new(path);
        let open = self.open_files.remove(&path_key)?;
        let root_key = PathKey::new(&open.root);
        if let Some(process) = self.roots.get_mut(&root_key).and_then(|state| state.process.as_mut()) {
            process.notify_close(&open.path);
        }
        self.remove_live_diagnostics(&open.path);
        if self.lang == RootedLanguage::Dart {
            self.closed_documents.push(open.path);
        }
        let still_open = self.open_files.values().any(|file| crate::platform::paths_equal(&file.root, &open.root));
        (!still_open).then_some(root_key)
    }

    pub fn stop_root(&mut self, root: &PathKey) -> bool {
        if let Some(mut state) = self.roots.remove(root)
            && let Some(process) = state.process.take()
        {
            process.shutdown();
        }
        self.root_statuses.remove(root);
        let before = self.live_diagnostics.len();
        self.live_diagnostics.retain(|_, diagnostics| &diagnostics.root != root);
        self.recompute_status(false);
        before != self.live_diagnostics.len()
    }

    pub fn root_has_open_files(&self, root: &PathKey) -> bool {
        self.open_files.values().any(|file| PathKey::new(&file.root) == *root)
    }

    pub fn process_for_path_mut(
        &mut self,
        path: &Path,
        executable: impl FnOnce(&Path) -> Option<PathBuf>,
        ui_waker: &UiWaker,
    ) -> Option<&mut LspProcess> {
        let root = self.open_files.get(&PathKey::new(path))?.root.clone();
        let root_key = PathKey::new(&root);
        let path = root.clone();
        self.ensure_process(&root_key, executable(&path), ui_waker);
        self.roots.get_mut(&root_key)?.process.as_mut()
    }

    pub fn poll_processes(
        &mut self,
        events: &mut Vec<LspEvent>,
        diagnostic_roots: &mut std::collections::HashMap<usize, PathKey>,
    ) {
        for (key, state) in &self.roots {
            let Some(process) = &state.process else { continue; };
            let event_start = events.len();
            process.poll(events);
            for (index, event) in events.iter().enumerate().skip(event_start) {
                if matches!(event, LspEvent::Diagnostics { server, .. } if *server == self.lang.server_def().kind) {
                    diagnostic_roots.insert(index, key.clone());
                }
                if let LspEvent::StatusChanged { server, status } = event
                    && *server == self.lang.server_def().kind
                {
                    self.root_statuses.insert(key.clone(), *status);
                }
            }
        }
        self.recompute_status(false);
    }

    pub fn mark_missing(&mut self) -> bool {
        self.missing = true;
        let keys = self.roots.keys().cloned().collect::<Vec<_>>();
        let mut diagnostics_changed = false;
        for key in keys { diagnostics_changed |= self.stop_root(&key); }
        self.recompute_status(false);
        diagnostics_changed
    }

    pub fn reconfigure(&mut self, workspaces: &[PathBuf]) {
        let files = self.open_files.values().map(|file| {
            (file.path.clone(), file.text.clone(), file.version)
        }).collect::<Vec<_>>();
        let keys = self.roots.keys().cloned().collect::<Vec<_>>();
        for key in keys {
            self.stop_root(&key);
        }
        self.open_files.clear();
        for (path, text, version) in files {
            let Some(root) = self.lang.root_for_path(&path, workspaces, &locate_dart_root) else { continue; };
            let root_key = PathKey::new(&root);
            let path_key = PathKey::new(&path);
            self.roots.entry(root_key).or_insert_with(|| RootedWorkspaceState { root: root.clone(), process: None });
            self.open_files.insert(path_key, OpenRootedFile { path, root, text, version });
        }
        self.recompute_status(false);
    }

    pub fn document_version(&self, path: &Path) -> Option<i32> {
        self.open_files.get(&PathKey::new(path)).map(|file| file.version)
    }

    pub fn drain_closed_documents(&mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.closed_documents)
    }

    pub fn set_enabled(&mut self, enabled: bool) -> bool {
        self.enabled = enabled;
        let mut diagnostics_changed = false;
        if !enabled {
            let keys = self.roots.keys().cloned().collect::<Vec<_>>();
            for key in keys { diagnostics_changed |= self.stop_root(&key); }
        }
        self.recompute_status(false);
        diagnostics_changed
    }

    pub fn restart_all(&mut self, executable: impl Fn(&Path) -> Option<PathBuf>, ui_waker: &UiWaker) -> bool {
        let keys = self.roots.keys().cloned().collect::<Vec<_>>();
        let mut diagnostics_changed = false;
        for key in keys {
            diagnostics_changed |= self.stop_root(&key);
        }
        let files = self.open_files.values().map(|file| (file.path.clone(), file.root.clone(), file.text.clone(), file.version)).collect::<Vec<_>>();
        let mut missing = false;
        for (path, root, text, version) in files {
            let path_root = root.clone();
            let exe = executable(&path_root);
            if exe.is_none() { missing = true; continue; }
            self.open_document(path, text, version, root, exe, ui_waker);
        }
        if missing { self.mark_missing(); }
        self.recompute_status(false);
        diagnostics_changed
    }

    pub fn set_init_options(&mut self, value: Option<serde_json::Value>) -> bool {
        if self.init_options == value { return false; }
        self.init_options = value;
        true
    }

    fn ensure_process(&mut self, root: &PathKey, executable: Option<PathBuf>, ui_waker: &UiWaker) {
        if !self.enabled || self.missing { return; }
        let Some(state) = self.roots.get_mut(root) else { return; };
        if state.process.is_some() { return; }
        let Some(executable) = executable else {
            self.mark_missing();
            return;
        };
        state.process = Some(LspProcess::start_with_executable(
            self.lang.server_def(), vec![state.root.clone()], Some(executable), self.init_options.as_ref(), ui_waker.clone(),
        ));
        self.root_statuses.insert(root.clone(), LspServerStatus::Starting);
        self.recompute_status(false);
    }

    pub(super) fn recompute_status(&mut self, has_pending: bool) {
        self.status = if !self.enabled { LspServerStatus::Disabled }
        else if self.missing { LspServerStatus::Missing }
        else if self.root_statuses.values().any(|status| *status == LspServerStatus::Crashed) { LspServerStatus::Crashed }
        else if self.root_statuses.values().any(|status| *status == LspServerStatus::Starting) || has_pending { LspServerStatus::Starting }
        else if self.root_statuses.values().any(|status| *status == LspServerStatus::Running) { LspServerStatus::Running }
        else { LspServerStatus::Disabled };
    }

    pub fn insert_live_diagnostics(&mut self, path: PathBuf, diagnostics: LiveDiagnostics) {
        self.live_diagnostics.insert(path, diagnostics);
    }

    pub fn remove_live_diagnostics(&mut self, path: &Path) -> bool {
        self.live_diagnostics.remove(path).is_some()
    }

    pub fn retain_live_diagnostics(&mut self, mut keep: impl FnMut(&Path, &LiveDiagnostics) -> bool) {
        self.live_diagnostics.retain(|path, diagnostics| keep(path, diagnostics));
    }

    #[cfg(test)]
    pub(super) fn insert_root_for_test(&mut self, root: PathBuf, process: Option<LspProcess>) {
        self.roots.insert(PathKey::new(&root), RootedWorkspaceState { root, process });
    }
}

fn locate_dart_root(path: &Path) -> Option<PathBuf> {
    (path.join("pubspec.yaml").is_file() || path.join("analysis_options.yaml").is_file())
        .then(|| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspaces() -> RootedWorkspaces { RootedWorkspaces::new(RootedLanguage::Dart, true) }

    #[test]
    fn close_returns_root_only_for_last_file() {
        let mut state = workspaces();
        let root = PathBuf::from("/workspace");
        state.open_files.insert(PathKey::new(Path::new("/workspace/a.dart")), OpenRootedFile { path: PathBuf::from("/workspace/a.dart"), root: root.clone(), text: Arc::from(""), version: 1 });
        state.open_files.insert(PathKey::new(Path::new("/workspace/b.dart")), OpenRootedFile { path: PathBuf::from("/workspace/b.dart"), root: root.clone(), text: Arc::from(""), version: 1 });
        assert_eq!(state.close_document(Path::new("/workspace/a.dart")), None);
        assert_eq!(state.close_document(Path::new("/workspace/b.dart")), Some(PathKey::new(&root)));
    }

    #[test]
    fn stop_root_removes_only_its_diagnostics() {
        let mut state = workspaces();
        let first = PathBuf::from("/one");
        let second = PathBuf::from("/two");
        state.roots.insert(PathKey::new(&first), RootedWorkspaceState { root: first.clone(), process: None });
        state.roots.insert(PathKey::new(&second), RootedWorkspaceState { root: second.clone(), process: None });
        for (path, root) in [("/one/a.dart", &first), ("/two/b.dart", &second)] {
            state.insert_live_diagnostics(PathBuf::from(path), LiveDiagnostics { version: 1, root: PathKey::new(root), items: Arc::from([]) });
        }
        assert!(state.stop_root(&PathKey::new(&first)));
        assert!(!state.live_diagnostics().contains_key(Path::new("/one/a.dart")));
        assert!(state.live_diagnostics().contains_key(Path::new("/two/b.dart")));
    }

    #[test]
    fn recompute_status_aggregates_root_processes() {
        let mut state = workspaces();
        assert_eq!(state.status(), LspServerStatus::Disabled);
        state.recompute_status(true);
        assert_eq!(state.status(), LspServerStatus::Starting);
        let root = PathKey::new(Path::new("/root"));
        state.roots.insert(root.clone(), RootedWorkspaceState { root: PathBuf::from("/root"), process: None });
        state.recompute_status(false);
        assert_eq!(state.status(), LspServerStatus::Disabled);
        state.root_statuses.insert(root.clone(), LspServerStatus::Running);
        state.recompute_status(false);
        assert_eq!(state.status(), LspServerStatus::Running);
        let other = PathKey::new(Path::new("/other"));
        state.roots.insert(other.clone(), RootedWorkspaceState { root: PathBuf::from("/other"), process: None });
        state.root_statuses.insert(other.clone(), LspServerStatus::Crashed);
        state.recompute_status(false);
        assert_eq!(state.status(), LspServerStatus::Crashed);
        state.root_statuses.insert(other, LspServerStatus::Starting);
        state.recompute_status(false);
        assert_eq!(state.status(), LspServerStatus::Starting);
        state.enabled = false;
        state.recompute_status(false);
        assert_eq!(state.status(), LspServerStatus::Disabled);
        state.enabled = true;
        state.missing = true;
        state.recompute_status(false);
        assert_eq!(state.status(), LspServerStatus::Missing);
    }

    #[test]
    fn deferred_stop_status_is_running_when_another_root_lives() {
        let mut state = workspaces();
        let live = PathKey::new(Path::new("/live"));
        let stopped = PathKey::new(Path::new("/stopped"));
        state.roots.insert(live.clone(), RootedWorkspaceState { root: PathBuf::from("/live"), process: None });
        state.roots.insert(stopped.clone(), RootedWorkspaceState { root: PathBuf::from("/stopped"), process: None });
        state.root_statuses.insert(live, LspServerStatus::Running);
        state.stop_root(&stopped);
        assert_eq!(state.status(), LspServerStatus::Running);
    }

    #[test]
    fn set_init_options_reports_only_changes() {
        let mut state = workspaces();
        let original = state.init_options().cloned();
        assert!(!state.set_init_options(original.clone()));
        assert!(state.set_init_options(None));
        assert!(!state.set_init_options(None));
    }

}
