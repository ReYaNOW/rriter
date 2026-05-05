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

    state.rect = Some((80.0, 40.0, 220.0, 140.0));
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
fn far_empty_space_does_not_keep_in_flight_hover_before_popup_rect_exists() {
    let mut state = HoverState::default();

    state.request_id = Some(9);
    assert!(!state.should_keep_popup_through_empty_space());

    state.request_id = None;
    state.definition_request_id = Some(10);
    assert!(!state.should_keep_popup_through_empty_space());

    state.definition_request_id = None;
    state.pending_popup = Some(crate::app::mouse::HoverPopup {
        text: "pending".to_string(),
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
    assert!(!state.should_keep_popup_through_empty_space());

    state.pending_popup = None;
    state.popup = Some(crate::app::mouse::HoverPopup {
        text: "not drawn yet".to_string(),
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
    assert!(!state.should_keep_popup_through_empty_space());

    state.rect = Some((80.0, 40.0, 220.0, 140.0));
    assert!(!state.should_keep_popup_through_empty_space());
}

#[test]
fn opening_popup_locks_hover_target_against_new_editor_word() {
    let mut state = HoverState::default();
    state.byte_offset = Some(17);
    state.popup = Some(crate::app::mouse::HoverPopup {
        text: "opening".to_string(),
        spans: Vec::new(),
        line_kinds: Vec::new(),
        inline_code_ranges: Vec::new(),
        byte_offset: 17,
        anchor_x: 100.0,
        anchor_y: 120.0,
        offset_x: None,
        offset_y: None,
        anim_progress: 0.4,
        scroll: crate::scroll::ScrollState::new(15.0),
        layout_cache: None,
    });
    state.rect = Some((80.0, 40.0, 220.0, 140.0));

    assert!(state.should_lock_hover_target_while_popup_opens(Some(99)));
    assert!(!state.should_lock_hover_target_while_popup_opens(Some(17)));

    state.popup.as_mut().unwrap().anim_progress = 1.0;
    assert!(!state.should_lock_hover_target_while_popup_opens(Some(99)));
}

#[test]
fn pending_hover_response_locks_target_before_popup_rect_exists() {
    let mut state = HoverState::default();
    state.byte_offset = Some(17);
    state.request_id = Some(1);

    assert!(state.should_lock_hover_target_while_popup_opens(Some(99)));

    state.request_id = None;
    state.pending_popup = Some(crate::app::mouse::HoverPopup {
        text: "waiting definition".to_string(),
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
    assert!(state.should_lock_hover_target_while_popup_opens(Some(99)));

    state.pending_popup = None;
    state.definition_request_id = Some(2);
    assert!(state.should_lock_hover_target_while_popup_opens(Some(99)));
}

#[test]
fn far_empty_space_does_not_keep_active_combined_popup_without_pending_request() {
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

    assert!(!state.keep_active_combined_popup_on_empty_space());
    assert_eq!(state.byte_offset, None);
    assert_eq!(state.timer, 0.19);
    assert!(!state.should_keep_popup_through_empty_space());
}

#[test]
fn far_empty_space_does_not_keep_combined_popup_or_opening_popup() {
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
        anim_progress: 0.3,
        scroll: crate::scroll::ScrollState::new(15.0),
        layout_cache: None,
    });
    state.popup_diag_type_target = Some(3717);
    state.rect = Some((80.0, 40.0, 220.0, 140.0));

    assert!(!state.should_keep_popup_through_empty_space());
    assert!(!state.keep_active_combined_popup_on_empty_space());

    state.pending_popup = Some(crate::app::mouse::HoverPopup {
        text: "pending next".to_string(),
        spans: Vec::new(),
        line_kinds: Vec::new(),
        inline_code_ranges: Vec::new(),
        byte_offset: 4000,
        anchor_x: 140.0,
        anchor_y: 120.0,
        offset_x: None,
        offset_y: None,
        anim_progress: 0.0,
        scroll: crate::scroll::ScrollState::new(15.0),
        layout_cache: None,
    });
    assert!(state.should_keep_popup_through_empty_space());
}

#[test]
fn mouse_motion_resets_type_hover_request_wait() {
    let mut state = HoverState::default();
    state.byte_offset = Some(12);
    state.timer = crate::app::mouse::HOVER_REQUEST_DELAY_SEC - 0.01;
    state.request_id = Some(1);
    state.definition_request_id = Some(2);
    state.pending_popup = Some(crate::app::mouse::HoverPopup {
        text: "pending".to_string(),
        spans: Vec::new(),
        line_kinds: Vec::new(),
        inline_code_ranges: Vec::new(),
        byte_offset: 12,
        anchor_x: 0.0,
        anchor_y: 0.0,
        offset_x: None,
        offset_y: None,
        anim_progress: 0.0,
        scroll: crate::scroll::ScrollState::new(15.0),
        layout_cache: None,
    });

    state.reset_type_hover_wait_after_mouse_motion();

    assert_eq!(state.timer, 0.0);
    assert!(state.request_id.is_none());
    assert!(state.definition_request_id.is_none());
    assert!(state.pending_popup.is_none());
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
