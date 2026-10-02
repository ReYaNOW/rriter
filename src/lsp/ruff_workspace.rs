use super::{DiagSeverity, Diagnostic};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::sync::Arc;
use std::time::Duration;
use crate::ui_waker::OneShotState;

pub(super) struct RuffWorkspaceResult {
    pub(super) workspaces: Vec<PathBuf>,
    pub(super) diagnostics: Result<HashMap<PathBuf, Arc<[Diagnostic]>>, String>,
}

pub(super) fn collect_workspace_diagnostics(workspaces: Vec<PathBuf>) -> RuffWorkspaceResult {
    let diagnostics = run_ruff_workspace_check(&workspaces);
    RuffWorkspaceResult {
        workspaces,
        diagnostics,
    }
}

impl super::LspManager {
    pub(super) fn poll_ruff_workspace_diagnostics(&mut self) -> usize {
        let Some(mut rx) = self.ruff_workspace_diag_rx.take() else {
            return 0;
        };

        match rx.poll() {
            OneShotState::Ready(mut result) => {
                self.ruff_workspace_diag_pending = false;
                match result.diagnostics.as_mut() {
                    Ok(diagnostics) => {
                        self.apply_ruff_workspace_diagnostics(&result.workspaces, diagnostics)
                    }
                    Err(error) => {
                        self.server_logs
                            .entry("ruff")
                            .or_default()
                            .push(super::LogEntry {
                                text: format!("[LSP] Ruff workspace diagnostics failed: {error}"),
                                spans: Vec::new(),
                                folds: Vec::new(),
                                created_at: std::time::Instant::now(),
                            });
                        0
                    }
                }
            }
            OneShotState::Pending => {
                self.ruff_workspace_diag_rx = Some(rx);
                0
            }
            OneShotState::Closed => {
                self.ruff_workspace_diag_pending = false;
                self.ruff_workspace_diagnostics.clear();
                self.mark_diagnostics_changed();
                self.python_status = super::LspServerStatus::Crashed;
                self.server_logs
                    .entry("ruff")
                    .or_default()
                    .push(super::LogEntry {
                        text: "[LSP] Ruff workspace diagnostics worker disconnected".to_string(),
                        spans: Vec::new(),
                        folds: Vec::new(),
                        created_at: std::time::Instant::now(),
                    });
                0
            }
        }
    }

    fn apply_ruff_workspace_diagnostics(
        &mut self,
        workspaces: &[PathBuf],
        diagnostics: &mut HashMap<PathBuf, Arc<[Diagnostic]>>,
    ) -> usize {
        for workspace in workspaces {
            self.ruff_workspace_diagnostics
                .retain(|path, _| !crate::platform::path_is_within(path, workspace));
        }

        let mut received = 0usize;
        let active_workspaces = self.active_workspaces.clone();
        for (path, items) in diagnostics.drain() {
            if !active_workspaces
                .iter()
                .any(|workspace| crate::platform::path_is_within(&path, workspace))
            {
                continue;
            }
            received = received.saturating_add(items.len());
            if items.is_empty() {
                continue;
            }
            let mut items = items;
            // The worker built each file's slice fresh, so it is still uniquely owned here.
            if let Some(items) = Arc::get_mut(&mut items) {
                // `parse_ruff_check_json` already shared the run's repeated messages.
                self.compact_diagnostic_text(items, None);
            }
            self.ruff_workspace_diagnostics.insert(path, items);
        }
        self.ruff_workspace_diagnostics.shrink_to_fit();

        self.prune_diag_text_pool();
        self.mark_diagnostics_changed();
        received
    }

