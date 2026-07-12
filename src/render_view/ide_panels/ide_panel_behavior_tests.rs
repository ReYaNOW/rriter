#[cfg(test)]
mod tests {
    use super::*;

    fn source_between<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
        let start_idx = source.find(start).expect("start marker exists");
        let tail = &source[start_idx..];
        let end_idx = tail.find(end).expect("end marker exists");
        &tail[..end_idx]
    }

    #[test]
    fn clipped_label_prefix_len_reserves_ellipsis_and_keeps_utf8_boundary() {
        assert_eq!(clipped_label_prefix_len("abcdef", 38.0, 8.0, |_| 10.0), 3);
        assert_eq!(
            clipped_label_prefix_len("абвг", 18.0, 8.0, |_| 5.0),
            "аб".len()
        );
        assert_eq!(clipped_label_prefix_len("abc", 4.0, 8.0, |_| 3.0), 0);
    }

    #[test]
    fn project_search_match_preview_clips_around_hit() {
        let text = format!("{}needle{}", "a".repeat(30), "b".repeat(30));
        let (visible, start, end) =
            project_search_visible_match_preview(&text, 30, 36, 90.0, |_| 10.0);

        assert!(visible.starts_with('…'));
        assert!(visible.ends_with('…'));
        assert_eq!(&visible[start..end], "needle");
    }

    #[test]
    fn git_graph_row_layout_shifts_text_for_many_lanes() {
        let one_lane = [crate::app::git_panel::GitGraphLane {
            column: 0,
            target_column: 0,
            color_idx: 0,
            kind: crate::app::git_panel::GitGraphLaneKind::VerticalTop,
        }];
        let six_lane = [
            crate::app::git_panel::GitGraphLane {
                column: 0,
                target_column: 0,
                color_idx: 0,
                kind: crate::app::git_panel::GitGraphLaneKind::VerticalTop,
            },
            crate::app::git_panel::GitGraphLane {
                column: 0,
                target_column: 5,
                color_idx: 5,
                kind: crate::app::git_panel::GitGraphLaneKind::Parent,
            },
        ];

        let one = git_graph_row_layout(10.0, 8.0, 1.0, 0, &one_lane);
        let six = git_graph_row_layout(10.0, 8.0, 1.0, 0, &six_lane);
        let last_lane_x = six.lane_start_x + six.lane_step * 5.0;

        let one_commit_far_right = git_graph_row_layout(10.0, 8.0, 1.0, 5, &one_lane);

        assert!((one.lane_step - 18.0).abs() < 0.001);
        assert!((six.lane_step - 18.0).abs() < 0.001);
        assert!(six.text_x > one.text_x);
        assert!(six.text_x > last_lane_x + 6.0);
        assert_eq!(six.text_x, one_commit_far_right.text_x);
    }

    #[test]
    fn branch_chip_y_uses_text_visual_center() {
        assert_eq!(branch_chip_y_from_text_center(34.0, 18.0), 25.0);
        assert_eq!(branch_chip_y_from_text_center(34.5, 19.0), 25.0);
        assert_eq!(branch_chip_width(50.0, 5.0, 140.0), 60.0);
        assert_eq!(branch_chip_width(160.0, 5.0, 140.0), 140.0);
    }

    #[test]
    fn git_graph_render_shift_to_commit_has_no_bottom_tail() {
        let source = include_str!("ide_panel_git_tooltip_renderer.rs");
        let body = source_between(
            source,
            "fn push_git_graph_shift_to_commit_segment",
            "#[allow(clippy::too_many_arguments)]",
        );

        assert!(body.contains("let mid_x = to_x - dir * radius;"));
        assert!(body.contains("self.push_git_graph_horizontal_segment("));
        assert!(!body.contains("turn_out_y"));
        assert!(!body.contains("row_y + row_h,"));
    }

    #[test]
    fn git_graph_render_soft_vertical_preserves_lane_alpha() {
        let source = include_str!("ide_panel_git_tooltip_renderer.rs");
        let body = source_between(
            source,
            "fn push_git_graph_soft_vertical_segment",
            "fn push_git_graph_parent_segment",
        );

        assert!(body.contains("self.push_git_graph_sdf_segment(x, top, x, bottom, width, color);"));
        assert!(!body.contains("color[3]"));
    }

    #[test]
    fn centered_dialog_button_positions_keep_pair_centered() {
        let (ok_x, cancel_x) = centered_dialog_button_positions(100.0, 420.0, 112.0, 10.0);

        assert_eq!(ok_x, 193.0);
        assert_eq!(cancel_x, 315.0);
        assert_eq!((ok_x + cancel_x + 112.0) / 2.0, 310.0);
    }

    #[test]
    fn git_row_hover_stays_visual_even_when_stage_click_is_locked() {
        assert!(git_row_visual_hovered(
            84.0, 128.0, 48.0, 112.0, 260.0, 28.0, false
        ));
        assert!(git_row_visual_hovered(
            260.0, 128.0, 48.0, 112.0, 260.0, 28.0, false
        ));
        assert!(!git_row_visual_hovered(
            84.0, 128.0, 48.0, 112.0, 260.0, 28.0, true
        ));

        assert!(git_file_row_hitbox_enabled(false));
        assert!(git_file_row_hitbox_enabled(true));
        assert!(!git_file_tooltip_hovered(true, 100.0, 100.0, 16.0));
        assert!(!git_file_tooltip_hovered(true, 116.0, 100.0, 16.0));
        assert!(git_file_tooltip_hovered(true, 116.1, 100.0, 16.0));
        assert!(!git_file_tooltip_hovered(false, 140.0, 100.0, 16.0));

        assert!(git_folder_stage_hitbox_enabled(Some(
            crate::app::git_panel::GitFolderStageState::All
        )));
        assert!(git_folder_stage_hitbox_enabled(Some(
            crate::app::git_panel::GitFolderStageState::Empty
        )));
        assert!(!git_folder_stage_hitbox_enabled(None));
    }

    #[test]
    fn git_disabled_color_dims_folder_text_alpha_only() {
        assert_eq!(
            git_disabled_color([0.2, 0.3, 0.4, 1.0], true, 0.38),
            [0.2, 0.3, 0.4, 0.38]
        );
        assert_eq!(
            git_disabled_color([0.2, 0.3, 0.4, 1.0], false, 0.38),
            [0.2, 0.3, 0.4, 1.0]
        );
    }

    #[test]
    fn git_progress_thumb_phase_ping_pongs_without_jump() {
        let cycle = 1.0 / GIT_PROGRESS_CYCLES_PER_SEC;
        assert!(git_progress_thumb_phase(0.0).abs() < 0.001);
        assert!((git_progress_thumb_phase(cycle * 0.25) - 0.5).abs() < 0.001);
        assert!((git_progress_thumb_phase(cycle * 0.5) - 1.0).abs() < 0.001);
        assert!((git_progress_thumb_phase(cycle * 0.75) - 0.5).abs() < 0.001);
        assert!(git_progress_thumb_phase(cycle).abs() < 0.001);
    }

    #[test]
    fn git_stage_controls_disable_for_pending_or_inactive_workspace() {
        assert!(git_stage_controls_disabled(true, false));
        assert!(git_stage_controls_disabled(false, true));
        assert!(git_stage_controls_disabled(true, true));
        assert!(!git_stage_controls_disabled(false, false));
    }

    #[test]
    fn git_checkbox_color_keeps_staged_state_visible_when_disabled() {
        let (active_bg, active_mark) = git_checkbox_color(true, false, false);
        let (disabled_bg, disabled_mark) = git_checkbox_color(true, false, true);

        assert_eq!(&active_bg[..3], &[0.48, 0.82, 0.52]);
        assert_eq!(&disabled_bg[..3], &[0.48, 0.82, 0.52]);
        assert!(disabled_bg[3] > 0.0 && disabled_bg[3] < active_bg[3]);
        assert!(disabled_mark[3] > 0.0 && disabled_mark[3] < active_mark[3]);

        let (partial_bg, partial_mark) = git_checkbox_color(false, true, true);
        assert!(partial_bg[3] > 0.0);
        assert!(partial_mark[3] > 0.0);
    }

    #[test]
    fn git_file_tooltip_uses_tilde_path_and_status_word() {
        let repo_root = std::path::Path::new("/home/reyan/projects/rriter");
        let file = crate::app::git_panel::GitFileEntry {
            workspace_idx: 0,
            rel_path: "src/main.rs".into(),
            old_rel_path: None,
            display_path: "src/main.rs".into(),
            depth: 1,
            staged: false,
            status: crate::app::git_panel::GitFileStatus::Modified,
        };

        assert_eq!(
            git_file_tooltip_text(repo_root, &file, Some(std::path::Path::new("/home/reyan"))),
            "~/projects/rriter/src/main.rs • Изменен"
        );
        assert_eq!(
            git_status_word(crate::app::git_panel::GitFileStatus::Deleted),
            "Удален"
        );
        assert_eq!(
            git_status_word(crate::app::git_panel::GitFileStatus::Untracked),
            "Не отслеживается"
        );
    }

    #[test]
    fn git_file_tooltip_timer_stays_active_while_mouse_moves_in_same_target() {
        let target = GitTooltipTarget {
            kind: GIT_TOOLTIP_FILE,
            workspace_idx: 1,
            item_idx: 2,
        };
        let other_target = GitTooltipTarget {
            kind: GIT_TOOLTIP_FILE,
            workspace_idx: 1,
            item_idx: 3,
        };
        let now = std::time::Instant::now();

        git_tooltip_reset();
        assert_eq!(git_tooltip_anchor(target, 10.0, 20.0, now), None);
        assert_eq!(
            git_tooltip_anchor(
                target,
                30.0,
                40.0,
                now + std::time::Duration::from_millis(200)
            ),
            None
        );
        assert_eq!(
            git_tooltip_anchor(
                target,
                50.0,
                60.0,
                now + std::time::Duration::from_millis(450)
            ),
            Some((10.0, 20.0))
        );
        assert_eq!(
            git_tooltip_anchor(
                target,
                70.0,
                80.0,
                now + std::time::Duration::from_millis(500)
            ),
            Some((10.0, 20.0))
        );
        assert_eq!(
            git_tooltip_anchor(
                other_target,
                90.0,
                100.0,
                now + std::time::Duration::from_millis(550)
            ),
            None
        );
        assert_eq!(
            git_tooltip_anchor(
                other_target,
                110.0,
                120.0,
                now + std::time::Duration::from_millis(1000)
            ),
            Some((90.0, 100.0))
        );
        assert_eq!(
            git_tooltip_anchor(
                target,
                130.0,
                140.0,
                now + std::time::Duration::from_millis(1100)
            ),
            None
        );
        assert_eq!(
            git_tooltip_anchor(
                target,
                150.0,
                160.0,
                now + std::time::Duration::from_millis(1550)
            ),
            Some((130.0, 140.0))
        );
        git_tooltip_reset();
    }

    #[test]
    fn git_tooltip_reset_requires_new_dwell_after_scroll() {
        let target = GitTooltipTarget {
            kind: GIT_TOOLTIP_FILE,
            workspace_idx: 1,
            item_idx: 2,
        };
        let now = std::time::Instant::now();

        git_tooltip_reset();
        assert_eq!(git_tooltip_anchor(target, 10.0, 20.0, now), None);
        assert_eq!(
            git_tooltip_anchor(
                target,
                10.0,
                20.0,
                now + std::time::Duration::from_millis(450)
            ),
            Some((10.0, 20.0))
        );

        git_tooltip_reset();
        assert_eq!(
            git_tooltip_anchor(
                target,
                10.0,
                20.0,
                now + std::time::Duration::from_millis(500)
            ),
            None
        );
        git_tooltip_reset();
    }

    #[test]
    fn git_folder_row_layout_uses_equal_gaps_and_centered_icon() {
        let layout = git_folder_row_layout(80.0, 40.0, 28.0, 1.0);
        let arrow_to_check =
            layout.check_x - (layout.arrow_x + crate::render_view::tree_ui::TREE_DISCLOSURE_SLOT);
        let check_to_icon = layout.icon_x - (layout.check_x + layout.check_size);

        assert_eq!(arrow_to_check, 6.0);
        assert_eq!(check_to_icon, 6.0);
        assert_eq!(layout.check_y, 48.0);
        assert_eq!(layout.icon_y, 44.0);
    }

    #[test]
    fn git_file_row_layout_draws_icon_between_checkbox_and_label() {
        let layout = git_file_row_layout(98.0, 40.0, 28.0, 1.0);
        let parent_folder_layout = git_folder_row_layout(80.0, 40.0, 28.0, 1.0);

        assert_eq!(layout.check_x, 100.0);
        assert_eq!(layout.check_x, parent_folder_layout.check_x);
        assert_eq!(layout.check_y, 48.0);
        assert_eq!(layout.icon_x - (layout.check_x + layout.check_size), 6.0);
        assert_eq!(layout.icon_x, parent_folder_layout.icon_x);
        assert_eq!(layout.icon_y, 44.0);
        assert_eq!(layout.text_x - (layout.icon_x + layout.icon_size), 4.0);
    }

    #[test]
    fn file_tree_context_menu_groups_insert_logical_separators() {
        use crate::app::file_tree::FileTreeMenuAction;

        let file_tree_entries = [
            FileTreeMenuAction::CreateFile,
            FileTreeMenuAction::CreateDirectory,
            FileTreeMenuAction::Paste,
            FileTreeMenuAction::Delete,
            FileTreeMenuAction::Rename,
            FileTreeMenuAction::OpenContainedFolder,
            FileTreeMenuAction::CopyRelativePath,
        ];

        assert!(!file_tree_menu_separator_before(&file_tree_entries, 0));
        assert!(!file_tree_menu_separator_before(&file_tree_entries, 2));
        assert!(file_tree_menu_separator_before(&file_tree_entries, 3));
        assert!(file_tree_menu_separator_before(&file_tree_entries, 5));
        assert_eq!(file_tree_menu_separator_count(&file_tree_entries), 2);

        let tab_entries = [
            FileTreeMenuAction::ShowInExplorer,
            FileTreeMenuAction::OpenContainedFolder,
            FileTreeMenuAction::CopyTargetAbsolutePath,
            FileTreeMenuAction::CopyTargetRelativePath,
        ];
        assert!(file_tree_menu_separator_before(&tab_entries, 1));
        assert!(!file_tree_menu_separator_before(&tab_entries, 2));
        assert_eq!(file_tree_menu_separator_count(&tab_entries), 1);
    }
}
