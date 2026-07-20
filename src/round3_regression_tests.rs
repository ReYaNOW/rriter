use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use crate::{
    app::project_search::{ProjectSearchState, project_search_layout},
    render_view::{
        search::search_panel_geometry,
        terminal_ui::{
            clamp_terminal_pty_dimension, terminal_search_geometry, terminal_tabs_metrics,
        },
    },
};

fn rect_is_finite_non_negative(x: f32, y: f32, w: f32, h: f32) {
    assert!(x.is_finite() && y.is_finite() && w.is_finite() && h.is_finite());
    assert!(w >= 0.0 && h >= 0.0);
}

#[test]
fn r3_006_terminal_tabs_fit_even_when_gap_has_to_collapse() {
    let m = terminal_tabs_metrics(0.0, 50.0, 20, 1.0);
    let used = m.per_tab * 20.0 + m.gap * 19.0;
    assert!(used <= m.available + f32::EPSILON);
}

#[test]
fn r3_007_terminal_add_button_remains_inside_panel() {
    let m = terminal_tabs_metrics(100.0, 50.0, 20, 1.0);
    assert!(m.add_x >= 100.0);
    assert!(m.add_x + m.add_size <= 150.0 + f32::EPSILON);
}

#[test]
fn r3_008_terminal_search_has_no_zero_width_input_hitbox() {
    let g = terminal_search_geometry(100.0, 20.0, 1.0);
    assert_eq!(g.input_w, 0.0);
    assert_eq!(g.text_viewport_w, 0.0);
}

#[test]
fn r3_009_terminal_search_close_never_moves_left_of_panel() {
    for width in 0..200 {
        let g = terminal_search_geometry(100.0, width as f32, 1.0);
        assert!(g.close_x >= 100.0);
        assert!(g.close_x + g.close_size <= 100.0 + width as f32 + f32::EPSILON);
    }
}

#[test]
fn r3_010_terminal_search_text_viewport_is_never_negative() {
    for width in 0..200 {
        assert!(terminal_search_geometry(0.0, width as f32, 1.0).text_viewport_w >= 0.0);
    }
}

#[test]
fn r3_011_empty_terminal_search_has_no_counter_reservation_when_narrow() {
    let g = terminal_search_geometry(0.0, 100.0, 1.0);
    assert_eq!(g.counter_reserve, 0.0);
}

#[test]
fn r3_012_terminal_pty_dimensions_saturate_instead_of_wrapping() {
    assert_eq!(clamp_terminal_pty_dimension(usize::MAX), u16::MAX);
    assert_eq!(clamp_terminal_pty_dimension(80), 80);
}

#[test]
fn r3_013_main_search_has_no_input_when_no_space_exists() {
    let g = search_panel_geometry(0.0, 1.0);
    assert_eq!(g.w, 0.0);
    assert_eq!(g.input_w, 0.0);
}

#[test]
fn r3_014_main_search_close_stays_inside_panel() {
    for right in 0..200 {
        let g = search_panel_geometry(right as f32, 1.0);
        assert!(g.close_x >= g.x);
        assert!(g.close_x + g.close_size <= g.x + g.w + f32::EPSILON);
    }
}

#[test]
fn r3_015_main_search_text_width_is_never_negative() {
    for right in 0..200 {
        assert!(search_panel_geometry(right as f32, 1.0).input_w >= 0.0);
    }
}

#[test]
fn r3_016_project_search_top_controls_fit_the_content_width() {
    for width in 0..240 {
        let l = project_search_layout(100.0, 20.0, width as f32, 300.0, 1.0);
        for rect in [l.query, l.case_button, l.run_button] {
            rect_is_finite_non_negative(rect.x, rect.y, rect.w, rect.h);
            assert!(rect.x >= 100.0 - f32::EPSILON);
            assert!(rect.x + rect.w <= 100.0 + width as f32 + f32::EPSILON);
        }
    }
}

#[test]
fn r3_017_project_search_help_stays_inside_the_panel() {
    for width in 0..240 {
        let l = project_search_layout(100.0, 20.0, width as f32, 300.0, 1.0);
        assert!(l.help_button.x >= 100.0 - f32::EPSILON);
        assert!(l.help_button.x + l.help_button.w <= 100.0 + width as f32 + f32::EPSILON);
    }
}

#[test]
fn r3_018_project_search_filter_fields_fit_available_width() {
    for width in 0..240 {
        let l = project_search_layout(100.0, 20.0, width as f32, 300.0, 1.0);
        for rect in [l.include, l.exclude, l.filter] {
            assert!(rect.w >= 0.0);
            assert!(rect.x + rect.w <= 100.0 + width as f32 + f32::EPSILON);
        }
    }
}

#[test]
fn r3_019_project_search_zero_width_layout_does_not_invent_pixels() {
    let l = project_search_layout(100.0, 20.0, 0.0, 0.0, 1.0);
    assert_eq!(l.query.w, 0.0);
    assert_eq!(l.include.w, 0.0);
    assert_eq!(l.exclude.w, 0.0);
    assert_eq!(l.filter.w, 0.0);
}

#[test]
fn r3_020_project_search_disconnect_clears_running_state() {
    let mut state = ProjectSearchState::default();
    state.running_generation = Some(7);
    assert!(state.handle_worker_disconnect());
    assert_eq!(state.running_generation, None);
    assert!(
        state
            .error
            .as_deref()
            .is_some_and(|e| e.contains("завершился"))
    );
}

#[test]
fn r3_021_project_search_preview_disconnect_clears_pending_keys() {
    let mut state = ProjectSearchState::default();
    state
        .preview_pending
        .insert(crate::app::project_search::ProjectSearchPreviewKey {
            file_idx: 0,
            match_idx: 0,
        });
    state.handle_preview_disconnect();
    assert!(state.preview_pending.is_empty());
    assert!(state.error.is_some());
}

