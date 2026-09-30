use super::{TextFileFormat, encode_text};
#[cfg(any(windows, target_os = "macos", test))]
use super::{atomic_write, decode_persisted_path, encode_persisted_path};
#[cfg(any(windows, target_os = "macos", test))]
use std::ffi::OsStr;
use std::ffi::OsString;
#[cfg(any(windows, target_os = "macos", test))]
use std::fs;
use std::io;
#[cfg(target_os = "linux")]
use std::io::Write;
use std::path::Path;
#[cfg(any(windows, target_os = "macos", test))]
use std::path::PathBuf;
#[cfg(target_os = "linux")]
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "linux")]
use std::time::Duration;

#[cfg(any(windows, target_os = "macos", test))]
#[cfg_attr(test, allow(dead_code))]
const ELEVATED_SAVE_FLAG: &str = "--rriter-elevated-save";
#[cfg(any(windows, target_os = "macos", test))]
const ELEVATED_SAVE_SCHEMA: u32 = 1;
#[cfg(any(windows, target_os = "macos", test))]
const REQUEST_FILE_NAME: &str = "request.json";
#[cfg(any(windows, target_os = "macos", test))]
const PAYLOAD_FILE_NAME: &str = "payload.bin";
#[cfg(any(windows, target_os = "macos", test))]
const RESULT_FILE_NAME: &str = "result.txt";
#[cfg(any(windows, target_os = "macos", test))]
const REQUEST_DIR_PREFIX: &str = ".rriter-elevated-save-";

#[cfg(any(windows, target_os = "macos", test))]
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct ElevatedSaveRequest {
    schema_version: u32,
    target: String,
}

#[cfg(any(windows, target_os = "macos", test))]
#[cfg_attr(test, allow(dead_code))]
#[derive(Debug)]
struct RequestFiles {
    directory: PathBuf,
    request: PathBuf,
    payload: PathBuf,
    result: PathBuf,
}

#[cfg(any(windows, target_os = "macos", test))]
#[cfg_attr(test, allow(dead_code))]
impl RequestFiles {
    fn create(bytes: &[u8]) -> io::Result<Self> {
        let root = super::state_dir().join("elevated-save");
        fs::create_dir_all(&root)?;
        let directory = root.join(format!(
            "{REQUEST_DIR_PREFIX}{}",
            super::next_operation_id()
        ));
        fs::create_dir(&directory)?;
        let files = Self::from_directory(directory);
        if let Err(error) = atomic_write(&files.payload, bytes) {
            files.cleanup();
            return Err(error);
        }
        Ok(files)
    }

    fn from_request_path(request: &Path) -> io::Result<Self> {
        if request.file_name() != Some(OsStr::new(REQUEST_FILE_NAME)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "elevated-save request must be named request.json",
            ));
        }
        let directory = request.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "elevated-save request has no parent directory",
            )
        })?;
        let valid_directory = directory
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| {
                name.starts_with(REQUEST_DIR_PREFIX)
                    && name[REQUEST_DIR_PREFIX.len()..]
                        .chars()
                        .all(|ch| ch.is_ascii_digit() || ch == '-')
            });
        if !valid_directory {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "elevated-save request directory is invalid",
            ));
        }
        Ok(Self::from_directory(directory.to_path_buf()))
    }

    fn from_directory(directory: PathBuf) -> Self {
        Self {
            request: directory.join(REQUEST_FILE_NAME),
            payload: directory.join(PAYLOAD_FILE_NAME),
            result: directory.join(RESULT_FILE_NAME),
            directory,
        }
    }

    fn write_request(&self, target: &Path) -> io::Result<()> {
        let request = ElevatedSaveRequest {
            schema_version: ELEVATED_SAVE_SCHEMA,
            target: encode_persisted_path(target),
        };
        let bytes = serde_json::to_vec(&request)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        atomic_write(&self.request, &bytes)
    }

    fn cleanup(&self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[cfg(any(windows, target_os = "macos", test))]
fn read_request(files: &RequestFiles) -> io::Result<PathBuf> {
    let bytes = fs::read(&files.request)?;
    let request = serde_json::from_slice::<ElevatedSaveRequest>(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if request.schema_version != ELEVATED_SAVE_SCHEMA {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unsupported elevated-save request schema",
        ));
    }
    decode_persisted_path(&request.target).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "elevated-save target path is invalid",
        )
    })
}

