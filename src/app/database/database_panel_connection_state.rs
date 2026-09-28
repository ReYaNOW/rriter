use super::*;

fn database_dialog_scrollbar_hit(
    track: crate::ui_system::UiClipRect,
    pointer_x: f32,
    pointer_y: f32,
) -> bool {
    track.contains(pointer_x, pointer_y)
}

impl DatabasePanelState {
    pub(crate) fn dialog_field_max_bytes(field: crate::app::database::DatabaseFormField) -> usize {
        if field.is_secret() { 4096 } else { 8192 }
    }

    pub(crate) fn clamp_dialog_scroll(&mut self, max_scroll: f32) {
        if let Some(dialog) = self.dialog.as_mut() {
            dialog.clamp_scroll(max_scroll);
        }
    }

    pub(crate) fn ensure_dialog_focus_visible(
        &mut self,
        row_h: f32,
        form_clip_h: f32,
        max_scroll: f32,
    ) {
        let Some(dialog) = self.dialog.as_mut() else {
            return;
        };
        let Some(row) = dialog.focused.and_then(|field| dialog.visible_field_index(field)) else {
            dialog.clamp_scroll(max_scroll);
            return;
        };
        dialog.ensure_row_visible(row, row_h, form_clip_h, max_scroll);
    }

    pub(crate) fn set_dialog_input_cursor(
        &mut self,
        field: crate::app::database::DatabaseFormField,
        target: usize,
        selecting: bool,
    ) -> bool {
        let Some(dialog) = self.dialog.as_mut() else {
            return false;
        };
        dialog.focused = Some(field);
        dialog.input_mut(field).set_cursor(target, selecting);
        true
    }

    pub(crate) fn edit_dialog_field(
        &mut self,
        field: crate::app::database::DatabaseFormField,
        physical_key: winit::keyboard::PhysicalKey,
        logical_text: Option<&str>,
        primary: bool,
        word: bool,
        shift: bool,
        text_input_allowed: bool,
        paste_text: Option<&str>,
    ) -> Option<String> {
        let dialog = self.dialog.as_mut()?;
        let copy_text = crate::app::single_line_input::handle_single_line_input(
            dialog.input_mut(field),
            physical_key,
            logical_text,
            primary,
            word,
            shift,
            text_input_allowed,
            paste_text,
            Self::dialog_field_max_bytes(field),
        );
        dialog.error = None;
        dialog.test_status = None;
        copy_text
    }

    pub(crate) fn focus_next_dialog_field(&mut self, shift: bool) {
        if let Some(dialog) = self.dialog.as_mut() {
            dialog.focus_next(shift);
        }
    }

    pub(crate) fn validate_dialog_config(
        &mut self,
        fallback_id: DatabaseConnectionId,
        status: &str,
    ) -> Option<(crate::app::database::DatabaseConnectionConfig, DatabaseSecretBundle, u64)> {
        let dialog = self.dialog.as_mut()?;
        let connection = match dialog.build_config(fallback_id) {
            Ok(connection) => connection,
            Err(error) => {
                dialog.error = Some(error);
                return None;
            }
        };
        let secrets = dialog.secret_bundle();
        dialog.error = None;
        dialog.test_status = Some(status.to_string());
        Some((connection, secrets, dialog.session_id))
    }

    pub(crate) fn open_connection_dialog(&mut self) {
        let color = self.settings().default_connection_color;
        let session_id = self.allocate_dialog_id();
        let mut dialog = DatabaseConnectionDialog::new(color);
        dialog.session_id = session_id;
        self.dialog = Some(dialog);
        self.context_menu = None;
        self.ddl_hover.borrow_mut().take();
    }

    pub(crate) fn edit_connection_dialog(&mut self, connection_id: DatabaseConnectionId) {
        let Some(connection) = self
            .connection(connection_id)
            .map(|node| node.config.clone())
        else {
            return;
        };
        let session_id = self.allocate_dialog_id();
        let mut dialog = DatabaseConnectionDialog::from_connection(&connection);
        dialog.session_id = session_id;
        self.dialog = Some(dialog);
        self.context_menu = None;
        self.ddl_hover.borrow_mut().take();
    }

