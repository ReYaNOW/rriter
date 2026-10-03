#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LangRoute {
    Python,
    Rooted(rooted_language::RootedLanguage),
}

pub struct LspServerSummary<'a> {
    pub name: &'static str,
    pub status: &'a LspServerStatus,
    pub log_count: usize,
}

#[cfg(test)]
mod language_route_tests {
    use super::*;

    #[test]
    fn language_for_ext_routes_supported_languages() {
        assert_eq!(LspManager::language_for_ext(concat!("p", "y")), Some(LangRoute::Python));
        assert_eq!(LspManager::language_for_ext(concat!("p", "y", "i")), Some(LangRoute::Python));
        assert_eq!(
            LspManager::language_for_ext(concat!("da", "rt")),
            Some(LangRoute::Rooted(rooted_language::RootedLanguage::Dart))
        );
        assert_eq!(LspManager::language_for_ext("rs"), Some(LangRoute::Rooted(rooted_language::RootedLanguage::Rust)));
        assert_eq!(LspManager::language_for_ext("PY"), None);
        assert_eq!(LspManager::language_for_ext("txt"), None);
    }

    #[test]
    fn ide_completion_ignores_supported_extensions_without_processes() {
        let mut manager = LspManager::new(Vec::new());
        assert_eq!(
            manager.request_ide_completion(
                &PathBuf::from(concat!("sample.", "da", "rt")),
                concat!("da", "rt"),
                0,
                0,
                None,
            ),
            None
        );
        assert_eq!(
            manager.request_ide_completion(&PathBuf::from("notes.txt"), "txt", 0, 0, None),
            None
        );
    }
}

fn python_line_count(text: &str) -> usize {
    text.as_bytes()
        .iter()
        .filter(|&&byte| byte == b'\n')
        .count()
        .saturating_add(1)
}

#[derive(Clone, Debug)]
struct OpenPythonFile {
    path: PathBuf,
    _lines: usize,
}

pub struct LspManager {
    python: Option<LspProcess>,
    ty_process: Option<LspProcess>,
    workspaces: Vec<PathBuf>,
    active_workspaces: Vec<PathBuf>,
    open_python_files: HashMap<crate::platform::PathKey, OpenPythonFile>,
    pub(super) dart: rooted_language::RootedWorkspaces,
    pub(super) rust: rooted_language::RootedWorkspaces,
    pub(super) rust_documents: HashMap<crate::platform::PathKey, rooted_language::OpenRootedFile>,
    pub(super) rust_roots: HashMap<crate::platform::PathKey, rust_workspace::RustRootEntry>,
    pub(super) rust_pending_docs: HashMap<crate::platform::PathKey, rooted_language::OpenRootedFile>,
    pub(super) rust_tools: Option<rust_workspace::RustTools>,
    pub(super) rust_roots_generation: u64,
    pub(super) rust_last_health_message: Option<String>,
    #[cfg(test)]
    rust_tools_override_for_test: Option<rust_workspace::RustTools>,
    #[cfg(test)]
    rust_run_cmd: Option<std::sync::Arc<rust_workspace::RunCmd>>,
    pub(super) dart_jobs: HashMap<crate::platform::PathKey, dart_workspace::DartAnalyzerState>,
    pub(super) dart_job_roots: HashMap<crate::platform::PathKey, PathBuf>,
    /// Актуальные диагностики для каждого открытого файла
    pub diagnostics: HashMap<PathBuf, Arc<[Diagnostic]>>,
    pub instant_diagnostics: HashMap<PathBuf, (i32, Arc<[Diagnostic]>)>,
    ruff_workspace_diagnostics: HashMap<PathBuf, Arc<[Diagnostic]>>,
    pub ty_instant_diagnostics: HashMap<PathBuf, (i32, Arc<[Diagnostic]>)>,
    dart_workspace_diagnostics: HashMap<PathBuf, Arc<[Diagnostic]>>,
    diagnostic_generation: u64,
    diagnostic_ancestor_severities: HashMap<PathBuf, DiagSeverity>,
    diagnostic_total_counts: (usize, usize),
    ty_diag_result_ids: HashMap<PathBuf, String>,
    /// One shared copy of every diagnostic code, URL and source text in the store;
    /// `prune_diag_text_pool` drops the entries no diagnostic references any more.
    diag_text_pool: std::collections::HashSet<Arc<str>>,
    ruff_workspace_diag_rx: Option<crate::ui_waker::OneShot<ruff_workspace::RuffWorkspaceResult>>,
    ruff_workspace_diag_pending: bool,
    ruff_workspace_diag_dirty: bool,
    ty_workspace_diag_pending: Option<i32>,
    ty_workspace_diag_dirty: bool,
    pub dirty_diagnostics: bool,
    pub last_change: Option<std::time::Instant>,
    current_path: Option<PathBuf>,
    current_python_file: Option<(PathBuf, Arc<str>, i32)>,
    current_python_lines: Option<usize>,
    /// Статус ruff сервера
    pub python_status: LspServerStatus,
    pub ty_status: LspServerStatus,
    /// Отключены ли Python-серверы вручную целиком.
    pub python_disabled: bool,
    ruff_disabled: bool,
    ty_disabled: bool,
    ruff_unavailable: bool,
    ty_unavailable: bool,
    dart_workspace_analysis_enabled: bool,
    pub server_logs: HashMap<&'static str, Vec<LogEntry>>,
    pub suppress_diagnostics: bool,
    /// Every server process and workspace job started by this manager wakes the UI with it.
    ui_waker: crate::ui_waker::UiWaker,
}

