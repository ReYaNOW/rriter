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
use std::io::{BufRead, BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use tree_sitter::StreamingIterator;

mod hover;
mod protocol;

use hover::PendingRequestKind;
pub use hover::{HoverLineKindPublic, highlight_hover_text};
use protocol::*;
pub use protocol::{
    CodeAction, LspCompletionItem, LspEvent, TextChange, WorkspaceEdit,
    highlight_diagnostic_message, offset_to_lsp_pos,
};
use std::thread;
use std::time::{Duration, Instant};

// ── Atomic request ID ─────────────────────────────────────────────────────────

static NEXT_ID: AtomicI32 = AtomicI32::new(1);
const LSP_LOG_RETENTION: Duration = Duration::from_secs(120);
const LSP_LOG_MAX_ENTRIES: usize = 64;
const LSP_LOG_MAX_BYTES: usize = 512 * 1024;
const LSP_LOG_ENTRY_MAX_BYTES: usize = 64 * 1024;
const LSP_LOG_HIGHLIGHT_MAX_BYTES: usize = 8 * 1024;

#[inline(always)]
fn next_id() -> i32 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

fn compact_lsp_log_message(message: &str) -> String {
    if message.len() <= LSP_LOG_ENTRY_MAX_BYTES {
        return message.to_string();
    }
    let mut end = LSP_LOG_ENTRY_MAX_BYTES;
    while end > 0 && !message.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}\n... truncated {} bytes ...",
        &message[..end],
        message.len().saturating_sub(end)
    )
}

fn format_lsp_log_entry(
    message: &str,
) -> (
    String,
    Vec<crate::highlighter::ColorSpan>,
    Vec<(usize, usize, usize)>,
) {
    let compact_message = compact_lsp_log_message(message);
    if compact_message.len() > LSP_LOG_HIGHLIGHT_MAX_BYTES {
        return (compact_message, Vec::new(), Vec::new());
    }
    format_and_highlight_json(&compact_message)
}

fn trim_lsp_logs(logs: &mut Vec<LogEntry>, now: Instant) {
    logs.retain(|log| now.duration_since(log.created_at) <= LSP_LOG_RETENTION);
    while logs.len() > LSP_LOG_MAX_ENTRIES {
        logs.remove(0);
    }
    let mut total_bytes: usize = logs.iter().map(|log| log.text.len()).sum();
    while total_bytes > LSP_LOG_MAX_BYTES && logs.len() > 1 {
        let removed = logs.remove(0);
        total_bytes = total_bytes.saturating_sub(removed.text.len());
    }
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
    pub folds: Vec<(usize, usize, usize)>,
    pub created_at: Instant,
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

struct SpawnedProcess {
    child: Child,
    out_tx: Sender<Vec<u8>>,
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn spawn_server(
    def: &'static LspServerDef,
    workspace: Option<&Path>,
    event_tx: Sender<LspEvent>,
    pending_requests: Arc<Mutex<HashMap<i32, PendingRequestKind>>>,
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

                dispatch_frame(
                    &body,
                    &event_tx,
                    def.program,
                    &reader_out_tx,
                    &pending_requests,
                );
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
        let log_msg = format!("[LSP SEND] {}", remove_sent_log_text_fields(s));

        let _ = event_tx.send(LspEvent::Log {
            name: server_name,
            message: log_msg,
        });
    }
    out_tx.send(msg)
}

fn remove_sent_log_text_fields(raw: &str) -> String {
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return raw.to_string();
    };
    if remove_json_text_fields(&mut value) {
        serde_json::to_string(&value).unwrap_or_else(|_| raw.to_string())
    } else {
        raw.to_string()
    }
}

