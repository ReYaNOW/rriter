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
fn hover_visibility_linter_error_only() {
    let (show_err, show_type, show_comb) = compute_hover_visibility(
        true,  // is_error_hovered
        true,  // error_timer_ready
        false, // has_type_popup
        None,  // hovered_diag_type_target
        None,  // type_popup_byte
        None,  // hover_byte_offset
        false, // stale_combined_popup
    );
    assert!(show_err);
    assert!(!show_type);
    assert!(!show_comb);
}

#[test]
fn hover_visibility_type_only() {
    let (show_err, show_type, show_comb) = compute_hover_visibility(
        false,     // is_error_hovered
        false,     // error_timer_ready
        true,      // has_type_popup
        None,      // hovered_diag_type_target
        Some(100), // type_popup_byte
        Some(100), // hover_byte_offset
        false,     // stale_combined_popup
    );
    assert!(!show_err);
    assert!(show_type);
    assert!(!show_comb);
}

#[test]
fn hover_visibility_combined() {
    let (show_err, show_type, show_comb) = compute_hover_visibility(
        true,      // is_error_hovered
        true,      // error_timer_ready
        true,      // has_type_popup
        Some(100), // hovered_diag_type_target
        Some(100), // type_popup_byte
        Some(100), // hover_byte_offset
        false,     // stale_combined_popup
    );
    assert!(show_err);
    assert!(show_type);
    assert!(show_comb);
}

#[test]
fn hover_visibility_during_transition() {
    let (show_err, show_type, show_comb) = compute_hover_visibility(
        true,      // is_error_hovered
        true,      // error_timer_ready
        true,      // has_type_popup
        Some(200), // hovered_diag_type_target (new location)
        Some(100), // type_popup_byte (old location)
        Some(200), // hover_byte_offset (new location)
        false,     // stale_combined_popup
    );
    assert!(!show_err);
    assert!(!show_type);
    assert!(!show_comb);
}

#[test]
fn hover_visibility_stale_combined_keeps_old_popup_when_cursor_moves() {
    let (show_err, show_type, show_comb) = compute_hover_visibility_from_matches(
        true,  // is_error_hovered
        false, // error_timer_ready
        true,  // has_type_popup
        true,  // diagnostic_needs_type
        true,  // type_matches_diag
        false, // hover_matches_diag
        false, // type_matches_hover
        true,  // stale_combined_popup
    );

    assert!(show_err);
    assert!(show_type);
    assert!(show_comb);
}

#[test]
fn hover_visibility_keeps_old_type_popup_without_diagnostic_match() {
    let (show_err, show_type, show_comb) = compute_hover_visibility_from_matches(
        true,  // is_error_hovered
        true,  // error_timer_ready
        true,  // has_type_popup
        true,  // diagnostic_needs_type
        false, // type_matches_diag
        false, // hover_matches_diag
        true,  // type_matches_hover
        false, // stale_combined_popup
    );

    assert!(!show_err);
    assert!(show_type);
    assert!(!show_comb);
}

#[test]
fn hover_visibility_waits_for_matching_type_before_showing_combined_type() {
    let (show_err, show_type, show_comb) =
        compute_hover_visibility(true, false, true, Some(100), Some(100), Some(100), false);
    assert!(!show_err);
    assert!(!show_type);
    assert!(!show_comb);

    let (show_err, show_type, show_comb) =
        compute_hover_visibility(true, true, true, Some(100), Some(200), Some(100), false);
    assert!(!show_err);
    assert!(!show_type);
    assert!(!show_comb);

    let (show_err, show_type, show_comb) =
        compute_hover_visibility(true, true, true, Some(100), Some(100), Some(100), false);
    assert!(show_err);
    assert!(show_type);
    assert!(show_comb);
}

#[test]
fn hover_animation_progress_uses_shared_slightly_slower_curve() {
    let next = advance_hover_anim_progress(0.0, 0.016);

    assert!((next - 0.128).abs() < 0.0001);
    assert!(next < 0.192);
    assert!(advance_hover_anim_progress(0.995, 0.016) < 1.0);
    assert_eq!(advance_hover_anim_progress(0.9995, 0.016), 1.0);
    assert_eq!(advance_hover_anim_progress(1.0, 0.016), 1.0);
}

