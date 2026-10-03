use super::*;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

fn test_process_with_events(
    def: &'static LspServerDef,
) -> (LspProcess, Receiver<Cmd>, Sender<LspEvent>) {
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();
    let process = LspProcess {
        cmd_tx,
        event_rx,
        current_uri: None,
        open_uris: std::collections::HashSet::new(),
        def,
        open_file_data: None,
        stop: Arc::new(AtomicBool::new(false)),
        supervisor: None,
        local_events: Mutex::new(Vec::new()),
        event_disconnected: AtomicBool::new(false),
    };
    (process, cmd_rx, event_tx)
}

fn diagnostic(message: &str) -> Diagnostic {
    Diagnostic {
        start_line: 0,
        start_col: 0,
        end_line: 0,
        end_col: 1,
        severity: DiagSeverity::Error,
        code: None,
        code_href: None,
        message: Arc::from(message),
        source: None,
        tags: DiagTags::NONE,
        extra: None,
    }
}

#[test]
fn rust_diagnostics_follow_document_versions_and_empty_items_remove_the_entry() {
    let path = PathBuf::from("/tmp/ws/src/lib.rs");
    let root = PathBuf::from("/tmp/ws");
    let (process, _cmd_rx, event_tx) = test_process_with_events(&RUST_ANALYZER_SERVER);
    let mut manager = LspManager::new(vec![root.clone()]);
    manager.rust.open_document(
        path.clone(), Arc::from("fn main() {}"), 4, root.clone(), None,
        &crate::ui_waker::UiWaker::counting(),
    );
    manager.rust.insert_root_for_test(root.clone(), Some(process));
    manager.rust.clear_missing();
    manager.dart.insert_root_for_test(root.clone(), None);
    manager.instant_diagnostics.insert(path.clone(), (1, Arc::from([diagnostic("ruff")])));
    manager.ty_instant_diagnostics.insert(path.clone(), (2, Arc::from([diagnostic("ty")])));
    manager.dart.insert_live_diagnostics(path.clone(), rooted_language::LiveDiagnostics {
        version: 3, root: crate::platform::PathKey::new(&root), items: Arc::from([diagnostic("dart")]),
    });
    manager.diagnostics.insert(path.clone(), Arc::from([diagnostic("legacy")]));
    event_tx.send(LspEvent::Diagnostics {
        server: LspServerKind::RustAnalyzer, path: path.clone(), version: None,
        items: vec![diagnostic("current")], result_id: None,
    }).unwrap();
    manager.poll();
    assert_eq!(manager.rust.live_diagnostics()[&path].version, 4);
    let displayed = manager.diagnostic_refs_for_path(&path);
    assert_eq!(displayed.len(), 4);
    assert_eq!(displayed[3].message.as_ref(), "current");

    event_tx.send(LspEvent::Diagnostics {
        server: LspServerKind::RustAnalyzer, path: path.clone(), version: Some(3),
        items: vec![diagnostic("stale")], result_id: None,
    }).unwrap();
    manager.poll();
    assert_eq!(manager.rust.live_diagnostics()[&path].items[0].message.as_ref(), "current");

    event_tx.send(LspEvent::Diagnostics {
        server: LspServerKind::RustAnalyzer, path: path.clone(), version: Some(4),
        items: Vec::new(), result_id: None,
    }).unwrap();
    manager.poll();
    assert!(!manager.rust.live_diagnostics().contains_key(&path));
    manager.instant_diagnostics.remove(&path);
    manager.ty_instant_diagnostics.remove(&path);
    manager.dart.remove_live_diagnostics(&path);
    manager.diagnostics.remove(&path);
    assert!(!manager.diagnostic_paths().contains(&&path));
}

#[test]
fn unopened_rust_diagnostics_keep_poll_root_and_stopping_root_clears_paths() {
    let path = PathBuf::from("/home/x/.cargo/registry/src/foo-1.0/lib.rs");
    let root = PathBuf::from("/home/x/project");
    let (process, _cmd_rx, event_tx) = test_process_with_events(&RUST_ANALYZER_SERVER);
    let mut manager = LspManager::new(vec![root.clone()]);
    manager.rust.insert_root_for_test(root.clone(), Some(process));
    event_tx.send(LspEvent::Diagnostics {
        server: LspServerKind::RustAnalyzer, path: path.clone(), version: None,
        items: vec![diagnostic("dependency")], result_id: None,
    }).unwrap();
    manager.poll();
    assert_eq!(manager.rust.live_diagnostics()[&path].version, 0);
    assert_eq!(manager.rust.live_diagnostics()[&path].root, crate::platform::PathKey::new(&root));
    manager.stop_rooted_root(rooted_language::RootedLanguage::Rust, &crate::platform::PathKey::new(&root));
    manager.poll();
    assert!(!manager.rust.live_diagnostics().contains_key(&path));
    assert!(!manager.diagnostic_paths().contains(&&path));
}

