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

/// Сериализует путь в стандартный file URI без ручной замены разделителей.
pub(super) fn path_to_uri(path: &std::path::Path) -> String {
    path_to_uri_for_platform(path, crate::platform::CURRENT_PLATFORM)
}

pub(super) fn path_to_uri_for_platform(
    path: &std::path::Path,
    platform: crate::platform::PlatformKind,
) -> String {
    if platform == crate::platform::PlatformKind::Windows {
        return windows_path_to_uri(path);
    }

    let absolute = crate::platform::canonicalize_or_absolutize(path);
    url::Url::from_file_path(&absolute)
        .map(|url| url.to_string())
        .unwrap_or_else(|_| {
            let mut url = url::Url::parse("file:///").expect("static file URL is valid");
            url.set_path(&absolute.to_string_lossy());
            url.to_string()
        })
}

fn windows_path_to_uri(path: &std::path::Path) -> String {
    let mut raw = path.to_string_lossy().replace('/', "\\");
    if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
        raw = format!(r"\\{rest}");
    } else if let Some(rest) = raw.strip_prefix(r"\\?\") {
        raw = rest.to_string();
    }

    if let Some(unc) = raw.strip_prefix(r"\\") {
        let mut parts = unc.splitn(2, '\\');
        let host = parts.next().unwrap_or_default();
        let tail = parts.next().unwrap_or_default().replace('\\', "/");
        let mut url = url::Url::parse("file://placeholder/").expect("static file URL is valid");
        if url.set_host(Some(host)).is_ok() {
            set_file_url_path(&mut url, &format!("/{tail}"));
            return url.to_string();
        }
    }

    if !crate::platform::windows_path_is_absolute(&raw) {
        let absolute = crate::platform::canonicalize_or_absolutize(path);
        if let Ok(url) = url::Url::from_file_path(absolute) {
            return url.to_string();
        }
    }

    let slash_path = raw.replace('\\', "/");
    let mut url = url::Url::parse("file:///").expect("static file URL is valid");
    set_file_url_path(
        &mut url,
        &format!("/{}", slash_path.trim_start_matches('/')),
    );
    url.to_string()
}

fn set_file_url_path(url: &mut url::Url, path: &str) {
    // `Url::set_path` intentionally preserves percent triplets. A filesystem
    // percent sign is data, so escape it first to keep `%`, `%20`, and malformed
    // sequences round-trippable instead of interpreting them as URL escapes.
    let escaped_percent = path.replace('%', "%25");
    url.set_path(&escaped_percent);
}

pub(super) fn uri_to_path(uri: &str) -> PathBuf {
    uri_to_path_for_platform(uri, crate::platform::CURRENT_PLATFORM)
}

pub(super) fn uri_to_path_for_platform(
    uri: &str,
    platform: crate::platform::PlatformKind,
) -> PathBuf {
    let Ok(url) = url::Url::parse(uri) else {
        return PathBuf::from(uri);
    };
    if url.scheme() != "file" {
        return PathBuf::from(uri);
    }

    if platform != crate::platform::PlatformKind::Windows {
        return url.to_file_path().unwrap_or_else(|_| PathBuf::from(uri));
    }

    let decoded = decode_percent_encoded_path(url.path()).unwrap_or_else(|| url.path().to_string());
    let path = decoded.trim_start_matches('/').replace('/', "\\");
    if let Some(host) = url.host_str().filter(|host| !host.eq_ignore_ascii_case("localhost")) {
        return PathBuf::from(format!(r"\\{host}\{path}"));
    }
    PathBuf::from(path)
}

