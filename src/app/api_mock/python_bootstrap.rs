use super::types::{ApiUvState, ApiUvStatus};
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
                state.last_error = err;
            }
        },
        None => {
            state.status = ApiUvStatus::Missing;
            state.last_error = "uv not found".to_string();
        }
    }
}
