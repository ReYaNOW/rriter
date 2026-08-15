use super::terminal::TermGrid;
use crate::platform::{self, PlatformKind, ProcessTree};
use alacritty_terminal::vte::Parser;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const INITIAL_COLS: u16 = 200;
const INITIAL_ROWS: u16 = 60;
const TERMINAL_SHUTDOWN_GRACE: Duration = Duration::from_millis(500);
const TERMINAL_TITLE_REFRESH_INTERVAL: Duration = Duration::from_millis(250);
const TERMINAL_EXIT_BEFORE_OUTPUT: &[u8] = b"RRiter terminal exited before producing output\r\n";
pub(crate) const TERMINAL_TITLE_MAX_BYTES: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProgrammedTerminalTitle {
    text: String,
    generation: u64,
    observation_serial: u64,
}

#[derive(Debug)]
pub(crate) struct TerminalTitleState {
    fallback: String,
    detected: Option<String>,
    programmed: Option<ProgrammedTerminalTitle>,
    display_suffix: Box<str>,
    generation: u64,
    observation_serial: u64,
}

pub(crate) type TerminalTitleCache = Arc<Mutex<TerminalTitleState>>;

impl TerminalTitleState {
    pub(crate) fn new(fallback: String) -> Self {
        Self {
            fallback,
            detected: None,
            programmed: None,
            display_suffix: Box::<str>::default(),
            generation: 0,
            observation_serial: 0,
        }
    }

    pub(crate) fn new_numbered(fallback: String, display_number: u64) -> Self {
        let mut state = Self::new(fallback);
        state.display_suffix = format!(" ({display_number})").into_boxed_str();
        state
    }

    pub(crate) fn set_fallback(&mut self, fallback: String) {
        self.fallback = fallback;
    }

    pub(crate) fn set_programmed(&mut self, text: String) {
        self.programmed = Some(ProgrammedTerminalTitle {
            text,
            generation: self.generation,
            observation_serial: self.observation_serial,
        });
    }

    fn observe_unchanged(&mut self) {
        self.observation_serial = self.observation_serial.wrapping_add(1);
    }

    fn observe_detected(&mut self, detected: String) {
        self.observation_serial = self.observation_serial.wrapping_add(1);
        self.detected = Some(detected);
    }

    fn observe_transition(&mut self, detected: Option<String>, carry_recent_programmed: bool) {
        let previous_serial = self.observation_serial;
        let previous_generation = self.generation;
        self.observation_serial = self.observation_serial.wrapping_add(1);
        self.generation = self.generation.wrapping_add(1);
        if carry_recent_programmed
            && let Some(programmed) = self.programmed.as_mut()
            && programmed.generation == previous_generation
            && programmed.observation_serial == previous_serial
        {
            programmed.generation = self.generation;
        }
        self.detected = detected;
    }

    fn resolved(&self) -> &str {
        if let Some(detected) = self.detected.as_deref() {
            detected
        } else if let Some(programmed) = self
            .programmed
            .as_ref()
            .filter(|programmed| programmed.generation == self.generation)
        {
            &programmed.text
        } else {
            &self.fallback
        }
    }

