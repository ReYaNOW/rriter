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

impl LspManager {
    fn prune_inactive_workspace_diagnostics(&mut self) {
        let active_workspaces = self.active_workspaces.clone();
        let open_python_files = self
            .open_python_files
            .keys()
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        let open_dart_files = self
            .open_dart_files
            .keys()
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        let dart_roots = self
            .dart_workspaces
            .values()
            .map(|state| state.root.clone())
            .collect::<Vec<_>>();
        let keep_path = |path: &PathBuf| {
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
            + self.dart_live_diagnostics.len()
            + self.dart_workspace_diagnostics.len()
            + self.merged_diagnostic_indices.len()
            + self.ty_diag_result_ids.len();
        self.diagnostics.retain(|path, _| keep_path(path));
        self.instant_diagnostics.retain(|path, _| keep_path(path));
        self.ruff_workspace_diagnostics
            .retain(|path, _| keep_path(path));
        self.ty_instant_diagnostics
            .retain(|path, _| keep_path(path));
        self.dart_live_diagnostics.retain(|path, _| keep_path(path));
        self.dart_workspace_diagnostics
            .retain(|path, _| keep_path(path));
        self.merged_diagnostic_indices
            .retain(|path, _| keep_path(path));
        self.ty_diag_result_ids.retain(|path, _| keep_path(path));
        self.rebuild_diag_text_pool();
        let after = self.diagnostics.len()
            + self.instant_diagnostics.len()
            + self.ruff_workspace_diagnostics.len()
            + self.ty_instant_diagnostics.len()
            + self.dart_live_diagnostics.len()
            + self.dart_workspace_diagnostics.len()
            + self.merged_diagnostic_indices.len()
            + self.ty_diag_result_ids.len();
        if before != after {
            self.rebuild_merged_diagnostic_indices();
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
        let mut workspace_diagnostics_done = false;
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
                            self.dart_live_diagnostics
                                .get(path)
                                .map(|(version, _)| *version)
                        } else {
                            self.instant_diagnostics
                                .get(path)
                                .map(|(version, _)| *version)
                        };
                        let path_key = crate::platform::PathKey::new(path);
                        let is_open_file = if is_dart {
                            self.open_dart_files.contains_key(&path_key)
                        } else {
                            self.open_python_files.contains_key(&path_key)
                        };
                        let current_dart_version =
                            is_dart.then(|| self.dart_document_version(path)).flatten();
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
                            self.compact_diagnostic_text(items);

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
                                self.dart_live_diagnostics
                                    .insert(path.clone(), (stored_version, items));
                            } else {
                                let items = Arc::<[Diagnostic]>::from(std::mem::take(items));
                                self.instant_diagnostics
                                    .insert(path.clone(), (stored_version, items));
                            }

                            self.mark_diagnostics_changed();
                            self.last_change = None;
                        } else {
                            items.clear();
                        }
                    }
                }
                LspEvent::StatusChanged { server, status } => {
                    if *server == LspServerKind::Dart {
                        self.dart_status = status.clone();
                        if *status == LspServerStatus::Running {
                            self.dart_unavailable = false;
                        } else if *status == LspServerStatus::Missing {
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

        if let Some(t) = self.last_change {
            if t.elapsed().as_secs_f32() >= 3.0 {
                if self.dirty_diagnostics {
                    self.rebuild_merged_diagnostic_indices();
                    self.dirty_diagnostics = false;
                }
                self.last_change = None;
            }
        } else {
            if self.dirty_diagnostics {
                self.rebuild_merged_diagnostic_indices();
                self.dirty_diagnostics = false;
            }
        }

        self.request_ty_workspace_diagnostics_if_ready();
        self.request_ruff_workspace_diagnostics_if_ready();
        trim_allocator_after_large_diagnostics(received_diagnostics, workspace_diagnostics_done);

        all
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
        self.dart_live_diagnostics.remove(&abs_path);
        self.dart_workspace_diagnostics.remove(&abs_path);
        self.merged_diagnostic_indices.remove(&abs_path);
        self.ty_diag_result_ids.remove(&abs_path);
        self.rebuild_diag_text_pool();
        self.rebuild_merged_diagnostic_indices();
        self.dirty_diagnostics = false;
    }

    fn compact_diagnostic_text(&mut self, items: &mut [Diagnostic]) {
        for diag in items {
            diag.code = self.intern_optional_diag_text(diag.code.take());
            diag.code_href = self.intern_optional_diag_text(diag.code_href.take());
            diag.source = self.intern_optional_diag_text(diag.source.take());
        }
    }

    fn intern_optional_diag_text(&mut self, value: Option<Arc<str>>) -> Option<Arc<str>> {
        let value = value?;
        if let Some((stored, _)) = self.diag_text_pool.get_key_value(value.as_ref()) {
            return Some(stored.clone());
        }
        self.diag_text_pool.insert(value.clone(), value.clone());
        Some(value)
    }

    fn rebuild_diag_text_pool(&mut self) {
        self.diag_text_pool.clear();
        let mut values = Vec::new();
        for (_, diags) in self.instant_diagnostics.values() {
            for diag in diags.iter() {
                if let Some(value) = &diag.code {
                    values.push(value.clone());
                }
                if let Some(value) = &diag.code_href {
                    values.push(value.clone());
                }
                if let Some(value) = &diag.source {
                    values.push(value.clone());
                }
            }
        }
        for (_, diags) in self.ty_instant_diagnostics.values() {
            for diag in diags.iter() {
                if let Some(value) = &diag.code {
                    values.push(value.clone());
                }
                if let Some(value) = &diag.code_href {
                    values.push(value.clone());
                }
                if let Some(value) = &diag.source {
                    values.push(value.clone());
                }
            }
        }
        for diags in self.ruff_workspace_diagnostics.values() {
            for diag in diags.iter() {
                if let Some(value) = &diag.code {
                    values.push(value.clone());
                }
                if let Some(value) = &diag.code_href {
                    values.push(value.clone());
                }
                if let Some(value) = &diag.source {
                    values.push(value.clone());
                }
            }
        }
        for (_, diags) in self.dart_live_diagnostics.values() {
            for diag in diags.iter() {
                if let Some(value) = &diag.code {
                    values.push(value.clone());
                }
                if let Some(value) = &diag.code_href {
                    values.push(value.clone());
                }
                if let Some(value) = &diag.source {
                    values.push(value.clone());
                }
            }
        }
        for diags in self.dart_workspace_diagnostics.values() {
            for diag in diags.iter() {
                if let Some(value) = &diag.code {
                    values.push(value.clone());
                }
                if let Some(value) = &diag.code_href {
                    values.push(value.clone());
                }
                if let Some(value) = &diag.source {
                    values.push(value.clone());
                }
            }
        }
        for value in values {
            let _ = self.intern_optional_diag_text(Some(value));
        }
    }

    fn lookup_abs_path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else if let Some(ws) = self.workspaces.first() {
            ws.join(path)
        } else {
            self.relative_lookup_path(path)
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

    fn ruff_diagnostic_len_for_abs_path(&self, path: &Path) -> usize {
        if let Some((_, diagnostics)) = self.instant_diagnostics.get(path) {
            return diagnostics.len();
        }
        self.ruff_workspace_diagnostics_for_abs_path(path)
            .map_or(0, |diagnostics| diagnostics.len())
    }

    fn ruff_diagnostic_at_for_abs_path(&self, path: &Path, index: usize) -> Option<&Diagnostic> {
        if let Some((_, diagnostics)) = self.instant_diagnostics.get(path) {
            return diagnostics.get(index);
        }
        self.ruff_workspace_diagnostics_for_abs_path(path)
            .and_then(|diagnostics| diagnostics.get(index))
    }

    fn dart_workspace_diagnostics_for_abs_path(&self, path: &Path) -> Option<&Arc<[Diagnostic]>> {
        if self
            .open_dart_files
            .contains_key(&crate::platform::PathKey::new(path))
            || self.dart_live_diagnostics.contains_key(path)
        {
            return None;
        }
        self.dart_workspace_diagnostics.get(path)
    }

    fn dart_diagnostic_len_for_abs_path(&self, path: &Path) -> usize {
        if let Some((_, diagnostics)) = self.dart_live_diagnostics.get(path) {
            return diagnostics.len();
        }
        self.dart_workspace_diagnostics_for_abs_path(path)
            .map_or(0, |diagnostics| diagnostics.len())
    }

    fn dart_diagnostic_at_for_abs_path(&self, path: &Path, index: usize) -> Option<&Diagnostic> {
        if let Some((_, diagnostics)) = self.dart_live_diagnostics.get(path) {
            return diagnostics.get(index);
        }
        self.dart_workspace_diagnostics_for_abs_path(path)
            .and_then(|diagnostics| diagnostics.get(index))
    }

    fn rebuild_merged_diagnostic_indices(&mut self) {
        // Every visible diagnostics change passes through here, including
        // paths that clear the dirty flag themselves (workspace pruning).
        self.diagnostic_generation = self.diagnostic_generation.wrapping_add(1);
        let mut paths = std::collections::HashSet::new();
        for path in self.diagnostics.keys() {
            paths.insert(path.clone());
        }
        for path in self.instant_diagnostics.keys() {
            paths.insert(path.clone());
        }
        for path in self.ruff_workspace_diagnostics.keys() {
            paths.insert(path.clone());
        }
        for path in self.ty_instant_diagnostics.keys() {
            paths.insert(path.clone());
        }
        for path in self.dart_live_diagnostics.keys() {
            paths.insert(path.clone());
        }
        for path in self.dart_workspace_diagnostics.keys() {
            paths.insert(path.clone());
        }

        self.merged_diagnostic_indices.clear();
        for path in paths {
            let ruff_len = self.ruff_diagnostic_len_for_abs_path(&path);
            let ty_len = self
                .ty_instant_diagnostics
                .get(&path)
                .map_or(0, |(_, diagnostics)| diagnostics.len());
            let dart_len = self.dart_diagnostic_len_for_abs_path(&path);
            let mut indices = Vec::with_capacity(ruff_len + ty_len + dart_len);
            for index in 0..ruff_len {
                indices.push(MergedDiagnosticIndex {
                    source: DiagnosticSourceKind::Ruff,
                    index,
                });
            }
            for index in 0..ty_len {
                indices.push(MergedDiagnosticIndex {
                    source: DiagnosticSourceKind::Ty,
                    index,
                });
            }
            for index in 0..dart_len {
                indices.push(MergedDiagnosticIndex {
                    source: DiagnosticSourceKind::Dart,
                    index,
                });
            }
            if indices.is_empty()
                && let Some(diagnostics) = self.diagnostics.get(&path)
            {
                indices.reserve(diagnostics.len());
                for index in 0..diagnostics.len() {
                    indices.push(MergedDiagnosticIndex {
                        source: DiagnosticSourceKind::Legacy,
                        index,
                    });
                }
            }
            if !indices.is_empty() {
                self.merged_diagnostic_indices
                    .insert(path, Arc::from(indices.into_boxed_slice()));
            }
        }

        let mut ancestor_severities = HashMap::new();
        let mut total_counts = (0usize, 0usize);
        for (path, indices) in &self.merged_diagnostic_indices {
            let mut summary = None;
            for index in indices.iter() {
                if let Some(diagnostic) = self.diagnostic_by_index(path, *index) {
                    match diagnostic.severity {
                        DiagSeverity::Error => total_counts.0 += 1,
                        DiagSeverity::Warning => total_counts.1 += 1,
                        _ => {}
                    }
                    Self::update_severity(&mut summary, diagnostic);
                }
            }
            let Some(severity) = summary else {
                continue;
            };
            if let Some(parent) = path.parent() {
                for ancestor in parent.ancestors() {
                    let entry = ancestor_severities
                        .entry(ancestor.to_path_buf())
                        .or_insert(severity);
                    if severity == DiagSeverity::Error {
                        *entry = DiagSeverity::Error;
                    }
                }
            }
        }
        self.diagnostic_ancestor_severities = ancestor_severities;
        self.diagnostic_total_counts = total_counts;
    }

    fn diagnostic_by_index<'a>(
        &'a self,
        path: &Path,
        index: MergedDiagnosticIndex,
    ) -> Option<&'a Diagnostic> {
        match index.source {
            DiagnosticSourceKind::Legacy => self.diagnostics.get(path)?.get(index.index),
            DiagnosticSourceKind::Ruff => self.ruff_diagnostic_at_for_abs_path(path, index.index),
            DiagnosticSourceKind::Ty => self.ty_instant_diagnostics.get(path)?.1.get(index.index),
            DiagnosticSourceKind::Dart => self.dart_diagnostic_at_for_abs_path(path, index.index),
        }
    }

    fn instant_diagnostic_count_for_abs_path(&self, path: &Path) -> usize {
        self.ruff_diagnostic_len_for_abs_path(path)
            + self
                .ty_instant_diagnostics
                .get(path)
                .map_or(0, |(_, diagnostics)| diagnostics.len())
            + self.dart_diagnostic_len_for_abs_path(path)
    }

    fn instant_diagnostic_at_for_abs_path(&self, path: &Path, index: usize) -> Option<&Diagnostic> {
        let ruff_len = self.ruff_diagnostic_len_for_abs_path(path);
        if index < ruff_len {
            return self.ruff_diagnostic_at_for_abs_path(path, index);
        }
        let ty_len = self
            .ty_instant_diagnostics
            .get(path)
            .map_or(0, |(_, diagnostics)| diagnostics.len());
        if index < ruff_len + ty_len {
            return self
                .ty_instant_diagnostics
                .get(path)
                .and_then(|(_, diagnostics)| diagnostics.get(index - ruff_len));
        }
        self.dart_diagnostic_at_for_abs_path(path, index - ruff_len - ty_len)
    }

    pub fn diagnostic_at(&self, path: &Path, index: usize) -> Option<&Diagnostic> {
        let abs_path = self.lookup_abs_path(path);
        if !self.dirty_diagnostics
            && let Some(indices) = self.merged_diagnostic_indices.get(&abs_path)
        {
            return indices
                .get(index)
                .and_then(|merged| self.diagnostic_by_index(&abs_path, *merged));
        }
        if let Some(diagnostic) = self.instant_diagnostic_at_for_abs_path(&abs_path, index) {
            return Some(diagnostic);
        }
        self.diagnostics
            .get(abs_path.as_path())
            .and_then(|diagnostics| diagnostics.get(index))
    }

    pub fn diagnostic_count(&self, path: &Path) -> usize {
        let abs_path = self.lookup_abs_path(path);
        if !self.dirty_diagnostics
            && let Some(indices) = self.merged_diagnostic_indices.get(&abs_path)
        {
            return indices.len();
        }
        let instant_count = self.instant_diagnostic_count_for_abs_path(&abs_path);
        if instant_count > 0 {
            return instant_count;
        }
        self.diagnostics
            .get(abs_path.as_path())
            .map_or(0, |diagnostics| diagnostics.len())
    }

    pub fn diagnostic_entries_for_path(&self, path: &Path) -> Vec<(usize, &Diagnostic)> {
        let abs_path = self.lookup_abs_path(path);
        if !self.dirty_diagnostics
            && let Some(indices) = self.merged_diagnostic_indices.get(&abs_path)
        {
            let mut entries = Vec::with_capacity(indices.len());
            for (visible_index, merged) in indices.iter().enumerate() {
                if let Some(diagnostic) = self.diagnostic_by_index(&abs_path, *merged) {
                    entries.push((visible_index, diagnostic));
                }
            }
            return entries;
        }
        let instant_count = self.instant_diagnostic_count_for_abs_path(&abs_path);
        if instant_count > 0 {
            let mut entries = Vec::with_capacity(instant_count);
            for visible_index in 0..instant_count {
                if let Some(diagnostic) =
                    self.instant_diagnostic_at_for_abs_path(&abs_path, visible_index)
                {
                    entries.push((visible_index, diagnostic));
                }
            }
            return entries;
        }
        self.diagnostics
            .get(abs_path.as_path())
            .map(|diagnostics| diagnostics.iter().enumerate().collect())
            .unwrap_or_default()
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
        if !self.dirty_diagnostics
            && let Some(indices) = self.merged_diagnostic_indices.get(path)
        {
            for index in indices.iter() {
                if let Some(diagnostic) = self.diagnostic_by_index(path, *index)
                    && Self::update_severity(&mut summary, diagnostic)
                {
                    return summary;
                }
            }
            return summary;
        }
        if let Some((_, diagnostics)) = self.instant_diagnostics.get(path) {
            for diagnostic in diagnostics.iter() {
                if Self::update_severity(&mut summary, diagnostic) {
                    return summary;
                }
            }
        }
        if let Some(diagnostics) = self.ruff_workspace_diagnostics_for_abs_path(path) {
            for diagnostic in diagnostics.iter() {
                if Self::update_severity(&mut summary, diagnostic) {
                    return summary;
                }
            }
        }
        if let Some((_, diagnostics)) = self.ty_instant_diagnostics.get(path) {
            for diagnostic in diagnostics.iter() {
                if Self::update_severity(&mut summary, diagnostic) {
                    return summary;
                }
            }
        }
        if let Some((_, diagnostics)) = self.dart_live_diagnostics.get(path) {
            for diagnostic in diagnostics.iter() {
                if Self::update_severity(&mut summary, diagnostic) {
                    return summary;
                }
            }
        } else if let Some(diagnostics) = self.dart_workspace_diagnostics_for_abs_path(path) {
            for diagnostic in diagnostics.iter() {
                if Self::update_severity(&mut summary, diagnostic) {
                    return summary;
                }
            }
        }
        if summary.is_some() {
            return summary;
        }
        if let Some(diagnostics) = self.diagnostics.get(path) {
            for diagnostic in diagnostics.iter() {
                if Self::update_severity(&mut summary, diagnostic) {
                    return summary;
                }
            }
        }
        summary
    }

    pub fn diagnostic_paths(&self) -> Vec<&PathBuf> {
        let mut paths: Vec<&PathBuf> = Vec::new();
        for path in self
            .merged_diagnostic_indices
            .keys()
            .chain(self.diagnostics.keys())
            .chain(self.instant_diagnostics.keys())
            .chain(self.ruff_workspace_diagnostics.keys())
            .chain(self.ty_instant_diagnostics.keys())
            .chain(self.dart_live_diagnostics.keys())
            .chain(self.dart_workspace_diagnostics.keys())
        {
            if !paths
                .iter()
                .any(|existing| existing.as_path() == path.as_path())
            {
                paths.push(path);
            }
        }
        paths.sort();
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
        let abs_path = self.lookup_abs_path(path);
        self.diagnostic_severity_for_abs_path_direct(&abs_path)
    }

    pub fn diagnostic_severity_under_path(&self, path: &Path) -> Option<DiagSeverity> {
        let abs_path = self.lookup_abs_path(path);
        if !self.dirty_diagnostics {
            return self
                .diagnostic_ancestor_severities
                .get(&abs_path)
                .copied()
                .or_else(|| self.diagnostic_severity_for_abs_path_direct(&abs_path));
        }
        let mut summary = None;
        for diagnostic_path in self
            .merged_diagnostic_indices
            .keys()
            .chain(self.diagnostics.keys())
            .chain(self.instant_diagnostics.keys())
            .chain(self.ruff_workspace_diagnostics.keys())
            .chain(self.ty_instant_diagnostics.keys())
            .chain(self.dart_live_diagnostics.keys())
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
        let dart = self.dart_live_diagnostics.get(path);
        let dart_workspace = if dart.is_none() {
            self.dart_workspace_diagnostics_for_abs_path(path)
        } else {
            None
        };
        let count = ruff.map_or(0, |(_, diags)| diags.len())
            + ruff_workspace.map_or(0, |diags| diags.len())
            + ty.map_or(0, |(_, diags)| diags.len())
            + dart.map_or(0, |(_, diags)| diags.len())
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
        if let Some((version, diagnostics)) = dart {
            max_v = max_v.max(*version);
            merged.extend(diagnostics.iter());
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
                || is_stale(&self.dart_live_diagnostics)
        } else if let Some(ws) = self.workspaces.first() {
            let abs_path = ws.join(path);
            let is_stale = |diags: &HashMap<PathBuf, (i32, Arc<[Diagnostic]>)>| {
                diags
                    .get(abs_path.as_path())
                    .is_some_and(|(version, _)| (*version as u64) < editor_version)
            };
            is_stale(&self.instant_diagnostics)
                || is_stale(&self.ty_instant_diagnostics)
                || is_stale(&self.dart_live_diagnostics)
        } else {
            let abs_path = self.relative_lookup_path(path);
            let is_stale = |diags: &HashMap<PathBuf, (i32, Arc<[Diagnostic]>)>| {
                diags
                    .get(abs_path.as_path())
                    .is_some_and(|(version, _)| (*version as u64) < editor_version)
            };
            is_stale(&self.instant_diagnostics)
                || is_stale(&self.ty_instant_diagnostics)
                || is_stale(&self.dart_live_diagnostics)
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
