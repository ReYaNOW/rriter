use super::{CURRENT_PLATFORM, PlatformKind};
use std::ffi::{OsStr, OsString};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{
    Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Output, Stdio,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(10);
const DROP_GRACE_PERIOD: Duration = Duration::from_millis(200);
const PROCESS_OUTPUT_CHANNEL_CAPACITY: usize = 256;
const PROCESS_OUTPUT_LINE_LIMIT: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessOutputStream {
    Stdout,
    Stderr,
}

/// A child process whose complete descendant tree is owned by RRiter.
///
/// Unix children are placed in a dedicated process group before `exec`. Windows
/// children are assigned to a Job Object configured with
/// `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`. Dropping this handle therefore cannot
/// silently leave language servers, Python workers or their descendants alive.
pub struct ManagedChild {
    child: Child,
    tree: ProcessTree,
    finished: bool,
}

impl ManagedChild {
    pub fn spawn(command: &mut Command) -> io::Result<Self> {
        configure_managed_command(command);
        let mut child = command.spawn()?;
        let tree = match ProcessTree::attach_std_child(&child) {
            Ok(tree) => tree,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        Ok(Self {
            child,
            tree,
            finished: false,
        })
    }

    pub fn take_stdin(&mut self) -> Option<ChildStdin> {
        self.child.stdin.take()
    }

    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.stdout.take()
    }

    pub fn take_stderr(&mut self) -> Option<ChildStderr> {
        self.child.stderr.take()
    }

    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        let status = self.child.try_wait()?;
        if status.is_some() {
            self.finished = true;
            self.tree.finish_after_owner_exit();
        }
        Ok(status)
    }

    pub fn wait_timeout(&mut self, timeout: Duration) -> io::Result<Option<ExitStatus>> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.try_wait()? {
                return Ok(Some(status));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            thread::sleep(
                PROCESS_POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())),
            );
        }
    }

    pub fn terminate(&mut self, grace_period: Duration) -> io::Result<()> {
        if self.try_wait()?.is_some() {
            return Ok(());
        }

        self.tree.terminate_gracefully()?;
        if self.wait_timeout(grace_period)?.is_some() {
            return Ok(());
        }

        self.tree.terminate_forcefully()?;
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.finished = true;
        self.tree.finish_after_owner_exit();
        Ok(())
    }
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.terminate(DROP_GRACE_PERIOD);
        }
    }
}

/// Owns the OS primitive used to terminate a complete process tree.
/// This is also used for processes spawned by `portable-pty`.
pub struct ProcessTree {
    #[cfg(unix)]
    process_group: i32,
    #[cfg(windows)]
    job: windows_sys::Win32::Foundation::HANDLE,
    #[cfg(not(any(unix, windows)))]
    process_id: u32,
    active: bool,
}

// A Windows HANDLE is an owned kernel handle. Moving the owner to another
// thread is safe; all access is serialized by the containing process handle.
#[cfg(windows)]
unsafe impl Send for ProcessTree {}

impl ProcessTree {
    fn attach_std_child(child: &Child) -> io::Result<Self> {
        Self::attach_process_id(child.id())
    }

    pub fn attach_process_id(process_id: u32) -> io::Result<Self> {
        #[cfg(unix)]
        {
            let process_group = i32::try_from(process_id)
                .map_err(|_| io::Error::other("process id does not fit in pid_t"))?;
            return Ok(Self {
                process_group,
                active: true,
            });
        }

        #[cfg(windows)]
        {
            return windows_process_tree(process_id);
        }

        #[cfg(not(any(unix, windows)))]
        {
            Ok(Self {
                process_id,
                active: true,
            })
        }
    }

    fn terminate(&self, force: bool) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        #[cfg(unix)]
        {
            let signal = if force { libc::SIGKILL } else { libc::SIGTERM };
            return signal_process_group(self.process_group, signal);
        }

        #[cfg(windows)]
        {
            let _ = force;
            return terminate_windows_job(self.job, 1);
        }

