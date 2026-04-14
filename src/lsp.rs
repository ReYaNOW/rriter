// src/lsp.rs
// Быстрый LSP-клиент для RRiter.
// Поддерживает: ruff (Python). Расширяется через LspServerDef.
//
// Архитектура:
//   Main Thread ──Cmd──▶ Supervisor Thread ──bytes──▶ Writer Thread ──▶ stdin
//                  ◀──LspEvent──   ◀──LspEvent── Reader Thread ◀── stdout
//
// Supervisor: владеет Child-процессом, при краше — перезапускает (с delay).
// Writer/Reader: легковесные треды, по одному на I/O направление.
// При рестарте: supervisor пересоздаёт writer+reader, заново отправляет
//   initialize + didOpen для текущего файла.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};

use std::thread;
use std::time::Duration;

// ── Atomic request ID ─────────────────────────────────────────────────────────

static NEXT_ID: AtomicI32 = AtomicI32::new(1);

#[inline(always)]
fn next_id() -> i32 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

// ── Публичные типы ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LspServerStatus {
    Starting,
    Running,
    Crashed,
    Disabled,
}

#[derive(Debug, Clone)]
pub struct LspServerInfo {
    pub name: &'static str,
    pub status: LspServerStatus,
    pub logs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// 0-based
    pub start_line: u32,
    /// 0-based, UTF-16 code units (для ASCII = байтовый столбец)
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub severity: DiagSeverity,
    /// Код ошибки (например "E501", "F401")
    pub code: Option<String>,
    pub message: String,
    pub source: Option<String>,
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
}

#[derive(Debug, Clone)]
pub struct CodeAction {
    pub title: String,
    pub kind: Option<String>,
    pub edit: Option<WorkspaceEdit>,
}

// ── Конфигурация LSP-серверов ─────────────────────────────────────────────────

struct LspServerDef {
    program: &'static str,
    args: &'static [&'static str],
    language_id: &'static str,
    #[allow(dead_code)]
    extensions: &'static [&'static str],
}

const RUFF_SERVER: LspServerDef = LspServerDef {
    program: "ruff",
    args: &["server"],
    language_id: "python",
    extensions: &["py"],
};

// ── Внутренние команды main → supervisor ─────────────────────────────────────

enum Cmd {
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
    Close { #[allow(dead_code)] uri: String },
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
    },
    Shutdown,
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
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = std::fmt::Write::write_fmt(
                    &mut out,
                    format_args!("\\u{:04x}", c as u32),
                );
            }
            c => out.push(c),
        }
    }
    out
}

/// Сериализует путь → file:// URI
fn path_to_uri(path: &str) -> String {
    format!("file://{}", path)
}

fn uri_to_path(uri: &str) -> PathBuf {
    let s = uri.strip_prefix("file://").unwrap_or(uri);
    PathBuf::from(s)
}

// ── Кодировщики JSON-RPC сообщений ────────────────────────────────────────────

fn make_initialize(id: i32, workspace: Option<&Path>) -> Vec<u8> {
    let (root_uri_json, workspace_json) = if let Some(ws) = workspace {
        let uri = path_to_uri(&ws.to_string_lossy());
        let escaped_uri = json_escape(&uri);
        (
            format!(r#""{}""#, escaped_uri),
            format!(
                r#","workspaceFolders":[{{"uri":"{}","name":"workspace"}}]"#,
                escaped_uri
            )
        )
    } else {
        (String::from("null"), String::new())
    };

    let body = format!(
        r#"{{"jsonrpc":"2.0","id":{id},"method":"initialize","params":{{"processId":{},"clientInfo":{{"name":"RRiter","version":"0.1"}},"capabilities":{{"workspace":{{"configuration":true,"didChangeConfiguration":{{"dynamicRegistration":true}},"didChangeWatchedFiles":{{"dynamicRegistration":true}},"workspaceFolders":true}},"textDocument":{{"synchronization":{{"dynamicRegistration":true,"willSave":false,"willSaveWaitUntil":false,"didSave":true}},"publishDiagnostics":{{"relatedInformation":false,"versionSupport":true,"codeDescriptionSupport":true}},"codeAction":{{"codeActionLiteralSupport":{{"codeActionKind":{{"valueSet":["quickfix","source","source.fixAll","source.organizeImports"]}}}},"resolveSupport":{{"properties":["edit"]}}}}}}}},"rootUri":{}{workspace_json}}}}}"#,
        std::process::id(),
        root_uri_json
    );
    body.into_bytes()
}

fn make_initialized() -> Vec<u8> {
    br#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#.to_vec()
}

fn make_did_open(uri: &str, lang: &str, version: i32, text: &str) -> Vec<u8> {
    let body = format!(
        r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":"{}","languageId":"{}","version":{},"text":"{}"}}}}}}"#,
        json_escape(uri),
        lang,
        version,
        json_escape(text)
    );
    body.into_bytes()
}

fn make_did_change_full(uri: &str, version: i32, text: &str) -> Vec<u8> {
    let body = format!(
        r#"{{"jsonrpc":"2.0","method":"textDocument/didChange","params":{{"textDocument":{{"uri":"{}","version":{}}},"contentChanges":[{{"text":"{}"}}]}}}}"#,
        json_escape(uri),
        version,
        json_escape(text)
    );
    body.into_bytes()
}

fn make_did_close(uri: &str) -> Vec<u8> {
    let body = format!(
        r#"{{"jsonrpc":"2.0","method":"textDocument/didClose","params":{{"textDocument":{{"uri":"{}"}}}}}}"#,
        json_escape(uri)
    );
    body.into_bytes()
}

fn make_code_action(
    id: i32,
    uri: &str,
    sl: u32, sc: u32, el: u32, ec: u32,
    diag_json: &str,
) -> Vec<u8> {
    let body = format!(
        r#"{{"jsonrpc":"2.0","id":{id},"method":"textDocument/codeAction","params":{{"textDocument":{{"uri":"{}"}},"range":{{"start":{{"line":{sl},"character":{sc}}},"end":{{"line":{el},"character":{ec}}}}},"context":{{"diagnostics":{diag_json},"only":["quickfix","source.fixAll"]}}}}}}"#,
        json_escape(uri)
    );
    body.into_bytes()
}

fn make_shutdown(id: i32) -> Vec<u8> {
    format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"shutdown","params":null}}"#).into_bytes()
}

