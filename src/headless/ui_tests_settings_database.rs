use crate::app::database::{DatabaseConnectionColor, DatabaseSettings, MAX_RESULT_ROWS};
use crate::headless::tests_support::{
    assert_ui_rect_inside_window, click_ui, dump, has_ui, open_settings_tab, run_script,
    session_for_test, wait_until,
};
use crate::headless::HeadlessSession;

fn database_settings_session() -> HeadlessSession {
    let mut session = session_for_test(2560, 1440);
    let lines = run_script(&mut session, b"scale 1.3333333\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    open_settings_tab(&mut session, 5);
    wait_until(&mut session, 5000, "database setting controls", |session| {
        has_ui(&dump(session), "SettingsDatabaseAdjust(9, 1)")
    });
    session
}

fn click_database_adjust(session: &mut HeadlessSession, row: usize, delta: i8) {
    click_ui(session, &format!("SettingsDatabaseAdjust({row}, {delta})"));
}

fn exercise_u64_row(
    session: &mut HeadlessSession,
    row: usize,
    step: u64,
    get: fn(&DatabaseSettings) -> u64,
) {
    let before = get(session.app.ide_panel.database.settings());
    click_database_adjust(session, row, 1);
    assert_eq!(get(session.app.ide_panel.database.settings()), before + step);
    click_database_adjust(session, row, -1);
    assert_eq!(get(session.app.ide_panel.database.settings()), before);
}

fn exercise_usize_row(
    session: &mut HeadlessSession,
    row: usize,
    step: usize,
    get: fn(&DatabaseSettings) -> usize,
) {
    let before = get(session.app.ide_panel.database.settings());
    assert!(before >= step);
    click_database_adjust(session, row, -1);
    assert_eq!(get(session.app.ide_panel.database.settings()), before - step);
    click_database_adjust(session, row, 1);
    assert_eq!(get(session.app.ide_panel.database.settings()), before);
}

#[test]
fn headless_settings_database_timeout_rows_increment_and_restore_through_ui() {
    let mut session = database_settings_session();
    let before = session.app.ide_panel.database.settings().clone();
    let rows: [(usize, u64, fn(&DatabaseSettings) -> u64); 5] = [
        (0, 30, |settings| settings.transaction_review_timeout_seconds),
        (1, 1, |settings| settings.statement_timeout_seconds),
        (2, 1, |settings| settings.lock_timeout_seconds),
        (3, 1, |settings| settings.connect_timeout_seconds),
        (4, 1, |settings| settings.ssh_startup_timeout_seconds),
    ];

    for (row, step, get) in rows {
        exercise_u64_row(&mut session, row, step, get);
    }

    assert_eq!(session.app.ide_panel.database.settings(), &before);
}

#[test]
fn headless_settings_database_limit_rows_decrement_and_restore_through_ui() {
    let mut session = database_settings_session();
    let before = session.app.ide_panel.database.settings().clone();
    let rows: [(usize, usize, fn(&DatabaseSettings) -> usize); 4] = [
        (5, 10, |settings| settings.default_table_limit),
        (6, 1_000, |settings| settings.result_row_limit),
        (7, 1024 * 1024, |settings| settings.result_memory_limit_bytes),
        (8, 10, |settings| settings.sql_history_limit),
    ];

    for (row, step, get) in rows {
        exercise_usize_row(&mut session, row, step, get);
    }

    assert_eq!(session.app.ide_panel.database.settings(), &before);
}

#[test]
fn headless_settings_database_adjustments_clamp_at_minimum_and_maximum() {
    let mut session = database_settings_session();
    let start_lock_timeout = session
        .app
        .ide_panel
        .database
        .settings()
        .lock_timeout_seconds;
    let decrements_to_minimum = start_lock_timeout.saturating_sub(1);
    for _ in 0..decrements_to_minimum {
        click_database_adjust(&mut session, 2, -1);
    }
    assert_eq!(
        session.app.ide_panel.database.settings().lock_timeout_seconds,
        1
    );
    click_database_adjust(&mut session, 2, -1);
    assert_eq!(
        session.app.ide_panel.database.settings().lock_timeout_seconds,
        1
    );
    for _ in 0..decrements_to_minimum {
        click_database_adjust(&mut session, 2, 1);
    }
    assert_eq!(
        session.app.ide_panel.database.settings().lock_timeout_seconds,
        start_lock_timeout
    );

    let start_result_limit = session
        .app
        .ide_panel
        .database
        .settings()
        .result_row_limit;
    assert_eq!(start_result_limit, MAX_RESULT_ROWS);
    click_database_adjust(&mut session, 6, 1);
    assert_eq!(
        session.app.ide_panel.database.settings().result_row_limit,
        start_result_limit
    );
    click_database_adjust(&mut session, 6, -1);
    assert_eq!(
        session.app.ide_panel.database.settings().result_row_limit,
        start_result_limit - 1_000
    );
    click_database_adjust(&mut session, 6, 1);
    assert_eq!(
        session.app.ide_panel.database.settings().result_row_limit,
        start_result_limit
    );
}

#[test]
fn headless_settings_database_last_row_is_reachable_at_fractional_scale() {
    let mut session = session_for_test(1280, 720);
    let lines = run_script(&mut session, b"scale 1.3333333\n");
    assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
    open_settings_tab(&mut session, 5);

    let id = "SettingsDatabaseAdjust(9, 1)";
    for _ in 0..12 {
        let state = dump(&mut session);
        if has_ui(&state, id) {
            assert_ui_rect_inside_window(&state, id);
            let before = session
                .app
                .ide_panel
                .database
                .settings()
                .default_connection_color;
            click_ui(&mut session, id);
            assert_ne!(
                session
                    .app
                    .ide_panel
                    .database
                    .settings()
                    .default_connection_color,
                before
            );
            return;
        }
        let lines = run_script(&mut session, b"mouse_move 900 550\nwheel 0 -8\nsettle 2000\n");
        assert!(lines.iter().all(|line| line.starts_with("ok")), "{lines:?}");
        assert!(lines.iter().any(|line| line.ends_with("settled=true")), "{lines:?}");
    }
    panic!("{id} did not become reachable after wheel scrolling: {}", dump(&mut session));
}

#[test]
fn headless_settings_database_default_connection_color_cycles_through_ui() {
    let mut session = database_settings_session();
    let before = session.app.ide_panel.database.settings().clone();
    assert_eq!(before.default_connection_color, DatabaseConnectionColor::Blue);

    click_database_adjust(&mut session, 9, 1);
    assert_eq!(
        session
            .app
            .ide_panel
            .database
            .settings()
            .default_connection_color,
        DatabaseConnectionColor::Green
    );
    click_database_adjust(&mut session, 9, -1);
    assert_eq!(session.app.ide_panel.database.settings(), &before);
}
