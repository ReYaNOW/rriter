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

pub struct LspManager {
    python: Option<LspProcess>,
    ty_process: Option<LspProcess>,
    workspaces: Vec<PathBuf>,
    /// Актуальные диагностики для каждого открытого файла
    pub diagnostics: HashMap<PathBuf, Vec<Diagnostic>>,
    pub instant_diagnostics: HashMap<PathBuf, (i32, Vec<Diagnostic>)>,
    pub merged_instant_diagnostics: HashMap<PathBuf, (i32, Vec<Diagnostic>)>,
    pub ty_instant_diagnostics: HashMap<PathBuf, (i32, Vec<Diagnostic>)>,
    ty_diag_result_ids: HashMap<PathBuf, String>,
    ty_workspace_diag_pending: Option<i32>,
    ty_workspace_diag_dirty: bool,
    pub dirty_diagnostics: bool,
    pub last_change: Option<std::time::Instant>,
    current_path: Option<PathBuf>,
    current_python_file: Option<(PathBuf, String, i32)>,
    /// Статус ruff сервера
    pub python_status: LspServerStatus,
    pub ty_status: LspServerStatus,
    /// Отключён ли ruff вручную
    pub python_disabled: bool,
    pub server_logs: HashMap<&'static str, Vec<LogEntry>>,
    pub suppress_diagnostics: bool,
}

impl LspManager {
    fn is_python_ext(ext: &str) -> bool {
        matches!(ext, "py" | "pyi")
    }

    pub fn new(workspaces: Vec<PathBuf>) -> Self {
        LspManager {
            python: None,
            ty_process: None,
            workspaces,
            diagnostics: HashMap::new(),
            instant_diagnostics: HashMap::new(),
            ty_instant_diagnostics: HashMap::new(),
            merged_instant_diagnostics: HashMap::new(),
            ty_diag_result_ids: HashMap::new(),
            ty_workspace_diag_pending: None,
            ty_workspace_diag_dirty: false,
            dirty_diagnostics: false,
            last_change: None,
            current_path: None,
            current_python_file: None,
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
            self.ty_diag_result_ids.clear();
            self.ty_workspace_diag_pending = None;
            self.ty_workspace_diag_dirty = true;
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
            self.ty_diag_result_ids.clear();
            self.ty_workspace_diag_pending = None;
            self.ty_workspace_diag_dirty = true;
        } else if !self.python_disabled {
            self.ty_status = LspServerStatus::Starting;
            self.ty_process = Some(LspProcess::start(&TY_SERVER, self.workspaces.clone()));
            self.ty_diag_result_ids.clear();
            self.ty_workspace_diag_pending = None;
            self.ty_workspace_diag_dirty = true;
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
        self.ty_diag_result_ids.clear();
        self.ty_workspace_diag_pending = None;
        self.ty_workspace_diag_dirty = false;
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
        self.ty_diag_result_ids.clear();
        self.ty_workspace_diag_pending = None;
        self.ty_workspace_diag_dirty = true;
        self.reopen_current_python_file();
    }

    fn reopen_current_python_file(&mut self) {
        let Some((path, text, version)) = self.current_python_file.clone() else {
            return;
        };
        let ws = self.workspaces.first().cloned();
        if let Some(proc) = &mut self.python {
            proc.notify_open(&path, &text, version, ws.as_ref());
        }
        if let Some(proc) = &mut self.ty_process {
            proc.notify_open(&path, &text, version, ws.as_ref());
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
            },
        ]
    }

    pub fn clear_server_logs(&mut self, name: &str) {
        self.server_logs.remove(name);
    }

