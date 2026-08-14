impl App {
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn toggle_git_file_stage(&mut self, workspace_idx: usize, file_idx: usize) {
        if git_stage_click_locked(&self.ide_panel.git, workspace_idx) {
            return;
        }
        if self
            .ide_panel
            .git
            .snapshot
            .active_staged_workspace_idx()
            .is_some_and(|idx| idx != workspace_idx)
        {
            return;
        }
        let Some((repo_root, file)) = self
            .ide_panel
            .git
            .snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_idx == workspace_idx)
            .and_then(|workspace| {
                let repo_root = workspace.repo_root.clone()?;
                workspace
                    .files
                    .get(file_idx)
                    .cloned()
                    .map(|file| (repo_root, file))
            })
        else {
            return;
        };
        if let Some(file_mut) = self
            .ide_panel
            .git
            .snapshot
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.workspace_idx == workspace_idx)
            .and_then(|workspace| workspace.files.get_mut(file_idx))
        {
            file_mut.staged = !file.staged;
        }
        self.ide_panel.git.stage_pending_workspace_idx = Some(workspace_idx);
        self.spawn_git_task(GitAction::ToggleStageMany {
            files: vec![GitStageFileCommand {
                repo_root,
                rel_path: file.rel_path.into(),
                old_rel_path: file.old_rel_path.map(Into::into),
                staged: file.staged,
            }],
        });
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn toggle_git_folder_stage(&mut self, workspace_idx: usize, row_idx: usize) {
        if git_stage_click_locked(&self.ide_panel.git, workspace_idx) {
            return;
        }
        if self
            .ide_panel
            .git
            .snapshot
            .active_staged_workspace_idx()
            .is_some_and(|idx| idx != workspace_idx)
        {
            return;
        }

        let Some((file_indices, target_staged, files)) = self
            .ide_panel
            .git
            .snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_idx == workspace_idx)
            .map(|workspace| {
                let file_indices = git_folder_file_indices(workspace, row_idx);
                let all_staged = !file_indices.is_empty()
                    && file_indices
                        .iter()
                        .all(|idx| workspace.files.get(*idx).is_some_and(|file| file.staged));
                let target_staged = !all_staged;
                let files = workspace
                    .repo_root
                    .as_ref()
                    .map(|repo_root| {
                        file_indices
                            .iter()
                            .filter_map(|idx| workspace.files.get(*idx))
                            .filter(|file| file.staged != target_staged)
                            .map(|file| GitStageFileCommand {
                                repo_root: repo_root.clone(),
                                rel_path: file.rel_path.to_string(),
                                old_rel_path: file.old_rel_path.as_ref().map(ToString::to_string),
                                staged: file.staged,
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                (file_indices, target_staged, files)
            })
        else {
            return;
        };
        if files.is_empty() {
            return;
        }

        if let Some(workspace) = self
            .ide_panel
            .git
            .snapshot
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.workspace_idx == workspace_idx)
        {
            for idx in file_indices {
                if let Some(file) = workspace.files.get_mut(idx) {
                    file.staged = target_staged;
                }
            }
        }

        self.ide_panel.git.stage_pending_workspace_idx = Some(workspace_idx);
        self.spawn_git_task(GitAction::ToggleStageMany { files });
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn commit_git_panel(&mut self) {
        self.commit_git_panel_with(false, false);
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn commit_git_panel_option(&mut self, option_idx: usize) {
        if let Some((amend, push_after)) = git_commit_option_flags(option_idx) {
            self.commit_git_panel_with(amend, push_after);
        }
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn commit_git_panel_with(&mut self, amend: bool, push_after: bool) {
        if self.ide_panel.git.pending {
            return;
        }
        if !self.ide_panel.git.commit_enabled() {
            return;
        }
        self.ide_panel.git.close_commit_menus();
        self.ide_panel.git.message_focused = false;
        let message = self.ide_panel.git.message_editor.get_full_text();
        let trimmed = message.trim();
        if trimmed.is_empty() {
            self.ide_panel.git.notice = Some("Commit message empty".to_string());
            return;
        }
        let repo_roots = self.ide_panel.git.snapshot.staged_repo_roots();
        if repo_roots.is_empty() {
            self.ide_panel.git.notice = Some("No staged files".to_string());
            return;
        }
        self.spawn_git_task(GitAction::Commit {
            repo_roots,
            message: trimmed.to_string(),
            amend,
            push_after,
            skip_hooks: self.ide_panel.git.commit_options.skip_hooks,
        });
        self.ide_panel.git.message_editor = Editor::new(512);
    }

    fn run_git_workspace_action(
        &mut self,
        workspace_idx: usize,
        action: impl FnOnce(PathBuf) -> GitAction,
    ) {
        if self.ide_panel.git.pending {
            return;
        }
        let Some(repo_root) = self
            .ide_panel
            .git
            .snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_idx == workspace_idx)
            .and_then(|workspace| workspace.repo_root.clone())
        else {
            return;
        };
        self.spawn_git_task(action(repo_root));
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn push_git_workspace(&mut self, workspace_idx: usize) {
        self.run_git_workspace_action(workspace_idx, |repo_root| GitAction::Push { repo_root });
    }

    pub fn fetch_git_workspace(&mut self, workspace_idx: usize) {
        self.run_git_workspace_action(workspace_idx, |repo_root| GitAction::Fetch { repo_root });
    }

    pub fn pull_git_workspace(&mut self, workspace_idx: usize) {
        self.run_git_workspace_action(workspace_idx, |repo_root| GitAction::Pull { repo_root });
    }

    pub fn open_git_rollback_staged_dialog(&mut self, workspace_idx: usize) {
        self.open_git_confirm_dialog(workspace_idx, GitConfirmAction::RollbackStaged);
    }

    pub fn open_git_unstage_all_dialog(&mut self, workspace_idx: usize) {
        self.unstage_all_git_workspace(workspace_idx);
    }

    pub fn stage_all_git_workspace(&mut self, workspace_idx: usize) {
        if git_stage_click_locked(&self.ide_panel.git, workspace_idx) {
            return;
        }
        if self
            .ide_panel
            .git
            .snapshot
            .active_staged_workspace_idx()
            .is_some_and(|idx| idx != workspace_idx)
        {
            return;
        }

        let Some((file_indices, files)) = self
            .ide_panel
            .git
            .snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_idx == workspace_idx)
            .map(|workspace| {
                let file_indices = workspace
                    .files
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, file)| (!file.staged).then_some(idx))
                    .collect::<Vec<_>>();
                let files = workspace
                    .repo_root
                    .as_ref()
                    .map(|repo_root| {
                        file_indices
                            .iter()
                            .filter_map(|idx| workspace.files.get(*idx))
                            .map(|file| GitStageFileCommand {
                                repo_root: repo_root.clone(),
                                rel_path: file.rel_path.to_string(),
                                old_rel_path: file.old_rel_path.as_ref().map(ToString::to_string),
                                staged: false,
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                (file_indices, files)
            })
        else {
            return;
        };
        if files.is_empty() {
            self.ide_panel.git.notice = Some("No unstaged files".to_string());
            return;
        }

        if let Some(workspace) = self
            .ide_panel
            .git
            .snapshot
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.workspace_idx == workspace_idx)
        {
            for idx in file_indices {
                if let Some(file) = workspace.files.get_mut(idx) {
                    file.staged = true;
                }
            }
        }
        self.ide_panel.git.stage_pending_workspace_idx = Some(workspace_idx);
        self.spawn_git_task(GitAction::ToggleStageMany { files });
    }

    pub fn unstage_all_git_workspace(&mut self, workspace_idx: usize) {
        if git_stage_click_locked(&self.ide_panel.git, workspace_idx) {
            return;
        }
        let files = git_staged_confirm_files(&self.ide_panel.git.snapshot, workspace_idx)
            .into_iter()
            .map(|file| GitStageFileCommand {
                repo_root: file.repo_root,
                rel_path: file.rel_path,
                old_rel_path: file.old_rel_path,
                staged: true,
            })
            .collect::<Vec<_>>();
        if files.is_empty() {
            self.ide_panel.git.notice = Some("No staged files".to_string());
            return;
        }

        if let Some(workspace) = self
            .ide_panel
            .git
            .snapshot
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.workspace_idx == workspace_idx)
        {
            for file in &mut workspace.files {
                if file.staged {
                    file.staged = false;
                }
            }
        }
        self.ide_panel.git.stage_pending_workspace_idx = Some(workspace_idx);
        self.spawn_git_task(GitAction::ToggleStageMany { files });
    }

    fn open_git_confirm_dialog(&mut self, workspace_idx: usize, action: GitConfirmAction) {
        if self.ide_panel.git.pending {
            return;
        }
        let files = git_staged_confirm_files(&self.ide_panel.git.snapshot, workspace_idx);
        if files.is_empty() {
            self.ide_panel.git.notice = Some("No staged files".to_string());
            return;
        }
        self.ide_panel.git.close_commit_menus();
        self.ide_panel.git.message_focused = false;
        self.ide_panel.git.confirm_dialog = Some(GitConfirmDialog {
            action,
            workspace_idx,
            files,
        });
    }

    pub fn confirm_git_dialog(&mut self) {
        if self.ide_panel.git.pending {
            return;
        }
        let Some(dialog) = self.ide_panel.git.confirm_dialog.take() else {
            return;
        };
        let files = dialog
            .files
            .into_iter()
            .map(|file| GitStageFileCommand {
                repo_root: file.repo_root,
                rel_path: file.rel_path,
                old_rel_path: file.old_rel_path,
                staged: true,
            })
            .collect::<Vec<_>>();
        if files.is_empty() {
            self.ide_panel.git.notice = Some("No staged files".to_string());
            return;
        }

        match dialog.action {
            GitConfirmAction::RollbackStaged => {
                self.spawn_git_task(GitAction::RollbackStaged { files });
            }
        }
    }

    pub fn toggle_git_workspace(&mut self, workspace_idx: usize) {
        if !self
            .ide_panel
            .git
            .snapshot
            .workspaces
            .iter()
            .any(|workspace| {
                workspace.workspace_idx == workspace_idx && workspace.has_collapsible_rows()
            })
        {
            return;
        }
        if !self
            .ide_panel
            .git
            .collapsed_workspaces
            .remove(&workspace_idx)
        {
            self.ide_panel
                .git
                .collapsed_workspaces
                .insert(workspace_idx);
        }
    }

    pub fn toggle_git_tree_folder(&mut self, workspace_idx: usize, row_idx: usize) {
        let Some(row) = self
            .ide_panel
            .git
            .snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_idx == workspace_idx)
            .and_then(|workspace| workspace.tree.get(row_idx))
        else {
            return;
        };
        if row.file_idx.is_some() {
            return;
        }
        let dirs = self
            .ide_panel
            .git
            .collapsed_dirs
            .entry(workspace_idx)
            .or_default();
        if !dirs.remove(row.path.as_ref()) {
            dirs.insert(row.path.to_string());
        }
    }

    pub(crate) fn reconcile_saved_current_file_git_index(&mut self) {
        if self.editor.git_base_text.is_none() || !self.editor.git_hunks.is_empty() {
            return;
        }
        if self.ide_panel.git.pending && self.ide_panel.git.stage_pending_workspace_idx.is_none() {
            return;
        }
        let Some((repo_root, file)) = self.current_git_file_entry() else {
            return;
        };
        if !file.staged || file.status != GitFileStatus::Modified {
            return;
        }
        if !self
            .ide_panel
            .git
            .take_stage_reconcile_candidate(&repo_root, file.rel_path.as_ref())
        {
            return;
        }
        self.spawn_git_task(GitAction::ReconcileStagedModified {
            file: GitReconcileFileCommand {
                repo_root,
                rel_path: file.rel_path.into(),
            },
        });
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    fn spawn_git_task(&mut self, action: GitAction) {
        let refresh = matches!(&action, GitAction::Refresh);
        if refresh
            && self
                .ide_panel
                .git
                .rx
                .iter()
                .any(|receiver| receiver.status_mutation)
        {
            self.ide_panel.git.status_refresh_dirty = true;
            return;
        }
        if refresh && !self.ide_panel.git.begin_status_refresh() {
            return;
        }

        if let GitAction::LoadGraph {
            workspace_idx,
            repo_root,
            offset,
            limit,
            reset_scroll,
            activate,
        } = action
        {
            let request_id = self.ide_panel.git.allocate_graph_request_id();
            self.ide_panel
                .git
                .graph_latest_request_by_root
                .insert(crate::platform::PathKey::new(&repo_root), request_id);
            self.ide_panel
                .git
                .graph_pending_roots
                .insert(crate::platform::PathKey::new(&repo_root));
            if activate {
                self.ide_panel.git.graph_pending = true;
                self.ide_panel.git.graph_notice = None;
                self.ide_panel.git.graph_repo_root = Some(repo_root.clone());
                self.ide_panel.git.graph_workspace_idx = Some(workspace_idx);
                self.ide_panel.git.graph_commit_limit = limit;
            } else if self
                .ide_panel
                .git
                .graph_repo_root
                .as_ref()
                .is_some_and(|active| crate::platform::paths_equal(active, &repo_root))
            {
                self.ide_panel.git.graph_pending = true;
            }

            let (tx, rx) = mpsc::channel();
            self.ide_panel.git.graph_rx.push(GitGraphReceiver {
                rx,
                request_id,
                repo_root: repo_root.clone(),
            });
            let worker_tx = tx.clone();
            let worker_repo_root = repo_root.clone();
            if let Err(err) = crate::platform::spawn_named("rriter-git-graph", move || {
                let (commits, lane_count, has_more, notice) =
                    match collect_git_graph(workspace_idx, &worker_repo_root, offset, limit) {
                        Ok((commits, lane_count, has_more)) => {
                            (commits, lane_count, has_more, None)
                        }
                        Err(err) => (Vec::new(), 1, false, Some(err)),
                    };
                let _ = worker_tx.send(GitGraphEvent {
                    request_id,
                    workspace_idx,
                    repo_root: worker_repo_root,
                    commits,
                    lane_count,
                    notice,
                    limit,
                    offset,
                    has_more,
                    reset_scroll,
                });
            }) {
                let _ = tx.send(GitGraphEvent {
                    request_id,
                    workspace_idx,
                    repo_root,
                    commits: Vec::new(),
                    lane_count: 1,
                    notice: Some(format!("Не удалось запустить загрузку Git Graph: {err}")),
                    limit,
                    offset,
                    has_more: false,
                    reset_scroll,
                });
            }
            return;
        }

        let request_id = self.ide_panel.git.allocate_status_request_id();
        let status_mutation = matches!(
            &action,
            GitAction::ToggleStageMany { .. } | GitAction::ReconcileStagedModified { .. }
        );
        let blocking = !refresh;
        if blocking {
            let now = std::time::Instant::now();
            self.ide_panel.git.pending = true;
            self.ide_panel.git.pending_label = match &action {
                GitAction::Commit {
                    push_after: _, ..
                } => Some("Подготовка".to_string()),
                GitAction::Push { .. } => Some("Push".to_string()),
                GitAction::Fetch { .. } => Some("Fetch".to_string()),
                GitAction::Pull { .. } => Some("Pull".to_string()),
                _ => None,
            };
            self.ide_panel.git.pending_started_at = Some(now);
            self.ide_panel.git.pending_label_until = self
                .ide_panel
                .git
                .pending_label
                .as_ref()
                .map(|_| now + std::time::Duration::from_secs(1));
        }
        self.ide_panel.git.notice = None;

        let workspaces = self.ide_workspaces.clone();
        let branch_ahead_cache = self.ide_panel.git.branch_ahead_cache.clone();
        let commit_transaction = matches!(&action, GitAction::Commit { .. });
        let (runtime_tx, runtime_rx) = if commit_transaction {
            let (runtime_tx, runtime_rx) = mpsc::sync_channel(GIT_RUNTIME_EVENT_CAPACITY);
            (Some(runtime_tx), Some(runtime_rx))
        } else {
            (None, None)
        };
        let (tx, rx) = mpsc::channel();
        self.ide_panel.git.rx.push(GitPanelReceiver {
            rx,
            runtime_rx,
            request_id,
            blocking,
            refresh,
            status_mutation,
        });

        let action = match action {
            GitAction::ToggleStageMany { files } => {
                self.ide_panel.git.update_stage_reconcile_candidates(&files);
                enqueue_git_stage_operation(
                    &mut self.ide_panel.git,
                    GitStageOperation::ToggleMany(files),
                    request_id,
                    workspaces,
                    branch_ahead_cache,
                    tx,
                );
                return;
            }
            GitAction::ReconcileStagedModified { file } => {
                enqueue_git_stage_operation(
                    &mut self.ide_panel.git,
                    GitStageOperation::ReconcileModified(file),
                    request_id,
                    workspaces,
                    branch_ahead_cache,
                    tx,
                );
                return;
            }
            action => action,
        };

        let worker_tx = tx.clone();
        let worker_runtime_tx = runtime_tx.clone();
        if let Err(err) = crate::platform::spawn_named("rriter-git-action", move || {
            let outcome = run_git_action(action, worker_runtime_tx.as_ref());
            let mut branch_ahead_cache = branch_ahead_cache;
            let snapshot = collect_git_status_with_cache(&workspaces, &mut branch_ahead_cache);
            let _ = worker_tx.send(GitPanelTaskResult {
                event: GitPanelEvent {
                    request_id,
                    snapshot,
                    notice: outcome.notice,
                    preserve_snapshot_on_empty: false,
                    clear_message: outcome.clear_message,
                    refresh_graph: outcome.refresh_graph,
                    transaction_failed: outcome.transaction_failed,
                },
                branch_ahead_cache,
            });
        }) {
            if let Some(runtime_tx) = runtime_tx {
                let _ = runtime_tx.send(GitRuntimeEvent::Info(format!(
                    "Не удалось запустить Git worker: {err}"
                )));
            }
            let _ = tx.send(GitPanelTaskResult {
                event: GitPanelEvent {
                    request_id,
                    snapshot: GitStatusSnapshot::default(),
                    notice: Some(format!("Не удалось запустить Git worker: {err}")),
                    preserve_snapshot_on_empty: true,
                    clear_message: false,
                    refresh_graph: false,
                    transaction_failed: commit_transaction,
                },
                branch_ahead_cache: BranchAheadCache::default(),
            });
        }
    }
}

fn enqueue_git_stage_operation(
    state: &mut GitPanelState,
    operation: GitStageOperation,
    request_id: u64,
    workspaces: Vec<PathBuf>,
    branch_ahead_cache: BranchAheadCache,
    tx: mpsc::Sender<GitPanelTaskResult>,
) {
    let mut command = GitStageCommand {
        request_id,
        operation,
        workspaces,
        branch_ahead_cache,
        tx,
    };
    if let Some(stage_tx) = &state.stage_tx {
        match stage_tx.send(command) {
            Ok(()) => return,
            Err(err) => command = err.0,
        }
    }

    let (stage_tx, stage_rx) = mpsc::channel::<GitStageCommand>();
    match crate::platform::spawn_named("rriter-git-stage", move || {
        let mut owned_stage_entries = FxHashMap::default();
        for command in stage_rx {
            let notice =
                run_git_stage_operation(&command.operation, &mut owned_stage_entries);
            let mut branch_ahead_cache = command.branch_ahead_cache;
            let snapshot =
                collect_git_status_with_cache(&command.workspaces, &mut branch_ahead_cache);
            let _ = command.tx.send(GitPanelTaskResult {
                event: GitPanelEvent {
                    request_id: command.request_id,
                    snapshot,
                    notice,
                    preserve_snapshot_on_empty: true,
                    clear_message: false,
                    refresh_graph: false,
                    transaction_failed: false,
                },
                branch_ahead_cache,
            });
        }
    }) {
        Ok(_) => {
            state.stage_tx = Some(stage_tx.clone());
            let _ = stage_tx.send(command);
        }
        Err(err) => {
            state.stage_tx = None;
            state.clear_stage_reconcile_candidates_for_operation(&command.operation);
            let _ = command.tx.send(GitPanelTaskResult {
                event: GitPanelEvent {
                    request_id: command.request_id,
                    snapshot: GitStatusSnapshot::default(),
                    notice: Some(format!("Не удалось запустить Git stage worker: {err}")),
                    preserve_snapshot_on_empty: true,
                    clear_message: false,
                    refresh_graph: false,
                    transaction_failed: false,
                },
                branch_ahead_cache: command.branch_ahead_cache,
            });
        }
    }
}

fn git_commit_option_flags(option_idx: usize) -> Option<(bool, bool)> {
    match option_idx {
        0 => Some((false, false)),
        1 => Some((true, false)),
        2 => Some((false, true)),
        _ => None,
    }
}
