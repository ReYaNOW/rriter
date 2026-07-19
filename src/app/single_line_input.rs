use crate::editor::Editor;
use winit::keyboard::{KeyCode, PhysicalKey};

/// Общая модель однострочного текстового поля. File tree и Database Tools
/// используют один и тот же keyboard/selection/clipboard path; различается
/// только контейнер текста (обычный Editor или zeroizing secret input).
pub(crate) trait SingleLineInputModel {
    fn len_bytes(&self) -> usize;
    fn selected_len_bytes(&self) -> usize;
    fn select_all(&mut self);
    fn selected_text_owned(&self) -> Option<String>;
    fn delete_selection(&mut self);
    fn insert_text(&mut self, text: &str);
    fn backspace(&mut self);
    fn delete_forward(&mut self);
    fn delete_word_backward(&mut self);
    fn delete_word_forward(&mut self);
    fn move_left(&mut self, selecting: bool);
    fn move_right(&mut self, selecting: bool);
    fn move_word_left(&mut self, selecting: bool);
    fn move_word_right(&mut self, selecting: bool);
    fn move_home(&mut self, selecting: bool);
    fn move_end(&mut self, selecting: bool);
    fn undo(&mut self) {}
    fn redo(&mut self) {}
}

impl SingleLineInputModel for Editor {
    fn len_bytes(&self) -> usize {
        self.len()
    }

    fn selected_len_bytes(&self) -> usize {
        self.selection_anchor
            .map(|anchor| anchor.abs_diff(self.cursor))
            .unwrap_or(0)
    }

    fn select_all(&mut self) {
        Editor::select_all(self);
    }

    fn selected_text_owned(&self) -> Option<String> {
        self.get_selection()
    }

    fn delete_selection(&mut self) {
        let _ = Editor::delete_selection(self);
    }

    fn insert_text(&mut self, text: &str) {
        let _ = self.insert_str(text);
    }

    fn backspace(&mut self) {
        let _ = Editor::backspace(self);
    }

    fn delete_forward(&mut self) {
        let _ = Editor::delete_forward(self);
    }

    fn delete_word_backward(&mut self) {
        let _ = Editor::delete_word_backward(self);
    }

    fn delete_word_forward(&mut self) {
        let _ = Editor::delete_word_forward(self);
    }

    fn move_left(&mut self, selecting: bool) {
        Editor::move_left(self, selecting);
    }

    fn move_right(&mut self, selecting: bool) {
        Editor::move_right(self, selecting);
    }

    fn move_word_left(&mut self, selecting: bool) {
        Editor::move_word_left(self, selecting);
    }

    fn move_word_right(&mut self, selecting: bool) {
        Editor::move_word_right(self, selecting);
    }

    fn move_home(&mut self, selecting: bool) {
        Editor::move_home(self, selecting);
    }

    fn move_end(&mut self, selecting: bool) {
        Editor::move_end(self, selecting);
    }

    fn undo(&mut self) {
        let _ = Editor::undo(self);
    }

    fn redo(&mut self) {
        let _ = Editor::redo(self);
    }
}

pub(crate) fn sanitize_single_line_text(
    text: &str,
    current_len: usize,
    selected_len: usize,
    max_bytes: usize,
) -> String {
    let room = max_bytes.saturating_sub(current_len.saturating_sub(selected_len));
    let mut clean = String::with_capacity(text.len().min(room));
    for ch in text.chars() {
        if ch == '\n' || ch == '\r' {
            continue;
        }
        if clean.len() + ch.len_utf8() > room {
            break;
        }
        clean.push(ch);
    }
    clean
}

pub(crate) fn insert_single_line_text<T: SingleLineInputModel>(
    input: &mut T,
    text: &str,
    max_bytes: usize,
) {
    let clean = sanitize_single_line_text(
        text,
        input.len_bytes(),
        input.selected_len_bytes(),
        max_bytes,
    );
    if !clean.is_empty() {
        input.insert_text(&clean);
    }
}

