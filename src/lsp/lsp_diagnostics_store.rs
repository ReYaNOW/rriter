#[cfg(target_os = "linux")]
fn trim_allocator_after_large_diagnostics(count: usize, workspace_done: bool) {
    if count < 1024 && !workspace_done {
        return;
    }
    unsafe extern "C" {
        fn malloc_trim(pad: usize) -> i32;
    }
    unsafe {
        malloc_trim(0);
    }
}

#[cfg(not(target_os = "linux"))]
fn trim_allocator_after_large_diagnostics(_count: usize, _workspace_done: bool) {}

/// Replaces `value` with the copy of its text stored in `pool`, or stores it there.
fn intern_text(pool: &mut std::collections::HashSet<Arc<str>>, value: &mut Arc<str>) {
    if let Some(stored) = pool.get(value.as_ref()) {
        if !Arc::ptr_eq(stored, value) {
            *value = stored.clone();
        }
        return;
    }
    pool.insert(value.clone());
}

impl LspManager {
    fn prune_inactive_workspace_diagnostics(&mut self) {
        let active_workspaces = self.active_workspaces.clone();
        let open_python_files = self
            .open_python_files
            .keys()
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        let open_dart_files = self
            .dart.open_files()
            .keys()
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        let dart_roots = self
            .dart.roots()
            .values()
            .map(|state| state.root.clone())
            .collect::<Vec<_>>();
        let keep_path = |path: &Path| {
            active_workspaces
                .iter()
                .any(|ws| crate::platform::path_is_within(path, ws))
                || open_python_files.contains(&crate::platform::PathKey::new(path))
                || open_dart_files.contains(&crate::platform::PathKey::new(path))
                || dart_roots
                    .iter()
                    .any(|root| crate::platform::path_is_within(path, root))
        };
        let before = self.diagnostics.len()
            + self.instant_diagnostics.len()
            + self.ruff_workspace_diagnostics.len()
            + self.ty_instant_diagnostics.len()
            + self.dart.live_diagnostics().len()
            + self.dart_workspace_diagnostics.len()
            + self.ty_diag_result_ids.len();
        self.diagnostics.retain(|path, _| keep_path(path));
        self.instant_diagnostics.retain(|path, _| keep_path(path));
        self.ruff_workspace_diagnostics
            .retain(|path, _| keep_path(path));
        self.ty_instant_diagnostics
            .retain(|path, _| keep_path(path));
        self.dart.retain_live_diagnostics(|path, _| keep_path(path));
        self.dart_workspace_diagnostics
            .retain(|path, _| keep_path(path));
        self.ty_diag_result_ids.retain(|path, _| keep_path(path));
        self.prune_diag_text_pool();
        let after = self.diagnostics.len()
            + self.instant_diagnostics.len()
            + self.ruff_workspace_diagnostics.len()
            + self.ty_instant_diagnostics.len()
            + self.dart.live_diagnostics().len()
            + self.dart_workspace_diagnostics.len()
            + self.ty_diag_result_ids.len();
        if before != after {
            self.rebuild_diagnostic_summary();
            self.dirty_diagnostics = false;
        }
    }

    fn ty_workspace_result_ids_json(&self) -> String {
        if self.ty_diag_result_ids.is_empty() {
            return String::from("[]");
        }

        let mut items = Vec::with_capacity(self.ty_diag_result_ids.len());
        for (path, value) in &self.ty_diag_result_ids {
            let uri = path_to_uri(path);
            items.push(format!(
                r#"{{"uri":"{}","value":"{}"}}"#,
                json_escape(&uri),
                json_escape(value)
            ));
        }
        format!("[{}]", items.join(","))
    }

