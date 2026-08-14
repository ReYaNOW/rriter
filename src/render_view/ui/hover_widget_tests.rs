use super::*;

#[test]
fn diagnostic_copy_includes_code_and_normalized_message() {
    let diagnostic = crate::lsp::Diagnostic {
        start_line: 0,
        start_col: 0,
        end_line: 0,
        end_col: 1,
        severity: crate::lsp::DiagSeverity::Error,
        code: Some(std::sync::Arc::<str>::from("SQL001")),
        code_href: None,
        message: std::sync::Arc::<str>::from("Ошибка\\nПодробность"),
        source: Some(std::sync::Arc::<str>::from("RRiter SQL")),
        quickfixes: Box::new([]),
        tags: Box::new([]),
    };

    assert_eq!(
        diagnostic_copy_text(&diagnostic),
        "SQL001: Ошибка\nПодробность"
    );
}

#[test]
fn module_header_wrap_does_not_split_marker_from_path() {
    assert!(!hover_wrap_space_can_break("[[MODULE]] ".chars().count()));
    assert!(hover_wrap_space_can_break(
        "[[MODULE]] car_wash.long.path ".chars().count()
    ));
}

#[test]
fn test_valid_diagnostic_popup_cache_drops_stale_indices() {
    let diagnostic = crate::lsp::Diagnostic {
        start_line: 0,
        start_col: 0,
        end_line: 0,
        end_col: 1,
        severity: crate::lsp::DiagSeverity::Warning,
        code: Some(std::sync::Arc::<str>::from("SQL119")),
        code_href: None,
        message: std::sync::Arc::<str>::from("Не используйте SELECT *"),
        source: Some(std::sync::Arc::<str>::from("RRiter SQL")),
        quickfixes: Box::new([]),
        tags: Box::new([]),
    };
    let diagnostics = [&diagnostic];
    let cache = vec![(0, 1.0, 2.0, 3.0, 4.0), (3, 5.0, 6.0, 7.0, 8.0)];
    assert_eq!(
        valid_diagnostic_popup_cache(cache, &diagnostics),
        vec![(0, 1.0, 2.0, 3.0, 4.0)]
    );
}

