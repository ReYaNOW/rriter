impl crate::app::database::DatabaseTableTabState {
    pub(crate) fn start_cell_edit(
        &mut self,
        position: crate::app::database::DatabaseCellPosition,
    ) -> DatabaseTableCellEditStart {
        let Some(metadata) = self.metadata.as_ref() else { return DatabaseTableCellEditStart::Unchanged; };
        let Some(column) = metadata.columns.get(position.column).cloned() else { return DatabaseTableCellEditStart::Unchanged; };
        if !metadata.editable || !column.editable() {
            self.error = Some(if column.primary_key {
                "Редактирование primary key пока отключено".to_string()
            } else if column.type_kind == crate::app::database::DatabaseTypeKind::Bytea {
                "Редактирование bytea отключено".to_string()
            } else {
                metadata.read_only_reason.clone()
                    .unwrap_or_else(|| "Ячейка доступна только для чтения".to_string())
            });
            return DatabaseTableCellEditStart::Unchanged;
        }
        let Some(value) = self.grid.row(position.row)
            .and_then(|row| row.cells.get(position.column))
            .map(|cell| cell.value.copy_text()) else { return DatabaseTableCellEditStart::Unchanged; };
        if column.type_kind == crate::app::database::DatabaseTypeKind::Boolean {
            if let Some(row) = self.grid.row_mut(position.row)
                && let Some(cell) = row.cells.get_mut(position.column)
            {
                let next = match cell.value {
                    crate::app::database::DatabaseCellValue::Boolean(true) => crate::app::database::DatabaseCellValue::Boolean(false),
                    crate::app::database::DatabaseCellValue::Boolean(false) if column.nullable => crate::app::database::DatabaseCellValue::Null,
                    _ => crate::app::database::DatabaseCellValue::Boolean(true),
                };
                cell.set(next);
            }
            return DatabaseTableCellEditStart::Unchanged;
        }
        let kind = match column.type_kind {
            crate::app::database::DatabaseTypeKind::Enum => crate::app::database::DatabaseCellEditorKind::Enum,
            crate::app::database::DatabaseTypeKind::Date
            | crate::app::database::DatabaseTypeKind::Time
            | crate::app::database::DatabaseTypeKind::Timestamp
            | crate::app::database::DatabaseTypeKind::TimestampTz => crate::app::database::DatabaseCellEditorKind::DateTime,
            crate::app::database::DatabaseTypeKind::Json
            | crate::app::database::DatabaseTypeKind::Jsonb => crate::app::database::DatabaseCellEditorKind::Multiline,
            _ if value.len() > 256 || value.contains('\n') => crate::app::database::DatabaseCellEditorKind::Multiline,
            _ => crate::app::database::DatabaseCellEditorKind::Inline,
        };
        if kind == crate::app::database::DatabaseCellEditorKind::Multiline {
            return DatabaseTableCellEditStart::Multiline { position, text: value };
        }
        let (calendar_year, calendar_month) = crate::app::database::database_calendar_year_month(&value).unwrap_or_else(|| {
            let days = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |duration| duration.as_secs() / 86_400);
            let (year, month, _) = crate::app::database::civil_date_from_unix_days(days as i64);
            (year, month)
        });
        let enum_index = if kind == crate::app::database::DatabaseCellEditorKind::Enum {
            column.enum_values.iter().position(|option| option == &value).unwrap_or(0)
        } else { 0 };
        self.grid.cell_editor = Some(crate::app::database::DatabaseCellEditorState {
            position, kind, input: crate::app::database::DatabaseDialogInput::new(value),
            enum_index, calendar_year, calendar_month, error: None,
        });
        self.grid.focused_input = Some(crate::app::DatabaseTableInputTarget::Cell);
        DatabaseTableCellEditStart::Inline
    }

    pub(crate) fn commit_cell_edit(&mut self, literal: bool) {
        let Some(editor) = self.grid.cell_editor.clone() else { return; };
        let Some(column) = self.metadata.as_ref()
            .and_then(|metadata| metadata.columns.get(editor.position.column)).cloned() else { return; };
        match crate::app::database::parse_editor_value(editor.input.text(), &column, literal) {
            Ok(value) => {
                if let Some(row) = self.grid.row_mut(editor.position.row)
                    && let Some(cell) = row.cells.get_mut(editor.position.column)
                { cell.set(value); }
                self.grid.cell_editor = None;
                self.grid.focused_input = None;
            }
            Err(error) => if let Some(editor) = self.grid.cell_editor.as_mut() { editor.error = Some(error); },
        }
    }

    pub(crate) fn page_cell_edit_enum_options(&mut self, next: bool) {
        let Some((kind, column_index)) = self.grid.cell_editor.as_ref()
            .map(|editor| (editor.kind.clone(), editor.position.column)) else { return; };
        if kind != crate::app::database::DatabaseCellEditorKind::Enum { return; }
        let option_count = self.metadata.as_ref()
            .and_then(|metadata| metadata.columns.get(column_index))
            .map_or(0, |column| column.enum_values.len());
        let Some(editor) = self.grid.cell_editor.as_mut() else { return; };
        editor.enum_index = if next {
            editor.enum_index.saturating_add(1).min(option_count.saturating_sub(1))
        } else {
            editor.enum_index.saturating_sub(1)
        };
    }

    pub(crate) fn select_cell_edit_enum_option(&mut self, option: usize) -> bool {
        let Some(editor) = self.grid.cell_editor.as_mut() else { return false; };
        let Some(value) = self.metadata.as_ref()
            .and_then(|metadata| metadata.columns.get(editor.position.column))
            .and_then(|column| column.enum_values.get(option)).cloned() else { return false; };
        editor.input.set_text(value);
        true
    }

    pub(crate) fn shift_cell_edit_calendar_month(&mut self, delta: i32) {
        let Some(editor) = self.grid.cell_editor.as_mut() else { return; };
        let (year, month) = crate::app::database::database_shift_calendar_month(
            editor.calendar_year, editor.calendar_month, delta,
        );
        editor.calendar_year = year;
        editor.calendar_month = month;
    }

    pub(crate) fn select_cell_edit_calendar_day(&mut self, day: u32) {
        let Some(editor) = self.grid.cell_editor.as_mut() else { return; };
        if day == 0 || day > crate::app::database::database_days_in_month(
            editor.calendar_year, editor.calendar_month,
        ) { return; }
        let current = editor.input.text().to_string();
        let suffix = current.get(10..).filter(|_| {
            crate::app::database::database_calendar_year_month(&current).is_some()
        });
        editor.input.set_text(format!(
            "{:04}-{:02}-{day:02}{}", editor.calendar_year, editor.calendar_month,
            suffix.unwrap_or(""),
        ));
    }

    pub(crate) fn set_cell_edit_date(&mut self, year: i32, month: u32, day: u32) {
        let Some(editor) = self.grid.cell_editor.as_mut() else { return; };
        let current = editor.input.text().to_string();
        let suffix = current.get(10..).filter(|_| {
            crate::app::database::database_calendar_year_month(&current).is_some()
        });
        editor.calendar_year = year;
        editor.calendar_month = month;
        editor.input.set_text(format!("{year:04}-{month:02}-{day:02}{}", suffix.unwrap_or("")));
    }

    pub(crate) fn set_cell_edit_time_text(&mut self, text: String, year: i32, month: u32) {
        let Some(editor) = self.grid.cell_editor.as_mut() else { return; };
        editor.calendar_year = year;
        editor.calendar_month = month;
        editor.input.set_text(text);
    }

    pub(crate) fn set_cell_edit_cursor(
        &mut self,
        target: crate::app::DatabaseTableInputTarget,
        target_index: usize,
        selecting: bool,
    ) -> bool {
        let input = match target {
            crate::app::DatabaseTableInputTarget::Where => Some(&mut self.grid.where_input),
            crate::app::DatabaseTableInputTarget::OrderBy => Some(&mut self.grid.order_by_input),
            crate::app::DatabaseTableInputTarget::Cell => self.grid.cell_editor.as_mut()
                .map(|editor| &mut editor.input),
        };
        let Some(input) = input else { return false; };
        input.set_cursor(target_index, selecting);
        self.grid.focused_input = Some(target);
        true
    }
}

