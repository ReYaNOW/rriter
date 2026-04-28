mod app_state;
pub mod events;
pub mod file_icons;
pub mod file_tree;
pub mod keyboard;
pub mod lsp_actions;
pub mod mouse;
pub mod terminal;
pub mod ui_handlers;
use crate::editor::Editor;
use crate::highlighter::{CompletionItem, SymbolKind};
use crate::renderer::Renderer;
use app_state::fuzzy_match;
pub use app_state::*;
use glutin::display::GetGlDisplay;
use rustc_hash::FxHashMap;
use std::path::{Path, PathBuf};
use winit::event_loop::ActiveEventLoop;
use winit::platform::wayland::WindowAttributesExtWayland;
use winit::window::Window;

fn is_python_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80
}

fn is_plain_assignment_after_token(after_token: &str) -> bool {
    let bytes = after_token.as_bytes();
    for (idx, &b) in bytes.iter().enumerate() {
        if b != b'=' {
            continue;
        }
        let prev = idx.checked_sub(1).and_then(|i| bytes.get(i)).copied();
        let next = bytes.get(idx + 1).copied();
        if matches!(prev, Some(b'=' | b'!' | b'<' | b'>' | b':')) || next == Some(b'=') {
            continue;
        }
        return true;
    }
    false
}

fn plain_assignment_index(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    for (idx, &b) in bytes.iter().enumerate() {
        if b != b'=' {
            continue;
        }
        let prev = idx.checked_sub(1).and_then(|i| bytes.get(i)).copied();
        let next = bytes.get(idx + 1).copied();
        if matches!(prev, Some(b'=' | b'!' | b'<' | b'>' | b':')) || next == Some(b'=') {
            continue;
        }
        return Some(idx);
    }
    None
}

fn token_occurrence_at_word_boundary(
    text: &str,
    token: &str,
    search_start: usize,
) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut cursor = search_start.min(text.len());
    while cursor < text.len() {
        let rel = text.get(cursor..)?.find(token)?;
        let start = cursor + rel;
        let end = start + token.len();
        let left_ok = start == 0 || !is_python_ident_byte(bytes[start - 1]);
        let right_ok = end >= bytes.len() || !is_python_ident_byte(bytes[end]);
        if left_ok && right_ok {
            return Some(start);
        }
        cursor = end;
    }
    None
}

fn previous_token_occurrence_at_word_boundary(
    text: &str,
    token: &str,
    search_end: usize,
) -> Option<usize> {
    let mut best = None;
    let mut cursor = 0;
    while let Some(pos) = token_occurrence_at_word_boundary(text, token, cursor) {
        if pos >= search_end {
            break;
        }
        best = Some(pos);
        cursor = pos + token.len();
    }
    best
}

fn nearest_python_assignment_usage(editor: &Editor, source_range: (usize, usize)) -> Option<usize> {
    let text = editor.get_full_text();
    let (start, end) = source_range;
    let token = text.get(start..end)?;
    if token.is_empty() {
        return None;
    }

    let bytes = text.as_bytes();
    let line_start = bytes[..start.min(bytes.len())]
        .iter()
        .rposition(|&b| b == b'\n')
        .map(|pos| pos + 1)
        .unwrap_or(0);
    let line_end = bytes[end.min(bytes.len())..]
        .iter()
        .position(|&b| b == b'\n')
        .map(|pos| end + pos)
        .unwrap_or(bytes.len());

    let before = text.get(line_start..start)?.trim_start();
    if before.starts_with("def ") || before.starts_with("class ") {
        return None;
    }
    let line = text.get(line_start..line_end)?;
    let Some(eq_idx) = plain_assignment_index(line) else {
        return None;
    };
    let target_end = line[..eq_idx].find(':').unwrap_or(eq_idx);
    let source_start_in_line = start.saturating_sub(line_start);
    let source_end_in_line = end.saturating_sub(line_start);
    if source_start_in_line >= target_end || source_end_in_line > target_end {
        return None;
    }
    if !text
        .get(end..line_end)
        .is_some_and(is_plain_assignment_after_token)
    {
        return None;
    }

    token_occurrence_at_word_boundary(&text, token, line_end)
        .or_else(|| previous_token_occurrence_at_word_boundary(&text, token, line_start))
}

fn cursor_line_bounds(text: &str, cursor: usize) -> (usize, usize) {
    let cursor = cursor.min(text.len());
    let start = text.as_bytes()[..cursor]
        .iter()
        .rposition(|&b| b == b'\n')
        .map(|pos| pos + 1)
        .unwrap_or(0);
    let end = text.as_bytes()[cursor..]
        .iter()
        .position(|&b| b == b'\n')
        .map(|pos| cursor + pos)
        .unwrap_or(text.len());
    (start, end)
}

fn cursor_in_python_string_or_comment(line_prefix: &str) -> bool {
    let mut single = false;
    let mut double = false;
    let mut escaped = false;
    for b in line_prefix.bytes() {
        if escaped {
            escaped = false;
            continue;
        }
        if b == b'\\' {
            escaped = true;
            continue;
        }
        if !single && !double && b == b'#' {
            return true;
        }
        if !double && b == b'\'' {
            single = !single;
        } else if !single && b == b'"' {
            double = !double;
        }
    }
    single || double
}