#[test]
fn hover_animation_progress_snap_delta_is_subpixel_for_large_popup() {
    let progress = advance_hover_anim_progress(0.9995, 0.016);
    assert_eq!(progress, 1.0);
    assert!((1.0 - 0.9995) * 500.0 < 0.26);
}

#[test]
fn hover_visibility_combines_offsets_inside_same_identifier() {
    let mut editor = crate::editor::Editor::new(128);
    editor.insert_str("    handlers\n");
    let text = editor.get_full_text();
    let handlers_start = text.find("handlers").unwrap();
    let handlers_middle = handlers_start + 7;

    assert!(hover_bytes_share_token(
        &editor,
        Some(handlers_start),
        Some(handlers_middle)
    ));

    let (show_err, show_type, show_comb) = compute_hover_visibility_from_matches(
        true,
        true,
        true,
        true,
        hover_bytes_share_token(&editor, Some(handlers_start), Some(handlers_middle)),
        hover_bytes_share_token(&editor, Some(handlers_start), Some(handlers_middle)),
        true,
        false,
    );

    assert!(show_err);
    assert!(show_type);
    assert!(show_comb);
}

#[test]
fn hover_state_resets_all_diagnostic_popup_fields() {
    let mut state = HoverState::default();
    state.diag_rect = Some((10.0, 20.0, 30.0, 40.0, 11.0, 21.0, 31.0));
    state.diag_scroll.current = 12.0;
    state.diag_scroll.target = 24.0;
    state.diag_max_scroll = 99.0;
    state.diag_hover_timer = 0.3;
    state.diag_hover_timer_idx = Some(7);
    state.hovered_diags.push(7);
    state.hovered_diags_cache.push((7, 1.0, 2.0, 3.0, 4.0));
    state.hovered_diag_type_target = Some(99);
    state.popup_diag_type_target = Some(99);
    state.stale_combined_popup = true;
    state.diag_hover_ready_after_stale = true;
    state.diag_anim_progress = 1.0;
    state.diag_selection_anchor = Some(1);
    state.diag_selection_cursor = Some(3);
    state.diag_selecting = true;
    state.diag_text.push_str("diagnostic");
    state.diag_href = Some("https://example.invalid".to_string());

    state.reset_diagnostic_popup();

    assert!(state.diag_rect.is_none());
    assert_eq!(state.diag_scroll.current, 0.0);
    assert_eq!(state.diag_scroll.target, 0.0);
    assert_eq!(state.diag_max_scroll, 0.0);
    assert_eq!(state.diag_hover_timer, 0.0);
    assert!(state.diag_hover_timer_idx.is_none());
    assert!(state.hovered_diags.is_empty());
    assert!(state.hovered_diags_cache.is_empty());
    assert!(state.hovered_diag_type_target.is_none());
    assert!(state.popup_diag_type_target.is_none());
    assert!(!state.stale_combined_popup);
    assert!(!state.diag_hover_ready_after_stale);
    assert_eq!(state.diag_anim_progress, 0.0);
    assert!(state.diag_selection_anchor.is_none());
    assert!(state.diag_selection_cursor.is_none());
    assert!(!state.diag_selecting);
    assert!(state.diag_text.is_empty());
    assert!(state.diag_href.is_none());
}

