//! Headless coverage for Database SQL results and table editing.

use crate::app::EditorTabKind;
use crate::headless::tests_support::{
    PostgresFixture, click_ui, connect_postgres_fixture_through_ui, dump, has_ui, run_script,
    scratch_dir, ui_center, wait_until, workspace_with_explorer,
};
use crate::headless::HeadlessSession;

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;

fn database_query_session(name: &str) -> (std::path::PathBuf, PostgresFixture, HeadlessSession) {
    let fixture = crate::headless::tests_support::postgres_fixture();
    let dir = scratch_dir(name);
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    let display_name = format!("Database edit fixture {}", fixture.port);
    let connection_index =
        connect_postgres_fixture_through_ui(&mut session, &fixture, &display_name);
    let database_index = session.app.ide_panel.database.connections[connection_index]
        .databases
        .iter()
        .position(|database| database.name == fixture.database)
        .unwrap_or_else(|| panic!("fixture database in catalog"));
    let database_row = format!("DatabaseRow({connection_index}, {database_index})");
    wait_until(&mut session, 5000, "fixture database row", |session| {
        has_ui(&dump(session), &database_row)
    });
    let (x, y) = ui_center(&dump(&mut session), &database_row);
    let lines = run_script(
        &mut session,
        format!("mouse_move {x} {y}\nclick right\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    wait_until(&mut session, 3000, "database context menu", |session| {
        has_ui(&dump(session), "DatabaseContextItem(0)")
    });
    click_ui(&mut session, "DatabaseContextItem(0)");
    wait_until(&mut session, 5000, "database SQL console", |session| {
        active_query_state(session).is_some() && has_ui(&dump(session), "DatabaseQueryRun")
    });
    (dir, fixture, session)
}

fn active_query_state(
    session: &HeadlessSession,
) -> Option<&crate::app::database::DatabaseQueryTabState> {
    session.app.tabs.get(session.app.active_tab).and_then(|tab| match &tab.kind {
        EditorTabKind::DatabaseQuery(_, state) => Some(state),
        _ => None,
    })
}

fn execute_query(session: &mut HeadlessSession, sql: &str) {
    let (x, y) = ui_center(&dump(session), "EditorTextBody");
    let lines = run_script(
        session,
        format!("mouse_move {x} {y}\nclick\nkey ctrl+a\ntype {sql}\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    wait_until(session, 10000, "idle database worker", |session| {
        session.app.ide_panel.database.pending_job.is_none()
    });
    click_ui(session, "DatabaseQueryRun");
    wait_until(session, 10000, "query result", |session| {
        active_query_state(session).is_some_and(|state| !state.running && !state.results.is_empty())
    });
}

fn commit_query(session: &mut HeadlessSession) {
    if active_query_state(session).is_some_and(|state| state.review.is_some()) {
        click_ui(session, "DatabaseQueryCommit");
        wait_until(session, 10000, "query transaction commit", |session| {
            active_query_state(session).is_some_and(|state| state.review.is_none())
                && session.app.ide_panel.database.pending_job.is_none()
        });
    }
}

#[test]
fn headless_database_query_update_returning_renders_returned_rows() {
    let (dir, _fixture, mut session) = database_query_session("ui-database-update-returning");
    execute_query(
        &mut session,
        "UPDATE public.pgo_items SET name = 'query-returned' WHERE id = 1 RETURNING id, name, active;",
    );

    let result = &active_query_state(&session).unwrap().results[0];
    assert_eq!(
        result.columns,
        vec![String::from("id"), String::from("name"), String::from("active")]
    );
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0][0].value.as_deref(), Some("1"));
    assert_eq!(result.rows[0][1].value.as_deref(), Some("query-returned"));
    assert_eq!(result.affected_rows, 1);
    assert!(has_ui(&dump(&mut session), "DatabaseQueryResultBody"));
    commit_query(&mut session);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_database_query_ddl_success_keeps_console_usable() {
    let (dir, _fixture, mut session) = database_query_session("ui-database-ddl-success");
    execute_query(&mut session, "CREATE TABLE public.ddl_probe (id integer);");

    let query = active_query_state(&session).unwrap();
    assert!(query.error.is_none(), "DDL returned an error: {query:?}");
    assert_eq!(query.results[0].command_kind, "CREATE");
    assert!(has_ui(&dump(&mut session), "DatabaseQueryResultBody"));
    commit_query(&mut session);

    execute_query(
        &mut session,
        "SELECT id, name, active FROM public.pgo_items ORDER BY id LIMIT 1;",
    );
    let query = active_query_state(&session).unwrap();
    assert!(query.error.is_none(), "console retained DDL error: {query:?}");
    assert_eq!(query.results[0].rows.len(), 1);
    assert!(has_ui(&dump(&mut session), "DatabaseQueryRun"));
    let _ = std::fs::remove_dir_all(dir);
}