        #[cfg(not(any(unix, windows)))]
        {
            let _ = (self.process_id, force);
            Ok(())
        }
    }

    pub fn terminate_gracefully(&self) -> io::Result<()> {
        self.terminate(false)
    }

    pub fn terminate_forcefully(&self) -> io::Result<()> {
        self.terminate(true)
    }

    /// The direct process has exited. Kill any descendants that still hold
    /// inherited pipes or other resources, then disarm PID-based cleanup so a
    /// later drop cannot target a recycled Unix process-group id.
    pub(crate) fn finish_after_owner_exit(&mut self) {
        if !self.active {
            return;
        }
        if self.terminate_forcefully().is_ok() {
            self.active = false;
        }
    }
}

#[cfg(windows)]
impl Drop for ProcessTree {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.job);
        }
    }
}

#[cfg(unix)]
impl Drop for ProcessTree {
    fn drop(&mut self) {
        if self.active {
            // The direct child may have exited while grandchildren are still
            // alive. Clear the dedicated process group while the id is still
            // owned by this handle.
            let _ = signal_process_group(self.process_group, libc::SIGKILL);
        }
    }
}

#[cfg(unix)]
fn signal_process_group(process_group: i32, signal: i32) -> io::Result<()> {
    let result = unsafe { libc::kill(-process_group, signal) };
    if result == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        Ok(())
    } else {
        Err(error)
    }
}

#[cfg(windows)]
fn windows_process_tree(process_id: u32) -> io::Result<ProcessTree> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
    };

    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() || job == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }

        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast(),
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) == 0
        {
            let error = io::Error::last_os_error();
            CloseHandle(job);
            return Err(error);
        }

        let process = OpenProcess(
            PROCESS_SET_QUOTA | PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            process_id,
        );
        if process.is_null() || process == INVALID_HANDLE_VALUE {
            let error = io::Error::last_os_error();
            CloseHandle(job);
            return Err(error);
        }

        let assigned = AssignProcessToJobObject(job, process);
        let assign_error = if assigned == 0 {
            Some(io::Error::last_os_error())
        } else {
            None
        };
        CloseHandle(process);
        if let Some(error) = assign_error {
            CloseHandle(job);
            return Err(error);
        }

        Ok(ProcessTree { job, active: true })
    }
}

#[cfg(windows)]
fn terminate_windows_job(
    job: windows_sys::Win32::Foundation::HANDLE,
    exit_code: u32,
) -> io::Result<()> {
    let result =
        unsafe { windows_sys::Win32::System::JobObjects::TerminateJobObject(job, exit_code) };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
pub fn configure_background_command(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::System::Threading::{CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW};
    command.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
}

#[cfg(not(windows))]
pub fn configure_background_command(_command: &mut Command) {}

fn configure_managed_command(command: &mut Command) {
    configure_background_command(command);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
}

#[cfg(all(test, unix))]
pub fn command_for(program: impl AsRef<OsStr>) -> io::Result<Command> {
    let program = program.as_ref();
    resolve_executable(program)
        .map(Command::new)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("executable not found: {}", program.to_string_lossy()),
            )
        })
}

/// Resolves a tool path, honoring an explicit environment override first.
/// Settings UI can pass the same override semantics later without changing
/// process ownership or restart behavior.
pub fn resolve_tool_executable(program: &OsStr, override_env: &str) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(override_env).filter(|value| !value.is_empty()) {
        return resolve_executable(&path);
    }
    if let Some(path) = super::configured_tool_path_for_env(override_env) {
        return resolve_executable(path.as_os_str());
    }
    resolve_executable(program)
}

pub fn command_for_executable(program: &Path) -> io::Result<Command> {
    resolve_executable(program.as_os_str())
        .map(Command::new)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("executable not found: {}", program.display()),
            )
        })
}

pub fn command_for_tool(program: &OsStr, override_env: &str) -> io::Result<Command> {
    resolve_tool_executable(program, override_env)
        .map(Command::new)
        .ok_or_else(|| {
            let override_hint = std::env::var_os(override_env)
                .filter(|value| !value.is_empty())
                .map(|value| {
                    format!(
                        " (configured by {override_env}={})",
                        value.to_string_lossy()
                    )
                })
                .or_else(|| {
                    super::configured_tool_path_for_env(override_env).map(|value| {
                        format!(" (configured in RRiter settings: {})", value.display())
                    })
                })
                .unwrap_or_default();
            io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "executable not found: {}{override_hint}",
                    program.to_string_lossy()
                ),
            )
        })
}

