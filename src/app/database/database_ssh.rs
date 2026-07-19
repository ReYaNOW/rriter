use super::{DatabaseSecretBundle, SshConnectionConfig, SshJumpHostConfig};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Read};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // Trust actions are exposed by the host-key dialog in stage 3.
pub enum SshHostKeyPolicy {
    #[default]
    Strict,
    TrustOnce,
    TrustAndStore,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SshBackendKind {
    System,
    Builtin,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SshBackendSelection {
    pub kind: SshBackendKind,
    pub reason: Option<String>,
    pub executable: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct SshConnectOptions {
    pub host_key_policy: SshHostKeyPolicy,
    pub force_builtin: bool,
    pub(crate) cancel: Option<Arc<AtomicBool>>,
}

impl Default for SshConnectOptions {
    fn default() -> Self {
        Self {
            host_key_policy: SshHostKeyPolicy::Strict,
            force_builtin: false,
            cancel: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedSshEndpoint {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub private_key_path: Option<PathBuf>,
    pub proxy_jump: Option<String>,
}

pub fn select_ssh_backend(
    config: &SshConnectionConfig,
    secrets: &DatabaseSecretBundle,
    options: &SshConnectOptions,
) -> SshBackendSelection {
    if options.force_builtin {
        return SshBackendSelection {
            kind: SshBackendKind::Builtin,
            reason: Some("встроенный SSH был явно выбран для этого подключения".to_string()),
            executable: None,
        };
    }
    if secrets.ssh_password.is_some() || secrets.jump_password.is_some() {
        return SshBackendSelection {
            kind: SshBackendKind::Builtin,
            reason: Some(
                "SSH-пароль нельзя безопасно передать системному OpenSSH из встроенного окна"
                    .to_string(),
            ),
            executable: None,
        };
    }
    if secrets.ssh_key_passphrase.is_some() || secrets.jump_key_passphrase.is_some() {
        return SshBackendSelection {
            kind: SshBackendKind::Builtin,
            reason: Some(
                "passphrase закрытого ключа нельзя безопасно передать системному OpenSSH"
                    .to_string(),
            ),
            executable: None,
        };
    }
    if config
        .jump_host
        .as_ref()
        .and_then(|jump| jump.private_key_path.as_ref())
        .is_some()
    {
        return SshBackendSelection {
            kind: SshBackendKind::Builtin,
            reason: Some(
                "отдельный закрытый ключ jump host требует встроенного SSH".to_string(),
            ),
            executable: None,
        };
    }
    match resolve_system_ssh() {
        Some(executable) => SshBackendSelection {
            kind: SshBackendKind::System,
            reason: None,
            executable: Some(executable),
        },
        None => SshBackendSelection {
            kind: SshBackendKind::Builtin,
            reason: Some("системный OpenSSH не найден".to_string()),
            executable: None,
        },
    }
}

pub fn resolve_system_ssh() -> Option<PathBuf> {
    if let Some(path) = crate::platform::resolve_executable(OsStr::new("ssh")) {
        return Some(path);
    }
    #[cfg(windows)]
    {
        for path in windows_ssh_candidates() {
            if path.is_file() {
                return Some(path);
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let path = PathBuf::from("/usr/bin/ssh");
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

#[cfg(any(windows, test))]
pub(crate) fn windows_ssh_candidates_with(system_root: &Path) -> [PathBuf; 2] {
    [
        system_root.join("System32").join("OpenSSH").join("ssh.exe"),
        system_root.join("Sysnative").join("OpenSSH").join("ssh.exe"),
    ]
}

#[cfg(windows)]
fn windows_ssh_candidates() -> Vec<PathBuf> {
    let root = std::env::var_os("SystemRoot")
        .or_else(|| std::env::var_os("WINDIR"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    windows_ssh_candidates_with(&root).into_iter().collect()
}

pub struct SystemSshTunnel {
    child: crate::platform::ManagedChild,
    stderr: Arc<Mutex<Vec<u8>>>,
    stderr_reader: Option<thread::JoinHandle<()>>,
    local_port: u16,
}

impl SystemSshTunnel {
    pub fn local_port(&self) -> u16 {
        self.local_port
    }

    pub fn stderr_snapshot(&self) -> String {
        let bytes = self.stderr.lock().map(|bytes| bytes.clone()).unwrap_or_default();
        String::from_utf8_lossy(&bytes).trim().to_string()
    }

    pub fn terminate(&mut self) -> io::Result<()> {
        self.child.terminate(Duration::from_millis(500))
    }
}

impl Drop for SystemSshTunnel {
    fn drop(&mut self) {
        let _ = self.child.terminate(Duration::from_millis(500));
        if let Some(reader) = self.stderr_reader.take() {
            let _ = reader.join();
        }
    }
}

pub(crate) fn start_system_ssh_tunnel_cancelable(
    executable: &Path,
    config: &SshConnectionConfig,
    postgres_host: &str,
    postgres_port: u16,
    startup_timeout: Duration,
    cancel: Option<&AtomicBool>,
) -> io::Result<SystemSshTunnel> {
    let local_port = reserve_local_port()?;
    let args = system_ssh_args(config, postgres_host, postgres_port, local_port)?;
    let mut command = Command::new(executable);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = crate::platform::ManagedChild::spawn(&mut command)?;
    let stderr_pipe = child
        .take_stderr()
        .ok_or_else(|| io::Error::other("system SSH stderr was not piped"))?;
    let stderr = Arc::new(Mutex::new(Vec::new()));
    let stderr_reader = Some(spawn_bounded_stderr_reader(stderr_pipe, Arc::clone(&stderr))?);
    let mut tunnel = SystemSshTunnel {
        child,
        stderr,
        stderr_reader,
        local_port,
    };

    let deadline = Instant::now() + startup_timeout;
    loop {
        if cancel.is_some_and(|cancel| cancel.load(Ordering::Acquire)) {
            let _ = tunnel.terminate();
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "system SSH tunnel startup was cancelled",
            ));
        }
        if TcpStream::connect(("127.0.0.1", local_port)).is_ok() {
            return Ok(tunnel);
        }
        if let Some(status) = tunnel.child.try_wait()? {
            if let Some(reader) = tunnel.stderr_reader.take() {
                let _ = reader.join();
            }
            let detail = tunnel.stderr_snapshot();
            return Err(io::Error::other(if detail.is_empty() {
                format!("system SSH exited before the tunnel was ready: {status}")
            } else {
                format!("system SSH failed: {detail}")
            }));
        }
        if Instant::now() >= deadline {
            let _ = tunnel.terminate();
            let detail = tunnel.stderr_snapshot();
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                if detail.is_empty() {
                    "system SSH tunnel startup timed out".to_string()
                } else {
                    format!("system SSH tunnel startup timed out: {detail}")
                },
            ));
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn reserve_local_port() -> io::Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    listener.local_addr().map(|address| address.port())
}

pub fn system_ssh_args(
    config: &SshConnectionConfig,
    postgres_host: &str,
    postgres_port: u16,
    local_port: u16,
) -> io::Result<Vec<OsString>> {
    let mut args = vec![
        OsString::from("-N"),
        OsString::from("-T"),
        OsString::from("-o"),
        OsString::from("ExitOnForwardFailure=yes"),
        OsString::from("-o"),
        OsString::from("BatchMode=yes"),
        OsString::from("-o"),
        OsString::from("NumberOfPasswordPrompts=0"),
        OsString::from("-o"),
        OsString::from("StrictHostKeyChecking=yes"),
        OsString::from("-o"),
        OsString::from("ServerAliveInterval=15"),
        OsString::from("-o"),
        OsString::from("ServerAliveCountMax=3"),
        OsString::from("-o"),
        OsString::from("LogLevel=ERROR"),
        OsString::from("-L"),
        OsString::from(format!(
            "127.0.0.1:{local_port}:{postgres_host}:{postgres_port}"
        )),
    ];

    if let Some(key) = &config.private_key_path {
        args.push(OsString::from("-i"));
        args.push(key.as_os_str().to_os_string());
    }
    if let Some(jump) = &config.jump_host {
        args.push(OsString::from("-J"));
        args.push(OsString::from(system_jump_target(jump)?));
    }
    if config.config_alias.as_deref().is_none_or(str::is_empty) {
        args.push(OsString::from("-p"));
        args.push(OsString::from(config.port.to_string()));
        args.push(OsString::from(format!(
            "{}@{}",
            config.username, config.host
        )));
    } else if let Some(alias) = config.config_alias.as_deref() {
        args.push(OsString::from(alias));
    }
    Ok(args)
}

fn system_jump_target(config: &SshJumpHostConfig) -> io::Result<String> {
    if let Some(alias) = config.config_alias.as_deref().filter(|alias| !alias.is_empty()) {
        return Ok(alias.to_string());
    }
    if config.host.is_empty() || config.username.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "SSH jump host and username are required",
        ));
    }
    Ok(if config.port == 22 {
        format!("{}@{}", config.username, config.host)
    } else {
        format!("{}@{}:{}", config.username, config.host, config.port)
    })
}

fn spawn_bounded_stderr_reader(
    mut pipe: impl Read + Send + 'static,
    output: Arc<Mutex<Vec<u8>>>,
) -> io::Result<thread::JoinHandle<()>> {
    crate::platform::spawn_named("rriter-ssh-stderr", move || {
        const LIMIT: usize = 64 * 1024;
        let mut buffer = [0_u8; 4096];
        loop {
            match pipe.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    let mut output = crate::platform::lock_recover(&output);
                    let remaining = LIMIT.saturating_sub(output.len());
                    output.extend_from_slice(&buffer[..count.min(remaining)]);
                }
            }
        }
    })
}

pub fn resolve_builtin_endpoint(config: &SshConnectionConfig) -> io::Result<ResolvedSshEndpoint> {
    let Some(alias) = config.config_alias.as_deref().filter(|alias| !alias.is_empty()) else {
        return Ok(ResolvedSshEndpoint {
            host: config.host.clone(),
            port: config.port,
            username: config.username.clone(),
            private_key_path: config.private_key_path.clone(),
            proxy_jump: None,
        });
    };

    let mut endpoint = if let Some(ssh) = resolve_system_ssh()
        && let Ok(endpoint) = resolve_alias_with_ssh_g(&ssh, alias)
    {
        endpoint
    } else {
        resolve_alias_from_user_config(alias)?
    };
    if let Some(private_key_path) = &config.private_key_path {
        endpoint.private_key_path = Some(private_key_path.clone());
    }
    Ok(endpoint)
}

fn resolve_alias_with_ssh_g(executable: &Path, alias: &str) -> io::Result<ResolvedSshEndpoint> {
    let mut command = Command::new(executable);
    command.arg("-G").arg("--").arg(alias);
    let output = crate::platform::run_command_output(&mut command, Duration::from_secs(5))?;
    if !output.status.success() {
        return Err(io::Error::other("ssh -G failed to resolve the config alias"));
    }
    parse_ssh_g(&String::from_utf8_lossy(&output.stdout))
}

fn parse_ssh_g(output: &str) -> io::Result<ResolvedSshEndpoint> {
    let mut host = None;
    let mut user = None;
    let mut port = None;
    let mut identity = None;
    let mut proxy_jump = None;
    for line in output.lines() {
        let Some((key, value)) = line.split_once(' ') else {
            continue;
        };
        let value = value.trim();
        match key.to_ascii_lowercase().as_str() {
            "hostname" => host = Some(value.to_string()),
            "user" => user = Some(value.to_string()),
            "port" => port = value.parse::<u16>().ok(),
            "identityfile" if identity.is_none() && value != "none" => {
                identity = Some(expand_home(value))
            }
            "proxyjump" if value != "none" => proxy_jump = Some(value.to_string()),
            _ => {}
        }
    }
    endpoint_from_parts(host, port, user, identity, proxy_jump)
}

fn resolve_alias_from_user_config(alias: &str) -> io::Result<ResolvedSshEndpoint> {
    let home = home_dir().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "cannot resolve SSH config alias because the home directory is unavailable",
        )
    })?;
    let content = fs::read_to_string(home.join(".ssh").join("config")).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            io::Error::new(
                io::ErrorKind::NotFound,
                "system OpenSSH is unavailable and ~/.ssh/config does not exist",
            )
        } else {
            error
        }
    })?;
    parse_user_ssh_config(&content, alias)
}

