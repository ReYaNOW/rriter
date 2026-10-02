#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProblemRow {
    pub path: std::sync::Arc<std::path::Path>,
    pub index: usize,
    source: Option<(std::sync::Arc<[crate::lsp::Diagnostic]>, u32)>,
}

impl ProblemRow {
    pub fn with_diagnostic_source(
        path: std::sync::Arc<std::path::Path>,
        index: usize,
        source: std::sync::Arc<[crate::lsp::Diagnostic]>,
        local_index: u32,
    ) -> Self {
        Self {
            path,
            index,
            source: Some((source, local_index)),
        }
    }

    pub fn group_header(path: std::sync::Arc<std::path::Path>) -> Self {
        Self {
            path,
            index: usize::MAX,
            source: None,
        }
    }

    pub fn diagnostic(&self) -> Option<&crate::lsp::Diagnostic> {
        self.source
            .as_ref()
            .and_then(|(slice, index)| slice.get(*index as usize))
    }

    pub fn is_group_header(&self) -> bool {
        self.index == usize::MAX
    }
}

impl IdePanelState {
    pub(crate) fn refresh_flat_diagnostics_if_needed(
        &mut self,
        active_tab: usize,
        active_file: Option<&std::path::Path>,
        query_problem: Option<(&str, &[crate::lsp::Diagnostic])>,
        lsp: Option<&crate::lsp::LspManager>,
    ) -> bool {
        // The rows serve only the open panel; a workspace check that streams thousands of
        // diagnostics must not rebuild them per update while nobody looks.
        if !self.is_open(PanelId::Problems) {
            return false;
        }
        let lsp_generation = lsp.map_or(0, crate::lsp::LspManager::diagnostic_generation);
        let query_changed = match query_problem {
            Some((database_name, diagnostics)) => {
                self.flat_diags_cache_source_name.as_deref() != Some(database_name)
                    || self.query_problem_diagnostics.as_ref() != diagnostics
            }
            None => self.flat_diags_cache_source_name.is_some(),
        };
        let active_file_changed = self.flat_diags_cache_file.as_deref() != active_file;
        if self.flat_diags_cache_tab == Some(active_tab)
            && !active_file_changed
            && !query_changed
            && self.flat_diags_cache_problems_tab == self.problems_tab
            && self.flat_diags_cache_collapsed == self.problems_collapsed
            && self.flat_diags_cache_lsp_generation == lsp_generation
            && self.flat_diags_cache_has_lsp == lsp.is_some()
        {
            return false;
        }

        self.flat_diags.clear();
        self.problem_groups.clear();
        self.flat_diags_cache_tab = Some(active_tab);
        self.flat_diags_cache_file = active_file.map(std::path::Path::to_path_buf);
        self.flat_diags_cache_problems_tab = self.problems_tab;
        self.flat_diags_cache_collapsed.clone_from(&self.problems_collapsed);
        self.flat_diags_cache_lsp_generation = lsp_generation;
        self.flat_diags_cache_has_lsp = lsp.is_some();
        if let Some((database_name, diagnostics)) = query_problem {
            let path = std::path::PathBuf::from(format!("SQL-консоль · {database_name}"));
            let shared = std::sync::Arc::<std::path::Path>::from(path.as_path());
            self.flat_diags_cache_source_name = Some(database_name.to_owned());
            self.query_problem_path = Some(path);
            self.query_problem_diagnostics = std::sync::Arc::from(diagnostics);
            if self.problems_tab == 1 {
                self.push_problem_group(shared.clone(), problem_severity_counts(diagnostics));
            }
            if self.problems_tab == 0 || !self.problems_collapsed.contains(shared.as_ref()) {
                self.flat_diags
                    .extend(diagnostics.iter().enumerate().map(|(index, _)| {
                        ProblemRow::with_diagnostic_source(
                            shared.clone(),
                            index,
                            std::sync::Arc::clone(&self.query_problem_diagnostics),
                            index as u32,
                        )
                    }));
            }
        } else {
            self.flat_diags_cache_source_name = None;
            self.query_problem_path = None;
            self.query_problem_diagnostics = std::sync::Arc::from([]);
        }

        if let Some(lsp) = lsp {
            if self.problems_tab == 0 {
                if self.query_problem_path.is_none()
                    && let Some(path) = active_file
                {
                    let mut diagnostics = lsp.diagnostic_entries_with_slices_for_path(path);
                    sort_problem_entries(&mut diagnostics);
                    let shared = std::sync::Arc::<std::path::Path>::from(path);
                    self.flat_diags.extend(diagnostics.into_iter().map(
                        |(index, slice, local_index)| {
                            ProblemRow::with_diagnostic_source(
                                shared.clone(),
                                index,
                                std::sync::Arc::clone(slice),
                                local_index,
                            )
                        },
                    ));
                }
            } else {
                for path in lsp.diagnostic_paths() {
                    let mut diagnostics = lsp.diagnostic_entries_with_slices_for_path(path);
                    if diagnostics.is_empty() {
                        continue;
                    }
                    sort_problem_entries(&mut diagnostics);
                    let shared = std::sync::Arc::<std::path::Path>::from(path.as_path());
                    let counts = problem_severity_counts(diagnostics.iter().filter_map(
                        |(_, slice, local_index)| slice.get(*local_index as usize),
                    ));
                    self.push_problem_group(shared.clone(), counts);
                    if !self.problems_collapsed.contains(path) {
                        self.flat_diags.extend(diagnostics.into_iter().map(
                            |(index, slice, local_index)| {
                                ProblemRow::with_diagnostic_source(
                                    shared.clone(),
                                    index,
                                    std::sync::Arc::clone(slice),
                                    local_index,
                                )
                            },
                        ));
                    }
                }
            }
        }
        true
    }

