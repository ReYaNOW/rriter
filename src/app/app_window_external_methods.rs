pub(crate) fn native_picker_spawn_error(action: &str, error: impl std::fmt::Display) -> String {
    format!("Не удалось запустить {action}: {error}")
}

pub(crate) fn external_changes_disconnect_message() -> &'static str {
    "Проверка внешних изменений неожиданно завершилась; выполняется повтор"
}

fn pick_tool_path(
    requests: &crate::platform::ExternalRequestSink,
    kind: crate::platform::ToolKind,
    title: &str,
) -> Option<std::path::PathBuf> {
    if kind == crate::platform::ToolKind::Dart {
        crate::platform::pick_folder(requests, title)
    } else {
        crate::platform::pick_file(requests, title)
    }
}

fn render_should_continue_after_present<T, E>(
    render_suspended: &mut bool,
    result: &Result<T, E>,
) -> bool {
    if result.is_err() {
        *render_suspended = true;
        false
    } else {
        true
    }
}

impl App {
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn update_window_title(window: &crate::platform::WindowHost, base_title: &str, is_dirty: bool) {
        let title = if is_dirty {
            format!("{} * — RRiter", base_title)
        } else {
            format!("{} — RRiter", base_title)
        };
        window.set_title(&title);
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    /// Asks before `action`. A question already on screen keeps its own action
    /// and the new request is dropped; a flow with no visible question (window
    /// still requested, Save-As picker open, answer not yet run) is replaced
    /// whole (`ConfirmDialog::supersede`). Failing to create the dialog window
    /// cancels the request instead of leaving it armed without a dialog.
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn show_action_dialog(&mut self, host: &crate::app::events::host_loop::HostLoop, action: PendingAction) {
        self.cancel_pointer_interactions();
        if !self.confirm_dialog.request(action) && !self.confirm_dialog.supersede(action) {
            return;
        }
        // A superseded flow no longer waits for its protected save.
        self.protected_saves.clear_awaiting_action();

        let Some(event_loop) = host.native() else {
            // Headless: no second window, the dialog is drawn into the main frame.
            self.confirm_dialog.attach_frame();
            self.request_main_redraw();
            return;
        };

        match self.create_confirm_dialog_window(event_loop) {
            Some((window, surface)) => {
                self.confirm_dialog.attach_window(std::sync::Arc::new(window), surface);
            }
            None => self.cancel_pending_action(),
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn create_confirm_dialog_window(
        &self,
        event_loop: &winit::event_loop::ActiveEventLoop,
    ) -> Option<(winit::window::Window, glutin::surface::Surface<glutin::surface::WindowSurface>)> {
        let attrs = crate::platform::apply_window_attributes(winit::window::Window::default_attributes()
            .with_title("Подтверждение — RRiter")
            .with_inner_size(winit::dpi::LogicalSize::new(660.0, 260.0))
            .with_window_level(winit::window::WindowLevel::AlwaysOnTop)
            .with_resizable(false));

        let window = match event_loop.create_window(attrs) {
            Ok(window) => window,
            Err(error) => {
                eprintln!("failed to create confirmation dialog window: {error}");
                return None;
            }
        };
        use glutin::display::GlDisplay;
        use winit::raw_window_handle::HasWindowHandle;
        let Ok(window_handle) = window.window_handle() else {
            eprintln!("failed to obtain confirmation dialog window handle");
            return None;
        };
        let Some(gl_config) = self.gl_config.as_ref() else {
            eprintln!("confirmation dialog requested before GL configuration was ready");
            return None;
        };
        let raw_handle = window_handle.as_raw();
        let display = gl_config.display();
        let scale = window.scale_factor();
        let phys_w = (660.0 * scale).round() as u32;
        let phys_h = (260.0 * scale).round() as u32;
        let surface_attrs =
            glutin::surface::SurfaceAttributesBuilder::<glutin::surface::WindowSurface>::new()
                .build(
                    raw_handle,
                    std::num::NonZeroU32::new(phys_w.max(1))
                        .unwrap_or(std::num::NonZeroU32::MIN),
                    std::num::NonZeroU32::new(phys_h.max(1))
                        .unwrap_or(std::num::NonZeroU32::MIN),
                );
        let surface = unsafe { display.create_window_surface(gl_config, &surface_attrs) };
        let Ok(surface) = surface else {
            eprintln!("failed to create confirmation dialog GL surface");
            return None;
        };
        Some((window, surface))
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) fn present_main_surface(&mut self) -> bool {
        use glutin::surface::GlSurface;
        let (Some(surface), Some(context)) = (self.gl_surface.as_ref(), self.gl_context.as_ref())
        else {
            self.render_suspended = true;
            eprintln!("cannot present RRiter frame: GL surface or context is unavailable");
            return false;
        };
        let result = surface.swap_buffers(context);
        if render_should_continue_after_present(&mut self.render_suspended, &result) {
            true
        } else {
            if let Err(error) = result {
                eprintln!("failed to present RRiter frame: {error}");
            }
            false
        }
    }

    /// Confirmation dialog is open: a second window, or drawn into the headless frame.
    pub(crate) fn modal_dialog_open(&self) -> bool {
        self.confirm_dialog.is_open()
    }

    /// The main window repaints after the confirmation dialog left the screen.
    fn request_main_redraw(&self) {
        if let Some(w) = self.window.as_ref() {
            w.request_redraw();
        }
    }

    pub(crate) fn cancel_pending_action(&mut self) {
        self.confirm_dialog.cancel();
        self.protected_saves.clear_awaiting_action();
        self.clear_pending_markdown_link_action();
        self.request_main_redraw();
    }

    pub fn close_current_file(&mut self) {
        if self.is_ide_mode && self.tabs.len() > 1 {
            self.close_tab_at(self.active_tab);
            return;
        }

        self.cancel_pointer_interactions();
        if self.is_ide_mode && !self.tabs.is_empty() {
            self.prepare_all_tabs_close();
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
        self.closing_hint_state.invalidate(self.editor.version);
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
        self.markdown = Default::default();

        self.scroll_y.reset();
        self.scroll_x.reset();

        if let Some(w) = self.window.as_ref() {
            App::update_window_title(w, &self.base_title, false);
            w.request_redraw();
        }
        self.save_tabs_state();
        self.start_file_watcher();
    }

    pub(crate) fn apply_tool_path_selection(
        &mut self,
        kind: crate::platform::ToolKind,
        path: Option<std::path::PathBuf>,
    ) {
        self.tool_paths.set(kind, path);
        crate::platform::configure_tool_paths(self.tool_paths.clone());
        self.save_current_config();
        if kind == crate::platform::ToolKind::Dart {
            self.restart_dart_server();
        }
        if kind == crate::platform::ToolKind::RustAnalyzer {
            if let Some(lsp) = &mut self.lsp {
                lsp.refresh_rust_resolution();
                self.ide_panel.lsp_servers = lsp.servers_info();
            }
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) fn trigger_settings_tool_picker(&mut self, kind: crate::platform::ToolKind) {
        if !crate::platform::receiver_slot_available(&self.settings_tool_picker_rx) {
            self.tool_installer
                .report_external_error("Окно выбора инструмента уже открыто");
            return;
        }
        let title = format!("Выбрать {}", kind.label());
        if crate::platform::native_dialog_requires_main_thread() {
            let path = pick_tool_path(self.external_requests.sink(), kind, &title);
            if path.is_some() {
                self.apply_tool_path_selection(kind, path);
            }
            return;
        }

        let (tx, rx) = self.ui_waker.channel();
        self.settings_tool_picker_rx = Some(rx);
        let requests = self.external_requests.sink().clone();
        if let Err(err) = crate::platform::spawn_named("rriter-tool-picker", move || {
            let path = pick_tool_path(&requests, kind, &title);
            let _ = tx.send((kind, path));
        }) {
            self.settings_tool_picker_rx = None;
            self.tool_installer.report_external_error(format!(
                "Не удалось запустить выбор инструмента: {err}"
            ));
        }
    }

    pub(crate) fn apply_selected_workspace_folder(&mut self, path: std::path::PathBuf) {
        let path = crate::platform::canonicalize_or_absolutize(&path);
        if !self
            .ide_workspaces
            .iter()
            .any(|existing| crate::platform::paths_equal(existing, &path))
        {
            self.ide_workspaces.push(path.clone());
            self.refresh_dart_tool_state();
        }
        if let Some(lsp) = &mut self.lsp {
            lsp.set_workspaces(self.ide_workspaces.clone());
        }
        self.clear_all_closing_hints();
        self.refresh_dart_closing_hints();
        self.ide_panel.file_tree_expanded.insert(path);
        self.refresh_file_tree();
        self.start_file_watcher();
        self.save_current_config();
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub(crate) fn apply_save_as_path(&mut self, path: std::path::PathBuf) -> bool {
        if !self.save_current_file_as(path) {
            return false;
        }
        if let Some(window) = self.window.as_ref() {
            App::update_window_title(window, &self.base_title, self.editor.is_dirty());
        }
        self.highlighter.reset(
            self.editor.version,
            self.editor.get_full_text(),
            self.file_extension.clone(),
            self.editor.cursor,
        );
        true
    }

    pub(crate) fn handle_save_as_selection(
        &mut self,
        selected: Option<std::path::PathBuf>,
    ) -> bool {
        let saved = selected
            .map(|path| self.apply_save_as_path(path))
            .unwrap_or(false);
        if self.confirm_dialog.waiting_for_save_as() {
            if saved {
                // The picker saved the active tab, whichever queue entry it was.
                self.confirm_dialog.finish_save_as_target(self.active_tab);
                self.continue_pending_tab_saves();
            } else {
                self.cancel_pending_action();
            }
        }
        saved
    }

    pub(crate) fn begin_pending_action_save(&mut self) {
        self.protected_saves.clear_awaiting_action();
        let action = self.confirm_dialog.action();
        if action == PendingAction::ResetKeymap {
            self.confirm_dialog.mark_ready();
            self.request_main_redraw();
            return;
        }
        if matches!(
            action,
            PendingAction::Quit | PendingAction::CloseAllTabs | PendingAction::CloseTab(_)
        ) {
            if self.has_blocking_database_changes_for_pending_action() {
                self.ide_panel.database.global_error = Some(
                    "Сначала завершите или отмените изменения и активные транзакции БД"
                        .to_string(),
                );
                self.cancel_pending_action();
                return;
            }
            let queue = match action {
                PendingAction::CloseTab(index) => self
                    .tab_text_is_dirty(index)
                    .then_some(index)
                    .into_iter()
                    .collect(),
                PendingAction::ResetKeymap => Vec::new(),
                _ => (0..self.tabs.len())
                    .filter(|index| self.tab_text_is_dirty(*index))
                    .collect(),
            };
            self.confirm_dialog.begin_save_as(queue);
            self.request_main_redraw();
            self.continue_pending_tab_saves();
            return;
        }
        if self.file_path.is_none() && !self.active_tab_is_git_diff() {
            self.confirm_dialog.begin_save_as(Vec::new());
            self.request_main_redraw();
            self.trigger_save_as_picker();
            return;
        }
        match self.save_current_file_outcome() {
            SaveOutcome::Saved | SaveOutcome::Unchanged => {
                self.confirm_dialog.mark_ready();
                self.request_main_redraw();
            }
            SaveOutcome::Pending(id) => {
                // The question closes; the action runs in `apply_protected_save_completion`.
                self.confirm_dialog.begin_save_as(Vec::new());
                self.protected_saves.await_for_action(id);
                self.request_main_redraw();
            }
            SaveOutcome::Failed => {}
        }
    }

    pub(crate) fn discard_pending_action_changes(&mut self) {
        self.protected_saves.clear_awaiting_action();
        self.confirm_dialog.mark_ready();
        self.request_main_redraw();
    }

    /// Saves the queued tabs head first; stops at an untitled tab to ask for
    /// its path (the picker's answer resumes here) and on a write failure.
    fn continue_pending_tab_saves(&mut self) {
        loop {
            let Some(index) = self.confirm_dialog.save_as_target() else {
                if self.pending_action_has_dirty_text() {
                    let action = self.confirm_dialog.action();
                    self.confirm_dialog.abort_save_as();
                    self.confirm_dialog.request(action);
                    self.request_main_redraw();
                    return;
                }
                self.confirm_dialog.mark_ready();
                return;
            };
            if index >= self.tabs.len() || !self.tab_text_is_dirty(index) {
                self.confirm_dialog.finish_save_as_target(index);
                continue;
            }
            if index != self.active_tab {
                self.switch_to_tab(index);
            }
            if self.file_path.is_none() {
                self.trigger_save_as_picker();
                return;
            }
            match self.save_current_file_outcome() {
                SaveOutcome::Saved | SaveOutcome::Unchanged => {}
                SaveOutcome::Pending(id) => {
                    // Resumed by `apply_protected_save_completion`; the
                    // tab is then clean (or dirty again and saved once more).
                    self.protected_saves.await_for_action(id);
                    return;
                }
                SaveOutcome::Failed => {
                    self.cancel_pending_action();
                    return;
                }
            }
            self.confirm_dialog.finish_save_as_target(index);
        }
    }

    fn pending_action_has_dirty_text(&self) -> bool {
        match self.confirm_dialog.action() {
            PendingAction::CloseTab(index) => self.tab_text_is_dirty(index),
            PendingAction::Quit | PendingAction::CloseAllTabs if self.is_ide_mode => {
                (0..self.tabs.len()).any(|index| self.tab_text_is_dirty(index))
            }
            PendingAction::Quit | PendingAction::CloseAllTabs => self.editor.is_dirty(),
            PendingAction::CloseFile | PendingAction::OpenFile | PendingAction::OpenLinkedFile => {
                self.editor.is_dirty()
            }
            PendingAction::None | PendingAction::ResetKeymap => false,
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn trigger_file_picker(&mut self) {
        if !crate::platform::receiver_slot_available(&self.open_file_rx) {
            self.ide_panel.file_tree_error =
                Some("Диалог выбора файла уже открыт".to_string());
            return;
        }
        if crate::platform::native_dialog_requires_main_thread() {
            if let Some(file) = crate::platform::pick_file(self.external_requests.sink(), "Открыть файл") {
                self.open_file_in_tab(file, true);
            }
            return;
        }
        let (tx, rx) = self.ui_waker.channel();
        self.open_file_rx = Some(rx);
        let requests = self.external_requests.sink().clone();
        if let Err(err) = crate::platform::spawn_named("rriter-file-picker", move || {
            let file = crate::platform::pick_file(&requests, "Открыть файл");
            let _ = tx.send(file);
        }) {
            self.open_file_rx = None;
            self.ide_panel.file_tree_error =
                Some(native_picker_spawn_error("выбор файла", err));
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn trigger_folder_picker(&mut self) {
        if !crate::platform::receiver_slot_available(&self.open_folder_rx) {
            self.ide_panel.file_tree_error =
                Some("Диалог выбора папки уже открыт".to_string());
            return;
        }
        if crate::platform::native_dialog_requires_main_thread() {
            if let Some(folder) = crate::platform::pick_folder(self.external_requests.sink(), "Выбрать папку") {
                self.apply_selected_workspace_folder(folder);
            }
            return;
        }
        let (tx, rx) = self.ui_waker.channel();
        self.open_folder_rx = Some(rx);
        let requests = self.external_requests.sink().clone();
        if let Err(err) = crate::platform::spawn_named("rriter-folder-picker", move || {
            let folder = crate::platform::pick_folder(&requests, "Выбрать папку");
            let _ = tx.send(folder);
        }) {
            self.open_folder_rx = None;
            self.ide_panel.file_tree_error =
                Some(native_picker_spawn_error("выбор папки", err));
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn trigger_save_as_picker(&mut self) {
        if !crate::platform::receiver_slot_available(&self.save_file_rx) {
            self.ide_panel.file_tree_error =
                Some("Диалог сохранения уже открыт".to_string());
            return;
        }
        if crate::platform::native_dialog_requires_main_thread() {
            let selected = crate::platform::save_file(
                self.external_requests.sink(),
                "Сохранить файл как...",
                "Безымянный.txt",
            );
            self.handle_save_as_selection(selected);
            return;
        }
        let (tx, rx) = self.ui_waker.channel();
        self.save_file_rx = Some(rx);
        let requests = self.external_requests.sink().clone();
        if let Err(err) = crate::platform::spawn_named("rriter-save-picker", move || {
            let file = crate::platform::save_file(&requests, "Сохранить файл как...", "Безымянный.txt");
            let _ = tx.send(file);
        }) {
            self.save_file_rx = None;
            // No picker will answer: the confirmation flow waiting for it ends here.
            self.confirm_dialog.abort_save_as();
            self.ide_panel.file_tree_error =
                Some(native_picker_spawn_error("выбор пути сохранения", err));
        }
    }

    /// `true` only when the file is on disk now; a protected save that went to
    /// the background returns `false` (see `save_current_file_outcome`).
    pub fn save_current_file(&mut self) -> bool {
        self.save_current_file_outcome() == SaveOutcome::Saved
    }

    /// Saves the active document. A plain write that is refused with
    /// `PermissionDenied` (or a path whose protected save is still running, so
    /// writes to one path never race) becomes a background elevated save of a
    /// snapshot of the current text and format.
    pub(crate) fn save_current_file_outcome(&mut self) -> SaveOutcome {
        if self.active_tab_is_git_diff() {
            return if self.save_active_git_diff() {
                SaveOutcome::Saved
            } else {
                SaveOutcome::Failed
            };
        }
        // A PDF tab keeps an empty hidden editor; writing it would replace the document with nothing.
        // Report it like every other failed save (`file_tree_error`), otherwise Ctrl+S is silent.
        if self.active_pdf_tab().is_some() {
            self.ide_panel.file_tree_error =
                Some("PDF нельзя сохранить: документ открыт только для просмотра".to_string());
            return SaveOutcome::Failed;
        }
        if self.tabs.get(self.active_tab).is_some_and(|tab| tab.kind.is_image()) {
            self.ide_panel.file_tree_error = Some("Изображение открыто только для просмотра".to_owned());
            return SaveOutcome::Failed;
        }
        let Some(path) = self.file_path.clone() else {
            self.trigger_save_as_picker();
            return SaveOutcome::Failed;
        };
        // An unread placeholder tab (`TabLoad::Pending`) has an empty buffer: never write it.
        if self.tabs.get(self.active_tab).is_some_and(|tab| tab.load == TabLoad::Pending) {
            return SaveOutcome::Unchanged;
        }
        if self.headless_write_blocked() {
            return SaveOutcome::Failed;
        }
        let content = self.editor.get_full_text();
        if self
            .protected_saves
            .is_pending(&crate::platform::PathKey::new(&path))
        {
            return self.start_protected_save(path, content);
        }
        match crate::platform::write_text_file(&path, &content, self.text_file_format) {
            Ok(()) => {
                self.ide_panel.file_tree_error = None;
                self.editor.mark_saved();
                self.set_tabs_deleted_under(&path, false);
                self.reconcile_saved_current_file_git_index();
                if self.is_ide_mode
                    && let Some(lsp) = &mut self.lsp
                {
                    lsp.notify_saved(&path, &self.file_extension);
                }
                self.save_tabs_state();
                SaveOutcome::Saved
            }
            // Refused elevation falls through to the plain write-error path below.
            Err(error)
                if error.kind() == std::io::ErrorKind::PermissionDenied
                    && self.protected_saves.elevation_allowed() =>
            {
                self.start_protected_save(path, content)
            }
            Err(error) => {
                self.ide_panel.file_tree_error =
                    Some(format!("Не удалось сохранить {}: {error}", path.display()));
                SaveOutcome::Failed
            }
        }
    }

    fn start_protected_save(&mut self, path: PathBuf, content: String) -> SaveOutcome {
        let format = self.text_file_format;
        match self
            .protected_saves
            .enqueue(&self.ui_waker, path.clone(), content, format)
        {
            Ok(id) => {
                self.ide_panel.file_tree_error = None;
                SaveOutcome::Pending(id)
            }
            Err(message) => {
                self.ide_panel.file_tree_error =
                    Some(format!("Не удалось сохранить {}: {message}", path.display()));
                SaveOutcome::Failed
            }
        }
    }

    /// Applies finished protected saves; returns whether anything changed on
    /// screen. Call it from the host loop's background-result drain.
    pub(crate) fn poll_protected_saves(&mut self) -> bool {
        if !self.protected_saves.has_pending() {
            return false;
        }
        let completions = self.protected_saves.poll(&self.ui_waker);
        if completions.is_empty() {
            return false;
        }
        for completion in completions {
            self.apply_protected_save_completion(completion);
        }
        true
    }

    fn apply_protected_save_completion(&mut self, done: ProtectedSaveCompletion) {
        let awaited = self.protected_saves.take_awaited(done.id);
        match &done.result {
            Ok(()) => {
                self.mark_protected_save_written(&done);
                self.set_tabs_deleted_under(&done.path, false);
                if awaited && self.confirm_dialog.waiting_for_save_as() {
                    self.continue_pending_tab_saves();
                }
            }
            Err(message) => {
                self.ide_panel.file_tree_error =
                    Some(format!("Не удалось сохранить {}: {message}", done.path.display()));
                // The tab stays open with the error: the confirmed action is dropped.
                if awaited && self.confirm_dialog.waiting_for_save_as() {
                    self.cancel_pending_action();
                }
            }
        }
    }

    /// The written snapshot becomes the saved baseline of whichever tab shows
    /// the file now (active or not); a tab closed meanwhile is simply gone.
    fn mark_protected_save_written(&mut self, done: &ProtectedSaveCompletion) {
        if self.file_key.as_ref() == Some(&done.key) {
            self.editor.mark_saved_as(&done.text);
            self.reconcile_saved_current_file_git_index();
            if self.is_ide_mode
                && let Some(lsp) = &mut self.lsp
            {
                lsp.notify_saved(&done.path, &self.file_extension);
            }
            if let Some(window) = self.window.as_ref() {
                App::update_window_title(window, &self.base_title, self.editor.is_dirty());
            }
        } else {
            let active_tab = self.active_tab;
            let Some(tab) = self
                .tabs
                .iter_mut()
                .enumerate()
                .find(|(index, tab)| *index != active_tab && tab.file_key.as_ref() == Some(&done.key))
                .map(|(_, tab)| tab)
            else {
                return;
            };
            tab.editor.mark_saved_as(&done.text);
            if self.is_ide_mode
                && let Some(lsp) = &mut self.lsp
            {
                lsp.notify_saved(&done.path, &tab.file_extension);
            }
        }
        self.save_tabs_state();
    }

    /// App exit: bounded wait for running protected saves, then the pkexec
    /// process tree is stopped (`ProtectedSaves::shutdown`).
    pub(crate) fn shutdown_protected_saves(&mut self) {
        self.protected_saves.shutdown();
    }

    /// Headless without --allow-writes: disk-mutating UI actions are refused with the readonly notice.
    pub(crate) fn headless_write_blocked(&mut self) -> bool {
        if crate::platform::editor_writes_allowed(crate::platform::headless_policy()) {
            return false;
        }
        self.show_readonly_notice();
        true
    }

    fn write_current_text_to_path(&mut self, path: &Path, content: &str) -> bool {
        if self.headless_write_blocked() {
            return false;
        }
        if self
            .protected_saves
            .is_pending(&crate::platform::PathKey::new(path))
        {
            self.ide_panel.file_tree_error =
                Some("Защищённое сохранение этого файла ещё выполняется".to_string());
            return false;
        }
        let result = match crate::platform::write_text_file(path, content, self.text_file_format) {
            Ok(()) => Ok(()),
            // Refused elevation falls through to the plain write-error path below.
            Err(error)
                if error.kind() == std::io::ErrorKind::PermissionDenied
                    && self.protected_saves.elevation_allowed() =>
            {
                // Save As stays synchronous: the document identity changes only
                // after the write, which a background save cannot promise here.
                self.protected_saves.write_synchronously(
                    path,
                    content,
                    self.text_file_format,
                    &std::sync::atomic::AtomicBool::new(false),
                )
            }
            Err(error) => Err(error),
        };
        match result {
            Ok(()) => {
                self.ide_panel.file_tree_error = None;
                true
            }
            Err(error) => {
                self.ide_panel.file_tree_error =
                    Some(format!("Не удалось сохранить {}: {error}", path.display()));
                false
            }
        }
    }

    /// Saves to a new path and changes the active document identity only after
    /// the replacement has completed successfully.
    pub fn save_current_file_as(&mut self, path: PathBuf) -> bool {
        if self.active_tab_is_git_diff() {
            return false;
        }
        // The async picker can return after the user switched to a PDF tab; its hidden editor is empty.
        if self.active_pdf_tab().is_some() {
            return false;
        }
        if self.tabs.get(self.active_tab).is_some_and(|tab| tab.kind.is_image()) {
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
        if crate::app::is_markdown_extension(&old_extension)
            != crate::app::is_markdown_extension(&self.file_extension)
        {
            self.markdown = Default::default();
        }
        self.editor.mark_saved();
        self.set_tabs_deleted_under(&path, false);

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
                crate::editor::lsp_document_version(self.editor.version),
            );
            lsp.notify_saved(&path, &self.file_extension);
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
        self.refresh_dart_closing_hints();
    }

    pub(crate) fn refresh_dart_closing_hints(&mut self) {
        let revision = self.editor.version;
        let settings = self.closing_hint_settings;
        if self.file_extension != "dart"
            || settings.mode != crate::languages::dart::ClosingHintMode::DartServerAndBlocks
        {
            self.closing_hint_state
                .replace_syntax(revision, Vec::new(), settings);
            return;
        }

        let text = self.editor.get_full_text();
        let hints = {
            let Some(tree) = self.highlighter.syntax_tree_for(revision, "dart") else {
                return;
            };
            crate::languages::dart::local_closing_hints(&text, tree, revision, settings)
        };
        self.closing_hint_state
            .replace_syntax(revision, hints, settings);
    }

    pub(crate) fn apply_server_closing_labels(
        &mut self,
        path: &std::path::Path,
        labels: &[crate::lsp::LspClosingLabel],
    ) -> bool {
        let settings = self.closing_hint_settings;
        if self.file_extension == "dart"
            && self
                .file_path
                .as_deref()
                .is_some_and(|current| crate::platform::paths_equal(current, path))
        {
            let revision = self.editor.version;
            let text = self.editor.get_full_text();
            let hints = crate::languages::dart::server_closing_hints(&text, revision, labels);
            self.closing_hint_state
                .replace_server(revision, hints, settings);
            return true;
        }

        for (index, tab) in self.tabs.iter_mut().enumerate() {
            if index == self.active_tab
                || tab.file_extension != "dart"
                || tab
                    .file_path
                    .as_deref()
                    .is_none_or(|current| !crate::platform::paths_equal(current, path))
            {
                continue;
            }
            let revision = tab.editor.version;
            let text = tab.editor.get_full_text();
            let hints = crate::languages::dart::server_closing_hints(&text, revision, labels);
            tab.closing_hints.replace_server(revision, hints, settings);
            return false;
        }
        false
    }

    pub(crate) fn clear_all_closing_hints(&mut self) {
        self.closing_hint_state.invalidate(self.editor.version);
        for tab in &mut self.tabs {
            tab.closing_hints.invalidate(tab.editor.version);
        }
    }

    pub(crate) fn set_closing_hint_settings(
        &mut self,
        settings: crate::languages::dart::ClosingHintSettings,
    ) {
        self.closing_hint_settings = settings;
        self.closing_hint_state.apply_settings(settings);
        for tab in &mut self.tabs {
            tab.closing_hints.apply_settings(settings);
        }
        self.refresh_dart_closing_hints();
    }

    pub(crate) fn sync_dart_closing_hint_settings(&mut self) {
        self.set_closing_hint_settings(self.dart_settings.closing_hint_settings());
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
        self.wait_for_current_highlight_with_timeouts(timeout, FILE_OPEN_HIGHLIGHT_TIMEOUT);
    }

    fn wait_for_current_highlight_with_timeouts(
        &mut self,
        worker_timeout: std::time::Duration,
        sync_fallback_timeout: std::time::Duration,
    ) {
        let version = self.editor.version;
        let mut worker_ready = self
            .highlighter
            .wait_for_first_result(version, worker_timeout);
        // A worker that already runs exactly this version has a head start on any sync pass
        // started now (the sync pass would redo the same parse from scratch on this
        // thread): keep waiting, within the long budget. A job still queued behind a stale
        // one falls back to the sync pass at once.
        if !worker_ready && self.highlighter.is_worker_processing(version) {
            worker_ready = self.highlighter.wait_for_first_result(
                version,
                FILE_OPEN_LARGE_PRIORITY_HIGHLIGHT_TIMEOUT.saturating_sub(worker_timeout),
            );
        }
        // The worker may have answered between the timeout and the `is_worker_processing`
        // check: take that result instead of redoing the parse on this thread.
        if !worker_ready {
            worker_ready = self.highlighter.poll(version);
        }
        let sync_ready = !worker_ready
            && self.highlighter.sync_highlight_after_edit(
                version,
                None,
                None,
                None,
                None,
                sync_fallback_timeout,
            );
        if worker_ready || sync_ready {
            self.apply_highlight_results();
        }
    }

    fn reset_highlighter_with_text(&mut self, text: String, _seed_immediately: bool) {
        self.editor.sync_edits.clear();
        self.closing_hint_state.invalidate(self.editor.version);
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
            let tab_bar_h = self.editor_top_inset(scale);
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
                self.startup_trace.mark("ide-tab-read");
                self.refresh_current_editor_git_base();
                self.startup_trace.mark("ide-tab-git");
                let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                self.base_title = file_name.into_owned();
                self.file_extension = path
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_default();
                self.markdown = crate::app::MarkdownTabState::for_file_extension(&self.file_extension);
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

                self.scroll_y.reset();
                self.scroll_x.reset();

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
                            crate::editor::lsp_document_version(self.editor.version),
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

}


#[cfg(test)]
mod app_window_failure_regression_tests {
    use super::*;

    #[test]
    fn bug_64_swap_failure_suspends_rendering_without_panicking() {
        let mut suspended = false;
        let failed: Result<(), &str> = Err("context lost");
        assert!(!render_should_continue_after_present(&mut suspended, &failed));
        assert!(suspended);

        let mut suspended = false;
        let success: Result<(), &str> = Ok(());
        assert!(render_should_continue_after_present(&mut suspended, &success));
        assert!(!suspended);
    }

    #[test]
    fn bug_65_confirmation_surface_creation_has_no_unwrap_path() {
        let source = include_str!("app_window_external_methods.rs");
        let production = source.split("#[cfg(test)]").next().unwrap_or(source);
        assert!(production.contains("let Ok(window_handle) = window.window_handle() else"));
        assert!(production.contains("let Ok(surface) = surface else"));
        assert!(!production.contains("create_window_surface(gl_config, &surface_attrs).unwrap"));
        assert!(!production.contains("window.window_handle().unwrap"));
    }

    #[test]
    fn bug_66_confirmation_context_and_present_errors_close_only_the_dialog() {
        let source = include_str!("events.rs");
        let gl_error = source
            .find("confirmation dialog disabled after GL error")
            .expect("GL error branch of the dialog redraw");
        // The branch drops the dialog together with its armed action, not the app.
        let after_error = source[gl_error..].lines().nth(1).unwrap_or_default();
        assert_eq!(after_error.trim(), "self.cancel_pending_action();");
        assert!(!source.contains("make_current(gl_surface).unwrap"));
        assert!(!source.contains("swap_buffers(gl_context).unwrap"));
    }
}

#[cfg(test)]
mod protected_save_app_regression_tests {
    use super::*;
    use crate::app::app_behavior_tests::{editor_with, tab_with, test_app};
    use crate::app::protected_save::ProtectedWriter;
    use crate::platform::TextFileFormat;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::{Duration, Instant};

    fn blocked_writer() -> (ProtectedWriter, mpsc::Sender<()>, Arc<AtomicUsize>) {
        let (release, release_rx) = mpsc::channel();
        let release_rx = Arc::new(Mutex::new(release_rx));
        let calls = Arc::new(AtomicUsize::new(0));
        let writer: ProtectedWriter = {
            let release_rx = Arc::clone(&release_rx);
            let calls = Arc::clone(&calls);
            Arc::new(move |_path, _text, _format, _cancel| {
                calls.fetch_add(1, Ordering::SeqCst);
                if let Ok(receiver) = release_rx.lock() {
                    let _ = receiver.recv();
                }
                Ok(())
            })
        };
        (writer, release, calls)
    }

    fn wait_for_calls(calls: &AtomicUsize, expected: usize) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while calls.load(Ordering::SeqCst) < expected {
            assert!(Instant::now() < deadline, "writer was not called {expected} times");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn wait_for_protected_saves(app: &mut App, expected_calls: usize, calls: &AtomicUsize) {
        wait_for_calls(calls, expected_calls);
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.protected_saves.has_pending() {
            app.poll_protected_saves();
            assert!(Instant::now() < deadline, "protected save did not finish");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn edit_during_pending_action_save_reopens_confirmation() {
        let Some(mut app) = test_app() else { return; };
        let path = PathBuf::from("/protected/pending-action.txt");
        app.file_path = Some(path.clone());
        app.file_key = Some(crate::platform::PathKey::new(&path));
        app.editor = editor_with("base");
        let _ = app.editor.insert_str(" edit");
        let (writer, release, calls) = blocked_writer();
        app.protected_saves = crate::app::ProtectedSaves::with_writer(writer);
        app.protected_saves
            .enqueue(&app.ui_waker, path, "first".into(), TextFileFormat::default())
            .unwrap();
        wait_for_calls(&calls, 1);

        assert!(app.confirm_dialog.request(PendingAction::CloseFile));
        app.confirm_dialog.attach_frame();
        app.begin_pending_action_save();
        let _ = app.editor.insert_str(" changed while saving");
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while calls.load(Ordering::SeqCst) < 2 {
            app.poll_protected_saves();
            assert!(Instant::now() < deadline, "queued writer was not started");
            std::thread::sleep(Duration::from_millis(2));
        }
        release.send(()).unwrap();
        wait_for_protected_saves(&mut app, 2, &calls);

        assert_eq!(app.confirm_dialog.needs_window(), Some(PendingAction::CloseFile));
        assert!(app.editor.is_dirty());
    }

    #[test]
    fn save_as_to_path_with_pending_protected_save_shows_error_without_write() {
        let Some(mut app) = test_app() else { return; };
        let path = PathBuf::from("/protected/save-as-pending.txt");
        let (writer, release, calls) = blocked_writer();
        app.protected_saves = crate::app::ProtectedSaves::with_writer(writer);
        app.protected_saves
            .enqueue(&app.ui_waker, path.clone(), "existing".into(), TextFileFormat::default())
            .unwrap();
        wait_for_calls(&calls, 1);

        assert!(!app.save_current_file_as(path));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            app.ide_panel.file_tree_error.as_deref(),
            Some("Защищённое сохранение этого файла ещё выполняется")
        );
        release.send(()).unwrap();
        wait_for_protected_saves(&mut app, 1, &calls);
    }

    #[test]
    fn clean_tab_with_pending_protected_save_still_asks_before_close() {
        let Some(mut app) = test_app() else { return; };
        let path = "/protected/clean-pending.txt";
        app.is_ide_mode = true;
        app.file_path = Some(PathBuf::from(path));
        app.file_key = Some(crate::platform::PathKey::new(std::path::Path::new(path)));
        app.editor = editor_with("clean");
        app.tabs = vec![tab_with("clean-pending.txt", Some(path), "clean")];
        app.active_tab = 0;
        let (writer, release, calls) = blocked_writer();
        app.protected_saves = crate::app::ProtectedSaves::with_writer(writer);
        app.protected_saves
            .enqueue(&app.ui_waker, PathBuf::from(path), "clean".into(), TextFileFormat::default())
            .unwrap();
        wait_for_calls(&calls, 1);

        app.close_tab_at(0);
        assert_eq!(app.confirm_dialog.action(), PendingAction::CloseTab(0));
        app.confirm_dialog.attach_frame();
        app.begin_pending_action_save();
        assert!(app.confirm_dialog.waiting_for_save_as());
        release.send(()).unwrap();
        wait_for_protected_saves(&mut app, 1, &calls);
        assert_eq!(app.confirm_dialog.take_ready(), Some(PendingAction::CloseTab(0)));
    }
}
