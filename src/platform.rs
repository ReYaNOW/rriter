use std::ffi::OsString;
#[cfg(any(windows, test))]
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Child;
#[cfg(any(windows, target_os = "linux"))]
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use winit::window::WindowAttributes;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;
mod process;
mod integration;
mod elevated_save;
pub use integration::{
    SystemProxyConfig, ToolKind, ToolPaths, app_paths, async_http_client_builder,
    blocking_http_client_builder, configure_tool_paths, configured_tool_path,
    current_process_memory_kb, proxy_routing_is_configured, refresh_tool_resolutions,
    resolve_tool_kind, system_proxy_config, user_cache_root,
};
#[cfg(test)]
pub(crate) type AppPaths = integration::AppPaths;

#[cfg_attr(test, allow(dead_code))]
pub fn config_dir() -> PathBuf {
    integration::config_dir()
}

#[cfg_attr(test, allow(dead_code))]
pub fn data_dir() -> PathBuf {
    integration::data_dir()
}

#[allow(dead_code)]
pub fn cache_dir() -> PathBuf {
    integration::cache_dir()
}

#[cfg_attr(test, allow(dead_code))]
pub fn state_dir() -> PathBuf {
    integration::state_dir()
}
pub(crate) use integration::configured_tool_path_for_env;
#[cfg(any(windows, test))]
pub(crate) use integration::parse_windows_proxy_config;
#[cfg(test)]
pub(crate) use integration::{
    app_paths_with, parse_macos_proxy_config, parse_pem_certificates, user_cache_root_with,
};
pub use elevated_save::{handle_startup_helper, write_text_file_elevated};
pub use process::{
    ManagedChild, ProcessOutputStream, ProcessTree, command_for_tool, resolve_executable,
    resolve_tool_executable, run_command_output, run_command_output_cancelable,
    run_command_streaming_cancelable,
};
#[cfg(all(test, unix))]
use process::command_for;

const APP_DIR_NAME: &str = "RRiter";
const PATH_RECORD_PREFIX: &str = "rriter-path-v1:";
const DPAPI_RECORD_PREFIX: &[u8] = b"rriter-dpapi-v1:";
const KEYCHAIN_RECORD_PREFIX: &[u8] = b"rriter-keychain-v1:";
const CLIPBOARD_RETRY_COUNT: usize = 5;
const CLIPBOARD_RETRY_DELAY: Duration = Duration::from_millis(8);
static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) fn next_operation_id() -> String {
    let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}-{counter}", std::process::id())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlatformKind {
    Linux,
    Windows,
    Macos,
    Other,
}


pub const CURRENT_PLATFORM: PlatformKind = if cfg!(target_os = "linux") {
    PlatformKind::Linux
} else if cfg!(target_os = "windows") {
    PlatformKind::Windows
} else if cfg!(target_os = "macos") {
    PlatformKind::Macos
} else {
    PlatformKind::Other
};

pub const DEFAULT_ACCENT_COLOR: [f32; 4] = [
    114.0 / 255.0,
    89.0 / 255.0,
    175.0 / 255.0,
    1.0,
];

