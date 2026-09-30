fn fixture_commit(
    repository: &git2::Repository,
    tree: &git2::Tree<'_>,
    parents: &[git2::Oid],
    message: &str,
    timestamp: i64,
) -> Result<git2::Oid, String> {
    let time = git2::Time::new(timestamp, 0);
    let signature = git2::Signature::new("RRiter PGO", "pgo@rriter.invalid", &time)
        .map_err(|error| format!("failed to create fixture Git signature: {error}"))?;
    let parent_commits = parents
        .iter()
        .map(|oid| repository.find_commit(*oid))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to load fixture Git parent: {error}"))?;
    let parent_refs = parent_commits.iter().collect::<Vec<_>>();
    repository
        .commit(None, &signature, &signature, message, tree, &parent_refs)
        .map_err(|error| format!("failed to create fixture Git commit: {error}"))
}

fn fixture_head_commit_count(repository: &git2::Repository) -> usize {
    let Ok(mut walk) = repository.revwalk() else {
        return 0;
    };
    if walk.push_head().is_err() {
        return 0;
    }
    walk.filter_map(Result::ok).count()
}

pub(crate) fn ensure_fixture_repository(workspace: &Path) -> Result<(), String> {
    if workspace.join(".git").exists() {
        let repository = git2::Repository::open(workspace)
            .map_err(|error| format!("failed to open fixture Git repository: {error}"))?;
        if fixture_head_commit_count(&repository) >= GIT_FIXTURE_COMMIT_COUNT {
            return Ok(());
        }
        return Err(
            "fixture Git repository exists but does not contain the 1000-commit graph".to_string(),
        );
    }

    let repository = git2::Repository::init(workspace)
        .map_err(|error| format!("failed to initialize fixture Git repository: {error}"))?;
    let mut index = repository
        .index()
        .map_err(|error| format!("failed to open fixture Git index: {error}"))?;
    index
        .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
        .map_err(|error| format!("failed to stage fixture files: {error}"))?;
    index
        .write()
        .map_err(|error| format!("failed to write fixture Git index: {error}"))?;
    let tree_id = index
        .write_tree()
        .map_err(|error| format!("failed to write fixture Git tree: {error}"))?;
    let tree = repository
        .find_tree(tree_id)
        .map_err(|error| format!("failed to load fixture Git tree: {error}"))?;

    let mut timestamp = 1_700_000_000i64;
    let root = fixture_commit(
        &repository,
        &tree,
        &[],
        "Create deterministic PGO fixture",
        timestamp,
    )?;
    let mut main_tip = root;
    for branch_index in 0..GIT_FIXTURE_BRANCH_COUNT {
        let branch_base = main_tip;
        for main_index in 0..13 {
            timestamp += 1;
            main_tip = fixture_commit(
                &repository,
                &tree,
                &[main_tip],
                &format!("main cycle {branch_index:02} commit {main_index:02}"),
                timestamp,
            )?;
        }
        let mut feature_tip = branch_base;
        for feature_index in 0..5 {
            timestamp += 1;
            feature_tip = fixture_commit(
                &repository,
                &tree,
                &[feature_tip],
                &format!("feature {branch_index:02} commit {feature_index:02}"),
                timestamp,
            )?;
        }
        repository
            .reference(
                &format!("refs/heads/feature-{branch_index:02}"),
                feature_tip,
                true,
                "RRiter PGO feature branch",
            )
            .map_err(|error| format!("failed to create fixture Git branch: {error}"))?;
        timestamp += 1;
        main_tip = fixture_commit(
            &repository,
            &tree,
            &[main_tip, feature_tip],
            &format!("merge feature-{branch_index:02}"),
            timestamp,
        )?;
    }
    for tail_index in 0..49 {
        timestamp += 1;
        main_tip = fixture_commit(
            &repository,
            &tree,
            &[main_tip],
            &format!("main tail commit {tail_index:02}"),
            timestamp,
        )?;
    }
    repository
        .reference("refs/heads/main", main_tip, true, "RRiter PGO main branch")
        .map_err(|error| format!("failed to update fixture Git main branch: {error}"))?;
    repository
        .set_head("refs/heads/main")
        .map_err(|error| format!("failed to set fixture Git HEAD: {error}"))?;

    let count = fixture_head_commit_count(&repository);
    if count != GIT_FIXTURE_COMMIT_COUNT {
        return Err(format!(
            "fixture Git graph has {count} commits, expected {GIT_FIXTURE_COMMIT_COUNT}"
        ));
    }
    Ok(())
}

