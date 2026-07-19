use super::*;

fn test_process(def: &'static LspServerDef) -> (LspProcess, Receiver<Cmd>) {
    let (proc, cmd_rx, _event_tx) = test_process_with_events(def);
    (proc, cmd_rx)
}

fn test_process_with_events(
    def: &'static LspServerDef,
) -> (LspProcess, Receiver<Cmd>, Sender<LspEvent>) {
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();
    let proc = LspProcess {
        cmd_tx,
        event_rx,
        current_uri: None,
        def,
        open_file_data: None,
        stop: Arc::new(AtomicBool::new(false)),
        supervisor: None,
        local_events: Mutex::new(Vec::new()),
        event_disconnected: AtomicBool::new(false),
    };
    (proc, cmd_rx, event_tx)
}

fn open_cmd(rx: &Receiver<Cmd>) -> (String, &'static str, i32, Arc<str>) {
    match rx.try_recv().unwrap() {
        Cmd::Open {
            uri,
            lang,
            version,
            text,
        } => (uri, lang, version, text),
        _ => panic!("expected open command"),
    }
}

fn diag_arc(items: Vec<Diagnostic>) -> Arc<[Diagnostic]> {
    Arc::from(items)
}

fn test_diag(message: &str, severity: DiagSeverity, code: Option<&str>) -> Diagnostic {
    Diagnostic {
        start_line: 1,
        start_col: 2,
        end_line: 1,
        end_col: 8,
        severity,
        code: code.map(std::sync::Arc::<str>::from),
        code_href: None,
        message: std::sync::Arc::<str>::from(message),
        source: Some(std::sync::Arc::<str>::from("ruff")),
        quickfixes: Vec::new().into_boxed_slice(),
        tags: Vec::new().into_boxed_slice(),
    }
}

fn python_text_with_lines(lines: usize) -> String {
    let mut text = String::with_capacity(lines.saturating_mul(2).saturating_sub(1));
    for line in 0..lines {
        if line > 0 {
            text.push('\n');
        }
        text.push('x');
    }
    text
}

