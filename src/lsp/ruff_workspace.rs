use super::{DiagSeverity, Diagnostic};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;

pub(super) struct RuffWorkspaceResult {
    pub(super) workspaces: Vec<PathBuf>,
    pub(super) diagnostics: HashMap<PathBuf, Vec<Diagnostic>>,
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
        let Some(rx) = self.ruff_workspace_diag_rx.take() else {
            return 0;
        };

        match rx.try_recv() {
            Ok(mut result) => {
                self.ruff_workspace_diag_pending = false;
                self.apply_ruff_workspace_diagnostics(&mut result)
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.ruff_workspace_diag_rx = Some(rx);
                0
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.ruff_workspace_diag_pending = false;
                0
            }
        }
    }

    fn apply_ruff_workspace_diagnostics(&mut self, result: &mut RuffWorkspaceResult) -> usize {
        for workspace in &result.workspaces {
            self.ruff_workspace_diagnostics
                .retain(|path, _| !path.starts_with(workspace));
            self.merged_diagnostic_indices
                .retain(|path, _| !path.starts_with(workspace));
        }

        let mut received = 0usize;
        let active_workspaces = self.active_workspaces.clone();
        for (path, items) in result.diagnostics.drain() {
            if !active_workspaces
                .iter()
                .any(|workspace| path.starts_with(workspace))
            {
                continue;
            }
            received = received.saturating_add(items.len());
            if items.is_empty() {
                continue;
            }
            let mut items = items;
            self.compact_diagnostic_text(&mut items);
            self.ruff_workspace_diagnostics
                .insert(path, Arc::from(items.into_boxed_slice()));
        }

        self.rebuild_diag_text_pool();
        self.dirty_diagnostics = true;
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
        let (tx, rx) = std::sync::mpsc::channel();
        let spawn_result = std::thread::Builder::new()
            .name("rriter-ruff-workspace".into())
            .spawn(move || {
                let result = collect_workspace_diagnostics(workspaces);
                let _ = tx.send(result);
            });

        if spawn_result.is_ok() {
            self.ruff_workspace_diag_rx = Some(rx);
            self.ruff_workspace_diag_pending = true;
            self.ruff_workspace_diag_dirty = false;
        } else {
            self.ruff_workspace_diag_dirty = false;
        }
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn run_ruff_workspace_check(workspaces: &[PathBuf]) -> HashMap<PathBuf, Vec<Diagnostic>> {
    if workspaces.is_empty() {
        return HashMap::new();
    }

    let Some(output) = run_ruff_check_command(workspaces) else {
        return HashMap::new();
    };

    if output.stdout.is_empty() && !output.status.success() {
        return HashMap::new();
    }

    parse_ruff_check_json(&output.stdout, workspaces).unwrap_or_default()
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn run_ruff_check_command(workspaces: &[PathBuf]) -> Option<Output> {
    let mut cmd = Command::new("ruff");
    cmd.arg("check")
        .arg("--output-format=json")
        .arg("--force-exclude");
    for workspace in workspaces {
        cmd.arg(workspace);
    }
    cmd.output().ok()
}

pub(super) fn parse_ruff_check_json(
    raw: &[u8],
    workspaces: &[PathBuf],
) -> Result<HashMap<PathBuf, Vec<Diagnostic>>, serde_json::Error> {
    let raw = json_array_payload(raw);
    if raw.is_empty() {
        return Ok(HashMap::new());
    }

    let items = serde_json::from_slice::<Vec<RuffCheckDiagnostic>>(raw)?;
    let mut out: HashMap<PathBuf, Vec<Diagnostic>> = HashMap::new();
    for item in items {
        if item.filename.is_empty() {
            continue;
        }
        let path = resolve_ruff_filename(&item.filename, workspaces);
        let diagnostic = diagnostic_from_ruff(item);
        out.entry(path).or_default().push(diagnostic);
    }

    for diagnostics in out.values_mut() {
        diagnostics.sort_by(|a, b| {
            a.start_line
                .cmp(&b.start_line)
                .then(a.start_col.cmp(&b.start_col))
                .then_with(|| a.code.as_deref().cmp(&b.code.as_deref()))
                .then_with(|| a.message.as_ref().cmp(b.message.as_ref()))
        });
    }
    Ok(out)
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

fn diagnostic_from_ruff(item: RuffCheckDiagnostic) -> Diagnostic {
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
        code: item.code.map(Arc::<str>::from),
        code_href: item.url.map(Arc::<str>::from),
        message: Arc::<str>::from(item.message),
        source: Some(Arc::<str>::from("ruff")),
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
struct RuffCheckDiagnostic {
    filename: String,
    location: RuffLocation,
    #[serde(default)]
    end_location: Option<RuffLocation>,
    #[serde(default)]
    code: Option<String>,
    message: String,
    #[serde(default)]
    url: Option<String>,
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

    fn parse(raw: &str) -> HashMap<PathBuf, Vec<Diagnostic>> {
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
}
