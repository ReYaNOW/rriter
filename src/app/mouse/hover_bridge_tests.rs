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
fn hover_bridge_reaches_full_source_line_when_popup_is_above() {
    let popup_rect = (220.0, 100.0, 500.0, 180.0);
    let line_top_y = 288.0;
    let line_bottom_y = 316.0;

    assert!(is_in_hover_popup_or_bridge(
        450.0,
        305.0,
        popup_rect,
        460.0,
        305.0,
        line_top_y,
        line_bottom_y,
        1000.0,
        1.0,
    ));
}

#[test]
fn hover_source_line_band_is_tight_around_anchor_y() {
    let (top, bottom) = hover_source_line_y_band(354.0, 1.0);

    assert_eq!((top, bottom), (344.0, 364.0));
}

#[test]
fn type_hover_y_hitbox_uses_middle_half_of_rendered_line() {
    let mut editor = crate::editor::Editor::new(64);
    editor.insert_str("alpha\nbeta\ngamma\n");
    let text = editor.get_full_text();
    let beta = text.find("beta").expect("beta token");
    let phys_to_visual = vec![0, 1, 2, 3];
    let line_h = 26.0;
    let baseline_offset = 19.0;
    let text_bias = baseline_offset - line_h * 0.5;
    let active_top = line_h + text_bias + line_h * 0.25;
    let active_bottom = line_h + text_bias + line_h * 0.75;

    assert!(!type_hover_screen_y_matches_byte_line(
        &editor,
        beta,
        &phys_to_visual,
        0.0,
        line_h,
        baseline_offset,
        active_top - 0.1,
    ));
    assert!(type_hover_screen_y_matches_byte_line(
        &editor,
        beta,
        &phys_to_visual,
        0.0,
        line_h,
        baseline_offset,
        active_top,
    ));
    assert!(type_hover_screen_y_matches_byte_line(
        &editor,
        beta,
        &phys_to_visual,
        0.0,
        line_h,
        baseline_offset,
        active_bottom - 0.1,
    ));
    assert!(!type_hover_screen_y_matches_byte_line(
        &editor,
        beta,
        &phys_to_visual,
        0.0,
        line_h,
        baseline_offset,
        active_bottom,
    ));
    assert!(!type_hover_screen_y_matches_byte_line(
        &editor,
        beta,
        &phys_to_visual,
        0.0,
        line_h,
        baseline_offset,
        line_h + text_bias,
    ));
    assert!(!type_hover_screen_y_matches_byte_line(
        &editor,
        beta,
        &phys_to_visual,
        0.0,
        line_h,
        baseline_offset,
        line_h + text_bias + line_h - 0.1,
    ));
}

#[test]
fn type_hover_y_hitbox_shrinks_first_line_after_top_clamp() {
    let mut editor = crate::editor::Editor::new(32);
    editor.insert_str("alpha\nbeta\n");
    let alpha = 0;
    let phys_to_visual = vec![0, 1, 2];

    assert_eq!(hover_screen_y_to_content_y(2.0, 0.0, 26.0, 19.0), Some(0.0));
    assert!(!type_hover_screen_y_matches_byte_line(
        &editor,
        alpha,
        &phys_to_visual,
        0.0,
        26.0,
        19.0,
        2.0,
    ));
    assert!(type_hover_screen_y_matches_byte_line(
        &editor,
        alpha,
        &phys_to_visual,
        0.0,
        26.0,
        19.0,
        13.0,
    ));
}

#[test]
fn hover_bridge_does_not_capture_below_type_anchor_band_when_popup_is_above() {
    let popup_rect = (96.0, 80.0, 760.0, 210.0);
    let (line_top_y, line_bottom_y) = hover_source_line_y_band(354.0, 1.0);

    assert!(is_in_hover_popup_or_bridge(
        620.0,
        line_bottom_y,
        popup_rect,
        620.0,
        354.0,
        line_top_y,
        line_bottom_y,
        1000.0,
        1.0,
    ));
    assert!(!is_in_hover_popup_or_bridge(
        620.0,
        line_bottom_y + 0.1,
        popup_rect,
        620.0,
        354.0,
        line_top_y,
        line_bottom_y,
        1000.0,
        1.0,
    ));
    assert!(!is_in_hover_popup_or_bridge(
        620.0,
        line_bottom_y + 18.0,
        popup_rect,
        620.0,
        354.0,
        line_top_y,
        line_bottom_y,
        1000.0,
        1.0,
    ));
}