fn decode_percent_encoded_path(path: &str) -> Option<String> {
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let high = crate::platform::hex_digit(bytes[index + 1])?;
            let low = crate::platform::hex_digit(bytes[index + 2])?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

// ── Кодировщики JSON-RPC сообщений ────────────────────────────────────────────

#[cfg(test)]
pub(super) fn make_initialize(id: i32, workspaces: &[PathBuf]) -> Vec<u8> {
    make_initialize_for_server(LspServerKind::Ruff, id, workspaces)
}

pub(super) fn make_initialize_for_server(
    server: LspServerKind,
    id: i32,
    workspaces: &[PathBuf],
) -> Vec<u8> {
    let root_uri = workspaces.first().map(|workspace| path_to_uri(workspace));
    let workspace_folders = (!workspaces.is_empty()).then(|| {
        workspaces
            .iter()
            .enumerate()
            .map(|(index, workspace)| {
                serde_json::json!({
                    "uri": path_to_uri(workspace),
                    "name": format!("workspace_{index}"),
                })
            })
            .collect::<Vec<_>>()
    });

    let capabilities = match server {
        LspServerKind::Dart => serde_json::json!({
            "workspace": {
                "configuration": true,
                "workspaceFolders": true
            },
            "textDocument": {
                "synchronization": {
                    "dynamicRegistration": false,
                    "willSave": false,
                    "willSaveWaitUntil": false,
                    "didSave": true
                },
                "publishDiagnostics": {
                    "relatedInformation": false,
                    "versionSupport": true,
                    "codeDescriptionSupport": true
                },
                "completion": {
                    "dynamicRegistration": false,
                    "completionItem": {
                        "snippetSupport": false,
                        "labelDetailsSupport": true,
                        "insertReplaceSupport": false
                    }
                },
                "inlayHint": { "dynamicRegistration": false },
                "codeAction": {
                    "dynamicRegistration": false,
                    "codeActionLiteralSupport": {
                        "codeActionKind": {
                            "valueSet": ["quickfix", "source", "source.fixAll", "source.organizeImports"]
                        }
                    }
                }
            }
        }),
        LspServerKind::Ruff | LspServerKind::Ty => serde_json::json!({
            "workspace": {
                "configuration": true,
                "didChangeConfiguration": { "dynamicRegistration": true },
                "didChangeWatchedFiles": {
                    "dynamicRegistration": true,
                    "relativePatternSupport": true
                },
                "workspaceFolders": true
            },
            "textDocument": {
                "synchronization": {
                    "dynamicRegistration": true,
                    "willSave": false,
                    "willSaveWaitUntil": false,
                    "didSave": true
                },
                "publishDiagnostics": {
                    "relatedInformation": false,
                    "versionSupport": true,
                    "codeDescriptionSupport": true
                },
                "completion": {
                    "completionItem": {
                        "snippetSupport": false,
                        "labelDetailsSupport": true,
                        "resolveSupport": {
                            "properties": ["additionalTextEdits", "textEdit", "detail"]
                        }
                    }
                },
                "inlayHint": { "dynamicRegistration": false },
                "codeAction": {
                    "codeActionLiteralSupport": {
                        "codeActionKind": {
                            "valueSet": ["quickfix", "source", "source.fixAll", "source.organizeImports"]
                        }
                    },
                    "resolveSupport": { "properties": ["edit"] }
                }
            }
        }),
    };

    let mut params = serde_json::json!({
        "processId": std::process::id(),
        "clientInfo": { "name": "RRiter", "version": "0.1" },
        "capabilities": capabilities,
        "rootUri": root_uri,
    });
    if let Some(folders) = workspace_folders {
        params["workspaceFolders"] = serde_json::Value::Array(folders);
    }
    if server == LspServerKind::Dart {
        params["initializationOptions"] = serde_json::json!({
            "onlyAnalyzeProjectsWithOpenFiles": true,
            "suggestFromUnimportedLibraries": true,
            "closingLabels": true
        });
    }

    serde_json::to_vec(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "initialize",
        "params": params,
    }))
    .unwrap_or_default()
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

fn make_position_request(
    id: i32,
    method: &str,
    uri: &str,
    line: u32,
    col: u32,
    context: &str,
) -> Vec<u8> {
    format!(
        r#"{{"jsonrpc":"2.0","id":{},"method":"{}","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":{},"character":{}}}{}}}}}"#,
        id,
        method,
        json_escape(uri),
        line,
        col,
        context
    )
    .into_bytes()
}

