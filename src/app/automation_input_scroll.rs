//! `input_scroll` PGO group: opens a generated 20 000-line Rust file and scrolls it every way
//! a user does: mouse wheel, PageUp/PageDown, Ctrl-Home/End and a drag of the vertical
//! scrollbar thumb from the top to the bottom.

use std::fmt::Write as _;
use std::path::Path;

use crate::app::App;
use crate::app::automation::{AutomationStep, AutomationTarget};
use crate::render_view::scrollbar_widget::ScrollbarGeometry;
use crate::ui_system::UiId;

const FILE: &str = "pgo_scroll/big.rs";
const LINES: usize = 20_000;
const WHEEL_DOWN_BATCHES: usize = 6;
const WHEEL_UP_BATCHES: usize = 3;
const WHEEL_PER_BATCH: usize = 10;
const PAGE_DOWN_PRESSES: usize = 20;
const PAGE_UP_PRESSES: usize = 10;
const DRAG_STEPS: u16 = 40;
/// Lines of the header and of one generated item (see `big_source`).
const PREAMBLE_LINES: usize = 2;
const ITEM_LINES: usize = 15;

/// Deterministic, realistic Rust: a doc comment, a function with a `match`, strings and a
/// trailing comment per item; at least `LINES` lines.
fn big_source() -> String {
    let mut text = String::with_capacity(LINES * 40);
    text.push_str("//! Generated PGO scroll fixture.\n\n");
    let mut item = 0usize;
    while PREAMBLE_LINES + item * ITEM_LINES < LINES {
        let _ = write!(
            text,
            "/// Scores item {item} against the given weight.\n\
             pub fn score_{item}(input: &str, weight: u32) -> u32 {{\n\
             \x20   let mut total = weight + {};\n\
             \x20   for (index, ch) in input.chars().enumerate() {{\n\
             \x20       match ch {{\n\
             \x20           'a'..='m' => total += index as u32,\n\
             \x20           'n'..='z' => total = total.wrapping_mul(3),\n\
             \x20           _ => total ^= {},\n\
             \x20       }}\n\
             \x20   }}\n\
             \x20   // Clamp the result of item {item}.\n\
             \x20   println!(\"item {item}: {{total}}\");\n\
             \x20   total.min(10_000)\n\
             }}\n\n",
            item % 97,
            item % 251,
        );
        item += 1;
    }
    text
}

fn prepare(app: &mut App, workspace: &Path) -> Result<(), String> {
    let dir = workspace.join("pgo_scroll");
    std::fs::create_dir_all(&dir).map_err(|error| format!("create pgo_scroll: {error}"))?;
    let path = workspace.join(FILE);
    std::fs::write(&path, big_source()).map_err(|error| format!("write big.rs: {error}"))?;
    app.open_file_in_tab(path, false);
    Ok(())
}

/// The active tab's path lives in `App::file_path`, not in the tab.
fn big_open(app: &App) -> bool {
    app.file_path.as_deref().is_some_and(|path| path.ends_with(FILE))
        && app.editor.line_offsets.len() >= LINES
}

fn scrollbar(app: &App) -> Option<(ScrollbarGeometry, (f32, f32, f32, f32))> {
    let lane = app.ui_registry.element_hits().find_map(|(id, _, rect, _)| {
        let rect = rect.filter(|_| id == UiId::EditorScrollbarY)?;
        Some((rect.x, rect.y, rect.w, rect.h))
    })?;
    let renderer = app.renderer.as_ref()?;
    let lines = app.editor.get_visible_lines_count();
    let geometry = crate::render_view::editor_vertical_scrollbar(
        lane,
        crate::render_view::editor_scroll_content_height(lines, renderer.line_height, lane.3),
        crate::render_view::editor_max_scroll_for_lines(lines, renderer.line_height, lane.3),
        app.scroll_y.current,
    )
    .geometry(renderer.scale_factor)?;
    Some((geometry, lane))
}

fn scrollbar_drawn(app: &App) -> bool {
    scrollbar(app).is_some()
}

fn at_top(app: &App) -> bool {
    app.scroll_y.current < 1.0 && app.scroll_y.target < 1.0
}

