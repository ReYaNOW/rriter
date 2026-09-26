    use super::*;
    #[test]
    fn git_context_menu_open_times_participate_in_animation_redraw_selection() {
        let mut ide_panel = crate::app::IdePanelState::default();
        let start = Instant::now();

        ide_panel.git.commit_menu_opened_at = Some(start);
        assert_eq!(active_context_menu_opened_at(&ide_panel), Some(start));

        let options = start + std::time::Duration::from_millis(2);
        ide_panel.git.commit_menu_opened_at = None;
        ide_panel.git.commit_options_menu_opened_at = Some(options);
        assert_eq!(active_context_menu_opened_at(&ide_panel), Some(options));

        let repo = options + std::time::Duration::from_millis(2);
        ide_panel.git.commit_options_menu_opened_at = None;
        ide_panel.git.toggle_repo_action_menu(0, repo);
        assert_eq!(active_context_menu_opened_at(&ide_panel), Some(repo));

        assert!(crate::app::context_menu::context_menu_anim_progress(
            repo,
            repo + std::time::Duration::from_millis(50),
        ) < 1.0);
        assert_eq!(
            crate::app::context_menu::context_menu_anim_progress(
                repo,
                repo + std::time::Duration::from_secs_f32(
                    crate::app::context_menu::CONTEXT_MENU_ANIM_SECS,
                ),
            ),
            1.0
        );
    }

    #[test]
    fn editor_and_terminal_tab_drag_share_continuous_redraw_lifecycle() {
        let drag = crate::app::TabDragState {
            start_idx: 0,
            start_x: 10.0,
            current_x: 20.0,
            threshold_passed: true,
        };
        let mut ide_panel = crate::app::IdePanelState::default();
        assert!(!tab_drag_animation_active(&ide_panel));

        ide_panel.tab_drag = Some(drag.clone());
        assert!(tab_drag_animation_active(&ide_panel));

        ide_panel.tab_drag = None;
        ide_panel.terminal_tab_drag = Some(drag);
        assert!(tab_drag_animation_active(&ide_panel));
    }

    #[test]
    fn active_animation_keeps_event_loop_polling() {
        for (autocomplete, git_progress, scroll) in
            [(true, false, false), (false, true, false), (false, false, true)]
        {
            assert!(needs_continuous_poll(autocomplete, git_progress, scroll));
        }
        assert!(!needs_continuous_poll(false, false, false));
    }

    #[test]
    fn animation_dt_restores_smooth_idle_scroll_start() {
        assert_eq!(animation_dt(1.0 / 60.0), 0.016);
        assert!((animation_dt(1.0 / 240.0) - 1.0 / 240.0).abs() < f32::EPSILON);
        assert_eq!(animation_dt(0.5), 0.016);
        assert_eq!(animation_dt(0.0), 0.0);
    }

    #[test]
    fn sticky_animation_add_remove_and_equal_length_are_pure_state_transitions() {
        let mut current = vec![];
        let target = vec![(1, 2), (3, 4)];
        let mut progress = 1.0;
        let mut adding = false;

        assert!(update_sticky_animation(
            &mut current,
            &target,
            &mut progress,
            &mut adding,
            0.01,
        ));
        assert_eq!(current, target);
        assert!(adding);
        assert!(progress > 0.0 && progress < 1.0);

        let target = vec![(1, 2)];
        progress = 1.0;
        assert!(update_sticky_animation(
            &mut current,
            &target,
            &mut progress,
            &mut adding,
            0.20,
        ));
        assert_eq!(current, target);
        assert!(!adding);
        assert_eq!(progress, 1.0);

        let target = vec![(9, 9)];
        assert!(update_sticky_animation(
            &mut current,
            &target,
            &mut progress,
            &mut adding,
            0.01,
        ));
        assert_eq!(current, target);
        assert_eq!(progress, 1.0);
    }

    #[test]
    fn about_wait_plan_prioritizes_redraw_highlight_hover_and_blink() {
        let now = Instant::now();
        let last_action = now - std::time::Duration::from_millis(1250);

        assert_eq!(
            compute_about_wait_plan(
                now,
                last_action,
                true,
                false,
                false,
                false,
                true,
                None,
                false,
                false,
            ),
            AboutWaitPlan::Wait,
        );
        assert_eq!(
            compute_about_wait_plan(
                now,
                last_action,
                false,
                true,
                true,
                false,
                true,
                None,
                false,
                false,
            ),
            AboutWaitPlan::Wait,
        );
        assert_eq!(
            compute_about_wait_plan(
                now,
                last_action,
                false,
                false,
                false,
                true,
                true,
                None,
                false,
                false,
            ),
            AboutWaitPlan::WaitUntil(now + std::time::Duration::from_millis(5)),
        );
        assert_eq!(
            compute_about_wait_plan(
                now,
                last_action,
                false,
                false,
                false,
                true,
                true,
                Some(now + std::time::Duration::from_millis(2)),
                true,
                false,
            ),
            AboutWaitPlan::WaitUntil(now + std::time::Duration::from_millis(2)),
        );
        assert_eq!(
            compute_about_wait_plan(
                now,
                last_action,
                false,
                false,
                false,
                false,
                true,
                None,
                false,
                false,
            ),
            AboutWaitPlan::WaitUntil(last_action + std::time::Duration::from_millis(1500)),
        );
        assert_eq!(
            compute_about_wait_plan(
                now,
                last_action,
                false,
                false,
                false,
                false,
                false,
                None,
                false,
                false,
            ),
            AboutWaitPlan::Wait,
        );
        assert_eq!(
            compute_about_wait_plan(
                now,
                last_action,
                false,
                false,
                false,
                false,
                false,
                Some(now + std::time::Duration::from_millis(20)),
                true,
                false,
            ),
            AboutWaitPlan::WaitUntil(now + std::time::Duration::from_millis(16)),
        );
    }

    #[test]
    fn suspended_wait_plan_keeps_waking_only_while_database_job_is_pending() {
        let now = Instant::now();
        assert!(matches!(suspended_about_wait_plan(now, false), AboutWaitPlan::Wait));
        match suspended_about_wait_plan(now, true) {
            AboutWaitPlan::WaitUntil(at) => {
                assert_eq!(at, now + std::time::Duration::from_millis(100));
            }
            AboutWaitPlan::Wait => panic!("pending database job must keep polling while suspended"),
        }
    }

    #[test]
    fn suspended_about_to_wait_still_polls_database_runtime() {
        let about = include_str!("../about.rs");
        let suspended = about
            .split("if app.render_suspended && !automation_running {")
            .nth(1)
            .expect("suspended branch")
            .split("return;")
            .next()
            .expect("suspended branch return");
        assert!(suspended.contains("app.poll_database_runtime();"));
        assert!(suspended.contains("suspended_about_wait_plan("));
    }