pub(crate) fn handle_input_history_shortcut<T: SingleLineInputModel>(
    input: &mut T,
    physical_key: PhysicalKey,
    primary: bool,
    shift: bool,
) -> bool {
    match physical_key {
        PhysicalKey::Code(KeyCode::KeyZ) if primary && shift => {
            input.redo();
            true
        }
        PhysicalKey::Code(KeyCode::KeyZ) if primary => {
            input.undo();
            true
        }
        PhysicalKey::Code(KeyCode::KeyY) if primary => {
            input.redo();
            true
        }
        _ => false,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_single_line_input<T: SingleLineInputModel>(
    input: &mut T,
    physical_key: PhysicalKey,
    logical_text: Option<&str>,
    primary: bool,
    word: bool,
    shift: bool,
    text_input_allowed: bool,
    paste_text: Option<&str>,
    max_bytes: usize,
) -> Option<String> {
    if handle_input_history_shortcut(input, physical_key, primary, shift) {
        return None;
    }
    match physical_key {
        PhysicalKey::Code(KeyCode::KeyA) if primary => {
            input.select_all();
            None
        }
        PhysicalKey::Code(KeyCode::KeyC) if primary => input.selected_text_owned(),
        PhysicalKey::Code(KeyCode::KeyX) if primary => {
            let copied = input.selected_text_owned();
            if copied.is_some() {
                input.delete_selection();
            }
            copied
        }
        PhysicalKey::Code(KeyCode::KeyV) if primary => {
            if let Some(text) = paste_text {
                insert_single_line_text(input, text, max_bytes);
            }
            None
        }
        PhysicalKey::Code(KeyCode::Backspace) => {
            if word {
                input.delete_word_backward();
            } else {
                input.backspace();
            }
            None
        }
        PhysicalKey::Code(KeyCode::Delete) => {
            if word {
                input.delete_word_forward();
            } else {
                input.delete_forward();
            }
            None
        }
        PhysicalKey::Code(KeyCode::ArrowLeft) => {
            if word {
                input.move_word_left(shift);
            } else {
                input.move_left(shift);
            }
            None
        }
        PhysicalKey::Code(KeyCode::ArrowRight) => {
            if word {
                input.move_word_right(shift);
            } else {
                input.move_right(shift);
            }
            None
        }
        PhysicalKey::Code(KeyCode::Home) => {
            input.move_home(shift);
            None
        }
        PhysicalKey::Code(KeyCode::End) => {
            input.move_end(shift);
            None
        }
        _ if text_input_allowed => {
            if let Some(text) = logical_text {
                insert_single_line_text(input, text, max_bytes);
            }
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::database::DatabaseDialogInput;

    #[test]
    fn sanitizer_removes_line_breaks_and_respects_utf8_limit() {
        assert_eq!(sanitize_single_line_text("a\nб\rв", 0, 0, 4), "aб");
        assert_eq!(sanitize_single_line_text("xyz", 5, 2, 5), "xy");
    }

    #[test]
    fn shared_keyboard_path_edits_zeroizing_database_input() {
        let mut input = DatabaseDialogInput::new("alpha beta");
        input.move_end(false);
        handle_single_line_input(
            &mut input,
            PhysicalKey::Code(KeyCode::Backspace),
            None,
            false,
            true,
            false,
            true,
            None,
            64,
        );
        assert_eq!(input.text(), "alpha ");
        handle_single_line_input(
            &mut input,
            PhysicalKey::Code(KeyCode::KeyA),
            None,
            true,
            false,
            false,
            true,
            None,
            64,
        );
        assert_eq!(input.selected_text(), Some("alpha "));
    }

    #[test]
    fn primary_f_does_not_select_all_input_text() {
        let mut input = DatabaseDialogInput::new("alpha beta");
        input.set_cursor(5, false);
        handle_single_line_input(
            &mut input,
            PhysicalKey::Code(KeyCode::KeyF),
            Some("f"),
            true,
            false,
            false,
            false,
            None,
            64,
        );
        assert_eq!(input.cursor, 5);
        assert_eq!(input.selection_anchor, None);
        assert_eq!(input.text(), "alpha beta");
    }

    #[test]
    fn shared_keyboard_path_matches_editor_selection_and_clipboard() {
        let mut editor = Editor::new(32);
        editor.set_text_clean("one two");
        handle_single_line_input(
            &mut editor,
            PhysicalKey::Code(KeyCode::KeyA),
            None,
            true,
            false,
            false,
            true,
            None,
            32,
        );
        let copied = handle_single_line_input(
            &mut editor,
            PhysicalKey::Code(KeyCode::KeyC),
            None,
            true,
            false,
            false,
            true,
            None,
            32,
        );
        assert_eq!(copied.as_deref(), Some("one two"));
    }
}
