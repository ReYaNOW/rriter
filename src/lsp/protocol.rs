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
        result_id: Option<String>,
    },
    ConfigurationServed {
        name: &'static str,
    },
    WorkspaceDiagnosticsDone {
        request_id: i32,
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
        target: Option<DefinitionTarget>,
    },
    CompletionResponse {
        request_id: i32,
        items: Vec<LspCompletionItem>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionTarget {
    pub path: PathBuf,
    pub line: u32,
    pub col: u32,
}

#[derive(Debug, Clone)]
pub struct CodeAction {
    pub title: String,
    pub kind: Option<String>,
    pub edit: Option<WorkspaceEdit>,
    pub code: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LspCompletionItem {
    pub label: String,
    pub kind: crate::highlighter::SymbolKind,
    pub module: Option<String>,
    pub detail: Option<String>,
    pub insert_text: Option<String>,
    pub text_edit: Option<TextChange>,
    pub additional_text_edits: Vec<TextChange>,
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
    Completion {
        id: i32,
        uri: String,
        line: u32,
        col: u32,
        trigger: Option<String>,
    },
    WorkspaceDiagnostic {
        id: i32,
        previous_result_ids_json: String,
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
        r#"{{"jsonrpc":"2.0","id":{id},"method":"initialize","params":{{"processId":{},"clientInfo":{{"name":"RRiter","version":"0.1"}},"capabilities":{{"workspace":{{"configuration":true,"didChangeConfiguration":{{"dynamicRegistration":true}},"didChangeWatchedFiles":{{"dynamicRegistration":true,"relativePatternSupport":true}},"workspaceFolders":true}},"textDocument":{{"synchronization":{{"dynamicRegistration":true,"willSave":false,"willSaveWaitUntil":false,"didSave":true}},"publishDiagnostics":{{"relatedInformation":false,"versionSupport":true,"codeDescriptionSupport":true}},"completion":{{"completionItem":{{"snippetSupport":false,"labelDetailsSupport":true,"resolveSupport":{{"properties":["additionalTextEdits","textEdit","detail"]}}}}}},"codeAction":{{"codeActionLiteralSupport":{{"codeActionKind":{{"valueSet":["quickfix","source","source.fixAll","source.organizeImports"]}}}},"resolveSupport":{{"properties":["edit"]}}}}}}}},"rootUri":{}{workspace_json}}}}}"#,
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

fn parse_completion_text_edit_value(v: &serde_json::Value) -> Option<TextChange> {
    if let Some(change) = parse_text_edit_value(v) {
        return Some(change);
    }
    let replace = v.get("replace").or_else(|| v.get("insert"))?;
    let start = replace.get("start")?;
    let end_r = replace.get("end")?;
    let new_text = v.get("newText").and_then(|t| t.as_str()).unwrap_or("");
    Some(TextChange {
        start_line: start.get("line")?.as_u64()? as u32,
        start_col: start.get("character")?.as_u64()? as u32,
        end_line: end_r.get("line")?.as_u64()? as u32,
        end_col: end_r.get("character")?.as_u64()? as u32,
        new_text: new_text.to_string(),
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

fn definition_position(v: &serde_json::Value) -> Option<(u32, u32)> {
    let line = v.get("line")?.as_u64()? as u32;
    let col = v.get("character")?.as_u64()? as u32;
    Some((line, col))
}

pub(super) fn parse_definition_target(v: &serde_json::Value) -> Option<DefinitionTarget> {
    if let Some(uri) = v.get("uri").and_then(|u| u.as_str()) {
        let (line, col) = v
            .pointer("/range/start")
            .and_then(definition_position)
            .unwrap_or((0, 0));
        return Some(DefinitionTarget {
            path: uri_to_path(uri),
            line,
            col,
        });
    }
    if let Some(uri) = v.get("targetUri").and_then(|u| u.as_str()) {
        let (line, col) = v
            .pointer("/targetSelectionRange/start")
            .or_else(|| v.pointer("/targetRange/start"))
            .and_then(definition_position)
            .unwrap_or((0, 0));
        return Some(DefinitionTarget {
            path: uri_to_path(uri),
            line,
            col,
        });
    }
    if let Some(arr) = v.as_array() {
        for item in arr {
            if let Some(target) = parse_definition_target(item) {
                return Some(target);
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

fn completion_kind(kind: Option<u64>) -> crate::highlighter::SymbolKind {
    match kind {
        Some(2 | 3 | 4) => crate::highlighter::SymbolKind::Function,
        Some(10) => crate::highlighter::SymbolKind::Property,
        Some(5 | 6 | 12 | 13 | 21) => crate::highlighter::SymbolKind::Variable,
        Some(7 | 8 | 22 | 25) => crate::highlighter::SymbolKind::Class,
        Some(9) => crate::highlighter::SymbolKind::Module,
        Some(14) => crate::highlighter::SymbolKind::Keyword,
        _ => crate::highlighter::SymbolKind::Unknown,
    }
}

fn refine_completion_kind(
    kind: crate::highlighter::SymbolKind,
    label: &str,
    detail: Option<&str>,
    insert_text: Option<&str>,
) -> crate::highlighter::SymbolKind {
    let Some(detail) = detail else {
        return if label.ends_with('=') || insert_text.is_some_and(|text| text.contains('=')) {
            crate::highlighter::SymbolKind::Parameter
        } else {
            kind
        };
    };
    if detail.starts_with("(parameter)")
        || label.ends_with('=')
        || insert_text.is_some_and(|text| text.contains('='))
    {
        crate::highlighter::SymbolKind::Parameter
    } else if detail.starts_with("(variable)") {
        crate::highlighter::SymbolKind::Variable
    } else if detail.starts_with("(property)") || detail.starts_with("(field)") {
        crate::highlighter::SymbolKind::Property
    } else if detail.starts_with("(function)")
        || detail.starts_with("(method)")
        || detail.starts_with("def ")
        || detail.starts_with("async def ")
    {
        crate::highlighter::SymbolKind::Function
    } else if detail.starts_with("class ") || detail.starts_with("type[") {
        crate::highlighter::SymbolKind::Class
    } else {
        kind
    }
}

fn completion_module(
    v: &serde_json::Value,
    kind: &crate::highlighter::SymbolKind,
    detail: Option<&str>,
) -> Option<String> {
    let label = v
        .get("label")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if let Some(full_name) = v
        .pointer("/data/fullName")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if let Some(owner) = owner_from_completion_detail(label, full_name) {
            return Some(owner);
        }
        return Some(full_name.to_string());
    }
    if let Some(owner) = v
        .pointer("/data/owner")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(owner.to_string());
    }
    let detail_owner = detail.and_then(|detail| owner_from_completion_detail(label, detail));
    let detail_is_field_type = detail.is_some_and(|detail| {
        detail.starts_with("(variable)")
            || detail.starts_with("(parameter)")
            || detail.starts_with("(property)")
            || detail.starts_with("(field)")
    });
    if detail_is_field_type {
        return detail_owner;
    }
    if !matches!(
        kind,
        crate::highlighter::SymbolKind::Variable
            | crate::highlighter::SymbolKind::Parameter
            | crate::highlighter::SymbolKind::Property
    ) {
        if let Some(module) = v
            .pointer("/data/module")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if let Some(owner) = detail_owner.as_deref().filter(|owner| {
                looks_like_python_module_path(module) && !module.ends_with(&format!(".{owner}"))
            }) {
                return Some(format!("{module}.{owner}"));
            }
            return Some(module.to_string());
        }
    }
    if let Some(owner) = detail_owner {
        return Some(owner);
    }
    if let Some(desc) = v
        .pointer("/labelDetails/description")
        .and_then(|value| value.as_str())
        .filter(|s| !s.trim().is_empty())
    {
        if matches!(
            kind,
            crate::highlighter::SymbolKind::Variable
                | crate::highlighter::SymbolKind::Parameter
                | crate::highlighter::SymbolKind::Property
        ) && !looks_like_python_module_path(desc)
        {
            return None;
        }
        return completion_description_source(desc);
    }
    None
}

fn looks_like_python_module_path(text: &str) -> bool {
    let s = text.trim();
    s.contains('.')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

fn completion_description_source(desc: &str) -> Option<String> {
    let desc = desc.trim();
    if desc.is_empty()
        || desc.contains('/')
        || desc.contains('\\')
        || desc.contains('|')
        || desc.contains('[')
        || desc.contains(']')
        || desc.contains("->")
        || desc.starts_with("def ")
        || desc.starts_with("async def ")
        || desc.starts_with("overload[")
        || desc.starts_with('(')
        || !desc
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
        || matches!(
            desc,
            "Any"
                | "None"
                | "bool"
                | "bytes"
                | "dict"
                | "float"
                | "int"
                | "list"
                | "set"
                | "str"
                | "tuple"
                | "type"
        )
    {
        None
    } else {
        Some(desc.to_string())
    }
}

fn completion_detail(v: &serde_json::Value) -> Option<String> {
    if let Some(label_detail) = v
        .pointer("/labelDetails/detail")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(label_detail.to_string());
    }
    if let Some(detail) = v
        .get("detail")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(detail.to_string());
    }
    if let Some(doc) = v.get("documentation") {
        if let Some(s) = doc.as_str().map(str::trim).filter(|s| !s.is_empty()) {
            return Some(s.to_string());
        }
        if let Some(s) = doc
            .get("value")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Some(s.to_string());
        }
    }
    None
}

fn owner_from_completion_detail(label: &str, detail: &str) -> Option<String> {
    if label.is_empty() {
        return None;
    }
    let needle = format!(".{label}");
    let idx = detail.find(&needle)?;
    let before = &detail[..idx];
    let owner_start = before
        .char_indices()
        .rev()
        .find(|(_, ch)| !(ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '.'))
        .map(|(idx, ch)| idx + ch.len_utf8())
        .unwrap_or(0);
    let owner = before[owner_start..].trim().trim_matches('`');
    (!owner.is_empty()).then(|| owner.to_string())
}

pub(super) fn parse_completion_item_value(v: &serde_json::Value) -> Option<LspCompletionItem> {
    let label = v.get("label")?.as_str()?.to_string();
    let insert_text = v
        .get("insertText")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .or_else(|| {
            v.pointer("/textEdit/newText")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        });
    let text_edit = v.get("textEdit").and_then(parse_completion_text_edit_value);
    let additional_text_edits = v
        .get("additionalTextEdits")
        .and_then(|value| value.as_array())
        .map(|items| items.iter().filter_map(parse_text_edit_value).collect())
        .unwrap_or_default();

    let detail = completion_detail(v);
    let kind = refine_completion_kind(
        completion_kind(v.get("kind").and_then(|value| value.as_u64())),
        &label,
        detail.as_deref(),
        insert_text.as_deref(),
    );
    Some(LspCompletionItem {
        label,
        module: completion_module(v, &kind, detail.as_deref()),
        kind,
        detail,
        insert_text,
        text_edit,
        additional_text_edits,
    })
}

pub(super) fn parse_completion_items(result: &serde_json::Value) -> Vec<LspCompletionItem> {
    let items = if let Some(arr) = result.as_array() {
        arr
    } else if let Some(arr) = result.get("items").and_then(|value| value.as_array()) {
        arr
    } else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(parse_completion_item_value)
        .collect()
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
                    if matches!(pending_kind, Some(PendingRequestKind::WorkspaceDiagnostic)) {
                        let _ = event_tx.send(LspEvent::WorkspaceDiagnosticsDone {
                            request_id: req_id as i32,
                        });
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

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod protocol_tests;
