//! Headless SQL-console coverage against the deterministic PostgreSQL fixture.

use crate::app::database::DatabaseQueryTabState;
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
    let display_name = format!("SQL fixture {}", fixture.port);
    let connection_index =
        connect_postgres_fixture_through_ui(&mut session, &fixture, &display_name);

    let database_index = session.app.ide_panel.database.connections[connection_index]
        .databases
        .iter()
        .position(|database| database.name == fixture.database)
        .expect("fixture database in catalog");
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
    wait_until(
        &mut session,
        3000,
        "database SQL-console context menu",
        |session| has_ui(&dump(session), "DatabaseContextItem(0)"),
    );
    click_ui(&mut session, "DatabaseContextItem(0)");
    wait_until(
        &mut session,
        5000,
        "active database SQL console",
        |session| {
            active_query_state(session).is_some()
                && has_ui(&dump(session), "DatabaseQueryRun")
        },
    );

    (dir, fixture, session)
}

fn active_query_state(session: &HeadlessSession) -> Option<&DatabaseQueryTabState> {
    session
        .app
        .tabs
        .get(session.app.active_tab)
        .and_then(|tab| match &tab.kind {
            EditorTabKind::DatabaseQuery(_, state) => Some(state),
            _ => None,
        })
}

fn execute_query(session: &mut HeadlessSession, sql: &str, explain: bool) {
    let state = dump(session);
    let (x, y) = ui_center(&state, "EditorTextBody");
    let lines = run_script(
        session,
        format!("mouse_move {x} {y}\nclick\nkey ctrl+a\ntype {sql}\n").as_bytes(),
    );
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
    // Run refuses to start while another database job (e.g. the console's completion
    // metadata load) is in flight.
    wait_until(session, 10000, "idle database worker", |session| {
        session.app.ide_panel.database.pending_job.is_none()
    });
    click_ui(
        session,
        if explain {
            "DatabaseQueryExplain"
        } else {
            "DatabaseQueryRun"
        },
    );
    let started = active_query_state(session).is_some_and(|state| {
        state.running || state.error.is_some() || !state.results.is_empty()
    });
    assert!(
        started,
        "SQL console did not start the query: global_error={:?}, state={:?}",
        session.app.ide_panel.database.global_error,
        active_query_state(session)
    );
    wait_until(
        session,
        10000,
        "database SQL query completion",
        |session| {
            active_query_state(session).is_some_and(|state| {
                !state.running && (state.error.is_some() || !state.results.is_empty())
            })
        },
    );
}

#[test]
fn headless_database_query_select_renders_result_grid() {
    let (dir, _fixture, mut session) = database_query_session("ui-database-query-select");
    execute_query(
        &mut session,
        "SELECT id, name, active FROM public.pgo_items ORDER BY id LIMIT 3;",
        false,
    );

    let result = &active_query_state(&session).unwrap().results[0];
    assert_eq!(
        result.columns,
        vec![String::from("id"), String::from("name"), String::from("active")]
    );
    assert_eq!(result.rows.len(), 3);
    assert_eq!(result.rows[0][0].value.as_deref(), Some("1"));
    let state = dump(&mut session);
    assert!(has_ui(&state, "DatabaseQueryResultBody"), "{state}");
    assert!(
        has_ui(&state, "DatabaseQueryRun"),
        "query toolbar missing: {state}"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_database_query_explain_renders_plan() {
    let (dir, _fixture, mut session) = database_query_session("ui-database-query-explain");
    execute_query(
        &mut session,
        "SELECT id, name, active FROM public.pgo_items ORDER BY id LIMIT 3;",
        true,
    );

    let result = &active_query_state(&session).unwrap().results[0];
    assert_eq!(result.columns, vec![String::from("QUERY PLAN")]);
    assert!(
        result.rows.iter().any(|row| {
            row.first()
                .and_then(|cell| cell.value.as_deref())
                .is_some_and(|line| line.contains("pgo_items"))
        }),
        "EXPLAIN plan did not mention pgo_items: {result:?}"
    );
    assert!(has_ui(&dump(&mut session), "DatabaseQueryResultBody"));

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_database_query_ddl_error_keeps_console_usable() {
    let (dir, _fixture, mut session) = database_query_session("ui-database-query-ddl-error");
    execute_query(&mut session, "CREATE TABLE public.unsupported_probe (id integer);", false);

    let error = active_query_state(&session)
        .unwrap()
        .error
        .as_deref()
        .expect("DDL fixture error should be visible in the query state");
    assert!(error.contains("unsupported"), "unexpected DDL error: {error}");
    let error_ui = dump(&mut session);
    assert!(has_ui(&error_ui, "DatabaseQueryRun"), "{error_ui}");
    assert!(has_ui(&error_ui, "DatabaseQueryResultBody"), "{error_ui}");

    execute_query(
        &mut session,
        "SELECT id, name, active FROM public.pgo_items ORDER BY id LIMIT 1;",
        false,
    );
    let query = active_query_state(&session).unwrap();
    assert!(
        query.error.is_none(),
        "query error remained after successful SELECT: {query:?}"
    );
    assert_eq!(query.results[0].rows.len(), 1);
    assert!(has_ui(&dump(&mut session), "DatabaseQueryResultBody"));

    let _ = std::fs::remove_dir_all(dir);
}
