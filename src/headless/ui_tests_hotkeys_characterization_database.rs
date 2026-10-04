//! Characterization of Database hotkeys before keymap changes.

use crate::app::EditorTabKind;
use crate::headless::tests_support::{
    PostgresFixture, click_ui, connect_postgres_fixture_through_ui, dump, has_ui,
    open_terminal_with_alt_q, panel_open, run_script, scratch_dir, ui_center, wait_until,
    workspace_with_explorer,
};
use crate::headless::HeadlessSession;

use super::ui_tests_hotkeys_characterization::assert_chord_effect;

const EXCLUDED_SECTION_12_CELLS: &[(&str, &str)] = &[
    (
        "SQL query hotkeys with terminal focus",
        "No UI path was found that preserves terminal focus while a Database query tab is active.",
    ),
    (
        "Database table NumpadEnter",
        "The headless key protocol has no NumpadEnter token, so this physical-key branch cannot be synthesized.",
    ),
];

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;

pub(super) fn run_ok(session: &mut HeadlessSession, script: &str) {
    let lines = run_script(session, script.as_bytes());
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
}

#[test]
fn headless_hotkeys_database_excluded_rows_have_reasons() {
    for (row, reason) in EXCLUDED_SECTION_12_CELLS {
        assert!(!row.is_empty() && !reason.is_empty());
    }
}

pub(super) fn active_query_state(
    session: &HeadlessSession,
) -> Option<&crate::app::database::DatabaseQueryTabState> {
    session.app.tabs.get(session.app.active_tab).and_then(|tab| match &tab.kind {
        EditorTabKind::DatabaseQuery(_, state) => Some(state),
        _ => None,
    })
}

pub(super) fn active_table_state(
    session: &HeadlessSession,
) -> Option<&crate::app::database::DatabaseTableTabState> {
    session.app.tabs.get(session.app.active_tab).and_then(|tab| match &tab.kind {
        EditorTabKind::DatabaseTable(_, state) => Some(state),
        _ => None,
    })
}

pub(super) fn database_query_session(
    name: &str,
) -> (std::path::PathBuf, PostgresFixture, HeadlessSession) {
    let fixture = crate::headless::tests_support::postgres_fixture();
    let dir = scratch_dir(name);
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    let display_name = format!("Database hotkey fixture {}", fixture.port);
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
    run_ok(&mut session, &format!("mouse_move {x} {y}\nclick right\n"));
    wait_until(&mut session, 3000, "database context menu", |session| {
        has_ui(&dump(session), "DatabaseContextItem(0)")
    });
    click_ui(&mut session, "DatabaseContextItem(0)");
    wait_until(&mut session, 5000, "database SQL console", |session| {
        active_query_state(session).is_some() && has_ui(&dump(session), "DatabaseQueryRun")
    });
    (dir, fixture, session)
}

pub(super) fn open_fixture_table(session: &mut HeadlessSession, fixture: &PostgresFixture) {
    let display_name = format!("Database table hotkey fixture {}", fixture.port);
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
    run_ok(session, &format!("mouse_move {x} {y}\ndblclick\n"));
    wait_until(session, 5000, "fixture table rows", |session| {
        active_table_state(session)
            .is_some_and(|state| state.metadata.is_some() && state.grid.row(0).is_some())
    });
}

