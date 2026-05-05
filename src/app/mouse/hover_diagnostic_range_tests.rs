#![allow(unused_imports)]

use super::super::{
    HOVER_STATE, HoverState, advance_hover_anim_progress, clear_hover_popup,
    compute_hover_visibility, compute_hover_visibility_from_matches,
    diagnostic_hover_byte_range_on_line, diagnostic_hover_range_on_line,
    diagnostic_hover_target_byte_on_line, diagnostic_hover_type_target_at_x,
    hover_byte_on_line_at_x, hover_bytes_share_token, hover_screen_y_to_content_y,
    hover_source_line_y_band, hover_token_bounds, hover_token_text, is_hover_target_byte,
    is_in_hover_popup_or_bridge, is_python_hover_keyword, normalize_hover_byte,
    suppress_hover_popup_until_mouse_move, type_hover_screen_y_matches_byte_line,
};

#[test]
fn diagnostic_hover_range_expands_when_target_is_string_prefix() {
    let mut editor = crate::editor::Editor::new(128);
    editor.insert_str("raise ValueError(f'513')\n");
    let text = editor.get_full_text();
    let f_string_offset = text.find("f'513'").unwrap();
    let f_string_col = text[..f_string_offset].encode_utf16().count() as u32;
    let f_string_end_col = f_string_col + "f'513'".encode_utf16().count() as u32;

    let byte_range =
        diagnostic_hover_byte_range_on_line(&editor, 0, f_string_col, f_string_end_col).unwrap();

    assert_eq!(byte_range.0, f_string_offset);
    assert_eq!(byte_range.1, f_string_offset + "f'513'".len());
}

#[test]
fn hover_visibility_shows_only_one_popup_during_conflict() {
    let (show_err, show_type, show_comb) = compute_hover_visibility(
        true,      // is_error_hovered
        true,      // error_timer_ready
        true,      // has_type_popup
        Some(100), // hovered_diag_type_target (e.g. byte of 'f')
        Some(103), // type_popup_byte (e.g. byte of '5')
        Some(103), // hover_byte_offset (e.g. byte of '5')
        false,     // stale_combined_popup
    );
    // Мы требуем ровно 1 попап в любой момент времени.
    // Приоритет отдается Type Popup для конкретного слова под курсором!
    assert!(!show_err);
    assert!(show_type);
    assert!(!show_comb);
}

#[test]
fn hover_state_bridge_keeps_diagnostic_and_type_as_one_popup_area() {
    let mut state = HoverState::default();
    state.diag_rect = Some((220.0, 100.0, 500.0, 120.0, 440.0, 480.0, 305.0));
    state.rect = Some((220.0, 220.0, 500.0, 180.0));
    state.popup = Some(crate::app::mouse::HoverPopup {
        text: "type info".to_string(),
        spans: Vec::new(),
        line_kinds: Vec::new(),
        inline_code_ranges: Vec::new(),
        byte_offset: 0,
        anchor_x: 460.0,
        anchor_y: 305.0,
        offset_x: None,
        offset_y: None,
        anim_progress: 0.0,
        scroll: crate::scroll::ScrollState::new(15.0),
        layout_cache: None,
    });

    let (inside_diag, diag_line) = state.popup_or_bridge_contains(450.0, 150.0, 1000.0, 1.0);
    let (inside_type, type_line) = state.popup_or_bridge_contains(450.0, 310.0, 1000.0, 1.0);
    let (outside, outside_line) = state.popup_or_bridge_contains(450.0, 460.0, 1000.0, 1.0);

    assert!(inside_diag);
    assert!(!diag_line);
    assert!(inside_type);
    assert!(type_line);
    assert!(!outside);
    assert!(!outside_line);
}

#[test]
fn hover_byte_ignores_whitespace_next_to_identifier() {
    let mut editor = crate::editor::Editor::new(128);
    editor.insert_str("    handlers\n");
    let text = editor.get_full_text();
    let handlers = text.find("handlers").unwrap();
    let after_handlers = handlers + "handlers".len();

    assert_eq!(normalize_hover_byte(&editor, handlers), Some(handlers));
    assert_eq!(
        normalize_hover_byte(&editor, after_handlers - 1),
        Some(after_handlers - 1)
    );
    assert_eq!(normalize_hover_byte(&editor, after_handlers), None);
    assert_eq!(normalize_hover_byte(&editor, handlers - 1), None);
}