fn active_table_state(
    session: &HeadlessSession,
) -> Option<&crate::app::database::DatabaseTableTabState> {
    session.app.tabs.get(session.app.active_tab).and_then(|tab| match &tab.kind {
        EditorTabKind::DatabaseTable(_, state) => Some(state),
        _ => None,
    })
}

/// Connects to `fixture` through the UI and opens `pgo_items` in a table tab with rows loaded.
fn open_fixture_table(session: &mut HeadlessSession, fixture: &PostgresFixture) {
    let display_name = format!("Database table fixture {}", fixture.port);
    let connection_index = connect_postgres_fixture_through_ui(session, fixture, &display_name);
    let database_index = session.app.ide_panel.database.connections[connection_index]
        .databases
        .iter()
        .position(|database| database.name == fixture.database)
        .unwrap_or_else(|| panic!("fixture database in catalog"));
    click_ui(session, &format!("DatabaseArrow({connection_index}, {database_index})"));
    wait_until(session, 5000, "fixture table catalog", |session| {
        session.app.ide_panel.database.connections[connection_index].databases[database_index]
            .tables_loaded
    });
    let table_index = session.app.ide_panel.database.connections[connection_index]
        .databases[database_index]
        .tables
        .iter()
        .position(|table| table.name == "pgo_items")
        .unwrap_or_else(|| panic!("fixture table in catalog"));
    let table_row = format!("DatabaseTableRow({connection_index}, {database_index}, {table_index})");
    let (x, y) = ui_center(&dump(session), &table_row);
    let lines = run_script(session, format!("mouse_move {x} {y}\ndblclick\n").as_bytes());
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    wait_until(session, 5000, "fixture table rows", |session| {
        active_table_state(session).is_some_and(|state| {
            state.metadata.is_some() && state.grid.row(0).is_some()
        })
    });
}

