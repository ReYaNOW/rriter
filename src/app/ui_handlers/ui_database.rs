use crate::app::App;
use crate::ui_system::UiId;
use super::repeated_ui_click;

#[cfg_attr(coverage_nightly, coverage(off))]
impl App {
    pub(super) fn handle_database_ui_click(&mut self, id: UiId, same_click_target: bool) {
        use crate::app::database::{DatabaseConnectionColor, PostgresTlsMode, SshHostKeyPolicy};

        if database_table_click_closes_cell_popup(id)
            && let Some(tab_id) = self.active_database_table_tab_id()
            && let Some((_, state)) = self.database_table_meta_state_mut(tab_id)
            && state.grid.cell_editor.as_ref().is_some_and(|editor| {
                matches!(
                    editor.kind,
                    crate::app::database::DatabaseCellEditorKind::Enum
                        | crate::app::database::DatabaseCellEditorKind::DateTime
                )
            })
        {
            state.grid.cell_editor = None;
            if state.grid.focused_input
                == Some(crate::app::database::DatabaseTableInputTarget::Cell)
            {
                state.grid.focused_input = None;
            }
        }

        match id {
            UiId::DatabasePanelBody
            | UiId::DatabaseDialogBody
            | UiId::DatabaseDdlBody
            | UiId::DatabaseDdlScroll
            | UiId::DatabaseTableGridBody
            | UiId::DatabaseTableModalBody => {}
            UiId::DatabaseGlobalErrorCopy => {
                if let Some(error) = self.ide_panel.database.global_error.clone() {
                    self.set_clipboard_text(error);
                }
            }
            UiId::DatabaseTableBody => {
                if let Some(tab_id) = self.active_database_table_tab_id()
                    && let Some((_, state)) = self.database_table_meta_state_mut(tab_id)
                {
                    state.clear_unavailable_selection();
                }
            }
            UiId::DatabaseTableUnavailableText => {
                let mouse_x = self
                    .renderer
                    .as_ref()
                    .map_or(0.0, |renderer| renderer.last_mouse_x);
                let input_index = self.database_table_unavailable_text_index_at(mouse_x);
                if let Some(tab_id) = self.active_database_table_tab_id()
                    && let Some((_, state)) = self.database_table_meta_state_mut(tab_id)
                {
                    state.unavailable_text_focused = true;
                    state.unavailable_text_dragging = input_index.is_some();
                }
                if let Some(input_index) = input_index {
                    self.set_database_table_unavailable_text_cursor(input_index, false);
                }
            }
            UiId::DatabaseAdd => self.open_database_connection_dialog(),
            UiId::DatabaseDelete => {
                if let Some(id) = self.ide_panel.database.selected_connection {
                    self.request_delete_database_connection(id);
                }
            }
            UiId::DatabaseRefresh => self.refresh_selected_database(),
            UiId::DatabaseConnectionRow(index) => {
                if let Some(id) = self
                    .ide_panel
                    .database
                    .connections
                    .get(index)
                    .map(|node| node.config.id)
                {
                    self.select_database_connection(id);
                }
            }
            UiId::DatabaseConnectionArrow(index) => {
                if let Some(id) = self
                    .ide_panel
                    .database
                    .connections
                    .get(index)
                    .map(|node| node.config.id)
                {
                    self.toggle_database_connection(id);
                }
            }
            UiId::DatabaseRow(connection_index, database_index) => {
                let selected = self
                    .ide_panel
                    .database
                    .connections
                    .get(connection_index)
                    .and_then(|node| {
                        node.databases
                            .get(database_index)
                            .map(|database| (node.config.id, database.name.clone()))
                    });
                if let Some((id, database_name)) = selected {
                    self.ide_panel.database.selected_connection = Some(id);
                    self.ide_panel.database.selected_database = Some((id, database_name));
                }
            }
            UiId::DatabaseArrow(connection_index, database_index) => {
                if let Some(id) = self
                    .ide_panel
                    .database
                    .connections
                    .get(connection_index)
                    .map(|node| node.config.id)
                {
                    self.toggle_database_node(id, database_index);
                }
            }
            UiId::DatabaseTableRow(connection_index, database_index, table_index) => {
                let target = self
                    .ide_panel
                    .database
                    .connections
                    .get(connection_index)
                    .and_then(|node| {
                        node.databases.get(database_index).and_then(|database| {
                            database.tables.get(table_index).map(|table| {
                                (node.config.id, database.name.clone(), table.name.clone())
                            })
                        })
                    });
                if let Some((id, database, table)) = target {
                    let now = std::time::Instant::now();
                    let table_key = (id, database.clone(), table.clone());
                    let double_click = self
                        .ide_panel
                        .database
                        .last_table_click
                        .as_ref()
                        .is_some_and(|(previous, at)| {
                            previous == &table_key
                                && now.saturating_duration_since(*at)
                                    <= std::time::Duration::from_millis(500)
                        });
                    self.ide_panel.database.selected_connection = Some(id);
                    self.ide_panel.database.selected_database = Some((id, database.clone()));
                    self.ide_panel.database.selected_table = Some(table_key.clone());
                    self.ide_panel.database.last_table_click = Some((table_key, now));
                    self.ide_panel.database.notice =
                        Some(format!("Выбрана таблица public.{table}"));
                    if double_click {
                        self.open_database_table_tab(id, &database, &table);
                    }
                }
            }
            UiId::DatabaseContextItem(index) => self.activate_database_context_action(index),
            UiId::DatabaseDialogField(field) => {
                let mouse_x = self
                    .renderer
                    .as_ref()
                    .map_or(0.0, |renderer| renderer.last_mouse_x);
                let target = self.database_dialog_input_index_at(field, mouse_x);
                if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
                    dialog.focused = Some(field);
                    dialog.dragging_field = Some(field);
                    dialog.error = None;
                    dialog.test_status = None;
                }
                self.last_action = std::time::Instant::now();
                self.last_blink_state = true;
                if let Some(target) = target {
                    self.set_database_dialog_input_cursor(field, target, false);
                }
            }
            UiId::DatabaseDialogSecretEye(field) => {
                if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
                    dialog.toggle_secret_visibility(field);
                    dialog.focused = Some(field);
                    dialog.error = None;
                    dialog.test_status = None;
                }
                self.last_action = std::time::Instant::now();
                self.last_blink_state = true;
            }
            UiId::DatabaseDialogTls => {
                if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
                    dialog.tls_mode = match dialog.tls_mode {
                        PostgresTlsMode::Disable => PostgresTlsMode::Prefer,
                        PostgresTlsMode::Prefer => PostgresTlsMode::Require,
                        PostgresTlsMode::Require => PostgresTlsMode::Disable,
                    };
                    dialog.error = None;
                    dialog.test_status = None;
                }
            }
            UiId::DatabaseDialogColor => {
                if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
                    dialog.color = match dialog.color {
                        DatabaseConnectionColor::Blue => DatabaseConnectionColor::Green,
                        DatabaseConnectionColor::Green => DatabaseConnectionColor::Yellow,
                        DatabaseConnectionColor::Yellow => DatabaseConnectionColor::Orange,
                        DatabaseConnectionColor::Orange => DatabaseConnectionColor::Red,
                        DatabaseConnectionColor::Red => DatabaseConnectionColor::Purple,
                        DatabaseConnectionColor::Purple => DatabaseConnectionColor::Cyan,
                        DatabaseConnectionColor::Cyan => DatabaseConnectionColor::Gray,
                        DatabaseConnectionColor::Gray => DatabaseConnectionColor::Blue,
                    };
                    dialog.error = None;
                    dialog.test_status = None;
                }
            }
            UiId::DatabaseDialogSshToggle => {
                if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
                    dialog.ssh_enabled = !dialog.ssh_enabled;
                    if !dialog.ssh_enabled {
                        dialog.jump_enabled = false;
                        if dialog
                            .focused
                            .is_some_and(|field| dialog.visible_field_index(field).is_none())
                        {
                            dialog.focused =
                                Some(crate::app::database::DatabaseFormField::MaintenanceDatabase);
                        }
                    }
                    dialog.error = None;
                    dialog.test_status = None;
                }
                self.clamp_database_dialog_scroll_to_layout();
            }
            UiId::DatabaseDialogJumpToggle => {
                if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
                    dialog.toggle_jump_host();
                }
                self.clamp_database_dialog_scroll_to_layout();
                self.ensure_database_dialog_focus_visible();
            }
            UiId::DatabaseDialogRememberPostgres => {
                if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
                    dialog.remember_postgres_password = !dialog.remember_postgres_password;
                }
            }
            UiId::DatabaseDialogRememberSshPassword => {
                if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
                    dialog.remember_ssh_password = !dialog.remember_ssh_password;
                }
            }
            UiId::DatabaseDialogRememberSshPassphrase => {
                if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
                    dialog.remember_ssh_key_passphrase = !dialog.remember_ssh_key_passphrase;
                }
            }
            UiId::DatabaseDialogRememberJumpPassword => {
                if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
                    dialog.remember_jump_password = !dialog.remember_jump_password;
                }
            }
            UiId::DatabaseDialogRememberJumpPassphrase => {
                if let Some(dialog) = self.ide_panel.database.dialog.as_mut() {
                    dialog.remember_jump_key_passphrase = !dialog.remember_jump_key_passphrase;
                }
            }
            UiId::DatabaseDialogTest => self.test_database_dialog_connection(),
            UiId::DatabaseDialogSave => self.save_database_connection_dialog(),
            UiId::DatabaseDialogCancel => self.cancel_database_dialog(),
            UiId::DatabaseDialogBackdrop => {
                if self.ide_panel.database.dialog.is_some() {
                    self.cancel_database_dialog();
                } else if self.ide_panel.database.delete_prompt.is_some() {
                    self.cancel_delete_database_connection();
                } else if self.ide_panel.database.host_key_prompt.is_some() {
                    self.cancel_database_host_key_prompt();
                }
            }
            UiId::DatabaseDeleteConfirm => self.confirm_delete_database_connection(),
            UiId::DatabaseDeleteCancel => self.cancel_delete_database_connection(),
            UiId::DatabaseHostKeyTrustOnce => {
                self.resolve_database_host_key(SshHostKeyPolicy::TrustOnce)
            }
            UiId::DatabaseHostKeyTrustStore => {
                self.resolve_database_host_key(SshHostKeyPolicy::TrustAndStore)
            }
            UiId::DatabaseHostKeyCancel => self.cancel_database_host_key_prompt(),
            UiId::DatabaseTableAddRow => {
                if let Some(tab) = self.active_database_table_tab_id() {
                    self.add_database_table_row(tab);
                }
            }
            UiId::DatabaseTableDeleteRows => {
                if let Some(tab) = self.active_database_table_tab_id() {
                    self.delete_database_table_selection(tab);
                }
            }
            UiId::DatabaseTableUndo => {
                if let Some(tab) = self.active_database_table_tab_id() {
                    self.undo_database_table_selection(tab);
                }
            }
            UiId::DatabaseTableSave => {
                if let Some(tab) = self.active_database_table_tab_id() {
                    self.save_database_table_changes(tab, false);
                }
            }
            UiId::DatabaseTablePreview => {
                if let Some(tab) = self.active_database_table_tab_id() {
                    self.preview_database_table_changes(tab);
                }
            }
            UiId::DatabaseTableRefresh => {
                if let Some(tab) = self.active_database_table_tab_id() {
                    self.request_database_table_refresh(tab);
                }
            }
            UiId::DatabaseTablePageFirst => {
                if let Some(tab) = self.active_database_table_tab_id() {
                    self.database_table_page_first(tab);
                }
            }
            UiId::DatabaseTablePagePrevious => {
                if let Some(tab) = self.active_database_table_tab_id() {
                    self.database_table_page_previous(tab);
                }
            }
            UiId::DatabaseTablePageNext => {
                if let Some(tab) = self.active_database_table_tab_id() {
                    self.database_table_page_next(tab);
                }
            }
            UiId::DatabaseTablePageLast => {
                if let Some(tab) = self.active_database_table_tab_id() {
                    self.database_table_page_last(tab);
                }
            }
            UiId::DatabaseTableLimit => {
                if let Some(tab) = self.active_database_table_tab_id() {
                    self.open_database_table_limit_dialog(tab);
                }
            }
            UiId::DatabaseTableModalInput => {
                let mouse = self.renderer.as_ref().map_or((0.0, 0.0), |renderer| {
                    (renderer.last_mouse_x, renderer.last_mouse_y)
                });
                let input_index = self.database_table_modal_input_index_at(mouse.0, mouse.1);
                self.ide_panel.database.table_modal_input_dragging = input_index.is_some();
                self.last_action = std::time::Instant::now();
                self.last_blink_state = true;
                if let Some(input_index) = input_index {
                    self.set_database_table_modal_input_cursor(input_index, false);
                }
            }
            UiId::DatabaseTableWhereInput => {
                let target = crate::app::database::DatabaseTableInputTarget::Where;
                let mouse_x = self
                    .renderer
                    .as_ref()
                    .map_or(0.0, |renderer| renderer.last_mouse_x);
                let input_index = self.database_table_input_index_at(target, mouse_x);
                if let Some(tab) = self.active_database_table_tab_id()
                    && let Some((_, state)) = self.database_table_meta_state_mut(tab)
                {
                    state.grid.focused_input = Some(target);
                    state.grid.text_drag = Some(target);
                    state.grid.cell_editor = None;
                }
                self.last_action = std::time::Instant::now();
                self.last_blink_state = true;
                if let Some(input_index) = input_index {
                    self.set_database_table_input_cursor(target, input_index, false);
                }
                self.close_autocomplete();
            }
            UiId::DatabaseTableOrderInput => {
                let target = crate::app::database::DatabaseTableInputTarget::OrderBy;
                let mouse_x = self
                    .renderer
                    .as_ref()
                    .map_or(0.0, |renderer| renderer.last_mouse_x);
                let input_index = self.database_table_input_index_at(target, mouse_x);
                if let Some(tab) = self.active_database_table_tab_id()
                    && let Some((_, state)) = self.database_table_meta_state_mut(tab)
                {
                    state.grid.focused_input = Some(target);
                    state.grid.text_drag = Some(target);
                    state.grid.cell_editor = None;
                }
                self.last_action = std::time::Instant::now();
                self.last_blink_state = true;
                if let Some(input_index) = input_index {
                    self.set_database_table_input_cursor(target, input_index, false);
                }
                self.close_autocomplete();
            }
            UiId::DatabaseTableCellEditor => {
                let target = crate::app::database::DatabaseTableInputTarget::Cell;
                let mouse_x = self
                    .renderer
                    .as_ref()
                    .map_or(0.0, |renderer| renderer.last_mouse_x);
                let input_index = self.database_table_input_index_at(target, mouse_x);
                if let Some(tab) = self.active_database_table_tab_id()
                    && let Some((_, state)) = self.database_table_meta_state_mut(tab)
                {
                    state.grid.focused_input = Some(target);
                    state.grid.text_drag = Some(target);
                }
                self.last_action = std::time::Instant::now();
                self.last_blink_state = true;
                if let Some(input_index) = input_index {
                    self.set_database_table_input_cursor(target, input_index, false);
                }
            }
            UiId::DatabaseTableHeader(column) => {
                if let Some(tab) = self.active_database_table_tab_id() {
                    self.cycle_database_table_sort(tab, column);
                }
            }
            UiId::DatabaseTableColumnResize(column) => {
                if let Some(tab) = self.active_database_table_tab_id() {
                    let now = std::time::Instant::now();
                    let mouse = self.renderer.as_ref().map_or((0.0, 0.0), |renderer| {
                        (renderer.last_mouse_x, renderer.last_mouse_y)
                    });
                    let double = repeated_ui_click(
                        same_click_target,
                        now.duration_since(self.last_click_time),
                        mouse.0 - self.last_click_pos.0,
                        mouse.1 - self.last_click_pos.1,
                    );
                    self.last_click_time = now;
                    self.last_click_pos = mouse;
                    if double {
                        self.auto_size_database_table_column(tab, column);
                    } else {
                        self.start_database_table_column_resize(tab, column, mouse.0);
                    }
                }
            }
            UiId::DatabaseGridRow(row) => {
                let extend = self.modifiers.shift_key();
                let toggle = self.modifiers.control_key() || self.modifiers.super_key();
                if let Some(tab) = self.active_database_table_tab_id()
                    && let Some((_, state)) = self.database_table_meta_state_mut(tab)
                {
                    state.grid.select_row(row, extend, toggle);
                    state.grid.focused_input = None;
                }
            }
            UiId::DatabaseTableCell(row, column) => {
                let now = std::time::Instant::now();
                let mouse = self.renderer.as_ref().map_or((0.0, 0.0), |renderer| {
                    (renderer.last_mouse_x, renderer.last_mouse_y)
                });
                let double = repeated_ui_click(
                    same_click_target,
                    now.duration_since(self.last_click_time),
                    mouse.0 - self.last_click_pos.0,
                    mouse.1 - self.last_click_pos.1,
                );
                self.last_click_time = now;
                self.last_click_pos = mouse;
                self.handle_database_table_cell_click(row, column, double);
            }
            UiId::DatabaseTableEnumOption(option) => self.select_database_table_enum_option(option),
            UiId::DatabaseTableEnumPreviousPage => self.page_database_table_enum_options(false),
            UiId::DatabaseTableEnumNextPage => self.page_database_table_enum_options(true),
            UiId::DatabaseTableDatePreviousMonth => self.shift_database_table_calendar_month(-1),
            UiId::DatabaseTableDateNextMonth => self.shift_database_table_calendar_month(1),
            UiId::DatabaseTableDateDay(day) => self.select_database_table_calendar_day(day as u32),
            UiId::DatabaseTableDateToday => self.set_database_table_date_today(),
            UiId::DatabaseTableDateNow => self.set_database_table_time_now_utc(),
            UiId::DatabaseTableScrollY => self.start_database_table_scroll_drag(false),
            UiId::DatabaseTableScrollX => self.start_database_table_scroll_drag(true),
            UiId::DatabaseTableModalScroll => self.start_database_sql_preview_scroll_drag(false),
            UiId::DatabaseTableModalScrollX => self.start_database_sql_preview_scroll_drag(true),
            UiId::DatabaseTableModalPrimary => self.activate_database_table_modal_action(0),
            UiId::DatabaseTableModalSecondary | UiId::DatabaseTableModalBackdrop => {
                self.activate_database_table_modal_action(1)
            }
            UiId::DatabaseTableModalTertiary => self.activate_database_table_modal_action(2),
            UiId::DatabaseQueryRun => {
                self.run_active_database_query(crate::app::database::DatabaseQueryMode::Run)
            }
            UiId::DatabaseQueryCancel => self.cancel_active_database_query(),
            UiId::DatabaseQueryExplain => {
                self.run_active_database_query(crate::app::database::DatabaseQueryMode::Explain)
            }
            UiId::DatabaseQueryExplainAnalyze => self
                .run_active_database_query(crate::app::database::DatabaseQueryMode::ExplainAnalyze),
            UiId::DatabaseQueryFormat => self.format_active_database_query(),
            UiId::DatabaseQueryHistory => self.toggle_active_database_query_history(),
            UiId::DatabaseQueryNextDiagnostic => {
                self.jump_to_next_active_database_query_diagnostic();
            }
            UiId::DatabaseQueryResultTab(index) => self.select_active_database_query_result(index),
            UiId::DatabaseQueryHistoryEntry(index) => self.load_database_query_history_entry(index),
            UiId::DatabaseQueryResultResize => self.start_database_query_result_resize(),
            UiId::DatabaseQueryColumnResize(column) => {
                let now = std::time::Instant::now();
                let mouse = self.renderer.as_ref().map_or((0.0, 0.0), |renderer| {
                    (renderer.last_mouse_x, renderer.last_mouse_y)
                });
                let double = repeated_ui_click(
                    same_click_target,
                    now.duration_since(self.last_click_time),
                    mouse.0 - self.last_click_pos.0,
                    mouse.1 - self.last_click_pos.1,
                );
                self.last_click_time = now;
                self.last_click_pos = mouse;
                if double {
                    self.auto_size_active_database_query_column(column);
                } else {
                    self.start_database_query_column_resize(column, mouse.0);
                }
            }
            UiId::DatabaseQueryScrollY => self.start_database_query_scroll_drag(false),
            UiId::DatabaseQueryScrollX => self.start_database_query_scroll_drag(true),
            UiId::DatabaseQueryReviewMessagesScrollY => {
                self.scroll_active_database_query_review_messages_to_pointer();
            }
            UiId::DatabaseQueryCommit => self.commit_active_database_query(),
            UiId::DatabaseQueryRollback | UiId::DatabaseQueryReviewBackdrop => {
                self.rollback_active_database_query();
            }
            UiId::DatabaseQueryResultBody
            | UiId::DatabaseQueryReviewBody
            | UiId::DatabaseQueryReviewMessagesBody => {}
            _ => {}
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    pub(crate) fn open_database_context_menu_for_hit(
        &mut self,
        id: UiId,
        mx: f32,
        my: f32,
    ) -> bool {
        use crate::app::database::DatabaseContextTarget;
        let target = match id {
            UiId::DatabaseConnectionRow(connection_index)
            | UiId::DatabaseConnectionArrow(connection_index) => self
                .ide_panel
                .database
                .connections
                .get(connection_index)
                .map(|node| DatabaseContextTarget::Connection(node.config.id)),
            UiId::DatabaseRow(connection_index, database_index)
            | UiId::DatabaseArrow(connection_index, database_index) => self
                .ide_panel
                .database
                .connections
                .get(connection_index)
                .map(|node| DatabaseContextTarget::Database(node.config.id, database_index)),
            UiId::DatabaseTableRow(connection_index, database_index, table_index) => self
                .ide_panel
                .database
                .connections
                .get(connection_index)
                .map(|node| {
                    DatabaseContextTarget::Table(node.config.id, database_index, table_index)
                }),
            _ => None,
        };
        if let Some(target) = target {
            self.open_database_context_menu(target, mx, my);
            true
        } else {
            false
        }
    }
}

fn database_table_click_closes_cell_popup(id: UiId) -> bool {
    !matches!(
        id,
        UiId::DatabaseTableCellEditor
            | UiId::DatabaseTableEnumOption(_)
            | UiId::DatabaseTableEnumPreviousPage
            | UiId::DatabaseTableEnumNextPage
            | UiId::DatabaseTableDatePreviousMonth
            | UiId::DatabaseTableDateNextMonth
            | UiId::DatabaseTableDateDay(_)
            | UiId::DatabaseTableDateToday
            | UiId::DatabaseTableDateNow
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_popup_stays_open_for_its_controls_and_closes_elsewhere() {
        assert!(!database_table_click_closes_cell_popup(
            UiId::DatabaseTableDateToday
        ));
        assert!(!database_table_click_closes_cell_popup(
            UiId::DatabaseTableDateDay(10)
        ));
        assert!(database_table_click_closes_cell_popup(
            UiId::DatabaseTableGridBody
        ));
        assert!(database_table_click_closes_cell_popup(
            UiId::DatabaseTableWhereInput
        ));
    }
}
