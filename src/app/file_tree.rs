//! Логика проводника файлов: структуры данных, фоновый скан, методы App.

use crate::app::App;
use crate::editor::Editor;
use rustc_hash::FxHashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Instant;

#[path = "file_tree_ops.rs"]
mod file_tree_ops;
#[path = "file_tree_scan.rs"]
mod file_tree_scan;
pub(crate) use file_tree_ops::relative_path_for_workspace;
use file_tree_ops::*;
pub use file_tree_scan::*;

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
const FILE_TREE_UNDO_LIMIT: usize = 64;

pub(crate) fn file_tree_context_menu_anchor(mx: f32, my: f32, scale: f32) -> (f32, f32) {
    crate::app::context_menu::context_menu_anchor(mx, my, scale)
}

/// Проверяет, должен ли узел быть скрыт по паттернам.
/// Поддерживает:
///   - точные имена:   `node_modules`, `.DS_Store`
///   - glob-wildcards: `*.pyc`, `foo*`
pub fn matches_ignore_pattern(name: &str, patterns: &[&str]) -> bool {
    matches_ignore_pattern_values(name, patterns.iter().copied())
}

pub fn matches_ignore_pattern_strings(name: &str, patterns: &[String]) -> bool {
    matches_ignore_pattern_values(name, patterns.iter().map(String::as_str))
}

