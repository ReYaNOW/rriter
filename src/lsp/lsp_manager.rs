pub struct LspServerSummary<'a> {
    pub name: &'static str,
    pub status: &'a LspServerStatus,
    pub log_count: usize,
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

#[cfg(target_os = "linux")]
fn trim_allocator_after_large_diagnostics(count: usize, workspace_done: bool) {
    if count < 1024 && !workspace_done {
        return;
    }
    unsafe extern "C" {
        fn malloc_trim(pad: usize) -> i32;
    }
    unsafe {
        malloc_trim(0);
    }
}

#[cfg(not(target_os = "linux"))]
fn trim_allocator_after_large_diagnostics(_count: usize, _workspace_done: bool) {}

pub struct LspManager {
    python: Option<LspProcess>,
    ty_process: Option<LspProcess>,
    workspaces: Vec<PathBuf>,
    active_workspaces: Vec<PathBuf>,
    open_python_files: HashMap<crate::platform::PathKey, OpenPythonFile>,
    open_dart_files: HashMap<crate::platform::PathKey, dart_workspace::OpenDartFile>,
    dart_workspaces:
        HashMap<crate::platform::PathKey, dart_workspace::DartWorkspaceState>,
    closed_dart_documents: Vec<PathBuf>,
    /// Актуальные диагностики для каждого открытого файла
    pub diagnostics: HashMap<PathBuf, Arc<[Diagnostic]>>,
    pub instant_diagnostics: HashMap<PathBuf, (i32, Arc<[Diagnostic]>)>,
    ruff_workspace_diagnostics: HashMap<PathBuf, Arc<[Diagnostic]>>,
    pub ty_instant_diagnostics: HashMap<PathBuf, (i32, Arc<[Diagnostic]>)>,
    dart_live_diagnostics: HashMap<PathBuf, (i32, Arc<[Diagnostic]>)>,
    dart_workspace_diagnostics: HashMap<PathBuf, Arc<[Diagnostic]>>,
    merged_diagnostic_indices: HashMap<PathBuf, Arc<[MergedDiagnosticIndex]>>,
    diagnostic_ancestor_severities: HashMap<PathBuf, DiagSeverity>,
    diagnostic_total_counts: (usize, usize),
    ty_diag_result_ids: HashMap<PathBuf, String>,
    diag_text_pool: HashMap<Arc<str>, Arc<str>>,
    ruff_workspace_diag_rx: Option<std::sync::mpsc::Receiver<ruff_workspace::RuffWorkspaceResult>>,
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
    pub dart_status: LspServerStatus,
    /// Отключены ли Python-серверы вручную целиком.
    pub python_disabled: bool,
    ruff_disabled: bool,
    ty_disabled: bool,
    ruff_unavailable: bool,
    ty_unavailable: bool,
    dart_disabled: bool,
    dart_unavailable: bool,
    dart_workspace_analysis_enabled: bool,
    pub server_logs: HashMap<&'static str, Vec<LogEntry>>,
    pub suppress_diagnostics: bool,
}

impl LspManager {
    fn is_python_ext(ext: &str) -> bool {
        matches!(ext, "py" | "pyi")
    }

