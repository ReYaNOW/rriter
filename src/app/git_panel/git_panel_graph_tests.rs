    #[test]
    fn git_graph_prefetch_skips_loaded_active_and_cached_repos() {
        assert!(!git_graph_prefetch_needed(
            false,
            true,
            GIT_GRAPH_LIMIT_STEP,
            None,
            GIT_GRAPH_LIMIT_STEP
        ));
        assert!(!git_graph_prefetch_needed(
            false,
            false,
            0,
            Some(GIT_GRAPH_LIMIT_STEP),
            GIT_GRAPH_LIMIT_STEP
        ));
        assert!(git_graph_prefetch_needed(
            false,
            true,
            GIT_GRAPH_LIMIT_STEP - 1,
            None,
            GIT_GRAPH_LIMIT_STEP
        ));
        assert!(git_graph_prefetch_needed(
            true,
            true,
            GIT_GRAPH_LIMIT_STEP,
            Some(GIT_GRAPH_LIMIT_STEP),
            GIT_GRAPH_LIMIT_STEP
        ));
    }

    #[test]
    fn git_graph_scroll_thumb_shrinks_as_loaded_commits_grow() {
        let thumb_h = |commits: usize, rows_h: f32, scale: f32| {
            git_graph_scrollbar(0.0, 300.0, 0.0, rows_h, commits, 0.0, scale)
                .geometry(scale)
                .map(|g| g.thumb.len)
                .expect("graph overflows")
        };
        let initial = thumb_h(100, 500.0, 1.0);
        let loaded_more = thumb_h(300, 500.0, 1.0);

        assert!(loaded_more < initial);
        assert!(loaded_more >= 10.0);

        let tiny = thumb_h(100, 23.921906, 1.3333334);
        assert!(tiny > 0.0);
        assert!(tiny <= 23.921906);
    }

    #[test]
    fn git_graph_drag_updates_target_without_teleporting_current() {
        let mut scroll = crate::scroll::ScrollState::new(15.0);
        scroll.current = 12.0;
        scroll.target = 12.0;
        scroll.velocity = 9.0;

        assert!(crate::app::mouse::apply_scrollbar_drag_target(&mut scroll, 240.0, 7.0));

        assert_eq!(scroll.current, 12.0);
        assert_eq!(scroll.target, 240.0);
        assert_eq!(scroll.velocity, 9.0);
        assert_eq!(scroll.drag_offset, 7.0);
        assert!(scroll.is_dragging);
        assert_eq!(scroll.anim_speed, 15.0);
        assert!(git_graph_near_load_more(scroll.target, 240.0, 1.0));
    }

    fn graph_commit(oid: &str, parents: &[&str]) -> GitGraphCommit {
        GitGraphCommit {
            oid: Arc::<str>::from(oid),
            short_oid: oid.chars().take(7).collect(),
            summary: oid.to_string(),
            branch_name: None,
            author_name: "A".to_string(),
            author_email: "a@example.invalid".to_string(),
            time_secs: 0,
            time_offset: 0,
            relative_time: String::new(),
            absolute_time: String::new(),
            local_refs: Vec::new(),
            remote_refs: Vec::new(),
            lanes: Vec::new(),
            column: 0,
            color_idx: 0,
            branch_total_count: None,
            is_head: false,
            github_url: None,
            stats: None,
            parent_oids: parents
                .iter()
                .map(|parent| Arc::<str>::from(*parent))
                .collect(),
        }
    }

    fn has_graph_lane(commit: &GitGraphCommit, kind: GitGraphLaneKind, column: usize) -> bool {
        commit.lanes.iter().any(|lane| {
            lane.kind == kind
                && match kind {
                    GitGraphLaneKind::Parent
                    | GitGraphLaneKind::Shift
                    | GitGraphLaneKind::ShiftToCommit => usize::from(lane.target_column) == column,
                    _ => usize::from(lane.column) == column,
                }
        })
    }

    #[test]
    fn git_graph_remote_url_parse_and_ref_normalize() {
        assert_eq!(
            github_base_url_from_remote_url("https://github.com/org/repo.git"),
            Some("https://github.com/org/repo".to_string())
        );
        assert_eq!(
            github_base_url_from_remote_url("git@github.com:org/repo.git"),
            Some("https://github.com/org/repo".to_string())
        );
        assert_eq!(
            github_base_url_from_remote_url("ssh://git@github.com/org/repo"),
            Some("https://github.com/org/repo".to_string())
        );
        assert_eq!(
            github_base_url_from_remote_url("https://example.com/x/y"),
            None
        );

        assert_eq!(
            normalize_git_ref_name("refs/heads/master"),
            Some(GitGraphRef {
                name: "master".to_string(),
                is_remote: false,
            })
        );
        assert_eq!(
            normalize_git_ref_name("refs/remotes/origin/master"),
            Some(GitGraphRef {
                name: "origin/master".to_string(),
                is_remote: true,
            })
        );
        assert_eq!(normalize_git_ref_name("refs/remotes/origin/HEAD"), None);
        assert_eq!(normalize_git_ref_name("refs/tags/v1"), None);
        assert_eq!(
            git_graph_merge_source_label("Merge pull request #2 from stormasm/update_api"),
            Some("merged from stormasm/update_api".to_string())
        );
        assert_eq!(
            git_graph_merge_source_label("Merge branch 'feature/ui'"),
            Some("merged from feature/ui".to_string())
        );
        assert_eq!(
            git_graph_merge_source_label("Merge branch 'feature/ui' into 'main'"),
            Some("merged from feature/ui".to_string())
        );
        assert_eq!(
            git_graph_merge_source_label("Merge remote-tracking branch 'origin/feature/ui'"),
            Some("merged from origin/feature/ui".to_string())
        );
        assert_eq!(
            git_graph_merge_source_label("Merged in feature/ui (pull request #7)"),
            Some("merged from feature/ui".to_string())
        );
        assert_eq!(
            git_graph_merge_source_label(
                "Merge branch from ac6feca32d5424753f2664167f6b07f89e70cf11 to master"
            ),
            None
        );
        assert_eq!(
            git_graph_merge_source_label("Merge branch from feature/ui to master"),
            Some("merged from feature/ui".to_string())
        );
        assert_eq!(git_graph_merge_source_label("feat: normal commit"), None);
        assert_eq!(
            git_graph_change_request_label("fix calculator (#12)"),
            Some("PR #12".to_string())
        );
        assert_eq!(
            git_graph_change_request_label("See merge request group/repo!34"),
            Some("MR !34".to_string())
        );
        assert_eq!(
            git_graph_merge_side_parent_label("Merge something custom"),
            "merged side branch"
        );
        assert_eq!(
            git_graph_note_label("See merge request group/repo!34"),
            Some("MR !34".to_string())
        );
        assert_eq!(
            git_graph_note_label("reviewed by ops"),
            Some("note: reviewed by ops".to_string())
        );
        assert_eq!(
            git_graph_reflog_label("merge feature/api: Merge made by the 'ort' strategy."),
            Some("reflog merge feature/api".to_string())
        );
        assert_eq!(
            git_graph_reflog_label("pull origin main: Fast-forward"),
            Some("reflog pull origin main".to_string())
        );
    }

    #[test]
    fn git_graph_summary_trims_hidden_prefixes() {
        assert_eq!(
            clean_git_summary("\u{feff}\u{200b}  fix check_item in audits;"),
            "fix check_item in audits;"
        );
        assert_eq!(clean_git_summary("\u{200b}"), "(no message)");
    }

    #[test]
    fn git_graph_time_format_is_cached_friendly() {
        assert_eq!(format_git_relative_time(100, 130), "только что");
        assert_eq!(format_git_relative_time(0, 60), "1 минута назад");
        assert_eq!(format_git_relative_time(0, 120), "2 минуты назад");
        assert_eq!(format_git_relative_time(0, 300), "5 минут назад");
        assert_eq!(format_git_relative_time(0, 3 * 3600), "3 часа назад");
        assert_eq!(format_git_absolute_time(0, 0), "1 января 1970 г. в 00:00");
        assert_eq!(format_git_absolute_time(0, 180), "1 января 1970 г. в 03:00");
    }

    #[test]
    fn git_graph_lane_layout_handles_branch_and_merge() {
        let mut commits = vec![
            graph_commit("merge", &["main", "branch"]),
            graph_commit("main", &["root"]),
            graph_commit("branch", &["root"]),
            graph_commit("root", &[]),
        ];

        let lane_count = apply_git_graph_lanes(&mut commits);

        assert_eq!(lane_count, 2);
        assert_eq!(commits[0].column, 0);
        assert_eq!(commits[2].column, 1);
        assert!(commits[0].lanes.iter().any(|lane| {
            lane.kind == GitGraphLaneKind::Parent
                && usize::from(lane.target_column) == commits[2].column
        }));
        assert!(has_graph_lane(
            &commits[2],
            GitGraphLaneKind::VerticalTop,
            commits[2].column
        ));
        assert!(has_graph_lane(
            &commits[2],
            GitGraphLaneKind::VerticalBottom,
            commits[2].column
        ));
        assert!(has_graph_lane(
            &commits[3],
            GitGraphLaneKind::ShiftToCommit,
            0
        ));
    }

    #[test]
    fn git_graph_lane_layout_collapses_side_branch_into_commit_without_shift_tail() {
        let mut commits = vec![
            graph_commit("merge", &["main", "side"]),
            graph_commit("main", &["base"]),
            graph_commit("side", &["base"]),
            graph_commit("base", &[]),
        ];

        apply_git_graph_lanes(&mut commits);

        let side_column = commits[2].column;
        let base_column = commits[3].column;
        assert_eq!(side_column, 1);
        assert_eq!(base_column, 0);
        assert!(commits[3].lanes.iter().any(|lane| {
            lane.kind == GitGraphLaneKind::ShiftToCommit
                && usize::from(lane.column) == side_column
                && usize::from(lane.target_column) == base_column
        }));
        assert!(!commits[3].lanes.iter().any(|lane| {
            lane.kind == GitGraphLaneKind::Shift
                && usize::from(lane.column) == side_column
                && usize::from(lane.target_column) == base_column
        }));
        assert!(!has_graph_lane(
            &commits[3],
            GitGraphLaneKind::VerticalBottom,
            side_column
        ));
    }

    #[test]
    fn git_graph_lane_layout_keeps_long_side_chains_connected() {
        let mut commits = vec![
            graph_commit("merge", &["main", "branch1", "branch2", "branch3"]),
            graph_commit("main", &["root"]),
            graph_commit("branch1", &["branch1_mid"]),
            graph_commit("branch2", &["branch2_mid"]),
            graph_commit("branch3", &["branch3_mid"]),
            graph_commit("branch1_mid", &["root"]),
            graph_commit("branch2_mid", &["root"]),
            graph_commit("branch3_mid", &["root"]),
            graph_commit("root", &[]),
        ];

        let lane_count = apply_git_graph_lanes(&mut commits);

        assert_eq!(lane_count, 4);
        assert_eq!(commits[0].column, 0);
        assert_eq!(commits[1].column, 0);
        assert_eq!(commits[2].column, 1);
        assert_eq!(commits[3].column, 2);
        assert_eq!(commits[4].column, 3);
        assert_eq!(commits[5].column, 1);
        assert_eq!(commits[6].column, 2);
        assert_eq!(commits[7].column, 3);
        assert_eq!(commits[8].column, 0);

        assert!(!has_graph_lane(
            &commits[0],
            GitGraphLaneKind::VerticalTop,
            commits[0].column
        ));
        for column in 1..=3 {
            assert!(has_graph_lane(
                &commits[0],
                GitGraphLaneKind::Parent,
                column
            ));
        }

        for idx in 2..=4 {
            assert!(has_graph_lane(
                &commits[idx],
                GitGraphLaneKind::VerticalTop,
                commits[idx].column
            ));
            assert!(has_graph_lane(
                &commits[idx],
                GitGraphLaneKind::VerticalBottom,
                commits[idx].column
            ));
        }
        for idx in 5..=7 {
            assert!(has_graph_lane(
                &commits[idx],
                GitGraphLaneKind::VerticalTop,
                commits[idx].column
            ));
            assert!(has_graph_lane(
                &commits[idx],
                GitGraphLaneKind::VerticalBottom,
                commits[idx].column
            ));
        }
        assert!(has_graph_lane(
            &commits[8],
            GitGraphLaneKind::VerticalTop,
            commits[8].column
        ));
        for idx in 1..=3 {
            assert!(commits[8].lanes.iter().any(|lane| {
                lane.kind == GitGraphLaneKind::ShiftToCommit
                    && usize::from(lane.column) == idx
                    && usize::from(lane.target_column) == commits[8].column
            }));
        }
        assert!(!has_graph_lane(
            &commits[8],
            GitGraphLaneKind::VerticalBottom,
            commits[8].column
        ));
    }

    #[test]
    fn git_file_status_maps_index_and_worktree_flags() {
        assert_eq!(
            git_file_status(git2::Status::WT_NEW, false),
            GitFileStatus::Untracked
        );
        assert_eq!(
            git_file_status(git2::Status::INDEX_NEW, true),
            GitFileStatus::Added
        );
        assert_eq!(
            git_file_status(git2::Status::WT_DELETED, false),
            GitFileStatus::Deleted
        );
        assert_eq!(
            git_file_status(git2::Status::INDEX_RENAMED, true),
            GitFileStatus::Renamed
        );
        assert_eq!(
            git_file_status(git2::Status::INDEX_TYPECHANGE, true),
            GitFileStatus::TypeChange
        );
        assert_eq!(
            git_file_status(git2::Status::WT_MODIFIED, false),
            GitFileStatus::Modified
        );
    }

    #[test]
    fn git_file_status_labels_match_editor_badges() {
        assert_eq!(GitFileStatus::Added.label(), "A");
        assert_eq!(GitFileStatus::Modified.label(), "M");
        assert_eq!(GitFileStatus::Deleted.label(), "D");
        assert_eq!(GitFileStatus::Renamed.label(), "R");
        assert_eq!(GitFileStatus::TypeChange.label(), "T");
        assert_eq!(GitFileStatus::Untracked.label(), "U");
    }

    #[test]
    fn stale_git_graph_disconnect_keeps_newer_request_pending() {
        let repo_root = PathBuf::from("/repo/current");
        let mut state = GitPanelState::default();
        state.seed_graph_request_for_test(repo_root.clone(), 2, true);

        state.handle_graph_disconnect(&repo_root, 1);

        assert!(state.graph_request_pending_for_test(&repo_root, 2));
        assert!(state.graph_pending);
        assert!(state.graph_notice.is_none());

        state.handle_graph_disconnect(&repo_root, 2);

        assert!(!state.graph_request_pending_for_test(&repo_root, 2));
        assert!(!state.graph_pending);
        assert_eq!(
            state.graph_notice.as_deref(),
            Some("Загрузка Git Graph неожиданно завершилась")
        );
    }