#[test]
fn r3_022_project_search_generation_wraps_to_nonzero_unique_value() {
    let mut state = ProjectSearchState::default();
    state.generation = u64::MAX;
    assert_eq!(state.advance_generation(), 1);
    assert_eq!(state.advance_generation(), 2);
}

#[test]
fn r3_023_new_project_search_cancels_previous_worker() {
    let mut state = ProjectSearchState::default();
    let cancel = Arc::new(AtomicBool::new(false));
    state.worker_cancel = Some(cancel.clone());
    state.running_generation = Some(1);
    state.cancel_running_worker();
    assert!(cancel.load(Ordering::Relaxed));
    assert_eq!(state.running_generation, None);
}

#[test]
fn r3_024_empty_project_search_uses_the_same_cancellation_path() {
    let mut state = ProjectSearchState::default();
    let cancel = Arc::new(AtomicBool::new(false));
    state.worker_cancel = Some(cancel.clone());
    state.cancel_running_worker();
    assert!(cancel.load(Ordering::Relaxed));
    assert!(state.worker_cancel.is_none());
}

#[test]
fn r3_025_preview_worker_failure_is_visible_in_state() {
    let mut state = ProjectSearchState::default();
    state.handle_preview_disconnect();
    assert!(
        state
            .error
            .as_deref()
            .is_some_and(|e| e.contains("Предпросмотр"))
    );
}

#[test]
fn r3_026_preview_disconnect_allows_worker_reinitialization() {
    let mut state = ProjectSearchState::default();
    let (tx, _request_rx) =
        std::sync::mpsc::channel::<crate::app::project_search::ProjectSearchPreviewRequest>();
    let (_message_tx, rx) =
        std::sync::mpsc::channel::<crate::app::project_search::ProjectSearchPreviewWorkerMessage>();
    state.preview_tx = Some(tx);
    state.preview_rx = Some(rx);
    state.handle_preview_disconnect();
    assert!(state.preview_tx.is_none());
    assert!(state.preview_rx.is_none());
    let (tx2, _request_rx2) =
        std::sync::mpsc::channel::<crate::app::project_search::ProjectSearchPreviewRequest>();
    let (_message_tx2, rx2) =
        std::sync::mpsc::channel::<crate::app::project_search::ProjectSearchPreviewWorkerMessage>();
    state.preview_tx = Some(tx2);
    state.preview_rx = Some(rx2);
    assert!(state.preview_tx.is_some() && state.preview_rx.is_some());
}

#[test]
fn r3_027_json_validation_disconnect_clears_stale_result() {
    use crate::app::api_client::{ApiClientState, ApiSpecId};
    let mut api = ApiClientState::default();
    api.seed_body_json_validation(ApiSpecId(1), 2, 3, true);
    assert_eq!(api.body_json_valid_for(ApiSpecId(1), 2, 3), Some(true));
    api.handle_json_validation_disconnect();
    assert_eq!(api.body_json_valid_for(ApiSpecId(1), 2, 3), None);
    assert!(api.import_error.is_some());
}

#[test]
fn r3_028_disconnected_import_receiver_is_removed() {
    let (tx, rx) = std::sync::mpsc::channel::<Option<std::path::PathBuf>>();
    let mut slot = Some(rx);
    drop(tx);
    assert_eq!(
        crate::platform::poll_optional_receiver(&mut slot),
        crate::platform::ReceiverPoll::Disconnected
    );
    assert!(slot.is_none());
}

#[test]
fn r3_029_disconnected_body_file_receiver_is_removed() {
    let (tx, rx) = std::sync::mpsc::channel::<Vec<std::path::PathBuf>>();
    let mut slot = Some(rx);
    drop(tx);
    assert_eq!(
        crate::platform::poll_optional_receiver(&mut slot),
        crate::platform::ReceiverPoll::Disconnected
    );
    assert!(slot.is_none());
}

#[test]
fn r3_030_python_path_disconnect_is_visible() {
    let mut api = crate::app::api_client::ApiClientState::default();
    api.handle_python_path_disconnect();
    assert!(api.mock.uv.last_error.contains("Python/uv"));
}

#[test]
fn r3_031_python_version_disconnect_clears_stale_rows() {
    let mut api = crate::app::api_client::ApiClientState::default();
    api.mock_python_versions_loading = true;
    api.mock_python_versions
        .push(crate::app::api_client::ApiPythonVersionRow {
            version: "3.13".into(),
            installed: true,
            detail: "stale".into(),
        });
    api.handle_python_versions_disconnect();
    assert!(!api.mock_python_versions_loading);
    assert!(api.mock_python_versions.is_empty());
    assert!(!api.mock.uv.last_error.is_empty());
}

#[test]
fn r3_032_python_install_disconnect_marks_runtime_invalid() {
    use crate::app::api_mock::types::ApiPythonRuntimeStatus;
    let mut api = crate::app::api_client::ApiClientState::default();
    api.mock_python_install_running = true;
    api.handle_python_install_disconnect();
    assert!(!api.mock_python_install_running);
    assert_eq!(api.mock.uv.status, ApiPythonRuntimeStatus::Invalid);
    assert!(
        api.mock_python_install_log
            .last()
            .is_some_and(|line| line.text.contains("завершилась"))
    );
}

#[test]
fn r3_033_openapi_disconnect_finishes_exact_generation_ticket() {
    use crate::app::api_client::{ApiClientState, ApiSpecId};
    let mut api = ApiClientState::default();
    let id = ApiSpecId(9);
    let generation = api.begin_load(id, false);
    assert!(api.loading.contains(&id));
    assert!(api.handle_load_disconnect(id, generation));
    assert!(!api.loading.contains(&id));
    assert!(!api.handle_load_disconnect(id, generation));
}

