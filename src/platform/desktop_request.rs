// Requests that leave the app for the desktop: native file pickers, the file
// manager and the browser. Headless intercepts every one of them and records
// it in the owning `App`'s `ExternalRequestLog` instead of touching the desktop.

use super::{HeadlessPolicy, headless_policy, reap_unit_thread, spawn_named};
#[cfg(target_os = "macos")]
use super::macos;
#[cfg(windows)]
use super::process;
#[cfg(windows)]
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
#[cfg(not(target_os = "macos"))]
use std::process::Command;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExternalRequest {
    PickFile,
    PickFiles,
    PickFolder,
    SaveFile,
    OpenUrl(String),
    RevealPath(PathBuf),
    OpenPath(PathBuf),
}

/// Sending half of one `ExternalRequestLog`; cloned into picker worker threads.
#[derive(Clone)]
pub struct ExternalRequestSink {
    sender: std::sync::mpsc::Sender<ExternalRequest>,
    picker_answers: Arc<Mutex<VecDeque<Vec<PathBuf>>>>,
    /// Set for a headless `App`: its requests never reach the desktop, even in
    /// cargo tests, where the process-wide headless policy is not installed.
    intercept: bool,
}

/// The requests an `App` would have sent to the desktop, recorded in headless
/// only. One per `App`, so parallel sessions never see each other's requests.
pub struct ExternalRequestLog {
    sink: ExternalRequestSink,
    rx: std::sync::mpsc::Receiver<ExternalRequest>,
    last: Option<ExternalRequest>,
}

impl Default for ExternalRequestLog {
    fn default() -> Self {
        Self::new(false)
    }
}

impl ExternalRequestLog {
    /// `intercept`: record every request of this `App` instead of touching the desktop.
    pub fn new(intercept: bool) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let picker_answers = Arc::new(Mutex::new(VecDeque::new()));
        Self {
            sink: ExternalRequestSink { sender: tx, picker_answers, intercept },
            rx,
            last: None,
        }
    }

    pub fn sink(&self) -> &ExternalRequestSink {
        &self.sink
    }

    /// Queues the paths returned by the next intercepted native picker call.
    pub fn queue_picker_answer(&self, paths: Vec<PathBuf>) {
        if let Ok(mut answers) = self.sink.picker_answers.lock() {
            answers.push_back(paths);
        }
    }

    /// Returns the last intercepted request and clears it (read by `dump`).
    pub fn take(&mut self) -> Option<ExternalRequest> {
        while let Ok(request) = self.rx.try_recv() {
            self.last = Some(request);
        }
        self.last.take()
    }
}

pub(crate) fn take_picker_answer(sink: &ExternalRequestSink) -> Option<Vec<PathBuf>> {
    sink.picker_answers.lock().ok()?.pop_front()
}

fn take_picker_path(sink: &ExternalRequestSink) -> Option<PathBuf> {
    take_picker_answer(sink)?.into_iter().next()
}

/// `true` when the caller must not touch the desktop: headless records the
/// request instead of opening a picker, browser, or file manager.
pub(crate) fn intercept_external(
    policy: Option<HeadlessPolicy>,
    sink: &ExternalRequestSink,
    request: ExternalRequest,
) -> bool {
    if policy.is_none() && !sink.intercept {
        return false;
    }
    // The log lives as long as its `App`; a send after it is gone has no reader.
    let _ = sink.sender.send(request);
    true
}

/// `true` when this sink's requests are recorded instead of reaching the desktop,
/// so callers may resolve and record synchronously.
pub(crate) fn external_requests_intercepted(sink: &ExternalRequestSink) -> bool {
    headless_policy().is_some() || sink.intercept
}

pub fn pick_file(requests: &ExternalRequestSink, title: &str) -> Option<PathBuf> {
    if intercept_external(headless_policy(), requests, ExternalRequest::PickFile) {
        return take_picker_path(requests);
    }
    rfd::FileDialog::new().set_title(title).pick_file()
}

pub fn pick_file_with_filter(
    requests: &ExternalRequestSink,
    title: &str,
    filter_name: &str,
    extensions: &[&str],
) -> Option<PathBuf> {
    if intercept_external(headless_policy(), requests, ExternalRequest::PickFile) {
        return take_picker_path(requests);
    }
    rfd::FileDialog::new()
        .set_title(title)
        .add_filter(filter_name, extensions)
        .pick_file()
}

pub fn pick_files(requests: &ExternalRequestSink, title: &str) -> Vec<PathBuf> {
    if intercept_external(headless_policy(), requests, ExternalRequest::PickFiles) {
        return take_picker_answer(requests).unwrap_or_default();
    }
    rfd::FileDialog::new()
        .set_title(title)
        .pick_files()
        .unwrap_or_default()
}

