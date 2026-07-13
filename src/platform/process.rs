use super::{CURRENT_PLATFORM, PlatformKind};
use std::ffi::{OsStr, OsString};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(10);
const DROP_GRACE_PERIOD: Duration = Duration::from_millis(200);

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

    pub fn terminate_gracefully(&self) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        #[cfg(unix)]
        {
            return signal_process_group(self.process_group, libc::SIGTERM);
        }

        #[cfg(windows)]
        {
            return terminate_windows_job(self.job, 1);
        }

        #[cfg(not(any(unix, windows)))]
        {
            let _ = self.process_id;
            Ok(())
        }
    }

    pub fn terminate_forcefully(&self) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        #[cfg(unix)]
        {
            return signal_process_group(self.process_group, libc::SIGKILL);
        }

        #[cfg(windows)]
        {
            return terminate_windows_job(self.job, 1);
        }

        #[cfg(not(any(unix, windows)))]
        {
            let _ = self.process_id;
            Ok(())
        }
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
    let result = unsafe {
        windows_sys::Win32::System::JobObjects::TerminateJobObject(job, exit_code)
    };
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
    resolve_executable(program)
}

pub fn command_for_tool(program: &OsStr, override_env: &str) -> io::Result<Command> {
    resolve_tool_executable(program, override_env)
        .map(Command::new)
        .ok_or_else(|| {
            let override_hint = std::env::var_os(override_env)
                .filter(|value| !value.is_empty())
                .map(|value| format!(" (configured by {override_env}={})", value.to_string_lossy()))
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
        .unwrap_or_else(|| [".COM", ".EXE", ".BAT", ".CMD"].map(OsString::from).to_vec())
}

pub fn run_command_output(command: &mut Command, timeout: Duration) -> io::Result<Output> {
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
    let stdout_reader = thread::spawn(move || read_pipe(stdout));
    let stderr_reader = thread::spawn(move || read_pipe(stderr));

    let status = match child.wait_timeout(timeout)? {
        Some(status) => status,
        None => {
            let _ = child.terminate(DROP_GRACE_PERIOD);
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("process exceeded timeout of {} ms", timeout.as_millis()),
            ));
        }
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

fn join_pipe_reader(
    handle: thread::JoinHandle<io::Result<Vec<u8>>>,
) -> io::Result<Vec<u8>> {
    handle
        .join()
        .map_err(|_| io::Error::other("process output reader panicked"))?
}
