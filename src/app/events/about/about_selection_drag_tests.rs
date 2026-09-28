    use super::*;
    #[test]
    fn tab_drag_autoscroll_keeps_inside_edge_band_and_outside_window_distance() {
        assert_eq!(drag_autoscroll_delta(50.0, 100.0, 500.0, 40.0), -50.0);
        assert_eq!(drag_autoscroll_delta(120.0, 100.0, 500.0, 40.0), -20.0);
        assert_eq!(drag_autoscroll_delta(480.0, 100.0, 500.0, 40.0), 20.0);
        assert_eq!(drag_autoscroll_delta(540.0, 100.0, 500.0, 40.0), 40.0);
        assert_eq!(drag_autoscroll_delta(250.0, 100.0, 500.0, 40.0), 0.0);
        assert!(drag_autoscroll_speed(30.0, false) >= DRAG_AUTOSCROLL_MIN_SPEED);
        assert!(drag_autoscroll_speed(-30.0, true) > drag_autoscroll_speed(30.0, false));
    }

    #[test]
    fn selection_drag_autoscroll_starts_only_outside_viewport() {
        for pos in [100.0, 101.0, 120.0, 480.0, 499.0, 500.0] {
            assert_eq!(selection_drag_autoscroll_delta(pos, 100.0, 500.0), 0.0);
        }

        assert_eq!(selection_drag_autoscroll_delta(99.0, 100.0, 500.0), -1.0);
        assert_eq!(selection_drag_autoscroll_delta(80.0, 100.0, 500.0), -20.0);
        assert_eq!(selection_drag_autoscroll_delta(501.0, 100.0, 500.0), 1.0);
        assert_eq!(selection_drag_autoscroll_delta(540.0, 100.0, 500.0), 40.0);
    }

    #[test]
    fn selection_drag_autoscroll_preserves_far_outside_distance() {
        assert_eq!(selection_drag_autoscroll_delta(-200.0, 100.0, 500.0), -300.0);
        assert_eq!(selection_drag_autoscroll_delta(1400.0, 100.0, 500.0), 900.0);
    }

    #[test]
    fn cursor_leave_projection_crosses_nearest_window_edge() {
        assert_eq!(
            project_cursor_outside_window_on_leave(400.0, 599.0, 800.0, 600.0),
            (400.0, 601.0)
        );
        assert_eq!(
            project_cursor_outside_window_on_leave(400.0, 1.0, 800.0, 600.0),
            (400.0, -1.0)
        );
        assert_eq!(
            project_cursor_outside_window_on_leave(1.0, 300.0, 800.0, 600.0),
            (-1.0, 300.0)
        );
        assert_eq!(
            project_cursor_outside_window_on_leave(799.0, 300.0, 800.0, 600.0),
            (801.0, 300.0)
        );
        assert_eq!(
            project_cursor_outside_window_on_leave(0.0, 0.0, 800.0, 600.0),
            (-1.0, 0.0)
        );
        assert_eq!(
            project_cursor_outside_window_on_leave(400.0, 601.0, 800.0, 600.0),
            (400.0, 601.0)
        );

        let (_, left_exit_y) =
            project_cursor_outside_window_on_leave(1.0, 300.0, 800.0, 600.0);
        let (_, right_exit_y) =
            project_cursor_outside_window_on_leave(799.0, 300.0, 800.0, 600.0);
        assert_eq!(selection_drag_autoscroll_delta(left_exit_y, 100.0, 500.0), 0.0);
        assert_eq!(selection_drag_autoscroll_delta(right_exit_y, 100.0, 500.0), 0.0);
        assert_eq!(
            markdown_read_selection_autoscroll_delta(left_exit_y, 100.0, 500.0, 24.0),
            0.0
        );
        assert_eq!(
            markdown_read_selection_autoscroll_delta(right_exit_y, 100.0, 500.0, 24.0),
            0.0
        );
    }

    #[test]
    fn markdown_reader_drag_route_precedes_generic_editor_scrollbar_drag_branch() {
        let dispatcher = include_str!("../../mouse/cursor.rs");
        let scroll_drags = include_str!("../../mouse/cursor/cursor_scroll_drags.rs");
        let drag_updates = include_str!("../../mouse/cursor/cursor_drag_updates.rs");
        assert!(
            scroll_drags.contains("self.drag_markdown_read_scrollbar_to(py)"),
            "Reader scrollbar drag route"
        );
        assert!(
            drag_updates.contains("else if self.scroll_y.is_dragging"),
            "generic Editor/minimap drag route"
        );
        let reader_phase = dispatcher
            .find("self.cursor_moved_scroll_drags(")
            .expect("Reader scrollbar drag phase");
        let generic_phase = dispatcher
            .find("self.cursor_moved_drag_updates(")
            .expect("generic Editor/minimap drag phase");
        assert!(reader_phase < generic_phase);
    }

    #[test]
    fn cursor_leave_projection_is_selection_only() {
        use crate::ui_system::UiId;

        assert!(selection_drag_active_on_cursor_leave(
            true,
            false,
            false,
            Some(UiId::EditorTextBody),
        ));
        assert!(selection_drag_active_on_cursor_leave(
            true,
            false,
            true,
            Some(UiId::TerminalBody),
        ));

        for (is_terminal, id) in [
            (false, UiId::EditorScrollbarY),
            (false, UiId::EditorMinimap),
            (true, UiId::TerminalTab(0)),
        ] {
            assert!(!selection_drag_active_on_cursor_leave(
                true,
                false,
                is_terminal,
                Some(id),
            ));
        }
        assert!(!selection_drag_active_on_cursor_leave(
            false,
            false,
            false,
            Some(UiId::EditorTextBody),
        ));
        assert!(!selection_drag_active_on_cursor_leave(
            true,
            true,
            false,
            Some(UiId::EditorTextBody),
        ));
        assert!(!selection_drag_active_on_cursor_leave(
            true,
            false,
            true,
            Some(UiId::EditorTextBody),
        ));
        assert!(!selection_drag_active_on_cursor_leave(
            true,
            false,
            false,
            Some(UiId::TerminalBody),
        ));
    }

    #[test]
    fn non_ide_editor_bottom_autoscroll_starts_after_native_cursor_leave() {
        let window_w = 800.0;
        let window_h = 600.0;
        let editor_top = 38.0;
        let editor_h = crate::render_view::editor_view_height(
            window_h,
            editor_top,
            0.0,
            false,
            1.0,
        );
        let editor_bottom = editor_top + editor_h;
        assert_eq!(editor_bottom, window_h);

        let last_inside_y = window_h - 1.0;
        assert_eq!(
            selection_drag_autoscroll_delta(last_inside_y, editor_top, editor_bottom),
            0.0
        );

        let (_, projected_y) = project_cursor_outside_window_on_leave(
            window_w * 0.5,
            last_inside_y,
            window_w,
            window_h,
        );
        assert!(projected_y > window_h);
        assert!(selection_drag_autoscroll_delta(projected_y, editor_top, editor_bottom) > 0.0);
    }

    #[test]
    fn selection_autoscroll_uses_registered_body_rects_and_updates_endpoints() {
        // `about_to_wait` delegates the drag autoscroll to its input tick section.
        assert_eq!(
            include_str!("../about.rs")
                .matches("about_to_wait_selection_drag_autoscroll(app, dt)")
                .count(),
            1
        );
        let about = include_str!("about_tick_input_sections.rs");
        let terminal = about
            .split("if app.ide_panel.is_dragging_terminal && app.is_dragging && !app.show_settings")
            .nth(1)
            .unwrap()
            .split("if app.is_dragging && !app.ide_panel.is_dragging_terminal")
            .next()
            .unwrap();
        assert!(terminal.contains("rect_for(crate::ui_system::UiId::TerminalBody)"));
        assert!(terminal.contains(
            "selection_drag_autoscroll_delta(my, term_y, term_y + term_h)"
        ));
        assert!(terminal.contains("term.scroll_y.target ="));
        assert!(terminal.contains("terminal_drag_cell("));
        assert!(terminal.contains("grid.selection = Some"));
        assert!(!terminal.contains("DRAG_AUTOSCROLL_EDGE_PX"));
        assert!(!terminal.contains("terminal_body_rect("));

        let editor = about
            .split(
                "if app.is_dragging && !app.ide_panel.is_dragging_terminal && !app.scroll_y.is_dragging",
            )
            .nth(1)
            .unwrap()
            .split("if let Some(w) = app.window.as_ref()")
            .next()
            .unwrap();
        assert!(editor.contains("rect_for(crate::ui_system::UiId::EditorTextBody)"));
        assert!(editor.contains(
            "selection_drag_autoscroll_delta(my, editor_y, editor_y + editor_h)"
        ));
        assert!(editor.contains(
            "selection_drag_autoscroll_delta(mx, editor_x, editor_x + editor_w)"
        ));
        assert!(editor.contains("app.scroll_y.target +="));
        assert!(editor.contains("app.scroll_x.target +="));
        assert!(editor.contains("app.editor.set_cursor_at_pos("));
        assert!(!editor.contains("DRAG_AUTOSCROLL_EDGE_PX"));
        assert!(!editor.contains("drag_autoscroll_editor_bottom("));
    }

    #[test]
    fn selection_drag_lifecycle_projects_cursor_leave_until_release_or_focus_loss() {
        let overlays = include_str!("../../mouse/cursor/cursor_overlays.rs");
        assert!(overlays.contains("if self.modal_dialog_open()"));
        let cursor_move = include_str!("../../mouse/cursor.rs")
            .split("pub fn handle_main_cursor_moved")
            .nth(1)
            .unwrap()
            .split("if self.cursor_moved_modal_overlays(")
            .next()
            .unwrap();
        assert!(cursor_move.contains("renderer.last_mouse_x = px;"));
        assert!(cursor_move.contains("renderer.last_mouse_y = py;"));

        let events = include_str!("../../events.rs");
        assert!(events.contains(
            "WindowEvent::CursorMoved { position, .. } => self.handle_main_cursor_moved(position)"
        ));
        let cursor_left = events
            .split("WindowEvent::CursorLeft { .. } => {")
            .nth(1)
            .unwrap()
            .split("WindowEvent::Ime")
            .next()
            .unwrap();
        assert!(cursor_left.contains("about::selection_drag_active_on_cursor_leave("));
        assert!(cursor_left.contains("about::project_cursor_outside_window_on_leave("));
        assert!(!cursor_left.contains("cancel_pointer_interactions"));
        let focus = events
            .split("WindowEvent::Focused(focused) =>")
            .nth(1)
            .unwrap()
            .split("WindowEvent::Occluded")
            .next()
            .unwrap();
        assert!(focus.contains("self.cancel_pointer_interactions();"));
        assert!(focus.contains("self.render_suspended = false;"));
        assert!(focus.contains("self.render_suspended = true;"));
        let occluded = events
            .split("WindowEvent::Occluded(occluded) =>")
            .nth(1)
            .unwrap()
            .split("WindowEvent::ScaleFactorChanged")
            .next()
            .unwrap();
        assert!(occluded.contains("self.render_suspended = occluded;"));
        assert!(occluded.contains("if !occluded"));

        let input = include_str!("../../mouse/input/mouse_drag_capture.rs");
        let release = input
            .split("// Завершаем DnD и ресайз IDE-панелей")
            .nth(1)
            .unwrap()
            .split("if state == ElementState::Pressed")
            .next()
            .unwrap();
        assert!(release.contains("self.cancel_pointer_interactions();"));
    }