#[test]
fn hover_byte_ignores_python_keywords_so_diagnostics_can_show() {
    let mut editor = crate::editor::Editor::new(128);
    editor.insert_str("    else:\n        raise ValueError()\n");
    let text = editor.get_full_text();
    let else_offset = text.find("else").unwrap();

    assert_eq!(normalize_hover_byte(&editor, else_offset), None);
    assert_eq!(normalize_hover_byte(&editor, else_offset + 2), None);
}

#[test]
fn diagnostic_hover_range_expands_to_whole_f_string_literal() {
    let mut editor = crate::editor::Editor::new(128);
    editor.insert_str("raise ValueError(f'513')\n");
    let text = editor.get_full_text();
    let literal_offset = text.find("513").unwrap();
    let literal_col = text[..literal_offset].encode_utf16().count() as u32;
    let f_string_offset = text.find("f'513'").unwrap();
    let f_string_col = text[..f_string_offset].encode_utf16().count() as u32;
    let f_string_end_col = f_string_col + "f'513'".encode_utf16().count() as u32;

    let range = diagnostic_hover_range_on_line(&editor, 0, literal_col, literal_col + 3).unwrap();
    let byte_range =
        diagnostic_hover_byte_range_on_line(&editor, 0, literal_col, literal_col + 3).unwrap();

    assert_eq!(range.0, f_string_col);
    assert_eq!(range.1, f_string_end_col);
    assert_eq!(byte_range.0, f_string_offset);
    assert_eq!(byte_range.1, f_string_offset + "f'513'".len());
    assert!(literal_offset >= byte_range.0);
    assert!(literal_offset + "513".len() <= byte_range.1);
    assert_eq!(normalize_hover_byte(&editor, range.2), Some(range.2));
}

#[test]
fn diagnostic_hover_target_is_stable_for_expanded_f_string_literal() {
    let mut editor = crate::editor::Editor::new(128);
    editor.insert_str("raise ValueError(f'513')\n");
    let text = editor.get_full_text();
    let f_string_offset = text.find("f'513'").unwrap();
    let f_string_col = text[..f_string_offset].encode_utf16().count() as u32;
    let f_string_end_col = f_string_col + "f'513'".encode_utf16().count() as u32;

    let first_target =
        diagnostic_hover_target_byte_on_line(&editor, 0, f_string_col, f_string_end_col);
    let second_target =
        diagnostic_hover_target_byte_on_line(&editor, 0, f_string_col, f_string_end_col);

    assert_eq!(first_target, Some(f_string_offset));
    assert_eq!(second_target, Some(f_string_offset));
}

#[test]
fn diagnostic_hover_range_does_not_create_type_target_for_keyword() {
    let mut editor = crate::editor::Editor::new(128);
    editor.insert_str("else:\n");

    assert_eq!(diagnostic_hover_range_on_line(&editor, 0, 0, 4), None);
}

#[test]
fn diagnostic_hover_range_returns_none_for_missing_line_or_empty_span() {
    let mut editor = crate::editor::Editor::new(128);
    editor.insert_str("value = 1\n");

    assert_eq!(diagnostic_hover_byte_range_on_line(&editor, 5, 0, 4), None);
    assert_eq!(diagnostic_hover_byte_range_on_line(&editor, 0, 6, 6), None);
    assert_eq!(diagnostic_hover_range_on_line(&editor, 5, 0, 4), None);
}

#[test]
fn diagnostic_hover_range_handles_utf16_columns_and_line_end_fallback() {
    let mut editor = crate::editor::Editor::new(128);
    editor.insert_str("emoji_😀 = value\n");
    let text = editor.get_full_text();
    let value = text.find("value").unwrap();
    let value_col = text[..value].encode_utf16().count() as u32;

    let range = diagnostic_hover_range_on_line(&editor, 0, value_col, value_col + 40).unwrap();
    let byte_range =
        diagnostic_hover_byte_range_on_line(&editor, 0, value_col, value_col + 40).unwrap();

    assert_eq!(&text[byte_range.0..byte_range.1], "value");
    assert_eq!(range.0, value_col);
    assert_eq!(range.2, value);
}

