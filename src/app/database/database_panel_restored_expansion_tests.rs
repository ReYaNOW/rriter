    fn panel_with_connection(id: u64, expanded: bool) -> DatabasePanelState {
        let mut panel = DatabasePanelState::default();
        let mut connection = DatabaseConnectionNode::new(config(id));
        connection.expanded = expanded;
        panel.connections.push(connection);
        panel
    }

    #[test]
    fn restored_expanded_connection_keeps_disclosure_state() {
        let mut persisted = DatabasePersistedState::default();
        persisted.connections.push(config(1));
        persisted.expanded_connections.push(DatabaseConnectionId(1));

        let panel = DatabasePanelState::from_persisted(persisted);

        assert!(panel.connections[0].expanded);
        assert_eq!(
            panel.connections[0].children_state(),
            DatabaseConnectionChildrenState::ExpandedUnloaded
        );
    }

    #[test]
    fn restored_collapsed_connection_stays_collapsed() {
        let mut persisted = DatabasePersistedState::default();
        persisted.connections.push(config(1));

        let panel = DatabasePanelState::from_persisted(persisted);

        assert_eq!(
            panel.connections[0].children_state(),
            DatabaseConnectionChildrenState::Collapsed
        );
    }

    #[test]
    fn stale_restored_expansion_id_is_removed_during_normalization() {
        let mut persisted = DatabasePersistedState::default();
        persisted.connections.push(config(1));
        persisted.expanded_connections.push(DatabaseConnectionId(99));

        let panel = DatabasePanelState::from_persisted(persisted);

        assert!(panel.persisted.expanded_connections.is_empty());
        assert!(!panel.connections[0].expanded);
    }

    #[test]
    fn restored_selection_does_not_expand_connection() {
        let mut persisted = DatabasePersistedState::default();
        persisted.connections.push(config(1));
        persisted.selected_connection = Some(DatabaseConnectionId(1));

        let panel = DatabasePanelState::from_persisted(persisted);

        assert_eq!(panel.selected_connection, Some(DatabaseConnectionId(1)));
        assert!(!panel.connections[0].expanded);
    }

    #[test]
    fn expanded_unloaded_connection_is_selected_for_catalog_load() {
        let mut panel = panel_with_connection(1, true);

        let loads = panel.begin_expanded_connection_catalog_loads();

        assert_eq!(loads, vec![DatabaseConnectionId(1)]);
    }

    #[test]
    fn catalog_reconcile_marks_connection_loading_before_dispatch() {
        let mut panel = panel_with_connection(1, true);

        panel.begin_expanded_connection_catalog_loads();

        let node = &panel.connections[0];
        assert!(node.loading);
        assert_eq!(node.status, DatabaseConnectionStatus::Connecting);
        assert_eq!(
            node.children_state(),
            DatabaseConnectionChildrenState::ExpandedLoading
        );
    }

    #[test]
    fn second_catalog_reconcile_does_not_duplicate_request() {
        let mut panel = panel_with_connection(1, true);

        assert_eq!(panel.begin_expanded_connection_catalog_loads().len(), 1);
        assert!(panel.begin_expanded_connection_catalog_loads().is_empty());
    }

    #[test]
    fn multiple_expanded_connections_each_start_once() {
        let mut panel = panel_with_connection(1, true);
        let mut second = DatabaseConnectionNode::new(config(2));
        second.expanded = true;
        panel.connections.push(second);

        assert_eq!(
            panel.begin_expanded_connection_catalog_loads(),
            vec![DatabaseConnectionId(1), DatabaseConnectionId(2)]
        );
        assert!(panel.begin_expanded_connection_catalog_loads().is_empty());
    }

    #[test]
    fn collapsed_connection_is_not_loaded_by_reconcile() {
        let mut panel = panel_with_connection(1, false);

        assert!(panel.begin_expanded_connection_catalog_loads().is_empty());
        assert!(!panel.connections[0].loading);
    }

    #[test]
    fn loaded_catalog_is_not_reloaded_by_reconcile() {
        let mut panel = panel_with_connection(1, true);
        panel.connections[0].databases_loaded = true;
        panel.connections[0]
            .databases
            .push(DatabaseDatabaseNode::new("postgres".to_string()));

        assert!(panel.begin_expanded_connection_catalog_loads().is_empty());
    }

    #[test]
    fn loaded_empty_catalog_is_not_confused_with_unloaded() {
        let mut panel = panel_with_connection(1, true);
        panel.connections[0].databases_loaded = true;

        assert_eq!(
            panel.connections[0].children_state(),
            DatabaseConnectionChildrenState::ExpandedEmpty
        );
        assert!(panel.begin_expanded_connection_catalog_loads().is_empty());
    }

    #[test]
    fn failed_catalog_is_not_retried_on_panel_reopen() {
        let mut panel = panel_with_connection(1, true);
        panel.connections[0].status = DatabaseConnectionStatus::Error;
        panel.connections[0].status_message = Some("denied".to_string());

        assert_eq!(
            panel.connections[0].children_state(),
            DatabaseConnectionChildrenState::ExpandedError
        );
        assert!(panel.begin_expanded_connection_catalog_loads().is_empty());
    }

    #[test]
    fn cancelled_catalog_load_is_not_automatically_retried() {
        let mut panel = panel_with_connection(1, true);
        panel.connections[0].catalog_load_attempted = true;
        panel.connections[0].status = DatabaseConnectionStatus::Disconnected;
        panel.connections[0].status_message = None;

        assert_eq!(
            panel.connections[0].children_state(),
            DatabaseConnectionChildrenState::ExpandedError
        );
        assert!(panel.begin_expanded_connection_catalog_loads().is_empty());
    }

    #[test]
    fn one_toggle_expands_closed_connection_and_requests_load() {
        let mut panel = panel_with_connection(1, false);

        assert!(panel.toggle_connection_expansion(DatabaseConnectionId(1)));
        assert!(panel.connections[0].expanded);
    }

    #[test]
    fn first_toggle_of_restored_expanded_connection_only_collapses() {
        let mut panel = panel_with_connection(1, true);

        assert!(!panel.toggle_connection_expansion(DatabaseConnectionId(1)));
        assert!(!panel.connections[0].expanded);
    }

    #[test]
    fn toggling_connection_already_loading_never_requests_duplicate_load() {
        let mut panel = panel_with_connection(1, true);
        panel.connections[0].loading = true;

        assert!(!panel.toggle_connection_expansion(DatabaseConnectionId(1)));
        assert!(!panel.connections[0].expanded);
        assert!(!panel.toggle_connection_expansion(DatabaseConnectionId(1)));
        assert!(panel.connections[0].expanded);
    }

    #[test]
    fn missing_connection_toggle_is_a_safe_noop() {
        let mut panel = DatabasePanelState::default();

        assert!(!panel.toggle_connection_expansion(DatabaseConnectionId(7)));
    }

    #[test]
    fn connection_children_state_reports_loading() {
        let mut node = DatabaseConnectionNode::new(config(1));
        node.expanded = true;
        node.loading = true;

        assert_eq!(
            node.children_state(),
            DatabaseConnectionChildrenState::ExpandedLoading
        );
    }

    #[test]
    fn connection_children_state_reports_loaded_children() {
        let mut node = DatabaseConnectionNode::new(config(1));
        node.expanded = true;
        node.databases_loaded = true;
        node.databases
            .push(DatabaseDatabaseNode::new("postgres".to_string()));

        assert_eq!(
            node.children_state(),
            DatabaseConnectionChildrenState::ExpandedLoaded
        );
    }

    #[test]
    fn loading_hint_row_is_counted_for_expanded_connection() {
        let mut panel = panel_with_connection(1, true);
        panel.connections[0].loading = true;

        assert_eq!(panel.visible_tree_row_count(), 2);
    }

    #[test]
    fn empty_catalog_hint_row_is_counted_for_expanded_connection() {
        let mut panel = panel_with_connection(1, true);
        panel.connections[0].databases_loaded = true;

        assert_eq!(panel.visible_tree_row_count(), 2);
    }

    #[test]
    fn error_hint_row_is_counted_for_expanded_connection() {
        let mut panel = panel_with_connection(1, true);
        panel.connections[0].status = DatabaseConnectionStatus::Error;
        panel.connections[0].status_message = Some("denied".to_string());

        assert_eq!(panel.visible_tree_row_count(), 2);
    }

    #[test]
    fn unloaded_fallback_row_prevents_blank_expanded_tree() {
        let panel = panel_with_connection(1, true);

        assert_eq!(panel.visible_tree_row_count(), 2);
    }

    #[test]
    fn loading_refresh_with_cached_children_does_not_hide_catalog_rows() {
        let mut panel = panel_with_connection(1, true);
        panel.connections[0].loading = true;
        panel.connections[0].databases_loaded = true;
        panel.connections[0]
            .databases
            .push(DatabaseDatabaseNode::new("postgres".to_string()));

        assert_eq!(panel.visible_tree_row_count(), 2);
    }

    #[test]
    fn restart_lifecycle_loads_restored_connection_without_toggle() {
        let mut persisted = DatabasePersistedState::default();
        persisted.connections.push(config(1));
        persisted.expanded_connections.push(DatabaseConnectionId(1));
        let mut panel = DatabasePanelState::from_persisted(persisted);

        assert_eq!(
            panel.begin_expanded_connection_catalog_loads(),
            vec![DatabaseConnectionId(1)]
        );
        panel.connections[0].loading = false;
        panel.connections[0].databases_loaded = true;
        panel.connections[0]
            .databases
            .push(DatabaseDatabaseNode::new("postgres".to_string()));

        assert_eq!(
            panel.connections[0].children_state(),
            DatabaseConnectionChildrenState::ExpandedLoaded
        );
        assert!(panel.begin_expanded_connection_catalog_loads().is_empty());
    }

    #[test]
    fn deleting_connection_removes_it_from_persisted_expansion_on_sync() {
        let mut panel = panel_with_connection(1, true);
        panel.sync_persisted_connections();
        assert_eq!(
            panel.persisted.expanded_connections,
            vec![DatabaseConnectionId(1)]
        );

        panel.connections.clear();
        panel.sync_persisted_connections();

        assert!(panel.persisted.expanded_connections.is_empty());
    }

    #[test]
    fn connection_children_state_is_deterministic_and_side_effect_free() {
        let panel = panel_with_connection(1, true);
        let before = panel.connections[0].clone();

        let first = panel.connections[0].children_state();
        let second = panel.connections[0].children_state();

        assert_eq!(first, second);
        assert_eq!(panel.connections[0], before);
    }