#[test]
fn rooted_diagnostic_removal_updates_generation_and_summary_on_poll() {
    let path = PathBuf::from("/tmp/ws/src/lib.rs");
    let root = PathBuf::from("/tmp/ws");
    let mut manager = LspManager::new(vec![root.clone()]);
    manager.rust.insert_root_for_test(root.clone(), None);
    manager.rust.insert_live_diagnostics(path.clone(), rooted_language::LiveDiagnostics {
        version: 1, root: crate::platform::PathKey::new(&root), items: Arc::from([diagnostic("live")]),
    });
    manager.rebuild_diagnostic_summary();
    let generation = manager.diagnostic_generation();
    manager.rust.mark_missing();
    manager.poll();
    assert!(manager.diagnostic_generation() > generation);
    assert!(!manager.diagnostic_paths().contains(&&path));
}

#[test]
fn changing_rust_check_options_clears_diagnostic_summary() {
    let path = PathBuf::from("/tmp/ws/src/lib.rs");
    let root = PathBuf::from("/tmp/ws");
    let mut manager = LspManager::new(vec![root.clone()]);
    manager.rust.insert_root_for_test(root.clone(), None);
    manager.rust.insert_live_diagnostics(path.clone(), rooted_language::LiveDiagnostics {
        version: 1, root: crate::platform::PathKey::new(&root), items: Arc::from([diagnostic("live")]),
    });
    manager.rebuild_diagnostic_summary();
    let generation = manager.diagnostic_generation();
    manager.set_rust_init_options(serde_json::json!({ "checkOnSave": true, "check": { "command": "check" } }));
    manager.poll();
    assert!(manager.diagnostic_generation() > generation);
    assert!(!manager.diagnostic_paths().contains(&&path));
}

#[test]
fn stopped_root_rejects_late_live_diagnostics() {
    let root = PathBuf::from("/tmp/ws");
    let path = PathBuf::from("/tmp/ws/src/lib.rs");
    let mut state = rooted_language::RootedWorkspaces::new(rooted_language::RootedLanguage::Rust, true);
    let key = crate::platform::PathKey::new(&root);
    state.mark_root_missing(&key);
    assert!(!state.insert_live_diagnostics(path, rooted_language::LiveDiagnostics {
        version: 0, root: key, items: Arc::from([diagnostic("stale")]),
    }));
    assert!(state.live_diagnostics().is_empty());
}

#[test]
fn component_missing_resolution_job_marks_only_its_rust_root() {
    let root_a = PathBuf::from("/workspace/a");
    let root_b = PathBuf::from("/workspace/b");
    let path_a = root_a.join("src/lib.rs");
    let path_b = root_b.join("src/lib.rs");
    let (process, _cmd_rx, event_tx) = test_process_with_events(&RUST_ANALYZER_SERVER);
    let mut manager = LspManager::new(vec![root_a.clone(), root_b.clone()]);
    manager.rust.insert_root_for_test(root_a.clone(), Some(process));
    event_tx.send(LspEvent::StatusChanged {
        server: LspServerKind::RustAnalyzer, status: LspServerStatus::Running,
    }).unwrap();
    event_tx.send(LspEvent::Diagnostics {
        server: LspServerKind::RustAnalyzer, path: path_a.clone(), version: None,
        items: vec![diagnostic("root A")], result_id: None,
    }).unwrap();
    manager.poll();

    let root_b_key = crate::platform::PathKey::new(&root_b);
    let doc_b = rooted_language::OpenRootedFile {
        path: path_b.clone(), root: root_b.clone(), text: Arc::from(""), version: 1,
    };
    manager.rust_documents.insert(crate::platform::PathKey::new(&path_b), doc_b.clone());
    manager.rust_pending_docs.insert(crate::platform::PathKey::new(&path_b), doc_b);
    let (job_tx, job_rx) = manager.ui_waker.one_shot_channel();
    manager.rust_roots.insert(root_b_key, rust_workspace::RustRootEntry::Pending(
        rust_workspace::RustRootJob { generation: manager.rust_roots_generation, rx: job_rx },
    ));
    job_tx.send(rust_workspace::RustRootResolution {
        root: root_b.clone(),
        executable: Err(rust_workspace::RustToolError::ComponentMissing),
        version: None,
    }).unwrap();
    manager.poll_rust_root_jobs();
    assert_eq!(manager.rust_status(), LspServerStatus::Running);
    assert_eq!(manager.rust.live_diagnostics()[&path_a].items[0].message.as_ref(), "root A");
    assert_eq!(manager.rust_row_info().status, LspServerStatus::Running);
    assert!(manager.rust_row_info().health_message.as_deref().is_some_and(|message| message.starts_with("недоступен 1 из 2 корней")));
    assert_eq!(manager.rust.root_status_counts(), (1, 2));
    assert_eq!(manager.rust.root_status(&crate::platform::PathKey::new(&root_b)), Some(LspServerStatus::Missing));
    assert_eq!(manager.rust.root_for_open_path(&path_b), None);
}

