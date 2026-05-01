//! Логика проводника файлов: структуры данных, фоновый скан, методы App.

use crate::app::App;
use crate::editor::Editor;
use rustc_hash::FxHashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Instant;

/// Паттерны-игноры по умолчанию (скрытые, всегда активны).
/// Пользователь не видит их в списке, но они применяются поверх пользовательских.
pub const DEFAULT_IGNORE_PATTERNS: &[&str] = &[
    "__pycache__",
    ".idea",
    ".vscode",
    ".DS_Store",
    "node_modules",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    ".tox",
    ".gradle",
    ".dart_tool",
    ".flutter-plugins",
    ".flutter-plugins-dependencies",
    "*.pyc",
    "*.pyo",
    "*.class",
    "*.o",
    "*.obj",
    ".cache",
    ".env",
    "venv",
    ".venv",
    "Thumbs.db",
    "*.swp",
    "*.swo",
];
pub const FILE_TREE_CONTEXT_MENU_ANIM_SECS: f32 = 0.12;

/// Проверяет, должен ли узел быть скрыт по паттернам.
/// Поддерживает:
///   - точные имена:   `node_modules`, `.DS_Store`
///   - glob-wildcards: `*.pyc`, `foo*`
pub fn matches_ignore_pattern(name: &str, patterns: &[&str]) -> bool {
    for pattern in patterns {
        let p = pattern.trim();
        if p.is_empty() {
            continue;
        }
        if p.starts_with('*') {
            let suffix = &p[1..];
            if name.ends_with(suffix) {
                return true;
            }
        } else if p.ends_with('*') {
            let prefix = &p[..p.len() - 1];
            if name.starts_with(prefix) {
                return true;
            }
        } else if name == p {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Структуры данных
// ---------------------------------------------------------------------------

/// Один узел плоского дерева файлов
#[derive(Clone, Debug)]
pub struct FileNode {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    pub is_dir: bool,
    pub is_expanded: bool,
    /// Ключ иконки из file_icons_map (вычисляется один раз при сборке дерева)
    pub icon_key: &'static str,
    pub is_ignored: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileTreeCreateKind {
    File,
    Directory,
}

impl FileTreeCreateKind {
    pub fn title(self) -> &'static str {
        match self {
            Self::File => "Создать файл",
            Self::Directory => "Создать директорию",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileTreeMenuAction {
    CreateFile,
    CreateDirectory,
    Paste,
    Delete,
    Copy,
    Cut,
    Rename,
    OpenContainedFolder,
    CopyAbsolutePath,
    CopyRelativePath,
}

impl FileTreeMenuAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::CreateFile => "Создать файл",
            Self::CreateDirectory => "Создать директорию",
            Self::Paste => "Вставить",
            Self::Delete => "Удалить",
            Self::Copy => "Копировать",
            Self::Cut => "Вырезать",
            Self::Rename => "Переименовать",
            Self::OpenContainedFolder => "Открыть папку с файлом",
            Self::CopyAbsolutePath => "Скопировать абсолютный путь",
            Self::CopyRelativePath => "Скопировать относительный путь",
        }
    }
}

#[derive(Clone, Debug)]
pub struct FileTreeContextMenu {
    pub x: f32,
    pub y: f32,
    pub target_path: Option<PathBuf>,
    pub target_is_dir: bool,
    pub target_dir: Option<PathBuf>,
    pub entries: Vec<FileTreeMenuAction>,
    pub opened_at: Instant,
}

pub fn file_tree_context_menu_anim_progress(opened_at: Instant, now: Instant) -> f32 {
    let elapsed = now
        .checked_duration_since(opened_at)
        .unwrap_or_default()
        .as_secs_f32();
    let progress = (elapsed / FILE_TREE_CONTEXT_MENU_ANIM_SECS).clamp(0.0, 1.0);
    if progress > 0.98 {
        1.0
    } else {
        progress
    }
}

pub struct FileTreeCreateDialog {
    pub kind: FileTreeCreateKind,
    pub parent_dir: PathBuf,
    pub editor: Editor,
    pub error: Option<String>,
}

pub struct FileTreeRenameDialog {
    pub path: PathBuf,
    pub editor: Editor,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileTreeClipboardMode {
    Copy,
    Cut,
}

#[derive(Clone, Debug)]
pub struct FileTreeClipboard {
    pub mode: FileTreeClipboardMode,
    pub paths: Vec<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct FileTreeDragState {
    pub paths: Vec<PathBuf>,
    pub start_x: f32,
    pub start_y: f32,
    pub current_x: f32,
    pub current_y: f32,
    pub target_idx: Option<usize>,
    pub threshold_passed: bool,
}

#[derive(Clone, Debug)]
pub struct FileTreeMoveDialog {
    pub sources: Vec<PathBuf>,
    pub target_dir: PathBuf,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileTreeDialogInputKind {
    Create,
    Rename,
}

pub fn file_tree_move_dialog_message(sources: &[PathBuf], target_dir: &Path) -> String {
    let target = target_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_else(|| target_dir.to_str().unwrap_or("workspace"));
    if sources.len() == 1 {
        let source = sources[0]
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_else(|| sources[0].to_str().unwrap_or("1 элемент"));
        format!("Переместить '{source}' в '{target}'?")
    } else {
        format!("Переместить {} элементов в '{target}'?", sources.len())
    }
}

pub fn file_tree_overlay_active_for_panel(ide_panel: &crate::app::IdePanelState) -> bool {
    ide_panel.file_tree_context_menu.is_some()
        || ide_panel.file_tree_create_dialog.is_some()
        || ide_panel.file_tree_rename_dialog.is_some()
        || ide_panel.file_tree_move_dialog.is_some()
}

const FILE_TREE_NAME_INPUT_MAX_BYTES: usize = 255;
pub(crate) const FILE_TREE_DIALOG_INPUT_TEXT_SCALE: f32 = 0.92;
pub(crate) const FILE_TREE_DIALOG_W: f32 = 460.0;
pub(crate) const FILE_TREE_DIALOG_SIDE_PAD: f32 = 28.0;

pub(crate) fn file_tree_name_input_scroll_x<F>(
    text: &str,
    cursor: usize,
    visible_width: f32,
    mut char_advance: F,
) -> f32
where
    F: FnMut(char) -> f32,
{
    let mut cursor_total_x = 0.0;
    let mut total_text_width = 0.0;
    for (byte_idx, ch) in text.char_indices() {
        let adv = char_advance(ch);
        if byte_idx < cursor {
            cursor_total_x += adv;
        }
        total_text_width += adv;
    }

    if cursor_total_x > visible_width {
        (cursor_total_x - visible_width)
            .min(total_text_width - visible_width)
            .max(0.0)
    } else {
        0.0
    }
}

pub(crate) fn file_tree_name_input_hit_index<F>(
    text: &str,
    x_offset: f32,
    mut char_advance: F,
) -> usize
where
    F: FnMut(char) -> f32,
{
    let mut current_x = 0.0;
    for (byte_idx, ch) in text.char_indices() {
        let adv = char_advance(ch);
        if x_offset <= current_x + adv / 2.0 {
            return byte_idx;
        }
        current_x += adv;
    }
    text.len()
}

fn insert_file_tree_name_text(editor: &mut Editor, text: &str) {
    let selected_len = editor
        .selection_anchor
        .map(|anchor| anchor.abs_diff(editor.cursor))
        .unwrap_or(0);
    let current_len = editor.get_full_text().len();
    let room =
        FILE_TREE_NAME_INPUT_MAX_BYTES.saturating_sub(current_len.saturating_sub(selected_len));
    if room == 0 {
        return;
    }

    let mut clean = String::new();
    for ch in text.chars() {
        if ch == '\n' || ch == '\r' {
            continue;
        }
        let next_len = clean.len() + ch.len_utf8();
        if next_len > room {
            break;
        }
        clean.push(ch);
    }
    if !clean.is_empty() {
        editor.insert_str(&clean);
    }
}

fn handle_file_tree_name_editor_input(
    editor: &mut Editor,
    physical_key: winit::keyboard::PhysicalKey,
    logical_text: Option<&str>,
    ctrl: bool,
    shift: bool,
    alt: bool,
    super_key: bool,
    paste_text: Option<String>,
) -> Option<String> {
    match physical_key {
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyZ) if ctrl && shift => {
            editor.redo();
            None
        }
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyZ) if ctrl => {
            editor.undo();
            None
        }
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyY) if ctrl => {
            editor.redo();
            None
        }
        winit::keyboard::PhysicalKey::Code(
            winit::keyboard::KeyCode::KeyA | winit::keyboard::KeyCode::KeyF,
        ) if ctrl => {
            editor.select_all();
            None
        }
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyC) if ctrl => {
            editor.get_selection()
        }
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyX) if ctrl => {
            let copy_text = editor.get_selection();
            if copy_text.is_some() {
                editor.delete_selection();
            }
            copy_text
        }
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyV) if ctrl => {
            if let Some(text) = paste_text {
                insert_file_tree_name_text(editor, &text);
            }
            None
        }
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Backspace) => {
            if ctrl {
                editor.delete_word_backward();
            } else {
                editor.backspace();
            }
            None
        }
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Delete) => {
            if ctrl {
                editor.delete_word_forward();
            } else {
                editor.delete_forward();
            }
            None
        }
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowLeft) => {
            if ctrl {
                editor.move_word_left(shift);
            } else {
                editor.move_left(shift);
            }
            None
        }
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowRight) => {
            if ctrl {
                editor.move_word_right(shift);
            } else {
                editor.move_right(shift);
            }
            None
        }
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Home) => {
            editor.move_home(shift);
            None
        }
        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::End) => {
            editor.move_end(shift);
            None
        }
        _ if !ctrl && !alt && !super_key => {
            if let Some(txt) = logical_text {
                insert_file_tree_name_text(editor, txt);
            }
            None
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Вспомогательная функция: читает прямых детей директории через `ignore`
// (уважает .gitignore, пропускает скрытые файлы).
// Возвращает (папки, файлы), обе группы отсортированы натурально.
// ---------------------------------------------------------------------------

use rayon::prelude::*;

pub static RASTERIZED_ICONS: once_cell::sync::Lazy<
    std::sync::Mutex<rustc_hash::FxHashMap<&'static str, Option<Vec<u8>>>>,
> = once_cell::sync::Lazy::new(|| std::sync::Mutex::new(rustc_hash::FxHashMap::default()));

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

fn read_children(dir: &PathBuf) -> (Vec<(String, PathBuf)>, Vec<(String, PathBuf)>) {
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

fn scan_dir_parallel(
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

    let (dirs, files) = read_children(&path);

    // Фильтруем по паттернам ДО параллельного рекурсивного обхода —
    // это экономит поток-часы на игнорируемых поддеревьях.
    let dirs: Vec<_> = dirs
        .into_iter()
        .filter(|(d_name, _)| !matches_ignore_pattern(d_name, all_patterns))
        .collect();
    let files: Vec<_> = files
        .into_iter()
        .filter(|(f_name, _)| !matches_ignore_pattern(f_name, all_patterns))
        .collect();

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

/// Запускает фоновый поток сканирования. Возвращает канал для результата.
pub fn spawn_scan(
    roots: Vec<PathBuf>,
    expanded: FxHashSet<PathBuf>,
    user_patterns: Vec<String>,
) -> mpsc::Receiver<Vec<FileNode>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        // STEP 1: Полный параллельный скан вглубь (работает < 5ms)
        let full_nodes: Vec<FileNode> = roots
            .into_par_iter()
            .flat_map(|root| {
                if !root.exists() {
                    return Vec::new();
                }

                let mut builder = ignore::gitignore::GitignoreBuilder::new(&root);
                let gitignore_path = root.join(".gitignore");
                if gitignore_path.exists() {
                    let _ = builder.add(gitignore_path);
                }
                let gitignore = builder
                    .build()
                    .unwrap_or_else(|_| ignore::gitignore::Gitignore::empty());

                let name = root
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| root.to_string_lossy().into_owned());

                let all_patterns_refs: Vec<&str> =
                    user_patterns.iter().map(|s| s.as_str()).collect();

                scan_dir_parallel(
                    root.clone(),
                    name,
                    0,
                    &expanded,
                    true,
                    10,
                    &gitignore,
                    &all_patterns_refs,
                )
            })
            .collect();

        // Отправляем полное дерево немедленно (текст появится мгновенно)
        let _ = tx.send(full_nodes.clone());

        // STEP 2: Параллельная растеризация иконок без блокировки UI
        let mut needed_icons = rustc_hash::FxHashSet::default();
        for node in &full_nodes {
            needed_icons.insert((node.icon_key, node.is_dir));
        }

        needed_icons.into_par_iter().for_each(|(key, is_dir)| {
            crate::app::file_tree::pre_rasterize_icon(key, is_dir);
        });

        // STEP 3: Финальный триггер для перерисовки (иконки появятся)
        let _ = tx.send(full_nodes);
    });
    rx
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
                Ok(result) => {
                    if result.is_ok() {
                        if tx.send(()).is_err() {
                            break; // главный поток упал / rx закрыт
                        }
                    }
                }
                Err(_) => break,
            }
        }
    });
}