fn parse_user_ssh_config(content: &str, alias: &str) -> io::Result<ResolvedSshEndpoint> {
    let default_username = std::env::var("USER")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| std::env::var("USERNAME").ok().filter(|value| !value.is_empty()));
    parse_user_ssh_config_with_default_user(content, alias, default_username.as_deref())
}

fn parse_user_ssh_config_with_default_user(
    content: &str,
    alias: &str,
    default_username: Option<&str>,
) -> io::Result<ResolvedSshEndpoint> {
    let mut active = false;
    let mut host = None;
    let mut user = None;
    let mut port = None;
    let mut identity = None;
    let mut proxy_jump = None;
    for raw_line in content.lines() {
        let line = raw_line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        let mut fields = line.split_whitespace();
        let Some(key) = fields.next() else {
            continue;
        };
        if key.eq_ignore_ascii_case("Host") {
            active = ssh_host_patterns_match(fields, alias);
            continue;
        }
        if !active {
            continue;
        }
        let value = fields.collect::<Vec<_>>().join(" ");
        match key.to_ascii_lowercase().as_str() {
            "hostname" if host.is_none() => host = Some(value),
            "user" if user.is_none() => user = Some(value),
            "port" if port.is_none() => port = value.parse::<u16>().ok(),
            "identityfile" if identity.is_none() => identity = Some(expand_home(&value)),
            "proxyjump" if proxy_jump.is_none() => proxy_jump = Some(value),
            _ => {}
        }
    }
    endpoint_from_parts(
        host.or_else(|| Some(alias.to_string())),
        port,
        user.or_else(|| default_username.map(str::to_string)),
        identity,
        proxy_jump,
    )
}