fn python_import_completion_allowed(editor: &Editor) -> bool {
    let text = editor.get_full_text();
    let cursor = editor.cursor.min(text.len());
    let (line_start, line_end) = cursor_line_bounds(&text, cursor);
    let line = text.get(line_start..line_end).unwrap_or("");
    let prefix = text.get(line_start..cursor).unwrap_or("");
    if cursor_in_python_string_or_comment(prefix) {
        return false;
    }
    let trimmed = line.trim_start();
    if trimmed.starts_with("def ")
        || trimmed.starts_with("async ")
        || trimmed.starts_with("class ")
        || trimmed.starts_with("from ")
        || trimmed.starts_with("import ")
    {
        return false;
    }
    let bytes = text.as_bytes();
    let prev_ident = cursor
        .checked_sub(1)
        .and_then(|idx| bytes.get(idx))
        .is_some_and(|&b| is_python_ident_byte(b));
    let next_ident = bytes
        .get(cursor)
        .is_some_and(|&b| is_python_ident_byte(b));
    !prev_ident && !next_ident
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

        if self.ide_panel.is_open(PanelId::Terminal) && self.ide_panel.terminals.is_empty() {
            self.ide_panel
                .terminals
                .push(crate::app::terminal::Terminal::spawn(self.window.clone()));
            self.ide_panel.active_terminal = 0;
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
                if self.highlighter.wait_for_first_result(
                    self.editor.version,
                    std::time::Duration::from_millis(50),
                ) {
                    self.apply_highlight_results();
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

        let highest = self
            .tabs
            .iter()
            .map(|t| t.editor.version)
            .max()
            .unwrap_or(0)
            .max(self.editor.version);
        self.editor.version = highest + 1;

        while let Ok(_) = self.highlighter.rx.try_recv() {}
        self.highlighter.reset(
            self.editor.version,
            self.editor.get_full_text(),
            self.file_extension.clone(),
        );

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
            });
            self.active_tab = 0;
            self.show_welcome = false;
            while let Ok(_) = self.highlighter.rx.try_recv() {}
            self.highlighter
                .reset(self.editor.version, String::new(), String::new());
            self.autocomplete_active = false;
            if let Some(w) = self.window.as_ref() {
                App::update_window_title(w, &self.base_title, false);
                w.request_redraw();
            }
            self.save_tabs_state();
            return;
        }

        self.sync_active_tab();
        let highest = self
            .tabs
            .iter()
            .map(|t| t.editor.version)
            .max()
            .unwrap_or(0)
            .max(self.editor.version);
        let mut new_editor = crate::editor::Editor::new(8192);
        new_editor.version = highest + 1;
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
        };
        self.tabs.push(new_tab);
        self.active_tab = self.tabs.len() - 1;
        self.sync_active_tab();
        while let Ok(_) = self.highlighter.rx.try_recv() {}
        self.highlighter
            .reset(self.editor.version, String::new(), String::new());

        self.autocomplete_active = false;
        self.show_welcome = false;

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
        } else {
            self.tabs.remove(idx);
            if idx < self.active_tab {
                self.active_tab -= 1;
            }
        }

        self.autocomplete_active = false;
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

        for (i, tab) in self.tabs.iter().enumerate() {
            if i == self.active_tab {
                if self.file_path.as_ref() == Some(&path) {
                    return;
                }
            } else {
                if tab.file_path.as_ref() == Some(&path) {
                    if wait_highlight {
                        self.switch_to_tab(i);
                    }
                    return;
                }
            }
        }

        if self.tabs.is_empty()
            || self.file_path.is_some()
            || self.editor.is_dirty()
            || self.editor.len() > 0
        {
            self.open_new_tab();
        }

        self.load_file_internal(path, add_to_history, wait_highlight);
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
            let visible_h = (wh - tab_bar_h).max(r.line_height);
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

    fn nearest_assignment_usage_target(
        &self,
        source_range: (usize, usize),
    ) -> Option<DefinitionJumpTarget> {
        let usage = nearest_python_assignment_usage(&self.editor, source_range)?;
        let (line, col) = crate::lsp::offset_to_lsp_pos(
            &self.editor.get_full_text(),
            usage,
            &self.editor.line_offsets,
        );
        Some(DefinitionJumpTarget {
            path: self.current_abs_path()?,
            line,
            col,
        })
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

    pub fn request_ty_autocomplete(
        &mut self,
        mode: AutocompleteMode,
        trigger: Option<&str>,
    ) {
        if !self.is_ide_mode || self.show_welcome {
            return;
        }
        if mode == AutocompleteMode::TyImports && self.get_current_word_prefix().is_empty() {
            self.autocomplete_mode = mode;
            self.autocomplete_active = true;
            self.autocomplete_options.clear();
            self.autocomplete_selected_idx = 0;
            self.autocomplete_pending_request_id = None;
            self.autocomplete_anim_progress = 0.0;
            self.autocomplete_scroll.current = 0.0;
            self.autocomplete_scroll.target = 0.0;
            return;
        }
        let Some(path) = self.file_path.clone() else {
            return;
        };
        let Some(lsp) = self.lsp.as_mut() else {
            return;
        };
        let text = self.editor.get_full_text();
        lsp.notify_change(&path, &self.file_extension, &text, self.editor.version as i32);
        let (line, col) =
            crate::lsp::offset_to_lsp_pos(&text, self.editor.cursor, &self.editor.line_offsets);
        if let Some(id) = lsp.request_ty_completion(&path, &self.file_extension, line, col, trigger)
        {
            self.autocomplete_mode = mode;
            self.autocomplete_pending_request_id = Some(id);
            if !self.autocomplete_active {
                self.autocomplete_anim_progress = 0.0;
                self.autocomplete_scroll.current = 0.0;
                self.autocomplete_scroll.target = 0.0;
            }
            self.autocomplete_active = true;
        }
    }

    pub fn update_ty_autocomplete(&mut self, items: Vec<crate::lsp::LspCompletionItem>) {
        let prefix = self.get_current_word_prefix();
        if self.autocomplete_mode == AutocompleteMode::TyImports && prefix.is_empty() {
            self.autocomplete_options.clear();
            self.autocomplete_active = true;
            return;
        }

        let prefix_lower = prefix.to_lowercase();
        let mut seen = FxHashMap::default();
        let mut matches = Vec::new();

        for item in items {
            let item: AutocompleteItem = item.into();
            if self.autocomplete_mode == AutocompleteMode::TyImports && item.module.is_none() {
                continue;
            }
            let key = (item.word.clone(), item.module.clone().unwrap_or_default());
            if seen.insert(key, ()).is_some() {
                continue;
            }
            let word_lower = item.word.to_lowercase();
            let indices = if prefix.is_empty() {
                Vec::new()
            } else if let Some(indices) = fuzzy_match(&prefix_lower, &word_lower) {
                indices
            } else {
                continue;
            };
            let is_prefix = prefix.is_empty() || word_lower.starts_with(&prefix_lower);
            matches.push((is_prefix, item.word.len(), item, indices));
        }

        matches.sort_unstable_by_key(|(is_prefix, len, item, _)| {
            let type_priority = match item.kind {
                SymbolKind::Variable | SymbolKind::Parameter => 0,
                SymbolKind::Function => 1,
                SymbolKind::Class => 2,
                SymbolKind::Keyword => 3,
                SymbolKind::Unknown => 4,
            };
            (!*is_prefix, type_priority, *len)
        });

        self.autocomplete_options = matches
            .into_iter()
            .take(80)
            .map(|(_, _, item, indices)| (item, indices))
            .collect();
        self.autocomplete_active =
            !self.autocomplete_options.is_empty() || self.autocomplete_mode == AutocompleteMode::TyImports;
        self.autocomplete_selected_idx = 0;
        self.autocomplete_hovered_idx = None;
        self.autocomplete_scroll.current = 0.0;
        self.autocomplete_scroll.target = 0.0;
    }

    pub fn update_autocomplete(&mut self) {
        let prefix = self.get_current_word_prefix();
        if prefix.is_empty() {
            self.autocomplete_active = false;
            self.autocomplete_options.clear();
            self.autocomplete_mode = AutocompleteMode::TreeSitter;
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

        self.autocomplete_options = matches
            .into_iter()
            .take(60)
            .map(|m| (m.2.into(), m.3))
            .collect();

        if !self.autocomplete_options.is_empty() {
            if !self.autocomplete_active {
                self.autocomplete_anim_progress = 0.0;
                self.autocomplete_scroll.current = 0.0;
                self.autocomplete_scroll.target = 0.0;
            }
            self.autocomplete_mode = AutocompleteMode::TreeSitter;
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
        let selected_item = self.autocomplete_options[self.autocomplete_selected_idx]
            .0
            .clone();
        if selected_item.text_edit.is_some() || !selected_item.additional_text_edits.is_empty() {
            self.apply_lsp_completion_item(&selected_item);
            self.autocomplete_active = false;
            self.autocomplete_selected_idx = 0;
            self.autocomplete_scroll.current = 0.0;
            self.autocomplete_scroll.target = 0.0;
            return;
        }
        let selected = selected_item
            .insert_text
            .clone()
            .unwrap_or(selected_item.word.clone());
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
        self.sync_after_autocomplete();

        if let Some(w) = self.window.as_ref() {
            App::update_window_title(w, &self.base_title, self.editor.is_dirty());
            w.request_redraw();
        }
    }

    fn apply_lsp_completion_item(&mut self, item: &AutocompleteItem) {
        let Some(main_edit) = item.text_edit.clone() else {
            if !item.additional_text_edits.is_empty() {
                if let Some(path) = self.file_path.clone() {
                    let mut changes = std::collections::HashMap::new();
                    changes.insert(path, item.additional_text_edits.clone());
                    self.apply_workspace_edit(&crate::lsp::WorkspaceEdit { changes }, true);
                }
            }
            return;
        };

        let text = self.editor.get_full_text();
        let main_start =
            crate::lsp::lsp_pos_to_offset(&text, main_edit.start_line, main_edit.start_col);
        let mut target_cursor = main_start + main_edit.new_text.len();
        let mut changes = item.additional_text_edits.clone();
        changes.push(main_edit);

        let mut ops = Vec::with_capacity(changes.len());
        for change in &changes {
            let start = crate::lsp::lsp_pos_to_offset(&text, change.start_line, change.start_col);
            let end = crate::lsp::lsp_pos_to_offset(&text, change.end_line, change.end_col);
            ops.push((start, end, change.new_text.clone()));
        }
        ops.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));

        for (start, end, new_text) in &ops {
            if *start <= *end {
                let (off, len, _) = self.editor.replace_range(*start, *end, new_text);
                self.highlighter.shift_delete(off, len);
                self.highlighter
                    .shift_insert(off, new_text.len(), Some(new_text));
            }
        }

        for (start, end, new_text) in &ops {
            if *end <= main_start {
                let delta = new_text.len() as isize - (*end - *start) as isize;
                target_cursor = ((target_cursor as isize) + delta).max(0) as usize;
            }
        }
        self.editor.cursor = target_cursor.min(self.editor.len());
        self.editor.selection_anchor = None;
        self.sync_after_autocomplete();

        if let Some(w) = self.window.as_ref() {
            App::update_window_title(w, &self.base_title, self.editor.is_dirty());
            w.request_redraw();
        }
    }

    fn sync_after_autocomplete(&mut self) {
        if self.editor.sync_edits.is_empty() {
            return;
        }
        let edits = std::mem::take(&mut self.editor.sync_edits);
        if self.is_ide_mode {
            if let (Some(lsp), Some(path)) = (&mut self.lsp, &self.file_path) {
                let text = self.editor.get_full_text();
                let ext = self.file_extension.clone();
                let path = path.clone();
                lsp.notify_change(&path, &ext, &text, self.editor.version as i32);
            }
        }
        self.highlighter
            .apply_edits(self.editor.version, edits, None, None);
        self.last_sent_version = self.editor.version;
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
            .reset(self.editor.version, "".to_string(), "".to_string());
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
                if is_autofold && el - sl >= threshold && !self.is_highlighted_once {
                    self.editor.folded_lines.insert(sl);
                    self.editor
                        .folded_start_bytes
                        .insert(self.editor.line_offsets[sl]);
                }
            }
        }
        self.is_highlighted_once = true;
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
                while let Ok(_) = self.highlighter.rx.try_recv() {}
                self.highlighter.reset(
                    self.editor.version,
                    self.editor.get_full_text(),
                    self.file_extension.clone(),
                );

                // Ждём до 50мс первого результата подсветки — убирает мерцание при открытии файла.
                // Для малых файлов Tree-sitter укладывается в < 5мс, большие файлы — просто не ждут.
                if wait_highlight {
                    if self.highlighter.wait_for_first_result(
                        self.editor.version,
                        std::time::Duration::from_millis(50),
                    ) {
                        self.apply_highlight_results();
                    }
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

    pub fn check_external_changes(&mut self) {
        self.sync_active_tab();
        let mut needs_redraw = false;
        for tab in &mut self.tabs {
            if !tab.editor.is_dirty() {
                if let Some(path) = &tab.file_path {
                    if let Ok(disk_text) = std::fs::read_to_string(path) {
                        if disk_text != tab.editor.get_full_text() {
                            let old_version = tab.editor.version;
                            tab.editor = crate::editor::Editor::new(disk_text.len() + 8192);
                            tab.editor.version = old_version + 1;
                            let _ = tab.editor.insert_str(&disk_text);
                            tab.editor.cursor = 0;
                            tab.editor.clear_history();
                            tab.editor.set_original_text();
                            tab.editor.sync_edits.clear();
                            tab.spans.clear();
                            tab.completions.clear();
                            tab.foldable_ranges.clear();
                            tab.is_highlighted_once = false;
                            needs_redraw = true;
                        }
                    }
                }
            }
        }
        self.sync_active_tab();
        if needs_redraw {
            if let Some(w) = self.window.as_ref() {
                w.request_redraw();
            }
        }
    }
}

