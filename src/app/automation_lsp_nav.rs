//! `lsp_nav` PGO group: writes a small Python module whose functions call each other, waits
//! for the `ty` language server, then goes to the definition of three calls with Ctrl-hover
//! and Ctrl-click. Skipped when `ty` cannot be resolved (see `requires`).

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use winit::keyboard::ModifiersState;

use crate::app::App;
use crate::app::automation::{AutomationButton, AutomationStep, AutomationTarget};
use crate::ui_system::UiId;

const FILE: &str = "pgo_lsp/nav.py";
/// Line numbers are zero based. The last call sits on line 15, inside the first screen.
const NAV_TEXT: &str = "\"\"\"Generated PGO navigation fixture.\"\"\"\n\ndef alpha(x):\n    return x + 1\n\n\n\
                        def beta(x):\n    return alpha(x) * 2\n\n\ndef gamma(x):\n    return beta(x) + 1\n\n\n\
                        def run():\n    return gamma(1)\n";
/// `(callee, line of the call, line of the definition)`; every definition line differs from
/// the cursor line right after opening the file and from the other jumps.
const JUMPS: [(&str, usize, usize); 3] = [("alpha", 7, 2), ("beta", 11, 6), ("gamma", 15, 10)];

/// `ty` must resolve the way the LSP manager resolves it (override env, settings, then PATH).
pub(super) fn requires(_app: &App) -> Result<(), String> {
    requires_with(|| crate::platform::resolve_tool_executable(OsStr::new("ty"), "RRITER_TY_PATH"))
}

fn requires_with(resolve: impl FnOnce() -> Option<PathBuf>) -> Result<(), String> {
    resolve().map(|_| ()).ok_or_else(|| "ty not found".to_string())
}

fn prepare(app: &mut App, workspace: &Path) -> Result<(), String> {
    let dir = workspace.join("pgo_lsp");
    std::fs::create_dir_all(&dir).map_err(|error| format!("create pgo_lsp: {error}"))?;
    let path = workspace.join(FILE);
    std::fs::write(&path, NAV_TEXT).map_err(|error| format!("write nav.py: {error}"))?;
    app.open_file_in_tab(path, false);
    Ok(())
}

/// The active tab's path lives in `App::file_path`, not in the tab.
fn nav_open(app: &App) -> bool {
    app.file_path.as_deref().is_some_and(|path| path.ends_with(FILE))
        && app.editor.line_offsets.len() > JUMPS[JUMPS.len() - 1].1
}

fn ty_running(app: &App) -> bool {
    app.ide_panel
        .lsp_servers
        .iter()
        .any(|server| server.name == "ty" && server.status == crate::lsp::LspServerStatus::Running)
}

/// Middle of the callee name of call `N` in the last drawn frame; `None` while the editor body
/// is not drawn or the line is off screen. The text is monospace ASCII, so the column maps
/// straight to pixels (no inlay hint precedes a callee in `NAV_TEXT`).
fn call_point<const N: usize>(app: &App) -> Option<(f32, f32)> {
    let (callee, line, _) = JUMPS[N];
    app.editor.line_offsets.get(line)?;
    let column = NAV_TEXT.lines().nth(line)?.find(callee)?;
    let renderer = app.renderer.as_ref()?;
    let (body_x, body_y, body_h) = app.ui_registry.element_hits().find_map(|(id, _, rect, _)| {
        let rect = rect.filter(|_| id == UiId::EditorTextBody)?;
        Some((rect.x, rect.y, rect.h))
    })?;
    let visual_line = renderer.phys_to_visual.get(line).copied().unwrap_or(line) as f32;
    let x = body_x + (column as f32 + callee.len() as f32 / 2.0) * renderer.ascii_advances['a' as usize]
        - app.scroll_x.current;
    let y = renderer.line_height * (visual_line + 0.5) - app.scroll_y.current;
    (y >= 0.0 && y < body_h).then(|| (x.round(), (body_y + y).round()))
}

/// Holds Ctrl (it must span the mouse events; the `Click` step restores it afterwards) and
/// moves the pointer over call `N`, which starts the definition request.
fn hover<const N: usize>(app: &mut App, _workspace: &Path) -> Result<(), String> {
    let (x, y) = call_point::<N>(app).ok_or_else(|| "call is not drawn".to_string())?;
    app.modifiers = ModifiersState::CONTROL;
    app.handle_main_cursor_moved(winit::dpi::PhysicalPosition::new(f64::from(x), f64::from(y)));
    Ok(())
}

fn release(app: &mut App, _workspace: &Path) -> Result<(), String> {
    app.modifiers = ModifiersState::empty();
    app.clear_ctrl_definition();
    Ok(())
}

fn definition_ready(app: &App) -> bool {
    app.ctrl_definition.target.is_some()
}

/// The cursor line is 1-based, like the state dump.
fn cursor_at_definition<const N: usize>(app: &App) -> bool {
    crate::render_view::cursor_line_and_character(&app.editor).0 == JUMPS[N].2 + 1
}

fn jump_steps<const N: usize>() -> Vec<AutomationStep> {
    use AutomationStep as S;
    vec![
        S::Call { what: "ctrl hover", run: hover::<N> },
        S::WaitUntil { what: "definition target", check: definition_ready, timeout_ms: 20_000 },
        S::Click {
            at: AutomationTarget::Find(call_point::<N>),
            button: AutomationButton::Left,
            mods: "ctrl",
            clicks: 1,
        },
        S::WaitUntil { what: "cursor at definition", check: cursor_at_definition::<N>, timeout_ms: 10_000 },
        S::Call { what: "ctrl release", run: release },
        S::WaitFrames(2),
    ]
}

pub(super) fn steps(_workspace: &Path) -> Vec<AutomationStep> {
    use AutomationStep as S;
    let mut steps = vec![
        S::Call { what: "lsp nav fixture", run: prepare },
        S::WaitUntil { what: "nav.py open", check: nav_open, timeout_ms: 15_000 },
        S::WaitUntil { what: "ty running", check: ty_running, timeout_ms: 30_000 },
        S::WaitUntil {
            what: "nav.py calls drawn",
            check: |app| {
                call_point::<0>(app).is_some()
                    && call_point::<1>(app).is_some()
                    && call_point::<2>(app).is_some()
            },
            timeout_ms: 10_000,
        },
    ];
    steps.extend(jump_steps::<0>());
    steps.extend(jump_steps::<1>());
    steps.extend(jump_steps::<2>());
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_ty_skips_the_group_with_a_reason() {
        assert_eq!(requires_with(|| None), Err("ty not found".to_string()));
        assert_eq!(requires_with(|| Some(PathBuf::from("/bin/ty"))), Ok(()));
    }

    #[test]
    fn jump_table_matches_the_fixture_text() {
        for (callee, call_line, def_line) in JUMPS {
            let call = NAV_TEXT.lines().nth(call_line).expect("call line");
            assert!(call.contains(&format!("{callee}(")), "{call:?}");
            let definition = NAV_TEXT.lines().nth(def_line).expect("definition line");
            assert!(definition.starts_with(&format!("def {callee}(")), "{definition:?}");
        }
    }
}
