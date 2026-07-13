const GIT_QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);
const GIT_NETWORK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);
const GIT_OVERRIDE_ENV: &str = "RRITER_GIT_PATH";
const GIT_SSL_BACKEND_ENV: &str = "RRITER_GIT_SSL_BACKEND";

fn git_command(
    repo_root: &std::path::Path,
    args: &[std::ffi::OsString],
    network: bool,
) -> Result<std::process::Command, String> {
    git_command_for_platform(
        repo_root,
        args,
        network,
        crate::platform::CURRENT_PLATFORM,
    )
}

fn git_command_for_platform(
    repo_root: &std::path::Path,
    args: &[std::ffi::OsString],
    network: bool,
    platform: crate::platform::PlatformKind,
) -> Result<std::process::Command, String> {
    let mut command = crate::platform::command_for_tool(
        std::ffi::OsStr::new("git"),
        GIT_OVERRIDE_ENV,
    )
    .map_err(git_spawn_error)?;
    command.arg("-C").arg(repo_root);

    // Git for Windows supports the Schannel backend. It uses the Windows
    // certificate store, including corporate/user-installed roots. Users can
    // explicitly select another backend when their Git distribution differs.
    if network && platform == crate::platform::PlatformKind::Windows {
        let backend = std::env::var_os(GIT_SSL_BACKEND_ENV)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| std::ffi::OsString::from("schannel"));
        command.arg("-c").arg({
            let mut setting = std::ffi::OsString::from("http.sslBackend=");
            setting.push(backend);
            setting
        });
    }

    command.args(args).env("GIT_TERMINAL_PROMPT", "0");
    if std::env::var_os("GCM_INTERACTIVE").is_none() {
        command.env("GCM_INTERACTIVE", "auto");
    }
    // Do not override GIT_SSH/GIT_SSH_COMMAND or core.sshCommand: Git for
    // Windows may be configured for OpenSSH, PuTTY/plink, or a vendor agent.
    // stdin is closed by the managed runner and the bounded timeout still
    // prevents an invisible terminal prompt from hanging RRiter forever.
    Ok(command)
}

fn git_output(
    repo_root: &std::path::Path,
    args: &[std::ffi::OsString],
    label: &str,
    network: bool,
) -> Result<std::process::Output, String> {
    println!(
        "[GIT {label}] repo={} operation={}",
        repo_root.display(),
        args.first()
            .map(|arg| arg.to_string_lossy())
            .unwrap_or_default()
    );
    let mut command = git_command(repo_root, args, network)?;
    let timeout = if network {
        GIT_NETWORK_TIMEOUT
    } else {
        GIT_QUERY_TIMEOUT
    };
    let output = crate::platform::run_command_output(&mut command, timeout)
        .map_err(|error| git_process_error(label, error))?;
    if output.status.success() {
        println!("[GIT {label}] ok");
        Ok(output)
    } else {
        let error = git_failed_output(label, &output);
        println!(
            "[GIT {label}] failed status={:?} error={}",
            output.status.code(),
            short_command_output(error.as_bytes())
        );
        Err(error)
    }
}

fn git_output_strs(
    repo_root: &std::path::Path,
    args: &[&str],
    label: &str,
    network: bool,
) -> Result<std::process::Output, String> {
    let args = args
        .iter()
        .map(std::ffi::OsString::from)
        .collect::<Vec<_>>();
    git_output(repo_root, &args, label, network)
}

fn run_git_checked(
    repo_root: &std::path::Path,
    args: &[&str],
    label: &str,
) -> Result<(), String> {
    git_output_strs(repo_root, args, label, true).map(|_| ())
}

fn run_git_checked_owned(
    repo_root: &std::path::Path,
    args: Vec<std::ffi::OsString>,
    label: &str,
) -> Result<(), String> {
    git_output(repo_root, &args, label, true).map(|_| ())
}

fn git_spawn_error(error: std::io::Error) -> String {
    if error.kind() == std::io::ErrorKind::NotFound {
        format!(
            "Git executable not found. Install Git or set {GIT_OVERRIDE_ENV}. ({error})"
        )
    } else {
        format!("Git could not be started: {error}")
    }
}

fn git_process_error(label: &str, error: std::io::Error) -> String {
    if error.kind() == std::io::ErrorKind::TimedOut {
        format!("Git {label} timed out and its process tree was terminated")
    } else {
        git_spawn_error(error)
    }
}

fn git_failed_output(label: &str, output: &std::process::Output) -> String {
    let stderr = short_command_output(&output.stderr);
    let stdout = short_command_output(&output.stdout);
    let detail = if stderr.is_empty() { stdout } else { stderr };
    classify_git_failure(label, &detail)
}

fn classify_git_failure(label: &str, detail: &str) -> String {
    let lowered = detail.to_ascii_lowercase();
    let category = if lowered.contains("authentication failed")
        || lowered.contains("could not read username")
        || lowered.contains("permission denied (publickey)")
        || lowered.contains("repository not found")
        || lowered.contains("terminal prompts disabled")
    {
        Some("authentication failed; check Git Credential Manager, ssh-agent, or the remote access token")
    } else if lowered.contains("certificate")
        || lowered.contains("ssl certificate problem")
        || lowered.contains("schannel") && lowered.contains("error")
    {
        Some("TLS certificate validation failed; check the Windows certificate store or Git SSL backend")
    } else if lowered.contains("proxy")
        && (lowered.contains("failed")
            || lowered.contains("unable")
            || lowered.contains("407"))
    {
        Some("proxy connection or proxy authentication failed; check Git proxy settings")
    } else if lowered.contains("host key verification failed") {
        Some("SSH host key verification failed; update known_hosts for this remote")
    } else if lowered.contains("could not resolve host") {
        Some("remote host could not be resolved; check DNS and proxy settings")
    } else {
        None
    };

    match (category, detail.is_empty()) {
        (Some(category), false) => format!("Git {label}: {category}. {detail}"),
        (Some(category), true) => format!("Git {label}: {category}"),
        (None, false) => detail.to_string(),
        (None, true) => format!("Git {label} failed without diagnostic output"),
    }
}