#[cfg(test)]
mod app_behavior_tests {
    use super::*;
    use arboard::Clipboard;
    use std::time::Instant;

    fn test_theme() -> crate::renderer::Theme {
        crate::renderer::Theme {
            bg: [0.156, 0.164, 0.211, 1.0],
            fg: [0.972, 0.972, 0.949, 1.0],
            sel: [0.55, 0.55, 0.55, 1.0],
            minimap_bg: [0.129, 0.133, 0.172, 1.0],
            line_num: [0.384, 0.447, 0.643, 1.0],
            minimap_cursor: [0.55, 0.55, 0.55, 1.0],
            modified_unsaved: [1.0, 0.474, 0.776, 1.0],
            modified_saved: [0.313, 0.980, 0.482, 1.0],
            diag_warn: [0.945, 0.980, 0.549, 1.0],
            diag_error: [1.0, 0.333, 0.333, 1.0],
            unused: [0.48, 0.48, 0.48, 0.6],
        }
    }

    fn editor_with(text: &str) -> Editor {
        let mut editor = Editor::new(text.len() + 64);
        let _ = editor.insert_str(text);
        editor.cursor = text.len();
        editor.clear_history();
        editor.set_original_text();
        editor.sync_edits.clear();
        editor
    }

    fn tab_with(title: &str, path: Option<&str>, text: &str) -> EditorTab {
        EditorTab {
            editor: editor_with(text),
            file_path: path.map(PathBuf::from),
            base_title: title.to_string(),
            file_extension: path
                .and_then(|p| std::path::Path::new(p).extension())
                .map(|ext| ext.to_string_lossy().to_string())
                .unwrap_or_default(),
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
        }
    }