pub fn pick_folder(requests: &ExternalRequestSink, title: &str) -> Option<PathBuf> {
    if intercept_external(headless_policy(), requests, ExternalRequest::PickFolder) {
        return take_picker_path(requests);
    }
    rfd::FileDialog::new().set_title(title).pick_folder()
}

pub fn save_file(requests: &ExternalRequestSink, title: &str, file_name: &str) -> Option<PathBuf> {
    if intercept_external(headless_policy(), requests, ExternalRequest::SaveFile) {
        return take_picker_path(requests);
    }
    rfd::FileDialog::new()
        .set_title(title)
        .set_file_name(file_name)
        .save_file()
}

pub fn save_file_with_filter(
    requests: &ExternalRequestSink,
    title: &str,
    file_name: &str,
    filter_name: &str,
    extensions: &[&str],
) -> Option<PathBuf> {
    if intercept_external(headless_policy(), requests, ExternalRequest::SaveFile) {
        return take_picker_path(requests);
    }
    rfd::FileDialog::new()
        .set_title(title)
        .set_file_name(file_name)
        .add_filter(filter_name, extensions)
        .save_file()
}

pub fn reveal_path(requests: &ExternalRequestSink, path: &Path) -> io::Result<Child> {
    if intercept_external(headless_policy(), requests, ExternalRequest::RevealPath(path.to_path_buf())) {
        return Err(io::Error::new(io::ErrorKind::Unsupported, "disabled in headless mode"));
    }
    spawn_reveal_path(path, false)
}

/// Starts the file manager on `path` without the headless check. `always_select`
/// makes macOS select the path even when it is a launchable bundle.
pub(crate) fn spawn_reveal_path(path: &Path, always_select: bool) -> io::Result<Child> {
    let _ = always_select;
    #[cfg(windows)]
    {
        let mut command = Command::new("explorer.exe");
        if path.is_dir() {
            command.arg(path);
        } else {
            command.arg("/select,").arg(path);
        }
        process::configure_background_command(&mut command);
        return command.spawn();
    }
    #[cfg(target_os = "macos")]
    {
        return macos::reveal_path(path, always_select);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let target = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(path)
        };
        return Command::new("xdg-open").arg(target).spawn();
    }
    #[allow(unreachable_code)]
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "revealing files is not supported on this platform",
    ))
}

/// Opens `path` with the platform's default handler. The caller passes a
/// canonical path; this function does no filtering of what it opens.
pub fn open_path(requests: &ExternalRequestSink, path: &Path) -> io::Result<()> {
    if intercept_external(headless_policy(), requests, ExternalRequest::OpenPath(path.to_path_buf())) {
        return Ok(());
    }
    match spawn_open_path(path)? {
        Some(child) => reap_child(child),
        None => Ok(()),
    }
}

/// Starts the platform's default handler for `path`. `None` when the handler
/// returns no child process (Windows ShellExecute).
pub(crate) fn spawn_open_path(path: &Path) -> io::Result<Option<Child>> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let operation = OsStr::new("open")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let target = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                operation.as_ptr(),
                target.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        if result as isize > 32 {
            return Ok(None);
        }
        return Err(io::Error::other(format!(
            "ShellExecuteW failed with code {}",
            result as isize
        )));
    }
    #[cfg(target_os = "macos")]
    {
        return macos::open_path(path).map(Some);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        return Command::new("xdg-open").arg(path).spawn().map(Some);
    }
    #[allow(unreachable_code)]
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "opening files is not supported on this platform",
    ))
}

/// Waits for one started handler on its own named thread so the child is reaped.
fn reap_child(mut child: Child) -> io::Result<()> {
    let worker = spawn_named("rriter-open-path-wait", move || {
        let _ = child.wait();
    })?;
    reap_unit_thread(worker);
    Ok(())
}

pub fn open_url(requests: &ExternalRequestSink, url: &str) -> io::Result<()> {
    if intercept_external(headless_policy(), requests, ExternalRequest::OpenUrl(url.to_string())) {
        return Ok(());
    }
    let parsed =
        url::Url::parse(url).map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    if !matches!(parsed.scheme(), "http" | "https" | "mailto") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "only http, https and mailto URLs may be opened",
        ));
    }

    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let operation = OsStr::new("open")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let target = OsStr::new(parsed.as_str())
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                operation.as_ptr(),
                target.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        if result as isize > 32 {
            return Ok(());
        }
        return Err(io::Error::other(format!(
            "ShellExecuteW failed with code {}",
            result as isize
        )));
    }
    #[cfg(target_os = "macos")]
    {
        return macos::open_url(parsed.as_str());
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        return Command::new("xdg-open")
            .arg(parsed.as_str())
            .spawn()
            .map(|_| ());
    }
    #[allow(unreachable_code)]
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "opening URLs is not supported on this platform",
    ))
}