impl crate::app::database::DatabaseTableTabState {
    pub(crate) fn change_plan(
        &self,
        meta: &crate::app::database::DatabaseTableTabMeta,
    ) -> Result<crate::app::database::DatabaseChangePlan, String> {
        let metadata = self.metadata.as_ref()
            .ok_or_else(|| "Metadata таблицы ещё не загружены".to_string())?;
        let mut operations = Vec::new();
        for chunk in self.grid.chunks.values() {
            for row in &chunk.rows {
                if row.state == crate::app::database::DatabaseRowState::Deleted {
                    operations.push(crate::app::database::DatabaseChangePlanOperation::Delete(row.clone()));
                } else if row.cells.iter().any(|cell| cell.dirty) {
                    operations.push(crate::app::database::DatabaseChangePlanOperation::Update(row.clone()));
                }
            }
        }
        for row in &self.grid.added_rows {
            if row.state == crate::app::database::DatabaseRowState::Added {
                operations.push(crate::app::database::DatabaseChangePlanOperation::Insert(row.clone()));
            }
        }
        crate::app::database::build_table_change_plan(
            metadata, &meta.database_name, &meta.table_name, operations,
        )
    }
}

pub(crate) enum DatabaseTableCellEditStart {
    Unchanged,
    Inline,
    Multiline { position: crate::app::database::DatabaseCellPosition, text: String },
}