    pub(crate) fn write_resolved(&self, output: &mut String) {
        output.clear();
        output.push_str(self.resolved());
        output.push_str(&self.display_suffix);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TerminalShellSpec {
    pub executable: PathBuf,
    pub args: Vec<OsString>,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TerminalShellCandidate {
    executable: OsString,
    args: Vec<OsString>,
    strict: bool,
}

pub(crate) struct TerminalProcess {
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master_pty: Arc<Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    tree: ProcessTree,
    title_stop_tx: Option<std::sync::mpsc::Sender<()>>,
    title_worker: Option<JoinHandle<()>>,
    finished: bool,
}

impl TerminalProcess {
    pub(crate) fn spawn(
        grid: Arc<Mutex<TermGrid>>,
        title_cache: TerminalTitleCache,
        window: Option<Arc<winit::window::Window>>,
        cwd: Option<&Path>,
    ) -> io::Result<(Self, TerminalShellSpec)> {
        let shell = resolve_terminal_shell()?;
        let fallback = terminal_fallback_title(cwd, &shell.title);
        crate::platform::lock_recover(&title_cache).set_fallback(fallback);
        let pty_system = NativePtySystem::default();
        let pair = pty_system
            .openpty(PtySize {
                rows: INITIAL_ROWS,
                cols: INITIAL_COLS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| io::Error::other(format!("failed to open PTY: {error}")))?;

        let mut command = CommandBuilder::new(&shell.executable);
        command.args(&shell.args);
        if let Some(cwd) = cwd.filter(|path| path.is_dir()) {
            command.cwd(cwd.as_os_str());
        }
        if platform::CURRENT_PLATFORM != PlatformKind::Windows {
            command.env("TERM", "xterm-256color");
        }

        let mut child = pair.slave.spawn_command(command).map_err(|error| {
            io::Error::other(format!("failed to spawn terminal shell: {error}"))
        })?;
        drop(pair.slave);

        let process_id = child.process_id().ok_or_else(|| {
            io::Error::other("terminal backend did not expose the child process id")
        })?;
        let mut tree = match ProcessTree::attach_process_id(process_id) {
            Ok(tree) => tree,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(io::Error::new(
                    error.kind(),
                    format!("failed to own terminal process tree: {error}"),
                ));
            }
        };

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| io::Error::other(format!("failed to clone PTY reader: {error}")))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| io::Error::other(format!("failed to take PTY writer: {error}")))?;
        let writer = Arc::new(Mutex::new(writer));
        let master_pty = Arc::new(Mutex::new(pair.master));
        let (title_stop_tx, title_worker) = match install_terminal_title_refresh(
            master_pty.clone(),
            title_cache,
            shell.title.clone(),
            cwd.map(Path::to_path_buf),
            window.clone(),
        ) {
            Ok(handles) => handles,
            Err(error) => {
                let _ = tree.terminate_forcefully();
                let _ = child.kill();
                let _ = child.wait();
                tree.finish_after_owner_exit();
                return Err(error);
            }
        };
        if let Err(error) = install_terminal_io_threads(&grid, reader, writer.clone(), window) {
            if let Some(stop_tx) = title_stop_tx {
                let _ = stop_tx.send(());
            }
            if let Some(worker) = title_worker {
                crate::platform::reap_unit_thread(worker);
            }
            let _ = tree.terminate_forcefully();
            let _ = child.kill();
            let _ = child.wait();
            tree.finish_after_owner_exit();
            return Err(error);
        }

        Ok((
            Self {
                writer,
                master_pty,
                child,
                tree,
                title_stop_tx,
                title_worker,
                finished: false,
            },
            shell,
        ))
    }

    pub(crate) fn write_input(&self, bytes: &[u8]) -> io::Result<()> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| io::Error::other("terminal writer lock is poisoned"))?;
        writer.write_all(bytes)?;
        writer.flush()
    }

    pub(crate) fn resize(&self, cols: u16, rows: u16) -> io::Result<()> {
        if cols == 0 || rows == 0 {
            return Ok(());
        }
        let master = self
            .master_pty
            .lock()
            .map_err(|_| io::Error::other("terminal PTY lock is poisoned"))?;
        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| io::Error::other(format!("failed to resize PTY: {error}")))
    }

    pub(crate) fn try_wait(&mut self) -> io::Result<bool> {
        if self.finished {
            return Ok(true);
        }
        if self.child.try_wait()?.is_some() {
            self.finished = true;
            self.stop_title_refresh();
            self.tree.finish_after_owner_exit();
        }
        Ok(self.finished)
    }

    fn stop_title_refresh(&mut self) {
        if let Some(stop_tx) = self.title_stop_tx.take() {
            let _ = stop_tx.send(());
        }
        if let Some(worker) = self.title_worker.take() {
            crate::platform::reap_unit_thread(worker);
        }
    }

    pub(crate) fn shutdown(&mut self) {
        self.stop_title_refresh();
        if self.finished {
            return;
        }

        let _ = self.tree.terminate_gracefully();
        let deadline = Instant::now() + TERMINAL_SHUTDOWN_GRACE;
        while Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(Some(_)) => {
                    self.finished = true;
                    self.tree.finish_after_owner_exit();
                    return;
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                Err(_) => break,
            }
        }

        let _ = self.tree.terminate_forcefully();
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.finished = true;
        self.tree.finish_after_owner_exit();
    }
}

impl Drop for TerminalProcess {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub(crate) fn terminal_programmed_title(parts: &[&[u8]]) -> Option<String> {
    let capacity = parts
        .iter()
        .fold(parts.len().saturating_sub(1), |total, part| {
            total.saturating_add(part.len())
        })
        .min(TERMINAL_TITLE_MAX_BYTES);
    let mut title = String::with_capacity(capacity);

    'parts: for (index, part) in parts.iter().enumerate() {
        if index != 0 {
            if title.len() == TERMINAL_TITLE_MAX_BYTES {
                break;
            }
            title.push(';');
        }
        let part = std::str::from_utf8(part).ok()?;
        for ch in part.chars() {
            if ch.is_control() {
                continue;
            }
            if title.len() + ch.len_utf8() > TERMINAL_TITLE_MAX_BYTES {
                break 'parts;
            }
            title.push(ch);
        }
    }

    let first_non_whitespace = title
        .char_indices()
        .find_map(|(index, ch)| (!ch.is_whitespace()).then_some(index))?;
    let trimmed_len = title.trim_end().len();
    title.truncate(trimmed_len);
    if first_non_whitespace != 0 {
        title.drain(..first_non_whitespace);
    }
    (!title.is_empty()).then_some(title)
}

fn bounded_terminal_title(text: &str) -> String {
    let mut title = String::with_capacity(text.len().min(TERMINAL_TITLE_MAX_BYTES));
    for ch in text.chars() {
        if ch.is_control() {
            continue;
        }
        if title.len() + ch.len_utf8() > TERMINAL_TITLE_MAX_BYTES {
            break;
        }
        title.push(ch);
    }
    let trimmed_len = title.trim_end().len();
    title.truncate(trimmed_len);
    title
}

fn terminal_process_program_name(snapshot: &platform::ProcessSnapshot, shell_title: &str) -> String {
    snapshot
        .executable
        .as_deref()
        .map(terminal_shell_title)
        .or_else(|| {
            snapshot
                .args
                .first()
                .map(|arg| terminal_shell_title(Path::new(arg)))
        })
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| shell_title.to_string())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SshDestination {
    user: Option<String>,
    host: String,
}

fn ssh_option_takes_value(option: &str) -> bool {
    matches!(
        option,
        "-B"
            | "-b"
            | "-c"
            | "-D"
            | "-E"
            | "-e"
            | "-F"
            | "-I"
            | "-i"
            | "-J"
            | "-L"
            | "-l"
            | "-m"
            | "-O"
            | "-o"
            | "-P"
            | "-p"
            | "-Q"
            | "-R"
            | "-S"
            | "-W"
            | "-w"
    )
}