pub fn system_accent_color() -> Option<[f32; 4]> {
    #[cfg(windows)]
    {
        return windows::system_accent_color();
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[inline(always)]
pub fn finish_present() {
    #[cfg(windows)]
    windows::finish_present();
}

pub fn primary_shortcut_modifier(modifiers: winit::keyboard::ModifiersState) -> bool {
    primary_shortcut_for_platform(
        CURRENT_PLATFORM,
        modifiers.control_key(),
        modifiers.alt_key(),
        modifiers.super_key(),
    )
}

pub fn word_navigation_modifier(modifiers: winit::keyboard::ModifiersState) -> bool {
    word_modifier_for_platform(
        CURRENT_PLATFORM,
        modifiers.control_key(),
        modifiers.alt_key(),
    )
}

pub fn terminal_control_modifier(modifiers: winit::keyboard::ModifiersState) -> bool {
    terminal_modifiers_for_platform(
        CURRENT_PLATFORM,
        modifiers.control_key(),
        modifiers.alt_key(),
    )
    .0
}

pub fn terminal_alt_modifier(modifiers: winit::keyboard::ModifiersState) -> bool {
    terminal_modifiers_for_platform(
        CURRENT_PLATFORM,
        modifiers.control_key(),
        modifiers.alt_key(),
    )
    .1
}

pub fn text_input_modifiers_allowed(modifiers: winit::keyboard::ModifiersState) -> bool {
    text_input_modifiers_allowed_for_platform(
        CURRENT_PLATFORM,
        modifiers.control_key(),
        modifiers.alt_key(),
        modifiers.super_key(),
    )
}

pub(crate) fn primary_modifier_for_platform(
    platform: PlatformKind,
    control: bool,
    super_key: bool,
) -> bool {
    if platform == PlatformKind::Macos {
        super_key
    } else {
        control
    }
}

pub(crate) fn primary_shortcut_for_platform(
    platform: PlatformKind,
    control: bool,
    alt: bool,
    super_key: bool,
) -> bool {
    // Windows reports AltGr as Ctrl+Alt. Treating that as the application
    // shortcut modifier would consume characters on many keyboard layouts.
    if platform == PlatformKind::Windows && control && alt {
        return false;
    }
    primary_modifier_for_platform(platform, control, super_key)
}

pub(crate) fn word_modifier_for_platform(
    platform: PlatformKind,
    control: bool,
    alt: bool,
) -> bool {
    match platform {
        PlatformKind::Macos => alt,
        PlatformKind::Windows => control && !alt,
        PlatformKind::Linux | PlatformKind::Other => control,
    }
}

pub(crate) fn terminal_modifiers_for_platform(
    platform: PlatformKind,
    control: bool,
    alt: bool,
) -> (bool, bool) {
    // Windows exposes AltGr as Ctrl+Alt. ConPTY must receive the composed
    // character, not Ctrl-letter plus an ESC prefix.
    if platform == PlatformKind::Windows && control && alt {
        (false, false)
    } else {
        (control, alt)
    }
}

pub(crate) fn text_input_modifiers_allowed_for_platform(
    platform: PlatformKind,
    control: bool,
    alt: bool,
    super_key: bool,
) -> bool {
    match platform {
        // AltGr is reported as Ctrl+Alt by Windows. It must remain available
        // for text input instead of being consumed as an application shortcut.
        PlatformKind::Windows => (!control && !alt && !super_key) || (control && alt && !super_key),
        // Option participates in normal text/dead-key input on macOS.
        PlatformKind::Macos => !control && !super_key,
        PlatformKind::Linux | PlatformKind::Other => !control && !alt && !super_key,
    }
}


pub fn initialize_gui_application() {
    #[cfg(windows)]
    windows::initialize_gui_application();
}

pub const fn native_dialog_requires_main_thread() -> bool {
    cfg!(target_os = "macos")
}

pub fn apply_window_attributes(attributes: WindowAttributes) -> WindowAttributes {
    #[cfg(target_os = "linux")]
    {
        use winit::platform::wayland::WindowAttributesExtWayland;
        attributes.with_name("rriter", "rriter")
    }
    #[cfg(not(target_os = "linux"))]
    {
        attributes
    }
}

#[derive(Clone, Debug, Eq)]
pub struct PathKey(Vec<u8>);

impl PathKey {
    pub fn new(path: &Path) -> Self {
        Self::for_platform(path, CURRENT_PLATFORM)
    }

    pub(crate) fn for_platform(path: &Path, platform: PlatformKind) -> Self {
        Self(normalized_path_bytes(path, platform))
    }

    fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl PartialEq for PathKey {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Hash for PathKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

pub fn paths_equal(left: &Path, right: &Path) -> bool {
    PathKey::new(left) == PathKey::new(right)
}

pub fn path_is_within(path: &Path, root: &Path) -> bool {
    path_is_within_for_platform(path, root, CURRENT_PLATFORM)
}

pub(crate) fn path_is_within_for_platform(
    path: &Path,
    root: &Path,
    platform: PlatformKind,
) -> bool {
    #[cfg(windows)]
    if platform == PlatformKind::Windows {
        return windows::path_is_within(path, root);
    }

    let path = PathKey::for_platform(path, platform);
    let root = PathKey::for_platform(root, platform);
    if path == root {
        return true;
    }
    let separator = if platform == PlatformKind::Windows {
        b'\\'
    } else {
        b'/'
    };
    let root_bytes = root.as_bytes();
    path.as_bytes().starts_with(root_bytes)
        && (root_bytes.last() == Some(&separator)
            || path.as_bytes().get(root_bytes.len()) == Some(&separator))
}

pub fn relative_to(path: &Path, root: &Path) -> Option<PathBuf> {
    if let Ok(relative) = path.strip_prefix(root) {
        return Some(relative.to_path_buf());
    }

    #[cfg(windows)]
    {
        return windows::relative_to(path, root);
    }

    #[cfg(not(windows))]
    {
        if !path_is_within(path, root) {
            return None;
        }

        let path_normalized = normalized_path_bytes(path, CURRENT_PLATFORM);
        let root_normalized = normalized_path_bytes(root, CURRENT_PLATFORM);
        let suffix = path_normalized.get(root_normalized.len()..)?;
        let suffix = suffix
            .iter()
            .position(|byte| !matches!(*byte, b'/' | b'\\'))
            .map(|start| &suffix[start..])
            .unwrap_or_default();
        normalized_bytes_to_path(suffix)
    }
}

pub fn is_absolute(path: &Path) -> bool {
    path.is_absolute()
}

pub fn canonicalize_or_absolutize(path: &Path) -> PathBuf {
    if let Ok(path) = fs::canonicalize(path) {
        #[cfg(windows)]
        return windows::without_extended_prefix(&path);
        #[cfg(target_os = "macos")]
        return macos_visible_path(&path);
        #[cfg(not(any(windows, target_os = "macos")))]
        return path;
    }
    if is_absolute(path) {
        #[cfg(windows)]
        return windows::without_extended_prefix(path);
        #[cfg(not(windows))]
        return path.to_path_buf();
    }
    std::env::current_dir()
        .unwrap_or_default()
        .join(path)
}

pub fn dedup_paths(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut seen = rustc_hash::FxHashSet::default();
    let mut unique = Vec::new();
    for path in paths {
        if seen.insert(PathKey::new(&path)) {
            unique.push(path);
        }
    }
    unique
}

fn normalized_path_bytes(path: &Path, platform: PlatformKind) -> Vec<u8> {
    if platform == PlatformKind::Windows {
        #[cfg(windows)]
        {
            return windows::normalized_path_bytes(path);
        }
        #[cfg(not(windows))]
        return normalize_windows_path(&path.to_string_lossy()).into_bytes();
    }

    #[cfg(unix)]
    if platform == CURRENT_PLATFORM {
        use std::os::unix::ffi::OsStrExt;
        let normalized = normalize_unix_path_bytes(path.as_os_str().as_bytes());
        return if platform == PlatformKind::Macos {
            normalize_macos_alias_bytes(&normalized)
        } else {
            normalized
        };
    }

    let normalized = normalize_unix_path_bytes(path.to_string_lossy().as_bytes());
    if platform == PlatformKind::Macos {
        normalize_macos_alias_bytes(&normalized)
    } else {
        normalized
    }
}

#[cfg(unix)]
fn normalized_bytes_to_path(bytes: &[u8]) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    Some(PathBuf::from(OsString::from_vec(bytes.to_vec())))
}

#[cfg(all(not(unix), not(windows)))]
fn normalized_bytes_to_path(bytes: &[u8]) -> Option<PathBuf> {
    Some(PathBuf::from(String::from_utf8(bytes.to_vec()).ok()?))
}

fn normalize_macos_alias_bytes(path: &[u8]) -> Vec<u8> {
    const ALIASES: [(&[u8], &[u8]); 3] = [
        (b"/private/var", b"/var"),
        (b"/private/tmp", b"/tmp"),
        (b"/private/etc", b"/etc"),
    ];
    for (private, visible) in ALIASES {
        if path == private {
            return visible.to_vec();
        }
        if path.starts_with(private) && path.get(private.len()) == Some(&b'/') {
            let mut normalized = Vec::with_capacity(path.len() - b"/private".len());
            normalized.extend_from_slice(visible);
            normalized.extend_from_slice(&path[private.len()..]);
            return normalized;
        }
    }
    path.to_vec()
}

#[cfg(target_os = "macos")]
fn macos_visible_path(path: &Path) -> PathBuf {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    PathBuf::from(OsString::from_vec(normalize_macos_alias_bytes(
        path.as_os_str().as_bytes(),
    )))
}

fn normalize_unix_path_bytes(raw: &[u8]) -> Vec<u8> {
    let absolute = raw.first() == Some(&b'/');
    let mut parts: Vec<&[u8]> = Vec::new();
    for part in raw.split(|byte| *byte == b'/') {
        if part.is_empty() || part == b"." {
            continue;
        }
        if part == b".." {
            if parts.last().is_some_and(|last| *last != b"..") {
                parts.pop();
            } else if !absolute {
                parts.push(part);
            }
            continue;
        }
        parts.push(part);
    }

    let mut out = Vec::with_capacity(raw.len());
    if absolute {
        out.push(b'/');
    }
    for (idx, part) in parts.iter().enumerate() {
        if idx > 0 {
            out.push(b'/');
        }
        out.extend_from_slice(part);
    }
    if out.is_empty() && absolute {
        out.push(b'/');
    }
    out
}

#[cfg(not(windows))]
fn normalize_windows_path(raw: &str) -> String {
    let mut path = raw.replace('/', "\\");
    if let Some(rest) = path.strip_prefix("\\\\?\\UNC\\") {
        path = format!("\\\\{rest}");
    } else if let Some(rest) = path.strip_prefix("\\\\?\\") {
        path = rest.to_string();
    } else if let Some(rest) = path.strip_prefix("\\??\\") {
        path = rest.to_string();
    }
    path = path.to_lowercase();

    let (prefix, rest, rooted, min_parts) = if let Some(rest) = path.strip_prefix("\\\\") {
        ("\\\\".to_string(), rest, true, 2usize)
    } else if path.as_bytes().get(1) == Some(&b':') {
        let prefix = path[..2].to_string();
        let rest = &path[2..];
        let rooted = rest.starts_with('\\');
        (prefix, rest.trim_start_matches('\\'), rooted, 0usize)
    } else if let Some(rest) = path.strip_prefix('\\') {
        ("\\".to_string(), rest, true, 0usize)
    } else {
        (String::new(), path.as_str(), false, 0usize)
    };

    let mut parts: Vec<&str> = Vec::new();
    for part in rest.split('\\') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            if parts.len() > min_parts && parts.last().is_some_and(|last| *last != "..") {
                parts.pop();
            } else if !rooted {
                parts.push(part);
            }
            continue;
        }
        parts.push(part);
    }

    let mut out = prefix;
    if rooted && !out.ends_with('\\') {
        out.push('\\');
    }
    for part in parts {
        if !out.is_empty() && !out.ends_with('\\') && !out.ends_with(':') {
            out.push('\\');
        } else if out.ends_with(':') && rooted {
            out.push('\\');
        }
        out.push_str(part);
    }
    if out.is_empty() {
        out.push('.');
    }
    out
}

pub(crate) fn windows_path_is_absolute(raw: &str) -> bool {
    let raw = raw.as_bytes();
    raw.starts_with(b"\\\\")
        || raw.starts_with(b"//")
        || (raw.len() >= 3
            && raw[0].is_ascii_alphabetic()
            && raw[1] == b':'
            && matches!(raw[2], b'\\' | b'/'))
}


pub fn encode_persisted_path(path: &Path) -> String {
    encode_persisted_path_for_platform(path, CURRENT_PLATFORM)
}

pub(crate) fn encode_persisted_path_for_platform(path: &Path, platform: PlatformKind) -> String {
    let (kind, bytes) = if platform == PlatformKind::Windows {
        ("w", path_to_utf16_bytes(path))
    } else {
        ("u", path_to_unix_bytes(path))
    };
    format!("{PATH_RECORD_PREFIX}{kind}:{}", hex_encode(&bytes))
}

pub fn decode_persisted_path(record: &str) -> Option<PathBuf> {
    let Some(encoded) = record.strip_prefix(PATH_RECORD_PREFIX) else {
        return Some(PathBuf::from(record));
    };
    let (kind, encoded) = encoded.split_once(':')?;
    let bytes = hex_decode(encoded)?;
    match kind {
        "w" => utf16_bytes_to_path(&bytes),
        "u" => unix_bytes_to_path(&bytes),
        _ => None,
    }
}

fn path_to_utf16_bytes(path: &Path) -> Vec<u8> {
    #[cfg(windows)]
    let words: Vec<u16> = {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str().encode_wide().collect()
    };
    #[cfg(not(windows))]
    let words: Vec<u16> = path.to_string_lossy().encode_utf16().collect();

    let mut bytes = Vec::with_capacity(words.len() * 2);
    for word in words {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes
}

fn utf16_bytes_to_path(bytes: &[u8]) -> Option<PathBuf> {
    if bytes.len() % 2 != 0 {
        return None;
    }
    let words = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        Some(PathBuf::from(OsString::from_wide(&words)))
    }
    #[cfg(not(windows))]
    {
        Some(PathBuf::from(String::from_utf16(&words).ok()?))
    }
}

fn path_to_unix_bytes(path: &Path) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes().to_vec()
    }
    #[cfg(not(unix))]
    {
        path.to_string_lossy().as_bytes().to_vec()
    }
}