#[test]
fn diagnostic_hover_range_targets_self_crud_repo_dotted_attr_tail() {
    let mut editor = crate::editor::Editor::new(256);
    editor.insert_str("        self.booking_repo: BookingRepository = self.crud_repo\n");
    editor.insert_str("        self.box_repo = BoxRepository(session)\n");
    let text = editor.get_full_text();
    let target = text.find("self.crud_repo").unwrap();
    let target_end = target + "self.crud_repo".len();
    let crud_repo = text.find("crud_repo").unwrap();
    let start_col = text[..target].encode_utf16().count() as u32;
    let end_col = text[..target_end].encode_utf16().count() as u32;

    let range = diagnostic_hover_byte_range_on_line(&editor, 0, start_col, end_col).unwrap();
    let (token_start, token_end) = hover_token_bounds(&editor, range.2);
    let (exact_start, exact_end) = hover_token_bounds(&editor, crud_repo);

    assert_eq!((range.0, range.1), (crud_repo, target_end));
    assert_eq!((token_start, token_end), (crud_repo, target_end));
    assert_eq!(range.2, crud_repo);
    assert_eq!((exact_start, exact_end), (crud_repo, target_end));
    assert_eq!(normalize_hover_byte(&editor, crud_repo), Some(crud_repo));
    assert_eq!(normalize_hover_byte(&editor, target), Some(target));
    assert_eq!(normalize_hover_byte(&editor, target_end), None);
}

#[test]
fn diagnostic_hover_range_keeps_plain_identifier_first_token() {
    let mut editor = crate::editor::Editor::new(128);
    editor.insert_str("        repository = crud_repo\n");
    let text = editor.get_full_text();
    let target = text.find("crud_repo").unwrap();
    let target_end = target + "crud_repo".len();
    let start_col = text[..target].encode_utf16().count() as u32;
    let end_col = text[..target_end].encode_utf16().count() as u32;

    let range = diagnostic_hover_byte_range_on_line(&editor, 0, start_col, end_col).unwrap();

    assert_eq!((range.0, range.1, range.2), (target, target_end, target));
}

#[test]
fn diagnostic_hover_x_selects_self_or_crud_repo_on_same_dotted_span() {
    let mut editor = crate::editor::Editor::new(256);
    editor.insert_str("        self.booking_repo: BookingRepository = self.crud_repo\n");
    let text = editor.get_full_text();
    let self_attr = text.find("self.crud_repo").unwrap();
    let crud_repo = text.find("crud_repo").unwrap();
    let char_w = 10.0;

    let self_byte = hover_byte_on_line_at_x(&editor, 0, self_attr as f32 * char_w, |_ch| char_w)
        .and_then(|byte| normalize_hover_byte(&editor, byte));
    let crud_byte = hover_byte_on_line_at_x(&editor, 0, crud_repo as f32 * char_w, |_ch| char_w)
        .and_then(|byte| normalize_hover_byte(&editor, byte));

    assert_eq!(self_byte, Some(self_attr));
    assert_eq!(crud_byte, Some(crud_repo));
}

#[test]
fn diagnostic_hover_x_keeps_right_edge_of_self_on_self_not_attr_tail() {
    let mut editor = crate::editor::Editor::new(256);
    editor.insert_str("        self.booking_repo: BookingRepository = self.crud_repo\n");
    let text = editor.get_full_text();
    let self_attr = text.find("self.crud_repo").unwrap();
    let attr_end = self_attr + "self.crud_repo".len();
    let start_col = text[..self_attr].encode_utf16().count() as u32;
    let end_col = text[..attr_end].encode_utf16().count() as u32;
    let last_self_byte = self_attr + "sel".len();
    let char_w = 10.0;
    let fallback = diagnostic_hover_byte_range_on_line(&editor, 0, start_col, end_col)
        .map(|(_, _, target)| target);

    let near_right_edge_of_f = hover_byte_on_line_at_x(
        &editor,
        0,
        (last_self_byte as f32 * char_w) + char_w * 0.95,
        |_ch| char_w,
    )
    .and_then(|byte| normalize_hover_byte(&editor, byte));
    let render_target = diagnostic_hover_type_target_at_x(
        &editor,
        0,
        (last_self_byte as f32 * char_w) + char_w * 0.95,
        fallback,
        |_ch| char_w,
    );

    assert_eq!(fallback, Some(text.find("crud_repo").unwrap()));
    assert_eq!(near_right_edge_of_f, Some(last_self_byte));
    assert_eq!(render_target, Some(last_self_byte));
    assert_eq!(
        hover_token_text(&editor, last_self_byte).as_deref(),
        Some("self")
    );
}

