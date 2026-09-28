
    #[test]
    fn full_scenario_uses_semantic_actions_and_covers_major_features() {
        let unique = format!("rriter-pgo-scenario-{}", std::process::id());
        let root = std::env::temp_dir().join(unique);
        let tests_dir = root.join("tests");
        std::fs::create_dir_all(&tests_dir).unwrap();
        std::fs::write(tests_dir.join("perf_alpha.py"), "value = 1\n").unwrap();
        std::fs::write(tests_dir.join("test_beta.py"), "value = 2\n").unwrap();

        let steps = full_pgo_scenario(&root);
        assert!(matches!(steps.first(), Some(AutomationStep::WaitReady)));
        assert!(matches!(steps.last(), Some(AutomationStep::Finish)));
        let setup_dart = steps
            .iter()
            .position(|step| matches!(step, AutomationStep::Dart(DartAutomationStep::Setup)))
            .unwrap();
        let apply_workspace = steps
            .iter()
            .position(|step| matches!(step, AutomationStep::ApplyWorkspace))
            .unwrap();
        let open_dart = steps
            .iter()
            .position(|step| {
                matches!(
                    step,
                    AutomationStep::OpenFile(path) if path == Path::new("lib/pgo_training.dart")
                )
            })
            .unwrap();
        let open_large = steps
            .iter()
            .position(|step| {
                matches!(
                    step,
                    AutomationStep::OpenFile(path) if path == Path::new("src/large.rs")
                )
            })
            .unwrap();
        let dart_steps = &steps[open_dart..open_large];
        assert!(setup_dart < apply_workspace);
        assert!(apply_workspace < open_dart);

        let open_worker = steps
            .iter()
            .position(|step| {
                matches!(
                    step,
                    AutomationStep::OpenFile(path) if path == Path::new("src/worker.py")
                )
            })
            .unwrap();
        let open_markdown = steps
            .iter()
            .position(|step| {
                matches!(
                    step,
                    AutomationStep::OpenFile(path) if path == Path::new("README.md")
                )
            })
            .unwrap();
        let return_to_main = steps
            .iter()
            .enumerate()
            .skip(open_markdown + 1)
            .find_map(|(index, step)| {
                matches!(
                    step,
                    AutomationStep::SwitchToFile(path) if path == Path::new("src/main.rs")
                )
                .then_some(index)
            })
            .unwrap();
        assert!(open_worker < open_markdown);
        assert!(open_markdown < return_to_main);
        assert!(
            steps[open_markdown..return_to_main]
                .iter()
                .any(|step| matches!(step, AutomationStep::Markdown(_)))
        );
        assert!(
            dart_steps
                .iter()
                .any(|step| matches!(step, AutomationStep::WaitHighlight))
        );
        assert_eq!(
            dart_steps
                .iter()
                .filter(|step| matches!(
                    step,
                    AutomationStep::Dart(DartAutomationStep::WaitClosingHints { minimum_count: 8 })
                ))
                .count(),
            2
        );
        assert_eq!(
            dart_steps
                .windows(2)
                .filter(|pair| matches!(
                    pair,
                    [
                        AutomationStep::Dart(DartAutomationStep::WaitClosingHints {
                            minimum_count: 8
                        }),
                        AutomationStep::WaitFrames(_)
                    ]
                ))
                .count(),
            2
        );
        assert!(
            dart_steps
                .iter()
                .any(|step| matches!(step, AutomationStep::ToggleFirstFold))
        );
        assert!(
            dart_steps
                .iter()
                .any(|step| matches!(step, AutomationStep::SetSearchQuery("pgoDartTarget")))
        );
        assert!(dart_steps.iter().any(|step| matches!(
            step,
            AutomationStep::ScrollEditorTimed { duration_secs: 10 }
        )));
        assert!(dart_steps.iter().any(|step| matches!(
            step,
            AutomationStep::JumpMinimap(fraction) if (*fraction - 0.18).abs() < f32::EPSILON
        )));
        assert!(dart_steps.iter().any(|step| matches!(
            step,
            AutomationStep::SetEditorCursorAfter("// pgoDartEditTarget")
        )));
        assert!(dart_steps.iter().any(|step| matches!(
            step,
            AutomationStep::TypeText(text) if text.contains("pgoDartEditedValue")
        )));
        assert!(
            dart_steps
                .iter()
                .any(|step| matches!(step, AutomationStep::SaveCurrentFile))
        );
        assert!(steps.iter().any(|step| matches!(
            step,
            AutomationStep::LoadGitGraph {
                min_commits: GIT_FIXTURE_COMMIT_COUNT
            }
        )));
        assert!(steps.iter().any(|step| matches!(
            step,
            AutomationStep::ScrollGitGraphTimed { duration_secs: 24 }
        )));
        assert!(
            steps
                .iter()
                .any(|step| matches!(step, AutomationStep::TriggerAutocomplete("print")))
        );
        assert!(steps.iter().any(|step| matches!(
            step,
            AutomationStep::SetEditorCursorAfter("pgo_completion_result = pri")
        )));
        assert!(
            steps
                .iter()
                .any(|step| matches!(step, AutomationStep::ShowHover { .. }))
        );
        assert!(!steps.iter().any(|step| step.name() == "wait-hover"));
        assert!(steps.iter().any(|step| matches!(
            step,
            AutomationStep::SetApiAuthValue {
                scheme: "BearerAuth",
                value: "rriter-pgo-bearer-token"
            }
        )));
        assert!(steps.iter().any(|step| matches!(
            step,
            AutomationStep::WaitApiResponse {
                expected_status: 200,
                body_marker: "RRITER_PGO_LOCAL_API_OK"
            }
        )));
        if cfg!(target_os = "linux") {
            assert!(
                steps
                    .iter()
                    .any(|step| matches!(step, AutomationStep::RunTerminalHtop))
            );
            assert!(
                steps
                    .iter()
                    .any(|step| matches!(step, AutomationStep::WaitTerminalHtopVisible))
            );
            assert!(
                steps
                    .iter()
                    .any(|step| matches!(step, AutomationStep::WaitTerminalHtopExit))
            );
            assert!(
                steps
                    .iter()
                    .any(|step| matches!(step, AutomationStep::WaitMillis(10_000)))
            );
        } else {
            assert!(
                steps
                    .iter()
                    .any(|step| matches!(step, AutomationStep::RunTerminalBasicCommand))
            );
            assert!(
                steps
                    .iter()
                    .any(|step| matches!(step, AutomationStep::WaitTerminalBasicCommandVisible))
            );
            assert!(
                !steps
                    .iter()
                    .any(|step| matches!(step, AutomationStep::RunTerminalHtop))
            );
        }
        assert!(
            !steps
                .iter()
                .any(|step| step.name().contains("terminal-listings"))
        );
        assert!(
            !steps
                .iter()
                .any(|step| step.name().contains("terminal-ready"))
        );
        for required in [
            DatabaseAutomationStep::SetupConnection,
            DatabaseAutomationStep::WaitCatalog,
            DatabaseAutomationStep::WaitTables,
            DatabaseAutomationStep::WaitDdl,
            DatabaseAutomationStep::WaitTable,
            DatabaseAutomationStep::WaitTableReview,
            DatabaseAutomationStep::WaitTableTransactionFinished,
            DatabaseAutomationStep::WaitQueryCompletion,
            DatabaseAutomationStep::WaitQueryResult,
            DatabaseAutomationStep::WaitExplain,
        ] {
            assert!(steps.iter().any(|step| matches!(
                step,
                AutomationStep::Database(candidate) if *candidate == required
            )));
        }
        assert!(steps.iter().any(|step| matches!(
            step,
            AutomationStep::Database(DatabaseAutomationStep::ScrollTableTimed { duration_secs: 8 })
        )));
        assert!(steps.iter().any(|step| matches!(
            step,
            AutomationStep::Database(DatabaseAutomationStep::ScrollQueryResultTimed {
                duration_secs: 8
            })
        )));
        let ddl_index = steps
            .iter()
            .position(|step| {
                matches!(
                    step,
                    AutomationStep::Database(DatabaseAutomationStep::LoadDdl)
                )
            })
            .unwrap();
        assert!(matches!(
            steps.get(ddl_index + 1),
            Some(AutomationStep::Database(DatabaseAutomationStep::WaitDdl))
        ));
        assert!(matches!(
            steps.get(ddl_index + 2),
            Some(AutomationStep::WaitFrames(6))
        ));
        assert!(matches!(
            steps.get(ddl_index + 3),
            Some(AutomationStep::Database(DatabaseAutomationStep::DismissDdl))
        ));
        assert!(matches!(
            steps.get(ddl_index + 4),
            Some(AutomationStep::Database(DatabaseAutomationStep::OpenTable))
        ));

        let explain_index = steps
            .iter()
            .position(|step| {
                matches!(
                    step,
                    AutomationStep::Database(DatabaseAutomationStep::WaitExplain)
                )
            })
            .unwrap();
        assert!(matches!(
            steps.get(explain_index + 1),
            Some(AutomationStep::WaitFrames(6))
        ));
        assert!(matches!(
            steps.get(explain_index + 2),
            Some(AutomationStep::Database(DatabaseAutomationStep::AssertIdle))
        ));
        assert!(matches!(
            steps.get(explain_index + 3),
            Some(AutomationStep::OpenPanel(PanelId::Explorer))
        ));

        let problems_index = steps
            .iter()
            .rposition(|step| matches!(step, AutomationStep::OpenPanel(PanelId::Problems)))
            .unwrap();
        assert!(matches!(
            steps.get(problems_index + 1),
            Some(AutomationStep::WaitFrames(8))
        ));
        assert!(matches!(
            steps.get(problems_index + 2),
            Some(AutomationStep::OpenPanel(PanelId::Terminal))
        ));
        assert!(matches!(
            steps.get(problems_index + 3),
            Some(AutomationStep::WaitTerminal)
        ));
        if cfg!(target_os = "linux") {
            assert!(matches!(
                steps.get(problems_index + 4),
                Some(AutomationStep::RunTerminalHtop)
            ));
            assert!(matches!(
                steps.get(problems_index + 5),
                Some(AutomationStep::WaitTerminalHtopVisible)
            ));
            assert!(matches!(
                steps.get(problems_index + 6),
                Some(AutomationStep::WaitMillis(10_000))
            ));
            assert!(matches!(
                steps.get(problems_index + 7),
                Some(AutomationStep::InterruptTerminal)
            ));
            assert!(matches!(
                steps.get(problems_index + 8),
                Some(AutomationStep::WaitTerminalHtopExit)
            ));
        }

        assert!(steps.iter().any(|step| matches!(
            step,
            AutomationStep::AddSettingsIgnore(".rriter-pgo-ignore/**")
        )));
        for panel in [
            PanelId::Explorer,
            PanelId::Git,
            PanelId::ApiClient,
            PanelId::Database,
            PanelId::Terminal,
            PanelId::Problems,
            PanelId::LspServers,
        ] {
            assert!(steps.iter().any(|step| matches!(
                step,
                AutomationStep::OpenPanel(candidate) if *candidate == panel
            )));
        }
        assert!(steps.iter().any(|step| matches!(
            step,
            AutomationStep::SetProjectSearchQuery("RRITER_PGO_AUTOMATION_MARKER")
        )));
        for tab in 0..=4 {
            assert!(steps.iter().any(|step| matches!(
                step,
                AutomationStep::SetSettingsTab(candidate) if *candidate == tab
            )));
        }
        assert!(steps.iter().any(|step| matches!(
            step,
            AutomationStep::OpenFile(path) if path == Path::new("tests/perf_alpha.py")
        )));
        assert!(steps.iter().any(|step| matches!(
            step,
            AutomationStep::OpenFile(path) if path == Path::new("tests/test_beta.py")
        )));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn step_start_marker_is_emitted_once_until_step_advances() {
        let unique = format!("rriter-pgo-step-start-{}", std::process::id());
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(root.join("tests")).unwrap();
        let mut controller = AutomationController::new(AutomationOptions {
            workspace: root.clone(),
            report_path: root.join("automation-report.json"),
            timeout: Duration::from_secs(30),
        });
        let first = controller.steps[0].clone();
        assert!(controller.log_step_start(&first));
        assert!(!controller.log_step_start(&first));
        controller.advance(first.name(), Instant::now());
        let second = controller.steps[1].clone();
        assert!(controller.log_step_start(&second));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn timeout_report_keeps_backward_failure_and_structured_metadata() {
        let unique = format!("rriter-pgo-failure-report-{}", std::process::id());
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(root.join("tests")).unwrap();
        let report_path = root.join("automation-report.json");
        let mut controller = AutomationController::new(AutomationOptions {
            workspace: root.clone(),
            report_path: report_path.clone(),
            timeout: Duration::from_secs(30),
        });
        controller.step_index = 1;
        controller.completed.push("wait-8-frames".to_string());
        let now = Instant::now();
        controller.started_at = now - Duration::from_millis(900);
        controller.step_started_at = now - Duration::from_millis(125);
        let step_name = controller.steps[controller.step_index].name();

        assert_eq!(
            controller.fail_and_exit(
                step_name.clone(),
                "step timeout after 0.1s".to_string(),
                Some("diagnostic-context".to_string()),
                now,
                AutomationFailureKind::Timeout,
            ),
            AutomationTick::Exit
        );

        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&report_path).unwrap()).unwrap();
        assert_eq!(report["failed_step"], "step timeout after 0.1s");
        assert_eq!(report["failed_step_index"], 1);
        assert_eq!(report["failed_step_name"], step_name);
        assert_eq!(report["failed_step_elapsed_ms"], 125);
        assert_eq!(report["failure_reason"], "step timeout after 0.1s");
        assert_eq!(report["previous_completed_step"], "wait-8-frames");
        assert_eq!(report["failure_context"], "diagnostic-context");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_step_metadata_tracks_index_name_reason_elapsed_and_previous_step() {
        let unique = format!("rriter-pgo-failure-metadata-{}", std::process::id());
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(root.join("tests")).unwrap();
        let report_path = root.join("automation-report.json");
        let mut controller = AutomationController::new(AutomationOptions {
            workspace: root.clone(),
            report_path: report_path.clone(),
            timeout: Duration::from_secs(30),
        });
        controller.step_index = 2;
        controller.completed.push("previous-step".to_string());
        let now = Instant::now();
        controller.started_at = now - Duration::from_millis(500);
        controller.step_started_at = now - Duration::from_millis(42);
        let step_name = controller.steps[controller.step_index].name();

        assert_eq!(
            controller.fail_and_exit(
                step_name.clone(),
                "forced failure".to_string(),
                Some("state-context".to_string()),
                now,
                AutomationFailureKind::Failed,
            ),
            AutomationTick::Exit
        );
        let failure = controller.failure.as_ref().unwrap();
        assert_eq!(failure.index, 2);
        assert_eq!(failure.name, step_name);
        assert_eq!(failure.reason, "forced failure");
        assert_eq!(failure.step_elapsed_ms, 42);
        assert_eq!(
            failure.previous_completed_step.as_deref(),
            Some("previous-step")
        );
        assert_eq!(failure.context.as_deref(), Some("state-context"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn terminal_open_panel_waits_for_existing_open_when_ready_lifecycle() {
        use crate::app::terminal::TerminalPresentationIntent;

        assert_eq!(
            terminal_panel_open_state_from_values(
                false,
                [(TerminalPresentationIntent::OpenPanelWhenReady, false)],
            ),
            TerminalPanelOpenState::WaitingForPresentation
        );
        assert_eq!(
            terminal_panel_open_state_from_values(
                false,
                [(TerminalPresentationIntent::OpenPanelWhenReady, true)],
            ),
            TerminalPanelOpenState::WaitingForPresentation
        );
        assert_eq!(
            terminal_panel_open_state_from_values(true, [(TerminalPresentationIntent::None, true)],),
            TerminalPanelOpenState::Open
        );
        assert_eq!(
            terminal_panel_open_state_from_values(
                false,
                [(TerminalPresentationIntent::ActivateWhenReady, false)],
            ),
            TerminalPanelOpenState::ClosedUnexpectedly
        );
    }

    #[test]
    fn terminal_open_panel_semantic_action_runs_only_on_first_poll() {
        let mut step_progress = 0;
        assert!(begin_open_panel_action(&mut step_progress));
        assert_eq!(step_progress, 1);
        assert!(!begin_open_panel_action(&mut step_progress));
        assert_eq!(step_progress, 1);
    }

    #[test]
    fn terminal_tui_state_requires_alt_screen_and_visible_content() {
        let mut grid = crate::app::terminal::TermGrid::new(12, 3);
        assert_eq!(
            terminal_grid_tui_state(&grid),
            TerminalTuiState {
                alternate_screen: false,
                non_blank_cells: 0,
            }
        );
        grid.is_alt = true;
        grid.lines[0][0].c = 'h';
        grid.lines[0][1].c = 't';
        grid.lines[0][2].c = 'o';
        grid.lines[0][3].c = 'p';
        assert_eq!(
            terminal_grid_tui_state(&grid),
            TerminalTuiState {
                alternate_screen: true,
                non_blank_cells: 4,
            }
        );
    }

    #[test]
    fn terminal_basic_output_is_detected_in_visible_or_scrollback_lines() {
        let mut grid = crate::app::terminal::TermGrid::new(16, 2);
        assert!(!terminal_grid_contains(&grid, TERMINAL_BASIC_OUTPUT));
        for (index, ch) in TERMINAL_BASIC_OUTPUT.chars().enumerate() {
            grid.lines[0][index].c = ch;
        }
        assert!(terminal_grid_contains(&grid, TERMINAL_BASIC_OUTPUT));

        grid.scrollback.push_back(grid.lines[0].clone());
        grid.lines[0].fill(crate::app::terminal::Cell::default());
        assert!(terminal_grid_contains(&grid, TERMINAL_BASIC_OUTPUT));
    }

    #[test]
    fn terminal_workload_is_htop_only_on_linux() {
        let linux = terminal_workload_steps_for("linux");
        assert!(
            linux
                .iter()
                .any(|step| matches!(step, AutomationStep::RunTerminalHtop))
        );
        assert!(
            linux
                .iter()
                .any(|step| matches!(step, AutomationStep::WaitMillis(10_000)))
        );
        assert!(
            !linux
                .iter()
                .any(|step| matches!(step, AutomationStep::RunTerminalBasicCommand))
        );

        for os in ["macos", "windows"] {
            let steps = terminal_workload_steps_for(os);
            assert_eq!(steps.len(), 2);
            assert!(matches!(steps[0], AutomationStep::RunTerminalBasicCommand));
            assert!(matches!(
                steps[1],
                AutomationStep::WaitTerminalBasicCommandVisible
            ));
            assert!(
                !steps
                    .iter()
                    .any(|step| matches!(step, AutomationStep::RunTerminalHtop))
            );
        }
    }

    #[test]
    fn automation_source_has_no_coordinate_or_ui_click_driver() {
        // automation.rs include!s its controller steps and semantic actions.
        let source = concat!(
            include_str!("automation.rs"),
            include_str!("automation_controller_steps.rs"),
            include_str!("automation_semantic_actions.rs"),
            include_str!("automation_fixtures.rs"),
        );
        assert!(!source.contains(concat!("handle_", "ui_click")));
        assert!(!source.contains(concat!("Physical", "Position")));
        assert!(!source.contains(concat!("Mouse", "Button")));
        assert!(!source.contains(concat!("MouseScroll", "Delta")));
        assert!(!source.contains(concat!("ui_", "registry")));
        assert!(!source.contains(concat!("rect_", "for")));
    }

    #[test]
    fn scenario_has_no_optional_steps() {
        let steps = full_pgo_scenario(Path::new("/nonexistent"));
        let optional = steps
            .iter()
            .filter(|step| step.optional())
            .map(AutomationStep::name)
            .collect::<Vec<_>>();
        assert!(optional.is_empty());
    }

    #[test]
    fn fixture_repository_contains_thousand_commits_and_many_branches() {
        let unique = format!(
            "rriter-pgo-fixture-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();

        ensure_fixture_repository(&root).unwrap();
        let repository = git2::Repository::open(&root).unwrap();
        assert_eq!(
            fixture_head_commit_count(&repository),
            GIT_FIXTURE_COMMIT_COUNT
        );
        let branch_count = repository
            .branches(Some(git2::BranchType::Local))
            .unwrap()
            .count();
        assert_eq!(branch_count, GIT_FIXTURE_BRANCH_COUNT + 1);
        let mut walk = repository.revwalk().unwrap();
        walk.push_head().unwrap();
        let merge_count = walk
            .filter_map(Result::ok)
            .filter(|oid| {
                repository
                    .find_commit(*oid)
                    .is_ok_and(|commit| commit.parent_count() > 1)
            })
            .count();
        assert_eq!(merge_count, GIT_FIXTURE_BRANCH_COUNT);
        let first = repository.head().unwrap().target().unwrap();
        ensure_fixture_repository(&root).unwrap();
        let second = repository.head().unwrap().target().unwrap();
        assert_eq!(first, second);

        drop(repository);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn python_fixture_discovery_is_size_ordered_and_excludes_completion_fixture() {
        let root = std::env::temp_dir().join(format!("rriter-pgo-python-{}", std::process::id()));
        let tests = root.join("tests");
        std::fs::create_dir_all(&tests).unwrap();
        std::fs::write(tests.join("small.py"), "x\n").unwrap();
        std::fs::write(tests.join("large.py"), "x".repeat(100)).unwrap();
        std::fs::write(tests.join("pgo_completion_hover.py"), "fixture\n").unwrap();
        let files = fixture_python_tests(&root);
        assert_eq!(
            files,
            vec![
                PathBuf::from("tests/large.py"),
                PathBuf::from("tests/small.py")
            ]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    fn ready_hover_prerequisites() -> HoverPrerequisites {
        HoverPrerequisites {
            active_file_matches: true,
            autocomplete_inactive: true,
            editor_clean: true,
            highlight_current: true,
            external_changes_idle: true,
            renderer_ready: true,
        }
    }

    #[test]
    fn completion_hover_sequence_is_preserved() {
        let root =
            std::env::temp_dir().join(format!("rriter-pgo-hover-sequence-{}", std::process::id()));
        std::fs::create_dir_all(root.join("tests")).unwrap();
        let names = full_pgo_scenario(&root)
            .into_iter()
            .map(|step| step.name())
            .collect::<Vec<_>>();
        let expected = [
            "open-file:tests/pgo_completion_hover.py",
            "wait-highlight",
            "set-cursor-after:pgo_completion_result = pri",
            "trigger-autocomplete:print",
            "wait-900ms",
            "select-autocomplete:print",
            "wait-900ms",
            "apply-autocomplete:print",
            "save-current-file",
            "show-hover:pgo_hover_target",
            "scroll-hover-timed:5s",
            "clear-hover",
        ];
        assert!(names.windows(expected.len()).any(|window| {
            window
                .iter()
                .map(String::as_str)
                .eq(expected.iter().copied())
        }));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hover_waits_for_external_change_worker_before_install() {
        let mut prerequisites = ready_hover_prerequisites();
        prerequisites.external_changes_idle = false;
        assert_eq!(
            hover_progress_action(prerequisites, (false, false), 0),
            HoverProgressAction::WaitStable
        );
    }

    #[test]
    fn hover_waits_for_current_highlight_revision_before_install() {
        assert!(!hover_highlight_is_current(true, true, 6, 7));
        assert!(!hover_highlight_is_current(false, true, 7, 7));
        assert!(!hover_highlight_is_current(true, false, 7, 7));
        assert!(hover_highlight_is_current(true, true, 7, 7));

        let mut prerequisites = ready_hover_prerequisites();
        prerequisites.highlight_current = false;
        assert_eq!(
            hover_progress_action(prerequisites, (false, false), 0),
            HoverProgressAction::WaitStable
        );
        prerequisites.highlight_current = true;
        assert_eq!(
            hover_progress_action(prerequisites, (false, false), 0),
            HoverProgressAction::Install
        );
    }

    #[test]
    fn hover_pointer_contract_uses_source_geometry_and_production_mouse_gate() {
        let source = include_str!("automation_semantic_actions.rs");
        let start = source
            .find("fn prepare_automation_hover_pointer")
            .expect("automation hover pointer helper");
        let tail = &source[start..];
        let end = tail
            .find("fn install_automation_hover_popup")
            .expect("automation hover install helper");
        let helper = &tail[..end];
        assert!(helper.contains("editor_top_inset"));
        assert!(helper.contains("scroll_y.current.round()"));
        assert!(helper.contains("hover_anchor_for_byte"));
        assert!(helper.contains("renderer.last_mouse_x = anchor.0"));
        assert!(helper.contains("renderer.last_mouse_y = anchor.1"));
        assert!(helper.contains("renderer.update_popup_mouse_move_gate()"));
        assert!(!helper.contains("hide_popups_until_mouse_move = false"));
        assert!(!helper.contains("340.0"));
        assert!(!helper.contains("set_cursor_position"));
    }

    #[test]
    fn hover_transient_clear_can_wait_and_reinstall_then_finish() {
        let ready = ready_hover_prerequisites();
        assert_eq!(
            hover_progress_action(ready, (false, false), 0),
            HoverProgressAction::Install
        );

        let popup = crate::app::events::source_hover_popup_for_editor(
            &crate::editor::Editor::new(64),
            0,
            "hover text".to_string(),
            None,
            (12.0, 24.0),
        );
        install_automation_hover_popup(0, popup);
        assert!(crate::app::mouse::clear_hover_popup(None));

        let mut transient = ready;
        transient.external_changes_idle = false;
        assert_eq!(
            hover_progress_action(transient, (false, false), 1),
            HoverProgressAction::WaitStable
        );
        assert_eq!(
            hover_progress_action(ready, (false, false), 1),
            HoverProgressAction::Install
        );

        let mut state = crate::app::mouse::HoverState::default();
        let popup = crate::app::events::source_hover_popup_for_editor(
            &crate::editor::Editor::new(64),
            0,
            "hover text".to_string(),
            None,
            (12.0, 24.0),
        );
        state.byte_offset = Some(0);
        state.popup = Some(popup);
        state.rect = Some((1.0, 2.0, 3.0, 4.0));
        assert_eq!(
            hover_progress_action(ready, hover_popup_status(&state, 0), 2),
            HoverProgressAction::Done
        );
    }

    #[test]
    fn hover_persistent_clear_fails_after_bounded_reinstalls() {
        assert_eq!(
            hover_progress_action(
                ready_hover_prerequisites(),
                (false, false),
                PGO_HOVER_MAX_INSTALL_ATTEMPTS,
            ),
            HoverProgressAction::FailRepeatedClear
        );
    }

    #[test]
    fn synthetic_hover_install_has_no_lsp_request_dependency() {
        let popup = crate::app::events::source_hover_popup_for_editor(
            &crate::editor::Editor::new(64),
            0,
            "hover text".to_string(),
            None,
            (12.0, 24.0),
        );
        install_automation_hover_popup(0, popup);
        crate::app::mouse::HOVER_STATE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.request_id, None);
            assert_eq!(state.definition_request_id, None);
            assert_eq!(state.byte_offset, Some(0));
            assert!(state.popup.is_some());
        });
        crate::app::mouse::clear_hover_popup(None);
    }

    #[test]
    fn hover_step_waits_for_a_matching_popup_to_be_drawn() {
        let mut state = crate::app::mouse::HoverState::default();
        let popup = crate::app::events::source_hover_popup_for_editor(
            &crate::editor::Editor::new(64),
            0,
            "hover text".to_string(),
            None,
            (0.0, 0.0),
        );
        state.byte_offset = Some(0);
        state.popup = Some(popup);
        assert_eq!(hover_popup_status(&state, 0), (true, false));
        state.rect = Some((1.0, 2.0, 3.0, 4.0));
        assert_eq!(hover_popup_status(&state, 0), (true, true));
        assert_eq!(hover_popup_status(&state, 1), (false, false));
    }

    #[test]
    fn interrupted_automation_writes_the_current_step_report() {
        let root = std::env::temp_dir().join(format!(
            "rriter-pgo-interrupted-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let report_path = root.join("report.json");
        let mut controller = AutomationController::new(AutomationOptions {
            workspace: root.clone(),
            report_path: report_path.clone(),
            timeout: Duration::from_secs(1),
        });
        controller.write_interrupted_report("test shutdown");
        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&report_path).unwrap()).unwrap();
        assert_eq!(report["status"], "failed");
        assert!(
            report["failed_step"]
                .as_str()
                .is_some_and(|message| message.contains("current_step=wait-ready"))
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn timed_scroll_uses_wall_clock_pause_and_reverse_motion() {
        let forward = timed_scroll_plan(1.0, 22);
        assert_eq!(forward.expected_impulses, 120);
        assert_eq!(forward.direction, 1.0);
        assert!(!forward.done);

        let pause = timed_scroll_plan(11.0, 22);
        assert_eq!(pause.expected_impulses, 1200);
        assert_eq!(pause.direction, 0.0);

        let reverse = timed_scroll_plan(13.0, 22);
        assert_eq!(reverse.expected_impulses, 1320);
        assert_eq!(reverse.direction, -1.0);

        let done = timed_scroll_plan(22.0, 22);
        assert_eq!(done.expected_impulses, 2400);
        assert!(done.done);
    }