#[test]
fn diagnostic_popup_cache_keeps_unique_messages_and_deduplicates_exact_copies() {
    let make_diagnostic = |code: &'static str, message: &'static str| crate::lsp::Diagnostic {
        start_line: 0,
        start_col: 7,
        end_line: 0,
        end_col: 8,
        severity: crate::lsp::DiagSeverity::Warning,
        code: Some(std::sync::Arc::<str>::from(code)),
        code_href: None,
        message: std::sync::Arc::<str>::from(message),
        source: Some(std::sync::Arc::<str>::from("RRiter SQL")),
        quickfixes: Box::new([]),
        tags: Box::new([]),
    };
    let select_star = make_diagnostic("SQL119", "Не используйте SELECT *");
    let duplicate_select_star = make_diagnostic("SQL119", "Не используйте SELECT *");
    let second_message = make_diagnostic("SQL999", "Другая диагностика на том же диапазоне");
    let diagnostics = [&select_star, &duplicate_select_star, &second_message];
    let cache = vec![
        (0, 10.0, 20.0, 40.0, 18.0),
        (1, 10.0, 20.0, 40.0, 18.0),
        (2, 10.0, 20.0, 40.0, 18.0),
    ];

    assert_eq!(
        valid_diagnostic_popup_cache(cache, &diagnostics),
        vec![
            (0, 10.0, 20.0, 40.0, 18.0),
            (2, 10.0, 20.0, 40.0, 18.0),
        ]
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
            byte_len: 1,
        });
        chars.push(super::DiagChar {
            x: 30.0,
            y: 20.0,
            w: 8.0,
            h: 16.0,
            byte_offset: 4,
            byte_len: 1,
        });
        chars.push(super::DiagChar {
            x: 10.0,
            y: 60.0,
            w: 8.0,
            h: 16.0,
            byte_offset: 20,
            byte_len: 1,
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
    let expected_w = 58.0 * smooth_hover_width_progress(0.5);
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
    let expected_w = 58.0 * smooth_hover_width_progress(0.5);
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
    let expected_w = 50.0 * smooth_hover_width_progress(0.5);
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
    let expected_w = 50.0 * smooth_hover_width_progress(0.5);
    let expected_h = 50.0 * smooth_hover_height_progress(0.5);
    assert!((frame_x + frame_w - 150.0).abs() < 0.001);
    assert!((frame_y + frame_h - 150.0).abs() < 0.001);
    assert!((frame_w - expected_w).abs() < 0.001);
    assert!((frame_h - expected_h).abs() < 0.001);
}

#[test]
fn test_pixel_frame_keeps_fractional_bottom_edge_fixed_when_popup_above_cursor() {
    let target = (100.2, 100.3, 500.0, 300.0);
    let anchor = (220.0, 520.0);
    let near_done = (100.2, 100.6, 500.0, 299.7);
    let final_frame = (100.2, 100.3, 500.0, 300.0);

    let (_, y1, _, h1) = pixel_stable_hover_popup_frame(near_done, target, anchor);
    let (_, y2, _, h2) = pixel_stable_hover_popup_frame(final_frame, target, anchor);

    assert_eq!(y1 + h1, 400.0);
    assert_eq!(y2 + h2, 400.0);
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
fn test_small_popup_pixel_width_settles_before_final_frames_when_expanding_right() {
    let anchor = (10.0, 20.0);
    for target_w in [120.0, 180.0, 240.0] {
        let target = (100.2, 100.3, target_w, 80.0);
        let near_done = compute_animated_popup_frame(
            anchor.0, anchor.1, target.0, target.1, target.2, target.3, 0.94,
        );
        let final_frame = compute_animated_popup_frame(
            anchor.0, anchor.1, target.0, target.1, target.2, target.3, 1.0,
        );

        let near_done = pixel_stable_hover_popup_frame(near_done, target, anchor);
        let final_frame = pixel_stable_hover_popup_frame(final_frame, target, anchor);

        assert_eq!(near_done, final_frame, "target_w={target_w}");
        assert_eq!(near_done.0, target.0.round(), "target_w={target_w}");
    }
}

#[test]
fn test_small_popup_pixel_width_settles_before_final_frames_when_expanding_left() {
    for target_w in [120.0, 180.0, 240.0] {
        let target = (100.2, 100.3, target_w, 80.0);
        let anchor = (target.0 + target.2 + 100.0, 20.0);
        let near_done = compute_animated_popup_frame(
            anchor.0, anchor.1, target.0, target.1, target.2, target.3, 0.94,
        );
        let final_frame = compute_animated_popup_frame(
            anchor.0, anchor.1, target.0, target.1, target.2, target.3, 1.0,
        );

        let near_done = pixel_stable_hover_popup_frame(near_done, target, anchor);
        let final_frame = pixel_stable_hover_popup_frame(final_frame, target, anchor);

        assert_eq!(near_done, final_frame, "target_w={target_w}");
        assert_eq!(
            near_done.0 + near_done.2,
            (target.0 + target.2).round(),
            "target_w={target_w}"
        );
    }
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
        compute_hover_scissor_rect(anim_scissor, 100.0, 100.0, 500.0, 180.0, Some(frame_y), None);
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
        None,
    );
    assert_eq!(scissor, content);
    assert_eq!(scissor.1 + scissor.3, 399.0);
}

#[test]
fn test_hover_type_content_scissor_respects_parent_code_clip() {
    let scissor = compute_hover_scissor_rect(
        (90.0, 90.0, 520.0, 320.0),
        100.0,
        100.0,
        500.0,
        300.0,
        None,
        Some((80.0, 160.0, 420.0, 100.0)),
    );
    assert_eq!(scissor, (100.0, 160.0, 400.0, 100.0));
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
    assert!((sc_w - 60.0 * smooth_hover_width_progress(0.5)).abs() < 0.001);
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

#[test]
fn diagnostic_hit_test_uses_full_utf8_character_length() {
    super::DIAG_CHARS.with(|chars| {
        let mut chars = chars.borrow_mut();
        chars.clear();
        chars.push(super::DiagChar {
            x: 10.0,
            y: 20.0,
            w: 8.0,
            h: 16.0,
            byte_offset: 0,
            byte_len: "я".len(),
        });
    });

    assert_eq!(super::diag_popup_byte_at(20.0, 25.0), "я".len());
    super::DIAG_CHARS.with(|chars| chars.borrow_mut().clear());
}

#[test]
fn diagnostic_wrapping_keeps_original_offsets_and_ignores_visual_indent() {
    let message = "  alpha beta";
    let lines = super::diagnostic_message_lines(message, &[], 0, 8.0, 2.0, false, |_| 1.0);

    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1][0].ch, ' ');
    assert!(!lines[1][0].selectable);
    assert_eq!(lines[1][1].ch, ' ');
    assert!(!lines[1][1].selectable);
    let beta = lines[1].iter().find(|item| item.ch == 'b').unwrap();
    assert!(beta.selectable);
    assert_eq!(beta.byte_offset, message.find("beta").unwrap());
}

#[test]
fn hover_y_position_clamps_fallback_to_viewport_top_range() {
    let y = compute_hover_y_position(50.0, 20.0, 100.0, 100.0, 1.0);
    assert_eq!(y, 40.0);
}