pub(crate) fn previous_cell_edit_char_boundary(text: &str, cursor: usize) -> usize {
    let mut cursor = cursor.min(text.len());
    if cursor == 0 { return 0; }
    cursor -= 1;
    while cursor > 0 && !text.is_char_boundary(cursor) { cursor -= 1; }
    cursor
}

pub(crate) fn next_cell_edit_char_boundary(text: &str, cursor: usize) -> usize {
    let mut cursor = cursor.min(text.len());
    if cursor >= text.len() { return text.len(); }
    cursor += 1;
    while cursor < text.len() && !text.is_char_boundary(cursor) { cursor += 1; }
    cursor
}

pub(crate) fn cell_edit_cursor_line(text: &str, cursor: usize) -> usize {
    text.as_bytes()[..cursor.min(text.len())]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
}

pub(crate) fn cell_edit_line_start_boundary(text: &str, cursor: usize) -> usize {
    crate::app::database::database_multiline_lines(text)
        .nth(cell_edit_cursor_line(text, cursor)).map_or(text.len(), |(start, _)| start)
}

pub(crate) fn cell_edit_line_end_boundary(text: &str, cursor: usize) -> usize {
    crate::app::database::database_multiline_lines(text)
        .nth(cell_edit_cursor_line(text, cursor))
        .map_or(text.len(), |(start, line)| start.saturating_add(line.len()))
}

pub(crate) fn cell_edit_vertical_cursor_target(text: &str, cursor: usize, direction: i32) -> usize {
    let current_line = cell_edit_cursor_line(text, cursor);
    let target_line = if direction < 0 { current_line.saturating_sub(1) }
        else { current_line.saturating_add(1)
            .min(crate::app::database::database_multiline_line_count(text).saturating_sub(1)) };
    if target_line == current_line { return cursor.min(text.len()); }
    let Some((current_start, current_text)) = crate::app::database::database_multiline_lines(text).nth(current_line) else {
        return text.len();
    };
    let current_end = current_start.saturating_add(current_text.len());
    let column = text[current_start..cursor.min(current_end)].chars().count();
    let Some((target_start, target_text)) = crate::app::database::database_multiline_lines(text).nth(target_line) else {
        return text.len();
    };
    target_start.saturating_add(target_text.char_indices().nth(column).map_or(target_text.len(), |(index, _)| index))
}

