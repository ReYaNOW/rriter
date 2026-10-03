use super::rooted_language::{OpenRootedFile, RootedLanguage};
use super::rust_workspace::{RustRootEntry, RustRootJob, RustRootResolution, RustToolError, RustTools};
use super::LspManager;
use crate::platform::{PathKey, ToolKind};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RustRowInfo {
    pub status: super::LspServerStatus,
    pub busy: bool,
    pub health_message: Option<String>,
    pub cargo_missing: bool,
    pub component_missing: bool,
    pub version: Option<String>,
    pub roots: usize,
}

impl LspManager {
    pub fn rust_row_info(&self) -> RustRowInfo {
        let component_missing = self.rust_roots.values().any(|entry| {
            matches!(entry, RustRootEntry::Ready(result) if result.executable == Err(RustToolError::ComponentMissing))
        });
        let version = self.rust_roots.values().find_map(|entry| match entry {
            RustRootEntry::Ready(result) => result.version.clone(),
            RustRootEntry::Pending(_) => None,
        });
        let (missing_roots, status_roots) = self.rust.root_status_counts();
        let roots = status_roots.max(self.rust_roots.len());
        let mut health_message = self.rust_last_health_message.clone();
        if missing_roots > 0 && missing_roots < roots {
            let missing_root = self.rust.first_missing_root_key()
                .and_then(|key| self.rust_roots.get(key))
                .and_then(|entry| match entry {
                    RustRootEntry::Ready(result) => Some((&result.root, result.executable.as_ref().err())),
                    RustRootEntry::Pending(_) => None,
                });
            let reason = missing_root
                .and_then(|(_, error)| error)
                .map(|error| match error {
                    RustToolError::ComponentMissing => "rust-analyzer отсутствует в toolchain",
                    RustToolError::Timeout => "таймаут разрешения rust-analyzer",
                    RustToolError::NotFound => "rust-analyzer не найден для корня",
                })
                .unwrap_or("причина неизвестна");
            let availability = match missing_root {
                Some((root, _)) => format!("недоступен {missing_roots} из {roots} корней · {reason}: {}", root.display()),
                None => format!("недоступен {missing_roots} из {roots} корней · {reason}"),
            };
            health_message = Some(match health_message {
                Some(message) => format!("{availability} · {message}"),
                None => availability,
            });
        }
        RustRowInfo {
            status: self.rust.status(),
            busy: self.rust.busy(),
            health_message,
            cargo_missing: self.rust_tools.as_ref().is_some_and(|tools| tools.cargo.is_none()),
            component_missing,
            version,
            roots,
        }
    }

    fn current_rust_tools(&mut self) -> &RustTools {
        #[cfg(test)]
        if let Some(tools) = self.rust_tools_override_for_test.as_ref() {
            return tools;
        }
        self.rust_tools.get_or_insert_with(|| RustTools {
            cargo: crate::platform::resolve_executable(std::ffi::OsStr::new("cargo")),
            rustup: crate::platform::resolve_executable(std::ffi::OsStr::new("rustup")),
            resolution: crate::platform::resolve_tool_kind(ToolKind::RustAnalyzer),
        })
    }

    pub(super) fn rust_executable_for_root(&mut self, root: &Path) -> Option<PathBuf> {
        self.rust_roots.values().find_map(|entry| match entry {
            RustRootEntry::Ready(result) if crate::platform::paths_equal(&result.root, root) => {
                result.executable.as_ref().ok().cloned()
            }
            _ => None,
        })
    }

