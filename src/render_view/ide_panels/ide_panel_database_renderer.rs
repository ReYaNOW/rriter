#[derive(Clone, Copy, Debug, PartialEq)]
struct DatabaseModalGeometry {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    scale: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DatabasePanelContentLayout {
    error_y: f32,
    error_h: f32,
    content_y: f32,
    content_h: f32,
}

fn database_panel_content_layout(
    panel_y: f32,
    panel_h: f32,
    toolbar_h: f32,
    scale: f32,
    has_global_error: bool,
) -> DatabasePanelContentLayout {
    let remaining_h = (panel_h - toolbar_h).max(0.0);
    let error_h = if has_global_error {
        (44.0 * scale).min(remaining_h)
    } else {
        0.0
    };
    let error_y = panel_y + toolbar_h;
    DatabasePanelContentLayout {
        error_y,
        error_h,
        content_y: error_y + error_h,
        content_h: (remaining_h - error_h).max(0.0),
    }
}

#[allow(clippy::too_many_arguments)]
fn database_modal_geometry(
    viewport_w: f32,
    viewport_h: f32,
    base_scale: f32,
    desired_w: f32,
    desired_h: f32,
    minimum_w: f32,
    minimum_h: f32,
) -> DatabaseModalGeometry {
    let base_scale = base_scale.max(0.01);
    let available_w = (viewport_w - 32.0 * base_scale).max(1.0);
    let available_h = (viewport_h - 32.0 * base_scale).max(1.0);
    let fit = (available_w / (minimum_w * base_scale).max(1.0))
        .min(available_h / (minimum_h * base_scale).max(1.0))
        .min(1.0)
        .max(0.1);
    let scale = base_scale * fit;
    let w = (desired_w * scale).min(available_w).max(1.0).round();
    let h = (desired_h * scale).min(available_h).max(1.0).round();
    DatabaseModalGeometry {
        x: ((viewport_w - w) * 0.5).max(0.0).round(),
        y: ((viewport_h - h) * 0.5).max(0.0).round(),
        w,
        h,
        scale,
    }
}

fn database_host_key_buttons_horizontal(inner_w: f32, scale: f32) -> bool {
    let total_w = (150.0 + 180.0 + 90.0 + 20.0) * scale;
    total_w <= inner_w
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Renderer {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_database_panel(
        &mut self,
        panel_x: f32,
        panel_y: f32,
        panel_w: f32,
        panel_h: f32,
        s: f32,
        ide_panel: &crate::app::IdePanelState,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
    ) {
        use crate::app::database::{
            DatabaseConnectionChildrenState, DatabaseConnectionStatus,
        };
        use crate::ui_system::UiId;
        use crate::widgets::{IconButton, IconType};

        let database = &ide_panel.database;
        let toolbar_h = 34.0 * s;
        self.push_rect(panel_x, panel_y, panel_w, panel_h, [0.129, 0.133, 0.173, 1.0]);
        ui_registry.register_blocker(
            UiId::DatabasePanelBody,
            panel_x,
            panel_y,
            panel_w,
            panel_h,
            mx,
            my,
        );
        self.push_rect(
            panel_x,
            panel_y,
            panel_w,
            toolbar_h,
            [0.15, 0.155, 0.20, 1.0],
        );

        let button_size = 24.0 * s;
        let mut button_x = panel_x + 7.0 * s;
        let button_y = panel_y + (toolbar_h - button_size) * 0.5;
        for (id, icon, enabled) in [
            (UiId::DatabaseAdd, IconType::Plus, true),
            (
                UiId::DatabaseDelete,
                IconType::GitMinus,
                database.selected_connection_delete_enabled(),
            ),
            (
                UiId::DatabaseRefresh,
                IconType::Reload,
                database.selected_connection_refresh_enabled(),
            ),
        ] {
            let button = IconButton {
                x: button_x,
                y: button_y,
                size: button_size,
                icon: Some(icon),
                is_active: false,
                icon_size: Some(17.0 * s),
                active_square_width: None,
                custom_color: (!enabled).then_some([0.4, 0.4, 0.45, 0.55]),
            };
            if enabled {
                ui_registry.register_icon_button(id, &button, self, mx, my, s, false);
            } else {
                button.render(self, -1.0, -1.0, s, false);
            }
            button_x += 28.0 * s;
        }

        if let Some(pending) = database.pending_job.as_ref() {
            let text = format!("Запрос #{}…", pending.id.0);
            self.draw_string_scaled_pixel_snapped(
                &text,
                button_x + 4.0 * s,
                panel_y + 22.0 * s,
                [0.63, 0.70, 0.92, 1.0],
                0.78,
            );
        }

        let panel_layout = database_panel_content_layout(
            panel_y,
            panel_h,
            toolbar_h,
            s,
            database.global_error.is_some(),
        );
        if let Some(error) = database.global_error.as_deref()
            && panel_layout.error_h > 0.0
        {
            self.push_rect(
                panel_x,
                panel_layout.error_y,
                panel_w,
                panel_layout.error_h,
                [0.34, 0.12, 0.15, 1.0],
            );
            let copy_size = (17.0 * s).min((panel_layout.error_h - 8.0 * s).max(0.0));
            let copy_slot_w = if copy_size > 0.0 { 30.0 * s } else { 0.0 };
            let mut scratch = String::new();
            self.draw_tree_label_clipped(
                error,
                panel_x + 8.0 * s,
                Self::tree_row_text_y(
                    panel_layout.error_y,
                    panel_layout.error_h,
                    s,
                ),
                (panel_w - 16.0 * s - copy_slot_w).max(4.0),
                [1.0, 0.74, 0.76, 1.0],
                0.78,
                &mut scratch,
            );
            if copy_size > 0.0 && panel_w >= 40.0 * s {
                let copy_x = panel_x + panel_w - 8.0 * s - copy_size;
                let copy_y = panel_layout.error_y
                    + (panel_layout.error_h - copy_size) * 0.5;
                let hovered = ui_registry.register_rect(
                    UiId::DatabaseGlobalErrorCopy,
                    copy_x - 4.0 * s,
                    copy_y - 4.0 * s,
                    copy_size + 8.0 * s,
                    copy_size + 8.0 * s,
                    mx,
                    my,
                );
                self.draw_atlas_icon(
                    IconType::Copy,
                    copy_x,
                    copy_y,
                    copy_size,
                    if hovered {
                        [1.0, 1.0, 1.0, 1.0]
                    } else {
                        [0.88, 0.70, 0.72, 1.0]
                    },
                );
            }
        }

        let content_y = panel_layout.content_y;
        let content_h = panel_layout.content_h;
        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            self.gl.scissor(
                panel_x.max(0.0) as i32,
                (self.height - content_y - content_h).max(0.0) as i32,
                panel_w.max(0.0) as i32,
                content_h.max(0.0) as i32,
            );
        }

        let row_h = crate::render_view::tree_ui::TREE_ROW_H * s;
        let scroll = database.scroll.current.round();
        let hover_settled = database.scroll.is_settled();
        let content_clip = crate::ui_system::UiClipRect::new(
            panel_x,
            content_y,
            panel_w,
            content_h,
        );
        let mut logical_row = 0usize;
        let mut label_scratch = String::new();
        for (connection_idx, connection) in database.connections.iter().enumerate() {
            let row_y = (content_y + logical_row as f32 * row_h - scroll).round();
            if row_y + row_h >= content_y && row_y <= content_y + content_h {
                let selected = database.selected_connection == Some(connection.config.id);
                let hovered = hover_settled
                    && ui_registry.register_rect_clipped(
                        UiId::DatabaseConnectionRow(connection_idx),
                        panel_x,
                        row_y,
                        panel_w,
                        row_h,
                        content_clip,
                        mx,
                        my,
                    );
                if selected {
                    self.push_rect(panel_x, row_y, panel_w, row_h, [0.60, 0.35, 0.85, 0.24]);
                } else if hovered {
                    self.push_rect(panel_x, row_y, panel_w, row_h, [1.0, 1.0, 1.0, 0.06]);
                }
                let arrow_x = panel_x + 6.0 * s;
                if hover_settled {
                    ui_registry.register_rect_clipped(
                        UiId::DatabaseConnectionArrow(connection_idx),
                        arrow_x - 4.0 * s,
                        row_y,
                        18.0 * s,
                        row_h,
                        content_clip,
                        mx,
                        my,
                    );
                }
                self.draw_tree_disclosure_icon(
                    connection.expanded,
                    arrow_x,
                    row_y,
                    row_h,
                    [0.72, 0.75, 0.82, 1.0],
                );
                let color = database_connection_color(connection.config.color);
                self.push_rounded_rect(
                    panel_x + 25.0 * s,
                    row_y + 8.0 * s,
                    8.0 * s,
                    8.0 * s,
                    4.0 * s,
                    color,
                );
                self.draw_atlas_icon(
                    IconType::Database,
                    panel_x + 37.0 * s,
                    database_tree_icon_y(row_y, row_h, 17.0 * s),
                    17.0 * s,
                    [1.0, 0.67, 0.16, 1.0],
                );
                let max_w = (panel_w - 82.0 * s).max(10.0);
                self.draw_tree_label_clipped(
                    &connection.config.display_name,
                    panel_x + 58.0 * s,
                    Self::tree_row_text_y(row_y, row_h, s),
                    max_w,
                    self.theme.fg,
                    0.86,
                    &mut label_scratch,
                );
                let status_color = match connection.status {
                    DatabaseConnectionStatus::Ready => [0.35, 0.85, 0.48, 1.0],
                    DatabaseConnectionStatus::BuiltinSsh => [0.95, 0.72, 0.25, 1.0],
                    DatabaseConnectionStatus::Connecting => [0.40, 0.67, 0.95, 1.0],
                    DatabaseConnectionStatus::Error => [0.95, 0.35, 0.38, 1.0],
                    DatabaseConnectionStatus::Disconnected => [0.45, 0.47, 0.53, 1.0],
                };
                self.push_rounded_rect(
                    panel_x + panel_w - 16.0 * s,
                    row_y + 9.0 * s,
                    6.0 * s,
                    6.0 * s,
                    3.0 * s,
                    status_color,
                );
            }
            logical_row += 1;

            if connection.expanded {
                let children_state = connection.children_state();
                let connection_hint = if connection.databases.is_empty() {
                    match children_state {
                        DatabaseConnectionChildrenState::ExpandedUnloaded => {
                            Some("Ожидание загрузки…")
                        }
                        DatabaseConnectionChildrenState::ExpandedLoading => {
                            Some("Подключение…")
                        }
                        DatabaseConnectionChildrenState::ExpandedEmpty => {
                            Some("Нет доступных баз")
                        }
                        DatabaseConnectionChildrenState::ExpandedError => Some(
                            if connection.status == DatabaseConnectionStatus::Disconnected {
                                "Соединение закрыто · нажмите Обновить"
                            } else {
                                "Ошибка загрузки · нажмите Обновить"
                            },
                        ),
                        DatabaseConnectionChildrenState::Collapsed
                        | DatabaseConnectionChildrenState::ExpandedLoaded => None,
                    }
                } else {
                    None
                };
                if let Some(hint) = connection_hint {
                    draw_database_hint(
                        self,
                        hint,
                        panel_x + 34.0 * s,
                        content_y + logical_row as f32 * row_h - scroll,
                        s,
                    );
                    logical_row += 1;
                }
                for (database_idx, database_node) in connection.databases.iter().enumerate() {
                    let row_y = (content_y + logical_row as f32 * row_h - scroll).round();
                    if row_y + row_h >= content_y && row_y <= content_y + content_h {
                        let selected = database
                            .selected_database
                            .as_ref()
                            .is_some_and(|(id, name)| *id == connection.config.id && name == &database_node.name);
                        let hovered = hover_settled
                            && ui_registry.register_rect_clipped(
                                UiId::DatabaseRow(connection_idx, database_idx),
                                panel_x,
                                row_y,
                                panel_w,
                                row_h,
                                content_clip,
                                mx,
                                my,
                            );
                        if selected {
                            self.push_rect(panel_x, row_y, panel_w, row_h, [0.35, 0.48, 0.72, 0.20]);
                        } else if hovered {
                            self.push_rect(panel_x, row_y, panel_w, row_h, [1.0, 1.0, 1.0, 0.05]);
                        }
                        let arrow_x = panel_x + 26.0 * s;
                        if hover_settled {
                            ui_registry.register_rect_clipped(
                                UiId::DatabaseArrow(connection_idx, database_idx),
                                arrow_x - 4.0 * s,
                                row_y,
                                18.0 * s,
                                row_h,
                                content_clip,
                                mx,
                                my,
                            );
                        }
                        self.draw_tree_disclosure_icon(
                            database_node.expanded,
                            arrow_x,
                            row_y,
                            row_h,
                            [0.68, 0.71, 0.79, 1.0],
                        );
                        self.draw_atlas_icon(
                            IconType::Database,
                            panel_x + 46.0 * s,
                            database_tree_icon_y(row_y, row_h, 15.0 * s),
                            15.0 * s,
                            [1.0, 0.67, 0.16, 1.0],
                        );
                        self.draw_tree_label_clipped(
                            &database_node.name,
                            panel_x + 65.0 * s,
                            Self::tree_row_text_y(row_y, row_h, s),
                            (panel_w - 73.0 * s).max(10.0),
                            self.theme.fg,
                            0.84,
                            &mut label_scratch,
                        );
                    }
                    logical_row += 1;
                    if database_node.expanded {
                        if database_node.loading && database_node.tables.is_empty() {
                            draw_database_hint(
                                self,
                                "Загрузка таблиц…",
                                panel_x + 58.0 * s,
                                content_y + logical_row as f32 * row_h - scroll,
                                s,
                            );
                            logical_row += 1;
                        }
                        for (table_idx, table) in database_node.tables.iter().enumerate() {
                            let row_y = (content_y + logical_row as f32 * row_h - scroll).round();
                            if row_y + row_h >= content_y && row_y <= content_y + content_h {
                                let hovered = hover_settled
                                    && ui_registry.register_rect_clipped(
                                        UiId::DatabaseTableRow(connection_idx, database_idx, table_idx),
                                        panel_x,
                                        row_y,
                                        panel_w,
                                        row_h,
                                        content_clip,
                                        mx,
                                        my,
                                    );
                                let selected = database.selected_table.as_ref().is_some_and(
                                    |(id, database_name, table_name)| {
                                        *id == connection.config.id
                                            && database_name == &database_node.name
                                            && table_name == &table.name
                                    },
                                );
                                if selected {
                                    self.push_rect(
                                        panel_x,
                                        row_y,
                                        panel_w,
                                        row_h,
                                        [0.35, 0.48, 0.72, 0.20],
                                    );
                                } else if hovered {
                                    self.push_rect(
                                        panel_x,
                                        row_y,
                                        panel_w,
                                        row_h,
                                        [1.0, 1.0, 1.0, 0.05],
                                    );
                                }
                                self.draw_atlas_icon(
                                    IconType::DatabaseTable,
                                    panel_x + 67.0 * s,
                                    database_tree_icon_y(row_y, row_h, 15.0 * s),
                                    15.0 * s,
                                    [0.20, 0.80, 0.75, 1.0],
                                );
                                self.draw_tree_label_clipped(
                                    &table.name,
                                    panel_x + 86.0 * s,
                                    Self::tree_row_text_y(row_y, row_h, s),
                                    (panel_w - 94.0 * s).max(10.0),
                                    self.theme.fg,
                                    0.82,
                                    &mut label_scratch,
                                );
                            }
                            logical_row += 1;
                        }
                    }
                }
            }
        }

        if database.connections.is_empty() {
            draw_database_hint(
                self,
                "Добавьте PostgreSQL подключение",
                panel_x + 14.0 * s,
                content_y + 30.0 * s,
                s,
            );
        }

        self.flush();
        unsafe { self.gl.disable(glow::SCISSOR_TEST) };

    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_database_overlays(
        &mut self,
        s: f32,
        ide_panel: &crate::app::IdePanelState,
        editor: &crate::editor::Editor,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        blink_alpha: f32,
    ) -> bool {
        let database = &ide_panel.database;
        if database.dialog.is_none() {
            self.reset_delayed_tooltip_anchor_namespace(DATABASE_DIALOG_TOOLTIP_NAMESPACE);
        }
        let mut drew = false;
        if let Some(menu) = database.context_menu.as_ref() {
            ui_registry.mark_overlay_start();
            self.draw_animated_context_menu(
                menu.x,
                menu.y,
                menu.opened_at,
                menu.entries.len(),
                |idx| database_context_action_label(menu.entries[idx]),
                crate::ui_system::UiId::DatabaseContextItem,
                |_| false,
                ui_registry,
                mx,
                my,
            );
            drew = true;
        }
        if let Some(modal) = database.table_modal.as_ref() {
            self.draw_database_table_modal(s, modal, ui_registry, mx, my, blink_alpha);
            drew = true;
        } else if let Some(dialog) = database.dialog.as_ref() {
            self.draw_database_connection_dialog(s, dialog, ui_registry, mx, my, blink_alpha);
            drew = true;
        } else if let Some(prompt) = database.delete_prompt.as_ref() {
            draw_database_confirmation(
                self,
                s,
                ui_registry,
                mx,
                my,
                "Удалить подключение?",
                if prompt.blocked_open_tabs > 0 {
                    "Подключение нельзя удалить, пока открыты связанные вкладки."
                } else {
                    "Сохранённые системные секреты подключения также будут удалены."
                },
                crate::ui_system::UiId::DatabaseDeleteConfirm,
                crate::ui_system::UiId::DatabaseDeleteCancel,
                prompt.blocked_open_tabs == 0,
            );
            drew = true;
        } else if let Some(prompt) = database.host_key_prompt.as_ref() {
            let detail = format!(
                "{}:{}\n{}\n{}",
                prompt.host, prompt.port, prompt.algorithm, prompt.fingerprint
            );
            draw_database_host_key_confirmation(
                self,
                s,
                ui_registry,
                mx,
                my,
                &detail,
            );
            drew = true;
        }

        if !database.modal_open()
            && let Ok(mut ddl) = database.ddl_hover.try_borrow_mut()
            && let Some(state) = ddl.as_mut()
        {
            state.popup.anim_progress = (state.popup.anim_progress + 0.12).min(1.0);
            let selection = match (state.selection_anchor, state.selection_cursor) {
                (Some(a), Some(b)) => Some((a.min(b), a.max(b))),
                _ => None,
            };
            let mut wants_pointer = false;
            let opacity = state.popup.anim_progress;
            let (x, y, w, h, max_scroll) = self.draw_hover_popup(
                &mut state.popup,
                None,
                selection,
                editor,
                ui_registry,
                mx,
                my,
                0.0,
                &mut wants_pointer,
                opacity,
                Some(((self.width - 80.0 * s).min(900.0 * s), (self.height - 100.0 * s).min(650.0 * s))),
                None,
            );
            state.rect = Some((x, y, w, h));
            state.max_scroll = max_scroll;
            drew = true;
        }
        drew
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_database_connection_dialog(
        &mut self,
        s: f32,
        dialog: &crate::app::database::DatabaseConnectionDialog,
        ui_registry: &mut crate::ui_system::UiRegistry,
        mx: f32,
        my: f32,
        blink_alpha: f32,
    ) {
        use crate::ui_system::UiId;
        ui_registry.mark_overlay_start();
        self.push_rect(0.0, 0.0, self.width, self.height, [0.0, 0.0, 0.0, 0.62]);
        ui_registry.register_blocker(
            UiId::DatabaseDialogBackdrop,
            0.0,
            0.0,
            self.width,
            self.height,
            mx,
            my,
        );
        let layout = database_connection_dialog_layout(
            self.width,
            self.height,
            s,
            dialog.visible_fields().count(),
        );
        let geometry = layout.modal;
        let s = geometry.scale;
        let width = geometry.w;
        let height = geometry.h;
        let x = geometry.x;
        let y = geometry.y;
        self.push_rounded_rect(x, y, width, height, 8.0 * s, [0.12, 0.125, 0.16, 1.0]);
        self.push_rounded_rect_border(
            x,
            y,
            width,
            height,
            8.0 * s,
            1.0,
            [1.0, 1.0, 1.0, 0.17],
            [0.12, 0.125, 0.16, 1.0],
        );
        ui_registry.register_blocker(UiId::DatabaseDialogBody, x, y, width, height, mx, my);
        self.draw_string_scaled_pixel_snapped(
            if dialog.editing_connection_id.is_some() {
                "Изменить PostgreSQL подключение"
            } else {
                "Добавить PostgreSQL подключение"
            },
            x + 20.0 * s,
            y + 30.0 * s,
            self.theme.fg,
            1.05,
        );

        let form_clip = layout.form_clip;
        let scroll_y = dialog.scroll.current.clamp(0.0, layout.max_scroll);
        let mut hovered_tooltip = None;
        self.flush();
        unsafe {
            self.gl.enable(glow::SCISSOR_TEST);
            self.gl.scissor(
                form_clip.x.max(0.0) as i32,
                (self.height - form_clip.y - form_clip.h).max(0.0) as i32,
                form_clip.w.max(0.0) as i32,
                form_clip.h.max(0.0) as i32,
            );
        }
        for (row, field) in dialog.visible_fields().enumerate() {
            let remember = database_remember_control(field, dialog);
            let field_layout = database_dialog_field_layout(
                &layout,
                row,
                scroll_y,
                remember.is_some(),
                field.is_secret(),
            );
            if !field_layout.row_visible {
                continue;
            }

            let label = database_field_label(field);
            let mut label_scratch = String::new();
            self.draw_tree_label_clipped(
                label,
                field_layout.label.x,
                Self::tree_row_text_y(field_layout.input.y, field_layout.input.h, s),
                field_layout.label.w,
                [0.72, 0.75, 0.82, 1.0],
                DATABASE_DIALOG_FIELD_TEXT_SCALE,
                &mut label_scratch,
            );
            let label_hover_w = self
                .measure_ui_width(label, DATABASE_DIALOG_FIELD_TEXT_SCALE)
                .min(field_layout.label.w)
                .max(1.0);
            if ui_registry.register_rect_clipped(
                UiId::DatabaseDialogField(field),
                field_layout.label.x,
                field_layout.input.y,
                label_hover_w,
                field_layout.input.h,
                form_clip,
                mx,
                my,
            ) {
                hovered_tooltip = Some((
                    DatabaseDialogTooltipTarget::Field(field),
                    field_layout.label.x + label_hover_w,
                    field_layout.input.y + field_layout.input.h,
                ));
            }

            let focused = dialog.focused == Some(field);
            ui_registry.register_text_input_clipped(
                UiId::DatabaseDialogField(field),
                field_layout.input.x,
                field_layout.input.y,
                field_layout.input.w,
                field_layout.input.h,
                form_clip,
                mx,
                my,
            );
            if let (Some((remember_id, enabled)), Some(remember_rect)) = (remember, field_layout.remember) {
                let hovered = ui_registry.register_rect_clipped(
                    remember_id,
                    remember_rect.x,
                    remember_rect.y,
                    remember_rect.w,
                    remember_rect.h,
                    form_clip,
                    mx,
                    my,
                );
                crate::widgets::CheckboxView {
                    x: remember_rect.x,
                    y: remember_rect.y,
                    w: remember_rect.w,
                    h: remember_rect.h,
                    label: "Запомнить",
                    checked: enabled,
                    enabled: true,
                }
                .render(self, hovered, s);
            }

            let input = dialog.input(field);
            let secret_masked = field.is_secret() && !dialog.secret_is_revealed(field);
            let eye_slot_w = field_layout.eye_hit.map_or(0.0, |rect| rect.w);
            let visible_width = (field_layout.input.w - 16.0 * s - eye_slot_w).max(1.0);
            let scroll_x = crate::app::file_tree::file_tree_name_input_scroll_x(
                input.text(),
                input.cursor,
                visible_width,
                |ch| {
                    let rendered = if secret_masked { '•' } else { ch };
                    self.get_ui_glyph(rendered)
                        .map(|glyph| {
                            Self::snapped_text_advance(
                                glyph.advance,
                                DATABASE_DIALOG_FIELD_TEXT_SCALE,
                            )
                        })
                        .unwrap_or(8.0)
                },
            );
            self.draw_one_line_dialog_input(
                input.text(),
                input.cursor,
                input.selection_anchor,
                secret_masked,
                focused,
                field_layout.input.x,
                field_layout.input.y,
                field_layout.input.w,
                field_layout.input.h,
                scroll_x,
                if focused { blink_alpha } else { 0.0 },
                DATABASE_DIALOG_FIELD_TEXT_SCALE,
                eye_slot_w,
            );
            if let (Some(hit), Some(visual)) = (field_layout.eye_hit, field_layout.eye_visual) {
                let hovered = ui_registry.register_rect_clipped(
                    UiId::DatabaseDialogSecretEye(field),
                    hit.x,
                    hit.y,
                    hit.w,
                    hit.h,
                    form_clip,
                    mx,
                    my,
                );
                let eye = crate::widgets::IconButton {
                    x: visual.x,
                    y: visual.y,
                    size: visual.w,
                    icon: Some(crate::widgets::IconType::Eye),
                    is_active: dialog.secret_is_revealed(field),
                    icon_size: Some((16.0 * s).round().min(visual.w)),
                    active_square_width: None,
                    custom_color: None,
                };
                let (render_mx, render_my) = if hovered {
                    (visual.x + visual.w * 0.5, visual.y + visual.h * 0.5)
                } else {
                    (f32::NEG_INFINITY, f32::NEG_INFINITY)
                };
                eye.render(self, render_mx, render_my, s, false);
            }
        }

        if let Some(track) = layout.scrollbar_track
            && let Some(thumb) = database_connection_dialog_scrollbar_thumb(&layout, scroll_y)
        {
            self.push_rounded_rect(
                track.x,
                track.y,
                track.w,
                track.h,
                track.w * 0.5,
                [0.20, 0.21, 0.25, 0.55],
            );
            self.push_rounded_rect(
                track.x,
                thumb.start,
                track.w,
                thumb.len,
                track.w * 0.5,
                [0.48, 0.50, 0.58, 0.90],
            );
        }
        self.flush();
        unsafe { self.gl.disable(glow::SCISSOR_TEST) };

        let footer = layout.footer;
        let toggle_label = format!(
            "TLS: {:?}   Цвет: {:?}   SSH: {}   Бастион: {}",
            dialog.tls_mode,
            dialog.color,
            if dialog.ssh_enabled { "да" } else { "нет" },
            if dialog.jump_enabled { "да" } else { "нет" },
        );
        let mut toggle_scratch = String::new();
        self.draw_tree_label_clipped(
            &toggle_label,
            x + 20.0 * s,
            footer.summary_baseline,
            (width - 40.0 * s).max(4.0),
            [0.66, 0.70, 0.80, 1.0],
            DATABASE_DIALOG_SECONDARY_TEXT_SCALE,
            &mut toggle_scratch,
        );
        let mut control_x = x + 20.0 * s;
        for (id, text, width, target) in [
            (
                UiId::DatabaseDialogTls,
                "TLS",
                66.0,
                DatabaseDialogTooltipTarget::Tls,
            ),
            (
                UiId::DatabaseDialogColor,
                "Цвет",
                66.0,
                DatabaseDialogTooltipTarget::Color,
            ),
            (
                UiId::DatabaseDialogSshToggle,
                "SSH",
                66.0,
                DatabaseDialogTooltipTarget::Ssh,
            ),
            (
                UiId::DatabaseDialogJumpToggle,
                "Бастион",
                92.0,
                DatabaseDialogTooltipTarget::Jump,
            ),
        ] {
            let button = crate::widgets::ButtonView {
                x: control_x,
                y: footer.toggle_y,
                w: width * s,
                h: 30.0 * s,
                text,
                icon: None,
                text_scale: 0.78,
                icon_size: 0.0,
            };
            if ui_registry.register_button_view(id, button, self, mx, my, s, false) {
                hovered_tooltip = Some((target, control_x + width * s, footer.toggle_y + 30.0 * s));
            }
            control_x += (width + 6.0) * s;
        }
        let right = x + width - 20.0 * s;
        let cancel_w = 86.0 * s;
        let save_w = 92.0 * s;
        let test_w = 120.0 * s;
        let cancel_x = right - cancel_w;
        let save_x = cancel_x - 8.0 * s - save_w;
        let test_x = save_x - 8.0 * s - test_w;
        for (id, bx, bw, text) in [
            (UiId::DatabaseDialogTest, test_x, test_w, "Проверить"),
            (UiId::DatabaseDialogSave, save_x, save_w, "Сохранить"),
            (UiId::DatabaseDialogCancel, cancel_x, cancel_w, "Отмена"),
        ] {
            ui_registry.register_button_view(
                id,
                crate::widgets::ButtonView {
                    x: bx,
                    y: footer.actions_y,
                    w: bw,
                    h: 30.0 * s,
                    text,
                    icon: None,
                    text_scale: 0.78,
                    icon_size: 0.0,
                },
                self,
                mx,
                my,
                s,
                false,
            );
        }
        let mut footer_scratch = String::new();
        if let Some(error) = dialog.error.as_deref() {
            self.draw_tree_label_clipped(
                error,
                x + 20.0 * s,
                footer.message_baseline,
                (width - 40.0 * s).max(20.0),
                [0.95, 0.38, 0.42, 1.0],
                0.78,
                &mut footer_scratch,
            );
        } else if let Some(status) = dialog.test_status.as_deref() {
            self.draw_tree_label_clipped(
                status,
                x + 20.0 * s,
                footer.message_baseline,
                (width - 40.0 * s).max(20.0),
                [0.45, 0.85, 0.56, 1.0],
                0.78,
                &mut footer_scratch,
            );
        } else if dialog.jump_enabled {
            self.draw_tree_label_clipped(
                "Бастион — промежуточный SSH-сервер между RRiter и сервером PostgreSQL.",
                x + 20.0 * s,
                footer.message_baseline,
                (width - 40.0 * s).max(20.0),
                [0.72, 0.75, 0.82, 1.0],
                DATABASE_DIALOG_SECONDARY_TEXT_SCALE,
                &mut footer_scratch,
            );
        }

        if let Some((target, anchor_x, anchor_y)) = hovered_tooltip {
            let tooltip_anchor = self.delayed_tooltip_anchor(
                Some(target.key()),
                anchor_x,
                anchor_y,
                std::time::Instant::now(),
            );
            if let Some((tooltip_x, tooltip_y)) = tooltip_anchor
                && !self.hide_popups_until_mouse_move
            {
                self.draw_database_dialog_tooltip(target.text(), tooltip_x, tooltip_y, s);
            }
        } else {
            self.reset_delayed_tooltip_anchor_namespace(DATABASE_DIALOG_TOOLTIP_NAMESPACE);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DatabaseDialogFooterLayout {
    form_bottom: f32,
    message_baseline: f32,
    summary_baseline: f32,
    toggle_y: f32,
    actions_y: f32,
}

fn database_dialog_footer_layout(y: f32, height: f32, s: f32) -> DatabaseDialogFooterLayout {
    let bottom = y + height;
    DatabaseDialogFooterLayout {
        form_bottom: bottom - 146.0 * s,
        message_baseline: bottom - 120.0 * s,
        summary_baseline: bottom - 94.0 * s,
        toggle_y: bottom - 84.0 * s,
        actions_y: bottom - 42.0 * s,
    }
}

fn database_remember_control(
    field: crate::app::database::DatabaseFormField,
    dialog: &crate::app::database::DatabaseConnectionDialog,
) -> Option<(crate::ui_system::UiId, bool)> {
    use crate::app::database::DatabaseFormField as Field;
    use crate::ui_system::UiId;
    match field {
        Field::PostgresPassword => Some((
            UiId::DatabaseDialogRememberPostgres,
            dialog.remember_postgres_password,
        )),
        Field::SshPassword => Some((
            UiId::DatabaseDialogRememberSshPassword,
            dialog.remember_ssh_password,
        )),
        Field::SshKeyPassphrase => Some((
            UiId::DatabaseDialogRememberSshPassphrase,
            dialog.remember_ssh_key_passphrase,
        )),
        Field::JumpPassword => Some((
            UiId::DatabaseDialogRememberJumpPassword,
            dialog.remember_jump_password,
        )),
        Field::JumpKeyPassphrase => Some((
            UiId::DatabaseDialogRememberJumpPassphrase,
            dialog.remember_jump_key_passphrase,
        )),
        _ => None,
    }
}

fn database_tree_icon_y(row_y: f32, row_h: f32, icon_size: f32) -> f32 {
    (row_y + (row_h - icon_size) * 0.5).round()
}

fn draw_database_hint(renderer: &mut Renderer, text: &str, x: f32, y: f32, s: f32) {
    let row_h = (crate::render_view::tree_ui::TREE_ROW_H * s).round();
    renderer.draw_string_scaled_pixel_snapped(
        text,
        x.round(),
        Renderer::tree_row_text_y(y.round(), row_h, s),
        [0.48, 0.50, 0.57, 1.0],
        0.78,
    );
}

fn database_connection_color(color: crate::app::database::DatabaseConnectionColor) -> [f32; 4] {
    use crate::app::database::DatabaseConnectionColor::*;
    match color {
        Blue => [0.35, 0.58, 0.95, 1.0],
        Green => [0.35, 0.82, 0.48, 1.0],
        Yellow => [0.92, 0.78, 0.30, 1.0],
        Orange => [0.95, 0.55, 0.26, 1.0],
        Red => [0.92, 0.34, 0.38, 1.0],
        Purple => [0.67, 0.42, 0.92, 1.0],
        Cyan => [0.30, 0.78, 0.86, 1.0],
        Gray => [0.56, 0.58, 0.64, 1.0],
    }
}

fn database_context_action_label(action: crate::app::database::DatabaseContextAction) -> &'static str {
    use crate::app::database::DatabaseContextAction::*;
    match action {
        OpenSql => "SQL-консоль",
        NewSqlConsole => "Новая SQL-консоль",
        Refresh => "Обновить",
        EditConnection => "Изменить подключение",
        TestConnection => "Проверить подключение",
        DeleteConnection => "Удалить",
        CloseConnection => "Закрыть соединение",
        ShowDdl => "Показать DDL",
        EditData => "Изменить данные",
    }
}

fn database_field_label(field: crate::app::database::DatabaseFormField) -> &'static str {
    use crate::app::database::DatabaseFormField::*;
    match field {
        DisplayName => "Имя подключения*",
        Host => "PostgreSQL host*",
        Port => "PostgreSQL port*",
        Username => "PostgreSQL user*",
        PostgresPassword => "PostgreSQL password*",
        MaintenanceDatabase => "Служебная база*",
        SshHost => "SSH host*",
        SshPort => "SSH port*",
        SshUsername => "SSH user*",
        SshPassword => "SSH password*",
        SshPrivateKey => "SSH private key*",
        SshKeyPassphrase => "SSH key passphrase*",
        SshConfigAlias => "SSH config alias*",
        JumpHost => "Бастион host*",
        JumpPort => "Бастион port*",
        JumpUsername => "Бастион user*",
        JumpPassword => "Бастион password*",
        JumpPrivateKey => "Бастион private key*",
        JumpKeyPassphrase => "Бастион passphrase*",
        JumpConfigAlias => "Бастион config alias*",
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_database_confirmation(
    renderer: &mut Renderer,
    s: f32,
    ui_registry: &mut crate::ui_system::UiRegistry,
    mx: f32,
    my: f32,
    title: &str,
    detail: &str,
    confirm_id: crate::ui_system::UiId,
    cancel_id: crate::ui_system::UiId,
    confirm_enabled: bool,
) {
    use crate::ui_system::UiId;
    ui_registry.mark_overlay_start();
    renderer.push_rect(0.0, 0.0, renderer.width, renderer.height, [0.0, 0.0, 0.0, 0.62]);
    ui_registry.register_blocker(
        UiId::DatabaseDialogBackdrop,
        0.0,
        0.0,
        renderer.width,
        renderer.height,
        mx,
        my,
    );
    let geometry = database_modal_geometry(
        renderer.width,
        renderer.height,
        s,
        520.0,
        210.0,
        300.0,
        170.0,
    );
    let s = geometry.scale;
    let (x, y, w, h) = (geometry.x, geometry.y, geometry.w, geometry.h);
    renderer.push_rounded_rect(x, y, w, h, 8.0 * s, [0.12, 0.125, 0.16, 1.0]);
    ui_registry.register_blocker(UiId::DatabaseDialogBody, x, y, w, h, mx, my);
    let mut scratch = String::new();
    renderer.draw_tree_label_clipped(
        title,
        x + 22.0 * s,
        y + 36.0 * s,
        (w - 44.0 * s).max(4.0),
        renderer.theme.fg,
        1.0,
        &mut scratch,
    );
    renderer.draw_tree_label_clipped(
        detail,
        x + 22.0 * s,
        y + 78.0 * s,
        (w - 44.0 * s).max(4.0),
        [0.72, 0.74, 0.80, 1.0],
        0.82,
        &mut scratch,
    );
    let inner_w = (w - 44.0 * s).max(1.0);
    let button_h = 30.0 * s;
    let gap = 10.0 * s;
    let nominal_confirm = 96.0 * s;
    let nominal_cancel = 92.0 * s;
    let horizontal = inner_w >= nominal_confirm + nominal_cancel + gap;
    let (confirm_x, confirm_y, confirm_w, cancel_x, cancel_y, cancel_w) = if horizontal {
        let cancel_x = x + w - 22.0 * s - nominal_cancel;
        (
            cancel_x - gap - nominal_confirm,
            y + h - 48.0 * s,
            nominal_confirm,
            cancel_x,
            y + h - 48.0 * s,
            nominal_cancel,
        )
    } else {
        let full = inner_w;
        (
            x + 22.0 * s,
            y + h - 84.0 * s,
            full,
            x + 22.0 * s,
            y + h - 48.0 * s,
            full,
        )
    };
    if confirm_enabled {
        ui_registry.register_button_view(
            confirm_id,
            crate::widgets::ButtonView {
                x: confirm_x,
                y: confirm_y,
                w: confirm_w,
                h: button_h,
                text: "Удалить",
                icon: None,
                text_scale: 0.8,
                icon_size: 0.0,
            },
            renderer,
            mx,
            my,
            s,
            false,
        );
    }
    ui_registry.register_button_view(
        cancel_id,
        crate::widgets::ButtonView {
            x: cancel_x,
            y: cancel_y,
            w: cancel_w,
            h: button_h,
            text: "Отмена",
            icon: None,
            text_scale: 0.8,
            icon_size: 0.0,
        },
        renderer,
        mx,
        my,
        s,
        false,
    );
}

fn draw_database_host_key_confirmation(
    renderer: &mut Renderer,
    s: f32,
    ui_registry: &mut crate::ui_system::UiRegistry,
    mx: f32,
    my: f32,
    detail: &str,
) {
    use crate::ui_system::UiId;
    ui_registry.mark_overlay_start();
    renderer.push_rect(0.0, 0.0, renderer.width, renderer.height, [0.0, 0.0, 0.0, 0.64]);
    ui_registry.register_blocker(
        UiId::DatabaseDialogBackdrop,
        0.0,
        0.0,
        renderer.width,
        renderer.height,
        mx,
        my,
    );
    let geometry = database_modal_geometry(
        renderer.width,
        renderer.height,
        s,
        620.0,
        300.0,
        300.0,
        230.0,
    );
    let s = geometry.scale;
    let (x, y, w, h) = (geometry.x, geometry.y, geometry.w, geometry.h);
    renderer.push_rounded_rect(x, y, w, h, 8.0 * s, [0.12, 0.125, 0.16, 1.0]);
    ui_registry.register_blocker(UiId::DatabaseDialogBody, x, y, w, h, mx, my);
    let mut scratch = String::new();
    renderer.draw_tree_label_clipped(
        "Неизвестный SSH host key",
        x + 22.0 * s,
        y + 36.0 * s,
        (w - 44.0 * s).max(4.0),
        renderer.theme.fg,
        1.0,
        &mut scratch,
    );
    for (idx, line) in detail.lines().take(4).enumerate() {
        renderer.draw_tree_label_clipped(
            line,
            x + 22.0 * s,
            y + (76.0 + idx as f32 * 24.0) * s,
            (w - 44.0 * s).max(4.0),
            [0.72, 0.75, 0.82, 1.0],
            0.82,
            &mut scratch,
        );
    }
    let buttons = [
        (UiId::DatabaseHostKeyTrustOnce, "Доверять один раз", 150.0),
        (UiId::DatabaseHostKeyTrustStore, "Доверять и сохранить", 180.0),
        (UiId::DatabaseHostKeyCancel, "Отмена", 90.0),
    ];
    let inner_w = (w - 44.0 * s).max(1.0);
    let gap = 10.0 * s;
    let horizontal = database_host_key_buttons_horizontal(inner_w, s);
    let mut horizontal_offset = 0.0;
    for (index, (id, text, nominal_w)) in buttons.iter().copied().enumerate() {
        let (bx, by, bw) = if horizontal {
            let bx = x + 22.0 * s + horizontal_offset;
            horizontal_offset += nominal_w * s + gap;
            (bx, y + h - 48.0 * s, nominal_w * s)
        } else {
            (
                x + 22.0 * s,
                y + h - (120.0 - index as f32 * 36.0) * s,
                inner_w,
            )
        };
        ui_registry.register_button_view(
            id,
            crate::widgets::ButtonView {
                x: bx,
                y: by,
                w: bw,
                h: 30.0 * s,
                text,
                icon: None,
                text_scale: 0.78,
                icon_size: 0.0,
            },
            renderer,
            mx,
            my,
            s,
            false,
        );
    }
}

#[cfg(test)]
mod database_dialog_layout_tests {
    use super::*;

    #[test]
    fn bug_25_connection_modal_never_exceeds_small_viewport() {
        let geometry = database_modal_geometry(300.0, 260.0, 1.0, 700.0, 780.0, 420.0, 420.0);
        assert!(geometry.x >= 0.0);
        assert!(geometry.y >= 0.0);
        assert!(geometry.x + geometry.w <= 300.0 + 0.5);
        assert!(geometry.y + geometry.h <= 260.0 + 0.5);
    }

    #[test]
    fn bug_27_connection_summary_is_given_only_modal_inner_width() {
        let geometry = database_modal_geometry(360.0, 500.0, 1.0, 700.0, 780.0, 420.0, 420.0);
        let clipped_width = (geometry.w - 40.0 * geometry.scale).max(4.0);
        assert!(clipped_width <= geometry.w);
        assert!(geometry.x + 20.0 * geometry.scale + clipped_width <= geometry.x + geometry.w + 0.5);
    }

    #[test]
    fn bug_28_host_key_buttons_stack_when_they_do_not_fit() {
        assert!(database_host_key_buttons_horizontal(500.0, 1.0));
        assert!(!database_host_key_buttons_horizontal(300.0, 1.0));
        assert!(!database_host_key_buttons_horizontal(150.0, 0.75));
    }

    #[test]
    fn bug_29_confirmation_modals_never_exceed_short_viewport() {
        for desired_h in [230.0, 300.0] {
            let geometry = database_modal_geometry(420.0, 150.0, 1.0, 620.0, desired_h, 300.0, 170.0);
            assert!(geometry.y >= 0.0);
            assert!(geometry.y + geometry.h <= 150.0 + 0.5);
        }
    }

    #[test]
    fn bug_30_database_tree_disables_hover_while_scroll_is_moving() {
        let source = include_str!("ide_panel_database_renderer.rs");
        assert!(source.contains("let hover_settled = database.scroll.is_settled();"));
        assert!(source.contains("if hover_settled"));
    }

    #[test]
    fn database_sql_actions_are_named_as_console_actions() {
        assert_eq!(
            database_context_action_label(
                crate::app::database::DatabaseContextAction::OpenSql
            ),
            "SQL-консоль"
        );
        assert_eq!(
            database_context_action_label(
                crate::app::database::DatabaseContextAction::NewSqlConsole
            ),
            "Новая SQL-консоль"
        );
    }

    #[test]
    fn database_global_error_reserves_space_above_tree_content() {
        let without_error = database_panel_content_layout(20.0, 400.0, 34.0, 1.0, false);
        let with_error = database_panel_content_layout(20.0, 400.0, 34.0, 1.0, true);

        assert_eq!(without_error.error_h, 0.0);
        assert_eq!(without_error.content_y, 54.0);
        assert_eq!(with_error.error_y, 54.0);
        assert_eq!(with_error.error_h, 44.0);
        assert_eq!(with_error.content_y, 98.0);
        assert_eq!(with_error.content_h, without_error.content_h - 44.0);

        let short = database_panel_content_layout(0.0, 40.0, 34.0, 1.0, true);
        assert_eq!(short.error_h, 6.0);
        assert_eq!(short.content_h, 0.0);
        assert!(short.content_y <= 40.0);
    }
}