    pub fn new(workspaces: Vec<PathBuf>) -> Self {
        let mut manager = LspManager {
            python: None,
            ty_process: None,
            workspaces: crate::platform::dedup_paths(workspaces),
            active_workspaces: Vec::new(),
            open_python_files: HashMap::new(),
            open_dart_files: HashMap::new(),
            dart_workspaces: HashMap::new(),
            closed_dart_documents: Vec::new(),
            diagnostics: HashMap::new(),
            instant_diagnostics: HashMap::new(),
            ruff_workspace_diagnostics: HashMap::new(),
            ty_instant_diagnostics: HashMap::new(),
            dart_live_diagnostics: HashMap::new(),
            dart_workspace_diagnostics: HashMap::new(),
            merged_diagnostic_indices: HashMap::new(),
            diagnostic_ancestor_severities: HashMap::new(),
            diagnostic_total_counts: (0, 0),
            ty_diag_result_ids: HashMap::new(),
            diag_text_pool: HashMap::new(),
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
            dart_status: LspServerStatus::Disabled,
            python_disabled: false,
            ruff_disabled: false,
            ty_disabled: false,
            ruff_unavailable: false,
            ty_unavailable: false,
            dart_disabled: false,
            dart_unavailable: false,
            dart_workspace_analysis_enabled: true,
            server_logs: HashMap::new(),
            suppress_diagnostics: false,
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

    fn prune_inactive_workspace_diagnostics(&mut self) {
        let active_workspaces = self.active_workspaces.clone();
        let open_python_files = self
            .open_python_files
            .keys()
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        let open_dart_files = self
            .open_dart_files
            .keys()
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        let dart_roots = self
            .dart_workspaces
            .values()
            .map(|state| state.root.clone())
            .collect::<Vec<_>>();
        let keep_path = |path: &PathBuf| {
            active_workspaces
                .iter()
                .any(|ws| crate::platform::path_is_within(path, ws))
                || open_python_files.contains(&crate::platform::PathKey::new(path))
                || open_dart_files.contains(&crate::platform::PathKey::new(path))
                || dart_roots
                    .iter()
                    .any(|root| crate::platform::path_is_within(path, root))
        };
        let before = self.diagnostics.len()
            + self.instant_diagnostics.len()
            + self.ruff_workspace_diagnostics.len()
            + self.ty_instant_diagnostics.len()
            + self.dart_live_diagnostics.len()
            + self.dart_workspace_diagnostics.len()
            + self.merged_diagnostic_indices.len()
            + self.ty_diag_result_ids.len();
        self.diagnostics.retain(|path, _| keep_path(path));
        self.instant_diagnostics.retain(|path, _| keep_path(path));
        self.ruff_workspace_diagnostics
            .retain(|path, _| keep_path(path));
        self.ty_instant_diagnostics.retain(|path, _| keep_path(path));
        self.dart_live_diagnostics.retain(|path, _| keep_path(path));
        self.dart_workspace_diagnostics
            .retain(|path, _| keep_path(path));
        self.merged_diagnostic_indices.retain(|path, _| keep_path(path));
        self.ty_diag_result_ids.retain(|path, _| keep_path(path));
        self.rebuild_diag_text_pool();
        let after = self.diagnostics.len()
            + self.instant_diagnostics.len()
            + self.ruff_workspace_diagnostics.len()
            + self.ty_instant_diagnostics.len()
            + self.dart_live_diagnostics.len()
            + self.dart_workspace_diagnostics.len()
            + self.merged_diagnostic_indices.len()
            + self.ty_diag_result_ids.len();
        if before != after {
            self.rebuild_merged_diagnostic_indices();
            self.dirty_diagnostics = false;
        }
    }

    fn reset_ty_workspace_state(&mut self) {
        self.ty_diag_result_ids.clear();
        self.merged_diagnostic_indices.clear();
        self.diagnostic_ancestor_severities.clear();
        self.diagnostic_total_counts = (0, 0);
        self.diag_text_pool.clear();
        self.dirty_diagnostics = true;
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
        self.python = Some(LspProcess::start(&RUFF_SERVER, workspaces.to_vec()));
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
        self.ty_process = Some(LspProcess::start(&TY_SERVER, workspaces.to_vec()));
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
        if self.open_dart_files.is_empty() {
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
        self.diagnostics.clear();
        self.instant_diagnostics.clear();
        self.ruff_workspace_diagnostics.clear();
        self.ty_instant_diagnostics.clear();
        self.ty_diag_result_ids.clear();
        self.merged_diagnostic_indices.clear();
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
        if name == DART_SERVER.program {
            self.dart_disabled = false;
            self.dart_unavailable = false;
            self.dart_status = LspServerStatus::Disabled;
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
        if name == DART_SERVER.program {
            self.dart_disabled = !enabled;
            if enabled {
                self.dart_unavailable = false;
                self.dart_status = LspServerStatus::Disabled;
                self.reconfigure_dart_workspaces();
            } else {
                for state in self.dart_workspaces.values_mut() {
                    state.cancel_job();
                    if let Some(process) = state.process.take() {
                        process.shutdown();
                    }
                }
                self.dart_status = LspServerStatus::Disabled;
                self.dart_live_diagnostics.clear();
                self.dart_workspace_diagnostics.clear();
                self.dirty_diagnostics = true;
            }
            self.rebuild_merged_diagnostic_indices();
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
                    self.ty_instant_diagnostics.clear();
                    self.ty_diag_result_ids.clear();
                    self.ty_workspace_diag_pending = None;
                    self.ty_workspace_diag_dirty = false;
                }
            }
            _ => return,
        }
        self.rebuild_merged_diagnostic_indices();
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
    pub fn server_summaries(&self) -> [LspServerSummary<'_>; 3] {
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
                status: &self.dart_status,
                log_count: self
                    .server_logs
                    .get(dart_workspace::DART_SERVER_NAME)
                    .map_or(0, Vec::len),
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
                status: self.dart_status.clone(),
                logs: dart_logs,
            },
        ]
    }

    pub fn clear_server_logs(&mut self, name: &str) {
        self.server_logs.remove(name);
    }

    fn ide_process_for_document(&mut self, path: &Path, ext: &str) -> Option<&mut LspProcess> {
        match ext {
            "py" | "pyi" => {
                self.ensure_python();
                self.ty_process.as_mut()
            }
            "dart" => self.dart_process_for_path_mut(path),
            _ => None,
        }
    }

    fn action_process_for_document(&mut self, path: &Path, ext: &str) -> Option<&mut LspProcess> {
        match ext {
            "py" | "pyi" => {
                self.ensure_python();
                self.python.as_mut()
            }
            "dart" => self.dart_process_for_path_mut(path),
            _ => None,
        }
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
        if Self::is_python_ext(ext) {
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
            self.dirty_diagnostics = true;
        } else if ext == "dart" {
            self.current_python_file = None;
            self.current_python_lines = None;
            self.open_dart_document(abs_path, Arc::<str>::from(text), version);
        } else {
            self.current_python_file = None;
            self.current_python_lines = None;
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
        if Self::is_python_ext(ext) {
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
            self.dirty_diagnostics = true;
        } else if ext == "dart"
            && self
                .open_dart_files
                .contains_key(&crate::platform::PathKey::new(&abs_path))
        {
            self.current_path = Some(abs_path.clone());
            self.current_python_file = None;
            self.current_python_lines = None;
            self.change_dart_document(abs_path, Arc::<str>::from(text), version);
        }
    }

    pub fn request_hover(
        &mut self,
        path: &PathBuf,
        ext: &str,
        line: u32,
        col: u32,
    ) -> Option<i32> {
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
        self.ide_process_for_document(&abs_path, ext).and_then(|process| {
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
        self.ide_process_for_document(&abs_path, ext).and_then(|process| {
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

    pub fn request_ty_completion(
        &mut self,
        path: &PathBuf,
        ext: &str,
        line: u32,
        col: u32,
        trigger: Option<&str>,
    ) -> Option<i32> {
        Self::is_python_ext(ext)
            .then_some(())
            .and_then(|_| self.request_completion(path, ext, line, col, trigger))
    }

    pub fn request_ty_signature_help(
        &mut self,
        path: &PathBuf,
        ext: &str,
        line: u32,
        col: u32,
        trigger: Option<&str>,
    ) -> Option<i32> {
        Self::is_python_ext(ext)
            .then_some(())
            .and_then(|_| self.request_signature_help(path, ext, line, col, trigger))
    }

    pub fn request_ty_inlay_hints(
        &mut self,
        path: &PathBuf,
        ext: &str,
        start_line: u32,
        start_col: u32,
        end_line: u32,
        end_col: u32,
    ) -> Option<i32> {
        Self::is_python_ext(ext).then_some(()).and_then(|_| {
            self.request_inlay_hints(path, ext, start_line, start_col, end_line, end_col)
        })
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
        if Self::is_python_ext(ext) {
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
                self.dirty_diagnostics = true;
            }
        } else if ext == "dart" {
            if self.current_path.as_ref() == Some(&abs_path) {
                self.current_path = None;
            }
            self.close_dart_document(&abs_path);
        }
    }

    fn ty_workspace_result_ids_json(&self) -> String {
        if self.ty_diag_result_ids.is_empty() {
            return String::from("[]");
        }

        let mut items = Vec::with_capacity(self.ty_diag_result_ids.len());
        for (path, value) in &self.ty_diag_result_ids {
            let uri = path_to_uri(path);
            items.push(format!(
                r#"{{"uri":"{}","value":"{}"}}"#,
                json_escape(&uri),
                json_escape(value)
            ));
        }
        format!("[{}]", items.join(","))
    }

    fn request_ty_workspace_diagnostics_if_ready(&mut self) {
        if !self.ty_workspace_diag_dirty
            || self.ty_workspace_diag_pending.is_some()
            || self.python_disabled
            || self.ty_disabled
            || self.suppress_diagnostics
            || self.ty_status != LspServerStatus::Running
            || self.active_workspaces.is_empty()
        {
            return;
        }
        if self
            .last_change
            .is_some_and(|last| last.elapsed().as_secs_f32() < 3.0)
        {
            return;
        }

        let previous_result_ids_json = self.ty_workspace_result_ids_json();
        if let Some(proc) = &mut self.ty_process
            && let Some(id) = proc.request_workspace_diagnostics(previous_result_ids_json)
        {
            self.ty_workspace_diag_pending = Some(id);
            self.ty_workspace_diag_dirty = false;
        }
    }

    /// Запрашивает code actions для позиции/диагностики
    pub fn request_code_actions(
        &mut self,
        path: &PathBuf,
        ext: &str,
        start_line: u32,
        start_col: u32,
        end_line: u32,
        end_col: u32,
        relevant_diags: &[Diagnostic],
        only: Option<Vec<String>>,
    ) -> Option<i32> {
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        let proc = self.action_process_for_document(&abs_path, ext)?;
        proc.request_code_actions(
            &abs_path,
            start_line,
            start_col,
            end_line,
            end_col,
            relevant_diags,
            only,
        )
    }

    fn should_accept_diagnostics_version(
        existing_version: Option<i32>,
        incoming_version: Option<i32>,
        is_open_file: bool,
    ) -> bool {
        if !is_open_file {
            return true;
        }
        match (existing_version, incoming_version) {
            (None, _) => true,
            (Some(_), None) => false,
            (Some(existing), Some(incoming)) => incoming >= existing,
        }
    }

    /// Опрашивает события от всех серверов. Вызывать раз в кадр.
    /// Обновляет self.diagnostics при получении новых диагностик.
    pub fn poll(&mut self) -> Vec<LspEvent> {
        let mut all = Vec::new();

        if let Some(proc) = &self.python {
            proc.poll(&mut all);
        }
        if let Some(proc) = &mut self.ty_process {
            proc.poll(&mut all);
        }
        self.poll_dart_processes(&mut all);

        // Обновляем кешированные диагностики и статусы
        let mut received_diagnostics = 0usize;
        let mut workspace_diagnostics_done = false;
        for ev in &mut all {
            match ev {
                LspEvent::Diagnostics {
                    server,
                    path,
                    version,
                    items,
                    result_id,
                    ..
                } => {
                    if !self.suppress_diagnostics {
                        let is_ty = *server == LspServerKind::Ty;
                        let is_dart = *server == LspServerKind::Dart;
                        let existing_version = if is_ty {
                            self.ty_instant_diagnostics.get(path).map(|(version, _)| *version)
                        } else if is_dart {
                            self.dart_live_diagnostics
                                .get(path)
                                .map(|(version, _)| *version)
                        } else {
                            self.instant_diagnostics.get(path).map(|(version, _)| *version)
                        };
                        let path_key = crate::platform::PathKey::new(path);
                        let is_open_file = if is_dart {
                            self.open_dart_files.contains_key(&path_key)
                        } else {
                            self.open_python_files.contains_key(&path_key)
                        };
                        let current_dart_version = is_dart
                            .then(|| self.dart_document_version(path))
                            .flatten();
                        let version_is_current = if is_dart {
                            is_open_file
                                && current_dart_version.is_some_and(|current| {
                                    version.is_some_and(|incoming| incoming >= current)
                                })
                        } else {
                            true
                        };
                        if version_is_current
                            && Self::should_accept_diagnostics_version(
                                existing_version,
                                *version,
                                is_open_file,
                            )
                        {
                            let stored_version = version.unwrap_or(0);
                            received_diagnostics =
                                received_diagnostics.saturating_add(items.len());
                            if is_dart {
                                for diagnostic in items.iter_mut() {
                                    diagnostic.source =
                                        Some(Arc::<str>::from(dart_workspace::DART_SERVER_NAME));
                                }
                            }
                            self.compact_diagnostic_text(items);

                            if is_ty {
                                if let Some(result_id) = result_id.as_ref() {
                                    self.ty_diag_result_ids
                                        .insert(path.clone(), result_id.clone());
                                }
                                let items = Arc::<[Diagnostic]>::from(std::mem::take(items));
                                self.ty_instant_diagnostics
                                    .insert(path.clone(), (stored_version, items));
                            } else if is_dart {
                                let items = Arc::<[Diagnostic]>::from(std::mem::take(items));
                                self.dart_live_diagnostics
                                    .insert(path.clone(), (stored_version, items));
                            } else {
                                let items = Arc::<[Diagnostic]>::from(std::mem::take(items));
                                self.instant_diagnostics
                                    .insert(path.clone(), (stored_version, items));
                            }

                            self.dirty_diagnostics = true;
                            self.last_change = None;
                        } else {
                            items.clear();
                        }
                    }
                }
                LspEvent::StatusChanged { server, status } => {
                    if *server == LspServerKind::Dart {
                        self.dart_status = status.clone();
                        if *status == LspServerStatus::Running {
                            self.dart_unavailable = false;
                        } else if *status == LspServerStatus::Missing {
                            self.mark_dart_missing();
                        }
                        continue;
                    }
                    if *server == LspServerKind::Ty {
                        self.ty_status = status.clone();
                        if *status == LspServerStatus::Running {
                            self.ty_unavailable = false;
                            self.ty_workspace_diag_dirty = true;
                        } else if *status == LspServerStatus::Starting
                            || *status == LspServerStatus::Crashed
                            || *status == LspServerStatus::Missing
                            || *status == LspServerStatus::Disabled
                        {
                            self.ty_workspace_diag_pending = None;
                            if matches!(
                                status,
                                LspServerStatus::Disabled | LspServerStatus::Missing
                            ) {
                                self.ty_unavailable = true;
                                self.ty_process = None;
                            }
                        }
                    } else {
                        self.python_status = status.clone();
                        if *status == LspServerStatus::Running {
                            self.ruff_unavailable = false;
                            self.ruff_workspace_diag_dirty = true;
                        } else if matches!(
                            status,
                            LspServerStatus::Disabled | LspServerStatus::Missing
                        ) {
                            self.ruff_unavailable = true;
                            self.python = None;
                            self.ruff_workspace_diag_rx = None;
                            self.ruff_workspace_diag_pending = false;
                            self.ruff_workspace_diag_dirty = false;
                        }
                    }
                }
                LspEvent::ConfigurationServed { server } => {
                    if *server == LspServerKind::Ty {
                        self.ty_workspace_diag_dirty = true;
                    }
                }
                LspEvent::WorkspaceDiagnosticsDone { request_id } => {
                    if self.ty_workspace_diag_pending == Some(*request_id) {
                        self.ty_workspace_diag_pending = None;
                    }
                    workspace_diagnostics_done = true;
                }
                LspEvent::Log { name, message } => {
                    let (final_text, spans, folds) = format_lsp_log_entry(message);
                    *message = final_text.clone();
                    let logs = self.server_logs.entry(*name).or_insert_with(Vec::new);
                    let now = Instant::now();
                    logs.push(LogEntry {
                        text: final_text,
                        spans,
                        folds,
                        created_at: now,
                    });
                    trim_lsp_logs(logs, now);
                }
                _ => {}
            }
        }

        let ruff_workspace_received = self.poll_ruff_workspace_diagnostics();
        if ruff_workspace_received > 0 {
            received_diagnostics = received_diagnostics.saturating_add(ruff_workspace_received);
            workspace_diagnostics_done = true;
        }
        let dart_workspace_received = self.poll_dart_workspace_diagnostics();
        if dart_workspace_received > 0 {
            received_diagnostics = received_diagnostics.saturating_add(dart_workspace_received);
            workspace_diagnostics_done = true;
        }

        if let Some(t) = self.last_change {
            if t.elapsed().as_secs_f32() >= 3.0 {
                if self.dirty_diagnostics {
                    self.rebuild_merged_diagnostic_indices();
                    self.dirty_diagnostics = false;
                }
                self.last_change = None;
            }
        } else {
            if self.dirty_diagnostics {
                self.rebuild_merged_diagnostic_indices();
                self.dirty_diagnostics = false;
            }
        }

        self.request_ty_workspace_diagnostics_if_ready();
        self.request_ruff_workspace_diagnostics_if_ready();
        trim_allocator_after_large_diagnostics(received_diagnostics, workspace_diagnostics_done);

        all
    }

    pub fn get_diagnostics(&self, path: &Path) -> &[Diagnostic] {
        if path.is_absolute() {
            self.diagnostics
                .get(path)
                .map(|v| v.as_ref())
                .unwrap_or(&[])
        } else if let Some(ws) = self.workspaces.first() {
            let abs_path = ws.join(path);
            self.diagnostics
                .get(abs_path.as_path())
                .map(|v| v.as_ref())
                .unwrap_or(&[])
        } else {
            let abs_path = self.relative_lookup_path(path);
            self.diagnostics
                .get(abs_path.as_path())
                .map(|v| v.as_ref())
                .unwrap_or(&[])
        }
    }

    pub fn clear_diagnostics_for_path(&mut self, path: &PathBuf) {
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        self.diagnostics.remove(&abs_path);
        self.instant_diagnostics.remove(&abs_path);
        self.ruff_workspace_diagnostics.remove(&abs_path);
        self.ty_instant_diagnostics.remove(&abs_path);
        self.dart_live_diagnostics.remove(&abs_path);
        self.dart_workspace_diagnostics.remove(&abs_path);
        self.merged_diagnostic_indices.remove(&abs_path);
        self.ty_diag_result_ids.remove(&abs_path);
        self.rebuild_diag_text_pool();
        self.rebuild_merged_diagnostic_indices();
        self.dirty_diagnostics = false;
    }

    fn compact_diagnostic_text(&mut self, items: &mut [Diagnostic]) {
        for diag in items {
            diag.code = self.intern_optional_diag_text(diag.code.take());
            diag.code_href = self.intern_optional_diag_text(diag.code_href.take());
            diag.source = self.intern_optional_diag_text(diag.source.take());
        }
    }

    fn intern_optional_diag_text(&mut self, value: Option<Arc<str>>) -> Option<Arc<str>> {
        let value = value?;
        if let Some((stored, _)) = self.diag_text_pool.get_key_value(value.as_ref()) {
            return Some(stored.clone());
        }
        self.diag_text_pool.insert(value.clone(), value.clone());
        Some(value)
    }

    fn rebuild_diag_text_pool(&mut self) {
        self.diag_text_pool.clear();
        let mut values = Vec::new();
        for (_, diags) in self.instant_diagnostics.values() {
            for diag in diags.iter() {
                if let Some(value) = &diag.code {
                    values.push(value.clone());
                }
                if let Some(value) = &diag.code_href {
                    values.push(value.clone());
                }
                if let Some(value) = &diag.source {
                    values.push(value.clone());
                }
            }
        }
        for (_, diags) in self.ty_instant_diagnostics.values() {
            for diag in diags.iter() {
                if let Some(value) = &diag.code {
                    values.push(value.clone());
                }
                if let Some(value) = &diag.code_href {
                    values.push(value.clone());
                }
                if let Some(value) = &diag.source {
                    values.push(value.clone());
                }
            }
        }
        for diags in self.ruff_workspace_diagnostics.values() {
            for diag in diags.iter() {
                if let Some(value) = &diag.code {
                    values.push(value.clone());
                }
                if let Some(value) = &diag.code_href {
                    values.push(value.clone());
                }
                if let Some(value) = &diag.source {
                    values.push(value.clone());
                }
            }
        }
        for (_, diags) in self.dart_live_diagnostics.values() {
            for diag in diags.iter() {
                if let Some(value) = &diag.code {
                    values.push(value.clone());
                }
                if let Some(value) = &diag.code_href {
                    values.push(value.clone());
                }
                if let Some(value) = &diag.source {
                    values.push(value.clone());
                }
            }
        }
        for diags in self.dart_workspace_diagnostics.values() {
            for diag in diags.iter() {
                if let Some(value) = &diag.code {
                    values.push(value.clone());
                }
                if let Some(value) = &diag.code_href {
                    values.push(value.clone());
                }
                if let Some(value) = &diag.source {
                    values.push(value.clone());
                }
            }
        }
        for value in values {
            let _ = self.intern_optional_diag_text(Some(value));
        }
    }

    fn lookup_abs_path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            self.relative_lookup_path(path)
        }
    }

    fn ruff_workspace_diagnostics_for_abs_path(&self, path: &Path) -> Option<&Arc<[Diagnostic]>> {
        if self
            .open_python_files
            .contains_key(&crate::platform::PathKey::new(path))
            || self.instant_diagnostics.contains_key(path)
        {
            return None;
        }
        self.ruff_workspace_diagnostics.get(path)
    }

    fn ruff_diagnostic_len_for_abs_path(&self, path: &Path) -> usize {
        if let Some((_, diagnostics)) = self.instant_diagnostics.get(path) {
            return diagnostics.len();
        }
        self.ruff_workspace_diagnostics_for_abs_path(path)
            .map_or(0, |diagnostics| diagnostics.len())
    }

    fn ruff_diagnostic_at_for_abs_path(&self, path: &Path, index: usize) -> Option<&Diagnostic> {
        if let Some((_, diagnostics)) = self.instant_diagnostics.get(path) {
            return diagnostics.get(index);
        }
        self.ruff_workspace_diagnostics_for_abs_path(path)
            .and_then(|diagnostics| diagnostics.get(index))
    }

    fn dart_workspace_diagnostics_for_abs_path(&self, path: &Path) -> Option<&Arc<[Diagnostic]>> {
        if self
            .open_dart_files
            .contains_key(&crate::platform::PathKey::new(path))
            || self.dart_live_diagnostics.contains_key(path)
        {
            return None;
        }
        self.dart_workspace_diagnostics.get(path)
    }

    fn dart_diagnostic_len_for_abs_path(&self, path: &Path) -> usize {
        if let Some((_, diagnostics)) = self.dart_live_diagnostics.get(path) {
            return diagnostics.len();
        }
        self.dart_workspace_diagnostics_for_abs_path(path)
            .map_or(0, |diagnostics| diagnostics.len())
    }

    fn dart_diagnostic_at_for_abs_path(&self, path: &Path, index: usize) -> Option<&Diagnostic> {
        if let Some((_, diagnostics)) = self.dart_live_diagnostics.get(path) {
            return diagnostics.get(index);
        }
        self.dart_workspace_diagnostics_for_abs_path(path)
            .and_then(|diagnostics| diagnostics.get(index))
    }

    fn rebuild_merged_diagnostic_indices(&mut self) {
        let mut paths = std::collections::HashSet::new();
        for path in self.diagnostics.keys() {
            paths.insert(path.clone());
        }
        for path in self.instant_diagnostics.keys() {
            paths.insert(path.clone());
        }
        for path in self.ruff_workspace_diagnostics.keys() {
            paths.insert(path.clone());
        }
        for path in self.ty_instant_diagnostics.keys() {
            paths.insert(path.clone());
        }
        for path in self.dart_live_diagnostics.keys() {
            paths.insert(path.clone());
        }
        for path in self.dart_workspace_diagnostics.keys() {
            paths.insert(path.clone());
        }

        self.merged_diagnostic_indices.clear();
        for path in paths {
            let ruff_len = self.ruff_diagnostic_len_for_abs_path(&path);
            let ty_len = self
                .ty_instant_diagnostics
                .get(&path)
                .map_or(0, |(_, diagnostics)| diagnostics.len());
            let dart_len = self.dart_diagnostic_len_for_abs_path(&path);
            let mut indices = Vec::with_capacity(ruff_len + ty_len + dart_len);
            for index in 0..ruff_len {
                indices.push(MergedDiagnosticIndex {
                    source: DiagnosticSourceKind::Ruff,
                    index,
                });
            }
            for index in 0..ty_len {
                indices.push(MergedDiagnosticIndex {
                    source: DiagnosticSourceKind::Ty,
                    index,
                });
            }
            for index in 0..dart_len {
                indices.push(MergedDiagnosticIndex {
                    source: DiagnosticSourceKind::Dart,
                    index,
                });
            }
            if indices.is_empty()
                && let Some(diagnostics) = self.diagnostics.get(&path)
            {
                indices.reserve(diagnostics.len());
                for index in 0..diagnostics.len() {
                    indices.push(MergedDiagnosticIndex {
                        source: DiagnosticSourceKind::Legacy,
                        index,
                    });
                }
            }
            if !indices.is_empty() {
                self.merged_diagnostic_indices
                    .insert(path, Arc::from(indices.into_boxed_slice()));
            }
        }

        let mut ancestor_severities = HashMap::new();
        let mut total_counts = (0usize, 0usize);
        for (path, indices) in &self.merged_diagnostic_indices {
            let mut summary = None;
            for index in indices.iter() {
                if let Some(diagnostic) = self.diagnostic_by_index(path, *index) {
                    match diagnostic.severity {
                        DiagSeverity::Error => total_counts.0 += 1,
                        DiagSeverity::Warning => total_counts.1 += 1,
                        _ => {}
                    }
                    Self::update_severity(&mut summary, diagnostic);
                }
            }
            let Some(severity) = summary else {
                continue;
            };
            if let Some(parent) = path.parent() {
                for ancestor in parent.ancestors() {
                    let entry = ancestor_severities
                        .entry(ancestor.to_path_buf())
                        .or_insert(severity);
                    if severity == DiagSeverity::Error {
                        *entry = DiagSeverity::Error;
                    }
                }
            }
        }
        self.diagnostic_ancestor_severities = ancestor_severities;
        self.diagnostic_total_counts = total_counts;
    }

    fn diagnostic_by_index<'a>(
        &'a self,
        path: &Path,
        index: MergedDiagnosticIndex,
    ) -> Option<&'a Diagnostic> {
        match index.source {
            DiagnosticSourceKind::Legacy => self.diagnostics.get(path)?.get(index.index),
            DiagnosticSourceKind::Ruff => self.ruff_diagnostic_at_for_abs_path(path, index.index),
            DiagnosticSourceKind::Ty => self.ty_instant_diagnostics.get(path)?.1.get(index.index),
            DiagnosticSourceKind::Dart => {
                self.dart_diagnostic_at_for_abs_path(path, index.index)
            }
        }
    }

    fn instant_diagnostic_count_for_abs_path(&self, path: &Path) -> usize {
        self.ruff_diagnostic_len_for_abs_path(path)
            + self
                .ty_instant_diagnostics
                .get(path)
                .map_or(0, |(_, diagnostics)| diagnostics.len())
            + self.dart_diagnostic_len_for_abs_path(path)
    }

    fn instant_diagnostic_at_for_abs_path(&self, path: &Path, index: usize) -> Option<&Diagnostic> {
        let ruff_len = self.ruff_diagnostic_len_for_abs_path(path);
        if index < ruff_len {
            return self.ruff_diagnostic_at_for_abs_path(path, index);
        }
        let ty_len = self
            .ty_instant_diagnostics
            .get(path)
            .map_or(0, |(_, diagnostics)| diagnostics.len());
        if index < ruff_len + ty_len {
            return self
                .ty_instant_diagnostics
                .get(path)
                .and_then(|(_, diagnostics)| diagnostics.get(index - ruff_len));
        }
        self.dart_diagnostic_at_for_abs_path(path, index - ruff_len - ty_len)
    }

    pub fn diagnostic_at(&self, path: &Path, index: usize) -> Option<&Diagnostic> {
        let abs_path = self.lookup_abs_path(path);
        if !self.dirty_diagnostics
            && let Some(indices) = self.merged_diagnostic_indices.get(&abs_path)
        {
            return indices
                .get(index)
                .and_then(|merged| self.diagnostic_by_index(&abs_path, *merged));
        }
        if let Some(diagnostic) = self.instant_diagnostic_at_for_abs_path(&abs_path, index) {
            return Some(diagnostic);
        }
        self.diagnostics
            .get(abs_path.as_path())
            .and_then(|diagnostics| diagnostics.get(index))
    }

    pub fn diagnostic_count(&self, path: &Path) -> usize {
        let abs_path = self.lookup_abs_path(path);
        if !self.dirty_diagnostics
            && let Some(indices) = self.merged_diagnostic_indices.get(&abs_path)
        {
            return indices.len();
        }
        let instant_count = self.instant_diagnostic_count_for_abs_path(&abs_path);
        if instant_count > 0 {
            return instant_count;
        }
        self.diagnostics
            .get(abs_path.as_path())
            .map_or(0, |diagnostics| diagnostics.len())
    }

    pub fn diagnostic_entries_for_path(&self, path: &Path) -> Vec<(usize, &Diagnostic)> {
        let abs_path = self.lookup_abs_path(path);
        if !self.dirty_diagnostics
            && let Some(indices) = self.merged_diagnostic_indices.get(&abs_path)
        {
            let mut entries = Vec::with_capacity(indices.len());
            for (visible_index, merged) in indices.iter().enumerate() {
                if let Some(diagnostic) = self.diagnostic_by_index(&abs_path, *merged) {
                    entries.push((visible_index, diagnostic));
                }
            }
            return entries;
        }
        let instant_count = self.instant_diagnostic_count_for_abs_path(&abs_path);
        if instant_count > 0 {
            let mut entries = Vec::with_capacity(instant_count);
            for visible_index in 0..instant_count {
                if let Some(diagnostic) =
                    self.instant_diagnostic_at_for_abs_path(&abs_path, visible_index)
                {
                    entries.push((visible_index, diagnostic));
                }
            }
            return entries;
        }
        self.diagnostics
            .get(abs_path.as_path())
            .map(|diagnostics| diagnostics.iter().enumerate().collect())
            .unwrap_or_default()
    }

    fn update_severity(summary: &mut Option<DiagSeverity>, diagnostic: &Diagnostic) -> bool {
        match diagnostic.severity {
            DiagSeverity::Error => {
                *summary = Some(DiagSeverity::Error);
                true
            }
            DiagSeverity::Warning => {
                if summary.is_none() {
                    *summary = Some(DiagSeverity::Warning);
                }
                false
            }
            _ => false,
        }
    }

    fn diagnostic_severity_for_abs_path_direct(&self, path: &Path) -> Option<DiagSeverity> {
        let mut summary = None;
        if !self.dirty_diagnostics
            && let Some(indices) = self.merged_diagnostic_indices.get(path)
        {
            for index in indices.iter() {
                if let Some(diagnostic) = self.diagnostic_by_index(path, *index)
                    && Self::update_severity(&mut summary, diagnostic)
                {
                    return summary;
                }
            }
            return summary;
        }
        if let Some((_, diagnostics)) = self.instant_diagnostics.get(path) {
            for diagnostic in diagnostics.iter() {
                if Self::update_severity(&mut summary, diagnostic) {
                    return summary;
                }
            }
        }
        if let Some(diagnostics) = self.ruff_workspace_diagnostics_for_abs_path(path) {
            for diagnostic in diagnostics.iter() {
                if Self::update_severity(&mut summary, diagnostic) {
                    return summary;
                }
            }
        }
        if let Some((_, diagnostics)) = self.ty_instant_diagnostics.get(path) {
            for diagnostic in diagnostics.iter() {
                if Self::update_severity(&mut summary, diagnostic) {
                    return summary;
                }
            }
        }
        if let Some((_, diagnostics)) = self.dart_live_diagnostics.get(path) {
            for diagnostic in diagnostics.iter() {
                if Self::update_severity(&mut summary, diagnostic) {
                    return summary;
                }
            }
        } else if let Some(diagnostics) = self.dart_workspace_diagnostics_for_abs_path(path) {
            for diagnostic in diagnostics.iter() {
                if Self::update_severity(&mut summary, diagnostic) {
                    return summary;
                }
            }
        }
        if summary.is_some() {
            return summary;
        }
        if let Some(diagnostics) = self.diagnostics.get(path) {
            for diagnostic in diagnostics.iter() {
                if Self::update_severity(&mut summary, diagnostic) {
                    return summary;
                }
            }
        }
        summary
    }

    pub fn diagnostic_paths(&self) -> Vec<&PathBuf> {
        let mut paths: Vec<&PathBuf> = Vec::new();
        for path in self
            .merged_diagnostic_indices
            .keys()
            .chain(self.diagnostics.keys())
            .chain(self.instant_diagnostics.keys())
            .chain(self.ruff_workspace_diagnostics.keys())
            .chain(self.ty_instant_diagnostics.keys())
            .chain(self.dart_live_diagnostics.keys())
            .chain(self.dart_workspace_diagnostics.keys())
        {
            if !paths.iter().any(|existing| existing.as_path() == path.as_path()) {
                paths.push(path);
            }
        }
        paths.sort();
        paths
    }

    pub fn diagnostic_counts_for_path(&self, path: &Path) -> (usize, usize) {
        let mut errors = 0usize;
        let mut warnings = 0usize;
        for (_, diagnostic) in self.diagnostic_entries_for_path(path) {
            match diagnostic.severity {
                DiagSeverity::Error => errors += 1,
                DiagSeverity::Warning => warnings += 1,
                _ => {}
            }
        }
        (errors, warnings)
    }

    pub fn total_diagnostic_counts(&self) -> (usize, usize) {
        self.diagnostic_total_counts
    }

    pub fn ruff_diagnostic_storage_counts(&self) -> (usize, usize) {
        let mut paths = std::collections::HashSet::new();
        let mut count = 0usize;
        for (path, (_, diagnostics)) in &self.instant_diagnostics {
            paths.insert(path);
            count = count.saturating_add(diagnostics.len());
        }
        for (path, diagnostics) in &self.ruff_workspace_diagnostics {
            paths.insert(path);
            count = count.saturating_add(diagnostics.len());
        }
        (paths.len(), count)
    }

    pub fn diagnostic_severity_for_path(&self, path: &Path) -> Option<DiagSeverity> {
        let abs_path = self.lookup_abs_path(path);
        self.diagnostic_severity_for_abs_path_direct(&abs_path)
    }

    pub fn diagnostic_severity_under_path(&self, path: &Path) -> Option<DiagSeverity> {
        let abs_path = self.lookup_abs_path(path);
        if !self.dirty_diagnostics {
            return self
                .diagnostic_ancestor_severities
                .get(&abs_path)
                .copied()
                .or_else(|| self.diagnostic_severity_for_abs_path_direct(&abs_path));
        }
        let mut summary = None;
        for diagnostic_path in self
            .merged_diagnostic_indices
            .keys()
            .chain(self.diagnostics.keys())
            .chain(self.instant_diagnostics.keys())
            .chain(self.ruff_workspace_diagnostics.keys())
            .chain(self.ty_instant_diagnostics.keys())
            .chain(self.dart_live_diagnostics.keys())
            .chain(self.dart_workspace_diagnostics.keys())
        {
            if crate::platform::path_is_within(diagnostic_path, &abs_path)
                && let Some(severity) = self.diagnostic_severity_for_abs_path_direct(diagnostic_path)
            {
                if severity == DiagSeverity::Error {
                    return Some(DiagSeverity::Error);
                }
                summary = Some(DiagSeverity::Warning);
            }
        }
        summary
    }

    pub fn diagnostic_refs_for_path(&self, path: &Path) -> Vec<&Diagnostic> {
        self.diagnostic_entries_for_path(path)
            .into_iter()
            .map(|(_, diagnostic)| diagnostic)
            .collect()
    }

    fn instant_merged_diagnostics_for_abs_path(&self, path: &Path) -> (i32, Vec<&Diagnostic>) {
        let ruff = self.instant_diagnostics.get(path);
        let ruff_workspace = if ruff.is_none() {
            self.ruff_workspace_diagnostics_for_abs_path(path)
        } else {
            None
        };
        let ty = self.ty_instant_diagnostics.get(path);
        let dart = self.dart_live_diagnostics.get(path);
        let dart_workspace = if dart.is_none() {
            self.dart_workspace_diagnostics_for_abs_path(path)
        } else {
            None
        };
        let count = ruff.map_or(0, |(_, diags)| diags.len())
            + ruff_workspace.map_or(0, |diags| diags.len())
            + ty.map_or(0, |(_, diags)| diags.len())
            + dart.map_or(0, |(_, diags)| diags.len())
            + dart_workspace.map_or(0, |diags| diags.len());
        if count == 0 {
            return (0, Vec::new());
        }

        let mut merged = Vec::with_capacity(count);
        let mut max_v = 0;
        if let Some((version, diagnostics)) = ruff {
            max_v = max_v.max(*version);
            merged.extend(diagnostics.iter());
        }
        if let Some(diagnostics) = ruff_workspace {
            merged.extend(diagnostics.iter());
        }
        if let Some((version, diagnostics)) = ty {
            max_v = max_v.max(*version);
            merged.extend(diagnostics.iter());
        }
        if let Some((version, diagnostics)) = dart {
            max_v = max_v.max(*version);
            merged.extend(diagnostics.iter());
        }
        if let Some(diagnostics) = dart_workspace {
            merged.extend(diagnostics.iter());
        }
        (max_v, merged)
    }

    pub fn instant_merged_diagnostics(&self, path: &Path) -> (i32, Vec<&Diagnostic>) {
        if path.is_absolute() {
            self.instant_merged_diagnostics_for_abs_path(path)
        } else if let Some(ws) = self.workspaces.first() {
            let abs_path = ws.join(path);
            self.instant_merged_diagnostics_for_abs_path(abs_path.as_path())
        } else {
            let abs_path = self.relative_lookup_path(path);
            self.instant_merged_diagnostics_for_abs_path(abs_path.as_path())
        }
    }

    pub fn has_stale_instant_diagnostics(&self, path: &Path, editor_version: u64) -> bool {
        if path.is_absolute() {
            let is_stale = |diags: &HashMap<PathBuf, (i32, Arc<[Diagnostic]>)>| {
                diags
                    .get(path)
                    .is_some_and(|(version, _)| (*version as u64) < editor_version)
            };
            is_stale(&self.instant_diagnostics)
                || is_stale(&self.ty_instant_diagnostics)
                || is_stale(&self.dart_live_diagnostics)
        } else if let Some(ws) = self.workspaces.first() {
            let abs_path = ws.join(path);
            let is_stale = |diags: &HashMap<PathBuf, (i32, Arc<[Diagnostic]>)>| {
                diags
                    .get(abs_path.as_path())
                    .is_some_and(|(version, _)| (*version as u64) < editor_version)
            };
            is_stale(&self.instant_diagnostics)
                || is_stale(&self.ty_instant_diagnostics)
                || is_stale(&self.dart_live_diagnostics)
        } else {
            let abs_path = self.relative_lookup_path(path);
            let is_stale = |diags: &HashMap<PathBuf, (i32, Arc<[Diagnostic]>)>| {
                diags
                    .get(abs_path.as_path())
                    .is_some_and(|(version, _)| (*version as u64) < editor_version)
            };
            is_stale(&self.instant_diagnostics)
                || is_stale(&self.ty_instant_diagnostics)
                || is_stale(&self.dart_live_diagnostics)
        }
    }

    /// Диагностики для текущего файла, отфильтрованные по строке
    pub fn diagnostics_for_line(&self, path: &PathBuf, line: u32) -> Vec<&Diagnostic> {
        self.diagnostic_entries_for_path(path)
            .into_iter()
            .map(|(_, diagnostic)| diagnostic)
            .filter(move |d| d.start_line == line)
            .collect()
    }

    fn request_source_action(
        &mut self,
        path: &PathBuf,
        ext: &str,
        action_kind: &str,
    ) -> Option<i32> {
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(workspace) = self.workspaces.first() {
            workspace.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        self.action_process_for_document(&abs_path, ext)?.request_code_actions(
            &abs_path,
            0,
            0,
            u32::MAX,
            0,
            &[],
            Some(vec![action_kind.to_string()]),
        )
    }

    /// Запрос на глобальный fix-all (source.fixAll) для текущего файла
    pub fn request_fix_all(&mut self, path: &PathBuf, ext: &str) -> Option<i32> {
        self.request_source_action(path, ext, "source.fixAll")
    }

    pub fn request_organize_imports(&mut self, path: &PathBuf, ext: &str) -> Option<i32> {
        self.request_source_action(path, ext, "source.organizeImports")
    }

    fn stop_processes(&mut self) {
        self.python_disabled = true;
        self.dart_disabled = true;
        if let Some(p) = self.python.take() {
            p.shutdown();
        }
        if let Some(p) = self.ty_process.take() {
            p.shutdown();
        }
        for state in self.dart_workspaces.values_mut() {
            state.cancel_job();
            if let Some(process) = state.process.take() {
                process.shutdown();
            }
        }
    }

    #[allow(dead_code)]
    pub fn shutdown(mut self) {
        self.stop_processes();
    }
}

impl Drop for LspManager {
    fn drop(&mut self) {
        self.stop_processes();
    }
}

// ── Утилита: конвертация LSP-позиции обратно в байт-offset ───────────────────

/// Конвертирует LSP {line, character} → байтовый offset в тексте.
/// Нужно для применения TextChange к буферу редактора.
pub fn lsp_pos_to_offset(text: &str, line: u32, col: u32) -> usize {
    let mut cur_line = 0u32;
    let mut cur_col = 0u32; // UTF-16 единицы

    for (i, ch) in text.char_indices() {
        if cur_line == line && cur_col >= col {
            return i;
        }
        if ch == '\n' {
            if cur_line == line {
                return i;
            }
            cur_line += 1;
            cur_col = 0;
        } else {
            cur_col += ch.len_utf16() as u32;
        }
    }

    if cur_line == line && cur_col >= col {
        return text.len();
    }

    text.len()
}

/// Применяет WorkspaceEdit к строке текста (для текущего файла).
/// Правки должны быть отсортированы с конца файла к началу, чтобы offset'ы не съехали.

#[allow(dead_code)]
pub fn format_and_highlight_json(
    raw_text: &str,
) -> (
    String,
    Vec<crate::highlighter::ColorSpan>,
    Vec<(usize, usize, usize)>,
) {
    let (prefix, content) = if raw_text.starts_with("[LSP RECV] ") {
        ("[LSP RECV]\n", &raw_text[11..])
    } else if raw_text.starts_with("[LSP SEND] ") {
        ("[LSP SEND]\n", &raw_text[11..])
    } else {
        ("", raw_text)
    };

    let is_json = content.trim().starts_with('{') || content.trim().starts_with('[');
    let pretty = if is_json {
        match serde_json::from_str::<serde_json::Value>(content) {
            Ok(v) => serde_json::to_string_pretty(&v).unwrap_or_else(|_| content.to_string()),
            Err(_) => content.to_string(),
        }
    } else {
        content.to_string()
    };

    let mut parser = tree_sitter::Parser::new();
    let lang = if is_json {
        tree_sitter_json::LANGUAGE.into()
    } else {
        tree_sitter_bash::LANGUAGE.into()
    };
    let _ = parser.set_language(&lang);

    let mut final_string = String::from(prefix);
    let mut spans = vec![crate::highlighter::ColorSpan {
        start: 0,
        end: prefix.len(),
        color: if prefix.contains("RECV") {
            [0.313, 0.980, 0.482, 1.0]
        } else {
            [0.545, 0.913, 0.992, 1.0]
        },
    }];
    let mut folds = Vec::new();

    if is_json {
        let tree = parser.parse(&pretty, None).unwrap();

        if let Some(fold_q) = crate::queries::get_folding_query("json") {
            if let Ok(query) = tree_sitter::Query::new(&lang, fold_q) {
                let mut cursor = tree_sitter::QueryCursor::new();
                let mut matches = cursor.matches(&query, tree.root_node(), pretty.as_bytes());
                while let Some(m) = matches.next() {
                    for cap in m.captures {
                        let node = cap.node;
                        if node.end_position().row > node.start_position().row + 1 {
                            folds.push((
                                node.start_byte() + prefix.len(),
                                node.end_byte() + prefix.len(),
                                json_container_depth(node),
                            ));
                        }
                    }
                }
            }
        }

        if let Some((_, queries)) = crate::queries::get_ts_config("json") {
            for q in queries {
                if let Ok(query) = tree_sitter::Query::new(&lang, q) {
                    let mut cursor = tree_sitter::QueryCursor::new();
                    let mut matches = cursor.matches(&query, tree.root_node(), pretty.as_bytes());
                    while let Some(m) = matches.next() {
                        for cap in m.captures {
                            let name = query.capture_names()[cap.index as usize];
                            let color = match name {
                                "property" => [0.545, 0.913, 0.992, 1.0],
                                "string" => [0.945, 0.980, 0.549, 1.0],
                                "number" => [0.741, 0.576, 0.976, 1.0],
                                "boolean" => [1.0, 0.474, 0.776, 1.0],
                                "keyword.control" => [1.0, 0.474, 0.776, 1.0],
                                "comment" => [0.384, 0.447, 0.643, 1.0],
                                _ => continue,
                            };
                            spans.push(crate::highlighter::ColorSpan {
                                start: cap.node.start_byte() + prefix.len(),
                                end: cap.node.end_byte() + prefix.len(),
                                color,
                            });
                        }
                    }
                }
            }
        }
        final_string.push_str(&pretty);
    } else {
        final_string.push_str(&pretty);
        spans.push(crate::highlighter::ColorSpan {
            start: prefix.len(),
            end: final_string.len(),
            color: [0.875, 0.882, 0.902, 1.0],
        });
    }

    spans.sort_by_key(|s| s.start);
    (final_string, spans, folds)
}

#[allow(dead_code)]
fn json_container_depth(node: tree_sitter::Node<'_>) -> usize {
    let mut depth = 1;
    let mut parent = node.parent();
    while let Some(p) = parent {
        if matches!(p.kind(), "object" | "array") {
            depth += 1;
        }
        parent = p.parent();
    }
    depth
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
        assert!(manager.ty_disabled, "without open files restart must remain a no-op");
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