fn make_exit() -> Vec<u8> {
    br#"{"jsonrpc":"2.0","method":"exit","params":null}"#.to_vec()
}

// ── Запись JSON-RPC фрейма ────────────────────────────────────────────────────

fn write_frame(writer: &mut BufWriter<std::process::ChildStdin>, body: &[u8]) -> bool {
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    writer.write_all(header.as_bytes()).is_ok()
        && writer.write_all(body).is_ok()
        && writer.flush().is_ok()
}

// ── Парсинг входящих JSON-RPC сообщений ──────────────────────────────────────

/// Минимальный value-tree для парсинга LSP ответов без полной serde-схемы.
/// Используем только базовый JSON-парсинг.

fn get_str_field<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let search = format!("\"{}\":", key);
    let start = json.find(&search)? + search.len();
    let rest = json[start..].trim_start();
    if rest.starts_with('"') {
        let inner = &rest[1..];
        let end = find_str_end(inner)?;
        Some(&inner[..end])
    } else {
        None
    }
}

fn get_num_field(json: &str, key: &str) -> Option<i64> {
    let search = format!("\"{}\":", key);
    let start = json.find(&search)? + search.len();
    let rest = json[start..].trim_start();
    let end = rest.find(|c: char| !c.is_ascii_digit() && c != '-').unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Находит конец строкового литерала (после открывающей кавычки)
fn find_str_end(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => return Some(i),
            b'\\' => i += 2,
            _ => i += 1,
        }
    }
    None
}

fn unescape_json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('/') => out.push('/'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    if let Ok(n) = u32::from_str_radix(&hex, 16) {
                        if let Some(ch) = char::from_u32(n) {
                            out.push(ch);
                        }
                    }
                }
                Some(c) => out.push(c),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Извлекает все JSON-объекты из массива верхнего уровня
/// (не рекурсивный full-парсер, но достаточно для LSP ответов)
fn extract_array_objects(json: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let bytes = json.as_bytes();
    let mut i = 0;
    // Ищем начало массива
    while i < bytes.len() && bytes[i] != b'[' { i += 1; }
    if i >= bytes.len() { return result; }
    i += 1;

    loop {
        while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\n' || bytes[i] == b'\r' || bytes[i] == b'\t' || bytes[i] == b',') {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] == b']' { break; }
        if bytes[i] == b'{' {
            let start = i;
            let mut depth = 0i32;
            let mut in_str = false;
            let mut escape = false;
            while i < bytes.len() {
                let b = bytes[i];
                if escape { escape = false; i += 1; continue; }
                if b == b'\\' && in_str { escape = true; i += 1; continue; }
                if b == b'"' { in_str = !in_str; i += 1; continue; }
                if !in_str {
                    if b == b'{' { depth += 1; }
                    else if b == b'}' {
                        depth -= 1;
                        if depth == 0 { i += 1; break; }
                    }
                }
                i += 1;
            }
            result.push(&json[start..i]);
        } else {
            i += 1;
        }
    }
    result
}

/// Извлекает значение ключа как JSON-подстроку (объект или массив)
fn get_object_field<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let search = format!("\"{}\":", key);
    let pos = json.find(&search)? + search.len();
    let rest = json[pos..].trim_start();
    let bytes = rest.as_bytes();
    if bytes.is_empty() { return None; }
    let opener = bytes[0];
    if opener != b'{' && opener != b'[' { return None; }
    let closer = if opener == b'{' { b'}' } else { b']' };
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escape = false;
    for (i, &b) in bytes.iter().enumerate() {
        if escape { escape = false; continue; }
        if b == b'\\' && in_str { escape = true; continue; }
        if b == b'"' { in_str = !in_str; continue; }
        if !in_str {
            if b == opener as u8 { depth += 1; }
            else if b == closer { depth -= 1; if depth == 0 { return Some(&rest[..=i]); } }
        }
    }
    None
}

