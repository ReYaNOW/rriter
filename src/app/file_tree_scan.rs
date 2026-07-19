use super::*;
use rayon::prelude::*;
use rustc_hash::FxHashSet;
use std::{
    path::{Component, Path, PathBuf},
    sync::{Arc, mpsc},
    time::{Duration, SystemTime},
};

pub static RASTERIZED_ICONS: once_cell::sync::Lazy<
    std::sync::Mutex<rustc_hash::FxHashMap<&'static str, RasterizedIconState>>,
> = once_cell::sync::Lazy::new(|| std::sync::Mutex::new(rustc_hash::FxHashMap::default()));
const RASTERIZED_ICON_CACHE_LIMIT: usize = 256;
const RASTERIZED_ICON_READY_BYTE_LIMIT: usize = 1024 * 1024;
const GITIGNORE_CACHE_LIMIT: usize = 32;
const FILE_TREE_PARALLEL_ENTRY_THRESHOLD: usize = 512;

pub enum RasterizedIconState {
    Pending,
    Missing,
    Ready(Box<[u8]>),
}

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
    std::sync::Mutex<rustc_hash::FxHashMap<crate::platform::PathKey, GitignoreCacheEntry>>,
> = once_cell::sync::Lazy::new(|| std::sync::Mutex::new(rustc_hash::FxHashMap::default()));

fn trim_rasterized_icon_cache(
    cache: &mut rustc_hash::FxHashMap<&'static str, RasterizedIconState>,
    keep_key: &'static str,
) {
    while cache.len() > RASTERIZED_ICON_CACHE_LIMIT {
        let victim = cache.iter().find_map(|(&key, value)| {
            (key != keep_key
                && matches!(
                    value,
                    RasterizedIconState::Pending | RasterizedIconState::Missing
                ))
            .then_some(key)
        });
        let victim = victim.or_else(|| {
            cache.iter().find_map(|(&key, value)| {
                (key != keep_key && matches!(value, RasterizedIconState::Ready(_))).then_some(key)
            })
        });
        let Some(victim) = victim else {
            break;
        };
        cache.remove(&victim);
    }

    let mut ready_bytes = cache
        .values()
        .map(|value| match value {
            RasterizedIconState::Ready(data) => data.len(),
            RasterizedIconState::Pending | RasterizedIconState::Missing => 0,
        })
        .sum::<usize>();
    while ready_bytes > RASTERIZED_ICON_READY_BYTE_LIMIT {
        let victim = cache.iter().find_map(|(&key, value)| match value {
            RasterizedIconState::Ready(data) if key != keep_key => Some((key, data.len())),
            RasterizedIconState::Ready(_)
            | RasterizedIconState::Pending
            | RasterizedIconState::Missing => None,
        });
        let Some((victim, bytes)) = victim else {
            break;
        };
        cache.remove(&victim);
        ready_bytes = ready_bytes.saturating_sub(bytes);
    }
}

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

fn trim_gitignore_cache(
    cache: &mut rustc_hash::FxHashMap<crate::platform::PathKey, GitignoreCacheEntry>,
    keep_root: &crate::platform::PathKey,
) {
    while cache.len() >= GITIGNORE_CACHE_LIMIT {
        let victim = cache
            .keys()
            .find(|path| *path != keep_root)
            .cloned();
        let Some(victim) = victim else {
            break;
        };
        cache.remove(&victim);
    }
}

