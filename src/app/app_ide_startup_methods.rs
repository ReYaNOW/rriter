// IDE mode entry and the `--ide` startup: the early read of the saved session, tabs restored
// as placeholders (`TabLoad`), the blank editor area until the active tab's first highlight,
// and the input gate for that window. Included into `app.rs` next to `app_ide_tab_methods.rs`.

impl App {
    /// Enters IDE mode and restores the saved session; everything is loaded on return.
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn enter_ide_mode(&mut self) {
        self.enter_ide_mode_impl(false);
    }

    /// `--ide` startup: only what the first content frame draws is loaded here (panels, tab
    /// list, the active tab highlighted). The rest runs in `run_ide_deferred` after that frame.
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) fn enter_ide_mode_deferred(&mut self) {
        self.enter_ide_mode_impl(true);
    }

    fn enter_ide_mode_impl(&mut self, defer: bool) {
        self.is_ide_mode = true;

        let was_welcome = self.show_welcome;
        self.show_welcome = false;
        if was_welcome && self.base_title == "Добро пожаловать" {
            self.base_title = "Безымянный".to_string();
            self.file_path = None;
            self.file_key = None;
            self.text_file_format = crate::platform::TextFileFormat::default();
        }

        // Re-entering IDE mode (Welcome after closing a file) must not stop a running mock server.
        let mock_server = std::mem::take(&mut self.ide_panel.api.mock.server);
        let mock_server_status = self.ide_panel.api.mock.server_status.clone();
        if self.is_automation_mode() {
            self.ide_panel = crate::app::IdePanelState::default();
        } else {
            self.ide_panel = crate::load_panel_state();
            self.ide_panel.api = crate::app::api_client::ApiClientState::load_persisted();
            self.load_database_panel_state();
        }
        self.ide_panel.api.mock.server = mock_server;
        self.ide_panel.api.mock.server_status = mock_server_status;
        self.ide_panel.enforce_single_open_per_group();

        if self.ide_panel.is_open(PanelId::Database) {
            self.reconcile_expanded_database_connections();
        }

        if self.ide_panel.is_open(PanelId::Terminal) && self.ide_panel.terminals.is_empty() {
            self.add_terminal();
        }
        self.startup_trace.mark("ide-state");

        let has_startup_file =
            self.file_path.is_some() || self.editor.len() > 0 || self.editor.is_dirty();

        if has_startup_file && self.tabs.is_empty() {
            self.tabs.push(EditorTab {
                editor: crate::editor::Editor::new(128),
                file_path: self.file_path.clone(),
                file_key: self.file_key.clone(),
                text_file_format: self.text_file_format,
                base_title: self.base_title.clone(),
                file_extension: self.file_extension.clone(),
                markdown: crate::app::MarkdownTabState::for_file_extension(&self.file_extension),
                pdf: None,
                image: None,
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
                closing_hints: Default::default(),
                deleted: false,
                // Opened before IDE mode: its git base is read once the workspaces are known
                // (`run_ide_deferred`), which also opens it on the LSP.
                load: crate::app::TabLoad::GitBasePending,
                kind: EditorTabKind::Normal,
            });
            self.active_tab = 0;
        }

        let (saved_tabs, saved_active) = if self.scroll_render_bench.is_some()
            || (self.is_automation_mode() && !self.automation_restores_session())
        {
            (Vec::new(), 0)
        } else if let Some(preload) = self.ide_preload.as_mut() {
            (std::mem::take(&mut preload.tabs), preload.active)
        } else {
            crate::load_open_tabs(true)
        };

        if !saved_tabs.is_empty() {
            let mut loaded_any = false;
            // The tab the saved active index ended up at (saved entries may be skipped).
            let mut target_tab = None;
            for (saved_idx, saved_tab) in saved_tabs.into_iter().enumerate() {
                if !matches!(saved_tab, crate::OpenTabSnapshot::File(_)) {
                    // These openers reset the highlighter, which drops the early `Reset`.
                    if let Some(preload) = self.ide_preload.as_mut() {
                        preload.file = None;
                        preload.highlight_version = None;
                    }
                }
                let mut restored = None;
                match saved_tab {
                    crate::OpenTabSnapshot::File(path) => {
                        if path.exists() {
                            let (idx, created) = self.open_pending_file_tab(path);
                            loaded_any |= created;
                            restored = Some(idx);
                        }
                    }
                    crate::OpenTabSnapshot::Empty => {
                        self.open_new_tab();
                        loaded_any = true;
                        restored = Some(self.active_tab);
                    }
                    crate::OpenTabSnapshot::Api {
                        spec_id,
                        route_idx,
                        auth_view,
                    } => {
                        if self
                            .ide_panel
                            .api
                            .specs
                            .iter()
                            .any(|entry| entry.id == spec_id)
                        {
                            if auth_view {
                                self.open_api_auth_tab(spec_id);
                            } else {
                                if let Some(route_idx) = route_idx {
                                    self.open_api_route_with_new_tab(spec_id, route_idx, true);
                                } else {
                                    self.open_api_spec_tab(spec_id);
                                }
                            }
                            loaded_any = true;
                            restored = Some(self.active_tab);
                        }
                    }
                    crate::OpenTabSnapshot::DatabaseTable {
                        connection_id,
                        database_name,
                        table_name,
                    } => {
                        if self.ide_panel.database.connection(connection_id).is_some() {
                            self.open_database_table_tab(connection_id, &database_name, &table_name);
                            loaded_any = true;
                            restored = Some(self.active_tab);
                        }
                    }
                    crate::OpenTabSnapshot::DatabaseQuery {
                        connection_id,
                        database_name,
                        console_id,
                    } => {
                        if self.ide_panel.database.connection(connection_id).is_some() {
                            self.restore_database_query_tab(connection_id, &database_name, console_id);
                            loaded_any = true;
                            restored = Some(self.active_tab);
                        }
                    }
                    crate::OpenTabSnapshot::Pdf { path, page, frac } => {
                        if path.exists() {
                            self.open_pdf_tab_restored(path, page, frac);
                            loaded_any = true;
                            restored = Some(self.active_tab);
                        }
                    }
                }
                if saved_idx == saved_active {
                    target_tab = restored;
                }
            }

            self.startup_trace.mark("ide-tabs-open");
            if loaded_any {
                let target = if has_startup_file {
                    0
                } else {
                    target_tab
                        .unwrap_or(saved_active)
                        .min(self.tabs.len().saturating_sub(1))
                };
                // Deferred startup does not wait for the highlight: the editor area stays blank
                // until `about_to_wait` applies it (see `begin_startup_editor_wait`).
                if target == self.active_tab
                    && self.tabs.get(target).is_some_and(|tab| tab.load != TabLoad::Loaded)
                {
                    // The last opened tab is the target: `switch_to_tab` would return early.
                    self.materialize_pending_tab(target, !defer);
                    if !self.is_highlighted_once {
                        self.begin_initial_tab_highlight(!defer);
                    }
                } else {
                    self.materialize_pending_tab(target, !defer);
                    self.switch_to_tab_options(target, !defer);
                }
                self.save_tabs_state();
                self.startup_trace.mark("ide-switch");
                if !defer && !self.is_highlighted_once {
                    self.wait_for_current_highlight();
                }
            }
        }

        let title = self.base_title.clone();
        if !self.tabs.is_empty() {
            self.tabs[self.active_tab].icon_key =
                crate::app::file_icons::file_icon_key_for_name(&title);
        }

        self.refresh_file_tree();
        self.start_file_watcher();

        if let Some(w) = self.window.as_ref() {
            App::update_window_title(w, &self.base_title, self.editor.is_dirty());
            w.request_redraw();
        }
        self.startup_trace.mark("ide-done");

        if let Some(preload) = self.ide_preload.as_mut() {
            // Taken over by the active tab or superseded by now.
            preload.highlight_version = None;
        }
        self.ide_deferred = IdeDeferred::AwaitFrame;
        if defer {
            self.begin_startup_editor_wait();
        } else {
            self.finish_ide_deferred();
        }
    }

    /// `--ide`: enters IDE mode as soon as the renderer exists (native: end of `resume`), so the
    /// first frame is the full IDE chrome. Loads no file synchronously: the active tab is
    /// highlighted by the worker while `startup_editor_pending` keeps the editor area blank.
    pub(crate) fn enter_ide_on_startup(&mut self) {
        if !self.run_ide_on_startup {
            return;
        }
        self.run_ide_on_startup = false;
        self.enter_ide_mode_deferred();
        self.startup_editor_before_first_frame();
    }

    /// Before the first frame: a highlight already computed during window/GL creation is taken
    /// now, so the first frame has the tabs. Otherwise the first frame is the chrome alone and
    /// the tabs follow no sooner than `STARTUP_EDITOR_REVEAL_DELAY` after it.
    fn startup_editor_before_first_frame(&mut self) {
        if self.startup_editor_pending.is_none() {
            return;
        }
        // Takes a queued result without waiting for one still being computed.
        let version = self.editor.version;
        if self.highlighter.wait_for_first_result(version, std::time::Duration::from_millis(1)) {
            self.apply_highlight_results();
            if self.startup_editor_ready() {
                self.clear_startup_editor_wait();
                return;
            }
        }
        self.startup_editor_reveal_at =
            Some(std::time::Instant::now() + STARTUP_EDITOR_REVEAL_DELAY);
        self.startup_trace.mark("editor-hidden");
    }

    /// Before the window exists: reads the saved tab list and the active file and sends its
    /// highlighter `Reset`, so the worker computes it while the window and GL are created.
    /// `enter_ide_mode` reuses the text and the version.
    pub(crate) fn preload_ide_startup(&mut self) {
        if !self.run_ide_on_startup
            || (self.is_automation_mode() && !self.automation_restores_session())
            || self.scroll_render_bench.is_some()
        {
            return;
        }
        let (tabs, active) = crate::load_open_tabs(true);
        self.preload_ide_session(tabs, active);
    }

    /// `preload_ide_startup` for a known saved session (`active` indexes `tabs`). Reading the
    /// file extension here is the earliest point the active language is known (`ext` below).
    pub(crate) fn preload_ide_session(&mut self, tabs: Vec<crate::OpenTabSnapshot>, active: usize) {
        let has_startup_file =
            self.file_path.is_some() || self.editor.len() > 0 || self.editor.is_dirty();
        let mut file = None;
        if !has_startup_file
            && let Some(crate::OpenTabSnapshot::File(path)) =
                tabs.get(active.min(tabs.len().saturating_sub(1)))
        {
            let path = crate::platform::canonicalize_or_absolutize(path);
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default();
            // Compile the language's queries while the file is read and parsed.
            self.highlighter.prewarm_query(&ext);
            if let Ok(decoded) = crate::platform::read_text_file(&path) {
                let version = self.editor.version.max(self.highlighter.current_version) + 1;
                self.highlighter.reset(version, decoded.text.clone(), ext, 0);
                file = Some(PreloadedFile {
                    path,
                    text: decoded.text,
                    format: decoded.format,
                    version,
                });
            }
        }
        let highlight_version = file.as_ref().map(|file| file.version);
        self.ide_preload = Some(IdePreload { tabs, active, file, highlight_version });
    }

    /// The early `Reset` of `preload_ide_startup` answers for a version the still-empty editor
    /// does not have yet: until the IDE entry takes it over, its result must stay queued.
    pub(crate) fn preloaded_highlight_awaits_ide_entry(&self) -> bool {
        !self.is_ide_mode
            && self
                .ide_preload
                .as_ref()
                .is_some_and(|preload| preload.highlight_version.is_some())
    }

    /// A tab restored from the saved session whose file is not read yet: only its identity
    /// (path, title, extension, icon) is set, so the tab bar is complete. Makes it the active
    /// tab like `open_new_tab`, but never touches the highlighter. A path that already has a
    /// tab (same file under another spelling) is not opened twice. Returns the tab's index and
    /// whether it was created.
    fn open_pending_file_tab(&mut self, path: PathBuf) -> (usize, bool) {
        let path = crate::platform::canonicalize_or_absolutize(&path);
        let key = crate::platform::PathKey::new(&path);
        if let Some(existing) = self.tab_index_for_path(&path, &key) {
            return (existing, false);
        }
        if !self.tabs.is_empty() {
            self.sync_active_tab();
        }
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let file_extension = path
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default();
        let markdown = crate::app::MarkdownTabState::for_file_extension(&file_extension);
        let mut editor = crate::editor::Editor::new(8192);
        editor.version = self.next_tab_highlight_version();
        self.tabs.push(EditorTab {
            editor,
            file_key: Some(key),
            file_path: Some(path.clone()),
            text_file_format: crate::platform::TextFileFormat::default(),
            icon_key: crate::app::file_icons::file_icon_key_for_name(&name),
            file_extension,
            base_title: name,
            markdown,
            pdf: None,
            image: None,
            scroll_y: crate::scroll::ScrollState::new(15.0),
            scroll_x: crate::scroll::ScrollState::new(15.0),
            spans: Vec::new(),
            completions: Vec::new(),
            foldable_ranges: Vec::new(),
            syntax_errors: Vec::new(),
            last_sent_version: u64::MAX,
            search_results: Vec::new(),
            search_current_idx: None,
            is_highlighted_once: false,
            is_highlight_complete: false,
            closing_hints: Default::default(),
            deleted: false,
            load: TabLoad::Pending,
            kind: EditorTabKind::Normal,
        });
        self.active_tab = self.tabs.len() - 1;
        self.sync_active_tab();
        (self.active_tab, true)
    }

    /// Completes the restore of tab `idx` (see `TabLoad`); the writer of `TabLoad` after the
    /// tab was created (with `discard_unreadable_pending_tab`, which it calls). `Pending`:
    /// reads the file into the editor (the early read of `preload_ide_startup` when it is
    /// this file); `with_git` also loads the git base for the gutter, otherwise the tab
    /// becomes `GitBasePending`. `GitBasePending`: loads the git base when `with_git`. The
    /// path is taken from the tab, so a rename in between is followed. A tab that becomes
    /// `Loaded` is opened on a running LSP (a new LSP opens every tab when it is created). A
    /// file that cannot be read leaves an unnamed empty tab (`discard_...`).
    pub(crate) fn materialize_pending_tab(&mut self, idx: usize, with_git: bool) {
        let Some(load) = self.tabs.get(idx).map(|tab| tab.load) else {
            return;
        };
        if load == TabLoad::Loaded {
            return;
        }
        let is_active = idx == self.active_tab;
        let (path, ext) = if is_active {
            (self.file_path.clone(), self.file_extension.clone())
        } else {
            (self.tabs[idx].file_path.clone(), self.tabs[idx].file_extension.clone())
        };
        let Some(path) = path else {
            self.tabs[idx].load = TabLoad::Loaded;
            return;
        };
        if load == TabLoad::GitBasePending {
            if with_git {
                let base = self.git_base_text_for_path(&path);
                let editor = if is_active { &mut self.editor } else { &mut self.tabs[idx].editor };
                editor.set_git_base_text(base);
                if is_active {
                    self.inline_git_popup = None;
                }
                self.tabs[idx].load = TabLoad::Loaded;
                self.notify_lsp_tab_open(idx);
            }
            return;
        }
        let current = if is_active { &self.editor } else { &self.tabs[idx].editor };
        if current.is_dirty() || current.len() > 0 {
            // Edited before it was read (not reachable through the UI): nothing to read over.
            self.tabs[idx].load = TabLoad::Loaded;
            return;
        }
        let old_version = current.version;

        // Matched by path, not by position in the saved list: skipped saved entries shift it.
        let preloaded = self.ide_preload.as_mut().and_then(|preload| {
            preload
                .file
                .take_if(|file| crate::platform::paths_equal(&file.path, &path))
        });
        // The early version is only valid while its highlighter `Reset` is the latest one.
        let early_request_alive = self
            .ide_preload
            .as_ref()
            .is_some_and(|preload| preload.highlight_version.is_some());
        let (text, format, version) = match preloaded {
            Some(file) if early_request_alive => (file.text, file.format, file.version),
            Some(file) => (file.text, file.format, old_version + 1),
            None => match crate::platform::read_text_file(&path) {
                Ok(decoded) => (decoded.text, decoded.format, old_version + 1),
                Err(_) => {
                    self.discard_unreadable_pending_tab(idx, &path);
                    return;
                }
            },
        };
        self.startup_trace.mark("ide-tab-read");
        let mut editor = crate::editor::Editor::new(text.len() + 8192);
        editor.version = version;
        editor.set_clean_text(&text);
        apply_initial_import_folds(&mut editor, &ext, &text);
        if with_git {
            editor.set_git_base_text(self.git_base_text_for_path(&path));
            self.startup_trace.mark("ide-tab-git");
        }
        if is_active {
            self.editor = editor;
            self.text_file_format = format;
        } else {
            let tab = &mut self.tabs[idx];
            tab.editor = editor;
            tab.text_file_format = format;
        }
        self.tabs[idx].load = if with_git { TabLoad::Loaded } else { TabLoad::GitBasePending };
        if with_git {
            self.notify_lsp_tab_open(idx);
        }
    }

    /// The file of a placeholder tab cannot be read (binary, ambiguous encoding, gone). Like
    /// the old open path: the file leaves the recent list and an empty unnamed tab stays. The
    /// path is cleared so the empty buffer can never be saved over the file.
    fn discard_unreadable_pending_tab(&mut self, idx: usize, path: &Path) {
        let untitled = "Безымянный".to_string();
        if idx == self.active_tab {
            self.file_path = None;
            self.file_key = None;
            self.text_file_format = crate::platform::TextFileFormat::default();
            self.base_title = untitled;
            self.file_extension = String::new();
            self.markdown = Default::default();
        } else {
            let tab = &mut self.tabs[idx];
            tab.file_path = None;
            tab.file_key = None;
            tab.text_file_format = crate::platform::TextFileFormat::default();
            tab.base_title = untitled;
            tab.file_extension = String::new();
            tab.markdown = Default::default();
            tab.icon_key = "default_file";
        }
        self.tabs[idx].deleted = false;
        self.tabs[idx].load = TabLoad::Loaded;
        self.recent_files
            .retain(|recent| !crate::platform::paths_equal(recent, path));
        crate::save_recent_files(&self.recent_files);
        self.save_tabs_state();
        if let Some(w) = self.window.as_ref() {
            w.request_redraw();
        }
    }

    /// `didOpen` for tab `idx` on a running LSP (no-op without one or for a tab whose file is
    /// not read yet). A fresh LSP is given every tab by `run_ide_deferred`.
    fn notify_lsp_tab_open(&mut self, idx: usize) {
        if !self.is_ide_mode {
            return;
        }
        let Some(lsp) = self.lsp.as_mut() else {
            return;
        };
        let Some(tab) = self.tabs.get(idx) else {
            return;
        };
        if !matches!(tab.kind, EditorTabKind::Normal) || tab.load == TabLoad::Pending {
            return;
        }
        let (editor, path, ext) = if idx == self.active_tab {
            (&self.editor, &self.file_path, &self.file_extension)
        } else {
            (&tab.editor, &tab.file_path, &tab.file_extension)
        };
        if let Some(path) = path {
            lsp.notify_open(
                path,
                ext,
                &editor.get_full_text(),
                crate::editor::lsp_document_version(editor.version),
            );
        }
    }

    /// First highlight of the tab that just became active without `switch_to_tab` (the last
    /// opened one): the early `Reset` of `preload_ide_startup` when it is still this text,
    /// otherwise a fresh version and `Reset`. `wait` blocks until the result is in (bounded);
    /// without it the result is applied by `about_to_wait`.
    fn begin_initial_tab_highlight(&mut self, wait: bool) {
        let preloaded = self
            .ide_preload
            .as_ref()
            .is_some_and(|preload| preload.highlight_version == Some(self.editor.version));
        if preloaded {
            if let Some(preload) = self.ide_preload.as_mut() {
                preload.highlight_version = None;
            }
            self.editor.sync_edits.clear();
            self.closing_hint_state.invalidate(self.editor.version);
        } else {
            if let Some(preload) = self.ide_preload.as_mut() {
                // This `Reset` supersedes the early one.
                preload.highlight_version = None;
            }
            self.editor.version = self.next_tab_highlight_version();
            while self.highlighter.rx.try_recv().is_ok() {}
            self.reset_highlighter_with_text(self.editor.get_full_text(), false);
        }
        if wait {
            self.wait_for_current_highlight();
        }
    }

    /// Restore work after the first content frame of `enter_ide_mode_deferred`, one step per
    /// call so no frame waits for all of it: each call reads one inactive tab (its file and
    /// git base) or, when none is left, does the final step (LSP, git panel). The caller
    /// repeats while `ide_deferred` is not `None` (`finish_ide_deferred` loops).
    pub(crate) fn run_ide_deferred(&mut self) {
        if self.ide_deferred == IdeDeferred::None {
            return;
        }
        if let Some(idx) = self.tabs.iter().position(|tab| tab.load != TabLoad::Loaded) {
            self.materialize_pending_tab(idx, true);
            return;
        }
        self.ide_deferred = IdeDeferred::None;
        self.ide_preload = None;

        if self.lsp.is_none() {
            let mut lsp = crate::lsp::LspManager::with_ui_waker(
                self.ide_workspaces.clone(),
                self.ui_waker.clone(),
            );
            lsp.set_dart_workspace_analysis_enabled(self.dart_settings.workspace_analysis);
            lsp.set_rust_init_options(crate::lsp::rust_initialization_options(
                self.rust_settings.check_command.config_value(),
            ));
            lsp.set_rust_enabled(self.rust_settings.enabled);
            if !self.dart_settings.enabled {
                lsp.set_server_enabled("dart", false);
            }
            self.lsp = Some(lsp);
            // No tab was opened on it yet; the active one goes last to stay the current document.
            let active = self.active_tab;
            for idx in (0..self.tabs.len()).filter(|&idx| idx != active) {
                self.notify_lsp_tab_open(idx);
            }
            self.notify_lsp_tab_open(self.active_tab);
        } else {
            // `didOpen` of the inactive tabs above made the last one the LSP's current document.
            self.notify_lsp_tab_open(self.active_tab);
        }
        if self.ide_panel.is_open(PanelId::Git) {
            self.refresh_git_panel();
        }
        if let Some(w) = self.window.as_ref() {
            w.request_redraw();
        }
        self.startup_trace.mark("ide-deferred-done");
    }

    /// Runs `run_ide_deferred` to the end (non-deferred entry, headless).
    pub(crate) fn finish_ide_deferred(&mut self) {
        while self.ide_deferred != IdeDeferred::None {
            self.run_ide_deferred();
        }
    }

    /// Starts the blank-editor wait if the active tab is a file whose first highlight is not
    /// applied yet. No-op otherwise: the editor area then shows at once.
    pub(crate) fn begin_startup_editor_wait(&mut self) {
        if self.startup_editor_ready() || self.editor.len() == 0 {
            return;
        }
        // Same rule as `wait_for_current_highlight`: a big file outside the front-priority
        // languages is not waited for, its full parse can take seconds; spans arrive later.
        let is_priority_lang = matches!(self.file_extension.as_str(), "py" | "pyi" | "rs");
        if self.editor.len() > FILE_OPEN_BLOCKING_HIGHLIGHT_MAX_BYTES && !is_priority_lang {
            return;
        }
        self.startup_editor_pending =
            Some(std::time::Instant::now() + FILE_OPEN_LARGE_PRIORITY_HIGHLIGHT_TIMEOUT);
    }

    /// The active tab can be shown: its highlight for the current version is applied, or it
    /// is not a highlighted file tab at all.
    fn startup_editor_ready(&self) -> bool {
        self.is_highlighted_once
            || !self.is_ide_mode
            || self.show_welcome
            || self
                .tabs
                .get(self.active_tab)
                .is_none_or(|tab| !matches!(tab.kind, EditorTabKind::Normal))
    }

    /// `about_to_wait` step after the highlighter was polled: ends the wait once the active
    /// tab is highlighted; at the deadline the highlight is computed synchronously first.
    /// Returns true when the wait ended (the caller redraws).
    pub(crate) fn poll_startup_editor_wait(&mut self) -> bool {
        let Some(deadline) = self.startup_editor_pending else {
            return false;
        };
        if !self.startup_editor_ready() {
            if std::time::Instant::now() < deadline {
                return false;
            }
            self.highlight_current_version_sync();
        }
        if self.startup_editor_reveal_at.is_some_and(|at| std::time::Instant::now() < at) {
            return false;
        }
        self.clear_startup_editor_wait();
        true
    }

    /// The worker did not answer in time: highlight the current version on this thread.
    fn highlight_current_version_sync(&mut self) {
        let version = self.editor.version;
        if self.highlighter.sync_highlight_after_edit(
            version,
            None,
            None,
            None,
            None,
            FILE_OPEN_HIGHLIGHT_TIMEOUT,
        ) {
            self.apply_highlight_results();
        }
    }

    fn clear_startup_editor_wait(&mut self) {
        self.startup_editor_pending = None;
        self.startup_editor_reveal_at = None;
        self.startup_trace.mark("editor-shown");
        if let Some(w) = self.window.as_ref() {
            w.request_redraw();
        }
    }

    /// Input gate of the startup wait for IME / text commits: ignored while the editor would
    /// receive them.
    pub(crate) fn startup_blocks_text_input(&self) -> bool {
        self.startup_editor_pending.is_some() && self.editor_has_input_focus()
    }

    /// Input gate of the startup wait for key presses: ignored when the editor has the input
    /// focus (typing, editing keys, every chord) and for chords that act on tabs or the
    /// document unless the terminal has the focus (save, undo/redo, close tab, tab switching).
    /// App-global chords pass: F1 (settings) and Alt+Q / Alt+W (panel toggles). Releases pass.
    pub(crate) fn startup_blocks_key_input(&self, key_event: &crate::app::keyboard::KeyInput) -> bool {
        let chord = crate::keymap::Chord::from_event(
            crate::platform::CURRENT_PLATFORM,
            key_event,
            self.modifiers,
        );
        self.startup_blocks_key_input_with_chord(key_event, chord)
    }

    pub(crate) fn startup_blocks_key_input_with_chord(
        &self,
        key_event: &crate::app::keyboard::KeyInput,
        chord: Option<crate::keymap::Chord>,
    ) -> bool {
        if self.startup_editor_pending.is_none()
            || key_event.state == winit::event::ElementState::Released
        {
            return false;
        }
        // A focused terminal owns its chords (Ctrl+Z/N/O/Q/T are shell bytes, Ctrl+4 closes a
        // terminal tab).
        if chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::SettingsToggle, chord)
            || self.keymap.hit(crate::keymap::Command::TerminalClose, chord)
            || self.keymap.hit(crate::keymap::Command::TerminalToggleFocus, chord)
            || self.keymap.hit(crate::keymap::Command::ViewToggleProblems, chord))
            || self.ide_panel.terminal_focused
            || self.ide_panel.term_search_focused
        {
            return false;
        }
        let assigned_startup_command = chord.is_some_and(|chord| {
            [crate::keymap::Command::FileSave, crate::keymap::Command::EditorUndo,
                crate::keymap::Command::EditorRedo, crate::keymap::Command::TabsCloseAll,
                crate::keymap::Command::TabsClose,
                crate::keymap::Command::FileOpen, crate::keymap::Command::TabsSwitchNext,
                crate::keymap::Command::TabsSwitchPrevious]
                .into_iter().any(|command| self.keymap.hit(command, chord))
        });
        self.editor_has_input_focus()
            || assigned_startup_command
    }

    /// Input gate of the startup wait for the mouse: a press over the tab bar and the editor
    /// area (right of the activity bar and side panels, above the bottom panel) is ignored,
    /// unless a settings page, dialog or menu is open over it. Releases are never filtered,
    /// so a drag that started elsewhere always ends.
    pub(crate) fn startup_blocks_pointer_input(&self) -> bool {
        if self.startup_editor_pending.is_none()
            || self.show_settings
            || self.modal_dialog_open()
            || self.ide_panel.file_tree_context_menu.is_some()
            || self.lsp_actions_menu.is_some()
        {
            return false;
        }
        let Some(renderer) = self.renderer.as_ref() else {
            return false;
        };
        let s = renderer.scale_factor;
        let editor_x = 48.0 * s + self.ide_panel.visible_left_width(s);
        let bottom_h = if self.ide_panel.any_bottom_open() {
            self.ide_panel.bottom_height * s
        } else {
            0.0
        };
        let bottom_y = crate::render_view::ide_bottom_panel_y(renderer.height, bottom_h, s);
        renderer.last_mouse_x >= editor_x && renderer.last_mouse_y < bottom_y
    }
}