#[test]
fn r3_034_http_disconnect_produces_a_visible_request_error() {
    let message = crate::app::api_client::api_request_disconnect_message(42);
    assert!(message.contains("#42"));
    assert!(message.contains("завершился"));
}

#[test]
fn r3_035_second_openapi_picker_is_rejected_while_first_is_active() {
    let (_tx, rx) = std::sync::mpsc::channel::<Option<std::path::PathBuf>>();
    assert!(!crate::app::api_client::native_picker_can_start(&Some(rx)));
}

#[test]
fn r3_036_second_body_picker_is_rejected_while_first_is_active() {
    let (_tx, rx) = std::sync::mpsc::channel::<Vec<std::path::PathBuf>>();
    assert!(!crate::app::api_client::native_picker_can_start(&Some(rx)));
}

#[test]
fn r3_037_second_python_path_picker_is_rejected_while_first_is_active() {
    let (_tx, rx) = std::sync::mpsc::channel::<()>();
    assert!(!crate::app::api_client::native_picker_can_start(&Some(rx)));
    let none: Option<std::sync::mpsc::Receiver<()>> = None;
    assert!(crate::app::api_client::native_picker_can_start(&none));
}

#[test]
fn r3_038_openapi_receiver_keeps_id_and_generation_for_disconnect_cleanup() {
    use crate::app::api_client::{ApiLoadReceiver, ApiLoadResult, ApiSpecId};
    let (_tx, rx) = std::sync::mpsc::channel::<ApiLoadResult>();
    let receiver = ApiLoadReceiver {
        id: ApiSpecId(8),
        generation: 77,
        rx,
    };
    assert_eq!(receiver.id, ApiSpecId(8));
    assert_eq!(receiver.generation, 77);
}

#[test]
fn r3_039_python_runtime_dialog_never_has_negative_dimensions() {
    for size in 0..200 {
        let l =
            crate::app::api_client::api_python_runtime_dialog_layout(size as f32, size as f32, 1.0);
        rect_is_finite_non_negative(l.box_x, l.box_y, l.box_w, l.box_h);
        assert!(l.content_w >= 0.0);
    }
}

#[test]
fn r3_040_python_version_list_is_clamped_to_dialog_body() {
    let l = crate::app::api_client::api_python_runtime_dialog_layout(80.0, 80.0, 1.0);
    let (x, y, w, h) = crate::app::api_client::api_python_version_list_rect(l, 1.0);
    rect_is_finite_non_negative(x, y, w, h);
    assert!(x >= l.box_x && x + w <= l.box_x + l.box_w + f32::EPSILON);
    assert!(y >= l.box_y && y + h <= l.box_y + l.box_h + f32::EPSILON);
}

#[test]
fn r3_041_mock_guide_modal_never_exceeds_small_window() {
    let l = crate::render_view::api_client_panel::api_overlay_layout(
        100.0, 90.0, 1.0, 860.0, 700.0, 24.0,
    );
    assert!(l.box_w <= 100.0 && l.box_h <= 90.0);
    assert!(l.box_x >= 0.0 && l.box_y >= 0.0);
}

#[test]
fn r3_042_mock_details_modal_never_exceeds_small_window() {
    let l = crate::render_view::api_client_panel::api_overlay_layout(
        70.0, 60.0, 1.0, 720.0, 560.0, 22.0,
    );
    assert!(l.box_w <= 70.0 && l.box_h <= 60.0);
}

#[test]
fn r3_043_mock_guide_content_padding_collapses_when_no_space_exists() {
    let l = crate::render_view::api_client_panel::api_overlay_layout(
        20.0, 20.0, 1.0, 860.0, 700.0, 24.0,
    );
    assert!(l.pad <= l.box_w * 0.25 + f32::EPSILON);
    assert!(l.pad <= l.box_h * 0.25 + f32::EPSILON);
}

#[test]
fn r3_044_api_overlay_close_button_remains_reachable() {
    for size in 0..160 {
        let l = crate::render_view::api_client_panel::api_overlay_layout(
            size as f32,
            size as f32,
            1.0,
            720.0,
            560.0,
            22.0,
        );
        assert!(l.close_x >= l.box_x - f32::EPSILON);
        assert!(l.close_y >= l.box_y - f32::EPSILON);
        assert!(l.close_x + l.close_size <= l.box_x + l.box_w + f32::EPSILON);
        assert!(l.close_y + l.close_size <= l.box_y + l.box_h + f32::EPSILON);
    }
}

fn database_pending(
    id: u64,
    kind: crate::app::database::DatabasePendingJobKind,
    owner: crate::app::database::DatabaseJobOwner,
    connection_id: u64,
    database: Option<&str>,
    table: Option<&str>,
) -> crate::app::database::DatabasePendingJob {
    crate::app::database::DatabasePendingJob {
        id: crate::app::database::DatabaseJobId(id),
        kind,
        owner,
        connection_id: crate::app::database::DatabaseConnectionId(connection_id),
        database_name: database.map(str::to_owned),
        table_name: table.map(str::to_owned),
    }
}

#[test]
fn r3_045_database_queue_never_drops_a_new_payload_as_already_active() {
    use crate::app::database::*;
    let mut panel = DatabasePanelState::default();
    panel.activate_command(
        DatabaseCommand::Shutdown,
        database_pending(
            1,
            DatabasePendingJobKind::LoadTables,
            DatabaseJobOwner::Connection(DatabaseConnectionId(1)),
            1,
            Some("a"),
            None,
        ),
    );
    assert_eq!(
        panel.queue_command(
            DatabaseCommand::Shutdown,
            database_pending(
                2,
                DatabasePendingJobKind::LoadTables,
                DatabaseJobOwner::Connection(DatabaseConnectionId(1)),
                1,
                Some("b"),
                None
            ),
        ),
        DatabaseQueueResult::Queued
    );
    assert_eq!(panel.queued_commands.len(), 1);
    assert_eq!(
        panel.queued_commands[0].1.database_name.as_deref(),
        Some("b")
    );
}