    pub(super) fn open_rust_document(&mut self, path: PathBuf, text: Arc<str>, version: i32) {
        let path_key = PathKey::new(&path);
        let Some(crate_dir) = super::rust_workspace::nearest_cargo_toml_dir(&path, &self.workspaces)
        else {
            return;
        };
        let (text, version) = self.rust_documents.get(&path_key)
            .filter(|existing| version <= existing.version)
            .map(|existing| (existing.text.clone(), existing.version))
            .unwrap_or((text, version));
        let document = OpenRootedFile {
            path: path.clone(),
            root: crate_dir.clone(),
            text,
            version,
        };
        self.rust_documents.insert(path_key.clone(), document.clone());
        if !self.rust.enabled() {
            return;
        }
        if let Some(existing) = self.rust.open_files().get(&path_key) {
            if version > existing.version {
                self.change_rust_document(path, document.text, version);
            }
            return;
        }
        if let Some(existing) = self.rust_pending_docs.get_mut(&path_key) {
            if version > existing.version {
                existing.text = document.text;
                existing.version = version;
            }
            return;
        }
        let root_key = PathKey::new(&crate_dir);
        let pending = document;
        if let Some(entry) = self.rust_roots.get(&root_key) {
            match entry {
                RustRootEntry::Ready(resolution) => {
                    if let Ok(executable) = &resolution.executable {
                        self.rust.open_document(
                            path,
                            pending.text,
                            version,
                            resolution.root.clone(),
                            Some(executable.clone()),
                            &self.ui_waker,
                        );
                    } else {
                        self.rust.mark_root_missing(&root_key);
                    }
                    return;
                }
                RustRootEntry::Pending(_) => {
                    self.rust_pending_docs.insert(path_key, pending);
                    self.rust.set_pending_status(!self.rust_pending_docs.is_empty());
                    return;
                }
            }
        }

        let tools = self.current_rust_tools();
        let cargo = tools.cargo.clone();
        let rustup = tools.rustup.clone();
        let resolution = tools.resolution.clone();
        let generation = self.rust_roots_generation;
        #[cfg(test)]
        let test_runner = self.rust_run_cmd.clone();
        let job_dir = crate_dir.clone();
        let spawn = self.ui_waker.spawn_one_shot("rriter-rust-root", move || {
            let tools = RustTools { cargo, rustup, resolution };
            #[cfg(test)]
            if let Some(run) = test_runner {
                return super::rust_workspace::resolve_rust_root_with(&job_dir, &tools, run.as_ref());
            }
            super::rust_workspace::resolve_rust_root_blocking(&job_dir, &tools)
        });
        match spawn {
            Ok(rx) => {
                self.rust_roots.insert(
                    root_key,
                    RustRootEntry::Pending(RustRootJob { generation, rx }),
                );
                self.rust_pending_docs.insert(path_key, pending);
                self.rust.set_pending_status(true);
            }
            Err(error) => {
                self.log_rust_error(format!("Rust root worker failed to start: {error}"));
                self.rust.mark_root_missing(&root_key);
            }
        }
    }

    pub(super) fn change_rust_document(&mut self, path: PathBuf, text: Arc<str>, version: i32) {
        let key = PathKey::new(&path);
        if let Some(document) = self.rust_documents.get_mut(&key) {
            if version > document.version {
                document.text = text.clone();
                document.version = version;
            }
        } else {
            return;
        }
        if let Some(pending) = self.rust_pending_docs.get_mut(&key) {
            if version > pending.version {
                pending.text = text;
                pending.version = version;
            }
            return;
        }
        if self.rust.open_files().contains_key(&key) {
            self.rust.change_document(&path, text, version);
        }
    }

    pub(super) fn close_rust_document(&mut self, path: &Path) {
        let key = PathKey::new(path);
        let root = self.rust_documents.remove(&key).map(|file| file.root);
        let was_pending = self.rust_pending_docs.remove(&key).is_some();
        if was_pending {
            self.rust.set_pending_status(!self.rust_pending_docs.is_empty());
        } else if let Some(root_key) = self.rust.close_document(path)
            && self.rooted_root_may_stop(RootedLanguage::Rust, &root_key)
        {
            self.stop_rooted_root(RootedLanguage::Rust, &root_key);
        }
        if let Some(root) = root {
            let root_key = PathKey::new(&root);
            if !self.rust_documents.values().any(|file| PathKey::new(&file.root) == root_key) {
                self.rust.clear_root_status_if_no_open_files(&root_key);
                self.rust_roots.remove(&root_key);
            }
        }
    }