#[test]
fn missing_lsp_binary_is_logged_and_disabled_without_restart_loop() {
    static MISSING_SERVER: LspServerDef = LspServerDef {
        program: "rriter-definitely-missing-lsp-server",
        override_env: "RRITER_TEST_MISSING_LSP_PATH",
        args: &[],
        language_id: "python",
        extensions: &["py"],
    };
    let (_cmd_tx, cmd_rx) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();
    let started = Instant::now();

    run_supervisor(
        &MISSING_SERVER,
        Vec::new(),
        cmd_rx,
        event_tx,
        Arc::new(AtomicBool::new(false)),
    );

    assert!(started.elapsed() < Duration::from_secs(1));
    let events = event_rx.try_iter().collect::<Vec<_>>();
    assert!(events.iter().any(|event| matches!(
        event,
        LspEvent::StatusChanged {
            name,
            status: LspServerStatus::Starting,
        } if *name == MISSING_SERVER.program
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        LspEvent::StatusChanged {
            name,
            status: LspServerStatus::Missing,
        } if *name == MISSING_SERVER.program
    )));
    assert!(!events.iter().any(|event| matches!(
        event,
        LspEvent::StatusChanged {
            status: LspServerStatus::Crashed,
            ..
        }
    )));
    let logs = events
        .iter()
        .filter_map(|event| match event {
            LspEvent::Log { message, .. } => Some(message.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(logs.len(), 1);
    assert!(logs[0].contains(MISSING_SERVER.program));
    assert!(logs[0].contains("PATH"));
    assert!(logs[0].contains(MISSING_SERVER.override_env));
}

#[test]
fn lsp_restart_budget_is_bounded_and_resets_after_stable_run() {
    let mut budget = LspRestartBudget::default();
    for attempt in 1..=LSP_MAX_CONSECUTIVE_ATTEMPTS {
        assert_eq!(budget.begin_attempt(), Some(attempt));
    }
    assert_eq!(budget.begin_attempt(), None);

    budget.mark_stable();
    assert_eq!(budget.begin_attempt(), Some(1));
}

#[test]
fn unavailable_python_servers_preserve_missing_or_disabled_state_until_retry() {
    let (ruff, _ruff_rx, ruff_tx) = test_process_with_events(&RUFF_SERVER);
    let (ty, _ty_rx, ty_tx) = test_process_with_events(&TY_SERVER);
    let mut manager = LspManager::new(vec![PathBuf::from("/tmp/ws")]);
    manager.python = Some(ruff);
    manager.ty_process = Some(ty);

    ruff_tx
        .send(LspEvent::StatusChanged {
            name: RUFF_SERVER.program,
            status: LspServerStatus::Missing,
        })
        .unwrap();
    ty_tx
        .send(LspEvent::StatusChanged {
            name: TY_SERVER.program,
            status: LspServerStatus::Disabled,
        })
        .unwrap();
    manager.poll();

    assert!(manager.ruff_unavailable);
    assert!(manager.ty_unavailable);
    assert_eq!(manager.python_status, LspServerStatus::Missing);
    assert_eq!(manager.ty_status, LspServerStatus::Disabled);
    assert!(manager.python.is_none());
    assert!(manager.ty_process.is_none());

    manager.note_open_python_file(PathBuf::from("/tmp/ws/app.py"), 1);
    manager.active_workspaces = vec![PathBuf::from("/tmp/ws")];
    manager.ensure_python();

    assert!(manager.python.is_none());
    assert!(manager.ty_process.is_none());
    assert_eq!(manager.python_status, LspServerStatus::Missing);
    assert_eq!(manager.ty_status, LspServerStatus::Disabled);

    manager.ruff_workspace_diag_dirty = true;
    manager.request_ruff_workspace_diagnostics_if_ready();
    assert!(manager.ruff_workspace_diag_rx.is_none());
    assert!(!manager.ruff_workspace_diag_pending);
}

#[test]
fn lsp_position_and_log_formatting_end_to_end() {
    let text = "one\nemoji 😀\nlast";
    assert_eq!(lsp_pos_to_offset(text, 1, 6), text.find("😀").unwrap());
    assert_eq!(lsp_pos_to_offset(text, 9, 0), text.len());

    let (pretty, spans, folds) =
        format_and_highlight_json(r#"[LSP RECV] {"jsonrpc":"2.0","result":{"items":[1,2,3]}}"#);

    assert!(pretty.starts_with("[LSP RECV]\n"));
    assert!(pretty.contains("\"jsonrpc\": \"2.0\""));
    assert!(pretty.contains("\"items\": ["));
    assert!(!spans.is_empty());
    assert!(spans.iter().any(|span| span.end <= pretty.len()));
    assert!(
        folds
            .iter()
            .all(|(start, end, _)| start < end && *end <= pretty.len())
    );
}

#[test]
fn diagnostics_json_escapes_optional_fields_and_severity() {
    let diags = vec![
        test_diag("bad \"name\"\nline", DiagSeverity::Warning, Some("F401")),
        Diagnostic {
            source: None,
            severity: DiagSeverity::Hint,
            code: None,
            ..test_diag("tab\tchar", DiagSeverity::Hint, None)
        },
    ];

    let encoded = encode_diagnostics_json(&diags);
    let parsed: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(parsed[0]["severity"], 2);
    assert_eq!(parsed[0]["code"], "F401");
    assert_eq!(parsed[0]["source"], "ruff");
    assert_eq!(parsed[0]["message"], "bad \"name\"\nline");
    assert_eq!(parsed[1]["severity"], 4);
    assert!(parsed[1].get("code").is_none());
    assert!(parsed[1].get("source").is_none());
}

#[test]
fn lsp_manager_keeps_and_reopens_saved_python_file_after_disable() {
    let path = PathBuf::from("/tmp/current.py");
    let mut manager = LspManager::new(vec![PathBuf::from("/tmp")]);
    manager.current_path = Some(path.clone());
    manager.current_python_file = Some((path.clone(), Arc::from("print(2)\n"), 9));

    manager.disable_python();
    assert_eq!(manager.current_python_file.as_ref().unwrap().2, 9);
    let (ruff, ruff_rx) = test_process(&RUFF_SERVER);
    let (ty, ty_rx) = test_process(&TY_SERVER);
    manager.python = Some(ruff);
    manager.ty_process = Some(ty);
    manager.reopen_current_python_file();
    for cmd in [open_cmd(&ruff_rx), open_cmd(&ty_rx)] {
        assert_eq!(cmd.0, path_to_uri(&path));
        assert_eq!(cmd.1, "python");
        assert_eq!(cmd.2, 9);
        assert_eq!(cmd.3.as_ref(), "print(2)\n");
    }
}

#[test]
fn lsp_process_commands_update_local_state_and_send_expected_requests() {
    let path = PathBuf::from("/tmp/pkg/main.py");
    let (mut proc, rx) = test_process(&RUFF_SERVER);

    proc.notify_open(&path, Arc::from("print(1)\n"), 3, None);
    let opened = open_cmd(&rx);
    assert_eq!(opened.0, path_to_uri(&path));
    assert_eq!(opened.1, "python");
    assert_eq!(opened.2, 3);
    assert_eq!(opened.3.as_ref(), "print(1)\n");
    assert_eq!(proc.current_uri.as_deref(), Some(opened.0.as_str()));
    assert_eq!(
        proc.open_file_data.as_ref().map(|(_, text)| text.as_ref()),
        Some("print(1)\n")
    );

    proc.notify_change(&path, Arc::from("print(2)\n"), 4);
    match rx.try_recv().unwrap() {
        Cmd::Change { uri, version, text } => {
            assert_eq!(uri, path_to_uri(&path));
            assert_eq!(version, 4);
            assert_eq!(text.as_ref(), "print(2)\n");
        }
        _ => panic!("expected change command"),
    }

    let hover_id = proc.request_hover(&path, 5, 6).unwrap();
    match rx.try_recv().unwrap() {
        Cmd::Hover { id, uri, line, col } => {
            assert_eq!(id, hover_id);
            assert_eq!(uri, path_to_uri(&path));
            assert_eq!((line, col), (5, 6));
        }
        _ => panic!("expected hover command"),
    }

    let def_id = proc.request_definition(&path, 7, 8).unwrap();
    match rx.try_recv().unwrap() {
        Cmd::Definition { id, uri, line, col } => {
            assert_eq!(id, def_id);
            assert_eq!(uri, path_to_uri(&path));
            assert_eq!((line, col), (7, 8));
        }
        _ => panic!("expected definition command"),
    }

    let action_id = proc
        .request_code_actions(
            &path,
            1,
            2,
            3,
            4,
            &[test_diag("fix me", DiagSeverity::Warning, Some("F401"))],
            Some(vec!["quickfix".to_string()]),
        )
        .unwrap();
    match rx.try_recv().unwrap() {
        Cmd::CodeAction {
            id,
            uri,
            start_line,
            start_col,
            end_line,
            end_col,
            diagnostics_json,
            only,
        } => {
            assert_eq!(id, action_id);
            assert_eq!(uri, path_to_uri(&path));
            assert_eq!((start_line, start_col, end_line, end_col), (1, 2, 3, 4));
            assert!(diagnostics_json.contains("F401"));
            assert_eq!(only, Some(vec!["quickfix".to_string()]));
        }
        _ => panic!("expected code action command"),
    }

    let ws_diag_id = proc
        .request_workspace_diagnostics(
            r#"[{"uri":"file:///tmp/app.py","value":"r1"}]"#.to_string(),
        )
        .unwrap();
    match rx.try_recv().unwrap() {
        Cmd::WorkspaceDiagnostic {
            id,
            previous_result_ids_json,
        } => {
            assert_eq!(id, ws_diag_id);
            assert!(previous_result_ids_json.contains("r1"));
        }
        _ => panic!("expected workspace diagnostic command"),
    }

    proc.notify_close(&PathBuf::from("/tmp/pkg/other.py"));
    assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
    assert_eq!(proc.current_uri.as_deref(), Some(opened.0.as_str()));

    proc.notify_close(&path);
    match rx.try_recv().unwrap() {
        Cmd::Close { uri } => assert_eq!(uri, path_to_uri(&path)),
        _ => panic!("expected close command"),
    }
    assert!(proc.current_uri.is_none());

    proc.shutdown();
    assert!(matches!(rx.try_recv().unwrap(), Cmd::Shutdown));
}

#[test]
fn lsp_manager_tracks_python_reopen_state_across_open_change_and_close() {
    let ws = PathBuf::from("/tmp/ws");
    let rel = PathBuf::from("pkg/main.py");
    let abs = ws.join(&rel);
    let (ruff, ruff_rx) = test_process(&RUFF_SERVER);
    let (ty, ty_rx) = test_process(&TY_SERVER);
    let mut manager = LspManager::new(vec![ws.clone()]);
    manager.python = Some(ruff);
    manager.ty_process = Some(ty);
    manager.note_open_python_file(abs.clone(), 2);
    manager.active_workspaces = vec![ws.clone()];

    manager.notify_open(&rel, "py", "x = 1\n", 11);
    assert_eq!(manager.current_path.as_ref(), Some(&abs));
    assert_eq!(
        manager
            .current_python_file
            .as_ref()
            .map(|(p, text, v)| (p, text.as_ref(), *v)),
        Some((&abs, "x = 1\n", 11))
    );
    assert_eq!(open_cmd(&ruff_rx).2, 11);
    assert_eq!(open_cmd(&ty_rx).2, 11);

    manager.notify_change(&rel, "py", "x = 2\n", 12);
    assert_eq!(
        manager
            .current_python_file
            .as_ref()
            .map(|(_, text, v)| (text.as_ref(), *v)),
        Some(("x = 2\n", 12))
    );
    assert!(matches!(
        ruff_rx.try_recv().unwrap(),
        Cmd::Change { version: 12, .. }
    ));
    assert!(matches!(
        ty_rx.try_recv().unwrap(),
        Cmd::Change { version: 12, .. }
    ));

    manager.notify_open(&PathBuf::from("notes.txt"), "txt", "plain", 1);
    assert!(manager.current_python_file.is_none());

    manager.current_python_file = Some((abs.clone(), Arc::from("x = 3\n"), 13));
    manager.current_path = Some(abs.clone());
    manager.notify_close(&rel, "py");
    assert!(manager.current_path.is_none());
    assert!(manager.current_python_file.is_none());
    assert!(matches!(ruff_rx.try_recv().unwrap(), Cmd::Close { .. }));
    assert!(matches!(ty_rx.try_recv().unwrap(), Cmd::Close { .. }));
}

#[test]
fn lsp_manager_chooses_the_deepest_platform_aware_workspace() {
    let root = PathBuf::from("/tmp/workspace");
    let nested = root.join("packages").join("api");
    let manager = LspManager::new(vec![root.clone(), nested.clone()]);

    assert_eq!(
        manager.configured_workspace_for_path(&nested.join("src/app.py")),
        Some(&nested)
    );
    assert_eq!(
        manager.configured_workspace_for_path(&root.join("README.py")),
        Some(&root)
    );
    assert!(
        manager
            .configured_workspace_for_path(Path::new("/tmp/workspace-old/app.py"))
            .is_none()
    );
}

#[test]
fn lsp_manager_tracks_active_workspaces_from_open_python_tabs() {
    let ws_a = PathBuf::from("/tmp/ws-a");
    let ws_b = PathBuf::from("/tmp/ws-b");
    let mut manager = LspManager::new(vec![ws_a.clone(), ws_b.clone()]);
    manager.python_disabled = true;

    let a_file = ws_a.join("a.py");
    let b_file = ws_b.join("b.py");
    manager.notify_open(&a_file, "py", "a = 1\n", 1);
    assert_eq!(manager.active_workspaces, vec![ws_a.clone()]);

    manager.notify_open(&b_file, "py", "b = 1\n", 1);
    assert_eq!(manager.active_workspaces, vec![ws_a.clone(), ws_b.clone()]);

    manager.notify_close(&a_file, "py");
    assert_eq!(manager.active_workspaces, vec![ws_b.clone()]);

    manager.notify_close(&b_file, "py");
    assert!(manager.active_workspaces.is_empty());
    assert!(manager.open_python_files.is_empty());
}

#[test]
fn lsp_manager_closing_last_workspace_file_prunes_workspace_diagnostics() {
    let ws_a = PathBuf::from("/tmp/ws-a");
    let ws_b = PathBuf::from("/tmp/ws-b");
    let a_file = ws_a.join("a.py");
    let b_file = ws_b.join("b.py");
    let stale = ws_a.join("pkg/stale.py");
    let keep = ws_b.join("pkg/keep.py");
    let mut manager = LspManager::new(vec![ws_a.clone(), ws_b.clone()]);
    manager.python_disabled = true;
    manager.notify_open(&a_file, "py", "a = 1\n", 1);
    manager.notify_open(&b_file, "py", "b = 1\n", 1);
    manager.diagnostics.insert(
        stale.clone(),
        diag_arc(vec![test_diag("stale", DiagSeverity::Error, None)]),
    );
    manager.instant_diagnostics.insert(
        stale.clone(),
        (
            1,
            diag_arc(vec![test_diag(
                "stale instant",
                DiagSeverity::Error,
                None,
            )]),
        ),
    );
    manager.ty_instant_diagnostics.insert(
        stale.clone(),
        (
            1,
            diag_arc(vec![test_diag("stale ty", DiagSeverity::Error, None)]),
        ),
    );
    manager.ruff_workspace_diagnostics.insert(
        stale.clone(),
        diag_arc(vec![test_diag(
            "stale workspace ruff",
            DiagSeverity::Warning,
            Some("F401"),
        )]),
    );
    manager
        .ty_diag_result_ids
        .insert(stale.clone(), "stale-r1".to_string());
    manager.diagnostics.insert(
        keep.clone(),
        diag_arc(vec![test_diag("keep", DiagSeverity::Warning, None)]),
    );

    manager.notify_close(&a_file, "py");

    assert_eq!(manager.active_workspaces, vec![ws_b]);
    assert!(!manager.diagnostics.contains_key(&stale));
    assert!(!manager.instant_diagnostics.contains_key(&stale));
    assert!(!manager.ruff_workspace_diagnostics.contains_key(&stale));
    assert!(!manager.ty_instant_diagnostics.contains_key(&stale));
    assert!(!manager.ty_diag_result_ids.contains_key(&stale));
    assert_eq!(manager.get_diagnostics(&keep).len(), 1);
}

#[test]
fn lsp_manager_keeps_workspace_diagnostics_when_another_file_stays_open() {
    let ws = PathBuf::from("/tmp/ws");
    let a_file = ws.join("a.py");
    let b_file = ws.join("b.py");
    let workspace_diag = ws.join("pkg/other.py");
    let mut manager = LspManager::new(vec![ws.clone()]);
    manager.python_disabled = true;
    manager.notify_open(&a_file, "py", "a = 1\n", 1);
    manager.notify_open(&b_file, "py", "b = 1\n", 1);
    manager.diagnostics.insert(
        workspace_diag.clone(),
        diag_arc(vec![test_diag("keep", DiagSeverity::Warning, None)]),
    );

    manager.notify_close(&a_file, "py");

    assert_eq!(manager.active_workspaces, vec![ws]);
    assert_eq!(manager.get_diagnostics(&workspace_diag).len(), 1);
}

#[test]
fn lsp_manager_request_methods_use_existing_processes_without_spawning() {
    let path = PathBuf::from("/tmp/ws/app.py");
    let (ruff, ruff_rx) = test_process(&RUFF_SERVER);
    let (ty, ty_rx) = test_process(&TY_SERVER);
    let mut manager = LspManager::new(vec![PathBuf::from("/tmp/ws")]);
    manager.python = Some(ruff);
    manager.ty_process = Some(ty);

    let hover_id = manager.request_hover(&path, "py", 1, 2).unwrap();
    assert!(matches!(
        ty_rx.try_recv().unwrap(),
        Cmd::Hover { id, line: 1, col: 2, .. } if id == hover_id
    ));

    let def_id = manager.request_definition(&path, "py", 3, 4).unwrap();
    assert!(matches!(
        ty_rx.try_recv().unwrap(),
        Cmd::Definition { id, line: 3, col: 4, .. } if id == def_id
    ));

    let action_id = manager
        .request_code_actions(
            &path,
            "py",
            2,
            3,
            4,
            5,
            &[test_diag("diag", DiagSeverity::Info, None)],
            None,
        )
        .unwrap();
    assert!(matches!(
        ruff_rx.try_recv().unwrap(),
        Cmd::CodeAction { id, start_line: 2, start_col: 3, end_line: 4, end_col: 5, .. } if id == action_id
    ));

    let fix_id = manager.request_fix_all(&path, "py").unwrap();
    assert!(matches!(
        ruff_rx.try_recv().unwrap(),
        Cmd::CodeAction { id, only: Some(only), .. } if id == fix_id && only == vec!["source.fixAll".to_string()]
    ));

    let imports_id = manager.request_organize_imports(&path, "py").unwrap();
    assert!(matches!(
        ruff_rx.try_recv().unwrap(),
        Cmd::CodeAction { id, only: Some(only), .. } if id == imports_id && only == vec!["source.organizeImports".to_string()]
    ));

    assert!(
        manager
            .request_code_actions(&path, "txt", 0, 0, 0, 0, &[], None)
            .is_none()
    );
}

#[test]
fn lsp_manager_poll_merges_events_updates_status_and_keeps_recent_logs() {
    let path = PathBuf::from("/tmp/ws/app.py");
    let (ruff, _ruff_rx, ruff_tx) = test_process_with_events(&RUFF_SERVER);
    let (ty, _ty_rx, ty_tx) = test_process_with_events(&TY_SERVER);
    let mut manager = LspManager::new(vec![PathBuf::from("/tmp/ws")]);
    manager.python = Some(ruff);
    manager.ty_process = Some(ty);
    manager.server_logs.insert(
        RUFF_SERVER.program,
        vec![LogEntry {
            text: "old log".to_string(),
            spans: Vec::new(),
            folds: Vec::new(),
            created_at: Instant::now() - Duration::from_secs(301),
        }],
    );

    ruff_tx
        .send(LspEvent::StatusChanged {
            name: RUFF_SERVER.program,
            status: LspServerStatus::Running,
        })
        .unwrap();
    ty_tx
        .send(LspEvent::StatusChanged {
            name: TY_SERVER.program,
            status: LspServerStatus::Crashed,
        })
        .unwrap();
    ruff_tx
        .send(LspEvent::Diagnostics {
            server_name: RUFF_SERVER.program,
            path: path.clone(),
            version: Some(2),
            items: vec![test_diag("ruff", DiagSeverity::Error, Some("E1"))],
            result_id: None,
        })
        .unwrap();
    ty_tx
        .send(LspEvent::Diagnostics {
            server_name: TY_SERVER.program,
            path: path.clone(),
            version: Some(5),
            items: vec![test_diag("ty", DiagSeverity::Warning, None)],
            result_id: Some("ty-r1".to_string()),
        })
        .unwrap();
    for i in 0..32 {
        ruff_tx
            .send(LspEvent::Log {
                name: RUFF_SERVER.program,
                message: format!("{{\"idx\":{i}}}"),
            })
            .unwrap();
    }

    let events = manager.poll();
    assert_eq!(events.len(), 36);
    assert_eq!(manager.python_status, LspServerStatus::Running);
    assert_eq!(manager.ty_status, LspServerStatus::Crashed);
    assert!(!manager.dirty_diagnostics);

    let diags = manager.diagnostic_refs_for_path(&path);
    assert_eq!(diags.len(), 2);
    assert_eq!(diags[0].message.as_ref(), "ruff");
    assert_eq!(diags[1].message.as_ref(), "ty");
    let (version, instant) = manager.instant_merged_diagnostics(&path);
    assert_eq!(version, 5);
    assert_eq!(instant.len(), 2);
    assert_eq!(
        manager.ty_diag_result_ids.get(&path).map(String::as_str),
        Some("ty-r1")
    );
    assert!(manager.has_stale_instant_diagnostics(&path, 5));
    assert!(!manager.has_stale_instant_diagnostics(&path, 2));
    assert_eq!(manager.diagnostics_for_line(&path, 1).len(), 2);
    assert!(manager.diagnostics_for_line(&path, 99).is_empty());

    let logs = &manager.server_logs[RUFF_SERVER.program];
    assert_eq!(logs.len(), 32);
    assert!(!logs.iter().any(|log| log.text.contains("old log")));
    assert!(logs[0].text.contains("\"idx\":0"));
    assert!(logs[31].text.contains("\"idx\":31"));
    assert!(logs[31].text.is_char_boundary(logs[31].text.len()));

    let info = manager.servers_info();
    assert_eq!(info[0].name, RUFF_SERVER.program);
    assert_eq!(info[0].status, LspServerStatus::Running);
    assert_eq!(info[0].logs.len(), 32);
    assert_eq!(info[1].status, LspServerStatus::Crashed);
}

#[test]
fn diagnostics_merge_combines_servers_and_tracks_max_version() {
    let path = PathBuf::from("/tmp/main.py");
    let only_ty = PathBuf::from("/tmp/ty_only.py");
    let mut ruff = HashMap::new();
    let mut ty = HashMap::new();
    ruff.insert(
        path.clone(),
        (
            3,
            diag_arc(vec![test_diag("ruff", DiagSeverity::Error, Some("E1"))]),
        ),
    );
    ty.insert(
        path.clone(),
        (7, diag_arc(vec![test_diag("ty", DiagSeverity::Info, None)])),
    );
    ty.insert(
        only_ty.clone(),
        (
            2,
            diag_arc(vec![test_diag("ty only", DiagSeverity::Warning, None)]),
        ),
    );

    let (version, merged) = merged_diagnostics_for_path(&path, &ruff, &ty);
    assert_eq!(version, 7);
    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0].message.as_ref(), "ruff");
    assert_eq!(merged[1].message.as_ref(), "ty");

    let all = merged_diagnostics_for_all_paths(&ruff, &ty);
    assert_eq!(all.len(), 2);
    let ty_only = all.iter().find(|(p, _)| p == &only_ty).unwrap();
    assert_eq!(ty_only.1.len(), 1);
    assert_eq!(ty_only.1[0].message.as_ref(), "ty only");
}

#[test]
fn instant_merged_diagnostics_lazily_combines_servers_and_handles_empty() {
    let path = PathBuf::from("/tmp/main.py");
    let mut manager = LspManager::new(vec![PathBuf::from("/tmp")]);

    let (empty_version, empty) = manager.instant_merged_diagnostics(&path);
    assert_eq!(empty_version, 0);
    assert!(empty.is_empty());

    manager.instant_diagnostics.insert(
        path.clone(),
        (
            3,
            diag_arc(vec![test_diag("ruff", DiagSeverity::Error, Some("E1"))]),
        ),
    );
    manager.ty_instant_diagnostics.insert(
        path.clone(),
        (7, diag_arc(vec![test_diag("ty", DiagSeverity::Info, None)])),
    );

    let (version, merged) = manager.instant_merged_diagnostics(&path);
    assert_eq!(version, 7);
    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0].message.as_ref(), "ruff");
    assert_eq!(merged[1].message.as_ref(), "ty");
    assert!(manager.get_diagnostics(&path).is_empty());
    assert_eq!(manager.diagnostic_count(&path), 2);
    assert_eq!(manager.diagnostic_counts_for_path(&path), (1, 0));
    assert_eq!(
        manager.diagnostic_at(&path, 1).map(|diag| diag.message.as_ref()),
        Some("ty")
    );
    assert!(manager.diagnostic_paths().iter().any(|p| *p == &path));
}