pub(super) fn gitignore_for_root(root: &Path) -> Arc<ignore::gitignore::Gitignore> {
    let root_key = crate::platform::PathKey::new(root);
    let gitignore_path = root.join(".gitignore");
    let fingerprint = gitignore_fingerprint(&gitignore_path);

    {
        let cache = crate::platform::lock_recover(&GITIGNORE_CACHE);
        if let Some(entry) = cache.get(&root_key)
            && entry.fingerprint == fingerprint
        {
            return Arc::clone(&entry.gitignore);
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

    let mut cache = crate::platform::lock_recover(&GITIGNORE_CACHE);
    trim_gitignore_cache(&mut cache, &root_key);
    cache.insert(
        root_key,
        GitignoreCacheEntry {
            fingerprint,
            gitignore: Arc::clone(&gitignore),
        },
    );

    gitignore
}

pub fn pre_rasterize_icon(key: &'static str, is_folder: bool) {
    if !reserve_rasterized_icon(key) {
        return;
    }
    finish_reserved_rasterized_icon(key, is_folder);
}

pub fn request_rasterized_icon(key: &'static str, is_folder: bool) {
    if reserve_rasterized_icon(key)
        && let Err(err) = crate::platform::spawn_named("rriter-file-icon", move || {
            finish_reserved_rasterized_icon(key, is_folder);
        })
    {
        eprintln!("RRiter: не удалось запустить rasterize icon worker: {err}");
        finish_reserved_rasterized_icon(key, is_folder);
    }
}

fn reserve_rasterized_icon(key: &'static str) -> bool {
    let Ok(mut cache) = RASTERIZED_ICONS.lock() else {
        return false;
    };
    if cache.contains_key(key) {
        return false;
    }
    cache.insert(key, RasterizedIconState::Pending);
    trim_rasterized_icon_cache(&mut cache, key);
    true
}

fn store_rasterized_icon_state(key: &'static str, state: RasterizedIconState) {
    let mut cache = crate::platform::lock_recover(&RASTERIZED_ICONS);
    cache.insert(key, state);
    trim_rasterized_icon_cache(&mut cache, key);
}

fn finish_reserved_rasterized_icon(key: &'static str, is_folder: bool) {
    let svg_bytes = crate::app::file_icons::svg_for_key(key, is_folder);
    if svg_bytes.is_empty() {
        store_rasterized_icon_state(key, RasterizedIconState::Missing);
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
            store_rasterized_icon_state(key, RasterizedIconState::Ready(data.into_boxed_slice()));
        } else {
            store_rasterized_icon_state(key, RasterizedIconState::Missing);
        }
    } else {
        store_rasterized_icon_state(key, RasterizedIconState::Missing);
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

            // Игнорируем только самую тяжелую папку .git.
            // Остальные (типа .env, .idea) показываем.
            if os_name_is_git_dir(&name, crate::platform::CURRENT_PLATFORM) {
                continue;
            }

            let name_str = name.to_string_lossy();

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

    if dirs.len().saturating_add(files.len()) >= FILE_TREE_PARALLEL_ENTRY_THRESHOLD {
        rayon::join(
            || dirs.sort_by(|a, b| lexical_sort::natural_lexical_cmp(&a.0, &b.0)),
            || files.sort_by(|a, b| lexical_sort::natural_lexical_cmp(&a.0, &b.0)),
        );
    } else {
        dirs.sort_by(|a, b| lexical_sort::natural_lexical_cmp(&a.0, &b.0));
        files.sort_by(|a, b| lexical_sort::natural_lexical_cmp(&a.0, &b.0));
    }

    (dirs, files)
}

fn os_name_is_git_dir(
    name: &std::ffi::OsStr,
    platform: crate::platform::PlatformKind,
) -> bool {
    if platform == crate::platform::PlatformKind::Windows {
        name.as_encoded_bytes().eq_ignore_ascii_case(b".git")
    } else {
        name == std::ffi::OsStr::new(".git")
    }
}

// ---------------------------------------------------------------------------
// Фоновый скан
// ---------------------------------------------------------------------------

#[cfg(test)]
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
    let mut out = Vec::new();
    push_scan_dir_nodes(
        &mut out,
        path,
        name,
        depth,
        expanded,
        is_root,
        max_depth,
        gitignore,
        all_patterns,
    );
    out
}

struct PendingScanDir {
    path: PathBuf,
    name: String,
    depth: usize,
    is_root: bool,
}

enum ScanTask {
    Dir(PendingScanDir),
    Files {
        files: Vec<(String, PathBuf)>,
        depth: usize,
    },
}

#[allow(clippy::too_many_arguments)]
fn push_scan_dir_nodes(
    out: &mut Vec<FileNode>,
    path: PathBuf,
    name: String,
    depth: usize,
    expanded: &FxHashSet<PathBuf>,
    is_root: bool,
    max_depth: usize,
    gitignore: &ignore::gitignore::Gitignore,
    all_patterns: &[&str],
) {
    let mut stack = vec![ScanTask::Dir(PendingScanDir {
        path,
        name,
        depth,
        is_root,
    })];

    while let Some(task) = stack.pop() {
        match task {
            ScanTask::Dir(PendingScanDir {
                path,
                name,
                depth,
                is_root,
            }) => {
                let is_expanded = expanded
                    .iter()
                    .any(|expanded_path| crate::platform::paths_equal(expanded_path, &path));
                let icon_key = crate::app::file_icons::folder_icon_key_for_name(&name);
                let is_ignored = if is_root {
                    false
                } else {
                    gitignore
                        .matched_path_or_any_parents(&path, true)
                        .is_ignore()
                        || matches_ignore_pattern(&name, all_patterns)
                };

                out.push(FileNode {
                    path: path.clone(),
                    name,
                    depth,
                    is_dir: true,
                    is_expanded,
                    icon_key,
                    is_ignored,
                });

                if !is_expanded || depth >= max_depth {
                    continue;
                }

                let (mut dirs, mut files) = read_children(&path);
                dirs.retain(|(d_name, _)| !matches_ignore_pattern(d_name, all_patterns));
                files.retain(|(f_name, _)| !matches_ignore_pattern(f_name, all_patterns));

                if !files.is_empty() {
                    stack.push(ScanTask::Files {
                        files,
                        depth: depth + 1,
                    });
                }
                for (d_name, d_path) in dirs.into_iter().rev() {
                    stack.push(ScanTask::Dir(PendingScanDir {
                        path: d_path,
                        name: d_name,
                        depth: depth + 1,
                        is_root: false,
                    }));
                }
            }
            ScanTask::Files { files, depth } => {
                push_file_nodes(out, files, depth, gitignore, all_patterns);
            }
        }
    }
}

fn file_tree_file_node(
    f_name: String,
    f_path: PathBuf,
    depth: usize,
    gitignore: &ignore::gitignore::Gitignore,
    all_patterns: &[&str],
) -> FileNode {
    let f_icon_key = crate::app::file_icons::file_icon_key_for_name(&f_name);
    let is_ignored = gitignore
        .matched_path_or_any_parents(&f_path, false)
        .is_ignore()
        || matches_ignore_pattern(&f_name, all_patterns);
    FileNode {
        path: f_path,
        name: f_name,
        depth,
        is_dir: false,
        is_expanded: false,
        icon_key: f_icon_key,
        is_ignored,
    }
}

fn push_file_nodes(
    out: &mut Vec<FileNode>,
    files: Vec<(String, PathBuf)>,
    depth: usize,
    gitignore: &ignore::gitignore::Gitignore,
    all_patterns: &[&str],
) {
    if files.len() >= FILE_TREE_PARALLEL_ENTRY_THRESHOLD {
        let mut file_nodes: Vec<FileNode> = files
            .into_par_iter()
            .map(|(f_name, f_path)| {
                file_tree_file_node(f_name, f_path, depth, gitignore, all_patterns)
            })
            .collect();
        out.append(&mut file_nodes);
    } else {
        out.reserve(files.len());
        for (f_name, f_path) in files {
            out.push(file_tree_file_node(
                f_name,
                f_path,
                depth,
                gitignore,
                all_patterns,
            ));
        }
    }
}

#[derive(Debug)]
pub enum FileTreeScanMessage {
    Nodes(Vec<FileNode>),
    IconsReady,
    Failed(String),
}

impl FileTreeScanMessage {
    pub(super) fn is_terminal(&self) -> bool {
        matches!(self, Self::IconsReady | Self::Failed(_))
    }
}

/// Запускает фоновый поток сканирования. Возвращает канал для результата.
pub fn spawn_scan(
    roots: Vec<PathBuf>,
    expanded: FxHashSet<PathBuf>,
    user_patterns: Vec<String>,
) -> mpsc::Receiver<FileTreeScanMessage> {
    let (tx, rx) = mpsc::channel();
    let worker_tx = tx.clone();
    if let Err(err) = crate::platform::spawn_named("rriter-file-tree-scan", move || {
        // STEP 1: Build ordered tree in one reusable buffer.
        let all_patterns_refs: Vec<&str> = user_patterns.iter().map(String::as_str).collect();
        let mut full_nodes = Vec::new();
        for root in roots {
            if !root.exists() {
                continue;
            }

            let gitignore = gitignore_for_root(&root);

            let name = root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| root.to_string_lossy().into_owned());

            push_scan_dir_nodes(
                &mut full_nodes,
                root.clone(),
                name,
                0,
                &expanded,
                true,
                10,
                gitignore.as_ref(),
                &all_patterns_refs,
            );
        }

        let mut needed_icons = rustc_hash::FxHashSet::default();
        for node in &full_nodes {
            needed_icons.insert((node.icon_key, node.is_dir));
        }

        // Отправляем полное дерево немедленно (текст появится мгновенно)
        let _ = worker_tx.send(FileTreeScanMessage::Nodes(full_nodes));

        // STEP 2: Параллельная растеризация иконок без блокировки UI
        if needed_icons.len() >= FILE_TREE_PARALLEL_ENTRY_THRESHOLD {
            needed_icons.into_par_iter().for_each(|(key, is_dir)| {
                crate::app::file_tree::pre_rasterize_icon(key, is_dir);
            });
        } else {
            for (key, is_dir) in needed_icons {
                crate::app::file_tree::pre_rasterize_icon(key, is_dir);
            }
        }

        // STEP 3: Финальный легкий триггер для перерисовки (иконки появятся)
        let _ = worker_tx.send(FileTreeScanMessage::IconsReady);
    }) {
        let _ = tx.send(FileTreeScanMessage::Failed(format!(
            "не удалось запустить file tree scan worker: {err}"
        )));
    }
    rx
}

