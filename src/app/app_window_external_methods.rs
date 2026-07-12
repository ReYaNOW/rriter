impl App {
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
        self.is_editor_drag_pending = false;
        self.scroll_y.is_dragging = false;
        self.scroll_x.is_dragging = false;
        self.pending_action = action;

        if self.dialog_window.is_some() {
            return;
        }

        let attrs = crate::platform::apply_window_attributes(winit::window::Window::default_attributes()
            .with_title("Подтверждение — RRiter")
            .with_inner_size(winit::dpi::LogicalSize::new(660.0, 260.0))
            .with_window_level(winit::window::WindowLevel::AlwaysOnTop)
            .with_resizable(false));

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
        self.file_key = None;
        self.text_file_format = crate::platform::TextFileFormat::default();
        let old_ext = self.file_extension.clone();
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

        if self.is_ide_mode {
            if let Some(lsp) = &mut self.lsp {
                if let Some(path) = path_to_close {
                    lsp.notify_close(&path, &old_ext);
                }
            }
            self.tabs.clear();
        }

        self.file_extension = String::new();

        self.scroll_y.current = 0.0;
        self.scroll_y.target = 0.0;
        self.scroll_x.current = 0.0;
        self.scroll_x.target = 0.0;

        if let Some(w) = self.window.as_ref() {
            App::update_window_title(w, &self.base_title, false);
            w.request_redraw();
        }
        self.save_tabs_state();
        self.start_file_watcher();
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn trigger_file_picker(&mut self) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.open_file_rx = Some(rx);
        std::thread::spawn(move || {
            let file = crate::platform::pick_file("Открыть файл");
            let _ = tx.send(file);
        });
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn trigger_folder_picker(&mut self) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.open_folder_rx = Some(rx);
        std::thread::spawn(move || {
            let folder = crate::platform::pick_folder("Выбрать папку");
            let _ = tx.send(folder);
        });
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn trigger_save_as_picker(&mut self) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.save_file_rx = Some(rx);
        std::thread::spawn(move || {
            let file = crate::platform::save_file("Сохранить файл как...", "Безымянный.txt");
            let _ = tx.send(file);
        });
    }

    pub fn save_current_file(&mut self) -> bool {
        if self.active_tab_is_git_diff() {
            return self.save_active_git_diff();
        }
        if let Some(path) = self.file_path.clone() {
            let content = self.editor.get_full_text();
            if self.write_current_text_to_path(&path, &content) {
                self.editor.mark_saved();
                self.save_tabs_state();
                return true;
            }
        } else {
            self.trigger_save_as_picker();
        }
        false
    }

    fn write_current_text_to_path(&self, path: &Path, content: &str) -> bool {
        match crate::platform::write_text_file(path, content, self.text_file_format) {
            Ok(()) => true,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                crate::platform::write_text_file_elevated(
                    path,
                    content,
                    self.text_file_format,
                )
                .is_ok()
            }
            Err(_) => false,
        }
    }

    /// Saves to a new path and changes the active document identity only after
    /// the replacement has completed successfully.
    pub fn save_current_file_as(&mut self, path: PathBuf) -> bool {
        if self.active_tab_is_git_diff() {
            return false;
        }

        let requested_path = crate::platform::canonicalize_or_absolutize(&path);
        let content = self.editor.get_full_text();
        if !self.write_current_text_to_path(&requested_path, &content) {
            return false;
        }

        let path = crate::platform::canonicalize_or_absolutize(&requested_path);
        let old_path = self.file_path.clone();
        let old_extension = self.file_extension.clone();
        self.file_path = Some(path.clone());
        self.file_key = Some(crate::platform::PathKey::new(&path));
        self.base_title = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        self.file_extension = path
            .extension()
            .map(|extension| extension.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.editor.mark_saved();

        if self.is_ide_mode
            && let Some(lsp) = &mut self.lsp
        {
            if let Some(old_path) = old_path.as_ref()
                && !crate::platform::paths_equal(old_path, &path)
            {
                lsp.notify_close(old_path, &old_extension);
            }
            lsp.notify_open(
                &path,
                &self.file_extension,
                &content,
                self.editor.version as i32,
            );
        }

        self.add_recent_file(path);
        self.refresh_current_editor_git_base();
        self.save_tabs_state();
        self.start_file_watcher();
        true
    }

    pub fn add_recent_file(&mut self, path: PathBuf) {
        self.recent_files
            .retain(|existing| !crate::platform::paths_equal(existing, &path));
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
        self.is_highlight_complete = self.highlighter.is_complete;
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
        let priority =
            crate::highlighter::should_prioritize_front_highlight(&self.file_extension, &text);
        if !cfg!(test)
            && (text.len() >= crate::highlighter::TREE_SITTER_HIGHLIGHT_MAX_BYTES || priority)
        {
            eprintln!(
                "[HL TRACE app:reset_clear] ver={} bytes={} lines={} ext={} priority={} cursor={} old_spans={} old_complete={}",
                self.editor.version,
                text.len(),
                text.as_bytes().iter().filter(|&&b| b == b'\n').count() + 1,
                self.file_extension,
                priority,
                self.editor.cursor.min(text.len()),
                self.highlighter.spans.len(),
                self.highlighter.is_complete,
            );
        }
        self.highlighter.spans.clear();
        self.highlighter.completions.clear();
        self.highlighter.foldable_ranges.clear();
        self.highlighter.syntax_errors.clear();
        self.is_highlight_complete = false;
        let version = self.editor.version;
        if self.highlighter.current_version >= version {
            self.highlighter.current_version = version.saturating_sub(1);
        }
        let ext = self.file_extension.clone();
        let priority_anchor = self.editor.cursor.min(text.len());
        self.highlighter.reset(version, text, ext, priority_anchor);
    }

    pub(crate) fn reprioritize_highlighter_around_cursor(&mut self) {
        while let Ok(_) = self.highlighter.rx.try_recv() {}
        self.reset_highlighter_with_text(self.editor.get_full_text(), false);
        self.is_highlighted_once = false;
    }

    pub(crate) fn request_visible_priority_highlight(&mut self) -> bool {
        if self.show_welcome
            || self.active_tab_is_api_client()
            || self.active_tab_is_git_diff()
            || self.editor.len() == 0
        {
            return false;
        }

        let window_height = self
            .window
            .as_ref()
            .map(|window| window.inner_size().height as f32)
            .unwrap_or(self.window_height as f32);
        let target_scroll = self.scroll_y.target.max(0.0);
        let moving_down = target_scroll >= self.scroll_y.current;
        let line_range = {
            let Some(renderer) = self.renderer.as_ref() else {
                return false;
            };
            let scale = renderer.scale_factor;
            let tab_bar_h = if self.show_welcome || !self.is_ide_mode {
                0.0
            } else {
                44.0 * scale
            };
            let editor_bottom_h = if self.is_ide_mode {
                self.ide_panel.editor_reserved_bottom_height(scale)
            } else {
                0.0
            };
            let visible_h = crate::render_view::editor_view_height(
                window_height,
                tab_bar_h,
                editor_bottom_h,
                self.is_ide_mode,
                scale,
            )
            .max(renderer.line_height);
            renderer.minimap_visible_physical_line_range(
                &self.editor,
                target_scroll,
                visible_h,
            )
        };
        if line_range.is_empty() {
            return false;
        }

        let first_line = line_range.start.min(self.editor.line_offsets.len() - 1);
        let range_start = self.editor.line_offsets[first_line].min(self.editor.len() - 1);
        let range_end = self
            .editor
            .line_offsets
            .get(line_range.end)
            .copied()
            .unwrap_or_else(|| self.editor.len())
            .min(self.editor.len());
        let Some(anchor) = self.highlighter.unhighlighted_anchor_in_range(
            range_start,
            range_end,
            moving_down,
        ) else {
            return false;
        };
        self.highlighter
            .request_priority_highlight(self.editor.version, anchor)
    }

    pub fn load_file_internal(
        &mut self,
        path: PathBuf,
        add_to_history: bool,
        wait_highlight: bool,
    ) {
        self.load_file_internal_options(path, add_to_history, wait_highlight, true);
    }

    pub fn load_file_internal_options(
        &mut self,
        path: PathBuf,
        add_to_history: bool,
        wait_highlight: bool,
        start_highlighter: bool,
    ) {
        let path = crate::platform::canonicalize_or_absolutize(&path);
        match crate::platform::read_text_file(&path) {
            Ok(decoded) => {
                let content = decoded.text;
                self.show_welcome = false;
                if add_to_history {
                    self.add_recent_file(path.clone());
                }

                let old_version = self.editor.version;
                self.editor = Editor::new(content.len() + 8192);
                self.editor.version = old_version + 1;
                self.editor.set_clean_text(&content);
                self.file_path = Some(path.clone());
                self.file_key = Some(crate::platform::PathKey::new(&path));
                self.text_file_format = decoded.format;
                self.refresh_current_editor_git_base();
                let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                self.base_title = file_name.into_owned();
                self.file_extension = path
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_default();
                self.is_highlighted_once = false;
                self.is_highlight_complete = false;
                if start_highlighter {
                    while let Ok(_) = self.highlighter.rx.try_recv() {}
                    self.reset_highlighter_with_text(content.clone(), !wait_highlight);
                } else {
                    self.highlighter.spans.clear();
                    self.highlighter.completions.clear();
                    self.highlighter.foldable_ranges.clear();
                    self.highlighter.syntax_errors.clear();
                }
                apply_initial_import_folds(&mut self.editor, &self.file_extension, &content);

                // Ждём до 150мс: малые файлы полностью, большие py/rs до первого priority chunk.
                if start_highlighter && wait_highlight {
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
                self.recent_files
                    .retain(|recent| !crate::platform::paths_equal(recent, &path));
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
                    EditorTabKind::Normal | EditorTabKind::ApiClient(_, _) => None,
                };
                if let Some(path) = tab.file_path.as_ref().or(diff_path.as_ref()) {
                    if let Ok(decoded) = crate::platform::read_text_file(path) {
                        let disk_text = decoded.text;
                        if let EditorTabKind::GitDiff(_, state) = &tab.kind {
                            if disk_text != state.worktree_text {
                                diff_reloads.push(idx);
                                needs_redraw = true;
                            }
                            continue;
                        }
                        if !tab.editor.text_equals(&disk_text) {
                            let old_version = tab.editor.version;
                            tab.editor = crate::editor::Editor::new(disk_text.len() + 8192);
                            tab.editor.version = old_version + 1;
                            let _ = tab.editor.insert_str(&disk_text);
                            tab.editor.cursor = 0;
                            tab.editor.clear_history();
                            tab.editor.set_original_text();
                            tab.editor.sync_edits.clear();
                            tab.text_file_format = decoded.format;
                            tab.completions.clear();
                            tab.foldable_ranges.clear();
                            tab.is_highlighted_once = false;
                            tab.is_highlight_complete = false;
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
                    EditorTabKind::ApiClient(_, _) => None,
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
                if let Ok(decoded) = crate::platform::read_text_file(&path) {
                    changes.push(crate::app::ExternalFileChange {
                        tab_idx,
                        path,
                        disk_text: decoded.text,
                        text_file_format: decoded.format,
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
                if tab.editor.is_dirty()
                    || !crate::platform::paths_equal(
                        &meta.repo_root.join(&meta.rel_path),
                        &change.path,
                    )
                {
                    continue;
                }
                if change.disk_text != state.worktree_text {
                    diff_reloads.push(change.tab_idx);
                    needs_redraw = true;
                }
                continue;
            }
            if !tab
                .file_path
                .as_deref()
                .is_some_and(|path| crate::platform::paths_equal(path, &change.path))
                || tab.editor.is_dirty()
            {
                continue;
            }
            if tab.editor.text_equals(&change.disk_text) {
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
            tab.text_file_format = change.text_file_format;
            tab.file_key = Some(crate::platform::PathKey::new(&change.path));
            tab.completions.clear();
            tab.foldable_ranges.clear();
            tab.is_highlighted_once = false;
            tab.is_highlight_complete = false;
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