#[test]
fn clear_hover_popup_reports_and_resets_thread_local_state() {
    assert!(!clear_hover_popup(None));

    HOVER_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.request_id = Some(11);
        state.definition_request_id = Some(12);
        state.byte_offset = Some(9);
        state.rect = Some((1.0, 2.0, 3.0, 4.0));
        state.diag_rect = Some((5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0));
        state.max_scroll = 20.0;
        state.selection_anchor = Some(1);
        state.selection_cursor = Some(2);
        state.selecting = true;
        state.diag_selection_anchor = Some(3);
        state.diag_selection_cursor = Some(4);
        state.diag_selecting = true;
        state.diag_text.push_str("diag");
    });

    assert!(clear_hover_popup(None));

    HOVER_STATE.with(|state| {
        let state = state.borrow();
        assert!(state.request_id.is_none());
        assert!(state.definition_request_id.is_none());
        assert!(state.byte_offset.is_none());
        assert!(state.rect.is_none());
        assert!(state.diag_rect.is_none());
        assert_eq!(state.max_scroll, 0.0);
        assert!(state.selection_anchor.is_none());
        assert!(state.selection_cursor.is_none());
        assert!(!state.selecting);
        assert!(state.diag_selection_anchor.is_none());
        assert!(state.diag_selection_cursor.is_none());
        assert!(!state.diag_selecting);
        assert!(state.diag_text.is_empty());
    });
}

#[test]
fn keyboard_suppression_clears_hover_timer_and_target() {
    HOVER_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.timer = 0.33;
        state.byte_offset = Some(42);
        state.request_id = Some(7);
        state.definition_request_id = Some(8);
    });

    assert!(suppress_hover_popup_until_mouse_move(None));

    HOVER_STATE.with(|state| {
        let state = state.borrow();
        assert_eq!(state.timer, 0.0);
        assert!(state.byte_offset.is_none());
        assert!(state.request_id.is_none());
        assert!(state.definition_request_id.is_none());
    });
}

#[test]
fn diagnostic_hover_timer_resets_when_hovered_diagnostic_changes() {
    let mut state = HoverState::default();

    assert!(!state.advance_diagnostic_hover_timer(Some(1), false, false, 0.21));
    assert_eq!(state.diag_hover_timer_idx, Some(1));
    assert_eq!(state.diag_hover_timer, 0.0);
    assert!(state.advance_diagnostic_hover_timer(Some(1), false, false, 0.21));

    assert!(!state.advance_diagnostic_hover_timer(Some(2), false, false, 0.21));
    assert_eq!(state.diag_hover_timer_idx, Some(2));
    assert_eq!(state.diag_hover_timer, 0.0);
}

#[test]
fn diagnostic_hover_timer_keeps_ticking_during_type_popup_transition() {
    let mut state = HoverState::default();

    assert!(!state.advance_diagnostic_hover_timer(None, false, true, 0.11));
    assert!(state.advance_diagnostic_hover_timer(None, false, true, 0.10));

    state.reset_diagnostic_popup();
    assert!(!state.advance_diagnostic_hover_timer(None, true, false, 0.19));
    assert!(state.advance_diagnostic_hover_timer(None, true, false, 0.02));
}

#[test]
fn pending_linter_popup_hide_does_not_reset_hover_timer_or_cache() {
    let mut state = HoverState::default();
    state.diag_rect = Some((10.0, 20.0, 30.0, 40.0, 11.0, 21.0, 31.0));
    state.diag_scroll.current = 9.0;
    state.diag_scroll.target = 9.0;
    state.diag_max_scroll = 30.0;
    state.diag_hover_timer = 0.11;
    state.diag_hover_timer_idx = Some(3);
    state.hovered_diags.push(3);
    state
        .hovered_diags_cache
        .push((3, 100.0, 200.0, 220.0, 140.0));
    state.diag_anim_progress = 0.7;
    state.diag_selection_anchor = Some(1);
    state.diag_selection_cursor = Some(2);
    state.diag_selecting = true;
    state.diag_text.push_str("pending");
    state.diag_href = Some("https://example.invalid".to_string());

    state.hide_diagnostic_popup_until_ready();

    assert!(state.diag_rect.is_none());
    assert_eq!(state.diag_scroll.current, 0.0);
    assert_eq!(state.diag_scroll.target, 0.0);
    assert_eq!(state.diag_max_scroll, 0.0);
    assert_eq!(state.diag_hover_timer, 0.11);
    assert_eq!(state.diag_hover_timer_idx, Some(3));
    assert_eq!(state.hovered_diags, vec![3]);
    assert_eq!(
        state.hovered_diags_cache,
        vec![(3, 100.0, 200.0, 220.0, 140.0)]
    );
    assert_eq!(state.diag_anim_progress, 0.0);
    assert!(state.diag_selection_anchor.is_none());
    assert!(state.diag_selection_cursor.is_none());
    assert!(!state.diag_selecting);
    assert!(state.diag_text.is_empty());
    assert!(state.diag_href.is_none());
}