fn trigger_context(trigger: Option<&str>) -> String {
    if let Some(character) = trigger {
        format!(
            r#","context":{{"triggerKind":2,"triggerCharacter":"{}"}}"#,
            json_escape(character)
        )
    } else {
        String::from(r#","context":{"triggerKind":1}"#)
    }
}

pub(super) fn make_hover(id: i32, uri: &str, line: u32, col: u32) -> Vec<u8> {
    make_position_request(id, "textDocument/hover", uri, line, col, "")
}

pub(super) fn make_definition(id: i32, uri: &str, line: u32, col: u32) -> Vec<u8> {
    make_position_request(id, "textDocument/definition", uri, line, col, "")
}

pub(super) fn make_references(
    id: i32,
    uri: &str,
    line: u32,
    col: u32,
    include_declaration: bool,
) -> Vec<u8> {
    let context = format!(r#","context":{{"includeDeclaration":{include_declaration}}}"#);
    make_position_request(id, "textDocument/references", uri, line, col, &context)
}

pub(super) fn make_prepare_rename(id: i32, uri: &str, line: u32, col: u32) -> Vec<u8> {
    make_position_request(id, "textDocument/prepareRename", uri, line, col, "")
}

pub(super) fn make_rename(
    id: i32,
    uri: &str,
    line: u32,
    col: u32,
    new_name: &str,
) -> Vec<u8> {
    let context = format!(r#","newName":"{}""#, json_escape(new_name));
    make_position_request(id, "textDocument/rename", uri, line, col, &context)
}

pub(super) fn make_completion(
    id: i32,
    uri: &str,
    line: u32,
    col: u32,
    trigger: Option<&str>,
) -> Vec<u8> {
    let context = trigger_context(trigger);
    make_position_request(id, "textDocument/completion", uri, line, col, &context)
}

pub(super) fn make_signature_help(
    id: i32,
    uri: &str,
    line: u32,
    col: u32,
    trigger: Option<&str>,
) -> Vec<u8> {
    let context = trigger_context(trigger);
    make_position_request(id, "textDocument/signatureHelp", uri, line, col, &context)
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

pub(super) fn make_formatting(
    id: i32,
    uri: &str,
    tab_size: u32,
    insert_spaces: bool,
) -> Vec<u8> {
    format!(
        r#"{{"jsonrpc":"2.0","id":{},"method":"textDocument/formatting","params":{{"textDocument":{{"uri":"{}"}},"options":{{"tabSize":{},"insertSpaces":{}}}}}}}"#,
        id,
        json_escape(uri),
        tab_size,
        insert_spaces
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

fn dart_configuration() -> serde_json::Value {
    serde_json::json!({
        "enableSdkFormatter": true,
        "completeFunctionCalls": true,
        "enableSnippets": false,
        "inlayHints": true,
        "documentation": "full",
        "showTodos": true,
        "analysisExcludedFolders": [],
        "closingLabels": true
    })
}

fn configuration_response_for(
    server: LspServerKind,
    item: &serde_json::Value,
) -> serde_json::Value {
    let section = item.get("section").and_then(|value| value.as_str()).unwrap_or("");
    match server {
        LspServerKind::Ty => match section {
            "ty.diagnosticMode" => serde_json::json!("workspace"),
            "ty" | "" => serde_json::json!({ "diagnosticMode": "workspace" }),
            _ => serde_json::json!({}),
        },
        LspServerKind::Dart => match section {
            "dart" | "" => dart_configuration(),
            "dart.enableSdkFormatter" => serde_json::json!(true),
            "dart.lineLength" => serde_json::Value::Null,
            "dart.completeFunctionCalls" => serde_json::json!(true),
            "dart.enableSnippets" => serde_json::json!(false),
            "dart.inlayHints" => serde_json::json!(true),
            "dart.documentation" => serde_json::json!("full"),
            "dart.showTodos" => serde_json::json!(true),
            "dart.analysisExcludedFolders" => serde_json::json!([]),
            "dart.closingLabels" => serde_json::json!(true),
            _ => serde_json::json!({}),
        },
        LspServerKind::Ruff => serde_json::json!({}),
    }
}

fn emit_workspace_diagnostic_report(
    uri: &str,
    report: &serde_json::Value,
    event_tx: &Sender<LspEvent>,
    server: LspServerKind,
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
        .and_then(|value| i32::try_from(value).ok());
    let result_id = report
        .get("resultId")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let _ = event_tx.send(LspEvent::Diagnostics {
        server,
        path: uri_to_path(uri),
        version,
        items,
        result_id,
    });
}

fn emit_workspace_diagnostics(
    result: &serde_json::Value,
    event_tx: &Sender<LspEvent>,
    server: LspServerKind,
) {
    let Some(items) = result.get("items").and_then(|v| v.as_array()) else {
        return;
    };

    for item in items {
        if let Some(uri) = item.get("uri").and_then(|v| v.as_str()) {
            emit_workspace_diagnostic_report(uri, item, event_tx, server);
        }

        if let Some(related) = item.get("relatedDocuments").and_then(|v| v.as_object()) {
            for (uri, report) in related {
                emit_workspace_diagnostic_report(uri, report, event_tx, server);
            }
        }
    }
}

// ── Основной парсер входящих фреймов ─────────────────────────────────────────

#[inline]
fn client_request_id(id: i64) -> Option<i32> {
    i32::try_from(id).ok()
}

fn rpc_reply_id_json(value: Option<&serde_json::Value>) -> Option<String> {
    match value? {
        serde_json::Value::Number(number) => Some(number.to_string()),
        serde_json::Value::String(text) => serde_json::to_string(text).ok(),
        _ => None,
    }
}

#[cfg(test)]
pub(super) fn dispatch_frame(
    body: &[u8],
    event_tx: &Sender<LspEvent>,
    server_name: &'static str,
    out_tx: &Sender<Vec<u8>>,
    pending_requests: &Arc<Mutex<HashMap<i32, PendingRequestKind>>>,
) {
    let server = LspServerKind::from_name(server_name).unwrap_or(LspServerKind::Ruff);
    dispatch_frame_for_server(body, event_tx, server, server_name, out_tx, pending_requests);
}

pub(super) fn dispatch_frame_for_server(
    body: &[u8],
    event_tx: &Sender<LspEvent>,
    server: LspServerKind,
    server_name: &'static str,
    out_tx: &Sender<Vec<u8>>,
    pending_requests: &Arc<Mutex<HashMap<i32, PendingRequestKind>>>,
) {
    let header = match serde_json::from_slice::<RpcHeader<'_>>(body) {
        Ok(header) => header,
        Err(e) => {
            let log_msg = format!("[LSP RECV ERROR] {}: {}", e, String::from_utf8_lossy(body));
            let _ = event_tx.send(LspEvent::Log {
                name: server_name,
                message: log_msg,
            });
            return;
        }
    };
    let header_id = header.id.as_ref().and_then(RpcId::as_i64);
    let header_request_id = header_id.and_then(client_request_id);
    let header_pending_kind = if header.method.is_none() {
        header_request_id.and_then(|request_id| {
            crate::platform::lock_recover(pending_requests).remove(&request_id)
        })
    } else {
        None
    };

    if header.method == Some("textDocument/publishDiagnostics") {
        let _ = event_tx.send(LspEvent::Log {
            name: server_name,
            message: recv_log_message_from_header(
                body,
                header.id.as_ref(),
                header.method,
                None,
            ),
        });
        match parse_publish_diagnostics_frame(body, server) {
            Ok(event) => {
                let _ = event_tx.send(event);
            }
            Err(error) => {
                let _ = event_tx.send(LspEvent::Log {
                    name: server_name,
                    message: format!("[LSP RECV ERROR] invalid publishDiagnostics: {error}"),
                });
            }
        }
        return;
    }

    if matches!(
        header_pending_kind.as_ref(),
        Some(PendingRequestKind::WorkspaceDiagnostic)
    ) {
        let _ = event_tx.send(LspEvent::Log {
            name: server_name,
            message: recv_log_message_from_header(
                body,
                header.id.as_ref(),
                header.method,
                top_result_items_len(body),
            ),
        });
        if header.error.is_none() {
            for event in parse_workspace_diagnostics_frame(body, server) {
                let _ = event_tx.send(event);
            }
        }
        if let Some(request_id) = header_request_id {
            let _ = event_tx.send(LspEvent::WorkspaceDiagnosticsDone { request_id });
        }
        return;
    }

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
    let reply_id = rpc_reply_id_json(msg.get("id"));
    let request_id = msg
        .get("id")
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        })
        .and_then(client_request_id);

    match method {
        Some("textDocument/publishDiagnostics") => {
            if let Some(params) = msg.get("params") {
                if let Some(uri) = params.get("uri").and_then(|v| v.as_str()) {
                    let path = uri_to_path(uri);
                    let version = params
                        .get("version")
                        .and_then(|v| v.as_i64())
                        .and_then(|value| i32::try_from(value).ok());

                    let mut items = Vec::new();
                    if let Some(diags) = params.get("diagnostics").and_then(|v| v.as_array()) {
                        for d in diags {
                            if let Some(diag) = parse_diagnostic_value(d) {
                                items.push(diag);
                            }
                        }
                    }
                    set_missing_diagnostic_sources(&mut items, server);
                    let _ = event_tx.send(LspEvent::Diagnostics {
                        server,
                        path,
                        version,
                        items,
                        result_id: None,
                    });
                }
            }
        }
        Some("dart/textDocument/publishClosingLabels") if server == LspServerKind::Dart => {
            match parse_closing_labels_frame(body, server) {
                Ok(event) => {
                    let _ = event_tx.send(event);
                }
                Err(error) => {
                    let _ = event_tx.send(LspEvent::Log {
                        name: server_name,
                        message: format!("[LSP RECV ERROR] invalid publishClosingLabels: {error}"),
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
            if let Some(req_id) = reply_id.as_deref() {
                let reply = format!(r#"{{"jsonrpc":"2.0","id":{},"result":null}}"#, req_id);
                let _ = out_tx.send(reply.into_bytes());
            }
        }
        Some("workspace/configuration") => {
            if let Some(req_id) = reply_id.as_deref() {
                let objs = if let Some(items) =
                    msg.pointer("/params/items").and_then(|v| v.as_array())
                {
                    let values = items
                        .iter()
                        .map(|item| configuration_response_for(server, item).to_string())
                        .collect::<Vec<_>>();
                    if values.is_empty() {
                        configuration_response_for(server, &serde_json::Value::Null)
                            .to_string()
                    } else {
                        values.join(",")
                    }
                } else {
                    configuration_response_for(server, &serde_json::Value::Null).to_string()
                };
                let reply = format!(r#"{{"jsonrpc":"2.0","id":{},"result":[{}]}}"#, req_id, objs);
                let _ = out_tx.send(reply.into_bytes());
                let _ = event_tx.send(LspEvent::ConfigurationServed { server });
            }
        }
        Some(m) => {
            if let Some(req_id) = reply_id.as_deref() {
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
            if let Some(req_id) = request_id {
                let pending_kind = header_pending_kind.or_else(|| {
                    crate::platform::lock_recover(pending_requests).remove(&req_id)
                });

                if msg.get("error").is_some() {
                    match pending_kind {
                        Some(PendingRequestKind::WorkspaceDiagnostic) => {
                            let _ = event_tx.send(LspEvent::WorkspaceDiagnosticsDone {
                                request_id: req_id,
                            });
                        }
                        Some(PendingRequestKind::InlayHint) => {
                            let _ = event_tx.send(LspEvent::InlayHintsResponse {
                                request_id: req_id,
                                hints: Vec::new(),
                            });
                        }
                        Some(PendingRequestKind::Completion) => {
                            let _ = event_tx.send(LspEvent::CompletionResponse {
                                request_id: req_id,
                                items: Vec::new(),
                                is_incomplete: false,
                            });
                        }
                        Some(PendingRequestKind::SignatureHelp) => {
                            let _ = event_tx.send(LspEvent::SignatureHelpResponse {
                                request_id: req_id,
                                help: LspSignatureHelp::default(),
                            });
                        }
                        Some(PendingRequestKind::References) => {
                            let _ = event_tx.send(LspEvent::ReferencesResponse {
                                request_id: req_id,
                                targets: Vec::new(),
                            });
                        }
                        Some(PendingRequestKind::PrepareRename) => {
                            let _ = event_tx.send(LspEvent::PrepareRenameResponse {
                                request_id: req_id,
                                range: None,
                            });
                        }
                        Some(PendingRequestKind::Rename) => {
                            let _ = event_tx.send(LspEvent::RenameResponse {
                                request_id: req_id,
                                edit: WorkspaceEdit::default(),
                            });
                        }
                        Some(PendingRequestKind::Formatting) => {
                            let _ = event_tx.send(LspEvent::FormattingResponse {
                                request_id: req_id,
                                edits: Vec::new(),
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
                                        request_id: req_id,
                                        text: Some(hover),
                                    });
                                }
                            } else if result.is_null() {
                                let _ = event_tx.send(LspEvent::HoverResponse {
                                    request_id: req_id,
                                    text: None,
                                });
                            }
                        }
                        Some(PendingRequestKind::CodeAction) => {
                            if let Some(arr) = result.as_array() {
                                let actions: Vec<CodeAction> =
                                    arr.iter().filter_map(parse_code_action_value).collect();
                                let _ = event_tx.send(LspEvent::CodeActions {
                                    request_id: req_id,
                                    actions,
                                });
                            }
                        }
                        Some(PendingRequestKind::Definition) => {
                            let target = parse_definition_target(result);
                            let _ = event_tx.send(LspEvent::DefinitionResponse {
                                request_id: req_id,
                                target,
                            });
                        }
                        Some(PendingRequestKind::References) => {
                            let targets = parse_definition_targets(result);
                            let _ = event_tx.send(LspEvent::ReferencesResponse {
                                request_id: req_id,
                                targets,
                            });
                        }
                        Some(PendingRequestKind::PrepareRename) => {
                            let range = if result.is_null() {
                                None
                            } else {
                                parse_prepare_rename_range(result)
                            };
                            let _ = event_tx.send(LspEvent::PrepareRenameResponse {
                                request_id: req_id,
                                range,
                            });
                        }
                        Some(PendingRequestKind::Rename) => {
                            let edit = if result.is_null() {
                                WorkspaceEdit::default()
                            } else {
                                parse_workspace_edit_value(result)
                            };
                            let _ = event_tx.send(LspEvent::RenameResponse {
                                request_id: req_id,
                                edit,
                            });
                        }
                        Some(PendingRequestKind::Completion) => {
                            let (items, is_incomplete) = parse_completion_items(result);
                            let _ = event_tx.send(LspEvent::CompletionResponse {
                                request_id: req_id,
                                items,
                                is_incomplete,
                            });
                        }
                        Some(PendingRequestKind::SignatureHelp) => {
                            let help = parse_signature_help(result);
                            let _ = event_tx.send(LspEvent::SignatureHelpResponse {
                                request_id: req_id,
                                help,
                            });
                        }
                        Some(PendingRequestKind::InlayHint) => {
                            let hints = parse_inlay_hints(result);
                            let _ = event_tx.send(LspEvent::InlayHintsResponse {
                                request_id: req_id,
                                hints,
                            });
                        }
                        Some(PendingRequestKind::Formatting) => {
                            let edits = result
                                .as_array()
                                .map(|items| {
                                    items
                                        .iter()
                                        .filter_map(parse_text_edit_value)
                                        .collect()
                                })
                                .unwrap_or_default();
                            let _ = event_tx.send(LspEvent::FormattingResponse {
                                request_id: req_id,
                                edits,
                            });
                        }
                        Some(PendingRequestKind::WorkspaceDiagnostic) => {
                            emit_workspace_diagnostics(result, event_tx, server);
                            let _ = event_tx.send(LspEvent::WorkspaceDiagnosticsDone {
                                request_id: req_id,
                            });
                        }
                        None => {
                            if result.get("contents").is_some() {
                                if let Some(hover) = parse_hover_value(result) {
                                    let _ = event_tx.send(LspEvent::HoverResponse {
                                        request_id: req_id,
                                        text: Some(hover),
                                    });
                                }
                            } else if let Some(arr) = result.as_array() {
                                let actions: Vec<CodeAction> =
                                    arr.iter().filter_map(parse_code_action_value).collect();
                                let _ = event_tx.send(LspEvent::CodeActions {
                                    request_id: req_id,
                                    actions,
                                });
                            } else if result.is_null() {
                                let _ = event_tx.send(LspEvent::HoverResponse {
                                    request_id: req_id,
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
    const LARGE_BODY_LOG_LIMIT: usize = 16 * 1024;
    if let Some(items_len) = msg
        .pointer("/result/items")
        .and_then(|value| value.as_array())
        .map(Vec::len)
        .filter(|len| *len > LARGE_ITEMS_LOG_LIMIT)
    {
        return recv_log_summary(msg, body.len(), Some(items_len));
    }
    if body.len() > LARGE_BODY_LOG_LIMIT {
        return recv_log_summary(msg, body.len(), None);
    }
    format!("[LSP RECV] {}", String::from_utf8_lossy(body))
}

fn recv_log_message_from_header(
    body: &[u8],
    id: Option<&RpcId<'_>>,
    method: Option<&str>,
    result_items_len: Option<usize>,
) -> String {
    const LARGE_ITEMS_LOG_LIMIT: usize = 80;
    const LARGE_BODY_LOG_LIMIT: usize = 16 * 1024;
    if let Some(items_len) = result_items_len.filter(|len| *len > LARGE_ITEMS_LOG_LIMIT) {
        return recv_log_summary_from_parts(id, method, body.len(), Some(items_len));
    }
    if body.len() > LARGE_BODY_LOG_LIMIT {
        return recv_log_summary_from_parts(id, method, body.len(), None);
    }
    format!("[LSP RECV] {}", String::from_utf8_lossy(body))
}

fn recv_log_summary(
    msg: &serde_json::Value,
    body_bytes: usize,
    omitted_items: Option<usize>,
) -> String {
    let id = msg
        .get("id")
        .and_then(|value| serde_json::to_string(value).ok())
        .unwrap_or_else(|| "null".to_string());
    let method = msg
        .get("method")
        .and_then(|value| value.as_str())
        .map(|method| format!(r#","method":"{}""#, json_escape(method)))
        .unwrap_or_default();
    let items = omitted_items
        .map(|len| format!(r#","items_omitted":{len},"reason":"large LSP result""#))
        .unwrap_or_else(|| r#","reason":"large LSP message""#.to_string());
    format!(r#"[LSP RECV] {{"jsonrpc":"2.0","id":{id}{method},"body_bytes":{body_bytes}{items}}}"#)
}

fn recv_log_summary_from_parts(
    id: Option<&RpcId<'_>>,
    method: Option<&str>,
    body_bytes: usize,
    omitted_items: Option<usize>,
) -> String {
    let id = id
        .map(RpcId::log_json)
        .unwrap_or_else(|| "null".to_string());
    let method = method
        .map(|method| format!(r#","method":"{}""#, json_escape(method)))
        .unwrap_or_default();
    let items = omitted_items
        .map(|len| format!(r#","items_omitted":{len},"reason":"large LSP result""#))
        .unwrap_or_else(|| r#","reason":"large LSP message""#.to_string());
    format!(r#"[LSP RECV] {{"jsonrpc":"2.0","id":{id}{method},"body_bytes":{body_bytes}{items}}}"#)
}

fn top_result_items_len(body: &[u8]) -> Option<usize> {
    #[derive(serde::Deserialize)]
    struct Frame {
        result: Option<ResultItems>,
    }
    #[derive(serde::Deserialize)]
    struct ResultItems {
        items: Option<Vec<serde::de::IgnoredAny>>,
    }
    serde_json::from_slice::<Frame>(body)
        .ok()
        .and_then(|frame| frame.result)
        .and_then(|result| result.items)
        .map(|items| items.len())
}

// ── Запуск процесса ───────────────────────────────────────────────────────────