fn parse_ssh_destination(args: &[OsString]) -> Option<SshDestination> {
    let mut explicit_user = None;
    let mut index = 0usize;
    while index < args.len() {
        let arg = args[index].to_string_lossy();
        if arg == "--" {
            index += 1;
            break;
        }
        if arg == "-l" {
            let user = args.get(index + 1)?.to_string_lossy();
            if !user.is_empty() {
                explicit_user = Some(user.into_owned());
            }
            index += 2;
            continue;
        }
        if let Some(user) = arg.strip_prefix("-l").filter(|user| !user.is_empty()) {
            explicit_user = Some(user.to_string());
            index += 1;
            continue;
        }
        if arg.starts_with('-') && arg != "-" {
            let short_option = arg.get(..2).unwrap_or(arg.as_ref());
            index += if arg.len() == 2 && ssh_option_takes_value(short_option) {
                2
            } else {
                1
            };
            continue;
        }
        break;
    }

    let operand = args.get(index)?.to_string_lossy();
    if operand.is_empty() {
        return None;
    }
    let (operand_user, host) = operand
        .split_once('@')
        .map_or((None, operand.as_ref()), |(user, host)| {
            ((!user.is_empty()).then(|| user.to_string()), host)
        });
    if host.is_empty() {
        return None;
    }
    Some(SshDestination {
        user: operand_user.or(explicit_user),
        host: host.to_string(),
    })
}

fn terminal_title_for_snapshot(
    snapshot: &platform::ProcessSnapshot,
    initial_cwd: Option<&Path>,
    home: Option<&Path>,
    shell_title: &str,
) -> String {
    let program = terminal_process_program_name(snapshot, shell_title);
    if program.eq_ignore_ascii_case("ssh")
        && let Some(destination) = parse_ssh_destination(snapshot.args.get(1..).unwrap_or_default())
    {
        let raw = match destination.user {
            Some(user) => format!("({user}) {}", destination.host),
            None => destination.host,
        };
        return bounded_terminal_title(&raw);
    }

    let cwd = snapshot.cwd.as_deref().or(initial_cwd);
    bounded_terminal_title(&terminal_fallback_title_with_home(
        cwd,
        home,
        &program,
    ))
}

#[cfg(target_os = "linux")]
fn terminal_process_identity_changed(
    previous: &platform::ProcessSnapshot,
    current: &platform::ProcessSnapshot,
) -> bool {
    if previous.process_id != current.process_id {
        return true;
    }
    match (&previous.executable, &current.executable) {
        (Some(previous), Some(current)) => previous != current,
        _ => previous.args.first() != current.args.first(),
    }
}

#[cfg(target_os = "linux")]
fn terminal_foreground_transitioned(
    previous_process_group: Option<u32>,
    process_group: u32,
    previous: Option<&platform::ProcessSnapshot>,
    current: Option<&platform::ProcessSnapshot>,
) -> bool {
    if previous_process_group != Some(process_group) {
        return true;
    }
    match (previous, current) {
        (None, Some(_)) => true,
        (Some(previous), Some(current)) => terminal_process_identity_changed(previous, current),
        _ => false,
    }
}

#[cfg(target_os = "linux")]
fn refresh_terminal_title_cache(
    master_pty: &Arc<Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
    title_cache: &TerminalTitleCache,
    shell_title: &str,
    initial_cwd: Option<&Path>,
    home: Option<&Path>,
    last_process_group: &mut Option<u32>,
    snapshot: &mut Option<platform::ProcessSnapshot>,
) -> bool {
    let process_group = crate::platform::lock_recover(master_pty)
        .process_group_leader()
        .and_then(|pid| u32::try_from(pid).ok());
    let Some(process_group) = process_group else {
        crate::platform::lock_recover(title_cache).observe_unchanged();
        return false;
    };

    let found = platform::foreground_process_snapshot(process_group);
    if terminal_foreground_transitioned(
        *last_process_group,
        process_group,
        snapshot.as_ref(),
        found.as_ref(),
    ) {
        let previous_was_shell = snapshot.as_ref().is_some_and(|snapshot| {
            terminal_process_program_name(snapshot, shell_title) == shell_title
        });
        let initial_observation = last_process_group.is_none();
        let detected = found.as_ref().map(|snapshot| {
            terminal_title_for_snapshot(snapshot, initial_cwd, home, shell_title)
        });
        let new_is_shell = found.as_ref().is_some_and(|snapshot| {
            terminal_process_program_name(snapshot, shell_title) == shell_title
        });
        let carry_recent_programmed =
            initial_observation || (previous_was_shell && !new_is_shell);
        *last_process_group = Some(process_group);
        *snapshot = found;
        crate::platform::lock_recover(title_cache)
            .observe_transition(detected, carry_recent_programmed);
        return true;
    }
    *last_process_group = Some(process_group);

    let Some(found) = found else {
        crate::platform::lock_recover(title_cache).observe_unchanged();
        return false;
    };
    if snapshot.as_ref() != Some(&found) {
        let detected = terminal_title_for_snapshot(&found, initial_cwd, home, shell_title);
        *snapshot = Some(found);
        crate::platform::lock_recover(title_cache).observe_detected(detected);
        return true;
    }

    crate::platform::lock_recover(title_cache).observe_unchanged();
    false
}