fn unix_bytes_to_path(bytes: &[u8]) -> Option<PathBuf> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Some(PathBuf::from(OsString::from_vec(bytes.to_vec())))
    }
    #[cfg(not(unix))]
    {
        Some(PathBuf::from(String::from_utf8(bytes.to_vec()).ok()?))
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn hex_decode(encoded: &str) -> Option<Vec<u8>> {
    if encoded.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(encoded.len() / 2);
    let bytes = encoded.as_bytes();
    for pair in bytes.chunks_exact(2) {
        out.push((hex_digit(pair[0])? << 4) | hex_digit(pair[1])?);
    }
    Some(out)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub fn seal_user_secret(bytes: &[u8], purpose: &str) -> io::Result<Vec<u8>> {
    #[cfg(windows)]
    {
        let protected = windows::protect_user_secret(bytes, purpose)?;
        let mut record = Vec::with_capacity(DPAPI_RECORD_PREFIX.len() + protected.len() * 2);
        record.extend_from_slice(DPAPI_RECORD_PREFIX);
        record.extend_from_slice(hex_encode(&protected).as_bytes());
        return Ok(record);
    }
    #[cfg(target_os = "macos")]
    {
        macos::store_keychain_secret(purpose, bytes)?;
        return Ok(KEYCHAIN_RECORD_PREFIX.to_vec());
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = purpose;
        Ok(bytes.to_vec())
    }
}

pub fn open_user_secret(record: &[u8], purpose: &str) -> io::Result<Vec<u8>> {
    if record == KEYCHAIN_RECORD_PREFIX {
        #[cfg(target_os = "macos")]
        {
            return macos::load_keychain_secret(purpose);
        }
        #[cfg(not(target_os = "macos"))]
        {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "macOS Keychain record cannot be opened on this platform",
            ));
        }
    }
    let Some(encoded) = record.strip_prefix(DPAPI_RECORD_PREFIX) else {
        // Migration path for auth files created before native secure storage.
        // The next successful save replaces this plaintext record atomically.
        return Ok(record.to_vec());
    };
    let encoded = std::str::from_utf8(encoded)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let protected = hex_decode(encoded)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid DPAPI record"))?;
    #[cfg(windows)]
    {
        windows::unprotect_user_secret(&protected, purpose)
    }
    #[cfg(not(windows))]
    {
        let _ = (purpose, protected);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Windows DPAPI record cannot be opened on this platform",
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextEncoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
    Cr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextFileFormat {
    pub encoding: TextEncoding,
    pub line_ending: LineEnding,
}

impl Default for TextFileFormat {
    fn default() -> Self {
        Self {
            encoding: TextEncoding::Utf8,
            line_ending: if cfg!(windows) {
                LineEnding::CrLf
            } else {
                LineEnding::Lf
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedTextFile {
    pub text: String,
    pub format: TextFileFormat,
}

pub fn read_text_file(path: &Path) -> io::Result<DecodedTextFile> {
    let bytes = fs::read(path)?;
    decode_text_bytes(&bytes)
}

pub fn decode_text_bytes(bytes: &[u8]) -> io::Result<DecodedTextFile> {
    let (encoding, raw_text) = if let Some(bytes) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
        (
            TextEncoding::Utf8Bom,
            std::str::from_utf8(bytes)
                .map_err(invalid_text_error)?
                .to_string(),
        )
    } else if let Some(bytes) = bytes.strip_prefix(&[0xff, 0xfe]) {
        (TextEncoding::Utf16Le, decode_utf16(bytes, true)?)
    } else if let Some(bytes) = bytes.strip_prefix(&[0xfe, 0xff]) {
        (TextEncoding::Utf16Be, decode_utf16(bytes, false)?)
    } else {
        (
            TextEncoding::Utf8,
            std::str::from_utf8(bytes)
                .map_err(invalid_text_error)?
                .to_string(),
        )
    };
    let line_ending = detect_line_ending(&raw_text);
    Ok(DecodedTextFile {
        text: normalize_line_endings(&raw_text),
        format: TextFileFormat {
            encoding,
            line_ending,
        },
    })
}

fn invalid_text_error(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("unsupported or invalid text encoding: {error}"),
    )
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> io::Result<String> {
    if bytes.len() % 2 != 0 {
        return Err(invalid_text_error("odd UTF-16 byte length"));
    }
    let words = bytes.chunks_exact(2).map(|pair| {
        if little_endian {
            u16::from_le_bytes([pair[0], pair[1]])
        } else {
            u16::from_be_bytes([pair[0], pair[1]])
        }
    });
    std::char::decode_utf16(words)
        .map(|item| item.map_err(invalid_text_error))
        .collect()
}

fn detect_line_ending(text: &str) -> LineEnding {
    let bytes = text.as_bytes();
    let mut crlf = 0usize;
    let mut lf = 0usize;
    let mut cr = 0usize;
    let mut idx = 0usize;
    while idx < bytes.len() {
        match bytes[idx] {
            b'\r' if bytes.get(idx + 1) == Some(&b'\n') => {
                crlf += 1;
                idx += 2;
            }
            b'\r' => {
                cr += 1;
                idx += 1;
            }
            b'\n' => {
                lf += 1;
                idx += 1;
            }
            _ => idx += 1,
        }
    }
    if crlf >= lf && crlf >= cr && crlf > 0 {
        LineEnding::CrLf
    } else if lf >= cr && lf > 0 {
        LineEnding::Lf
    } else if cr > 0 {
        LineEnding::Cr
    } else {
        TextFileFormat::default().line_ending
    }
}

fn normalize_line_endings(text: &str) -> String {
    if !text.as_bytes().contains(&b'\r') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            out.push('\n');
        } else {
            out.push(ch);
        }
    }
    out
}

pub fn encode_text(text: &str, format: TextFileFormat) -> Vec<u8> {
    let external_text = match format.line_ending {
        LineEnding::Lf => text.to_string(),
        LineEnding::CrLf => text.replace('\n', "\r\n"),
        LineEnding::Cr => text.replace('\n', "\r"),
    };
    match format.encoding {
        TextEncoding::Utf8 => external_text.into_bytes(),
        TextEncoding::Utf8Bom => {
            let mut bytes = Vec::with_capacity(external_text.len() + 3);
            bytes.extend_from_slice(&[0xef, 0xbb, 0xbf]);
            bytes.extend_from_slice(external_text.as_bytes());
            bytes
        }
        TextEncoding::Utf16Le | TextEncoding::Utf16Be => {
            let little_endian = format.encoding == TextEncoding::Utf16Le;
            let mut bytes = Vec::with_capacity(external_text.len() * 2 + 2);
            bytes.extend_from_slice(if little_endian {
                &[0xff, 0xfe]
            } else {
                &[0xfe, 0xff]
            });
            for word in external_text.encode_utf16() {
                let encoded = if little_endian {
                    word.to_le_bytes()
                } else {
                    word.to_be_bytes()
                };
                bytes.extend_from_slice(&encoded);
            }
            bytes
        }
    }
}

pub fn write_text_file(path: &Path, text: &str, format: TextFileFormat) -> io::Result<()> {
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty())
        && !parent.is_dir()
    {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("parent directory does not exist: {}", parent.display()),
        ));
    }
    atomic_write(path, &encode_text(text, format))
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    atomic_write_impl(path, bytes, false)
}

