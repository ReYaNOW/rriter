pub mod events;
pub mod file_tree;
pub mod input;

use crate::editor::Editor;
use crate::highlighter::{CompletionItem, Highlighter, SymbolKind};
use crate::renderer::{Renderer, Theme};
use arboard::Clipboard;
use glutin::context::PossiblyCurrentContext;
use glutin::display::GetGlDisplay;
use glutin::surface::{Surface, WindowSurface};
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::PathBuf;
use std::time::Instant;
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::ModifiersState;
use winit::platform::wayland::WindowAttributesExtWayland;
use winit::window::Window;

#[derive(Clone, Copy, PartialEq)]
pub enum PendingAction {
    Quit,
    OpenFile,
    CloseFile,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PanelId {
    Explorer,
    Terminal,
    Problems,
}

impl PanelId {
    pub fn label(self) -> &'static str {
        match self {
            PanelId::Explorer => "Проводник",
            PanelId::Terminal => "Терминал",
            PanelId::Problems => "Ляпы",
        }
    }
    pub fn icon(self) -> crate::widgets::IconType {
        match self {
            PanelId::Explorer => crate::widgets::IconType::Plus,
            PanelId::Terminal => crate::widgets::IconType::CaseMatch,
            PanelId::Problems => crate::widgets::IconType::Warning,
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
}

impl Default for IdePanelState {
    fn default() -> Self {
        Self {
            slots: vec![
                PanelSlot { id: PanelId::Explorer, group: PanelGroup::Top,    open: false },
                PanelSlot { id: PanelId::Terminal, group: PanelGroup::Bottom, open: false },
                PanelSlot { id: PanelId::Problems, group: PanelGroup::Bottom, open: false },
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
        }
    }
}

impl IdePanelState {
    pub fn any_top_open(&self) -> bool {
        self.slots.iter().any(|s| s.group == PanelGroup::Top && s.open)
    }
    pub fn any_bottom_open(&self) -> bool {
        self.slots.iter().any(|s| s.group == PanelGroup::Bottom && s.open)
    }
    pub fn toggle(&mut self, id: PanelId) {
        if let Some(slot) = self.slots.iter_mut().find(|s| s.id == id) {
            slot.open = !slot.open;
        }
    }
    pub fn is_open(&self, id: PanelId) -> bool {
        self.slots.iter().find(|s| s.id == id).map(|s| s.open).unwrap_or(false)
    }
}

#[inline(always)]
fn fuzzy_match(pattern: &str, target: &str) -> Option<Vec<usize>> {
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

pub struct App {
    pub gl_config: Option<glutin::config::Config>,
    pub gl_context: Option<PossiblyCurrentContext>,
    pub gl_surface: Option<Surface<WindowSurface>>,
    pub window: Option<Window>,
    pub dialog_window: Option<Window>,
    pub dialog_gl_surface: Option<Surface<WindowSurface>>,
    pub settings_scroll: crate::scroll::ScrollState,
    pub renderer: Option<Renderer>,
    pub editor: Editor,
    pub clipboard: Clipboard,
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
    pub open_folder_rx: Option<std::sync::mpsc::Receiver<Option<PathBuf>>>,

    pub show_search: bool,
    pub search_anim_y: f32,
    pub search_editor: Editor,
    pub search_focused: bool,
    pub search_case_sensitive: bool,
    pub search_results: Vec<(usize, usize)>,
    pub search_current_idx: Option<usize>,
    pub is_dragging_search: bool,

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

            pub ide_panel: IdePanelState,
            pub file_tree_rx: Option<std::sync::mpsc::Receiver<Vec<crate::app::file_tree::FileNode>>>,
        }

impl App {
    pub fn ensure_cursor_visible(
        target_scroll_y: &mut f32,
        target_scroll_x: &mut f32,
        editor: &Editor,
        renderer: &mut Renderer,
        window_width: f32,
        window_height: f32,
    ) {
        let (cx_screen, cy) = renderer.get_cursor_xy(editor);

        if cy - renderer.baseline_offset < *target_scroll_y {
            *target_scroll_y = (cy - renderer.baseline_offset).max(0.0);
            *target_scroll_y =
                (*target_scroll_y / renderer.line_height).floor() * renderer.line_height;
        } else if cy - renderer.baseline_offset + renderer.line_height
            > *target_scroll_y + window_height
        {
            *target_scroll_y = cy - renderer.baseline_offset + renderer.line_height - window_height;
            *target_scroll_y =
                (*target_scroll_y / renderer.line_height).ceil() * renderer.line_height;
        }

        let max_s_y = renderer.get_max_scroll(editor, window_height);
        *target_scroll_y = target_scroll_y.clamp(0.0, max_s_y).round();

        let visible_left = renderer.left_padding + 30.0;
        let visible_right = window_width - renderer.minimap_width - 40.0;

        if cx_screen < visible_left {
            *target_scroll_x -= visible_left - cx_screen;
        } else if cx_screen > visible_right {
            *target_scroll_x += cx_screen - visible_right;
        }

        *target_scroll_x = target_scroll_x.clamp(0.0, renderer.max_scroll_x).round();
    }

    pub fn get_current_word_prefix(&self) -> String {
        let mut p = self.editor.cursor;
        while p > 0 {
            let b = self.editor.byte_at(p - 1);
            if !(b.is_ascii_alphanumeric() || b == b'_') {
                break;
            }
            p -= 1;
        }
        if p == self.editor.cursor {
            return String::new();
        }
        let len = self.editor.cursor - p;
        let mut res = Vec::with_capacity(len);
        for i in p..self.editor.cursor {
            res.push(self.editor.byte_at(i));
        }
        String::from_utf8(res).unwrap_or_default()
    }

    pub fn update_autocomplete(&mut self) {
        let prefix = self.get_current_word_prefix();
        if prefix.is_empty() {
            self.autocomplete_active = false;
            self.autocomplete_options.clear();
            return;
        }

        let prefix_lower = prefix.to_lowercase();
        let cursor = self.editor.cursor;

        let mut best_scopes: FxHashMap<String, CompletionItem> = FxHashMap::default();

        for comp in &self.highlighter.completions {
            if cursor >= comp.scope_start && cursor <= comp.scope_end {
                let current_size = comp.scope_end.saturating_sub(comp.scope_start);
                if let Some(existing) = best_scopes.get(&comp.word) {
                    let ex_size = existing.scope_end.saturating_sub(existing.scope_start);
                    if current_size < ex_size {
                        best_scopes.insert(comp.word.clone(), comp.clone());
                    }
                } else {
                    best_scopes.insert(comp.word.clone(), comp.clone());
                }
            }
        }

        let mut matches = Vec::with_capacity(best_scopes.len());

        for (_, comp) in best_scopes {
            if comp.word == prefix {
                continue;
            }

            let comp_lower = comp.word.to_lowercase();
            if let Some(indices) = fuzzy_match(&prefix_lower, &comp_lower) {
                let is_prefix = comp_lower.starts_with(&prefix_lower);
                let mut score = 0i64;
                let scope_bonus = if comp.kind == SymbolKind::Keyword {
                    0
                } else {
                    let scope_size = comp.scope_end.saturating_sub(comp.scope_start);
                    let sz = scope_size.min(i64::MAX as usize) as i64;
                    10_000_000 / (sz + 1).max(1)
                };
                score += scope_bonus;
                score -= (comp.word.len() as i64) * 10;
                matches.push((is_prefix, score, comp, indices));
            }
        }

        matches.sort_unstable_by_key(|(is_prefix, score, comp, _)| {
            let type_priority = match comp.kind {
                SymbolKind::Variable | SymbolKind::Parameter => 0,
                SymbolKind::Function => 1,
                SymbolKind::Class => 2,
                SymbolKind::Keyword => 3,
                SymbolKind::Unknown => 4,
            };

            let match_priority = if *is_prefix { 0 } else { 1 };
            (match_priority, type_priority, std::cmp::Reverse(*score))
        });

        self.autocomplete_options = matches.into_iter().take(60).map(|m| (m.2, m.3)).collect();

        if !self.autocomplete_options.is_empty() {
            if !self.autocomplete_active {
                self.autocomplete_anim_progress = 0.0;
                self.autocomplete_scroll.current = 0.0;
                self.autocomplete_scroll.target = 0.0;
            }
            self.autocomplete_active = true;
            self.autocomplete_selected_idx = 0;
        } else {
            self.autocomplete_active = false;
        }
    }

    pub fn ensure_autocomplete_visible(&mut self) {
        let scale = self
            .renderer
            .as_ref()
            .map(|r| r.scale_factor)
            .unwrap_or(1.0);
        let step = 36.0 * scale;
        let visible_items = 7.0;

        self.autocomplete_scroll.anim_speed = 15.0;
        let top = self.autocomplete_scroll.target;
        let bottom = top + (visible_items * step);

        let item_top = self.autocomplete_selected_idx as f32 * step;
        let item_bottom = item_top + step;

        if item_top < top {
            self.autocomplete_scroll.set_target(item_top);
        } else if item_bottom > bottom {
            self.autocomplete_scroll
                .set_target(item_bottom - (visible_items * step));
        }

        let total_items = self.autocomplete_options.len() as f32;
        let visible_limit = total_items.min(visible_items);
        let max_scroll = ((total_items - visible_limit) * step).max(0.0);

        self.autocomplete_scroll.clamp_target(0.0, max_scroll);
    }

    pub fn apply_autocomplete(&mut self) {
        if !self.autocomplete_active || self.autocomplete_options.is_empty() {
            return;
        }
        let selected = self.autocomplete_options[self.autocomplete_selected_idx]
            .0
            .word
            .clone();
        let prefix_len = self.get_current_word_prefix().len();

        for _ in 0..prefix_len {
            if let Some((offset, len)) = self.editor.backspace() {
                self.highlighter.shift_delete(offset, len);
            }
        }

        let (del_info, ins_len) = self.editor.insert_str(&selected);
        if let Some((offset, len)) = del_info {
            self.highlighter.shift_delete(offset, len);
        }
        self.highlighter
            .shift_insert(self.editor.cursor - ins_len, ins_len, Some(&selected));

        self.autocomplete_active = false;
        self.autocomplete_selected_idx = 0;
        self.autocomplete_scroll.current = 0.0;
        self.autocomplete_scroll.target = 0.0;

        if let Some(w) = self.window.as_ref() {
            App::update_window_title(w, &self.base_title, self.editor.is_dirty());
            w.request_redraw();
        }
    }

    pub fn update_search(&mut self) {
        let previous_match_start = self
            .search_current_idx
            .and_then(|idx| self.search_results.get(idx).map(|&(s, _)| s));
        self.search_results.clear();
        self.search_current_idx = None;
        let query_text = self.search_editor.get_full_text();
        if query_text.is_empty() {
            return;
        }

        let full_text = self.editor.get_full_text();
        let escaped_query = regex::escape(&query_text);
        if let Ok(re) = regex::RegexBuilder::new(&escaped_query)
            .case_insensitive(!self.search_case_sensitive)
            .dot_matches_new_line(true)
            .build()
        {
            for mat in re.find_iter(&full_text) {
                self.search_results.push((mat.start(), mat.end()));
            }
        }

        if !self.search_results.is_empty() {
            if let Some(prev_start) = previous_match_start {
                if let Ok(idx) = self
                    .search_results
                    .binary_search_by_key(&prev_start, |&(s, _)| s)
                {
                    self.search_current_idx = Some(idx);
                    return;
                }
            }
            let cursor = self.editor.cursor;
            let mut nearest_idx = 0;
            let mut min_dist = usize::MAX;
            for (i, &(s_start, s_end)) in self.search_results.iter().enumerate() {
                let dist = if cursor < s_start {
                    s_start - cursor
                } else if cursor > s_end {
                    cursor - s_end
                } else {
                    0
                };
                if dist < min_dist {
                    min_dist = dist;
                    nearest_idx = i;
                    if dist == 0 {
                        break;
                    }
                }
            }
            self.search_current_idx = Some(nearest_idx);
        }
    }

    pub fn jump_to_search_result(&mut self) {
        if let Some(idx) = self.search_current_idx {
            if let Some(&(start, end)) = self.search_results.get(idx) {
                self.editor.cursor = end;
                self.editor.selection_anchor = Some(start);
                if let Some(r) = self.renderer.as_mut() {
                    let phys_line = self
                        .editor
                        .line_offsets
                        .partition_point(|&o| o <= end)
                        .saturating_sub(1);

                    let line_top_y = phys_line as f32 * r.line_height;

                    let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                    self.scroll_y.target = (line_top_y - wh / 2.0).max(0.0);

                    let max_s = r.get_max_scroll(&self.editor, wh);
                    self.scroll_y.clamp_target(0.0, max_s);
                    self.scroll_y.target = self.scroll_y.target.round();
                    self.scroll_y.anim_speed = 10.0;
                }
            }
        }
    }

    pub fn update_window_title(window: &Window, base_title: &str, is_dirty: bool) {
        let title = if is_dirty {
            format!("{} * — RRiter", base_title)
        } else {
            format!("{} — RRiter", base_title)
        };
        window.set_title(&title);
    }

    pub fn show_action_dialog(&mut self, event_loop: &ActiveEventLoop, action: PendingAction) {
        self.is_dragging = false;
        self.scroll_y.is_dragging = false;
        self.scroll_x.is_dragging = false;
        self.pending_action = action;

        if self.dialog_window.is_some() {
            return;
        }

        let attrs = winit::window::Window::default_attributes()
            .with_title("Подтверждение — RRiter")
            .with_inner_size(winit::dpi::LogicalSize::new(660.0, 260.0))
            .with_name("rriter", "rriter")
            .with_window_level(winit::window::WindowLevel::AlwaysOnTop)
            .with_resizable(false);

        if let Ok(window) = event_loop.create_window(attrs) {
            use glutin::display::GlDisplay;
            use winit::raw_window_handle::HasWindowHandle;
            let raw_handle = window.window_handle().unwrap().as_raw();
            let display = self.gl_config.as_ref().unwrap().display();
            let scale = window.scale_factor();
            let phys_w = (660.0 * scale).round() as u32;
            let phys_h = (260.0 * scale).round() as u32;
            let surface_attrs =
                glutin::surface::SurfaceAttributesBuilder::<glutin::surface::WindowSurface>::new()
                    .build(
                        raw_handle,
                        std::num::NonZeroU32::new(phys_w.max(1)).unwrap(),
                        std::num::NonZeroU32::new(phys_h.max(1)).unwrap(),
                    );
            let surface = unsafe {
                display
                    .create_window_surface(self.gl_config.as_ref().unwrap(), &surface_attrs)
                    .unwrap()
            };
            self.dialog_window = Some(window);
            self.dialog_gl_surface = Some(surface);
        }
    }

    pub fn close_dialog(&mut self) {
        self.dialog_window = None;
        self.dialog_gl_surface = None;
        if let Some(w) = self.window.as_ref() {
            w.request_redraw();
        }
    }

    pub fn close_current_file(&mut self) {
        self.file_path = None;
        self.base_title = "Добро пожаловать".to_string();
        let old_version = self.editor.version;
        self.editor = Editor::new(8192);
        self.editor.version = old_version + 1;
        self.editor.set_original_text();
        self.editor.sync_edits.clear();
        self.highlighter
            .reset(self.editor.version, "".to_string(), "".to_string());
        self.search_results.clear();
        self.search_current_idx = None;
        self.show_search = false;
        self.autocomplete_active = false;
        self.show_welcome = true;

        self.scroll_y.current = 0.0;
        self.scroll_y.target = 0.0;
        self.scroll_x.current = 0.0;
        self.scroll_x.target = 0.0;

        if let Some(w) = self.window.as_ref() {
            App::update_window_title(w, &self.base_title, false);
            w.request_redraw();
        }
    }

    pub fn trigger_file_picker(&mut self) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.open_file_rx = Some(rx);
        std::thread::spawn(move || {
            let file = rfd::FileDialog::new().set_title("Открыть файл").pick_file();
            let _ = tx.send(file);
        });
    }

    pub fn trigger_folder_picker(&mut self) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.open_folder_rx = Some(rx);
        std::thread::spawn(move || {
            let folder = rfd::FileDialog::new()
                .set_title("Выбрать папку")
                .pick_folder();
            let _ = tx.send(folder);
        });
    }

    pub fn trigger_save_as_picker(&mut self) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.save_file_rx = Some(rx);
        std::thread::spawn(move || {
            let file = rfd::FileDialog::new()
                .set_title("Сохранить файл как...")
                .set_file_name("Безымянный.txt")
                .save_file();
            let _ = tx.send(file);
        });
    }

    pub fn save_current_file(&mut self) -> bool {
        if let Some(path) = self.file_path.clone() {
            let content = self.editor.get_full_text();
            match std::fs::write(&path, &content) {
                Ok(_) => {
                    self.editor.mark_saved();
                    return true;
                }
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    use std::io::Write;
                    use std::process::{Command, Stdio};
                    if let Ok(mut child) = Command::new("pkexec")
                        .arg("tee")
                        .arg(&path)
                        .stdin(Stdio::piped())
                        .stdout(Stdio::null())
                        .spawn()
                    {
                        if let Some(mut stdin) = child.stdin.take() {
                            let _ = stdin.write_all(content.as_bytes());
                        }
                        if let Ok(status) = child.wait() {
                            if status.success() {
                                self.editor.mark_saved();
                                return true;
                            }
                        }
                    }
                }
                Err(_) => {}
            }
        } else {
            self.trigger_save_as_picker();
        }
        false
    }