#[test]
fn headless_hotkeys_characterize_database_query_run_and_completion() {
    let (dir, _fixture, mut session) = database_query_session("ui-hotkeys-database-query");
    wait_until(&mut session, 10000, "SQL completion metadata", |session| {
        active_query_state(session).is_some_and(|state| state.completion_loaded)
            && session.app.ide_panel.database.pending_job.is_none()
    });
    let sql = "SELECT id, name, active FROM public.pgo_items ORDER BY id LIMIT 1;";
    let (x, y) = ui_center(&dump(&mut session), "EditorTextBody");
    run_ok(
        &mut session,
        &format!("mouse_move {x} {y}\nclick\ntype {sql}\n"),
    );
    wait_until(&mut session, 10000, "idle database worker", |session| {
        session.app.ide_panel.database.pending_job.is_none()
    });
    assert_chord_effect(
        &mut session,
        "ctrl+enter",
        |_| {},
        |session| {
            wait_until(session, 10000, "Ctrl+Enter SQL result", |session| {
                active_query_state(session)
                    .is_some_and(|state| {
                        !state.running && (state.error.is_some() || !state.results.is_empty())
                    })
            });
            let state = active_query_state(session).unwrap();
            assert!(state.error.is_none(), "query failed: {:?}", state.error);
            assert_eq!(state.results[0].rows.len(), 1);
        },
    );
    wait_until(&mut session, 10000, "idle database worker", |session| {
        session.app.ide_panel.database.pending_job.is_none()
    });
    let (x, y) = ui_center(&dump(&mut session), "EditorTextBody");
    run_ok(
        &mut session,
        &format!(
            "mouse_move {x} {y}\nclick\nkey ctrl+a\ntype SELECT id, name, active FROM public.pgo_items ORDER BY id LIMIT 2;\n"
        ),
    );
    assert_chord_effect(
        &mut session,
        "ctrl+enter",
        |_| {},
        |session| {
            wait_until(session, 10000, "second Ctrl+Enter SQL result", |session| {
                active_query_state(session).is_some_and(|state| {
                    !state.running
                        && (state.error.is_some()
                            || state.results.first().is_some_and(|result| result.rows.len() == 2))
                })
            });
            let state = active_query_state(session).unwrap();
            assert!(state.error.is_none(), "query failed: {:?}", state.error);
            assert_eq!(state.results[0].rows.len(), 2);
        },
    );

    let (x, y) = ui_center(&dump(&mut session), "EditorTextBody");
    run_ok(
        &mut session,
        &format!(
            "mouse_move {x} {y}\nclick\nkey ctrl+a\ntype SELECT * FROM public.pgo_items WHERE i\n"
        ),
    );
    wait_until(&mut session, 10000, "SQL completion metadata", |session| {
        active_query_state(session).is_some_and(|state| state.completion_loaded)
    });
    assert_chord_effect(
        &mut session,
        "ctrl+space",
        |_| {},
        |session| {
            wait_until(session, 5000, "SQL completion popup", |session| {
                session.app.autocomplete_active && !session.app.autocomplete_options.is_empty()
            });
        },
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_characterize_database_table_cell_filter_and_terminal_focus() {
    let fixture = crate::headless::tests_support::postgres_fixture();
    let dir = scratch_dir("ui-hotkeys-database-table");
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    open_fixture_table(&mut session, &fixture);

    click_ui(&mut session, "DatabaseTableWhereInput");
    run_ok(&mut session, "type i\nkey ctrl+space\n");
    wait_until(&mut session, 5000, "table filter completion", |session| {
        session.app.autocomplete_active && !session.app.autocomplete_options.is_empty()
    });
    run_ok(&mut session, "key escape\n");

    let name_cell = "DatabaseTableCell(0, 1)";
    let original = active_table_state(&session)
        .unwrap()
        .grid
        .row(0)
        .unwrap()
        .cells[1]
        .value
        .copy_text();
    open_terminal_with_alt_q(&mut session);
    assert!(panel_open(&dump(&mut session), "terminal"));
    assert!(session.app.ide_panel.terminal_focused);
    let (x, y) = ui_center(&dump(&mut session), name_cell);
    run_ok(&mut session, &format!("mouse_move {x} {y}\nclick\n"));
    click_ui(&mut session, "TerminalBody");
    assert!(session.app.ide_panel.terminal_focused);
    assert_chord_effect(
        &mut session,
        "ctrl+c",
        |_| {},
        |session| {
            assert!(
                dump(session)["clipboard"]["text"]
                    .as_str()
                    .is_some_and(|text| text.contains(&original)),
                "Ctrl+C should copy the selected database table value"
            );
            assert!(session.app.ide_panel.terminal_focused);
        },
    );

    let (x, y) = ui_center(&dump(&mut session), name_cell);
    run_ok(&mut session, &format!("mouse_move {x} {y}\ndblclick\n"));
    wait_until(&mut session, 3000, "database table cell editor", |session| {
        has_ui(&dump(session), "DatabaseTableCellEditor")
    });
    let (x, y) = ui_center(&dump(&mut session), "DatabaseTableCellEditor");
    run_ok(
        &mut session,
        &format!(
            "mouse_move {x} {y}\nclick\nkey ctrl+a\ntype plain-enter-probe\nkey enter\n"
        ),
    );
    assert_eq!(
        active_table_state(&session).unwrap().grid.row(0).unwrap().cells[1]
            .value
            .copy_text(),
        "plain-enter-probe"
    );
    let (x, y) = ui_center(&dump(&mut session), name_cell);
    run_ok(&mut session, &format!("mouse_move {x} {y}\ndblclick\n"));
    wait_until(&mut session, 3000, "database table cell editor", |session| {
        has_ui(&dump(session), "DatabaseTableCellEditor")
    });
    let (x, y) = ui_center(&dump(&mut session), "DatabaseTableCellEditor");
    run_ok(
        &mut session,
        &format!("mouse_move {x} {y}\nclick\nkey ctrl+a\ntype <NULL>\nkey enter\n"),
    );
    // today: <NULL> refused for NOT NULL column
    assert_eq!(
        active_table_state(&session).unwrap().grid.row(0).unwrap().cells[1]
            .value
            .copy_text(),
        "plain-enter-probe"
    );
    let (x, y) = ui_center(&dump(&mut session), name_cell);
    run_ok(&mut session, &format!("mouse_move {x} {y}\ndblclick\n"));
    wait_until(&mut session, 3000, "database table cell editor", |session| {
        has_ui(&dump(session), "DatabaseTableCellEditor")
    });
    let (x, y) = ui_center(&dump(&mut session), "DatabaseTableCellEditor");
    run_ok(
        &mut session,
        &format!("mouse_move {x} {y}\nclick\nkey ctrl+a\ntype <NULL>\n"),
    );
    click_ui(&mut session, "TerminalBody");
    assert!(session.app.ide_panel.terminal_focused);
    assert_chord_effect(
        &mut session,
        "ctrl+enter",
        |_| {},
        |session| {
            let cell = &active_table_state(session).unwrap().grid.row(0).unwrap().cells[1];
            assert_eq!(cell.value.copy_text(), "<NULL>");
            assert!(cell.dirty);
            assert!(session.app.ide_panel.terminal_focused);
        },
    );

    let (x, y) = ui_center(&dump(&mut session), name_cell);
    run_ok(&mut session, &format!("mouse_move {x} {y}\nclick\n"));
    click_ui(&mut session, "TerminalBody");
    assert!(session.app.ide_panel.terminal_focused);
    assert_chord_effect(
        &mut session,
        "ctrl+z",
        |_| {},
        |session| {
            assert_eq!(
                active_table_state(session)
                    .unwrap()
                    .grid
                    .row(0)
                    .unwrap()
                    .cells[1]
                    .value
                    .copy_text(),
                original
            );
            assert!(session.app.ide_panel.terminal_focused);
        },
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_characterize_database_table_insert_delete_and_filter_focus_guards() {
    let fixture = crate::headless::tests_support::postgres_fixture();
    let dir = scratch_dir("ui-hotkeys-database-row-ops");
    let mut session = workspace_with_explorer(TEST_WIDTH, TEST_HEIGHT, TEST_SCALE, &dir);
    open_fixture_table(&mut session, &fixture);

    let state = active_table_state(&session).unwrap();
    assert!(state.grid.added_rows.is_empty());
    assert_eq!(
        state.grid.row(0).unwrap().state,
        crate::app::database::DatabaseRowState::Clean
    );
    assert_chord_effect(
        &mut session,
        "insert",
        |_| {},
        |session| {
            let state = active_table_state(session).unwrap();
            assert_eq!(state.grid.added_rows.len(), 1);
            assert_eq!(
                state.grid.added_rows[0].state,
                crate::app::database::DatabaseRowState::Added
            );
        },
    );
    let (x, y) = ui_center(&dump(&mut session), "DatabaseTableCell(0, 1)");
    run_ok(&mut session, &format!("mouse_move {x} {y}\ndblclick\n"));
    wait_until(&mut session, 3000, "database table cell editor", |session| {
        has_ui(&dump(session), "DatabaseTableCellEditor")
    });
    let (x, y) = ui_center(&dump(&mut session), "DatabaseTableCellEditor");
    run_ok(
        &mut session,
        &format!("mouse_move {x} {y}\nclick\nkey ctrl+a\ntype focus-guard-probe\nkey enter\n"),
    );
    click_ui(&mut session, "DatabaseTableWhereInput");
    run_ok(&mut session, "type filter-probe\nkey ctrl+a\nkey ctrl+c\nkey ctrl+z\n");
    assert_eq!(dump(&mut session)["clipboard"]["text"], "filter-probe");
    let row = active_table_state(&session).unwrap().grid.row(0).unwrap();
    assert!(row.cells[1].dirty);
    assert_eq!(row.cells[1].value.copy_text(), "focus-guard-probe");

    let (x, y) = ui_center(&dump(&mut session), "DatabaseTableCell(0, 0)");
    run_ok(&mut session, &format!("mouse_move {x} {y}\nclick\nkey delete\n"));
    assert_eq!(
        active_table_state(&session).unwrap().grid.row(0).unwrap().state,
        crate::app::database::DatabaseRowState::Deleted
    );

    let _ = std::fs::remove_dir_all(dir);
}