fn diagnostic_visual_lines(
    message: &str,
    base_offset: usize,
    balanced: bool,
) -> Vec<Vec<DiagnosticVisualChar>> {
    diagnostic_message_lines(
        message,
        &[],
        base_offset,
        1_000.0,
        40.0,
        balanced,
        |_| 1.0,
    )
}

fn visual_line_text(line: &[DiagnosticVisualChar]) -> String {
    line.iter().map(|item| item.ch).collect()
}

fn test_diagnostic(source: &str, message: &str) -> crate::lsp::Diagnostic {
    crate::lsp::Diagnostic {
        start_line: 0,
        start_col: 0,
        end_line: 0,
        end_col: 1,
        severity: crate::lsp::DiagSeverity::Warning,
        code: Some(std::sync::Arc::<str>::from("SQL119")),
        code_href: None,
        message: std::sync::Arc::<str>::from(message),
        source: Some(std::sync::Arc::<str>::from(source)),
        quickfixes: Box::new([]),
        tags: Box::new([]),
    }
}

#[test]
fn five_word_sql_diagnostic_stays_on_one_visual_line() {
    let lines = diagnostic_visual_lines("one two three four five", 0, true);
    assert_eq!(lines.len(), 1);
    assert_eq!(visual_line_text(&lines[0]), "one two three four five");
}

#[test]
fn six_word_sql_diagnostic_balances_into_two_visual_lines() {
    let lines = diagnostic_visual_lines("one two three four five six", 0, true);
    assert_eq!(lines.len(), 2);
    assert_eq!(visual_line_text(&lines[0]), "one two three");
    assert_eq!(visual_line_text(&lines[1]), "four five six");
}

#[test]
fn long_sql_warning_never_creates_third_visual_line() {
    let message = "SELECT star should include explicit ORDER BY for stable result ordering";
    let lines = diagnostic_message_lines(message, &[], 0, 4.0, 1.0, true, |_| 1.0);
    assert_eq!(lines.len(), 2);
    assert_eq!(
        lines.iter().map(|line| visual_line_text(line)).collect::<Vec<_>>(),
        vec![
            "SELECT star should include explicit".to_string(),
            "ORDER BY for stable result ordering".to_string(),
        ]
    );
}

#[test]
fn balanced_sql_diagnostic_keeps_unicode_words_intact() {
    let message = "Ошибка запроса требует явного порядка строк результата";
    let lines = diagnostic_visual_lines(message, 0, true);
    assert_eq!(lines.len(), 2);
    let rendered = lines
        .iter()
        .map(|line| visual_line_text(line))
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(rendered, message);
    for line in lines {
        assert!(std::str::from_utf8(visual_line_text(&line).as_bytes()).is_ok());
    }
}

#[test]
fn balanced_sql_diagnostic_copy_keeps_original_text_without_visual_newline() {
    let message = "one two three four five six";
    let diagnostic = test_diagnostic("RRiter SQL", message);
    let lines = diagnostic_visual_lines(message, 0, true);
    assert_eq!(lines.len(), 2);
    assert_eq!(diagnostic_copy_text(&diagnostic), format!("SQL119: {message}"));
    assert!(!diagnostic_copy_text(&diagnostic).contains('\n'));
}

#[test]
fn balanced_sql_diagnostic_lines_keep_original_byte_offsets() {
    let message = "один два три четыре пять шесть";
    let base_offset = 17;
    let lines = diagnostic_visual_lines(message, base_offset, true);
    assert_eq!(lines.len(), 2);

    let first = lines[0].first().unwrap();
    let second = lines[1].first().unwrap();
    let second_text = visual_line_text(&lines[1]);
    assert_eq!(first.byte_offset, base_offset);
    assert_eq!(second.byte_offset, base_offset + message.find(&second_text).unwrap());
    assert_eq!(second.byte_len, second.ch.len_utf8());
}

#[test]
fn balanced_sql_diagnostics_keep_independent_base_offsets() {
    let first_message = "one two three four five six";
    let second_message = "alpha beta gamma delta epsilon zeta";
    let first = diagnostic_visual_lines(first_message, 0, true);
    let second_base = first_message.len() + 2;
    let second = diagnostic_visual_lines(second_message, second_base, true);

    assert_eq!(first.len(), 2);
    assert_eq!(second.len(), 2);
    assert!(first.iter().flatten().all(|item| item.byte_offset < second_base));
    assert!(second
        .iter()
        .flatten()
        .all(|item| item.byte_offset >= second_base));
}

#[test]
fn balanced_sql_diagnostic_adds_exactly_one_line_height() {
    let line_h = 22.0;
    let five = diagnostic_visual_lines("one two three four five", 0, true);
    let six = diagnostic_visual_lines("one two three four five six", 0, true);
    assert_eq!((six.len() as f32 - five.len() as f32) * line_h, line_h);
}

