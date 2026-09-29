use crate::platform::{
    CURRENT_PLATFORM, PlatformKind, ProcessOutputStream, ToolKind, resolve_executable,
    resolve_tool_kind, run_command_output_cancelable, run_command_streaming_cancelable,
};
use crate::scroll::ScrollState;
use crate::ui_waker::{UiWaker, WakeSyncSender};
use std::ffi::{OsStr, OsString};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, TrySendError};
use std::thread::JoinHandle;
use std::time::Duration;

const UV_INSTALL_URL_UNIX: &str = "https://astral.sh/uv/install.sh";
const UV_INSTALL_URL_WINDOWS: &str = "https://astral.sh/uv/install.ps1";
const MAX_INSTALLER_BYTES: usize = 2 * 1024 * 1024;
const INSTALL_EVENT_CAPACITY: usize = 256;
const INSTALL_LOG_LIMIT: usize = 4096;
const INSTALL_LOG_BYTES_LIMIT: usize = 1024 * 1024;
const INSTALL_LINE_BYTES_LIMIT: usize = 16 * 1024;
const INSTALL_CANCELLED_MESSAGE: &str = "Установка отменена";
// Downloads are bounded by inactivity, not by total time: a slow but steady link must
// finish. `DOWNLOAD_TOTAL_LIMIT` is only a hard ceiling for a link that trickles forever.
const DOWNLOAD_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const DOWNLOAD_STALL_TIMEOUT: Duration = Duration::from_secs(30);
const DOWNLOAD_TOTAL_LIMIT: Duration = Duration::from_secs(60 * 60);
// An operation directory this old cannot belong to a running install (see `DOWNLOAD_TOTAL_LIMIT`).
const STALE_OP_DIR_MIN_AGE: Duration = Duration::from_secs(2 * 60 * 60);
const MAX_STALE_OP_DIRS_PER_RUN: usize = 32;
const UV_INSTALL_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const TOOL_INSTALL_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const TOOL_VALIDATE_TIMEOUT: Duration = Duration::from_secs(30);
const DART_VERSION_TIMEOUT: Duration = Duration::from_secs(10);
const LOG_LINE_HEIGHT: f32 = 17.0;
const TRUNCATED_LOG_MARKER: &str = "… начало журнала удалено из-за ограничения памяти …";

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DartToolStatus {
    #[default]
    NotFound,
    Checking,
    Ready,
    Installing,
    Updating,
    Cancelling,
    Error,
}
impl DartToolStatus {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::NotFound => "Не найден",
            Self::Checking => "Проверка",
            Self::Ready => "Готов",
            Self::Installing => "Установка",
            Self::Updating => "Обновление",
            Self::Cancelling => "Отмена",
            Self::Error => "Ошибка",
        }
    }
}
#[derive(Debug)]
struct DartProbeResult {
    generation: u64,
    result: Result<String, String>,
}

pub(crate) struct DartToolState {
    status: DartToolStatus,
    path: Option<PathBuf>,
    sdk_root: Option<PathBuf>,
    source: Option<&'static str>,
    version: Option<String>,
    error: Option<String>,
    generation: u64,
    rx: Option<Receiver<DartProbeResult>>,
    cancel: Option<Arc<AtomicBool>>,
    worker: Option<JoinHandle<()>>,
}
impl Default for DartToolState {
    fn default() -> Self {
        Self {
            status: DartToolStatus::NotFound,
            path: None,
            sdk_root: None,
            source: None,
            version: None,
            error: None,
            generation: 0,
            rx: None,
            cancel: None,
            worker: None,
        }
    }
}
impl DartToolState {
    pub(crate) fn status(&self) -> DartToolStatus {
        self.status
    }

    pub(crate) fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub(crate) fn sdk_root(&self) -> Option<&Path> {
        self.sdk_root.as_deref()
    }