#[test]
fn workspace_ruff_diagnostics_cover_closed_files_without_overriding_open_buffers() {
    let ws = PathBuf::from("/tmp/ws");
    let open_path = ws.join("pkg/open.py");
    let closed_path = ws.join("pkg/closed.py");
    let mut manager = LspManager::new(vec![ws.clone()]);
    manager.note_open_python_file(open_path.clone(), 3);
    manager.active_workspaces = vec![ws];
    manager.ruff_workspace_diagnostics.insert(
        open_path.clone(),
        diag_arc(vec![test_diag(
            "stale disk ruff",
            DiagSeverity::Warning,
            Some("F401"),
        )]),
    );
    manager.ruff_workspace_diagnostics.insert(
        closed_path.clone(),
        diag_arc(vec![test_diag(
            "closed file ruff",
            DiagSeverity::Warning,
            Some("F401"),
        )]),
    );
    manager.ty_instant_diagnostics.insert(
        closed_path.clone(),
        (
            2,
            diag_arc(vec![test_diag("closed file ty", DiagSeverity::Error, None)]),
        ),
    );
    manager.rebuild_merged_diagnostic_indices();

    assert_eq!(manager.diagnostic_count(&open_path), 0);
    assert_eq!(
        manager
            .instant_merged_diagnostics(&open_path)
            .1
            .iter()
            .map(|diag| diag.message.as_ref())
            .collect::<Vec<_>>(),
        Vec::<&str>::new()
    );

    let closed = manager.diagnostic_refs_for_path(&closed_path);
    assert_eq!(closed.len(), 2);
    assert_eq!(closed[0].message.as_ref(), "closed file ruff");
    assert_eq!(closed[1].message.as_ref(), "closed file ty");
    assert_eq!(
        manager.diagnostic_severity_under_path(Path::new("/tmp/ws/pkg")),
        Some(DiagSeverity::Error)
    );
    assert_eq!(manager.total_diagnostic_counts(), (1, 1));
    assert!(manager
        .diagnostic_paths()
        .iter()
        .any(|path| *path == &closed_path));

    manager.instant_diagnostics.insert(
        open_path.clone(),
        (
            4,
            diag_arc(vec![test_diag(
                "live buffer ruff",
                DiagSeverity::Warning,
                Some("F401"),
            )]),
        ),
    );
    manager.dirty_diagnostics = true;

    assert_eq!(manager.diagnostic_count(&open_path), 1);
    assert_eq!(
        manager
            .diagnostic_at(&open_path, 0)
            .map(|diag| diag.message.as_ref()),
        Some("live buffer ruff")
    );
    assert_eq!(manager.ruff_diagnostic_storage_counts(), (2, 3));
}