pub fn resolve_executable(program: &OsStr) -> Option<PathBuf> {
    resolve_executable_with(
        program,
        std::env::var_os("PATH").as_deref(),
        std::env::var_os("PATHEXT").as_deref(),
        CURRENT_PLATFORM,
    )
}

pub(crate) fn resolve_executable_with(
    program: &OsStr,
    path: Option<&OsStr>,
    path_ext: Option<&OsStr>,
    platform: PlatformKind,
) -> Option<PathBuf> {
    let candidate = Path::new(program);
    if candidate.components().count() > 1 || candidate.is_absolute() {
        return candidate.is_file().then(|| candidate.to_path_buf());
    }

    let path = path?;
    let extensions = executable_extensions_with(path_ext, platform);
    for directory in std::env::split_paths(path) {
        let direct = directory.join(candidate);
        if direct.is_file() {
            return Some(direct);
        }
        if platform == PlatformKind::Windows && candidate.extension().is_none() {
            for extension in &extensions {
                let mut file_name = candidate.as_os_str().to_os_string();
                file_name.push(extension);
                let file = directory.join(file_name);
                if file.is_file() {
                    return Some(file);
                }
            }
        }
    }
    None
}

fn executable_extensions_with(path_ext: Option<&OsStr>, platform: PlatformKind) -> Vec<OsString> {
    if platform != PlatformKind::Windows {
        return Vec::new();
    }
    path_ext
        .map(|value| {
            value
                .to_string_lossy()
                .split(';')
                .filter(|extension| !extension.is_empty())
                .map(|extension| {
                    if extension.starts_with('.') {
                        OsString::from(extension)
                    } else {
                        OsString::from(format!(".{extension}"))
                    }
                })
                .collect::<Vec<_>>()
        })
        .filter(|extensions| !extensions.is_empty())
        .unwrap_or_else(|| {
            [".COM", ".EXE", ".BAT", ".CMD"]
                .map(OsString::from)
                .to_vec()
        })
}

pub fn run_command_output(command: &mut Command, timeout: Duration) -> io::Result<Output> {
    run_command_output_inner(command, timeout, None)
}

pub fn run_command_output_cancelable(
    command: &mut Command,
    timeout: Duration,
    cancel: &AtomicBool,
) -> io::Result<Output> {
    run_command_output_inner(command, timeout, Some(cancel))
}

/// Runs a managed process while forwarding complete output lines as soon as
/// they are available. Cancellation and timeout terminate the complete process
/// tree, not only the direct child.
pub fn run_command_streaming_cancelable<F>(
    command: &mut Command,
    timeout: Duration,
    cancel: &AtomicBool,
    mut on_line: F,
) -> io::Result<ExitStatus>
where
    F: FnMut(ProcessOutputStream, String),
{
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = ManagedChild::spawn(command)?;
    let stdout = child
        .take_stdout()
        .ok_or_else(|| io::Error::other("child stdout was not piped"))?;
    let stderr = child
        .take_stderr()
        .ok_or_else(|| io::Error::other("child stderr was not piped"))?;
    let (tx, rx) = mpsc::sync_channel(PROCESS_OUTPUT_CHANNEL_CAPACITY);
    let stdout_reader = spawn_line_reader(stdout, ProcessOutputStream::Stdout, tx.clone())?;
    let stderr_reader = match spawn_line_reader(stderr, ProcessOutputStream::Stderr, tx) {
        Ok(reader) => reader,
        Err(error) => {
            let _ = child.terminate(DROP_GRACE_PERIOD);
            let _ = join_line_reader(stdout_reader);
            return Err(error);
        }
    };
    let deadline = Instant::now() + timeout;

    let result = loop {
        drain_process_lines(&rx, &mut on_line);
        if cancel.load(Ordering::Acquire) {
            let _ = child.terminate(DROP_GRACE_PERIOD);
            break Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "process was cancelled",
            ));
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.terminate(DROP_GRACE_PERIOD);
                break Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("process exceeded timeout of {} ms", timeout.as_millis()),
                ));
            }
            Ok(None) => thread::sleep(
                PROCESS_POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())),
            ),
            Err(error) => {
                let _ = child.terminate(DROP_GRACE_PERIOD);
                break Err(error);
            }
        }
    };

    while !stdout_reader.is_finished() || !stderr_reader.is_finished() {
        drain_process_lines(&rx, &mut on_line);
        thread::sleep(Duration::from_millis(1));
    }
    drain_process_lines(&rx, &mut on_line);
    let stdout_result = join_line_reader(stdout_reader);
    let stderr_result = join_line_reader(stderr_reader);
    stdout_result?;
    stderr_result?;
    result
}

