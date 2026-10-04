//! Configurable Database command coverage.

use crate::app::database::DatabaseTableModal;
use crate::headless::HeadlessSession;
use crate::headless::tests_support::{click_ui, dump, scratch_dir, ui_center, wait_until};

use super::ui_tests_hotkeys::install_config_keymap;
use super::ui_tests_hotkeys_characterization_database::{
    active_query_state, active_table_state, database_query_session, open_fixture_table, run_ok,
};

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_SCALE: f32 = 4.0 / 3.0;

fn enter_sql(session: &mut HeadlessSession, sql: &str) {
    let (x, y) = ui_center(&dump(session), "EditorTextBody");
    run_ok(
        session,
        &format!("mouse_move {x} {y}\nclick\nkey ctrl+a\ntype {sql}\n"),
    );
}

#[test]
fn headless_hotkeys_database_table_refresh_save_and_preview_match_buttons() {
    let fixture = crate::headless::tests_support::postgres_fixture();
    let dir = scratch_dir("ui-hotkeys-database-table-commands");
    let mut session = crate::headless::tests_support::workspace_with_explorer(
        TEST_WIDTH,
        TEST_HEIGHT,
        TEST_SCALE,
        &dir,
    );
    open_fixture_table(&mut session, &fixture);
    wait_until(&mut session, 10000, "idle database worker", |session| {
        session.app.ide_panel.database.pending_job.is_none()
    });
    let config_dir = scratch_dir("ui-hotkeys-database-table-commands-config");
    install_config_keymap(
        &mut session,
        &config_dir.join("config.json"),
        serde_json::json!({
            "database.table.refresh": ["ctrl+shift+r"],
            "database.table.save": ["ctrl+shift+s"],
            "database.table.preview_sql": ["ctrl+shift+p"]
        }),
    );

    let generation = active_table_state(&session).unwrap().generation;
    run_ok(&mut session, "key ctrl+shift+r\n");
    assert_ne!(active_table_state(&session).unwrap().generation, generation);
    wait_until(&mut session, 10000, "table refresh", |session| {
        session.app.ide_panel.database.pending_job.is_none()
            && active_table_state(session).is_some_and(|state| !state.grid.refreshing)
    });

    let (x, y) = ui_center(&dump(&mut session), "DatabaseTableCell(0, 1)");
    run_ok(&mut session, &format!("mouse_move {x} {y}\ndblclick\n"));
    wait_until(&mut session, 3000, "table cell editor", |session| {
        crate::headless::tests_support::has_ui(&dump(session), "DatabaseTableCellEditor")
    });
    let (x, y) = ui_center(&dump(&mut session), "DatabaseTableCellEditor");
    run_ok(
        &mut session,
        &format!("mouse_move {x} {y}\nclick\nkey ctrl+a\ntype hotkey-pending\nkey enter\n"),
    );
    assert!(active_table_state(&session).unwrap().grid.row(0).unwrap().is_dirty());

    run_ok(&mut session, "key ctrl+shift+p\n");
    let preview = match session.app.ide_panel.database.table_modal.as_ref() {
        Some(DatabaseTableModal::SqlPreview { text, .. }) => text,
        modal => panic!("preview command did not open SQL preview: {modal:?}"),
    };
    assert!(preview.contains("hotkey-pending"), "preview omitted pending edit: {preview}");
    run_ok(&mut session, "key escape\nkey ctrl+shift+s\n");
    wait_until(&mut session, 3000, "table save review", |session| {
        matches!(
            session.app.ide_panel.database.table_modal.as_ref(),
            Some(DatabaseTableModal::Review { .. })
        )
    });
    click_ui(&mut session, "DatabaseTableModalPrimary");
    wait_until(&mut session, 10000, "table save and reload", |session| {
        active_table_state(session).is_some_and(|state| {
            !state.loading
                && !state.grid.refreshing
                && state.grid.row(0).is_some_and(|row| {
                    row.cells[1].value.copy_text() == "hotkey-pending"
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
        "hotkey-pending"
    );
    let _ = std::fs::remove_dir_all(config_dir);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_database_query_explain_commands_run_plans() {
    let (dir, _fixture, mut session) = database_query_session("ui-hotkeys-database-query-plans");
    wait_until(&mut session, 10000, "idle database worker", |session| {
        session.app.ide_panel.database.pending_job.is_none()
    });
    let config_dir = scratch_dir("ui-hotkeys-database-query-plans-config");
    install_config_keymap(
        &mut session,
        &config_dir.join("config.json"),
        serde_json::json!({
            "database.query.explain": ["ctrl+alt+shift+f10"],
            "database.query.explain_analyze": ["ctrl+alt+shift+f11"]
        }),
    );

    for (chord, sql, fixture_error) in [
        (
            "ctrl+alt+shift+f10",
            "SELECT id FROM public.pgo_items LIMIT 1;",
            false,
        ),
        (
            "ctrl+alt+shift+f11",
            "SELECT id FROM public.pgo_items LIMIT 2;",
            true,
        ),
    ] {
        enter_sql(&mut session, sql);
        wait_until(&mut session, 10000, "idle database worker", |session| {
            session.app.ide_panel.database.pending_job.is_none()
        });
        run_ok(&mut session, &format!("key {chord}\n"));
        assert!(active_query_state(&session).is_some_and(|state| state.running),
            "{chord} did not start the explain command: {:?}", active_query_state(&session));
        wait_until(&mut session, 10000, "database explain plan", |session| {
            active_query_state(session).is_some_and(|state| {
                !state.running && (state.error.is_some() || !state.results.is_empty())
            })
        });
        let query = active_query_state(&session).unwrap();
        if fixture_error {
            assert!(
                query.error.as_deref().is_some_and(|error| error.contains("unsupported SQL family")),
                "EXPLAIN ANALYZE did not report the fixture's unsupported query: {query:?}"
            );
        } else {
            assert!(query.error.is_none(), "explain command failed: {:?}", query.error);
            assert!(!query.results[0].rows.is_empty(), "explain plan has no rows: {query:?}");
        }
    }
    let _ = std::fs::remove_dir_all(config_dir);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_database_query_format_and_next_diagnostic_match_buttons() {
    let (dir, _fixture, mut session) = database_query_session("ui-hotkeys-database-query-editing");
    let config_dir = scratch_dir("ui-hotkeys-database-query-editing-config");
    install_config_keymap(
        &mut session,
        &config_dir.join("config.json"),
        serde_json::json!({
            "database.query.format": ["ctrl+alt+shift+f10"],
            "database.query.next_diagnostic": ["ctrl+alt+shift+f11"]
        }),
    );

    enter_sql(&mut session, "select  id,name  from  public.pgo_items where id=1;");
    run_ok(&mut session, "key ctrl+alt+shift+f10\n");
    let formatted = session.app.editor.get_full_text();
    assert!(formatted.contains('\n'), "format command left SQL on one line: {formatted}");
    assert!(formatted.to_ascii_uppercase().contains("SELECT"));
    assert!(formatted.contains("pgo_items"));

    let invalid_sql = "SELECT 1, FROM public.pgo_items;";
    enter_sql(&mut session, invalid_sql);
    wait_until(&mut session, 5000, "SQL diagnostic", |session| {
        active_query_state(session).is_some_and(|state| !state.editor_diagnostics.is_empty())
    });
    let expected = {
        let diagnostic = &active_query_state(&session).unwrap().editor_diagnostics[0];
        crate::lsp::lsp_pos_to_offset(
            invalid_sql,
            diagnostic.start_line,
            diagnostic.start_col,
        )
    };
    run_ok(&mut session, "key ctrl+alt+shift+f11\n");
    assert_eq!(session.app.editor.cursor, expected, "next diagnostic did not move the cursor");
    let _ = std::fs::remove_dir_all(config_dir);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn headless_hotkeys_database_table_command_without_table_reports_unavailable() {
    let (dir, _fixture, mut session) = database_query_session("ui-hotkeys-database-no-table");
    let config_dir = scratch_dir("ui-hotkeys-database-no-table-config");
    install_config_keymap(
        &mut session,
        &config_dir.join("config.json"),
        serde_json::json!({"database.table.refresh": ["ctrl+shift+r"]}),
    );
    let text_before = session.app.editor.get_full_text();

    run_ok(&mut session, "key ctrl+shift+r\n");
    assert_eq!(session.app.readonly_notice_text, "Нет открытой таблицы БД");
    assert_eq!(session.app.editor.get_full_text(), text_before, "unavailable chord was not consumed");
    let _ = std::fs::remove_dir_all(config_dir);
    let _ = std::fs::remove_dir_all(dir);
}