    pub(super) fn request_ruff_workspace_diagnostics_if_ready(&mut self) {
        if !self.ruff_workspace_diag_dirty
            || self.ruff_workspace_diag_pending
            || self.python_disabled
            || self.python_status != super::LspServerStatus::Running
            || self.suppress_diagnostics
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

        let workspaces = self.active_workspaces.clone();
        match self.ui_waker.spawn_one_shot("rriter-ruff-workspace", move || {
            collect_workspace_diagnostics(workspaces)
        }) {
            Ok(job) => {
                self.ruff_workspace_diag_rx = Some(job);
                self.ruff_workspace_diag_pending = true;
                self.ruff_workspace_diag_dirty = false;
            }
            Err(error) => {
                self.ruff_workspace_diag_dirty = false;
                self.python_status = super::LspServerStatus::Crashed;
                self.server_logs
                    .entry("ruff")
                    .or_default()
                    .push(super::LogEntry {
                        text: format!("[LSP] Ruff workspace diagnostics worker failed to start: {error}"),
                        spans: Vec::new(),
                        folds: Vec::new(),
                        created_at: std::time::Instant::now(),
                    });
            }
        }
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn run_ruff_workspace_check(
    workspaces: &[PathBuf],
) -> Result<HashMap<PathBuf, Arc<[Diagnostic]>>, String> {
    if workspaces.is_empty() {
        return Ok(HashMap::new());
    }

    let output = run_ruff_check_command(workspaces)?;

    if output.stdout.is_empty() && !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "ruff exited with {}{}",
            output.status,
            (!stderr.trim().is_empty())
                .then(|| format!(": {}", stderr.trim()))
                .unwrap_or_default()
        ));
    }

    parse_ruff_check_json(&output.stdout, workspaces)
        .map_err(|error| format!("invalid ruff JSON: {error}"))
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn run_ruff_check_command(workspaces: &[PathBuf]) -> Result<Output, String> {
    let mut cmd = crate::platform::command_for_tool("ruff".as_ref(), "RRITER_RUFF_PATH")
        .map_err(|error| error.to_string())?;
    cmd.arg("check")
        .arg("--output-format=json")
        .arg("--force-exclude");
    for workspace in workspaces {
        cmd.arg(workspace);
    }
    crate::platform::run_command_output(&mut cmd, Duration::from_secs(120))
        .map_err(|error| error.to_string())
}

pub(super) fn parse_ruff_check_json(
    raw: &[u8],
    workspaces: &[PathBuf],
) -> Result<HashMap<PathBuf, Arc<[Diagnostic]>>, serde_json::Error> {
    let raw = json_array_payload(raw);
    if raw.is_empty() {
        return Ok(HashMap::new());
    }

    let mut deserializer = serde_json::Deserializer::from_slice(raw);
    let out = serde::Deserializer::deserialize_seq(
        &mut deserializer,
        RuffCheckVisitor { workspaces },
    )?;
    deserializer.end()?;
    Ok(out)
}

/// Streams the `ruff check --output-format=json` array straight into per-file diagnostics:
/// items borrow from the raw output and are converted one by one (no owned intermediate
/// `Vec`), a file's path is resolved once per run of its items (ruff groups its output by
/// file), and repeated texts (rule code, URL, message, source) share one allocation.
struct RuffCheckVisitor<'w> {
    workspaces: &'w [PathBuf],
}

impl<'de> serde::de::Visitor<'de> for RuffCheckVisitor<'_> {
    type Value = HashMap<PathBuf, Arc<[Diagnostic]>>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("an array of ruff diagnostics")
    }

    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut grouped: HashMap<PathBuf, Vec<Diagnostic>> = HashMap::new();
        let mut texts = RuffTextPool::default();
        let mut current_name = String::new();
        let mut current_path = PathBuf::new();
        let mut current: Vec<Diagnostic> = Vec::new();
        while let Some(item) = seq.next_element::<RuffCheckDiagnostic<'de>>()? {
            if item.filename.is_empty() {
                continue;
            }
            if item.filename != current_name.as_str() {
                flush_ruff_file(&mut grouped, &mut current_path, &mut current);
                current_name.clear();
                current_name.push_str(&item.filename);
                current_path = resolve_ruff_filename(&item.filename, self.workspaces);
            }
            current.push(diagnostic_from_ruff(item, &mut texts));
        }
        flush_ruff_file(&mut grouped, &mut current_path, &mut current);

        Ok(grouped
            .into_iter()
            .map(|(path, mut diagnostics)| {
                diagnostics.sort_by(|a, b| {
                    a.start_line
                        .cmp(&b.start_line)
                        .then(a.start_col.cmp(&b.start_col))
                        .then_with(|| a.code.as_deref().cmp(&b.code.as_deref()))
                        .then_with(|| a.message.as_ref().cmp(b.message.as_ref()))
                });
                (path, Arc::from(diagnostics))
            })
            .collect())
    }
}