    pub(super) fn poll_rust_root_jobs(&mut self) {
        let keys = self.rust_roots.keys().cloned().collect::<Vec<_>>();
        let mut completed = Vec::new();
        for key in keys {
            let Some(RustRootEntry::Pending(job)) = self.rust_roots.get_mut(&key) else {
                continue;
            };
            match job.rx.poll() {
                crate::ui_waker::OneShotState::Ready(result) => {
                    completed.push((key, job.generation, Ok(result)));
                }
                crate::ui_waker::OneShotState::Pending => {}
                crate::ui_waker::OneShotState::Closed => {
                    // A dropped worker is a failure of this root only, never "no rust-analyzer at all".
                    completed.push((key, job.generation, Err(RustToolError::Timeout)));
                }
            }
        }
        for (key, generation, result) in completed {
            if generation != self.rust_roots_generation {
                continue;
            }
            let resolution = match result {
                Ok(resolution) => resolution,
                Err(error) => {
                    self.log_rust_error(format!("Rust root worker disconnected: {error:?}"));
                    let Some(root) = self.rust_pending_docs.values()
                        .find(|file| PathKey::new(&file.root) == key)
                        .map(|file| file.root.clone())
                    else {
                        self.rust_roots.remove(&key);
                        continue;
                    };
                    RustRootResolution {
                        root,
                        executable: Err(error),
                        version: None,
                    }
                }
            };
            let pending_keys = self.rust_pending_docs.iter()
                .filter(|(_, file)| PathKey::new(&file.root) == key)
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>();
            if pending_keys.is_empty() {
                self.rust_roots.remove(&key);
                continue;
            }
            self.rust_roots.insert(key.clone(), RustRootEntry::Ready(resolution.clone()));
            if let Ok(executable) = &resolution.executable {
                for path_key in pending_keys {
                    let Some(file) = self.rust_documents.get(&path_key).cloned() else {
                        self.rust_pending_docs.remove(&path_key);
                        continue;
                    };
                    self.rust_pending_docs.remove(&path_key);
                    self.rust.open_document(
                        file.path,
                        file.text,
                        file.version,
                        resolution.root.clone(),
                        Some(executable.clone()),
                        &self.ui_waker,
                    );
                }
            } else {
                for path_key in pending_keys {
                    self.rust_pending_docs.remove(&path_key);
                }
                match resolution.executable {
                    Err(RustToolError::ComponentMissing | RustToolError::Timeout) => {
                        self.rust.mark_root_missing(&key);
                    }
                    Err(RustToolError::NotFound) => {
                        let globally_missing = self.current_rust_tools().resolution.path.is_none();
                        if globally_missing {
                            self.rust.mark_missing();
                        } else {
                            self.rust.mark_root_missing(&key);
                        }
                    }
                    Ok(_) => {}
                }
            }
        }
        self.rust.set_pending_status(!self.rust_pending_docs.is_empty());
    }

    fn log_rust_error(&mut self, message: String) {
        self.server_logs.entry(super::RUST_ANALYZER_SERVER.program)
            .or_default()
            .push(super::LogEntry {
                text: format!("[LSP] {message}"),
                spans: Vec::new(),
                folds: Vec::new(),
                created_at: std::time::Instant::now(),
            });
    }

    pub fn set_rust_enabled(&mut self, enabled: bool) {
        let was_enabled = self.rust.enabled();
        let was_missing = self.rust.missing();
        self.rust.set_enabled(enabled);
        if was_enabled != enabled && !enabled {
            self.rust_roots_generation = self.rust_roots_generation.wrapping_add(1);
            self.rust_pending_docs.clear();
            self.rust_roots.clear();
        }
        if enabled {
            self.rust.clear_missing();
            if !was_enabled || was_missing {
                self.refresh_rust_resolution();
            }
        }
    }

