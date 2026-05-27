// ── JSON helpers ──────────────────────────────────────────────────────────────

/// Экранирует строку для встраивания в JSON (без внешних кавычек)
pub(super) fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = std::fmt::Write::write_fmt(&mut out, format_args!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

/// Сериализует путь → file:// URI
pub(super) fn path_to_uri(path: &str) -> String {
    let p = std::path::Path::new(path);
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    };
    let mut s = abs.to_string_lossy().replace('\\', "/");
    if !s.starts_with('/') {
        s.insert(0, '/');
    }
    format!("file://{}", s)
}

pub(super) fn uri_to_path(uri: &str) -> PathBuf {
    let mut s = uri.strip_prefix("file://").unwrap_or(uri);
    // Для Windows (file:///C:/...) убираем первый слеш
    if s.starts_with('/') && s.chars().nth(2) == Some(':') {
        s = &s[1..];
    }
    PathBuf::from(s)
}

// ── Кодировщики JSON-RPC сообщений ────────────────────────────────────────────

pub(super) fn make_initialize(id: i32, workspaces: &[PathBuf]) -> Vec<u8> {
    let (root_uri_json, workspace_json) = if let Some(first_ws) = workspaces.first() {
        let root_uri = path_to_uri(&first_ws.to_string_lossy());
        let escaped_root = json_escape(&root_uri);

        let mut folders = Vec::new();
        for (i, ws) in workspaces.iter().enumerate() {
            let uri = path_to_uri(&ws.to_string_lossy());
            folders.push(format!(
                r#"{{"uri":"{}","name":"workspace_{}"}}"#,
                json_escape(&uri),
                i
            ));
        }

        (
            format!(r#""{}""#, escaped_root),
            format!(r#","workspaceFolders":[{}]"#, folders.join(",")),
        )
    } else {
        (String::from("null"), String::new())
    };

    let body = format!(
        r#"{{"jsonrpc":"2.0","id":{id},"method":"initialize","params":{{"processId":{},"clientInfo":{{"name":"RRiter","version":"0.1"}},"capabilities":{{"workspace":{{"configuration":true,"didChangeConfiguration":{{"dynamicRegistration":true}},"didChangeWatchedFiles":{{"dynamicRegistration":true,"relativePatternSupport":true}},"workspaceFolders":true}},"textDocument":{{"synchronization":{{"dynamicRegistration":true,"willSave":false,"willSaveWaitUntil":false,"didSave":true}},"publishDiagnostics":{{"relatedInformation":false,"versionSupport":true,"codeDescriptionSupport":true}},"completion":{{"completionItem":{{"snippetSupport":false,"labelDetailsSupport":true,"resolveSupport":{{"properties":["additionalTextEdits","textEdit","detail"]}}}}}},"inlayHint":{{"dynamicRegistration":false}},"codeAction":{{"codeActionLiteralSupport":{{"codeActionKind":{{"valueSet":["quickfix","source","source.fixAll","source.organizeImports"]}}}},"resolveSupport":{{"properties":["edit"]}}}}}}}},"rootUri":{}{workspace_json}}}}}"#,
        std::process::id(),
        root_uri_json
    );
    body.into_bytes()
}

pub(super) fn make_initialized() -> Vec<u8> {
    br#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#.to_vec()
}

pub(super) fn make_did_open(uri: &str, lang: &str, version: i32, text: &str) -> Vec<u8> {
    let body = format!(
        r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"{}","version":{},"text":"{}"}}}}}}"#,
        json_escape(uri),
        lang,
        version,
        json_escape(text)
    );
    body.into_bytes()
}

pub(super) fn make_did_change_full(uri: &str, version: i32, text: &str) -> Vec<u8> {
    let body = format!(
        r#"{{"jsonrpc":"2.0","method":"textDocument/didChange","params":{{"textDocument":{{"uri":"{}","version":{}}},"contentChanges":[{{"text":"{}"}}]}}}}"#,
        json_escape(uri),
        version,
        json_escape(text)
    );
    body.into_bytes()
}