    pub(crate) fn source(&self) -> Option<&'static str> {
        self.source
    }

    pub(crate) fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub(crate) fn refresh(&mut self, workspace: Option<&Path>, ui_waker: &UiWaker) {
        self.cancel_probe();
        crate::platform::configure_dart_workspace_root(workspace.map(Path::to_path_buf));
        let resolution = resolve_tool_kind(ToolKind::Dart);
        self.path = resolution.path.clone();
        self.sdk_root = resolution.sdk_root.clone();
        self.source = resolution.source_label(ToolKind::Dart);
        self.version = None;
        self.error = None;
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        let Some(path) = self.path.clone() else {
            self.status = if resolution.is_invalid_override() {
                self.error = Some("Выбранный Dart SDK недоступен".to_string());
                DartToolStatus::Error
            } else {
                DartToolStatus::NotFound
            };
            return;
        };

        self.status = DartToolStatus::Checking;
        let (tx, rx) = ui_waker.sync_channel(1);
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        match crate::platform::spawn_named("rriter-dart-version", move || {
            let result = probe_dart_version(&path, &worker_cancel);
            let _ = tx.send(DartProbeResult { generation, result });
        }) {
            Ok(worker) => {
                self.rx = Some(rx);
                self.cancel = Some(cancel);
                self.worker = Some(worker);
            }
            Err(error) => {
                self.status = DartToolStatus::Error;
                self.error = Some(format!("Не удалось запустить проверку Dart: {error}"));
            }
        }
    }

    pub(crate) fn poll(&mut self) -> bool {
        let Some(rx) = self.rx.as_ref() else {
            self.join_finished_worker();
            return false;
        };
        let result = match rx.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(DartProbeResult {
                generation: self.generation,
                result: Err("Проверка Dart завершилась без результата".to_string()),
            }),
        };
        let Some(result) = result else {
            self.join_finished_worker();
            return false;
        };
        self.rx = None;
        self.cancel = None;
        self.join_terminal_worker();
        if result.generation != self.generation {
            return false;
        }
        match result.result {
            Ok(version) => {
                self.status = DartToolStatus::Ready;
                self.version = Some(version);
                self.error = None;
            }
            Err(error) => {
                self.status = DartToolStatus::Error;
                self.version = None;
                self.error = Some(error);
            }
        }
        true
    }

    fn cancel_probe(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::Release);
        }
        self.rx = None;
        if let Some(worker) = self.worker.take() {
            crate::platform::reap_unit_thread(worker);
        }
    }

    fn join_finished_worker(&mut self) {
        if self.worker.as_ref().is_some_and(JoinHandle::is_finished)
            && let Some(worker) = self.worker.take()
        {
            let _ = worker.join();
        }
    }

    fn join_terminal_worker(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl Drop for DartToolState {
    fn drop(&mut self) {
        self.cancel_probe();
    }
}
fn probe_dart_version(path: &Path, cancel: &AtomicBool) -> Result<String, String> {
    let mut command = Command::new(path);
    command.arg("--version");
    let output = run_command_output_cancelable(&mut command, DART_VERSION_TIMEOUT, cancel)
        .map_err(|error| format!("Не удалось выполнить Dart --version: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Dart --version завершился с кодом {}",
            output.status
        ));
    }
    parse_dart_version_output(&output.stdout, &output.stderr)
}
fn parse_dart_version_output(stdout: &[u8], stderr: &[u8]) -> Result<String, String> {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);
    let value = stdout
        .lines()
        .chain(stderr.lines())
        .map(str::trim)
        .find(|line| line.contains("Dart SDK version:"))
        .unwrap_or_default();
    if value.is_empty() {
        Err("Dart --version вернул неизвестный формат".to_string())
    } else {
        Ok(value.to_string())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ToolInstallPhase {
    #[default]
    Idle,
    DownloadingUv,
    InstallingUv,
    InstallingTool,
    Validating,
    /// PDF engine archive: download, hash check, extraction of the library.
    Downloading,
    Verifying,
    Extracting,
    Succeeded,
    Failed,
    Cancelled,
}

impl ToolInstallPhase {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Idle => "Ожидание",
            Self::DownloadingUv => "Загрузка установщика uv",
            Self::InstallingUv => "Установка uv",
            Self::InstallingTool => "Установка инструмента",
            Self::Validating => "Проверка установки",
            Self::Downloading => "Загрузка",
            Self::Verifying => "Проверка архива",
            Self::Extracting => "Распаковка",
            Self::Succeeded => "Готово",
            Self::Failed => "Ошибка",
            Self::Cancelled => "Отменено",
        }
    }

    fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolInstallLogKind {
    Info,
    Output,
    Error,
    Success,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ToolInstallLogLine {
    pub kind: ToolInstallLogKind,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ToolInstallOutcome {
    pub paths: Vec<(ToolKind, PathBuf)>,
}

/// Terminal result of one install run, as seen by `ToolInstaller::poll_finish`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ToolInstallFinish {
    Installed(ToolInstallOutcome),
    Failed(String),
    Cancelled,
}

#[derive(Debug)]
enum ToolInstallEvent {
    Phase(ToolInstallPhase, String),
    Line(ToolInstallLogLine),
    Done(Result<ToolInstallOutcome, String>),
    Cancelled,
}

fn terminal_install_event(
    result: Result<ToolInstallOutcome, String>,
    cancellation_requested: bool,
) -> ToolInstallEvent {
    match result {
        // Once validation and transactional promotion completed, a late click
        // on Cancel must not discard the successful paths or leave an orphaned
        // generation. Cancellation only wins while the operation is failing.
        Ok(outcome) => ToolInstallEvent::Done(Ok(outcome)),
        Err(error) if cancellation_requested && error == INSTALL_CANCELLED_MESSAGE => {
            ToolInstallEvent::Cancelled
        }
        Err(error) => ToolInstallEvent::Done(Err(error)),
    }
}

/// Every successful send wakes the UI (`WakeSyncSender`). A full channel therefore always
/// has a wake outstanding, so a blocking control message cannot wait on an idle loop.
#[derive(Clone)]
struct ToolInstallReporter {
    tx: WakeSyncSender<ToolInstallEvent>,
    dropped_lines: Arc<AtomicUsize>,
}

impl ToolInstallReporter {
    fn send_control(&self, event: ToolInstallEvent) {
        self.flush_dropped_lines(true);
        let _ = self.tx.send(event);
    }

    fn send_line_event(&self, line: ToolInstallLogLine) {
        self.flush_dropped_lines(false);
        match self.tx.try_send(ToolInstallEvent::Line(line)) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                self.dropped_lines.fetch_add(1, Ordering::Relaxed);
            }
            Err(TrySendError::Disconnected(_)) => {}
        }
    }

    fn flush_dropped_lines(&self, blocking: bool) {
        let count = self.dropped_lines.swap(0, Ordering::AcqRel);
        if count == 0 {
            return;
        }
        let event = ToolInstallEvent::Line(ToolInstallLogLine {
            kind: ToolInstallLogKind::Info,
            text: format!("… пропущено строк вывода: {count}"),
        });
        if blocking {
            if self.tx.send(event).is_err() {
                self.dropped_lines.fetch_add(count, Ordering::Relaxed);
            }
        } else if let Err(error) = self.tx.try_send(event)
            && !matches!(error, TrySendError::Disconnected(_))
        {
            self.dropped_lines.fetch_add(count, Ordering::Relaxed);
        }
    }

    fn phase(&self, phase: ToolInstallPhase, detail: impl Into<String>) {
        let detail = detail.into();
        self.send_control(ToolInstallEvent::Phase(phase, detail.clone()));
        self.line(ToolInstallLogKind::Info, detail);
    }

    fn line(&self, kind: ToolInstallLogKind, text: impl Into<String>) {
        self.send_line_event(ToolInstallLogLine {
            kind,
            text: text.into(),
        });
    }
}

