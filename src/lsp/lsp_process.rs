// src/lsp.rs
// Быстрый LSP-клиент для RRiter.
// Поддерживает Ruff и Ty; новые серверы описываются через LspServerDef.
//
// Архитектура:
//   Main Thread ──Cmd──▶ Supervisor Thread ──bytes──▶ Writer Thread ──▶ stdin
//                  ◀──LspEvent──   ◀──LspEvent── Reader Thread ◀── stdout
//
// Supervisor владеет полным деревом процесса. После краша он использует
// ограниченный exponential backoff; отсутствующий сервер отключается без
// restart-spam. При рестарте заново отправляются initialize и didOpen.
// Writer/Reader — легковесные треды, по одному на направление I/O.

use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use tree_sitter::StreamingIterator;

mod hover;
mod protocol;

use hover::PendingRequestKind;
pub use hover::{HoverLineKindPublic, highlight_hover_text};
use protocol::*;
pub use protocol::{
    CodeAction, LspCompletionItem, LspEvent, LspInlayHint, TextChange, WorkspaceEdit,
    highlight_diagnostic_message, offset_to_lsp_pos,
};
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::platform::{self, ManagedChild};

// ── Atomic request ID ─────────────────────────────────────────────────────────

static NEXT_ID: AtomicI32 = AtomicI32::new(1);
const LSP_LOG_RETENTION: Duration = Duration::from_secs(120);
const LSP_LOG_MAX_ENTRIES: usize = 64;
const LSP_LOG_MAX_BYTES: usize = 512 * 1024;
const LSP_LOG_ENTRY_MAX_BYTES: usize = 64 * 1024;
const LSP_LOG_HIGHLIGHT_MAX_BYTES: usize = 8 * 1024;
const LSP_MAX_CONSECUTIVE_ATTEMPTS: u8 = 4;
const LSP_STABLE_RUNTIME: Duration = Duration::from_secs(30);

#[derive(Default)]
struct LspRestartBudget {
    consecutive_attempts: u8,
}

impl LspRestartBudget {
    fn begin_attempt(&mut self) -> Option<u8> {
        if self.consecutive_attempts >= LSP_MAX_CONSECUTIVE_ATTEMPTS {
            return None;
        }
        self.consecutive_attempts += 1;
        Some(self.consecutive_attempts)
    }