    fn request_ty_workspace_diagnostics_if_ready(&mut self) {
        if !self.ty_workspace_diag_dirty
            || self.ty_workspace_diag_pending.is_some()
            || self.python_disabled
            || self.ty_disabled
            || self.suppress_diagnostics
            || self.ty_status != LspServerStatus::Running
            || self.active_workspaces.is_empty()
        {
            return;
        }
        if self
            .last_change
            .is_some_and(|last| last.elapsed().as_secs_f32() < 3.0)
        {
            return;
        }

        let previous_result_ids_json = self.ty_workspace_result_ids_json();
        if let Some(proc) = &mut self.ty_process
            && let Some(id) = proc.request_workspace_diagnostics(previous_result_ids_json)
        {
            self.ty_workspace_diag_pending = Some(id);
            self.ty_workspace_diag_dirty = false;
        }
    }

    fn should_accept_diagnostics_version(
        existing_version: Option<i32>,
        incoming_version: Option<i32>,
        is_open_file: bool,
    ) -> bool {
        if !is_open_file {
            return true;
        }
        match (existing_version, incoming_version) {
            (None, _) => true,
            (Some(_), None) => false,
            (Some(existing), Some(incoming)) => incoming >= existing,
        }
    }

    fn store_dart_live_diagnostics(
        &mut self,
        path: &Path,
        version: i32,
        items: Arc<[Diagnostic]>,
        root: Option<&crate::platform::PathKey>,
    ) {
        if let Some(root) = root {
            self.dart.insert_live_diagnostics(
                path.to_path_buf(),
                rooted_language::LiveDiagnostics {
                    version,
                    root: root.clone(),
                    items,
                },
            );
        }
    }