pub(crate) struct ToolInstaller {
    target: Option<ToolKind>,
    phase: ToolInstallPhase,
    detail: String,
    logs: Vec<ToolInstallLogLine>,
    log_bytes: usize,
    log_truncated: bool,
    revision: u64,
    log_open: bool,
    log_scroll: ScrollState,
    follow_log: bool,
    rx: Option<Receiver<ToolInstallEvent>>,
    cancel: Option<Arc<AtomicBool>>,
    worker: Option<JoinHandle<()>>,
}

impl Default for ToolInstaller {
    fn default() -> Self {
        Self {
            target: None,
            phase: ToolInstallPhase::Idle,
            detail: String::new(),
            logs: Vec::new(),
            log_bytes: 0,
            log_truncated: false,
            revision: 0,
            log_open: false,
            log_scroll: ScrollState::new(7.0),
            follow_log: true,
            rx: None,
            cancel: None,
            worker: None,
        }
    }
}

impl ToolInstaller {
    pub(crate) fn target(&self) -> Option<ToolKind> {
        self.target
    }

    pub(crate) fn phase(&self) -> ToolInstallPhase {
        self.phase
    }

    pub(crate) fn detail(&self) -> &str {
        &self.detail
    }

    pub(crate) fn logs(&self) -> &[ToolInstallLogLine] {
        &self.logs
    }

    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn is_running(&self) -> bool {
        self.rx.is_some() && !self.phase.is_terminal()
    }

