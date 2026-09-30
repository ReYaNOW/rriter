//! Registry of the PGO automation groups. A group is a `GroupStart`..`GroupEnd` step run;
//! `Full` appends every group of `FULL_GROUPS`, `group:<name>` runs one on its own.

use std::path::Path;

use crate::app::App;
use crate::app::automation::AutomationStep;

/// Groups appended to the `full` scenario, in order; each group task adds its own name.
pub(super) const FULL_GROUPS: &[&str] = &["pdf"];

/// Tabs the `startup` scenario expects the restored session to hold.
const STARTUP_MIN_TABS: usize = 3;

/// `startup`: the IDE entry already ran with a saved session (see
/// `App::automation_restores_session`); no session, or an unreadable one, fails "restored tabs".
pub(super) fn startup_steps() -> Vec<AutomationStep> {
    use AutomationStep as S;
    vec![
        S::WaitUntil {
            what: "restored tabs",
            check: |app| app.tabs.len() >= STARTUP_MIN_TABS,
            timeout_ms: 5_000,
        },
        S::WaitUntil {
            what: "active tab highlighted",
            check: |app| app.startup_editor_pending.is_none() && !app.highlighter.spans.is_empty(),
            timeout_ms: 15_000,
        },
        S::ScrollEditorTimed { duration_secs: 2 },
        S::Finish,
    ]
}

/// `welcome`: starts on the welcome screen (the runner does not enter the IDE for it), then
/// takes the same path as opening a folder there.
pub(super) fn welcome_steps() -> Vec<AutomationStep> {
    use AutomationStep as S;
    vec![
        S::WaitUntil {
            what: "welcome visible",
            check: |app| app.show_welcome && !app.is_ide_mode,
            timeout_ms: 15_000,
        },
        S::ApplyWorkspace,
        S::Call {
            what: "enter ide",
            run: |app, _| {
                app.enter_ide_mode_deferred();
                app.finish_ide_deferred();
                Ok(())
            },
        },
        S::WaitReady,
        S::WaitFileTree,
        S::Finish,
    ]
}

type Requires = Option<fn(&App) -> Result<(), String>>;

/// Steps of the group `name` wrapped in `GroupStart`/`GroupEnd`; `None` for an unknown name.
pub(super) fn group_steps(name: &str, workspace: &Path) -> Option<Vec<AutomationStep>> {
    let (name, requires, body) = lookup(name, workspace)?;
    let mut steps = Vec::with_capacity(body.len() + 2);
    steps.push(AutomationStep::GroupStart { name, requires });
    steps.extend(body);
    steps.push(AutomationStep::GroupEnd);
    Some(steps)
}

fn lookup(
    name: &str,
    workspace: &Path,
) -> Option<(&'static str, Requires, Vec<AutomationStep>)> {
    match name {
        "pdf" => Some(("pdf", None, super::automation_pdf::steps(workspace))),
        "api_mock" => Some(("api_mock", None, super::automation_api_mock::steps(workspace))),
        #[cfg(test)]
        "test_never" => Some(("test_never", None, test_groups::never_steps())),
        #[cfg(test)]
        "test_input" => Some(("test_input", None, test_groups::input_steps())),
        #[cfg(test)]
        "test_skip" => Some((
            "test_skip",
            Some(|_| Err("no tool".to_string())),
            test_groups::skip_steps(),
        )),
        #[cfg(test)]
        "test_find_none" => Some(("test_find_none", None, test_groups::find_none_steps())),
        _ => None,
    }
}

#[cfg(test)]
mod registry_tests {
    use std::path::Path;

    use super::{FULL_GROUPS, group_steps};

    /// An unknown name in `FULL_GROUPS` would be dropped silently when the `full` scenario is built.
    #[test]
    fn every_full_group_resolves_to_wrapped_steps() {
        for name in FULL_GROUPS {
            let steps = group_steps(name, Path::new("/tmp/pgo-registry-test"))
                .unwrap_or_else(|| panic!("FULL_GROUPS entry {name:?} is not in the registry"));
            assert!(steps.len() > 2, "group {name:?} has no steps");
        }
    }
}

#[cfg(test)]
mod test_groups {
    use std::path::Path;

    use crate::app::App;
    use crate::ui_system::UiId;
    use crate::app::automation::{AutomationButton, AutomationStep, AutomationTarget};

    pub(super) fn never_steps() -> Vec<AutomationStep> {
        vec![AutomationStep::WaitUntil {
            what: "test-never-satisfied",
            check: |_| false,
            timeout_ms: 200,
        }]
    }

    pub(super) fn skip_steps() -> Vec<AutomationStep> {
        vec![AutomationStep::Call {
            what: "test-skip-body",
            run: |_, _| Err("the body of a skipped group must not run".to_string()),
        }]
    }

    pub(super) fn find_none_steps() -> Vec<AutomationStep> {
        vec![AutomationStep::Click {
            at: AutomationTarget::Find(|_| None),
            button: AutomationButton::Left,
            mods: "",
            clicks: 1,
        }]
    }

    fn write_and_open(app: &mut App, workspace: &Path) -> Result<(), String> {
        let dir = workspace.join("pgo_test_input");
        std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        let text: String = (0..50).map(|n| format!("line {n:02} = {n}\n")).collect();
        let path = dir.join("a.py");
        std::fs::write(&path, text).map_err(|error| error.to_string())?;
        app.open_file_in_tab(path, false);
        Ok(())
    }

    /// Inside the first text row of the editor body, from the last drawn frame.
    fn editor_first_row(app: &App) -> Option<(f32, f32)> {
        app.ui_registry.element_hits().find_map(|(id, _, rect, _)| {
            let rect = rect.filter(|_| id == UiId::EditorTextBody)?;
            Some((rect.x + 15.0, rect.y + 15.0))
        })
    }

    pub(super) fn input_steps() -> Vec<AutomationStep> {
        use AutomationStep as S;
        vec![
            S::Call { what: "test-input-open", run: write_and_open },
            S::WaitUntil {
                what: "test-input-file-open",
                check: |app| app.file_path.as_ref().is_some_and(|p| p.ends_with("pgo_test_input/a.py")),
                timeout_ms: 10_000,
            },
            S::FocusEditor,
            S::Key("ctrl+a"),
            S::TypeText("x"),
            S::WaitUntil {
                what: "test-input-replaced",
                check: |app| app.editor.get_full_text() == "x",
                timeout_ms: 5_000,
            },
            S::Key("ctrl+z"),
            S::WaitUntil {
                what: "test-input-undone",
                check: |app| app.editor.get_full_text().starts_with("line 00"),
                timeout_ms: 5_000,
            },
            S::Click {
                at: AutomationTarget::Find(editor_first_row),
                button: AutomationButton::Left,
                mods: "",
                clicks: 2,
            },
            S::WaitUntil {
                what: "test-input-word-selected",
                check: |app| app.editor.selection_anchor.is_some(),
                timeout_ms: 5_000,
            },
            S::Wheel {
                at: AutomationTarget::Find(editor_first_row),
                dx: 0.0,
                dy: -5.0,
            },
            S::WaitUntil {
                what: "test-input-scrolled",
                check: |app| app.scroll_y.target > 0.0,
                timeout_ms: 5_000,
            },
        ]
    }
}