impl LspManager {
    pub fn dart_status(&self) -> LspServerStatus {
        self.dart.status()
    }

    pub fn rust_status(&self) -> LspServerStatus { self.rust.status() }

    pub fn diagnostic_generation(&self) -> u64 {
        self.diagnostic_generation
    }

    pub(super) fn mark_diagnostics_changed(&mut self) {
        self.diagnostic_generation = self.diagnostic_generation.wrapping_add(1);
        self.dirty_diagnostics = true;
    }

    pub fn language_for_ext(ext: &str) -> Option<LangRoute> {
        match ext {
            "py" | "pyi" => Some(LangRoute::Python),
            "dart" => Some(LangRoute::Rooted(rooted_language::RootedLanguage::Dart)),
            "rs" => Some(LangRoute::Rooted(rooted_language::RootedLanguage::Rust)),
            _ => None,
        }
    }

    pub(super) fn rooted_mut(
        &mut self,
        lang: rooted_language::RootedLanguage,
    ) -> &mut rooted_language::RootedWorkspaces {
        match lang {
            rooted_language::RootedLanguage::Dart => &mut self.dart,
            rooted_language::RootedLanguage::Rust => &mut self.rust,
        }
    }

    pub(super) fn rooted(
        &self,
        lang: rooted_language::RootedLanguage,
    ) -> &rooted_language::RootedWorkspaces {
        match lang {
            rooted_language::RootedLanguage::Dart => &self.dart,
            rooted_language::RootedLanguage::Rust => &self.rust,
        }
    }

    #[cfg(test)]
    pub fn new(workspaces: Vec<PathBuf>) -> Self {
        Self::with_ui_waker(workspaces, crate::ui_waker::UiWaker::counting())
    }

    /// Python files `notify_open` was called for (the LSP's open document set).
    #[cfg(test)]
    pub(crate) fn open_python_paths_for_test(&self) -> Vec<PathBuf> {
        self.open_python_files.values().map(|file| file.path.clone()).collect()
    }

    pub fn with_ui_waker(workspaces: Vec<PathBuf>, ui_waker: crate::ui_waker::UiWaker) -> Self {
        let mut manager = LspManager {
            python: None,
            ty_process: None,
            workspaces: crate::platform::dedup_paths(workspaces),
            active_workspaces: Vec::new(),
            open_python_files: HashMap::new(),
            dart: rooted_language::RootedWorkspaces::new(rooted_language::RootedLanguage::Dart, true),
            rust: rooted_language::RootedWorkspaces::new(rooted_language::RootedLanguage::Rust, true),
            rust_documents: HashMap::new(),
            rust_roots: HashMap::new(),
            rust_pending_docs: HashMap::new(),
            rust_tools: None,
            rust_roots_generation: 0,
            rust_last_health_message: None,
            #[cfg(test)]
            rust_tools_override_for_test: None,
            #[cfg(test)]
            rust_run_cmd: None,
            dart_jobs: HashMap::new(),
            dart_job_roots: HashMap::new(),
            diagnostics: HashMap::new(),
            instant_diagnostics: HashMap::new(),
            ruff_workspace_diagnostics: HashMap::new(),
            ty_instant_diagnostics: HashMap::new(),
            dart_workspace_diagnostics: HashMap::new(),
            diagnostic_generation: 0,
            diagnostic_ancestor_severities: HashMap::new(),
            diagnostic_total_counts: (0, 0),
            ty_diag_result_ids: HashMap::new(),
            diag_text_pool: std::collections::HashSet::new(),
            ruff_workspace_diag_rx: None,
            ruff_workspace_diag_pending: false,
            ruff_workspace_diag_dirty: false,
            ty_workspace_diag_pending: None,
            ty_workspace_diag_dirty: false,
            dirty_diagnostics: false,
            last_change: None,
            current_path: None,
            current_python_file: None,
            current_python_lines: None,
            python_status: LspServerStatus::Disabled,
            ty_status: LspServerStatus::Disabled,
            python_disabled: false,
            ruff_disabled: false,
            ty_disabled: false,
            ruff_unavailable: false,
            ty_unavailable: false,
            dart_workspace_analysis_enabled: true,
            server_logs: HashMap::new(),
            suppress_diagnostics: false,
            ui_waker,
        };
        manager.schedule_configured_dart_projects();
        manager
    }