#[test]
fn r3_046_repeated_test_connection_keeps_the_new_command() {
    use crate::app::database::*;
    let mut panel = DatabasePanelState::default();
    let owner = DatabaseJobOwner::Dialog(7);
    panel.queue_command(
        DatabaseCommand::Shutdown,
        database_pending(
            1,
            DatabasePendingJobKind::TestConnection,
            owner,
            1,
            None,
            None,
        ),
    );
    panel.queue_command(
        DatabaseCommand::Shutdown,
        database_pending(
            2,
            DatabasePendingJobKind::TestConnection,
            owner,
            1,
            None,
            None,
        ),
    );
    assert_eq!(
        panel
            .queued_commands
            .iter()
            .map(|(_, p)| p.id.0)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
}

#[test]
fn r3_047_repeated_save_keeps_each_payload_and_secret_owner_distinct() {
    use crate::app::database::*;
    let mut panel = DatabasePanelState::default();
    let owner = DatabaseJobOwner::Dialog(8);
    panel.queue_command(
        DatabaseCommand::Shutdown,
        database_pending(
            1,
            DatabasePendingJobKind::SaveConnection,
            owner,
            11,
            None,
            None,
        ),
    );
    panel.queue_command(
        DatabaseCommand::Shutdown,
        database_pending(
            2,
            DatabasePendingJobKind::SaveConnection,
            owner,
            12,
            None,
            None,
        ),
    );
    assert_eq!(
        panel.queued_commands[0].1.connection_id,
        DatabaseConnectionId(11)
    );
    assert_eq!(
        panel.queued_commands[1].1.connection_id,
        DatabaseConnectionId(12)
    );
}

#[test]
fn r3_048_load_tables_for_different_databases_are_both_queued() {
    use crate::app::database::*;
    let mut panel = DatabasePanelState::default();
    let owner = DatabaseJobOwner::Connection(DatabaseConnectionId(1));
    for (id, db) in [(1, "one"), (2, "two")] {
        panel.queue_command(
            DatabaseCommand::Shutdown,
            database_pending(
                id,
                DatabasePendingJobKind::LoadTables,
                owner,
                1,
                Some(db),
                None,
            ),
        );
    }
    assert_eq!(
        panel
            .queued_commands
            .iter()
            .map(|(_, p)| p.database_name.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("one"), Some("two")]
    );
}

#[test]
fn r3_049_load_ddl_for_different_tables_is_not_coalesced() {
    use crate::app::database::*;
    let mut panel = DatabasePanelState::default();
    let owner = DatabaseJobOwner::Connection(DatabaseConnectionId(1));
    for (id, table) in [(1, "users"), (2, "orders")] {
        panel.queue_command(
            DatabaseCommand::Shutdown,
            database_pending(
                id,
                DatabasePendingJobKind::LoadDdl,
                owner,
                1,
                Some("db"),
                Some(table),
            ),
        );
    }
    assert_eq!(
        panel.queued_commands[0].1.table_name.as_deref(),
        Some("users")
    );
    assert_eq!(
        panel.queued_commands[1].1.table_name.as_deref(),
        Some("orders")
    );
}

#[test]
fn r3_050_count_rows_for_new_filter_is_not_lost() {
    use crate::app::database::*;
    let mut panel = DatabasePanelState::default();
    let owner = DatabaseJobOwner::Table(DatabaseTabId(1));
    panel.queue_command(
        DatabaseCommand::Shutdown,
        database_pending(
            1,
            DatabasePendingJobKind::CountRows,
            owner,
            1,
            Some("db"),
            Some("t"),
        ),
    );
    panel.queue_command(
        DatabaseCommand::Shutdown,
        database_pending(
            2,
            DatabasePendingJobKind::CountRows,
            owner,
            1,
            Some("db"),
            Some("t"),
        ),
    );
    assert_eq!(panel.queued_commands.len(), 2);
}

#[test]
fn r3_051_load_metadata_for_new_table_context_is_not_lost() {
    use crate::app::database::*;
    let mut panel = DatabasePanelState::default();
    let owner = DatabaseJobOwner::Table(DatabaseTabId(3));
    for (id, table) in [(1, "a"), (2, "b")] {
        panel.queue_command(
            DatabaseCommand::Shutdown,
            database_pending(
                id,
                DatabasePendingJobKind::LoadMetadata,
                owner,
                1,
                Some("db"),
                Some(table),
            ),
        );
    }
    assert_eq!(panel.queued_commands.len(), 2);
}

#[test]
fn r3_052_queue_never_replaces_an_existing_job_without_recovery() {
    use crate::app::database::*;
    let mut panel = DatabasePanelState::default();
    let owner = DatabaseJobOwner::Table(DatabaseTabId(3));
    panel.queue_command(
        DatabaseCommand::Shutdown,
        database_pending(
            1,
            DatabasePendingJobKind::LoadMetadata,
            owner,
            1,
            Some("db"),
            Some("a"),
        ),
    );
    panel.queue_command(
        DatabaseCommand::Shutdown,
        database_pending(
            2,
            DatabasePendingJobKind::LoadMetadata,
            owner,
            1,
            Some("db"),
            Some("b"),
        ),
    );
    assert_eq!(panel.pop_queued_command().unwrap().1.id, DatabaseJobId(1));
    assert_eq!(panel.pop_queued_command().unwrap().1.id, DatabaseJobId(2));
}

#[test]
fn r3_053_queue_result_only_reports_started_or_queued_work() {
    use crate::app::database::*;
    let mut panel = DatabasePanelState::default();
    assert_eq!(
        panel.queue_command(
            DatabaseCommand::Shutdown,
            database_pending(
                1,
                DatabasePendingJobKind::RunUserSql,
                DatabaseJobOwner::Query(SqlConsoleId(1)),
                1,
                Some("db"),
                None
            )
        ),
        DatabaseQueueResult::Queued
    );
}

#[test]
fn r3_054_cancel_send_failure_can_clear_active_command_immediately() {
    use crate::app::database::*;
    let mut panel = DatabasePanelState::default();
    panel.activate_command(
        DatabaseCommand::Shutdown,
        database_pending(
            1,
            DatabasePendingJobKind::RunUserSql,
            DatabaseJobOwner::Query(SqlConsoleId(1)),
            1,
            Some("db"),
            None,
        ),
    );
    panel.clear_active_command();
    assert!(panel.pending_job.is_none() && panel.active_command.is_none());
}

#[test]
fn r3_055_database_cancel_watchdog_expires_after_timeout() {
    let mut panel = crate::app::database::DatabasePanelState::default();
    let now = std::time::Instant::now();
    panel.cancel_requested_at = Some(now - std::time::Duration::from_secs(3));
    assert!(panel.cancel_timed_out(now, std::time::Duration::from_secs(2)));
}

#[test]
fn r3_056_database_cancel_tombstones_expire_without_an_event() {
    use crate::app::database::*;
    let mut panel = DatabasePanelState::default();
    let now = std::time::Instant::now();
    panel
        .cancelled_job_ids
        .insert(DatabaseJobId(1), now - std::time::Duration::from_secs(1));
    panel
        .cancelled_job_ids
        .insert(DatabaseJobId(2), now + std::time::Duration::from_secs(1));
    panel.prune_cancelled_jobs(now);
    assert!(!panel.cancelled_job_ids.contains_key(&DatabaseJobId(1)));
    assert!(panel.cancelled_job_ids.contains_key(&DatabaseJobId(2)));
}

#[test]
fn r3_057_database_runtime_reports_event_channel_disconnect() {
    let runtime = crate::app::database::DatabaseRuntime::disconnected_for_test();
    let mut events = Vec::new();
    assert!(!runtime.drain_events(&mut events));
    std::mem::forget(runtime);
}

#[test]
fn r3_058_connection_allocator_skips_host_key_retry_connection_id() {
    use crate::app::database::*;
    let mut panel = DatabasePanelState::default();
    panel.next_connection_id = u64::MAX;
    panel.host_key_retry = Some((
        DatabaseCommand::Shutdown,
        database_pending(
            1,
            DatabasePendingJobKind::TestConnection,
            DatabaseJobOwner::Dialog(1),
            u64::MAX,
            None,
            None,
        ),
    ));
    assert_ne!(
        panel.allocate_connection_id(),
        DatabaseConnectionId(u64::MAX)
    );
}

#[test]
fn r3_059_connection_allocator_skips_pending_secret_connection_id() {
    use crate::app::database::*;
    let mut panel = DatabasePanelState::default();
    panel.next_connection_id = 5;
    panel
        .pending_session_secrets
        .insert(DatabaseConnectionId(5), DatabaseSecretBundle::empty());
    assert_ne!(panel.allocate_connection_id(), DatabaseConnectionId(5));
}

#[test]
fn r3_060_tab_and_console_allocators_skip_host_key_retry_owners() {
    use crate::app::database::*;
    let mut table_panel = DatabasePanelState::default();
    table_panel.next_tab_id = 5;
    table_panel.host_key_retry = Some((
        DatabaseCommand::Shutdown,
        database_pending(
            1,
            DatabasePendingJobKind::LoadMetadata,
            DatabaseJobOwner::Table(DatabaseTabId(5)),
            1,
            Some("db"),
            Some("t"),
        ),
    ));
    assert_ne!(table_panel.allocate_tab_id(), DatabaseTabId(5));

    let mut query_panel = DatabasePanelState::default();
    query_panel.next_console_id = 9;
    query_panel.host_key_retry = Some((
        DatabaseCommand::Shutdown,
        database_pending(
            1,
            DatabasePendingJobKind::RunUserSql,
            DatabaseJobOwner::Query(SqlConsoleId(9)),
            1,
            Some("db"),
            None,
        ),
    ));
    assert_ne!(query_panel.allocate_console_id(), SqlConsoleId(9));
}

#[test]
fn r3_061_git_diff_disconnect_produces_non_loading_error_state() {
    let state = crate::app::git_diff::GitDiffState::error(
        "Загрузка Git diff неожиданно завершилась".to_string(),
        7,
    );
    assert!(!state.loading);
    assert!(
        state
            .error
            .as_deref()
            .is_some_and(|e| e.contains("завершилась"))
    );
    assert_eq!(state.version, 7);
}

#[test]
fn r3_062_git_graph_disconnect_clears_exact_pending_root() {
    let mut state = crate::app::git_panel::GitPanelState::default();
    let root = std::path::PathBuf::from("/tmp/rriter-git-a");
    state.seed_graph_request_for_test(root.clone(), 11, true);
    state.handle_graph_disconnect(&root, 11);
    assert!(!state.graph_pending);
    assert!(
        state
            .graph_notice
            .as_deref()
            .is_some_and(|e| e.contains("завершилась"))
    );
}

#[test]
fn r3_063_blocking_git_disconnect_sets_visible_notice() {
    let mut state = crate::app::git_panel::GitPanelState::default();
    state.latest_request_id = 8;
    state.handle_status_disconnect(8);
    assert!(
        state
            .notice
            .as_deref()
            .is_some_and(|e| e.contains("Git-операция"))
    );
}

#[test]
fn r3_064_git_diff_version_wraps_to_nonzero_value() {
    assert_eq!(crate::app::git_diff::next_git_diff_version(u64::MAX), 1);
    assert_eq!(crate::app::git_diff::next_git_diff_version(1), 2);
}

#[test]
fn r3_065_git_status_request_ids_wrap_without_repeating_zero() {
    let mut state = crate::app::git_panel::GitPanelState::default();
    state.next_request_id = u64::MAX;
    assert_eq!(state.allocate_status_request_id(), u64::MAX);
    assert_eq!(state.allocate_status_request_id(), 1);
    assert_eq!(state.latest_request_id, 1);
}

#[test]
fn r3_066_git_graph_request_ids_wrap_without_repeating_zero() {
    let mut state = crate::app::git_panel::GitPanelState::default();
    state.set_graph_next_request_id_for_test(u64::MAX);
    assert_eq!(state.allocate_graph_request_id(), u64::MAX);
    assert_eq!(state.allocate_graph_request_id(), 1);
}

#[test]
fn r3_067_git_graph_disconnect_uses_receiver_metadata_not_global_reset() {
    let mut state = crate::app::git_panel::GitPanelState::default();
    let a = std::path::PathBuf::from("/tmp/a");
    let b = std::path::PathBuf::from("/tmp/b");
    state.seed_graph_request_for_test(a.clone(), 1, true);
    state.seed_graph_request_for_test(b.clone(), 2, false);
    state.handle_graph_disconnect(&a, 1);
    state.handle_graph_disconnect(&b, 1);
    assert!(state.graph_notice.is_some());
}

#[test]
fn r3_068_empty_workspace_reset_cancels_old_git_receivers() {
    let mut state = crate::app::git_panel::GitPanelState::default();
    let root = std::path::PathBuf::from("/tmp/a");
    state.seed_graph_request_for_test(root, 1, true);
    state.pending = true;
    state.reset_async_state();
    assert!(state.async_state_is_empty_for_test());
}

#[test]
fn r3_069_refresh_window_reset_cancels_blocking_and_graph_receivers() {
    let mut state = crate::app::git_panel::GitPanelState::default();
    state.pending = true;
    state.graph_pending = true;
    state.reset_async_state();
    assert!(state.async_state_is_empty_for_test());
    assert!(state.pending_label.is_none());
}

#[test]
fn r3_070_empty_workspace_roots_clear_old_file_tree_receiver() {
    use crate::app::file_tree::{FileTreeScanMessage, clear_file_tree_for_empty_roots};
    let (_tx, rx) = std::sync::mpsc::channel::<FileTreeScanMessage>();
    let mut slot = Some(rx);
    let mut panel = crate::app::IdePanelState::default();
    panel.file_tree_error = Some("old".into());
    clear_file_tree_for_empty_roots(&mut panel, &mut slot);
    assert!(slot.is_none());
    assert!(panel.file_tree_nodes.is_empty());
    assert!(panel.file_tree_error.is_none());
}

#[test]
fn r3_071_file_tree_scan_failure_is_visible_in_ui_state() {
    let mut panel = crate::app::IdePanelState::default();
    crate::app::file_tree::apply_file_tree_scan_error(&mut panel, "scan failed");
    assert_eq!(panel.file_tree_error.as_deref(), Some("scan failed"));
}

#[test]
fn r3_072_file_watcher_disconnect_requests_visible_restart() {
    let message = crate::app::events::file_watcher_disconnect_message();
    assert!(message.contains("перезапуск"));
    assert!(message.contains("завершилось"));
}

#[test]
fn r3_073_open_file_picker_slot_rejects_second_dialog() {
    let (_tx, rx) = std::sync::mpsc::channel::<Option<std::path::PathBuf>>();
    assert!(!crate::platform::receiver_slot_available(&Some(rx)));
}

#[test]
fn r3_074_folder_picker_slot_rejects_second_dialog() {
    let (_tx, rx) = std::sync::mpsc::channel::<Option<std::path::PathBuf>>();
    assert!(!crate::platform::receiver_slot_available(&Some(rx)));
}

#[test]
fn r3_075_save_as_picker_slot_rejects_second_dialog() {
    let (_tx, rx) = std::sync::mpsc::channel::<Option<std::path::PathBuf>>();
    assert!(!crate::platform::receiver_slot_available(&Some(rx)));
}

#[test]
fn r3_076_disconnected_native_dialog_receiver_is_cleared() {
    let (tx, rx) = std::sync::mpsc::channel::<Option<std::path::PathBuf>>();
    let mut slot = Some(rx);
    drop(tx);
    assert_eq!(
        crate::platform::poll_optional_receiver(&mut slot),
        crate::platform::ReceiverPoll::Disconnected
    );
    assert!(slot.is_none());
}

#[test]
fn r3_077_native_picker_spawn_failure_has_user_facing_message() {
    let message = crate::app::native_picker_spawn_error("выбор файла", "denied");
    assert!(message.contains("выбор файла"));
    assert!(message.contains("denied"));
}

#[test]
fn r3_078_external_changes_disconnect_requests_a_retry() {
    let message = crate::app::external_changes_disconnect_message();
    assert!(message.contains("повтор"));
}

#[test]
fn r3_079_settings_width_never_becomes_negative() {
    for width in 0..200 {
        let l = crate::render_view::settings_ui::settings_modal_layout(width as f32, 500.0, 1.0);
        assert!(l.outer.w >= 0.0 && l.inner.w >= 0.0);
        assert!(l.outer.x >= 0.0);
    }
}

#[test]
fn r3_080_settings_height_never_becomes_negative() {
    for height in 0..200 {
        let l = crate::render_view::settings_ui::settings_modal_layout(500.0, height as f32, 1.0);
        assert!(l.outer.h >= 0.0 && l.inner.h >= 0.0);
        assert!(l.outer.y >= 0.0);
    }
}

#[test]
fn r3_081_settings_sidebar_never_consumes_the_whole_inner_panel() {
    for width in 0..240 {
        let l = crate::render_view::settings_ui::settings_modal_layout(width as f32, 300.0, 1.0);
        assert!(l.sidebar_w <= l.inner.w * 0.35 + f32::EPSILON);
        assert!(l.sidebar_w <= l.inner.w + f32::EPSILON);
    }
}

#[test]
fn r3_082_database_modal_fit_never_reapplies_a_larger_minimum() {
    let r = crate::ui_system::fit_centered_rect(100.0, 80.0, 980.0, 700.0, 16.0);
    assert!(r.w <= 100.0 && r.h <= 80.0);
    assert!(r.x >= 0.0 && r.y >= 0.0);
}

#[test]
fn r3_083_tool_log_modal_width_fits_small_window() {
    let r = crate::ui_system::fit_centered_rect(100.0, 300.0, 720.0, 520.0, 16.0);
    assert!(r.w <= 100.0);
    assert!(r.x >= 0.0);
}

#[test]
fn r3_084_tool_log_modal_height_fits_small_window() {
    for height in 0..240 {
        let h = crate::app::tool_installer::log_modal_height(height as f32, 1.0);
        assert!(h >= 0.0 && h <= height as f32 + f32::EPSILON);
    }
}

#[test]
fn r3_085_tool_log_viewport_collapses_to_zero_instead_of_overflowing() {
    for height in 0..180 {
        let modal = crate::app::tool_installer::log_modal_height(height as f32, 1.0);
        let viewport = crate::app::tool_installer::log_viewport_height(height as f32, 1.0);
        assert!(viewport >= 0.0 && viewport <= modal + f32::EPSILON);
    }
}

#[test]
fn r3_086_project_search_help_modal_fits_any_tiny_window() {
    for size in 0..200 {
        let r = crate::ui_system::fit_centered_rect(size as f32, size as f32, 520.0, 430.0, 16.0);
        assert!(r.w <= size as f32 + f32::EPSILON);
        assert!(r.h <= size as f32 + f32::EPSILON);
        assert!(r.x >= 0.0 && r.y >= 0.0);
    }
}

#[test]
fn r3_087_tool_installer_state_changes_only_after_successful_worker_start() {
    let mut installer = crate::app::tool_installer::ToolInstaller::default();
    let before_revision = installer.revision();
    let result = installer.start(crate::platform::ToolKind::Git, None);
    assert!(result.is_err());
    assert_eq!(installer.target(), None);
    assert!(!installer.is_log_open());
    assert_eq!(installer.revision(), before_revision);
}

#[test]
fn r3_088_tool_installer_shutdown_transfers_unfinished_worker_to_reaper() {
    let mut installer = crate::app::tool_installer::ToolInstaller::default();
    let cancel = installer.seed_running_worker_for_test(std::time::Duration::from_millis(80));
    assert!(installer.has_worker_for_test());
    installer.shutdown();
    assert!(cancel.load(Ordering::Acquire));
    assert!(!installer.has_worker_for_test());
}

#[test]
fn r3_089_tool_installer_shutdown_does_not_poll_or_sleep_on_ui_thread() {
    let mut installer = crate::app::tool_installer::ToolInstaller::default();
    installer.seed_running_worker_for_test(std::time::Duration::from_millis(120));
    let started = std::time::Instant::now();
    installer.shutdown();
    assert!(started.elapsed() < std::time::Duration::from_millis(50));
}

#[test]
fn r3_109_poison_recovery_returns_inner_state_instead_of_panicking() {
    assert_eq!(crate::platform::recover_poisoned::<u32>(Err(42)), 42);
    assert_eq!(crate::platform::recover_poisoned::<u32>(Ok(7)), 7);
}

#[test]
fn r3_110_hover_trace_time_before_epoch_is_safe() {
    let before = std::time::UNIX_EPOCH - std::time::Duration::from_secs(5);
    assert_eq!(crate::render_view::hover_trace_epoch_millis(before), 0);
}

#[test]
fn r3_112_terminal_tab_metrics_bound_the_actual_total_width() {
    for panel_w in 0..=200 {
        for count in 0..=40 {
            let metrics = terminal_tabs_metrics(13.0, panel_w as f32, count, 1.0);
            let used = if count == 0 {
                0.0
            } else {
                metrics.per_tab * count as f32 + metrics.gap * count.saturating_sub(1) as f32
            };
            assert!(
                used <= metrics.available + 0.001,
                "w={panel_w} count={count} used={used} available={}",
                metrics.available
            );
            assert!(metrics.add_x + metrics.add_size <= 13.0 + panel_w as f32 + 0.001);
        }
    }
}

#[test]
fn r3_113_explicit_clip_cannot_escape_nested_parent_clip() {
    use crate::ui_system::{UiClipRect, UiId, UiRegistry};
    let mut registry = UiRegistry::new();
    registry.push_clip(UiClipRect {
        x: 20.0,
        y: 20.0,
        w: 10.0,
        h: 10.0,
    });
    registry.register_rect_clipped(
        UiId::SearchInput,
        0.0,
        0.0,
        100.0,
        100.0,
        UiClipRect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
        },
        -1.0,
        -1.0,
    );
    registry.pop_clip();
    assert!(registry.find_at(25.0, 25.0).is_some());
    assert!(registry.find_at(5.0, 5.0).is_none());
}