/// Strict loop model: a refresh slower than `DATABASE_REFRESH_INDICATOR_DELAY` must draw
/// its indicator with no input, so the event loop has to arm a wake-up for that moment.
#[test]
fn headless_database_table_slow_refresh_draws_indicator_without_input() {
    const CHUNK_DELAY_MS: u64 = 1300;
    let fixture = crate::headless::tests_support::postgres_fixture_with_args(&[
        "--table-chunk-delay-ms",
        &CHUNK_DELAY_MS.to_string(),
    ]);
    let dir = scratch_dir("ui-database-slow-refresh");
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    open_fixture_table(&mut session, &fixture);
    wait_until(&mut session, 10000, "idle database worker", |session| {
        session.app.ide_panel.database.pending_job.is_none()
    });

    click_ui(&mut session, "DatabaseTableRefresh");
    let started = active_table_state(&session)
        .and_then(|state| state.grid.refresh_started.filter(|_| state.grid.refreshing))
        .unwrap_or_else(|| panic!("refresh did not start: {}", dump(&mut session)));
    let delay = crate::app::database::DATABASE_REFRESH_INDICATOR_DELAY;
    // Only the native loop's own wake-ups run from here: no input, no driver schedule.
    let mut causes = std::collections::BTreeMap::<&str, u32>::new();
    let mut indicator_frame = false;
    let due = started + delay;
    let budget = std::time::Instant::now() + std::time::Duration::from_millis(CHUNK_DELAY_MS + 3000);
    while active_table_state(&session).is_some_and(|state| state.grid.refreshing)
        && !indicator_frame
    {
        let left = budget.saturating_duration_since(std::time::Instant::now());
        assert!(!left.is_zero(), "refresh did not finish; wake-ups: {causes:?}");
        let wake = session.native_wake(left);
        *causes.entry(wake.cause.name()).or_default() += 1;
        // The frame renders after `stepped_at`; `refreshing` only changes inside that pass.
        let refreshing = active_table_state(&session).is_some_and(|state| state.grid.refreshing);
        indicator_frame = wake.frame
            && refreshing
            && wake.stepped_at.is_some_and(|at| at >= due);
        if !refreshing || indicator_frame || wake.frame {
            continue;
        }
        // The loop goes to sleep while the indicator is still due: a wake-up must be armed
        // for the moment it is due (`deadline_ms` is rounded down, hence the 1 ms).
        let event_loop = dump(&mut session)["event_loop"].clone();
        let now = std::time::Instant::now();
        let wakes_in_time = match event_loop["control_flow"].as_str() {
            Some("poll") => true,
            Some("wait_until") => event_loop["deadline_ms"].as_u64().is_some_and(|ms| {
                now + std::time::Duration::from_millis(ms) <= due + std::time::Duration::from_millis(1)
            }),
            _ => false,
        };
        assert!(
            wakes_in_time,
            "the loop sleeps without a wake-up at the indicator delay {delay:?} \
             ({:?} after the refresh started): {event_loop}; wake-ups: {causes:?}",
            now.duration_since(started)
        );
    }
    assert!(
        indicator_frame,
        "no frame drew the refresh indicator between {delay:?} and the result; wake-ups: {causes:?}"
    );
    let idle = run_script(&mut session, b"idle 500\n");
    let frames = idle
        .first()
        .and_then(|line| line.strip_prefix("ok frames="))
        .and_then(|line| line.split_whitespace().next())
        .and_then(|frames| frames.parse::<u32>().ok())
        .unwrap_or_else(|| panic!("invalid idle result: {idle:?}"));
    // Five 100 ms spinner steps plus one frame per delayed row chunk that
    // arrives meanwhile; a redraw every frame would give 30+ at 60 Hz.
    assert!(
        frames <= 14,
        "refresh indicator requested too many frames in 500ms: {frames}; idle result: {idle:?}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_database_table_cell_edit_saves_and_reloads() {
    let fixture = crate::headless::tests_support::postgres_fixture();
    let dir = scratch_dir("ui-database-cell-edit");
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    open_fixture_table(&mut session, &fixture);

    let id_cell = "DatabaseTableCell(0, 0)";
    let (x, y) = ui_center(&dump(&mut session), id_cell);
    let _ = run_script(&mut session, format!("mouse_move {x} {y}\ndblclick\n").as_bytes());
    assert!(active_table_state(&session)
        .unwrap()
        .error
        .as_deref()
        .is_some_and(|error| error.contains("primary key")));

    let name_cell = "DatabaseTableCell(0, 1)";
    let (x, y) = ui_center(&dump(&mut session), name_cell);
    let lines = run_script(
        &mut session,
        format!("mouse_move {x} {y}\ndblclick\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    wait_until(&mut session, 3000, "cell editor", |session| {
        has_ui(&dump(session), "DatabaseTableCellEditor")
    });
    let (x, y) = ui_center(&dump(&mut session), "DatabaseTableCellEditor");
    let lines = run_script(
        &mut session,
        format!("mouse_move {x} {y}\nclick\nkey ctrl+a\ntype edited-through-grid\nkey enter\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    assert!(active_table_state(&session).unwrap().grid.row(0).unwrap().is_dirty());
    wait_until(&mut session, 10000, "idle database worker", |session| {
        session.app.ide_panel.database.pending_job.is_none()
    });
    click_ui(&mut session, "DatabaseTableSave");
    wait_until(&mut session, 10000, "table transaction review", |session| {
        matches!(
            session.app.ide_panel.database.table_modal.as_ref(),
            Some(crate::app::database::DatabaseTableModal::Review { .. })
        )
            && has_ui(&dump(session), "DatabaseTableModalPrimary")
    });
    click_ui(&mut session, "DatabaseTableModalPrimary");
    wait_until(&mut session, 10000, "table commit and reload", |session| {
        active_table_state(session).is_some_and(|state| {
            !state.loading
                && !state.grid.refreshing
                && !state.grid.post_commit_refresh_pending
                && state.grid.row(0).is_some_and(|row| {
                    row.cells[1].value.copy_text() == "edited-through-grid"
                })
        })
    });
    assert_eq!(
        active_table_state(&session)
            .unwrap()
            .grid
            .row(0)
            .unwrap()
            .cells[1]
            .value
            .copy_text(),
        "edited-through-grid"
    );
    let _ = std::fs::remove_dir_all(dir);
}