    /// Возвращает процесс для нужного расширения, запустив при необходимости
    fn process_for_ext(&mut self, ext: &str) -> Option<&mut LspProcess> {
        match ext {
            "py" | "pyi" => {
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
        if Self::is_python_ext(ext) {
            self.current_python_file = Some((abs_path.clone(), text.to_string(), version));
            self.ensure_python();
            if let Some(proc) = &mut self.python {
                proc.notify_open(&abs_path, text, version, ws.as_ref());
            }
            if let Some(proc) = &mut self.ty_process {
                proc.notify_open(&abs_path, text, version, ws.as_ref());
            }
            self.ty_workspace_diag_dirty = true;
        } else {
            self.current_python_file = None;
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
        if Self::is_python_ext(ext) {
            self.current_path = Some(abs_path.clone());
            self.current_python_file = Some((abs_path.clone(), text.to_string(), version));
            self.ensure_python();
            if let Some(proc) = &mut self.python {
                proc.notify_change(&abs_path, text, version);
            }
            if let Some(proc) = &mut self.ty_process {
                proc.notify_change(&abs_path, text, version);
            }
            self.ty_workspace_diag_dirty = true;
        }
    }

    pub fn request_hover(
        &mut self,
        path: &PathBuf,
        _ext: &str,
        line: u32,
        col: u32,
    ) -> Option<i32> {
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

    pub fn request_definition(
        &mut self,
        path: &PathBuf,
        _ext: &str,
        line: u32,
        col: u32,
    ) -> Option<i32> {
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        if let Some(proc) = &mut self.ty_process {
            Some(proc.request_definition(&abs_path, line, col))
        } else {
            None
        }
    }

    pub fn request_ty_completion(
        &mut self,
        path: &PathBuf,
        ext: &str,
        line: u32,
        col: u32,
        trigger: Option<&str>,
    ) -> Option<i32> {
        if !Self::is_python_ext(ext) {
            return None;
        }
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        self.ensure_python();
        self.ty_process
            .as_mut()
            .map(|proc| proc.request_completion(&abs_path, line, col, trigger))
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
        if Self::is_python_ext(ext) {
            if self.current_path.as_ref() == Some(&abs_path) {
                self.current_path = None;
            }
            if matches!(self.current_python_file.as_ref(), Some((path, _, _)) if path == &abs_path)
            {
                self.current_python_file = None;
            }
            if let Some(proc) = &mut self.python {
                proc.notify_close(&abs_path);
            }
            if let Some(proc) = &mut self.ty_process {
                proc.notify_close(&abs_path);
            }
            self.ty_workspace_diag_dirty = true;
        }
    }

    fn ty_workspace_result_ids_json(&self) -> String {
        if self.ty_diag_result_ids.is_empty() {
            return String::from("[]");
        }

        let mut items = Vec::with_capacity(self.ty_diag_result_ids.len());
        for (path, value) in &self.ty_diag_result_ids {
            let uri = path_to_uri(&path.to_string_lossy());
            items.push(format!(
                r#"{{"uri":"{}","value":"{}"}}"#,
                json_escape(&uri),
                json_escape(value)
            ));
        }
        format!("[{}]", items.join(","))
    }

    fn request_ty_workspace_diagnostics_if_ready(&mut self) {
        if !self.ty_workspace_diag_dirty
            || self.ty_workspace_diag_pending.is_some()
            || self.python_disabled
            || self.suppress_diagnostics
            || self.ty_status != LspServerStatus::Running
        {
            return;
        }
        if self
            .last_change
            .is_some_and(|last| last.elapsed().as_secs_f32() < 3.0)
        {
            return;
        }

        let previous_result_ids_json = self.ty_workspace_result_ids_json();
        if let Some(proc) = &mut self.ty_process {
            let id = proc.request_workspace_diagnostics(previous_result_ids_json);
            self.ty_workspace_diag_pending = Some(id);
            self.ty_workspace_diag_dirty = false;
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
                LspEvent::Diagnostics {
                    server_name,
                    path,
                    version,
                    items,
                    result_id,
                    ..
                } => {
                    if !self.suppress_diagnostics {
                        let v = version.unwrap_or(0);

                        if *server_name == TY_SERVER.program {
                            if let Some(result_id) = result_id.as_ref() {
                                self.ty_diag_result_ids
                                    .insert(path.clone(), result_id.clone());
                            }
                            self.ty_instant_diagnostics
                                .insert(path.clone(), (v, items.clone()));
                        } else {
                            self.instant_diagnostics
                                .insert(path.clone(), (v, items.clone()));
                        }

                        let (max_v, merged) = merged_diagnostics_for_path(
                            path,
                            &self.instant_diagnostics,
                            &self.ty_instant_diagnostics,
                        );
                        self.merged_instant_diagnostics
                            .insert(path.clone(), (max_v, merged));

                        self.dirty_diagnostics = true;
                        self.last_change = None;
                    }
                }
                LspEvent::StatusChanged { name, status } => {
                    if *name == TY_SERVER.program {
                        self.ty_status = status.clone();
                        if *status == LspServerStatus::Running {
                            self.ty_workspace_diag_dirty = true;
                        } else if *status == LspServerStatus::Starting
                            || *status == LspServerStatus::Crashed
                            || *status == LspServerStatus::Disabled
                        {
                            self.ty_workspace_diag_pending = None;
                        }
                    } else {
                        self.python_status = status.clone();
                    }
                }
                LspEvent::ConfigurationServed { name } => {
                    if *name == TY_SERVER.program {
                        self.ty_workspace_diag_dirty = true;
                    }
                }
                LspEvent::WorkspaceDiagnosticsDone { request_id } => {
                    if self.ty_workspace_diag_pending == Some(*request_id) {
                        self.ty_workspace_diag_pending = None;
                    }
                }
                LspEvent::Log { name, message } => {
                    let compact_message = compact_lsp_log_message(message);
                    let (final_text, spans, folds) = format_and_highlight_json(&compact_message);
                    *message = final_text.clone();
                    let logs = self.server_logs.entry(*name).or_insert_with(Vec::new);
                    let now = Instant::now();
                    logs.push(LogEntry {
                        text: final_text,
                        spans,
                        folds,
                        created_at: now,
                    });
                    trim_lsp_logs(logs, now);
                }
                _ => {}
            }
        }

        if let Some(t) = self.last_change {
            if t.elapsed().as_secs_f32() >= 3.0 {
                if self.dirty_diagnostics {
                    for (path, merged) in merged_diagnostics_for_all_paths(
                        &self.instant_diagnostics,
                        &self.ty_instant_diagnostics,
                    ) {
                        self.diagnostics.insert(path, merged);
                    }
                    self.dirty_diagnostics = false;
                }
                self.last_change = None;
            }
        } else {
            if self.dirty_diagnostics {
                for (path, merged) in merged_diagnostics_for_all_paths(
                    &self.instant_diagnostics,
                    &self.ty_instant_diagnostics,
                ) {
                    self.diagnostics.insert(path, merged);
                }
                self.dirty_diagnostics = false;
            }
        }

        self.request_ty_workspace_diagnostics_if_ready();

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

    pub fn clear_diagnostics_for_path(&mut self, path: &PathBuf) {
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        self.diagnostics.remove(&abs_path);
        self.instant_diagnostics.remove(&abs_path);
        self.ty_instant_diagnostics.remove(&abs_path);
        self.merged_instant_diagnostics.remove(&abs_path);
        self.ty_diag_result_ids.remove(&abs_path);
        self.dirty_diagnostics = false;
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

    pub fn has_stale_instant_diagnostics(&self, path: &PathBuf, editor_version: u64) -> bool {
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        let is_stale = |diags: &HashMap<PathBuf, (i32, Vec<Diagnostic>)>| {
            diags
                .get(&abs_path)
                .is_some_and(|(version, _)| (*version as u64) < editor_version)
        };
        is_stale(&self.instant_diagnostics) || is_stale(&self.ty_instant_diagnostics)
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
    Vec<(usize, usize, usize)>,
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
                                json_container_depth(node),
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

fn json_container_depth(node: tree_sitter::Node<'_>) -> usize {
    let mut depth = 1;
    let mut parent = node.parent();
    while let Some(p) = parent {
        if matches!(p.kind(), "object" | "array") {
            depth += 1;
        }
        parent = p.parent();
    }
    depth
}

#[cfg(test)]
mod lsp_tests;
