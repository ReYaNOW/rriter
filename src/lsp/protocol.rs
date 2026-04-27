use super::hover::PendingRequestKind;
use super::*;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use tree_sitter::StreamingIterator;

pub fn highlight_diagnostic_message(msg: &str) -> Vec<crate::highlighter::ColorSpan> {
    let mut spans = Vec::new();
    let mut backtick_ranges = Vec::new();
    let mut in_tick = false;
    let mut tick_start = 0;

    for (offset, c) in msg.char_indices() {
        if c == '`' {
            if in_tick {
                backtick_ranges.push((tick_start, offset));
                in_tick = false;
            } else {
                tick_start = offset + 1;
                in_tick = true;
            }
            spans.push(crate::highlighter::ColorSpan {
                start: offset,
                end: offset + c.len_utf8(),
                color: [0.6, 0.6, 0.65, 1.0],
            });
        } else if c == '├' || c == '─' || c == '│' || c == '└' {
            spans.push(crate::highlighter::ColorSpan {
                start: offset,
                end: offset + c.len_utf8(),
                color: [0.45, 0.45, 0.50, 1.0],
            });
        }
    }

    if !backtick_ranges.is_empty() {
        crate::languages::python::TS_DIAG_PARSER.with(|p_cell| {
            crate::languages::python::TS_DIAG_QUERY.with(|q_cell| {
                crate::languages::python::TS_DIAG_CURSOR.with(|c_cell| {
                    let mut parser = p_cell.borrow_mut();
                    let query_opt = q_cell.borrow();
                    let mut cursor = c_cell.borrow_mut();

                    if let Some(query) = query_opt.as_ref() {
                        for &(start, end) in &backtick_ranges {
                            if start >= end {
                                continue;
                            }
                            let code = &msg[start..end];
                            if let Some(tree) = parser.parse(code, None) {
                                let mut matches =
                                    cursor.matches(query, tree.root_node(), code.as_bytes());
                                while let Some(m) = matches.next() {
                                    for cap in m.captures {
                                        let name = query.capture_names()[cap.index as usize];
                                        let color = match name {
                                            "property" | "variable" => [0.972, 0.972, 0.949, 1.0],
                                            "string" => [0.945, 0.980, 0.549, 1.0],
                                            "type" | "class_name" => [0.545, 0.913, 0.992, 1.0],
                                            "keyword.control" | "keyword" | "operator" => {
                                                [1.0, 0.474, 0.776, 1.0]
                                            }
                                            "function" | "py_function" | "py_builtin_or_func" => {
                                                [0.313, 0.980, 0.482, 1.0]
                                            }
                                            "number" => [0.741, 0.576, 0.976, 1.0],
                                            "comment" => [0.384, 0.447, 0.643, 1.0],
                                            _ => [0.972, 0.972, 0.949, 1.0],
                                        };
                                        if color != [0.972, 0.972, 0.949, 1.0] {
                                            spans.push(crate::highlighter::ColorSpan {
                                                start: start + cap.node.start_byte(),
                                                end: start + cap.node.end_byte(),
                                                color,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                })
            })
        });
    }

    spans.sort_unstable_by_key(|s| s.start);
    spans
}

/// Одна замена текста (из workspace/applyEdit или codeAction)
#[derive(Debug, Clone)]
pub struct TextChange {
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub new_text: String,
}

/// Набор правок по файлам
#[derive(Debug, Clone, Default)]
pub struct WorkspaceEdit {
    pub changes: HashMap<PathBuf, Vec<TextChange>>,
}

/// Событие от LSP-сервера → главный поток
#[derive(Debug)]
pub enum LspEvent {
    Log {
        name: &'static str,
        message: String,
    },
    /// Диагностика для файла (ошибки/предупреждения от ruff)
    Diagnostics {
        server_name: &'static str,
        path: PathBuf,
        #[allow(dead_code)]
        version: Option<i32>,
        items: Vec<Diagnostic>,
    },
    /// Ответ на запрос codeAction (исправления от ruff)
    CodeActions {
        request_id: i32,
        actions: Vec<CodeAction>,
    },
    /// Сервер готов принимать запросы
    ServerReady,
    /// Статус сервера изменился
    StatusChanged {
        #[allow(dead_code)]
        name: &'static str,
        status: LspServerStatus,
    },
    HoverResponse {
        request_id: i32,
        text: Option<String>,
    },
    DefinitionResponse {
        request_id: i32,
        path: Option<PathBuf>,
    },
}

#[derive(Debug, Clone)]
pub struct CodeAction {
    pub title: String,
    pub kind: Option<String>,
    pub edit: Option<WorkspaceEdit>,
    pub code: Option<String>,
}

// ── Конфигурация LSP-серверов ─────────────────────────────────────────────────

pub(super) struct LspServerDef {
    pub(super) program: &'static str,
    pub(super) args: &'static [&'static str],
    pub(super) language_id: &'static str,
    #[allow(dead_code)]
    pub(super) extensions: &'static [&'static str],
}

pub(super) const RUFF_SERVER: LspServerDef = LspServerDef {
    program: "ruff",
    args: &["server"],
    language_id: "python",
    extensions: &["py"],
};

pub(super) const TY_SERVER: LspServerDef = LspServerDef {
    program: "ty",
    args: &["server"],
    language_id: "python",
    extensions: &["py"],
};

// ── Внутренние команды main → supervisor ─────────────────────────────────────

pub(super) enum Cmd {
    /// Перезапустить сервер
    Restart,
    /// Открыть файл (didOpen)
    Open {
        uri: String,
        lang: &'static str,
        version: i32,
        text: String,
    },
    /// Изменить файл (didChange, полный текст)
    Change {
        uri: String,
        version: i32,
        text: String,
    },
    /// Закрыть файл (didClose)
    Close {
        #[allow(dead_code)]
        uri: String,
    },
    /// Запросить codeActions для позиции
    CodeAction {
        id: i32,
        uri: String,
        start_line: u32,
        start_col: u32,
        end_line: u32,
        end_col: u32,
        /// JSON-encoded массив диагностик для контекста (для ruff это важно)
        diagnostics_json: String,
        only: Option<Vec<String>>,
    },
    Shutdown,
    Hover {
        id: i32,
        uri: String,
        line: u32,
        col: u32,
    },
    Definition {
        id: i32,
        uri: String,
        line: u32,
        col: u32,
    },
}

// ── Конвертация byte-offset → LSP Position ───────────────────────────────────

/// Конвертирует байтовый offset в LSP Position {line, character}.
/// `line_offsets[i]` = байтовый offset начала строки i.
/// LSP character = UTF-16 code units (для ASCII = байты).
#[inline]
#[allow(dead_code)]
pub fn offset_to_lsp_pos(text: &str, offset: usize, line_offsets: &[usize]) -> (u32, u32) {
    let offset = offset.min(text.len());
    // Бинарный поиск строки
    let line = line_offsets
        .partition_point(|&o| o <= offset)
        .saturating_sub(1);
    let line_start = line_offsets.get(line).copied().unwrap_or(0);
    let col_bytes = offset.saturating_sub(line_start);

    // Считаем UTF-16 единицы (для ASCII — тривиально; для Unicode — точно)
    let line_slice_end = (line_start + col_bytes).min(text.len());
    let line_slice = text.get(line_start..line_slice_end).unwrap_or("");
    let utf16_col: u32 = line_slice.chars().map(|c| c.len_utf16() as u32).sum();

    (line as u32, utf16_col)
}

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
        r#"{{"jsonrpc":"2.0","id":{id},"method":"initialize","params":{{"processId":{},"clientInfo":{{"name":"RRiter","version":"0.1"}},"capabilities":{{"workspace":{{"configuration":true,"didChangeConfiguration":{{"dynamicRegistration":true}},"didChangeWatchedFiles":{{"dynamicRegistration":true,"relativePatternSupport":true}},"workspaceFolders":true}},"textDocument":{{"synchronization":{{"dynamicRegistration":true,"willSave":false,"willSaveWaitUntil":false,"didSave":true}},"publishDiagnostics":{{"relatedInformation":false,"versionSupport":true,"codeDescriptionSupport":true}},"codeAction":{{"codeActionLiteralSupport":{{"codeActionKind":{{"valueSet":["quickfix","source","source.fixAll","source.organizeImports"]}}}},"resolveSupport":{{"properties":["edit"]}}}}}}}},"rootUri":{}{workspace_json}}}}}"#,
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

// ── Парсинг входящих JSON-RPC сообщений ──────────────────────────────────────

/// Минимальный value-tree для парсинга LSP ответов без полной serde-схемы.
/// Используем только базовый JSON-парсинг.

pub(super) fn parse_diagnostic_value(v: &serde_json::Value) -> Option<Diagnostic> {
    let range = v.get("range")?;
    let start = range.get("start")?;
    let end = range.get("end")?;

    let sl = start.get("line")?.as_u64()? as u32;
    let sc = start.get("character")?.as_u64()? as u32;
    let el = end.get("line")?.as_u64()? as u32;
    let ec = end.get("character")?.as_u64()? as u32;

    let severity = match v.get("severity").and_then(|s| s.as_u64()).unwrap_or(1) {
        1 => DiagSeverity::Error,
        2 => DiagSeverity::Warning,
        3 => DiagSeverity::Info,
        _ => DiagSeverity::Hint,
    };

    let mut message = v
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .to_string();

    if message.contains("info: ") {
        let mut clean_msg = String::with_capacity(message.len());
        for line in message.lines() {
            let mut l = line;
            if l.starts_with("info: ") {
                l = &l[6..];
            }
            clean_msg.push_str(l);
            clean_msg.push('\n');
        }
        message = clean_msg.trim_end().to_string();
    }

    let code = v.get("code").and_then(|c| {
        if let Some(s) = c.as_str() {
            Some(s.to_string())
        } else if let Some(n) = c.as_u64() {
            Some(n.to_string())
        } else {
            None
        }
    });

    let source = v
        .get("source")
        .and_then(|s| s.as_str())
        .map(|s| s.to_string());

    let code_href = v
        .get("codeDescription")
        .and_then(|cd| cd.get("href"))
        .and_then(|h| h.as_str())
        .map(|s| s.to_string());

    let mut tags = Vec::new();
    if let Some(tags_arr) = v.get("tags").and_then(|t| t.as_array()) {
        for t in tags_arr {
            if let Some(tag_id) = t.as_u64() {
                tags.push(tag_id as u32);
            }
        }
    }

    let mut quickfixes = Vec::new();
    if let Some(data) = v.get("data") {
        if let Some(title) = data.get("title").and_then(|t| t.as_str()) {
            if let Some(edits_arr) = data.get("edits").and_then(|e| e.as_array()) {
                let mut edits = Vec::new();
                for e in edits_arr {
                    if let Some(tc) = parse_text_edit_value(e) {
                        edits.push(tc);
                    }
                }
                if !edits.is_empty() {
                    quickfixes.push(QuickFix {
                        title: title.to_string(),
                        edits,
                    });
                }
            }
        }
    }

    message = message
        .replace("\\n", "\n")
        .replace("\\t", "    ")
        .replace('\r', "");

    Some(Diagnostic {
        start_line: sl,
        start_col: sc,
        end_line: el,
        end_col: ec,
        severity,
        code,
        code_href,
        message,
        source,
        quickfixes,
        tags,
        spans: Vec::new(),
    })
}

pub(super) fn parse_text_edit_value(v: &serde_json::Value) -> Option<TextChange> {
    let range = v.get("range")?;
    let start = range.get("start")?;
    let end_r = range.get("end")?;

    let sl = start.get("line")?.as_u64()? as u32;
    let sc = start.get("character")?.as_u64()? as u32;
    let el = end_r.get("line")?.as_u64()? as u32;
    let ec = end_r.get("character")?.as_u64()? as u32;

    let new_text = v
        .get("newText")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();

    Some(TextChange {
        start_line: sl,
        start_col: sc,
        end_line: el,
        end_col: ec,
        new_text,
    })
}

pub(super) fn parse_workspace_edit_value(v: &serde_json::Value) -> WorkspaceEdit {
    let mut edit = WorkspaceEdit::default();

    if let Some(changes) = v.get("changes").and_then(|c| c.as_object()) {
        for (uri, edits) in changes {
            let path = uri_to_path(uri);
            if let Some(arr) = edits.as_array() {
                let parsed_edits: Vec<TextChange> =
                    arr.iter().filter_map(parse_text_edit_value).collect();
                if !parsed_edits.is_empty() {
                    edit.changes.entry(path).or_default().extend(parsed_edits);
                }
            }
        }
    }

    if let Some(doc_changes) = v.get("documentChanges").and_then(|d| d.as_array()) {
        for item in doc_changes {
            if let Some(td) = item.get("textDocument") {
                if let Some(uri) = td.get("uri").and_then(|u| u.as_str()) {
                    let path = uri_to_path(uri);
                    if let Some(edits) = item.get("edits").and_then(|e| e.as_array()) {
                        let parsed_edits: Vec<TextChange> =
                            edits.iter().filter_map(parse_text_edit_value).collect();
                        if !parsed_edits.is_empty() {
                            edit.changes.entry(path).or_default().extend(parsed_edits);
                        }
                    }
                }
            }
        }
    }

    edit
}

pub(super) fn parse_hover_value(v: &serde_json::Value) -> Option<String> {
    let contents = v.get("contents")?;
    if let Some(s) = contents.as_str() {
        return Some(s.to_string());
    }
    if let Some(obj) = contents.as_object() {
        if let Some(val) = obj.get("value").and_then(|v| v.as_str()) {
            return Some(val.to_string());
        }
    }
    if let Some(arr) = contents.as_array() {
        let mut out = String::new();
        for item in arr {
            if let Some(s) = item.as_str() {
                out.push_str(s);
                out.push('\n');
            } else if let Some(obj) = item.as_object() {
                if let Some(val) = obj.get("value").and_then(|v| v.as_str()) {
                    out.push_str(val);
                    out.push('\n');
                }
            }
        }
        return Some(out.trim_end().to_string());
    }
    None
}

pub(super) fn parse_definition_path(v: &serde_json::Value) -> Option<PathBuf> {
    if let Some(uri) = v.get("uri").and_then(|u| u.as_str()) {
        return Some(uri_to_path(uri));
    }
    if let Some(uri) = v.get("targetUri").and_then(|u| u.as_str()) {
        return Some(uri_to_path(uri));
    }
    if let Some(arr) = v.as_array() {
        for item in arr {
            if let Some(path) = parse_definition_path(item) {
                return Some(path);
            }
        }
    }
    None
}

pub(super) fn parse_code_action_value(v: &serde_json::Value) -> Option<CodeAction> {
    let title = v
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();
    let kind = v
        .get("kind")
        .and_then(|k| k.as_str())
        .map(|s| s.to_string());
    let edit = v.get("edit").map(parse_workspace_edit_value);

    let mut code = None;
    if let Some(diags) = v.get("diagnostics").and_then(|d| d.as_array()) {
        if let Some(first) = diags.first() {
            if let Some(c) = first.get("code") {
                if let Some(s) = c.as_str() {
                    code = Some(s.to_string());
                } else if let Some(n) = c.as_u64() {
                    code = Some(n.to_string());
                }
            }
        }
    }

    Some(CodeAction {
        title,
        kind,
        edit,
        code,
    })
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

    let log_msg = format!("[LSP RECV] {}", String::from_utf8_lossy(body));
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
                let mut count = 1;
                if let Some(items) = msg.pointer("/params/items").and_then(|v| v.as_array()) {
                    count = items.len().max(1);
                }
                let config_obj = r#"{}"#;
                let objs = vec![config_obj; count].join(",");
                let reply = format!(r#"{{"jsonrpc":"2.0","id":{},"result":[{}]}}"#, req_id, objs);
                let _ = out_tx.send(reply.into_bytes());
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
                if let Some(result) = msg.get("result") {
                    let pending_kind = pending_requests
                        .lock()
                        .ok()
                        .and_then(|mut p| p.remove(&(req_id as i32)));
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
                            let path = parse_definition_path(result);
                            let _ = event_tx.send(LspEvent::DefinitionResponse {
                                request_id: req_id as i32,
                                path,
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

// ── Запуск процесса ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
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

        let open = String::from_utf8(make_did_open(&uri, "python", 2, "x = \"q\"\n")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&open).unwrap();
        assert_eq!(parsed["params"]["textDocument"]["languageId"], "python");
        assert_eq!(parsed["params"]["textDocument"]["text"], "x = \"q\"\n");
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
        assert_eq!(diag.message, "remove unused import\nnext");
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
            if !matches!(event, LspEvent::Log { .. }) {
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
            init["params"]["capabilities"]["textDocument"]["codeAction"]
                ["codeActionLiteralSupport"]["codeActionKind"]["valueSet"][2],
            "source.fixAll"
        );

        let init_without_workspace: serde_json::Value =
            serde_json::from_slice(&make_initialize(43, &[])).unwrap();
        assert!(init_without_workspace["params"]["rootUri"].is_null());
        assert!(init_without_workspace["params"]
            .get("workspaceFolders")
            .is_none());

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
            serde_json::from_slice(&make_code_action(99, uri, 1, 2, 3, 4, "[]", Some(&only)))
                .unwrap();
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
        assert_eq!(diag.message, "raw    text");
        assert!(parse_diagnostic_value(&serde_json::json!({})).is_none());

        assert_eq!(
            parse_hover_value(&serde_json::json!({"contents": "plain"})).as_deref(),
            Some("plain")
        );
        assert_eq!(
            parse_hover_value(
                &serde_json::json!({"contents": {"kind": "markdown", "value": "obj"}})
            )
            .as_deref(),
            Some("obj")
        );
        assert_eq!(
            parse_hover_value(&serde_json::json!({"contents": [123, {"value": "kept"}, "tail"]}))
                .as_deref(),
            Some("kept\ntail")
        );
        assert_eq!(
            parse_definition_path(&serde_json::json!([{"targetUri": "file:///tmp/target.py"}])),
            Some(PathBuf::from("/tmp/target.py"))
        );

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
            } => {
                assert_eq!(server_name, "ruff");
                assert_eq!(path, PathBuf::from("/tmp/a.py"));
                assert_eq!(version, Some(3));
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].message, "boom");
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
            LspEvent::DefinitionResponse { request_id, path } => {
                assert_eq!(request_id, 2);
                assert_eq!(path, Some(PathBuf::from("/tmp/definition.py")));
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
}
