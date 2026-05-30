use super::*;

impl App {
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn file_tree_dialog_input_index_at(
        &mut self,
        kind: FileTreeDialogInputKind,
        mx: f32,
    ) -> Option<usize> {
        let (text, cursor, parent_dir) = match kind {
            FileTreeDialogInputKind::Create => {
                let dialog = self.ide_panel.file_tree_create_dialog.as_ref()?;
                (
                    dialog.editor.get_full_text(),
                    dialog.editor.cursor,
                    Some(dialog.parent_dir.clone()),
                )
            }
            FileTreeDialogInputKind::Rename => {
                let dialog = self.ide_panel.file_tree_rename_dialog.as_ref()?;
                (
                    dialog.editor.get_full_text(),
                    dialog.editor.cursor,
                    dialog.path.parent().map(Path::to_path_buf),
                )
            }
        };

        let r = self.renderer.as_mut()?;
        let s = r.scale_factor;
        let base_w = (FILE_TREE_DIALOG_W * s).min(r.width - 32.0 * s);
        let mut w = base_w;
        if matches!(kind, FileTreeDialogInputKind::Rename) {
            let base_x = ((r.width - base_w) / 2.0).round();
            let base_input_w = if let Some(parent_dir) = parent_dir.as_ref() {
                let (_, _, input_w) =
                    file_tree_path_input_layout(base_x, base_w, s, parent_dir, |text| {
                        r.measure_ui_width(text, FILE_TREE_DIALOG_INPUT_TEXT_SCALE)
                    });
                input_w
            } else {
                base_w - FILE_TREE_DIALOG_SIDE_PAD * 2.0 * s
            };
            let text_w = r.measure_ui_width(&text, FILE_TREE_DIALOG_INPUT_TEXT_SCALE);
            w = file_tree_rename_dialog_width(base_w, r.width - 32.0 * s, base_input_w, text_w, s);
        }
        let x = ((r.width - w) / 2.0).round();
        let (input_x, input_w) = if let Some(parent_dir) = parent_dir.as_ref() {
            let (_, input_x, input_w) = if matches!(kind, FileTreeDialogInputKind::Rename) {
                file_tree_rename_path_input_layout(x, w, base_w, s, parent_dir, |text| {
                    r.measure_ui_width(text, FILE_TREE_DIALOG_INPUT_TEXT_SCALE)
                })
            } else {
                file_tree_path_input_layout(x, w, s, parent_dir, |text| {
                    r.measure_ui_width(text, FILE_TREE_DIALOG_INPUT_TEXT_SCALE)
                })
            };
            (input_x, input_w)
        } else {
            (
                x + FILE_TREE_DIALOG_SIDE_PAD * s,
                w - FILE_TREE_DIALOG_SIDE_PAD * 2.0 * s,
            )
        };
        let pad_x = 8.0 * s;
        let visible_width = (input_w - pad_x * 2.0).max(0.0);
        let scale = FILE_TREE_DIALOG_INPUT_TEXT_SCALE;
        let scroll_x = if matches!(kind, FileTreeDialogInputKind::Rename) {
            self.ide_panel
                .file_tree_rename_dialog
                .as_ref()
                .map(|dialog| dialog.input_scroll_x.current.round())
                .unwrap_or(0.0)
        } else {
            file_tree_name_input_scroll_x(&text, cursor, visible_width, |ch| {
                r.get_ui_glyph(ch)
                    .map(|g| g.advance * scale)
                    .unwrap_or(10.0 * scale)
            })
        };
        let x_offset = (mx - (input_x + pad_x) + scroll_x).max(0.0);
        Some(file_tree_name_input_hit_index(&text, x_offset, |ch| {
            r.get_ui_glyph(ch)
                .map(|g| g.advance * scale)
                .unwrap_or(10.0 * scale)
        }))
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn set_file_tree_dialog_input_cursor(
        &mut self,
        kind: FileTreeDialogInputKind,
        target_idx: usize,
        reset_anchor: bool,
    ) {
        let mut sync_rename_scroll = false;
        let editor = match kind {
            FileTreeDialogInputKind::Create => self
                .ide_panel
                .file_tree_create_dialog
                .as_mut()
                .map(|dialog| &mut dialog.editor),
            FileTreeDialogInputKind::Rename => self
                .ide_panel
                .file_tree_rename_dialog
                .as_mut()
                .map(|dialog| &mut dialog.editor),
        };
        if let Some(editor) = editor {
            editor.cursor = target_idx;
            if reset_anchor || editor.selection_anchor.is_none() {
                editor.selection_anchor = Some(target_idx);
            }
            sync_rename_scroll = matches!(kind, FileTreeDialogInputKind::Rename);
        }
        if sync_rename_scroll {
            self.sync_file_tree_rename_scroll_target(true);
        }
    }

    fn sync_file_tree_rename_scroll_target(&mut self, immediate: bool) {
        let Some(rect) = self
            .ui_registry
            .rect_for(crate::ui_system::UiId::FileTreeRenameInput)
        else {
            return;
        };
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let s = renderer.scale_factor;
        let visible_w = (rect.2 - 16.0 * s).max(1.0);
        let Some(dialog) = self.ide_panel.file_tree_rename_dialog.as_mut() else {
            return;
        };
        crate::app::sync_one_line_input_scroll_target(
            renderer,
            &dialog.editor,
            &mut dialog.input_scroll_x,
            visible_w,
            FILE_TREE_DIALOG_INPUT_TEXT_SCALE,
            10.0 * s,
            immediate,
        );
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn handle_file_tree_modal_keyboard(&mut self, key_event: &winit::event::KeyEvent) -> bool {
        if self
            .ide_panel
            .api
            .mock_contract_field_delete_dialog
            .is_some()
        {
            if key_event.state != winit::event::ElementState::Pressed {
                return true;
            }
            match key_event.physical_key {
                winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape) => {
                    self.ide_panel.api.mock_contract_field_delete_dialog = None;
                }
                winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Enter)
                | winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::NumpadEnter) => {
                    self.confirm_api_mock_contract_field_delete();
                }
                _ => {}
            }
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            self.last_action = std::time::Instant::now();
            self.last_blink_state = true;
            return true;
        }