// ── Парсинг диагностик ────────────────────────────────────────────────────────

fn parse_diagnostic_obj(obj: &str) -> Option<Diagnostic> {
    let range = get_object_field(obj, "range")?;
    let start = get_object_field(range, "start")?;
    let end = get_object_field(range, "end")?;

    let sl = get_num_field(start, "line")? as u32;
    let sc = get_num_field(start, "character")? as u32;
    let el = get_num_field(end, "line")? as u32;
    let ec = get_num_field(end, "character")? as u32;

    let severity_n = get_num_field(obj, "severity").unwrap_or(1);
    let severity = match severity_n {
        1 => DiagSeverity::Error,
        2 => DiagSeverity::Warning,
        3 => DiagSeverity::Info,
        _ => DiagSeverity::Hint,
    };

    let message = if let Some(s) = get_str_field(obj, "message") {
        unescape_json_str(s)
    } else {
        return None;
    };

    // code может быть строкой или числом
    let code: Option<String> = get_str_field(obj, "code")
        .map(|s| s.to_string())
        .or_else(|| get_num_field(obj, "code").map(|n| n.to_string()));

    let source = get_str_field(obj, "source").map(|s| s.to_string());

    Some(Diagnostic { start_line: sl, start_col: sc, end_line: el, end_col: ec, severity, code, message, source })
}

// ── Парсинг TextEdit / WorkspaceEdit ─────────────────────────────────────────

fn parse_text_edit_obj(obj: &str) -> Option<TextChange> {
    let range = get_object_field(obj, "range")?;
    let start = get_object_field(range, "start")?;
    let end_r = get_object_field(range, "end")?;

    let sl = get_num_field(start, "line")? as u32;
    let sc = get_num_field(start, "character")? as u32;
    let el = get_num_field(end_r, "line")? as u32;
    let ec = get_num_field(end_r, "character")? as u32;

    let new_text = get_str_field(obj, "newText")
        .map(unescape_json_str)
        .unwrap_or_default();

    Some(TextChange { start_line: sl, start_col: sc, end_line: el, end_col: ec, new_text })
}

fn parse_workspace_edit_from_json(json: &str) -> WorkspaceEdit {
    let mut edit = WorkspaceEdit::default();

    // Формат 1: "changes": { "file:///path": [ TextEdit ] }
    if let Some(changes_obj) = get_object_field(json, "changes") {
        // Парсим ключи (URI) и массивы правок
        parse_changes_object(changes_obj, &mut edit);
    }

    // Формат 2: "documentChanges": [ { textDocument: {uri}, edits: [TextEdit] } ]
    if let Some(doc_changes) = get_object_field(json, "documentChanges") {
        for item in extract_array_objects(doc_changes) {
            if let Some(td) = get_object_field(item, "textDocument") {
                if let Some(uri) = get_str_field(td, "uri") {
                    let path = uri_to_path(uri);
                    if let Some(edits_arr) = get_object_field(item, "edits") {
                        let changes: Vec<TextChange> = extract_array_objects(edits_arr)
                            .into_iter()
                            .filter_map(parse_text_edit_obj)
                            .collect();
                        if !changes.is_empty() {
                            edit.changes.entry(path).or_default().extend(changes);
                        }
                    }
                }
            }
        }
    }

    edit
}

fn parse_changes_object(json: &str, edit: &mut WorkspaceEdit) {
    // Итерируем по "uri": [edits] парам внутри объекта
    let bytes = json.as_bytes();
    let mut i = 0;
    // Пропускаем открывающую {
    while i < bytes.len() && bytes[i] != b'{' { i += 1; }
    i += 1;

    loop {
        // Ищем ключ (URI)
        while i < bytes.len() && bytes[i] != b'"' && bytes[i] != b'}' { i += 1; }
        if i >= bytes.len() || bytes[i] == b'}' { break; }
        i += 1; // skip "
        let key_start = i;
        while i < bytes.len() && bytes[i] != b'"' { i += 1; }
        let uri = &json[key_start..i];
        i += 1; // skip "
        // Ищем :
        while i < bytes.len() && bytes[i] != b':' { i += 1; }
        i += 1;
        // Ищем [
        while i < bytes.len() && bytes[i] != b'[' && bytes[i] != b'}' { i += 1; }
        if i >= bytes.len() || bytes[i] == b'}' { break; }

        // Находим конец массива
        let arr_start = i;
        let mut depth = 0i32;
        let mut in_s = false;
        let mut esc = false;
        let mut j = i;
        while j < bytes.len() {
            let b = bytes[j];
            if esc { esc = false; j += 1; continue; }
            if b == b'\\' && in_s { esc = true; j += 1; continue; }
            if b == b'"' { in_s = !in_s; j += 1; continue; }
            if !in_s {
                if b == b'[' { depth += 1; }
                else if b == b']' { depth -= 1; if depth == 0 { j += 1; break; } }
            }
            j += 1;
        }
        let arr_json = &json[arr_start..j];
        i = j;

        let path = uri_to_path(uri);
        let changes: Vec<TextChange> = extract_array_objects(arr_json)
            .into_iter()
            .filter_map(parse_text_edit_obj)
            .collect();
        if !changes.is_empty() {
            edit.changes.entry(path).or_default().extend(changes);
        }
    }
}