#[cfg(target_os = "linux")]
fn install_terminal_title_refresh(
    master_pty: Arc<Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
    title_cache: TerminalTitleCache,
    shell_title: String,
    initial_cwd: Option<PathBuf>,
    window: Option<Arc<winit::window::Window>>,
) -> io::Result<(
    Option<std::sync::mpsc::Sender<()>>,
    Option<JoinHandle<()>>,
)> {
    let (stop_tx, stop_rx) = std::sync::mpsc::channel();
    let worker = crate::platform::spawn_named("rriter-session-title", move || {
        let home = platform::user_home_dir();
        let mut last_process_group = None;
        let mut snapshot = None;
        loop {
            if refresh_terminal_title_cache(
                &master_pty,
                &title_cache,
                &shell_title,
                initial_cwd.as_deref(),
                home.as_deref(),
                &mut last_process_group,
                &mut snapshot,
            ) && let Some(window) = window.as_ref()
            {
                window.request_redraw();
            }

            match stop_rx.recv_timeout(TERMINAL_TITLE_REFRESH_INTERVAL) {
                Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    })?;
    Ok((Some(stop_tx), Some(worker)))
}

#[cfg(not(target_os = "linux"))]
fn install_terminal_title_refresh(
    _master_pty: Arc<Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
    _title_cache: TerminalTitleCache,
    _shell_title: String,
    _initial_cwd: Option<PathBuf>,
    _window: Option<Arc<winit::window::Window>>,
) -> io::Result<(
    Option<std::sync::mpsc::Sender<()>>,
    Option<JoinHandle<()>>,
)> {
    Ok((None, None))
}

fn advance_terminal_output_batch(
    parser: &mut Parser,
    grid: &mut TermGrid,
    chunks: &[Vec<u8>],
) {
    for chunk in chunks {
        parser.advance(grid, chunk);
    }
    grid.dirty = true;
}

fn finish_terminal_output_stream(parser: &mut Parser, grid: &mut TermGrid) -> bool {
    if grid.presentation_ready {
        return false;
    }

    parser.advance(grid, TERMINAL_EXIT_BEFORE_OUTPUT);
    true
}

fn install_terminal_io_threads(
    grid: &Arc<Mutex<TermGrid>>,
    reader: Box<dyn Read + Send>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    window: Option<Arc<winit::window::Window>>,
) -> io::Result<()> {
    let (reply_tx, reply_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    let reader_finished = Arc::new(AtomicBool::new(false));

    let parser_grid = grid.clone();
    let parser_reader_finished = reader_finished.clone();
    crate::platform::spawn_named("rriter-terminal-parser", move || {
        let mut parser = Parser::new();
        let request_redraw = || {
            if let Some(window) = window.as_ref() {
                window.request_redraw();
            }
        };
        let mut first = rx.recv().ok();

        while let Some(first_chunk) = first {
            let mut chunks = vec![first_chunk];
            let started = Instant::now();
            loop {
                match rx.recv_timeout(Duration::from_millis(8)) {
                    Ok(next) => {
                        chunks.push(next);
                        if started.elapsed() >= Duration::from_millis(32) {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }

            let Ok(mut grid) = parser_grid.lock() else {
                break;
            };
            advance_terminal_output_batch(&mut parser, &mut grid, &chunks);
            drop(grid);
            request_redraw();
            first = rx.recv().ok();
        }

        if parser_reader_finished.load(Ordering::Acquire) {
            let Ok(mut grid) = parser_grid.lock() else {
                return;
            };
            if finish_terminal_output_stream(&mut parser, &mut grid) {
                drop(grid);
                request_redraw();
            }
        }
    })
    .map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("failed to spawn terminal parser: {error}"),
        )
    })?;

    crate::platform::spawn_named("rriter-terminal-writer", move || {
        while let Ok(message) = reply_rx.recv() {
            let Ok(mut writer) = writer.lock() else {
                break;
            };
            if writer.write_all(&message).is_err() || writer.flush().is_err() {
                break;
            }
        }
    })
    .map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("failed to spawn terminal writer: {error}"),
        )
    })?;

    crate::platform::spawn_named("rriter-terminal-reader", move || {
        let mut reader = reader;
        let mut buffer = [0_u8; 65_536];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    if tx.send(buffer[..read].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        reader_finished.store(true, Ordering::Release);
    })
    .map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("failed to spawn terminal reader: {error}"),
        )
    })?;

    crate::platform::lock_recover(&grid).reply_tx = Some(reply_tx);
    Ok(())
}

pub(crate) fn resolve_terminal_shell() -> io::Result<TerminalShellSpec> {
    let candidates =
        terminal_shell_candidates_with(platform::CURRENT_PLATFORM, |name| std::env::var_os(name));
    for candidate in candidates {
        if let Some(executable) = platform::resolve_executable(&candidate.executable) {
            let title = terminal_shell_title(&executable);
            return Ok(TerminalShellSpec {
                executable,
                args: candidate.args,
                title,
            });
        }
        if candidate.strict {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "configured terminal shell was not found: {}",
                    candidate.executable.to_string_lossy()
                ),
            ));
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "no supported terminal shell was found",
    ))
}

