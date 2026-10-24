// Hyperlinks printed by programs in the terminal. A link is untrusted data, so
// every rule here runs before anything is opened: only http, https and local
// file links are acted on, a file is canonicalized before the opener sees it,
// and a launchable file is revealed in the file manager instead of run.

use super::desktop_request::{
    ExternalRequestSink, external_requests_intercepted, open_path, open_url, reveal_path,
    spawn_open_path, spawn_reveal_path,
};
use super::terminal_cwd::{local_host_name, osc7_path};
use super::{CURRENT_PLATFORM, PlatformKind, hex_digit, reap_unit_thread, spawn_named};
use std::ffi::OsStr;
use std::fs::Metadata;
use std::io;
use std::path::{Path, PathBuf};

/// Longest local path a terminal link may name, in decoded bytes.
const MAX_LINK_PATH_BYTES: usize = 4096;

/// Extensions that run on macOS without an executable bit; the file manager selects them.
const MACOS_LAUNCHABLE_EXTENSIONS: &[&str] = &[
    "app", "command", "tool", "terminal", "workflow", "pkg", "mpkg", "prefpane", "mobileconfig",
    "jar",
];

/// Extensions that run or open with a shell on Windows, whatever `PATHEXT` says.
const WINDOWS_LAUNCHABLE_EXTENSIONS: &[&str] = &[
    "exe", "com", "bat", "cmd", "lnk", "url", "scr", "msi", "msp", "msc", "ps1", "vbs", "js",
    "jse", "wsf", "hta", "cpl", "reg", "jar", "appref-ms", "application", "chm", "pif", "inf",
    "settingcontent-ms",
];

/// Routes one terminal hyperlink. Returns `true` when a request was issued: an
/// opener was started, or in headless mode the request was recorded. `mailto:`
/// and every other scheme give `false` without any request.
pub fn open_terminal_uri(requests: &ExternalRequestSink, uri: &str) -> bool {
    let Ok(parsed) = url::Url::parse(uri) else {
        return false;
    };
    match parsed.scheme() {
        "http" | "https" => {
            ascii_authority(uri)
                && open_url(requests, uri)
                    .inspect_err(|error| eprintln!("RRiter could not open terminal link {uri:?}: {error}"))
                    .is_ok()
        }
        "file" => open_file_uri(requests, &parsed),
        _ => false,
    }
}

/// Whether the authority of `uri` is spelled in ASCII. Checked on the text as printed,
/// because `url` turns an internationalized host into punycode and would hide it.
fn ascii_authority(uri: &str) -> bool {
    let rest = uri.split_once("//").map_or("", |(_, rest)| rest);
    rest.split(['/', '?', '#']).next().unwrap_or_default().is_ascii()
}

/// Decodes the percent-encoded path of a URI into raw bytes. Malformed escapes,
/// NUL bytes, and paths longer than the link limit give `None`.
pub fn percent_decode_uri_path(raw: &str) -> Option<Vec<u8>> {
    let input = raw.as_bytes();
    let mut decoded = Vec::with_capacity(input.len());
    let mut index = 0;
    while let Some(&byte) = input.get(index) {
        if byte == b'%' {
            let high = hex_digit(*input.get(index + 1)?)?;
            let low = hex_digit(*input.get(index + 2)?)?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(byte);
            index += 1;
        }
        if decoded.len() > MAX_LINK_PATH_BYTES {
            return None;
        }
    }
    (!decoded.contains(&0)).then_some(decoded)
}

fn open_file_uri(requests: &ExternalRequestSink, uri: &url::Url) -> bool {
    let Some(bytes) = percent_decode_uri_path(uri.path()) else {
        return false;
    };
    // An empty path or a bare "/" names only the root, which is never opened.
    if bytes.len() <= 1 {
        return false;
    }
    let Some(path) = osc7_path(uri.host_str().unwrap_or(""), &bytes, local_host_name()) else {
        return false;
    };
    dispatch_local_path(requests, path)
}

/// Headless resolves inline so the recorded request is visible at once. A real
/// session resolves, decides, and waits for the opener on one named thread, so
/// the click itself touches no file system.
fn dispatch_local_path(requests: &ExternalRequestSink, path: PathBuf) -> bool {
    if external_requests_intercepted(requests) {
        return match resolve_local_path(&path, CURRENT_PLATFORM) {
            Some(LinkAction::Open(target)) => open_path(requests, &target).is_ok(),
            Some(LinkAction::Reveal(target)) => {
                let _ = reveal_path(requests, &target);
                true
            }
            None => false,
        };
    }
    match spawn_named("terminal-link-open", move || run_local_path(path)) {
        Ok(worker) => {
            reap_unit_thread(worker);
            true
        }
        Err(_) => false,
    }
}

