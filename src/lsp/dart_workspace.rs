use super::{DART_SERVER, DiagSeverity, Diagnostic, LogEntry, LspManager, LspProcess};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

pub(super) const DART_SERVER_NAME: &str = DART_SERVER.program;

#[derive(Clone, Debug)]
pub(super) struct OpenDartFile {
    pub(super) path: PathBuf,
    pub(super) root: PathBuf,
    pub(super) text: Arc<str>,
    pub(super) version: i32,
}

pub(super) struct DartAnalyzerJob {
    generation: u64,
    cancel: Arc<AtomicBool>,
    rx: mpsc::Receiver<DartWorkspaceResult>,
}

pub(super) struct DartWorkspaceState {
    pub(super) root: PathBuf,
    pub(super) process: Option<LspProcess>,
    generation: u64,
    due_at: Option<Instant>,
    job: Option<DartAnalyzerJob>,
}

impl DartWorkspaceState {
    pub(super) fn new(root: PathBuf) -> Self {
        Self {
            root,
            process: None,
            generation: 0,
            due_at: None,
            job: None,
        }
    }

    pub(super) fn cancel_job(&mut self) {
        if let Some(job) = &self.job {
            job.cancel.store(true, Ordering::Release);
        }
    }
}

pub(super) struct DartWorkspaceResult {
    root: PathBuf,
    generation: u64,
    diagnostics: Result<HashMap<PathBuf, Vec<Diagnostic>>, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DartRoot {
    pub(super) path: PathBuf,
    pub(super) is_flutter: bool,
}

pub(super) fn dart_root_for_path(path: &Path, workspaces: &[PathBuf]) -> DartRoot {
    let file_dir = path.parent().unwrap_or(path);
    let configured = workspaces
        .iter()
        .filter(|workspace| crate::platform::path_is_within(path, workspace))
        .max_by_key(|workspace| workspace.components().count());

    let search_stop = configured.map(PathBuf::as_path);
    let pubspec_root = nearest_marker(file_dir, search_stop, "pubspec.yaml");
    let analysis_root = nearest_marker(file_dir, search_stop, "analysis_options.yaml");
    let root = pubspec_root
        .or(analysis_root)
        .or_else(|| configured.cloned())
        .unwrap_or_else(|| file_dir.to_path_buf());

    DartRoot {
        is_flutter: pubspec_declares_flutter(&root.join("pubspec.yaml")),
        path: root,
    }
}

fn nearest_marker(start: &Path, stop: Option<&Path>, marker: &str) -> Option<PathBuf> {
    for ancestor in start.ancestors() {
        if ancestor.join(marker).is_file() {
            return Some(ancestor.to_path_buf());
        }
        if stop.is_some_and(|stop| crate::platform::paths_equal(ancestor, stop)) {
            break;
        }
    }
    None
}

fn pubspec_declares_flutter(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if metadata.len() > 1024 * 1024 {
        return false;
    }
    let Ok(content) = std::fs::read_to_string(path) else {
        return false;
    };
    content.lines().any(|line| {
        let trimmed = line.trim_start();
        let indent = line.len().saturating_sub(trimmed.len());
        indent > 0 && trimmed.starts_with("flutter:")
    })
}

impl LspManager {
    pub(super) fn open_dart_document(
        &mut self,
        path: PathBuf,
        text: Arc<str>,
        version: i32,
    ) {
        let path_key = crate::platform::PathKey::new(&path);
        if let Some(existing_version) = self
            .open_dart_files
            .get(&path_key)
            .map(|document| document.version)
        {
            if version > existing_version {
                self.change_dart_document(path, text, version);
            }
            return;
        }
        let root = dart_root_for_path(&path, &self.workspaces).path;
        let root_key = crate::platform::PathKey::new(&root);
        self.open_dart_files.insert(
            path_key,
            OpenDartFile {
                path: path.clone(),
                root: root.clone(),
                text: text.clone(),
                version,
            },
        );
        self.dart_workspaces
            .entry(root_key.clone())
            .or_insert_with(|| DartWorkspaceState::new(root.clone()));
        self.ensure_dart_process(&root_key);
        if let Some(state) = self.dart_workspaces.get_mut(&root_key)
            && let Some(process) = &mut state.process
        {
            process.notify_open(&path, text, version, Some(&root));
        }
        self.schedule_dart_workspace_analysis(&root, Duration::from_secs(1));
        self.dirty_diagnostics = true;
    }

    pub(super) fn change_dart_document(
        &mut self,
        path: PathBuf,
        text: Arc<str>,
        version: i32,
    ) {
        let path_key = crate::platform::PathKey::new(&path);
        let Some(existing) = self.open_dart_files.get(&path_key) else {
            return;
        };
        if version <= existing.version {
            return;
        }
        let root = existing.root.clone();
        if let Some(open) = self.open_dart_files.get_mut(&path_key) {
            open.text = text.clone();
            open.version = version;
        }
        let root_key = crate::platform::PathKey::new(&root);
        self.ensure_dart_process(&root_key);
        if let Some(state) = self.dart_workspaces.get_mut(&root_key)
            && let Some(process) = &mut state.process
        {
            process.notify_change(&path, text, version);
        }
        self.dirty_diagnostics = true;
    }

