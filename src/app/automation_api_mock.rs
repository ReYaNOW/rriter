//! `api_mock` PGO group: adds a manual static route, starts the API Mock server on a free
//! loopback port, sends requests to it over a raw socket and stops it again. Python routes
//! and hot updates are not covered (Python needs `uv`, static routes have no UI hot path).

use std::path::Path;

use crate::app::api_mock::server::request_to_mock;
use crate::app::api_mock::types::ApiMockServerStatus;
use crate::app::automation::{AutomationButton, AutomationStep, AutomationTarget};
use crate::app::{App, PanelId};
use crate::ui_system::UiId;

/// Path of the first manual route the panel creates.
const ROUTE_PATH: &str = "/mock-1";
const ROUTE_RESPONSE: &str = r#"{"source":"rriter-pgo"}"#;
const REQUESTS: usize = 5;
/// Log line the server writes for one answered request to `ROUTE_PATH`.
const REQUEST_LOG: &str = "GET /mock-1 -> 200";

fn prepare(app: &mut App, _workspace: &Path) -> Result<(), String> {
    // Port zero asks the OS for an unused port; the panel reports the bound URL.
    app.ide_panel.api.mock.port = 0;
    // The sidebar slot toggles, so an already open API panel must be closed for the click to open it.
    if app.ide_panel.is_open(PanelId::ApiClient) {
        app.ide_panel.toggle(PanelId::ApiClient);
    }
    // `full` leaves the terminal open at the bottom of its 1280x800 window; the bottom panel
    // shortens the side panel, so the manual-route controls would be clipped away.
    if let Some(bottom) = app.ide_panel.open_bottom_panel_id() {
        app.ide_panel.toggle(bottom);
    }
    Ok(())
}

fn send_request(app: &mut App, _workspace: &Path) -> Result<(), String> {
    let url = app
        .ide_panel
        .api
        .mock
        .server_status
        .running_url()
        .ok_or_else(|| "mock server is not running".to_string())?
        .to_string();
    // The worker thread answers on its own; the log line is the completion signal.
    drop(request_to_mock(&url, "GET", ROUTE_PATH, &[], b""));
    Ok(())
}

fn ui_visible(app: &App, id: UiId) -> bool {
    app.ui_registry.element_hits().any(|(hit_id, _, rect, _)| hit_id == id && rect.is_some())
}

/// State of the API panel and its surroundings for a timed-out `api mock` wait: which panels
/// are open, whether the panel scroll lets the controls register, and what the last frame holds.
pub(super) fn diagnostics(app: &App) -> String {
    let scroll = &app.ide_panel.api.panel_scroll;
    let (mut mock_ids, mut total) = (0usize, 0usize);
    for (id, _, _, _) in app.ui_registry.element_hits() {
        total += 1;
        mock_ids += usize::from(format!("{id:?}").starts_with("ApiMock"));
    }
    format!(
        "api_open={} top_open={:?} bottom_open={:?} scroll_current={} scroll_target={} scroll_dragging={} scroll_settled={} import_menu_open={} import_url_open={} import_error={} sidebar_drag={} show_settings={} registry_total={total} registry_api_mock={mock_ids} active_tab={}/{}",
        app.ide_panel.is_open(PanelId::ApiClient),
        app.ide_panel.slots.iter().find(|slot| slot.open && slot.group == crate::app::app_state::PanelGroup::Top).map(|slot| slot.id),
        app.ide_panel.open_bottom_panel_id(),
        scroll.current,
        scroll.target,
        scroll.is_dragging,
        scroll.is_settled(),
        app.ide_panel.api.import_menu_open,
        app.ide_panel.api.import_url_open,
        app.ide_panel.api.import_error.is_some(),
        app.ide_panel.drag.is_some(),
        app.show_settings,
        app.active_tab,
        app.tabs.len(),
    )
}

fn logged_requests(app: &App) -> usize {
    app.ide_panel.api.mock_server_logs.iter().filter(|log| log.text.contains(REQUEST_LOG)).count()
}

fn click(id: UiId) -> AutomationStep {
    AutomationStep::Click {
        at: AutomationTarget::Ui(id),
        button: AutomationButton::Left,
        mods: "",
        clicks: 1,
    }
}

pub(super) fn steps(_workspace: &Path) -> Vec<AutomationStep> {
    use AutomationStep as S;
    let mut steps = vec![
        S::Call { what: "api mock prepare", run: prepare },
        click(UiId::SidebarSlot(PanelId::ApiClient)),
        S::WaitUntil {
            what: "api mock controls",
            check: |app| ui_visible(app, UiId::ApiMockAddManualRoute),
            timeout_ms: 5_000,
        },
        click(UiId::ApiMockAddManualRoute),
        S::WaitUntil {
            what: "manual route input",
            check: |app| ui_visible(app, UiId::ApiMockStaticResponseInput(0)),
            timeout_ms: 5_000,
        },
        click(UiId::ApiMockStaticResponseInput(0)),
        S::WaitFrames(2),
        S::Key("ctrl+a"),
        S::TypeText(ROUTE_RESPONSE),
        click(UiId::ApiMockServerToggle),
        S::WaitUntil {
            what: "mock server running",
            check: |app| app.ide_panel.api.mock.server_status.running_url().is_some(),
            timeout_ms: 10_000,
        },
        S::Call { what: "mock request", run: send_request },
        S::WaitUntil {
            what: "mock request logged",
            check: |app| logged_requests(app) >= 1,
            timeout_ms: 10_000,
        },
    ];
    steps.extend((1..REQUESTS).map(|_| S::Call { what: "mock request", run: send_request }));
    steps.extend([
        S::WaitUntil {
            what: "mock requests logged",
            check: |app| logged_requests(app) >= REQUESTS,
            timeout_ms: 10_000,
        },
        click(UiId::ApiMockServerToggle),
        S::WaitUntil {
            what: "mock server stopped",
            check: |app| matches!(app.ide_panel.api.mock.server_status, ApiMockServerStatus::Stopped),
            timeout_ms: 10_000,
        },
    ]);
    steps
}