pub(super) fn path_has_git_dir(path: &std::path::Path) -> bool {
    path_has_git_dir_for_platform(path, crate::platform::CURRENT_PLATFORM)
}

fn path_has_git_dir_for_platform(
    path: &std::path::Path,
    platform: crate::platform::PlatformKind,
) -> bool {
    if platform == crate::platform::PlatformKind::Windows {
        return path
            .as_os_str()
            .as_encoded_bytes()
            .split(|byte| matches!(byte, b'/' | b'\\'))
            .any(|component| component.eq_ignore_ascii_case(b".git"));
    }
    path.components().any(|component| {
        matches!(component, Component::Normal(name) if name == std::ffi::OsStr::new(".git"))
    })
}

pub(super) fn notify_paths_need_file_tree_refresh<'a>(
    paths: impl IntoIterator<Item = &'a std::path::Path>,
) -> bool {
    paths.into_iter().any(|path| !path_has_git_dir(path))
}

fn push_watch_path(
    path: &Path,
    platform: crate::platform::PlatformKind,
    seen: &mut FxHashSet<crate::platform::PathKey>,
    out: &mut Vec<PathBuf>,
) {
    if seen.insert(crate::platform::PathKey::for_platform(path, platform)) {
        out.push(path.to_path_buf());
    }
}

