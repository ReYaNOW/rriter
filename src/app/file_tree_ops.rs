use super::*;
pub(super) fn validate_child_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Имя не задано".to_string());
    }
    if trimmed == "." || trimmed == ".." {
        return Err("Недопустимое имя".to_string());
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err("Введите только имя, без пути".to_string());
    }
    Ok(())
}

pub(super) fn is_workspace_path(path: &Path, workspaces: &[PathBuf]) -> bool {
    workspaces.iter().any(|root| path.starts_with(root))
}

pub(super) fn is_workspace_root(path: &Path, workspaces: &[PathBuf]) -> bool {
    workspaces.iter().any(|root| path == root)
}

pub(super) fn can_modify_path(path: &Path, workspaces: &[PathBuf]) -> bool {
    is_workspace_path(path, workspaces) && !is_workspace_root(path, workspaces)
}

pub fn relative_path_for_workspace(path: &Path, workspaces: &[PathBuf]) -> PathBuf {
    for root in workspaces {
        if let Ok(rel) = path.strip_prefix(root) {
            return rel.to_path_buf();
        }
    }
    path.to_path_buf()
}

pub(super) fn unique_child_path(target_dir: &Path, name: &str) -> PathBuf {
    let first = target_dir.join(name);
    if !first.exists() {
        return first;
    }

    let path = Path::new(name);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(name);
    let ext = path.extension().and_then(|e| e.to_str());
    for idx in 1..10_000 {
        let candidate_name = match ext {
            Some(ext) if idx == 1 => format!("{stem} copy.{ext}"),
            Some(ext) => format!("{stem} copy {idx}.{ext}"),
            None if idx == 1 => format!("{stem} copy"),
            None => format!("{stem} copy {idx}"),
        };
        let candidate = target_dir.join(candidate_name);
        if !candidate.exists() {
            return candidate;
        }
    }
    target_dir.join(format!("{name} copy"))
}

pub(super) fn copy_path_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(src)?;
    if meta.is_dir() {
        std::fs::create_dir(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let child_dst = dst.join(entry.file_name());
            copy_path_recursive(&entry.path(), &child_dst)?;
        }
    } else {
        std::fs::copy(src, dst).map(|_| ())?;
    }
    Ok(())
}

pub(super) fn delete_path(path: &Path) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(path)?;
    if meta.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

pub(super) fn move_path_exact(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.exists() {
        return Err(format!("Не найдено: {}", src.display()));
    }
    if dst.exists() && src != dst {
        return Err(format!("Уже существует: {}", dst.display()));
    }
    if let Some(parent) = dst.parent() {
        if !parent.is_dir() {
            return Err(format!("Не найдена папка: {}", parent.display()));
        }
    }
    if src == dst {
        return Ok(());
    }
    match std::fs::rename(src, dst) {
        Ok(_) => Ok(()),
        Err(_) => {
            copy_path_recursive(src, dst).map_err(|err| err.to_string())?;
            delete_path(src).map_err(|err| err.to_string())
        }
    }
}

pub(super) fn prune_nested_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut sorted = paths.to_vec();
    sorted.sort_by(|a, b| {
        a.components()
            .count()
            .cmp(&b.components().count())
            .then_with(|| a.cmp(b))
    });
    let mut pruned: Vec<PathBuf> = Vec::new();
    for path in sorted {
        if !pruned
            .iter()
            .any(|parent| path != *parent && path.starts_with(parent))
        {
            pruned.push(path);
        }
    }
    pruned
}

pub(super) fn trash_dirs() -> Result<(PathBuf, PathBuf), String> {
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(|home| PathBuf::from(home).join(".local/share"))
        })
        .ok_or_else(|| "Не удалось найти XDG trash".to_string())?;
    let trash_dir = data_home.join("Trash");
    Ok((trash_dir.join("files"), trash_dir.join("info")))
}

pub(super) fn trash_info_path_value(path: &Path) -> String {
    let text = path.to_string_lossy();
    let mut out = String::with_capacity(text.len());
    for &byte in text.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'-' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

pub(super) fn unix_days_to_ymd(days_since_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };
    (year as i32, month as u32, day as u32)
}

pub(super) fn trash_deletion_date() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (year, month, day) = unix_days_to_ymd(days);
    let hour = rem / 3_600;
    let min = (rem % 3_600) / 60;
    let sec = rem % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}")
}

pub(super) fn trash_single_path(
    path: &Path,
    files_dir: &Path,
    info_dir: &Path,
) -> Result<FileTreeTrashEntry, String> {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return Err("Не удалось прочитать имя".to_string());
    };
    let trash_path = unique_child_path(files_dir, name);
    let trash_name = trash_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Не удалось создать имя в корзине".to_string())?;
    let info_path = info_dir.join(format!("{trash_name}.trashinfo"));
    move_path_exact(path, &trash_path)?;
    let info = format!(
        "[Trash Info]\nPath={}\nDeletionDate={}\n",
        trash_info_path_value(path),
        trash_deletion_date()
    );
    if let Err(err) = std::fs::write(&info_path, info) {
        let _ = move_path_exact(&trash_path, path);
        return Err(err.to_string());
    }
    Ok(FileTreeTrashEntry {
        original_path: path.to_path_buf(),
        trash_path,
        info_path,
    })
}

