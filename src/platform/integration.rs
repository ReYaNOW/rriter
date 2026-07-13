use super::{PlatformKind, CURRENT_PLATFORM, APP_DIR_NAME, resolve_executable};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, RwLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolKind {
    Git,
    Ruff,
    Ty,
    Uv,
    Python,
    Shell,
}

impl ToolKind {
    pub const ALL: [Self; 6] = [
        Self::Git,
        Self::Ruff,
        Self::Ty,
        Self::Uv,
        Self::Python,
        Self::Shell,
    ];

    pub const fn index(self) -> usize {
        match self {
            Self::Git => 0,
            Self::Ruff => 1,
            Self::Ty => 2,
            Self::Uv => 3,
            Self::Python => 4,
            Self::Shell => 5,
        }
    }

    pub const fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::Git),
            1 => Some(Self::Ruff),
            2 => Some(Self::Ty),
            3 => Some(Self::Uv),
            4 => Some(Self::Python),
            5 => Some(Self::Shell),
            _ => None,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Git => "Git",
            Self::Ruff => "Ruff",
            Self::Ty => "Ty",
            Self::Uv => "uv",
            Self::Python => "Python",
            Self::Shell => "Терминал",
        }
    }

    pub const fn config_key(self) -> &'static str {
        match self {
            Self::Git => "git",
            Self::Ruff => "ruff",
            Self::Ty => "ty",
            Self::Uv => "uv",
            Self::Python => "python",
            Self::Shell => "shell",
        }
    }

    pub const fn override_env(self) -> &'static str {
        match self {
            Self::Git => "RRITER_GIT_PATH",
            Self::Ruff => "RRITER_RUFF_PATH",
            Self::Ty => "RRITER_TY_PATH",
            Self::Uv => "RRITER_UV_PATH",
            Self::Python => "RRITER_PYTHON_PATH",
            Self::Shell => "RRITER_SHELL",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolPaths {
    paths: [Option<PathBuf>; 6],
}

impl ToolPaths {
    pub fn get(&self, kind: ToolKind) -> Option<&Path> {
        self.paths[kind.index()].as_deref()
    }

    pub fn set(&mut self, kind: ToolKind, path: Option<PathBuf>) {
        self.paths[kind.index()] = path.filter(|path| !path.as_os_str().is_empty());
    }

    pub fn iter(&self) -> impl Iterator<Item = (ToolKind, Option<&Path>)> {
        ToolKind::ALL
            .into_iter()
            .map(|kind| (kind, self.get(kind)))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolPathSource {
    Environment,
    Settings,
    Path,
}

impl ToolPathSource {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Environment => "RRITER_*_PATH",
            Self::Settings => "настройки",
            Self::Path => "PATH",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolResolution {
    pub path: Option<PathBuf>,
    pub configured_path: Option<PathBuf>,
    pub source: Option<ToolPathSource>,
}

impl ToolResolution {
    pub fn is_ready(&self) -> bool {
        self.path.is_some()
    }

    pub fn is_invalid_override(&self) -> bool {
        self.path.is_none() && self.configured_path.is_some()
    }
}

static TOOL_PATHS: LazyLock<RwLock<ToolPaths>> =
    LazyLock::new(|| RwLock::new(ToolPaths::default()));
static TOOL_RESOLUTION_CACHE: LazyLock<RwLock<[Option<ToolResolution>; 6]>> =
    LazyLock::new(|| RwLock::new(std::array::from_fn(|_| None)));

pub fn refresh_tool_resolutions() {
    if let Ok(mut cache) = TOOL_RESOLUTION_CACHE.write() {
        *cache = std::array::from_fn(|_| None);
    }
}

pub fn configure_tool_paths(paths: ToolPaths) {
    if let Ok(mut configured) = TOOL_PATHS.write() {
        *configured = paths;
    }
    refresh_tool_resolutions();
}

pub fn configured_tool_path(kind: ToolKind) -> Option<PathBuf> {
    TOOL_PATHS
        .read()
        .ok()
        .and_then(|configured| configured.get(kind).map(Path::to_path_buf))
}

pub(crate) fn configured_tool_path_for_env(override_env: &str) -> Option<PathBuf> {
    ToolKind::ALL
        .into_iter()
        .find(|kind| kind.override_env() == override_env)
        .and_then(configured_tool_path)
}

pub fn resolve_tool_kind(kind: ToolKind) -> ToolResolution {
    if let Ok(cache) = TOOL_RESOLUTION_CACHE.read()
        && let Some(resolution) = cache[kind.index()].as_ref()
    {
        return resolution.clone();
    }
    let resolution = resolve_tool_kind_uncached(kind);
    if let Ok(mut cache) = TOOL_RESOLUTION_CACHE.write() {
        cache[kind.index()] = Some(resolution.clone());
    }
    resolution
}

fn resolve_tool_kind_uncached(kind: ToolKind) -> ToolResolution {
    if let Some(path) = std::env::var_os(kind.override_env()).filter(|value| !value.is_empty()) {
        let configured_path = PathBuf::from(path);
        return ToolResolution {
            path: resolve_executable(configured_path.as_os_str()),
            configured_path: Some(configured_path),
            source: Some(ToolPathSource::Environment),
        };
    }
    if let Some(configured_path) = configured_tool_path(kind) {
        return ToolResolution {
            path: resolve_executable(configured_path.as_os_str()),
            configured_path: Some(configured_path),
            source: Some(ToolPathSource::Settings),
        };
    }

    let candidates: &[&str] = match (kind, CURRENT_PLATFORM) {
        (ToolKind::Git, _) => &["git"],
        (ToolKind::Ruff, _) => &["ruff"],
        (ToolKind::Ty, _) => &["ty"],
        (ToolKind::Uv, _) => &["uv"],
        (ToolKind::Python, PlatformKind::Windows) => &["py.exe", "python.exe", "python3.exe"],
        (ToolKind::Python, _) => &["python3", "python"],
        (ToolKind::Shell, PlatformKind::Windows) => &["pwsh.exe", "powershell.exe", "cmd.exe"],
        (ToolKind::Shell, PlatformKind::Macos) => &["/bin/zsh", "/bin/bash", "/bin/sh"],
        (ToolKind::Shell, _) => &["/bin/bash", "/bin/sh"],
    };
    let path = candidates
        .iter()
        .find_map(|candidate| resolve_executable(OsStr::new(candidate)));
    ToolResolution {
        source: path.as_ref().map(|_| ToolPathSource::Path),
        path,
        configured_path: None,
    }
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppPaths {
    pub config: PathBuf,
    pub data: PathBuf,
    pub cache: PathBuf,
    pub state: PathBuf,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct SystemProxyConfig {
    pub all: Option<String>,
    pub http: Option<String>,
    pub https: Option<String>,
    pub bypass: Option<String>,
}

impl SystemProxyConfig {
    #[cfg(any(windows, test))]
    pub fn is_empty(&self) -> bool {
        self.all.is_none() && self.http.is_none() && self.https.is_none()
    }
}

pub fn app_paths() -> AppPaths {
    app_paths_with(CURRENT_PLATFORM, |name| std::env::var_os(name))
}

#[cfg_attr(test, allow(dead_code))]
pub fn config_dir() -> PathBuf {
    app_paths().config
}

#[allow(dead_code)]
pub fn data_dir() -> PathBuf {
    app_paths().data
}

#[allow(dead_code)]
pub fn cache_dir() -> PathBuf {
    app_paths().cache
}

pub fn state_dir() -> PathBuf {
    app_paths().state
}

/// Returns the platform's shared user cache root rather than RRiter's own
/// cache directory. Third-party tools such as `ty` keep their caches here.
pub fn user_cache_root() -> PathBuf {
    user_cache_root_with(CURRENT_PLATFORM, |name| std::env::var_os(name))
}

pub(crate) fn user_cache_root_with(
    platform: PlatformKind,
    mut env_value: impl FnMut(&str) -> Option<OsString>,
) -> PathBuf {
    let home = env_value("HOME")
        .or_else(|| env_value("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from("."));

    match platform {
        PlatformKind::Windows => env_value("LOCALAPPDATA")
            .map(PathBuf::from)
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| home.join("AppData").join("Local")),
        PlatformKind::Macos => home.join("Library").join("Caches"),
        PlatformKind::Linux | PlatformKind::Other => env_value("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| home.join(".cache")),
    }
}

fn explicit_proxy_environment_configured() -> bool {
    [
        "ALL_PROXY",
        "all_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
    ]
    .iter()
    .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
}

pub fn system_proxy_config() -> Option<SystemProxyConfig> {
    // Reqwest already honors explicit proxy environment variables. Native
    // Windows settings are only a fallback so a user override always wins.
    if explicit_proxy_environment_configured() {
        return None;
    }

    #[cfg(windows)]
    {
        let (proxy, bypass) = super::windows::raw_system_proxy_config()?;
        return parse_windows_proxy_config(&proxy, bypass.as_deref());
    }
    #[cfg(target_os = "macos")]
    {
        return super::macos::system_proxy_config();
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        None
    }
}

pub fn proxy_routing_is_configured() -> bool {
    explicit_proxy_environment_configured() || system_proxy_config().is_some()
}

#[cfg(any(windows, test))]
pub(crate) fn parse_windows_proxy_config(
    proxy: &str,
    bypass: Option<&str>,
) -> Option<SystemProxyConfig> {
    fn normalized_proxy_url(value: &str) -> Option<String> {
        let value = value.trim();
        if value.is_empty() {
            return None;
        }
        if value.contains("://") {
            Some(value.to_string())
        } else {
            Some(format!("http://{value}"))
        }
    }

    let mut config = SystemProxyConfig::default();
    let mut tokens = Vec::new();
    for segment in proxy.split(';') {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        if segment.matches('=').count() > 1 || (segment.contains('=') && segment.contains(' ')) {
            tokens.extend(segment.split_whitespace().filter(|part| !part.is_empty()));
        } else {
            tokens.push(segment);
        }
    }
    for token in tokens {
        if let Some((kind, value)) = token.split_once('=') {
            let value = normalized_proxy_url(value);
            match kind.trim().to_ascii_lowercase().as_str() {
                "http" => config.http = value,
                "https" => config.https = value,
                "proxy" | "all" | "socks" => config.all = value,
                _ => {}
            }
        } else if config.all.is_none() {
            config.all = normalized_proxy_url(token);
        }
    }
    config.bypass = bypass
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.replace(';', ","));
    (!config.is_empty()).then_some(config)
}

#[cfg(any(target_os = "macos", test))]
pub(crate) fn parse_macos_proxy_config(output: &str) -> Option<SystemProxyConfig> {
    fn value<'a>(output: &'a str, key: &str) -> Option<&'a str> {
        output.lines().find_map(|line| {
            let (found, value) = line.trim().split_once(" : ")?;
            (found == key).then_some(value.trim())
        })
    }
    fn enabled(output: &str, key: &str) -> bool {
        value(output, key) == Some("1")
    }
    fn endpoint(output: &str, prefix: &str) -> Option<String> {
        if !enabled(output, &format!("{prefix}Enable")) {
            return None;
        }
        let host = value(output, &format!("{prefix}Proxy"))?;
        let port = value(output, &format!("{prefix}Port"));
        Some(match port {
            Some(port) => format!("http://{host}:{port}"),
            None => format!("http://{host}"),
        })
    }

    let http = endpoint(output, "HTTP");
    let https = endpoint(output, "HTTPS");
    let bypass = parse_macos_proxy_exceptions(output);
    (http.is_some() || https.is_some()).then_some(SystemProxyConfig {
        all: None,
        http,
        https,
        bypass,
    })
}

#[cfg(any(target_os = "macos", test))]
fn parse_macos_proxy_exceptions(output: &str) -> Option<String> {
    let mut inside = false;
    let mut values = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("ExceptionsList : <array>") {
            inside = true;
            continue;
        }
        if inside && trimmed == "}" {
            break;
        }
        if inside {
            if let Some((_, value)) = trimmed.split_once(" : ") {
                values.push(value.trim().to_string());
            }
        }
    }
    (!values.is_empty()).then(|| values.join(","))
}

#[cfg(any(target_os = "macos", test))]
pub(crate) fn parse_pem_certificates(input: &[u8]) -> Vec<Vec<u8>> {
    const BEGIN: &[u8] = b"-----BEGIN CERTIFICATE-----";
    const END: &[u8] = b"-----END CERTIFICATE-----";
    let mut certificates = Vec::new();
    let mut remaining = input;
    while let Some(start) = find_bytes(remaining, BEGIN) {
        remaining = &remaining[start + BEGIN.len()..];
        let Some(end) = find_bytes(remaining, END) else {
            break;
        };
        if let Some(der) = decode_base64(&remaining[..end]) {
            certificates.push(der);
        }
        remaining = &remaining[end + END.len()..];
    }
    certificates
}

#[cfg(any(target_os = "macos", test))]
fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

#[cfg(any(target_os = "macos", test))]
fn decode_base64(input: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut chunk = [0_u8; 4];
    let mut len = 0;
    for byte in input.iter().copied().filter(|byte| !byte.is_ascii_whitespace()) {
        chunk[len] = byte;
        len += 1;
        if len != 4 {
            continue;
        }
        let a = base64_value(chunk[0])?;
        let b = base64_value(chunk[1])?;
        let c = (chunk[2] != b'=').then(|| base64_value(chunk[2])).flatten();
        let d = (chunk[3] != b'=').then(|| base64_value(chunk[3])).flatten();
        out.push((a << 2) | (b >> 4));
        if let Some(c) = c {
            out.push((b << 4) | (c >> 2));
            if let Some(d) = d {
                out.push((c << 6) | d);
            }
        }
        len = 0;
    }
    (len == 0).then_some(out)
}

#[cfg(any(target_os = "macos", test))]
fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

static NATIVE_ROOT_CERTIFICATES: LazyLock<Vec<Vec<u8>>> = LazyLock::new(|| {
    #[cfg(windows)]
    {
        super::windows::native_root_certificates_der().unwrap_or_default()
    }
    #[cfg(target_os = "macos")]
    {
        super::macos::native_root_certificates_der().unwrap_or_default()
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        Vec::new()
    }
});

/// Additional certificates from the native OS trust store. Reqwest's bundled
/// WebPKI roots stay enabled; these roots add corporate and user-installed CAs
/// on Windows without replacing the portable baseline.
pub fn native_root_certificates_der() -> &'static [Vec<u8>] {
    NATIVE_ROOT_CERTIFICATES.as_slice()
}

pub fn current_process_memory_kb() -> Option<usize> {
    #[cfg(windows)]
    {
        return super::windows::current_process_memory_kb();
    }
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        return status.lines().find_map(|line| {
            let rest = line.strip_prefix("VmRSS:")?;
            rest.split_whitespace().next()?.parse::<usize>().ok()
        });
    }
    #[cfg(target_os = "macos")]
    {
        return super::macos::current_process_memory_kb();
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

pub(crate) fn app_paths_with(
    platform: PlatformKind,
    mut env_value: impl FnMut(&str) -> Option<OsString>,
) -> AppPaths {
    let home = env_value("HOME")
        .or_else(|| env_value("USERPROFILE"))
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from("."));

    match platform {
        PlatformKind::Windows => {
            let roaming = env_value("APPDATA")
                .map(PathBuf::from)
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or_else(|| home.join("AppData").join("Roaming"));
            let local = env_value("LOCALAPPDATA")
                .map(PathBuf::from)
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or_else(|| home.join("AppData").join("Local"));
            let local_app = local.join(APP_DIR_NAME);
            AppPaths {
                config: roaming.join(APP_DIR_NAME),
                data: local_app.clone(),
                cache: local_app.join("cache"),
                state: local_app.join("state"),
            }
        }
        PlatformKind::Macos => {
            let app_support = home.join("Library").join("Application Support");
            AppPaths {
                config: app_support.join(APP_DIR_NAME),
                data: app_support.join(APP_DIR_NAME),
                cache: home.join("Library").join("Caches").join(APP_DIR_NAME),
                state: app_support.join(APP_DIR_NAME).join("state"),
            }
        }
        PlatformKind::Linux | PlatformKind::Other => {
            let config_root = env_value("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or_else(|| home.join(".config"));
            let data_root = env_value("XDG_DATA_HOME")
                .map(PathBuf::from)
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or_else(|| home.join(".local").join("share"));
            let cache_root = env_value("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or_else(|| home.join(".cache"));
            let state_root = env_value("XDG_STATE_HOME")
                .map(PathBuf::from)
                .filter(|path| !path.as_os_str().is_empty())
                .unwrap_or_else(|| home.join(".local").join("state"));
            AppPaths {
                config: config_root.join(APP_DIR_NAME),
                data: data_root.join(APP_DIR_NAME),
                cache: cache_root.join(APP_DIR_NAME),
                state: state_root.join(APP_DIR_NAME),
            }
        }
    }
}