        if self.ide_panel.api.mock_route_reset_dialog.is_some() {
            if key_event.state != winit::event::ElementState::Pressed {
                return true;
            }
            match key_event.physical_key {
                winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape) => {
                    self.ide_panel.api.mock_route_reset_dialog = None;
                }
                winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Enter)
                | winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::NumpadEnter) => {
                    self.confirm_api_mock_route_reset();
                }
                _ => {}
            }
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            self.last_action = std::time::Instant::now();
            self.last_blink_state = true;
            return true;
        }

        if self.ide_panel.git.confirm_dialog.is_some() {
            if key_event.state != winit::event::ElementState::Pressed {
                return true;
            }
            match key_event.physical_key {
                winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape) => {
                    self.ide_panel.git.confirm_dialog = None;
                }
                winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Enter)
                | winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::NumpadEnter) => {
                    self.confirm_git_dialog();
                }
                _ => {}
            }
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            self.last_action = std::time::Instant::now();
            self.last_blink_state = true;
            return true;
        }

        if self.ide_panel.file_tree_context_menu.is_some() {
            if key_event.state == winit::event::ElementState::Pressed
                && key_event.physical_key
                    == winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape)
            {
                self.ide_panel.file_tree_context_menu = None;
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
            return true;
        }

        if self.ide_panel.file_tree_move_dialog.is_some() {
            if key_event.state != winit::event::ElementState::Pressed {
                return true;
            }
            match key_event.physical_key {
                winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape) => {
                    self.ide_panel.file_tree_move_dialog = None;
                }
                winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Enter)
                | winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::NumpadEnter) => {
                    self.finish_file_tree_move();
                }
                _ => {}
            }
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            self.last_action = std::time::Instant::now();
            self.last_blink_state = true;
            return true;
        }

        if self.ide_panel.file_tree_delete_dialog.is_some() {
            if key_event.state != winit::event::ElementState::Pressed {
                return true;
            }
            match key_event.physical_key {
                winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape) => {
                    self.ide_panel.file_tree_delete_dialog = None;
                }
                winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Enter)
                | winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::NumpadEnter) => {
                    let _ = self.confirm_file_tree_delete();
                }
                _ => {}
            }
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            self.last_action = std::time::Instant::now();
            self.last_blink_state = true;
            return true;
        }

        if self.ide_panel.file_tree_rename_dialog.is_some() {
            if key_event.state != winit::event::ElementState::Pressed {
                return true;
            }

            let ctrl = self.modifiers.control_key() || self.modifiers.super_key();
            let shift = self.modifiers.shift_key();
            let mut submit = false;
            let mut cancel = false;
            let paste_text = if ctrl
                && key_event.physical_key
                    == winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyV)
            {
                self.get_clipboard_text()
            } else {
                None
            };
            let mut copy_text: Option<String> = None;

            if let Some(dialog) = self.ide_panel.file_tree_rename_dialog.as_mut() {
                dialog.error = None;
                match key_event.physical_key {
                    winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape) => {
                        cancel = true;
                    }
                    winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Enter)
                    | winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::NumpadEnter) => {
                        submit = true;
                    }
                    _ => {
                        copy_text = handle_file_tree_name_editor_input(
                            &mut dialog.editor,
                            key_event.physical_key,
                            key_event.logical_key.to_text(),
                            ctrl,
                            shift,
                            self.modifiers.alt_key(),
                            self.modifiers.super_key(),
                            paste_text,
                        );
                    }
                }
            }

            if let Some(text) = copy_text {
                self.set_clipboard_text(text);
            }
            if cancel {
                self.ide_panel.file_tree_rename_dialog = None;
            } else if submit {
                self.submit_file_tree_rename_dialog();
            } else {
                self.sync_file_tree_rename_scroll_target(false);
            }
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            self.last_action = std::time::Instant::now();
            self.last_blink_state = true;
            return true;
        }

        if self.ide_panel.file_tree_create_dialog.is_none() {
            return false;
        }
        if key_event.state != winit::event::ElementState::Pressed {
            return true;
        }

        let ctrl = self.modifiers.control_key() || self.modifiers.super_key();
        let shift = self.modifiers.shift_key();
        let mut submit = false;
        let mut cancel = false;
        let paste_text = if ctrl
            && key_event.physical_key
                == winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyV)
        {
            self.get_clipboard_text()
        } else {
            None
        };
        let mut copy_text: Option<String> = None;

        if let Some(dialog) = self.ide_panel.file_tree_create_dialog.as_mut() {
            dialog.error = None;
            match key_event.physical_key {
                winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape) => {
                    cancel = true;
                }
                winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Enter)
                | winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::NumpadEnter) => {
                    submit = true;
                }
                _ => {
                    copy_text = handle_file_tree_name_editor_input(
                        &mut dialog.editor,
                        key_event.physical_key,
                        key_event.logical_key.to_text(),
                        ctrl,
                        shift,
                        self.modifiers.alt_key(),
                        self.modifiers.super_key(),
                        paste_text,
                    );
                }
            }
        }

        if let Some(text) = copy_text {
            self.set_clipboard_text(text);
        }
        if cancel {
            self.ide_panel.file_tree_create_dialog = None;
        } else if submit {
            self.submit_file_tree_create_dialog();
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
        self.last_action = std::time::Instant::now();
        self.last_blink_state = true;
        true
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn handle_file_tree_shortcut(
        &mut self,
        physical_key: winit::keyboard::PhysicalKey,
        ctrl: bool,
    ) -> bool {
        if !self.ide_panel.file_tree_focused || self.show_settings {
            return false;
        }
        if ctrl
            && physical_key == winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyZ)
        {
            let _ = self.undo_file_tree_operation();
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            return true;
        }
        if self.ide_panel.file_tree_selection.is_empty() {
            return false;
        }
        if physical_key == winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Delete) {
            let fallback = match self.ide_panel.file_tree_selection.iter().next() {
                Some(path) => path.clone(),
                None => return false,
            };
            let paths = self.file_tree_selected_paths_for(&fallback);
            self.open_file_tree_delete_dialog(paths);
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            return true;
        }
        if physical_key == winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::F2) {
            if let Some(path) = self.file_tree_single_selected_path() {
                self.open_file_tree_rename_dialog(path);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
                return true;
            }
            return false;
        }
        if !ctrl {
            return false;
        }
        let fallback = match self.ide_panel.file_tree_selection.iter().next() {
            Some(path) => path.clone(),
            None => return false,
        };
        match physical_key {
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyC) => {
                self.copy_file_tree_paths(fallback, FileTreeClipboardMode::Copy);
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyX) => {
                self.copy_file_tree_paths(fallback, FileTreeClipboardMode::Cut);
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyV) => {
                if let Some(target_dir) = self.file_tree_default_paste_dir() {
                    let _ = self.paste_file_tree_clipboard(target_dir);
                }
            }
            _ => return false,
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
        true
    }
}