fn remove_json_text_fields(value: &mut serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(map) => {
            let mut changed = false;
            let keys = map.keys().cloned().collect::<Vec<_>>();
            for key in keys {
                let remove = map
                    .get(&key)
                    .is_some_and(|child| key == "text" && child.is_string());
                if remove {
                    map.remove(&key);
                    changed = true;
                } else if let Some(child) = map.get_mut(&key) {
                    if remove_json_text_fields(child) {
                        changed = true;
                    }
                }
            }
            changed
        }
        serde_json::Value::Array(items) => {
            let mut changed = false;
            for child in items {
                if remove_json_text_fields(child) {
                    changed = true;
                }
            }
            changed
        }
        _ => false,
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
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
    let pending_requests: Arc<Mutex<HashMap<i32, PendingRequestKind>>> =
        Arc::new(Mutex::new(HashMap::new()));

    'outer: loop {
        if let Ok(mut pending) = pending_requests.lock() {
            pending.clear();
        }
        let _ = event_tx.send(LspEvent::StatusChanged {
            name: def.program,
            status: LspServerStatus::Starting,
        });
        // ── Запускаем процесс ─────────────────────────────────────────
        let mut proc = match spawn_server(
            def,
            workspaces.first().map(|p| p.as_path()),
            event_tx.clone(),
            pending_requests.clone(),
        ) {
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
                        if let Ok(mut pending) = pending_requests.lock() {
                            pending.insert(id, PendingRequestKind::Hover);
                        }
                        let msg = make_hover(id, &uri, line, col);
                        if send_and_log(&proc.out_tx, &event_tx, def.program, msg).is_err() {
                            break 'inner;
                        }
                    }
                    Ok(Cmd::Definition { id, uri, line, col }) => {
                        if let Ok(mut pending) = pending_requests.lock() {
                            pending.insert(id, PendingRequestKind::Definition);
                        }
                        let msg = make_definition(id, &uri, line, col);
                        if send_and_log(&proc.out_tx, &event_tx, def.program, msg).is_err() {
                            break 'inner;
                        }
                    }
                    Ok(Cmd::Completion {
                        id,
                        uri,
                        line,
                        col,
                        trigger,
                    }) => {
                        if let Ok(mut pending) = pending_requests.lock() {
                            pending.insert(id, PendingRequestKind::Completion);
                        }
                        let msg = make_completion(id, &uri, line, col, trigger.as_deref());
                        if send_and_log(&proc.out_tx, &event_tx, def.program, msg).is_err() {
                            break 'inner;
                        }
                    }
                    Ok(Cmd::WorkspaceDiagnostic {
                        id,
                        previous_result_ids_json,
                    }) => {
                        if let Ok(mut pending) = pending_requests.lock() {
                            pending.insert(id, PendingRequestKind::WorkspaceDiagnostic);
                        }
                        let msg = make_workspace_diagnostic(id, &previous_result_ids_json);
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
                        if let Ok(mut pending) = pending_requests.lock() {
                            pending.insert(id, PendingRequestKind::CodeAction);
                        }
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
}

impl LspProcess {
    #[cfg_attr(coverage_nightly, coverage(off))]
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
        }
    }

    /// textDocument/didOpen
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn notify_open(
        &mut self,
        path: &PathBuf,
        text: &str,
        version: i32,
        _workspace: Option<&PathBuf>,
    ) {
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
        let _ = self.cmd_tx.send(Cmd::Hover { id, uri, line, col });
        id
    }

    pub fn request_definition(&mut self, path: &PathBuf, line: u32, col: u32) -> i32 {
        let id = next_id();
        let uri = path_to_uri(&path.to_string_lossy());
        let _ = self.cmd_tx.send(Cmd::Definition { id, uri, line, col });
        id
    }

    pub fn request_completion(
        &mut self,
        path: &PathBuf,
        line: u32,
        col: u32,
        trigger: Option<&str>,
    ) -> i32 {
        let id = next_id();
        let uri = path_to_uri(&path.to_string_lossy());
        let _ = self.cmd_tx.send(Cmd::Completion {
            id,
            uri,
            line,
            col,
            trigger: trigger.map(str::to_string),
        });
        id
    }

    pub fn request_workspace_diagnostics(&mut self, previous_result_ids_json: String) -> i32 {
        let id = next_id();
        let _ = self.cmd_tx.send(Cmd::WorkspaceDiagnostic {
            id,
            previous_result_ids_json,
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

fn merged_diagnostics_for_path(
    path: &Path,
    ruff: &HashMap<PathBuf, (i32, Vec<Diagnostic>)>,
    ty: &HashMap<PathBuf, (i32, Vec<Diagnostic>)>,
) -> (i32, Vec<Diagnostic>) {
    let mut merged = Vec::new();
    let mut max_v = 0;
    if let Some((v, d)) = ruff.get(path) {
        merged.extend(d.clone());
        max_v = max_v.max(*v);
    }
    if let Some((v, d)) = ty.get(path) {
        merged.extend(d.clone());
        max_v = max_v.max(*v);
    }
    (max_v, merged)
}

fn merged_diagnostics_for_all_paths(
    ruff: &HashMap<PathBuf, (i32, Vec<Diagnostic>)>,
    ty: &HashMap<PathBuf, (i32, Vec<Diagnostic>)>,
) -> Vec<(PathBuf, Vec<Diagnostic>)> {
    let mut paths = std::collections::HashSet::new();
    for k in ruff.keys() {
        paths.insert(k.clone());
    }
    for k in ty.keys() {
        paths.insert(k.clone());
    }

    let mut out = Vec::with_capacity(paths.len());
    for path in paths {
        let (_, merged) = merged_diagnostics_for_path(&path, ruff, ty);
        out.push((path, merged));
    }
    out
}

// ── LspManager: главный фасад для App ─────────────────────────────────────────

