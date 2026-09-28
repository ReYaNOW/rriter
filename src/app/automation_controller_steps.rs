impl AutomationController {
    fn run_step(
        &mut self,
        app: &mut App,
        _event_loop: &HostLoop,
        step: &AutomationStep,
        now: Instant,
    ) -> StepResult {
        match step {
            AutomationStep::WaitReady => {
                if app.is_ready && app.is_ide_mode && app.window.is_some() && app.renderer.is_some()
                {
                    StepResult::Done
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::ResizeWindow { width, height } => {
                let Some(window) = app.window.as_ref() else {
                    return StepResult::Pending;
                };
                let _ = window.request_inner_size(PhysicalSize::new(*width, *height));
                window.request_redraw();
                StepResult::Done
            }
            AutomationStep::ApplyWorkspace => {
                if !self.options.workspace.is_dir() {
                    return StepResult::Failed(format!(
                        "automation workspace does not exist: {}",
                        self.options.workspace.display()
                    ));
                }
                if let Err(error) = ensure_fixture_repository(&self.options.workspace) {
                    return StepResult::Failed(error);
                }
                app.apply_selected_workspace_folder(self.options.workspace.clone());
                StepResult::Done
            }
            AutomationStep::WaitFileTree => {
                if app.file_tree_rx.is_none() && !app.ide_panel.file_tree_nodes.is_empty() {
                    StepResult::Done
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::WaitFrames(frames) => {
                self.step_progress = self.step_progress.saturating_add(1);
                if self.step_progress >= u32::from(*frames) {
                    StepResult::Done
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::WaitMillis(millis) => {
                if now.saturating_duration_since(self.step_started_at)
                    >= Duration::from_millis(*millis)
                {
                    StepResult::Done
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::OpenPanel(panel) => {
                if begin_open_panel_action(&mut self.step_progress) {
                    open_panel_semantic(app, *panel);
                }
                if *panel != PanelId::Terminal {
                    if app.ide_panel.is_open(*panel) {
                        StepResult::Done
                    } else {
                        StepResult::Failed(format!("panel did not open: {panel:?}"))
                    }
                } else {
                    match terminal_panel_open_state(app) {
                        TerminalPanelOpenState::Open => StepResult::Done,
                        TerminalPanelOpenState::WaitingForPresentation => StepResult::Pending,
                        TerminalPanelOpenState::ClosedUnexpectedly => {
                            StepResult::Failed("panel did not open: Terminal".to_string())
                        }
                    }
                }
            }
            AutomationStep::ExpandWorkspaceRoot => {
                let node_idx = app
                    .ide_panel
                    .file_tree_nodes
                    .iter()
                    .position(|node| node.is_dir && node.path == self.options.workspace)
                    .or_else(|| {
                        app.ide_panel
                            .file_tree_nodes
                            .iter()
                            .position(|node| node.is_dir)
                    });
                let Some(node_idx) = node_idx else {
                    return StepResult::Pending;
                };
                if !app.ide_panel.file_tree_nodes[node_idx].is_expanded {
                    app.toggle_file_tree_dir(node_idx);
                }
                StepResult::Done
            }
            AutomationStep::OpenFile(relative) => {
                let path = self.options.workspace.join(relative);
                if !path.is_file() {
                    return StepResult::Failed(format!(
                        "fixture file is missing: {}",
                        path.display()
                    ));
                }
                app.open_file_in_tab(path, false);
                StepResult::Done
            }
            AutomationStep::SwitchToFile(relative) => {
                let target = self.options.workspace.join(relative);
                let Some(index) = app.tabs.iter().position(|tab| {
                    tab.file_path
                        .as_ref()
                        .is_some_and(|path| crate::platform::paths_equal(path, &target))
                }) else {
                    return StepResult::Failed(format!("open tab not found: {}", target.display()));
                };
                app.switch_to_tab(index);
                StepResult::Done
            }
            AutomationStep::WaitHighlight => {
                if app.is_highlight_complete || app.highlighter.is_complete {
                    StepResult::Done
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::FocusEditor => {
                focus_main_editor_semantic(app);
                StepResult::Done
            }
            AutomationStep::TypeText(text) => {
                app.handle_main_ime_commit(text);
                StepResult::Done
            }
            AutomationStep::SaveCurrentFile => {
                if app.save_current_file() {
                    StepResult::Done
                } else {
                    StepResult::Failed("saving the automation fixture failed".to_string())
                }
            }
            AutomationStep::OpenActiveTabContext => {
                if app.open_tab_context_menu(app.active_tab, 96.0, 64.0) {
                    StepResult::Done
                } else {
                    StepResult::Failed("active tab context menu did not open".to_string())
                }
            }
            AutomationStep::OpenFileTreeContext => open_file_tree_context_semantic(app),
            AutomationStep::CloseContextMenu => {
                app.ide_panel.file_tree_context_menu = None;
                StepResult::Done
            }
            AutomationStep::OpenSearch => {
                app.show_search = true;
                app.search_focused = true;
                app.search_editor.select_all();
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::SetSearchQuery(query) => {
                app.search_editor.set_text_clean(query);
                app.search_editor.cursor = query.len();
                app.search_focused = true;
                app.update_search();
                app.jump_to_search_result();
                request_redraw(app);
                if app.search_results.is_empty() {
                    StepResult::Failed(format!("editor search returned no results: {query}"))
                } else {
                    StepResult::Done
                }
            }
            AutomationStep::ToggleSearchCase => {
                app.search_case_sensitive = !app.search_case_sensitive;
                app.update_search();
                app.jump_to_search_result();
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::NextSearchResult => {
                if app.search_results.is_empty() {
                    return StepResult::Failed("editor search results disappeared".to_string());
                }
                let next = app
                    .search_current_idx
                    .map_or(0, |idx| (idx + 1) % app.search_results.len());
                app.search_current_idx = Some(next);
                app.jump_to_search_result();
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::PreviousSearchResult => {
                if app.search_results.is_empty() {
                    return StepResult::Failed("editor search results disappeared".to_string());
                }
                let previous = app.search_current_idx.map_or(0, |idx| {
                    if idx == 0 {
                        app.search_results.len() - 1
                    } else {
                        idx - 1
                    }
                });
                app.search_current_idx = Some(previous);
                app.jump_to_search_result();
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::CloseSearch => {
                app.show_search = false;
                app.search_focused = false;
                app.search_results.clear();
                app.search_current_idx = None;
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::ScrollEditorTimed { duration_secs } => {
                self.timed_scroll(app, now, *duration_secs, |app, direction| {
                    let max_scroll = app.renderer.as_ref().map_or(0.0, |renderer| {
                        (app.editor.line_offsets.len() as f32 * renderer.line_height
                            - renderer.height)
                            .max(0.0)
                    });
                    app.scroll_y.scroll_by(36.0 * direction);
                    app.scroll_y.clamp_target(0.0, max_scroll);
                })
            }
            AutomationStep::Markdown(markdown_step) => match *markdown_step {
                MarkdownAutomationStep::ScrollReadTimed { duration_secs } => {
                    match crate::app::automation_markdown::prepare_read_scroll(app) {
                        Ok(true) => {}
                        Ok(false) => return StepResult::Pending,
                        Err(error) => return StepResult::Failed(error),
                    }
                    let mut failure = None;
                    let result = self.timed_scroll(app, now, duration_secs, |app, direction| {
                        if failure.is_none()
                            && let Err(error) =
                                crate::app::automation_markdown::scroll_read(app, direction)
                        {
                            failure = Some(error);
                        }
                    });
                    request_redraw(app);
                    failure.map_or(result, StepResult::Failed)
                }
                _ => match crate::app::automation_markdown::run_step(app, *markdown_step) {
                    MarkdownStepResult::Pending => StepResult::Pending,
                    MarkdownStepResult::Done => StepResult::Done,
                    MarkdownStepResult::Failed(message) => StepResult::Failed(message),
                },
            },
            AutomationStep::JumpMinimap(fraction) => {
                let Some(renderer) = app.renderer.as_mut() else {
                    return StepResult::Pending;
                };
                let height = app
                    .window
                    .as_ref()
                    .map_or(renderer.height, |window| window.inner_size().height as f32);
                let max_scroll = renderer.get_max_scroll(&app.editor, height);
                let target = (max_scroll * fraction.clamp(0.0, 1.0)).round();
                app.scroll_y.jump_to(target);
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::ToggleFirstFold => {
                let Some(line) = app.editor.foldable_lines.keys().copied().min() else {
                    return StepResult::Pending;
                };
                if !app.editor.folded_lines.remove(&line) {
                    app.editor.folded_lines.insert(line);
                    if let Some(offset) = app.editor.line_offsets.get(line).copied() {
                        app.editor.folded_start_bytes.insert(offset);
                    }
                } else if let Some(offset) = app.editor.line_offsets.get(line).copied() {
                    app.editor.folded_start_bytes.remove(&offset);
                }
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::SetEditorCursorAfter(needle) => {
                let text = app.editor.get_full_text();
                let Some(offset) = text.rfind(needle).map(|offset| offset + needle.len()) else {
                    return StepResult::Failed(format!("editor marker not found: {needle}"));
                };
                app.editor.cursor = offset;
                app.editor.selection_anchor = None;
                focus_main_editor_semantic(app);
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::TriggerAutocomplete(expected) => {
                app.update_autocomplete();
                if app.autocomplete_active
                    && app
                        .autocomplete_options
                        .iter()
                        .any(|(item, _)| item.word == *expected)
                {
                    request_redraw(app);
                    StepResult::Done
                } else if app.is_highlight_complete || app.highlighter.is_complete {
                    StepResult::Failed(autocomplete_failure_diagnostics(app, expected))
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::SelectAutocomplete(expected) => {
                let Some(index) = app
                    .autocomplete_options
                    .iter()
                    .position(|(item, _)| item.word == *expected)
                else {
                    return StepResult::Pending;
                };
                app.autocomplete_selected_idx = index;
                app.ensure_autocomplete_visible();
                app.request_active_autocomplete_detail_for_index(index);
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::ApplyAutocomplete(expected) => {
                app.apply_autocomplete();
                if !app.autocomplete_active && app.editor.get_full_text().contains(expected) {
                    StepResult::Done
                } else {
                    StepResult::Failed(format!("completion was not applied: {expected}"))
                }
            }
            AutomationStep::ShowHover { needle, text } => show_hover_semantic(
                app,
                &self.options.workspace,
                needle,
                text,
                &mut self.step_progress,
                &mut self.hover_last_anchor,
            ),
            AutomationStep::ScrollHoverTimed { duration_secs } => {
                let hover_ready = app.hover.popup.is_some() && app.hover.rect.is_some();
                if !hover_ready {
                    return StepResult::Failed(format!(
                        "hover popup disappeared before scrolling; {}",
                        hover_state_diagnostics(&app.hover)
                    ));
                }
                self.timed_scroll(app, now, *duration_secs, |app, direction| {
                    let state = &mut app.hover;
                    let max_scroll = state.max_scroll.max(480.0);
                    if let Some(popup) = state.popup.as_mut() {
                        popup.scroll.scroll_by(24.0 * direction);
                        popup.scroll.clamp_target(0.0, max_scroll);
                    }
                })
            }
            AutomationStep::ClearHover => {
                app.hover = crate::app::mouse::HoverState::default();
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::SetProjectSearchQuery(query) => {
                open_panel_semantic(app, PanelId::Search);
                app.ide_panel
                    .project_search
                    .query_editor
                    .set_text_clean(query);
                app.ide_panel.project_search.query_editor.cursor = query.len();
                app.ide_panel.project_search.focused =
                    Some(crate::app::project_search::ProjectSearchField::Query);
                app.ide_panel.project_search.dirty = true;
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::RunProjectSearch => {
                app.start_project_search();
                StepResult::Done
            }
            AutomationStep::WaitProjectSearch => {
                let search = &app.ide_panel.project_search;
                if search.has_run && search.running_generation.is_none() && search.rx.is_none() {
                    if let Some(error) = &search.error {
                        StepResult::Failed(format!("project search failed: {error}"))
                    } else if search.total_matches == 0 {
                        StepResult::Failed(
                            "project search completed without the fixture marker".to_string(),
                        )
                    } else {
                        StepResult::Done
                    }
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::JumpFirstProjectSearchMatch => {
                if app
                    .ide_panel
                    .project_search
                    .results
                    .first()
                    .is_some_and(|file| !file.matches.is_empty())
                {
                    app.handle_project_search_match_click(0, 0);
                    StepResult::Done
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::WaitGit => {
                if !app.ide_panel.is_open(PanelId::Git) {
                    return StepResult::Failed("Git panel did not open".to_string());
                }
                if self.step_progress == 0 {
                    app.refresh_git_panel();
                    self.step_progress = 1;
                    return StepResult::Pending;
                }
                let git = &app.ide_panel.git;
                if !git.pending && !git.status_loading() {
                    if git.snapshot.workspaces.is_empty() {
                        StepResult::Failed(
                            "Git panel did not discover the fixture repository".to_string(),
                        )
                    } else {
                        StepResult::Done
                    }
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::ToggleGitGraph => {
                if !app.ide_panel.git.graph_open() {
                    app.toggle_git_graph();
                }
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::WaitGitGraph => {
                let git = &app.ide_panel.git;
                if !git.graph_pending {
                    if git.graph_snapshot.is_empty() {
                        StepResult::Failed("Git graph produced no commits".to_string())
                    } else {
                        StepResult::Done
                    }
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::LoadGitGraph { min_commits } => {
                if app.ide_panel.git.graph_pending {
                    return StepResult::Pending;
                }
                let loaded = app.ide_panel.git.graph_snapshot.len();
                if loaded >= *min_commits {
                    return StepResult::Done;
                }
                if !app.ide_panel.git.graph_has_more {
                    return StepResult::Failed(format!(
                        "Git graph ended at {loaded} commits; expected at least {min_commits}"
                    ));
                }
                app.load_more_git_graph_commits();
                StepResult::Pending
            }
            AutomationStep::ScrollGitGraphTimed { duration_secs } => {
                self.timed_scroll(app, now, *duration_secs, |app, direction| {
                    let scale = app
                        .renderer
                        .as_ref()
                        .map_or(1.0, |renderer| renderer.scale_factor);
                    let view_h = crate::app::mouse::git_graph_rows_bounds(app, scale)
                        .map_or(0.0, |(_, rows_h)| rows_h);
                    let max_scroll = crate::app::git_panel::git_graph_max_scroll(
                        app.ide_panel.git.graph_snapshot.len(),
                        view_h,
                        scale,
                    );
                    app.ide_panel.git.graph_scroll.anim_speed = 7.0;
                    app.ide_panel.git.graph_scroll.scroll_by(120.0 * direction);
                    app.ide_panel.git.graph_scroll.clamp_target(0.0, max_scroll);
                })
            }
            AutomationStep::WaitTerminal => {
                if app.ide_panel.is_open(PanelId::Terminal) && !app.ide_panel.terminals.is_empty() {
                    StepResult::Done
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::RunTerminalHtop => write_terminal_semantic(app, b"htop\r"),
            AutomationStep::WaitTerminalHtopVisible => match active_terminal_tui_state(app) {
                Some(state) if state.alternate_screen && state.non_blank_cells >= 20 => {
                    StepResult::Done
                }
                _ => StepResult::Pending,
            },
            AutomationStep::InterruptTerminal => write_terminal_semantic(app, b"\x03"),
            AutomationStep::WaitTerminalHtopExit => match active_terminal_tui_state(app) {
                Some(state) if !state.alternate_screen => StepResult::Done,
                _ => StepResult::Pending,
            },
            AutomationStep::RunTerminalBasicCommand => {
                write_terminal_semantic(app, terminal_basic_command())
            }
            AutomationStep::WaitTerminalBasicCommandVisible => {
                match active_terminal_contains(app, TERMINAL_BASIC_OUTPUT) {
                    Some(true) => StepResult::Done,
                    Some(false) | None => StepResult::Pending,
                }
            }
            AutomationStep::ImportApiSpec => {
                let path = self.options.workspace.join("openapi.json");
                if !path.is_file() {
                    return StepResult::Failed(format!(
                        "OpenAPI fixture is missing: {}",
                        path.display()
                    ));
                }
                app.start_api_local_import(path);
                StepResult::Done
            }
            AutomationStep::WaitApiSpec => {
                let api = &app.ide_panel.api;
                if api.loading.is_empty() && !api.specs.is_empty() && !api.models.is_empty() {
                    StepResult::Done
                } else if api.loading.is_empty() && api.import_error.is_some() {
                    StepResult::Failed(format!(
                        "OpenAPI import failed: {}",
                        api.import_error.as_deref().unwrap_or("unknown error")
                    ))
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::WaitApiRoutesPanel => {
                let Some(model) = app.ide_panel.api.selected_model() else {
                    return StepResult::Pending;
                };
                if model.routes.is_empty() {
                    StepResult::Failed("OpenAPI import completed without routes".to_string())
                } else if app.ide_panel.is_open(PanelId::ApiClient) {
                    StepResult::Done
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::ScrollApiRoutesTimed { duration_secs } => {
                self.timed_scroll(app, now, *duration_secs, |app, direction| {
                    let scale = app
                        .renderer
                        .as_ref()
                        .map_or(1.0, |renderer| renderer.scale_factor);
                    let visible_h = if app.renderer.is_some() {
                        crate::app::mouse::app_panel_scroll_rect(app, PanelId::ApiClient, scale).3
                    } else {
                        720.0
                    };
                    let max_scroll = crate::app::api_client::api_panel_max_scroll(
                        &app.ide_panel.api,
                        visible_h,
                        scale,
                    );
                    app.ide_panel.api.panel_scroll.scroll_by(72.0 * direction);
                    app.ide_panel.api.panel_scroll.clamp_target(0.0, max_scroll);
                })
            }
            AutomationStep::ResetApiPanelScroll => {
                reset_scroll(&mut app.ide_panel.api.panel_scroll);
                StepResult::Done
            }
            AutomationStep::SetApiRouteFilter(needle) => {
                app.focus_api_input(ApiFocus::RouteFilter);
                app.ide_panel.api.input_editor.set_text_clean(needle);
                app.ide_panel.api.input_editor.cursor = needle.len();
                app.commit_api_focus();
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::WaitApiRouteFilter(needle) => {
                let Some(model) = app.ide_panel.api.selected_model() else {
                    return StepResult::Pending;
                };
                let matching = model.routes.iter().any(|route| {
                    route.operation_id.contains(needle)
                        || route.summary.contains(needle)
                        || route.path.contains(needle)
                });
                if !matching {
                    StepResult::Failed(format!("OpenAPI fixture route not found: {needle}"))
                } else if app.ide_panel.api.route_filter.contains(needle) {
                    StepResult::Done
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::OpenApiRouteMatching(needle) => {
                let Some(model) = app.ide_panel.api.selected_model() else {
                    return StepResult::Pending;
                };
                let spec_id = model.id;
                let route_idx = model.routes.iter().position(|route| {
                    route.operation_id.contains(needle)
                        || route.summary.contains(needle)
                        || route.path.contains(needle)
                });
                let Some(route_idx) = route_idx else {
                    return StepResult::Failed(format!(
                        "OpenAPI fixture route not found: {needle}"
                    ));
                };
                app.commit_api_focus();
                app.ide_panel.api.focused = None;
                app.open_api_route(spec_id, route_idx);
                if app.active_api_tab().is_some_and(|(meta, state)| {
                    meta.spec_id == spec_id && state.route_idx == Some(route_idx)
                }) {
                    StepResult::Done
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::WaitApiRouteOpen(needle) => {
                let opened = app.active_api_tab().is_some_and(|(meta, state)| {
                    state.route_idx.is_some()
                        && (meta.route_path.contains(needle)
                            || app
                                .ide_panel
                                .api
                                .models
                                .get(&meta.spec_id)
                                .and_then(|model| {
                                    state.route_idx.and_then(|idx| model.routes.get(idx))
                                })
                                .is_some_and(|route| {
                                    route.operation_id.contains(needle)
                                        || route.summary.contains(needle)
                                }))
                });
                if opened {
                    StepResult::Done
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::ScrollApiTabTimed { duration_secs } => {
                self.timed_scroll(app, now, *duration_secs, |app, direction| {
                    scroll_active_api_tab(app, 72.0 * direction)
                })
            }
            AutomationStep::OpenApiAuth => {
                let Some((meta, _)) = app.active_api_tab() else {
                    return StepResult::Pending;
                };
                let spec_id = meta.spec_id;
                app.open_api_auth_tab(spec_id);
                if app.active_api_tab().is_some_and(|(active_meta, state)| {
                    active_meta.spec_id == spec_id && state.auth_view
                }) {
                    StepResult::Done
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::FocusApiAuth(scheme) => {
                let Some((meta, _)) = app.active_api_tab() else {
                    return StepResult::Pending;
                };
                let spec_id = meta.spec_id;
                let Some(_scheme_idx) = app.ide_panel.api.models.get(&spec_id).and_then(|model| {
                    model
                        .security_schemes
                        .iter()
                        .position(|candidate| candidate.name == *scheme)
                }) else {
                    return StepResult::Failed(format!("OpenAPI auth scheme not found: {scheme}"));
                };
                app.focus_api_input(ApiFocus::AuthValue {
                    spec_id,
                    scheme: (*scheme).to_string(),
                });
                if matches!(
                    app.ide_panel.api.focused,
                    Some(ApiFocus::AuthValue { spec_id: focused_spec, scheme: ref focused_scheme })
                        if focused_spec == spec_id && focused_scheme == *scheme
                ) {
                    StepResult::Done
                } else {
                    StepResult::Pending
                }
            }
            AutomationStep::SetApiAuthValue { scheme, value } => {
                let Some((meta, state)) = app.active_api_tab() else {
                    return StepResult::Pending;
                };
                if !state.auth_view {
                    return StepResult::Failed("OpenAPI auth tab is not active".to_string());
                }
                let spec_id = meta.spec_id;
                let focus = ApiFocus::AuthValue {
                    spec_id,
                    scheme: (*scheme).to_string(),
                };
                if app.ide_panel.api.focused.as_ref() != Some(&focus) {
                    app.focus_api_input(focus);
                }
                app.ide_panel.api.input_editor.set_text_clean(value);
                app.ide_panel.api.input_editor.cursor = app.ide_panel.api.input_editor.len();
                app.ide_panel.api.input_editor.selection_anchor = None;
                app.ide_panel.api.input_editor.sync_edits.clear();
                request_redraw(app);
                let actual = app.ide_panel.api.input_editor.get_full_text();
                if actual == *value {
                    StepResult::Done
                } else {
                    StepResult::Failed(format!(
                        "OpenAPI auth editor did not accept value for scheme {scheme}; expected_len={} actual_len={} focus={:?}",
                        value.len(),
                        actual.len(),
                        app.ide_panel.api.focused
                    ))
                }
            }
            AutomationStep::SaveApiAuth { scheme, value } => {
                let Some((meta, _)) = app.active_api_tab() else {
                    return StepResult::Pending;
                };
                let spec_id = meta.spec_id;
                let Some(_scheme_idx) = app.ide_panel.api.models.get(&spec_id).and_then(|model| {
                    model
                        .security_schemes
                        .iter()
                        .position(|candidate| candidate.name == *scheme)
                }) else {
                    return StepResult::Failed(format!("OpenAPI auth scheme not found: {scheme}"));
                };
                let editor_value = app.ide_panel.api.input_editor.get_full_text();
                app.ide_panel
                    .api
                    .auth
                    .set_value(spec_id, scheme, editor_value.clone());
                app.ide_panel.api.focused = None;
                app.ide_panel.api.persist();
                if app
                    .ide_panel
                    .api
                    .auth
                    .entry(spec_id, scheme)
                    .is_some_and(|entry| entry.value == *value)
                {
                    StepResult::Done
                } else {
                    let saved = app
                        .ide_panel
                        .api
                        .auth
                        .entry(spec_id, scheme)
                        .map(|entry| {
                            format!(
                                "value_len={} token_type={:?}",
                                entry.value.len(),
                                entry.token_type
                            )
                        })
                        .unwrap_or_else(|| "missing entry".to_string());
                    StepResult::Failed(format!(
                        "OpenAPI auth value was not saved for scheme {scheme}; expected_len={} editor_len={} saved={saved}",
                        value.len(),
                        editor_value.len()
                    ))
                }
            }
            AutomationStep::StartApiRequest => {
                app.start_active_api_request();
                let Some((_, state)) = app.active_api_tab() else {
                    return StepResult::Failed(
                        "API request started without an active API tab".to_string(),
                    );
                };
                if state.pending || state.pending_request_id.is_some() || state.response.is_some() {
                    StepResult::Done
                } else {
                    StepResult::Failed("API request did not enter pending state".to_string())
                }
            }
            AutomationStep::WaitApiResponse {
                expected_status,
                body_marker,
            } => {
                let Some((_, state)) = app.active_api_tab() else {
                    return StepResult::Pending;
                };
                if state.pending || state.pending_request_id.is_some() {
                    return StepResult::Pending;
                }
                let Some(response) = state.response.as_ref() else {
                    return StepResult::Pending;
                };
                if let Some(error) = response.error.as_ref() {
                    return StepResult::Failed(format!(
                        "local API request failed: {:?}: {}",
                        error.kind, error.message
                    ));
                }
                if response.status != Some(*expected_status) {
                    return StepResult::Failed(format!(
                        "local API request returned {:?}, expected {expected_status}; body={:?}",
                        response.status, response.body
                    ));
                }
                if !response.body.contains(body_marker) {
                    return StepResult::Failed(format!(
                        "local API response is missing marker {body_marker:?}; body={:?}",
                        response.body
                    ));
                }
                StepResult::Done
            }
            AutomationStep::ResetApiTabScroll => {
                let Some(active_tab) = app.tabs.get_mut(app.active_tab) else {
                    return StepResult::Pending;
                };
                let crate::app::EditorTabKind::ApiClient(_, state) = &mut active_tab.kind else {
                    return StepResult::Pending;
                };
                reset_scroll(&mut state.tab_scroll);
                StepResult::Done
            }
            AutomationStep::ClearApiRouteFilter => {
                app.ide_panel.api.route_filter.clear();
                if matches!(app.ide_panel.api.focused, Some(ApiFocus::RouteFilter)) {
                    app.ide_panel.api.input_editor.set_text_clean("");
                    app.ide_panel.api.input_editor.cursor = 0;
                    app.ide_panel.api.input_editor.selection_anchor = None;
                }
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::Dart(dart_step) => {
                match crate::app::automation_dart::run(app, *dart_step) {
                    DartStepResult::Pending => StepResult::Pending,
                    DartStepResult::Done => StepResult::Done,
                    DartStepResult::Failed(message) => StepResult::Failed(message),
                }
            }
            AutomationStep::Database(database_step) => match *database_step {
                DatabaseAutomationStep::ScrollTableTimed { duration_secs } => {
                    let mut failure = None;
                    let result = self.timed_scroll(app, now, duration_secs, |app, direction| {
                        if failure.is_none()
                            && let Err(error) =
                                crate::app::automation_database::scroll_table(app, direction)
                        {
                            failure = Some(error);
                        }
                    });
                    failure.map_or(result, StepResult::Failed)
                }
                DatabaseAutomationStep::ScrollQueryResultTimed { duration_secs } => {
                    let mut failure = None;
                    let result = self.timed_scroll(app, now, duration_secs, |app, direction| {
                        if failure.is_none()
                            && let Err(error) =
                                crate::app::automation_database::scroll_query_result(app, direction)
                        {
                            failure = Some(error);
                        }
                    });
                    failure.map_or(result, StepResult::Failed)
                }
                _ => match crate::app::automation_database::run_step(app, *database_step) {
                    DatabaseStepResult::Pending => StepResult::Pending,
                    DatabaseStepResult::Done => StepResult::Done,
                    DatabaseStepResult::Failed(message) => StepResult::Failed(message),
                },
            },
            AutomationStep::ShowSettings(show) => {
                app.show_settings = *show;
                app.settings_anim_progress = if *show { 0.0 } else { 1.0 };
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::SetSettingsTab(tab) => {
                app.settings_tab = *tab;
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::AddSettingsIgnore(pattern) => {
                if !app
                    .ide_ignore_patterns
                    .iter()
                    .any(|candidate| candidate == pattern)
                {
                    app.ide_ignore_patterns.push((*pattern).to_string());
                    app.save_current_config();
                    app.refresh_file_tree();
                    request_redraw(app);
                }
                if app
                    .ide_ignore_patterns
                    .iter()
                    .any(|candidate| candidate == pattern)
                {
                    StepResult::Done
                } else {
                    StepResult::Failed(format!("settings ignore pattern was not added: {pattern}"))
                }
            }
            AutomationStep::RemoveSettingsIgnore(pattern) => {
                let Some(index) = app
                    .ide_ignore_patterns
                    .iter()
                    .position(|candidate| candidate == pattern)
                else {
                    return StepResult::Failed(format!(
                        "settings ignore pattern not found: {pattern}"
                    ));
                };
                app.ide_ignore_patterns.remove(index);
                app.save_current_config();
                app.refresh_file_tree();
                request_redraw(app);
                if app
                    .ide_ignore_patterns
                    .iter()
                    .all(|candidate| candidate != pattern)
                {
                    StepResult::Done
                } else {
                    StepResult::Failed(format!(
                        "settings ignore pattern was not removed: {pattern}"
                    ))
                }
            }
            AutomationStep::RefreshSettingsTools => {
                crate::platform::refresh_tool_resolutions();
                request_redraw(app);
                StepResult::Done
            }
            AutomationStep::Finish => StepResult::Exit,
        }
    }
}