fn run_command_output_inner(
    command: &mut Command,
    timeout: Duration,
    cancel: Option<&AtomicBool>,
) -> io::Result<Output> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = ManagedChild::spawn(command)?;
    let stdout = child
        .take_stdout()
        .ok_or_else(|| io::Error::other("child stdout was not piped"))?;
    let stderr = child
        .take_stderr()
        .ok_or_else(|| io::Error::other("child stderr was not piped"))?;
    let stdout_reader = super::spawn_named("rriter-process-stdout", move || read_pipe(stdout))?;
    let stderr_reader = match super::spawn_named("rriter-process-stderr", move || read_pipe(stderr))
    {
        Ok(reader) => reader,
        Err(error) => {
            let _ = child.terminate(DROP_GRACE_PERIOD);
            let _ = join_pipe_reader(stdout_reader);
            return Err(error);
        }
    };

    let deadline = Instant::now() + timeout;
    let status = loop {
        if cancel.is_some_and(|cancel| cancel.load(Ordering::Acquire)) {
            let _ = child.terminate(DROP_GRACE_PERIOD);
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "process was cancelled",
            ));
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.terminate(DROP_GRACE_PERIOD);
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("process exceeded timeout of {} ms", timeout.as_millis()),
            ));
        }
        thread::sleep(
            PROCESS_POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())),
        );
    };

    let stdout = join_pipe_reader(stdout_reader)?;
    let stderr = join_pipe_reader(stderr_reader)?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn read_pipe(mut pipe: impl Read) -> io::Result<Vec<u8>> {
    let mut data = Vec::new();
    pipe.read_to_end(&mut data)?;
    Ok(data)
}

fn join_pipe_reader(handle: thread::JoinHandle<io::Result<Vec<u8>>>) -> io::Result<Vec<u8>> {
    handle
        .join()
        .map_err(|_| io::Error::other("process output reader panicked"))?
}

fn spawn_line_reader<R>(
    mut stream: R,
    output_stream: ProcessOutputStream,
    tx: mpsc::SyncSender<(ProcessOutputStream, String)>,
) -> io::Result<thread::JoinHandle<io::Result<()>>>
where
    R: Read + Send + 'static,
{
    super::spawn_named("rriter-process-line-reader", move || {
        let mut read_buffer = [0u8; 4096];
        let mut line = Vec::with_capacity(256);
        let mut truncated = false;
        loop {
            let read = stream.read(&mut read_buffer)?;
            if read == 0 {
                break;
            }
            for byte in &read_buffer[..read] {
                if *byte == b'\n' {
                    emit_process_line(&tx, output_stream, &mut line, truncated)?;
                    truncated = false;
                } else if line.len() < PROCESS_OUTPUT_LINE_LIMIT {
                    line.push(*byte);
                } else {
                    truncated = true;
                }
            }
        }
        if !line.is_empty() || truncated {
            emit_process_line(&tx, output_stream, &mut line, truncated)?;
        }
        Ok(())
    })
}

fn emit_process_line(
    tx: &mpsc::SyncSender<(ProcessOutputStream, String)>,
    output_stream: ProcessOutputStream,
    line: &mut Vec<u8>,
    truncated: bool,
) -> io::Result<()> {
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    let mut text = String::from_utf8_lossy(line).into_owned();
    if truncated {
        text.push_str(" … [output line truncated]");
    }
    line.clear();
    tx.send((output_stream, text))
        .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "output receiver closed"))
}

fn drain_process_lines<F>(rx: &mpsc::Receiver<(ProcessOutputStream, String)>, on_line: &mut F)
where
    F: FnMut(ProcessOutputStream, String),
{
    while let Ok((stream, line)) = rx.try_recv() {
        on_line(stream, line);
    }
}

fn join_line_reader(handle: thread::JoinHandle<io::Result<()>>) -> io::Result<()> {
    handle
        .join()
        .map_err(|_| io::Error::other("process line reader panicked"))?
}
