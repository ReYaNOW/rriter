use super::*;

#[test]
fn module_header_wrap_does_not_split_marker_from_path() {
    assert!(!hover_wrap_space_can_break("[[MODULE]] ".chars().count()));
    assert!(hover_wrap_space_can_break(
        "[[MODULE]] car_wash.long.path ".chars().count()
    ));
}

#[test]
fn test_valid_diagnostic_popup_cache_drops_stale_indices() {
    let cache = vec![(0, 1.0, 2.0, 3.0, 4.0), (3, 5.0, 6.0, 7.0, 8.0)];
    assert_eq!(
        valid_diagnostic_popup_cache(cache, 3),
        vec![(0, 1.0, 2.0, 3.0, 4.0)]
    );
}

#[test]
fn test_hover_y_position_fits_above() {
    let y = compute_hover_y_position(100.0, 20.0, 40.0, 1000.0, 1.0);
    // line_top_y=100, box_h=40, margin=8. Ожидаем сверху: 100 - 40 - 8 = 52.
    // 52 >= 40.0 (min_y). Влезает сверху.
    assert_eq!(y, 52.0);
}

#[test]
fn test_hover_y_position_fallback_below() {
    let y = compute_hover_y_position(60.0, 20.0, 40.0, 1000.0, 1.0);
    // line_top_y=60, box_h=40, margin=8. Ожидаем сверху: 60 - 40 - 8 = 12.
    // 12 < 40.0. Не влезает сверху.
    // Снизу: 60 + 20 + 8 = 88. Высота: 88 + 40 = 128 <= 990 (max_y). Влезает снизу!
    assert_eq!(y, 88.0);
}

#[test]
fn test_hover_y_position_clamped_top_when_both_fail_but_above_is_better() {
    // Окно очень маленькое (200px). line_top_y = 120, box_h = 100.
    // Сверху: 120 - 100 - 8 = 12. Места сверху: 120 - 40 = 80.
    // Снизу: 120 + 20 + 8 = 148. Места снизу: 190 - 140 = 50.
    // 80 > 50 -> оставляем сверху и прижимаем к минимуму (40.0).
    let y = compute_hover_y_position(120.0, 20.0, 100.0, 200.0, 1.0);
    assert_eq!(y, 40.0);
}

#[test]
fn test_hover_y_position_forced_below_when_both_fail_but_below_is_better() {
    // Окно 200px. line_top_y = 60, box_h = 100.
    // Сверху: 60 - 100 - 8 = -48. Места сверху: 60 - 40 = 20.
    // Снизу: 60 + 20 + 8 = 88. Места снизу: 190 - 80 = 110.
    // 110 > 20 -> насильно переносим вниз.
    let y = compute_hover_y_position(60.0, 20.0, 100.0, 200.0, 1.0);
    assert_eq!(y, 88.0);
}

#[test]
fn test_diagnostic_layout_end_to_end_fits_above() {
    let (bx, by) =
        compute_diagnostic_layout(500.0, 20.0, 200.0, 100.0, 1000.0, 1000.0, 1.0, 300.0, None);
    assert_eq!(bx, 300.0);
    assert_eq!(by, 392.0); // 500 - 100 - 8
}

#[test]
fn test_diagnostic_layout_end_to_end_shifts_x_when_out_of_bounds() {
    let (bx, _by) =
        compute_diagnostic_layout(500.0, 20.0, 200.0, 100.0, 1000.0, 1000.0, 1.0, 900.0, None);
    assert_eq!(bx, 780.0); // 1000 - 200 - 20 = 780
}

#[test]
fn test_diagnostic_layout_end_to_end_considers_hover_anchor_x() {
    let (bx, _by) = compute_diagnostic_layout(
        500.0,
        20.0,
        200.0,
        100.0,
        1000.0,
        1000.0,
        1.0,
        400.0,
        Some(350.0),
    );
    assert_eq!(bx, 350.0); // min(400, 350)
}

#[test]
fn test_diagnostic_layout_end_to_end_forces_below_when_y_too_small() {
    let (_bx, by) =
        compute_diagnostic_layout(40.0, 20.0, 200.0, 100.0, 1000.0, 1000.0, 1.0, 100.0, None);
    assert_eq!(by, 68.0); // Не влезло сверху (40-100-8 < 40). Идет вниз: 40 + 20 + 8 = 68
}

