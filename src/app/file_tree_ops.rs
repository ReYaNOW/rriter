use super::*;
use std::ffi::{OsStr, OsString};

pub(super) fn validate_child_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Имя не задано".to_string());
    }
    if trimmed == "." || trimmed == ".." {
        return Err("Недопустимое имя".to_string());
    }
    crate::platform::validate_child_name(trimmed).map_err(|reason| match reason {
        "name contains a path separator" => "Введите только имя, без пути".to_string(),
        "name contains a character forbidden by Windows" => {
            "Имя содержит символ, запрещённый в Windows".to_string()
        }
        "Windows names cannot end with a dot or space" => {
            "В Windows имя не может заканчиваться точкой или пробелом".to_string()
        }
        "name is reserved by Windows" => "Это имя зарезервировано Windows".to_string(),
        _ => "Недопустимое имя".to_string(),
    })
}

pub(super) fn is_workspace_path(path: &Path, workspaces: &[PathBuf]) -> bool {
    workspaces
        .iter()
        .any(|root| crate::platform::path_is_within(path, root))
}

pub(super) fn is_workspace_root(path: &Path, workspaces: &[PathBuf]) -> bool {
    workspaces
        .iter()
        .any(|root| crate::platform::paths_equal(path, root))
}

pub(super) fn can_modify_path(path: &Path, workspaces: &[PathBuf]) -> bool {
    is_workspace_path(path, workspaces) && !is_workspace_root(path, workspaces)
}

pub fn relative_path_for_workspace(path: &Path, workspaces: &[PathBuf]) -> PathBuf {
    workspaces
        .iter()
        .find_map(|root| crate::platform::relative_to(path, root))
        .unwrap_or_else(|| path.to_path_buf())
}

fn copy_candidate_name(name: &OsStr, index: usize) -> OsString {
    let name_path = Path::new(name);
    let stem = name_path
        .file_stem()
        .filter(|stem| !stem.is_empty())
        .unwrap_or(name);
    let mut candidate = OsString::from(stem);
    if index == 1 {
        candidate.push(" copy");
    } else {
        candidate.push(format!(" copy {index}"));
    }
    if let Some(extension) = name_path.extension() {
        candidate.push(".");
        candidate.push(extension);
    }
    candidate
}

pub(super) fn unique_child_path(target_dir: &Path, name: impl AsRef<OsStr>) -> PathBuf {
    let name = name.as_ref();
    let first = target_dir.join(name);
    if !crate::platform::path_entry_exists(&first) {
        return first;
    }

    for index in 1..10_000 {
        let candidate = target_dir.join(copy_candidate_name(name, index));
        if !crate::platform::path_entry_exists(&candidate) {
            return candidate;
        }
    }
    target_dir.join(copy_candidate_name(name, 10_000))
}

fn copy_cleanup_error(dst: &Path, error: std::io::Error) -> std::io::Error {
    match crate::platform::remove_path_entry(dst) {
        Ok(()) => error,
        Err(cleanup) if cleanup.kind() == std::io::ErrorKind::NotFound => error,
        Err(cleanup) => std::io::Error::new(
            error.kind(),
            format!("{error}; не удалось удалить неполную копию {}: {cleanup}", dst.display()),
        ),
    }
}

pub(super) fn copy_path_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    let metadata = std::fs::symlink_metadata(src)?;
    if crate::platform::metadata_is_link(&metadata) {
        return crate::platform::copy_symlink(src, dst);
    }
    if metadata.is_dir() {
        std::fs::create_dir(dst)?;
        let copy_result = (|| {
            for entry in std::fs::read_dir(src)? {
                let entry = entry?;
                copy_path_recursive(&entry.path(), &dst.join(entry.file_name()))?;
            }
            std::fs::set_permissions(dst, metadata.permissions())?;
            Ok(())
        })();
        copy_result.map_err(|error| copy_cleanup_error(dst, error))
    } else {
        let copy_result = (|| {
            std::fs::copy(src, dst)?;
            std::fs::set_permissions(dst, metadata.permissions())?;
            Ok(())
        })();
        copy_result.map_err(|error| copy_cleanup_error(dst, error))
    }
}

pub(super) fn delete_path(path: &Path) -> std::io::Result<()> {
    crate::platform::remove_path_entry(path)
}

fn is_real_directory(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| {
        metadata.is_dir() && !crate::platform::metadata_is_link(&metadata)
    })
}

