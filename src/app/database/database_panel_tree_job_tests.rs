#[test]
fn panel_allocates_ids_above_persisted_values() {
        let mut persisted = DatabasePersistedState::default();
        persisted.connections.push(config(40));
        let mut panel = DatabasePanelState::from_persisted(persisted);
        assert_eq!(panel.allocate_connection_id(), DatabaseConnectionId(41));
        assert_eq!(panel.allocate_job_id(), DatabaseJobId(1));
    }

    #[test]
    fn database_tree_scroll_counts_loading_rows_and_excludes_toolbar_from_viewport() {
        let mut panel = DatabasePanelState::default();
        let mut connection = DatabaseConnectionNode::new(config(1));
        connection.expanded = true;
        connection.loading = true;
        assert_eq!(panel.visible_tree_row_count(), 0);

        panel.connections.push(connection);
        assert_eq!(panel.visible_tree_row_count(), 2);

        let mut database = DatabaseDatabaseNode::new("main".to_string());
        database.expanded = true;
        database.loading = true;
        panel.connections[0].loading = false;
        panel.connections[0].databases.push(database);
        assert_eq!(panel.visible_tree_row_count(), 3);

        panel.connections[0].databases[0].loading = false;
        panel.connections[0].databases[0].tables = vec![
            DatabaseTableInfo {
                name: "a".to_string(),
                partitioned: false,
            },
            DatabaseTableInfo {
                name: "b".to_string(),
                partitioned: false,
            },
        ];
        assert_eq!(panel.visible_tree_row_count(), 4);
        assert_eq!(panel.max_tree_scroll(100.0, 1.0), 46.0);
    }

    #[test]
    fn modal_state_blocks_background_and_toolbar_states_follow_selection() {
        let mut panel = DatabasePanelState::default();
        assert!(!panel.modal_open());
        assert!(!panel.selected_connection_refresh_enabled());
        panel
            .connections
            .push(DatabaseConnectionNode::new(config(1)));
        panel.selected_connection = Some(DatabaseConnectionId(1));
        assert!(panel.selected_connection_refresh_enabled());
        panel.dialog = Some(DatabaseConnectionDialog::new(DatabaseConnectionColor::Blue));
        assert!(panel.modal_open());
    }

    fn pending(id: u64, kind: DatabasePendingJobKind) -> DatabasePendingJob {
        DatabasePendingJob {
            id: DatabaseJobId(id),
            kind,
            owner: DatabaseJobOwner::Connection(DatabaseConnectionId(1)),
            connection_id: DatabaseConnectionId(1),
            database_name: Some("db".to_string()),
            table_name: Some("items".to_string()),
        }
    }

    #[test]
    fn bug_42_multiple_restored_table_loads_are_queued_in_order() {
        let mut panel = DatabasePanelState::default();
        let mut first = pending(1, DatabasePendingJobKind::LoadMetadata);
        first.owner = DatabaseJobOwner::Table(crate::app::database::DatabaseTabId(1));
        let mut second = pending(2, DatabasePendingJobKind::LoadMetadata);
        second.owner = DatabaseJobOwner::Table(crate::app::database::DatabaseTabId(2));
        panel.activate_command(super::super::DatabaseCommand::Shutdown, first);
        panel.queue_command(super::super::DatabaseCommand::Shutdown, second);
        assert_eq!(
            panel.pending_job.as_ref().map(|job| job.id),
            Some(DatabaseJobId(1))
        );
        assert_eq!(
            panel.queued_commands.front().map(|(_, job)| job.id),
            Some(DatabaseJobId(2))
        );
    }

    #[test]
    fn bug_43_new_database_job_never_overwrites_active_pending_context() {
        let mut panel = DatabasePanelState::default();
        panel.activate_command(
            super::super::DatabaseCommand::Shutdown,
            pending(7, DatabasePendingJobKind::RunUserSql),
        );
        panel.queue_command(
            super::super::DatabaseCommand::Shutdown,
            pending(8, DatabasePendingJobKind::LoadDatabases),
        );
        assert_eq!(
            panel.pending_job.as_ref().map(|job| job.id),
            Some(DatabaseJobId(7))
        );
        assert_eq!(panel.queued_commands.len(), 1);
    }

    #[test]
    fn bug_45_finishing_busy_job_preserves_queued_commands() {
        let mut panel = DatabasePanelState::default();
        panel.activate_command(
            super::super::DatabaseCommand::Shutdown,
            pending(1, DatabasePendingJobKind::RunUserSql),
        );
        panel.queue_command(
            super::super::DatabaseCommand::Shutdown,
            pending(2, DatabasePendingJobKind::LoadDatabases),
        );
        panel.clear_active_command();
        assert!(panel.pending_job.is_none());
        assert!(panel.active_command.is_none());
        assert_eq!(
            panel.pop_queued_command().map(|(_, job)| job.id),
            Some(DatabaseJobId(2))
        );
    }

    #[test]
    fn bug_46_busy_metadata_job_clears_table_loading_target() {
        assert!(
            DatabasePendingJobKind::LoadMetadata
                .recovery_targets()
                .metadata_loading
        );
    }

    #[test]
    fn bug_47_busy_database_list_job_clears_connection_loading_target() {
        assert!(
            DatabasePendingJobKind::LoadDatabases
                .recovery_targets()
                .connection_loading
        );
    }

    #[test]
    fn bug_48_busy_table_list_job_clears_database_loading_target() {
        assert!(
            DatabasePendingJobKind::LoadTables
                .recovery_targets()
                .database_loading
        );
    }

    #[test]
    fn bug_49_busy_connection_test_clears_dialog_status_target() {
        assert!(
            DatabasePendingJobKind::TestConnection
                .recovery_targets()
                .dialog_status
        );
    }

    #[test]
    fn bug_50_busy_connection_save_clears_dialog_status_target() {
        assert!(
            DatabasePendingJobKind::SaveConnection
                .recovery_targets()
                .dialog_status
        );
    }

    #[test]
    fn bug_51_busy_user_sql_clears_query_running_target() {
        assert!(
            DatabasePendingJobKind::RunUserSql
                .recovery_targets()
                .query_running
        );
    }

    #[test]
    fn bug_52_busy_commit_clears_both_transaction_targets() {
        let targets = DatabasePendingJobKind::CommitTransaction.recovery_targets();
        assert!(targets.table_transaction);
        assert!(targets.query_transaction);
    }

    #[test]
    fn bug_53_send_failure_recovers_running_sql_state() {
        let source = include_str!("database_app_methods.rs");
        assert!(source.contains("self.recover_database_pending_job(&pending, &message, false);"));
        assert!(
            DatabasePendingJobKind::RunUserSql
                .recovery_targets()
                .query_running
        );
    }

    #[test]
    fn bug_54_send_failure_recovers_test_and_save_dialog_state() {
        for kind in [
            DatabasePendingJobKind::TestConnection,
            DatabasePendingJobKind::SaveConnection,
        ] {
            assert!(kind.recovery_targets().dialog_status);
        }
    }

    #[test]
    fn bug_55_send_failure_recovers_database_and_table_loaders() {
        assert!(
            DatabasePendingJobKind::LoadDatabases
                .recovery_targets()
                .connection_loading
        );
        assert!(
            DatabasePendingJobKind::LoadTables
                .recovery_targets()
                .database_loading
        );
        assert!(
            DatabasePendingJobKind::LoadMetadata
                .recovery_targets()
                .metadata_loading
        );
    }

    #[test]
    fn bug_56_send_failure_recovers_table_commit_state() {
        let targets = DatabasePendingJobKind::CommitTransaction.recovery_targets();
        assert!(targets.table_transaction);
    }

    #[test]
    fn bug_57_cancelled_jobs_use_the_same_complete_recovery_map() {
        let source = include_str!("database_app_event_methods.rs");
        assert!(
            source
                .contains("self.recover_database_pending_job(pending, \"Запрос отменён\", true);")
        );
        assert!(
            DatabasePendingJobKind::CountRows
                .recovery_targets()
                .count_loading
        );
        assert!(
            DatabasePendingJobKind::LoadChunk
                .recovery_targets()
                .chunk_loading
        );
        assert!(
            DatabasePendingJobKind::RollbackTransaction
                .recovery_targets()
                .query_transaction
        );
    }

    fn pending_for(
        id: u64,
        owner: DatabaseJobOwner,
        kind: DatabasePendingJobKind,
    ) -> DatabasePendingJob {
        DatabasePendingJob {
            id: DatabaseJobId(id),
            kind,
            owner,
            connection_id: DatabaseConnectionId(1),
            database_name: None,
            table_name: None,
        }
    }

    #[test]
    fn r2_001_cancel_removes_queued_save_for_dialog_owner() {
        let mut panel = DatabasePanelState::default();
        let owner = DatabaseJobOwner::Dialog(41);
        assert_eq!(
            panel.queue_command(
                super::super::DatabaseCommand::Shutdown,
                pending_for(1, owner, DatabasePendingJobKind::SaveConnection),
            ),
            DatabaseQueueResult::Queued
        );
        let removed = panel.remove_queued_owner(owner);
        assert_eq!(removed.len(), 1);
        assert!(matches!(
            removed[0].kind,
            DatabasePendingJobKind::SaveConnection
        ));
        assert!(panel.queued_commands.is_empty());
    }

    #[test]
    fn r2_002_cancel_removes_queued_test_for_closed_dialog() {
        let mut panel = DatabasePanelState::default();
        let owner = DatabaseJobOwner::Dialog(77);
        panel.queue_command(
            super::super::DatabaseCommand::Shutdown,
            pending_for(2, owner, DatabasePendingJobKind::TestConnection),
        );
        assert_eq!(panel.remove_queued_owner(owner).len(), 1);
        assert!(panel.pop_queued_command().is_none());
    }

    #[test]
    fn r2_003_cancelled_active_job_is_tombstoned_before_dialog_closes() {
        let mut panel = DatabasePanelState::default();
        let job_id = DatabaseJobId(41);
        let now = std::time::Instant::now();
        panel.mark_job_cancelled(job_id, now);
        let expires = panel
            .cancelled_job_ids
            .get(&job_id)
            .copied()
            .expect("active cancellation must create a tombstone");
        assert!(expires > now);
        panel.prune_cancelled_jobs(expires);
        assert!(!panel.cancelled_job_ids.contains_key(&job_id));
    }

    #[test]
    fn r2_004_repeated_save_reuses_one_reserved_connection_id() {
        let mut panel = DatabasePanelState::default();
        let reserved = panel.allocate_connection_id();
        let mut dialog = DatabaseConnectionDialog::new(DatabaseConnectionColor::Blue);
        dialog.reserved_connection_id = Some(reserved);
        panel.dialog = Some(dialog);
        assert_eq!(
            panel.dialog.as_ref().unwrap().reserved_connection_id,
            Some(reserved)
        );
        assert_ne!(panel.allocate_connection_id(), reserved);
        let methods = include_str!("database_app_methods.rs");
        assert!(methods.contains(
            "dialog.reserved_connection_id = dialog.editing_connection_id.or(allocated_id)"
        ));
    }

    #[test]
    fn r2_005_repeated_test_connection_is_coalesced() {
        let mut panel = DatabasePanelState::default();
        let owner = DatabaseJobOwner::Dialog(9);
        assert_eq!(
            panel.queue_command(
                super::super::DatabaseCommand::Shutdown,
                pending_for(1, owner, DatabasePendingJobKind::TestConnection),
            ),
            DatabaseQueueResult::Queued
        );
        assert_eq!(
            panel.queue_command(
                super::super::DatabaseCommand::Shutdown,
                pending_for(2, owner, DatabasePendingJobKind::TestConnection),
            ),
            DatabaseQueueResult::Queued
        );
        assert_eq!(panel.queued_commands.len(), 2);
        assert_eq!(panel.queued_commands[0].1.id, DatabaseJobId(1));
        assert_eq!(panel.queued_commands[1].1.id, DatabaseJobId(2));
    }

    #[test]
    fn r2_006_save_secrets_remain_pending_until_success() {
        let methods = include_str!("database_app_methods.rs");
        assert!(methods.contains("pending_session_secrets"));
        assert!(!methods.contains("session_secrets.insert(connection_id, session_secrets)"));
        let events = include_str!("database_app_event_methods.rs");
        assert!(events.contains("pending_session_secrets"));
        assert!(events.contains("session_secrets.insert(connection.id, secrets)"));
    }

    #[test]
    fn r2_007_database_queue_has_a_hard_limit() {
        let mut panel = DatabasePanelState::default();
        for idx in 0..MAX_QUEUED_DATABASE_COMMANDS {
            let result = panel.queue_command(
                super::super::DatabaseCommand::Shutdown,
                pending_for(
                    idx as u64 + 1,
                    DatabaseJobOwner::Connection(DatabaseConnectionId(idx as u64 + 1)),
                    DatabasePendingJobKind::LoadDatabases,
                ),
            );
            assert_eq!(result, DatabaseQueueResult::Queued);
        }
        assert_eq!(
            panel.queue_command(
                super::super::DatabaseCommand::Shutdown,
                pending_for(
                    999,
                    DatabaseJobOwner::Connection(DatabaseConnectionId(999)),
                    DatabasePendingJobKind::LoadDatabases,
                ),
            ),
            DatabaseQueueResult::Full
        );
        assert_eq!(panel.queued_commands.len(), MAX_QUEUED_DATABASE_COMMANDS);
    }

    #[test]
    fn r2_008_closing_tab_removes_all_jobs_owned_by_that_tab() {
        let mut panel = DatabasePanelState::default();
        let owner = DatabaseJobOwner::Table(super::super::DatabaseTabId(55));
        panel.queue_command(
            super::super::DatabaseCommand::Shutdown,
            pending_for(1, owner, DatabasePendingJobKind::LoadMetadata),
        );
        panel.queue_command(
            super::super::DatabaseCommand::Shutdown,
            pending_for(2, owner, DatabasePendingJobKind::LoadChunk),
        );
        assert_eq!(panel.remove_queued_owner(owner).len(), 2);
        assert!(panel.queued_commands.is_empty());
    }

    #[test]
    fn r2_009_failed_host_key_retry_drains_next_queued_command() {
        let source = include_str!("database_app_event_methods.rs");
        assert!(source.contains("if !self.start_database_command_now(command, pending)"));
        assert!(source.contains("self.drain_database_command_queue();"));
    }

    #[test]
    fn r2_010_database_ids_skip_live_values_after_wraparound() {
        let mut panel = DatabasePanelState::default();
        panel.next_job_id = u64::MAX;
        panel
            .cancelled_job_ids
            .insert(DatabaseJobId(u64::MAX), std::time::Instant::now());
        panel
            .cancelled_job_ids
            .insert(DatabaseJobId(1), std::time::Instant::now());
        assert_eq!(panel.allocate_job_id(), DatabaseJobId(2));

        panel.next_connection_id = u64::MAX;
        panel
            .connections
            .push(DatabaseConnectionNode::new(config(u64::MAX)));
        panel
            .connections
            .push(DatabaseConnectionNode::new(config(1)));
        assert_eq!(panel.allocate_connection_id(), DatabaseConnectionId(2));

        panel.next_dialog_id = u64::MAX;
        let mut dialog = DatabaseConnectionDialog::new(DatabaseConnectionColor::Blue);
        dialog.session_id = u64::MAX;
        panel.dialog = Some(dialog);
        panel.queued_commands.push_back((
            super::super::DatabaseCommand::Shutdown,
            pending_for(
                44,
                DatabaseJobOwner::Dialog(1),
                DatabasePendingJobKind::TestConnection,
            ),
        ));
        assert_eq!(panel.allocate_dialog_id(), 2);
    }

    #[test]
    fn r3_106_invalid_persisted_database_state_surfaces_global_error() {
        let mut persisted = DatabasePersistedState::default();
        persisted.version = u32::MAX;
        let panel = DatabasePanelState::from_persisted(persisted);
        assert!(panel.global_error.as_deref().is_some_and(|error| {
            error.contains("Повреждено состояние Database Tools")
                && error.contains("newer RRiter version")
        }));
    }