fn validate_child_name(name: &str) -> Result<(), String> {
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

fn is_workspace_path(path: &Path, workspaces: &[PathBuf]) -> bool {
    workspaces.iter().any(|root| path.starts_with(root))
}

fn is_workspace_root(path: &Path, workspaces: &[PathBuf]) -> bool {
    workspaces.iter().any(|root| path == root)
}

fn can_modify_path(path: &Path, workspaces: &[PathBuf]) -> bool {
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

fn unique_child_path(target_dir: &Path, name: &str) -> PathBuf {
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

fn copy_path_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
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

fn delete_path(path: &Path) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(path)?;
    if meta.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

fn move_path_to_dir(src: &Path, target_dir: &Path) -> Result<(PathBuf, PathBuf), String> {
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

fn rename_path(path: &Path, new_name: &str, workspaces: &[PathBuf]) -> Result<PathBuf, String> {
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

fn path_after_rename(path: &Path, old_root: &Path, new_root: &Path) -> Option<PathBuf> {
    if path == old_root {
        Some(new_root.to_path_buf())
    } else {
        path.strip_prefix(old_root)
            .ok()
            .map(|rel| new_root.join(rel))
    }
}

fn copy_paths_to_dir(paths: &[PathBuf], target_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut copied = Vec::new();
    for src in paths {
        if !src.exists() {
            return Err(format!("Не найдено: {}", src.display()));
        }
        if src.is_dir() && target_dir.starts_with(src) {
            return Err("Нельзя копировать папку внутрь самой себя".to_string());
        }
        let Some(name) = src.file_name().and_then(|name| name.to_str()) else {
            return Err("Не удалось прочитать имя".to_string());
        };
        let dst = unique_child_path(target_dir, name);
        copy_path_recursive(src, &dst).map_err(|err| err.to_string())?;
        copied.push(dst);
    }
    Ok(copied)
}

fn delete_paths(paths: &[PathBuf], workspaces: &[PathBuf]) -> Result<(), String> {
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

fn selected_paths(
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

impl App {
    /// Запускает фоновый скан дерева. Вызывать при открытии Explorer,
    /// добавлении workspace или разворачивании папки.
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn refresh_file_tree(&mut self) {
        let roots = self.ide_workspaces.clone();
        if roots.is_empty() {
            self.ide_panel.file_tree_nodes.clear();
            return;
        }
        // Новые корни (которых ещё нет в дереве) автоматически раскрываем.
        // Уже существующие корни не трогаем — пользователь мог их свернуть.
        let existing_roots: rustc_hash::FxHashSet<std::path::PathBuf> = self
            .ide_panel
            .file_tree_nodes
            .iter()
            .filter(|n| n.depth == 0)
            .map(|n| n.path.clone())
            .collect();
        for root in &roots {
            if !existing_roots.contains(root) {
                self.ide_panel.file_tree_expanded.insert(root.clone());
            }
        }
        let expanded = self.ide_panel.file_tree_expanded.clone();
        let patterns = self.ide_ignore_patterns.clone();
        self.file_tree_rx = Some(spawn_scan(roots, expanded, patterns));
    }

    /// Поллит канал результатов фонового скана.
    /// Возвращает true если пришли новые данные (нужен redraw).
    /// Вызывать из about_to_wait.
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn poll_file_tree(&mut self) -> bool {
        let mut updated = false;
        if let Some(rx) = &self.file_tree_rx {
            while let Ok(nodes) = rx.try_recv() {
                self.ide_panel.file_tree_nodes = nodes;
                self.ide_panel
                    .file_tree_selection
                    .retain(|path| path.exists());
                updated = true;
            }
            if let Err(std::sync::mpsc::TryRecvError::Disconnected) = rx.try_recv() {
                self.file_tree_rx = None;
            }
        }
        updated
    }

    /// Запускает (или перезапускает) фоновый watcher для текущих workspaces.
    /// Старый watcher бricht автоматически, т.к. его `Sender` дропается вместе с rx.
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn start_file_watcher(&mut self) {
        if self.ide_workspaces.is_empty() {
            self.file_tree_notify_rx = None;
            return;
        }
        let paths = self.ide_workspaces.clone();
        let (tx, rx) = mpsc::channel();
        self.file_tree_notify_rx = Some(rx);
        crate::app::file_tree::spawn_watcher(paths, tx);
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn toggle_file_tree_dir(&mut self, node_idx: usize) {
        let node = match self.ide_panel.file_tree_nodes.get(node_idx) {
            Some(n) => n.clone(),
            None => return,
        };
        if !node.is_dir {
            return;
        }
        if node.is_expanded {
            self.ide_panel.file_tree_expanded.remove(&node.path);
        } else {
            self.ide_panel.file_tree_expanded.insert(node.path.clone());
        }
        self.refresh_file_tree();
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn open_file_tree_node(&mut self, node_idx: usize) {
        let node = match self.ide_panel.file_tree_nodes.get(node_idx) {
            Some(n) => n.clone(),
            None => return,
        };
        if node.is_dir {
            self.toggle_file_tree_dir(node_idx);
        } else {
            self.open_file_in_tab(node.path, false);
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn handle_file_tree_left_click(&mut self, node_idx: usize, arrow: bool) {
        let Some(node) = self.ide_panel.file_tree_nodes.get(node_idx).cloned() else {
            return;
        };
        self.ide_panel.file_tree_context_menu = None;
        self.ide_panel.file_tree_focused = true;
        self.ide_panel.terminal_focused = false;

        if arrow && node.is_dir {
            self.ide_panel.file_tree_selection.clear();
            self.ide_panel.file_tree_selection.insert(node.path);
            self.toggle_file_tree_dir(node_idx);
            return;
        }

        let ctrl = self.modifiers.control_key() || self.modifiers.super_key();
        if ctrl {
            if !self.ide_panel.file_tree_selection.remove(&node.path) {
                self.ide_panel.file_tree_selection.insert(node.path.clone());
            }
        } else {
            self.ide_panel.file_tree_selection.clear();
            self.ide_panel.file_tree_selection.insert(node.path.clone());
        }

        let (mx, my) = self
            .renderer
            .as_ref()
            .map(|r| (r.last_mouse_x, r.last_mouse_y))
            .unwrap_or((0.0, 0.0));
        let paths = self.file_tree_selected_paths_for(&node.path);
        self.ide_panel.file_tree_drag = Some(FileTreeDragState {
            paths,
            start_x: mx,
            start_y: my,
            current_x: mx,
            current_y: my,
            target_idx: None,
            threshold_passed: false,
        });

        let now = std::time::Instant::now();
        let double_click = (self.last_click_pos.0 - mx).abs() < 5.0
            && (self.last_click_pos.1 - my).abs() < 5.0
            && now.duration_since(self.last_click_time).as_millis() < 400;
        self.last_click_time = now;
        self.last_click_pos = (mx, my);
        if double_click {
            self.ide_panel.file_tree_drag = None;
            self.open_file_tree_node(node_idx);
        }
    }

    pub fn file_tree_selected_paths_for(&self, fallback: &Path) -> Vec<PathBuf> {
        selected_paths(
            &self.ide_panel.file_tree_nodes,
            &self.ide_panel.file_tree_selection,
            fallback,
        )
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn open_file_tree_context_menu(&mut self, mx: f32, my: f32) {
        let target_idx = self.file_tree_node_at(mx, my);
        let target = target_idx.and_then(|idx| self.ide_panel.file_tree_nodes.get(idx).cloned());
        if let Some(node) = &target {
            if !self.ide_panel.file_tree_selection.contains(&node.path) {
                self.ide_panel.file_tree_selection.clear();
                self.ide_panel.file_tree_selection.insert(node.path.clone());
            }
            self.ide_panel.file_tree_focused = true;
        }

        let target_path = target.as_ref().map(|node| node.path.clone());
        let target_is_dir = target.as_ref().is_some_and(|node| node.is_dir);
        let target_dir = target
            .as_ref()
            .and_then(|node| {
                if node.is_dir {
                    Some(node.path.clone())
                } else {
                    node.path.parent().map(Path::to_path_buf)
                }
            })
            .or_else(|| self.ide_workspaces.first().cloned());

        let mut entries = vec![
            FileTreeMenuAction::CreateFile,
            FileTreeMenuAction::CreateDirectory,
        ];
        if self.ide_panel.file_tree_clipboard.is_some() {
            entries.push(FileTreeMenuAction::Paste);
        }
        if target_path.is_some() {
            let selected_count = self.ide_panel.file_tree_selection.len();
            entries.extend([
                FileTreeMenuAction::Delete,
                FileTreeMenuAction::Copy,
                FileTreeMenuAction::Cut,
            ]);
            if selected_count == 1 {
                entries.push(FileTreeMenuAction::Rename);
            }
            entries.extend([
                FileTreeMenuAction::OpenContainedFolder,
                FileTreeMenuAction::CopyAbsolutePath,
                FileTreeMenuAction::CopyRelativePath,
            ]);
        }

        self.ide_panel.file_tree_context_menu = Some(FileTreeContextMenu {
            x: mx,
            y: my,
            target_path,
            target_is_dir,
            target_dir,
            entries,
            opened_at: Instant::now(),
        });
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn handle_file_tree_context_item(&mut self, idx: usize) {
        let Some(menu) = self.ide_panel.file_tree_context_menu.clone() else {
            return;
        };
        let Some(action) = menu.entries.get(idx).copied() else {
            return;
        };
        self.ide_panel.file_tree_context_menu = None;
        self.handle_file_tree_menu_action(action, menu);
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn handle_file_tree_menu_action(
        &mut self,
        action: FileTreeMenuAction,
        menu: FileTreeContextMenu,
    ) {
        match action {
            FileTreeMenuAction::CreateFile => {
                if let Some(parent_dir) = menu.target_dir {
                    self.open_file_tree_create_dialog(FileTreeCreateKind::File, parent_dir);
                }
            }
            FileTreeMenuAction::CreateDirectory => {
                if let Some(parent_dir) = menu.target_dir {
                    self.open_file_tree_create_dialog(FileTreeCreateKind::Directory, parent_dir);
                }
            }
            FileTreeMenuAction::Paste => {
                if let Some(target_dir) = menu.target_dir {
                    let _ = self.paste_file_tree_clipboard(target_dir);
                }
            }
            FileTreeMenuAction::Delete => {
                if let Some(target_path) = menu.target_path {
                    let paths = self.file_tree_selected_paths_for(&target_path);
                    let _ = self.delete_file_tree_paths(paths);
                }
            }
            FileTreeMenuAction::Copy => {
                if let Some(target_path) = menu.target_path {
                    self.copy_file_tree_paths(target_path, FileTreeClipboardMode::Copy);
                }
            }
            FileTreeMenuAction::Cut => {
                if let Some(target_path) = menu.target_path {
                    self.copy_file_tree_paths(target_path, FileTreeClipboardMode::Cut);
                }
            }
            FileTreeMenuAction::Rename => {
                if let Some(path) = self.file_tree_single_selected_path() {
                    self.open_file_tree_rename_dialog(path);
                }
            }
            FileTreeMenuAction::OpenContainedFolder => {
                if let Some(target_path) = menu.target_path {
                    self.open_contained_folder(&target_path, menu.target_is_dir);
                }
            }
            FileTreeMenuAction::CopyAbsolutePath => {
                if let Some(target_path) = menu.target_path {
                    let paths = self.file_tree_selected_paths_for(&target_path);
                    let text = paths
                        .iter()
                        .map(|p| p.to_string_lossy())
                        .collect::<Vec<_>>()
                        .join("\n");
                    self.set_clipboard_text(text);
                }
            }
            FileTreeMenuAction::CopyRelativePath => {
                if let Some(target_path) = menu.target_path {
                    let paths = self.file_tree_selected_paths_for(&target_path);
                    let text = paths
                        .iter()
                        .map(|p| {
                            relative_path_for_workspace(p, &self.ide_workspaces)
                                .to_string_lossy()
                                .into_owned()
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    self.set_clipboard_text(text);
                }
            }
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn open_file_tree_create_dialog(&mut self, kind: FileTreeCreateKind, parent_dir: PathBuf) {
        self.ide_panel.file_tree_create_dialog = Some(FileTreeCreateDialog {
            kind,
            parent_dir,
            editor: Editor::new(256),
            error: None,
        });
        self.ide_panel.file_tree_context_menu = None;
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn open_file_tree_rename_dialog(&mut self, path: PathBuf) {
        if !can_modify_path(&path, &self.ide_workspaces) {
            return;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            return;
        }
        let mut editor = Editor::new(name.len() + 64);
        let _ = editor.insert_str(&name);
        editor.select_all();
        self.ide_panel.file_tree_rename_dialog = Some(FileTreeRenameDialog {
            path,
            editor,
            error: None,
        });
        self.ide_panel.file_tree_context_menu = None;
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn submit_file_tree_create_dialog(&mut self) {
        let Some(dialog) = self.ide_panel.file_tree_create_dialog.as_mut() else {
            return;
        };
        let name = dialog.editor.get_full_text().trim().to_string();
        if let Err(err) = validate_child_name(&name) {
            dialog.error = Some(err);
            return;
        }
        if !is_workspace_path(&dialog.parent_dir, &self.ide_workspaces) {
            dialog.error = Some("Путь вне workspace".to_string());
            return;
        }
        let path = dialog.parent_dir.join(&name);
        if path.exists() {
            dialog.error = Some("Уже существует".to_string());
            return;
        }
        let result = match dialog.kind {
            FileTreeCreateKind::File => std::fs::File::create(&path).map(|_| ()),
            FileTreeCreateKind::Directory => std::fs::create_dir(&path),
        };
        if let Err(err) = result {
            dialog.error = Some(err.to_string());
            return;
        }
        self.ide_panel
            .file_tree_expanded
            .insert(dialog.parent_dir.clone());
        self.ide_panel.file_tree_selection.clear();
        self.ide_panel.file_tree_selection.insert(path);
        self.ide_panel.file_tree_create_dialog = None;
        self.refresh_file_tree();
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn submit_file_tree_rename_dialog(&mut self) {
        let Some(dialog) = self.ide_panel.file_tree_rename_dialog.as_mut() else {
            return;
        };
        let old_path = dialog.path.clone();
        let new_name = dialog.editor.get_full_text().trim().to_string();
        match rename_path(&old_path, &new_name, &self.ide_workspaces) {
            Ok(new_path) => {
                self.update_open_paths_after_file_tree_rename(&old_path, &new_path);
                self.ide_panel.file_tree_selection.clear();
                self.ide_panel.file_tree_selection.insert(new_path);
                self.ide_panel.file_tree_rename_dialog = None;
                self.refresh_file_tree();
            }
            Err(err) => {
                dialog.error = Some(err);
            }
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn update_open_paths_after_file_tree_rename(&mut self, old_path: &Path, new_path: &Path) {
        if let Some(current_path) = self.file_path.clone() {
            if let Some(updated) = path_after_rename(&current_path, old_path, new_path) {
                let old_ext = current_path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .unwrap_or("")
                    .to_string();
                self.file_path = Some(updated.clone());
                self.base_title = updated
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Безымянный".to_string());
                self.file_extension = updated
                    .extension()
                    .map(|ext| ext.to_string_lossy().to_string())
                    .unwrap_or_default();
                if let Some(lsp) = &mut self.lsp {
                    lsp.notify_close(&current_path, &old_ext);
                    let text = self.editor.get_full_text();
                    lsp.notify_open(
                        &updated,
                        &self.file_extension,
                        &text,
                        self.editor.version as i32,
                    );
                }
                self.highlighter.reset(
                    self.editor.version,
                    self.editor.get_full_text(),
                    self.file_extension.clone(),
                );
            }
        }

        for tab in &mut self.tabs {
            if let Some(path) = tab.file_path.clone() {
                if let Some(updated) = path_after_rename(&path, old_path, new_path) {
                    tab.file_path = Some(updated.clone());
                    tab.base_title = updated
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "Безымянный".to_string());
                    tab.file_extension = updated
                        .extension()
                        .map(|ext| ext.to_string_lossy().to_string())
                        .unwrap_or_default();
                    tab.icon_key =
                        crate::app::file_icons::file_icon_key(&tab.base_title.to_ascii_lowercase());
                }
            }
        }

        if let Some(w) = self.window.as_ref() {
            App::update_window_title(w, &self.base_title, self.editor.is_dirty());
            w.request_redraw();
        }
        self.save_tabs_state();
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn copy_file_tree_paths(&mut self, fallback: PathBuf, mode: FileTreeClipboardMode) {
        let paths = self.file_tree_selected_paths_for(&fallback);
        self.ide_panel.file_tree_clipboard = Some(FileTreeClipboard { mode, paths });
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn paste_file_tree_clipboard(&mut self, target_dir: PathBuf) -> Result<(), String> {
        let Some(clipboard) = self.ide_panel.file_tree_clipboard.clone() else {
            return Ok(());
        };
        if !is_workspace_path(&target_dir, &self.ide_workspaces) {
            return Err("Путь вне workspace".to_string());
        }
        if !target_dir.is_dir() {
            return Err("Цель не директория".to_string());
        }
        let result = match clipboard.mode {
            FileTreeClipboardMode::Copy => {
                let copied = copy_paths_to_dir(&clipboard.paths, &target_dir)?;
                self.ide_panel.file_tree_selection.clear();
                self.ide_panel.file_tree_selection.extend(copied);
                Ok(())
            }
            FileTreeClipboardMode::Cut => {
                for path in &clipboard.paths {
                    if !can_modify_path(path, &self.ide_workspaces) {
                        return Err("Можно вырезать только элементы внутри workspace".to_string());
                    }
                }
                let mut moved = Vec::new();
                for src in &clipboard.paths {
                    let (_, dst) = move_path_to_dir(src, &target_dir)?;
                    moved.push(dst);
                }
                self.ide_panel.file_tree_selection.clear();
                self.ide_panel.file_tree_selection.extend(moved);
                self.ide_panel.file_tree_clipboard = None;
                Ok(())
            }
        };
        if result.is_ok() {
            self.ide_panel.file_tree_expanded.insert(target_dir);
            self.refresh_file_tree();
        }
        result
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn delete_file_tree_paths(&mut self, paths: Vec<PathBuf>) -> Result<(), String> {
        delete_paths(&paths, &self.ide_workspaces)?;
        self.ide_panel.file_tree_selection.clear();
        self.refresh_file_tree();
        Ok(())
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn file_tree_default_paste_dir(&self) -> Option<PathBuf> {
        for node in &self.ide_panel.file_tree_nodes {
            if self.ide_panel.file_tree_selection.contains(&node.path) {
                if node.is_dir {
                    return Some(node.path.clone());
                }
                return node.path.parent().map(Path::to_path_buf);
            }
        }
        self.ide_workspaces.first().cloned()
    }

    pub fn file_tree_single_selected_path(&self) -> Option<PathBuf> {
        if self.ide_panel.file_tree_selection.len() == 1 {
            self.ide_panel.file_tree_selection.iter().next().cloned()
        } else {
            None
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn finish_file_tree_move(&mut self) {
        let Some(dialog) = self.ide_panel.file_tree_move_dialog.as_mut() else {
            return;
        };
        if !is_workspace_path(&dialog.target_dir, &self.ide_workspaces) {
            dialog.error = Some("Путь вне workspace".to_string());
            return;
        }
        let mut moved = Vec::new();
        for src in &dialog.sources {
            if !can_modify_path(src, &self.ide_workspaces) {
                dialog.error =
                    Some("Можно перемещать только элементы внутри workspace".to_string());
                return;
            }
            match move_path_to_dir(src, &dialog.target_dir) {
                Ok((_, dst)) => moved.push(dst),
                Err(err) => {
                    dialog.error = Some(err);
                    return;
                }
            }
        }
        let target_dir = dialog.target_dir.clone();
        self.ide_panel.file_tree_move_dialog = None;
        self.ide_panel.file_tree_selection.clear();
        self.ide_panel.file_tree_selection.extend(moved);
        self.ide_panel.file_tree_expanded.insert(target_dir);
        self.refresh_file_tree();
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn open_contained_folder(&mut self, path: &Path, is_dir: bool) {
        let folder = if is_dir {
            path.to_path_buf()
        } else {
            path.parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| path.to_path_buf())
        };
        #[cfg(target_os = "windows")]
        let _ = std::process::Command::new("explorer").arg(folder).spawn();
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("open").arg(folder).spawn();
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let _ = std::process::Command::new("xdg-open").arg(folder).spawn();
    }

    /// Возвращает индекс узла дерева под экранными координатами (mx, my),
    /// или None если курсор не над областью дерева файлов.
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn file_tree_node_at(&self, mx: f32, my: f32) -> Option<usize> {
        if self.show_settings || self.dialog_window.is_some() {
            return None;
        }
        if !self.is_ide_mode {
            return None;
        }
        if !self.ide_panel.is_open(crate::app::PanelId::Explorer) {
            return None;
        }
        let r = self.renderer.as_ref()?;
        let s = r.scale_factor;
        let sb_w = 48.0 * s;
        let panel_left_w = if self.ide_panel.any_top_open() {
            self.ide_panel.left_width * s
        } else {
            return None;
        };
        if mx < sb_w || mx > sb_w + panel_left_w {
            return None;
        }
        let title_h = 32.0 * s;
        if my < title_h {
            return None;
        }
        let row_h = 28.0 * s;
        let content_y = my - title_h + self.ide_panel.explorer_scroll.current;
        if content_y < 0.0 {
            return None;
        }
        let idx = (content_y / row_h) as usize;
        if idx < self.ide_panel.file_tree_nodes.len() {
            Some(idx)
        } else {
            None
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn file_tree_panel_contains(&self, mx: f32, my: f32) -> bool {
        if self.show_settings || self.dialog_window.is_some() || !self.is_ide_mode {
            return false;
        }
        if !self.ide_panel.is_open(crate::app::PanelId::Explorer) {
            return false;
        }
        let Some(r) = self.renderer.as_ref() else {
            return false;
        };
        let s = r.scale_factor;
        let sb_w = 48.0 * s;
        let panel_left_w = self.ide_panel.left_width * s;
        let title_h = 32.0 * s;
        mx >= sb_w && mx <= sb_w + panel_left_w && my >= title_h
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn file_tree_drop_target_dir(&self, target_idx: Option<usize>) -> Option<PathBuf> {
        let idx = target_idx?;
        let node = self.ide_panel.file_tree_nodes.get(idx)?;
        if node.is_dir {
            Some(node.path.clone())
        } else {
            node.path.parent().map(Path::to_path_buf)
        }
    }

    pub fn file_tree_overlay_active(&self) -> bool {
        file_tree_overlay_active_for_panel(&self.ide_panel)
    }

    pub fn ui_id_is_file_tree_overlay(id: crate::ui_system::UiId) -> bool {
        matches!(
            id,
            crate::ui_system::UiId::FileTreeMenuItem(_)
                | crate::ui_system::UiId::FileTreeCreateInput
                | crate::ui_system::UiId::FileTreeCreateConfirm
                | crate::ui_system::UiId::FileTreeCreateCancel
                | crate::ui_system::UiId::FileTreeRenameInput
                | crate::ui_system::UiId::FileTreeRenameConfirm
                | crate::ui_system::UiId::FileTreeRenameCancel
                | crate::ui_system::UiId::FileTreeMoveConfirm
                | crate::ui_system::UiId::FileTreeMoveCancel
        )
    }
}

impl App {
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn file_tree_dialog_input_index_at(
        &mut self,
        kind: FileTreeDialogInputKind,
        mx: f32,
    ) -> Option<usize> {
        let (text, cursor) = match kind {
            FileTreeDialogInputKind::Create => {
                let dialog = self.ide_panel.file_tree_create_dialog.as_ref()?;
                (dialog.editor.get_full_text(), dialog.editor.cursor)
            }
            FileTreeDialogInputKind::Rename => {
                let dialog = self.ide_panel.file_tree_rename_dialog.as_ref()?;
                (dialog.editor.get_full_text(), dialog.editor.cursor)
            }
        };

        let r = self.renderer.as_mut()?;
        let s = r.scale_factor;
        let w = (FILE_TREE_DIALOG_W * s).min(r.width - 32.0 * s);
        let x = ((r.width - w) / 2.0).round();
        let input_x = x + FILE_TREE_DIALOG_SIDE_PAD * s;
        let input_w = w - FILE_TREE_DIALOG_SIDE_PAD * 2.0 * s;
        let pad_x = 8.0 * s;
        let visible_width = (input_w - pad_x * 2.0).max(0.0);
        let scale = FILE_TREE_DIALOG_INPUT_TEXT_SCALE;
        let scroll_x = file_tree_name_input_scroll_x(&text, cursor, visible_width, |ch| {
            r.get_ui_glyph(ch)
                .map(|g| g.advance * scale)
                .unwrap_or(10.0 * scale)
        });
        let x_offset = (mx - (input_x + pad_x) + scroll_x).max(0.0);
        Some(file_tree_name_input_hit_index(&text, x_offset, |ch| {
            r.get_ui_glyph(ch)
                .map(|g| g.advance * scale)
                .unwrap_or(10.0 * scale)
        }))
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn set_file_tree_dialog_input_cursor(
        &mut self,
        kind: FileTreeDialogInputKind,
        target_idx: usize,
        reset_anchor: bool,
    ) {
        let editor = match kind {
            FileTreeDialogInputKind::Create => self
                .ide_panel
                .file_tree_create_dialog
                .as_mut()
                .map(|dialog| &mut dialog.editor),
            FileTreeDialogInputKind::Rename => self
                .ide_panel
                .file_tree_rename_dialog
                .as_mut()
                .map(|dialog| &mut dialog.editor),
        };
        if let Some(editor) = editor {
            editor.cursor = target_idx;
            if reset_anchor || editor.selection_anchor.is_none() {
                editor.selection_anchor = Some(target_idx);
            }
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn handle_file_tree_modal_keyboard(&mut self, key_event: &winit::event::KeyEvent) -> bool {
        if self.ide_panel.file_tree_context_menu.is_some() {
            if key_event.state == winit::event::ElementState::Pressed
                && key_event.physical_key
                    == winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape)
            {
                self.ide_panel.file_tree_context_menu = None;
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            return true;
        }

        if self.ide_panel.file_tree_move_dialog.is_some() {
            if key_event.state != winit::event::ElementState::Pressed {
                return true;
            }
            match key_event.physical_key {
                winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape) => {
                    self.ide_panel.file_tree_move_dialog = None;
                }
                winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Enter)
                | winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::NumpadEnter) => {
                    self.finish_file_tree_move();
                }
                _ => {}
            }
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            self.last_action = std::time::Instant::now();
            self.last_blink_state = true;
            return true;
        }

        if self.ide_panel.file_tree_rename_dialog.is_some() {
            if key_event.state != winit::event::ElementState::Pressed {
                return true;
            }

            let ctrl = self.modifiers.control_key() || self.modifiers.super_key();
            let shift = self.modifiers.shift_key();
            let mut submit = false;
            let mut cancel = false;
            let paste_text = if ctrl
                && key_event.physical_key
                    == winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyV)
            {
                self.get_clipboard_text()
            } else {
                None
            };
            let mut copy_text: Option<String> = None;

            if let Some(dialog) = self.ide_panel.file_tree_rename_dialog.as_mut() {
                dialog.error = None;
                match key_event.physical_key {
                    winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape) => {
                        cancel = true;
                    }
                    winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Enter)
                    | winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::NumpadEnter) => {
                        submit = true;
                    }
                    _ => {
                        copy_text = handle_file_tree_name_editor_input(
                            &mut dialog.editor,
                            key_event.physical_key,
                            key_event.logical_key.to_text(),
                            ctrl,
                            shift,
                            self.modifiers.alt_key(),
                            self.modifiers.super_key(),
                            paste_text,
                        );
                    }
                }
            }

            if let Some(text) = copy_text {
                self.set_clipboard_text(text);
            }
            if cancel {
                self.ide_panel.file_tree_rename_dialog = None;
            } else if submit {
                self.submit_file_tree_rename_dialog();
            }
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            self.last_action = std::time::Instant::now();
            self.last_blink_state = true;
            return true;
        }

        if self.ide_panel.file_tree_create_dialog.is_none() {
            return false;
        }
        if key_event.state != winit::event::ElementState::Pressed {
            return true;
        }

        let ctrl = self.modifiers.control_key() || self.modifiers.super_key();
        let shift = self.modifiers.shift_key();
        let mut submit = false;
        let mut cancel = false;
        let paste_text = if ctrl
            && key_event.physical_key
                == winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyV)
        {
            self.get_clipboard_text()
        } else {
            None
        };
        let mut copy_text: Option<String> = None;

        if let Some(dialog) = self.ide_panel.file_tree_create_dialog.as_mut() {
            dialog.error = None;
            match key_event.physical_key {
                winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape) => {
                    cancel = true;
                }
                winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Enter)
                | winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::NumpadEnter) => {
                    submit = true;
                }
                _ => {
                    copy_text = handle_file_tree_name_editor_input(
                        &mut dialog.editor,
                        key_event.physical_key,
                        key_event.logical_key.to_text(),
                        ctrl,
                        shift,
                        self.modifiers.alt_key(),
                        self.modifiers.super_key(),
                        paste_text,
                    );
                }
            }
        }

        if let Some(text) = copy_text {
            self.set_clipboard_text(text);
        }
        if cancel {
            self.ide_panel.file_tree_create_dialog = None;
        } else if submit {
            self.submit_file_tree_create_dialog();
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
        self.last_action = std::time::Instant::now();
        self.last_blink_state = true;
        true
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn handle_file_tree_shortcut(
        &mut self,
        physical_key: winit::keyboard::PhysicalKey,
        ctrl: bool,
    ) -> bool {
        if !self.ide_panel.file_tree_focused || self.show_settings {
            return false;
        }
        if self.ide_panel.file_tree_selection.is_empty() {
            return false;
        }
        if physical_key == winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::F2) {
            if let Some(path) = self.file_tree_single_selected_path() {
                self.open_file_tree_rename_dialog(path);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
                return true;
            }
            return false;
        }
        if !ctrl {
            return false;
        }
        let fallback = match self.ide_panel.file_tree_selection.iter().next() {
            Some(path) => path.clone(),
            None => return false,
        };
        match physical_key {
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyC) => {
                self.copy_file_tree_paths(fallback, FileTreeClipboardMode::Copy);
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyX) => {
                self.copy_file_tree_paths(fallback, FileTreeClipboardMode::Cut);
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyV) => {
                if let Some(target_dir) = self.file_tree_default_paste_dir() {
                    let _ = self.paste_file_tree_clipboard(target_dir);
                }
            }
            _ => return false,
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("rriter_{name}_{}", std::process::id()))
    }

    #[test]
    fn file_tree_ignore_patterns_cover_exact_prefix_and_suffix() {
        let patterns = ["node_modules", "*.pyc", "target*"];

        assert!(matches_ignore_pattern("node_modules", &patterns));
        assert!(matches_ignore_pattern("main.pyc", &patterns));
        assert!(matches_ignore_pattern("target-debug", &patterns));
        assert!(!matches_ignore_pattern("src", &patterns));
    }

    #[test]
    fn file_tree_name_input_stays_single_line_and_bounded() {
        let mut editor = Editor::new(16);
        insert_file_tree_name_text(&mut editor, "alpha\nbeta\r");

        assert_eq!(editor.get_full_text(), "alphabeta");

        editor.select_all();
        insert_file_tree_name_text(&mut editor, &"x".repeat(FILE_TREE_NAME_INPUT_MAX_BYTES + 20));

        assert_eq!(editor.get_full_text().len(), FILE_TREE_NAME_INPUT_MAX_BYTES);
    }

    #[test]
    fn file_tree_name_input_edit_keys_cover_undo_redo_and_copy() {
        let mut editor = Editor::new(16);
        handle_file_tree_name_editor_input(
            &mut editor,
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyA),
            Some("a"),
            false,
            false,
            false,
            false,
            None,
        );
        handle_file_tree_name_editor_input(
            &mut editor,
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyB),
            Some("b"),
            false,
            false,
            false,
            false,
            None,
        );
        assert_eq!(editor.get_full_text(), "ab");

        handle_file_tree_name_editor_input(
            &mut editor,
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyZ),
            None,
            true,
            false,
            false,
            false,
            None,
        );
        assert_eq!(editor.get_full_text(), "");

        handle_file_tree_name_editor_input(
            &mut editor,
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyY),
            None,
            true,
            false,
            false,
            false,
            None,
        );
        assert_eq!(editor.get_full_text(), "ab");

        handle_file_tree_name_editor_input(
            &mut editor,
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyZ),
            None,
            true,
            false,
            false,
            false,
            None,
        );
        handle_file_tree_name_editor_input(
            &mut editor,
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyZ),
            None,
            true,
            true,
            false,
            false,
            None,
        );
        assert_eq!(editor.get_full_text(), "ab");

        handle_file_tree_name_editor_input(
            &mut editor,
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyA),
            None,
            true,
            false,
            false,
            false,
            None,
        );
        let copied = handle_file_tree_name_editor_input(
            &mut editor,
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyC),
            None,
            true,
            false,
            false,
            false,
            None,
        );
        assert_eq!(copied.as_deref(), Some("ab"));
    }

    #[test]
    fn file_tree_name_input_hit_testing_accounts_for_scroll() {
        let text = "abcdef";
        let scroll_x = file_tree_name_input_scroll_x(text, text.len(), 30.0, |_| 10.0);

        assert_eq!(scroll_x, 30.0);
        assert_eq!(
            file_tree_name_input_hit_index(text, 5.0 + scroll_x, |_| 10.0),
            3
        );
        assert_eq!(file_tree_name_input_hit_index(text, 500.0, |_| 10.0), text.len());
    }

    #[test]
    fn file_tree_scan_sorts_expands_and_skips_ignored_nodes_end_to_end() {
        let root = test_root("file_tree_scan");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("dir10")).unwrap();
        std::fs::create_dir_all(root.join("dir2")).unwrap();
        std::fs::create_dir_all(root.join("__pycache__")).unwrap();
        std::fs::write(root.join("b.txt"), "b").unwrap();
        std::fs::write(root.join("a.py"), "a").unwrap();
        std::fs::write(root.join("z.pyc"), "ignored").unwrap();

        let mut expanded = FxHashSet::default();
        expanded.insert(root.clone());
        let gitignore = ignore::gitignore::Gitignore::empty();
        let nodes = scan_dir_parallel(
            root.clone(),
            "workspace".to_string(),
            0,
            &expanded,
            true,
            2,
            &gitignore,
            DEFAULT_IGNORE_PATTERNS,
        );
        let names: Vec<_> = nodes.iter().map(|node| node.name.as_str()).collect();

        assert_eq!(names, vec!["workspace", "dir2", "dir10", "a.py", "b.txt"]);
        assert!(nodes[0].is_expanded);
        assert_eq!(nodes[1].depth, 1);
        assert!(nodes.iter().all(|node| node.name != "__pycache__"));
        assert!(nodes.iter().all(|node| node.name != "z.pyc"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn file_tree_scan_covers_collapsed_depth_limit_and_gitignore_marks() {
        let root = test_root("file_tree_depth_gitignore");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("alpha").join("nested")).unwrap();
        std::fs::create_dir_all(root.join("ignored_dir")).unwrap();
        std::fs::write(root.join("alpha").join("nested").join("deep.py"), "deep").unwrap();
        std::fs::write(root.join("ignored.txt"), "ignored").unwrap();
        std::fs::write(root.join(".gitignore"), "ignored.txt\nignored_dir/\n").unwrap();

        let mut builder = ignore::gitignore::GitignoreBuilder::new(&root);
        let _ = builder.add(root.join(".gitignore"));
        let gitignore = builder.build().unwrap();

        let collapsed = FxHashSet::default();
        let collapsed_nodes = scan_dir_parallel(
            root.clone(),
            "workspace".to_string(),
            0,
            &collapsed,
            true,
            10,
            &gitignore,
            DEFAULT_IGNORE_PATTERNS,
        );
        assert_eq!(collapsed_nodes.len(), 1);
        assert_eq!(collapsed_nodes[0].name, "workspace");
        assert!(!collapsed_nodes[0].is_expanded);
        assert!(!collapsed_nodes[0].is_ignored);

        let mut expanded = FxHashSet::default();
        expanded.insert(root.clone());
        expanded.insert(root.join("alpha"));
        let nodes = scan_dir_parallel(
            root.clone(),
            "workspace".to_string(),
            0,
            &expanded,
            true,
            1,
            &gitignore,
            DEFAULT_IGNORE_PATTERNS,
        );
        let names: Vec<_> = nodes.iter().map(|node| node.name.as_str()).collect();

        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"ignored_dir"));
        assert!(names.contains(&"ignored.txt"));
        assert!(names.contains(&".gitignore"));
        assert!(!names.contains(&"deep.py"));

        let alpha = nodes.iter().find(|node| node.name == "alpha").unwrap();
        assert!(alpha.is_dir);
        assert!(alpha.is_expanded);
        assert_eq!(alpha.depth, 1);

        let ignored_file = nodes
            .iter()
            .find(|node| node.name == "ignored.txt")
            .unwrap();
        assert!(!ignored_file.is_dir);
        assert!(ignored_file.is_ignored);

        let ignored_dir = nodes
            .iter()
            .find(|node| node.name == "ignored_dir")
            .unwrap();
        assert!(ignored_dir.is_dir);
        assert!(ignored_dir.is_ignored);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn spawn_scan_skips_missing_roots_applies_user_patterns_and_sends_final_tree() {
        let root = test_root("spawn_scan");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("keep_dir")).unwrap();
        std::fs::create_dir_all(root.join("skip_dir")).unwrap();
        std::fs::write(root.join("keep.rs"), "keep").unwrap();
        std::fs::write(root.join("skip.py"), "skip").unwrap();

        let mut expanded = FxHashSet::default();
        expanded.insert(root.clone());
        let rx = spawn_scan(
            vec![root.join("missing"), root.clone()],
            expanded,
            vec!["skip*".to_string()],
        );

        let first = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        let second = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();

        let names: Vec<_> = first.iter().map(|node| node.name.as_str()).collect();
        let second_names: Vec<_> = second.iter().map(|node| node.name.as_str()).collect();

        assert_eq!(names, second_names);
        assert!(names.contains(&"keep_dir"));
        assert!(names.contains(&"keep.rs"));
        assert!(!names.contains(&"missing"));
        assert!(!names.contains(&"skip_dir"));
        assert!(!names.contains(&"skip.py"));
        assert!(first.iter().all(|node| node.path.exists()));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn file_tree_new_child_validation_blocks_empty_paths_and_traversal() {
        assert!(validate_child_name("main.rs").is_ok());
        assert!(validate_child_name("").is_err());
        assert!(validate_child_name("../x").is_err());
        assert!(validate_child_name("a/b").is_err());
        assert!(validate_child_name("..").is_err());
    }

    #[test]
    fn file_tree_relative_path_uses_first_matching_workspace() {
        let root = PathBuf::from("/tmp/rriter_ws");
        let path = root.join("src/main.rs");

        assert_eq!(
            relative_path_for_workspace(&path, &[root]),
            PathBuf::from("src/main.rs")
        );
    }

    #[test]
    fn file_tree_move_dialog_message_names_source_and_target() {
        assert_eq!(
            file_tree_move_dialog_message(
                &[PathBuf::from("/tmp/ws/.env.test")],
                Path::new("/tmp/ws/tests"),
            ),
            "Переместить '.env.test' в 'tests'?"
        );
        assert_eq!(
            file_tree_move_dialog_message(
                &[PathBuf::from("/tmp/ws/a.py"), PathBuf::from("/tmp/ws/b.py")],
                Path::new("/tmp/ws/tests"),
            ),
            "Переместить 2 элементов в 'tests'?"
        );
    }

    #[test]
    fn file_tree_overlay_state_covers_menu_dialogs_and_overlay_ids() {
        let root = PathBuf::from("/tmp/ws");
        let mut panel = crate::app::IdePanelState::default();

        assert!(!file_tree_overlay_active_for_panel(&panel));

        panel.file_tree_context_menu = Some(FileTreeContextMenu {
            x: 1.0,
            y: 2.0,
            target_path: Some(root.join("main.rs")),
            target_is_dir: false,
            target_dir: Some(root.clone()),
            entries: vec![FileTreeMenuAction::Copy],
            opened_at: Instant::now(),
        });
        assert!(file_tree_overlay_active_for_panel(&panel));
        panel.file_tree_context_menu = None;

        panel.file_tree_create_dialog = Some(FileTreeCreateDialog {
            kind: FileTreeCreateKind::File,
            parent_dir: root.clone(),
            editor: Editor::new(64),
            error: None,
        });
        assert!(file_tree_overlay_active_for_panel(&panel));
        panel.file_tree_create_dialog = None;

        panel.file_tree_rename_dialog = Some(FileTreeRenameDialog {
            path: root.join("old.rs"),
            editor: Editor::new(64),
            error: None,
        });
        assert!(file_tree_overlay_active_for_panel(&panel));
        panel.file_tree_rename_dialog = None;

        panel.file_tree_move_dialog = Some(FileTreeMoveDialog {
            sources: vec![root.join("old.rs")],
            target_dir: root.join("src"),
            error: None,
        });
        assert!(file_tree_overlay_active_for_panel(&panel));

        assert!(crate::app::App::ui_id_is_file_tree_overlay(
            crate::ui_system::UiId::FileTreeRenameInput
        ));
        assert!(!crate::app::App::ui_id_is_file_tree_overlay(
            crate::ui_system::UiId::EditorTextBody
        ));
    }

    #[test]
    fn file_tree_context_menu_labels_and_anim_progress_are_stable() {
        assert_eq!(
            FileTreeMenuAction::CopyRelativePath.label(),
            "Скопировать относительный путь"
        );

        let start = Instant::now();
        assert_eq!(file_tree_context_menu_anim_progress(start, start), 0.0);
        assert_eq!(
            file_tree_context_menu_anim_progress(
                start,
                start + std::time::Duration::from_secs(1)
            ),
            1.0
        );
    }

    #[test]
    fn file_tree_copy_move_delete_paths_end_to_end() {
        let root = test_root("file_tree_ops");
        let _ = std::fs::remove_dir_all(&root);
        let src_dir = root.join("src");
        let target_dir = root.join("target_dir");
        std::fs::create_dir_all(src_dir.join("nested")).unwrap();
        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(src_dir.join("main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(src_dir.join("nested").join("mod.rs"), "mod x;\n").unwrap();

        let copied = copy_paths_to_dir(
            &[src_dir.join("main.rs"), src_dir.join("nested")],
            &target_dir,
        )
        .unwrap();
        assert_eq!(copied.len(), 2);
        assert!(target_dir.join("main.rs").exists());
        assert!(target_dir.join("nested").join("mod.rs").exists());

        let (_old, moved) = move_path_to_dir(&target_dir.join("main.rs"), &src_dir).unwrap();
        assert!(moved.exists());
        assert!(
            moved
                .file_name()
                .unwrap()
                .to_string_lossy()
                .contains("copy")
        );
        assert!(!target_dir.join("main.rs").exists());

        assert!(delete_paths(&[target_dir.join("nested")], &[root.clone()]).is_ok());
        assert!(!target_dir.join("nested").exists());
        assert!(delete_paths(&[root.clone()], &[root.clone()]).is_err());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn file_tree_rename_path_updates_file_and_rejects_workspace_root() {
        let root = test_root("file_tree_rename");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let old = root.join("old.env");
        std::fs::write(&old, "x=1\n").unwrap();

        let new = rename_path(&old, "new.env", &[root.clone()]).unwrap();
        assert_eq!(new, root.join("new.env"));
        assert!(!old.exists());
        assert!(new.exists());
        assert!(rename_path(&root, "renamed-root", &[root.clone()]).is_err());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn file_tree_path_after_rename_updates_nested_open_paths() {
        let old_root = PathBuf::from("/tmp/ws/package");
        let new_root = PathBuf::from("/tmp/ws/package2");
        assert_eq!(
            path_after_rename(&old_root.join("src/main.rs"), &old_root, &new_root),
            Some(new_root.join("src/main.rs"))
        );
        assert_eq!(
            path_after_rename(Path::new("/tmp/ws/other.rs"), &old_root, &new_root),
            None
        );
    }

    #[test]
    fn file_tree_selected_paths_preserve_visible_tree_order() {
        let root = PathBuf::from("/tmp/ws");
        let a = FileNode {
            path: root.join("a.rs"),
            name: "a.rs".to_string(),
            depth: 1,
            is_dir: false,
            is_expanded: false,
            icon_key: "default_file",
            is_ignored: false,
        };
        let b = FileNode {
            path: root.join("b.rs"),
            name: "b.rs".to_string(),
            depth: 1,
            is_dir: false,
            is_expanded: false,
            icon_key: "default_file",
            is_ignored: false,
        };
        let mut selection = FxHashSet::default();
        selection.insert(b.path.clone());
        selection.insert(a.path.clone());

        assert_eq!(
            selected_paths(&[a.clone(), b.clone()], &selection, &root),
            vec![a.path, b.path]
        );
        assert_eq!(
            selected_paths(&[], &FxHashSet::default(), &root),
            vec![root]
        );
    }
}