fn build_file_tree_watch_paths_for_platform(
    roots: &[PathBuf],
    expanded_dirs: &FxHashSet<PathBuf>,
    open_file_parent_dirs: &[PathBuf],
    platform: crate::platform::PlatformKind,
) -> Vec<PathBuf> {
    let mut seen = FxHashSet::default();
    let mut out = Vec::with_capacity(
        roots
            .len()
            .saturating_add(expanded_dirs.len())
            .saturating_add(open_file_parent_dirs.len()),
    );

    for root in roots {
        push_watch_path(root, platform, &mut seen, &mut out);
    }
    let mut expanded = expanded_dirs
        .iter()
        .filter(|dir| {
            roots.iter().any(|root| {
                crate::platform::path_is_within_for_platform(dir, root, platform)
            })
        })
        .collect::<Vec<_>>();
    expanded.sort();
    for dir in expanded {
        push_watch_path(dir, platform, &mut seen, &mut out);
    }
    let mut open_dirs = open_file_parent_dirs.iter().collect::<Vec<_>>();
    open_dirs.sort();
    for dir in open_dirs {
        push_watch_path(dir, platform, &mut seen, &mut out);
    }
    out
}

pub(crate) fn build_file_tree_watch_paths(
    roots: &[PathBuf],
    expanded_dirs: &FxHashSet<PathBuf>,
    open_file_parent_dirs: &[PathBuf],
) -> Vec<PathBuf> {
    build_file_tree_watch_paths_for_platform(
        roots,
        expanded_dirs,
        open_file_parent_dirs,
        crate::platform::CURRENT_PLATFORM,
    )
}