pub fn atomic_write_secret(path: &Path, bytes: &[u8]) -> io::Result<()> {
    atomic_write_impl(path, bytes, true)
}

fn atomic_write_impl(path: &Path, bytes: &[u8], secret: bool) -> io::Result<()> {
    // Saving through a symlink must update its target instead of replacing the
    // link itself with a regular file. This preserves the pre-atomic-save
    // behavior on Linux and the equivalent reparse-point behavior on Windows.
    let resolved_path = fs::symlink_metadata(path)
        .ok()
        .filter(metadata_is_link)
        .map(|_| fs::canonicalize(path))
        .transpose()?;
    let path = resolved_path.as_deref().unwrap_or(path);
    let parent = path.parent().filter(|path| !path.as_os_str().is_empty());
    if let Some(parent) = parent {
        fs::create_dir_all(parent)?;
    }

    let (temp_path, mut file) = create_atomic_temp_file(path, secret)?;
    let result = (|| {
        if secret {
            set_secret_permissions(&file)?;
        } else if let Ok(metadata) = fs::metadata(path) {
            let _ = file.set_permissions(metadata.permissions());
        }
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        replace_file_with_retry(&temp_path, path)?;
        sync_parent_directory(parent);
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn set_secret_permissions(file: &File) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))
    }
    #[cfg(not(unix))]
    {
        let _ = file;
        Ok(())
    }
}