#[test]
fn missing_root_status_is_removed_when_its_last_rust_document_closes() {
    let root = PathBuf::from("/workspace/b");
    let path = root.join("src/lib.rs");
    let root_key = crate::platform::PathKey::new(&root);
    let mut manager = LspManager::new(vec![root.clone()]);
    manager.rust_documents.insert(crate::platform::PathKey::new(&path), rooted_language::OpenRootedFile {
        path: path.clone(), root: root.clone(), text: Arc::from(""), version: 1,
    });
    manager.rust.mark_root_missing(&root_key);
    assert_eq!(manager.rust.root_status_counts(), (1, 1));
    manager.close_rust_document(&path);
    assert_eq!(manager.rust.root_status_counts(), (0, 0));
    assert!(!manager.rust.missing());
}

#[test]
fn missing_statuses_are_independent_in_either_root_order() {
    for order in [["a", "b"], ["b", "a"]] {
        let mut state = rooted_language::RootedWorkspaces::new(rooted_language::RootedLanguage::Rust, true);
        let root_a = crate::platform::PathKey::new(Path::new("/workspace/a"));
        let root_b = crate::platform::PathKey::new(Path::new("/workspace/b"));
        state.insert_root_for_test(PathBuf::from("/workspace/a"), None);
        state.insert_root_for_test(PathBuf::from("/workspace/b"), None);
        for root in order {
            state.mark_root_missing(if root == "a" { &root_a } else { &root_b });
        }
        assert_eq!(state.root_status_counts(), (2, 2));
        assert_eq!(state.status(), LspServerStatus::Missing);
    }
}

#[test]
fn close_non_last_dart_document_commits_removed_diagnostics_on_poll() {
    let root = PathBuf::from("/tmp/dart");
    let first = root.join("a.dart");
    let second = root.join("b.dart");
    let root_key = crate::platform::PathKey::new(&root);
    let mut manager = LspManager::new(vec![root.clone()]);
    manager.dart.insert_root_for_test(root.clone(), None);
    manager.dart.insert_open_file_for_test(first.clone(), root.clone());
    manager.dart.insert_open_file_for_test(second.clone(), root.clone());
    manager.dart.insert_live_diagnostics(first.clone(), rooted_language::LiveDiagnostics {
        version: 1, root: root_key, items: Arc::from([diagnostic("dart")]),
    });
    manager.rebuild_diagnostic_summary();
    let generation = manager.diagnostic_generation();
    manager.close_dart_document(&first);
    assert_eq!(manager.dart.open_files().len(), 1);
    manager.poll();
    assert!(manager.diagnostic_generation() > generation);
    assert!(manager.take_diagnostics_commit());
    assert!(!manager.diagnostic_paths().contains(&&first));
}

#[test]
fn unopened_dart_diagnostics_are_dropped_with_or_without_a_version() {
    let path = PathBuf::from("/tmp/dart/lib.dart");
    let root = PathBuf::from("/tmp/dart");
    let (process, _cmd_rx, event_tx) = test_process_with_events(&DART_SERVER);
    let mut manager = LspManager::new(vec![root.clone()]);
    manager.dart.insert_root_for_test(root, Some(process));
    for version in [None, Some(1)] {
        event_tx.send(LspEvent::Diagnostics {
            server: LspServerKind::Dart, path: path.clone(), version,
            items: vec![diagnostic("closed")], result_id: None,
        }).unwrap();
        manager.poll();
    }
    assert!(!manager.dart.live_diagnostics().contains_key(&path));
    assert!(!manager.diagnostic_paths().contains(&&path));
}