#[test]
fn hover_bridge_does_not_capture_above_type_anchor_band_when_popup_is_below() {
    let popup_rect = (96.0, 420.0, 760.0, 210.0);
    let (line_top_y, line_bottom_y) = hover_source_line_y_band(354.0, 1.0);

    assert!(is_in_hover_popup_or_bridge(
        620.0,
        line_top_y,
        popup_rect,
        620.0,
        354.0,
        line_top_y,
        line_bottom_y,
        1000.0,
        1.0,
    ));
    assert!(!is_in_hover_popup_or_bridge(
        620.0,
        line_top_y - 0.1,
        popup_rect,
        620.0,
        354.0,
        line_top_y,
        line_bottom_y,
        1000.0,
        1.0,
    ));
    assert!(!is_in_hover_popup_or_bridge(
        620.0,
        line_top_y - 18.0,
        popup_rect,
        620.0,
        354.0,
        line_top_y,
        line_bottom_y,
        1000.0,
        1.0,
    ));
}

#[test]
fn hover_bridge_handles_popup_shifted_away_from_anchor() {
    let popup_rect = (20.0, 100.0, 520.0, 180.0);
    let line_top_y = 288.0;
    let line_bottom_y = 316.0;

    assert!(is_in_hover_popup_or_bridge(
        520.0,
        247.0,
        popup_rect,
        760.0,
        305.0,
        line_top_y,
        line_bottom_y,
        800.0,
        1.0,
    ));
}

#[test]
fn hover_bridge_keeps_popup_when_moving_up_from_token_anchor() {
    let popup_rect = (96.0, 80.0, 760.0, 210.0);
    let line_top_y = 340.0;
    let line_bottom_y = 368.0;

    assert!(is_in_hover_popup_or_bridge(
        620.0,
        318.0,
        popup_rect,
        620.0,
        354.0,
        line_top_y,
        line_bottom_y,
        1000.0,
        1.0,
    ));
}

#[test]
fn hover_bridge_does_not_capture_next_line_when_popup_is_above() {
    let popup_rect = (96.0, 80.0, 760.0, 210.0);
    let line_top_y = 340.0;
    let line_bottom_y = 368.0;

    assert!(!is_in_hover_popup_or_bridge(
        620.0,
        394.0,
        popup_rect,
        620.0,
        354.0,
        line_top_y,
        line_bottom_y,
        1000.0,
        1.0,
    ));
}

#[test]
fn hover_bridge_keeps_popup_when_cursor_moves_slightly_sideways() {
    let popup_rect = (96.0, 80.0, 760.0, 210.0);
    let line_top_y = 340.0;
    let line_bottom_y = 368.0;

    assert!(is_in_hover_popup_or_bridge(
        676.0,
        300.0,
        popup_rect,
        620.0,
        354.0,
        line_top_y,
        line_bottom_y,
        1000.0,
        1.0,
    ));
}

// --- ГЛОБАЛЬНЫЕ ТЕСТЫ НА ВЕСЬ МОДУЛЬ HOVER ---

#[test]
fn global_test_hover_token_bounds() {
    let mut editor = crate::editor::Editor::new(128);
    editor.insert_str("def my_func(arg1: int = 42):\n    return f'hello {arg1}'\n");
    let text = editor.get_full_text();

    let my_func_pos = text.find("my_func").unwrap();
    let (s, e) = hover_token_bounds(&editor, my_func_pos + 2);
    assert_eq!(&text[s..e], "my_func");

    let f_pos = text.find("f'hello").unwrap();
    let (s, e) = hover_token_bounds(&editor, f_pos);
    assert_eq!(&text[s..e], "f'hello {arg1}'");
}

#[test]
fn global_test_hover_identifier_pipeline_end_to_end() {
    let mut editor = crate::editor::Editor::new(128);
    editor.insert_str("value = my_func(42)\n");
    let text = editor.get_full_text();
    let symbol_pos = text.find("my_func").unwrap();
    let after_symbol = symbol_pos + "my_func".len();

    let normalized = normalize_hover_byte(&editor, after_symbol - 1).unwrap();
    let (start, end) = hover_token_bounds(&editor, normalized);
    let range =
        diagnostic_hover_byte_range_on_line(&editor, 0, symbol_pos as u32, after_symbol as u32)
            .unwrap();

    assert_eq!(normalize_hover_byte(&editor, after_symbol), None);
    assert_eq!(normalized, after_symbol - 1);
    assert_eq!(&text[start..end], "my_func");
    assert_eq!(
        hover_token_text(&editor, normalized).as_deref(),
        Some("my_func")
    );
    assert_eq!(
        (range.0, range.1, range.2),
        (symbol_pos, after_symbol, symbol_pos)
    );
}

#[test]
fn global_test_hover_f_string_pipeline_end_to_end() {
    let mut editor = crate::editor::Editor::new(128);
    editor.insert_str("message = f'hello {arg1}'\n");
    let text = editor.get_full_text();
    let literal_start = text.find("f'hello").unwrap();
    let literal_end = literal_start + "f'hello {arg1}'".len();
    let arg_pos = text.find("arg1").unwrap();

    let normalized = normalize_hover_byte(&editor, arg_pos + 2).unwrap();
    let (start, end) = hover_token_bounds(&editor, normalized);
    let range =
        diagnostic_hover_byte_range_on_line(&editor, 0, arg_pos as u32, (arg_pos + 4) as u32)
            .unwrap();

    assert_eq!(&text[start..end], "f'hello {arg1}'");
    assert_eq!(
        hover_token_text(&editor, normalized).as_deref(),
        Some("f'hello {arg1}'")
    );
    assert_eq!((range.0, range.1), (literal_start, literal_end));
    assert!(range.2 >= arg_pos && range.2 < arg_pos + 4);
}