fn fixture_python_tests(workspace: &Path) -> Vec<PathBuf> {
    let tests_dir = workspace.join("tests");
    let Ok(entries) = std::fs::read_dir(tests_dir) else {
        return Vec::new();
    };
    let mut files = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().and_then(|extension| extension.to_str()) == Some("py")
                && path.file_name().and_then(|name| name.to_str())
                    != Some("pgo_completion_hover.py")
        })
        .filter_map(|path| {
            let size = path.metadata().ok()?.len();
            Some((size, PathBuf::from("tests").join(path.file_name()?)))
        })
        .collect::<Vec<_>>();
    files.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    files.into_iter().map(|(_, path)| path).collect()
}

fn scenario_steps(
    scenario: &PgoScenario,
    workspace: &Path,
) -> Result<Vec<AutomationStep>, String> {
    match scenario {
        PgoScenario::Full => Ok(full_pgo_scenario(workspace)),
        PgoScenario::Smoke => Ok(vec![
            AutomationStep::WaitReady,
            AutomationStep::ResizeWindow {
                width: 1600,
                height: 900,
            },
            AutomationStep::WaitFrames(3),
            AutomationStep::Finish,
        ]),
        PgoScenario::Group(name) => {
            let group = crate::app::automation_groups::group_steps(name, workspace)
                .ok_or_else(|| format!("unknown PGO group {name:?}"))?;
            let mut steps = vec![AutomationStep::WaitReady];
            steps.extend(group);
            steps.push(AutomationStep::Finish);
            Ok(steps)
        }
        PgoScenario::Startup | PgoScenario::Welcome => Err(format!(
            "PGO scenario {} is not implemented",
            scenario.as_str()
        )),
    }
}