    pub(super) fn close_dart_document(&mut self, path: &Path) {
        let path_key = crate::platform::PathKey::new(path);
        let Some(open) = self.open_dart_files.remove(&path_key) else {
            return;
        };
        let root_key = crate::platform::PathKey::new(&open.root);
        if let Some(state) = self.dart_workspaces.get_mut(&root_key)
            && let Some(process) = &mut state.process
        {
            process.notify_close(&open.path);
        }
        self.dart_live_diagnostics.remove(&open.path);
        self.merged_diagnostic_indices.remove(&open.path);
        self.closed_dart_documents.push(open.path.clone());
        self.dirty_diagnostics = true;

        let root_still_open = self
            .open_dart_files
            .values()
            .any(|document| crate::platform::paths_equal(&document.root, &open.root));
        if !root_still_open
            && let Some(state) = self.dart_workspaces.get_mut(&root_key)
            && state.job.is_none()
        {
            if let Some(process) = state.process.take() {
                process.shutdown();
            }
            self.dart_status = super::LspServerStatus::Disabled;
        }
    }

    pub fn notify_saved(&mut self, path: &Path, ext: &str) {
        let is_analysis_configuration = path.file_name().is_some_and(|name| {
            name == std::ffi::OsStr::new("analysis_options.yaml")
                || name == std::ffi::OsStr::new("pubspec.yaml")
        });
        if is_analysis_configuration {
            self.notify_analysis_configuration_changed(path);
            return;
        }
        if ext != "dart" {
            return;
        }
        let path_key = crate::platform::PathKey::new(path);
        if let Some(open) = self.open_dart_files.get(&path_key) {
            let root = open.root.clone();
            self.schedule_dart_workspace_analysis(&root, Duration::from_millis(250));
        }
    }

    pub fn refresh_workspace_diagnostics(&mut self, path: &Path, ext: &str) {
        if ext != "dart" {
            return;
        }
        let root = self
            .open_dart_files
            .get(&crate::platform::PathKey::new(path))
            .map(|open| open.root.clone())
            .unwrap_or_else(|| dart_root_for_path(path, &self.workspaces).path);
        self.schedule_dart_workspace_analysis(&root, Duration::ZERO);
    }

    pub fn notify_analysis_configuration_changed(&mut self, path: &Path) {
        let root = dart_root_for_path(path, &self.workspaces).path;
        self.schedule_dart_workspace_analysis(&root, Duration::from_millis(250));
    }

    pub fn drain_closed_dart_documents(&mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.closed_dart_documents)
    }

    pub(super) fn dart_document_version(&self, path: &Path) -> Option<i32> {
        self.open_dart_files
            .get(&crate::platform::PathKey::new(path))
            .map(|open| open.version)
    }

    pub(super) fn dart_process_for_path_mut(&mut self, path: &Path) -> Option<&mut LspProcess> {
        let root = self
            .open_dart_files
            .get(&crate::platform::PathKey::new(path))?
            .root
            .clone();
        let root_key = crate::platform::PathKey::new(&root);
        self.ensure_dart_process(&root_key);
        self.dart_workspaces
            .get_mut(&root_key)
            .and_then(|state| state.process.as_mut())
    }

    pub(super) fn poll_dart_processes(&self, events: &mut Vec<super::LspEvent>) {
        for state in self.dart_workspaces.values() {
            if let Some(process) = &state.process {
                process.poll(events);
            }
        }
    }

    pub(super) fn poll_dart_workspace_diagnostics(&mut self) -> usize {
        let keys = self.dart_workspaces.keys().cloned().collect::<Vec<_>>();
        let mut completed = Vec::new();
        for key in &keys {
            let Some(state) = self.dart_workspaces.get_mut(key) else {
                continue;
            };
            let Some(job) = state.job.take() else {
                continue;
            };
            match job.rx.try_recv() {
                Ok(result) => completed.push((key.clone(), result)),
                Err(mpsc::TryRecvError::Empty) => state.job = Some(job),
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.log_dart_workspace_error(
                        "Dart workspace diagnostics worker disconnected".to_string(),
                    );
                }
            }
        }

