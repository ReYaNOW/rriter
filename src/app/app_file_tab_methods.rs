impl App {
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
                is_highlight_complete: false,
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
            is_highlight_complete: false,
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
            if self.active_tab_is_api_client() {
                while let Ok(_) = self.highlighter.rx.try_recv() {}
            } else {
                self.editor.version = self.next_tab_highlight_version();
                while let Ok(_) = self.highlighter.rx.try_recv() {}
                self.reset_highlighter_with_text(self.editor.get_full_text(), false);
                self.wait_for_current_highlight();
            }
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
            self.tabs.is_empty() && self.file_path.is_none() && self.editor.len() == 0;
        self.clamp_tab_scroll_to_content_now();

        if let Some(w) = self.window.as_ref() {
            App::update_window_title(w, &self.base_title, self.editor.is_dirty());
            w.request_redraw();
        }
        self.save_tabs_state();
        self.start_file_watcher();
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
        self.open_file_in_tab_internal_options(path, add_to_history, wait_highlight, true);
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn open_file_in_tab_internal_options(
        &mut self,
        path: PathBuf,
        add_to_history: bool,
        wait_highlight: bool,
        start_highlighter: bool,
    ) {
        if !self.is_ide_mode {
            if start_highlighter {
                self.load_file_internal(path, add_to_history, wait_highlight);
            } else {
                self.load_file_internal_options(path, add_to_history, wait_highlight, false);
            }
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
            || self.active_tab_is_api_client()
        {
            self.open_new_tab();
        }

        if start_highlighter {
            self.load_file_internal(path, false, wait_highlight);
        } else {
            self.load_file_internal_options(path, false, wait_highlight, false);
        }
        self.reveal_active_tab_now();
        self.start_file_watcher();
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

    fn path_is_open_in_tabs(&self, path: &Path) -> bool {
        let abs_path = self.abs_path_for_workspace(path);
        if self.current_abs_path().as_ref() == Some(&abs_path) {
            return true;
        }
        self.tabs.iter().enumerate().any(|(i, tab)| {
            i != self.active_tab
                && tab
                    .file_path
                    .as_ref()
                    .is_some_and(|p| self.abs_path_for_workspace(p) == abs_path)
        })
    }

    pub(crate) fn jump_to_lsp_position_in_file(
        &mut self,
        path: PathBuf,
        line: u32,
        col: u32,
        add_to_history: bool,
        center_ratio: f32,
    ) -> bool {
        let was_open = self.path_is_open_in_tabs(&path);
        self.open_file_in_tab_internal_options(path, add_to_history, was_open, was_open);

        let text = self.editor.get_full_text();
        let offset = crate::lsp::lsp_pos_to_offset(&text, line, col).min(self.editor.len());
        self.editor.cursor = offset;
        self.editor.selection_anchor = None;

        if !was_open {
            self.reprioritize_highlighter_around_cursor();
            self.wait_for_current_highlight();
        }

        self.scroll_cursor_near_center(center_ratio, !was_open);
        was_open
    }

    fn scroll_cursor_near_center(&mut self, center_ratio: f32, snap: bool) {
        if let Some(r) = self.renderer.as_mut() {
            let wh = self
                .window
                .as_ref()
                .map(|w| w.inner_size().height as f32)
                .unwrap_or(r.height);
            let tab_bar_h = if self.show_welcome || !self.is_ide_mode {
                0.0
            } else {
                44.0 * r.scale_factor
            };
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
            let (_, cursor_y) = r.get_cursor_xy(&self.editor);
            let cursor_line_top_y = (cursor_y - r.baseline_offset).max(0.0);
            let target_y = (cursor_line_top_y - visible_h * center_ratio)
                .max(0.0)
                .min(max_scroll)
                .round();
            self.scroll_y.target = target_y;
            self.scroll_y.anim_speed = 15.0;
            self.scroll_x.target = 0.0;
            self.scroll_x.anim_speed = 15.0;

            if snap {
                self.scroll_y.current = target_y;
                self.scroll_y.velocity = 0.0;
                self.scroll_x.current = 0.0;
                self.scroll_x.velocity = 0.0;
            }
        }
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

    pub(crate) fn editor_has_input_focus(&self) -> bool {
        !self.show_welcome
            && !self.active_tab_is_git_diff()
            && !self.active_tab_is_api_client()
            && !self.search_focused
            && !self.settings_ignore_focused
            && self.ide_panel.api.focused.is_none()
            && !self.ide_panel.terminal_focused
            && !self.ide_panel.term_search_focused
            && !self.ide_panel.git.message_focused
            && self.ide_panel.project_search.focused.is_none()
            && self.ide_panel.lsp_logs_focused.is_none()
            && !self.ide_panel.lsp_log_filter_focused
            && !self.ide_panel.file_tree_focused
    }

    pub(crate) fn autosave_current_file_if_dirty(&mut self) -> bool {
        if !self.is_ide_mode
            || self.active_tab_is_git_diff()
            || self.active_tab_is_api_client()
            || self.file_path.is_none()
            || !self.editor.is_dirty()
        {
            return false;
        }
        let saved = self.save_current_file();
        if saved && let Some(window) = self.window.as_ref() {
            App::update_window_title(window, &self.base_title, self.editor.is_dirty());
        }
        saved
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
            44.0 * r.scale_factor
        };
        let mouse_x = r.last_mouse_x;
        let mouse_y = r.last_mouse_y + self.scroll_y.current.round() - tab_bar_h;
        let byte = r.get_byte_at_xy(&self.editor, mouse_x, mouse_y);
        let normalized = crate::app::mouse::normalize_hover_byte(&self.editor, byte)?;
        (crate::app::mouse::hover_token_bounds(&self.editor, normalized) == source_range)
            .then_some(target)
    }

    pub(crate) fn jump_to_definition_target(&mut self, target: DefinitionJumpTarget) {
        self.jump_to_lsp_position_in_file(target.path, target.line, target.col, true, 0.42);
        self.clear_ctrl_definition();

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
                        44.0 * s
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

}