#[test]
fn keyword_linter_popup_becomes_visible_after_delay_without_type_target() {
    let mut state = HoverState::default();
    state.hovered_diags_cache.push((0, 10.0, 20.0, 40.0, 60.0));
    state.hovered_diags.push(0);

    let ready = state.advance_diagnostic_hover_timer(Some(0), false, false, 0.0);
    let (show_err, show_type, show_combined) =
        compute_hover_visibility(true, ready, false, None, None, None, false);
    assert!(!show_err);
    assert!(!show_type);
    assert!(!show_combined);

    state.hide_diagnostic_popup_until_ready();
    assert!(!state.advance_diagnostic_hover_timer(Some(0), false, false, 0.0));
    let ready = state.advance_diagnostic_hover_timer(Some(0), false, false, 0.21);
    let (show_err, show_type, show_combined) =
        compute_hover_visibility(true, ready, false, None, None, None, false);

    assert!(show_err);
    assert!(!show_type);
    assert!(!show_combined);
    assert_eq!(state.hovered_diags_cache, vec![(0, 10.0, 20.0, 40.0, 60.0)]);
}

#[test]
fn type_popup_draw_flow_does_not_hold_hover_state_borrow() {
    let mut state = HoverState::default();
    state.popup = Some(crate::app::mouse::HoverPopup {
        text: "type info".to_string(),
        spans: Vec::new(),
        line_kinds: Vec::new(),
        inline_code_ranges: Vec::new(),
        byte_offset: 42,
        anchor_x: 100.0,
        anchor_y: 120.0,
        offset_x: None,
        offset_y: None,
        anim_progress: 0.0,
        scroll: crate::scroll::ScrollState::new(15.0),
        layout_cache: None,
    });
    state.selection_anchor = Some(9);
    state.selection_cursor = Some(3);
    state.diag_rect = Some((10.0, 20.0, 30.0, 40.0, 1.0, 2.0, 3.0));

    let (mut popup, selection, attached_diag) = state.take_type_popup_for_draw(true);

    assert!(state.popup.is_none());
    assert_eq!(selection, Some((3, 9)));
    assert_eq!(attached_diag, Some((10.0, 20.0, 30.0, 40.0)));

    popup
        .as_mut()
        .expect("popup must be detached for draw")
        .offset_x = Some(5.0);
    state.put_type_popup_after_draw(popup, Some((1.0, 2.0, 3.0, 4.0)), 12.0);

    assert!(state.popup.is_some());
    assert_eq!(state.rect, Some((1.0, 2.0, 3.0, 4.0)));
    assert_eq!(state.max_scroll, 12.0);
    assert_eq!(state.popup.as_ref().and_then(|p| p.offset_x), Some(5.0));
}

#[test]
fn stale_type_popup_stays_visible_while_new_target_loads() {
    let mut state = HoverState::default();
    state.popup = Some(crate::app::mouse::HoverPopup {
        text: "old ValueError hover".to_string(),
        spans: Vec::new(),
        line_kinds: Vec::new(),
        inline_code_ranges: Vec::new(),
        byte_offset: 17,
        anchor_x: 100.0,
        anchor_y: 120.0,
        offset_x: None,
        offset_y: None,
        anim_progress: 0.0,
        scroll: crate::scroll::ScrollState::new(15.0),
        layout_cache: None,
    });
    state.byte_offset = Some(25);

    assert!(state.should_show_stale_popup_while_target_loads(false));
    assert!(!state.should_show_stale_popup_while_target_loads(true));

    state.byte_offset = None;
    assert!(state.should_show_stale_popup_while_target_loads(false));
}

