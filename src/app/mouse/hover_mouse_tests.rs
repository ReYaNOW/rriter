#[path = "hover_bridge_tests.rs"]
mod hover_bridge_tests;
#[path = "hover_diagnostic_range_tests.rs"]
mod hover_diagnostic_range_tests;
#[path = "hover_transition_tests.rs"]
mod hover_transition_tests;
#[path = "hover_visibility_tests.rs"]
mod hover_visibility_tests;

use super::*;

fn sql_warning(
    start_line: u32,
    start_col: u32,
    end_col: u32,
    code: &'static str,
    message: &'static str,
) -> crate::lsp::Diagnostic {
    crate::lsp::Diagnostic {
        start_line,
        start_col,
        end_line: start_line,
        end_col,
        severity: crate::lsp::DiagSeverity::Warning,
        code: Some(std::sync::Arc::<str>::from(code)),
        code_href: None,
        message: std::sync::Arc::<str>::from(message),
        source: Some(std::sync::Arc::<str>::from("RRiter SQL")),
        quickfixes: Box::new([]),
        tags: Box::new([]),
    }
}

#[test]
fn sql_diagnostic_only_hover_does_not_wait_for_type_popup() {
    let mut editor = crate::editor::Editor::new(128);
    editor.insert_str("SELECT *\nFROM \"public\".\"car__model\"\nLIMIT 100;\n");
    let limit = editor
        .get_full_text()
        .find("LIMIT")
        .expect("test SQL must contain LIMIT");
    let mut state = HoverState::default();
    state.set_database_query_hover_context(Some(11));

    assert_eq!(
        update_editor_hover_state_for_cursor(
            &mut state,
            &editor,
            limit,
            Some(limit),
            true,
            false,
            false,
            false,
        ),
        Some(false)
    );
    assert_eq!(state.byte_offset, Some(limit));
    assert!(state.request_id.is_none());
    assert!(state.pending_popup.is_none());
    assert_eq!(
        state.record_hovered_diagnostic((1, 40.0, 60.0, 80.0, 90.0), Some(limit)),
        None
    );
    assert_eq!(state.effective_hovered_diag_type_target(Some(limit)), None);

    let (show_error, show_type, show_combined) = compute_hover_visibility_from_matches(
        true, true, false, false, false, false, false, false,
    );
    assert!(show_error);
    assert!(!show_type);
    assert!(!show_combined);
}

#[test]
fn switching_between_file_and_sql_hover_sources_clears_stale_popup() {
    let mut state = HoverState::default();
    state.byte_offset = Some(7);
    state.hovered_diag_type_target = Some(7);
    state.hovered_diags_cache.push((0, 10.0, 20.0, 40.0, 30.0));
    state.diag_rect = Some((10.0, 40.0, 200.0, 80.0, 20.0, 40.0, 30.0));
    state.popup = Some(HoverPopup {
        text: "type".to_string(),
        spans: Vec::new(),
        line_kinds: Vec::new(),
        inline_code_ranges: Vec::new(),
        byte_offset: 7,
        anchor_x: 20.0,
        anchor_y: 40.0,
        offset_x: None,
        offset_y: None,
        anim_progress: 1.0,
        scroll: crate::scroll::ScrollState::new(15.0),
        layout_cache: None,
    });

    state.set_database_query_hover_context(Some(11));

    assert!(!state.diagnostic_type_hover_enabled);
    assert!(state.popup.is_none());
    assert!(state.diag_rect.is_none());
    assert!(state.hovered_diags_cache.is_empty());
    assert!(state.byte_offset.is_none());
}

#[test]
fn switching_sql_console_context_clears_stale_hover() {
    let mut state = HoverState::default();
    state.set_database_query_hover_context(Some(11));
    state.byte_offset = Some(7);
    state.hovered_diags_cache.push((0, 10.0, 20.0, 40.0, 30.0));
    state.diag_rect = Some((10.0, 40.0, 200.0, 80.0, 20.0, 40.0, 30.0));

    state.set_database_query_hover_context(Some(12));

    assert_eq!(state.database_query_hover_context, Some(12));
    assert!(!state.diagnostic_type_hover_enabled);
    assert!(state.diag_rect.is_none());
    assert!(state.hovered_diags_cache.is_empty());
    assert!(state.byte_offset.is_none());
}