#[test]
fn r3_114_disconnected_worker_slots_are_cleared_across_subsystems() {
    let (tx, rx) = std::sync::mpsc::channel::<u8>();
    let mut slot = Some(rx);
    drop(tx);
    assert_eq!(
        crate::platform::poll_optional_receiver(&mut slot),
        crate::platform::ReceiverPoll::Disconnected
    );
    assert!(slot.is_none());

    let mut project = ProjectSearchState::default();
    project.running_generation = Some(3);
    assert!(project.handle_worker_disconnect());
    assert!(project.running_generation.is_none());

    let mut git = crate::app::git_panel::GitPanelState::default();
    let request_id = git.allocate_status_request_id();
    git.handle_status_disconnect(request_id);
    assert!(
        git.notice
            .as_deref()
            .is_some_and(|notice| notice.contains("завершилась"))
    );
}

#[test]
fn r3_115_all_shared_layouts_survive_systematic_tiny_windows() {
    for width in 0..=200 {
        for height in [0, 1, 8, 32, 64, 128, 200] {
            let width = width as f32;
            let height = height as f32;
            let fitted = crate::ui_system::fit_centered_rect(width, height, 980.0, 700.0, 16.0);
            rect_is_finite_non_negative(fitted.x, fitted.y, fitted.w, fitted.h);
            assert!(fitted.x + fitted.w <= width + 0.001);
            assert!(fitted.y + fitted.h <= height + 0.001);

            let settings =
                crate::render_view::settings_ui::settings_modal_layout(width, height, 1.0);
            rect_is_finite_non_negative(
                settings.outer.x,
                settings.outer.y,
                settings.outer.w,
                settings.outer.h,
            );
            let api = crate::render_view::api_client_panel::api_overlay_layout(
                width, height, 1.0, 720.0, 520.0, 16.0,
            );
            rect_is_finite_non_negative(api.box_x, api.box_y, api.box_w, api.box_h);
            let project = project_search_layout(0.0, 0.0, width, height, 1.0);
            for rect in [
                project.query,
                project.include,
                project.exclude,
                project.filter,
                project.help_button,
            ] {
                rect_is_finite_non_negative(rect.x, rect.y, rect.w, rect.h);
                assert!(rect.x + rect.w <= width + 0.001);
            }
        }
    }
}