fn terminal_shell_candidates_with(
    platform: PlatformKind,
    mut env_value: impl FnMut(&str) -> Option<OsString>,
) -> Vec<TerminalShellCandidate> {
    let mut candidates = Vec::new();
    if let Some(shell) = env_value("RRITER_SHELL").filter(|value| !value.is_empty()) {
        candidates.push(TerminalShellCandidate {
            executable: shell,
            args: Vec::new(),
            strict: true,
        });
        return candidates;
    }
    if let Some(shell) = platform::configured_tool_path(platform::ToolKind::Shell) {
        candidates.push(TerminalShellCandidate {
            executable: shell.into_os_string(),
            args: Vec::new(),
            strict: true,
        });
        return candidates;
    }

    match platform {
        PlatformKind::Windows => {
            candidates.push(shell_candidate("pwsh.exe", ["-NoLogo"]));
            candidates.push(shell_candidate("powershell.exe", ["-NoLogo"]));
            if let Some(comspec) = env_value("COMSPEC").filter(|value| !value.is_empty()) {
                candidates.push(TerminalShellCandidate {
                    executable: comspec,
                    args: Vec::new(),
                    strict: false,
                });
            }
            candidates.push(shell_candidate("cmd.exe", std::iter::empty::<&str>()));
        }
        PlatformKind::Macos => {
            if let Some(shell) = env_value("SHELL").filter(|value| !value.is_empty()) {
                candidates.push(TerminalShellCandidate {
                    executable: shell,
                    args: Vec::new(),
                    strict: false,
                });
            }
            candidates.push(shell_candidate("/bin/zsh", std::iter::empty::<&str>()));
            candidates.push(shell_candidate("/bin/bash", std::iter::empty::<&str>()));
            candidates.push(shell_candidate("/bin/sh", std::iter::empty::<&str>()));
        }
        PlatformKind::Linux | PlatformKind::Other => {
            if let Some(shell) = env_value("SHELL").filter(|value| !value.is_empty()) {
                candidates.push(TerminalShellCandidate {
                    executable: shell,
                    args: Vec::new(),
                    strict: false,
                });
            }
            candidates.push(shell_candidate("/bin/bash", std::iter::empty::<&str>()));
            candidates.push(shell_candidate("/bin/sh", std::iter::empty::<&str>()));
        }
    }
    candidates
}

fn shell_candidate(
    executable: impl Into<OsString>,
    args: impl IntoIterator<Item = impl Into<OsString>>,
) -> TerminalShellCandidate {
    TerminalShellCandidate {
        executable: executable.into(),
        args: args.into_iter().map(Into::into).collect(),
        strict: false,
    }
}

pub(crate) fn select_terminal_working_directory(
    current_file: Option<&Path>,
    workspaces: &[PathBuf],
) -> Option<PathBuf> {
    if let Some(file) = current_file {
        if let Some(workspace) = workspaces
            .iter()
            .filter(|workspace| platform::path_is_within(file, workspace))
            .max_by_key(|workspace| workspace.components().count())
        {
            return Some(workspace.clone());
        }
        if let Some(parent) = file
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            return Some(parent.to_path_buf());
        }
    }
    workspaces.first().cloned()
}

fn terminal_shell_title(path: &Path) -> String {
    let path_text = path.as_os_str().to_string_lossy();
    let file_name = path_text
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or("terminal");
    if file_name
        .get(file_name.len().saturating_sub(4)..)
        .is_some_and(|suffix| suffix.eq_ignore_ascii_case(".exe"))
    {
        file_name[..file_name.len() - 4].to_string()
    } else {
        file_name.to_string()
    }
}

pub(crate) fn terminal_fallback_title(cwd: Option<&Path>, shell_title: &str) -> String {
    let home = platform::user_home_dir();
    terminal_fallback_title_with_home(cwd, home.as_deref(), shell_title)
}

fn terminal_fallback_title_with_home(
    cwd: Option<&Path>,
    home: Option<&Path>,
    shell_title: &str,
) -> String {
    let Some(cwd) = cwd else {
        return shell_title.to_string();
    };

    let cwd_label = if home.is_some_and(|home| platform::paths_equal(cwd, home)) {
        "~".to_string()
    } else if let Some(name) = cwd.file_name().filter(|name| !name.is_empty()) {
        name.to_string_lossy().into_owned()
    } else {
        let root = cwd.as_os_str().to_string_lossy();
        if root.is_empty() {
            "~".to_string()
        } else {
            root.into_owned()
        }
    };

    format!("{cwd_label} : {shell_title}")
}

#[cfg(test)]
mod tests {
    use super::*;


    fn process_snapshot(program: &str, cwd: &str, args: &[&str]) -> platform::ProcessSnapshot {
        platform::ProcessSnapshot {
            process_id: 42,
            executable: Some(PathBuf::from(format!("/usr/bin/{program}"))),
            cwd: Some(PathBuf::from(cwd)),
            args: args.iter().map(OsString::from).collect(),
        }
    }

    fn resolved_title(state: &TerminalTitleState) -> String {
        let mut title = String::new();
        state.write_resolved(&mut title);
        title
    }