        let mut received = 0usize;
        for (key, result) in completed {
            received = received.saturating_add(self.apply_dart_workspace_result(result));
            let root_still_open = self.open_dart_files.values().any(|document| {
                self.dart_workspaces
                    .get(&key)
                    .is_some_and(|state| crate::platform::paths_equal(&document.root, &state.root))
            });
            if !root_still_open
                && let Some(state) = self.dart_workspaces.get_mut(&key)
                && let Some(process) = state.process.take()
            {
                process.shutdown();
            }
        }
        self.start_due_dart_workspace_jobs();
        received
    }

    pub(super) fn mark_dart_missing(&mut self) {
        self.dart_unavailable = true;
        for state in self.dart_workspaces.values_mut() {
            state.cancel_job();
            if let Some(process) = state.process.take() {
                process.shutdown();
            }
        }
    }

    pub(super) fn reconfigure_dart_workspaces(&mut self) {
        if self.open_dart_files.is_empty() {
            for state in self.dart_workspaces.values_mut() {
                state.cancel_job();
                if let Some(process) = state.process.take() {
                    process.shutdown();
                }
            }
            self.dart_workspaces.clear();
            self.dart_live_diagnostics.clear();
            self.dart_workspace_diagnostics.clear();
            self.dirty_diagnostics = true;
            return;
        }
        let documents = self.open_dart_files.values().cloned().collect::<Vec<_>>();
        for state in self.dart_workspaces.values_mut() {
            state.cancel_job();
            if let Some(process) = state.process.take() {
                process.shutdown();
            }
        }
        self.dart_workspaces.clear();
        self.open_dart_files.clear();
        self.dart_live_diagnostics.clear();
        self.dart_workspace_diagnostics.clear();
        for document in documents {
            self.open_dart_document(document.path, document.text, document.version);
        }
        self.dirty_diagnostics = true;
    }

    pub fn set_dart_workspace_analysis_enabled(&mut self, enabled: bool) {
        if self.dart_workspace_analysis_enabled == enabled {
            return;
        }
        self.dart_workspace_analysis_enabled = enabled;
        if !enabled {
            for state in self.dart_workspaces.values_mut() {
                state.cancel_job();
                state.generation = state.generation.wrapping_add(1).max(1);
                state.due_at = None;
            }
            self.dart_workspace_diagnostics.clear();
            self.dirty_diagnostics = true;
            return;
        }
        if self.open_dart_files.is_empty() {
            self.schedule_configured_dart_projects();
        } else {
            let roots = self
                .open_dart_files
                .values()
                .map(|document| document.root.clone())
                .collect::<Vec<_>>();
            for root in crate::platform::dedup_paths(roots) {
                self.schedule_dart_workspace_analysis(&root, Duration::ZERO);
            }
        }
    }

    pub(super) fn schedule_configured_dart_projects(&mut self) {
        if !self.dart_workspace_analysis_enabled {
            return;
        }
        let roots = self
            .workspaces
            .iter()
            .filter(|workspace| {
                workspace.join("pubspec.yaml").is_file()
                    || workspace.join("analysis_options.yaml").is_file()
            })
            .cloned()
            .collect::<Vec<_>>();
        for root in roots {
            self.schedule_dart_workspace_analysis(&root, Duration::from_secs(1));
        }
    }

    fn ensure_dart_process(&mut self, root_key: &crate::platform::PathKey) {
        if self.dart_disabled || self.dart_unavailable {
            return;
        }
        let Some(root) = self
            .dart_workspaces
            .get(root_key)
            .map(|state| state.root.clone())
        else {
            return;
        };
        let executable = dart_executable_for_root(&root);
        let Some(state) = self.dart_workspaces.get_mut(root_key) else {
            return;
        };
        if state.process.is_none() {
            self.dart_status = super::LspServerStatus::Starting;
            state.process = Some(LspProcess::start_with_executable(
                &DART_SERVER,
                vec![root],
                Some(executable),
            ));
        }
    }

    fn schedule_dart_workspace_analysis(&mut self, root: &Path, debounce: Duration) {
        if !self.dart_workspace_analysis_enabled {
            return;
        }
        let root_key = crate::platform::PathKey::new(root);
        let state = self
            .dart_workspaces
            .entry(root_key)
            .or_insert_with(|| DartWorkspaceState::new(root.to_path_buf()));
        state.generation = state.generation.wrapping_add(1).max(1);
        state.cancel_job();
        state.due_at = Some(Instant::now() + debounce);
    }

    fn start_due_dart_workspace_jobs(&mut self) {
        if !self.dart_workspace_analysis_enabled
            || self.dart_disabled
            || self.dart_unavailable
            || self.suppress_diagnostics
        {
            return;
        }
        let now = Instant::now();
        let keys = self.dart_workspaces.keys().cloned().collect::<Vec<_>>();
        for key in keys {
            let Some(state) = self.dart_workspaces.get_mut(&key) else {
                continue;
            };
            if state.job.is_some() || state.due_at.is_none_or(|due| due > now) {
                continue;
            }
            state.due_at = None;
            let root = state.root.clone();
            let generation = state.generation;
            let cancel = Arc::new(AtomicBool::new(false));
            let worker_cancel = cancel.clone();
            let (tx, rx) = mpsc::channel();
            let spawn = crate::platform::spawn_named("rriter-dart-analyze", move || {
                let diagnostics = run_dart_workspace_check(&root, &worker_cancel);
                let _ = tx.send(DartWorkspaceResult {
                    root,
                    generation,
                    diagnostics,
                });
            });
            if spawn.is_ok() {
                state.job = Some(DartAnalyzerJob {
                    generation,
                    cancel,
                    rx,
                });
            } else {
                self.log_dart_workspace_error(
                    "Dart workspace diagnostics worker failed to start".to_string(),
                );
            }
        }
    }

    fn apply_dart_workspace_result(&mut self, mut result: DartWorkspaceResult) -> usize {
        if !self.dart_workspace_analysis_enabled {
            return 0;
        }
        let root_key = crate::platform::PathKey::new(&result.root);
        let Some(state) = self.dart_workspaces.get(&root_key) else {
            return 0;
        };
        if result.generation != state.generation {
            return 0;
        }
        if let Some(job) = &state.job
            && job.generation > result.generation
        {
            return 0;
        }

        let diagnostics = match result.diagnostics.as_mut() {
            Ok(diagnostics) => diagnostics,
            Err(error) => {
                self.log_dart_workspace_error(format!(
                    "Dart workspace diagnostics failed for {}: {error}",
                    result.root.display()
                ));
                return 0;
            }
        };

        self.dart_workspace_diagnostics
            .retain(|path, _| !crate::platform::path_is_within(path, &result.root));
        let mut received = 0usize;
        for (path, items) in diagnostics.drain() {
            if !crate::platform::path_is_within(&path, &result.root) || !path.exists() {
                continue;
            }
            received = received.saturating_add(items.len());
            if items.is_empty() {
                continue;
            }
            let mut items = items;
            self.compact_diagnostic_text(&mut items);
            self.dart_workspace_diagnostics
                .insert(path, Arc::from(items.into_boxed_slice()));
        }
        self.rebuild_diag_text_pool();
        self.dirty_diagnostics = true;
        received
    }

    fn log_dart_workspace_error(&mut self, text: String) {
        self.server_logs
            .entry(DART_SERVER_NAME)
            .or_default()
            .push(LogEntry {
                text: format!("[LSP] {text}"),
                spans: Vec::new(),
                folds: Vec::new(),
                created_at: Instant::now(),
            });
    }
}

