#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_delta_handles_line_and_pixel_units() {
        assert_eq!(
            wheel_delta(MouseScrollDelta::LineDelta(2.0, -3.0), 10.0, 1.0),
            (-80.0, 120.0)
        );
        assert_eq!(
            wheel_delta(MouseScrollDelta::LineDelta(2.0, -3.0), 10.0, 2.0),
            (-160.0, 240.0)
        );
        assert_eq!(
            wheel_delta(
                MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition::new(12.5, -8.0)),
                10.0,
                5.0,
            ),
            (-62.5, 40.0)
        );
    }

    #[test]
    fn configured_ctrl_multiplier_scales_line_and_pixel_deltas_exactly_once() {
        assert_eq!(
            wheel_delta(MouseScrollDelta::LineDelta(1.0, -1.0), 10.0, 5.0),
            (-200.0, 200.0)
        );
        assert_eq!(
            wheel_delta(
                MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition::new(12.5, -8.0)),
                10.0,
                5.0,
            ),
            (-62.5, 40.0)
        );
    }

    #[test]
    fn ctrl_wheel_multiplier_uses_physical_control_only() {
        use winit::keyboard::ModifiersState;
        assert_eq!(ctrl_wheel_line_multiplier(ModifiersState::empty(), 3.25), 1.0);
        assert_eq!(ctrl_wheel_line_multiplier(ModifiersState::SUPER, 3.25), 1.0);
        assert_eq!(ctrl_wheel_line_multiplier(ModifiersState::CONTROL, 3.25), 3.25);
        assert_eq!(
            ctrl_wheel_line_multiplier(ModifiersState::CONTROL | ModifiersState::SHIFT, 3.25),
            3.25
        );
    }

    #[test]
    fn line_multiplier_is_applied_once_before_representative_routes() {
        let (_, dy) = wheel_delta(MouseScrollDelta::LineDelta(0.0, -1.0), 10.0, 2.5);
        assert_eq!(dy, 100.0);
        let mut editor = crate::scroll::ScrollState::new(15.0);
        editor.scroll_by(dy);
        assert_eq!(editor.target, 100.0);
        let mut nested = crate::scroll::ScrollState::new(15.0);
        scroll_database_dialog_form(&mut nested, dy, 500.0, true);
        assert_eq!(nested.target, 100.0);
    }

    #[test]
    fn point_in_rect_uses_inclusive_edges() {
        let rect = (10.0, 20.0, 30.0, 40.0);
        assert!(crate::ui_system::point_in_rect(10.0, 20.0, rect));
        assert!(crate::ui_system::point_in_rect(40.0, 60.0, rect));
        assert!(crate::ui_system::point_in_rect(25.0, 45.0, rect));
        assert!(!crate::ui_system::point_in_rect(9.9, 45.0, rect));
        assert!(!crate::ui_system::point_in_rect(25.0, 60.1, rect));
    }

    #[test]
    fn panel_scroll_rect_covers_top_and_bottom_layouts() {
        assert_eq!(
            panel_scroll_rect(true, 2.0, 96.0, 240.0, 360.0, 1600.0, 1000.0),
            (96.0, 64.0, 480.0, 516.0)
        );
        assert_eq!(
            panel_scroll_rect(false, 2.0, 96.0, 240.0, 360.0, 1600.0, 1000.0),
            (96.0, 645.0, 1504.0, 295.0)
        );
    }

    #[test]
    fn git_changes_total_height_keeps_disabled_workspaces_scrollable_when_staged() {
        let mut state = crate::app::git_panel::GitPanelState::default();
        state.snapshot = crate::app::git_panel::GitStatusSnapshot {
            workspaces: vec![
                git_workspace_for_wheel_test(0, true),
                git_workspace_for_wheel_test(1, false),
            ],
        };

        assert_eq!(state.staged_workspace_lock(), Some(0));
        assert_eq!(
            git_changes_total_height(&state, 1.0),
            60.0 + crate::render_view::tree_ui::TREE_ROW_H * 2.0
        );
    }

    #[test]
    fn autocomplete_max_scroll_matches_visible_limit() {
        assert_eq!(autocomplete_max_scroll(0, 1.0), 0.0);
        assert_eq!(autocomplete_max_scroll(7, 1.0), 0.0);
        assert_eq!(autocomplete_max_scroll(10, 1.0), 108.0);
        assert_eq!(autocomplete_max_scroll(9, 2.0), 144.0);
    }

    #[test]
    fn scroll_autocomplete_list_clamps_without_closing_state() {
        let mut scroll = crate::scroll::ScrollState::new(15.0);
        scroll_autocomplete_list(&mut scroll, 300.0, 10, 1.0);
        assert_eq!(scroll.target, 108.0);

        scroll_autocomplete_list(&mut scroll, -500.0, 10, 1.0);
        assert_eq!(scroll.target, 0.0);
    }

    #[test]
    fn terminal_tab_strip_wheel_uses_file_tab_speed_and_clamps() {
        let mut scroll = crate::scroll::ScrollState::new(15.0);
        scroll_terminal_tab_strip(&mut scroll, 90.0, 60.0);
        assert_eq!(scroll.anim_speed, 7.0);
        assert_eq!(scroll.target, 60.0);
        assert_eq!(scroll.current, 0.0);

        scroll_terminal_tab_strip(&mut scroll, -200.0, 60.0);
        assert_eq!(scroll.target, 0.0);
    }

    #[test]
    fn database_dialog_wheel_only_moves_inside_scrollable_form_viewport() {
        let mut scroll = crate::scroll::ScrollState::new(15.0);
        scroll_database_dialog_form(&mut scroll, 120.0, 400.0, false);
        assert_eq!(scroll.target, 0.0);

        scroll_database_dialog_form(&mut scroll, 120.0, 400.0, true);
        assert_eq!(scroll.target, 120.0);
        assert_eq!(scroll.current, 0.0);
        assert_eq!(scroll.anim_speed, 7.0);
        assert!(scroll.update(0.016));
        assert!(scroll.current > 0.0 && scroll.current < scroll.target);

        scroll_database_dialog_form(&mut scroll, 500.0, 400.0, true);
        assert_eq!(scroll.target, 400.0);
    }

    fn git_workspace_for_wheel_test(
        workspace_idx: usize,
        staged: bool,
    ) -> crate::app::git_panel::GitWorkspaceStatus {
        let rel_path = format!("src/file_{workspace_idx}.rs");
        crate::app::git_panel::GitWorkspaceStatus {
            workspace_idx,
            root: std::path::PathBuf::from(format!("/repo_{workspace_idx}")),
            repo_root: Some(std::path::PathBuf::from(format!("/repo_{workspace_idx}"))),
            branch_name: None,
            files: vec![crate::app::git_panel::GitFileEntry {
                workspace_idx,
                rel_path: rel_path.clone().into_boxed_str(),
                old_rel_path: None,
                display_path: rel_path.clone().into_boxed_str(),
                depth: 0,
                staged,
                status: crate::app::git_panel::GitFileStatus::Modified,
            }],
            tree: vec![crate::app::git_panel::GitTreeRow {
                name: rel_path.into_boxed_str(),
                path: format!("src/file_{workspace_idx}.rs").into_boxed_str(),
                depth: 0,
                file_idx: Some(0),
                icon_key: "default_file",
            }],
            ahead: 0,
            error: None,
        }
    }

    #[test]
    fn settings_ide_max_scroll_counts_chip_wrapping() {
        assert_eq!(
            crate::render_view::settings_ui::settings_ide_max_scroll(
                crate::render_view::settings_ui::settings_modal_layout(1000.0, 900.0, 1.0),
                0,
                std::iter::empty(),
                1.0,
            ),
            0.0
        );

        let layout = crate::render_view::settings_ui::settings_modal_layout(1000.0, 500.0, 1.0);
        let no_wrap = crate::render_view::settings_ui::settings_ide_max_scroll(
            layout,
            2,
            [100.0, 120.0],
            1.0,
        );
        let wrapped = crate::render_view::settings_ui::settings_ide_max_scroll(
            layout,
            2,
            [430.0, 120.0, 450.0],
            1.0,
        );

        assert!(no_wrap > 0.0);
        assert!(wrapped > no_wrap);
    }
}
