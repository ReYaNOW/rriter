use std::path::PathBuf;
use std::time::Duration;

use crate::app::automation::{AutomationStep, request_redraw};
use crate::app::{App, MarkdownMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MarkdownAutomationStep {
    SetMode(MarkdownMode),
    WaitReadReady,
    ScrollReadTimed { duration_secs: u16 },
}

impl MarkdownAutomationStep {
    pub(super) fn name(self) -> String {
        match self {
            Self::SetMode(MarkdownMode::Edit) => "set-markdown-mode:edit".to_string(),
            Self::SetMode(MarkdownMode::Read) => "set-markdown-mode:read".to_string(),
            Self::WaitReadReady => "wait-markdown-read-ready".to_string(),
            Self::ScrollReadTimed { duration_secs } => {
                format!("scroll-markdown-read-timed:{duration_secs}s")
            }
        }
    }

    pub(super) fn timeout(self) -> Duration {
        match self {
            Self::WaitReadReady => Duration::from_secs(30),
            Self::ScrollReadTimed { duration_secs } => {
                Duration::from_secs(u64::from(duration_secs) + 5)
            }
            Self::SetMode(_) => Duration::from_secs(12),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum MarkdownStepResult {
    Pending,
    Done,
    Failed(String),
}

pub(super) fn run_step(app: &mut App, step: MarkdownAutomationStep) -> MarkdownStepResult {
    match step {
        MarkdownAutomationStep::SetMode(mode) => {
            app.set_markdown_mode(mode);
            if app.markdown_mode() == mode {
                MarkdownStepResult::Done
            } else {
                MarkdownStepResult::Failed(format!(
                    "Markdown mode {:?} is unavailable for the active automation document",
                    mode
                ))
            }
        }
        MarkdownAutomationStep::WaitReadReady => {
            if app.markdown_mode() != MarkdownMode::Read {
                return MarkdownStepResult::Failed(
                    "Markdown Read readiness requested outside Read mode".to_string(),
                );
            }
            if app.markdown.read_document(app.editor.version).is_some() {
                MarkdownStepResult::Done
            } else {
                app.refresh_markdown_read_model_if_stale();
                request_redraw(app);
                if app.markdown.read_document(app.editor.version).is_some() {
                    MarkdownStepResult::Done
                } else {
                    MarkdownStepResult::Pending
                }
            }
        }
        MarkdownAutomationStep::ScrollReadTimed { .. } => MarkdownStepResult::Failed(
            "timed Markdown Read scroll dispatched through wrong path".to_string(),
        ),
    }
}

pub(super) fn prepare_read_scroll(app: &mut App) -> Result<bool, String> {
    if app.markdown_mode() != MarkdownMode::Read {
        return Err("Markdown Read scroll requested outside Read mode".to_string());
    }
    if app.markdown.read_max_scroll <= 0.0 {
        request_redraw(app);
        return Ok(false);
    }
    Ok(true)
}

pub(super) fn scroll_read(app: &mut App, direction: f32) -> Result<(), String> {
    if app.markdown_mode() != MarkdownMode::Read {
        return Err("Markdown Read scroll requested outside Read mode".to_string());
    }
    let max_scroll = app.markdown.read_max_scroll;
    if max_scroll <= 0.0 {
        return Err("Markdown Read scroll range is unavailable".to_string());
    }
    crate::app::markdown::scroll_markdown_read(
        &mut app.markdown.read_scroll_y,
        max_scroll,
        36.0 * direction,
    );
    request_redraw(app);
    Ok(())
}

pub(super) fn markdown_scenario_steps() -> Vec<AutomationStep> {
    use AutomationStep as S;
    use MarkdownAutomationStep as M;

    vec![
        S::OpenFile(PathBuf::from("README.md")),
        S::WaitHighlight,
        S::WaitFrames(6),
        S::ScrollEditorTimed { duration_secs: 6 },
        S::SetEditorCursorAfter("RRITER_PGO_MARKDOWN_EDIT_TARGET"),
        S::FocusEditor,
        S::TypeText(
            "\n\n### Incremental Markdown PGO\n\n**incremental strong** and `inline_incremental()` ",
        ),
        S::WaitHighlight,
        S::Markdown(M::SetMode(MarkdownMode::Read)),
        S::Markdown(M::WaitReadReady),
        S::WaitFrames(8),
        S::Markdown(M::ScrollReadTimed { duration_secs: 8 }),
        S::WaitFrames(4),
        S::Markdown(M::SetMode(MarkdownMode::Edit)),
        S::WaitHighlight,
        S::WaitFrames(4),
        S::SetEditorCursorAfter("`inline_incremental()`"),
        S::FocusEditor,
        S::TypeText(
            "\n\n> Rebuilt preview after incremental edit.\n> RRITER_PGO_MARKDOWN_INCREMENTAL_TWO",
        ),
        S::WaitHighlight,
        S::Markdown(M::SetMode(MarkdownMode::Read)),
        S::Markdown(M::WaitReadReady),
        S::WaitFrames(8),
        S::Markdown(M::ScrollReadTimed { duration_secs: 6 }),
        S::WaitFrames(4),
        S::Markdown(M::SetMode(MarkdownMode::Edit)),
        S::WaitHighlight,
        S::WaitFrames(4),
    ]
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::app::app_behavior_tests::test_app;

    fn position(steps: &[AutomationStep], predicate: impl Fn(&AutomationStep) -> bool) -> usize {
        steps.iter().position(predicate).unwrap()
    }

    fn position_after(
        steps: &[AutomationStep],
        start: usize,
        predicate: impl Fn(&AutomationStep) -> bool,
    ) -> usize {
        steps
            .iter()
            .enumerate()
            .skip(start)
            .find_map(|(index, step)| predicate(step).then_some(index))
            .unwrap()
    }

    #[test]
    fn markdown_step_names_and_timeouts_are_stable_for_reports() {
        assert_eq!(
            MarkdownAutomationStep::SetMode(MarkdownMode::Edit).name(),
            "set-markdown-mode:edit"
        );
        assert_eq!(
            MarkdownAutomationStep::SetMode(MarkdownMode::Read).name(),
            "set-markdown-mode:read"
        );
        assert_eq!(
            MarkdownAutomationStep::WaitReadReady.name(),
            "wait-markdown-read-ready"
        );
        assert_eq!(
            MarkdownAutomationStep::ScrollReadTimed { duration_secs: 8 }.name(),
            "scroll-markdown-read-timed:8s"
        );
        assert_eq!(
            MarkdownAutomationStep::SetMode(MarkdownMode::Read).timeout(),
            Duration::from_secs(12)
        );
        assert_eq!(
            MarkdownAutomationStep::WaitReadReady.timeout(),
            Duration::from_secs(30)
        );
        assert_eq!(
            MarkdownAutomationStep::ScrollReadTimed { duration_secs: 8 }.timeout(),
            Duration::from_secs(13)
        );
    }

    #[test]
    fn markdown_scenario_covers_edit_read_rebuild_sequence() {
        let steps = markdown_scenario_steps();
        assert!(matches!(
            steps.first(),
            Some(AutomationStep::OpenFile(path)) if path == Path::new("README.md")
        ));
        assert!(matches!(steps.get(1), Some(AutomationStep::WaitHighlight)));
        assert!(steps.iter().any(|step| matches!(
            step,
            AutomationStep::ScrollEditorTimed { duration_secs: 6 }
        )));

        let first_source_edit = position(&steps, |step| {
            matches!(
                step,
                AutomationStep::TypeText(text) if text.contains("Incremental Markdown PGO")
            )
        });
        assert!(matches!(
            steps.get(first_source_edit + 1),
            Some(AutomationStep::WaitHighlight)
        ));

        let first_read = position_after(&steps, first_source_edit + 1, |step| {
            matches!(
                step,
                AutomationStep::Markdown(MarkdownAutomationStep::SetMode(MarkdownMode::Read))
            )
        });
        assert!(matches!(
            steps.get(first_read + 1),
            Some(AutomationStep::Markdown(
                MarkdownAutomationStep::WaitReadReady
            ))
        ));
        let first_read_scroll = position_after(&steps, first_read + 1, |step| {
            matches!(
                step,
                AutomationStep::Markdown(MarkdownAutomationStep::ScrollReadTimed {
                    duration_secs: 8
                })
            )
        });
        let edit_after_first_read = position_after(&steps, first_read_scroll + 1, |step| {
            matches!(
                step,
                AutomationStep::Markdown(MarkdownAutomationStep::SetMode(MarkdownMode::Edit))
            )
        });

        let second_source_edit = position_after(&steps, edit_after_first_read + 1, |step| {
            matches!(
                step,
                AutomationStep::TypeText(text)
                    if text.contains("RRITER_PGO_MARKDOWN_INCREMENTAL_TWO")
            )
        });
        assert!(matches!(
            steps.get(second_source_edit + 1),
            Some(AutomationStep::WaitHighlight)
        ));

        let second_read = position_after(&steps, second_source_edit + 1, |step| {
            matches!(
                step,
                AutomationStep::Markdown(MarkdownAutomationStep::SetMode(MarkdownMode::Read))
            )
        });
        assert!(matches!(
            steps.get(second_read + 1),
            Some(AutomationStep::Markdown(
                MarkdownAutomationStep::WaitReadReady
            ))
        ));
        let second_read_scroll = position_after(&steps, second_read + 1, |step| {
            matches!(
                step,
                AutomationStep::Markdown(MarkdownAutomationStep::ScrollReadTimed {
                    duration_secs: 6
                })
            )
        });
        let final_edit = position_after(&steps, second_read_scroll + 1, |step| {
            matches!(
                step,
                AutomationStep::Markdown(MarkdownAutomationStep::SetMode(MarkdownMode::Edit))
            )
        });
        assert!(matches!(
            steps.get(final_edit + 1),
            Some(AutomationStep::WaitHighlight)
        ));
        assert!(matches!(steps.last(), Some(AutomationStep::WaitFrames(4))));

        assert_eq!(
            steps
                .iter()
                .filter(|step| matches!(
                    step,
                    AutomationStep::Markdown(MarkdownAutomationStep::SetMode(MarkdownMode::Read))
                ))
                .count(),
            2
        );
        assert_eq!(
            steps
                .iter()
                .filter(|step| matches!(
                    step,
                    AutomationStep::Markdown(MarkdownAutomationStep::SetMode(MarkdownMode::Edit))
                ))
                .count(),
            2
        );
        assert_eq!(
            steps
                .iter()
                .filter(|step| matches!(
                    step,
                    AutomationStep::Markdown(MarkdownAutomationStep::WaitReadReady)
                ))
                .count(),
            2
        );
        assert_eq!(
            steps
                .iter()
                .filter(|step| matches!(
                    step,
                    AutomationStep::Markdown(MarkdownAutomationStep::ScrollReadTimed { .. })
                ))
                .count(),
            2
        );
    }

    #[test]
    fn markdown_scenario_uses_semantic_steps_without_coordinate_clicks() {
        let steps = markdown_scenario_steps();
        assert!(!steps.iter().any(|step| matches!(
            step,
            AutomationStep::JumpMinimap(_)
        )));
        for step in &steps {
            let debug = format!("{step:?}").to_ascii_lowercase();
            assert!(!debug.contains("click"), "coordinate click step: {step:?}");
            assert!(!debug.contains("uiid"), "UI-id step: {step:?}");
        }
    }

    #[test]
    fn markdown_run_step_preserves_current_failure_diagnostics() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.file_path = Some(PathBuf::from("/tmp/readme.md"));
        app.file_extension = "md".to_string();

        assert_eq!(
            run_step(&mut app, MarkdownAutomationStep::WaitReadReady),
            MarkdownStepResult::Failed(
                "Markdown Read readiness requested outside Read mode".to_string()
            )
        );

        app.file_path = Some(PathBuf::from("/tmp/note.txt"));
        app.file_extension = "txt".to_string();
        assert_eq!(
            run_step(
                &mut app,
                MarkdownAutomationStep::SetMode(MarkdownMode::Read)
            ),
            MarkdownStepResult::Failed(
                "Markdown mode Read is unavailable for the active automation document".to_string()
            )
        );
    }

    #[test]
    fn markdown_read_scroll_waits_for_layout_and_uses_production_scroll_state() {
        let Some(mut app) = test_app() else {
            return;
        };
        app.file_path = Some(PathBuf::from("/tmp/readme.md"));
        app.file_extension = "md".to_string();
        assert_eq!(
            run_step(
                &mut app,
                MarkdownAutomationStep::SetMode(MarkdownMode::Read)
            ),
            MarkdownStepResult::Done
        );
        assert_eq!(prepare_read_scroll(&mut app), Ok(false));

        app.markdown.read_max_scroll = 200.0;
        assert_eq!(prepare_read_scroll(&mut app), Ok(true));
        assert_eq!(app.markdown.read_scroll_y.target, 0.0);
        assert_eq!(scroll_read(&mut app, 1.0), Ok(()));
        assert_eq!(app.markdown.read_scroll_y.target, 36.0);
        assert_eq!(app.markdown.read_scroll_y.anim_speed, 7.0);
    }
}
