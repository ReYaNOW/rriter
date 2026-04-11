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
}

// ---------------------------------------------------------------------------
// Вспомогательная функция: читает прямых детей директории через `ignore`
// (уважает .gitignore, пропускает скрытые файлы).
// Возвращает (папки, файлы), обе группы отсортированы натурально.
// ---------------------------------------------------------------------------

fn read_children(dir: &PathBuf) -> (Vec<(String, PathBuf)>, Vec<(String, PathBuf)>) {
    let mut dirs: Vec<(String, PathBuf)> = Vec::new();
    let mut files: Vec<(String, PathBuf)> = Vec::new();

    let walker = ignore::WalkBuilder::new(dir)
        .max_depth(Some(1))
        .hidden(true) // фильтровать скрытые (начинающиеся с '.')
        .git_ignore(true) // уважать .gitignore
        .build();

    for entry in walker.skip(1).flatten() {
        let path = entry.path().to_path_buf();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            dirs.push((name, path));
        } else {
            files.push((name, path));
        }
    }

    // Натуральная сортировка: file2 < file10
    dirs.sort_by(|a, b| lexical_sort::natural_lexical_cmp(&a.0, &b.0));
    files.sort_by(|a, b| lexical_sort::natural_lexical_cmp(&a.0, &b.0));

    (dirs, files)
}

// ---------------------------------------------------------------------------
// Фоновый скан
// ---------------------------------------------------------------------------

fn scan_recursive(
    root: &PathBuf,
    depth: usize,
    expanded: &FxHashSet<PathBuf>,
    result: &mut Vec<FileNode>,
    max_depth: usize,
) {
    if depth > max_depth {
        return;
    }

    let (dirs, files) = read_children(root);

    for (name, path) in dirs {
        let is_expanded = expanded.contains(&path);
        result.push(FileNode {
            path: path.clone(),
            name,
            depth,
            is_dir: true,
            is_expanded,
        });
        if is_expanded {
            scan_recursive(&path, depth + 1, expanded, result, max_depth);
        }
    }
    for (name, path) in files {
        result.push(FileNode {
            path,
            name,
            depth,
            is_dir: false,
            is_expanded: false,
        });
    }
}

/// Запускает фоновый поток сканирования. Возвращает канал для результата.
pub fn spawn_scan(
    roots: Vec<PathBuf>,
    expanded: FxHashSet<PathBuf>,
) -> mpsc::Receiver<Vec<FileNode>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut nodes: Vec<FileNode> = Vec::new();
        for root in &roots {
            if !root.exists() {
                continue;
            }
            let name = root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| root.to_string_lossy().into_owned());
            let is_expanded = expanded.contains(root);
            nodes.push(FileNode {
                path: root.clone(),
                name,
                depth: 0,
                is_dir: true,
                is_expanded,
            });
            if is_expanded {
                scan_recursive(root, 1, &expanded, &mut nodes, 10);
            }
        }
        let _ = tx.send(nodes);
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
        if let Some(rx) = &self.file_tree_rx {
            if let Ok(nodes) = rx.try_recv() {
                self.file_tree_rx = None;
                self.ide_panel.file_tree_nodes = nodes;
                return true;
            }
        }
        false
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