    /// Опрашивает события от всех серверов. Вызывать раз в кадр.
    /// Обновляет self.diagnostics при получении новых диагностик.
    pub fn poll(&mut self) -> Vec<LspEvent> {
        let mut all = Vec::new();

        if let Some(proc) = &self.python {
            proc.poll(&mut all);
        }
        if let Some(proc) = &mut self.ty_process {
            proc.poll(&mut all);
        }
        self.poll_dart_processes(&mut all);

        // Обновляем кешированные диагностики и статусы
        let mut received_diagnostics = 0usize;
        let mut diagnostics_replaced = false;
        let mut workspace_diagnostics_done = false;
        let mut batch_messages = std::collections::HashSet::new();
        for ev in &mut all {
            match ev {
                LspEvent::Diagnostics {
                    server,
                    path,
                    version,
                    items,
                    result_id,
                    ..
                } => {
                    if !self.suppress_diagnostics {
                        let is_ty = *server == LspServerKind::Ty;
                        let is_dart = *server == LspServerKind::Dart;
                        let existing_version = if is_ty {
                            self.ty_instant_diagnostics
                                .get(path)
                                .map(|(version, _)| *version)
                        } else if is_dart {
                            self.dart.live_diagnostics()
                                .get(path)
                                .map(|diagnostics| diagnostics.version)
                        } else {
                            self.instant_diagnostics
                                .get(path)
                                .map(|(version, _)| *version)
                        };
                        let path_key = crate::platform::PathKey::new(path);
                        let is_open_file = if is_dart {
                            self.dart.open_files().contains_key(&path_key)
                        } else {
                            self.open_python_files.contains_key(&path_key)
                        };
                        let current_dart_version =
                            is_dart.then(|| self.dart_document_version(path)).flatten();
                        let diagnostic_root = if is_dart {
                            self.dart.root_for_open_path(path).map(crate::platform::PathKey::new)
                        } else {
                            None
                        };
                        let version_is_current = if is_dart {
                            is_open_file
                                && current_dart_version.is_some_and(|current| {
                                    version.is_none_or(|incoming| incoming >= current)
                                })
                        } else {
                            true
                        };
                        if version_is_current
                            && (is_dart && version.is_none()
                                || Self::should_accept_diagnostics_version(
                                    existing_version,
                                    *version,
                                    is_open_file,
                                ))
                        {
                            let stored_version = if is_dart {
                                version.or(current_dart_version).unwrap_or(0)
                            } else {
                                version.unwrap_or(0)
                            };
                            received_diagnostics = received_diagnostics.saturating_add(items.len());
                            if is_dart {
                                for diagnostic in items.iter_mut() {
                                    diagnostic.source =
                                        Some(Arc::<str>::from(dart_workspace::DART_SERVER_NAME));
                                }
                            }
                            self.compact_diagnostic_text(items, Some(&mut batch_messages));

                            if is_ty {
                                if let Some(result_id) = result_id.as_ref() {
                                    self.ty_diag_result_ids
                                        .insert(path.clone(), result_id.clone());
                                }
                                let items = Arc::<[Diagnostic]>::from(std::mem::take(items));
                                self.ty_instant_diagnostics
                                    .insert(path.clone(), (stored_version, items));
                            } else if is_dart {
                                let items = Arc::<[Diagnostic]>::from(std::mem::take(items));
                                self.store_dart_live_diagnostics(
                                    path,
                                    stored_version,
                                    items,
                                    diagnostic_root.as_ref(),
                                );
                            } else {
                                let items = Arc::<[Diagnostic]>::from(std::mem::take(items));
                                self.instant_diagnostics
                                    .insert(path.clone(), (stored_version, items));
                            }

                            self.mark_diagnostics_changed();
                            self.last_change = None;
                            diagnostics_replaced = true;
                        } else {
                            items.clear();
                        }
                    }
                }
                LspEvent::StatusChanged { server, status } => {
                    if *server == LspServerKind::Dart {
                        if *status == LspServerStatus::Missing {
                            self.mark_dart_missing();
                        }
                        continue;
                    }
                    if *server == LspServerKind::Ty {
                        self.ty_status = status.clone();
                        if *status == LspServerStatus::Running {
                            self.ty_unavailable = false;
                            self.ty_workspace_diag_dirty = true;
                        } else if *status == LspServerStatus::Starting
                            || *status == LspServerStatus::Crashed
                            || *status == LspServerStatus::Missing
                            || *status == LspServerStatus::Disabled
                        {
                            self.ty_workspace_diag_pending = None;
                            if matches!(
                                status,
                                LspServerStatus::Disabled | LspServerStatus::Missing
                            ) {
                                self.ty_unavailable = true;
                                self.ty_process = None;
                            }
                        }
                    } else {
                        self.python_status = status.clone();
                        if *status == LspServerStatus::Running {
                            self.ruff_unavailable = false;
                            self.ruff_workspace_diag_dirty = true;
                        } else if matches!(
                            status,
                            LspServerStatus::Disabled | LspServerStatus::Missing
                        ) {
                            self.ruff_unavailable = true;
                            self.python = None;
                            self.ruff_workspace_diag_rx = None;
                            self.ruff_workspace_diag_pending = false;
                            self.ruff_workspace_diag_dirty = false;
                        }
                    }
                }
                LspEvent::ConfigurationServed { server } => {
                    if *server == LspServerKind::Ty {
                        self.ty_workspace_diag_dirty = true;
                    }
                }
                LspEvent::WorkspaceDiagnosticsDone { request_id } => {
                    if self.ty_workspace_diag_pending == Some(*request_id) {
                        self.ty_workspace_diag_pending = None;
                    }
                    workspace_diagnostics_done = true;
                }
                LspEvent::Log { name, message } => {
                    let (final_text, spans, folds) = format_lsp_log_entry(message);
                    *message = final_text.clone();
                    let logs = self.server_logs.entry(*name).or_insert_with(Vec::new);
                    let now = Instant::now();
                    logs.push(LogEntry {
                        text: final_text,
                        spans,
                        folds,
                        created_at: now,
                    });
                    trim_lsp_logs(logs, now);
                }
                _ => {}
            }
        }

        let ruff_workspace_received = self.poll_ruff_workspace_diagnostics();
        if ruff_workspace_received > 0 {
            received_diagnostics = received_diagnostics.saturating_add(ruff_workspace_received);
            workspace_diagnostics_done = true;
        }
        let dart_workspace_received = self.poll_dart_workspace_diagnostics();
        if dart_workspace_received > 0 {
            received_diagnostics = received_diagnostics.saturating_add(dart_workspace_received);
            workspace_diagnostics_done = true;
        }

        self.finish_diagnostics_poll(diagnostics_replaced);
        self.request_ty_workspace_diagnostics_if_ready();
        self.request_ruff_workspace_diagnostics_if_ready();
        trim_allocator_after_large_diagnostics(received_diagnostics, workspace_diagnostics_done);

        all
    }