fn full_pgo_scenario(workspace: &Path) -> Vec<AutomationStep> {
    use AutomationStep as S;
    let mut steps = vec![
        S::WaitReady,
        S::ResizeWindow {
            width: 1280,
            height: 800,
        },
        S::Dart(DartAutomationStep::Setup),
        S::ApplyWorkspace,
        S::WaitFileTree,
        S::OpenPanel(PanelId::Explorer),
        S::ExpandWorkspaceRoot,
        S::WaitFileTree,
        S::OpenFile(PathBuf::from("src/main.rs")),
        S::WaitHighlight,
        S::ToggleFirstFold,
        S::WaitMillis(350),
        S::ToggleFirstFold,
        S::WaitMillis(350),
        S::OpenFile(PathBuf::from("src/worker.py")),
        S::WaitHighlight,
        S::WaitMillis(350),
    ];
    steps.extend(crate::app::automation_markdown::markdown_scenario_steps());
    steps.extend([
        S::SwitchToFile(PathBuf::from("src/main.rs")),
        S::WaitFrames(2),
        S::SwitchToFile(PathBuf::from("src/worker.py")),
        S::OpenActiveTabContext,
        S::WaitMillis(750),
        S::CloseContextMenu,
        S::FocusEditor,
        S::SetEditorCursorAfter("return sum(item.weight for item in items)"),
        S::TypeText("\n# RRITER_PGO_AUTOMATION_MARKER\nmessage = 'Привет PGO 🚀'\n"),
        S::WaitHighlight,
        S::SaveCurrentFile,
        S::OpenSearch,
        S::SetSearchQuery("RRITER_PGO_AUTOMATION_MARKER"),
        S::ToggleSearchCase,
        S::NextSearchResult,
        S::WaitMillis(250),
        S::PreviousSearchResult,
        S::WaitMillis(250),
        S::CloseSearch,
        S::OpenFile(PathBuf::from("lib/pgo_training.dart")),
        S::WaitHighlight,
        S::Dart(DartAutomationStep::WaitClosingHints { minimum_count: 8 }),
        S::WaitFrames(8),
        S::ToggleFirstFold,
        S::WaitFrames(4),
        S::ToggleFirstFold,
        S::OpenSearch,
        S::SetSearchQuery("pgoDartTarget"),
        S::NextSearchResult,
        S::PreviousSearchResult,
        S::CloseSearch,
        S::WaitFrames(8),
        S::ScrollEditorTimed { duration_secs: 10 },
        S::JumpMinimap(0.18),
        S::WaitFrames(6),
        S::SetEditorCursorAfter("// pgoDartEditTarget"),
        S::FocusEditor,
        S::TypeText("\n  final int pgoDartEditedValue = pgoDartTargetValue + 1;"),
        S::WaitHighlight,
        S::Dart(DartAutomationStep::WaitClosingHints { minimum_count: 8 }),
        S::WaitFrames(8),
        S::SaveCurrentFile,
        S::WaitHighlight,
        S::JumpMinimap(0.05),
        S::WaitFrames(8),
        S::ScrollEditorTimed { duration_secs: 10 },
        S::OpenFile(PathBuf::from("src/large.rs")),
        S::WaitHighlight,
        S::ScrollEditorTimed { duration_secs: 22 },
        S::JumpMinimap(0.72),
        S::WaitMillis(500),
    ]);

    for python_test in fixture_python_tests(workspace) {
        let should_scroll = python_test
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("perf_"));
        steps.push(S::OpenFile(python_test));
        steps.push(S::WaitHighlight);
        steps.push(S::WaitMillis(300));
        if should_scroll {
            steps.push(S::ScrollEditorTimed { duration_secs: 5 });
        }
    }

    steps.extend([
        S::OpenFile(PathBuf::from("tests/pgo_completion_hover.py")),
        S::WaitHighlight,
        S::SetEditorCursorAfter("pgo_completion_result = pri"),
        S::TriggerAutocomplete("print"),
        S::WaitMillis(900),
        S::SelectAutocomplete("print"),
        S::WaitMillis(900),
        S::ApplyAutocomplete("print"),
        S::SaveCurrentFile,
        S::ShowHover {
            needle: "pgo_hover_target",
            text: "async def pgo_hover_target(model: PgoCompletionModel) -> dict[str, int]\n\nDeterministic source hover used to train layout, syntax spans, inline code and scrolling.\n\n- Resolves a Python model\n- Produces a normalized mapping\n- Exercises the complete hover renderer\n\n`PgoCompletionModel` is defined in the same fixture module.",
        },
        S::ScrollHoverTimed { duration_secs: 5 },
        S::ClearHover,
        S::SetProjectSearchQuery("RRITER_PGO_AUTOMATION_MARKER"),
        S::RunProjectSearch,
        S::WaitProjectSearch,
        S::JumpFirstProjectSearchMatch,
        S::OpenPanel(PanelId::Git),
        S::WaitGit,
        S::WaitMillis(750),
        S::ToggleGitGraph,
        S::WaitGitGraph,
        S::LoadGitGraph { min_commits: GIT_FIXTURE_COMMIT_COUNT },
        S::WaitMillis(1000),
        S::ScrollGitGraphTimed { duration_secs: 24 },
        S::OpenPanel(PanelId::ApiClient),
        S::WaitMillis(500),
        S::ImportApiSpec,
        S::WaitApiSpec,
        S::WaitMillis(1000),
        S::WaitApiRoutesPanel,
        S::ScrollApiRoutesTimed { duration_secs: 12 },
        S::ResetApiPanelScroll,
        S::WaitMillis(500),
        S::SetApiRouteFilter("PGO_FEATURED_WRITE"),
        S::WaitApiRouteFilter("PGO_FEATURED_WRITE"),
        S::WaitMillis(500),
        S::OpenApiRouteMatching("PGO_FEATURED_WRITE"),
        S::WaitApiRouteOpen("PGO_FEATURED_WRITE"),
        S::WaitMillis(750),
        S::ScrollApiTabTimed { duration_secs: 10 },
        S::ResetApiTabScroll,
        S::ResetApiPanelScroll,
        S::ClearApiRouteFilter,
        S::WaitMillis(500),
        S::OpenApiAuth,
        S::WaitMillis(750),
        S::FocusApiAuth("BearerAuth"),
        S::SetApiAuthValue {
            scheme: "BearerAuth",
            value: "rriter-pgo-bearer-token",
        },
        S::SaveApiAuth { scheme: "BearerAuth", value: "rriter-pgo-bearer-token" },
        S::WaitMillis(500),
        S::SetApiRouteFilter("PGO_LOCAL_SERVER_PING"),
        S::WaitApiRouteFilter("PGO_LOCAL_SERVER_PING"),
        S::OpenApiRouteMatching("PGO_LOCAL_SERVER_PING"),
        S::WaitApiRouteOpen("PGO_LOCAL_SERVER_PING"),
        S::StartApiRequest,
        S::WaitApiResponse {
            expected_status: 200,
            body_marker: "RRITER_PGO_LOCAL_API_OK",
        },
        S::WaitMillis(500),
        S::OpenPanel(PanelId::Database),
        S::Database(DatabaseAutomationStep::SetupConnection),
        S::Database(DatabaseAutomationStep::LoadCatalog),
        S::Database(DatabaseAutomationStep::WaitCatalog),
        S::Database(DatabaseAutomationStep::LoadTables),
        S::Database(DatabaseAutomationStep::WaitTables),
        S::Database(DatabaseAutomationStep::LoadDdl),
        S::Database(DatabaseAutomationStep::WaitDdl),
        S::WaitFrames(6),
        S::Database(DatabaseAutomationStep::DismissDdl),
        S::Database(DatabaseAutomationStep::OpenTable),
        S::Database(DatabaseAutomationStep::WaitTable),
        S::WaitFrames(8),
        S::Database(DatabaseAutomationStep::ScrollTableTimed { duration_secs: 8 }),
        S::Database(DatabaseAutomationStep::SortTable),
        S::Database(DatabaseAutomationStep::WaitTableReload),
        S::WaitFrames(6),
        S::Database(DatabaseAutomationStep::EditTableCell),
        S::Database(DatabaseAutomationStep::SaveTableChanges),
        S::Database(DatabaseAutomationStep::WaitTableReview),
        S::WaitFrames(6),
        S::Database(DatabaseAutomationStep::RollbackTableTransaction),
        S::Database(DatabaseAutomationStep::WaitTableTransactionFinished),
        S::Database(DatabaseAutomationStep::OpenQuery),
        S::Database(DatabaseAutomationStep::WaitQueryCompletion),
        S::Database(DatabaseAutomationStep::SetQueryText),
        S::Database(DatabaseAutomationStep::RunQuery),
        S::Database(DatabaseAutomationStep::WaitQueryResult),
        S::WaitFrames(6),
        S::Database(DatabaseAutomationStep::ScrollQueryResultTimed { duration_secs: 8 }),
        S::Database(DatabaseAutomationStep::RunExplain),
        S::Database(DatabaseAutomationStep::WaitExplain),
        S::WaitFrames(6),
        S::Database(DatabaseAutomationStep::AssertIdle),
        S::OpenPanel(PanelId::Explorer),
        S::WaitFrames(3),
        S::OpenFileTreeContext,
        S::WaitFrames(4),
        S::CloseContextMenu,
        S::OpenPanel(PanelId::LspServers),
        S::WaitFrames(8),
        S::OpenPanel(PanelId::Problems),
        S::WaitFrames(8),
        S::OpenPanel(PanelId::Terminal),
        S::WaitTerminal,
    ]);
    steps.extend(terminal_workload_steps_for(std::env::consts::OS));
    steps.extend([
        S::WaitMillis(500),
        S::ShowSettings(true),
        S::WaitFrames(5),
        S::SetSettingsTab(0),
        S::WaitMillis(350),
        S::SetSettingsTab(1),
        S::WaitMillis(350),
        S::AddSettingsIgnore(".rriter-pgo-ignore/**"),
        S::WaitFileTree,
        S::WaitMillis(500),
        S::RemoveSettingsIgnore(".rriter-pgo-ignore/**"),
        S::WaitFileTree,
        S::SetSettingsTab(2),
        S::WaitMillis(350),
        S::RefreshSettingsTools,
        S::SetSettingsTab(3),
        S::WaitMillis(350),
        S::SetSettingsTab(4),
        S::WaitFrames(5),
        S::ShowSettings(false),
        S::WaitFrames(5),
    ]);
    for group in crate::app::automation_groups::FULL_GROUPS {
        steps.extend(crate::app::automation_groups::group_steps(group, workspace).unwrap_or_default());
    }
    steps.push(S::Finish);
    steps
}

fn terminal_workload_steps_for(os: &str) -> Vec<AutomationStep> {
    use AutomationStep as S;
    if os == "linux" {
        vec![
            S::RunTerminalHtop,
            S::WaitTerminalHtopVisible,
            S::WaitMillis(10_000),
            S::InterruptTerminal,
            S::WaitTerminalHtopExit,
        ]
    } else {
        vec![
            S::RunTerminalBasicCommand,
            S::WaitTerminalBasicCommandVisible,
        ]
    }
}