fn create_atomic_temp_file(path: &Path, secret: bool) -> io::Result<(PathBuf, File)> {
    let mut last_error = None;
    for _ in 0..64 {
        let temp_path = temporary_sibling_path(path);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        if secret {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        #[cfg(not(unix))]
        let _ = secret;
        match options.open(&temp_path) {
            Ok(file) => return Ok((temp_path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                last_error = Some(error);
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "failed to allocate a unique atomic-save temporary file",
        )
    }))
}

fn temporary_sibling_path(path: &Path) -> PathBuf {
    let name = OsString::from(format!(".rriter-save-{}.tmp", next_operation_id()));
    path.with_file_name(name)
}

fn replace_file_with_retry(temp_path: &Path, target_path: &Path) -> io::Result<()> {
    let attempts = if cfg!(windows) { 5 } else { 1 };
    let mut last_error = None;
    for attempt in 0..attempts {
        match replace_file(temp_path, target_path) {
            Ok(()) => return Ok(()),
            Err(error) => {
                let retry = should_retry_replace(&error);
                last_error = Some(error);
                if !retry || attempt + 1 == attempts {
                    break;
                }
                std::thread::sleep(Duration::from_millis(12 * (attempt as u64 + 1)));
            }
        }
    }
    Err(last_error.expect("replace loop always executes"))
}

fn should_retry_replace(error: &io::Error) -> bool {
    #[cfg(windows)]
    {
        matches!(error.raw_os_error(), Some(5 | 32 | 33))
    }
    #[cfg(not(windows))]
    {
        let _ = error;
        false
    }
}

#[cfg(windows)]
fn replace_file(temp_path: &Path, target_path: &Path) -> io::Result<()> {
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        REPLACEFILE_IGNORE_MERGE_ERRORS, REPLACEFILE_WRITE_THROUGH, ReplaceFileW,
    };

    let temp = windows_wide_path(temp_path);
    let target = windows_wide_path(target_path);
    let replaced = if path_entry_exists(target_path) {
        unsafe {
            ReplaceFileW(
                target.as_ptr(),
                temp.as_ptr(),
                std::ptr::null(),
                REPLACEFILE_IGNORE_MERGE_ERRORS | REPLACEFILE_WRITE_THROUGH,
                std::ptr::null(),
                std::ptr::null(),
            )
        }
    } else {
        0
    };
    if replaced != 0 {
        return Ok(());
    }

    let moved = unsafe {
        MoveFileExW(
            temp.as_ptr(),
            target.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved != 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(windows)]
fn windows_wide_path(path: &Path) -> Vec<u16> {
    windows::extended_length_path(&canonicalize_or_absolutize(path))
}

#[cfg(not(windows))]
fn replace_file(temp_path: &Path, target_path: &Path) -> io::Result<()> {
    fs::rename(temp_path, target_path)
}

#[cfg(unix)]
fn sync_parent_directory(parent: Option<&Path>) {
    if let Some(parent) = parent {
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_all();
        }
    }
}

#[cfg(not(unix))]
fn sync_parent_directory(_parent: Option<&Path>) {}

pub fn validate_child_name(name: &str) -> Result<(), &'static str> {
    validate_child_name_for_platform(name, CURRENT_PLATFORM)
}