    fn mark_stable(&mut self) {
        self.consecutive_attempts = 0;
    }
}

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
    let mut spans = Vec::new();
    if compact_message.len() <= LSP_LOG_HIGHLIGHT_MAX_BYTES {
        let prefix_end = compact_message
            .find('\n')
            .or_else(|| compact_message.find(']').map(|idx| idx + 1))
            .unwrap_or(compact_message.len());
        let color = if compact_message.contains("[LSP RECV]") {
            [0.313, 0.980, 0.482, 1.0]
        } else {
            [0.545, 0.913, 0.992, 1.0]
        };
        spans.push(crate::highlighter::ColorSpan {
            start: 0,
            end: prefix_end,
            color,
        });
    }
    (compact_message, spans, Vec::new())
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
    Missing,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickFix {
    pub title: String,
    pub edits: Vec<TextChange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// 0-based
    pub start_line: u32,
    /// 0-based, UTF-16 code units (для ASCII = байтовый столбец)
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub severity: DiagSeverity,
    /// Код ошибки (например "E501", "F401")
    pub code: Option<Arc<str>>,
    /// Ссылка на документацию (из codeDescription.href)
    pub code_href: Option<Arc<str>>,
    pub message: Arc<str>,
    pub source: Option<Arc<str>>,
    pub quickfixes: Box<[QuickFix]>,
    pub tags: Box<[u32]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiagnosticSourceKind {
    Legacy,
    Ruff,
    Ty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MergedDiagnosticIndex {
    source: DiagnosticSourceKind,
    index: usize,
}

struct SpawnedProcess {
    child: ManagedChild,
    out_tx: Sender<Vec<u8>>,
}

fn abort_spawned_child(child: &mut ManagedChild, error: io::Error) -> io::Error {
    let _ = child.terminate(Duration::from_millis(100));
    error
}

fn missing_process_pipe(child: &mut ManagedChild, pipe: &'static str) -> io::Error {
    abort_spawned_child(
        child,
        io::Error::new(
            io::ErrorKind::BrokenPipe,
            format!("spawned LSP process has no {pipe} pipe"),
        ),
    )
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn spawn_server(
    def: &'static LspServerDef,
    workspace: Option<&Path>,
    event_tx: Sender<LspEvent>,
    pending_requests: Arc<Mutex<HashMap<i32, PendingRequestKind>>>,
) -> io::Result<SpawnedProcess> {
    let mut cmd = platform::command_for_tool(def.program.as_ref(), def.override_env)?;
    if let Some(ws) = workspace.filter(|ws| ws.is_dir()) {
        cmd.current_dir(ws);
    }
    for arg in def.args {
        cmd.arg(arg);
    }
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = ManagedChild::spawn(&mut cmd)?;

    let stdin = match child.take_stdin() {
        Some(stdin) => stdin,
        None => return Err(missing_process_pipe(&mut child, "stdin")),
    };
    let stdout = match child.take_stdout() {
        Some(stdout) => stdout,
        None => return Err(missing_process_pipe(&mut child, "stdout")),
    };
    let stderr = match child.take_stderr() {
        Some(stderr) => stderr,
        None => return Err(missing_process_pipe(&mut child, "stderr")),
    };

    let (out_tx, out_rx) = mpsc::channel::<Vec<u8>>();
    let reader_out_tx = out_tx.clone();

    let err_tx = event_tx.clone();
    let srv_name = def.program;
    if let Err(error) = thread::Builder::new()
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
    {
        return Err(abort_spawned_child(&mut child, error));
    }

    // Тред-писатель: получает байты, оборачивает в Content-Length фрейм
    if let Err(error) = thread::Builder::new()
        .name("lsp-writer".into())
        .spawn(move || {
            let mut writer = BufWriter::with_capacity(128 * 1024, stdin);
            for body in out_rx {
                if !write_frame(&mut writer, &body) {
                    break;
                }
            }
        })
    {
        return Err(abort_spawned_child(&mut child, error));
    }

    // Тред-читатель: парсит stdout и шлёт события
    if let Err(error) = thread::Builder::new()
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
    {
        return Err(abort_spawned_child(&mut child, error));
    }

    Ok(SpawnedProcess { child, out_tx })
}

// ── Supervisor тред ───────────────────────────────────────────────────────────

/// Состояние supervisor: что открыто сейчас (для реопена после рестарта)
#[derive(Clone)]
struct OpenFile {
    uri: String,
    lang: &'static str,
    version: i32,
    text: Arc<str>,
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

fn disable_lsp_server(
    event_tx: &Sender<LspEvent>,
    server_name: &'static str,
    message: String,
) {
    let _ = event_tx.send(LspEvent::Log {
        name: server_name,
        message,
    });
    let _ = event_tx.send(LspEvent::StatusChanged {
        name: server_name,
        status: LspServerStatus::Disabled,
    });
}

fn report_missing_lsp_server(
    event_tx: &Sender<LspEvent>,
    server_name: &'static str,
    override_env: &'static str,
    error: &io::Error,
) {
    let _ = event_tx.send(LspEvent::Log {
        name: server_name,
        message: format!(
            "[LSP] '{server_name}' was not found; install it, add it to PATH, or set {override_env}: {error}"
        ),
    });
    let _ = event_tx.send(LspEvent::StatusChanged {
        name: server_name,
        status: LspServerStatus::Missing,
    });
}

fn wait_interruptibly(stop: &AtomicBool, duration: Duration) -> bool {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        if stop.load(Ordering::Acquire) {
            return false;
        }
        thread::sleep(
            Duration::from_millis(10)
                .min(deadline.saturating_duration_since(Instant::now())),
        );
    }
    !stop.load(Ordering::Acquire)
}

fn shutdown_spawned_process(
    proc: &mut SpawnedProcess,
    event_tx: &Sender<LspEvent>,
    server_name: &'static str,
) {
    let sid = next_id();
    let _ = send_and_log(&proc.out_tx, event_tx, server_name, make_shutdown(sid));
    thread::sleep(Duration::from_millis(100));
    let _ = send_and_log(&proc.out_tx, event_tx, server_name, make_exit());
    match proc.child.wait_timeout(Duration::from_millis(750)) {
        Ok(Some(_)) => {}
        Ok(None) | Err(_) => {
            let _ = proc.child.terminate(Duration::from_millis(150));
        }
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn run_supervisor(
    def: &'static LspServerDef,
    workspaces: Vec<PathBuf>,
    cmd_rx: Receiver<Cmd>,
    event_tx: Sender<LspEvent>,
    stop: Arc<AtomicBool>,
) {
    let mut open_file: Option<OpenFile> = None;
    let mut init_id;
    let mut restart_delay = Duration::from_millis(500);
    let mut restart_budget = LspRestartBudget::default();
    let pending_requests: Arc<Mutex<HashMap<i32, PendingRequestKind>>> =
        Arc::new(Mutex::new(HashMap::new()));

    'outer: loop {
        if stop.load(Ordering::Acquire) {
            return;
        }
        let Some(attempt) = restart_budget.begin_attempt() else {
            disable_lsp_server(
                &event_tx,
                def.program,
                format!(
                    "[LSP] '{}' disabled after {} consecutive start/crash attempts; fix the server and use Restart to try again",
                    def.program, LSP_MAX_CONSECUTIVE_ATTEMPTS
                ),
            );
            return;
        };
        if attempt > 1 {
            if !wait_interruptibly(&stop, restart_delay) {
                return;
            }
            restart_delay = (restart_delay * 2).min(Duration::from_secs(10));
        }
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
            Ok(p) => p,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                report_missing_lsp_server(
                    &event_tx,
                    def.program,
                    def.override_env,
                    &error,
                );
                return;
            }
            Err(error) => {
                let _ = event_tx.send(LspEvent::Log {
                    name: def.program,
                    message: format!(
                        "[LSP] failed to start '{}' (attempt {}/{}): {}",
                        def.program, attempt, LSP_MAX_CONSECUTIVE_ATTEMPTS, error
                    ),
                });
                let _ = event_tx.send(LspEvent::StatusChanged {
                    name: def.program,
                    status: LspServerStatus::Crashed,
                });
                continue 'outer;
            }
        };

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
            if stop.load(Ordering::Acquire) {
                shutdown_spawned_process(&mut proc, &event_tx, def.program);
                return;
            }
            // Проверяем crash
            match proc.child.try_wait() {
                Ok(Some(_)) => continue 'outer,
                Ok(None) => {}
                Err(_) => continue 'outer,
            }
            // Ждём немного - initialize ответ придёт через reader тред в event_tx
            // Но нам нужно знать когда сервер готов — используем специальный подход:
            // просто ждём 200мс (ruff server стартует быстро), потом шлём initialized
            if !wait_interruptibly(&stop, Duration::from_millis(200)) {
                shutdown_spawned_process(&mut proc, &event_tx, def.program);
                return;
            }
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
        let running_since = Instant::now();

        // Если был открыт файл — reopenуем после рестарта
        if let Some(ref of) = open_file {
            let msg = make_did_open(&of.uri, of.lang, of.version, of.text.as_ref());
            if send_and_log(&proc.out_tx, &event_tx, def.program, msg).is_err() {
                continue 'outer;
            }
        }

        // ── Основной цикл supervisor ──────────────────────────────────────
        'inner: loop {
            if stop.load(Ordering::Acquire) {
                shutdown_spawned_process(&mut proc, &event_tx, def.program);
                return;
            }
            if restart_budget.consecutive_attempts != 0
                && running_since.elapsed() >= LSP_STABLE_RUNTIME
            {
                restart_budget.mark_stable();
                restart_delay = Duration::from_millis(500);
            }
            // Проверяем краш процесса
            match proc.child.try_wait() {
                Ok(Some(status)) => {
                    let _ = event_tx.send(LspEvent::Log {
                        name: def.program,
                        message: format!(
                            "[LSP] '{}' exited unexpectedly with status {}",
                            def.program, status
                        ),
                    });
                    let _ = event_tx.send(LspEvent::StatusChanged {
                        name: def.program,
                        status: LspServerStatus::Crashed,
                    });
                    break 'inner; // рестарт
                }
                Ok(None) => {}
                Err(error) => {
                    let _ = event_tx.send(LspEvent::Log {
                        name: def.program,
                        message: format!(
                            "[LSP] failed to query '{}' process status: {}",
                            def.program, error
                        ),
                    });
                    let _ = event_tx.send(LspEvent::StatusChanged {
                        name: def.program,
                        status: LspServerStatus::Crashed,
                    });
                    break 'inner;
                }
            }

            // Обрабатываем команды от главного треда
            loop {
                match cmd_rx.try_recv() {
                    Ok(Cmd::Open {
                        uri,
                        lang,
                        version,
                        text,
                    }) => {
                        let msg = make_did_open(&uri, lang, version, text.as_ref());
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
                        let msg = make_did_change_full(&uri, version, text.as_ref());
                        if send_and_log(&proc.out_tx, &event_tx, def.program, msg).is_err() {
                            break 'inner;
                        }
                    }
                    Ok(Cmd::Close { uri }) => {
                        if let Some(ref of) = open_file && of.uri == uri {
                            let msg = make_did_close(&uri);
                            let _ = send_and_log(&proc.out_tx, &event_tx, def.program, msg);
                            open_file = None;
                        }
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
                    Ok(Cmd::SignatureHelp {
                        id,
                        uri,
                        line,
                        col,
                        trigger,
                    }) => {
                        if let Ok(mut pending) = pending_requests.lock() {
                            pending.insert(id, PendingRequestKind::SignatureHelp);
                        }
                        let msg = make_signature_help(id, &uri, line, col, trigger.as_deref());
                        if send_and_log(&proc.out_tx, &event_tx, def.program, msg).is_err() {
                            break 'inner;
                        }
                    }
                    Ok(Cmd::InlayHint {
                        id,
                        uri,
                        start_line,
                        start_col,
                        end_line,
                        end_col,
                    }) => {
                        if let Ok(mut pending) = pending_requests.lock() {
                            pending.insert(id, PendingRequestKind::InlayHint);
                        }
                        let msg =
                            make_inlay_hint(id, &uri, start_line, start_col, end_line, end_col);
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
                        stop.store(true, Ordering::Release);
                        shutdown_spawned_process(&mut proc, &event_tx, def.program);
                        return; // выходим из supervisor насовсем
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        shutdown_spawned_process(&mut proc, &event_tx, def.program);
                        return;
                    }
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
    pub open_file_data: Option<(String, Arc<str>)>, // (lang, text) for re-open after restart
    stop: Arc<AtomicBool>,
    supervisor: Option<JoinHandle<()>>,
}

impl LspProcess {
    #[cfg_attr(coverage_nightly, coverage(off))]
    fn start(def: &'static LspServerDef, workspaces: Vec<PathBuf>) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let ws = workspaces.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let supervisor_stop = stop.clone();

        let supervisor_event_tx = event_tx.clone();
        let supervisor = match thread::Builder::new()
            .name(format!("lsp-supervisor-{}", def.program))
            .spawn(move || {
                run_supervisor(def, ws, cmd_rx, supervisor_event_tx, supervisor_stop)
            })
        {
            Ok(supervisor) => Some(supervisor),
            Err(error) => {
                let _ = event_tx.send(LspEvent::Log {
                    name: def.program,
                    message: format!("[LSP] failed to start supervisor thread: {error}"),
                });
                let _ = event_tx.send(LspEvent::StatusChanged {
                    name: def.program,
                    status: LspServerStatus::Disabled,
                });
                stop.store(true, Ordering::Release);
                None
            }
        };

        LspProcess {
            cmd_tx,
            event_rx,
            current_uri: None,
            def,
            open_file_data: None,
            stop,
            supervisor,
        }
    }

    /// textDocument/didOpen
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn notify_open(
        &mut self,
        path: &PathBuf,
        text: Arc<str>,
        version: i32,
        _workspace: Option<&PathBuf>,
    ) {
        let uri = path_to_uri(path);
        self.current_uri = Some(uri.clone());
        self.open_file_data = Some((self.def.language_id.to_string(), text.clone()));
        let _ = self.cmd_tx.send(Cmd::Open {
            uri,
            lang: self.def.language_id,
            version,
            text,
        });
    }

    /// textDocument/didChange — полный текст (Full Sync).
    /// Вызывать когда editor.sync_edits непуст.
    pub fn notify_change(&mut self, path: &PathBuf, text: Arc<str>, version: i32) {
        let uri = path_to_uri(path);
        self.current_uri = Some(uri.clone());
        let _ = self.cmd_tx.send(Cmd::Change {
            uri,
            version,
            text,
        });
    }

    pub fn request_hover(&mut self, path: &PathBuf, line: u32, col: u32) -> i32 {
        let id = next_id();
        let uri = path_to_uri(path);
        let _ = self.cmd_tx.send(Cmd::Hover { id, uri, line, col });
        id
    }

    pub fn request_definition(&mut self, path: &PathBuf, line: u32, col: u32) -> i32 {
        let id = next_id();
        let uri = path_to_uri(path);
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
        let uri = path_to_uri(path);
        let _ = self.cmd_tx.send(Cmd::Completion {
            id,
            uri,
            line,
            col,
            trigger: trigger.map(str::to_string),
        });
        id
    }

    pub fn request_signature_help(
        &mut self,
        path: &PathBuf,
        line: u32,
        col: u32,
        trigger: Option<&str>,
    ) -> i32 {
        let id = next_id();
        let uri = path_to_uri(path);
        let _ = self.cmd_tx.send(Cmd::SignatureHelp {
            id,
            uri,
            line,
            col,
            trigger: trigger.map(str::to_string),
        });
        id
    }

    pub fn request_inlay_hints(
        &mut self,
        path: &PathBuf,
        start_line: u32,
        start_col: u32,
        end_line: u32,
        end_col: u32,
    ) -> i32 {
        let id = next_id();
        let uri = path_to_uri(path);
        let _ = self.cmd_tx.send(Cmd::InlayHint {
            id,
            uri,
            start_line,
            start_col,
            end_line,
            end_col,
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
        let uri = path_to_uri(path);
        if self.current_uri.as_deref() == Some(uri.as_str()) {
            let _ = self.cmd_tx.send(Cmd::Close { uri });
            self.current_uri = None;
            self.open_file_data = None;
        }
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
        let uri = path_to_uri(path);

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

    fn stop_and_join(&mut self) {
        if !self.stop.swap(true, Ordering::AcqRel) {
            let _ = self.cmd_tx.send(Cmd::Shutdown);
        }
        if let Some(supervisor) = self.supervisor.take() {
            let _ = supervisor.join();
        }
    }

    pub fn shutdown(mut self) {
        self.stop_and_join();
    }
}

impl Drop for LspProcess {
    fn drop(&mut self) {
        self.stop_and_join();
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

#[allow(dead_code)]
fn merged_diagnostics_for_path(
    path: &Path,
    ruff: &HashMap<PathBuf, (i32, Arc<[Diagnostic]>)>,
    ty: &HashMap<PathBuf, (i32, Arc<[Diagnostic]>)>,
) -> (i32, Arc<[Diagnostic]>) {
    match (ruff.get(path), ty.get(path)) {
        (Some((rv, rd)), Some((tv, td))) => {
            if rd.is_empty() {
                return (*tv, td.clone());
            }
            if td.is_empty() {
                return (*rv, rd.clone());
            }
            let mut merged = Vec::with_capacity(rd.len() + td.len());
            merged.extend(rd.iter().cloned());
            merged.extend(td.iter().cloned());
            ((*rv).max(*tv), Arc::from(merged.into_boxed_slice()))
        }
        (Some((v, d)), None) | (None, Some((v, d))) => (*v, d.clone()),
        (None, None) => (0, Arc::from([])),
    }
}

#[allow(dead_code)]
fn merged_diagnostics_for_all_paths(
    ruff: &HashMap<PathBuf, (i32, Arc<[Diagnostic]>)>,
    ty: &HashMap<PathBuf, (i32, Arc<[Diagnostic]>)>,
) -> Vec<(PathBuf, Arc<[Diagnostic]>)> {
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