fn flush_ruff_file(
    grouped: &mut HashMap<PathBuf, Vec<Diagnostic>>,
    path: &mut PathBuf,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if diagnostics.is_empty() {
        return;
    }
    let path = std::mem::take(path);
    match grouped.entry(path) {
        std::collections::hash_map::Entry::Occupied(mut entry) => entry.get_mut().append(diagnostics),
        std::collections::hash_map::Entry::Vacant(entry) => {
            entry.insert(std::mem::take(diagnostics));
        }
    }
}

/// Shared copies of the texts one `ruff check` run repeats across its diagnostics.
struct RuffTextPool {
    source: Arc<str>,
    texts: std::collections::HashSet<Arc<str>>,
}

impl Default for RuffTextPool {
    fn default() -> Self {
        Self {
            source: Arc::from("ruff"),
            texts: std::collections::HashSet::new(),
        }
    }
}

impl RuffTextPool {
    fn intern(&mut self, text: &str) -> Arc<str> {
        if let Some(shared) = self.texts.get(text) {
            return shared.clone();
        }
        let shared = Arc::<str>::from(text);
        self.texts.insert(shared.clone());
        shared
    }
}

fn json_array_payload(raw: &[u8]) -> &[u8] {
    let trimmed = trim_ascii_ws(raw);
    if trimmed.is_empty() || trimmed.first() == Some(&b'[') {
        return trimmed;
    }

    let Some(start) = trimmed.iter().position(|byte| *byte == b'[') else {
        return trimmed;
    };
    let Some(end) = trimmed.iter().rposition(|byte| *byte == b']') else {
        return trimmed;
    };
    if start > end {
        return trimmed;
    }
    &trimmed[start..=end]
}

fn trim_ascii_ws(raw: &[u8]) -> &[u8] {
    let mut start = 0usize;
    let mut end = raw.len();
    while start < end && raw[start].is_ascii_whitespace() {
        start += 1;
    }
    while end > start && raw[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    &raw[start..end]
}

fn resolve_ruff_filename(filename: &str, workspaces: &[PathBuf]) -> PathBuf {
    let path = PathBuf::from(filename);
    if path.is_absolute() {
        return path;
    }

    for workspace in workspaces {
        let candidate = workspace.join(&path);
        if candidate.exists() {
            return candidate;
        }
    }

    workspaces
        .first()
        .map_or(path.clone(), |workspace| workspace.join(path))
}

fn diagnostic_from_ruff(item: RuffCheckDiagnostic<'_>, texts: &mut RuffTextPool) -> Diagnostic {
    let (start_line, start_col) = lsp_position(&item.location);
    let (mut end_line, mut end_col) = item
        .end_location
        .as_ref()
        .map(lsp_position)
        .unwrap_or((start_line, start_col.saturating_add(1)));

    if end_line < start_line {
        end_line = start_line;
        end_col = start_col.saturating_add(1);
    } else if end_line == start_line && end_col <= start_col {
        end_col = start_col.saturating_add(1);
    }

    Diagnostic {
        start_line,
        start_col,
        end_line,
        end_col,
        severity: severity_for_ruff_code(item.code.as_deref()),
        code: item.code.as_deref().map(|code| texts.intern(code)),
        code_href: item.url.as_deref().map(|url| texts.intern(url)),
        message: texts.intern(&item.message),
        source: Some(texts.source.clone()),
        quickfixes: Vec::new().into_boxed_slice(),
        tags: Vec::new().into_boxed_slice(),
    }
}

fn lsp_position(location: &RuffLocation) -> (u32, u32) {
    (
        location.row.saturating_sub(1),
        location.column.saturating_sub(1),
    )
}

fn severity_for_ruff_code(code: Option<&str>) -> DiagSeverity {
    let Some(code) = code else {
        return DiagSeverity::Warning;
    };

    if code.starts_with("E9")
        || code == "F821"
        || code == "F822"
        || code == "F823"
        || code.contains("syntax")
    {
        DiagSeverity::Error
    } else {
        DiagSeverity::Warning
    }
}

#[derive(serde::Deserialize)]
struct RuffCheckDiagnostic<'a> {
    #[serde(borrow)]
    filename: std::borrow::Cow<'a, str>,
    location: RuffLocation,
    #[serde(default)]
    end_location: Option<RuffLocation>,
    #[serde(default, borrow)]
    code: Option<std::borrow::Cow<'a, str>>,
    #[serde(borrow)]
    message: std::borrow::Cow<'a, str>,
    #[serde(default, borrow)]
    url: Option<std::borrow::Cow<'a, str>>,
}