#[test]
fn diagnostic_hover_x_uses_line_relative_x_with_nonzero_line_start() {
    let mut editor = crate::editor::Editor::new(256);
    editor.insert_str("before = 1\n");
    editor.insert_str("        self.booking_repo: BookingRepository = self.crud_repo\n");
    let text = editor.get_full_text();
    let line = 1;
    let line_start = editor.line_offsets[line];
    let self_attr = text.find("self.crud_repo").unwrap();
    let attr_end = self_attr + "self.crud_repo".len();
    let start_col = text[line_start..self_attr].encode_utf16().count() as u32;
    let end_col = text[line_start..attr_end].encode_utf16().count() as u32;
    let last_self_byte = self_attr + "sel".len();
    let last_self_col = last_self_byte - line_start;
    let char_w = 10.0;
    let fallback = diagnostic_hover_byte_range_on_line(&editor, line, start_col, end_col)
        .map(|(_, _, target)| target);

    let render_target = diagnostic_hover_type_target_at_x(
        &editor,
        line,
        (last_self_col as f32 * char_w) + char_w * 0.95,
        fallback,
        |_ch| char_w,
    );

    assert_eq!(fallback, Some(text.find("crud_repo").unwrap()));
    assert_eq!(render_target, Some(last_self_byte));
}

#[test]
fn diagnostic_hover_x_over_dot_falls_back_to_dotted_attr_tail() {
    let mut editor = crate::editor::Editor::new(256);
    editor.insert_str("        self.booking_repo: BookingRepository = self.crud_repo\n");
    let text = editor.get_full_text();
    let self_attr = text.find("self.crud_repo").unwrap();
    let attr_end = self_attr + "self.crud_repo".len();
    let dot = self_attr + "self".len();
    let start_col = text[..self_attr].encode_utf16().count() as u32;
    let end_col = text[..attr_end].encode_utf16().count() as u32;
    let char_w = 10.0;
    let byte_under_dot = hover_byte_on_line_at_x(&editor, 0, dot as f32 * char_w, |_ch| char_w);
    let fallback = diagnostic_hover_byte_range_on_line(&editor, 0, start_col, end_col)
        .map(|(_, _, target)| target);

    assert_eq!(
        byte_under_dot.and_then(|byte| normalize_hover_byte(&editor, byte)),
        None
    );
    assert_eq!(fallback, Some(text.find("crud_repo").unwrap()));
}

#[test]
fn diagnostic_hover_range_does_not_leak_self_crud_repo_to_next_line() {
    let mut editor = crate::editor::Editor::new(256);
    editor.insert_str("        self.booking_repo: BookingRepository = self.crud_repo\n");
    editor.insert_str("        self.box_repo = BoxRepository(session)\n");
    let text = editor.get_full_text();
    let next_line_box = text.find("self.box_repo").unwrap();
    let next_start_col = text[editor.line_offsets[1]..next_line_box]
        .encode_utf16()
        .count() as u32;
    let next_end_col = next_start_col + "self.box_repo".encode_utf16().count() as u32;

    let range =
        diagnostic_hover_byte_range_on_line(&editor, 1, next_start_col, next_end_col).unwrap();
    let target = text.find("self.crud_repo").unwrap();
    let next_target = text.find("box_repo").unwrap();

    assert_ne!(range.2, target);
    assert_eq!(range.2, next_target);
    assert_eq!(&text[range.0..range.1], "box_repo");
}

#[test]
fn hover_token_bounds_handles_empty_and_escaped_string_literal() {
    let editor = crate::editor::Editor::new(8);
    assert_eq!(hover_token_bounds(&editor, 99), (0, 0));
    assert_eq!(hover_token_text(&editor, 0).as_deref(), Some(""));

    let mut editor = crate::editor::Editor::new(128);
    editor.insert_str("value = r'can\\'t stop'\n");
    let text = editor.get_full_text();
    let escaped = text.find("stop").unwrap();
    let (start, end) = hover_token_bounds(&editor, escaped);

    assert_eq!(&text[start..end], "r'can\\'t stop'");
    assert!(hover_bytes_share_token(
        &editor,
        Some(text.find("can").unwrap()),
        Some(escaped)
    ));
}
