impl App {
    pub fn refresh_git_panel(&mut self) {
        if self.ide_workspaces.is_empty() {
            self.ide_panel.git.snapshot = GitStatusSnapshot::default();
            self.ide_panel.git.reset_async_state();
            self.ide_panel.git.pending_label_until = None;
            return;
        }
        self.ide_panel.git.graph_refresh_after_status = true;
        self.spawn_git_task(GitAction::Refresh);
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn refresh_git_panel_window(&mut self) {
        self.ide_panel.git.commit_menu_open = false;
        self.ide_panel.git.repo_action_menu_workspace_idx = None;
        self.ide_panel.git.snapshot = GitStatusSnapshot::default();
        self.ide_panel.git.selected_file = None;
        self.ide_panel.git.branch_ahead_cache.clear();
        self.ide_panel.git.notice = None;
        self.ide_panel.git.rx.clear();
        self.ide_panel.git.graph_rx.clear();
        self.ide_panel.git.status_refresh_pending = false;
        self.ide_panel.git.status_refresh_dirty = false;
        self.ide_panel.git.graph_cache.clear();
        self.ide_panel.git.graph_latest_request_by_root.clear();
        self.ide_panel.git.graph_pending_roots.clear();
        self.ide_panel.git.graph_pending = false;
        self.ide_panel.git.graph_snapshot.clear();
        self.ide_panel.git.graph_lane_count = 1;
        self.ide_panel.git.graph_commit_limit = GIT_GRAPH_LIMIT_STEP;
        self.ide_panel.git.graph_has_more = false;
        self.ide_panel.git.graph_copied_commit = None;
        self.ide_panel.git.graph_notice = None;
        self.ide_panel.git.graph_refresh_after_status = !self.ide_workspaces.is_empty();
        self.refresh_git_panel();
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn poll_git_panel(&mut self) -> bool {
        let now = std::time::Instant::now();
        let mut updated = false;
        let mut stale_seen = false;
        let mut reload_graph_cache = false;
        let mut prefetch_graph_after_status = false;
        let mut force_prefetch_graph_after_status = false;
        let mut refresh_rerun = false;
        let mut stale_refresh_dirty = false;
        let mut status_event_applied = false;
        let mut next_rx = Vec::with_capacity(self.ide_panel.git.rx.len());
        let receivers = std::mem::take(&mut self.ide_panel.git.rx);
        for receiver in receivers {
            let mut keep = true;
            loop {
                match receiver.rx.try_recv() {
                    Ok(result) => {
                        self.ide_panel
                            .git
                            .branch_ahead_cache
                            .extend(result.branch_ahead_cache);
                        let event = result.event;
                        if event.request_id == self.ide_panel.git.latest_request_id {
                            if receiver.refresh {
                                refresh_rerun |= self.ide_panel.git.finish_status_refresh();
                            }
                            let reload_graph =
                                event.notice.as_deref().is_some_and(|notice| {
                                    notice.starts_with("Committed ")
                                        || notice == "Fetch done"
                                        || notice == "Pull done"
                                });
                            self.ide_panel.git.apply_event(event);
                            status_event_applied = true;
                            self.ide_panel.git.pending = false;
                            if reload_graph {
                                reload_graph_cache = true;
                                prefetch_graph_after_status = self.ide_panel.git.graph_open;
                                force_prefetch_graph_after_status = prefetch_graph_after_status;
                            } else if self.ide_panel.git.graph_refresh_after_status
                                && self.ide_panel.git.graph_open
                            {
                                prefetch_graph_after_status = true;
                            }
                            updated = true;
                        } else {
                            if receiver.refresh && self.ide_panel.git.finish_status_refresh() {
                                stale_refresh_dirty = true;
                            }
                            stale_seen = true;
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        if receiver.refresh {
                            refresh_rerun |= self.ide_panel.git.finish_status_refresh();
                        }
                        if receiver.request_id == self.ide_panel.git.latest_request_id {
                            self.ide_panel.git.handle_status_disconnect(receiver.request_id);
                            updated = true;
                        }
                        keep = false;
                        break;
                    }
                }
            }
            if keep {
                next_rx.push(receiver);
            }
        }
        self.ide_panel.git.rx = next_rx;
        self.ide_panel.git.pending = self.ide_panel.git.rx.iter().any(|rx| rx.blocking);
        if !self.ide_panel.git.pending {
            self.ide_panel.git.stage_pending_workspace_idx = None;
            self.ide_panel.git.pending_started_at = None;
            if self
                .ide_panel
                .git
                .pending_label_until
                .is_none_or(|until| now >= until)
            {
                if self.ide_panel.git.pending_label.take().is_some() {
                    updated = true;
                }
                self.ide_panel.git.pending_label_until = None;
            } else {
                updated = true;
            }
        }
        if stale_refresh_dirty
            && !status_event_applied
            && self.ide_panel.git.applied_request_id < self.ide_panel.git.latest_request_id
        {
            refresh_rerun = true;
        }
        if stale_seen
            && self.ide_panel.git.rx.is_empty()
            && self.ide_panel.git.applied_request_id < self.ide_panel.git.latest_request_id
        {
            self.ide_panel.git.status_refresh_dirty = true;
            refresh_rerun = true;
        }
        if refresh_rerun
            && self.ide_panel.git.rx.is_empty()
            && !self.ide_panel.git.status_refresh_pending
        {
            self.spawn_git_task(GitAction::Refresh);
            updated = true;
        }
        if reload_graph_cache {
            self.ide_panel.git.graph_cache.clear();
            self.ide_panel.git.graph_latest_request_by_root.clear();
            self.ide_panel.git.graph_pending_roots.clear();
            self.ide_panel.git.graph_snapshot.clear();
            self.ide_panel.git.graph_lane_count = 1;
            self.ide_panel.git.graph_has_more = false;
            self.ide_panel.git.graph_pending = false;
        }
        if prefetch_graph_after_status {
            self.ide_panel.git.graph_refresh_after_status = false;
            self.prefetch_git_graph_for_known_workspaces(force_prefetch_graph_after_status);
        } else if self.ide_panel.git.graph_refresh_after_status && !self.ide_panel.git.graph_open {
            self.ide_panel.git.graph_refresh_after_status = false;
        }
        let mut next_graph_rx = Vec::with_capacity(self.ide_panel.git.graph_rx.len());
        let graph_receivers = std::mem::take(&mut self.ide_panel.git.graph_rx);
        for receiver in graph_receivers {
            let mut keep = true;
            loop {
                match receiver.rx.try_recv() {
                    Ok(event) => {
                        let latest_for_root = self
                            .ide_panel
                            .git
                            .graph_latest_request_by_root
                            .get(&crate::platform::PathKey::new(&event.repo_root))
                            .copied();
                        if latest_for_root == Some(event.request_id) {
                            self.ide_panel.git.graph_latest_request_id = self
                                .ide_panel
                                .git
                                .graph_latest_request_id
                                .max(event.request_id);
                            self.ide_panel
                                .git
                                .graph_latest_request_by_root
                                .remove(&crate::platform::PathKey::new(&event.repo_root));
                            self.ide_panel
                                .git
                                .graph_pending_roots
                                .remove(&crate::platform::PathKey::new(&event.repo_root));
                            let same_workspace =
                                self.ide_panel.git.graph_workspace_idx == Some(event.workspace_idx);
                            let same_root = self
                                .ide_panel
                                .git
                                .graph_repo_root
                                .as_ref()
                                .is_some_and(|root| crate::platform::paths_equal(root, &event.repo_root));
                            if same_workspace && same_root {
                                if event.offset == 0 {
                                    self.apply_git_graph_result(
                                        event.commits,
                                        event.lane_count,
                                        event.notice,
                                        event.limit,
                                        event.has_more,
                                        event.reset_scroll,
                                    );
                                } else if event.offset == self.ide_panel.git.graph_snapshot.len() {
                                    self.append_git_graph_result(
                                        event.commits,
                                        event.limit,
                                        event.has_more,
                                    );
                                }
                            } else if event.offset == 0 {
                                let cache_entry = GitGraphCacheEntry {
                                    commits: event.commits,
                                    lane_count: event.lane_count.max(1),
                                    notice: event.notice,
                                    limit: event.limit,
                                    has_more: event.has_more,
                                };
                                self.ide_panel
                                    .git
                                    .graph_cache
                                    .insert(
                                        crate::platform::PathKey::new(&event.repo_root),
                                        cache_entry,
                                    );
                            }
                            updated = true;
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.ide_panel
                            .git
                            .handle_graph_disconnect(&receiver.repo_root, receiver.request_id);
                        updated = true;
                        keep = false;
                        break;
                    }
                }
            }
            if keep {
                next_graph_rx.push(receiver);
            }
        }
        self.ide_panel.git.graph_rx = next_graph_rx;
        self.ide_panel.git.graph_pending = self
            .ide_panel
            .git
            .graph_repo_root
            .as_ref()
            .is_some_and(|root| {
                self.ide_panel
                    .git
                    .graph_pending_roots
                    .contains(&crate::platform::PathKey::new(root))
            });
        updated
    }

    pub fn toggle_git_graph(&mut self) {
        self.ide_panel.git.commit_menu_open = false;
        self.ide_panel.git.graph_open = !self.ide_panel.git.graph_open;
        if self.ide_panel.git.graph_open {
            self.ensure_git_graph_loaded();
        }
    }

    pub fn select_git_graph_workspace(&mut self, workspace_idx: usize) {
        self.ide_panel.git.commit_menu_open = false;
        if self.ide_panel.git.graph_workspace_idx == Some(workspace_idx) {
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
            self.ide_panel.git.graph_notice = Some("No Git repo".to_string());
            return;
        };
        self.ide_panel.git.graph_workspace_idx = Some(workspace_idx);
        self.ide_panel.git.graph_repo_root = Some(repo_root);
        self.ide_panel.git.graph_snapshot.clear();
        self.ide_panel.git.graph_lane_count = 1;
        self.ide_panel.git.graph_commit_limit = GIT_GRAPH_LIMIT_STEP;
        self.ide_panel.git.graph_has_more = false;
        self.ide_panel.git.graph_copied_commit = None;
        self.ide_panel.git.graph_scroll.current = 0.0;
        self.ide_panel.git.graph_scroll.target = 0.0;
        if !self.apply_cached_git_graph_for_selected(true) {
            self.load_git_graph_for_selected_workspace();
        }
    }

    pub fn copy_git_graph_commit(&mut self, workspace_idx: usize, commit_idx: usize) {
        if self.ide_panel.git.graph_workspace_idx != Some(workspace_idx) {
            return;
        }
        let Some(oid) = self
            .ide_panel
            .git
            .graph_snapshot
            .get(commit_idx)
            .map(|commit| commit.oid.to_string())
        else {
            return;
        };
        self.set_clipboard_text(oid);
        self.ide_panel.git.graph_copied_commit = Some((workspace_idx, commit_idx));
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.git_graph_tooltip_seen_copied = Some((workspace_idx, commit_idx));
            renderer.git_graph_tooltip_visible_copied = Some((workspace_idx, commit_idx));
        }
    }

    pub fn open_git_graph_commit(&mut self, workspace_idx: usize, commit_idx: usize) {
        if self.ide_panel.git.graph_workspace_idx != Some(workspace_idx) {
            return;
        }
        let Some(url) = self
            .ide_panel
            .git
            .graph_snapshot
            .get(commit_idx)
            .and_then(|commit| commit.github_url.clone())
        else {
            self.ide_panel.git.graph_notice = Some("No GitHub remote".to_string());
            return;
        };
        match open_url_async(&url) {
            Ok(()) => {
                self.ide_panel.git.graph_notice = Some("Opening GitHub".to_string());
            }
            Err(err) => {
                self.ide_panel.git.graph_notice = Some(err);
            }
        }
    }

    fn apply_git_graph_cache_entry(&mut self, cache_entry: GitGraphCacheEntry, reset_scroll: bool) {
        self.apply_git_graph_result(
            cache_entry.commits,
            cache_entry.lane_count,
            cache_entry.notice,
            cache_entry.limit,
            cache_entry.has_more,
            reset_scroll,
        );
    }

    fn apply_git_graph_result(
        &mut self,
        commits: Vec<GitGraphCommit>,
        lane_count: usize,
        notice: Option<String>,
        limit: usize,
        has_more: bool,
        reset_scroll: bool,
    ) {
        self.ide_panel.git.graph_snapshot = commits;
        self.ide_panel.git.graph_lane_count = lane_count.max(1);
        self.ide_panel.git.graph_notice = notice;
        self.ide_panel.git.graph_commit_limit = limit;
        self.ide_panel.git.graph_has_more = has_more;
        self.ide_panel.git.graph_pending = false;
        if reset_scroll {
            self.ide_panel.git.graph_scroll.set_target(0.0);
            self.ide_panel.git.graph_scroll.current = 0.0;
            self.ide_panel.git.graph_scroll.velocity = 0.0;
        }
    }

    fn append_git_graph_result(
        &mut self,
        mut commits: Vec<GitGraphCommit>,
        limit: usize,
        has_more: bool,
    ) {
        self.ide_panel.git.graph_snapshot.append(&mut commits);
        self.ide_panel.git.graph_lane_count =
            apply_git_graph_lanes(&mut self.ide_panel.git.graph_snapshot);
        self.ide_panel.git.graph_notice = None;
        self.ide_panel.git.graph_commit_limit = limit;
        self.ide_panel.git.graph_has_more = has_more;
        self.ide_panel.git.graph_pending = false;
    }

    fn apply_cached_git_graph_for_selected(&mut self, reset_scroll: bool) -> bool {
        let Some(repo_root) = self.ide_panel.git.graph_repo_root.clone() else {
            return false;
        };
        let Some(cache_entry) = self
            .ide_panel
            .git
            .graph_cache
            .get(&crate::platform::PathKey::new(&repo_root))
            .cloned()
        else {
            return false;
        };
        self.apply_git_graph_cache_entry(cache_entry, reset_scroll);
        true
    }

    fn ensure_git_graph_loaded(&mut self) {
        if self.ide_panel.git.graph_workspace_idx.is_none()
            || self.ide_panel.git.graph_workspace_idx.is_some_and(|idx| {
                !self
                    .ide_panel
                    .git
                    .snapshot
                    .workspaces
                    .iter()
                    .any(|workspace| {
                        workspace.workspace_idx == idx && workspace.repo_root.is_some()
                    })
            })
        {
            if let Some((workspace_idx, repo_root)) = self
                .ide_panel
                .git
                .snapshot
                .workspaces
                .iter()
                .find_map(|workspace| {
                    workspace
                        .repo_root
                        .as_ref()
                        .map(|repo_root| (workspace.workspace_idx, repo_root.clone()))
                })
            {
                self.ide_panel.git.graph_workspace_idx = Some(workspace_idx);
                self.ide_panel.git.graph_repo_root = Some(repo_root);
                self.ide_panel.git.graph_snapshot.clear();
                self.ide_panel.git.graph_lane_count = 1;
                self.ide_panel.git.graph_commit_limit = GIT_GRAPH_LIMIT_STEP;
                self.ide_panel.git.graph_has_more = false;
            }
        }
        if self.ide_panel.git.graph_snapshot.is_empty()
            && self.apply_cached_git_graph_for_selected(true)
        {
            return;
        }
        if self.ide_panel.git.graph_snapshot.is_empty() && !self.ide_panel.git.graph_pending {
            self.load_git_graph_for_selected_workspace();
        }
    }

    fn load_git_graph_for_selected_workspace(&mut self) {
        let Some(workspace_idx) = self.ide_panel.git.graph_workspace_idx else {
            self.ide_panel.git.graph_notice = Some("No Git workspace".to_string());
            return;
        };
        let Some(repo_root) = self.ide_panel.git.graph_repo_root.clone() else {
            self.ide_panel.git.graph_notice = Some("No Git repo".to_string());
            return;
        };
        if self
            .ide_panel
            .git
            .graph_pending_roots
            .contains(&crate::platform::PathKey::new(&repo_root))
        {
            self.ide_panel.git.graph_pending = true;
            return;
        }
        self.spawn_git_task(GitAction::LoadGraph {
            workspace_idx,
            repo_root,
            offset: 0,
            limit: self.ide_panel.git.graph_commit_limit,
            reset_scroll: self.ide_panel.git.graph_snapshot.is_empty(),
            activate: true,
        });
    }

    pub fn load_more_git_graph_commits(&mut self) {
        if self.ide_panel.git.graph_pending || !self.ide_panel.git.graph_has_more {
            return;
        }
        let Some(workspace_idx) = self.ide_panel.git.graph_workspace_idx else {
            return;
        };
        let Some(repo_root) = self.ide_panel.git.graph_repo_root.clone() else {
            return;
        };
        let offset = self.ide_panel.git.graph_snapshot.len();
        let next_limit = self
            .ide_panel
            .git
            .graph_commit_limit
            .saturating_add(GIT_GRAPH_LIMIT_STEP);
        self.ide_panel.git.graph_commit_limit = next_limit;
        self.spawn_git_task(GitAction::LoadGraph {
            workspace_idx,
            repo_root,
            offset,
            limit: next_limit,
            reset_scroll: false,
            activate: true,
        });
    }

    fn prefetch_git_graph_for_repo(
        &mut self,
        workspace_idx: usize,
        repo_root: PathBuf,
        limit: usize,
        force_reload: bool,
    ) {
        if self
            .ide_panel
            .git
            .graph_latest_request_by_root
            .contains_key(&crate::platform::PathKey::new(&repo_root))
        {
            return;
        }
        let is_active_graph = self
            .ide_panel
            .git
            .graph_repo_root
            .as_ref()
            .is_some_and(|active| crate::platform::paths_equal(active, &repo_root));
        let active_loaded_len = if is_active_graph {
            self.ide_panel.git.graph_snapshot.len()
        } else {
            0
        };
        let cached_limit = self
            .ide_panel
            .git
            .graph_cache
            .get(&crate::platform::PathKey::new(&repo_root))
            .map(|cache| cache.limit);
        if !git_graph_prefetch_needed(
            force_reload,
            is_active_graph,
            active_loaded_len,
            cached_limit,
            limit,
        ) {
            return;
        }
        self.spawn_git_task(GitAction::LoadGraph {
            workspace_idx,
            repo_root,
            offset: 0,
            limit,
            reset_scroll: false,
            activate: false,
        });
    }

    fn prefetch_git_graph_for_known_workspaces(&mut self, force_reload: bool) {
        let mut seen = FxHashSet::default();
        let workspaces = self
            .ide_panel
            .git
            .snapshot
            .workspaces
            .iter()
            .filter_map(|workspace| {
                let repo_root = workspace.repo_root.clone()?;
                seen.insert(crate::platform::PathKey::new(&repo_root))
                    .then_some((workspace.workspace_idx, repo_root))
            })
            .collect::<Vec<_>>();
        for (workspace_idx, repo_root) in workspaces {
            let limit = if self
                .ide_panel
                .git
                .graph_repo_root
                .as_ref()
                .is_some_and(|active| crate::platform::paths_equal(active, &repo_root))
            {
                self.ide_panel.git.graph_commit_limit
            } else {
                GIT_GRAPH_LIMIT_STEP
            };
            self.prefetch_git_graph_for_repo(workspace_idx, repo_root, limit, force_reload);
        }
    }

    pub(crate) fn prefetch_active_tab_git_graph(&mut self) {
        if !self.is_ide_mode || !self.is_ready || !self.ide_panel.is_open(crate::app::PanelId::Git)
        {
            return;
        }
        let Some(file_path) = self.file_path.as_ref() else {
            return;
        };
        let abs_path = git_abs_path_for_workspaces(file_path, &self.ide_workspaces);
        let Some((workspace_idx, repo_root)) =
            git_graph_workspace_for_path(&self.ide_panel.git.snapshot, &abs_path)
        else {
            return;
        };
        self.prefetch_git_graph_for_repo(workspace_idx, repo_root, GIT_GRAPH_LIMIT_STEP, false);
    }

}
