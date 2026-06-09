use super::*;
use std::collections::HashMap;
use std::sync::mpsc;

#[test]
fn lsp_protocol_encodes_positions_paths_and_requests_end_to_end() {
    let text = "a\nпривет\n";
    let line_offsets = vec![0, 2, text.len()];
    assert_eq!(
        offset_to_lsp_pos(text, text.find("вет").unwrap(), &line_offsets),
        (1, 3)
    );

    let uri = path_to_uri("/tmp/rriter file.py");
    assert_eq!(uri_to_path(&uri), PathBuf::from("/tmp/rriter file.py"));

    let hover = String::from_utf8(make_hover(7, &uri, 1, 3)).unwrap();
    assert!(hover.contains(r#""id":7"#));
    assert!(hover.contains(r#""method":"textDocument/hover""#));
    assert!(hover.contains(r#""character":3"#));

    let signature = String::from_utf8(make_signature_help(8, &uri, 2, 4, Some(","))).unwrap();
    assert!(signature.contains(r#""id":8"#));
    assert!(signature.contains(r#""method":"textDocument/signatureHelp""#));
    assert!(signature.contains(r#""triggerCharacter":",""#));

    let open = String::from_utf8(make_did_open(&uri, "python", 2, "x = \"q\"\n")).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&open).unwrap();
    assert_eq!(parsed["params"]["textDocument"]["languageId"], "python");
    assert_eq!(parsed["params"]["textDocument"]["text"], "x = \"q\"\n");
}

#[test]
fn parses_signature_help_parameter_names() {
    let help = serde_json::json!({
        "activeSignature": 0,
        "signatures": [{
            "label": "create_pool(dsn=None, *, min_size=10, max_size=10, **kwargs)",
            "parameters": [
                {"label": "dsn=None"},
                {"label": "*, min_size=10"},
                {"label": [38, 49]},
                {"label": "**kwargs"},
                {"label": "self"}
            ]
        }]
    });

    assert_eq!(
        parse_signature_help_parameters(&help),
        vec![
            "dsn".to_string(),
            "min_size".to_string(),
            "max_size".to_string()
        ]
    );
}

#[test]
fn lsp_protocol_parses_diagnostics_workspace_edits_hover_and_actions() {
    let diag_json = serde_json::json!({
        "range": {
            "start": {"line": 1, "character": 2},
            "end": {"line": 1, "character": 5}
        },
        "severity": 2,
        "code": "F401",
        "source": "ruff",
        "message": "info: remove unused import\\nnext",
        "codeDescription": {"href": "https://example.invalid/F401"},
        "data": {
            "title": "Remove import",
            "edits": [{
                "range": {
                    "start": {"line": 1, "character": 2},
                    "end": {"line": 1, "character": 5}
                },
                "newText": ""
            }]
        },
        "tags": [1]
    });
    let diag = parse_diagnostic_value(&diag_json).unwrap();
    assert_eq!(diag.start_line, 1);
    assert_eq!(diag.severity, DiagSeverity::Warning);
    assert_eq!(diag.code.as_deref(), Some("F401"));
    assert_eq!(diag.source.as_deref(), Some("ruff"));
    assert_eq!(diag.message.as_ref(), "remove unused import\nnext");
    assert_eq!(diag.quickfixes.len(), 1);

    let edit_json = serde_json::json!({
        "changes": {
            "file:///tmp/a.py": [{
                "range": {
                    "start": {"line": 0, "character": 0},
                    "end": {"line": 0, "character": 1}
                },
                "newText": "b"
            }]
        },
        "documentChanges": [{
            "textDocument": {"uri": "file:///tmp/b.py"},
            "edits": [{
                "range": {
                    "start": {"line": 2, "character": 0},
                    "end": {"line": 2, "character": 3}
                },
                "newText": "pass"
            }]
        }]
    });
    let edit = parse_workspace_edit_value(&edit_json);
    assert_eq!(edit.changes.len(), 2);

    let hover_json = serde_json::json!({
        "contents": [
            {"language": "python", "value": "def fn() -> int"},
            "docs"
        ]
    });
    assert_eq!(
        parse_hover_value(&hover_json).as_deref(),
        Some("def fn() -> int\ndocs")
    );

    let action_json = serde_json::json!({
        "title": "Fix all",
        "kind": "source.fixAll",
        "diagnostics": [{"code": 123}],
        "edit": edit_json
    });
    let action = parse_code_action_value(&action_json).unwrap();
    assert_eq!(action.title, "Fix all");
    assert_eq!(action.kind.as_deref(), Some("source.fixAll"));
    assert_eq!(action.code.as_deref(), Some("123"));
    assert!(action.edit.is_some());

    let completion_json = serde_json::json!({
        "label": "Path",
        "kind": 7,
        "labelDetails": {"detail": " -> Path", "description": "pathlib"},
        "detail": "fallback detail should not be owner",
        "insertText": "Path",
        "additionalTextEdits": [{
            "range": {
                "start": {"line": 0, "character": 0},
                "end": {"line": 0, "character": 0}
            },
            "newText": "from pathlib import Path\n"
        }]
    });
    let completion = parse_completion_item_value(&completion_json).unwrap();
    assert_eq!(completion.label, "Path");
    assert_eq!(completion.module.as_deref(), Some("pathlib"));
    assert_eq!(completion.detail.as_deref(), Some("-> Path"));
    assert_eq!(completion.kind, crate::highlighter::SymbolKind::Class);
    assert_eq!(completion.additional_text_edits.len(), 1);

    let ty_import_completion = parse_completion_item_value(&serde_json::json!({
        "label": "RepoBase",
        "kind": 7,
        "labelDetails": {"detail": " (import car_wash.core.db.repo_base)"},
        "insertText": "RepoBase",
        "additionalTextEdits": [{
            "range": {
                "start": {"line": 0, "character": 0},
                "end": {"line": 0, "character": 0}
            },
            "newText": "from car_wash.core.db.repo_base import RepoBase\n"
        }]
    }))
    .unwrap();
    assert_eq!(
        ty_import_completion.module.as_deref(),
        Some("car_wash.core.db.repo_base")
    );
}

#[test]
fn completion_owner_comes_from_member_detail_and_detail_stays_full() {
    let completion_json = serde_json::json!({
        "label": "id",
        "kind": 5,
        "labelDetails": {"description": "int"},
        "data": {"fullName": "BoxReadPublic.id"},
        "detail": "(variable) BoxReadPublic.id: int"
    });

    let completion = parse_completion_item_value(&completion_json).unwrap();

    assert_eq!(completion.label, "id");
    assert_eq!(completion.module.as_deref(), Some("BoxReadPublic"));
    assert_eq!(
        completion.detail.as_deref(),
        Some("(variable) BoxReadPublic.id: int")
    );
    assert_eq!(completion.kind, crate::highlighter::SymbolKind::Variable);

    let typed_attr = parse_completion_item_value(&serde_json::json!({
        "label": "id",
        "kind": 5,
        "labelDetails": {"description": "int"},
        "detail": "(variable) id: int"
    }))
    .unwrap();
    assert_eq!(typed_attr.module, None);

    let datetime_attr = parse_completion_item_value(&serde_json::json!({
        "label": "created_at",
        "kind": 5,
        "labelDetails": {"description": "datetime"},
        "detail": "(variable) created_at: datetime"
    }))
    .unwrap();
    assert_eq!(datetime_attr.module, None);

    let imported_router = parse_completion_item_value(&serde_json::json!({
        "label": "cars_router",
        "kind": 6,
        "labelDetails": {
            "detail": ": Router",
            "description": "car_wash.domains.cars.controller"
        },
        "detail": "(variable) cars_router: Router"
    }))
    .unwrap();
    assert_eq!(
        imported_router.module.as_deref(),
        Some("car_wash.domains.cars.controller")
    );
    assert_eq!(
        imported_router.detail.as_deref(),
        Some("(variable) cars_router: Router")
    );
    assert_eq!(
        imported_router.kind,
        crate::highlighter::SymbolKind::Variable
    );

    let data_module_type_attr = parse_completion_item_value(&serde_json::json!({
        "label": "car_wash",
        "kind": 7,
        "data": {"module": "CarWashRead"},
        "detail": "(variable) car_wash: CarWashRead"
    }))
    .unwrap();
    assert_eq!(data_module_type_attr.module, None);

    let signature_description = parse_completion_item_value(&serde_json::json!({
        "label": "dir",
        "kind": 3,
        "labelDetails": {"description": "def dir(o: object = ..., /) -> list[str]"}
    }))
    .unwrap();
    assert_eq!(signature_description.module, None);

    let dotted_method = parse_completion_item_value(&serde_json::json!({
        "label": "initialize_all",
        "kind": 3,
        "data": {"fullName": "car_wash.core.db.repo_base.RepoBase.initialize_all"},
        "detail": "def RepoBase.initialize_all() -> None"
    }))
    .unwrap();
    assert_eq!(
        dotted_method.module.as_deref(),
        Some("car_wash.core.db.repo_base.RepoBase")
    );

    let dotted_variable = parse_completion_item_value(&serde_json::json!({
        "label": "RepoBase",
        "kind": 6,
        "labelDetails": {"description": "car_wash.core.db.repo_base"}
    }))
    .unwrap();
    assert_eq!(
        dotted_variable.module.as_deref(),
        Some("car_wash.core.db.repo_base")
    );

    let class_detail = parse_completion_item_value(&serde_json::json!({
        "label": "Router",
        "kind": 6,
        "labelDetails": {
            "detail": "Router",
            "description": "litestar.router"
        },
        "detail": "class Router"
    }))
    .unwrap();
    assert_eq!(class_detail.kind, crate::highlighter::SymbolKind::Class);
    assert_eq!(class_detail.detail.as_deref(), Some("class Router"));
    assert_eq!(class_detail.module.as_deref(), Some("litestar.router"));

    let class_with_docs = parse_completion_item_value(&serde_json::json!({
        "label": "map",
        "kind": 7,
        "labelDetails": {
            "detail": "class map",
            "description": "builtins"
        },
        "documentation": {
            "kind": "plaintext",
            "value": "Make an iterator that computes the function using arguments from\neach of the iterables."
        }
    }))
    .unwrap();
    assert_eq!(
        class_with_docs.detail.as_deref(),
        Some("class map\n---\nMake an iterator that computes the function using arguments from\neach of the iterables.")
    );
    assert_eq!(class_with_docs.module.as_deref(), Some("builtins"));

    let generic_class_doc = parse_completion_item_value(&serde_json::json!({
        "label": "RepoBase",
        "kind": 7,
        "labelDetails": {
            "detail": "class RepoBase",
            "description": "car_wash.core.db.repo_base"
        },
        "documentation": {
            "kind": "markdown",
            "value": "car_wash.core.db.repo_base\nclass RepoBase[TModel: Base, TReadStruct: BasedStruct]\n---\nRepo core"
        }
    }))
    .unwrap();
    assert_eq!(
        generic_class_doc.detail.as_deref(),
        Some("car_wash.core.db.repo_base\nclass RepoBase[TModel: Base, TReadStruct: BasedStruct]\n---\nRepo core")
    );
    assert_eq!(
        generic_class_doc.module.as_deref(),
        Some("car_wash.core.db.repo_base")
    );
}

#[test]
fn completion_kind_uses_ty_detail_for_property_parameter_and_type() {
    let property = parse_completion_item_value(&serde_json::json!({
        "label": "status",
        "kind": 10,
        "detail": "(property) status: str"
    }))
    .unwrap();
    assert_eq!(property.kind, crate::highlighter::SymbolKind::Property);

    let parameter = parse_completion_item_value(&serde_json::json!({
        "label": "timeout",
        "kind": 6,
        "detail": "(parameter) timeout: float",
        "insertText": "timeout="
    }))
    .unwrap();
    assert_eq!(parameter.kind, crate::highlighter::SymbolKind::Parameter);

    let inferred_type = parse_completion_item_value(&serde_json::json!({
        "label": "Path",
        "detail": "type[Path]",
        "labelDetails": {"description": "pathlib"}
    }))
    .unwrap();
    assert_eq!(inferred_type.kind, crate::highlighter::SymbolKind::Class);
    assert_eq!(inferred_type.module.as_deref(), Some("pathlib"));
}

#[test]
fn parses_inlay_hint_string_and_label_parts() {
    let hints = parse_inlay_hints(&serde_json::json!([
        {
            "position": {"line": 2, "character": 8},
            "label": "id:"
        },
        {
            "position": {"line": 2, "character": 12},
            "label": [{"value": "name"}, ":"]
        }
    ]));

    assert_eq!(
        hints,
        vec![
            LspInlayHint {
                line: 2,
                col: 8,
                label: "id:".to_string(),
            },
            LspInlayHint {
                line: 2,
                col: 12,
                label: "name:".to_string(),
            },
        ]
    );
}

#[test]
fn lsp_dispatch_routes_pending_responses_end_to_end() {
    let (event_tx, event_rx) = mpsc::channel();
    let (out_tx, _out_rx) = mpsc::channel();
    let pending = Arc::new(Mutex::new(HashMap::from([(9, PendingRequestKind::Hover)])));

    dispatch_frame(
        br#"{"jsonrpc":"2.0","id":9,"result":{"contents":{"value":"hover text"}}}"#,
        &event_tx,
        "test",
        &out_tx,
        &pending,
    );

    let _log = event_rx.recv().unwrap();
    match event_rx.recv().unwrap() {
        LspEvent::HoverResponse { request_id, text } => {
            assert_eq!(request_id, 9);
            assert_eq!(text.as_deref(), Some("hover text"));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

fn recv_non_log(rx: &mpsc::Receiver<LspEvent>) -> LspEvent {
    loop {
        let event = rx.recv().unwrap();
        if !matches!(
            event,
            LspEvent::Log { .. } | LspEvent::ConfigurationServed { .. }
        ) {
            return event;
        }
    }
}

#[test]
fn lsp_protocol_encodes_initialize_change_close_action_definition_shutdown() {
    let workspaces = vec![PathBuf::from("/tmp/ws one"), PathBuf::from("/tmp/ws2")];
    let init: serde_json::Value =
        serde_json::from_slice(&make_initialize(42, &workspaces)).unwrap();
    assert_eq!(init["id"], 42);
    assert_eq!(init["method"], "initialize");
    assert_eq!(init["params"]["rootUri"], path_to_uri("/tmp/ws one"));
    assert_eq!(
        init["params"]["workspaceFolders"]
            .as_array()
            .map(|items| items.len()),
        Some(2)
    );
    assert_eq!(
        init["params"]["capabilities"]["textDocument"]["codeAction"]["codeActionLiteralSupport"]["codeActionKind"]
            ["valueSet"][2],
        "source.fixAll"
    );

    let init_without_workspace: serde_json::Value =
        serde_json::from_slice(&make_initialize(43, &[])).unwrap();
    assert!(init_without_workspace["params"]["rootUri"].is_null());
    assert!(
        init_without_workspace["params"]
            .get("workspaceFolders")
            .is_none()
    );

    let uri = "file:///tmp/project/main.py";
    let changed: serde_json::Value =
        serde_json::from_slice(&make_did_change_full(uri, 5, "a\\b\n\t\"q\"")).unwrap();
    assert_eq!(changed["method"], "textDocument/didChange");
    assert_eq!(
        changed["params"]["contentChanges"][0]["text"],
        "a\\b\n\t\"q\""
    );

    let closed: serde_json::Value = serde_json::from_slice(&make_did_close(uri)).unwrap();
    assert_eq!(closed["method"], "textDocument/didClose");
    assert_eq!(closed["params"]["textDocument"]["uri"], uri);

    let only = vec!["quickfix".to_string(), "source.fixAll".to_string()];
    let action: serde_json::Value =
        serde_json::from_slice(&make_code_action(99, uri, 1, 2, 3, 4, "[]", Some(&only))).unwrap();
    assert_eq!(action["id"], 99);
    assert_eq!(action["params"]["range"]["start"]["line"], 1);
    assert_eq!(action["params"]["range"]["end"]["character"], 4);
    assert_eq!(
        action["params"]["context"]["diagnostics"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(action["params"]["context"]["only"][1], "source.fixAll");

    let definition: serde_json::Value =
        serde_json::from_slice(&make_definition(100, uri, 7, 8)).unwrap();
    assert_eq!(definition["method"], "textDocument/definition");
    assert_eq!(definition["params"]["position"]["line"], 7);

    let completion: serde_json::Value =
        serde_json::from_slice(&make_completion(102, uri, 2, 4, Some("."))).unwrap();
    assert_eq!(completion["method"], "textDocument/completion");
    assert_eq!(completion["params"]["context"]["triggerKind"], 2);
    assert_eq!(completion["params"]["context"]["triggerCharacter"], ".");

    let workspace_diag: serde_json::Value = serde_json::from_slice(&make_workspace_diagnostic(
        103,
        r#"[{"uri":"file:///tmp/project/main.py","value":"r1"}]"#,
    ))
    .unwrap();
    assert_eq!(workspace_diag["method"], "workspace/diagnostic");
    assert_eq!(workspace_diag["params"]["identifier"], "ty");
    assert_eq!(
        workspace_diag["params"]["previousResultIds"][0]["value"],
        "r1"
    );

    let shutdown: serde_json::Value = serde_json::from_slice(&make_shutdown(101)).unwrap();
    assert_eq!(shutdown["method"], "shutdown");
    assert!(shutdown["params"].is_null());

    let exit: serde_json::Value = serde_json::from_slice(&make_exit()).unwrap();
    assert_eq!(exit["method"], "exit");
}

#[test]
fn lsp_protocol_parses_edge_shapes_and_dispatches_server_requests() {
    let spans = highlight_diagnostic_message("`NameError` ├─ branch │ tail");
    assert!(spans.iter().any(|s| s.color == [0.6, 0.6, 0.65, 1.0]));
    assert!(spans.iter().any(|s| s.color == [0.45, 0.45, 0.50, 1.0]));

    let diag_json = serde_json::json!({
        "range": {
            "start": {"line": 4, "character": 1},
            "end": {"line": 4, "character": 9}
        },
        "severity": 99,
        "code": false,
        "message": "raw\\ttext\r"
    });
    let diag = parse_diagnostic_value(&diag_json).unwrap();
    assert_eq!(diag.severity, DiagSeverity::Hint);
    assert_eq!(diag.code, None);
    assert_eq!(diag.source, None);
    assert_eq!(diag.message.as_ref(), "raw    text");
    assert!(parse_diagnostic_value(&serde_json::json!({})).is_none());

    assert_eq!(
        parse_hover_value(&serde_json::json!({"contents": "plain"})).as_deref(),
        Some("plain")
    );
    assert_eq!(
        parse_hover_value(&serde_json::json!({"contents": {"kind": "markdown", "value": "obj"}}))
            .as_deref(),
        Some("obj")
    );
    assert_eq!(
        parse_hover_value(&serde_json::json!({"contents": [123, {"value": "kept"}, "tail"]}))
            .as_deref(),
        Some("kept\ntail")
    );
    assert_eq!(
        parse_definition_target(&serde_json::json!([{"targetUri": "file:///tmp/target.py"}]))
            .map(|target| target.path),
        Some(PathBuf::from("/tmp/target.py"))
    );
    let target = parse_definition_target(&serde_json::json!([{
        "targetUri": "file:///tmp/target.py",
        "targetSelectionRange": {
            "start": {"line": 12, "character": 4},
            "end": {"line": 12, "character": 10}
        }
    }]))
    .unwrap();
    assert_eq!(target.path, PathBuf::from("/tmp/target.py"));
    assert_eq!(target.line, 12);
    assert_eq!(target.col, 4);

    let (event_tx, event_rx) = mpsc::channel();
    let (out_tx, out_rx) = mpsc::channel();
    let pending = Arc::new(Mutex::new(HashMap::new()));

    dispatch_frame(
        br#"{"jsonrpc":"2.0","id":"55","method":"client/registerCapability","params":{}}"#,
        &event_tx,
        "test",
        &out_tx,
        &pending,
    );
    let reply: serde_json::Value = serde_json::from_slice(&out_rx.try_recv().unwrap()).unwrap();
    assert_eq!(reply["id"], 55);
    assert!(reply["result"].is_null());

    dispatch_frame(
            br#"{"jsonrpc":"2.0","id":56,"method":"workspace/configuration","params":{"items":[{},{}]}}"#,
            &event_tx,
            "test",
            &out_tx,
            &pending,
        );
    let reply: serde_json::Value = serde_json::from_slice(&out_rx.try_recv().unwrap()).unwrap();
    assert_eq!(reply["id"], 56);
    assert_eq!(reply["result"].as_array().unwrap().len(), 2);

    dispatch_frame(
            br#"{"jsonrpc":"2.0","id":560,"method":"workspace/configuration","params":{"items":[{"section":"ty"},{"section":"ty.diagnosticMode"},{"section":"other"}]}}"#,
            &event_tx,
            "ty",
            &out_tx,
            &pending,
        );
    let reply: serde_json::Value = serde_json::from_slice(&out_rx.try_recv().unwrap()).unwrap();
    assert_eq!(reply["id"], 560);
    assert_eq!(reply["result"][0]["diagnosticMode"], "workspace");
    assert_eq!(reply["result"][1], "workspace");
    assert_eq!(reply["result"][2], serde_json::json!({}));

    dispatch_frame(
        br#"{"jsonrpc":"2.0","id":57,"method":"unknown/request","params":{}}"#,
        &event_tx,
        "test",
        &out_tx,
        &pending,
    );
    let reply: serde_json::Value = serde_json::from_slice(&out_rx.try_recv().unwrap()).unwrap();
    assert_eq!(reply["error"]["code"], -32601);

    dispatch_frame(
            br#"{"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{"uri":"file:///tmp/a.py","version":3,"diagnostics":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"message":"boom"}]}}"#,
            &event_tx,
            "ruff",
            &out_tx,
            &pending,
        );
    match recv_non_log(&event_rx) {
        LspEvent::Diagnostics {
            server_name,
            path,
            version,
            items,
            result_id,
        } => {
            assert_eq!(server_name, "ruff");
            assert_eq!(path, PathBuf::from("/tmp/a.py"));
            assert_eq!(version, Some(3));
            assert_eq!(result_id, None);
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].message.as_ref(), "boom");
        }
        other => panic!("unexpected event: {other:?}"),
    }

    dispatch_frame(
            br#"{"jsonrpc":"2.0","id":58,"method":"workspace/applyEdit","params":{"edit":{"changes":{"file:///tmp/a.py":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},"newText":"x"}]}}}}"#,
            &event_tx,
            "ruff",
            &out_tx,
            &pending,
        );
    match recv_non_log(&event_rx) {
        LspEvent::CodeActions {
            request_id,
            actions,
        } => {
            assert_eq!(request_id, -1);
            assert_eq!(actions.len(), 1);
            assert_eq!(actions[0].title, "workspace/applyEdit");
            assert_eq!(actions[0].edit.as_ref().unwrap().changes.len(), 1);
        }
        other => panic!("unexpected event: {other:?}"),
    }

    let (bad_tx, bad_rx) = mpsc::channel();
    dispatch_frame(b"not json", &bad_tx, "bad", &out_tx, &pending);
    match bad_rx.recv().unwrap() {
        LspEvent::Log { name, message } => {
            assert_eq!(name, "bad");
            assert!(message.contains("[LSP RECV ERROR]"));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn lsp_dispatch_handles_pending_kinds_fallbacks_and_notifications() {
    let (event_tx, event_rx) = mpsc::channel();
    let (out_tx, out_rx) = mpsc::channel();
    let pending = Arc::new(Mutex::new(HashMap::from([
        (1, PendingRequestKind::CodeAction),
        (2, PendingRequestKind::Definition),
        (3, PendingRequestKind::Hover),
        (6, PendingRequestKind::Completion),
        (7, PendingRequestKind::WorkspaceDiagnostic),
    ])));

    dispatch_frame(
        br#"{"jsonrpc":"2.0","method":"window/logMessage","params":{"message":"server note"}}"#,
        &event_tx,
        "ruff",
        &out_tx,
        &pending,
    );
    match event_rx.recv().unwrap() {
        LspEvent::Log { name, message } => {
            assert_eq!(name, "ruff");
            assert!(message.contains("\"window/logMessage\""));
        }
        other => panic!("unexpected event: {other:?}"),
    }
    match event_rx.recv().unwrap() {
        LspEvent::Log { name, message } => {
            assert_eq!(name, "ruff");
            assert_eq!(message, "server note");
        }
        other => panic!("unexpected event: {other:?}"),
    }

    dispatch_frame(
            br#"{"jsonrpc":"2.0","id":1,"result":[{"title":"Apply","kind":"quickfix","diagnostics":[{"code":"F401"}],"edit":{"documentChanges":[{"textDocument":{"uri":"file:///tmp/doc.py"},"edits":[{"range":{"start":{"line":1,"character":0},"end":{"line":1,"character":1}},"newText":"x"}]}]}}]}"#,
            &event_tx,
            "ruff",
            &out_tx,
            &pending,
        );
    match recv_non_log(&event_rx) {
        LspEvent::CodeActions {
            request_id,
            actions,
        } => {
            assert_eq!(request_id, 1);
            assert_eq!(actions.len(), 1);
            assert_eq!(actions[0].title, "Apply");
            assert_eq!(actions[0].code.as_deref(), Some("F401"));
            let edit = actions[0].edit.as_ref().unwrap();
            assert_eq!(edit.changes[&PathBuf::from("/tmp/doc.py")][0].new_text, "x");
        }
        other => panic!("unexpected event: {other:?}"),
    }

    dispatch_frame(
        br#"{"jsonrpc":"2.0","id":2,"result":{"uri":"file:///tmp/definition.py"}}"#,
        &event_tx,
        "ty",
        &out_tx,
        &pending,
    );
    match recv_non_log(&event_rx) {
        LspEvent::DefinitionResponse { request_id, target } => {
            assert_eq!(request_id, 2);
            let target = target.unwrap();
            assert_eq!(target.path, PathBuf::from("/tmp/definition.py"));
            assert_eq!(target.line, 0);
            assert_eq!(target.col, 0);
        }
        other => panic!("unexpected event: {other:?}"),
    }

    dispatch_frame(
        br#"{"jsonrpc":"2.0","id":3,"result":null}"#,
        &event_tx,
        "ty",
        &out_tx,
        &pending,
    );
    match recv_non_log(&event_rx) {
        LspEvent::HoverResponse { request_id, text } => {
            assert_eq!(request_id, 3);
            assert_eq!(text, None);
        }
        other => panic!("unexpected event: {other:?}"),
    }

    dispatch_frame(
            br#"{"jsonrpc":"2.0","id":6,"result":{"isIncomplete":false,"items":[{"label":"Path","kind":7,"labelDetails":{"description":"pathlib"}}]}}"#,
            &event_tx,
            "ty",
            &out_tx,
            &pending,
        );
    match recv_non_log(&event_rx) {
        LspEvent::CompletionResponse { request_id, items } => {
            assert_eq!(request_id, 6);
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].module.as_deref(), Some("pathlib"));
        }
        other => panic!("unexpected event: {other:?}"),
    }

    dispatch_frame(
            br#"{"jsonrpc":"2.0","id":7,"result":{"items":[{"kind":"full","uri":"file:///tmp/workspace.py","version":4,"resultId":"r1","items":[{"range":{"start":{"line":2,"character":0},"end":{"line":2,"character":3}},"message":"workspace boom","severity":1}],"relatedDocuments":{"file:///tmp/related.py":{"kind":"full","resultId":"r2","items":[{"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},"message":"related boom"}]}}},{"kind":"unchanged","uri":"file:///tmp/unchanged.py","resultId":"r3"}]}}"#,
            &event_tx,
            "ty",
            &out_tx,
            &pending,
        );
    match recv_non_log(&event_rx) {
        LspEvent::Diagnostics {
            server_name,
            path,
            version,
            items,
            result_id,
        } => {
            assert_eq!(server_name, "ty");
            assert_eq!(path, PathBuf::from("/tmp/workspace.py"));
            assert_eq!(version, Some(4));
            assert_eq!(items[0].message.as_ref(), "workspace boom");
            assert_eq!(result_id.as_deref(), Some("r1"));
        }
        other => panic!("unexpected event: {other:?}"),
    }
    match recv_non_log(&event_rx) {
        LspEvent::Diagnostics {
            path,
            items,
            result_id,
            ..
        } => {
            assert_eq!(path, PathBuf::from("/tmp/related.py"));
            assert_eq!(items[0].message.as_ref(), "related boom");
            assert_eq!(result_id.as_deref(), Some("r2"));
        }
        other => panic!("unexpected event: {other:?}"),
    }
    match recv_non_log(&event_rx) {
        LspEvent::WorkspaceDiagnosticsDone { request_id } => assert_eq!(request_id, 7),
        other => panic!("unexpected event: {other:?}"),
    }

    let (large_event_tx, large_event_rx) = mpsc::channel();
    let (large_out_tx, _large_out_rx) = mpsc::channel();
    let large_pending = Arc::new(Mutex::new(HashMap::from([(
        6,
        PendingRequestKind::Completion,
    )])));
    let items = (0..96)
        .map(|idx| format!(r#"{{"label":"Item{idx}","kind":7}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let body = format!(
        r#"{{"jsonrpc":"2.0","id":6,"result":{{"isIncomplete":false,"items":[{items}]}}}}"#
    );
    dispatch_frame(
        body.as_bytes(),
        &large_event_tx,
        "ty",
        &large_out_tx,
        &large_pending,
    );
    match large_event_rx.recv().unwrap() {
        LspEvent::Log { name, message } => {
            assert_eq!(name, "ty");
            assert!(message.contains(r#""items_omitted":96"#));
            assert!(message.contains(r#""body_bytes":"#));
            assert!(!message.contains("Item95"));
            assert!(message.len() < 160);
        }
        other => panic!("unexpected event: {other:?}"),
    }
    match recv_non_log(&large_event_rx) {
        LspEvent::CompletionResponse { request_id, items } => {
            assert_eq!(request_id, 6);
            assert_eq!(items.len(), 96);
            assert_eq!(items[95].label, "Item95");
        }
        other => panic!("unexpected event: {other:?}"),
    }

    dispatch_frame(
        br#"{"jsonrpc":"2.0","id":4,"result":[{"title":"Fallback"}]}"#,
        &event_tx,
        "ruff",
        &out_tx,
        &pending,
    );
    match recv_non_log(&event_rx) {
        LspEvent::CodeActions {
            request_id,
            actions,
        } => {
            assert_eq!(request_id, 4);
            assert_eq!(actions[0].title, "Fallback");
        }
        other => panic!("unexpected event: {other:?}"),
    }

    dispatch_frame(
        br#"{"jsonrpc":"2.0","id":5,"method":"client/unregisterCapability","params":{}}"#,
        &event_tx,
        "ruff",
        &out_tx,
        &pending,
    );
    let reply: serde_json::Value = serde_json::from_slice(&out_rx.try_recv().unwrap()).unwrap();
    assert_eq!(reply["id"], 5);
    assert!(reply["result"].is_null());
}