#[test]
fn r3_116_behavioral_invariants_cover_clipping_disconnect_and_payload_queueing() {
    use crate::ui_system::{UiClipRect, UiId, UiRegistry};
    let mut registry = UiRegistry::new();
    registry.push_clip(UiClipRect {
        x: 10.0,
        y: 10.0,
        w: 10.0,
        h: 10.0,
    });
    registry.register_rect_clipped(
        UiId::SearchInput,
        0.0,
        0.0,
        30.0,
        30.0,
        UiClipRect {
            x: 0.0,
            y: 0.0,
            w: 30.0,
            h: 30.0,
        },
        -1.0,
        -1.0,
    );
    registry.pop_clip();
    assert!(registry.find_at(15.0, 15.0).is_some());
    assert!(registry.find_at(5.0, 5.0).is_none());

    let (tx, rx) = std::sync::mpsc::channel::<u8>();
    let mut slot = Some(rx);
    drop(tx);
    assert_eq!(
        crate::platform::poll_optional_receiver(&mut slot),
        crate::platform::ReceiverPoll::Disconnected
    );

    let mut panel = crate::app::database::DatabasePanelState::default();
    let first = database_pending(
        900,
        crate::app::database::DatabasePendingJobKind::LoadTables,
        crate::app::database::DatabaseJobOwner::Connection(
            crate::app::database::DatabaseConnectionId(1),
        ),
        1,
        Some("alpha"),
        None,
    );
    let second = database_pending(
        901,
        crate::app::database::DatabasePendingJobKind::LoadTables,
        crate::app::database::DatabaseJobOwner::Connection(
            crate::app::database::DatabaseConnectionId(1),
        ),
        1,
        Some("beta"),
        None,
    );
    panel.activate_command(crate::app::database::DatabaseCommand::Shutdown, first);
    assert!(matches!(
        panel.queue_command(crate::app::database::DatabaseCommand::Shutdown, second),
        crate::app::database::DatabaseQueueResult::Queued
    ));
    assert_eq!(panel.queued_commands.len(), 1);
}

