use super::*;
use rayon::prelude::*;
use std::{
    path::{Component, Path},
    sync::Arc,
    time::SystemTime,
};

pub static RASTERIZED_ICONS: once_cell::sync::Lazy<
    std::sync::Mutex<rustc_hash::FxHashMap<&'static str, Option<Vec<u8>>>>,
> = once_cell::sync::Lazy::new(|| std::sync::Mutex::new(rustc_hash::FxHashMap::default()));

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct GitignoreFingerprint {
    exists: bool,
    len: u64,
    modified: Option<SystemTime>,
}

struct GitignoreCacheEntry {
    fingerprint: GitignoreFingerprint,
    gitignore: Arc<ignore::gitignore::Gitignore>,
}

static GITIGNORE_CACHE: once_cell::sync::Lazy<
    std::sync::Mutex<rustc_hash::FxHashMap<PathBuf, GitignoreCacheEntry>>,
> = once_cell::sync::Lazy::new(|| std::sync::Mutex::new(rustc_hash::FxHashMap::default()));

fn gitignore_fingerprint(gitignore_path: &Path) -> GitignoreFingerprint {
    match std::fs::metadata(gitignore_path) {
        Ok(metadata) => GitignoreFingerprint {
            exists: true,
            len: metadata.len(),
            modified: metadata.modified().ok(),
        },
        Err(_) => GitignoreFingerprint {
            exists: false,
            len: 0,
            modified: None,
        },
    }
}

pub(super) fn gitignore_for_root(root: &Path) -> Arc<ignore::gitignore::Gitignore> {
    let gitignore_path = root.join(".gitignore");
    let fingerprint = gitignore_fingerprint(&gitignore_path);

    if let Ok(cache) = GITIGNORE_CACHE.lock() {
        if let Some(entry) = cache.get(root) {
            if entry.fingerprint == fingerprint {
                return Arc::clone(&entry.gitignore);
            }
        }
    }

    let mut builder = ignore::gitignore::GitignoreBuilder::new(root);
    if fingerprint.exists {
        let _ = builder.add(gitignore_path);
    }
    let gitignore = Arc::new(
        builder
            .build()
            .unwrap_or_else(|_| ignore::gitignore::Gitignore::empty()),
    );

    if let Ok(mut cache) = GITIGNORE_CACHE.lock() {
        cache.insert(
            root.to_path_buf(),
            GitignoreCacheEntry {
                fingerprint,
                gitignore: Arc::clone(&gitignore),
            },
        );
    }

    gitignore
}

pub fn pre_rasterize_icon(key: &'static str, is_folder: bool) {
    let cache = RASTERIZED_ICONS.lock().unwrap();
    if let Some(state) = cache.get(key) {
        if state.is_some() {
            return;
        }
    }
    drop(cache); // Не блокируем другие потоки во время рендеринга

    let svg_bytes = crate::app::file_icons::svg_for_key(key, is_folder);
    if svg_bytes.is_empty() {
        RASTERIZED_ICONS.lock().unwrap().insert(key, None);
        return;
    }
    let opt = resvg::usvg::Options::default();
    let svg_str = String::from_utf8_lossy(svg_bytes).replace("currentColor", "#ffffff");

    if let Ok(tree) = resvg::usvg::Tree::from_data(svg_str.as_bytes(), &opt) {
        let target = 64u32;
        if let Some(mut pixmap) = tiny_skia::Pixmap::new(target, target) {
            let sz = tree.size();
            let scale = (target as f32) / sz.width().max(sz.height());
            let dx = (target as f32 - sz.width() * scale) / 2.0;
            let dy = (target as f32 - sz.height() * scale) / 2.0;
            let transform = tiny_skia::Transform::from_row(scale, 0.0, 0.0, scale, dx, dy);
            resvg::render(&tree, transform, &mut pixmap.as_mut());

            let mut data = pixmap.take();
            for px in data.chunks_exact_mut(4) {
                let a = px[3] as u32;
                if a > 0 && a < 255 {
                    px[0] = ((px[0] as u32 * 255) / a).min(255) as u8;
                    px[1] = ((px[1] as u32 * 255) / a).min(255) as u8;
                    px[2] = ((px[2] as u32 * 255) / a).min(255) as u8;
                }
            }
            RASTERIZED_ICONS.lock().unwrap().insert(key, Some(data));
        }
    } else {
        RASTERIZED_ICONS.lock().unwrap().insert(key, None);
    }
}

