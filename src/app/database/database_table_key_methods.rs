use crate::app::database::database_table_cell_edit_state::{
    database_multiline_edit_may_change_text, database_vertical_cursor_target,
    edit_database_table_input, line_end_boundary, line_start_boundary, next_char_boundary,
    previous_char_boundary,
};
use crate::app::database::database_table_modal_state::{
    database_sql_preview_copy_text, database_table_modal_input_mut, move_read_only_cursor,
};

struct DatabaseTableKeyContext<'a> {
    key_event: &'a crate::app::keyboard::KeyInput,
    chord: Option<crate::keymap::Chord>,
    terminal_owns_chord: bool,
    primary: bool,
    word: bool,
    shift: bool,
    text_input_allowed: bool,
    paste_text: Option<String>,
    default_table_copy_chord: bool,
    default_table_undo_chord: bool,
}

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
        let primary_mod = if crate::platform::CURRENT_PLATFORM
            == crate::platform::PlatformKind::Macos
        {
            crate::keymap::Mods::SUPER
        } else {
            crate::keymap::Mods::CTRL
        };
        let (default_table_copy_chord, default_table_undo_chord) = chord.map_or(
            (false, false),
            |chord| {
                (
                    chord.mods == primary_mod && chord.key == KeyCode::KeyC,
                    chord.mods == primary_mod && chord.key == KeyCode::KeyZ,
                )
            },
        );
        let paste_text = if primary
            && key_event.physical_key == PhysicalKey::Code(KeyCode::KeyV)
        {
            self.get_clipboard_text()
        } else {
            None
        };
        let context = DatabaseTableKeyContext {
            key_event,
            chord,
            terminal_owns_chord,
            primary,
            word: crate::platform::word_navigation_modifier(self.modifiers),
            shift: self.modifiers.shift_key(),
            text_input_allowed: crate::platform::text_input_modifiers_allowed(self.modifiers),
            paste_text,
            default_table_copy_chord,
            default_table_undo_chord,
        };

        if self.ide_panel.database.table_modal.is_some() {
            return self.handle_database_table_modal_key(&context);
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
            return self.handle_database_table_unavailable_key(tab_id, &context);
        }

        self.handle_database_table_grid_key(tab_id, &context)
    }

    fn handle_database_table_modal_key(&mut self, context: &DatabaseTableKeyContext<'_>) -> bool {
        if matches!(
            self.ide_panel.database.table_modal,
            Some(DatabaseTableModal::SqlPreview { .. })
        ) {
            return self.handle_database_sql_preview_key(context);
        }

        let mut copy_text = None;
        match context.key_event.physical_key {
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape) => {
                self.activate_database_table_modal_action(1)
            }
            winit::keyboard::PhysicalKey::Code(
                winit::keyboard::KeyCode::Enter | winit::keyboard::KeyCode::NumpadEnter,
            ) => {
                let multiline = matches!(
                    self.ide_panel.database.table_modal,
                    Some(DatabaseTableModal::MultilineEditor { .. })
                );
                if multiline && !context.primary {
                    if let Some(DatabaseTableModal::MultilineEditor { input, error, .. }) =
                        self.ide_panel.database.table_modal.as_mut()
                    {
                        input.insert(
                            "\n",
                            crate::app::database::MAX_EDITABLE_MULTILINE_BYTES,
                        );
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
                        context.key_event.logical_text.as_deref(),
                        context.primary,
                        context.text_input_allowed,
                    );
                if let Some(input) =
                    database_table_modal_input_mut(&mut self.ide_panel.database.table_modal)
                {
                    copy_text = edit_database_table_input(
                        input,
                        physical_key,
                        context.key_event.logical_text.as_deref(),
                        context.primary,
                        context.word,
                        context.shift,
                        context.text_input_allowed,
                        context.paste_text.clone(),
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
        true
    }

    fn handle_database_sql_preview_key(
        &mut self,
        context: &DatabaseTableKeyContext<'_>,
    ) -> bool {
        use winit::keyboard::{KeyCode, PhysicalKey};

        if context.primary && context.key_event.physical_key == PhysicalKey::Code(KeyCode::KeyC) {
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
        if context.key_event.physical_key == PhysicalKey::Code(KeyCode::Escape) {
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
            let target = match context.key_event.physical_key {
                PhysicalKey::Code(KeyCode::KeyA) if context.primary => {
                    *selection_anchor = Some(0);
                    *cursor = text.len();
                    return true;
                }
                PhysicalKey::Code(KeyCode::ArrowLeft) => previous_char_boundary(text, *cursor),
                PhysicalKey::Code(KeyCode::ArrowRight) => next_char_boundary(text, *cursor),
                PhysicalKey::Code(KeyCode::ArrowUp) => {
                    database_vertical_cursor_target(text, *cursor, -1)
                }
                PhysicalKey::Code(KeyCode::ArrowDown) => {
                    database_vertical_cursor_target(text, *cursor, 1)
                }
                PhysicalKey::Code(KeyCode::Home) => {
                    if context.primary {
                        0
                    } else {
                        line_start_boundary(text, *cursor)
                    }
                }
                PhysicalKey::Code(KeyCode::End) => {
                    if context.primary {
                        text.len()
                    } else {
                        line_end_boundary(text, *cursor)
                    }
                }
                _ => return true,
            };
            move_read_only_cursor(cursor, selection_anchor, target, context.shift);
        }
        true
    }

    fn handle_database_table_unavailable_key(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
        context: &DatabaseTableKeyContext<'_>,
    ) -> bool {
        use winit::keyboard::{KeyCode, PhysicalKey};

        let mut copied = None;
        if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
            if context.key_event.physical_key == PhysicalKey::Code(KeyCode::Escape) {
                state.clear_unavailable_selection();
            } else {
                let input = &mut state.unavailable_text;
                match context.key_event.physical_key {
                    PhysicalKey::Code(KeyCode::KeyA) if context.primary => input.select_all(),
                    PhysicalKey::Code(KeyCode::KeyC) if context.primary => {
                        copied = input.selected_text().map(str::to_owned);
                    }
                    PhysicalKey::Code(KeyCode::ArrowLeft) => {
                        let target = previous_char_boundary(input.text(), input.cursor);
                        input.set_cursor(target, context.shift);
                    }
                    PhysicalKey::Code(KeyCode::ArrowRight) => {
                        let target = next_char_boundary(input.text(), input.cursor);
                        input.set_cursor(target, context.shift);
                    }
                    PhysicalKey::Code(KeyCode::Home) => input.set_cursor(0, context.shift),
                    PhysicalKey::Code(KeyCode::End) => {
                        input.set_cursor(input.text().len(), context.shift);
                    }
                    _ => {}
                }
            }
        }
        if let Some(text) = copied {
            self.set_clipboard_text(text);
        }
        true
    }

    fn handle_database_table_grid_key(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
        context: &DatabaseTableKeyContext<'_>,
    ) -> bool {
        let focus = self
            .database_table_meta_state(tab_id)
            .and_then(|(_, state)| state.grid.focused_input);
        let filter_focus = matches!(
            focus,
            Some(DatabaseTableInputTarget::Where | DatabaseTableInputTarget::OrderBy)
        );
        if filter_focus && self.autocomplete_active {
            match self.handle_active_autocomplete_key(
                context.key_event.physical_key,
                context.primary,
            ) {
                crate::app::AutocompletePopupKeyResult::Consumed => return true,
                crate::app::AutocompletePopupKeyResult::Continue
                | crate::app::AutocompletePopupKeyResult::NotHandled => {}
            }
        }
        if self.handle_database_table_filter_completion_key(focus, filter_focus, context) {
            return true;
        }

        let before_filter_text = self.database_table_filter_text(tab_id, focus, filter_focus);
        if focus == Some(DatabaseTableInputTarget::Cell)
            && context.chord.is_some_and(|chord| {
                self.keymap
                    .hit(crate::keymap::Command::DatabaseTableCommitCellLiteral, chord)
            })
        {
            self.commit_database_table_cell_editor(tab_id, true);
            return true;
        }

        if !self.dispatch_database_table_grid_key(tab_id, focus, context) {
            return false;
        }
        self.refresh_database_table_filter_completion(tab_id, focus, before_filter_text);
        true
    }

    fn dispatch_database_table_grid_key(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
        focus: Option<DatabaseTableInputTarget>,
        context: &DatabaseTableKeyContext<'_>,
    ) -> bool {
        let mut copy_text = None;
        match context.key_event.physical_key {
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape)
                if context.chord.is_some_and(|chord| {
                    crate::keymap::is_reserved(crate::platform::CURRENT_PLATFORM, chord)
                }) =>
            {
                if let Some((_, state)) = self.database_table_meta_state_mut(tab_id) {
                    state.grid.cell_editor = None;
                    state.grid.focused_input = None;
                }
            }
            winit::keyboard::PhysicalKey::Code(
                winit::keyboard::KeyCode::Enter | winit::keyboard::KeyCode::NumpadEnter,
            ) if context.chord.is_some_and(|chord| {
                crate::keymap::is_reserved(crate::platform::CURRENT_PLATFORM, chord)
            }) => match focus {
                Some(DatabaseTableInputTarget::Where | DatabaseTableInputTarget::OrderBy) => {
                    self.apply_database_table_filters(tab_id)
                }
                Some(DatabaseTableInputTarget::Cell) => {
                    self.commit_database_table_cell_editor(tab_id, context.primary)
                }
                None => return false,
            },
            _ if focus.is_none()
                && (!context.terminal_owns_chord || context.default_table_copy_chord)
                && context.chord.is_some_and(|chord| {
                    self.keymap.hit(crate::keymap::Command::DatabaseTableCopy, chord)
                }) =>
            {
                self.copy_database_table_selection(tab_id)
            }
            _ if focus.is_none()
                && (!context.terminal_owns_chord || context.default_table_undo_chord)
                && context.chord.is_some_and(|chord| {
                    self.keymap.hit(crate::keymap::Command::DatabaseTableUndo, chord)
                }) =>
            {
                self.undo_database_table_selection(tab_id)
            }
            winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Delete)
                if focus.is_none()
                    && context.chord.is_some_and(|chord| chord.mods.is_empty()) =>
            {
                self.delete_database_table_selection(tab_id)
            }
            _ if focus.is_none()
                && (!context.terminal_owns_chord
                    || context.chord.is_some_and(|chord| {
                        crate::app::keyboard::input_owner::is_default_chord(
                            crate::keymap::Command::DatabaseTableAddRow,
                            chord,
                            crate::platform::CURRENT_PLATFORM,
                        )
                    }))
                && context.chord.is_some_and(|chord| {
                    self.keymap.hit(crate::keymap::Command::DatabaseTableAddRow, chord)
                }) =>
            {
                self.add_database_table_row(tab_id)
            }
            physical_key => {
                if !self.edit_database_table_grid_input(
                    tab_id,
                    focus,
                    physical_key,
                    context,
                    &mut copy_text,
                ) {
                    return false;
                }
            }
        }
        if let Some(text) = copy_text {
            self.set_clipboard_text(text);
        }
        true
    }

    fn handle_database_table_filter_completion_key(
        &mut self,
        focus: Option<DatabaseTableInputTarget>,
        filter_focus: bool,
        context: &DatabaseTableKeyContext<'_>,
    ) -> bool {
        if filter_focus
            && context.chord.is_some_and(|chord| {
                self.keymap
                    .hit(crate::keymap::Command::DatabaseTableFilterComplete, chord)
            })
        {
            if let Some(target) = focus {
                self.show_active_database_table_filter_completion(target, true);
            }
            return true;
        }
        false
    }

    fn database_table_filter_text(
        &self,
        tab_id: crate::app::database::DatabaseTabId,
        focus: Option<DatabaseTableInputTarget>,
        filter_focus: bool,
    ) -> Option<String> {
        if !filter_focus {
            return None;
        }
        self.database_table_meta_state(tab_id)
            .and_then(|(_, state)| match focus {
                Some(DatabaseTableInputTarget::Where) => {
                    Some(state.grid.where_input.text().to_string())
                }
                Some(DatabaseTableInputTarget::OrderBy) => {
                    Some(state.grid.order_by_input.text().to_string())
                }
                _ => None,
            })
    }

    fn edit_database_table_grid_input(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
        focus: Option<DatabaseTableInputTarget>,
        physical_key: winit::keyboard::PhysicalKey,
        context: &DatabaseTableKeyContext<'_>,
        copy_text: &mut Option<String>,
    ) -> bool {
        let Some((_, state)) = self.database_table_meta_state_mut(tab_id) else {
            return false;
        };
        if matches!(focus, Some(DatabaseTableInputTarget::Where | DatabaseTableInputTarget::OrderBy))
        {
            state.grid.filter_error = None;
        }
        let input = match focus {
            Some(DatabaseTableInputTarget::Where) => Some(&mut state.grid.where_input),
            Some(DatabaseTableInputTarget::OrderBy) => Some(&mut state.grid.order_by_input),
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
        *copy_text = edit_database_table_input(
            input,
            physical_key,
            context.key_event.logical_text.as_deref(),
            context.primary,
            context.word,
            context.shift,
            context.text_input_allowed,
            context.paste_text.clone(),
            if matches!(focus, Some(DatabaseTableInputTarget::Cell)) {
                crate::app::database::MAX_EDITABLE_MULTILINE_BYTES
            } else {
                64 * 1024
            },
            false,
        );
        true
    }

    fn refresh_database_table_filter_completion(
        &mut self,
        tab_id: crate::app::database::DatabaseTabId,
        focus: Option<DatabaseTableInputTarget>,
        before_filter_text: Option<String>,
    ) {
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
    }
}