#[test]
fn test_diag_popup_byte_at_chooses_nearest_line_and_side() {
    super::DIAG_CHARS.with(|chars| {
        let mut chars = chars.borrow_mut();
        chars.clear();
        chars.push(super::DiagChar {
            x: 10.0,
            y: 20.0,
            w: 8.0,
            h: 16.0,
            byte_offset: 3,
        });
        chars.push(super::DiagChar {
            x: 30.0,
            y: 20.0,
            w: 8.0,
            h: 16.0,
            byte_offset: 4,
        });
        chars.push(super::DiagChar {
            x: 10.0,
            y: 60.0,
            w: 8.0,
            h: 16.0,
            byte_offset: 20,
        });
    });

    assert_eq!(super::diag_popup_byte_at(12.0, 25.0), 3);
    assert_eq!(super::diag_popup_byte_at(40.0, 25.0), 5);
    assert_eq!(super::diag_popup_byte_at(12.0, 65.0), 20);

    super::DIAG_CHARS.with(|chars| chars.borrow_mut().clear());
    assert_eq!(super::diag_popup_byte_at(0.0, 0.0), 0);
}

#[test]
fn test_animated_scissor_keeps_nearest_corner_fixed() {
    let (sc_x, sc_y, sc_w, sc_h) =
        compute_animated_scissor(10.0, 20.0, 100.0, 100.0, 50.0, 50.0, 0.5);
    // Cursor is at 10, 20. Popup is at 100, 100 with size 50, 50.
    // Target is 96, 96, 58, 58 (due to -4.0 margin and +8.0 size)
    // Popup is below cursor, so top-left stays fixed while far edges expand.
    let expected_w = 58.0 * smooth_hover_width_progress(0.5, 58.0);
    let expected_h = 58.0 * smooth_hover_height_progress(0.5);
    assert_eq!(sc_x, 96.0);
    assert_eq!(sc_y, 96.0);
    assert!((sc_w - expected_w).abs() < 0.001);
    assert!((sc_h - expected_h).abs() < 0.001);
}

#[test]
fn test_animated_scissor_keeps_right_edge_and_bottom_edge_fixed_when_popup_above_cursor() {
    let (sc_x, sc_y, sc_w, sc_h) =
        compute_animated_scissor(200.0, 200.0, 100.0, 100.0, 50.0, 50.0, 0.5);
    let expected_w = 58.0 * smooth_hover_width_progress(0.5, 58.0);
    let expected_h = 58.0 * smooth_hover_height_progress(0.5);
    assert!((sc_x + sc_w - 154.0).abs() < 0.001);
    assert!((sc_y + sc_h - 154.0).abs() < 0.001);
    assert!((sc_w - expected_w).abs() < 0.001);
    assert!((sc_h - expected_h).abs() < 0.001);
}

#[test]
fn test_animated_scissor_is_full_popup_at_progress_one() {
    let (sc_x, sc_y, sc_w, sc_h) =
        compute_animated_scissor(10.0, 20.0, 100.0, 100.0, 50.0, 50.0, 1.0);
    assert_eq!(sc_x, 96.0);
    assert_eq!(sc_y, 96.0);
    assert_eq!(sc_w, 58.0);
    assert_eq!(sc_h, 58.0);
}

#[test]
fn test_animated_scissor_is_zero_at_progress_zero() {
    let (sc_x, sc_y, sc_w, sc_h) =
        compute_animated_scissor(10.0, 20.0, 100.0, 100.0, 50.0, 50.0, 0.0);
    assert_eq!(sc_x, 96.0);
    assert_eq!(sc_y, 96.0);
    assert_eq!(sc_w, 0.0);
    assert_eq!(sc_h, 0.0);
}

#[test]
fn test_animated_popup_frame_keeps_nearest_corner_fixed() {
    let (frame_x, frame_y, frame_w, frame_h) =
        compute_animated_popup_frame(10.0, 20.0, 100.0, 100.0, 50.0, 50.0, 0.5);
    let expected_w = 50.0 * smooth_hover_width_progress(0.5, 50.0);
    let expected_h = 50.0 * smooth_hover_height_progress(0.5);
    assert_eq!(frame_x, 100.0);
    assert_eq!(frame_y, 100.0);
    assert!((frame_w - expected_w).abs() < 0.001);
    assert!((frame_h - expected_h).abs() < 0.001);
}