#[test]
fn sql_hover_uses_same_fractional_scroll_rounding_as_renderer() {
    let line_height = 24.0f32;
    let baseline_offset = 19.0f32;
    let editor_top_inset = crate::render_view::editor_content_top_inset(false, true, true, 1.0);
    let scroll_y = 37.6f32;
    let render_scroll_y = scroll_y.round() - editor_top_inset;
    let visual_line_top = line_height * 2.0;
    let text_top_bias = (baseline_offset - line_height * 0.5).clamp(0.0, line_height * 0.5);
    let pointer_y = visual_line_top - render_scroll_y + text_top_bias + line_height * 0.5;

    let content_y = hover_screen_y_to_content_y(
        pointer_y,
        render_scroll_y,
        line_height,
        baseline_offset,
    )
    .expect("pointer must map back into the rendered SQL line");

    assert!(hover_content_y_in_line_hitbox(
        content_y,
        visual_line_top,
        line_height,
    ));
    assert_eq!(content_y, visual_line_top + line_height * 0.5);
}

#[test]
fn sql_warning_pointer_selection_respects_horizontal_scroll_and_ranges() {
    let sql = "SELECT *\nFROM \"public\".\"car__model\"\nLIMIT 100;\n";
    let mut editor = crate::editor::Editor::new(128);
    editor.insert_str(sql);
    let diagnostics = [
        sql_warning(0, 7, 8, "SQL119", "Не используйте SELECT *"),
        sql_warning(2, 0, 9, "SQL117", "LIMIT без ORDER BY"),
    ];
    let left_padding = 120.0;
    let scroll_x = 16.0;
    let char_width = 10.0;
    let pointer_for = |col: usize| {
        left_padding + col as f32 * char_width - scroll_x + char_width * 0.5
    };
    let selected_code = |line: usize, pointer_x: f32| {
        diagnostics.iter().find_map(|diag| {
            if diag.start_line as usize != line {
                return None;
            }
            let (start, end) = diagnostic_visual_byte_range_on_line(
                &editor,
                line,
                diag.start_col,
                diag.end_col,
            )?;
            let line_start = editor.line_offsets[line];
            let start_col = start.saturating_sub(line_start);
            let end_col = end.saturating_sub(line_start);
            let x = pointer_x - left_padding + scroll_x;
            (x >= start_col as f32 * char_width && x <= end_col as f32 * char_width)
                .then(|| diag.code.as_deref().unwrap_or(""))
        })
    };

    assert_eq!(selected_code(0, pointer_for(7)), Some("SQL119"));
    assert_eq!(selected_code(2, pointer_for(2)), Some("SQL117"));
    assert_eq!(selected_code(1, pointer_for(7)), None);
}

#[test]
fn moving_between_sql_warnings_restarts_diagnostic_timer_for_new_message() {
    let mut state = HoverState::default();
    state.set_database_query_hover_context(Some(11));
    state.record_hovered_diagnostic((0, 10.0, 20.0, 40.0, 18.0), Some(7));
    assert!(!state.advance_diagnostic_hover_timer(Some(0), false, false, 0.21));
    assert!(state.advance_diagnostic_hover_timer(Some(0), false, false, 0.21));

    state.hovered_diags_cache.clear();
    state.record_hovered_diagnostic((1, 10.0, 60.0, 80.0, 90.0), Some(45));
    assert!(!state.advance_diagnostic_hover_timer(Some(1), false, false, 0.01));
    assert_eq!(state.diag_hover_timer_idx, Some(1));
}

#[test]
fn database_query_result_widgets_block_editor_hover_after_resize() {
    use crate::ui_system::{UiId, UiRegistry};

    let mut ui = UiRegistry::new();
    ui.register_rect(
        UiId::DatabaseQueryResultResize,
        100.0,
        96.0,
        600.0,
        8.0,
        -1.0,
        -1.0,
    );
    ui.register_rect(
        UiId::DatabaseQueryResultBody,
        100.0,
        140.0,
        600.0,
        300.0,
        -1.0,
        -1.0,
    );

    assert!(HoverState::database_query_results_block_hover_at(
        &ui, 200.0, 100.0,
    ));
    assert!(HoverState::database_query_results_block_hover_at(
        &ui, 200.0, 120.0,
    ));
    assert!(HoverState::database_query_results_block_hover_at(
        &ui, 200.0, 200.0,
    ));
    assert!(!HoverState::database_query_results_block_hover_at(
        &ui, 200.0, 90.0,
    ));
}
