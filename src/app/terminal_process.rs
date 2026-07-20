use super::terminal::TermGrid;
use crate::platform::{self, PlatformKind, ProcessTree};
use alacritty_terminal::vte::Parser;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const INITIAL_COLS: u16 = 200;
const INITIAL_ROWS: u16 = 60;
const TERMINAL_SHUTDOWN_GRACE: Duration = Duration::from_millis(500);

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
    finished: bool,
}

impl TerminalProcess {
    pub(crate) fn spawn(
        grid: Arc<Mutex<TermGrid>>,
        window: Option<Arc<winit::window::Window>>,
        cwd: Option<&Path>,
    ) -> io::Result<(Self, TerminalShellSpec)> {
        let shell = resolve_terminal_shell()?;
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
        if let Err(error) = install_terminal_io_threads(&grid, reader, writer.clone(), window) {
            let _ = tree.terminate_forcefully();
            let _ = child.kill();
            let _ = child.wait();
            tree.finish_after_owner_exit();
            return Err(error);
        }

        Ok((
            Self {
                writer,
                master_pty: Arc::new(Mutex::new(pair.master)),
                child,
                tree,
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
            self.tree.finish_after_owner_exit();
        }
        Ok(self.finished)
    }

    pub(crate) fn shutdown(&mut self) {
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

fn install_terminal_io_threads(
    grid: &Arc<Mutex<TermGrid>>,
    reader: Box<dyn Read + Send>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    window: Option<Arc<winit::window::Window>>,
) -> io::Result<()> {
    let (reply_tx, reply_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();

    let parser_grid = grid.clone();
    crate::platform::spawn_named("rriter-terminal-parser", move || {
        let mut parser = Parser::new();
        while let Ok(first) = rx.recv() {
            let mut chunks = vec![first];
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
            for chunk in &chunks {
                parser.advance(&mut *grid, chunk);
            }
            grid.dirty = true;
            drop(grid);
            if let Some(window) = window.as_ref() {
                window.request_redraw();
            }
        }
    })
    .map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("failed to spawn terminal parser: {error}"),
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
    })
    .map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("failed to spawn terminal reader: {error}"),
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn terminal_title_hides_windows_executable_suffix() {
        assert_eq!(
            terminal_shell_title(Path::new(r"C:\Tools\pwsh.exe")),
            "pwsh"
        );
        assert_eq!(terminal_shell_title(Path::new("/bin/zsh")), "zsh");
    }
}
