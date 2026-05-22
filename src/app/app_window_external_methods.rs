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
                self.refresh_current_editor_git_base();
                let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                self.base_title = file_name.into_owned();
                self.file_extension = path
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_default();
                self.is_highlighted_once = false;
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
