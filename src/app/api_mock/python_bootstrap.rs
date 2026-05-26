use super::types::{ApiPythonRuntimeMode, ApiUvState, ApiUvStatus};
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn detect_uv_path() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join("uv"))
            .find(|path| path.is_file())
    })
}

pub fn validate_uv_path(path: &Path) -> Result<String, String> {
    let output = Command::new(path)
        .arg("--version")
        .output()
        .map_err(|err| err.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn validate_python_path(path: &Path) -> Result<String, String> {
    let output = Command::new(path)
        .arg("--version")
        .output()
        .map_err(|err| err.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
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
            state.last_error = "uv не найден. Укажите путь к uv.".to_string();
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
