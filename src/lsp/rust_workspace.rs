use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::platform::{ToolKind, ToolResolution};

pub const RUST_ANALYZER_RELEASE_TAG: &str = "2026-09-28";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RustAnalyzerArchive {
    pub triple: &'static str,
    pub asset: &'static str,
    pub sha256: &'static str,
}

pub const RUST_ANALYZER_ARCHIVES: &[RustAnalyzerArchive] = &[
    RustAnalyzerArchive {
        triple: "x86_64-unknown-linux-gnu",
        asset: "rust-analyzer-x86_64-unknown-linux-gnu.gz",
        sha256: "23f711d86b5f826e22886f01d7355dc01e0f4c1357dafa29710a95b903b48c85",
    },
    RustAnalyzerArchive {
        triple: "aarch64-unknown-linux-gnu",
        asset: "rust-analyzer-aarch64-unknown-linux-gnu.gz",
        sha256: "03bad9c3dabb0f07a2678d5f9f8f1575a3742ea141506e14b3a26b42a1f896f3",
    },
    RustAnalyzerArchive {
        triple: "x86_64-apple-darwin",
        asset: "rust-analyzer-x86_64-apple-darwin.gz",
        sha256: "d032c0eb75e4597cc8ffc35ea4cdbd9eecc8341936b6edac6749e679fc3f0682",
    },
    RustAnalyzerArchive {
        triple: "aarch64-apple-darwin",
        asset: "rust-analyzer-aarch64-apple-darwin.gz",
        sha256: "54ec873d8996e2c127d758bf45d4eacb6d3371dae4f6f6d5d3f05cedbae5fd59",
    },
    RustAnalyzerArchive {
        triple: "x86_64-pc-windows-msvc",
        asset: "rust-analyzer-x86_64-pc-windows-msvc.zip",
        sha256: "ad78fb368525404c6ac09c4bba33e90797902ce1f5a17db0925c695cae096ccc",
    },
    RustAnalyzerArchive {
        triple: "aarch64-pc-windows-msvc",
        asset: "rust-analyzer-aarch64-pc-windows-msvc.zip",
        sha256: "f63c7fc9a00a7e863b21b5e0b77cda7ff61aaa9f6f83b0affa5bbb525be7c43c",
    },
];

pub fn rust_analyzer_archive_for_platform() -> Option<&'static RustAnalyzerArchive> {
    let triple = match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
        ("aarch64", "linux") => "aarch64-unknown-linux-gnu",
        ("x86_64", "macos") => "x86_64-apple-darwin",
        ("aarch64", "macos") => "aarch64-apple-darwin",
        ("x86_64", "windows") => "x86_64-pc-windows-msvc",
        ("aarch64", "windows") => "aarch64-pc-windows-msvc",
        _ => return None,
    };
    RUST_ANALYZER_ARCHIVES.iter().find(|archive| archive.triple == triple)
}

pub fn cargo_root_for_path(
    path: &Path,
    workspaces: &[PathBuf],
    locate: &dyn Fn(&Path) -> Option<PathBuf>,
) -> Option<PathBuf> {
    let crate_dir = nearest_cargo_toml_dir(path, workspaces)?;
    locate(&crate_dir)
        .and_then(|cargo_toml| cargo_toml.parent().map(Path::to_path_buf))
        .or(Some(crate_dir))
}

pub fn nearest_cargo_toml_dir(path: &Path, workspaces: &[PathBuf]) -> Option<PathBuf> {
    let file_dir = path.parent().unwrap_or(path);
    let search_stop = workspaces
        .iter()
        .filter(|workspace| crate::platform::path_is_within(path, workspace))
        .max_by_key(|workspace| workspace.components().count());
    super::dart_workspace::nearest_marker(file_dir, search_stop.map(PathBuf::as_path), "Cargo.toml")
}