    /// Drops texts of replaced sets and rebuilds the summary once the burst of changes settles.
    fn finish_diagnostics_poll(&mut self, diagnostics_replaced: bool) {
        if diagnostics_replaced {
            self.prune_diag_text_pool();
        }
        if let Some(t) = self.last_change {
            if t.elapsed().as_secs_f32() < 3.0 {
                return;
            }
            self.last_change = None;
        }
        if self.dirty_diagnostics {
            self.rebuild_diagnostic_summary();
            self.dirty_diagnostics = false;
        }
    }

    pub fn get_diagnostics(&self, path: &Path) -> &[Diagnostic] {
        if path.is_absolute() {
            self.diagnostics
                .get(path)
                .map(|v| v.as_ref())
                .unwrap_or(&[])
        } else if let Some(ws) = self.workspaces.first() {
            let abs_path = ws.join(path);
            self.diagnostics
                .get(abs_path.as_path())
                .map(|v| v.as_ref())
                .unwrap_or(&[])
        } else {
            let abs_path = self.relative_lookup_path(path);
            self.diagnostics
                .get(abs_path.as_path())
                .map(|v| v.as_ref())
                .unwrap_or(&[])
        }
    }

    pub fn clear_diagnostics_for_path(&mut self, path: &PathBuf) {
        let abs_path = if path.is_absolute() {
            path.clone()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            std::env::current_dir().unwrap_or_default().join(path)
        };
        self.mark_diagnostics_changed();
        self.diagnostics.remove(&abs_path);
        self.instant_diagnostics.remove(&abs_path);
        self.ruff_workspace_diagnostics.remove(&abs_path);
        self.ty_instant_diagnostics.remove(&abs_path);
        self.dart.remove_live_diagnostics(&abs_path);
        self.dart_workspace_diagnostics.remove(&abs_path);
        self.ty_diag_result_ids.remove(&abs_path);
        self.prune_diag_text_pool();
        self.rebuild_diagnostic_summary();
        self.dirty_diagnostics = false;
    }

    /// Points code, URL and source of `items` at the pool's shared copy, and every message at
    /// one copy per text within the batch `messages` collects (`None`: the producer already
    /// shared them). Messages are not kept in the
    /// persistent pool: a workspace check may repeat them across files (ty on a Django
    /// project: ~9.8k diagnostics, ~2.1k distinct messages) or not at all (ruff on this repo:
    /// ~8k diagnostics, ~8k distinct), and a pool entry per unique message would cost more
    /// than the sharing saves.
    fn compact_diagnostic_text(
        &mut self,
        items: &mut [Diagnostic],
        mut messages: Option<&mut std::collections::HashSet<Arc<str>>>,
    ) {
        for diag in items {
            if let Some(messages) = messages.as_deref_mut() {
                intern_text(messages, &mut diag.message);
            }
            for text in [&mut diag.code, &mut diag.code_href, &mut diag.source]
                .into_iter()
                .flatten()
            {
                intern_text(&mut self.diag_text_pool, text);
            }
        }
    }

    /// Drops pool entries no stored diagnostic references any more: the pool's own reference
    /// is then the only one, so no walk over the diagnostics is needed.
    fn prune_diag_text_pool(&mut self) {
        self.diag_text_pool.retain(|text| Arc::strong_count(text) > 1);
        if self.diag_text_pool.capacity() > self.diag_text_pool.len() * 2 + 64 {
            self.diag_text_pool.shrink_to_fit();
        }
    }