#[derive(serde::Deserialize)]
struct RuffLocation {
    row: u32,
    column: u32,
}

#[allow(dead_code)]
fn is_under_workspace(path: &Path, workspaces: &[PathBuf]) -> bool {
    workspaces
        .iter()
        .any(|workspace| path.starts_with(workspace))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> PathBuf {
        PathBuf::from("/tmp/rriter-ruff-ws")
    }

    fn parse(raw: &str) -> HashMap<PathBuf, Arc<[Diagnostic]>> {
        parse_ruff_check_json(raw.as_bytes(), &[ws()]).unwrap()
    }

    #[test]
    fn parse_ruff_json_groups_by_file_and_normalizes_positions() {
        let diagnostics = parse(
            r#"
            [
              {
                "cell": null,
                "code": "F401",
                "end_location": {"row": 3, "column": 16},
                "filename": "pkg/main.py",
                "fix": null,
                "location": {"row": 3, "column": 8},
                "message": "`typing.Any` imported but unused",
                "noqa_row": 3,
                "url": "https://docs.astral.sh/ruff/rules/unused-import"
              },
              {
                "cell": null,
                "code": "F821",
                "end_location": {"row": 8, "column": 12},
                "filename": "pkg/main.py",
                "fix": null,
                "location": {"row": 8, "column": 5},
                "message": "Undefined name `missing`",
                "noqa_row": 8,
                "url": "https://docs.astral.sh/ruff/rules/undefined-name"
              }
            ]
            "#,
        );

        let path = ws().join("pkg/main.py");
        let items = diagnostics.get(&path).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].start_line, 2);
        assert_eq!(items[0].start_col, 7);
        assert_eq!(items[0].end_line, 2);
        assert_eq!(items[0].end_col, 15);
        assert_eq!(items[0].severity, DiagSeverity::Warning);
        assert_eq!(items[0].code.as_deref(), Some("F401"));
        assert_eq!(items[0].source.as_deref(), Some("ruff"));
        assert!(items[0].quickfixes.is_empty());
        assert!(items[0].tags.is_empty());
        assert!(
            items[0]
                .code_href
                .as_deref()
                .unwrap()
                .contains("unused-import")
        );

        assert_eq!(items[1].start_line, 7);
        assert_eq!(items[1].start_col, 4);
        assert_eq!(items[1].severity, DiagSeverity::Error);
        assert_eq!(items[1].code.as_deref(), Some("F821"));
    }

    #[test]
    fn parse_ruff_json_keeps_absolute_paths_and_repairs_empty_ranges() {
        let raw = r#"
        [
          {
            "code": "E999",
            "end_location": {"row": 4, "column": 1},
            "filename": "/tmp/other/app.py",
            "location": {"row": 4, "column": 1},
            "message": "SyntaxError: invalid syntax",
            "url": null
          },
          {
            "code": null,
            "filename": "/tmp/other/app.py",
            "location": {"row": 9, "column": 20},
            "message": "fallback warning"
          }
        ]
        "#;

        let diagnostics = parse(raw);
        let items = diagnostics
            .get(&PathBuf::from("/tmp/other/app.py"))
            .unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].severity, DiagSeverity::Error);
        assert_eq!(items[0].start_line, 3);
        assert_eq!(items[0].start_col, 0);
        assert_eq!(items[0].end_line, 3);
        assert_eq!(items[0].end_col, 1);
        assert_eq!(items[1].severity, DiagSeverity::Warning);
        assert_eq!(items[1].start_line, 8);
        assert_eq!(items[1].start_col, 19);
        assert_eq!(items[1].end_col, 20);
    }

    #[test]
    fn parse_ruff_json_sorts_diagnostics_by_range_then_code() {
        let diagnostics = parse(
            r#"
            [
              {
                "code": "W2",
                "filename": "a.py",
                "location": {"row": 5, "column": 5},
                "message": "second"
              },
              {
                "code": "W1",
                "filename": "a.py",
                "location": {"row": 5, "column": 5},
                "message": "first"
              },
              {
                "code": "W3",
                "filename": "a.py",
                "location": {"row": 1, "column": 1},
                "message": "top"
              }
            ]
            "#,
        );

        let items = diagnostics.get(&ws().join("a.py")).unwrap();
        assert_eq!(items[0].message.as_ref(), "top");
        assert_eq!(items[1].message.as_ref(), "first");
        assert_eq!(items[2].message.as_ref(), "second");
    }

    #[test]
    fn json_array_payload_accepts_wrapped_stdout_and_empty_output() {
        let wrapped = b"ruff header\n[{\"filename\":\"a.py\",\"location\":{\"row\":1,\"column\":1},\"message\":\"m\"}]\n";
        let parsed = parse_ruff_check_json(wrapped, &[ws()]).unwrap();
        assert_eq!(parsed.get(&ws().join("a.py")).unwrap().len(), 1);

        let empty = parse_ruff_check_json(b" \n\t ", &[ws()]).unwrap();
        assert!(empty.is_empty());
    }

    #[test]
    fn severity_for_ruff_code_marks_only_runtime_breaking_codes_as_errors() {
        assert_eq!(severity_for_ruff_code(Some("F401")), DiagSeverity::Warning);
        assert_eq!(severity_for_ruff_code(Some("E501")), DiagSeverity::Warning);
        assert_eq!(severity_for_ruff_code(Some("E999")), DiagSeverity::Error);
        assert_eq!(severity_for_ruff_code(Some("F821")), DiagSeverity::Error);
        assert_eq!(
            severity_for_ruff_code(Some("invalid-syntax")),
            DiagSeverity::Error
        );
        assert_eq!(severity_for_ruff_code(None), DiagSeverity::Warning);
    }

    #[test]
    fn workspace_membership_helper_is_prefix_based() {
        let roots = [PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")];
        assert!(is_under_workspace(Path::new("/tmp/a/pkg/main.py"), &roots));
        assert!(is_under_workspace(Path::new("/tmp/b/app.py"), &roots));
        assert!(!is_under_workspace(Path::new("/tmp/c/app.py"), &roots));
    }

    #[test]
    fn malformed_ruff_json_is_an_error_instead_of_an_empty_success() {
        let error = parse_ruff_check_json(b"not json", &[ws()]).unwrap_err();
        assert!(error.to_string().contains("expected"));
    }
}