    fn relative_lookup_path(&self, path: &Path) -> PathBuf {
        if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        }
    }

    fn configured_workspace_for_path(&self, path: &Path) -> Option<&PathBuf> {
        self.workspaces
            .iter()
            .filter(|ws| crate::platform::path_is_within(path, ws))
            .max_by_key(|ws| ws.components().count())
    }

    fn refresh_active_workspaces(&mut self) -> bool {
        let mut next = Vec::new();
        for ws in &self.workspaces {
            if self
                .open_python_files
                .values()
                .any(|open| crate::platform::path_is_within(&open.path, ws))
            {
                next.push(ws.clone());
            }
        }
        if self.active_workspaces.len() == next.len()
            && self
                .active_workspaces
                .iter()
                .zip(&next)
                .all(|(left, right)| crate::platform::paths_equal(left, right))
        {
            return false;
        }
        self.active_workspaces = next;
        true
    }


    fn reset_ty_workspace_state(&mut self) {
        self.ty_diag_result_ids.clear();
        self.diagnostic_ancestor_severities.clear();
        self.diagnostic_total_counts = (0, 0);
        self.diag_text_pool.clear();
        self.mark_diagnostics_changed();
        self.ty_workspace_diag_pending = None;
        self.ty_workspace_diag_dirty =
            !self.open_python_files.is_empty() && !self.active_workspaces.is_empty();
        self.ruff_workspace_diag_dirty = self.ty_workspace_diag_dirty;
    }

    fn start_ruff_process_if_available(&mut self, workspaces: &[PathBuf]) -> bool {
        if self.python.is_some() {
            return false;
        }
        if self.ruff_disabled || self.ruff_unavailable {
            return false;
        }
        self.python_status = LspServerStatus::Starting;
        self.python = Some(LspProcess::start(
            &RUFF_SERVER,
            workspaces.to_vec(),
            self.ui_waker.clone(),
        ));
        true
    }

    fn start_ty_process_if_available(&mut self, workspaces: &[PathBuf]) -> bool {
        if self.ty_process.is_some() {
            return false;
        }
        if self.ty_disabled || self.ty_unavailable {
            return false;
        }
        self.ty_status = LspServerStatus::Starting;
        self.ty_process = Some(LspProcess::start(
            &TY_SERVER,
            workspaces.to_vec(),
            self.ui_waker.clone(),
        ));
        true
    }

    fn sync_python_processes_after_open_set_change(&mut self, reopen_current: bool) {
        self.prune_inactive_workspace_diagnostics();
        self.reset_ty_workspace_state();

        if self.open_python_files.is_empty() {
            if let Some(p) = self.python.take() {
                p.shutdown();
            }
            if let Some(p) = self.ty_process.take() {
                p.shutdown();
            }
            self.python_status = LspServerStatus::Disabled;
            self.ty_status = LspServerStatus::Disabled;
            self.ty_workspace_diag_dirty = false;
            self.ruff_workspace_diag_rx = None;
            self.ruff_workspace_diag_pending = false;
            self.ruff_workspace_diag_dirty = false;
            return;
        }

        if self.python_disabled {
            return;
        }

        let workspaces = self.active_workspaces.clone();
        if let Some(p) = self.python.take() {
            p.shutdown();
        }
        if let Some(p) = self.ty_process.take() {
            p.shutdown();
        }
        self.start_ruff_process_if_available(&workspaces);
        self.start_ty_process_if_available(&workspaces);
        self.reset_ty_workspace_state();

        if reopen_current {
            self.reopen_current_python_file();
        }
    }

    fn note_open_python_file(&mut self, path: PathBuf, lines: usize) -> bool {
        let had_open = !self.open_python_files.is_empty();
        self.open_python_files.insert(
            crate::platform::PathKey::new(&path),
            OpenPythonFile {
                path,
                _lines: lines,
            },
        );
        let active_changed = self.refresh_active_workspaces();
        active_changed || had_open != !self.open_python_files.is_empty()
    }

    fn note_close_python_file(&mut self, path: &PathBuf) -> bool {
        let had_open = !self.open_python_files.is_empty();
        self.open_python_files
            .remove(&crate::platform::PathKey::new(path));
        let active_changed = self.refresh_active_workspaces();
        active_changed || had_open != !self.open_python_files.is_empty()
    }

    pub fn set_workspaces(&mut self, workspaces: Vec<PathBuf>) {
        self.workspaces = crate::platform::dedup_paths(workspaces);
        self.reconfigure_dart_workspaces();
        self.refresh_rust_resolution();
        if self.dart.open_files().is_empty() {
            self.schedule_configured_dart_projects();
        }
        if self.refresh_active_workspaces() {
            self.sync_python_processes_after_open_set_change(true);
        } else {
            self.prune_inactive_workspace_diagnostics();
        }
    }

    /// Запускает нужный LSP-сервер если ещё не запущен (lazy)
    fn ensure_python(&mut self) {
        if self.open_python_files.is_empty() {
            return;
        }
        if self.python_disabled {
            return;
        }
        let workspaces = self.active_workspaces.clone();
        self.start_ruff_process_if_available(&workspaces);
        if self.start_ty_process_if_available(&workspaces) {
            self.reset_ty_workspace_state();
        }
    }

    /// Перезапустить ruff сервер
    pub fn restart_python(&mut self) {
        if self.open_python_files.is_empty() || self.python_disabled {
            return;
        }
        self.ruff_unavailable = false;
        self.ty_unavailable = false;
        self.sync_python_processes_after_open_set_change(true);
    }

    /// Отключить ruff (остановить и не перезапускать)
    pub fn disable_python(&mut self) {
        self.python_disabled = true;
        self.ruff_disabled = true;
        self.ty_disabled = true;
        self.python_status = LspServerStatus::Disabled;
        self.ty_status = LspServerStatus::Disabled;
        if let Some(p) = self.python.take() {
            p.shutdown();
        }
        if let Some(p) = self.ty_process.take() {
            p.shutdown();
        }
        self.mark_diagnostics_changed();
        self.diagnostics.clear();
        self.instant_diagnostics.clear();
        self.ruff_workspace_diagnostics.clear();
        self.ty_instant_diagnostics.clear();
        self.ty_diag_result_ids.clear();
        self.diagnostic_ancestor_severities.clear();
        self.diagnostic_total_counts = (0, 0);
        self.diag_text_pool.clear();
        self.ruff_workspace_diag_rx = None;
        self.ruff_workspace_diag_pending = false;
        self.ruff_workspace_diag_dirty = false;
        self.ty_workspace_diag_pending = None;
        self.ty_workspace_diag_dirty = false;
        self.dirty_diagnostics = false;
        self.server_logs.clear();
    }

    /// Включить ruff обратно
    pub fn enable_python(&mut self) {
        self.python_disabled = false;
        self.ruff_disabled = false;
        self.ty_disabled = false;
        self.ruff_unavailable = false;
        self.ty_unavailable = false;
        if self.open_python_files.is_empty() {
            self.python_status = LspServerStatus::Disabled;
            self.ty_status = LspServerStatus::Disabled;
            return;
        }
        let workspaces = self.active_workspaces.clone();
        self.start_ruff_process_if_available(&workspaces);
        self.start_ty_process_if_available(&workspaces);
        self.reset_ty_workspace_state();
        self.current_python_lines = self
            .current_python_file
            .as_ref()
            .map(|(_, text, _)| python_line_count(text.as_ref()));
        self.reopen_current_python_file();
    }

    pub fn restart_server(&mut self, name: &str) {
        if name == RUST_ANALYZER_SERVER.program {
            self.refresh_rust_resolution();
            return;
        }
        if name == DART_SERVER.program {
            self.dart.set_enabled(true);
            self.dart.clear_missing();
            self.reconfigure_dart_workspaces();
            return;
        }
        if self.open_python_files.is_empty() || self.python_disabled {
            return;
        }
        let workspaces = self.active_workspaces.clone();
        match name {
            name if name == RUFF_SERVER.program => {
                self.ruff_disabled = false;
                self.ruff_unavailable = false;
                if let Some(process) = self.python.take() {
                    process.shutdown();
                }
                self.start_ruff_process_if_available(&workspaces);
            }
            name if name == TY_SERVER.program => {
                self.ty_disabled = false;
                self.ty_unavailable = false;
                if let Some(process) = self.ty_process.take() {
                    process.shutdown();
                }
                if self.start_ty_process_if_available(&workspaces) {
                    self.reset_ty_workspace_state();
                }
            }
            _ => return,
        }
        self.reopen_current_python_file();
    }

    pub fn set_server_enabled(&mut self, name: &str, enabled: bool) {
        if name == RUST_ANALYZER_SERVER.program {
            self.set_rust_enabled(enabled);
            return;
        }
        if name == DART_SERVER.program {
            if enabled {
                self.dart.set_enabled(true);
                self.dart.clear_missing();
                self.reconfigure_dart_workspaces();
            } else {
                for state in self.dart_jobs.values_mut() {
                    state.cancel_job();
                    state.generation = state.generation.wrapping_add(1).max(1);
                    state.due_at = None;
                }
                let changed = self.dart.set_enabled(false);
                self.dart_workspace_diagnostics.clear();
                if changed {
                    self.mark_diagnostics_changed();
                    self.prune_diag_text_pool();
                }
            }
            self.rebuild_diagnostic_summary();
            return;
        }
        self.python_disabled = false;
        let workspaces = self.active_workspaces.clone();
        match name {
            name if name == RUFF_SERVER.program => {
                self.ruff_disabled = !enabled;
                if enabled {
                    self.ruff_unavailable = false;
                    self.start_ruff_process_if_available(&workspaces);
                    self.reopen_current_python_file();
                } else {
                    if let Some(process) = self.python.take() {
                        process.shutdown();
                    }
                    self.python_status = LspServerStatus::Disabled;
                    self.mark_diagnostics_changed();
                    self.ruff_workspace_diagnostics.clear();
                    self.ruff_workspace_diag_rx = None;
                    self.ruff_workspace_diag_pending = false;
                    self.ruff_workspace_diag_dirty = false;
                }
            }
            name if name == TY_SERVER.program => {
                self.ty_disabled = !enabled;
                if enabled {
                    self.ty_unavailable = false;
                    if self.start_ty_process_if_available(&workspaces) {
                        self.reset_ty_workspace_state();
                    }
                    self.reopen_current_python_file();
                } else {
                    if let Some(process) = self.ty_process.take() {
                        process.shutdown();
                    }
                    self.ty_status = LspServerStatus::Disabled;
                    self.mark_diagnostics_changed();
                    self.ty_instant_diagnostics.clear();
                    self.ty_diag_result_ids.clear();
                    self.ty_workspace_diag_pending = None;
                    self.ty_workspace_diag_dirty = false;
                }
            }
            _ => return,
        }
        self.rebuild_diagnostic_summary();
    }

    pub fn stop_server(&mut self, name: &str) {
        self.set_server_enabled(name, false);
    }

    fn reopen_current_python_file(&mut self) {
        let Some((path, text, version)) = self.current_python_file.clone() else {
            return;
        };
        let ws = self.configured_workspace_for_path(&path).cloned();
        if let Some(proc) = &mut self.python {
            proc.notify_open(&path, text.clone(), version, ws.as_ref());
        }
        if let Some(proc) = &mut self.ty_process {
            proc.notify_open(&path, text.clone(), version, ws.as_ref());
        }
    }

    /// Лёгкая информация о серверах без клонирования логов.
    pub fn server_summaries(&self) -> [LspServerSummary<'_>; 4] {
        [
            LspServerSummary {
                name: RUFF_SERVER.program,
                status: &self.python_status,
                log_count: self
                    .server_logs
                    .get(RUFF_SERVER.program)
                    .map_or(0, Vec::len),
            },
            LspServerSummary {
                name: TY_SERVER.program,
                status: &self.ty_status,
                log_count: self.server_logs.get(TY_SERVER.program).map_or(0, Vec::len),
            },
            LspServerSummary {
                name: dart_workspace::DART_SERVER_NAME,
                status: self.dart.status_ref(),
                log_count: self
                    .server_logs
                    .get(dart_workspace::DART_SERVER_NAME)
                    .map_or(0, Vec::len),
            },
            LspServerSummary {
                name: RUST_ANALYZER_SERVER.program,
                status: self.rust.status_ref(),
                log_count: self.server_logs.get(RUST_ANALYZER_SERVER.program).map_or(0, Vec::len),
            },
        ]
    }

    /// Информация о серверах для UI
    pub fn servers_info(&self) -> Vec<LspServerInfo> {
        let logs = self
            .server_logs
            .get(RUFF_SERVER.program)
            .cloned()
            .unwrap_or_default();
        let ty_logs = self
            .server_logs
            .get(TY_SERVER.program)
            .cloned()
            .unwrap_or_default();
        let dart_logs = self
            .server_logs
            .get(dart_workspace::DART_SERVER_NAME)
            .cloned()
            .unwrap_or_default();
        let rust_logs = self.server_logs.get(RUST_ANALYZER_SERVER.program).cloned().unwrap_or_default();
        vec![
            LspServerInfo {
                name: RUFF_SERVER.program,
                status: self.python_status.clone(),
                logs,
            },
            LspServerInfo {
                name: TY_SERVER.program,
                status: self.ty_status.clone(),
                logs: ty_logs,
            },
            LspServerInfo {
                name: dart_workspace::DART_SERVER_NAME,
                status: self.dart_status(),
                logs: dart_logs,
            },
            LspServerInfo {
                name: RUST_ANALYZER_SERVER.program,
                status: self.rust_status(),
                logs: rust_logs,
            },
        ]
    }

    pub fn clear_server_logs(&mut self, name: &str) {
        self.server_logs.remove(name);
    }

    fn ide_process_for_document(&mut self, path: &Path, ext: &str) -> Option<&mut LspProcess> {
        match Self::language_for_ext(ext)? {
            LangRoute::Python => {
                self.ensure_python();
                self.ty_process.as_mut()
            }
            LangRoute::Rooted(lang) => self.rooted_process_for_path_mut(lang, path),
        }
    }

    fn action_process_for_document(&mut self, path: &Path, ext: &str) -> Option<&mut LspProcess> {
        match Self::language_for_ext(ext)? {
            LangRoute::Python => {
                self.ensure_python();
                self.python.as_mut()
            }
            LangRoute::Rooted(lang) => self.rooted_process_for_path_mut(lang, path),
        }
    }

    fn rooted_process_for_path_mut(
        &mut self,
        lang: rooted_language::RootedLanguage,
        path: &Path,
    ) -> Option<&mut LspProcess> {
        let root = self.rooted(lang).root_for_open_path(path)?.to_path_buf();
        let executable = match lang {
            rooted_language::RootedLanguage::Dart => dart_workspace::dart_executable_for_root(&root),
            rooted_language::RootedLanguage::Rust => self.rust_executable_for_root(&root)?,
        };
        let ui_waker = self.ui_waker.clone();
        self.rooted_mut(lang)
            .process_for_path_mut(path, |_| Some(executable), &ui_waker)
    }

    /// Уведомляет LSP об открытии файла
    pub fn notify_open(&mut self, path: &PathBuf, ext: &str, text: &str, version: i32) {
        self.suppress_diagnostics = false;
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        self.current_path = Some(abs_path.clone());
        let Some(route) = Self::language_for_ext(ext) else {
            self.current_python_file = None;
            self.current_python_lines = None;
            return;
        };
        match route {
            LangRoute::Python => {
            let text = Arc::<str>::from(text);
            let lines = python_line_count(text.as_ref());
            self.current_python_file = Some((abs_path.clone(), text.clone(), version));
            self.current_python_lines = Some(lines);
            let open_set_changed = self.note_open_python_file(abs_path.clone(), lines);
            if open_set_changed {
                self.sync_python_processes_after_open_set_change(false);
            } else {
                self.ensure_python();
            }
            let ws = self.configured_workspace_for_path(&abs_path).cloned();
            if let Some(proc) = &mut self.python {
                proc.notify_open(&abs_path, text.clone(), version, ws.as_ref());
            }
            if let Some(proc) = &mut self.ty_process {
                proc.notify_open(&abs_path, text.clone(), version, ws.as_ref());
            }
            self.ty_workspace_diag_dirty = true;
            self.ruff_workspace_diag_dirty = true;
            self.mark_diagnostics_changed();
            }
            LangRoute::Rooted(lang) => {
                self.current_python_file = None;
                self.current_python_lines = None;
                match lang {
                    rooted_language::RootedLanguage::Dart => {
                        self.open_dart_document(abs_path, Arc::<str>::from(text), version);
                    }
                    rooted_language::RootedLanguage::Rust => {
                        self.open_rust_document(abs_path, Arc::<str>::from(text), version);
                    }
                }
            }
        }
    }

    /// Уведомляет LSP об изменении файла (когда sync_edits непуст)
    pub fn notify_change(&mut self, path: &PathBuf, ext: &str, text: &str, version: i32) {
        self.suppress_diagnostics = false;
        self.last_change = Some(std::time::Instant::now());
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        let Some(route) = Self::language_for_ext(ext) else {
            return;
        };
        match route {
            LangRoute::Python => {
            self.current_path = Some(abs_path.clone());
            let text = Arc::<str>::from(text);
            let lines = python_line_count(text.as_ref());
            let was_open = self
                .open_python_files
                .contains_key(&crate::platform::PathKey::new(&abs_path));
            self.current_python_file = Some((abs_path.clone(), text.clone(), version));
            self.current_python_lines = Some(lines);
            let open_set_changed = self.note_open_python_file(abs_path.clone(), lines);
            if open_set_changed {
                self.sync_python_processes_after_open_set_change(false);
            } else {
                self.ensure_python();
            }
            let ws = self.configured_workspace_for_path(&abs_path).cloned();
            if let Some(proc) = &mut self.python {
                if was_open {
                    proc.notify_change(&abs_path, text.clone(), version);
                } else {
                    proc.notify_open(&abs_path, text.clone(), version, ws.as_ref());
                }
            }
            if let Some(proc) = &mut self.ty_process {
                if was_open {
                    proc.notify_change(&abs_path, text.clone(), version);
                } else {
                    proc.notify_open(&abs_path, text.clone(), version, ws.as_ref());
                }
            }
            self.ty_workspace_diag_dirty = true;
            self.ruff_workspace_diag_dirty = true;
            self.mark_diagnostics_changed();
            }
            LangRoute::Rooted(lang) => {
                let path_key = crate::platform::PathKey::new(&abs_path);
                let is_open = self.rooted(lang).open_files().contains_key(&path_key);
                let is_rust_pending = lang == rooted_language::RootedLanguage::Rust
                    && self.rust_documents.contains_key(&path_key);
                if is_open || is_rust_pending {
                    self.current_path = Some(abs_path.clone());
                    self.current_python_file = None;
                    self.current_python_lines = None;
                    match lang {
                        rooted_language::RootedLanguage::Dart => {
                            self.change_dart_document(abs_path, Arc::<str>::from(text), version);
                        }
                        rooted_language::RootedLanguage::Rust => {
                            self.change_rust_document(abs_path, Arc::<str>::from(text), version);
                        }
                    }
                }
            }
        }
    }

    pub fn request_hover(&mut self, path: &PathBuf, ext: &str, line: u32, col: u32) -> Option<i32> {
        let abs_path = self.lookup_abs_path(path);
        self.ide_process_for_document(&abs_path, ext)
            .and_then(|process| process.request_hover(&abs_path, line, col))
    }

    pub fn request_definition(
        &mut self,
        path: &PathBuf,
        ext: &str,
        line: u32,
        col: u32,
    ) -> Option<i32> {
        let abs_path = self.lookup_abs_path(path);
        self.ide_process_for_document(&abs_path, ext)
            .and_then(|process| process.request_definition(&abs_path, line, col))
    }

    pub fn request_references(
        &mut self,
        path: &PathBuf,
        ext: &str,
        line: u32,
        col: u32,
        include_declaration: bool,
    ) -> Option<i32> {
        let abs_path = self.lookup_abs_path(path);
        self.ide_process_for_document(&abs_path, ext)
            .and_then(|process| {
                process.request_references(&abs_path, line, col, include_declaration)
            })
    }

    pub fn request_prepare_rename(
        &mut self,
        path: &PathBuf,
        ext: &str,
        line: u32,
        col: u32,
    ) -> Option<i32> {
        let abs_path = self.lookup_abs_path(path);
        self.ide_process_for_document(&abs_path, ext)
            .and_then(|process| process.request_prepare_rename(&abs_path, line, col))
    }

    pub fn request_rename(
        &mut self,
        path: &PathBuf,
        ext: &str,
        line: u32,
        col: u32,
        new_name: &str,
    ) -> Option<i32> {
        let abs_path = self.lookup_abs_path(path);
        self.ide_process_for_document(&abs_path, ext)
            .and_then(|process| process.request_rename(&abs_path, line, col, new_name))
    }

    pub fn request_completion(
        &mut self,
        path: &PathBuf,
        ext: &str,
        line: u32,
        col: u32,
        trigger: Option<&str>,
    ) -> Option<i32> {
        let abs_path = self.lookup_abs_path(path);
        self.ide_process_for_document(&abs_path, ext)
            .and_then(|process| process.request_completion(&abs_path, line, col, trigger))
    }

    pub fn request_signature_help(
        &mut self,
        path: &PathBuf,
        ext: &str,
        line: u32,
        col: u32,
        trigger: Option<&str>,
    ) -> Option<i32> {
        let abs_path = self.lookup_abs_path(path);
        self.ide_process_for_document(&abs_path, ext)
            .and_then(|process| process.request_signature_help(&abs_path, line, col, trigger))
    }

    pub fn request_inlay_hints(
        &mut self,
        path: &PathBuf,
        ext: &str,
        start_line: u32,
        start_col: u32,
        end_line: u32,
        end_col: u32,
    ) -> Option<i32> {
        let abs_path = self.lookup_abs_path(path);
        self.ide_process_for_document(&abs_path, ext)
            .and_then(|process| {
                process.request_inlay_hints(&abs_path, start_line, start_col, end_line, end_col)
            })
    }

    pub fn request_formatting(
        &mut self,
        path: &PathBuf,
        ext: &str,
        tab_size: u32,
        insert_spaces: bool,
    ) -> Option<i32> {
        let abs_path = self.lookup_abs_path(path);
        self.ide_process_for_document(&abs_path, ext)
            .and_then(|process| process.request_formatting(&abs_path, tab_size, insert_spaces))
    }

    pub fn request_ide_completion(
        &mut self,
        path: &PathBuf,
        ext: &str,
        line: u32,
        col: u32,
        trigger: Option<&str>,
    ) -> Option<i32> {
        Self::language_for_ext(ext)
            .is_some()
            .then(|| self.request_completion(path, ext, line, col, trigger))
            .flatten()
    }

    pub fn request_ide_signature_help(
        &mut self,
        path: &PathBuf,
        ext: &str,
        line: u32,
        col: u32,
        trigger: Option<&str>,
    ) -> Option<i32> {
        Self::language_for_ext(ext)
            .is_some()
            .then(|| self.request_signature_help(path, ext, line, col, trigger))
            .flatten()
    }

    pub fn request_ide_inlay_hints(
        &mut self,
        path: &PathBuf,
        ext: &str,
        start_line: u32,
        start_col: u32,
        end_line: u32,
        end_col: u32,
    ) -> Option<i32> {
        Self::language_for_ext(ext).is_some().then(|| {
            self.request_inlay_hints(path, ext, start_line, start_col, end_line, end_col)
        }).flatten()
    }

    /// Уведомляет LSP о закрытии файла
    pub fn notify_close(&mut self, path: &PathBuf, ext: &str) {
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        let Some(route) = Self::language_for_ext(ext) else {
            return;
        };
        match route {
            LangRoute::Python => {
            if self.current_path.as_ref() == Some(&abs_path) {
                self.current_path = None;
            }
            if matches!(self.current_python_file.as_ref(), Some((path, _, _)) if path == &abs_path)
            {
                self.current_python_file = None;
                self.current_python_lines = None;
            }
            if let Some(proc) = &mut self.python {
                proc.notify_close(&abs_path);
            }
            if let Some(proc) = &mut self.ty_process {
                proc.notify_close(&abs_path);
            }
            if self.note_close_python_file(&abs_path) {
                self.sync_python_processes_after_open_set_change(true);
            } else {
                self.prune_inactive_workspace_diagnostics();
                self.ty_workspace_diag_dirty = !self.active_workspaces.is_empty();
                self.ruff_workspace_diag_dirty = self.ty_workspace_diag_dirty;
                self.mark_diagnostics_changed();
            }
            }
            LangRoute::Rooted(lang) => {
                if self.current_path.as_ref() == Some(&abs_path) {
                    self.current_path = None;
                }
                match lang {
                    rooted_language::RootedLanguage::Dart => self.close_dart_document(&abs_path),
                    rooted_language::RootedLanguage::Rust => self.close_rust_document(&abs_path),
                }
            }
        }
    }

    pub fn notify_python_tab_close(&mut self, path: &PathBuf, ext: &str) {
        if matches!(Self::language_for_ext(ext), Some(LangRoute::Python)) {
            self.notify_close(path, ext);
        }
    }










































}

