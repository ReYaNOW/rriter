use crate::app::database::database_table_cell_edit_state::{
    database_multiline_edit_may_change_text, database_vertical_cursor_target,
    edit_database_table_input, line_end_boundary, line_start_boundary, next_char_boundary,
    previous_char_boundary,
};
use crate::app::database::database_table_modal_state::{
    database_sql_preview_copy_text, database_table_modal_input_mut, move_read_only_cursor,
};

impl App {
    pub(crate) fn handle_database_table_key(
        &mut self,
        key_event: &crate::app::keyboard::KeyInput,
        chord: Option<crate::keymap::Chord>,
        terminal_owns_chord: bool,
    ) -> bool {
        use winit::event::ElementState;
        use winit::keyboard::{KeyCode, PhysicalKey};
        let has_modal = self.ide_panel.database.table_modal.is_some();
        let has_table = self.active_database_table_tab_id().is_some();
        if !has_modal && !has_table {
            return false;
        }
        if key_event.state != ElementState::Pressed {
            return true;
        }
        self.last_action = std::time::Instant::now();
        self.last_blink_state = true;

        let primary = crate::platform::primary_shortcut_modifier(self.modifiers);
        let primary_mod = if crate::platform::CURRENT_PLATFORM == crate::platform::PlatformKind::Macos { crate::keymap::Mods::SUPER } else { crate::keymap::Mods::CTRL };
        let (default_table_copy_chord, default_table_undo_chord) = chord.map_or((false, false), |chord| (chord.mods == primary_mod && chord.key == KeyCode::KeyC, chord.mods == primary_mod && chord.key == KeyCode::KeyZ));
        let word = crate::platform::word_navigation_modifier(self.modifiers);
        let shift = self.modifiers.shift_key();
        let text_input_allowed = crate::platform::text_input_modifiers_allowed(self.modifiers);
        let paste_text = if primary
            && key_event.physical_key == PhysicalKey::Code(KeyCode::KeyV)
        {
            self.get_clipboard_text()
        } else {
            None
        };
        let mut copy_text = None;

        if self.ide_panel.database.table_modal.is_some() {
            if matches!(
                self.ide_panel.database.table_modal,
                Some(DatabaseTableModal::SqlPreview { .. })
            ) {
                if primary && key_event.physical_key == PhysicalKey::Code(KeyCode::KeyC) {
                    let selected = self
                        .ide_panel
                        .database
                        .table_modal
                        .as_ref()
                        .and_then(database_sql_preview_copy_text);
                    if let Some(selected) = selected {
                        self.set_clipboard_text(selected);
                    }
                    return true;
                }
                if key_event.physical_key == PhysicalKey::Code(KeyCode::Escape) {
                    self.ide_panel.database.table_modal = None;
                    return true;
                }
                if let Some(DatabaseTableModal::SqlPreview {
                    text,
                    cursor,
                    selection_anchor,
                    ..
                }) = self.ide_panel.database.table_modal.as_mut()
                {
                    let target = match key_event.physical_key {
                        PhysicalKey::Code(KeyCode::KeyA) if primary => {
                            *selection_anchor = Some(0);
                            *cursor = text.len();
                            return true;
                        }
                        PhysicalKey::Code(KeyCode::ArrowLeft) => {
                            previous_char_boundary(text, *cursor)
                        }
                        PhysicalKey::Code(KeyCode::ArrowRight) => {
                            next_char_boundary(text, *cursor)
                        }
                        PhysicalKey::Code(KeyCode::ArrowUp) => {
                            database_vertical_cursor_target(text, *cursor, -1)
                        }
                        PhysicalKey::Code(KeyCode::ArrowDown) => {
                            database_vertical_cursor_target(text, *cursor, 1)
                        }
                        PhysicalKey::Code(KeyCode::Home) => {
                            if primary { 0 } else { line_start_boundary(text, *cursor) }
                        }
                        PhysicalKey::Code(KeyCode::End) => {
                            if primary { text.len() } else { line_end_boundary(text, *cursor) }
                        }
                        _ => return true,
                    };
                    move_read_only_cursor(cursor, selection_anchor, target, shift);
                }
                return true;
            }
            match key_event.physical_key {
                PhysicalKey::Code(KeyCode::Escape) => self.activate_database_table_modal_action(1),
                PhysicalKey::Code(KeyCode::Enter | KeyCode::NumpadEnter) => {
                    if matches!(
                        self.ide_panel.database.table_modal,
                        Some(DatabaseTableModal::MultilineEditor { .. })
                    ) && !primary
                    {
                        if let Some(DatabaseTableModal::MultilineEditor { input, error, .. }) =
                            self.ide_panel.database.table_modal.as_mut()
                        {
                            input.insert("\n", crate::app::database::MAX_EDITABLE_MULTILINE_BYTES);
                            *error = None;
                        }
                        self.ide_panel
                            .database
                            .table_modal_layout_cache
                            .get_mut()
                            .invalidate();
                    } else {
                        self.activate_database_table_modal_action(0);
                    }
                }
                physical_key => {
                    let multiline = matches!(
                        self.ide_panel.database.table_modal,
                        Some(DatabaseTableModal::MultilineEditor { .. })
                    );
                    let invalidate_multiline_layout = multiline
                        && database_multiline_edit_may_change_text(
                            physical_key,
                            key_event.logical_text.as_deref(),
                            primary,
                            text_input_allowed,
                        );
                    if let Some(input) = database_table_modal_input_mut(
                        &mut self.ide_panel.database.table_modal,
                    ) {
                        copy_text = edit_database_table_input(
                            input,
                            physical_key,
                            key_event.logical_text.as_deref(),
                            primary,
                            word,
                            shift,
                            text_input_allowed,
                            paste_text,
                            if multiline {
                                crate::app::database::MAX_EDITABLE_MULTILINE_BYTES
                            } else {
                                16
                            },
                            multiline,
                        );
                    }
                    if invalidate_multiline_layout {
                        self.ide_panel
                            .database
                            .table_modal_layout_cache
                            .get_mut()
                            .invalidate();
                    }
                }
            }
            if let Some(text) = copy_text {
                self.set_clipboard_text(text);
            }
            return true;
        }

        let Some(tab_id) = self.active_database_table_tab_id() else {
            return false;
        };
        let unavailable_focused = self
            .database_table_meta_state(tab_id)
            .is_some_and(|(_, state)| {
                !state.loading
                    && state.metadata.is_none()
                    && state.unavailable_text_focused
            });
        if unavailable_focused {
            let mut copied = None;
            if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
                if key_event.physical_key == PhysicalKey::Code(KeyCode::Escape) {
                    state.clear_unavailable_selection();
                } else {
                    let input = &mut state.unavailable_text;
                    match key_event.physical_key {
                        PhysicalKey::Code(KeyCode::KeyA) if primary => input.select_all(),
                        PhysicalKey::Code(KeyCode::KeyC) if primary => {
                            copied = input.selected_text().map(str::to_owned);
                        }
                        PhysicalKey::Code(KeyCode::ArrowLeft) => {
                            let target = previous_char_boundary(input.text(), input.cursor);
                            input.set_cursor(target, shift);
                        }
                        PhysicalKey::Code(KeyCode::ArrowRight) => {
                            let target = next_char_boundary(input.text(), input.cursor);
                            input.set_cursor(target, shift);
                        }
                        PhysicalKey::Code(KeyCode::Home) => input.set_cursor(0, shift),
                        PhysicalKey::Code(KeyCode::End) => {
                            input.set_cursor(input.text().len(), shift);
                        }
                        _ => {}
                    }
                }
            }
            if let Some(text) = copied {
                self.set_clipboard_text(text);
            }
            return true;
        }
        let focus = self
            .database_table_meta_state(tab_id)
            .and_then(|(_, state)| state.grid.focused_input);
        let filter_focus = matches!(
            focus,
            Some(DatabaseTableInputTarget::Where | DatabaseTableInputTarget::OrderBy)
        );
        if filter_focus && self.autocomplete_active {
            match self.handle_active_autocomplete_key(key_event.physical_key, primary) {
                crate::app::AutocompletePopupKeyResult::Consumed => return true,
                crate::app::AutocompletePopupKeyResult::Continue
                | crate::app::AutocompletePopupKeyResult::NotHandled => {}
            }
        }
        if filter_focus && chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::DatabaseTableFilterComplete, chord)) {
            if let Some(target) = focus {
                self.show_active_database_table_filter_completion(target, true);
            }
            return true;
        }
        let before_filter_text = if filter_focus {
            self.database_table_meta_state(tab_id).and_then(|(_, state)| match focus {
                Some(DatabaseTableInputTarget::Where) => {
                    Some(state.grid.where_input.text().to_string())
                }
                Some(DatabaseTableInputTarget::OrderBy) => {
                    Some(state.grid.order_by_input.text().to_string())
                }
                _ => None,
            })
        } else {
            None
        };
        if focus == Some(DatabaseTableInputTarget::Cell) && chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::DatabaseTableCommitCellLiteral, chord)) {
            self.commit_database_table_cell_editor(tab_id, true);
            return true;
        }
        match key_event.physical_key {
            PhysicalKey::Code(KeyCode::Escape) if chord.is_some_and(|chord| crate::keymap::is_reserved(crate::platform::CURRENT_PLATFORM, chord)) => {
                if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
                    state.grid.cell_editor = None;
                    state.grid.focused_input = None;
                }
            }
            PhysicalKey::Code(KeyCode::Enter | KeyCode::NumpadEnter)
                if chord.is_some_and(|chord| crate::keymap::is_reserved(crate::platform::CURRENT_PLATFORM, chord)) => match focus {
                Some(DatabaseTableInputTarget::Where | DatabaseTableInputTarget::OrderBy) => {
                    self.apply_database_table_filters(tab_id)
                }
                Some(DatabaseTableInputTarget::Cell) => {
                    self.commit_database_table_cell_editor(tab_id, primary)
                }
                None => return false,
            },
            _ if focus.is_none() && (!terminal_owns_chord || default_table_copy_chord)
                && chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::DatabaseTableCopy, chord)) => {
                self.copy_database_table_selection(tab_id)
            }
            _ if focus.is_none() && (!terminal_owns_chord || default_table_undo_chord)
                && chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::DatabaseTableUndo, chord)) => {
                self.undo_database_table_selection(tab_id)
            }
            PhysicalKey::Code(KeyCode::Delete) if focus.is_none() && chord.is_some_and(|chord| chord.mods.is_empty()) => {
                self.delete_database_table_selection(tab_id)
            }
            _ if focus.is_none() && (!terminal_owns_chord || chord.is_some_and(|chord| crate::app::keyboard::input_owner::is_default_chord(crate::keymap::Command::DatabaseTableAddRow, chord, crate::platform::CURRENT_PLATFORM)))
                && chord.is_some_and(|chord| self.keymap.hit(crate::keymap::Command::DatabaseTableAddRow, chord)) => {
                self.add_database_table_row(tab_id)
            }
            physical_key => {
                let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else {
                    return false;
                };
                if matches!(focus, Some(DatabaseTableInputTarget::Where | DatabaseTableInputTarget::OrderBy)) {
                    state.grid.filter_error = None;
                }
                let input = match focus {
                    Some(DatabaseTableInputTarget::Where) => Some(&mut state.grid.where_input),
                    Some(DatabaseTableInputTarget::OrderBy) => {
                        Some(&mut state.grid.order_by_input)
                    }
                    Some(DatabaseTableInputTarget::Cell) => state
                        .grid
                        .cell_editor
                        .as_mut()
                        .map(|editor| &mut editor.input),
                    None => None,
                };
                let Some(input) = input else {
                    return false;
                };
                copy_text = edit_database_table_input(
                    input,
                    physical_key,
                    key_event.logical_text.as_deref(),
                    primary,
                    word,
                    shift,
                    text_input_allowed,
                    paste_text,
                    if matches!(focus, Some(DatabaseTableInputTarget::Cell)) {
                        crate::app::database::MAX_EDITABLE_MULTILINE_BYTES
                    } else {
                        64 * 1024
                    },
                    false,
                );
            }
        }
        if let Some(text) = copy_text {
            self.set_clipboard_text(text);
        }
        if let (Some(target), Some(before)) = (focus, before_filter_text) {
            let changed = self.database_table_meta_state(tab_id).is_some_and(|(_, state)| {
                let after = match target {
                    DatabaseTableInputTarget::Where => state.grid.where_input.text(),
                    DatabaseTableInputTarget::OrderBy => state.grid.order_by_input.text(),
                    DatabaseTableInputTarget::Cell => return false,
                };
                after != before
            });
            if changed {
                self.show_active_database_table_filter_completion(target, false);
            }
        }
        true
    }
}
