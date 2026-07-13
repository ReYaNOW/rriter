use super::types::{ApiPythonRuntimeMode, ApiUvState, ApiUvStatus};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const TOOL_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

pub fn detect_uv_path() -> Option<PathBuf> {
    crate::platform::resolve_tool_executable(OsStr::new("uv"), "RRITER_UV_PATH")
}

#[allow(dead_code)]
pub fn detect_python_path() -> Option<PathBuf> {
    let candidates: &[&str] = if cfg!(windows) {
        &["py.exe", "python.exe", "python3.exe"]
    } else {
        &["python3", "python"]
    };
    candidates
        .iter()
        .find_map(|candidate| crate::platform::resolve_executable(OsStr::new(candidate)))
}

pub(crate) fn python_command(path: &Path) -> Command {
    let mut command = Command::new(path);
    if is_windows_py_launcher(path) {
        command.arg("-3");
    }
    command
}

fn is_windows_py_launcher(path: &Path) -> bool {
    let raw = path.as_os_str().to_string_lossy();
    raw.rsplit(['\\', '/']).next().is_some_and(|name| {
        name.eq_ignore_ascii_case("py.exe") || name.eq_ignore_ascii_case("py")
    })
}

pub fn validate_uv_path(path: &Path) -> Result<String, String> {
    let mut command = Command::new(path);
    command.arg("--version");
    validate_tool_command(&mut command)
}

pub fn validate_python_path(path: &Path) -> Result<String, String> {
    let mut command = python_command(path);
    command.arg("--version");
    validate_tool_command(&mut command)
}

fn validate_tool_command(command: &mut Command) -> Result<String, String> {
    let output = crate::platform::run_command_output(command, TOOL_PROBE_TIMEOUT)
        .map_err(|err| err.to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !output.status.success() {
        return Err(if stderr.is_empty() { stdout } else { stderr });
    }
    if !stdout.is_empty() {
        Ok(stdout)
    } else {
        Ok(stderr)
    }
}

pub fn refresh_uv_status(state: &mut ApiUvState) {
    let candidate = state.configured_path.clone().or_else(detect_uv_path);
    match candidate {
        Some(path) => match validate_uv_path(&path) {
            Ok(_) => {
                state.detected_path = Some(path);
                state.status = ApiUvStatus::Ready;
                state.last_error.clear();
            }
            Err(err) => {
                state.status = ApiUvStatus::Invalid;
                state.last_error = format!("Ошибка проверки uv: {err}");
            }
        },
        None => {
            state.status = ApiUvStatus::Missing;
            state.last_error =
                "uv не найден. Укажите путь к uv или задайте RRITER_UV_PATH.".to_string();
        }
    }
}

pub fn refresh_python_runtime_status(state: &mut ApiUvState) {
    match state.mode {
        ApiPythonRuntimeMode::UvManaged => refresh_uv_status(state),
        ApiPythonRuntimeMode::CustomPython => match state.custom_python_path.clone() {
            Some(path) => match validate_python_path(&path) {
                Ok(version) => {
                    state.status = ApiUvStatus::Ready;
                    state.last_error = format!("Версия: {version}");
                }
                Err(err) => {
                    state.status = ApiUvStatus::Invalid;
                    state.last_error = format!("Ошибка проверки Python: {err}");
                }
            },
            None => {
                state.status = ApiUvStatus::Missing;
                state.last_error = "Путь к Python не задан.".to_string();
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_python_launcher_receives_major_version_selector() {
        let command = python_command(Path::new(r"C:\Windows\py.exe"));
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(args, vec!["-3"]);
    }

    #[test]
    fn regular_python_executable_has_no_launcher_prefix() {
        let command = python_command(Path::new(r"C:\Python313\python.exe"));
        assert!(command.get_args().next().is_none());
    }
}
