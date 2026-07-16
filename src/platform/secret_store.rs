use super::resolve_executable;
use std::io;
#[cfg(target_os = "linux")]
use std::ffi::OsStr;
#[cfg(any(windows, target_os = "linux"))]
use std::path::PathBuf;

const MAX_SECRET_PURPOSE_BYTES: usize = 512;

fn validate_purpose(purpose: &str) -> io::Result<()> {
    if purpose.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "system secret purpose is empty",
        ));
    }
    if purpose.len() > MAX_SECRET_PURPOSE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "system secret purpose is too long",
        ));
    }
    if purpose.chars().any(char::is_control) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "system secret purpose contains control characters",
        ));
    }
    Ok(())
}

/// Stores a secret using the current operating system's user secret service.
///
/// This API deliberately has no plaintext fallback. Callers may keep a value
/// in memory for the current session when the service is unavailable, but must
/// not persist it by another means.
pub fn store_system_user_secret(purpose: &str, bytes: &[u8]) -> io::Result<()> {
    validate_purpose(purpose)?;

    #[cfg(windows)]
    {
        let protected = super::windows::protect_user_secret(bytes, purpose)?;
        return super::atomic_write_secret(&windows_secret_path(purpose), &protected);
    }

    #[cfg(target_os = "macos")]
    {
        return super::macos::store_keychain_secret(purpose, bytes);
    }

    #[cfg(target_os = "linux")]
    {
        return linux_secret_tool_store(purpose, bytes);
    }

    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        let _ = bytes;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "this platform does not provide a supported system secret service",
        ))
    }
}

/// Loads a secret from the current operating system's user secret service.
#[allow(dead_code)] // Database connection UI starts loading stored secrets in stage 3.
pub fn load_system_user_secret(purpose: &str) -> io::Result<Option<Vec<u8>>> {
    validate_purpose(purpose)?;

    #[cfg(windows)]
    {
        let path = windows_secret_path(purpose);
        let protected = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        return super::windows::unprotect_user_secret(&protected, purpose).map(Some);
    }

    #[cfg(target_os = "macos")]
    {
        return match super::macos::load_keychain_secret(purpose) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if super::macos::is_keychain_item_not_found(&error) => Ok(None),
            Err(error) => Err(error),
        };
    }

    #[cfg(target_os = "linux")]
    {
        return linux_secret_tool_lookup(purpose);
    }

    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "this platform does not provide a supported system secret service",
        ))
    }
}

/// Deletes a secret from the current operating system's user secret service.
pub fn delete_system_user_secret(purpose: &str) -> io::Result<()> {
    validate_purpose(purpose)?;

    #[cfg(windows)]
    {
        return match std::fs::remove_file(windows_secret_path(purpose)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        };
    }

    #[cfg(target_os = "macos")]
    {
        return super::macos::delete_keychain_secret(purpose);
    }

    #[cfg(target_os = "linux")]
    {
        return linux_secret_tool_clear(purpose);
    }

    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "this platform does not provide a supported system secret service",
        ))
    }
}

#[cfg(windows)]
fn windows_secret_path(purpose: &str) -> PathBuf {
    let mut encoded = String::with_capacity(purpose.len().saturating_mul(2));
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in purpose.as_bytes() {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    super::config_dir()
        .join("system-secrets")
        .join(format!("{encoded}.secret"))
}

#[cfg(target_os = "linux")]
fn secret_tool_path() -> io::Result<PathBuf> {
    resolve_executable(OsStr::new("secret-tool")).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "system secret storage is unavailable: secret-tool was not found",
        )
    })
}

#[cfg(target_os = "linux")]
fn secret_tool_base_args(purpose: &str) -> [String; 4] {
    [
        "application".to_string(),
        "rriter".to_string(),
        "purpose".to_string(),
        purpose.to_string(),
    ]
}

