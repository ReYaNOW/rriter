const GIT_RUNTIME_EVENT_CAPACITY: usize = 256;
const GIT_LOG_TEXT_BUDGET_BYTES: usize = 2 * 1024 * 1024;
const GIT_DIAGNOSTIC_LIMIT_BYTES: usize = 8 * 1024;
pub(crate) const GIT_LOG_TRUNCATION_MARKER: &str = "older Git output truncated";
pub(crate) const GIT_LOG_TOOLBAR_H: f32 = 30.0;
pub(crate) const GIT_LOG_ROW_H: f32 = 20.0;

pub(crate) fn git_logs_max_scroll(line_count: usize, view_h: f32, scale: f32) -> f32 {
    let rows_h = (view_h - GIT_LOG_TOOLBAR_H * scale).max(0.0);
    let total_h = line_count as f32 * GIT_LOG_ROW_H * scale;
    (total_h - rows_h).max(0.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GitRuntimeStage {
    Commit,
    Push,
}

impl GitRuntimeStage {
    fn label(self) -> &'static str {
        match self {
            Self::Commit => "Commit",
            Self::Push => "Push",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct GitRuntimeContext {
    stage: GitRuntimeStage,
    repo_index: usize,
    repo_total: usize,
    repo_name: Arc<str>,
}

impl GitRuntimeContext {
    fn new(stage: GitRuntimeStage, repo_index: usize, repo_total: usize, repo_root: &Path) -> Self {
        let repo_name = repo_root
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .map(Arc::<str>::from)
            .unwrap_or_else(|| Arc::<str>::from(repo_root.to_string_lossy().into_owned()));
        Self {
            stage,
            repo_index,
            repo_total,
            repo_name,
        }
    }

    fn progress_label(&self, hook_name: Option<&str>) -> String {
        let mut label = format!(
            "{} {}/{} · {}",
            self.stage.label(),
            self.repo_index,
            self.repo_total,
            self.repo_name
        );
        if let Some(hook_name) = hook_name {
            label.push_str(" · ");
            label.push_str(hook_name);
        }
        label
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GitLogKind {
    Header,
    Stdout,
    Stderr,
    Hook,
    Success,
    Failure,
    Info,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GitLogSpan {
    pub text: String,
    pub ansi_fg: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GitLogLine {
    pub kind: GitLogKind,
    pub spans: Vec<GitLogSpan>,
}

impl GitLogLine {
    fn plain(kind: GitLogKind, text: impl Into<String>) -> Self {
        Self {
            kind,
            spans: vec![GitLogSpan {
                text: text.into(),
                ansi_fg: None,
            }],
        }
    }

    fn text_bytes(&self) -> usize {
        self.spans.iter().map(|span| span.text.len()).sum()
    }

    #[cfg(test)]
    fn plain_text(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct GitLogBuffer {
    lines: VecDeque<GitLogLine>,
    text_bytes: usize,
    truncated: bool,
}

impl Default for GitLogBuffer {
    fn default() -> Self {
        Self {
            lines: VecDeque::new(),
            text_bytes: 0,
            truncated: false,
        }
    }
}

impl GitLogBuffer {
    pub(crate) fn clear(&mut self) {
        self.lines.clear();
        self.text_bytes = 0;
        self.truncated = false;
    }

    pub(crate) fn append(&mut self, line: GitLogLine) {
        let bytes = line.text_bytes();
        self.lines.push_back(line);
        self.text_bytes = self.text_bytes.saturating_add(bytes);
        while self.text_bytes > GIT_LOG_TEXT_BUDGET_BYTES {
            let Some(oldest) = self.lines.pop_front() else {
                self.text_bytes = 0;
                break;
            };
            self.text_bytes = self.text_bytes.saturating_sub(oldest.text_bytes());
            self.truncated = true;
        }
    }

    pub(crate) fn line_count(&self) -> usize {
        self.lines.len() + usize::from(self.truncated)
    }

    pub(crate) fn line_at(&self, index: usize) -> Option<GitLogLineRef<'_>> {
        if self.truncated {
            if index == 0 {
                return Some(GitLogLineRef::TruncationMarker);
            }
            self.lines
                .get(index.saturating_sub(1))
                .map(GitLogLineRef::Line)
        } else {
            self.lines.get(index).map(GitLogLineRef::Line)
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.lines.is_empty() && !self.truncated
    }

    #[cfg(test)]
    fn text_bytes(&self) -> usize {
        self.text_bytes
    }
}

pub(crate) enum GitLogLineRef<'a> {
    TruncationMarker,
    Line(&'a GitLogLine),
}

impl<'a> GitLogLineRef<'a> {
    pub(crate) fn kind(&self) -> GitLogKind {
        match self {
            Self::TruncationMarker => GitLogKind::Info,
            Self::Line(line) => line.kind,
        }
    }

    pub(crate) fn spans(&self) -> GitLogSpansRef<'a> {
        match self {
            Self::TruncationMarker => GitLogSpansRef::TruncationMarker,
            Self::Line(line) => GitLogSpansRef::Line(&line.spans),
        }
    }
}

pub(crate) enum GitLogSpansRef<'a> {
    TruncationMarker,
    Line(&'a [GitLogSpan]),
}

#[derive(Clone, Debug)]
pub(crate) enum GitRuntimeEvent {
    Stage(GitRuntimeContext),
    RefreshingStatus,
    CommandStarted {
        context: GitRuntimeContext,
        unix_secs: u64,
    },
    Output {
        stream: crate::platform::ProcessOutputStream,
        spans: Vec<GitLogSpan>,
    },
    HookStarted {
        context: GitRuntimeContext,
        hook_name: String,
        session_id: String,
        child_id: u64,
    },
    HookFinished {
        context: GitRuntimeContext,
        hook_name: String,
        session_id: String,
        child_id: u64,
        code: i64,
        duration_secs: Option<f64>,
    },
    CommandFinished {
        context: GitRuntimeContext,
        code: Option<i32>,
        success: bool,
        duration_secs: f32,
        detail: Option<String>,
    },
    Info(String),
}

impl GitPanelState {
    pub(crate) fn apply_runtime_event(&mut self, event: GitRuntimeEvent) {
        match event {
            GitRuntimeEvent::Stage(context) => {
                self.active_git_hooks.clear();
                self.pending_label = Some(context.progress_label(None));
            }
            GitRuntimeEvent::RefreshingStatus => {
                self.active_git_hooks.clear();
                self.pending_label = Some("Обновление Git status".to_string());
            }
            GitRuntimeEvent::CommandStarted { context, unix_secs } => {
                self.active_git_hooks.clear();
                self.pending_label = Some(context.progress_label(None));
                self.git_logs.append(GitLogLine::plain(
                    GitLogKind::Header,
                    format!(
                        "{}  [{}]  git {}",
                        git_runtime_clock(unix_secs),
                        context.repo_name,
                        context.stage.label().to_ascii_lowercase()
                    ),
                ));
            }
            GitRuntimeEvent::Output { stream, spans } => {
                let kind = match stream {
                    crate::platform::ProcessOutputStream::Stdout => GitLogKind::Stdout,
                    crate::platform::ProcessOutputStream::Stderr => GitLogKind::Stderr,
                };
                self.git_logs.append(GitLogLine { kind, spans });
            }
            GitRuntimeEvent::HookStarted {
                context,
                hook_name,
                session_id,
                child_id,
            } => {
                self.active_git_hooks
                    .retain(|(sid, id, _)| sid != &session_id || *id != child_id);
                self.active_git_hooks
                    .push((session_id, child_id, hook_name.clone()));
                self.pending_label = Some(context.progress_label(Some(&hook_name)));
                self.git_logs.append(GitLogLine::plain(
                    GitLogKind::Hook,
                    format!("[hook] {hook_name} started (child {child_id})"),
                ));
            }
            GitRuntimeEvent::HookFinished {
                context,
                hook_name,
                session_id,
                child_id,
                code,
                duration_secs,
            } => {
                self.active_git_hooks
                    .retain(|(sid, id, _)| sid != &session_id || *id != child_id);
                self.pending_label = Some(context.progress_label(
                    self.active_git_hooks.last().map(|(_, _, name)| name.as_str()),
                ));
                let duration = duration_secs
                    .map(|duration| format!(", {duration:.1}s"))
                    .unwrap_or_default();
                let (kind, status) = if code == 0 {
                    (GitLogKind::Hook, "completed")
                } else {
                    (GitLogKind::Failure, "failed")
                };
                self.git_logs.append(GitLogLine::plain(
                    kind,
                    format!("[hook] {hook_name} {status} (exit {code}{duration}, child {child_id})"),
                ));
                if code != 0 {
                    self.open_logs_for_failure();
                }
            }
            GitRuntimeEvent::CommandFinished {
                context,
                code,
                success,
                duration_secs,
                detail,
            } => {
                self.active_git_hooks.clear();
                self.pending_label = Some(context.progress_label(None));
                let kind = if success {
                    GitLogKind::Success
                } else {
                    GitLogKind::Failure
                };
                let code = code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "?".to_string());
                let status = if success { "completed" } else { "failed" };
                let mut line = format!(
                    "[exit {code}] {} {status} in {duration_secs:.1}s",
                    context.stage.label().to_ascii_lowercase()
                );
                if let Some(detail) = detail.filter(|detail| !detail.is_empty()) {
                    line.push_str(": ");
                    line.push_str(&detail);
                }
                self.git_logs.append(GitLogLine::plain(kind, line));
                if !success {
                    self.open_logs_for_failure();
                }
            }
            GitRuntimeEvent::Info(message) => {
                self.git_logs.append(GitLogLine::plain(GitLogKind::Info, message));
            }
        }
    }
}

fn git_runtime_clock(unix_secs: u64) -> String {
    let day_secs = unix_secs % 86_400;
    let hours = day_secs / 3_600;
    let minutes = (day_secs % 3_600) / 60;
    let seconds = day_secs % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

fn git_runtime_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

struct GitRuntimeEmitter<'a> {
    tx: Option<&'a mpsc::SyncSender<GitRuntimeEvent>>,
    output_dropped: bool,
}

impl<'a> GitRuntimeEmitter<'a> {
    fn new(tx: Option<&'a mpsc::SyncSender<GitRuntimeEvent>>) -> Self {
        Self {
            tx,
            output_dropped: false,
        }
    }

    fn reliable(&mut self, event: GitRuntimeEvent) {
        let Some(tx) = self.tx else {
            return;
        };
        if self.output_dropped {
            let _ = tx.send(GitRuntimeEvent::Info(
                "Git output dropped because the UI event queue was full".to_string(),
            ));
            self.output_dropped = false;
        }
        let _ = tx.send(event);
    }

    fn output(&mut self, event: GitRuntimeEvent) {
        let Some(tx) = self.tx else {
            return;
        };
        match tx.try_send(event) {
            Ok(()) => {}
            Err(mpsc::TrySendError::Full(_)) => self.output_dropped = true,
            Err(mpsc::TrySendError::Disconnected(_)) => {}
        }
    }

    fn finish(&mut self) {
        if !self.output_dropped {
            return;
        }
        let Some(tx) = self.tx else {
            return;
        };
        let _ = tx.send(GitRuntimeEvent::Info(
            "Git output dropped because the UI event queue was full".to_string(),
        ));
        self.output_dropped = false;
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Trace2HookEvent {
    Started {
        hook_name: String,
        session_id: String,
        child_id: u64,
    },
    Finished {
        hook_name: String,
        session_id: String,
        child_id: u64,
        code: i64,
        duration_secs: Option<f64>,
    },
}

#[derive(Clone, Debug, PartialEq)]
enum Trace2Disposition {
    UserOutput,
    Metadata(Option<Trace2HookEvent>),
}

#[derive(Default)]
struct Trace2HookTracker {
    active: FxHashMap<(String, u64), String>,
}

impl Trace2HookTracker {
    fn parse_line(&mut self, line: &str) -> Trace2Disposition {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            return Trace2Disposition::UserOutput;
        };
        let Some(object) = value.as_object() else {
            return Trace2Disposition::UserOutput;
        };
        let envelope = object.get("event").and_then(serde_json::Value::as_str).is_some()
            && object.get("sid").and_then(serde_json::Value::as_str).is_some()
            && object.get("thread").and_then(serde_json::Value::as_str).is_some()
            && object.get("time").and_then(serde_json::Value::as_str).is_some()
            && object.get("file").and_then(serde_json::Value::as_str).is_some()
            && object.get("line").and_then(serde_json::Value::as_u64).is_some();
        if !envelope {
            return Trace2Disposition::UserOutput;
        }

        let event = object
            .get("event")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let sid = object
            .get("sid")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let child_id = object
            .get("child_id")
            .and_then(serde_json::Value::as_u64);

        if event == "child_start"
            && object
                .get("child_class")
                .and_then(serde_json::Value::as_str)
                == Some("hook")
            && let (Some(child_id), Some(hook_name)) = (
                child_id,
                object.get("hook_name").and_then(serde_json::Value::as_str),
            )
        {
            self.active
                .insert((sid.to_string(), child_id), hook_name.to_string());
            return Trace2Disposition::Metadata(Some(Trace2HookEvent::Started {
                hook_name: hook_name.to_string(),
                session_id: sid.to_string(),
                child_id,
            }));
        }

        if event == "child_exit"
            && let Some(child_id) = child_id
            && let Some(hook_name) = self.active.remove(&(sid.to_string(), child_id))
        {
            let code = object
                .get("code")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(-1);
            let duration_secs = object.get("t_rel").and_then(serde_json::Value::as_f64);
            return Trace2Disposition::Metadata(Some(Trace2HookEvent::Finished {
                hook_name,
                session_id: sid.to_string(),
                child_id,
                code,
                duration_secs,
            }));
        }

        Trace2Disposition::Metadata(None)
    }
}

struct GitAnsiDecoder {
    parser: alacritty_terminal::vte::Parser,
    fg: Option<u8>,
    bold: bool,
}

impl Default for GitAnsiDecoder {
    fn default() -> Self {
        Self {
            parser: alacritty_terminal::vte::Parser::new(),
            fg: None,
            bold: false,
        }
    }
}

impl GitAnsiDecoder {
    fn decode_line(&mut self, line: &str) -> Vec<GitLogSpan> {
        let mut spans = Vec::new();
        let Self { parser, fg, bold } = self;
        let mut performer = GitAnsiPerformer {
            fg,
            bold,
            spans: &mut spans,
        };
        parser.advance(&mut performer, line.as_bytes());
        spans
    }
}

struct GitAnsiPerformer<'a> {
    fg: &'a mut Option<u8>,
    bold: &'a mut bool,
    spans: &'a mut Vec<GitLogSpan>,
}

impl GitAnsiPerformer<'_> {
    fn push_char(&mut self, c: char) {
        if let Some(last) = self.spans.last_mut()
            && last.ansi_fg == *self.fg
        {
            last.text.push(c);
            return;
        }
        self.spans.push(GitLogSpan {
            text: c.to_string(),
            ansi_fg: *self.fg,
        });
    }

    fn apply_sgr(&mut self, params: &alacritty_terminal::vte::Params) {
        crate::app::terminal::apply_ansi_sgr(
            params,
            self.fg,
            self.bold,
            None,
            None,
            0,
        );
    }
}

impl alacritty_terminal::vte::Perform for GitAnsiPerformer<'_> {
    fn print(&mut self, c: char) {
        self.push_char(c);
    }

    fn execute(&mut self, byte: u8) {
        if byte == b'\t' {
            for _ in 0..4 {
                self.push_char(' ');
            }
        }
    }

    fn hook(
        &mut self,
        _params: &alacritty_terminal::vte::Params,
        _intermediates: &[u8],
        _ignore: bool,
        _action: char,
    ) {
    }

    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}
    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}

    fn csi_dispatch(
        &mut self,
        params: &alacritty_terminal::vte::Params,
        _intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        if action == 'm' {
            self.apply_sgr(params);
        }
    }
}

fn sanitize_git_output_line(line: &str) -> String {
    let mut output = line.to_string();
    let mut search = 0usize;
    while search < output.len() {
        let Some(relative) = output[search..].find("://") else {
            break;
        };
        let separator = search + relative;
        let scheme_start = output.as_bytes()[..separator]
            .iter()
            .rposition(|byte| !byte.is_ascii_alphanumeric() && !matches!(*byte, b'+' | b'-' | b'.'))
            .map_or(0, |index| index + 1);
        if scheme_start == separator
            || !output.as_bytes()[scheme_start].is_ascii_alphabetic()
        {
            search = separator + 3;
            continue;
        }

        let authority_start = separator + 3;
        let authority_end = output[authority_start..]
            .find(|ch: char| {
                ch.is_whitespace()
                    || matches!(ch, '/' | '?' | '#' | '\'' | '"' | '>' | ')' | ']')
            })
            .map_or(output.len(), |offset| authority_start + offset);
        let Some(at_offset) = output[authority_start..authority_end].rfind('@') else {
            search = authority_end.max(authority_start);
            continue;
        };
        let userinfo_end = authority_start + at_offset;
        output.replace_range(authority_start..userinfo_end, "<redacted>");
        search = authority_start + "<redacted>@".len();
    }
    output
}

fn append_diagnostic(target: &mut String, spans: &[GitLogSpan]) {
    if target.len() >= GIT_DIAGNOSTIC_LIMIT_BYTES {
        return;
    }
    if !target.is_empty() {
        target.push('\n');
    }
    for span in spans {
        if target.len() >= GIT_DIAGNOSTIC_LIMIT_BYTES {
            break;
        }
        let remaining = GIT_DIAGNOSTIC_LIMIT_BYTES - target.len();
        if span.text.len() <= remaining {
            target.push_str(&span.text);
        } else {
            let end = span
                .text
                .char_indices()
                .take_while(|(idx, _)| *idx < remaining)
                .map(|(idx, ch)| idx + ch.len_utf8())
                .last()
                .unwrap_or(0)
                .min(span.text.len());
            target.push_str(&span.text[..end]);
        }
    }
}

fn run_git_logged_command(
    repo_root: &Path,
    args: &[std::ffi::OsString],
    label: &str,
    network: bool,
    context: GitRuntimeContext,
    emitter: &mut GitRuntimeEmitter<'_>,
) -> Result<(), String> {
    emitter.reliable(GitRuntimeEvent::Stage(context.clone()));
    emitter.reliable(GitRuntimeEvent::CommandStarted {
        context: context.clone(),
        unix_secs: git_runtime_unix_secs(),
    });

    let cancel = std::sync::atomic::AtomicBool::new(false);
    let started = std::time::Instant::now();
    let mut trace = Trace2HookTracker::default();
    let mut stdout_ansi = GitAnsiDecoder::default();
    let mut stderr_ansi = GitAnsiDecoder::default();
    let mut stdout_detail = String::new();
    let mut stderr_detail = String::new();

    let status = run_git_streaming_status(
        repo_root,
        args,
        label,
        network,
        GIT_COMMIT_TIMEOUT,
        true,
        &cancel,
        |stream, line| {
            if stream == crate::platform::ProcessOutputStream::Stderr {
                match trace.parse_line(&line) {
                    Trace2Disposition::Metadata(Some(Trace2HookEvent::Started {
                        hook_name,
                        session_id,
                        child_id,
                    })) => {
                        emitter.reliable(GitRuntimeEvent::HookStarted {
                            context: context.clone(),
                            hook_name,
                            session_id,
                            child_id,
                        });
                        return;
                    }
                    Trace2Disposition::Metadata(Some(Trace2HookEvent::Finished {
                        hook_name,
                        session_id,
                        child_id,
                        code,
                        duration_secs,
                    })) => {
                        emitter.reliable(GitRuntimeEvent::HookFinished {
                            context: context.clone(),
                            hook_name,
                            session_id,
                            child_id,
                            code,
                            duration_secs,
                        });
                        return;
                    }
                    Trace2Disposition::Metadata(None) => return,
                    Trace2Disposition::UserOutput => {}
                }
            }

            let line = sanitize_git_output_line(&line);
            let spans = match stream {
                crate::platform::ProcessOutputStream::Stdout => stdout_ansi.decode_line(&line),
                crate::platform::ProcessOutputStream::Stderr => stderr_ansi.decode_line(&line),
            };
            match stream {
                crate::platform::ProcessOutputStream::Stdout => {
                    append_diagnostic(&mut stdout_detail, &spans)
                }
                crate::platform::ProcessOutputStream::Stderr => {
                    append_diagnostic(&mut stderr_detail, &spans)
                }
            }
            emitter.output(GitRuntimeEvent::Output { stream, spans });
        },
    );

    let duration_secs = started.elapsed().as_secs_f32();
    let (code, success, detail, result) = match status {
        Ok(status) if status.success() => (status.code(), true, None, Ok(())),
        Ok(status) => {
            let raw = if stderr_detail.trim().is_empty() {
                stdout_detail.trim()
            } else {
                stderr_detail.trim()
            };
            let detail = classify_git_failure(label, raw);
            (
                status.code(),
                false,
                Some(short_command_output(detail.as_bytes())),
                Err(detail),
            )
        }
        Err(error) => (
            None,
            false,
            Some(short_command_output(error.as_bytes())),
            Err(error),
        ),
    };
    emitter.reliable(GitRuntimeEvent::CommandFinished {
        context,
        code,
        success,
        duration_secs,
        detail,
    });
    emitter.finish();
    result
}

fn git_runtime_preflight_failure(
    context: GitRuntimeContext,
    detail: String,
    emitter: &mut GitRuntimeEmitter<'_>,
) -> String {
    emitter.reliable(GitRuntimeEvent::Stage(context.clone()));
    emitter.reliable(GitRuntimeEvent::CommandStarted {
        context: context.clone(),
        unix_secs: git_runtime_unix_secs(),
    });
    emitter.reliable(GitRuntimeEvent::CommandFinished {
        context,
        code: None,
        success: false,
        duration_secs: 0.0,
        detail: Some(short_command_output(detail.as_bytes())),
    });
    detail
}

fn commit_repo_with_runtime(
    repo_root: &Path,
    message: &str,
    amend: bool,
    skip_hooks: bool,
    repo_index: usize,
    repo_total: usize,
    emitter: &mut GitRuntimeEmitter<'_>,
) -> Result<(), String> {
    let context = GitRuntimeContext::new(GitRuntimeStage::Commit, repo_index, repo_total, repo_root);
    let repo = git2::Repository::open(repo_root).map_err(|error| {
        git_runtime_preflight_failure(context.clone(), short_git_error(error), emitter)
    })?;
    let fallback_identity = repo.signature().is_err();
    drop(repo);
    let args = git_commit_args(message, amend, skip_hooks, fallback_identity);
    run_git_logged_command(repo_root, &args, "COMMIT", false, context, emitter)
}

fn push_repo_with_runtime(
    repo_root: &Path,
    repo_index: usize,
    repo_total: usize,
    emitter: &mut GitRuntimeEmitter<'_>,
) -> Result<(), String> {
    let context = GitRuntimeContext::new(GitRuntimeStage::Push, repo_index, repo_total, repo_root);
    let (remote_name, branch, remote_ref) = git_push_target(repo_root).map_err(|error| {
        git_runtime_preflight_failure(context.clone(), error, emitter)
    })?;
    let args = git_push_args(&remote_name, &branch, &remote_ref);
    run_git_logged_command(repo_root, &args, "PUSH", true, context, emitter)
}

fn commit_repo(repo_root: &Path, message: &str, amend: bool) -> Result<(), String> {
    let mut emitter = GitRuntimeEmitter::new(None);
    commit_repo_with_runtime(repo_root, message, amend, false, 1, 1, &mut emitter)
}

#[cfg(test)]
mod git_commit_runtime_tests {
    use super::*;

    fn trace_line(extra: &str) -> String {
        format!(
            r#"{{"event":"version","sid":"s","thread":"main","time":"2026-08-13T00:00:00Z","file":"trace2.c","line":1,{extra}}}"#
        )
    }

    fn test_hook_finished(context: &GitRuntimeContext, code: i64) -> GitRuntimeEvent {
        GitRuntimeEvent::HookFinished {
            context: context.clone(),
            hook_name: "pre-commit".to_string(),
            session_id: "s".to_string(),
            child_id: 1,
            code,
            duration_secs: Some(0.2),
        }
    }

    fn test_command_finished(context: &GitRuntimeContext, success: bool) -> GitRuntimeEvent {
        GitRuntimeEvent::CommandFinished {
            context: context.clone(),
            code: Some(if success { 0 } else { 1 }),
            success,
            duration_secs: 0.3,
            detail: None,
        }
    }

    #[test]
    fn trace2_parser_tracks_hook_start_and_exit_by_session_and_child_id() {
        let mut tracker = Trace2HookTracker::default();
        let start = r#"{"event":"child_start","sid":"s","thread":"main","time":"2026-08-13T00:00:00Z","file":"run-command.c","line":722,"child_id":4,"child_class":"hook","hook_name":"pre-commit"}"#;
        assert_eq!(
            tracker.parse_line(start),
            Trace2Disposition::Metadata(Some(Trace2HookEvent::Started {
                hook_name: "pre-commit".to_string(),
                session_id: "s".to_string(),
                child_id: 4,
            }))
        );
        let exit = r#"{"event":"child_exit","sid":"s","thread":"main","time":"2026-08-13T00:00:01Z","file":"run-command.c","line":979,"child_id":4,"code":1,"t_rel":1.25}"#;
        assert_eq!(
            tracker.parse_line(exit),
            Trace2Disposition::Metadata(Some(Trace2HookEvent::Finished {
                hook_name: "pre-commit".to_string(),
                session_id: "s".to_string(),
                child_id: 4,
                code: 1,
                duration_secs: Some(1.25),
            }))
        );
    }

    #[test]
    fn trace2_parser_keeps_user_json_and_malformed_json_visible() {
        let mut tracker = Trace2HookTracker::default();
        assert_eq!(
            tracker.parse_line(r#"{"event":"child_start","message":"hook-like user json"}"#),
            Trace2Disposition::UserOutput
        );
        assert_eq!(tracker.parse_line("{not json"), Trace2Disposition::UserOutput);
        assert_eq!(
            tracker.parse_line(&trace_line(r#""answer":42"#)),
            Trace2Disposition::Metadata(None)
        );
    }

    #[test]
    fn trace2_parser_handles_multiple_children_and_unknown_nested_events() {
        let mut tracker = Trace2HookTracker::default();
        for (sid, id, hook) in [("s", 1, "pre-commit"), ("s/child", 1, "commit-msg")] {
            let start = format!(
                r#"{{"event":"child_start","sid":"{sid}","thread":"main","time":"2026-08-13T00:00:00Z","file":"run-command.c","line":1,"child_id":{id},"child_class":"hook","hook_name":"{hook}"}}"#
            );
            assert!(matches!(
                tracker.parse_line(&start),
                Trace2Disposition::Metadata(Some(Trace2HookEvent::Started { .. }))
            ));
        }
        assert_eq!(
            tracker.parse_line(&trace_line(r#""child_id":99"#)),
            Trace2Disposition::Metadata(None)
        );
        let exit = r#"{"event":"child_exit","sid":"s/child","thread":"main","time":"2026-08-13T00:00:01Z","file":"run-command.c","line":2,"child_id":1,"code":0,"t_rel":0.2}"#;
        assert!(matches!(
            tracker.parse_line(exit),
            Trace2Disposition::Metadata(Some(Trace2HookEvent::Finished { hook_name, .. })) if hook_name == "commit-msg"
        ));
    }

    #[test]
    fn ansi_parser_preserves_basic_sgr_spans_and_hides_escape_bytes() {
        let mut parser = GitAnsiDecoder::default();
        let spans = parser.decode_line("plain \x1b[31mred\x1b[0m normal");
        assert_eq!(
            spans,
            vec![
                GitLogSpan {
                    text: "plain ".to_string(),
                    ansi_fg: None,
                },
                GitLogSpan {
                    text: "red".to_string(),
                    ansi_fg: Some(1),
                },
                GitLogSpan {
                    text: " normal".to_string(),
                    ansi_fg: None,
                },
            ]
        );
        assert!(!spans.iter().any(|span| span.text.contains('\x1b')));
        let background_only = parser.decode_line("\x1b[48;5;1mbackground\x1b[0m");
        assert_eq!(
            background_only,
            vec![GitLogSpan {
                text: "background".to_string(),
                ansi_fg: None,
            }]
        );
        let indexed = parser.decode_line("\x1b[38;5;12mindexed\x1b[0m");
        assert_eq!(indexed[0].ansi_fg, Some(12));
        let malformed = parser.decode_line("ok\x1b[31");
        assert_eq!(
            malformed.iter().map(|span| span.text.as_str()).collect::<String>(),
            "ok"
        );
    }

    #[test]
    fn git_output_redacts_authenticated_url_userinfo_before_logging() {
        let line = "fatal: unable to access 'https://alice:secret@example.test/repo': denied";
        let clean = sanitize_git_output_line(line);
        assert!(clean.contains("https://<redacted>@example.test/repo"));
        assert!(!clean.contains("alice"));
        assert!(!clean.contains("secret"));

        let token_user = sanitize_git_output_line("https://token@example.test/org/repo.git");
        assert_eq!(token_user, "https://<redacted>@example.test/org/repo.git");
        assert_eq!(
            sanitize_git_output_line("git@github.com:org/repo.git"),
            "git@github.com:org/repo.git"
        );
    }

    #[test]
    fn log_buffer_is_bounded_fifo_with_single_virtual_truncation_marker() {
        let mut logs = GitLogBuffer::default();
        let chunk = "x".repeat(16 * 1024);
        for idx in 0..140 {
            logs.append(GitLogLine::plain(
                GitLogKind::Stdout,
                format!("{idx}:{chunk}"),
            ));
        }
        assert!(logs.text_bytes() <= GIT_LOG_TEXT_BUDGET_BYTES);
        assert!(logs.truncated);
        assert!(matches!(logs.line_at(0), Some(GitLogLineRef::TruncationMarker)));
        assert_eq!(
            (0..logs.line_count())
                .filter(|index| matches!(logs.line_at(*index), Some(GitLogLineRef::TruncationMarker)))
                .count(),
            1
        );
        logs.clear();
        assert!(logs.is_empty());
        assert!(!logs.truncated);
    }

    #[test]
    fn progress_and_failure_pane_state_are_typed_and_mutually_exclusive() {
        let root = Path::new("/tmp/repo-a");
        let context = GitRuntimeContext::new(GitRuntimeStage::Commit, 1, 2, root);
        let mut state = GitPanelState::default();
        state.pending = true;
        state.apply_runtime_event(GitRuntimeEvent::Stage(context.clone()));
        assert_eq!(state.pending_label.as_deref(), Some("Commit 1/2 · repo-a"));
        state.apply_runtime_event(GitRuntimeEvent::HookStarted {
            context: context.clone(),
            hook_name: "pre-commit".to_string(),
            session_id: "s".to_string(),
            child_id: 7,
        });
        assert_eq!(
            state.pending_label.as_deref(),
            Some("Commit 1/2 · repo-a · pre-commit")
        );
        state.apply_runtime_event(GitRuntimeEvent::Output {
            stream: crate::platform::ProcessOutputStream::Stderr,
            spans: vec![GitLogSpan {
                text: "hook output".to_string(),
                ansi_fg: None,
            }],
        });
        assert_eq!(
            state.pending_label.as_deref(),
            Some("Commit 1/2 · repo-a · pre-commit")
        );
        state.apply_runtime_event(GitRuntimeEvent::HookFinished {
            context: context.clone(),
            hook_name: "pre-commit".to_string(),
            session_id: "s".to_string(),
            child_id: 7,
            code: 0,
            duration_secs: Some(0.4),
        });
        assert_eq!(state.pending_label.as_deref(), Some("Commit 1/2 · repo-a"));
        let push = GitRuntimeContext::new(GitRuntimeStage::Push, 1, 2, root);
        state.apply_runtime_event(GitRuntimeEvent::Stage(push.clone()));
        assert_eq!(state.pending_label.as_deref(), Some("Push 1/2 · repo-a"));
        state.apply_runtime_event(GitRuntimeEvent::RefreshingStatus);
        assert_eq!(
            state.pending_label.as_deref(),
            Some("Обновление Git status")
        );

        state.bottom_pane = GitBottomPane::Closed;
        state.toggle_graph_pane();
        assert_eq!(state.bottom_pane, GitBottomPane::Graph);
        state.toggle_graph_pane();
        assert_eq!(state.bottom_pane, GitBottomPane::Closed);
        state.toggle_logs_pane();
        assert_eq!(state.bottom_pane, GitBottomPane::Logs);
        state.toggle_logs_pane();
        assert_eq!(state.bottom_pane, GitBottomPane::Closed);
        state.toggle_graph_pane();
        state.toggle_logs_pane();
        assert_eq!(state.bottom_pane, GitBottomPane::Logs);
        state.toggle_graph_pane();
        assert_eq!(state.bottom_pane, GitBottomPane::Graph);
    }

    #[test]
    fn nested_trace2_hooks_restore_the_outer_hook_progress_label() {
        let context = GitRuntimeContext::new(
            GitRuntimeStage::Commit,
            1,
            1,
            Path::new("/tmp/repo-a"),
        );
        let mut state = GitPanelState::default();
        state.apply_runtime_event(GitRuntimeEvent::HookStarted {
            context: context.clone(),
            hook_name: "pre-commit".to_string(),
            session_id: "s".to_string(),
            child_id: 1,
        });
        state.apply_runtime_event(GitRuntimeEvent::HookStarted {
            context: context.clone(),
            hook_name: "commit-msg".to_string(),
            session_id: "s/child".to_string(),
            child_id: 1,
        });
        assert_eq!(
            state.pending_label.as_deref(),
            Some("Commit 1/1 · repo-a · commit-msg")
        );
        state.apply_runtime_event(GitRuntimeEvent::HookFinished {
            context,
            hook_name: "commit-msg".to_string(),
            session_id: "s/child".to_string(),
            child_id: 1,
            code: 0,
            duration_secs: Some(0.1),
        });
        assert_eq!(
            state.pending_label.as_deref(),
            Some("Commit 1/1 · repo-a · pre-commit")
        );
    }

    #[test]
    fn runtime_failure_events_open_logs_immediately_and_preserve_larger_height() {
        let context = GitRuntimeContext::new(
            GitRuntimeStage::Commit,
            1,
            2,
            Path::new("/tmp/repo-a"),
        );
        for (event, pane, ratio, expected_ratio) in [
            (test_hook_finished(&context, 1), GitBottomPane::Closed, 0.25, 0.50),
            (test_command_finished(&context, false), GitBottomPane::Graph, 0.30, 0.50),
            (test_command_finished(&context, false), GitBottomPane::Graph, 0.67, 0.67),
            (test_hook_finished(&context, 2), GitBottomPane::Logs, 0.72, 0.72),
        ] {
            let mut state = GitPanelState::default();
            state.bottom_pane = pane;
            state.graph_height_ratio = ratio;
            state.apply_runtime_event(event);
            assert_eq!(state.bottom_pane, GitBottomPane::Logs);
            assert_eq!(state.graph_height_ratio, expected_ratio);
        }

        let mut success = GitPanelState::default();
        success.bottom_pane = GitBottomPane::Graph;
        success.graph_height_ratio = 0.31;
        success.apply_runtime_event(test_hook_finished(&context, 0));
        assert_eq!(success.bottom_pane, GitBottomPane::Graph);
        success.apply_runtime_event(test_command_finished(&context, true));
        assert_eq!(success.bottom_pane, GitBottomPane::Graph);
        assert_eq!(success.graph_height_ratio, 0.31);
    }

    #[test]
    fn runtime_failure_auto_open_restores_follow_tail_only_when_logs_were_hidden() {
        let context = GitRuntimeContext::new(
            GitRuntimeStage::Commit,
            1,
            1,
            Path::new("/tmp/repo-a"),
        );
        for (event, pane, follow_tail_before, follow_tail_after) in [
            (
                test_command_finished(&context, false),
                GitBottomPane::Closed,
                false,
                true,
            ),
            (
                test_hook_finished(&context, 1),
                GitBottomPane::Graph,
                false,
                true,
            ),
            (
                test_command_finished(&context, false),
                GitBottomPane::Logs,
                false,
                false,
            ),
            (
                test_hook_finished(&context, 1),
                GitBottomPane::Logs,
                true,
                true,
            ),
        ] {
            let mut state = GitPanelState::default();
            state.bottom_pane = pane;
            state.logs_follow_tail = follow_tail_before;
            state.apply_runtime_event(event);
            assert_eq!(state.bottom_pane, GitBottomPane::Logs);
            assert_eq!(state.logs_follow_tail, follow_tail_after);
        }
    }

    #[test]
    fn successful_runtime_events_do_not_change_hidden_logs_follow_tail() {
        let context = GitRuntimeContext::new(
            GitRuntimeStage::Commit,
            1,
            1,
            Path::new("/tmp/repo-a"),
        );
        let mut state = GitPanelState::default();
        state.bottom_pane = GitBottomPane::Graph;
        state.logs_follow_tail = false;
        state.apply_runtime_event(test_hook_finished(&context, 0));
        assert_eq!(state.bottom_pane, GitBottomPane::Graph);
        assert!(!state.logs_follow_tail);
        state.apply_runtime_event(test_command_finished(&context, true));
        assert_eq!(state.bottom_pane, GitBottomPane::Graph);
        assert!(!state.logs_follow_tail);
    }

    #[test]
    fn first_repo_runtime_failure_opens_logs_before_second_repo_stage() {
        let mut state = GitPanelState::default();
        let repo_one = GitRuntimeContext::new(
            GitRuntimeStage::Commit,
            1,
            2,
            Path::new("/tmp/repo-a"),
        );
        state.apply_runtime_event(GitRuntimeEvent::Stage(repo_one.clone()));
        state.apply_runtime_event(test_command_finished(&repo_one, false));
        assert_eq!(state.bottom_pane, GitBottomPane::Logs);

        let repo_two = GitRuntimeContext::new(
            GitRuntimeStage::Commit,
            2,
            2,
            Path::new("/tmp/repo-b"),
        );
        state.apply_runtime_event(GitRuntimeEvent::Stage(repo_two));
        assert_eq!(state.bottom_pane, GitBottomPane::Logs);
        assert_eq!(state.pending_label.as_deref(), Some("Commit 2/2 · repo-b"));
    }

    #[test]
    fn commit_menus_have_open_times_and_are_mutually_exclusive() {
        let mut state = GitPanelState::default();
        let first = std::time::Instant::now();
        state.toggle_commit_menu(first);
        assert_eq!(state.commit_menu_opened_at, Some(first));
        assert!(state.commit_options_menu_opened_at.is_none());
        let second = first + std::time::Duration::from_millis(2);
        state.toggle_commit_options_menu(second);
        assert!(state.commit_menu_opened_at.is_none());
        assert_eq!(state.commit_options_menu_opened_at, Some(second));
        assert!(!state.commit_options.any_enabled());
        state.commit_options.skip_hooks = true;
        assert!(state.commit_options.any_enabled());
    }

    #[test]
    fn commit_dropdown_options_keep_existing_action_semantics() {
        assert_eq!(git_commit_option_flags(0), Some((false, false)));
        assert_eq!(git_commit_option_flags(1), Some((true, false)));
        assert_eq!(git_commit_option_flags(2), Some((false, true)));
        assert_eq!(git_commit_option_flags(3), None);
        assert_eq!(GIT_COMMIT_TIMEOUT.as_secs(), 300);
    }

    #[test]
    fn runtime_events_build_semantic_console_lines_without_text_guessing() {
        let context = GitRuntimeContext::new(GitRuntimeStage::Commit, 1, 1, Path::new("/tmp/repo"));
        let mut state = GitPanelState::default();
        state.apply_runtime_event(GitRuntimeEvent::CommandStarted {
            context: context.clone(),
            unix_secs: 3_661,
        });
        state.apply_runtime_event(GitRuntimeEvent::Output {
            stream: crate::platform::ProcessOutputStream::Stdout,
            spans: vec![GitLogSpan {
                text: "error is just stdout text".to_string(),
                ansi_fg: None,
            }],
        });
        state.apply_runtime_event(GitRuntimeEvent::Output {
            stream: crate::platform::ProcessOutputStream::Stderr,
            spans: vec![GitLogSpan {
                text: "plain stderr".to_string(),
                ansi_fg: None,
            }],
        });
        state.apply_runtime_event(GitRuntimeEvent::HookStarted {
            context: context.clone(),
            hook_name: "commit-msg".to_string(),
            session_id: "s".to_string(),
            child_id: 2,
        });
        state.apply_runtime_event(GitRuntimeEvent::HookFinished {
            context: context.clone(),
            hook_name: "commit-msg".to_string(),
            session_id: "s".to_string(),
            child_id: 2,
            code: 1,
            duration_secs: Some(0.5),
        });
        state.apply_runtime_event(GitRuntimeEvent::CommandFinished {
            context,
            code: Some(1),
            success: false,
            duration_secs: 0.6,
            detail: Some("failed".to_string()),
        });

        let kinds = (0..state.git_logs.line_count())
            .filter_map(|idx| state.git_logs.line_at(idx))
            .map(|line| line.kind())
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec![
                GitLogKind::Header,
                GitLogKind::Stdout,
                GitLogKind::Stderr,
                GitLogKind::Hook,
                GitLogKind::Failure,
                GitLogKind::Failure,
            ]
        );
        let header = match state.git_logs.line_at(0).unwrap() {
            GitLogLineRef::Line(line) => line.plain_text(),
            GitLogLineRef::TruncationMarker => String::new(),
        };
        assert!(header.starts_with("01:01:01  [repo]  git commit"));
    }

    #[test]
    fn log_scroll_follows_tail_until_user_scrolls_up_and_elapsed_keeps_running() {
        let mut state = GitPanelState::default();
        state.logs_follow_tail = true;
        assert!(state.update_git_logs_scroll(0.016, 120.0));
        assert_eq!(state.logs_scroll.target, 120.0);
        state.scroll_git_logs_by(-50.0, 120.0);
        assert!(!state.logs_follow_tail);
        let target = state.logs_scroll.target;
        state.update_git_logs_scroll(0.016, 180.0);
        assert_eq!(state.logs_scroll.target, target);
        state.scroll_git_logs_by(10_000.0, 180.0);
        assert!(state.logs_follow_tail);

        let now = std::time::Instant::now();
        state.pending = true;
        state.pending_started_at = Some(now - std::time::Duration::from_secs(2));
        let before = state.pending_elapsed_secs(now).unwrap();
        state.apply_runtime_event(GitRuntimeEvent::RefreshingStatus);
        let later = state
            .pending_elapsed_secs(now + std::time::Duration::from_secs(1))
            .unwrap();
        assert!(later >= before + 1.0);
    }

    #[cfg(unix)]
    fn make_hook_repo(name: &str, exit_code: i32) -> (PathBuf, git2::Repository, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "rriter-hook-{name}-{}-{}",
            std::process::id(),
            crate::platform::next_operation_id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let repo = git2::Repository::init(&root).unwrap();
        std::fs::write(root.join("file.txt"), "content\n").unwrap();
        toggle_stage(&root, "file.txt", None, false).unwrap();
        let marker = root.join("hook-marker.txt");
        let hook = root.join(".git/hooks/pre-commit");
        std::fs::write(
            &hook,
            format!(
                "#!/bin/sh\nprintf 'hook stdout\\n'\nprintf 'hook stderr\\n' >&2\nprintf ran > '{}'\nexit {exit_code}\n",
                marker.display()
            ),
        )
        .unwrap();
        let mut perms = std::fs::metadata(&hook).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&hook, perms).unwrap();
        (root, repo, marker)
    }

    #[cfg(unix)]
    fn collect_runtime_events(
        repo_root: &Path,
        skip_hooks: bool,
    ) -> (Result<(), String>, Vec<GitRuntimeEvent>) {
        let (tx, rx) = mpsc::sync_channel(GIT_RUNTIME_EVENT_CAPACITY);
        let mut emitter = GitRuntimeEmitter::new(Some(&tx));
        let result = commit_repo_with_runtime(repo_root, "test commit", false, skip_hooks, 1, 1, &mut emitter);
        drop(emitter);
        drop(tx);
        let events = rx.try_iter().collect::<Vec<_>>();
        (result, events)
    }

    #[cfg(unix)]
    #[test]
    fn real_commit_hook_runs_and_trace2_identifies_it_structurally() {
        let (root, repo, marker) = make_hook_repo("success", 0);
        let (result, events) = collect_runtime_events(&root, false);
        assert!(result.is_ok());
        assert!(marker.exists());
        assert!(repo.head().unwrap().peel_to_commit().is_ok());
        assert!(events.iter().any(|event| matches!(
            event,
            GitRuntimeEvent::HookStarted { hook_name, .. } if hook_name == "pre-commit"
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            GitRuntimeEvent::Output { spans, .. }
                if spans.iter().any(|span| span.text.contains("hook stdout"))
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            GitRuntimeEvent::Output { spans, .. }
                if spans.iter().any(|span| span.text.contains("hook stderr"))
        )));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn failing_hook_blocks_commit_but_no_verify_bypasses_commit_hooks_only() {
        let (root, repo, marker) = make_hook_repo("failure", 3);
        let (result, events) = collect_runtime_events(&root, false);
        assert!(result.is_err());
        assert!(marker.exists());
        assert!(repo.head().is_err());
        assert!(events.iter().any(|event| matches!(
            event,
            GitRuntimeEvent::HookFinished { hook_name, code: 3, .. } if hook_name == "pre-commit"
        )));

        std::fs::remove_file(&marker).unwrap();
        let (result, _events) = collect_runtime_events(&root, true);
        assert!(result.is_ok());
        assert!(!marker.exists());
        assert!(repo.head().unwrap().peel_to_commit().is_ok());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn commit_fallback_identity_is_command_local_and_does_not_write_repo_config() {
        let (root, repo, _marker) = make_hook_repo("identity", 0);
        let config = repo.config().unwrap();
        assert!(config.get_string("user.name").is_err());
        assert!(config.get_string("user.email").is_err());
        drop(config);
        commit_repo(&root, "identity", false).unwrap();
        let config = repo.config().unwrap();
        assert!(config.get_string("user.name").is_err());
        assert!(config.get_string("user.email").is_err());
        let commit = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(commit.author().name(), Some("RRiter"));
        assert_eq!(commit.author().email(), Some("rriter@example.invalid"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn configured_identity_is_preserved_and_amend_reset_author_matches_old_semantics() {
        let (root, repo, _marker) = make_hook_repo("configured-identity", 0);
        {
            let mut config = repo.config().unwrap();
            config.set_str("user.name", "Configured One").unwrap();
            config.set_str("user.email", "one@example.test").unwrap();
        }
        commit_repo(&root, "initial", false).unwrap();
        let initial = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(initial.author().name(), Some("Configured One"));
        drop(initial);

        {
            let mut config = repo.config().unwrap();
            config.set_str("user.name", "Configured Two").unwrap();
            config.set_str("user.email", "two@example.test").unwrap();
        }
        std::fs::write(root.join("file.txt"), "changed\n").unwrap();
        toggle_stage(&root, "file.txt", None, false).unwrap();
        commit_repo(&root, "amended", true).unwrap();
        let amended = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(amended.message(), Some("amended\n"));
        assert_eq!(amended.author().name(), Some("Configured Two"));
        assert_eq!(amended.author().email(), Some("two@example.test"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn streaming_push_reports_pre_push_hook_by_trace2_name() {
        use std::os::unix::fs::PermissionsExt;
        let (root, repo, _marker) = make_hook_repo("pre-push", 0);
        commit_repo(&root, "initial", false).unwrap();
        let remote_root = root.with_extension("bare.git");
        git2::Repository::init_bare(&remote_root).unwrap();
        repo.remote("origin", remote_root.to_string_lossy().as_ref())
            .unwrap();

        let marker = root.join("pre-push-marker.txt");
        let hook = root.join(".git/hooks/pre-push");
        std::fs::write(
            &hook,
            format!(
                "#!/bin/sh\nprintf ran > '{}'\nprintf 'pre-push output\\n'\nexit 0\n",
                marker.display()
            ),
        )
        .unwrap();
        let mut perms = std::fs::metadata(&hook).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&hook, perms).unwrap();

        let (tx, rx) = mpsc::sync_channel(GIT_RUNTIME_EVENT_CAPACITY);
        let mut emitter = GitRuntimeEmitter::new(Some(&tx));
        push_repo_with_runtime(&root, 1, 1, &mut emitter).unwrap();
        drop(emitter);
        drop(tx);
        let events = rx.try_iter().collect::<Vec<_>>();
        assert!(marker.exists());
        assert!(events.iter().any(|event| matches!(
            event,
            GitRuntimeEvent::HookStarted { hook_name, context, .. }
                if hook_name == "pre-push" && context.stage == GitRuntimeStage::Push
        )));

        std::fs::remove_dir_all(&root).unwrap();
        std::fs::remove_dir_all(&remote_root).unwrap();
    }
}