#[test]
fn diagnostic_only_sql_hover_uses_balanced_message_policy() {
    let diagnostic = test_diagnostic(
        "RRiter SQL",
        "query should include explicit order by for stable results",
    );
    assert!(should_balance_diagnostic_message(&diagnostic));
    let lines = diagnostic_message_lines(
        &diagnostic.message,
        &[],
        0,
        8.0,
        2.0,
        should_balance_diagnostic_message(&diagnostic),
        |_| 1.0,
    );
    assert_eq!(lines.len(), 2);
}

#[test]
fn short_python_diagnostic_keeps_existing_layout_policy() {
    let diagnostic = test_diagnostic("Pyright", "short python diagnostic");
    assert!(!should_balance_diagnostic_message(&diagnostic));
    let lines = diagnostic_message_lines(
        &diagnostic.message,
        &[],
        0,
        1_000.0,
        40.0,
        should_balance_diagnostic_message(&diagnostic),
        |_| 1.0,
    );
    assert_eq!(lines.len(), 1);
    assert_eq!(visual_line_text(&lines[0]), "short python diagnostic");
}

#[test]
fn balanced_diagnostic_does_not_break_inside_quoted_identifier() {
    let message = "Use quoted \"very long identifier\" in stable query ordering";
    let lines = diagnostic_visual_lines(message, 0, true);
    assert_eq!(lines.len(), 2);
    assert!(lines
        .iter()
        .any(|line| visual_line_text(line).contains("\"very long identifier\"")));
}

#[test]
fn hover_surface_fill_and_border_share_one_snapped_outer_rect() {
    let surface = hover_surface_layout((100.25, 50.75, 300.4, 120.6), 7.5, 2.5);
    assert_eq!(surface.outer_rect, (100.0, 51.0, 300.0, 121.0));
    assert_eq!(surface.clip_rect, surface.inner_rect);
}

#[test]
fn hover_surface_inner_radius_fits_after_border_inset() {
    let surface = hover_surface_layout((10.0, 20.0, 80.0, 40.0), 12.0, 4.0);
    assert!(surface.inner_radius <= surface.inner_rect.2 * 0.5);
    assert!(surface.inner_radius <= surface.inner_rect.3 * 0.5);
    assert_eq!(surface.inner_radius, surface.outer_radius - surface.border_width);
}

#[test]
fn tiny_hover_surface_has_non_negative_geometry() {
    let surface = hover_surface_layout((1.4, 2.6, 1.0, 1.0), 20.0, 8.0);
    assert!(surface.outer_radius >= 0.0);
    assert!(surface.inner_radius >= 0.0);
    assert!(surface.inner_rect.2 >= 0.0);
    assert!(surface.inner_rect.3 >= 0.0);
    assert!(surface.border_width <= 0.5);
}

#[test]
fn fractional_dpi_surface_reuses_identical_snapped_bounds() {
    let python = hover_surface_layout((40.625, 80.375, 510.625, 220.375), 7.5, 2.5);
    let sql = hover_surface_layout((40.625, 80.375, 510.625, 220.375), 7.5, 2.5);
    assert_eq!(python, sql);
    assert_eq!(python.outer_rect, (41.0, 80.0, 511.0, 220.0));
}

#[test]
fn hover_surface_inner_rect_covers_all_four_inner_corners() {
    let surface = hover_surface_layout((100.0, 100.0, 200.0, 80.0), 8.0, 2.0);
    let (x, y, w, h) = surface.inner_rect;
    let corners = [(x, y), (x + w, y), (x, y + h), (x + w, y + h)];
    for (corner_x, corner_y) in corners {
        assert!(corner_x >= surface.outer_rect.0);
        assert!(corner_x <= surface.outer_rect.0 + surface.outer_rect.2);
        assert!(corner_y >= surface.outer_rect.1);
        assert!(corner_y <= surface.outer_rect.1 + surface.outer_rect.3);
    }
}

#[test]
fn hover_scrollbar_does_not_change_surface_geometry() {
    let without_scrollbar = hover_surface_layout((20.0, 30.0, 400.0, 160.0), 6.0, 2.0);
    let with_scrollbar = hover_surface_layout((20.0, 30.0, 400.0, 160.0), 6.0, 2.0);
    assert_eq!(without_scrollbar, with_scrollbar);
}

#[test]
fn hover_bridge_hitbox_does_not_modify_surface_background() {
    let surface = hover_surface_layout((100.0, 100.0, 300.0, 120.0), 6.0, 2.0);
    let before = surface;
    let _ = crate::app::mouse::is_in_hover_popup_or_bridge(
        250.0,
        160.0,
        surface.outer_rect,
        220.0,
        260.0,
        220.0,
        240.0,
        800.0,
        1.0,
    );
    assert_eq!(surface, before);
}