    pub fn add_recent_file(&mut self, path: PathBuf) {
        self.recent_files.retain(|p| p != &path);
        self.recent_files.insert(0, path);
        self.recent_files.truncate(10);
        crate::save_recent_files(&self.recent_files);
    }

    pub fn load_file(&mut self, path: PathBuf) {
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                self.show_welcome = false;
                self.add_recent_file(path.clone());

                let old_version = self.editor.version;
                self.editor = Editor::new(content.len() + 8192);
                self.editor.version = old_version + 1;

                if !content.is_empty() {
                    let _ = self.editor.insert_str(&content);
                    self.editor.cursor = 0;
                    self.editor.clear_history();
                }
                self.editor.set_original_text();
                self.editor.sync_edits.clear();
                self.file_path = Some(path.clone());
                let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                self.base_title = file_name.into_owned();
                self.file_extension = path
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_default();
                self.highlighter.spans.clear();
                self.is_highlighted_once = false;
                self.highlighter.reset(
                    self.editor.version,
                    self.editor.get_full_text(),
                    self.file_extension.clone(),
                );

                self.scroll_y.current = 0.0;
                self.scroll_y.target = 0.0;
                self.scroll_x.current = 0.0;
                self.scroll_x.target = 0.0;

                self.last_sent_version = u64::MAX;
                self.search_results.clear();
                self.search_current_idx = None;
                self.autocomplete_active = false;
                if let Some(w) = self.window.as_ref() {
                    App::update_window_title(w, &self.base_title, false);
                    w.request_redraw();
                }
            }
            Err(_) => {
                self.recent_files.retain(|p| p != &path);
                crate::save_recent_files(&self.recent_files);
                if let Some(w) = self.window.as_ref() {
                    w.request_redraw();
                }
            }
        }
    }
}
