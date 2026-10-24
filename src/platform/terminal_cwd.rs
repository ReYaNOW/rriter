// OSC 7 working-directory reports. The terminal sends `file://host/path`; only
// a local host yields a path, and that path is then used as a native path.

use std::path::PathBuf;

/// Maps an OSC 7 host and path to a local path. `path` holds the bytes of the
/// URI path after the caller's percent-decoding. `None` for a remote host, an
/// empty or relative path, a NUL byte, or a form this platform cannot name.
pub fn osc7_path(host: &str, path: &[u8], local_host: &str) -> Option<PathBuf> {
    if !is_local_host(host, local_host) || path.contains(&0) {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let candidate = std::path::Path::new(std::ffi::OsStr::from_bytes(path));
        return candidate.is_absolute().then(|| candidate.to_path_buf());
    }
    #[cfg(windows)]
    {
        return osc7_windows_path(std::str::from_utf8(path).ok()?);
    }
    #[allow(unreachable_code)]
    None
}

/// Empty, `localhost`, or the machine name, compared without regard to case.
fn is_local_host(host: &str, local_host: &str) -> bool {
    host.is_empty()
        || host.eq_ignore_ascii_case("localhost")
        || (!local_host.is_empty() && host.eq_ignore_ascii_case(local_host))
}

/// Converts the Windows drive form `/C:/dir` to `C:\dir`. Anything else
/// (UNC, relative, no drive letter) gives `None`.
#[cfg(any(windows, test))]
fn osc7_windows_path(path: &str) -> Option<PathBuf> {
    let rest = path.strip_prefix('/')?;
    let bytes = rest.as_bytes();
    if bytes.len() < 2 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' {
        return None;
    }
    if bytes.len() > 2 && bytes[2] != b'/' {
        return None;
    }
    // A colon after the drive letter names an NTFS alternate stream, not a path.
    if rest[2..].contains(':') {
        return None;
    }
    let mut native = rest.replace('/', "\\");
    if native.len() == 2 {
        native.push('\\');
    }
    Some(PathBuf::from(native))
}

/// The machine name; empty when it cannot be read. Read per call (OSC 7 and
/// link clicks only), so no process-wide cache is needed.
pub fn local_host_name() -> String {
    read_host_name()
}

fn read_host_name() -> String {
    #[cfg(unix)]
    {
        let mut buffer = [0u8; 256];
        // SAFETY: `buffer` is valid for writes of its full length, and its
        // contents are only read when `gethostname` reports success.
        let status = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
        if status != 0 {
            return String::new();
        }
        let end = buffer.iter().position(|&byte| byte == 0).unwrap_or(buffer.len());
        return String::from_utf8_lossy(&buffer[..end]).into_owned();
    }
    #[cfg(windows)]
    {
        return std::env::var("COMPUTERNAME").unwrap_or_default();
    }
    #[allow(unreachable_code)]
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn osc7_path_unix_and_host_check() {
        assert_eq!(osc7_path("", b"/tmp/a b", "box"), Some(PathBuf::from("/tmp/a b")));
        assert_eq!(osc7_path("localhost", b"/tmp", "box"), Some(PathBuf::from("/tmp")));
        assert_eq!(osc7_path("LocalHost", b"/tmp", "box"), Some(PathBuf::from("/tmp")));
        assert_eq!(osc7_path("BOX", b"/tmp", "box"), Some(PathBuf::from("/tmp")));
        assert_eq!(osc7_path("other", b"/tmp", "box"), None);
        assert_eq!(osc7_path("box", b"/tmp", ""), None);
        assert_eq!(osc7_path("", b"", "box"), None);
        assert_eq!(osc7_path("", b"relative/dir", "box"), None);
        assert_eq!(osc7_path("", b"/tmp/a\0b", "box"), None);
    }

    #[test]
    fn osc7_path_windows_drive_form() {
        assert_eq!(osc7_windows_path("/C:/x"), Some(PathBuf::from(r"C:\x")));
        assert_eq!(osc7_windows_path("/c:/a b/c"), Some(PathBuf::from(r"c:\a b\c")));
        assert_eq!(osc7_windows_path("/C:"), Some(PathBuf::from(r"C:\")));
        assert_eq!(osc7_windows_path("/C:/"), Some(PathBuf::from(r"C:\")));
        assert_eq!(osc7_windows_path("C:/x"), None);
        assert_eq!(osc7_windows_path("/C:x"), None);
        assert_eq!(osc7_windows_path("//server/share"), None);
        assert_eq!(osc7_windows_path("/1:/x"), None);
        assert_eq!(osc7_windows_path("/"), None);
        assert_eq!(osc7_windows_path(""), None);
        // Alternate stream names (NTFS "file:stream") are not paths.
        assert_eq!(osc7_windows_path("/C:/x.exe:s"), None);
        assert_eq!(osc7_windows_path("/C:/a:b/c"), None);
        assert_eq!(osc7_windows_path("/C::x"), None);
    }
}