pub(crate) fn validate_child_name_for_platform(
    name: &str,
    platform: PlatformKind,
) -> Result<(), &'static str> {
    if name.is_empty() || name == "." || name == ".." {
        return Err("name is empty or reserved");
    }
    if name.contains('/') || name.contains('\\') || name.contains('\0') {
        return Err("name contains a path separator");
    }
    if platform != PlatformKind::Windows {
        return Ok(());
    }
    if name.chars().any(|ch| {
        ch <= '\u{1f}' || matches!(ch, '<' | '>' | ':' | '"' | '|' | '?' | '*')
    }) {
        return Err("name contains a character forbidden by Windows");
    }
    if name.ends_with(['.', ' ']) {
        return Err("Windows names cannot end with a dot or space");
    }
    let base = name
        .split('.')
        .next()
        .unwrap_or(name)
        .trim_end_matches(['.', ' '])
        .to_ascii_uppercase();
    let reserved = matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || base
            .strip_prefix("COM")
            .or_else(|| base.strip_prefix("LPT"))
            .is_some_and(|suffix| {
                suffix.len() == 1 && matches!(suffix.as_bytes()[0], b'1'..=b'9')
            });
    if reserved {
        return Err("name is reserved by Windows");
    }
    Ok(())
}

pub fn is_cross_device_error(error: &io::Error) -> bool {
    if error.kind() == io::ErrorKind::CrossesDevices {
        return true;
    }
    #[cfg(windows)]
    {
        // ERROR_NOT_SAME_DEVICE
        error.raw_os_error() == Some(17)
    }
    #[cfg(unix)]
    {
        // EXDEV on the supported Unix targets.
        error.raw_os_error() == Some(18)
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

pub fn metadata_is_link(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

pub fn path_entry_exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

pub fn remove_path_entry(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata_is_link(&metadata) {
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;
            if metadata.file_attributes() & FILE_ATTRIBUTE_DIRECTORY != 0 {
                return fs::remove_dir(path);
            }
        }
        return fs::remove_file(path);
    }
    if metadata.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

pub fn rename_path(source: &Path, destination: &Path) -> io::Result<()> {
    if source == destination {
        return Ok(());
    }
    if paths_equal(source, destination) {
        let parent = source.parent().unwrap_or_else(|| Path::new("."));
        let temp = (0..64)
            .map(|_| {
                let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
                parent.join(format!(
                    ".rriter-case-rename-{}-{counter}",
                    std::process::id()
                ))
            })
            .find(|candidate| !path_entry_exists(candidate))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "failed to allocate a temporary path for case-only rename",
                )
            })?;
        fs::rename(source, &temp)?;
        match fs::rename(&temp, destination) {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = fs::rename(&temp, source);
                Err(error)
            }
        }
    } else {
        fs::rename(source, destination)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrashLayout {
    pub files_dir: PathBuf,
    pub info_dir: PathBuf,
    pub freedesktop: bool,
}

pub fn trash_layout() -> TrashLayout {
    match CURRENT_PLATFORM {
        PlatformKind::Linux => {
            let data_home = std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or_else(|| {
                    std::env::var_os("HOME")
                        .map(PathBuf::from)
                        .unwrap_or_default()
                        .join(".local")
                        .join("share")
                });
            let root = data_home.join("Trash");
            TrashLayout {
                files_dir: root.join("files"),
                info_dir: root.join("info"),
                freedesktop: true,
            }
        }
        PlatformKind::Windows | PlatformKind::Macos | PlatformKind::Other => {
            let root = state_dir().join("trash");
            TrashLayout {
                files_dir: root.join("files"),
                info_dir: root.join("info"),
                freedesktop: false,
            }
        }
    }
}

pub struct Clipboard {
    inner: arboard::Clipboard,
}

impl Clipboard {
    pub fn new() -> Result<Self, arboard::Error> {
        arboard::Clipboard::new().map(|inner| Self { inner })
    }

    pub fn set_text(&mut self, text: String) -> Result<(), arboard::Error> {
        clipboard_retry(|| self.inner.set_text(text.clone()))
    }

    pub fn get_text(&mut self) -> Result<String, arboard::Error> {
        clipboard_retry(|| self.inner.get_text())
    }
}

fn clipboard_retry<T>(
    mut action: impl FnMut() -> Result<T, arboard::Error>,
) -> Result<T, arboard::Error> {
    let attempts = if cfg!(windows) {
        CLIPBOARD_RETRY_COUNT
    } else {
        1
    };
    let mut last_error = None;
    for attempt in 0..attempts {
        match action() {
            Ok(value) => return Ok(value),
            Err(error) => last_error = Some(error),
        }
        if attempt + 1 < attempts {
            std::thread::sleep(CLIPBOARD_RETRY_DELAY);
        }
    }
    Err(last_error.expect("clipboard retry loop always executes"))
}

pub fn pick_file(title: &str) -> Option<PathBuf> {
    rfd::FileDialog::new().set_title(title).pick_file()
}

pub fn pick_file_with_filter(
    title: &str,
    filter_name: &str,
    extensions: &[&str],
) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title(title)
        .add_filter(filter_name, extensions)
        .pick_file()
}