#[cfg(test)]
mod lsp_manager_allocation_tests {
    use super::*;

    #[test]
    fn server_summaries_report_counts_without_cloning_logs() {
        let mut manager = LspManager::new(Vec::new());
        manager.python_status = LspServerStatus::Running;
        manager.server_logs.insert(
            RUFF_SERVER.program,
            vec![LogEntry {
                text: "ruff log".to_string(),
                spans: Vec::new(),
                folds: Vec::new(),
                created_at: Instant::now(),
            }],
        );

        let summaries = manager.server_summaries();

        assert_eq!(summaries[0].name, RUFF_SERVER.program);
        assert_eq!(summaries[0].log_count, 1);
        assert!(std::ptr::eq(summaries[0].status, &manager.python_status));
        assert_eq!(summaries[1].name, TY_SERVER.program);
        assert_eq!(summaries[1].log_count, 0);
        assert!(std::ptr::eq(summaries[1].status, &manager.ty_status));
    }

    #[test]
    fn r2_070_restart_ty_does_not_mutate_ruff_disable_state() {
        let mut manager = LspManager::new(Vec::new());
        manager.ruff_disabled = true;
        manager.ty_disabled = true;
        manager.restart_server(TY_SERVER.program);
        assert!(manager.ruff_disabled);
        assert!(
            manager.ty_disabled,
            "without open files restart must remain a no-op"
        );
        let source = include_str!("lsp_manager.rs");
        assert!(source.contains("name if name == TY_SERVER.program"));
    }

    #[test]
    fn r2_071_toggling_ty_does_not_toggle_ruff() {
        let mut manager = LspManager::new(Vec::new());
        manager.ruff_disabled = false;
        manager.ty_disabled = false;
        manager.python_status = LspServerStatus::Running;
        manager.ty_status = LspServerStatus::Running;
        manager.set_server_enabled(TY_SERVER.program, false);
        assert!(!manager.ruff_disabled);
        assert!(manager.ty_disabled);
        assert_eq!(manager.python_status, LspServerStatus::Running);
        assert_eq!(manager.ty_status, LspServerStatus::Disabled);
    }

    #[test]
    fn r2_072_stopping_ty_keeps_ruff_and_panel_server_available() {
        let mut manager = LspManager::new(Vec::new());
        manager.python_status = LspServerStatus::Running;
        manager.ty_status = LspServerStatus::Running;
        manager.stop_server(TY_SERVER.program);
        let summaries = manager.server_summaries();
        assert_eq!(*summaries[0].status, LspServerStatus::Running);
        assert_eq!(*summaries[1].status, LspServerStatus::Disabled);
        assert!(!manager.ruff_disabled);
        assert!(manager.ty_disabled);
    }
}

#[cfg(test)]
mod lsp_tests;