pub(super) fn read_children(dir: &PathBuf) -> (Vec<(String, PathBuf)>, Vec<(String, PathBuf)>) {
    let mut dirs = Vec::new();
    let mut files = Vec::new();

    // Быстрое чтение директории напрямую через ОС
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            // Игнорируем только самую тяжелую папку .git.
            // Остальные (типа .env, .idea) показываем.
            if name_str == ".git" {
                continue;
            }

            let is_dir = entry
                .file_type()
                .map(|ft| ft.is_dir())
                .unwrap_or_else(|_| path.is_dir());

            if is_dir {
                dirs.push((name_str.into_owned(), path));
            } else {
                files.push((name_str.into_owned(), path));
            }
        }
    }

    // Параллельная многопоточная натуральная сортировка O(N log N)
    rayon::join(
        || dirs.sort_by(|a, b| lexical_sort::natural_lexical_cmp(&a.0, &b.0)),
        || files.sort_by(|a, b| lexical_sort::natural_lexical_cmp(&a.0, &b.0)),
    );

    (dirs, files)
}

// ---------------------------------------------------------------------------
// Фоновый скан
// ---------------------------------------------------------------------------

pub(super) fn scan_dir_parallel(
    path: PathBuf,
    name: String,
    depth: usize,
    expanded: &FxHashSet<PathBuf>,
    is_root: bool,
    max_depth: usize,
    gitignore: &ignore::gitignore::Gitignore,
    all_patterns: &[&str],
) -> Vec<FileNode> {
    let is_expanded = expanded.contains(&path);
    let icon_key = crate::app::file_icons::folder_icon_key(&name.to_ascii_lowercase());

    let is_ignored = if is_root {
        false
    } else {
        gitignore
            .matched_path_or_any_parents(&path, true)
            .is_ignore()
            || matches_ignore_pattern(&name, all_patterns)
    };

    let me = FileNode {
        path: path.clone(),
        name,
        depth,
        is_dir: true,
        is_expanded,
        icon_key,
        is_ignored,
    };

    if !is_expanded || depth >= max_depth {
        return vec![me];
    }

    let (mut dirs, mut files) = read_children(&path);

    // Фильтруем по паттернам ДО параллельного рекурсивного обхода —
    // это экономит поток-часы на игнорируемых поддеревьях.
    dirs.retain(|(d_name, _)| !matches_ignore_pattern(d_name, all_patterns));
    files.retain(|(f_name, _)| !matches_ignore_pattern(f_name, all_patterns));

    // Многопоточный обход дерева. flat_map в rayon собирает результаты
    // асинхронно, но СТРОГО соблюдая оригинальный порядок массивов.
    let mut dir_nodes: Vec<FileNode> = dirs
        .into_par_iter()
        .flat_map(|(d_name, d_path)| {
            scan_dir_parallel(
                d_path,
                d_name,
                depth + 1,
                expanded,
                false,
                max_depth,
                gitignore,
                all_patterns,
            )
        })
        .collect();

    // Параллельное применение Regex паттернов для подбора иконок файлов
    let mut file_nodes: Vec<FileNode> = files
        .into_par_iter()
        .map(|(f_name, f_path)| {
            let f_icon_key = crate::app::file_icons::file_icon_key(&f_name.to_ascii_lowercase());
            let is_ignored = gitignore
                .matched_path_or_any_parents(&f_path, false)
                .is_ignore()
                || matches_ignore_pattern(&f_name, all_patterns);
            FileNode {
                path: f_path,
                name: f_name,
                depth: depth + 1,
                is_dir: false,
                is_expanded: false,
                icon_key: f_icon_key,
                is_ignored,
            }
        })
        .collect();

    let mut result = Vec::with_capacity(1 + dir_nodes.len() + file_nodes.len());
    result.push(me);
    result.append(&mut dir_nodes);
    result.append(&mut file_nodes);
    result
}

#[derive(Debug)]
pub enum FileTreeScanMessage {
    Nodes(Vec<FileNode>),
    IconsReady,
}