#[test]
fn diagnostic_accessors_use_live_instant_store_while_index_is_dirty() {
    let path = PathBuf::from("/tmp/ws/pkg/current.py");
    let offscreen = PathBuf::from("/tmp/ws/pkg/offscreen.py");
    let mut manager = LspManager::new(vec![PathBuf::from("/tmp/ws")]);
    manager.instant_diagnostics.insert(
        path.clone(),
        (
            1,
            diag_arc(vec![test_diag("old", DiagSeverity::Warning, Some("W1"))]),
        ),
    );
    manager.rebuild_merged_diagnostic_indices();
    assert_eq!(manager.diagnostic_count(&path), 1);

    manager.instant_diagnostics.insert(
        path.clone(),
        (
            2,
            diag_arc(vec![
                test_diag("new warning", DiagSeverity::Warning, Some("W2")),
                test_diag("new error", DiagSeverity::Error, Some("E1")),
            ]),
        ),
    );
    manager.ty_instant_diagnostics.insert(
        offscreen.clone(),
        (
            3,
            diag_arc(vec![test_diag("workspace", DiagSeverity::Error, Some("T1"))]),
        ),
    );
    manager.dirty_diagnostics = true;

    assert_eq!(manager.diagnostic_count(&path), 2);
    assert_eq!(
        manager.diagnostic_at(&path, 1).map(|diag| diag.message.as_ref()),
        Some("new error")
    );
    assert_eq!(manager.diagnostic_counts_for_path(&path), (1, 1));
    assert_eq!(
        manager.diagnostic_severity_under_path(Path::new("/tmp/ws/pkg")),
        Some(DiagSeverity::Error)
    );
    assert_eq!(
        manager.diagnostic_severity_under_path(Path::new("/tmp/ws/pkg/current.py")),
        Some(DiagSeverity::Error)
    );
    assert!(manager
        .diagnostic_paths()
        .iter()
        .any(|path| *path == &offscreen));
}

