mod app_state;
mod autocomplete;
pub mod events;
pub mod file_icons;
pub mod file_tree;
pub mod git_diff;
pub mod git_panel;
pub mod keyboard;
pub mod lsp_actions;
pub mod mouse;
mod python_completion;
pub mod terminal;
pub mod ui_handlers;
use crate::editor::Editor;
use crate::highlighter::{CompletionItem, SymbolKind, TREE_SITTER_HIGHLIGHT_MAX_BYTES};
use crate::renderer::Renderer;
use app_state::fuzzy_match;
pub use app_state::*;
use glutin::display::GetGlDisplay;
use python_completion::*;
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::{Path, PathBuf};
use winit::event_loop::ActiveEventLoop;
use winit::platform::wayland::WindowAttributesExtWayland;
use winit::window::Window;

const FILE_OPEN_HIGHLIGHT_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(150);
const FILE_OPEN_LARGE_PRIORITY_HIGHLIGHT_TIMEOUT: std::time::Duration =
    std::time::Duration::from_millis(700);
const FILE_OPEN_BLOCKING_HIGHLIGHT_MAX_BYTES: usize = TREE_SITTER_HIGHLIGHT_MAX_BYTES;

fn apply_initial_import_folds(editor: &mut Editor, ext: &str, text: &str) {
    let mut add_fold = |start_b: usize, end_b: usize| {
        if editor
            .foldable_ranges_bytes
            .iter()
            .any(|&(start, end, _)| start == start_b && end == end_b)
        {
            return;
        }
        editor.foldable_ranges_bytes.push((start_b, end_b, false));
        let sl = editor
            .line_offsets
            .partition_point(|&x| x <= start_b)
            .saturating_sub(1);
        let el = editor
            .line_offsets
            .partition_point(|&x| x <= end_b)
            .saturating_sub(1);
        if el > sl {
            editor.foldable_lines.insert(sl, el);
            editor.folded_lines.insert(sl);
            editor.folded_start_bytes.insert(editor.line_offsets[sl]);
        }
    };

    match ext {
        "py" | "pyi" => {
            for block in crate::languages::python::import_blocks(text) {
                add_fold(block.start, block.end);
            }
            for (start, end) in initial_python_bracket_folds(text) {
                add_fold(start, end);
            }
        }
        "rs" => {
            for block in crate::languages::rust::import_blocks(text) {
                add_fold(block.start, block.end);
            }
        }
        "dart" => {
            for block in crate::languages::dart::import_blocks(text) {
                add_fold(block.start, block.end);
            }
        }
        _ => {}
    }
}

