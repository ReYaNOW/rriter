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

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SingleLineTextGeometry {
    pub text_start_x: f32,
    pub content_w: f32,
}

pub(crate) fn single_line_text_geometry(
    input_x: f32,
    input_w: f32,
    horizontal_padding: f32,
    right_inset: f32,
) -> SingleLineTextGeometry {
    let input_x = input_x.round();
    let input_w = input_w.round().max(1.0);
    let pad_x = horizontal_padding.round().clamp(0.0, input_w * 0.5);
    let right_inset = right_inset.round().clamp(0.0, input_w - 1.0);
    let content_w = (input_w - pad_x * 2.0 - right_inset).max(1.0);
    SingleLineTextGeometry {
        text_start_x: input_x + pad_x,
        content_w,
    }
}

pub(crate) fn single_line_rendered_x(
    geometry: SingleLineTextGeometry,
    logical_x: f32,
    scroll_x: f32,
) -> f32 {
    geometry.text_start_x + logical_x - scroll_x
}

pub(crate) fn single_line_hit_offset(
    geometry: SingleLineTextGeometry,
    rendered_x: f32,
    scroll_x: f32,
) -> f32 {
    (rendered_x - geometry.text_start_x + scroll_x).max(0.0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SingleLineCursorGeometry {
    pub scroll_x: f32,
    pub cursor_x: f32,
    pub total_width: f32,
    pub max_scroll: f32,
}

pub(crate) fn single_line_cursor_geometry<F>(
    text: &str,
    cursor: usize,
    visible_width: f32,
    current_scroll: f32,
    edge_pad: f32,
    trailing_pad: f32,
    mut char_advance: F,
) -> SingleLineCursorGeometry
where
    F: FnMut(char) -> f32,
{
    let cursor = cursor.min(text.len());
    let mut cursor_x = 0.0;
    let mut total_width = 0.0;
    for (byte_idx, ch) in text.char_indices() {
        let advance = char_advance(ch).max(0.0);
        if byte_idx < cursor {
            cursor_x += advance;
        }
        total_width += advance;
    }

    let visible_width = visible_width.max(1.0);
    let edge_pad = edge_pad.max(0.0).min(visible_width);
    let trailing_pad = trailing_pad.max(edge_pad);
    let max_scroll = (total_width - visible_width + trailing_pad).max(0.0);
    let mut scroll_x = if current_scroll.is_finite() {
        current_scroll.clamp(0.0, max_scroll)
    } else {
        0.0
    };
    let right_limit = (visible_width - edge_pad).max(0.0);
    if cursor_x - scroll_x > right_limit {
        scroll_x = cursor_x - right_limit;
    } else if cursor_x < scroll_x {
        scroll_x = cursor_x;
    }

    SingleLineCursorGeometry {
        scroll_x: scroll_x.clamp(0.0, max_scroll),
        cursor_x,
        total_width,
        max_scroll,
    }
}

pub(crate) fn single_line_hit_index<F>(
    text: &str,
    x_offset: f32,
    mut char_advance: F,
) -> usize
where
    F: FnMut(char) -> f32,
{
    let mut current_x = 0.0;
    for (byte_idx, ch) in text.char_indices() {
        let advance = char_advance(ch).max(0.0);
        if x_offset <= current_x + advance * 0.5 {
            return byte_idx;
        }
        current_x += advance;
    }
    text.len()
}

pub(crate) fn single_line_caret_width(scale_factor: f32) -> f32 {
    (1.5 * scale_factor).round().max(1.0)
}

pub(crate) fn single_line_cursor_edge_pad(scale_factor: f32) -> f32 {
    single_line_caret_width(scale_factor) + (1.0 * scale_factor).round().max(1.0)
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

    #[test]
    fn cursor_scroll_keeps_caret_inside_exclusive_right_clip() {
        let scale = 1.0;
        let caret_w = single_line_caret_width(scale);
        let edge_pad = single_line_cursor_edge_pad(scale);
        let geometry = single_line_cursor_geometry(
            "abcdef",
            6,
            30.0,
            0.0,
            edge_pad,
            edge_pad,
            |_| 10.0,
        );

        assert_eq!(geometry.cursor_x, 60.0);
        assert_eq!(geometry.total_width, 60.0);
        assert_eq!(geometry.scroll_x, 33.0);
        assert_eq!(geometry.max_scroll, 33.0);
        let cursor_draw_x = geometry.cursor_x - geometry.scroll_x;
        assert!(cursor_draw_x + caret_w < 30.0);

        let old_boundary_case = single_line_cursor_geometry(
            "abc",
            3,
            30.0,
            0.0,
            edge_pad,
            edge_pad,
            |_| 10.0,
        );
        assert_eq!(old_boundary_case.scroll_x, edge_pad);
        assert!(old_boundary_case.cursor_x - old_boundary_case.scroll_x + caret_w < 30.0);

        let narrow = single_line_cursor_geometry(
            "a",
            1,
            caret_w,
            0.0,
            edge_pad,
            edge_pad,
            |_| caret_w,
        );
        assert!(narrow.cursor_x - narrow.scroll_x + caret_w <= caret_w);
    }

    #[test]
    fn short_text_that_fits_with_caret_pad_keeps_zero_scroll() {
        let scale = 1.75;
        let edge_pad = single_line_cursor_edge_pad(scale);
        let geometry = single_line_cursor_geometry(
            "short",
            "short".len(),
            120.0,
            0.0,
            edge_pad,
            edge_pad,
            |_| 9.0,
        );

        assert_eq!(geometry.total_width, 45.0);
        assert_eq!(geometry.max_scroll, 0.0);
        assert_eq!(geometry.scroll_x, 0.0);
    }

    #[test]
    fn cursor_scroll_home_left_and_fractional_text_geometry_stay_consistent() {
        for scale in [1.25, 1.5, 1.75, 1.33] {
            let horizontal = single_line_text_geometry(
                10.4,
                101.6,
                8.0 * scale,
                28.0 * scale,
            );
            let expected_pad = (8.0 * scale).round().clamp(0.0, 51.0);
            let expected_right_inset = (28.0 * scale).round().clamp(0.0, 101.0);
            assert_eq!(horizontal.text_start_x, 10.0 + expected_pad);
            assert_eq!(
                horizontal.content_w,
                (102.0 - expected_pad * 2.0 - expected_right_inset).max(1.0)
            );
            assert!(horizontal.content_w >= 1.0);

            let edge_pad = single_line_cursor_edge_pad(scale);
            let caret_w = single_line_caret_width(scale);
            let at_end = single_line_cursor_geometry(
                "abcdefghijk",
                11,
                horizontal.content_w,
                0.0,
                edge_pad,
                edge_pad,
                |_| 10.0,
            );
            let cursor_draw_x = at_end.cursor_x - at_end.scroll_x;
            assert!(cursor_draw_x + caret_w < horizontal.content_w);

            // This is the same X used by Database Tools autocomplete. It must
            // stay inside the actual clipped text content, including the
            // connection secret-field eye-button reservation.
            let anchor_x = horizontal.text_start_x + cursor_draw_x;
            assert!(anchor_x + caret_w < horizontal.text_start_x + horizontal.content_w);

            let one_left = single_line_cursor_geometry(
                "abcdefghijk",
                3,
                horizontal.content_w,
                at_end.scroll_x,
                edge_pad,
                edge_pad,
                |_| 10.0,
            );
            let home = single_line_cursor_geometry(
                "abcdefghijk",
                0,
                horizontal.content_w,
                one_left.scroll_x,
                edge_pad,
                edge_pad,
                |_| 10.0,
            );
            assert!(one_left.scroll_x < at_end.scroll_x);
            assert_eq!(home.scroll_x, 0.0);
        }

        let horizontal = single_line_text_geometry(10.4, 101.6, 10.0, 35.0);
        assert_eq!(horizontal.text_start_x, 20.0);
        assert_eq!(horizontal.content_w, 47.0);
    }

    #[test]
    fn hit_index_round_trips_mixed_utf8_boundaries_with_horizontal_scroll() {
        let text = "abвг界deёжXYZ";
        for scale in [1.25_f32, 1.75_f32] {
            let advance = |ch| {
                let base = match ch {
                    'a' | 'd' | 'X' => 6.2,
                    'b' | 'e' | 'Y' => 7.4,
                    'в' | 'ё' => 8.1,
                    'г' | 'ж' => 9.3,
                    '界' => 12.6,
                    'Z' => 8.7,
                    _ => 7.0,
                };
                (base * scale).round().max(1.0)
            };
            let input = single_line_text_geometry(
                17.4,
                (74.0 * scale).round(),
                (10.0 * scale).round(),
                0.0,
            );
            let edge_pad = single_line_cursor_edge_pad(scale);
            let cursor = single_line_cursor_geometry(
                text,
                text.len(),
                input.content_w,
                0.0,
                edge_pad,
                edge_pad,
                advance,
            );
            assert!(cursor.scroll_x > 0.0);

            let mut logical_boundaries = Vec::new();
            let mut logical_x = 0.0;
            logical_boundaries.push((0usize, logical_x));
            for (byte_idx, ch) in text.char_indices() {
                logical_x += advance(ch);
                logical_boundaries.push((byte_idx + ch.len_utf8(), logical_x));
            }

            let clip_left = input.text_start_x;
            let clip_right = input.text_start_x + input.content_w;
            let visible = logical_boundaries
                .iter()
                .copied()
                .filter(|(_, x)| {
                    let rendered = single_line_rendered_x(input, *x, cursor.scroll_x);
                    rendered >= clip_left && rendered <= clip_right
                })
                .collect::<Vec<_>>();
            assert!(visible.len() >= 3);

            let cases = [visible[0], visible[visible.len() / 2], visible[visible.len() - 1]];
            for (byte_idx, logical_x) in cases {
                let rendered_x = single_line_rendered_x(input, logical_x, cursor.scroll_x);
                let hit_offset = single_line_hit_offset(input, rendered_x, cursor.scroll_x);
                let hit = single_line_hit_index(text, hit_offset, advance);
                assert_eq!(hit, byte_idx, "scale={scale} rendered_x={rendered_x}");
                assert!(text.is_char_boundary(hit));
            }

            let left_distance = single_line_rendered_x(input, visible[0].1, cursor.scroll_x)
                - clip_left;
            let right_distance = clip_right
                - single_line_rendered_x(input, visible[visible.len() - 1].1, cursor.scroll_x);
            let max_advance = text.chars().map(advance).fold(0.0_f32, f32::max);
            assert!(left_distance <= max_advance, "left boundary not exercised at scale={scale}");
            assert!(right_distance <= max_advance, "right boundary not exercised at scale={scale}");
        }
    }

    #[test]
    fn keyboard_word_and_shift_navigation_keep_utf8_boundaries() {
        let mut input = DatabaseDialogInput::new("one бета");
        input.move_end(false);
        handle_single_line_input(
            &mut input,
            PhysicalKey::Code(KeyCode::ArrowLeft),
            None,
            false,
            true,
            false,
            true,
            None,
            64,
        );
        assert_eq!(&input.text()[input.cursor..], "бета");
        assert!(input.text().is_char_boundary(input.cursor));

        input.set_cursor(0, false);
        handle_single_line_input(
            &mut input,
            PhysicalKey::Code(KeyCode::ArrowRight),
            None,
            false,
            false,
            true,
            true,
            None,
            64,
        );
        handle_single_line_input(
            &mut input,
            PhysicalKey::Code(KeyCode::ArrowRight),
            None,
            false,
            false,
            true,
            true,
            None,
            64,
        );
        assert_eq!(input.selected_text(), Some("on"));
        handle_single_line_input(
            &mut input,
            PhysicalKey::Code(KeyCode::ArrowLeft),
            None,
            false,
            false,
            true,
            true,
            None,
            64,
        );
        assert_eq!(input.selected_text(), Some("o"));
        assert!(input.text().is_char_boundary(input.cursor));
    }

    #[test]
    fn reverse_shift_selection_keeps_active_cursor_left_of_anchor() {
        let mut input = DatabaseDialogInput::new("abб界z");
        input.move_end(false);
        let anchor = input.cursor;

        for _ in 0..3 {
            handle_single_line_input(
                &mut input,
                PhysicalKey::Code(KeyCode::ArrowLeft),
                None,
                false,
                false,
                true,
                true,
                None,
                64,
            );
        }

        assert_eq!(input.selection_anchor, Some(anchor));
        assert!(input.cursor < anchor);
        assert_eq!(input.selected_text(), Some("б界z"));
        assert!(input.text().is_char_boundary(input.cursor));

        handle_single_line_input(
            &mut input,
            PhysicalKey::Code(KeyCode::ArrowRight),
            None,
            false,
            false,
            true,
            true,
            None,
            64,
        );
        assert_eq!(input.selection_anchor, Some(anchor));
        assert!(input.cursor < anchor);
        assert_eq!(input.selected_text(), Some("界z"));
        assert!(input.text().is_char_boundary(input.cursor));
    }

    #[test]
    fn database_one_line_surfaces_do_not_use_legacy_file_tree_scroll_math() {
        let sources = [
            include_str!("database/database_table_app_methods.rs"),
            include_str!("database/database_table_edit_methods.rs"),
            include_str!("database/database_app_methods.rs"),
            include_str!("../render_view/database_table_tab.rs"),
            include_str!("../render_view/database_table_tab_overlay.rs"),
            include_str!("../render_view/ide_panels/ide_panel_database_renderer.rs"),
        ];
        for source in sources {
            assert!(!source.contains("file_tree_name_input_scroll_x"));
            assert!(source.contains("single_line_cursor_geometry"));
        }
        let connection_hit = include_str!("database/database_app_methods.rs");
        assert!(!connection_hit.contains("glyph.advance * text_scale"));

        let shared_renderer = include_str!("../render_view/ide_panels/ide_panel_dialog_renderer.rs");
        assert!(!shared_renderer.contains("content_w + edge_pad"));
    }
}