fn run_local_path(path: PathBuf) {
    match resolve_local_path(&path, CURRENT_PLATFORM) {
        Some(LinkAction::Open(target)) => match spawn_open_path(&target) {
            Ok(Some(mut child)) => {
                let _ = child.wait();
            }
            Ok(None) => {}
            Err(error) => eprintln!("RRiter could not open terminal link target {:?}: {error}", target),
        },
        Some(LinkAction::Reveal(target)) => match spawn_reveal_path(&target, true) {
            Ok(mut child) => {
                let _ = child.wait();
            }
            Err(error) => eprintln!("RRiter could not reveal terminal link target {:?}: {error}", target),
        },
        None => {}
    }
}

#[derive(Debug, PartialEq, Eq)]
enum LinkAction {
    /// Hand the canonical path to the platform's default handler.
    Open(PathBuf),
    /// Show the canonical path in the file manager.
    Reveal(PathBuf),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FileKind {
    Directory,
    RegularFile,
    Other,
}

/// Resolves a link's local path to the path the opener receives. `None` when
/// the path does not resolve or is not a file system entry; that is logged once.
fn resolve_local_path(path: &Path, platform: PlatformKind) -> Option<LinkAction> {
    let canonical = match canonical_local_path(path) {
        Ok(canonical) => canonical,
        Err(error) => {
            eprintln!("RRiter ignored a terminal link to an unresolved path {:?}: {error}", path);
            return None;
        }
    };
    let metadata = std::fs::metadata(&canonical).ok()?;
    let kind = if metadata.is_dir() {
        FileKind::Directory
    } else if metadata.is_file() {
        FileKind::RegularFile
    } else {
        FileKind::Other
    };
    let pathext = std::env::var_os("PATHEXT");
    Some(choose_action(
        platform,
        canonical,
        kind,
        executable_bit(&metadata),
        pathext.as_deref(),
    ))
}

/// Directories open, except launchable macOS bundles. Regular files reveal when
/// they would run. Anything that is not a regular file or directory reveals.
fn choose_action(
    platform: PlatformKind,
    path: PathBuf,
    kind: FileKind,
    executable_bit: bool,
    pathext: Option<&OsStr>,
) -> LinkAction {
    let reveal = match kind {
        FileKind::Directory => {
            platform == PlatformKind::Macos
                && launchable_for_platform(platform, &path, false, pathext)
        }
        FileKind::RegularFile => launchable_for_platform(platform, &path, executable_bit, pathext),
        FileKind::Other => true,
    };
    if reveal {
        LinkAction::Reveal(path)
    } else {
        LinkAction::Open(path)
    }
}

/// Whether a file would run when opened. The executable bit is supplied by the
/// caller; the extension lists are fixed per platform, and `PATHEXT` only adds.
pub(crate) fn launchable_for_platform(
    platform: PlatformKind,
    path: &Path,
    executable_bit: bool,
    pathext: Option<&OsStr>,
) -> bool {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match platform {
        PlatformKind::Linux | PlatformKind::Other => executable_bit || extension == "desktop",
        PlatformKind::Macos => {
            executable_bit || MACOS_LAUNCHABLE_EXTENSIONS.contains(&extension.as_str())
        }
        PlatformKind::Windows => {
            WINDOWS_LAUNCHABLE_EXTENSIONS.contains(&extension.as_str())
                || pathext_lists(pathext, &extension)
        }
    }
}

/// `true` when the `PATHEXT` list names `extension` (lowercase, without a dot).
fn pathext_lists(pathext: Option<&OsStr>, extension: &str) -> bool {
    !extension.is_empty()
        && pathext.and_then(|list| list.to_str()).is_some_and(|list| {
            list.split(';').any(|item| {
                item.trim().trim_start_matches('.').eq_ignore_ascii_case(extension)
            })
        })
}

/// Arguments for `/usr/bin/open` that reveal `path` in Finder. Files are always
/// selected; `always_select` extends that to bundles that would otherwise run.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn macos_reveal_args(path: &Path, always_select: bool) -> Vec<std::ffi::OsString> {
    let mut args = Vec::with_capacity(2);
    if always_select || path.is_file() {
        args.push(std::ffi::OsString::from("-R"));
    }
    args.push(path.as_os_str().to_os_string());
    args
}

/// Turns a canonical Windows path into the form Explorer and ShellExecute accept.
#[cfg(any(windows, test))]
pub(crate) fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    let Some(raw) = path.to_str() else {
        return path.to_path_buf();
    };
    if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = raw.strip_prefix(r"\\?\") {
        let bytes = rest.as_bytes();
        if bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\' {
            return PathBuf::from(rest);
        }
    }
    path.to_path_buf()
}

fn canonical_local_path(path: &Path) -> io::Result<PathBuf> {
    std::fs::canonicalize(path).map(without_verbatim_prefix)
}