#[cfg(target_os = "linux")]
fn linux_secret_tool_store(purpose: &str, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    use std::time::Duration;

    let program = secret_tool_path()?;
    let mut command = Command::new(program);
    command
        .arg("store")
        .arg("--label=RRiter database secret")
        .args(secret_tool_base_args(purpose))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = super::ManagedChild::spawn(&mut command)?;
    let mut stdin = child
        .take_stdin()
        .ok_or_else(|| io::Error::other("secret-tool stdin was not piped"))?;
    stdin.write_all(bytes)?;
    drop(stdin);
    let stdout = child
        .take_stdout()
        .ok_or_else(|| io::Error::other("secret-tool stdout was not piped"))?;
    let stderr = child
        .take_stderr()
        .ok_or_else(|| io::Error::other("secret-tool stderr was not piped"))?;
    let stdout_reader = std::thread::spawn(move || read_bounded(stdout));
    let stderr_reader = std::thread::spawn(move || read_bounded(stderr));
    let status = child
        .wait_timeout(Duration::from_secs(30))?
        .ok_or_else(|| {
            let _ = child.terminate(Duration::from_millis(200));
            io::Error::new(io::ErrorKind::TimedOut, "secret-tool store timed out")
        })?;
    let _ = join_bounded_reader(stdout_reader)?;
    let stderr = join_bounded_reader(stderr_reader)?;
    command_status(status.success(), "secret-tool store", &stderr)
}

#[cfg(target_os = "linux")]
fn linux_secret_tool_lookup(purpose: &str) -> io::Result<Option<Vec<u8>>> {
    use std::process::Command;
    use std::time::Duration;

    let mut command = Command::new(secret_tool_path()?);
    command.arg("lookup").args(secret_tool_base_args(purpose));
    let output = super::run_command_output(&mut command, Duration::from_secs(30))?;
    if output.status.success() {
        return Ok(Some(trim_single_trailing_newline(output.stdout)));
    }
    if output.status.code() == Some(1) && output.stderr.is_empty() {
        return Ok(None);
    }
    command_status(false, "secret-tool lookup", &output.stderr)?;
    unreachable!()
}

#[cfg(target_os = "linux")]
fn linux_secret_tool_clear(purpose: &str) -> io::Result<()> {
    use std::process::Command;
    use std::time::Duration;

    let mut command = Command::new(secret_tool_path()?);
    command.arg("clear").args(secret_tool_base_args(purpose));
    let output = super::run_command_output(&mut command, Duration::from_secs(30))?;
    if output.status.success()
        || (output.status.code() == Some(1) && output.stderr.is_empty())
    {
        return Ok(());
    }
    command_status(false, "secret-tool clear", &output.stderr)
}

#[cfg(target_os = "linux")]
fn trim_single_trailing_newline(mut bytes: Vec<u8>) -> Vec<u8> {
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
    }
    bytes
}

#[cfg(target_os = "linux")]
fn command_status(success: bool, operation: &str, stderr: &[u8]) -> io::Result<()> {
    if success {
        return Ok(());
    }
    let detail = String::from_utf8_lossy(stderr);
    let detail = detail.trim();
    let message = if detail.is_empty() {
        format!("{operation} failed")
    } else {
        format!("{operation} failed: {detail}")
    };
    Err(io::Error::other(message))
}

#[cfg(target_os = "linux")]
fn read_bounded(mut reader: impl std::io::Read) -> io::Result<Vec<u8>> {
    use std::io::Read;
    const LIMIT: u64 = 16 * 1024;
    let mut bytes = Vec::new();
    reader.by_ref().take(LIMIT).read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[cfg(target_os = "linux")]
fn join_bounded_reader(
    handle: std::thread::JoinHandle<io::Result<Vec<u8>>>,
) -> io::Result<Vec<u8>> {
    handle
        .join()
        .map_err(|_| io::Error::other("secret-tool output reader panicked"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purpose_validation_rejects_empty_control_and_oversized_values() {
        assert!(validate_purpose("").is_err());
        assert!(validate_purpose("database\npassword").is_err());
        assert!(validate_purpose(&"x".repeat(MAX_SECRET_PURPOSE_BYTES + 1)).is_err());
        assert!(validate_purpose("database:42:postgres_password").is_ok());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn secret_tool_attributes_do_not_contain_secret_material() {
        let args = secret_tool_base_args("database:42:postgres_password");
        assert_eq!(args[0], "application");
        assert_eq!(args[1], "rriter");
        assert_eq!(args[2], "purpose");
        assert_eq!(args[3], "database:42:postgres_password");
        assert!(!args.iter().any(|arg| arg.contains("actual-password")));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn lookup_trims_only_the_protocol_newline() {
        assert_eq!(trim_single_trailing_newline(b"value\n".to_vec()), b"value");
        assert_eq!(trim_single_trailing_newline(b"value\r\n".to_vec()), b"value");
        assert_eq!(trim_single_trailing_newline(b"value\n\n".to_vec()), b"value\n");
    }
}