#[test]
fn diagnostic_ancestor_severity_cache_rebuilds_and_clears() {
    let warning_path = PathBuf::from("/tmp/ws/pkg/warning.py");
    let error_path = PathBuf::from("/tmp/ws/pkg/nested/error.py");
    let mut manager = LspManager::new(vec![PathBuf::from("/tmp/ws")]);
    manager.instant_diagnostics.insert(
        warning_path.clone(),
        (
            1,
            diag_arc(vec![test_diag("warning", DiagSeverity::Warning, Some("W1"))]),
        ),
    );
    manager.ty_instant_diagnostics.insert(
        error_path.clone(),
        (
            1,
            diag_arc(vec![test_diag("error", DiagSeverity::Error, Some("E1"))]),
        ),
    );
    manager.rebuild_merged_diagnostic_indices();

    assert_eq!(
        manager.diagnostic_severity_under_path(Path::new("/tmp/ws/pkg")),
        Some(DiagSeverity::Error)
    );
    assert_eq!(
        manager.diagnostic_severity_under_path(&warning_path),
        Some(DiagSeverity::Warning)
    );

    manager.clear_diagnostics_for_path(&error_path);
    assert_eq!(
        manager.diagnostic_severity_under_path(Path::new("/tmp/ws/pkg")),
        Some(DiagSeverity::Warning)
    );
    assert_eq!(manager.total_diagnostic_counts(), (0, 1));
    manager.clear_diagnostics_for_path(&warning_path);
    assert_eq!(
        manager.diagnostic_severity_under_path(Path::new("/tmp/ws/pkg")),
        None
    );
    assert_eq!(manager.total_diagnostic_counts(), (0, 0));
}

#[test]
fn manager_poll_moves_diagnostic_items_into_instant_store() {
    let path = PathBuf::from("/tmp/ws/app.py");
    let (ruff, _ruff_rx, ruff_tx) = test_process_with_events(&RUFF_SERVER);
    let mut manager = LspManager::new(vec![PathBuf::from("/tmp/ws")]);
    manager.python = Some(ruff);

    ruff_tx
        .send(LspEvent::Diagnostics {
            server_name: RUFF_SERVER.program,
            path: path.clone(),
            version: Some(4),
            items: vec![
                test_diag("moved", DiagSeverity::Error, Some("E1")),
                test_diag("warn", DiagSeverity::Warning, Some("W1")),
            ],
            result_id: None,
        })
        .unwrap();

    let events = manager.poll();
    match &events[0] {
        LspEvent::Diagnostics { items, .. } => assert!(items.is_empty()),
        other => panic!("unexpected event: {other:?}"),
    }

    let (version, instant) = manager.instant_merged_diagnostics(&path);
    assert_eq!(version, 4);
    assert_eq!(instant.len(), 2);
    assert_eq!(instant[0].message.as_ref(), "moved");
    assert_eq!(manager.total_diagnostic_counts(), (1, 1));
}

