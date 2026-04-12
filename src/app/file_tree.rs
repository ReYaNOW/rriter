//! Логика проводника файлов: структуры данных, фоновый скан, методы App.

use crate::app::App;
use rustc_hash::FxHashSet;
use std::path::PathBuf;
use std::sync::mpsc;

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

// ---------------------------------------------------------------------------
// Вспомогательная функция: читает прямых детей директории через `ignore`
// (уважает .gitignore, пропускает скрытые файлы).
// Возвращает (папки, файлы), обе группы отсортированы натурально.
// ---------------------------------------------------------------------------

use rayon::prelude::*;

pub static RASTERIZED_ICONS: once_cell::sync::Lazy<
    std::sync::Mutex<rustc_hash::FxHashMap<&'static str, Vec<u8>>>,
> = once_cell::sync::Lazy::new(|| std::sync::Mutex::new(rustc_hash::FxHashMap::default()));

pub fn pre_rasterize_icon(key: &'static str, is_folder: bool) {
    let cache = RASTERIZED_ICONS.lock().unwrap();
    if cache.contains_key(key) {
        return;
    }
    drop(cache); // Не блокируем другие потоки во время рендеринга

    let svg_bytes = crate::app::file_icons::svg_for_key(key, is_folder);
    if svg_bytes.is_empty() {
        return;
    }
    let opt = resvg::usvg::Options::default();
    let svg_str = String::from_utf8_lossy(svg_bytes).replace("currentColor", "#ffffff");

    if let Ok(tree) = resvg::usvg::Tree::from_data(svg_str.as_bytes(), &opt) {
        let target = 128u32;
        if let Some(mut pixmap) = tiny_skia::Pixmap::new(target, target) {
            let sz = tree.size();
            let scale = (target as f32) / sz.width().max(sz.height());
            let dx = (target as f32 - sz.width() * scale) / 2.0;
            let dy = (target as f32 - sz.height() * scale) / 2.0;
            let transform = tiny_skia::Transform::from_row(scale, 0.0, 0.0, scale, dx, dy);
            resvg::render(&tree, transform, &mut pixmap.as_mut());

            let mut data = pixmap.data().to_vec();
            for px in data.chunks_exact_mut(4) {
                let a = px[3] as u32;
                if a > 0 && a < 255 {
                    px[0] = ((px[0] as u32 * 255) / a).min(255) as u8;
                    px[1] = ((px[1] as u32 * 255) / a).min(255) as u8;
                    px[2] = ((px[2] as u32 * 255) / a).min(255) as u8;
                }
            }
            RASTERIZED_ICONS.lock().unwrap().insert(key, data);
        }
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
) -> Vec<FileNode> {
    let is_expanded = is_root || expanded.contains(&path);
    let icon_key = crate::app::file_icons::folder_icon_key(&name.to_ascii_lowercase());

    let is_ignored = if is_root {
        false
    } else {
        gitignore
            .matched_path_or_any_parents(&path, true)
            .is_ignore()
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
                .is_ignore();
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

                scan_dir_parallel(root.clone(), name, 0, &expanded, true, 10, &gitignore)
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

// ---------------------------------------------------------------------------
// Методы App
// ---------------------------------------------------------------------------

impl App {
    /// Запускает фоновый скан дерева. Вызывать при открытии Explorer,
    /// добавлении workspace или разворачивании папки.
    pub fn refresh_file_tree(&mut self) {
        let roots = self.ide_workspaces.clone();
        if roots.is_empty() {
            self.ide_panel.file_tree_nodes.clear();
            return;
        }
        let expanded = self.ide_panel.file_tree_expanded.clone();
        self.file_tree_rx = Some(spawn_scan(roots, expanded));
    }

    /// Поллит канал результатов фонового скана.
    /// Возвращает true если пришли новые данные (нужен redraw).
    /// Вызывать из about_to_wait.
    pub fn poll_file_tree(&mut self) -> bool {
        let mut updated = false;
        if let Some(rx) = &self.file_tree_rx {
            while let Ok(nodes) = rx.try_recv() {
                self.ide_panel.file_tree_nodes = nodes;
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

    /// Обрабатывает клик по узлу с индексом node_idx.
    /// Папки — разворачивает/сворачивает, файлы — открывает.
    pub fn handle_file_tree_click(&mut self, node_idx: usize) {
        let node = match self.ide_panel.file_tree_nodes.get(node_idx) {
            Some(n) => n.clone(),
            None => return,
        };
        if node.is_dir {
            if node.is_expanded {
                self.ide_panel.file_tree_expanded.remove(&node.path);
            } else {
                self.ide_panel.file_tree_expanded.insert(node.path.clone());
            }
            self.refresh_file_tree();
        } else {
            self.load_file(node.path.clone(), false);
        }
    }

    /// Возвращает индекс узла дерева под экранными координатами (mx, my),
    /// или None если курсор не над областью дерева файлов.
    pub fn file_tree_node_at(&self, mx: f32, my: f32) -> Option<usize> {
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
}