#[test]
fn test_animated_popup_frame_keeps_right_edge_and_bottom_edge_fixed_when_popup_above_cursor() {
    let (frame_x, frame_y, frame_w, frame_h) =
        compute_animated_popup_frame(200.0, 200.0, 100.0, 100.0, 50.0, 50.0, 0.5);
    let expected_w = 50.0 * smooth_hover_width_progress(0.5, 50.0);
    let expected_h = 50.0 * smooth_hover_height_progress(0.5);
    assert!((frame_x + frame_w - 150.0).abs() < 0.001);
    assert!((frame_y + frame_h - 150.0).abs() < 0.001);
    assert!((frame_w - expected_w).abs() < 0.001);
    assert!((frame_h - expected_h).abs() < 0.001);
}

#[test]
fn test_opening_hover_popup_animation_uses_stable_source_anchor() {
    let target = (100.0, 100.0, 500.0, 300.0);
    let progress = 0.55;
    let source_anchor = (220.0, 520.0);
    let live_mouse_inside_popup = (460.0, 180.0);
    let (stable_mx, stable_my) = stable_hover_animation_mouse(
        live_mouse_inside_popup.0,
        live_mouse_inside_popup.1,
        source_anchor.0,
        source_anchor.1,
        progress,
    );

    let frame_from_source = compute_animated_popup_frame(
        source_anchor.0,
        source_anchor.1,
        target.0,
        target.1,
        target.2,
        target.3,
        progress,
    );
    let frame_from_stable_mouse = compute_animated_popup_frame(
        stable_mx, stable_my, target.0, target.1, target.2, target.3, progress,
    );
    let frame_from_live_mouse = compute_animated_popup_frame(
        live_mouse_inside_popup.0,
        live_mouse_inside_popup.1,
        target.0,
        target.1,
        target.2,
        target.3,
        progress,
    );

    assert_eq!(frame_from_stable_mouse, frame_from_source);
    assert_ne!(frame_from_live_mouse, frame_from_source);
}

#[test]
fn test_opening_combined_popup_keeps_source_side_when_cursor_moves_inside() {
    let target = (100.0, 100.0, 500.0, 300.0);
    let progress = 0.55;
    let source_anchor = (650.0, 520.0);
    let live_mouse_inside_left_side = (130.0, 180.0);
    let (stable_mx, stable_my) = stable_hover_animation_mouse(
        live_mouse_inside_left_side.0,
        live_mouse_inside_left_side.1,
        source_anchor.0,
        source_anchor.1,
        progress,
    );

    let frame_from_stable_mouse = compute_combined_popup_frame(
        stable_mx, stable_my, target.0, target.1, target.2, target.3, progress, true,
    );
    let frame_from_live_mouse = compute_combined_popup_frame(
        live_mouse_inside_left_side.0,
        live_mouse_inside_left_side.1,
        target.0,
        target.1,
        target.2,
        target.3,
        progress,
        true,
    );

    assert!((frame_from_stable_mouse.0 + frame_from_stable_mouse.2 - 600.0).abs() < 0.001);
    assert!(frame_from_stable_mouse.0 > 100.0);
    assert_eq!(frame_from_live_mouse.0, 100.0);
}

#[test]
fn test_fully_open_hover_popup_animation_uses_live_mouse() {
    assert_eq!(
        stable_hover_animation_mouse(460.0, 180.0, 220.0, 520.0, 1.0),
        (460.0, 180.0)
    );
}

#[test]
fn test_popup_above_cursor_expands_bottom_to_top() {
    let (frame_x, frame_y, frame_w, frame_h) =
        compute_animated_popup_frame(220.0, 520.0, 100.0, 100.0, 500.0, 300.0, 0.25);
    assert!((frame_y + frame_h - 400.0).abs() < 0.001);
    assert!(frame_y > 100.0);
    assert!(frame_h > 0.0);
    assert!(frame_w > 0.0);
    assert!(frame_x >= 100.0);
}

#[test]
fn test_combined_popup_above_cursor_expands_bottom_to_top() {
    let (frame_x, frame_y, frame_w, frame_h) =
        compute_combined_popup_frame(220.0, 520.0, 100.0, 100.0, 500.0, 300.0, 0.25, true);
    assert!((frame_y + frame_h - 400.0).abs() < 0.001);
    assert!(frame_y > 100.0);
    assert!(frame_h > 0.0);
    assert!(frame_w > 0.0);
    assert!(frame_x >= 100.0);
}