#[test]
fn stale_type_popup_stays_visible_when_cursor_moves_to_whitespace() {
    let mut state = HoverState::default();
    state.popup = Some(crate::app::mouse::HoverPopup {
        text: "some text".to_string(),
        spans: Vec::new(),
        line_kinds: Vec::new(),
        inline_code_ranges: Vec::new(),
        byte_offset: 17,
        anchor_x: 100.0,
        anchor_y: 120.0,
        offset_x: None,
        offset_y: None,
        anim_progress: 0.0,
        scroll: crate::scroll::ScrollState::new(15.0),
        layout_cache: None,
    });
    state.byte_offset = None;

    assert!(state.should_show_stale_popup_while_target_loads(false));
    assert!(!state.should_show_stale_popup_while_target_loads(true));
}

#[test]
fn combined_popup_stays_visible_while_next_word_loads() {
    let mut state = HoverState::default();
    state.popup = Some(crate::app::mouse::HoverPopup {
        text: "Literal[\"513\"]".to_string(),
        spans: Vec::new(),
        line_kinds: Vec::new(),
        inline_code_ranges: Vec::new(),
        byte_offset: 20,
        anchor_x: 100.0,
        anchor_y: 120.0,
        offset_x: None,
        offset_y: None,
        anim_progress: 0.0,
        scroll: crate::scroll::ScrollState::new(15.0),
        layout_cache: None,
    });
    state.rect = Some((90.0, 80.0, 240.0, 120.0));
    state.diag_rect = Some((90.0, 210.0, 240.0, 80.0, 100.0, 130.0, 145.0));
    state.hovered_diags.push(0);
    state
        .hovered_diags_cache
        .push((0, 100.0, 140.0, 162.0, 130.0));
    state.hovered_diag_type_target = Some(17);
    state.popup_diag_type_target = Some(17);

    let should_reset_diagnostics = state.begin_type_hover_transition(7);

    assert!(!should_reset_diagnostics);
    assert_eq!(state.byte_offset, Some(7));
    assert_eq!(state.popup.as_ref().map(|p| p.byte_offset), Some(20));
    assert_eq!(state.rect, Some((90.0, 80.0, 240.0, 120.0)));
    assert_eq!(
        state.diag_rect,
        Some((90.0, 210.0, 240.0, 80.0, 100.0, 130.0, 145.0))
    );
    assert_eq!(
        state.hovered_diags_cache,
        vec![(0, 100.0, 140.0, 162.0, 130.0)]
    );
    assert!(state.stale_combined_popup);
    assert!(!state.should_show_stale_popup_while_target_loads(false));
}

#[test]
fn combined_popup_stays_visible_when_diag_rect_was_cleared_before_transition() {
    let mut state = HoverState::default();
    state.popup = Some(crate::app::mouse::HoverPopup {
        text: "list[...]".to_string(),
        spans: Vec::new(),
        line_kinds: Vec::new(),
        inline_code_ranges: Vec::new(),
        byte_offset: 3717,
        anchor_x: 100.0,
        anchor_y: 120.0,
        offset_x: Some(20.0),
        offset_y: Some(-80.0),
        anim_progress: 0.0,
        scroll: crate::scroll::ScrollState::new(15.0),
        layout_cache: None,
    });
    state.rect = Some((120.0, 40.0, 260.0, 120.0));
    state.diag_rect = None;
    state.hovered_diag_type_target = None;
    state.popup_diag_type_target = Some(3717);
    state
        .hovered_diags_cache
        .push((0, 90.0, 100.0, 126.0, 180.0));

    let should_reset_diagnostics = state.begin_type_hover_transition(3659);

    assert!(!should_reset_diagnostics);
    assert!(state.stale_combined_popup);
    assert_eq!(state.popup.as_ref().map(|p| p.byte_offset), Some(3717));
    assert_eq!(state.rect, Some((120.0, 40.0, 260.0, 120.0)));
    assert_eq!(
        state.effective_hovered_diag_type_target(Some(3659)),
        Some(3717)
    );
    assert_eq!(
        state.record_hovered_diagnostic((1, 10.0, 20.0, 46.0, 60.0), Some(3659)),
        Some(3717)
    );

    let (show_error, show_type, show_combined) =
        compute_hover_visibility(true, true, true, Some(3717), Some(3717), Some(3659), true);
    assert!(show_error);
    assert!(show_type);
    assert!(show_combined);
}

