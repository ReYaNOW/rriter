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
use tree_sitter::StreamingIterator;

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
pub struct LogEntry {
    pub text: String,
    pub spans: Vec<crate::highlighter::ColorSpan>,
    pub folds: Vec<(usize, usize)>,
}

#[derive(Debug, Clone)]
pub struct LspServerInfo {
    pub name: &'static str,
    pub status: LspServerStatus,
    pub logs: Vec<LogEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

#[derive(Debug, Clone)]
pub struct QuickFix {
    pub title: String,
    pub edits: Vec<TextChange>,
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
    /// Ссылка на документацию (из codeDescription.href)
    pub code_href: Option<String>,
            pub message: String,
    pub source: Option<String>,
    pub quickfixes: Vec<QuickFix>,
    pub tags: Vec<u32>,
    pub spans: Vec<crate::highlighter::ColorSpan>,
}

thread_local! {
    static TS_DIAG_PARSER: std::cell::RefCell<tree_sitter::Parser> = {
        let mut parser = tree_sitter::Parser::new();
        if let Some((lang, _)) = crate::queries::get_ts_config("py") {
            let _ = parser.set_language(&lang);
        }
        std::cell::RefCell::new(parser)
    };
    static TS_DIAG_QUERY: std::cell::RefCell<Option<tree_sitter::Query>> = std::cell::RefCell::new({
        if let Some((lang, queries)) = crate::queries::get_ts_config("py") {
            let full = queries.join("\n");
            tree_sitter::Query::new(&lang, &full).ok()
        } else {
            None
        }
    });
    static TS_DIAG_CURSOR: std::cell::RefCell<tree_sitter::QueryCursor> = std::cell::RefCell::new(tree_sitter::QueryCursor::new());
}

pub fn highlight_hover_text(msg: &str) -> (String, Vec<crate::highlighter::ColorSpan>) {
    let clean_msg = msg.replace('\r', "").replace("```python", "").replace("```", "").trim().to_string();
    if clean_msg.contains(":param ") {
        let mut spans = highlight_python_hover_doc(&clean_msg);
        spans.sort_unstable_by_key(|s| s.start);
        return (clean_msg, spans);
    }
    let mut spans = Vec::new();

    TS_DIAG_PARSER.with(|p_cell| {
    TS_DIAG_QUERY.with(|q_cell| {
    TS_DIAG_CURSOR.with(|c_cell| {
        let mut parser = p_cell.borrow_mut();
        let query_opt = q_cell.borrow();
        let mut cursor = c_cell.borrow_mut();

        if let Some(query) = query_opt.as_ref() {
            if let Some(tree) = parser.parse(&clean_msg, None) {
                let mut matches = cursor.matches(query, tree.root_node(), clean_msg.as_bytes());
                while let Some(m) = matches.next() {
                    for cap in m.captures {
                        let name = query.capture_names()[cap.index as usize];
                        let color = match name {
                            "property" | "variable" =>[0.972, 0.972, 0.949, 1.0],
                            "string" =>[0.945, 0.980, 0.549, 1.0],
                            "type" | "class_name" =>[0.545, 0.913, 0.992, 1.0],
                            "keyword.control" | "keyword" | "operator" =>[1.0, 0.474, 0.776, 1.0],
                            "function" | "py_function" | "py_builtin_or_func" =>[0.313, 0.980, 0.482, 1.0],
                            "number" =>[0.741, 0.576, 0.976, 1.0],
                            "comment" =>[0.384, 0.447, 0.643, 1.0],
                            _ => continue,
                        };
                        spans.push(crate::highlighter::ColorSpan {
                            start: cap.node.start_byte(),
                            end: cap.node.end_byte(),
                            color,
                        });
                    }
                }
            }
        }
    })})});

    spans.sort_unstable_by_key(|s| s.start);
    (clean_msg, spans)
}

fn highlight_python_hover_doc(msg: &str) -> Vec<crate::highlighter::ColorSpan> {
    let gray = [0.56, 0.60, 0.66, 1.0];
    let kw = [1.0, 0.474, 0.776, 1.0];
    let ty = [0.545, 0.913, 0.992, 1.0];
    let arg = [1.0, 0.62, 0.24, 1.0];
    let func = [0.313, 0.980, 0.482, 1.0];

    let mut spans = vec![crate::highlighter::ColorSpan {
        start: 0,
        end: msg.len(),
        color: gray,
    }];

    let mut in_code_block = false;
    let mut in_signature = false;
    let mut byte = 0usize;
    for raw_line in msg.split('\n') {
        let line = raw_line;
        let line_start = byte;
        let line_end = line_start + line.len();
        let trimmed = line.trim_start();

        if trimmed.starts_with(".. code-block:: python") {
            in_code_block = true;
        } else if in_code_block && !line.starts_with("    ") && !trimmed.is_empty() {
            in_code_block = false;
        }

        if trimmed.starts_with("def ") || trimmed.starts_with("async def ") {
            in_signature = true;
            // function name
            if let Some(def_pos) = line.find("def ") {
                let fn_start = line_start + def_pos + 4;
                let mut fn_end = fn_start;
                while fn_end < line_end {
                    let ch = msg.as_bytes()[fn_end];
                    if ch == b'(' || ch == b' ' || ch == b':' {
                        break;
                    }
                    fn_end += 1;
                }
                if fn_end > fn_start {
                    spans.push(crate::highlighter::ColorSpan {
                        start: fn_start,
                        end: fn_end,
                        color: func,
                    });
                }
            }

            // args in signature -> orange
            if let (Some(lp), Some(rp)) = (line.find('('), line.rfind(')')) {
                let args = &line[lp + 1..rp];
                let mut off = line_start + lp + 1;
                let mut tok_start: Option<usize> = None;
                for c in args.chars() {
                    let is_ident = c == '_' || c.is_ascii_alphanumeric();
                    if is_ident {
                        if tok_start.is_none() {
                            tok_start = Some(off);
                        }
                    } else if let Some(st) = tok_start.take() {
                        let token = &msg[st..off];
                        if token != "None" && token != "Any" {
                            spans.push(crate::highlighter::ColorSpan {
                                start: st,
                                end: off,
                                color: arg,
                            });
                        }
                    }
                    off += c.len_utf8();
                }
                if let Some(st) = tok_start.take() {
                    let token = &msg[st..off];
                    if token != "None" && token != "Any" {
                        spans.push(crate::highlighter::ColorSpan {
                            start: st,
                            end: off,
                            color: arg,
                        });
                    }
                }
            }
        }

        if in_signature {
            let mut token_start: Option<usize> = None;
            let mut offset = 0usize;
            for ch in line.chars() {
                let is_ident = ch == '_' || ch.is_ascii_alphanumeric();
                if is_ident {
                    if token_start.is_none() {
                        token_start = Some(offset);
                    }
                } else if let Some(st_rel) = token_start.take() {
                    let en_rel = offset;
                    let tok = &line[st_rel..en_rel];
                    if tok != "def" && tok != "async" && tok != "None" && tok != "Any" {
                        let mut left = st_rel;
                        while left > 0 && line.as_bytes()[left - 1].is_ascii_whitespace() {
                            left -= 1;
                        }
                        let prev = if left > 0 {
                            line.as_bytes()[left - 1] as char
                        } else {
                            '\0'
                        };
                        let mut right = en_rel;
                        while right < line.len() && line.as_bytes()[right].is_ascii_whitespace() {
                            right += 1;
                        }
                        let next = if right < line.len() {
                            line.as_bytes()[right] as char
                        } else {
                            '\0'
                        };

                        let color = if prev == ':' {
                            ty
                        } else if next == ':' || next == '=' || next == ',' || next == ')' {
                            arg
                        } else {
                            gray
                        };
                        if color != gray {
                            spans.push(crate::highlighter::ColorSpan {
                                start: line_start + st_rel,
                                end: line_start + en_rel,
                                color,
                            });
                        }
                    }
                }
                offset += ch.len_utf8();
            }
            if let Some(st_rel) = token_start.take() {
                let en_rel = line.len();
                let tok = &line[st_rel..en_rel];
                if tok != "def" && tok != "async" && tok != "None" && tok != "Any" {
                    spans.push(crate::highlighter::ColorSpan {
                        start: line_start + st_rel,
                        end: line_start + en_rel,
                        color: arg,
                    });
                }
            }
            if line.contains(") ->") || trimmed.ends_with(')') || trimmed.ends_with(") -> Unknown") {
                in_signature = false;
            }
        }

        if trimmed.starts_with(":param ") {
            let pfx = line.find(":param ").unwrap_or(0);
            let rest = &line[pfx + 7..];
            let mut ws = rest.char_indices().filter(|(_, c)| !c.is_whitespace());
            if let Some((type_start_rel, _)) = ws.next() {
                let type_start = line_start + pfx + 7 + type_start_rel;
                let mut type_end = type_start;
                while type_end < line_end {
                    let b = msg.as_bytes()[type_end];
                    if b == b' ' || b == b':' {
                        break;
                    }
                    type_end += 1;
                }
                if type_end > type_start {
                    spans.push(crate::highlighter::ColorSpan {
                        start: type_start,
                        end: type_end,
                        color: ty,
                    });
                }
            }
        }

        if in_code_block || line.contains("``async with``") {
            let kws = ["async", "with", "await", "def", "try", "finally", "as"];
            for kwd in kws {
                let mut from = 0usize;
                while let Some(pos) = line[from..].find(kwd) {
                    let s = from + pos;
                    let e = s + kwd.len();
                    let prev_ok = s == 0 || !line.as_bytes()[s - 1].is_ascii_alphanumeric();
                    let next_ok = e >= line.len() || !line.as_bytes()[e].is_ascii_alphanumeric();
                    if prev_ok && next_ok {
                        spans.push(crate::highlighter::ColorSpan {
                            start: line_start + s,
                            end: line_start + e,
                            color: kw,
                        });
                    }
                    from = e;
                    if from >= line.len() {
                        break;
                    }
                }
            }
        }

        byte = line_end + 1;
    }

    spans
}

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
        TS_DIAG_PARSER.with(|p_cell| {
        TS_DIAG_QUERY.with(|q_cell| {
        TS_DIAG_CURSOR.with(|c_cell| {
            let mut parser = p_cell.borrow_mut();
            let query_opt = q_cell.borrow();
            let mut cursor = c_cell.borrow_mut();

            if let Some(query) = query_opt.as_ref() {
                for &(start, end) in &backtick_ranges {
                    if start >= end { continue; }
                    let code = &msg[start..end];
                    if let Some(tree) = parser.parse(code, None) {
                        let mut matches = cursor.matches(query, tree.root_node(), code.as_bytes());
                        while let Some(m) = matches.next() {
                            for cap in m.captures {
                                let name = query.capture_names()[cap.index as usize];
                                let color = match name {
                                    "property" | "variable" => [0.972, 0.972, 0.949, 1.0],
                                    "string" => [0.945, 0.980, 0.549, 1.0],
                                    "type" | "class_name" => [0.545, 0.913, 0.992, 1.0],
                                    "keyword.control" | "keyword" | "operator" => [1.0, 0.474, 0.776, 1.0],
                                    "function" | "py_function" | "py_builtin_or_func" => [0.313, 0.980, 0.482, 1.0],
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
        })})});
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
}

#[derive(Debug, Clone)]
pub struct CodeAction {
    pub title: String,
    pub kind: Option<String>,
    pub edit: Option<WorkspaceEdit>,
    pub code: Option<String>,
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

const TY_SERVER: LspServerDef = LspServerDef {
    program: "ty",
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
                let _ = std::fmt::Write::write_fmt(&mut out, format_args!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

/// Сериализует путь → file:// URI
fn path_to_uri(path: &str) -> String {
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

fn uri_to_path(uri: &str) -> PathBuf {
    let mut s = uri.strip_prefix("file://").unwrap_or(uri);
    // Для Windows (file:///C:/...) убираем первый слеш
    if s.starts_with('/') && s.chars().nth(2) == Some(':') {
        s = &s[1..];
    }
    PathBuf::from(s)
}

// ── Кодировщики JSON-RPC сообщений ────────────────────────────────────────────

fn make_initialize(id: i32, workspaces: &[PathBuf]) -> Vec<u8> {
    let (root_uri_json, workspace_json) = if let Some(first_ws) = workspaces.first() {
        let root_uri = path_to_uri(&first_ws.to_string_lossy());
        let escaped_root = json_escape(&root_uri);

        let mut folders = Vec::new();
        for (i, ws) in workspaces.iter().enumerate() {
            let uri = path_to_uri(&ws.to_string_lossy());
            folders.push(format!(
                r#"{{"uri":"{}","name":"workspace_{}"}}"#,
                json_escape(&uri), i
            ));
        }

        (
            format!(r#""{}""#, escaped_root),
            format!(r#","workspaceFolders":[{}]"#, folders.join(","))
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

fn make_hover(id: i32, uri: &str, line: u32, col: u32) -> Vec<u8> {
    format!(
        r#"{{"jsonrpc":"2.0","id":{},"method":"textDocument/hover","params":{{"textDocument":{{"uri":"{}"}},"position":{{"line":{},"character":{}}}}}}}"#,
        id, json_escape(uri), line, col
    ).into_bytes()
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

fn parse_diagnostic_value(v: &serde_json::Value) -> Option<Diagnostic> {
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

                message = message.replace("\\n", "\n").replace("\\t", "    ").replace('\r', "");

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
        })}

fn parse_text_edit_value(v: &serde_json::Value) -> Option<TextChange> {
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

fn parse_workspace_edit_value(v: &serde_json::Value) -> WorkspaceEdit {
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

fn parse_hover_value(v: &serde_json::Value) -> Option<String> {
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

fn parse_code_action_value(v: &serde_json::Value) -> Option<CodeAction> {
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

fn dispatch_frame(
    body: &[u8],
    event_tx: &Sender<LspEvent>,
    server_name: &'static str,
    out_tx: &Sender<Vec<u8>>,
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
                    let reply = format!(r#"{{"jsonrpc":"2.0","id":{},"error":{{"code":-32601,"message":"Method not found"}}}}"#,
                        req_id
                    );
                    let _ = out_tx.send(reply.into_bytes());
                }
            }
        }
                None => {
            if let Some(req_id) = id {
                if let Some(result) = msg.get("result") {
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

// ── Запуск процесса ───────────────────────────────────────────────────────────

struct SpawnedProcess {
    child: Child,
    out_tx: Sender<Vec<u8>>,
}

fn spawn_server(
    def: &'static LspServerDef,
    workspace: Option<&Path>,
    event_tx: Sender<LspEvent>,
) -> Option<SpawnedProcess> {
    let mut cmd = Command::new(def.program);
    if let Some(ws) = workspace {
        cmd.current_dir(ws);
    }
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
                    let _ = err_tx.send(LspEvent::Log {
                        name: srv_name,
                        message: msg,
                    });
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
                let content_len =
                    if let Some(rest) = header_buf.trim().strip_prefix("Content-Length:") {
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

                if content_len == 0 {
                    continue;
                }

                let mut body = vec![0u8; content_len];
                let mut read = 0;
                while read < content_len {
                    match std::io::Read::read(&mut reader, &mut body[read..]) {
                        Ok(0) => {
                            break;
                        }
                        Ok(n) => read += n,
                        Err(_) => {
                            break;
                        }
                    }
                }
                if read < content_len {
                    break;
                }

                dispatch_frame(&body, &event_tx, def.program, &reader_out_tx);
            }
        })
        .ok()?;

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

fn send_and_log(
    out_tx: &Sender<Vec<u8>>,
    event_tx: &Sender<LspEvent>,
    server_name: &'static str,
    msg: Vec<u8>,
) -> Result<(), mpsc::SendError<Vec<u8>>> {
    if let Ok(s) = std::str::from_utf8(&msg) {
        let log_msg = if s.contains("\"textDocument/didOpen\"") || s.contains("\"textDocument/didChange\"") {
            if let Some(idx) = s.find("\"text\":\"") {
                let mut temp = String::with_capacity(s.len().min(512));
                temp.push_str(&s[..idx + 8]);
                temp.push_str("<TRUNCATED>\"");
                if s.contains("didOpen") {
                    temp.push_str("}}}}");
                } else {
                    temp.push_str("}]}}");
                }
                format!("[LSP SEND] {}", temp)
            } else {
                format!("[LSP SEND] {}", s)
            }
        } else {
            format!("[LSP SEND] {}", s)
        };

        let _ = event_tx.send(LspEvent::Log {
            name: server_name,
            message: log_msg,
        });
    }
    out_tx.send(msg)
}

fn run_supervisor(
    def: &'static LspServerDef,
    workspaces: Vec<PathBuf>,
    cmd_rx: Receiver<Cmd>,
    event_tx: Sender<LspEvent>,
) {
    let mut open_file: Option<OpenFile> = None;
    let mut init_id;
    let mut restart_delay = Duration::from_millis(500);
    let mut user_requested_restart = false;

    'outer: loop {
        let _ = event_tx.send(LspEvent::StatusChanged {
            name: def.program,
            status: LspServerStatus::Starting,
        });
        // ── Запускаем процесс ─────────────────────────────────────────
        let mut proc = match spawn_server(def, workspaces.first().map(|p| p.as_path()), event_tx.clone()) {
            Some(p) => p,
            None => {
                let _ = event_tx.send(LspEvent::StatusChanged {
                    name: def.program,
                    status: LspServerStatus::Crashed,
                });
                thread::sleep(restart_delay);
                restart_delay = (restart_delay * 2).min(Duration::from_secs(10));
                continue 'outer;
            }
        };
        restart_delay = Duration::from_millis(500); // сброс на удачный запуск

        // ── Handshake: initialize ─────────────────────────────────────────
        init_id = next_id();
        let init_msg = make_initialize(init_id, &workspaces);
        if send_and_log(&proc.out_tx, &event_tx, def.program, init_msg).is_err() {
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
        if !initialized {
            continue 'outer;
        }

        // Шлём initialized notification
        if send_and_log(&proc.out_tx, &event_tx, def.program, make_initialized()).is_err() {
            continue 'outer;
        }
        let _ = event_tx.send(LspEvent::ServerReady);
        let _ = event_tx.send(LspEvent::StatusChanged {
            name: def.program,
            status: LspServerStatus::Running,
        });

        // Если был открыт файл — reopenуем после рестарта
        if let Some(ref of) = open_file {
            let msg = make_did_open(&of.uri, of.lang, of.version, &of.text);
            if send_and_log(&proc.out_tx, &event_tx, def.program, msg).is_err() {
                continue 'outer;
            }
        }

        // ── Основной цикл supervisor ──────────────────────────────────────
        'inner: loop {
            // Проверяем краш процесса
            match proc.child.try_wait() {
                Ok(Some(_)) => {
                    if !user_requested_restart {
                        let _ = event_tx.send(LspEvent::StatusChanged {
                            name: def.program,
                            status: LspServerStatus::Crashed,
                        });
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
                    Ok(Cmd::Open {
                        uri,
                        lang,
                        version,
                        text,
                    }) => {
                        let msg = make_did_open(&uri, lang, version, &text);
                        open_file = Some(OpenFile {
                            uri,
                            lang,
                            version,
                            text,
                        });
                        if send_and_log(&proc.out_tx, &event_tx, def.program, msg).is_err() {
                            break 'inner;
                        }
                    }
                    Ok(Cmd::Change { uri, version, text }) => {
                        if let Some(ref mut of) = open_file {
                            of.version = version;
                            of.text = text.clone();
                        }
                        let msg = make_did_change_full(&uri, version, &text);
                        if send_and_log(&proc.out_tx, &event_tx, def.program, msg).is_err() {
                            break 'inner;
                        }
                    }
                    Ok(Cmd::Close { uri: _ }) => {
                        if let Some(ref of) = open_file {
                            let msg = make_did_close(&of.uri);
                            let _ = send_and_log(&proc.out_tx, &event_tx, def.program, msg);
                        }
                        open_file = None;
                    }
                                        Ok(Cmd::Hover { id, uri, line, col }) => {
                        let msg = make_hover(id, &uri, line, col);
                        if send_and_log(&proc.out_tx, &event_tx, def.program, msg).is_err() {
                            break 'inner;
                        }
                    }
                    Ok(Cmd::CodeAction {
                        id,
                        uri,
                        start_line,
                        start_col,
                        end_line,
                        end_col,
                        diagnostics_json,
                        only,
                    }) => {
                        let msg = make_code_action(
                            id,
                            &uri,
                            start_line,
                            start_col,
                            end_line,
                            end_col,
                            &diagnostics_json,
                            only.as_deref(),
                        );
                        if send_and_log(&proc.out_tx, &event_tx, def.program, msg).is_err() {
                            break 'inner;
                        }
                    }
                    Ok(Cmd::Shutdown) => {
                        let sid = next_id();
                        let _ =
                            send_and_log(&proc.out_tx, &event_tx, def.program, make_shutdown(sid));
                        thread::sleep(Duration::from_millis(200));
                        let _ = send_and_log(&proc.out_tx, &event_tx, def.program, make_exit());
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
    workspace_scanned: bool,
}

impl LspProcess {
        fn start(def: &'static LspServerDef, workspaces: Vec<PathBuf>) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let ws = workspaces.clone();

        thread::Builder::new()
            .name(format!("lsp-supervisor-{}", def.program))
            .spawn(move || run_supervisor(def, ws, cmd_rx, event_tx))
            .expect("failed to start LSP supervisor");

        LspProcess {
            cmd_tx,
            event_rx,
            current_uri: None,
            def,
            open_file_data: None,
            workspace_scanned: false,
        }
    }

    /// textDocument/didOpen
    pub fn notify_open(
        &mut self,
        path: &PathBuf,
        text: &str,
        version: i32,
        workspace: Option<&PathBuf>,
    ) {
        if !self.workspace_scanned {
            self.workspace_scanned = true;
            if let Some(ws) = workspace {
                let tx = self.cmd_tx.clone();
                let lang = self.def.language_id;
                let ws = ws.clone();
                std::thread::spawn(move || {
                    for entry in ignore::Walk::new(&ws).flatten() {
                        let p = entry.path();
                        if p.extension().and_then(|s| s.to_str()) == Some("py") {
                            if let Ok(content) = std::fs::read_to_string(p) {
                                let uri = path_to_uri(&p.to_string_lossy());
                                let _ = tx.send(Cmd::Open {
                                    uri,
                                    lang,
                                    version: 1,
                                    text: content,
                                });
                            }
                        }
                    }
                });
            }
        }

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
        let _ = self.cmd_tx.send(Cmd::Change {
            uri,
            version,
            text: text.to_string(),
        });
    }

        pub fn request_hover(&mut self, path: &PathBuf, line: u32, col: u32) -> i32 {
        let id = next_id();
        let uri = path_to_uri(&path.to_string_lossy());
        let _ = self.cmd_tx.send(Cmd::Hover {
            id,
            uri,
            line,
            col,
        });
        id
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
        start_line: u32,
        start_col: u32,
        end_line: u32,
        end_col: u32,
        diagnostics: &[Diagnostic],
        only: Option<Vec<String>>,
    ) -> i32 {
        let id = next_id();
        let uri = path_to_uri(&path.to_string_lossy());

        // Кодируем диагностики в JSON для контекста запроса
        let diag_json = encode_diagnostics_json(diagnostics);

        let _ = self.cmd_tx.send(Cmd::CodeAction {
            id,
            uri,
            start_line,
            start_col,
            end_line,
            end_col,
            diagnostics_json: diag_json,
            only,
        });
        id
    }

    /// Опрашивает входящие события (non-blocking). Вызывать раз в кадр.
        pub fn poll(&self, events: &mut Vec<LspEvent>) {
        loop {
            match self.event_rx.try_recv() {
                Ok(e) => events.push(e),
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
    }

    pub fn shutdown(self) {
        let _ = self.cmd_tx.send(Cmd::Shutdown);
    }
}

// ── Encode diagnostics for codeAction context ────────────────────────────────

fn encode_diagnostics_json(diags: &[Diagnostic]) -> String {
    let mut out = String::from('[');
    for (i, d) in diags.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
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
    ty_process: Option<LspProcess>,
    workspaces: Vec<PathBuf>,
    /// Актуальные диагностики для каждого открытого файла
            pub diagnostics: HashMap<PathBuf, Vec<Diagnostic>>,
    pub instant_diagnostics: HashMap<PathBuf, (i32, Vec<Diagnostic>)>,
    pub merged_instant_diagnostics: HashMap<PathBuf, (i32, Vec<Diagnostic>)>,
    pub ty_instant_diagnostics: HashMap<PathBuf, (i32, Vec<Diagnostic>)>,
    pub dirty_diagnostics: bool,
    pub last_change: Option<std::time::Instant>,
    current_path: Option<PathBuf>,
    /// Статус ruff сервера
    pub python_status: LspServerStatus,
    pub ty_status: LspServerStatus,
    /// Отключён ли ruff вручную
    pub python_disabled: bool,
    pub server_logs: HashMap<&'static str, Vec<LogEntry>>,
    pub suppress_diagnostics: bool,
}

impl LspManager {
            pub fn new(workspaces: Vec<PathBuf>) -> Self {
        LspManager {
            python: None,
            ty_process: None,
            workspaces,
                        diagnostics: HashMap::new(),
            instant_diagnostics: HashMap::new(),
            ty_instant_diagnostics: HashMap::new(),
            merged_instant_diagnostics: HashMap::new(),
            dirty_diagnostics: false,
            last_change: None,
            current_path: None,
            python_status: LspServerStatus::Disabled,
            ty_status: LspServerStatus::Disabled,
            python_disabled: false,
            server_logs: HashMap::new(),
            suppress_diagnostics: false,
        }
    }

    /// Запускает нужный LSP-сервер если ещё не запущен (lazy)
            fn ensure_python(&mut self) {
        if self.python.is_none() && !self.python_disabled {
            self.python_status = LspServerStatus::Starting;
            self.python = Some(LspProcess::start(&RUFF_SERVER, self.workspaces.clone()));
        }
        if self.ty_process.is_none() && !self.python_disabled {
            self.ty_status = LspServerStatus::Starting;
            self.ty_process = Some(LspProcess::start(&TY_SERVER, self.workspaces.clone()));
        }
    }

    /// Перезапустить ruff сервер
    pub fn restart_python(&mut self) {
        if let Some(proc) = &mut self.python {
            proc.restart();
            self.python_status = LspServerStatus::Starting;
        } else if !self.python_disabled {
            self.python_status = LspServerStatus::Starting;
            self.python = Some(LspProcess::start(&RUFF_SERVER, self.workspaces.clone()));
        }
        if let Some(proc) = &mut self.ty_process {
            proc.restart();
            self.ty_status = LspServerStatus::Starting;
        } else if !self.python_disabled {
            self.ty_status = LspServerStatus::Starting;
            self.ty_process = Some(LspProcess::start(&TY_SERVER, self.workspaces.clone()));
        }
    }

    /// Отключить ruff (остановить и не перезапускать)
                        pub fn disable_python(&mut self) {
        self.python_disabled = true;
        self.python_status = LspServerStatus::Disabled;
        self.ty_status = LspServerStatus::Disabled;
        if let Some(p) = self.python.take() {
            p.shutdown();
        }
        if let Some(p) = self.ty_process.take() {
            p.shutdown();
        }
        self.diagnostics.clear();
        self.instant_diagnostics.clear();
        self.ty_instant_diagnostics.clear();
        self.merged_instant_diagnostics.clear();
        self.dirty_diagnostics = false;
        self.server_logs.clear();
    }

    /// Включить ruff обратно
    pub fn enable_python(&mut self) {
        self.python_disabled = false;
        self.python_status = LspServerStatus::Starting;
        self.ty_status = LspServerStatus::Starting;
        self.python = Some(LspProcess::start(&RUFF_SERVER, self.workspaces.clone()));
        self.ty_process = Some(LspProcess::start(&TY_SERVER, self.workspaces.clone()));
        // Re-open current file if any
        let ws = self.workspaces.first().cloned();
        if let Some(path) = &self.current_path.clone() {
            if let Some(proc) = &mut self.python {
                if let Some((_, text)) = &proc.open_file_data.clone() {
                    proc.notify_open(path, text, 1, ws.as_ref());
                }
            }
            if let Some(proc) = &mut self.ty_process {
                if let Some((_, text)) = &proc.open_file_data.clone() {
                    proc.notify_open(path, text, 1, ws.as_ref());
                }
            }
        }
    }

    /// Информация о серверах для UI
        pub fn servers_info(&self) -> Vec<LspServerInfo> {
        let logs = self
            .server_logs
            .get(RUFF_SERVER.program)
            .cloned()
            .unwrap_or_default();
        let ty_logs = self
            .server_logs
            .get(TY_SERVER.program)
            .cloned()
            .unwrap_or_default();
        vec![
            LspServerInfo {
                name: RUFF_SERVER.program,
                status: self.python_status.clone(),
                logs,
            },
            LspServerInfo {
                name: TY_SERVER.program,
                status: self.ty_status.clone(),
                logs: ty_logs,
            }
        ]
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
        self.suppress_diagnostics = false;
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        self.current_path = Some(abs_path.clone());
        let ws = self.workspaces.first().cloned();
        if ext == "py" {
            self.ensure_python();
            if let Some(proc) = &mut self.python {
                proc.notify_open(&abs_path, text, version, ws.as_ref());
            }
            if let Some(proc) = &mut self.ty_process {
                proc.notify_open(&abs_path, text, version, ws.as_ref());
            }
        }
    }

        /// Уведомляет LSP об изменении файла (когда sync_edits непуст)
        pub fn notify_change(&mut self, path: &PathBuf, ext: &str, text: &str, version: i32) {
        self.suppress_diagnostics = false;
        self.last_change = Some(std::time::Instant::now());
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        if ext == "py" {
            self.ensure_python();
            if let Some(proc) = &mut self.python {
                proc.notify_change(&abs_path, text, version);
            }
            if let Some(proc) = &mut self.ty_process {
                proc.notify_change(&abs_path, text, version);
            }
        }
    }

                pub fn request_hover(&mut self, path: &PathBuf, _ext: &str, line: u32, col: u32) -> Option<i32> {
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        if let Some(proc) = &mut self.ty_process {
            Some(proc.request_hover(&abs_path, line, col))
        } else {
            None
        }
    }

    /// Уведомляет LSP о закрытии файла
            pub fn notify_close(&mut self, path: &PathBuf, ext: &str) {
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        if ext == "py" {
            if let Some(proc) = &mut self.python {
                proc.notify_close(&abs_path);
            }
            if let Some(proc) = &mut self.ty_process {
                proc.notify_close(&abs_path);
            }
        }
    }

        /// Запрашивает code actions для позиции/диагностики
    pub fn request_code_actions(
        &mut self,
        path: &PathBuf,
        ext: &str,
        start_line: u32,
        start_col: u32,
        end_line: u32,
        end_col: u32,
        relevant_diags: &[Diagnostic],
        only: Option<Vec<String>>,
    ) -> Option<i32> {
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        let proc = self.process_for_ext(ext)?;
        Some(proc.request_code_actions(
            &abs_path,
            start_line,
            start_col,
            end_line,
            end_col,
            relevant_diags,
            only,
        ))
    }

    /// Опрашивает события от всех серверов. Вызывать раз в кадр.
    /// Обновляет self.diagnostics при получении новых диагностик.
        pub fn poll(&mut self) -> Vec<LspEvent> {
                let mut all = Vec::new();

        if let Some(proc) = &self.python {
            proc.poll(&mut all);
        }
        if let Some(proc) = &mut self.ty_process {
            proc.poll(&mut all);
        }

                // Обновляем кешированные диагностики и статусы
        for ev in &mut all {
            match ev {
                                                                LspEvent::Diagnostics { server_name, path, version, items, .. } => {
                    if !self.suppress_diagnostics {
                        let v = version.unwrap_or(0);

                                                if *server_name == TY_SERVER.program {
                            self.ty_instant_diagnostics.insert(path.clone(), (v, items.clone()));
                        } else {
                            self.instant_diagnostics.insert(path.clone(), (v, items.clone()));
                        }

                                                let mut merged = Vec::new();
                        let mut max_v = v;
                        if let Some((v1, d)) = self.instant_diagnostics.get(path.as_path()) {
                            merged.extend(d.clone());
                            max_v = max_v.max(*v1);
                        }
                        if let Some((v2, d)) = self.ty_instant_diagnostics.get(path.as_path()) {
                            merged.extend(d.clone());
                            max_v = max_v.max(*v2);
                        }
                        self.merged_instant_diagnostics.insert(path.clone(), (max_v, merged));

                        self.dirty_diagnostics = true;
                        self.last_change = None;
                    }
                }
                LspEvent::StatusChanged { name, status } => {
                    if *name == TY_SERVER.program {
                        self.ty_status = status.clone();
                    } else {
                        self.python_status = status.clone();
                    }
                }
                LspEvent::Log { name, message } => {
                    if message.len() > 5000 {
                        let mut split_at = 5000;
                        while split_at > 0 && !message.is_char_boundary(split_at) {
                            split_at -= 1;
                        }
                        message.truncate(split_at);
                        message.push_str("\n... [TRUNCATED TO SAVE RAM]");
                    }
                    let (final_text, spans, folds) = format_and_highlight_json(message);
                    *message = final_text.clone();
                    let logs = self.server_logs.entry(*name).or_insert_with(Vec::new);
                    logs.push(LogEntry {
                        text: final_text,
                        spans,
                        folds,
                    });
                    if logs.len() > 30 {
                        logs.remove(0);
                    }
                }
                                _ => {}
            }
        }

                                                                                                if let Some(t) = self.last_change {
            if t.elapsed().as_secs_f32() >= 3.0 {
                if self.dirty_diagnostics {
                    let mut paths = std::collections::HashSet::new();
                    for k in self.instant_diagnostics.keys() { paths.insert(k.as_path()); }
                    for k in self.ty_instant_diagnostics.keys() { paths.insert(k.as_path()); }

                    for path in paths {
                        let mut merged = Vec::new();
                        if let Some((_, d)) = self.instant_diagnostics.get(path) { merged.extend(d.iter().cloned()); }
                        if let Some((_, d)) = self.ty_instant_diagnostics.get(path) { merged.extend(d.iter().cloned()); }
                        self.diagnostics.insert(path.to_path_buf(), merged);
                    }
                    self.dirty_diagnostics = false;
                }
                self.last_change = None;
            }
        } else {
            if self.dirty_diagnostics {
                let mut paths = std::collections::HashSet::new();
                for k in self.instant_diagnostics.keys() { paths.insert(k.as_path()); }
                for k in self.ty_instant_diagnostics.keys() { paths.insert(k.as_path()); }

                for path in paths {
                    let mut merged = Vec::new();
                    if let Some((_, d)) = self.instant_diagnostics.get(path) { merged.extend(d.iter().cloned()); }
                    if let Some((_, d)) = self.ty_instant_diagnostics.get(path) { merged.extend(d.iter().cloned()); }
                    self.diagnostics.insert(path.to_path_buf(), merged);
                }
                self.dirty_diagnostics = false;
            }
        }

        all
    }

                pub fn get_diagnostics(&self, path: &PathBuf) -> &[Diagnostic] {
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        self.diagnostics
            .get(&abs_path)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

        pub fn get_instant_diagnostics_with_version(&self, path: &PathBuf) -> (i32, &[Diagnostic]) {
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        self.merged_instant_diagnostics
            .get(&abs_path)
                        .map(|(v, d)| (*v, d.as_slice()))
            .unwrap_or((0, &[]))
    }

    /// Диагностики для текущего файла, отфильтрованные по строке
    pub fn diagnostics_for_line(&self, path: &PathBuf, line: u32) -> Vec<&Diagnostic> {
        self.get_diagnostics(path)
            .iter()
            .filter(move |d| d.start_line == line)
            .collect()
    }

    /// Запрос на глобальный fix-all (source.fixAll) для текущего файла
        pub fn request_fix_all(&mut self, path: &PathBuf, ext: &str) -> Option<i32> {
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        let proc = self.process_for_ext(ext)?;
        let id = next_id();
        let uri = path_to_uri(&abs_path.to_string_lossy());
        let _ = proc.cmd_tx.send(Cmd::CodeAction {
            id,
            uri,
            start_line: 0,
            start_col: 0,
            end_line: u32::MAX,
            end_col: 0,
            diagnostics_json: String::from("[]"),
            only: Some(vec!["source.fixAll".to_string()]),
        });
        Some(id)
    }

        pub fn request_organize_imports(&mut self, path: &PathBuf, ext: &str) -> Option<i32> {
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        let proc = self.process_for_ext(ext)?;
        let id = next_id();
        let uri = path_to_uri(&abs_path.to_string_lossy());
        let _ = proc.cmd_tx.send(Cmd::CodeAction {
            id,
            uri,
            start_line: 0,
            start_col: 0,
            end_line: u32::MAX,
            end_col: 0,
            diagnostics_json: String::from("[]"),
            only: Some(vec!["source.organizeImports".to_string()]),
        });
        Some(id)
    }

        #[allow(dead_code)]
    pub fn shutdown(mut self) {
        self.python_disabled = true;
        if let Some(p) = self.python.take() {
            p.shutdown();
        }
        if let Some(p) = self.ty_process.take() {
            p.shutdown();
        }
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

pub fn format_and_highlight_json(
    raw_text: &str,
) -> (
    String,
    Vec<crate::highlighter::ColorSpan>,
    Vec<(usize, usize)>,
) {
    let (prefix, content) = if raw_text.starts_with("[LSP RECV] ") {
        ("[LSP RECV]\n", &raw_text[11..])
    } else if raw_text.starts_with("[LSP SEND] ") {
        ("[LSP SEND]\n", &raw_text[11..])
    } else {
        ("", raw_text)
    };

    let is_json = content.trim().starts_with('{') || content.trim().starts_with('[');
    let pretty = if is_json {
        match serde_json::from_str::<serde_json::Value>(content) {
            Ok(v) => serde_json::to_string_pretty(&v).unwrap_or_else(|_| content.to_string()),
            Err(_) => content.to_string(),
        }
    } else {
        content.to_string()
    };

    let mut parser = tree_sitter::Parser::new();
    let lang = if is_json {
        tree_sitter_json::LANGUAGE.into()
    } else {
        tree_sitter_bash::LANGUAGE.into()
    };
    let _ = parser.set_language(&lang);

    let mut final_string = String::from(prefix);
    let mut spans = vec![crate::highlighter::ColorSpan {
        start: 0,
        end: prefix.len(),
        color: if prefix.contains("RECV") {
            [0.313, 0.980, 0.482, 1.0]
        } else {
            [0.545, 0.913, 0.992, 1.0]
        },
    }];
    let mut folds = Vec::new();

    if is_json {
        let tree = parser.parse(&pretty, None).unwrap();

        if let Some(fold_q) = crate::queries::get_folding_query("json") {
            if let Ok(query) = tree_sitter::Query::new(&lang, fold_q) {
                let mut cursor = tree_sitter::QueryCursor::new();
                let mut matches = cursor.matches(&query, tree.root_node(), pretty.as_bytes());
                while let Some(m) = matches.next() {
                    for cap in m.captures {
                        let node = cap.node;
                        if node.end_position().row > node.start_position().row + 1 {
                            folds.push((
                                node.start_byte() + prefix.len(),
                                node.end_byte() + prefix.len(),
                            ));
                        }
                    }
                }
            }
        }

        if let Some((_, queries)) = crate::queries::get_ts_config("json") {
            for q in queries {
                if let Ok(query) = tree_sitter::Query::new(&lang, q) {
                    let mut cursor = tree_sitter::QueryCursor::new();
                    let mut matches = cursor.matches(&query, tree.root_node(), pretty.as_bytes());
                    while let Some(m) = matches.next() {
                        for cap in m.captures {
                            let name = query.capture_names()[cap.index as usize];
                            let color = match name {
                                "property" => [0.545, 0.913, 0.992, 1.0],
                                "string" => [0.945, 0.980, 0.549, 1.0],
                                "number" => [0.741, 0.576, 0.976, 1.0],
                                "boolean" => [1.0, 0.474, 0.776, 1.0],
                                "keyword.control" => [1.0, 0.474, 0.776, 1.0],
                                "comment" => [0.384, 0.447, 0.643, 1.0],
                                _ => continue,
                            };
                            spans.push(crate::highlighter::ColorSpan {
                                start: cap.node.start_byte() + prefix.len(),
                                end: cap.node.end_byte() + prefix.len(),
                                color,
                            });
                        }
                    }
                }
            }
        }
        final_string.push_str(&pretty);
    } else {
        final_string.push_str(&pretty);
        spans.push(crate::highlighter::ColorSpan {
            start: prefix.len(),
            end: final_string.len(),
            color: [0.875, 0.882, 0.902, 1.0],
        });
    }

    spans.sort_by_key(|s| s.start);
    (final_string, spans, folds)
}