pub fn initialization_options(check_command: &str) -> serde_json::Value {
    serde_json::json!({
        "checkOnSave": true,
        "check": { "command": check_command },
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RustRootResolution {
    pub root: PathBuf,
    pub executable: Result<PathBuf, RustToolError>,
    pub version: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RustToolError {
    NotFound,
    ComponentMissing,
    Timeout,
}

pub(super) struct RustRootJob {
    pub(super) generation: u64,
    pub(super) rx: crate::ui_waker::OneShot<RustRootResolution>,
}

pub(super) enum RustRootEntry {
    Pending(RustRootJob),
    Ready(RustRootResolution),
}

pub(super) struct RustTools {
    pub cargo: Option<PathBuf>,
    pub rustup: Option<PathBuf>,
    pub resolution: ToolResolution,
}

pub(super) type RunCmd =
    dyn Fn(&Path, &[&str], &Path, Duration) -> Result<String, RustToolError> + Send + Sync;

pub(super) fn run_cmd(
    program: &Path,
    args: &[&str],
    cwd: &Path,
    timeout: Duration,
) -> Result<String, RustToolError> {
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .env("RUSTUP_AUTO_INSTALL", "0");
    let output = crate::platform::run_command_output(&mut command, timeout).map_err(|error| {
        if error.kind() == std::io::ErrorKind::TimedOut {
            RustToolError::Timeout
        } else {
            RustToolError::NotFound
        }
    })?;
    if !output.status.success() {
        return Err(RustToolError::ComponentMissing);
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub(super) fn resolve_rust_root_blocking(
    crate_dir: &Path,
    tools: &RustTools,
) -> RustRootResolution {
    resolve_rust_root_with(crate_dir, tools, &run_cmd)
}

pub(super) fn resolve_rust_root_with(
    crate_dir: &Path,
    tools: &RustTools,
    run: &RunCmd,
) -> RustRootResolution {
    let root = tools
        .cargo
        .as_deref()
        .and_then(|cargo| {
            run(
                cargo,
                &["locate-project", "--workspace", "--message-format", "plain"],
                crate_dir,
                Duration::from_secs(2),
            )
            .ok()
        })
        .and_then(|stdout| {
            let cargo_toml = stdout.trim();
            let cargo_toml = Path::new(cargo_toml);
            if cargo_toml.file_name() != Some(std::ffi::OsStr::new("Cargo.toml")) {
                return None;
            }
            cargo_toml
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .map(Path::to_path_buf)
        })
        .unwrap_or_else(|| crate_dir.to_path_buf());

    let executable = resolved_executable(&root, tools, run);
    let version = executable.as_ref().ok().and_then(|executable| {
        run(
            executable,
            &["--version"],
            &root,
            Duration::from_secs(2),
        )
        .ok()
        .and_then(|stdout| stdout.lines().map(str::trim).find(|line| !line.is_empty()).map(str::to_owned))
    });

    RustRootResolution {
        root,
        executable,
        version,
    }
}

fn resolved_executable(root: &Path, tools: &RustTools, run: &RunCmd) -> Result<PathBuf, RustToolError> {
    let resolution_path = tools.resolution.path.as_deref();
    let resolution_is_proxy = resolution_path.is_some_and(|path| {
        tools.resolution.source_label(ToolKind::RustAnalyzer) == Some("PATH")
            && tools
                .rustup
                .as_deref()
                .and_then(Path::parent)
                .is_some_and(|rustup_dir| crate::platform::path_is_within(path, rustup_dir))
    });
    if let Some(path) = resolution_path.filter(|_| !resolution_is_proxy) {
        return Ok(path.to_path_buf());
    }

    let Some(rustup) = tools.rustup.as_deref() else {
        return resolution_path
            .map(Path::to_path_buf)
            .ok_or(RustToolError::NotFound);
    };
    match run(
        rustup,
        &["which", "rust-analyzer"],
        root,
        Duration::from_secs(3),
    ) {
        Ok(stdout) => {
            let executable = stdout.trim();
            if executable.is_empty() {
                Err(RustToolError::ComponentMissing)
            } else {
                Ok(PathBuf::from(executable))
            }
        }
        Err(RustToolError::Timeout) => Err(RustToolError::Timeout),
        Err(_) => Err(RustToolError::ComponentMissing),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "rriter-rust-workspace-{name}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn cargo_file(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"fixture\"\n").unwrap();
    }

    fn resolution(path: Option<PathBuf>) -> ToolResolution {
        ToolResolution {
            path,
            configured_path: None,
            source: None,
            sdk_root: None,
        }
    }

    fn no_run(_: &Path, _: &[&str], _: &Path, _: Duration) -> Result<String, RustToolError> {
        Err(RustToolError::ComponentMissing)
    }

    #[test]
    fn cargo_root_uses_cargo_workspace_output() {
        let dir = temp_dir("workspace-output");
        let member = dir.join("member");
        let source = member.join("src/lib.rs");
        cargo_file(&member);
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        let root = dir.join("Cargo.toml");
        let result = cargo_root_for_path(&source, std::slice::from_ref(&dir), &|_| Some(root.clone()));
        assert_eq!(result, Some(dir.clone()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn cargo_root_falls_back_to_nearest_crate_without_cargo_result() {
        let dir = temp_dir("member-fallback");
        let member = dir.join("member");
        let source = member.join("src/lib.rs");
        cargo_file(&member);
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[workspace]\nmembers = [\"member\"]\n").unwrap();
        assert_eq!(cargo_root_for_path(&source, std::slice::from_ref(&dir), &|_| None), Some(member.clone()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn cargo_root_finds_single_crate() {
        let dir = temp_dir("single-crate");
        let source = dir.join("src/main.rs");
        cargo_file(&dir);
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        assert_eq!(cargo_root_for_path(&source, &[], &|_| None), Some(dir.clone()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn cargo_root_requires_manifest_and_respects_workspace_boundary() {
        let dir = temp_dir("boundary");
        let workspace = dir.join("editor");
        let outside_crate = dir.join("outside");
        let source = workspace.join("nested/src/main.rs");
        cargo_file(&dir);
        cargo_file(&outside_crate);
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        assert_eq!(nearest_cargo_toml_dir(&source, std::slice::from_ref(&workspace)), None);
        assert_eq!(nearest_cargo_toml_dir(&outside_crate.join("src/main.rs"), &[workspace]), Some(outside_crate.clone()));
        let no_manifest = temp_dir("no-manifest");
        assert_eq!(nearest_cargo_toml_dir(&no_manifest.join("plain.rs"), &[]), None);
        let _ = std::fs::remove_dir_all(no_manifest);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn initialization_options_configures_check_on_save() {
        let options = initialization_options("clippy");
        assert_eq!(options["check"]["command"], "clippy");
        assert_eq!(options["checkOnSave"], true);
    }

    #[test]
    fn resolver_prefers_override_without_running_rustup() {
        let override_path = PathBuf::from("/configured/rust-analyzer");
        let tools = RustTools {
            cargo: None,
            rustup: Some(PathBuf::from("/rustup/bin/rustup")),
            resolution: resolution(Some(override_path.clone())),
        };
        let calls = Arc::new(Mutex::new(0));
        let run_calls = Arc::clone(&calls);
        let run = move |_: &Path, args: &[&str], _: &Path, _: Duration| {
            if args.first() == Some(&"which") {
                *run_calls.lock().unwrap() += 1;
            }
            Err(RustToolError::ComponentMissing)
        };
        let result = resolve_rust_root_with(Path::new("/crate"), &tools, &run);
        assert_eq!(result.executable, Ok(override_path));
        assert_eq!(*calls.lock().unwrap(), 0);
    }

    #[test]
    fn resolver_uses_rustup_for_path_proxy_and_reports_which_errors() {
        let rustup = PathBuf::from("/cargo/bin/rustup");
        let tools = RustTools {
            cargo: None,
            rustup: Some(rustup),
            resolution: resolution(None),
        };
        let run = |_: &Path, args: &[&str], _: &Path, _: Duration| {
            if args.first() == Some(&"which") {
                Ok("/toolchain/bin/rust-analyzer\n".to_owned())
            } else {
                Err(RustToolError::ComponentMissing)
            }
        };
        let result = resolve_rust_root_with(Path::new("/crate"), &tools, &run);
        assert_eq!(result.executable, Ok(PathBuf::from("/toolchain/bin/rust-analyzer")));

        let missing = |_: &Path, _: &[&str], _: &Path, _: Duration| {
            Err(RustToolError::ComponentMissing)
        };
        assert_eq!(resolve_rust_root_with(Path::new("/crate"), &tools, &missing).executable, Err(RustToolError::ComponentMissing));
        let timeout = |_: &Path, args: &[&str], _: &Path, _: Duration| {
            Err(RustToolError::Timeout)
        };
        assert_eq!(resolve_rust_root_with(Path::new("/crate"), &tools, &timeout).executable, Err(RustToolError::Timeout));
    }

    #[test]
    fn resolver_handles_missing_tools_bad_cargo_output_and_version_errors() {
        let no_tools = RustTools {
            cargo: None,
            rustup: None,
            resolution: resolution(None),
        };
        assert_eq!(resolve_rust_root_with(Path::new("/crate"), &no_tools, &no_run).executable, Err(RustToolError::NotFound));

        let cargo = PathBuf::from("cargo");
        let executable = PathBuf::from("/configured/ra");
        let tools = RustTools {
            cargo: Some(cargo),
            rustup: None,
            resolution: resolution(Some(executable.clone())),
        };
        let expected_executable = executable.clone();
        let run = move |program: &Path, args: &[&str], _: &Path, _: Duration| {
            if args.first() == Some(&"locate-project") {
                Ok("not a Cargo.toml path\n".to_owned())
            } else if program == expected_executable {
                Err(RustToolError::Timeout)
            } else {
                Ok(String::new())
            }
        };
        let result = resolve_rust_root_with(Path::new("/crate"), &tools, &run);
        assert_eq!(result.root, PathBuf::from("/crate"));
        assert_eq!(result.version, None);

        let empty = |_: &Path, args: &[&str], _: &Path, _: Duration| {
            if args.first() == Some(&"locate-project") { Ok("  \n".to_owned()) } else { Ok(String::new()) }
        };
        assert_eq!(resolve_rust_root_with(Path::new("/crate"), &tools, &empty).root, PathBuf::from("/crate"));
        let locate_error = |_: &Path, args: &[&str], _: &Path, _: Duration| {
            if args.first() == Some(&"locate-project") { Err(RustToolError::Timeout) } else { Ok(String::new()) }
        };
        assert_eq!(resolve_rust_root_with(Path::new("/crate"), &tools, &locate_error).root, PathBuf::from("/crate"));
    }

    #[cfg(unix)]
    #[test]
    fn run_cmd_maps_timeout_missing_program_and_nonzero_exit() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir("run-command");
        let script = dir.join("sleep.sh");
        std::fs::write(&script, "#!/bin/sh\nsleep 10\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let started = Instant::now();
        assert_eq!(run_cmd(&script, &["which", "rust-analyzer"], &dir, Duration::from_secs(3)), Err(RustToolError::Timeout));
        assert!(started.elapsed() <= Duration::from_millis(3500));

        assert_eq!(run_cmd(&dir.join("missing"), &[], &dir, Duration::from_secs(1)), Err(RustToolError::NotFound));
        let failing = dir.join("fail.sh");
        std::fs::write(&failing, "#!/bin/sh\nexit 1\n").unwrap();
        std::fs::set_permissions(&failing, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(run_cmd(&failing, &[], &dir, Duration::from_secs(1)), Err(RustToolError::ComponentMissing));
        let _ = std::fs::remove_dir_all(dir);
    }
}