#[test]
fn test_combined_popup_below_cursor_expands_top_to_bottom() {
    let (_frame_x, frame_y, _frame_w, frame_h) =
        compute_combined_popup_frame(220.0, 20.0, 100.0, 100.0, 500.0, 300.0, 0.25, true);
    assert_eq!(frame_y, 100.0);
    assert!(frame_y + frame_h < 400.0);
}

#[test]
fn test_popup_below_cursor_expands_top_to_bottom() {
    let (_frame_x, frame_y, _frame_w, frame_h) =
        compute_animated_popup_frame(220.0, 20.0, 100.0, 100.0, 500.0, 300.0, 0.25);
    assert_eq!(frame_y, 100.0);
    assert!(frame_y + frame_h < 400.0);
}

#[test]
fn test_animated_popup_frame_is_target_at_progress_one() {
    let frame = compute_animated_popup_frame(10.0, 20.0, 100.0, 100.0, 50.0, 50.0, 1.0);
    assert_eq!(frame, (100.0, 100.0, 50.0, 50.0));
}

#[test]
fn test_animated_popup_frame_is_visible_before_reaching_target() {
    let (frame_x, _frame_y, frame_w, frame_h) =
        compute_animated_popup_frame(10.0, 20.0, 100.0, 100.0, 50.0, 50.0, 0.2);
    assert!(frame_w > 0.0);
    assert!(frame_h > 0.0);
    assert_eq!(frame_x, 100.0);
}

#[test]
fn test_animated_popup_frame_uses_settings_style_smooth_progress() {
    let (_frame_x, _frame_y, frame_w, frame_h) =
        compute_animated_popup_frame(10.0, 20.0, 100.0, 100.0, 500.0, 300.0, 0.8);
    let expected_w = 500.0 * smooth_hover_anim_progress(0.8 / 0.94);
    let expected_h = 300.0 * smooth_hover_height_progress(0.8);
    assert!((frame_w - expected_w).abs() < 0.001);
    assert!((frame_h - expected_h).abs() < 0.001);
}

#[test]
fn test_wide_popup_right_edge_settles_before_final_frames() {
    let (frame_x, _frame_y, frame_w, _frame_h) =
        compute_animated_popup_frame(10.0, 20.0, 100.0, 100.0, 500.0, 300.0, 0.94);
    assert_eq!(frame_x, 100.0);
    assert!((frame_x + frame_w - 600.0).abs() < 0.001);
}

#[test]
fn test_combined_separator_waits_until_frame_reaches_it() {
    let visible =
        compute_combined_separator_visible_rect(100.0, 180.0, 500.0, 100.0, 100.0, 500.0, 60.0);
    assert_eq!(visible, None);
}

#[test]
fn test_combined_separator_clips_to_animated_frame_width() {
    let visible =
        compute_combined_separator_visible_rect(100.0, 180.0, 500.0, 100.0, 100.0, 260.0, 100.0);
    assert_eq!(visible, Some((100.0, 260.0)));
}

#[test]
fn test_combined_separator_reaches_full_width_with_frame() {
    let visible =
        compute_combined_separator_visible_rect(100.0, 180.0, 500.0, 100.0, 100.0, 500.0, 300.0);
    assert_eq!(visible, Some((100.0, 500.0)));
}

#[test]
fn test_combined_diagnostic_background_does_not_cover_moving_top_border() {
    let (_frame_x, frame_y, _frame_w, frame_h) =
        compute_combined_popup_frame(220.0, 520.0, 100.0, 100.0, 500.0, 300.0, 0.65, true);
    assert!(frame_y > 100.0);
    assert!((frame_y + frame_h - 400.0).abs() < 0.001);

    let anim_scissor = compute_animated_scissor(220.0, 520.0, 100.0, 100.0, 500.0, 300.0, 0.65);
    let (_bg_x, bg_y, _bg_w, bg_h) =
        compute_hover_scissor_rect(anim_scissor, 100.0, 100.0, 500.0, 180.0, Some(frame_y));
    assert_eq!(bg_y, frame_y);
    assert!(bg_h > 0.0);
}

