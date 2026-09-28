impl App {
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn handle_file_tree_menu_action(
        &mut self,
        action: FileTreeMenuAction,
        menu: FileTreeContextMenu,
    ) {
        match action {
            FileTreeMenuAction::CreateFile => {
                if let Some(parent_dir) = menu.target_dir {
                    self.open_file_tree_create_dialog(FileTreeCreateKind::File, parent_dir);
                }
            }
            FileTreeMenuAction::CreateDirectory => {
                if let Some(parent_dir) = menu.target_dir {
                    self.open_file_tree_create_dialog(FileTreeCreateKind::Directory, parent_dir);
                }
            }
            FileTreeMenuAction::Paste => {
                if let Some(target_dir) = menu.target_dir {
                    let _ = self.paste_file_tree_clipboard(target_dir);
                }
            }
            FileTreeMenuAction::Delete => {
                if let Some(target_path) = menu.target_path {
                    let paths = self.file_tree_selected_paths_for(&target_path);
                    self.open_file_tree_delete_dialog(paths);
                }
            }
            FileTreeMenuAction::Copy => {
                if let Some(target_path) = menu.target_path {
                    self.copy_file_tree_paths(target_path, FileTreeClipboardMode::Copy);
                }
            }
            FileTreeMenuAction::Cut => {
                if let Some(target_path) = menu.target_path {
                    self.copy_file_tree_paths(target_path, FileTreeClipboardMode::Cut);
                }
            }
            FileTreeMenuAction::Rename => {
                if let Some(path) = self.file_tree_single_selected_path() {
                    self.open_file_tree_rename_dialog(path);
                }
            }
            FileTreeMenuAction::OpenContainedFolder => {
                if let Some(target_path) = menu.target_path {
                    self.open_contained_folder(&target_path, menu.target_is_dir);
                }
            }
            FileTreeMenuAction::ShowInExplorer => {
                if let Some(target_path) = menu.target_path {
                    self.show_path_in_file_tree(&target_path);
                }
            }
            action @ (FileTreeMenuAction::CopyAbsolutePath
            | FileTreeMenuAction::CopyTargetAbsolutePath) => {
                if let Some(target_path) = menu.target_path {
                    let paths = if action == FileTreeMenuAction::CopyTargetAbsolutePath {
                        vec![target_path]
                    } else {
                        self.file_tree_selected_paths_for(&target_path)
                    };
                    let text = paths
                        .iter()
                        .map(|p| p.to_string_lossy())
                        .collect::<Vec<_>>()
                        .join("\n");
                    self.set_clipboard_text(text);
                }
            }
            action @ (FileTreeMenuAction::CopyRelativePath
            | FileTreeMenuAction::CopyTargetRelativePath) => {
                if let Some(target_path) = menu.target_path {
                    let paths = if action == FileTreeMenuAction::CopyTargetRelativePath {
                        vec![target_path]
                    } else {
                        self.file_tree_selected_paths_for(&target_path)
                    };
                    let text = paths
                        .iter()
                        .map(|p| {
                            relative_path_for_workspace(p, &self.ide_workspaces)
                                .to_string_lossy()
                                .into_owned()
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    self.set_clipboard_text(text);
                }
            }
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn open_file_tree_create_dialog(&mut self, kind: FileTreeCreateKind, parent_dir: PathBuf) {
        self.ide_panel.file_tree_create_dialog = Some(FileTreeCreateDialog {
            kind,
            parent_dir,
            editor: Editor::new(256),
            error: None,
        });
        self.ide_panel.file_tree_context_menu = None;
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn open_file_tree_rename_dialog(&mut self, path: PathBuf) {
        if !can_modify_path(&path, &self.ide_workspaces) {
            return;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            return;
        }
        let mut editor = Editor::new(name.len() + 64);
        let _ = editor.insert_str(&name);
        editor.select_all();
        self.ide_panel.file_tree_rename_dialog = Some(FileTreeRenameDialog {
            path,
            editor,
            input_scroll_x: crate::scroll::ScrollState::new(7.0),
            error: None,
        });
        self.ide_panel.file_tree_context_menu = None;
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn submit_file_tree_create_dialog(&mut self) {
        if self.headless_write_blocked() { return; }
        let Some(dialog) = self.ide_panel.file_tree_create_dialog.as_mut() else {
            return;
        };
        let name = dialog.editor.get_full_text().trim().to_string();
        if let Err(err) = validate_child_name(&name) {
            dialog.error = Some(err);
            return;
        }
        if !is_workspace_path(&dialog.parent_dir, &self.ide_workspaces) {
            dialog.error = Some("Путь вне workspace".to_string());
            return;
        }
        let path = dialog.parent_dir.join(&name);
        if crate::platform::path_entry_exists(&path) {
            dialog.error = Some("Уже существует".to_string());
            return;
        }
        let result = match dialog.kind {
            FileTreeCreateKind::File => std::fs::File::create(&path).map(|_| ()),
            FileTreeCreateKind::Directory => std::fs::create_dir(&path),
        };
        if let Err(err) = result {
            dialog.error = Some(err.to_string());
            return;
        }
        self.ide_panel
            .file_tree_expanded
            .insert(dialog.parent_dir.clone());
        self.ide_panel.file_tree_selection.clear();
        self.ide_panel.file_tree_selection.insert(path.clone());
        self.push_file_tree_undo(FileTreeUndoAction::Created { paths: vec![path] });
        self.ide_panel.file_tree_create_dialog = None;
        self.refresh_file_tree();
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn submit_file_tree_rename_dialog(&mut self) {
        if self.headless_write_blocked() { return; }
        let Some(dialog) = self.ide_panel.file_tree_rename_dialog.as_mut() else {
            return;
        };
        let old_path = dialog.path.clone();
        let new_name = dialog.editor.get_full_text().trim().to_string();
        match rename_path(&old_path, &new_name, &self.ide_workspaces) {
            Ok(new_path) => {
                self.update_open_paths_after_file_tree_rename(&old_path, &new_path);
                self.ide_panel.file_tree_selection.clear();
                self.ide_panel.file_tree_selection.insert(new_path.clone());
                self.push_file_tree_undo(FileTreeUndoAction::Renamed { old_path, new_path });
                self.ide_panel.file_tree_rename_dialog = None;
                self.refresh_file_tree();
            }
            Err(err) => {
                dialog.error = Some(err);
            }
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn update_open_paths_after_file_tree_rename(&mut self, old_path: &Path, new_path: &Path) {
        if let Some(current_path) = self.file_path.clone() {
            if let Some(updated) = path_after_rename(&current_path, old_path, new_path) {
                let old_ext = current_path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .unwrap_or("")
                    .to_string();
                self.file_path = Some(updated.clone());
                self.file_key = Some(crate::platform::PathKey::new(&updated));
                self.base_title = updated
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Безымянный".to_string());
                self.file_extension = updated
                    .extension()
                    .map(|ext| ext.to_string_lossy().to_string())
                    .unwrap_or_default();
                if crate::app::is_markdown_extension(&old_ext)
                    != crate::app::is_markdown_extension(&self.file_extension)
                {
                    self.markdown = Default::default();
                }
                if let Some(lsp) = &mut self.lsp {
                    lsp.notify_close(&current_path, &old_ext);
                    let text = self.editor.get_full_text();
                    lsp.notify_open(
                        &updated,
                        &self.file_extension,
                        &text,
                        crate::editor::lsp_document_version(self.editor.version),
                    );
                }
                self.highlighter.reset(
                    self.editor.version,
                    self.editor.get_full_text(),
                    self.file_extension.clone(),
                    self.editor.cursor,
                );
            }
        }

        for tab in &mut self.tabs {
            if let Some(path) = tab.file_path.clone() {
                if let Some(updated) = path_after_rename(&path, old_path, new_path) {
                    tab.file_path = Some(updated.clone());
                    tab.file_key = Some(crate::platform::PathKey::new(&updated));
                    tab.base_title = updated
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "Безымянный".to_string());
                    let old_extension = tab.file_extension.clone();
                    tab.file_extension = updated
                        .extension()
                        .map(|ext| ext.to_string_lossy().to_string())
                        .unwrap_or_default();
                    if crate::app::is_markdown_extension(&old_extension)
                        != crate::app::is_markdown_extension(&tab.file_extension)
                    {
                        tab.markdown = Default::default();
                    }
                    tab.icon_key = crate::app::file_icons::file_icon_key_for_name(&tab.base_title);
                }
            }
        }

        if remap_paths_after_rename(&mut self.recent_files, old_path, new_path) {
            self.recent_files =
                crate::platform::dedup_paths(std::mem::take(&mut self.recent_files));
            crate::save_recent_files(&self.recent_files);
        }
        remap_path_set_after_rename(&mut self.ide_panel.file_tree_selection, old_path, new_path);
        remap_path_set_after_rename(&mut self.ide_panel.file_tree_expanded, old_path, new_path);
        if let Some(clipboard) = &mut self.ide_panel.file_tree_clipboard {
            remap_paths_after_rename(&mut clipboard.paths, old_path, new_path);
            clipboard.paths = crate::platform::dedup_paths(std::mem::take(&mut clipboard.paths));
        }
        remap_paths_after_rename(&mut self.file_tree_watched_dirs, old_path, new_path);
        for path in [
            &mut self.autocomplete_pending_request_path,
            &mut self.autocomplete_detail_request_path,
            &mut self.python_inlay_hint_path,
            &mut self.python_inlay_hint_pending_path,
        ] {
            remap_optional_path_after_rename(path, old_path, new_path);
        }

        if let Some(window) = self.window.as_ref() {
            App::update_window_title(window, &self.base_title, self.editor.is_dirty());
            window.request_redraw();
        }
        self.save_tabs_state();
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn copy_file_tree_paths(&mut self, fallback: PathBuf, mode: FileTreeClipboardMode) {
        let paths = self.file_tree_selected_paths_for(&fallback);
        self.ide_panel.file_tree_clipboard = Some(FileTreeClipboard { mode, paths });
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn paste_file_tree_clipboard(&mut self, target_dir: PathBuf) -> Result<(), String> {
        if self.headless_write_blocked() { return Ok(()); }
        let Some(clipboard) = self.ide_panel.file_tree_clipboard.clone() else {
            return Ok(());
        };
        if !is_workspace_path(&target_dir, &self.ide_workspaces) {
            return Err("Путь вне workspace".to_string());
        }
        if !target_dir.is_dir() {
            return Err("Цель не директория".to_string());
        }
        let result = match clipboard.mode {
            FileTreeClipboardMode::Copy => {
                let copied = copy_paths_to_dir(&clipboard.paths, &target_dir)?;
                self.ide_panel.file_tree_selection.clear();
                self.ide_panel.file_tree_selection.extend(copied.clone());
                if !copied.is_empty() {
                    self.push_file_tree_undo(FileTreeUndoAction::Copied { paths: copied });
                }
                Ok(())
            }
            FileTreeClipboardMode::Cut => {
                let paths = prune_nested_paths(&clipboard.paths);
                for path in &paths {
                    if !can_modify_path(path, &self.ide_workspaces) {
                        return Err("Можно вырезать только элементы внутри workspace".to_string());
                    }
                }
                let pairs = move_paths_to_dir_atomic(&paths, &target_dir)?;
                let mut moved = Vec::with_capacity(pairs.len());
                for (old_path, dst) in &pairs {
                    self.update_open_paths_after_file_tree_rename(old_path, dst);
                    moved.push(dst.clone());
                }
                self.ide_panel.file_tree_selection.clear();
                self.ide_panel.file_tree_selection.extend(moved);
                self.ide_panel.file_tree_clipboard = None;
                if !pairs.is_empty() {
                    self.push_file_tree_undo(FileTreeUndoAction::Moved { pairs });
                }
                Ok(())
            }
        };
        if result.is_ok() {
            self.ide_panel.file_tree_expanded.insert(target_dir);
            self.refresh_file_tree();
        }
        result
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn open_file_tree_delete_dialog(&mut self, paths: Vec<PathBuf>) {
        let paths = prune_nested_paths(&paths);
        if paths.is_empty() {
            return;
        }
        self.ide_panel.file_tree_delete_dialog = Some(FileTreeDeleteDialog { paths, error: None });
        self.ide_panel.file_tree_context_menu = None;
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn confirm_file_tree_delete(&mut self) -> Result<(), String> {
        if self.headless_write_blocked() { return Ok(()); }
        let Some(dialog) = self.ide_panel.file_tree_delete_dialog.as_mut() else {
            return Ok(());
        };
        let paths = dialog.paths.clone();
        match trash_paths(&paths, &self.ide_workspaces) {
            Ok(entries) => {
                self.ide_panel.file_tree_delete_dialog = None;
                if !entries.is_empty() {
                    self.push_file_tree_undo(FileTreeUndoAction::Trashed { entries });
                }
            }
            Err(err) => {
                dialog.error = Some(err.clone());
                return Err(err);
            }
        }
        self.ide_panel.file_tree_selection.clear();
        self.refresh_file_tree();
        Ok(())
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn file_tree_default_paste_dir(&self) -> Option<PathBuf> {
        for node in &self.ide_panel.file_tree_nodes {
            if selection_contains_path(&self.ide_panel.file_tree_selection, &node.path) {
                if node.is_dir {
                    return Some(node.path.clone());
                }
                return node.path.parent().map(Path::to_path_buf);
            }
        }
        self.ide_workspaces.first().cloned()
    }

    pub fn file_tree_single_selected_path(&self) -> Option<PathBuf> {
        if self.ide_panel.file_tree_selection.len() == 1 {
            self.ide_panel.file_tree_selection.iter().next().cloned()
        } else {
            None
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn finish_file_tree_move(&mut self) {
        if self.headless_write_blocked() { return; }
        let Some(dialog) = self.ide_panel.file_tree_move_dialog.as_ref() else {
            return;
        };
        let target_dir = dialog.target_dir.clone();
        let sources = prune_nested_paths(&dialog.sources);
        if !is_workspace_path(&target_dir, &self.ide_workspaces) {
            if let Some(dialog) = self.ide_panel.file_tree_move_dialog.as_mut() {
                dialog.error = Some("Путь вне workspace".to_string());
            }
            return;
        }
        for src in &sources {
            if !can_modify_path(src, &self.ide_workspaces) {
                if let Some(dialog) = self.ide_panel.file_tree_move_dialog.as_mut() {
                    dialog.error =
                        Some("Можно перемещать только элементы внутри workspace".to_string());
                }
                return;
            }
        }
        let pairs = match move_paths_to_dir_atomic(&sources, &target_dir) {
            Ok(pairs) => pairs,
            Err(error) => {
                if let Some(dialog) = self.ide_panel.file_tree_move_dialog.as_mut() {
                    dialog.error = Some(error);
                }
                return;
            }
        };
        let mut moved = Vec::with_capacity(pairs.len());
        for (old_path, dst) in &pairs {
            self.update_open_paths_after_file_tree_rename(old_path, dst);
            moved.push(dst.clone());
        }
        self.ide_panel.file_tree_move_dialog = None;
        self.ide_panel.file_tree_selection.clear();
        self.ide_panel.file_tree_selection.extend(moved);
        self.ide_panel.file_tree_expanded.insert(target_dir);
        if !pairs.is_empty() {
            self.push_file_tree_undo(FileTreeUndoAction::Moved { pairs });
        }
        self.refresh_file_tree();
    }
}