fn ssh_host_patterns_match<'a>(patterns: impl Iterator<Item = &'a str>, alias: &str) -> bool {
    let mut positive_match = false;
    for pattern in patterns {
        let (negated, pattern) = pattern
            .strip_prefix('!')
            .map_or((false, pattern), |pattern| (true, pattern));
        if pattern.is_empty() || !ssh_wildcard_match(pattern, alias) {
            continue;
        }
        if negated {
            return false;
        }
        positive_match = true;
    }
    positive_match
}

fn ssh_wildcard_match(pattern: &str, value: &str) -> bool {
    let pattern = pattern.as_bytes();
    let value = value.as_bytes();
    let mut pattern_index = 0usize;
    let mut value_index = 0usize;
    let mut star_index = None;
    let mut star_value_index = 0usize;
    while value_index < value.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == b'?'
                || pattern[pattern_index].eq_ignore_ascii_case(&value[value_index]))
        {
            pattern_index += 1;
            value_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            star_index = Some(pattern_index);
            pattern_index += 1;
            star_value_index = value_index;
        } else if let Some(star) = star_index {
            pattern_index = star + 1;
            star_value_index += 1;
            value_index = star_value_index;
        } else {
            return false;
        }
    }
    while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
}

fn endpoint_from_parts(
    host: Option<String>,
    port: Option<u16>,
    username: Option<String>,
    private_key_path: Option<PathBuf>,
    proxy_jump: Option<String>,
) -> io::Result<ResolvedSshEndpoint> {
    Ok(ResolvedSshEndpoint {
        host: host.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "SSH alias has no HostName"))?,
        port: port.unwrap_or(22),
        username: username.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "SSH alias has no User"))?,
        private_key_path,
        proxy_jump,
    })
}