    pub fn set_rust_init_options(&mut self, value: serde_json::Value) {
        if self.rust.set_init_options(Some(value)) {
            let executables = self.rust_roots.values().filter_map(|entry| match entry {
                RustRootEntry::Ready(result) => Some((PathKey::new(&result.root), result.executable.as_ref().ok().cloned())),
                RustRootEntry::Pending(_) => None,
            }).collect::<HashMap<_, _>>();
            let ui_waker = self.ui_waker.clone();
            self.rust.restart_all(|root| executables.get(&PathKey::new(root)).cloned().flatten(), &ui_waker);
        }
    }

    pub fn refresh_rust_resolution(&mut self) {
        let was_enabled = self.rust.enabled();
        let documents = self.rust_documents.values()
            .map(|file| (file.path.clone(), file.text.clone(), file.version))
            .collect::<Vec<_>>();
        if was_enabled { self.rust.set_enabled(false); }
        if was_enabled {
            for (path, _, _) in &documents {
                self.rust.close_document(path);
            }
        }
        crate::platform::refresh_tool_resolutions();
        self.rust_tools = None;
        self.rust_roots.clear();
        self.rust_roots_generation = self.rust_roots_generation.wrapping_add(1);
        self.rust.clear_missing();
        self.rust_pending_docs.clear();
        if was_enabled {
            self.rust.set_enabled(true);
            for (path, text, version) in documents {
                self.open_rust_document(path, text, version);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn set_rust_tools_for_test(&mut self, tools: RustTools) {
        self.rust_tools_override_for_test = Some(tools);
    }

    #[cfg(test)]
    pub(crate) fn set_rust_run_cmd_for_test(&mut self, run: Arc<super::rust_workspace::RunCmd>) {
        self.rust_run_cmd = Some(run);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    fn fixture(name: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("rriter-rust-manager-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"fixture\"\n").unwrap();
        (root.clone(), root.join("src/lib.rs"))
    }

    fn manager_with_runner(
        root: &Path,
        release_rx: mpsc::Receiver<()>,
        started_tx: mpsc::Sender<()>,
    ) -> LspManager {
        let mut manager = LspManager::new(vec![root.to_path_buf()]);
        manager.set_rust_tools_for_test(RustTools {
            cargo: None,
            rustup: Some(PathBuf::from("rustup-stub")),
            resolution: crate::platform::ToolResolution {
                path: None,
                configured_path: None,
                source: None,
                sdk_root: None,
            },
        });
        let release_rx = Arc::new(Mutex::new(release_rx));
        let first = Arc::new(AtomicBool::new(true));
        manager.set_rust_run_cmd_for_test(Arc::new(move |_, args, _, _| {
            if args.first() == Some(&"which") {
                if first.swap(false, Ordering::AcqRel) {
                    let _ = started_tx.send(());
                    let _ = release_rx.lock().unwrap().recv();
                }
                Ok(String::from("rust-analyzer-stub"))
            } else {
                Ok(String::from("rust-analyzer 1.0.0"))
            }
        }));
        manager
    }

    fn poll_until(manager: &mut LspManager, timeout: Duration, done: impl Fn(&LspManager) -> bool) {
        let start = Instant::now();
        while !done(manager) && start.elapsed() < timeout {
            manager.poll_rust_root_jobs();
            std::thread::sleep(Duration::from_millis(5));
        }
        manager.poll_rust_root_jobs();
        assert!(done(manager), "Rust root job did not finish before timeout");
    }

    #[test]
    fn pending_documents_share_one_job_and_latest_text_is_opened() {
        let (root, path) = fixture("shared-job");
        let second = root.join("src/other.rs");
        let (release_tx, release_rx) = mpsc::channel();
        let (started_tx, started_rx) = mpsc::channel();
        let mut manager = manager_with_runner(&root, release_rx, started_tx);
        manager.notify_open(&path, "rs", "first", 1);
        assert!(started_rx.recv_timeout(Duration::from_secs(2)).is_ok());
        manager.notify_open(&second, "rs", "second", 1);
        manager.notify_change(&path, "rs", "latest", 2);
        assert_eq!(manager.rust_roots.len(), 1);
        assert_eq!(manager.rust_pending_docs.len(), 2);
        release_tx.send(()).unwrap();
        poll_until(&mut manager, Duration::from_secs(2), |manager| {
            manager.rust.open_files().len() == 2
        });
        assert_eq!(manager.rust.open_files().get(&PathKey::new(&path)).map(|file| file.text.as_ref()), Some("latest"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn closing_pending_document_prevents_did_open() {
        let (root, path) = fixture("closed-pending");
        let (release_tx, release_rx) = mpsc::channel();
        let (started_tx, started_rx) = mpsc::channel();
        let mut manager = manager_with_runner(&root, release_rx, started_tx);
        manager.notify_open(&path, "rs", "pending", 1);
        assert!(started_rx.recv_timeout(Duration::from_secs(2)).is_ok());
        manager.notify_close(&path, "rs");
        release_tx.send(()).unwrap();
        poll_until(&mut manager, Duration::from_secs(2), |manager| manager.rust_roots.is_empty());
        assert!(manager.rust.open_files().is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn disabling_rust_discards_in_flight_root_result() {
        let (root, path) = fixture("disabled-pending");
        let (release_tx, release_rx) = mpsc::channel();
        let (started_tx, started_rx) = mpsc::channel();
        let mut manager = manager_with_runner(&root, release_rx, started_tx);
        manager.notify_open(&path, "rs", "pending", 1);
        assert!(started_rx.recv_timeout(Duration::from_secs(2)).is_ok());
        manager.set_rust_enabled(false);
        release_tx.send(()).unwrap();
        poll_until(&mut manager, Duration::from_secs(2), |manager| manager.rust_roots.is_empty());
        assert!(manager.rust_pending_docs.is_empty());
        assert!(manager.rust.open_files().is_empty());
        assert!(manager.rust_documents.contains_key(&PathKey::new(&path)));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn missing_analyzer_refresh_reopens_the_document() {
        let (root, path) = fixture("missing-refresh");
        let mut manager = LspManager::new(vec![root.clone()]);
        let installed = Arc::new(AtomicBool::new(false));
        manager.set_rust_tools_for_test(RustTools {
            cargo: None,
            rustup: Some(PathBuf::from("rustup-stub")),
            resolution: crate::platform::ToolResolution {
                path: None,
                configured_path: None,
                source: None,
                sdk_root: None,
            },
        });
        let installed_by_runner = installed.clone();
        manager.set_rust_run_cmd_for_test(Arc::new(move |_, args, _, _| {
            if !installed_by_runner.load(Ordering::Acquire) {
                return Err(RustToolError::ComponentMissing);
            }
            if args.first() == Some(&"which") {
                Ok(String::from("rust-analyzer-stub"))
            } else {
                Ok(String::from("rust-analyzer 1.0.0"))
            }
        }));
        manager.notify_open(&path, "rs", "still open", 1);
        poll_until(&mut manager, Duration::from_secs(2), |manager| manager.rust.missing());
        assert!(manager.rust_documents.contains_key(&PathKey::new(&path)));

        installed.store(true, Ordering::Release);
        manager.refresh_rust_resolution();
        let key = PathKey::new(&path);
        poll_until(&mut manager, Duration::from_secs(2), |manager| {
            manager.rust_pending_docs.contains_key(&key) || manager.rust.open_files().contains_key(&key)
        });
        poll_until(&mut manager, Duration::from_secs(2), |manager| manager.rust.open_files().contains_key(&key));
        assert_eq!(manager.rust_documents.get(&key).map(|file| file.text.as_ref()), Some("still open"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn enabling_rust_reopens_disabled_documents() {
        let (root, path) = fixture("enable-reopen");
        let (release_tx, release_rx) = mpsc::channel();
        let (started_tx, started_rx) = mpsc::channel();
        let mut manager = manager_with_runner(&root, release_rx, started_tx);
        manager.notify_open(&path, "rs", "disabled text", 1);
        assert!(started_rx.recv_timeout(Duration::from_secs(2)).is_ok());
        manager.set_rust_enabled(false);
        release_tx.send(()).unwrap();
        manager.set_rust_enabled(true);
        let key = PathKey::new(&path);
        poll_until(&mut manager, Duration::from_secs(2), |manager| manager.rust.open_files().contains_key(&key));
        assert_eq!(manager.rust.open_files().get(&key).map(|file| file.text.as_ref()), Some("disabled text"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn refresh_during_pending_job_reopens_latest_text() {
        let (root, path) = fixture("refresh-pending");
        let (release_tx, release_rx) = mpsc::channel();
        let (started_tx, started_rx) = mpsc::channel();
        let mut manager = manager_with_runner(&root, release_rx, started_tx);
        manager.notify_open(&path, "rs", "old", 1);
        assert!(started_rx.recv_timeout(Duration::from_secs(2)).is_ok());
        manager.notify_change(&path, "rs", "latest", 2);
        release_tx.send(()).unwrap();
        manager.refresh_rust_resolution();
        let key = PathKey::new(&path);
        poll_until(&mut manager, Duration::from_secs(2), |manager| {
            manager.rust_pending_docs.contains_key(&key)
                || manager.rust.open_files().get(&key).is_some_and(|file| file.version == 2)
                || manager.rust.missing()
        });
        poll_until(&mut manager, Duration::from_secs(2), |manager| {
            manager.rust.open_files().get(&key).is_some_and(|file| file.version == 2)
        });
        assert_eq!(manager.rust.open_files().get(&key).map(|file| file.text.as_ref()), Some("latest"));
        assert_eq!(manager.rust.open_files().get(&key).map(|file| file.version), Some(2));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn refresh_keeps_rust_disabled() {
        let (root, path) = fixture("disabled-refresh");
        let (release_tx, release_rx) = mpsc::channel();
        let (started_tx, started_rx) = mpsc::channel();
        let mut manager = manager_with_runner(&root, release_rx, started_tx);
        manager.notify_open(&path, "rs", "disabled text", 1);
        assert!(started_rx.recv_timeout(Duration::from_secs(2)).is_ok());
        manager.set_rust_enabled(false);
        release_tx.send(()).unwrap();
        poll_until(&mut manager, Duration::from_secs(2), |manager| manager.rust_roots.is_empty());
        manager.refresh_rust_resolution();
        assert!(!manager.rust.enabled());
        assert!(manager.rust_pending_docs.is_empty());
        assert!(manager.rust_documents.contains_key(&PathKey::new(&path)));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn workspace_configuration_uses_process_init_options() {
        let waker = crate::ui_waker::UiWaker::counting();
        let (event_tx, _event_rx) = waker.channel();
        let (out_tx, out_rx) = mpsc::channel();
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let options = super::super::rust_workspace::initialization_options("clippy");
        super::super::protocol::dispatch_frame_for_server_with_init_options(
            br#"{"jsonrpc":"2.0","id":1,"method":"workspace/configuration","params":{"items":[{"section":"rust-analyzer"}]}}"#,
            &event_tx,
            super::super::LspServerKind::RustAnalyzer,
            "rust-analyzer",
            &out_tx,
            &pending,
            Some(&options),
        );
        let response = out_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        let response: serde_json::Value = serde_json::from_slice(&response).unwrap();
        assert_eq!(response["result"][0]["check"]["command"], "clippy");
        assert_eq!(response["result"][0]["checkOnSave"], true);
    }
}