/// Middle of the thumb, where the drag grabs it.
fn thumb_point(app: &App) -> Option<(f32, f32)> {
    let (geometry, _) = scrollbar(app)?;
    let (x, y, w, h) = geometry.thumb_rect;
    Some((x + w / 2.0, y + h / 2.0))
}

/// Bottom edge of the lane; the drag clamps at the end of the track.
fn track_end_point(app: &App) -> Option<(f32, f32)> {
    let (_, (x, y, w, h)) = scrollbar(app)?;
    Some((x + w / 2.0, y + h))
}

fn scrolled_to_bottom(app: &App) -> bool {
    scrollbar(app).is_some_and(|(geometry, _)| {
        geometry.max_scroll > 0.0 && geometry.offset >= geometry.max_scroll - 1.0
    })
}

fn wheel_batches(steps: &mut Vec<AutomationStep>, batches: usize, dy: f32) {
    for _ in 0..batches {
        for _ in 0..WHEEL_PER_BATCH {
            steps.push(AutomationStep::Wheel {
                at: AutomationTarget::Ui(UiId::EditorTextBody),
                dx: 0.0,
                dy,
            });
        }
        // The wheel animates the scroll; let the frames run before the next batch.
        steps.push(AutomationStep::WaitFrames(2));
    }
}

pub(super) fn steps(_workspace: &Path) -> Vec<AutomationStep> {
    use AutomationStep as S;
    let mut steps = vec![
        S::Call { what: "scroll fixture", run: prepare },
        S::WaitUntil { what: "big.rs open", check: big_open, timeout_ms: 20_000 },
        S::WaitUntil { what: "scrollbar drawn", check: scrollbar_drawn, timeout_ms: 10_000 },
        // Earlier groups leave the git message input or a panel focused, which would swallow
        // the paging keys below.
        S::FocusEditor,
    ];
    wheel_batches(&mut steps, WHEEL_DOWN_BATCHES, -3.0);
    wheel_batches(&mut steps, WHEEL_UP_BATCHES, 3.0);
    steps.extend((0..PAGE_DOWN_PRESSES).map(|_| S::Key("pagedown")));
    steps.extend((0..PAGE_UP_PRESSES).map(|_| S::Key("pageup")));
    steps.extend([
        S::Key("ctrl+end"),
        S::WaitFrames(4),
        S::Key("ctrl+home"),
        S::WaitUntil { what: "scrolled to top", check: at_top, timeout_ms: 10_000 },
        S::Drag {
            from: AutomationTarget::Find(thumb_point),
            to: AutomationTarget::Find(track_end_point),
            steps: DRAG_STEPS,
        },
        S::WaitUntil { what: "scrolled to bottom", check: scrolled_to_bottom, timeout_ms: 10_000 },
    ]);
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_is_deterministic_and_long_enough() {
        let text = big_source();
        assert_eq!(text, big_source());
        assert!(text.lines().count() >= LINES);
        assert_eq!(text.lines().count(), PREAMBLE_LINES + (LINES - PREAMBLE_LINES).div_ceil(ITEM_LINES) * ITEM_LINES);
        assert!(text.contains("match ch {"));
        assert!(text.contains("println!(\"item 7: {total}\");"));
    }

    #[test]
    fn steps_hold_the_requested_input_counts() {
        let steps = steps(Path::new("/tmp/pgo-scroll-test"));
        let count = |wanted: fn(&AutomationStep) -> bool| steps.iter().filter(|s| wanted(s)).count();
        assert_eq!(
            count(|s| matches!(s, AutomationStep::Wheel { dy, .. } if *dy < 0.0)),
            WHEEL_DOWN_BATCHES * WHEEL_PER_BATCH
        );
        assert_eq!(
            count(|s| matches!(s, AutomationStep::Wheel { dy, .. } if *dy > 0.0)),
            WHEEL_UP_BATCHES * WHEEL_PER_BATCH
        );
        assert_eq!(count(|s| matches!(s, AutomationStep::Key("pagedown"))), PAGE_DOWN_PRESSES);
        assert_eq!(count(|s| matches!(s, AutomationStep::Key("pageup"))), PAGE_UP_PRESSES);
        assert_eq!(count(|s| matches!(s, AutomationStep::Drag { steps: DRAG_STEPS, .. })), 1);
    }
}