#[test]
fn combined_popup_stays_visible_when_only_diag_cache_survived() {
    let mut state = HoverState::default();
    state.popup = Some(crate::app::mouse::HoverPopup {
        text: "old combined".to_string(),
        spans: Vec::new(),
        line_kinds: Vec::new(),
        inline_code_ranges: Vec::new(),
        byte_offset: 3717,
        anchor_x: 100.0,
        anchor_y: 120.0,
        offset_x: Some(20.0),
        offset_y: Some(-80.0),
        anim_progress: 0.0,
        scroll: crate::scroll::ScrollState::new(15.0),
        layout_cache: None,
    });
    state.rect = Some((120.0, 40.0, 260.0, 120.0));
    state.diag_rect = None;
    state.hovered_diag_type_target = None;
    state.popup_diag_type_target = None;
    state
        .hovered_diags_cache
        .push((0, 90.0, 100.0, 126.0, 180.0));

    state.mark_type_popup_drawn(false, None);
    let should_reset_diagnostics = state.begin_type_hover_transition(3659);

    assert!(!should_reset_diagnostics);
    assert!(state.stale_combined_popup);
    assert_eq!(state.popup_diag_type_target, Some(3717));
    assert_eq!(state.combined_type_target(), Some(3717));
    assert_eq!(
        state.effective_hovered_diag_type_target(Some(3659)),
        Some(3717)
    );

    let (show_error, show_type, show_combined) =
        compute_hover_visibility(true, true, true, Some(3717), Some(3717), Some(3659), true);
    assert!(show_error);
    assert!(show_type);
    assert!(show_combined);
}

#[test]
fn type_only_popup_stays_visible_while_next_word_loads() {
    let mut state = HoverState::default();
    state.popup = Some(crate::app::mouse::HoverPopup {
        text: "old hover".to_string(),
        spans: Vec::new(),
        line_kinds: Vec::new(),
        inline_code_ranges: Vec::new(),
        byte_offset: 20,
        anchor_x: 100.0,
        anchor_y: 120.0,
        offset_x: None,
        offset_y: None,
        anim_progress: 0.0,
        scroll: crate::scroll::ScrollState::new(15.0),
        layout_cache: None,
    });

    let should_reset_diagnostics = state.begin_type_hover_transition(7);

    assert!(!should_reset_diagnostics);
    assert_eq!(state.byte_offset, Some(7));
    assert_eq!(state.popup.as_ref().map(|p| p.byte_offset), Some(20));
    assert!(state.should_show_stale_popup_while_target_loads(false));
}

#[test]
fn clear_active_combined_popup_drops_type_and_diagnostic_together() {
    let mut state = HoverState::default();
    state.byte_offset = Some(17);
    state.popup = Some(crate::app::mouse::HoverPopup {
        text: "old combined".to_string(),
        spans: Vec::new(),
        line_kinds: Vec::new(),
        inline_code_ranges: Vec::new(),
        byte_offset: 17,
        anchor_x: 100.0,
        anchor_y: 120.0,
        offset_x: None,
        offset_y: None,
        anim_progress: 1.0,
        scroll: crate::scroll::ScrollState::new(15.0),
        layout_cache: None,
    });
    state.hovered_diag_type_target = Some(17);
    state.hovered_diags_cache.push((2, 90.0, 100.0, 122.0, 180.0));
    state.diag_rect = Some((90.0, 122.0, 240.0, 80.0, 90.0, 180.0, 111.0));

    assert!(state.clear_active_combined_popup());
    assert!(state.popup.is_none());
    assert!(state.byte_offset.is_none());
    assert!(state.diagnostic_popup_cache_is_empty());
    assert!(state.diag_rect.is_none());
    assert!(state.hovered_diag_type_target.is_none());
}