#[test]
fn r3_117_animated_settings_layout_moves_outer_and_inner_together() {
    let base = crate::render_view::settings_ui::settings_modal_layout(800.0, 600.0, 1.0);
    let animated =
        crate::render_view::settings_ui::animated_settings_modal_layout(800.0, 600.0, 1.0, 0.35);
    let outer_delta = animated.outer.y - base.outer.y;
    let inner_delta = animated.inner.y - base.inner.y;
    assert!((outer_delta - inner_delta).abs() < 0.001);
    assert_eq!(animated.outer.x, base.outer.x);
    assert_eq!(animated.inner.x, base.inner.x);
}

#[test]
fn r3_118_settings_ignore_input_stays_inside_content_width() {
    for width in [120.0, 240.0, 480.0, 1000.0] {
        for workspaces in [0, 1, 20] {
            let layout = crate::render_view::settings_ui::settings_modal_layout(width, 700.0, 1.0);
            let rect = crate::render_view::settings_ui::settings_ignore_input_rect(
                layout, 1.0, workspaces, 0.0,
            );
            assert!(rect.w >= 0.0);
            assert!(rect.x >= layout.inner.x - 0.001);
            assert!(rect.x + rect.w <= layout.inner.x + layout.inner.w + 0.001);
        }
    }
}

#[test]
fn r3_119_scrollbar_thumb_never_exceeds_tiny_track() {
    for track in [0.0, 1.0, 5.0, 10.0] {
        let thumb = crate::scroll::scrollbar_thumb(0.0, track, 10.0, 1000.0, 0.0, 40.0);
        if let Some(thumb) = thumb {
            assert!(thumb.len <= track + 0.001);
        }
    }
}

#[test]
fn r3_120_nested_gl_scissor_is_intersected_not_replaced() {
    let intersection =
        crate::render_view::intersect_scissor_boxes([20, 20, 10, 10], [0, 0, 100, 100]);
    assert_eq!(intersection, [20, 20, 10, 10]);
    let partial = crate::render_view::intersect_scissor_boxes([20, 20, 10, 10], [25, 5, 20, 20]);
    assert_eq!(partial, [25, 20, 5, 5]);
}