pub(super) fn cross_volume_move(src: &Path, dst: &Path) -> Result<(), String> {
    let parent = src
        .parent()
        .ok_or_else(|| "Не удалось найти исходную папку".to_string())?;
    let staging = (0..64)
        .map(|_| {
            let suffix = crate::platform::next_operation_id();
            parent.join(format!(".rriter-move-{suffix}"))
        })
        .find(|candidate| !crate::platform::path_entry_exists(candidate))
        .ok_or_else(|| "Не удалось подготовить безопасное перемещение".to_string())?;

    std::fs::rename(src, &staging).map_err(|error| error.to_string())?;
    if let Err(copy_error) = copy_path_recursive(&staging, dst) {
        return match std::fs::rename(&staging, src) {
            Ok(()) => Err(copy_error.to_string()),
            Err(rollback_error) => Err(format!(
                "Не удалось скопировать путь: {copy_error}; исходный путь остался во временном имени {} и откат не удался: {rollback_error}",
                staging.display()
            )),
        };
    }

    if let Err(delete_error) = delete_path(&staging) {
        let destination_cleanup = delete_path(dst);
        let source_restore = std::fs::rename(&staging, src);
        return match (destination_cleanup, source_restore) {
            (Ok(()), Ok(())) => Err(format!(
                "Копирование завершено, но исходный путь не удалось удалить: {delete_error}; операция полностью откачена"
            )),
            (cleanup, restore) => {
                let cleanup = cleanup
                    .err()
                    .map(|error| format!("; копию {} удалить не удалось: {error}", dst.display()))
                    .unwrap_or_default();
                let restore = restore
                    .err()
                    .map(|error| format!("; имя исходника {} восстановить не удалось: {error}", src.display()))
                    .unwrap_or_default();
                Err(format!(
                    "Копирование завершено, но временный исходный путь {} не удалось удалить: {delete_error}{cleanup}{restore}",
                    staging.display()
                ))
            }
        };
    }
    Ok(())
}

pub(super) fn move_path_exact(src: &Path, dst: &Path) -> Result<(), String> {
    if !crate::platform::path_entry_exists(src) {
        return Err(format!("Не найдено: {}", src.display()));
    }
    if crate::platform::path_entry_exists(dst) && !crate::platform::paths_equal(src, dst) {
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
    match crate::platform::rename_path(src, dst) {
        Ok(()) => Ok(()),
        Err(error) if crate::platform::is_cross_device_error(&error) => cross_volume_move(src, dst),
        Err(error) => Err(error.to_string()),
    }
}

pub(super) fn prune_nested_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut sorted = crate::platform::dedup_paths(paths.iter().cloned());
    sorted.sort_by(|left, right| {
        left.components()
            .count()
            .cmp(&right.components().count())
            .then_with(|| left.cmp(right))
    });
    let mut pruned: Vec<PathBuf> = Vec::new();
    for path in sorted {
        if !pruned
            .iter()
            .any(|parent| crate::platform::path_is_within(&path, parent))
        {
            pruned.push(path);
        }
    }
    pruned
}