#[cfg(windows)]
fn without_verbatim_prefix(path: PathBuf) -> PathBuf {
    strip_verbatim_prefix(&path)
}

#[cfg(not(windows))]
fn without_verbatim_prefix(path: PathBuf) -> PathBuf {
    path
}

#[cfg(unix)]
fn executable_bit(metadata: &Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn executable_bit(_metadata: &Metadata) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{ExternalRequest, ExternalRequestLog};
    use std::ffi::OsString;

    /// Runs one link through a headless sink and returns its result and last request.
    #[cfg(unix)]
    fn intercepted(uri: &str) -> (bool, Option<ExternalRequest>) {
        let mut log = ExternalRequestLog::new(true);
        let accepted = open_terminal_uri(log.sink(), uri);
        (accepted, log.take())
    }

    /// A fresh directory per test, so parallel tests never share files.
    #[cfg(unix)]
    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rriter-terminal-link-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch directory");
        dir
    }

    #[test]
    fn percent_decode_uri_path_rules() {
        assert_eq!(percent_decode_uri_path("/a%20b"), Some(b"/a b".to_vec()));
        assert_eq!(percent_decode_uri_path("/%2e%2E"), Some(b"/..".to_vec()));
        assert_eq!(percent_decode_uri_path("/a%zz"), None);
        assert_eq!(percent_decode_uri_path("/a%2"), None);
        assert_eq!(percent_decode_uri_path("/a%"), None);
        assert_eq!(percent_decode_uri_path("/a%00"), None);
        let exact = format!("/{}", "a".repeat(MAX_LINK_PATH_BYTES - 1));
        assert_eq!(percent_decode_uri_path(&exact).map(|bytes| bytes.len()), Some(MAX_LINK_PATH_BYTES));
        let too_long = format!("/{}", "a".repeat(MAX_LINK_PATH_BYTES));
        assert_eq!(percent_decode_uri_path(&too_long), None);
    }

    #[test]
    fn launchable_path_rules() {
        let linux = PlatformKind::Linux;
        assert!(launchable_for_platform(linux, Path::new("a.sh"), true, None));
        assert!(!launchable_for_platform(linux, Path::new("a.txt"), false, None));
        assert!(launchable_for_platform(linux, Path::new("x.desktop"), false, None));
        assert!(launchable_for_platform(linux, Path::new("x.DESKTOP"), false, None));
        assert!(!launchable_for_platform(linux, Path::new("x.app"), false, None));

        let macos = PlatformKind::Macos;
        assert!(launchable_for_platform(macos, Path::new("X.app"), false, None));
        assert!(launchable_for_platform(macos, Path::new("a.command"), false, None));
        assert!(launchable_for_platform(macos, Path::new("a.pkg"), false, None));
        assert!(launchable_for_platform(macos, Path::new("a.jar"), false, None));
        assert!(launchable_for_platform(macos, Path::new("a.txt"), true, None));
        assert!(!launchable_for_platform(macos, Path::new("a.txt"), false, None));

        let windows = PlatformKind::Windows;
        assert!(launchable_for_platform(windows, Path::new("a.exe"), false, None));
        assert!(launchable_for_platform(windows, Path::new("a.EXE"), false, None));
        assert!(launchable_for_platform(windows, Path::new("a.lnk"), false, None));
        assert!(launchable_for_platform(windows, Path::new("a.settingcontent-ms"), false, None));
        assert!(!launchable_for_platform(windows, Path::new("a.txt"), true, None));
        assert!(!launchable_for_platform(windows, Path::new("a.py"), false, None));
    }

    #[test]
    fn windows_launchable_ignores_narrow_pathext() {
        let narrow = Some(OsStr::new(".PY"));
        assert!(launchable_for_platform(PlatformKind::Windows, Path::new("a.py"), false, narrow));
        assert!(launchable_for_platform(PlatformKind::Windows, Path::new("a.exe"), false, narrow));
        assert!(launchable_for_platform(PlatformKind::Windows, Path::new("b.CMD"), false, narrow));
        assert!(!launchable_for_platform(PlatformKind::Windows, Path::new("a"), false, narrow));
        assert!(!launchable_for_platform(PlatformKind::Windows, Path::new("a.py"), false, None));
    }

    #[test]
    fn macos_reveal_args_always_select() {
        let dir = std::env::temp_dir().join(format!("rriter-reveal-args-{}", std::process::id()));
        let app = dir.join("X.app");
        std::fs::create_dir_all(&app).expect("bundle directory");
        assert_eq!(
            macos_reveal_args(&app, true),
            vec![OsString::from("-R"), app.as_os_str().to_os_string()]
        );
        assert_eq!(macos_reveal_args(&app, false), vec![app.as_os_str().to_os_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn strip_verbatim_prefix_forms() {
        assert_eq!(strip_verbatim_prefix(Path::new(r"\\?\C:\x")), PathBuf::from(r"C:\x"));
        assert_eq!(
            strip_verbatim_prefix(Path::new(r"\\?\UNC\srv\share\x")),
            PathBuf::from(r"\\srv\share\x")
        );
        assert_eq!(strip_verbatim_prefix(Path::new(r"C:\x")), PathBuf::from(r"C:\x"));
        assert_eq!(strip_verbatim_prefix(Path::new(r"\\srv\share")), PathBuf::from(r"\\srv\share"));
        assert_eq!(
            strip_verbatim_prefix(Path::new(r"\\?\Volume{1}\x")),
            PathBuf::from(r"\\?\Volume{1}\x")
        );
    }

    #[test]
    fn choose_action_rules() {
        let linux = PlatformKind::Linux;
        let dir = PathBuf::from("/x/d");
        assert_eq!(
            choose_action(linux, dir.clone(), FileKind::Directory, true, None),
            LinkAction::Open(dir.clone())
        );
        assert_eq!(
            choose_action(linux, PathBuf::from("/x/a.txt"), FileKind::RegularFile, false, None),
            LinkAction::Open(PathBuf::from("/x/a.txt"))
        );
        assert_eq!(
            choose_action(linux, PathBuf::from("/x/run"), FileKind::RegularFile, true, None),
            LinkAction::Reveal(PathBuf::from("/x/run"))
        );
        assert_eq!(
            choose_action(linux, PathBuf::from("/x/fifo"), FileKind::Other, false, None),
            LinkAction::Reveal(PathBuf::from("/x/fifo"))
        );
        assert_eq!(
            choose_action(PlatformKind::Macos, PathBuf::from("/x/X.app"), FileKind::Directory, false, None),
            LinkAction::Reveal(PathBuf::from("/x/X.app"))
        );
        assert_eq!(
            choose_action(PlatformKind::Windows, PathBuf::from(r"C:\a.exe"), FileKind::RegularFile, false, None),
            LinkAction::Reveal(PathBuf::from(r"C:\a.exe"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn file_link_matrix_in_headless() {
        use std::os::unix::fs::PermissionsExt;

        // Schemes other than http, https and file are ignored.
        assert_eq!(intercepted("mailto:a@b"), (false, None));
        assert_eq!(intercepted("javascript:alert(1)"), (false, None));
        assert_eq!(intercepted("https://a"), (true, Some(ExternalRequest::OpenUrl("https://a".into()))));

        // Local directories and missing paths.
        assert_eq!(intercepted("file:///tmp"), (true, Some(ExternalRequest::OpenPath("/tmp".into()))));
        assert_eq!(intercepted("file://LOCALHOST/tmp"), (true, Some(ExternalRequest::OpenPath("/tmp".into()))));
        assert_eq!(intercepted("file://other/tmp"), (false, None));
        assert_eq!(intercepted("file:///tmp/a%zz"), (false, None));
        assert_eq!(intercepted("file:///tmp/a%00"), (false, None));

        let dir = scratch_dir("matrix");
        let canonical_dir = std::fs::canonicalize(&dir).expect("canonical scratch directory");
        let file_uri = |name: &str| format!("file://{}/{name}", dir.display());

        // A missing path is ignored.
        assert_eq!(intercepted(&file_uri("missing")), (false, None));

        // A plain file opens, at its canonical path, with the escape decoded.
        std::fs::write(dir.join("a b"), b"text").expect("plain file");
        assert_eq!(
            intercepted(&file_uri("a%20b")),
            (true, Some(ExternalRequest::OpenPath(canonical_dir.join("a b"))))
        );

        // An executable file and a .desktop file are revealed, never opened.
        let script = dir.join("run.sh");
        std::fs::write(&script, b"#!/bin/sh\n").expect("script");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("mode");
        assert_eq!(
            intercepted(&file_uri("run.sh")),
            (true, Some(ExternalRequest::RevealPath(canonical_dir.join("run.sh"))))
        );
        std::fs::write(dir.join("entry.desktop"), b"[Desktop Entry]\n").expect("desktop entry");
        assert_eq!(
            intercepted(&file_uri("entry.desktop")),
            (true, Some(ExternalRequest::RevealPath(canonical_dir.join("entry.desktop"))))
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn root_and_non_ascii_host_are_rejected() {
        // The root names nothing to open, whether spelled empty or as "/".
        assert_eq!(intercepted("file:///"), (false, None));
        assert_eq!(intercepted("file://localhost/"), (false, None));
        // A non-ASCII host is refused as printed, although `url` would punycode it.
        assert_eq!(intercepted("https://ß"), (false, None));
        assert_eq!(intercepted("http://a.ß/x"), (false, None));
    }
}