/// Запускает фоновый поток watcher-а через `notify-debouncer-mini`.
/// Отправляет `()` в `tx` при каждом дебаунсированном событии в watched папках.
/// Дебаунс = 300 мс, поэтому спам событий ОС сворачивается в одно сообщение.
#[cfg_attr(coverage_nightly, coverage(off))]
pub fn spawn_watcher(
    paths: Vec<PathBuf>,
    tx: mpsc::Sender<()>,
    stop_rx: mpsc::Receiver<()>,
) -> bool {
    match crate::platform::spawn_named("rriter-file-tree-watcher", move || {
        use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode};

        let (dtx, drx) = mpsc::channel();
        let mut debouncer = match new_debouncer(Duration::from_millis(300), dtx) {
            Ok(d) => d,
            Err(_) => return,
        };

        for path in &paths {
            let _ = debouncer.watcher().watch(path, RecursiveMode::NonRecursive);
        }

        // Блокируемся в цикле — debouncer должен жить, пока работает watcher.
        loop {
            match stop_rx.try_recv() {
                Ok(()) | Err(mpsc::TryRecvError::Disconnected) => break,
                Err(mpsc::TryRecvError::Empty) => {}
            }
            match drx.recv_timeout(Duration::from_millis(250)) {
                Ok(Ok(events)) => {
                    let paths = events.iter().map(|event| event.path.as_path());
                    if notify_paths_need_file_tree_refresh(paths) {
                        if tx.send(()).is_err() {
                            break; // главный поток упал / rx закрыт
                        }
                    }
                }
                Ok(Err(_)) => {}
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => break,
            }
        }
    }) {
        Ok(_) => true,
        Err(err) => {
            eprintln!("RRiter: не удалось запустить file tree watcher: {err}");
            false
        }
    }
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
    fn file_tree_watch_paths_keep_visible_and_open_dirs_only() {
        let root = PathBuf::from("/workspace");
        let mut expanded = FxHashSet::default();
        expanded.insert(root.clone());
        expanded.insert(root.join("src"));
        expanded.insert(PathBuf::from("/other"));
        let open_dirs = vec![root.join("src"), PathBuf::from("/tmp")];

        let paths = build_file_tree_watch_paths(&[root.clone()], &expanded, &open_dirs);

        assert_eq!(
            paths,
            vec![root.clone(), root.join("src"), PathBuf::from("/tmp"),]
        );
    }

    #[test]
    fn windows_git_paths_and_watch_keys_are_case_insensitive() {
        assert!(path_has_git_dir_for_platform(
            Path::new(r"C:\Work\.GIT\index"),
            crate::platform::PlatformKind::Windows,
        ));
        assert!(!path_has_git_dir_for_platform(
            Path::new(r"C:\Work\not.git\index"),
            crate::platform::PlatformKind::Windows,
        ));

        let roots = vec![PathBuf::from(r"C:\Work")];
        let mut expanded = FxHashSet::default();
        expanded.insert(PathBuf::from(r"c:/work"));
        expanded.insert(PathBuf::from(r"C:\WORK\src"));
        let open_dirs = vec![
            PathBuf::from(r"c:\work\SRC"),
            PathBuf::from(r"D:\shared"),
        ];
        let paths = build_file_tree_watch_paths_for_platform(
            &roots,
            &expanded,
            &open_dirs,
            crate::platform::PlatformKind::Windows,
        );

        assert_eq!(paths.len(), 3);
        assert!(crate::platform::paths_equal(&paths[0], &roots[0]) || paths[0] == roots[0]);
        let keys = paths
            .iter()
            .map(|path| {
                crate::platform::PathKey::for_platform(
                    path,
                    crate::platform::PlatformKind::Windows,
                )
            })
            .collect::<FxHashSet<_>>();
        assert_eq!(keys.len(), paths.len());
        assert!(keys.contains(&crate::platform::PathKey::for_platform(
            Path::new(r"C:\work\src"),
            crate::platform::PlatformKind::Windows,
        )));
        assert!(keys.contains(&crate::platform::PathKey::for_platform(
            Path::new(r"D:\shared"),
            crate::platform::PlatformKind::Windows,
        )));
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