pub(super) fn trash_info_path_value(path: &Path) -> String {
    #[cfg(unix)]
    let bytes = {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes().to_vec()
    };
    #[cfg(not(unix))]
    let bytes = path.to_string_lossy().as_bytes().to_vec();

    let mut out = String::with_capacity(bytes.len());
    for byte in bytes {
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

fn trash_info_file_name(trash_name: &OsStr) -> OsString {
    let mut name = OsString::from(trash_name);
    name.push(".trashinfo");
    name
}

pub(super) fn trash_single_path_with_layout(
    path: &Path,
    files_dir: &Path,
    info_dir: &Path,
    freedesktop: bool,
) -> Result<FileTreeTrashEntry, String> {
    let name = path
        .file_name()
        .ok_or_else(|| "Не удалось прочитать имя".to_string())?;
    let trash_path = unique_child_path(files_dir, name);
    let trash_name = trash_path
        .file_name()
        .ok_or_else(|| "Не удалось создать имя в корзине".to_string())?;
    let info_path = info_dir.join(trash_info_file_name(trash_name));
    move_path_exact(path, &trash_path)?;
    let stored_path = if freedesktop {
        trash_info_path_value(path)
    } else {
        crate::platform::encode_persisted_path(path)
    };
    let section = if freedesktop {
        "Trash Info"
    } else {
        "RRiter Trash"
    };
    let info = format!(
        "[{section}]\nPath={stored_path}\nDeletionDate={}\n",
        trash_deletion_date()
    );
    if let Err(error) = crate::platform::atomic_write(&info_path, info.as_bytes()) {
        return match move_path_exact(&trash_path, path) {
            Ok(()) => Err(error.to_string()),
            Err(rollback) => Err(format!(
                "Не удалось записать метаданные корзины: {error}; откат {} -> {} также не удался: {rollback}",
                trash_path.display(),
                path.display()
            )),
        };
    }
    Ok(FileTreeTrashEntry {
        original_path: path.to_path_buf(),
        trash_path,
        info_path,
    })
}

#[cfg(test)]
pub(super) fn trash_single_path(
    path: &Path,
    files_dir: &Path,
    info_dir: &Path,
) -> Result<FileTreeTrashEntry, String> {
    trash_single_path_with_layout(path, files_dir, info_dir, cfg!(target_os = "linux"))
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
    let layout = crate::platform::trash_layout();
    std::fs::create_dir_all(&layout.files_dir).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&layout.info_dir).map_err(|error| error.to_string())?;
    let mut trashed = Vec::new();
    for path in paths {
        if !crate::platform::path_entry_exists(&path) {
            continue;
        }
        match trash_single_path_with_layout(
            &path,
            &layout.files_dir,
            &layout.info_dir,
            layout.freedesktop,
        ) {
            Ok(entry) => trashed.push(entry),
            Err(error) => {
                let _ = restore_trash_entries(&trashed);
                return Err(error);
            }
        }
    }
    Ok(trashed)
}

pub(super) fn restore_trash_entries(
    entries: &[FileTreeTrashEntry],
) -> Result<Vec<PathBuf>, String> {
    let mut restored: Vec<(&FileTreeTrashEntry, PathBuf)> = Vec::new();
    for entry in entries.iter().rev() {
        let result = (|| {
            if !crate::platform::path_entry_exists(&entry.trash_path) {
                return Err(format!(
                    "Не найдено в корзине: {}",
                    entry.trash_path.display()
                ));
            }
            let parent = entry
                .original_path
                .parent()
                .ok_or_else(|| "Не удалось найти исходную папку".to_string())?;
            if !parent.is_dir() {
                return Err(format!("Не найдена папка: {}", parent.display()));
            }
            let restore_path = if crate::platform::path_entry_exists(&entry.original_path) {
                let name = entry
                    .original_path
                    .file_name()
                    .ok_or_else(|| "Не удалось прочитать имя".to_string())?;
                unique_child_path(parent, name)
            } else {
                entry.original_path.clone()
            };
            move_path_exact(&entry.trash_path, &restore_path)?;
            Ok(restore_path)
        })();
        match result {
            Ok(path) => restored.push((entry, path)),
            Err(error) => {
                let mut rollback_errors = Vec::new();
                for (restored_entry, restored_path) in restored.iter().rev() {
                    if let Err(rollback) = move_path_exact(restored_path, &restored_entry.trash_path)
                    {
                        rollback_errors.push(rollback);
                    }
                }
                return if rollback_errors.is_empty() {
                    Err(error)
                } else {
                    Err(format!(
                        "{error}; откат восстановления не удался: {}",
                        rollback_errors.join("; ")
                    ))
                };
            }
        }
    }
    let mut metadata_errors = Vec::new();
    for (entry, _) in &restored {
        if let Err(error) = std::fs::remove_file(&entry.info_path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            metadata_errors.push(format!("{}: {error}", entry.info_path.display()));
        }
    }
    restored.reverse();
    if metadata_errors.is_empty() {
        Ok(restored.into_iter().map(|(_, path)| path).collect())
    } else {
        Err(format!(
            "Файлы восстановлены, но не удалось удалить метаданные корзины: {}",
            metadata_errors.join("; ")
        ))
    }
}

pub(super) fn move_path_to_dir(
    src: &Path,
    target_dir: &Path,
) -> Result<(PathBuf, PathBuf), String> {
    if !crate::platform::path_entry_exists(src) {
        return Err(format!("Не найдено: {}", src.display()));
    }
    if is_real_directory(src) && crate::platform::path_is_within(target_dir, src) {
        return Err("Нельзя переместить папку внутрь самой себя".to_string());
    }
    if src
        .parent()
        .is_some_and(|parent| crate::platform::paths_equal(parent, target_dir))
    {
        return Ok((src.to_path_buf(), src.to_path_buf()));
    }

    let name = src
        .file_name()
        .ok_or_else(|| "Не удалось прочитать имя".to_string())?;
    let dst = unique_child_path(target_dir, name);
    move_path_exact(src, &dst)?;
    Ok((src.to_path_buf(), dst))
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
    if path == dst {
        return Ok(dst);
    }
    if crate::platform::path_entry_exists(&dst) && !crate::platform::paths_equal(path, &dst) {
        return Err("Уже существует".to_string());
    }
    crate::platform::rename_path(path, &dst).map_err(|error| error.to_string())?;
    Ok(dst)
}

pub(super) fn path_after_rename(path: &Path, old_root: &Path, new_root: &Path) -> Option<PathBuf> {
    if crate::platform::paths_equal(path, old_root) {
        Some(new_root.to_path_buf())
    } else {
        crate::platform::relative_to(path, old_root).map(|relative| new_root.join(relative))
    }
}

/// Remaps stored paths after a file-tree rename while preserving the original
/// spelling of paths that are outside the renamed subtree.
pub(super) fn remap_paths_after_rename(
    paths: &mut [PathBuf],
    old_root: &Path,
    new_root: &Path,
) -> bool {
    let mut changed = false;
    for path in paths {
        if let Some(updated) = path_after_rename(path, old_root, new_root) {
            *path = updated;
            changed = true;
        }
    }
    changed
}

pub(super) fn remap_path_set_after_rename(
    paths: &mut FxHashSet<PathBuf>,
    old_root: &Path,
    new_root: &Path,
) {
    *paths = paths
        .drain()
        .map(|path| path_after_rename(&path, old_root, new_root).unwrap_or(path))
        .collect();
}

pub(super) fn remap_optional_path_after_rename(
    path: &mut Option<PathBuf>,
    old_root: &Path,
    new_root: &Path,
) {
    if let Some(updated) = path
        .as_deref()
        .and_then(|path| path_after_rename(path, old_root, new_root))
    {
        *path = Some(updated);
    }
}

pub(super) fn copy_paths_to_dir(
    paths: &[PathBuf],
    target_dir: &Path,
) -> Result<Vec<PathBuf>, String> {
    let mut copied: Vec<PathBuf> = Vec::new();
    for src in prune_nested_paths(paths) {
        if !crate::platform::path_entry_exists(&src) {
            return Err(format!("Не найдено: {}", src.display()));
        }
        if is_real_directory(&src) && crate::platform::path_is_within(target_dir, &src) {
            return Err("Нельзя копировать папку внутрь самой себя".to_string());
        }
        let name = src
            .file_name()
            .ok_or_else(|| "Не удалось прочитать имя".to_string())?;
        let dst = unique_child_path(target_dir, name);
        if let Err(error) = copy_path_recursive(&src, &dst) {
            let _ = delete_path(&dst);
            for path in copied.iter().rev() {
                let _ = delete_path(path);
            }
            return Err(error.to_string());
        }
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
        if crate::platform::path_entry_exists(path) {
            delete_path(path).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

pub(super) fn path_set_remove(paths: &mut FxHashSet<PathBuf>, candidate: &Path) -> bool {
    let existing = paths
        .iter()
        .find(|path| crate::platform::paths_equal(path, candidate))
        .cloned();
    existing.is_some_and(|path| paths.remove(&path))
}

pub(super) fn path_lists_equal(left: &[PathBuf], right: &[PathBuf]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| crate::platform::paths_equal(left, right))
}

pub(super) fn selection_contains_path(
    selection: &FxHashSet<PathBuf>,
    candidate: &Path,
) -> bool {
    selection
        .iter()
        .any(|selected| crate::platform::paths_equal(selected, candidate))
}

pub(super) fn selected_paths(
    nodes: &[FileNode],
    selection: &FxHashSet<PathBuf>,
    fallback: &Path,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for node in nodes {
        if selection_contains_path(selection, &node.path) {
            paths.push(node.path.clone());
        }
    }
    if paths.is_empty() {
        paths.push(fallback.to_path_buf());
    }
    crate::platform::dedup_paths(paths)
}

// ---------------------------------------------------------------------------
// Методы App
// ---------------------------------------------------------------------------