pub(crate) fn edit_database_table_input(
    input: &mut crate::app::database::DatabaseDialogInput,
    physical_key: winit::keyboard::PhysicalKey,
    logical_text: Option<&str>, primary: bool, word: bool, shift: bool,
    text_input_allowed: bool, paste_text: Option<String>, max_bytes: usize, multiline: bool,
) -> Option<String> {
    use winit::keyboard::{KeyCode, PhysicalKey};
    if crate::app::single_line_input::handle_input_history_shortcut(input, physical_key, primary, shift) { return None; }
    if !multiline {
        return crate::app::single_line_input::handle_single_line_input(
            input, physical_key, logical_text, primary, word, shift, text_input_allowed,
            paste_text.as_deref(), max_bytes,
        );
    }
    match physical_key {
        PhysicalKey::Code(KeyCode::KeyA) if primary => { input.select_all(); None }
        PhysicalKey::Code(KeyCode::KeyC) if primary => input.selected_text().map(str::to_owned),
        PhysicalKey::Code(KeyCode::KeyX) if primary => {
            let selected = input.selected_text().map(str::to_owned);
            if selected.is_some() { input.delete_selection(); }
            selected
        }
        PhysicalKey::Code(KeyCode::KeyV) if primary => { if let Some(text) = paste_text { input.insert(&text, max_bytes); } None }
        PhysicalKey::Code(KeyCode::Backspace) => { if word { input.delete_word_backward(); } else { input.backspace(); } None }
        PhysicalKey::Code(KeyCode::Delete) => { if word { input.delete_word_forward(); } else { input.delete_forward(); } None }
        PhysicalKey::Code(KeyCode::ArrowLeft) => { if word { input.move_word_left(shift); } else { input.move_left(shift); } None }
        PhysicalKey::Code(KeyCode::ArrowRight) => { if word { input.move_word_right(shift); } else { input.move_right(shift); } None }
        PhysicalKey::Code(KeyCode::ArrowUp) => { input.set_cursor(cell_edit_vertical_cursor_target(input.text(), input.cursor, -1), shift); None }
        PhysicalKey::Code(KeyCode::ArrowDown) => { input.set_cursor(cell_edit_vertical_cursor_target(input.text(), input.cursor, 1), shift); None }
        PhysicalKey::Code(KeyCode::Home) => { input.set_cursor(if primary { 0 } else { cell_edit_line_start_boundary(input.text(), input.cursor) }, shift); None }
        PhysicalKey::Code(KeyCode::End) => { input.set_cursor(if primary { input.text().len() } else { cell_edit_line_end_boundary(input.text(), input.cursor) }, shift); None }
        _ if text_input_allowed => { if let Some(text) = logical_text { if !text.is_empty() { input.insert(text, max_bytes); } } None }
        _ => None,
    }
}

pub(crate) fn previous_char_boundary(text: &str, cursor: usize) -> usize {
    previous_cell_edit_char_boundary(text, cursor)
}

pub(crate) fn next_char_boundary(text: &str, cursor: usize) -> usize {
    next_cell_edit_char_boundary(text, cursor)
}

pub(crate) fn database_cursor_line(text: &str, cursor: usize) -> usize {
    cell_edit_cursor_line(text, cursor)
}

pub(crate) fn line_start_boundary(text: &str, cursor: usize) -> usize {
    cell_edit_line_start_boundary(text, cursor)
}

pub(crate) fn line_end_boundary(text: &str, cursor: usize) -> usize {
    cell_edit_line_end_boundary(text, cursor)
}

pub(crate) fn database_vertical_cursor_target(text: &str, cursor: usize, direction: i32) -> usize {
    cell_edit_vertical_cursor_target(text, cursor, direction)
}