    pub(crate) fn is_running_for(&self, kind: ToolKind) -> bool {
        self.is_running() && self.target == Some(kind)
    }

    pub(crate) fn is_log_open(&self) -> bool {
        self.log_open
    }

    pub(crate) fn log_scroll_y(&self) -> f32 {
        self.log_scroll.current
    }

    pub(crate) fn log_scroll_is_dragging(&self) -> bool {
        self.log_scroll.is_dragging
    }

    pub(crate) fn begin_log_scroll_drag(
        &mut self,
        pointer: f32,
        track_start: f32,
        track_len: f32,
        viewport_len: f32,
        content_len: f32,
        min_thumb_len: f32,
    ) -> bool {
        let Some(thumb) = crate::scroll::scrollbar_thumb(
            track_start,
            track_len,
            viewport_len,
            content_len,
            self.log_scroll.current,
            min_thumb_len,
        ) else {
            return false;
        };
        let max_scroll = (content_len - viewport_len).max(0.0);
        let Some((drag_offset, target)) = crate::scroll::scrollbar_drag_target(
            pointer,
            track_start,
            track_len,
            thumb,
            max_scroll,
            None,
        ) else {
            return false;
        };
        if !crate::app::mouse::apply_scrollbar_drag_target(
            &mut self.log_scroll,
            target,
            drag_offset,
        ) {
            return false;
        }
        self.follow_log = false;
        true
    }

    pub(crate) fn drag_log_scroll(
        &mut self,
        pointer: f32,
        track_start: f32,
        track_len: f32,
        viewport_len: f32,
        content_len: f32,
        min_thumb_len: f32,
    ) -> bool {
        if !self.log_scroll.is_dragging {
            return false;
        }
        let Some(thumb) = crate::scroll::scrollbar_thumb(
            track_start,
            track_len,
            viewport_len,
            content_len,
            self.log_scroll.current,
            min_thumb_len,
        ) else {
            self.log_scroll.end_drag();
            return false;
        };
        let max_scroll = (content_len - viewport_len).max(0.0);
        let Some((_, target)) = crate::scroll::scrollbar_drag_target(
            pointer,
            track_start,
            track_len,
            thumb,
            max_scroll,
            Some(self.log_scroll.drag_offset),
        ) else {
            return false;
        };
        let drag_offset = self.log_scroll.drag_offset;
        if !crate::app::mouse::apply_scrollbar_drag_target(
            &mut self.log_scroll,
            target,
            drag_offset,
        ) {
            return false;
        }
        self.follow_log = false;
        true
    }

    pub(crate) fn end_log_scroll_drag(&mut self) {
        self.log_scroll.end_drag();
    }

    pub(crate) fn stop_log_scroll_anim(&mut self) {
        if !self.log_scroll.is_dragging {
            self.log_scroll.stop_anim();
        }
    }