impl App {
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) fn set_clipboard_text(&mut self, text: impl Into<String>) {
        if let Some(clipboard) = self.clipboard.as_mut() {
            let _ = clipboard.set_text(text.into());
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) fn get_clipboard_text(&mut self) -> Option<String> {
        self.clipboard.as_mut()?.get_text().ok()
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn enter_ide_mode(&mut self) {
        self.is_ide_mode = true;

        let was_welcome = self.show_welcome;
        self.show_welcome = false;
        if was_welcome && self.base_title == "Добро пожаловать" {
            self.base_title = "Безымянный".to_string();
            self.file_path = None;
        }

        self.ide_panel = crate::load_panel_state();
        self.ide_panel.enforce_single_open_per_group();

        if self.ide_panel.is_open(PanelId::Terminal) && self.ide_panel.terminals.is_empty() {
            self.ide_panel
                .terminals
                .push(crate::app::terminal::Terminal::spawn(self.window.clone()));
            self.ide_panel.active_terminal = 0;
            self.ide_panel.terminal_focused = true;
        }

        if self.lsp.is_none() {
            self.lsp = Some(crate::lsp::LspManager::new(self.ide_workspaces.clone()));
        }

        let has_startup_file =
            self.file_path.is_some() || self.editor.len() > 0 || self.editor.is_dirty();

        if has_startup_file && self.tabs.is_empty() {
            self.tabs.push(EditorTab {
                editor: crate::editor::Editor::new(128),
                file_path: self.file_path.clone(),
                base_title: self.base_title.clone(),
                file_extension: self.file_extension.clone(),
                scroll_y: crate::scroll::ScrollState::new(15.0),
                scroll_x: crate::scroll::ScrollState::new(15.0),
                spans: Vec::new(),
                completions: Vec::new(),
                foldable_ranges: Vec::new(),
                last_sent_version: u64::MAX,
                search_results: Vec::new(),
                search_current_idx: None,
                is_highlighted_once: false,
                icon_key: "default_file",
                syntax_errors: Vec::new(),
                kind: EditorTabKind::Normal,
            });
            self.active_tab = 0;
        }

        let (saved_tabs, saved_active) = crate::load_open_tabs(true);

        if !saved_tabs.is_empty() {
            let mut loaded_any = false;
            for path_opt in saved_tabs {
                if let Some(path) = path_opt {
                    if path.exists() {
                        self.open_file_in_tab_bg(path, false);
                        loaded_any = true;
                    }
                } else {
                    self.open_new_tab();
                    loaded_any = true;
                }
            }

            if loaded_any {
                let target = if has_startup_file {
                    0
                } else {
                    saved_active.min(self.tabs.len().saturating_sub(1))
                };
                self.switch_to_tab(target);
                if !self.is_highlighted_once {
                    self.wait_for_current_highlight();
                }
            }
        }

        let title = self.base_title.clone();
        if !self.tabs.is_empty() {
            self.tabs[self.active_tab].icon_key =
                crate::app::file_icons::file_icon_key(&title.to_lowercase());
        }

        if let Some(path) = &self.file_path {
            if let Some(lsp) = &mut self.lsp {
                let text = self.editor.get_full_text();
                lsp.notify_open(
                    path,
                    &self.file_extension,
                    &text,
                    self.editor.version as i32,
                );
            }
        }

        self.refresh_file_tree();
        self.start_file_watcher();
        if self.ide_panel.is_open(PanelId::Git) {
            self.refresh_git_panel();
        }

        if let Some(w) = self.window.as_ref() {
            App::update_window_title(w, &self.base_title, self.editor.is_dirty());
            w.request_redraw();
        }
    }
    pub fn save_tabs_state(&mut self) {
        if !self.is_ide_mode {
            return;
        }
        self.sync_active_tab();
        crate::save_open_tabs(&self.tabs, self.active_tab, self.is_ide_mode);
        self.sync_active_tab();
    }

    pub fn sync_active_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        let ai = self.active_tab;
        std::mem::swap(&mut self.editor, &mut self.tabs[ai].editor);
        std::mem::swap(&mut self.highlighter.spans, &mut self.tabs[ai].spans);
        std::mem::swap(
            &mut self.highlighter.completions,
            &mut self.tabs[ai].completions,
        );
        std::mem::swap(
            &mut self.highlighter.foldable_ranges,
            &mut self.tabs[ai].foldable_ranges,
        );
        std::mem::swap(
            &mut self.highlighter.syntax_errors,
            &mut self.tabs[ai].syntax_errors,
        );
        std::mem::swap(&mut self.file_path, &mut self.tabs[ai].file_path);
        std::mem::swap(&mut self.base_title, &mut self.tabs[ai].base_title);
        std::mem::swap(&mut self.file_extension, &mut self.tabs[ai].file_extension);
        std::mem::swap(&mut self.scroll_y, &mut self.tabs[ai].scroll_y);
        std::mem::swap(&mut self.scroll_x, &mut self.tabs[ai].scroll_x);
        std::mem::swap(&mut self.search_results, &mut self.tabs[ai].search_results);
        std::mem::swap(
            &mut self.search_current_idx,
            &mut self.tabs[ai].search_current_idx,
        );
        std::mem::swap(
            &mut self.last_sent_version,
            &mut self.tabs[ai].last_sent_version,
        );
        std::mem::swap(
            &mut self.is_highlighted_once,
            &mut self.tabs[ai].is_highlighted_once,
        );

        let title_to_use = if self.base_title.len() > self.tabs[ai].base_title.len() {
            &self.base_title
        } else {
            &self.tabs[ai].base_title
        };
        let icon_key = crate::app::file_icons::file_icon_key(&title_to_use.to_lowercase());
        self.tabs[ai].icon_key = icon_key;
    }

    fn next_tab_highlight_version(&self) -> u64 {
        self.tabs
            .iter()
            .map(|t| t.editor.version)
            .max()
            .unwrap_or(0)
            .max(self.editor.version)
            .max(self.highlighter.current_version)
            .saturating_add(1)
    }

    fn tab_display_titles(&self) -> Vec<String> {
        let mut paths: Vec<Option<&PathBuf>> =
            self.tabs.iter().map(|t| t.file_path.as_ref()).collect();
        if self.active_tab < paths.len() {
            paths[self.active_tab] = self.file_path.as_ref();
        }

        let mut display_titles = vec![String::new(); self.tabs.len()];
        for i in 0..self.tabs.len() {
            if let Some(p1) = paths[i] {
                let mut diff_level = 0;
                let mut collision = false;
                for j in 0..self.tabs.len() {
                    if i == j {
                        continue;
                    }
                    if let Some(p2) = paths[j] {
                        if p1.file_name() == p2.file_name() {
                            collision = true;
                            let mut it1 = p1.components().rev();
                            let mut it2 = p2.components().rev();
                            let mut level = 0;
                            loop {
                                let c1 = it1.next();
                                let c2 = it2.next();
                                if c1 != c2 {
                                    diff_level = diff_level.max(level);
                                    break;
                                }
                                if c1.is_none() && c2.is_none() {
                                    break;
                                }
                                level += 1;
                            }
                        }
                    }
                }
                if collision && diff_level > 0 {
                    let comps: Vec<_> = p1.components().rev().collect();
                    if diff_level < comps.len() {
                        let diff_dir = comps[diff_level].as_os_str().to_string_lossy();
                        let file_name = comps[0].as_os_str().to_string_lossy();
                        display_titles[i] = if diff_level == 1 {
                            format!("{}/{}", diff_dir, file_name)
                        } else {
                            format!("{}/.../{}", diff_dir, file_name)
                        };
                    } else {
                        display_titles[i] = p1.to_string_lossy().into_owned();
                    }
                } else {
                    display_titles[i] = p1
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                }
            } else {
                let title = if i == self.active_tab {
                    if self.base_title.is_empty() {
                        "Безымянный"
                    } else {
                        &self.base_title
                    }
                } else if self.tabs[i].base_title.is_empty() {
                    "Безымянный"
                } else {
                    &self.tabs[i].base_title
                };
                display_titles[i] = title.to_string();
            }
        }
        display_titles
    }

    fn reveal_tab_now(&mut self, idx: usize) {
        if !self.is_ide_mode || idx >= self.tabs.len() {
            return;
        }

        let titles = self.tab_display_titles();
        if let Some(r) = self.renderer.as_mut() {
            let s = r.scale_factor;
            let tab_x = (48.0 * s + self.ide_panel.left_width * s).round() + 1.0;
            let viewport_w = (r.width - tab_x).max(0.0);
            if viewport_w <= 0.0 {
                return;
            }

            let tab_pad = 16.0 * s;
            let icon_size_tab = 20.0 * s;
            let mut tab_left = 0.0;
            let mut total_w = 0.0;
            for (i, title) in titles.iter().enumerate() {
                let title_w = r.measure_ui_width(title, 1.0);
                let tab_w = tab_pad * 2.0 + icon_size_tab + 8.0 * s + title_w + 30.0 * s;
                if i < idx {
                    tab_left += tab_w;
                }
                total_w += tab_w;
            }

            let title_w = r.measure_ui_width(&titles[idx], 1.0);
            let tab_w = tab_pad * 2.0 + icon_size_tab + 8.0 * s + title_w + 30.0 * s;
            let tab_right = tab_left + tab_w;
            let max_scroll = (total_w - viewport_w).max(0.0);
            let margin = (12.0 * s).min(viewport_w * 0.25);
            let mut target = self.tab_scroll.target;

            if tab_left < target + margin {
                target = tab_left - margin;
            } else if tab_right > target + viewport_w - margin {
                target = tab_right + margin - viewport_w;
            }

            let target = target.clamp(0.0, max_scroll);
            self.tab_scroll.target = target;
            self.tab_scroll.current = target;
        }
    }

    fn reveal_active_tab_now(&mut self) {
        self.reveal_tab_now(self.active_tab);
    }

    pub fn switch_to_tab(&mut self, new_idx: usize) {
        if !self.is_ide_mode || self.tabs.is_empty() {
            return;
        }
        if new_idx == self.active_tab || new_idx >= self.tabs.len() {
            return;
        }

        self.sync_active_tab();
        // Урезаем потребление RAM: освобождаем тяжелый AST автокомплита у старой вкладки.
        // При возврате вкладка перепарсится автоматически за 10-30 мс без лагов.
        self.tabs[self.active_tab].completions.clear();
        self.active_tab = new_idx;
        self.sync_active_tab();
        self.prefetch_active_tab_git_graph();

        if self.active_tab_is_git_diff() {
            while let Ok(_) = self.highlighter.rx.try_recv() {}
            if !self.is_highlighted_once {
                self.editor.version = self.next_tab_highlight_version();
                self.prepare_active_git_diff_highlight_after_switch();
            }
        } else {
            self.editor.version = self.next_tab_highlight_version();
            while let Ok(_) = self.highlighter.rx.try_recv() {}
            self.reset_highlighter_with_text(self.editor.get_full_text(), false);
            self.wait_for_current_highlight();
        }

        if self.is_ide_mode {
            if let Some(lsp) = &mut self.lsp {
                if let Some(path) = &self.file_path {
                    let text = self.editor.get_full_text();
                    lsp.notify_open(
                        path,
                        &self.file_extension,
                        &text,
                        self.editor.version as i32,
                    );
                }
            }
        }

        self.autocomplete_active = false;
        self.show_welcome =
            self.tabs.len() <= 1 && self.file_path.is_none() && self.editor.len() == 0;
        self.reveal_active_tab_now();
        if let Some(w) = self.window.as_ref() {
            App::update_window_title(w, &self.base_title, self.editor.is_dirty());
            w.request_redraw();
        }
        self.save_tabs_state();
    }

    pub fn open_new_tab(&mut self) {
        if !self.is_ide_mode {
            self.close_current_file();
            return;
        }

        if self.tabs.is_empty() {
            let old_version = self.editor.version;
            self.editor = crate::editor::Editor::new(8192);
            self.editor.version = old_version + 1;
            self.file_path = None;
            self.base_title = "Безымянный".to_string();
            self.file_extension = String::new();

            let mut tab_editor = crate::editor::Editor::new(8192);
            tab_editor.version = old_version + 1;
            self.tabs.push(EditorTab {
                editor: tab_editor,
                file_path: None,
                base_title: String::new(),
                file_extension: String::new(),
                scroll_y: crate::scroll::ScrollState::new(15.0),
                scroll_x: crate::scroll::ScrollState::new(15.0),
                spans: Vec::new(),
                completions: Vec::new(),
                foldable_ranges: Vec::new(),
                last_sent_version: 0,
                search_results: Vec::new(),
                search_current_idx: None,
                is_highlighted_once: false,
                icon_key: "default_file",
                syntax_errors: Vec::new(),
                kind: EditorTabKind::Normal,
            });
            self.active_tab = 0;
            self.show_welcome = false;
            while let Ok(_) = self.highlighter.rx.try_recv() {}
            self.highlighter
                .reset(self.editor.version, String::new(), String::new(), 0);
            self.autocomplete_active = false;
            if let Some(w) = self.window.as_ref() {
                App::update_window_title(w, &self.base_title, false);
                w.request_redraw();
            }
            self.save_tabs_state();
            return;
        }

        self.sync_active_tab();
        let mut new_editor = crate::editor::Editor::new(8192);
        new_editor.version = self.next_tab_highlight_version();
        let new_tab = EditorTab {
            editor: new_editor,
            file_path: None,
            base_title: "Безымянный".to_string(),
            file_extension: String::new(),
            scroll_y: crate::scroll::ScrollState::new(15.0),
            scroll_x: crate::scroll::ScrollState::new(15.0),
            spans: Vec::new(),
            completions: Vec::new(),
            foldable_ranges: Vec::new(),
            last_sent_version: u64::MAX,
            search_results: Vec::new(),
            search_current_idx: None,
            is_highlighted_once: false,
            icon_key: "default_file",
            syntax_errors: Vec::new(),
            kind: EditorTabKind::Normal,
        };
        self.tabs.push(new_tab);
        self.active_tab = self.tabs.len() - 1;
        self.sync_active_tab();
        while let Ok(_) = self.highlighter.rx.try_recv() {}
        self.highlighter
            .reset(self.editor.version, String::new(), String::new(), 0);

        self.autocomplete_active = false;
        self.show_welcome = false;
        self.reveal_active_tab_now();

        if let Some(w) = self.window.as_ref() {
            App::update_window_title(w, &self.base_title, false);
            w.request_redraw();
        }
        self.save_tabs_state();
    }

    pub fn close_tab_at(&mut self, idx: usize) {
        if !self.is_ide_mode {
            self.close_current_file();
            return;
        }

        if self.tabs.len() <= 1 {
            self.close_current_file();
            return;
        }

        if idx == self.active_tab {
            self.sync_active_tab();
            self.tabs.remove(idx);
            self.active_tab = if idx > 0 { idx - 1 } else { 0 };
            self.sync_active_tab();
            self.editor.version = self.next_tab_highlight_version();
            while let Ok(_) = self.highlighter.rx.try_recv() {}
            self.reset_highlighter_with_text(self.editor.get_full_text(), false);
            self.wait_for_current_highlight();
            self.clear_ctrl_definition();
            crate::app::mouse::HOVER_STATE.with(|state| {
                *state.borrow_mut() = crate::app::mouse::HoverState::default();
            });
        } else {
            self.tabs.remove(idx);
            if idx < self.active_tab {
                self.active_tab -= 1;
            }
        }

        self.close_autocomplete();
        self.show_welcome =
            self.tabs.len() <= 1 && self.file_path.is_none() && self.editor.len() == 0;

        if let Some(w) = self.window.as_ref() {
            App::update_window_title(w, &self.base_title, self.editor.is_dirty());
            w.request_redraw();
        }
        self.save_tabs_state();
    }

    pub fn open_file_in_tab(&mut self, path: PathBuf, add_to_history: bool) {
        self.open_file_in_tab_internal(path, add_to_history, true);
    }

    pub fn open_file_in_tab_bg(&mut self, path: PathBuf, add_to_history: bool) {
        self.open_file_in_tab_internal(path, add_to_history, false);
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn open_file_in_tab_internal(
        &mut self,
        path: PathBuf,
        add_to_history: bool,
        wait_highlight: bool,
    ) {
        if !self.is_ide_mode {
            self.load_file_internal(path, add_to_history, wait_highlight);
            return;
        }

        let mut matching_tab = None;
        for (i, tab) in self.tabs.iter().enumerate() {
            if i == self.active_tab {
                if self.file_path.as_ref() == Some(&path) {
                    matching_tab = Some(i);
                    break;
                }
            } else {
                if tab.file_path.as_ref() == Some(&path) {
                    matching_tab = Some(i);
                    break;
                }
            }
        }

        if let Some(i) = matching_tab {
            if i == self.active_tab {
                self.reveal_active_tab_now();
            } else if wait_highlight {
                self.switch_to_tab(i);
            }
            return;
        }

        if self.tabs.is_empty()
            || self.file_path.is_some()
            || self.editor.is_dirty()
            || self.editor.len() > 0
        {
            self.open_new_tab();
        }

        self.load_file_internal(path, add_to_history, wait_highlight);
        self.reveal_active_tab_now();
    }
    pub fn ensure_cursor_visible(
        target_scroll_y: &mut f32,
        target_scroll_x: &mut f32,
        editor: &Editor,
        renderer: &mut Renderer,
        window_width: f32,
        window_height: f32,
        tab_bar_h: f32,
    ) {
        let window_height = window_height - tab_bar_h;
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

    fn abs_path_for_workspace(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else if let Some(ws) = self.ide_workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        }
    }

    fn current_abs_path(&self) -> Option<PathBuf> {
        self.file_path
            .as_ref()
            .map(|path| self.abs_path_for_workspace(path))
    }

    pub(crate) fn ctrl_definition_highlight_range(&self) -> Option<(usize, usize)> {
        if self.modifiers.control_key() && self.ctrl_definition.target.is_some() {
            self.ctrl_definition.source_range
        } else {
            None
        }
    }

    pub(crate) fn clear_ctrl_definition(&mut self) {
        self.ctrl_definition = CtrlDefinitionState::default();
    }

    pub(crate) fn update_ctrl_definition_hover(&mut self, byte_offset: Option<usize>) {
        if !self.modifiers.control_key()
            || !matches!(self.file_extension.as_str(), "py" | "pyi")
            || !self.is_ide_mode
        {
            self.clear_ctrl_definition();
            return;
        }

        let Some(byte_offset) = byte_offset else {
            self.clear_ctrl_definition();
            return;
        };
        let Some(source_path) = self.current_abs_path() else {
            self.clear_ctrl_definition();
            return;
        };
        let source_range = crate::app::mouse::hover_token_bounds(&self.editor, byte_offset);
        if self.ctrl_definition.source_path.as_ref() == Some(&source_path)
            && self.ctrl_definition.source_range == Some(source_range)
        {
            return;
        }

        self.ctrl_definition = CtrlDefinitionState {
            request_id: None,
            source_path: Some(source_path.clone()),
            source_range: Some(source_range),
            target: self.nearest_assignment_usage_target(source_range),
        };

        if self.ctrl_definition.target.is_some() {
            return;
        }

        let Some(path) = self.file_path.clone() else {
            return;
        };
        let (line, col) = crate::lsp::offset_to_lsp_pos(
            &self.editor.get_full_text(),
            byte_offset,
            &self.editor.line_offsets,
        );
        self.ctrl_definition.request_id = self
            .lsp
            .as_mut()
            .and_then(|lsp| lsp.request_definition(&path, &self.file_extension, line, col));
    }

    pub(crate) fn ctrl_definition_target_from_lsp(
        &self,
        target: Option<DefinitionJumpTarget>,
    ) -> Option<DefinitionJumpTarget> {
        let target = target?;
        let source_path = self.ctrl_definition.source_path.as_ref()?;
        let source_range = self.ctrl_definition.source_range?;
        if self.abs_path_for_workspace(&target.path) == *source_path {
            let text = self.editor.get_full_text();
            let target_offset = crate::lsp::lsp_pos_to_offset(&text, target.line, target.col);
            if target_offset >= source_range.0 && target_offset <= source_range.1 {
                return self.nearest_assignment_usage_target(source_range);
            }
        }
        Some(target)
    }

    pub(crate) fn ctrl_definition_target_under_mouse(&mut self) -> Option<DefinitionJumpTarget> {
        let target = self.ctrl_definition.target.clone()?;
        let source_range = self.ctrl_definition.source_range?;
        let r = self.renderer.as_mut()?;
        let tab_bar_h = if self.show_welcome || !self.is_ide_mode {
            0.0
        } else {
            38.0 * r.scale_factor
        };
        let mouse_x = r.last_mouse_x;
        let mouse_y = r.last_mouse_y + self.scroll_y.current.round() - tab_bar_h;
        let byte = r.get_byte_at_xy(&self.editor, mouse_x, mouse_y);
        let normalized = crate::app::mouse::normalize_hover_byte(&self.editor, byte)?;
        (crate::app::mouse::hover_token_bounds(&self.editor, normalized) == source_range)
            .then_some(target)
    }

    pub(crate) fn jump_to_definition_target(&mut self, target: DefinitionJumpTarget) {
        self.open_file_in_tab(target.path.clone(), true);
        let text = self.editor.get_full_text();
        let offset = crate::lsp::lsp_pos_to_offset(&text, target.line, target.col);
        self.editor.cursor = offset;
        self.editor.selection_anchor = None;
        self.reprioritize_highlighter_around_cursor();
        self.clear_ctrl_definition();

        if let Some(r) = self.renderer.as_mut() {
            let wh = self
                .window
                .as_ref()
                .map(|w| w.inner_size().height as f32)
                .unwrap_or(r.height);
            let tab_bar_h = if self.show_welcome || !self.is_ide_mode {
                0.0
            } else {
                38.0 * r.scale_factor
            };
            let line = (target.line as usize).min(self.editor.line_offsets.len().saturating_sub(1));
            let line_top_y = line as f32 * r.line_height;
            let panel_bottom_h = if self.is_ide_mode && self.ide_panel.any_bottom_open() {
                self.ide_panel.bottom_height * r.scale_factor
            } else {
                0.0
            };
            let visible_h = crate::render_view::editor_view_height(
                wh,
                tab_bar_h,
                panel_bottom_h,
                self.is_ide_mode,
                r.scale_factor,
            )
            .max(r.line_height);
            let max_scroll = r.get_max_scroll(&self.editor, visible_h);
            self.scroll_y.target = (line_top_y - visible_h * 0.45)
                .max(0.0)
                .min(max_scroll)
                .round();
            self.scroll_y.anim_speed = 15.0;
            self.scroll_x.target = 0.0;
        }

        if let Some(w) = self.window.as_ref() {
            w.request_redraw();
        }
    }

    pub fn update_terminal_search(&mut self) {
        self.ide_panel.term_search_results.clear();
        self.ide_panel.term_search_current_idx = None;
        let query_text = self.ide_panel.term_search_editor.get_full_text();
        if query_text.is_empty() {
            return;
        }
        let escaped_query = regex::escape(&query_text);
        if let Ok(re) = regex::RegexBuilder::new(&escaped_query)
            .case_insensitive(!self.ide_panel.term_search_case_sensitive)
            .build()
        {
            if let Some(term) = self.ide_panel.terminals.get(self.ide_panel.active_terminal) {
                let grid = term.grid.lock().unwrap();
                let total_lines = if grid.is_alt {
                    grid.lines.len()
                } else {
                    grid.scrollback.len() + grid.lines.len()
                };
                for y in 0..total_lines {
                    let row = if grid.is_alt {
                        &grid.lines[y]
                    } else if y < grid.scrollback.len() {
                        &grid.scrollback[y]
                    } else {
                        &grid.lines[y - grid.scrollback.len()]
                    };
                    let line_str: String = row.iter().map(|c| c.c).collect();
                    for mat in re.find_iter(&line_str) {
                        self.ide_panel.term_search_results.push((
                            mat.start(),
                            y,
                            mat.end().saturating_sub(1),
                            y,
                        ));
                    }
                }
            }
        }
        if !self.ide_panel.term_search_results.is_empty() {
            self.ide_panel.term_search_current_idx =
                Some(self.ide_panel.term_search_results.len() - 1);
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn jump_to_terminal_search_result(&mut self) {
        if let Some(idx) = self.ide_panel.term_search_current_idx {
            if let Some(&(sx, sy, ex, ey)) = self.ide_panel.term_search_results.get(idx) {
                if let Some(term) = self
                    .ide_panel
                    .terminals
                    .get_mut(self.ide_panel.active_terminal)
                {
                    let mut grid = term.grid.lock().unwrap();
                    grid.selection = Some((sx, sy, ex, ey));
                    if let Some(r) = self.renderer.as_ref() {
                        let s = r.scale_factor;
                        let char_h = r.line_height * 1.05;
                        let total_lines = if grid.is_alt {
                            grid.lines.len()
                        } else {
                            grid.scrollback.len() + grid.lines.len()
                        };
                        let offset_from_bottom = total_lines.saturating_sub(1).saturating_sub(sy);

                        let bottom_h = self.ide_panel.bottom_height * s;
                        let term_content_h = bottom_h - 1.0 * s - 32.0 * s - 32.0 * s;
                        let max_scroll = if grid.is_alt {
                            0.0
                        } else {
                            ((total_lines as f32 * char_h) - term_content_h).max(0.0)
                        };

                        term.scroll_y.target =
                            (offset_from_bottom as f32 * char_h).clamp(0.0, max_scroll);
                    }
                }
            }
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

        let escaped_query = regex::escape(&query_text);
        let full_text = self.editor.get_full_text();
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

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn jump_to_search_result(&mut self) {
        if let Some(idx) = self.search_current_idx {
            if let Some(&(start, end)) = self.search_results.get(idx) {
                self.editor.cursor = end;
                self.editor.selection_anchor = Some(start);
                self.reprioritize_highlighter_around_cursor();
                if let Some(r) = self.renderer.as_mut() {
                    let phys_line = self
                        .editor
                        .line_offsets
                        .partition_point(|&o| o <= end)
                        .saturating_sub(1);

                    let line_top_y = phys_line as f32 * r.line_height;

                    let wh = self.window.as_ref().unwrap().inner_size().height as f32;
                    let s = r.scale_factor;
                    let tab_bar_h = if self.show_welcome || !self.is_ide_mode {
                        0.0
                    } else {
                        38.0 * s
                    };
                    let panel_bottom_h = if self.is_ide_mode && self.ide_panel.any_bottom_open() {
                        self.ide_panel.bottom_height * s
                    } else {
                        0.0
                    };
                    let visible_h = crate::render_view::editor_view_height(
                        wh,
                        tab_bar_h,
                        panel_bottom_h,
                        self.is_ide_mode,
                        s,
                    );
                    self.scroll_y.target = (line_top_y - visible_h / 2.0).max(0.0);

                    let max_s = r.get_max_scroll(&self.editor, visible_h);
                    self.scroll_y.clamp_target(0.0, max_s);
                    self.scroll_y.target = self.scroll_y.target.round();
                    self.scroll_y.anim_speed = 10.0;
                }
            }
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn update_window_title(window: &Window, base_title: &str, is_dirty: bool) {
        let title = if is_dirty {
            format!("{} * — RRiter", base_title)
        } else {
            format!("{} — RRiter", base_title)
        };
        window.set_title(&title);
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
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
            self.dialog_window = Some(std::sync::Arc::new(window));
            self.dialog_gl_surface = Some(surface);
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn close_dialog(&mut self) {
        self.dialog_window = None;
        self.dialog_gl_surface = None;
        if let Some(w) = self.window.as_ref() {
            w.request_redraw();
        }
    }

    pub fn close_current_file(&mut self) {
        if self.is_ide_mode && self.tabs.len() > 1 {
            self.close_tab_at(self.active_tab);
            return;
        }

        let path_to_close = self.file_path.take();
        self.base_title = "Добро пожаловать".to_string();
        let old_version = self.editor.version;
        self.editor = Editor::new(8192);
        self.editor.version = old_version + 1;
        self.editor.set_original_text();
        self.editor.sync_edits.clear();
        while let Ok(_) = self.highlighter.rx.try_recv() {}
        self.highlighter
            .reset(self.editor.version, "".to_string(), "".to_string(), 0);
        self.search_results.clear();
        self.search_current_idx = None;
        self.show_search = false;
        self.autocomplete_active = false;
        self.show_welcome = true;

        self.file_extension = String::new();

        if self.is_ide_mode {
            if let Some(lsp) = &mut self.lsp {
                let ext = self.file_extension.clone();
                if let Some(path) = path_to_close {
                    lsp.notify_close(&path, &ext);
                }
            }
            self.tabs.clear();
        }

        self.scroll_y.current = 0.0;
        self.scroll_y.target = 0.0;
        self.scroll_x.current = 0.0;
        self.scroll_x.target = 0.0;

        if let Some(w) = self.window.as_ref() {
            App::update_window_title(w, &self.base_title, false);
            w.request_redraw();
        }
        self.save_tabs_state();
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn trigger_file_picker(&mut self) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.open_file_rx = Some(rx);
        std::thread::spawn(move || {
            let file = rfd::FileDialog::new().set_title("Открыть файл").pick_file();
            let _ = tx.send(file);
        });
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
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

    #[cfg_attr(coverage_nightly, coverage(off))]
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
        if self.active_tab_is_git_diff() {
            return self.save_active_git_diff();
        }
        if let Some(path) = self.file_path.clone() {
            let content = self.editor.get_full_text();
            match std::fs::write(&path, &content) {
                Ok(_) => {
                    self.editor.mark_saved();
                    self.save_tabs_state();
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
                                self.save_tabs_state();
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

    /// Применяет последние результаты подсветки (foldable ranges) к состоянию редактора.
    /// Вызывать после `highlighter.poll()` или `highlighter.wait_for_first_result()`.
    pub fn apply_highlight_results(&mut self) {
        let ext = self.file_extension.as_str();
        let threshold = match ext {
            "json" | "toml" | "yaml" | "yml" | "html" | "css" | "xml" | "md" | "txt" => 20,
            "py" | "pyi" | "rs" | "dart" => 1,
            _ => 2,
        };
        let should_autofold_initial =
            !self.is_highlighted_once && self.editor.folded_start_bytes.is_empty();
        self.editor.foldable_lines.clear();
        self.editor.foldable_ranges_bytes.clear();
        for &(start_b, end_b, is_autofold, is_sticky) in &self.highlighter.foldable_ranges {
            self.editor
                .foldable_ranges_bytes
                .push((start_b, end_b, is_sticky));
            let sl = self
                .editor
                .line_offsets
                .partition_point(|&x| x <= start_b)
                .saturating_sub(1);
            let el = self
                .editor
                .line_offsets
                .partition_point(|&x| x <= end_b)
                .saturating_sub(1);
            if el > sl {
                self.editor.foldable_lines.insert(sl, el);
                if is_autofold && el - sl >= threshold && should_autofold_initial {
                    self.editor.folded_lines.insert(sl);
                    self.editor
                        .folded_start_bytes
                        .insert(self.editor.line_offsets[sl]);
                }
            }
        }
        self.is_highlighted_once = true;
    }

    fn wait_for_current_highlight(&mut self) {
        if self.highlighter.current_version == self.editor.version {
            return;
        }
        let is_large = self.editor.len() > FILE_OPEN_BLOCKING_HIGHLIGHT_MAX_BYTES;
        let is_priority_lang = matches!(self.file_extension.as_str(), "py" | "pyi" | "rs");
        if is_large && !is_priority_lang {
            return;
        }
        let timeout = if is_large {
            FILE_OPEN_LARGE_PRIORITY_HIGHLIGHT_TIMEOUT
        } else {
            FILE_OPEN_HIGHLIGHT_TIMEOUT
        };
        if self
            .highlighter
            .wait_for_first_result(self.editor.version, timeout)
        {
            self.apply_highlight_results();
        }
    }

    fn reset_highlighter_with_text(&mut self, text: String, _seed_immediately: bool) {
        self.highlighter.spans.clear();
        self.highlighter.completions.clear();
        self.highlighter.foldable_ranges.clear();
        self.highlighter.syntax_errors.clear();
        let version = self.editor.version;
        let ext = self.file_extension.clone();
        let priority_anchor = self.editor.cursor.min(text.len());
        self.highlighter.reset(version, text, ext, priority_anchor);
    }

    pub(crate) fn reprioritize_highlighter_around_cursor(&mut self) {
        while let Ok(_) = self.highlighter.rx.try_recv() {}
        self.reset_highlighter_with_text(self.editor.get_full_text(), false);
        self.is_highlighted_once = false;
    }

    pub fn load_file_internal(
        &mut self,
        path: PathBuf,
        add_to_history: bool,
        wait_highlight: bool,
    ) {
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                self.show_welcome = false;
                if add_to_history {
                    self.add_recent_file(path.clone());
                }

                let old_version = self.editor.version;
                self.editor = Editor::new(content.len() + 8192);
                self.editor.version = old_version + 1;
                self.editor.set_clean_text(&content);
                self.file_path = Some(path.clone());
                let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                self.base_title = file_name.into_owned();
                self.file_extension = path
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_default();
                self.is_highlighted_once = false;
                while let Ok(_) = self.highlighter.rx.try_recv() {}
                self.reset_highlighter_with_text(content.clone(), !wait_highlight);
                apply_initial_import_folds(&mut self.editor, &self.file_extension, &content);

                // Ждём до 150мс: малые файлы полностью, большие py/rs до первого priority chunk.
                if wait_highlight {
                    self.wait_for_current_highlight();
                }

                self.scroll_y.current = 0.0;
                self.scroll_y.target = 0.0;
                self.scroll_x.current = 0.0;
                self.scroll_x.target = 0.0;

                self.last_sent_version = u64::MAX;
                self.search_results.clear();
                self.search_current_idx = None;
                self.autocomplete_active = false;
                if self.is_ide_mode {
                    if let Some(lsp) = &mut self.lsp {
                        lsp.notify_open(
                            &path,
                            &self.file_extension,
                            &content,
                            self.editor.version as i32,
                        );
                    }
                }
                if let Some(w) = self.window.as_ref() {
                    App::update_window_title(w, &self.base_title, false);
                    w.request_redraw();
                }
                self.save_tabs_state();
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

    #[cfg(test)]
    pub fn check_external_changes(&mut self) {
        self.sync_active_tab();
        let mut needs_redraw = false;
        let mut active_reloaded = false;
        let active_idx = self.active_tab;
        let mut diff_reloads = Vec::new();
        for (idx, tab) in self.tabs.iter_mut().enumerate() {
            if !tab.editor.is_dirty() {
                let diff_path = match &tab.kind {
                    EditorTabKind::GitDiff(meta, _) => Some(meta.repo_root.join(&meta.rel_path)),
                    EditorTabKind::Normal => None,
                };
                if let Some(path) = tab.file_path.as_ref().or(diff_path.as_ref()) {
                    if let Ok(disk_text) = std::fs::read_to_string(path) {
                        if let EditorTabKind::GitDiff(_, state) = &tab.kind {
                            if disk_text != state.worktree_text {
                                diff_reloads.push(idx);
                                needs_redraw = true;
                            }
                            continue;
                        }
                        if disk_text != tab.editor.get_full_text() {
                            let old_version = tab.editor.version;
                            tab.editor = crate::editor::Editor::new(disk_text.len() + 8192);
                            tab.editor.version = old_version + 1;
                            let _ = tab.editor.insert_str(&disk_text);
                            tab.editor.cursor = 0;
                            tab.editor.clear_history();
                            tab.editor.set_original_text();
                            tab.editor.sync_edits.clear();
                            tab.completions.clear();
                            tab.foldable_ranges.clear();
                            tab.is_highlighted_once = false;
                            if self.is_ide_mode {
                                if let Some(lsp) = &mut self.lsp {
                                    lsp.clear_diagnostics_for_path(path);
                                    lsp.notify_change(
                                        path,
                                        &tab.file_extension,
                                        &disk_text,
                                        tab.editor.version as i32,
                                    );
                                }
                            }
                            if idx == active_idx {
                                active_reloaded = true;
                            }
                            needs_redraw = true;
                        }
                    }
                }
            }
        }
        for idx in diff_reloads {
            self.reload_git_diff_tab(idx);
        }
        self.sync_active_tab();
        if active_reloaded {
            while let Ok(_) = self.highlighter.rx.try_recv() {}
            self.reset_highlighter_with_text(self.editor.get_full_text(), false);
            self.wait_for_current_highlight();
            crate::app::mouse::clear_hover_popup(self.renderer.as_mut());
            self.lsp_actions_menu = None;
            self.last_sent_version = self.editor.version;
        }
        if needs_redraw {
            if let Some(w) = self.window.as_ref() {
                w.request_redraw();
            }
        }
    }

    pub fn start_external_changes_check(&mut self) {
        if self.external_changes_rx.is_some() {
            return;
        }
        self.sync_active_tab();
        let clean_tabs = self
            .tabs
            .iter()
            .enumerate()
            .filter_map(|(idx, tab)| {
                if tab.editor.is_dirty() {
                    return None;
                }
                match &tab.kind {
                    EditorTabKind::GitDiff(meta, _) => {
                        Some((idx, meta.repo_root.join(&meta.rel_path)))
                    }
                    EditorTabKind::Normal => tab.file_path.clone().map(|path| (idx, path)),
                }
            })
            .collect::<Vec<_>>();
        self.sync_active_tab();
        if clean_tabs.is_empty() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut changes = Vec::new();
            for (tab_idx, path) in clean_tabs {
                if let Ok(disk_text) = std::fs::read_to_string(&path) {
                    changes.push(crate::app::ExternalFileChange {
                        tab_idx,
                        path,
                        disk_text,
                    });
                }
            }
            let _ = tx.send(changes);
        });
        self.external_changes_rx = Some(rx);
    }

    pub fn poll_external_changes(&mut self) -> bool {
        let Some(rx) = &self.external_changes_rx else {
            return false;
        };
        let changes = match rx.try_recv() {
            Ok(changes) => changes,
            Err(std::sync::mpsc::TryRecvError::Empty) => return false,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.external_changes_rx = None;
                return false;
            }
        };
        self.external_changes_rx = None;
        if changes.is_empty() {
            return false;
        }

        self.sync_active_tab();
        let mut needs_redraw = false;
        let mut active_reloaded = false;
        let active_idx = self.active_tab;
        let mut diff_reloads = Vec::new();
        for change in changes {
            let Some(tab) = self.tabs.get_mut(change.tab_idx) else {
                continue;
            };
            if let EditorTabKind::GitDiff(meta, state) = &tab.kind {
                if tab.editor.is_dirty() || meta.repo_root.join(&meta.rel_path) != change.path {
                    continue;
                }
                if change.disk_text != state.worktree_text {
                    diff_reloads.push(change.tab_idx);
                    needs_redraw = true;
                }
                continue;
            }
            if tab.file_path.as_ref() != Some(&change.path) || tab.editor.is_dirty() {
                continue;
            }
            if change.disk_text == tab.editor.get_full_text() {
                continue;
            }
            let old_version = tab.editor.version;
            tab.editor = crate::editor::Editor::new(change.disk_text.len() + 8192);
            tab.editor.version = old_version + 1;
            let _ = tab.editor.insert_str(&change.disk_text);
            tab.editor.cursor = 0;
            tab.editor.clear_history();
            tab.editor.set_original_text();
            tab.editor.sync_edits.clear();
            tab.completions.clear();
            tab.foldable_ranges.clear();
            tab.is_highlighted_once = false;
            if self.is_ide_mode
                && let Some(lsp) = &mut self.lsp
            {
                lsp.clear_diagnostics_for_path(&change.path);
                lsp.notify_change(
                    &change.path,
                    &tab.file_extension,
                    &change.disk_text,
                    tab.editor.version as i32,
                );
            }
            if change.tab_idx == active_idx {
                active_reloaded = true;
            }
            needs_redraw = true;
        }
        for idx in diff_reloads {
            self.reload_git_diff_tab(idx);
        }
        self.sync_active_tab();
        if active_reloaded {
            while self.highlighter.rx.try_recv().is_ok() {}
            self.reset_highlighter_with_text(self.editor.get_full_text(), false);
            crate::app::mouse::clear_hover_popup(self.renderer.as_mut());
            self.lsp_actions_menu = None;
            self.last_sent_version = self.editor.version;
        }
        if needs_redraw && let Some(w) = self.window.as_ref() {
            w.request_redraw();
        }
        needs_redraw
    }
}

#[cfg(test)]
mod app_behavior_tests;
