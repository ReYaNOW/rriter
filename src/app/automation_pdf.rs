//! `pdf` PGO group: opens a generated 3-page PDF and drives the viewer (raster, scroll,
//! paging, dark-page toggle, search).

use std::path::Path;

use crate::app::App;
use crate::app::automation::{AutomationButton, AutomationStep, AutomationTarget};
use crate::app::pdf_tab::PdfPhase;
use crate::ui_system::UiId;

const WHEEL_STEPS: usize = 20;
/// Frames the viewer gets to re-rasterize after a dark-page toggle.
const RERASTER_FRAMES: u16 = 20;

fn write_and_open(app: &mut App, workspace: &Path) -> Result<(), String> {
    let dir = workspace.join("pgo_pdf");
    std::fs::create_dir_all(&dir).map_err(|error| format!("create pgo_pdf: {error}"))?;
    let path = crate::pdf::fixture::write_fixture_pdf(&dir);
    if !path.is_file() {
        return Err(format!("fixture pdf was not written: {}", path.display()));
    }
    app.open_file_in_tab(path, false);
    Ok(())
}

fn rasterized(app: &App) -> bool {
    app.active_pdf_tab()
        .is_some_and(|pdf| matches!(pdf.phase, PdfPhase::Ready) && !pdf.textures.is_empty())
}

fn search_found_both_matches(app: &App) -> bool {
    app.active_pdf_tab()
        .is_some_and(|pdf| pdf.search.done && pdf.search.matches.len() == 2)
}

pub(super) fn steps(_workspace: &Path) -> Vec<AutomationStep> {
    use AutomationStep as S;
    let mut steps = vec![
        S::Call { what: "pdf open", run: write_and_open },
        S::WaitUntil { what: "pdf rasterized", check: rasterized, timeout_ms: 20_000 },
    ];
    // A negative wheel delta scrolls the document down.
    steps.extend((0..WHEEL_STEPS).map(|_| S::Wheel {
        at: AutomationTarget::Ui(UiId::PdfBody),
        dx: 0.0,
        dy: -3.0,
    }));
    steps.extend([S::Key("pagedown"), S::Key("pagedown"), S::Key("pagedown"), S::Key("end"), S::Key("home")]);
    for _ in 0..2 {
        steps.push(S::Click {
            at: AutomationTarget::Ui(UiId::PdfDarkToggle),
            button: AutomationButton::Left,
            mods: "",
            clicks: 1,
        });
        steps.push(S::WaitFrames(RERASTER_FRAMES));
    }
    steps.extend([
        S::WaitUntil { what: "pdf rasterized after toggle", check: rasterized, timeout_ms: 10_000 },
        S::Key("ctrl+f"),
        S::TypeText("second"),
        S::WaitUntil { what: "pdf search done", check: search_found_both_matches, timeout_ms: 10_000 },
    ]);
    steps
}