fn dart_executable_for_root(root: &Path) -> PathBuf {
    crate::platform::resolve_dart_for_workspace(Some(root))
        .path
        .unwrap_or_else(|| PathBuf::from(DART_SERVER.program))
}

fn run_dart_workspace_check(
    root: &Path,
    cancel: &AtomicBool,
) -> Result<HashMap<PathBuf, Vec<Diagnostic>>, String> {
    let executable = dart_executable_for_root(root);
    let mut command = crate::platform::command_for_executable(&executable)
        .map_err(|error| error.to_string())?;
    command
        .current_dir(root)
        .arg("analyze")
        .arg("--format")
        .arg("machine")
        .arg(root);
    let output = crate::platform::run_command_output_cancelable(
        &mut command,
        Duration::from_secs(180),
        cancel,
    )
    .map_err(|error| error.to_string())?;

    if output.stdout.is_empty() && !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "dart analyze exited with {}{}",
            output.status,
            (!stderr.trim().is_empty())
                .then(|| format!(": {}", stderr.trim()))
                .unwrap_or_default()
        ));
    }
    Ok(parse_dart_machine_output(&output.stdout, root))
}

pub(super) fn parse_dart_machine_output(
    raw: &[u8],
    root: &Path,
) -> HashMap<PathBuf, Vec<Diagnostic>> {
    let text = String::from_utf8_lossy(raw);
    let mut diagnostics: HashMap<PathBuf, Vec<Diagnostic>> = HashMap::new();
    for raw_line in text.lines() {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        let Some((path, diagnostic)) = parse_dart_machine_line(line, root) else {
            continue;
        };
        diagnostics.entry(path).or_default().push(diagnostic);
    }
    for items in diagnostics.values_mut() {
        items.sort_by(|left, right| {
            left.start_line
                .cmp(&right.start_line)
                .then(left.start_col.cmp(&right.start_col))
                .then_with(|| left.code.as_deref().cmp(&right.code.as_deref()))
                .then_with(|| left.message.as_ref().cmp(right.message.as_ref()))
        });
    }
    diagnostics
}

fn parse_dart_machine_line(line: &str, root: &Path) -> Option<(PathBuf, Diagnostic)> {
    let fields = split_dart_machine_fields(line);
    if fields.len() < 8 {
        return None;
    }
    let severity = dart_severity(&fields[0]);
    let code = non_empty_arc(&fields[2]);
    let path = resolve_dart_path(&fields[3], root)?;
    let line_number = fields[4].parse::<u32>().ok()?.saturating_sub(1);
    let column = fields[5].parse::<u32>().ok()?.saturating_sub(1);
    let length = fields[6].parse::<u32>().ok()?.max(1);
    let message = if fields.len() == 8 {
        fields[7].clone()
    } else {
        fields[7..].join("|")
    };
    if message.is_empty() {
        return None;
    }

    Some((
        path,
        Diagnostic {
            start_line: line_number,
            start_col: column,
            end_line: line_number,
            end_col: column.saturating_add(length),
            severity,
            code,
            code_href: None,
            message: Arc::<str>::from(message),
            source: Some(Arc::<str>::from(DART_SERVER_NAME)),
            quickfixes: Vec::new().into_boxed_slice(),
            tags: Vec::new().into_boxed_slice(),
        },
    ))
}

fn split_dart_machine_fields(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.peek().copied() {
                Some('|') | Some('\\') => {
                    if let Some(escaped) = chars.next()
                        && let Some(field) = fields.last_mut()
                    {
                        field.push(escaped);
                    }
                }
                Some(_) | None => {
                    if let Some(field) = fields.last_mut() {
                        field.push('\\');
                    }
                }
            }
        } else if ch == '|' {
            fields.push(String::new());
        } else if let Some(field) = fields.last_mut() {
            field.push(ch);
        }
    }
    fields
}

fn resolve_dart_path(value: &str, root: &Path) -> Option<PathBuf> {
    if value.is_empty() {
        return None;
    }
    let path = PathBuf::from(value);
    let bytes = value.as_bytes();
    let windows_absolute = bytes.len() >= 3
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
        || value.starts_with("\\\\");
    Some(if path.is_absolute() || windows_absolute {
        path
    } else {
        root.join(path)
    })
}

fn non_empty_arc(value: &str) -> Option<Arc<str>> {
    (!value.is_empty()).then(|| Arc::<str>::from(value))
}

