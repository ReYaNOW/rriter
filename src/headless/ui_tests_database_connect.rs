//! Headless UI coverage for adding and opening PostgreSQL connections.

use crate::app::EditorTabKind;
use crate::headless::tests_support::{
    add_database_connection_through_ui, click_ui, connect_postgres_fixture_through_ui, dump,
    has_ui, run_script, scratch_dir, ui_center, wait_until, workspace_with_explorer,
};
use crate::headless::HeadlessSession;

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;

fn database_session(name: &str) -> (std::path::PathBuf, HeadlessSession) {
    let dir = scratch_dir(name);
    let session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    (dir, session)
}

fn double_click_ui(session: &mut HeadlessSession, id: &str) {
    let (x, y) = ui_center(&dump(session), id);
    let lines = run_script(session, format!("mouse_move {x} {y}\ndblclick\n").as_bytes());
    assert!(lines.iter().all(|line| line == "ok"), "{lines:?}");
}

#[test]
fn headless_database_connection_form_saves_fixture_settings() {
    let (dir, mut session) = database_session("ui-database-save");
    let display_name = format!("Saved PostgreSQL fixture {}", std::process::id());
    let index = add_database_connection_through_ui(&mut session, &display_name, 5432);

    let state = dump(&mut session);
    assert!(has_ui(&state, &format!("DatabaseConnectionRow({index})")), "{state}");
    let connections = &session.app.ide_panel.database.connections;
    assert_eq!(
        connections
            .iter()
            .filter(|connection| connection.config.display_name == display_name)
            .count(),
        1
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_database_fixture_catalog_opens_table_rows_and_columns() {
    let fixture = crate::headless::tests_support::postgres_fixture();
    let (dir, mut session) = database_session("ui-database-catalog");
    let display_name = format!("Catalog PostgreSQL fixture {}", fixture.port);
    let index = connect_postgres_fixture_through_ui(&mut session, &fixture, &display_name);

    let database_index = session.app.ide_panel.database.connections[index]
        .databases
        .iter()
        .position(|database| database.name == fixture.database)
        .expect("fixture database in catalog");
    click_ui(&mut session, &format!("DatabaseArrow({index}, {database_index})"));
    wait_until(&mut session, 5000, "public PostgreSQL table catalog", |session| {
        session.app.ide_panel.database.connections[index].databases[database_index].tables_loaded
    });
    let database = &session.app.ide_panel.database.connections[index].databases[database_index];
    let table_index = database
        .tables
        .iter()
        .position(|table| table.name == "pgo_items")
        .expect("pgo_items in table catalog");
    let table_row = format!("DatabaseTableRow({index}, {database_index}, {table_index})");
    wait_until(&mut session, 5000, "pgo_items table row", |session| {
        has_ui(&dump(session), &table_row)
    });

    double_click_ui(&mut session, &table_row);
    wait_until(&mut session, 5000, "fixture table data and metadata", |session| {
        session.app.tabs.iter().any(|tab| match &tab.kind {
            EditorTabKind::DatabaseTable(meta, state) => {
                meta.table_name == "pgo_items"
                    && state.metadata.is_some()
                    && state.grid.count == Some(80)
                    && !state.grid.chunks.is_empty()
            }
            _ => false,
        })
    });
    let table = session
        .app
        .tabs
        .iter()
        .find_map(|tab| match &tab.kind {
            EditorTabKind::DatabaseTable(meta, state) if meta.table_name == "pgo_items" => {
                Some(state)
            }
            _ => None,
        })
        .unwrap();
    let metadata = table.metadata.as_ref().unwrap();
    assert_eq!(metadata.database_name, fixture.database);
    assert!(metadata.columns.iter().any(|column| column.name == "id"));
    assert!(metadata.columns.iter().any(|column| column.name == "name"));
    assert!(metadata.columns.iter().any(|column| column.name == "active"));
    assert_eq!(table.grid.count, Some(80));
    assert!(table.grid.chunks.values().any(|chunk| !chunk.rows.is_empty()));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_database_closed_port_shows_connection_error() {
    let (dir, mut session) = database_session("ui-database-closed-port");
    let display_name = format!("Closed PostgreSQL port {}", std::process::id());
    let index = add_database_connection_through_ui(&mut session, &display_name, 1);
    click_ui(&mut session, &format!("DatabaseConnectionArrow({index})"));
    wait_until(&mut session, 5000, "closed-port connection error", |session| {
        session
            .app
            .ide_panel
            .database
            .connections
            .get(index)
            .is_some_and(|connection| connection.status == crate::app::database::DatabaseConnectionStatus::Error)
    });

    let state = dump(&mut session);
    let connection = &session.app.ide_panel.database.connections[index];
    assert!(has_ui(&state, &format!("DatabaseConnectionRow({index})")), "{state}");
    assert!(connection.status_message.as_deref().is_some_and(|message| !message.is_empty()));
    assert!(connection.databases.is_empty());
    let _ = std::fs::remove_dir_all(dir);
}
