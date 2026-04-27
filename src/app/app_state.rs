use crate::editor::Editor;
use crate::highlighter::{CompletionItem, Highlighter};
use crate::renderer::{Renderer, Theme};
use arboard::Clipboard;
use glutin::context::PossiblyCurrentContext;
use glutin::surface::{Surface, WindowSurface};
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::PathBuf;
use std::time::Instant;
use winit::keyboard::ModifiersState;
use winit::window::Window;

pub struct EditorTab {
    pub editor: crate::editor::Editor,
    pub file_path: Option<PathBuf>,
    pub base_title: String,
    pub file_extension: String,
    pub scroll_y: crate::scroll::ScrollState,
    pub scroll_x: crate::scroll::ScrollState,
    pub spans: Vec<crate::highlighter::ColorSpan>,
    pub completions: Vec<crate::highlighter::CompletionItem>,
    pub foldable_ranges: Vec<(usize, usize, bool, bool)>,
    pub syntax_errors: Vec<(usize, usize)>,
    pub last_sent_version: u64,
    pub search_results: Vec<(usize, usize)>,
    pub search_current_idx: Option<usize>,
    pub is_highlighted_once: bool,
    pub icon_key: &'static str,
}

#[derive(Clone, Copy, PartialEq)]
pub enum PendingAction {
    Quit,
    OpenFile,
    CloseFile,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum PanelId {
    Explorer,
    Terminal,
    Problems,
    LspServers,
}

impl PanelId {
    pub fn label(self) -> &'static str {
        match self {
            PanelId::Explorer => "Проводник",
            PanelId::Terminal => "Терминал",
            PanelId::Problems => "Ляпы",
            PanelId::LspServers => "Языковые серверы",
        }
    }
    pub fn icon(self) -> crate::widgets::IconType {
        match self {
            PanelId::Explorer => crate::widgets::IconType::Explorer,
            PanelId::Terminal => crate::widgets::IconType::Terminal,
            PanelId::Problems => crate::widgets::IconType::Problems,
            PanelId::LspServers => crate::widgets::IconType::LspServers,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PanelGroup {
    Top,
    Bottom,
}

#[derive(Clone, Debug)]
pub struct PanelSlot {
    pub id: PanelId,
    pub group: PanelGroup,
    pub open: bool,
}

#[derive(Clone, Debug)]
pub struct PanelDragState {
    pub panel_id: PanelId,
    pub start_y: f32,
    pub current_y: f32,
    pub threshold_passed: bool,
}

#[derive(Clone, Debug)]
pub struct TabDragState {
    pub start_idx: usize,
    pub start_x: f32,
    pub current_x: f32,
    pub threshold_passed: bool,
}

#[derive(Debug, Clone)]
pub enum LspActionItem {
    /// Авто-правка от ruff (workspace edit)
    CodeAction(crate::lsp::CodeAction),
    /// Добавить # noqa: CODES к строке
    AddNoqa {
        codes: Vec<String>,
    },
    /// Добавить # noqa (отключить все для строки)
    AddNoqaAll,
    FixAll,
    OrganizeImports,
}

/// Состояние всплывающего меню Alt+Enter
#[derive(Debug, Clone)]
pub struct LspActionsMenu {
    /// Физическая строка курсора (0-based)
    pub cursor_line: u32,
    pub items: Vec<LspActionItem>,
    pub selected: usize,
    /// Позиция меню на экране
    pub menu_x: f32,
    pub menu_y: f32,
    /// ID запроса code actions (ждём ответа)
    pub pending_request_id: Option<i32>,
}

pub struct IdePanelState {
    pub slots: Vec<PanelSlot>,
    pub left_width: f32,
    pub bottom_height: f32,
    pub drag: Option<PanelDragState>,
    pub is_resizing_left: bool,
    pub is_resizing_bottom: bool,
    pub file_tree_nodes: Vec<crate::app::file_tree::FileNode>,
    pub file_tree_expanded: FxHashSet<std::path::PathBuf>,
    pub explorer_scroll: crate::scroll::ScrollState,
    pub file_tree_hovered_idx: Option<usize>,
    /// Актуальная инфа о LSP серверах для рендера панели
    pub lsp_servers: Vec<crate::lsp::LspServerInfo>,
    pub lsp_logs_expanded: FxHashSet<String>,
    pub lsp_scroll_y: crate::scroll::ScrollState,
    pub lsp_scroll_x: crate::scroll::ScrollState,
    pub lsp_log_editors: FxHashMap<String, Editor>,
    pub lsp_logs_scroll_y: FxHashMap<String, crate::scroll::ScrollState>,
    pub lsp_logs_scroll_x: FxHashMap<String, crate::scroll::ScrollState>,
    pub lsp_logs_focused: Option<String>,
    pub diag_copied_idx: Option<usize>,
    pub problems_tab: usize,
    pub flat_diags: Vec<(std::path::PathBuf, usize)>,
    pub problems_collapsed: FxHashSet<std::path::PathBuf>,
    pub problems_scroll: crate::scroll::ScrollState,
    pub terminals: Vec<crate::app::terminal::Terminal>,
    pub active_terminal: usize,
    pub terminal_focused: bool,
    pub is_dragging_terminal: bool,
    pub tab_drag: Option<TabDragState>,
    pub term_show_search: bool,
    pub term_search_editor: crate::editor::Editor,
    pub term_search_focused: bool,
    pub term_search_case_sensitive: bool,
    pub term_search_results: Vec<(usize, usize, usize, usize)>,
    pub term_search_current_idx: Option<usize>,
}

impl Default for IdePanelState {
    fn default() -> Self {
        Self {
            slots: vec![
                PanelSlot {
                    id: PanelId::Explorer,
                    group: PanelGroup::Top,
                    open: false,
                },
                PanelSlot {
                    id: PanelId::LspServers,
                    group: PanelGroup::Top,
                    open: false,
                },
                PanelSlot {
                    id: PanelId::Terminal,
                    group: PanelGroup::Bottom,
                    open: false,
                },
                PanelSlot {
                    id: PanelId::Problems,
                    group: PanelGroup::Bottom,
                    open: false,
                },
            ],
            left_width: 240.0,
            bottom_height: 180.0,
            drag: None,
            is_resizing_left: false,
            is_resizing_bottom: false,
            file_tree_nodes: Vec::new(),
            file_tree_expanded: FxHashSet::default(),
            explorer_scroll: crate::scroll::ScrollState::new(15.0),
            file_tree_hovered_idx: None,
            lsp_servers: Vec::new(),
            lsp_logs_expanded: FxHashSet::default(),
            lsp_scroll_y: crate::scroll::ScrollState::new(15.0),
            lsp_scroll_x: crate::scroll::ScrollState::new(15.0),
            lsp_log_editors: FxHashMap::default(),
            lsp_logs_scroll_y: FxHashMap::default(),
            lsp_logs_scroll_x: FxHashMap::default(),
            lsp_logs_focused: None,
            diag_copied_idx: None,
            problems_tab: 0,
            flat_diags: Vec::new(),
            problems_collapsed: FxHashSet::default(),
            problems_scroll: crate::scroll::ScrollState::new(15.0),
            terminals: Vec::new(),
            active_terminal: 0,
            terminal_focused: false,
            is_dragging_terminal: false,
            tab_drag: None,
            term_show_search: false,
            term_search_editor: crate::editor::Editor::new(256),
            term_search_focused: false,
            term_search_case_sensitive: false,
            term_search_results: Vec::new(),
            term_search_current_idx: None,
        }
    }
}

impl IdePanelState {
    pub fn any_top_open(&self) -> bool {
        self.slots
            .iter()
            .any(|s| s.group == PanelGroup::Top && s.open)
    }
    pub fn any_bottom_open(&self) -> bool {
        self.slots
            .iter()
            .any(|s| s.group == PanelGroup::Bottom && s.open)
    }
    pub fn toggle(&mut self, id: PanelId) {
        if let Some(slot) = self.slots.iter_mut().find(|s| s.id == id) {
            slot.open = !slot.open;
        }
    }
    pub fn is_open(&self, id: PanelId) -> bool {
        self.slots
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.open)
            .unwrap_or(false)
    }
}

#[inline(always)]
pub(super) fn fuzzy_match(pattern: &str, target: &str) -> Option<Vec<usize>> {
    let mut p_chars = pattern.chars().peekable();
    let mut indices = Vec::with_capacity(pattern.len());
    for (i, c) in target.chars().enumerate() {
        if let Some(&pc) = p_chars.peek() {
            if c.to_ascii_lowercase() == pc.to_ascii_lowercase() {
                indices.push(i);
                p_chars.next();
            }
        } else {
            break;
        }
    }
    if p_chars.peek().is_none() {
        Some(indices)
    } else {
        None
    }
}

#[derive(Clone, Debug)]
pub struct KeyLog {
    pub key: String,
    pub t0: std::time::Instant,
    pub t_highlight: Option<std::time::Instant>,
    pub t_render: Option<std::time::Instant>,
}

pub struct App {
    pub pending_key_log: Option<KeyLog>,
    pub gl_config: Option<glutin::config::Config>,
    pub gl_context: Option<PossiblyCurrentContext>,
    pub gl_surface: Option<Surface<WindowSurface>>,
    pub window: Option<std::sync::Arc<Window>>,
    pub dialog_window: Option<std::sync::Arc<Window>>,
    pub dialog_gl_surface: Option<Surface<WindowSurface>>,
    pub settings_scroll: crate::scroll::ScrollState,
    pub tab_scroll: crate::scroll::ScrollState,
    pub renderer: Option<Renderer>,
    pub editor: Editor,
    pub clipboard: Option<Clipboard>,
    pub theme: Theme,
    pub base_title: String,
    pub file_path: Option<PathBuf>,

    pub file_extension: String,
    pub highlighter: Highlighter,
    pub last_sent_version: u64,

    pub scroll_y: crate::scroll::ScrollState,
    pub scroll_x: crate::scroll::ScrollState,

    pub last_frame: Instant,
    pub last_action: Instant,
    pub last_blink_state: bool,
    pub modifiers: ModifiersState,
    pub is_dragging: bool,
    pub is_focused: bool,
    pub current_cursor: winit::window::CursorIcon,

    pub show_fps: bool,
    pub window_width: f64,
    pub window_height: f64,

    pub last_resize_time: Option<Instant>,

    pub last_click_time: Instant,
    pub click_count: u8,
    pub last_click_pos: (f32, f32),

    pub pending_action: PendingAction,
    pub open_file_rx: Option<std::sync::mpsc::Receiver<Option<PathBuf>>>,
    pub save_file_rx: Option<std::sync::mpsc::Receiver<Option<PathBuf>>>,

    pub show_welcome: bool,
    pub recent_files: Vec<PathBuf>,

    pub is_ide_mode: bool,
    pub ide_workspaces: Vec<PathBuf>,
    /// Пользовательские паттерны игноров для дерева файлов
    pub ide_ignore_patterns: Vec<String>,
    /// Текущий ввод в поле добавления нового паттерна игнора (настройки → IDE)
    pub settings_ignore_editor: Editor,
    /// Поле ввода игнора сфокусировано
    pub settings_ignore_focused: bool,
    pub settings_ignore_scroll_x: f32,
    pub is_dragging_settings_ignore: bool,
    pub open_folder_rx: Option<std::sync::mpsc::Receiver<Option<PathBuf>>>,

    pub show_search: bool,
    pub search_anim_y: f32,
    pub search_editor: Editor,
    pub search_focused: bool,
    pub search_case_sensitive: bool,
    pub search_results: Vec<(usize, usize)>,
    pub search_current_idx: Option<usize>,
    pub is_dragging_search: bool,

    pub is_dragging_lsp_log: bool,

    pub faq_editor: Editor,

    pub is_ready: bool,
    pub is_highlighted_once: bool,
    pub tried_maximize: bool,
    pub should_maximize: bool,

    pub autocomplete_active: bool,
    pub autocomplete_options: Vec<(CompletionItem, Vec<usize>)>,
    pub autocomplete_selected_idx: usize,
    pub autocomplete_anim_progress: f32,
    pub autocomplete_scroll: crate::scroll::ScrollState,
    pub autocomplete_hovered_idx: Option<usize>,
    pub autocomplete_rect: Option<(f32, f32, f32, f32)>,

    pub current_sticky_lines: Vec<(usize, usize)>,
    pub target_sticky_lines: Vec<(usize, usize)>,
    pub sticky_anim_progress: f32,
    pub sticky_anim_is_adding: bool,

    pub show_settings: bool,
    pub settings_anim_progress: f32,
    pub settings_y: f32,
    pub settings_tab: usize,
    pub settings_ide_scroll: crate::scroll::ScrollState,

    pub ide_panel: IdePanelState,
    pub file_tree_rx: Option<std::sync::mpsc::Receiver<Vec<crate::app::file_tree::FileNode>>>,
    /// Канал сигналов от notify-watcher. `()` = что-то изменилось в workspaces.
    pub file_tree_notify_rx: Option<std::sync::mpsc::Receiver<()>>,
    /// LSP менеджер: стартует лениво при открытии .py в IDE-режиме
    pub lsp: Option<crate::lsp::LspManager>,
    /// Меню быстрых действий LSP (Alt+Enter)
    pub lsp_actions_menu: Option<LspActionsMenu>,
    /// Ожидаем ответа на Fix All запрос
    pub pending_fix_all_id: Option<i32>,

    /// Декларативная система UI для автоматической обработки кликов
    pub ui_registry: crate::ui_system::UiRegistry,

    pub tabs: Vec<EditorTab>,
    pub active_tab: usize,

    /// Флаг для отложенного входа в IDE-режим при старте с --ide
    pub run_ide_on_startup: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_state_toggle_and_group_queries_end_to_end() {
        let mut panels = IdePanelState::default();

        assert!(!panels.any_top_open());
        assert!(!panels.any_bottom_open());
        assert!(!panels.is_open(PanelId::Explorer));

        panels.toggle(PanelId::Explorer);
        panels.toggle(PanelId::Terminal);

        assert!(panels.is_open(PanelId::Explorer));
        assert!(panels.is_open(PanelId::Terminal));
        assert!(panels.any_top_open());
        assert!(panels.any_bottom_open());

        panels.toggle(PanelId::Explorer);
        assert!(!panels.is_open(PanelId::Explorer));
        assert!(panels.any_bottom_open());
    }

    #[test]
    fn fuzzy_match_preserves_target_indices_case_insensitively() {
        assert_eq!(fuzzy_match("rtr", "RRiter"), Some(vec![0, 3, 5]));
        assert_eq!(fuzzy_match("IDE", "IntegratedDevEnv"), Some(vec![0, 9, 11]));
        assert_eq!(fuzzy_match("xyz", "RRiter"), None);
    }

    #[test]
    fn panel_metadata_maps_to_labels_and_icons() {
        assert_eq!(PanelId::Explorer.label(), "Проводник");
        assert_eq!(PanelId::Terminal.label(), "Терминал");
        assert!(PanelId::Problems.icon() == crate::widgets::IconType::Problems);
        assert!(PanelId::LspServers.icon() == crate::widgets::IconType::LspServers);
    }
}