    fn test_app() -> Option<App> {
        let now = Instant::now();
        Some(App {
            pending_key_log: None,
            gl_config: None,
            gl_context: None,
            gl_surface: None,
            window: None,
            dialog_window: None,
            dialog_gl_surface: None,
            settings_scroll: crate::scroll::ScrollState::new(15.0),
            tab_scroll: crate::scroll::ScrollState::new(15.0),
            renderer: None,
            editor: Editor::new(128),
            clipboard: Clipboard::new().ok(),
            theme: test_theme(),
            base_title: "Безымянный".to_string(),
            file_path: None,
            file_extension: String::new(),
            highlighter: crate::highlighter::Highlighter::new(),
            last_sent_version: u64::MAX,
            scroll_y: crate::scroll::ScrollState::new(15.0),
            scroll_x: crate::scroll::ScrollState::new(15.0),
            last_frame: now,
            last_action: now,
            last_blink_state: true,
            modifiers: winit::keyboard::ModifiersState::empty(),
            is_dragging: false,
            is_focused: true,
            render_suspended: false,
            current_cursor: winit::window::CursorIcon::Default,
            show_fps: false,
            window_width: 1000.0,
            window_height: 800.0,
            last_resize_time: None,
            last_click_time: now,
            click_count: 0,
            last_click_pos: (0.0, 0.0),
            pending_action: PendingAction::Quit,
            open_file_rx: None,
            save_file_rx: None,
            show_welcome: true,
            recent_files: Vec::new(),
            is_ide_mode: false,
            ide_workspaces: Vec::new(),
            ide_ignore_patterns: Vec::new(),
            settings_ignore_editor: Editor::new(128),
            settings_ignore_focused: false,
            settings_ignore_scroll_x: 0.0,
            is_dragging_settings_ignore: false,
            open_folder_rx: None,
            show_search: false,
            search_anim_y: -120.0,
            search_editor: Editor::new(256),
            search_focused: false,
            search_case_sensitive: false,
            search_results: Vec::new(),
            search_current_idx: None,
            is_dragging_search: false,
            is_dragging_lsp_log: false,
            faq_editor: Editor::new(128),
            is_ready: false,
            is_highlighted_once: false,
            tried_maximize: false,
            should_maximize: false,
            autocomplete_active: false,
            autocomplete_options: Vec::new(),
            autocomplete_selected_idx: 0,
            autocomplete_anim_progress: 0.0,
            autocomplete_scroll: crate::scroll::ScrollState::new(15.0),
            autocomplete_hovered_idx: None,
            autocomplete_rect: None,
            autocomplete_mode: AutocompleteMode::TreeSitter,
            autocomplete_pending_request_id: None,
            current_sticky_lines: Vec::new(),
            target_sticky_lines: Vec::new(),
            sticky_anim_progress: 1.0,
            sticky_anim_is_adding: false,
            show_settings: false,
            settings_anim_progress: 0.0,
            settings_y: 10000.0,
            settings_tab: 0,
            settings_ide_scroll: crate::scroll::ScrollState::new(7.0),
            ide_panel: IdePanelState::default(),
            file_tree_rx: None,
            file_tree_notify_rx: None,
            lsp: None,
            lsp_actions_menu: None,
            pending_fix_all_id: None,
            ctrl_definition: CtrlDefinitionState::default(),
            ui_registry: crate::ui_system::UiRegistry::new(),
            tabs: Vec::new(),
            active_tab: 0,
            run_ide_on_startup: false,
        })
    }

    fn completion(
        word: &str,
        kind: SymbolKind,
        scope_start: usize,
        scope_end: usize,
    ) -> CompletionItem {
        CompletionItem {
            word: word.to_string(),
            kind,
            scope_start,
            scope_end,
        }
    }

    #[test]
    fn search_update_finds_nearest_match_preserves_previous_and_honors_case() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.editor = editor_with("alpha beta\nAlpha beta\nbeta tail");
        app.editor.cursor = 18;
        app.search_editor = editor_with("beta");

        app.update_search();
        assert_eq!(app.search_results.len(), 3);
        assert_eq!(app.search_current_idx, Some(1));

        let previous = app.search_current_idx;
        app.update_search();
        assert_eq!(app.search_current_idx, previous);

        app.search_case_sensitive = true;
        app.search_editor = editor_with("Alpha");
        app.update_search();
        assert_eq!(app.search_results, vec![(11, 16)]);
        assert_eq!(app.search_current_idx, Some(0));

