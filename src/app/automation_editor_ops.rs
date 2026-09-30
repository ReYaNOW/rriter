//! `editor_ops` PGO group: opens a generated Python file, puts a cursor on five lines with
//! Alt-clicks, types at all of them, then copies the whole buffer, pastes it three times and
//! walks the undo history back and forth.

use std::fmt::Write as _;
use std::path::Path;

use crate::app::App;
use crate::app::automation::{AutomationButton, AutomationStep, AutomationTarget};
use crate::ui_system::UiId;

const FILE: &str = "pgo_editor/ops.py";
/// `ops.py` holds this many 6-line functions (well over 200 lines).
const FUNCTIONS: usize = 40;
/// Lines that get an extra cursor (zero based; all inside the first screen of the file).
const CURSOR_LINES: [usize; 5] = [2, 4, 6, 8, 10];
/// Horizontal offset of the Alt-clicks from the left edge of the editor body, in pixels.
const TEXT_INSET: f32 = 40.0;
const PASTES: usize = 3;
const UNDOS: usize = 5;
const REDOS: usize = 3;

fn ops_text() -> String {
    let mut text = String::from("\"\"\"Generated PGO fixture.\"\"\"\n\n\n");
    for index in 0..FUNCTIONS {
        let _ = write!(
            text,
            "def op_{index:02}(items):\n    total = {index}\n    for item in items:\n        \
             total += item * {index}\n    return total\n\n\n"
        );
    }
    text
}

fn prepare(app: &mut App, workspace: &Path) -> Result<(), String> {
    let dir = workspace.join("pgo_editor");
    std::fs::create_dir_all(&dir).map_err(|error| format!("create pgo_editor: {error}"))?;
    let path = workspace.join(FILE);
    std::fs::write(&path, ops_text()).map_err(|error| format!("write ops.py: {error}"))?;
    app.open_file_in_tab(path, false);
    Ok(())
}

/// The active tab's path lives in `App::file_path`, not in the tab.
fn ops_open(app: &App) -> bool {
    app.file_path.as_deref().is_some_and(|path| path.ends_with(FILE))
        && app.editor.line_offsets.len() > CURSOR_LINES[CURSOR_LINES.len() - 1] + 2
}

/// Point inside the visible editor row `line` (near its left edge), from the last drawn frame; `None` while the
/// body is not drawn or the line is off screen.
fn line_point<const LINE: usize>(app: &App) -> Option<(f32, f32)> {
    app.editor.line_offsets.get(LINE)?;
    let renderer = app.renderer.as_ref()?;
    let (body_x, body_y, body_h) = app.ui_registry.element_hits().find_map(|(id, _, rect, _)| {
        let rect = rect.filter(|_| id == UiId::EditorTextBody)?;
        Some((rect.x, rect.y, rect.h))
    })?;
    let y = renderer.line_height * (LINE as f32 + 0.5) - app.scroll_y.current;
    // Clear of the git change markers at the very left of the body: a click there opens the
    // inline git panel, which then blocks the editor.
    (y >= 0.0 && y < body_h).then(|| ((body_x + TEXT_INSET).round(), (body_y + y).round()))
}

/// Typed at all carets; does not occur in `ops_text`.
const TYPED: &str = "_x";
/// Marker of the per-caret clipboard lines (`@0` .. `@5`); does not occur in `ops_text`.
const PASTED: &str = "@";
/// The primary caret plus the Alt-click carets.
const CARETS: usize = CURSOR_LINES.len() + 1;

fn typed_everywhere(app: &App, needle: &str, expected: usize) -> bool {
    app.editor.get_full_text().matches(needle).count() == expected
}

fn set_caret_clipboard(app: &mut App, _workspace: &Path) -> Result<(), String> {
    let lines: Vec<String> = (0..CARETS).map(|index| format!("{PASTED}{index}")).collect();
    app.set_clipboard_text(lines.join("\n"));
    Ok(())
}

fn alt_click(at: fn(&App) -> Option<(f32, f32)>) -> AutomationStep {
    AutomationStep::Click {
        at: AutomationTarget::Find(at),
        button: AutomationButton::Left,
        mods: "alt",
        clicks: 1,
    }
}

fn buffer_changed(app: &App) -> bool {
    app.tab_text_is_dirty(app.active_tab) && app.editor.get_full_text() != ops_text()
}

pub(super) fn steps(_workspace: &Path) -> Vec<AutomationStep> {
    use AutomationStep as S;
    let mut steps = vec![
        S::Call { what: "editor ops fixture", run: prepare },
        S::WaitUntil { what: "ops.py open", check: ops_open, timeout_ms: 15_000 },
        S::WaitUntil {
            what: "ops.py rows drawn",
            check: |app| line_point::<2>(app).is_some() && line_point::<10>(app).is_some(),
            timeout_ms: 10_000,
        },
        alt_click(line_point::<2>),
        alt_click(line_point::<4>),
        alt_click(line_point::<6>),
        alt_click(line_point::<8>),
        alt_click(line_point::<10>),
        S::WaitUntil {
            what: "five cursors",
            check: |app| app.editor.extra_cursors().len() == CURSOR_LINES.len(),
            timeout_ms: 5_000,
        },
        // Earlier groups leave panel inputs (terminal, search, commit message) focused, and those
        // swallow typed text before it reaches the editor; hand the keyboard back to it.
        S::FocusEditor,
        S::TypeText(TYPED),
        S::WaitUntil {
            what: "text typed at every cursor",
            check: |app| typed_everywhere(app, TYPED, CARETS),
            timeout_ms: 5_000,
        },
        // Grouped undo: one Ctrl+Z reverts the edit at every cursor, Ctrl+Y replays it.
        S::Key("ctrl+z"),
        S::WaitUntil {
            what: "typing undone at every cursor",
            check: |app| typed_everywhere(app, TYPED, 0),
            timeout_ms: 5_000,
        },
        S::Key("ctrl+y"),
        S::WaitUntil {
            what: "typing redone at every cursor",
            check: |app| typed_everywhere(app, TYPED, CARETS),
            timeout_ms: 5_000,
        },
        // One clipboard line per caret takes the per-caret paste path.
        S::Call { what: "one clipboard line per caret", run: set_caret_clipboard },
        S::Key("ctrl+v"),
        S::WaitUntil {
            what: "paste landed at every cursor",
            check: |app| typed_everywhere(app, PASTED, CARETS),
            timeout_ms: 5_000,
        },
        S::Key("ctrl+z"),
        S::WaitUntil {
            what: "paste undone at every cursor",
            check: |app| typed_everywhere(app, PASTED, 0) && typed_everywhere(app, TYPED, CARETS),
            timeout_ms: 5_000,
        },
        S::Key("escape"),
        S::WaitFrames(2),
        S::Key("ctrl+a"),
        S::Key("ctrl+c"),
        S::Key("ctrl+end"),
    ];
    steps.extend((0..PASTES).map(|_| S::Key("ctrl+v")));
    steps.extend((0..UNDOS).map(|_| S::Key("ctrl+z")));
    steps.extend((0..REDOS).map(|_| S::Key("ctrl+y")));
    steps.push(S::WaitUntil { what: "buffer changed", check: buffer_changed, timeout_ms: 10_000 });
    steps
}