#[cfg(any(windows, target_os = "macos", test))]
fn write_helper_result(files: &RequestFiles, outcome: &io::Result<()>) -> io::Result<()> {
    let message = match outcome {
        Ok(()) => "ok\n".to_string(),
        Err(error) => format!(
            "error\t{}\t{}\n",
            error.raw_os_error().unwrap_or_default(),
            error
        ),
    };
    fs::write(&files.result, message.as_bytes())
}

#[cfg(any(windows, target_os = "macos", test))]
fn execute_request(request_path: &Path) -> io::Result<()> {
    let files = RequestFiles::from_request_path(request_path)?;
    let target = read_request(&files)?;
    let payload = fs::read(&files.payload)?;
    atomic_write(&target, &payload)
}

#[cfg(any(windows, target_os = "macos", test))]
fn run_helper(request_path: &Path) -> i32 {
    let files = RequestFiles::from_request_path(request_path);
    let outcome = match &files {
        Ok(_) => execute_request(request_path),
        Err(error) => Err(io::Error::new(error.kind(), error.to_string())),
    };
    if let Ok(files) = &files
        && let Err(error) = write_helper_result(files, &outcome)
    {
        eprintln!("RRiter elevated save could not publish its result: {error}");
        return 1;
    }
    if let Err(error) = outcome {
        eprintln!("RRiter elevated save failed: {error}");
        1
    } else {
        0
    }
}

pub fn handle_startup_helper(args: &[OsString]) -> Option<i32> {
    // The media decoding helper runs on every platform, unlike the elevated save helper.
    if let Some(exit_code) = crate::markdown_media::run_media_helper_if_requested() {
        return Some(exit_code);
    }
    #[cfg(any(windows, target_os = "macos"))]
    {
        if args.len() == 3 && args[1] == OsStr::new(ELEVATED_SAVE_FLAG) {
            return Some(run_helper(Path::new(&args[2])));
        }
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = args;
    }
    None
}

#[cfg(any(windows, target_os = "macos", test))]
fn elevated_result(files: &RequestFiles, exit_code: i32) -> io::Result<()> {
    let response = fs::read_to_string(&files.result).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("elevated save helper did not publish a result: {error}"),
        )
    })?;
    if exit_code == 0 && response.trim() == "ok" {
        return Ok(());
    }
    if let Some(message) = response
        .strip_prefix("error\t")
        .and_then(|value| value.split_once('\t'))
        .map(|(_, message)| message.trim())
        .filter(|message| !message.is_empty())
    {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, message));
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        format!("elevated save helper returned an invalid result (exit code {exit_code})"),
    ))
}

#[cfg(any(windows, target_os = "macos"))]
fn write_with_native_elevation(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let files = RequestFiles::create(bytes)?;
    let outcome = (|| {
        files.write_request(path)?;
        let executable = std::env::current_exe()?;
        #[cfg(windows)]
        let exit_code = super::windows::run_elevated_helper(&executable, &files.request)?;
        #[cfg(target_os = "macos")]
        let exit_code = super::macos::run_elevated_helper(&executable, &files.request)?;
        elevated_result(&files, exit_code)
    })();
    files.cleanup();
    outcome
}

fn encode_elevated_text(text: &str, format: TextFileFormat) -> io::Result<Vec<u8>> {
    encode_text(text, format)
}

/// `ErrorKind::Interrupted`: the caller requested cancellation (app shutdown).
fn elevated_save_cancelled() -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, "elevated save was cancelled")
}

/// Blocking elevated write; run it off the UI thread. `cancel` is checked before
/// the elevation prompt starts and, on Linux, while pkexec runs: the managed
/// process tree then gets a graceful stop and is killed after
/// `PKEXEC_TERMINATE_GRACE`. The native Windows/macOS helpers are not
/// interruptible once started.
pub fn write_text_file_elevated(
    path: &Path,
    text: &str,
    format: TextFileFormat,
    cancel: &AtomicBool,
) -> io::Result<()> {
    let bytes = encode_elevated_text(text, format)?;
    if cancel.load(Ordering::Acquire) {
        return Err(elevated_save_cancelled());
    }
    #[cfg(target_os = "linux")]
    {
        return write_with_pkexec(path, bytes, cancel);
    }
    #[cfg(any(windows, target_os = "macos"))]
    {
        write_with_native_elevation(path, &bytes)
    }
    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    {
        let _ = (path, bytes);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "elevated file replacement is not supported on this platform",
        ))
    }
}

#[cfg(target_os = "linux")]
const PKEXEC_POLL_INTERVAL: Duration = Duration::from_millis(50);
#[cfg(target_os = "linux")]
const PKEXEC_TERMINATE_GRACE: Duration = Duration::from_millis(500);