    fn push_problem_group(
        &mut self,
        path: std::sync::Arc<std::path::Path>,
        counts: (usize, usize),
    ) {
        self.problem_groups.push(ProblemGroupHeader {
            row: self.flat_diags.len(),
            counts,
            name: problem_group_name(&path),
        });
        self.flat_diags.push(ProblemRow::group_header(path));
    }

    fn problem_group_at(&self, row: usize) -> Option<&ProblemGroupHeader> {
        self.problem_groups
            .binary_search_by_key(&row, |group| group.row)
            .ok()
            .map(|found| &self.problem_groups[found])
    }

    /// `(errors, warnings)` of the file group header at Problems row `row`, cached when the
    /// rows are built so drawing a header does not walk the file's diagnostics.
    pub fn problem_group_counts_at(&self, row: usize) -> (usize, usize) {
        self.problem_group_at(row).map_or((0, 0), |group| group.counts)
    }

    /// Display name of the file group header at Problems row `row`, cached when the rows
    /// are built so drawing a header allocates nothing.
    pub fn problem_group_name_at(&self, row: usize) -> &str {
        self.problem_group_at(row).map_or("", |group| &group.name)
    }

    pub fn problem_diagnostic<'a>(
        &'a self,
        lsp: Option<&'a crate::lsp::LspManager>,
        path: &std::path::Path,
        index: usize,
    ) -> Option<&'a crate::lsp::Diagnostic> {
        if self.query_problem_path.as_deref() == Some(path) {
            self.query_problem_diagnostics.get(index)
        } else {
            lsp.and_then(|manager| manager.diagnostic_at(path, index))
        }
    }

    /// Rows of the Problems list. O(1): `refresh_flat_diagnostics_if_needed` keeps only
    /// renderable rows, so the wheel handler and every frame need no walk over them.
    pub fn visible_problem_row_count(&self) -> usize {
        self.flat_diags.len()
    }

    pub fn problem_row_visible(
        &self,
        lsp: Option<&crate::lsp::LspManager>,
        path: &std::path::Path,
        index: usize,
    ) -> bool {
        index == usize::MAX || self.problem_diagnostic(lsp, path, index).is_some()
    }

    pub fn problem_counts(
        &self,
        lsp: Option<&crate::lsp::LspManager>,
        path: &std::path::Path,
    ) -> (usize, usize) {
        if self.query_problem_path.as_deref() == Some(path) {
            problem_severity_counts(self.query_problem_diagnostics.iter())
        } else {
            lsp.map_or((0, 0), |manager| manager.diagnostic_counts_for_path(path))
        }
    }

    pub fn is_query_problem_path(&self, path: &std::path::Path) -> bool {
        self.query_problem_path.as_deref() == Some(path)
    }
}