#[test]
fn send_and_log_removes_text_document_payloads_but_sends_original_body() {
    let (out_tx, out_rx) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();

    let body = make_did_open(
        "file:///tmp/main.py",
        "python",
        1,
        "secret payload that must not be logged",
    );
    send_and_log(&out_tx, &event_tx, RUFF_SERVER.program, body.clone()).unwrap();

    assert_eq!(out_rx.try_recv().unwrap(), body);
    match event_rx.try_recv().unwrap() {
        LspEvent::Log { name, message } => {
            assert_eq!(name, RUFF_SERVER.program);
            assert!(message.contains("[LSP SEND]"));
            assert!(!message.contains("\"text\""));
            assert!(!message.contains("secret payload"));
        }
        other => panic!("unexpected event: {other:?}"),
    }

    let hover = make_hover(77, "file:///tmp/main.py", 2, 3);
    send_and_log(&out_tx, &event_tx, TY_SERVER.program, hover.clone()).unwrap();

    assert_eq!(out_rx.try_recv().unwrap(), hover);
    match event_rx.try_recv().unwrap() {
        LspEvent::Log { name, message } => {
            assert_eq!(name, TY_SERVER.program);
            assert!(message.contains("textDocument/hover"));
            assert!(!message.contains("<TRUNCATED>"));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn send_and_log_removes_only_sent_text_fields_and_keeps_json_shape() {
    let (out_tx, out_rx) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();

    let body = make_did_change_full(
        "file:///tmp/main.py",
        17,
        "secret payload\nwith many lines\nthat must not be logged",
    );
    send_and_log(&out_tx, &event_tx, RUFF_SERVER.program, body.clone()).unwrap();

    assert_eq!(out_rx.try_recv().unwrap(), body);
    match event_rx.try_recv().unwrap() {
        LspEvent::Log { name, message } => {
            assert_eq!(name, RUFF_SERVER.program);
            assert!(message.starts_with("[LSP SEND] "));
            assert!(!message.contains("secret payload"));
            assert!(!message.contains("\"text\""));

            let json = message.trim_start_matches("[LSP SEND] ");
            let parsed: serde_json::Value = serde_json::from_str(json).unwrap();
            assert_eq!(parsed["method"], "textDocument/didChange");
            assert_eq!(
                parsed["params"]["textDocument"]["uri"],
                "file:///tmp/main.py"
            );
            assert_eq!(parsed["params"]["textDocument"]["version"], 17);
            assert!(parsed["params"]["contentChanges"][0].get("text").is_none());
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn lsp_process_poll_drains_events_and_shutdown_sends_command() {
    let (proc, rx, tx) = test_process_with_events(&RUFF_SERVER);
    tx.send(LspEvent::ServerReady).unwrap();
    tx.send(LspEvent::Log {
        name: RUFF_SERVER.program,
        message: "ready".to_string(),
    })
    .unwrap();

    let mut events = Vec::new();
    proc.poll(&mut events);
    assert_eq!(events.len(), 2);

    proc.poll(&mut events);
    assert_eq!(events.len(), 2);

    proc.shutdown();
    assert!(matches!(rx.try_recv().unwrap(), Cmd::Shutdown));
}

#[test]
fn manager_suppresses_diagnostics_then_flushes_after_delay() {
    let path = PathBuf::from("/tmp/ws/app.py");
    let (ruff, _ruff_rx, ruff_tx) = test_process_with_events(&RUFF_SERVER);
    let mut manager = LspManager::new(vec![PathBuf::from("/tmp/ws")]);
    manager.python = Some(ruff);
    manager.suppress_diagnostics = true;

    ruff_tx
        .send(LspEvent::Diagnostics {
            server_name: RUFF_SERVER.program,
            path: path.clone(),
            version: Some(1),
            items: vec![test_diag("suppressed", DiagSeverity::Error, None)],
            result_id: None,
        })
        .unwrap();

    let events = manager.poll();
    assert_eq!(events.len(), 1);
    assert!(manager.get_diagnostics(&path).is_empty());
    assert!(
        manager
            .instant_merged_diagnostics(&path)
            .1
            .is_empty()
    );
    assert!(!manager.dirty_diagnostics);

    manager.suppress_diagnostics = false;
    manager.last_change = Some(std::time::Instant::now() - Duration::from_secs(4));
    ruff_tx
        .send(LspEvent::Diagnostics {
            server_name: RUFF_SERVER.program,
            path: path.clone(),
            version: Some(2),
            items: vec![test_diag("flushed", DiagSeverity::Warning, Some("W1"))],
            result_id: None,
        })
        .unwrap();

    let events = manager.poll();
    assert_eq!(events.len(), 1);
    assert_eq!(manager.diagnostic_count(&path), 1);
    assert_eq!(
        manager.diagnostic_at(&path, 0).map(|diag| diag.message.as_ref()),
        Some("flushed")
    );
    assert_eq!(manager.instant_merged_diagnostics(&path).0, 2);
    assert!(!manager.dirty_diagnostics);
    assert!(manager.last_change.is_none());
}

#[test]
fn manager_requests_ty_workspace_diagnostics_after_config_and_reuses_result_ids() {
    let path = PathBuf::from("/tmp/ws/pkg/offscreen.py");
    let (ty, ty_rx, ty_tx) = test_process_with_events(&TY_SERVER);
    let mut manager = LspManager::new(vec![PathBuf::from("/tmp/ws")]);
    manager.note_open_python_file(PathBuf::from("/tmp/ws/current.py"), 1);
    manager.active_workspaces = vec![PathBuf::from("/tmp/ws")];
    manager.ty_process = Some(ty);
    manager.ty_status = LspServerStatus::Running;

    ty_tx
        .send(LspEvent::ConfigurationServed {
            name: TY_SERVER.program,
        })
        .unwrap();

    let events = manager.poll();
    assert_eq!(events.len(), 1);
    let request_id = match ty_rx.try_recv().unwrap() {
        Cmd::WorkspaceDiagnostic {
            id,
            previous_result_ids_json,
        } => {
            assert_eq!(previous_result_ids_json, "[]");
            id
        }
        _ => panic!("expected workspace diagnostic command"),
    };

    ty_tx
        .send(LspEvent::Diagnostics {
            server_name: TY_SERVER.program,
            path: path.clone(),
            version: None,
            items: vec![test_diag("offscreen", DiagSeverity::Error, Some("T1"))],
            result_id: Some("next-r1".to_string()),
        })
        .unwrap();
    ty_tx
        .send(LspEvent::WorkspaceDiagnosticsDone { request_id })
        .unwrap();

    let events = manager.poll();
    assert_eq!(events.len(), 2);
    assert_eq!(
        manager.diagnostic_at(&path, 0).map(|diag| diag.message.as_ref()),
        Some("offscreen")
    );
    assert!(manager.ty_workspace_diag_pending.is_none());

    manager.ty_workspace_diag_dirty = true;
    let events = manager.poll();
    assert!(events.is_empty());
    match ty_rx.try_recv().unwrap() {
        Cmd::WorkspaceDiagnostic {
            previous_result_ids_json,
            ..
        } => {
            assert!(previous_result_ids_json.contains(&path_to_uri(&path)));
            assert!(previous_result_ids_json.contains("next-r1"));
        }
        _ => panic!("expected second workspace diagnostic command"),
    }
}

#[test]
fn manager_requests_ty_workspace_diagnostics_for_large_current_python_file() {
    let path = PathBuf::from("/tmp/ws/huge.py");
    let (ty, ty_rx, _ty_event_tx) = test_process_with_events(&TY_SERVER);
    let mut manager = LspManager::new(vec![PathBuf::from("/tmp/ws")]);
    let large_line_count = 50_000;
    let text = python_text_with_lines(large_line_count);
    manager.python_disabled = true;
    manager.notify_open(&path, "py", &text, 1);
    assert_eq!(manager.current_python_lines, Some(large_line_count));

    manager.python_disabled = false;
    manager.ty_process = Some(ty);
    manager.ty_status = LspServerStatus::Running;
    manager.ty_workspace_diag_dirty = true;

    let events = manager.poll();
    assert!(events.is_empty());
    match ty_rx.try_recv().unwrap() {
        Cmd::WorkspaceDiagnostic {
            previous_result_ids_json,
            ..
        } => assert_eq!(previous_result_ids_json, "[]"),
        _ => panic!("unexpected command"),
    }
    assert!(manager.ty_workspace_diag_pending.is_some());
    assert!(!manager.ty_workspace_diag_dirty);
}

#[test]
fn manager_skips_ty_workspace_diagnostics_without_active_workspace_root() {
    let path = PathBuf::from("/tmp/external.py");
    let (ty, ty_rx, _ty_event_tx) = test_process_with_events(&TY_SERVER);
    let mut manager = LspManager::new(vec![PathBuf::from("/tmp/ws")]);
    manager.python_disabled = true;
    manager.notify_open(&path, "py", "x = 1\n", 1);
    assert!(manager.active_workspaces.is_empty());

    manager.python_disabled = false;
    manager.ty_process = Some(ty);
    manager.ty_status = LspServerStatus::Running;
    manager.ty_workspace_diag_dirty = true;

    let events = manager.poll();
    assert!(events.is_empty());
    assert!(manager.ty_workspace_diag_pending.is_none());
    assert!(manager.ty_workspace_diag_dirty);
    assert!(matches!(ty_rx.try_recv(), Err(TryRecvError::Empty)));
}

#[test]
fn format_json_handles_send_prefix_invalid_json_plain_text_and_offsets() {
    let (send_text, send_spans, send_folds) = format_and_highlight_json("[LSP SEND] not json");
    assert_eq!(send_text, "[LSP SEND]\nnot json");
    assert!(
        send_spans
            .iter()
            .any(|span| span.end == "[LSP SEND]\n".len())
    );
    assert!(send_folds.is_empty());

    let (invalid_text, invalid_spans, invalid_folds) =
        format_and_highlight_json("[LSP RECV] {not-json");
    assert_eq!(invalid_text, "[LSP RECV]\n{not-json");
    assert!(!invalid_spans.is_empty());
    assert!(invalid_folds.is_empty());

    let text = "a😀\nline";
    assert_eq!(lsp_pos_to_offset(text, 0, 0), 0);
    assert_eq!(lsp_pos_to_offset(text, 0, 1), 1);
    assert_eq!(lsp_pos_to_offset(text, 0, 2), "a😀".len());
    assert_eq!(lsp_pos_to_offset(text, 1, 99), text.len());
    assert_eq!(lsp_pos_to_offset(text, 9, 0), text.len());
}

#[test]
fn lsp_manager_disable_shutdown_and_non_python_paths_are_state_safe() {
    let path = PathBuf::from("/tmp/ws/app.py");
    let (ruff, ruff_rx) = test_process(&RUFF_SERVER);
    let (ty, ty_rx) = test_process(&TY_SERVER);
    let mut manager = LspManager::new(vec![PathBuf::from("/tmp/ws")]);
    manager.python = Some(ruff);
    manager.ty_process = Some(ty);
    manager.python_status = LspServerStatus::Running;
    manager.ty_status = LspServerStatus::Running;
    manager.diagnostics.insert(
        path.clone(),
        diag_arc(vec![test_diag("stale", DiagSeverity::Error, None)]),
    );
    manager.instant_diagnostics.insert(
        path.clone(),
        (
            1,
            diag_arc(vec![test_diag("instant", DiagSeverity::Warning, None)]),
        ),
    );
    manager.ty_instant_diagnostics.insert(
        path.clone(),
        (2, diag_arc(vec![test_diag("ty", DiagSeverity::Info, None)])),
    );
    manager.ruff_workspace_diagnostics.insert(
        path.clone(),
        diag_arc(vec![test_diag(
            "workspace ruff",
            DiagSeverity::Warning,
            Some("F401"),
        )]),
    );
    manager.dirty_diagnostics = true;
    manager.server_logs.insert(
        RUFF_SERVER.program,
        vec![LogEntry {
            text: "log".to_string(),
            spans: Vec::new(),
            folds: Vec::new(),
            created_at: Instant::now(),
        }],
    );

    manager.disable_python();
    assert!(manager.python_disabled);
    assert_eq!(manager.python_status, LspServerStatus::Disabled);
    assert_eq!(manager.ty_status, LspServerStatus::Disabled);
    assert!(manager.python.is_none());
    assert!(manager.ty_process.is_none());
    assert!(manager.diagnostics.is_empty());
    assert!(manager.instant_diagnostics.is_empty());
    assert!(manager.ruff_workspace_diagnostics.is_empty());
    assert!(manager.ty_instant_diagnostics.is_empty());
    assert!(!manager.dirty_diagnostics);
    assert!(manager.server_logs.is_empty());
    assert!(matches!(ruff_rx.try_recv().unwrap(), Cmd::Shutdown));
    assert!(matches!(ty_rx.try_recv().unwrap(), Cmd::Shutdown));

    let mut manager = LspManager::new(vec![PathBuf::from("/tmp/ws")]);
    manager.notify_open(&PathBuf::from("notes.txt"), "txt", "plain", 1);
    assert_eq!(
        manager.current_path,
        Some(PathBuf::from("/tmp/ws/notes.txt"))
    );
    assert!(manager.current_python_file.is_none());
    assert!(manager.request_hover(&path, "txt", 0, 0).is_none());
    assert!(manager.request_definition(&path, "txt", 0, 0).is_none());
    assert!(manager.request_fix_all(&path, "txt").is_none());
    assert!(manager.request_organize_imports(&path, "txt").is_none());
    manager.notify_change(&PathBuf::from("notes.txt"), "txt", "changed", 2);
    assert!(manager.current_python_file.is_none());
    assert!(manager.last_change.is_some());
    manager.notify_close(&PathBuf::from("notes.txt"), "txt");
    assert_eq!(
        manager.current_path,
        Some(PathBuf::from("/tmp/ws/notes.txt"))
    );

    let manager = LspManager::new(vec![PathBuf::from("/tmp/ws")]);
    manager.shutdown();
}

#[test]
fn clear_diagnostics_for_path_removes_abs_and_relative_entries() {
    let path = PathBuf::from("/tmp/ws/app.py");
    let mut manager = LspManager::new(vec![PathBuf::from("/tmp/ws")]);
    manager.diagnostics.insert(
        path.clone(),
        diag_arc(vec![test_diag("stale", DiagSeverity::Error, None)]),
    );
    manager.instant_diagnostics.insert(
        path.clone(),
        (
            1,
            diag_arc(vec![test_diag("instant", DiagSeverity::Warning, None)]),
        ),
    );
    manager.ty_instant_diagnostics.insert(
        path.clone(),
        (2, diag_arc(vec![test_diag("ty", DiagSeverity::Info, None)])),
    );
    manager.ruff_workspace_diagnostics.insert(
        path.clone(),
        diag_arc(vec![test_diag(
            "workspace ruff",
            DiagSeverity::Warning,
            Some("F401"),
        )]),
    );
    manager.dirty_diagnostics = true;

    manager.clear_diagnostics_for_path(&PathBuf::from("app.py"));

    assert!(manager.get_diagnostics(&path).is_empty());
    assert!(!manager.ruff_workspace_diagnostics.contains_key(&path));
    let (version, instant) = manager.instant_merged_diagnostics(&path);
    assert_eq!(version, 0);
    assert!(instant.is_empty());
    assert!(!manager.dirty_diagnostics);
}

#[test]
fn lsp_format_json_pretty_prints_spans_and_folds_multiline_payloads() {
    let raw =
        r#"[LSP RECV] {"outer":{"inner":[1,true,"s",{"deep":false}]},"arr":[{"x":1},{"y":2}]}"#;
    let (text, spans, folds) = format_and_highlight_json(raw);

    assert!(text.starts_with("[LSP RECV]\n{"));
    assert!(text.contains("\"outer\""));
    assert!(text.contains("\"deep\": false"));
    assert!(spans.iter().any(|s| s.color == [0.313, 0.980, 0.482, 1.0]));
    assert!(spans.iter().any(|s| s.color == [0.545, 0.913, 0.992, 1.0]));
    assert!(spans.iter().any(|s| s.color == [0.945, 0.980, 0.549, 1.0]));
    assert!(spans.iter().any(|s| s.color == [0.741, 0.576, 0.976, 1.0]));
    assert!(spans.iter().any(|s| s.color == [1.0, 0.474, 0.776, 1.0]));
    assert!(folds.iter().any(|(start, end, _)| *start < *end));
    assert!(folds.iter().any(|(_, _, depth)| *depth == 1));
    assert!(folds.iter().any(|(_, _, depth)| *depth == 2));
}

#[test]
fn r3_090_did_open_send_failure_does_not_mark_document_open() {
    let (mut process, command_rx) = test_process(&RUFF_SERVER);
    drop(command_rx);
    let path = PathBuf::from("/tmp/r3-open.py");
    assert!(!process.notify_open(&path, Arc::from("x = 1\n"), 1, None));
    assert!(process.current_uri.is_none());
    assert!(process.open_file_data.is_none());
}

#[test]
fn r3_091_did_change_send_failure_does_not_advance_current_uri() {
    let (mut process, command_rx) = test_process(&RUFF_SERVER);
    drop(command_rx);
    let path = PathBuf::from("/tmp/r3-change.py");
    assert!(!process.notify_change(&path, Arc::from("x = 2\n"), 2));
    assert!(process.current_uri.is_none());
}

#[test]
fn r3_092_request_send_failure_returns_no_pending_id() {
    let (mut process, command_rx) = test_process(&RUFF_SERVER);
    drop(command_rx);
    let path = PathBuf::from("/tmp/r3-hover.py");
    assert!(process.request_hover(&path, 0, 0).is_none());
    assert!(process.request_definition(&path, 0, 0).is_none());
    assert!(process.request_completion(&path, 0, 0, None).is_none());
}

#[test]
fn r3_093_event_channel_disconnect_emits_disabled_once() {
    let (process, _command_rx, event_tx) = test_process_with_events(&RUFF_SERVER);
    drop(event_tx);
    let mut events = Vec::new();
    process.poll(&mut events);
    assert!(events.iter().any(|event| matches!(
        event,
        LspEvent::StatusChanged { status: LspServerStatus::Disabled, .. }
    )));
    let first_len = events.len();
    process.poll(&mut events);
    assert_eq!(events.len(), first_len);
}

#[test]
fn r3_094_request_id_allocator_cycles_without_zero_or_negative_ids() {
    let counter = AtomicI32::new(i32::MAX - 1);
    assert_eq!(allocate_request_id(&counter), Some(i32::MAX - 1));
    assert_eq!(allocate_request_id(&counter), Some(i32::MAX));
    assert_eq!(allocate_request_id(&counter), Some(1));
    counter.store(0, Ordering::Relaxed);
    assert_eq!(allocate_request_id(&counter), Some(1));
}

#[test]
fn r3_095_ruff_workspace_disconnect_clears_stale_diagnostics() {
    let mut manager = LspManager::new(vec![PathBuf::from("/tmp")]);
    manager.ruff_workspace_diagnostics.insert(
        PathBuf::from("/tmp/stale.py"),
        diag_arc(vec![test_diag("stale", DiagSeverity::Warning, None)]),
    );
    let (tx, rx) = mpsc::channel();
    manager.ruff_workspace_diag_rx = Some(rx);
    manager.ruff_workspace_diag_pending = true;
    drop(tx);
    manager.poll_ruff_workspace_diagnostics();
    assert!(manager.ruff_workspace_diagnostics.is_empty());
    assert!(!manager.ruff_workspace_diag_pending);
}

#[test]
fn r3_096_lsp_drop_reaps_unfinished_supervisor_without_blocking() {
    let (cmd_tx, _cmd_rx) = mpsc::channel();
    let (_event_tx, event_rx) = mpsc::channel();
    let supervisor = std::thread::spawn(|| std::thread::sleep(Duration::from_millis(120)));
    let process = LspProcess {
        cmd_tx,
        event_rx,
        current_uri: None,
        def: &RUFF_SERVER,
        open_file_data: None,
        stop: Arc::new(AtomicBool::new(false)),
        supervisor: Some(supervisor),
        local_events: Mutex::new(Vec::new()),
        event_disconnected: AtomicBool::new(false),
    };
    let started = Instant::now();
    drop(process);
    assert!(started.elapsed() < Duration::from_millis(80));
}


#[test]
fn lsp_position_clamps_oversized_column_to_requested_line_end() {
    let text = "a😀\nline";
    assert_eq!(lsp_pos_to_offset(text, 0, 99), "a😀".len());
    assert_eq!(lsp_pos_to_offset(text, 1, 99), text.len());
}

#[test]
fn lsp_frame_reader_accepts_reordered_extra_headers() {
    let body = br#"{"jsonrpc":"2.0","id":1}"#;
    let mut frame = format!(
        "Content-Type: application/vscode-jsonrpc; charset=utf-8\r\ncontent-length: {}\r\nX-Test: yes\r\n\r\n",
        body.len()
    )
    .into_bytes();
    frame.extend_from_slice(body);
    let mut reader = std::io::Cursor::new(frame);
    let mut header = String::new();

    assert_eq!(read_lsp_frame(&mut reader, &mut header).unwrap(), Some(body.to_vec()));
    assert_eq!(read_lsp_frame(&mut reader, &mut header).unwrap(), None);
}

#[test]
fn lsp_frame_reader_rejects_oversized_body_before_allocation() {
    let frame = format!("Content-Length: {}\r\n\r\n", LSP_MAX_FRAME_BYTES + 1);
    let mut reader = std::io::Cursor::new(frame.into_bytes());
    let mut header = String::new();

    let error = read_lsp_frame(&mut reader, &mut header).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn open_file_diagnostics_do_not_regress_to_older_or_versionless_results() {
    let path = PathBuf::from("/tmp/ws/app.py");
    let (ruff, _cmd_rx, event_tx) = test_process_with_events(&RUFF_SERVER);
    let mut manager = LspManager::new(vec![PathBuf::from("/tmp/ws")]);
    manager.note_open_python_file(path.clone(), 1);
    manager.python = Some(ruff);

    event_tx
        .send(LspEvent::Diagnostics {
            server_name: RUFF_SERVER.program,
            path: path.clone(),
            version: Some(5),
            items: vec![test_diag("new", DiagSeverity::Error, None)],
            result_id: None,
        })
        .unwrap();
    manager.poll();

    event_tx
        .send(LspEvent::Diagnostics {
            server_name: RUFF_SERVER.program,
            path: path.clone(),
            version: Some(3),
            items: vec![test_diag("old", DiagSeverity::Warning, None)],
            result_id: None,
        })
        .unwrap();
    manager.poll();

    event_tx
        .send(LspEvent::Diagnostics {
            server_name: RUFF_SERVER.program,
            path: path.clone(),
            version: None,
            items: vec![test_diag("versionless", DiagSeverity::Info, None)],
            result_id: None,
        })
        .unwrap();
    manager.poll();

    let (version, diagnostics) = manager.instant_diagnostics.get(&path).unwrap();
    assert_eq!(*version, 5);
    assert_eq!(diagnostics[0].message.as_ref(), "new");
}
