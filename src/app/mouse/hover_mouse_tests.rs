mod tests {
    use super::super::{
        advance_hover_anim_progress, compute_hover_visibility,
        compute_hover_visibility_from_matches, diagnostic_hover_byte_range_on_line,
        diagnostic_hover_range_on_line, diagnostic_hover_target_byte_on_line,
        hover_bytes_share_token, hover_token_bounds, hover_token_text, is_hover_target_byte,
        is_in_hover_popup_or_bridge, is_python_hover_keyword, normalize_hover_byte, HoverState,
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

    #[test]
    fn stale_transition_completion_keeps_diagnostic_timer_ready_for_new_popup() {
        let mut state = HoverState::default();
        state.stale_combined_popup = true;
        state.popup_diag_type_target = Some(3717);
        state.diag_rect = Some((10.0, 20.0, 30.0, 40.0, 11.0, 21.0, 31.0));
        state.hovered_diags.push(0);
        state.hovered_diags_cache.push((0, 11.0, 20.0, 46.0, 41.0));
        state.hovered_diag_type_target = Some(3717);
        state.diag_hover_timer_idx = Some(0);
        state.diag_hover_timer = 0.2;
        state.diag_anim_progress = 1.0;
        state.diag_text.push_str("old diagnostic");

        state.finish_stale_combined_transition();

        assert!(!state.stale_combined_popup);
        assert!(state.popup_diag_type_target.is_none());
        assert!(state.diag_rect.is_none());
        assert!(state.hovered_diags.is_empty());
        assert!(state.hovered_diags_cache.is_empty());
        assert!(state.hovered_diag_type_target.is_none());
        assert!(state.diag_text.is_empty());
        assert_eq!(state.diag_anim_progress, 0.0);
        assert!(state.diag_hover_ready_after_stale);
        assert!(state.advance_diagnostic_hover_timer(Some(1), true, false, 0.0));
        assert_eq!(state.diag_hover_timer_idx, Some(1));
        assert_eq!(state.diag_hover_timer, 0.2);
        assert!(!state.diag_hover_ready_after_stale);
    }

    #[test]
    fn empty_space_keeps_popup_while_hover_transition_is_pending() {
        let mut state = HoverState::default();

        assert!(!state.should_keep_popup_through_empty_space());

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

        assert!(!state.should_keep_popup_through_empty_space());

        state.request_id = Some(1);
        assert!(state.should_keep_popup_through_empty_space());

        state.request_id = None;
        state.definition_request_id = Some(2);
        assert!(state.should_keep_popup_through_empty_space());

        state.definition_request_id = None;
        state.pending_popup = Some(crate::app::mouse::HoverPopup {
            text: "new hover".to_string(),
            spans: Vec::new(),
            line_kinds: Vec::new(),
            inline_code_ranges: Vec::new(),
            byte_offset: 3659,
            anchor_x: 140.0,
            anchor_y: 120.0,
            offset_x: None,
            offset_y: None,
            anim_progress: 0.0,
            scroll: crate::scroll::ScrollState::new(15.0),
            layout_cache: None,
        });
        assert!(state.should_keep_popup_through_empty_space());

        state.pending_popup = None;
        state.stale_combined_popup = true;
        assert!(state.should_keep_popup_through_empty_space());
    }

    #[test]
    fn empty_space_keeps_active_combined_popup_without_pending_request() {
        let mut state = HoverState::default();
        state.popup = Some(crate::app::mouse::HoverPopup {
            text: "combined".to_string(),
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
        state.byte_offset = None;
        state.timer = 0.19;

        assert!(state.keep_active_combined_popup_on_empty_space());
        assert_eq!(state.byte_offset, Some(3717));
        assert_eq!(state.timer, 0.0);
        assert!(state.should_keep_popup_through_empty_space());
    }

    #[test]
    fn stale_combined_popup_uses_frozen_diagnostics_not_live_cache() {
        let mut state = HoverState::default();
        state.popup = Some(crate::app::mouse::HoverPopup {
            text: "old openapi_config_arg hover".to_string(),
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
        state.diag_rect = Some((120.0, 160.0, 260.0, 80.0, 130.0, 170.0, 180.0));
        state.hovered_diag_type_target = Some(3717);
        state.popup_diag_type_target = Some(3717);
        state
            .hovered_diags_cache
            .push((0, 130.0, 170.0, 192.0, 180.0));

        state.begin_type_hover_transition(3659);
        state
            .hovered_diags_cache
            .push((1, 90.0, 110.0, 132.0, 140.0));

        assert!(state.stale_combined_popup);
        assert_eq!(
            state.diagnostic_popup_cache(),
            &[(0, 130.0, 170.0, 192.0, 180.0)]
        );
    }

    #[test]
    fn stale_combined_popup_ignores_new_hovered_diagnostic_until_new_popup_ready() {
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
        state.popup_diag_type_target = Some(3717);
        state.hovered_diags_cache.push((0, 10.0, 20.0, 42.0, 60.0));

        state.begin_type_hover_transition(3659);
        let target = state.record_hovered_diagnostic((1, 80.0, 90.0, 112.0, 120.0), Some(3659));

        assert_eq!(target, Some(3717));
        assert_eq!(state.hovered_diags_cache, vec![(0, 10.0, 20.0, 42.0, 60.0)]);
        assert_eq!(
            state.diagnostic_popup_cache(),
            &[(0, 10.0, 20.0, 42.0, 60.0)]
        );
    }

    #[test]
    fn active_combined_popup_switches_target_for_same_diagnostic() {
        let mut state = HoverState::default();
        state.popup = Some(crate::app::mouse::HoverPopup {
            text: "combined".to_string(),
            spans: Vec::new(),
            line_kinds: Vec::new(),
            inline_code_ranges: Vec::new(),
            byte_offset: 100,
            anchor_x: 0.0,
            anchor_y: 0.0,
            offset_x: None,
            offset_y: None,
            anim_progress: 0.0,
            scroll: crate::scroll::ScrollState::new(15.0),
            layout_cache: None,
        });
        state.popup_diag_type_target = Some(100);
        state.hovered_diags_cache.push((0, 10.0, 20.0, 42.0, 60.0));

        let target = state.record_hovered_diagnostic((0, 10.0, 20.0, 42.0, 60.0), Some(200));

        assert_eq!(target, Some(200));
        assert_eq!(state.hovered_diags_cache, vec![(0, 10.0, 20.0, 42.0, 60.0)]);
    }

    #[test]
    fn active_combined_popup_does_not_collect_different_target_diagnostic() {
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
        state.popup_diag_type_target = Some(3717);
        state.hovered_diags_cache.push((0, 10.0, 20.0, 42.0, 60.0));

        let target = state.record_hovered_diagnostic((1, 80.0, 90.0, 112.0, 120.0), Some(3659));

        assert_eq!(target, Some(3717));
        assert_eq!(state.hovered_diags_cache, vec![(0, 10.0, 20.0, 42.0, 60.0)]);
        assert_eq!(
            state.diagnostic_popup_cache(),
            &[(0, 10.0, 20.0, 42.0, 60.0)]
        );
    }

    #[test]
    fn stale_completion_clears_frozen_diagnostics_before_handlers_popup() {
        let mut state = HoverState::default();
        state.stale_combined_popup = true;
        state.popup_diag_type_target = Some(3717);
        state
            .stale_hovered_diags_cache
            .push((0, 10.0, 20.0, 42.0, 60.0));
        state.hovered_diags_cache.push((0, 10.0, 20.0, 42.0, 60.0));

        state.finish_stale_combined_transition();

        assert!(!state.stale_combined_popup);
        assert!(state.stale_hovered_diags_cache.is_empty());
        assert!(state.hovered_diags_cache.is_empty());

        assert_eq!(
            state.record_hovered_diagnostic((1, 80.0, 90.0, 112.0, 120.0), Some(3659)),
            Some(3659)
        );
        assert_eq!(
            state.diagnostic_popup_cache(),
            &[(1, 80.0, 90.0, 112.0, 120.0)]
        );
    }

    #[test]
    fn hover_response_preserves_pending_diagnostic_context_for_combined_popup() {
        let mut state = HoverState::default();
        state.hovered_diags.push(0);
        state.hovered_diags_cache.push((0, 10.0, 20.0, 40.0, 60.0));
        state.diag_hover_timer = 0.21;
        state.diag_hover_timer_idx = Some(0);

        state.hide_diagnostic_popup_until_ready();

        assert_eq!(state.hovered_diags, vec![0]);
        assert_eq!(state.hovered_diags_cache, vec![(0, 10.0, 20.0, 40.0, 60.0)]);
        assert_eq!(state.diag_hover_timer, 0.21);
        assert_eq!(state.diag_hover_timer_idx, Some(0));
    }

    #[test]
    fn diagnostic_hover_range_expands_when_target_is_string_prefix() {
        let mut editor = crate::editor::Editor::new(128);
        editor.insert_str("raise ValueError(f'513')\n");
        let text = editor.get_full_text();
        let f_string_offset = text.find("f'513'").unwrap();
        let f_string_col = text[..f_string_offset].encode_utf16().count() as u32;
        let f_string_end_col = f_string_col + "f'513'".encode_utf16().count() as u32;

        let byte_range =
            diagnostic_hover_byte_range_on_line(&editor, 0, f_string_col, f_string_end_col)
                .unwrap();

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

        let range =
            diagnostic_hover_range_on_line(&editor, 0, literal_col, literal_col + 3).unwrap();
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
}
