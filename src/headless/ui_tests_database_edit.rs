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

#[test]
fn headless_database_table_cell_edit_saves_and_reloads() {
    let fixture = crate::headless::tests_support::postgres_fixture();
    let dir = scratch_dir("ui-database-cell-edit");
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    let display_name = format!("Database table fixture {}", fixture.port);
    let connection_index =
        connect_postgres_fixture_through_ui(&mut session, &fixture, &display_name);
    let database_index = session.app.ide_panel.database.connections[connection_index]
        .databases
        .iter()
        .position(|database| database.name == fixture.database)
        .expect("fixture database in catalog");
    click_ui(&mut session, &format!("DatabaseArrow({connection_index}, {database_index})"));
    wait_until(&mut session, 5000, "fixture table catalog", |session| {
        session.app.ide_panel.database.connections[connection_index].databases[database_index]
            .tables_loaded
    });
    let table_index = session.app.ide_panel.database.connections[connection_index]
        .databases[database_index]
        .tables
        .iter()
        .position(|table| table.name == "pgo_items")
        .expect("fixture table in catalog");
    let table_row = format!("DatabaseTableRow({connection_index}, {database_index}, {table_index})");
    let (x, y) = ui_center(&dump(&mut session), &table_row);
    let lines = run_script(&mut session, format!("mouse_move {x} {y}\ndblclick\n").as_bytes());
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    wait_until(&mut session, 5000, "fixture table rows", |session| {
        active_table_state(session).is_some_and(|state| {
            state.metadata.is_some() && state.grid.row(0).is_some()
        })
    });

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