pub(super) fn make_did_close(uri: &str) -> Vec<u8> {
    let body = format!(
        r#"{{"jsonrpc":"2.0","method":"textDocument/didClose","params":{{"textDocument":{{"uri":"{}"}}}}}}"#,
        json_escape(uri)
    );
    body.into_bytes()
}

pub(super) fn make_code_action(
    id: i32,
    uri: &str,
    sl: u32,
    sc: u32,
    el: u32,
    ec: u32,
    diag_json: &str,
    only: Option<&[String]>,
) -> Vec<u8> {
    let only_json = match only {
        Some(arr) => {
            let vals: Vec<String> = arr
                .iter()
                .map(|s| format!("\"{}\"", json_escape(s)))
                .collect();
            format!(r#","only":[{}]"#, vals.join(","))
        }
        None => String::new(),
    };
    let body = format!(
        r#"{{"jsonrpc":"2.0","id":{id},"method":"textDocument/codeAction","params":{{"textDocument":{{"uri":"{}"}},"range":{{"start":{{"line":{sl},"character":{sc}}},"end":{{"line":{el},"character":{ec}}}}},"context":{{"diagnostics":{diag_json}{only_json}}}}}}}"#,
        json_escape(uri)
    );
    body.into_bytes()
}

pub(super) fn make_hover(id: i32, uri: &str, line: u32, col: u32) -> Vec<u8> {
    format!(
        r#"{{"jsonrpc":"2.0","id":{},"method":"textDocument/hover","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":{},"character":{}}}}}}}"#,
        id, json_escape(uri), line, col
    ).into_bytes()
}

pub(super) fn make_definition(id: i32, uri: &str, line: u32, col: u32) -> Vec<u8> {
    format!(
        r#"{{"jsonrpc":"2.0","id":{},"method":"textDocument/definition","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":{},"character":{}}}}}}}"#,
        id, json_escape(uri), line, col
    ).into_bytes()
}

pub(super) fn make_completion(
    id: i32,
    uri: &str,
    line: u32,
    col: u32,
    trigger: Option<&str>,
) -> Vec<u8> {
    let context = if let Some(ch) = trigger {
        format!(
            r#","context":{{"triggerKind":2,"triggerCharacter":"{}"}}"#,
            json_escape(ch)
        )
    } else {
        String::from(r#","context":{"triggerKind":1}"#)
    };
    format!(
        r#"{{"jsonrpc":"2.0","id":{},"method":"textDocument/completion","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":{},"character":{}}}{}}}}}"#,
        id,
        json_escape(uri),
        line,
        col,
        context
    )
    .into_bytes()
}

pub(super) fn make_signature_help(
    id: i32,
    uri: &str,
    line: u32,
    col: u32,
    trigger: Option<&str>,
) -> Vec<u8> {
    let context = if let Some(ch) = trigger {
        format!(
            r#","context":{{"triggerKind":2,"triggerCharacter":"{}"}}"#,
            json_escape(ch)
        )
    } else {
        String::from(r#","context":{"triggerKind":1}"#)
    };
    format!(
        r#"{{"jsonrpc":"2.0","id":{},"method":"textDocument/signatureHelp","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":{},"character":{}}}{}}}}}"#,
        id,
        json_escape(uri),
        line,
        col,
        context
    )
    .into_bytes()
}

pub(super) fn make_inlay_hint(
    id: i32,
    uri: &str,
    start_line: u32,
    start_col: u32,
    end_line: u32,
    end_col: u32,
) -> Vec<u8> {
    format!(
        r#"{{"jsonrpc":"2.0","id":{},"method":"textDocument/inlayHint","params":{{"textDocument":{{"uri":"{}"}},"range":{{"start":{{"line":{},"character":{}}},"end":{{"line":{},"character":{}}}}}}}}}"#,
        id,
        json_escape(uri),
        start_line,
        start_col,
        end_line,
        end_col
    )
    .into_bytes()
}

pub(super) fn make_workspace_diagnostic(id: i32, previous_result_ids_json: &str) -> Vec<u8> {
    format!(
        r#"{{"jsonrpc":"2.0","id":{},"method":"workspace/diagnostic","params":{{"identifier":"ty","previousResultIds":{}}}}}"#,
        id, previous_result_ids_json
    )
    .into_bytes()
}

pub(super) fn make_shutdown(id: i32) -> Vec<u8> {
    format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"shutdown","params":null}}"#).into_bytes()
}

pub(super) fn make_exit() -> Vec<u8> {
    br#"{"jsonrpc":"2.0","method":"exit","params":null}"#.to_vec()
}

// ── Запись JSON-RPC фрейма ────────────────────────────────────────────────────

pub(super) fn write_frame(writer: &mut BufWriter<std::process::ChildStdin>, body: &[u8]) -> bool {
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    writer.write_all(header.as_bytes()).is_ok()
        && writer.write_all(body).is_ok()
        && writer.flush().is_ok()
}


fn configuration_response_for(
    server_name: &'static str,
    item: &serde_json::Value,
) -> serde_json::Value {
    if server_name != TY_SERVER.program {
        return serde_json::json!({});
    }

    match item.get("section").and_then(|v| v.as_str()).unwrap_or("ty") {
        "ty.diagnosticMode" => serde_json::json!("workspace"),
        "ty" | "" => serde_json::json!({ "diagnosticMode": "workspace" }),
        _ => serde_json::json!({}),
    }
}

fn emit_workspace_diagnostic_report(
    uri: &str,
    report: &serde_json::Value,
    event_tx: &Sender<LspEvent>,
    server_name: &'static str,
) {
    let kind = report.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    if kind == "unchanged" {
        return;
    }

    let Some(diags) = report.get("items").and_then(|v| v.as_array()) else {
        return;
    };

    let items = diags
        .iter()
        .filter_map(parse_diagnostic_value)
        .collect::<Vec<_>>();
    let version = report
        .get("version")
        .and_then(|v| v.as_i64())
        .map(|v| v as i32);
    let result_id = report
        .get("resultId")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let _ = event_tx.send(LspEvent::Diagnostics {
        server_name,
        path: uri_to_path(uri),
        version,
        items,
        result_id,
    });
}

fn emit_workspace_diagnostics(
    result: &serde_json::Value,
    event_tx: &Sender<LspEvent>,
    server_name: &'static str,
) {
    let Some(items) = result.get("items").and_then(|v| v.as_array()) else {
        return;
    };

    for item in items {
        if let Some(uri) = item.get("uri").and_then(|v| v.as_str()) {
            emit_workspace_diagnostic_report(uri, item, event_tx, server_name);
        }

        if let Some(related) = item.get("relatedDocuments").and_then(|v| v.as_object()) {
            for (uri, report) in related {
                emit_workspace_diagnostic_report(uri, report, event_tx, server_name);
            }
        }
    }
}

// ── Основной парсер входящих фреймов ─────────────────────────────────────────

pub(super) fn dispatch_frame(
    body: &[u8],
    event_tx: &Sender<LspEvent>,
    server_name: &'static str,
    out_tx: &Sender<Vec<u8>>,
    pending_requests: &Arc<Mutex<HashMap<i32, PendingRequestKind>>>,
) {
    let msg: serde_json::Value = match serde_json::from_slice(body) {
        Ok(v) => v,
        Err(e) => {
            let log_msg = format!("[LSP RECV ERROR] {}: {}", e, String::from_utf8_lossy(body));
            let _ = event_tx.send(LspEvent::Log {
                name: server_name,
                message: log_msg,
            });
            return;
        }
    };

    let log_msg = recv_log_message(body, &msg);
    let _ = event_tx.send(LspEvent::Log {
        name: server_name,
        message: log_msg,
    });

    let method = msg.get("method").and_then(|v| v.as_str());
    let id = msg.get("id").and_then(|v| {
        v.as_i64()
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
    });

    match method {
        Some("textDocument/publishDiagnostics") => {
            if let Some(params) = msg.get("params") {
                if let Some(uri) = params.get("uri").and_then(|v| v.as_str()) {
                    let path = uri_to_path(uri);
                    let version = params
                        .get("version")
                        .and_then(|v| v.as_i64())
                        .map(|v| v as i32);

                    let mut items = Vec::new();
                    if let Some(diags) = params.get("diagnostics").and_then(|v| v.as_array()) {
                        for d in diags {
                            if let Some(diag) = parse_diagnostic_value(d) {
                                items.push(diag);
                            }
                        }
                    }
                    let _ = event_tx.send(LspEvent::Diagnostics {
                        server_name,
                        path,
                        version,
                        items,
                        result_id: None,
                    });
                }
            }
        }
        Some("workspace/applyEdit") => {
            if let Some(params) = msg.get("params") {
                if let Some(edit_obj) = params.get("edit") {
                    let edit = parse_workspace_edit_value(edit_obj);
                    let action = CodeAction {
                        title: "workspace/applyEdit".to_string(),
                        kind: None,
                        edit: Some(edit),
                        code: None,
                    };
                    let _ = event_tx.send(LspEvent::CodeActions {
                        request_id: -1,
                        actions: vec![action],
                    });
                }
            }
        }
        Some("initialize") => {}
        Some("window/logMessage") | Some("window/showMessage") => {
            if let Some(params) = msg.get("params") {
                if let Some(msg_str) = params.get("message").and_then(|v| v.as_str()) {
                    let _ = event_tx.send(LspEvent::Log {
                        name: server_name,
                        message: msg_str.to_string(),
                    });
                }
            }
        }
        Some("client/registerCapability") | Some("client/unregisterCapability") => {
            if let Some(req_id) = id {
                let reply = format!(r#"{{"jsonrpc":"2.0","id":{},"result":null}}"#, req_id);
                let _ = out_tx.send(reply.into_bytes());
            }
        }
        Some("workspace/configuration") => {
            if let Some(req_id) = id {
                let objs = if let Some(items) =
                    msg.pointer("/params/items").and_then(|v| v.as_array())
                {
                    let values = items
                        .iter()
                        .map(|item| configuration_response_for(server_name, item).to_string())
                        .collect::<Vec<_>>();
                    if values.is_empty() {
                        configuration_response_for(server_name, &serde_json::Value::Null)
                            .to_string()
                    } else {
                        values.join(",")
                    }
                } else {
                    configuration_response_for(server_name, &serde_json::Value::Null).to_string()
                };
                let reply = format!(r#"{{"jsonrpc":"2.0","id":{},"result":[{}]}}"#, req_id, objs);
                let _ = out_tx.send(reply.into_bytes());
                let _ = event_tx.send(LspEvent::ConfigurationServed { name: server_name });
            }
        }
        Some(m) => {
            if let Some(req_id) = id {
                if m != "window/logMessage"
                    && m != "window/showMessage"
                    && m != "textDocument/publishDiagnostics"
                    && m != "workspace/applyEdit"
                {
                    let reply = format!(
                        r#"{{"jsonrpc":"2.0","id":{},"error":{{"code":-32601,"message":"Method not found"}}}}"#,
                        req_id
                    );
                    let _ = out_tx.send(reply.into_bytes());
                }
            }
        }
        None => {
            if let Some(req_id) = id {
                let pending_kind = pending_requests
                    .lock()
                    .ok()
                    .and_then(|mut p| p.remove(&(req_id as i32)));

                if msg.get("error").is_some() {
                    match pending_kind {
                        Some(PendingRequestKind::WorkspaceDiagnostic) => {
                            let _ = event_tx.send(LspEvent::WorkspaceDiagnosticsDone {
                                request_id: req_id as i32,
                            });
                        }
                        Some(PendingRequestKind::InlayHint) => {
                            let _ = event_tx.send(LspEvent::InlayHintsResponse {
                                request_id: req_id as i32,
                                hints: Vec::new(),
                            });
                        }
                        _ => {}
                    }
                    return;
                }

                if let Some(result) = msg.get("result") {
                    match pending_kind {
                        Some(PendingRequestKind::Hover) => {
                            if result.get("contents").is_some() {
                                if let Some(hover) = parse_hover_value(result) {
                                    let _ = event_tx.send(LspEvent::HoverResponse {
                                        request_id: req_id as i32,
                                        text: Some(hover),
                                    });
                                }
                            } else if result.is_null() {
                                let _ = event_tx.send(LspEvent::HoverResponse {
                                    request_id: req_id as i32,
                                    text: None,
                                });
                            }
                        }
                        Some(PendingRequestKind::CodeAction) => {
                            if let Some(arr) = result.as_array() {
                                let actions: Vec<CodeAction> =
                                    arr.iter().filter_map(parse_code_action_value).collect();
                                let _ = event_tx.send(LspEvent::CodeActions {
                                    request_id: req_id as i32,
                                    actions,
                                });
                            }
                        }
                        Some(PendingRequestKind::Definition) => {
                            let target = parse_definition_target(result);
                            let _ = event_tx.send(LspEvent::DefinitionResponse {
                                request_id: req_id as i32,
                                target,
                            });
                        }
                        Some(PendingRequestKind::Completion) => {
                            let items = parse_completion_items(result);
                            let _ = event_tx.send(LspEvent::CompletionResponse {
                                request_id: req_id as i32,
                                items,
                            });
                        }
                        Some(PendingRequestKind::SignatureHelp) => {
                            let parameters = parse_signature_help_parameters(result);
                            let _ = event_tx.send(LspEvent::SignatureHelpResponse {
                                request_id: req_id as i32,
                                parameters,
                            });
                        }
                        Some(PendingRequestKind::InlayHint) => {
                            let hints = parse_inlay_hints(result);
                            let _ = event_tx.send(LspEvent::InlayHintsResponse {
                                request_id: req_id as i32,
                                hints,
                            });
                        }
                        Some(PendingRequestKind::WorkspaceDiagnostic) => {
                            emit_workspace_diagnostics(result, event_tx, server_name);
                            let _ = event_tx.send(LspEvent::WorkspaceDiagnosticsDone {
                                request_id: req_id as i32,
                            });
                        }
                        None => {
                            if result.get("contents").is_some() {
                                if let Some(hover) = parse_hover_value(result) {
                                    let _ = event_tx.send(LspEvent::HoverResponse {
                                        request_id: req_id as i32,
                                        text: Some(hover),
                                    });
                                }
                            } else if let Some(arr) = result.as_array() {
                                let actions: Vec<CodeAction> =
                                    arr.iter().filter_map(parse_code_action_value).collect();
                                let _ = event_tx.send(LspEvent::CodeActions {
                                    request_id: req_id as i32,
                                    actions,
                                });
                            } else if result.is_null() {
                                let _ = event_tx.send(LspEvent::HoverResponse {
                                    request_id: req_id as i32,
                                    text: None,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
}

fn recv_log_message(body: &[u8], msg: &serde_json::Value) -> String {
    const LARGE_ITEMS_LOG_LIMIT: usize = 80;
    if let Some(items_len) = msg
        .pointer("/result/items")
        .and_then(|value| value.as_array())
        .map(Vec::len)
        .filter(|len| *len > LARGE_ITEMS_LOG_LIMIT)
    {
        let mut compact = msg.clone();
        if let Some(result) = compact
            .get_mut("result")
            .and_then(|value| value.as_object_mut())
        {
            result.insert(
                "items".to_string(),
                serde_json::json!({
                    "omitted": items_len,
                    "reason": "large LSP result"
                }),
            );
        }
        if let Ok(text) = serde_json::to_string(&compact) {
            return format!("[LSP RECV] {text}");
        }
    }
    format!("[LSP RECV] {}", String::from_utf8_lossy(body))
}

// ── Запуск процесса ───────────────────────────────────────────────────────────
