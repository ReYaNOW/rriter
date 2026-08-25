use std::time::Duration;

use crate::app::{App, DartClosingLabelsMode, DartSettings};
use crate::languages::dart::{
    ClosingHintMode, ClosingHintSettings, ClosingHintSource, ClosingHintState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DartAutomationStep {
    Setup,
    WaitClosingHints { minimum_count: usize },
}

impl DartAutomationStep {
    pub(super) fn name(self) -> String {
        match self {
            Self::Setup => "dart-setup-pgo".to_string(),
            Self::WaitClosingHints { minimum_count } => {
                format!("dart-wait-closing-hints:{minimum_count}")
            }
        }
    }

    pub(super) fn timeout(self) -> Duration {
        match self {
            Self::WaitClosingHints { .. } => Duration::from_secs(90),
            Self::Setup => Duration::from_secs(12),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DartStepResult {
    Pending,
    Done,
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DartHintStatus {
    extension_is_dart: bool,
    editor_revision: u64,
    highlight_complete: bool,
    syntax_tree_available: bool,
    hint_revision: u64,
    hint_count: usize,
    syntax_hint_count: usize,
    settings: ClosingHintSettings,
}

impl DartHintStatus {
    fn ready(self, minimum_count: usize) -> bool {
        self.extension_is_dart
            && self.highlight_complete
            && self.syntax_tree_available
            && self.hint_revision == self.editor_revision
            && self.settings.mode == ClosingHintMode::DartServerAndBlocks
            && self.syntax_hint_count >= minimum_count
    }

    fn configuration_error(self) -> bool {
        !self.extension_is_dart || self.settings.mode != ClosingHintMode::DartServerAndBlocks
    }

    fn diagnostics(self, minimum_count: usize) -> String {
        format!(
            "dart closing hints: extension_is_dart={} editor_revision={} highlight_complete={} syntax_tree_available={} hint_revision={} hint_count={} syntax_hint_count={} required={} mode={:?} minimum_nesting_depth={} minimum_block_lines={}",
            self.extension_is_dart,
            self.editor_revision,
            self.highlight_complete,
            self.syntax_tree_available,
            self.hint_revision,
            self.hint_count,
            self.syntax_hint_count,
            minimum_count,
            self.settings.mode,
            self.settings.minimum_nesting_depth,
            self.settings.minimum_block_lines,
        )
    }
}

fn pgo_dart_settings() -> DartSettings {
    DartSettings {
        enabled: false,
        workspace_analysis: false,
        closing_labels: DartClosingLabelsMode::DartServerAndBlocks,
        ..DartSettings::default()
    }
}

fn wait_status(
    file_extension: &str,
    editor_revision: u64,
    highlight_complete: bool,
    syntax_tree_available: bool,
    state: &ClosingHintState,
    settings: ClosingHintSettings,
) -> DartHintStatus {
    let syntax_hint_count = state
        .hints()
        .iter()
        .filter(|hint| hint.source == ClosingHintSource::SyntaxTree)
        .count();
    DartHintStatus {
        extension_is_dart: file_extension == "dart",
        editor_revision,
        highlight_complete,
        syntax_tree_available,
        hint_revision: state.revision(),
        hint_count: state.hints().len(),
        syntax_hint_count,
        settings,
    }
}

fn status_for_app(app: &App) -> DartHintStatus {
    let revision = app.editor.version;
    wait_status(
        &app.file_extension,
        revision,
        app.is_highlight_complete || app.highlighter.is_complete,
        app.highlighter.syntax_tree_for(revision, "dart").is_some(),
        &app.closing_hint_state,
        app.closing_hint_settings,
    )
}

pub(super) fn diagnostics(app: &App, minimum_count: usize) -> String {
    status_for_app(app).diagnostics(minimum_count)
}

pub(super) fn run(app: &mut App, step: DartAutomationStep) -> DartStepResult {
    match step {
        DartAutomationStep::Setup => {
            app.dart_settings = pgo_dart_settings();
            if let Some(lsp) = &mut app.lsp {
                lsp.set_dart_workspace_analysis_enabled(false);
                lsp.set_server_enabled("dart", false);
                app.ide_panel.lsp_servers = lsp.servers_info();
            }
            app.sync_dart_closing_hint_settings();
            DartStepResult::Done
        }
        DartAutomationStep::WaitClosingHints { minimum_count } => {
            let status = status_for_app(app);
            if status.configuration_error() {
                DartStepResult::Failed(status.diagnostics(minimum_count))
            } else if status.ready(minimum_count) {
                DartStepResult::Done
            } else {
                // Production highlighter completion owns syntax-hint refresh. Keep this wait
                // observational so PGO proves the normal tree-sitter -> closing-hint path ran.
                DartStepResult::Pending
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::languages::dart::ClosingHint;

    fn syntax_hint(revision: u64, line: usize) -> ClosingHint {
        ClosingHint {
            revision,
            line,
            anchor_byte: line * 10,
            label: Arc::from("PgoNode.compute"),
            source: ClosingHintSource::SyntaxTree,
        }
    }

    fn syntax_state(revision: u64, count: usize) -> ClosingHintState {
        let settings = ClosingHintSettings::default();
        let mut state = ClosingHintState::default();
        state.replace_syntax(
            revision,
            (0..count)
                .map(|line| syntax_hint(revision, line))
                .collect(),
            settings,
        );
        state
    }

    #[test]
    fn pgo_settings_disable_external_sdk_but_keep_local_closing_blocks() {
        let settings = pgo_dart_settings();

        assert!(!settings.enabled);
        assert!(!settings.workspace_analysis);
        assert_eq!(
            settings.closing_labels,
            DartClosingLabelsMode::DartServerAndBlocks
        );
        assert_eq!(
            settings.closing_hint_settings().mode,
            ClosingHintMode::DartServerAndBlocks
        );
    }

    #[test]
    fn dart_step_names_and_timeouts_are_stable_for_reports() {
        assert_eq!(DartAutomationStep::Setup.name(), "dart-setup-pgo");
        assert_eq!(DartAutomationStep::Setup.timeout(), Duration::from_secs(12));
        assert_eq!(
            DartAutomationStep::WaitClosingHints { minimum_count: 8 }.name(),
            "dart-wait-closing-hints:8"
        );
        assert_eq!(
            DartAutomationStep::WaitClosingHints { minimum_count: 8 }.timeout(),
            Duration::from_secs(90)
        );
    }

    #[test]
    fn closing_hint_wait_accepts_current_syntax_hints() {
        let settings = ClosingHintSettings::default();
        let state = syntax_state(7, 8);
        let status = wait_status("dart", 7, true, true, &state, settings);

        assert!(status.ready(8), "{}", status.diagnostics(8));
        assert!(!status.configuration_error());
        assert_eq!(status.syntax_hint_count, 8);
        assert_eq!(status.hint_count, 8);
    }

    #[test]
    fn closing_hint_wait_rejects_stale_revision_with_diagnostics() {
        let settings = ClosingHintSettings::default();
        let state = syntax_state(7, 8);
        let status = wait_status("dart", 8, true, true, &state, settings);
        let diagnostics = status.diagnostics(8);

        assert!(!status.ready(8));
        assert!(diagnostics.contains("editor_revision=8"));
        assert!(diagnostics.contains("hint_revision=7"));
        assert!(diagnostics.contains("syntax_hint_count=8"));
    }

    #[test]
    fn closing_hint_wait_rejects_zero_hints() {
        let settings = ClosingHintSettings::default();
        let state = syntax_state(8, 0);
        let status = wait_status("dart", 8, true, true, &state, settings);

        assert!(!status.ready(1));
        assert!(status.diagnostics(1).contains("syntax_hint_count=0"));
    }

    #[test]
    fn closing_hint_wait_requires_syntax_tree_source() {
        let settings = ClosingHintSettings::default();
        let mut state = ClosingHintState::default();
        state.replace_server(
            9,
            vec![ClosingHint {
                revision: 9,
                line: 12,
                anchor_byte: 120,
                label: Arc::from("server label"),
                source: ClosingHintSource::DartServer,
            }],
            settings,
        );
        let status = wait_status("dart", 9, true, true, &state, settings);

        assert!(!status.ready(1));
        assert_eq!(status.hint_count, 1);
        assert_eq!(status.syntax_hint_count, 0);
    }

    #[test]
    fn closing_hint_wait_requires_current_tree_highlight_and_dart_extension() {
        let settings = ClosingHintSettings::default();
        let state = syntax_state(10, 1);

        assert!(!wait_status("dart", 10, false, true, &state, settings).ready(1));
        assert!(!wait_status("dart", 10, true, false, &state, settings).ready(1));
        let wrong_extension = wait_status("rs", 10, true, true, &state, settings);
        assert!(!wrong_extension.ready(1));
        assert!(wrong_extension.configuration_error());
    }

    #[test]
    fn closing_hint_wait_requires_local_block_mode() {
        let mut settings = ClosingHintSettings::default();
        let state = syntax_state(11, 1);

        settings.mode = ClosingHintMode::DartServer;
        let server_only = wait_status("dart", 11, true, true, &state, settings);
        assert!(!server_only.ready(1));
        assert!(server_only.configuration_error());

        settings.mode = ClosingHintMode::Off;
        let off = wait_status("dart", 11, true, true, &state, settings);
        assert!(!off.ready(1));
        assert!(off.configuration_error());
    }

    #[test]
    fn representative_dart_syntax_parses_folds_imports_and_builds_local_hints() {
        let source = r#"import 'dart:async';
import 'dart:collection';

class PgoNode {
  Future<int> compute(List<int> values) async {
    var total = 0;
    for (final value in values) {
      if (value.isEven) {
        try {
          var cursor = value;
          while (cursor > 0) {
            total += cursor;
            cursor -= 1;
          }
        } catch (error) {
          total -= error.hashCode;
        } finally {
          total += values.length;
        }
      } else {
        total -= value;
      }
    }
    return total;
  }
}
"#;
        let language: tree_sitter::Language = tree_sitter_dart::LANGUAGE.into();
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&language).unwrap();
        let tree = parser.parse(source, None).unwrap();

        assert!(!tree.root_node().has_error());
        assert_eq!(crate::languages::dart::import_blocks(source).len(), 1);
        let hints = crate::languages::dart::local_closing_hints(
            source,
            &tree,
            42,
            ClosingHintSettings::default(),
        );
        assert!(hints.len() >= 4, "hints={hints:?}");
        assert!(
            hints
                .iter()
                .all(|hint| hint.source == ClosingHintSource::SyntaxTree)
        );
        assert!(hints.iter().all(|hint| hint.revision == 42));
    }

    #[test]
    fn diagnostics_include_all_state_needed_for_timeout_triage() {
        let settings = ClosingHintSettings {
            mode: ClosingHintMode::DartServerAndBlocks,
            minimum_nesting_depth: 4,
            minimum_block_lines: 9,
        };
        let state = syntax_state(12, 2);
        let diagnostics = wait_status("dart", 13, false, false, &state, settings).diagnostics(3);

        for marker in [
            "extension_is_dart=true",
            "editor_revision=13",
            "highlight_complete=false",
            "syntax_tree_available=false",
            "hint_revision=12",
            "hint_count=2",
            "syntax_hint_count=2",
            "required=3",
            "mode=DartServerAndBlocks",
            "minimum_nesting_depth=4",
            "minimum_block_lines=9",
        ] {
            assert!(diagnostics.contains(marker), "missing {marker}: {diagnostics}");
        }
    }
}
