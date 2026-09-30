//! `git_changes` PGO group: commits a generated Python file, edits it in five places, opens
//! the change list and its diff, walks the hunks, rolls one back, stages and commits the file,
//! then checks that a tab of a file deleted on disk gets marked.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::app::automation::{AutomationButton, AutomationStep, AutomationTarget};
use crate::app::git_panel::run_git_local;
use crate::app::{App, PanelId};
use crate::ui_system::UiId;

const DIR: &str = "pgo_git";
const CHANGES: &str = "pgo_git/changes.py";
const GONE: &str = "pgo_git/gone.py";
/// `changes.py` holds this many 8-line functions (well over 300 lines).
const FUNCTIONS: usize = 42;
/// Every `EDIT_EVERY`-th function is edited, so the edits form separate diff hunks.
const EDIT_EVERY: usize = 8;
/// Present in the edited file only.
const EDIT_MARK: &str = "# pgo edit";
const COMMIT_MESSAGE: &str = "pgo commit";
const WHEEL_STEPS: usize = 10;

fn changes_text(edited: bool) -> String {
    let mut text = String::from("\"\"\"Generated PGO fixture.\"\"\"\n\n\n");
    for index in 0..FUNCTIONS {
        let init = if edited && index % EDIT_EVERY == 2 {
            format!("total = 1  {EDIT_MARK} {index}")
        } else {
            "total = 0".to_string()
        };
        let _ = write!(
            text,
            "def func_{index:02}(x):\n    \"\"\"Deterministic helper {index}.\"\"\"\n    {init}\n    \
             for step in range({index}):\n        total += step * x\n    return total\n\n\n"
        );
    }
    text
}

/// Writes and commits the two files, then edits `changes.py`. Runs `git` off the frame thread;
/// the edited `changes.py` is the completion signal (see `fixture_ready`).
fn prepare(app: &mut App, workspace: &Path) -> Result<(), String> {
    let dir = workspace.join(DIR);
    std::fs::create_dir_all(&dir).map_err(|error| format!("create {DIR}: {error}"))?;
    std::fs::write(workspace.join(CHANGES), changes_text(false))
        .map_err(|error| format!("write changes.py: {error}"))?;
    std::fs::write(workspace.join(GONE), "GONE = True\n")
        .map_err(|error| format!("write gone.py: {error}"))?;
    // The sidebar slot toggles, so an already open Git panel must be closed for the click.
    if app.ide_panel.is_open(PanelId::Git) {
        app.ide_panel.toggle(PanelId::Git);
    }
    // An open bottom panel (the terminal `full` leaves open) shortens the side panel and clips
    // the commit controls in a small window.
    if let Some(bottom) = app.ide_panel.open_bottom_panel_id() {
        app.ide_panel.toggle(bottom);
    }
    let root = workspace.to_path_buf();
    std::thread::spawn(move || {
        if let Err(error) = commit_and_edit(&root) {
            eprintln!("[PGO git_changes] fixture failed: {error}");
        }
    });
    Ok(())
}

fn commit_and_edit(root: &Path) -> Result<(), String> {
    run_git_local(root, &["add", "--", CHANGES, GONE], "pgo add")?;
    run_git_local(
        root,
        &[
            "-c", "user.name=RRiter PGO", "-c", "user.email=pgo@rriter.invalid",
            "-c", "commit.gpgsign=false", "commit", "-q", "--no-verify",
            "-m", "PGO git_changes fixture",
        ],
        "pgo commit",
    )?;
    std::fs::write(root.join(CHANGES), changes_text(true))
        .map_err(|error| format!("edit changes.py: {error}"))
}

fn workspace_root(app: &App) -> Option<PathBuf> {
    app.ide_workspaces.first().cloned()
}

fn fixture_ready(app: &App) -> bool {
    workspace_root(app)
        .and_then(|root| std::fs::read_to_string(root.join(CHANGES)).ok())
        .is_some_and(|text| text.contains(EDIT_MARK))
}

/// Index of `changes.py` in the first workspace's change list.
fn changes_index(app: &App) -> Option<usize> {
    let workspace = app.ide_panel.git.snapshot.workspaces.first()?;
    workspace.files.iter().position(|file| &*file.rel_path == CHANGES)
}

fn changes_entry_staged(app: &App) -> Option<bool> {
    let workspace = app.ide_panel.git.snapshot.workspaces.first()?;
    workspace.files.iter().find(|file| &*file.rel_path == CHANGES).map(|file| file.staged)
}

/// Center of the `changes.py` row element built by `id` (the file list is not sorted the way
/// the fixture is written, so the index is looked up instead of assumed).
fn row_center(app: &App, id: fn(usize, usize) -> UiId) -> Option<(f32, f32)> {
    let wanted = id(0, changes_index(app)?);
    app.ui_registry.element_hits().find_map(|(hit_id, _, rect, _)| {
        let rect = rect.filter(|_| hit_id == wanted)?;
        Some((rect.x + rect.w / 2.0, rect.y + rect.h / 2.0))
    })
}

fn diff_row(app: &App) -> Option<(f32, f32)> {
    row_center(app, UiId::GitFileDiff)
}

fn stage_checkbox(app: &App) -> Option<(f32, f32)> {
    row_center(app, UiId::GitFile)
}