#[test]
fn global_test_hover_popup_state_end_to_end_without_glow() {
    let mut state = HoverState::default();
    state.hovered_diags.push(0);
    state.hovered_diags_cache.push((0, 10.0, 20.0, 40.0, 60.0));
    state.byte_offset = Some(42);
    state.request_id = Some(7);

    assert!(!state.advance_diagnostic_hover_timer(Some(0), false, false, 0.0));
    let ready = state.advance_diagnostic_hover_timer(Some(0), false, false, 0.21);
    let popup = crate::app::mouse::HoverPopup {
        text: "value: int".to_string(),
        spans: vec![],
        line_kinds: vec![],
        inline_code_ranges: vec![],
        byte_offset: 42,
        anchor_x: 120.0,
        anchor_y: 64.0,
        offset_x: None,
        offset_y: None,
        anim_progress: 0.0,
        scroll: crate::scroll::ScrollState::new(15.0),
        layout_cache: None,
    };
    state.put_type_popup_after_draw(Some(popup), Some((80.0, 40.0, 220.0, 140.0)), 0.0);

    let (show_err, show_type, show_combined) = compute_hover_visibility(
        true,
        ready,
        state.popup.is_some(),
        Some(42),
        Some(42),
        Some(42),
        false,
    );
    let (inside_popup, _) = state.popup_or_bridge_contains(100.0, 60.0, 800.0, 1.0);

    assert!(show_err);
    assert!(show_type);
    assert!(show_combined);
    assert!(inside_popup);
    assert_eq!(
        state.popup.as_ref().map(|popup| popup.byte_offset),
        Some(42)
    );
}

#[test]
fn global_test_normalize_hover_byte() {
    let mut editor = crate::editor::Editor::new(128);
    editor.insert_str("    class MyClass:\n");
    let text = editor.get_full_text();

    let class_pos = text.find("class").unwrap();
    assert_eq!(normalize_hover_byte(&editor, class_pos), None);

    let myclass_pos = text.find("MyClass").unwrap();
    assert_eq!(
        normalize_hover_byte(&editor, myclass_pos),
        Some(myclass_pos)
    );
}

#[test]
fn global_test_is_python_hover_keyword() {
    assert!(is_python_hover_keyword("def"));
    assert!(is_python_hover_keyword("class"));
    assert!(is_python_hover_keyword("yield"));
    assert!(!is_python_hover_keyword("my_var"));
    assert!(!is_python_hover_keyword("int"));
}

#[test]
fn global_test_is_hover_target_byte() {
    let mut editor = crate::editor::Editor::new(128);
    editor.insert_str("word 123 _var =\n");
    let text = editor.get_full_text();

    assert!(is_hover_target_byte(&editor, text.find('w').unwrap()));
    assert!(is_hover_target_byte(&editor, text.find('1').unwrap()));
    assert!(is_hover_target_byte(&editor, text.find('_').unwrap()));
    assert!(!is_hover_target_byte(&editor, text.find('=').unwrap()));
    assert!(!is_hover_target_byte(&editor, text.find(' ').unwrap()));
}

#[test]
fn global_test_hover_state_machine() {
    let mut state = HoverState::default();

    state.diag_hover_timer_idx = Some(0);
    let ready = state.advance_diagnostic_hover_timer(Some(0), false, false, 0.3);
    assert!(ready);

    state.diag_rect = Some((10.0, 20.0, 100.0, 50.0, 15.0, 25.0, 100.0));
    state.byte_offset = Some(42);
    state.request_id = Some(1);

    let popup = crate::app::mouse::HoverPopup {
        text: "test".to_string(),
        spans: vec![],
        line_kinds: vec![],
        inline_code_ranges: vec![],
        byte_offset: 42,
        anchor_x: 10.0,
        anchor_y: 20.0,
        offset_x: None,
        offset_y: None,
        anim_progress: 0.0,
        scroll: crate::scroll::ScrollState::new(15.0),
        layout_cache: None,
    };
    state.put_type_popup_after_draw(Some(popup), Some((10.0, 20.0, 100.0, 50.0)), 0.0);
    assert!(state.popup.is_some());
    assert_eq!(state.rect.unwrap().0, 10.0);

    state.hide_diagnostic_popup_until_ready();
    assert!(state.diag_rect.is_none());
    assert!(state.popup.is_some());

    state.reset_diagnostic_popup();
    assert!(state.diag_rect.is_none());
}