    fn lookup_abs_path(&self, path: &Path) -> PathBuf {
        self.diagnostic_lookup_path(path).into_owned()
    }

    /// Absolute key of `path` in the diagnostics maps; borrows absolute paths, so per-row
    /// lookups from the Problems panel do not allocate.
    fn diagnostic_lookup_path<'a>(&self, path: &'a Path) -> std::borrow::Cow<'a, Path> {
        if path.is_absolute() {
            std::borrow::Cow::Borrowed(path)
        } else if let Some(ws) = self.workspaces.first() {
            std::borrow::Cow::Owned(ws.join(path))
        } else {
            std::borrow::Cow::Owned(self.relative_lookup_path(path))
        }
    }

    fn ruff_workspace_diagnostics_for_abs_path(&self, path: &Path) -> Option<&Arc<[Diagnostic]>> {
        if self
            .open_python_files
            .contains_key(&crate::platform::PathKey::new(path))
            || self.instant_diagnostics.contains_key(path)
        {
            return None;
        }
        self.ruff_workspace_diagnostics.get(path)
    }

    fn dart_workspace_diagnostics_for_abs_path(&self, path: &Path) -> Option<&Arc<[Diagnostic]>> {
        if self
            .dart.open_files()
            .contains_key(&crate::platform::PathKey::new(path))
            || self.dart.live_diagnostics().contains_key(path)
        {
            return None;
        }
        self.dart_workspace_diagnostics.get(path)
    }

    /// Every diagnostic shown for `path`, in display index order: Ruff (live, else workspace
    /// check), ty, Dart (live, else workspace analysis), and the legacy store only when those
    /// have none. Display indices are positions in this concatenation, computed from the
    /// stored slices on demand instead of a per-diagnostic index table.
    fn diagnostic_slices_for_abs_path(&self, path: &Path) -> [&[Diagnostic]; 4] {
        let ruff = match self.instant_diagnostics.get(path) {
            Some((_, diagnostics)) => diagnostics.as_ref(),
            None => self
                .ruff_workspace_diagnostics_for_abs_path(path)
                .map_or(&[][..], |diagnostics| diagnostics.as_ref()),
        };
        let ty = self
            .ty_instant_diagnostics
            .get(path)
            .map_or(&[][..], |(_, diagnostics)| diagnostics.as_ref());
        let dart = match self.dart.live_diagnostics().get(path) {
            Some(diagnostics) => diagnostics.items.as_ref(),
            None => self
                .dart_workspace_diagnostics_for_abs_path(path)
                .map_or(&[][..], |diagnostics| diagnostics.as_ref()),
        };
        let legacy = if ruff.is_empty() && ty.is_empty() && dart.is_empty() {
            self.diagnostics
                .get(path)
                .map_or(&[][..], |diagnostics| diagnostics.as_ref())
        } else {
            &[]
        };
        [ruff, ty, dart, legacy]
    }

    fn rebuild_diagnostic_summary(&mut self) {
        // Every visible diagnostics change passes through here, including
        // paths that clear the dirty flag themselves (workspace pruning).
        self.diagnostic_generation = self.diagnostic_generation.wrapping_add(1);
        let mut ancestor_severities: HashMap<PathBuf, DiagSeverity> = HashMap::new();
        let mut total_counts = (0usize, 0usize);
        for path in self.diagnostic_paths() {
            let mut summary = None;
            for diagnostic in self.diagnostic_slices_for_abs_path(path).into_iter().flatten() {
                match diagnostic.severity {
                    DiagSeverity::Error => total_counts.0 += 1,
                    DiagSeverity::Warning => total_counts.1 += 1,
                    _ => {}
                }
                Self::update_severity(&mut summary, diagnostic);
            }
            let Some(severity) = summary else {
                continue;
            };
            let Some(parent) = path.parent() else {
                continue;
            };
            for ancestor in parent.ancestors() {
                match ancestor_severities.get_mut(ancestor) {
                    // Every ancestor above an already summarized one is summarized too.
                    Some(entry) if *entry == DiagSeverity::Error || *entry == severity => break,
                    Some(entry) => *entry = severity,
                    None => {
                        ancestor_severities.insert(ancestor.to_path_buf(), severity);
                    }
                }
            }
        }
        self.diagnostic_ancestor_severities = ancestor_severities;
        self.diagnostic_total_counts = total_counts;
    }

    pub fn diagnostic_at(&self, path: &Path, index: usize) -> Option<&Diagnostic> {
        let abs_path = self.diagnostic_lookup_path(path);
        let mut index = index;
        for diagnostics in self.diagnostic_slices_for_abs_path(&abs_path) {
            if let Some(diagnostic) = diagnostics.get(index) {
                return Some(diagnostic);
            }
            index -= diagnostics.len();
        }
        None
    }

    pub fn diagnostic_count(&self, path: &Path) -> usize {
        let abs_path = self.diagnostic_lookup_path(path);
        self.diagnostic_slices_for_abs_path(&abs_path)
            .iter()
            .map(|diagnostics| diagnostics.len())
            .sum()
    }

    pub fn diagnostic_entries_for_path(&self, path: &Path) -> Vec<(usize, &Diagnostic)> {
        let abs_path = self.diagnostic_lookup_path(path);
        self.diagnostic_slices_for_abs_path(&abs_path)
            .into_iter()
            .flatten()
            .enumerate()
            .collect()
    }

    pub fn diagnostic_entries_with_slices_for_path(
        &self,
        path: &Path,
    ) -> Vec<(usize, &Arc<[Diagnostic]>, u32)> {
        let abs_path = self.diagnostic_lookup_path(path);
        let mut entries = Vec::new();
        let mut global_index = 0;
        for diagnostics in self
            .diagnostic_arc_slices_for_abs_path(&abs_path)
            .into_iter()
            .flatten()
        {
            entries.extend((0..diagnostics.len()).map(|local_index| {
                let index = global_index + local_index;
                (index, diagnostics, local_index as u32)
            }));
            global_index += diagnostics.len();
        }
        entries
    }

    fn diagnostic_arc_slices_for_abs_path(&self, path: &Path) -> [Option<&Arc<[Diagnostic]>>; 4] {
        let ruff = match self.instant_diagnostics.get(path) {
            Some((_, diagnostics)) => Some(diagnostics),
            None => self.ruff_workspace_diagnostics_for_abs_path(path),
        };
        let ty = self
            .ty_instant_diagnostics
            .get(path)
            .map(|(_, diagnostics)| diagnostics);
        let dart = match self.dart.live_diagnostics().get(path) {
            Some(diagnostics) => Some(&diagnostics.items),
            None => self.dart_workspace_diagnostics_for_abs_path(path),
        };
        let legacy = if ruff.is_none_or(|diagnostics| diagnostics.is_empty())
            && ty.is_none_or(|diagnostics| diagnostics.is_empty())
            && dart.is_none_or(|diagnostics| diagnostics.is_empty())
        {
            self.diagnostics.get(path)
        } else {
            None
        };
        [ruff, ty, dart, legacy]
    }

    fn update_severity(summary: &mut Option<DiagSeverity>, diagnostic: &Diagnostic) -> bool {
        match diagnostic.severity {
            DiagSeverity::Error => {
                *summary = Some(DiagSeverity::Error);
                true
            }
            DiagSeverity::Warning => {
                if summary.is_none() {
                    *summary = Some(DiagSeverity::Warning);
                }
                false
            }
            _ => false,
        }
    }

    fn diagnostic_severity_for_abs_path_direct(&self, path: &Path) -> Option<DiagSeverity> {
        let mut summary = None;
        for diagnostic in self.diagnostic_slices_for_abs_path(path).into_iter().flatten() {
            if Self::update_severity(&mut summary, diagnostic) {
                return summary;
            }
        }
        summary
    }

    /// Sorted, de-duplicated paths of every diagnostics map.
    pub fn diagnostic_paths(&self) -> Vec<&PathBuf> {
        let mut paths: Vec<&PathBuf> = self
            .diagnostics
            .keys()
            .chain(self.instant_diagnostics.keys())
            .chain(self.ruff_workspace_diagnostics.keys())
            .chain(self.ty_instant_diagnostics.keys())
            .chain(self.dart.live_diagnostics().keys())
            .chain(self.dart_workspace_diagnostics.keys())
            .collect();
        paths.sort_unstable();
        paths.dedup();
        paths
    }

    pub fn diagnostic_counts_for_path(&self, path: &Path) -> (usize, usize) {
        let mut errors = 0usize;
        let mut warnings = 0usize;
        for (_, diagnostic) in self.diagnostic_entries_for_path(path) {
            match diagnostic.severity {
                DiagSeverity::Error => errors += 1,
                DiagSeverity::Warning => warnings += 1,
                _ => {}
            }
        }
        (errors, warnings)
    }

    pub fn total_diagnostic_counts(&self) -> (usize, usize) {
        self.diagnostic_total_counts
    }

    pub fn ruff_diagnostic_storage_counts(&self) -> (usize, usize) {
        let mut paths = std::collections::HashSet::new();
        let mut count = 0usize;
        for (path, (_, diagnostics)) in &self.instant_diagnostics {
            paths.insert(path);
            count = count.saturating_add(diagnostics.len());
        }
        for (path, diagnostics) in &self.ruff_workspace_diagnostics {
            paths.insert(path);
            count = count.saturating_add(diagnostics.len());
        }
        (paths.len(), count)
    }

    pub fn diagnostic_severity_for_path(&self, path: &Path) -> Option<DiagSeverity> {
        let abs_path = self.diagnostic_lookup_path(path);
        self.diagnostic_severity_for_abs_path_direct(&abs_path)
    }

    pub fn diagnostic_severity_under_path(&self, path: &Path) -> Option<DiagSeverity> {
        let abs_path = self.diagnostic_lookup_path(path);
        if !self.dirty_diagnostics {
            return self
                .diagnostic_ancestor_severities
                .get(abs_path.as_ref())
                .copied()
                .or_else(|| self.diagnostic_severity_for_abs_path_direct(&abs_path));
        }
        let mut summary = None;
        for diagnostic_path in self
            .diagnostics
            .keys()
            .chain(self.instant_diagnostics.keys())
            .chain(self.ruff_workspace_diagnostics.keys())
            .chain(self.ty_instant_diagnostics.keys())
            .chain(self.dart.live_diagnostics().keys())
            .chain(self.dart_workspace_diagnostics.keys())
        {
            if crate::platform::path_is_within(diagnostic_path, &abs_path)
                && let Some(severity) =
                    self.diagnostic_severity_for_abs_path_direct(diagnostic_path)
            {
                if severity == DiagSeverity::Error {
                    return Some(DiagSeverity::Error);
                }
                summary = Some(DiagSeverity::Warning);
            }
        }
        summary
    }

    pub fn diagnostic_refs_for_path(&self, path: &Path) -> Vec<&Diagnostic> {
        self.diagnostic_entries_for_path(path)
            .into_iter()
            .map(|(_, diagnostic)| diagnostic)
            .collect()
    }

    fn instant_merged_diagnostics_for_abs_path(&self, path: &Path) -> (i32, Vec<&Diagnostic>) {
        let ruff = self.instant_diagnostics.get(path);
        let ruff_workspace = if ruff.is_none() {
            self.ruff_workspace_diagnostics_for_abs_path(path)
        } else {
            None
        };
        let ty = self.ty_instant_diagnostics.get(path);
        let dart = self.dart.live_diagnostics().get(path);
        let dart_workspace = if dart.is_none() {
            self.dart_workspace_diagnostics_for_abs_path(path)
        } else {
            None
        };
        let count = ruff.map_or(0, |(_, diags)| diags.len())
            + ruff_workspace.map_or(0, |diags| diags.len())
            + ty.map_or(0, |(_, diags)| diags.len())
            + dart.map_or(0, |diagnostics| diagnostics.items.len())
            + dart_workspace.map_or(0, |diags| diags.len());
        if count == 0 {
            return (0, Vec::new());
        }

        let mut merged = Vec::with_capacity(count);
        let mut max_v = 0;
        if let Some((version, diagnostics)) = ruff {
            max_v = max_v.max(*version);
            merged.extend(diagnostics.iter());
        }
        if let Some(diagnostics) = ruff_workspace {
            merged.extend(diagnostics.iter());
        }
        if let Some((version, diagnostics)) = ty {
            max_v = max_v.max(*version);
            merged.extend(diagnostics.iter());
        }
        if let Some(diagnostics) = dart {
            max_v = max_v.max(diagnostics.version);
            merged.extend(diagnostics.items.iter());
        }
        if let Some(diagnostics) = dart_workspace {
            merged.extend(diagnostics.iter());
        }
        (max_v, merged)
    }

    pub fn instant_merged_diagnostics(&self, path: &Path) -> (i32, Vec<&Diagnostic>) {
        if path.is_absolute() {
            self.instant_merged_diagnostics_for_abs_path(path)
        } else if let Some(ws) = self.workspaces.first() {
            let abs_path = ws.join(path);
            self.instant_merged_diagnostics_for_abs_path(abs_path.as_path())
        } else {
            let abs_path = self.relative_lookup_path(path);
            self.instant_merged_diagnostics_for_abs_path(abs_path.as_path())
        }
    }

    pub fn has_stale_instant_diagnostics(&self, path: &Path, editor_version: u64) -> bool {
        if path.is_absolute() {
            let is_stale = |diags: &HashMap<PathBuf, (i32, Arc<[Diagnostic]>)>| {
                diags
                    .get(path)
                    .is_some_and(|(version, _)| (*version as u64) < editor_version)
            };
            is_stale(&self.instant_diagnostics)
                || is_stale(&self.ty_instant_diagnostics)
                || self.dart.live_diagnostics().get(path)
                    .is_some_and(|diagnostics| (diagnostics.version as u64) < editor_version)
        } else if let Some(ws) = self.workspaces.first() {
            let abs_path = ws.join(path);
            let is_stale = |diags: &HashMap<PathBuf, (i32, Arc<[Diagnostic]>)>| {
                diags
                    .get(abs_path.as_path())
                    .is_some_and(|(version, _)| (*version as u64) < editor_version)
            };
            is_stale(&self.instant_diagnostics)
                || is_stale(&self.ty_instant_diagnostics)
                || self.dart.live_diagnostics().get(abs_path.as_path())
                    .is_some_and(|diagnostics| (diagnostics.version as u64) < editor_version)
        } else {
            let abs_path = self.relative_lookup_path(path);
            let is_stale = |diags: &HashMap<PathBuf, (i32, Arc<[Diagnostic]>)>| {
                diags
                    .get(abs_path.as_path())
                    .is_some_and(|(version, _)| (*version as u64) < editor_version)
            };
            is_stale(&self.instant_diagnostics)
                || is_stale(&self.ty_instant_diagnostics)
                || self.dart.live_diagnostics().get(abs_path.as_path())
                    .is_some_and(|diagnostics| (diagnostics.version as u64) < editor_version)
        }
    }
    /// Диагностики для текущего файла, отфильтрованные по строке
    pub fn diagnostics_for_line(&self, path: &PathBuf, line: u32) -> Vec<&Diagnostic> {
        self.diagnostic_entries_for_path(path)
            .into_iter()
            .map(|(_, diagnostic)| diagnostic)
            .filter(move |d| d.start_line == line)
            .collect()
    }
}