#[test]
fn stale_visibility_can_still_show_existing_combined_popup() {
    let mut state = HoverState::default();
    state.stale_combined_popup = true;
    state.hovered_diag_type_target = Some(17);

    assert_eq!(
        state.record_hovered_diagnostic((0, 100.0, 140.0, 162.0, 130.0), Some(99)),
        Some(17)
    );
    assert!(state.hovered_diags_cache.is_empty());

    let (show_error, show_type, show_combined) =
        compute_hover_visibility_from_matches(true, true, true, true, true, false, false, true);

    assert!(show_error);
    assert!(show_type);
    assert!(show_combined);
}

#[test]
fn stale_combined_popup_freezes_diagnostic_target_during_new_hover() {
    let mut state = HoverState::default();
    state.stale_combined_popup = true;
    state.hovered_diag_type_target = Some(3768);
    state.popup_diag_type_target = Some(3768);

    state.update_hovered_diag_type_target_for_frame(Some(3717));

    assert_eq!(state.hovered_diag_type_target, Some(3768));
    assert_eq!(
        state.effective_hovered_diag_type_target(Some(3717)),
        Some(3768)
    );
}

#[test]
fn active_combined_popup_keeps_target_during_whitespace_frame() {
    let mut state = HoverState::default();
    state.popup = Some(crate::app::mouse::HoverPopup {
        text: "old combined".to_string(),
        spans: Vec::new(),
        line_kinds: Vec::new(),
        inline_code_ranges: Vec::new(),
        byte_offset: 3717,
        anchor_x: 100.0,
        anchor_y: 120.0,
        offset_x: Some(20.0),
        offset_y: Some(-80.0),
        anim_progress: 0.0,
        scroll: crate::scroll::ScrollState::new(15.0),
        layout_cache: None,
    });
    state.hovered_diag_type_target = Some(3717);
    state.popup_diag_type_target = Some(3717);
    state.byte_offset = None;

    assert!(state.has_active_combined_type_popup());
    state.update_hovered_diag_type_target_for_frame(None);

    assert_eq!(state.hovered_diag_type_target, Some(3717));
    assert_eq!(state.effective_hovered_diag_type_target(None), Some(3717));
}

#[test]
fn non_stale_frame_accepts_new_diagnostic_target() {
    let mut state = HoverState::default();
    state.hovered_diag_type_target = Some(3768);

    state.update_hovered_diag_type_target_for_frame(Some(3717));

    assert_eq!(state.hovered_diag_type_target, Some(3717));
    assert_eq!(
        state.effective_hovered_diag_type_target(Some(3717)),
        Some(3717)
    );
}

#[test]
fn mark_type_popup_drawn_tracks_combined_target_only() {
    let mut state = HoverState::default();
    state.popup = Some(crate::app::mouse::HoverPopup {
        text: "old combined".to_string(),
        spans: Vec::new(),
        line_kinds: Vec::new(),
        inline_code_ranges: Vec::new(),
        byte_offset: 3717,
        anchor_x: 100.0,
        anchor_y: 120.0,
        offset_x: None,
        offset_y: None,
        anim_progress: 0.0,
        scroll: crate::scroll::ScrollState::new(15.0),
        layout_cache: None,
    });

    state.mark_type_popup_drawn(true, Some(3717));
    assert_eq!(state.popup_diag_type_target, Some(3717));

    state.mark_type_popup_drawn(false, None);
    assert_eq!(state.popup_diag_type_target, Some(3717));

    state.popup = None;
    state.mark_type_popup_drawn(false, None);
    assert!(state.popup_diag_type_target.is_none());

    state.popup = Some(crate::app::mouse::HoverPopup {
        text: "old combined".to_string(),
        spans: Vec::new(),
        line_kinds: Vec::new(),
        inline_code_ranges: Vec::new(),
        byte_offset: 3717,
        anchor_x: 100.0,
        anchor_y: 120.0,
        offset_x: None,
        offset_y: None,
        anim_progress: 0.0,
        scroll: crate::scroll::ScrollState::new(15.0),
        layout_cache: None,
    });
    state.popup_diag_type_target = Some(3717);
    state.stale_combined_popup = true;
    state.mark_type_popup_drawn(false, None);
    assert_eq!(state.popup_diag_type_target, Some(3717));
}