pub(crate) fn database_multiline_edit_may_change_text(
    physical_key: winit::keyboard::PhysicalKey,
    logical_text: Option<&str>,
    primary: bool,
    text_input_allowed: bool,
) -> bool {
    use winit::keyboard::{KeyCode, PhysicalKey};
    match physical_key {
        PhysicalKey::Code(KeyCode::KeyX | KeyCode::KeyV | KeyCode::KeyZ | KeyCode::KeyY)
            if primary => true,
        PhysicalKey::Code(KeyCode::Backspace | KeyCode::Delete) => true,
        _ => text_input_allowed && logical_text.is_some_and(|text| !text.is_empty()),
    }
}

pub(crate) fn database_table_transaction_finish_allowed(committing: bool) -> bool {
    !committing
}

#[cfg(test)]
mod database_table_cell_edit_state_tests {
    use super::*;

    #[test]
    fn read_only_cursor_helpers_preserve_utf8_boundaries_and_lines() {
        let text = "Жx\nSELECT";
        assert_eq!(next_char_boundary(text, 0), 'Ж'.len_utf8());
        assert_eq!(previous_char_boundary(text, 'Ж'.len_utf8()), 0);
        assert_eq!(line_start_boundary(text, text.len()), 4);
        assert_eq!(line_end_boundary(text, 0), 3);
    }

    #[test]
    fn multiline_cursor_navigation_preserves_character_column_and_trailing_line() {
        let text = "Жx\na\n";
        let first_line_after_x = "Жx".len();
        assert_eq!(database_vertical_cursor_target(text, first_line_after_x, 1), 5);
        assert_eq!(database_vertical_cursor_target(text, 5, 1), text.len());
        assert_eq!(database_vertical_cursor_target(text, text.len(), -1), 4);
        assert_eq!(line_start_boundary(text, text.len()), text.len());
        assert_eq!(line_end_boundary(text, text.len()), text.len());
    }

    #[test]
    fn multiline_layout_invalidation_only_tracks_edit_capable_keys() {
        use winit::keyboard::{KeyCode, PhysicalKey};
        for key in [KeyCode::ArrowLeft, KeyCode::ArrowRight, KeyCode::ArrowUp, KeyCode::ArrowDown, KeyCode::Home, KeyCode::End] {
            assert!(!database_multiline_edit_may_change_text(PhysicalKey::Code(key), None, false, false));
        }
        assert!(!database_multiline_edit_may_change_text(PhysicalKey::Code(KeyCode::KeyC), None, true, false));
        for key in [KeyCode::Backspace, KeyCode::Delete] {
            assert!(database_multiline_edit_may_change_text(PhysicalKey::Code(key), None, false, false));
        }
        for key in [KeyCode::KeyX, KeyCode::KeyV, KeyCode::KeyZ, KeyCode::KeyY] {
            assert!(database_multiline_edit_may_change_text(PhysicalKey::Code(key), None, true, false));
        }
        assert!(database_multiline_edit_may_change_text(PhysicalKey::Code(KeyCode::KeyA), Some("Ж"), false, true));
    }

    #[test]
    fn multiline_modal_reuses_shared_undo_and_redo_shortcuts() {
        use winit::keyboard::{KeyCode, PhysicalKey};
        let mut input = crate::app::database::DatabaseDialogInput::new("alpha");
        input.move_end(false);
        edit_database_table_input(&mut input, PhysicalKey::Code(KeyCode::KeyB), Some("β"), false, false, false, true, None, 1024, true);
        assert_eq!(input.text(), "alphaβ");
        edit_database_table_input(&mut input, PhysicalKey::Code(KeyCode::KeyZ), None, true, false, false, false, None, 1024, true);
        assert_eq!(input.text(), "alpha");
        edit_database_table_input(&mut input, PhysicalKey::Code(KeyCode::KeyZ), None, true, false, true, false, None, 1024, true);
        assert_eq!(input.text(), "alphaβ");
    }

    #[test]
    fn bug_59_table_transaction_finish_rejects_duplicate_commit_or_rollback() {
        assert!(database_table_transaction_finish_allowed(false));
        assert!(!database_table_transaction_finish_allowed(true));
    }
}