    #[test]
    fn numbered_display_title_keeps_suffix_across_dynamic_and_ssh_titles() {
        let home = Path::new("/home/reyan");
        let cwd = Path::new("/home/reyan/projects/car-wash-api");
        let mut state = TerminalTitleState::new_numbered("car-wash-api : fish".to_string(), 3);
        assert_eq!(resolved_title(&state), "car-wash-api : fish (3)");

        let htop = process_snapshot("htop", "/home/reyan/projects/car-wash-api", &["htop"]);
        state.observe_transition(
            Some(terminal_title_for_snapshot(
                &htop,
                Some(cwd),
                Some(home),
                "fish",
            )),
            false,
        );
        assert_eq!(resolved_title(&state), "car-wash-api : htop (3)");

        let fish = process_snapshot("fish", "/home/reyan/projects/car-wash-api", &["fish"]);
        state.observe_transition(
            Some(terminal_title_for_snapshot(
                &fish,
                Some(cwd),
                Some(home),
                "fish",
            )),
            false,
        );
        assert_eq!(resolved_title(&state), "car-wash-api : fish (3)");

        let ssh = process_snapshot(
            "ssh",
            "/home/reyan/projects/car-wash-api",
            &["ssh", "reyan@89.169.37.107"],
        );
        state.observe_transition(
            Some(terminal_title_for_snapshot(
                &ssh,
                Some(cwd),
                Some(home),
                "fish",
            )),
            false,
        );
        assert_eq!(resolved_title(&state), "(reyan) 89.169.37.107 (3)");
    }

    #[test]
    fn numbered_title_stays_in_single_allocation_light_renderer_string_path() {
        let source = include_str!("terminal_process.rs");
        let writer = source
            .split("pub(crate) fn write_resolved")
            .nth(1)
            .unwrap()
            .split("\n    }\n}")
            .next()
            .unwrap();
        assert!(writer.contains("output.push_str(&self.display_suffix);"));
        assert!(!writer.contains("format!("));

        let renderer = include_str!("../render_view/terminal_ui.rs");
        let title_path = renderer
            .split("let mut display_titles")
            .nth(1)
            .unwrap()
            .split("let mut actual_xs")
            .next()
            .unwrap();
        assert!(title_path.contains("terminal.write_display_title(title);"));
        assert!(title_path.contains("self.measure_ui_width(title, 1.0)"));
        assert!(!title_path.contains("display_suffix"));
        assert!(!title_path.contains("format!("));
    }

    fn parser_grid() -> (TermGrid, TerminalTitleCache) {
        let cache = Arc::new(Mutex::new(TerminalTitleState::new("fallback".to_string())));
        (
            TermGrid::new_with_title_cache(8, 2, cache.clone()),
            cache,
        )
    }

    #[test]
    fn terminal_presentation_waits_for_displayable_output_without_timeout_fallback() {
        let mut grid = TermGrid::new(24, 3);
        let mut parser = Parser::new();

        assert!(!grid.presentation_ready);
        advance_terminal_output_batch(
            &mut parser,
            &mut grid,
            &[b"\x1b[?25l\x1b[2J\r\n\t   \x1b[?25h".to_vec()],
        );
        assert!(!grid.presentation_ready);

        advance_terminal_output_batch(
            &mut parser,
            &mut grid,
            &[b"\x1b[32muser@host> \x1b[0m".to_vec()],
        );
        assert!(grid.presentation_ready);

        let source = include_str!("terminal_process.rs");
        let production = source.split("\n#[cfg(test)]").next().unwrap_or(source);
        assert!(!production.contains("TERMINAL_PRESENTATION_FALLBACK"));
        assert!(!production.contains("terminal_presentation_wait_remaining"));
        assert!(!production.contains("grid.mark_presentation_ready();"));
        assert!(production.contains("let mut first = rx.recv().ok();"));
    }

    #[test]
    fn parsed_terminal_output_reveals_once_without_changing_grid_bytes() {
        let chunks = vec![b"\x1b[31mready\x1b[0m".to_vec()];
        let mut expected = TermGrid::new(12, 2);
        let mut expected_parser = Parser::new();
        for chunk in &chunks {
            expected_parser.advance(&mut expected, chunk);
        }

        let mut actual = TermGrid::new(12, 2);
        assert!(!actual.presentation_ready);
        assert!(!actual.presentation_layout_ready);
        let mut parser = Parser::new();
        advance_terminal_output_batch(&mut parser, &mut actual, &chunks);

        assert!(actual.presentation_ready);
        assert!(actual.lines == expected.lines);
        assert_eq!((actual.cur_x, actual.cur_y), (expected.cur_x, expected.cur_y));
        assert_eq!((actual.cur_fg, actual.cur_bg), (expected.cur_fg, expected.cur_bg));

        actual.dirty = false;
        advance_terminal_output_batch(&mut parser, &mut actual, &[b"!".to_vec()]);
        assert!(actual.presentation_ready);
        assert!(actual.dirty);
    }

    #[test]
    fn terminal_stream_end_before_displayable_output_shows_explicit_state_once() {
        let mut grid = TermGrid::new(64, 3);
        let mut parser = Parser::new();
        advance_terminal_output_batch(
            &mut parser,
            &mut grid,
            &[b"\x1b[?25l\x1b[2J\r\n".to_vec()],
        );
        assert!(!grid.presentation_ready);

        assert!(finish_terminal_output_stream(&mut parser, &mut grid));
        assert!(grid.presentation_ready);
        let text = grid
            .lines
            .iter()
            .flat_map(|line| line.iter().map(|cell| cell.c))
            .collect::<String>();
        assert!(text.contains("RRiter terminal exited before producing output"));

        let lines = grid.lines.clone();
        assert!(!finish_terminal_output_stream(&mut parser, &mut grid));
        assert!(grid.lines == lines);
    }