/// Inside the first text row of the editor body (the diff view), from the last drawn frame.
fn diff_body(app: &App) -> Option<(f32, f32)> {
    app.ui_registry.element_hits().find_map(|(id, _, rect, _)| {
        let rect = rect.filter(|_| id == UiId::EditorTextBody)?;
        Some((rect.x + rect.w / 2.0, rect.y + rect.h / 2.0))
    })
}

/// Rollback icon of a hunk whose header is on screen in the active (diff) tab. After the
/// hunk jumps and the wheel the first hunk has scrolled out of view, and the id carries the
/// tab index, which is not 0 when earlier groups left tabs open.
fn rollback_visible_hunk(app: &App) -> Option<(f32, f32)> {
    let tab = app.active_tab;
    app.ui_registry.element_hits().find_map(|(hit_id, _, rect, _)| {
        let rect = rect?;
        matches!(hit_id, UiId::GitDiffRollbackHunk(hit_tab, _) if hit_tab == tab)
            .then(|| (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0))
    })
}

fn ui_visible(app: &App, id: UiId) -> bool {
    app.ui_registry.element_hits().any(|(hit_id, _, rect, _)| hit_id == id && rect.is_some())
}

fn open_gone_tab(app: &mut App, workspace: &Path) -> Result<(), String> {
    app.open_file_in_tab(workspace.join(GONE), false);
    Ok(())
}

/// Index of the `gone.py` tab; the active tab's path lives in `App::file_path`, not in the tab.
fn gone_tab_index(app: &App) -> Option<usize> {
    (0..app.tabs.len()).find(|&index| {
        let path = if index == app.active_tab {
            app.file_path.as_deref()
        } else {
            app.tabs[index].file_path.as_deref()
        };
        path.is_some_and(|path| path.ends_with(GONE))
    })
}

fn gone_tab_open(app: &App) -> bool {
    gone_tab_index(app).is_some()
}

fn remove_gone(_app: &mut App, workspace: &Path) -> Result<(), String> {
    std::fs::remove_file(workspace.join(GONE)).map_err(|error| format!("remove gone.py: {error}"))
}

fn gone_tab_deleted(app: &App) -> bool {
    gone_tab_index(app).and_then(|index| app.tabs.get(index)).is_some_and(|tab| tab.deleted)
}

fn click(at: AutomationTarget, clicks: u8) -> AutomationStep {
    AutomationStep::Click { at, button: AutomationButton::Left, mods: "", clicks }
}

fn click_ui(id: UiId) -> AutomationStep {
    click(AutomationTarget::Ui(id), 1)
}

pub(super) fn steps(_workspace: &Path) -> Vec<AutomationStep> {
    use AutomationStep as S;
    let mut steps = vec![
        S::Call { what: "git changes fixture", run: prepare },
        S::WaitUntil { what: "git fixture edited", check: fixture_ready, timeout_ms: 20_000 },
        click_ui(UiId::SidebarSlot(PanelId::Git)),
        S::WaitUntil {
            what: "status shows changes.py",
            check: |app| diff_row(app).is_some(),
            timeout_ms: 20_000,
        },
        click(AutomationTarget::Find(diff_row), 2),
        S::WaitUntil {
            what: "diff view open",
            check: |app| app.active_tab_is_git_diff() && ui_visible(app, UiId::GitDiffNextHunk),
            timeout_ms: 10_000,
        },
    ];
    steps.extend((0..4).map(|_| click_ui(UiId::GitDiffNextHunk)));
    steps.extend((0..2).map(|_| click_ui(UiId::GitDiffPrevHunk)));
    // A negative wheel delta scrolls the text down.
    steps.extend((0..WHEEL_STEPS).map(|_| S::Wheel {
        at: AutomationTarget::Find(diff_body),
        dx: 0.0,
        dy: -3.0,
    }));
    // The hunk jumps animate the scroll, so the wheel lands on a moving target and (under load)
    // can leave no hunk header on screen. Jump once more and wait until the scroll has settled
    // on a hunk header whose rollback icon is registered before clicking it.
    steps.extend([
        click_ui(UiId::GitDiffNextHunk),
        S::WaitUntil {
            what: "rollback icon visible",
            check: |app| app.scroll_y.is_settled() && rollback_visible_hunk(app).is_some(),
            timeout_ms: 10_000,
        },
        S::WaitFrames(2),
        click(AutomationTarget::Find(rollback_visible_hunk), 1),
        S::WaitFrames(2),
        S::Key("ctrl+s"),
        S::WaitFrames(5),
        click(AutomationTarget::Find(stage_checkbox), 1),
        S::WaitUntil {
            what: "changes.py staged",
            check: |app| changes_entry_staged(app) == Some(true),
            timeout_ms: 10_000,
        },
        click_ui(UiId::GitMessageInput),
        S::WaitFrames(2),
        S::TypeText(COMMIT_MESSAGE),
        S::WaitFrames(2),
        click_ui(UiId::GitCommit),
        S::WaitUntil {
            what: "commit landed",
            check: |app| {
                !app.ide_panel.git.pending
                    && !app.ide_panel.git.snapshot.workspaces.is_empty()
                    && changes_index(app).is_none()
            },
            timeout_ms: 20_000,
        },
        S::Call { what: "gone tab open", run: open_gone_tab },
        S::WaitUntil { what: "gone tab opened", check: gone_tab_open, timeout_ms: 10_000 },
        S::Call { what: "gone file remove", run: remove_gone },
        S::WaitUntil { what: "deleted tab", check: gone_tab_deleted, timeout_ms: 10_000 },
    ]);
    steps
}