fn parse_code_action_obj(obj: &str) -> Option<CodeAction> {
    let title = get_str_field(obj, "title")
        .map(unescape_json_str)
        .unwrap_or_default();

    let kind = get_str_field(obj, "kind").map(|s| s.to_string());

    let edit = get_object_field(obj, "edit")
        .map(parse_workspace_edit_from_json);

    Some(CodeAction { title, kind, edit })
}

// ── Основной парсер входящих фреймов ─────────────────────────────────────────

fn dispatch_frame(body: &[u8], event_tx: &Sender<LspEvent>, server_name: &'static str, out_tx: &Sender<Vec<u8>>) {
    let json = match std::str::from_utf8(body) {
        Ok(s) => s,
        Err(_) => return,
    };

    let method = get_str_field(json, "method");
    let id = get_num_field(json, "id");

    match method {
        // ── Уведомление от сервера: диагностика ──────────────────────────
        Some("textDocument/publishDiagnostics") => {
            if let Some(params) = get_object_field(json, "params") {
                if let Some(uri) = get_str_field(params, "uri") {
                    let path = uri_to_path(uri);
                    let version = get_num_field(params, "version").map(|v| v as i32);

                    let items = if let Some(diag_arr) = get_object_field(params, "diagnostics") {
                        extract_array_objects(diag_arr)
                            .into_iter()
                            .filter_map(parse_diagnostic_obj)
                            .collect()
                    } else {
                        Vec::new()
                    };

                    let _ = event_tx.send(LspEvent::Diagnostics { path, version, items });
                }
            }
        }

        // ── Сервер просит применить правку ───────────────────────────────
        Some("workspace/applyEdit") => {
            // Отвечаем серверу что применили (без ошибок)
            // Сами правки шлём в главный поток как CodeActions
            if let Some(params) = get_object_field(json, "params") {
                if let Some(edit_obj) = get_object_field(params, "edit") {
                    let edit = parse_workspace_edit_from_json(edit_obj);
                    let action = CodeAction {
                        title: "workspace/applyEdit".to_string(),
                        kind: None,
                        edit: Some(edit),
                    };
                    let _ = event_tx.send(LspEvent::CodeActions {
                        request_id: -1,
                        actions: vec![action],
                    });
                }
            }
        }

        // ── Ответ на запрос initialize ───────────────────────────────────
        Some("initialize") | None if id.is_some() && get_object_field(json, "result").is_some() => {
            // Проверяем: если есть "result" и нет "error" — это успешный ответ
            if get_object_field(json, "result").is_some()
                && get_object_field(json, "error").is_none()
            {
                // Если это ответ на initialize — ничего не делаем, supervisor сам шлёт initialized
                // Если это ответ на codeAction — парсим как array
                if let Some(req_id) = id {
                    if let Some(result) = get_object_field(json, "result") {
                        // Пробуем распарсить как массив code actions
                        if result.trim_start().starts_with('[') {
                            let actions: Vec<CodeAction> = extract_array_objects(result)
                                .into_iter()
                                .filter_map(parse_code_action_obj)
                                .collect();
                            let _ = event_tx.send(LspEvent::CodeActions {
                                request_id: req_id as i32,
                                actions,
                            });
                        }
                    }
                }
            }
        }

        // ── Сервер шлёт запросы (например window/showMessage) ───────────
                Some("window/logMessage") => {
            if let Some(params) = get_object_field(json, "params") {
                if let Some(msg) = get_str_field(params, "message") {
                    let unescaped = unescape_json_str(msg);
                    let _ = event_tx.send(LspEvent::Log { name: server_name, message: unescaped });
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
                if let Some(params) = get_object_field(json, "params") {
                    if let Some(items) = get_object_field(params, "items") {
                        count = extract_array_objects(items).len().max(1);
                    }
                }
                let empty_objs = vec![r#"{}"#; count].join(",");
                let reply = format!(r#"{{"jsonrpc":"2.0","id":{},"result":[{}]}}"#, req_id, empty_objs);
                let _ = out_tx.send(reply.into_bytes());
            }
        }

        Some(m) => {
            // Игнорируем незнакомые уведомления. Если это запрос, отвечаем MethodNotFound, чтобы сервер не завис.
            if let Some(req_id) = id {
                if m != "window/logMessage" && m != "textDocument/publishDiagnostics" && m != "workspace/applyEdit" {
                    let reply = format!(r#"{{"jsonrpc":"2.0","id":{},"error":{{"code":-32601,"message":"Method not found"}}}}"#, req_id);
                    let _ = out_tx.send(reply.into_bytes());
                }
            }
        }

        None => {
            // Ответ на запрос (id есть, method нет)
            if let Some(req_id) = id {
                if let Some(result) = get_object_field(json, "result") {
                    if result.trim_start().starts_with('[') {
                        let actions: Vec<CodeAction> = extract_array_objects(result)
                            .into_iter()
                            .filter_map(parse_code_action_obj)
                            .collect();
                        let _ = event_tx.send(LspEvent::CodeActions {
                            request_id: req_id as i32,
                            actions,
                        });
                    }
                }
            }
        }
    }
}

// ── Запуск процесса ───────────────────────────────────────────────────────────

struct SpawnedProcess {
    child: Child,
    out_tx: Sender<Vec<u8>>,
}

fn spawn_server(
    def: &'static LspServerDef,
    event_tx: Sender<LspEvent>,
) -> Option<SpawnedProcess> {
    let mut cmd = Command::new(def.program);
    for arg in def.args {
        cmd.arg(arg);
    }
            let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .ok()?;

        let stdin = child.stdin.take()?;
        let stdout = child.stdout.take()?;
        let stderr = child.stderr.take()?;

                let (out_tx, out_rx) = mpsc::channel::<Vec<u8>>();
        let reader_out_tx = out_tx.clone();

        let err_tx = event_tx.clone();
        let srv_name = def.program;
        thread::Builder::new()
            .name(format!("lsp-stderr-{}", srv_name))
            .spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines() {
                    if let Ok(msg) = line {
                        let _ = err_tx.send(LspEvent::Log { name: srv_name, message: msg });
                    }
                }
            })
            .ok()?;

    // Тред-писатель: получает байты, оборачивает в Content-Length фрейм
    thread::Builder::new()
        .name("lsp-writer".into())
        .spawn(move || {
            let mut writer = BufWriter::with_capacity(128 * 1024, stdin);
            for body in out_rx {
                if !write_frame(&mut writer, &body) {
                    break;
                }
            }
        })
        .ok()?;

    // Тред-читатель: парсит stdout и шлёт события
    thread::Builder::new()
        .name("lsp-reader".into())
        .spawn(move || {
            let mut reader = BufReader::with_capacity(128 * 1024, stdout);
            let mut header_buf = String::with_capacity(64);
            loop {
                header_buf.clear();
                match reader.read_line(&mut header_buf) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                let content_len = if let Some(rest) = header_buf
                    .trim()
                    .strip_prefix("Content-Length:")
                {
                    match rest.trim().parse::<usize>() {
                        Ok(n) => n,
                        Err(_) => continue,
                    }
                } else {
                    continue;
                };

                // Пропускаем \r\n разделитель
                header_buf.clear();
                if reader.read_line(&mut header_buf).unwrap_or(0) == 0 {
                    break;
                }

                if content_len == 0 { continue; }

                let mut body = vec![0u8; content_len];
                let mut read = 0;
                while read < content_len {
                    match std::io::Read::read(&mut reader, &mut body[read..]) {
                        Ok(0) => { break; }
                        Ok(n) => read += n,
                        Err(_) => { break; }
                    }
                }
                                                if read < content_len { break; }

                dispatch_frame(&body, &event_tx, def.program, &reader_out_tx);
            }
        }).ok()?;

    Some(SpawnedProcess { child, out_tx })
}

// ── Supervisor тред ───────────────────────────────────────────────────────────

/// Состояние supervisor: что открыто сейчас (для реопена после рестарта)
#[derive(Clone)]
struct OpenFile {
    uri: String,
    lang: &'static str,
    version: i32,
    text: String,
}

fn run_supervisor(
    def: &'static LspServerDef,
    workspace: Option<PathBuf>,
    cmd_rx: Receiver<Cmd>,
    event_tx: Sender<LspEvent>,
) {
    let mut open_file: Option<OpenFile> = None;
    let mut init_id;
    let mut restart_delay = Duration::from_millis(500);
    let mut user_requested_restart = false;

    'outer: loop {
        let _ = event_tx.send(LspEvent::StatusChanged { name: def.program, status: LspServerStatus::Starting });
        // ── Запускаем процесс ─────────────────────────────────────────
        let mut proc = match spawn_server(def, event_tx.clone()) {
            Some(p) => p,
            None => {
                let _ = event_tx.send(LspEvent::StatusChanged { name: def.program, status: LspServerStatus::Crashed });
                thread::sleep(restart_delay);
                restart_delay = (restart_delay * 2).min(Duration::from_secs(10));
                continue 'outer;
            }
        };
        restart_delay = Duration::from_millis(500); // сброс на удачный запуск

        // ── Handshake: initialize ─────────────────────────────────────────
        init_id = next_id();
        let init_msg = make_initialize(init_id, workspace.as_deref());
        if proc.out_tx.send(init_msg).is_err() {
            continue 'outer;
        }

        // Ждём ответ на initialize (простой polling цикл)
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut initialized = false;
        while std::time::Instant::now() < deadline {
            // Проверяем crash
            match proc.child.try_wait() {
                Ok(Some(_)) => continue 'outer,
                Ok(None) => {}
                Err(_) => continue 'outer,
            }
            // Ждём немного - initialize ответ придёт через reader тред в event_tx
            // Но нам нужно знать когда сервер готов — используем специальный подход:
            // просто ждём 200мс (ruff server стартует быстро), потом шлём initialized
            thread::sleep(Duration::from_millis(200));
            initialized = true;
            break;
        }
        if !initialized { continue 'outer; }

        // Шлём initialized notification
        if proc.out_tx.send(make_initialized()).is_err() {
            continue 'outer;
        }
                let _ = event_tx.send(LspEvent::ServerReady);
        let _ = event_tx.send(LspEvent::StatusChanged { name: def.program, status: LspServerStatus::Running });

        // Если был открыт файл — reopenуем после рестарта
        if let Some(ref of) = open_file {
            let msg = make_did_open(&of.uri, of.lang, of.version, &of.text);
            if proc.out_tx.send(msg).is_err() {
                continue 'outer;
            }
        }

        // ── Основной цикл supervisor ──────────────────────────────────────
        'inner: loop {
                        // Проверяем краш процесса
            match proc.child.try_wait() {
                Ok(Some(_)) => {
                    if !user_requested_restart {
                        let _ = event_tx.send(LspEvent::StatusChanged { name: def.program, status: LspServerStatus::Crashed });
                    }
                    user_requested_restart = false;
                    thread::sleep(Duration::from_millis(1000));
                    break 'inner; // рестарт
                }
                Ok(None) => {}
                Err(_) => break 'inner,
            }

            // Обрабатываем команды от главного треда
            loop {
                match cmd_rx.try_recv() {
                    Ok(Cmd::Restart) => {
                        user_requested_restart = true;
                        // Убиваем текущий процесс — supervisor перезапустит
                        let _ = proc.child.kill();
                        break 'inner;
                    }
                    Ok(Cmd::Open { uri, lang, version, text }) => {
                        let msg = make_did_open(&uri, lang, version, &text);
                        open_file = Some(OpenFile { uri, lang, version, text });
                        if proc.out_tx.send(msg).is_err() { break 'inner; }
                    }
                    Ok(Cmd::Change { uri, version, text }) => {
                        if let Some(ref mut of) = open_file {
                            of.version = version;
                            of.text = text.clone();
                        }
                        let msg = make_did_change_full(&uri, version, &text);
                        if proc.out_tx.send(msg).is_err() { break 'inner; }
                    }
                    Ok(Cmd::Close { uri: _ }) => {
                        if let Some(ref of) = open_file {
                            let msg = make_did_close(&of.uri);
                            let _ = proc.out_tx.send(msg);
                        }
                        open_file = None;
                    }
                    Ok(Cmd::CodeAction { id, uri, start_line, start_col, end_line, end_col, diagnostics_json }) => {
                        let msg = make_code_action(id, &uri, start_line, start_col, end_line, end_col, &diagnostics_json);
                        if proc.out_tx.send(msg).is_err() { break 'inner; }
                    }
                    Ok(Cmd::Shutdown) => {
                        let sid = next_id();
                        let _ = proc.out_tx.send(make_shutdown(sid));
                        thread::sleep(Duration::from_millis(200));
                        let _ = proc.out_tx.send(make_exit());
                        let _ = proc.child.wait();
                        return; // выходим из supervisor насовсем
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return, // App завершился
                }
            }

            thread::sleep(Duration::from_millis(5));
        }
    }
}

// ── LspProcess: публичный handle одного сервера ───────────────────────────────

pub struct LspProcess {
    cmd_tx: Sender<Cmd>,
    pub event_rx: Receiver<LspEvent>,
    current_uri: Option<String>,
    def: &'static LspServerDef,
    pub open_file_data: Option<(String, String)>, // (lang, text) for re-open after restart
}

impl LspProcess {
    fn start(def: &'static LspServerDef, workspace: Option<PathBuf>) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let ws = workspace.clone();

        thread::Builder::new()
            .name(format!("lsp-supervisor-{}", def.program))
            .spawn(move || run_supervisor(def, ws, cmd_rx, event_tx))
            .expect("failed to start LSP supervisor");

                LspProcess { cmd_tx, event_rx, current_uri: None, def, open_file_data: None }
    }

        /// textDocument/didOpen
    pub fn notify_open(&mut self, path: &PathBuf, text: &str, version: i32) {
        let uri = path_to_uri(&path.to_string_lossy());
        self.current_uri = Some(uri.clone());
        self.open_file_data = Some((self.def.language_id.to_string(), text.to_string()));
        let _ = self.cmd_tx.send(Cmd::Open {
            uri,
            lang: self.def.language_id,
            version,
            text: text.to_string(),
        });
    }

    /// Перезапустить сервер
    pub fn restart(&mut self) {
        let _ = self.cmd_tx.send(Cmd::Restart);
    }

    /// textDocument/didChange — полный текст (Full Sync).
    /// Вызывать когда editor.sync_edits непуст.
    pub fn notify_change(&mut self, path: &PathBuf, text: &str, version: i32) {
        let uri = path_to_uri(&path.to_string_lossy());
        self.current_uri = Some(uri.clone());
        let _ = self.cmd_tx.send(Cmd::Change { uri, version, text: text.to_string() });
    }

    /// textDocument/didClose
    pub fn notify_close(&mut self, path: &PathBuf) {
        let uri = path_to_uri(&path.to_string_lossy());
        let _ = self.cmd_tx.send(Cmd::Close { uri });
        self.current_uri = None;
    }

    /// Запрашивает code actions (быстрые исправления от ruff) для позиции.
    /// Возвращает id запроса — по нему придёт LspEvent::CodeActions.
    pub fn request_code_actions(
        &mut self,
        path: &PathBuf,
        start_line: u32, start_col: u32,
        end_line: u32, end_col: u32,
        diagnostics: &[Diagnostic],
    ) -> i32 {
        let id = next_id();
        let uri = path_to_uri(&path.to_string_lossy());

        // Кодируем диагностики в JSON для контекста запроса
        let diag_json = encode_diagnostics_json(diagnostics);

        let _ = self.cmd_tx.send(Cmd::CodeAction {
            id, uri, start_line, start_col, end_line, end_col,
            diagnostics_json: diag_json,
        });
        id
    }

    /// Опрашивает входящие события (non-blocking). Вызывать раз в кадр.
    pub fn poll(&self) -> Vec<LspEvent> {
        let mut events = Vec::new();
        loop {
            match self.event_rx.try_recv() {
                Ok(e) => events.push(e),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
        events
    }

    pub fn shutdown(self) {
        let _ = self.cmd_tx.send(Cmd::Shutdown);
    }
}

// ── Encode diagnostics for codeAction context ────────────────────────────────

fn encode_diagnostics_json(diags: &[Diagnostic]) -> String {
    let mut out = String::from('[');
    for (i, d) in diags.iter().enumerate() {
        if i > 0 { out.push(','); }
        let sev = match d.severity {
            DiagSeverity::Error => 1,
            DiagSeverity::Warning => 2,
            DiagSeverity::Info => 3,
            DiagSeverity::Hint => 4,
        };
        let code_json = match &d.code {
            Some(c) => format!(r#","code":"{}""#, json_escape(c)),
            None => String::new(),
        };
        let source_json = match &d.source {
            Some(s) => format!(r#","source":"{}""#, json_escape(s)),
            None => String::new(),
        };
        out.push_str(&format!(
            r#"{{"range":{{"start":{{"line":{},"character":{}}},"end":{{"line":{},"character":{}}}}},"severity":{},"message":"{}"{}{}}}"#,
            d.start_line, d.start_col, d.end_line, d.end_col,
            sev,
            json_escape(&d.message),
            code_json, source_json
        ));
    }
    out.push(']');
    out
}

// ── LspManager: главный фасад для App ─────────────────────────────────────────

pub struct LspManager {
    python: Option<LspProcess>,
    workspace: Option<PathBuf>,
    /// Актуальные диагностики текущего файла
    pub diagnostics: Vec<Diagnostic>,
    current_path: Option<PathBuf>,
    /// Статус ruff сервера
    pub python_status: LspServerStatus,
    /// Отключён ли ruff вручную
    pub python_disabled: bool,
    pub server_logs: HashMap<&'static str, Vec<String>>,
}

impl LspManager {
        pub fn new(workspace: Option<PathBuf>) -> Self {
        LspManager {
            python: None,
            workspace,
            diagnostics: Vec::new(),
            current_path: None,
            python_status: LspServerStatus::Disabled,
            python_disabled: false,
            server_logs: HashMap::new(),
        }
    }

        /// Запускает нужный LSP-сервер если ещё не запущен (lazy)
    fn ensure_python(&mut self) {
        if self.python.is_none() && !self.python_disabled {
            self.python_status = LspServerStatus::Starting;
            self.python = Some(LspProcess::start(&RUFF_SERVER, self.workspace.clone()));
        }
    }

    /// Перезапустить ruff сервер
    pub fn restart_python(&mut self) {
        if let Some(proc) = &mut self.python {
            proc.restart();
            self.python_status = LspServerStatus::Starting;
        } else if !self.python_disabled {
            self.python_status = LspServerStatus::Starting;
            self.python = Some(LspProcess::start(&RUFF_SERVER, self.workspace.clone()));
        }
    }

    /// Отключить ruff (остановить и не перезапускать)
        pub fn disable_python(&mut self) {
        self.python_disabled = true;
        self.python_status = LspServerStatus::Disabled;
        if let Some(p) = self.python.take() {
            p.shutdown();
        }
        self.diagnostics.clear();
        self.server_logs.clear();
    }

    /// Включить ruff обратно
    pub fn enable_python(&mut self) {
        self.python_disabled = false;
        self.python_status = LspServerStatus::Starting;
        self.python = Some(LspProcess::start(&RUFF_SERVER, self.workspace.clone()));
        // Re-open current file if any
        if let Some(path) = &self.current_path.clone() {
            if let Some(proc) = &mut self.python {
                if let Some((_, text)) = &proc.open_file_data.clone() {
                    proc.notify_open(path, text, 1);
                }
            }
        }
    }

    /// Информация о серверах для UI
        pub fn servers_info(&self) -> Vec<LspServerInfo> {
        let logs = self.server_logs.get(RUFF_SERVER.program).cloned().unwrap_or_default();
        vec![LspServerInfo {
            name: RUFF_SERVER.program,
            status: self.python_status.clone(),
            logs,
        }]
    }

    /// Возвращает процесс для нужного расширения, запустив при необходимости
    fn process_for_ext(&mut self, ext: &str) -> Option<&mut LspProcess> {
        match ext {
            "py" => {
                self.ensure_python();
                self.python.as_mut()
            }
            _ => None,
        }
    }

    /// Уведомляет LSP об открытии файла
    pub fn notify_open(&mut self, path: &PathBuf, ext: &str, text: &str, version: i32) {
        self.current_path = Some(path.clone());
        self.diagnostics.clear();
        if let Some(proc) = self.process_for_ext(ext) {
            proc.notify_open(path, text, version);
        }
    }

    /// Уведомляет LSP об изменении файла (когда sync_edits непуст)
    pub fn notify_change(&mut self, path: &PathBuf, ext: &str, text: &str, version: i32) {
        if let Some(proc) = self.process_for_ext(ext) {
            proc.notify_change(path, text, version);
        }
    }

    /// Уведомляет LSP о закрытии файла
    pub fn notify_close(&mut self, ext: &str) {
        if let Some(path) = self.current_path.take() {
            if let Some(proc) = self.process_for_ext(ext) {
                proc.notify_close(&path);
            }
        }
        self.diagnostics.clear();
    }

    /// Запрашивает code actions для позиции/диагностики
    pub fn request_code_actions(
        &mut self,
        ext: &str,
        start_line: u32, start_col: u32,
        end_line: u32, end_col: u32,
        relevant_diags: &[Diagnostic],
    ) -> Option<i32> {
        let path = self.current_path.clone()?;
        let proc = self.process_for_ext(ext)?;
        Some(proc.request_code_actions(
            &path, start_line, start_col, end_line, end_col, relevant_diags,
        ))
    }

        /// Опрашивает события от всех серверов. Вызывать раз в кадр.
    /// Обновляет self.diagnostics при получении новых диагностик.
    pub fn poll(&mut self) -> Vec<LspEvent> {
        let mut all = Vec::new();

        if let Some(proc) = &self.python {
            all.extend(proc.poll());
        }

        // Обновляем кешированные диагностики и статусы
        for ev in &all {
                        match ev {
                LspEvent::Diagnostics { path, items, .. } => {
                    if self.current_path.as_deref() == Some(path.as_path()) {
                        self.diagnostics = items.clone();
                    }
                }
                LspEvent::StatusChanged { status, .. } => {
                    self.python_status = status.clone();
                }
                LspEvent::Log { name, message } => {
                    let logs = self.server_logs.entry(name).or_insert_with(Vec::new);
                    logs.push(message.clone());
                    if logs.len() > 100 {
                        logs.remove(0);
                    }
                }
                _ => {}
            }
        }

        all
    }

        /// Диагностики для текущего файла, отфильтрованные по строке
    pub fn diagnostics_for_line(&self, line: u32) -> Vec<&Diagnostic> {
        self.diagnostics.iter().filter(move |d| d.start_line == line).collect()
    }

    /// Запрос на глобальный fix-all (source.fixAll) для текущего файла
    pub fn request_fix_all(&mut self, ext: &str) -> Option<i32> {
        let path = self.current_path.clone()?;
        let proc = self.process_for_ext(ext)?;
        let id = next_id();
        let uri = path_to_uri(&path.to_string_lossy());
        let _ = proc.cmd_tx.send(Cmd::CodeAction {
            id, uri,
            start_line: 0, start_col: 0, end_line: u32::MAX, end_col: 0,
            diagnostics_json: String::from("[]"),
        });
        Some(id)
    }

            #[allow(dead_code)]
    pub fn shutdown(mut self) {
        self.python_disabled = true;
        if let Some(p) = self.python.take() { p.shutdown(); }
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
pub fn apply_workspace_edit_to_text(text: &str, edit: &WorkspaceEdit, path: &PathBuf) -> String {
    let Some(changes) = edit.changes.get(path) else {
        return text.to_string();
    };

    // Сортируем правки с конца файла к началу
    let mut sorted = changes.clone();
    sorted.sort_unstable_by(|a, b| {
        b.start_line.cmp(&a.start_line)
            .then(b.start_col.cmp(&a.start_col))
    });

    let mut result = text.to_string();
    for change in &sorted {
        let start = lsp_pos_to_offset(&result, change.start_line, change.start_col);
        let end = lsp_pos_to_offset(&result, change.end_line, change.end_col);
        if start <= end && end <= result.len() {
            result.replace_range(start..end, &change.new_text);
        }
    }
    result
}