pub fn pick_files(title: &str) -> Vec<PathBuf> {
    rfd::FileDialog::new()
        .set_title(title)
        .pick_files()
        .unwrap_or_default()
}

pub fn pick_folder(title: &str) -> Option<PathBuf> {
    rfd::FileDialog::new().set_title(title).pick_folder()
}

pub fn save_file(title: &str, file_name: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title(title)
        .set_file_name(file_name)
        .save_file()
}

pub fn save_file_with_filter(
    title: &str,
    file_name: &str,
    filter_name: &str,
    extensions: &[&str],
) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title(title)
        .set_file_name(file_name)
        .add_filter(filter_name, extensions)
        .save_file()
}

pub fn reveal_path(path: &Path) -> io::Result<Child> {
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
        return macos::reveal_path(path);
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

pub fn open_url(url: &str) -> io::Result<()> {
    let parsed = url::Url::parse(url)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "only http and https URLs may be opened",
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


pub fn copy_symlink(source: &Path, destination: &Path) -> io::Result<()> {
    let target = fs::read_link(source)?;
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, destination)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;
        let target_is_dir = fs::symlink_metadata(source)
            .map(|metadata| metadata.file_attributes() & FILE_ATTRIBUTE_DIRECTORY != 0)
            .or_else(|_| fs::metadata(source).map(|metadata| metadata.is_dir()))
            .unwrap_or(false);
        if target_is_dir {
            std::os::windows::fs::symlink_dir(target, destination)
        } else {
            std::os::windows::fs::symlink_file(target, destination)
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (target, destination);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "symbolic links are not supported on this platform",
        ))
    }
}

#[cfg(test)]
mod tests;
