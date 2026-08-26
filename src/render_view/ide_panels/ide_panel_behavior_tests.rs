#[cfg(test)]
mod tests {
    use super::*;

    fn source_between<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
        let start_idx = source.find(start).expect("start marker exists");
        let tail = &source[start_idx..];
        let end_idx = tail.find(end).expect("end marker exists");
        &tail[..end_idx]
    }

    fn assert_rect_inside(
        inner: crate::ui_system::UiClipRect,
        outer: crate::ui_system::UiClipRect,
    ) {
        assert!(inner.x >= outer.x - 0.5);
        assert!(inner.y >= outer.y - 0.5);
        assert!(inner.x + inner.w <= outer.x + outer.w + 0.5);
        assert!(inner.y + inner.h <= outer.y + outer.h + 0.5);
    }


    #[test]
    fn draggable_panels_share_one_full_content_dispatcher_in_both_groups() {
        let side = include_str!("ide_panel_side_renderer.rs");
        let bottom = include_str!("ide_panel_dialog_renderer.rs");
        let dispatcher = source_between(
            side,
            "fn draw_ide_panel_content",
            "pub(crate) fn draw_ide_side_panels",
        );

        for panel in [
            "PanelId::Explorer",
            "PanelId::Search",
            "PanelId::Git",
            "PanelId::ApiClient",
            "PanelId::Database",
            "PanelId::Terminal",
            "PanelId::Problems",
            "PanelId::LspServers",
        ] {
            assert!(dispatcher.contains(panel), "missing shared renderer for {panel}");
        }
        assert_eq!(side.matches("self.draw_ide_panel_content(").count(), 1);
        assert_eq!(bottom.matches("self.draw_ide_panel_content(").count(), 1);
        assert!(!bottom.contains("Плейсхолдер контента"));
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
    fn dialog_button_text_baseline_is_pixel_stable_at_fractional_scale() {
        for scale in [1.0, 1.25, 1.5, 1.75] {
            let btn_y = 180.0;
            let btn_h = 34.0 * scale;
            let baseline = dialog_button_text_baseline(btn_y, btn_h, scale);
            let visual_center = btn_y + btn_h * 0.5 + 5.0 * scale;

            assert_eq!(baseline.fract(), 0.0);
            assert!((baseline - visual_center).abs() <= 0.5);
        }
    }

    #[test]
    fn project_search_help_compresses_before_reaching_close_button() {
        let short_h = 268.0;
        let factor = project_search_help_content_factor(short_h, 1.0);
        let content_bottom = 58.0 + 255.0 * factor;
        let close_button_top = short_h - 64.0;

        assert!(factor < 1.0);
        assert!(factor >= 0.45);
        assert!(content_bottom <= close_button_top - 12.0 + f32::EPSILON);
        assert_eq!(project_search_help_content_factor(420.0, 1.0), 1.0);
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
    fn git_pending_state_dims_stage_checkboxes_like_other_disabled_controls() {
        let (active_bg, active_mark) = git_stage_checkbox_color(true, false, false, false);
        let (pending_bg, pending_mark) = git_stage_checkbox_color(true, false, false, true);

        assert!(pending_bg[3] < active_bg[3]);
        assert!(pending_mark[3] < active_mark[3]);
        assert_eq!(&pending_bg[..3], &active_bg[..3]);
    }

    #[test]
    fn git_simple_tooltip_layout_stays_inside_window_and_clips_long_text() {
        let layout = git_simple_tooltip_layout(320.0, 180.0, 310.0, 170.0, 900.0, 1.0)
            .expect("tooltip layout");

        assert!(layout.x >= 8.0);
        assert!(layout.y >= 8.0);
        assert!(layout.x + layout.w <= 312.0);
        assert!(layout.y + layout.h <= 172.0);
        assert!(layout.text_w <= layout.w - 24.0 + f32::EPSILON);
        assert!(layout.w < 900.0 + 24.0);
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
        assert_eq!(
            (1..file_tree_entries.len())
                .filter(|&idx| file_tree_menu_separator_before(&file_tree_entries, idx))
                .count(),
            2
        );

        let tab_entries = [
            FileTreeMenuAction::ShowInExplorer,
            FileTreeMenuAction::OpenContainedFolder,
            FileTreeMenuAction::CopyTargetAbsolutePath,
            FileTreeMenuAction::CopyTargetRelativePath,
        ];
        assert!(file_tree_menu_separator_before(&tab_entries, 1));
        assert!(!file_tree_menu_separator_before(&tab_entries, 2));
        assert_eq!(
            (1..tab_entries.len())
                .filter(|&idx| file_tree_menu_separator_before(&tab_entries, idx))
                .count(),
            1
        );
    }

    #[test]
    fn settings_ignore_add_reuses_shared_disabled_button_renderer() {
        let source = include_str!("../settings_ui.rs");
        let body = source_between(
            source,
            "// Кнопка «Добавить» — неактивна если поле пустое или только пробелы",
            "content_y += input_h + 16.0 * s;",
        );

        assert!(body.contains("btn_ignore_add.render_disabled(self, s);"));
        assert!(!body.contains("push_rounded_rect_border"));
        assert!(!body.contains("draw_atlas_icon"));
    }


    #[test]
    fn connection_dialog_footer_rows_never_overlap() {
        let layout = database_dialog_footer_layout(10.0, 600.0, 1.0);
        assert!(layout.form_bottom < layout.message_baseline);
        assert!(layout.message_baseline < layout.summary_baseline);
        assert!(layout.summary_baseline < layout.toggle_y);
        assert!(layout.toggle_y + 30.0 < layout.actions_y);
        assert!(layout.actions_y + 30.0 <= 610.0);
    }

    #[test]
    fn connection_dialog_without_ssh_fits_without_scrollbar() {
        let layout = database_connection_dialog_layout(1200.0, 900.0, 1.0, 6);
        assert_eq!(layout.max_scroll, 0.0);
        assert!(layout.scrollbar_track.is_none());
        assert!(layout.content_height <= layout.form_clip.h);
    }

    #[test]
    fn bastion_form_on_short_window_has_shared_positive_max_scroll() {
        let layout = database_connection_dialog_layout(800.0, 500.0, 1.0, 20);
        assert!(layout.max_scroll > 0.0);
        assert!(layout.scrollbar_track.is_some());
        assert_eq!(
            layout.max_scroll,
            (layout.content_height - layout.form_clip.h).max(0.0)
        );
    }

    #[test]
    fn last_bastion_field_can_be_fully_scrolled_above_fixed_footer() {
        let layout = database_connection_dialog_layout(800.0, 500.0, 1.0, 20);
        let last = database_dialog_field_layout(&layout, 19, layout.max_scroll, false, false);
        assert!(last.input.y >= layout.form_clip.y - 0.5);
        assert!(last.input.y + last.input.h <= layout.footer.form_bottom + 0.5);
        assert_eq!(layout.form_clip.y + layout.form_clip.h, layout.footer.form_bottom);
    }

    #[test]
    fn dialog_scrollbar_thumb_reaches_both_track_ends() {
        let layout = database_connection_dialog_layout(800.0, 500.0, 1.0, 20);
        let track = layout.scrollbar_track.unwrap();
        let at_start = database_connection_dialog_scrollbar_thumb(&layout, 0.0).unwrap();
        let at_end =
            database_connection_dialog_scrollbar_thumb(&layout, layout.max_scroll).unwrap();
        assert_eq!(at_start.start, track.y);
        assert!((at_end.start + at_end.len - (track.y + track.h)).abs() <= 0.5);
    }

    #[test]
    fn tiny_dialog_viewport_never_inverts_form_or_scrollbar_geometry() {
        let layout = database_connection_dialog_layout(90.0, 70.0, 1.75, 20);
        assert!(layout.form_clip.w >= 0.0);
        assert!(layout.form_clip.h >= 0.0);
        assert!(layout.max_scroll.is_finite());
        if let Some(track) = layout.scrollbar_track {
            assert!(track.w > 0.0);
            assert!(track.h > 0.0);
            assert_rect_inside(track, crate::ui_system::UiClipRect::new(
                layout.modal.x,
                layout.modal.y,
                layout.modal.w,
                layout.modal.h,
            ));
        }
    }

    #[test]
    fn field_draw_hit_label_and_eye_share_one_scrolled_row_geometry() {
        let layout = database_connection_dialog_layout(800.0, 500.0, 1.25, 20);
        let before = database_dialog_field_layout(&layout, 10, 0.0, true, true);
        let after = database_dialog_field_layout(&layout, 10, 47.0, true, true);
        assert_eq!(before.input.y - after.input.y, 47.0);
        assert_eq!(before.label.y - after.label.y, 47.0);
        assert_eq!(
            before.eye_hit.unwrap().y - after.eye_hit.unwrap().y,
            47.0
        );
        assert_eq!(before.input.y, before.label.y);
        assert_eq!(before.input.y, before.eye_hit.unwrap().y);
    }

    #[test]
    fn footer_geometry_is_fixed_when_content_row_count_changes() {
        let direct = database_connection_dialog_layout(800.0, 500.0, 1.0, 6);
        let bastion = database_connection_dialog_layout(800.0, 500.0, 1.0, 20);
        assert_eq!(direct.footer, bastion.footer);
        assert_eq!(direct.form_clip, bastion.form_clip);
    }

    #[test]
    fn every_connection_field_label_marks_and_describes_its_tooltip() {
        for field in crate::app::database::DatabaseFormField::ALL {
            let label = database_field_label(field);
            let tooltip = DatabaseDialogTooltipTarget::Field(field).text();
            assert!(label.ends_with('*'), "missing tooltip marker for {field:?}");
            assert!(!tooltip.trim().is_empty(), "missing tooltip for {field:?}");
            assert_ne!(tooltip.trim_end_matches('.'), label.trim_end_matches('*'));
        }
    }

    #[test]
    fn password_tooltips_never_include_runtime_secret_values() {
        let secret = "actual-password-123";
        for field in [
            crate::app::database::DatabaseFormField::PostgresPassword,
            crate::app::database::DatabaseFormField::SshPassword,
            crate::app::database::DatabaseFormField::SshKeyPassphrase,
            crate::app::database::DatabaseFormField::JumpPassword,
            crate::app::database::DatabaseFormField::JumpKeyPassphrase,
        ] {
            assert!(!DatabaseDialogTooltipTarget::Field(field).text().contains(secret));
        }
    }

    #[test]
    fn all_four_footer_controls_have_distinct_standard_tooltip_targets() {
        let targets = [
            DatabaseDialogTooltipTarget::Tls,
            DatabaseDialogTooltipTarget::Color,
            DatabaseDialogTooltipTarget::Ssh,
            DatabaseDialogTooltipTarget::Jump,
        ];
        let mut keys = std::collections::HashSet::new();
        for target in targets {
            assert!(keys.insert(target.key()));
            assert!(!target.text().is_empty());
            assert_eq!(target.key() & DATABASE_DIALOG_TOOLTIP_NAMESPACE, DATABASE_DIALOG_TOOLTIP_NAMESPACE);
        }
    }

    #[test]
    fn eye_hover_circle_is_smaller_but_keeps_original_hit_target_and_center() {
        let layout = database_connection_dialog_layout(800.0, 500.0, 1.0, 20);
        let field = database_dialog_field_layout(&layout, 0, 0.0, false, true);
        let hit = field.eye_hit.unwrap();
        let visual = field.eye_visual.unwrap();
        assert!(visual.w < hit.w);
        assert!(visual.w >= hit.w * 0.85 - 0.5);
        assert!(visual.w <= hit.w * 0.90 + 0.5);
        assert_eq!(visual.w, visual.h);
        assert!(((visual.x + visual.w * 0.5) - (hit.x + hit.w * 0.5)).abs() <= 0.5);
        assert!(((visual.y + visual.h * 0.5) - (hit.y + hit.h * 0.5)).abs() <= 0.5);
        assert_rect_inside(visual, field.input);
    }

    #[test]
    fn secondary_database_dialog_text_uses_readable_stable_scale() {
        assert!(DATABASE_DIALOG_FIELD_TEXT_SCALE >= 0.82);
        assert!(DATABASE_DIALOG_SECONDARY_TEXT_SCALE >= 0.78);
        for scale in [1.0, 1.25, 1.5, 1.75] {
            let layout = database_connection_dialog_layout(800.0, 600.0, scale, 20);
            let field = database_dialog_field_layout(&layout, 4, 17.0, false, false);
            assert_eq!(field.input.y, field.input.y.round());
            assert_eq!(field.label.y, field.label.y.round());
            assert_eq!(field.input.h, field.input.h.round());
        }
    }

    #[test]
    fn tooltip_rect_stays_inside_window_after_scroll_or_fractional_dpi() {
        for scale in [1.0, 1.25, 1.5, 1.75] {
            let rect = database_dialog_tooltip_rect(
                640.0,
                360.0,
                630.0,
                350.0,
                520.0,
                180.0,
                scale,
            )
            .unwrap();
            assert_rect_inside(rect, crate::ui_system::UiClipRect::new(0.0, 0.0, 640.0, 360.0));
        }
    }

    #[test]
    fn git_commit_control_layout_keeps_shared_anchors_stable_at_fractional_scales() {
        for scale in [1.0, 1.25, 1.5, 1.75] {
            let panel_x = 48.0 * scale;
            let panel_w = 320.0 * scale;
            let layout = git_commit_controls_layout(panel_x, panel_w, 32.0 * scale, scale);

            assert!(layout.commit.x + layout.commit.w <= layout.menu.x + 0.01);
            assert!(layout.menu.x + layout.menu.w <= layout.options.x + 0.01);
            assert!(layout.commit.x >= panel_x);
            assert!(layout.options.x + layout.options.w <= panel_x + panel_w + 0.01);

            let menu_anchor = git_dropdown_anchor(layout.menu, scale);
            let options_anchor = git_dropdown_anchor(layout.options, scale);
            assert_eq!(menu_anchor.0, menu_anchor.0.round());
            assert_eq!(menu_anchor.1, menu_anchor.1.round());
            assert_eq!(options_anchor.0, options_anchor.0.round());
            assert_eq!(options_anchor.1, options_anchor.1.round());
            assert!(menu_anchor.1 >= layout.menu.y + layout.menu.h);
            assert!(options_anchor.1 >= layout.options.y + layout.options.h);
        }
    }

    #[test]
    fn all_git_dropdowns_render_only_from_the_late_context_overlay_path() {
        let workspace = include_str!("ide_panel_git_workspace_renderer.rs");
        let side = include_str!("ide_panel_side_renderer.rs");
        let root = include_str!("../root_frame_renderer.rs");
        let root_overlays = include_str!("../root_frame_overlay_helpers.rs");
        let shared_finish = source_between(
            root_overlays,
            "fn finish_root_overlays_and_telemetry",
            "fn finalize_root_frame_telemetry",
        );
        let overlay = source_between(
            side,
            "pub(crate) fn draw_git_dropdown_overlays",
            "pub(crate) fn draw_ide_side_panels",
        );
        let early_side_pass = &side[side
            .find("pub(crate) fn draw_ide_side_panels")
            .expect("side-panel pass exists")..];

        assert_eq!(workspace.matches("draw_animated_context_menu(").count(), 0);
        assert!(!workspace.contains("UiId::GitFetch("));
        assert!(!workspace.contains("UiId::GitPull("));
        assert!(!early_side_pass.contains("draw_git_dropdown_overlays("));

        assert_eq!(overlay.matches("draw_animated_context_menu(").count(), 3);
        assert!(overlay.contains("commit_menu_opened_at"));
        assert!(overlay.contains("commit_options_menu_opened_at"));
        assert!(overlay.contains("active_repo_action_menu_opened_at"));
        assert!(overlay.contains("ui_registry.mark_overlay_start()"));
        assert!(overlay.contains("UiId::GitFetch"));
        assert!(overlay.contains("UiId::GitPull"));

        assert!(root_overlays.contains("self.draw_git_dropdown_overlays("));
        assert_eq!(root.matches("draw_file_tree_overlays(").count(), 0);
        assert_eq!(root.matches("draw_ide_context_overlays(").count(), 2);
        assert_eq!(
            shared_finish
                .matches("self.draw_ide_context_overlays(")
                .count(),
            1
        );
    }

    #[test]
    fn git_dropdown_items_registered_after_overlay_mark_win_over_editor_hitboxes() {
        for overlay_id in [
            crate::ui_system::UiId::GitCommitMenuItem(0),
            crate::ui_system::UiId::GitCommitOptionsItem(0),
            crate::ui_system::UiId::GitFetch(0),
        ] {
            let mut registry = crate::ui_system::UiRegistry::new();
            registry.register_rect(
                crate::ui_system::UiId::EditorTextBody,
                0.0,
                0.0,
                100.0,
                40.0,
                10.0,
                10.0,
            );
            registry.mark_overlay_start();
            registry.register_rect(overlay_id, 0.0, 0.0, 100.0, 40.0, 10.0, 10.0);
            assert_eq!(registry.find_overlay_at(10.0, 10.0), Some(overlay_id));
            assert_eq!(registry.find_at(10.0, 10.0), Some(overlay_id));
        }
    }

    #[test]
    fn markdown_status_mode_and_labels_follow_active_mode_only_for_markdown() {
        assert_eq!(
            markdown_status_mode_for_ext("md", crate::app::MarkdownMode::Read),
            Some(crate::app::MarkdownMode::Read)
        );
        assert_eq!(
            markdown_status_mode_for_ext("markdown", crate::app::MarkdownMode::Edit),
            Some(crate::app::MarkdownMode::Edit)
        );
        assert_eq!(
            markdown_status_mode_for_ext("py", crate::app::MarkdownMode::Read),
            None
        );
        assert_eq!(
            markdown_status_mode_label(crate::app::MarkdownMode::Edit),
            "↔ Редактирование"
        );
        assert_eq!(
            markdown_status_mode_label(crate::app::MarkdownMode::Read),
            "↔ Чтение"
        );
    }

    #[test]
    fn markdown_status_narrow_layout_stays_inside_bar_without_overlap() {
        let bar = crate::ui_system::UiClipRect::new(48.0, 700.0, 312.0, 28.0);
        let layout = status_language_layout(bar, 70.0, Some(112.0), 1.0);
        let mode = layout.mode_rect.expect("markdown mode button");
        assert!(mode.w > 0.0 && mode.h > 0.0);
        assert_rect_inside(mode, bar);
        assert!(mode.x + mode.w <= layout.language_x - 8.0 + 0.5);
        assert!(!status_diagnostics_fit(layout, mode.x + 1.0, 1.0));

        let mut registry = crate::ui_system::UiRegistry::new();
        let mx = mode.x + mode.w * 0.5;
        let my = mode.y + mode.h * 0.5;
        assert!(registry.register_rect_clipped(
            crate::ui_system::UiId::MarkdownModeToggle,
            mode.x,
            mode.y,
            mode.w,
            mode.h,
            bar,
            mx,
            my,
        ));
        assert_eq!(
            registry.find_at(mx, my),
            Some(crate::ui_system::UiId::MarkdownModeToggle)
        );
    }

}