fn expand_home(value: &str) -> PathBuf {
    if value == "~" {
        return home_dir().unwrap_or_else(|| PathBuf::from(value));
    }
    if let Some(rest) = value.strip_prefix("~/")
        && let Some(home) = home_dir()
    {
        return home.join(rest);
    }
    PathBuf::from(value)
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeroize::Zeroizing;

    fn ssh_config() -> SshConnectionConfig {
        SshConnectionConfig {
            host: "bastion.example.com".to_string(),
            port: 2222,
            username: "deploy".to_string(),
            ..SshConnectionConfig::default()
        }
    }

    #[test]
    fn password_selects_builtin_without_leaking_the_value() {
        let config = ssh_config();
        let mut secrets = DatabaseSecretBundle::empty();
        secrets.ssh_password = Some(Zeroizing::new("actual-password".to_string()));
        let selected = select_ssh_backend(&config, &secrets, &SshConnectOptions::default());
        assert_eq!(selected.kind, SshBackendKind::Builtin);
        let reason = selected.reason.unwrap();
        assert!(reason.contains("SSH-пароль"));
        assert!(!reason.contains("actual-password"));
    }

    #[test]
    fn jump_host_private_key_selects_builtin_instead_of_ignoring_the_key() {
        let mut config = ssh_config();
        config.jump_host = Some(SshJumpHostConfig {
            host: "jump.example.com".to_string(),
            username: "jump-user".to_string(),
            private_key_path: Some(PathBuf::from("jump-key")),
            ..SshJumpHostConfig::default()
        });
        let selected = select_ssh_backend(
            &config,
            &DatabaseSecretBundle::empty(),
            &SshConnectOptions::default(),
        );
        assert_eq!(selected.kind, SshBackendKind::Builtin);
        assert!(selected.reason.unwrap().contains("jump host"));
    }

    #[test]
    fn system_arguments_preserve_agent_and_config_without_shell_strings() {
        let config = ssh_config();
        let args = system_ssh_args(&config, "db.internal", 5432, 39001).unwrap();
        let args = args
            .into_iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(args.contains(&"BatchMode=yes".to_string()));
        assert!(args.contains(&"StrictHostKeyChecking=yes".to_string()));
        assert!(args.contains(&"127.0.0.1:39001:db.internal:5432".to_string()));
        assert_eq!(args.last().unwrap(), "deploy@bastion.example.com");
    }

    #[test]
    fn ssh_g_and_user_config_resolve_aliases() {
        let endpoint = parse_ssh_g(
            "hostname db-bastion.example.com\nuser deploy\nport 2202\nidentityfile ~/.ssh/id_ed25519\nproxyjump none\n",
        )
        .unwrap();
        assert_eq!(endpoint.host, "db-bastion.example.com");
        assert_eq!(endpoint.port, 2202);
        assert_eq!(endpoint.username, "deploy");

        let endpoint = parse_user_ssh_config(
            "Host prod\n  HostName prod.example.com\n  User release\n  Port 2222\n",
            "prod",
        )
        .unwrap();
        assert_eq!(endpoint.host, "prod.example.com");
        assert_eq!(endpoint.username, "release");
        assert_eq!(endpoint.port, 2222);
    }

    #[test]
    fn a4_b019_user_config_uses_openssh_alias_defaults() {
        let endpoint = parse_user_ssh_config_with_default_user(
            "Host prod\n  Port 2222\n",
            "prod",
            Some("local-user"),
        )
        .unwrap();
        assert_eq!(endpoint.host, "prod");
        assert_eq!(endpoint.username, "local-user");
        assert_eq!(endpoint.port, 2222);
    }

    #[test]
    fn a4_b020_user_config_respects_negated_and_wildcard_host_patterns() {
        let endpoint = parse_user_ssh_config_with_default_user(
            "Host * !prod-*\n  HostName wrong.example.com\n  User wrong\n\nHost prod-*\n  HostName right.example.com\n  User release\n",
            "prod-eu",
            Some("local-user"),
        )
        .unwrap();
        assert_eq!(endpoint.host, "right.example.com");
        assert_eq!(endpoint.username, "release");
        assert!(ssh_host_patterns_match(
            ["PROD-?U"].into_iter(),
            "prod-eu"
        ));
    }

    #[test]
    fn windows_candidates_include_system32_and_sysnative() {
        let candidates = windows_ssh_candidates_with(Path::new(r"C:\Windows"));
        let system32 = candidates[0].to_string_lossy().replace('\\', "/");
        let sysnative = candidates[1].to_string_lossy().replace('\\', "/");
        assert!(system32.ends_with("System32/OpenSSH/ssh.exe"));
        assert!(sysnative.ends_with("Sysnative/OpenSSH/ssh.exe"));
    }
}