    pub(crate) fn open_log(&mut self) {
        self.log_open = true;
        self.follow_log = true;
        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn close_log(&mut self) {
        self.log_open = false;
        self.log_scroll.end_drag();
        self.log_scroll.stop_anim();
        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn scroll_log_by(&mut self, delta: f32, max_scroll: f32) {
        self.log_scroll.anim_speed = 7.0;
        self.log_scroll.scroll_by(delta);
        self.log_scroll.clamp_target(0.0, max_scroll);
        self.follow_log = self.log_scroll.target >= (max_scroll - 1.0).max(0.0);
    }

    pub(crate) fn update_log_scroll(&mut self, dt: f32, max_scroll: f32) -> bool {
        let before_current = self.log_scroll.current;
        let before_target = self.log_scroll.target;
        if self.follow_log {
            self.log_scroll.set_target(max_scroll);
        } else {
            self.log_scroll.clamp_target(0.0, max_scroll);
        }
        self.log_scroll.clamp_current(0.0, max_scroll);
        let animated = self.log_scroll.update(dt);
        animated
            || self.log_scroll.current != before_current
            || self.log_scroll.target != before_target
    }

    pub(crate) fn start(
        &mut self,
        kind: ToolKind,
        ui_waker: &UiWaker,
    ) -> Result<(), String> {
        if !kind.supports_managed_install() {
            return Err(format!("{} нельзя установить из RRiter", kind.label()));
        }
        if self.is_running() {
            return Err("Другая установка уже выполняется".to_string());
        }
        self.join_finished_worker();
        let initial_detail = format!("Подготовка установки {}", kind.label());
        let existing_uv = resolve_tool_kind(ToolKind::Uv).path;
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_for_worker = Arc::clone(&cancel);
        let (tx, rx) = ui_waker.sync_channel(INSTALL_EVENT_CAPACITY);
        let reporter = ToolInstallReporter {
            tx,
            dropped_lines: Arc::new(AtomicUsize::new(0)),
        };
        let worker = crate::platform::spawn_named("rriter-tool-installer", move || {
            let result = install_tool(kind, existing_uv, &cancel_for_worker, &reporter);
            reporter.send_control(terminal_install_event(
                result,
                cancel_for_worker.load(Ordering::Acquire),
            ));
        })
        .map_err(|err| format!("Не удалось запустить установщик {}: {err}", kind.label()))?;

        self.begin_run(kind, initial_detail, true, cancel, rx, worker);
        Ok(())
    }

    /// Downloads the pinned PDF engine archive into a fresh generation directory and
    /// unpacks the library into the versioned managed directory. Never retries on its own.
    /// The settings log window stays closed: the PDF tab shows the progress line itself.
    pub(crate) fn start_pdfium_install(&mut self, ui_waker: &UiWaker) -> Result<(), String> {
        if self.is_running() {
            return Err("Другая установка уже выполняется".to_string());
        }
        self.join_finished_worker();
        let manifest = crate::pdf::library::manifest()?;
        let entry = manifest
            .platform
            .ok_or_else(|| "платформа не поддерживается".to_string())?;
        let plan = PdfiumInstallPlan {
            url: crate::pdf::library::archive_url(&manifest.version, &entry),
            slug: crate::pdf::library::version_slug(&manifest.version),
            archive: entry.archive,
            sha256: entry.sha256,
            lib_member: entry.lib,
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_for_worker = Arc::clone(&cancel);
        let (tx, rx) = ui_waker.sync_channel(INSTALL_EVENT_CAPACITY);
        let reporter = ToolInstallReporter {
            tx,
            dropped_lines: Arc::new(AtomicUsize::new(0)),
        };
        let worker = crate::platform::spawn_named("rriter-pdfium-installer", move || {
            let result = install_pdfium(&plan, &cancel_for_worker, &reporter);
            reporter.send_control(terminal_install_event(
                result,
                cancel_for_worker.load(Ordering::Acquire),
            ));
        })
        .map_err(|err| format!("Не удалось запустить установку PDF-движка: {err}"))?;
        self.begin_run(
            ToolKind::Pdfium,
            "Подготовка загрузки PDF-движка".to_string(),
            false,
            cancel,
            rx,
            worker,
        );
        Ok(())
    }

    fn begin_run(
        &mut self,
        kind: ToolKind,
        initial_detail: String,
        log_open: bool,
        cancel: Arc<AtomicBool>,
        rx: Receiver<ToolInstallEvent>,
        worker: JoinHandle<()>,
    ) {
        self.target = Some(kind);
        self.phase = ToolInstallPhase::Idle;
        self.detail = initial_detail;
        self.logs.clear();
        self.log_bytes = 0;
        self.log_truncated = false;
        self.log_scroll = ScrollState::new(7.0);
        self.follow_log = true;
        self.log_open = log_open;
        self.revision = self.revision.wrapping_add(1);
        self.push_log(ToolInstallLogKind::Info, self.detail.clone());
        self.cancel = Some(cancel);
        self.rx = Some(rx);
        self.worker = Some(worker);
    }

    pub(crate) fn report_external_error(&mut self, message: impl Into<String>) {
        let message = message.into();
        if !self.is_running() {
            self.target = None;
            self.phase = ToolInstallPhase::Failed;
            self.detail = message.clone();
        }
        self.log_open = true;
        self.follow_log = true;
        self.push_log(ToolInstallLogKind::Error, message);
        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn cancel(&self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Release);
        }
    }

    pub(crate) fn poll(&mut self) -> Option<ToolInstallOutcome> {
        match self.poll_finish() {
            Some(ToolInstallFinish::Installed(outcome)) => Some(outcome),
            Some(ToolInstallFinish::Failed(_) | ToolInstallFinish::Cancelled) | None => None,
        }
    }

    /// Drains worker events; a terminal event is returned once, including failure and
    /// cancellation (which `poll` folds into `None`).
    pub(crate) fn poll_finish(&mut self) -> Option<ToolInstallFinish> {
        let Some(rx) = self.rx.take() else {
            self.join_finished_worker();
            return None;
        };
        let mut keep_receiver = true;
        let mut outcome = None;
        loop {
            match rx.try_recv() {
                Ok(event) => match event {
                    ToolInstallEvent::Phase(phase, detail) => {
                        self.phase = phase;
                        self.detail = detail;
                        self.revision = self.revision.wrapping_add(1);
                    }
                    ToolInstallEvent::Line(line) => self.push_line(line),
                    ToolInstallEvent::Done(result) => {
                        keep_receiver = false;
                        match result {
                            Ok(done) => {
                                self.phase = ToolInstallPhase::Succeeded;
                                self.detail = format!(
                                    "{} установлен и проверен",
                                    self.target.map(ToolKind::label).unwrap_or("Инструмент")
                                );
                                self.push_log(ToolInstallLogKind::Success, self.detail.clone());
                                outcome = Some(ToolInstallFinish::Installed(done));
                            }
                            Err(error) => {
                                self.phase = ToolInstallPhase::Failed;
                                self.detail = error.clone();
                                self.push_log(ToolInstallLogKind::Error, error.clone());
                                outcome = Some(ToolInstallFinish::Failed(error));
                            }
                        }
                    }
                    ToolInstallEvent::Cancelled => {
                        keep_receiver = false;
                        self.phase = ToolInstallPhase::Cancelled;
                        self.detail = "Установка отменена".to_string();
                        self.push_log(ToolInstallLogKind::Info, self.detail.clone());
                        outcome = Some(ToolInstallFinish::Cancelled);
                    }
                },
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    keep_receiver = false;
                    if !self.phase.is_terminal() {
                        self.phase = ToolInstallPhase::Failed;
                        self.detail = "Поток установки неожиданно завершился".to_string();
                        self.push_log(ToolInstallLogKind::Error, self.detail.clone());
                        outcome = Some(ToolInstallFinish::Failed(self.detail.clone()));
                    }
                    break;
                }
            }
        }
        if keep_receiver {
            self.rx = Some(rx);
        } else {
            self.cancel = None;
            self.join_terminal_worker();
        }
        outcome
    }

    pub(crate) fn full_log(&self) -> String {
        let mut text = String::new();
        if let Some(target) = self.target {
            text.push_str(target.label());
            text.push_str(": ");
            text.push_str(self.phase.label());
            if !self.detail.is_empty() {
                text.push_str(" — ");
                text.push_str(&self.detail);
            }
            text.push('\n');
        }
        for line in &self.logs {
            let prefix = match line.kind {
                ToolInstallLogKind::Info => "[info] ",
                ToolInstallLogKind::Output => "[out] ",
                ToolInstallLogKind::Error => "[error] ",
                ToolInstallLogKind::Success => "[ok] ",
            };
            text.push_str(prefix);
            text.push_str(&line.text);
            text.push('\n');
        }
        text
    }

    pub(crate) fn note(&mut self, text: impl Into<String>) {
        self.push_log(ToolInstallLogKind::Info, text.into());
    }

    #[cfg(test)]
    pub(crate) fn seed_running_worker_for_test(
        &mut self,
        duration: std::time::Duration,
    ) -> Arc<AtomicBool> {
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Some(cancel.clone());
        self.worker = Some(std::thread::spawn(move || std::thread::sleep(duration)));
        cancel
    }

    #[cfg(test)]
    pub(crate) fn has_worker_for_test(&self) -> bool {
        self.worker.is_some()
    }

    pub(crate) fn shutdown(&mut self) {
        self.cancel();
        self.rx = None;
        self.cancel = None;
        if let Some(worker) = self.worker.take() {
            if worker.is_finished() {
                let _ = worker.join();
            } else {
                crate::platform::reap_unit_thread(worker);
            }
        }
    }

    fn push_log(&mut self, kind: ToolInstallLogKind, text: String) {
        self.push_line(ToolInstallLogLine { kind, text });
    }

    fn push_line(&mut self, mut line: ToolInstallLogLine) {
        if line.text.trim().is_empty() {
            return;
        }
        if line.text.len() > INSTALL_LINE_BYTES_LIMIT {
            let mut boundary = INSTALL_LINE_BYTES_LIMIT;
            while boundary > 0 && !line.text.is_char_boundary(boundary) {
                boundary -= 1;
            }
            line.text.truncate(boundary);
            line.text.push_str(" … [сообщение обрезано]");
        }
        self.log_bytes = self.log_bytes.saturating_add(line.text.len());
        self.logs.push(line);

        let marker_present = self.log_truncated
            && self
                .logs
                .first()
                .is_some_and(|line| line.text == TRUNCATED_LOG_MARKER);
        let first_removable = usize::from(marker_present);
        let allowed_lines = INSTALL_LOG_LIMIT + first_removable;
        let mut remove_count = 0usize;
        let mut removed_bytes = 0usize;
        while self.logs.len().saturating_sub(remove_count) > allowed_lines
            || self.log_bytes.saturating_sub(removed_bytes) > INSTALL_LOG_BYTES_LIMIT
        {
            let index = first_removable + remove_count;
            removed_bytes = removed_bytes.saturating_add(self.logs[index].text.len());
            remove_count += 1;
        }
        if remove_count > 0 {
            self.logs
                .drain(first_removable..first_removable + remove_count);
            self.log_bytes = self.log_bytes.saturating_sub(removed_bytes);
            if !marker_present {
                self.log_truncated = true;
                let marker = ToolInstallLogLine {
                    kind: ToolInstallLogKind::Info,
                    text: TRUNCATED_LOG_MARKER.to_string(),
                };
                self.log_bytes = self.log_bytes.saturating_add(marker.text.len());
                self.logs.insert(0, marker);
            }
        }
        self.revision = self.revision.wrapping_add(1);
    }

    fn join_finished_worker(&mut self) {
        if self.worker.as_ref().is_some_and(JoinHandle::is_finished)
            && let Some(worker) = self.worker.take()
        {
            let _ = worker.join();
        }
    }

    fn join_terminal_worker(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for ToolInstaller {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub(crate) fn log_modal_height(window_height: f32, scale: f32) -> f32 {
    (520.0 * scale)
        .min((window_height - 24.0 * scale).max(0.0))
        .round()
}

pub(crate) fn log_viewport_height(window_height: f32, scale: f32) -> f32 {
    (log_modal_height(window_height, scale) - (145.0 * scale).round())
        .max(0.0)
        .round()
}

pub(crate) fn log_max_scroll(line_count: usize, window_height: f32, scale: f32) -> f32 {
    let content_height = line_count.max(1) as f32 * log_line_height(scale) + (12.0 * scale).round();
    (content_height - log_viewport_height(window_height, scale))
        .max(0.0)
        .round()
}

pub(crate) fn log_line_height(scale: f32) -> f32 {
    (LOG_LINE_HEIGHT * scale).round().max(1.0)
}

include!("tool_installer_download.rs");
impl crate::app::App {
    pub(crate) fn refresh_dart_tool_state(&mut self) {
        let workspace = self
            .file_path
            .as_deref()
            .and_then(|path| {
                self.ide_workspaces
                    .iter()
                    .find(|workspace| crate::platform::path_is_within(path, workspace))
            })
            .or_else(|| self.ide_workspaces.first())
            .map(PathBuf::as_path);
        self.dart_tool_state.refresh(workspace, &self.ui_waker);
    }

    pub(crate) fn poll_dart_tool_state(&mut self) -> bool {
        self.dart_tool_state.poll()
    }

    pub(crate) fn restart_dart_server(&mut self) {
        self.clear_all_closing_hints();
        self.refresh_dart_closing_hints();
        if let Some(lsp) = &mut self.lsp {
            lsp.restart_server("dart");
            self.ide_panel.lsp_servers = lsp.servers_info();
        }
        self.refresh_dart_tool_state();
    }

    pub(crate) fn trigger_tool_install(&mut self, kind: ToolKind) {
        if self.tool_installer.is_running_for(kind) {
            self.tool_installer.cancel();
        } else {
            match self.tool_installer.start(kind, &self.ui_waker) {
                Ok(()) => {
                    // A picker opened immediately before installation may still
                    // complete on another thread. Its stale result must not
                    // replace the transactional managed path.
                    self.settings_tool_picker_rx = None;
                }
                Err(error) => {
                    eprintln!("Failed to start {} installation: {error}", kind.label());
                }
            }
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub(crate) fn tool_install_log_max_scroll(&self) -> f32 {
        let scale = self
            .renderer
            .as_ref()
            .map_or(1.0, |renderer| renderer.scale_factor);
        let height = self
            .window
            .as_ref()
            .map_or(700.0 * scale, |window| window.inner_size().height as f32);
        log_max_scroll(self.tool_installer.logs().len(), height, scale)
    }

    pub(crate) fn poll_tool_installer(&mut self) -> bool {
        let before_revision = self.tool_installer.revision();
        let was_running = self.tool_installer.is_running();
        let finish = self.tool_installer.poll_finish();
        let mut changed = was_running != self.tool_installer.is_running()
            || before_revision != self.tool_installer.revision();
        // The PDF engine is not a configured tool: no `tool_paths`, no config save.
        // `locate()` finds the library under the managed directory by itself.
        let pdfium = self.tool_installer.target() == Some(ToolKind::Pdfium);
        if pdfium {
            changed |= self.sync_pdf_engine_install(finish);
        } else if let Some(ToolInstallFinish::Installed(outcome)) = finish {
            let mut restart_lsp = false;
            let mut persist_api_runtime = false;
            for (kind, path) in outcome.paths {
                if std::env::var_os(kind.override_env()).is_some() {
                    self.tool_installer.note(format!(
                        "{} установлен, но {} имеет приоритет над настройкой RRiter",
                        kind.label(),
                        kind.override_env()
                    ));
                }
                if kind == ToolKind::Uv {
                    let runtime = &mut self.ide_panel.api.mock.uv;
                    runtime.configured_path = Some(path.clone());
                    runtime.detected_path = Some(path.clone());
                    runtime.status = crate::app::api_mock::types::ApiUvStatus::Ready;
                    runtime.last_error.clear();
                    persist_api_runtime = true;
                }
                self.tool_paths.set(kind, Some(path));
                restart_lsp |= matches!(kind, ToolKind::Ruff | ToolKind::Ty);
            }
            crate::platform::configure_tool_paths(self.tool_paths.clone());
            self.save_current_config();
            if restart_lsp && let Some(lsp) = &mut self.lsp {
                lsp.restart_python();
                self.ide_panel.lsp_servers = lsp.servers_info();
            }
            if persist_api_runtime {
                self.ide_panel.api.commit_mock_config();
            }
            changed = true;
        }
        changed
    }
}

#[cfg(test)]
#[path = "tool_installer_tests.rs"]
mod tests;