#[test]
fn test_hover_frame_content_rect_insets_bottom_border() {
    let content = compute_hover_frame_content_rect(100.0, 100.0, 500.0, 300.0, 1.0);
    assert_eq!(content, (101.0, 101.0, 498.0, 298.0));
    assert_eq!(content.1 + content.3, 399.0);
}

#[test]
fn test_hover_type_content_scissor_does_not_cover_bottom_border() {
    let content = compute_hover_frame_content_rect(100.0, 100.0, 500.0, 300.0, 1.0);
    let scissor = compute_hover_scissor_rect(
        (96.0, 96.0, 508.0, 308.0),
        content.0,
        content.1,
        content.2,
        content.3,
        None,
    );
    assert_eq!(scissor, content);
    assert_eq!(scissor.1 + scissor.3, 399.0);
}

#[test]
fn test_animated_popup_frame_near_done_snap_is_invisible_with_smooth_progress() {
    let (_frame_x, _frame_y, frame_w, frame_h) =
        compute_animated_popup_frame(10.0, 20.0, 100.0, 100.0, 500.0, 300.0, 0.9995);
    assert!((500.0 - frame_w) < 0.001);
    assert!((300.0 - frame_h) < 0.001);
}

#[test]
fn test_hover_scrollbar_alpha_waits_until_popup_is_nearly_open() {
    assert_eq!(compute_hover_scrollbar_alpha(0.0), 0.0);
    assert_eq!(compute_hover_scrollbar_alpha(0.88), 0.0);
}

#[test]
fn test_hover_scrollbar_alpha_fades_smoothly_near_end() {
    let mid = compute_hover_scrollbar_alpha(0.94);
    assert!((mid - 0.5).abs() < 0.001);
    assert!(compute_hover_scrollbar_alpha(0.91) < mid);
    assert!(compute_hover_scrollbar_alpha(0.97) > mid);
}

#[test]
fn test_hover_scrollbar_alpha_is_full_at_end() {
    assert_eq!(compute_hover_scrollbar_alpha(1.0), 1.0);
    assert_eq!(compute_hover_scrollbar_alpha(1.5), 1.0);
}

#[test]
fn test_attached_hover_content_scissor_uses_shared_combined_animation() {
    let (sc_x, sc_y, sc_w, sc_h) = compute_hover_content_scissor(
        10.0,
        20.0,
        100.0,
        100.0,
        50.0,
        50.0,
        0.25,
        Some((90.0, 70.0, 60.0, 30.0)),
        0.5,
    );
    assert_eq!(sc_x, 90.0);
    assert_eq!(sc_y, 70.0);
    assert_eq!(sc_w, 30.0);
    assert!((sc_h - 80.0 * smooth_hover_height_progress(0.5)).abs() < 0.001);
}

#[test]
fn test_attached_hover_content_scissor_above_cursor_expands_bottom_to_top() {
    let (sc_x, sc_y, sc_w, sc_h) = compute_hover_content_scissor(
        200.0,
        300.0,
        100.0,
        100.0,
        50.0,
        50.0,
        0.25,
        Some((90.0, 70.0, 60.0, 30.0)),
        0.5,
    );
    assert!((sc_x + sc_w - 150.0).abs() < 0.001);
    assert!((sc_y + sc_h - 150.0).abs() < 0.001);
    assert!(sc_y > 70.0);
    assert!(sc_h > 0.0);
}

#[test]
fn test_detached_hover_content_scissor_keeps_popup_animation() {
    let direct = compute_animated_popup_frame(10.0, 20.0, 100.0, 100.0, 50.0, 50.0, 0.25);
    let content =
        compute_hover_content_scissor(10.0, 20.0, 100.0, 100.0, 50.0, 50.0, 0.25, None, 1.0);
    assert_eq!(content, direct);
}

#[test]
fn test_hover_content_scissor_stays_inside_animated_frame() {
    let frame = compute_animated_popup_frame(10.0, 20.0, 100.0, 100.0, 500.0, 300.0, 0.25);
    let content =
        compute_hover_content_scissor(10.0, 20.0, 100.0, 100.0, 500.0, 300.0, 0.25, None, 1.0);
    assert_eq!(content, frame);

    let old_scissor = compute_animated_scissor(10.0, 20.0, 100.0, 100.0, 500.0, 300.0, 0.25);
    assert!(old_scissor.0 < frame.0);
    assert!(old_scissor.2 > frame.2);
}
