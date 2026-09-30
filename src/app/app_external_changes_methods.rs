// External file changes: the background probe of the open tabs' files (`start_external_changes_check`),
// applying its result to the tabs (`poll_external_changes`), and the synchronous test variant.
// Included into `app.rs` next to `app_window_external_methods.rs`.

impl App {
    #[cfg(test)]
    pub fn check_external_changes(&mut self) {
        self.sync_active_tab();
        let mut needs_redraw = false;
        let mut active_reloaded = false;
        let active_idx = self.active_tab;
        let mut diff_reloads = Vec::new();
        for (idx, tab) in self.tabs.iter_mut().enumerate() {
            // A placeholder tab (`TabLoad::Pending`) is read from disk when it is needed.
            if !tab.editor.is_dirty() && tab.load != TabLoad::Pending {
                let diff_path = match &tab.kind {
                    EditorTabKind::GitDiff(meta, _) => Some(meta.repo_root.join(&meta.rel_path)),
                    EditorTabKind::Normal
                    | EditorTabKind::ApiClient(_, _)
                    | EditorTabKind::DatabaseTable(_, _)
                    | EditorTabKind::DatabaseQuery(_, _)
                    | EditorTabKind::Pdf => None,
                };
                if let Some(path) = tab.file_path.as_ref().or(diff_path.as_ref())
                    && let Ok(decoded) = crate::platform::read_text_file(path)
                {
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
                        if self.is_ide_mode
                            && let Some(lsp) = &mut self.lsp
                        {
                            lsp.clear_diagnostics_for_path(path);
                            lsp.notify_change(
                                path,
                                &tab.file_extension,
                                &disk_text,
                                crate::editor::lsp_document_version(tab.editor.version),
                            );
                        }
                        if idx == active_idx {
                            active_reloaded = true;
                        }
                        needs_redraw = true;
                    }
                }
            }
        }
        for idx in diff_reloads {
            self.reload_git_diff_tab(idx);
        }
        self.sync_active_tab();
        if active_reloaded {
            while self.highlighter.rx.try_recv().is_ok() {}
            self.reset_highlighter_with_text(self.editor.get_full_text(), false);
            self.wait_for_current_highlight();
            crate::app::mouse::clear_hover_popup(&mut self.hover);
            self.lsp_actions_menu = None;
            self.last_sent_version = self.editor.version;
        }
        if needs_redraw && let Some(w) = self.window.as_ref() {
            w.request_redraw();
        }
    }

    pub fn start_external_changes_check(&mut self) {
        if self.external_changes_rx.is_some() {
            return;
        }
        self.sync_active_tab();
        // Clean tabs are re-read; dirty ones are only probed for existence.
        let clean_tabs = self
            .tabs
            .iter()
            .enumerate()
            .filter_map(|(idx, tab)| {
                let clean = !tab.editor.is_dirty();
                match &tab.kind {
                    EditorTabKind::GitDiff(meta, _) if clean => {
                        Some((idx, meta.repo_root.join(&meta.rel_path), true))
                    }
                    EditorTabKind::GitDiff(_, _) => None,
                    EditorTabKind::Normal => tab.file_path.clone().map(|path| (idx, path, clean)),
                    EditorTabKind::ApiClient(_, _)
                    | EditorTabKind::DatabaseTable(_, _)
                    | EditorTabKind::DatabaseQuery(_, _)
                    | EditorTabKind::Pdf => None,
                }
            })
            .collect::<Vec<_>>();
        self.sync_active_tab();
        if clean_tabs.is_empty() {
            return;
        }
        let (tx, rx) = self.ui_waker.channel();
        match crate::platform::spawn_named("rriter-external-changes", move || {
            let mut changes = Vec::new();
            for (tab_idx, path, read) in clean_tabs {
                if let Some(disk) = crate::app::ExternalDiskState::probe(&path, read) {
                    changes.push(crate::app::ExternalFileChange { tab_idx, path, disk });
                }
            }
            let _ = tx.send(changes);
        }) {
            Ok(_) => self.external_changes_rx = Some(rx),
            Err(err) => {
                self.ide_panel.file_tree_error = Some(format!(
                    "Не удалось запустить проверку внешних изменений: {err}"
                ));
            }
        }
    }

    pub fn poll_external_changes(&mut self) -> bool {
        let Some(rx) = self.external_changes_rx.take() else {
            return false;
        };
        let changes = match rx.try_recv() {
            Ok(changes) => changes,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.external_changes_rx = Some(rx);
                return false;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.ide_panel.file_tree_error = Some(
                    external_changes_disconnect_message().to_string(),
                );
                self.start_external_changes_check();
                return true;
            }
        };
        if changes.is_empty() {
            return false;
        }

        self.sync_active_tab();
        let mut needs_redraw = false;
        let mut active_reloaded = false;
        let active_idx = self.active_tab;
        let mut diff_reloads = Vec::new();
        // (path, exists) for `set_tabs_deleted_under`, applied once the active tab is back in `App`.
        let mut presence = Vec::new();
        for change in changes {
            let (disk_text, text_file_format) = match change.disk {
                crate::app::ExternalDiskState::Text { disk_text, text_file_format } => {
                    presence.push((change.path.clone(), true));
                    (disk_text, text_file_format)
                }
                crate::app::ExternalDiskState::Present => {
                    presence.push((change.path, true));
                    continue;
                }
                crate::app::ExternalDiskState::Missing => {
                    presence.push((change.path, false));
                    continue;
                }
            };
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
                if disk_text != state.worktree_text {
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
                // `materialize_pending_tab` reads the current disk text when it is needed.
                || tab.load == TabLoad::Pending
            {
                continue;
            }
            if tab.editor.text_equals(&disk_text) {
                continue;
            }
            let old_version = tab.editor.version;
            tab.editor = crate::editor::Editor::new(disk_text.len() + 8192);
            tab.editor.version = old_version + 1;
            let _ = tab.editor.insert_str(&disk_text);
            tab.editor.cursor = 0;
            tab.editor.clear_history();
            tab.editor.set_original_text();
            tab.editor.sync_edits.clear();
            tab.closing_hints.invalidate(tab.editor.version);
            tab.text_file_format = text_file_format;
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
                    &disk_text,
                    crate::editor::lsp_document_version(tab.editor.version),
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
        for (path, exists) in presence {
            // A save may have recreated the file after the probe ran.
            let deleted = !exists && !path.is_file();
            needs_redraw |= self.set_tabs_deleted_under(&path, deleted);
        }
        if active_reloaded {
            while self.highlighter.rx.try_recv().is_ok() {}
            self.reset_highlighter_with_text(self.editor.get_full_text(), false);
            crate::app::mouse::clear_hover_popup(&mut self.hover);
            self.lsp_actions_menu = None;
            self.last_sent_version = self.editor.version;
        }
        if needs_redraw && let Some(w) = self.window.as_ref() {
            w.request_redraw();
        }
        needs_redraw
    }
}