#[cfg(target_os = "linux")]
fn write_with_pkexec(path: &Path, bytes: Vec<u8>, cancel: &AtomicBool) -> io::Result<()> {
    let mut command = Command::new("pkexec");
    command
        .arg("tee")
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = super::ManagedChild::spawn(&mut command)?;
    // pkexec reads stdin only after authentication: a payload larger than the pipe
    // buffer would block a direct write past a cancel request, so a feeder thread
    // writes it and closes stdin (EOF for tee) when done.
    let feeder = match child.take_stdin() {
        Some(mut stdin) => Some(super::spawn_named("rriter-elevated-save-stdin", move || {
            stdin.write_all(&bytes)
        })?),
        None => None,
    };
    let status = loop {
        if let Some(status) = child.wait_timeout(PKEXEC_POLL_INTERVAL)? {
            break status;
        }
        if cancel.load(Ordering::Acquire) {
            child.terminate(PKEXEC_TERMINATE_GRACE)?;
            // The feeder is not joined: a root tee that survived the group signal may
            // still hold the pipe; the thread ends with the write or with EPIPE.
            return Err(elevated_save_cancelled());
        }
    };
    if !status.success() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "elevated file replacement was rejected",
        ));
    }
    match feeder.map(|feeder| feeder.join()) {
        None | Some(Ok(Ok(()))) => Ok(()),
        Some(Ok(Err(error))) => Err(error),
        Some(Err(_)) => Err(io::Error::other("elevated save input writer panicked")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn test_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "rriter-elevated-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn elevated_text_encoding_rejects_unrepresentable_legacy_text_before_write() {
        let format = TextFileFormat {
            encoding: super::super::TextEncoding::Legacy(super::super::LegacyEncoding::Iso8859_5),
            line_ending: super::super::LineEnding::Lf,
        };
        let error = encode_elevated_text("Привет 😀", format).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("ISO-8859-5"));
    }

    #[test]
    fn request_roundtrip_preserves_native_target_paths() {
        let root = test_root("roundtrip");
        let directory = root.join(format!("{REQUEST_DIR_PREFIX}10-20"));
        fs::create_dir_all(&directory).unwrap();
        let files = RequestFiles::from_directory(directory);
        let target = PathBuf::from(r"C:\Program Files\RRiter\Пример.txt");
        files.write_request(&target).unwrap();
        assert_eq!(read_request(&files).unwrap(), target);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn helper_rejects_untrusted_request_layouts() {
        let root = test_root("invalid");
        fs::create_dir_all(&root).unwrap();
        let request = root.join(REQUEST_FILE_NAME);
        fs::write(&request, b"{}").unwrap();
        let error = RequestFiles::from_request_path(&request).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn helper_applies_payload_atomically_and_reports_success() {
        let root = test_root("apply");
        let request_dir = root.join(format!("{REQUEST_DIR_PREFIX}30-40"));
        fs::create_dir_all(&request_dir).unwrap();
        let files = RequestFiles::from_directory(request_dir);
        let target = root.join("protected.txt");
        fs::write(&target, b"old").unwrap();
        fs::write(&files.payload, b"new").unwrap();
        files.write_request(&target).unwrap();

        assert_eq!(run_helper(&files.request), 0);
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert!(fs::read_to_string(&files.result).unwrap().starts_with("ok"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn helper_failure_is_written_for_the_parent_process() {
        let root = test_root("failure");
        let request_dir = root.join(format!("{REQUEST_DIR_PREFIX}50-60"));
        fs::create_dir_all(&request_dir).unwrap();
        let files = RequestFiles::from_directory(request_dir);
        let target_directory = root.join("existing-directory");
        fs::create_dir_all(&target_directory).unwrap();
        files.write_request(&target_directory).unwrap();
        fs::write(&files.payload, b"new").unwrap();

        assert_eq!(run_helper(&files.request), 1);
        let response = fs::read_to_string(&files.result).unwrap();
        assert!(response.starts_with("error\t"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn elevated_result_propagates_helper_message() {
        let root = test_root("result");
        let request_dir = root.join(format!("{REQUEST_DIR_PREFIX}70-80"));
        fs::create_dir_all(&request_dir).unwrap();
        let files = RequestFiles::from_directory(request_dir);
        fs::write(&files.result, b"error\t5\taccess denied\n").unwrap();
        let error = elevated_result(&files, 1).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(error.to_string(), "access denied");
        let _ = fs::remove_dir_all(root);
    }
}