fn short_command_output(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes)
        .replace("\r\n", "\n")
        .trim()
        .to_string();
    if text.len() > 360 {
        let end = text
            .char_indices()
            .take_while(|(idx, _)| *idx <= 360)
            .map(|(idx, ch)| idx + ch.len_utf8())
            .last()
            .unwrap_or(0)
            .min(text.len());
        format!("{}...", &text[..end])
    } else {
        text
    }
}

#[cfg(test)]
mod git_process_tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn environment_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn git_failures_are_classified_without_hiding_original_detail() {
        let auth = classify_git_failure(
            "PUSH",
            "fatal: Authentication failed for 'https://example.test/repo'",
        );
        assert!(auth.contains("Credential Manager"));
        assert!(auth.contains("Authentication failed"));

        let certificate = classify_git_failure(
            "FETCH",
            "fatal: unable to access: SSL certificate problem: unable to get local issuer",
        );
        assert!(certificate.contains("certificate store"));
        assert!(certificate.contains("local issuer"));

        let proxy = classify_git_failure(
            "PULL",
            "fatal: unable to access: CONNECT tunnel failed, response 407 Proxy Authentication Required",
        );
        assert!(proxy.contains("proxy authentication"));
        assert!(proxy.contains("407"));
    }

    #[test]
    fn command_uses_override_and_keeps_git_credential_integration() {
        let _guard = environment_lock().lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rriter-git-command-{}-{}",
            std::process::id(),
            crate::platform::next_operation_id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let executable = root.join(if cfg!(windows) { "git.exe" } else { "git" });
        std::fs::write(&executable, b"").unwrap();
        let old = std::env::var_os(GIT_OVERRIDE_ENV);
        unsafe { std::env::set_var(GIT_OVERRIDE_ENV, &executable) };
        let command = git_command(
            std::path::Path::new("repo with spaces"),
            &[std::ffi::OsString::from("fetch")],
            true,
        )
        .unwrap();
        if let Some(old) = old {
            unsafe { std::env::set_var(GIT_OVERRIDE_ENV, old) };
        } else {
            unsafe { std::env::remove_var(GIT_OVERRIDE_ENV) };
        }

        assert_eq!(command.get_program(), executable.as_os_str());
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(args.windows(2).any(|pair| pair == ["-C", "repo with spaces"]));
        assert!(args.iter().any(|arg| arg == "fetch"));
        assert_eq!(
            command
                .get_envs()
                .find(|(key, _)| *key == "GIT_TERMINAL_PROMPT")
                .and_then(|(_, value)| value),
            Some(std::ffi::OsStr::new("0"))
        );
        assert!(
            command
                .get_envs()
                .all(|(key, _)| key != std::ffi::OsStr::new("GIT_SSH_COMMAND"))
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn windows_network_command_uses_schannel_without_replacing_ssh_agent() {
        let _guard = environment_lock().lock().unwrap();
        let root = std::env::temp_dir().join(format!(
            "rriter-git-windows-command-{}-{}",
            std::process::id(),
            crate::platform::next_operation_id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let executable = root.join("git.exe");
        std::fs::write(&executable, b"").unwrap();
        let old_path = std::env::var_os(GIT_OVERRIDE_ENV);
        let old_backend = std::env::var_os(GIT_SSL_BACKEND_ENV);
        unsafe {
            std::env::set_var(GIT_OVERRIDE_ENV, &executable);
            std::env::remove_var(GIT_SSL_BACKEND_ENV);
        }
        let command = git_command_for_platform(
            std::path::Path::new(r"C:\repo with spaces"),
            &[std::ffi::OsString::from("fetch")],
            true,
            crate::platform::PlatformKind::Windows,
        )
        .unwrap();
        match old_path {
            Some(value) => unsafe { std::env::set_var(GIT_OVERRIDE_ENV, value) },
            None => unsafe { std::env::remove_var(GIT_OVERRIDE_ENV) },
        }
        match old_backend {
            Some(value) => unsafe { std::env::set_var(GIT_SSL_BACKEND_ENV, value) },
            None => unsafe { std::env::remove_var(GIT_SSL_BACKEND_ENV) },
        }

        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(args.windows(2).any(|pair| {
            pair == ["-c", "http.sslBackend=schannel"]
        }));
        assert!(
            command
                .get_envs()
                .all(|(key, _)| key != std::ffi::OsStr::new("GIT_SSH_COMMAND"))
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn short_output_normalizes_windows_line_endings_and_is_bounded() {
        assert_eq!(short_command_output(b"first\r\nsecond\r\n"), "first\nsecond");
        let long = "x".repeat(500);
        assert!(short_command_output(long.as_bytes()).len() < long.len());
    }
}