fn matches_ignore_pattern_values<'a>(
    name: &str,
    patterns: impl IntoIterator<Item = &'a str>,
) -> bool {
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
    ShowInExplorer,
    CopyAbsolutePath,
    CopyRelativePath,
    CopyTargetAbsolutePath,
    CopyTargetRelativePath,
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
            Self::ShowInExplorer => "Показать в проводнике",
            Self::CopyAbsolutePath | Self::CopyTargetAbsolutePath => "Скопировать абсолютный путь",
            Self::CopyRelativePath | Self::CopyTargetRelativePath => {
                "Скопировать относительный путь"
            }
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

pub(crate) fn file_tree_context_menu_cursor(
    hovered_overlay: Option<crate::ui_system::UiId>,
) -> winit::window::CursorIcon {
    crate::app::context_menu::context_menu_cursor(hovered_overlay)
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
    pub input_scroll_x: crate::scroll::ScrollState,
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

#[derive(Clone, Debug)]
pub struct FileTreeDeleteDialog {
    pub paths: Vec<PathBuf>,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct FileTreeTrashEntry {
    pub original_path: PathBuf,
    pub trash_path: PathBuf,
    pub info_path: PathBuf,
}

#[derive(Clone, Debug)]
pub enum FileTreeUndoAction {
    Created {
        paths: Vec<PathBuf>,
    },
    Copied {
        paths: Vec<PathBuf>,
    },
    Moved {
        pairs: Vec<(PathBuf, PathBuf)>,
    },
    Renamed {
        old_path: PathBuf,
        new_path: PathBuf,
    },
    Trashed {
        entries: Vec<FileTreeTrashEntry>,
    },
}

#[derive(Clone, Debug)]
pub struct FileTreeUndoEntry {
    pub action: FileTreeUndoAction,
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

pub fn file_tree_delete_dialog_message(paths: &[PathBuf]) -> String {
    if paths.len() == 1 {
        let name = paths[0]
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_else(|| paths[0].to_str().unwrap_or("1 элемент"));
        format!("Переместить '{name}' в корзину?")
    } else {
        format!("Переместить {} элементов в корзину?", paths.len())
    }
}

pub fn file_tree_modal_overlay_active_for_panel(ide_panel: &crate::app::IdePanelState) -> bool {
    ide_panel.file_tree_create_dialog.is_some()
        || ide_panel.file_tree_rename_dialog.is_some()
        || ide_panel.file_tree_move_dialog.is_some()
        || ide_panel.file_tree_delete_dialog.is_some()
        || ide_panel.git.confirm_dialog.is_some()
        || ide_panel.api.spec_remove_dialog.is_some()
        || ide_panel.api.mock_route_reset_dialog.is_some()
        || ide_panel.api.mock_contract_field_delete_dialog.is_some()
}

pub fn file_tree_overlay_active_for_panel(ide_panel: &crate::app::IdePanelState) -> bool {
    ide_panel.file_tree_context_menu.is_some()
        || file_tree_modal_overlay_active_for_panel(ide_panel)
}

const FILE_TREE_NAME_INPUT_MAX_BYTES: usize = 255;
pub(crate) const FILE_TREE_DIALOG_INPUT_TEXT_SCALE: f32 = 0.92;
pub(crate) const FILE_TREE_DIALOG_W: f32 = 460.0;
pub(crate) const FILE_TREE_DIALOG_SIDE_PAD: f32 = 28.0;
pub(crate) const FILE_TREE_PATH_INPUT_MIN_W: f32 = 150.0;

pub(crate) fn file_tree_parent_path_prefix(parent_dir: &Path) -> String {
    let mut text = parent_dir.to_string_lossy().into_owned();
    if !text.ends_with(std::path::MAIN_SEPARATOR) {
        text.push(std::path::MAIN_SEPARATOR);
    }
    text
}

pub(crate) fn file_tree_clipped_path_suffix<F>(text: &str, max_w: f32, mut measure: F) -> String
where
    F: FnMut(&str) -> f32,
{
    if measure(text) <= max_w {
        return text.to_string();
    }
    let ellipsis = "...";
    let ellipsis_w = measure(ellipsis);
    if ellipsis_w >= max_w {
        return ellipsis.to_string();
    }

    let mut suffix_start = text.len();
    for (idx, _) in text.char_indices().rev() {
        if ellipsis_w + measure(&text[idx..]) > max_w {
            break;
        }
        suffix_start = idx;
    }
    format!("{ellipsis}{}", &text[suffix_start..])
}

pub(crate) fn file_tree_path_input_layout<F>(
    dialog_x: f32,
    dialog_w: f32,
    scale: f32,
    parent_dir: &Path,
    mut measure: F,
) -> (String, f32, f32)
where
    F: FnMut(&str) -> f32,
{
    file_tree_path_input_layout_with_base(
        dialog_x,
        dialog_w,
        dialog_w,
        scale,
        parent_dir,
        &mut measure,
    )
}

pub(crate) fn file_tree_rename_dialog_width(
    base_w: f32,
    max_w: f32,
    base_input_w: f32,
    text_w: f32,
    scale: f32,
) -> f32 {
    let wanted_input_w = text_w + 16.0 * scale;
    let extra_w = (wanted_input_w - base_input_w).max(0.0);
    (base_w + extra_w).min(max_w).max(base_w.min(max_w))
}

pub(crate) fn file_tree_rename_path_input_layout<F>(
    dialog_x: f32,
    dialog_w: f32,
    base_dialog_w: f32,
    scale: f32,
    parent_dir: &Path,
    mut measure: F,
) -> (String, f32, f32)
where
    F: FnMut(&str) -> f32,
{
    file_tree_path_input_layout_with_base(
        dialog_x,
        dialog_w,
        base_dialog_w,
        scale,
        parent_dir,
        &mut measure,
    )
}

fn file_tree_path_input_layout_with_base<F>(
    dialog_x: f32,
    dialog_w: f32,
    prefix_dialog_w: f32,
    scale: f32,
    parent_dir: &Path,
    mut measure: F,
) -> (String, f32, f32)
where
    F: FnMut(&str) -> f32,
{
    let side_pad = FILE_TREE_DIALOG_SIDE_PAD * scale;
    let content_x = dialog_x + side_pad;
    let content_w = (dialog_w - side_pad * 2.0).max(0.0);
    let gap = 6.0 * scale;
    let base_content_w = (prefix_dialog_w - side_pad * 2.0).max(0.0);
    let min_input_w = (FILE_TREE_PATH_INPUT_MIN_W * scale)
        .min(base_content_w * 0.58)
        .max(0.0);
    let max_prefix_w = (base_content_w - min_input_w - gap).max(0.0);
    let prefix = file_tree_clipped_path_suffix(
        &file_tree_parent_path_prefix(parent_dir),
        max_prefix_w,
        &mut measure,
    );
    let prefix_w = measure(&prefix).min(max_prefix_w);
    let input_x = content_x + prefix_w + gap;
    let input_w = (content_x + content_w - input_x)
        .max(0.0)
        .max(min_input_w.min(content_w));
    (prefix, input_x, input_w)
}

pub(crate) fn file_tree_name_input_scroll_x<F>(
    text: &str,
    cursor: usize,
    visible_width: f32,
    char_advance: F,
) -> f32
where
    F: FnMut(char) -> f32,
{
    crate::app::single_line_input::single_line_cursor_geometry(
        text,
        cursor,
        visible_width,
        0.0,
        0.0,
        0.0,
        char_advance,
    )
    .scroll_x
}

pub(crate) fn file_tree_name_input_hit_index<F>(text: &str, x_offset: f32, char_advance: F) -> usize
where
    F: FnMut(char) -> f32,
{
    crate::app::single_line_input::single_line_hit_index(text, x_offset, char_advance)
}

pub(crate) fn file_tree_row_index_at(
    panel_y: f32,
    mouse_y: f32,
    row_h: f32,
    current_scroll: f32,
    total_nodes: usize,
) -> Option<usize> {
    if !panel_y.is_finite()
        || !mouse_y.is_finite()
        || !row_h.is_finite()
        || row_h <= 0.0
        || !current_scroll.is_finite()
    {
        return None;
    }
    let content_y = mouse_y - panel_y + current_scroll.round();
    if content_y < 0.0 {
        return None;
    }
    let idx = (content_y / row_h).floor() as usize;
    (idx < total_nodes).then_some(idx)
}

/// Explorer scrollbar shared by the renderer and the press/drag handlers: 12 px lane at the
/// panel's right edge, inset 4 px top and bottom; the viewport is the whole panel.
pub(crate) fn file_tree_scrollbar(
    panel_x: f32,
    panel_y: f32,
    panel_w: f32,
    panel_h: f32,
    scale: f32,
    total_nodes: usize,
    current_scroll: f32,
) -> Option<crate::render_view::scrollbar_widget::Scrollbar> {
    use crate::render_view::scrollbar_widget::{
        Scrollbar, ScrollbarAxis, ScrollbarExtent, ScrollbarStyle,
    };
    if !panel_x.is_finite()
        || !panel_y.is_finite()
        || !panel_w.is_finite()
        || !panel_h.is_finite()
        || !scale.is_finite()
        || scale <= 0.0
        || panel_w <= 0.0
        || panel_h <= 0.0
    {
        return None;
    }
    let content_h = total_nodes as f32 * crate::render_view::tree_ui::TREE_ROW_H * scale;
    Some(Scrollbar {
        style: ScrollbarStyle::FILE_TREE,
        axis: ScrollbarAxis::Vertical,
        lane: (
            panel_x + panel_w - 12.0 * scale,
            panel_y + 4.0 * scale,
            12.0 * scale,
            (panel_h - 8.0 * scale).max(0.0),
        ),
        extent: ScrollbarExtent::new(panel_h, content_h, current_scroll),
    })
}

#[cfg(test)]
fn insert_file_tree_name_text(editor: &mut Editor, text: &str) {
    crate::app::single_line_input::insert_single_line_text(
        editor,
        text,
        FILE_TREE_NAME_INPUT_MAX_BYTES,
    );
}

fn handle_file_tree_name_editor_input(
    editor: &mut Editor,
    physical_key: winit::keyboard::PhysicalKey,
    logical_text: Option<&str>,
    primary: bool,
    word: bool,
    shift: bool,
    text_input_allowed: bool,
    paste_text: Option<String>,
) -> Option<String> {
    crate::app::single_line_input::handle_single_line_input(
        editor,
        physical_key,
        logical_text,
        primary,
        word,
        shift,
        text_input_allowed,
        paste_text.as_deref(),
        FILE_TREE_NAME_INPUT_MAX_BYTES,
    )
}

// ---------------------------------------------------------------------------
// Вспомогательная функция: читает прямых детей директории через `ignore`
// (уважает .gitignore, пропускает скрытые файлы).
// Возвращает (папки, файлы), обе группы отсортированы натурально.
// ---------------------------------------------------------------------------

pub(crate) fn clear_file_tree_for_empty_roots(
    panel: &mut crate::app::IdePanelState,
    receiver: &mut Option<std::sync::mpsc::Receiver<FileTreeScanMessage>>,
) {
    panel.file_tree_nodes.clear();
    panel.file_tree_error = None;
    *receiver = None;
}

pub(crate) fn apply_file_tree_scan_error(
    panel: &mut crate::app::IdePanelState,
    message: impl Into<String>,
) {
    panel.file_tree_error = Some(message.into());
}

impl App {
    fn push_file_tree_undo(&mut self, action: FileTreeUndoAction) {
        self.ide_panel
            .file_tree_undo_stack
            .push(FileTreeUndoEntry { action });
        if self.ide_panel.file_tree_undo_stack.len() > FILE_TREE_UNDO_LIMIT {
            self.ide_panel.file_tree_undo_stack.remove(0);
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn undo_file_tree_operation(&mut self) -> Result<(), String> {
        if self.headless_write_blocked() { return Ok(()); }
        let Some(entry) = self.ide_panel.file_tree_undo_stack.pop() else {
            return Ok(());
        };
        let mut selection = Vec::new();
        match entry.action {
            FileTreeUndoAction::Created { paths } | FileTreeUndoAction::Copied { paths } => {
                let trashed = trash_paths(&paths, &self.ide_workspaces)?;
                selection.extend(trashed.into_iter().map(|entry| entry.original_path));
                self.close_or_mark_tabs_after_tree_delete(&selection);
            }
            FileTreeUndoAction::Moved { pairs } => {
                if let Err(error) = undo_moved_pairs(&pairs) {
                    if error.retryable {
                        self.push_file_tree_undo(FileTreeUndoAction::Moved { pairs });
                    }
                    return Err(error.message);
                }
                for (old_path, new_path) in &pairs {
                    self.update_open_paths_after_file_tree_rename(new_path, old_path);
                    selection.push(old_path.clone());
                }
            }
            FileTreeUndoAction::Renamed { old_path, new_path } => {
                move_path_exact(&new_path, &old_path)?;
                self.update_open_paths_after_file_tree_rename(&new_path, &old_path);
                selection.push(old_path);
            }
            FileTreeUndoAction::Trashed { entries } => {
                // Restored paths come back in `entries` order.
                selection = restore_trash_entries(&entries)?;
                for (entry, restored) in entries.iter().zip(&selection) {
                    self.rebind_deleted_tabs_after_restore(&entry.original_path, restored);
                }
            }
        }
        self.ide_panel.file_tree_selection.clear();
        self.ide_panel.file_tree_selection.extend(selection);
        self.refresh_file_tree();
        Ok(())
    }

    /// Запускает фоновый скан дерева. Вызывать при открытии Explorer,
    /// добавлении workspace или разворачивании папки.
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn refresh_file_tree(&mut self) {
        let roots = self.ide_workspaces.clone();
        if roots.is_empty() {
            clear_file_tree_for_empty_roots(&mut self.ide_panel, &mut self.file_tree_rx);
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
            if !existing_roots
                .iter()
                .any(|existing| crate::platform::paths_equal(existing, root))
            {
                self.ide_panel.file_tree_expanded.insert(root.clone());
            }
        }
        let expanded = self.ide_panel.file_tree_expanded.clone();
        let patterns = self.ide_ignore_patterns.clone();
        let is_dark = self.renderer.as_ref().is_some_and(|renderer| renderer.ui.is_dark);
        let known_icons = self.renderer.as_ref().map_or_else(FxHashSet::default, |renderer| {
            renderer
                .file_icon_cache
                .keys()
                .chain(renderer.rasterized_file_icons.keys())
                .copied()
                .filter(|(_, variant)| *variant == is_dark)
                .collect()
        });
        self.ide_panel.file_tree_error = None;
        self.file_tree_rx = Some(spawn_scan(
            roots,
            expanded,
            patterns,
            known_icons,
            is_dark,
            &self.ui_waker,
        ));
    }

    /// Поллит канал результатов фонового скана.
    /// Возвращает true если пришли новые данные (нужен redraw).
    /// Вызывать из about_to_wait.
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn poll_file_tree(&mut self) -> bool {
        let mut updated = false;
        let mut finished = false;
        let mut reveal_path = None;
        if let Some(rx) = &self.file_tree_rx {
            loop {
                match rx.try_recv() {
                    Ok(message) => {
                        let terminal = message.is_terminal();
                        match message {
                            crate::app::file_tree::FileTreeScanMessage::Nodes(nodes) => {
                                let selected_path = if self.ide_panel.file_tree_selection.len() == 1
                                {
                                    self.ide_panel.file_tree_selection.iter().next().cloned()
                                } else {
                                    None
                                };
                                let selected_was_visible =
                                    selected_path.as_ref().is_some_and(|path| {
                                        self.ide_panel.file_tree_nodes.iter().any(|node| {
                                            crate::platform::paths_equal(&node.path, path)
                                        })
                                    });
                                self.ide_panel.file_tree_nodes = nodes;
                                self.ide_panel
                                    .file_tree_selection
                                    .retain(|path| path.exists());
                                if !selected_was_visible
                                    && selected_path.as_ref().is_some_and(|path| {
                                        self.ide_panel.file_tree_nodes.iter().any(|node| {
                                            crate::platform::paths_equal(&node.path, path)
                                        })
                                    })
                                {
                                    reveal_path = selected_path;
                                }
                                updated = true;
                            }
                            crate::app::file_tree::FileTreeScanMessage::Icon(key, is_dark, state) => {
                                if let Some(renderer) = self.renderer.as_mut()
                                    && renderer.ui.is_dark == is_dark
                                    && !renderer.file_icon_cache.contains_key(&(key, is_dark))
                                {
                                    renderer.rasterized_file_icons.insert((key, is_dark), state);
                                }
                                updated = true;
                            }
                            crate::app::file_tree::FileTreeScanMessage::IconsReady => {
                                updated = true;
                            }
                            crate::app::file_tree::FileTreeScanMessage::Failed(error) => {
                                apply_file_tree_scan_error(&mut self.ide_panel, error);
                                updated = true;
                            }
                        }
                        if terminal {
                            finished = true;
                            break;
                        }
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        apply_file_tree_scan_error(
                            &mut self.ide_panel,
                            "Фоновое сканирование дерева файлов неожиданно завершилось",
                        );
                        finished = true;
                        updated = true;
                        break;
                    }
                }
            }
        }
        if finished {
            self.file_tree_rx = None;
        }
        if let Some(path) = reveal_path {
            self.center_file_tree_path(&path);
        }
        updated
    }

    fn center_file_tree_path(&mut self, path: &Path) -> bool {
        let Some(node_idx) = self
            .ide_panel
            .file_tree_nodes
            .iter()
            .position(|node| crate::platform::paths_equal(&node.path, path))
        else {
            return false;
        };
        let scale = self
            .renderer
            .as_ref()
            .map(|renderer| renderer.scale_factor)
            .unwrap_or(1.0);
        let viewport_h = self
            .renderer
            .as_ref()
            .map(|renderer| renderer.height)
            .unwrap_or(self.window_height as f32)
            - 32.0 * scale;
        let row_h = crate::render_view::tree_ui::TREE_ROW_H * scale;
        let total_h = self.ide_panel.file_tree_nodes.len() as f32 * row_h;
        let max_scroll = (total_h - viewport_h.max(row_h)).max(0.0);
        let row_center = node_idx as f32 * row_h + row_h * 0.5;
        let target = (row_center - viewport_h * 0.5)
            .clamp(0.0, max_scroll)
            .round();
        self.ide_panel.explorer_scroll.jump_to(target);
        true
    }

    fn file_tree_open_file_parent_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        if let Some(parent) = self.file_path.as_ref().and_then(|path| path.parent()) {
            dirs.push(parent.to_path_buf());
        }
        for tab in &self.tabs {
            if !matches!(&tab.kind, crate::app::EditorTabKind::Normal) {
                continue;
            }
            if let Some(parent) = tab.file_path.as_ref().and_then(|path| path.parent()) {
                dirs.push(parent.to_path_buf());
            }
        }
        dirs
    }

    fn stop_file_watcher(&mut self) {
        if let Some(stop_tx) = self.file_tree_watcher_stop_tx.take() {
            let _ = stop_tx.send(());
        }
        self.file_tree_notify_rx = None;
        self.file_tree_watched_dirs.clear();
    }

    /// Запускает (или перезапускает) lazy watcher для текущих visible/open dirs.
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn start_file_watcher(&mut self) {
        if self.ide_workspaces.is_empty() {
            self.stop_file_watcher();
            return;
        }
        let open_dirs = self.file_tree_open_file_parent_dirs();
        let paths = crate::app::file_tree::build_file_tree_watch_paths(
            &self.ide_workspaces,
            &self.ide_panel.file_tree_expanded,
            &open_dirs,
        );
        if path_lists_equal(&paths, &self.file_tree_watched_dirs)
            && self.file_tree_notify_rx.is_some()
        {
            return;
        }
        if let Some(stop_tx) = self.file_tree_watcher_stop_tx.take() {
            let _ = stop_tx.send(());
        }
        if paths.is_empty() {
            self.file_tree_notify_rx = None;
            self.file_tree_watched_dirs.clear();
            return;
        }
        let (tx, rx) = self.ui_waker.channel();
        let (stop_tx, stop_rx) = mpsc::channel();
        self.file_tree_notify_rx = Some(rx);
        self.file_tree_watcher_stop_tx = Some(stop_tx);
        self.file_tree_watched_dirs = paths.clone();
        if !crate::app::file_tree::spawn_watcher(paths, tx, stop_rx) {
            self.file_tree_notify_rx = None;
            self.file_tree_watcher_stop_tx = None;
            self.file_tree_watched_dirs.clear();
            self.ide_panel.file_tree_error =
                Some("Не удалось запустить наблюдение за файлами".to_string());
        }
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
            path_set_remove(&mut self.ide_panel.file_tree_expanded, &node.path);
        } else {
            self.ide_panel.file_tree_expanded.insert(node.path.clone());
        }
        self.refresh_file_tree();
        self.start_file_watcher();
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
    pub fn handle_file_tree_left_click(
        &mut self,
        node_idx: usize,
        arrow: bool,
        same_click_target: bool,
    ) {
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

        let ctrl = crate::platform::primary_shortcut_modifier(self.modifiers);
        if ctrl {
            if !path_set_remove(&mut self.ide_panel.file_tree_selection, &node.path) {
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
        let double_click = crate::app::ui_handlers::repeated_ui_click(
            same_click_target,
            now.duration_since(self.last_click_time),
            mx - self.last_click_pos.0,
            my - self.last_click_pos.1,
        );
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
        self.ide_panel.database.context_menu = None;
        let scale = self
            .renderer
            .as_ref()
            .map(|renderer| renderer.scale_factor)
            .unwrap_or(1.0);
        let (menu_x, menu_y) = file_tree_context_menu_anchor(mx, my, scale);
        let target_idx = self.file_tree_node_at(mx, my);
        let target = target_idx.and_then(|idx| self.ide_panel.file_tree_nodes.get(idx).cloned());
        if let Some(node) = &target {
            if !selection_contains_path(&self.ide_panel.file_tree_selection, &node.path) {
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
            x: menu_x,
            y: menu_y,
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

}

include!("file_tree_app_actions.rs");

impl App {
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn open_contained_folder(&mut self, path: &Path, is_dir: bool) {
        let folder = if is_dir {
            path.to_path_buf()
        } else {
            path.parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| path.to_path_buf())
        };
        let _ = crate::platform::reveal_path(self.external_requests.sink(), &folder);
    }

    pub(crate) fn show_path_in_file_tree(&mut self, path: &Path) {
        let path = self.abs_path_for_workspace(path);
        self.ide_panel.open(crate::app::PanelId::Explorer);
        self.ide_panel.file_tree_focused = true;

        let Some(workspace) = self
            .ide_workspaces
            .iter()
            .find(|workspace| crate::platform::path_is_within(&path, workspace))
            .cloned()
        else {
            return;
        };

        self.ide_panel.file_tree_selection.clear();
        self.ide_panel.file_tree_selection.insert(path.clone());
        let mut expansion_changed = false;
        let mut parent = path.parent();
        while let Some(dir) = parent {
            if !crate::platform::path_is_within(dir, &workspace) {
                break;
            }
            expansion_changed |= self.ide_panel.file_tree_expanded.insert(dir.to_path_buf());
            if crate::platform::paths_equal(dir, &workspace) {
                break;
            }
            parent = dir.parent();
        }

        let target_visible = self.center_file_tree_path(&path);
        if expansion_changed || !target_visible {
            self.refresh_file_tree();
        }
        self.start_file_watcher();
    }

    /// Возвращает индекс узла дерева под экранными координатами (mx, my),
    /// или None если курсор не над областью дерева файлов.
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn file_tree_node_at(&self, mx: f32, my: f32) -> Option<usize> {
        if self.show_settings || self.modal_dialog_open() {
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
        let (panel_x, panel_y, panel_w, panel_h, _) =
            crate::app::mouse::app_panel_scroll_rect(self, crate::app::PanelId::Explorer, s);
        if !crate::ui_system::point_in_rect(mx, my, (panel_x, panel_y, panel_w, panel_h)) {
            return None;
        }
        let row_h = crate::render_view::tree_ui::TREE_ROW_H * s;
        file_tree_row_index_at(
            panel_y,
            my,
            row_h,
            self.ide_panel.explorer_scroll.current,
            self.ide_panel.file_tree_nodes.len(),
        )
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn file_tree_panel_contains(&self, mx: f32, my: f32) -> bool {
        if self.show_settings || self.modal_dialog_open() || !self.is_ide_mode {
            return false;
        }
        if !self.ide_panel.is_open(crate::app::PanelId::Explorer) {
            return false;
        }
        let Some(r) = self.renderer.as_ref() else {
            return false;
        };
        let s = r.scale_factor;
        let (panel_x, panel_y, panel_w, panel_h, _) =
            crate::app::mouse::app_panel_scroll_rect(self, crate::app::PanelId::Explorer, s);
        crate::ui_system::point_in_rect(mx, my, (panel_x, panel_y, panel_w, panel_h))
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

    pub fn file_tree_modal_overlay_active(&self) -> bool {
        file_tree_modal_overlay_active_for_panel(&self.ide_panel)
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
                | crate::ui_system::UiId::FileTreeDeleteConfirm
                | crate::ui_system::UiId::FileTreeDeleteCancel
                | crate::ui_system::UiId::GitConfirmAction
                | crate::ui_system::UiId::GitConfirmCancel
                | crate::ui_system::UiId::ApiSpecRemoveConfirm
                | crate::ui_system::UiId::ApiSpecRemoveCancel
                | crate::ui_system::UiId::ApiMockRouteResetConfirm
                | crate::ui_system::UiId::ApiMockRouteResetCancel
                | crate::ui_system::UiId::ApiMockContractFieldRemoveConfirm
                | crate::ui_system::UiId::ApiMockContractFieldRemoveCancel
        )
    }
}

#[path = "file_tree_dialog.rs"]
mod file_tree_dialog;

#[cfg(test)]
#[path = "file_tree_tests.rs"]
mod file_tree_tests;