pub(super) fn trash_paths(
    paths: &[PathBuf],
    workspaces: &[PathBuf],
) -> Result<Vec<FileTreeTrashEntry>, String> {
    let paths = prune_nested_paths(paths);
    for path in &paths {
        if !can_modify_path(path, workspaces) {
            return Err("Можно удалять только элементы внутри workspace".to_string());
        }
    }
    let (files_dir, info_dir) = trash_dirs()?;
    std::fs::create_dir_all(&files_dir).map_err(|err| err.to_string())?;
    std::fs::create_dir_all(&info_dir).map_err(|err| err.to_string())?;
    let mut trashed = Vec::new();
    for path in paths {
        if !path.exists() {
            continue;
        }
        match trash_single_path(&path, &files_dir, &info_dir) {
            Ok(entry) => trashed.push(entry),
            Err(err) => {
                let _ = restore_trash_entries(&trashed);
                return Err(err);
            }
        }
    }
    Ok(trashed)
}

pub(super) fn restore_trash_entries(
    entries: &[FileTreeTrashEntry],
) -> Result<Vec<PathBuf>, String> {
    let mut restored = Vec::new();
    for entry in entries.iter().rev() {
        if !entry.trash_path.exists() {
            return Err(format!(
                "Не найдено в корзине: {}",
                entry.trash_path.display()
            ));
        }
        let Some(parent) = entry.original_path.parent() else {
            return Err("Не удалось найти исходную папку".to_string());
        };
        if !parent.is_dir() {
            return Err(format!("Не найдена папка: {}", parent.display()));
        }
        let restore_path = if entry.original_path.exists() {
            let Some(name) = entry
                .original_path
                .file_name()
                .and_then(|name| name.to_str())
            else {
                return Err("Не удалось прочитать имя".to_string());
            };
            unique_child_path(parent, name)
        } else {
            entry.original_path.clone()
        };
        move_path_exact(&entry.trash_path, &restore_path)?;
        let _ = std::fs::remove_file(&entry.info_path);
        restored.push(restore_path);
    }
    restored.reverse();
    Ok(restored)
}

pub(super) fn move_path_to_dir(
    src: &Path,
    target_dir: &Path,
) -> Result<(PathBuf, PathBuf), String> {
    if !src.exists() {
        return Err(format!("Не найдено: {}", src.display()));
    }
    if src.is_dir() && target_dir.starts_with(src) {
        return Err("Нельзя переместить папку внутрь самой себя".to_string());
    }
    if src.parent() == Some(target_dir) {
        return Ok((src.to_path_buf(), src.to_path_buf()));
    }

    let Some(name) = src.file_name().and_then(|name| name.to_str()) else {
        return Err("Не удалось прочитать имя".to_string());
    };
    let dst = unique_child_path(target_dir, name);
    match std::fs::rename(src, &dst) {
        Ok(_) => Ok((src.to_path_buf(), dst)),
        Err(_) => {
            copy_path_recursive(src, &dst).map_err(|err| err.to_string())?;
            delete_path(src).map_err(|err| err.to_string())?;
            Ok((src.to_path_buf(), dst))
        }
    }
}

pub(super) fn rename_path(
    path: &Path,
    new_name: &str,
    workspaces: &[PathBuf],
) -> Result<PathBuf, String> {
    validate_child_name(new_name)?;
    if !can_modify_path(path, workspaces) {
        return Err("Можно переименовать только элементы внутри workspace".to_string());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "Не удалось найти родительскую директорию".to_string())?;
    let dst = parent.join(new_name);
    if dst == path {
        return Ok(dst);
    }
    if dst.exists() {
        return Err("Уже существует".to_string());
    }
    std::fs::rename(path, &dst).map_err(|err| err.to_string())?;
    Ok(dst)
}

pub(super) fn path_after_rename(path: &Path, old_root: &Path, new_root: &Path) -> Option<PathBuf> {
    if path == old_root {
        Some(new_root.to_path_buf())
    } else {
        path.strip_prefix(old_root)
            .ok()
            .map(|rel| new_root.join(rel))
    }
}

pub(super) fn copy_paths_to_dir(
    paths: &[PathBuf],
    target_dir: &Path,
) -> Result<Vec<PathBuf>, String> {
    let mut copied = Vec::new();
    for src in prune_nested_paths(paths) {
        if !src.exists() {
            return Err(format!("Не найдено: {}", src.display()));
        }
        if src.is_dir() && target_dir.starts_with(&src) {
            return Err("Нельзя копировать папку внутрь самой себя".to_string());
        }
        let Some(name) = src.file_name().and_then(|name| name.to_str()) else {
            return Err("Не удалось прочитать имя".to_string());
        };
        let dst = unique_child_path(target_dir, name);
        copy_path_recursive(&src, &dst).map_err(|err| err.to_string())?;
        copied.push(dst);
    }
    Ok(copied)
}

#[cfg(test)]
pub(super) fn delete_paths(paths: &[PathBuf], workspaces: &[PathBuf]) -> Result<(), String> {
    for path in paths {
        if !can_modify_path(path, workspaces) {
            return Err("Можно удалять только элементы внутри workspace".to_string());
        }
    }
    for path in paths {
        if path.exists() {
            delete_path(path).map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

pub(super) fn selected_paths(
    nodes: &[FileNode],
    selection: &FxHashSet<PathBuf>,
    fallback: &Path,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for node in nodes {
        if selection.contains(&node.path) {
            paths.push(node.path.clone());
        }
    }
    if paths.is_empty() {
        paths.push(fallback.to_path_buf());
    }
    paths
}

// ---------------------------------------------------------------------------
// Методы App
// ---------------------------------------------------------------------------