        app.search_editor = Editor::new(32);
        app.update_search();
        assert!(app.search_results.is_empty());
        assert_eq!(app.search_current_idx, None);
    }

    #[test]
    fn autocomplete_filters_scores_scrolls_and_applies_selected_completion() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.editor = editor_with("pri");
        app.editor.cursor = 3;
        app.highlighter.completions = vec![
            completion("print", SymbolKind::Function, 0, 100),
            completion("private_value", SymbolKind::Variable, 0, 100),
            completion("printf", SymbolKind::Function, 10, 20),
            completion("pri", SymbolKind::Variable, 0, 100),
        ];

        app.update_autocomplete();
        assert!(app.autocomplete_active);
        assert_eq!(app.autocomplete_selected_idx, 0);
        assert_eq!(app.autocomplete_options.len(), 2);
        assert_eq!(app.autocomplete_options[0].0.word, "private_value");
        assert_eq!(app.autocomplete_options[1].0.word, "print");

        app.autocomplete_selected_idx = 1;
        app.autocomplete_scroll.target = 200.0;
        app.ensure_autocomplete_visible();
        assert!(app.autocomplete_scroll.target <= 36.0);

        app.apply_autocomplete();
        assert_eq!(app.editor.get_full_text(), "print");
        assert!(!app.autocomplete_active);
        assert_eq!(app.autocomplete_selected_idx, 0);
        assert_eq!(app.autocomplete_scroll.target, 0.0);
    }

    #[test]
    fn ty_import_autocomplete_waits_for_prefix_and_requires_module() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.editor = editor_with("");
        app.autocomplete_mode = AutocompleteMode::TyImports;

        app.update_ty_autocomplete(vec![crate::lsp::LspCompletionItem {
            label: "Path".to_string(),
            kind: SymbolKind::Class,
            module: Some("pathlib".to_string()),
            insert_text: Some("Path".to_string()),
            text_edit: None,
            additional_text_edits: Vec::new(),
        }]);
        assert!(app.autocomplete_active);
        assert!(app.autocomplete_options.is_empty());

        app.editor = editor_with("Pa");
        app.autocomplete_mode = AutocompleteMode::TyImports;
        app.update_ty_autocomplete(vec![
            crate::lsp::LspCompletionItem {
                label: "Path".to_string(),
                kind: SymbolKind::Class,
                module: Some("pathlib".to_string()),
                insert_text: Some("Path".to_string()),
                text_edit: None,
                additional_text_edits: Vec::new(),
            },
            crate::lsp::LspCompletionItem {
                label: "ParamSpec".to_string(),
                kind: SymbolKind::Class,
                module: None,
                insert_text: Some("ParamSpec".to_string()),
                text_edit: None,
                additional_text_edits: Vec::new(),
            },
        ]);
        assert_eq!(app.autocomplete_options.len(), 1);
        assert_eq!(app.autocomplete_options[0].0.word, "Path");
        assert_eq!(app.autocomplete_options[0].0.module.as_deref(), Some("pathlib"));
    }

    #[test]
    fn python_import_completion_guard_rejects_def_async_and_strings() {
        let mut ok = editor_with("\n");
        ok.cursor = 0;
        assert!(python_import_completion_allowed(&ok));

        let mut in_def = editor_with("def func(");
        in_def.cursor = in_def.len();
        assert!(!python_import_completion_allowed(&in_def));

        let mut in_async = editor_with("async ");
        in_async.cursor = in_async.len();
        assert!(!python_import_completion_allowed(&in_async));

        let mut in_string = editor_with("value = \"Pa");
        in_string.cursor = in_string.len();
        assert!(!python_import_completion_allowed(&in_string));
    }

    #[test]
    fn tab_sync_swaps_editor_metadata_and_current_icon() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.editor = editor_with("live");
        app.base_title = "live.rs".to_string();
        app.file_path = Some(PathBuf::from("/tmp/live.rs"));
        app.file_extension = "rs".to_string();
        app.search_results = vec![(0, 1)];
        app.search_current_idx = Some(0);
        app.last_sent_version = 7;
        app.is_highlighted_once = true;

        app.tabs
            .push(tab_with("other.py", Some("/tmp/other.py"), "tab text"));
        app.active_tab = 0;

        app.sync_active_tab();

        assert_eq!(app.editor.get_full_text(), "tab text");
        assert_eq!(app.base_title, "other.py");
        assert_eq!(app.file_extension, "py");
        assert_eq!(app.tabs[0].editor.get_full_text(), "live");
        assert_eq!(app.tabs[0].base_title, "live.rs");
        assert_eq!(app.tabs[0].search_results, vec![(0, 1)]);
        assert_eq!(app.tabs[0].search_current_idx, Some(0));
        assert_eq!(app.tabs[0].last_sent_version, 7);
        assert!(app.tabs[0].is_highlighted_once);
        assert_ne!(app.tabs[0].icon_key, "default_file");
    }

    #[test]
    fn close_current_file_resets_editor_search_scroll_and_welcome_state() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.editor = editor_with("dirty text");
        app.file_path = Some(PathBuf::from("/tmp/current.py"));
        app.base_title = "current.py".to_string();
        app.file_extension = "py".to_string();
        app.search_results = vec![(0, 5)];
        app.search_current_idx = Some(0);
        app.show_search = true;
        app.autocomplete_active = true;
        app.show_welcome = false;
        app.scroll_y.current = 123.0;
        app.scroll_y.target = 456.0;
        app.scroll_x.current = 12.0;
        app.scroll_x.target = 34.0;

        app.close_current_file();

        assert_eq!(app.base_title, "Добро пожаловать");
        assert!(app.file_path.is_none());
        assert_eq!(app.file_extension, "");
        assert_eq!(app.editor.get_full_text(), "");
        assert!(app.search_results.is_empty());
        assert_eq!(app.search_current_idx, None);
        assert!(!app.show_search);
        assert!(!app.autocomplete_active);
        assert!(app.show_welcome);
        assert_eq!(app.scroll_y.current, 0.0);
        assert_eq!(app.scroll_x.target, 0.0);
    }

    #[test]
    fn file_loading_saving_and_missing_file_cleanup_update_state_without_window() {
        let Some(mut app) = test_app() else {
            return;
        };
        let unique = format!(
            "rriter-app-test-{}-{}",
            std::process::id(),
            Instant::now().elapsed().as_nanos()
        );
        let dir = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("demo.py");
        std::fs::write(&path, "print('hi')\n").unwrap();

        app.load_file_internal(path.clone(), false, false);
        assert_eq!(app.file_path.as_ref(), Some(&path));
        assert_eq!(app.base_title, "demo.py");
        assert_eq!(app.file_extension, "py");
        assert_eq!(app.editor.get_full_text(), "print('hi')\n");
        assert!(!app.show_welcome);
        assert_eq!(app.scroll_y.current, 0.0);
        assert_eq!(app.last_sent_version, u64::MAX);

        app.editor = editor_with("print('bye')\n");
        app.file_path = Some(path.clone());
        assert!(app.save_current_file());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "print('bye')\n");
        assert!(!app.editor.is_dirty());

        let missing = dir.join("missing.py");
        app.recent_files = vec![missing.clone(), path.clone()];
        app.load_file_internal(missing.clone(), false, false);
        assert_eq!(app.recent_files, vec![path.clone()]);

        std::fs::remove_file(path).ok();
        std::fs::remove_dir(dir).ok();
    }

    #[test]
    fn highlight_results_update_fold_maps_and_autofold_once() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.editor = editor_with("fn main() {\n  if true {\n    println!(\"x\");\n  }\n}\n");
        app.file_extension = "rs".to_string();
        let text = app.editor.get_full_text();
        let block_start = text.find("fn main").unwrap();
        let block_end = text.rfind('}').unwrap();
        app.highlighter.foldable_ranges = vec![(block_start, block_end, true, true)];

        app.apply_highlight_results();

        assert!(app.is_highlighted_once);
        assert_eq!(app.editor.foldable_ranges_bytes.len(), 1);
        assert!(app.editor.foldable_lines.contains_key(&0));
        assert!(app.editor.folded_lines.contains(&0));
        assert!(app.editor.folded_start_bytes.contains(&0));

        app.editor.folded_lines.clear();
        app.highlighter.foldable_ranges = vec![(block_start, block_end, true, false)];
        app.apply_highlight_results();
        assert!(app.editor.folded_lines.is_empty());
    }

    #[test]
    fn check_external_changes_refreshes_clean_tabs_and_leaves_dirty_tabs_alone() {
        let Some(mut app) = test_app() else {
            return;
        };
        let unique = format!(
            "rriter-tabs-test-{}-{}",
            std::process::id(),
            Instant::now().elapsed().as_nanos()
        );
        let dir = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&dir).unwrap();
        let clean_path = dir.join("clean.txt");
        let dirty_path = dir.join("dirty.txt");
        std::fs::write(&clean_path, "new clean\n").unwrap();
        std::fs::write(&dirty_path, "disk dirty\n").unwrap();

        let mut clean_tab = tab_with(
            "clean.txt",
            Some(clean_path.to_str().unwrap()),
            "old clean\n",
        );
        clean_tab.editor.set_original_text();
        let mut dirty_tab = tab_with(
            "dirty.txt",
            Some(dirty_path.to_str().unwrap()),
            "old dirty\n",
        );
        let _ = dirty_tab.editor.insert_str("local change");
        app.tabs = vec![clean_tab, dirty_tab];
        app.active_tab = 0;
        app.editor = Editor::new(32);
        app.base_title = "scratch".to_string();
        app.sync_active_tab();

        app.check_external_changes();
        app.sync_active_tab();

        let clean = app
            .tabs
            .iter()
            .find(|tab| tab.file_path.as_ref() == Some(&clean_path))
            .unwrap();
        let dirty = app
            .tabs
            .iter()
            .find(|tab| tab.file_path.as_ref() == Some(&dirty_path))
            .unwrap();
        assert_eq!(clean.editor.get_full_text(), "new clean\n");
        assert!(clean.spans.is_empty());
        assert!(!clean.is_highlighted_once);
        assert!(dirty.editor.get_full_text().contains("local change"));

        std::fs::remove_file(clean_path).ok();
        std::fs::remove_file(dirty_path).ok();
        std::fs::remove_dir(dir).ok();
    }

    #[test]
    fn app_tabs_recent_files_search_jump_and_autocomplete_empty_paths() {
        let Some(mut app) = test_app() else {
            return;
        };

        app.open_new_tab();
        assert!(app.tabs.is_empty());
        assert!(app.show_welcome);
        assert_eq!(app.base_title, "Добро пожаловать");

        app.is_ide_mode = true;
        app.open_new_tab();
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.active_tab, 0);
        assert_eq!(app.base_title, "Безымянный");
        assert!(!app.show_welcome);

        app.editor = editor_with("first tab");
        app.base_title = "first.py".to_string();
        app.file_extension = "py".to_string();
        app.file_path = Some(PathBuf::from("/tmp/first.py"));
        app.open_new_tab();
        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.active_tab, 1);
        assert_eq!(app.editor.get_full_text(), "");
        assert_eq!(app.base_title, "Безымянный");

        app.close_tab_at(0);
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.active_tab, 0);

        for idx in 0..12 {
            app.add_recent_file(PathBuf::from(format!("/tmp/recent-{idx}.py")));
        }
        app.add_recent_file(PathBuf::from("/tmp/recent-5.py"));
        assert_eq!(app.recent_files.len(), 10);
        assert_eq!(app.recent_files[0], PathBuf::from("/tmp/recent-5.py"));
        assert_eq!(
            app.recent_files
                .iter()
                .filter(|p| **p == PathBuf::from("/tmp/recent-5.py"))
                .count(),
            1
        );

        app.editor = editor_with("one two one");
        app.search_editor = editor_with("one");
        app.update_search();
        assert_eq!(app.search_results, vec![(0, 3), (8, 11)]);
        app.search_current_idx = Some(1);
        app.jump_to_search_result();
        assert_eq!(app.editor.selection_anchor, Some(8));
        assert_eq!(app.editor.cursor, 11);

        app.editor = editor_with("pri.");
        app.editor.cursor = 4;
        assert_eq!(app.get_current_word_prefix(), "");
        app.update_autocomplete();
        assert!(!app.autocomplete_active);
        assert!(app.autocomplete_options.is_empty());

        app.editor = editor_with("pr");
        app.highlighter.completions = vec![
            completion("print", SymbolKind::Function, 0, 10),
            completion("private", SymbolKind::Variable, 0, 100),
            completion("property", SymbolKind::Class, 0, 100),
            completion("pr", SymbolKind::Keyword, 0, 100),
        ];
        app.update_autocomplete();
        assert!(app.autocomplete_active);
        assert_eq!(app.autocomplete_options.len(), 3);
        assert_eq!(app.autocomplete_options[0].0.word, "private");
        assert_eq!(app.autocomplete_options[1].0.word, "print");
        assert_eq!(app.autocomplete_options[2].0.word, "property");
    }

    #[test]
    fn ide_mode_startup_tab_and_tab_close_paths_are_headless_safe() {
        let Some(mut app) = test_app() else {
            return;
        };

        app.show_welcome = true;
        app.base_title = "Добро пожаловать".to_string();
        app.editor = editor_with("startup buffer");
        app.file_extension = "txt".to_string();

        app.enter_ide_mode();
        assert!(app.is_ide_mode);
        assert!(!app.show_welcome);
        assert_eq!(app.base_title, "Безымянный");
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.active_tab, 0);
        assert!(app.lsp.is_some());
        assert!(app.tabs[0].file_path.is_none());

        app.editor = editor_with("active");
        app.base_title = "active.rs".to_string();
        app.file_extension = "rs".to_string();
        app.tabs
            .push(tab_with("other.py", Some("/tmp/other.py"), "other"));
        app.active_tab = 0;

        app.close_tab_at(0);
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.active_tab, 0);
        assert_eq!(app.editor.get_full_text(), "other");
        assert_eq!(app.base_title, "other.py");

        app.close_tab_at(99);
        assert!(app.show_welcome);
        assert_eq!(app.base_title, "Добро пожаловать");
    }

    #[test]
    fn open_file_in_tab_reuses_existing_tabs_and_loads_into_empty_slot() {
        let Some(mut app) = test_app() else {
            return;
        };
        let unique = format!(
            "rriter-open-tab-test-{}-{}",
            std::process::id(),
            Instant::now().elapsed().as_nanos()
        );
        let dir = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&dir).unwrap();
        let first = dir.join("first.py");
        let second = dir.join("second.txt");
        std::fs::write(&first, "first\n").unwrap();
        std::fs::write(&second, "second\n").unwrap();

        app.is_ide_mode = true;
        app.tabs.push(tab_with(
            "first.py",
            Some(first.to_str().unwrap()),
            "cached first\n",
        ));
        app.tabs.push(tab_with("scratch", None, ""));
        app.active_tab = 1;
        app.editor = Editor::new(32);
        app.base_title = "scratch".to_string();

        app.open_file_in_tab_bg(first.clone(), false);
        assert_eq!(app.active_tab, 1);
        assert_eq!(app.editor.get_full_text(), "");

        app.open_file_in_tab(first.clone(), false);
        assert_eq!(app.active_tab, 0);
        assert_eq!(app.editor.get_full_text(), "cached first\n");

        app.switch_to_tab(1);
        app.open_file_in_tab(second.clone(), false);
        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.file_path.as_ref(), Some(&second));
        assert_eq!(app.editor.get_full_text(), "second\n");

        std::fs::remove_file(first).ok();
        std::fs::remove_file(second).ok();
        std::fs::remove_dir(dir).ok();
    }

    #[test]
    fn python_assignment_declaration_jump_prefers_nearest_usage() {
        let editor = editor_with("value = build()\nprint(value)\nvalue_other = value\n");
        let source_start = editor.get_full_text().find("value").unwrap();
        let usage = nearest_python_assignment_usage(&editor, (source_start, source_start + 5))
            .expect("expected usage");

        assert_eq!(editor.get_full_text().get(usage..usage + 5), Some("value"));
        assert_eq!(editor.get_full_text()[..usage].lines().count(), 2);
    }

    #[test]
    fn python_assignment_declaration_ignores_def_and_comparisons() {
        let def_editor = editor_with("def value():\n    return value\n");
        let def_start = def_editor.get_full_text().find("value").unwrap();
        assert_eq!(
            nearest_python_assignment_usage(&def_editor, (def_start, def_start + 5)),
            None
        );

        let cmp_editor = editor_with("value == other\nprint(value)\n");
        let cmp_start = cmp_editor.get_full_text().find("value").unwrap();
        assert_eq!(
            nearest_python_assignment_usage(&cmp_editor, (cmp_start, cmp_start + 5)),
            None
        );
    }

    #[test]
    fn python_assignment_declaration_ignores_annotation_type_tokens() {
        let editor = editor_with("title: t.Optional[str] = None\nbody: t.Optional[str] = None\n");
        let optional_start = editor.get_full_text().find("Optional").unwrap();
        let title_start = editor.get_full_text().find("title").unwrap();

        assert_eq!(
            nearest_python_assignment_usage(&editor, (optional_start, optional_start + 8)),
            None
        );
        assert_eq!(
            nearest_python_assignment_usage(&editor, (title_start, title_start + 5)),
            None
        );
    }

    #[test]
    fn ctrl_definition_same_declaration_target_redirects_to_usage() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.is_ide_mode = true;
        app.file_extension = "py".to_string();
        app.file_path = Some(PathBuf::from("/tmp/ctrl_def.py"));
        app.editor = editor_with("value = build()\nprint(value)\n");

        let source_start = app.editor.get_full_text().find("value").unwrap();
        let source_range = (source_start, source_start + 5);
        let (line, col) = crate::lsp::offset_to_lsp_pos(
            &app.editor.get_full_text(),
            source_start,
            &app.editor.line_offsets,
        );
        app.ctrl_definition.source_path = app.current_abs_path();
        app.ctrl_definition.source_range = Some(source_range);

        let target = app
            .ctrl_definition_target_from_lsp(Some(DefinitionJumpTarget {
                path: PathBuf::from("/tmp/ctrl_def.py"),
                line,
                col,
            }))
            .expect("expected usage target");

        assert_eq!(target.path, PathBuf::from("/tmp/ctrl_def.py"));
        assert_eq!(target.line, 1);
        assert_eq!(target.col, 6);
    }

    #[test]
    fn highlight_thresholds_and_prefix_edges_cover_non_default_paths() {
        let Some(mut app) = test_app() else {
            return;
        };

        app.editor = editor_with("root:\n  a: 1\n  b: 2\n");
        app.file_extension = "yaml".to_string();
        let end = app.editor.len();
        app.highlighter.foldable_ranges = vec![(0, end, true, false)];
        app.apply_highlight_results();
        assert!(app.editor.foldable_lines.contains_key(&0));
        assert!(app.editor.folded_lines.is_empty());

        app.editor = editor_with("obj.attr\nsnake_case");
        app.editor.cursor = app.editor.len();
        assert_eq!(app.get_current_word_prefix(), "snake_case");
        app.editor.cursor = 3;
        assert_eq!(app.get_current_word_prefix(), "obj");
    }

    #[test]
    fn lsp_actions_noqa_workspace_edit_and_panel_log_sizes_headless() {
        let Some(mut app) = test_app() else {
            return;
        };

        let path = PathBuf::from("/tmp/main.py");
        app.file_path = Some(path.clone());
        app.file_extension = "py".to_string();
        app.base_title = "main.py".to_string();
        app.editor = editor_with("x = 1\nvalue = 2  # noqa: E501\n");

        app.insert_noqa_comment(0, &["F401".to_string(), "E501".to_string()]);
        assert!(
            app.editor
                .get_full_text()
                .starts_with("x = 1  # noqa: F401, E501\n")
        );

        app.insert_noqa_comment(1, &["F821".to_string(), "E501".to_string()]);
        assert!(
            app.editor
                .get_full_text()
                .contains("value = 2  # noqa: E501, F821")
        );

        app.insert_noqa_comment(1, &[]);
        assert!(app.editor.get_full_text().contains("value = 2  # noqa\n"));

        app.editor = editor_with("abc\ndef\nghi\n");
        app.editor.cursor = 5;
        app.editor.selection_anchor = Some(1);

        let mut changes = std::collections::HashMap::new();
        changes.insert(
            path.clone(),
            vec![
                crate::lsp::TextChange {
                    start_line: 1,
                    start_col: 0,
                    end_line: 1,
                    end_col: 3,
                    new_text: "DEF".to_string(),
                },
                crate::lsp::TextChange {
                    start_line: 0,
                    start_col: 1,
                    end_line: 0,
                    end_col: 2,
                    new_text: "B".to_string(),
                },
            ],
        );
        app.apply_workspace_edit(&crate::lsp::WorkspaceEdit { changes }, true);
        assert_eq!(app.editor.get_full_text(), "aBc\nDEF\nghi\n");
        assert_eq!(app.editor.cursor, 5);
        assert_eq!(app.editor.selection_anchor, Some(1));

        let mut action_changes = std::collections::HashMap::new();
        action_changes.insert(
            path.clone(),
            vec![crate::lsp::TextChange {
                start_line: 2,
                start_col: 0,
                end_line: 2,
                end_col: 3,
                new_text: "GHI".to_string(),
            }],
        );
        app.lsp_actions_menu = Some(LspActionsMenu {
            cursor_line: 0,
            items: vec![LspActionItem::CodeAction(crate::lsp::CodeAction {
                title: "Upper".to_string(),
                kind: Some("quickfix".to_string()),
                edit: Some(crate::lsp::WorkspaceEdit {
                    changes: action_changes,
                }),
                code: Some("T001".to_string()),
            })],
            selected: 0,
            menu_x: 0.0,
            menu_y: 0.0,
            pending_request_id: None,
        });
        app.apply_selected_lsp_action();
        assert_eq!(app.editor.get_full_text(), "aBc\nDEF\nGHI\n");

        app.lsp_actions_menu = Some(LspActionsMenu {
            cursor_line: 0,
            items: vec![LspActionItem::AddNoqa {
                codes: vec!["T002".to_string()],
            }],
            selected: 0,
            menu_x: 0.0,
            menu_y: 0.0,
            pending_request_id: None,
        });
        app.apply_selected_lsp_action();
        assert!(
            app.editor
                .get_full_text()
                .starts_with("aBc  # noqa: T002\n")
        );

        assert!(app.lsp_panel_bounds().is_none());

        let info = crate::lsp::LspServerInfo {
            name: "ruff",
            status: crate::lsp::LspServerStatus::Running,
            logs: Vec::new(),
        };
        app.ide_panel.lsp_servers = vec![info.clone()];
        assert_eq!(app.lsp_server_logs_h(&info, 1.0), 0.0);

        app.ide_panel.lsp_logs_expanded.insert("ruff".to_string());
        let mut log_editor = editor_with("header\n  detail\nlast line\n");
        log_editor.foldable_lines.insert(0, 1);
        log_editor.folded_lines.insert(0);
        app.ide_panel
            .lsp_log_editors
            .insert("ruff".to_string(), log_editor);

        let (inner_h, inner_w) = app.lsp_server_inner_size(&info, 1.0);
        assert!(inner_h >= 32.0);
        assert!(inner_w > 0.0);
        assert!(app.lsp_server_logs_h(&info, 1.0) >= 50.0);
        assert!(app.lsp_panel_total_h(1.0) >= 210.0);
    }

    #[test]
    fn ui_handlers_state_only_branches_work_without_window() {
        let Some(mut app) = test_app() else {
            return;
        };

        app.handle_ui_click(crate::ui_system::UiId::HoverPopupScroll);
        app.handle_ui_click(crate::ui_system::UiId::BottomPanelBody);

        app.ide_panel.terminal_focused = true;
        app.handle_ui_click(crate::ui_system::UiId::ResizeLeft);
        app.handle_ui_click(crate::ui_system::UiId::ResizeBottom);
        assert!(!app.ide_panel.is_resizing_left);
        assert!(!app.ide_panel.is_resizing_bottom);

        app.handle_ui_click(crate::ui_system::UiId::LspScrollY);
        app.handle_ui_click(crate::ui_system::UiId::LspScrollX);
        assert!(app.ide_panel.lsp_scroll_y.is_dragging);
        assert!(app.ide_panel.lsp_scroll_x.is_dragging);

        app.handle_ui_click(crate::ui_system::UiId::EditorScrollbarX);
        assert!(app.scroll_x.is_dragging);

        app.ide_panel.lsp_servers = vec![crate::lsp::LspServerInfo {
            name: "ruff",
            status: crate::lsp::LspServerStatus::Running,
            logs: Vec::new(),
        }];
        app.handle_ui_click(crate::ui_system::UiId::LspLogScrollY(0));
        app.handle_ui_click(crate::ui_system::UiId::LspLogScrollX(0));
        assert!(
            app.ide_panel
                .lsp_logs_scroll_y
                .get("ruff")
                .is_some_and(|scroll| scroll.is_dragging)
        );
        assert!(
            app.ide_panel
                .lsp_logs_scroll_x
                .get("ruff")
                .is_some_and(|scroll| scroll.is_dragging)
        );

        app.ide_panel
            .flat_diags
            .push((PathBuf::from("/tmp/main.py"), 0));
        app.handle_ui_click(crate::ui_system::UiId::ProblemFileToggle(0));
        assert!(app.ide_panel.problems_collapsed.is_empty());
    }

    #[test]
    fn ui_handlers_search_problem_log_and_diagnostic_actions_are_headless_safe() {
        let Some(mut app) = test_app() else {
            return;
        };

        app.editor = editor_with("alpha beta alpha");
        app.editor.cursor = 0;
        app.search_editor = editor_with("alpha");
        app.show_search = true;
        app.search_focused = true;
        app.update_search();
        assert_eq!(app.search_current_idx, Some(0));

        app.handle_ui_click(crate::ui_system::UiId::SearchNext);
        assert_eq!(app.search_current_idx, Some(1));
        assert_eq!(app.editor.selection_anchor, Some(11));
        assert_eq!(app.editor.cursor, 16);

        app.handle_ui_click(crate::ui_system::UiId::SearchPrev);
        assert_eq!(app.search_current_idx, Some(0));
        assert_eq!(app.editor.selection_anchor, Some(0));
        assert_eq!(app.editor.cursor, 5);

        app.handle_ui_click(crate::ui_system::UiId::SearchCaseToggle);
        assert!(app.search_case_sensitive);
        app.handle_ui_click(crate::ui_system::UiId::SearchClose);
        assert!(!app.show_search);
        assert!(!app.search_focused);
        assert!(app.search_results.is_empty());
        assert_eq!(app.search_current_idx, None);

        let path = PathBuf::from("/tmp/main.py");
        app.ide_panel.flat_diags.push((path.clone(), usize::MAX));
        app.handle_ui_click(crate::ui_system::UiId::ProblemFileToggle(0));
        assert!(app.ide_panel.problems_collapsed.contains(&path));
        app.handle_ui_click(crate::ui_system::UiId::ProblemFileToggle(0));
        assert!(!app.ide_panel.problems_collapsed.contains(&path));

        app.handle_ui_click(crate::ui_system::UiId::ProblemsTab(2));
        assert_eq!(app.ide_panel.problems_tab, 2);

        app.ide_panel.lsp_servers = vec![crate::lsp::LspServerInfo {
            name: "ruff",
            status: crate::lsp::LspServerStatus::Running,
            logs: Vec::new(),
        }];
        let mut log_editor = editor_with("line one\nline two\n");
        log_editor.selection_anchor = Some(0);
        app.ide_panel
            .lsp_log_editors
            .insert("ruff".to_string(), log_editor);

        app.handle_ui_click(crate::ui_system::UiId::LspLogArea(0));
        assert_eq!(app.ide_panel.lsp_logs_focused.as_deref(), Some("ruff"));
        assert!(app.is_dragging_lsp_log);
        assert_eq!(
            app.ide_panel
                .lsp_log_editors
                .get("ruff")
                .and_then(|ed| ed.selection_anchor),
            None
        );
    }
}