    #[test]
    fn no_osc_title_tracks_shell_child_return_and_dynamic_cwd() {
        let home = Path::new("/home/reyan");
        let fish = process_snapshot("fish", "/home/reyan", &["fish"]);
        let sleep = process_snapshot("sleep", "/home/reyan", &["sleep", "10"]);
        let sleep_bin = process_snapshot("sleep", "/home/reyan/bin", &["sleep", "10"]);
        let mut state = TerminalTitleState::new("~ : fish".to_string());

        state.observe_transition(
            Some(terminal_title_for_snapshot(
                &fish,
                Some(home),
                Some(home),
                "fish",
            )),
            false,
        );
        assert_eq!(resolved_title(&state), "~ : fish");

        state.observe_unchanged();
        state.observe_transition(
            Some(terminal_title_for_snapshot(
                &sleep,
                Some(home),
                Some(home),
                "fish",
            )),
            false,
        );
        assert_eq!(resolved_title(&state), "~ : sleep");

        state.observe_unchanged();
        state.observe_transition(
            Some(terminal_title_for_snapshot(
                &fish,
                Some(home),
                Some(home),
                "fish",
            )),
            false,
        );
        assert_eq!(resolved_title(&state), "~ : fish");

        state.observe_transition(
            Some(terminal_title_for_snapshot(
                &sleep_bin,
                Some(home),
                Some(home),
                "fish",
            )),
            false,
        );
        assert_eq!(resolved_title(&state), "bin : sleep");
    }

    #[test]
    fn no_osc_ssh_title_uses_process_arguments_not_terminal_text() {
        let home = Path::new("/home/reyan");
        let direct = process_snapshot(
            "ssh",
            "/home/reyan",
            &["ssh", "reyan@89.169.37.107"],
        );
        assert_eq!(
            terminal_title_for_snapshot(&direct, Some(home), Some(home), "fish"),
            "(reyan) 89.169.37.107"
        );

        let login_option = process_snapshot(
            "ssh",
            "/home/reyan",
            &["ssh", "-p", "2222", "-i", "/tmp/key", "-l", "reyan", "89.169.37.107"],
        );
        assert_eq!(
            terminal_title_for_snapshot(&login_option, Some(home), Some(home), "fish"),
            "(reyan) 89.169.37.107"
        );

        let no_user = process_snapshot("ssh", "/home/reyan", &["ssh", "-p", "22", "host"]);
        assert_eq!(
            terminal_title_for_snapshot(&no_user, Some(home), Some(home), "fish"),
            "host"
        );
    }

    #[test]
    fn ssh_parser_skips_option_values_and_supports_attached_login_user() {
        let args = [
            "-F",
            "/tmp/config",
            "-o",
            "ProxyJump=bastion",
            "-lreyan",
            "server.example",
            "uptime",
        ]
        .map(OsString::from);
        assert_eq!(
            parse_ssh_destination(&args),
            Some(SshDestination {
                user: Some("reyan".to_string()),
                host: "server.example".to_string(),
            })
        );
    }

    #[test]
    fn detected_process_title_has_priority_over_programmed_osc() {
        let home = Path::new("/home/reyan");
        let cwd = Path::new("/home/reyan/projects/car-wash-api");
        let htop = process_snapshot("htop", "/home/reyan/projects/car-wash-api", &["htop"]);
        let mut state = TerminalTitleState::new("car-wash-api : fish".to_string());
        state.observe_transition(
            Some(terminal_title_for_snapshot(&htop, Some(cwd), Some(home), "fish")),
            false,
        );
        state.set_programmed("~/projects/car-wash-api: htop - htop".to_string());
        assert_eq!(resolved_title(&state), "car-wash-api : htop");
    }

    #[test]
    fn detected_ssh_title_has_priority_over_wrapper_osc() {
        let cwd = Path::new("/home/reyan/projects/car-wash-api");
        let ssh = process_snapshot(
            "ssh",
            "/home/reyan/projects/car-wash-api",
            &["ssh", "reyan@89.169.37.107"],
        );
        let mut state = TerminalTitleState::new("car-wash-api : fish".to_string());
        state.observe_transition(
            Some(terminal_title_for_snapshot(
                &ssh,
                Some(cwd),
                Some(Path::new("/home/reyan")),
                "fish",
            )),
            false,
        );
        state.set_programmed("~/projects/car-wash-api: ssh_prod - ssh_prod".to_string());
        assert_eq!(resolved_title(&state), "(reyan) 89.169.37.107");
    }

    #[test]
    fn programmed_title_remains_fallback_when_detected_metadata_is_absent() {
        let mut state = TerminalTitleState::new("~ : fish".to_string());
        state.set_programmed("custom title".to_string());
        assert_eq!(resolved_title(&state), "custom title");
    }