fn dart_severity(value: &str) -> DiagSeverity {
    match value.to_ascii_uppercase().as_str() {
        "ERROR" => DiagSeverity::Error,
        "WARNING" => DiagSeverity::Warning,
        "INFO" => DiagSeverity::Info,
        "HINT" => DiagSeverity::Hint,
        _ => DiagSeverity::Info,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::{
        Cmd, LspEvent, LspRestartBudget, LspServerKind, PendingRequestCleanup,
        PendingRequestKind, RUFF_SERVER, TY_SERVER, command_for_server,
    };
    use std::collections::HashSet;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

    fn temp_dir(name: &str) -> PathBuf {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "rriter-dart-{name}-{}-{id}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn test_dart_process() -> (LspProcess, mpsc::Receiver<Cmd>) {
        let (process, commands, _events) = test_dart_process_with_events();
        (process, commands)
    }

    fn test_dart_process_with_events(
    ) -> (LspProcess, mpsc::Receiver<Cmd>, mpsc::Sender<LspEvent>) {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel::<LspEvent>();
        (
            LspProcess {
                cmd_tx,
                event_rx,
                current_uri: None,
                open_uris: HashSet::new(),
                def: &DART_SERVER,
                open_file_data: None,
                stop: Arc::new(AtomicBool::new(false)),
                supervisor: None,
                local_events: Mutex::new(Vec::new()),
                event_disconnected: AtomicBool::new(false),
            },
            cmd_rx,
            event_tx,
        )
    }

    fn test_diagnostic(message: &str) -> Diagnostic {
        Diagnostic {
            start_line: 0,
            start_col: 0,
            end_line: 0,
            end_col: 1,
            severity: DiagSeverity::Warning,
            code: Some(Arc::<str>::from("test_code")),
            code_href: None,
            message: Arc::<str>::from(message),
            source: Some(Arc::<str>::from("dart")),
            quickfixes: Vec::new().into_boxed_slice(),
            tags: Vec::new().into_boxed_slice(),
        }
    }

    #[test]
    fn dart_root_prefers_nearest_pubspec_inside_workspace() {
        let workspace = temp_dir("root-pubspec");
        let package = workspace.join("packages/app");
        let source = package.join("lib/main.dart");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(package.join("pubspec.yaml"), "name: app\n").unwrap();
        let root = dart_root_for_path(&source, &[workspace.clone()]);
        assert!(crate::platform::paths_equal(&root.path, &package));
        assert!(!root.is_flutter);
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[test]
    fn dart_root_uses_analysis_options_for_loose_package() {
        let workspace = temp_dir("root-analysis");
        let source = workspace.join("src/main.dart");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(workspace.join("analysis_options.yaml"), "analyzer:\n").unwrap();
        let root = dart_root_for_path(&source, &[]);
        assert!(crate::platform::paths_equal(&root.path, &workspace));
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[test]
    fn dart_root_uses_file_directory_for_loose_file() {
        let directory = temp_dir("root-loose");
        let source = directory.join("main.dart");
        let root = dart_root_for_path(&source, &[]);
        assert!(crate::platform::paths_equal(&root.path, &directory));
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn dart_root_detects_flutter_package() {
        let package = temp_dir("root-flutter");
        let source = package.join("lib/main.dart");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(
            package.join("pubspec.yaml"),
            "name: app\ndependencies:\n  flutter:\n    sdk: flutter\n",
        )
        .unwrap();
        let root = dart_root_for_path(&source, &[package.clone()]);
        assert!(root.is_flutter);
        let _ = std::fs::remove_dir_all(package);
    }

    #[test]
    fn dart_machine_parser_handles_escaped_pipe_and_backslash() {
        let root = PathBuf::from("/tmp/dart-root");
        let raw = b"WARNING|STATIC_WARNING|unused_local_variable|lib/main.dart|3|5|4|message with \\| pipe and \\\\ slash\n";
        let parsed = parse_dart_machine_output(raw, &root);
        let items = parsed.get(&root.join("lib/main.dart")).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].message.as_ref(), "message with | pipe and \\ slash");
        assert_eq!(items[0].start_line, 2);
        assert_eq!(items[0].start_col, 4);
        assert_eq!(items[0].end_col, 8);
        assert_eq!(items[0].severity, DiagSeverity::Warning);
        assert_eq!(items[0].source.as_deref(), Some("dart"));
    }

    #[test]
    fn dart_machine_parser_preserves_windows_paths_and_unicode() {
        let root = PathBuf::from("C:\\workspace");
        let raw = "ERROR|COMPILE_TIME_ERROR|undefined_identifier|C:\\workspace\\lib\\тест.dart|2|7|1|Неизвестное имя\r\n";
        let parsed = parse_dart_machine_output(raw.as_bytes(), &root);
        let path = PathBuf::from("C:\\workspace\\lib\\тест.dart");
        let items = parsed.get(&path).unwrap();
        assert_eq!(items[0].message.as_ref(), "Неизвестное имя");
        assert_eq!(items[0].severity, DiagSeverity::Error);
    }

    #[test]
    fn malformed_dart_machine_lines_do_not_drop_valid_results() {
        let root = PathBuf::from("/tmp/dart-root");
        let raw = b"bad line\nINFO|LINT|avoid_print|lib/main.dart|1|1|5|Avoid print\nWARNING|TYPE|code|lib/other.dart|x|1|1|bad number\n";
        let parsed = parse_dart_machine_output(raw, &root);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[&root.join("lib/main.dart")].len(), 1);
    }

    #[test]
    fn unknown_dart_severity_is_non_fatal_info() {
        assert_eq!(dart_severity("NOTICE"), DiagSeverity::Info);
    }

    #[test]
    fn multiple_dart_documents_share_one_workspace_process() {
        let package = temp_dir("lifecycle-shared");
        std::fs::write(package.join("pubspec.yaml"), "name: app\n").unwrap();
        let first = package.join("lib/a.dart");
        let second = package.join("lib/b.dart");
        std::fs::create_dir_all(first.parent().unwrap()).unwrap();
        let (process, commands) = test_dart_process();
        let root_key = crate::platform::PathKey::new(&package);
        let mut state = DartWorkspaceState::new(package.clone());
        state.process = Some(process);
        let mut manager = LspManager::new(vec![package.clone()]);
        manager.dart_workspaces.insert(root_key, state);

        manager.notify_open(&first, "dart", "void a() {}\n", 1);
        manager.notify_open(&second, "dart", "void b() {}\n", 1);

        assert_eq!(manager.dart_workspaces.len(), 1);
        assert_eq!(manager.open_dart_files.len(), 2);
        for expected in [&first, &second] {
            match commands.try_recv().unwrap() {
                Cmd::Open {
                    uri,
                    lang,
                    version,
                    ..
                } => {
                    assert_eq!(uri, crate::lsp::path_to_uri(expected));
                    assert_eq!(lang, "dart");
                    assert_eq!(version, 1);
                }
                _ => panic!("expected Dart didOpen"),
            }
        }
        let _ = std::fs::remove_dir_all(package);
    }

    #[test]
    fn dart_change_versions_are_monotonic_and_close_is_per_document() {
        let package = temp_dir("lifecycle-version");
        std::fs::write(package.join("pubspec.yaml"), "name: app\n").unwrap();
        let first = package.join("lib/a.dart");
        let second = package.join("lib/b.dart");
        std::fs::create_dir_all(first.parent().unwrap()).unwrap();
        let (process, commands) = test_dart_process();
        let root_key = crate::platform::PathKey::new(&package);
        let mut state = DartWorkspaceState::new(package.clone());
        state.process = Some(process);
        let mut manager = LspManager::new(vec![package.clone()]);
        manager.dart_workspaces.insert(root_key.clone(), state);
        manager.notify_open(&first, "dart", "void a() {}\n", 1);
        manager.notify_open(&second, "dart", "void b() {}\n", 1);
        let _ = commands.try_recv().unwrap();
        let _ = commands.try_recv().unwrap();

        manager.notify_change(&first, "dart", "void a() { print(1); }\n", 3);
        assert!(matches!(
            commands.try_recv().unwrap(),
            Cmd::Change { version: 3, .. }
        ));
        manager.notify_change(&first, "dart", "stale\n", 2);
        assert!(commands.try_recv().is_err());
        assert_eq!(
            manager
                .open_dart_files
                .get(&crate::platform::PathKey::new(&first))
                .map(|open| open.version),
            Some(3)
        );

        manager.notify_close(&first, "dart");
        assert!(matches!(commands.try_recv().unwrap(), Cmd::Close { .. }));
        assert!(manager
            .dart_workspaces
            .get(&root_key)
            .and_then(|state| state.process.as_ref())
            .is_some());
        manager.notify_close(&second, "dart");
        assert!(matches!(commands.try_recv().unwrap(), Cmd::Close { .. }));
        assert!(matches!(commands.try_recv().unwrap(), Cmd::Shutdown));
        assert!(manager.open_dart_files.is_empty());
        assert_eq!(manager.drain_closed_dart_documents().len(), 2);
        let _ = std::fs::remove_dir_all(package);
    }

    #[test]
    fn dart_project_open_and_analysis_config_save_schedule_workspace_analysis() {
        let package = temp_dir("analyzer-project-open");
        std::fs::write(package.join("pubspec.yaml"), "name: app\n").unwrap();
        let mut manager = LspManager::new(vec![package.clone()]);
        let root_key = crate::platform::PathKey::new(&package);
        let initial_generation = manager.dart_workspaces[&root_key].generation;
        assert!(manager.dart_workspaces[&root_key].due_at.is_some());

        manager.notify_saved(&package.join("analysis_options.yaml"), "yaml");
        assert!(manager.dart_workspaces[&root_key].generation > initial_generation);
        manager.set_dart_workspace_analysis_enabled(false);
        assert!(manager.dart_workspaces[&root_key].due_at.is_none());
        assert!(manager.dart_workspace_diagnostics.is_empty());
        let disabled_generation = manager.dart_workspaces[&root_key].generation;
        manager.notify_saved(&package.join("pubspec.yaml"), "yaml");
        assert_eq!(manager.dart_workspaces[&root_key].generation, disabled_generation);
        let _ = std::fs::remove_dir_all(package);
    }

    #[test]
    fn dart_analyzer_is_not_scheduled_for_each_edit() {
        let package = temp_dir("analyzer-debounce");
        std::fs::write(package.join("pubspec.yaml"), "name: app\n").unwrap();
        let path = package.join("lib/main.dart");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut manager = LspManager::new(vec![package.clone()]);
        manager.dart_unavailable = true;
        manager.notify_open(&path, "dart", "void main() {}\n", 1);
        let root_key = crate::platform::PathKey::new(&package);
        let initial_generation = manager.dart_workspaces[&root_key].generation;
        manager.dart_workspaces.get_mut(&root_key).unwrap().due_at = None;

        manager.notify_change(&path, "dart", "void main() { print(1); }\n", 2);
        assert_eq!(manager.dart_workspaces[&root_key].generation, initial_generation);
        assert!(manager.dart_workspaces[&root_key].due_at.is_none());

        manager.notify_saved(&path, "dart");
        assert!(manager.dart_workspaces[&root_key].due_at.is_some());
        assert!(manager.dart_workspaces[&root_key].generation > initial_generation);
        let _ = std::fs::remove_dir_all(package);
    }

    #[test]
    fn live_dart_diagnostics_override_workspace_diagnostics_while_open() {
        let package = temp_dir("diagnostics-priority");
        std::fs::write(package.join("pubspec.yaml"), "name: app\n").unwrap();
        let path = package.join("lib/main.dart");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "void main() {}\n").unwrap();
        let mut manager = LspManager::new(vec![package.clone()]);
        manager.dart_unavailable = true;
        manager.notify_open(&path, "dart", "void main() {}\n", 4);
        manager.dart_workspace_diagnostics.insert(
            path.clone(),
            Arc::from(vec![test_diagnostic("workspace")].into_boxed_slice()),
        );
        manager.dart_live_diagnostics.insert(
            path.clone(),
            (4, Arc::from(vec![test_diagnostic("live")].into_boxed_slice())),
        );
        manager.rebuild_merged_diagnostic_indices();
        manager.dirty_diagnostics = false;

        assert_eq!(manager.diagnostic_count(&path), 1);
        assert_eq!(manager.diagnostic_at(&path, 0).unwrap().message.as_ref(), "live");

        manager.notify_close(&path, "dart");
        manager.rebuild_merged_diagnostic_indices();
        manager.dirty_diagnostics = false;
        assert_eq!(manager.diagnostic_count(&path), 1);
        assert_eq!(
            manager.diagnostic_at(&path, 0).unwrap().message.as_ref(),
            "workspace"
        );
        let _ = std::fs::remove_dir_all(package);
    }

    #[test]
    fn dart_live_diagnostics_require_current_open_document_version() {
        let package = temp_dir("diagnostics-version");
        std::fs::write(package.join("pubspec.yaml"), "name: app\n").unwrap();
        let first = package.join("lib/first.dart");
        let second = package.join("lib/second.dart");
        std::fs::create_dir_all(first.parent().unwrap()).unwrap();
        let (process, commands, events) = test_dart_process_with_events();
        let root_key = crate::platform::PathKey::new(&package);
        let mut state = DartWorkspaceState::new(package.clone());
        state.process = Some(process);
        let mut manager = LspManager::new(vec![package.clone()]);
        manager.dart_workspaces.insert(root_key, state);
        manager.notify_open(&first, "dart", "void first() {}\n", 3);
        manager.notify_open(&second, "dart", "void second() {}\n", 1);
        let _ = commands.try_recv().unwrap();
        let _ = commands.try_recv().unwrap();

        events
            .send(LspEvent::Diagnostics {
                server: crate::lsp::LspServerKind::Dart,
                path: first.clone(),
                version: Some(2),
                items: vec![test_diagnostic("stale")],
                result_id: None,
            })
            .unwrap();
        manager.poll();
        assert_eq!(manager.diagnostic_count(&first), 0);

        events
            .send(LspEvent::Diagnostics {
                server: crate::lsp::LspServerKind::Dart,
                path: first.clone(),
                version: Some(3),
                items: vec![test_diagnostic("current")],
                result_id: None,
            })
            .unwrap();
        manager.poll();
        assert_eq!(manager.diagnostic_count(&first), 1);

        manager.notify_close(&first, "dart");
        assert!(matches!(commands.try_recv().unwrap(), Cmd::Close { .. }));
        manager.notify_change(&first, "dart", "late edit\n", 4);
        assert!(commands.try_recv().is_err());
        assert!(!manager
            .open_dart_files
            .contains_key(&crate::platform::PathKey::new(&first)));
        events
            .send(LspEvent::Diagnostics {
                server: crate::lsp::LspServerKind::Dart,
                path: first.clone(),
                version: Some(3),
                items: vec![test_diagnostic("after close")],
                result_id: None,
            })
            .unwrap();
        manager.poll();
        assert_eq!(manager.diagnostic_count(&first), 0);

        manager.notify_close(&second, "dart");
        let _ = std::fs::remove_dir_all(package);
    }

    #[test]
    fn stale_dart_analyzer_generation_is_ignored() {
        let package = temp_dir("generation-stale");
        let path = package.join("lib/main.dart");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "void main() {}\n").unwrap();
        let root_key = crate::platform::PathKey::new(&package);
        let mut manager = LspManager::new(vec![package.clone()]);
        let mut state = DartWorkspaceState::new(package.clone());
        state.generation = 2;
        manager.dart_workspaces.insert(root_key, state);
        let mut diagnostics = HashMap::new();
        diagnostics.insert(path.clone(), vec![test_diagnostic("stale")]);

        let received = manager.apply_dart_workspace_result(DartWorkspaceResult {
            root: package.clone(),
            generation: 1,
            diagnostics: Ok(diagnostics),
        });

        assert_eq!(received, 0);
        assert!(!manager.dart_workspace_diagnostics.contains_key(&path));
        let _ = std::fs::remove_dir_all(package);
    }

    #[test]
    fn current_dart_analyzer_result_removes_deleted_file_diagnostics() {
        let package = temp_dir("deleted-diagnostic");
        let removed = package.join("lib/removed.dart");
        let root_key = crate::platform::PathKey::new(&package);
        let mut manager = LspManager::new(vec![package.clone()]);
        let mut state = DartWorkspaceState::new(package.clone());
        state.generation = 1;
        manager.dart_workspaces.insert(root_key, state);
        manager.dart_workspace_diagnostics.insert(
            removed.clone(),
            Arc::from(vec![test_diagnostic("old")].into_boxed_slice()),
        );

        let received = manager.apply_dart_workspace_result(DartWorkspaceResult {
            root: package.clone(),
            generation: 1,
            diagnostics: Ok(HashMap::new()),
        });

        assert_eq!(received, 0);
        assert!(!manager.dart_workspace_diagnostics.contains_key(&removed));
        let _ = std::fs::remove_dir_all(package);
    }
    #[test]
    fn expected_dart_shutdown_does_not_mark_sdk_missing() {
        let root = temp_dir("expected-shutdown-status");
        let root_key = crate::platform::PathKey::new(&root);
        let (process, _commands, events) = test_dart_process_with_events();
        let mut state = DartWorkspaceState::new(root.clone());
        state.process = Some(process);
        let mut manager = LspManager::new(vec![root.clone()]);
        manager.dart_workspaces.insert(root_key, state);

        events
            .send(LspEvent::StatusChanged {
                server: LspServerKind::Dart,
                status: crate::lsp::LspServerStatus::Disabled,
            })
            .unwrap();
        let _ = manager.poll();

        assert_eq!(
            manager.dart_status,
            crate::lsp::LspServerStatus::Disabled
        );
        assert!(!manager.dart_unavailable);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn dart_enable_and_restart_do_not_depend_on_open_python_documents() {
        let mut manager = LspManager::new(Vec::new());
        manager.python_disabled = true;

        manager.set_server_enabled(DART_SERVER_NAME, false);
        assert!(manager.dart_disabled);
        assert!(manager.python_disabled);

        manager.set_server_enabled(DART_SERVER_NAME, true);
        assert!(!manager.dart_disabled);
        assert!(manager.python_disabled);

        manager.dart_disabled = true;
        manager.dart_unavailable = true;
        manager.restart_server(DART_SERVER_NAME);
        assert!(!manager.dart_disabled);
        assert!(!manager.dart_unavailable);
        assert!(manager.python_disabled);
    }


    #[test]
    fn dart_and_python_server_definitions_are_stable() {
        assert_eq!(DART_SERVER.kind, LspServerKind::Dart);
        assert_eq!(DART_SERVER.program, "dart");
        assert_eq!(DART_SERVER.override_env, "RRITER_DART_PATH");
        assert_eq!(DART_SERVER.args, &["language-server"]);
        assert_eq!(DART_SERVER.language_id, "dart");
        assert_eq!(DART_SERVER.extensions, &["dart"]);

        assert_eq!(RUFF_SERVER.kind, LspServerKind::Ruff);
        assert_eq!(RUFF_SERVER.program, "ruff");
        assert_eq!(RUFF_SERVER.override_env, "RRITER_RUFF_PATH");
        assert_eq!(RUFF_SERVER.args, &["server"]);
        assert_eq!(RUFF_SERVER.language_id, "python");
        assert_eq!(RUFF_SERVER.extensions, &["py"]);

        assert_eq!(TY_SERVER.kind, LspServerKind::Ty);
        assert_eq!(TY_SERVER.program, "ty");
        assert_eq!(TY_SERVER.override_env, "RRITER_TY_PATH");
        assert_eq!(TY_SERVER.args, &["server"]);
        assert_eq!(TY_SERVER.language_id, "python");
        assert_eq!(TY_SERVER.extensions, &["py"]);
    }

    #[test]
    fn explicit_dart_executable_precedes_environment_and_path_resolution() {
        let executable = std::env::current_exe().unwrap();
        let command = command_for_server(&DART_SERVER, Some(&executable), None).unwrap();
        assert_eq!(Path::new(command.get_program()), executable.as_path());
        assert_eq!(
            command
                .get_args()
                .map(|argument| argument.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            vec!["language-server".to_string()]
        );
    }

    #[test]
    fn dart_restart_budget_is_bounded_and_pending_requests_are_cleared() {
        let mut budget = LspRestartBudget::default();
        for attempt in 1..=LspServerKind::Dart.restart_attempt_limit() {
            assert_eq!(budget.begin_attempt_for(LspServerKind::Dart), Some(attempt));
        }
        assert_eq!(budget.begin_attempt_for(LspServerKind::Dart), None);
        budget.mark_stable();
        assert_eq!(budget.begin_attempt_for(LspServerKind::Dart), Some(1));

        let pending = Arc::new(Mutex::new(HashMap::from([(
            44,
            PendingRequestKind::Completion,
        )])));
        {
            let _cleanup = PendingRequestCleanup(pending.clone());
        }
        assert!(crate::platform::lock_recover(&pending).is_empty());
    }
}