    pub(crate) fn dialog_owner(&self) -> Option<DatabaseJobOwner> {
        self.dialog
            .as_ref()
            .map(|dialog| DatabaseJobOwner::Dialog(dialog.session_id))
    }

    pub(crate) fn close_dialog(&mut self) {
        self.dialog = None;
    }

    pub(crate) fn cancel_delete_prompt(&mut self) {
        self.delete_prompt = None;
    }

    pub(crate) fn select_connection(&mut self, connection_id: DatabaseConnectionId) {
        self.selected_connection = Some(connection_id);
        self.selected_database = None;
    }

    pub(crate) fn toggle_database_node_selection(
        &mut self,
        connection_id: DatabaseConnectionId,
        database_idx: usize,
    ) -> Option<(String, Option<String>)> {
        let connection = self.connection_mut(connection_id)?;
        let database = connection.databases.get_mut(database_idx)?;
        database.expanded = !database.expanded;
        let selected_name = database.name.clone();
        let load_name = (database.expanded && !database.tables_loaded && !database.loading)
            .then(|| selected_name.clone());
        self.selected_connection = Some(connection_id);
        self.selected_database = Some((connection_id, selected_name.clone()));
        Some((selected_name, load_name))
    }

    pub(crate) fn close_connection(&mut self, connection_id: DatabaseConnectionId) {
        if let Some(connection) = self.connection_mut(connection_id) {
            connection.status = DatabaseConnectionStatus::Disconnected;
            connection.status_message = Some("Соединение закрыто".to_string());
            connection.loading = false;
            for database in &mut connection.databases {
                database.loading = false;
            }
        }
    }

    pub(crate) fn context_menu_entries(target: DatabaseContextTarget) -> Vec<DatabaseContextAction> {
        match target {
            DatabaseContextTarget::Connection(_) => vec![
                DatabaseContextAction::Refresh,
                DatabaseContextAction::TestConnection,
                DatabaseContextAction::EditConnection,
                DatabaseContextAction::DeleteConnection,
            ],
            DatabaseContextTarget::Database(_, _) => vec![
                DatabaseContextAction::OpenSql,
                DatabaseContextAction::NewSqlConsole,
                DatabaseContextAction::Refresh,
                DatabaseContextAction::CloseConnection,
            ],
            DatabaseContextTarget::Table(_, _, _) => vec![
                DatabaseContextAction::ShowDdl,
                DatabaseContextAction::EditData,
                DatabaseContextAction::OpenSql,
            ],
        }
    }
}

#[cfg(test)]
mod dialog_scroll_tests {
    use super::*;

    #[test]
    fn database_dialog_scrollbar_drag_requires_pointer_inside_track_on_both_axes() {
        let track = crate::ui_system::UiClipRect::new(760.0, 100.0, 8.0, 240.0);
        assert!(database_dialog_scrollbar_hit(track, 764.0, 220.0));
        assert!(!database_dialog_scrollbar_hit(track, 300.0, 220.0));
        assert!(!database_dialog_scrollbar_hit(track, 764.0, 360.0));
    }

    #[test]
    fn database_dialog_scrollbar_drag_sets_target_without_teleporting_current() {
        let track = crate::ui_system::UiClipRect::new(760.0, 100.0, 8.0, 240.0);
        let max_scroll = 480.0;
        let current = 160.0;
        let thumb = crate::scroll::scrollbar_thumb(
            track.y, track.h, track.h, track.h + max_scroll, current, 28.0,
        )
        .expect("dialog thumb");
        let pointer = thumb.start + 6.0;
        let (offset, _) = crate::scroll::scrollbar_drag_target(
            pointer, track.y, track.h, thumb, max_scroll, None,
        )
        .expect("dialog drag starts");
        let (_, target) = crate::scroll::scrollbar_drag_target(
            pointer + 30.0, track.y, track.h, thumb, max_scroll, Some(offset),
        )
        .expect("dialog drag moves");

        let mut scroll = crate::scroll::ScrollState::new(7.0);
        scroll.jump_to(current);
        assert!(crate::app::mouse::apply_scrollbar_drag_target(
            &mut scroll, target, offset,
        ));
        assert_eq!(scroll.current, current);
        assert_eq!(scroll.target, target);
        assert_eq!(scroll.drag_offset, offset);
    }
}