    #[test]
    fn recent_programmed_title_cannot_override_new_detected_process() {
        let mut state = TerminalTitleState::new("~ : fish".to_string());
        state.observe_transition(Some("~ : fish".to_string()), false);
        state.observe_unchanged();
        state.set_programmed("htop custom".to_string());
        state.observe_transition(Some("~ : htop".to_string()), true);
        assert_eq!(resolved_title(&state), "~ : htop");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn same_process_group_effective_pid_change_is_a_transition() {
        let wrapper = process_snapshot("fish", "/home/reyan", &["fish"]);
        let mut ssh = process_snapshot("ssh", "/home/reyan", &["ssh", "host"]);
        ssh.process_id = wrapper.process_id + 1;
        assert!(terminal_foreground_transitioned(
            Some(700),
            700,
            Some(&wrapper),
            Some(&ssh),
        ));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn same_pid_exec_change_is_a_transition() {
        let wrapper = process_snapshot("fish", "/home/reyan", &["fish"]);
        let ssh = process_snapshot("ssh", "/home/reyan", &["ssh", "host"]);
        assert_eq!(wrapper.process_id, ssh.process_id);
        assert!(terminal_foreground_transitioned(
            Some(700),
            700,
            Some(&wrapper),
            Some(&ssh),
        ));
    }

    #[test]
    fn osc_zero_bel_and_osc_two_st_update_the_shared_title_cache() {
        let (mut grid, cache) = parser_grid();
        let mut parser = Parser::new();
        parser.advance(&mut grid, b"\x1b]0;~ : htop\x07");
        assert_eq!(resolved_title(&crate::platform::lock_recover(&cache)), "~ : htop");

        parser.advance(&mut grid, b"\x1b]2;bin : sleep\x1b\\");
        assert_eq!(resolved_title(&crate::platform::lock_recover(&cache)), "bin : sleep");
    }

    #[test]
    fn programmed_title_is_utf8_control_safe_bounded_and_sequential() {
        let (mut grid, cache) = parser_grid();
        let mut parser = Parser::new();
        parser.advance(&mut grid, b"\x1b]0;\x07");
        assert_eq!(
            resolved_title(&crate::platform::lock_recover(&cache)),
            "fallback"
        );
        parser.advance(&mut grid, "\x1b]0;~ : компилятор\x07".as_bytes());
        parser.advance(&mut grid, b"\x1b]0;(reyan) 89.169.37.107\x07");
        assert_eq!(
            resolved_title(&crate::platform::lock_recover(&cache)),
            "(reyan) 89.169.37.107"
        );

        assert_eq!(terminal_programmed_title(&[b"\xff"]), None);
        assert_eq!(
            terminal_programmed_title(&[b"  safe\n\0title  "]).as_deref(),
            Some("safetitle")
        );
        let huge = "x".repeat(TERMINAL_TITLE_MAX_BYTES * 8);
        let sequence = format!("\x1b]2;{huge}\x07");
        parser.advance(&mut grid, sequence.as_bytes());
        assert_eq!(
            resolved_title(&crate::platform::lock_recover(&cache)).len(),
            TERMINAL_TITLE_MAX_BYTES
        );
    }

    #[test]
    fn windows_shell_order_prefers_powershell_and_honors_comspec() {
        let candidates = terminal_shell_candidates_with(PlatformKind::Windows, |name| {
            (name == "COMSPEC").then(|| OsString::from(r"C:\Windows\System32\cmd.exe"))
        });
        let names = candidates
            .iter()
            .map(|candidate| candidate.executable.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                OsString::from("pwsh.exe"),
                OsString::from("powershell.exe"),
                OsString::from(r"C:\Windows\System32\cmd.exe"),
                OsString::from("cmd.exe"),
            ]
        );
        assert_eq!(candidates[0].args, vec![OsString::from("-NoLogo")]);
    }

    #[test]
    fn explicit_shell_override_is_strict_and_stops_fallbacks() {
        let candidates = terminal_shell_candidates_with(PlatformKind::Linux, |name| {
            (name == "RRITER_SHELL").then(|| OsString::from("/custom/shell"))
        });
        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].strict);
        assert_eq!(candidates[0].executable, OsString::from("/custom/shell"));
    }

    #[test]
    fn macos_shell_fallback_starts_with_user_shell_then_zsh() {
        let candidates = terminal_shell_candidates_with(PlatformKind::Macos, |name| {
            (name == "SHELL").then(|| OsString::from("/opt/homebrew/bin/fish"))
        });
        assert_eq!(
            candidates[0].executable,
            OsString::from("/opt/homebrew/bin/fish")
        );
        assert_eq!(candidates[1].executable, OsString::from("/bin/zsh"));
    }

    #[test]
    fn terminal_cwd_prefers_deepest_workspace_then_file_parent() {
        let workspaces = vec![PathBuf::from("/work"), PathBuf::from("/work/project")];
        assert_eq!(
            select_terminal_working_directory(
                Some(Path::new("/work/project/src/main.rs")),
                &workspaces,
            ),
            Some(PathBuf::from("/work/project"))
        );
        assert_eq!(
            select_terminal_working_directory(Some(Path::new("/outside/main.rs")), &workspaces,),
            Some(PathBuf::from("/outside"))
        );
        assert_eq!(
            select_terminal_working_directory(None, &workspaces),
            Some(PathBuf::from("/work"))
        );
    }

    #[test]
    fn terminal_fallback_title_uses_home_marker_or_cwd_basename() {
        let home = Path::new("/home/reyan");
        assert_eq!(
            terminal_fallback_title_with_home(Some(home), Some(home), "fish"),
            "~ : fish"
        );
        assert_eq!(
            terminal_fallback_title_with_home(
                Some(Path::new("/home/reyan/bin")),
                Some(home),
                "fish",
            ),
            "bin : fish"
        );
        assert_eq!(
            terminal_fallback_title_with_home(
                Some(Path::new("/home/reyan/bin")),
                Some(home),
                "bash",
            ),
            "bin : bash"
        );
        assert_eq!(
            terminal_fallback_title_with_home(Some(Path::new("/")), Some(home), "bash"),
            "/ : bash"
        );
    }

    #[test]
    fn terminal_title_hides_windows_executable_suffix() {
        assert_eq!(
            terminal_shell_title(Path::new(r"C:\Tools\pwsh.exe")),
            "pwsh"
        );
        assert_eq!(terminal_shell_title(Path::new("/bin/zsh")), "zsh");
    }
}
