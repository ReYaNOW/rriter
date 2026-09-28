impl Renderer {
    pub(crate) fn draw_file_tree_overlays(
        &mut self,
        ide_panel: &crate::app::IdePanelState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        blink_alpha: f32,
    ) -> bool {
        let s = self.scale_factor;
        let mut wants_pointer = false;
        let mut label_scratch = String::new();
        if crate::app::file_tree::file_tree_overlay_active_for_panel(ide_panel) {
            ui_registry.mark_overlay_start();
            if crate::app::file_tree::file_tree_modal_overlay_active_for_panel(ide_panel) {
                ui_registry.reset_cursor_state();
            }
        }

        if let Some(menu) = &ide_panel.file_tree_context_menu {
            wants_pointer |= self.draw_animated_context_menu(
                menu.x,
                menu.y,
                menu.opened_at,
                menu.entries.len(),
                |idx| menu.entries[idx].label(),
                crate::ui_system::UiId::FileTreeMenuItem,
                |idx| file_tree_menu_separator_before(&menu.entries, idx),
                ui_registry,
                mx,
                my,
            );
        }

        if let Some(dialog) = &ide_panel.file_tree_create_dialog {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.42]);
            let w = (crate::app::file_tree::FILE_TREE_DIALOG_W * s).min(self.width - 32.0 * s);
            let h = 178.0 * s;
            let x = ((self.width - w) / 2.0).round();
            let y = ((self.height - h) / 2.0).round();
            let side_pad = crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * s;
            self.draw_file_tree_dialog_shell(x, y, w, h, s);
            self.draw_string_scaled(
                dialog.kind.title(),
                x + side_pad,
                y + 38.0 * s,
                self.theme.fg,
                1.0,
            );

            let path_scale = crate::app::file_tree::FILE_TREE_DIALOG_INPUT_TEXT_SCALE;
            let (path_prefix, input_x, input_w) =
                crate::app::file_tree::file_tree_path_input_layout(
                    x,
                    w,
                    s,
                    &dialog.parent_dir,
                    |text| self.measure_ui_width(text, path_scale),
                );
            let input_y = y + 66.0 * s;
            let input_h = 34.0 * s;
            self.draw_string_scaled(
                &path_prefix,
                x + side_pad,
                input_y + 23.0 * s,
                [0.55, 0.57, 0.64, 1.0],
                path_scale,
            );
            ui_registry.register_text_input(
                crate::ui_system::UiId::FileTreeCreateInput,
                input_x,
                input_y,
                input_w,
                input_h,
                mx,
                my,
            );
            let create_text = dialog.editor.get_full_text();
            let create_scroll_x = crate::app::file_tree::file_tree_name_input_scroll_x(
                &create_text,
                dialog.editor.cursor,
                (input_w - 16.0 * s).max(0.0),
                |ch| {
                    let char_to_render = if ch == '\n' { '↵' } else { ch };
                    self.get_ui_glyph(char_to_render)
                        .map(|g| g.advance * path_scale)
                        .unwrap_or(10.0 * path_scale)
                },
            );
            self.draw_file_tree_dialog_input(
                &dialog.editor,
                input_x,
                input_y,
                input_w,
                input_h,
                create_scroll_x,
                blink_alpha,
            );
            if let Some(error) = &dialog.error {
                self.draw_string_scaled(
                    error,
                    x + side_pad,
                    input_y + input_h + 20.0 * s,
                    self.theme.diag_error,
                    0.8,
                );
            }

            let btn_w = 112.0 * s;
            let btn_h = 32.0 * s;
            let (ok_x, cancel_x) = centered_dialog_button_positions(x, w, btn_w, 10.0 * s);
            let btn_y = y + h - 64.0 * s;
            wants_pointer |= self.draw_file_tree_dialog_buttons(
                ui_registry,
                [
                (
                    crate::ui_system::UiId::FileTreeCreateConfirm,
                    "Создать",
                    ok_x,
                ),
                (
                    crate::ui_system::UiId::FileTreeCreateCancel,
                    "Отмена",
                    cancel_x,
                ),
                ],
                btn_y,
                btn_w,
                btn_h,
                s,
                mx,
                my,
            );
        }

        if let Some(dialog) = &ide_panel.file_tree_rename_dialog {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.42]);
            let base_w = (crate::app::file_tree::FILE_TREE_DIALOG_W * s)
                .min(self.width - 32.0 * s);
            let path_scale = crate::app::file_tree::FILE_TREE_DIALOG_INPUT_TEXT_SCALE;
            let base_x = ((self.width - base_w) / 2.0).round();
            let base_input_w = if let Some(parent_dir) = dialog.path.parent() {
                let (_, _, input_w) =
                    crate::app::file_tree::file_tree_path_input_layout(
                        base_x,
                        base_w,
                        s,
                        parent_dir,
                        |text| self.measure_ui_width(text, path_scale),
                    );
                input_w
            } else {
                base_w - crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * 2.0 * s
            };
            let rename_text = dialog.editor.get_full_text();
            let rename_text_w = self.measure_ui_width(&rename_text, path_scale);
            let w = crate::app::file_tree::file_tree_rename_dialog_width(
                base_w,
                self.width - 32.0 * s,
                base_input_w,
                rename_text_w,
                s,
            );
            let h = 178.0 * s;
            let x = ((self.width - w) / 2.0).round();
            let y = ((self.height - h) / 2.0).round();
            let side_pad = crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * s;
            self.draw_file_tree_dialog_shell(x, y, w, h, s);
            self.draw_string_scaled(
                "Переименовать",
                x + side_pad,
                y + 38.0 * s,
                self.theme.fg,
                1.0,
            );

            let (path_prefix, input_x, input_w) = if let Some(parent_dir) = dialog.path.parent() {
                crate::app::file_tree::file_tree_rename_path_input_layout(
                    x,
                    w,
                    base_w,
                    s,
                    parent_dir,
                    |text| self.measure_ui_width(text, path_scale),
                )
            } else {
                (String::new(), x + side_pad, w - side_pad * 2.0)
            };
            let input_y = y + 66.0 * s;
            let input_h = 34.0 * s;
            if !path_prefix.is_empty() {
                self.draw_string_scaled(
                    &path_prefix,
                    x + side_pad,
                    input_y + 23.0 * s,
                    [0.55, 0.57, 0.64, 1.0],
                    path_scale,
                );
            }
            ui_registry.register_text_input(
                crate::ui_system::UiId::FileTreeRenameInput,
                input_x,
                input_y,
                input_w,
                input_h,
                mx,
                my,
            );
            self.draw_file_tree_dialog_input(
                &dialog.editor,
                input_x,
                input_y,
                input_w,
                input_h,
                dialog.input_scroll_x.current,
                blink_alpha,
            );
            if let Some(error) = &dialog.error {
                self.draw_string_scaled(
                    error,
                    x + side_pad,
                    input_y + input_h + 20.0 * s,
                    self.theme.diag_error,
                    0.8,
                );
            }

            let btn_w = 130.0 * s;
            let btn_h = 32.0 * s;
            let (ok_x, cancel_x) = centered_dialog_button_positions(x, w, btn_w, 10.0 * s);
            let btn_y = y + h - 64.0 * s;
            wants_pointer |= self.draw_file_tree_dialog_buttons(
                ui_registry,
                [
                (
                    crate::ui_system::UiId::FileTreeRenameConfirm,
                    "Переименовать",
                    ok_x,
                ),
                (
                    crate::ui_system::UiId::FileTreeRenameCancel,
                    "Отмена",
                    cancel_x,
                ),
                ],
                btn_y,
                btn_w,
                btn_h,
                s,
                mx,
                my,
            );
        }

        if let Some(dialog) = &ide_panel.file_tree_move_dialog {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.42]);
            let w =
                ((crate::app::file_tree::FILE_TREE_DIALOG_W + 20.0) * s).min(self.width - 32.0 * s);
            let h = 154.0 * s;
            let x = ((self.width - w) / 2.0).round();
            let y = ((self.height - h) / 2.0).round();
            let side_pad = crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * s;
            self.draw_file_tree_dialog_shell(x, y, w, h, s);
            self.draw_string_scaled(
                "Подтвердить перемещение",
                x + side_pad,
                y + 38.0 * s,
                self.theme.fg,
                1.0,
            );
            let message = crate::app::file_tree::file_tree_move_dialog_message(
                &dialog.sources,
                &dialog.target_dir,
            );
            self.draw_string_scaled(
                &message,
                x + side_pad,
                y + 74.0 * s,
                [0.75, 0.76, 0.82, 1.0],
                0.88,
            );
            if let Some(error) = &dialog.error {
                self.draw_string_scaled(
                    error,
                    x + side_pad,
                    y + 100.0 * s,
                    self.theme.diag_error,
                    0.8,
                );
            }

            let btn_w = 122.0 * s;
            let btn_h = 32.0 * s;
            let (ok_x, cancel_x) = centered_dialog_button_positions(x, w, btn_w, 10.0 * s);
            let btn_y = y + h - 64.0 * s;
            wants_pointer |= self.draw_file_tree_dialog_buttons(
                ui_registry,
                [
                (
                    crate::ui_system::UiId::FileTreeMoveConfirm,
                    "Переместить",
                    ok_x,
                ),
                (
                    crate::ui_system::UiId::FileTreeMoveCancel,
                    "Отмена",
                    cancel_x,
                ),
                ],
                btn_y,
                btn_w,
                btn_h,
                s,
                mx,
                my,
            );
        }

        if let Some(dialog) = &ide_panel.file_tree_delete_dialog {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.42]);
            let w =
                ((crate::app::file_tree::FILE_TREE_DIALOG_W + 20.0) * s).min(self.width - 32.0 * s);
            let h = 154.0 * s;
            let x = ((self.width - w) / 2.0).round();
            let y = ((self.height - h) / 2.0).round();
            let side_pad = crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * s;
            self.draw_file_tree_dialog_shell(x, y, w, h, s);
            self.draw_string_scaled(
                "Удалить в корзину",
                x + side_pad,
                y + 38.0 * s,
                self.theme.fg,
                1.0,
            );
            let message = crate::app::file_tree::file_tree_delete_dialog_message(&dialog.paths);
            self.draw_string_scaled(
                &message,
                x + side_pad,
                y + 74.0 * s,
                [0.75, 0.76, 0.82, 1.0],
                0.88,
            );
            if let Some(error) = &dialog.error {
                self.draw_string_scaled(
                    error,
                    x + side_pad,
                    y + 100.0 * s,
                    self.theme.diag_error,
                    0.8,
                );
            }

            let btn_w = 122.0 * s;
            let btn_h = 32.0 * s;
            let (ok_x, cancel_x) = centered_dialog_button_positions(x, w, btn_w, 10.0 * s);
            let btn_y = y + h - 64.0 * s;
            wants_pointer |= self.draw_file_tree_dialog_buttons(
                ui_registry,
                [
                (
                    crate::ui_system::UiId::FileTreeDeleteConfirm,
                    "В корзину",
                    ok_x,
                ),
                (
                    crate::ui_system::UiId::FileTreeDeleteCancel,
                    "Отмена",
                    cancel_x,
                ),
                ],
                btn_y,
                btn_w,
                btn_h,
                s,
                mx,
                my,
            );
        }

        if let Some(dialog) = &ide_panel.api.spec_remove_dialog {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.42]);
            let w =
                ((crate::app::file_tree::FILE_TREE_DIALOG_W + 20.0) * s).min(self.width - 32.0 * s);
            let h = 204.0 * s;
            let x = ((self.width - w) / 2.0).round();
            let y = ((self.height - h) / 2.0).round();
            let side_pad = crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * s;
            self.draw_file_tree_dialog_shell(x, y, w, h, s);
            self.draw_string_scaled(
                "Удалить OpenAPI",
                x + side_pad,
                y + 38.0 * s,
                self.theme.fg,
                1.0,
            );
            self.draw_string_scaled(
                "Удалить импортированную спецификацию?",
                x + side_pad,
                y + 70.0 * s,
                [0.75, 0.76, 0.82, 1.0],
                0.86,
            );
            let label = if dialog.title.is_empty() {
                dialog.source.as_str()
            } else {
                dialog.title.as_str()
            };
            self.draw_tree_label_clipped(
                label,
                x + side_pad,
                y + 94.0 * s,
                w - side_pad * 2.0,
                [0.72, 0.76, 0.88, 1.0],
                0.82,
                &mut label_scratch,
            );
            if !dialog.source.is_empty() && dialog.source != label {
                self.draw_tree_label_clipped(
                    dialog.source.as_str(),
                    x + side_pad,
                    y + 120.0 * s,
                    w - side_pad * 2.0,
                    [0.58, 0.61, 0.70, 1.0],
                    0.74,
                    &mut label_scratch,
                );
            }

            let btn_w = 122.0 * s;
            let btn_h = 32.0 * s;
            let (ok_x, cancel_x) = centered_dialog_button_positions(x, w, btn_w, 10.0 * s);
            let btn_y = y + h - 58.0 * s;
            wants_pointer |= self.draw_file_tree_dialog_buttons(
                ui_registry,
                [
                (
                    crate::ui_system::UiId::ApiSpecRemoveConfirm,
                    "Удалить",
                    ok_x,
                ),
                (
                    crate::ui_system::UiId::ApiSpecRemoveCancel,
                    "Отмена",
                    cancel_x,
                ),
                ],
                btn_y,
                btn_w,
                btn_h,
                s,
                mx,
                my,
            );
        }

        if let Some(dialog) = &ide_panel.api.mock_contract_field_delete_dialog {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.42]);
            let w =
                ((crate::app::file_tree::FILE_TREE_DIALOG_W + 20.0) * s).min(self.width - 32.0 * s);
            let h = 158.0 * s;
            let x = ((self.width - w) / 2.0).round();
            let y = ((self.height - h) / 2.0).round();
            let side_pad = crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * s;
            self.draw_file_tree_dialog_shell(x, y, w, h, s);
            self.draw_string_scaled(
                "Удалить переменную",
                x + side_pad,
                y + 38.0 * s,
                self.theme.fg,
                1.0,
            );
            self.draw_string_scaled(
                "Удалить переменную из контракта мока?",
                x + side_pad,
                y + 70.0 * s,
                [0.75, 0.76, 0.82, 1.0],
                0.86,
            );
            self.draw_tree_label_clipped(
                dialog.field_label.as_str(),
                x + side_pad,
                y + 94.0 * s,
                w - side_pad * 2.0,
                [0.72, 0.76, 0.88, 1.0],
                0.82,
                &mut label_scratch,
            );

            let btn_w = 122.0 * s;
            let btn_h = 32.0 * s;
            let (ok_x, cancel_x) = centered_dialog_button_positions(x, w, btn_w, 10.0 * s);
            let btn_y = y + h - 64.0 * s;
            wants_pointer |= self.draw_file_tree_dialog_buttons(
                ui_registry,
                [
                (
                    crate::ui_system::UiId::ApiMockContractFieldRemoveConfirm,
                    "Удалить",
                    ok_x,
                ),
                (
                    crate::ui_system::UiId::ApiMockContractFieldRemoveCancel,
                    "Отмена",
                    cancel_x,
                ),
                ],
                btn_y,
                btn_w,
                btn_h,
                s,
                mx,
                my,
            );
        }

        if let Some(dialog) = &ide_panel.api.mock_route_reset_dialog {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.42]);
            let w =
                ((crate::app::file_tree::FILE_TREE_DIALOG_W + 20.0) * s).min(self.width - 32.0 * s);
            let h = 166.0 * s;
            let x = ((self.width - w) / 2.0).round();
            let y = ((self.height - h) / 2.0).round();
            let side_pad = crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * s;
            self.draw_file_tree_dialog_shell(x, y, w, h, s);
            self.draw_string_scaled("Сбросить мок", x + side_pad, y + 38.0 * s, self.theme.fg, 1.0);
            self.draw_string_scaled(
                "Удалить все настройки мока для route?",
                x + side_pad,
                y + 70.0 * s,
                [0.75, 0.76, 0.82, 1.0],
                0.86,
            );
            self.draw_tree_label_clipped(
                dialog.route_label.as_str(),
                x + side_pad,
                y + 94.0 * s,
                w - side_pad * 2.0,
                [0.72, 0.76, 0.88, 1.0],
                0.82,
                &mut label_scratch,
            );

            let btn_w = 122.0 * s;
            let btn_h = 32.0 * s;
            let (ok_x, cancel_x) = centered_dialog_button_positions(x, w, btn_w, 10.0 * s);
            let btn_y = y + h - 64.0 * s;
            wants_pointer |= self.draw_file_tree_dialog_buttons(
                ui_registry,
                [
                (
                    crate::ui_system::UiId::ApiMockRouteResetConfirm,
                    "Сбросить",
                    ok_x,
                ),
                (
                    crate::ui_system::UiId::ApiMockRouteResetCancel,
                    "Отмена",
                    cancel_x,
                ),
                ],
                btn_y,
                btn_w,
                btn_h,
                s,
                mx,
                my,
            );
        }

        if let Some(dialog) = &ide_panel.git.confirm_dialog {
            self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.42]);
            let w =
                ((crate::app::file_tree::FILE_TREE_DIALOG_W + 40.0) * s).min(self.width - 32.0 * s);
            let visible_files = dialog.files.len().min(7);
            let h = (172.0 * s + visible_files as f32 * 20.0 * s).min(self.height - 32.0 * s);
            let x = ((self.width - w) / 2.0).round();
            let y = ((self.height - h) / 2.0).round();
            let side_pad = crate::app::file_tree::FILE_TREE_DIALOG_SIDE_PAD * s;
            self.draw_file_tree_dialog_shell(x, y, w, h, s);

            let (title, message, confirm_label) = match dialog.action {
                crate::app::git_panel::GitConfirmAction::RollbackStaged => (
                    "Откатить staged файлы",
                    "Отменить staged изменения в выбранных файлах?",
                    "Откатить",
                ),
            };
            self.draw_string_scaled(title, x + side_pad, y + 38.0 * s, self.theme.fg, 1.0);
            self.draw_string_scaled(
                message,
                x + side_pad,
                y + 70.0 * s,
                [0.75, 0.76, 0.82, 1.0],
                0.86,
            );

            let list_x = x + side_pad;
            let list_y = y + 92.0 * s;
            let list_w = w - side_pad * 2.0;
            for (idx, file) in dialog.files.iter().take(visible_files).enumerate() {
                self.draw_tree_label_clipped(
                    file.display_path.as_str(),
                    list_x,
                    list_y + idx as f32 * 20.0 * s,
                    list_w,
                    [0.72, 0.76, 0.88, 1.0],
                    0.82,
                    &mut label_scratch,
                );
            }
            if dialog.files.len() > visible_files {
                let more = format!("+{} more", dialog.files.len() - visible_files);
                self.draw_string_scaled(
                    &more,
                    list_x,
                    list_y + visible_files as f32 * 20.0 * s,
                    [0.55, 0.57, 0.64, 1.0],
                    0.8,
                );
            }

            let btn_w = 122.0 * s;
            let btn_h = 32.0 * s;
            let (ok_x, cancel_x) = centered_dialog_button_positions(x, w, btn_w, 10.0 * s);
            let btn_y = y + h - 64.0 * s;
            wants_pointer |= self.draw_file_tree_dialog_buttons(
                ui_registry,
                [
                (
                    crate::ui_system::UiId::GitConfirmAction,
                    confirm_label,
                    ok_x,
                ),
                (crate::ui_system::UiId::GitConfirmCancel, "Отмена", cancel_x),
                ],
                btn_y,
                btn_w,
                btn_h,
                s,
                mx,
                my,
            );
        }

        wants_pointer
    }
}