/// Запускает фоновый поток сканирования. Возвращает канал для результата.
pub fn spawn_scan(
    roots: Vec<PathBuf>,
    expanded: FxHashSet<PathBuf>,
    user_patterns: Vec<String>,
) -> mpsc::Receiver<FileTreeScanMessage> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        // STEP 1: Полный параллельный скан вглубь (работает < 5ms)
        let all_patterns_refs: Vec<&str> = user_patterns.iter().map(String::as_str).collect();
        let full_nodes: Vec<FileNode> = roots
            .into_par_iter()
            .flat_map(|root| {
                if !root.exists() {
                    return Vec::new();
                }

                let gitignore = gitignore_for_root(&root);

                let name = root
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| root.to_string_lossy().into_owned());

                scan_dir_parallel(
                    root.clone(),
                    name,
                    0,
                    &expanded,
                    true,
                    10,
                    gitignore.as_ref(),
                    &all_patterns_refs,
                )
            })
            .collect();

        let mut needed_icons = rustc_hash::FxHashSet::default();
        for node in &full_nodes {
            needed_icons.insert((node.icon_key, node.is_dir));
        }

        // Отправляем полное дерево немедленно (текст появится мгновенно)
        let _ = tx.send(FileTreeScanMessage::Nodes(full_nodes));

        // STEP 2: Параллельная растеризация иконок без блокировки UI
        needed_icons.into_par_iter().for_each(|(key, is_dir)| {
            crate::app::file_tree::pre_rasterize_icon(key, is_dir);
        });

        // STEP 3: Финальный легкий триггер для перерисовки (иконки появятся)
        let _ = tx.send(FileTreeScanMessage::IconsReady);
    });
    rx
}

pub(super) fn path_has_git_dir(path: &std::path::Path) -> bool {
    path.components().any(|component| {
        matches!(component, Component::Normal(name) if name == std::ffi::OsStr::new(".git"))
    })
}

pub(super) fn notify_paths_need_file_tree_refresh<'a>(
    paths: impl IntoIterator<Item = &'a std::path::Path>,
) -> bool {
    paths.into_iter().any(|path| !path_has_git_dir(path))
}

/// Запускает фоновый поток watcher-а через `notify-debouncer-mini`.
/// Отправляет `()` в `tx` при каждом дебаунсированном событии в watched папках.
/// Дебаунс = 300 мс, поэтому спам событий ОС сворачивается в одно сообщение.
#[cfg_attr(coverage_nightly, coverage(off))]
pub fn spawn_watcher(paths: Vec<PathBuf>, tx: mpsc::Sender<()>) {
    std::thread::spawn(move || {
        use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode};

        let (dtx, drx) = mpsc::channel();
        let mut debouncer = match new_debouncer(std::time::Duration::from_millis(300), dtx) {
            Ok(d) => d,
            Err(_) => return,
        };

        for path in &paths {
            let _ = debouncer.watcher().watch(path, RecursiveMode::Recursive);
        }

        // Блокируемся в цикле — debouncer должен жить, пока работает watcher.
        loop {
            match drx.recv() {
                Ok(Ok(events)) => {
                    let paths = events.iter().map(|event| event.path.as_path());
                    if notify_paths_need_file_tree_refresh(paths) {
                        if tx.send(()).is_err() {
                            break; // главный поток упал / rx закрыт
                        }
                    }
                }
                Ok(Err(_)) => {}
                Err(_) => break,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn temp_scan_root(test_name: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "rriter_{test_name}_{}_{}",
            std::process::id(),
            stamp
        ))
    }

    #[test]
    fn git_internal_notify_paths_do_not_refresh_file_tree() {
        assert!(!notify_paths_need_file_tree_refresh([
            Path::new("/workspace/.git/index"),
            Path::new("/workspace/.git/objects/aa/bb"),
        ]));
        assert!(notify_paths_need_file_tree_refresh([
            Path::new("/workspace/.git/index"),
            Path::new("/workspace/src/main.rs"),
        ]));
        assert!(notify_paths_need_file_tree_refresh([Path::new(
            "/workspace/not.git/index"
        )]));
    }

    #[test]
    fn gitignore_for_root_reuses_unchanged_cache_entry() {
        let root = temp_scan_root("gitignore_cache_reuse");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(".gitignore"), "target/\n").unwrap();

        let first = gitignore_for_root(&root);
        let second = gitignore_for_root(&root);

        assert!(Arc::ptr_eq(&first, &second));
        assert!(
            second
                .matched_path_or_any_parents(root.join("target"), true)
                .is_ignore()
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn gitignore_for_root_rebuilds_after_file_change() {
        let root = temp_scan_root("gitignore_cache_rebuild");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join(".gitignore"), "target/\n").unwrap();

        let first = gitignore_for_root(&root);
        assert!(
            !first
                .matched_path_or_any_parents(root.join("node_modules"), true)
                .is_ignore()
        );

        std::fs::write(root.join(".gitignore"), "target/\nnode_modules/\n").unwrap();
        let second = gitignore_for_root(&root);

        assert!(!Arc::ptr_eq(&first, &second));
        assert!(
            second
                .matched_path_or_any_parents(root.join("node_modules"), true)
                .is_ignore()
        );

        let _ = std::fs::remove_dir_all(root);
    }
}
